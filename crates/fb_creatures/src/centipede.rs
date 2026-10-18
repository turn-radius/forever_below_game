//! Giant centipedes: [`CentipedeState`] from a seed, and its [`Rig`].
//!
//! The rig is a head, a spine of [`CentipedeState::segments`] plates laid out the way the
//! snake's spine is ([`crate::pose::centipede`] lays them along a shallow wave, each plate a
//! rounded length of body that overlaps its neighbours so the body stays whole as it bends),
//! and a pair of legs hung from every plate. The head carries a pair of antennae, a cluster of
//! simple eyes either side and the forcipules, the hooked, venom-bearing first legs that fold
//! under the chin and open to bite. Every bone is rigid, so the legs can step in a wave down the
//! body ([`crate::pose::centipede`]) without a skeleton.
//!
//! The plates are bare, glossy chitin in a colouring (rust, olive, night, amber or slate) with a
//! head and legs of their own colour; the banding lays a dark rim along the back edge of each
//! plate, or tints every other plate, or runs one stripe down the back. The last pair of legs
//! trails behind, long and held off the floor, as a Scolopendra's do; every other leg is a
//! walking leg, bent at a raised knee and ending in a claw that stands on the floor through its
//! stance.
//!
//! Lengths, girths, colourings and banding are our own proposal, after the Scolopendra
//! centipedes (the giant redheaded and the Amazonian giant). A giant centipede is drawn bigger
//! than the SRD's picture of it, a Small creature of about a metre, so it reads as a threat
//! across a room; only the drawing departs, and hit points and bite stay the stat block's.

use armory::geom::vec3::{self, V3};
use armory::geom::{MeshBuilder, Shading, SweepOptions, spike, tube, uv_sphere};
use armory::{MaterialKind, Part, Rgb};
use fb_body::Placement;
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, PI};

use crate::pose::{CentipedePose, centipede as pose_centipede};
use crate::rat::{mix, part};
use crate::rig::Rig;
use crate::shape::{Trunk, eye, keyed};
use crate::{Size, Species};

/// Fewest and most segments a centipede has (the pairs of legs it walks on, the last pair
/// trailing). Our own: a Scolopendra has 21 or 23 pairs, drawn fewer here so each plate is read.
pub const SEGMENTS: (usize, usize) = (15, 21);

/// Bone indices in a centipede's [`Rig`]. A pair is the creature's right then its left.
pub mod bone {
    pub const HEAD: usize = 0;
    /// The forcipules, right then left.
    pub const FORCIPULE: usize = 1;
    /// The antennae, right then left.
    pub const ANTENNA: usize = 3;
    /// The first body plate, just behind the head; plate `k` is `SEGMENT + k`.
    pub const SEGMENT: usize = 5;
}

/// A centipede's colours: back, rim, head, legs and eyes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CentipedeColouring {
    /// Rust red plates, a brighter head, yellow legs.
    Rust,
    /// Olive plates, an orange head, yellow legs.
    Olive,
    /// Near black plates, a red head, orange legs.
    Night,
    /// Amber plates, a gold head and pale legs.
    Amber,
    /// Slate blue plates, a rust head, tan legs.
    Slate,
}

impl CentipedeColouring {
    pub const ALL: [CentipedeColouring; 5] = [CentipedeColouring::Rust, CentipedeColouring::Olive, CentipedeColouring::Night, CentipedeColouring::Amber, CentipedeColouring::Slate];

    /// Back, rim, head, legs and the eyes' iris. Our own picks.
    pub fn colours(self) -> [Rgb; 5] {
        match self {
            CentipedeColouring::Rust => [Rgb::hex(0x8a3a22), Rgb::hex(0x3e1a10), Rgb::hex(0xb8541f), Rgb::hex(0xd9a441), Rgb::hex(0xe8b030)],
            CentipedeColouring::Olive => [Rgb::hex(0x5e5c2c), Rgb::hex(0x2a2a12), Rgb::hex(0xc27a2c), Rgb::hex(0xd8b24a), Rgb::hex(0xd8c040)],
            CentipedeColouring::Night => [Rgb::hex(0x2c2732), Rgb::hex(0x120e18), Rgb::hex(0x8a2a20), Rgb::hex(0xc7792c), Rgb::hex(0xe05a30)],
            CentipedeColouring::Amber => [Rgb::hex(0xc27a2a), Rgb::hex(0x6a3412), Rgb::hex(0xe0a030), Rgb::hex(0xf0d27a), Rgb::hex(0x7ac040)],
            CentipedeColouring::Slate => [Rgb::hex(0x3e4c5e), Rgb::hex(0x182230), Rgb::hex(0xa0502c), Rgb::hex(0xd0aa6a), Rgb::hex(0xe8d040)],
        }
    }

    pub fn back(self) -> Rgb {
        self.colours()[0]
    }
    pub fn rim(self) -> Rgb {
        self.colours()[1]
    }
    pub fn head(self) -> Rgb {
        self.colours()[2]
    }
    pub fn legs(self) -> Rgb {
        self.colours()[3]
    }
    pub fn eyes(self) -> Rgb {
        self.colours()[4]
    }
}

/// How the plates are banded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CentipedeBanding {
    Plain,
    /// A dark rim along the back edge of every plate.
    Rimmed,
    /// Every other plate tinted toward the rim colour, each with its rim.
    Alternate,
    /// One dark stripe down the back, over rimmed plates.
    Striped,
}

impl CentipedeBanding {
    pub const ALL: [CentipedeBanding; 4] = [CentipedeBanding::Plain, CentipedeBanding::Rimmed, CentipedeBanding::Alternate, CentipedeBanding::Striped];
}

const FANG: Rgb = Rgb::hex(0x1a1210);
const PUPIL: Rgb = Rgb::hex(0x0a0807);
const GLINT: Rgb = Rgb::hex(0xf4f1ea);
const SALT: u32 = 0xCE47_1BED;

/// One giant centipede, every choice the seed makes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CentipedeState {
    pub species: Species,
    pub seed: u32,
    /// Plates in the spine, [`SEGMENTS`].
    pub segments: usize,
    /// Nose to the last plate's end, in world units.
    pub length: f32,
    /// The body's greatest half width.
    pub girth: f32,
    pub colouring: CentipedeColouring,
    pub banding: CentipedeBanding,
    /// How long the antennae are, in girths.
    pub antennae: f32,
    /// How the antennae curl: 1 or -1 turns their tips the same way on both, 0 holds them
    /// straight out; between is a lean.
    pub curl: f32,
}

impl CentipedeState {
    /// A giant centipede from `seed` ([`crate::centipede`]).
    pub fn roll(seed: u32) -> CentipedeState {
        let mut rng = Mulberry32::new(seed ^ SALT);
        // About a metre, near the SRD's Small and bigger than a real one's picture; the segment
        // count follows the length so every plate keeps to about a girth long.
        let length = rng.range_f32(9.5, 13.0);
        let girth = rng.range_f32(0.46, 0.64);
        let per_girth = rng.range_f32(0.85, 1.1);
        let body = length - 1.45 * girth;
        let segments = ((body / (per_girth * girth)).round() as usize).clamp(SEGMENTS.0, SEGMENTS.1);
        let colouring = *rng.weighted(&[(CentipedeColouring::Rust, 4), (CentipedeColouring::Olive, 3), (CentipedeColouring::Night, 2), (CentipedeColouring::Amber, 2), (CentipedeColouring::Slate, 2)]);
        let banding = *rng.weighted(&[(CentipedeBanding::Rimmed, 4), (CentipedeBanding::Plain, 2), (CentipedeBanding::Alternate, 2), (CentipedeBanding::Striped, 2)]);
        CentipedeState { species: Species::GiantCentipede, seed, segments, length, girth, colouring, banding, antennae: rng.range_f32(3.4, 5.0), curl: rng.range_f32(-1.0, 1.0) }
    }

    /// Its SRD size category, which its hit points and bite go with.
    pub fn size(&self) -> Size {
        self.species.stat_block().size
    }

    /// The size its body is drawn at: its SRD size, Small. A metre long, its body fits the
    /// space; the antennae and the trailing legs reach beyond it.
    pub fn drawn_size(&self) -> Size {
        Size::Small
    }

    pub fn dims(&self) -> CentipedeDims {
        let head_length = 1.45 * self.girth;
        let body_length = self.length - head_length;
        let segment = body_length / self.segments as f32;
        CentipedeDims { girth: self.girth, head_length, body_length, segment, overlap: 0.22 * segment, clearance: CLEARANCE * self.girth }
    }

    /// The body's half width `s` along it (0 the neck, 1 the last plate's end).
    pub fn radius_at(&self, s: f32) -> f32 {
        self.girth * keyed(&[(0.0, 0.88), (0.1, 1.0), (0.5, 1.0), (0.85, 0.8), (1.0, 0.55)], s)
    }

    /// Plate `k`'s half width at its middle.
    pub fn plate_radius(&self, k: usize) -> f32 {
        self.radius_at((k as f32 + 0.5) / self.segments as f32)
    }

    /// How far below a plate's centre line its belly lies, for a half width `r`: the trunk's
    /// pressed belly ([`Trunk::belly`] of [`BELLY`]).
    pub fn belly_depth(r: f32) -> f32 {
        DEPTH * r * (BELLY + (1.0 - BELLY) / 3.0)
    }

    /// How high plate `k`'s centre line stands off the floor: its belly's clearance and depth.
    pub fn centre_height(&self, k: usize) -> f32 {
        self.dims().clearance + CentipedeState::belly_depth(self.plate_radius(k))
    }

    /// Where plate `k`'s `side` leg (1 right, -1 left) hangs from it, in the plate's frame:
    /// low on its flank, just inside the surface.
    pub fn hip(&self, k: usize, side: f32) -> V3 {
        let r = self.plate_radius(k);
        [side * 0.85 * r, 0.0, -0.3 * DEPTH * r]
    }

    /// How far a walking leg's claw hangs below its hip, so that it stands on the floor with
    /// the plate level.
    pub fn leg_drop(&self, k: usize) -> f32 {
        self.centre_height(k) + self.hip(k, 1.0)[2]
    }

    /// How far out from its hip a walking leg's claw lies, in the plate's frame.
    pub fn leg_reach(&self, k: usize) -> f32 {
        LEG_REACH * self.plate_radius(k)
    }

    /// Whether plate `k` is the last, whose legs trail behind.
    pub fn is_last(&self, k: usize) -> bool {
        k + 1 == self.segments
    }

    /// How many bones the rig has: the head, forcipules, antennae, plates and a leg either side
    /// of every plate.
    pub fn bone_count(&self) -> usize {
        bone::SEGMENT + 3 * self.segments
    }

    /// The bone of plate `k`.
    pub fn segment_bone(&self, k: usize) -> usize {
        bone::SEGMENT + k
    }

    /// The bone of plate `k`'s leg on `side` (1 right, -1 left).
    pub fn leg_bone(&self, k: usize, side: f32) -> usize {
        bone::SEGMENT + self.segments + 2 * k + usize::from(side < 0.0)
    }

    /// The circle it covers on the floor at rest, about its centre: its plates' reach, the
    /// head's and the legs'.
    pub fn radius(&self) -> f32 {
        let rest = self.rest();
        let reach = |p: &Placement| p.origin[0].hypot(p.origin[2]);
        let body = (0..self.segments).map(|k| reach(&rest[self.segment_bone(k)]) + self.leg_reach(k)).fold(0.0f32, f32::max);
        body.max(reach(&rest[bone::HEAD]) + self.dims().head_length)
    }

    /// Plate `k`'s trunk in its own spine frame, centred on its middle: a squared-off length of
    /// body, rounded at both ends past its joints so neighbours overlap.
    pub fn segment_trunk(&self, k: usize) -> Trunk {
        let d = self.dims();
        let l = d.segment;
        let half = 0.5 * l + d.overlap;
        let s_of = |y: f32| ((k as f32 + 0.5) * l - y) / d.body_length;
        let samples = 16;
        let (mut width, mut height) = (Vec::with_capacity(samples + 1), Vec::with_capacity(samples + 1));
        for i in 0..=samples {
            let t = i as f32 / samples as f32;
            let u = 2.0 * t - 1.0;
            let r = self.radius_at(s_of(u * half).clamp(0.0, 1.0));
            // Squared plan with rounded corners; a low dome in the section.
            width.push((t, r * (1.0 - u.abs().powi(6)).max(0.0).powf(0.25)));
            height.push((t, DEPTH * r * (1.0 - u.abs().powi(4)).max(0.0).sqrt()));
        }
        Trunk { start: -half, length: 2.0 * half, width, height, lift: vec![(0.0, 0.0)], squareness: 2.4, belly: BELLY }
    }

    /// The head's trunk in the head bone's frame (the neck at its origin, snout along +Y).
    pub fn head_trunk(&self) -> Trunk {
        let g = self.girth;
        let hl = self.dims().head_length;
        Trunk {
            start: -0.3 * hl,
            length: 1.3 * hl,
            width: vec![(0.0, 0.0), (0.05, 0.55 * g), (0.2, 0.9 * g), (0.5, 0.98 * g), (0.82, 0.9 * g), (0.95, 0.6 * g), (1.0, 0.0)],
            height: vec![(0.0, 0.0), (0.05, 0.32 * g), (0.25, 0.5 * g), (0.6, 0.52 * g), (0.9, 0.4 * g), (1.0, 0.0)],
            lift: vec![(0.0, 0.0)],
            squareness: 2.6,
            belly: 0.5,
        }
    }

    /// How far the head reaches below its bone, level: its chin on the floor puts the bone this
    /// high.
    pub fn head_depth(&self) -> f32 {
        let head = self.head_trunk();
        (0..=24)
            .flat_map(|i| (0..=16).map(move |j| (i as f32 / 24.0, j as f32 / 16.0)))
            .map(|(t, a)| -head.point(t, PI + a * PI, 0.0)[2])
            .fold(0.0, f32::max)
    }

    /// Where the forcipule on `side` (1 right, -1 left) hinges, in the head's frame: under the
    /// front of the head, to the side.
    pub fn forcipule_hinge(&self, side: f32) -> V3 {
        let head = self.head_trunk();
        let t = 0.62;
        [side * 0.34 * self.girth, head.start + t * head.length, -0.7 * head.half_height(t)]
    }

    /// Where the antenna on `side` roots, in the head's frame: on the front corner, on top.
    pub fn antenna_root(&self, side: f32) -> V3 {
        let head = self.head_trunk();
        let t = 0.84;
        [side * 0.5 * head.half_width(t), head.start + t * head.length, 0.25 * head.half_height(t)]
    }

    /// Its bones and meshes, laid out at rest.
    pub fn build(&self) -> Rig {
        let c = self.colouring;
        let rest = pose_centipede(self, &CentipedePose::REST);
        let mut rig = Rig::default();

        // Head, with its eyes and the plate's brow.
        let head = self.head_trunk();
        let mut head_parts = vec![part(head.mesh(22, 20), "head", MaterialKind::Skin, c.head(), true)];
        head_parts.extend(self.head_details(&head));
        let h = rig.add("head", None, rest[bone::HEAD], head_parts);

        // Forcipules and antennae hang from the head.
        for (i, (name, side)) in [("right", 1.0f32), ("left", -1.0)].into_iter().enumerate() {
            let forcipule = self.forcipule(side);
            rig.add(format!("forcipule_{name}"), Some(h), rest[bone::FORCIPULE + i], vec![part(forcipule.0, "jaw", MaterialKind::Skin, mix(c.head(), c.rim(), 0.45), true), part(forcipule.1, "fang", MaterialKind::Bone, FANG, false)]);
        }
        for (i, (name, side)) in [("right", 1.0f32), ("left", -1.0)].into_iter().enumerate() {
            rig.add(format!("antenna_{name}"), Some(h), rest[bone::ANTENNA + i], vec![part(self.antenna(side), "antenna", MaterialKind::Skin, mix(c.legs(), c.head(), 0.4), false)]);
        }

        // Plates.
        let mut plates = Vec::with_capacity(self.segments);
        for k in 0..self.segments {
            plates.push(rig.add(format!("segment_{k}"), None, rest[self.segment_bone(k)], self.plate_parts(k)));
        }
        // Legs, a pair to each plate.
        for (k, &plate) in plates.iter().enumerate() {
            for (name, side) in [("right", 1.0f32), ("left", -1.0)] {
                let i = rig.add(format!("leg_{k}_{name}"), Some(plate), rest[self.leg_bone(k, side)], vec![part(self.leg(k, side), "leg", MaterialKind::Skin, c.legs(), true)]);
                debug_assert_eq!(i, self.leg_bone(k, side));
            }
        }
        debug_assert_eq!(rig.bones.len(), self.bone_count());
        rig
    }

    /// The parts of plate `k`: its back, belly and banding.
    fn plate_parts(&self, k: usize) -> Vec<Part> {
        let d = self.dims();
        let c = self.colouring;
        let trunk = self.segment_trunk(k);
        let l = d.segment;
        let alternate = self.banding == CentipedeBanding::Alternate && k % 2 == 1;
        let back = if alternate { mix(c.back(), c.rim(), 0.5) } else { c.back() };
        let mut parts = vec![part(trunk.mesh(18, 18), "spine", MaterialKind::Skin, back, true)];
        let t_of = |y: f32| (y + trunk.start.abs()) / trunk.length;
        let off = 0.012 * self.girth;
        // The belly's pale scutes.
        let scutes = armory::fabric::SurfacePattern { kind: armory::fabric::PatternKind::Hoops, color: c.legs().scaled(0.8), scale: 1.0 / (0.3 * self.girth) };
        if let Some(m) = trunk.patch(t_of(-0.5 * l), t_of(0.5 * l), -FRAC_PI_2, |_| 0.8, 0.004 * self.girth, false) {
            parts.push(part(m, "belly", MaterialKind::Skin, mix(c.legs(), c.back(), 0.45), false).with_pattern(Some(scutes)));
        }
        let mut marks = MeshBuilder::new();
        if self.banding != CentipedeBanding::Plain {
            // A rim along the back edge, all the way round.
            if let Some(m) = trunk.patch(t_of(-0.5 * l - 0.04 * l), t_of(-0.5 * l + 0.3 * l), FRAC_PI_2, |t| if t > 0.0 { PI } else { 0.0 }, off, false) {
                marks.append(&m);
            }
        }
        if self.banding == CentipedeBanding::Striped {
            if let Some(m) = trunk.patch(t_of(-0.4 * l), t_of(0.4 * l), FRAC_PI_2, |_| 0.2, 1.4 * off, false) {
                marks.append(&m);
            }
        }
        if !marks.is_empty() {
            parts.push(part(marks, "markings", MaterialKind::Skin, c.rim(), false));
        }
        parts
    }

    fn head_details(&self, head: &Trunk) -> Vec<Part> {
        let c = self.colouring;
        let g = self.girth;
        let mut parts = Vec::new();
        // A cluster of simple eyes either side, up the flank toward the front: a larger one and
        // two smaller, each a bright iris with a dark pupil and a glint.
        let (mut balls, mut pupils, mut glints) = (MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new());
        for (side, a0) in [(1.0f32, 0.55f32), (-1.0, PI - 0.55)] {
            let mirror = side;
            for (t, da, r) in [(0.7, 0.0, 0.15), (0.6, -0.34, 0.1), (0.79, 0.38, 0.1)] {
                let a = a0 - mirror * da;
                let n = head.normal(t, a);
                let er = r * g;
                let at = head.point(t, a, -0.25 * er);
                let (ball, glint) = eye(at, er, n, [0.0, 0.5, 0.8]);
                balls.append(&ball);
                glints.append(&glint);
                let mut pupil = uv_sphere(1.0, 10, 8);
                pupil.scale([0.55 * er, 0.55 * er, 0.4 * er]);
                pupil.place_basis(vec3::add(at, vec3::scale(n, 0.82 * er)), n, vec3::Y);
                // The pupil's disc faces along `n` (its short axis), set on the iris.
                pupils.append(&pupil);
            }
        }
        parts.push(part(balls, "eyes", MaterialKind::Gem, c.eyes(), true));
        parts.push(part(pupils, "pupils", MaterialKind::Gem, PUPIL, false));
        parts.push(part(glints, "glints", MaterialKind::Gem, GLINT, false).with_glow(0.5));
        // A pair of palps by the mouth.
        let mut palps = MeshBuilder::new();
        for side in [1.0f32, -1.0] {
            let mut palp = spike(0.09 * g, 0.5 * g, 6, 0.4, Shading::SMOOTH);
            palp.place([side * 0.16 * g, head.start + 0.88 * head.length, -0.3 * g], vec3::normalize_or([side * 0.15, 1.0, -0.5], vec3::Y));
            palps.append(&palp);
        }
        parts.push(part(palps, "palps", MaterialKind::Skin, mix(c.head(), c.rim(), 0.4), false));
        parts
    }

    /// The centre line of the forcipule on `side`, from its hinge along +Y, in its own frame: a
    /// stout fang curling in toward the middle, and the point its dark tip ends at.
    pub fn forcipule_path(&self, side: f32) -> Vec<V3> {
        let g = self.girth;
        let pts = [[0.0, 0.0, 0.0], [0.0, 0.32, -0.04], [-0.03, 0.62, -0.1], [-0.08, 0.84, -0.17], [-0.16, 0.98, -0.2]];
        (0..=12).map(|i| curve(&pts, i as f32 / 12.0)).map(|p| [side * p[0] * g, p[1] * g, p[2] * g]).collect()
    }

    /// One forcipule on `side`: the fang, and its dark tip.
    fn forcipule(&self, side: f32) -> (MeshBuilder, MeshBuilder) {
        let g = self.girth;
        let path = self.forcipule_path(side);
        let body = tube(&path, 8, &SweepOptions { cap_end: false, ..Default::default() }, |s| g * (0.17 - 0.1 * s * s));
        // The tip: a dark spike carrying on from the end of the curve.
        let end = path[path.len() - 1];
        let before = path[path.len() - 3];
        let dir = vec3::normalize_or(vec3::sub(end, before), vec3::Y);
        let mut tip = spike(g * 0.075, 0.34 * g, 7, 0.9, Shading::SMOOTH);
        tip.place(end, dir);
        (body, tip)
    }

    /// One antenna on `side`, from its root along +Y: a long tapering, beaded whip that arches
    /// up and curls at the end.
    fn antenna(&self, side: f32) -> MeshBuilder {
        let g = self.girth;
        let len = self.antennae * g;
        let curl = self.curl;
        let path: Vec<V3> = (0..=24)
            .map(|i| {
                let s = i as f32 / 24.0;
                let lean = side * 0.1 * len * s * s + curl * 0.16 * len * s.powi(3);
                [lean, len * s * (1.0 - 0.14 * s), 0.2 * len * (PI * 0.85 * s).sin() * (1.0 - 0.3 * s)]
            })
            .collect();
        tube(&path, 6, &SweepOptions::default(), |s| g * 0.075 * (1.0 - 0.8 * s) * (1.0 + 0.16 * (s * 30.0).cos()))
    }

    /// One leg of plate `k` on `side`, from its hip along ±X: a walking leg, bent up at the knee
    /// and ending in a claw at the floor, or the last plate's long trailing leg.
    fn leg(&self, k: usize, side: f32) -> MeshBuilder {
        let r = self.plate_radius(k);
        if self.is_last(k) {
            return self.trailing_leg(k, side);
        }
        let drop = self.leg_drop(k);
        let reach = self.leg_reach(k);
        let claw = 0.14 * r;
        // (out, up) points from the hip: up over a high knee, then down and out.
        let pts = [[0.0, 0.0], [0.4 * reach, 0.14 * r], [0.62 * reach, 0.36 * r], [0.85 * reach, 0.1 * r], [0.96 * reach, -0.5 * drop], [reach, -drop + claw]];
        let path: Vec<V3> = (0..=14).map(|i| curve2(&pts, i as f32 / 14.0)).map(|(o, z)| [side * o, 0.0, z]).collect();
        let mut leg = tube(&path, 7, &SweepOptions { cap_end: false, ..Default::default() }, |s| r * (0.13 - 0.085 * s) * (1.0 + 0.4 * (-((s - 0.42) / 0.09).powi(2)).exp()));
        // The claw: a spike carrying on down to stand on the floor exactly.
        let dir = vec3::normalize_or([side * 0.12, 0.0, -1.0], vec3::Z);
        let mut tip = spike(r * 0.05, claw / -dir[2], 6, 0.8, Shading::SMOOTH);
        tip.place(path[path.len() - 1], dir);
        leg.append(&tip);
        leg
    }

    /// The last plate's leg: long, going out and trailing back behind the body, held off the
    /// floor.
    fn trailing_leg(&self, k: usize, side: f32) -> MeshBuilder {
        let r = self.plate_radius(k);
        let length = 5.2 * r;
        let hang = self.leg_drop(k);
        let pts = [[0.0, 0.0, 0.0], [0.5, -0.1, 0.12], [1.0, -0.5, 0.1], [1.25, -1.5, -0.1], [1.2, -2.6, -0.22], [1.0, -3.0, -0.3]];
        let path: Vec<V3> = (0..=18)
            .map(|i| curve(&pts, i as f32 / 18.0))
            .map(|p| {
                let z = (p[2] * length * 0.3).max(-0.55 * hang);
                [side * p[0] * length * 0.3, p[1] * length * 0.3, z]
            })
            .collect();
        tube(&path, 7, &SweepOptions::default(), |s| r * (0.14 - 0.1 * s) * (1.0 + 0.25 * (s * 22.0).cos()))
    }
}

/// A smooth curve through `points` at `t` in 0..=1 (Catmull-Rom through [`keyed`]).
fn curve(points: &[V3], t: f32) -> V3 {
    let n = points.len() - 1;
    [0, 1, 2].map(|axis| {
        let keys: Vec<(f32, f32)> = points.iter().enumerate().map(|(i, p)| (i as f32 / n as f32, p[axis])).collect();
        keyed(&keys, t)
    })
}

/// [`curve`] through 2D points.
fn curve2(points: &[[f32; 2]], t: f32) -> (f32, f32) {
    let n = points.len() - 1;
    let along = |axis: usize| {
        let keys: Vec<(f32, f32)> = points.iter().enumerate().map(|(i, p)| (i as f32 / n as f32, p[axis])).collect();
        keyed(&keys, t)
    };
    (along(0), along(1))
}

/// How far down a centipede's flanks its belly is pressed flat ([`Trunk::belly`]).
pub const BELLY: f32 = 0.4;
/// A centipede's plates are about half as deep as they are wide.
const DEPTH: f32 = 0.55;
/// How far the belly clears the floor, as a share of the girth.
const CLEARANCE: f32 = 0.34;
/// How far a walking leg's claw lies out from its hip, as a share of the plate's half width.
const LEG_REACH: f32 = 2.0;

/// A centipede's proportions, which its rig and poses share.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CentipedeDims {
    pub girth: f32,
    pub head_length: f32,
    /// Neck to the last plate's end.
    pub body_length: f32,
    /// One plate's length between its joints.
    pub segment: f32,
    /// How far a plate reaches past its joints to overlap the next.
    pub overlap: f32,
    /// How high the belly stands off the floor.
    pub clearance: f32,
}

impl CentipedeState {
    /// Every plate's placement at rest, for tests.
    pub fn rest(&self) -> Vec<Placement> {
        pose_centipede(self, &CentipedePose::REST)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn centipedes_are_deterministic() {
        for seed in [0, 7, 1207, u32::MAX] {
            assert_eq!(CentipedeState::roll(seed), CentipedeState::roll(seed));
            assert_eq!(CentipedeState::roll(seed).build(), CentipedeState::roll(seed).build());
        }
        assert_ne!(CentipedeState::roll(1), CentipedeState::roll(2));
    }

    #[test]
    fn every_centipede_part_validates() {
        for seed in 0..24 {
            let state = CentipedeState::roll(seed);
            let rig = state.build();
            rig.validate().unwrap_or_else(|e| panic!("{seed}: {e}"));
            assert_eq!(rig.bones.len(), state.bone_count());
            assert!((SEGMENTS.0..=SEGMENTS.1).contains(&state.segments), "{seed}: {}", state.segments);
            for name in ["head", "forcipule_right", "forcipule_left", "antenna_right", "antenna_left", "segment_0", "leg_0_right", "leg_0_left"] {
                assert!(rig.index(name).is_some(), "{name}");
            }
            let last = state.segments - 1;
            assert_eq!(rig.index(&format!("leg_{last}_left")), Some(state.leg_bone(last, -1.0)));
            assert_eq!(rig.index(&format!("segment_{last}")), Some(state.segment_bone(last)));
        }
    }

    #[test]
    fn a_centipede_has_eyes_antennae_and_a_leg_on_every_plate() {
        let state = CentipedeState::roll(3);
        let rig = state.build();
        let head: HashSet<&str> = rig.bone("head").unwrap().mesh.parts.iter().map(|p| p.name).collect();
        for name in ["head", "eyes", "pupils", "glints"] {
            assert!(head.contains(name), "{name}");
        }
        // Six eyes in all: three to a side.
        let eyes = rig.bone("head").unwrap().mesh.parts.iter().find(|p| p.name == "eyes").unwrap();
        assert!(eyes.positions.len() > 6 * 20);
        for k in 0..state.segments {
            for side in [1.0, -1.0] {
                let bone = &rig.bones[state.leg_bone(k, side)];
                assert_eq!(bone.parent, Some(state.segment_bone(k)));
                assert!(bone.mesh.parts.iter().any(|p| p.name == "leg"));
            }
        }
        // A leg reaches out to the side it hangs on.
        let x = |bone: usize, sign: f32| rig.bones[bone].mesh.parts[0].positions.iter().map(|p| sign * p[0]).fold(f32::MIN, f32::max);
        assert!(x(state.leg_bone(2, 1.0), 1.0) > 0.9 * state.leg_reach(2));
        assert!(x(state.leg_bone(2, -1.0), -1.0) > 0.9 * state.leg_reach(2));
    }

    #[test]
    fn banding_marks_the_plates() {
        let banded = (0..64).map(CentipedeState::roll).find(|s| s.banding == CentipedeBanding::Striped).unwrap();
        let plate = banded.build();
        let parts: HashSet<&str> = plate.bone("segment_4").unwrap().mesh.parts.iter().map(|p| p.name).collect();
        for name in ["spine", "belly", "markings"] {
            assert!(parts.contains(name), "{name}");
        }
        let plain = (0..64).map(CentipedeState::roll).find(|s| s.banding == CentipedeBanding::Plain).unwrap();
        assert!(!plain.build().bone("segment_4").unwrap().mesh.parts.iter().any(|p| p.name == "markings"));
    }

    #[test]
    fn a_centipede_at_rest_stands_on_its_claws() {
        for seed in 0..12 {
            let state = CentipedeState::roll(seed);
            let (lo, hi) = state.build().rest_mesh().bounds().unwrap();
            assert!(lo[1].abs() < 0.04 * state.girth, "{seed}: lowest point {}", lo[1]);
            // About a metre long, a hand's breadth or two wide, low to the floor.
            assert!(hi[1] < 2.4 * state.girth, "{seed}: {}", hi[1]);
            assert!((9.5..=13.0).contains(&state.length));
            assert!(state.radius() < 0.4 * crate::UNITS_PER_FOOT * 5.0 * 1.2, "{seed}: {}", state.radius());
        }
    }

    #[test]
    fn colourings_and_banding_vary_across_seeds() {
        let all: Vec<CentipedeState> = (0..300).map(CentipedeState::roll).collect();
        assert_eq!(all.iter().map(|s| s.colouring).collect::<HashSet<_>>().len(), CentipedeColouring::ALL.len());
        assert_eq!(all.iter().map(|s| s.banding).collect::<HashSet<_>>().len(), CentipedeBanding::ALL.len());
        assert!(all.iter().map(|s| s.segments).collect::<HashSet<_>>().len() >= 4);
    }

    #[test]
    fn the_giant_centipede_is_the_srd_stat_block() {
        let block = Species::GiantCentipede.stat_block();
        assert_eq!(block.name, "Giant Centipede");
        assert_eq!((block.size, block.hit_points, block.hit_dice, block.armour_class, block.attack_bonus), (Size::Small, 4, crate::Dice::new(1, 6, 1), 13, 4));
        assert_eq!((block.bite, block.venom, block.speed_feet, block.senses_feet), (crate::Dice::new(1, 4, 2), Some(crate::Dice::new(3, 6, 0)), 30.0, 30.0));
        assert_eq!((block.abilities.strength, block.abilities.dexterity, block.abilities.constitution), (5, 14, 12));
        assert_eq!((block.abilities.intelligence, block.abilities.wisdom, block.abilities.charisma), (1, 7, 3));
        assert_eq!((block.challenge, block.xp, block.page), ("1/4", 50, 374));
        assert!(Species::GiantCentipede.is_centipede() && !Species::GiantCentipede.is_snake());
        assert!((Species::GiantCentipede.speed() - 15.24).abs() < 0.01);
        assert_eq!(Creature::of(Species::GiantCentipede, 5).species(), Species::GiantCentipede);
    }

    use crate::Creature;
}
