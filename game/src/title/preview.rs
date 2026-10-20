//! The creator's paper doll: the inventory's doll leaf ([`doll_leaf`]), with the runic seal,
//! the drafted delver's body in the kit they start with ([`starting_kit`]) and that kit in its
//! slots, beside the form. As in the inventory, the body stands on a stage far below everything
//! on the doll's render layer, lit by the Armory's key and fill and by the torch they hold, and
//! drawn by its own camera into the doll's image ([`DollAssets::doll`]); it turns slowly, and
//! dragging across it turns it by hand.
//!
//! Dressing a body takes a few tenths of a second, so it runs off the main thread
//! ([`Fitting`]); the figure on show stays until the new one is ready. Only the folk and the
//! look change the figure: class and name leave it be.

use std::sync::Arc;

use bevy::{
    app::Propagate,
    camera::{Exposure, RenderTarget, ScalingMode, visibility::RenderLayers},
    input::mouse::AccumulatedMouseMotion,
    light::NotShadowCaster,
    prelude::*,
    render::view::Tonemapping,
    window::PrimaryWindow,
};
use fb_items::Slot;

use super::creator::{Draft, ShapeMoved};
use crate::figure::{Figure, spawn_figure};
use crate::fitting::{Fitting, figure_key};
use crate::inventory::doll::{DOLL_LAYER, DollView, HELD_LIGHT_PEAK, HELD_LIGHT_REACH, SlotView, TURN_PER_PIXEL, doll_view};
use crate::inventory::{DollAssets, Inventory, starting_kit};
use crate::launch::AppState;
use crate::light::{StudioLight, Torch, point_light, reach_falloff};
use crate::render::ItemSpawner;
use crate::ui::{node_rect, ui_cursor};

/// Where the figure stands: far below the level and away from the inventory doll's and the
/// icons' stages, though no level is about while the title shows.
const STAGE: Vec3 = Vec3::new(0.0, -50_000.0, 30_000.0);
/// Radians the figure turns each second when it is not dragged (our own tuning: once round in
/// about 25 seconds).
const TURN_SPEED: f32 = 0.25;
/// The key's strength before the shader's quarter steps, as for the doll.
const KEY_LEVEL: f32 = 0.6;

/// What the figure was dressed for ([`figure_key`]).
type Dressed = String;

/// What the draft wants dressed next: its key, the body, the kit and the dirt.
type Wanted = (Dressed, fb_body::BodySpec, Vec<(Slot, fb_items::Item)>, f32);

/// A starting kit and the folk and body seed it was rolled for.
type Kit = ((fb_body::Folk, u32), Vec<(Slot, fb_items::Item)>);

/// The figure's stage: what it was dressed for, the figure and camera, the job dressing the
/// next one and what is wanted after it, and how far it is turned.
#[derive(Resource, Default)]
pub struct PreviewStage {
    dressed: Option<Dressed>,
    fitting: Option<Fitting<Dressed, Arc<Figure>>>,
    wanted: Option<Wanted>,
    /// The starting kit last rolled, and for which folk and body seed: a drag of a body slider
    /// refits the body in the same kit without rolling it again.
    kit: Option<Kit>,
    figure: Option<Entity>,
    camera: Option<Entity>,
    turn: f32,
}

/// What the drafted delver wears: the starting kit, rolled from their body seed, one piece to
/// a slot.
pub fn preview_kit(draft: &Draft) -> Vec<(Slot, fb_items::Item)> {
    let mut kit: Vec<(Slot, fb_items::Item)> = Vec::new();
    for (slot, item) in starting_kit(|salt| draft.body_seed.wrapping_mul(0x2545_f491).wrapping_add(salt), draft.folk) {
        if !kit.iter().any(|(s, _)| *s == slot) {
            kit.push((slot, item));
        }
    }
    kit
}

/// The drafted delver's starting kit worn in an inventory of its own, for the doll leaf's
/// slots.
pub fn preview_inventory(draft: &Draft) -> Inventory {
    let mut inventory = Inventory::default();
    for (slot, item) in preview_kit(draft) {
        let id = inventory.add(item);
        if inventory.wear(id, slot).is_err() {
            inventory.forget(id);
        }
    }
    inventory
}

pub struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PreviewStage>()
            .init_resource::<ShapeMoved>()
            .add_systems(OnExit(AppState::Title), |mut stage: ResMut<PreviewStage>| *stage = PreviewStage::default())
            .add_systems(Update, (fit_preview, turn_preview).chain().run_if(in_state(AppState::Title)));
    }
}

/// Starts dressing the figure when the draft's body or kit changes, shows it when it is ready,
/// and takes it away when the creator closes. While a slider is dragged the draft changes
/// every frame: the fit in flight is finished and shown, then the latest body is fitted, so
/// the doll follows the drag instead of waiting for it to stop. The camera comes with the creator, so the doll's
/// image is cleared of whatever the inventory last drew there before the first figure shows.
fn fit_preview(mut commands: Commands, draft: Option<Res<Draft>>, mut moved: ResMut<ShapeMoved>, assets: Res<DollAssets>, mut stage: ResMut<PreviewStage>, mut spawner: ItemSpawner) {
    let Some(draft) = draft else {
        if stage.figure.is_some() || stage.camera.is_some() || stage.fitting.is_some() {
            for entity in [stage.figure, stage.camera].into_iter().flatten() {
                commands.entity(entity).despawn();
            }
            *stage = PreviewStage::default();
        }
        return;
    };
    if stage.camera.is_none() {
        stage.camera = Some(spawn_camera(&mut commands, &assets));
    }
    if draft.is_changed() || std::mem::take(&mut moved.0) {
        let spec = draft.character().spec();
        let rolled_for = (draft.folk, draft.body_seed);
        if stage.kit.as_ref().is_none_or(|(k, _)| *k != rolled_for) {
            stage.kit = Some((rolled_for, preview_kit(&draft)));
        }
        let kit = stage.kit.as_ref().map(|(_, kit)| kit.clone()).unwrap_or_default();
        let key = figure_key(&spec, &kit, draft.dirt);
        stage.wanted = Some((key, spec, kit, draft.dirt));
    }
    if let Some(mut fitting) = stage.fitting.take() {
        match fitting.poll() {
            Some(figure) => {
                stage.dressed = Some(fitting.key().clone());
                show_figure(&mut commands, &mut spawner, &mut stage, &figure);
            }
            None => stage.fitting = Some(fitting),
        }
    }
    if stage.fitting.is_none()
        && let Some((key, spec, kit, dirt)) = stage.wanted.take()
        && stage.dressed.as_ref() != Some(&key)
    {
        stage.fitting = Some(Fitting::start(key, move || Arc::new(Figure::dress(&spec, &kit, dirt))));
    }
}

/// The camera drawing the stage into the doll's image, and its key light, far off along the
/// Armory key's direction so its falloff is nearly flat over the figure, as for the doll.
fn spawn_camera(commands: &mut Commands, assets: &DollAssets) -> Entity {
    let layer = RenderLayers::layer(DOLL_LAYER);
    let exposure = Exposure::INDOOR;
    // A human is about 17 units tall; the view is set to the figure once it shows.
    let side = 20.0;
    let distance = 12.0 * side;
    let range = 8.0 * distance;
    let key = point_light(Color::WHITE, KEY_LEVEL / reach_falloff(distance, range), range, &exposure);
    let (projection, at) = framing(side, side * 0.5);
    commands
        .spawn((
            Name::new("Creator doll camera"),
            crate::render::ScreenInkExempt,
            DespawnOnExit(AppState::Title),
            Camera3d::default(),
            Camera { order: -3, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(assets.doll.clone().into()),
            projection,
            exposure,
            Tonemapping::Linear,
            AmbientLight { color: StudioLight::ARMORY.ambient_color, brightness: StudioLight::ARMORY.ambient / exposure.exposure(), ..default() },
            layer.clone(),
            at,
        ))
        .with_child((Name::new("Creator doll key light"), key, Transform::from_translation(StudioLight::ARMORY.key_from() * distance), layer))
        .id()
}

/// The square orthographic view `side` across, looking at height `centre_y` on the stage.
fn framing(side: f32, centre_y: f32) -> (Projection, Transform) {
    let looking_at = STAGE + Vec3::Y * centre_y;
    let depth = 4.0 * side;
    let projection = Projection::Orthographic(OrthographicProjection {
        scaling_mode: ScalingMode::Fixed { width: side, height: side },
        near: 0.1,
        far: 2.0 * depth,
        ..OrthographicProjection::default_3d()
    });
    (projection, Transform::from_translation(looking_at + Vec3::Z * depth).looking_at(looking_at, Vec3::Y))
}

/// Puts `figure` on the stage in place of the one there, with the torch it holds lit, and
/// frames the camera on it as the inventory frames its doll.
fn show_figure(commands: &mut Commands, spawner: &mut ItemSpawner, stage: &mut PreviewStage, figure: &Figure) {
    if let Some(old) = stage.figure.take() {
        commands.entity(old).despawn();
    }
    let Some(bounds) = figure.bounds() else { return };
    let layer = RenderLayers::layer(DOLL_LAYER);
    let root = spawn_figure(commands, spawner, figure);
    commands.entity(root).insert((
        Name::new("Creator doll"),
        DespawnOnExit(AppState::Title),
        Transform::from_translation(STAGE).with_rotation(Quat::from_rotation_y(stage.turn)),
        Propagate(layer.clone()),
        Propagate(NotShadowCaster),
    ));
    if let Some(light) = figure.light {
        let colour = Color::srgb(light.anchor.color.0, light.anchor.color.1, light.anchor.color.2);
        let lamp = Torch::flickering(point_light(colour, HELD_LIGHT_PEAK, figure.body.height * HELD_LIGHT_REACH, &Exposure::INDOOR), light.anchor.flicker);
        commands.entity(root).with_child((lamp, Transform::from_translation(light.at), layer));
    }
    stage.figure = Some(root);
    if let Some(camera) = stage.camera {
        let (side, centre_y) = doll_view(bounds);
        commands.entity(camera).insert(framing(side, centre_y));
    }
}

/// Turns the figure: by hand while the mouse drags across the doll (not from a slot), slowly by
/// itself otherwise.
#[allow(clippy::too_many_arguments)]
fn turn_preview(
    time: Res<Time<Real>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    views: Query<(&ComputedNode, &UiGlobalTransform), With<DollView>>,
    slots: Query<(&ComputedNode, &UiGlobalTransform), With<SlotView>>,
    mut stage: ResMut<PreviewStage>,
    mut transforms: Query<&mut Transform>,
    mut turning: Local<bool>,
) {
    let Some(figure) = stage.figure else { return };
    if buttons.just_pressed(MouseButton::Left) {
        let cursor = window.and_then(|w| ui_cursor(&w, ui_scale.as_deref()));
        let on = |rect: Rect| cursor.is_some_and(|c| rect.contains(c));
        *turning = views.iter().any(|(n, t)| on(node_rect(n, t))) && !slots.iter().any(|(n, t)| on(node_rect(n, t)));
    }
    if !buttons.pressed(MouseButton::Left) {
        *turning = false;
    }
    let step = if *turning { motion.delta.x * TURN_PER_PIXEL } else { time.delta_secs() * TURN_SPEED };
    stage.turn = (stage.turn + step).rem_euclid(std::f32::consts::TAU);
    if let Ok(mut transform) = transforms.get_mut(figure) {
        transform.rotation = Quat::from_rotation_y(stage.turn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_doll_wears_the_starting_kit_one_piece_to_a_slot() {
        let draft = Draft::roll(12);
        let kit = preview_kit(&draft);
        for slot in [Slot::Belt, Slot::Offhand] {
            assert_eq!(kit.iter().filter(|(s, _)| *s == slot).count(), 1, "{slot:?}");
        }
        assert!(kit.iter().any(|(_, item)| item.base == "Torch"));
        assert_eq!(kit, preview_kit(&draft), "the same draft dresses the same");
        let inventory = preview_inventory(&draft);
        for (slot, item) in &kit {
            assert_eq!(inventory.worn(*slot).and_then(|id| inventory.item(id)).map(|i| &i.name), Some(&item.name), "{slot:?} is in its slot");
        }
    }

    #[test]
    fn only_folk_and_look_change_what_is_dressed() {
        let mut draft = Draft::roll(4);
        let key = |d: &Draft| figure_key(&d.character().spec(), &preview_kit(d), d.dirt);
        let first = key(&draft);
        draft.class = crate::character::Class::Mage;
        draft.roll_name();
        draft.roll_scores();
        assert_eq!(key(&draft), first);
        draft.roll_look();
        assert_ne!(key(&draft), first);
    }
}
