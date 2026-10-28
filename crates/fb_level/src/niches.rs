//! Niches: shallow arched recesses cut into the dressed stone of some walls, about chest
//! height and a third of a cell wide. Most stand empty; some hold a lit candle, a skull, a
//! scatter of bones or a small urn.
//!
//! They are a list of their own, not [`crate::Prop`]s, and are only ever cut into a level of
//! stone halls: rough rock and the caverns have no dressed wall to cut. They come from a stream
//! of the level seed of their own, placed after everything else (the props, the banners, the
//! stairs, the lights), so nothing else in a level moves for them, and each stands on a bare
//! stretch of wall clear of all of it ([`crate::walls`]).
//!
//! A level is rebuilt from its seed and its niches with it, so a niche is not saved. What one
//! holds is scenery, not loot: nothing in a niche can be taken.
//!
//! The game cuts the recess into the wall's blocks (`level::geometry::stone`) and draws what
//! it holds; a candle's recess is lit by a small point light of its own.

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::banners::BannerSpot;
use crate::folk::by_door;
use crate::generate::{item_seed, shuffle};
use crate::walls::{bare_wall_faces, near_light_or_prop};
use crate::{CELL, Cell, Facing, Level, LevelStyle};

/// Salt for the niche stream.
pub(crate) const NICHE_SALT: u32 = 0x41C4_E5D3;

/// How far a niche stands from a banner, a light or a prop, in cells.
const CLEAR: f32 = 1.3;
/// Fewest cells between two niches, along either axis.
const APART: i32 = 4;
/// Chance a room has a niche, and a second one in a roomy one.
const ROOM_CHANCE: f64 = 0.45;
const SECOND_CHANCE: f64 = 0.2;
/// The lowest ceiling a niche is cut under, in world units: the arch peaks at about 16.
const LOW_CEILING: f32 = 22.0;

/// The width of a niche's opening, the height of its sill off the floor, the height its arch
/// springs from and how far the arch rises over its springing, and how deep it is cut, in
/// world units. The game's recess and its contents are built to these.
pub const WIDTH: f32 = 6.4;
pub const SILL: f32 = 7.5;
pub const SPRING: f32 = 11.8;
pub const RISE: f32 = 1.5;
pub const DEPTH: f32 = 4.2;

/// What a niche holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NicheContents {
    Empty,
    /// A lit candle: the recess glows.
    Candle,
    Skull,
    Bones,
    Urn,
}

/// A niche in a wall: the floor cell it opens from and the way the wall faces from that cell
/// ([`Facing::step`] points at the rock); the niche is centred on the wall face of the cell.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NicheSpot {
    pub x: i32,
    pub y: i32,
    pub facing: Facing,
    pub contents: NicheContents,
    /// Rolls what it holds.
    pub seed: u32,
}

impl NicheSpot {
    /// The middle of its opening on the wall, in grid coordinates.
    pub fn face(&self) -> (f32, f32) {
        let (dx, dy) = self.facing.step();
        (self.x as f32 + 0.5 + dx as f32 * 0.5, self.y as f32 + 0.5 + dy as f32 * 0.5)
    }

    /// The same spot `away` world units out from the wall, in the open cell, in grid coordinates.
    pub fn in_front(&self, away: f32) -> (f32, f32) {
        let (dx, dy) = self.facing.step();
        let (fx, fy) = self.face();
        (fx - dx as f32 * away / CELL, fy - dy as f32 * away / CELL)
    }
}

/// Whether the face of `cell` toward `wall` can take a niche: the wall runs on a cell to each
/// side (so it is not a corner or a jamb), the cell and those beside it are open floor clear of
/// doors and stairs, and the cell in front is open.
fn takes_niche(level: &Level, cell: (i32, i32), wall: (i32, i32)) -> bool {
    let along = (wall.1.abs(), wall.0.abs());
    let floor_with_rock = |c: (i32, i32)| {
        level.cell(c.0, c.1) == Cell::Floor && level.cell(c.0 + wall.0, c.1 + wall.1) == Cell::Rock && !by_door(level, c.0, c.1) && level.stair_at(c.0, c.1).is_none()
    };
    let beside = |k: i32| (cell.0 + along.0 * k, cell.1 + along.1 * k);
    let front = (cell.0 - wall.0, cell.1 - wall.1);
    let by_stairs = level.stairs.iter().any(|s| (s.x as i32 - cell.0).abs().max((s.y as i32 - cell.1).abs()) <= 1);
    !by_stairs && floor_with_rock(cell) && floor_with_rock(beside(-1)) && floor_with_rock(beside(1)) && level.cell(front.0, front.1) == Cell::Floor
}

fn facing_of(wall: (i32, i32)) -> Facing {
    Facing::ALL.into_iter().find(|f| f.step() == wall).expect("a wall is a grid step")
}

fn crowds_a_banner(banners: &[BannerSpot], at: (f32, f32)) -> bool {
    banners.iter().any(|b| (b.x - at.0).hypot(b.y - at.1) < CLEAR)
}

/// Cuts the level's niches: none outside a level of stone halls.
pub(crate) fn place_niches(level: &Level) -> Vec<NicheSpot> {
    let mut niches: Vec<NicheSpot> = Vec::new();
    if level.style != LevelStyle::StoneHalls || level.ceiling < LOW_CEILING {
        return niches;
    }
    let mut rng = Mulberry32::new(level.seed ^ NICHE_SALT);
    for room in &level.rooms {
        let mut faces = bare_wall_faces(level, room);
        shuffle(&mut faces, &mut rng);
        let mut want = if rng.chance(ROOM_CHANCE) { 1 } else { 0 };
        if want == 1 && room.w * room.h >= 36 && rng.chance(SECOND_CHANCE) {
            want = 2;
        }
        // Rolled for every face, used or not, so a wall that fails the checks never moves a later one.
        let rolls: Vec<(f64, u32)> = faces.iter().map(|_| (rng.next_f64(), item_seed(&mut rng))).collect();
        let mut made = 0;
        for (&(cell, wall), &(pick, seed)) in faces.iter().zip(&rolls) {
            if made == want {
                break;
            }
            if !takes_niche(level, cell, wall) {
                continue;
            }
            let spot = NicheSpot { x: cell.0, y: cell.1, facing: facing_of(wall), contents: contents_for(pick), seed };
            let at = spot.face();
            if crowds_a_banner(&level.banners, at) || near_light_or_prop(level, at.0, at.1, CLEAR) {
                continue;
            }
            if niches.iter().any(|o| (o.x - cell.0).abs().max((o.y - cell.1).abs()) < APART) {
                continue;
            }
            niches.push(spot);
            made += 1;
        }
    }
    niches
}

/// What a niche holds, from a roll of `0..1`: more than half stand empty.
fn contents_for(pick: f64) -> NicheContents {
    const TABLE: [(NicheContents, f64); 5] = [
        (NicheContents::Empty, 0.55),
        (NicheContents::Candle, 0.2),
        (NicheContents::Skull, 0.08),
        (NicheContents::Bones, 0.08),
        (NicheContents::Urn, 0.09),
    ];
    let mut edge = 0.0;
    for (contents, share) in TABLE {
        edge += share;
        if pick < edge {
            return contents;
        }
    }
    NicheContents::Empty
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate;

    fn sweep() -> impl Iterator<Item = (u32, u32)> {
        [1, 2, 3, 4, 5, 6].into_iter().flat_map(|depth| (1..=40).map(move |seed| (seed, depth)))
    }

    #[test]
    fn stone_halls_have_a_few_niches_and_rock_and_caverns_none() {
        let (mut halls, mut with, mut total, mut contents) = (0, 0, 0, std::collections::HashSet::new());
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            if level.style == LevelStyle::StoneHalls {
                halls += 1;
                with += !level.niches.is_empty() as u32;
                total += level.niches.len() as u32;
                assert!(level.niches.len() <= 12, "seed {seed} depth {depth}: {}", level.niches.len());
                contents.extend(level.niches.iter().map(|n| n.contents));
            } else {
                assert!(level.niches.is_empty(), "seed {seed} depth {depth}: {:?}", level.style);
            }
        }
        assert!(halls > 50 && with * 10 >= halls * 9, "{with} of {halls} halls have niches");
        assert!(total >= halls * 2, "{total} niches in {halls} halls");
        assert_eq!(contents.len(), 5, "{contents:?}");
    }

    #[test]
    fn a_niche_stands_on_a_bare_straight_wall_clear_of_everything() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for n in &level.niches {
                let (wall, at) = (n.facing.step(), n.face());
                let tag = format!("seed {seed} depth {depth}: {n:?}");
                assert!(takes_niche(&level, (n.x, n.y), wall), "{tag}");
                assert!(!crowds_a_banner(&level.banners, at), "{tag}");
                assert!(!near_light_or_prop(&level, at.0, at.1, CLEAR), "{tag}");
                                assert!(level.niches.iter().all(|o| o == n || (o.x - n.x).abs().max((o.y - n.y).abs()) >= APART), "{tag}");
            }
        }
    }

    #[test]
    fn the_same_seed_cuts_the_same_niches() {
        for (seed, depth) in sweep().take(40) {
            assert_eq!(generate(seed, depth).niches, generate(seed, depth).niches);
        }
    }

    #[test]
    fn niches_move_nothing_else() {
        for (seed, depth) in sweep().take(60) {
            let params = crate::LevelParams::for_depth(depth);
            let before = crate::generate::generate_before_niches(seed, seed, depth, &params);
            assert!(before.niches.is_empty());
            let level = generate(seed, depth);
            assert_eq!(Level { niches: Vec::new(), ..level }, before, "seed {seed} depth {depth}: the niche step moved something");
        }
    }
}
