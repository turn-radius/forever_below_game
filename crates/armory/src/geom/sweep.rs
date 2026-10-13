//! Sweeps: a 2D cross-section carried along a 3D path on rotation-minimizing frames. Cords,
//! grip wraps (along a [`super::spline::helix`]), prongs, curls, bow limbs and strings,
//! torcs and rings.

use super::builder::MeshBuilder;
use super::grid::{GridOptions, Shading, stitch_grid};
use super::polygon::{P2, signed_area, triangulate};
use super::spline::{Frame, rotation_minimizing_frames};
use super::vec3::{self, V3};

/// How [`sweep`] and [`tube`] follow a path.
#[derive(Clone, Debug, PartialEq)]
pub struct SweepOptions {
    pub shading: Shading,
    /// Close the start with a flat cap (skipped where the section has shrunk to a point).
    pub cap_start: bool,
    /// Close the end with a flat cap (skipped where the section has shrunk to a point).
    pub cap_end: bool,
    /// The path's last point joins its first (a ring, a loop of cord). Closed sweeps have no caps.
    pub closed: bool,
    /// Which way the section's +x points at the start of the path. None picks a stable default.
    pub up: Option<V3>,
    /// Extra turn of the section from start to end, in radians (twisted wire, a twisted haft).
    pub twist: f32,
}

impl Default for SweepOptions {
    fn default() -> Self {
        SweepOptions { shading: Shading::SMOOTH, cap_start: true, cap_end: true, closed: false, up: None, twist: 0.0 }
    }
}

impl SweepOptions {
    /// A closed loop: no caps.
    pub fn closed_loop() -> Self {
        SweepOptions { closed: true, cap_start: false, cap_end: false, ..Default::default() }
    }
}

/// `sides` points evenly round a circle of `radius`, counter-clockwise from angle `phase`.
pub fn regular_polygon(sides: usize, radius: f32, phase: f32) -> Vec<P2> {
    let sides = sides.max(3);
    (0..sides)
        .map(|k| {
            let (s, c) = (phase + std::f32::consts::TAU * k as f32 / sides as f32).sin_cos();
            [radius * c, radius * s]
        })
        .collect()
}

/// Normalized arc length at each path point (a closed path counts its closing edge).
fn arc_parameters(path: &[V3], closed: bool) -> Vec<f32> {
    let mut t = vec![0.0f32; path.len()];
    for i in 1..path.len() {
        t[i] = t[i - 1] + vec3::distance(path[i - 1], path[i]);
    }
    let closing = if closed && path.len() > 1 { vec3::distance(path[path.len() - 1], path[0]) } else { 0.0 };
    let total = t.last().copied().unwrap_or(0.0) + closing;
    if total > 1e-9 {
        t.iter_mut().for_each(|x| *x /= total);
    }
    t
}

fn place(frame: &Frame, point: P2, angle: f32, scale: f32) -> V3 {
    let (s, c) = angle.sin_cos();
    frame.place([(point[0] * c - point[1] * s) * scale, (point[0] * s + point[1] * c) * scale])
}

/// Sweeps `section` (a closed 2D polygon; x along the frame normal, y along the binormal)
/// along `path`, scaled by `scale(t)` where `t` runs 0..=1 by arc length. The section may
/// be given in either winding. UV u runs round the section, v along the path.
pub fn sweep(path: &[V3], section: &[P2], options: &SweepOptions, scale: impl Fn(f32) -> f32) -> MeshBuilder {
    if path.len() < 2 || section.len() < 3 {
        return MeshBuilder::new();
    }
    let section: Vec<P2> = if signed_area(section) < 0.0 { section.iter().rev().copied().collect() } else { section.to_vec() };
    let frames = rotation_minimizing_frames(path, options.up, options.closed);
    let ts = arc_parameters(path, options.closed);
    let scales: Vec<f32> = ts.iter().map(|&t| scale(t).max(0.0)).collect();
    let rows: Vec<Vec<V3>> = frames
        .iter()
        .zip(&ts)
        .zip(&scales)
        .map(|((f, &t), &k)| section.iter().map(|&q| place(f, q, options.twist * t, k)).collect())
        .collect();
    let mut mesh = stitch_grid(&rows, &GridOptions { wrap_around: true, wrap_along: options.closed, shading: options.shading, row_v: None });

    if !options.closed {
        let tris = triangulate(&section, &[]);
        let ends = [(options.cap_start, 0usize, false), (options.cap_end, rows.len() - 1, true)];
        for (wanted, row, at_end) in ends {
            if !wanted || scales[row] < 1e-6 {
                continue;
            }
            // CCW in the (normal, binormal) plane faces +tangent.
            let normal = if at_end { frames[row].tangent } else { vec3::neg(frames[row].tangent) };
            let base = mesh.vertex_count() as u32;
            let r = section.iter().map(|q| q[0].hypot(q[1])).fold(1e-6f32, f32::max);
            for (k, &p) in rows[row].iter().enumerate() {
                let q = section[k];
                mesh.push_vertex(p, normal, [0.5 + 0.5 * q[0] / r, 0.5 + 0.5 * q[1] / r]);
            }
            for t in &tris {
                if at_end { mesh.push_triangle(base + t[0], base + t[1], base + t[2]) } else { mesh.push_triangle(base + t[0], base + t[2], base + t[1]) };
            }
        }
    }
    mesh
}

/// A round (or `sides`-sided) tube along `path` with radius `radius(t)`, `t` 0..=1 by arc length.
pub fn tube(path: &[V3], sides: usize, options: &SweepOptions, radius: impl Fn(f32) -> f32) -> MeshBuilder {
    sweep(path, &regular_polygon(sides, 1.0, 0.0), options, radius)
}
