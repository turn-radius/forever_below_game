//! Cut stones and their settings: head gems, pommel stones, guard stones, rings, amulets
//! and charms all set their stones with these.
//!
//! Every stone is built about the origin with its girdle (widest ring) at y = 0 and its
//! table or dome facing +Y, so a caller seats it by placing the girdle. Faceted cuts are
//! flat shaded, one normal per facet; cabochons and orbs are smooth.
//!
//! The prototype's `gemGeo` stood in a three.js solid for each cut (an icosahedron for a
//! rose, an octahedron for a shard). These are cut properly instead: a brilliant has a
//! table, star and kite crown facets, a girdle and a pavilion to a culet; a rose has a flat
//! base and a crown of triangles; a shard is a terminated hexagonal crystal.

use crate::gem::{GemCut, Gemstone};
use crate::geom::{
    CatmullRom, LatheOptions, MeshBuilder, OutlineMode, Part, Shading, SweepOptions, V3, icosphere, lathe, spline_profile, tube,
    uv_sphere, vec3,
};
use crate::material::MaterialKind;

/// A horizontal ring of `n` points at height `y`, the first at angle `phase` (radians, from
/// +X toward -Z, as [`crate::geom::lathe`] turns).
fn ring(radius: f32, y: f32, n: usize, phase: f32) -> Vec<V3> {
    (0..n)
        .map(|k| {
            let a = phase + std::f32::consts::TAU * k as f32 / n as f32;
            [radius * a.cos(), y, -radius * a.sin()]
        })
        .collect()
}

/// Builds a convex faceted stone from flat triangles, each wound to face away from `center`.
struct Facets {
    mesh: MeshBuilder,
    center: V3,
    size: f32,
}

impl Facets {
    fn new(center_y: f32, size: f32) -> Self {
        Facets { mesh: MeshBuilder::new(), center: [0.0, center_y, 0.0], size: size.max(1e-6) }
    }

    fn uv(&self, p: V3) -> [f32; 2] {
        [0.5 + 0.5 * p[0] / self.size, 0.5 + 0.5 * p[2] / self.size]
    }

    fn tri(&mut self, a: V3, b: V3, c: V3) {
        let mid = vec3::centroid(&[a, b, c]);
        let facing = vec3::sub(mid, self.center);
        let uvs = [self.uv(a), self.uv(b), self.uv(c)];
        self.mesh.push_flat_triangle([a, b, c], uvs, facing);
    }

    /// Triangles from every point of `ring` to `apex`.
    fn fan(&mut self, ring: &[V3], apex: V3) {
        for k in 0..ring.len() {
            self.tri(ring[k], ring[(k + 1) % ring.len()], apex);
        }
    }

    /// Joins two rings of equal count. When `staggered`, ring `b` sits half a step round from
    /// `a` and the band is a zigzag of triangles (star and kite facets); otherwise it is a
    /// row of flat quads.
    fn band(&mut self, a: &[V3], b: &[V3], staggered: bool) {
        let n = a.len();
        for k in 0..n {
            let (a0, a1, b0, b1) = (a[k], a[(k + 1) % n], b[k], b[(k + 1) % n]);
            if staggered {
                self.tri(a0, a1, b0);
                self.tri(b0, a1, b1);
            } else {
                let mid = vec3::centroid(&[a0, a1, b1, b0]);
                let facing = vec3::sub(mid, self.center);
                let uvs = [self.uv(a0), self.uv(a1), self.uv(b1), self.uv(b0)];
                self.mesh.push_flat_quad([a0, a1, b1, b0], uvs, facing);
            }
        }
    }

    /// A flat polygon closing `ring`, facing `facing`.
    fn cap(&mut self, ring: &[V3], facing: V3) {
        self.mesh.push_fan_cap(ring, facing);
    }
}

/// A cut stone of girdle radius `size` (the prototype's `gemGeo(cut, s)`), girdle at y = 0,
/// table up.
pub fn cut_stone(cut: GemCut, size: f32) -> MeshBuilder {
    use std::f32::consts::PI;
    let s = size.max(1e-4);
    match cut {
        GemCut::Brilliant => {
            let n = 8;
            let half = PI / n as f32;
            let mut f = Facets::new(-0.1 * s, s);
            let pavilion = ring(0.52 * s, -0.32 * s, n, half);
            let girdle_low = ring(s, -0.03 * s, n, 0.0);
            let girdle_high = ring(s, 0.03 * s, n, 0.0);
            let crown = ring(0.8 * s, 0.2 * s, n, half);
            let table = ring(0.55 * s, 0.32 * s, n, 0.0);
            f.fan(&pavilion, [0.0, -0.64 * s, 0.0]);
            f.band(&pavilion, &girdle_low, true);
            f.band(&girdle_low, &girdle_high, false);
            f.band(&girdle_high, &crown, true);
            f.band(&crown, &table, true);
            f.cap(&table, vec3::Y);
            f.mesh
        }
        GemCut::Rose => {
            let n = 8;
            let mut f = Facets::new(0.15 * s, s);
            let base = ring(s, 0.0, n, 0.0);
            let crown = ring(0.62 * s, 0.3 * s, n, PI / n as f32);
            f.cap(&base, vec3::neg(vec3::Y));
            f.band(&base, &crown, true);
            f.fan(&crown, [0.0, 0.48 * s, 0.0]);
            f.mesh
        }
        GemCut::Octahedron => {
            let mut f = Facets::new(0.0, s);
            let girdle = ring(s, 0.0, 4, 0.0);
            f.fan(&girdle, [0.0, s, 0.0]);
            f.fan(&girdle, [0.0, -s, 0.0]);
            f.mesh
        }
        GemCut::Shard => {
            // A terminated hexagonal crystal, a little irregular so it reads as raw.
            let n = 6;
            let mut f = Facets::new(0.1 * s, s);
            let low = ring(0.5 * s, -0.8 * s, n, 0.0);
            let high: Vec<V3> = ring(0.56 * s, 0.85 * s, n, 0.0)
                .into_iter()
                .enumerate()
                .map(|(k, p)| [p[0], p[1] + [0.0, 0.12, 0.05, -0.06, 0.1, 0.02][k] * s, p[2]])
                .collect();
            f.fan(&low, [0.04 * s, -1.25 * s, 0.0]);
            f.band(&low, &high, false);
            f.fan(&high, [-0.05 * s, 1.75 * s, 0.03 * s]);
            f.mesh
        }
        GemCut::Cabochon => {
            let profile = spline_profile(&[[s, 0.0], [0.94 * s, 0.24 * s], [0.6 * s, 0.56 * s], [0.0, 0.66 * s]], 14);
            lathe(&profile, &LatheOptions { facets: 28, shading: Shading::SMOOTH, cap_start: true, cap_end: false, phase: 0.0 })
        }
        GemCut::Orb => icosphere(s, 2),
    }
}

/// How far a stone of `size` reaches below and above its girdle: `(down, up)`, both positive.
pub fn stone_reach(cut: GemCut, size: f32) -> (f32, f32) {
    let (down, up) = match cut {
        GemCut::Brilliant => (0.64, 0.32),
        GemCut::Rose => (0.0, 0.48),
        GemCut::Octahedron => (1.0, 1.0),
        GemCut::Shard => (1.25, 1.87),
        GemCut::Cabochon => (0.0, 0.66),
        GemCut::Orb => (1.0, 1.0),
    };
    (down * size, up * size)
}

/// `cut`, or the nearest cut `stone` takes: opaque stones (turquoise, moonstone) fall back to
/// a cabochon, and clear ones to a brilliant.
pub fn cut_for(stone: Gemstone, cut: GemCut) -> GemCut {
    if stone.suits(cut) {
        cut
    } else if stone.is_opaque() {
        GemCut::Cabochon
    } else {
        GemCut::Brilliant
    }
}

/// A bezel for a stone of girdle radius `radius`: a shallow cup whose rim turns in over the
/// girdle, with the girdle at y = 0 so it is placed with the stone it holds (the
/// prototype's mace setting). Its foot reaches `0.35 * radius` below the girdle.
pub fn bezel(radius: f32, sides: usize, shading: Shading) -> MeshBuilder {
    let r = radius;
    let control = [[0.0, -0.35 * r], [0.55 * r, -0.3 * r], [0.98 * r, 0.05 * r], [1.04 * r, 0.2 * r], [0.88 * r, 0.25 * r]];
    lathe(&spline_profile(&control, 16), &LatheOptions { facets: sides, shading, cap_start: false, cap_end: true, phase: 0.0 })
}

/// A collet: a raised cup whose lip turns in over a stone of girdle radius `radius`, its foot
/// at y = 0 and its lip `height + 0.16 * radius` up (the prototype's `bezelCup`, for rings,
/// pendants and accent stones). Seat the stone's girdle at [`collet_seat`].
pub fn collet(radius: f32, height: f32, sides: usize) -> MeshBuilder {
    let (r, h) = (radius, height.max(radius * 0.15));
    let control = [[r * 0.5, 0.0], [r * 1.02, h * 0.3], [r * 1.1, h], [r * 1.14, h + r * 0.1], [r * 0.98, h + r * 0.16]];
    lathe(&spline_profile(&control, 14), &LatheOptions { facets: sides, shading: Shading::SMOOTH, cap_start: true, cap_end: true, phase: 0.0 })
}

/// The girdle height of a stone in a [`collet`] of the same `radius` and `height`: just
/// under the turned-in lip.
pub fn collet_seat(radius: f32, height: f32) -> f32 {
    height.max(radius * 0.15) + radius * 0.06
}

/// A claw setting for a stone of girdle radius `radius`: a basket rising `height` from
/// y = 0, and `count` claws that climb past the girdle and curl over it, each tipped with a
/// bead. `bend(k)` pushes claw `k`'s tip out (bent by damage) as a fraction of the radius.
/// Returns `(basket, claws)` so the claws can go without an outline. Seat the girdle at
/// [`claw_seat`].
pub fn claw_setting(radius: f32, height: f32, count: u32, bend: impl Fn(u32) -> f32) -> (MeshBuilder, MeshBuilder) {
    let (r, h) = (radius, height.max(radius * 0.3));
    let basket = lathe(
        &spline_profile(&[[r * 0.22, 0.0], [r * 0.32, h * 0.25], [r * 0.72, h * 0.85], [r * 0.78, h]], 10),
        &LatheOptions { facets: 20, shading: Shading::SMOOTH, ..Default::default() },
    );
    let mut claws = MeshBuilder::new();
    let count = count.max(2);
    let tip = uv_sphere(r * 0.085, 8, 6);
    for k in 0..count {
        let a = std::f32::consts::TAU * k as f32 / count as f32 + std::f32::consts::PI / count as f32;
        let dir = [a.cos(), 0.0, -a.sin()];
        let at = |rr: f32, y: f32| vec3::add(vec3::scale(dir, rr), [0.0, y, 0.0]);
        let control = vec![at(r * 0.3, 0.0), at(r * 0.85, h * 0.6), at(r * 1.04, h + r * 0.05), at(r * (0.86 + bend(k)), h + r * 0.34)];
        let path = CatmullRom::centripetal(control).sample(8);
        claws.append(&tube(&path, 8, &SweepOptions::default(), |t| r * (0.11 - 0.04 * t)));
        let mut bead = tip.clone();
        bead.translate(path[path.len() - 1]);
        claws.append(&bead);
    }
    (basket, claws)
}

/// The girdle height of a stone in a [`claw_setting`] of the same `radius` and `height`.
pub fn claw_seat(radius: f32, height: f32) -> f32 {
    height.max(radius * 0.3) + radius * 0.05
}

/// Finishes stone geometry into a part: gem material, the stone's own colour, and a glow.
pub fn stone_part(name: &'static str, mesh: MeshBuilder, stone: Gemstone, glow: f32) -> Part {
    mesh.into_part(name, OutlineMode::Welded).with_material(MaterialKind::Gem).with_tint(stone.rgb()).with_glow(glow)
}
