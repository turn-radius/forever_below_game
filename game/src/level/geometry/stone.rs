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
use fb_level::{CELL, Cell, Facing, Level};
use procgen_rng::Mulberry32;

use super::{
    CEILING_STONE, CellView, DIRECTIONS, FLOOR_STONE, Face, Ink, LevelMesh, Relief, SEGMENTS, WALL_STONE, door_frame_in,
    facing_vector,
};

/// Where every vault springs from, and the height of a corridor's flat soffit.
pub const SPRING: f32 = 30.0;
/// The highest a crown can stand over [`SPRING`].
const MAX_RISE: f32 = 26.0;

/// The gap between neighbouring blocks, and the chamfer on a block's face edges.
const JOINT: f32 = 0.3;
const CHAMFER: f32 = 0.28;
/// How far a wall block's face stands proud of the shell, at least and at most.
const PROUD: (f32, f32) = (0.45, 0.85);
/// A wall block's length along a course.
const BLOCK_LENGTH: (f32, f32) = (5.5, 11.5);
/// Blocks stop this far short of a wall that meets theirs at an inside corner, clear of the
/// perpendicular wall's own blocks.
const INSIDE_CORNER: f32 = 1.0;
/// Wall blocks stop this far under the soffit, leaving the shell bare along the seam where the ink runs.
const UNDER_SOFFIT: f32 = 0.45;
/// Flagstones stand this high off the shell, at least and at most, with a chamfer of [`FLAG_CHAMFER`].
const FLAG_RAISE: (f32, f32) = (0.12, 0.26);
const FLAG_CHAMFER: f32 = 0.09;
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
    let uv = |p: V3| [vec3::dot(p, f.a) / CELL, vec3::dot(p, f.b) / CELL];
    let mut quad = |corners: [V3; 4], facing: V3| {
        mesh.push_flat_quad(corners, corners.map(uv), facing);
    };
    let ([a0, a1], [b0, b1]) = (ar, br);
    let rim = depth - ch;
    quad([f.at(a0 + ch, b0 + ch, depth), f.at(a1 - ch, b0 + ch, depth), f.at(a1 - ch, b1 - ch, depth), f.at(a0 + ch, b1 - ch, depth)], f.n);
    // The four chamfers, from the block's edge at `rim` up to its face.
    quad([f.at(a0, b0, rim), f.at(a1, b0, rim), f.at(a1 - ch, b0 + ch, depth), f.at(a0 + ch, b0 + ch, depth)], vec3::sub(f.n, f.b));
    quad([f.at(a0, b1, rim), f.at(a1, b1, rim), f.at(a1 - ch, b1 - ch, depth), f.at(a0 + ch, b1 - ch, depth)], vec3::add(f.n, f.b));
    quad([f.at(a0, b0, rim), f.at(a0, b1, rim), f.at(a0 + ch, b1 - ch, depth), f.at(a0 + ch, b0 + ch, depth)], vec3::sub(f.n, f.a));
    quad([f.at(a1, b0, rim), f.at(a1, b1, rim), f.at(a1 - ch, b1 - ch, depth), f.at(a1 - ch, b0 + ch, depth)], vec3::add(f.n, f.a));
    if skirt && rim > 0.02 {
        quad([f.at(a0, b0, 0.0), f.at(a1, b0, 0.0), f.at(a1, b0, rim), f.at(a0, b0, rim)], vec3::neg(f.b));
        quad([f.at(a0, b1, 0.0), f.at(a1, b1, 0.0), f.at(a1, b1, rim), f.at(a0, b1, rim)], f.b);
        quad([f.at(a0, b0, 0.0), f.at(a0, b1, 0.0), f.at(a0, b1, rim), f.at(a0, b0, rim)], vec3::neg(f.a));
        quad([f.at(a1, b0, 0.0), f.at(a1, b1, 0.0), f.at(a1, b1, rim), f.at(a1, b0, rim)], f.a);
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
        SPRING + self.rise * shape
    }
}

/// The level's ceiling as a height over the floor plan. Corridors are flat at [`SPRING`]; a
/// room is vaulted over its floor, and the cell of wall a doorway cuts through slopes from the
/// room's vault to the corridor's soffit, which the doorway's masonry hides the join of.
#[derive(Clone, Debug, PartialEq)]
pub struct Vaults {
    rooms: Vec<RoomVault>,
}

impl Vaults {
    pub fn new(level: &Level) -> Self {
        let mut rooms = Vec::new();
        for room in &level.rooms {
            let lo = [room.x as f32 * CELL, room.y as f32 * CELL];
            let hi = [(room.x + room.w) as f32 * CELL, (room.y + room.h) as f32 * CELL];
            let columned = (room.y..room.y + room.h)
                .any(|y| (room.x..room.x + room.w).any(|x| level.cell(x as i32, y as i32) == Cell::Column));
            let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
            rooms.push(if columned {
                let span = w.min(h) * 0.5;
                RoomVault { lo, hi, vault: Vault::Barrel { along_x: room.w >= room.h }, rise: (span * 0.28).clamp(8.0, MAX_RISE) }
            } else {
                let bays = [((room.w as f32 / 5.0).round() as u32).max(1), ((room.h as f32 / 5.0).round() as u32).max(1)];
                let half = (w / bays[0] as f32).min(h / bays[1] as f32) * 0.5;
                RoomVault { lo, hi, vault: Vault::Groin { bays }, rise: (half * 0.3).clamp(5.0, 16.0) }
            });
        }
        Vaults { rooms }
    }

    /// The ceiling's height over world `(x, z)`.
    pub fn height(&self, x: f32, z: f32) -> f32 {
        for room in &self.rooms {
            let (cx, cz) = (x.clamp(room.lo[0], room.hi[0]), z.clamp(room.lo[1], room.hi[1]));
            let outside = (x - cx).abs().max((z - cz).abs());
            if outside > CELL {
                continue;
            }
            let crown = room.crown(cx, cz);
            return crown + (SPRING - crown) * outside / CELL;
        }
        SPRING
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

/// World coordinate of the `i`th of a cell's [`SEGMENTS`] grid steps along an axis. Whole
/// numbers all the way, so a wall's top and the ceiling beside it name the same point.
fn grid(cell: i32, i: usize) -> f32 {
    cell as f32 * CELL + CELL * i as f32 / SEGMENTS as f32
}

/// The courses every wall in a level is laid in, `(bottom, height)` from the floor up past the
/// highest crown: eight to the spring, so a flat wall ends on a course, then on. Shared by the
/// doorways' masonry so it is coursed with the walls.
fn courses(seed: u32) -> Vec<(f32, f32)> {
    let mut rng = Mulberry32::new(seed ^ 0xc0a2_5e55);
    let raw: Vec<f32> = (0..8).map(|_| 0.85 + 0.3 * rng.range_f32(0.0, 1.0)).collect();
    let total: f32 = raw.iter().sum();
    let mut out = Vec::new();
    let mut y = 0.0;
    for r in raw {
        let h = SPRING * r / total;
        out.push((y, h));
        y += h;
    }
    while y < SPRING + MAX_RISE + 4.0 {
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
    let courses = courses(level.seed);
    let in_range = |x: i32, y: i32| x >= x0 && x < x1 && y >= y0 && y < y1;
    // Columns stand in drawn cells: the stone is laid under and over them.
    let drawn = |x: i32, y: i32| view.cell(x, y) != Cell::Rock;
    let has_face = |x: i32, y: i32, f: Facing| drawn(x, y) && view.cell(x + f.step().0, y + f.step().1) == Cell::Rock;

    let (mut floor, mut ceiling, mut walls) = (MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new());
    let mut ink = Ink::default();
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
                flagstones(level.seed, x, y, &mut floor);
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
                    lay_blocks(&run, &vaults, &courses, &owns, &mut walls);
                }
            }
        }
    }
    let mut parts = Parts::new();
    let stone = |tint: Rgb| Look::of(MaterialKind::Stone).tinted(tint).without_outline();
    parts.add_look("floor", floor, stone(FLOOR_STONE));
    parts.add_look("walls", walls, stone(WALL_STONE));
    parts.add_look("ceiling", ceiling, stone(CEILING_STONE));
    LevelMesh { surfaces: parts.finish(), ink: ink.into_part() }
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

/// Lays the blocks of every course of `run` that this stretch owns: `owns` takes a block's
/// middle along the wall. The joints come from the run's own seed, never the stretch's, so
/// a block cut off by the stretch's edge is laid whole by the one that owns it.
fn lay_blocks(run: &Run, vaults: &Vaults, courses: &[(f32, f32)], owns: &impl Fn(f32) -> bool, mesh: &mut MeshBuilder) {
    let len = run.length();
    let frame = run.frame();
    let height_at = |s: f32| {
        let p = run.point(s);
        vaults.height(p[0], p[2])
    };
    let tallest = (0..=(len / 2.0) as usize).map(|i| height_at(i as f32 * 2.0)).fold(0.0f32, f32::max);
    for (k, &(y0, h)) in courses.iter().enumerate() {
        if y0 + 1.4 >= tallest {
            break;
        }
        let mut rng = Mulberry32::new(run.key ^ (k as u32).wrapping_mul(0x9e37_79b1));
        let mut s = 0.0;
        while s < len - 1e-3 {
            // Every block draws the same four numbers, owned or not, so the course lays out
            // alike whichever stretch is asking.
            let (long, short_chance, short, proud) =
                (rng.range_f32(BLOCK_LENGTH.0, BLOCK_LENGTH.1), rng.range_f32(0.0, 1.0), rng.range_f32(3.4, 4.8), rng.range_f32(PROUD.0, PROUD.1));
            let mut l = if short_chance < 0.12 { short } else { long };
            if len - (s + l) < 3.0 {
                l = len - s;
            }
            let (a, b) = (s, s + l);
            s = b;
            if !owns((a + b) * 0.5) {
                continue;
            }
            let lo = a + if a < 1e-3 { run.margin[0] } else { JOINT * 0.5 };
            let hi = b - if b > len - 1e-3 { run.margin[1] } else { JOINT * 0.5 };
            let lowest = (0..=4).map(|i| height_at(lo + (hi - lo) * i as f32 / 4.0)).fold(f32::MAX, f32::min);
            let (bottom, top) = (y0 + JOINT * 0.5, (y0 + h - JOINT * 0.5).min(lowest - UNDER_SOFFIT));
            if hi - lo < 2.0 || top - bottom < 1.4 {
                continue;
            }
            slab(mesh, &frame, [lo, hi], [bottom, top], proud, CHAMFER, true);
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

/// Splits `0..total` into `n` strips with their joints wandering a little off even.
fn split(total: f32, n: usize, rng: &mut Mulberry32) -> Vec<f32> {
    let step = total / n as f32;
    (0..=n).map(|i| if i == 0 || i == n { i as f32 * step } else { (i as f32 + rng.range_f32(-0.22, 0.22)) * step }).collect()
}

/// Flagstones over cell `(x, y)`: two or three rows of two or three slabs, the joints
/// wandering and the slabs a little uneven.
fn flagstones(seed: u32, x: i32, y: i32, mesh: &mut MeshBuilder) {
    let mut rng = Mulberry32::new(seed ^ (x as u32).wrapping_mul(0x9e37_79b1) ^ (y as u32).wrapping_mul(0x85eb_ca6b) ^ 0xf1a6_5700);
    let frame = Frame { origin: [x as f32 * CELL, 0.0, y as f32 * CELL], a: [1.0, 0.0, 0.0], b: [0.0, 0.0, 1.0], n: [0.0, 1.0, 0.0] };
    let rows = 2 + (rng.range_f32(0.0, 1.0) < 0.5) as usize;
    let zs = split(CELL, rows, &mut rng);
    for r in 0..rows {
        let cols = 2 + (rng.range_f32(0.0, 1.0) < 0.5) as usize;
        let xs = split(CELL, cols, &mut rng);
        for c in 0..cols {
            let raise = rng.range_f32(FLAG_RAISE.0, FLAG_RAISE.1);
            let inset = JOINT * 0.5;
            slab(mesh, &frame, [xs[c] + inset, xs[c + 1] - inset], [zs[r] + inset, zs[r + 1] - inset], raise, FLAG_CHAMFER, false);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Doorways
// ---------------------------------------------------------------------------------------------

/// A doorway's frame and masonry in a hall: the rock one's, with the infill coursed with the
/// walls and reaching up to the highest crown over the doorway's cell.
pub fn door_frame(level: &Level, (x, y): (i32, i32), seed: u32) -> ItemMesh {
    let vaults = Vaults::new(level);
    let top = vaults.highest_over(x, y) + 1.5;
    let courses: Vec<(f32, f32)> = courses(level.seed).into_iter().filter(|&(bottom, _)| bottom < top).collect();
    door_frame_in(&courses, top, CELL * 0.5 + 1.0, seed)
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
        let view = CellView::new(&level);
        let a = level_mesh(&view, view.bounds());
        assert_eq!(a, level_mesh(&view, view.bounds()));
        assert_eq!(a.surfaces.parts.iter().map(|p| p.name).collect::<Vec<_>>(), ["floor", "walls", "ceiling"]);
        for p in a.surfaces.parts.iter().chain([&a.ink]) {
            p.validate().unwrap();
            assert!(p.normals.iter().all(|n| (vec3::length(*n) - 1.0).abs() < 1e-3), "{}: normals are unit", p.name);
        }
        let other = halls(1208);
        let view = CellView::new(&other);
        assert_ne!(a, level_mesh(&view, view.bounds()));
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
        let view = CellView::new(&level);
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
            let mut all: Vec<[i64; 3]> = part(&whole, "walls").positions.iter().map(key).collect();
            let mut pieces: Vec<[i64; 3]> = Vec::new();
            for range in super::super::chunks(&view, 5) {
                let piece = level_mesh(&view, range);
                pieces.extend(piece.surfaces.parts.iter().filter(|p| p.name == "walls").flat_map(|p| p.positions.iter().map(key)));
            }
            all.sort_unstable();
            pieces.sort_unstable();
            assert_eq!(all.len(), pieces.len(), "seed {seed}");
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
            let frame = door_frame(&level, (door.x, door.y), level.seed);
            for p in &frame.parts {
                p.validate().unwrap();
            }
            let highest = frame.parts[0].positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
            assert!(highest >= vaults.highest_over(door.x, door.y), "masonry stops short of the vault at ({}, {})", door.x, door.y);
        }
    }

    #[test]
    fn the_vault_is_continuous_and_springs_from_the_corridors() {
        for seed in [1207, 7, 99] {
            let level = halls(seed);
            let vaults = Vaults::new(&level);
            for z in (0..level.height * 20).step_by(7) {
                for x in (0..level.width * 20).step_by(5) {
                    let (px, pz) = (x as f32 + 0.37, z as f32 + 0.11);
                    let (a, b) = (vaults.height(px, pz), vaults.height(px + 0.1, pz));
                    assert!((a - b).abs() < 0.2, "seed {seed}: ceiling jumps {a} to {b} at ({px}, {pz})");
                    assert!((SPRING - 1e-3..=SPRING + MAX_RISE + 1e-3).contains(&a), "{a}");
                }
            }
        }
    }

    #[test]
    fn the_courses_end_on_the_spring_and_reach_the_crowns() {
        let courses = courses(1207);
        let (bottom, height) = courses[7];
        assert!((bottom + height - SPRING).abs() < 1e-3);
        let (bottom, height) = *courses.last().unwrap();
        assert!(bottom + height >= SPRING + MAX_RISE);
        assert!(courses.iter().all(|&(_, h)| h > 3.0 && h < 4.8));
        assert!(courses.windows(2).all(|w| (w[0].0 + w[0].1 - w[1].0).abs() < 1e-4));
    }

    #[test]
    fn rough_rock_levels_draw_no_dressed_stone() {
        let level = fb_level::generate(1207, 1);
        assert_eq!(level.style, LevelStyle::RoughRock);
        assert!(columns(&level).is_empty());
    }
}
