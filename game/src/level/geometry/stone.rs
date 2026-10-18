//! Dressed stone for levels of stone halls, as plain mesh data like the rest of [`super`]:
//! ashlar walls in courses of squared blocks with chamfered joints, flagstone floors, vaulted
//! ceilings (a barrel over each colonnaded hall, groins over the other rooms, a flat soffit
//! over corridors), and the columns that stand in the halls.
//!
//! The stone is true to the grid, with none of the rock's relief. Each surface is a closed
//! *shell* on the cell planes (wall, floor and ceiling share their edge vertices, so nothing
//! leaks at a seam) with the dressed stone laid over it: wall blocks and flagstones stand a
//! few centimetres proud of the shell, so they never share a plane with it, and the joints
//! between them show the shell as dark mortar. Walls tile across cells because their joints
//! are laid along whole *runs* of wall (every cell face in a line), not cell by cell, and
//! they turn corners and doorways by stopping short of a perpendicular wall.
//!
//! The ceiling is one height field over the level ([`Vaults`]), so the shell's walls, which
//! rise to it, meet it along the same points.

use std::collections::HashMap;

use armory::{
    ItemMesh, MaterialKind, Rgb,
    build::{
        parts::{Look, Parts},
        plank::plank,
    },
    geom::{LatheOptions, MeshBuilder, Shading, V3, lathe, value_noise3, vec3},
};
use fb_level::{CELL, Cell, DoorStyle, Facing, Level, LevelStyle, WallFinish};
use procgen_rng::Mulberry32;

use super::{
    CEILING_STONE, CellView, DIRECTIONS, FLOOR_STONE, Face, Ink, LevelMesh, Relief, SEGMENTS, WALL_STONE, door_frame_in,
    facing_vector,
};

/// The highest a crown can stand over the spring, which is the level's ceiling.
const MAX_RISE: f32 = 26.0;

/// The gap between neighbouring blocks, and the chamfer on a block's face edges.
const JOINT: f32 = 0.3;
const CHAMFER: f32 = 0.28;
/// How far a wall block's face stands proud of the shell, at least and at most.
const PROUD: (f32, f32) = (0.45, 0.85);
/// A wall block's length along a course.
const BLOCK_LENGTH: (f32, f32) = (5.5, 11.5);
/// Rubble: the height of a course, a stone's length, how far it stands proud and how big its
/// chamfer is, the gap between stones, how much shorter than its course a stone may fall, the
/// share of stones that are two, and how high the base ink floats to clear the lowest course.
const RUBBLE_COURSE: (f32, f32) = (1.6, 4.4);
const RUBBLE_LENGTH: (f32, f32) = (1.8, 9.0);
const RUBBLE_PROUD: (f32, f32) = (0.2, 1.6);
const RUBBLE_CHAMFER: (f32, f32) = (0.25, 0.8);
const RUBBLE_JOINT: f32 = 0.35;
const RUBBLE_DROP: f32 = 0.2;
const RUBBLE_SPLIT: f32 = 0.15;
/// How far in a rubble stone's corners may be drawn.
const RUBBLE_CUT: f32 = 0.8;
/// The shares of rubble stones that stop short for a pinning stone and that are through-stones.
const RUBBLE_PIN: f32 = 0.22;
const RUBBLE_TALL: f32 = 0.12;
const RUBBLE_BASE_INK: f32 = 1.9;
/// Blocks stop this far short of a wall that meets theirs at an inside corner, clear of the
/// perpendicular wall's own blocks.
const INSIDE_CORNER: f32 = 1.0;
/// Wall blocks stop this far under the soffit, leaving the shell bare along the seam where the ink runs.
const UNDER_SOFFIT: f32 = 0.45;
/// Flagstones stand this high off the shell, at least and at most, with a chamfer of [`FLAG_CHAMFER`].
const FLAG_RAISE: (f32, f32) = (0.12, 0.26);
const FLAG_CHAMFER: f32 = 0.09;
/// The lowest course of wall is darker where the damp climbs it, and may be chipped.
pub const DAMP_STONE: Rgb = Rgb::hex(0x676a67);
/// How often a block of the lowest course has a corner knocked off, and how big a bite, in world units.
const CHIP_CHANCE: f32 = 0.3;
const CHIP: (f32, f32) = (1.1, 2.1);
/// The fine ink along a joint sits this far off the shell, down in the groove between blocks.
const JOINT_INK_LIFT: f32 = 0.1;
/// The ink along the base of a wall floats this high and this far off the shell, to clear the
/// lowest course in front of it and the flagstones beneath.
const BASE_INK: (f32, f32) = (0.3, 0.95);
/// Ink on a vault's ribs and creases stands this far off the soffit, to clear the grid the
/// ceiling is drawn on, which cuts across a crease.
const RIB_INK_LIFT: f32 = 0.4;
/// Ink on an upright corner stands this far off the shell, clear of the blocks either side.
const CORNER_INK_LIFT: f32 = 0.12;

/// The colour of a column, a shade lighter than the wall so it reads against it.
pub const COLUMN_STONE: Rgb = Rgb::hex(0x8c8e88);

/// A frame on a plane: `origin`, axes `a` and `b` along it and `n` out of it.
#[derive(Clone, Copy, Debug)]
struct Frame {
    origin: V3,
    a: V3,
    b: V3,
    n: V3,
}

impl Frame {
    fn at(&self, a: f32, b: f32, depth: f32) -> V3 {
        vec3::add(self.origin, vec3::add(vec3::add(vec3::scale(self.a, a), vec3::scale(self.b, b)), vec3::scale(self.n, depth)))
    }
}

/// Appends a dressed block over `ar` by `br` of `frame`, its face `depth` proud of the plane
/// with a chamfer `ch` all round. Faces are flat and hard-edged; with `skirt` the sides run
/// down to the plane, so a block beside a lower one shows a face rather than a gap. Texture
/// coordinates are the frame's axes in world space, a cell to one, so the stone's grain carries
/// across blocks.
fn slab(mesh: &mut MeshBuilder, f: &Frame, ar: [f32; 2], br: [f32; 2], depth: f32, ch: f32, skirt: bool) {
    slab_rough(mesh, f, ar, br, depth, ch, skirt, [0.0; 8]);
}

/// As [`slab`], with each corner of the block drawn inward along both axes by its share of `cut`
/// (`a` then `b` for the corners in the order low-low, high-low, high-high, low-high), so the
/// block is an irregular quadrilateral and the shell shows in the extra gaps round it.
#[allow(clippy::too_many_arguments)]
fn slab_rough(mesh: &mut MeshBuilder, f: &Frame, ar: [f32; 2], br: [f32; 2], depth: f32, ch: f32, skirt: bool, cut: [f32; 8]) {
    let uv = |p: V3| [vec3::dot(p, f.a) / CELL, vec3::dot(p, f.b) / CELL];
    let mut quad = |corners: [V3; 4], facing: V3| {
        mesh.push_flat_quad(corners, corners.map(uv), facing);
    };
    let ([a0, a1], [b0, b1]) = (ar, br);
    let rim = depth - ch;
    // The block's edge corners at the plane's height `d`, drawn in by `inset` more, and the
    // face's at `depth`.
    let corner = |i: usize, d: f32, inset: f32| {
        let (sa, sb) = ([1.0, -1.0, -1.0, 1.0][i], [1.0, 1.0, -1.0, -1.0][i]);
        let (a, b) = ([a0, a1, a1, a0][i], [b0, b0, b1, b1][i]);
        f.at(a + sa * (cut[2 * i] + inset), b + sb * (cut[2 * i + 1] + inset), d)
    };
    let edge = |i: usize| corner(i, rim, 0.0);
    let face = |i: usize| corner(i, depth, ch);
    let foot = |i: usize| corner(i, 0.0, 0.0);
    quad([face(0), face(1), face(2), face(3)], f.n);
    // The four chamfers, from the block's edge at `rim` up to its face.
    quad([edge(0), edge(1), face(1), face(0)], vec3::sub(f.n, f.b));
    quad([edge(3), edge(2), face(2), face(3)], vec3::add(f.n, f.b));
    quad([edge(0), edge(3), face(3), face(0)], vec3::sub(f.n, f.a));
    quad([edge(1), edge(2), face(2), face(1)], vec3::add(f.n, f.a));
    if skirt && rim > 0.02 {
        quad([foot(0), foot(1), edge(1), edge(0)], vec3::neg(f.b));
        quad([foot(3), foot(2), edge(2), edge(3)], f.b);
        quad([foot(0), foot(3), edge(3), edge(0)], vec3::neg(f.a));
        quad([foot(1), foot(2), edge(2), edge(1)], f.a);
    }
}

// ---------------------------------------------------------------------------------------------
// Vaults
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Vault {
    /// One barrel along the room's long side: the crown rises across the short.
    Barrel { along_x: bool },
    /// Square bays, each a pair of crossing barrels meeting in groins.
    Groin { bays: [u32; 2] },
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RoomVault {
    lo: [f32; 2],
    hi: [f32; 2],
    vault: Vault,
    rise: f32,
    /// Where the vault springs from: the level's ceiling.
    spring: f32,
}

impl RoomVault {
    /// The ceiling over the point `(x, z)` inside the room.
    fn crown(&self, x: f32, z: f32) -> f32 {
        // A shallow parabola across a span: zero at its springs and one at the middle.
        let arch = |t: f32| 1.0 - t * t;
        let local = |p: f32, lo: f32, hi: f32, n: f32| {
            let f = ((p - lo) / (hi - lo) * n).clamp(0.0, n - 1e-4);
            2.0 * (f - f.floor()) - 1.0
        };
        let shape = match self.vault {
            Vault::Barrel { along_x } => {
                let (p, lo, hi) = if along_x { (z, self.lo[1], self.hi[1]) } else { (x, self.lo[0], self.hi[0]) };
                arch(local(p, lo, hi, 1.0))
            }
            Vault::Groin { bays } => {
                let u = local(x, self.lo[0], self.hi[0], bays[0] as f32);
                let v = local(z, self.lo[1], self.hi[1], bays[1] as f32);
                arch(u).max(arch(v))
            }
        };
        self.spring + self.rise * shape
    }
}

/// The level's ceiling as a height over the floor plan. Corridors are flat at the level's
/// ceiling height, which every vault springs from; a room is vaulted over its floor, and the
/// cell of wall a doorway cuts through slopes from the room's vault to the corridor's soffit,
/// which the doorway's masonry hides the join of.
#[derive(Clone, Debug, PartialEq)]
pub struct Vaults {
    rooms: Vec<RoomVault>,
    spring: f32,
}

impl Vaults {
    pub fn new(level: &Level) -> Self {
        let spring = level.ceiling;
        let mut rooms = Vec::new();
        for room in &level.rooms {
            let lo = [room.x as f32 * CELL, room.y as f32 * CELL];
            let hi = [(room.x + room.w) as f32 * CELL, (room.y + room.h) as f32 * CELL];
            let columned = (room.y..room.y + room.h)
                .any(|y| (room.x..room.x + room.w).any(|x| level.cell(x as i32, y as i32) == Cell::Column));
            let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
            rooms.push(if columned {
                let span = w.min(h) * 0.5;
                RoomVault { lo, hi, vault: Vault::Barrel { along_x: room.w >= room.h }, rise: (span * 0.28).clamp(8.0, MAX_RISE), spring }
            } else {
                let bays = [((room.w as f32 / 5.0).round() as u32).max(1), ((room.h as f32 / 5.0).round() as u32).max(1)];
                let half = (w / bays[0] as f32).min(h / bays[1] as f32) * 0.5;
                RoomVault { lo, hi, vault: Vault::Groin { bays }, rise: (half * 0.3).clamp(5.0, 16.0), spring }
            });
        }
        Vaults { rooms, spring }
    }

    /// The ceiling's height over world `(x, z)`: the vault of the room the point is in, or
    /// failing that the highest of the slopes down to the soffit from the vaults of the rooms
    /// within a cell. A room's own height is its crown (no cells outside it), and another room's
    /// slope has fallen to the soffit by the next cell on, so over a room this is that room's
    /// however close its neighbour stands, and the height is continuous all the way across.
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let mut top = self.spring;
        for room in &self.rooms {
            let (cx, cz) = (x.clamp(room.lo[0], room.hi[0]), z.clamp(room.lo[1], room.hi[1]));
            let outside = (x - cx).abs().max((z - cz).abs());
            if outside > CELL {
                continue;
            }
            let crown = room.crown(cx, cz);
            top = top.max(crown + (self.spring - crown) * outside / CELL);
        }
        top
    }

    /// Only the rooms that can set the ceiling anywhere within `reach` of the box `lo..hi` over
    /// the floor plan (a room's slope falls to the soffit a cell beyond it), so a ceiling asked
    /// about again and again in one place need not visit every room.
    pub fn near(&self, lo: [f32; 2], hi: [f32; 2], reach: f32) -> Vaults {
        let rooms = self
            .rooms
            .iter()
            .filter(|r| r.lo[0] - CELL - reach <= hi[0] && r.hi[0] + CELL + reach >= lo[0] && r.lo[1] - CELL - reach <= hi[1] && r.hi[1] + CELL + reach >= lo[1])
            .copied()
            .collect();
        Vaults { rooms, spring: self.spring }
    }

    /// The soffit's height, and where every vault springs from.
    pub fn spring(&self) -> f32 {
        self.spring
    }

    /// The lines a vault is built along, as polylines over the floor plan: each barrel's arches
    /// across it every couple of cells, and each groin bay's two creases and the arches
    /// between bays.
    pub fn ribs(&self) -> Vec<Vec<[f32; 2]>> {
        let mut out = Vec::new();
        // A line from `a` to `b` in steps no longer than a cell's grid step.
        let straight = |a: [f32; 2], b: [f32; 2]| {
            let steps = (((b[0] - a[0]).hypot(b[1] - a[1])) / (CELL / SEGMENTS as f32)).ceil().max(1.0) as usize;
            (0..=steps).map(|i| [a[0] + (b[0] - a[0]) * i as f32 / steps as f32, a[1] + (b[1] - a[1]) * i as f32 / steps as f32]).collect::<Vec<_>>()
        };
        for room in &self.rooms {
            let (lo, hi) = (room.lo, room.hi);
            match room.vault {
                Vault::Barrel { along_x } => {
                    let (axis, cross) = if along_x { (0, 1) } else { (1, 0) };
                    let arches = (((hi[axis] - lo[axis]) / (CELL * 2.0)).round() as usize).max(1);
                    for i in 0..=arches {
                        let t = lo[axis] + (hi[axis] - lo[axis]) * i as f32 / arches as f32;
                        let (mut a, mut b) = ([0.0; 2], [0.0; 2]);
                        (a[axis], b[axis], a[cross], b[cross]) = (t, t, lo[cross], hi[cross]);
                        out.push(straight(a, b));
                    }
                }
                Vault::Groin { bays } => {
                    let (bw, bh) = ((hi[0] - lo[0]) / bays[0] as f32, (hi[1] - lo[1]) / bays[1] as f32);
                    for j in 0..bays[1] {
                        for i in 0..bays[0] {
                            let (x0, z0) = (lo[0] + bw * i as f32, lo[1] + bh * j as f32);
                            out.push(straight([x0, z0], [x0 + bw, z0 + bh]));
                            out.push(straight([x0 + bw, z0], [x0, z0 + bh]));
                        }
                    }
                    for i in 1..bays[0] {
                        let x = lo[0] + bw * i as f32;
                        out.push(straight([x, lo[1]], [x, hi[1]]));
                    }
                    for j in 1..bays[1] {
                        let z = lo[1] + bh * j as f32;
                        out.push(straight([lo[0], z], [hi[0], z]));
                    }
                }
            }
        }
        out
    }

    /// The ceiling's downward normal at `(x, z)`.
    pub fn normal(&self, x: f32, z: f32) -> V3 {
        let e = 0.75;
        let dx = (self.height(x + e, z) - self.height(x - e, z)) / (2.0 * e);
        let dz = (self.height(x, z + e) - self.height(x, z - e)) / (2.0 * e);
        vec3::normalize_or([dx, -1.0, dz], [0.0, -1.0, 0.0])
    }

    /// The highest the ceiling stands over a cell.
    pub fn highest_over(&self, x: i32, y: i32) -> f32 {
        let mut top = 0.0f32;
        for j in 0..=SEGMENTS {
            for i in 0..=SEGMENTS {
                top = top.max(self.height(grid(x, i), grid(y, j)));
            }
        }
        top
    }
}

/// The ink along a vault's ribs and creases, for the part of them over the cells `in_range`
/// takes: each stretch between two samples is drawn by whichever range holds its middle, so
/// the ribs of neighbouring ranges meet and none is drawn twice.
fn vault_ribs(vaults: &Vaults, in_range: &impl Fn(i32, i32) -> bool, ink: &mut Ink) {
    let at = |p: [f32; 2]| [p[0], vaults.height(p[0], p[1]), p[1]];
    for rib in vaults.ribs() {
        let mut run: Vec<V3> = Vec::new();
        let flush = |run: &mut Vec<V3>, ink: &mut Ink| {
            let normals: Vec<V3> = run.iter().map(|p| vaults.normal(p[0], p[2])).collect();
            ink.ribbon_lifted(run, &normals, RIB_INK_LIFT);
            run.clear();
        };
        for pair in rib.windows(2) {
            let middle = [(pair[0][0] + pair[1][0]) * 0.5, (pair[0][1] + pair[1][1]) * 0.5];
            let (cx, cy) = ((middle[0] / CELL).floor() as i32, (middle[1] / CELL).floor() as i32);
            if !in_range(cx, cy) {
                flush(&mut run, ink);
                continue;
            }
            if run.is_empty() {
                run.push(at(pair[0]));
            }
            run.push(at(pair[1]));
        }
        flush(&mut run, ink);
    }
}

/// World coordinate of the `i`th of a cell's [`SEGMENTS`] grid steps along an axis. Whole
/// numbers all the way, so a wall's top and the ceiling beside it name the same point.
fn grid(cell: i32, i: usize) -> f32 {
    cell as f32 * CELL + CELL * i as f32 / SEGMENTS as f32
}

/// The ceiling over world `(x, z)` in `level`: the vaulted stone's of a level of stone halls,
/// the flat `level.ceiling` of any other. What hangs from the ceiling hangs from this.
pub fn ceiling_at(level: &Level, x: f32, z: f32) -> f32 {
    match level.style {
        LevelStyle::StoneHalls => Vaults::new(level).height(x, z),
        LevelStyle::RoughRock => level.ceiling,
    }
}

/// The courses every wall in a level is laid in, `(bottom, height)` from the floor up past the
/// highest crown: about eight to the `spring`, so a flat wall ends on a course, then on. Shared
/// by the doorways' masonry so it is coursed with the walls.
fn courses(seed: u32, spring: f32) -> Vec<(f32, f32)> {
    let mut rng = Mulberry32::new(seed ^ 0xc0a2_5e55);
    let count = ((spring / 3.75).round() as usize).max(4);
    let raw: Vec<f32> = (0..count).map(|_| 0.85 + 0.3 * rng.range_f32(0.0, 1.0)).collect();
    let total: f32 = raw.iter().sum();
    let mut out = Vec::new();
    let mut y = 0.0;
    for r in raw {
        let h = spring * r / total;
        out.push((y, h));
        y += h;
    }
    while y < spring + MAX_RISE + 4.0 {
        let h = 3.75 * (0.85 + 0.3 * rng.range_f32(0.0, 1.0));
        out.push((y, h));
        y += h;
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Walls
// ---------------------------------------------------------------------------------------------

/// A line of cell faces looking the same way with nothing between them: one stretch of wall.
#[derive(Clone, Copy, Debug)]
struct Run {
    start: (i32, i32),
    cells: i32,
    /// Along the wall with the room on the left; `n` into the room.
    u: V3,
    n: V3,
    /// Where the run begins, on the floor, on the wall's plane.
    origin: V3,
    key: u32,
    /// How far the first and last blocks stop short of their ends.
    margin: [f32; 2],
}

impl Run {
    fn frame(&self) -> Frame {
        Frame { origin: self.origin, a: self.u, b: [0.0, 1.0, 0.0], n: self.n }
    }

    fn length(&self) -> f32 {
        self.cells as f32 * CELL
    }

    fn point(&self, s: f32) -> V3 {
        vec3::add(self.origin, vec3::scale(self.u, s))
    }
}

fn unit_step(v: V3) -> (i32, i32) {
    (v[0].round() as i32, v[2].round() as i32)
}

/// The directions along a wall facing `facing` (room on its left) and into the room.
fn wall_axes(facing: Facing) -> (V3, V3) {
    let inward = vec3::neg(facing_vector(facing));
    (vec3::cross([0.0, 1.0, 0.0], inward), inward)
}

/// The stone of one stretch of the level, for [`super::level_mesh`] on a level of stone halls.
pub fn hall_mesh(view: &CellView, (x0, y0, x1, y1): (i32, i32, i32, i32)) -> LevelMesh {
    let level = view.level();
    let vaults = Vaults::new(level);
    let courses = courses(level.seed, level.ceiling);
    let in_range = |x: i32, y: i32| x >= x0 && x < x1 && y >= y0 && y < y1;
    // Columns stand in drawn cells: the stone is laid under and over them.
    let drawn = |x: i32, y: i32| view.cell(x, y) != Cell::Rock;
    let has_face = |x: i32, y: i32, f: Facing| drawn(x, y) && view.cell(x + f.step().0, y + f.step().1) == Cell::Rock;

    let (mut floor, mut ceiling, mut walls, mut damp) = (MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new());
    let mut cobbles = super::cobble::Stones::default();
    let (mut ink, mut joints) = (Ink::default(), Ink::default());
    let flat = Relief::new(level);
    let up = [0.0, 1.0, 0.0];
    for y in y0..y1 {
        for x in x0..x1 {
            if !drawn(x, y) {
                continue;
            }
            let corner = [x as f32 * CELL, 0.0, y as f32 * CELL];
            // Where a stair goes through (`crate::level::stairs`), no floor under a way down and
            // no vault over a way up, as in the rough rock.
            let stair = level.stair_at(x, y);
            if !crate::level::stairs::opens_floor(stair) {
                Face {
                    origin: corner,
                    u: [1.0, 0.0, 0.0],
                    v: [0.0, 0.0, 1.0],
                    size: [CELL, CELL],
                    steps: [SEGMENTS, SEGMENTS],
                    facing: up,
                }
                .build(&flat, &mut floor);
                if view.is_cobbled(x, y) {
                    let open = view.neighbours_open(x, y);
                    super::cobble::cobbles(level.seed, x, y, &|_, _| 0.0, &|px, pz| view.floor_wear(x, y, open, px, pz), &mut cobbles);
                } else {
                    flagstones(level.seed, x, y, &mut floor);
                }
            }
            if !crate::level::stairs::opens_ceiling(stair) {
                vault_cell(&vaults, x, y, &mut ceiling);
            }

            for facing in DIRECTIONS {
                if !has_face(x, y, facing) {
                    continue;
                }
                let (u, _) = wall_axes(facing);
                let (ux, uz) = unit_step(u);
                // Upright ink where the wall turns a corner rather than running on.
                let corners = [!has_face(x - ux, y - uz, facing), !has_face(x + ux, y + uz, facing)];
                wall_shell(&vaults, (x, y), facing, corners, &mut walls, &mut ink);
                // Each run is laid once per stretch: from its first face in this range.
                if !(in_range(x - ux, y - uz) && has_face(x - ux, y - uz, facing)) {
                    let run = find_run(view, (x, y), facing, level.seed, &has_face);
                    let owns = |s: f32| {
                        let i = (s / CELL).floor() as i32;
                        in_range(run.start.0 + ux * i, run.start.1 + uz * i)
                    };
                    // A block is rubble or ashlar by the cell it starts in.
                    let rubble_at = |s: f32| {
                        let i = ((s / CELL).floor() as i32).clamp(0, run.cells - 1);
                        view.finishes().at(run.start.0 + ux * i, run.start.1 + uz * i).is_some_and(|f| f.walls == WallFinish::Rubble)
                    };
                    lay_blocks(&run, &vaults, &courses, &owns, &rubble_at, (&mut walls, &mut damp), &mut joints);
                }
            }
        }
    }
    vault_ribs(&vaults, &|x, y| in_range(x, y), &mut joints);
    let mut parts = Parts::new();
    let stone = |tint: Rgb| Look::of(MaterialKind::Stone).tinted(tint).without_outline();
    parts.add_look("floor", floor, stone(FLOOR_STONE));
    parts.add_look("walls", walls, stone(WALL_STONE));
    parts.add_look("damp", damp, stone(DAMP_STONE));
    cobbles.add_parts(&mut parts, super::cobble::stone_look);
    parts.add_look("ceiling", ceiling, stone(CEILING_STONE));
    let mut joints = joints.into_part();
    joints.name = "joints";
    LevelMesh { surfaces: parts.finish(), ink: ink.into_part(), joints }
}

/// The run of faces looking `facing` through `at`, found by walking out to both its ends.
fn find_run(view: &CellView, at: (i32, i32), facing: Facing, seed: u32, has_face: &impl Fn(i32, i32, Facing) -> bool) -> Run {
    let (u, n) = wall_axes(facing);
    let (ux, uz) = unit_step(u);
    let mut start = at;
    while has_face(start.0 - ux, start.1 - uz, facing) {
        start = (start.0 - ux, start.1 - uz);
    }
    let mut cells = 1;
    while has_face(start.0 + ux * cells, start.1 + uz * cells, facing) {
        cells += 1;
    }
    let d = facing_vector(facing);
    let centre = super::cell_centre(start.0, start.1);
    let origin = vec3::sub(vec3::add(centre, vec3::scale(d, CELL * 0.5)), vec3::scale(u, CELL * 0.5));
    // A run ends in rock where the wall turns an inside corner, and in the open where it turns
    // a jamb or an outside corner.
    let before = view.cell(start.0 - ux, start.1 - uz) == Cell::Rock;
    let after = view.cell(start.0 + ux * cells, start.1 + uz * cells) == Cell::Rock;
    let margin = [if before { INSIDE_CORNER } else { JOINT * 0.5 }, if after { INSIDE_CORNER } else { JOINT * 0.5 }];
    let key = seed
        ^ (facing as u32 + 1).wrapping_mul(0x9e37_79b1)
        ^ (start.0 as u32).wrapping_mul(0x85eb_ca6b)
        ^ (start.1 as u32).wrapping_mul(0xc2b2_ae35);
    Run { start, cells, u, n, origin, key, margin }
}

/// One cell's face of a wall: the shell from the floor to the ceiling, flat on the cell's
/// plane, and the ink along its top and its upright `corners` (start and end).
fn wall_shell(vaults: &Vaults, (x, y): (i32, i32), facing: Facing, corners: [bool; 2], walls: &mut MeshBuilder, ink: &mut Ink) {
    let (u, n) = wall_axes(facing);
    let (dx, dy) = facing.step();
    // The wall's grid points, in order along `u`: the plane is a whole number of cells, the
    // other axis steps with the ceiling's own grid.
    let point = |j: usize| -> (f32, f32) {
        let along = |cell: i32, forward: bool| if forward { grid(cell, j) } else { grid(cell, SEGMENTS - j) };
        if dx == 0 {
            (along(x, u[0] > 0.0), (y + (dy + 1) / 2) as f32 * CELL)
        } else {
            ((x + (dx + 1) / 2) as f32 * CELL, along(y, u[2] > 0.0))
        }
    };
    let uv = |p: V3| [vec3::dot(p, u) / CELL, p[1] / CELL];
    let tops: Vec<V3> = (0..=SEGMENTS)
        .map(|j| {
            let (px, pz) = point(j);
            [px, vaults.height(px, pz), pz]
        })
        .collect();
    for j in 0..SEGMENTS {
        let (a, b) = ([tops[j][0], 0.0, tops[j][2]], [tops[j + 1][0], 0.0, tops[j + 1][2]]);
        let quad = [a, b, tops[j + 1], tops[j]];
        walls.push_flat_quad(quad, quad.map(uv), n);
    }

    // Where the wall meets the ceiling, a ribbon on each.
    let on_ceiling: Vec<V3> = tops.iter().map(|p| vaults.normal(p[0], p[2])).collect();
    ink.ribbon(&tops, &vec![n; tops.len()]);
    ink.ribbon(&tops, &on_ceiling);
    for (j, corner) in [(0, corners[0]), (SEGMENTS, corners[1])] {
        if !corner {
            continue;
        }
        let top = tops[j];
        let rows = (top[1] / 4.0).ceil().max(1.0) as usize;
        let upright: Vec<V3> = (0..=rows).map(|r| [top[0], top[1] * r as f32 / rows as f32, top[2]]).collect();
        ink.ribbon_lifted(&upright, &vec![n; upright.len()], CORNER_INK_LIFT);
    }
}

/// The courses of rubble along a run, `(bottom, height)` from the floor to `top`: 1.6 to 4.4
/// units each, wandering. Drawn from the run's seed, not the level's, so each run's courses
/// meet its neighbours' only by chance, as rubble does.
fn rubble_courses(key: u32, top: f32) -> Vec<(f32, f32)> {
    let mut rng = Mulberry32::new(key ^ 0x7ab1_ec05);
    let mut out = Vec::new();
    let mut y = 0.0;
    while y < top {
        let h = rng.range_f32(RUBBLE_COURSE.0, RUBBLE_COURSE.1);
        out.push((y, h));
        y += h;
    }
    out
}

/// Lays the blocks of every course of `run` that this stretch owns: `owns` takes a block's
/// middle along the wall. The joints come from the run's own seed, never the stretch's, so
/// a block cut off by the stretch's edge is laid whole by the one that owns it. Each block
/// takes the masonry of the cell it starts in (`rubble_at` takes a position along the wall):
/// ashlar in the level's courses, or rubble in the run's own, so a run changes style at a cell
/// boundary. Blocks of the lowest course go to `damp`, a few with a corner chipped off; `joints`
/// takes the fine ink along the course lines and the joints between blocks.
fn lay_blocks(
    run: &Run,
    vaults: &Vaults,
    courses: &[(f32, f32)],
    owns: &impl Fn(f32) -> bool,
    rubble_at: &impl Fn(f32) -> bool,
    (mesh, damp): (&mut MeshBuilder, &mut MeshBuilder),
    joints: &mut Ink,
) {
    let len = run.length();
    let frame = run.frame();
    let height_at = |s: f32| {
        let p = run.point(s);
        vaults.height(p[0], p[2])
    };
    let line = |a: f32, b: f32, y: f32| [vec3::add(run.point(a), [0.0, y, 0.0]), vec3::add(run.point(b), [0.0, y, 0.0])];
    let tallest = (0..=(len / 2.0) as usize).map(|i| height_at(i as f32 * 2.0)).fold(0.0f32, f32::max);
    let any_rubble = (0..run.cells).any(|i| rubble_at((i as f32 + 0.5) * CELL));
    let rubble_courses = if any_rubble { rubble_courses(run.key, tallest) } else { Vec::new() };
    // The course `k` of a style, if it is below the highest crown.
    let course = |rubble: bool, k: usize| -> Option<(f32, f32)> {
        let list = if rubble { &rubble_courses } else { courses };
        list.get(k).copied().filter(|&(y0, _)| y0 + 1.4 < tallest)
    };
    let count = courses.len().max(rubble_courses.len());
    for k in 0..count {
        if course(false, k).is_none() && course(true, k).is_none() {
            break;
        }
        let mut rng = Mulberry32::new(run.key ^ (k as u32).wrapping_mul(0x9e37_79b1));
        // Rubble draws from a stream of its own, so ashlar lays out as it always did.
        let mut rough = Mulberry32::new(run.key ^ (k as u32).wrapping_mul(0x9e37_79b1) ^ 0x5eed_7ab1);
        let mut s = 0.0;
        while s < len - 1e-3 {
            // Every block draws the same numbers, owned or not, so the course lays out
            // alike whichever stretch is asking.
            let (long, short_chance, short, proud) =
                (rng.range_f32(BLOCK_LENGTH.0, BLOCK_LENGTH.1), rng.range_f32(0.0, 1.0), rng.range_f32(3.4, 4.8), rng.range_f32(PROUD.0, PROUD.1));
            let cut: [f32; 8] = std::array::from_fn(|_| rough.range_f32(0.0, RUBBLE_CUT));
            let (r_pin, r_gap, r_pin_y, r_tall) = (rough.range_f32(0.0, 1.0), rough.range_f32(1.3, 2.2), rough.range_f32(0.0, 1.0), rough.range_f32(0.0, 1.0));
            let (r_long, r_split, r_chamfer, r_drop, r_proud) = (
                rough.range_f32(RUBBLE_LENGTH.0, RUBBLE_LENGTH.1),
                rough.range_f32(0.0, 1.0),
                rough.range_f32(RUBBLE_CHAMFER.0, RUBBLE_CHAMFER.1),
                rough.range_f32(0.0, 1.0),
                rough.range_f32(RUBBLE_PROUD.0, RUBBLE_PROUD.1),
            );
            let rubble = rubble_at(s);
            let (mut l, absorb) = if rubble {
                // Many short stones and a few long ones.
                let u = (r_long - RUBBLE_LENGTH.0) / (RUBBLE_LENGTH.1 - RUBBLE_LENGTH.0);
                (RUBBLE_LENGTH.0 + (RUBBLE_LENGTH.1 - RUBBLE_LENGTH.0) * u.powf(1.7), RUBBLE_LENGTH.0)
            } else if short_chance < 0.12 {
                (short, 3.0)
            } else {
                (long, 3.0)
            };
            if len - (s + l) < absorb {
                l = len - s;
            }
            let (a, b) = (s, s + l);
            s = b;
            if !owns((a + b) * 0.5) {
                continue;
            }
            let Some((y0, h)) = course(rubble, k) else { continue };
            let (joint, proud) = if rubble { (RUBBLE_JOINT, r_proud) } else { (JOINT, proud) };
            let lo = a + if a < 1e-3 { run.margin[0] } else { joint * 0.5 };
            let mut hi = b - if b > len - 1e-3 { run.margin[1] } else { joint * 0.5 };
            // Now and then a long stone stops short, and a small pinning stone is wedged in the gap.
            let pin = (rubble && r_pin < RUBBLE_PIN && hi - lo > 4.5).then(|| {
                hi -= r_gap;
                [hi + joint, hi + r_gap]
            });
            let lowest = (0..=4).map(|i| height_at(lo + (hi - lo) * i as f32 / 4.0)).fold(f32::MAX, f32::min);
            // A rubble stone is a little shorter than its course, and a different amount each time.
            let drop = if rubble { r_drop * RUBBLE_DROP } else { 0.0 };
            // A few stones are through-stones, as tall as two courses and standing further out.
            let tall = rubble && k > 0 && r_tall < RUBBLE_TALL;
            let reach = if tall { course(true, k + 1).map_or(0.0, |(_, h2)| h2) } else { 0.0 };
            let proud = if reach > 0.0 { proud + 0.3 } else { proud };
            let (bottom, top) = (y0 + joint * 0.5, (y0 + h + reach - joint * 0.5 - drop).min(lowest - UNDER_SOFFIT));
            if hi - lo < 2.0 || top - bottom < 1.4 {
                continue;
            }
            // Only rubble has uneven chamfers and corners drawn in, and not so far as to leave
            // nothing of a small stone or a half of one.
            let dress = |out: &mut MeshBuilder, ar: [f32; 2], br: [f32; 2], depth: f32| {
                if !rubble {
                    return slab(out, &frame, ar, br, depth, CHAMFER, true);
                }
                let room = ((ar[1] - ar[0]).min(br[1] - br[0])).max(0.0);
                let chamfer = r_chamfer.min(depth * 0.95).min(room * 0.2);
                let tame = (room * 0.22 / RUBBLE_CUT).min(1.0);
                slab_rough(out, &frame, ar, br, depth, chamfer, true, cut.map(|c| c * tame));
            };
            // The ink in the joint below this block (the base of the wall for the lowest), and the
            // upright one before it.
            if k == 0 {
                let lift = if rubble { RUBBLE_BASE_INK } else { BASE_INK.1 };
                joints.ribbon_lifted(&line(lo, hi, BASE_INK.0), &[run.n; 2], lift);
            } else {
                joints.ribbon_lifted(&line(lo, hi, y0), &[run.n; 2], JOINT_INK_LIFT);
            }
            if a > 1e-3 {
                let at = |y: f32| vec3::add(run.point(a), [0.0, y, 0.0]);
                joints.ribbon_lifted(&[at(bottom), at(top)], &[run.n; 2], JOINT_INK_LIFT);
            }
            let out = if k == 0 { &mut *damp } else { &mut *mesh };
            if let Some(wedge) = pin {
                let height = ((top - bottom) * 0.5).min((wedge[1] - wedge[0]) * 1.4 + 0.4);
                let at = bottom + (top - bottom - height) * r_pin_y;
                dress(out, wedge, [at, at + height], proud * 0.7);
            }
            // Now and then a rubble stone is two, one on the other.
            if rubble && r_split < RUBBLE_SPLIT && top - bottom > 2.6 {
                let mid = (bottom + top) * 0.5 + (r_drop - 0.5) * 0.6;
                joints.ribbon_lifted(&line(lo, hi, mid), &[run.n; 2], JOINT_INK_LIFT);
                let (lower, upper) = ((bottom, mid - joint * 0.5), (mid + joint * 0.5, top));
                for (b0, b1) in [lower, upper] {
                    dress(out, [lo, hi], [b0, b1], proud);
                }
                continue;
            }
            if k > 0 {
                dress(out, [lo, hi], [bottom, top], proud);
                continue;
            }
            // Damp darkens the lowest course, and now and then the knocks of a floor's traffic
            // have taken a corner off a block.
            let mut luck = Mulberry32::new(run.key ^ (a * 64.0) as u32 ^ 0x0da4_9b10);
            let (chipped, size, left) = (luck.range_f32(0.0, 1.0) < CHIP_CHANCE, luck.range_f32(CHIP.0, CHIP.1), luck.range_f32(0.0, 1.0) < 0.5);
            if chipped && hi - lo > size * 2.5 && top - bottom > size * 2.0 {
                dress(damp, [lo, hi], [bottom + size, top], proud);
                let strip = if left { [lo + size, hi] } else { [lo, hi - size] };
                dress(damp, strip, [bottom, bottom + size], if rubble { proud * 0.6 } else { proud - 0.2 });
            } else {
                dress(damp, [lo, hi], [bottom, top], proud);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Floors and ceilings
// ---------------------------------------------------------------------------------------------

/// The ceiling over cell `(x, y)` as a [`SEGMENTS`] square grid on the height field.
fn vault_cell(vaults: &Vaults, x: i32, y: i32, mesh: &mut MeshBuilder) {
    let base = mesh.vertex_count() as u32;
    for j in 0..=SEGMENTS {
        for i in 0..=SEGMENTS {
            let (px, pz) = (grid(x, i), grid(y, j));
            mesh.push_vertex([px, vaults.height(px, pz), pz], vaults.normal(px, pz), [px / CELL, pz / CELL]);
        }
    }
    let at = |i: usize, j: usize| base + (j * (SEGMENTS + 1) + i) as u32;
    for j in 0..SEGMENTS {
        for i in 0..SEGMENTS {
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            mesh.push_triangle(a, b, c).push_triangle(a, c, d);
        }
    }
}

/// Splits `0..total` into `n` strips with their joints wandering up to `wander` of a strip off even.
fn split(total: f32, n: usize, rng: &mut Mulberry32, wander: f32) -> Vec<f32> {
    let step = total / n as f32;
    (0..=n).map(|i| if i == 0 || i == n { i as f32 * step } else { (i as f32 + rng.range_f32(-wander, wander)) * step }).collect()
}

/// Flagstones over cell `(x, y)`: two to four rows of two to four slabs of all sizes, the
/// joints wandering. A slab is now and then sunk, or cracked in two with one half lower.
fn flagstones(seed: u32, x: i32, y: i32, mesh: &mut MeshBuilder) {
    let mut rng = Mulberry32::new(seed ^ (x as u32).wrapping_mul(0x9e37_79b1) ^ (y as u32).wrapping_mul(0x85eb_ca6b) ^ 0xf1a6_5700);
    let frame = Frame { origin: [x as f32 * CELL, 0.0, y as f32 * CELL], a: [1.0, 0.0, 0.0], b: [0.0, 0.0, 1.0], n: [0.0, 1.0, 0.0] };
    let rows = 2 + (rng.range_f32(0.0, 1.0) * 2.6) as usize;
    let zs = split(CELL, rows, &mut rng, 0.34);
    for r in 0..rows {
        let cols = 2 + (rng.range_f32(0.0, 1.0) * 2.6) as usize;
        let xs = split(CELL, cols, &mut rng, 0.34);
        for c in 0..cols {
            let (sunk, cracked, at, drop) = (rng.range_f32(0.0, 1.0) < 0.1, rng.range_f32(0.0, 1.0) < 0.08, rng.range_f32(0.35, 0.65), rng.range_f32(0.04, 0.1));
            let raise = if sunk { rng.range_f32(0.02, 0.06) } else { rng.range_f32(FLAG_RAISE.0, FLAG_RAISE.1) };
            let inset = JOINT * 0.5;
            let (a0, a1, b0, b1) = (xs[c] + inset, xs[c + 1] - inset, zs[r] + inset, zs[r + 1] - inset);
            if !cracked || a1 - a0 < 3.0 || b1 - b0 < 3.0 {
                slab(mesh, &frame, [a0, a1], [b0, b1], raise, FLAG_CHAMFER.min(raise * 0.9), false);
                continue;
            }
            // The crack runs across the longer side, a hair wide, and one half has settled.
            let low = (raise - drop).max(0.03);
            if a1 - a0 >= b1 - b0 {
                let m = a0 + (a1 - a0) * at;
                slab(mesh, &frame, [a0, m - 0.12], [b0, b1], raise, FLAG_CHAMFER.min(raise * 0.9), false);
                slab(mesh, &frame, [m + 0.12, a1], [b0, b1], low, FLAG_CHAMFER.min(low * 0.9), false);
            } else {
                let m = b0 + (b1 - b0) * at;
                slab(mesh, &frame, [a0, a1], [b0, m - 0.12], raise, FLAG_CHAMFER.min(raise * 0.9), false);
                slab(mesh, &frame, [a0, a1], [m + 0.12, b1], low, FLAG_CHAMFER.min(low * 0.9), false);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Doorways
// ---------------------------------------------------------------------------------------------

/// A doorway's frame and masonry in a hall: the rock one's, with the infill coursed with the
/// walls and reaching up to the highest crown over the doorway's cell.
pub fn door_frame(level: &Level, (x, y): (i32, i32), style: DoorStyle, seed: u32) -> ItemMesh {
    let vaults = Vaults::new(level);
    let top = vaults.highest_over(x, y) + 1.5;
    let courses: Vec<(f32, f32)> = courses(level.seed, level.ceiling).into_iter().filter(|&(bottom, _)| bottom < top).collect();
    door_frame_in(style, &courses, top, CELL * 0.5 + 1.0, (JOINT, CHAMFER), seed)
}

// ---------------------------------------------------------------------------------------------
// Columns
// ---------------------------------------------------------------------------------------------

/// Half the width of a column's square plinth and abacus.
const COLUMN_HALF: f32 = 6.2;
/// How many different columns a level draws, taking turns.
pub const COLUMN_VARIANTS: u32 = 3;

/// A column standing in a hall's cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnSpot {
    pub cell: (i32, i32),
    /// Height of the capital's abacus top above the floor: the lowest the vault comes over the
    /// column's footprint.
    pub seat: f32,
    /// Height of the impost block that rises from the abacus to the highest the vault comes
    /// over the footprint, bedded a hair into it. The vault slopes across a column's width, so
    /// the block fills the wedge between the abacus and the soffit.
    pub top: f32,
    pub variant: u32,
    /// Quarter turns about the shaft, so a repeated column does not wear the same way on the same face.
    pub turn: u32,
}

/// Every column of the level, with how tall each stands.
pub fn columns(level: &Level) -> Vec<ColumnSpot> {
    let vaults = Vaults::new(level);
    let mut out = Vec::new();
    for y in 0..level.height as i32 {
        for x in 0..level.width as i32 {
            if level.cell(x, y) != Cell::Column {
                continue;
            }
            let (cx, cz) = ((x as f32 + 0.5) * CELL, (y as f32 + 0.5) * CELL);
            let over: Vec<f32> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (0.0, 0.0)]
                .iter()
                .map(|&(sx, sz)| vaults.height(cx + sx * COLUMN_HALF, cz + sz * COLUMN_HALF))
                .collect();
            // Whole decimetres, so columns of a hall share a mesh.
            let seat = over.iter().copied().fold(f32::MAX, f32::min).floor();
            let top = (over.iter().copied().fold(0.0, f32::max) + 0.3).ceil();
            let h = (x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663) ^ level.seed;
            out.push(ColumnSpot { cell: (x, y), seat, top, variant: (h >> 7) % COLUMN_VARIANTS, turn: (h >> 13) % 4 });
        }
    }
    out
}

/// A column from the floor to `top`: a square plinth, a round base, a shaft built of drums that
/// swells a little a third of the way up and narrows toward the top (entasis), and a capital of
/// an echinus under a square abacus whose top is at `seat`, with an impost block above it
/// running up to `top`. The shaft is worn smooth and chipped: `variant` and `seed` choose
/// where. In its own space the column stands on the origin.
pub fn column(seat: f32, top: f32, variant: u32, seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ variant.wrapping_mul(0x85eb_ca6b) ^ 0xc010_0000);
    let (plinth, abacus, echinus) = (1.6, 1.4, 2.4);
    let shaft_bottom = plinth + 1.5;
    let shaft_top = seat - abacus - echinus - 0.6;
    let (r_bottom, r_top) = (4.3 + rng.range_f32(0.0, 0.25), 3.6 + rng.range_f32(0.0, 0.2));
    let drums = (((shaft_top - shaft_bottom) / 9.0).round() as usize).max(3);
    let joints: Vec<f32> = (1..drums).map(|i| shaft_bottom + (shaft_top - shaft_bottom) * i as f32 / drums as f32).collect();

    // The turned body, bottom to top, as (radius, height).
    let mut profile: Vec<[f32; 2]> = vec![[0.0, plinth], [5.5, plinth], [5.6, plinth + 0.4], [5.3, plinth + 0.9], [4.9, plinth + 1.2]];
    let swell = |t: f32| 0.32 * (std::f32::consts::PI * t.powf(0.8)).sin();
    let shaft_radius = |y: f32| {
        let t = ((y - shaft_bottom) / (shaft_top - shaft_bottom)).clamp(0.0, 1.0);
        let dip = joints.iter().map(|&j| ((1.0 - ((y - j) / 0.5).powi(2)).max(0.0)) * 0.16).fold(0.0f32, f32::max);
        r_bottom + (r_top - r_bottom) * t + swell(t) - dip
    };
    let mut ys: Vec<f32> = Vec::new();
    let steps = ((shaft_top - shaft_bottom) / 1.5).ceil() as usize;
    for i in 0..=steps {
        ys.push(shaft_bottom + (shaft_top - shaft_bottom) * i as f32 / steps as f32);
    }
    for &j in &joints {
        ys.extend([j - 0.5, j - 0.25, j, j + 0.25, j + 0.5]);
    }
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    profile.extend(ys.iter().map(|&y| [shaft_radius(y), y]));
    // The necking ring, then the echinus flaring out under the abacus.
    let e = shaft_top;
    profile.extend([[r_top + 0.45, e + 0.2], [r_top + 0.45, e + 0.5], [r_top + 0.1, e + 0.7]]);
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        profile.push([r_top + 0.1 + (5.9 - r_top - 0.1) * (1.0 - (1.0 - t).powi(2)), e + 0.7 + (echinus - 0.1) * t]);
    }
    profile.push([5.9, seat - abacus]);
    profile.push([0.0, seat - abacus]);
    // Dense enough that chips and wear have vertices to move.
    let profile = densify(&profile, 1.0);
    let mut body = lathe(&profile, &LatheOptions { facets: 28, shading: Shading::SMOOTH, ..Default::default() });
    wear(&mut body, &mut rng, [shaft_bottom, shaft_top], r_bottom, seed ^ variant);

    let mut foot = plank(2.0 * COLUMN_HALF, plinth, 2.0 * COLUMN_HALF, 0.4);
    foot.translate([0.0, plinth * 0.5, 0.0]);
    body.append(&foot);
    let mut cap = plank(2.0 * COLUMN_HALF, top - seat + abacus, 2.0 * COLUMN_HALF, 0.4);
    cap.translate([0.0, (top + seat - abacus) * 0.5, 0.0]);
    body.append(&cap);

    let mut parts = Parts::new();
    parts.add_look("column", body, Look::of(MaterialKind::Stone).tinted(COLUMN_STONE));
    parts.finish()
}

/// `profile` with no step longer than `max`.
fn densify(profile: &[[f32; 2]], max: f32) -> Vec<[f32; 2]> {
    let mut out = vec![profile[0]];
    for pair in profile.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let pieces = (length / max).ceil().max(1.0) as usize;
        for i in 1..=pieces {
            let t = i as f32 / pieces as f32;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

/// Erodes the turned body: a slow wander of the surface, worse at the foot where the damp
/// sits, and a few chips knocked out of the shaft and the capital.
fn wear(body: &mut MeshBuilder, rng: &mut Mulberry32, [shaft_bottom, shaft_top]: [f32; 2], radius: f32, seed: u32) {
    let chips: Vec<(V3, f32, f32)> = (0..4 + (rng.range_f32(0.0, 3.0) as usize))
        .map(|_| {
            let (angle, y) = (rng.range_f32(0.0, std::f32::consts::TAU), rng.range_f32(shaft_bottom, shaft_top + 2.0));
            // Chips bite deeper low down, where a column takes its knocks.
            let low = 1.0 - ((y - shaft_bottom) / (shaft_top - shaft_bottom)).clamp(0.0, 1.0);
            ([angle.cos() * radius, y, angle.sin() * radius], rng.range_f32(1.1, 2.3), 0.25 + 0.45 * low * rng.range_f32(0.4, 1.0))
        })
        .collect();
    for p in &mut body.positions {
        let r = (p[0] * p[0] + p[2] * p[2]).sqrt();
        // The base moulding and the tops are left whole.
        if r < 1e-3 || p[1] < shaft_bottom - 0.2 {
            continue;
        }
        let (dx, dz) = (p[0] / r, p[2] / r);
        let foot = (-(p[1] - shaft_bottom).max(0.0) / 6.0).exp();
        let mut sink = (value_noise3([p[0] / 3.0, p[1] / 5.0, p[2] / 3.0], seed) - 0.5) * (0.3 + 0.5 * foot);
        for &(centre, size, depth) in &chips {
            let d = vec3::distance(*p, centre);
            if d < size {
                sink += depth * (1.0 - d / size).powi(2);
            }
        }
        p[0] -= dx * sink;
        p[2] -= dz * sink;
    }
    weld_normals(body);
}

/// Normals as the area-weighted mean over the triangles meeting at each place, so a seam in
/// the vertex list (the lathe's, where it closes) does not show as a crease.
fn weld_normals(mesh: &mut MeshBuilder) {
    let key = |p: V3| p.map(|c| (c * 200.0).round() as i64);
    let mut sums: HashMap<[i64; 3], V3> = HashMap::new();
    for tri in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| mesh.positions[i as usize]);
        let face = vec3::cross(vec3::sub(b, a), vec3::sub(c, a));
        for p in [a, b, c] {
            let sum = sums.entry(key(p)).or_insert(vec3::ZERO);
            *sum = vec3::add(*sum, face);
        }
    }
    for (p, n) in mesh.positions.iter().zip(&mut mesh.normals) {
        if let Some(sum) = sums.get(&key(*p)) {
            *n = vec3::normalize_or(*sum, *n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_level::LevelStyle;

    /// A stone halls level without its stairs, whose openings in the floor and vault are
    /// tested on their own ([`stairs_open_the_floor_and_the_vault`]).
    fn halls(seed: u32) -> Level {
        let level = fb_level::generate(seed, 2);
        assert_eq!(level.style, LevelStyle::StoneHalls);
        Level { stairs: Vec::new(), ..level }
    }

    #[test]
    fn stairs_open_the_floor_and_the_vault() {
        let level = fb_level::generate(1207, 2);
        let view = CellView::new(&level);
        let mesh = level_mesh(&view, view.bounds());
        let over = |name: &str, (x, y): (u32, u32)| {
            let (x0, z0) = (x as f32 * CELL, y as f32 * CELL);
            part(&mesh, name).positions.iter().any(|p| p[0] > x0 + 0.5 && p[0] < x0 + CELL - 0.5 && p[2] > z0 + 0.5 && p[2] < z0 + CELL - 0.5)
        };
        let down = level.stair(fb_level::StairKind::Down).expect("a way down");
        let up = level.stair(fb_level::StairKind::Up).expect("a way up");
        assert!(!over("floor", (down.x, down.y)), "no floor over the way down");
        assert!(!over("ceiling", (up.x, up.y)), "no vault over the way up");
        assert!(over("ceiling", (down.x, down.y)), "the vault still covers the way down");
    }

    fn key(p: &[f32; 3]) -> [i64; 3] {
        p.map(|c| (c * 1000.0).round() as i64)
    }

    fn part<'a>(mesh: &'a LevelMesh, name: &str) -> &'a armory::Part {
        mesh.surfaces.parts.iter().find(|p| p.name == name).unwrap()
    }

    fn level_mesh(view: &CellView, range: (i32, i32, i32, i32)) -> LevelMesh {
        super::super::level_mesh(view, range)
    }

    #[test]
    fn the_stone_is_deterministic_and_drawable() {
        let level = halls(1207);
        let view = CellView::uncobbled(&level);
        let a = level_mesh(&view, view.bounds());
        assert_eq!(a, level_mesh(&view, view.bounds()));
        assert_eq!(a.surfaces.parts.iter().map(|p| p.name).collect::<Vec<_>>(), ["floor", "walls", "damp", "ceiling"]);
        assert!(!a.joints.indices.is_empty());
        for p in a.surfaces.parts.iter().chain([&a.ink, &a.joints]) {
            p.validate().unwrap();
            assert!(p.normals.iter().all(|n| (vec3::length(*n) - 1.0).abs() < 1e-3), "{}: normals are unit", p.name);
        }
        let other = halls(1208);
        let view = CellView::new(&other);
        assert_ne!(a, level_mesh(&view, view.bounds()));
    }

    #[test]
    fn cobbles_replace_the_flagstones_in_a_cobbled_cell() {
        // A level with a timbered quarter, whose floors are cobbled.
        let level = (1..200).map(halls).find(|l| l.rooms.iter().any(|r| CellView::new(l).is_cobbled(r.x as i32, r.y as i32))).expect("some hall has a cobbled room");
        let view = CellView::new(&level);
        let mesh = level_mesh(&view, view.bounds());
        assert!(mesh.surfaces.parts.iter().any(|p| p.name.starts_with("stones_")));
        let flags = part(&level_mesh(&CellView::uncobbled(&level), view.bounds()), "floor").indices.len();
        assert_ne!(part(&mesh, "floor").indices.len(), flags);
        for p in &mesh.surfaces.parts {
            p.validate().unwrap();
        }
    }

    #[test]
    fn no_triangle_is_degenerate() {
        let level = halls(1207);
        let view = CellView::new(&level);
        let mesh = level_mesh(&view, view.bounds());
        for p in &mesh.surfaces.parts {
            for tri in p.indices.as_chunks::<3>().0 {
                let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| p.positions[i as usize]);
                let area = vec3::length(vec3::cross(vec3::sub(b, a), vec3::sub(c, a))) * 0.5;
                assert!(area > 1e-4, "{}: sliver of area {area}", p.name);
            }
        }
    }

    #[test]
    fn faces_look_into_the_open() {
        // From each triangle's middle a short step along its winding normal stays in drawn
        // floor, and floors face up and ceilings down.
        let level = halls(1207);
        let view = CellView::new(&level);
        let mesh = level_mesh(&view, view.bounds());
        for (name, toward) in [("walls", None), ("floor", Some(1.0f32)), ("ceiling", Some(-1.0))] {
            let p = part(&mesh, name);
            for tri in p.indices.as_chunks::<3>().0 {
                let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| p.positions[i as usize]);
                let face = vec3::normalize_or(vec3::cross(vec3::sub(b, a), vec3::sub(c, a)), [0.0; 3]);
                let centre = vec3::scale(vec3::add(vec3::add(a, b), c), 1.0 / 3.0);
                let probe = vec3::add(centre, vec3::scale(face, 0.05));
                let cell = view.cell((probe[0] / CELL).floor() as i32, (probe[2] / CELL).floor() as i32);
                assert_ne!(cell, Cell::Rock, "{name}: a face looks into rock at {centre:?}");
                if let Some(sign) = toward {
                    assert!(face[1] * sign > -0.01, "{name}: faces the wrong way at {centre:?}");
                }
            }
        }
    }

    #[test]
    fn walls_rise_to_the_ceiling_on_shared_points() {
        for seed in [1207, 7] {
            let level = halls(seed);
            let view = CellView::new(&level);
            let vaults = Vaults::new(&level);
            let mesh = level_mesh(&view, view.bounds());
            let ceiling: std::collections::HashSet<_> = part(&mesh, "ceiling").positions.iter().map(key).collect();
            let tops: Vec<_> = part(&mesh, "walls").positions.iter().filter(|p| p[1] > 1.0 && (p[1] - vaults.height(p[0], p[2])).abs() < 1e-4).collect();
            assert!(tops.len() > 100);
            assert!(tops.iter().all(|p| ceiling.contains(&key(p))), "seed {seed}: a wall top is off the ceiling's grid");
        }
    }

    #[test]
    fn only_the_floor_shell_lies_in_the_floors_plane() {
        let level = halls(1207);
        let view = CellView::uncobbled(&level);
        let mesh = level_mesh(&view, view.bounds());
        let floor = part(&mesh, "floor");
        let (x0, y0, x1, y1) = view.bounds();
        let cells = (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| view.cell(x, y) != Cell::Rock).count();
        let in_plane = floor.indices.as_chunks::<3>().0.iter().filter(|tri| tri.iter().all(|&i| floor.positions[i as usize][1] == 0.0)).count();
        assert_eq!(in_plane, cells * SEGMENTS * SEGMENTS * 2);
        assert!(floor.positions.iter().all(|p| p[1] >= 0.0 && p[1] <= FLAG_RAISE.1 + 1e-4));
    }

    #[test]
    fn wall_blocks_stand_proud_of_the_shell() {
        let level = halls(1207);
        let view = CellView::new(&level);
        let mesh = level_mesh(&view, view.bounds());
        let walls = part(&mesh, "walls");
        // A triangle facing along a wall's normal either is the shell, whole on a cell plane
        // and reaching the floor, or stands off it. Blocks have no face of their own in the plane.
        let on_plane = |p: &[f32; 3], axis: usize| (p[axis] / CELL - (p[axis] / CELL).round()).abs() < 1e-5;
        let (mut proud, mut shell) = (0, 0);
        for tri in walls.indices.as_chunks::<3>().0 {
            let ps = [tri[0], tri[1], tri[2]].map(|i| walls.positions[i as usize]);
            let n = vec3::normalize_or(vec3::cross(vec3::sub(ps[1], ps[0]), vec3::sub(ps[2], ps[0])), [0.0; 3]);
            let axis = if n[0].abs() > 0.99 { 0 } else if n[2].abs() > 0.99 { 2 } else { continue };
            if ps.iter().all(|p| on_plane(p, axis)) {
                shell += 1;
                assert!(ps.iter().any(|p| p[1] == 0.0) || ps.iter().all(|p| p[1] > 0.0), "a stone face lies in the shell's plane");
            } else {
                proud += 1;
            }
        }
        assert!(proud > 1000 && shell > 100, "{proud} {shell}");
    }

    #[test]
    fn walls_tile_the_same_whichever_stretch_lays_them() {
        // Laid whole, or in chunks that cut across the runs, every block comes out the same.
        for seed in [1207, 7] {
            let level = halls(seed);
            let view = CellView::new(&level);
            let whole = level_mesh(&view, view.bounds());
            let mut all: Vec<[i64; 3]> = part(&whole, "walls").positions.iter().chain(&part(&whole, "damp").positions).map(key).collect();
            let mut pieces: Vec<[i64; 3]> = Vec::new();
            let mut fine: Vec<[i64; 3]> = Vec::new();
            for range in super::super::chunks(&view, 5) {
                let piece = level_mesh(&view, range);
                pieces.extend(piece.surfaces.parts.iter().filter(|p| p.name == "walls" || p.name == "damp").flat_map(|p| p.positions.iter().map(key)));
                fine.extend(piece.joints.positions.iter().map(key));
            }
            all.sort_unstable();
            pieces.sort_unstable();
            assert_eq!(all.len(), pieces.len(), "seed {seed}");
            assert_eq!(all, pieces, "seed {seed}");
            // A rib cut by a chunk's edge ends there and the next chunk's begins, so the joints'
            // points are the same set, with the ends of cut ribs drawn twice.
            let mut whole_fine: Vec<[i64; 3]> = whole.joints.positions.iter().map(key).collect();
            whole_fine.sort_unstable();
            whole_fine.dedup();
            fine.sort_unstable();
            fine.dedup();
            assert_eq!(whole_fine, fine, "seed {seed}: joints");
        }
    }

    #[test]
    fn rubble_is_rougher_than_ashlar_and_tiles_across_stretches() {
        fn with(level: &Level, rubble_chance: f64) -> CellView<'_> {
            // The chances weigh the quarters' characters against the plain one: all rubble needs a
            // weight that swamps it, and half of them one near its own.
            let rubble_chance = if rubble_chance >= 1.0 { 1e6 } else if rubble_chance > 0.0 { 0.4 } else { 0.0 };
            let params = fb_level::LevelParams { rubble_chance, cobble_chance: 0.0, arch_chance: 0.0, ..fb_level::LevelParams::for_style(level.style) };
            CellView::with_params(level, &params)
        }
        let walls_triangles = |m: &LevelMesh| ["walls", "damp"].iter().map(|n| part(m, n).indices.len() / 3).sum::<usize>();
        for seed in [1207, 7, 31] {
            let level = halls(seed);
            let (ashlar, rubble, mixed) = (with(&level, 0.0), with(&level, 1.0), with(&level, 0.5));
            let (a, r) = (level_mesh(&ashlar, ashlar.bounds()), level_mesh(&rubble, rubble.bounds()));
            let styles: std::collections::HashSet<_> = (0..level.height as i32).flat_map(|y| (0..level.width as i32).map(move |x| (x, y))).filter_map(|(x, y)| mixed.finishes().at(x, y)).map(|f| f.walls).collect();
            assert!(styles.contains(&WallFinish::Rubble) && styles.contains(&WallFinish::Ashlar), "seed {seed}: the mixed view has both styles");
            // Smaller stones in shorter courses: more of them over the same walls.
            assert!(walls_triangles(&r) as f32 > 1.15 * walls_triangles(&a) as f32, "seed {seed}: {} against {}", walls_triangles(&r), walls_triangles(&a));
            for p in &r.surfaces.parts {
                p.validate().unwrap();
            }
            // Blocks change style at a cell boundary, and laid in chunks come out the same.
            let whole = level_mesh(&mixed, mixed.bounds());
            let mut all: Vec<[i64; 3]> = ["walls", "damp"].iter().flat_map(|n| part(&whole, n).positions.iter().map(key)).collect();
            let mut pieces: Vec<[i64; 3]> = Vec::new();
            for range in super::super::chunks(&mixed, 5) {
                let piece = level_mesh(&mixed, range);
                pieces.extend(piece.surfaces.parts.iter().filter(|p| p.name == "walls" || p.name == "damp").flat_map(|p| p.positions.iter().map(key)));
            }
            all.sort_unstable();
            pieces.sort_unstable();
            assert_eq!(all, pieces, "seed {seed}");
        }
    }

    #[test]
    fn blocks_never_reach_into_rock() {
        // Off a cell plane a vertex is a block's, standing out from its wall, so it is in
        // drawn floor, not in the rock behind the wall.
        let level = halls(1207);
        let view = CellView::new(&level);
        let mesh = level_mesh(&view, view.bounds());
        for p in &part(&mesh, "walls").positions {
            let on_x = (p[0] / CELL - (p[0] / CELL).round()).abs() < 1e-5;
            let on_z = (p[2] / CELL - (p[2] / CELL).round()).abs() < 1e-5;
            if on_x && on_z {
                continue;
            }
            let (cx, cz) = ((p[0] / CELL).floor() as i32, (p[2] / CELL).floor() as i32);
            // Skirts end on the plane itself, which is the edge of rock; a block's own vertex
            // is clear of it, in the cell its face looks into.
            let inside = view.cell(cx, cz) != Cell::Rock || (on_x && view.cell(cx - 1, cz) != Cell::Rock) || (on_z && view.cell(cx, cz - 1) != Cell::Rock);
            assert!(inside, "a block vertex lies in rock at {p:?}");
        }
    }

    #[test]
    fn every_hall_has_its_columns_and_they_fit_their_cells() {
        let level = halls(1207);
        let spots = columns(&level);
        let counted = level.cells.iter().filter(|&&c| c == Cell::Column).count();
        assert_eq!(spots.len(), counted);
        assert!(counted > 0);
        let vaults = Vaults::new(&level);
        for spot in spots {
            let column = column(spot.seat, spot.top, spot.variant, level.seed);
            for p in &column.parts {
                p.validate().unwrap();
                assert!(p.normals.iter().all(|n| (vec3::length(*n) - 1.0).abs() < 1e-3));
            }
            let (lo, hi) = column.bounds().unwrap();
            // Inside the cell, thinner than it.
            assert!(lo[0] >= -CELL * 0.5 && hi[0] <= CELL * 0.5 && lo[2] >= -CELL * 0.5 && hi[2] <= CELL * 0.5);
            assert!(hi[0] - lo[0] < CELL * 0.7);
            assert!(lo[1] >= 0.0 && (hi[1] - spot.top).abs() < 1e-3);
            // It reaches the vault without poking far through it.
            let (cx, cz) = ((spot.cell.0 as f32 + 0.5) * CELL, (spot.cell.1 as f32 + 0.5) * CELL);
            for (sx, sz) in [(-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (0.0, 0.0)] {
                let ceiling = vaults.height(cx + sx * COLUMN_HALF, cz + sz * COLUMN_HALF);
                assert!(spot.seat <= ceiling + 1e-3 && spot.top >= ceiling, "seat {} and top {} about ceiling {ceiling}", spot.seat, spot.top);
            }
        }
    }

    #[test]
    fn a_column_swells_then_narrows_and_is_chipped() {
        let made = column(34.0, 36.0, 1, 1207);
        let body = &made.parts[0];
        let radius_near = |y: f32| {
            let r: Vec<f32> = body.positions.iter().filter(|p| (p[1] - y).abs() < 0.8 && p[0].hypot(p[2]) > 2.0 && p[0].hypot(p[2]) < 5.4).map(|p| p[0].hypot(p[2])).collect();
            r.iter().sum::<f32>() / r.len() as f32
        };
        let (low, third, top) = (radius_near(5.0), radius_near(13.0), radius_near(24.0));
        assert!(third > low - 0.15 && third > top, "entasis: {low} {third} {top}");
        // The chips and the wander make the shaft uneven round its circumference.
        let ring: Vec<f32> = body.positions.iter().filter(|p| (p[1] - 6.0).abs() < 0.8 && p[0].hypot(p[2]) < 5.4).map(|p| p[0].hypot(p[2])).collect();
        let (lo, hi) = ring.iter().fold((f32::MAX, f32::MIN), |(l, h), &r| (l.min(r), h.max(r)));
        assert!(hi - lo > 0.1, "{lo} {hi}");
        // The same column twice, and a different variant another.
        assert_eq!(made, column(34.0, 36.0, 1, 1207));
        assert_ne!(made, column(34.0, 36.0, 2, 1207));
    }

    #[test]
    fn doorway_masonry_reaches_the_vault() {
        let level = halls(1207);
        let view = CellView::new(&level);
        let vaults = Vaults::new(&level);
        let doors = super::super::doorways(&view);
        assert!(!doors.is_empty());
        for door in doors {
            let frame = door_frame(&level, (door.x, door.y), DoorStyle::Timber, level.seed);
            for p in &frame.parts {
                p.validate().unwrap();
            }
            let highest = frame.parts[0].positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
            assert!(highest >= vaults.highest_over(door.x, door.y), "masonry stops short of the vault at ({}, {})", door.x, door.y);
        }
    }

    #[test]
    fn the_vault_is_continuous_and_springs_from_the_corridors() {
        // Finely stepped over many levels along both axes, so a seam between two rooms a single
        // cell of rock apart would show as a jump.
        for seed in 0..8 {
            let level = halls(seed);
            let vaults = Vaults::new(&level);
            let spring = level.ceiling;
            assert_eq!(vaults.spring(), spring);
            for z in (0..level.height * 80).step_by(5) {
                for x in (0..level.width * 80).step_by(7) {
                    let (px, pz) = (x as f32 * 0.25 + 0.037, z as f32 * 0.25 + 0.011);
                    let (a, b, c) = (vaults.height(px, pz), vaults.height(px + 0.1, pz), vaults.height(px, pz + 0.1));
                    assert!((a - b).abs() < 0.2, "seed {seed}: ceiling jumps {a} to {b} at ({px}, {pz})");
                    assert!((a - c).abs() < 0.2, "seed {seed}: ceiling jumps {a} to {c} at ({px}, {pz})");
                    assert!((spring - 1e-3..=spring + MAX_RISE + 1e-3).contains(&a), "{a}");
                }
            }
        }
    }

    #[test]
    fn a_room_keeps_its_own_vault_beside_a_neighbour() {
        // A second room a single cell of rock from the first leaves the ceiling over the first as
        // it was alone, right up to the edge, and the ceiling continuous across the rock between.
        let mut level = halls(1207);
        level.rooms.truncate(1);
        let alone = Vaults::new(&level);
        let a = level.rooms[0];
        let mut b = a;
        b.x = a.x + a.w + 1;
        level.rooms.push(b);
        let both = Vaults::new(&level);
        for step in 0..=40 {
            let z = (a.y as f32 + a.h as f32 * step as f32 / 40.0) * CELL;
            for back in [0.0, 0.5, 1.0, 3.0] {
                let x = (a.x + a.w) as f32 * CELL - back;
                assert!((both.height(x, z) - alone.height(x, z)).abs() < 1e-4, "room a's own ceiling at ({x}, {z})");
            }
            let edge = (a.x + a.w) as f32 * CELL;
            for k in 0..40 {
                let x = edge + CELL * 2.0 * k as f32 / 40.0;
                assert!((both.height(x, z) - both.height(x + 0.05, z)).abs() < 0.2, "a jump between the rooms at ({x}, {z})");
            }
        }
    }

    #[test]
    fn the_courses_end_on_the_spring_and_reach_the_crowns() {
        for spring in [30.0, 40.0] {
            let courses = courses(1207, spring);
            let to_spring = (spring / 3.75).round() as usize;
            let (bottom, height) = courses[to_spring - 1];
            assert!((bottom + height - spring).abs() < 1e-3);
            let (bottom, height) = *courses.last().unwrap();
            assert!(bottom + height >= spring + MAX_RISE);
            assert!(courses.iter().all(|&(_, h)| h > 3.0 && h < 5.2));
            assert!(courses.windows(2).all(|w| (w[0].0 + w[0].1 - w[1].0).abs() < 1e-4));
        }
    }

    #[test]
    fn the_ceiling_is_the_vault_in_halls_and_flat_in_rock() {
        let level = halls(1207);
        let vaults = Vaults::new(&level);
        for (x, z) in [(10.0, 10.0), (200.0, 150.0), (400.0, 300.0)] {
            assert_eq!(ceiling_at(&level, x, z), vaults.height(x, z));
        }
        let rock = fb_level::generate(1207, 1);
        assert_eq!(ceiling_at(&rock, 200.0, 150.0), rock.ceiling);
    }

    #[test]
    fn rough_rock_levels_draw_no_dressed_stone() {
        let level = fb_level::generate(1207, 1);
        assert_eq!(level.style, LevelStyle::RoughRock);
        assert!(columns(&level).is_empty());
    }
}
