//! Rats and giant rats: [`RatState`] from a seed, and its [`Rig`].
//!
//! The rig is a body, a head, four legs of three bones each (upper leg, shin, paw) and a tail
//! of [`TAIL_BONES`] bones, so the game can trot it, swing its tail and lunge its head without a
//! skeleton. Fur is the body's own colour with tufts standing out of it along the back and
//! flanks; ears, paws and tail are bare skin; the eyes are wet black (or red) beads with a
//! glint; whiskers fan from the muzzle; the incisors are orange, as rats' enamel is.
//!
//! Sizes, proportions, colourings and patterns are our own proposal. The colour and pattern
//! names follow the varieties the American Fancy Rat and Mouse Association's standards list
//! (<https://www.afrma.org/rstndrd.htm>): agouti, black, blue, fawn, albino; self (plain),
//! Berkshire (white belly), hooded and variegated. Rats met in the abyss are mostly the wild
//! agouti and black.

use armory::geom::vec3::{self, Mat3, V3};
use armory::geom::{CatmullRom, GridOptions, LatheOptions, MeshBuilder, OutlineMode, Shading, cuboid, lathe, spike, thick_sheet, tube, uv_sphere, SweepOptions};
use armory::{MaterialKind, Part, Rgb};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, PI};

use crate::rig::Rig;
use crate::shape::{Trunk, eye};
use crate::{Size, Species};

/// Bones in a rat's tail.
pub const TAIL_BONES: usize = 4;

/// The legs in bone order, each with its side (+1 right, -1 left, in the spine frame's x) and
/// whether it is a hind leg.
pub const LEGS: [(&str, f32, bool); 4] = [("leg_front_left", -1.0, false), ("leg_front_right", 1.0, false), ("leg_hind_left", -1.0, true), ("leg_hind_right", 1.0, true)];

/// Bone indices in a rat's [`Rig`].
pub mod bone {
    pub const BODY: usize = 0;
    pub const HEAD: usize = 1;
    /// The first leg's upper bone; leg `k` (in [`super::LEGS`] order) has its upper leg at
    /// `LEGS + 3k`, its shin after it and its paw after that.
    pub const LEGS: usize = 2;
    pub const TAIL: usize = LEGS + 12;
    pub const COUNT: usize = TAIL + super::TAIL_BONES;
}

/// A rat's coat colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RatColouring {
    /// The wild brown, each hair banded dark and light.
    Agouti,
    Black,
    /// Slate grey.
    Blue,
    /// Warm orange-tan, with ruby eyes.
    Fawn,
    /// White, with pink-red eyes.
    Albino,
}

impl RatColouring {
    pub const ALL: [RatColouring; 5] = [RatColouring::Agouti, RatColouring::Black, RatColouring::Blue, RatColouring::Fawn, RatColouring::Albino];

    /// The coat's colour. Our own picks.
    pub fn fur(self) -> Rgb {
        match self {
            RatColouring::Agouti => Rgb::hex(0x7a5f45),
            RatColouring::Black => Rgb::hex(0x2b2522),
            RatColouring::Blue => Rgb::hex(0x676b72),
            RatColouring::Fawn => Rgb::hex(0xb07a46),
            RatColouring::Albino => Rgb::hex(0xebe5d9),
        }
    }

    /// The paler belly the coat fades to underneath.
    pub fn belly(self) -> Rgb {
        match self {
            RatColouring::Agouti => Rgb::hex(0xb5a690),
            RatColouring::Black => Rgb::hex(0x3d3530),
            RatColouring::Blue => Rgb::hex(0x9a9ca2),
            RatColouring::Fawn => Rgb::hex(0xdcc6a2),
            RatColouring::Albino => Rgb::hex(0xf3eee5),
        }
    }

    /// Bare skin: ears, paws, tail and nose.
    pub fn skin(self) -> Rgb {
        match self {
            RatColouring::Agouti | RatColouring::Black | RatColouring::Blue => Rgb::hex(0xa88880),
            RatColouring::Fawn | RatColouring::Albino => Rgb::hex(0xdca59b),
        }
    }

    pub fn eyes(self) -> Rgb {
        match self {
            RatColouring::Fawn => Rgb::hex(0x5c1414),
            RatColouring::Albino => Rgb::hex(0xc22a3c),
            _ => Rgb::hex(0x0d0b0b),
        }
    }

    /// Whiskers: pale on a dark coat, dark on a pale one.
    pub fn whiskers(self) -> Rgb {
        match self {
            RatColouring::Fawn | RatColouring::Albino => Rgb::hex(0x8a7d72),
            _ => Rgb::hex(0xd8d2c6),
        }
    }
}

/// Where a rat's coat is white.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RatPattern {
    /// One colour all over, fading paler under the belly ("self").
    Plain,
    /// A white belly.
    Berkshire,
    /// White, with the colour on the head and shoulders and a stripe down the back.
    Hooded,
    /// White, with the colour on the head and in patches over the back.
    Variegated,
}

impl RatPattern {
    pub const ALL: [RatPattern; 4] = [RatPattern::Plain, RatPattern::Berkshire, RatPattern::Hooded, RatPattern::Variegated];
}

/// White where a patterned coat is white.
const WHITE_FUR: Rgb = Rgb::hex(0xece6da);
/// Rats' incisors are orange-yellow: their enamel carries iron.
const INCISOR: Rgb = Rgb::hex(0xd9a03c);
const CLAW: Rgb = Rgb::hex(0x3b302a);
/// Glints on the eyes.
const GLINT: Rgb = Rgb::hex(0xf4f1ea);

/// One rat, every choice the seed makes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RatState {
    pub species: Species,
    pub seed: u32,
    /// Head and body, nose to rump, in world units.
    pub length: f32,
    /// 0 lean to 1 stout.
    pub build: f32,
    /// The tail's length as a share of [`RatState::length`].
    pub tail: f32,
    /// Ear size, 1 usual.
    pub ears: f32,
    /// Whisker length as a share of [`RatState::length`].
    pub whiskers: f32,
    pub colouring: RatColouring,
    pub pattern: RatPattern,
}

/// Salt for the rat stream, so a rat and a snake from one seed are unrelated. Our own.
const RAT_SALT: u32 = 0x0047_A7E1;

impl RatState {
    /// A rat or giant rat from `seed` ([`crate::rat`]).
    pub fn roll(seed: u32) -> RatState {
        let mut rng = Mulberry32::new(seed ^ RAT_SALT ^ 0x5151);
        let species = if rng.chance(0.2) { Species::GiantRat } else { Species::Rat };
        RatState::roll_as(seed, species)
    }

    /// A rat of `species` (a rat or a giant rat) from `seed`.
    pub fn roll_as(seed: u32, species: Species) -> RatState {
        let species = if species.is_rat() { species } else { Species::Rat };
        let mut rng = Mulberry32::new(seed ^ RAT_SALT);
        let giant = species == Species::GiantRat;
        // A rat of 45 to 60 cm nose to rump, twice a brown rat's, and a giant rat of 1 to 1.5 m
        // that stands to a delver's shin or knee: our own departure from the SRD's sizes
        // ([`RatState::drawn_size`]), so a pack reads as a threat. Hit points and bite stay
        // the stat block's.
        let length = if giant { rng.range_f32(10.0, 15.0) } else { rng.range_f32(4.5, 6.0) };
        let colouring = *rng.weighted(&[(RatColouring::Agouti, 6), (RatColouring::Black, 4), (RatColouring::Blue, 2), (RatColouring::Fawn, 1), (RatColouring::Albino, 1)]);
        let pattern = *rng.weighted(&[(RatPattern::Plain, 6), (RatPattern::Berkshire, 2), (RatPattern::Hooded, 1), (RatPattern::Variegated, 1)]);
        RatState {
            species,
            seed,
            length,
            build: rng.range_f32(0.0, 1.0),
            tail: rng.range_f32(0.85, 1.05),
            ears: rng.range_f32(0.85, 1.15) * if giant { 0.85 } else { 1.0 },
            whiskers: rng.range_f32(0.26, 0.36),
            colouring,
            pattern,
        }
    }

    /// Its proportions, which the rig and its poses share.
    pub fn dims(&self) -> RatDims {
        let l = self.length;
        let girth = 0.88 + 0.3 * self.build;
        let half_width = 0.15 * l * girth;
        let half_height = 0.13 * l * (0.94 + 0.12 * self.build);
        let body_length = 0.7 * l;
        let hip_height = 0.17 * l;
        // The hips and shoulders sit inside the trunk, a little under its centre line.
        let centre_height = hip_height + 0.35 * half_height;
        RatDims {
            length: l,
            half_width,
            half_height,
            body_length,
            head_length: 0.36 * l,
            centre_height,
            hip: [0.62 * half_width, -0.3 * body_length, -0.35 * half_height],
            shoulder: [0.55 * half_width, 0.27 * body_length, -0.35 * half_height],
            neck: [0.0, 0.43 * body_length, -0.02 * half_height],
            // Crouched: a leg's bones together are well over the joint's height, so it reaches
            // forward and back through a stride.
            hind: Leg { upper: 0.105 * l, lower: 0.105 * l, ankle: 0.04 * l, paw: 0.17 * l },
            fore: Leg { upper: 0.095 * l, lower: 0.09 * l, ankle: 0.03 * l, paw: 0.075 * l },
            tail_root: [0.0, -0.47 * body_length, 0.12 * half_height],
            tail_length: self.tail * l,
            tail_radius: 0.045 * l * girth,
        }
    }

    /// Its SRD size category, which its hit points and bite go with.
    pub fn size(&self) -> Size {
        self.species.stat_block().size
    }

    /// The size its body is drawn at, one up from the SRD's: a rat Small where the SRD's is
    /// Tiny, a giant rat Medium where the SRD's is Small. Our own departure; its body, nose to
    /// rump, fits this size's space.
    pub fn drawn_size(&self) -> Size {
        if self.species == Species::GiantRat { Size::Medium } else { Size::Small }
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
        let mut rng = Mulberry32::new(self.seed ^ RAT_SALT ^ 0xB0D7);
        let c = self.colouring;
        let patterned = matches!(self.pattern, RatPattern::Hooded | RatPattern::Variegated);
        let coat = if patterned { WHITE_FUR } else { c.fur() };
        let tail_tint = mix(c.skin(), c.fur(), 0.25);
        let rest = crate::pose::rat(self, &crate::pose::RatPose::default());
        let mut rig = Rig::default();

        // Body.
        let trunk = self.body_trunk(&d);
        let mut fur = trunk.mesh(22, 28);
        fur.append(&trunk.tufts(90, 0.075 * d.length, &mut rng, |t, a| (0.05..0.93).contains(&t) && a.sin() > -0.4));
        let mut body = vec![part(fur, "fur", MaterialKind::Hair, coat, true)];
        let off = 0.006 * d.length;
        let belly_width = if self.pattern == RatPattern::Berkshire { 1.25 } else { 0.95 };
        let belly_tint = if self.pattern == RatPattern::Berkshire || patterned { WHITE_FUR } else { c.belly() };
        if let Some(m) = trunk.patch(0.1, 0.9, -FRAC_PI_2, |t| belly_width * bell(t, 0.1, 0.9), off, false) {
            body.push(part(m, "belly", MaterialKind::Hair, belly_tint, false));
        }
        match self.pattern {
            RatPattern::Hooded => {
                let mut hood = MeshBuilder::new();
                hood.append(&trunk.patch(0.66, 1.0, FRAC_PI_2, |_| PI, off, false).unwrap_or_default());
                hood.append(&trunk.patch(0.04, 0.7, FRAC_PI_2, |t| 0.28 * bell(t, 0.04, 0.7).sqrt(), off * 1.5, false).unwrap_or_default());
                body.push(part(hood, "markings", MaterialKind::Hair, c.fur(), false));
            }
            RatPattern::Variegated => {
                let mut patches = MeshBuilder::new();
                patches.append(&trunk.patch(0.72, 1.0, FRAC_PI_2, |_| PI, off, false).unwrap_or_default());
                for _ in 0..rng.int(3, 6) {
                    let (t0, len) = (rng.range_f32(0.06, 0.62), rng.range_f32(0.08, 0.2));
                    let (centre, width) = (FRAC_PI_2 + rng.range_f32(-0.9, 0.9), rng.range_f32(0.35, 0.7));
                    patches.append(&trunk.patch(t0, t0 + len, centre, |t| width * bell(t, t0, t0 + len).sqrt(), off * 1.5, false).unwrap_or_default());
                }
                body.push(part(patches, "markings", MaterialKind::Hair, c.fur(), false));
            }
            _ => {}
        }
        let body_bone = rig.add("body", None, rest[bone::BODY], body);

        // Head: always the coat's colour, a hooded or variegated rat's too.
        rig.add("head", Some(body_bone), rest[bone::HEAD], self.head_parts(&d, &mut rng, c.fur()));

        // Legs: upper leg (fur), shin (fur, then bare at the ankle) and paw (bare, with toes
        // and claws).
        for (k, &(name, _, hind)) in LEGS.iter().enumerate() {
            let leg = if hind { d.hind } else { d.fore };
            let girth = if hind { 0.5 } else { 0.32 } * d.half_width;
            let upper = rig.add(name, Some(body_bone), rest[bone::LEGS + 3 * k], vec![part(upper_leg(leg.upper, girth, hind), "fur", MaterialKind::Hair, coat, true)]);
            let shin_fur = coat;
            let shin = rig.add(format!("{name}_shin"), Some(upper), rest[bone::LEGS + 3 * k + 1], vec![part(shin_mesh(leg.lower, girth), "fur", MaterialKind::Hair, shin_fur, true)]);
            let (paw, claws) = paw_mesh(&leg, girth, hind);
            rig.add(
                format!("{name}_paw"),
                Some(shin),
                rest[bone::LEGS + 3 * k + 2],
                vec![part(paw, "paw", MaterialKind::Skin, c.skin(), true), part(claws, "claws", MaterialKind::Bone, CLAW, false)],
            );
        }

        // Tail.
        let mut parent = body_bone;
        let segment = d.tail_length / TAIL_BONES as f32;
        for k in 0..TAIL_BONES {
            let (s0, s1) = (k as f32 / TAIL_BONES as f32, (k + 1) as f32 / TAIL_BONES as f32);
            let r = |s: f32| d.tail_radius * (1.0 - 0.82 * s);
            let mesh = tail_segment(segment, r(s0), r(s1), k + 1 == TAIL_BONES);
            let scales = armory::fabric::SurfacePattern { kind: armory::fabric::PatternKind::Scales, color: tail_tint, scale: 40.0 / d.length.sqrt() };
            let tail = part(mesh, "tail", MaterialKind::Skin, tail_tint, true).with_pattern(Some(scales));
            parent = rig.add(format!("tail_{k}"), Some(parent), rest[bone::TAIL + k], vec![tail]);
        }
        debug_assert_eq!(rig.bones.len(), bone::COUNT);
        rig
    }

    /// The body's trunk in its own spine frame, centred on the body bone.
    pub fn body_trunk(&self, d: &RatDims) -> Trunk {
        let (w, h) = (d.half_width, d.half_height);
        Trunk {
            start: -0.5 * d.body_length,
            length: d.body_length,
            width: vec![(0.0, 0.0), (0.035, 0.5 * w), (0.15, 0.88 * w), (0.32, w), (0.55, 0.9 * w), (0.78, 0.7 * w), (0.94, 0.48 * w), (1.0, 0.0)],
            height: vec![(0.0, 0.0), (0.035, 0.55 * h), (0.18, 0.95 * h), (0.38, h), (0.6, 0.88 * h), (0.8, 0.74 * h), (0.94, 0.5 * h), (1.0, 0.0)],
            lift: vec![(0.0, 0.05 * h), (0.3, 0.16 * h), (0.7, 0.06 * h), (1.0, -0.04 * h)],
            squareness: 2.15,
            belly: 0.6,
        }
    }

    /// The head's trunk in the head bone's frame (the neck at its origin, snout along +Y).
    pub fn head_trunk(&self, d: &RatDims) -> Trunk {
        let (w, h, l) = (0.135 * d.length * (0.92 + 0.16 * self.build), 0.115 * d.length, d.head_length);
        Trunk {
            start: -0.1 * d.length,
            length: l,
            width: vec![(0.0, 0.0), (0.06, 0.6 * w), (0.26, 0.97 * w), (0.42, 0.86 * w), (0.7, 0.47 * w), (0.9, 0.26 * w), (0.975, 0.14 * w), (1.0, 0.0)],
            height: vec![(0.0, 0.0), (0.06, 0.62 * h), (0.3, 0.94 * h), (0.5, 0.74 * h), (0.75, 0.44 * h), (0.92, 0.25 * h), (0.975, 0.14 * h), (1.0, 0.0)],
            lift: vec![(0.0, 0.0), (0.3, 0.03 * h), (0.75, -0.05 * h), (1.0, -0.2 * h)],
            squareness: 2.1,
            belly: 0.7,
        }
    }

    fn head_parts(&self, d: &RatDims, rng: &mut Mulberry32, fur: Rgb) -> Vec<Part> {
        let c = self.colouring;
        let head = self.head_trunk(d);
        let l = d.length;
        let mut skin = head.mesh(20, 24);
        skin.append(&head.tufts(26, 0.055 * l, rng, |t, a| (0.12..0.55).contains(&t) && a.sin() > -0.3));
        let mut parts = vec![part(skin, "fur", MaterialKind::Hair, fur, true)];

        // Ears: rounded, cupped and thin, standing up and back from the crown.
        let mut ears = MeshBuilder::new();
        for side in [1.0f32, -1.0] {
            let a = if side > 0.0 { FRAC_PI_2 - 0.62 } else { FRAC_PI_2 + 0.62 };
            let t = 0.24;
            let n = head.normal(t, a);
            let root = head.point(t, a, -0.012 * l);
            let up = vec3::normalize_or(vec3::add(vec3::scale(n, 0.75), [0.18 * side, -0.35, 0.55]), n);
            let facing = vec3::normalize_or([0.35 * side, 1.0, 0.0], vec3::Y);
            ears.append(&ear(0.06 * l * self.ears, 0.105 * l * self.ears, 0.007 * l, root, up, facing));
        }
        parts.push(part(ears, "ears", MaterialKind::Skin, mix(c.skin(), c.fur(), 0.3), true));

        // Eyes: wet beads set into the side of the head, a glint on each.
        let (mut balls, mut glints) = (MeshBuilder::new(), MeshBuilder::new());
        let r = 0.024 * l;
        for a in [0.55f32, PI - 0.55] {
            let t = 0.5;
            let n = head.normal(t, a);
            let at = head.point(t, a, -0.35 * r);
            let (ball, glint) = eye(at, r, n, [0.0, 0.6, 0.8]);
            balls.append(&ball);
            glints.append(&glint);
        }
        parts.push(part(balls, "eyes", MaterialKind::Gem, c.eyes(), true));
        parts.push(part(glints, "glints", MaterialKind::Gem, GLINT, false).with_glow(0.5));

        // Nose: a pink pad at the snout's tip.
        let mut nose = uv_sphere(0.028 * l, 12, 8);
        nose.scale([1.15, 0.85, 0.8]).translate(head.point(0.985, FRAC_PI_2, -0.01 * l));
        parts.push(part(nose, "nose", MaterialKind::Skin, mix(c.skin(), Rgb::hex(0xe48f8f), 0.5), true));

        // Whiskers: fanning out from the muzzle and sweeping back, slightly drooping.
        let mut whiskers = MeshBuilder::new();
        for side in [1.0f32, -1.0] {
            for k in 0..7 {
                let a_side = if side > 0.0 { 0.0 } else { PI };
                let rise = -0.45 + 0.13 * k as f32;
                let a = a_side + side * rise;
                let t = 0.86 + 0.012 * (k % 3) as f32;
                let root = head.point(t, a, -0.004 * l);
                let len = self.whiskers * l * rng.range_f32(0.7, 1.1);
                let out = [side, 0.0, 0.0];
                let path: Vec<V3> = (0..=6)
                    .map(|i| {
                        let s = i as f32 / 6.0;
                        let spread = vec3::add(vec3::scale(out, 0.75 * s), [0.0, -0.45 * s * s + 0.15 * s, (rise * 0.6 - 0.2 * s) * s]);
                        vec3::add(root, vec3::scale(spread, len))
                    })
                    .collect();
                let base = 0.0035 * l;
                whiskers.append(&tube(&path, 4, &SweepOptions { cap_end: false, ..Default::default() }, |s| base * (1.0 - 0.75 * s)));
            }
        }
        parts.push(part(whiskers, "whiskers", MaterialKind::Hair, c.whiskers(), false));

        // Incisors: two orange teeth under the snout's tip.
        let mut teeth = MeshBuilder::new();
        let tip = head.point(0.93, -FRAC_PI_2, -0.004 * l);
        let long = if self.species == Species::GiantRat { 1.45 } else { 1.0 };
        for side in [1.0f32, -1.0] {
            let mut tooth = cuboid([0.011 * l, 0.009 * l, 0.034 * l * long]);
            tooth.rotate_x(-0.25).translate(vec3::add(tip, [side * 0.0065 * l, 0.0, -0.012 * l * long]));
            teeth.append(&tooth);
        }
        parts.push(part(teeth, "teeth", MaterialKind::Bone, INCISOR, false));
        parts
    }
}

/// A rat's proportions: everything its rig and poses share.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RatDims {
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
}

/// One leg's lengths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leg {
    /// Hip (or shoulder) to knee (or elbow).
    pub upper: f32,
    /// Knee to ankle (or wrist).
    pub lower: f32,
    /// The ankle joint's height over the floor with the paw flat.
    pub ankle: f32,
    /// The paw's length, heel to toe.
    pub paw: f32,
}

impl RatDims {
    /// Where leg `k` (in [`LEGS`] order) joins the body, in the body bone's spine frame.
    pub fn leg_root(&self, k: usize) -> V3 {
        let (_, side, hind) = LEGS[k];
        let p = if hind { self.hip } else { self.shoulder };
        [p[0] * side, p[1], p[2]]
    }

    pub fn leg(&self, k: usize) -> Leg {
        if LEGS[k].2 { self.hind } else { self.fore }
    }
}

/// Mixes `a` toward `b` by `t`.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    Rgb(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}

/// A soft bump over `t0..t1`: 0 at both ends, 1 across the middle.
fn bell(t: f32, t0: f32, t1: f32) -> f32 {
    let s = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
    (4.0 * s * (1.0 - s)).clamp(0.0, 1.0).sqrt()
}

/// Finishes a mesh into a part with a material and colour.
pub(crate) fn part(mesh: MeshBuilder, name: &'static str, material: MaterialKind, tint: Rgb, outline: bool) -> Part {
    let part = mesh.into_part(name, OutlineMode::Welded).with_material(material).with_tint(tint);
    if outline { part } else { part.without_outline() }
}

/// An ear: a rounded, cupped sheet `width` across and `height` tall, `thickness` thick, its
/// root at `root`, standing along `up` with its hollow facing `facing`.
fn ear(width: f32, height: f32, thickness: f32, root: V3, up: V3, facing: V3) -> MeshBuilder {
    let (cols, rows) = (9usize, 8usize);
    let points: Vec<Vec<V3>> = (0..=rows)
        .map(|r| {
            let v = r as f32 / rows as f32;
            let half = width * 0.5 * (PI * (0.12 + 0.88 * v)).sin().max(0.0).powf(0.6);
            (0..=cols)
                .map(|c| {
                    let u = c as f32 / cols as f32 * 2.0 - 1.0;
                    // Cupped: the middle sits back from the rim, most near the top.
                    [u * half, v * height, -0.35 * half * (1.0 - u * u) * (0.4 + 0.6 * v)]
                })
                .collect()
        })
        .collect();
    let mut sheet = thick_sheet(&points, &GridOptions { wrap_around: false, wrap_along: false, shading: Shading::SMOOTH, row_v: None }, thickness);
    sheet.place_basis(root, up, facing);
    sheet
}

/// An upper leg hanging down -Z from its joint, `length` long: a haunch or shoulder bulging
/// into the body, tapering to the knee.
fn upper_leg(length: f32, girth: f32, hind: bool) -> MeshBuilder {
    let top = if hind { 1.0 } else { 0.8 };
    let profile = armory::geom::spline_profile(
        &[[0.0, -length - 0.35 * girth], [0.45 * girth, -length], [0.62 * girth, -0.55 * length], [top * girth, -0.15 * length], [0.75 * top * girth, 0.25 * girth], [0.0, 0.6 * girth]],
        18,
    );
    let mut m = lathe(&profile, &LatheOptions::new(12, Shading::SMOOTH));
    // Lathed about +Y; stand it along -Z... rather, the profile runs up +Y, so turn +Y to +Z.
    m.transform(&Mat3::from_cols([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]));
    // Flatter across the body than front to back.
    m.scale([0.8, 1.0, 1.0]);
    m
}

/// A shin hanging down -Z from the knee, `length` long, slender to the ankle.
fn shin_mesh(length: f32, girth: f32) -> MeshBuilder {
    let profile = armory::geom::spline_profile(&[[0.0, -length - 0.2 * girth], [0.26 * girth, -length], [0.36 * girth, -0.5 * length], [0.46 * girth, 0.0], [0.0, 0.42 * girth]], 14);
    let mut m = lathe(&profile, &LatheOptions::new(10, Shading::SMOOTH));
    m.transform(&Mat3::from_cols([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]));
    m
}

/// A paw: a flat pad forward of the ankle (a long sole on a hind foot), with toes and claws,
/// lying on the floor `leg.ankle` under the ankle joint at its origin. Returns the paw and its
/// claws.
fn paw_mesh(leg: &Leg, girth: f32, hind: bool) -> (MeshBuilder, MeshBuilder) {
    let (len, wide, thick) = (leg.paw, if hind { 0.3 } else { 0.32 } * girth, leg.ankle);
    let mut paw = MeshBuilder::new();
    // The sole: from the heel (just behind the ankle) to the ball of the foot.
    let heel = if hind { -0.12 * len } else { -0.2 * len };
    let path = [[0.0, heel, -0.45 * thick], [0.0, 0.25 * len, -0.62 * thick], [0.0, 0.62 * len, -0.68 * thick]];
    paw.append(&tube(&path, 10, &SweepOptions::default(), |s| (0.5 + 0.5 * (std::f32::consts::PI * (0.2 + 0.8 * s)).sin()) * wide * 0.5));
    paw.scale([1.0, 1.0, 0.75]);
    // The ankle, so the shin meets the paw.
    let mut ankle = uv_sphere(0.5 * wide, 10, 7);
    ankle.scale([1.0, 1.0, 1.2]).translate([0.0, 0.0, -0.2 * thick]);
    paw.append(&ankle);
    let mut claws = MeshBuilder::new();
    let toes = if hind { 5 } else { 4 };
    for i in 0..toes {
        let u = i as f32 / (toes - 1) as f32 * 2.0 - 1.0;
        let spread = u * 0.42;
        let reach = if hind { 0.38 } else { 0.5 } * len * (1.0 - 0.25 * u * u);
        let base = [u * wide * 0.4, 0.6 * len, -0.68 * thick];
        let dir = vec3::normalize_or([spread.sin(), spread.cos(), -0.12], vec3::Y);
        let tip = vec3::add(base, vec3::scale(dir, reach));
        let toe_path = CatmullRom::centripetal(vec![base, vec3::lerp(base, tip, 0.5), tip]).sample(3);
        paw.append(&tube(&toe_path, 6, &SweepOptions::default(), |s| 0.11 * wide * (1.0 - 0.3 * s)));
        let mut claw = spike(0.07 * wide, 0.18 * len.max(0.3 * wide), 4, 0.8, Shading::FLAT);
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

/// One bone of the tail: a tapering tube from its joint back along -Y, `length` long, from
/// radius `r0` to `r1`, rounded at the joint so the bones overlap without a gap as it bends,
/// and pointed at the tip.
fn tail_segment(length: f32, r0: f32, r1: f32, tip: bool) -> MeshBuilder {
    let end = if tip { [0.0, -length - 2.0 * r1] } else { [0.0, -length - r1] };
    let mut profile = vec![end];
    if !tip {
        profile.push([0.7 * r1, -length - 0.7 * r1]);
    }
    for k in 0..=6 {
        let s = k as f32 / 6.0;
        profile.push([r1 + (r0 - r1) * s, -length * (1.0 - s)]);
    }
    profile.push([0.7 * r0, 0.7 * r0]);
    profile.push([0.0, r0]);
    lathe(&profile, &LatheOptions::new(10, Shading::SMOOTH))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn rats_are_deterministic() {
        for seed in [0, 7, 1207, u32::MAX] {
            assert_eq!(RatState::roll(seed), RatState::roll(seed));
            assert_eq!(RatState::roll(seed).build(), RatState::roll(seed).build());
        }
        assert_ne!(RatState::roll(1), RatState::roll(2));
    }

    #[test]
    fn every_rat_part_validates() {
        for seed in 0..24 {
            for species in [Species::Rat, Species::GiantRat] {
                let rat = RatState::roll_as(seed, species);
                let rig = rat.build();
                rig.validate().unwrap_or_else(|e| panic!("{species:?} {seed}: {e}"));
                assert_eq!(rig.bones.len(), bone::COUNT);
                for name in ["body", "head", "leg_front_left", "leg_hind_right", "tail_0", "tail_3"] {
                    assert!(rig.index(name).is_some(), "{name}");
                }
            }
        }
    }

    #[test]
    fn rats_have_fur_ears_whiskers_and_eyes_that_read() {
        let rig = RatState::roll_as(5, Species::Rat).build();
        let head = rig.bone("head").unwrap();
        let names: HashSet<&str> = head.mesh.parts.iter().map(|p| p.name).collect();
        for name in ["fur", "ears", "eyes", "glints", "nose", "whiskers", "teeth"] {
            assert!(names.contains(name), "{name}");
        }
        // Fur tufts make the body much more than a smooth blob.
        let fur = &rig.bone("body").unwrap().mesh.parts[0];
        assert!(fur.indices.len() / 3 > 2000, "{} triangles", fur.indices.len() / 3);
        // A glint glows a little so the eye catches the light.
        assert!(head.mesh.parts.iter().any(|p| p.name == "glints" && p.glow > 0.0));
    }

    #[test]
    fn rats_are_drawn_a_size_up_from_the_srd() {
        for seed in 0..32 {
            for species in [Species::Rat, Species::GiantRat] {
                let rat = RatState::roll_as(seed, species);
                let size = rat.drawn_size();
                assert_eq!(size.smaller(), Some(rat.size()));
                let (lo, hi) = rat.build().rest_mesh().bounds().unwrap();
                let body = rat.length;
                let whole = (hi[2] - lo[2]).max(hi[0] - lo[0]);
                // The body fits its space; tail and all, it is bigger than the size below.
                assert!(body <= size.space(), "{species:?} {seed}: {body} > {}", size.space());
                if let Some(smaller) = size.smaller() {
                    assert!(whole > smaller.space(), "{species:?} {seed}: {whole} fits a {smaller:?} space");
                }
                // 45 to 60 cm for a rat; a giant rat 1 to 1.5 m, its back at a delver's shin or
                // knee (a 17-unit human's shin is about 3 units up, the knee 4.8).
                let (short, long) = if species == Species::GiantRat { (10.0, 15.0) } else { (4.5, 6.0) };
                assert!((short..=long).contains(&body), "{species:?} {seed}: {body}");
                if species == Species::GiantRat {
                    assert!((3.0..=crate::pose::KNEE + 0.5).contains(&rat.height()), "{seed}: back at {}", rat.height());
                }
                // Standing on the floor, not in it or over it.
                assert!(lo[1] > -0.02 * body && lo[1] < 0.02 * body, "{species:?} {seed}: lowest point {}", lo[1]);
            }
        }
    }

    #[test]
    fn colourings_and_patterns_vary_across_seeds() {
        let rats: Vec<RatState> = (0..200).map(RatState::roll).collect();
        let colourings: HashSet<_> = rats.iter().map(|r| r.colouring).collect();
        let patterns: HashSet<_> = rats.iter().map(|r| r.pattern).collect();
        assert_eq!(colourings.len(), RatColouring::ALL.len());
        assert_eq!(patterns.len(), RatPattern::ALL.len());
        let giants = rats.iter().filter(|r| r.species == Species::GiantRat).count();
        assert!((20..=60).contains(&giants), "{giants} giant rats in 200");
    }
}
