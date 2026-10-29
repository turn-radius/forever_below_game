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
//! Each niche has a height of its own (a short one holds a candle, a tall one an urn), and each room
//! a style for all of its niches ([`NicheStyle`]): plain cut in a plain room, moulded in a claimed
//! one, with a keystone in a hall. Corridors take niches too, plain.
//!
//! Some niches in moulded and grand rooms stand tall, up to about a human height ([`TALL_RANGE`]).
//!
//! A level is rebuilt from its seed and its niches with it, so a niche is not saved. A candle,
//! a skull and bones are scenery; an urn is a container the delver opens, and what is taken from
//! it is kept in the level record (`game::niches`).
//!
//! The game cuts the recess into the wall's blocks (`level::geometry::stone`) and draws what
//! it holds; a candle's recess is lit by a small point light of its own.

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::banners::BannerSpot;
use crate::folk::by_door;
use crate::generate::{item_seed, shuffle};
use crate::roles::RoomRole;
use crate::walls::{bare_wall_faces, corridor_wall_faces, near_light_or_prop};
use crate::{CELL, Cell, Facing, Finishes, Level, LevelParams, LevelStyle, WallFinish, finishes};

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

/// The width of a niche's opening, how far its arch rises over its springing, and how deep it is
/// cut, in world units. A niche's sill and springing are its own ([`NicheSpot::sill`]).
pub const WIDTH: f32 = 6.4;
pub const RISE: f32 = 1.5;
pub const DEPTH: f32 = 4.2;
/// How high a niche's sill stands off the floor, least and most, and how tall its opening is from
/// sill to springing, least and most, in world units.
pub const SILL_RANGE: (f32, f32) = (6.0, 9.0);
pub const OPENING_RANGE: (f32, f32) = (3.8, 8.8);
/// How tall the opening of a tall niche is, least and most: it reaches about a human height
/// (17 units) and over the delver's head. Only moulded and grand rooms have them, for the top
/// [`TALL_SHARE`] of the rolls that set a niche's height.
pub const TALL_RANGE: (f32, f32) = (8.8, 17.0);
const TALL_SHARE: f64 = 0.4;
/// The highest the arch crown of a niche of the ordinary height may reach, under the vault and clear
/// of its moulding.
pub const CROWN_LIMIT: f32 = 17.5;
/// How far under the level's ceiling a tall niche's crown stops, in world units: the vaults spring
/// from the ceiling, so the wall stands at least that high, and the keystone and the blocks
/// over the arch need the room below it.
pub const TALL_HEADROOM: f32 = 4.0;

/// The highest the crown of a tall niche may stand in a level whose ceiling is `ceiling`.
pub fn tall_crown_limit(ceiling: f32) -> f32 {
    (ceiling - TALL_HEADROOM).max(CROWN_LIMIT)
}
/// A moulded surround: how wide the architrave is round the opening, how far it and the sill
/// ledge stand proud of the wall, how far the ledge reaches out to each side and down.
pub const MOULD: f32 = 1.0;
pub const MOULD_PROUD: f32 = 0.9;
pub const LEDGE: f32 = 1.0;
/// A keystone over the crown of a grand niche: half its width, its height, how proud it stands.
pub const KEY_HALF: f32 = 0.9;
pub const KEY_HEIGHT: f32 = 1.6;
pub const KEY_PROUD: f32 = 1.3;
/// Corridor niches: the most a level has, and the chance each wall face that takes one does.
const MOST_IN_CORRIDORS: usize = 5;
const CORRIDOR_CHANCE: f64 = 0.06;

/// The surround of a room's niches, the same for every niche in the room.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NicheStyle {
    /// Cut straight into the wall: wild rooms and corridors.
    Plain,
    /// An architrave round the arch and a ledge under the sill: claimed rooms.
    Moulded,
    /// As moulded, with a keystone over the arch: a faction's hall.
    Grand,
}

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
    /// How high the sill stands off the floor, and how high the arch springs, in world units.
    pub sill: f32,
    pub spring: f32,
    /// The surround, shared by every niche of the room it is in.
    pub style: NicheStyle,
}

impl NicheSpot {
    /// How high the crown of the arch stands.
    pub fn crown(&self) -> f32 {
        self.spring + RISE
    }

    /// Whether the surround is moulded.
    pub fn moulded(&self) -> bool {
        self.style != NicheStyle::Plain
    }

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
/// side (so it is not a corner or a jamb), all of dressed stone, the cell and those beside it are open floor clear of
/// doors and stairs, and the cell in front is open.
fn takes_niche(level: &Level, finishes: &Finishes, cell: (i32, i32), wall: (i32, i32)) -> bool {
    let along = (wall.1.abs(), wall.0.abs());
    let floor_with_rock = |c: (i32, i32)| {
        level.cell(c.0, c.1) == Cell::Floor && level.cell(c.0 + wall.0, c.1 + wall.1) == Cell::Rock && !by_door(level, c.0, c.1) && level.stair_at(c.0, c.1).is_none()
    };
    let beside = |k: i32| (cell.0 + along.0 * k, cell.1 + along.1 * k);
    let front = (cell.0 - wall.0, cell.1 - wall.1);
    let by_stairs = level.stairs.iter().any(|s| (s.x as i32 - cell.0).abs().max((s.y as i32 - cell.1).abs()) <= 1);
    // Ashlar only: rubble is laid in rough stones, which have no frame to give a niche.
    let dressed = |c: (i32, i32)| finishes.at(c.0, c.1).is_some_and(|f| f.walls != WallFinish::Rubble);
    !by_stairs && dressed(cell) && dressed(beside(-1)) && dressed(beside(1)) && floor_with_rock(cell) && floor_with_rock(beside(-1)) && floor_with_rock(beside(1)) && level.cell(front.0, front.1) == Cell::Floor
}

fn facing_of(wall: (i32, i32)) -> Facing {
    Facing::ALL.into_iter().find(|f| f.step() == wall).expect("a wall is a grid step")
}

fn crowds_a_banner(banners: &[BannerSpot], at: (f32, f32)) -> bool {
    banners.iter().any(|b| (b.x - at.0).hypot(b.y - at.1) < CLEAR)
}

/// What one wall face rolls for a niche, whether or not it is used.
struct Rolls {
    pick: f64,
    seed: u32,
    sill: f64,
    opening: f64,
    chance: f64,
}

fn roll(rng: &mut Mulberry32) -> Rolls {
    Rolls { pick: rng.next_f64(), seed: item_seed(rng), sill: rng.next_f64(), opening: rng.next_f64(), chance: rng.next_f64() }
}

impl Rolls {
    fn spot(&self, cell: (i32, i32), wall: (i32, i32), style: NicheStyle, ceiling: f32) -> NicheSpot {
        let sill = SILL_RANGE.0 + (SILL_RANGE.1 - SILL_RANGE.0) * self.sill as f32;
        // The height roll is one draw, as it always was: the top of its range, in a moulded or
        // grand room, is a tall niche rather than a higher shelf.
        let tall_from = 1.0 - TALL_SHARE;
        let (opening, limit) = if style != NicheStyle::Plain && self.opening >= tall_from {
            let t = ((self.opening - tall_from) / TALL_SHARE) as f32;
            (TALL_RANGE.0 + (TALL_RANGE.1 - TALL_RANGE.0) * t, tall_crown_limit(ceiling))
        } else {
            (OPENING_RANGE.0 + (OPENING_RANGE.1 - OPENING_RANGE.0) * self.opening as f32, CROWN_LIMIT)
        };
        let spring = (sill + opening).min(limit - RISE);
        NicheSpot { x: cell.0, y: cell.1, facing: facing_of(wall), contents: contents_for(self.pick), seed: self.seed, sill, spring, style }
    }
}

/// The surround of the niches in room `room`: a hall's are grand, any other claimed room's moulded,
/// a wild room's plain.
fn style_of(level: &Level, room: usize) -> NicheStyle {
    let roles = level.roles_of(room);
    if roles.contains(&RoomRole::Hall) {
        NicheStyle::Grand
    } else if !roles.is_empty() {
        NicheStyle::Moulded
    } else {
        NicheStyle::Plain
    }
}

/// Cuts the level's niches: none outside a level of stone halls.
pub(crate) fn place_niches(level: &Level) -> Vec<NicheSpot> {
    let mut niches: Vec<NicheSpot> = Vec::new();
    if level.style != LevelStyle::StoneHalls || level.ceiling < LOW_CEILING {
        return niches;
    }
    let mut rng = Mulberry32::new(level.seed ^ NICHE_SALT);
    let finishes = finishes(level, &LevelParams::for_style(level.style));
    let fits = |niches: &[NicheSpot], spot: &NicheSpot| {
        let at = spot.face();
        !(crowds_a_banner(&level.banners, at) || near_light_or_prop(level, at.0, at.1, CLEAR) || niches.iter().any(|o| (o.x - spot.x).abs().max((o.y - spot.y).abs()) < APART))
    };
    for (r, room) in level.rooms.iter().enumerate() {
        let style = style_of(level, r);
        let mut faces = bare_wall_faces(level, room);
        shuffle(&mut faces, &mut rng);
        let mut want = if rng.chance(ROOM_CHANCE) { 1 } else { 0 };
        if want == 1 && room.w * room.h >= 36 && rng.chance(SECOND_CHANCE) {
            want = 2;
        }
        // Rolled for every face, used or not, so a wall that fails the checks never moves a later one.
        let rolls: Vec<Rolls> = faces.iter().map(|_| roll(&mut rng)).collect();
        let mut made = 0;
        for (&(cell, wall), rolled) in faces.iter().zip(&rolls) {
            if made == want {
                break;
            }
            if !takes_niche(level, &finishes, cell, wall) {
                continue;
            }
            let spot = rolled.spot(cell, wall, style, level.ceiling);
            if fits(&niches, &spot) {
                niches.push(spot);
                made += 1;
            }
        }
    }
    // Then the corridors, plain: a few walls of the level's passages.
    let mut faces = corridor_wall_faces(level);
    shuffle(&mut faces, &mut rng);
    let rolls: Vec<Rolls> = faces.iter().map(|_| roll(&mut rng)).collect();
    let mut made = 0;
    for (&(cell, wall), rolled) in faces.iter().zip(&rolls) {
        if made == MOST_IN_CORRIDORS {
            break;
        }
        if rolled.chance >= CORRIDOR_CHANCE || !takes_niche(level, &finishes, cell, wall) {
            continue;
        }
        let spot = rolled.spot(cell, wall, NicheStyle::Plain, level.ceiling);
        if fits(&niches, &spot) {
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
        assert!(halls > 50 && with * 4 >= halls * 3, "{with} of {halls} halls have niches");
        assert!(total >= halls * 2, "{total} niches in {halls} halls");
        assert_eq!(contents.len(), 5, "{contents:?}");
    }

    #[test]
    fn a_niche_stands_on_a_bare_straight_wall_clear_of_everything() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let finishes = finishes(&level, &LevelParams::for_style(level.style));
            for n in &level.niches {
                let (wall, at) = (n.facing.step(), n.face());
                let tag = format!("seed {seed} depth {depth}: {n:?}");
                assert!(takes_niche(&level, &finishes, (n.x, n.y), wall), "{tag}");
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

    #[test]
    fn niches_vary_in_height_and_stay_under_the_vault() {
        let (mut lo, mut hi, mut total) = (f32::MAX, f32::MIN, 0);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for n in &level.niches {
                let opening = n.spring - n.sill;
                assert!((SILL_RANGE.0..=SILL_RANGE.1).contains(&n.sill), "{n:?}");
                assert!(opening >= 2.0 && n.crown() <= tall_crown_limit(level.ceiling) + 1e-3, "{n:?}");
                (lo, hi, total) = (lo.min(opening), hi.max(opening), total + 1);
            }
        }
        assert!(total > 100 && hi - lo > 4.0, "{total} niches, openings {lo} to {hi}");
    }

    #[test]
    fn every_niche_of_a_room_shares_its_style_and_corridors_are_plain() {
        let (mut moulded, mut grand, mut plain_rooms, mut corridor) = (0, 0, 0, 0);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for (r, room) in level.rooms.iter().enumerate() {
                let styles: std::collections::HashSet<_> = level.niches.iter().filter(|n| room.contains(n.x, n.y)).map(|n| n.style).collect();
                assert!(styles.len() <= 1, "seed {seed} depth {depth}: room {r} mixes {styles:?}");
                for style in styles {
                    assert_eq!(style, style_of(&level, r), "seed {seed} depth {depth}");
                    match style {
                        NicheStyle::Moulded => moulded += 1,
                        NicheStyle::Grand => grand += 1,
                        NicheStyle::Plain => plain_rooms += 1,
                    }
                }
            }
            for n in level.niches.iter().filter(|n| !level.rooms.iter().any(|r| r.contains(n.x, n.y))) {
                assert_eq!(n.style, NicheStyle::Plain);
                corridor += 1;
            }
            assert!(level.niches.iter().filter(|n| !level.rooms.iter().any(|r| r.contains(n.x, n.y))).count() <= MOST_IN_CORRIDORS);
        }
        assert!(moulded > 5 && grand > 3 && plain_rooms > 5 && corridor > 10, "{moulded} moulded, {grand} grand, {plain_rooms} plain rooms, {corridor} corridor niches");
    }

    #[test]
    fn only_moulded_and_grand_rooms_have_tall_niches_and_they_stay_under_the_vault() {
        let (mut tall, mut grand_tall, mut highest) = (0, 0, 0.0f32);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for n in &level.niches {
                let opening = n.spring - n.sill;
                if n.crown() > CROWN_LIMIT + 1e-3 || opening > OPENING_RANGE.1 + 1e-3 {
                    assert_ne!(n.style, NicheStyle::Plain, "seed {seed} depth {depth}: {n:?}");
                    assert!(n.crown() <= level.ceiling - TALL_HEADROOM + 1e-3 && opening <= TALL_RANGE.1 + 1e-3, "seed {seed} depth {depth}: {n:?}");
                    tall += 1;
                    grand_tall += (n.style == NicheStyle::Grand) as u32;
                    highest = highest.max(opening);
                }
                if n.style == NicheStyle::Plain {
                    assert!(opening <= OPENING_RANGE.1 + 1e-3 && n.crown() <= CROWN_LIMIT + 1e-3, "{n:?}");
                }
            }
        }
        assert!(tall >= 10 && grand_tall >= 3, "{tall} tall niches, {grand_tall} grand");
        assert!(highest >= 12.0, "the tallest opening is {highest}");
    }
}
