//! The delve: one level of the abyss, played first person. This module holds what the play
//! plugins share; each plugin owns its own file.
//!
//! - [`crate::level`]: the level's stone geometry, its fixed lights and the entrance door.
//! - [`crate::player`]: the first-person body, camera, movement and collision.
//! - [`crate::carry`]: the torch in hand, items lying in the level, picking them up, the HUD.
//! - [`crate::folk`]: the folk standing in the level.
//! - [`crate::creatures`]: the rats and snakes in the level.
//! - [`crate::combat`]: swinging weapons, trading blows, hit points and falling.
//! - [`crate::props`]: the containers and scenery standing along the walls, and opening
//!   containers.
//! - [`crate::inventory`]: the inventory screen over the level (I).
//! - [`crate::keymap`]: every key and button, read by action.
//! - [`crate::menu`]: the pause menu over the level (Esc).
//! - [`crate::audio`]: the level's music.

use bevy::{
    platform::collections::HashSet,
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_level::{FloorShape, Level};

use crate::keymap::{Action, ActionInput, Keymap};
use crate::launch::AppState;

/// Every system that plays the level, in `FixedUpdate`, `Update` and `PostUpdate`. The play plugins
/// put their systems here, and [`PlayPlugin`] runs the set only while [`AppState::Delving`];
/// a plugin added on its own (in tests) runs its systems always. What a delve spawns at its
/// start goes in `OnEnter(AppState::Delving)`. Everything a delve spawns, at its start or
/// later, is scoped to it: its root carries `DespawnOnExit(AppState::Delving)`, given at the
/// spawn or required by its marker component ([`Player`], `LevelPiece`, `Critter`, `Denizen`,
/// `FloorItem`, `Hud` and the like), so leaving the delve despawns it with no list to keep
/// (Bevy's state-scoped entities,
/// <https://docs.rs/bevy/0.20.0/bevy/state/state_scoped/struct.DespawnOnExit.html>).
/// `game/tests/delve_scope.rs` checks it.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct DelveSystems;

/// Runs [`DelveSystems`] only while delving, in `FixedUpdate` as well as `Update` and `PostUpdate`.
pub fn run_delve_systems_while_delving(app: &mut App) {
    app.configure_sets(FixedUpdate, DelveSystems.run_if(in_state(AppState::Delving)))
        .configure_sets(Update, DelveSystems.run_if(in_state(AppState::Delving)))
        .configure_sets(PostUpdate, DelveSystems.run_if(in_state(AppState::Delving)));
}

/// The level being played. Inserted before the play plugins run.
#[derive(Resource, Clone, Debug)]
pub struct CurrentLevel(pub Level);

/// The player's body. Its `Transform` is at the feet, on the floor; the camera is a child.
#[derive(Component, Debug, Default, Reflect)]
#[reflect(Component)]
#[require(DespawnOnExit<crate::launch::AppState> = DespawnOnExit(crate::launch::AppState::Delving), crate::exhaustion::Exhaustion, crate::exhaustion::ExhaustionTimes, crate::character::Load, crate::character::DelverNeeds, crate::character::DelverStamina, crate::character::DelverMana, crate::character::DelverVisibility, crate::character::DelverRecovery, crate::player::GearPace)]
pub struct Player;

/// The first-person camera, a child of [`Player`].
#[derive(Component, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct PlayerCamera;

/// Eye height above the floor in world units (1.6 m, a human's eyes at 17 units tall).
pub const EYE_HEIGHT: f32 = 16.0;

/// Grid cells that are open floor in the level data but closed to walking now, such as the
/// entrance once its door shuts. Player collision treats them as rock.
#[derive(Resource, Clone, Debug, Default)]
pub struct BlockedCells(pub HashSet<(i32, i32)>);

/// Something on the floor that stops the player as rock does, on the floor plane (world
/// `(x, z)`, world units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Obstacle {
    /// Someone standing, a barrel, a heap of rubble.
    Circle { centre: Vec2, radius: f32 },
    /// A chest, a crate, a pillar's plinth: half extents along its own x and z, turned `yaw`
    /// about +Y as a [`Transform`]'s rotation turns it.
    Rect { centre: Vec2, half: Vec2, yaw: f32 },
}

impl Obstacle {
    /// `shape` standing where `transform` puts it.
    pub fn of(shape: FloorShape, transform: &GlobalTransform) -> Obstacle {
        let centre = transform.translation().xz();
        match shape {
            FloorShape::Round(radius) => Obstacle::Circle { centre, radius },
            FloorShape::Square { half_width, half_depth } => {
                let (yaw, _, _) = transform.rotation().to_euler(EulerRot::YXZ);
                Obstacle::Rect { centre, half: Vec2::new(half_width, half_depth), yaw }
            }
        }
    }

    /// `at` in the obstacle's own frame (its x and z), from its centre.
    fn local(&self, at: Vec2) -> Vec2 {
        match *self {
            Obstacle::Circle { centre, .. } => at - centre,
            Obstacle::Rect { centre, yaw, .. } => (Quat::from_rotation_y(-yaw) * Vec3::new(at.x - centre.x, 0.0, at.y - centre.y)).xz(),
        }
    }

    /// A point of the obstacle's own frame back on the floor.
    fn world(&self, local: Vec2) -> Vec2 {
        match *self {
            Obstacle::Circle { centre, .. } => centre + local,
            Obstacle::Rect { centre, yaw, .. } => centre + (Quat::from_rotation_y(yaw) * Vec3::new(local.x, 0.0, local.y)).xz(),
        }
    }

    /// Where a circle of `radius` at `at` goes to clear the obstacle, or `None` if it already
    /// does. `previous` is a clear spot nearby, which chooses the way out when `at` is deep
    /// inside.
    pub fn push_out(&self, at: Vec2, radius: f32, previous: Vec2) -> Option<Vec2> {
        let p = self.local(at);
        match *self {
            Obstacle::Circle { radius: size, .. } => {
                let reach = size + radius;
                if p.length() >= reach {
                    return None;
                }
                let normal = p.try_normalize().or_else(|| self.local(previous).try_normalize()).unwrap_or(Vec2::Y);
                Some(self.world(normal * reach))
            }
            Obstacle::Rect { half, .. } => {
                let nearest = p.clamp(-half, half);
                let offset = p - nearest;
                let distance = offset.length();
                if distance >= radius {
                    return None;
                }
                if distance > 1e-5 {
                    return Some(self.world(nearest + offset / distance * radius));
                }
                // Inside: out through the face `previous` was furthest outside of.
                let before = self.local(previous);
                let faces = [(before.x - half.x, Vec2::X), (-before.x - half.x, Vec2::NEG_X), (before.y - half.y, Vec2::Y), (-before.y - half.y, Vec2::NEG_Y)];
                let normal = faces.into_iter().max_by(|a, b| a.0.total_cmp(&b.0)).map_or(Vec2::Y, |f| f.1);
                let out = if normal.x != 0.0 { Vec2::new(normal.x * (half.x + radius), p.y) } else { Vec2::new(p.x, normal.y * (half.y + radius)) };
                Some(self.world(out))
            }
        }
    }

    /// Whether a circle of `radius` at `at` overlaps it (by more than a hair).
    pub fn overlaps(&self, at: Vec2, radius: f32) -> bool {
        let p = self.local(at);
        match *self {
            Obstacle::Circle { radius: size, .. } => p.length() < size + radius - 1e-3,
            Obstacle::Rect { half, .. } => (p - p.clamp(-half, half)).length() < radius - 1e-3,
        }
    }
}

/// What stops the player on the floor besides rock: one [`Obstacle`] for every entity that
/// [`BlocksTheWay`], gathered each frame.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Obstacles(pub Vec<Obstacle>);

/// On an entity standing in the level that the player cannot walk through (folk, props): the
/// floor it covers, centred on and turned with its transform.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BlocksTheWay(pub FloorShape);

/// Keeps [`Obstacles`] to what [`BlocksTheWay`] now.
pub fn gather_obstacles(blockers: Query<(&BlocksTheWay, &GlobalTransform)>, mut obstacles: ResMut<Obstacles>) {
    let now: Vec<Obstacle> = blockers.iter().map(|(blocks, at)| Obstacle::of(blocks.0, at)).collect();
    if obstacles.0 != now {
        obstacles.0 = now;
    }
}

/// A screen drawn over the level, which takes the mouse and keys while it is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Screen {
    /// The inventory ([`crate::inventory`]).
    Inventory,
    /// The pause menu ([`crate::menu`]).
    Menu,
    /// The map of the level ([`crate::map`]).
    Map,
}

/// Which screen, if any, is open over the level. The level keeps drawing behind it, but the
/// body stands still and does not look round.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScreenOpen(pub Option<Screen>);

impl ScreenOpen {
    /// Whether `screen` is the one open.
    pub fn is(&self, screen: Screen) -> bool {
        self.0 == Some(screen)
    }
}

/// Run condition: no screen is open over the level, the delver is not between levels
/// ([`crate::travel`]) and the debug inspector is closed ([`crate::inspect`]).
pub fn level_has_focus(screen: Option<Res<ScreenOpen>>, journey: Option<Res<crate::travel::Journey>>, inspector: Option<Res<crate::inspect::InspectorOpen>>) -> bool {
    !screen.is_some_and(|s| s.0.is_some()) && journey.is_none() && !inspector.is_some_and(|i| i.0)
}

/// Run condition: the inventory screen is open, for keys that act on the item under the mouse.
pub fn inventory_open(open: Option<Res<ScreenOpen>>) -> bool {
    open.is_some_and(|o| o.is(Screen::Inventory))
}

/// Which leaf of the inventory screen is showing: the plate's tab bar. The outfit tab is a
/// label only for now.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SheetTab {
    /// The grids: the pack, the floor and the slots.
    #[default]
    Inventory,
    /// The character sheet and status.
    Character,
}

/// The keys pressed this frame that open, close or swap screens.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScreenKeys {
    pub inventory: bool,
    pub character: bool,
    pub map: bool,
    pub back: bool,
}

/// The screen and tab open after this frame's presses, from `open` and `tab`: Esc closes an
/// open screen first, and with none open brings up the menu; the inventory key opens the
/// inventory on its inventory tab and the character key on its character tab, each closing
/// the screen when its tab is already showing and otherwise swapping to its tab; the map key
/// opens and closes the map, swapping with the inventory. None of them opens over the menu.
/// With no inventory showing the tab is the inventory's.
pub fn next_screen(open: Option<Screen>, tab: SheetTab, keys: ScreenKeys) -> (Option<Screen>, SheetTab) {
    let ScreenKeys { inventory, character, map, back } = keys;
    let tab_asked = if inventory {
        Some(SheetTab::Inventory)
    } else if character {
        Some(SheetTab::Character)
    } else {
        None
    };
    let next = match (open, tab_asked) {
        (Some(_), _) if back => (None, tab),
        (None, _) if back => (Some(Screen::Menu), tab),
        (Some(Screen::Inventory), Some(asked)) if asked == tab => (None, tab),
        (Some(Screen::Inventory), Some(asked)) => (open, asked),
        (None | Some(Screen::Map), Some(asked)) => (Some(Screen::Inventory), asked),
        (Some(Screen::Map), None) if map => (None, tab),
        (None | Some(Screen::Inventory), None) if map => (Some(Screen::Map), tab),
        _ => (open, tab),
    };
    if next.0 == Some(Screen::Inventory) { next } else { (next.0, SheetTab::Inventory) }
}

/// Opens `screen` (or closes every screen with `None`), freeing the mouse for a screen and
/// taking it back for the level.
pub fn show_screen(open: &mut ScreenOpen, screen: Option<Screen>, cursor: Option<&mut CursorOptions>) {
    open.0 = screen;
    if let Some(cursor) = cursor {
        crate::player::set_grab(cursor, screen.is_none());
    }
}

/// Opens and closes screens from the keyboard ([`next_screen`]). Systems that draw a screen
/// run after it.
pub fn switch_screens(
    input: ActionInput,
    mut open: ResMut<ScreenOpen>,
    mut tab: ResMut<SheetTab>,
    journey: Option<Res<crate::travel::Journey>>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
) {
    // No screen opens between levels.
    if journey.is_some() {
        return;
    }
    let keys = ScreenKeys {
        inventory: input.just_pressed(Action::Inventory),
        character: input.just_pressed(Action::Character),
        map: input.just_pressed(Action::Map),
        back: input.just_pressed(Action::Back),
    };
    let (next, next_tab) = next_screen(open.0, *tab, keys);
    if next != open.0 {
        show_screen(&mut open, next, cursor.as_deref_mut().map(|c| &mut **c));
    }
    if next_tab != *tab {
        *tab = next_tab;
    }
}

/// Everything needed to play a level.
pub struct PlayPlugin;

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        run_delve_systems_while_delving(app);
        app.init_resource::<BlockedCells>()
            .init_resource::<Obstacles>()
            .init_resource::<ScreenOpen>()
            .init_resource::<SheetTab>()
            .init_resource::<Keymap>()
            .add_systems(Update, switch_screens.in_set(DelveSystems))
            .add_systems(PostUpdate, gather_obstacles.after(TransformSystems::Propagate).in_set(DelveSystems))
            .add_plugins((
            (crate::ambient::AmbientPlugin, crate::footsteps::FootstepsPlugin, crate::breath::BreathPlugin, crate::game_clock::GameClockPlugin),
            crate::level::LevelPlugin,
            crate::player::PlayerPlugin,
            crate::carry::CarryPlugin,
            (crate::folk::FolkPlugin, crate::folk_routine::FolkRoutinePlugin, crate::folk_combat::FolkCombatPlugin),
            crate::creatures::CreaturesPlugin,
            crate::combat::CombatPlugin,
            crate::props::PropsPlugin,
            crate::inventory::InventoryPlugin,
            crate::audio::MusicPlugin,
            crate::menu::MenuPlugin,
            crate::kindle::KindlePlugin,
            crate::eat::EatPlugin,
            crate::simulate::SimulatePlugin,
            crate::map::MapPlugin,
        ))
        // Over the plugin tuple's limit.
        .add_plugins((crate::quickslots::QuickslotPlugin, crate::status::StatusPlugin, crate::travel::TravelPlugin, crate::sight::SightPlugin, crate::dazzle::DazzlePlugin));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turned_rect_pushes_a_body_out_across_its_nearest_side() {
        // A chest 8 wide and 4 deep, turned a quarter turn so its width runs along z.
        let rect = Obstacle::Rect { centre: Vec2::new(10.0, 10.0), half: Vec2::new(4.0, 2.0), yaw: std::f32::consts::FRAC_PI_2 };
        // Coming from +x, 1 unit into its side: pushed back out along +x.
        let out = rect.push_out(Vec2::new(14.0, 10.0), 3.0, Vec2::new(20.0, 10.0)).unwrap();
        assert!(out.abs_diff_eq(Vec2::new(15.0, 10.0), 1e-4), "{out}");
        assert!(!rect.overlaps(out, 3.0));
        // Along its width it reaches 4 either way: a body at z = 17 is clear.
        assert_eq!(rect.push_out(Vec2::new(10.0, 17.0), 3.0, Vec2::ZERO), None);
        assert!(rect.overlaps(Vec2::new(10.0, 16.0), 3.0));
        // From deep inside, out the side it came from.
        let out = rect.push_out(Vec2::new(10.5, 10.0), 3.0, Vec2::new(20.0, 10.0)).unwrap();
        assert!(out.abs_diff_eq(Vec2::new(15.0, 10.0), 1e-4), "{out}");
        let circle = Obstacle::Circle { centre: Vec2::ZERO, radius: 2.0 };
        assert!(circle.push_out(Vec2::new(1.0, 0.0), 1.0, Vec2::ZERO).unwrap().abs_diff_eq(Vec2::new(3.0, 0.0), 1e-5));
    }

    #[test]
    fn obstacles_follow_what_blocks_the_way() {
        let mut app = App::new();
        app.add_plugins(bevy::transform::TransformPlugin)
            .init_resource::<Obstacles>()
            .add_systems(PostUpdate, gather_obstacles.after(TransformSystems::Propagate));
        app.world_mut().spawn((BlocksTheWay(FloorShape::Round(2.5)), Transform::from_xyz(4.0, 0.0, 6.0)));
        let square = FloorShape::Square { half_width: 3.0, half_depth: 1.0 };
        app.world_mut().spawn((BlocksTheWay(square), Transform::from_xyz(-4.0, 0.0, 0.0).with_rotation(Quat::from_rotation_y(0.5))));
        app.update();
        let obstacles = &app.world().resource::<Obstacles>().0;
        assert!(obstacles.contains(&Obstacle::Circle { centre: Vec2::new(4.0, 6.0), radius: 2.5 }));
        let turned = |o: &Obstacle| matches!(*o, Obstacle::Rect { centre, half, yaw } if centre == Vec2::new(-4.0, 0.0) && half == Vec2::new(3.0, 1.0) && (yaw - 0.5).abs() < 1e-5);
        assert!(obstacles.iter().any(turned), "{obstacles:?}");
    }

    #[test]
    fn the_delve_plays_only_while_delving() {
        use bevy::state::app::StatesPlugin;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Title)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Keymap>()
            .init_resource::<ScreenOpen>()
            .init_resource::<SheetTab>()
            .add_systems(Update, switch_screens.in_set(DelveSystems));
        run_delve_systems_while_delving(&mut app);
        let escape = |app: &mut App| {
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
            app.update();
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::Escape);
            keys.clear();
        };
        escape(&mut app);
        assert_eq!(app.world().resource::<ScreenOpen>().0, None, "Esc on the title opens no pause menu");
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Delving);
        app.update();
        escape(&mut app);
        assert_eq!(app.world().resource::<ScreenOpen>().0, Some(Screen::Menu));
    }

    fn keys(inventory: bool, character: bool, map: bool, back: bool) -> ScreenKeys {
        ScreenKeys { inventory, character, map, back }
    }

    #[test]
    fn escape_closes_a_screen_before_it_opens_the_menu() {
        use Screen::{Inventory, Menu};
        use SheetTab::Inventory as Grids;
        let screen = |open, k| next_screen(open, Grids, k).0;
        assert_eq!(screen(Some(Inventory), keys(false, false, false, true)), None);
        assert_eq!(screen(None, keys(false, false, false, true)), Some(Menu));
        assert_eq!(screen(Some(Menu), keys(false, false, false, true)), None);
        assert_eq!(screen(None, keys(true, false, false, false)), Some(Inventory));
        assert_eq!(screen(Some(Inventory), keys(true, false, false, false)), None);
        assert_eq!(screen(Some(Menu), keys(true, false, false, false)), Some(Menu), "no inventory over the menu");
        assert_eq!(screen(Some(Menu), keys(false, true, false, false)), Some(Menu), "no sheet over the menu");
        assert_eq!(screen(None, keys(false, false, false, false)), None);
    }

    #[test]
    fn c_opens_the_character_tab_and_i_the_inventory_tab() {
        use Screen::Inventory;
        use SheetTab::{Character, Inventory as Grids};
        let none = keys(false, false, false, false);
        assert_eq!(next_screen(None, Grids, keys(false, true, false, false)), (Some(Inventory), Character));
        assert_eq!(next_screen(None, Grids, keys(true, false, false, false)), (Some(Inventory), Grids));
        assert_eq!(next_screen(Some(Inventory), Character, keys(true, false, false, false)), (Some(Inventory), Grids), "I swaps to the inventory tab");
        assert_eq!(next_screen(Some(Inventory), Grids, keys(false, true, false, false)), (Some(Inventory), Character), "K swaps to the character tab");
        assert_eq!(next_screen(Some(Inventory), Character, keys(false, true, false, false)), (None, Grids), "K closes its own tab");
        assert_eq!(next_screen(Some(Inventory), Character, keys(false, false, false, true)), (None, Grids), "Esc closes it");
        assert_eq!(next_screen(Some(Inventory), Character, none), (Some(Inventory), Character));
        assert_eq!(next_screen(Some(Screen::Map), Grids, keys(false, true, false, false)), (Some(Inventory), Character));
        assert_eq!(next_screen(Some(Inventory), Character, keys(false, false, true, false)), (Some(Screen::Map), Grids), "the map swaps for the sheet");
    }

    #[test]
    fn m_opens_and_closes_the_map_and_esc_closes_it() {
        use Screen::{Inventory, Map, Menu};
        use SheetTab::Inventory as Grids;
        let screen = |open, k| next_screen(open, Grids, k).0;
        assert_eq!(screen(None, keys(false, false, true, false)), Some(Map));
        assert_eq!(screen(Some(Map), keys(false, false, true, false)), None);
        assert_eq!(screen(Some(Map), keys(false, false, false, true)), None, "Esc closes the map, not opening the menu");
        assert_eq!(screen(Some(Inventory), keys(false, false, true, false)), Some(Map), "the map swaps for the inventory");
        assert_eq!(screen(Some(Map), keys(true, false, false, false)), Some(Inventory));
        assert_eq!(screen(Some(Menu), keys(false, false, true, false)), Some(Menu), "no map over the menu");
    }
}
