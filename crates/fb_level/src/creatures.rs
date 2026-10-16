//! Creatures met in the level: rats in packs, and snakes alone in quiet rooms
//! ([`fb_creatures`] builds each from its species and seed).
//!
//! Creatures come from a fifth stream of the level seed, after the layout, the furnishing, the
//! folk and the props, so adding or changing creatures never moves a wall, an item, a light,
//! anyone standing about or a prop. They keep out of the starting room, doorways (and the cells
//! beside them) and corridors, clear of every item, light, folk and prop.
//!
//! How many, of what and where is our own proposal.

use fb_creatures::{Creature, Species};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::folk::by_door;
use crate::generate::{item_seed, shuffle};
use crate::{CELL, Cell, Level, Room};

/// A creature in the level, at a point in grid coordinates (cells, fractional).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureSpot {
    pub x: f32,
    pub y: f32,
    /// The way it faces, radians about +Y, as [`crate::FolkSpot::yaw`]: 0 faces north (-y).
    pub yaw: f32,
    pub species: Species,
    /// Rolls the creature ([`Creature::of`]).
    pub seed: u32,
    /// Which pack it belongs to: creatures placed together share it.
    pub pack: u32,
}

impl CreatureSpot {
    /// The creature standing here.
    pub fn creature(&self) -> Creature {
        Creature::of(self.species, self.seed)
    }
}

/// Salt for the creature stream.
pub(crate) const CREATURE_SALT: u32 = 0xC4EA_7E25;
/// Rat packs in a level, inclusive, and rats in a pack.
pub const RAT_PACKS: (u32, u32) = (1, 3);
pub const PACK_SIZE: (u32, u32) = (2, 5);
/// Snakes in a level, inclusive, each alone in a room with no folk.
pub const SNAKES: (u32, u32) = (0, 2);
/// Chance that a pack is of giant rats, and that a snake is a constrictor.
const GIANT_PACK: f64 = 0.25;
const CONSTRICTOR: f64 = 0.35;
/// Smallest room a pack or a snake takes.
const MIN_ROOM_AREA: u32 = 9;
/// How far a pack spreads round its centre, in cells.
const PACK_SPREAD: f32 = 0.7;
/// Least gap, in world units, between a creature's edge and a wall, a prop's edge or another
/// creature's edge; and from its edge to an item, a light or a folk's centre.
const WALL_GAP: f32 = 2.5;
const GAP: f32 = 1.0;
const CLEARANCE: f32 = 6.0;

/// Lays out the level's creatures: a few rat packs, each in a room of its own, and maybe a
/// snake or two, each alone in a room nobody stands in. Never in the starting room.
pub(crate) fn place_creatures(level: &Level, rng: &mut Mulberry32) -> Vec<CreatureSpot> {
    let start_room = level.start_room();
    let mut rooms: Vec<usize> = (0..level.rooms.len()).filter(|&i| Some(i) != start_room && level.rooms[i].area() >= MIN_ROOM_AREA).collect();
    shuffle(&mut rooms, rng);
    let folk_in = |i: usize| level.folk.iter().any(|f| level.room_at(f.x as i32, f.y as i32) == Some(i));
    // Snakes first, in the quiet rooms; rats in what is left, quiet rooms first.
    let (quiet, busy): (Vec<usize>, Vec<usize>) = rooms.iter().partition(|&&i| !folk_in(i));
    let snakes = rng.int(SNAKES.0, SNAKES.1) as usize;
    let packs = rng.int(RAT_PACKS.0, RAT_PACKS.1) as usize;
    let mut placed: Vec<CreatureSpot> = Vec::new();
    let mut pack = 0;
    let mut quiet = quiet.into_iter();
    for _ in 0..snakes {
        let Some(i) = quiet.next() else { break };
        let species = if rng.chance(CONSTRICTOR) { Species::ConstrictorSnake } else { Species::PoisonousSnake };
        let room = &level.rooms[i];
        let centre = room.centre();
        if let Some(spot) = settle(level, room, centre, 0.0, species, pack, &placed, rng) {
            placed.push(spot);
            pack += 1;
        }
    }
    let rat_rooms: Vec<usize> = quiet.chain(busy).collect();
    for &i in rat_rooms.iter().take(packs) {
        let room = &level.rooms[i];
        let species = if rng.chance(GIANT_PACK) { Species::GiantRat } else { Species::Rat };
        let size = rng.int(PACK_SIZE.0, PACK_SIZE.1);
        let centre = (rng.range_f32(room.x as f32 + 1.0, (room.x + room.w) as f32 - 1.0), rng.range_f32(room.y as f32 + 1.0, (room.y + room.h) as f32 - 1.0));
        let mut members = 0;
        for _ in 0..size {
            if let Some(spot) = settle(level, room, centre, PACK_SPREAD, species, pack, &placed, rng) {
                placed.push(spot);
                members += 1;
            }
        }
        if members > 0 {
            pack += 1;
        }
    }
    placed
}

/// A creature of `species` near `centre` (within `spread` cells) in `room`, where it fits, or
/// `None` after a few tries.
#[allow(clippy::too_many_arguments)]
fn settle(level: &Level, room: &Room, centre: (f32, f32), spread: f32, species: Species, pack: u32, placed: &[CreatureSpot], rng: &mut Mulberry32) -> Option<CreatureSpot> {
    let seed = item_seed(rng);
    let radius = Creature::of(species, seed).radius();
    for _ in 0..24 {
        let angle = rng.range_f32(0.0, std::f32::consts::TAU);
        let r = spread * rng.range_f32(0.0, 1.0).sqrt();
        let (x, y) = (centre.0 + angle.cos() * r, centre.1 + angle.sin() * r);
        if fits(level, room, x, y, radius, placed) {
            return Some(CreatureSpot { x, y, yaw: rng.range_f32(-std::f32::consts::PI, std::f32::consts::PI), species, seed, pack });
        }
    }
    None
}

/// Whether a creature of `radius` (world units) can stand at `(x, y)` in `room`: on its floor
/// and off its walls, not by a doorway, clear of items, lights, folk, props and `placed`.
pub(crate) fn fits(level: &Level, room: &Room, x: f32, y: f32, radius: f32, placed: &[CreatureSpot]) -> bool {
    let margin = (radius + WALL_GAP) / CELL;
    let inside = x >= room.x as f32 + margin && y >= room.y as f32 + margin && x <= (room.x + room.w) as f32 - margin && y <= (room.y + room.h) as f32 - margin;
    if !inside {
        return false;
    }
    let (cx, cy) = (x.floor() as i32, y.floor() as i32);
    if level.cell(cx, cy) != Cell::Floor || by_door(level, cx, cy) {
        return false;
    }
    let apart = |px: f32, py: f32| (px - x).hypot(py - y) * CELL;
    !level.items.iter().any(|i| apart(i.x, i.y) < radius + CLEARANCE)
        && !level.lights.iter().any(|l| apart(l.x, l.y) < radius + CLEARANCE)
        && !level.folk.iter().any(|f| apart(f.x, f.y) < radius + CLEARANCE)
        && !level.props.iter().any(|p| apart(p.x, p.y) < radius + p.prop.floor_shape().radius() + GAP)
        && !placed.iter().any(|c| apart(c.x, c.y) < radius + c.creature().radius() + GAP)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate;
    use crate::generate::reachable_from;

    const SEEDS: std::ops::RangeInclusive<u32> = 1..=64;

    #[test]
    fn creatures_are_deterministic() {
        for seed in [1, 1207, u32::MAX] {
            assert_eq!(generate(seed, 1).creatures, generate(seed, 1).creatures);
        }
        assert_ne!(generate(1, 1).creatures, generate(2, 1).creatures);
    }

    #[test]
    fn rats_come_in_packs_and_snakes_alone_in_quiet_rooms() {
        let mut snakes = 0;
        for seed in SEEDS {
            let level = generate(seed, 1);
            let start_room = level.start_room();
            let mut packs: std::collections::HashMap<u32, Vec<&CreatureSpot>> = Default::default();
            for c in &level.creatures {
                packs.entry(c.pack).or_default().push(c);
                let (x, y) = (c.x.floor() as i32, c.y.floor() as i32);
                let room = level.room_at(x, y);
                assert!(room.is_some(), "seed {seed}: {c:?} is in a corridor");
                assert_ne!(room, start_room, "seed {seed}: {c:?} is in the starting room");
                assert_eq!(level.cell(x, y), Cell::Floor, "seed {seed}: {c:?}");
                assert!(!by_door(&level, x, y), "seed {seed}: {c:?} is by a doorway");
            }
            for members in packs.values() {
                let species = members[0].species;
                assert!(members.iter().all(|m| m.species == species), "seed {seed}: a mixed pack");
                let room = level.room_at(members[0].x as i32, members[0].y as i32);
                assert!(members.iter().all(|m| level.room_at(m.x as i32, m.y as i32) == room), "seed {seed}: a pack across rooms");
                if species.is_snake() {
                    snakes += 1;
                    assert_eq!(members.len(), 1, "seed {seed}: snakes are alone");
                    assert!(!level.folk.iter().any(|f| level.room_at(f.x as i32, f.y as i32) == room), "seed {seed}: a snake in a room with folk");
                    assert!(level.creatures.iter().filter(|c| level.room_at(c.x as i32, c.y as i32) == room).count() == 1, "seed {seed}: a snake shares its room");
                } else {
                    assert!(members.len() as u32 <= PACK_SIZE.1, "seed {seed}: {} rats", members.len());
                }
            }
            assert!(packs.values().filter(|m| m[0].species.is_rat()).count() >= 1, "seed {seed}: no rats");
        }
        assert!(snakes >= 16, "only {snakes} snakes in {} levels", SEEDS.count());
    }

    #[test]
    fn creatures_keep_clear_of_everything_else() {
        for seed in SEEDS {
            let level = generate(seed, 1);
            for (i, c) in level.creatures.iter().enumerate() {
                let room = level.rooms[level.room_at(c.x as i32, c.y as i32).unwrap()];
                assert!(fits(&level, &room, c.x, c.y, c.creature().radius(), &level.creatures[..i]), "seed {seed}: {c:?}");
            }
        }
    }

    #[test]
    fn creatures_do_not_move_walls_items_lights_folk_or_props() {
        let level = generate(1207, 1);
        let (sx, sy, _) = level.start();
        assert_eq!((sx, sy, level.rooms.len()), (1, 22, 6));
        assert_eq!((level.items.len(), level.folk.len(), level.props.len()), (13, 6, 16));
        assert_eq!(level.folk[5].seed, 748_339_772);
        assert_eq!(level.props[0].prop, crate::Prop::Container { style: armory::families::chests::ChestStyle::Chest, seed: 1_184_162_579, loot: 3_392_196_572 });
        let (before, _) = crate::generate::generate_without_creatures(1207, 1, &crate::LevelParams::default());
        assert!(before.creatures.is_empty() && !level.creatures.is_empty());
        assert_eq!(Level { creatures: Vec::new(), ..level }, before);
    }

    #[test]
    fn rats_on_1207_can_be_reached_from_the_start() {
        let level = generate(1207, 1);
        let (x, y, _) = level.start();
        let seen = reachable_from(&level, x as i32, y as i32);
        let rats: Vec<&CreatureSpot> = level.creatures.iter().filter(|c| c.species.is_rat()).collect();
        assert!(rats.len() >= 2, "{rats:?}");
        for r in rats {
            assert!(seen[(r.y as u32 * level.width + r.x as u32) as usize], "{r:?} is cut off");
        }
    }
}
