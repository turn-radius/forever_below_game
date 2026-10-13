//! Geometry for a thrusting polearm head: the [`Spearhead`] blade and its [`Lugs`].
//!
//! The blade is the prototype's `bladeGeo` width profile (shoulder, belly, taper, point,
//! flame waves) times a cross-section (lenticular, diamond, hexagonal or flat, with fullers),
//! rebuilt for spears: each face between creases is its own smooth strip, so the ridge of a
//! diamond and the bevels of a hexagonal section stay crisp while the faces curve; the two
//! sides meet at a true knife edge; the socket's neck carries on up the blade as a round
//! midrib that blends into the faces; and the thickness never exceeds the width, so a narrow
//! bodkin or awl head closes to a square point. Wings are flat plates (thick at the root,
//! thinner at the tip) in the blade's plane rather than swept rods, and a bar is a turned
//! toggle through the socket.
//!
//! Built in head space: the blade's base at y = 0, its point up +Y, edges along ±X, faces
//! toward ±Z. The family sets it on the socket.

use procgen_rng::Mulberry32;

use super::hafted::smoothstep;
use crate::geom::polygon::P2;
use crate::geom::{GridOptions, LatheOptions, MeshBuilder, Shading, V3, extrude_tapered, lathe, spline_profile, stitch_grid, uv_sphere, vec3};
use crate::sections::spearhead::{LugEnds, LugStyle, Lugs, Spearhead};

/// Rows along the blade before waves add more.
const ROWS: usize = 110;
/// Columns across each strip between creases.
const COLUMNS: usize = 14;

/// The blade's half-width at `s` (0 at the base, 1 at the point), before nicks or breaks:
/// the prototype's `prof()` for a straight, double-edged blade.
pub fn half_width(head: &Spearhead, s: f32) -> f32 {
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let ps = head.point_start.clamp(0.05, 0.995);
    let bel = head.belly.clamp(0.001, ps - 0.001);
    let body = |x: f32| {
        if x <= bel {
            lerp(head.shoulder, head.belly_width, smoothstep(0.0, bel, x))
        } else {
            lerp(head.belly_width, 1.0 - head.taper, smoothstep(bel, ps, x))
        }
    };
    if s >= 1.0 {
        return 0.0;
    }
    let (mut w, u) = if s > ps {
        let u = (s - ps) / (1.0 - ps);
        let fullness = head.point_fullness;
        (body(ps) * ((1.0 - u) * (1.0 - fullness) + (1.0 - u * u).max(0.0).sqrt() * fullness), u)
    } else {
        (body(s), 0.0)
    };
    if head.waves > 0 && head.wave_depth > 0.0 {
        let fade = if s > ps { 1.0 - u } else { smoothstep(0.0, 0.06, s) };
        w += head.wave_depth * (s * head.waves as f32 * std::f32::consts::TAU).sin() * fade;
    }
    head.width * 0.5 * w.max(0.02)
}

/// The widest the blade gets, either side of its axis.
pub fn max_half_width(head: &Spearhead) -> f32 {
    (0..=200).map(|i| half_width(head, i as f32 / 200.0)).fold(0.0, f32::max)
}

/// How a snapped point ends: the blade stops at `at` (a fraction of its length) in a ragged
/// break drawn from `seed`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Break {
    pub at: f32,
    pub seed: u32,
}

/// The spearhead blade over a neck of radius `neck` (the socket's top), its base at y = 0.
/// `broken` snaps the point off.
pub fn spearhead_mesh(head: &Spearhead, neck: f32, broken: Option<Break>) -> MeshBuilder {
    let length = head.length.max(0.1);
    let s_max = broken.map_or(1.0, |b| b.at.clamp(0.2, 1.0));
    let rows = ROWS + head.waves as usize * 12;
    let half_t = head.thickness * 0.5;
    let fullers = head.fullers.max(1);
    let fuller_centre = |k: u32| if fullers == 1 { 0.0 } else { (k as f32 / (fullers - 1) as f32 - 0.5) * 0.75 };
    let rib_len = head.midrib.clamp(0.0, 1.0);
    // Ragged break: the last row steps back by a jagged amount across the blade.
    let jag = |v: f32| -> f32 {
        match broken {
            Some(b) => {
                let mut rng = Mulberry32::new(b.seed.wrapping_mul(89).wrapping_add(13));
                let (p1, p2) = (rng.range_f32(0.0, 6.0), rng.range_f32(0.0, 6.0));
                length * 0.035 * (0.5 + 0.5 * (v * 5.3 + p1).sin() * (v * 11.7 + p2).cos())
            }
            None => 0.0,
        }
    };

    // One row of [x, y, z] per s, `columns` across v in -1..=1 for the upper face.
    // The shoulders slope in to the neck over the first few percent, so a wide blade grows
    // out of the socket instead of sitting on it.
    let ramp = (0.35 / length).clamp(0.03, 0.12);
    let sample = |s: f32, v: f32, last: bool| -> V3 {
        let w = half_width(head, s);
        let at_neck = (neck * 0.9).min(w);
        let w = at_neck + (w - at_neck) * smoothstep(0.0, ramp, s);
        let x = v * w;
        let t = (half_t * (1.0 - 0.6 * s)).min(w * 0.9).max(if s < 1.0 { 0.002 } else { 0.0 });
        let mut z = t * head.section.thickness_at(v);
        if head.fuller > 0.0 {
            let fade = smoothstep(0.0, 0.04, s) * (1.0 - smoothstep(head.fuller_length - 0.06, head.fuller_length, s));
            let d: f32 = (0..fullers).map(|k| (-((v - fuller_centre(k)) / (head.fuller_width * 0.5).max(0.02)).powi(2)).exp()).sum();
            z = (z * 0.35).max(z - t * head.fuller * 0.6 * fade * d.min(1.0));
        }
        if rib_len > 0.0 {
            let rr = (neck * (1.0 - (s / rib_len).min(1.0)).sqrt()).min(w * 0.6).min(t * 1.6);
            if rr > 0.0 && x.abs() < rr {
                z = z.max((rr * rr - x * x).sqrt());
            }
        }
        let y = if last { s * length - jag(v) } else { s * length };
        [x, y, z]
    };

    let creases = head.section.creases();
    let mut out = MeshBuilder::new();
    for side in [1.0f32, -1.0] {
        for pair in creases.windows(2) {
            let (v0, v1) = (pair[0], pair[1]);
            let columns: Vec<f32> = (0..=COLUMNS).map(|j| v0 + (v1 - v0) * j as f32 / COLUMNS as f32).collect();
            // The underside runs the other way across, so both faces front outward.
            let columns: Vec<f32> = if side > 0.0 { columns } else { columns.into_iter().rev().collect() };
            let points: Vec<Vec<V3>> = (0..=rows)
                .map(|i| {
                    let s = s_max * i as f32 / rows as f32;
                    columns
                        .iter()
                        .map(|&v| {
                            let p = sample(s, v, i == rows && broken.is_some());
                            [p[0], p[1], side * p[2]]
                        })
                        .collect()
                })
                .collect();
            out.append(&stitch_grid(&points, &GridOptions { wrap_around: false, wrap_along: false, shading: Shading::SMOOTH, row_v: None }));
        }
    }
    // Close the base, and the break if there is one.
    let ring = |s: f32, last: bool| -> Vec<V3> {
        let across: Vec<f32> = (0..=COLUMNS * 2).map(|j| -1.0 + j as f32 / COLUMNS as f32).collect();
        let mut pts: Vec<V3> = across.iter().map(|&v| sample(s, v, last)).collect();
        pts.extend(across.iter().rev().skip(1).take(across.len() - 2).map(|&v| {
            let p = sample(s, v, last);
            [p[0], p[1], -p[2]]
        }));
        pts
    };
    out.push_fan_cap(&ring(0.0, false), vec3::neg(vec3::Y));
    if broken.is_some() {
        out.push_fan_cap(&ring(s_max, true), vec3::Y);
    }
    out
}

/// A terminal for the end of a lug, its foot at the origin facing +Y.
fn terminal(ends: LugEnds, size: f32, root: f32) -> MeshBuilder {
    let e = size;
    let control: &[[f32; 2]] = match ends {
        LugEnds::None => return MeshBuilder::new(),
        LugEnds::Ball => {
            let mut ball = uv_sphere(e, 16, 10);
            ball.translate([0.0, e * 0.75, 0.0]);
            return ball;
        }
        LugEnds::Disc => &[[root, 0.0], [e * 1.1, e * 0.1], [e * 1.15, e * 0.35], [e, e * 0.5], [0.0, e * 0.55]],
        LugEnds::Flare => &[[root * 0.9, 0.0], [e * 0.6, e * 0.5], [e, e], [e * 1.02, e * 1.15], [0.0, e * 1.18]],
    };
    lathe(&spline_profile(control, 14), &LatheOptions { facets: 16, shading: Shading::SMOOTH, ..Default::default() })
}

/// Wings or a bar either side of the axis, centred on y = 0, reaching `span` out from a
/// root circle of `root` (the socket or blade base they spring from). Wings lie flat in the
/// blade's plane (XY), sweeping toward the point by `curve`.
pub fn lugs_mesh(lugs: &Lugs, root: f32) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    let th = lugs.thickness;
    let span = lugs.span;
    match lugs.style {
        LugStyle::Bar => {
            // A turned toggle through the socket, thinning toward its ends.
            let reach = root + span;
            let end_r = th * (1.0 - lugs.taper * 0.5);
            let control = [[end_r, -reach], [th * 0.95, -reach * 0.55], [th * 1.05, 0.0], [th * 0.95, reach * 0.55], [end_r, reach]];
            let mut bar = lathe(&spline_profile(&control, 24), &LatheOptions { facets: 12, shading: Shading::SMOOTH, ..Default::default() });
            bar.rotate_z(-std::f32::consts::FRAC_PI_2);
            out.append(&bar);
            for side in [1.0f32, -1.0] {
                let mut t = terminal(lugs.ends, lugs.end_size, end_r);
                t.place([side * (reach - lugs.end_size * 0.1), 0.0, 0.0], [side, 0.0, 0.0]);
                out.append(&t);
            }
        }
        LugStyle::Wings => {
            // The centreline from root to tip, and the plate's half-height round it.
            let n = 24;
            let rise = lugs.curve * span * 0.6;
            let centre = |t: f32| -> P2 { [root * 0.85 + (span + root * 0.15) * t, rise * t * t] };
            let tangent = |t: f32| -> P2 {
                let d = [span + root * 0.15, 2.0 * rise * t];
                let l = d[0].hypot(d[1]).max(1e-6);
                [d[0] / l, d[1] / l]
            };
            // Deep at the root, narrowing straight to the tip (more so with taper), the
            // leading (upper) edge fuller than the trailing one, and the very tip rounded.
            let h0 = th * 1.5 + span * 0.3;
            let half = |t: f32| h0 * (1.0 - t * (0.45 + 0.5 * lugs.taper)) * (1.0 - t.powi(8)).max(0.0).powf(0.35);
            let mut upper = Vec::with_capacity(n + 1);
            let mut lower = Vec::with_capacity(n + 1);
            for i in 0..=n {
                let t = i as f32 / n as f32;
                let (c, d) = (centre(t), tangent(t));
                let normal = [-d[1], d[0]];
                let h = half(t);
                upper.push([c[0] + normal[0] * h * 1.15, c[1] + normal[1] * h * 1.15]);
                lower.push([c[0] - normal[0] * h * 0.75, c[1] - normal[1] * h * 0.75]);
            }
            let mut outline: Vec<P2> = lower;
            outline.extend(upper.into_iter().rev().skip(1));
            let wing = extrude_tapered(&outline, &[], th * 1.2, th * 1.2 * (1.0 - lugs.taper * 0.6));
            let tip = centre(1.0);
            let dir = tangent(1.0);
            let end_root = half(0.97).max(th * 0.4);
            for side in [1.0f32, -1.0] {
                let mut w = wing.clone();
                let mut t = terminal(lugs.ends, lugs.end_size, end_root);
                t.place([tip[0] - dir[0] * lugs.end_size * 0.2, tip[1] - dir[1] * lugs.end_size * 0.2, 0.0], [dir[0], dir[1], 0.0]);
                if side < 0.0 {
                    w.scale([-1.0, 1.0, 1.0]);
                    t.scale([-1.0, 1.0, 1.0]);
                }
                out.append(&w);
                out.append(&t);
            }
        }
    }
    out
}
