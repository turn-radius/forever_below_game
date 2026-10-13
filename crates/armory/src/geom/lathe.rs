//! Lathe: revolve a radial profile about +Y. Hafts, grips, pommels, ball heads, helm bowls,
//! bottles, ring bands and anything else turned on a wheel.

use super::builder::MeshBuilder;
use super::grid::{GridOptions, Shading, stitch_grid};
use super::spline::CatmullRom;
use super::vec3::{self, V3};

/// How [`lathe`] revolves a profile.
#[derive(Clone, Debug, PartialEq)]
pub struct LatheOptions {
    /// Points around (sides of the polygon the profile sweeps). At least 3.
    pub facets: usize,
    pub shading: Shading,
    /// Close the first profile point with a flat disc, if it is off the axis.
    pub cap_start: bool,
    /// Close the last profile point with a flat disc, if it is off the axis.
    pub cap_end: bool,
    /// Rotation of the first facet about +Y, in radians (turn a hexagon to sit flat or on edge).
    pub phase: f32,
}

impl Default for LatheOptions {
    fn default() -> Self {
        LatheOptions { facets: 16, shading: Shading::SMOOTH, cap_start: true, cap_end: true, phase: 0.0 }
    }
}

impl LatheOptions {
    pub fn new(facets: usize, shading: Shading) -> Self {
        LatheOptions { facets, shading, ..Default::default() }
    }
}

/// Below this radius a profile point counts as on the axis (a pole), and gets no cap.
const AXIS_EPSILON: f32 = 1e-5;

/// Revolves `profile`, a list of `[radius, y]` points, about +Y.
///
/// Walk the profile so the outside is on your right in the (radius, y) plane: bottom to top
/// for a solid like a haft. A point with radius 0 is a pole and closes the surface there
/// without NaN normals. UV u runs around, v along the profile's arc length.
pub fn lathe(profile: &[[f32; 2]], options: &LatheOptions) -> MeshBuilder {
    let facets = options.facets.max(3);
    let profile: Vec<[f32; 2]> = profile.iter().map(|&[r, y]| [r.max(0.0), y]).collect();
    if profile.len() < 2 {
        return MeshBuilder::new();
    }
    let angles: Vec<(f32, f32)> =
        (0..facets).map(|j| (options.phase + std::f32::consts::TAU * j as f32 / facets as f32).sin_cos()).collect();
    // Angle runs from +X toward -Z, so du x dv faces outward for a profile walked upward.
    let points: Vec<Vec<V3>> = profile.iter().map(|&[r, y]| angles.iter().map(|&(s, c)| [r * c, y, -r * s]).collect()).collect();

    let mut arc = vec![0.0f32; profile.len()];
    for i in 1..profile.len() {
        let (a, b) = (profile[i - 1], profile[i]);
        arc[i] = arc[i - 1] + ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    }
    let total = arc[arc.len() - 1].max(1e-9);
    let row_v = arc.iter().map(|a| a / total).collect();

    let mut mesh = stitch_grid(&points, &GridOptions { wrap_around: true, wrap_along: false, shading: options.shading, row_v: Some(row_v) });

    let (first, second) = (profile[0], profile[1]);
    if options.cap_start && first[0] > AXIS_EPSILON {
        let facing = if first[1] <= second[1] { vec3::neg(vec3::Y) } else { vec3::Y };
        mesh.push_fan_cap(&points[0], facing);
    }
    let (last, before) = (profile[profile.len() - 1], profile[profile.len() - 2]);
    if options.cap_end && last[0] > AXIS_EPSILON {
        let facing = if last[1] >= before[1] { vec3::Y } else { vec3::neg(vec3::Y) };
        mesh.push_fan_cap(&points[points.len() - 1], facing);
    }
    mesh
}

/// Revolves a profile given as a function of `t` in `0..=1`, sampled at `samples + 1` points.
pub fn lathe_fn(samples: usize, options: &LatheOptions, profile: impl Fn(f32) -> [f32; 2]) -> MeshBuilder {
    let samples = samples.max(1);
    let points: Vec<[f32; 2]> = (0..=samples).map(|i| profile(i as f32 / samples as f32)).collect();
    lathe(&points, options)
}

/// Smooths a lathe profile of `[radius, y]` control points into about `samples` points on a
/// centripetal Catmull-Rom spline through every control point (the prototype's
/// `SplineCurve(...).getPoints()`, without its cusps). Radii are clamped at zero, so a
/// control point on the axis stays a clean pole.
pub fn spline_profile(control: &[[f32; 2]], samples: usize) -> Vec<[f32; 2]> {
    if control.len() < 3 {
        return control.to_vec();
    }
    let curve = CatmullRom::centripetal(control.iter().map(|&[r, y]| [r, y, 0.0]).collect());
    let per_segment = samples.div_ceil(curve.segment_count()).max(1);
    curve.sample(per_segment).into_iter().map(|p| [p[0].max(0.0), p[1]]).collect()
}
