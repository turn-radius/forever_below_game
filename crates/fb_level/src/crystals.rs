//! Crystals: what grows in a crystal cavern ([`crate::LevelStyle::CrystalCavern`]). Large single
//! crystals and clusters stand on the floor, lean from the walls and hang from the ceiling, and
//! geodes are set split open in the walls. Every one is the Armory's crystal family
//! ([`armory::families::crystal`]); the spot holds only what places it and the seed that rolls it.
//!
//! Crystals come from a stream of the level seed of their own, after everything else, so they
//! never move a wall, an item, a light, a prop, a stair or anyone standing. Their variety is
//! per place: a level draws two stones and each chamber takes one of them, and each chamber
//! leans to one habit, so a chamber reads as one growth rather than a scatter of rolls.
//!
//! Where they go is our own proposal:
//! - Floor crystals stand only in cells whose eight neighbours are all open floor and no two in
//!   neighbouring cells, and each covers at most [`MOST_FLOOR_RADIUS`] units, under half a
//!   cell's width, so there is always a lane round every crystal and the way between the stairs
//!   is never blocked. They keep clear of every item, light, folk, prop, stair, doorway and the
//!   way in from the entrance, and so of a faction's furniture.
//! - Wall crystals grow from the wall face high above the head, and geodes are set in it at
//!   hand height; both only where the wall is at least three cells deep in open floor and not
//!   at a corner or by a doorway or banner.
//! - Ceiling crystals hang over open interior cells, their tips well above head height.


use armory::families::crystal::{self, CrystalState, Crystals, Growth, Habit};
use armory::gem::Gemstone;
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::generate::item_seed;
use crate::{CELL, Cell, Facing, Level, LevelStyle, Room};

/// Salt for the stream that grows crystals.
pub(crate) const CRYSTAL_SALT: u32 = 0xC4A5_7A15;

/// The most a floor crystal covers about its base, in world units: under half a cell, so a lane
/// stays open between it and a crystal two cells away.
pub const MOST_FLOOR_RADIUS: f32 = 8.5;
/// Where a crystal grows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CrystalSurface {
    /// From the floor, pointing up.
    Floor,
    /// From a wall, pointing out into the cavern and tilted up.
    Wall,
    /// From the ceiling, pointing down.
    Ceiling,
}

/// A crystal growing in the level, at a point in grid coordinates (cells, fractional).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrystalSpot {
    /// Where its base meets the surface: on the floor, on the wall's face or on the ceiling.
    pub x: f32,
    pub y: f32,
    /// How high its base is above the floor, in world units: 0 for the floor, the height up the
    /// wall for a wall, the ceiling's height for a ceiling.
    pub up: f32,
    /// A wall crystal looks out from its wall this way (turn about +Y, radians, as
    /// [`crate::PropSpot::yaw`]); a floor or ceiling crystal leans toward it.
    pub yaw: f32,
    /// How far its axis leans off its surface's own direction (up from the floor, out from the
    /// wall, down from the ceiling) toward up for a wall and toward `yaw` for the others,
    /// radians.
    pub lean: f32,
    pub surface: CrystalSurface,
    pub habit: Habit,
    pub growth: Growth,
    pub stone: Gemstone,
    /// Rolls its shape.
    pub seed: u32,
    /// The crystal's `size` ([`armory::families::crystal::Crystal::size`]), in world units.
    pub size: f32,
    /// The floor a floor crystal covers about its base, in world units; 0 for the others, which
    /// block no one's way.
    pub radius: f32,
}

/// The Armory state of the crystal at `spot`.
pub fn crystal_state(spot: &CrystalSpot) -> CrystalState {
    let mut state = Crystals::roll_as(spot.seed, spot.habit, spot.growth);
    state.crystal.stone = spot.stone;
    state.crystal.size = spot.size;
    state.fit(&crystal::LIMITS);
    state
}

impl CrystalSpot {
    /// The way its axis points, a unit vector in world axes (x east, y up, z south).
    pub fn axis(&self) -> [f32; 3] {
        let (s, c) = self.yaw.sin_cos();
        let (sl, cl) = self.lean.sin_cos();
        match self.surface {
            CrystalSurface::Floor => [s * sl, cl, c * sl],
            CrystalSurface::Ceiling => [s * sl, -cl, c * sl],
            CrystalSurface::Wall => [s * cl, sl, c * cl],
        }
    }
}

/// The crystals of a crystal cavern, and none elsewhere.
pub(crate) fn place_crystals(level: &Level, rng: &mut Mulberry32) -> Vec<CrystalSpot> {
    if level.style != LevelStyle::CrystalCavern {
        return Vec::new();
    }
    // The level's own stones: two, and each chamber takes one.
    let stones = [*rng.pick(&STONES), *rng.pick(&STONES)];
    let keep_clear = keep_clear(level);
    let mut spots: Vec<CrystalSpot> = Vec::new();
    let mut floor_cells: Vec<(i32, i32)> = Vec::new();

    for (r, room) in level.rooms.iter().enumerate() {
        let stone = stones[r % 2];
        let lean = *rng.pick(&[Habit::Prismatic, Habit::Columnar, Habit::Acicular]);
        let mut cells = interior(level, room);
        crate::generate::shuffle(&mut cells, rng);
        let want = (room.area() / FLOOR_CELLS).min(MOST_ON_FLOOR);
        let mut placed = 0;
        for (cx, cy) in cells {
            if placed >= want {
                break;
            }
            if floor_cells.iter().any(|&(px, py)| (px - cx).abs() <= 1 && (py - cy).abs() <= 1) {
                continue;
            }
            let (x, y) = (cx as f32 + 0.5 + rng.range_f32(-0.25, 0.25), cy as f32 + 0.5 + rng.range_f32(-0.25, 0.25));
            let Some(spot) = floor_crystal(x, y, stone, lean, &keep_clear, rng) else { continue };
            floor_cells.push((cx, cy));
            spots.push(spot);
            placed += 1;
        }
    }

    // Walls: crystals high up and geodes at hand height.
    let mut walls = wall_faces(level, &keep_clear);
    crate::generate::shuffle(&mut walls, rng);
    let geodes = rng.int(2, 5) as usize;
    let crystals = (walls.len() / WALL_FACES_PER_CRYSTAL).min(MOST_ON_WALLS);
    let mut taken: Vec<(i32, i32)> = Vec::new();
    for face in walls {
        let wanted = taken.len();
        if wanted >= geodes + crystals {
            break;
        }
        // Not beside another of its kind on the same wall.
        if taken.iter().any(|&(tx, ty)| (tx - face.cell.0).abs() <= 2 && (ty - face.cell.1).abs() <= 2) {
            continue;
        }
        let stone = level.room_at(face.cell.0, face.cell.1).map_or(stones[0], |r| stones[r % 2]);
        let geode = wanted < geodes;
        let (x, y) = face.point(rng.range_f32(-0.2, 0.2));
        let yaw = face.out_yaw();
        let spot = if geode {
            CrystalSpot { x, y, up: rng.range_f32(9.0, 16.0), yaw, lean: 0.0, surface: CrystalSurface::Wall, habit: *rng.pick(&GEODE_HABITS), growth: Growth::Geode, stone, seed: item_seed(rng), size: rng.range_f32(9.0, 14.0), radius: 0.0 }
        } else {
            let highest = (level.ceiling - 16.0).max(WALL_FROM + 1.0);
            let growth = *rng.weighted(&[(Growth::Single, 3), (Growth::Cluster, 2)]);
            CrystalSpot { x, y, up: rng.range_f32(WALL_FROM, highest), yaw, lean: rng.range_f32(0.2, 0.75), surface: CrystalSurface::Wall, habit: *rng.weighted(&HABITS), growth, stone, seed: item_seed(rng), size: rng.range_f32(12.0, 26.0), radius: 0.0 }
        };
        taken.push(face.cell);
        spots.push(spot);
    }

    // The ceiling: long points hanging well above head height.
    let mut over: Vec<(i32, i32)> = level.rooms.iter().flat_map(|room| interior(level, room)).collect();
    crate::generate::shuffle(&mut over, rng);
    let hang = (over.len() / CEILING_CELLS).min(MOST_ON_CEILING);
    let mut hung: Vec<(i32, i32)> = Vec::new();
    for (cx, cy) in over {
        if hung.len() >= hang {
            break;
        }
        if hung.iter().any(|&(px, py)| (px - cx).abs() <= 1 && (py - cy).abs() <= 1) {
            continue;
        }
        let stone = level.room_at(cx, cy).map_or(stones[0], |r| stones[r % 2]);
        let growth = *rng.weighted(&[(Growth::Single, 3), (Growth::Cluster, 2)]);
        hung.push((cx, cy));
        spots.push(CrystalSpot {
            x: cx as f32 + 0.5 + rng.range_f32(-0.3, 0.3),
            y: cy as f32 + 0.5 + rng.range_f32(-0.3, 0.3),
            up: level.ceiling,
            yaw: rng.range_f32(0.0, std::f32::consts::TAU),
            lean: rng.range_f32(0.0, 0.3),
            surface: CrystalSurface::Ceiling,
            habit: *rng.weighted(&HABITS),
            growth,
            stone,
            seed: item_seed(rng),
            size: rng.range_f32(12.0, 28.0),
            radius: 0.0,
        });
    }
    spots
}

/// A floor crystal at `(x, y)`: a tall single or a cluster of the chamber's habit, shrunk until
/// it covers no more than [`MOST_FLOOR_RADIUS`], or `None` if it will not fit clear of everything
/// in `keep_clear`.
fn floor_crystal(x: f32, y: f32, stone: Gemstone, lean_habit: Habit, keep_clear: &[Clearance], rng: &mut Mulberry32) -> Option<CrystalSpot> {
    let habit = if rng.chance(0.75) { lean_habit } else { *rng.weighted(&FLOOR_HABITS) };
    let growth = *rng.weighted(&[(Growth::Single, 3), (Growth::Cluster, 2)]);
    let mut spot = CrystalSpot { x, y, up: 0.0, yaw: rng.range_f32(0.0, std::f32::consts::TAU), lean: rng.range_f32(0.0, 0.12), surface: CrystalSurface::Floor, habit, growth, stone, seed: item_seed(rng), size: rng.range_f32(14.0, 50.0), radius: 0.0 };
    for _ in 0..8 {
        let radius = footprint(&spot);
        if radius <= MOST_FLOOR_RADIUS {
            spot.radius = radius;
            let at = (x * CELL, y * CELL);
            let clear = keep_clear.iter().all(|k| ((at.0 - k.x).powi(2) + (at.1 - k.y).powi(2)).sqrt() >= k.radius + radius + CLEAR_GAP);
            return clear.then_some(spot);
        }
        spot.size *= 0.82;
    }
    None
}

/// The floor a crystal of `spot` covers, in world units: how far its mesh reaches from its base,
/// sideways, as built.
pub fn footprint(spot: &CrystalSpot) -> f32 {
    let mesh = armory::Item::Crystal(crystal_state(spot)).build();
    mesh.parts
        .iter()
        .flat_map(|p| p.positions.iter())
        .map(|p| p[0].hypot(p[2]))
        .fold(0.0, f32::max)
}

/// Least gap, in world units, between a floor crystal's foot and anything it keeps clear of.
const CLEAR_GAP: f32 = 2.0;
/// Chamber cells for each floor crystal there, and the most in one chamber.
const FLOOR_CELLS: u32 = 22;
const MOST_ON_FLOOR: u32 = 12;
/// Wall faces for each wall crystal, and the most on a level's walls.
const WALL_FACES_PER_CRYSTAL: usize = 14;
const MOST_ON_WALLS: usize = 14;
/// Open interior cells for each hanging crystal, and the most on a level.
const CEILING_CELLS: usize = 30;
const MOST_ON_CEILING: usize = 16;
/// The lowest a wall crystal's base is set, in world units: over a delver's head.
const WALL_FROM: f32 = 22.0;

/// The stones crystals grow in a cavern: the clear and coloured ones, never the opaque.
const STONES: [Gemstone; 7] = [Gemstone::Garnet, Gemstone::Amethyst, Gemstone::Sapphire, Gemstone::Emerald, Gemstone::Citrine, Gemstone::Ruby, Gemstone::Diamond];
/// What stands on a floor, besides the chamber's lean: long prisms, now and then a stubby form.
const FLOOR_HABITS: [(Habit, u32); 6] = [(Habit::Prismatic, 4), (Habit::Columnar, 4), (Habit::Acicular, 2), (Habit::Bladed, 2), (Habit::Octahedral, 1), (Habit::Cubic, 1)];
/// What grows from walls and ceilings.
const HABITS: [(Habit, u32); 8] = [(Habit::Prismatic, 4), (Habit::Columnar, 4), (Habit::Acicular, 3), (Habit::Bladed, 2), (Habit::Tabular, 1), (Habit::Cubic, 1), (Habit::Octahedral, 1), (Habit::Dodecahedral, 1)];
/// What lines a geode.
const GEODE_HABITS: [Habit; 4] = [Habit::Prismatic, Habit::Columnar, Habit::Cubic, Habit::Dodecahedral];

/// A spot the level's contents keep crystals off: a point and a radius, in world units.
#[derive(Clone, Copy, Debug)]
struct Clearance {
    x: f32,
    y: f32,
    radius: f32,
}

/// Everything a floor crystal keeps clear of: every item, light and folk, every prop (and the room
/// a chest's lid needs behind it), banners, doorways, stairs and the way in from the entrance.
fn keep_clear(level: &Level) -> Vec<Clearance> {
    let at = |x: f32, y: f32, radius: f32| Clearance { x: x * CELL, y: y * CELL, radius };
    let mut out = Vec::new();
    out.extend(level.items.iter().map(|i| at(i.x, i.y, 3.0)));
    out.extend(level.lights.iter().map(|l| at(l.x, l.y, 4.0)));
    out.extend(level.folk.iter().map(|f| at(f.x, f.y, 4.0)));
    out.extend(level.props.iter().map(|p| at(p.x, p.y, p.prop.floor_shape().radius() + p.prop.room_behind() + 3.0)));
    out.extend(level.banners.iter().map(|b| at(b.x, b.y, CELL * 0.8)));
    out.extend(level.stairs.iter().flat_map(|s| {
        let (ax, ay) = s.approach();
        [at(s.x as f32 + 0.5, s.y as f32 + 0.5, CELL * 1.2), at(ax as f32 + 0.5, ay as f32 + 0.5, CELL * 1.2)]
    }));
    let (ex, ey) = (level.entrance.x as f32 + 0.5, level.entrance.y as f32 + 0.5);
    out.push(at(ex, ey, CELL * 2.0));
    out.extend(crate::props::entrance_path(level).into_iter().map(|(x, y)| at(x as f32 + 0.5, y as f32 + 0.5, CELL * 0.9)));
    for y in 0..level.height as i32 {
        for x in 0..level.width as i32 {
            if level.cell(x, y).is_doorway() {
                out.push(at(x as f32 + 0.5, y as f32 + 0.5, CELL * 1.2));
            }
        }
    }
    out
}

/// The cells of `room` with floor all about them: no wall, pillar, doorway or stair within a
/// cell, so a crystal standing in one leaves a lane round it.
fn interior(level: &Level, room: &Room) -> Vec<(i32, i32)> {
    let mut cells = Vec::new();
    for y in room.y as i32..(room.y + room.h) as i32 {
        for x in room.x as i32..(room.x + room.w) as i32 {
            if (-1..=1).all(|dy| (-1..=1).all(|dx| level.cell(x + dx, y + dy) == Cell::Floor)) {
                cells.push((x, y));
            }
        }
    }
    cells
}

/// A face of a wall a crystal may grow from: the open cell in front of it and the way the rock
/// lies from that cell.
#[derive(Clone, Copy, Debug)]
struct WallFace {
    cell: (i32, i32),
    rock: Facing,
}

impl WallFace {
    /// The point on the wall's face, shifted `along` cells along the wall, in grid coordinates.
    fn point(&self, along: f32) -> (f32, f32) {
        let (dx, dy) = self.rock.step();
        let (lx, ly) = (-dy as f32, dx as f32);
        (self.cell.0 as f32 + 0.5 + dx as f32 * 0.5 + lx * along, self.cell.1 as f32 + 0.5 + dy as f32 * 0.5 + ly * along)
    }

    /// The turn that faces out from the wall.
    fn out_yaw(&self) -> f32 {
        let (dx, dy) = self.rock.step();
        (-dx as f32).atan2(-dy as f32)
    }
}

/// Every wall face a crystal may grow from: rock (or a pillar) on one side with floor three
/// cells deep before it, floor to either side along it, nothing of the level's by it.
fn wall_faces(level: &Level, keep_clear: &[Clearance]) -> Vec<WallFace> {
    let mut faces = Vec::new();
    for y in 0..level.height as i32 {
        for x in 0..level.width as i32 {
            if level.cell(x, y) != Cell::Floor {
                continue;
            }
            for rock in Facing::ALL {
                let (dx, dy) = rock.step();
                if !matches!(level.cell(x + dx, y + dy), Cell::Rock | Cell::Column) {
                    continue;
                }
                let deep = (1..=2).all(|k| level.cell(x - dx * k, y - dy * k) == Cell::Floor);
                let along = [-1, 1].iter().all(|&s| level.cell(x - dy * s, y + dx * s) == Cell::Floor);
                if !(deep && along) {
                    continue;
                }
                let face = WallFace { cell: (x, y), rock };
                let (px, py) = face.point(0.0);
                let near = keep_clear.iter().any(|k| ((px * CELL - k.x).powi(2) + (py * CELL - k.y).powi(2)).sqrt() < k.radius + CELL * 0.6);
                if !near {
                    faces.push(face);
                }
            }
        }
    }
    faces
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LevelParams, StairKind, generate_in};

    fn cavern(seed: u32, depth: u32) -> Level {
        generate_in(seed, depth, &LevelParams::for_style(LevelStyle::CrystalCavern))
    }

    #[test]
    fn only_caverns_grow_crystals() {
        for depth in [1, 2, 3, 4, 6, 7] {
            assert!(generate_in(11, depth, &LevelParams::for_style(LevelStyle::for_depth(depth))).crystals.is_empty(), "depth {depth}");
        }
        assert!(!cavern(11, 5).crystals.is_empty());
    }

    #[test]
    fn a_cavern_grows_crystals_on_floor_wall_and_ceiling_and_geodes_in_walls() {
        let mut seen = std::collections::HashSet::new();
        let mut geodes = 0;
        for seed in 1..=6 {
            let level = cavern(seed, 5);
            for c in &level.crystals {
                seen.insert(c.surface);
                if c.growth == Growth::Geode {
                    assert_eq!(c.surface, CrystalSurface::Wall, "geodes are set in walls");
                    geodes += 1;
                }
            }
            assert!(level.crystals.len() >= 10, "seed {seed}: only {} crystals", level.crystals.len());
        }
        assert_eq!(seen.len(), 3, "{seen:?}");
        assert!(geodes > 0);
    }

    #[test]
    fn crystals_are_the_same_every_time_and_the_stones_are_the_chambers() {
        assert_eq!(cavern(3, 5).crystals, cavern(3, 5).crystals);
        assert_ne!(cavern(3, 5).crystals, cavern(4, 5).crystals);
        // A level draws at most two stones across all its floor crystals.
        for seed in 1..=6 {
            let level = cavern(seed, 5);
            let stones: std::collections::HashSet<_> = level.crystals.iter().map(|c| c.stone).collect();
            assert!(stones.len() <= 2, "seed {seed}: {stones:?}");
        }
    }

    #[test]
    fn floor_crystals_never_block_the_way_or_stand_on_the_levels_furniture() {
        for seed in 1..=6 {
            for depth in [5, 9] {
                let level = cavern(seed, depth);
                let what = format!("seed {seed} depth {depth}");
                let floor: Vec<_> = level.crystals.iter().filter(|c| c.surface == CrystalSurface::Floor).collect();
                assert!(!floor.is_empty(), "{what}: no crystals on the floor");
                for c in &floor {
                    assert!(c.radius > 0.0 && c.radius <= MOST_FLOOR_RADIUS, "{what}: radius {}", c.radius);
                    assert!(level.is_open(c.x as i32, c.y as i32), "{what}: a crystal outside the floor");
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            assert_eq!(level.cell(c.x as i32 + dx, c.y as i32 + dy), Cell::Floor, "{what}: a crystal by a wall");
                        }
                    }
                }
                // Two floor crystals never stand in neighbouring cells.
                for (i, a) in floor.iter().enumerate() {
                    for b in &floor[i + 1..] {
                        assert!((a.x as i32 - b.x as i32).abs() > 1 || (a.y as i32 - b.y as i32).abs() > 1, "{what}: crystals side by side");
                    }
                }
                // Clear of items, lights, folk, props, stairs and the way in.
                for c in &floor {
                    let (cx, cy) = (c.x * CELL, c.y * CELL);
                    let gap = |x: f32, y: f32| ((cx - x * CELL).powi(2) + (cy - y * CELL).powi(2)).sqrt() - c.radius;
                    for p in &level.props {
                        assert!(gap(p.x, p.y) >= p.prop.floor_shape().radius(), "{what}: a crystal in a prop");
                    }
                    for i in &level.items {
                        assert!(gap(i.x, i.y) >= 3.0, "{what}: a crystal on an item");
                    }
                    for l in &level.lights {
                        assert!(gap(l.x, l.y) >= 4.0, "{what}: a crystal in a light");
                    }
                    for f in &level.folk {
                        assert!(gap(f.x, f.y) >= 4.0, "{what}: a crystal on someone");
                    }
                    for s in &level.stairs {
                        assert!(gap(s.x as f32 + 0.5, s.y as f32 + 0.5) >= CELL, "{what}: a crystal at a stair");
                    }
                }
                // With every crystal's foot a wall, every open cell is still reachable, and the
                // stairs can be reached from each other.
                let blocked = |x: i32, y: i32| floor.iter().any(|c| (c.x - (x as f32 + 0.5)).abs() * CELL < c.radius + 1.0 && (c.y - (y as f32 + 0.5)).abs() * CELL < c.radius + 1.0);
                let (sx, sy, _) = level.start();
                let mut seen = vec![false; level.cells.len()];
                let mut stack = vec![(sx as i32, sy as i32)];
                seen[(sy * level.width + sx) as usize] = true;
                while let Some((x, y)) = stack.pop() {
                    for f in Facing::ALL {
                        let (dx, dy) = f.step();
                        let (nx, ny) = (x + dx, y + dy);
                        if level.is_open(nx, ny) && !blocked(nx, ny) && !seen[(ny as u32 * level.width + nx as u32) as usize] {
                            seen[(ny as u32 * level.width + nx as u32) as usize] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
                for s in &level.stairs {
                    let (ax, ay) = s.approach();
                    assert!(seen[(ay * level.width + ax) as usize], "{what}: {:?} stair cut off by crystals", s.kind);
                }
                assert!(level.stairs.iter().any(|s| s.kind == StairKind::Down));
            }
        }
    }

    #[test]
    fn wall_crystals_and_geodes_grow_from_a_wall_and_face_out_of_it() {
        for seed in 1..=6 {
            let level = cavern(seed, 5);
            for c in level.crystals.iter().filter(|c| c.surface == CrystalSurface::Wall) {
                // Out from the wall is open floor and into the wall is rock.
                let (ox, oy) = (c.yaw.sin().round(), c.yaw.cos().round());
                let (bx, by) = (c.x + ox * 0.25, c.y + oy * 0.25);
                assert!(level.is_open(bx.floor() as i32, by.floor() as i32), "seed {seed}: a wall crystal faces into rock");
                let (ix, iy) = (c.x - ox * 0.25, c.y - oy * 0.25);
                assert!(!level.is_open(ix.floor() as i32, iy.floor() as i32), "seed {seed}: a wall crystal grows from open floor");
                if c.growth == Growth::Geode {
                    assert!(c.up < 20.0, "geodes sit at hand height");
                } else {
                    assert!(c.up >= WALL_FROM, "wall crystals grow over a delver's head");
                }
            }
        }
    }

    #[test]
    fn ceiling_crystals_hang_clear_of_the_floor() {
        for seed in 1..=4 {
            let level = cavern(seed, 5);
            for c in level.crystals.iter().filter(|c| c.surface == CrystalSurface::Ceiling) {
                assert_eq!(c.up, level.ceiling);
                assert!(c.axis()[1] < -0.9, "hangs down");
                let mesh = armory::Item::Crystal(crystal_state(c)).build();
                let (lo, hi) = mesh.bounds().unwrap();
                // Its tip is at least 25 units over the floor, well above a delver's head.
                assert!(level.ceiling - (hi[1] - lo[1]) > 25.0, "seed {seed}: a ceiling crystal hangs to {}", level.ceiling - (hi[1] - lo[1]));
            }
        }
    }

    #[test]
    fn every_crystal_state_is_within_the_armorys_limits() {
        for seed in 1..=4 {
            for c in &cavern(seed, 5).crystals {
                let state = crystal_state(c);
                assert!(state.out_of_range(&crystal::LIMITS).is_empty());
                assert_eq!(state.crystal.stone, c.stone);
                assert_eq!(state.crystal.growth, c.growth);
            }
        }
    }
}
