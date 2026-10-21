//! Stone arches: doorways in the hall's round and pointed arches and the cave's rock-cut one,
//! built from wedge voussoirs over dressed jambs, with the wall's coursed masonry cut round the
//! ring. An arch is described in its doorway's space: `x` across, `y` up from the floor, `z`
//! through, centred on the cell.

use armory::{
    ItemMesh, MaterialKind, Rgb,
    build::{
        parts::{Look, Parts},
        plank::plank,
    },
    geom::{MeshBuilder, P2, extrude, signed_area},
};
use fb_level::{CELL, Cell, Character, DoorStyle, Finishes, Level, LevelStyle};
use procgen_rng::Mulberry32;

use super::{DOOR_HEIGHT, DOOR_WIDTH, FLOOR_RELIEF, INFILL_DEPTH, stone};

/// How thick the ring of voussoirs is, measured radially.
const RING: f32 = 3.0;
/// How far the ring stands proud of its wall, as a whole depth through the wall.
const RING_DEPTH: f32 = 4.6;
/// The keystone rises this far above a doorway's ring.
const KEY_RISE: f32 = 1.0;
/// The keystone stands this much further out than the ring.
const KEY_PROUD: f32 = 0.9;
/// Half of the keystone's width, along the intrados.
const KEY_HALF: f32 = 1.7;
/// The impost under the ring's first stone, and its depth through the wall.
const IMPOST_HEIGHT: f32 = 1.5;
const IMPOST_DEPTH: f32 = 5.0;
/// The jambs' depth through the wall.
const JAMB_DEPTH: f32 = 4.2;
/// A voussoir's wedge is about this long along the intrados.
const WEDGE: f32 = 3.4;
/// Masonry cut round the ring is laid this far under the ring's outer face, so it tucks behind
/// the stones and no seam shows.
const TUCK: f32 = 0.4;
/// The hall arches' stone, paler than the walls so the ring reads against them.
const ARCH_STONE: Rgb = Rgb::hex(0xa5a39a);
/// The rock-cut rim's stone, a little paler than the cave's walls.
const CAVE_ARCH: Rgb = Rgb::hex(0x7d7a72);
/// The rock-cut arch's stones are rougher: thicker, deeper and more chamfered.
const ROCK_RING: (f32, f32) = (2.4, 3.8);

pub(super) use fb_level::door::Arch;

/// The angle at which `arch`'s arcs meet.
fn theta_max(arch: &Arch) -> f32 {
    ((arch.r - arch.hw) / arch.r).acos()
}

/// The point at angle `theta` on the arc of `side` (+1 right, -1 left), at `radius`.
fn point(arch: &Arch, side: f32, theta: f32, radius: f32) -> P2 {
    let c = arch.hw - arch.r;
    [side * (c + radius * theta.cos()), arch.spring + radius * theta.sin()]
}

/// A convex polygon, however it is wound, as a block `depth` through the wall centred on
/// `z = 0`, its faces chamfered by `ch` all round.
pub(super) fn prism(poly: &[P2], depth: f32, ch: f32) -> MeshBuilder {
    let mut poly = poly.to_vec();
    if signed_area(&poly) < 0.0 {
        poly.reverse();
    }
    let n = poly.len();
    let mut mesh = MeshBuilder::new();
    if n < 3 {
        return mesh;
    }
    let edge = |i: usize| (poly[i], poly[(i + 1) % n]);
    let normal = |i: usize| {
        let (a, b) = edge(i);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = (dx * dx + dy * dy).sqrt().max(1e-6);
        [-dy / l, dx / l]
    };
    let shortest = (0..n).map(|i| {
        let (a, b) = edge(i);
        ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
    });
    let ch = ch.min(shortest.fold(f32::MAX, f32::min) * 0.3).min(depth * 0.25);
    // The face's outline, each corner moved in along the mitre.
    let inset: Vec<P2> = (0..n)
        .map(|i| {
            let (p, a, b) = (poly[i], normal((i + n - 1) % n), normal(i));
            let k = ch / (1.0 + a[0] * b[0] + a[1] * b[1]).max(0.3);
            [p[0] + (a[0] + b[0]) * k, p[1] + (a[1] + b[1]) * k]
        })
        .collect();
    let h = depth * 0.5;
    let uv = |p: [f32; 3]| [p[0] / CELL, p[1] / CELL];
    let at = |p: P2, z: f32| [p[0], p[1], z];
    for side in [1.0f32, -1.0] {
        let face = side * h;
        let rim = side * (h - ch);
        for i in 1..n - 1 {
            let tri = [at(inset[0], face), at(inset[i], face), at(inset[i + 1], face)];
            mesh.push_flat_triangle(tri, tri.map(uv), [0.0, 0.0, side]);
        }
        for i in 0..n {
            let j = (i + 1) % n;
            let nm = normal(i);
            let quad = [at(poly[i], rim), at(poly[j], rim), at(inset[j], face), at(inset[i], face)];
            mesh.push_flat_quad(quad, quad.map(uv), [-nm[0], -nm[1], side]);
        }
    }
    let (front, back) = (h - ch, -(h - ch));
    for i in 0..n {
        let j = (i + 1) % n;
        let nm = normal(i);
        let quad = [at(poly[i], front), at(poly[j], front), at(poly[j], back), at(poly[i], back)];
        mesh.push_flat_quad(quad, quad.map(uv), [-nm[0], -nm[1], 0.0]);
    }
    mesh
}

/// A block `lo..hi` across by `bottom..top` up, `depth` through the wall.
fn block(lo: f32, hi: f32, bottom: f32, top: f32, depth: f32, ch: f32) -> MeshBuilder {
    prism(&[[lo, bottom], [hi, bottom], [hi, top], [lo, top]], depth, ch)
}

/// `poly` (convex) cut to the side of the line `y = from.1 + slope * (x - from.0)` that lies above.
fn above(poly: &[P2], from: P2, slope: f32) -> Vec<P2> {
    let d = |p: P2| p[1] - (from[1] + slope * (p[0] - from[0]));
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (da, db) = (d(a), d(b));
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            let t = da / (da - db);
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

/// The wedge stones of the ring over `arch`, `thick` radially and `depth` through the wall,
/// from each spring up to a keystone seat that rises `key_rise` over the ring and stands proud
/// of it (none when `rough`, whose
/// stones are thicker, deeper and wander outward).
fn voussoirs(arch: &Arch, (joint, chamfer): (f32, f32), (thick, depth): (f32, f32), key_rise: f32, rough: bool, rng: &mut Mulberry32) -> MeshBuilder {
    let mut ring = MeshBuilder::new();
    let r_mid = arch.r + thick * 0.5;
    let theta_max = theta_max(&arch);
    let key_theta = (KEY_HALF / r_mid).min(theta_max * 0.4);
    let seat = theta_max - key_theta;
    let stones = ((r_mid * seat / WEDGE).round() as usize).max(2);
    let gap_in = joint / arch.r;
    for side in [-1.0f32, 1.0] {
        let mut lo = 0.0;
        for k in 0..stones {
            let hi = if k + 1 == stones { seat } else { seat * (k + 1) as f32 / stones as f32 + if rough { rng.range_f32(-0.08, 0.08) } else { 0.0 } };
            let (a, b) = (lo + gap_in * 0.5, (hi - gap_in * 0.5).max(lo + gap_in));
            lo = hi;
            let t = if rough { rng.range_f32(ROCK_RING.0, ROCK_RING.1) } else { thick };
            // The rock-cut rim wanders outward only, so the opening is never narrower.
            let inner = if rough { rng.range_f32(0.0, 0.3) } else { 0.0 };
            let poly = [point(&arch, side, a, arch.r + inner), point(&arch, side, b, arch.r + inner), point(&arch, side, b, arch.r + t), point(&arch, side, a, arch.r + t)];
            let depth = depth + if rough { rng.range_f32(0.0, 0.9) } else { (rng.range_f32(0.0, 1.0) - 0.5) * 0.25 };
            let mut stone = prism(&poly, depth, chamfer);
            stone.translate([0.0, 0.0, (rng.range_f32(0.0, 1.0) - 0.5) * 0.2]);
            ring.append(&stone);
        }
    }
    if !rough {
        // The keystone: wedge-sided over the crown, rising a little above the ring.
        let key_in = |side: f32| point(&arch, side, seat, arch.r);
        let key_out = |side: f32| point(&arch, side, seat, arch.r + thick);
        let crown = arch.height(0.0, arch.r + thick).unwrap_or(arch.spring + arch.r + thick) + key_rise;
        let poly = [key_in(-1.0), key_in(1.0), key_out(1.0), [1.2, crown], [-1.2, crown], key_out(-1.0)];
        ring.append(&prism(&poly, depth + KEY_PROUD, chamfer));
    }
    ring
}

/// A doorway under a stone arch in its own space: dressed jambs and imposts, the ring of
/// voussoirs with its keystone (rougher and unkeyed for [`DoorStyle::RockCut`]), and the
/// wall's masonry laid in `courses` up to `top`, reaching `reach` either side of the centre,
/// cut round the ring's outer face.
pub fn door_arch(style: DoorStyle, courses: &[(f32, f32)], top: f32, reach: f32, (joint, chamfer): (f32, f32), seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0xa4c4_d007);
    let rough = style == DoorStyle::RockCut;
    let arch = Arch::new(style);
    let (hw, spring) = (arch.hw, arch.spring);
    let ring_chamfer = if rough { chamfer * 1.6 } else { chamfer * 1.3 };

    // The thinnest a stone is, which the masonry is cut to.
    let thin = if rough { ROCK_RING.0 } else { RING };
    let mut ring = voussoirs(&arch, (joint, ring_chamfer), (RING, RING_DEPTH), KEY_RISE, rough, &mut rng);

    // Jambs of dressed blocks up to the impost, the stones alternating wide and narrow.
    let impost_bottom = if rough { spring } else { spring - IMPOST_HEIGHT };
    let courses_up = ((impost_bottom / 4.4).round() as usize).max(2);
    let mut jambs = MeshBuilder::new();
    for side in [-1.0f32, 1.0] {
        let mut bottom = 0.0;
        for k in 0..courses_up {
            let h = if rough { rng.range_f32(3.4, 5.6) } else { impost_bottom / courses_up as f32 };
            let top_of = if k + 1 == courses_up { impost_bottom } else { (bottom + h).min(impost_bottom - 1.0) };
            let long = if rough { rng.range_f32(0.0, 1.8) } else if k % 2 == 0 { 0.0 } else { 1.3 };
            let (x0, x1) = (hw, hw + RING + long);
            let depth = JAMB_DEPTH + if rough { rng.range_f32(0.0, 0.8) } else { 0.0 };
            let (lo, hi) = if side < 0.0 { (-x1, -x0) } else { (x0, x1) };
            jambs.append(&block(lo, hi, bottom + joint * 0.5, top_of - joint * 0.5, depth, ring_chamfer));
            bottom = top_of;
        }
        if !rough {
            let (x0, x1) = (hw, hw + RING + 0.7);
            let (lo, hi) = if side < 0.0 { (-x1, -x0) } else { (x0, x1) };
            jambs.append(&block(lo, hi, impost_bottom, spring - joint * 0.5, IMPOST_DEPTH, ring_chamfer));
        }
    }

    // The wall's masonry, cut round the ring's outer face.
    let cut = arch.r + thin;
    let edge_x = hw + (cut - arch.r);
    let mut masonry = MeshBuilder::new();
    let gap = joint;
    let mut piece = |lo: f32, hi: f32, bottom: f32, top_of: f32, rng: &mut Mulberry32| {
        let (lo, hi) = (lo + gap * 0.5, hi - gap * 0.5);
        if hi - lo < 0.6 || top_of - bottom < 0.6 {
            return;
        }
        let inside = lo >= -edge_x - 1e-3 && hi <= edge_x + 1e-3;
        let mut poly = vec![[lo, bottom], [hi, bottom], [hi, top_of], [lo, top_of]];
        if inside {
            // Above the outer face: its chord, a hair lower, so the block tucks behind the ring.
            let (Some(ya), Some(yb)) = (arch.height(lo, cut), arch.height(hi, cut)) else { return };
            let (ya, yb) = (ya - TUCK, yb - TUCK);
            poly = above(&poly, [lo, ya], (yb - ya) / (hi - lo));
            let area = signed_area(&poly).abs();
            if poly.len() < 3 || area < 0.6 {
                return;
            }
        }
        let depth = INFILL_DEPTH + (rng.range_f32(0.0, 1.0) - 0.5) * 0.4;
        let mut stone = prism(&poly, depth, chamfer);
        stone.translate([0.0, 0.0, (rng.range_f32(0.0, 1.0) - 0.5) * 0.25]);
        masonry.append(&stone);
    };
    for (row, &(y, h)) in courses.iter().enumerate() {
        let (bottom, top_of) = (y + gap * 0.5, y + h - gap * 0.5);
        // Stagger alternate courses by half a block.
        let mut x = -reach - if row % 2 == 1 { 3.0 } else { 0.0 };
        while x < reach {
            let len = 5.2 + rng.range_f32(0.0, 1.0) * 2.4;
            let (a, b) = (x.max(-reach), (x + len).min(reach));
            x += len;
            // Cut where the ring's span starts and ends, and at the crown, under the keystone.
            let mut at = vec![a];
            at.extend([-edge_x, 0.0, edge_x].into_iter().filter(|&c| c > a + 0.05 && c < b - 0.05));
            at.push(b);
            for pair in at.windows(2) {
                piece(pair[0], pair[1], bottom, top_of, &mut rng);
            }
        }
    }
    // A thinner core of stone behind, so joints read as recesses rather than slits: the wall
    // either side, and over the arch's intrados.
    let core_depth = INFILL_DEPTH * 0.6;
    let floor = -FLOOR_RELIEF;
    let inner = hw + 0.05;
    for (lo, hi) in [(-reach, -inner), (inner, reach)] {
        let mut core = plank(hi - lo, top - floor, core_depth, 0.1);
        core.translate([(lo + hi) * 0.5, (floor + top) * 0.5, 0.0]);
        masonry.append(&core);
    }
    let (steps, theta_max) = (14, theta_max(&arch));
    let mut outline: Vec<P2> = vec![[-inner, top], [-inner, spring], [-hw, spring]];
    let intrados = |side: f32, theta: f32| point(&arch, side, theta, arch.r);
    outline.extend((1..=steps).map(|i| intrados(-1.0, theta_max * i as f32 / steps as f32)));
    outline.extend((0..steps).rev().map(|i| intrados(1.0, theta_max * i as f32 / steps as f32)));
    outline.extend([[inner, spring], [inner, top]]);
    outline.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4);
    masonry.append(&extrude(&outline, &[], core_depth));

    let mut parts = Parts::new();
    parts.add("masonry", masonry, MaterialKind::Stone);
    ring.append(&jambs);
    parts.add_look("arch", ring, Look::of(MaterialKind::Stone).tinted(if rough { CAVE_ARCH } else { ARCH_STONE }));
    parts.finish()
}

// ---------------------------------------------------------------------------------------------
// Corridor arches
// ---------------------------------------------------------------------------------------------

/// How far a pilaster's face stands from the corridor's wall plane.
const PILASTER_DEPTH: f32 = 1.5;
/// A pilaster's width along the corridor, and the plinth's and impost's, a little wider.
const PILASTER_ALONG: f32 = 4.6;
const PILASTER_FOOT_ALONG: f32 = 5.4;
const PLINTH_HEIGHT: f32 = 2.0;
/// How far the plinth and impost stand out from the pilaster's face.
const IMPOST_PROJECTION: f32 = 0.7;
/// The rib's thickness, radially, and its width along the corridor.
const RIB_THICK: f32 = 2.4;
const RIB_ALONG: f32 = 3.4;
/// The rib's keystone rises this far over the rib, so it ends just under the soffit.
const RIB_KEY_RISE: f32 = 0.35;
/// The rib's crown stands this far under the soffit, with the keystone's rise to spare.
const SOFFIT_GAP: f32 = 0.45;
/// Cells apart, at least and at most, the arches along a run stand.
const ARCH_SPACING: (u32, u32) = (2, 4);
/// The corridor's cell is the same height to its edge, this near.
const FLAT_SOFFIT: f32 = 1e-3;
/// Joint and chamfer of the pilasters' and rib's stones.
const RIB_STONE: (f32, f32) = (0.3, 0.35);
/// The pilasters and rib take the columns' stone.
const RIB_TINT: Rgb = stone::COLUMN_STONE;

/// A stretch of corridor cells in a line, each with rock to both sides: no door, turn,
/// junction or room in it.
#[derive(Clone, Debug, PartialEq)]
pub struct CorridorRun {
    pub cells: Vec<(i32, i32)>,
    /// Whether it runs east to west (otherwise north to south).
    pub along_x: bool,
}

/// An arch spanning one corridor cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CorridorArch {
    pub cell: (i32, i32),
    pub along_x: bool,
    /// The height of the corridor's flat soffit over the cell.
    pub soffit: f32,
    /// Whether its rib comes to a point, as its quarter's doorways do, rather than round.
    pub pointed: bool,
}

/// Which way the corridor through `(x, y)` runs, if it is a plain cell of one: open floor
/// outside every room, with open neighbours on two opposite sides and rock on the others.
fn straight(level: &Level, x: i32, y: i32) -> Option<bool> {
    if level.cell(x, y) != Cell::Floor || level.room_at(x, y).is_some() {
        return None;
    }
    let (n, e, s, w) = (level.is_open(x, y - 1), level.is_open(x + 1, y), level.is_open(x, y + 1), level.is_open(x - 1, y));
    match (n, e, s, w) {
        (false, true, false, true) => Some(true),
        (true, false, true, false) => Some(false),
        _ => None,
    }
}

/// Every straight stretch of corridor in the level, in reading order.
pub fn corridor_runs(level: &Level) -> Vec<CorridorRun> {
    let mut runs: Vec<CorridorRun> = Vec::new();
    for along_x in [true, false] {
        let (w, h) = (level.width as i32, level.height as i32);
        let (lines, length) = if along_x { (h, w) } else { (w, h) };
        for line in 0..lines {
            let mut run: Vec<(i32, i32)> = Vec::new();
            for at in 0..=length {
                let (x, y) = if along_x { (at, line) } else { (line, at) };
                if at < length && straight(level, x, y) == Some(along_x) {
                    run.push((x, y));
                } else if !run.is_empty() {
                    runs.push(CorridorRun { cells: std::mem::take(&mut run), along_x });
                }
            }
        }
    }
    runs
}

/// The corridor arches of a level of stone halls: along each straight run one arch every two to
/// four cells from the level's seed, never in a cell beside a door, a room, a turn or a stair, nor
/// where the soffit is not flat, and only in the corridors of a Vaulted quarter, in that
/// quarter's one arch shape. A pure function of the level and its finishes. Rough rock has none.
pub fn corridor_arches(level: &Level, finishes: &Finishes) -> Vec<CorridorArch> {
    if level.style != LevelStyle::StoneHalls {
        return Vec::new();
    }
    let vaults = stone::Vaults::new(level);
    let soffit = level.ceiling;
    let flat = |x: i32, y: i32| {
        let (cx, cz) = ((x as f32 + 0.5) * CELL, (y as f32 + 0.5) * CELL);
        [(0.0, 0.0), (-0.5, 0.0), (0.5, 0.0), (0.0, -0.5), (0.0, 0.5)].iter().all(|&(dx, dz)| (vaults.height(cx + dx * CELL, cz + dz * CELL) - soffit).abs() < FLAT_SOFFIT)
    };
    let near_stair = |x: i32, y: i32| level.stairs.iter().any(|s| (s.x as i32 - x).abs() <= 1 && (s.y as i32 - y).abs() <= 1);
    // A plain corridor cell: floor, outside every room.
    let plain = |x: i32, y: i32| level.cell(x, y) == Cell::Floor && level.room_at(x, y).is_none();
    let mut out = Vec::new();
    for run in corridor_runs(level) {
        let (x0, y0) = run.cells[0];
        let mut rng = Mulberry32::new(level.seed ^ level.depth.wrapping_mul(0x9e37_79b9) ^ (x0 as u32).wrapping_mul(0x85eb_ca6b) ^ (y0 as u32).wrapping_mul(0xc2b2_ae35) ^ 0xa4c4_c0a0);
        let mut i = rng.int(0, ARCH_SPACING.1 - ARCH_SPACING.0) as usize;
        while i < run.cells.len() {
            let (x, y) = run.cells[i];
            let (dx, dy) = if run.along_x { (1, 0) } else { (0, 1) };
            let vaulted = finishes.quarter(x, y).map(|q| &finishes.quarters()[q]).filter(|q| q.character == Character::Vaulted);
            if let Some(quarter) = vaulted
                && plain(x - dx, y - dy)
                && plain(x + dx, y + dy)
                && flat(x, y)
                && !near_stair(x, y)
            {
                out.push(CorridorArch { cell: (x, y), along_x: run.along_x, soffit, pointed: quarter.door == DoorStyle::PointedArch });
            }
            i += rng.int(ARCH_SPACING.0, ARCH_SPACING.1) as usize;
        }
    }
    out
}

/// An arch across a corridor under a flat soffit `soffit` high, in its own space: `x` across
/// the corridor, `y` up, `z` along it, centred on the cell. A pilaster against each wall on a
/// plinth, an impost, and a rib of voussoirs, round or `pointed`, with a keystone springing from
/// the imposts to just under the soffit. `None` where the soffit is too low for a rib the height of a door.
pub fn corridor_arch(soffit: f32, seed: u32, pointed: bool) -> Option<ItemMesh> {
    let mut rng = Mulberry32::new(seed ^ 0xa4c4_b1b5);
    let face = CELL * 0.5 - PILASTER_DEPTH;
    // The rib springs from imposts that stand out from the pilasters' faces, and its stones
    // end in the wall.
    let radius = face - IMPOST_PROJECTION;
    // A pointed rib's arcs have the span for their radius, and its extrados crowns higher.
    let r = if pointed { radius * 2.0 } else { radius };
    let outer_rise = ((r + RIB_THICK).powi(2) - (r - radius).powi(2)).sqrt();
    let inner_rise = (r * r - (r - radius).powi(2)).sqrt();
    let spring = soffit - SOFFIT_GAP - outer_rise;
    let impost_bottom = spring - IMPOST_HEIGHT;
    if spring + inner_rise < DOOR_HEIGHT || impost_bottom < PLINTH_HEIGHT + 4.0 {
        return None;
    }
    let (joint, chamfer) = RIB_STONE;
    let arch = Arch { hw: radius, spring, r };
    let mut stone = voussoirs(&arch, (joint, chamfer), (RIB_THICK, RIB_ALONG), RIB_KEY_RISE, false, &mut rng);

    // Into the wall, so the pilasters bed behind its blocks.
    let wall = CELL * 0.5 + 0.2;
    let side_block = |mesh: &mut MeshBuilder, [near, far]: [f32; 2], [bottom, top]: [f32; 2], along: f32| {
        for side in [-1.0f32, 1.0] {
            let (lo, hi) = if side < 0.0 { (-far, -near) } else { (near, far) };
            mesh.append(&block(lo, hi, bottom, top, along, chamfer));
        }
    };
    side_block(&mut stone, [radius, wall], [0.0, PLINTH_HEIGHT], PILASTER_FOOT_ALONG);
    side_block(&mut stone, [radius, wall], [impost_bottom, spring - joint * 0.5], PILASTER_FOOT_ALONG);
    // The shaft, in drums a little over four units tall.
    let drums = (((impost_bottom - PLINTH_HEIGHT) / 4.4).round() as usize).max(2);
    let drum = (impost_bottom - PLINTH_HEIGHT) / drums as f32;
    for k in 0..drums {
        let bottom = PLINTH_HEIGHT + k as f32 * drum;
        let along = PILASTER_ALONG + (rng.range_f32(0.0, 1.0) - 0.5) * 0.2;
        side_block(&mut stone, [face, wall], [bottom + joint * 0.5, bottom + drum - joint * 0.5], along);
    }
    let mut parts = Parts::new();
    parts.add_look("arch", stone, Look::of(MaterialKind::Stone).tinted(RIB_TINT));
    Some(parts.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::geometry::{CEILING_RELIEF, WALL_RELIEF, stone};

    const STYLES: [DoorStyle; 3] = [DoorStyle::RoundArch, DoorStyle::PointedArch, DoorStyle::RockCut];

    fn made(style: DoorStyle, seed: u32) -> ItemMesh {
        let top = 40.0 + CEILING_RELIEF;
        let courses: Vec<(f32, f32)> = (0..(top / 4.6).ceil() as usize).map(|i| (i as f32 * 4.6, 4.6f32.min(top - i as f32 * 4.6))).collect();
        door_arch(style, &courses, top, CELL * 0.5 + WALL_RELIEF, (0.18, 0.35), seed)
    }

    #[test]
    fn arches_are_deterministic() {
        for style in STYLES {
            for seed in 1..=5 {
                assert_eq!(made(style, seed), made(style, seed));
            }
        }
    }

    #[test]
    fn nothing_stands_in_the_opening() {
        for style in STYLES {
            let arch = Arch::new(style);
            for seed in 1..=20 {
                for part in &made(style, seed).parts {
                    for p in &part.positions {
                        // Under the intrados and between the jambs, bar the keystone's flat seat,
                        // which sits a hair inside the arc.
                        let under = arch.height(p[0], arch.r).map_or(p[1] < arch.spring, |h| p[1] < h - 0.3);
                        assert!(!(p[0].abs() < arch.hw - 0.02 && p[1] > 0.5 && under), "{style:?} seed {seed}: {p:?} in the opening");
                    }
                }
            }
        }
    }

    #[test]
    fn the_clear_height_at_the_middle_is_never_below_the_door() {
        for style in STYLES {
            let a = Arch::new(style);
            assert!((a.hw * 2.0 - DOOR_WIDTH).abs() < 1e-4);
            for seed in 1..=10 {
                // The lowest underside of anything over the middle of the opening.
                let lowest = made(style, seed)
                    .parts
                    .iter()
                    .flat_map(|part| part.positions.iter())
                    .filter(|p| p[0].abs() < 0.2 && p[1] > DOOR_HEIGHT - 3.0)
                    .map(|p| p[1])
                    .fold(f32::MAX, f32::min);
                assert!(lowest >= DOOR_HEIGHT - 1e-3, "{style:?} seed {seed}: clear height {lowest}");
            }
        }
    }

    #[test]
    fn arches_stay_inside_the_cell_and_under_the_top() {
        for style in STYLES {
            for seed in 1..=10 {
                let (lo, hi) = made(style, seed).bounds().unwrap();
                assert!(lo[0] >= -(CELL * 0.5 + WALL_RELIEF) - 1e-3 && hi[0] <= CELL * 0.5 + WALL_RELIEF + 1e-3, "{style:?} {lo:?} {hi:?}");
                assert!(hi[1] <= 40.0 + CEILING_RELIEF + 1e-3, "{style:?} reaches {}", hi[1]);
                assert!(lo[1] >= -0.5);
                assert!(hi[2] < 3.5 && lo[2] > -3.5, "{style:?} depth {lo:?} {hi:?}");
            }
        }
    }

    #[test]
    fn a_halls_doorway_takes_its_style() {
        let level = fb_level::generate(1207, 2);
        let doorway = (0..level.height as i32).flat_map(|y| (0..level.width as i32).map(move |x| (x, y))).find(|&(x, y)| level.cell(x, y) == fb_level::Cell::Door).unwrap();
        let timber = stone::door_frame(&level, doorway, DoorStyle::Timber, 1);
        let round = stone::door_frame(&level, doorway, DoorStyle::RoundArch, 1);
        assert!(timber.parts.iter().any(|p| p.name == "frame"));
        assert!(!round.parts.iter().any(|p| p.name == "frame"));
        assert!(round.parts.iter().any(|p| p.name == "arch"));
    }

    #[test]
    fn corridor_arches_stand_only_in_plain_straight_corridor_cells() {
        let mut total = 0;
        for seed in 1..=30 {
            let level = fb_level::generate(seed, 2);
            let finishes = fb_level::finishes(&level, &fb_level::LevelParams::for_style(level.style));
            let arches = corridor_arches(&level, &finishes);
            assert_eq!(arches, corridor_arches(&level, &finishes), "seed {seed}");
            for a in &arches {
                let (x, y) = a.cell;
                let quarter = &finishes.quarters()[finishes.quarter(x, y).unwrap()];
                assert_eq!(quarter.character, Character::Vaulted, "seed {seed}: arch outside a vaulted quarter at {x},{y}");
                assert_eq!(a.pointed, quarter.door == DoorStyle::PointedArch, "seed {seed}: arch shape differs from its quarter's at {x},{y}");
                assert_eq!(level.cell(x, y), Cell::Floor);
                assert!(level.room_at(x, y).is_none(), "seed {seed}: arch in a room at {x},{y}");
                assert_eq!(straight(&level, x, y), Some(a.along_x), "seed {seed}: arch in a turn or junction at {x},{y}");
                let (dx, dy) = if a.along_x { (1, 0) } else { (0, 1) };
                for k in [-1, 1] {
                    assert_eq!(level.cell(x + dx * k, y + dy * k), Cell::Floor, "seed {seed}: arch beside a door at {x},{y}");
                    assert!(level.room_at(x + dx * k, y + dy * k).is_none());
                }
                assert!(level.stairs.iter().all(|s| (s.x as i32 - x).abs() > 1 || (s.y as i32 - y).abs() > 1), "seed {seed}: arch by a stair");
            }
            total += arches.len();
        }
        assert!(total > 30, "only {total} arches in thirty levels");
    }

    #[test]
    fn corridor_arches_keep_their_rhythm_along_a_run() {
        let mut rhythmic = 0;
        for seed in 1..=30 {
            let level = fb_level::generate(seed, 2);
            let arches = corridor_arches(&level, &fb_level::finishes(&level, &fb_level::LevelParams::for_style(level.style)));
            for run in corridor_runs(&level) {
                let at: Vec<usize> = run.cells.iter().enumerate().filter(|(_, c)| arches.iter().any(|a| a.cell == **c)).map(|(i, _)| i).collect();
                for pair in at.windows(2) {
                    let gap = pair[1] - pair[0];
                    // Ineligible cells are skipped, so a gap is a spacing or a few of them.
                    assert!(gap >= ARCH_SPACING.0 as usize, "seed {seed}: arches {gap} cells apart");
                }
                rhythmic += (at.len() >= 3) as u32;
            }
        }
        assert!(rhythmic > 0, "no corridor shows a rhythm of three arches");
    }

    #[test]
    fn rough_rock_has_no_corridor_arches() {
        for seed in 1..=20 {
            let level = fb_level::generate(seed, 1);
            assert!(corridor_arches(&level, &fb_level::finishes(&level, &fb_level::LevelParams::for_style(level.style))).is_empty());
        }
    }

    #[test]
    fn a_corridor_arch_spans_the_corridor_without_blocking_it() {
        let level = fb_level::generate(1207, 2);
        let soffit = level.ceiling;
        for pointed in [false, true] {
            corridor_arch_spans(soffit, pointed);
            // Other ceilings: where a rib is made at all it clears a door and fits its soffit.
            let made: Vec<f32> = [30.0, 34.0, 38.0, 44.0, 52.0, 64.0].into_iter().filter(|&s| corridor_arch(s, 1, pointed).is_some()).collect();
            assert!(!made.is_empty(), "no ceiling takes a pointed={pointed} rib");
            for s in made {
                corridor_arch_spans(s, pointed);
            }
        }
        assert!(corridor_arch(20.0, 1, false).is_none());
        assert!(corridor_arch(20.0, 1, true).is_none());
    }

    fn corridor_arch_spans(soffit: f32, pointed: bool) {
        let mesh = corridor_arch(soffit, 1, pointed).expect("the halls' soffit is high enough");
        let (lo, hi) = mesh.bounds().unwrap();
        // Inside the cell's width and up to just under the soffit.
        assert!(lo[0] >= -(CELL * 0.5 + 0.3) && hi[0] <= CELL * 0.5 + 0.3, "{lo:?} {hi:?}");
        assert!(hi[1] < soffit && hi[1] > soffit - 1.0, "tops out at {} under a soffit at {soffit}", hi[1]);
        // Nothing across the middle of the cell under the rib's clear height.
        let radius = CELL * 0.5 - PILASTER_DEPTH - IMPOST_PROJECTION;
        let r = if pointed { radius * 2.0 } else { radius };
        let spring = soffit - SOFFIT_GAP - ((r + RIB_THICK).powi(2) - (r - radius).powi(2)).sqrt();
        assert!(spring + (r * r - (r - radius).powi(2)).sqrt() >= DOOR_HEIGHT);
        for part in &mesh.parts {
            for p in &part.positions {
                let clear = if p[1] < spring { f32::MAX } else { (r * r - (p[0].abs() - (radius - r)).powi(2)).max(0.0).sqrt() + spring };
                assert!(p[0].abs() >= radius - 0.02 || p[1] >= clear - 0.3, "{p:?} stands in the way");
            }
        }
        // And it is the same arch from the same seed.
        assert_eq!(Some(mesh), corridor_arch(soffit, 1, pointed));
    }
}
