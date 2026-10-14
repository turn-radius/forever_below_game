//! The props standing along the level's walls (`fb_level::PropSpot`): containers and scenery,
//! and opening containers.
//!
//! - Containers are Armory chests, barrels, crates and amphorae (`fb_level::chest_state`),
//!   drawn in their own palette, condition and etching as any item is. Scenery (rubble, bones,
//!   pillars) is built by `armory::build::scenery` and drawn as the level's fixtures are.
//! - Each rests on the drawn floor (`geometry::Relief::floor_height`) at its lowest point
//!   under it, turned to its spot's yaw, and is lit by the level's lights like everything else.
//! - Each blocks the way ([`BlocksTheWay`]) with its `fb_level::FloorShape`: a circle for
//!   round things, a rectangle turned with it for chests, crates and pillars.
//! - Every container is kept track of in the [`Inventory`], loose, as an item with its inner
//!   grid (`fb_items::container_item`), filled when the level is laid out with what its loot
//!   seed rolls (`fb_items::roll_contents`). What is in it stays there for the rest of the run,
//!   whatever is taken out or put in.
//! - Looked at within reach, a container is under the crosshair as a floor item is
//!   (`carry::Aimed`), and the pick-up key opens it instead of picking it up: the inventory
//!   opens with its grid beside what lies underfoot ([`OpenContainer`]). A chest, strongbox or
//!   coffer then stands open in the level, its lid raised, for good.
//!
//! `FB_OPEN_CONTAINER=<n>` opens the level's `n`th container (0 the first, in `print_level`'s
//! order) a few frames in, for captures.

use armory::{Item, build::scenery::Scenery, cond::ConditionState};
use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_items::footprint::Footprint;
use fb_level::{CELL, Level, Prop, PropSpot, chest_state};

use crate::carry::{Aimed, appearance_of, item_points};
use crate::inventory::{Inventory, ItemId, OpenContainer};
use crate::keymap::{Action, ActionInput, Keymap};
use crate::level::{LevelPiece, fixture_appearance, geometry::Relief};
use crate::play::{BlocksTheWay, CurrentLevel, DelveSystems, Screen, ScreenOpen, level_has_focus, show_screen};
use crate::render::ItemSpawner;

pub struct PropsPlugin;

impl Plugin for PropsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Inventory>()
            .init_resource::<Aimed>()
            .init_resource::<Keymap>()
            .init_resource::<ScreenOpen>()
            .init_resource::<OpenContainer>()
            .add_systems(
                Update,
                (
                    lay_out_props.run_if(resource_exists_and_changed::<CurrentLevel>),
                    open_aimed_container.run_if(level_has_focus),
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
pub fn prop_mesh(prop: Prop, level: &Level, opened: bool) -> (armory::ItemMesh, crate::render::ItemAppearance) {
    match prop {
        Prop::Container { style, seed, .. } => {
            let item = Item::Chests(chest_state(style, seed, opened));
            (item.build(), appearance_of(&item))
        }
        Prop::Scenery { kind, seed } => {
            let state = if kind == Scenery::Bones { ConditionState::Neglected } else { ConditionState::BattleWorn };
            (kind.build(seed, level.ceiling), fixture_appearance(seed, state))
        }
    }
}

/// Where a prop of item-space `points` stands at `spot`: turned to its yaw, with its lowest
/// point at the lowest the drawn floor falls under its footprint, less [`SINK`].
pub fn prop_transform(level: &Level, spot: &PropSpot, points: &[Vec3]) -> Option<Transform> {
    let low = points.iter().map(|p| p.y).reduce(f32::min)?;
    let relief = Relief::new(level);
    let centre = Vec3::from_array(Level::world(spot.x, spot.y));
    let reach = spot.prop.floor_shape().radius();
    let floor = std::iter::once(Vec2::ZERO)
        .chain((0..8).map(|k| Vec2::from_angle(k as f32 * std::f32::consts::TAU / 8.0) * reach * 0.8))
        .map(|offset| relief.floor_height(centre.x + offset.x, centre.z + offset.y))
        .fold(f32::INFINITY, f32::min);
    Some(Transform::from_translation(Vec3::new(centre.x, floor - low - SINK, centre.z)).with_rotation(Quat::from_rotation_y(spot.yaw)))
}

/// Spawns prop `index` of the level, shut or `opened`, and returns its commands to insert on.
/// `None` for a prop with no vertices.
pub fn spawn_prop<'a>(spawner: &'a mut ItemSpawner, level: &Level, index: usize, opened: bool) -> Option<EntityCommands<'a>> {
    let spot = *level.props.get(index)?;
    let (mesh, appearance) = prop_mesh(spot.prop, level, opened);
    let points = item_points(&mesh);
    let transform = prop_transform(level, &spot, &points)?;
    let (min, max) = points.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), &p| (lo.min(p), hi.max(p)));
    let name = match spot.prop {
        Prop::Container { style, .. } => format!("{style:?}"),
        Prop::Scenery { kind, .. } => format!("{kind:?}"),
    };
    let mut root = spawner.spawn(&mesh, &appearance);
    root.insert((LevelPiece, Name::new(name), transform, PropPiece { index, spot, min, max }, BlocksTheWay(spot.prop.floor_shape())));
    Some(root)
}

/// Keeps track of a container prop in `inventory`, loose, holding what its loot rolls: each
/// thing at its first free spot, upright first and then turned, and anything with no room
/// left out. `None` for scenery.
pub fn stock_container(inventory: &mut Inventory, prop: Prop) -> Option<ItemId> {
    let Prop::Container { style, seed, loot } = prop else { return None };
    let container = inventory.add(fb_items::container_item(chest_state(style, seed, false)));
    for item in fb_items::roll_contents(style, loot) {
        let id = inventory.add(item);
        let footprint = inventory.footprint(id).unwrap_or(Footprint { cols: 1, rows: 1 });
        let at = inventory.grid(container).and_then(|g| g.first_free(footprint));
        if at.is_none_or(|at| inventory.put(id, container, at).is_err()) {
            inventory.forget(id);
        }
    }
    Some(container)
}

/// Spawns (or respawns, when the level changes) every prop the level lays out, each container
/// stocked in the inventory.
fn lay_out_props(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    old: Query<(Entity, Option<&Container>), With<PropPiece>>,
    mut inventory: ResMut<Inventory>,
    mut spawner: ItemSpawner,
) {
    for (entity, container) in &old {
        if let Some(container) = container {
            inventory.forget(container.id);
        }
        commands.entity(entity).despawn();
    }
    for (index, spot) in level.0.props.iter().enumerate() {
        let stocked = stock_container(&mut inventory, spot.prop);
        if let Some(mut root) = spawn_prop(&mut spawner, &level.0, index, false)
            && let Some(id) = stocked
        {
            root.insert(Container { id, opened: false });
        }
    }
}

/// Opens container prop `entity`: its grid goes up beside what lies underfoot with the
/// inventory screen, and a lidded one is redrawn standing open, once.
#[allow(clippy::too_many_arguments)]
fn open_container(
    commands: &mut Commands,
    spawner: &mut ItemSpawner,
    level: &Level,
    entity: Entity,
    piece: &PropPiece,
    container: Container,
    open: &mut ScreenOpen,
    showing: &mut OpenContainer,
    cursor: Option<&mut CursorOptions>,
) {
    showing.0 = Some(container.id);
    show_screen(open, Some(Screen::Inventory), cursor);
    if container.opened {
        return;
    }
    commands.entity(entity).despawn();
    if let Some(mut root) = spawn_prop(spawner, level, piece.index, true) {
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
    mut spawner: ItemSpawner,
) {
    if !input.just_pressed(Action::PickUp) {
        return;
    }
    let Some(entity) = aimed.0 else { return };
    let Ok((piece, &container)) = containers.get(entity) else { return };
    aimed.0 = None;
    let cursor = cursor.as_deref_mut().map(|c| &mut **c);
    open_container(&mut commands, &mut spawner, &level.0, entity, piece, container, &mut open, &mut showing, cursor);
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
    open_container(&mut commands, &mut spawner, &level.0, entity, piece, container, &mut open, &mut showing, cursor);
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
    use crate::player::{BODY_RADIUS, blocked_by, slide_around};

    /// An app that lays out the props of 1207, with no renderer.
    fn props_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::transform::TransformPlugin)
            .insert_resource(CurrentLevel(fb_level::generate(1207)))
            .init_resource::<Obstacles>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
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
            .find(|(_, p, _)| matches!(p.spot.prop, Prop::Container { style: armory::families::chests::ChestStyle::Chest, .. }))
            .map(|(e, _, c)| (e, c))
            .expect("a chest on 1207")
    }

    /// Looks at `entity` and presses the pick-up key for a frame.
    fn press_pick_up_on(app: &mut App, entity: Entity) {
        app.world_mut().resource_mut::<Aimed>().0 = Some(entity);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyE);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::KeyE);
        keys.clear();
    }

    fn contents(app: &App, container: ItemId) -> Vec<(ItemId, String)> {
        let inventory = app.world().resource::<Inventory>();
        inventory.grid(container).unwrap().entries().iter().map(|e| (e.id, inventory.item(e.id).unwrap().title().to_string())).collect()
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

        // E on it: the inventory opens with its grid, and it stands open in the level.
        press_pick_up_on(&mut app, entity);
        assert!(app.world().resource::<ScreenOpen>().is(Screen::Inventory));
        assert_eq!(app.world().resource::<OpenContainer>().0, Some(container.id));
        assert!(app.world().get_entity(entity).is_err(), "redrawn standing open");
        let (opened_entity, opened) = chest(&mut app);
        assert_eq!(opened, Container { id: container.id, opened: true });
        let piece = *app.world().get::<PropPiece>(opened_entity).unwrap();
        let shut = prop_mesh(piece.spot.prop, &app.world().resource::<CurrentLevel>().0, false).0.bounds().unwrap();
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
    fn every_container_on_1207_holds_what_its_loot_rolls() {
        let app = props_app();
        let level = fb_level::generate(1207);
        let inventory = app.world().resource::<Inventory>();
        for spot in &level.props {
            let Prop::Container { style, loot, .. } = spot.prop else { continue };
            let rolled = fb_items::roll_contents(style, loot);
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
        let level = fb_level::generate(1207);
        let mut pieces = app.world_mut().query::<(&PropPiece, &Children)>();
        let drawn: Vec<(usize, usize)> = pieces.iter(app.world()).map(|(p, children)| (p.index, children.len())).collect();
        assert_eq!(drawn.len(), level.props.len());
        assert!(drawn.iter().all(|&(_, parts)| parts > 0), "{drawn:?}");
        let mut meshes = app.world_mut().query::<&Mesh3d>();
        assert!(meshes.iter(app.world()).count() >= level.props.len());
    }

    #[test]
    fn no_prop_on_1207_floats_or_sinks() {
        let level = fb_level::generate(1207);
        let relief = Relief::new(&level);
        for (index, spot) in level.props.iter().enumerate() {
            let (mesh, _) = prop_mesh(spot.prop, &level, false);
            let points = item_points(&mesh);
            let transform = prop_transform(&level, spot, &points).unwrap();
            let bottom = points.iter().map(|&p| transform.transform_point(p).y).fold(f32::INFINITY, f32::min);
            let floor = relief.floor_height(transform.translation.x, transform.translation.z);
            assert!(bottom <= floor + 1e-3, "prop {index} floats: base {bottom}, floor {floor}");
            assert!(bottom > floor - 0.4, "prop {index} sinks: base {bottom}, floor {floor}");
        }
    }

    #[test]
    fn the_player_cannot_walk_through_props() {
        let app = props_app();
        let level = fb_level::generate(1207);
        let obstacles = app.world().resource::<Obstacles>().0.clone();
        assert_eq!(obstacles.len(), level.props.len());
        let blocked = crate::play::BlockedCells::default();
        for spot in &level.props {
            let centre = floor_centre(spot);
            let at = |o: &&Obstacle| match o {
                Obstacle::Circle { centre: c, .. } | Obstacle::Rect { centre: c, .. } => c.abs_diff_eq(centre, 1e-3),
            };
            let obstacle = obstacles.iter().find(at).unwrap_or_else(|| panic!("{spot:?} blocks nothing"));
            match (spot.prop.floor_shape(), obstacle) {
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
}
