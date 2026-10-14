//! Geometry for bow limbs and crossbow prods, the string looped over their tips, and a cord
//! wound round the handle. Bows and crossbows both build here from the
//! [`crate::sections::ranged`] sections.
//!
//! A [`Stave`] is both limbs tip to tip through the handle, laid out in a [`LimbPlane`]: the
//! family says where the handle sits, which way the limbs run and which way the string lies.
//! Everything else (nocks, string loops, the string itself, a grip wrap) is placed from it.
//!
//! From the prototype's `limbProfile`, `buildLimbs` and `buildString` in
//! `reference/spline_armory_engine.js`, rebuilt at higher fidelity: limbs have a real
//! cross-section (a longbow's D, flat limbs elsewhere) whose width and thickness taper
//! separately, composite tips are stiff levers rather than a bend, horn nocks carry a
//! string groove, and the string ends in loops round the tips instead of stopping in the air.

use crate::build::hafted::smoothstep;
use crate::geom::{
    CatmullRom, Frame, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, lathe, resample_by_length, spline_profile,
    sweep_scaled, tube, vec3,
};
use crate::geom::polygon::P2;
use crate::sections::ranged::{Bowstring, LimbShape, Limbs, NockStyle};

/// Points along the stave's centreline, evenly spaced by arc length.
const STAVE_SAMPLES: usize = 241;

/// Where a stave lies: `origin` is the middle of the handle, the limbs run along `along`
/// (the first tip at `-along`, the second at `+along`) and bend toward `toward_string`.
/// Width runs across both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimbPlane {
    pub origin: V3,
    pub along: V3,
    pub toward_string: V3,
}

impl LimbPlane {
    /// The point `a` along the limbs and `b` toward the string.
    pub fn at(&self, a: f32, b: f32) -> V3 {
        vec3::add(self.origin, vec3::add(vec3::scale(self.along, a), vec3::scale(self.toward_string, b)))
    }

    /// Side to side across the limbs.
    pub fn width_axis(&self) -> V3 {
        vec3::normalize_or(vec3::cross(self.toward_string, self.along), vec3::Z)
    }
}

/// The thickened middle of a stave: a bow's handle, the middle of a prod where it passes
/// through the stock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimbGrip {
    /// Half the grip's length, as a fraction of half the stave.
    pub half_span: f32,
    /// Full width and thickness at the grip.
    pub width: f32,
    pub thickness: f32,
}

/// One half-limb's side profile: `[along, bend]` from the handle (`u` = 0) out to the tip,
/// bend measured toward the string. `half_length` is the reach of one limb, `bend` how far
/// the tips sit toward the string (the brace height at rest).
pub fn limb_profile(limbs: &Limbs, half_length: f32, bend: f32) -> Vec<[f32; 2]> {
    let (h, t, rc) = (half_length, bend, limbs.recurve);
    const K: usize = 48;
    let mut out = Vec::with_capacity(K + 8);
    if limbs.shape == LimbShape::Composite {
        // The working limb bends evenly, then the siyah runs straight on, angled forward.
        let u0 = 1.0 - limbs.siyah.max(0.02);
        for k in 0..=K {
            let u = k as f32 / K as f32 * u0;
            out.push([h * u, t * 0.9 * (u / u0).powf(1.6)]);
        }
        let (last, prev) = (out[K], out[K - 1]);
        let d = [last[0] - prev[0], last[1] - prev[1]];
        let len = d[0].hypot(d[1]).max(1e-6);
        let d = [d[0] / len, d[1] / len];
        let turn = 0.35 + rc * 0.6;
        let (s, c) = turn.sin_cos();
        let lever = [d[0] * c + d[1] * s, -d[0] * s + d[1] * c];
        let reach = h * limbs.siyah * 1.1;
        for k in 1..=6 {
            let f = k as f32 / 6.0;
            out.push([last[0] + lever[0] * reach * f, last[1] + lever[1] * reach * f]);
        }
        return out;
    }
    for k in 0..=K {
        let u = k as f32 / K as f32;
        let e = smoothstep(0.6, 1.0, u);
        let mut a = h * u;
        let b = match limbs.shape {
            LimbShape::Recurve => {
                a *= 1.0 - 0.04 * rc * e;
                t * u.powf(1.7) - rc * h * 0.22 * e * e
            }
            LimbShape::Reflex => {
                // Deflexed at the handle, eased in so the two limbs meet without a kink.
                t * u.powf(2.4) - 0.05 * h * (u * std::f32::consts::PI).sin() * smoothstep(0.0, 0.25, u) - rc * h * 0.15 * smoothstep(0.65, 1.0, u).powi(2)
            }
            _ => t * (1.0 - (u * std::f32::consts::FRAC_PI_2).cos()) - rc * h * 0.12 * e * e,
        };
        out.push([a, b]);
    }
    out
}

/// The unit cross-section of a limb in the frame's (normal, binormal) plane, x toward the
/// string (the belly), spanning -1..=1 both ways. A longbow is D-shaped: flat back, round
/// belly. Other limbs are flat with rounded edges.
fn limb_section(shape: LimbShape) -> Vec<P2> {
    const N: usize = 28;
    let raw: Vec<P2> = (0..N)
        .map(|k| {
            let (s, c) = (std::f32::consts::TAU * k as f32 / N as f32).sin_cos();
            match shape {
                LimbShape::Longbow if c >= 0.0 => [c, s.signum() * s.abs().powf(0.85)],
                LimbShape::Longbow => [-0.35 * c.abs().powf(0.4), s.signum() * s.abs().powf(0.3)],
                _ => [c.signum() * c.abs().powf(0.7), s.signum() * s.abs().powf(0.35)],
            }
        })
        .collect();
    let (lo, hi) = raw.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[0]), hi.max(p[0])));
    raw.iter().map(|p| [(p[0] - lo) / (hi - lo) * 2.0 - 1.0, p[1]]).collect()
}

/// Where the string loops onto a tip: the loop's frame (tangent pointing out to the tip,
/// normal toward the string) and the half extents it wraps round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StringAnchor {
    pub frame: Frame,
    /// Half extents toward the string and across the width.
    pub half: [f32; 2],
}

impl StringAnchor {
    /// Where a string of radius `cord` leaves the loop toward the string side.
    pub fn belly_point(&self, cord: f32) -> V3 {
        vec3::add(self.frame.origin, vec3::scale(self.frame.normal, self.half[0] + cord * 2.0))
    }
}

/// Both limbs, tip to tip through the handle. `t` runs 0 (first tip) to 1 (second tip) by
/// arc length.
#[derive(Clone, Debug, PartialEq)]
pub struct Stave {
    limbs: Limbs,
    plane: LimbPlane,
    grip: LimbGrip,
    path: Vec<V3>,
    tangents: Vec<V3>,
    length: f32,
    section: Vec<P2>,
}

impl Stave {
    /// Lays out `limbs` in `plane`, each limb reaching `half_length` and bending `bend`
    /// toward the string.
    pub fn new(limbs: &Limbs, plane: LimbPlane, grip: LimbGrip, half_length: f32, bend: f32) -> Stave {
        let half = limb_profile(limbs, half_length, bend);
        let mut control: Vec<V3> = half.iter().skip(1).rev().map(|&[a, b]| plane.at(-a, b)).collect();
        control.extend(half.iter().map(|&[a, b]| plane.at(a, b)));
        let sampled = CatmullRom::centripetal(control).sample(6);
        let path = resample_by_length(&sampled, STAVE_SAMPLES, false);
        let length = crate::geom::polyline_length(&path, false);
        let n = path.len();
        let tangents = (0..n)
            .map(|i| vec3::normalize_or(vec3::sub(path[(i + 1).min(n - 1)], path[i.saturating_sub(1)]), plane.along))
            .collect();
        Stave { limbs: *limbs, plane, grip, path, tangents, length, section: limb_section(limbs.shape) }
    }

    pub fn length(&self) -> f32 {
        self.length
    }

    pub fn plane(&self) -> LimbPlane {
        self.plane
    }

    fn sample(&self, t: f32) -> (usize, f32) {
        let x = t.clamp(0.0, 1.0) * (self.path.len() - 1) as f32;
        let i = (x.floor() as usize).min(self.path.len() - 2);
        (i, x - i as f32)
    }

    pub fn point(&self, t: f32) -> V3 {
        let (i, f) = self.sample(t);
        vec3::lerp(self.path[i], self.path[i + 1], f)
    }

    /// The frame at `t`: tangent toward the second tip, normal toward the string side of the
    /// limb (the belly), binormal across the width.
    pub fn frame(&self, t: f32) -> Frame {
        let (i, f) = self.sample(t);
        let tangent = vec3::normalize_or(vec3::lerp(self.tangents[i], self.tangents[i + 1], f), self.plane.along);
        let normal = vec3::normalize_or(vec3::cross(tangent, self.plane.width_axis()), self.plane.toward_string);
        Frame { origin: self.point(t), tangent, normal, binormal: vec3::cross(tangent, normal) }
    }

    /// Half thickness (toward the string) and half width at `t`.
    pub fn half_extents(&self, t: f32) -> [f32; 2] {
        let l = &self.limbs;
        let u = (2.0 * t - 1.0).abs();
        let mut w = l.width * (1.0 - l.taper * 0.7 * u.powf(1.3));
        let mut th = l.thickness * (1.0 - l.taper * 0.55 * u);
        if l.shape == LimbShape::Composite {
            // The siyah: a stiff lever, narrower and deeper than the working limb.
            let k = smoothstep(1.0 - l.siyah - 0.015, 1.0 - l.siyah + 0.015, u);
            w *= 1.0 - 0.3 * k;
            th *= 1.0 + 0.3 * k;
        }
        let g = 1.0 - smoothstep(self.grip.half_span * 0.8, self.grip.half_span * 1.8, u);
        w += (self.grip.width - w) * g;
        th += (self.grip.thickness - th) * g;
        [th * 0.5, w * 0.5]
    }

    /// The grip's span as `(t0, t1)`, `reach` times its half length either side of the middle.
    pub fn grip_span(&self, reach: f32) -> (f32, f32) {
        let half = (self.grip.half_span * reach * 0.5).min(0.45);
        (0.5 - half, 0.5 + half)
    }

    /// The stave from `t0` to `t1`, its section scaled by `inflate`. Ends at the tips are
    /// capped; ends inside the stave are left open to meet the next piece, unless the piece
    /// is inflated into a sleeve, whose ends show.
    pub fn mesh(&self, t0: f32, t1: f32, inflate: f32) -> MeshBuilder {
        let rows = (((t1 - t0) * (STAVE_SAMPLES - 1) as f32).ceil() as usize).max(2) + 1;
        let ts: Vec<f32> = (0..rows).map(|i| t0 + (t1 - t0) * i as f32 / (rows - 1) as f32).collect();
        let frames: Vec<Frame> = ts.iter().map(|&t| self.frame(t)).collect();
        let sleeve = inflate > 1.0;
        let options = SweepOptions { cap_start: sleeve || t0 <= 1e-4, cap_end: sleeve || t1 >= 1.0 - 1e-4, ..Default::default() };
        sweep_scaled(&frames, &self.section, &options, |f| {
            let t = t0 + (t1 - t0) * f;
            self.half_extents(t).map(|k| k * inflate)
        })
    }

    /// Each tip's frame, tangent pointing out past the tip.
    pub fn tips(&self) -> [Frame; 2] {
        let mut first = self.frame(0.0);
        first.tangent = vec3::neg(first.tangent);
        first.binormal = vec3::neg(first.binormal);
        [first, self.frame(1.0)]
    }

    /// Length of a nock of `style`.
    fn nock_length(style: NockStyle) -> f32 {
        match style {
            NockStyle::Horn => 0.3,
            NockStyle::Cap => 0.16,
            NockStyle::None => 0.0,
        }
    }

    /// The radius a nock turns to over a tip: wide enough to sheathe the limb's end.
    fn nock_radius(&self) -> f32 {
        let [a, b] = self.half_extents(1.0);
        a.max(b) * 1.05
    }

    /// Turned tips over both ends, or nothing for self nocks.
    pub fn nocks_mesh(&self) -> MeshBuilder {
        let style = self.limbs.nocks;
        let len = Self::nock_length(style);
        let r0 = self.nock_radius();
        let control: Vec<[f32; 2]> = match style {
            NockStyle::None => return MeshBuilder::new(),
            NockStyle::Horn => vec![
                [r0, -0.05],
                [r0 * 1.2, len * 0.22],
                [r0 * 0.92, len * 0.35],
                [r0 * 1.12, len * 0.47],
                [r0 * 0.85, len * 0.75],
                [r0 * 0.3, len * 0.96],
                [0.0, len],
            ],
            NockStyle::Cap => vec![[r0 * 1.12, -0.04], [r0 * 1.14, len * 0.3], [r0 * 1.1, len * 0.55], [r0 * 0.4, len * 0.74], [0.0, len * 0.76]],
        };
        let profile = spline_profile(&control, 24);
        let one = lathe(&profile, &LatheOptions { facets: 18, shading: Shading::SMOOTH, ..Default::default() });
        let mut out = MeshBuilder::new();
        for tip in self.tips() {
            let mut m = one.clone();
            m.place(vec3::sub(tip.origin, vec3::scale(tip.tangent, len * 0.55)), tip.tangent);
            out.append(&m);
        }
        out
    }

    /// Where the string loops on at each tip: in a horn nock's groove, round the shoulder of
    /// a cap, or in self nocks cut a little way in from the tip.
    pub fn string_anchors(&self) -> [StringAnchor; 2] {
        let style = self.limbs.nocks;
        let len = Self::nock_length(style);
        let back = match style {
            NockStyle::Horn => len * 0.55 - len * 0.35,
            NockStyle::Cap => len * 0.55 - len * 0.2,
            NockStyle::None => 0.1,
        };
        let groove = match style {
            NockStyle::Horn => Some(self.nock_radius() * 0.95),
            NockStyle::Cap => Some(self.nock_radius() * 1.14),
            NockStyle::None => None,
        };
        let dt = back / self.length;
        let ts = [dt, 1.0 - dt];
        let tips = self.tips();
        [0, 1].map(|i| {
            let mut frame = self.frame(ts[i]);
            frame.tangent = tips[i].tangent;
            frame.binormal = vec3::cross(frame.tangent, frame.normal);
            let half = match groove {
                Some(r) => [r, r],
                // A self nock is cut into the limb's edges: the loop bites in a little.
                None => {
                    let [a, b] = self.half_extents(ts[i]);
                    [a, b * 0.9]
                }
            };
            StringAnchor { frame, half }
        })
    }
}

impl Stave {
    /// Where a string of radius `cord` runs from the first anchor to the second: the
    /// string-side hull of the limbs' bellies, so it lies along recurved tips and leaves
    /// them at the string bridges, pulled out to any `extra` points (the nut of a spanned
    /// crossbow, a nocking point drawn back). Points run in order along the limbs.
    pub fn string_path(&self, cord: f32, extra: &[V3]) -> Vec<V3> {
        let anchors = self.string_anchors();
        let (t0, t1) = (self.closest_t(anchors[0].frame.origin), self.closest_t(anchors[1].frame.origin));
        let n = (((t1 - t0) * (STAVE_SAMPLES - 1) as f32) as usize).max(2);
        let mut points = vec![anchors[0].belly_point(cord)];
        points.extend((1..n).map(|i| {
            let t = t0 + (t1 - t0) * i as f32 / n as f32;
            let f = self.frame(t);
            vec3::add(f.origin, vec3::scale(f.normal, self.half_extents(t)[0] + cord))
        }));
        points.push(anchors[1].belly_point(cord));
        points.extend_from_slice(extra);
        let plane = self.plane;
        let key = |p: V3| {
            let d = vec3::sub(p, plane.origin);
            [vec3::dot(d, plane.along), vec3::dot(d, plane.toward_string)]
        };
        let mut keyed: Vec<([f32; 2], V3)> = points.iter().map(|&p| (key(p), p)).collect();
        keyed.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
        // Monotone chain, keeping the side toward the string.
        let mut hull: Vec<([f32; 2], V3)> = Vec::new();
        for q in keyed {
            while hull.len() >= 2 {
                let (o, a) = (hull[hull.len() - 2].0, hull[hull.len() - 1].0);
                let turn = (a[0] - o[0]) * (q.0[1] - o[1]) - (a[1] - o[1]) * (q.0[0] - o[0]);
                if turn >= 0.0 { hull.pop(); } else { break; }
            }
            hull.push(q);
        }
        hull.into_iter().map(|(_, p)| p).collect()
    }

    /// The `t` of the centreline sample nearest `p`.
    fn closest_t(&self, p: V3) -> f32 {
        let i = (0..self.path.len()).min_by(|&a, &b| vec3::distance(self.path[a], p).total_cmp(&vec3::distance(self.path[b], p))).unwrap_or(0);
        i as f32 / (self.path.len() - 1) as f32
    }
}

/// A closed loop of `cord` round an anchor's section, `turns` times side by side.
fn string_loop(anchor: &StringAnchor, cord: f32, turns: usize) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    let f = anchor.frame;
    for k in 0..turns {
        let shift = (k as f32 - (turns - 1) as f32 * 0.5) * cord * 1.9;
        let centre = vec3::add(f.origin, vec3::scale(f.tangent, shift));
        let path: Vec<V3> = (0..32)
            .map(|j| {
                let a = std::f32::consts::TAU * j as f32 / 32.0;
                let (s, c) = a.sin_cos();
                // Square the loop off a little so it hugs a flat limb.
                let (c, s) = (c.signum() * c.abs().powf(0.8), s.signum() * s.abs().powf(0.8));
                vec3::add(centre, vec3::add(vec3::scale(f.normal, c * (anchor.half[0] + cord)), vec3::scale(f.binormal, s * (anchor.half[1] + cord))))
            })
            .collect();
        out.append(&tube(&path, 6, &SweepOptions { up: Some(f.tangent), ..SweepOptions::closed_loop() }, |_| cord));
    }
    out
}

/// The string: a loop round each anchor and runs along `path` (from the first anchor's
/// belly point to the second's, as [`Stave::string_path`] gives it). Gentle bends, where
/// the string lies along a recurved tip, are swept smoothly; sharp ones (a string bridge,
/// the nut of a spanned crossbow) get a small knuckle.
pub fn bowstring_mesh(anchors: &[StringAnchor; 2], path: &[V3], string: &Bowstring) -> MeshBuilder {
    let cord = string.thickness;
    let mut out = MeshBuilder::new();
    let mut points: Vec<V3> = Vec::with_capacity(path.len());
    for &p in path {
        if points.last().is_none_or(|&q| vec3::distance(p, q) > cord * 0.5) {
            points.push(p);
        }
    }
    if points.len() < 2 {
        return out;
    }
    // Split into runs at sharp corners.
    let mut runs: Vec<Vec<V3>> = vec![vec![points[0]]];
    for i in 1..points.len() {
        runs.last_mut().unwrap().push(points[i]);
        if i + 1 < points.len() {
            let a = vec3::normalize_or(vec3::sub(points[i], points[i - 1]), vec3::Y);
            let b = vec3::normalize_or(vec3::sub(points[i + 1], points[i]), vec3::Y);
            if vec3::dot(a, b) < 0.985 {
                let mut ball = crate::geom::uv_sphere(cord, 8, 4);
                ball.translate(points[i]);
                out.append(&ball);
                runs.push(vec![points[i]]);
            }
        }
    }
    for run in &runs {
        out.append(&tube(run, 8, &SweepOptions::default(), |_| cord));
    }
    for a in anchors {
        out.append(&string_loop(a, cord, 2));
    }
    out
}

/// The point on `path` (a string path) at `a` along `plane`, or the nearest end.
pub fn point_along(path: &[V3], plane: &LimbPlane, a: f32) -> V3 {
    let along = |p: V3| vec3::dot(vec3::sub(p, plane.origin), plane.along);
    for w in path.windows(2) {
        let (a0, a1) = (along(w[0]), along(w[1]));
        if (a0 - a) * (a1 - a) <= 0.0 && (a1 - a0).abs() > 1e-9 {
            return vec3::lerp(w[0], w[1], (a - a0) / (a1 - a0));
        }
    }
    let first = path.first().copied().unwrap_or(plane.origin);
    let last = path.last().copied().unwrap_or(plane.origin);
    if (along(first) - a).abs() < (along(last) - a).abs() { first } else { last }
}

/// A serving: tight thread wound over the string for `reach` either side of `nock`,
/// following `path` (the string path the nock lies on).
pub fn serving_mesh(path: &[V3], nock: V3, reach: f32, string: &Bowstring) -> MeshBuilder {
    if path.len() < 2 {
        return MeshBuilder::new();
    }
    // Arc length along the path, and where the nock falls on it.
    let mut s = vec![0.0f32; path.len()];
    for i in 1..path.len() {
        s[i] = s[i - 1] + vec3::distance(path[i - 1], path[i]);
    }
    let (mut best, mut at) = (f32::MAX, 0.0);
    for i in 0..path.len() - 1 {
        let (a, b) = (path[i], path[i + 1]);
        let ab = vec3::sub(b, a);
        let len2 = vec3::dot(ab, ab).max(1e-12);
        let f = (vec3::dot(vec3::sub(nock, a), ab) / len2).clamp(0.0, 1.0);
        let d = vec3::distance(nock, vec3::lerp(a, b, f));
        if d < best {
            best = d;
            at = s[i] + f * (s[i + 1] - s[i]);
        }
    }
    let total = s[s.len() - 1];
    let reach = reach.min(total * 0.3);
    let (from, to) = ((at - reach).max(0.0), (at + reach).min(total));
    let point_at = |x: f32| {
        let i = s.partition_point(|&v| v < x).clamp(1, path.len() - 1);
        let span = (s[i] - s[i - 1]).max(1e-9);
        vec3::lerp(path[i - 1], path[i], ((x - s[i - 1]) / span).clamp(0.0, 1.0))
    };
    let samples = 24;
    let run: Vec<V3> = (0..=samples).map(|k| point_at(from + (to - from) * k as f32 / samples as f32)).collect();
    let r = string.thickness * 1.6;
    tube(&run, 8, &SweepOptions::default(), |t| r * (1.0 - 0.25 * (smoothstep(0.0, 0.08, t.min(1.0 - t)) - 1.0).abs()))
}

/// A strap section, `thick` across the frame normal and `wide` across the binormal, with
/// rounded edges.
fn strap_section(thick: f32, wide: f32) -> Vec<P2> {
    (0..12)
        .map(|k| {
            let (s, c) = (std::f32::consts::TAU * k as f32 / 12.0).sin_cos();
            [thick * c.signum() * c.abs().powf(0.6), wide * s.signum() * s.abs().powf(0.3)]
        })
        .collect()
}

/// A wrap wound `turns` times round the stave from `t0` to `t1`, over a surface inflated by
/// `inflate` (a riser sleeve), with a binding of tight turns at each end. A `strap` (a
/// leather thong) lies flat, as wide as the pitch so its edges meet; otherwise a round cord
/// of radius `cord`, thinned if needed so neighbouring turns do not overlap.
pub fn stave_wrap_mesh(stave: &Stave, t0: f32, t1: f32, inflate: f32, turns: u32, cord: f32, strap: bool) -> MeshBuilder {
    use std::f32::consts::TAU;
    let mut out = MeshBuilder::new();
    let around = |t: f32, theta: f32, lift: f32| {
        let f = stave.frame(t);
        let [a, b] = stave.half_extents(t);
        let (s, c) = theta.sin_cos();
        let (c, s) = (c.signum() * c.abs().powf(0.7), s.signum() * s.abs().powf(0.7));
        let p = vec3::add(f.origin, vec3::add(vec3::scale(f.normal, c * (a * inflate + lift)), vec3::scale(f.binormal, s * (b * inflate + lift))));
        (p, f.origin)
    };
    let turns = turns.max(1);
    let pitch = (t1 - t0) * stave.length() / turns as f32;
    let (lift, section) = if strap {
        let thick = cord * 0.4;
        (thick, strap_section(thick, pitch * 0.5))
    } else {
        let cord = cord.min(pitch * 0.5);
        (cord * 0.9, crate::geom::regular_polygon(6, cord, 0.0))
    };
    let steps = (turns as usize * 32).max(96);
    let wound: Vec<(V3, V3)> = (0..=steps)
        .map(|i| {
            let f = i as f32 / steps as f32;
            around(t0 + (t1 - t0) * f, turns as f32 * TAU * f, lift)
        })
        .collect();
    // Frames whose normal points out from the stave, so a strap lies flat on it.
    let n = wound.len();
    let frames: Vec<Frame> = (0..n)
        .map(|i| {
            let (p, centre) = wound[i];
            let tangent = vec3::normalize_or(vec3::sub(wound[(i + 1).min(n - 1)].0, wound[i.saturating_sub(1)].0), stave.plane.along);
            let out = vec3::sub(p, centre);
            let normal = vec3::normalize_or(vec3::sub(out, vec3::scale(tangent, vec3::dot(out, tangent))), vec3::any_perpendicular(tangent));
            Frame { origin: p, tangent, normal, binormal: vec3::cross(tangent, normal) }
        })
        .collect();
    out.append(&crate::geom::sweep_frames(&frames, &section, &SweepOptions::default(), |_| 1.0));
    let tie = if strap { cord * 0.5 } else { cord * 0.8 };
    for t in [t0, t1] {
        let n = 3 * 32;
        let dt = tie * 2.2 / stave.length();
        let ring: Vec<V3> = (0..=n)
            .map(|j| {
                let f = j as f32 / n as f32;
                around(t + (f - 0.5) * 3.0 * dt, 3.0 * TAU * f, lift * 2.0 + tie).0
            })
            .collect();
        out.append(&tube(&ring, 6, &SweepOptions::default(), |_| tie));
    }
    out
}
