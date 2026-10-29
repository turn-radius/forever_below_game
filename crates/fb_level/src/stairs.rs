//! The ways between levels: a stair or shaft down on every level and, below the first, a stair
//! up where the level is entered.
//!
//! A stair is a cell of floor that nobody walks onto. It has an approach cell beside it, the
//! one you stand in to use it, and `facing` is the way from the approach cell into the stair.
//! Arriving by a stair puts you in its approach cell turned the other way, away from it.
//!
//! The way down is placed after everything else, on its own salted stream
//! ([`STAIR_SALT`]) and as a pure function of the finished layout, so nothing about a level
//! that existed before stairs changed (the first level's seeds still lay out as they did). It
//! stands in wild ground (never in a claim's room or behind its doors), against a wall of a room, far by path from the start, on a cell nothing else uses,
//! and shutting that cell cuts no part of the level off. Below the first level the entrance
//! door is not a door: its cell is the stair up, and the start cell in front of it is where you
//! arrive.

use std::collections::{HashSet, VecDeque};

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::{Cell, Facing, Level};

/// Salts the stream the way down is placed from, apart from every other stream of a level.
pub const STAIR_SALT: u32 = 0x57A1_25D0;

/// The share of the longest way from the start that a way down must lie beyond, among the cells
/// it could stand on.
pub(crate) const FAR_SHARE: f32 = 0.85;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StairKind {
    /// A stair or shaft into the floor, to the level below.
    Down,
    /// A stair rising into the ceiling, to the level above.
    Up,
}

/// A way between levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stair {
    /// The stair's own cell, which is not walked onto.
    pub x: u32,
    pub y: u32,
    /// The way from its approach cell into it.
    pub facing: Facing,
    pub kind: StairKind,
}

impl Stair {
    /// The cell you stand in to use the stair, and arrive in: the one behind it, against
    /// `facing`.
    pub fn approach(&self) -> (u32, u32) {
        let (dx, dy) = self.facing.step();
        ((self.x as i32 - dx) as u32, (self.y as i32 - dy) as u32)
    }

    /// Where arriving by this stair puts you: its approach cell, turned away from it.
    pub fn arrival(&self) -> (u32, u32, Facing) {
        let (x, y) = self.approach();
        (x, y, self.facing.opposite())
    }
}

/// The seed of the level at `depth` in the run for `run_seed`: the run's own seed at the first
/// depth, so a run keeps naming its first level, and a hash of the two below it. A depth is
/// always the same level.
pub fn depth_seed(run_seed: u32, depth: u32) -> u32 {
    if depth <= 1 {
        return run_seed;
    }
    // A finalizer in the style of splitmix32 over the seed and the depth.
    let mut h = run_seed ^ depth.wrapping_mul(0x9E37_79B9);
    h = (h ^ (h >> 16)).wrapping_mul(0x85EB_CA6B);
    h = (h ^ (h >> 13)).wrapping_mul(0xC2B2_AE35);
    h ^ (h >> 16)
}

/// Path lengths in cells from `from` over open cells, `-1` where there is no way, with `shut` (a
/// cell) counted as rock.
fn distances(level: &Level, from: (i32, i32), shut: Option<(i32, i32)>) -> Vec<i32> {
    let mut dist = vec![-1; level.cells.len()];
    let index = |x: i32, y: i32| (y as u32 * level.width + x as u32) as usize;
    if !level.is_open(from.0, from.1) || shut == Some(from) {
        return dist;
    }
    dist[index(from.0, from.1)] = 0;
    let mut queue = VecDeque::from([from]);
    while let Some((x, y)) = queue.pop_front() {
        for facing in Facing::ALL {
            let (dx, dy) = facing.step();
            let (nx, ny) = (x + dx, y + dy);
            if level.is_open(nx, ny) && shut != Some((nx, ny)) && dist[index(nx, ny)] < 0 {
                dist[index(nx, ny)] = dist[index(x, y)] + 1;
                queue.push_back((nx, ny));
            }
        }
    }
    dist
}

/// The path length in cells from the start to every cell of `level` (row-major, as
/// [`Level::cells`]), `-1` where there is none.
pub fn distances_from_start(level: &Level) -> Vec<i32> {
    let (sx, sy, _) = level.start();
    distances(level, (sx as i32, sy as i32), None)
}

/// Lays the stairs of a finished level: its way down, and below the first level the way up at
/// the entrance. See the module docs for the rules. A level too cramped for a way down (only
/// possible with absurd parameters) gets none.
pub(crate) fn place_stairs(level: &mut Level, rng: &mut Mulberry32) {
    let (sx, sy, _) = level.start();
    let start = (sx as i32, sy as i32);
    let mut taken: HashSet<(i32, i32)> = HashSet::new();
    let mut take = |x: f32, y: f32| {
        taken.insert((x.floor() as i32, y.floor() as i32));
    };
    level.items.iter().for_each(|s| take(s.x, s.y));
    level.lights.iter().for_each(|s| take(s.x, s.y));
    level.folk.iter().for_each(|s| take(s.x, s.y));
    level.props.iter().for_each(|s| take(s.x, s.y));
    level.creatures.iter().for_each(|s| take(s.x, s.y));

    let entrance = (level.entrance.x as i32, level.entrance.y as i32);
    let mut stairs = Vec::new();
    if level.depth >= 2 {
        // The door the player came in by is a way up instead.
        let at = entrance.1 as usize * level.width as usize + entrance.0 as usize;
        level.cells[at] = Cell::Floor;
        stairs.push(Stair { x: level.entrance.x, y: level.entrance.y, facing: level.entrance.inward.opposite(), kind: StairKind::Up });
        taken.insert(entrance);
    }

    // The way down lies in wild ground: not in a claim's rooms or behind its doors.
    let held = crate::claims::held_cells(level, &level.claims);
    let mut dist = distances(level, start, None);
    for (d, &h) in dist.iter_mut().zip(&held) {
        if h {
            *d = -1;
        }
    }
    let index = |x: i32, y: i32| (y as u32 * level.width + x as u32) as usize;
    let total_open = level.cells.iter().filter(|c| c.is_open()).count();
    // A cell can hold the way down when it and its approach are free floor. The tight pool is
    // the cells against a wall of a room, the loose one any at all.
    let (mut tight, mut loose) = (Vec::new(), Vec::new());
    for y in 0..level.height as i32 {
        for x in 0..level.width as i32 {
            if level.cell(x, y) != Cell::Floor || (x, y) == entrance || taken.contains(&(x, y)) || level.on_dais(x, y) || dist[index(x, y)] < 0 {
                continue;
            }
            let near_door = (-1..=1).any(|dy| (-1..=1).any(|dx| matches!(level.cell(x + dx, y + dy), Cell::Door | Cell::Gate | Cell::Bars)));
            for (n, facing) in Facing::ALL.into_iter().enumerate() {
                let (dx, dy) = facing.step();
                let (ax, ay) = (x - dx, y - dy);
                if level.cell(ax, ay) != Cell::Floor || taken.contains(&(ax, ay)) || level.on_dais(ax, ay) || dist[index(ax, ay)] < 0 {
                    continue;
                }
                let candidate = (dist[index(ax, ay)], y, x, n);
                let in_room = level.room_at(x, y).is_some() && level.room_at(ax, ay) == level.room_at(x, y);
                if !level.is_open(x + dx, y + dy) && !near_door && in_room {
                    tight.push(candidate);
                }
                loose.push(candidate);
            }
        }
    }
    // Whether shutting the cell leaves every other open cell reachable.
    let whole_without = |x: i32, y: i32| distances(level, start, Some((x, y))).iter().filter(|&&d| d >= 0).count() == total_open - 1;
    for mut pool in [tight, loose] {
        if pool.is_empty() {
            continue;
        }
        // Furthest first, ties in grid order, so the order is a function of the layout.
        pool.sort_by_key(|&(d, y, x, n)| (-d, y, x, n));
        let furthest = pool[0].0 as f32;
        let reach = pool.iter().take_while(|c| c.0 as f32 >= furthest * FAR_SHARE).count();
        let first = rng.int(0, reach as u32 - 1) as usize;
        // From the pick on through the far cells, round to the pick, then the rest.
        let order = (0..reach).map(|k| (first + k) % reach).chain(reach..pool.len());
        let chosen = order.map(|k| pool[k]).find(|&(_, y, x, _)| whole_without(x, y));
        if let Some((_, y, x, n)) = chosen {
            stairs.push(Stair { x: x as u32, y: y as u32, facing: Facing::ALL[n], kind: StairKind::Down });
            break;
        }
    }
    level.stairs = stairs;
}
