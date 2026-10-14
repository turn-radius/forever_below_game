//! Item icons for the grids and cards: each item drawn once from its Armory mesh in the house
//! toon style, into an image the shape of its footprint, and kept.
//!
//! An icon is rendered by its own orthographic camera into an image (Bevy's render to texture,
//! as in its `render_to_texture` example), on [`ICON_LAYER`], at a stage far below the level
//! so no light of the level reaches it (Bevy 0.20 clusters point lights without regard to
//! render layers; see `VIEW_MODEL_STAGE` in `crate::carry`). Each stage has its own light: a
//! point light far off in the Armory key's direction, so it falls nearly evenly across the
//! item, and the Armory's ambient fill on the camera. Directional lights reach every view the
//! toon shader draws, so the icon cannot use one.
//!
//! A camera renders for [`RENDER_SECONDS`] (time for its pipelines to compile the first time)
//! and is then taken down; the image keeps its last frame. Icons are keyed by base, seed and
//! whether the item lies turned, since an item is a pure function of its base and seed.

use std::collections::{HashMap, VecDeque};

use bevy::{
    app::Propagate,
    asset::RenderAssetUsages,
    camera::{Exposure, RenderTarget, ScalingMode, visibility::RenderLayers},
    ecs::system::SystemParam,
    light::NotShadowCaster,
    prelude::*,
    render::{render_resource::TextureFormat, view::Tonemapping},
};
use fb_items::Item;

use crate::carry::appearance_of;
use crate::light::{StudioLight, point_light, reach_falloff};
use crate::render::ItemSpawner;

use super::grid::oriented;

/// Render layer the icon cameras and the items they draw are on.
pub const ICON_LAYER: usize = 2;

/// Pixels per grid cell in an icon: twice the screen's cell, so icons stay sharp on a
/// high-density display and in the card.
pub const ICON_CELL_PX: u32 = 92;

/// Where icons are drawn: far below the level and the view model's stage.
const ICON_STAGE: Vec3 = Vec3::new(0.0, -50_000.0, 0.0);
/// How far apart stages are, further than any stage light reaches.
const STAGE_SPACING: f32 = 6_000.0;
/// How many icons are drawn at once.
const STAGES: usize = 6;

/// How long an icon's camera draws before it is taken down, and the fewest frames it draws.
pub const RENDER_SECONDS: f32 = 1.0;
const RENDER_FRAMES: u32 = 8;

/// The view's turn and tilt off square: the plate's item viewer looks from `(.32, .3, 1)`
/// (`viewerShow` in `reference/forever-below-plate.html`), about 0.31 rad round and 0.29 rad
/// down.
const VIEW_YAW: f32 = 0.31;
const VIEW_PITCH: f32 = 0.29;
/// Room left round the item in its image, as a multiple of its size.
const PADDING: f32 = 1.12;
/// The key's strength on the item before the shader's quarter steps: 0.6 steps up to 0.75,
/// close to the Armory key's 0.78, and stays in that step across the item.
const KEY_LEVEL: f32 = 0.6;

/// Which icon: an item by its base and seed, lying upright or turned.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IconKey {
    pub base: String,
    pub seed: u32,
    pub turned: bool,
}

impl IconKey {
    pub fn of(item: &Item, turned: bool) -> Self {
        IconKey { base: item.base.clone(), seed: item.seed, turned }
    }
}

/// Icons made or being made.
#[derive(Resource, Default)]
pub struct IconCache {
    images: HashMap<IconKey, Handle<Image>>,
    queued: VecDeque<(IconKey, Item, Handle<Image>)>,
    drawing: Vec<Drawing>,
}

struct Drawing {
    stage: usize,
    entities: [Entity; 3],
    started: f32,
    frames: u32,
}

impl IconCache {
    /// Whether nothing is waiting to be drawn or being drawn.
    pub fn is_idle(&self) -> bool {
        self.queued.is_empty() && self.drawing.is_empty()
    }
}

/// The size in pixels of the icon for `item` lying `turned`.
pub fn icon_size(item: &Item, turned: bool) -> UVec2 {
    let size = oriented(item.profile.footprint, turned);
    UVec2::new(size.cols as u32, size.rows as u32) * ICON_CELL_PX
}

/// Hands out icons, queueing any not made yet.
#[derive(SystemParam)]
pub struct Icons<'w> {
    cache: ResMut<'w, IconCache>,
    images: ResMut<'w, Assets<Image>>,
}

impl Icons<'_> {
    /// The icon for `item` lying `turned`. A new one is blank until it has been drawn.
    pub fn of(&mut self, item: &Item, turned: bool) -> Handle<Image> {
        let key = IconKey::of(item, turned);
        if let Some(image) = self.cache.images.get(&key) {
            return image.clone();
        }
        let size = icon_size(item, turned);
        let mut target = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
        target.asset_usage = RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD;
        let image = self.images.add(target);
        self.cache.images.insert(key.clone(), image.clone());
        self.cache.queued.push_back((key, item.clone(), image.clone()));
        image
    }
}

/// How an item is posed for its icon: turned so its longest side runs up the image and its
/// next longest across (as its footprint's rows and columns do), a quarter turn more if it
/// lies `turned`, then turned and tilted a little toward the viewer. Returns the rotation and
/// the posed bounds. `None` for an item with no vertices.
pub fn icon_pose(points: &[Vec3], turned: bool) -> Option<(Quat, Vec3, Vec3)> {
    let (min, max) = bounds(points.iter().copied())?;
    let size = max - min;
    let mut axes = [0, 1, 2];
    axes.sort_by(|&a, &b| size[b].total_cmp(&size[a]));
    // Columns of the matrix taking item space to the posed view: longest to +Y, next to +X.
    let mut columns = [Vec3::ZERO; 3];
    columns[axes[0]] = Vec3::Y;
    columns[axes[1]] = Vec3::X;
    columns[axes[2]] = Vec3::Z;
    if Mat3::from_cols(columns[0], columns[1], columns[2]).determinant() < 0.0 {
        columns[axes[2]] = Vec3::NEG_Z;
    }
    let align = Quat::from_mat3(&Mat3::from_cols(columns[0], columns[1], columns[2]));
    let turn = if turned { Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2) } else { Quat::IDENTITY };
    let view = Quat::from_rotation_x(VIEW_PITCH) * Quat::from_rotation_y(-VIEW_YAW);
    let rotation = (view * turn * align).normalize();
    let (min, max) = bounds(points.iter().map(|&p| rotation * p))?;
    Some((rotation, min, max))
}

/// The orthographic view, width and height in world units, that holds posed bounds `min..max`
/// in an image of `aspect` (width over height), with [`PADDING`].
pub fn icon_view(min: Vec3, max: Vec3, aspect: f32) -> Vec2 {
    let extent = (max - min).truncate().max(Vec2::splat(0.01));
    let height = extent.y.max(extent.x / aspect) * PADDING;
    Vec2::new(height * aspect, height)
}

fn bounds(points: impl Iterator<Item = Vec3>) -> Option<(Vec3, Vec3)> {
    points.fold(None, |acc, p| Some(acc.map_or((p, p), |(lo, hi): (Vec3, Vec3)| (lo.min(p), hi.max(p)))))
}

/// Starts drawing queued icons on free stages and takes down cameras that are done.
pub fn draw_icons(mut commands: Commands, mut cache: ResMut<IconCache>, mut spawner: ItemSpawner, time: Res<Time<Real>>) {
    let now = time.elapsed_secs();
    cache.drawing.retain_mut(|drawing| {
        drawing.frames += 1;
        let done = drawing.frames >= RENDER_FRAMES && now - drawing.started >= RENDER_SECONDS;
        if done {
            for entity in drawing.entities {
                commands.entity(entity).despawn();
            }
        }
        !done
    });
    while cache.drawing.len() < STAGES {
        let Some((key, item, image)) = cache.queued.pop_front() else { break };
        let stage = (0..STAGES).find(|s| cache.drawing.iter().all(|d| d.stage != *s)).expect("a free stage");
        if let Some(entities) = stage_icon(&mut commands, &mut spawner, &item, key.turned, image, stage) {
            cache.drawing.push(Drawing { stage, entities, started: now, frames: 0 });
        }
    }
}

/// Spawns `item` posed on `stage` with a camera drawing it into `image` and its light.
/// Returns the camera, the item and the light.
fn stage_icon(commands: &mut Commands, spawner: &mut ItemSpawner, item: &Item, turned: bool, image: Handle<Image>, stage: usize) -> Option<[Entity; 3]> {
    let mesh = item.shape.build();
    let points: Vec<Vec3> = mesh.parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p)).collect();
    let (rotation, min, max) = icon_pose(&points, turned)?;
    let size = icon_size(item, turned);
    let view = icon_view(min, max, size.x as f32 / size.y as f32);
    let centre = (min + max) * 0.5;
    let at = ICON_STAGE + Vec3::X * stage as f32 * STAGE_SPACING;
    let layer = RenderLayers::layer(ICON_LAYER);
    let exposure = Exposure::INDOOR;

    let mut root = spawner.spawn(&mesh, &appearance_of(&item.shape));
    root.insert((
        Name::new(format!("Icon: {}", item.name)),
        Transform::from_translation(at - centre).with_rotation(rotation),
        Propagate(layer.clone()),
        Propagate(NotShadowCaster),
    ));
    let root = root.id();

    let depth = (max.z - min.z).max(1.0);
    let camera = commands
        .spawn((
            Name::new("Icon camera"),
            Camera3d::default(),
            Camera { order: -10 - stage as isize, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(image.into()),
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed { width: view.x, height: view.y },
                near: 0.1,
                far: depth + 20.0,
                ..OrthographicProjection::default_3d()
            }),
            exposure,
            Tonemapping::Linear,
            AmbientLight {
                color: StudioLight::ARMORY.ambient_color,
                brightness: StudioLight::ARMORY.ambient / exposure.exposure(),
                ..default()
            },
            layer.clone(),
            Transform::from_translation(at + Vec3::Z * (depth * 0.5 + 10.0)),
        ))
        .id();

    // Far off along the Armory key's direction, so its falloff is nearly flat over the item.
    let reach = (max - min).max_element().clamp(1.0, 30.0);
    let distance = 12.0 * reach;
    let range = 8.0 * distance;
    let peak = KEY_LEVEL / reach_falloff(distance, range);
    let light = commands
        .spawn((
            Name::new("Icon light"),
            point_light(Color::WHITE, peak, range, &exposure),
            Transform::from_translation(at + StudioLight::ARMORY.key_from() * distance),
            layer,
        ))
        .id();
    Some([camera, root, light])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_points(size: Vec3) -> Vec<Vec3> {
        (0..8).map(|i| Vec3::new(if i & 1 == 0 { 0.0 } else { size.x }, if i & 2 == 0 { 0.0 } else { size.y }, if i & 4 == 0 { 0.0 } else { size.z })).collect()
    }

    #[test]
    fn the_longest_side_runs_up_the_icon() {
        // Long along X, wide along Z, thin along Y: a sword lying down.
        let (_, min, max) = icon_pose(&box_points(Vec3::new(9.0, 0.3, 2.0)), false).unwrap();
        let posed = max - min;
        assert!(posed.y > posed.x * 2.0, "{posed}");
        let (_, min, max) = icon_pose(&box_points(Vec3::new(9.0, 0.3, 2.0)), true).unwrap();
        let turned = max - min;
        assert!(turned.x > turned.y * 2.0, "{turned}");
        assert!(icon_pose(&[], false).is_none());
    }

    #[test]
    fn the_view_holds_the_item_in_its_image() {
        let view = icon_view(Vec3::new(-1.0, -4.0, 0.0), Vec3::new(1.0, 4.0, 1.0), 0.5);
        assert!((view.x / view.y - 0.5).abs() < 1e-5);
        assert!(view.y >= 8.0 && view.x >= 2.0);
        // A wide thing in a tall image is held by its width.
        let wide = icon_view(Vec3::new(-5.0, -1.0, 0.0), Vec3::new(5.0, 1.0, 0.0), 0.5);
        assert!(wide.x >= 10.0, "{wide}");
    }

    #[test]
    fn icons_are_the_shape_of_their_footprint() {
        let pack = fb_items::roll_item("Pack", 7).unwrap();
        let fp = pack.profile.footprint;
        assert_eq!(icon_size(&pack, false), UVec2::new(fp.cols as u32, fp.rows as u32) * ICON_CELL_PX);
        assert_eq!(icon_size(&pack, true), UVec2::new(fp.rows as u32, fp.cols as u32) * ICON_CELL_PX);
    }
}
