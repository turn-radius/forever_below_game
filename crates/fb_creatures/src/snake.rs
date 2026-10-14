//! Poisonous snakes and constrictors: [`SnakeState`] from a seed, and its [`Rig`].
//!
//! The rig is a head with a jaw and a forked tongue, and a spine of [`SEGMENTS`] segments that
//! the game lays along a curve each frame ([`crate::pose::snake`]): a travelling wave to
//! slither, a flat spiral to coil, the front raised to rear and strike. Each segment is a
//! rounded length of body that overlaps its neighbours, so the body stays whole as it bends.
//! The skin carries the Armory's `Scales` pattern; the back carries markings (diamonds,
//! saddles, bands or a stripe) laid over it as patches whose scales line up with the skin's,
//! and the belly its broad scutes. The head has slit-pupilled eyes with a glint, nostrils, heat
//! pits on a pit viper, fangs or rows of small teeth, and a pale mouth lining that shows when
//! the jaw drops.
//!
//! Lengths, girths, colourings and patterns are our own proposal, after vipers and pit vipers
//! for the poisonous snake and boas and pythons for the constrictor.

use armory::fabric::{PatternKind, SurfacePattern};
use armory::geom::vec3::{self, V3};
use armory::geom::{GridOptions, MeshBuilder, Shading, SweepOptions, parametric_surface, spike, tube, uv_sphere};
use armory::{MaterialKind, Part, Rgb};
use fb_body::Placement;
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::rat::{mix, part};
use crate::rig::Rig;
use crate::shape::{Trunk, eye};
use crate::{Size, Species};

/// Segments in a snake's spine.
pub const SEGMENTS: usize = 22;

/// Bone indices in a snake's [`Rig`].
pub mod bone {
    pub const HEAD: usize = 0;
    pub const JAW: usize = 1;
    pub const TONGUE: usize = 2;
    /// The first spine segment, just behind the head; segment `k` is `SPINE + k`.
    pub const SPINE: usize = 3;
    pub const COUNT: usize = SPINE + super::SEGMENTS;
}

/// A snake's colours: back, belly, markings, eyes and tongue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SnakeColouring {
    /// Browns, with dark brown markings.
    Earth,
    /// Ash grey, with black markings.
    Ash,
    /// Olive green, with dark green markings and a yellow belly.
    Moss,
    /// Red, with black markings.
    Coral,
    /// Pale sand, with rust markings.
    Sand,
    /// Near black, with pale markings.
    Night,
}

impl SnakeColouring {
    pub const ALL: [SnakeColouring; 6] = [SnakeColouring::Earth, SnakeColouring::Ash, SnakeColouring::Moss, SnakeColouring::Coral, SnakeColouring::Sand, SnakeColouring::Night];

    /// Back, belly, markings, iris and tongue. Our own picks.
    pub fn colours(self) -> [Rgb; 5] {
        match self {
            SnakeColouring::Earth => [Rgb::hex(0x8a6a48), Rgb::hex(0xcdb894), Rgb::hex(0x3e2a1c), Rgb::hex(0xc9932e), Rgb::hex(0x1c1414)],
            SnakeColouring::Ash => [Rgb::hex(0x8d8a84), Rgb::hex(0xc8c3b8), Rgb::hex(0x1f1d1c), Rgb::hex(0xb8a24a), Rgb::hex(0x231c1e)],
            SnakeColouring::Moss => [Rgb::hex(0x5e6b34), Rgb::hex(0xd6c86a), Rgb::hex(0x27301a), Rgb::hex(0xd4b434), Rgb::hex(0x2a1d2a)],
            SnakeColouring::Coral => [Rgb::hex(0xa8392a), Rgb::hex(0xe0b884), Rgb::hex(0x191514), Rgb::hex(0x9c8a4a), Rgb::hex(0x24181a)],
            SnakeColouring::Sand => [Rgb::hex(0xc4a77a), Rgb::hex(0xe6d8b8), Rgb::hex(0x8a4a26), Rgb::hex(0xd88a2e), Rgb::hex(0x3a1c1c)],
            SnakeColouring::Night => [Rgb::hex(0x24221f), Rgb::hex(0x5a5650), Rgb::hex(0xbdb39a), Rgb::hex(0xb83a26), Rgb::hex(0xa02c3a)],
        }
    }

    pub fn back(self) -> Rgb {
        self.colours()[0]
    }
    pub fn belly(self) -> Rgb {
        self.colours()[1]
    }
    pub fn markings(self) -> Rgb {
        self.colours()[2]
    }
    pub fn eyes(self) -> Rgb {
        self.colours()[3]
    }
    pub fn tongue(self) -> Rgb {
        self.colours()[4]
    }
}

/// The markings down a snake's back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SnakePattern {
    Plain,
    /// A chain of diamonds point to point down the spine.
    Diamonds,
    /// Rounded blotches straddling the back.
    Saddles,
    /// Rings round the whole body.
    Bands,
    /// One stripe down the spine.
    Stripe,
}

impl SnakePattern {
    pub const ALL: [SnakePattern; 5] = [SnakePattern::Plain, SnakePattern::Diamonds, SnakePattern::Saddles, SnakePattern::Bands, SnakePattern::Stripe];

    /// How far round the body the markings reach either side of the spine at `s` along the
    /// body (0 the neck, 1 the tail's tip), radians: 0 where there is none, half a turn for a
    /// band all round. `count` is how many repeats run down the body.
    pub fn half_angle(self, s: f32, count: f32) -> f32 {
        if !(0.03..=0.95).contains(&s) {
            return 0.0;
        }
        let f = (s * count).fract();
        match self {
            SnakePattern::Plain => 0.0,
            SnakePattern::Diamonds => 1.15 * (1.0 - (2.0 * f - 1.0).abs() / 0.92).max(0.0),
            SnakePattern::Saddles => 1.05 * (1.0 - ((f - 0.5) / 0.3).powi(2)).max(0.0).sqrt(),
            SnakePattern::Bands => {
                if (f - 0.5).abs() < 0.17 {
                    PI
                } else {
                    0.0
                }
            }
            SnakePattern::Stripe => 0.24,
        }
    }

    /// Where along the body (0 neck to 1 tail) each marking starts and ends.
    pub fn spans(self, count: f32) -> Vec<(f32, f32)> {
        let one = |k: f32, a: f32, b: f32| ((k + a) / count, (k + b) / count);
        let mut spans: Vec<(f32, f32)> = match self {
            SnakePattern::Plain => vec![],
            SnakePattern::Stripe => vec![(0.03, 0.95)],
            SnakePattern::Diamonds => (0..count.ceil() as i32).map(|k| one(k as f32, 0.04, 0.96)).collect(),
            SnakePattern::Saddles => (0..count.ceil() as i32).map(|k| one(k as f32, 0.2, 0.8)).collect(),
            SnakePattern::Bands => (0..count.ceil() as i32).map(|k| one(k as f32, 0.33, 0.67)).collect(),
        };
        spans.retain_mut(|(a, b)| {
            *a = a.max(0.03);
            *b = b.min(0.95);
            b > a
        });
        spans
    }
}

/// White fangs, a pale mouth lining.
const FANG: Rgb = Rgb::hex(0xeee6d4);
const MOUTH: Rgb = Rgb::hex(0xd99a9a);
const DARK: Rgb = Rgb::hex(0x16110e);
const PUPIL: Rgb = Rgb::hex(0x0a0807);
const GLINT: Rgb = Rgb::hex(0xf4f1ea);

/// One snake, every choice the seed makes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnakeState {
    pub species: Species,
    pub seed: u32,
    /// Nose to tail tip, in world units.
    pub length: f32,
    /// The body's greatest radius.
    pub girth: f32,
    /// The head's half width over the body's girth: a viper's broad wedge, a constrictor's
    /// narrower head.
    pub head_width: f32,
    pub colouring: SnakeColouring,
    pub pattern: SnakePattern,
    /// Markings down the body.
    pub markings: f32,
    /// Which way it coils: 1 or -1.
    pub hand: f32,
}

const SNAKE_SALT: u32 = 0x05A4_E5ED;

impl SnakeState {
    /// A poisonous snake or a constrictor from `seed` ([`crate::snake`]).
    pub fn roll(seed: u32) -> SnakeState {
        let mut rng = Mulberry32::new(seed ^ SNAKE_SALT ^ 0x5151);
        let species = if rng.chance(2.0 / 3.0) { Species::PoisonousSnake } else { Species::ConstrictorSnake };
        SnakeState::roll_as(seed, species)
    }

    /// A snake of `species` (a poisonous snake or a constrictor) from `seed`.
    pub fn roll_as(seed: u32, species: Species) -> SnakeState {
        let species = if species.is_snake() { species } else { Species::PoisonousSnake };
        let mut rng = Mulberry32::new(seed ^ SNAKE_SALT);
        let constrictor = species == Species::ConstrictorSnake;
        // A viper of a metre or so; a Large constrictor three to nearly four metres long.
        let (length, girth) = if constrictor { (rng.range_f32(28.0, 38.0), rng.range_f32(0.6, 0.82)) } else { (rng.range_f32(8.5, 12.0), rng.range_f32(0.17, 0.24)) };
        let colouring = if constrictor {
            *rng.weighted(&[(SnakeColouring::Earth, 3), (SnakeColouring::Sand, 3), (SnakeColouring::Moss, 2), (SnakeColouring::Night, 2), (SnakeColouring::Ash, 1)])
        } else {
            *rng.weighted(&[(SnakeColouring::Earth, 3), (SnakeColouring::Ash, 3), (SnakeColouring::Moss, 2), (SnakeColouring::Coral, 2), (SnakeColouring::Sand, 2), (SnakeColouring::Night, 1)])
        };
        let pattern = if constrictor {
            *rng.weighted(&[(SnakePattern::Saddles, 4), (SnakePattern::Diamonds, 2), (SnakePattern::Plain, 1), (SnakePattern::Stripe, 1)])
        } else if colouring == SnakeColouring::Coral {
            SnakePattern::Bands
        } else {
            *rng.weighted(&[(SnakePattern::Diamonds, 4), (SnakePattern::Saddles, 2), (SnakePattern::Bands, 1), (SnakePattern::Stripe, 1), (SnakePattern::Plain, 1)])
        };
        SnakeState {
            species,
            seed,
            length,
            girth,
            head_width: if constrictor { rng.range_f32(1.05, 1.25) } else { rng.range_f32(1.4, 1.7) },
            colouring,
            pattern,
            markings: rng.range_f32(14.0, 22.0),
            hand: if rng.chance(0.5) { 1.0 } else { -1.0 },
        }
    }

    pub fn size(&self) -> Size {
        self.species.stat_block().size
    }

    pub fn dims(&self) -> SnakeDims {
        let head_length = 2.7 * self.girth;
        let body_length = self.length - 0.85 * head_length;
        let constrictor = self.species == Species::ConstrictorSnake;
        SnakeDims {
            girth: self.girth,
            head_length,
            body_length,
            segment: body_length / SEGMENTS as f32,
            neck: body_length * if constrictor { 0.24 } else { 0.3 },
            coil_inner: 1.6 * self.girth,
            coil_spacing: 2.15 * self.girth / TAU,
            constrictor,
        }
    }

    /// The body's radius `s` along it (0 the neck, 1 the tail's tip).
    pub fn radius_at(&self, s: f32) -> f32 {
        let keys: &[(f32, f32)] = if self.species == Species::ConstrictorSnake {
            &[(0.0, 0.5), (0.07, 0.68), (0.35, 1.0), (0.65, 0.95), (0.85, 0.55), (0.95, 0.25), (1.0, 0.06)]
        } else {
            &[(0.0, 0.55), (0.06, 0.7), (0.3, 1.0), (0.6, 0.92), (0.78, 0.6), (0.9, 0.28), (1.0, 0.05)]
        };
        self.girth * crate::shape::keyed(keys, s)
    }

    /// How far below a segment's centre line its belly lies, for a radius `r`: the trunk's
    /// pressed belly ([`Trunk::belly`] of [`BELLY`]).
    pub fn belly_depth(r: f32) -> f32 {
        DEPTH * r * (BELLY + (1.0 - BELLY) / 3.0)
    }

    /// The circle it covers on the floor coiled, about its centre: its segments' reach and
    /// the head's.
    pub fn radius(&self) -> f32 {
        let rest = self.rest();
        let reach = |p: &Placement| p.origin[0].hypot(p.origin[2]);
        let body = rest[bone::SPINE..].iter().map(reach).fold(0.0f32, f32::max) + self.girth;
        body.max(reach(&rest[bone::HEAD]) + self.dims().head_length)
    }

    /// Segment `k`'s trunk in its own spine frame, centred on its middle: a length of body
    /// rounded at both ends past its joints, so neighbours overlap.
    pub fn segment_trunk(&self, k: usize) -> Trunk {
        let d = self.dims();
        let l = d.segment;
        let s_of = |y: f32| ((k as f32 + 0.5) * l - y) / d.body_length;
        let (rf, rb) = (self.radius_at(s_of(0.5 * l)), self.radius_at(s_of(-0.5 * l)));
        // From the back cap's pole to the front cap's.
        let (start, end) = (-0.5 * l - rb, 0.5 * l + rf);
        let length = end - start;
        let samples = 14;
        let mut keys = Vec::with_capacity(samples + 1);
        for i in 0..=samples {
            let t = i as f32 / samples as f32;
            let y = start + t * length;
            let r = if y < -0.5 * l {
                let dy = y + 0.5 * l;
                (rb * rb - dy * dy).max(0.0).sqrt()
            } else if y > 0.5 * l {
                let dy = y - 0.5 * l;
                (rf * rf - dy * dy).max(0.0).sqrt()
            } else {
                self.radius_at(s_of(y))
            };
            keys.push((t, r));
        }
        Trunk { start, length, width: keys.clone(), height: keys.iter().map(|&(t, r)| (t, DEPTH * r)).collect(), lift: vec![(0.0, 0.0)], squareness: 2.2, belly: BELLY }
    }

    /// The head's trunk in the head bone's frame (the neck at its origin, snout along +Y).
    pub fn head_trunk(&self) -> Trunk {
        let d = self.dims();
        let (r, w) = (self.girth, self.head_width);
        let viper = !d.constrictor;
        let width = if viper {
            vec![(0.0, 0.0), (0.06, 0.62 * r), (0.32, 0.95 * w * r), (0.5, w * r), (0.74, 0.72 * w * r), (0.92, 0.42 * r), (0.98, 0.25 * r), (1.0, 0.0)]
        } else {
            vec![(0.0, 0.0), (0.06, 0.6 * r), (0.35, w * r), (0.6, 0.92 * w * r), (0.85, 0.62 * r), (0.96, 0.38 * r), (1.0, 0.0)]
        };
        Trunk {
            start: -0.35 * d.head_length,
            length: 1.3 * d.head_length,
            width,
            height: vec![(0.0, 0.0), (0.06, 0.6 * r), (0.35, 0.64 * r), (0.7, 0.52 * r), (0.92, 0.33 * r), (1.0, 0.0)],
            lift: vec![(0.0, 0.0), (0.5, 0.05 * r), (1.0, -0.08 * r)],
            squareness: 2.8,
            belly: 1.0,
        }
    }

    /// How far the head reaches below its bone, level: its chin on the floor puts the bone
    /// this high.
    pub fn head_depth(&self) -> f32 {
        let head = self.head_trunk();
        (0..=40).map(|i| i as f32 / 40.0).map(|t| head.half_height(t) - crate::shape::keyed(&head.lift, t)).fold(0.0, f32::max)
    }

    /// Height of the mouth's line in the head's frame, and the jaw's hinge.
    pub fn mouth(&self) -> (f32, V3) {
        let head = self.head_trunk();
        let line = -0.18 * self.girth;
        (line, [0.0, head.start + 0.24 * head.length, line])
    }

    fn scales(&self) -> SurfacePattern {
        // About two dozen scale rows round the body at its thickest.
        SurfacePattern { kind: PatternKind::Scales, color: self.colouring.markings(), scale: 4.0 / self.girth }
    }

    /// Its bones and meshes, coiled at rest.
    pub fn build(&self) -> Rig {
        let d = self.dims();
        let c = self.colouring;
        let rest = crate::pose::snake(self, &crate::pose::SnakePose::REST);
        let mut rig = Rig::default();
        let scales = Some(self.scales());

        // Head: the upper head down to the mouth's line, and a jaw under it.
        let head = self.head_trunk();
        let (line, hinge) = self.mouth();
        let upper = clamped(&head, 20, 22, |p| [p[0], p[1], p[2].max(line)]);
        let mut head_parts = vec![part(upper, "head", MaterialKind::Skin, c.back(), true).with_pattern(scales)];
        head_parts.extend(self.head_details(&head, line));
        let h = rig.add("head", None, rest[bone::HEAD], head_parts);

        let jaw_trunk = Trunk { width: head.width.iter().map(|&(t, w)| (t, 0.94 * w * if t > 0.93 { 0.0 } else { 1.0 })).collect(), ..head.clone() };
        let mut jaw = clamped(&jaw_trunk, 20, 22, |p| [p[0], p[1], p[2].min(line)]);
        jaw.translate(vec3::neg(hinge));
        let mut lining = mouth_lining(&jaw_trunk, line, -1.0);
        lining.translate(vec3::neg(hinge));
        let mut jaw_parts = vec![part(jaw, "jaw", MaterialKind::Skin, mix(c.back(), c.belly(), 0.5), true).with_pattern(scales), part(lining, "mouth", MaterialKind::Skin, MOUTH, false)];
        if d.constrictor {
            let mut teeth = teeth_row(&jaw_trunk, line, 1.0, self.girth);
            teeth.translate(vec3::neg(hinge));
            jaw_parts.push(part(teeth, "teeth", MaterialKind::Bone, FANG, false));
        }
        rig.add("jaw", Some(h), rest[bone::JAW], jaw_parts);

        // Tongue: forked, lying in the mouth until it flicks out.
        rig.add("tongue", Some(h), rest[bone::TONGUE], vec![part(self.tongue(), "tongue", MaterialKind::Skin, c.tongue(), false)]);

        // Spine.
        let marks = self.pattern.spans(self.markings);
        for k in 0..SEGMENTS {
            let trunk = self.segment_trunk(k);
            let skin = trunk.mesh(16, 18);
            let mut parts = vec![part(skin, "spine", MaterialKind::Skin, c.back(), true).with_pattern(scales)];
            let off = 0.012 * self.girth;
            // Segment `k` covers this much of the body, its caps aside.
            let (s0, s1) = (k as f32 / SEGMENTS as f32, (k + 1) as f32 / SEGMENTS as f32);
            // Local t on the trunk for a share `s` of the body: s grows toward the tail, -Y.
            let t_of = |s: f32| {
                let y = (k as f32 + 0.5) * d.segment - s * d.body_length;
                (y - trunk.start) / trunk.length
            };
            let s_of_t = |t: f32| ((k as f32 + 0.5) * d.segment - (trunk.start + t * trunk.length)) / d.body_length;
            let scutes = SurfacePattern { kind: PatternKind::Hoops, color: c.belly().scaled(0.86), scale: 1.0 / (0.3 * self.girth) };
            if let Some(m) = trunk.patch(t_of(s1), t_of(s0), -FRAC_PI_2, |_| 1.0, 0.004 * self.girth, true) {
                parts.push(part(m, "belly", MaterialKind::Skin, c.belly(), false).with_pattern(Some(scutes)));
            }
            let mut markings = MeshBuilder::new();
            for &(a, b) in &marks {
                let (a, b) = (a.max(s0), b.min(s1));
                if b <= a {
                    continue;
                }
                let pattern = self.pattern;
                let count = self.markings;
                if let Some(m) = trunk.patch(t_of(b), t_of(a), FRAC_PI_2, |t| pattern.half_angle(s_of_t(t), count), off, false) {
                    markings.append(&m);
                }
            }
            if !markings.is_empty() {
                trunk.anchor_wrap(&mut markings);
                parts.push(part(markings, "markings", MaterialKind::Skin, c.markings(), false).with_pattern(scales));
            }
            rig.add(format!("spine_{k}"), None, rest[bone::SPINE + k], parts);
        }
        debug_assert_eq!(rig.bones.len(), bone::COUNT);
        rig
    }

    fn head_details(&self, head: &Trunk, line: f32) -> Vec<Part> {
        let c = self.colouring;
        let d = self.dims();
        let r = self.girth;
        let mut parts = Vec::new();
        // Eyes: up the sides toward the front, each with a slit pupil and a glint; a viper's
        // brow scale juts over it.
        let (mut balls, mut pupils, mut glints, mut brows) = (MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new());
        let er = 0.21 * r;
        for (side, a) in [(1.0f32, 0.42f32), (-1.0, PI - 0.42)] {
            let t = if d.constrictor { 0.66 } else { 0.62 };
            let n = head.normal(t, a);
            let at = head.point(t, a, -0.3 * er);
            let (ball, glint) = eye(at, er, n, [0.0, 0.5, 0.8]);
            balls.append(&ball);
            glints.append(&glint);
            let mut slit = uv_sphere(1.0, 10, 8);
            slit.scale([0.2 * er, 0.85 * er, 0.12 * er]);
            slit.place_basis(vec3::add(at, vec3::scale(n, 0.93 * er)), vec3::Z, n);
            pupils.append(&slit);
            if !d.constrictor {
                let mut brow = uv_sphere(1.0, 10, 7);
                brow.scale([0.7 * er, 1.1 * er, 0.35 * er]);
                brow.place_basis(head.point(t, a + side * 0.42, 0.05 * er), n, vec3::Y);
                brows.append(&brow);
            }
        }
        parts.push(part(balls, "eyes", MaterialKind::Gem, c.eyes(), true));
        parts.push(part(pupils, "pupils", MaterialKind::Gem, PUPIL, false));
        parts.push(part(glints, "glints", MaterialKind::Gem, GLINT, false).with_glow(0.5));
        if !brows.is_empty() {
            parts.push(part(brows, "brows", MaterialKind::Skin, c.back(), true).with_pattern(Some(self.scales())));
        }
        // Nostrils near the tip, and a pit viper's heat pits between nostril and eye.
        let mut dark = MeshBuilder::new();
        for a in [0.55f32, PI - 0.55] {
            let mut nostril = uv_sphere(0.06 * r, 8, 6);
            nostril.translate(head.point(0.95, a, -0.02 * r));
            dark.append(&nostril);
            if !d.constrictor {
                let mut pit = uv_sphere(0.07 * r, 8, 6);
                pit.scale([1.0, 1.4, 1.0]).translate(head.point(0.8, a - (a - FRAC_PI_2).signum() * 0.5, -0.03 * r));
                dark.append(&pit);
            }
        }
        parts.push(part(dark, "nostrils", MaterialKind::Skin, DARK, false));
        // The mouth's lining, and fangs (a viper's pair, folded down from the front of the
        // upper jaw) or rows of small teeth (a constrictor's).
        parts.push(part(mouth_lining(head, line, 1.0), "mouth", MaterialKind::Skin, MOUTH, false));
        if d.constrictor {
            parts.push(part(teeth_row(head, line, -1.0, r), "teeth", MaterialKind::Bone, FANG, false));
        } else {
            let mut fangs = MeshBuilder::new();
            for side in [1.0f32, -1.0] {
                let t = 0.84;
                let w = head.half_width(t);
                let mut fang = spike(0.075 * r, 0.42 * r, 6, 0.7, Shading::SMOOTH);
                fang.place([side * 0.45 * w, head.centre(t)[1], line + 0.02 * r], vec3::normalize_or([0.0, -0.35, -1.0], vec3::Y));
                fangs.append(&fang);
            }
            parts.push(part(fangs, "fangs", MaterialKind::Bone, FANG, false));
        }
        parts
    }

    /// The forked tongue along +Y from the tongue bone, in its rest place inside the mouth.
    fn tongue(&self) -> MeshBuilder {
        let r = self.girth;
        let length = 0.85 * self.dims().head_length;
        let w = 0.045 * r;
        let fork = 0.68;
        let stem: Vec<V3> = (0..=6).map(|i| [0.0, fork * length * i as f32 / 6.0, 0.0]).collect();
        let mut m = tube(&stem, 6, &SweepOptions::default(), |_| w);
        for side in [1.0f32, -1.0] {
            let tine: Vec<V3> = (0..=4)
                .map(|i| {
                    let s = i as f32 / 4.0;
                    [side * 0.14 * length * s * (0.6 + 0.4 * s), (fork + (1.0 - fork) * s) * length, 0.02 * length * s]
                })
                .collect();
            m.append(&tube(&tine, 5, &SweepOptions { cap_end: false, ..Default::default() }, |s| w * (1.0 - 0.8 * s)));
        }
        m
    }
}

/// How far down a snake's flanks its belly is pressed flat ([`Trunk::belly`]).
pub const BELLY: f32 = 0.45;
/// A snake's body is a little less deep than it is wide.
const DEPTH: f32 = 0.92;

/// A snake's proportions, which its rig and poses share.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnakeDims {
    pub girth: f32,
    pub head_length: f32,
    /// Neck to tail tip.
    pub body_length: f32,
    /// One spine segment's length between its joints.
    pub segment: f32,
    /// The front of the body that rears and strikes.
    pub neck: f32,
    /// The coil's innermost radius, and how far each turn lies out from the last per radian.
    pub coil_inner: f32,
    pub coil_spacing: f32,
    pub constrictor: bool,
}

/// A trunk's surface with every point passed through `clamp` (cut flat at the mouth's line).
fn clamped(trunk: &Trunk, around: usize, along: usize, clamp: impl Fn(V3) -> V3) -> MeshBuilder {
    let options = GridOptions { wrap_around: true, wrap_along: false, shading: Shading::SMOOTH, row_v: None };
    parametric_surface(around, along, &options, |u, v| clamp(trunk.point(v, -u * TAU - FRAC_PI_2, 0.0)))
}

/// The mouth's lining: a flat sheet just inside the mouth's line, facing down (`facing` 1, on
/// the upper head) or up (-1, on the jaw).
fn mouth_lining(head: &Trunk, line: f32, facing: f32) -> MeshBuilder {
    let rows = 14;
    let mut ring = Vec::new();
    // Round the outline of the head where it meets the mouth's line, a little inside.
    for i in 0..=rows {
        let t = 0.1 + 0.85 * i as f32 / rows as f32;
        ring.push([0.88 * head.half_width(t), head.centre(t)[1], line - facing * 0.004]);
    }
    for i in (0..=rows).rev() {
        let t = 0.1 + 0.85 * i as f32 / rows as f32;
        ring.push([-0.88 * head.half_width(t), head.centre(t)[1], line - facing * 0.004]);
    }
    let mut m = MeshBuilder::new();
    m.push_fan_cap(&ring, [0.0, 0.0, -facing]);
    m
}

/// A row of small backward-hooked teeth along each side of the mouth, pointing down (`dir`
/// -1, the upper head) or up (1, the jaw).
fn teeth_row(head: &Trunk, line: f32, dir: f32, r: f32) -> MeshBuilder {
    let mut m = MeshBuilder::new();
    for side in [1.0f32, -1.0] {
        for k in 0..7 {
            let t = 0.35 + 0.5 * k as f32 / 6.0;
            let mut tooth = spike(0.035 * r, 0.12 * r, 4, 0.6, Shading::FLAT);
            tooth.place([side * 0.75 * head.half_width(t), head.centre(t)[1], line], vec3::normalize_or([0.0, -0.5, dir], vec3::Z));
            m.append(&tooth);
        }
    }
    m
}

impl SnakeState {
    /// Every segment's placement at rest, for tests.
    pub fn rest(&self) -> Vec<Placement> {
        crate::pose::snake(self, &crate::pose::SnakePose::REST)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn snakes_are_deterministic() {
        for seed in [0, 7, 1207, u32::MAX] {
            assert_eq!(SnakeState::roll(seed), SnakeState::roll(seed));
            assert_eq!(SnakeState::roll(seed).build(), SnakeState::roll(seed).build());
        }
        assert_ne!(SnakeState::roll(1), SnakeState::roll(2));
    }

    #[test]
    fn every_snake_part_validates() {
        for seed in 0..16 {
            for species in [Species::PoisonousSnake, Species::ConstrictorSnake] {
                let snake = SnakeState::roll_as(seed, species);
                let rig = snake.build();
                rig.validate().unwrap_or_else(|e| panic!("{species:?} {seed}: {e}"));
                assert_eq!(rig.bones.len(), bone::COUNT);
                for name in ["head", "jaw", "tongue", "spine_0", "spine_21"] {
                    assert!(rig.index(name).is_some(), "{name}");
                }
            }
        }
    }

    #[test]
    fn snakes_have_scales_markings_and_eyes_that_read() {
        let snake = (0..64).map(|s| SnakeState::roll_as(s, Species::PoisonousSnake)).find(|s| s.pattern == SnakePattern::Diamonds).unwrap();
        let rig = snake.build();
        let spine = rig.bone("spine_4").unwrap();
        assert!(spine.mesh.parts.iter().any(|p| p.name == "spine" && p.pattern.is_some_and(|p| p.kind == PatternKind::Scales)));
        assert!(spine.mesh.parts.iter().any(|p| p.name == "markings"));
        assert!(spine.mesh.parts.iter().any(|p| p.name == "belly"));
        let head: HashSet<&str> = rig.bone("head").unwrap().mesh.parts.iter().map(|p| p.name).collect();
        for name in ["head", "eyes", "pupils", "glints", "nostrils", "fangs", "mouth"] {
            assert!(head.contains(name), "{name}");
        }
        // Markings sit over the skin's scales: the pattern wraps round the same axis.
        let wrap = |name: &str| armory::fabric::Wrap::of(&spine.mesh.parts.iter().find(|p| p.name == name).unwrap().positions);
        let (skin, marks) = (wrap("spine"), wrap("markings"));
        assert!((skin.base - marks.base).abs() < 1e-4 && (skin.length - marks.length).abs() < 1e-4);
        assert!(vec3::distance(skin.origin, marks.origin) < 0.25 * snake.girth, "{:?} {:?}", skin.origin, marks.origin);
    }

    #[test]
    fn snakes_fill_their_srd_size() {
        for seed in 0..24 {
            for species in [Species::PoisonousSnake, Species::ConstrictorSnake] {
                let snake = SnakeState::roll_as(seed, species);
                let size = snake.size();
                let (lo, hi) = snake.build().rest_mesh().bounds().unwrap();
                // Coiled at rest it fits its space; laid out, it is longer than the size below.
                let across = (hi[0] - lo[0]).max(hi[2] - lo[2]);
                assert!(across <= size.space(), "{species:?} {seed}: coiled {across} > {}", size.space());
                if let Some(smaller) = size.smaller() {
                    assert!(snake.length > smaller.space(), "{species:?} {seed}");
                }
                assert!(lo[1] > -0.05 * snake.girth && lo[1] < 0.05 * snake.girth, "{species:?} {seed}: lowest point {}", lo[1]);
            }
        }
    }

    #[test]
    fn colourings_and_patterns_vary_across_seeds() {
        let snakes: Vec<SnakeState> = (0..300).map(SnakeState::roll).collect();
        assert_eq!(snakes.iter().map(|s| s.colouring).collect::<HashSet<_>>().len(), SnakeColouring::ALL.len());
        assert_eq!(snakes.iter().map(|s| s.pattern).collect::<HashSet<_>>().len(), SnakePattern::ALL.len());
        let constrictors = snakes.iter().filter(|s| s.species == Species::ConstrictorSnake).count();
        assert!((60..=140).contains(&constrictors), "{constrictors}");
    }
}
