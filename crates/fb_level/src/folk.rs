//! Folk met in the level: who stands where, in groups by faction.
//!
//! The factions come from the world's pool (`fb_faction::World`, rolled from the world seed): a
//! level holds the two to four its seed draws ([`crate::Level::factions`]), and each group is
//! one of them. A faction's folk leans its members (an insular one is wholly its own folk).
//!
//! Folk come from a third stream of the level seed, after the layout and the furnishing, so
//! adding or changing folk never moves a wall, an item or a light.

use fb_body::Folk;
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::claims::MIN_ROOM_AREA;
use crate::generate::{is_colonnaded, item_seed, shuffle};
use crate::{Cell, Level, Room};
pub use fb_faction::{Faction, FactionSpec};

/// Someone standing in the level, at a point in grid coordinates (cells, fractional).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FolkSpot {
    pub x: f32,
    pub y: f32,
    /// The way they face, radians about +Y, as [`crate::Facing::yaw`]: 0 faces north (-y), a
    /// quarter turn faces west.
    pub yaw: f32,
    pub folk: Folk,
    /// Rolls their body, name and outfit.
    pub seed: u32,
    pub faction: Faction,
}

impl FolkSpot {
    /// The unit grid direction they face, `(dx, dy)` with `y` south.
    pub fn facing(&self) -> (f32, f32) {
        (-self.yaw.sin(), -self.yaw.cos())
    }
}

/// Salt for the folk stream.
pub(crate) const FOLK_SALT: u32 = 0xF01C_5A17;
/// Folk in a level, inclusive.
pub const FOLK_COUNT: (u32, u32) = (3, 10);
/// Rings a group tries, each [`RING_STEP`] cells wider than the last.
const RINGS: usize = 4;
const RING_STEP: f32 = 0.5;
/// Folk in one group, inclusive.
const GROUP_SIZE: (u32, u32) = (2, 4);
/// Least distance between two folk, in cells (1.4 m).
const FOLK_SPACING: f32 = 0.7;
/// Least distance from a folk to an item or a light, in cells, so nobody stands in a brazier
/// or on the loot.
const CLEARANCE: f32 = 0.55;
/// Least distance from a folk to a room's wall, in cells.
const WALL_MARGIN: f32 = 0.3;

/// Lays out the level's folk: in each faction's claim ([`crate::Claim`]), a few groups gathered
/// round a point in a room (a brazier where the room has one) and facing it, as if talking.
/// `strengths[k]` is how many members claim `k` places. Never in the starting room (no claim
/// holds it), a corridor, a doorway or beside one. A room too cramped for a whole group takes
/// what fits, and later passes top up the groups already placed.
pub(crate) fn place_folk(level: &Level, strengths: &[usize], rng: &mut Mulberry32) -> Vec<FolkSpot> {
    let lit = |i: &usize| level.lights.iter().any(|l| level.room_at(l.x as i32, l.y as i32) == Some(*i));
    let mut folk: Vec<FolkSpot> = Vec::new();
    for (claim, &strength) in level.claims.iter().zip(strengths) {
        let faction = level.faction(claim.faction);
        let mut rooms: Vec<usize> = claim.rooms.iter().copied().filter(|&i| level.rooms[i].area() >= MIN_ROOM_AREA).collect();
        shuffle(&mut rooms, rng);
        // Lit rooms first: people gather at the light.
        if level.style == crate::LevelStyle::StoneHalls {
            // Folk of the halls stand among the columns first.
            rooms.sort_by_key(|i| (!is_colonnaded(level, &level.rooms[*i]), !lit(i)));
        } else {
            rooms.sort_by_key(|i| !lit(i));
        }
        let groups: Vec<(&Room, Gathering)> = rooms.iter().map(|&i| (&level.rooms[i], Gathering::in_room(level, &level.rooms[i], rng))).collect();
        let mut placed = 0;
        for pass in 0..3 {
            for &(room, gathering) in &groups {
                let left = strength.saturating_sub(placed);
                if left == 0 {
                    break;
                }
                let size = if pass == 0 { rng.int(GROUP_SIZE.0, GROUP_SIZE.1) as usize } else { 1 }.min(left);
                let made = gathering.gather(level, room, faction, size, &folk, rng);
                placed += made.len();
                folk.extend(made);
            }
        }
    }
    folk
}

/// Where a group stands: round a centre, at about a radius, starting at an angle.
#[derive(Clone, Copy, Debug)]
struct Gathering {
    centre: (f32, f32),
    radius: f32,
    start: f32,
}

impl Gathering {
    /// Round the room's brazier, if it has one, otherwise round a point well inside it.
    fn in_room(level: &Level, room: &Room, rng: &mut Mulberry32) -> Gathering {
        let brazier = level.lights.iter().find(|l| l.kind == crate::LightKind::Brazier && room.contains(l.x as i32, l.y as i32));
        let centre = match brazier {
            Some(b) => (b.x, b.y),
            None => (rng.range_f32(room.x as f32 + 1.0, (room.x + room.w) as f32 - 1.0), rng.range_f32(room.y as f32 + 1.0, (room.y + room.h) as f32 - 1.0)),
        };
        // Wider round a fire.
        let radius = if brazier.is_some() { rng.range_f32(1.0, 1.3) } else { rng.range_f32(0.55, 0.8) };
        Gathering { centre, radius, start: rng.range_f32(0.0, std::f32::consts::TAU) }
    }

    /// Up to `size` of `faction` round the centre, clear of `others`: evenly round the ring
    /// with a little jitter, then round wider rings for those who did not fit.
    fn gather(&self, level: &Level, room: &Room, faction: &FactionSpec, size: usize, others: &[FolkSpot], rng: &mut Mulberry32) -> Vec<FolkSpot> {
        let (cx, cy) = self.centre;
        let around = size.max(3);
        let mut placed: Vec<FolkSpot> = Vec::new();
        for k in 0..around * RINGS {
            if placed.len() == size {
                break;
            }
            let ring = (k / around) as f32;
            let angle = self.start + k as f32 / around as f32 * std::f32::consts::TAU + ring * 0.5 + rng.range_f32(-0.25, 0.25);
            let r = (self.radius + ring * RING_STEP) * rng.range_f32(0.92, 1.08);
            let (x, y) = (cx + angle.cos() * r, cy + angle.sin() * r);
            if !fits(level, room, x, y, others.iter().chain(&placed)) {
                continue;
            }
            let folk = faction.member_folk(rng);
            // Facing the centre, give or take: yaw 0 faces -y, so the yaw toward `(dx, dy)` is
            // `atan2(-dx, -dy)`.
            let yaw = (x - cx).atan2(y - cy) + rng.range_f32(-0.3, 0.3);
            placed.push(FolkSpot { x, y, yaw, folk, seed: item_seed(rng), faction: faction.id });
        }
        placed
    }
}

/// Whether someone can stand at `(x, y)` in `room`: on its floor, off its walls, not in or
/// beside a doorway, and clear of items, lights and `others`.
fn fits<'a>(level: &Level, room: &Room, x: f32, y: f32, mut others: impl Iterator<Item = &'a FolkSpot>) -> bool {
    let inside = x >= room.x as f32 + WALL_MARGIN
        && y >= room.y as f32 + WALL_MARGIN
        && x <= (room.x + room.w) as f32 - WALL_MARGIN
        && y <= (room.y + room.h) as f32 - WALL_MARGIN;
    if !inside {
        return false;
    }
    let (cx, cy) = (x.floor() as i32, y.floor() as i32);
    if level.cell(cx, cy) != Cell::Floor || by_door(level, cx, cy) {
        return false;
    }
    let near = |px: f32, py: f32, d: f32| (px - x).hypot(py - y) < d;
    !level.items.iter().any(|i| near(i.x, i.y, CLEARANCE))
        && !level.lights.iter().any(|l| near(l.x, l.y, CLEARANCE))
        && !others.any(|f| near(f.x, f.y, FOLK_SPACING))
}

/// Whether cell `(x, y)` is beside a doorway, or diagonally beside one, so nobody stands in
/// the way in or out.
pub(crate) fn by_door(level: &Level, x: i32, y: i32) -> bool {
    (-1..=1).any(|dy| (-1..=1).any(|dx| level.cell(x + dx, y + dy) == Cell::Door))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate;
    use crate::generate::reachable_from;
    use crate::generate_in;
    use std::collections::{HashMap, HashSet};

    const SEEDS: std::ops::RangeInclusive<u32> = 1..=64;

    /// Every seed on the first level and on the second, which a placement test holds for alike.
    fn sweep() -> impl Iterator<Item = (u32, u32)> {
        [1, 2].into_iter().flat_map(|depth| SEEDS.map(move |seed| (seed, depth)))
    }

    #[test]
    fn a_level_holds_two_to_four_of_its_worlds_factions() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let world = fb_faction::World::new(seed);
            let drawn = world.draw_specs(seed);
            assert!((2..=4).contains(&drawn.len()));
            // Those of the draw that found ground, in the draw's order.
            assert!(!level.factions.is_empty() && level.factions.iter().all(|s| drawn.contains(s)), "seed {seed}");
            assert!(level.factions.windows(2).all(|w| drawn.iter().position(|d| *d == w[0]) < drawn.iter().position(|d| *d == w[1])));
            for f in &level.folk {
                assert!(level.factions.iter().any(|s| s.id == f.faction), "seed {seed}: {f:?} is of a faction the level does not hold");
            }
        }
    }

    #[test]
    fn a_world_draws_the_same_spec_on_every_level() {
        let world = 1207;
        let mut seen = HashMap::new();
        for depth in 1..=12 {
            for spec in generate_in(world, depth, &crate::LevelParams::for_style(crate::LevelStyle::for_depth(depth))).factions {
                assert_eq!(*seen.entry(spec.id).or_insert_with(|| spec.clone()), spec);
            }
        }
        assert!(seen.len() > 3, "{} factions in 12 levels", seen.len());
    }

    #[test]
    fn insular_folk_are_all_their_factions_folk_on_every_level() {
        let mut insular = 0;
        for world in 1..=48 {
            for depth in 1..=3 {
                let level = generate_in(world, depth, &crate::LevelParams::for_style(crate::LevelStyle::for_depth(depth)));
                for f in &level.folk {
                    let spec = level.faction(f.faction);
                    if spec.insular {
                        insular += 1;
                        assert_eq!(f.folk, spec.folk, "world {world} depth {depth}: {} has a {:?}", spec.name, f.folk);
                    }
                }
            }
        }
        assert!(insular > 20, "only {insular} folk of insular factions");
    }

    #[test]
    fn folk_are_deterministic() {
        for seed in [1, 1207, u32::MAX] {
            assert_eq!(generate(seed, 1).folk, generate(seed, 1).folk);
        }
        assert_ne!(generate(1, 1).folk, generate(2, 1).folk);
    }

    #[test]
    fn each_level_has_three_to_ten_folk() {
        for seed in SEEDS {
            let n = generate(seed, 1).folk.len() as u32;
            assert!((FOLK_COUNT.0..=FOLK_COUNT.1).contains(&n), "seed {seed}: {n} folk");
        }
    }

    #[test]
    fn nobody_stands_in_the_starting_room_a_corridor_a_doorway_or_rock() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let start_room = level.start_room().unwrap();
            for f in &level.folk {
                let (x, y) = (f.x.floor() as i32, f.y.floor() as i32);
                assert_eq!(level.cell(x, y), Cell::Floor, "seed {seed}: {f:?} is not on floor");
                let room = level.room_at(x, y);
                assert!(room.is_some(), "seed {seed}: {f:?} stands in a corridor");
                assert_ne!(room, Some(start_room), "seed {seed}: {f:?} stands in the starting room");
                assert!(!by_door(&level, x, y), "seed {seed}: {f:?} stands by a doorway");
            }
        }
    }

    #[test]
    fn folk_keep_apart_and_off_the_loot_and_lights() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for (i, a) in level.folk.iter().enumerate() {
                for b in &level.folk[i + 1..] {
                    assert!((a.x - b.x).hypot(a.y - b.y) >= FOLK_SPACING, "seed {seed}: {a:?} and {b:?}");
                }
                let spots = level.items.iter().map(|s| (s.x, s.y)).chain(level.lights.iter().map(|l| (l.x, l.y)));
                for (x, y) in spots {
                    assert!((a.x - x).hypot(a.y - y) >= CLEARANCE, "seed {seed}: {a:?} on ({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn groups_are_one_faction_to_a_room_and_mostly_its_folk() {
        let mut leaning = (0, 0);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let mut by_room = HashMap::new();
            for f in &level.folk {
                let room = level.room_at(f.x as i32, f.y as i32).unwrap();
                assert_eq!(*by_room.entry(room).or_insert(f.faction), f.faction, "seed {seed}: two factions in room {room}");
                leaning.0 += (f.folk == level.faction(f.faction).folk) as u32;
                leaning.1 += 1;
            }
        }
        assert!(leaning.0 * 10 >= leaning.1 * 6, "only {} of {} folk are their faction's folk", leaning.0, leaning.1);
    }

    #[test]
    fn levels_show_more_than_one_faction() {
        let factions: HashSet<Faction> = (1..=32).flat_map(|s| generate(s, 1).folk).map(|f| f.faction).collect();
        assert!(factions.len() >= 2, "{factions:?}");
        let folk: HashSet<Folk> = (1..=32).flat_map(|s| generate(s, 1).folk).map(|f| f.folk).collect();
        assert_eq!(folk.len(), Folk::ALL.len(), "{folk:?}");
    }

    #[test]
    fn folk_on_1207_can_be_reached_from_the_start() {
        let level = generate(1207, 1);
        let (x, y, _) = level.start();
        let seen = reachable_from(&level, x as i32, y as i32);
        assert!(!level.folk.is_empty());
        for f in &level.folk {
            let i = (f.y as u32 * level.width + f.x as u32) as usize;
            assert!(seen[i], "{f:?} is cut off");
        }
    }

    #[test]
    fn folk_do_not_move_the_loot() {
        // Seed 1207's items as they lay before folk came (`print_level 1207`).
        let level = generate(1207, 1);
        assert_eq!(level.items.len(), 13);
        assert_eq!((level.items[0].family, level.items[0].seed), (armory::Family::Torch, 3_185_309_893));
        assert_eq!((level.items[12].family, level.items[12].seed), (armory::Family::Mace, 3_024_996_869));
        assert!((level.items[12].x - 10.6).abs() < 0.05 && (level.items[12].y - 10.5).abs() < 0.05);
    }

    #[test]
    fn facing_follows_the_yaw() {
        let spot = |yaw: f32| FolkSpot { x: 0.0, y: 0.0, yaw, folk: Folk::Human, seed: 0, faction: Faction(0) };
        let (dx, dy) = spot(crate::Facing::North.yaw()).facing();
        assert!(dx.abs() < 1e-6 && (dy + 1.0).abs() < 1e-6);
        let (dx, dy) = spot(crate::Facing::West.yaw()).facing();
        assert!((dx + 1.0).abs() < 1e-6 && dy.abs() < 1e-6);
    }
}

