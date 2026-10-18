//! Corner shading baked into the level's stone: how shut in each vertex is, worked out from the
//! floor plan alone (no rays), and stored on the part (`Part::occlusion`) for the toon shader to
//! scale the banded light by (`docs/rendering.md`, "Corner shading").
//!
//! The level is a signed distance field: the nearest of the floor, the ceiling and the walls
//! (the edges of the cells that are not open), positive in the air and negative in the rock. A
//! vertex looks along the field's gradient, which points out of the nearest stone, and asks how
//! much slower the field grows there than it would from a flat surface: a flat wall gains a
//! full step for each step out (nothing shut in), a crease between wall and floor gains only
//! 0.7 of it, an inner corner of three surfaces 0.58. Measuring the *gain* rather than the
//! distance leaves out the relief the rock wanders by and the blocks that stand proud of a wall,
//! and measuring along the gradient rather than the normal keeps a block's sides, which face
//! along the wall, from reading as creases. Taken at three reaches, so a crease is dark close in
//! and the dark fades out across a few decimetres. All of it is a pure function of the level.

use std::{collections::HashMap, hash::{BuildHasherDefault, Hasher}};

use armory::{Part, geom::V3};
use fb_level::{CELL, Level, LevelStyle, StairKind};

use super::{CellView, stone::Vaults};

/// How far out from a vertex the field is read, in world units (15, 30 and 60 cm), and how much
/// each reach counts.
const REACHES: [(f32, f32); 3] = [(1.5, 0.45), (3.0, 0.35), (6.0, 0.2)];
/// How much of the light a fully shut-in point loses, and the least a vertex is ever left.
/// Subtle on purpose: it should read as the stone meeting, not as a shadow.
pub const CORNER_SHADE: f32 = 0.9;
const LEAST_OPEN: f32 = 0.4;
/// The furthest a sample strays from its vertex, with a little over.
const REACH: f32 = 8.0;
/// Step for the field's gradient, in world units.
const GRADIENT_STEP: f32 = 0.4;

/// A cheap multiplicative hash for the points: the default is built to resist attack, which a
/// table of the level's own vertices has no need of.
#[derive(Default)]
struct Fx(u64);

impl Hasher for Fx {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }

    fn write_u32(&mut self, n: u32) {
        self.write_u64(n as u64);
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

/// The level as a distance field over the cells of one stretch and a margin, for
/// [`Enclosure::open_at`].
pub struct Enclosure<'a> {
    level: &'a Level,
    /// The vaults over a level of stone halls, cell by cell as the open map is, each holding only
    /// the rooms near it; rough rock has a flat ceiling.
    vaults: Vec<Vaults>,
    /// Which cells are open, over the drawn cells and a margin of rock: the field asks about
    /// nine cells per sample, too many to look up through the view's passage each time.
    open: Vec<bool>,
    origin: (i32, i32),
    width: i32,
    /// The cells a stair goes through: no floor under a way down, no ceiling over a way up.
    shafts_down: Vec<(i32, i32)>,
    shafts_up: Vec<(i32, i32)>,
}

/// Cells of rock kept round the drawn ones in the open map, more than a sample's reach.
const MARGIN: i32 = 2;

impl<'a> Enclosure<'a> {
    pub fn new(view: &'a CellView<'a>, (x0, y0, x1, y1): (i32, i32, i32, i32)) -> Self {
        let level: &Level = view.level();
        let (origin, width, height) = ((x0 - MARGIN, y0 - MARGIN), x1 - x0 + 2 * MARGIN, y1 - y0 + 2 * MARGIN);
        let mut open = vec![false; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                open[(y * width + x) as usize] = view.is_open(origin.0 + x, origin.1 + y);
            }
        }
        let vaults = if level.style == LevelStyle::StoneHalls {
            let all = Vaults::new(level);
            (0..height)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .map(|(x, y)| {
                    let lo = [(origin.0 + x) as f32 * CELL, (origin.1 + y) as f32 * CELL];
                    all.near(lo, [lo[0] + CELL, lo[1] + CELL], REACH)
                })
                .collect()
        } else {
            Vec::new()
        };
        let shafts = |kind| level.stairs.iter().filter(|s| s.kind == kind).map(|s| (s.x as i32, s.y as i32)).collect();
        Enclosure {
            level,
            vaults,
            open,
            origin,
            width,
            shafts_down: shafts(StairKind::Down),
            shafts_up: shafts(StairKind::Up),
        }
    }

    fn is_open(&self, x: i32, y: i32) -> bool {
        let (lx, ly) = (x - self.origin.0, y - self.origin.1);
        lx >= 0 && lx < self.width && ly >= 0 && self.open.get((ly * self.width + lx) as usize).copied().unwrap_or(false)
    }

    fn ceiling(&self, x: f32, z: f32) -> f32 {
        let (lx, ly) = ((x / CELL).floor() as i32 - self.origin.0, (z / CELL).floor() as i32 - self.origin.1);
        match self.vaults.get((ly * self.width + lx) as usize) {
            Some(vaults) if lx >= 0 && lx < self.width && ly >= 0 => vaults.height(x, z),
            _ => self.level.ceiling,
        }
    }

    /// Distance from `(x, z)` to the walls on the floor plan: positive in the open, negative
    /// inside rock, at most a cell either way.
    fn plan(&self, x: f32, z: f32) -> f32 {
        let (cx, cz) = ((x / CELL).floor() as i32, (z / CELL).floor() as i32);
        let inside_open = self.is_open(cx, cz);
        let mut nearest = CELL;
        for dz in -1..=1 {
            for dx in -1..=1 {
                let (nx, nz) = (cx + dx, cz + dz);
                // Looking for the other kind of cell than the one the point is in.
                if self.is_open(nx, nz) == inside_open {
                    continue;
                }
                let (lo_x, lo_z) = (nx as f32 * CELL, nz as f32 * CELL);
                let gap_x = (lo_x - x).max(x - (lo_x + CELL)).max(0.0);
                let gap_z = (lo_z - z).max(z - (lo_z + CELL)).max(0.0);
                nearest = nearest.min((gap_x * gap_x + gap_z * gap_z).sqrt());
            }
        }
        if inside_open { nearest } else { -nearest }
    }

    /// The signed distance to the nearest stone, floor and ceiling included.
    fn distance(&self, p: V3) -> f32 {
        let cell = ((p[0] / CELL).floor() as i32, (p[2] / CELL).floor() as i32);
        let mut nearest = self.plan(p[0], p[2]);
        // Where a stair goes through there is no floor, or no ceiling, to meet.
        if !self.shafts_down.contains(&cell) {
            nearest = nearest.min(p[1]);
        }
        // No ceiling is lower than the level's, so a point further under it than the nearest
        // stone has no use for the vault's height, the costly part.
        if self.level.ceiling - p[1] < nearest && !self.shafts_up.contains(&cell) {
            nearest = nearest.min(self.ceiling(p[0], p[2]) - p[1]);
        }
        nearest
    }

    /// How open `p` is to the light: 1 away from any corner, down to [`LEAST_OPEN`] shut in a
    /// corner of floor, ceiling and walls.
    pub fn open_at(&self, p: V3, normal: V3) -> f32 {
        let here = self.distance(p);
        let step = |axis: usize| {
            let mut q = p;
            q[axis] += GRADIENT_STEP;
            self.distance(q) - here
        };
        let grad = [step(0), step(1), step(2)];
        let length = (grad[0] * grad[0] + grad[1] * grad[1] + grad[2] * grad[2]).sqrt();
        let out = if length > 1e-4 { [grad[0] / length, grad[1] / length, grad[2] / length] } else { normal };
        let mut shut = 0.0;
        for (reach, weight) in REACHES {
            let there = self.distance([p[0] + out[0] * reach, p[1] + out[1] * reach, p[2] + out[2] * reach]);
            shut += weight * (1.0 - (there - here) / reach).clamp(0.0, 1.0);
        }
        (1.0 - CORNER_SHADE * shut).clamp(LEAST_OPEN, 1.0)
    }

    /// Stores each vertex's openness on `part`.
    pub fn bake(&self, part: &mut Part) {
        // Hard-edged stone repeats each corner's point once a face, so each point is worked out once.
        let mut seen: HashMap<[u32; 3], f32, BuildHasherDefault<Fx>> = HashMap::default();
        part.occlusion = part
            .positions
            .iter()
            .zip(&part.normals)
            .map(|(&p, &n)| *seen.entry(p.map(f32::to_bits)).or_insert_with(|| self.open_at(p, n)))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::geometry::level_mesh;

    fn average(values: impl Iterator<Item = f32>) -> f32 {
        let (sum, n) = values.fold((0.0, 0), |(s, n), v| (s + v, n + 1));
        assert!(n > 0, "some vertices to average");
        sum / n as f32
    }

    fn key(p: &[f32; 3]) -> [u32; 3] {
        [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]
    }

    #[test]
    fn the_same_level_gives_the_same_values() {
        for (seed, depth) in [(1207, 1), (1207, 2), (7, 1)] {
            let level = fb_level::generate(seed, depth);
            let view = CellView::new(&level);
            let a = level_mesh(&view, view.bounds());
            assert_eq!(a, level_mesh(&view, view.bounds()));
            for part in &a.surfaces.parts {
                assert_eq!(part.occlusion.len(), part.positions.len(), "{}: one value a vertex", part.name);
                assert!(part.occlusion.iter().all(|o| (LEAST_OPEN..=1.0).contains(o)), "{}: within range", part.name);
            }
            // A stretch of the level is shaded as the whole is: the values are of the place.
            let (x0, y0, x1, y1) = view.bounds();
            let half = level_mesh(&view, (x0, y0, (x0 + x1) / 2, y1));
            let whole: std::collections::HashMap<[u32; 3], f32> =
                a.surfaces.parts.iter().flat_map(|p| p.positions.iter().zip(&p.occlusion)).map(|(p, &o)| (key(p), o)).collect();
            let (mut matched, mut total) = (0, 0);
            for part in &half.surfaces.parts {
                for (p, o) in part.positions.iter().zip(&part.occlusion) {
                    total += 1;
                    if let Some(same) = whole.get(&key(p)) {
                        matched += 1;
                        assert_eq!(o, same, "{}: a chunk and the whole agree at {p:?}", part.name);
                    }
                }
            }
            assert!(total > 0 && matched * 10 >= total * 9, "seed {seed} depth {depth}: {matched} of {total} vertices of the half found in the whole");
        }
    }

    /// What the baking adds to building every chunk of a level, in thread CPU time. Run with
    /// `cargo test -p forever_below --release occlusion_cost -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing, for release builds"]
    fn occlusion_cost() {
        for (seed, depth) in [(1207, 1), (1207, 2), (7, 1), (7, 2)] {
            let level = fb_level::generate(seed, depth);
            let view = CellView::new(&level);
            let chunks = crate::level::geometry::chunks(&view, crate::level::CHUNK_CELLS);
            let time = |bake: bool| {
                let start = crate::thread_cpu_seconds();
                for &range in &chunks {
                    if bake {
                        level_mesh(&view, range);
                    } else {
                        crate::level::geometry::bare_level_mesh(&view, range);
                    }
                }
                crate::thread_cpu_seconds() - start
            };
            let (bare, baked) = (time(false), time(true));
            let vertices: usize = chunks.iter().map(|&r| level_mesh(&view, r).surfaces.parts.iter().map(|p| p.positions.len()).sum::<usize>()).sum();
            println!("seed {seed} depth {depth}: {vertices} vertices, bare {:.1} ms, baked {:.1} ms, added {:.1} ms", bare * 1e3, baked * 1e3, (baked - bare) * 1e3);
        }
    }

    #[test]
    fn a_corner_is_darker_than_mid_wall() {
        for depth in [1, 2] {
            let level = fb_level::generate(1207, depth);
            let view = CellView::new(&level);
            let mesh = level_mesh(&view, view.bounds());
            let walls = mesh.surfaces.parts.iter().find(|p| p.name == "walls").unwrap();
            let rows = walls.positions.iter().zip(&walls.occlusion);
            let seam = average(rows.clone().filter(|(p, _)| p[1] < 0.6).map(|(_, &o)| o));
            let mid = level.ceiling * 0.5;
            let middle = average(rows.filter(|(p, _)| (p[1] - mid).abs() < 2.0).map(|(_, &o)| o));
            assert!(seam < middle - 0.1, "depth {depth}: the seam ({seam}) is darker than mid-wall ({middle})");
            assert!(middle > 0.9, "depth {depth}: mid-wall is left alone ({middle})");
        }
    }

    #[test]
    fn an_inner_corner_is_darker_than_a_wall_run() {
        let level = fb_level::generate(1207, 1);
        let view = CellView::new(&level);
        let enclosure = Enclosure::new(&view, view.bounds());
        let (x0, y0, x1, y1) = view.bounds();
        // An open cell with rock to the east and south and air to the north and west: an inner
        // corner of two walls. Beside its post, mid-height, is darker than mid-wall.
        let (cx, cy) = (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .find(|&(x, y)| {
                view.is_open(x, y) && view.is_open(x - 1, y) && view.is_open(x, y - 1) && !view.is_open(x + 1, y) && !view.is_open(x, y + 1)
            })
            .expect("a level has an inner corner");
        let height = level.ceiling * 0.5;
        let (east, south) = ((cx + 1) as f32 * CELL, (cy + 1) as f32 * CELL);
        let corner = enclosure.open_at([east - 0.2, height, south - 0.2], [0.0, 0.0, -1.0]);
        let flat = enclosure.open_at([east - 0.2, height, south - CELL * 0.5], [-1.0, 0.0, 0.0]);
        assert!(corner < flat - 0.05, "the corner ({corner}) is darker than the wall run ({flat})");
    }
}
