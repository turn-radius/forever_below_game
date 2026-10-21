//! Iron bars for the cell blocks of [`fb_level::past`], as plain mesh data like the rest of
//! [`super`]: the fronts and partitions of a cell block ([`Cell::Bars`]) and the barred gate
//! in each front ([`Cell::Gate`]).
//!
//! A Bars cell draws an *arm* toward each neighbour it joins (another Bars cell, a gate, or the
//! rock of the wall, when the cell runs on through it on the far side). Every arm carries posts
//! at the same 2.5 unit pitch from the cell centre, so bars keep their spacing across every cell
//! seam, with a flat rail near the foot and another near the head. A cell where the run turns
//! gets a stouter post at its centre. Posts rise to the vault over them.
//!
//! A gate is a fixed frame (jambs, a lintel, bars either side and above) and a leaf hinged on
//! its `-x` jamb that stands ajar, swung into the cell. Both are built with `x` along the bars
//! and `z` through them, on the cell's centre.
//!
//! How well the iron has kept ([`Keep`]) belongs to the cell block, not the bar: one block stands
//! in plain iron grey, another is weathered, another rusted through, and every bar and gate of a
//! block matches.

use armory::{
    ItemMesh, MaterialKind, Rgb,
    cond::{Condition, ConditionState},
    build::{
        parts::{Look, Parts},
        plank::plank,
    },
    geom::{MeshBuilder, Shading, SweepOptions, V3, cylinder, tube},
};
use fb_level::{
    CELL, Cell, Level, LevelStyle,
    door::{DOOR_HEIGHT, DOOR_WIDTH},
};
use procgen_rng::Mulberry32;

use super::stone::Vaults;

/// Bar radius (0.8 units thick, an 8 cm bar with a 17 cm gap), the pitch between posts, and the rails' height and thickness.
const BAR_RADIUS: f32 = 0.4;
const PITCH: f32 = 2.5;
const RAIL_HEIGHT: f32 = 1.8;
const RAIL_THICK: f32 = 0.6;
/// Posts stop this far under the vault.
const HEADROOM: f32 = 0.15;
/// How far a gate stands open, degrees from shut.
pub const GATE_AJAR: f32 = 70.0;
/// The gate leaf's hinge, across the cell from its centre.
pub const GATE_HINGE: f32 = -(DOOR_WIDTH * 0.5 - 0.4);

/// The tint of a rusted bar: a dull brown close to the iron, so a run reads as one piece.
const RUST: Rgb = Rgb::hex(0x4a3c33);

/// The four grid directions an arm can run, as `(dx, dz)`: east, south, west, north.
pub const ARMS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// A Bars cell and the neighbours its bars run toward, in [`ARMS`] order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarsSpot {
    pub cell: (i32, i32),
    pub arms: [bool; 4],
}

/// A Gate cell: whether its bars run along `x`, and which way the cell it opens into lies, as a
/// grid step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateSpot {
    pub cell: (i32, i32),
    pub along_x: bool,
    /// Grid step from the gate into its cell (the side the leaf swings toward).
    pub into: (i32, i32),
}

fn barred(c: Cell) -> bool {
    matches!(c, Cell::Bars | Cell::Gate)
}

fn solid(c: Cell) -> bool {
    matches!(c, Cell::Rock | Cell::Column)
}

/// The neighbours the bars of the Bars cell at `(x, y)` run toward, in [`ARMS`] order.
pub fn arms_of(level: &Level, x: i32, y: i32) -> [bool; 4] {
    let at = |k: usize| level.cell(x + ARMS[k].0, y + ARMS[k].1);
    let mut arms = [0, 1, 2, 3].map(|k| barred(at(k)));
    // Bars run on into a wall they meet head on.
    for k in 0..4 {
        if !arms[k] && solid(at(k)) && barred(at((k + 2) % 4)) {
            arms[k] = true;
        }
    }
    if !arms.iter().any(|&a| a) {
        arms[0] = true;
        arms[2] = true;
    }
    arms
}

/// Every Bars cell of the level with the arms it draws.
pub fn bars(level: &Level) -> Vec<BarsSpot> {
    let mut out = Vec::new();
    for y in 0..level.height as i32 {
        for x in 0..level.width as i32 {
            if level.cell(x, y) == Cell::Bars {
                out.push(BarsSpot { cell: (x, y), arms: arms_of(level, x, y) });
            }
        }
    }
    out
}

/// The gate at `(x, y)`: which way its bars run and which side is the cell.
pub fn gate_at(level: &Level, x: i32, y: i32) -> GateSpot {
    let along_x = barred(level.cell(x - 1, y)) || barred(level.cell(x + 1, y));
    let sides: [(i32, i32); 2] = if along_x { [(0, -1), (0, 1)] } else { [(-1, 0), (1, 0)] };
    // The cell side is the one whose row, a step over, runs into a partition.
    let enclosed = |s: (i32, i32)| {
        let (rx, ry) = (x + s.0, y + s.1);
        let run: [(i32, i32); 2] = if along_x { [(-1, 0), (1, 0)] } else { [(0, -1), (0, 1)] };
        run.iter().any(|d| (1..=4).map(|k| level.cell(rx + d.0 * k, ry + d.1 * k)).find(|c| *c != Cell::Floor) == Some(Cell::Bars))
    };
    let into = if enclosed(sides[0]) && !enclosed(sides[1]) { sides[0] } else { sides[1] };
    GateSpot { cell: (x, y), along_x, into }
}

/// Every Gate cell of the level.
pub fn gates(level: &Level) -> Vec<GateSpot> {
    let mut out = Vec::new();
    for y in 0..level.height as i32 {
        for x in 0..level.width as i32 {
            if level.cell(x, y) == Cell::Gate {
                out.push(gate_at(level, x, y));
            }
        }
    }
    out
}

/// The ceiling over world points, as the stone draws it.
pub struct Ceiling<'a> {
    vaults: Option<Vaults>,
    flat: f32,
    _level: std::marker::PhantomData<&'a Level>,
}

impl<'a> Ceiling<'a> {
    pub fn new(level: &'a Level) -> Self {
        let vaults = (level.style == LevelStyle::StoneHalls).then(|| Vaults::new(level));
        Ceiling { vaults, flat: level.ceiling, _level: std::marker::PhantomData }
    }

    pub fn at(&self, x: f32, z: f32) -> f32 {
        self.vaults.as_ref().map_or(self.flat, |v| v.height(x, z))
    }

    /// The lowest the ceiling comes over a post of bar radius at `(x, z)`.
    fn over_post(&self, x: f32, z: f32) -> f32 {
        let r = BAR_RADIUS;
        [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)].iter().map(|&(dx, dz)| self.at(x + dx, z + dz)).fold(f32::MAX, f32::min)
    }
}

fn hash(cell: (i32, i32), seed: u32, salt: u32) -> u32 {
    (cell.0 as u32).wrapping_mul(73_856_093) ^ (cell.1 as u32).wrapping_mul(19_349_663) ^ seed ^ salt
}

/// How well a cell block's iron has kept, the same for every bar and gate of the block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Keep {
    /// Plain iron grey, barely worn: no rusted bars, the odd one bent.
    Kept,
    /// Dulled, with a little rust and a few bars bent.
    Weathered,
    /// Rusted through: many bars brown, bent or gone.
    Rusted,
}

impl Keep {
    /// The keep of the cell block holding `cell`: drawn from the level's seed and the block's
    /// room, so a block keeps it on every visit. Two in five blocks are kept, a third weathered,
    /// the rest rusted.
    pub fn of(level: &Level, cell: (i32, i32)) -> Keep {
        let room = level.room_at(cell.0, cell.1).map_or(u32::MAX, |r| r as u32);
        let roll = Mulberry32::new(level.seed ^ room.wrapping_mul(0x9e37_79b9) ^ 0x1c07_ee90).range_f32(0.0, 1.0);
        if roll < 0.4 {
            Keep::Kept
        } else if roll < 0.75 {
            Keep::Weathered
        } else {
            Keep::Rusted
        }
    }

    /// The condition the block's iron is drawn in: kept iron is barely dulled, and even a rusted
    /// block stops short of neglect, so rust stays a tone on the iron.
    pub fn condition(self) -> Condition {
        match self {
            Keep::Kept => Condition { wear: 0.1, rust: 0.5, ..Condition::of(ConditionState::Serviceable) },
            Keep::Weathered => Condition { wear: 0.3, ..Condition::of(ConditionState::Serviceable) },
            Keep::Rusted => Condition { wear: 0.5, ..Condition::of(ConditionState::BattleWorn) },
        }
    }

    /// The tint of its sound iron: a plain iron grey when kept, darker as it weathers, browning
    /// when rusted.
    pub fn iron(self) -> Rgb {
        match self {
            Keep::Kept => Rgb::hex(0x474c53),
            Keep::Weathered => Rgb::hex(0x48494c),
            Keep::Rusted => Rgb::hex(0x3d3834),
        }
    }

    /// The share of bars missing, bent and rusted, each counted on from the last.
    fn odds(self) -> (f32, f32, f32) {
        match self {
            Keep::Kept => (0.0, 0.04, 0.04),
            Keep::Weathered => (0.02, 0.1, 0.22),
            Keep::Rusted => (0.06, 0.18, 0.45),
        }
    }
}

/// Wear on the bars of one run: which are bent, rusted or gone, by the block's [`Keep`].
struct Wear {
    rng: Mulberry32,
    keep: Keep,
}

enum Post {
    Sound,
    Rusty,
    Bent(f32),
    Missing,
}

impl Wear {
    fn post(&mut self, allow_missing: bool) -> Post {
        let roll = self.rng.range_f32(0.0, 1.0);
        let (missing, bent, rusty) = self.keep.odds();
        if allow_missing && roll < missing {
            Post::Missing
        } else if roll < bent {
            Post::Bent(self.rng.range_f32(0.5, 1.4) * if self.rng.chance(0.5) { 1.0 } else { -1.0 })
        } else if roll < rusty {
            Post::Rusty
        } else {
            Post::Sound
        }
    }
}

/// A vertical post from the floor to `top`, leaning `bend` out of the plane (along `normal`) at
/// its middle.
fn post(x: f32, z: f32, top: f32, bend: f32, normal: V3) -> MeshBuilder {
    if bend == 0.0 {
        let mut bar = cylinder(BAR_RADIUS, top, 8, Shading::SMOOTH);
        bar.translate([x, 0.0, z]);
        return bar;
    }
    let path: Vec<V3> = (0..=6)
        .map(|i| {
            let t = i as f32 / 6.0;
            let off = bend * (std::f32::consts::PI * t).sin().powi(2);
            [x + normal[0] * off, top * t, z + normal[2] * off]
        })
        .collect();
    tube(&path, 8, &SweepOptions::default(), |_| BAR_RADIUS)
}

/// A flat iron rail from `(x0, z0)` to `(x1, z1)` at height `y`.
fn rail(from: (f32, f32), to: (f32, f32), y: f32) -> MeshBuilder {
    let (dx, dz) = (to.0 - from.0, to.1 - from.1);
    let len = dx.hypot(dz);
    let mut r = plank(len, RAIL_HEIGHT, RAIL_THICK, 0.15);
    r.rotate_y((-dz).atan2(dx)).translate([(from.0 + to.0) * 0.5, y, (from.1 + to.1) * 0.5]);
    r
}

/// Tops and rails of the bars over a run, shared by arms and gates.
struct Builder {
    sound: MeshBuilder,
    rusty: MeshBuilder,
    rails: MeshBuilder,
    /// The tint of the sound iron, by the block's [`Keep`].
    iron: Rgb,
}

impl Builder {
    fn new(keep: Keep) -> Self {
        Builder { sound: MeshBuilder::new(), rusty: MeshBuilder::new(), rails: MeshBuilder::new(), iron: keep.iron() }
    }

    fn finish(self) -> ItemMesh {
        let mut parts = Parts::new();
        parts.add_look("bars", self.sound, Look::of(MaterialKind::Iron).tinted(self.iron));
        parts.add_look("rusted bars", self.rusty, Look::of(MaterialKind::Iron).tinted(RUST));
        parts.add_look("rails", self.rails, Look::of(MaterialKind::Iron).tinted(self.iron));
        parts.finish()
    }

    fn put(&mut self, mesh: MeshBuilder, kind: &Post) {
        match kind {
            Post::Rusty => self.rusty.append(&mesh),
            _ => self.sound.append(&mesh),
        };
    }
}

/// The bars of one Bars cell, on the cell's centre at floor height.
pub fn bars_mesh(spot: &BarsSpot, level: &Level, ceiling: &Ceiling) -> ItemMesh {
    let (cx, cz) = ((spot.cell.0 as f32 + 0.5) * CELL, (spot.cell.1 as f32 + 0.5) * CELL);
    let half = CELL * 0.5;
    let keep = Keep::of(level, spot.cell);
    let mut wear = Wear { rng: Mulberry32::new(hash(spot.cell, level.seed, 0xba25_0001)), keep };
    let mut out = Builder::new(keep);
    let count = spot.arms.iter().filter(|&&a| a).count();
    let straight = count <= 2 && (spot.arms[0] == spot.arms[2]) && (spot.arms[1] == spot.arms[3]);

    // The post at the centre where the run turns or ends.
    if !straight {
        let top = ceiling.over_post(cx, cz) - HEADROOM;
        let mut stout = cylinder(BAR_RADIUS * 1.6, top, 10, Shading::SMOOTH);
        stout.translate([0.0, 0.0, 0.0]);
        out.sound.append(&stout);
    }
    for (k, &(dx, dz)) in ARMS.iter().enumerate() {
        if !spot.arms[k] {
            continue;
        }
        let (ux, uz) = (dx as f32, dz as f32);
        let normal: V3 = [uz.abs(), 0.0, ux.abs()];
        // Rails to the seam, and a little into a wall where the arm runs into rock.
        let into_wall = solid(level.cell(spot.cell.0 + dx, spot.cell.1 + dz));
        let reach = half + if into_wall { 0.4 } else { 0.0 };
        let low_rail = 3.0;
        let mut min_ceiling = f32::MAX;
        for i in 0..4 {
            let s = PITCH * 0.5 + PITCH * i as f32;
            let (px, pz) = (ux * s, uz * s);
            min_ceiling = min_ceiling.min(ceiling.over_post(cx + px, cz + pz));
            let top = ceiling.over_post(cx + px, cz + pz) - HEADROOM;
            // A bar is never missing beside the centre or at the seam, so a run keeps its ends.
            let kind = wear.post(i > 0 && i < 3);
            match kind {
                Post::Missing => {}
                Post::Bent(b) => out.put(post(px, pz, top, b, normal), &kind),
                _ => out.put(post(px, pz, top, 0.0, normal), &kind),
            }
        }
        let high_rail = min_ceiling - HEADROOM - 3.0;
        out.rails.append(&rail((0.0, 0.0), (ux * reach, uz * reach), low_rail));
        out.rails.append(&rail((0.0, 0.0), (ux * reach, uz * reach), high_rail));
    }
    out.finish()
}

/// The gate's fixed frame, and its leaf hung at its hinge (at the origin, shut along `+x`).
pub struct GateMesh {
    pub frame: ItemMesh,
    pub leaf: ItemMesh,
    /// The leaf's hinge, from the cell centre.
    pub hinge: V3,
    /// How high the frame's opening is.
    pub opening: f32,
}

pub fn gate_mesh(spot: &GateSpot, level: &Level, ceiling: &Ceiling) -> GateMesh {
    let (cx, cz) = ((spot.cell.0 as f32 + 0.5) * CELL, (spot.cell.1 as f32 + 0.5) * CELL);
    // World offsets of local (x, z) from the cell centre: a gate along z is turned a quarter.
    let world = |x: f32, z: f32| if spot.along_x { (cx + x, cz + z) } else { (cx + z, cz - x) };
    let half = CELL * 0.5;
    let iron_keep = Keep::of(level, spot.cell);
    let mut wear = Wear { rng: Mulberry32::new(hash(spot.cell, level.seed, 0x6a7e_0002)), keep: iron_keep };
    let mut fixed = Builder::new(iron_keep);
    let (mut pins, mut lock) = (MeshBuilder::new(), MeshBuilder::new());
    let mut min_ceiling = f32::MAX;
    let tops = |x: f32| {
        let (wx, wz) = world(x, 0.0);
        ceiling.over_post(wx, wz) - HEADROOM
    };
    for i in 0..8 {
        min_ceiling = min_ceiling.min(tops(-half + PITCH * (i as f32 + 0.5)));
    }
    let opening = DOOR_HEIGHT.min(min_ceiling - 5.0);
    let jamb = DOOR_WIDTH * 0.5 + 0.9;

    // Jambs, and the bars beside the opening and over it.
    for side in [-1.0f32, 1.0] {
        let mut j = cylinder(0.95, opening, 10, Shading::SMOOTH);
        j.translate([side * jamb, 0.0, 0.0]);
        fixed.sound.append(&j);
        let x = side * (half - PITCH * 0.5);
        let kind = Post::Sound;
        fixed.put(post(x, 0.0, tops(x), 0.0, [0.0, 0.0, 1.0]), &kind);
        fixed.rails.append(&rail((side * jamb, 0.0), (side * half, 0.0), 3.0));
        if side < 0.0 {
            // Hinge pins, standing off the jamb.
            for y in [4.0, opening - 4.0] {
                let mut pin = cylinder(0.5, 2.4, 8, Shading::SMOOTH);
                pin.translate([GATE_HINGE, y - 1.2, 0.0]);
                pins.append(&pin);
            }
        }
    }
    let mut x = -half + PITCH * 0.5;
    while x < half {
        let top = tops(x);
        let kind = wear.post(false);
        match kind {
            Post::Bent(b) => fixed.put(post(x, opening, top, b, [0.0, 0.0, 1.0]), &kind),
            _ => {
                let mut bar = cylinder(BAR_RADIUS, top - opening, 8, Shading::SMOOTH);
                bar.translate([x, opening, 0.0]);
                fixed.put(bar, &kind);
            }
        }
        x += PITCH;
    }
    fixed.rails.append(&rail((-half, 0.0), (half, 0.0), opening + 0.6));
    let high = min_ceiling - HEADROOM - 3.0;
    if high > opening + 3.0 {
        fixed.rails.append(&rail((-half, 0.0), (half, 0.0), high));
    }

    // The leaf, hinged at the origin and running along +x.
    let width = DOOR_WIDTH - 0.8;
    let mut leaf = Builder::new(iron_keep);
    let (hang_top, hang_low) = (opening - 1.0, 2.0);
    for s in [0.0, width] {
        let mut stile = cylinder(0.7, hang_top - hang_low + RAIL_HEIGHT, 8, Shading::SMOOTH);
        stile.translate([s, hang_low - RAIL_HEIGHT * 0.5, 0.0]);
        leaf.sound.append(&stile);
    }
    for y in [hang_low, (hang_low + hang_top) * 0.5, hang_top] {
        leaf.rails.append(&rail((0.0, 0.0), (width, 0.0), y));
    }
    let n = (width / PITCH).round() as i32;
    for i in 1..n {
        let x = width * i as f32 / n as f32;
        let kind = wear.post(false);
        let mut bar = cylinder(BAR_RADIUS * 0.9, hang_top - hang_low, 8, Shading::SMOOTH);
        bar.translate([x, hang_low, 0.0]);
        leaf.put(bar, &kind);
    }
    // Strap hinges over the leaf's face, and the lock box on the latch stile.
    for y in [4.0, opening - 4.0] {
        leaf.rails.append(&rail((0.0, 0.0), (3.5, 0.0), y + 0.3));
    }
    let mut boxed = plank(3.2, 3.0, 1.9, 0.3);
    boxed.translate([width - 2.0, 9.5, 0.0]);
    lock.append(&boxed);
    let mut keep = cylinder(0.35, 2.2, 8, Shading::SMOOTH);
    keep.rotate_x(std::f32::consts::FRAC_PI_2).translate([width - 2.0, 9.5, -1.1]);
    lock.append(&keep);

    let mut frame = fixed.finish();
    let mut extra = Parts::new();
    extra.add_look("hinge pins", pins, Look::of(MaterialKind::Iron).tinted(iron_keep.iron()).without_outline());
    frame.parts.extend(extra.finish().parts);
    let mut leaf_mesh = leaf.finish();
    let mut extra = Parts::new();
    extra.add_look("lock box", lock, Look::of(MaterialKind::Iron).tinted(iron_keep.iron()));
    leaf_mesh.parts.extend(extra.finish().parts);
    GateMesh { frame, leaf: leaf_mesh, hinge: [GATE_HINGE, 0.0, 0.0], opening }
}

/// The grid-axis turn about `y` that carries a gate's local `x` onto the cell's run.
pub fn gate_turn(spot: &GateSpot) -> f32 {
    if spot.along_x { 0.0 } else { std::f32::consts::FRAC_PI_2 }
}

/// The leaf's turn about its hinge, so it stands ajar toward the cell it opens into: radians
/// about `y` in the gate's local frame.
pub fn leaf_swing(spot: &GateSpot) -> f32 {
    // Local +z, in the world, is south along x and east along z.
    let local_z = if spot.along_x { (0, 1) } else { (1, 0) };
    let toward = if spot.into == local_z { 1.0 } else { -1.0 };
    // Rotating about y by a carries +x to (cos a, 0, -sin a).
    -toward * GATE_AJAR.to_radians()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_levels() -> Vec<Level> {
        [2u32, 3, 5].iter().map(|&s| fb_level::generate(s, 2)).collect()
    }

    #[test]
    fn a_cell_blocks_iron_keeps_alike_and_every_keep_shows_up() {
        let mut seen = std::collections::HashSet::new();
        for level in (1..=40).flat_map(|s| [2, 4].map(|d| fb_level::generate(s, d))) {
            for r in level.rooms_for(fb_level::Purpose::CellBlock) {
                let room = level.rooms[r];
                let cells: Vec<(i32, i32)> = (room.y..room.y + room.h).flat_map(|y| (room.x..room.x + room.w).map(move |x| (x as i32, y as i32))).filter(|&(x, y)| matches!(level.cell(x, y), Cell::Bars | Cell::Gate)).collect();
                let keep = Keep::of(&level, cells[0]);
                assert!(cells.iter().all(|&c| Keep::of(&level, c) == keep), "seed {}: a block of mixed iron", level.seed);
                seen.insert(keep);
            }
        }
        assert_eq!(seen.len(), 3, "{seen:?}");
    }

    #[test]
    fn every_bars_cell_and_gate_gets_a_piece() {
        for level in block_levels() {
            let n = level.cells.iter().filter(|c| **c == Cell::Bars).count();
            let g = level.cells.iter().filter(|c| **c == Cell::Gate).count();
            assert!(n > 0 && g > 0, "seed {} has a cell block", level.seed);
            assert_eq!(bars(&level).len(), n);
            assert_eq!(gates(&level).len(), g);
        }
    }

    #[test]
    fn a_bars_mesh_stays_in_its_cell_and_under_the_ceiling() {
        for level in block_levels() {
            let ceiling = Ceiling::new(&level);
            for spot in bars(&level) {
                let mesh = bars_mesh(&spot, &level, &ceiling);
                let (cx, cz) = ((spot.cell.0 as f32 + 0.5) * CELL, (spot.cell.1 as f32 + 0.5) * CELL);
                assert!(!mesh.parts.iter().all(|p| p.positions.is_empty()), "bars at {:?}", spot.cell);
                for part in &mesh.parts {
                    for p in &part.positions {
                        assert!((p[0] - 0.0).abs() <= CELL * 0.5 + 0.5 && (p[2]).abs() <= CELL * 0.5 + 0.5, "{:?} {p:?}", spot.cell);
                        assert!(p[1] >= -0.05 && p[1] <= ceiling.at(cx + p[0], cz + p[2]) + 0.01, "{:?} {p:?}", spot.cell);
                    }
                }
            }
        }
    }

    #[test]
    fn arms_reach_their_own_side() {
        let level = fb_level::generate(2, 2);
        let ceiling = Ceiling::new(&level);
        for spot in bars(&level) {
            let mesh = bars_mesh(&spot, &level, &ceiling);
            for (k, &(dx, dz)) in ARMS.iter().enumerate() {
                let reaches = mesh.parts.iter().flat_map(|p| &p.positions).any(|p| p[0] * dx as f32 + p[2] * dz as f32 > CELL * 0.5 - 0.5);
                assert_eq!(reaches, spot.arms[k], "arm {k} of {:?}", spot.cell);
            }
        }
    }

    #[test]
    fn a_gate_stands_ajar_into_its_cell_under_the_ceiling() {
        for level in block_levels() {
            let ceiling = Ceiling::new(&level);
            for spot in gates(&level) {
                let g = gate_mesh(&spot, &level, &ceiling);
                assert!(g.frame.parts.iter().chain(&g.leaf.parts).all(|p| !p.positions.is_empty()));
                let a = leaf_swing(&spot);
                // The leaf's free end, turned about the hinge.
                let end = [GATE_HINGE + (DOOR_WIDTH - 0.8) * a.cos(), -(DOOR_WIDTH - 0.8) * a.sin()];
                let local_z = if spot.along_x { (0, 1) } else { (1, 0) };
                let toward = if spot.into == local_z { 1.0 } else { -1.0 };
                assert!(end[1] * toward > 0.0, "the leaf swings into the cell");
                assert!(level.cell(spot.cell.0 + spot.into.0, spot.cell.1 + spot.into.1) == Cell::Floor);
            }
        }
    }

    #[test]
    fn no_wall_face_stands_on_a_bars_cell() {
        for level in block_levels() {
            let view = super::super::CellView::new(&level);
            let mesh = super::super::level_mesh(&view, view.bounds());
            let walls = mesh.surfaces.parts.iter().find(|p| p.name == "walls").expect("walls");
            for spot in bars(&level) {
                let (x0, z0) = (spot.cell.0 as f32 * CELL, spot.cell.1 as f32 * CELL);
                let inside = walls.positions.iter().any(|p| p[0] > x0 + 1.2 && p[0] < x0 + CELL - 1.2 && p[2] > z0 + 1.2 && p[2] < z0 + CELL - 1.2);
                assert!(!inside, "a wall inside the bars cell {:?}", spot.cell);
            }
        }
    }
}
