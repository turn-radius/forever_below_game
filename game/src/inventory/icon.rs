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
//!
//! Every icon the screen asks for is drawn at once, each on a stage of its own (up to
//! [`STAGES`]), so a full pack and doll are all drawn within about a second of the screen
//! opening rather than a few at a time. Until an icon is drawn, [`Icons::spawn`] shows the
//! item's initial faintly in its place, and [`reveal_icons`] takes the initial away once the
//! icon is drawn. An icon counts as drawn as soon as its image, read back from the GPU each
//! frame (Bevy's `Readback`, as in its `gpu_readback` example), has any pixel that is not
//! clear, or when its camera is taken down, whichever is first.

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::{
    app::Propagate,
    asset::RenderAssetUsages,
    camera::{Exposure, RenderTarget, ScalingMode, visibility::RenderLayers},
    ecs::system::SystemParam,
    light::NotShadowCaster,
    prelude::*,
    render::{
        gpu_readback::{Readback, ReadbackComplete},
        render_resource::{TextureFormat, TextureUsages},
        view::Tonemapping,
    },
};
use armory::families::{Generator, banner::{Banner, BannerState}};
use armory::heraldry::Field;
use fb_items::Item;

use crate::launch::AppState;
use crate::carry::appearance_of;
use crate::light::{StudioLight, point_light, reach_falloff};
use crate::render::ItemSpawner;
use crate::ui::{MUTED, UiFonts};

use super::grid::oriented;

/// Render layer the icon cameras and the items they draw are on.
pub const ICON_LAYER: usize = 2;

/// Pixels per grid cell in an icon: twice the screen's 72-pixel cell at 1080 lines
/// (`[interface] cell_size`), so an icon is drawn at its full resolution on a display of twice
/// the density and is only ever shrunk, never enlarged, on one of ordinary density, in the
/// card and up to 2160 lines. Our own.
pub const ICON_CELL_PX: u32 = 144;

/// The size in pixels of a crest's image: tall as a shield is, for the largest the map or the
/// tab draws one and more.
pub const CREST_PX: UVec2 = UVec2::new(160, 208);

/// The size in pixels of a faction banner's image: tall as a hall banner hangs, rod included.
pub const BANNER_PX: UVec2 = UVec2::new(112, 280);

/// Where icons are drawn: far below the level and the view model's stage.
const ICON_STAGE: Vec3 = Vec3::new(0.0, -50_000.0, 0.0);
/// How far apart stages are, further than any stage light reaches.
const STAGE_SPACING: f32 = 6_000.0;
/// Stages per row of the square of stages; rows run along +Z, away from the doll's stage.
const STAGE_ROW: usize = 8;
/// How many icons are drawn at once: enough for a full screen of them.
pub const STAGES: usize = STAGE_ROW * STAGE_ROW;

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
    /// Whether a torch or lantern is lit, since its flame shows; `None` for anything else.
    pub lit: Option<bool>,
    /// Lines of history, which grow whenever an enchanter changes the item, so its icon is drawn
    /// again with its new rarity, etching and wear.
    pub worked: usize,
    /// How many arrows or bolts the icon draws: one, three for a few, five for a stack.
    pub pieces: u8,
}

/// How many arrows an icon draws for `item`: ammunition reads as a bundle, since one arrow in a
/// cell is a hairline; one piece draws one, a few three, a stack five. Everything else 1.
pub fn bundle_pieces(item: &Item) -> u8 {
    if !item.profile.tags.contains(&fb_items::Tag::Ammunition) {
        return 1;
    }
    match item.quantity {
        0..=1 => 1,
        2..=5 => 3,
        _ => 5,
    }
}

impl IconKey {
    pub fn of(item: &Item, turned: bool) -> Self {
        IconKey { base: item.base.clone(), seed: item.seed, turned, lit: item.shape.lit(), worked: item.history.len(), pieces: bundle_pieces(item) }
    }
}

/// Icons made or being made.
#[derive(Resource, Default)]
pub struct IconCache {
    images: HashMap<IconKey, Handle<Image>>,
    queued: VecDeque<(Subject, Handle<Image>)>,
    arms: HashMap<Arms, Handle<Image>>,
    drawing: Vec<Drawing>,
    drawn: HashSet<AssetId<Image>>,
}

/// What an icon shows: an item, a faction's crest ([`armory::crest`]), or a faction's banner.
enum Subject {
    Item(IconKey, Box<Item>),
    Crest(Box<Field>),
    Banner(Box<BannerState>),
}

/// A faction's arms as kept: a crest by its field's key, a banner by its field's key and seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Arms {
    Crest(u64),
    Banner(u64, u32),
}

struct Drawing {
    stage: usize,
    image: AssetId<Image>,
    entities: [Entity; 4],
    started: f32,
    frames: u32,
}

impl IconCache {
    /// Whether nothing is waiting to be drawn or being drawn.
    pub fn is_idle(&self) -> bool {
        self.queued.is_empty() && self.drawing.is_empty()
    }

    /// Whether the icon in `image` has been drawn.
    pub fn is_drawn(&self, image: impl Into<AssetId<Image>>) -> bool {
        self.drawn.contains(&image.into())
    }

    /// How many icons are made or being made.
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// Whether no icon has been asked for.
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// How many icons are being drawn now.
    pub fn drawing_count(&self) -> usize {
        self.drawing.len()
    }

    /// Forgets every icon that is drawn and not drawing, so each is made again when next asked
    /// for (the soak's `FB_SOAK_CHURN`, to make and take down many icon cameras).
    pub fn forget_drawn(&mut self) {
        let busy: HashSet<AssetId<Image>> = self.drawing.iter().map(|d| d.image).chain(self.queued.iter().map(|q| q.1.id())).collect();
        self.images.retain(|_, image| busy.contains(&image.id()));
        self.arms.retain(|_, image| busy.contains(&image.id()));
        self.drawn.retain(|id| busy.contains(id));
    }

    /// Forgets the icons queued or being drawn, whose stages are despawned with the delve, so
    /// each is made afresh when next asked for. Icons already drawn are kept.
    pub fn abandon_drawing(&mut self) {
        let busy: HashSet<AssetId<Image>> = self.drawing.drain(..).map(|d| d.image).chain(self.queued.drain(..).map(|q| q.1.id())).collect();
        self.images.retain(|_, image| !busy.contains(&image.id()));
        self.arms.retain(|_, image| !busy.contains(&image.id()));
    }

    /// Forgets every crest and banner, drawn or not, as when the delve ends: the next run's world rolls
    /// other factions, so the kept arms stay to one world's.
    pub fn forget_arms(&mut self) {
        for image in self.arms.drain().map(|(_, image)| image) {
            self.drawn.remove(&image.id());
        }
    }

    /// Records the icon in `image` as drawn, as [`draw_icons`] does.
    #[cfg(test)]
    pub(super) fn mark_drawn(&mut self, image: impl Into<AssetId<Image>>) {
        self.drawn.insert(image.into());
    }
}

/// Where stage `stage` is: a square of stages [`STAGE_SPACING`] apart, kept compact so the
/// furthest is still near enough the origin for fine detail in 32-bit floats.
pub fn stage_at(stage: usize) -> Vec3 {
    ICON_STAGE + Vec3::new((stage % STAGE_ROW) as f32, 0.0, (stage / STAGE_ROW) as f32) * STAGE_SPACING
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
    fonts: Res<'w, UiFonts>,
}

/// An icon node whose icon is not drawn yet, and the initial shown in its place.
#[derive(Component, Debug)]
pub struct AwaitingIcon {
    image: AssetId<Image>,
    placeholder: Entity,
}

/// The faint initial standing in for an icon not drawn yet.
#[derive(Component, Debug)]
pub struct IconPlaceholder;

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
        // Read back by `stage_icon` to see when the item first shows.
        target.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let image = self.images.add(target);
        self.cache.images.insert(key.clone(), image.clone());
        self.cache.queued.push_back((Subject::Item(key, Box::new(item.clone())), image.clone()));
        image
    }

    /// The crest for `field`, drawn once from the armory and kept: the faction's arms flat on a
    /// shield, from the same field its banners are painted from. Blank until drawn.
    pub fn crest(&mut self, field: &Field) -> Handle<Image> {
        self.arms(Arms::Crest(armory::crest::field_key(field)), CREST_PX, || Subject::Crest(Box::new(*field)))
    }

    /// The image of `banner` hanging face on, drawn once from the armory and kept. Blank until
    /// drawn.
    pub fn banner(&mut self, banner: &BannerState) -> Handle<Image> {
        let key = Arms::Banner(armory::crest::field_key(&banner.field), banner.seed);
        self.arms(key, BANNER_PX, || Subject::Banner(Box::new(banner.clone())))
    }

    /// The kept image of `key`, or a new one of `size` queued to draw `subject`.
    fn arms(&mut self, key: Arms, size: UVec2, subject: impl FnOnce() -> Subject) -> Handle<Image> {
        if let Some(image) = self.cache.arms.get(&key) {
            return image.clone();
        }
        let mut target = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
        target.asset_usage = RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD;
        target.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let image = self.images.add(target);
        self.cache.arms.insert(key, image.clone());
        self.cache.queued.push_back((subject(), image.clone()));
        image
    }

    /// Spawns `node` showing the crest for `field`; until it is drawn, a plain shield stands in
    /// its place.
    pub fn spawn_crest<'a>(&mut self, parent: &'a mut ChildSpawnerCommands, field: &Field, node: Node) -> EntityCommands<'a> {
        let image = self.crest(field);
        let shield = BorderRadius { top_left: px(3).into(), top_right: px(3).into(), bottom_left: percent(50).into(), bottom_right: percent(50).into() };
        self.spawn_arms(parent, image, node, shield)
    }

    /// Spawns `node` showing `banner`; until it is drawn, a plain cloth stands in its place.
    pub fn spawn_banner<'a>(&mut self, parent: &'a mut ChildSpawnerCommands, banner: &BannerState, node: Node) -> EntityCommands<'a> {
        let image = self.banner(banner);
        self.spawn_arms(parent, image, node, BorderRadius::all(px(3)))
    }

    /// Spawns `node` showing `image`, with a faint shape rounded by `corners` in its place until
    /// it is drawn.
    fn spawn_arms<'a>(&mut self, parent: &'a mut ChildSpawnerCommands, image: Handle<Image>, node: Node, corners: BorderRadius) -> EntityCommands<'a> {
        let drawn = self.cache.is_drawn(&image);
        let mut arms = parent.spawn((node, ImageNode::new(image.clone())));
        if !drawn {
            let mut placeholder = Entity::PLACEHOLDER;
            arms.with_children(|arms| {
                placeholder = arms
                    .spawn((
                        IconPlaceholder,
                        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), border_radius: corners, ..default() },
                        BackgroundColor(MUTED.with_alpha(0.25)),
                        Pickable::IGNORE,
                    ))
                    .id();
            });
            arms.insert(AwaitingIcon { image: image.id(), placeholder });
        }
        arms
    }

    /// Spawns `node` showing the icon for `item` lying `turned`. Until the icon is drawn, the
    /// item's initial stands faintly in its place, `glyph_px` high.
    pub fn spawn<'a>(&mut self, parent: &'a mut ChildSpawnerCommands, item: &Item, turned: bool, node: Node, glyph_px: f32) -> EntityCommands<'a> {
        let image = self.of(item, turned);
        let drawn = self.cache.is_drawn(&image);
        let mut icon = parent.spawn((node, ImageNode::new(image.clone())));
        if !drawn {
            let mut placeholder = Entity::PLACEHOLDER;
            icon.with_children(|icon| {
                placeholder = icon
                    .spawn((
                        IconPlaceholder,
                        Node {
                            position_type: PositionType::Absolute,
                            width: percent(100),
                            height: percent(100),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        Pickable::IGNORE,
                    ))
                    .with_child((Text::new(initial(&item.name)), UiFonts::style(&self.fonts.display, glyph_px, MUTED.with_alpha(0.45)), Pickable::IGNORE))
                    .id();
            });
            icon.insert(AwaitingIcon { image: image.id(), placeholder });
        }
        icon
    }
}

/// The first letter or digit of `name`, upper case, for an icon's placeholder.
pub fn initial(name: &str) -> String {
    name.chars().find(|c| c.is_alphanumeric()).map_or_else(String::new, |c| c.to_uppercase().collect())
}

/// Takes away the placeholder of every icon that has been drawn.
pub fn reveal_icons(mut commands: Commands, cache: Res<IconCache>, awaiting: Query<(Entity, &AwaitingIcon)>) {
    for (entity, icon) in &awaiting {
        if cache.is_drawn(icon.image) {
            commands.entity(icon.placeholder).try_despawn();
            commands.entity(entity).remove::<AwaitingIcon>();
        }
    }
}

/// Whether `item` is drawn standing as it is built rather than laid along its longest side
/// ([`fb_items::profile::stands_upright`]): worn gear as it is worn, built on the implied
/// mannequin +Y up with its front toward +Z, and the Armory's chests, barrels, crates and
/// amphorae as they stand on the floor. Each reads as what it is only that way up; laid along
/// its longest side a coif or a tunic lay on its side. Its footprint runs the same way, so
/// lying unturned it is upright, and a quarter turn (R) is the only other way it can lie.
pub fn stands_upright(item: &Item) -> bool {
    // A thing built to stand on a base (a pitched tent, a set fire, a candle) is drawn standing
    // as built too: end-on it is a beige box.
    fb_items::profile::shape_stands_upright(&item.shape)
}

/// How an item is posed for its icon: turned so its longest side runs up the image and its
/// next longest across, or, `upright` ([`stands_upright`]), kept as it is built with its
/// front to the viewer; a quarter turn more (clockwise) if it lies `turned`, then turned and
/// tilted a little toward the viewer. Returns the rotation and the posed bounds. `None` for an item
/// with no vertices.
pub fn icon_pose(points: &[Vec3], upright: bool, turned: bool) -> Option<(Quat, Vec3, Vec3)> {
    let (min, max) = bounds(points.iter().copied())?;
    let align = if upright { Quat::IDENTITY } else { longest_up(max - min) };
    let turn = if turned { Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2) } else { Quat::IDENTITY };
    let view = Quat::from_rotation_x(VIEW_PITCH) * Quat::from_rotation_y(-VIEW_YAW);
    let rotation = (view * turn * align).normalize();
    let (min, max) = bounds(points.iter().map(|&p| rotation * p))?;
    Some((rotation, min, max))
}

/// The turn taking an item of `size` so its longest side runs up (+Y) and its next longest
/// across (+X).
fn longest_up(size: Vec3) -> Quat {
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
    Quat::from_mat3(&Mat3::from_cols(columns[0], columns[1], columns[2]))
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
    let IconCache { drawing, drawn, .. } = &mut *cache;
    drawing.retain_mut(|drawing| {
        drawing.frames += 1;
        let done = drawing.frames >= RENDER_FRAMES && now - drawing.started >= RENDER_SECONDS;
        if done {
            for entity in drawing.entities {
                commands.entity(entity).despawn();
            }
            drawn.insert(drawing.image);
        }
        !done
    });
    while cache.drawing.len() < STAGES {
        let Some((subject, image)) = cache.queued.pop_front() else { break };
        let stage = (0..STAGES).find(|s| cache.drawing.iter().all(|d| d.stage != *s)).expect("a free stage");
        let id = image.id();
        // An item with no shape is never drawn and keeps its placeholder.
        let entities = match subject {
            Subject::Item(key, item) => stage_icon(&mut commands, &mut spawner, &item, key.turned, image, stage),
            Subject::Crest(field) => stage_arms(&mut commands, &mut spawner, &armory::crest::crest_mesh(&field), "Icon: crest", CREST_PX, image, stage),
            Subject::Banner(banner) => stage_arms(&mut commands, &mut spawner, &Banner::build(&banner), "Icon: banner", BANNER_PX, image, stage),
        };
        if let Some(entities) = entities {
            cache.drawing.push(Drawing { stage, image: id, entities, started: now, frames: 0 });
        }
    }
}

/// Spawns `item` posed on `stage` with a camera drawing it into `image`, its light, and a
/// readback watching for it to show. Returns the camera, the item, the light and the readback.
fn stage_icon(commands: &mut Commands, spawner: &mut ItemSpawner, item: &Item, turned: bool, image: Handle<Image>, stage: usize) -> Option<[Entity; 4]> {
    let mut mesh = item.shape.build();
    let ammunition = item.profile.tags.contains(&fb_items::Tag::Ammunition);
    if ammunition {
        // Laid slantwise, fanned and thickened so they read in one cell.
        mesh = mesh.fanned(bundle_pieces(item) as usize, 0.45, -0.75, 2.8);
    }
    let points: Vec<Vec3> = mesh.parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p)).collect();
    let (rotation, min, max) = icon_pose(&points, ammunition || stands_upright(item), turned)?;
    let size = icon_size(item, turned);
    Some(stage_mesh(commands, spawner, &mesh, &appearance_of(&item.shape), &format!("Icon: {}", item.name), (rotation, min, max), size, image, stage))
}

/// Spawns a faction's arms, `mesh` (its crest or its banner), on `stage`, face on, into an
/// image of `size`, with the camera, light and readback of an item's icon.
fn stage_arms(commands: &mut Commands, spawner: &mut ItemSpawner, mesh: &armory::geom::ItemMesh, name: &str, size: UVec2, image: Handle<Image>, stage: usize) -> Option<[Entity; 4]> {
    let points: Vec<Vec3> = mesh.parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p)).collect();
    let (min, max) = bounds(points.iter().copied())?;
    let appearance = crate::render::ItemAppearance { palette: crate::render::ItemPalette::ARMORY, ..default() };
    Some(stage_mesh(commands, spawner, mesh, &appearance, name, (Quat::IDENTITY, min, max), size, image, stage))
}

/// Spawns `mesh` posed by `pose` (rotation and posed bounds) on `stage`, with a camera drawing
/// it into `image` of `size`, its light, and a readback watching for it to show. Returns the
/// camera, the mesh, the light and the readback.
#[allow(clippy::too_many_arguments)]
fn stage_mesh(
    commands: &mut Commands,
    spawner: &mut ItemSpawner,
    mesh: &armory::geom::ItemMesh,
    appearance: &crate::render::ItemAppearance,
    name: &str,
    pose: (Quat, Vec3, Vec3),
    size: UVec2,
    image: Handle<Image>,
    stage: usize,
) -> [Entity; 4] {
    let (rotation, min, max) = pose;
    let view = icon_view(min, max, size.x as f32 / size.y as f32);
    let centre = (min + max) * 0.5;
    let at = stage_at(stage);
    let layer = RenderLayers::layer(ICON_LAYER);
    let exposure = Exposure::INDOOR;

    let mut root = spawner.spawn(mesh, appearance);
    root.insert((
        Name::new(name.to_string()),
        DespawnOnExit(AppState::Delving),
        Transform::from_translation(at - centre).with_rotation(rotation),
        Propagate(layer.clone()),
        Propagate(NotShadowCaster),
    ));
    let root = root.id();

    let depth = (max.z - min.z).max(1.0);
    let image_id = image.id();
    let camera = commands
        .spawn((
            Name::new("Icon camera"),
            crate::render::ScreenInkExempt,
            DespawnOnExit(AppState::Delving),
            Camera3d::default(),
            Camera { order: -10 - stage as isize, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(image.clone().into()),
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
            DespawnOnExit(AppState::Delving),
            point_light(Color::WHITE, peak, range, &exposure),
            Transform::from_translation(at + StudioLight::ARMORY.key_from() * distance),
            layer,
        ))
        .id();
    // Each frame until the item shows, its image is read back to see whether anything is drawn.
    let (width, height) = (size.x as usize, size.y as usize);
    let readback = commands
        .spawn((Name::new("Icon readback"), DespawnOnExit(AppState::Delving), Readback::texture(image.clone())))
        .observe(move |event: On<ReadbackComplete>, mut cache: ResMut<IconCache>, mut commands: Commands| {
            if any_pixel_shows(&event.data, width, height) {
                cache.drawn.insert(image_id);
                commands.entity(event.entity).try_remove::<Readback>();
            }
        })
        .id();
    [camera, root, light, readback]
}

/// Whether any pixel of an RGBA8 image `width` by `height`, read back with its rows padded
/// (to wgpu's `COPY_BYTES_PER_ROW_ALIGNMENT`), is not fully clear.
pub fn any_pixel_shows(data: &[u8], width: usize, height: usize) -> bool {
    if width == 0 || height == 0 {
        return false;
    }
    let stride = data.len() / height;
    (0..height).any(|y| data.get(y * stride..y * stride + width * 4).is_some_and(|row| row.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0)))
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
        let (_, min, max) = icon_pose(&box_points(Vec3::new(9.0, 0.3, 2.0)), false, false).unwrap();
        let posed = max - min;
        assert!(posed.y > posed.x * 2.0, "{posed}");
        let (_, min, max) = icon_pose(&box_points(Vec3::new(9.0, 0.3, 2.0)), false, true).unwrap();
        let turned = max - min;
        assert!(turned.x > turned.y * 2.0, "{turned}");
        assert!(icon_pose(&[], false, false).is_none());
    }

    #[test]
    fn a_pitched_tent_is_posed_standing_not_end_on() {
        let packed = fb_items::roll_item("Tent", 4).unwrap();
        let pitched = fb_items::camp::set_up(&packed).unwrap();
        assert!(!stands_upright(&packed), "rolled up it lies along its longest side");
        assert!(stands_upright(&pitched));
        let points: Vec<Vec3> = pitched.shape.build().parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p)).collect();
        let (rotation, min, max) = icon_pose(&points, stands_upright(&pitched), false).unwrap();
        assert!((rotation * Vec3::Y).y > 0.9, "ground down");
        let posed = max - min;
        assert!(posed.x > posed.y, "wider than tall, not stood on end: {posed}");
    }

    #[test]
    fn containers_stand_upright_facing_the_viewer() {
        use armory::families::chests::ChestStyle;
        for style in [ChestStyle::Chest, ChestStyle::Strongbox, ChestStyle::Coffer, ChestStyle::Barrel, ChestStyle::Crate, ChestStyle::Amphora] {
            let item = fb_items::container_item(fb_level::chest_state(style, 11, false));
            assert!(stands_upright(&item), "{style:?}");
            let points: Vec<Vec3> = item.shape.build().parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p)).collect();
            let (rotation, ..) = icon_pose(&points, true, false).unwrap();
            assert!((rotation * Vec3::Y).y > 0.9 && (rotation * Vec3::Z).z > 0.9, "{style:?}");
            // Turned while dragged, it still turns a quarter in the image.
            let (turned, ..) = icon_pose(&points, true, true).unwrap();
            assert!((turned * Vec3::Y).x.abs() > 0.9, "{style:?}");
        }
        assert!(!stands_upright(&fb_items::roll_item("Sword", 3).unwrap()));
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
    fn stages_are_apart_and_clear_of_the_doll() {
        let stages: Vec<Vec3> = (0..STAGES).map(stage_at).collect();
        for (i, a) in stages.iter().enumerate() {
            for b in &stages[i + 1..] {
                assert!(a.distance(*b) >= STAGE_SPACING - 1e-3, "{a} {b}");
            }
            assert!(a.distance(super::super::doll::DOLL_STAGE) > 3.0 * STAGE_SPACING, "{a}");
            // Near enough the origin that a 32-bit float still resolves a hundredth of a unit.
            assert!(a.length() < 120_000.0, "{a}");
        }
        assert_eq!(initial("  apple"), "A");
        assert_eq!(initial(""), "");
        // Two rows of two pixels, each row padded to eight pixels; only the padding is set.
        let mut data = vec![0u8; 2 * 8 * 4];
        data[2 * 4 + 3] = 255;
        assert!(!any_pixel_shows(&data, 2, 2));
        data[8 * 4 + 4 + 3] = 40;
        assert!(any_pixel_shows(&data, 2, 2));
    }

    #[test]
    fn icons_are_drawn_for_the_cells_they_fill() {
        let interface = crate::tuning::Tuning::default().interface;
        // Twice the cell at 1080 lines, so never enlarged up to 2160 lines at one pixel a point.
        assert!(ICON_CELL_PX as f32 >= interface.cell_px(Vec2::new(3840.0, 2160.0)));
        assert!(ICON_CELL_PX as f32 >= 2.0 * interface.cell_px(Vec2::new(1920.0, 1080.0)));
    }

    #[test]
    fn ammunition_icons_draw_a_bundle_by_count_and_the_bundle_fills_the_cell() {
        let arrow = fb_items::roll_item("Arrow", 3).unwrap();
        let counts: Vec<u8> = [1, 2, 5, 6, 10].iter().map(|&n| bundle_pieces(&arrow.clone().with_quantity(n))).collect();
        assert_eq!(counts, vec![1, 3, 3, 5, 5]);
        assert_eq!(bundle_pieces(&fb_items::roll_item("Sword", 3).unwrap()), 1);
        // Fanned five ways, the bundle is a fat diagonal, not the hairline one arrow is.
        let one = arrow.shape.build();
        let (lo, hi) = one.bounds().unwrap();
        let bundle = one.fanned(5, 0.45, -0.75, 2.8);
        assert_eq!(bundle.parts.len(), one.parts.len() * 5);
        let points: Vec<Vec3> = bundle.parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p)).collect();
        let (blo, bhi) = bounds(points.iter().copied()).unwrap();
        let (width, height) = (bhi.x - blo.x, bhi.y - blo.y);
        assert!(width > 0.5 * (hi[0] - lo[0]) && height > 0.5 * (hi[0] - lo[0]), "diagonal: {width} by {height}");
    }

    #[test]
    fn icons_are_the_shape_of_their_footprint() {
        let pack = fb_items::roll_item("Pack", 7).unwrap();
        let fp = pack.profile.footprint;
        assert_eq!(icon_size(&pack, false), UVec2::new(fp.cols as u32, fp.rows as u32) * ICON_CELL_PX);
        assert_eq!(icon_size(&pack, true), UVec2::new(fp.rows as u32, fp.cols as u32) * ICON_CELL_PX);
    }
}
