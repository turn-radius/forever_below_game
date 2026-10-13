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

/// Revolved rows from angle `start` to `end` in `facets` steps, in [`lathe`]'s convention.
fn sector_points(profile: &[[f32; 2]], facets: usize, start: f32, end: f32, warp: &impl Fn(V3) -> V3) -> Vec<Vec<V3>> {
    let angles: Vec<(f32, f32)> = (0..=facets).map(|j| (start + (end - start) * j as f32 / facets as f32).sin_cos()).collect();
    profile.iter().map(|&[r, y]| angles.iter().map(|&(s, c)| warp([r * c, y, -r * s])).collect()).collect()
}

/// Normalized arc length along a profile, one value per point.
fn profile_row_v(profile: &[[f32; 2]]) -> Vec<f32> {
    let mut arc = vec![0.0f32; profile.len()];
    for i in 1..profile.len() {
        let (a, b) = (profile[i - 1], profile[i]);
        arc[i] = arc[i - 1] + ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    }
    let total = arc[arc.len() - 1].max(1e-9);
    arc.iter().map(|a| a / total).collect()
}

/// [`lathe`] over part of a turn, from angle `start` to `end` radians (`end > start`, same
/// convention as [`lathe`]): a cheese with a wedge cut out, a slice of pie, a cut apple. The
/// cut sides stay open; close them with [`super::section::profile_face`] turned to `start`
/// (facing as built) and to `end` (flipped). `facets` counts the steps across the arc, and
/// `cap_start` / `cap_end` close off-axis profile ends with flat wedges. `phase` is ignored.
pub fn lathe_sector(profile: &[[f32; 2]], options: &LatheOptions, start: f32, end: f32) -> MeshBuilder {
    let facets = options.facets.max(1);
    let profile: Vec<[f32; 2]> = profile.iter().map(|&[r, y]| [r.max(0.0), y]).collect();
    if profile.len() < 2 || end <= start {
        return MeshBuilder::new();
    }
    let points = sector_points(&profile, facets, start, end, &|p| p);
    let row_v = profile_row_v(&profile);
    let mut mesh = stitch_grid(&points, &GridOptions { wrap_around: false, wrap_along: false, shading: options.shading, row_v: Some(row_v) });
    let last = profile.len() - 1;
    for (wanted, row, neighbour) in [(options.cap_start, 0usize, 1usize), (options.cap_end, last, last - 1)] {
        let [r, y] = profile[row];
        if !wanted || r <= AXIS_EPSILON {
            continue;
        }
        let up = y >= profile[neighbour][1];
        let facing = if up { vec3::Y } else { vec3::neg(vec3::Y) };
        let axis = [0.0, y, 0.0];
        let center = mesh.push_vertex(axis, facing, [0.5, 0.5]);
        let rim: Vec<u32> = points[row].iter().map(|&p| mesh.push_vertex(p, facing, [0.5 + 0.5 * p[0] / r, 0.5 + 0.5 * p[2] / r])).collect();
        for k in 0..rim.len() - 1 {
            let face = vec3::cross(vec3::sub(points[row][k], axis), vec3::sub(points[row][k + 1], axis));
            if vec3::dot(face, facing) >= 0.0 { mesh.push_triangle(center, rim[k], rim[k + 1]) } else { mesh.push_triangle(center, rim[k + 1], rim[k]) };
        }
    }
    mesh
}

/// [`lathe`] with every revolved point passed through `warp` before the surface is stitched,
/// so shading follows the warped shape: lumpy roasts, gilled caps, wrinkled skins, squashed
/// or bent turnings. Ends are left open (warp a pole-to-pole profile for a closed solid).
pub fn lathe_warped(profile: &[[f32; 2]], options: &LatheOptions, warp: impl Fn(V3) -> V3) -> MeshBuilder {
    let facets = options.facets.max(3);
    let profile: Vec<[f32; 2]> = profile.iter().map(|&[r, y]| [r.max(0.0), y]).collect();
    if profile.len() < 2 {
        return MeshBuilder::new();
    }
    let turn = std::f32::consts::TAU;
    let mut points = sector_points(&profile, facets, options.phase, options.phase + turn, &warp);
    for row in &mut points {
        row.pop();
    }
    stitch_grid(&points, &GridOptions { wrap_around: true, wrap_along: false, shading: options.shading, row_v: Some(profile_row_v(&profile)) })
}

/// A hollow vessel's profile from its outside: up `outer` (which should start on the axis, at
/// the base), over a flat lip `wall` wide, and back down the inside `wall` thick to the axis.
/// Lathe it for a cup, a tankard, a dish or a bowl with real thickness at the lip.
pub fn shell_profile(outer: &[[f32; 2]], wall: f32) -> Vec<[f32; 2]> {
    if outer.len() < 2 {
        return outer.to_vec();
    }
    let mut inner = super::section::inset_polyline(outer, wall, false);
    let rim = outer[outer.len() - 1];
    if let Some(top) = inner.last_mut() {
        *top = [rim[0] - wall, rim[1]];
    }
    inner[0][0] = 0.0;
    let mut out = outer.to_vec();
    out.extend(inner.into_iter().rev().map(|[r, y]| [r.max(0.0), y]));
    out
}

/// The inside of a [`shell_profile`]: its points after the lip, walked from the axis at the
/// bottom up to the lip. What a vessel holds is this, clipped with [`clip_profile`].
pub fn shell_inside(shell: &[[f32; 2]], outer_len: usize) -> Vec<[f32; 2]> {
    shell[outer_len.min(shell.len())..].iter().rev().copied().collect()
}

/// The part of a profile walked bottom to top that lies at or below `level`, closed onto the
/// axis at `level`: the liquid in a vessel, given the vessel's inside profile.
pub fn clip_profile(profile: &[[f32; 2]], level: f32) -> Vec<[f32; 2]> {
    let mut out = Vec::new();
    for w in profile.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a[1] <= level {
            out.push(a);
        }
        if (a[1] <= level) != (b[1] <= level) {
            let t = (level - a[1]) / (b[1] - a[1]);
            out.push([a[0] + (b[0] - a[0]) * t, level]);
        }
    }
    if let Some(&last) = profile.last()
        && last[1] <= level
    {
        out.push(last);
    }
    if let Some(&last) = out.last()
        && last[0] > AXIS_EPSILON
    {
        out.push([0.0, last[1]]);
    }
    out
}

/// Radius of a profile at height `y`, from its first span that crosses `y` (searching from
/// the top when `from_top`), or `None` if it never reaches that height. For setting things
/// on a turned surface: a hoop round a tankard, a label on a bottle.
pub fn profile_radius_at(profile: &[[f32; 2]], y: f32, from_top: bool) -> Option<f32> {
    let find = |w: &[[f32; 2]]| {
        let (a, b) = (w[0], w[1]);
        if y < a[1].min(b[1]) || y > a[1].max(b[1]) {
            return None;
        }
        let t = if (b[1] - a[1]).abs() < 1e-9 { 0.5 } else { (y - a[1]) / (b[1] - a[1]) };
        Some(a[0] + (b[0] - a[0]) * t)
    };
    if from_top { profile.windows(2).rev().find_map(find) } else { profile.windows(2).find_map(find) }
}

/// Height of a profile's upper surface at radius `r` (its last span covering `r`): the top
/// of a dome, a crust or a cap, for setting scores, vents and spots on it.
pub fn profile_height_at(profile: &[[f32; 2]], r: f32) -> Option<f32> {
    profile.windows(2).rev().find_map(|w| {
        let (a, b) = (w[0], w[1]);
        if r < a[0].min(b[0]) || r > a[0].max(b[0]) {
            return None;
        }
        let t = if (b[0] - a[0]).abs() < 1e-9 { 0.5 } else { (r - a[0]) / (b[0] - a[0]) };
        Some(a[1] + (b[1] - a[1]) * t)
    })
}
