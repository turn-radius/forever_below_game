//! Straps and their hardware: flat straps laid along a surface, buckles, D-rings and domed
//! rivets. Bags, chests (iron banding is a strap too), belts, armour and boots build theirs
//! here.
//!
//! A strap is described by a path and, at each point, the direction its face looks (out of
//! the surface it lies on). The strap's back sits on the path and its face stands
//! `thickness` proud of it, so a path sampled on a surface gives a strap that lies on it.
//!
//! Hardware is built in a local space: the strap runs along +Y, its face looks along +Z and
//! its width is along X. [`MeshBuilder::place_basis`] sets it on a strap.

use crate::geom::{CatmullRom, Frame, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, lathe, spline_profile, sweep_frames, tube};
use crate::geom::vec3;

/// Frames along `path` with each normal along the matching `faces` direction (made
/// perpendicular to the path), so a flat section keeps its face where it is told.
pub fn strap_frames(path: &[V3], faces: &[V3], closed: bool) -> Vec<Frame> {
    let n = path.len();
    (0..n)
        .map(|i| {
            let prev = if i > 0 { path[i - 1] } else if closed { path[n - 1] } else { path[i] };
            let next = if i + 1 < n { path[i + 1] } else if closed { path[0] } else { path[i] };
            let tangent = vec3::normalize_or(vec3::sub(next, prev), vec3::Y);
            let face = faces.get(i).copied().unwrap_or(vec3::Z);
            let normal = vec3::try_normalize(vec3::sub(face, vec3::scale(tangent, vec3::dot(face, tangent))))
                .unwrap_or_else(|| vec3::any_perpendicular(tangent));
            Frame { origin: path[i], tangent, normal, binormal: vec3::cross(tangent, normal) }
        })
        .collect()
}

/// A flat strap of `width` and `thickness` along `path`, its back on the path and its face
/// looking along `faces` (one direction per path point). Hard edges round the section,
/// smooth along it; a `closed` strap is a band with no ends (a hoop, a strap round a roll).
pub fn strap(path: &[V3], faces: &[V3], width: f32, thickness: f32, closed: bool) -> MeshBuilder {
    if path.len() < 2 {
        return MeshBuilder::new();
    }
    let (w, t) = (width * 0.5, thickness.max(1e-4));
    let section = [[0.0, -w], [t, -w], [t, w], [0.0, w]];
    let options = SweepOptions { shading: Shading::FACETED, cap_start: !closed, cap_end: !closed, closed, up: None, twist: 0.0 };
    sweep_frames(&strap_frames(path, faces, closed), &section, &options, |_| 1.0)
}

/// [`strap`] along a surface given as `at(t) -> (point, outward normal)` for `t` in `0..=1`,
/// sampled at `samples + 1` points.
pub fn strap_along(samples: usize, width: f32, thickness: f32, at: impl Fn(f32) -> (V3, V3)) -> MeshBuilder {
    let samples = samples.max(1);
    let (path, faces): (Vec<V3>, Vec<V3>) = (0..=samples).map(|i| at(i as f32 / samples as f32)).unzip();
    strap(&path, &faces, width, thickness, false)
}

/// A rectangular buckle for a strap of `width`, bent from round wire of radius `wire`: the
/// frame and its tongue. Local space: the strap runs along +Y through the frame, the
/// tongue points up +Y lying over it, and the buckle sits on z = 0 facing +Z.
pub fn buckle(width: f32, wire: f32) -> MeshBuilder {
    let (hx, hy) = (width * 0.5 + wire * 1.6, width * 0.32 + wire * 1.2);
    // A superellipse of exponent 6: a rectangle with rounded corners.
    let frame: Vec<V3> = (0..48)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / 48.0;
            let (s, c) = a.sin_cos();
            let e = 1.0 / 3.0;
            [hx * c.signum() * c.abs().powf(e), hy * s.signum() * s.abs().powf(e), wire]
        })
        .collect();
    let mut mesh = tube(&frame, 8, &SweepOptions { up: Some(vec3::Z), ..SweepOptions::closed_loop() }, |_| wire);
    let tongue = [[0.0, -hy, wire], [0.0, 0.0, wire * 2.1], [0.0, hy + wire * 0.6, wire * 2.0]];
    let path = CatmullRom::centripetal(tongue.to_vec()).sample(8);
    mesh.append(&tube(&path, 6, &SweepOptions::default(), |t| wire * 0.75 * if t > 0.85 { 1.0 - (t - 0.85) * 4.0 } else { 1.0 }));
    mesh
}

/// A D-ring for a strap of `width`: a straight bar along X at y = 0 (sewn into a strap loop)
/// and a bow rising to `0.65 * width` in +Y, round wire of radius `wire`, lying on z = 0.
pub fn d_ring(width: f32, wire: f32) -> MeshBuilder {
    let (hw, h) = (width * 0.5 + wire, width * 0.65 + wire);
    let control = vec![
        [-hw, 0.0, 0.0], [0.0, -wire * 0.3, 0.0], [hw, 0.0, 0.0], [hw * 0.95, h * 0.55, 0.0], [hw * 0.55, h * 0.92, 0.0],
        [0.0, h, 0.0], [-hw * 0.55, h * 0.92, 0.0], [-hw * 0.95, h * 0.55, 0.0],
    ];
    let path = CatmullRom::centripetal(control).with_closed(true).sample_evenly(48);
    tube(&path, 8, &SweepOptions { up: Some(vec3::Z), ..SweepOptions::closed_loop() }, |_| wire)
}

/// A domed rivet head of `radius`, base at y = 0, rising to `0.6 * radius`.
pub fn rivet(radius: f32) -> MeshBuilder {
    let r = radius;
    let profile = spline_profile(&[[r, 0.0], [0.9 * r, 0.3 * r], [0.5 * r, 0.56 * r], [0.0, 0.62 * r]], 8);
    lathe(&profile, &LatheOptions { facets: 12, shading: Shading::SMOOTH, cap_start: true, cap_end: false, phase: 0.0 })
}

/// Domed rivets of `radius` at each `(point, outward normal)` spot.
pub fn rivets(spots: &[(V3, V3)], radius: f32) -> MeshBuilder {
    let head = rivet(radius);
    let mut mesh = MeshBuilder::new();
    for &(at, normal) in spots {
        let mut r = head.clone();
        r.place(at, normal);
        mesh.append(&r);
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    #[test]
    fn strap_lies_on_its_path_and_stands_proud() {
        let path: Vec<V3> = (0..=10).map(|i| [0.0, i as f32 * 0.1, 1.0]).collect();
        let faces = vec![vec3::Z; path.len()];
        let mesh = strap(&path, &faces, 0.2, 0.05, false);
        let (lo, hi) = mesh.bounds().unwrap();
        assert!((lo[2] - 1.0).abs() < 1e-4 && (hi[2] - 1.05).abs() < 1e-4, "{lo:?} {hi:?}");
        assert!((hi[0] - 0.1).abs() < 1e-4 && (lo[0] + 0.1).abs() < 1e-4);
        mesh.into_part("strap", OutlineMode::Welded).validate().unwrap();
    }

    #[test]
    fn hardware_is_drawable() {
        for mesh in [buckle(0.2, 0.02), d_ring(0.2, 0.02), rivets(&[([0.0; 3], vec3::X), ([1.0, 0.0, 0.0], vec3::Y)], 0.05)] {
            assert!(!mesh.is_empty());
            mesh.into_part("hardware", OutlineMode::Welded).validate().unwrap();
        }
    }
}
