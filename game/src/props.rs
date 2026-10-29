//! The props standing along the level's walls (`fb_level::PropSpot`): containers and scenery,
//! and opening containers.
//!
//! - Containers are Armory chests, barrels, crates and amphorae (`fb_level::chest_state`),
//!   drawn in their own palette, condition and etching as any item is. Scenery (rubble, bones,
//!   pillars) is built by `armory::build::scenery` and drawn as the level's fixtures are.
//! - Each rests on the drawn floor (`geometry::Relief::floor_height`) at its lowest point
//!   under it, turned to its spot's yaw, and is lit by the level's lights like everything else.
//!   Containers are drawn larger than true Armory scale, at the tuning's `container_scale`
//!   ([`prop_scale`]), so a chest is about a metre wide.
//! - Each blocks the way ([`BlocksTheWay`]) with its `fb_level::FloorShape`: a circle for
//!   round things, a rectangle turned with it for chests, crates and pillars, grown or shrunk
//!   with a container's scale ([`prop_floor_shape`]).
//! - Every container is kept track of in the [`Inventory`], loose, as an item with its inner
//!   grid (`fb_items::container_item`), filled when the level is laid out with what its loot
//!   seed rolls (`fb_items::roll_contents`). What is in it stays there for the rest of the run,
//!   whatever is taken out or put in.
//! - Looked at within reach, a container is under the crosshair as a floor item is
//!   (`carry::Aimed`), and the pick-up key opens it instead of picking it up: the inventory
//!   opens with its grid beside what lies underfoot ([`OpenContainer`]). A chest, strongbox or
//!   coffer then stands open in the level, its lid raised, for good.
//!
//! - Aimed at, a container also puts its verbs over the crosshair ([`prompt_aimed_container`]):
//!   "F Open" while shut, nothing once it stands open.
//!
//! `FB_OPEN_CONTAINER=<n>` opens the level's `n`th container (0 the first, in `print_level`'s
//! order) a few frames in, for captures.

use armory::{Item, build::scenery::Scenery, cond::ConditionState};
use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_items::footprint::Footprint;
use armory::families::banner::BannerState;
use fb_level::{BannerSite, BannerSpot, CELL, CrystalSpot, CrystalSurface, Level, Prop, PropSpot, chest_state, crystal_state};

use crate::carry::{Aimed, appearance_of, item_points};
use crate::inventory::{Inventory, ItemId, OpenContainer};
use crate::interact::{CrosshairPrompt, Delver, Lock, Target, key_line, verbs_for};
use crate::keymap::{Action, ActionInput, Keymap};
use crate::level::{LevelPiece, fixture_appearance, geometry::Relief};
use crate::play::{BlocksTheWay, CurrentLevel, DelveSystems, Screen, ScreenOpen, level_has_focus, show_screen};
use crate::record::{WorldRecord, restore_contents};
use crate::render::ItemSpawner;
use crate::tuning::Tuning;

pub struct PropsPlugin;

impl Plugin for PropsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((crate::interact::InteractPlugin, crate::remains::RemainsPlugin))
            .init_resource::<Inventory>()
            .init_resource::<Aimed>()
            .init_resource::<Keymap>()
            .init_resource::<ScreenOpen>()
            .init_resource::<OpenContainer>()
            .add_systems(
                Update,
                (
                    lay_out_props.run_if(resource_exists_and_changed::<CurrentLevel>),
                    lay_out_banners.run_if(resource_exists_and_changed::<CurrentLevel>),
                    crate::niches::lay_out_niches.run_if(resource_exists_and_changed::<CurrentLevel>),
                    lay_out_crystals.run_if(resource_exists_and_changed::<CurrentLevel>),
                    rescale_props.run_if(resource_exists_and_changed::<Tuning>),
                    open_aimed_container.run_if(level_has_focus),
                    prompt_aimed_container.after(crate::carry::aim),
                    open_for_capture.run_if(|| std::env::var("FB_OPEN_CONTAINER").is_ok()),
                )
                    .chain()
                    .in_set(DelveSystems),
            );
    }
}

/// A prop standing in the level: which of `CurrentLevel.props` it is, and its item-space
/// bounds for looking at it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PropPiece {
    pub index: usize,
    pub spot: PropSpot,
    pub min: Vec3,
    pub max: Vec3,
}

/// On a container prop: the item it is in the [`Inventory`], whose grid holds what is in it,
/// and whether it has been opened.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Container {
    pub id: ItemId,
    pub opened: bool,
}

/// How far below the floor's lowest point under it a prop is set, so no seam of light shows
/// under it where the floor's relief rises and falls across its base.
const SINK: f32 = 0.05;

/// The prop's mesh and look. A container is shut until `opened`.
pub fn prop_mesh(spot: &PropSpot, level: &Level, opened: bool) -> (armory::ItemMesh, crate::render::ItemAppearance) {
    match spot.prop {
        Prop::Container { style, seed, .. } => {
            let item = Item::Chests(chest_state(style, seed, opened));
            (item.build(), appearance_of(&item))
        }
        Prop::Campfire { seed } => {
            let item = Item::Campfire(fb_level::props::campfire_state(seed));
            (item.build(), appearance_of(&item))
        }
        Prop::Scenery { kind, seed } => {
            let state = if kind == Scenery::Bones { ConditionState::Neglected } else { ConditionState::BattleWorn };
            let [x, _, z] = Level::world(spot.x, spot.y);
            (kind.build(seed, crate::level::geometry::stone::ceiling_at(level, x, z)), fixture_appearance(seed, state))
        }
    }
}

/// How much larger than its own size `prop` is drawn: a container at the tuning's
/// `container_scale`, scenery as built.
pub fn prop_scale(prop: Prop, tuning: &Tuning) -> f32 {
    if prop.is_container() { tuning.view.container_scale } else { 1.0 }
}

/// The floor `prop` blocks drawn `scale` times its size ([`prop_scale`]). `fb_level` sizes a
/// container's shape for [`fb_level::CONTAINER_SCALE`]; another scale grows or shrinks it.
pub fn prop_floor_shape(prop: Prop, scale: f32) -> fb_level::FloorShape {
    let shape = prop.floor_shape();
    if prop.is_container() { shape.scaled(scale / fb_level::CONTAINER_SCALE) } else { shape }
}

/// Where a prop whose item-space lowest point is `low` stands at `spot`, drawn `scale` times
/// its size: turned to its yaw, with its lowest point at the lowest the drawn floor falls
/// under its footprint, less [`SINK`].
pub fn prop_transform(level: &Level, spot: &PropSpot, low: f32, scale: f32) -> Transform {
    let relief = Relief::new(level);
    let centre = Vec3::from_array(Level::world(spot.x, spot.y));
    let reach = prop_floor_shape(spot.prop, scale).radius();
    let floor = std::iter::once(Vec2::ZERO)
        .chain((0..8).map(|k| Vec2::from_angle(k as f32 * std::f32::consts::TAU / 8.0) * reach * 0.8))
        .map(|offset| relief.floor_height(centre.x + offset.x, centre.z + offset.y))
        .fold(f32::INFINITY, f32::min);
    Transform::from_translation(Vec3::new(centre.x, floor - low * scale - SINK, centre.z))
        .with_rotation(Quat::from_rotation_y(spot.yaw))
        .with_scale(Vec3::splat(scale))
}

/// Spawns prop `index` of the level, shut or `opened`, at the tuning's scale ([`prop_scale`]),
/// and returns its commands to insert on. `None` for a prop with no vertices.
pub fn spawn_prop<'a>(spawner: &'a mut ItemSpawner, level: &Level, index: usize, opened: bool, tuning: &Tuning) -> Option<EntityCommands<'a>> {
    let spot = *level.props.get(index)?;
    let (mesh, appearance) = prop_mesh(&spot, level, opened);
    spawn_prop_shaped(spawner, level, index, &mesh, &appearance, tuning)
}

/// [`spawn_prop`] for a prop whose `mesh` and `appearance` are made already (`crate::staged`).
pub fn spawn_prop_shaped<'a>(spawner: &'a mut ItemSpawner, level: &Level, index: usize, mesh: &armory::ItemMesh, appearance: &crate::render::ItemAppearance, tuning: &Tuning) -> Option<EntityCommands<'a>> {
    let spot = *level.props.get(index)?;
    let points = item_points(mesh);
    let (min, max) = points.iter().fold(None, |acc: Option<(Vec3, Vec3)>, &p| Some(acc.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p)))))?;
    let scale = prop_scale(spot.prop, tuning);
    let transform = prop_transform(level, &spot, min.y, scale);
    let name = match spot.prop {
        Prop::Container { style, .. } => format!("{style:?}"),
        Prop::Scenery { kind, .. } => format!("{kind:?}"),
        Prop::Campfire { .. } => "Campfire".to_string(),
    };
    let mut root = spawner.spawn(mesh, appearance);
    root.insert((LevelPiece, Name::new(name), transform, PropPiece { index, spot, min, max }, BlocksTheWay(prop_floor_shape(spot.prop, scale))));
    Some(root)
}

/// Redraws every prop at the tuning's scale when it changes, standing where it stood, with the
/// floor it blocks to match.
fn rescale_props(tuning: Res<Tuning>, level: Res<CurrentLevel>, mut props: Query<(&PropPiece, &mut Transform, &mut BlocksTheWay)>) {
    for (piece, mut transform, mut blocks) in &mut props {
        let scale = prop_scale(piece.spot.prop, &tuning);
        if transform.scale.x != scale {
            *transform = prop_transform(&level.0, &piece.spot, piece.min.y, scale);
            blocks.0 = prop_floor_shape(piece.spot.prop, scale);
        }
    }
}

/// A faction banner hanging on a wall: which of `CurrentLevel.banners` it is. Banners are
/// cloth on a rod, high on the wall, so they block nothing.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BannerPiece {
    pub index: usize,
}

/// How far below the ceiling a rod hangs at most, in world units, and the least clear space
/// under a banner's lowest point (its fringe included).
const BANNER_BELOW_CEILING: f32 = 2.0;
const BANNER_OFF_FLOOR: f32 = 4.0;
/// Where a hall banner's middle hangs, in world units up: a little above the eye, so the cloth
/// reads from across the hall.
const BANNER_MIDDLE: f32 = 20.0;
/// A lintel banner's lowest point: just over the top of the doorway.
const LINTEL_FOOT: f32 = crate::level::geometry::DOOR_HEIGHT + 1.5;

/// The banner `spot` hangs, and how high its rod hangs: its faction's own banner
/// ([`fb_faction::FactionSpec::banner`]), the same shape wherever the faction hangs it, and short
/// and wide over a door. The rod is set from the cloth, so the middle sits at [`BANNER_MIDDLE`]
/// (or the foot just over the doorway for a lintel banner); under a low ceiling the cloth is
/// shortened rather than hung higher.
pub fn banner_hang(spot: &BannerSpot, level: &Level) -> (BannerState, f32) {
    let faction = level.faction(spot.faction);
    let lintel = spot.site == BannerSite::Lintel;
    let mut state = if lintel { faction.banner_sized((5.0, 7.0), (5.0, 8.0)) } else { faction.banner() };
    let [x, _, z] = Level::world(spot.x, spot.y);
    let highest = crate::level::geometry::stone::ceiling_at(level, x, z) - BANNER_BELOW_CEILING;
    let length = state.cloth.length;
    let (top, length) = if lintel {
        let top = (LINTEL_FOOT + length).min(highest);
        (top, length.min(top - LINTEL_FOOT).max(3.0))
    } else {
        let top = (BANNER_MIDDLE + length / 2.0).min(highest);
        (top, length.min(top - BANNER_OFF_FLOOR))
    };
    state.cloth.length = length;
    (state, top.max(length + BANNER_OFF_FLOOR))
}

/// Where a banner's rod hangs: at its spot, `top` up, facing out from the wall.
pub fn banner_transform(spot: &BannerSpot, top: f32) -> Transform {
    let centre = Vec3::from_array(Level::world(spot.x, spot.y));
    Transform::from_translation(Vec3::new(centre.x, top, centre.z)).with_rotation(Quat::from_rotation_y(spot.yaw))
}

/// Spawns (or respawns, when the level changes) the level's banners.
fn lay_out_banners(mut commands: Commands, level: Res<CurrentLevel>, old: Query<Entity, With<BannerPiece>>, mut spawner: ItemSpawner) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    for (index, spot) in level.0.banners.iter().enumerate() {
        let (state, top) = banner_hang(spot, &level.0);
        let item = Item::Banner(state);
        let (mesh, appearance) = (item.build(), appearance_of(&item));
        let name = format!("Banner of {}", level.0.faction(spot.faction).name);
        spawner.spawn(&mesh, &appearance).insert((LevelPiece, Name::new(name), banner_transform(spot, top), BannerPiece { index }));
    }
}

/// A crystal growing in a crystal cavern: which of `CurrentLevel.crystals` it is. A floor
/// crystal blocks the way with the floor its foot covers; wall and ceiling crystals and geodes
/// stand over head height or flush with the wall and block nothing.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CrystalPiece {
    pub index: usize,
}

/// How far a single crystal's base is sunk into the rock it grows from, in world units.
const CRYSTAL_ROOT: f32 = 0.8;
/// How far a geode's rim stands proud of the wall it is set in.
const GEODE_PROUD: f32 = 0.25;

/// How far up a crystal's item axis its mesh meets the surface it grows from, in item units:
/// the middle of a cluster's rock (half of it sunk in), a geode's rim (the rind stands in the
/// wall), or a single's base, sunk a little.
pub fn crystal_mount(mesh: &armory::ItemMesh, spot: &CrystalSpot) -> f32 {
    if spot.growth == armory::families::crystal::Growth::Mass {
        return armory::families::crystal::mass_embed(spot.size);
    }
    let part = |name: &str| mesh.parts.iter().find(|p| p.name == name);
    let ys = |p: &armory::Part| p.positions.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| (lo.min(v[1]), hi.max(v[1])));
    if let Some(rind) = part("rind") {
        ys(rind).1 - GEODE_PROUD
    } else if let Some(matrix) = part("matrix") {
        let (lo, hi) = ys(matrix);
        (lo + hi) * 0.5
    } else {
        CRYSTAL_ROOT
    }
}

/// Where the crystal at `spot` stands: a floor crystal rests on the drawn floor at its lowest
/// point under its foot, a wall or ceiling crystal has its root `mount` into the surface it
/// grows from, its axis along [`CrystalSpot::axis`] and turned about it by its seed.
pub fn crystal_transform(level: &Level, spot: &CrystalSpot, mesh: &armory::ItemMesh, low: f32) -> Transform {
    let [x, _, z] = Level::world(spot.x, spot.y);
    let axis = Vec3::from_array(spot.axis());
    let twist = Quat::from_rotation_y((spot.seed % 6283) as f32 * 0.001);
    let rotation = Quat::from_rotation_arc(Vec3::Y, axis) * twist;
    match spot.surface {
        CrystalSurface::Floor | CrystalSurface::Foot => {
            let relief = Relief::new(level);
            let reach = spot.radius.max(1.0);
            let floor = std::iter::once(Vec2::ZERO)
                .chain((0..8).map(|k| Vec2::from_angle(k as f32 * std::f32::consts::TAU / 8.0) * reach * 0.8))
                .map(|offset| relief.floor_height(x + offset.x, z + offset.y))
                .fold(f32::INFINITY, f32::min);
            let low = if spot.growth == armory::families::crystal::Growth::Mass { crystal_mount(mesh, spot) } else { low };
            Transform::from_translation(Vec3::new(x, floor - low - SINK, z)).with_rotation(rotation)
        }
        CrystalSurface::Wall => Transform::from_translation(Vec3::new(x, spot.up, z) - axis * crystal_mount(mesh, spot)).with_rotation(rotation),
        CrystalSurface::Ceiling => {
            let top = crate::level::geometry::stone::ceiling_at(level, x, z);
            Transform::from_translation(Vec3::new(x, top, z) - axis * crystal_mount(mesh, spot)).with_rotation(rotation)
        }
    }
}

/// Spawns (or respawns, when the level changes) the level's crystals.
fn lay_out_crystals(mut commands: Commands, level: Res<CurrentLevel>, old: Query<Entity, With<CrystalPiece>>, mut spawner: ItemSpawner) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    for (index, spot) in level.0.crystals.iter().enumerate() {
        let item = Item::Crystal(crystal_state(spot));
        let (mesh, mut appearance) = (item.build(), appearance_of(&item));
        // The rock they grow from is the cavern's own pale stone.
        if let Some(stone) = level.0.cavern_stone() {
            appearance.palette.materials.stone = stone.rgb();
        }
        let low = item_points(&mesh).iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let name = item.noun();
        let mut root = spawner.spawn(&mesh, &appearance);
        root.insert((LevelPiece, Name::new(name), crystal_transform(&level.0, spot, &mesh, low), CrystalPiece { index }));
        if matches!(spot.surface, CrystalSurface::Floor | CrystalSurface::Foot) {
            root.insert(BlocksTheWay(fb_level::FloorShape::Round(spot.radius)));
        }
    }
}

/// Keeps track of a container prop in `inventory`, loose, holding what its loot rolls against
/// `odds`: each thing at its first free spot, upright first and then turned, and anything with
/// no room left out. `None` for scenery. A container set out empty ([`fb_level::NO_LOOT`]) holds nothing.
pub fn stock_container(inventory: &mut Inventory, spot: &PropSpot, odds: fb_items::Odds, level: &fb_level::Level, lore: &fb_items::notes::Lore) -> Option<ItemId> {
    let contents = level.container_contents(spot, odds, lore);
    stock_container_with(inventory, spot.prop, contents)
}

/// [`stock_container`] with the `contents` it rolled already (`crate::staged`).
pub fn stock_container_with(inventory: &mut Inventory, prop: Prop, contents: Vec<fb_items::Item>) -> Option<ItemId> {
    let Prop::Container { style, seed, .. } = prop else { return None };
    let container = inventory.add(fb_items::container_item(chest_state(style, seed, false)));
    for item in contents {
        // What is found in a container is unknown if it is magic.
        let id = inventory.add(crate::identify::found_unknown(item));
        let footprint = inventory.footprint(id).unwrap_or(Footprint { cols: 1, rows: 1 });
        let at = inventory.grid(container).and_then(|g| g.first_free(footprint));
        if at.is_none_or(|at| inventory.put(id, container, at).is_err()) {
            inventory.forget(id);
        }
    }
    Some(container)
}

/// Spawns (or respawns, when the level changes) every prop the level lays out, each container
/// stocked in the inventory, with what was left in it and open or shut as it was left if the
/// level was visited before (`crate::record`).
#[allow(clippy::too_many_arguments)]
fn lay_out_props(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    old: Query<(Entity, Option<&Container>), With<PropPiece>>,
    mut inventory: ResMut<Inventory>,
    tuning: Option<Res<Tuning>>,
    record: Option<Res<WorldRecord>>,
    mut staged: Option<ResMut<crate::staged::Staged>>,
    character: Option<Res<crate::character::Character>>,
    mut spawner: ItemSpawner,
) {
    let tuning = tuning.as_deref().cloned().unwrap_or_default();
    // Chests hold what rolls for the luck the delver wears as they come to the level.
    let odds = crate::character::loot_odds(character.as_deref(), Some(&inventory), level.0.depth);
    let mut ahead = staged.as_mut().and_then(|s| s.take_props(&level.0, odds)).map(Vec::into_iter);
    let saved = record.as_deref().and_then(|r| r.level(level.0.depth));
    let lore = level.0.lore();
    for (entity, container) in &old {
        if let Some(container) = container {
            inventory.forget(container.id);
        }
        commands.entity(entity).despawn();
    }
    for (index, spot) in level.0.props.iter().enumerate() {
        // What was made ahead of time, in step with the props.
        let ahead = ahead.as_mut().and_then(Iterator::next);
        // A container that was left holds what was left in it, and stands open if it was opened.
        let left = saved.and_then(|l| l.containers.get(&index));
        let stocked = match (left, spot.prop) {
            (Some(left), Prop::Container { style, seed, .. }) => {
                let container = inventory.add(fb_items::container_item(chest_state(style, seed, false)));
                restore_contents(&mut inventory, container, &left.contents);
                Some(container)
            }
            _ => match &ahead {
                Some(ahead) => stock_container_with(&mut inventory, spot.prop, ahead.contents.clone()),
                None => stock_container(&mut inventory, spot, odds, &level.0, &lore),
            },
        };
        let opened = left.is_some_and(|c| c.opened);
        let root = match &ahead {
            Some(ahead) if !opened => spawn_prop_shaped(&mut spawner, &level.0, index, &ahead.mesh, &ahead.appearance, &tuning),
            _ => spawn_prop(&mut spawner, &level.0, index, opened, &tuning),
        };
        if let Some(mut root) = root
            && let Some(id) = stocked
        {
            root.insert(Container { id, opened });
        }
    }
}

/// Opens container prop `entity`: its grid goes up beside what lies underfoot with the
/// inventory screen, and a lidded one is redrawn standing open, once.
#[allow(clippy::too_many_arguments)]
pub fn open_container(
    commands: &mut Commands,
    spawner: &mut ItemSpawner,
    level: &Level,
    entity: Entity,
    piece: &PropPiece,
    container: Container,
    open: &mut ScreenOpen,
    showing: &mut OpenContainer,
    cursor: Option<&mut CursorOptions>,
    tuning: &Tuning,
) {
    showing.0 = Some(container.id);
    show_screen(open, Some(Screen::Inventory), cursor);
    if container.opened {
        return;
    }
    commands.entity(entity).despawn();
    if let Some(mut root) = spawn_prop(spawner, level, piece.index, true, tuning) {
        root.insert(Container { opened: true, ..container });
    }
}

/// The pick-up key on a container under the crosshair opens it ([`open_container`]).
#[allow(clippy::too_many_arguments)]
fn open_aimed_container(
    mut commands: Commands,
    input: ActionInput,
    mut aimed: ResMut<Aimed>,
    containers: Query<(&PropPiece, &Container)>,
    level: Res<CurrentLevel>,
    mut open: ResMut<ScreenOpen>,
    mut showing: ResMut<OpenContainer>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
    tuning: Option<Res<Tuning>>,
    mut spawner: ItemSpawner,
) {
    if !input.just_pressed(Action::Primary) {
        return;
    }
    let Some(entity) = aimed.0 else { return };
    let Ok((piece, &container)) = containers.get(entity) else { return };
    aimed.0 = None;
    let cursor = cursor.as_deref_mut().map(|c| &mut **c);
    let tuning = tuning.as_deref().cloned().unwrap_or_default();
    open_container(&mut commands, &mut spawner, &level.0, entity, piece, container, &mut open, &mut showing, cursor, &tuning);
}

/// Says what the aimed container's verbs are over the crosshair. Containers have no locks yet,
/// so a shut one is opened and an open one looked in.
pub(crate) fn prompt_aimed_container(
    aimed: Res<Aimed>,
    containers: Query<&Container>,
    keymap: Res<Keymap>,
    mut prompt: ResMut<CrosshairPrompt>,
    mut said: Local<Option<String>>,
) {
    let line = aimed.0.and_then(|entity| containers.get(entity).ok()).map(|container| {
        let target = Target::Container { open: container.opened, lock: Lock::NONE };
        key_line(&keymap, &verbs_for(&target, &Delver::default()))
    });
    let line = line.filter(|line| !line.is_empty());
    // Clear only what this system set: other aimed things (folk) have their own say.
    if line.is_some() || said.is_some() {
        if prompt.0 != line {
            prompt.0.clone_from(&line);
        }
        *said = line;
    }
}

/// For captures (`FB_OPEN_CONTAINER=<n>`): a few frames in, opens the level's `n`th container.
#[allow(clippy::too_many_arguments)]
fn open_for_capture(
    mut commands: Commands,
    containers: Query<(Entity, &PropPiece, &Container)>,
    level: Res<CurrentLevel>,
    mut open: ResMut<ScreenOpen>,
    mut showing: ResMut<OpenContainer>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
    tuning: Option<Res<Tuning>>,
    mut spawner: ItemSpawner,
    mut frames: Local<u32>,
) {
    *frames += 1;
    if *frames != 4 {
        return;
    }
    let n: usize = std::env::var("FB_OPEN_CONTAINER").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut all: Vec<(Entity, &PropPiece, &Container)> = containers.iter().collect();
    all.sort_by_key(|(_, piece, _)| piece.index);
    let Some(&(entity, piece, &container)) = all.get(n) else { return };
    let cursor = cursor.as_deref_mut().map(|c| &mut **c);
    let tuning = tuning.as_deref().cloned().unwrap_or_default();
    open_container(&mut commands, &mut spawner, &level.0, entity, piece, container, &mut open, &mut showing, cursor, &tuning);
}

/// A prop's centre on the floor plane (world `(x, z)`).
pub fn floor_centre(spot: &PropSpot) -> Vec2 {
    Vec2::new(spot.x, spot.y) * CELL
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::{Obstacle, Obstacles, gather_obstacles};
    use fb_level::FloorShape;
    use armory::families::banner::BannerMount;
    use crate::player::{blocked_by, slide_around};
    /// The player's body radius as shipped.
    const BODY_RADIUS: f32 = 3.5;

    /// An app that lays out the props of 1207, with no renderer.
    fn props_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::transform::TransformPlugin)
            .insert_resource(CurrentLevel(fb_level::generate(1207, 1)))
            .init_resource::<Obstacles>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
            .init_resource::<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>()
            .init_resource::<crate::render::OutlineMaterials>()
            .add_plugins(PropsPlugin)
            .add_systems(PostUpdate, gather_obstacles.after(TransformSystems::Propagate));
        app.update();
        app
    }

    /// The first chest on 1207 (`print_level 1207`): its entity and its container.
    fn chest(app: &mut App) -> (Entity, Container) {
        let mut chests = app.world_mut().query::<(Entity, &PropPiece, &Container)>();
        let mut all: Vec<(Entity, PropPiece, Container)> = chests.iter(app.world()).map(|(e, p, c)| (e, *p, *c)).collect();
        all.sort_by_key(|(_, p, _)| p.index);
        all.into_iter()
            // The first chest that holds loot: a market's stalls are set out empty.
            .find(|(_, p, _)| matches!(p.spot.prop, Prop::Container { style: armory::families::chests::ChestStyle::Chest, .. }) && p.spot.prop.holds_loot())
            .map(|(e, _, c)| (e, c))
            .expect("a chest with loot on 1207")
    }

    /// Looks at `entity` and presses the pick-up key for a frame.
    fn press_pick_up_on(app: &mut App, entity: Entity) {
        app.world_mut().resource_mut::<Aimed>().0 = Some(entity);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyF);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::KeyF);
        keys.clear();
    }

    fn contents(app: &App, container: ItemId) -> Vec<(ItemId, String)> {
        let inventory = app.world().resource::<Inventory>();
        inventory.grid(container).unwrap().entries().iter().map(|e| (e.id, inventory.item(e.id).unwrap().title().to_string())).collect()
    }

    #[test]
    fn aiming_at_a_shut_container_prompts_to_open_it() {
        let mut app = props_app();
        let (entity, container) = chest(&mut app);
        assert_eq!(app.world().resource::<CrosshairPrompt>().0, None);
        app.world_mut().resource_mut::<Aimed>().0 = Some(entity);
        app.update();
        assert_eq!(app.world().resource::<CrosshairPrompt>().0.as_deref(), Some("F Open"));
        // Rebound, the prompt follows.
        let mut keymap = app.world_mut().resource_mut::<Keymap>();
        keymap.bindings.iter_mut().find(|b| b.action == Action::Primary).unwrap().inputs = vec![crate::keymap::Input::Key(KeyCode::KeyE)];
        app.update();
        assert_eq!(app.world().resource::<CrosshairPrompt>().0.as_deref(), Some("E Open"));
        // Standing open, the same key looks in it again; looking away clears the line.
        app.world_mut().entity_mut(entity).insert(Container { opened: true, ..container });
        app.update();
        assert_eq!(app.world().resource::<CrosshairPrompt>().0.as_deref(), Some("E Look in"));
        app.world_mut().resource_mut::<Aimed>().0 = None;
        app.update();
        assert_eq!(app.world().resource::<CrosshairPrompt>().0, None);
    }

    #[test]
    fn a_chest_on_1207_opens_to_its_loot_and_keeps_what_is_left_in_it() {
        let mut app = props_app();
        app.init_resource::<ButtonInput<KeyCode>>();
        // A pack on the delver's back to take things into.
        let pack = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
            inventory.wear(pack, fb_items::Slot::Back).unwrap();
            pack
        };
        let (entity, container) = chest(&mut app);
        assert!(!container.opened);
        let loot = contents(&app, container.id);
        assert!(!loot.is_empty(), "an empty chest on 1207");
        let (cols, rows, _) = fb_items::profile::chest_grid(armory::families::chests::ChestStyle::Chest);
        let grid = app.world().resource::<Inventory>().grid(container.id).unwrap().clone();
        assert_eq!((grid.cols, grid.rows), (cols, rows));

        // F on it: the inventory opens with its grid, and it stands open in the level.
        press_pick_up_on(&mut app, entity);
        assert!(app.world().resource::<ScreenOpen>().is(Screen::Inventory));
        assert_eq!(app.world().resource::<OpenContainer>().0, Some(container.id));
        assert!(app.world().get_entity(entity).is_err(), "redrawn standing open");
        let (opened_entity, opened) = chest(&mut app);
        assert_eq!(opened, Container { id: container.id, opened: true });
        let piece = *app.world().get::<PropPiece>(opened_entity).unwrap();
        let shut = prop_mesh(&piece.spot, &app.world().resource::<CurrentLevel>().0, false).0.bounds().unwrap();
        assert!(piece.max.y > shut.1[1] + 1.0, "the lid stands raised: {} over {}", piece.max.y, shut.1[1]);

        // Dragging the first thing out onto the pack's grid puts it in the pack, and changes
        // nothing in the level.
        let (taken, _) = loot[0];
        {
            use crate::inventory::{Drag, GridView, Target, drop_at, underfoot::UNDERFOOT_COLS};
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let size = inventory.footprint(taken).unwrap();
            let drag = Drag { id: taken, grab: IVec2::ZERO, size, turned: false, press: Vec2::ZERO, active: true, target: None };
            let changes = drop_at(&mut inventory, &drag, Target::Cell { view: GridView::Container(pack), cell: IVec2::ZERO }, UNDERFOOT_COLS).unwrap();
            assert!(changes.is_empty());
            assert!(inventory.is_carried(taken));
            assert_eq!(inventory.location(taken), Some(crate::inventory::Location::In { container: pack, at: crate::inventory::Placement::new(0, 0, false) }));
        }

        // Closed and opened again: the same chest, the same contents less what was taken, and
        // it stays open.
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        app.world_mut().resource_mut::<OpenContainer>().0 = None;
        press_pick_up_on(&mut app, opened_entity);
        assert_eq!(app.world().resource::<OpenContainer>().0, Some(container.id));
        assert!(app.world().get_entity(opened_entity).is_ok(), "an open chest is not redrawn");
        assert_eq!(contents(&app, container.id), loot[1..].to_vec());
        assert_eq!(chest(&mut app).1, opened);
    }

    #[test]
    fn the_level_s_things_are_spawned_once_and_stay_put() {
        use crate::carry::{Aimed, FloorItem, lay_out_items, pick_up};
        use crate::folk::{Denizen, FolkFigures, idle_folk, spawn_folk};
        use crate::inventory::underfoot::{FloorChanges, apply_floor_changes};
        use crate::play::{BlockedCells, CurrentLevel};
        let mut app = props_app();
        // Everything that lays out, or may touch, the level every frame while delving.
        app.add_plugins(bevy::time::TimePlugin)
            .init_resource::<Aimed>()
            .init_resource::<BlockedCells>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<crate::keymap::Keymap>()
            .init_resource::<FolkFigures>()
            .init_resource::<FloorChanges>()
            .add_systems(
                Update,
                (
                    lay_out_items.run_if(resource_exists_and_changed::<CurrentLevel>),
                    spawn_folk.run_if(resource_exists_and_changed::<CurrentLevel>),
                    idle_folk,
                    pick_up,
                    apply_floor_changes,
                ),
            );
        // The level was laid out by `props_app`'s first frame; items and folk need it changed.
        app.world_mut().resource_mut::<CurrentLevel>().set_changed();
        app.update();
        type Roots<'w> = (Entity, &'w Transform, Has<FloorItem>, Has<PropPiece>, Has<Denizen>);
        let snapshot = |app: &mut App| {
            let mut roots = app.world_mut().query_filtered::<Roots, Or<(With<FloorItem>, With<PropPiece>, With<Denizen>)>>();
            let mut all: Vec<(Entity, Transform, bool, bool, bool)> = roots.iter(app.world()).map(|(e, t, f, p, d)| (e, *t, f, p, d)).collect();
            all.sort_by_key(|r| r.0);
            all
        };
        let first = snapshot(&mut app);
        let level = fb_level::generate(1207, 1);
        assert_eq!(first.iter().filter(|r| r.2).count(), level.items.len());
        assert_eq!(first.iter().filter(|r| r.3).count(), level.props.len());
        assert_eq!(first.iter().filter(|r| r.4).count(), level.folk.len());
        for _ in 0..120 {
            app.update();
        }
        assert_eq!(snapshot(&mut app), first, "something in the level was respawned or moved");
    }

    #[test]
    fn a_container_opened_from_the_level_has_an_upright_icon() {
        use crate::inventory::icon::{icon_pose, stands_upright};
        let mut app = props_app();
        app.init_resource::<ButtonInput<KeyCode>>();
        let mut containers = app.world_mut().query::<(Entity, &PropPiece, &Container)>();
        let all: Vec<(Entity, Prop)> = containers.iter(app.world()).map(|(e, p, _)| (e, p.spot.prop)).collect();
        for (entity, prop) in all {
            app.world_mut().resource_mut::<OpenContainer>().0 = None;
            app.world_mut().resource_mut::<ScreenOpen>().0 = None;
            press_pick_up_on(&mut app, entity);
            let id = app.world().resource::<OpenContainer>().0.expect("opened");
            let item = app.world().resource::<Inventory>().item(id).unwrap().clone();
            assert!(stands_upright(&item), "{prop:?}");
            let points: Vec<Vec3> = item.shape.build().parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p)).collect();
            let (rotation, ..) = icon_pose(&points, stands_upright(&item), false).unwrap();
            // Up stays up and the front faces the viewer, give or take the view's small turn
            // and tilt.
            assert!((rotation * Vec3::Y).y > 0.9, "{prop:?}: up is {}", rotation * Vec3::Y);
            assert!((rotation * Vec3::Z).z > 0.9, "{prop:?}: front is {}", rotation * Vec3::Z);
        }
    }

    #[test]
    fn every_container_on_1207_holds_what_its_loot_rolls() {
        let app = props_app();
        let level = fb_level::generate(1207, 1);
        let inventory = app.world().resource::<Inventory>();
        for spot in &level.props {
            let Prop::Container { style, loot, .. } = spot.prop else { continue };
            if !spot.prop.holds_loot() {
                continue;
            }
            let rolled = level.container_contents(spot, fb_items::Odds::loot(level.depth), &level.lore());
            assert!(!rolled.is_empty());
            // Some container in the inventory holds exactly these, by title, in roll order.
            let titles: Vec<String> = rolled.iter().map(|i| i.title().to_string()).collect();
            let held = inventory.ids().filter(|&id| inventory.grid(id).is_some() && inventory.location(id) == Some(crate::inventory::Location::Loose)).any(|id| {
                let mut held: Vec<String> = inventory.grid(id).unwrap().entries().iter().map(|e| inventory.item(e.id).unwrap().title().to_string()).collect();
                held.truncate(titles.len());
                held == titles
            });
            assert!(held, "{style:?} {loot}: {titles:?}");
        }
    }

    #[test]
    fn every_prop_on_1207_is_drawn() {
        let mut app = props_app();
        let level = fb_level::generate(1207, 1);
        let mut pieces = app.world_mut().query::<(&PropPiece, &Children)>();
        let drawn: Vec<(usize, usize)> = pieces.iter(app.world()).map(|(p, children)| (p.index, children.len())).collect();
        assert_eq!(drawn.len(), level.props.len());
        assert!(drawn.iter().all(|&(_, parts)| parts > 0), "{drawn:?}");
        let mut meshes = app.world_mut().query::<&Mesh3d>();
        assert!(meshes.iter(app.world()).count() >= level.props.len());
    }

    #[test]
    fn containers_are_drawn_a_metre_wide_and_block_what_they_cover() {
        let level = fb_level::generate(1207, 1);
        let tuning = Tuning::default();
        for spot in level.props.iter().filter(|s| s.prop.is_container()) {
            let scale = prop_scale(spot.prop, &tuning);
            let (mesh, _) = prop_mesh(spot, &level, true);
            let (lo, hi) = mesh.bounds().unwrap();
            let (lo, hi) = (Vec3::from_array(lo) * scale, Vec3::from_array(hi) * scale);
            if let Prop::Container { style: armory::families::chests::ChestStyle::Chest, .. } = spot.prop {
                let width = hi.x - lo.x;
                assert!((8.5..=12.0).contains(&width), "a chest {width} wide");
            }
            match prop_floor_shape(spot.prop, scale) {
                FloorShape::Round(r) => assert!(lo.x.abs().max(hi.x).max(lo.z.abs()).max(hi.z) <= r, "{spot:?}"),
                FloorShape::Square { half_width, half_depth } => {
                    let behind = spot.prop.room_behind() * scale / fb_level::CONTAINER_SCALE;
                    assert!(lo.x >= -half_width && hi.x <= half_width && hi.z <= half_depth && lo.z >= -half_depth - behind, "{spot:?}: {lo} {hi}");
                }
            }
        }
    }

    #[test]
    fn a_changed_container_scale_redraws_props_where_they_stand() {
        let mut app = props_app();
        app.init_resource::<Tuning>();
        app.update();
        let (entity, _) = chest(&mut app);
        let before = *app.world().get::<Transform>(entity).unwrap();
        assert_eq!(before.scale, Vec3::splat(fb_level::CONTAINER_SCALE));
        app.world_mut().resource_mut::<Tuning>().view.container_scale = 1.0;
        app.update();
        let after = *app.world().get::<Transform>(entity).unwrap();
        assert_eq!(after.scale, Vec3::ONE);
        assert_eq!(after.translation.xz(), before.translation.xz());
        let blocks = app.world().get::<BlocksTheWay>(entity).unwrap().0;
        assert!(blocks.radius() < prop_floor_shape(app.world().get::<PropPiece>(entity).unwrap().spot.prop, fb_level::CONTAINER_SCALE).radius());
    }

    #[test]
    fn no_prop_on_1207_floats_or_sinks() {
        let level = fb_level::generate(1207, 1);
        let relief = Relief::new(&level);
        for (index, spot) in level.props.iter().enumerate() {
            let (mesh, _) = prop_mesh(spot, &level, false);
            let points = item_points(&mesh);
            let low = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            let transform = prop_transform(&level, spot, low, prop_scale(spot.prop, &Tuning::default()));
            let bottom = points.iter().map(|&p| transform.transform_point(p).y).fold(f32::INFINITY, f32::min);
            let floor = relief.floor_height(transform.translation.x, transform.translation.z);
            assert!(bottom <= floor + 1e-3, "prop {index} floats: base {bottom}, floor {floor}");
            assert!(bottom > floor - 0.4, "prop {index} sinks: base {bottom}, floor {floor}");
        }
    }

    #[test]
    fn the_player_cannot_walk_through_props() {
        let app = props_app();
        let level = fb_level::generate(1207, 1);
        let obstacles = app.world().resource::<Obstacles>().0.clone();
        assert_eq!(obstacles.len(), level.props.len());
        let blocked = crate::play::BlockedCells::default();
        for spot in &level.props {
            let centre = floor_centre(spot);
            let at = |o: &&Obstacle| match o {
                Obstacle::Circle { centre: c, .. } | Obstacle::Rect { centre: c, .. } => c.abs_diff_eq(centre, 1e-3),
            };
            let obstacle = obstacles.iter().find(at).unwrap_or_else(|| panic!("{spot:?} blocks nothing"));
            match (prop_floor_shape(spot.prop, prop_scale(spot.prop, &Tuning::default())), obstacle) {
                (FloorShape::Round(r), Obstacle::Circle { centre: c, radius }) => assert!(c.abs_diff_eq(centre, 1e-3) && *radius == r),
                (FloorShape::Square { .. }, Obstacle::Rect { centre: c, .. }) => assert!(c.abs_diff_eq(centre, 1e-3)),
                other => panic!("{other:?}"),
            }
            // Walking straight at it from a step into the room: stopped short, never inside.
            let into_room = Vec2::new(spot.yaw.sin(), spot.yaw.cos());
            let mut at = centre + into_room * (spot.prop.floor_shape().radius() + BODY_RADIUS + 4.0);
            for _ in 0..120 {
                at = slide_around(at, at - into_room * 0.5, BODY_RADIUS, blocked_by(&level, &blocked), &obstacles);
                assert!(!obstacle.overlaps(at, BODY_RADIUS), "{spot:?}: walked into it at {at}");
            }
        }
    }

    #[test]
    fn banners_carry_their_factions_field_and_hang_big_and_mid_wall() {
        let mut seen = 0;
        for seed in [1207, 3, 9, 7, 12] {
            let level = fb_level::generate(seed, 1);
            for spot in &level.banners {
                seen += 1;
                let (state, top) = banner_hang(spot, &level);
                assert_eq!(state.field, level.faction(spot.faction).field, "seed {seed}: {spot:?}");
                assert_eq!(state.hanging.mount, BannerMount::Rod);
                let ceiling = crate::level::geometry::stone::ceiling_at(&level, spot.x * CELL, spot.y * CELL);
                assert!(top <= ceiling - 1.9 && top - state.cloth.length >= BANNER_OFF_FLOOR - 1e-3, "{spot:?}: top {top}, length {}", state.cloth.length);
                let middle = top - state.cloth.length / 2.0;
                if spot.site == BannerSite::Lintel {
                    assert!(top - state.cloth.length >= LINTEL_FOOT - 1e-3 && state.cloth.width >= 5.0, "{spot:?}: {:?}", state.cloth);
                } else {
                    assert!(state.cloth.length >= 14.0 && state.cloth.width >= 5.0, "{spot:?}: {:?}", state.cloth);
                    assert!((17.0..=21.0).contains(&middle), "{spot:?}: middle {middle}");
                }
                assert_eq!(state, banner_hang(spot, &level).0, "the same seed hangs the same banner");
            }
        }
        assert!(seen > 0);
    }

    #[test]
    fn a_faction_hangs_one_banner_shape_everywhere() {
        // A banner's shape, apart from the length a low ceiling cuts it to and the size over a door.
        let shape = |b: &BannerState| (b.cloth.end, b.cloth.end_depth, b.hem, b.fringe.on, b.hanging.stuff, b.hanging.finial, b.palette);
        let mut compared = 0;
        for seed in [1207, 3, 9, 7, 12] {
            let level = fb_level::generate(seed, 1);
            for spot in &level.banners {
                let (state, _) = banner_hang(spot, &level);
                let own = level.faction(spot.faction).banner();
                assert_eq!(shape(&state), shape(&own), "seed {seed}: {spot:?}");
                if spot.site != BannerSite::Lintel {
                    assert_eq!(state.cloth.width, own.cloth.width, "seed {seed}: {spot:?}");
                }
                compared += 1;
            }
        }
        assert!(compared > 0);
    }

    #[test]
    fn banners_are_drawn_and_block_nothing() {
        let mut app = props_app();
        let level = fb_level::generate(1207, 1);
        let mut banners = app.world_mut().query_filtered::<(&BannerPiece, &Children), Without<BlocksTheWay>>();
        assert_eq!(banners.iter(app.world()).count(), level.banners.len());
        assert!(!level.banners.is_empty());
        assert!(banners.iter(app.world()).all(|(_, parts)| !parts.is_empty()));
    }
}
