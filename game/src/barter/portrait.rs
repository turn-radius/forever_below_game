//! The trader's portrait on the barter screen: their head, drawn by the same camera the delver's
//! header portrait uses ([`spawn_portrait_camera`]) from the same figure the world draws them
//! with ([`Figure::dress`] on their [`Person`]'s body, gear and dirt), so it is the same folk,
//! colouring and worn headgear. The figure stands on a stage of its own far below the level
//! ([`TRADER_STAGE`], [`TRADER_LAYER`]); the camera and its texture exist only while the
//! barter does.

use std::sync::Arc;

use bevy::{app::Propagate, camera::visibility::RenderLayers, light::NotShadowCaster, prelude::*};
use fb_body::BodySpec;
use fb_items::{Item, Slot};

use super::Barter;
use crate::figure::{Figure, spawn_figure};
use crate::fitting::{FitCache, Fitting, figure_key};
use crate::folk::Person;
use crate::inventory::doll::{doll_view, portrait_target, spawn_portrait_camera, studio_key};
use crate::launch::AppState;
use crate::play::{Screen, ScreenOpen};
use crate::render::ItemSpawner;

/// Render layer the trader's figure and its camera are on (the icons are on 2, the doll on 3
/// and the commission's item on 4).
pub const TRADER_LAYER: usize = 5;
/// Where the trader stands: far below the level and away from the other stages.
pub const TRADER_STAGE: Vec3 = Vec3::new(0.0, -50_000.0, -60_000.0);

/// What the trader looks like: what the world dresses their figure from.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub spec: BodySpec,
    pub gear: Vec<(Slot, Item)>,
    pub dirt: f32,
}

impl Look {
    /// The look of `person`, as [`crate::folk`] builds their figure.
    pub fn of(person: &Person) -> Look {
        Look {
            spec: person.spec.clone(),
            gear: person.gear.clone(),
            dirt: person.dirt,
        }
    }

    /// The key of the figure this look dresses ([`figure_key`]).
    pub fn key(&self) -> String {
        figure_key(&self.spec, &self.gear, self.dirt)
    }

    /// The figure the world draws for this look.
    pub fn dress(&self) -> Figure {
        Figure::dress(&self.spec, &self.gear, self.dirt)
    }
}

/// The trader's figure on its stage: what it was dressed for, the job dressing it, and the
/// figure's root with the portrait camera under it.
#[derive(Resource, Default)]
pub struct TraderStage {
    dressed: Option<String>,
    fitting: Option<Fitting<String, Arc<Figure>>>,
    root: Option<Entity>,
    camera: Option<Entity>,
}

/// Gives a barter its portrait texture and dresses the trader's figure for it.
pub fn stage_trader(
    mut commands: Commands,
    mut barter: ResMut<Barter>,
    mut images: ResMut<Assets<Image>>,
    mut stage: ResMut<TraderStage>,
    mut cache: ResMut<FitCache>,
    mut spawner: ItemSpawner,
) {
    let Some(session) = barter.session.as_ref() else {
        return;
    };
    let key = session.look.key();
    if stage.dressed.as_ref().is_some_and(|d| d != &key) {
        // Another trader: the old face goes at once, and nothing is drawn till the new one is fitted.
        if let Some(old) = stage.root.take() {
            commands.entity(old).despawn();
        }
        stage.camera = None;
        stage.dressed = None;
    }
    if stage.dressed.as_ref() != Some(&key)
        && stage.fitting.as_ref().is_none_or(|f| f.key() != &key)
    {
        stage.fitting = Some(match cache.figure(&key) {
            Some(figure) => Fitting::ready(key.clone(), figure),
            None => {
                let look = session.look.clone();
                Fitting::start(key.clone(), move || Arc::new(look.dress()))
            }
        });
    }
    if barter.portrait.is_none() {
        barter.portrait = Some(images.add(portrait_target()));
    }
    let Some(mut fitting) = stage.fitting.take() else {
        return;
    };
    let Some(figure) = fitting.poll() else {
        stage.fitting = Some(fitting);
        return;
    };
    cache.store_figure(fitting.key(), figure.clone());
    let Some(image) = barter.portrait.clone() else {
        return;
    };
    if let Some(old) = stage.root.take() {
        commands.entity(old).despawn();
    }
    let layer = RenderLayers::layer(TRADER_LAYER);
    let root = spawn_figure(&mut commands, &mut spawner, &figure);
    commands.entity(root).insert((
        Name::new("Trader"),
        DespawnOnExit(AppState::Delving),
        Transform::from_translation(TRADER_STAGE),
        Propagate(layer.clone()),
        Propagate(NotShadowCaster),
    ));
    // Lit as the doll is, so the portraits match.
    if let Some(bounds) = figure.bounds() {
        let key = studio_key(doll_view(bounds).0);
        commands.spawn((
            Name::new("Trader key light"),
            key.0,
            key.1,
            layer.clone(),
            ChildOf(root),
        ));
    }
    stage.camera = Some(spawn_portrait_camera(
        &mut commands,
        image,
        &figure,
        layer,
        root,
    ));
    stage.root = Some(root);
    stage.dressed = Some(fitting.key().clone());
}

/// Draws the portrait only while the barter's screen is open.
pub fn show_trader(
    open: Res<ScreenOpen>,
    barter: Res<Barter>,
    stage: Res<TraderStage>,
    mut cameras: Query<&mut Camera>,
) {
    let shown = barter.active() && open.is(Screen::Inventory);
    if let Some(mut camera) = stage.camera.and_then(|c| cameras.get_mut(c).ok())
        && camera.is_active != shown
    {
        camera.is_active = shown;
    }
}

/// When the barter is over, takes the trader and its camera down and lets go of the texture.
pub fn clear_trader(
    mut commands: Commands,
    mut barter: ResMut<Barter>,
    mut stage: ResMut<TraderStage>,
) {
    if barter.active() {
        return;
    }
    if barter.portrait.is_some() {
        barter.portrait = None;
    }
    if let Some(root) = stage.root.take() {
        commands.entity(root).despawn();
    }
    stage.camera = None;
    stage.dressed = None;
    stage.fitting = None;
}

impl TraderStage {
    /// Whether a figure is standing for the barter.
    pub fn is_dressed(&self) -> bool {
        self.dressed.is_some()
    }

    #[cfg(test)]
    pub fn mark_dressed_for_test(&mut self, key: &str) {
        self.dressed = Some(key.to_string());
    }
}
