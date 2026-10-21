//! The level's shapes as plain mesh data, with no Bevy in them: the rock faces round every open
//! cell, the ink along their edges, door frames, the entrance door and the fixtures that hold
//! the level's lights. Everything is a pure function of the [`Level`], so a level looks the
//! same on every machine.
//!
//! Grid and world axes are `fb_level`'s: `x` east, grid `y` south (world +Z), floor at world
//! `y = 0`, one cell [`CELL`] units across.

use armory::{
    ItemMesh, MaterialKind, Part, Rgb,
    build::{
        fitting::{drop_handle, lock_plate},
        light::{CHAR, EMBER, add_flame, glowing},
        parts::{Look, Parts},
        plank::{plank, planked_panel},
    },
    geom::{
        CatmullRom, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, cylinder, icosphere, lathe,
        torus, tube, value_noise3, vec3,
    },
};
use fb_level::{
    CELL, Cell, DoorStyle, Facing, FloorFinish, Level, LevelParams, LevelStyle, WallFinish,
    finish::{Finishes, finishes},
};
use procgen_rng::Mulberry32;

pub mod arch;
pub mod cobble;
pub mod door;
pub mod occlusion;
pub mod stone;

/// Cells of passage drawn outward behind the entrance door, so the open door shows a way
/// back into the dark rather than a rock face. Drawn only; the level data keeps them rock.
pub const ENTRANCE_PASSAGE: i32 = 3;

/// Grid segments along one cell's width (4 units each), for floor, ceiling and walls alike,
/// so every seam between them shares its vertices.
const SEGMENTS: usize = 5;
/// Height of one row of wall grid, in world units.
const ROW: f32 = 4.0;

/// How far the rock wanders off its cell's plane, sideways (walls) and up and down (floor
/// and ceiling), in world units. Wavelengths are a metre or so: relief the toon bands can
/// show as form, not speckle.
const WALL_RELIEF: f32 = 2.2;
const FLOOR_RELIEF: f32 = 0.35;
const CEILING_RELIEF: f32 = 1.6;
const RELIEF_WAVELENGTH: f32 = 11.0;

/// Ink ribbons sit this far off the stone, so they win the depth test without z-fighting.
const INK_LIFT: f32 = 0.05;

/// A doorway's opening, in world units: 1.3 m wide and 2.2 m high.
pub use fb_level::door::{DOOR_HEIGHT, DOOR_WIDTH, FRAME_DEPTH, LEAF_LAP, LEAF_THICKNESS};
/// The timber frame round a doorway: beam width and depth through the wall.
const FRAME_BEAM: f32 = 1.8;
/// The masonry that fills a doorway's cell round the frame, through the wall.
const INFILL_DEPTH: f32 = 3.0;

/// The entrance door's leaf: it lies over the frame on its inward face, so it is larger than
/// the opening by this much on every side but the floor.

/// What the level's surfaces are coloured: a cold grey stone, a little darker underfoot and
/// darker again overhead, where no light ever reaches for long.
pub const WALL_STONE: Rgb = Rgb::hex(0x7f817c);
pub const FLOOR_STONE: Rgb = Rgb::hex(0x6e6c66);
pub const CEILING_STONE: Rgb = Rgb::hex(0x5c5e5c);
/// The floor under cobbles and slabs: the dark of their joints.
pub const GROUT_STONE: Rgb = Rgb::hex(0x3b3a37);

/// The level's cells as drawn: the level's own, plus [`ENTRANCE_PASSAGE`] cells of floor
/// carved outward behind the entrance where the rock there is solid.
#[derive(Clone, Debug)]
pub struct CellView<'a> {
    level: &'a Level,
    passage: Vec<(i32, i32)>,
    finishes: Finishes,
}

impl<'a> CellView<'a> {
    pub fn new(level: &'a Level) -> Self {
        Self::with_finishes(level, level_finishes(level))
    }

    /// The view of `level` with its stone finished as `finishes` say.
    pub fn with_finishes(level: &'a Level, finishes: Finishes) -> Self {
        CellView { level, passage: entrance_passage(level), finishes }
    }

    /// The view with the finishes `params` choose.
    pub fn with_params(level: &'a Level, params: &LevelParams) -> Self {
        Self::with_finishes(level, finishes(level, params))
    }

    /// The view with every finish at its plain: no paving, layered or rubble walls and no arches,
    /// for tests of the plain stone.
    #[cfg(test)]
    pub fn uncobbled(level: &'a Level) -> Self {
        Self::with_params(level, &LevelParams { cobble_chance: 0.0, flagstone_chance: 0.0, strata_chance: 0.0, rubble_chance: 0.0, arch_chance: 0.0, ..LevelParams::for_style(level.style) })
    }

    /// What each room's and corridor's stone is finished with, and how damp it is.
    pub fn finishes(&self) -> &Finishes {
        &self.finishes
    }

    pub fn level(&self) -> &Level {
        self.level
    }

    /// Whether the floor of cell `(x, y)` is laid with cobbles.
    pub fn is_cobbled(&self, x: i32, y: i32) -> bool {
        self.finishes.at(x, y).is_some_and(|f| f.floor == FloorFinish::Cobble)
    }

    /// Whether the cells west, east, north and south of `(cx, cy)` are open, for [`CellView::floor_wear`].
    pub fn neighbours_open(&self, cx: i32, cy: i32) -> [bool; 4] {
        [self.is_open(cx - 1, cy), self.is_open(cx + 1, cy), self.is_open(cx, cy - 1), self.is_open(cx, cy + 1)]
    }

    /// How worn the floor is at world `(x, z)` in cell `(cx, cy)`, 0 to 1: more by a wall, where
    /// the stone is trodden and broken, and wherever moss grows.
    pub fn floor_wear(&self, cx: i32, cy: i32, open: [bool; 4], x: f32, z: f32) -> f32 {
        let (lx, lz) = (x - cx as f32 * CELL, z - cy as f32 * CELL);
        let by_wall = |open: bool, distance: f32| if open { 0.0 } else { (1.0 - distance / 5.0).max(0.0) };
        let wall = by_wall(open[0], lx).max(by_wall(open[1], CELL - lx)).max(by_wall(open[2], lz)).max(by_wall(open[3], CELL - lz));
        (wall * 0.7).max(self.finishes.moss_at(x, z) * 0.8)
    }

    /// The drawn-only cells behind the entrance, nearest first.
    pub fn passage(&self) -> &[(i32, i32)] {
        &self.passage
    }

    pub fn cell(&self, x: i32, y: i32) -> Cell {
        if self.passage.contains(&(x, y)) { Cell::Floor } else { self.level.cell(x, y) }
    }

    pub fn is_open(&self, x: i32, y: i32) -> bool {
        self.cell(x, y).is_open()
    }

    /// The cells to draw, `(x0, y0, x1, y1)` half open: the grid and the passage.
    pub fn bounds(&self) -> (i32, i32, i32, i32) {
        let (mut x0, mut y0, mut x1, mut y1) = (0, 0, self.level.width as i32, self.level.height as i32);
        for &(x, y) in &self.passage {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
        }
        (x0, y0, x1, y1)
    }
}

/// The finishes of `level` at its style's default odds: the levels carry no odds of their own.
pub fn level_finishes(level: &Level) -> Finishes {
    finishes(level, &LevelParams::for_style(level.style))
}

/// Sets each vertex's moss, the finish's smooth value at its world position, in the part's
/// second channel (`Part::uvs_b`), for the toon shader to paint over the stone.
pub fn bake_moss(part: &mut Part, finishes: &Finishes) {
    part.uvs_b = part.positions.iter().map(|p| [finishes.moss_at(p[0], p[2]), 0.0]).collect();
}

/// The cells behind the entrance to carve, nearest first: straight outward, stopping before
/// any cell that would touch open floor of the level (so the passage never breaks into a room).
pub fn entrance_passage(level: &Level) -> Vec<(i32, i32)> {
    // Below the first level the entrance is a stair up, with solid rock behind it.
    if level.depth >= 2 {
        return Vec::new();
    }
    let e = level.entrance;
    let (dx, dy) = e.inward.step();
    let (ox, oy) = (-dx, -dy);
    let (lx, ly) = (-oy, ox);
    let mut out = Vec::new();
    for k in 1..=ENTRANCE_PASSAGE {
        let (x, y) = (e.x as i32 + ox * k, e.y as i32 + oy * k);
        // The cell, its two sides, and the three ahead of it must all be rock.
        let around = [(0, 0), (lx, ly), (-lx, -ly), (ox, oy), (ox + lx, oy + ly), (ox - lx, oy - ly)];
        if around.iter().any(|&(ax, ay)| level.is_open(x + ax, y + ay)) {
            break;
        }
        out.push((x, y));
    }
    out
}

/// World position of a cell's centre on the floor.
pub fn cell_centre(x: i32, y: i32) -> V3 {
    [(x as f32 + 0.5) * CELL, 0.0, (y as f32 + 0.5) * CELL]
}

/// A grid step as a world direction on the floor plane.
pub fn facing_vector(facing: Facing) -> V3 {
    let (dx, dy) = facing.step();
    [dx as f32, 0.0, dy as f32]
}

const DIRECTIONS: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

/// The rock's relief: every drawn vertex is moved by one smooth field of world position, so
/// faces that meet share their moved edges and the stone stays closed at every seam.
#[derive(Clone, Copy, Debug)]
pub struct Relief {
    seed: u32,
    ceiling: f32,
    /// Whether the stone wanders at all: dressed stone is true to the grid.
    rough: bool,
}

impl Relief {
    pub fn new(level: &Level) -> Self {
        Relief { seed: level.seed ^ 0x5701_e5ed, ceiling: level.ceiling, rough: level.style == LevelStyle::RoughRock }
    }

    /// Two octaves of value noise in -0.5..0.5 for channel `channel`.
    fn wander(&self, p: V3, channel: u32) -> f32 {
        let q = vec3::scale([p[0], p[1] * 1.6, p[2]], 1.0 / RELIEF_WAVELENGTH);
        let seed = self.seed.wrapping_add(channel.wrapping_mul(0x9e37_79b9));
        (value_noise3(q, seed) - 0.5) + 0.35 * (value_noise3(vec3::scale(q, 2.7), seed ^ 0x2545_f491) - 0.5)
    }

    /// The height of the drawn floor over world `(x, z)`: the grid floor point's height once
    /// moved. The point also moves a little sideways, a fraction of the relief's metre-long
    /// wavelength, so this is the floor's height to within a couple of centimetres.
    pub fn floor_height(&self, x: f32, z: f32) -> f32 {
        self.displace([x, 0.0, z])[1]
    }

    /// Where the drawn rock is for a point `p` on the cell grid.
    pub fn displace(&self, p: V3) -> V3 {
        if !self.rough {
            return p;
        }
        let up = (p[1] / self.ceiling.max(1.0)).clamp(0.0, 1.0);
        let vertical = FLOOR_RELIEF + (CEILING_RELIEF - FLOOR_RELIEF) * up;
        [p[0] + WALL_RELIEF * self.wander(p, 1), p[1] + vertical * self.wander(p, 2), p[2] + WALL_RELIEF * self.wander(p, 3)]
    }

    /// The moved surface's normal at grid point `p` on a plane spanned by `u` and `v`,
    /// turned to `facing`. A function of `p` alone for a given plane, so coplanar faces that
    /// meet shade alike across the seam.
    pub fn normal(&self, p: V3, u: V3, v: V3, facing: V3) -> V3 {
        let e = 0.5;
        let du = vec3::sub(self.displace(vec3::add(p, vec3::scale(u, e))), self.displace(vec3::sub(p, vec3::scale(u, e))));
        let dv = vec3::sub(self.displace(vec3::add(p, vec3::scale(v, e))), self.displace(vec3::sub(p, vec3::scale(v, e))));
        let n = vec3::normalize_or(vec3::cross(du, dv), facing);
        if vec3::dot(n, facing) < 0.0 { vec3::neg(n) } else { n }
    }
}

/// A flat rectangle of the cell grid before relief: `origin` plus `u` and `v` (unit axes)
/// times up to `size`, cut into `steps`, facing `facing`.
#[derive(Clone, Copy, Debug)]
struct Face {
    origin: V3,
    u: V3,
    v: V3,
    size: [f32; 2],
    steps: [usize; 2],
    facing: V3,
}

impl Face {
    fn point(&self, i: usize, j: usize) -> V3 {
        let (s, t) = (self.size[0] * i as f32 / self.steps[0] as f32, self.size[1] * j as f32 / self.steps[1] as f32);
        vec3::add(self.origin, vec3::add(vec3::scale(self.u, s), vec3::scale(self.v, t)))
    }

    /// Appends the face, moved by `relief`, with world-planar uvs (one per cell).
    fn build(&self, relief: &Relief, mesh: &mut MeshBuilder) {
        let [nu, nv] = self.steps;
        let base = mesh.vertex_count() as u32;
        for j in 0..=nv {
            for i in 0..=nu {
                let p = self.point(i, j);
                let uv = [vec3::dot(p, self.u) / CELL, vec3::dot(p, self.v) / CELL];
                mesh.push_vertex(relief.displace(p), relief.normal(p, self.u, self.v, self.facing), uv);
            }
        }
        let flip = vec3::dot(vec3::cross(self.u, self.v), self.facing) < 0.0;
        let at = |i: usize, j: usize| base + (j * (nu + 1) + i) as u32;
        for j in 0..nv {
            for i in 0..nu {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                if flip {
                    mesh.push_triangle(a, c, b).push_triangle(a, d, c);
                } else {
                    mesh.push_triangle(a, b, c).push_triangle(a, c, d);
                }
            }
        }
    }

    /// Grid points along one edge: `j = 0` (`Edge::Start`) or the far row, or a column.
    fn edge(&self, edge: Edge) -> Vec<V3> {
        let [nu, nv] = self.steps;
        match edge {
            Edge::Bottom => (0..=nu).map(|i| self.point(i, 0)).collect(),
            Edge::Top => (0..=nu).map(|i| self.point(i, nv)).collect(),
            Edge::Left => (0..=nv).map(|j| self.point(0, j)).collect(),
            Edge::Right => (0..=nv).map(|j| self.point(nu, j)).collect(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Edge {
    Bottom,
    Top,
    Left,
    Right,
}

/// Ink drawn along the stone's edges: ribbons with no width of their own whose two sides
/// carry opposite push directions in their outline normals, so `OutlineMaterial` widens them
/// to a line a set number of pixels across. Each ribbon lies on one surface and is wound both
/// ways, so whichever side faces away from the camera draws (the outline pipeline culls front
/// faces). Where a ribbon's far half runs into rock it is hidden; at a convex corner it hangs
/// in the air and draws the silhouette.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ink {
    pub positions: Vec<V3>,
    /// The surface each ribbon lies on.
    pub normals: Vec<V3>,
    /// Which way each vertex is pushed.
    pub push: Vec<V3>,
    pub indices: Vec<u32>,
}

impl Ink {
    /// A ribbon along `points` (already moved by relief) on a surface with `normals` there.
    pub fn ribbon(&mut self, points: &[V3], normals: &[V3]) {
        self.ribbon_lifted(points, normals, INK_LIFT);
    }

    /// As [`Ink::ribbon`], standing `lift` off the surface, to clear stone that stands proud of it.
    pub fn ribbon_lifted(&mut self, points: &[V3], normals: &[V3], lift: f32) {
        if points.len() < 2 {
            return;
        }
        let base = self.positions.len() as u32;
        for (k, (&p, &n)) in points.iter().zip(normals).enumerate() {
            let (prev, next) = (points[k.saturating_sub(1)], points[(k + 1).min(points.len() - 1)]);
            let along = vec3::sub(next, prev);
            let side = vec3::normalize_or(vec3::cross(along, n), vec3::any_perpendicular(n));
            let lifted = vec3::add(p, vec3::scale(n, lift));
            for push in [side, vec3::neg(side)] {
                self.positions.push(lifted);
                self.normals.push(n);
                self.push.push(push);
            }
        }
        for k in 0..points.len() as u32 - 1 {
            let (a, b, c, d) = (base + 2 * k, base + 2 * k + 1, base + 2 * k + 3, base + 2 * k + 2);
            self.indices.extend([a, b, c, a, c, d, a, c, b, a, d, c]);
        }
    }

    /// As an armory [`Part`], push directions in its outline normals, for `part_to_mesh`.
    pub fn into_part(self) -> Part {
        let n = self.positions.len();
        Part {
            name: "ink",
            outline: false,
            positions: self.positions,
            normals: self.normals,
            uvs: vec![[0.0, 0.0]; n],
            outline_normals: self.push,
            indices: self.indices,
            ..Default::default()
        }
    }
}

/// One stretch of the level's stone: floor, walls and ceiling as [`Part`]s (stone, drawn
/// without hulls) and the ink along their edges. Dressed stone also has `joints`, a finer ink
/// along its courses, joints and vault ribs; rough rock has none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LevelMesh {
    pub surfaces: ItemMesh,
    pub ink: Part,
    pub joints: Part,
}

/// The stone of every drawn cell in `x0..x1`, `y0..y1`, in world space. Walls stand on each
/// face of rock touching an open cell; the floor and ceiling cover every open cell. Each
/// surface carries how shut in its vertices are (`occlusion`), which the shader shades corners
/// by.
pub fn level_mesh(view: &CellView, range: (i32, i32, i32, i32)) -> LevelMesh {
    let mut mesh = bare_level_mesh(view, range);
    let enclosure = occlusion::Enclosure::new(view, range);
    for part in &mut mesh.surfaces.parts {
        enclosure.bake(part);
        bake_moss(part, view.finishes());
    }
    mesh
}

/// [`level_mesh`] before corner shading.
fn bare_level_mesh(view: &CellView, (x0, y0, x1, y1): (i32, i32, i32, i32)) -> LevelMesh {
    let level = view.level();
    if level.style == LevelStyle::StoneHalls {
        return stone::hall_mesh(view, (x0, y0, x1, y1));
    }
    let relief = Relief::new(level);
    let (mut floor, mut ceiling, mut walls, mut strata, mut hewn) = (MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new());
    let mut paving = cobble::Stones::default();
    let mut grout = MeshBuilder::new();
    let mut ink = Ink::default();
    let rows = (level.ceiling / ROW).ceil().max(1.0) as usize;
    let up = [0.0, 1.0, 0.0];
    for y in y0..y1 {
        for x in x0..x1 {
            if !view.is_open(x, y) {
                continue;
            }
            let corner = [x as f32 * CELL, 0.0, y as f32 * CELL];
            let flat = |height: f32, facing: V3| Face {
                origin: [corner[0], height, corner[2]],
                u: [1.0, 0.0, 0.0],
                v: [0.0, 0.0, 1.0],
                size: [CELL, CELL],
                steps: [SEGMENTS, SEGMENTS],
                facing,
            };
            // Where a stair goes through (`super::stairs`), no floor under a way down and no
            // ceiling over a way up.
            let stair = level.stair_at(x, y);
            if !super::stairs::opens_floor(stair) {
                let on_relief = |px: f32, pz: f32| relief.floor_height(px, pz);
                // Under paving the shell is the dark of the joints, so the stones stand out of it.
                match view.finishes().at(x, y).map(|f| f.floor) {
                    Some(FloorFinish::Cobble) => {
                        flat(0.0, up).build(&relief, &mut grout);
                        let open = view.neighbours_open(x, y);
                        cobble::cobbles(level.seed, x, y, &on_relief, &|px, pz| view.floor_wear(x, y, open, px, pz), &mut paving);
                    }
                    Some(FloorFinish::Flagstone) => {
                        flat(0.0, up).build(&relief, &mut grout);
                        cobble::paving(level.seed, x, y, &on_relief, &mut paving);
                    }
                    _ => flat(0.0, up).build(&relief, &mut floor),
                }
            }
            if !super::stairs::opens_ceiling(stair) {
                flat(level.ceiling, vec3::neg(up)).build(&relief, &mut ceiling);
            }

            for facing in DIRECTIONS {
                let (dx, dy) = facing.step();
                if view.is_open(x + dx, y + dy) {
                    continue;
                }
                let d = facing_vector(facing);
                let inward = vec3::neg(d);
                // Run along the wall with the room on the left, so `u x up` faces the room.
                let u = vec3::cross(up, inward);
                let centre = cell_centre(x, y);
                let origin = vec3::sub(vec3::add(centre, vec3::scale(d, CELL * 0.5)), vec3::scale(u, CELL * 0.5));
                let face = Face { origin, u, v: up, size: [CELL, level.ceiling], steps: [SEGMENTS, rows], facing: inward };
                // Layered rooms have a part of their own, so the shader gives them beds, and hewn
                // ones another, so it gives them craggy facets. Every other wall is plain stone.
                let layered = view.finishes().at(x, y).is_some_and(|f| f.walls == WallFinish::Strata);
                let carved = view.finishes().character_at(x, y) == Some(fb_level::Character::Hewn) && !layered;
                face.build(&relief, if layered { &mut strata } else if carved { &mut hewn } else { &mut walls });

                // Seams with the floor and ceiling: a ribbon on the wall and one on the flat.
                for (edge, flat_facing) in [(Edge::Bottom, up), (Edge::Top, vec3::neg(up))] {
                    let base = face.edge(edge);
                    let moved: Vec<V3> = base.iter().map(|&p| relief.displace(p)).collect();
                    let on_wall: Vec<V3> = base.iter().map(|&p| relief.normal(p, u, up, inward)).collect();
                    let on_flat: Vec<V3> = base.iter().map(|&p| relief.normal(p, u, inward, flat_facing)).collect();
                    ink.ribbon(&moved, &on_wall);
                    ink.ribbon(&moved, &on_flat);
                }
                // Upright edges, where this wall turns a corner rather than running on.
                for (edge, side) in [(Edge::Left, vec3::neg(u)), (Edge::Right, u)] {
                    let (sx, sy) = (side[0].round() as i32, side[2].round() as i32);
                    let runs_on = view.is_open(x + sx, y + sy) && !view.is_open(x + sx + dx, y + sy + dy);
                    if runs_on {
                        continue;
                    }
                    let base = face.edge(edge);
                    let moved: Vec<V3> = base.iter().map(|&p| relief.displace(p)).collect();
                    let normals: Vec<V3> = base.iter().map(|&p| relief.normal(p, u, up, inward)).collect();
                    ink.ribbon(&moved, &normals);
                }
            }
        }
    }
    let mut parts = Parts::new();
    let stone = |tint: Rgb| Look::of(MaterialKind::Stone).tinted(tint).without_outline();
    parts.add_look("floor", floor, stone(FLOOR_STONE));
    if grout.vertex_count() > 0 {
        parts.add_look("grout", grout, stone(GROUT_STONE));
    }
    paving.add_parts(&mut parts, cobble::stone_look);
    parts.add_look("walls", walls, stone(WALL_STONE));
    if !strata.is_empty() {
        parts.add_look("strata", strata, stone(WALL_STONE));
    }
    if !hewn.is_empty() {
        parts.add_look("hewn", hewn, stone(WALL_STONE));
    }
    parts.add_look("ceiling", ceiling, stone(CEILING_STONE));
    LevelMesh { surfaces: parts.finish(), ink: ink.into_part(), joints: Part::default() }
}

/// The cell ranges to draw the level in, `size` cells square, covering the whole view.
pub fn chunks(view: &CellView, size: i32) -> Vec<(i32, i32, i32, i32)> {
    let (x0, y0, x1, y1) = view.bounds();
    let size = size.max(1);
    let mut out = Vec::new();
    let mut y = y0;
    while y < y1 {
        let mut x = x0;
        while x < x1 {
            out.push((x, y, (x + size).min(x1), (y + size).min(y1)));
            x += size;
        }
        y += size;
    }
    out
}

/// A doorway: a door cell with rock either side, and the way through it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Doorway {
    pub x: i32,
    pub y: i32,
    /// Unit direction through the doorway on the floor plane. For the entrance, inward.
    pub through: V3,
}

impl Doorway {
    /// The frame's axes in world space: `x` across the opening, `y` up, `z` through.
    pub fn basis(&self) -> [V3; 3] {
        let up = [0.0, 1.0, 0.0];
        [vec3::cross(up, self.through), up, self.through]
    }
}

/// Every doorway in the view. A door cell gets a frame where it has rock on both sides of one
/// axis; the entrance always faces inward.
pub fn doorways(view: &CellView) -> Vec<Doorway> {
    let level = view.level();
    let mut out = Vec::new();
    for y in 0..level.height as i32 {
        for x in 0..level.width as i32 {
            if level.cell(x, y) != Cell::Door {
                continue;
            }
            let through = if (x as u32, y as u32) == (level.entrance.x, level.entrance.y) {
                Some(facing_vector(level.entrance.inward))
            } else if !view.is_open(x, y - 1) && !view.is_open(x, y + 1) {
                Some([1.0, 0.0, 0.0])
            } else if !view.is_open(x - 1, y) && !view.is_open(x + 1, y) {
                Some([0.0, 0.0, 1.0])
            } else {
                None
            };
            if let Some(through) = through {
                out.push(Doorway { x, y, through });
            }
        }
    }
    out
}

/// A doorway's frame in its own space (`x` across, `y` up, `z` through, origin on the floor at
/// the cell's centre): a timber frame round a [`DOOR_WIDTH`] by [`DOOR_HEIGHT`] opening in a
/// course of dressed stone blocks that closes the rest of the cell up to the ceiling. A
/// rock-cut `style` swaps the timber for an arch cut in the blocks.
pub fn door_frame(ceiling: f32, style: DoorStyle, seed: u32) -> ItemMesh {
    // Past the cell's edge and the ceiling, so the blocks bed into the rock's relief.
    let top = ceiling + CEILING_RELIEF;
    let course: f32 = 4.6;
    let mut courses = Vec::new();
    let mut y = 0.0;
    while y < top {
        courses.push((y, course.min(top - y)));
        y += course;
    }
    door_frame_in(style, &courses, top, CELL * 0.5 + WALL_RELIEF, (0.18, 0.35), seed)
}

/// A doorway's frame and masonry with the infill laid in `courses` (each a bottom and a
/// height, from the floor up to `top`), its blocks reaching `reach` either side of the centre,
/// with `joint` between them and a `chamfer` on their edges. Any style but timber is an arch.
fn door_frame_in(style: DoorStyle, courses: &[(f32, f32)], top: f32, reach: f32, (joint, chamfer): (f32, f32), seed: u32) -> ItemMesh {
    if style != DoorStyle::Timber {
        return arch::door_arch(style, courses, top, reach, (joint, chamfer), seed);
    }
    let mut rng = Mulberry32::new(seed ^ 0xd00f_4a3e);
    let half_open = DOOR_WIDTH * 0.5;
    let lintel_top = DOOR_HEIGHT + FRAME_BEAM;

    let mut masonry = MeshBuilder::new();
    let gap = joint;
    // The frame's outer edge across the opening: blocks beside it stop here, and blocks over
    // it sit on the lintel.
    let clear = half_open + FRAME_BEAM;
    let mut block = |lo: f32, hi: f32, bottom: f32, top: f32, rng: &mut Mulberry32| {
        if hi - lo < 0.6 || top - bottom < 0.6 {
            return;
        }
        let depth = INFILL_DEPTH + (rng.range_f32(0.0, 1.0) - 0.5) * 0.4;
        let mut stone = plank(hi - lo - gap, top - bottom, depth, chamfer);
        stone.translate([(lo + hi) * 0.5, (bottom + top) * 0.5, (rng.range_f32(0.0, 1.0) - 0.5) * 0.25]);
        masonry.append(&stone);
    };
    for (row, &(y, h)) in courses.iter().enumerate() {
        let (bottom, top_of_course) = (y + gap * 0.5, y + h - gap * 0.5);
        // Stagger alternate courses by half a block.
        let mut x = -reach - if row % 2 == 1 { 3.0 } else { 0.0 };
        while x < reach {
            let len = 5.2 + rng.range_f32(0.0, 1.0) * 2.4;
            let (a, b) = (x.max(-reach), (x + len).min(reach));
            x += len;
            if y >= lintel_top || b <= -clear || a >= clear {
                block(a, b, bottom, top_of_course, &mut rng);
                continue;
            }
            // A course level with the frame: full height beside it, and over it only from the
            // lintel's top, so the stone sits on the lintel with no gap between.
            block(a, a.max(-clear).min(b), bottom, top_of_course, &mut rng);
            block(a.max(-clear), b.min(clear), lintel_top, top_of_course, &mut rng);
            block(b.min(clear).max(a), b, bottom, top_of_course, &mut rng);
        }
    }
    // A thinner core of stone behind the blocks, so the mortar joints read as recesses rather
    // than slits through the wall. It beds into the floor's relief and hides its edges behind
    // the jambs and the lintel.
    let core_depth = INFILL_DEPTH * 0.6;
    let floor = -FLOOR_RELIEF;
    let inner = half_open + 0.05;
    let over = DOOR_HEIGHT + FRAME_BEAM * 0.5;
    for (lo, hi, bottom) in [(-reach, -inner, floor), (inner, reach, floor), (-inner, inner, over)] {
        let mut core = plank(hi - lo, top - bottom, core_depth, 0.1);
        core.translate([(lo + hi) * 0.5, (bottom + top) * 0.5, 0.0]);
        masonry.append(&core);
    }

    let mut timber = MeshBuilder::new();
    for side in [-1.0f32, 1.0] {
        let mut jamb = plank(lintel_top, FRAME_BEAM, FRAME_DEPTH, 0.25);
        jamb.rotate_z(std::f32::consts::FRAC_PI_2).translate([side * (half_open + FRAME_BEAM * 0.5), lintel_top * 0.5, 0.0]);
        timber.append(&jamb);
    }
    let mut lintel = plank(DOOR_WIDTH + FRAME_BEAM * 2.6, FRAME_BEAM, FRAME_DEPTH + 0.3, 0.25);
    lintel.translate([0.0, DOOR_HEIGHT + FRAME_BEAM * 0.5, 0.0]);
    timber.append(&lintel);
    let mut sill = plank(DOOR_WIDTH, 0.5, FRAME_DEPTH, 0.15);
    sill.translate([0.0, 0.25, 0.0]);
    timber.append(&sill);

    let mut parts = Parts::new();
    parts.add("masonry", masonry, MaterialKind::Stone);
    parts.add("frame", timber, MaterialKind::Wood);
    parts.finish()
}

/// Where the entrance door hangs in its doorway's space: the hinge line on the frame's inward
/// face, at the opening's left edge.
pub fn entrance_hinge() -> V3 {
    [fb_level::door::HINGE_ACROSS, 0.0, fb_level::door::HINGE_THROUGH]
}

/// The entrance door's leaf, in hinge space: the hinge line is the +Y axis, the leaf runs
/// along +X from it and its faces look along ±Z. Vertical boards held by three iron bands
/// riveted through, the bands turned round the hinge pins, with a ring pull and a lock plate
/// on each face.
pub fn entrance_leaf(seed: u32) -> ItemMesh {
    let width = DOOR_WIDTH + LEAF_LAP * 2.0;
    let height = DOOR_HEIGHT + LEAF_LAP;
    let mut rng = Mulberry32::new(seed ^ 0x1eaf_d00b);
    let boards = 5 + (rng.range_f32(0.0, 1.0) * 2.0) as u32;
    let mut planks = planked_panel(height - 0.2, width, boards, 0.14, LEAF_THICKNESS, 0.16);
    // Boards run up the door: the panel's length (X) turns to Y.
    planks.rotate_z(std::f32::consts::FRAC_PI_2).translate([width * 0.5, height * 0.5 + 0.1, 0.0]);

    let mut iron = MeshBuilder::new();
    let band_t = 0.2;
    for &h in &[0.16, 0.5, 0.84] {
        let y = height * h;
        for face in [-1.0f32, 1.0] {
            let z = face * (LEAF_THICKNESS * 0.5 + band_t * 0.5);
            let mut band = plank(width * 0.9, 1.4, band_t, 0.05);
            band.translate([width * 0.45, y, z]);
            iron.append(&band);
            // Rivets clenched through the boards, one per board.
            for b in 0..boards {
                let x = width * (b as f32 + 0.5) / boards as f32;
                if x > width * 0.9 {
                    continue;
                }
                let mut rivet = icosphere(0.22, 1);
                rivet.scale([1.0, 1.0, 0.6]).translate([x, y, z + face * band_t * 0.5]);
                iron.append(&rivet);
            }
        }
        // The hinge knuckle round the pin.
        let mut knuckle = cylinder(0.55, 2.0, 12, Shading::BANDED);
        knuckle.translate([0.0, y - 1.0, 0.0]);
        iron.append(&knuckle);
    }
    // The pin runs the height of the leaf.
    let mut pin = cylinder(0.2, height * 0.9, 8, Shading::SMOOTH);
    pin.translate([0.0, height * 0.05, 0.0]);
    iron.append(&pin);

    let handle_x = width - 2.6;
    for face in [-1.0f32, 1.0] {
        let z = face * LEAF_THICKNESS * 0.5;
        let forward = [0.0, 0.0, face];
        let mut ring = drop_handle(2.4, 0.3);
        ring.place_basis([handle_x, height * 0.47, z], [0.0, 1.0, 0.0], forward);
        iron.append(&ring);
        let (mut plate, mut keyhole) = lock_plate(1.8, 2.6, 0.18);
        plate.place_basis([handle_x, height * 0.6, z], [0.0, 1.0, 0.0], forward);
        keyhole.place_basis([handle_x, height * 0.6, z], [0.0, 1.0, 0.0], forward);
        iron.append(&plate);
        iron.append(&keyhole);
    }

    let mut parts = Parts::new();
    parts.add("boards", planks, MaterialKind::Wood);
    parts.add("ironwork", iron, MaterialKind::Iron);
    parts.finish()
}

/// The entrance door's turn about its hinge, radians, as it swings shut: `progress` 0 is
/// standing open into the level, 1 is shut. Eased in and out, as a heavy door gathers way
/// and then settles.
pub fn entrance_swing(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    ENTRANCE_OPEN_ANGLE * (1.0 - eased)
}

/// How far the entrance door stands open, radians from shut (turned into the level).
pub const ENTRANCE_OPEN_ANGLE: f32 = -fb_level::door::OPEN_DEGREES * std::f32::consts::PI / 180.0;

/// An iron brazier standing on the floor at the origin: a bowl on three splayed legs braced
/// by a ring, heaped with coals under a flame. Returns the mesh and the height of the
/// flame's heart, where its light hangs.
pub fn brazier(seed: u32) -> (ItemMesh, f32) {
    let mut rng = Mulberry32::new(seed ^ 0xb7a2_1e70);
    let bowl_y = 8.0;
    // Outside up to the rim, then back down the inside.
    let profile = [
        [0.0, bowl_y - 0.4],
        [1.6, bowl_y - 0.3],
        [3.3, bowl_y + 0.6],
        [4.2, bowl_y + 2.0],
        [4.5, bowl_y + 2.4],
        [4.15, bowl_y + 2.4],
        [3.0, bowl_y + 1.0],
        [1.4, bowl_y + 0.3],
        [0.0, bowl_y + 0.2],
    ];
    let mut iron = lathe(&profile, &LatheOptions { facets: 24, shading: Shading::SMOOTH, ..Default::default() });
    let legs = 3;
    for k in 0..legs {
        let a = std::f32::consts::TAU * (k as f32 + rng.range_f32(0.0, 1.0) * 0.1) / legs as f32;
        let (s, c) = a.sin_cos();
        let path = CatmullRom::centripetal(vec![
            [c * 2.6, bowl_y + 0.3, s * 2.6],
            [c * 3.4, bowl_y * 0.55, s * 3.4],
            [c * 4.6, 0.6, s * 4.6],
            [c * 5.4, 0.25, s * 5.4],
        ])
        .sample(8);
        iron.append(&tube(&path, 8, &SweepOptions::default(), |_| 0.32));
    }
    let mut brace = torus(3.55, 0.2, 32, 6, Shading::SMOOTH);
    brace.translate([0.0, bowl_y * 0.45, 0.0]);
    iron.append(&brace);

    let (mut coals, mut embers) = (MeshBuilder::new(), MeshBuilder::new());
    for _ in 0..14 {
        let (a, r) = (rng.range_f32(0.0, 1.0) * std::f32::consts::TAU, rng.range_f32(0.0, 1.0).sqrt() * 3.0);
        let mut lump = icosphere(0.55 + rng.range_f32(0.0, 1.0) * 0.45, 1);
        lump.scale([1.0, 0.7, 1.0]).translate([a.cos() * r, bowl_y + 0.9 + rng.range_f32(0.0, 1.0) * 0.4, a.sin() * r]);
        if rng.range_f32(0.0, 1.0) < 0.45 { embers.append(&lump) } else { coals.append(&lump) };
    }

    let mut parts = Parts::new();
    parts.add("brazier", iron, MaterialKind::Iron);
    parts.add_tinted("coals", coals, MaterialKind::Wood, CHAR);
    parts.add_look("embers", embers, glowing(EMBER, 0.8));
    let flame_base = bowl_y + 1.2;
    let flame_height = 4.5 + rng.range_f32(0.0, 1.0) * 1.5;
    add_flame(&mut parts, [0.0, flame_base, 0.0], flame_height, 1.5, 0.08, seed);
    // The light hangs just over the flame, so the bowl's rim does not cut it off from the room.
    (parts.finish(), flame_base + flame_height + 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Per seed: the hash of the level's stone and ink, and of its door frame.
    const ROUGH_ROCK_HASHES: [(u32, (u64, u64)); 2] = [(1207, (3701189268064332467, 10396788927676635465)), (2, (10639431740370850025, 9434967241214962033))];

    fn level() -> Level {
        fb_level::generate(1207, 1)
    }

    #[test]
    fn the_passage_runs_outward_behind_the_entrance() {
        let level = level();
        let view = CellView::new(&level);
        let (dx, dy) = level.entrance.inward.step();
        let first = (level.entrance.x as i32 - dx, level.entrance.y as i32 - dy);
        assert_eq!(view.passage().first(), Some(&first));
        assert!(view.passage().len() as i32 <= ENTRANCE_PASSAGE);
        for &(x, y) in view.passage() {
            assert!(!level.is_open(x, y), "the passage is drawn only");
            assert!(view.is_open(x, y));
        }
    }

    #[test]
    fn the_passage_stops_short_of_open_floor() {
        // A 5 by 8 grid: the entrance at (2, 4) facing north into (2, 3), and open floor at
        // (2, 7), three cells behind it. Only (2, 5) can be carved: (2, 6) touches (2, 7).
        let (width, height) = (5u32, 8u32);
        let mut cells = vec![Cell::Rock; (width * height) as usize];
        cells[(3 * width + 2) as usize] = Cell::Floor;
        cells[(4 * width + 2) as usize] = Cell::Door;
        cells[(7 * width + 2) as usize] = Cell::Floor;
        let level = Level {
            width,
            height,
            cells,
            entrance: fb_level::Entrance { x: 2, y: 4, inward: Facing::North },
            ..level()
        };
        assert_eq!(entrance_passage(&level), vec![(2, 5)]);
    }

    #[test]
    fn the_stone_is_deterministic_and_drawable() {
        let level = level();
        let view = CellView::uncobbled(&level);
        let a = level_mesh(&view, view.bounds());
        assert_eq!(a, level_mesh(&view, view.bounds()));
        for part in a.surfaces.parts.iter().chain([&a.ink]) {
            part.validate().unwrap();
        }
        // Plain stone: no paving and no layered walls, which have parts of their own.
        assert_eq!(a.surfaces.parts.iter().map(|p| p.name).collect::<Vec<_>>(), ["floor", "walls", "ceiling"]);
        assert!(a.surfaces.parts.iter().all(|p| p.material == Some(MaterialKind::Stone) && !p.outline));
    }

    #[test]
    fn cobbled_floors_are_laid_and_split_exactly() {
        // A cave with a cobbled quarter and no flagged one, so the per-cell cost is the cobbles'.
        let paved = |l: &Level| {
            let view = CellView::new(l);
            let floors: Vec<_> = (0..l.height as i32).flat_map(|y| (0..l.width as i32).map(move |x| (x, y))).filter_map(|(x, y)| view.finishes().at(x, y).map(|f| f.floor)).collect();
            floors.contains(&FloorFinish::Cobble) && !floors.contains(&FloorFinish::Flagstone)
        };
        let level = (1..400).map(|seed| fb_level::generate(seed, 5)).find(paved).expect("some cave has paving");
        let view = CellView::new(&level);
        let cobbled = (0..level.height as i32).flat_map(|y| (0..level.width as i32).map(move |x| (x, y))).filter(|&(x, y)| view.finishes().at(x, y).is_some_and(|f| matches!(f.floor, FloorFinish::Cobble | FloorFinish::Flagstone))).count();
        assert!(cobbled > 0);
        let whole = level_mesh(&view, view.bounds());
        assert!(whole.surfaces.parts.iter().any(|p| p.name.starts_with("stones_")));
        let plain = level_mesh(&CellView::uncobbled(&level), view.bounds());
        let tris = |m: &LevelMesh| m.surfaces.parts.iter().map(|p| p.indices.len() / 3).sum::<usize>();
        let per_cell = (tris(&whole) - tris(&plain)) / cobbled;
        assert!((300..=1600).contains(&per_cell), "{per_cell} triangles per cobbled cell");
        for part in &whole.surfaces.parts {
            part.validate().unwrap();
        }
        // Laid in chunks that cut across the rooms, every vertex of every part comes out the same.
        let points = |meshes: &[LevelMesh]| {
            let mut out: Vec<(&'static str, [i64; 3])> = meshes
                .iter()
                .flat_map(|m| m.surfaces.parts.iter())
                .filter(|p| p.name.starts_with("stones_") || p.name == "grout")
                .flat_map(|p| p.positions.iter().map(|v| (p.name, v.map(|c| (c * 1000.0).round() as i64))))
                .collect();
            out.sort_unstable();
            out
        };
        let pieces: Vec<LevelMesh> = chunks(&view, 4).iter().map(|&c| level_mesh(&view, c)).collect();
        assert_eq!(points(&[whole]), points(&pieces));
    }

    #[test]
    fn every_open_cell_is_floored_and_walled() {
        let level = level();
        let view = CellView::uncobbled(&level);
        let (x0, y0, x1, y1) = view.bounds();
        // A way down has no floor under it and a way up no ceiling over it.
        let (mut open, mut floors, mut ceilings, mut faces) = (0, 0, 0, 0);
        for y in y0..y1 {
            for x in x0..x1 {
                if view.is_open(x, y) {
                    open += 1;
                    floors += !crate::level::stairs::opens_floor(level.stair_at(x, y)) as usize;
                    ceilings += !crate::level::stairs::opens_ceiling(level.stair_at(x, y)) as usize;
                    faces += DIRECTIONS.iter().filter(|f| !view.is_open(x + f.step().0, y + f.step().1)).count();
                }
            }
        }
        let mesh = level_mesh(&view, view.bounds());
        let quads = |name: &str| mesh.surfaces.parts.iter().find(|p| p.name == name).map_or(0, |p| p.indices.len() / 6);
        assert!(floors < open, "the way down leaves its cell unfloored");
        assert_eq!(quads("floor"), floors * SEGMENTS * SEGMENTS);
        assert_eq!(quads("ceiling"), ceilings * SEGMENTS * SEGMENTS);
        let rows = (level.ceiling / ROW).ceil() as usize;
        assert_eq!(quads("walls") + quads("strata") + quads("hewn"), faces * SEGMENTS * rows);
    }

    #[test]
    fn chunks_split_the_stone_without_losing_any() {
        let level = level();
        let view = CellView::new(&level);
        let whole = level_mesh(&view, view.bounds());
        let tris = |m: &LevelMesh| m.surfaces.parts.iter().map(|p| p.indices.len()).sum::<usize>();
        let pieces: usize = chunks(&view, 4).iter().map(|&c| tris(&level_mesh(&view, c))).sum();
        assert_eq!(pieces, tris(&whole));
    }

    #[test]
    fn walls_meet_the_floor_on_shared_points() {
        // Without the stairs, which leave their cells unfloored.
        let level = Level { stairs: Vec::new(), ..level() };
        let view = CellView::uncobbled(&level);
        let mesh = level_mesh(&view, view.bounds());
        let key = |p: &[f32; 3]| p.map(|c| (c * 1000.0).round() as i64);
        let floor: std::collections::HashSet<_> = mesh.surfaces.parts[0].positions.iter().map(key).collect();
        let walls = &mesh.surfaces.parts[1];
        // The lowest wall row lies within the floor's relief, and every point of it is a floor point.
        let low: Vec<_> = walls.positions.iter().filter(|p| p[1].abs() <= FLOOR_RELIEF).collect();
        assert!(!low.is_empty());
        assert!(low.iter().all(|p| floor.contains(&key(p))));
    }

    #[test]
    fn relief_is_gentle_and_seeded() {
        let level = level();
        let relief = Relief::new(&level);
        let other = Relief::new(&Level { seed: level.seed + 1, ..level.clone() });
        let mut moved = 0;
        for i in 0..200 {
            let p = [i as f32 * 1.7, (i % 30) as f32, i as f32 * 0.9];
            let q = relief.displace(p);
            assert!((q[0] - p[0]).abs() <= WALL_RELIEF * 0.7 && (q[2] - p[2]).abs() <= WALL_RELIEF * 0.7);
            if q != other.displace(p) {
                moved += 1;
            }
        }
        assert!(moved > 190);
    }

    #[test]
    fn the_entrance_has_a_doorway_facing_inward() {
        let level = level();
        let view = CellView::new(&level);
        let doors = doorways(&view);
        let entrance = doors.iter().find(|d| (d.x as u32, d.y as u32) == (level.entrance.x, level.entrance.y)).unwrap();
        assert_eq!(entrance.through, facing_vector(level.entrance.inward));
        let [x, y, z] = entrance.basis();
        assert!(vec3::dot(vec3::cross(x, y), z) > 0.99, "right-handed");
    }

    #[test]
    fn fixtures_are_drawable() {
        let frame = door_frame(30.0, DoorStyle::Timber, 1207);
        let leaf = entrance_leaf(1207);
        let (brazier, flame) = brazier(1207);
        for part in frame.parts.iter().chain(&leaf.parts).chain(&brazier.parts) {
            part.validate().unwrap();
        }
        assert!(flame > 12.0 && flame < 18.0);
        // The masonry leaves the opening clear.
        let masonry = &frame.parts[0];
        assert!(masonry.positions.iter().all(|p| p[1] > DOOR_HEIGHT || p[0].abs() > DOOR_WIDTH * 0.5));
        // The leaf covers the opening from the hinge.
        let (lo, hi) = leaf.bounds().unwrap();
        assert!(lo[0] < 0.0 && hi[0] > DOOR_WIDTH && hi[1] > DOOR_HEIGHT);
    }

    /// Whether the ray from `from` along `dir` crosses any triangle of `mesh`, by the
    /// Möller–Trumbore test (Möller and Trumbore, "Fast, Minimum Storage Ray/Triangle
    /// Intersection", Journal of Graphics Tools 2(1), 1997).
    fn ray_hits(mesh: &ItemMesh, from: V3, dir: V3) -> bool {
        mesh.parts.iter().any(|part| {
            part.indices.as_chunks::<3>().0.iter().any(|&[i, j, k]| {
                let [a, b, c] = [i, j, k].map(|n| part.positions[n as usize]);
                let (e1, e2) = (vec3::sub(b, a), vec3::sub(c, a));
                let p = vec3::cross(dir, e2);
                let det = vec3::dot(e1, p);
                if det.abs() < 1e-9 {
                    return false;
                }
                let s = vec3::sub(from, a);
                let u = vec3::dot(s, p) / det;
                let q = vec3::cross(s, e1);
                let v = vec3::dot(dir, q) / det;
                u >= 0.0 && v >= 0.0 && u + v <= 1.0 && vec3::dot(e2, q) / det > 0.0
            })
        })
    }

    #[test]
    fn stone_closes_a_doorway_cell_round_the_opening() {
        // The farthest the relief can move rock off its cell's plane: two octaves of value
        // noise, each at most half its amplitude.
        let peak = 0.5 + 0.35 * 0.5;
        for seed in [1207, 2] {
            let level = fb_level::generate(seed, 1);
            let view = CellView::new(&level);
            let doors = doorways(&view);
            assert!(!doors.is_empty());
            for door in doors {
                let frame = door_frame(level.ceiling, DoorStyle::Timber, level.seed ^ ((door.x as u32) << 16 | door.y as u32));
                // Out to where the walls and ceiling can stand, and down into the floor's dip.
                let (across, low, high) = (CELL * 0.5 + WALL_RELIEF * peak, -FLOOR_RELIEF * peak, level.ceiling + CEILING_RELIEF * peak);
                let mut rays = 0;
                for i in 0..=96 {
                    for j in 0..=80 {
                        let x = -across + 2.0 * across * i as f32 / 96.0;
                        let y = low + (high - low) * j as f32 / 80.0;
                        if x.abs() < DOOR_WIDTH * 0.5 + 0.1 && y < DOOR_HEIGHT - 0.1 {
                            continue;
                        }
                        // Straight through, and slanting as seen from a step off to one side.
                        for dir in [[0.0, 0.0, 1.0], vec3::normalize_or([0.5, 0.25, 1.0], [0.0, 0.0, 1.0])] {
                            let from = vec3::sub([x, y, 0.0], vec3::scale(dir, CELL));
                            assert!(ray_hits(&frame, from, dir), "seed {seed} door ({}, {}): open at x {x:.2} y {y:.2} along {dir:?}", door.x, door.y);
                            rays += 1;
                        }
                    }
                }
                assert!(rays > 5_000);
            }
        }
    }

    /// FNV-1a over every position, normal and index of the parts.
    fn hash_parts<'a>(parts: impl IntoIterator<Item = &'a Part>) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        let mut eat = |bits: u32| {
            for b in bits.to_le_bytes() {
                h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
            }
        };
        for part in parts {
            part.positions.iter().chain(&part.normals).flatten().for_each(|c| eat(c.to_bits()));
            part.indices.iter().for_each(|&i| eat(i));
        }
        h
    }

    #[test]
    fn rough_rock_geometry_is_unchanged() {
        for (seed, expected) in ROUGH_ROCK_HASHES {
            // Without the stairs, which came later and leave their cells unfloored.
            let level = Level { stairs: Vec::new(), ..fb_level::generate(seed, 1) };
            // With no cobbled or flagged floors and no layered walls, which came later.
            let view = CellView::uncobbled(&level);
            let mesh = level_mesh(&view, view.bounds());
            let frame = door_frame(level.ceiling, DoorStyle::Timber, level.seed);
            let seen = (hash_parts(mesh.surfaces.parts.iter().chain([&mesh.ink])), hash_parts(&frame.parts));
            assert_eq!(seen, expected, "rough rock changed for seed {seed}");
        }
    }

    #[test]
    fn the_first_level_draws_plain_stone_parts_only() {
        for seed in 1..=30 {
            let level = Level { stairs: Vec::new(), ..fb_level::generate(seed, 1) };
            let drawn = level_mesh(&CellView::new(&level), CellView::new(&level).bounds());
            let plain = CellView::uncobbled(&level);
            let bare = level_mesh(&plain, plain.bounds());
            assert_eq!(drawn.surfaces.parts.iter().map(|p| p.name).collect::<Vec<_>>(), ["floor", "walls", "ceiling"], "seed {seed}");
            assert_eq!(hash_parts(drawn.surfaces.parts.iter().chain([&drawn.ink])), hash_parts(bare.surfaces.parts.iter().chain([&bare.ink])), "seed {seed}");
        }
    }

    #[test]
    fn only_hewn_quarters_walls_are_split_off_for_the_craggy_shader() {
        let hewn = |l: &Level| CellView::new(l).finishes().quarters().iter().any(|q| q.character == fb_level::Character::Hewn);
        let level = (1..400).map(|seed| fb_level::generate(seed, 5)).find(hewn).expect("some cave has a hewn quarter");
        let view = CellView::new(&level);
        let mesh = level_mesh(&view, view.bounds());
        let tris = |name: &str| mesh.surfaces.parts.iter().find(|p| p.name == name).map_or(0, |p| p.indices.len());
        assert!(tris("hewn") > 0, "hewn walls have a part of their own");
        // Floors and ceilings stay plain stone, and the walls split lose nothing.
        let plain = CellView::with_finishes(&level, finishes(&level, &LevelParams { arch_chance: 0.0, strata_chance: 0.0, cobble_chance: 0.0, flagstone_chance: 0.0, ..LevelParams::for_style(level.style) }));
        let bare = level_mesh(&plain, plain.bounds());
        let walls = |m: &LevelMesh| m.surfaces.parts.iter().filter(|p| ["walls", "hewn", "strata"].contains(&p.name)).map(|p| p.indices.len()).sum::<usize>();
        assert_eq!(walls(&mesh), walls(&bare));
    }

    #[test]
    fn stone_vertices_carry_the_finishes_moss() {
        // Both styles: rough rock at depth 3, stone halls at depth 4 (the first level is dry).
        for depth in [3, 4] {
            let mut damp = 0;
            for seed in 1207..1237 {
                let level = fb_level::generate(seed, depth);
                let view = CellView::new(&level);
                let mesh = level_mesh(&view, view.bounds());
                for part in &mesh.surfaces.parts {
                    assert_eq!(part.uvs_b.len(), part.positions.len(), "{}", part.name);
                    assert!(part.validate().is_ok());
                    for (p, uv) in part.positions.iter().zip(&part.uvs_b) {
                        assert_eq!(uv[0], view.finishes().moss_at(p[0], p[2]));
                        assert!((0.0..=1.0).contains(&uv[0]));
                        damp += (uv[0] > 0.2) as usize;
                    }
                }
            }
            assert!(damp > 0, "no moss anywhere at depth {depth}");
        }
    }

    #[test]
    fn layered_rooms_have_their_own_walls() {
        // A cave with a layered quarter.
        let layered = |l: &Level| CellView::new(l).finishes().quarters().iter().any(|q| q.walls == fb_level::WallFinish::Strata);
        let level = (1..400).map(|seed| fb_level::generate(seed, 5)).find(layered).expect("some cave has strata");
        let view = CellView::new(&level);
        let mesh = level_mesh(&view, view.bounds());
        let walls = |name: &str| mesh.surfaces.parts.iter().find(|p| p.name == name).map_or(0, |p| p.indices.len());
        assert!(walls("strata") > 0, "a layered room");
        // Splitting the walls loses none: the unlayered total is the two together.
        let plain = CellView::with_finishes(&level, finishes(&level, &LevelParams { strata_chance: 0.0, ..LevelParams::for_style(level.style) }));
        let unsplit = level_mesh(&plain, plain.bounds());
        let total: usize = unsplit.surfaces.parts.iter().filter(|p| ["walls", "hewn", "strata"].contains(&p.name)).map(|p| p.indices.len()).sum();
        assert_eq!(walls("walls") + walls("strata") + walls("hewn"), total);
    }

    #[test]
    fn the_door_swings_from_open_to_shut() {
        assert!((entrance_swing(0.0) - ENTRANCE_OPEN_ANGLE).abs() < 1e-6);
        assert_eq!(entrance_swing(1.0), 0.0);
        assert_eq!(entrance_swing(2.0), 0.0);
        let mut last = entrance_swing(0.0);
        for i in 1..=20 {
            let a = entrance_swing(i as f32 / 20.0);
            assert!(a >= last);
            last = a;
        }
    }
}
