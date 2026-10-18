//! The webs of a spider's nest drawn as plain mesh data: a pure function of the web spot
//! ([`web_mesh`]), with no Bevy, and how they hold a walker back ([`web_pace`]).
//!
//! Built in the spot's own frame so the game only has to stand it up: the origin is the
//! spot's point on the floor, +Y is up and the spot's `yaw` is the turn about +Y that brings
//! the frame's -Z onto the spot's facing (a corner's diagonal into the room, the way a strand
//! runs).
//!
//! - A corner sheet is a fan of radial threads from a hub low in the corner to the diagonal
//!   top edge between the two walls, crossed by capture thread that zigzags out from the hub
//!   in a spiral, each pass sagging a little between the radials.
//! - A ceiling strand runs from the vault down toward a wall with a sag, with a few fine
//!   threads fanned from where it is anchored.
//! - A floor strand is a bundle of low threads strung between two points on the floor, each
//!   sagging between them, with a cross thread or two tying them together.
//!
//! Threads are thin five-sided tubes (`armory::geom::tube`) in cord, so they carry proper
//! normals, and the whole web is one part. Each thread's wobble comes from a seed taken from
//! the spot's position, so the same spot always gives the same web.
//!
//! It lives here, beside [`WebSpot`], rather than in `armory`: a web is a piece of a level,
//! not an item, and `fb_level` already builds on the Armory's sweeps.

use armory::geom::{MeshBuilder, OutlineMode, SweepOptions, V3, tube};
use armory::{ItemMesh, MaterialKind, Rgb};
use fb_creatures::Species;
use procgen_rng::Mulberry32;

use crate::wild::{WebKind, WebSpot};

/// Radius of a thread, world units: 3 mm.
pub const THREAD: f32 = 0.03;
/// Half the width of a floor strand's bundle of threads, world units.
pub const FLOOR_SPREAD: f32 = 2.0;
/// The colour of the thread: pale, a little grey, so it reads against stone in the dark.
pub const WEB_TINT: Rgb = Rgb(0.86, 0.87, 0.84);
/// How much of its pace a walker keeps while its body is in a floor web. Our own: the SRD's
/// web spell holds creatures fast (p. 218), and a nest's loose floor strands only hold a
/// walker back, to about two thirds of a stride.
pub const WEB_PACE: f32 = 0.65;

/// How high a corner sheet's hub is, as a fraction of its top: low in the corner.
const HUB: f32 = 0.06;
const SQRT_HALF: f32 = std::f32::consts::FRAC_1_SQRT_2;

fn seed_of(spot: &WebSpot) -> u32 {
    let kind = match spot.kind {
        WebKind::Corner => 0x57ED_0001,
        WebKind::Ceiling => 0x57ED_0002,
        WebKind::Floor => 0x57ED_0003,
    };
    spot.x.to_bits() ^ spot.y.to_bits().rotate_left(13) ^ spot.size.to_bits().rotate_left(7) ^ kind
}

fn lerp(a: V3, b: V3, t: f32) -> V3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// `from` to `to` in `steps` pieces, hanging `sag` below the straight line at the middle.
fn strand(from: V3, to: V3, sag: f32, steps: usize) -> Vec<V3> {
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let mut p = lerp(from, to, t);
            p[1] -= sag * 4.0 * t * (1.0 - t);
            p
        })
        .collect()
}

fn thread(mesh: &mut MeshBuilder, path: &[V3]) {
    mesh.append(&tube(path, 5, &SweepOptions::default(), |_| THREAD));
}

/// The mesh of the web at `spot`, in the spot's frame. `roof` is the height of the ceiling
/// over the spot, which a ceiling strand hangs from.
pub fn web_mesh(spot: &WebSpot, roof: f32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed_of(spot));
    let mut mesh = MeshBuilder::new();
    match spot.kind {
        WebKind::Corner => corner_sheet(&mut mesh, spot, &mut rng),
        WebKind::Ceiling => ceiling_strands(&mut mesh, spot, roof, &mut rng),
        WebKind::Floor => floor_strands(&mut mesh, spot, &mut rng),
    }
    let part = mesh.into_part("web", OutlineMode::Shading).with_material(MaterialKind::Cord).with_tint(WEB_TINT);
    ItemMesh { parts: vec![part] }
}

fn corner_sheet(mesh: &mut MeshBuilder, spot: &WebSpot, rng: &mut Mulberry32) {
    const RADIALS: usize = 7;
    const PASSES: usize = 6;
    let (top, leg) = (spot.height, spot.size);
    let hub = [0.0, top * HUB, 0.0];
    // The two walls run at 45 degrees either side of the diagonal.
    let (a, b) = ([leg * SQRT_HALF, top, -leg * SQRT_HALF], [-leg * SQRT_HALF, top, -leg * SQRT_HALF]);
    let ends: Vec<V3> = (0..RADIALS)
        .map(|i| {
            let mut e = lerp(a, b, i as f32 / (RADIALS - 1) as f32);
            if i != 0 && i != RADIALS - 1 {
                // The sheet is not drawn tight: its middle threads fall a little short.
                e = lerp(hub, e, rng.range_f32(0.9, 1.0));
            }
            e
        })
        .collect();
    for end in &ends {
        thread(mesh, &strand(hub, *end, 0.0, 6));
    }
    // The frame thread along the top, from wall to wall.
    thread(mesh, &strand(a, b, leg * 0.06, 10));
    // Capture thread: out from the hub in passes, each one back across the fan.
    let mut last: Option<V3> = None;
    for pass in 0..PASSES {
        let f = 0.22 + 0.72 * pass as f32 / (PASSES - 1) as f32;
        let order: Vec<usize> = if pass % 2 == 0 { (0..RADIALS).collect() } else { (0..RADIALS).rev().collect() };
        let points: Vec<V3> = order.iter().map(|&i| lerp(hub, ends[i], f)).collect();
        if let Some(from) = last {
            thread(mesh, &strand(from, points[0], 0.0, 2));
        }
        for pair in points.windows(2) {
            let gap = ((pair[1][0] - pair[0][0]).powi(2) + (pair[1][2] - pair[0][2]).powi(2)).sqrt();
            thread(mesh, &strand(pair[0], pair[1], gap * rng.range_f32(0.08, 0.16), 5));
        }
        last = points.last().copied();
    }
}

fn ceiling_strands(mesh: &mut MeshBuilder, spot: &WebSpot, roof: f32, rng: &mut Mulberry32) {
    let half = spot.size * 0.5;
    // High end anchored in the vault, low end hanging free toward the wall.
    let anchor = [0.0, roof, -half];
    let low_y = (2.0 * spot.height - roof).clamp(roof * 0.45, roof - 1.0);
    let end = [0.0, low_y, half];
    thread(mesh, &strand(anchor, end, spot.size * 0.05, 14));
    // Fine threads fanned from the anchor, shorter than the main one.
    for _ in 0..rng.int(2, 3) {
        let reach = rng.range_f32(0.4, 0.8);
        let spread = rng.range_f32(-0.25, 0.25) * spot.size;
        let tip = lerp(anchor, [spread, low_y + rng.range_f32(0.0, 2.0), half], reach);
        thread(mesh, &strand(anchor, tip, spot.size * 0.03, 8));
    }
}

fn floor_strands(mesh: &mut MeshBuilder, spot: &WebSpot, rng: &mut Mulberry32) {
    let half = spot.size * 0.5;
    const STAKE: f32 = 1.1;
    let count = rng.int(4, 6) as usize;
    let mut lines: Vec<(V3, V3)> = Vec::new();
    for k in 0..count {
        let across = -FLOOR_SPREAD + 2.0 * FLOOR_SPREAD * (k as f32 + rng.range_f32(0.2, 0.8)) / count as f32;
        let (a, b) = (across + rng.range_f32(-0.5, 0.5), across + rng.range_f32(-0.5, 0.5));
        let (za, zb) = (-half * rng.range_f32(0.85, 1.0), half * rng.range_f32(0.85, 1.0));
        let (ya, yb) = (STAKE * rng.range_f32(0.7, 1.0), STAKE * rng.range_f32(0.7, 1.0));
        lines.push(([a, ya, za], [b, yb, zb]));
    }
    for &(from, to) in &lines {
        thread(mesh, &strand(from, to, 0.7, 12));
    }
    // Cross threads tie neighbouring strands together part of the way along.
    for pair in lines.windows(2) {
        let t = rng.range_f32(0.25, 0.75);
        let (p, q) = (lerp(pair[0].0, pair[0].1, t), lerp(pair[1].0, pair[1].1, t));
        let drop = 4.0 * t * (1.0 - t) * 0.7;
        let (p, q) = ([p[0], p[1] - drop, p[2]], [q[0], q[1] - drop, q[2]]);
        thread(mesh, &strand(p, q, 0.1, 3));
    }
}

/// Whether the walker standing at world `(x, z)` with a body of `radius` is in `spot`'s
/// strands. Only floor webs hold anyone: a sheet in a corner or a strand overhead is not
/// underfoot.
pub fn web_underfoot(spot: &WebSpot, x: f32, z: f32, radius: f32) -> bool {
    if spot.kind != WebKind::Floor {
        return false;
    }
    let [cx, _, cz] = crate::Level::world(spot.x, spot.y);
    // Into the web's own frame: the strand's direction is -Z turned by `yaw` about +Y.
    let (dx, dz) = (x - cx, z - cz);
    let (sin, cos) = spot.yaw.sin_cos();
    let along = -(dx * sin + dz * cos);
    let across = dx * cos - dz * sin;
    along.abs() <= spot.size * 0.5 + radius && across.abs() <= FLOOR_SPREAD + radius
}

/// The multiple of its pace a walker keeps at world `(x, z)`: `pace` while in any floor web,
/// otherwise 1. A giant spider crosses them at full speed (Web Walker, SRD 5.1 p. 379).
pub fn web_pace(webs: &[WebSpot], x: f32, z: f32, radius: f32, walker: Option<Species>, pace: f32) -> f32 {
    if walker.is_some_and(Species::is_spider) {
        return 1.0;
    }
    if webs.iter().any(|w| web_underfoot(w, x, z, radius)) { pace } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CELL;

    const ALL: [WebKind; 3] = [WebKind::Corner, WebKind::Ceiling, WebKind::Floor];

    fn spot(kind: WebKind) -> WebSpot {
        let (height, size) = match kind {
            WebKind::Corner => (18.0, 9.0),
            WebKind::Ceiling => (24.0, 12.0),
            WebKind::Floor => (0.0, 10.0),
        };
        WebSpot { x: 12.3, y: 7.6, height, yaw: 0.7, size, kind }
    }

    #[test]
    fn webs_are_deterministic() {
        for kind in ALL {
            assert_eq!(web_mesh(&spot(kind), 30.0), web_mesh(&spot(kind), 30.0), "{kind:?}");
        }
        let moved = WebSpot { x: 12.4, ..spot(WebKind::Floor) };
        assert_ne!(web_mesh(&moved, 30.0), web_mesh(&spot(WebKind::Floor), 30.0));
    }

    #[test]
    fn every_part_validates() {
        for kind in ALL {
            let mesh = web_mesh(&spot(kind), 30.0);
            assert!(!mesh.parts.is_empty());
            for part in &mesh.parts {
                assert!(!part.positions.is_empty(), "{kind:?}");
                part.validate().unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            }
        }
    }

    #[test]
    fn each_kind_hangs_where_it_should() {
        let bounds = |k| web_mesh(&spot(k), 30.0).bounds().unwrap();
        let (lo, hi) = bounds(WebKind::Corner);
        assert!(lo[1] < 2.0 && (hi[1] - 18.0).abs() < 0.5, "a corner sheet runs from the floor to its top: {lo:?} {hi:?}");
        let (lo, hi) = bounds(WebKind::Ceiling);
        assert!(lo[1] > 10.0 && (hi[1] - 30.0).abs() < 0.5, "a ceiling strand hangs from the roof: {lo:?} {hi:?}");
        let (lo, hi) = bounds(WebKind::Floor);
        assert!(lo[1] > 0.0 && hi[1] < 2.0, "a floor strand lies low: {lo:?} {hi:?}");
    }

    #[test]
    fn only_floor_webs_are_underfoot() {
        for kind in ALL {
            let s = spot(kind);
            let [x, _, z] = crate::Level::world(s.x, s.y);
            assert_eq!(web_underfoot(&s, x, z, 3.5), kind == WebKind::Floor, "{kind:?}");
        }
        let s = spot(WebKind::Floor);
        let [x, _, z] = crate::Level::world(s.x, s.y);
        assert!(!web_underfoot(&s, x + CELL, z + CELL, 3.5));
    }

    #[test]
    fn a_spider_is_not_slowed_and_others_are() {
        let s = spot(WebKind::Floor);
        let [x, _, z] = crate::Level::world(s.x, s.y);
        assert_eq!(web_pace(&[s], x, z, 3.5, Some(Species::GiantSpider), WEB_PACE), 1.0);
        assert_eq!(web_pace(&[s], x, z, 3.5, Some(Species::Rat), WEB_PACE), WEB_PACE);
        assert_eq!(web_pace(&[s], x, z, 3.5, None, WEB_PACE), WEB_PACE);
        assert_eq!(web_pace(&[s], x + CELL, z, 3.5, None, WEB_PACE), 1.0);
    }
}
