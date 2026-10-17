//! Creatures met in the level: rats in packs, and snakes alone in quiet rooms
//! ([`fb_creatures`] builds each from its species and seed).
//!
//! Creatures come from a fifth stream of the level seed, after the layout, the furnishing, the
//! folk and the props, so adding or changing creatures never moves a wall, an item, a light,
//! anyone standing about or a prop. They keep out of the starting room, doorways (and the cells
//! beside them) and corridors, clear of every item, light, folk and prop.
//!
//! Skeletons stand guard in the levels of stone halls and nowhere else: among the columns of the
//! colonnaded halls, each in a gap between two columns facing the aisle, and in a side room or
//! two, facing its door. They are placed last, after every rat and snake, so a level of rough rock
//! draws nothing new from the stream and keeps its creatures as they were.
//!
//! A troll stands alone in a large room on about one level in three from depth
//! [`TROLLS_FROM_DEPTH`]. It is placed after every other creature, from a stream of its own (the
//! level's seed mixed with [`TROLL_SALT`] and the depth), so it draws nothing from the
//! creature stream and no wall, item, light, folk, prop, rat, snake or skeleton moves on any
//! seed.
//!
//! How many, of what and where is our own proposal.

use fb_creatures::{Creature, Species};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::folk::by_door;
use crate::generate::{is_colonnaded, item_seed, shuffle, straight_in_from_a_door};
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
/// The shallowest level giant rats are found on. A pack of giant rats (SRD 5.1 p. 378: AC 12,
/// +4 to hit, 1d4 + 2) is too much for a fresh delver, so the first level has only rats. Our own.
pub const GIANTS_FROM_DEPTH: u32 = 2;
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
/// Skeletons standing in the gaps of each colonnaded hall (up to three halls), inclusive, and
/// the side rooms with a skeleton or a pair, inclusive.
pub const HALL_GUARDS: (u32, u32) = (1, 3);
/// In the first stone halls (depth 2, `fb_items::undead::FIRST_HALLS`), at most two stand in
/// each hall, so a fresh delver meets them a pair at a time (#354). Our own.
pub const FIRST_HALLS_GUARDS: (u32, u32) = (1, 2);
pub const FIRST_HALLS_DEPTH: u32 = 2;
pub const GUARDED_HALLS: usize = 3;
pub const SIDE_GUARDS: (u32, u32) = (1, 2);
/// Chance that a guarded side room holds a pair.
const PAIR: f64 = 0.4;

/// Lays out the level's creatures: a few rat packs, each in a room of its own, and maybe a
/// snake or two, each alone in a room nobody stands in. Never in the starting room.
pub(crate) fn place_creatures(level: &Level, rng: &mut Mulberry32) -> Vec<CreatureSpot> {
    let start_room = level.start_room();
    let mut rooms: Vec<usize> = (0..level.rooms.len()).filter(|&i| Some(i) != start_room && level.rooms[i].area() >= MIN_ROOM_AREA).collect();
    shuffle(&mut rooms, rng);
    if level.style == crate::LevelStyle::StoneHalls {
        // Rats and snakes keep to the side rooms; the grand halls are theirs only when nothing else is left.
        rooms.sort_by_key(|&i| is_colonnaded(level, &level.rooms[i]));
    }
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
    let mut rat_rooms: Vec<usize> = quiet.chain(busy).collect();
    if level.style == crate::LevelStyle::StoneHalls {
        rat_rooms.sort_by_key(|&i| is_colonnaded(level, &level.rooms[i]));
    }
    for &i in rat_rooms.iter().take(packs) {
        let room = &level.rooms[i];
        // The draw is made on every level, so a level's other placements do not shift with depth.
        let giant = rng.chance(GIANT_PACK);
        let species = if giant && level.depth >= GIANTS_FROM_DEPTH { Species::GiantRat } else { Species::Rat };
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
    if level.style == crate::LevelStyle::StoneHalls {
        place_skeletons(level, &mut placed, pack, rng);
    }
    place_trolls(level, &mut placed);
    placed
}

/// The shallowest level a troll is found on. A troll (SRD 5.1 p. 351: challenge 5, 84 hit
/// points, three attacks a turn) is far past a first-level delver. Our own.
pub const TROLLS_FROM_DEPTH: u32 = 6;
/// Chance that a level from [`TROLLS_FROM_DEPTH`] down holds a troll. Our own.
const TROLL_ODDS: f64 = 1.0 / 3.0;
/// Smallest room a troll takes, in cells: a Large creature (SRD 5.1 p. 92, ten feet across)
/// needs a hall to move in. Our own.
pub const TROLL_ROOM_AREA: u32 = 30;
/// Salt for the troll's stream, mixed with the level's seed and depth.
const TROLL_SALT: u32 = 0x7120_C47E;

/// A troll in a large room of `level`, added to `placed` as a pack of its own (one past the
/// highest pack there is), if the level's stream says there is one. Never in the starting room,
/// and it keeps clear of everything else as the other creatures do ([`stands`]).
fn place_trolls(level: &Level, placed: &mut Vec<CreatureSpot>) {
    if level.depth < TROLLS_FROM_DEPTH {
        return;
    }
    let mut rng = Mulberry32::new(level.seed ^ level.depth.wrapping_mul(0x9E37_79B9) ^ TROLL_SALT);
    if !rng.chance(TROLL_ODDS) {
        return;
    }
    let start_room = level.start_room();
    let mut rooms: Vec<usize> = (0..level.rooms.len()).filter(|&i| Some(i) != start_room && level.rooms[i].area() >= TROLL_ROOM_AREA).collect();
    shuffle(&mut rooms, &mut rng);
    let pack = placed.iter().map(|c| c.pack + 1).max().unwrap_or(0);
    let seed = item_seed(&mut rng);
    let radius = Creature::of(Species::Troll, seed).radius();
    for i in rooms {
        let room = &level.rooms[i];
        let centre = room.centre();
        for _ in 0..24 {
            let angle = rng.range_f32(0.0, std::f32::consts::TAU);
            let r = 2.0 * rng.range_f32(0.0, 1.0).sqrt();
            let (x, y) = (centre.0 + angle.cos() * r, centre.1 + angle.sin() * r);
            if stands(level, room, x, y, radius, placed) {
                placed.push(CreatureSpot { x, y, yaw: rng.range_f32(-std::f32::consts::PI, std::f32::consts::PI), species: Species::Troll, seed, pack });
                return;
            }
        }
    }
}

/// The skeletons of a level of stone halls, added to `placed`; their packs count up from `pack`.
/// Each colonnaded hall (up to [`GUARDED_HALLS`] of them, the largest first) has a few in the
/// gaps between its columns, facing the aisle, as one pack; a side room or two holds one, or
/// a pair, facing its door. Never in the starting room, a snake's room, nor on the two cells
/// straight in from a door. A level always has at least one skeleton when it has a room for it.
fn place_skeletons(level: &Level, placed: &mut Vec<CreatureSpot>, mut pack: u32, rng: &mut Mulberry32) {
    let start_room = level.start_room();
    let snake_rooms: Vec<usize> = placed.iter().filter(|c| c.species.is_snake()).filter_map(|c| level.room_at(c.x as i32, c.y as i32)).collect();
    let snake_room = |i: usize| snake_rooms.contains(&i);
    let mut halls: Vec<usize> = (0..level.rooms.len()).filter(|&i| Some(i) != start_room && is_colonnaded(level, &level.rooms[i]) && !snake_room(i)).collect();
    halls.sort_by_key(|&i| std::cmp::Reverse(level.rooms[i].area()));
    halls.truncate(GUARDED_HALLS);
    let mut guarded = 0;
    for &i in &halls {
        let room = &level.rooms[i];
        let mut gaps = column_gaps(level, room);
        shuffle(&mut gaps, rng);
        let (least, most) = if level.depth <= FIRST_HALLS_DEPTH { FIRST_HALLS_GUARDS } else { HALL_GUARDS };
        let want = rng.int(least, most) as usize;
        let mut members = 0;
        for (x, y, aisle) in gaps {
            if members == want {
                break;
            }
            let seed = item_seed(rng);
            let radius = Creature::of(Species::Skeleton, seed).radius();
            if stands(level, room, x, y, radius, placed) {
                placed.push(CreatureSpot { x, y, yaw: yaw_toward(aisle), species: Species::Skeleton, seed, pack });
                members += 1;
            }
        }
        if members > 0 {
            pack += 1;
            guarded += 1;
        }
    }
    let mut sides: Vec<usize> = (0..level.rooms.len()).filter(|&i| Some(i) != start_room && !is_colonnaded(level, &level.rooms[i]) && level.rooms[i].area() >= MIN_ROOM_AREA && !snake_room(i)).collect();
    shuffle(&mut sides, rng);
    let rooms = (rng.int(SIDE_GUARDS.0, SIDE_GUARDS.1) as usize).max(if guarded == 0 { 1 } else { 0 });
    let mut filled = 0;
    for &i in &sides {
        if filled == rooms {
            break;
        }
        let room = &level.rooms[i];
        let size = if rng.chance(PAIR) { 2 } else { 1 };
        let centre = room.centre();
        let mut members = 0;
        for _ in 0..size {
            let seed = item_seed(rng);
            let radius = Creature::of(Species::Skeleton, seed).radius();
            for _ in 0..24 {
                let angle = rng.range_f32(0.0, std::f32::consts::TAU);
                let r = 1.0 * rng.range_f32(0.0, 1.0).sqrt();
                let (x, y) = (centre.0 + angle.cos() * r, centre.1 + angle.sin() * r);
                if stands(level, room, x, y, radius, placed) {
                    let door = nearest_door(level, room, (x, y)).unwrap_or((centre.0, centre.1 - 1.0));
                    placed.push(CreatureSpot { x, y, yaw: yaw_toward((door.0 - x, door.1 - y)), species: Species::Skeleton, seed, pack });
                    members += 1;
                    break;
                }
            }
        }
        if members > 0 {
            pack += 1;
            filled += 1;
        }
    }
}

/// The yaw of facing along `(dx, dy)` in grid coordinates ([`CreatureSpot::yaw`]: 0 faces -y).
fn yaw_toward((dx, dy): (f32, f32)) -> f32 {
    (-dx).atan2(-dy)
}

/// The floor cells of a colonnaded `room` that stand between two columns of a row, each as the
/// cell's centre and the way across to the aisle.
fn column_gaps(level: &Level, room: &Room) -> Vec<(f32, f32, (f32, f32))> {
    let mut gaps = Vec::new();
    for y in room.y..room.y + room.h {
        for x in room.x..room.x + room.w {
            let (cx, cy) = (x as i32, y as i32);
            if level.cell(cx, cy) != Cell::Floor {
                continue;
            }
            let column = |dx: i32, dy: i32| level.cell(cx + dx, cy + dy) == Cell::Column;
            // Between two columns along a row, with the aisle to one side and the wall's ring to the other.
            let across = if column(-1, 0) && column(1, 0) {
                Some([(0, -1), (0, 1)])
            } else if column(0, -1) && column(0, 1) {
                Some([(-1, 0), (1, 0)])
            } else {
                None
            };
            if let Some(sides) = across {
                // The aisle is the side the room's middle lies on.
                let (mx, my) = room.centre();
                let toward = sides.into_iter().min_by(|a, b| {
                    let d = |s: &(i32, i32)| (x as f32 + 0.5 + s.0 as f32 * 2.0 - mx).hypot(y as f32 + 0.5 + s.1 as f32 * 2.0 - my);
                    d(a).total_cmp(&d(b))
                });
                if let Some(s) = toward {
                    gaps.push((x as f32 + 0.5, y as f32 + 0.5, (s.0 as f32, s.1 as f32)));
                }
            }
        }
    }
    gaps
}

/// The point of a door of `room` nearest `from`, in grid coordinates.
fn nearest_door(level: &Level, room: &Room, from: (f32, f32)) -> Option<(f32, f32)> {
    let (x0, y0) = (room.x as i32 - 1, room.y as i32 - 1);
    let (x1, y1) = ((room.x + room.w) as i32, (room.y + room.h) as i32);
    let ring = (x0..=x1).flat_map(|x| [(x, y0), (x, y1)]).chain((y0..=y1).flat_map(|y| [(x0, y), (x1, y)]));
    ring.filter(|&(x, y)| level.cell(x, y) == Cell::Door)
        .map(|(x, y)| (x as f32 + 0.5, y as f32 + 0.5))
        .min_by(|a, b| (a.0 - from.0).hypot(a.1 - from.1).total_cmp(&(b.0 - from.0).hypot(b.1 - from.1)))
}

/// Whether a skeleton of `radius` can stand at `(x, y)` in `room`: [`fits`], and not on the
/// two cells straight in from a door, nor against a column's face.
fn stands(level: &Level, room: &Room, x: f32, y: f32, radius: f32, placed: &[CreatureSpot]) -> bool {
    let (cx, cy) = (x.floor() as i32, y.floor() as i32);
    if straight_in_from_a_door(level, cx, cy) || !fits(level, room, x, y, radius, placed) {
        return false;
    }
    // Clear of every column by the skeleton's radius and a little more.
    let (px, py) = (x * CELL, y * CELL);
    !(-1..=1).any(|dy| {
        (-1..=1).any(|dx| {
            let (nx, ny) = (cx + dx, cy + dy);
            level.cell(nx, ny) == Cell::Column && {
                let (qx, qy) = (px.clamp(nx as f32 * CELL, (nx + 1) as f32 * CELL), py.clamp(ny as f32 * CELL, (ny + 1) as f32 * CELL));
                (px - qx).hypot(py - qy) < radius + GAP
            }
        })
    })
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

    /// Every seed on the first level and on the second, which a placement test holds for alike.
    fn sweep() -> impl Iterator<Item = (u32, u32)> {
        [1, 2].into_iter().flat_map(|depth| SEEDS.map(move |seed| (seed, depth)))
    }

    #[test]
    fn the_first_level_has_no_giant_rats_and_deeper_ones_do() {
        let giants = |depth| (1..=40).flat_map(|seed| generate(seed, depth).creatures).filter(|c| c.species == Species::GiantRat).count();
        assert_eq!(giants(1), 0);
        assert!(giants(GIANTS_FROM_DEPTH) > 0);
    }

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
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
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
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
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
        assert_eq!(Level { creatures: Vec::new(), stairs: Vec::new(), ..level }, before);
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

    fn skeletons(level: &Level) -> Vec<&CreatureSpot> {
        level.creatures.iter().filter(|c| c.species.is_skeleton()).collect()
    }

    #[test]
    fn stone_halls_have_skeletons_reachable_from_the_entrance() {
        let (mut among_columns, mut in_side_rooms) = (0, 0);
        for seed in SEEDS {
            let level = generate(seed, 2);
            let (x, y, _) = level.start();
            let seen = reachable_from(&level, x as i32, y as i32);
            let guards = skeletons(&level);
            assert!(!guards.is_empty(), "seed {seed}: no skeletons in the stone halls");
            for g in guards {
                assert!(seen[(g.y as u32 * level.width + g.x as u32) as usize], "seed {seed}: {g:?} is cut off");
                let room = level.room_at(g.x as i32, g.y as i32).expect("a skeleton stands in a room");
                assert_ne!(Some(room), level.start_room(), "seed {seed}: {g:?} in the starting room");
                if is_colonnaded(&level, &level.rooms[room]) {
                    among_columns += 1;
                } else {
                    in_side_rooms += 1;
                }
            }
        }
        assert!(among_columns > 0 && in_side_rooms > 0, "{among_columns} among columns, {in_side_rooms} in side rooms");
    }

    #[test]
    fn rough_rock_has_no_skeletons_and_the_halls_keep_their_rats_and_snakes() {
        for seed in SEEDS {
            assert!(skeletons(&generate(seed, 1)).is_empty(), "seed {seed}");
        }
        let level = generate(1207, 2);
        assert!(level.creatures.iter().any(|c| !c.species.is_skeleton()));
    }

    #[test]
    fn no_skeleton_stands_on_a_column_a_stair_or_where_a_delver_comes_in() {
        for depth in [2, 4] {
            for seed in SEEDS {
                let level = generate(seed, depth);
                let (sx, sy, _) = level.start();
                for g in skeletons(&level) {
                    let (cx, cy) = (g.x.floor() as i32, g.y.floor() as i32);
                    assert_eq!(level.cell(cx, cy), Cell::Floor, "seed {seed}: {g:?} not on floor");
                    assert!((cx, cy) != (sx as i32, sy as i32), "seed {seed}: {g:?} on the start");
                    assert!(!straight_in_from_a_door(&level, cx, cy), "seed {seed}: {g:?} straight in from a door");
                    assert!(!by_door(&level, cx, cy), "seed {seed}: {g:?} by a door");
                    for stair in &level.stairs {
                        let cells = [(stair.x, stair.y), stair.approach()];
                        assert!(!cells.iter().any(|&(x, y)| (x as i32, y as i32) == (cx, cy)), "seed {seed}: {g:?} on a stair or its approach");
                    }
                }
            }
        }
    }

    #[test]
    fn guards_face_the_aisle_from_between_the_columns() {
        let mut checked = 0;
        for seed in SEEDS {
            let level = generate(seed, 2);
            for g in skeletons(&level) {
                let room = level.rooms[level.room_at(g.x as i32, g.y as i32).unwrap()];
                if !is_colonnaded(&level, &room) {
                    continue;
                }
                let (cx, cy) = (g.x.floor() as i32, g.y.floor() as i32);
                let column = |dx: i32, dy: i32| level.cell(cx + dx, cy + dy) == Cell::Column;
                assert!((column(-1, 0) && column(1, 0)) || (column(0, -1) && column(0, 1)), "seed {seed}: {g:?} is not between two columns");
                // It faces the room's middle, the aisle, rather than the wall behind it.
                let (mx, my) = room.centre();
                let (fx, fy) = (-g.yaw.sin(), -g.yaw.cos());
                assert!((mx - g.x) * fx + (my - g.y) * fy > 0.0 || (mx - g.x).abs().max((my - g.y).abs()) < 1.0, "seed {seed}: {g:?} faces away from the aisle");
                checked += 1;
            }
        }
        assert!(checked >= 20, "only {checked} guards among columns");
    }

    #[test]
    fn skeleton_packs_stay_in_one_room_and_small() {
        for (seed, depth) in SEEDS.flat_map(|s| [(s, 2), (s, 4)]) {
            let level = generate(seed, depth);
            let most = if depth <= FIRST_HALLS_DEPTH { FIRST_HALLS_GUARDS.1 } else { HALL_GUARDS.1 };
            let mut packs: std::collections::HashMap<u32, Vec<&CreatureSpot>> = Default::default();
            for g in skeletons(&level) {
                packs.entry(g.pack).or_default().push(g);
            }
            for members in packs.values() {
                let room = level.room_at(members[0].x as i32, members[0].y as i32);
                assert!(members.iter().all(|m| level.room_at(m.x as i32, m.y as i32) == room), "seed {seed}: a pack across rooms");
                assert!(members.len() <= most as usize, "seed {seed} depth {depth}: {} in a pack", members.len());
            }
            assert!(packs.len() <= GUARDED_HALLS + SIDE_GUARDS.1 as usize, "seed {seed}");
        }
    }
}
