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

/// One mark on the map: where it stands on the plan's grid.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CrestMark {
    pub at: Vec2,
}

/// A claim the delver has seen part of: whose it is, and the middle of the cells seen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub faction: Faction,
    pub at: Vec2,
}

/// The marks for the claims of `level` that have any cell seen, in claim order. A mark stands
/// at the mean of the centres of the seen cells in the claim's rooms.
pub fn claim_marks(level: &Level, seen: &Explored) -> Vec<Mark> {
    level
        .claims
        .iter()
        .filter_map(|claim| {
            let (mut sum, mut n) = (Vec2::ZERO, 0u32);
            for &room in &claim.rooms {
                let Some(room) = level.rooms.get(room) else { continue };
                for y in room.y..room.y + room.h {
                    for x in room.x..room.x + room.w {
                        if seen.is_seen(x as i32, y as i32) {
                            sum += Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                            n += 1;
                        }
                    }
                }
            }
            (n > 0).then(|| Mark { faction: claim.faction, at: sum / n as f32 })
        })
        .collect()
}

/// A crest's width in screen pixels at map scale `scale` in a window of `size`.
pub fn crest_width(scale: f32, size: Vec2) -> f32 {
    let window = (size.x / 1280.0).min(size.y / 720.0).clamp(1.0, MAX_WINDOW_SCALE);
    (scale * WIDTH_CELLS).clamp(WIDTH_PX.0, WIDTH_PX.1) * window
}

/// `at`, moved off any of `stairs` its crest of radius `radius` cells would cover. Each stair
/// is a cell, so a crest clear of it keeps its radius and half a cell from the cell's middle.
pub fn clear_of_stairs(at: Vec2, radius: f32, stairs: &[Vec2]) -> Vec2 {
    let keep = radius + 0.5;
    let mut at = at;
    for _ in 0..4 {
        let mut moved = false;
        for &stair in stairs {
            let away = at - stair;
            let d = away.length();
            if d < keep {
                let dir = if d > 1e-4 { away / d } else { Vec2::NEG_Y };
                at = stair + dir * keep;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    at
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
    let empty = Explored::new(&level.0);
    let seen = explored.get(&level.0).unwrap_or(&empty);
    let met = |f: Faction| record.as_ref().is_some_and(|r| r.standings.has_met(f));
    let met_here = level.0.claims.iter().filter(|c| met(c.faction)).count();
    let key = (level.0.seed, seen.count(), met_here, abyss.run_seed);
    if state.key == Some(key) {
        return;
    }
    state.key = Some(key);
    let world = World::new(abyss.run_seed);
    let marks = claim_marks(&level.0, seen);
    commands.entity(layer).despawn_related::<Children>().with_children(|layer| {
        for mark in marks {
            let field = if met(mark.faction) { world.faction(mark.faction).field } else { plain_field() };
            let mut shield = crest(layer, &mut icons, &field, WIDTH_PX.0);
            shield.insert((CrestMark { at: mark.at }, Pickable::IGNORE));
            shield.entry::<Node>().and_modify(|mut n| n.position_type = PositionType::Absolute);
        }
    });
}

/// Sizes and lays the marks where the view puts them.
fn place_crests(
    open: Res<ScreenOpen>,
    view: Res<MapView>,
    level: Option<Res<CurrentLevel>>,
    explored: Res<ExploredLevels>,
    screens: Query<&ComputedNode, With<MapScreen>>,
    mut marks: Query<(&CrestMark, &mut Node)>,
) {
    if !open.is(Screen::Map) {
        return;
    }
    let (Some(level), Some(size)) = (level, screens.iter().next().map(screen_size)) else { return };
    if size.x <= 0.0 {
        return;
    }
    let empty = Explored::new(&level.0);
    let seen = explored.get(&level.0).unwrap_or(&empty);
    let stairs: Vec<Vec2> = level.0.stairs.iter().filter(|s| seen.is_seen(s.x as i32, s.y as i32)).map(|s| Vec2::new(s.x as f32 + 0.5, s.y as f32 + 0.5)).collect();
    let w = crest_width(view.scale, size);
    let h = w * ASPECT;
    // The shield's reach in cells, from its middle.
    let radius = (w.hypot(h) * 0.5) / view.scale;
    for (mark, mut node) in &mut marks {
        let at = clear_of_stairs(mark.at, radius, &stairs);
        let corner = (at + Vec2::splat(super::plan::MARGIN)) * view.scale - Vec2::new(w, h) * 0.5;
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

    #[test]
    fn a_seen_claim_is_marked_at_the_middle_of_the_seen_cells() {
        let level = level_with_claims();
        let claim = &level.claims[0];
        let all = Explored::all(&level);
        let marks = claim_marks(&level, &all);
        let mark = marks.iter().find(|m| m.faction == claim.faction).unwrap();
        let cells: Vec<(i32, i32)> = claim.rooms.iter().flat_map(|&r| room_cells(&level, r)).filter(|&(x, y)| all.is_seen(x, y)).collect();
        assert!(!cells.is_empty());
        let mean = cells.iter().fold(Vec2::ZERO, |a, &(x, y)| a + Vec2::new(x as f32 + 0.5, y as f32 + 0.5)) / cells.len() as f32;
        assert!(mark.at.abs_diff_eq(mean, 1e-4), "{} vs {mean}", mark.at);
        assert_eq!(marks, claim_marks(&level, &all), "deterministic");
    }

    #[test]
    fn only_the_seen_part_of_a_claim_counts() {
        let level = level_with_claims();
        let claim = &level.claims[0];
        let mut seen = Explored::new(&level);
        let cell = room_cells(&level, claim.rooms[0])[0];
        see(&level, &mut seen, &[cell]);
        let marks = claim_marks(&level, &seen);
        let ours: Vec<_> = marks.iter().filter(|m| m.faction == claim.faction).collect();
        if seen.count() > 0 {
            assert_eq!(ours.len(), 1, "one mark for the claim with a cell seen");
            let at = ours[0].at;
            assert!((at.x - at.x.floor() - 0.5).abs() < 0.5 + 1e-4);
        }
        // The other claims, whose rooms are unseen, are unmarked.
        for other in &level.claims[1..] {
            if other.rooms.iter().all(|&r| room_cells(&level, r).iter().all(|&(x, y)| !seen.is_seen(x, y))) {
                assert!(marks.iter().all(|m| m.faction != other.faction || other.faction == claim.faction));
            }
        }
    }

    #[test]
    fn stairs_are_never_under_a_crest() {
        let stairs = [Vec2::new(10.5, 10.5)];
        let at = clear_of_stairs(Vec2::new(10.7, 10.4), 1.5, &stairs);
        assert!(at.distance(stairs[0]) >= 2.0 - 1e-4, "{at}");
        let far = Vec2::new(30.0, 4.0);
        assert_eq!(clear_of_stairs(far, 1.5, &stairs), far, "a crest well away stays where it is");
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
