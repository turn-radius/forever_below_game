//! Geometry for the handle half of a hafted item: the shaft, its bands, the grip, the wrap
//! and the pommel. Staves, maces, swords, spears and daggers all build their handles here
//! from the [`crate::sections::hafted`] sections.
//!
//! Item space runs +Y from the butt. A [`Haft`] is the shaft's centreline and radius: every
//! other piece is laid along it, so a gnarled, bent shaft carries its grip, wrap, bands and
//! pommel with it.
//!
//! From the prototype's `build()` in `reference/spline_armory_engine.js` ("Shaft spline",
//! "Grip region", "Wrapping", "Shaft bands", "Pommel"), rebuilt at higher fidelity: knots
//! are local bumps rather than rings, the wrap is a strap or cord seated on the grip (and
//! woven over and under when cross-wound), bindings are tight turns of cord rather than a
//! torus, and stones are seated in the pommel rather than stuck on it.

use procgen_rng::Mulberry32;

use super::gem::{cut_for, cut_stone, stone_reach};
use crate::gem::GemCut;
use crate::geom::{
    CatmullRom, Frame, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, lathe, regular_polygon, resample_by_length,
    rotation_minimizing_frames, spline_profile, sweep_frames, tube, tube_varying, vec3,
};
use crate::material::MaterialKind;
use crate::sections::hafted::{Grip, Pommel, PommelGem, PommelStyle, Shaft, Wrap};

/// Points along the haft's centreline, evenly spaced by arc length.
const AXIS_SAMPLES: usize = 161;

/// Hermite smoothstep from `a` to `b` (either order).
pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Faceted shading for low facet counts (a hexagonal pommel), smooth and densely turned
/// for anything meant to read as round. Returns the sides to lathe and the shading.
pub fn turned(facets: u32) -> (usize, Shading) {
    if facets <= 12 { (facets.max(3) as usize, Shading::FACETED) } else { ((facets as usize * 2).clamp(28, 40), Shading::SMOOTH) }
}

/// A knot in the wood: a bump at one height and angle, with the grain swelling round it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Knot {
    t: f32,
    angle: f32,
    size: f32,
}

/// The shaft's centreline and radius. `t` runs 0 (butt) to 1 (top) by arc length.
#[derive(Clone, Debug, PartialEq)]
pub struct Haft {
    axis: Vec<V3>,
    frames: Vec<Frame>,
    length: f32,
    radius: f32,
    taper: f32,
    gnarl: f32,
    knots: Vec<Knot>,
}

impl Haft {
    /// Lays out `shaft`. The seed bends a gnarled shaft and places its knots.
    pub fn new(shaft: &Shaft, seed: u32) -> Haft {
        let length = shaft.length.max(0.1);
        let gnarl = shaft.gnarl.max(0.0);
        let mut bend = Mulberry32::new(seed.wrapping_mul(7).wrapping_add(1));
        let amp = gnarl * shaft.radius * 2.4;
        // Bends most in the middle, least at the ends where the fittings sit.
        let control: Vec<V3> = (0..=9)
            .map(|k| {
                let t = k as f32 / 9.0;
                let e = (std::f32::consts::PI * t).sin() * 0.85 + 0.15;
                let (x, z) = ((bend.next_f64() as f32 - 0.5) * 2.0, (bend.next_f64() as f32 - 0.5) * 2.0);
                [amp * x * e, length * t, amp * z * e]
            })
            .collect();
        let axis = resample_by_length(&CatmullRom::centripetal(control).sample(16), AXIS_SAMPLES, false);
        let frames = rotation_minimizing_frames(&axis, Some(vec3::X), false);

        let mut grain = Mulberry32::new(seed.wrapping_mul(37).wrapping_add(17));
        let count = if gnarl > 0.01 { (shaft.gnarl_freq * length).round().max(1.0) as usize } else { 0 };
        let knots = (0..count)
            .map(|k| Knot {
                t: (k as f32 + 0.2 + 0.6 * grain.next_f64() as f32) / count as f32,
                angle: grain.range_f32(0.0, std::f32::consts::TAU),
                size: grain.range_f32(0.5, 1.0),
            })
            .collect();
        Haft { axis, frames, length, radius: shaft.radius, taper: shaft.taper, gnarl, knots }
    }

    /// Sands the knots off `t0..=t1` (and a little either side), where a grip or wrap goes.
    pub fn clear(&mut self, t0: f32, t1: f32) {
        let margin = 0.25 / self.length;
        self.knots.retain(|k| k.t < t0 - margin || k.t > t1 + margin);
    }

    pub fn length(&self) -> f32 {
        self.length
    }

    fn sample(&self, t: f32) -> (usize, f32) {
        let x = t.clamp(0.0, 1.0) * (self.axis.len() - 1) as f32;
        let i = (x.floor() as usize).min(self.axis.len() - 2);
        (i, x - i as f32)
    }

    /// The centreline point at `t`.
    pub fn point(&self, t: f32) -> V3 {
        let (i, f) = self.sample(t);
        vec3::lerp(self.axis[i], self.axis[i + 1], f)
    }

    /// The rotation-minimizing frame at `t` (tangent up the shaft).
    pub fn frame(&self, t: f32) -> Frame {
        let (i, f) = self.sample(t);
        let (a, b) = (self.frames[i], self.frames[i + 1]);
        let tangent = vec3::normalize_or(vec3::lerp(a.tangent, b.tangent, f), a.tangent);
        let n = vec3::lerp(a.normal, b.normal, f);
        let normal = vec3::normalize_or(vec3::sub(n, vec3::scale(tangent, vec3::dot(n, tangent))), a.normal);
        Frame { origin: self.point(t), tangent, normal, binormal: vec3::cross(tangent, normal) }
    }

    pub fn tangent(&self, t: f32) -> V3 {
        self.frame(t).tangent
    }

    /// The turned radius at `t`, tapered, without knots.
    pub fn radius(&self, t: f32) -> f32 {
        self.radius * (1.0 + (self.taper - 1.0) * t.clamp(0.0, 1.0))
    }

    /// The surface radius at `t` and `angle` (radians from the frame normal toward the
    /// binormal), knots included.
    pub fn surface_radius(&self, t: f32, angle: f32) -> f32 {
        let mut k = 1.0;
        for knot in &self.knots {
            let dy = (t - knot.t) * self.length;
            if dy.abs() > 0.8 {
                continue;
            }
            let mut da = (angle - knot.angle).rem_euclid(std::f32::consts::TAU);
            da = da.min(std::f32::consts::TAU - da);
            let swell = (-(dy / 0.22).powi(2)).exp();
            let bump = (-(dy / 0.09).powi(2)).exp() * (-(da / 0.5).powi(2)).exp();
            k += self.gnarl * knot.size * (0.35 * swell + 1.6 * bump);
        }
        self.radius(t) * k
    }

    /// The widest the surface gets round `t`: what a band or socket must clear.
    pub fn outer_radius(&self, t: f32) -> f32 {
        (0..24).map(|k| self.surface_radius(t, std::f32::consts::TAU * k as f32 / 24.0)).fold(self.radius(t), f32::max)
    }

    /// `count` centreline points from `t0` to `t1`.
    pub fn path(&self, t0: f32, t1: f32, count: usize) -> Vec<V3> {
        let count = count.max(2);
        (0..count).map(|i| self.point(t0 + (t1 - t0) * i as f32 / (count - 1) as f32)).collect()
    }

    /// The shaft itself: a tube along the centreline, knotted where the grain says.
    pub fn mesh(&self) -> MeshBuilder {
        // Knots need rows and sides to show; a plain tapered haft is straight between bends.
        let (rows, sides) = if self.knots.is_empty() { ((16.0 + self.length * 5.0).round() as usize, 20) } else { ((40.0 + self.length * 16.0).round() as usize, 28) };
        let path = self.path(0.0, 1.0, rows);
        // tube_varying measures angle from its own frames; seed them like the haft's.
        let options = SweepOptions { up: Some(vec3::X), ..Default::default() };
        tube_varying(&path, sides, &options, |t, a| self.surface_radius(t, a))
    }
}

/// Raised metal rings along the shaft, skipping any `avoid` span (grips, wraps).
pub fn shaft_bands(haft: &Haft, shaft: &Shaft, avoid: &[(f32, f32)]) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    let n = shaft.bands;
    let bs = shaft.band_size;
    for i in 0..n {
        let t = (i + 1) as f32 / (n + 1) as f32;
        if avoid.iter().any(|&(a, b)| t > a - 0.02 && t < b + 0.02) {
            continue;
        }
        let r = haft.outer_radius(t);
        let control = [[r * 0.97, -bs], [r + bs * 0.7, -bs * 0.5], [r + bs * 0.8, 0.0], [r + bs * 0.7, bs * 0.5], [r * 0.97, bs]];
        let mut band = lathe(&spline_profile(&control, 12), &LatheOptions { facets: 28, shading: Shading::SMOOTH, ..Default::default() });
        band.place(haft.point(t), haft.tangent(t));
        out.append(&band);
    }
    out
}

/// Where the grip sits on the haft, as `(t0, t1)`.
pub fn grip_span(shaft: &Shaft, grip: &Grip) -> (f32, f32) {
    let t0 = grip.position.min(0.95);
    (t0, (t0 + grip.length / shaft.length.max(0.1)).min(1.0))
}

/// The radius of the grip's surface at `u` (0..=1 across the span): the shaft swelling to
/// `grip.thickness` with eased ends. A grip that is off leaves the bare shaft.
pub fn grip_radius(haft: &Haft, grip: &Grip, span: (f32, f32), u: f32) -> f32 {
    let t = span.0 + (span.1 - span.0) * u;
    let k = if grip.on { 1.0 + (grip.thickness - 1.0) * smoothstep(0.0, 0.07, u) * smoothstep(1.0, 0.93, u) } else { 1.0 };
    haft.radius(t) * k * 1.003
}

/// The grip sleeve. Bone grips are carved in shallow rings.
pub fn grip_mesh(haft: &Haft, grip: &Grip, span: (f32, f32)) -> MeshBuilder {
    let len = (span.1 - span.0) * haft.length();
    let rows = (12.0 + len * if grip.material == MaterialKind::Bone { 30.0 } else { 10.0 }).round() as usize;
    let path = haft.path(span.0, span.1, rows);
    let ribs = if grip.material == MaterialKind::Bone { (grip.length * 5.0).round().max(3.0) } else { 0.0 };
    let options = SweepOptions { up: Some(vec3::X), ..Default::default() };
    tube_varying(&path, 24, &options, |u, _| {
        let rib = if ribs > 0.0 { 1.0 - 0.035 * (1.0 - (u * ribs * std::f32::consts::TAU).cos()) * 0.5 } else { 1.0 };
        grip_radius(haft, grip, span, u) * rib
    })
}

/// A flat strap section, wider than thick: `thick` across the frame normal, `wide` across
/// the binormal, with rounded edges.
fn strap_section(thick: f32, wide: f32) -> Vec<[f32; 2]> {
    (0..10)
        .map(|k| {
            let a = std::f32::consts::TAU * k as f32 / 10.0;
            let (s, c) = a.sin_cos();
            [thick * c.signum() * c.abs().powf(0.6), wide * s.signum() * s.abs().powf(0.35)]
        })
        .collect()
}

/// Frames for a path wound round the haft: tangent along the path, normal pointing out from
/// the axis so a strap lies flat on the surface beneath it.
fn wound_frames(points: &[V3], outward: &[V3]) -> Vec<Frame> {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i.saturating_sub(1)], points[(i + 1).min(n - 1)]);
            let tangent = vec3::normalize_or(vec3::sub(b, a), vec3::Y);
            let o = outward[i];
            let normal = vec3::normalize_or(vec3::sub(o, vec3::scale(tangent, vec3::dot(o, tangent))), vec3::any_perpendicular(tangent));
            Frame { origin: points[i], tangent, normal, binormal: vec3::cross(tangent, normal) }
        })
        .collect()
}

/// Cord or strap wound over the grip span. Leather goes on as a flat strap, cord and wire
/// as round cord. A cross-wound wrap runs a second winding the other way, the two weaving
/// over and under where they cross. Bindings are tight turns of cord at each end.
pub fn wrap_mesh(haft: &Haft, grip: &Grip, span: (f32, f32), wrap: &Wrap) -> MeshBuilder {
    use std::f32::consts::TAU;
    let mut out = MeshBuilder::new();
    let cord = wrap.cord;
    let span_len = ((span.1 - span.0) * haft.length()).max(1e-3);
    // Keep the cord's ends inside the span, so a wrap that runs to the butt stops short of it.
    let inset = (cord * 1.2 / span_len).min(0.25);
    let c0 = ((1.0 - wrap.coverage) * 0.5).max(inset);
    let c1 = 1.0 - c0;
    let turns = wrap.turns.max(1) as f32;
    let pitch = span_len * (c1 - c0) / turns;
    let strap = wrap.material == MaterialKind::Leather;
    // Half thickness off the surface, and the section.
    let (half, section) = if strap {
        let thick = cord * 0.42;
        (thick, strap_section(thick, (pitch * 0.46).min(cord * 3.5).max(cord * 0.8)))
    } else {
        (cord, regular_polygon(6, cord, 0.0))
    };
    let t_of = |u: f32| span.0 + (span.1 - span.0) * u;
    let steps = ((turns * 16.0) as usize).max(64);
    let strands: &[f32] = if wrap.cross { &[1.0, -1.0] } else { &[1.0] };
    for &sign in strands {
        let mut points = Vec::with_capacity(steps + 1);
        let mut outward = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let f = i as f32 / steps as f32;
            let u = c0 + (c1 - c0) * f;
            let frame = haft.frame(t_of(u));
            let theta = sign * turns * TAU * f;
            let radial = vec3::add(vec3::scale(frame.normal, theta.cos()), vec3::scale(frame.binormal, theta.sin()));
            // Cross-wound strands ride over and under each other at alternate crossings.
            let weave = if wrap.cross { half * 0.9 * (1.0 + sign * theta.cos()) } else { 0.0 };
            let r = grip_radius(haft, grip, span, u) + half * 0.95 + weave;
            points.push(vec3::add(frame.origin, vec3::scale(radial, r)));
            outward.push(radial);
        }
        let frames = wound_frames(&points, &outward);
        out.append(&sweep_frames(&frames, &section, &SweepOptions::default(), |_| 1.0));
    }
    if wrap.bind {
        let binding_cord = if strap { cord * 0.5 } else { cord * 0.8 };
        let lift = half * 2.0 + if wrap.cross { half * 1.8 } else { 0.0 };
        let wraps = 3.0;
        let reach = wraps * binding_cord * 2.1;
        // Bindings straddle the wrap's ends but stay on the span.
        let half_reach = ((reach * 0.5 + binding_cord * 1.2) / span_len).min(0.5);
        for u in [c0.max(half_reach), c1.min(1.0 - half_reach)] {
            let t = t_of(u);
            let r = grip_radius(haft, grip, span, u) + lift + binding_cord;
            let n = 48;
            let path: Vec<V3> = (0..=n)
                .map(|j| {
                    let f = j as f32 / n as f32;
                    let frame = haft.frame(t + (f - 0.5) * reach / haft.length());
                    let a = wraps * TAU * f;
                    vec3::add(frame.origin, vec3::add(vec3::scale(frame.normal, r * a.cos()), vec3::scale(frame.binormal, r * a.sin())))
                })
                .collect();
            out.append(&tube(&path, 6, &SweepOptions::default(), |_| binding_cord));
        }
    }
    out
}

/// Rivets through the grip (a knife's or sword's scales pinned to the tang): `grip.rivets`
/// domed heads spaced along the span, one on each side where the tang's flats face (the
/// haft frame's binormal, ±Z on a straight haft).
pub fn rivets_mesh(haft: &Haft, grip: &Grip, span: (f32, f32)) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    let n = grip.rivets;
    if !grip.on || n == 0 {
        return out;
    }
    for k in 0..n {
        let u = (k as f32 + 0.7) / (n as f32 + 0.4);
        let surface = grip_radius(haft, grip, span, u);
        let head = (surface * 0.22).clamp(0.012, 0.035);
        let control = [[head * 1.05, -0.012], [head, 0.002], [head * 0.8, head * 0.35], [0.0, head * 0.5]];
        let dome = lathe(&spline_profile(&control, 10), &LatheOptions::new(16, Shading::SMOOTH));
        let frame = haft.frame(span.0 + (span.1 - span.0) * u);
        for side in [1.0f32, -1.0] {
            let out_dir = vec3::scale(frame.binormal, side);
            let mut m = dome.clone();
            m.place(vec3::add(frame.origin, vec3::scale(out_dir, surface * 0.98)), out_dir);
            out.append(&m);
        }
    }
    out
}

/// The radius the pommel fits over: the grip, if it reaches the butt, else the shaft.
pub fn butt_radius(haft: &Haft, grip: &Grip, span: (f32, f32)) -> f32 {
    if grip.on && span.0 < 0.03 { haft.radius(0.0) * grip.thickness } else { haft.outer_radius(0.0) }
}

/// A pommel's body and its stones, built upright: local +Y points away from the shaft, the
/// shaft end is at y = 0, and nothing reaches past y = `pommel.length`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PommelMeshes {
    pub body: MeshBuilder,
    pub stones: MeshBuilder,
}

impl PommelMeshes {
    /// Sets both meshes onto the butt of `haft`, pointing away from the shaft.
    pub fn placed_on(mut self, haft: &Haft) -> PommelMeshes {
        let (origin, down) = (haft.point(0.0), vec3::neg(haft.tangent(0.0)));
        self.body.place(origin, down);
        self.stones.place(origin, down);
        self
    }
}

/// Builds `pommel` over a butt of radius `butt` (see [`butt_radius`]). Stones take the
/// pommel's own stone and are seated: an end stone sits in a flat seat cut into the end, a
/// ring of cabochons sits round the widest point facing out.
pub fn pommel_mesh(pommel: &Pommel, butt: f32) -> PommelMeshes {
    if pommel.style == PommelStyle::Wheel {
        return wheel_pommel(pommel, butt);
    }
    let rs = butt;
    let h = pommel.length;
    let rp = pommel.radius.max(rs * 1.05);
    // Sunk a little into the shaft so there is no seam.
    let z = -0.04;
    let control: Vec<[f32; 2]> = match pommel.style {
        PommelStyle::Ball => vec![[rs, z], [rs * 1.05, h * 0.08], [rp * 0.75, h * 0.25], [rp, h * 0.55], [rp * 0.7, h * 0.88], [0.0, h]],
        PommelStyle::Ferrule => vec![
            [rs * 1.06, z],
            [(rs * 1.1).max(rp * 0.6), h * 0.45],
            [rs.max(rp * 0.55) * 0.95, h * 0.82],
            [rs * 0.45, h * 0.98],
            [0.0, h],
        ],
        PommelStyle::Flared => vec![[rs, z], [rs * 1.1, h * 0.3], [rp * 0.55, h * 0.6], [rp, h * 0.85], [rp * 0.92, h * 0.97], [0.0, h]],
        PommelStyle::Disc => vec![[rs, z], [rs * 1.2, h * 0.15], [rp, h * 0.38], [rp, h * 0.62], [rs * 1.2, h * 0.85], [0.0, h]],
        PommelStyle::Spike => vec![[rs, z], [rp * 0.7, h * 0.12], [rp * 0.5, h * 0.3], [rp * 0.15, h * 0.8], [0.0, h]],
        PommelStyle::ScentStopper => {
            vec![[rs, z], [rs * 1.15, h * 0.08], [rp * 0.55, h * 0.22], [rp, h * 0.5], [rp * 0.62, h * 0.74], [rp * 0.78, h * 0.86], [0.0, h]]
        }
        PommelStyle::Wheel => unreachable!(),
    };
    let mut profile = spline_profile(&control, 28);
    let (sides, shading) = if pommel.style == PommelStyle::ScentStopper {
        (pommel.facets.clamp(6, 12) as usize, Shading::FLAT)
    } else {
        turned(pommel.facets)
    };
    let mut stones = MeshBuilder::new();
    let size = pommel.gem_size;
    match pommel.gem {
        PommelGem::EndStone => {
            let cut = cut_for(pommel.gem_stone, GemCut::Brilliant);
            let (_, up) = stone_reach(cut, size);
            // Cut a flat seat into the end, wide enough for the girdle, low enough that the
            // table stays inside the pommel's length.
            let seat_y = profile
                .iter()
                .rev()
                .find(|p| p[0] >= size * 1.15)
                .map_or(h - up, |p| p[1])
                .min(h - up - size * 0.02);
            if let Some(cut_at) = profile.iter().position(|p| p[1] >= seat_y && p[0] < size * 1.15) {
                let r_seat = radius_on(&profile, seat_y).max(size * 1.1);
                profile.truncate(cut_at);
                profile.push([r_seat, seat_y]);
            }
            let mut stone = cut_stone(cut, size);
            stone.translate([0.0, seat_y + size * 0.02, 0.0]);
            stones.append(&stone);
        }
        PommelGem::Ring => {
            let widest = profile.iter().copied().fold([0.0f32, 0.0], |best, p| if p[0] > best[0] { p } else { best });
            let cut = cut_for(pommel.gem_stone, GemCut::Cabochon);
            let n = pommel.gem_count.max(1);
            for i in 0..n {
                let a = std::f32::consts::TAU * i as f32 / n as f32;
                let dir = [a.cos(), 0.0, -a.sin()];
                let mut stone = cut_stone(cut, size);
                stone.place(vec3::add([0.0, widest[1], 0.0], vec3::scale(dir, widest[0] * 0.97)), dir);
                stones.append(&stone);
            }
        }
        PommelGem::None => {}
    }
    let body = lathe(&profile, &LatheOptions { facets: sides, shading, cap_start: true, cap_end: true, phase: 0.0 });
    PommelMeshes { body, stones }
}

/// Radius of a `[radius, y]` profile at height `y` (the first span that contains it).
pub fn radius_on(profile: &[[f32; 2]], y: f32) -> f32 {
    for w in profile.windows(2) {
        let (a, b) = (w[0], w[1]);
        if (y >= a[1] && y <= b[1]) || (y <= a[1] && y >= b[1]) {
            let t = if (b[1] - a[1]).abs() < 1e-9 { 0.0 } else { (y - a[1]) / (b[1] - a[1]) };
            return a[0] + (b[0] - a[0]) * t;
        }
    }
    0.0
}

/// A wheel pommel: a neck, a thick disc on edge with raised bosses on both faces, and a peen
/// block at the end, all within the pommel's length.
fn wheel_pommel(pommel: &Pommel, rs: f32) -> PommelMeshes {
    let h = pommel.length;
    let peen = (h * 0.12).min(0.08);
    let rw = pommel.radius.max(rs * 1.05).min((h - peen - h * 0.08) * 0.5);
    let center = h - peen - rw;
    let tw = (rw * 0.55).max(0.06);
    let (sides, shading) = turned(pommel.facets);
    let mut body = MeshBuilder::new();
    let neck = [[rs, -0.04], [rs * 1.12, 0.03], [rs * 0.85, center - rw * 0.7], [rs * 0.7, center]];
    body.append(&lathe(&spline_profile(&neck, 12), &LatheOptions { facets: 24, shading: Shading::SMOOTH, ..Default::default() }));
    let disc = [
        [0.0, -tw * 0.5 - tw * 0.18],
        [rw * 0.32, -tw * 0.5 - tw * 0.18],
        [rw * 0.4, -tw * 0.5],
        [rw * 0.72, -tw * 0.5],
        [rw, -tw * 0.22],
        [rw, tw * 0.22],
        [rw * 0.72, tw * 0.5],
        [rw * 0.4, tw * 0.5],
        [rw * 0.32, tw * 0.5 + tw * 0.18],
        [0.0, tw * 0.5 + tw * 0.18],
    ];
    // Hard steps between the rim, the faces and the bosses.
    let mut wheel = lathe(&disc, &LatheOptions { facets: sides.max(16), shading: Shading { smooth_around: shading.smooth_around, smooth_along: false }, ..Default::default() });
    wheel.rotate_x(std::f32::consts::FRAC_PI_2).translate([0.0, center, 0.0]);
    body.append(&wheel);
    let block = [[rs * 0.7, center + rw - 0.02], [rs * 0.75, center + rw + peen * 0.4], [rs * 0.45, center + rw + peen * 0.85], [0.0, h]];
    body.append(&lathe(&spline_profile(&block, 10), &LatheOptions { facets: 10, shading: Shading::FACETED, ..Default::default() }));

    let mut stones = MeshBuilder::new();
    let face = tw * 0.5 + tw * 0.18;
    let cut = cut_for(pommel.gem_stone, GemCut::Cabochon);
    let mut put = |x: f32, y: f32, size: f32, face_z: f32| {
        for side in [1.0f32, -1.0] {
            let mut stone = cut_stone(cut, size);
            stone.place([x, y, side * face_z], [0.0, 0.0, side]);
            stones.append(&stone);
        }
    };
    match pommel.gem {
        PommelGem::EndStone => put(0.0, center, (rw * 0.3).min(pommel.gem_size * 1.5), face - 0.005),
        PommelGem::Ring => {
            let n = pommel.gem_count.max(1);
            for i in 0..n {
                let a = std::f32::consts::TAU * i as f32 / n as f32;
                put(a.cos() * rw * 0.62, center + a.sin() * rw * 0.62, pommel.gem_size.min(rw * 0.18), tw * 0.5 - 0.005);
            }
        }
        PommelGem::None => {}
    }
    PommelMeshes { body, stones }
}
