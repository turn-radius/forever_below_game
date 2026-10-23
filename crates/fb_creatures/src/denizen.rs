//! What the denizens' bodies share: the pieces the minotaur is made of, and that the cyclops, the
//! faun and the demons are made of after it.
//!
//! - [`Frame`] lays an upright, heavy denizen out on a human's joints, as a troll is: the human
//!   `fb_body` body is rolled for the stature and frame, then its joints are moved to the
//!   denizen's (a hunched trunk lofted along a curve, a thick neck thrusting the head forward,
//!   arms of its own length, legs that may be digitigrade) and read back as a
//!   [`SkeletonDims`](crate::skeleton::SkeletonDims), so the skeleton's walk and the poses in
//!   [`crate::pose`] move any denizen built on it.
//! - [`horn`] sweeps a horn along a curve ([`HornSpec`]: length, spread, rise, curl, wear, a
//!   broken tip), in a base and a darker tip, so bulls' horns, goats' and demons' differ by their
//!   numbers alone.
//! - [`digitigrade_foot`] and [`hoof`] build the lower leg that stands on the toes: a pastern
//!   sloping down to a cloven hoof whose sole is flat on the floor.
//! - [`tail`] hangs a tail of segment bones with a tuft at its end, and [`hand`] a heavy
//!   five-fingered hand with blunt nails.
//! - [`Loft`] and the little mesh helpers (`limb_tube`, `ball`, `blot`) are the troll's, moved
//!   here: a surface lofted between rings along a curved line, and tubes and balls for limbs.
//!
//! Space is the skeleton's: 1 unit = 10 cm, soles on y = 0, facing +Z, the creature's left +X.
//! Every piece is a pure function of its numbers.

use armory::geom::vec3::{self, V3};
use armory::geom::{CatmullRom, GridOptions, MeshBuilder, Shading, SweepOptions, spike, stitch_grid, tube, uv_sphere};
use armory::{MaterialKind, Part, Rgb};
use fb_body::{Body, BodySpec, Folk, Side};
use std::f32::consts::{FRAC_PI_2, TAU};

use crate::rat::part;
use crate::shape::keyed;
use crate::skeleton::{SkeletonDims, arm_bone, index, leg_bone};

// ── Lofts and limbs ──

/// A surface lofted between rings along a curved centre line: each ring is a squared ellipse in
/// the plane across the line, with its own half width and depths in front of the line and behind
/// it. The torso, and the head's upper half, are lofts.
#[derive(Clone, Debug)]
pub struct Loft {
    pub(crate) length: f32,
    pub(crate) tilt: Vec<(f32, f32)>,
    pub(crate) half_width: Vec<(f32, f32)>,
    pub(crate) front: Vec<(f32, f32)>,
    pub(crate) back: Vec<(f32, f32)>,
    pub(crate) squareness: f32,
    /// The centre line sampled evenly by length.
    pub(crate) samples: Vec<V3>,
}

pub(crate) const LOFT_SAMPLES: usize = 96;

impl Loft {
    /// A loft starting at `start` and running `length` along a line that leans forward by
    /// `tilt` (radians from vertical) at each distance along it.
    pub(crate) fn new(start: V3, length: f32, tilt: Vec<(f32, f32)>, half_width: Vec<(f32, f32)>, front: Vec<(f32, f32)>, back: Vec<(f32, f32)>, squareness: f32) -> Loft {
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

    /// The centre line at distance `a` along it.
    pub(crate) fn centre(&self, a: f32) -> V3 {
        let f = (a / self.length * LOFT_SAMPLES as f32).clamp(0.0, LOFT_SAMPLES as f32);
        let i = (f.floor() as usize).min(LOFT_SAMPLES - 1);
        vec3::lerp(self.samples[i], self.samples[i + 1], f - i as f32)
    }

    pub(crate) fn lean(&self, a: f32) -> f32 {
        keyed(&self.tilt, a)
    }

    /// The way the line runs at `a`, and the way is forward across it.
    pub(crate) fn along(&self, a: f32) -> V3 {
        let th = self.lean(a);
        [0.0, th.cos(), th.sin()]
    }

    pub(crate) fn forward(&self, a: f32) -> V3 {
        let th = self.lean(a);
        [0.0, -th.sin(), th.cos()]
    }

    /// The skin at distance `a` along the line and `phi` round it (0 the creature's left, a
    /// quarter turn on the front), `inflate` out.
    pub(crate) fn point(&self, a: f32, phi: f32, inflate: f32) -> V3 {
        let e = 2.0 / self.squareness.max(0.5);
        let (s, c) = phi.sin_cos();
        let w = (keyed(&self.half_width, a) + inflate).max(0.0);
        let depth = if s >= 0.0 { keyed(&self.front, a) } else { keyed(&self.back, a) } + inflate;
        let spow = |x: f32| x.signum() * x.abs().powf(e);
        let centre = self.centre(a);
        vec3::add(vec3::add(centre, [w * spow(c), 0.0, 0.0]), vec3::scale(self.forward(a), depth.max(0.0) * spow(s)))
    }

    /// The outward direction of the skin at `a`, `phi` (the ring's, near enough).
    pub(crate) fn normal(&self, a: f32, phi: f32) -> V3 {
        let p = self.point(a, phi, 0.0);
        vec3::normalize_or(vec3::sub(p, self.centre(a)), self.forward(a))
    }

    /// The loft from distance `a0` to `a1`, closed at both ends. `dip` is how far the first
    /// `overlap` of it sinks inside the surface (reaching nothing at its far end), so a piece
    /// that sits on another reaches into it and does not open when bent.
    pub(crate) fn mesh(&self, a0: f32, a1: f32, rows: usize, cols: usize, overlap: f32, dip: f32) -> MeshBuilder {
        let ring = |a: f32| -> Vec<V3> {
            let inflate = if overlap > 0.0 { -dip * (1.0 - ((a - a0) / overlap).clamp(0.0, 1.0)) } else { 0.0 };
            (0..cols).map(|c| self.point(a, TAU * c as f32 / cols as f32, inflate)).collect()
        };
        let rings: Vec<Vec<V3>> = (0..=rows).map(|r| ring(a0 + (a1 - a0) * r as f32 / rows as f32)).collect();
        let mut mesh = stitch_grid(&rings, &GridOptions { wrap_around: true, ..Default::default() });
        mesh.flip();
        mesh.push_fan_cap(&rings[0], vec3::neg(self.along(a0)));
        mesh.push_fan_cap(&rings[rows], self.along(a1));
        mesh
    }
}

/// A part, or none when its mesh is empty.
pub(crate) fn some(parts: &mut Vec<Part>, mesh: MeshBuilder, make: impl FnOnce(MeshBuilder) -> Part) {
    if !mesh.is_empty() {
        parts.push(make(mesh));
    }
}

/// A tube through `points`, smoothed, `radius` along it.
pub(crate) fn limb_tube(points: &[V3], sides: usize, radius: impl Fn(f32) -> f32) -> MeshBuilder {
    let path = CatmullRom::centripetal(points.to_vec()).sample_evenly(28);
    tube(&path, sides, &SweepOptions::default(), radius)
}

/// A ball at `at`.
pub(crate) fn ball(at: V3, r: f32, squash: V3) -> MeshBuilder {
    let mut m = uv_sphere(r, 14, 10);
    m.scale(squash).translate(at);
    m
}

/// A flattened blot (or a wart) standing out of a surface at `at`, facing `normal`, `r` across.
pub(crate) fn blot(at: V3, normal: V3, r: f32, height: f32) -> MeshBuilder {
    let mut m = uv_sphere(1.0, 8, 6);
    m.scale([r, height, r]).place(vec3::sub(at, vec3::scale(normal, 0.3 * height)), normal);
    m
}

/// The troll's skin, one bone's worth: `mesh` shifted to be about `origin`.
pub(crate) fn about(mut mesh: MeshBuilder, origin: V3) -> MeshBuilder {
    mesh.translate(vec3::neg(origin));
    mesh
}

/// The point on a tube round the line through `line` at share `t`, `around` its section, and
/// its outward direction.
pub(crate) fn tube_surface(line: &[V3; 2], t: f32, around: f32, radius: f32) -> (V3, V3) {
    let axis = vec3::normalize_or(vec3::sub(line[1], line[0]), vec3::neg(vec3::Y));
    let u = vec3::normalize_or(vec3::cross(axis, vec3::X), vec3::Z);
    let v = vec3::cross(axis, u);
    let n = vec3::add(vec3::scale(u, around.cos()), vec3::scale(v, around.sin()));
    let centre = vec3::lerp(line[0], line[1], t);
    (n, vec3::add(centre, vec3::scale(n, radius)))
}

/// The distance along `loft` at height `y` (its line is vertical).
pub(crate) fn cran_y(loft: &Loft, y: f32) -> f32 {
    (y - loft.samples[0][1]).clamp(0.0, loft.length)
}
// ── The frame ──

/// How a denizen's legs stand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Legs {
    /// On the soles of its feet, as a human does.
    Plantigrade,
    /// On its toes, the heel lifted into a hock behind: the thigh slopes forward to the knee, the
    /// shin back to the hock and the pastern forward again to the hoof.
    Digitigrade(Digitigrade),
}

/// The shape of a digitigrade leg, in shares of the hip's height so it scales with the body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Digitigrade {
    /// How high the hock is, 0.25..0.35: the pastern drops this far to the floor.
    pub hock: f32,
    /// How far up from the hock to the hip the knee sits, 0..1.
    pub knee_height: f32,
    /// How far the knee stands in front of the hip.
    pub knee_forward: f32,
    /// How far the hoof's coronet stands in front of the hock, as a share of the hock's height.
    pub pastern: f32,
}

impl Digitigrade {
    /// A bull's: a long, steeply folded leg with a hock high behind.
    pub const BULL: Digitigrade = Digitigrade { hock: 0.3, knee_height: 0.52, knee_forward: 0.27, pastern: 0.5 };
}

/// What a [`Frame`] is laid out from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameSpec {
    /// Seeds the human body its joints are taken from.
    pub seed: u32,
    /// Height as a share of a typical human's before it leans.
    pub stature: f32,
    /// How broad the frame is (a human's `frame`, 0.6..1.1).
    pub frame: f32,
    /// Bulk of the limbs and chest, 0..=1.
    pub build: f32,
    /// How far the belly stands out, 0..=1.
    pub belly: f32,
    /// How far the top of the trunk leans forward, radians.
    pub lean: f32,
    /// Shoulders' breadth as a multiple of a human's.
    pub shoulders: f32,
    /// How much the muscle of the shoulders and neck humps up the back, 0..=1.
    pub hump: f32,
    /// Arm length as a multiple of a human's.
    pub reach: f32,
    /// The neck's thickness as a multiple of a human's, and its length as a multiple of the head's
    /// half height.
    pub neck: (f32, f32),
    /// The head's half width, half height and half depth as multiples of a human's.
    pub head: (f32, f32, f32),
    /// How far the head tips forward, radians.
    pub head_lean: f32,
    /// Where the head's centre sits from the atlas: up it, in half heights, and forward along it,
    /// in half depths.
    pub head_set: (f32, f32),
    pub legs: Legs,
    /// How much longer its legs are than a human's, as a share of the hip's height: a folded
    /// digitigrade leg is shorter than it stands, so the hips ride higher.
    pub leg_lift: f32,
    /// How far apart the hips stand, as a multiple of a human's: heavy thighs need room.
    pub hip_spread: f32,
}

/// Where everything of an upright denizen sits: the trunk's loft and its stations, the neck and
/// head, the limbs' girths, and the joints as the skeleton's poses take them.
#[derive(Clone, Debug)]
pub struct Frame {
    pub spec: FrameSpec,
    pub body: Body,
    pub dims: SkeletonDims,
    pub torso: Loft,
    /// Distances along the torso's line: the hip, the waist (the spine bone's joint), the chest
    /// (the ribcage's) and the neck's base.
    pub stations: [f32; 4],
    pub atlas: V3,
    pub neck_radius: f32,
    pub head_centre: V3,
    /// Half width, half height and half depth of the head.
    pub head: (f32, f32, f32),
    /// The girth of an upper arm.
    pub limb: f32,
    pub hand_length: f32,
    /// The girth of a thigh.
    pub leg_radius: f32,
}

impl Frame {
    pub fn new(spec: &FrameSpec) -> Frame {
        let mut body_spec = BodySpec::roll(Folk::Human, spec.seed);
        body_spec.stature = spec.stature;
        body_spec.frame = spec.frame;
        body_spec.figure = 0.0;
        (body_spec.muscle, body_spec.slender, body_spec.bust) = (0.95, 0.15, 0.0);
        let body = Body::new(&body_spec);
        let t = body.trunk_length;
        let hip_y = body.hip_y * (1.0 + spec.leg_lift);
        let sw = body.shoulder_half_width * spec.shoulders;
        let (hip, waist, chest) = (body.hip, body.waist, body.chest);
        let build = spec.build;
        // The stations along the torso's line, from the crotch.
        let a_h = 0.16 * t;
        let a_s = a_h + 0.30 * t;
        let a_r = a_h + 0.60 * t;
        let a_n = a_h + 1.04 * t;
        let lean_top = spec.lean;
        let tilt = vec![(0.0, 0.0), (a_h, 0.03), (a_s, 0.2 * lean_top + 0.02), (a_r, 0.58 * lean_top), (a_n, lean_top)];
        let half_width = vec![
            (0.0, 0.5 * hip.half_width),
            (a_h, 1.1 * hip.half_width),
            (a_s, waist.half_width * (1.2 + 0.3 * build)),
            (a_r, chest.half_width * (1.3 + 0.2 * build) * (0.5 + 0.5 * spec.shoulders)),
            (a_n - 0.2 * t, sw * 1.12),
            (a_n - 0.08 * t, sw * 0.95),
            (a_n, 0.72 * sw),
        ];
        let front = vec![
            (0.0, 0.5 * hip.half_depth),
            (a_h, 1.1 * hip.half_depth),
            (a_s, waist.half_depth * (1.1 + spec.belly)),
            (a_r, chest.half_depth * (1.2 + 0.4 * build + 0.3 * spec.belly)),
            (a_n - 0.2 * t, chest.half_depth * 1.05),
            (a_n - 0.08 * t, chest.half_depth * 0.8),
            (a_n, chest.half_depth * 0.62),
        ];
        let back = vec![
            (0.0, 0.7 * hip.half_depth),
            (a_h, 1.35 * hip.half_depth),
            (a_s, waist.half_depth * 1.0),
            (a_r, chest.half_depth * 1.18),
            (a_n - 0.22 * t, chest.half_depth * (1.3 + 0.45 * spec.hump)),
            (a_n - 0.08 * t, chest.half_depth * (1.25 + 0.35 * spec.hump)),
            (a_n, chest.half_depth * 1.05),
        ];
        let torso = Loft::new([0.0, hip_y - a_h, 0.0], a_n, tilt, half_width, front, back, 2.4);

        // The neck rises from the hump and thrusts the head forward.
        let head_form = body.head;
        let neck_radius = spec.neck.0 * body.neck_radius;
        let c7 = torso.centre(a_n);
        let neck_len = spec.neck.1 * head_form.half_height;
        let neck_lean = (lean_top - 0.12).max(0.0);
        let atlas = vec3::add(c7, [0.0, neck_lean.cos() * neck_len, neck_lean.sin() * neck_len]);
        let head = (spec.head.0 * head_form.half_width, spec.head.1 * head_form.half_height, spec.head.2 * head_form.half_depth);
        let (sin, cos) = spec.head_lean.sin_cos();
        let (up, fwd) = ([0.0, cos, sin], [0.0, -sin, cos]);
        let head_centre = vec3::add(atlas, vec3::add(vec3::scale(up, spec.head_set.0 * head.1), vec3::scale(fwd, spec.head_set.1 * head.2)));

        // The skeleton's joints, moved to the denizen's.
        let mut dims = SkeletonDims::of(body.clone());
        let a_sh = a_n - 0.16 * t;
        let limb = 0.5 * (body.arm(Side::Right).radius + body.arm(Side::Left).radius) * (1.25 + 0.45 * build);
        let mut arms = dims.arms;
        for (k, side) in Side::BOTH.into_iter().enumerate() {
            let [s0, e0, w0] = body.arm(side).joints;
            let c = torso.centre(a_sh);
            let shoulder = vec3::add(c, [side.sign() * (sw * 0.98 + 0.1 * limb), -0.1 * t, 0.12 * t]);
            let elbow = vec3::add(shoulder, vec3::scale(vec3::sub(e0, s0), spec.reach));
            let wrist = vec3::add(elbow, vec3::scale(vec3::sub(w0, e0), spec.reach));
            arms[k] = [shoulder, elbow, wrist];
        }
        dims.arms = arms;
        let hinge_local = [0.0, -0.18 * head.1, -0.3 * head.2];
        let hinge = vec3::add(head_centre, [hinge_local[0], hinge_local[1] * cos - hinge_local[2] * sin, hinge_local[1] * sin + hinge_local[2] * cos]);
        dims.hinge = hinge;
        dims.origins[index::PELVIS] = [0.0, hip_y, 0.0];
        dims.origins[index::SPINE] = torso.centre(a_s);
        dims.origins[index::RIBCAGE] = torso.centre(a_r);
        dims.origins[index::NECK] = c7;
        dims.origins[index::SKULL] = atlas;
        dims.origins[index::JAW] = hinge;
        for (side, arm) in Side::BOTH.into_iter().zip(arms) {
            for (j, &origin) in arm.iter().enumerate() {
                dims.origins[arm_bone(side) + j] = origin;
            }
        }
        let leg_radius = 0.5 * (body.leg(Side::Right).radius + body.leg(Side::Left).radius) * (1.1 + 0.3 * build);
        let lengthen = hip_y / body.hip_y;
        for k in 0..2 {
            let [hip, knee, ankle] = dims.legs[k];
            let hip = [hip[0] * spec.hip_spread, hip[1] * lengthen, hip[2]];
            let (knee, ankle) = match spec.legs {
                Legs::Plantigrade => ([knee[0] * spec.hip_spread, knee[1] * lengthen, knee[2]], [ankle[0] * spec.hip_spread, ankle[1], ankle[2]]),
                Legs::Digitigrade(g) => {
                    let hock_y = g.hock * hip[1];
                    ([hip[0], hock_y + g.knee_height * (hip[1] - hock_y), hip[2] + g.knee_forward * hip[1]], [hip[0], hock_y, hip[2]])
                }
            };
            dims.legs[k] = [hip, knee, ankle];
            dims.ankles[k] = ankle;
            for j in 0..3 {
                dims.origins[leg_bone(Side::BOTH[k]) + j] = dims.legs[k][j];
            }
        }
        let planar = |a: V3, b: V3| (b[1] - a[1]).hypot(b[2] - a[2]);
        dims.thigh = planar(dims.legs[0][0], dims.legs[0][1]);
        dims.shin = planar(dims.legs[0][1], dims.legs[0][2]);
        dims.foot_length = match spec.legs {
            Legs::Plantigrade => dims.foot_length * 1.2,
            Legs::Digitigrade(g) => (g.pastern + 0.7) * dims.legs[0][2][1],
        };
        let hand_length = 0.105 * body.height * body.race().arm * 1.4;
        Frame { spec: *spec, body, dims, torso, stations: [a_h, a_s, a_r, a_n], atlas, neck_radius, head_centre, head, limb, hand_length, leg_radius }
    }
}

// ── Horns ──

/// A horn's shape. The same numbers make a bull's, a goat's or a demon's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HornSpec {
    /// Along its curve.
    pub length: f32,
    /// At the root.
    pub radius: f32,
    /// How far above level it leaves the head, radians, and how far above level it points at the
    /// tip.
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

/// A horn on the side `side` (1 the creature's left) rooted at `root`, in the head's space (+Z
/// forward, +Y up, +X left). It leaves the head along `yaw` and `rise` and turns toward the
/// front (`curl`) and up (`rise_tip`) as it goes; its radius tapers to a point, or a stub when
/// worn, or ends square where it broke, with a few splinters.
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
    // The path between shares `s0` and `s1` of the whole, as a tube.
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
/// the wall's at the heel, and `split` how far the toes part (0 together, 1 well apart).
pub fn hoof(heel: f32, sole: f32, length: f32, width: f32, height: f32, split: f32) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    let gap = 0.04 * width + 0.12 * width * split;
    let toe_w = 0.5 * (width - gap);
    for sg in [-1.0f32, 1.0] {
        // Along the toe from heel to tip: its half width, the wall above the axis, a thin sole
        // under it.
        let l = length;
        let wall = |f: f32| height * (1.0 - 0.62 * f * f);
        let loft = Loft::new(
            [sg * (0.5 * gap + toe_w), sole + 0.5 * height, heel],
            l,
            vec![(0.0, FRAC_PI_2)],
            vec![(0.0, 0.78 * toe_w), (0.2 * l, toe_w), (0.7 * l, 0.92 * toe_w), (l, 0.6 * toe_w)],
            vec![(0.0, 0.5 * height), (l, 0.5 * height)],
            vec![(0.0, 0.5 * wall(0.0)), (0.3 * l, 0.5 * wall(0.3)), (0.7 * l, 0.5 * wall(0.7)), (l, 0.5 * wall(1.0))],
            3.0,
        );
        out.append(&loft.mesh(0.0, l, 14, 18, 0.0, 0.0));
    }
    out
}

/// What [`digitigrade_foot`] is built from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootSpec {
    /// How far the hock stands above the floor: the foot bone's origin is the hock.
    pub drop: f32,
    /// How far in front of the hock the pastern ends at the hoof.
    pub forward: f32,
    /// The pastern's radius.
    pub radius: f32,
    /// The hoof's breadth, length and wall height.
    pub hoof: (f32, f32, f32),
    /// How far the hoof's toes part, 0..=1.
    pub split: f32,
}

/// The foot of a digitigrade leg in the foot bone's space (origin at the hock): the pastern, a
/// tube from the hock sloping forward and down with a fetlock, and the hoof, as flesh and
/// keratin. The hoof's sole lies on y = -`drop`, so a pose only has to keep the foot level to
/// stand it on the floor.
pub fn digitigrade_foot(spec: &FootSpec) -> (MeshBuilder, MeshBuilder) {
    let (width, length, height) = spec.hoof;
    let top = [0.0, -spec.drop + 0.8 * height, spec.forward];
    let mut flesh = limb_tube(&[[0.0, 0.0, 0.0], [0.0, -0.3 * spec.drop, 0.25 * spec.forward], [0.0, -0.62 * spec.drop, 0.62 * spec.forward], top], 16, |t| spec.radius * (1.1 - 0.35 * t));
    // The fetlock: a swelling at the back of the pastern's upper end, and the coronet over the hoof.
    flesh.append(&ball([0.0, -0.45 * spec.drop, 0.3 * spec.forward], 1.05 * spec.radius, [1.0, 1.1, 1.0]));
    flesh.append(&ball(top, 1.25 * spec.radius, [1.15, 0.8, 1.2]));
    let heel = spec.forward - 0.4 * length;
    let hoof = hoof(heel, -spec.drop, length, width, height, spec.split);
    (flesh, hoof)
}

// ── Tail ──

/// How many bones a tail has.
pub const TAIL_BONES: usize = 3;
/// The index of the tail's first bone: after the skeleton's eighteen.
pub const TAIL_FIRST: usize = index::COUNT;
/// How far the tail's root swings back from hanging straight down, radians.
pub const TAIL_REST: f32 = 0.3;

/// The length of one of the tail's segments.
pub fn tail_segment(d: &SkeletonDims) -> f32 {
    0.2 * d.legs[0][0][1]
}

/// Where each tail bone sits on its parent at rest: the first on the pelvis, behind the seat, the
/// others one segment down their parent.
pub fn tail_offsets(d: &SkeletonDims) -> [V3; TAIL_BONES] {
    let seg = tail_segment(d);
    [[0.0, 0.25 * d.body.hip.half_depth, -1.3 * d.body.hip.half_depth], [0.0, -seg, 0.0], [0.0, -seg, 0.0]]
}

/// What a tail is built from.
#[derive(Clone, Copy, Debug)]
pub struct TailSpec {
    pub seed: u32,
    /// The cord's radius at the root.
    pub radius: f32,
    /// The tuft's fullness, 0..=1.
    pub tuft: f32,
    pub cord: Rgb,
    pub hair: Rgb,
}

/// A tail of [`TAIL_BONES`] segments hanging down their bones' -Y, tapering to a tuft at the
/// last one's end: for each bone, the cord (`"tail"`, skin) and, on the last, the tuft (`"hair"`).
pub fn tail(d: &SkeletonDims, spec: &TailSpec) -> Vec<Vec<Part>> {
    let seg = tail_segment(d);
    let mut rng = procgen_rng::Mulberry32::new(spec.seed ^ 0x7a11_7ef7);
    let mut out = Vec::new();
    for i in 0..TAIL_BONES {
        let r0 = spec.radius * (1.0 - 0.28 * i as f32);
        let r1 = spec.radius * (1.0 - 0.28 * (i + 1) as f32);
        let mut cord = limb_tube(&[[0.0, 0.0, 0.0], [0.0, -0.5 * seg, 0.0], [0.0, -1.04 * seg, 0.0]], 10, |t| r0 + (r1 - r0) * t);
        cord.append(&ball([0.0, 0.0, 0.0], r0 * 1.02, [1.0, 1.0, 1.0]));
        let mut parts = vec![part(cord, "tail", MaterialKind::Skin, spec.cord, true)];
        if i == TAIL_BONES - 1 {
            // The tuft: strands from the tip, falling and flaring.
            let root = [0.0, -seg, 0.0];
            let strands = 14 + (14.0 * spec.tuft) as usize;
            let mut tuft = MeshBuilder::new();
            tuft.append(&ball(vec3::add(root, [0.0, -0.18 * seg, 0.0]), 0.75 * spec.radius * (1.0 + spec.tuft), [1.0, 1.5, 1.0]));
            for _ in 0..strands {
                let a = rng.range_f32(0.0, TAU);
                let spread = rng.range_f32(0.15, 0.6);
                let len = rng.range_f32(0.55, 1.1) * seg * (0.7 + 0.6 * spec.tuft);
                let out_dir = [a.cos(), 0.0, a.sin()];
                let path: Vec<V3> = (0..=5)
                    .map(|k| {
                        let s = k as f32 / 5.0;
                        vec3::add(root, vec3::add(vec3::scale(out_dir, spread * len * s * (1.3 - s) * 0.9), [0.0, -len * s - 0.05 * seg, 0.0]))
                    })
                    .collect();
                tuft.append(&tube(&path, 5, &SweepOptions::default(), |s| 0.2 * spec.radius * (1.0 + 0.4 * spec.tuft) * (1.0 - 0.85 * s) + 0.01));
            }
            parts.push(part(tuft, "hair", MaterialKind::Hair, spec.hair, false));
        }
        out.push(parts);
    }
    out
}

// ── Hands ──

/// What [`hand`] is built from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandSpec {
    /// Wrist to middle fingertip.
    pub length: f32,
    /// The wrist's radius.
    pub wrist: f32,
    /// How far each finger joint bends, radians: 0.3 open and relaxed, 1.2 a fist.
    pub curl: f32,
    /// How thick the fingers are, 1 for a human's share of the hand.
    pub thick: f32,
}

/// A heavy hand and its blunt nails in hand space: origin at the wrist, +Y toward the fingers,
/// +Z the back of the hand, the thumb toward -X (a right hand). The fingers curl toward the
/// palm (-Z).
pub fn hand(spec: &HandSpec) -> (MeshBuilder, MeshBuilder) {
    let l = spec.length;
    let mut flesh = MeshBuilder::new();
    let mut nails = MeshBuilder::new();
    let half = 0.36 * l;
    let mut palm = uv_sphere(1.0, 18, 12);
    palm.scale([half, 0.3 * l, 0.17 * l]).translate([0.0, 0.2 * l, 0.0]);
    flesh.append(&palm);
    flesh.append(&ball([0.0, 0.0, 0.0], 1.05 * spec.wrist, [1.0, 1.0, 1.0]));
    let fr = 0.078 * l * spec.thick;
    for &(kx, scale) in &[(-0.7f32, 0.9f32), (-0.24, 1.0), (0.24, 0.94), (0.7, 0.78)] {
        let knuckle = [kx * half, 0.4 * l, 0.0];
        let mut at = knuckle;
        let mut angle = 0.0;
        let mut pts = vec![[kx * half * 0.8, 0.3 * l, 0.0], knuckle];
        let mut dir = [0.0, 1.0, 0.0];
        for (k, share) in [0.22f32, 0.19, 0.17].into_iter().enumerate() {
            angle += spec.curl * (0.7 + 0.25 * k as f32);
            dir = [0.0, angle.cos(), -angle.sin()];
            at = vec3::add(at, vec3::scale(dir, l * share * scale));
            pts.push(at);
        }
        flesh.append(&limb_tube(&pts, 10, |t| fr * (1.0 - 0.3 * t)));
        for p in &pts[2..] {
            flesh.append(&ball(*p, fr * 0.96, [1.0, 1.0, 1.0]));
        }
        nails.append(&ball(vec3::add(at, vec3::add(vec3::scale(dir, 0.3 * fr), [0.0, 0.0, 0.0])), 0.62 * fr, [1.0, 1.0, 1.0]));
    }
    // The thumb: from the base of the palm out and forward.
    let thumb = [[-0.28 * half, 0.1 * l, -0.02 * l], [-half, 0.26 * l, -0.07 * l], [-1.22 * half, 0.4 * l, -0.14 * l], [-1.12 * half, 0.5 * l, -0.2 * l - 0.1 * spec.curl * l]];
    flesh.append(&limb_tube(&thumb, 10, |t| 1.2 * fr * (1.0 - 0.3 * t)));
    for p in &thumb[1..] {
        flesh.append(&ball(*p, fr * 1.0, [1.0, 1.0, 1.0]));
    }
    nails.append(&ball(vec3::add(thumb[3], [0.0, 0.2 * fr, 0.5 * fr]), 0.62 * fr, [1.0, 0.8, 1.0]));
    (flesh, nails)
}

/// A ring of tube `thick` round, a nose ring or an earring, standing in the plane through
/// `centre` spanned by `across` and `down`: hung from `centre`, `radius` across, open at the
/// top where it passes through.
pub fn ring(centre: V3, across: V3, down: V3, radius: f32, thick: f32) -> MeshBuilder {
    let path: Vec<V3> = (0..=24)
        .map(|k| {
            let a = 0.12 + (k as f32 / 24.0) * (TAU - 0.24);
            vec3::add(centre, vec3::add(vec3::scale(across, radius * a.sin()), vec3::scale(down, radius * (1.0 - a.cos()))))
        })
        .collect();
    tube(&path, 8, &SweepOptions::default(), |_| thick)
}

/// A lock of hair: a tapering tube from `root` that leaves along `out`, then falls under its
/// own weight (`droop` the share of `length` it drops by its end) and drifts by `sway`.
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
