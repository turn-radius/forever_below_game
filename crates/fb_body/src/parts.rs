//! The parts a race can turn on beyond the folk's own: horns, legs that stand on the toes with
//! cloven hooves, a jointed tail, and the bull's muzzle. A race turns a part on by giving its row
//! in `data/races.json` the field (`horns`, `legs`, `tail_shape`, `muzzle`); the body, the rig,
//! the skin weights, the gait and the fitting then handle it with no race named. A minotaur
//! turns on all four; a faun would turn on `horns` and `legs` (a lower `hock`, a shorter
//! `pastern`) and a demon `tail_shape` with a spade end.
//!
//! - [`Traits`]: what a seed rolls within a row's ranges: the horns' length, curve, spread,
//!   rise, wear and a break, a nose ring, the mane, a coat's marking, the tail's tuft, the hooves.
//! - [`horn`]: a horn swept along a curve ([`HornSpec`]), a base and a darker tip.
//! - [`hoof`] and [`digitigrade_foot`]: the pastern and cloven hoof a leg stands on.
//! - [`Loft`]: a surface lofted between rings along a line (the muzzle).
//!
//! Space is the body's: 1 unit = 10 cm, soles on y = 0, face +Z, the wearer's left +X. Every
//! piece is a pure function of its numbers; the rolls use [`procgen_rng::Mulberry32`] only.
//! The shapes and numbers began as the first minotaur's, built as a creature (our own).

use std::f32::consts::{FRAC_PI_2, TAU};

use armory::geom::vec3::{self, V3};
use armory::geom::{CatmullRom, GridOptions, MeshBuilder, Shading, SweepOptions, spike, stitch_grid, tube, uv_sphere};
use armory::Rgb;
use procgen_rng::Mulberry32;

use crate::folk::{Digitigrade, HornRow, Race, Span};

/// Mixed into a body's seed for the denizen traits, so they do not roll the numbers the body's
/// own fields do.
const TRAITS_SALT: u32 = 0x31_0a7a_e5;

/// What is marked on a coat, beyond its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Marking {
    None,
    /// A white stripe down the face.
    Blaze,
    /// White on the lower legs only.
    Socks,
    /// Large white patches over the body.
    Pied,
}

/// What a nose ring is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RingMetal {
    Iron,
    Brass,
}

/// How a seed shaped the pair of horns, each a share (0..=1) of its row's range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HornRoll {
    pub length: f32,
    pub curve: f32,
    pub spread: f32,
    pub rise: f32,
    /// Each horn's wear (right, left) as a share of the row's range.
    pub wear: [f32; 2],
    /// A break: the roll the row's chance is tested against (a horn is broken when it is under
    /// it), the side (0 right, 1 left) and how far along the horn.
    pub broken_roll: f32,
    pub broken_side: u8,
    pub broken_at: f32,
}

/// Everything a seed decides about a body's denizen parts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Traits {
    pub horns: HornRoll,
    pub marking: Marking,
    pub ring: Option<RingMetal>,
    /// How big the muzzle is (0..=1 over the row's range), the ears, the mane's shagginess and
    /// its tone (0 the coat's own to 1 near black).
    pub muzzle: f32,
    pub ears: f32,
    pub mane: f32,
    pub mane_tone: f32,
    /// Eye colour, 0 amber to 1 red.
    pub eyes: f32,
    /// The tail's tuft, 0..=1.
    pub tuft: f32,
    /// The hooves, 0 black to 1 pale horn.
    pub hoof_tone: f32,
}

impl Traits {
    /// The traits of the body rolled from `seed`.
    pub fn roll(seed: u32) -> Traits {
        let mut rng = Mulberry32::new(seed ^ TRAITS_SALT);
        let marking = match rng.int(0, 9) {
            0..=3 => Marking::None,
            4..=5 => Marking::Blaze,
            6..=7 => Marking::Socks,
            _ => Marking::Pied,
        };
        let muzzle = rng.next_f64() as f32;
        let ring = if rng.chance(0.4) { Some(if rng.chance(0.5) { RingMetal::Iron } else { RingMetal::Brass }) } else { None };
        let ears = rng.next_f64() as f32;
        let mane = rng.range_f32(0.25, 1.0);
        let mane_tone = rng.next_f64() as f32;
        let eyes = rng.next_f64() as f32;
        let (length, curve, spread, rise) = (rng.next_f64() as f32, rng.next_f64() as f32, rng.next_f64() as f32, rng.next_f64() as f32);
        let wear = [rng.range_f32(0.0, 1.0), rng.range_f32(0.0, 1.0)];
        let broken_roll = rng.next_f64() as f32;
        let broken_side = rng.int(0, 1) as u8;
        let broken_at = rng.range_f32(0.35, 0.8);
        let tuft = rng.next_f64() as f32;
        let hoof_tone = rng.next_f64() as f32;
        let horns = HornRoll { length, curve, spread, rise, wear, broken_roll, broken_side, broken_at };
        Traits { horns, marking, ring, muzzle, ears, mane, mane_tone, eyes, tuft, hoof_tone }
    }

    /// The ring's colour and material kind.
    pub fn ring_colour(metal: RingMetal) -> (armory::MaterialKind, Rgb) {
        match metal {
            RingMetal::Iron => (armory::MaterialKind::Iron, Rgb::hex(0x60605f)),
            RingMetal::Brass => (armory::MaterialKind::Brass, Rgb::hex(0xb08a3c)),
        }
    }

    /// The shape of horn `side` (0 the right, 1 the left) in a head whose half width is `hw`, as
    /// `row` allows.
    pub fn horn_spec(&self, row: &HornRow, side: usize, hw: f32) -> HornSpec {
        let h = &self.horns;
        let lerp = |s: Span, u: f32| s[0] + (s[1] - s[0]) * u;
        let (curve, spread, rise) = (lerp(row.curve, h.curve), lerp(row.spread, h.spread), lerp(row.rise, h.rise));
        let broken = (h.broken_side as usize == side && h.broken_roll < row.broken).then_some(h.broken_at);
        HornSpec {
            length: lerp(row.length, h.length) * hw,
            radius: (0.2 + 0.05 * h.length) * hw,
            rise: 0.3 + 0.22 * rise,
            rise_tip: 0.7 + 0.8 * rise + 0.25 * curve,
            yaw: -0.25 + 0.3 * (1.0 - spread),
            curl: (0.9 + 1.1 * curve) * (1.3 - 0.55 * spread),
            wear: lerp(row.wear, h.wear[side]),
            broken,
        }
    }
}

/// A coat colour's hide at its colour, a shade off: the lighter and the darker of a coat.
pub fn shade(c: Rgb, k: f32) -> Rgb {
    if k < 0.0 { Rgb(c.0 * (1.0 + k), c.1 * (1.0 + k), c.2 * (1.0 + k)) } else { Rgb(c.0 + (1.0 - c.0) * k, c.1 + (1.0 - c.1) * k, c.2 + (1.0 - c.2) * k) }
}

/// A colour moved `k` of the way to `to`.
pub fn mix(a: Rgb, b: Rgb, k: f32) -> Rgb {
    Rgb(a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k, a.2 + (b.2 - a.2) * k)
}

// ── Lofts and small solids ──

/// A value through `keys` (`(t, value)`, `t` ascending) on a cubic Hermite curve with
/// Catmull-Rom tangents, flat beyond the ends.
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

const LOFT_SAMPLES: usize = 64;

/// A surface lofted between rings along a line that leans by `tilt` from vertical (a quarter
/// turn runs it along +Z): each ring a squared ellipse across the line with its own half width and
/// depths in front of it and behind it.
#[derive(Clone, Debug)]
pub struct Loft {
    pub length: f32,
    tilt: Vec<(f32, f32)>,
    half_width: Vec<(f32, f32)>,
    front: Vec<(f32, f32)>,
    back: Vec<(f32, f32)>,
    squareness: f32,
    samples: Vec<V3>,
}

impl Loft {
    pub fn new(start: V3, length: f32, tilt: Vec<(f32, f32)>, half_width: Vec<(f32, f32)>, front: Vec<(f32, f32)>, back: Vec<(f32, f32)>, squareness: f32) -> Loft {
        let mut samples = vec![start];
        let step = length / LOFT_SAMPLES as f32;
        for k in 0..LOFT_SAMPLES {
            let a = (k as f32 + 0.5) * step;
            let th = keyed(&tilt, a);
            let last = samples[k];
            samples.push(vec3::add(last, [0.0, th.cos() * step, th.sin() * step]));
        }
        Loft { length, tilt, half_width, front, back, squareness, samples }
    }

    pub fn centre(&self, a: f32) -> V3 {
        let f = (a / self.length * LOFT_SAMPLES as f32).clamp(0.0, LOFT_SAMPLES as f32);
        let i = (f.floor() as usize).min(LOFT_SAMPLES - 1);
        vec3::lerp(self.samples[i], self.samples[i + 1], f - i as f32)
    }

    fn along(&self, a: f32) -> V3 {
        let th = keyed(&self.tilt, a);
        [0.0, th.cos(), th.sin()]
    }

    fn forward(&self, a: f32) -> V3 {
        let th = keyed(&self.tilt, a);
        [0.0, -th.sin(), th.cos()]
    }

    /// The skin at distance `a` along the line and `phi` round it (0 the creature's left, a
    /// quarter turn on the front), `inflate` out.
    pub fn point(&self, a: f32, phi: f32, inflate: f32) -> V3 {
        let e = 2.0 / self.squareness.max(0.5);
        let (s, c) = phi.sin_cos();
        let w = (keyed(&self.half_width, a) + inflate).max(0.0);
        let depth = if s >= 0.0 { keyed(&self.front, a) } else { keyed(&self.back, a) } + inflate;
        let spow = |x: f32| x.signum() * x.abs().powf(e);
        vec3::add(vec3::add(self.centre(a), [w * spow(c), 0.0, 0.0]), vec3::scale(self.forward(a), depth.max(0.0) * spow(s)))
    }

    /// The loft from `a0` to `a1`, closed at both ends.
    pub fn mesh(&self, a0: f32, a1: f32, rows: usize, cols: usize) -> MeshBuilder {
        let ring = |a: f32| -> Vec<V3> { (0..cols).map(|c| self.point(a, TAU * c as f32 / cols as f32, 0.0)).collect() };
        let rings: Vec<Vec<V3>> = (0..=rows).map(|r| ring(a0 + (a1 - a0) * r as f32 / rows as f32)).collect();
        let mut mesh = stitch_grid(&rings, &GridOptions { wrap_around: true, ..Default::default() });
        mesh.flip();
        mesh.push_fan_cap(&rings[0], vec3::neg(self.along(a0)));
        mesh.push_fan_cap(&rings[rows], self.along(a1));
        mesh
    }
}

/// A tube through `points`, smoothed, `radius` along it.
pub fn limb_tube(points: &[V3], sides: usize, radius: impl Fn(f32) -> f32) -> MeshBuilder {
    let path = CatmullRom::centripetal(points.to_vec()).sample_evenly(28);
    tube(&path, sides, &SweepOptions::default(), radius)
}

/// An ellipsoid at `at`, `r` times `squash`.
pub fn ball(at: V3, r: f32, squash: V3) -> MeshBuilder {
    let mut m = uv_sphere(r, 14, 10);
    m.scale(squash).translate(at);
    m
}

/// A ring of tube `thick` round, a nose ring: standing in the plane through `centre` spanned by
/// `across` and `down`, hung from `centre`, `radius` across, open at the top where it passes through.
pub fn ring(centre: V3, across: V3, down: V3, radius: f32, thick: f32) -> MeshBuilder {
    let path: Vec<V3> = (0..=24)
        .map(|k| {
            let a = 0.12 + (k as f32 / 24.0) * (TAU - 0.24);
            vec3::add(centre, vec3::add(vec3::scale(across, radius * a.sin()), vec3::scale(down, radius * (1.0 - a.cos()))))
        })
        .collect();
    tube(&path, 8, &SweepOptions::default(), |_| thick)
}

/// A lock of hair: a tapering tube from `root` that leaves along `out`, then falls under its own
/// weight (`droop` the share of `length` it drops by its end) and drifts by `sway`.
pub fn lock(root: V3, out: V3, length: f32, width: f32, droop: f32, sway: V3) -> MeshBuilder {
    let path: Vec<V3> = (0..=6)
        .map(|k| {
            let s = k as f32 / 6.0;
            let along = 0.7 * length * (1.0 - (1.0 - s) * (1.0 - s));
            vec3::add(vec3::add(root, vec3::scale(out, along)), vec3::add([0.0, -length * droop * s.powf(1.6), 0.0], vec3::scale(sway, s * s)))
        })
        .collect();
    tube(&path, 5, &SweepOptions::default(), |s| width * (1.0 - 0.85 * s) + 0.01)
}

// ── Horns ──

/// A horn's shape. The same numbers make a bull's, a goat's or a demon's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HornSpec {
    /// Along its curve.
    pub length: f32,
    /// At the root.
    pub radius: f32,
    /// How far above level it leaves the head, radians, and how far above level it points at the tip.
    pub rise: f32,
    pub rise_tip: f32,
    /// How far from straight out it leaves the head, radians, forward positive.
    pub yaw: f32,
    /// How far its heading turns toward the front along its length, radians: the hook.
    pub curl: f32,
    /// How blunt the tip is worn, 0 sharp to 1 stub.
    pub wear: f32,
    /// Where along it (0..1 of its length) it is broken off, if it is.
    pub broken: Option<f32>,
}

/// A horn built by [`horn`]: the base in horn colour and the tip, darker, in the same shape.
#[derive(Clone, Debug)]
pub struct Horn {
    pub base: MeshBuilder,
    pub tip: MeshBuilder,
    /// Where it ends.
    pub end: V3,
}

/// The share of a horn along its length where the tip colour starts.
pub const HORN_TIP_AT: f32 = 0.62;

/// A horn on the side `side` (1 the left) rooted at `root`, in body space. It leaves the head
/// along `yaw` and `rise` and turns toward the front (`curl`) and up (`rise_tip`) as it goes; its
/// radius tapers to a point, or a stub when worn, or ends square where it broke, with a few splinters.
pub fn horn(root: V3, side: f32, spec: &HornSpec) -> Horn {
    let reach = spec.broken.map_or(1.0, |b| b.clamp(0.15, 1.0));
    let steps = 44usize;
    let heading = |s: f32| -> V3 {
        let psi = spec.yaw + spec.curl * s.powf(1.35);
        let theta = spec.rise + (spec.rise_tip - spec.rise) * s;
        [side * psi.cos() * theta.cos(), theta.sin(), psi.sin() * theta.cos()]
    };
    let tip_radius = spec.radius * (0.03 + 0.3 * spec.wear);
    let radius_at = |s: f32| -> f32 {
        let rings = 1.0 + 0.035 * (s * TAU * 7.0).sin() * (1.0 - s);
        (tip_radius + (spec.radius - tip_radius) * (1.0 - s).powf(0.8)) * rings
    };
    let mut points = vec![root];
    let step = spec.length * reach / steps as f32;
    for i in 0..steps {
        let s = (i as f32 + 0.5) / steps as f32 * reach;
        let last = points[i];
        points.push(vec3::add(last, vec3::scale(heading(s), step)));
    }
    let end = points[steps];
    let piece = |s0: f32, s1: f32| -> MeshBuilder {
        if s1 <= s0 + 0.02 {
            return MeshBuilder::new();
        }
        let (i0, i1) = (((s0 / reach) * steps as f32).round() as usize, ((s1 / reach) * steps as f32).round() as usize);
        let (i0, i1) = (i0.min(steps - 1), i1.clamp(i0 + 1, steps));
        let sub = &points[i0..=i1];
        let (a, b) = (i0 as f32 / steps as f32 * reach, i1 as f32 / steps as f32 * reach);
        tube(sub, 16, &SweepOptions::default(), |t| radius_at(a + (b - a) * t))
    };
    let split = HORN_TIP_AT.min(reach);
    let mut base = piece(0.0, split);
    let mut tip = piece(split, reach);
    if spec.broken.is_some() {
        // Splinters standing up from the break.
        let dir = heading(reach);
        let r = radius_at(reach);
        let u = vec3::normalize_or(vec3::cross(dir, vec3::Y), vec3::X);
        let v = vec3::cross(dir, u);
        for k in 0..4 {
            let a = k as f32 * TAU / 4.0 + 0.5;
            let off = vec3::add(vec3::scale(u, r * 0.55 * a.cos()), vec3::scale(v, r * 0.55 * a.sin()));
            let mut splinter = spike(0.3 * r, (0.6 + 0.4 * (k % 2) as f32) * r, 5, 0.0, Shading::SMOOTH);
            splinter.place(vec3::add(end, off), vec3::normalize_or(vec3::add(dir, vec3::scale(off, 0.5 / r.max(1e-3))), dir));
            if reach > HORN_TIP_AT { tip.append(&splinter) } else { base.append(&splinter) };
        }
    } else if spec.wear > 0.25 {
        tip.append(&ball(end, radius_at(1.0), [1.0, 1.0, 1.0]));
    }
    Horn { base, tip, end }
}

// ── Hooves ──

/// A cloven hoof: two toes side by side, the sole flat on the plane y = `sole`, the heel at
/// z = `heel` and the tip `length` in front of it. `width` is the whole hoof's breadth, `height`
/// the wall's at the heel, and `split` how far the toes part (0 together, 1 well apart). The
/// hoof is centred on x = `x`.
pub fn hoof(x: f32, heel: f32, sole: f32, length: f32, width: f32, height: f32, split: f32) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    let gap = 0.04 * width + 0.12 * width * split;
    let toe_w = 0.5 * (width - gap);
    for sg in [-1.0f32, 1.0] {
        let l = length;
        let wall = |f: f32| height * (1.0 - 0.62 * f * f);
        let loft = Loft::new(
            [x + sg * (0.5 * gap + toe_w), sole + 0.5 * height, heel],
            l,
            vec![(0.0, FRAC_PI_2)],
            vec![(0.0, 0.78 * toe_w), (0.2 * l, toe_w), (0.7 * l, 0.92 * toe_w), (l, 0.6 * toe_w)],
            vec![(0.0, 0.5 * height), (l, 0.5 * height)],
            vec![(0.0, 0.5 * wall(0.0)), (0.3 * l, 0.5 * wall(0.3)), (0.7 * l, 0.5 * wall(0.7)), (l, 0.5 * wall(1.0))],
            3.0,
        );
        out.append(&loft.mesh(0.0, l, 14, 18));
    }
    out
}

/// The lower leg beyond the hock of a leg that stands on its toes, in body space: the pastern
/// (a tube from the hock sloping forward and down, with a fetlock) and the hoof, as flesh and
/// keratin. The hoof's sole lies on y = 0. Where the hoof stands: its heel and its tip along Z.
#[derive(Clone, Debug)]
pub struct Foot {
    pub flesh: MeshBuilder,
    pub hoof: MeshBuilder,
    /// The coronet, where the pastern meets the hoof.
    pub coronet: V3,
    /// Heel and toe of the hoof (z).
    pub heel: f32,
    pub toe: f32,
}

/// The foot of the leg whose hock is `hock` (its y is the drop to the floor), for the shape
/// `legs`, with `radius` the pastern's thickness and `leg_radius` the leg's.
pub fn digitigrade_foot(hock: V3, legs: &Digitigrade, leg_radius: f32) -> Foot {
    let drop = hock[1];
    let forward = legs.pastern * drop;
    let radius = 0.3 * leg_radius;
    let width = legs.hoof * leg_radius;
    let (length, height) = (1.15 * width, 0.5 * width);
    let top = [hock[0], 0.8 * height, hock[2] + forward];
    let at = |f: f32, y: f32| [hock[0], hock[1] - y, hock[2] + forward * f];
    let mut flesh = limb_tube(&[hock, at(0.25, 0.3 * drop), at(0.62, 0.62 * drop), top], 16, |t| radius * (1.1 - 0.35 * t));
    // The fetlock: a swelling at the back of the pastern's upper end, and the coronet over the hoof.
    flesh.append(&ball(at(0.3, 0.45 * drop), 1.05 * radius, [1.0, 1.1, 1.0]));
    flesh.append(&ball(top, 1.25 * radius, [1.15, 0.8, 1.2]));
    let heel = top[2] - 0.4 * length;
    let hoof = hoof(hock[0], heel, 0.0, length, width, height, legs.split);
    Foot { flesh, hoof, coronet: top, heel, toe: heel + length }
}

// ── Tail ──

/// The longest tail a rig has bones for.
pub const TAIL_BONES: usize = 4;

/// A tuft at the end of a tail: strands from the tip, falling and flaring. `end` is where the
/// cord ends, `seg` the length of its last segment, `radius` the cord's thickness there.
pub fn tuft(end: V3, seg: f32, radius: f32, fullness: f32, seed: u32) -> MeshBuilder {
    let mut rng = Mulberry32::new(seed ^ 0x7a11_7ef7);
    let strands = 14 + (14.0 * fullness) as usize;
    let mut out = MeshBuilder::new();
    out.append(&ball(vec3::add(end, [0.0, -0.18 * seg, 0.0]), 0.75 * radius * (1.0 + fullness), [1.0, 1.5, 1.0]));
    for _ in 0..strands {
        let a = rng.range_f32(0.0, TAU);
        let spread = rng.range_f32(0.15, 0.6);
        let len = rng.range_f32(0.55, 1.1) * seg * (0.7 + 0.6 * fullness);
        let out_dir = [a.cos(), 0.0, a.sin()];
        let path: Vec<V3> = (0..=5)
            .map(|k| {
                let s = k as f32 / 5.0;
                vec3::add(end, vec3::add(vec3::scale(out_dir, spread * len * s * (1.3 - s) * 0.9), [0.0, -len * s - 0.05 * seg, 0.0]))
            })
            .collect();
        out.append(&tube(&path, 5, &SweepOptions::default(), |s| 0.2 * radius * (1.0 + 0.4 * fullness) * (1.0 - 0.85 * s) + 0.01));
    }
    out
}

/// Whether this race row turns on any denizen part.
pub fn is_denizen_row(race: &Race) -> bool {
    race.horns.is_some() || race.legs.is_some() || race.tail_shape.is_some() || race.muzzle.is_some()
}
