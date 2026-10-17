//! The lab's figure: the character's body with their gear fitted, drawn as `--doll` and the
//! inventory's doll draw it ([`Figure::dress`], the house toon look, the Armory's key and fill
//! light) by a camera of its own into an image the sheet shows at its top left.
//!
//! Like the inventory's doll it stands on a stage far from anything else, on its own render
//! layer. Fitting takes most of a second, so it runs off the main thread ([`Fitting`]) and the
//! figure drawn before stays up until the new one is ready; the sheet changing its scores or
//! its tooltips does not refit, only folk, body, dirt and gear do. Add [`LabFigurePlugin`] with
//! [`crate::render::ArmoryRenderPlugin`]; without it the sheet has an empty frame.

use std::hash::{Hash, Hasher};

use bevy::{
    app::{HierarchyPropagatePlugin, Propagate},
    asset::RenderAssetUsages,
    camera::{Exposure, RenderTarget, ScalingMode, visibility::RenderLayers},
    light::NotShadowCaster,
    prelude::*,
    render::{render_resource::TextureFormat, view::Tonemapping},
};
use fb_items::{Item, Slot};

use crate::figure::{Figure, spawn_figure};
use crate::fitting::Fitting;
use crate::inventory::doll::{DOLL_LAYER, DOLL_STAGE};
use crate::lab_view::LabState;
use crate::light::{StudioLight, Torch, point_light, reach_falloff};
use crate::render::ItemSpawner;

/// The figure's frame on the sheet, in logical pixels, and its image's pixels per logical one.
pub const FRAME: Vec2 = Vec2::new(120.0, 168.0);
const PIXELS_PER_POINT: f32 = 2.0;
/// Room round the figure in its frame, as a multiple of its height.
const PADDING: f32 = 1.05;
/// The key's strength on the figure before the shader's quarter steps, as for the inventory's doll.
const KEY_LEVEL: f32 = 0.6;
/// A held light's strength close up and its reach in multiples of the body's height.
const HELD_LIGHT_PEAK: f32 = 0.55;
const HELD_LIGHT_REACH: f32 = 0.9;

/// The image the figure is drawn into, once the plugin has made it.
#[derive(Resource, Default, Clone, Debug)]
pub struct LabFigure {
    pub image: Option<Handle<Image>>,
}

/// What the figure was last dressed for: a hash of the body and everything worn.
type Dressed = u64;

#[derive(Resource, Default)]
struct Stage {
    dressed: Option<Dressed>,
    figure: Option<Entity>,
    camera: Option<Entity>,
    fitting: Option<Fitting<Dressed, Figure>>,
}

pub struct LabFigurePlugin;

impl Plugin for LabFigurePlugin {
    fn build(&self, app: &mut App) {
        // The figure's layer and shadows are set on its root and passed down to every piece.
        if !app.is_plugin_added::<HierarchyPropagatePlugin<RenderLayers>>() {
            app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(Update));
        }
        if !app.is_plugin_added::<HierarchyPropagatePlugin<NotShadowCaster>>() {
            app.add_plugins(HierarchyPropagatePlugin::<NotShadowCaster>::new(Update));
        }
        app.init_resource::<LabFigure>()
            .init_resource::<Stage>()
            .insert_resource(StudioLight::ARMORY.ambient_light(&Exposure::INDOOR))
            .add_systems(PreStartup, make_image)
            .add_systems(Update, (start_fitting.run_if(resource_changed::<LabState>), show_figure).chain());
    }
}

fn make_image(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let size = (FRAME * PIXELS_PER_POINT).round().as_uvec2();
    let mut image = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
    image.asset_usage = RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD;
    commands.insert_resource(LabFigure { image: Some(images.add(image)) });
}

/// What the figure depends on: the body, and each worn thing as rolled and dressed.
fn dressing(state: &LabState) -> (Dressed, Vec<(Slot, Item)>) {
    let worn: Vec<(Slot, Item)> = state.items().into_iter().map(|(slot, rolled)| (slot, rolled.item)).collect();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let c = &state.character;
    (c.folk.key(), c.body_seed, c.dirt.to_bits()).hash(&mut hasher);
    for (slot, item) in &worn {
        (slot, &item.base, item.seed, &item.name, item.shape.lit()).hash(&mut hasher);
    }
    (hasher.finish(), worn)
}

/// Starts fitting the figure when the body or the gear has changed.
fn start_fitting(state: Res<LabState>, mut stage: ResMut<Stage>) {
    let (key, worn) = dressing(&state);
    if stage.dressed == Some(key) {
        stage.fitting = None;
        return;
    }
    if stage.fitting.as_ref().is_some_and(|f| *f.key() == key) {
        return;
    }
    let (spec, dirt) = (state.character.spec(), state.character.dirt);
    stage.fitting = Some(Fitting::start(key, move || Figure::dress(&spec, &worn, dirt)));
}

/// Puts the fitted figure on the stage and points the camera at it.
fn show_figure(mut commands: Commands, figure: Res<LabFigure>, mut stage: ResMut<Stage>, mut spawner: ItemSpawner) {
    let Some(mut fitting) = stage.fitting.take() else { return };
    let Some(dressed) = fitting.poll() else {
        stage.fitting = Some(fitting);
        return;
    };
    stage.dressed = Some(*fitting.key());
    if let Some(old) = stage.figure.take() {
        commands.entity(old).despawn();
    }
    let Some((min, max)) = dressed.bounds() else { return };
    let layer = RenderLayers::layer(DOLL_LAYER);
    let exposure = Exposure::INDOOR;
    let root = spawn_figure(&mut commands, &mut spawner, &dressed);
    commands.entity(root).insert((Name::new("Lab figure"), Transform::from_translation(DOLL_STAGE), Propagate(layer.clone()), Propagate(NotShadowCaster)));
    if let Some(light) = dressed.light {
        let colour = Color::srgb(light.anchor.color.0, light.anchor.color.1, light.anchor.color.2);
        let lamp = Torch::flickering(point_light(colour, HELD_LIGHT_PEAK, dressed.body.height * HELD_LIGHT_REACH, &exposure), light.anchor.flicker);
        commands.entity(root).with_child((lamp, Transform::from_translation(light.at), layer.clone()));
    }
    stage.figure = Some(root);

    // The frame is taller than wide: the figure fills its height, widened if its reach needs it.
    let height = (max.y - min.y) * PADDING;
    let reach = min.xz().abs().max(max.xz().abs()).x * 2.0 * PADDING;
    let width = (height * FRAME.x / FRAME.y).max(reach);
    let height = height.max(width * FRAME.y / FRAME.x);
    let looking_at = DOLL_STAGE + Vec3::Y * (min.y + max.y) * 0.5;
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
            let Some(image) = figure.image.clone() else { return };
            // Far off along the Armory key's direction, so its falloff is nearly flat over the figure.
            let distance = 12.0 * height;
            let range = 8.0 * distance;
            let key = point_light(Color::WHITE, KEY_LEVEL / reach_falloff(distance, range), range, &exposure);
            let camera = commands
                .spawn((
                    Name::new("Lab figure camera"),
                    Camera3d::default(),
                    Camera { order: -3, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
                    RenderTarget::Image(image.into()),
                    projection,
                    exposure,
                    Tonemapping::Linear,
                    AmbientLight { color: StudioLight::ARMORY.ambient_color, brightness: StudioLight::ARMORY.ambient / exposure.exposure(), ..default() },
                    layer.clone(),
                    at,
                ))
                .with_child((Name::new("Lab figure key light"), key, Transform::from_translation(StudioLight::ARMORY.key_from() * distance), layer))
                .id();
            stage.camera = Some(camera);
        }
    }
}
