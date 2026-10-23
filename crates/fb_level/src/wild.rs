//! The creatures that came after the first five species: giant centipedes, bats and giant bats,
//! giant spiders with their nests, and jackalopes. (Trolls are placed with the rest in
//! [`crate::creatures`].)
//!
//! They are placed last, after the stairs, each species from a stream of its own (the level's
//! seed and depth mixed with the species' salt). Nothing is drawn from the creature stream, so no
//! wall, item, light, folk, prop, stair or earlier creature moves on any seed. Each keeps out of
//! the starting room, any room folk stand in, doorways (and the cells beside them), corridors,
//! the stairs and their approach cells, and clear of everything else ([`crate::creatures::fits`]). A group takes a
//! room no other creature holds, and by preference one that opens onto no occupied room; when no
//! such room is left the group is left off the level (#662). A room holds one creature to every
//! four cells of floor at most.
//!
//! A bat's spot is on the floor grid; the game hangs it from the ceiling. The room where giant
//! spiders are placed is their nest, hung with webs ([`WebSpot`]) drawn from the spiders' own
//! stream after the spiders are placed.
//!
//! How many, of what and where is our own proposal.

use std::f32::consts::{PI, TAU};

use fb_creatures::{Creature, Species};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::creatures::{CreatureSpot, fits, has_folk};
use crate::folk::by_door;
use crate::generate::{is_colonnaded, item_seed, shuffle, straight_in_from_a_door};
use crate::{CELL, Cell, Level, Room};

/// Salts for each species' stream, mixed with the level's seed and depth.
pub const CENTIPEDE_SALT: u32 = 0xCE27_1BED;
pub const BAT_SALT: u32 = 0xBA75_F11E;
pub const SPIDER_SALT: u32 = 0x5B1D_E250;
pub const JACKALOPE_SALT: u32 = 0x4A4C_A10E;

/// Giant centipedes: rooms that hold some, inclusive, and centipedes in each room, inclusive.
/// SRD 5.1 p. 374, challenge 1/4.
pub const CENTIPEDE_ROOMS: (u32, u32) = (1, 2);
/// The shallowest level giant centipedes are found on. A bite with its 3d6 venom (about 14 on a
/// failed save) outdoes a fresh delver's 6 to 12 hit points, so the first level has none, as it
/// has no giant rats ([`crate::GIANTS_FROM_DEPTH`]). Our own.
pub const CENTIPEDES_FROM_DEPTH: u32 = 2;
pub const CENTIPEDES_PER_ROOM: (u32, u32) = (1, 2);
/// Chance a level has a roost of bats, and the bats in a flock, inclusive. Large rooms only.
pub const BAT_ODDS: f64 = 0.5;
pub const FLOCK: (u32, u32) = (3, 8);
/// From this depth giant bats (one or two) take the flock's place.
pub const GIANT_BATS_FROM_DEPTH: u32 = 4;
pub const GIANT_BATS: (u32, u32) = (1, 2);
/// The lowest ceiling, world units, a giant bat flies under: its wings reach about 12 above and
/// below its body, so under the rough rock's 3 m it would clip. Lower levels keep a flock. Our own.
pub const GIANT_BAT_CEILING: f32 = 35.0;
/// Smallest room a flock or a giant bat takes, in cells.
pub const BAT_ROOM_AREA: u32 = 24;
/// The shallowest level with a spider's nest, the chance a level from there on has one, and the
/// chance its spiders are a pair rather than one.
pub const SPIDERS_FROM_DEPTH: u32 = 3;
pub const SPIDER_ODDS: f64 = 0.5;
pub const SPIDER_PAIR: f64 = 0.4;
/// Chance a level holds a jackalope (about one in ten).
pub const JACKALOPE_ODDS: f64 = 0.1;
/// Smallest room any of them takes, in cells.
const MIN_ROOM_AREA: u32 = 9;

/// What a spot of web is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebKind {
    /// A sheet across a corner of the room, a triangle between two walls. Its spot is just
    /// inside the corner, `yaw` points from the corner into the room along the diagonal, `size`
    /// is the length of the sheet's legs along the walls and `height` its top.
    Corner,
    /// A strand from the ceiling slanting down toward a wall. Its spot is the strand's middle,
    /// `yaw` the way it runs, `size` its length.
    Ceiling,
    /// A strand lying across the floor. Its spot is the middle, `yaw` the way it runs, `size`
    /// its length.
    Floor,
}

/// A bit of web in a spider's nest, at a point in grid coordinates (cells, fractional).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WebSpot {
    pub x: f32,
    pub y: f32,
    /// Height above the floor of the spot, world units: a sheet's top, a strand's middle.
    pub height: f32,
    /// Orientation about +Y, radians, as [`CreatureSpot::yaw`].
    pub yaw: f32,
    /// Length in world units (see [`WebKind`]).
    pub size: f32,
    pub kind: WebKind,
}

fn stream(level: &Level, salt: u32) -> Mulberry32 {
    Mulberry32::new(level.seed ^ level.depth.wrapping_mul(0x9E37_79B9) ^ salt)
}

/// Whether a point is clear of every stair and the cell you stand in to use it.
fn off_stairs(level: &Level, x: f32, y: f32, radius: f32) -> bool {
    level.stairs.iter().all(|s| {
        let (ax, ay) = s.approach();
        let near = |cx: u32, cy: u32| (cx as f32 + 0.5 - x).hypot(cy as f32 + 0.5 - y) * CELL < CELL + radius;
        !near(s.x, s.y) && !near(ax, ay)
    })
}

fn standing(level: &Level, room: &Room, x: f32, y: f32, radius: f32, placed: &[CreatureSpot]) -> bool {
    let (cx, cy) = (x.floor() as i32, y.floor() as i32);
    fits(level, room, x, y, radius, placed) && !straight_in_from_a_door(level, cx, cy) && off_stairs(level, x, y, radius)
}

/// For each room, the rooms it opens straight onto: reached from it through doorways and
/// corridors without crossing another room.
pub(crate) fn room_links(level: &Level) -> Vec<Vec<usize>> {
    let (w, h) = (level.width as i32, level.height as i32);
    level
        .rooms
        .iter()
        .enumerate()
        .map(|(i, room)| {
            let mut seen = vec![false; (w * h) as usize];
            let mut queue = std::collections::VecDeque::new();
            for y in room.y..room.y + room.h {
                for x in room.x..room.x + room.w {
                    seen[(y as i32 * w + x as i32) as usize] = true;
                    queue.push_back((x as i32, y as i32));
                }
            }
            let mut links = Vec::new();
            while let Some((x, y)) = queue.pop_front() {
                for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                    if nx < 0 || ny < 0 || nx >= w || ny >= h || seen[(ny * w + nx) as usize] || !level.is_open(nx, ny) {
                        continue;
                    }
                    seen[(ny * w + nx) as usize] = true;
                    match level.room_at(nx, ny) {
                        Some(j) => {
                            if j != i && !links.contains(&j) {
                                links.push(j);
                            }
                        }
                        None => queue.push_back((nx, ny)),
                    }
                }
            }
            links
        })
        .collect()
}

struct Placer<'a> {
    level: &'a Level,
    start_room: Option<usize>,
    /// Every creature so far, the level's own first and then the new ones.
    placed: Vec<CreatureSpot>,
    pack: u32,
    /// Which rooms open straight onto which ([`room_links`]).
    links: Vec<Vec<usize>>,
}

/// The most creatures a room holds, wild ones and the rest together: one to every this many
/// cells of floor.
const CELLS_PER_CREATURE: u32 = 4;

impl Placer<'_> {
    /// Whether any creature stands in room `i`.
    fn held(&self, i: usize) -> bool {
        self.placed.iter().any(|c| self.level.room_at(c.x as i32, c.y as i32) == Some(i))
    }

    /// Whether a room that `i` opens straight onto holds a creature.
    fn next_to_held(&self, i: usize) -> bool {
        self.links[i].iter().any(|&j| self.held(j))
    }

    /// Puts rooms that open onto no occupied room first and colonnaded halls last, keeping the
    /// order otherwise.
    fn prefer_apart(&self, rooms: &mut [usize]) {
        rooms.sort_by_key(|&i| (self.next_to_held(i), is_colonnaded(self.level, &self.level.rooms[i]), self.level.rooms[i].area() >= BAT_ROOM_AREA));
    }

    /// Rooms of at least `area` that are not the starting room and hold no folk and no
    /// creature, shuffled, those opening onto no occupied room first and colonnaded halls last.
    /// A level with none left gets no group rather than a doubled one (#662).
    fn rooms(&self, area: u32, rng: &mut Mulberry32) -> Vec<usize> {
        let level = self.level;
        // Never a room with a stair in it, nor the one a stair opens into: whoever comes down
        // or up arrives clear of anything waiting for them.
        let mut rooms: Vec<usize> = (0..level.rooms.len())
            .filter(|&i| Some(i) != self.start_room && level.rooms[i].area() >= area && !by_stair(level, i) && !has_folk(level, i) && !self.held(i))
            .collect();
        shuffle(&mut rooms, rng);
        self.prefer_apart(&mut rooms);
        rooms
    }

    /// One `species` near `centre` (within `spread` cells) in `room`, where it fits.
    fn settle(&mut self, room: &Room, centre: (f32, f32), spread: f32, species: Species, rng: &mut Mulberry32) -> bool {
        let seed = item_seed(rng);
        let radius = Creature::of(species, seed).radius();
        for _ in 0..24 {
            let angle = rng.range_f32(0.0, TAU);
            let r = spread * rng.range_f32(0.0, 1.0).sqrt();
            let (x, y) = (centre.0 + angle.cos() * r, centre.1 + angle.sin() * r);
            if standing(self.level, room, x, y, radius, &self.placed) {
                let yaw = rng.range_f32(-PI, PI);
                self.placed.push(CreatureSpot { x, y, yaw, species, seed, pack: self.pack });
                return true;
            }
        }
        false
    }

    /// A group of up to `count` of `species` in the first of `rooms` that takes any, as a pack
    /// of its own; the room's index.
    fn group(&mut self, rooms: &[usize], count: u32, spread: f32, species: Species, rng: &mut Mulberry32) -> Option<usize> {
        for &i in rooms {
            let room = &self.level.rooms[i];
            if self.held(i) {
                continue;
            }
            let count = count.min((room.area() / CELLS_PER_CREATURE).max(1));
            let centre = (rng.range_f32(room.x as f32 + 1.0, (room.x + room.w) as f32 - 1.0), rng.range_f32(room.y as f32 + 1.0, (room.y + room.h) as f32 - 1.0));
            let mut members = 0;
            for _ in 0..count {
                if self.settle(room, centre, spread, species, rng) {
                    members += 1;
                }
            }
            if members > 0 {
                self.pack += 1;
                return Some(i);
            }
        }
        None
    }
}

/// Whether room `i` is by a stair: holds one or is the room one opens into. Whoever comes down
/// or up arrives clear of anything waiting for them.
fn by_stair(level: &Level, i: usize) -> bool {
    level.stairs.iter().any(|s| {
        let (ax, ay) = s.approach();
        level.room_at(s.x as i32, s.y as i32) == Some(i) || level.room_at(ax as i32, ay as i32) == Some(i)
    })
}

/// Whether any creature of `level` stands in room `i`.
fn holds_creature(level: &Level, i: usize) -> bool {
    level.creatures.iter().any(|c| level.room_at(c.x as i32, c.y as i32) == Some(i))
}

/// Rooms of at least `area` the wild species may take, held or not.
fn open_rooms(level: &Level, area: u32) -> Vec<usize> {
    let start = level.start_room();
    (0..level.rooms.len()).filter(|&i| Some(i) != start && level.rooms[i].area() >= area && !by_stair(level, i) && !has_folk(level, i)).collect()
}

/// How many wild groups the level's streams ask for, and whether one is a flock that needs a
/// large room. Reads the first draws of each species' stream, as [`place_wild`] does.
fn wanted(level: &Level) -> (usize, bool) {
    let mut groups = 0;
    if level.depth >= SPIDERS_FROM_DEPTH && stream(level, SPIDER_SALT).chance(SPIDER_ODDS) {
        groups += 1;
    }
    if stream(level, JACKALOPE_SALT).chance(JACKALOPE_ODDS) {
        groups += 1;
    }
    let flock = stream(level, BAT_SALT).chance(BAT_ODDS);
    groups += flock as usize;
    if level.depth >= CENTIPEDES_FROM_DEPTH {
        groups += stream(level, CENTIPEDE_SALT).int(CENTIPEDE_ROOMS.0, CENTIPEDE_ROOMS.1) as usize;
    }
    (groups, flock)
}

/// Frees rooms for the wild groups on a crowded level (#662). The rats and snakes took their
/// rooms first and, on a small level, left none, so the wild species would all be dropped. The
/// groups are shared out fairly instead: while there are fewer free rooms than wild groups, the
/// rat pack or snake of whichever kind has the most groups goes, so every kind keeps one group
/// before any gets a second; then a lone snake, then a lone rat pack (not on the first level, which keeps its rats). Skeletons and trolls
/// stay. The pick is by count and then the highest pack, so it is deterministic.
fn make_room_for_wild(level: &mut Level) {
    // A rat pack or snake never shares a room with a skeleton or troll, or with one another.
    for r in 0..level.rooms.len() {
        loop {
            let packs: std::collections::BTreeSet<u32> = level.creatures.iter().filter(|c| level.room_at(c.x as i32, c.y as i32) == Some(r)).map(|c| c.pack).collect();
            if packs.len() < 2 {
                break;
            }
            let groups = |f: fn(Species) -> bool| level.creatures.iter().filter(|c| f(c.species)).map(|c| c.pack).collect::<std::collections::BTreeSet<_>>().len();
            let here = |f: fn(Species) -> bool, spare: bool| {
                let enough = |s: Species| !spare || if s.is_skeleton() { groups(Species::is_skeleton) } else if s.is_rat() { groups(Species::is_rat) } else { groups(Species::is_snake) } > 1;
                level.creatures.iter().filter(|c| f(c.species) && enough(c.species) && level.room_at(c.x as i32, c.y as i32) == Some(r)).map(|c| c.pack).max()
            };
            let beast = |s: Species| s.is_rat() || s.is_snake();
            // A beast that has a fellow elsewhere goes first, then a surplus skeleton pack, then
            // any beast: every kind keeps a group where it can.
            let give_way = here(beast, true).or_else(|| here(Species::is_skeleton, true)).or_else(|| here(beast, false).filter(|_| level.depth > 1));
            let Some(pack) = give_way else { break };
            level.creatures.retain(|c| c.pack != pack);
        }
    }
    let (want, flock) = wanted(level);
    if want == 0 {
        return;
    }
    loop {
        let rooms = open_rooms(level, MIN_ROOM_AREA);
        let free: Vec<usize> = rooms.iter().copied().filter(|&i| !holds_creature(level, i)).collect();
        let short_of_rooms = free.len() < want;
        let short_of_big = flock && !free.iter().any(|&i| level.rooms[i].area() >= BAT_ROOM_AREA);
        if !short_of_rooms && !short_of_big {
            return;
        }
        // Each rat or snake group in a room the wild species could use: (pack, species, room).
        let mut groups: Vec<(u32, Species, usize)> = Vec::new();
        for c in &level.creatures {
            if (c.species.is_snake() || c.species.is_rat()) && !groups.iter().any(|g| g.0 == c.pack) {
                if let Some(r) = level.room_at(c.x as i32, c.y as i32).filter(|r| rooms.contains(r)) {
                    groups.push((c.pack, c.species, r));
                }
            }
        }
        let on_level = |s: Species| level.creatures.iter().filter(|c| c.species.is_rat() == s.is_rat() && (c.species.is_rat() || c.species.is_snake())).map(|c| c.pack).collect::<std::collections::BTreeSet<_>>().len();
        // Surplus first, then a lone snake; a room too small for the flock does not help it.
        let rank = |g: &(u32, Species, usize)| {
            let n = on_level(g.1);
            if n > 1 {
                Some((2, n))
            } else if g.1.is_snake() {
                Some((1, n))
            } else if level.depth > 1 {
                Some((0, n))
            } else {
                None
            }
        };
        let helps = |g: &(u32, Species, usize)| short_of_rooms || level.rooms[g.2].area() >= BAT_ROOM_AREA;
        let Some(&(pack, ..)) = groups.iter().filter(|g| helps(g) && rank(g).is_some()).max_by_key(|g| (rank(g), g.0)) else { return };
        level.creatures.retain(|c| c.pack != pack);
    }
}

/// The newer species of a finished level (stairs and all) as extra creatures, and the webs of
/// its spiders' nest. A crowded level first loses some of its rats and snakes to make room
/// ([`make_room_for_wild`]).
pub(crate) fn place_wild(level: &mut Level) -> (Vec<CreatureSpot>, Vec<WebSpot>) {
    make_room_for_wild(level);
    let level = &*level;
    let mut p = Placer { level, start_room: level.start_room(), placed: level.creatures.clone(), pack: level.creatures.iter().map(|c| c.pack + 1).max().unwrap_or(0), links: room_links(level) };
    let old = p.placed.len();
    let mut webs = Vec::new();

    // Spiders: a nest in a quiet room, from depth 3.
    if level.depth >= SPIDERS_FROM_DEPTH {
        let mut rng = stream(level, SPIDER_SALT);
        if rng.chance(SPIDER_ODDS) {
            let count = if rng.chance(SPIDER_PAIR) { 2 } else { 1 };
            let rooms = p.rooms(MIN_ROOM_AREA, &mut rng);
            let before = p.placed.len();
            if let Some(i) = p.group(&rooms, count, 1.2, Species::GiantSpider, &mut rng) {
                let spiders: Vec<(f32, f32)> = p.placed[before..].iter().map(|c| (c.x, c.y)).collect();
                webs = hang_webs(level, &level.rooms[i], &spiders, &mut rng);
            }
        }
    }
    // Bats in a flock, or giant bats from depth 4 under a high enough ceiling, in a large room.
    let mut rng = stream(level, BAT_SALT);
    if rng.chance(BAT_ODDS) {
        let rooms = p.rooms(BAT_ROOM_AREA, &mut rng);
        if level.depth >= GIANT_BATS_FROM_DEPTH && level.ceiling >= GIANT_BAT_CEILING {
            let count = rng.int(GIANT_BATS.0, GIANT_BATS.1);
            p.group(&rooms, count, 2.0, Species::GiantBat, &mut rng);
        } else {
            let count = rng.int(FLOCK.0, FLOCK.1);
            p.group(&rooms, count, 2.0, Species::Bat, &mut rng);
        }
    }
    // Giant centipedes, one or two in each of a room or two, from depth 2.
    let mut rng = stream(level, CENTIPEDE_SALT);
    let wanted = if level.depth >= CENTIPEDES_FROM_DEPTH { rng.int(CENTIPEDE_ROOMS.0, CENTIPEDE_ROOMS.1) } else { 0 };
    // A jackalope, alone in a quiet room, from depth 1. It is placed before the centipedes so
    // the rarest species is not the one left out on a level short of rooms (#662).
    let mut jackalope = stream(level, JACKALOPE_SALT);
    if jackalope.chance(JACKALOPE_ODDS) {
        let rooms = p.rooms(MIN_ROOM_AREA, &mut jackalope);
        p.group(&rooms, 1, 1.5, Species::Jackalope, &mut jackalope);
    }
    let mut rooms = p.rooms(MIN_ROOM_AREA, &mut rng);
    for _ in 0..wanted {
        let count = rng.int(CENTIPEDES_PER_ROOM.0, CENTIPEDES_PER_ROOM.1);
        if let Some(i) = p.group(&rooms, count, 1.5, Species::GiantCentipede, &mut rng) {
            rooms.retain(|&r| r != i);
            p.prefer_apart(&mut rooms);
        }
    }
    (p.placed.split_off(old), webs)
}

/// Webs for the nest `room`: a sheet in most corners, a few strands from the ceiling and a few
/// across the floor near the `spiders`. None in a doorway, on the cells beside one, on a column
/// or on a stair.
fn hang_webs(level: &Level, room: &Room, spiders: &[(f32, f32)], rng: &mut Mulberry32) -> Vec<WebSpot> {
    let ok = |x: f32, y: f32| {
        let (cx, cy) = (x.floor() as i32, y.floor() as i32);
        room.contains(cx, cy) && level.cell(cx, cy) == Cell::Floor && !by_door(level, cx, cy) && off_stairs(level, x, y, 0.0)
    };
    let mut webs = Vec::new();
    let (x0, y0, x1, y1) = (room.x as f32, room.y as f32, (room.x + room.w) as f32, (room.y + room.h) as f32);
    // Corner sheets, with the diagonal pointing into the room.
    const INSET: f32 = 0.05;
    for (cx, cy, sx, sy) in [(x0, y0, 1.0, 1.0), (x1, y0, -1.0, 1.0), (x0, y1, 1.0, -1.0), (x1, y1, -1.0, -1.0)] {
        let draw = rng.chance(0.75);
        let size = rng.range_f32(6.0, 12.0);
        let height = rng.range_f32(0.55, 0.95) * level.ceiling;
        if draw && ok(cx + sx * INSET, cy + sy * INSET) {
            webs.push(WebSpot { x: cx + sx * INSET, y: cy + sy * INSET, height, yaw: yaw_toward((sx, sy)), size, kind: WebKind::Corner });
        }
    }
    // Strands from the ceiling, slanting toward a wall.
    for _ in 0..rng.int(3, 6) {
        let (x, y) = (rng.range_f32(x0 + 0.5, x1 - 0.5), rng.range_f32(y0 + 0.5, y1 - 0.5));
        let (yaw, size) = (rng.range_f32(-PI, PI), rng.range_f32(8.0, 16.0));
        if ok(x, y) {
            webs.push(WebSpot { x, y, height: level.ceiling * 0.8, yaw, size, kind: WebKind::Ceiling });
        }
    }
    // Strands across the floor, near the spiders.
    for _ in 0..rng.int(1, 3) {
        let (sx, sy) = spiders[rng.int(0, spiders.len() as u32 - 1) as usize];
        let angle = rng.range_f32(0.0, TAU);
        let r = 2.0 * rng.range_f32(0.0, 1.0).sqrt();
        let (x, y) = (sx + angle.cos() * r, sy + angle.sin() * r);
        let (yaw, size) = (rng.range_f32(-PI, PI), rng.range_f32(6.0, 14.0));
        if ok(x, y) && !straight_in_from_a_door(level, x.floor() as i32, y.floor() as i32) {
            webs.push(WebSpot { x, y, height: 0.0, yaw, size, kind: WebKind::Floor });
        }
    }
    webs
}

/// The yaw of facing along `(dx, dy)` in grid coordinates (0 faces -y).
fn yaw_toward((dx, dy): (f32, f32)) -> f32 {
    (-dx).atan2(-dy)
}

#[cfg(test)]
#[path = "wild_tests.rs"]
mod tests;
