//! What each room and corridor is made of: the stone of its walls, how its floor is laid and how
//! damp it is, and a style for every doorway. [`finishes`] is a pure function of a level and
//! its [`LevelParams`], drawn from a stream of its own, so choosing finishes never moves a
//! layout, item, light or creature. The game and the level view draw from it.
//!
//! Rooms are grouped into quarters of two to four neighbours along the corridors, and each quarter
//! has one [`Character`] that fixes its walls, its floor and its doorways together. A corridor
//! cell takes the quarter of the nearest room, and a doorway the style of the room it opens into.
//! A level has one dominant character that every quarter takes, bar at most one quarter (holding
//! a third of the rooms or fewer) that may take a second; the odds of a second character and of a
//! non-plain dominant one grow with depth, and the first level is plain throughout. Moss is the
//! one thing that varies cell by cell, and it is faint: a low-frequency damp field over the
//! level, a rare grotto, and corridors that stay mostly dry. It ignores quarters, and the first
//! level has none.

use procgen_rng::Mulberry32;

use crate::{CELL, Cell, Level, LevelParams, LevelStyle};

/// Salt for the finish stream, apart from the layout's and every other stream's.
const FINISH_SALT: u32 = 0xF1A5_7011;
/// Cells between the lattice points of the damp field: its patches are about this wide.
const DAMP_SPAN: f32 = 8.0;
/// Corridors keep this share of the damp field's moss: mostly dry.
const CORRIDOR_DRYNESS: f32 = 0.35;
/// Rough rock cuts its own arches this share as often as stone halls build theirs.
const ROCK_CUT_SHARE: f64 = 1.0 / 3.0;

/// What a room's or corridor's walls are made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WallFinish {
    /// Craggy cave rock, the rough rock levels' own.
    Rock,
    /// Layered cave rock.
    Strata,
    /// Cut and coursed blocks, the stone halls' own.
    Ashlar,
    /// Rough-coursed rubble.
    Rubble,
}

/// How a floor is laid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloorFinish {
    /// Bare cave floor.
    Rock,
    Flagstone,
    Cobble,
}

/// How a doorway is framed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DoorStyle {
    /// A timber frame.
    Timber,
    RoundArch,
    PointedArch,
    /// An arch cut from the living rock.
    RockCut,
}

/// What a quarter of rooms is like: its walls, floor and doorways, fixed together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Character {
    /// The style's plain look: ashlar and flagstone in the halls, bare rock in the caves; timber
    /// doorways.
    Plain,
    /// A worked, mine-like feel: timber frames over cobbles, with rubble walls in the halls.
    Timbered,
    /// Stone halls: ashlar, flagstones and one shape of arch, round or pointed, with arches
    /// across its corridors.
    Vaulted,
    /// Rough rock: rock walls, rock or flagstone floors and rock-cut arches.
    Hewn,
    /// Rough rock: strata walls and rock floors, with timber frames.
    Layered,
}

impl Character {
    /// Its name, for the level view's legend.
    pub fn name(self) -> &'static str {
        match self {
            Character::Plain => "plain",
            Character::Timbered => "timbered",
            Character::Vaulted => "vaulted",
            Character::Hewn => "hewn",
            Character::Layered => "layered",
        }
    }

    /// The characters a level of `style` draws from.
    pub fn of_style(style: LevelStyle) -> &'static [Character] {
        match style {
            LevelStyle::StoneHalls => &[Character::Plain, Character::Timbered, Character::Vaulted],
            LevelStyle::RoughRock | LevelStyle::CrystalCavern => &[Character::Plain, Character::Timbered, Character::Hewn, Character::Layered],
        }
    }
}

/// A group of neighbouring rooms that share one look.
#[derive(Clone, Debug, PartialEq)]
pub struct Quarter {
    /// Indices into the level's rooms.
    pub rooms: Vec<usize>,
    pub character: Character,
    pub walls: WallFinish,
    pub floor: FloorFinish,
    /// How the doorways of its rooms and corridors are framed.
    pub door: DoorStyle,
}

/// The finish of one open cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Finish {
    pub walls: WallFinish,
    pub floor: FloorFinish,
    /// How mossy the cell is, 0 to 1.
    pub moss: f32,
}

/// The finishes of a level: one per open cell, moss everywhere, and a style per doorway.
#[derive(Clone, Debug, PartialEq)]
pub struct Finishes {
    width: u32,
    height: u32,
    /// Walls and floor per cell, `None` for rock and columns.
    cells: Vec<Option<(WallFinish, FloorFinish)>>,
    /// Moss per cell, rock included so it fades past a wall's foot rather than stopping at it.
    moss: Vec<f32>,
    /// The style of each door cell, `None` elsewhere.
    doors: Vec<Option<DoorStyle>>,
    /// The quarter of each open cell.
    quarter_of: Vec<Option<usize>>,
    quarters: Vec<Quarter>,
    lean: Character,
}

impl Finishes {
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32).then(|| (y as u32 * self.width + x as u32) as usize)
    }

    /// The finish of open cell `(x, y)`; `None` for rock, columns and outside the grid.
    pub fn at(&self, x: i32, y: i32) -> Option<Finish> {
        let i = self.index(x, y)?;
        let (walls, floor) = self.cells[i]?;
        Some(Finish { walls, floor, moss: self.moss[i] })
    }

    /// The style of the doorway in cell `(x, y)`, the entrance included; `None` for any other cell.
    pub fn door_style(&self, x: i32, y: i32) -> Option<DoorStyle> {
        self.doors[self.index(x, y)?]
    }

    /// The quarter of open cell `(x, y)`; `None` for rock, columns, outside the grid and cells no
    /// room reaches.
    pub fn quarter(&self, x: i32, y: i32) -> Option<usize> {
        self.quarter_of[self.index(x, y)?]
    }

    /// The level's quarters, in the order of their first rooms.
    pub fn quarters(&self) -> &[Quarter] {
        &self.quarters
    }

    /// The character of the quarter holding cell `(x, y)`.
    pub fn character_at(&self, x: i32, y: i32) -> Option<Character> {
        self.quarter(x, y).map(|q| self.quarters[q].character)
    }

    /// The character this level favours, rolled before its quarters'.
    pub fn lean(&self) -> Character {
        self.lean
    }

    /// Moss at a world position (x east, z south, units as the level's): the cells' moss
    /// interpolated between their centres, so it fades over a cell or two rather than stopping
    /// at a cell's edge. Outside the grid it is the edge's.
    pub fn moss_at(&self, world_x: f32, world_z: f32) -> f32 {
        let gx = world_x / CELL - 0.5;
        let gy = world_z / CELL - 0.5;
        let (x0, y0) = (gx.floor(), gy.floor());
        let (tx, ty) = (smooth(gx - x0), smooth(gy - y0));
        let at = |x: f32, y: f32| {
            let x = (x as i32).clamp(0, self.width as i32 - 1);
            let y = (y as i32).clamp(0, self.height as i32 - 1);
            self.moss[(y as u32 * self.width + x as u32) as usize]
        };
        let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1.0, y0) * tx;
        let bottom = at(x0, y0 + 1.0) * (1.0 - tx) + at(x0 + 1.0, y0 + 1.0) * tx;
        top * (1.0 - ty) + bottom * ty
    }
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn smoothstep(lo: f32, hi: f32, v: f32) -> f32 {
    smooth(((v - lo) / (hi - lo)).clamp(0.0, 1.0))
}

/// The damp field at grid point `(x, y)` (cells, fractional), 0 to 1: value noise over a
/// lattice [`DAMP_SPAN`] cells apart.
fn damp(seed: u32, x: f32, y: f32) -> f32 {
    let (gx, gy) = (x / DAMP_SPAN, y / DAMP_SPAN);
    let (ix, iy) = (gx.floor(), gy.floor());
    let (tx, ty) = (smooth(gx - ix), smooth(gy - iy));
    let lattice = |dx: i32, dy: i32| {
        let h = seed ^ ((ix as i32 + dx) as u32).wrapping_mul(0x85EB_CA6B) ^ ((iy as i32 + dy) as u32).wrapping_mul(0xC2B2_AE35);
        Mulberry32::new(h).next_f64() as f32
    };
    let top = lattice(0, 0) * (1.0 - tx) + lattice(1, 0) * tx;
    let bottom = lattice(0, 1) * (1.0 - tx) + lattice(1, 1) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// The plain character's own weight against the sliders' for the rest.
const PLAIN_WEIGHT: f64 = 0.6;
/// How much of the sliders' weight each depth below the first brings in: a quarter per depth, so
/// depth 2 is mostly plain and the fifth is at full weight.
const DEPTH_EASE: f64 = 0.25;
/// The chance of a second character at depth 2, how much it grows per depth, and its ceiling.
const SECOND_CHANCE: (f64, f64, f64) = (0.15, 0.1, 0.6);
/// How strongly the damp field grows moss at its fullest: faint, so most rooms have none.
const DAMP_STRENGTH: f32 = 0.6;
/// A grotto's moss: patchy, never solid.
const GROTTO_MOSS: f32 = 0.5;
/// The fewest and most rooms in a quarter.
const QUARTER_SIZE: (u32, u32) = (2, 4);
/// Salt for the quarters' grouping, a stream apart from the finish stream.
const GROUP_SALT: u32 = 0x9A17_E125;

/// Shortest path length, in open cells, from the cells of `from` to every cell; `u32::MAX` where
/// unreachable.
fn distances(level: &Level, from: &[(i32, i32)]) -> Vec<u32> {
    let (w, h) = (level.width as i32, level.height as i32);
    let mut dist = vec![u32::MAX; (w * h) as usize];
    let mut queue = std::collections::VecDeque::new();
    for &(x, y) in from {
        dist[(y * w + x) as usize] = 0;
        queue.push_back((x, y));
    }
    while let Some((x, y)) = queue.pop_front() {
        let d = dist[(y * w + x) as usize];
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let (nx, ny) = (x + dx, y + dy);
            if level.is_open(nx, ny) && dist[(ny * w + nx) as usize] == u32::MAX {
                dist[(ny * w + nx) as usize] = d + 1;
                queue.push_back((nx, ny));
            }
        }
    }
    dist
}

/// The rooms grouped into quarters of two to four that lie near each other by path distance. The
/// start room's quarter comes first; each quarter grows from its first unassigned room by the
/// nearest unassigned room to any of its members. A level of three rooms or fewer is one quarter.
fn group_rooms(level: &Level) -> Vec<Vec<usize>> {
    let n = level.rooms.len();
    if n <= 3 {
        return if n == 0 { Vec::new() } else { vec![(0..n).collect()] };
    }
    let cells_of = |r: usize| {
        let room = level.rooms[r];
        (room.y..room.y + room.h).flat_map(move |y| (room.x..room.x + room.w).map(move |x| (x as i32, y as i32))).collect::<Vec<_>>()
    };
    let w = level.width as i32;
    // dist[a][b]: path length from room a to the nearest cell of room b.
    let dist: Vec<Vec<u32>> = (0..n)
        .map(|a| {
            let d = distances(level, &cells_of(a));
            (0..n).map(|b| cells_of(b).iter().map(|&(x, y)| d[(y * w + x) as usize]).min().unwrap_or(u32::MAX)).collect()
        })
        .collect();
    let mut rng = Mulberry32::new(level.seed ^ level.depth.wrapping_mul(0x9E37_79B9) ^ GROUP_SALT);
    let mut left: Vec<usize> = (0..n).collect();
    if let Some(start) = level.start_room() {
        left.retain(|&r| r != start);
        left.insert(0, start);
    }
    let mut groups = Vec::new();
    while !left.is_empty() {
        let mut size = rng.int(QUARTER_SIZE.0, QUARTER_SIZE.1) as usize;
        if left.len() <= QUARTER_SIZE.1 as usize {
            size = left.len();
        } else if left.len() - size == 1 {
            size = if size > QUARTER_SIZE.0 as usize { size - 1 } else { size + 1 };
        }
        let mut group = vec![left.remove(0)];
        while group.len() < size {
            let next = (0..left.len()).min_by_key(|&i| (group.iter().map(|&m| dist[m][left[i]]).min().unwrap_or(u32::MAX), left[i])).unwrap();
            group.push(left.remove(next));
        }
        group.sort_unstable();
        groups.push(group);
    }
    groups
}

/// The walls, floor and doorway a quarter of `character` has, from its own draws: `floor` varies
/// the floor (Timbered caves, and Hewn ones against `flagstone_chance`) and `arch` picks a
/// vaulted quarter's one arch shape. The plain character is the style's plain, with no draw.
fn dress(character: Character, rock: bool, floor: f64, arch: f64, params: &LevelParams) -> (WallFinish, FloorFinish, DoorStyle) {
    use {DoorStyle as D, FloorFinish as F, WallFinish as W};
    match (rock, character) {
        (false, Character::Timbered) => (W::Rubble, F::Cobble, D::Timber),
        (false, Character::Vaulted) => (W::Ashlar, F::Flagstone, if arch < 0.5 { D::RoundArch } else { D::PointedArch }),
        (false, _) => (W::Ashlar, F::Flagstone, D::Timber),
        (true, Character::Timbered) => (W::Rock, if floor < 0.7 { F::Cobble } else { F::Rock }, D::Timber),
        (true, Character::Hewn) => (W::Rock, if floor < params.flagstone_chance { F::Flagstone } else { F::Rock }, D::RockCut),
        (true, Character::Layered) => (W::Strata, F::Rock, D::Timber),
        (true, _) => (W::Rock, F::Rock, D::Timber),
    }
}

/// The finishes of `level` under `params`. Deterministic: the same level, seed and depth give
/// the same finishes. The draws are a fixed number whatever the odds (four for the level, two per
/// quarter, one per room), so moving a slider changes only what that slider governs.
pub fn finishes(level: &Level, params: &LevelParams) -> Finishes {
    let seed = level.seed ^ level.depth.wrapping_mul(0x9E37_79B9) ^ FINISH_SALT;
    let mut rng = Mulberry32::new(seed);
    let rock = level.style.is_rock();

    // The level has one dominant character and, now and then, one second character on one
    // quarter. Depth 1 is plain whatever the sliders say. The draws are fixed in number: four for
    // the level, two per quarter.
    let characters = Character::of_style(level.style);
    let ease = (level.depth.saturating_sub(1) as f64 * DEPTH_EASE).min(1.0);
    let weight = |c: Character| {
        let slider = match (rock, c) {
            (_, Character::Plain) => return PLAIN_WEIGHT,
            (false, Character::Timbered) => params.rubble_chance,
            (false, _) => params.arch_chance,
            (true, Character::Timbered) => params.cobble_chance,
            (true, Character::Hewn) => params.arch_chance * ROCK_CUT_SHARE,
            (true, _) => params.strata_chance,
        };
        slider * ease
    };
    let (u_lean, u_second, u_pick, u_which) = (rng.next_f64(), rng.next_f64(), rng.next_f64(), rng.next_f64());
    let pick = |u: f64, weights: &[(Character, f64)]| {
        let mut left = u * weights.iter().map(|w| w.1).sum::<f64>();
        weights.iter().find(|w| { left -= w.1; left < 0.0 }).map(|w| w.0)
    };
    let all: Vec<_> = characters.iter().map(|&c| (c, weight(c))).collect();
    let lean = if level.depth <= 1 { Character::Plain } else { pick(u_lean, &all).unwrap_or(Character::Plain) };
    let groups = group_rooms(level);
    // The second character takes one quarter that holds at most a third of the rooms, so the
    // dominant one keeps at least two thirds.
    let small: Vec<usize> = (0..groups.len()).filter(|&q| groups[q].len() * 3 <= level.rooms.len()).collect();
    let others: Vec<_> = all.iter().copied().filter(|w| w.0 != lean).collect();
    let second_chance = (SECOND_CHANCE.0 + SECOND_CHANCE.1 * level.depth.saturating_sub(2) as f64).min(SECOND_CHANCE.2);
    let second = (level.depth > 1 && u_second < second_chance && !small.is_empty())
        .then(|| pick(u_pick, &others))
        .flatten()
        .map(|c| (small[((u_which * small.len() as f64) as usize).min(small.len() - 1)], c));
    let quarters: Vec<Quarter> = groups
        .into_iter()
        .enumerate()
        .map(|(q, rooms)| {
            let (f, a) = (rng.next_f64(), rng.next_f64());
            let character = match second {
                Some((sq, c)) if sq == q => c,
                _ => lean,
            };
            let (walls, floor, door) = dress(character, rock, f, a, params);
            Quarter { rooms, character, walls, floor, door }
        })
        .collect();
    let grottos: Vec<bool> = level.rooms.iter().map(|_| rng.next_f64() < params.grotto_chance).collect();

    let (w, h) = (level.width as i32, level.height as i32);
    let mut quarter_of_room = vec![0usize; level.rooms.len()];
    for (q, quarter) in quarters.iter().enumerate() {
        for &r in &quarter.rooms {
            quarter_of_room[r] = q;
        }
    }
    // Every open cell takes the quarter of the nearest room along the open cells, the first
    // room reached winning a tie.
    let mut quarter_of = vec![None; (w * h) as usize];
    let mut queue = std::collections::VecDeque::new();
    for y in 0..h {
        for x in 0..w {
            if let Some(r) = level.room_at(x, y).filter(|_| level.is_open(x, y)) {
                quarter_of[(y * w + x) as usize] = Some(quarter_of_room[r]);
                queue.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let q = quarter_of[(y * w + x) as usize];
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let (nx, ny) = (x + dx, y + dy);
            if level.is_open(nx, ny) && quarter_of[(ny * w + nx) as usize].is_none() {
                quarter_of[(ny * w + nx) as usize] = q;
                queue.push_back((nx, ny));
            }
        }
    }

    let plain = dress(Character::Plain, rock, 1.0, 1.0, params);
    let mut cells = vec![None; (w * h) as usize];
    let mut moss = vec![0.0f32; (w * h) as usize];
    let mut doors = vec![None; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            if !level.is_open(x, y) {
                continue;
            }
            let i = (y * w + x) as usize;
            let wet = if level.depth <= 1 { 0.0 } else { smoothstep(0.62, 1.0, damp(seed, x as f32 + 0.5, y as f32 + 0.5)) * params.moss_amount * DAMP_STRENGTH };
            let (walls, floor) = quarter_of[i].map_or((plain.0, plain.1), |q| (quarters[q].walls, quarters[q].floor));
            let level_moss = match level.room_at(x, y) {
                Some(r) if grottos[r] && level.depth > 1 => wet.max((GROTTO_MOSS + 0.2 * wet) * params.moss_amount),
                Some(_) => wet,
                None => wet * CORRIDOR_DRYNESS,
            };
            cells[i] = Some((walls, floor));
            moss[i] = level_moss.clamp(0.0, 1.0);
        }
    }
    // Rock and columns take the moss of the open cells beside them, so it runs to a wall's foot
    // and the interpolation between cells does not fade it to nothing there.
    let open_moss = moss.clone();
    for y in 0..h {
        for x in 0..w {
            if level.is_open(x, y) {
                continue;
            }
            let near = (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (x + dx, y + dy)));
            moss[(y * w + x) as usize] = near.filter(|&(nx, ny)| level.is_open(nx, ny)).map(|(nx, ny)| open_moss[(ny * w + nx) as usize]).fold(0.0, f32::max);
        }
    }
    // A doorway takes the style of the room it opens into (the first room beside it, when it
    // joins two); the entrance keeps its timber frame, which its door hangs from.
    let entrance = (level.entrance.x as i32, level.entrance.y as i32);
    for y in 0..h {
        for x in 0..w {
            if level.cell(x, y) != Cell::Door {
                continue;
            }
            let beside = [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().find_map(|&(dx, dy)| level.room_at(x + dx, y + dy)).map(|r| quarter_of_room[r]);
            let q = beside.or(quarter_of[(y * w + x) as usize]);
            doors[(y * w + x) as usize] = Some(if (x, y) == entrance { DoorStyle::Timber } else { q.map_or(plain.2, |q| quarters[q].door) });
        }
    }
    // The entrance's doorway is not a door cell, but it has its timber frame all the same.
    if let Some(entry) = doors.get_mut((entrance.1 * w + entrance.0) as usize) {
        *entry = Some(DoorStyle::Timber);
    }
    Finishes { width: level.width, height: level.height, cells, moss, doors, quarter_of, quarters, lean }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generate_with, LevelStyle};
    use std::collections::HashSet;

    fn level_at(seed: u32, depth: u32) -> (Level, LevelParams) {
        let params = LevelParams::for_depth(depth);
        (generate_with(seed, depth, &params), params)
    }

    /// No chance, no moss, as the style's plain rooms.
    fn plain(depth: u32) -> LevelParams {
        LevelParams {
            moss_amount: 0.0,
            grotto_chance: 0.0,
            strata_chance: 0.0,
            rubble_chance: 0.0,
            cobble_chance: 0.0,
            flagstone_chance: 0.0,
            arch_chance: 0.0,
            ..LevelParams::for_depth(depth)
        }
    }

    #[test]
    fn finishes_are_deterministic() {
        for depth in 1..=2 {
            for seed in 1..=10 {
                let (level, params) = level_at(seed, depth);
                assert_eq!(finishes(&level, &params), finishes(&level, &params));
            }
        }
    }

    #[test]
    fn a_quarters_rooms_share_walls_floor_and_doors() {
        for depth in 1..=2 {
            for seed in 1..=50 {
                let (level, params) = level_at(seed, depth);
                let f = finishes(&level, &params);
                let n = level.rooms.len();
                let mut seen = vec![0; n];
                for (q, quarter) in f.quarters().iter().enumerate() {
                    assert!(n <= 3 || (2..=4).contains(&quarter.rooms.len()), "seed {seed} depth {depth}: {} rooms in a quarter of {n}", quarter.rooms.len());
                    for &r in &quarter.rooms {
                        seen[r] += 1;
                        let room = level.rooms[r];
                        for y in room.y..room.y + room.h {
                            for x in room.x..room.x + room.w {
                                let Some(c) = f.at(x as i32, y as i32) else { continue };
                                assert_eq!((c.walls, c.floor), (quarter.walls, quarter.floor), "seed {seed} depth {depth} room {r}");
                                assert_eq!(f.quarter(x as i32, y as i32), Some(q));
                            }
                        }
                    }
                }
                assert!(seen.iter().all(|&s| s == 1), "every room is in one quarter");
                // Every open cell has a quarter and wears its finish; corridors beside a room
                // wear that room's.
                for y in 0..level.height as i32 {
                    for x in 0..level.width as i32 {
                        if let Some(c) = f.at(x, y) {
                            let q = &f.quarters()[f.quarter(x, y).expect("an open cell has a quarter")];
                            assert_eq!((c.walls, c.floor), (q.walls, q.floor));
                        } else {
                            assert_eq!(f.quarter(x, y), None);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_doorway_matches_a_room_it_opens_into_except_the_entrance() {
        let entrance_at = |l: &Level| (l.entrance.x as i32, l.entrance.y as i32);
        for depth in 1..=2 {
            for seed in 1..=50 {
                let (level, params) = level_at(seed, depth);
                let f = finishes(&level, &params);
                for y in 0..level.height as i32 {
                    for x in 0..level.width as i32 {
                        if level.cell(x, y) != Cell::Door || (x, y) == entrance_at(&level) {
                            continue;
                        }
                        let beside: Vec<DoorStyle> = [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().filter_map(|&(dx, dy)| level.room_at(x + dx, y + dy)).map(|r| f.quarters().iter().find(|q| q.rooms.contains(&r)).unwrap().door).collect();
                        let style = f.door_style(x, y).unwrap();
                        assert!(beside.is_empty() || beside.contains(&style), "seed {seed} depth {depth} door at {x},{y}: {style:?} not in {beside:?}");
                        // A single quarter's doors never mix arch shapes.
                        let q = &f.quarters()[f.quarter(x, y).unwrap()];
                        if q.character == Character::Vaulted && beside.len() == 1 {
                            assert_eq!(style, q.door);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_vaulted_quarter_has_one_arch_shape() {
        let mut vaulted = 0;
        for seed in 1..=50 {
            let (level, params) = level_at(seed, 2);
            let f = finishes(&level, &params);
            for q in f.quarters() {
                if q.character == Character::Vaulted {
                    vaulted += 1;
                    assert!(matches!(q.door, DoorStyle::RoundArch | DoorStyle::PointedArch));
                    assert_eq!((q.walls, q.floor), (WallFinish::Ashlar, FloorFinish::Flagstone));
                } else {
                    assert!(!matches!(q.door, DoorStyle::RoundArch | DoorStyle::PointedArch), "{:?}", q.character);
                }
            }
            // Every arched door in the level belongs to a vaulted quarter, in its one shape.
            for y in 0..level.height as i32 {
                for x in 0..level.width as i32 {
                    if let Some(DoorStyle::RoundArch | DoorStyle::PointedArch) = f.door_style(x, y) {
                        let q = &f.quarters()[f.quarter(x, y).unwrap()];
                        assert_eq!(q.character, Character::Vaulted, "seed {seed}");
                        assert_eq!(f.door_style(x, y), Some(q.door));
                    }
                }
            }
        }
        assert!(vaulted > 10, "{vaulted} vaulted quarters");
    }

    /// The depths past the first, by style: halls on even depths, caves on odd ones.
    const HALL_DEPTHS: [u32; 4] = [2, 4, 6, 8];
    const CAVE_DEPTHS: [u32; 3] = [3, 5, 7];

    #[test]
    fn every_character_appears_in_its_style() {
        for (depths, style) in [(&HALL_DEPTHS[..], LevelStyle::StoneHalls), (&CAVE_DEPTHS[..], LevelStyle::RoughRock)] {
            let mut seen = HashSet::new();
            for &depth in depths {
                for seed in 1..=60 {
                    let (level, params) = level_at(seed, depth);
                    seen.extend(finishes(&level, &params).quarters().iter().map(|q| q.character));
                }
            }
            assert_eq!(seen, Character::of_style(style).iter().copied().collect(), "{style:?}");
        }
    }

    #[test]
    fn the_first_level_is_plain_and_dry_on_every_seed_whatever_the_sliders() {
        let loud = |p: LevelParams| LevelParams { moss_amount: 1.0, grotto_chance: 1.0, strata_chance: 1.0, cobble_chance: 1.0, flagstone_chance: 1.0, arch_chance: 1.0, ..p };
        for seed in 1..=100 {
            let (level, defaults) = level_at(seed, 1);
            for params in [defaults, loud(defaults)] {
                let f = finishes(&level, &params);
                assert!(f.quarters().iter().all(|q| (q.character, q.walls, q.floor, q.door) == (Character::Plain, WallFinish::Rock, FloorFinish::Rock, DoorStyle::Timber)), "seed {seed}");
                for y in 0..level.height as i32 {
                    for x in 0..level.width as i32 {
                        assert_eq!(f.moss_at((x as f32 + 0.5) * CELL, (y as f32 + 0.5) * CELL), 0.0, "seed {seed}");
                        if let Some(c) = f.at(x, y) {
                            assert_eq!((c.walls, c.floor, c.moss), (WallFinish::Rock, FloorFinish::Rock, 0.0));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_level_has_at_most_two_characters_and_the_dominant_one_holds_two_thirds() {
        let (mut second, mut non_plain) = ([0u32; 9], [0u32; 9]);
        for depth in 2..=8 {
            for seed in 1..=100 {
                let (level, params) = level_at(seed, depth);
                let f = finishes(&level, &params);
                let mut rooms: Vec<(Character, usize)> = Vec::new();
                for q in f.quarters() {
                    match rooms.iter_mut().find(|r| r.0 == q.character) {
                        Some(r) => r.1 += q.rooms.len(),
                        None => rooms.push((q.character, q.rooms.len())),
                    }
                }
                assert!(rooms.len() <= 2, "seed {seed} depth {depth}: {rooms:?}");
                let total: usize = rooms.iter().map(|r| r.1).sum();
                let most = rooms.iter().map(|r| r.1).max().unwrap_or(0);
                assert!(most * 3 >= total * 2, "seed {seed} depth {depth}: {rooms:?}");
                second[depth as usize] += (rooms.len() == 2) as u32;
                non_plain[depth as usize] += (f.lean() != Character::Plain) as u32;
            }
        }
        // Variety grows with depth and plain leads at the shallow end.
        assert!(second[2] < second[8], "second characters: {second:?}");
        assert!(non_plain[2] < non_plain[8], "non-plain dominants: {non_plain:?}");
        assert!(non_plain[2] < 50 && non_plain[3] < 60, "plain should lead early: {non_plain:?}");
        assert!(second[8] > 0 && second[2] > 0, "second characters: {second:?}");
    }

    #[test]
    fn corridors_take_the_nearest_rooms_quarter() {
        for depth in 1..=2 {
            for seed in 1..=40 {
                let (level, params) = level_at(seed, depth);
                let f = finishes(&level, &params);
                // Rooms' cells are their quarter's; every other open cell is in the quarter of
                // some room at the least path distance from it.
                let w = level.width as i32;
                let from_room: Vec<Vec<u32>> = level
                    .rooms
                    .iter()
                    .map(|room| {
                        let cells: Vec<_> = (room.y..room.y + room.h).flat_map(|y| (room.x..room.x + room.w).map(move |x| (x as i32, y as i32))).filter(|&(x, y)| level.is_open(x, y)).collect();
                        distances(&level, &cells)
                    })
                    .collect();
                let quarter_of_room = |r: usize| f.quarters().iter().position(|q| q.rooms.contains(&r));
                for y in 0..level.height as i32 {
                    for x in 0..w {
                        if !level.is_open(x, y) {
                            continue;
                        }
                        let i = (y * w + x) as usize;
                        if let Some(r) = level.room_at(x, y) {
                            assert_eq!(f.quarter(x, y), quarter_of_room(r));
                            continue;
                        }
                        let least = from_room.iter().map(|d| d[i]).min().unwrap();
                        let nearest: Vec<_> = (0..level.rooms.len()).filter(|&r| from_room[r][i] == least).map(quarter_of_room).collect();
                        assert!(nearest.contains(&f.quarter(x, y)), "seed {seed} depth {depth}: corridor cell {x},{y} is in {:?}, nearest rooms are in {nearest:?}", f.quarter(x, y));
                    }
                }
            }
        }
    }

    #[test]
    fn every_style_shows_each_of_its_finishes_and_moss_gathers() {
        for (depths, rock) in [(&HALL_DEPTHS[..], false), (&CAVE_DEPTHS[..], true)] {
            let mut walls = HashSet::new();
            let mut floors = HashSet::new();
            let mut doors = HashSet::new();
            let (mut mossy, mut corridor, mut dry_corridor) = (0, 0, 0);
            for &depth in depths {
                for seed in 1..=60 {
                    let (level, params) = level_at(seed, depth);
                    let f = finishes(&level, &params);
                    for y in 0..level.height as i32 {
                        for x in 0..level.width as i32 {
                            if let Some(c) = f.at(x, y) {
                                walls.insert(c.walls);
                                floors.insert(c.floor);
                                if c.moss > 0.35 {
                                    mossy += 1;
                                }
                                if level.room_at(x, y).is_none() {
                                    corridor += 1;
                                    dry_corridor += (c.moss < 0.3) as u32;
                                }
                            }
                            doors.extend(f.door_style(x, y));
                        }
                    }
                }
            }
            let (want_walls, want_floors): (&[_], &[_]) = if rock {
                (&[WallFinish::Rock, WallFinish::Strata], &[FloorFinish::Rock, FloorFinish::Flagstone, FloorFinish::Cobble])
            } else {
                (&[WallFinish::Ashlar, WallFinish::Rubble], &[FloorFinish::Flagstone, FloorFinish::Cobble])
            };
            assert_eq!(walls, want_walls.iter().copied().collect(), "rock {rock} walls");
            assert_eq!(floors, want_floors.iter().copied().collect(), "rock {rock} floors");
            assert!(mossy > 0, "rock {rock} has no moss above 0.35");
            assert!(dry_corridor as f32 > 0.9 * corridor as f32, "rock {rock}: {dry_corridor} of {corridor} corridor cells dry");
            let want_doors: HashSet<_> = if rock { [DoorStyle::Timber, DoorStyle::RockCut].into() } else { [DoorStyle::Timber, DoorStyle::RoundArch, DoorStyle::PointedArch].into() };
            assert_eq!(doors, want_doors, "rock {rock} doors");
        }
    }

    #[test]
    fn most_rooms_have_no_noticeable_moss() {
        let (mut rooms, mut mossy) = (0, 0);
        for depth in 2..=8 {
            for seed in 1..=40 {
                let (level, params) = level_at(seed, depth);
                let f = finishes(&level, &params);
                for room in &level.rooms {
                    let (cx, cy) = room.centre();
                    rooms += 1;
                    mossy += (f.moss_at(cx * CELL, cy * CELL) > 0.2) as u32;
                }
            }
        }
        assert!((mossy as f32) < 0.2 * rooms as f32, "{mossy} of {rooms} room centres are mossy");
    }

    #[test]
    fn the_entrance_is_always_timber() {
        for depth in 1..=4 {
            for seed in 1..=50 {
                let (level, params) = level_at(seed, depth);
                let (x, y) = (level.entrance.x as i32, level.entrance.y as i32);
                assert_eq!(finishes(&level, &params).door_style(x, y), Some(DoorStyle::Timber), "seed {seed} depth {depth}");
                // Even with every door an arch.
                let arched = LevelParams { arch_chance: 1.0, ..params };
                assert_eq!(finishes(&level, &arched).door_style(x, y), Some(DoorStyle::Timber));
            }
        }
    }

    #[test]
    fn with_every_chance_at_zero_rooms_are_plain() {
        for depth in 1..=2 {
            for seed in 1..=50 {
                let params = plain(depth);
                let level = generate_with(seed, depth, &params);
                let f = finishes(&level, &params);
                let (walls, floor) = if depth == 1 { (WallFinish::Rock, FloorFinish::Rock) } else { (WallFinish::Ashlar, FloorFinish::Flagstone) };
                for y in 0..level.height as i32 {
                    for x in 0..level.width as i32 {
                        if let Some(c) = f.at(x, y) {
                            assert_eq!((c.walls, c.floor, c.moss), (walls, floor, 0.0));
                        }
                        if level.cell(x, y) == Cell::Door || (x, y) == (level.entrance.x as i32, level.entrance.y as i32) {
                            assert_eq!(f.door_style(x, y), Some(DoorStyle::Timber));
                        } else {
                            assert_eq!(f.door_style(x, y), None);
                        }
                        assert_eq!(f.moss_at((x as f32 + 0.5) * CELL, (y as f32 + 0.5) * CELL), 0.0);
                    }
                }
            }
        }
    }

    #[test]
    fn moss_is_smooth_between_cell_centres() {
        let (level, params) = level_at(7, 1);
        let f = finishes(&level, &params);
        // At a cell's centre the value is that cell's; along any line it never jumps.
        let step = CELL / 16.0;
        let mut largest = 0.0f32;
        for y in 0..level.height as i32 {
            for x in 0..level.width as i32 - 1 {
                let wz = (y as f32 + 0.5) * CELL;
                let mut last = f.moss_at(x as f32 * CELL + CELL / 2.0, wz);
                for k in 1..=16 {
                    let v = f.moss_at(x as f32 * CELL + CELL / 2.0 + k as f32 * step, wz);
                    largest = largest.max((v - last).abs());
                    last = v;
                }
            }
        }
        assert!(largest < 0.2, "moss jumps by {largest} within a sixteenth of a cell");
        let c = f.at(level.start().0 as i32, level.start().1 as i32).unwrap();
        let (sx, sy) = (level.start().0 as f32, level.start().1 as f32);
        assert!((f.moss_at((sx + 0.5) * CELL, (sy + 0.5) * CELL) - c.moss).abs() < 1e-5);
    }

    #[test]
    fn a_grotto_is_mossy() {
        let params = LevelParams { grotto_chance: 1.0, ..LevelParams::for_style(LevelStyle::RoughRock) };
        let level = generate_with(3, 3, &params);
        let f = finishes(&level, &params);
        let room = level.rooms[0];
        let (cx, cy) = room.centre();
        assert!(f.at(cx as i32, cy as i32).unwrap().moss >= GROTTO_MOSS);
        assert!(f.moss_at(cx * CELL, cy * CELL) >= GROTTO_MOSS);
    }

    #[test]
    fn old_params_without_the_finish_section_still_load() {
        let mut value = serde_json::to_value(LevelParams::default()).unwrap();
        for name in ["moss_amount", "grotto_chance", "strata_chance", "rubble_chance", "cobble_chance", "flagstone_chance", "arch_chance"] {
            value.as_object_mut().unwrap().remove(name);
        }
        let old: LevelParams = serde_json::from_value(value).unwrap();
        assert_eq!(old, LevelParams::default());
    }
}
