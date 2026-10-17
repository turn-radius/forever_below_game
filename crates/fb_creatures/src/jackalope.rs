//! Jackalopes: [`JackalopeState`] from a seed, and its [`Rig`].
//!
//! A jackalope is a large hare with a pair of branching antlers: long hind legs and a long hind
//! foot, short forelegs, a deep chest and round haunches, a rump that stands higher than the
//! shoulders, long ears and a short tufted tail. The rig is a body, a head carrying the
//! antlers, two ears of two bones each (a base and a tip, so they can lie back and flop), four
//! legs of three bones each (upper leg, shin and paw, as a rat's) and the tail's tuft, so the
//! game can hop it, sit it up with its ears turning and lower its head to gore without a
//! skeleton.
//!
//! Fur is the body's own colour with tufts standing out of it, a paler belly and a darker saddle
//! down the back; the ears are lined with pink skin and tipped dark; the eyes are large amber
//! rounds with a dark pupil and a glint, set high on the sides of the head; the incisors are
//! ivory; the antlers are pale bone, a beam on each side with a brow tine and as many more
//! tines as the seed gives.
//!
//! The SRD has no jackalope, so the stat block is our own ([`crate::Species::Jackalope`]), and so
//! are the sizes, proportions and colourings: a brown hare's tawny, a mountain hare's grey and
//! a snow-white, after the hares of the Old World.

use armory::geom::vec3::{self, Mat3, V3};
use armory::geom::{CatmullRom, GridOptions, LatheOptions, MeshBuilder, Shading, SweepOptions, lathe, spike, thick_sheet, tube, uv_sphere};
use armory::{MaterialKind, Part, Rgb};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, PI};

use crate::rat::{LEGS, Leg, mix, part};
use crate::rig::Rig;
use crate::shape::{Trunk, eye, spine_to_creature};
use crate::{Size, Species};

/// Bone indices in a jackalope's [`Rig`].
pub mod bone {
    pub const BODY: usize = 0;
    pub const HEAD: usize = 1;
    /// The left ear's base, then its tip; the right ear's base, then its tip.
    pub const EAR_LEFT: usize = 2;
    pub const EAR_RIGHT: usize = 4;
    /// The first leg's upper bone; leg `k` (in [`crate::rat::LEGS`] order) has its upper leg at
    /// `LEGS + 3k`, its shin after it and its paw after that.
    pub const LEGS: usize = 6;
    pub const TAIL: usize = LEGS + 12;
    pub const COUNT: usize = TAIL + 1;
}

/// How far the body is tipped nose down at rest, radians: the haunches stand higher than the
/// shoulders.
pub const REST_PITCH: f32 = -0.17;

/// A jackalope's coat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JackalopeColouring {
    /// The brown hare's warm brown.
    Tawny,
    /// Reddish brown.
    Russet,
    /// The mountain hare's slate grey.
    Grey,
    /// A pale sandy brown.
    Dun,
    /// White, with dark ear tips.
    Snow,
}

impl JackalopeColouring {
    pub const ALL: [JackalopeColouring; 5] = [JackalopeColouring::Tawny, JackalopeColouring::Russet, JackalopeColouring::Grey, JackalopeColouring::Dun, JackalopeColouring::Snow];

    /// The coat's colour. Our own picks.
    pub fn fur(self) -> Rgb {
        match self {
            JackalopeColouring::Tawny => Rgb::hex(0x94704a),
            JackalopeColouring::Russet => Rgb::hex(0x8e5230),
            JackalopeColouring::Grey => Rgb::hex(0x7d7e7a),
            JackalopeColouring::Dun => Rgb::hex(0xae9570),
            JackalopeColouring::Snow => Rgb::hex(0xe6e3dc),
        }
    }

    /// The paler belly, chest and the inside of the legs.
    pub fn belly(self) -> Rgb {
        match self {
            JackalopeColouring::Tawny => Rgb::hex(0xd6c3a2),
            JackalopeColouring::Russet => Rgb::hex(0xd0a883),
            JackalopeColouring::Grey => Rgb::hex(0xcfccc2),
            JackalopeColouring::Dun => Rgb::hex(0xe2d6bb),
            JackalopeColouring::Snow => Rgb::hex(0xf4f2ec),
        }
    }

    /// The darker saddle down the back, and the ears' tips.
    pub fn saddle(self) -> Rgb {
        match self {
            JackalopeColouring::Tawny => Rgb::hex(0x5e4630),
            JackalopeColouring::Russet => Rgb::hex(0x5a3320),
            JackalopeColouring::Grey => Rgb::hex(0x4f504f),
            JackalopeColouring::Dun => Rgb::hex(0x7a6648),
            JackalopeColouring::Snow => Rgb::hex(0xb4b0a6),
        }
    }
}

/// The ears' lining and the nose.
const PINK: Rgb = Rgb::hex(0xd9958f);
/// The ears' tips, and a snow-white coat's dark ear tips.
const EAR_TIP: Rgb = Rgb::hex(0x2c2622);
/// The tail's tuft: white, as a hare's scut is underneath.
const COTTON: Rgb = Rgb::hex(0xf0ece2);
const IRIS: Rgb = Rgb::hex(0xc98a2a);
const PUPIL: Rgb = Rgb::hex(0x0e0b0a);
const GLINT: Rgb = Rgb::hex(0xf6f3ec);
const INCISOR: Rgb = Rgb::hex(0xe9e0c8);
const CLAW: Rgb = Rgb::hex(0x3b302a);
/// The antlers: pale bone, darker toward the base.
const ANTLER: Rgb = Rgb::hex(0xcdbb96);
const BURR: Rgb = Rgb::hex(0x8d7a58);

/// One jackalope, every choice the seed makes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JackalopeState {
    pub seed: u32,
    /// Head and body, nose to rump, in world units.
    pub length: f32,
    /// 0 lean to 1 stout.
    pub build: f32,
    /// The ears' length as a share of [`JackalopeState::length`].
    pub ears: f32,
    /// Tines on the right and the left antler, beyond the beam itself.
    pub tines: [u8; 2],
    /// How wide the antlers splay, 1 usual.
    pub spread: f32,
    pub colouring: JackalopeColouring,
}

/// Salt for the jackalope stream, so a jackalope and a rat from one seed are unrelated. Our own.
const JACKALOPE_SALT: u32 = 0x4A41_C4A1;

impl JackalopeState {
    /// A jackalope from `seed` ([`crate::jackalope`]).
    pub fn roll(seed: u32) -> JackalopeState {
        let mut rng = Mulberry32::new(seed ^ JACKALOPE_SALT);
        // A large hare of 75 cm to a metre, nose to rump: our own, so it reads as a quarry
        // and not a rat. Hit points and antlers stay the stat block's.
        let length = rng.range_f32(7.5, 10.0);
        let build = rng.range_f32(0.0, 1.0);
        let ears = rng.range_f32(0.36, 0.5);
        let tines = [rng.int(2, 4) as u8, rng.int(2, 4) as u8];
        let spread = rng.range_f32(0.75, 1.3);
        let colouring = *rng.weighted(&[(JackalopeColouring::Tawny, 5), (JackalopeColouring::Russet, 3), (JackalopeColouring::Grey, 3), (JackalopeColouring::Dun, 2), (JackalopeColouring::Snow, 1)]);
        JackalopeState { seed, length, build, ears, tines, spread, colouring }
    }

    /// Its proportions, which the rig and its poses share.
    pub fn dims(&self) -> JackalopeDims {
        let l = self.length;
        let girth = 0.9 + 0.2 * self.build;
        let half_width = 0.12 * l * girth;
        let half_height = 0.15 * l * (0.95 + 0.1 * self.build);
        let body_length = 0.7 * l;
        let hind = Leg { upper: 0.19 * l, lower: 0.2 * l, ankle: 0.05 * l, paw: 0.27 * l };
        let fore = Leg { upper: 0.15 * l, lower: 0.15 * l, ankle: 0.035 * l, paw: 0.1 * l };
        let hip = [0.62 * half_width, -0.30 * body_length, -0.30 * half_height];
        let shoulder = [0.5 * half_width, 0.26 * body_length, -0.30 * half_height];
        // The hind legs stand a little bent: the hip is four fifths of the legs' reach above the
        // ankle's height.
        let hip_height = hind.ankle + 0.8 * (hind.upper + hind.lower);
        let rest = spine_to_creature().mul_mat(&pitch_about_x(REST_PITCH));
        let centre_height = hip_height - rest.transform([0.0, hip[1], hip[2]])[1];
        let ear_length = self.ears * l;
        JackalopeDims {
            length: l,
            half_width,
            half_height,
            body_length,
            head_length: 0.29 * l,
            centre_height,
            hip,
            shoulder,
            neck: [0.0, 0.43 * body_length, 0.12 * half_height],
            hind,
            fore,
            tail_root: [0.0, -0.485 * body_length, 0.45 * half_height],
            tail_length: 0.15 * l,
            tail_radius: 0.045 * l,
            ear_length,
            ear_width: 0.1 * l,
            antler_length: 0.3 * l,
        }
    }

    /// Its SRD size category, which its hit points and antlers go with.
    pub fn size(&self) -> Size {
        self.species().stat_block().size
    }

    pub fn species(&self) -> Species {
        Species::Jackalope
    }

    /// The size its body is drawn at: the stat block's own.
    pub fn drawn_size(&self) -> Size {
        self.size()
    }

    /// How high its back stands off the floor at rest, world units.
    pub fn height(&self) -> f32 {
        let d = self.dims();
        d.centre_height + d.half_height
    }

    /// The circle it covers on the floor, about its centre: its body, nose to rump.
    pub fn radius(&self) -> f32 {
        0.5 * self.length
    }

    /// Its bones and meshes, standing at rest.
    pub fn build(&self) -> Rig {
        let d = self.dims();
        let mut rng = Mulberry32::new(self.seed ^ JACKALOPE_SALT ^ 0xB0D7);
        let c = self.colouring;
        let rest = crate::pose::jackalope(self, &crate::pose::JackalopePose::default());
        let mut rig = Rig::default();

        // Body: fur with tufts standing out of it, a pale belly and a dark saddle.
        let trunk = self.body_trunk(&d);
        let mut fur = trunk.mesh(24, 30);
        fur.append(&trunk.tufts(150, 0.07 * d.length, &mut rng, |t, a| (0.05..0.93).contains(&t) && a.sin() > -0.35));
        let mut body = vec![part(fur, "fur", MaterialKind::Hair, c.fur(), true)];
        let off = 0.006 * d.length;
        if let Some(m) = trunk.patch(0.1, 0.92, -FRAC_PI_2, |t| 0.95 * bell(t, 0.1, 0.92), off, false) {
            body.push(part(m, "belly", MaterialKind::Hair, c.belly(), false));
        }
        if let Some(m) = trunk.patch(0.14, 0.9, FRAC_PI_2, |t| 0.34 * bell(t, 0.14, 0.9).sqrt(), off * 1.5, false) {
            body.push(part(m, "saddle", MaterialKind::Hair, c.saddle(), false));
        }
        let body_bone = rig.add("body", None, rest[bone::BODY], body);

        // Head: fur, the eyes, nose, whiskers and teeth, and the antlers.
        let head_bone = rig.add("head", Some(body_bone), rest[bone::HEAD], self.head_parts(&d, &mut rng));

        // Ears: left, then right; each a base and a tip.
        for (k, side) in [-1.0f32, 1.0].into_iter().enumerate() {
            let name = if side < 0.0 { "ear_left" } else { "ear_right" };
            let (base, tip) = self.ear_parts(&d);
            let base_bone = rig.add(name, Some(head_bone), rest[bone::EAR_LEFT + 2 * k], base);
            rig.add(format!("{name}_tip"), Some(base_bone), rest[bone::EAR_LEFT + 2 * k + 1], tip);
        }

        // Legs: upper leg (fur), shin (fur) and paw (furred foot, toes and claws).
        for (k, &(name, _, hind)) in LEGS.iter().enumerate() {
            let leg = if hind { d.hind } else { d.fore };
            let girth = if hind { 0.62 } else { 0.4 } * d.half_width;
            let upper = rig.add(name, Some(body_bone), rest[bone::LEGS + 3 * k], vec![part(upper_leg(leg.upper, girth, hind), "fur", MaterialKind::Hair, c.fur(), true)]);
            let shin_tint = if hind { c.fur() } else { mix(c.fur(), c.belly(), 0.35) };
            let shin = rig.add(format!("{name}_shin"), Some(upper), rest[bone::LEGS + 3 * k + 1], vec![part(shin_mesh(leg.lower, girth, hind), "fur", MaterialKind::Hair, shin_tint, true)]);
            let (paw, claws) = paw_mesh(&leg, girth, hind);
            rig.add(
                format!("{name}_paw"),
                Some(shin),
                rest[bone::LEGS + 3 * k + 2],
                vec![part(paw, "paw", MaterialKind::Hair, mix(c.fur(), c.belly(), 0.5), true), part(claws, "claws", MaterialKind::Bone, CLAW, false)],
            );
        }

        // Tail: a short tuft, white.
        let mut tail = tail_tuft(d.tail_length, d.tail_radius);
        let mut cotton = MeshBuilder::new();
        let mut tufts = Mulberry32::new(self.seed ^ JACKALOPE_SALT ^ 0x7A11);
        for _ in 0..34 {
            let (u, a) = (tufts.range_f32(0.1, 0.95), tufts.range_f32(0.0, 2.0 * PI));
            let r = d.tail_radius * tail_profile(u);
            let root = [r * a.cos(), -u * d.tail_length, r * a.sin()];
            let out = vec3::normalize_or([a.cos(), -0.9, a.sin()], vec3::neg(vec3::Y));
            let mut tuft = spike(0.011 * d.length, 0.05 * d.length * tufts.range_f32(0.8, 1.3), 5, 0.6, Shading::SMOOTH);
            tuft.place(root, out);
            cotton.append(&tuft);
        }
        tail.append(&cotton);
        rig.add("tail", Some(body_bone), rest[bone::TAIL], vec![part(tail, "fur", MaterialKind::Hair, COTTON, true)]);
        debug_assert_eq!(rig.bones.len(), bone::COUNT);
        rig
    }

    /// The body's trunk in its own spine frame, centred on the body bone: deep chest, tucked
    /// loin, round haunches.
    pub fn body_trunk(&self, d: &JackalopeDims) -> Trunk {
        let (w, h) = (d.half_width, d.half_height);
        Trunk {
            start: -0.5 * d.body_length,
            length: d.body_length,
            width: vec![(0.0, 0.0), (0.03, 0.45 * w), (0.12, 0.92 * w), (0.26, w), (0.45, 0.76 * w), (0.62, 0.8 * w), (0.8, 0.9 * w), (0.93, 0.62 * w), (1.0, 0.0)],
            height: vec![(0.0, 0.0), (0.03, 0.5 * h), (0.12, 0.92 * h), (0.26, h), (0.45, 0.7 * h), (0.62, 0.8 * h), (0.8, 0.95 * h), (0.93, 0.66 * h), (1.0, 0.0)],
            lift: vec![(0.0, 0.05 * h), (0.25, 0.18 * h), (0.5, 0.1 * h), (0.8, 0.08 * h), (1.0, -0.1 * h)],
            squareness: 2.1,
            belly: 0.6,
        }
    }

    /// The head's trunk in the head bone's frame (the neck at its origin, snout along +Y).
    pub fn head_trunk(&self, d: &JackalopeDims) -> Trunk {
        let (w, h, l) = (0.092 * d.length * (0.95 + 0.1 * self.build), 0.112 * d.length, d.head_length);
        Trunk {
            start: -0.07 * d.length,
            length: l,
            width: vec![(0.0, 0.0), (0.07, 0.62 * w), (0.25, w), (0.45, 0.9 * w), (0.7, 0.56 * w), (0.9, 0.34 * w), (0.975, 0.22 * w), (1.0, 0.0)],
            height: vec![(0.0, 0.0), (0.07, 0.66 * h), (0.3, h), (0.5, 0.86 * h), (0.75, 0.56 * h), (0.92, 0.36 * h), (0.975, 0.24 * h), (1.0, 0.0)],
            lift: vec![(0.0, 0.0), (0.3, 0.04 * h), (0.75, -0.1 * h), (1.0, -0.25 * h)],
            squareness: 2.1,
            belly: 0.7,
        }
    }

    fn head_parts(&self, d: &JackalopeDims, rng: &mut Mulberry32) -> Vec<Part> {
        let c = self.colouring;
        let head = self.head_trunk(d);
        let l = d.length;
        let mut skin = head.mesh(22, 26);
        skin.append(&head.tufts(34, 0.05 * l, rng, |t, a| (0.12..0.58).contains(&t) && a.sin() > -0.3));
        let mut parts = vec![part(skin, "fur", MaterialKind::Hair, c.fur(), true)];

        // Eyes: large amber rounds set high on the sides of the head, a dark pupil looking
        // along the head and a glint, so the eye reads from the front and the side.
        let (mut irises, mut pupils, mut glints) = (MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new());
        let r = 0.03 * l;
        for a in [0.62f32, PI - 0.62] {
            let t = 0.5;
            let n = head.normal(t, a);
            let at = head.point(t, a, -0.4 * r);
            let look = vec3::normalize_or(vec3::add(vec3::scale(n, 0.85), [0.0, 0.5, 0.0]), n);
            let (ball, glint) = eye(at, r, look, [0.0, 0.5, 0.85]);
            irises.append(&ball);
            glints.append(&glint);
            let mut pupil = uv_sphere(0.62 * r, 12, 8);
            pupil.scale([1.0, 1.0, 1.0]).translate(vec3::add(at, vec3::scale(look, 0.42 * r)));
            pupils.append(&pupil);
        }
        parts.push(part(irises, "eyes", MaterialKind::Gem, IRIS, true));
        parts.push(part(pupils, "pupils", MaterialKind::Gem, PUPIL, false));
        parts.push(part(glints, "glints", MaterialKind::Gem, GLINT, false).with_glow(0.5));

        // Nose: a small cleft pad at the muzzle's tip.
        let mut nose = MeshBuilder::new();
        for side in [1.0f32, -1.0] {
            let mut half = uv_sphere(0.017 * l, 10, 7);
            half.scale([1.0, 0.9, 0.8]).translate(vec3::add(head.point(0.99, FRAC_PI_2, -0.008 * l), [side * 0.009 * l, 0.0, 0.0]));
            nose.append(&half);
        }
        parts.push(part(nose, "nose", MaterialKind::Skin, mix(PINK, c.fur(), 0.2), true));

        // Whiskers: a few fine ones a side, shorter than the head, swept back along the cheek and
        // drooping a little, as a hare's are.
        let mut whiskers = MeshBuilder::new();
        for side in [1.0f32, -1.0] {
            for k in 0..4 {
                let a_side = if side > 0.0 { 0.0 } else { PI };
                let rise = -0.3 + 0.18 * k as f32;
                let a = a_side + side * rise;
                let t = 0.86 + 0.012 * (k % 2) as f32;
                let root = head.point(t, a, -0.004 * l);
                let len = 0.13 * l * rng.range_f32(0.8, 1.1);
                let out = [side, 0.0, 0.0];
                let path: Vec<V3> = (0..=6)
                    .map(|i| {
                        let s = i as f32 / 6.0;
                        let spread = vec3::add(vec3::scale(out, 0.6 * s), [0.0, -0.55 * s, (rise * 0.5 - 0.25 * s) * s]);
                        vec3::add(root, vec3::scale(spread, len))
                    })
                    .collect();
                whiskers.append(&tube(&path, 4, &SweepOptions { cap_end: false, ..Default::default() }, |s| 0.0022 * l * (1.0 - 0.7 * s)));
            }
        }
        parts.push(part(whiskers, "whiskers", MaterialKind::Hair, mix(c.belly(), Rgb::hex(0xffffff), 0.3), false));

        // Incisors: two ivory teeth under the muzzle's tip.
        let mut teeth = MeshBuilder::new();
        let tip = head.point(0.93, -FRAC_PI_2, -0.004 * l);
        for side in [1.0f32, -1.0] {
            let mut tooth = armory::geom::cuboid([0.009 * l, 0.007 * l, 0.028 * l]);
            tooth.rotate_x(-0.25).translate(vec3::add(tip, [side * 0.006 * l, 0.0, -0.012 * l]));
            teeth.append(&tooth);
        }
        parts.push(part(teeth, "teeth", MaterialKind::Bone, INCISOR, false));

        // Antlers: a burr at each root, a beam and its tines.
        let (antlers, burrs) = self.antlers(d, &head);
        parts.push(part(antlers, "antlers", MaterialKind::Bone, ANTLER, true));
        parts.push(part(burrs, "burrs", MaterialKind::Bone, BURR, false));
        parts
    }

    /// The two antlers in the head bone's frame, and the burrs they grow from. Each is a beam
    /// sweeping up and back, then out and forward at the tip, with a brow tine pointing
    /// forward and the rest alternating forward and back along it. Seeded by the state's tines
    /// and spread.
    pub fn antlers(&self, d: &JackalopeDims, head: &Trunk) -> (MeshBuilder, MeshBuilder) {
        let (mut antlers, mut burrs) = (MeshBuilder::new(), MeshBuilder::new());
        let a_len = d.antler_length;
        let r0 = 0.02 * d.length;
        for (side, tines) in [(1.0f32, self.tines[0]), (-1.0, self.tines[1])] {
            let angle = FRAC_PI_2 - side * 0.5;
            let root = head.point(0.33, angle, -0.012 * d.length);
            let sp = self.spread;
            let keys: [V3; 5] = [[0.0, 0.0, 0.0], [0.07 * sp, -0.07, 0.3], [0.17 * sp, -0.11, 0.6], [0.29 * sp, -0.06, 0.84], [0.42 * sp, 0.07, 1.0]];
            let at = |k: V3| vec3::add(root, [side * k[0] * a_len, k[1] * a_len, k[2] * a_len]);
            let curve = CatmullRom::centripetal(keys.iter().map(|&k| at(k)).collect());
            let path = curve.sample(5);
            antlers.append(&tube(&path, 8, &SweepOptions::default(), |s| r0 * (1.0 - 0.7 * s.powf(1.1))));
            // Tines along the beam: the brow low and forward, the rest alternating.
            for k in 0..tines {
                let s = 0.14 + 0.6 * (k as f32 / tines.max(1) as f32);
                let base = curve.point(s);
                let (ahead, behind) = (curve.point((s + 0.03).min(1.0)), curve.point((s - 0.03).max(0.0)));
                let along = vec3::normalize_or(vec3::sub(ahead, behind), vec3::Z);
                let dir = if k == 0 {
                    vec3::normalize_or([side * 0.3, 0.95, 0.3], vec3::Y)
                } else if k % 2 == 1 {
                    vec3::normalize_or(vec3::add(vec3::scale(along, 0.7), [side * 0.55, -0.45, 0.2]), along)
                } else {
                    vec3::normalize_or(vec3::add(vec3::scale(along, 0.7), [side * 0.2, 0.7, 0.2]), along)
                };
                let len = a_len * (if k == 0 { 0.34 } else { 0.4 }) * (1.0 - 0.45 * s);
                let points = vec![
                    vec3::sub(base, vec3::scale(dir, 0.1 * len)),
                    vec3::add(base, vec3::scale(dir, 0.4 * len)),
                    vec3::add(vec3::add(base, vec3::scale(dir, 0.78 * len)), [0.0, 0.0, 0.1 * len]),
                    vec3::add(vec3::add(base, vec3::scale(dir, len)), [0.0, 0.0, 0.3 * len]),
                ];
                let r = r0 * 0.62 * (1.0 - 0.5 * s);
                antlers.append(&tube(&CatmullRom::centripetal(points).sample(4), 6, &SweepOptions::default(), |t| r * (1.0 - 0.88 * t.powf(1.2))));
            }
            // The burr: a flat knob round the antler's root.
            let mut burr = uv_sphere(1.55 * r0, 12, 8);
            burr.scale([1.0, 1.0, 0.6]).translate(vec3::add(root, [0.0, 0.0, 0.2 * r0]));
            burrs.append(&burr);
        }
        (antlers, burrs)
    }

    /// An ear's two bones' parts, in each bone's own space: the base (from the head) and the tip.
    fn ear_parts(&self, d: &JackalopeDims) -> (Vec<Part>, Vec<Part>) {
        let c = self.colouring;
        let tip_tint = if c == JackalopeColouring::Snow { EAR_TIP } else { mix(c.saddle(), EAR_TIP, 0.5) };
        let base = |inner: bool| ear_sheet(d.ear_width, d.ear_length, 0.0, EAR_SPLIT, inner);
        let tip = |inner: bool| ear_sheet(d.ear_width, d.ear_length, EAR_SPLIT, 1.0, inner);
        (
            vec![part(base(false), "fur", MaterialKind::Hair, c.fur(), true), part(base(true), "inner", MaterialKind::Skin, PINK, false)],
            vec![part(tip(false), "fur", MaterialKind::Hair, tip_tint, true), part(tip(true), "inner", MaterialKind::Skin, mix(PINK, tip_tint, 0.35), false)],
        )
    }
}

/// Where along an ear (0 its root, 1 its tip) the tip bone begins.
pub const EAR_SPLIT: f32 = 0.55;

/// A jackalope's proportions: everything its rig and poses share.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JackalopeDims {
    pub length: f32,
    pub half_width: f32,
    pub half_height: f32,
    pub body_length: f32,
    pub head_length: f32,
    /// Height of the body bone over the floor at rest.
    pub centre_height: f32,
    /// The right hip, shoulder and the neck in the body bone's spine frame (left mirrors x).
    pub hip: V3,
    pub shoulder: V3,
    pub neck: V3,
    pub hind: Leg,
    pub fore: Leg,
    /// Where the tail starts, in the body bone's spine frame.
    pub tail_root: V3,
    pub tail_length: f32,
    pub tail_radius: f32,
    pub ear_length: f32,
    pub ear_width: f32,
    pub antler_length: f32,
}

impl JackalopeDims {
    /// Where leg `k` (in [`LEGS`] order) joins the body, in the body bone's spine frame.
    pub fn leg_root(&self, k: usize) -> V3 {
        let (_, side, hind) = LEGS[k];
        let p = if hind { self.hip } else { self.shoulder };
        [p[0] * side, p[1], p[2]]
    }

    pub fn leg(&self, k: usize) -> Leg {
        if LEGS[k].2 { self.hind } else { self.fore }
    }

    /// Where an ear on `side` (+1 right, -1 left) grows from the head, in the head bone's frame,
    /// and its placement there at rest: standing up, splayed out and a little back, its hollow
    /// facing forward and out.
    pub fn ear_root(&self, state: &JackalopeState, side: f32) -> (V3, Mat3) {
        let head = state.head_trunk(self);
        let a = FRAC_PI_2 - side * 0.5;
        let root = head.point(0.2, a, -0.02 * self.length);
        let up = vec3::normalize_or([0.26 * side, -0.3, 1.0], vec3::Z);
        let facing = vec3::normalize_or([0.6 * side, 0.8, 0.0], vec3::Y);
        (root, basis(up, facing))
    }
}

/// The rotation about +X of the spine frame (tipping +Y up toward +Z).
pub(crate) fn pitch_about_x(a: f32) -> Mat3 {
    Mat3::from_axis_angle([1.0, 0.0, 0.0], a)
}

/// The orthonormal basis with +Y along `up`, +Z as near `facing` as is square to it, and +X
/// across them.
pub(crate) fn basis(up: V3, facing: V3) -> Mat3 {
    let y = vec3::normalize_or(up, vec3::Y);
    let z = vec3::try_normalize(vec3::sub(facing, vec3::scale(y, vec3::dot(facing, y)))).unwrap_or_else(|| vec3::any_perpendicular(y));
    Mat3::from_cols(vec3::cross(y, z), y, z)
}

/// A soft bump over `t0..t1`: 0 at both ends, 1 across the middle.
fn bell(t: f32, t0: f32, t1: f32) -> f32 {
    let s = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
    (4.0 * s * (1.0 - s)).clamp(0.0, 1.0).sqrt()
}

/// An ear's outline: its half width at `v` along it (0 root, 1 tip), as a share of the half
/// width at its widest. Rolled narrow at the root, widest a third up, a rounded tip.
fn ear_half(v: f32) -> f32 {
    if v < 0.35 { 0.62 + 0.38 * (PI * v / 0.7).sin() } else { (1.0 - ((v - 0.35) / 0.65).powf(2.2)).max(0.0).sqrt() }
}

/// How far an ear curves back from straight at `v` along it, world units per ear length: it
/// bows backward (-Z).
pub(crate) fn ear_bow(v: f32) -> f32 {
    -0.14 * v * v
}

/// The part of an ear from `v0` to `v1` along it (0 root, 1 tip), in the space of a bone
/// whose origin is the ear's point at `v0`: the cupped, thin sheet, or with `inner` the pink
/// lining lying just on its hollow side.
fn ear_sheet(width: f32, length: f32, v0: f32, v1: f32, inner: bool) -> MeshBuilder {
    let (cols, rows) = (9usize, 9usize);
    let origin = [0.0, v0 * length, ear_bow(v0) * length];
    let shrink = if inner { 0.72 } else { 1.0 };
    let points: Vec<Vec<V3>> = (0..=rows)
        .map(|r| {
            let v = v0 + (v1 - v0) * r as f32 / rows as f32;
            let half = width * 0.5 * ear_half(v) * shrink;
            let v_in = if inner { v0 + (v1 - v0) * (0.03 + 0.94 * r as f32 / rows as f32) } else { v };
            (0..=cols)
                .map(|c| {
                    let u = c as f32 / cols as f32 * 2.0 - 1.0;
                    // Cupped: the middle sits back from the rim.
                    let cup = -0.8 * width * 0.5 * ear_half(v) * (1.0 - u * u);
                    let lift = if inner { 0.012 * length } else { 0.0 };
                    let p = [u * half, v_in * length, ear_bow(v) * length + cup + lift];
                    vec3::sub(p, origin)
                })
                .collect()
        })
        .collect();
    let thickness = if inner { 0.0015 * length } else { 0.03 * length };
    thick_sheet(&points, &GridOptions { wrap_around: false, wrap_along: false, shading: Shading::SMOOTH, row_v: None }, thickness)
}

/// The tuft's radius at `u` (0 its root, 1 its tip), as a share of the tail's radius.
fn tail_profile(u: f32) -> f32 {
    (PI * (0.18 + 0.78 * u)).sin().clamp(0.0, 1.0)
}

/// The tail: a rounded tuft from its joint back along -Y.
fn tail_tuft(length: f32, radius: f32) -> MeshBuilder {
    let mut profile = vec![[0.0, -length - 0.5 * radius]];
    for k in (0..=8).rev() {
        let u = k as f32 / 8.0;
        profile.push([radius * tail_profile(u).max(0.12 * (1.0 - u)), -length * u]);
    }
    profile.push([0.0, 0.5 * radius]);
    let mut m = lathe(&profile, &LatheOptions::new(12, Shading::SMOOTH));
    m.scale([1.0, 1.0, 1.15]);
    m
}

/// An upper leg hanging down -Z from its joint, `length` long: a haunch or shoulder bulging
/// into the body, tapering to the knee.
fn upper_leg(length: f32, girth: f32, hind: bool) -> MeshBuilder {
    let top = if hind { 1.25 } else { 0.85 };
    let profile = armory::geom::spline_profile(
        &[[0.0, -length - 0.35 * girth], [0.42 * girth, -length], [0.6 * girth, -0.55 * length], [top * girth, -0.15 * length], [0.8 * top * girth, 0.28 * girth], [0.0, 0.7 * girth]],
        18,
    );
    let mut m = lathe(&profile, &LatheOptions::new(12, Shading::SMOOTH));
    m.transform(&Mat3::from_cols([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]));
    // The thigh is longer front to back than across.
    m.scale([0.75, if hind { 1.3 } else { 1.0 }, 1.0]);
    m
}

/// A shin hanging down -Z from the knee, `length` long, slender to the ankle (a hind leg's
/// hock is the ankle, high and bony).
fn shin_mesh(length: f32, girth: f32, hind: bool) -> MeshBuilder {
    let belly = if hind { 0.62 } else { 0.5 };
    let profile = armory::geom::spline_profile(&[[0.0, -length - 0.2 * girth], [0.28 * girth, -length], [0.36 * girth, -0.5 * length], [belly * girth, -0.06 * length], [0.0, 0.46 * girth]], 14);
    let mut m = lathe(&profile, &LatheOptions::new(10, Shading::SMOOTH));
    m.transform(&Mat3::from_cols([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]));
    m
}

/// A paw: a narrow furred foot forward of the ankle (a hind foot is very long), toes and
/// small claws, lying on the floor `leg.ankle` under the ankle joint at its origin. Returns the
/// paw and its claws.
fn paw_mesh(leg: &Leg, girth: f32, hind: bool) -> (MeshBuilder, MeshBuilder) {
    let (len, wide, thick) = (leg.paw, if hind { 0.34 } else { 0.4 } * girth, leg.ankle);
    let mut paw = MeshBuilder::new();
    let heel = if hind { -0.1 * len } else { -0.2 * len };
    let path = [[0.0, heel, -0.45 * thick], [0.0, 0.25 * len, -0.64 * thick], [0.0, 0.62 * len, -0.7 * thick]];
    paw.append(&tube(&path, 10, &SweepOptions::default(), |s| (0.55 + 0.45 * (PI * (0.2 + 0.8 * s)).sin()) * wide * 0.5));
    paw.scale([1.0, 1.0, 0.8]);
    let mut ankle = uv_sphere(0.55 * wide, 10, 7);
    ankle.scale([1.0, 1.0, 1.2]).translate([0.0, 0.0, -0.2 * thick]);
    paw.append(&ankle);
    let mut claws = MeshBuilder::new();
    let toes = 4;
    for i in 0..toes {
        let u = i as f32 / (toes - 1) as f32 * 2.0 - 1.0;
        let spread = u * 0.32;
        let reach = if hind { 0.34 } else { 0.5 } * len * (1.0 - 0.2 * u * u);
        let base = [u * wide * 0.32, 0.6 * len, -0.7 * thick];
        let dir = vec3::normalize_or([spread.sin(), spread.cos(), -0.1], vec3::Y);
        let tip = vec3::add(base, vec3::scale(dir, reach));
        let toe_path = CatmullRom::centripetal(vec![base, vec3::lerp(base, tip, 0.5), tip]).sample(3);
        paw.append(&tube(&toe_path, 6, &SweepOptions::default(), |s| 0.14 * wide * (1.0 - 0.3 * s)));
        let mut claw = spike(0.06 * wide, 0.13 * len.max(0.3 * wide), 4, 0.8, Shading::FLAT);
        claw.place(tip, vec3::normalize_or(vec3::add(dir, [0.0, 0.0, -0.25]), dir));
        claws.append(&claw);
    }
    // The paw rests on the floor: its lowest point at `-thick`.
    if let Some((lo, _)) = MeshBuilder::merged([&paw, &claws]).bounds() {
        let drop = -thick - lo[2];
        paw.translate([0.0, 0.0, drop]);
        claws.translate([0.0, 0.0, drop]);
    }
    (paw, claws)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn jackalopes_are_deterministic() {
        for seed in [0, 7, 1207, u32::MAX] {
            assert_eq!(JackalopeState::roll(seed), JackalopeState::roll(seed));
            assert_eq!(JackalopeState::roll(seed).build(), JackalopeState::roll(seed).build());
        }
        assert_ne!(JackalopeState::roll(1), JackalopeState::roll(2));
    }

    #[test]
    fn every_jackalope_part_validates() {
        for seed in 0..24 {
            let jackalope = JackalopeState::roll(seed);
            let rig = jackalope.build();
            rig.validate().unwrap_or_else(|e| panic!("{seed}: {e}"));
            assert_eq!(rig.bones.len(), bone::COUNT);
            for name in ["body", "head", "ear_left", "ear_left_tip", "ear_right", "ear_right_tip", "leg_front_left", "leg_hind_right_paw", "tail"] {
                assert!(rig.index(name).is_some(), "{name}");
            }
        }
    }

    #[test]
    fn jackalopes_have_fur_ears_eyes_and_antlers_that_read() {
        let rig = JackalopeState::roll(5).build();
        let head = rig.bone("head").unwrap();
        let names: HashSet<&str> = head.mesh.parts.iter().map(|p| p.name).collect();
        for name in ["fur", "eyes", "pupils", "glints", "nose", "whiskers", "teeth", "antlers", "burrs"] {
            assert!(names.contains(name), "{name}");
        }
        // Fur tufts make the body much more than a smooth blob, and the antlers are well
        // modelled.
        let fur = &rig.bone("body").unwrap().mesh.parts[0];
        assert!(fur.indices.len() / 3 > 3000, "{} triangles", fur.indices.len() / 3);
        let antlers = head.mesh.parts.iter().find(|p| p.name == "antlers").unwrap();
        assert!(antlers.indices.len() / 3 > 400, "{} triangles", antlers.indices.len() / 3);
        assert!(head.mesh.parts.iter().any(|p| p.name == "glints" && p.glow > 0.0));
        // The ear's hollow is lined.
        assert!(rig.bone("ear_right").unwrap().mesh.parts.iter().any(|p| p.name == "inner"));
    }

    #[test]
    fn antlers_branch_as_the_state_says_and_grow_up_from_the_crown() {
        for seed in 0..16 {
            let j = JackalopeState::roll(seed);
            assert!(j.tines.iter().all(|&t| (2..=4).contains(&t)), "{:?}", j.tines);
            let rig = j.build();
            let head = rig.bone("head").unwrap();
            let antlers = head.mesh.parts.iter().find(|p| p.name == "antlers").unwrap();
            let d = j.dims();
            let top = antlers.positions.iter().map(|p| p[2]).fold(f32::MIN, f32::max);
            let skull = j.head_trunk(&d).point(0.33, FRAC_PI_2, 0.0)[2];
            assert!(top > skull + 0.2 * d.antler_length, "{seed}: antlers rise {} over the skull", top - skull);
            // More tines make a bigger antler.
            let few = JackalopeState { tines: [2, 2], ..j.clone() };
            let many = JackalopeState { tines: [4, 4], ..j.clone() };
            let tris = |s: &JackalopeState| s.build().bone("head").unwrap().mesh.parts.iter().find(|p| p.name == "antlers").unwrap().indices.len();
            assert!(tris(&many) > tris(&few));
        }
    }

    #[test]
    fn jackalopes_are_large_hares_with_long_hind_legs_and_ears() {
        for seed in 0..32 {
            let j = JackalopeState::roll(seed);
            let d = j.dims();
            assert!((7.5..=10.0).contains(&j.length), "{seed}: {}", j.length);
            // The haunches stand higher than the shoulders, and the hind leg outreaches the fore.
            let rest = crate::pose::jackalope(&j, &crate::pose::JackalopePose::default());
            let world = j.build().world(&rest);
            let hip = world[bone::LEGS + 6].origin[1];
            let shoulder = world[bone::LEGS].origin[1];
            assert!(hip > shoulder + 0.02 * j.length, "{seed}: hip {hip} shoulder {shoulder}");
            assert!(d.hind.upper + d.hind.lower > 1.2 * (d.fore.upper + d.fore.lower));
            // Its ears stand well over its head.
            let (lo, hi) = j.build().rest_mesh().bounds().unwrap();
            assert!(hi[1] > 1.2 * j.height(), "{seed}: ears reach {} of a back {}", hi[1], j.height());
            // Standing on the floor, not in it or over it.
            assert!(lo[1] > -0.02 * j.length && lo[1] < 0.02 * j.length, "{seed}: lowest point {}", lo[1]);
            // Its body fits the Small space it is drawn in.
            assert!(j.length <= j.drawn_size().space());
        }
    }

    #[test]
    fn colourings_and_antlers_vary_across_seeds() {
        let all: Vec<JackalopeState> = (0..200).map(JackalopeState::roll).collect();
        let colourings: HashSet<_> = all.iter().map(|j| j.colouring).collect();
        assert_eq!(colourings.len(), JackalopeColouring::ALL.len());
        let tines: HashSet<_> = all.iter().map(|j| j.tines).collect();
        assert!(tines.len() >= 6, "{} antler pairings", tines.len());
        assert!(all.iter().any(|j| j.tines[0] != j.tines[1]), "some are lopsided");
    }
}
