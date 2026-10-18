//! The creator's figure: the drafted delver's body in the kit they start with
//! ([`starting_kit`]), turning slowly beside the form. As the inventory's doll is
//! ([`crate::inventory::doll`]), it stands on a stage far below everything on its own render
//! layer, lit by the Armory's key and fill, and drawn by its own camera into an image the
//! creator's page shows.
//!
//! Dressing a body takes most of a second, so it runs off the main thread ([`Fitting`]); the
//! figure on show stays until the new one is ready. Only the folk, the look and the dirt change
//! the figure: class and name leave it be.

use std::sync::Arc;

use bevy::{
    app::Propagate,
    asset::RenderAssetUsages,
    camera::{Exposure, RenderTarget, ScalingMode, visibility::RenderLayers},
    light::NotShadowCaster,
    prelude::*,
    render::{render_resource::TextureFormat, view::Tonemapping},
};
use fb_items::Slot;

use super::creator::Draft;
use crate::figure::{Figure, spawn_figure};
use crate::fitting::{Fitting, figure_key};
use crate::inventory::doll::DOLL_LAYER;
use crate::inventory::starting_kit;
use crate::launch::AppState;
use crate::light::{StudioLight, point_light, reach_falloff};
use crate::render::ItemSpawner;

/// The figure's frame on the page, in logical pixels, and its image's pixels per logical pixel.
pub const PREVIEW: Vec2 = Vec2::new(300.0, 440.0);
const PIXELS_PER_POINT: f32 = 2.0;
/// Where the figure stands: far below the level and away from the doll's and the icons'
/// stages, though no level is about while the title shows.
const STAGE: Vec3 = Vec3::new(0.0, -50_000.0, 30_000.0);
/// Radians the figure turns each second (our own tuning: once round in about 16 seconds).
const TURN_SPEED: f32 = 0.4;
/// The key's strength before the shader's quarter steps, as for the doll.
const KEY_LEVEL: f32 = 0.6;
/// Room round the figure, as a multiple of its height or its turning width.
const PADDING: f32 = 1.06;

/// What the figure was dressed for ([`figure_key`]).
type Dressed = String;

/// The figure's stage: its image, what it was dressed for, the figure and camera, and the job
/// dressing the next one.
#[derive(Resource)]
pub struct PreviewStage {
    pub image: Handle<Image>,
    dressed: Option<Dressed>,
    fitting: Option<Fitting<Dressed, Arc<Figure>>>,
    figure: Option<Entity>,
    camera: Option<Entity>,
    turn: f32,
}

impl FromWorld for PreviewStage {
    fn from_world(world: &mut World) -> Self {
        let size = (PREVIEW * PIXELS_PER_POINT).round().as_uvec2();
        let mut image = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
        image.asset_usage = RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD;
        let image = world.get_resource_mut::<Assets<Image>>().map_or_else(Handle::default, |mut images| images.add(image));
        PreviewStage { image, dressed: None, fitting: None, figure: None, camera: None, turn: 0.0 }
    }
}

impl PreviewStage {
    /// Forgets the figure and camera (despawned with the title, or by [`fit_preview`]).
    fn clear(&mut self) {
        *self = PreviewStage { image: self.image.clone(), dressed: None, fitting: None, figure: None, camera: None, turn: 0.0 };
    }
}

/// What the drafted delver wears in the figure: the starting kit, rolled from their body seed,
/// one piece to a slot.
pub fn preview_kit(draft: &Draft) -> Vec<(Slot, fb_items::Item)> {
    let mut kit: Vec<(Slot, fb_items::Item)> = Vec::new();
    for (slot, item) in starting_kit(|salt| draft.body_seed.wrapping_mul(0x2545_f491).wrapping_add(salt), draft.folk) {
        if !kit.iter().any(|(s, _)| *s == slot) {
            kit.push((slot, item));
        }
    }
    kit
}

pub struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PreviewStage>()
            .add_systems(OnExit(AppState::Title), |mut stage: ResMut<PreviewStage>| stage.clear())
            .add_systems(Update, (fit_preview, turn_preview).chain().run_if(in_state(AppState::Title)));
    }
}

/// Starts dressing the figure when the draft's body or kit changes, shows it when it is ready,
/// and takes it away when the creator closes.
fn fit_preview(mut commands: Commands, draft: Option<Res<Draft>>, mut stage: ResMut<PreviewStage>, mut spawner: ItemSpawner) {
    let Some(draft) = draft else {
        if stage.figure.is_some() || stage.camera.is_some() || stage.fitting.is_some() {
            for entity in [stage.figure, stage.camera].into_iter().flatten() {
                commands.entity(entity).despawn();
            }
            stage.clear();
        }
        return;
    };
    if draft.is_changed() {
        let spec = draft.character().spec();
        let dirt = draft.dirt;
        let kit = preview_kit(&draft);
        let key = figure_key(&spec, &kit, dirt);
        if stage.dressed.as_ref() == Some(&key) {
            stage.fitting = None;
        } else if stage.fitting.as_ref().is_none_or(|f| f.key() != &key) {
            stage.fitting = Some(Fitting::start(key, move || Arc::new(Figure::dress(&spec, &kit, dirt))));
        }
    }
    let Some(mut fitting) = stage.fitting.take() else { return };
    match fitting.poll() {
        Some(figure) => {
            stage.dressed = Some(fitting.key().clone());
            show_figure(&mut commands, &mut spawner, &mut stage, &figure);
        }
        None => stage.fitting = Some(fitting),
    }
}

/// Puts `figure` on the stage in place of the one there, and frames the camera on it.
fn show_figure(commands: &mut Commands, spawner: &mut ItemSpawner, stage: &mut PreviewStage, figure: &Figure) {
    if let Some(old) = stage.figure.take() {
        commands.entity(old).despawn();
    }
    let Some(bounds) = figure.bounds() else { return };
    let layer = RenderLayers::layer(DOLL_LAYER);
    let exposure = Exposure::INDOOR;
    let root = spawn_figure(commands, spawner, figure);
    commands.entity(root).insert((
        Name::new("Creator figure"),
        DespawnOnExit(AppState::Title),
        Transform::from_translation(STAGE).with_rotation(Quat::from_rotation_y(stage.turn)),
        Propagate(layer.clone()),
        Propagate(NotShadowCaster),
    ));
    stage.figure = Some(root);

    // The frame is taller than wide: fit the height, and the turning width if it is wider.
    let (min, max) = bounds;
    let reach = min.xz().abs().max(max.xz().abs()).length();
    let aspect = PREVIEW.x / PREVIEW.y;
    let height = (max.y - min.y).max(2.0 * reach / aspect) * PADDING;
    let width = height * aspect;
    let looking_at = STAGE + Vec3::Y * (min.y + max.y) * 0.5;
    let depth = 4.0 * height;
    let projection = Projection::Orthographic(OrthographicProjection {
        scaling_mode: ScalingMode::Fixed { width, height },
        near: 0.1,
        far: 2.0 * depth,
        ..OrthographicProjection::default_3d()
    });
    let at = Transform::from_translation(looking_at + Vec3::Z * depth).looking_at(looking_at, Vec3::Y);
    match stage.camera {
        Some(camera) => {
            commands.entity(camera).insert((projection, at));
        }
        None => {
            // Far off along the Armory key's direction, so its falloff is nearly flat over the
            // figure, as for the doll.
            let distance = 12.0 * height;
            let range = 8.0 * distance;
            let key = point_light(Color::WHITE, KEY_LEVEL / reach_falloff(distance, range), range, &exposure);
            let camera = commands
                .spawn((
                    Name::new("Creator figure camera"),
                    crate::render::ScreenInkExempt,
                    DespawnOnExit(AppState::Title),
                    Camera3d::default(),
                    Camera { order: -3, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
                    RenderTarget::Image(stage.image.clone().into()),
                    projection,
                    exposure,
                    Tonemapping::Linear,
                    AmbientLight { color: StudioLight::ARMORY.ambient_color, brightness: StudioLight::ARMORY.ambient / exposure.exposure(), ..default() },
                    layer.clone(),
                    at,
                ))
                .with_child((Name::new("Creator key light"), key, Transform::from_translation(StudioLight::ARMORY.key_from() * distance), layer))
                .id();
            stage.camera = Some(camera);
        }
    }
}

/// Turns the figure slowly about its own axis.
fn turn_preview(time: Res<Time<Real>>, mut stage: ResMut<PreviewStage>, mut transforms: Query<&mut Transform>) {
    let Some(figure) = stage.figure else { return };
    stage.turn = (stage.turn + time.delta_secs() * TURN_SPEED).rem_euclid(std::f32::consts::TAU);
    if let Ok(mut transform) = transforms.get_mut(figure) {
        transform.rotation = Quat::from_rotation_y(stage.turn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_figure_wears_the_starting_kit_one_piece_to_a_slot() {
        let draft = Draft::roll(12);
        let kit = preview_kit(&draft);
        for slot in [Slot::Belt, Slot::Offhand] {
            assert_eq!(kit.iter().filter(|(s, _)| *s == slot).count(), 1, "{slot:?}");
        }
        assert!(kit.iter().any(|(_, item)| item.base == "Torch"));
        assert_eq!(kit, preview_kit(&draft), "the same draft dresses the same");
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
