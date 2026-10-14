//! Shapes creatures share: an organic [`Trunk`] (a closed surface round a centre line, with
//! superelliptic sections that swell and taper along it), [`Trunk::patch`] for markings laid
//! on it, fur [`Trunk::tufts`], and beady eyes.
//!
//! Every bone's mesh is built in the **spine frame**: +Y forward along the body, +Z up (the
//! back), +X to the creature's right. [`spine_to_creature`] turns it into creature space
//! (+Z forward, +Y up, +X to the left). Long parts run along +Y so the Armory's wrapped
//! patterns ([`armory::fabric::Wrap`], which wraps a part that straddles x = 0 round a
//! vertical axis) run round them, rows along the body.

use armory::geom::vec3::{self, Mat3, V3};
use armory::geom::{GridOptions, MeshBuilder, Shading, parametric_surface, spike, uv_sphere};
use procgen_rng::Mulberry32;
use std::f32::consts::{PI, TAU};

/// The rotation from the spine frame (+Y forward, +Z up, +X right) to creature space (+Z
/// forward, +Y up, +X left). A proper rotation, so faces stay outward.
pub fn spine_to_creature() -> Mat3 {
    Mat3::from_cols([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0])
}

/// A value through `keys` (`(t, value)`, `t` ascending) on a cubic Hermite curve with
/// Catmull-Rom tangents (uniform, as Catmull and Rom's 1974 spline): smooth, passing through
/// every key, flat beyond the ends.
pub fn keyed(keys: &[(f32, f32)], t: f32) -> f32 {
    match keys {
        [] => 0.0,
        [(_, v)] => *v,
        _ => {
            let n = keys.len();
            if t <= keys[0].0 {
                return keys[0].1;
            }
            if t >= keys[n - 1].0 {
                return keys[n - 1].1;
            }
            let i = keys.windows(2).position(|w| t < w[1].0).unwrap_or(n - 2);
            let (t0, v0) = keys[i];
            let (t1, v1) = keys[i + 1];
            let h = (t1 - t0).max(1e-6);
            let slope = |k: usize| -> f32 {
                let (a, b) = (keys[k.saturating_sub(1)], keys[(k + 1).min(n - 1)]);
                (b.1 - a.1) / (b.0 - a.0).max(1e-6)
            };
            let (m0, m1) = (slope(i) * h, slope(i + 1) * h);
            let s = (t - t0) / h;
            let (s2, s3) = (s * s, s * s * s);
            (2.0 * s3 - 3.0 * s2 + 1.0) * v0 + (s3 - 2.0 * s2 + s) * m0 + (-2.0 * s3 + 3.0 * s2) * v1 + (s3 - s2) * m1
        }
    }
}

/// `sign(x) * |x|^p`: the superellipse's signed power.
fn spow(x: f32, p: f32) -> f32 {
    x.signum() * x.abs().powf(p)
}

/// An organic body: a closed surface round a centre line running along +Y from `start` for
/// `length`, its section a superellipse `width` across (half width) and `height` up and down
/// (half height), the centre lifted by `lift`, each keyed along `t` in 0..=1 from the back
/// end to the front. Width and height should fall to zero at both ends to close the surface.
#[derive(Clone, Debug, PartialEq)]
pub struct Trunk {
    pub start: f32,
    pub length: f32,
    pub width: Vec<(f32, f32)>,
    pub height: Vec<(f32, f32)>,
    pub lift: Vec<(f32, f32)>,
    /// Superellipse exponent: 2 an ellipse, more for a boxier section.
    pub squareness: f32,
    /// Flattens the underside: below this share of the half height the section is pressed
    /// up to a third (a snake's belly, a rat's chest). 1 leaves it round.
    pub belly: f32,
}

impl Trunk {
    /// The centre line at `t`.
    pub fn centre(&self, t: f32) -> V3 {
        [0.0, self.start + t * self.length, keyed(&self.lift, t)]
    }

    pub fn half_width(&self, t: f32) -> f32 {
        keyed(&self.width, t).max(0.0)
    }

    pub fn half_height(&self, t: f32) -> f32 {
        keyed(&self.height, t).max(0.0)
    }

    /// The surface point at `t` and angle `a` round the centre line (0 the right side, a
    /// quarter turn the back, a half turn the left, three quarters the belly), pushed out
    /// `off` along the section's outward direction.
    pub fn point(&self, t: f32, a: f32, off: f32) -> V3 {
        let (s, c) = a.sin_cos();
        let e = 2.0 / self.squareness.max(0.5);
        let (w, h) = (self.half_width(t), self.half_height(t));
        let mut z = h * spow(s, e);
        if self.belly < 1.0 && z < -self.belly * h {
            let below = -self.belly * h - z;
            z = -self.belly * h - below / 3.0;
        }
        let x = w * spow(c, e);
        let centre = self.centre(t);
        let out = vec3::normalize_or([c, 0.0, s], vec3::Z);
        vec3::add(centre, [x + out[0] * off, 0.0, z + out[2] * off])
    }

    /// The outward surface normal at `t`, `a`, by finite differences on the surface.
    pub fn normal(&self, t: f32, a: f32) -> V3 {
        let (dt, da) = (1e-3, 1e-3);
        let du = vec3::sub(self.point(t, a + da, 0.0), self.point(t, a - da, 0.0));
        let dv = vec3::sub(self.point((t + dt).min(1.0), a, 0.0), self.point((t - dt).max(0.0), a, 0.0));
        // Angle runs right, back, left: du x dv faces out.
        let n = vec3::cross(dv, du);
        vec3::normalize_or(n, vec3::normalize_or([a.cos(), 0.0, a.sin()], vec3::Z))
    }

    /// The closed surface: `around` facets round, `along` rows from end to end.
    pub fn mesh(&self, around: usize, along: usize) -> MeshBuilder {
        let options = GridOptions { wrap_around: true, wrap_along: false, shading: Shading::SMOOTH, row_v: None };
        // u runs from the belly through the left, back and right: du x dv faces out.
        parametric_surface(around, along, &options, |u, v| self.point(v, -u * TAU - PI * 0.5, 0.0))
    }

    /// A marking laid on the surface `off` out from it: over `t` from `t0` to `t1`, and round
    /// the section `half_angle(t)` either side of angle `centre` (radians, as
    /// [`Trunk::point`]). Where the half angle falls to zero the patch comes to a point, so a
    /// half angle shaped like a tent makes a diamond and a constant one a stripe. A half angle
    /// of half a turn or more is a whole band round the body. `None` if the patch is empty.
    ///
    /// With `anchor`, the patch is anchored to the trunk's wrap ([`Trunk::anchor_wrap`]).
    pub fn patch(&self, t0: f32, t1: f32, centre: f32, half_angle: impl Fn(f32) -> f32, off: f32, anchor: bool) -> Option<MeshBuilder> {
        if t1 <= t0 {
            return None;
        }
        let rows = (((t1 - t0) * 40.0).ceil() as usize).clamp(3, 24);
        let ts: Vec<f32> = (0..=rows).map(|k| t0 + (t1 - t0) * k as f32 / rows as f32).collect();
        if ts.iter().all(|&t| half_angle(t) <= 1e-4) {
            return None;
        }
        let ring = ts.iter().any(|&t| half_angle(t) >= PI - 1e-3);
        let cols = if ring { 24 } else { 10 };
        let points: Vec<Vec<V3>> = ts
            .iter()
            .map(|&t| {
                let h = half_angle(t).clamp(0.0, PI);
                (0..=cols).map(|k| self.point(t, centre - h + 2.0 * h * k as f32 / cols as f32, off)).collect()
            })
            .collect();
        // Columns run toward increasing angle (right, back, left), rows forward: du x dv
        // faces in, so flip.
        let options = GridOptions { wrap_around: false, wrap_along: false, shading: Shading::SMOOTH, row_v: None };
        let mut mesh = armory::geom::stitch_grid(&points, &options);
        mesh.flip();
        if anchor {
            self.anchor_wrap(&mut mesh);
        }
        Some(mesh)
    }

    /// Adds three vertices no triangle uses to a part laid on this trunk: the trunk's two
    /// poles and the middle of its belly. A wrapped pattern ([`armory::fabric::Wrap`], which
    /// measures a part from its vertices) is then laid on the part round the same axis and
    /// over the same rows as on the trunk itself, so scales run on across a marking instead of
    /// starting afresh. Each lies on the trunk's own surface, so nothing reaches further.
    pub fn anchor_wrap(&self, mesh: &mut MeshBuilder) {
        mesh.push_vertex(self.centre(0.0), vec3::neg(vec3::Y), [0.0, 0.0]);
        mesh.push_vertex(self.centre(1.0), vec3::Y, [0.0, 0.0]);
        mesh.push_vertex(self.point(0.5, -PI * 0.5, 0.0), vec3::neg(vec3::Z), [0.0, 0.0]);
    }

    /// `count` fur tufts scattered over the surface where `place(t, a)` is true: small cones
    /// standing out of the skin, combed back along the body (-Y) and a little down the flanks,
    /// `size` long. Seeded, so the same body grows the same fur.
    pub fn tufts(&self, count: usize, size: f32, rng: &mut Mulberry32, place: impl Fn(f32, f32) -> bool) -> MeshBuilder {
        let mut fur = MeshBuilder::new();
        let mut tries = 0;
        let mut grown = 0;
        while grown < count && tries < count * 20 {
            tries += 1;
            let t = rng.range_f32(0.0, 1.0);
            let a = rng.range_f32(-PI, PI);
            if !place(t, a) {
                continue;
            }
            grown += 1;
            let n = self.normal(t, a);
            let length = size * rng.range_f32(0.7, 1.25);
            let base = length * rng.range_f32(0.32, 0.45);
            // Combed back and down: mostly along the body, lifted off the skin by the normal.
            let comb = vec3::normalize_or(vec3::add(vec3::scale(n, 0.55), [rng.range_f32(-0.15, 0.15), -1.0, -0.25]), n);
            let mut tuft = spike(base, length, 5, 0.6, Shading::SMOOTH);
            let root = self.point(t, a, -base * 0.6);
            tuft.place(root, comb);
            fur.append(&tuft);
        }
        fur
    }
}

/// A round eye of radius `r` at `at`, looking along `look`: the eyeball, and a glint of light
/// on it toward `light` (up and forward), so an eye reads as wet and alive in torchlight.
pub fn eye(at: V3, r: f32, look: V3, light: V3) -> (MeshBuilder, MeshBuilder) {
    let mut ball = uv_sphere(r, 14, 10);
    ball.translate(at);
    let toward = vec3::normalize_or(vec3::add(vec3::scale(look, 0.7), vec3::scale(light, 0.6)), look);
    let mut glint = uv_sphere(r * 0.28, 8, 6);
    glint.translate(vec3::add(at, vec3::scale(toward, r * 0.86)));
    (ball, glint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyed_passes_through_its_keys_and_holds_beyond_them() {
        let keys = [(0.0, 0.0), (0.3, 1.0), (1.0, 0.5)];
        for &(t, v) in &keys {
            assert!((keyed(&keys, t) - v).abs() < 1e-5);
        }
        assert_eq!(keyed(&keys, -1.0), 0.0);
        assert_eq!(keyed(&keys, 2.0), 0.5);
        let mid = keyed(&keys, 0.15);
        assert!(mid > 0.0 && mid < 1.0);
    }

    #[test]
    fn the_spine_frame_turns_into_creature_space() {
        let m = spine_to_creature();
        assert_eq!(m.transform([0.0, 1.0, 0.0]), [0.0, 0.0, 1.0], "forward");
        assert_eq!(m.transform([0.0, 0.0, 1.0]), [0.0, 1.0, 0.0], "up");
        assert_eq!(m.transform([1.0, 0.0, 0.0]), [-1.0, 0.0, 0.0], "right");
        assert!((m.determinant() - 1.0).abs() < 1e-6);
    }

    fn trunk() -> Trunk {
        Trunk {
            start: -1.0,
            length: 2.0,
            width: vec![(0.0, 0.0), (0.2, 0.5), (0.8, 0.5), (1.0, 0.0)],
            height: vec![(0.0, 0.0), (0.2, 0.4), (0.8, 0.4), (1.0, 0.0)],
            lift: vec![(0.0, 0.0)],
            squareness: 2.0,
            belly: 1.0,
        }
    }

    #[test]
    fn a_trunk_closes_and_faces_out() {
        let t = trunk();
        let part = t.mesh(16, 20).into_part("trunk", armory::geom::OutlineMode::Welded);
        part.validate().unwrap();
        // Normals point away from the centre line.
        for (p, n) in part.positions.iter().zip(&part.normals) {
            let r = [p[0], 0.0, p[2]];
            if vec3::length(r) > 0.2 {
                assert!(vec3::dot(r, *n) > 0.0, "{p:?} {n:?}");
            }
        }
        // Angle a quarter turn is the back, three quarters the belly.
        assert!(t.point(0.5, PI * 0.5, 0.0)[2] > 0.39);
        assert!(t.point(0.5, -PI * 0.5, 0.0)[2] < -0.39);
        assert!(t.normal(0.5, PI * 0.5)[2] > 0.9);
    }

    #[test]
    fn patches_lie_outside_the_surface() {
        let t = trunk();
        let diamond = t.patch(0.3, 0.7, PI * 0.5, |s| 0.8 * (1.0 - (s - 0.5).abs() / 0.2).max(0.0), 0.01, false).unwrap();
        let part = diamond.into_part("mark", armory::geom::OutlineMode::Shading);
        part.validate().unwrap();
        assert!(part.normals.iter().all(|n| n[2] > 0.0), "a mark on the back faces up");
        assert!(t.patch(0.3, 0.7, 0.0, |_| 0.0, 0.01, false).is_none());
    }
}
