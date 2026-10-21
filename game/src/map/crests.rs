//! Faction crests on the map. Each claim on the level that the delver has seen part of is
//! marked at the middle of the part seen: with the faction's crest once the delver has met the
//! faction, and with a plain shield (no colours, no name) until then. The marks sit in a layer
//! under the delver's marker, and keep clear of the stairs.

use bevy::prelude::*;
use fb_faction::{Faction, World};
use fb_level::Level;
use fb_level::sight::Explored;

use super::explore::ExploredLevels;
use super::screen::{MapScreen, MapView, screen_size};
use crate::crest::{ASPECT, crest, plain_field};
use crate::inventory::icon::Icons;
use crate::play::{CurrentLevel, DelveSystems, Screen, ScreenOpen};
use crate::record::WorldRecord;

/// The crest's width on the map in screen pixels to a cell of the plan, and its nearest and
/// furthest width at a 1280 by 720 window. Our own: big enough to read when the whole level is
/// in view, small enough to stay on its claim when zoomed in.
pub const WIDTH_CELLS: f32 = 1.6;
pub const WIDTH_PX: (f32, f32) = (22.0, 58.0);
/// How much larger the marks are drawn in a window bigger than 1280 by 720, at most.
pub const MAX_WINDOW_SCALE: f32 = 2.0;
/// The least a crest shrinks to beside a stair, pixels.
pub const MIN_PX: f32 = 12.0;

pub(super) fn add(app: &mut App) {
    app.add_systems(Update, (refresh_crests, place_crests).chain().after(super::screen::build_screen).in_set(DelveSystems));
}

/// The layer the marks sit in, a child of the plan picture drawn beneath the delver's marker.
/// `key` is what the marks were last made from.
#[derive(Component, Debug, Default)]
pub struct CrestLayer {
    pub key: Option<CrestKey>,
}

/// What the marks depend on: the level, how much of it is seen, how many factions are met, and
/// the world they come from.
pub type CrestKey = (u32, usize, usize, u32);

/// One mark on the map: where it stands on the plan's grid, and how far the nearest seen stair
/// is from its middle in cells (infinite if none is seen), so the crest can shrink to clear it.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CrestMark {
    pub at: Vec2,
    pub clearance: f32,
}

/// A claim the delver has seen part of: whose it is, the middle of the cell it stands on, and
/// the distance from there to the nearest seen stair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub faction: Faction,
    pub at: Vec2,
    pub clearance: f32,
}

/// How far from a stair's middle a mark's cell has to be to count as clear of it, in cells.
const CLEAR_CELLS: f32 = 1.5;

/// The marks for the claims of `level` that have any cell seen, in claim order. A mark stands
/// in the middle of the seen cell of the claim nearest the mean of its seen cells, preferring
/// one clear of the seen stairs, so it is always on ground of its own claim, even where the
/// claim is split over rooms and the mean falls in rock.
pub fn claim_marks(level: &Level, seen: &Explored) -> Vec<Mark> {
    let stairs: Vec<Vec2> = level.stairs.iter().filter(|s| seen.is_seen(s.x as i32, s.y as i32)).map(|s| Vec2::new(s.x as f32 + 0.5, s.y as f32 + 0.5)).collect();
    let clearance = |at: Vec2| stairs.iter().map(|s| s.distance(at)).fold(f32::INFINITY, f32::min);
    level
        .claims
        .iter()
        .filter_map(|claim| {
            let mut cells = Vec::new();
            for &room in &claim.rooms {
                let Some(room) = level.rooms.get(room) else { continue };
                for y in room.y..room.y + room.h {
                    for x in room.x..room.x + room.w {
                        if seen.is_seen(x as i32, y as i32) {
                            cells.push(Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
                        }
                    }
                }
            }
            if cells.is_empty() {
                return None;
            }
            let mean = cells.iter().sum::<Vec2>() / cells.len() as f32;
            let nearest = |from: &[Vec2]| from.iter().copied().min_by(|a, b| a.distance_squared(mean).total_cmp(&b.distance_squared(mean)));
            let clear: Vec<Vec2> = cells.iter().copied().filter(|&c| clearance(c) >= CLEAR_CELLS).collect();
            let at = nearest(&clear).or_else(|| nearest(&cells))?;
            Some(Mark { faction: claim.faction, at, clearance: clearance(at) })
        })
        .collect()
}

/// A crest's width in screen pixels at map scale `scale` in a window of `size`.
pub fn crest_width(scale: f32, size: Vec2) -> f32 {
    let window = (size.x / 1280.0).min(size.y / 720.0).clamp(1.0, MAX_WINDOW_SCALE);
    (scale * WIDTH_CELLS).clamp(WIDTH_PX.0, WIDTH_PX.1) * window
}

/// The width a crest of `width` pixels is drawn at, shrunk where a stair `clearance` cells from
/// its middle would otherwise show under it: the crest's reach is kept short of the stair's
/// cell, but never below [`MIN_PX`].
pub fn fit_to_stairs(width: f32, clearance: f32, scale: f32) -> f32 {
    let reach = (clearance - 0.5).max(0.0) * scale;
    // The reach is from the middle to a corner: half the diagonal.
    let largest = reach * 2.0 / 1.0f32.hypot(ASPECT);
    width.min(largest.max(MIN_PX))
}

/// Makes the marks again when what they depend on has changed since they were made.
#[allow(clippy::too_many_arguments)]
fn refresh_crests(
    mut commands: Commands,
    open: Res<ScreenOpen>,
    level: Option<Res<CurrentLevel>>,
    explored: Res<ExploredLevels>,
    record: Option<Res<WorldRecord>>,
    abyss: Option<Res<crate::travel::Abyss>>,
    mut layers: Query<(Entity, &mut CrestLayer)>,
    mut icons: Icons,
) {
    if !open.is(Screen::Map) {
        return;
    }
    let (Some(level), Some(abyss)) = (level, abyss) else { return };
    let Some((layer, mut state)) = layers.iter_mut().next() else { return };
    let seen = explored.get(&level.0);
    let met = |f: Faction| record.as_ref().is_some_and(|r| r.standings.has_met(f));
    let met_here = level.0.claims.iter().filter(|c| met(c.faction)).count();
    let key = (level.0.seed, seen.map_or(0, Explored::count), met_here, abyss.run_seed);
    if state.key == Some(key) {
        return;
    }
    state.key = Some(key);
    let marks = seen.map(|seen| claim_marks(&level.0, seen)).unwrap_or_default();
    let world = World::new(abyss.run_seed);
    commands.entity(layer).despawn_related::<Children>().with_children(|layer| {
        for mark in marks {
            let field = if met(mark.faction) { world.faction(mark.faction).field } else { plain_field() };
            let mut shield = crest(layer, &mut icons, &field, WIDTH_PX.0);
            shield.insert((CrestMark { at: mark.at, clearance: mark.clearance }, Pickable::IGNORE));
            shield.entry::<Node>().and_modify(|mut n| n.position_type = PositionType::Absolute);
        }
    });
}

/// Sizes and lays the marks where the view puts them.
fn place_crests(open: Res<ScreenOpen>, view: Res<MapView>, screens: Query<&ComputedNode, With<MapScreen>>, mut marks: Query<(&CrestMark, &mut Node)>) {
    if !open.is(Screen::Map) {
        return;
    }
    let Some(size) = screens.iter().next().map(screen_size) else { return };
    if size.x <= 0.0 {
        return;
    }
    let width = crest_width(view.scale, size);
    for (mark, mut node) in &mut marks {
        let w = fit_to_stairs(width, mark.clearance, view.scale);
        let h = w * ASPECT;
        let corner = (mark.at + Vec2::splat(super::plan::MARGIN)) * view.scale - Vec2::new(w, h) * 0.5;
        node.left = px(corner.x);
        node.top = px(corner.y);
        node.width = px(w);
        node.height = px(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::CurrentLevel;
    use crate::ui::UiFonts;

    /// A level with at least two claims, and a seen set to start from.
    fn level_with_claims() -> Level {
        (1u32..200)
            .map(|s| fb_level::generate(1207 + s, 2))
            .find(|l| l.claims.len() >= 2)
            .expect("some level has two claims")
    }

    fn room_cells(level: &Level, room: usize) -> Vec<(i32, i32)> {
        let r = level.rooms[room];
        (r.y..r.y + r.h).flat_map(|y| (r.x..r.x + r.w).map(move |x| (x as i32, y as i32))).collect()
    }

    fn see(level: &Level, seen: &mut Explored, cells: &[(i32, i32)]) {
        // Marks cells through the one door the type gives: `look` from each cell's middle.
        for &(x, y) in cells {
            let eye = fb_level::sight::Eye { x: x as f32 + 0.5, y: y as f32 + 0.5, forward: (1.0, 0.0), half_fov: 3.2, range: 0.6 };
            let lamp = [fb_level::sight::Lamp { x: x as f32 + 0.5, y: y as f32 + 0.5, reach: 0.6 }];
            seen.look(level, &eye, &lamp);
        }
    }

    #[test]
    fn an_unexplored_claim_shows_nothing() {
        let level = level_with_claims();
        assert!(claim_marks(&level, &Explored::new(&level)).is_empty());
    }

    /// Whether `at` is the middle of a seen cell in a room of `claim`.
    fn on_seen_ground(level: &Level, claim: &fb_level::Claim, seen: &Explored, at: Vec2) -> bool {
        let (x, y) = (at.x.floor() as i32, at.y.floor() as i32);
        seen.is_seen(x, y) && (at.x - x as f32 - 0.5).abs() < 1e-4 && claim.rooms.iter().any(|&r| level.rooms[r].contains(x, y))
    }

    #[test]
    fn a_mark_is_a_seen_cell_of_its_own_claim() {
        for seed in 1u32..60 {
            let level = fb_level::generate(1207 + seed, 2);
            let all = Explored::all(&level);
            let marks = claim_marks(&level, &all);
            for claim in &level.claims {
                let Some(mark) = marks.iter().find(|m| m.faction == claim.faction) else { continue };
                // Two claims of one faction are told apart by the room they hold.
                let own = level.claims.iter().filter(|c| c.faction == claim.faction).any(|c| on_seen_ground(&level, c, &all, mark.at));
                assert!(own, "seed {seed}: mark {} off its claim", mark.at);
            }
            assert_eq!(marks, claim_marks(&level, &all), "deterministic");
        }
    }

    #[test]
    fn a_claim_split_over_rooms_is_marked_on_a_room_not_in_the_rock_between() {
        let mut level = level_with_claims();
        let corner = |i: usize| Vec2::new(level.rooms[i].x as f32, level.rooms[i].y as f32);
        // One claim over the two rooms furthest apart.
        let (a, b) = (0..level.rooms.len()).flat_map(|a| (0..a).map(move |b| (a, b))).max_by(|&(a, b), &(c, d)| corner(a).distance(corner(b)).total_cmp(&corner(c).distance(corner(d)))).unwrap();
        level.claims[0].rooms = vec![a, b];
        let all = Explored::all(&level);
        let mark = *claim_marks(&level, &all).iter().find(|m| m.faction == level.claims[0].faction).unwrap();
        assert!(on_seen_ground(&level, &level.claims[0], &all, mark.at), "{}", mark.at);
    }

    #[test]
    fn only_the_seen_part_of_a_claim_counts() {
        let level = level_with_claims();
        let claim = &level.claims[0];
        let mut seen = Explored::new(&level);
        let cell = room_cells(&level, claim.rooms[0])[0];
        see(&level, &mut seen, &[cell]);
        let marks = claim_marks(&level, &seen);
        for mark in &marks {
            let owner = level.claims.iter().find(|c| c.faction == mark.faction).unwrap();
            assert!(on_seen_ground(&level, owner, &seen, mark.at), "{}", mark.at);
        }
        for other in &level.claims[1..] {
            if other.rooms.iter().all(|&r| room_cells(&level, r).iter().all(|&(x, y)| !seen.is_seen(x, y))) {
                assert!(marks.iter().all(|m| m.faction != other.faction || other.faction == claim.faction));
            }
        }
    }

    #[test]
    fn a_crest_shrinks_to_keep_off_a_stair_and_never_moves() {
        let scale = 40.0;
        assert_eq!(fit_to_stairs(58.0, f32::INFINITY, scale), 58.0, "no stair, no change");
        let beside = fit_to_stairs(58.0, 1.6, scale);
        assert!(beside < 58.0 && beside >= MIN_PX, "{beside}");
        // Its reach, middle to corner, stays short of the stair's cell.
        assert!(beside * 1.0f32.hypot(ASPECT) * 0.5 <= (1.6 - 0.5) * scale + 1e-3);
        assert_eq!(fit_to_stairs(58.0, 0.0, scale), MIN_PX, "never vanishes");
    }

    /// An app that keeps the marks of `level`, with the map open and a layer to put them in.
    fn marked(level: Level, seen: Explored) -> App {
        let mut app = App::new();
        let mut explored = ExploredLevels::default();
        explored.0.insert(level.seed, seen);
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .init_resource::<UiFonts>()
            .init_resource::<crate::inventory::icon::IconCache>()
            .init_resource::<WorldRecord>()
            .init_resource::<crate::travel::Abyss>()
            .insert_resource(explored)
            .insert_resource(CurrentLevel(level))
            .insert_resource(ScreenOpen(Some(Screen::Map)))
            .add_systems(Update, refresh_crests);
        app.world_mut().spawn(CrestLayer::default());
        app.update();
        app
    }

    /// The marks on the layer: the picture each shows and where it stands.
    fn shown(app: &mut App) -> Vec<(AssetId<Image>, Vec2)> {
        let mut q = app.world_mut().query::<(&ImageNode, &CrestMark)>();
        let mut v: Vec<_> = q.iter(app.world()).map(|(i, m)| (i.image.id(), m.at)).collect();
        v.sort_by(|a, b| a.1.x.total_cmp(&b.1.x).then(a.1.y.total_cmp(&b.1.y)));
        v
    }

    #[test]
    fn an_unmet_claim_shows_a_plain_shield_and_a_met_one_its_crest() {
        let level = level_with_claims();
        let first = level.claims[0].faction;
        let mut app = marked(level.clone(), Explored::all(&level));
        let before = shown(&mut app);
        let plain = before[0].0;
        assert_eq!(before.len(), level.claims.len());
        assert!(before.iter().all(|(i, _)| *i == plain), "one plain shield for every unmet claim");
        // Meeting one faction swaps its marks for its crest, and only its.
        app.world_mut().resource_mut::<WorldRecord>().standings.meet(first);
        app.update();
        let after = shown(&mut app);
        let held_by = |f| level.claims.iter().filter(|c| c.faction == f).count();
        assert_eq!(after.iter().filter(|(i, _)| *i != plain).count(), held_by(first));
        assert_eq!(after.iter().filter(|(i, _)| *i == plain).count(), level.claims.len() - held_by(first));
        assert_eq!(before.iter().map(|m| m.1).collect::<Vec<_>>(), after.iter().map(|m| m.1).collect::<Vec<_>>(), "they stand where they did");
    }

    #[test]
    fn a_faction_holding_two_claims_is_marked_on_each() {
        let mut level = level_with_claims();
        let faction = level.claims[0].faction;
        level.claims[1].faction = faction;
        let mut app = marked(level.clone(), Explored::all(&level));
        let plain = shown(&mut app)[0].0;
        app.world_mut().resource_mut::<WorldRecord>().standings.meet(faction);
        app.update();
        let marks = shown(&mut app);
        let crests = marks.iter().filter(|(i, _)| *i != plain).count();
        assert_eq!(crests, level.claims.iter().filter(|c| c.faction == faction).count());
        assert!(crests >= 2);
        let own: Vec<_> = marks.iter().filter(|(i, _)| *i != plain).map(|m| m.0).collect();
        assert!(own.iter().all(|i| *i == own[0]), "both wear the same arms");
    }

    #[test]
    fn nothing_is_marked_until_a_claim_is_seen_and_the_marks_follow_the_exploring() {
        let level = level_with_claims();
        let mut app = marked(level.clone(), Explored::new(&level));
        assert!(shown(&mut app).is_empty());
        let cell = room_cells(&level, level.claims[0].rooms[0])[0];
        let mut more = Explored::new(&level);
        see(&level, &mut more, &[cell]);
        if more.count() > 0 {
            app.world_mut().resource_mut::<ExploredLevels>().0.insert(level.seed, more);
            app.update();
            assert_eq!(shown(&mut app).len(), 1);
        }
    }

    #[test]
    fn the_crest_scales_with_the_zoom_and_the_window() {
        let small = Vec2::new(1280.0, 720.0);
        let big = Vec2::new(2560.0, 1440.0);
        assert_eq!(crest_width(8.0, small), WIDTH_PX.0, "never smaller than reads");
        assert_eq!(crest_width(72.0, small), WIDTH_PX.1);
        assert!(crest_width(20.0, small) > WIDTH_PX.0);
        assert!((crest_width(40.0, big) - 2.0 * crest_width(40.0, small)).abs() < 1e-3, "a bigger window, bigger crests");
        assert_eq!(crest_width(40.0, Vec2::new(1000.0, 600.0)), crest_width(40.0, small), "no smaller than at 1280 by 720");
    }
}
