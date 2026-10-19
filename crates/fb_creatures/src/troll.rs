//! Trolls: [`TrollState`] from a seed, its [`Rig`] of named bones, and the rule that makes one
//! hard to kill ([`Regeneration`]).
//!
//! A troll is laid out on a human's joints as a skeleton is (`fb_body`'s body, then the same
//! eighteen bones in the same order, [`crate::skeleton::index`]), but fleshy, large and stooped.
//! The joints a human has are moved to where a troll's are: the spine curves forward into a
//! hump at the shoulders, so the rest pose is already stooped and the bones never need a hunch
//! turned onto them; the neck rises again and thrusts the head forward; the arms are a third
//! longer than a human's and hang past the knees. Because the layout is a
//! [`SkeletonDims`](crate::skeleton::SkeletonDims), the skeleton's walk and swing poses move a
//! troll too ([`crate::pose::troll_with`]).
//!
//! The skin is lofted between rings along the spine's curve, so the belly, hump, chest and hips
//! are one continuous surface cut at the bones' joints; each upper piece dips a little inside
//! the one below so a bent joint does not open. Limbs are tubes with a muscle's swell and balls
//! at the joints. The hands are long-fingered with claws, the feet broad with clawed toes, and
//! the head has a heavy brow over deep-set glowing eyes, a long hooked nose, ears, a jutting
//! lower jaw with tusks, and lank hair. Warts and darker blotches lie over the skin.
//!
//! Seeded variation: height, bloat, stoop, reach, skin from moss green to stone grey, mottling,
//! warts, nose, brow, tusks, ears, eyes, hair and claws. All of it our own; the proportions
//! follow a human's.
//!
//! The SRD's Troll (5.1 p. 351) is a Large giant with Regeneration: it regains 10 hit points at
//! the start of its turn unless it took acid or fire damage since its last turn, and it dies
//! only if it starts its turn at 0 hit points and does not regenerate. [`Regeneration`] is that
//! rule on a clock, with no engine in it.

use armory::geom::vec3::{self, V3};
use armory::geom::{CatmullRom, GridOptions, MeshBuilder, Shading, SweepOptions, ellipsoid, spike, stitch_grid, tube, uv_sphere};
use armory::{MaterialKind, Part, Rgb};
use fb_body::{Body, BodySpec, Folk, Placement, Side};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::rat::{mix, part};
use crate::rig::Rig;
use crate::shape::{eye, keyed};
use crate::skeleton::{NAMES, SkeletonDims, arm_bone, index, leg_bone, parent_of};
use crate::{Dice, Size, Species};

/// Mixed into a troll's seed so it does not roll the same numbers as a body of that seed.
const TROLL_SALT: u32 = 0x7120_11e5;

/// What it regains at the start of each of its turns ([SRD 5.1 p. 351](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=351),
/// "Regeneration").
pub const REGAIN: u32 = 10;
/// Seconds between its turns: the SRD's round is 6 seconds (p. 90).
pub const TURN_SECONDS: f32 = 6.0;
/// One claw's damage on a hit: 14 (2d6 + 4) slashing ([SRD 5.1 p. 351](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=351)).
/// The bite is the stat block's [`crate::StatBlock::bite`], 7 (1d6 + 4) piercing.
pub const CLAW: Dice = Dice::new(2, 6, 4);
/// Claws in its Multiattack, with the one bite.
pub const CLAWS: u32 = 2;

/// What a troll's turn came to ([`Regeneration::tick`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    /// It regained this many hit points.
    Regained(u32),
    /// It took acid or fire damage since its last turn, so it regained nothing.
    Stopped,
    /// It was at 0 hit points and regenerated: it gets up with this many.
    Rose(u32),
    /// It was at 0 hit points and did not regenerate: it is dead for good.
    Died,
}

/// A troll's regeneration, on a clock of its own turns ([`TURN_SECONDS`] apart).
///
/// - At the start of each turn it regains [`REGAIN`] hit points, up to its maximum.
/// - Acid or fire damage taken since its last turn ([`Regeneration::burn`]) stops that turn's
///   regeneration, and only that turn's: the next one regenerates again.
/// - At 0 hit points it falls ([`Regeneration::fall`] starts its next turn a full turn away). At
///   that turn, unless it was burned, it rises with the hit points it regenerated; burned, it
///   stays down for good.
///
/// Our own reading of the SRD for real time: a turn is a clock, not a place in an initiative.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Regeneration {
    /// Seconds into the current turn.
    clock: f32,
    /// Whether acid or fire hurt it since its last turn.
    burned: bool,
}

impl Regeneration {
    /// A fresh clock, nothing burned.
    pub fn new() -> Regeneration {
        Regeneration::default()
    }

    /// Acid or fire damage taken: the next regeneration is stopped.
    pub fn burn(&mut self) {
        self.burned = true;
    }

    /// Whether the next turn's regeneration is stopped.
    pub fn is_stopped(&self) -> bool {
        self.burned
    }

    /// It has just fallen to 0 hit points: its next turn is a whole turn away.
    pub fn fall(&mut self) {
        self.clock = 0.0;
    }

    /// Lets `dt` seconds pass for a troll with `hp` of `max` hit points. At the end of a turn,
    /// says what the turn came to; `None` while a turn is under way, or when nothing happens
    /// (at full hit points and not burned).
    pub fn tick(&mut self, dt: f32, hp: u32, max: u32) -> Option<Turn> {
        self.clock += dt;
        if self.clock < TURN_SECONDS {
            return None;
        }
        self.clock = self.clock.rem_euclid(TURN_SECONDS);
        if std::mem::take(&mut self.burned) {
            return Some(if hp == 0 { Turn::Died } else { Turn::Stopped });
        }
        if hp == 0 {
            Some(Turn::Rose(REGAIN.min(max)))
        } else if hp < max {
            Some(Turn::Regained(REGAIN.min(max - hp)))
        } else {
            None
        }
    }
}

/// One troll, every choice the seed makes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrollState {
    pub seed: u32,
    /// Height as a share of a typical human's before it stoops, 1.42..1.58.
    pub stature: f32,
    /// 0.55 heavy to 1 very heavy: the belly and the limbs' girth.
    pub build: f32,
    /// How far it stoops, 0..=1.
    pub stoop: f32,
    /// How much longer its arms are than a human's, 1.25..1.45.
    pub reach: f32,
    /// The skin, 0 moss green through 0.5 grey green to 1 stone grey.
    pub tone: f32,
    /// How much darker blotches show on the skin, 0..=1.
    pub mottle: f32,
    /// Warts on the skin.
    pub warts: u32,
    /// How long and hooked the nose is, 0..=1.
    pub nose: f32,
    /// How heavy the brow ridge is, 0..=1.
    pub brow: f32,
    /// How long the tusks are, 0 for none to 1.
    pub tusks: f32,
    /// How big the ears are, 0..=1.
    pub ears: f32,
    /// The eyes, 0 yellow to 1 red.
    pub eyes: f32,
    /// How much lank hair, 0 bald to 1.
    pub hair: f32,
    /// The hair's colour, 0 black through brown to 1 grey.
    pub hair_tone: f32,
    /// How long the claws are, 0..=1.
    pub claws: f32,
}

impl TrollState {
    /// A troll rolled from `seed`: the same seed always gives the same troll.
    pub fn roll(seed: u32) -> TrollState {
        let mut rng = Mulberry32::new(seed ^ TROLL_SALT);
        let stature = rng.range_f32(1.42, 1.58);
        let build = 0.55 + 0.45 * rng.next_f64() as f32;
        let stoop = rng.next_f64() as f32;
        let reach = rng.range_f32(1.25, 1.45);
        let tone = rng.next_f64() as f32;
        let mottle = rng.next_f64() as f32;
        let warts = rng.int(6, 30);
        let nose = rng.next_f64() as f32;
        let brow = rng.next_f64() as f32;
        // Seven in ten have tusks.
        let tusks = if rng.chance(0.7) { rng.range_f32(0.3, 1.0) } else { 0.0 };
        let ears = rng.next_f64() as f32;
        let eyes = rng.next_f64() as f32;
        // Most have a few lank strands.
        let hair = if rng.chance(0.85) { rng.range_f32(0.3, 1.0) } else { 0.0 };
        let hair_tone = rng.next_f64() as f32;
        let claws = rng.next_f64() as f32;
        TrollState { seed, stature, build, stoop, reach, tone, mottle, warts, nose, brow, tusks, ears, eyes, hair, hair_tone, claws }
    }

    pub fn species(&self) -> Species {
        Species::Troll
    }

    /// The size its body is drawn at: its SRD size.
    pub fn drawn_size(&self) -> Size {
        Species::Troll.stat_block().size
    }

    /// The skin: moss green, grey green, stone grey, by [`TrollState::tone`]. Our own picks.
    pub fn skin_colour(&self) -> Rgb {
        const MOSS: Rgb = Rgb::hex(0x667f45);
        const GREY_GREEN: Rgb = Rgb::hex(0x73805f);
        const STONE: Rgb = Rgb::hex(0x848a82);
        if self.tone < 0.5 { mix(MOSS, GREY_GREEN, self.tone / 0.5) } else { mix(GREY_GREEN, STONE, (self.tone - 0.5) / 0.5) }
    }

    /// The darker blotches and the warts' colour.
    fn blotch_colour(&self) -> Rgb {
        mix(self.skin_colour(), Rgb::hex(0x2f3a22), 0.5)
    }

    fn wart_colour(&self) -> Rgb {
        mix(self.skin_colour(), Rgb::hex(0xa59a6a), 0.45)
    }

    /// The irises: yellow to red. Our own picks.
    pub fn eye_colour(&self) -> Rgb {
        mix(Rgb::hex(0xe6c438), Rgb::hex(0xd2381e), self.eyes)
    }

    fn hair_colour(&self) -> Rgb {
        mix(Rgb::hex(0x1d1a16), Rgb::hex(0x8d8a82), self.hair_tone)
    }

    /// The human body its joints are taken from: stature and frame from the troll's. A tall,
    /// heavy, broad-shouldered human.
    pub fn body(&self) -> Body {
        Body::new(&self.spec())
    }

    /// The spec of [`TrollState::body`].
    pub fn spec(&self) -> BodySpec {
        let mut spec = BodySpec::roll(Folk::Human, self.seed ^ TROLL_SALT);
        spec.stature = self.stature;
        spec.frame = 0.6 + 0.45 * self.build;
        spec.figure = 0.0;
        (spec.muscle, spec.slender, spec.bust) = (0.9, 0.2, 0.0);
        spec
    }

    /// The troll's joints and measures, stooped, in the shape the skeleton's poses take.
    pub fn dims(&self) -> SkeletonDims {
        Layout::new(self).dims
    }

    /// Crown to soles at rest, stooped, world units.
    pub fn height(&self) -> f32 {
        let l = Layout::new(self);
        l.head_centre[1] + l.head.1
    }

    /// How far it reaches across the floor from its centre at rest: a hand hanging at its side,
    /// a foot out in front, or the head thrust forward.
    pub fn radius(&self) -> f32 {
        let l = Layout::new(self);
        let d = &l.dims;
        let around = |p: V3| p[0].hypot(p[2]);
        let hands = d.arms.iter().map(|a| around([a[2][0], 0.0, a[2][2]]) + l.hand_length * 0.4);
        let feet = d.ankles.iter().map(|a| around([a[0], 0.0, a[2] + d.foot_length]));
        let head = around([0.0, 0.0, l.head_centre[2] + l.head.2 * (1.2 + 0.9 * self.nose)]);
        let shoulders = d.arms.iter().map(|a| around([a[0][0], 0.0, a[0][2]]) + 0.8 * l.limb);
        hands.chain(feet).chain(shoulders).chain([head]).fold(0.0, f32::max)
    }

    /// Its bones and meshes, standing at rest.
    pub fn build(&self) -> Rig {
        build(self)
    }
}

// ── Layout ──

/// A surface lofted between rings along a curved centre line: each ring is a squared ellipse in
/// the plane across the line, with its own half width and depths in front of the line and behind
/// it. The torso, and the head's upper half, are lofts.
#[derive(Clone, Debug)]
struct Loft {
    length: f32,
    tilt: Vec<(f32, f32)>,
    half_width: Vec<(f32, f32)>,
    front: Vec<(f32, f32)>,
    back: Vec<(f32, f32)>,
    squareness: f32,
    /// The centre line sampled evenly by length.
    samples: Vec<V3>,
}

const LOFT_SAMPLES: usize = 96;

impl Loft {
    /// A loft starting at `start` and running `length` along a line that leans forward by
    /// `tilt` (radians from vertical) at each distance along it.
    fn new(start: V3, length: f32, tilt: Vec<(f32, f32)>, half_width: Vec<(f32, f32)>, front: Vec<(f32, f32)>, back: Vec<(f32, f32)>, squareness: f32) -> Loft {
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
    fn centre(&self, a: f32) -> V3 {
        let f = (a / self.length * LOFT_SAMPLES as f32).clamp(0.0, LOFT_SAMPLES as f32);
        let i = (f.floor() as usize).min(LOFT_SAMPLES - 1);
        vec3::lerp(self.samples[i], self.samples[i + 1], f - i as f32)
    }

    fn lean(&self, a: f32) -> f32 {
        keyed(&self.tilt, a)
    }

    /// The way the line runs at `a`, and the way is forward across it.
    fn along(&self, a: f32) -> V3 {
        let th = self.lean(a);
        [0.0, th.cos(), th.sin()]
    }

    fn forward(&self, a: f32) -> V3 {
        let th = self.lean(a);
        [0.0, -th.sin(), th.cos()]
    }

    /// The skin at distance `a` along the line and `phi` round it (0 the creature's left, a
    /// quarter turn on the front), `inflate` out.
    fn point(&self, a: f32, phi: f32, inflate: f32) -> V3 {
        let e = 2.0 / self.squareness.max(0.5);
        let (s, c) = phi.sin_cos();
        let w = (keyed(&self.half_width, a) + inflate).max(0.0);
        let depth = if s >= 0.0 { keyed(&self.front, a) } else { keyed(&self.back, a) } + inflate;
        let spow = |x: f32| x.signum() * x.abs().powf(e);
        let centre = self.centre(a);
        vec3::add(vec3::add(centre, [w * spow(c), 0.0, 0.0]), vec3::scale(self.forward(a), depth.max(0.0) * spow(s)))
    }

    /// The outward direction of the skin at `a`, `phi` (the ring's, near enough).
    fn normal(&self, a: f32, phi: f32) -> V3 {
        let p = self.point(a, phi, 0.0);
        vec3::normalize_or(vec3::sub(p, self.centre(a)), self.forward(a))
    }

    /// The loft from distance `a0` to `a1`, closed at both ends. `dip` is how far the first
    /// `overlap` of it sinks inside the surface (reaching nothing at its far end), so a piece
    /// that sits on another reaches into it and does not open when bent.
    fn mesh(&self, a0: f32, a1: f32, rows: usize, cols: usize, overlap: f32, dip: f32) -> MeshBuilder {
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

/// Where everything of a troll sits: the torso's loft and its stations, the neck and head, the
/// limbs' girths, and the joints as the skeleton's poses take them.
struct Layout {
    body: Body,
    dims: SkeletonDims,
    torso: Loft,
    /// Distances along the torso's line: the hip, the waist (the spine bone's joint), the chest
    /// (the ribcage's) and the neck's base.
    stations: [f32; 4],
    atlas: V3,
    neck_radius: f32,
    head_centre: V3,
    /// How far the head leans forward, radians.
    head_lean: f32,
    /// Half width, half height and half depth of the head.
    head: (f32, f32, f32),
    /// The right then left shoulder balls' radius, and the girth of an upper arm.
    limb: f32,
    hand_length: f32,
}

impl Layout {
    fn new(state: &TrollState) -> Layout {
        let body = state.body();
        let t = body.trunk_length;
        let hip_y = body.hip_y;
        let sw = body.shoulder_half_width;
        let (hip, waist, chest) = (body.hip, body.waist, body.chest);
        let belly = 0.95 * state.build;
        // The stations along the torso's line, from the crotch.
        let a_h = 0.16 * t;
        let a_s = a_h + 0.30 * t;
        let a_r = a_h + 0.60 * t;
        let a_n = a_h + 1.04 * t;
        let lean_top = 0.58 + 0.24 * state.stoop;
        let tilt = vec![(0.0, 0.0), (a_h, 0.03), (a_s, 0.2 * lean_top + 0.02), (a_r, 0.58 * lean_top), (a_n, lean_top)];
        let half_width = vec![
            (0.0, 0.5 * hip.half_width),
            (a_h, 1.05 * hip.half_width),
            (a_s, waist.half_width * (1.3 + 0.35 * state.build)),
            (a_r, chest.half_width * (1.3 + 0.1 * state.build)),
            (a_n - 0.2 * t, sw * 1.12),
            (a_n - 0.08 * t, sw * 0.95),
            (a_n, 0.7 * sw),
        ];
        let front = vec![
            (0.0, 0.5 * hip.half_depth),
            (a_h, 1.1 * hip.half_depth),
            (a_s, waist.half_depth * (1.15 + belly)),
            (a_r, chest.half_depth * (1.1 + 0.45 * belly)),
            (a_n - 0.2 * t, chest.half_depth * 0.9),
            (a_n - 0.08 * t, chest.half_depth * 0.75),
            (a_n, chest.half_depth * 0.6),
        ];
        let back = vec![
            (0.0, 0.7 * hip.half_depth),
            (a_h, 1.35 * hip.half_depth),
            (a_s, waist.half_depth * 1.0),
            (a_r, chest.half_depth * 1.18),
            (a_n - 0.22 * t, chest.half_depth * (1.7 + 0.3 * state.stoop)),
            (a_n - 0.08 * t, chest.half_depth * (1.45 + 0.2 * state.stoop)),
            (a_n, chest.half_depth * 1.05),
        ];
        let torso = Loft::new([0.0, hip_y - a_h, 0.0], a_n, tilt, half_width, front, back, 2.4);

        // The neck rises from the hump, thicker than a human's, and thrusts the head forward.
        let head_form = body.head;
        let neck_radius = 1.8 * body.neck_radius;
        let c7 = torso.centre(a_n);
        let neck_len = 1.1 * head_form.half_height;
        let neck_lean = lean_top - 0.12;
        let atlas = vec3::add(c7, [0.0, neck_lean.cos() * neck_len, neck_lean.sin() * neck_len]);
        let head_lean = (0.5 * lean_top - 0.1).max(0.05);
        let head = (1.45 * head_form.half_width, 1.3 * head_form.half_height, 1.4 * head_form.half_depth);
        let head_centre = vec3::add(atlas, [0.0, 0.55 * head.1 * head_lean.cos(), 0.55 * head.1 * head_lean.sin() + 0.25 * head.2]);

        // The skeleton's joints, moved to the troll's.
        let mut dims = SkeletonDims::of(body.clone());
        let a_sh = a_n - 0.16 * t;
        let limb = 0.5 * (body.arm(Side::Right).radius + body.arm(Side::Left).radius) * (1.3 + 0.4 * state.build);
        let mut arms = dims.arms;
        for (k, side) in Side::BOTH.into_iter().enumerate() {
            let [s0, e0, w0] = body.arm(side).joints;
            let c = torso.centre(a_sh);
            let shoulder = vec3::add(c, [side.sign() * (sw * 0.98 + 0.1 * limb), -0.1 * t, 0.12 * t]);
            let elbow = vec3::add(shoulder, vec3::scale(vec3::sub(e0, s0), state.reach));
            let wrist = vec3::add(elbow, vec3::scale(vec3::sub(w0, e0), state.reach));
            arms[k] = [shoulder, elbow, wrist];
        }
        dims.arms = arms;
        dims.foot_length *= 1.2;
        let hinge_local = [0.0, -0.18 * head.1, -0.3 * head.2];
        let (sin, cos) = head_lean.sin_cos();
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
        let hand_length = 0.105 * body.height * body.race().arm * 1.2;
        Layout { body, dims, torso, stations: [a_h, a_s, a_r, a_n], atlas, neck_radius, head_centre, head_lean, head, limb, hand_length }
    }
}

// ── Rig ──

/// What the build carries along: the skin's colours.
struct Kit<'a> {
    state: &'a TrollState,
}

impl Kit<'_> {
    fn skin(&self, mesh: MeshBuilder, name: &'static str) -> Part {
        part(mesh, name, MaterialKind::Skin, self.state.skin_colour(), true)
    }

    fn detail(&self, mesh: MeshBuilder, name: &'static str, tint: Rgb) -> Part {
        part(mesh, name, MaterialKind::Skin, tint, false)
    }

    fn claws(&self, mesh: MeshBuilder) -> Part {
        part(mesh, "claws", MaterialKind::Bone, Rgb::hex(0x4a4132), false)
    }
}

/// A part, or none when its mesh is empty.
fn some(parts: &mut Vec<Part>, mesh: MeshBuilder, make: impl FnOnce(MeshBuilder) -> Part) {
    if !mesh.is_empty() {
        parts.push(make(mesh));
    }
}

/// A tube through `points`, smoothed, `radius` along it.
fn limb_tube(points: &[V3], sides: usize, radius: impl Fn(f32) -> f32) -> MeshBuilder {
    let path = CatmullRom::centripetal(points.to_vec()).sample_evenly(28);
    tube(&path, sides, &SweepOptions::default(), radius)
}

/// A ball at `at`.
fn ball(at: V3, r: f32, squash: V3) -> MeshBuilder {
    let mut m = uv_sphere(r, 14, 10);
    m.scale(squash).translate(at);
    m
}

/// A flattened blot (or a wart) standing out of a surface at `at`, facing `normal`, `r` across.
fn blot(at: V3, normal: V3, r: f32, height: f32) -> MeshBuilder {
    let mut m = uv_sphere(1.0, 8, 6);
    m.scale([r, height, r]).place(vec3::sub(at, vec3::scale(normal, 0.3 * height)), normal);
    m
}

/// The troll's skin, one bone's worth: `mesh` shifted to be about `origin`.
fn about(mut mesh: MeshBuilder, origin: V3) -> MeshBuilder {
    mesh.translate(vec3::neg(origin));
    mesh
}

fn build(state: &TrollState) -> Rig {
    let l = Layout::new(state);
    let d = &l.dims;
    let kit = Kit { state };
    let mut rng = Mulberry32::new(state.seed ^ TROLL_SALT ^ 0xB0D7);
    let [a_h, a_s, a_r, a_n] = l.stations;
    let mut meshes: Vec<Vec<Part>> = (0..index::COUNT).map(|_| Vec::new()).collect();
    // Warts and blotches, laid on each bone's surface by where they fall.
    let mut warts: Vec<MeshBuilder> = (0..index::COUNT).map(|_| MeshBuilder::new()).collect();
    let mut blotches: Vec<MeshBuilder> = (0..index::COUNT).map(|_| MeshBuilder::new()).collect();
    let girth = l.body.girth;

    // ── Torso: pelvis, belly and chest, one surface cut at the joints ──
    let overlap = 0.12 * l.body.trunk_length;
    let dip = 0.07 * girth;
    meshes[index::PELVIS] = vec![kit.skin(about(l.torso.mesh(0.0, a_s, 22, 40, 0.0, 0.0), d.origins[index::PELVIS]), "skin")];
    meshes[index::SPINE] = vec![kit.skin(about(l.torso.mesh(a_s - overlap, a_r, 22, 40, overlap, dip), d.origins[index::SPINE]), "skin")];
    let mut ribs = l.torso.mesh(a_r - overlap, a_n, 26, 40, overlap, dip);
    // Muscle and fat over the shoulders: a trapezius each side sloping from the neck out over the
    // arm's root, and a hump high on the back, so the stoop reads massive and the arms join it.
    {
        let t = l.body.trunk_length;
        let a_t = a_n - 0.14 * t;
        let c = l.torso.centre(a_t);
        let (fwd, up) = (l.torso.forward(a_t), l.torso.along(a_t));
        let sw = l.body.shoulder_half_width;
        for sg in [-1.0, 1.0] {
            let at = vec3::add(vec3::add(c, [sg * 0.62 * sw, 0.0, 0.0]), vec3::add(vec3::scale(up, 0.02 * t), vec3::scale(fwd, -0.05 * t)));
            ribs.append(&ball(at, 0.5 * sw, [1.15, 0.7, 1.0]));
        }
        let hump = vec3::add(c, vec3::add(vec3::scale(up, 0.03 * t), vec3::scale(fwd, -0.12 * t)));
        ribs.append(&ball(hump, 0.62 * sw, [1.1, 0.85, 0.95]));
    }
    meshes[index::RIBCAGE] = vec![kit.skin(about(ribs, d.origins[index::RIBCAGE]), "skin")];
    // The legs' tops are part of the pelvis: the thighs' roots swell out of the hips.
    let _ = a_h;

    // Marks on the torso: each lands on the bone whose stretch it falls in.
    let total_marks = state.warts as usize;
    for k in 0..total_marks + (state.mottle * 14.0) as usize {
        let wart = k < total_marks;
        // Below the hump and the trapezius, which cover the top of the back.
        let a = rng.range_f32(0.1 * a_n, a_n - 0.2 * (a_n - a_r) - 0.12 * l.body.trunk_length);
        // Mostly the back and the shoulders, some the belly.
        let phi = if rng.chance(0.55) { rng.range_f32(-PI + 0.3, -0.3) } else { rng.range_f32(0.2, PI - 0.2) };
        let n = l.torso.normal(a, phi);
        let at = l.torso.point(a, phi, 0.0);
        let bone = if a < a_s { index::PELVIS } else if a < a_r { index::SPINE } else { index::RIBCAGE };
        let r = if wart { rng.range_f32(0.07, 0.16) * girth } else { rng.range_f32(0.35, 0.8) * girth };
        let mark = if wart { blot(at, n, r, 1.1 * r) } else { blot(at, n, r, 0.12 * r) };
        let mut mark = mark;
        mark.translate(vec3::neg(d.origins[bone]));
        if wart { warts[bone].append(&mark) } else { blotches[bone].append(&mark) };
    }

    // ── Neck ──
    {
        let c7 = d.origins[index::NECK];
        let mid = vec3::lerp(c7, l.atlas, 0.5);
        let mut mesh = limb_tube(&[vec3::sub(c7, [0.0, 0.3, 0.0]), c7, mid, vec3::add(l.atlas, [0.0, 0.2, 0.0])], 22, |t| l.neck_radius * (1.12 - 0.22 * t));
        mesh.append(&ball(c7, 1.05 * l.neck_radius, [1.3, 0.9, 1.1]));
        meshes[index::NECK] = vec![kit.skin(about(mesh, c7), "skin")];
    }

    head(&l, state, &kit, &mut rng, &mut meshes);

    // ── Arms and hands ──
    for (k, side) in Side::BOTH.into_iter().enumerate() {
        let sg = side.sign();
        let [shoulder, elbow, wrist] = d.arms[k];
        let ub = arm_bone(side);
        let r0 = l.limb;
        // Upper arm: a swelling shoulder tapering to the elbow, a ball at each end.
        let up = vec3::normalize_or(vec3::sub(elbow, shoulder), vec3::neg(vec3::Y));
        let root = vec3::sub(shoulder, vec3::add(vec3::scale(up, 0.3 * r0), [sg * 0.5 * r0, 0.0, 0.0]));
        let mut mesh = limb_tube(&[root, shoulder, vec3::lerp(shoulder, elbow, 0.5), elbow], 20, |t| r0 * (1.15 - 0.4 * t + 0.14 * (PI * t).sin()));
        mesh.append(&ball(shoulder, 1.22 * r0, [1.0, 1.0, 1.0]));
        // A deltoid reaching in over the trunk and up under the trapezius, so the shoulder stays
        // covered however the arm swings.
        mesh.append(&ball(vec3::add(shoulder, [-sg * 0.45 * r0, 0.3 * r0, -0.05 * r0]), 1.5 * r0, [1.0, 1.0, 1.0]));
        mesh.append(&ball(elbow, 0.8 * r0, [1.0, 1.0, 1.0]));
        for _ in 0..(state.warts / 8) {
            let t = rng.range_f32(0.2, 0.9);
            let around = rng.range_f32(0.0, TAU);
            let (n, at) = tube_surface(&[shoulder, elbow], t, around, r0 * (1.15 - 0.4 * t));
            let mark = blot(at, n, rng.range_f32(0.07, 0.14) * girth, 0.14 * girth);
            warts[ub].append(&about(mark, d.origins[ub]));
        }
        meshes[ub] = vec![kit.skin(about(mesh, d.origins[ub]), "skin")];

        // Forearm: heavier than the upper arm, tapering to a thick wrist.
        let r1 = 0.8 * r0;
        let mut mesh = limb_tube(&[elbow, vec3::lerp(elbow, wrist, 0.3), vec3::lerp(elbow, wrist, 0.7), wrist], 20, |t| r1 * (1.0 + 0.18 * (PI * t.sqrt()).sin() - 0.42 * t));
        mesh.append(&ball(elbow, 0.8 * r0, [1.0, 1.0, 1.0]));
        mesh.append(&ball(wrist, 0.58 * r0, [1.0, 1.0, 1.0]));
        meshes[ub + 1] = vec![kit.skin(about(mesh, d.origins[ub + 1]), "skin")];

        // Hand: built in hand space (+Y to the fingers, +Z the back, the thumb to -X), a left one
        // mirrored, then set on the wrist along the forearm.
        let forearm = vec3::normalize_or(vec3::sub(wrist, elbow), vec3::neg(vec3::Y));
        let out = [sg, 0.0, 0.0];
        let z = vec3::normalize_or(vec3::sub(out, vec3::scale(forearm, vec3::dot(out, forearm))), vec3::Z);
        let frame = Placement::new(wrist, vec3::cross(forearm, z), forearm, z);
        let (mut hand, mut claws) = hand(state, l.hand_length, 0.58 * r0, k as u32, &mut rng);
        for m in [&mut hand, &mut claws] {
            if sg > 0.0 {
                m.scale([-1.0, 1.0, 1.0]);
            }
            m.transform(&frame.rotation);
        }
        meshes[ub + 2] = vec![part(hand, "paw", MaterialKind::Skin, state.skin_colour(), true), kit.claws(claws)];
    }

    // ── Legs and feet ──
    for (k, side) in Side::BOTH.into_iter().enumerate() {
        let sg = side.sign();
        let [hip, knee, ankle] = d.legs[k];
        let tb = leg_bone(side);
        let leg_r = l.body.leg(side).radius * (1.3 + 0.35 * state.build);
        // Thigh: from inside the hip, a heavy swell to the knee.
        let root = vec3::add(hip, [-sg * 0.3 * leg_r, 0.5 * leg_r, 0.0]);
        let mut mesh = limb_tube(&[root, hip, vec3::lerp(hip, knee, 0.5), knee], 22, |t| leg_r * (1.12 - 0.34 * t));
        mesh.append(&ball(hip, 1.05 * leg_r, [1.0, 1.0, 1.0]));
        mesh.append(&ball(knee, 0.78 * leg_r, [1.0, 1.0, 1.0]));
        for _ in 0..(state.warts / 10) {
            let t = rng.range_f32(0.2, 0.9);
            let around = rng.range_f32(0.0, TAU);
            let (n, at) = tube_surface(&[hip, knee], t, around, leg_r * (1.12 - 0.34 * t));
            warts[tb].append(&about(blot(at, n, rng.range_f32(0.07, 0.14) * girth, 0.14 * girth), d.origins[tb]));
        }
        meshes[tb] = vec![kit.skin(about(mesh, d.origins[tb]), "skin")];

        // Shin: a calf behind, narrowing to the ankle.
        let r2 = 0.72 * leg_r;
        let mut mesh = limb_tube(&[knee, vec3::lerp(knee, ankle, 0.3), vec3::lerp(knee, ankle, 0.7), ankle], 20, |t| r2 * (1.0 + 0.2 * (PI * t.sqrt()).sin() - 0.5 * t));
        mesh.append(&ball(knee, 0.78 * leg_r, [1.0, 1.0, 1.0]));
        mesh.append(&ball(ankle, 0.52 * leg_r, [1.0, 1.0, 1.0]));
        meshes[tb + 1] = vec![kit.skin(about(mesh, d.origins[tb + 1]), "skin")];

        // Foot, in the foot's own frame, its sole flat on the floor.
        let (mut foot_mesh, mut foot_claws) = foot(state, d.foot_length, ankle[1], leg_r);
        for m in [&mut foot_mesh, &mut foot_claws] {
            m.rotate_y(sg * 0.12);
        }
        if let Some((lo, _)) = foot_mesh.bounds() {
            for m in [&mut foot_mesh, &mut foot_claws] {
                m.translate([0.0, -ankle[1] - lo[1], 0.0]);
            }
        }
        meshes[tb + 2] = vec![part(foot_mesh, "paw", MaterialKind::Skin, state.skin_colour(), true), kit.claws(foot_claws)];
    }

    // Blotches and warts: after the surfaces, so each lies over its own bone.
    for bone in 0..index::COUNT {
        let (w, b) = (std::mem::take(&mut warts[bone]), std::mem::take(&mut blotches[bone]));
        some(&mut meshes[bone], b, |m| kit.detail(m, "blotches", state.blotch_colour()));
        some(&mut meshes[bone], w, |m| kit.detail(m, "warts", state.wart_colour()));
    }
    let mut rig = Rig::default();
    for (bone, parts) in meshes.into_iter().enumerate() {
        rig.add(NAMES[bone], parent_of(bone), Placement::translation(d.offset(bone)), parts);
    }
    rig
}

/// The point on a tube round the line through `line` at share `t`, `around` its section, and
/// its outward direction.
fn tube_surface(line: &[V3; 2], t: f32, around: f32, radius: f32) -> (V3, V3) {
    let axis = vec3::normalize_or(vec3::sub(line[1], line[0]), vec3::neg(vec3::Y));
    let u = vec3::normalize_or(vec3::cross(axis, vec3::X), vec3::Z);
    let v = vec3::cross(axis, u);
    let n = vec3::add(vec3::scale(u, around.cos()), vec3::scale(v, around.sin()));
    let centre = vec3::lerp(line[0], line[1], t);
    (n, vec3::add(centre, vec3::scale(n, radius)))
}

// ── Head ──

/// The head and jaw. Built upright about the head's centre, then leaned and set on the atlas.
fn head(l: &Layout, state: &TrollState, kit: &Kit, rng: &mut Mulberry32, meshes: &mut [Vec<Part>]) {
    let (hw, hh, hd) = l.head;
    let d = &l.dims;
    // The upper head, from the mouth line to the crown, as a loft on the vertical.
    let y_mouth = -0.34 * hh;
    let top = 1.0 * hh;
    let cran = Loft::new(
        [0.0, y_mouth, 0.0],
        top - y_mouth,
        vec![(0.0, 0.0)],
        vec![
            (0.0, 0.5 * hw),
            (0.14 * hh, 0.82 * hw),
            (0.45 * hh, 0.98 * hw),
            (0.8 * hh, 1.0 * hw),
            (1.1 * hh, 0.9 * hw),
            (1.38 * hh, 0.62 * hw),
            (1.34 * hh + 0.08 * hh, 0.12 * hw),
        ],
        vec![
            (0.0, 0.8 * hd),
            (0.14 * hh, 0.88 * hd),
            (0.45 * hh, 0.9 * hd),
            (0.8 * hh, 0.94 * hd),
            (1.1 * hh, 0.8 * hd),
            (1.38 * hh, 0.52 * hd),
            (1.42 * hh, 0.1 * hd),
        ],
        vec![
            (0.0, 0.55 * hd),
            (0.14 * hh, 0.75 * hd),
            (0.45 * hh, 0.95 * hd),
            (0.8 * hh, 1.04 * hd),
            (1.1 * hh, 0.96 * hd),
            (1.38 * hh, 0.62 * hd),
            (1.42 * hh, 0.1 * hd),
        ],
        2.2,
    );
    let place = |p: V3| -> V3 {
        let (s, c) = l.head_lean.sin_cos();
        vec3::add(l.head_centre, [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c])
    };
    let finish = |mut m: MeshBuilder| -> MeshBuilder {
        m.rotate_x(l.head_lean).translate(vec3::sub(l.head_centre, d.origins[index::SKULL]));
        m
    };

    let mut skin = cran.mesh(0.0, cran.length, 26, 40, 0.0, 0.0);
    // Cheekbones and the heavy brow ridge.
    let brow_y = 0.52 * hh;
    let brow_z = |x: f32| (0.9 - 0.25 * (x / hw).powi(2)) * hd;
    let brow_pts: Vec<V3> = (0..=8).map(|i| {
        let x = (i as f32 / 8.0 * 2.0 - 1.0) * 0.86 * hw;
        [x, brow_y + 0.06 * hh * (1.0 - (x / hw).abs()), brow_z(x) - 0.04 * hd]
    }).collect();
    let brow_r = (0.2 + 0.12 * state.brow) * hh * 0.5;
    skin.append(&limb_tube(&brow_pts, 12, |t| brow_r * (0.8 + 0.4 * (PI * t).sin().powf(0.4))));
    for sg in [-1.0f32, 1.0] {
        skin.append(&ball([sg * 0.62 * hw, 0.0, 0.62 * hd], 0.24 * hh, [1.0, 0.8, 0.9]));
    }
    // A broad muzzle and the upper lip, over the teeth.
    skin.append(&ball([0.0, -0.12 * hh, 0.72 * hd], 0.38 * hh, [1.5, 0.8, 1.0]));
    // Nose: hooked, long, drooping over the lip.
    let nose_len = (0.5 + 0.7 * state.nose) * hh;
    let nose_root = [0.0, 0.42 * hh, 0.88 * hd];
    let nose_path = [
        nose_root,
        [0.0, 0.18 * hh, 1.08 * hd + 0.2 * nose_len * 0.3],
        [0.0, -0.02 * hh, 1.14 * hd + 0.5 * nose_len * 0.5],
        [0.0, -0.22 * hh - 0.1 * nose_len * 0.5, 1.05 * hd + 0.5 * nose_len * 0.5],
    ];
    let nr = 0.17 * hh;
    skin.append(&limb_tube(&nose_path, 14, |t| nr * (0.8 + 0.55 * t - 0.2 * t * t)));
    let tip = nose_path[3];
    skin.append(&ball(tip, 1.2 * nr, [1.0, 0.9, 1.0]));
    for sg in [-1.0f32, 1.0] {
        skin.append(&ball(vec3::add(tip, [sg * 1.0 * nr, 0.1 * nr, -0.3 * nr]), 0.78 * nr, [1.0, 0.85, 0.9]));
    }
    // Ears: long, pointed and swept back from the sides of the head.
    let ear_len = (0.75 + 0.8 * state.ears) * hh;
    for sg in [-1.0f32, 1.0] {
        let mut ear = spike(0.3 * hh, ear_len, 12, 0.35, Shading::SMOOTH);
        ear.scale([0.35, 1.0, 1.0]);
        ear.rotate_z(-sg * (FRAC_PI_2 - 0.15));
        ear.rotate_y(sg * 0.5);
        ear.translate([sg * 0.9 * hw, 0.38 * hh, -0.18 * hd]);
        ear.rotate_x(0.0);
        skin.append(&ear);
    }
    let mut parts = vec![part(finish(skin), "head", MaterialKind::Skin, state.skin_colour(), true)];

    // Eyes: small and deep under the brow, a dark pupil and a glint.
    let (mut balls, mut pupils, mut glints) = (MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new());
    let er = 0.115 * hh;
    for sg in [-1.0f32, 1.0] {
        let x = sg * 0.4 * hw;
        let a = cran_y(&cran, 0.3 * hh);
        let z = cran.point(a, FRAC_PI_2 - sg.signum() * ((x / hw).asin()), 0.0)[2].min(0.98 * hd) - 0.03 * hd;
        let at = [x, 0.3 * hh, z.max(0.55 * hd)];
        let look = vec3::normalize_or([0.18 * sg, -0.05, 1.0], vec3::Z);
        let (b, g) = eye(at, er, look, [0.0, 0.6, 0.8]);
        balls.append(&b);
        glints.append(&g);
        let mut pupil = uv_sphere(0.5 * er, 8, 6);
        pupil.scale([0.7, 1.0, 0.5]).translate(vec3::add(at, vec3::scale(look, 0.78 * er)));
        pupils.append(&pupil);
    }
    parts.push(part(finish(balls), "eyes", MaterialKind::Gem, state.eye_colour(), true).with_glow(0.35));
    parts.push(part(finish(pupils), "pupils", MaterialKind::Gem, Rgb::hex(0x140d0a), false));
    parts.push(part(finish(glints), "glints", MaterialKind::Gem, Rgb::hex(0xfff6dc), false).with_glow(0.5));

    // Mouth: a dark slot inside the lips, which shows as the jaw drops.
    let mut mouth = ellipsoid([0.62 * hw, 0.12 * hh, 0.6 * hd]);
    mouth.translate([0.0, -0.3 * hh, 0.36 * hd]);
    parts.push(part(finish(mouth), "mouth", MaterialKind::Skin, Rgb::hex(0x3a1a18), false));

    // Warts on the face.
    let mut face_warts = MeshBuilder::new();
    for _ in 0..(state.warts / 3) {
        let y = rng.range_f32(-0.2 * hh, 0.9 * hh);
        let phi = rng.range_f32(0.35, PI - 0.35);
        let a = cran_y(&cran, y);
        let p = cran.point(a, phi, 0.0);
        let n = cran.normal(a, phi);
        face_warts.append(&blot(p, n, rng.range_f32(0.04, 0.09) * hh, 0.08 * hh));
    }
    some(&mut parts, finish(face_warts), |m| kit.detail(m, "warts", state.wart_colour()));

    // Upper teeth: a few short fangs.
    let mut teeth = MeshBuilder::new();
    for k in 0..4 {
        let x = (k as f32 - 1.5) * 0.22 * hw;
        let mut tooth = spike(0.045 * hh, 0.16 * hh, 6, 0.0, Shading::SMOOTH);
        tooth.rotate_x(PI).translate([x, -0.3 * hh, 0.9 * hd - 0.1 * (x / hw).abs() * hd]);
        teeth.append(&tooth);
    }
    parts.push(part(finish(teeth), "teeth", MaterialKind::Bone, Rgb::hex(0xd8cfa8), false));

    // Hair: lank strands from the crown and the back of the head, hanging with gravity.
    if state.hair > 0.0 {
        let mut hair = MeshBuilder::new();
        let strands = (10.0 + 30.0 * state.hair) as usize;
        for _ in 0..strands {
            let y = rng.range_f32(0.5 * hh, 1.3 * hh);
            let phi = rng.range_f32(PI + 0.2, TAU - 0.2).clamp(PI + 0.2, TAU - 0.2);
            let phi = if rng.chance(0.3) { rng.range_f32(0.1, PI - 0.1) } else { phi };
            let a = cran_y(&cran, y);
            let root = place(vec3::scale(cran.point(a, phi, -0.02 * hh), 1.0));
            let out = cran.normal(a, phi);
            let out = [out[0], 0.0, out[2]];
            let len = rng.range_f32(0.8, 2.4) * hh * (0.5 + 0.5 * state.hair);
            let sway = rng.range_f32(-0.2, 0.2);
            let n = 5;
            let path: Vec<V3> = (0..=n)
                .map(|i| {
                    let s = i as f32 / n as f32;
                    vec3::add(root, vec3::add(vec3::scale(out, 0.3 * hh * (s * (1.2 - s)) * 2.0), [sway * s * hh, -len * s, -0.15 * len * s * s]))
                })
                .collect();
            hair.append(&tube(&path, 5, &SweepOptions::default(), |s| 0.05 * hh * (1.0 - 0.8 * s)));
        }
        hair.translate(vec3::neg(d.origins[index::SKULL]));
        parts.push(part(hair, "hair", MaterialKind::Hair, state.hair_colour(), false));
    }
    meshes[index::SKULL] = parts;

    // Jaw: a heavy lower jaw swinging from the hinge, jutting past the upper, with tusks.
    let hinge = d.origins[index::JAW];
    let (sin, cos) = l.head_lean.sin_cos();
    let to_head = |p: V3| -> V3 { [p[0], p[1] * cos - p[2] * sin, p[1] * sin + p[2] * cos] };
    let hinge_local = [0.0, -0.18 * hh, -0.3 * hd];
    let jaw_pts: Vec<V3> = (0..=10)
        .map(|i| {
            let s = i as f32 / 10.0;
            let a = (s - 0.5) * PI * 0.95;
            let x = a.sin() * 0.78 * hw;
            let z = (a.cos() * 1.28 - 0.2) * hd + 0.2 * hd * (1.0 - a.cos());
            [x, -0.5 * hh, z]
        })
        .collect();
    let jr = 0.2 * hh;
    let mut jaw = limb_tube(&jaw_pts, 14, |t| jr * (0.9 + 0.45 * (PI * t).sin() * 0.5 + 0.2 * (1.0 - (2.0 * t - 1.0).abs())));
    // The flesh under it and the chin.
    let mut chin = uv_sphere(1.0, 14, 10);
    chin.scale([0.64 * hw, 0.3 * hh, 0.62 * hd]).translate([0.0, -0.5 * hh, 0.82 * hd]);
    jaw.append(&chin);
    for sg in [-1.0f32, 1.0] {
        jaw.append(&ball([sg * 0.74 * hw, -0.42 * hh, 0.0], 0.24 * hh, [1.0, 1.3, 1.0]));
    }
    let mut jaw_parts = Vec::new();
    let to_jaw = |mut m: MeshBuilder| -> MeshBuilder {
        m.rotate_x(l.head_lean).translate(vec3::sub(l.head_centre, hinge));
        m
    };
    // The jaw mesh is built in head-local space; hinge_local is where it swings from.
    let _ = (hinge_local, to_head);
    jaw_parts.push(part(to_jaw(jaw), "jaw", MaterialKind::Skin, state.skin_colour(), true));
    // Lower teeth and tusks.
    let mut lower = MeshBuilder::new();
    for k in 0..6 {
        let s = k as f32 / 5.0;
        let a = (s - 0.5) * PI * 0.7;
        let (x, z) = (a.sin() * 0.62 * hw, (a.cos() * 1.1 - 0.18) * hd + 0.2 * hd * (1.0 - a.cos()));
        let mut tooth = spike(0.04 * hh, 0.13 * hh, 6, 0.0, Shading::SMOOTH);
        tooth.translate([x, -0.4 * hh, z]);
        lower.append(&tooth);
    }
    jaw_parts.push(part(to_jaw(lower), "teeth", MaterialKind::Bone, Rgb::hex(0xd8cfa8), false));
    if state.tusks > 0.0 {
        let mut tusks = MeshBuilder::new();
        for sg in [-1.0f32, 1.0] {
            let mut tusk = spike(0.085 * hh, (0.3 + 0.55 * state.tusks) * hh, 8, 0.6, Shading::SMOOTH);
            tusk.rotate_z(-sg * 0.14);
            tusk.rotate_x(0.12);
            tusk.translate([sg * 0.34 * hw, -0.45 * hh, 1.0 * hd]);
            tusks.append(&tusk);
        }
        jaw_parts.push(part(to_jaw(tusks), "tusks", MaterialKind::Bone, Rgb::hex(0xe0d6ae), false));
    }
    meshes[index::JAW] = jaw_parts;
}

/// The distance along `loft` at height `y` (its line is vertical).
fn cran_y(loft: &Loft, y: f32) -> f32 {
    (y - loft.samples[0][1]).clamp(0.0, loft.length)
}

// ── Hands and feet ──

/// A hand and its claws in hand space: origin at the wrist, +Y toward the fingers, +Z the back
/// of the hand, the thumb toward -X (a right hand). `length` is wrist to middle fingertip,
/// `wrist` the wrist's radius.
fn hand(state: &TrollState, length: f32, wrist: f32, side: u32, rng: &mut Mulberry32) -> (MeshBuilder, MeshBuilder) {
    let l = length;
    let mut flesh = MeshBuilder::new();
    let mut claws = MeshBuilder::new();
    // The palm: a thick slab at the wrist's end.
    let half = 0.34 * l;
    let mut palm = uv_sphere(1.0, 16, 10);
    palm.scale([half, 0.3 * l, 0.17 * l]).translate([0.0, 0.2 * l, 0.0]);
    flesh.append(&palm);
    flesh.append(&ball([0.0, 0.0, 0.0], 1.05 * wrist, [1.0, 1.0, 1.0]));
    let fr = 0.072 * l;
    let claw_len = (0.1 + 0.07 * state.claws) * l;
    // (x of the knuckle in half widths, length share, curl)
    let fingers = [(-0.7f32, 0.9f32), (-0.24, 1.0), (0.24, 0.94), (0.7, 0.78)];
    for (i, &(kx, scale)) in fingers.iter().enumerate() {
        let knuckle = [kx * half, 0.4 * l, 0.0];
        let mut at = knuckle;
        let mut angle = 0.0;
        let mut pts = vec![[kx * half * 0.8, 0.3 * l, 0.0], knuckle];
        let mut dir = [0.0, 1.0, 0.0];
        for (k, share) in [0.22f32, 0.19, 0.17].into_iter().enumerate() {
            angle += (0.28 + 0.5 * k as f32 * 0.5) * (0.9 + 0.2 * rng.range_f32(0.0, 1.0)) + 0.05 * side as f32;
            let len = l * share * scale;
            dir = [0.0, angle.cos(), -angle.sin()];
            at = vec3::add(at, vec3::scale(dir, len));
            pts.push(at);
        }
        flesh.append(&limb_tube(&pts, 9, |t| fr * (1.0 - 0.38 * t)));
        for p in &pts[2..] {
            flesh.append(&ball(*p, fr * 0.95, [1.0, 1.0, 1.0]));
        }
        let mut claw = spike(0.55 * fr, claw_len, 7, 0.9, Shading::SMOOTH);
        claw.place(at, vec3::normalize_or(vec3::add(dir, [0.0, 0.0, -0.35]), dir));
        claws.append(&claw);
        let _ = i;
    }
    // The thumb: from the base of the palm out and forward.
    let thumb = [[-0.28 * half, 0.1 * l, -0.02 * l], [-half, 0.28 * l, -0.07 * l], [-1.25 * half, 0.44 * l, -0.16 * l], [-1.2 * half, 0.56 * l, -0.26 * l]];
    flesh.append(&limb_tube(&thumb, 9, |t| 1.15 * fr * (1.0 - 0.4 * t)));
    for p in &thumb[1..] {
        flesh.append(&ball(*p, fr * 0.95, [1.0, 1.0, 1.0]));
    }
    let mut claw = spike(0.6 * fr, 0.9 * claw_len, 7, 0.9, Shading::SMOOTH);
    claw.place(thumb[3], vec3::normalize_or([0.1, 0.8, -0.5], vec3::Y));
    claws.append(&claw);
    (flesh, claws)
}

/// A foot and its claws in the foot's frame: origin at the ankle, +Z toward the toes. Broad
/// and flat, with three clawed toes in front and a heel behind. The sole's height is for the
/// caller to set.
fn foot(state: &TrollState, length: f32, ankle_height: f32, leg_r: f32) -> (MeshBuilder, MeshBuilder) {
    let mut flesh = MeshBuilder::new();
    let mut claws = MeshBuilder::new();
    let w = 0.2 * length * (1.0 + 0.2 * state.build);
    let top = -ankle_height;
    // The heel and instep.
    let mut heel = uv_sphere(1.0, 14, 10);
    heel.scale([0.8 * w, 0.62 * ankle_height + 0.0, 0.34 * length]).translate([0.0, top + 0.62 * ankle_height, 0.0]);
    flesh.append(&heel);
    // The ball of the foot.
    let mut front = uv_sphere(1.0, 14, 10);
    front.scale([1.1 * w, 0.42 * ankle_height + 0.0, 0.26 * length]).translate([0.0, top + 0.42 * ankle_height, 0.34 * length]);
    flesh.append(&front);
    let _ = leg_r;
    // Toes: four fat ones, the middle two longest, each ending in a claw.
    for k in 0..4 {
        let x = (k as f32 - 1.5) * 0.52 * w;
        let reach = [0.72, 0.85, 0.85, 0.7][k] * length;
        let r = 0.24 * w * if k == 0 || k == 3 { 0.9 } else { 1.0 };
        let base = [x * 0.8, top + r, 0.44 * length];
        let tip = [x, top + r, reach];
        let mut toe = limb_tube(&[base, vec3::lerp(base, tip, 0.55), tip], 10, |t| r * (1.0 - 0.15 * t));
        toe.append(&ball(tip, r, [1.0, 1.0, 1.0]));
        flesh.append(&toe);
        let mut claw = spike(0.55 * r, (0.12 + 0.08 * state.claws) * length, 7, 0.9, Shading::SMOOTH);
        claw.place(vec3::add(tip, [0.0, 0.05 * r, 0.6 * r]), vec3::normalize_or([0.0, -0.12, 1.0], vec3::Z));
        claws.append(&claw);
    }
    (flesh, claws)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pose::{skeleton_with, SkeletonPose};
    use crate::{Creature, Species};

    #[test]
    fn the_same_seed_gives_the_same_troll() {
        for seed in [0, 1, 7, 1207, u32::MAX] {
            let (a, b) = (TrollState::roll(seed), TrollState::roll(seed));
            assert_eq!(a, b);
            assert_eq!(a.build(), b.build(), "seed {seed}");
            assert_eq!(Creature::of(Species::Troll, seed), Creature::Troll(a));
        }
        assert_ne!(TrollState::roll(1), TrollState::roll(2));
        assert_ne!(TrollState::roll(1).build(), TrollState::roll(2).build());
        let _ = skeleton_with;
        let _ = SkeletonPose::default;
    }
}
