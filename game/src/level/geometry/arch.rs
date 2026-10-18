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
use fb_level::{CELL, DoorStyle};
use procgen_rng::Mulberry32;

use super::{DOOR_HEIGHT, DOOR_WIDTH, FLOOR_RELIEF, INFILL_DEPTH};

/// How thick the ring of voussoirs is, measured radially.
const RING: f32 = 3.0;
/// How far the ring stands proud of its wall, as a whole depth through the wall.
const RING_DEPTH: f32 = 4.6;
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

/// The two arcs that make an opening: `hw` either side of the centre at the spring, each of
/// radius `r` about a centre `r - hw` across the axis. A round arch has `r == hw`, so both
/// arcs share a centre.
#[derive(Clone, Copy, Debug)]
struct Arch {
    hw: f32,
    spring: f32,
    r: f32,
}

impl Arch {
    fn new(style: DoorStyle) -> Self {
        let hw = DOOR_WIDTH * 0.5;
        match style {
            DoorStyle::PointedArch => {
                // An equilateral-ish point: the arcs' radius is the span, the crown about 26.
                let r = DOOR_WIDTH;
                let rise = (r * r - (r - hw) * (r - hw)).sqrt();
                Arch { hw, spring: 26.0 - rise, r }
            }
            // Clear height at the middle is a little over the door's.
            _ => Arch { hw, spring: DOOR_HEIGHT - hw + 0.5, r: hw },
        }
    }

    /// The angle at which the arcs meet.
    fn theta_max(&self) -> f32 {
        ((self.r - self.hw) / self.r).acos()
    }

    /// The point at angle `theta` on the arc of `side` (+1 right, -1 left), at `radius`.
    fn point(&self, side: f32, theta: f32, radius: f32) -> P2 {
        let c = self.hw - self.r;
        [side * (c + radius * theta.cos()), self.spring + radius * theta.sin()]
    }

    /// The height of the circle of `radius` over `x`, or `None` past its spring.
    fn height(&self, x: f32, radius: f32) -> Option<f32> {
        let dx = x.abs() - (self.hw - self.r);
        (radius * radius - dx * dx > 0.0 && x.abs() < self.hw + (radius - self.r)).then(|| self.spring + (radius * radius - dx * dx).sqrt())
    }
}

/// A convex polygon, however it is wound, as a block `depth` through the wall centred on
/// `z = 0`, its faces chamfered by `ch` all round.
fn prism(poly: &[P2], depth: f32, ch: f32) -> MeshBuilder {
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

    let mut ring = MeshBuilder::new();
    // The thinnest a stone is, which the masonry is cut to.
    let thin = if rough { ROCK_RING.0 } else { RING };
    let thickness = |rng: &mut Mulberry32| {
        if rough {
            rng.range_f32(ROCK_RING.0, ROCK_RING.1)
        } else {
            RING
        }
    };

    // The voussoirs of each side, from the spring up to the keystone's seat.
    let r_mid = arch.r + RING * 0.5;
    let theta_max = arch.theta_max();
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
            let t = thickness(&mut rng);
            // The rock-cut rim wanders outward only, so the opening is never narrower.
            let inner = if rough { rng.range_f32(0.0, 0.3) } else { 0.0 };
            let poly = [arch.point(side, a, arch.r + inner), arch.point(side, b, arch.r + inner), arch.point(side, b, arch.r + t), arch.point(side, a, arch.r + t)];
            let depth = RING_DEPTH + if rough { rng.range_f32(0.0, 0.9) } else { (rng.range_f32(0.0, 1.0) - 0.5) * 0.25 };
            let mut stone = prism(&poly, depth, ring_chamfer);
            stone.translate([0.0, 0.0, (rng.range_f32(0.0, 1.0) - 0.5) * 0.2]);
            ring.append(&stone);
        }
    }
    if !rough {
        // The keystone: wedge-sided over the crown, rising a little above the ring.
        let key_in = |side: f32| arch.point(side, seat, arch.r);
        let key_out = |side: f32| arch.point(side, seat, arch.r + RING);
        let crown = arch.height(0.0, arch.r + RING).unwrap_or(spring + arch.r + RING) + 1.0;
        let poly = [key_in(-1.0), key_in(1.0), key_out(1.0), [1.2, crown], [-1.2, crown], key_out(-1.0)];
        ring.append(&prism(&poly, RING_DEPTH + KEY_PROUD, ring_chamfer));
    }

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
    let steps = 14;
    let mut outline: Vec<P2> = vec![[-inner, top], [-inner, spring], [-hw, spring]];
    let intrados = |side: f32, theta: f32| arch.point(side, theta, arch.r);
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
}
