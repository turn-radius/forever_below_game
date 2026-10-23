//! Minotaurs: [`MinotaurState`] from a seed and its [`Rig`] of named bones. The first of the
//! denizens, the intelligent folk of the deeper levels, and the pattern the others follow.
//!
//! A minotaur is laid out on a human's joints as a troll is, by the denizens' shared
//! [`Frame`](crate::denizen::Frame): the same eighteen bones in the skeleton's order
//! ([`crate::skeleton::index`]), then a tail of three more ([`crate::denizen::TAIL_FIRST`]). It
//! stands upright but hunched, half again a man's height, with a deep chest, heavy shoulders and a
//! hump of muscle at the withers that the neck rises from, thick as the skull it carries. Its
//! arms are longer than a man's and end in heavy hands. Its legs are digitigrade: a short thigh
//! sloping forward, a long shin back to a high hock, and a pastern forward again to a cloven
//! hoof, so the skeleton's walk ([`crate::pose::minotaur_with`]) steps on its toes.
//!
//! The head is a bull's: a broad skull and a long, deep muzzle lofted along one line, a nose pad
//! with flared nostrils (and often a ring through the septum), a brow over each eye with its
//! horizontal pupil, ears set out from under the horns, a forelock and a shaggy mane down the
//! neck and over the shoulders. The horns ([`crate::denizen::horn`]) sweep out and forward and
//! up, each its own length of wear, one sometimes broken off. A bull's tail with its tuft hangs
//! behind.
//!
//! Seeded variation: height, bulk, hunch, reach, coat from black through brown and red to dun and
//! ash with its shade, a blaze, white socks or pied patches, muzzle length, the ring, ears, mane,
//! eyes, horn length, curve, spread, rise, wear and breakage, the tail's tuft and the hooves.
//! All of it our own; the proportions follow a human's.
//!
//! The SRD's Minotaur (5.1 p. 333) is a Large monstrosity with a greataxe and a gore
//! ([`GREATAXE`], [`crate::StatBlock::bite`]).

use armory::geom::vec3::{self, V3};
use armory::geom::{MeshBuilder, SweepOptions, tube, uv_sphere};
use armory::{MaterialKind, Part, Rgb};
use fb_body::{Placement, Side};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::denizen::{self, Digitigrade, Frame, FrameSpec, HandSpec, HornSpec, Legs, Loft, TailSpec, about, ball, blot, limb_tube, some, tube_surface};
use crate::rat::{mix, part};
use crate::rig::Rig;
use crate::shape::eye;
use crate::skeleton::{NAMES, arm_bone, index, leg_bone, parent_of};
use crate::{Dice, Size, Species};

/// Mixed into a minotaur's seed so it does not roll the same numbers as a body of that seed.
const MINOTAUR_SALT: u32 = 0x31_0a7a_e5;

/// The greataxe's damage on a hit: 17 (2d12 + 4) slashing ([SRD 5.1 p. 333](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=333)).
/// The gore is the stat block's [`crate::StatBlock::bite`], 13 (2d8 + 4) piercing.
pub const GREATAXE: Dice = Dice::new(2, 12, 4);

/// The base colour of a minotaur's hide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Coat {
    Black,
    Brown,
    Red,
    Dun,
    Ash,
}

impl Coat {
    pub const ALL: [Coat; 5] = [Coat::Black, Coat::Brown, Coat::Red, Coat::Dun, Coat::Ash];

    /// The hide at its middling shade. Our own picks.
    pub fn colour(self) -> Rgb {
        match self {
            Coat::Black => Rgb::hex(0x2c2420),
            Coat::Brown => Rgb::hex(0x5b3d29),
            Coat::Red => Rgb::hex(0x7d4125),
            Coat::Dun => Rgb::hex(0x9b7b55),
            Coat::Ash => Rgb::hex(0x716b64),
        }
    }
}

/// What is marked on a minotaur's hide, beyond its coat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Marking {
    /// Plain.
    None,
    /// A white stripe down the face.
    Blaze,
    /// White hands and pasterns.
    Socks,
    /// Large white patches over the body.
    Pied,
}

/// What the nose ring is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RingMetal {
    Iron,
    Brass,
}

/// The shape of a minotaur's horns, shared by the pair (each has its own wear).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Horns {
    /// 0 short to 1 long.
    pub length: f32,
    /// 0 straight to 1 a deep hook.
    pub curve: f32,
    /// 0 close and forward to 1 wide out to the sides.
    pub spread: f32,
    /// 0 level to 1 lifted high at the tip.
    pub rise: f32,
    /// How worn each tip is (right, left), 0 sharp to 1 a stub.
    pub wear: [f32; 2],
    /// A horn that is broken off: the side (0 right, 1 left) and how far along it, 0..1.
    pub broken: Option<(u8, f32)>,
}

/// One minotaur, every choice the seed makes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MinotaurState {
    pub seed: u32,
    /// Height as a share of a typical human's before its legs are lengthened and it hunches,
    /// 1.2..1.35.
    pub stature: f32,
    /// 0.6 heavy to 1 huge: the chest, shoulders and limbs.
    pub build: f32,
    /// How far it hunches over its shoulders, 0..=1.
    pub stoop: f32,
    /// How much longer its arms are than a human's, 1.1..1.25.
    pub reach: f32,
    pub coat: Coat,
    /// Within the coat, 0 darker to 1 lighter.
    pub shade: f32,
    pub marking: Marking,
    /// How long the muzzle is, 0..=1.
    pub muzzle: f32,
    /// A ring through the septum, in four of ten.
    pub ring: Option<RingMetal>,
    /// How big the ears are, 0..=1.
    pub ears: f32,
    /// How shaggy the forelock and mane are, 0..=1.
    pub mane: f32,
    /// The mane's colour, 0 the coat's own to 1 darkened toward black.
    pub mane_tone: f32,
    /// The eyes, 0 amber to 1 red.
    pub eyes: f32,
    pub horns: Horns,
    /// How full the tail's tuft is, 0..=1.
    pub tuft: f32,
    /// The hooves, 0 black to 1 a pale horn.
    pub hoof_tone: f32,
}

impl MinotaurState {
    /// A minotaur rolled from `seed`: the same seed always gives the same minotaur.
    pub fn roll(seed: u32) -> MinotaurState {
        let mut rng = Mulberry32::new(seed ^ MINOTAUR_SALT);
        let stature = rng.range_f32(1.2, 1.35);
        let build = 0.6 + 0.4 * rng.next_f64() as f32;
        let stoop = rng.next_f64() as f32;
        let reach = rng.range_f32(1.1, 1.25);
        let coat = *rng.pick(&Coat::ALL);
        let shade = rng.next_f64() as f32;
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
        let length = rng.next_f64() as f32;
        let curve = rng.next_f64() as f32;
        let spread = rng.next_f64() as f32;
        let rise = rng.next_f64() as f32;
        let wear = [rng.range_f32(0.0, 0.6), rng.range_f32(0.0, 0.6)];
        let broken = if rng.chance(0.2) { Some((rng.int(0, 1) as u8, rng.range_f32(0.35, 0.8))) } else { None };
        let tuft = rng.next_f64() as f32;
        let hoof_tone = rng.next_f64() as f32;
        MinotaurState { seed, stature, build, stoop, reach, coat, shade, marking, muzzle, ring, ears, mane, mane_tone, eyes, horns: Horns { length, curve, spread, rise, wear, broken }, tuft, hoof_tone }
    }

    pub fn species(&self) -> Species {
        Species::Minotaur
    }

    /// The size its body is drawn at: its SRD size.
    pub fn drawn_size(&self) -> Size {
        Species::Minotaur.stat_block().size
    }

    /// The hide: the coat at its shade, a little darker or lighter. Our own.
    pub fn hide_colour(&self) -> Rgb {
        let base = self.coat.colour();
        if self.shade < 0.5 { mix(base, Rgb::hex(0x14100e), 0.3 * (1.0 - self.shade * 2.0)) } else { mix(base, Rgb::hex(0xc9b79a), 0.28 * (self.shade * 2.0 - 1.0)) }
    }

    /// The white of a blaze, socks and patches.
    fn white(&self) -> Rgb {
        Rgb::hex(0xd8d0bf)
    }

    /// The mane and forelock and the tail's tuft: the hide, darkened.
    pub fn hair_colour(&self) -> Rgb {
        mix(self.hide_colour(), Rgb::hex(0x0e0b0a), 0.25 + 0.5 * self.mane_tone)
    }

    /// The irises: amber to red. Our own picks.
    pub fn eye_colour(&self) -> Rgb {
        mix(Rgb::hex(0xe0a22c), Rgb::hex(0xc4281a), self.eyes)
    }

    /// The horns: a yellowed bone at the base, dark at the tip.
    fn horn_colour(&self) -> Rgb {
        Rgb::hex(0xcfc4a6)
    }

    fn horn_tip_colour(&self) -> Rgb {
        Rgb::hex(0x2b241d)
    }

    fn hoof_colour(&self) -> Rgb {
        mix(Rgb::hex(0x1f1a17), Rgb::hex(0x8d8473), self.hoof_tone)
    }

    /// What its frame is laid out from.
    pub fn frame_spec(&self) -> FrameSpec {
        FrameSpec {
            seed: self.seed ^ MINOTAUR_SALT,
            stature: self.stature,
            frame: 0.75 + 0.3 * self.build,
            build: self.build,
            belly: 0.05 + 0.15 * (1.0 - self.build),
            lean: 0.16 + 0.2 * self.stoop,
            shoulders: 1.0 + 0.12 * self.build,
            hump: 0.4 + 0.6 * self.stoop,
            reach: self.reach,
            neck: (2.4, 1.7),
            head: (2.0, 1.25, 1.45),
            head_lean: 0.32 + 0.12 * self.stoop,
            head_set: (0.1, 1.0),
            legs: Legs::Digitigrade(Digitigrade::BULL),
            leg_lift: 0.17,
            hip_spread: 1.55,
        }
    }

    /// The frame its bones are laid out on.
    pub fn frame(&self) -> Frame {
        let mut frame = Frame::new(&self.frame_spec());
        // The jaw hinges under the back of the skull, low, where a bull's does.
        let (_, hh, hd) = frame.head;
        let (sin, cos) = frame.spec.head_lean.sin_cos();
        let (y, z) = (-0.42 * hh, -hd + 0.22 * skull_length(self, hd));
        frame.dims.hinge = vec3::add(frame.head_centre, [0.0, y * cos - z * sin, y * sin + z * cos]);
        frame.dims.origins[index::JAW] = frame.dims.hinge;
        frame
    }

    /// The minotaur's joints and measures, in the shape the skeleton's poses take them.
    pub fn dims(&self) -> crate::skeleton::SkeletonDims {
        self.frame().dims
    }

    /// The shape of one horn, `side` 0 the right and 1 the left, in a head whose half width is `hw`.
    pub fn horn_spec(&self, side: usize, hw: f32) -> HornSpec {
        let h = &self.horns;
        HornSpec {
            length: (1.5 + 1.6 * h.length) * hw,
            radius: (0.2 + 0.05 * h.length) * hw,
            rise: 0.1 + 0.22 * h.rise,
            rise_tip: 0.5 + 0.8 * h.rise + 0.25 * h.curve,
            yaw: -0.25 + 0.3 * (1.0 - h.spread),
            curl: (0.9 + 1.1 * h.curve) * (1.3 - 0.55 * h.spread),
            wear: h.wear[side],
            broken: h.broken.filter(|&(s, _)| s as usize == side).map(|(_, at)| at),
        }
    }

    /// Crown of the head to soles at rest, hunched, world units (the horns not counted).
    pub fn height(&self) -> f32 {
        let f = self.frame();
        let (_, hh, _) = f.head;
        f.head_centre[1] + hh * (0.95 * f.spec.head_lean.cos() + 0.2)
    }

    /// How far it reaches across the floor from its centre at rest: a hand hanging at its side,
    /// a hoof out in front, the muzzle thrust forward, or the horns out to the sides.
    pub fn radius(&self) -> f32 {
        let f = self.frame();
        let d = &f.dims;
        let around = |p: V3| p[0].hypot(p[2]);
        let hands = d.arms.iter().map(|a| around([a[2][0], 0.0, a[2][2]]) + f.hand_length * 0.4);
        let feet = d.ankles.iter().map(|a| around([a[0], 0.0, a[2] + d.foot_length]));
        let (_, _, hd) = f.head;
        let muzzle = skull_length(self, hd) - hd;
        let head = around([0.0, 0.0, f.head_centre[2] + muzzle * f.spec.head_lean.cos()]);
        let shoulders = d.arms.iter().map(|a| around([a[0][0], 0.0, a[0][2]]) + 0.8 * f.limb);
        hands.chain(feet).chain(shoulders).chain([head]).fold(0.0, f32::max)
    }

    /// Its bones and meshes, standing at rest.
    pub fn build(&self) -> Rig {
        build(self)
    }
}

// ── Rig ──

/// What the build carries along: the hide's colours.
struct Kit<'a> {
    state: &'a MinotaurState,
}

impl Kit<'_> {
    fn skin(&self, mesh: MeshBuilder, name: &'static str) -> Part {
        part(mesh, name, MaterialKind::Skin, self.state.hide_colour(), true)
    }

    fn detail(&self, mesh: MeshBuilder, name: &'static str, tint: Rgb) -> Part {
        part(mesh, name, MaterialKind::Skin, tint, false)
    }

    fn hair(&self, mesh: MeshBuilder) -> Part {
        part(mesh, "hair", MaterialKind::Hair, self.state.hair_colour(), false)
    }
}

fn build(state: &MinotaurState) -> Rig {
    let l = state.frame();
    let d = &l.dims;
    let kit = Kit { state };
    let mut rng = Mulberry32::new(state.seed ^ MINOTAUR_SALT ^ 0xB011);
    let [_, a_s, a_r, a_n] = l.stations;
    let mut meshes: Vec<Vec<Part>> = (0..index::COUNT).map(|_| Vec::new()).collect();
    // White patches, laid on each bone's surface by where they fall.
    let mut patches: Vec<MeshBuilder> = (0..index::COUNT).map(|_| MeshBuilder::new()).collect();
    let mut manes: Vec<MeshBuilder> = (0..index::COUNT).map(|_| MeshBuilder::new()).collect();
    let girth = l.body.girth;
    let t = l.body.trunk_length;

    // ── Torso: pelvis, belly and chest, one surface cut at the joints ──
    let overlap = 0.12 * t;
    let dip = 0.07 * girth;
    meshes[index::PELVIS] = vec![kit.skin(about(l.torso.mesh(0.0, a_s, 22, 40, 0.0, 0.0), d.origins[index::PELVIS]), "skin")];
    meshes[index::SPINE] = vec![kit.skin(about(l.torso.mesh(a_s - overlap, a_r, 22, 40, overlap, dip), d.origins[index::SPINE]), "skin")];
    let mut ribs = l.torso.mesh(a_r - overlap, a_n, 26, 40, overlap, dip);
    // The muscle over the shoulders: a trapezius each side sloping from the neck over the arm's
    // root, and the withers, a hump high on the back the neck rises from.
    {
        let a_t = a_n - 0.14 * t;
        let c = l.torso.centre(a_t);
        let (fwd, up) = (l.torso.forward(a_t), l.torso.along(a_t));
        let sw = l.body.shoulder_half_width * l.spec.shoulders;
        for sg in [-1.0, 1.0] {
            let at = vec3::add(vec3::add(c, [sg * 0.62 * sw, 0.0, 0.0]), vec3::add(vec3::scale(up, 0.02 * t), vec3::scale(fwd, -0.05 * t)));
            ribs.append(&ball(at, 0.5 * sw, [1.15, 0.75, 1.0]));
        }
        let hump = vec3::add(c, vec3::add(vec3::scale(up, 0.05 * t), vec3::scale(fwd, -0.1 * t)));
        ribs.append(&ball(hump, 0.5 * sw, [1.1, 0.95, 0.95]));
    }
    // The mane: locks over the withers and down the back between the shoulders, falling forward
    // over the hump from the neck, and a brisket of hair on the chest.
    {
        let strands = (16.0 + 56.0 * state.mane) as usize;
        for _ in 0..strands {
            let a = rng.range_f32(a_r + 0.1 * t, a_n - 0.02 * t);
            let phi = -FRAC_PI_2 + rng.range_f32(-0.7, 0.7);
            let root = l.torso.point(a, phi, -0.1 * girth);
            let out = vec3::normalize_or(vec3::add(l.torso.normal(a, phi), [0.0, 0.0, -0.4]), vec3::Y);
            let len = rng.range_f32(0.5, 1.4) * girth * (0.8 + 0.5 * state.mane);
            let sway = [rng.range_f32(-0.3, 0.3) * girth, 0.0, rng.range_f32(-0.4, 0.1) * girth];
            let mut lock = denizen::lock(root, out, len, 0.3 * girth, 1.0, sway);
            lock.translate(vec3::neg(d.origins[index::RIBCAGE]));
            manes[index::RIBCAGE].append(&lock);
        }
    }
    meshes[index::RIBCAGE] = vec![kit.skin(about(ribs, d.origins[index::RIBCAGE]), "skin")];

    // Pied patches on the torso: each lands on the bone whose stretch it falls in.
    if state.marking == Marking::Pied {
        for _ in 0..9 {
            let a = rng.range_f32(a_s, a_n - 0.25 * t);
            let phi = rng.range_f32(0.0, TAU);
            let n = l.torso.normal(a, phi);
            let at = l.torso.point(a, phi, 0.0);
            let bone = if a < a_s { index::PELVIS } else if a < a_r { index::SPINE } else { index::RIBCAGE };
            let mut mark = blot(at, n, rng.range_f32(0.5, 1.1) * girth, 0.1 * girth);
            mark.translate(vec3::neg(d.origins[bone]));
            patches[bone].append(&mark);
        }
    }

    // ── Neck: thick as the skull, with the mane down its crest ──
    {
        let c7 = d.origins[index::NECK];
        let mid = vec3::lerp(c7, l.atlas, 0.5);
        let r = l.neck_radius;
        let mut mesh = limb_tube(&[vec3::sub(c7, [0.0, 0.4, 0.0]), c7, mid, vec3::add(l.atlas, [0.0, 0.2, 0.0])], 26, |t| r * (1.12 - 0.12 * t));
        mesh.append(&ball(c7, 1.05 * r, [1.25, 0.95, 1.1]));
        let axis = vec3::normalize_or(vec3::sub(l.atlas, c7), vec3::Y);
        let back = vec3::normalize_or([0.0, axis[2], -axis[1]], vec3::neg(vec3::Z));
        for _ in 0..(10.0 + 34.0 * state.mane) as usize {
            let u = rng.range_f32(0.05, 0.95);
            let around = rng.range_f32(-0.9, 0.9);
            let centre = vec3::lerp(c7, l.atlas, u);
            let dir = vec3::normalize_or(vec3::add(vec3::scale(back, around.cos()), [around.sin(), 0.0, 0.0]), back);
            let root = vec3::add(centre, vec3::scale(dir, 0.85 * r));
            let len = rng.range_f32(0.7, 1.7) * r * (0.7 + 0.5 * state.mane);
            let mut lock = denizen::lock(root, vec3::normalize_or(vec3::add(dir, [0.0, -0.5, 0.0]), dir), len, 0.14 * r, 0.9, [0.0, 0.0, -0.2 * r]);
            lock.translate(vec3::neg(c7));
            manes[index::NECK].append(&lock);
        }
        meshes[index::NECK] = vec![kit.skin(about(mesh, c7), "skin")];
    }

    head(&l, state, &mut rng, &mut meshes);

    // ── Arms and hands ──
    let hand_spec = HandSpec { length: l.hand_length, wrist: 0.0, curl: 0.3, thick: 1.3 };
    for (k, side) in Side::BOTH.into_iter().enumerate() {
        let sg = side.sign();
        let [shoulder, elbow, wrist] = d.arms[k];
        let ub = arm_bone(side);
        let r0 = l.limb;
        // Upper arm: a swelling shoulder tapering to the elbow, a ball at each end.
        let up = vec3::normalize_or(vec3::sub(elbow, shoulder), vec3::neg(vec3::Y));
        let root = vec3::sub(shoulder, vec3::add(vec3::scale(up, 0.3 * r0), [sg * 0.5 * r0, 0.0, 0.0]));
        let mut mesh = limb_tube(&[root, shoulder, vec3::lerp(shoulder, elbow, 0.5), elbow], 22, |t| r0 * (1.15 - 0.4 * t + 0.16 * (PI * t).sin()));
        mesh.append(&ball(shoulder, 1.22 * r0, [1.0, 1.0, 1.0]));
        // A deltoid reaching in over the trunk and up under the trapezius, so the shoulder stays
        // covered however the arm swings.
        mesh.append(&ball(vec3::add(shoulder, [-sg * 0.45 * r0, 0.3 * r0, -0.05 * r0]), 1.5 * r0, [1.0, 1.0, 1.0]));
        mesh.append(&ball(elbow, 0.8 * r0, [1.0, 1.0, 1.0]));
        meshes[ub] = vec![kit.skin(about(mesh, d.origins[ub]), "skin")];
        if state.marking == Marking::Pied && rng.chance(0.6) {
            let (n, at) = tube_surface(&[shoulder, elbow], rng.range_f32(0.3, 0.7), rng.range_f32(0.0, TAU), r0 * 0.95);
            patches[ub].append(&about(blot(at, n, 0.8 * r0, 0.1 * r0), d.origins[ub]));
        }

        // Forearm: heavy, tapering to a thick wrist.
        let r1 = 0.82 * r0;
        let mut mesh = limb_tube(&[elbow, vec3::lerp(elbow, wrist, 0.3), vec3::lerp(elbow, wrist, 0.7), wrist], 22, |t| r1 * (1.0 + 0.2 * (PI * t.sqrt()).sin() - 0.42 * t));
        mesh.append(&ball(elbow, 0.8 * r0, [1.0, 1.0, 1.0]));
        mesh.append(&ball(wrist, 0.58 * r0, [1.0, 1.0, 1.0]));
        meshes[ub + 1] = vec![kit.skin(about(mesh, d.origins[ub + 1]), "skin")];

        // Hand: built in hand space (+Y to the fingers, +Z the back, the thumb to -X), a left
        // one mirrored, then set on the wrist along the forearm.
        let forearm = vec3::normalize_or(vec3::sub(wrist, elbow), vec3::neg(vec3::Y));
        let out = [sg, 0.0, 0.0];
        let z = vec3::normalize_or(vec3::sub(out, vec3::scale(forearm, vec3::dot(out, forearm))), vec3::Z);
        let frame = Placement::new(wrist, vec3::cross(forearm, z), forearm, z);
        let (mut flesh, mut nails) = denizen::hand(&HandSpec { wrist: 0.58 * r0, ..hand_spec });
        for m in [&mut flesh, &mut nails] {
            if sg > 0.0 {
                m.scale([-1.0, 1.0, 1.0]);
            }
            m.transform(&frame.rotation);
        }
        meshes[ub + 2] = vec![part(flesh, "paw", MaterialKind::Skin, state.hide_colour(), true), part(nails, "nails", MaterialKind::Bone, Rgb::hex(0x2d2620), false)];
    }

    // ── Legs and hooves ──
    for (k, side) in Side::BOTH.into_iter().enumerate() {
        let sg = side.sign();
        let [hip, knee, hock] = d.legs[k];
        let tb = leg_bone(side);
        let leg_r = l.leg_radius;
        // Thigh: broad at the hip, tapering to the knee, with the quadriceps swelling in front
        // and the hamstring behind. The tube runs a little past the knee and the shank starts a
        // little before it, so the joint is one surface and not a ball.
        let root = vec3::add(hip, [-sg * 0.3 * leg_r, 0.5 * leg_r, 0.0]);
        let thigh_dir = vec3::normalize_or(vec3::sub(knee, hip), vec3::neg(vec3::Y));
        let shank_dir = vec3::normalize_or(vec3::sub(hock, knee), vec3::neg(vec3::Y));
        let past_knee = vec3::add(knee, vec3::scale(thigh_dir, 0.45 * leg_r));
        let mut mesh = limb_tube(&[root, hip, vec3::lerp(hip, knee, 0.4), vec3::lerp(hip, knee, 0.8), past_knee], 26, |t| leg_r * (1.3 - 0.62 * t + 0.1 * (PI * t).sin()));
        mesh.append(&ball(hip, 1.15 * leg_r, [1.0, 1.0, 1.0]));
        let side_out = [sg * 0.05 * leg_r, 0.0, 0.0];
        let at = |t: f32, back: f32| vec3::add(vec3::add(vec3::lerp(hip, knee, t), side_out), [0.0, 0.0, -back * leg_r]);
        // The hamstring behind the upper thigh, the quadriceps in front of it.
        mesh.append(&ball(at(0.32, 0.55), 0.6 * leg_r, [1.0, 1.6, 1.0]));
        mesh.append(&ball(at(0.42, -0.4), 0.55 * leg_r, [0.95, 1.7, 0.9]));
        meshes[tb] = vec![kit.skin(about(mesh, d.origins[tb]), "skin")];
        if state.marking == Marking::Pied && rng.chance(0.5) {
            let (n, at) = tube_surface(&[hip, knee], rng.range_f32(0.2, 0.7), rng.range_f32(0.0, TAU), leg_r * 0.95);
            patches[tb].append(&about(blot(at, n, 0.9 * leg_r, 0.1 * leg_r), d.origins[tb]));
        }

        // Shank: from the knee a calf swelling behind, tapering to a lean tendon at the high
        // hock, which stands out behind as a bony point.
        let r2 = 0.68 * leg_r;
        let before_knee = vec3::sub(knee, vec3::scale(shank_dir, 0.1 * leg_r));
        let mut mesh = limb_tube(&[before_knee, knee, vec3::lerp(knee, hock, 0.3), vec3::lerp(knee, hock, 0.7), hock], 24, |t| r2 * (1.0 + 0.35 * (PI * t.powf(0.6)).sin() - 0.5 * t));
        let calf = vec3::add(vec3::lerp(knee, hock, 0.25), [0.0, 0.0, -0.4 * leg_r]);
        mesh.append(&ball(calf, 0.5 * leg_r, [0.95, 1.7, 1.0]));
        mesh.append(&ball(vec3::add(hock, [0.0, 0.0, -0.22 * leg_r]), 0.38 * leg_r, [0.85, 1.3, 1.0]));
        meshes[tb + 1] = vec![kit.skin(about(mesh, d.origins[tb + 1]), "skin")];

        // Pastern and cloven hoof, in the foot's own frame, the sole flat on the floor.
        let g = match l.spec.legs {
            Legs::Digitigrade(g) => g,
            Legs::Plantigrade => Digitigrade::BULL,
        };
        let width = 0.8 * leg_r;
        let (flesh, hoof) = denizen::digitigrade_foot(&denizen::FootSpec { drop: hock[1], forward: g.pastern * hock[1], radius: 0.3 * leg_r, hoof: (width, 1.15 * width, 0.5 * width), split: 0.4 });
        let tint = if state.marking == Marking::Socks { state.white() } else { state.hide_colour() };
        meshes[tb + 2] = vec![part(flesh, "paw", MaterialKind::Skin, tint, true), part(hoof, "hoof", MaterialKind::Bone, state.hoof_colour(), true)];
    }

    // Patches and mane: after the surfaces, so each lies over its own bone.
    for bone in 0..index::COUNT {
        let (p, m) = (std::mem::take(&mut patches[bone]), std::mem::take(&mut manes[bone]));
        some(&mut meshes[bone], p, |m| kit.detail(m, "markings", state.white()));
        some(&mut meshes[bone], m, |m| kit.hair(m));
    }

    let mut rig = Rig::default();
    for (bone, parts) in meshes.into_iter().enumerate() {
        rig.add(NAMES[bone], parent_of(bone), Placement::translation(d.offset(bone)), parts);
    }
    // The tail, hung from behind the seat.
    let tail = denizen::tail(d, &TailSpec { seed: state.seed, radius: 0.34 * l.leg_radius, tuft: state.tuft, cord: state.hide_colour(), hair: state.hair_colour() });
    let offsets = denizen::tail_offsets(d);
    for (i, parts) in tail.into_iter().enumerate() {
        let parent = if i == 0 { index::PELVIS } else { denizen::TAIL_FIRST + i - 1 };
        let rest = if i == 0 { Placement { rotation: armory::geom::vec3::Mat3::from_axis_angle([1.0, 0.0, 0.0], denizen::TAIL_REST), origin: offsets[0] } } else { Placement::translation(offsets[i]) };
        rig.add(format!("tail_{i}"), Some(parent), rest, parts);
    }
    rig
}

// ── Head ──

/// The length of the skull and muzzle, along the face, for a head whose half depth is `hd`.
fn skull_length(state: &MinotaurState, hd: f32) -> f32 {
    (2.7 + 0.7 * state.muzzle) * hd
}

/// The bull's skull: one loft along the face (`z` in head space, `y` up, `x` to the left), from
/// the back of the head, wide at the horns and brow, running on forward into the long, deep,
/// blunt muzzle. Stations along it are shares `f` of its length.
struct Skull {
    loft: Loft,
    /// The skull's length, and the z of its back.
    length: f32,
    back: f32,
}

impl Skull {
    fn new(state: &MinotaurState, hw: f32, hh: f32, hd: f32) -> Skull {
        let length = skull_length(state, hd);
        let at = |f: f32| f * length;
        let wide = |k: &[(f32, f32)]| -> Vec<(f32, f32)> { k.iter().map(|&(f, v)| (at(f), v * hw)).collect() };
        let tall = |k: &[(f32, f32)]| -> Vec<(f32, f32)> { k.iter().map(|&(f, v)| (at(f), v * hh)).collect() };
        let loft = Loft::new(
            [0.0, 0.0, -hd],
            length,
            vec![(0.0, FRAC_PI_2)],
            wide(&[(0.0, 0.6), (0.1, 0.9), (0.3, 1.0), (0.45, 0.97), (0.58, 0.85), (0.7, 0.7), (0.85, 0.66), (0.94, 0.72), (1.0, 0.55)]),
            // Below the line of the face, then above it.
            tall(&[(0.0, 0.55), (0.1, 0.75), (0.3, 0.8), (0.45, 0.74), (0.58, 0.66), (0.7, 0.6), (0.85, 0.56), (0.94, 0.52), (1.0, 0.42)]),
            tall(&[(0.0, 0.6), (0.1, 0.85), (0.3, 0.97), (0.45, 0.88), (0.58, 0.74), (0.7, 0.64), (0.85, 0.58), (0.94, 0.55), (1.0, 0.44)]),
            2.3,
        );
        Skull { loft, length, back: -hd }
    }

    /// The point on the skull's surface at share `f` of its length, `phi` round it (a quarter
    /// turn is below the face's line, three quarters above), `inflate` out.
    fn point(&self, f: f32, phi: f32, inflate: f32) -> V3 {
        self.loft.point(f * self.length, phi, inflate)
    }

    /// The z at share `f` of the way along the face.
    fn z(&self, f: f32) -> f32 {
        self.back + f * self.length
    }

    /// The z of the loft's tip.
    fn tip(&self) -> f32 {
        self.back + self.length
    }
}

/// The head and jaw. Built level about the head's centre, then tipped and set on the atlas.
fn head(l: &Frame, state: &MinotaurState, rng: &mut Mulberry32, meshes: &mut [Vec<Part>]) {
    let (hw, hh, hd) = l.head;
    let d = &l.dims;
    let lean = l.spec.head_lean;
    let skull = Skull::new(state, hw, hh, hd);
    let finish = |mut m: MeshBuilder| -> MeshBuilder {
        m.rotate_x(lean).translate(vec3::sub(l.head_centre, d.origins[index::SKULL]));
        m
    };

    let mut flesh = skull.loft.mesh(0.0, skull.loft.length, 30, 44, 0.0, 0.0);
    // The brow ridges over the eyes, and the cheekbones.
    for sg in [-1.0f32, 1.0] {
        let pts = [[sg * 0.5 * hw, 0.8 * hh, skull.z(0.25)], [sg * 0.92 * hw, 0.62 * hh, skull.z(0.36)], [sg * 0.95 * hw, 0.52 * hh, skull.z(0.46)], [sg * 0.8 * hw, 0.42 * hh, skull.z(0.58)]];
        flesh.append(&limb_tube(&pts, 12, |t| 0.17 * hh * (0.8 + 0.4 * (PI * t).sin())));
        flesh.append(&ball([sg * 0.78 * hw, -0.1 * hh, skull.z(0.58)], 0.28 * hh, [0.9, 0.8, 1.6]));
    }
    // The nose pad over the muzzle's end, broad and damp, and the lip over the teeth.
    let tip = skull.tip();
    flesh.append(&ball([0.0, -0.02 * hh, tip - 0.1 * hd], 1.0, [0.58 * hw, 0.44 * hh, 0.42 * hd]));
    flesh.append(&ball([0.0, -0.3 * hh, skull.z(0.84)], 1.0, [0.66 * hw, 0.2 * hh, 0.7 * hd]));
    // Ears: broad leaves set out from under the horns, drooping and swept back.
    let ear_len = (0.75 + 0.55 * state.ears) * hw;
    let (mut ear_flesh, mut ear_inner) = (MeshBuilder::new(), MeshBuilder::new());
    for sg in [-1.0f32, 1.0] {
        let mut ear = uv_sphere(1.0, 16, 10);
        ear.scale([ear_len, 0.09 * hh, 0.36 * hh]).translate([ear_len * 0.85, 0.0, 0.0]);
        let mut inner = ear.clone();
        inner.scale([0.8, 1.0, 0.7]).translate([0.0, 0.03 * hh, 0.0]);
        for m in [&mut ear, &mut inner] {
            if sg < 0.0 {
                m.scale([-1.0, 1.0, 1.0]).flip();
            }
            m.rotate_z(-sg * 0.35).rotate_y(sg * 0.5).translate([sg * 0.9 * hw, 0.5 * hh, skull.z(0.2)]);
        }
        ear_flesh.append(&ear);
        ear_inner.append(&inner);
    }
    let mut parts = vec![part(finish(flesh), "head", MaterialKind::Skin, state.hide_colour(), true)];
    parts.push(part(finish(ear_flesh), "ears", MaterialKind::Skin, state.hide_colour(), false));
    parts.push(part(finish(ear_inner), "ear_hollows", MaterialKind::Skin, mix(state.hide_colour(), Rgb::hex(0xb88478), 0.5), false));

    // A blaze: a stripe of white down the face, lying on the skull.
    if state.marking == Marking::Blaze {
        let pts: Vec<V3> = (0..=10).map(|i| skull.point(0.3 + 0.62 * i as f32 / 10.0, 1.5 * PI, -0.05 * hh)).collect();
        let blaze = tube(&pts, 10, &SweepOptions::default(), |t| (0.2 - 0.06 * t) * hw);
        parts.push(part(finish(blaze), "markings", MaterialKind::Skin, state.white(), false));
    }

    // Nostrils: dark flared slits at the front of the pad, with a rim of flesh round each.
    let mut nostrils = MeshBuilder::new();
    let mut rims = MeshBuilder::new();
    for sg in [-1.0f32, 1.0] {
        let at = [sg * 0.26 * hw, 0.04 * hh, tip + 0.12 * hd];
        let mut hole = uv_sphere(1.0, 12, 8);
        hole.scale([0.14 * hw, 0.1 * hh, 0.09 * hd]).rotate_z(-sg * 0.5).translate(at);
        nostrils.append(&hole);
        let mut rim = uv_sphere(1.0, 12, 8);
        rim.scale([0.21 * hw, 0.16 * hh, 0.14 * hd]).rotate_z(-sg * 0.5).translate(vec3::sub(at, [0.0, 0.0, 0.06 * hd]));
        rims.append(&rim);
    }
    parts.push(part(finish(rims), "head", MaterialKind::Skin, mix(state.hide_colour(), Rgb::hex(0x7a5a52), 0.3), true));
    parts.push(part(finish(nostrils), "nostrils", MaterialKind::Skin, Rgb::hex(0x140d0c), false));
    // The ring through the septum, hanging forward and down.
    if let Some(metal) = state.ring {
        let (material, tint) = match metal {
            RingMetal::Iron => (MaterialKind::Iron, Rgb::hex(0x60605f)),
            RingMetal::Brass => (MaterialKind::Brass, Rgb::hex(0xb08a3c)),
        };
        let ring = denizen::ring([0.0, -0.1 * hh, tip + 0.15 * hd], vec3::Z, vec3::neg(vec3::Y), 0.42 * hh, 0.06 * hh);
        parts.push(part(finish(ring), "ring", material, tint, true));
    }

    // Eyes: set at the sides under the brow, with a horizontal pupil and a glint.
    let (mut balls, mut pupils, mut glints) = (MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new());
    let er = 0.2 * hh;
    for sg in [-1.0f32, 1.0] {
        let at = [sg * 0.9 * hw, 0.22 * hh, skull.z(0.44)];
        let look = vec3::normalize_or([sg * 0.85, -0.05, 0.5], vec3::Z);
        let (b, g) = eye(at, er, look, [0.0, 0.6, 0.8]);
        balls.append(&b);
        glints.append(&g);
        let mut pupil = uv_sphere(0.62 * er, 10, 6);
        pupil.scale([0.45, 0.5, 1.0]).place(vec3::add(at, vec3::scale(look, 0.74 * er)), look);
        pupils.append(&pupil);
    }
    parts.push(part(finish(balls), "eyes", MaterialKind::Gem, state.eye_colour(), true).with_glow(0.3));
    parts.push(part(finish(pupils), "pupils", MaterialKind::Gem, Rgb::hex(0x120c08), false));
    parts.push(part(finish(glints), "glints", MaterialKind::Gem, Rgb::hex(0xfff6dc), false).with_glow(0.5));

    // The horns: a boss at each root, the sweep, and the darker tip.
    let (mut horn_base, mut horn_tip) = (MeshBuilder::new(), MeshBuilder::new());
    for (side, sg) in [-1.0f32, 1.0].into_iter().enumerate() {
        let root = [sg * 0.72 * hw, 0.84 * hh, skull.z(0.28)];
        let spec = state.horn_spec(side, hw);
        let h = denizen::horn(root, sg, &spec);
        horn_base.append(&h.base);
        horn_tip.append(&h.tip);
        horn_base.append(&ball(root, 1.18 * spec.radius, [1.0, 0.8, 1.0]));
    }
    parts.push(part(finish(horn_base), "horns", MaterialKind::Bone, state.horn_colour(), true));
    some(&mut parts, finish(horn_tip), |m| part(m, "horn_tips", MaterialKind::Bone, state.horn_tip_colour(), true));

    // The forelock: locks from the poll, falling forward over the brow.
    let mut forelock = MeshBuilder::new();
    for _ in 0..(8.0 + 22.0 * state.mane) as usize {
        let f = rng.range_f32(0.05, 0.4);
        let phi = -FRAC_PI_2 + rng.range_f32(-0.5, 0.5);
        let root = skull.point(f, phi, -0.05 * hh);
        let out = vec3::normalize_or([rng.range_f32(-0.3, 0.3), 0.15, 1.0], vec3::Z);
        let len = rng.range_f32(0.5, 1.1) * hh * (0.5 + 0.6 * state.mane);
        forelock.append(&denizen::lock(root, out, len, 0.1 * hh, 0.9, [rng.range_f32(-0.2, 0.2) * hh, 0.0, 0.0]));
    }
    parts.push(part(finish(forelock), "hair", MaterialKind::Hair, state.hair_colour(), false));
    meshes[index::SKULL] = parts;

    // Jaw: a lower jaw swinging from the hinge, shorter than the muzzle and narrower, with the
    // chin and a row of blunt teeth.
    let hinge = d.origins[index::JAW];
    let jaw_z = skull.z(0.22);
    let jaw_len = 0.72 * skull.length;
    let jaw_loft = Loft::new(
        [0.0, -0.45 * hh, jaw_z],
        jaw_len,
        vec![(0.0, FRAC_PI_2 - 0.05)],
        vec![(0.0, 0.8 * hw), (0.15 * jaw_len, 0.8 * hw), (0.5 * jaw_len, 0.64 * hw), (jaw_len, 0.52 * hw)],
        vec![(0.0, 0.34 * hh), (0.5 * jaw_len, 0.27 * hh), (jaw_len, 0.22 * hh)],
        vec![(0.0, 0.26 * hh), (jaw_len, 0.2 * hh)],
        2.2,
    );
    let mut jaw = jaw_loft.mesh(0.0, jaw_len, 22, 34, 0.0, 0.0);
    let front = jaw_z + jaw_len;
    jaw.append(&ball([0.0, -0.52 * hh, front - 0.08 * hd], 1.0, [0.52 * hw, 0.24 * hh, 0.3 * hd]));
    let to_jaw = |mut m: MeshBuilder| -> MeshBuilder {
        m.rotate_x(lean).translate(vec3::sub(l.head_centre, hinge));
        m
    };
    let mut jaw_parts = vec![part(to_jaw(jaw), "jaw", MaterialKind::Skin, state.hide_colour(), true)];
    let mut teeth = MeshBuilder::new();
    for k in 0..6 {
        let x = (k as f32 - 2.5) * 0.15 * hw;
        let mut tooth = uv_sphere(1.0, 8, 6);
        tooth.scale([0.07 * hw, 0.11 * hh, 0.06 * hd]).translate([x, -0.28 * hh, front - 0.1 * hd - 0.012 * hd * (x / hw).abs() * 20.0]);
        teeth.append(&tooth);
    }
    jaw_parts.push(part(to_jaw(teeth), "teeth", MaterialKind::Bone, Rgb::hex(0xdcd2b4), false));
    // A tongue lying along the jaw.
    let mut tongue = uv_sphere(1.0, 12, 8);
    tongue.scale([0.3 * hw, 0.08 * hh, 0.7 * hd]).translate([0.0, -0.26 * hh, jaw_z + 0.55 * jaw_len]);
    jaw_parts.push(part(to_jaw(tongue), "tongue", MaterialKind::Skin, Rgb::hex(0x8a4a48), false));
    meshes[index::JAW] = jaw_parts;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Creature;

    #[test]
    fn the_same_seed_gives_the_same_minotaur() {
        for seed in [0, 1, 7, 1207, u32::MAX] {
            let (a, b) = (MinotaurState::roll(seed), MinotaurState::roll(seed));
            assert_eq!(a, b);
            assert_eq!(a.build(), b.build(), "seed {seed}");
            assert_eq!(Creature::of(Species::Minotaur, seed), Creature::Minotaur(a));
        }
        assert_ne!(MinotaurState::roll(1), MinotaurState::roll(2));
        assert_ne!(MinotaurState::roll(1).build(), MinotaurState::roll(2).build());
    }

    /// The lowest and highest y of the whole rig at rest.
    fn extent(state: &MinotaurState) -> (f32, f32) {
        let b = state.build().rest_mesh().bounds().unwrap();
        (b.0[1], b.1[1])
    }

    #[test]
    fn a_minotaur_stands_half_again_a_mans_height_on_the_floor() {
        for seed in 0..40 {
            let state = MinotaurState::roll(seed);
            let (low, high) = extent(&state);
            assert!(low.abs() < 0.3, "seed {seed}: soles at {low}");
            assert!((22.0..29.0).contains(&high), "seed {seed}: {high} tall with its horns");
            assert!((20.0..27.0).contains(&state.height()), "seed {seed}: head at {}", state.height());
            assert!(state.radius() > 5.0, "seed {seed}");
        }
    }

    #[test]
    fn the_rig_is_the_skeletons_bones_and_a_tail() {
        let state = MinotaurState::roll(7);
        let rig = state.build();
        rig.validate().unwrap();
        assert_eq!(rig.bones.len(), index::COUNT + denizen::TAIL_BONES);
        for (bone, name) in NAMES.iter().enumerate() {
            assert_eq!(rig.bones[bone].name, *name);
            assert_eq!(rig.bones[bone].parent, parent_of(bone));
        }
        assert_eq!(rig.bones[denizen::TAIL_FIRST].parent, Some(index::PELVIS));
        assert_eq!(rig.bones[denizen::TAIL_FIRST + 2].parent, Some(denizen::TAIL_FIRST + 1));
        let has = |bone: usize, part: &str| rig.bones[bone].mesh.parts.iter().any(|p| p.name == part);
        assert!(has(index::SKULL, "head") && has(index::SKULL, "horns") && has(index::SKULL, "eyes") && has(index::JAW, "jaw"));
        assert!(has(leg_bone(Side::Left) + 2, "hoof") && has(leg_bone(Side::Right) + 2, "hoof"));
        assert!(has(denizen::TAIL_FIRST + 2, "hair"), "the tail's tuft");
        // The rig and every body part have a capsule to be struck by.
        assert!(rig.bones.iter().take(index::COUNT).all(|b| b.hit().is_some()));
    }

    #[test]
    fn the_legs_are_digitigrade_and_the_hooves_stand_on_the_floor() {
        for seed in [1, 7, 99] {
            let state = MinotaurState::roll(seed);
            let d = state.dims();
            for k in 0..2 {
                let [hip, knee, hock] = d.legs[k];
                assert!(knee[2] > hip[2] + 1.0, "the knee stands in front of the hip");
                assert!(hock[1] > 0.25 * hip[1], "the hock is high: {hock:?}");
                assert_eq!(hock[2], hip[2], "the hock is under the hip");
            }
            let rig = state.build();
            let world = rig.world(&rig.rest());
            for side in Side::BOTH {
                let foot = leg_bone(side) + 2;
                let sole = rig.bones[foot].mesh.parts.iter().filter(|p| p.name == "hoof").flat_map(|p| &p.positions).map(|&q| world[foot].point(q)[1]).fold(f32::MAX, f32::min);
                assert!(sole.abs() < 0.05, "seed {seed}: sole at {sole}");
            }
        }
    }

    #[test]
    fn horns_and_coats_vary_and_one_horn_may_be_broken() {
        let states: Vec<MinotaurState> = (0..200).map(MinotaurState::roll).collect();
        assert!(Coat::ALL.iter().all(|c| states.iter().any(|s| s.coat == *c)));
        assert!(states.iter().any(|s| s.marking == Marking::Blaze) && states.iter().any(|s| s.marking == Marking::Socks) && states.iter().any(|s| s.marking == Marking::Pied));
        assert!(states.iter().any(|s| s.ring == Some(RingMetal::Iron)) && states.iter().any(|s| s.ring == Some(RingMetal::Brass)) && states.iter().any(|s| s.ring.is_none()));
        let broken: Vec<&MinotaurState> = states.iter().filter(|s| s.horns.broken.is_some()).collect();
        assert!(!broken.is_empty() && broken.len() < 80, "{} broken", broken.len());
        for s in broken.iter().take(5) {
            let (side, _) = s.horns.broken.unwrap();
            let hw = s.frame().head.0;
            let (whole, stub) = (s.horn_spec(1 - side as usize, hw), s.horn_spec(side as usize, hw));
            assert!(whole.broken.is_none() && stub.broken.is_some());
        }
        // A seed's horns are as long as the seed says, in a head of this width.
        let (short, long) = (states.iter().map(|s| s.horns.length).fold(1.0, f32::min), states.iter().map(|s| s.horns.length).fold(0.0, f32::max));
        assert!(short < 0.1 && long > 0.9);
    }
}
