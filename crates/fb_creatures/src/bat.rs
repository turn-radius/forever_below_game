//! Bats and giant bats: [`BatState`] from a seed, and its [`Rig`].
//!
//! The rig is a furred body with a short tail membrane, a head with a separate jaw, two hind
//! legs that grip, and two wings of three bones each: an arm (shoulder to elbow), a forearm
//! (elbow to wrist) and a hand (the fan of fingers from the wrist). Each wing bone carries a
//! tube for its bone and the sheet of membrane stretched beside it; the sheets meet along the
//! lines the next bone hinges on, so the wing folds like a book where the elbow and the wrist
//! bend (the elbow hinges on the line from the elbow to the membrane's trailing edge, the wrist
//! on the fifth finger) and stays whole, no gap and no skinning, through a wingbeat and
//! folded round a roosting body. Membranes are thin, double-sided sheets bellied a little by
//! the air and scalloped between the fingertips; the finger bones, claw on the thumb, joint
//! knuckles and fur-covered arm are drawn over them.
//!
//! The head is short and broad, with large cupped ears (pointed, rounded or tall), a nose that
//! is plain, a leaf or a horseshoe, small black eyes with a glint, and fangs; the jaw drops for a
//! bite and shows its lining. Fur tufts stand out of the body and head in the coat's colour,
//! paler on the belly.
//!
//! Sizes, proportions, colourings and shapes are our own proposal, after the flying foxes and
//! mouse-eared bats for the bat's silhouette and the leaf-nosed and horseshoe bats for noses
//! (Bat Conservation International, <https://www.batcon.org/about-bats/bat-profiles/>). Both are
//! drawn bigger than their SRD size, so they read across a room ([`BatState::drawn_size`]): a
//! bat with a wingspan of 70 to 110 cm, a giant bat of 2 to 3 m. Hit points and bite stay the
//! stat block's.

use armory::geom::vec3::{self, V3};
use armory::geom::{CatmullRom, GridOptions, MeshBuilder, Shading, SweepOptions, parametric_surface, spike, thick_sheet, tube, uv_sphere};
use armory::{MaterialKind, Part, Rgb};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::rat::{mix, part};
use crate::rig::Rig;
use crate::shape::{Trunk, eye};
use crate::{Size, Species};

/// Bone indices in a bat's [`Rig`].
pub mod bone {
    pub const BODY: usize = 0;
    pub const HEAD: usize = 1;
    pub const JAW: usize = 2;
    /// The right wing's arm; its forearm and hand follow, then the left wing's three.
    pub const WINGS: usize = 3;
    /// The right hind leg, then the left.
    pub const LEGS: usize = WINGS + 6;
    pub const COUNT: usize = LEGS + 2;

    /// The first bone of wing `side` (0 right, 1 left).
    pub const fn wing(side: usize) -> usize {
        WINGS + 3 * side
    }
}

/// A bat's coat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BatColouring {
    Brown,
    /// Near black.
    Dark,
    /// Rusty orange.
    Russet,
    Grey,
    /// Sandy, nearly cream.
    Pale,
}

impl BatColouring {
    pub const ALL: [BatColouring; 5] = [BatColouring::Brown, BatColouring::Dark, BatColouring::Russet, BatColouring::Grey, BatColouring::Pale];

    /// The coat's colour. Our own picks.
    pub fn fur(self) -> Rgb {
        match self {
            BatColouring::Brown => Rgb::hex(0x5b4636),
            BatColouring::Dark => Rgb::hex(0x2b2523),
            BatColouring::Russet => Rgb::hex(0x8e5230),
            BatColouring::Grey => Rgb::hex(0x6c6b6e),
            BatColouring::Pale => Rgb::hex(0xc4b59c),
        }
    }

    pub fn belly(self) -> Rgb {
        match self {
            BatColouring::Brown => Rgb::hex(0x8d7761),
            BatColouring::Dark => Rgb::hex(0x453c38),
            BatColouring::Russet => Rgb::hex(0xbd8f62),
            BatColouring::Grey => Rgb::hex(0xa9a59f),
            BatColouring::Pale => Rgb::hex(0xe8dec9),
        }
    }

    /// Bare skin: ears, nose and feet.
    pub fn skin(self) -> Rgb {
        match self {
            BatColouring::Pale | BatColouring::Russet => Rgb::hex(0xb88a80),
            _ => Rgb::hex(0x6e5650),
        }
    }

    pub fn eyes(self) -> Rgb {
        match self {
            BatColouring::Russet | BatColouring::Pale => Rgb::hex(0x3a1410),
            _ => Rgb::hex(0x0d0b0b),
        }
    }
}

/// The tint of a bat's wing membrane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BatMembrane {
    /// Dusky violet-brown.
    Dusky,
    /// Grey-black.
    Smoky,
    /// Leathery tan-brown.
    Leathery,
    /// Pinkish, thin enough to show blood.
    Rosy,
}

impl BatMembrane {
    pub const ALL: [BatMembrane; 4] = [BatMembrane::Dusky, BatMembrane::Smoky, BatMembrane::Leathery, BatMembrane::Rosy];

    pub fn tint(self) -> Rgb {
        match self {
            BatMembrane::Dusky => Rgb::hex(0x5a444c),
            BatMembrane::Smoky => Rgb::hex(0x44444c),
            BatMembrane::Leathery => Rgb::hex(0x7b5a43),
            BatMembrane::Rosy => Rgb::hex(0x94626a),
        }
    }
}

/// The shape of a bat's ears.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BatEars {
    /// Broad at the base, coming to a point.
    Pointed,
    /// Broad and rounded at the top.
    Rounded,
    /// Narrow and very long.
    Tall,
}

impl BatEars {
    pub const ALL: [BatEars; 3] = [BatEars::Pointed, BatEars::Rounded, BatEars::Tall];
}

/// The shape of a bat's nose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BatNose {
    /// A bare pad with nostrils.
    Plain,
    /// A spear-shaped leaf standing up over the nostrils.
    Leaf,
    /// A horseshoe-shaped ridge round the nostrils.
    Horseshoe,
}

impl BatNose {
    pub const ALL: [BatNose; 3] = [BatNose::Plain, BatNose::Leaf, BatNose::Horseshoe];
}

/// Teeth.
const FANG: Rgb = Rgb::hex(0xe6dfcf);
const CLAW: Rgb = Rgb::hex(0x2e2724);
/// The lining of the mouth.
const MOUTH: Rgb = Rgb::hex(0x7a2f36);
/// Glints on the eyes.
const GLINT: Rgb = Rgb::hex(0xf4f1ea);

/// One bat or giant bat, every choice the seed makes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatState {
    pub species: Species,
    pub seed: u32,
    /// Head and body, nose to rump, in world units.
    pub length: f32,
    /// Wing tip to wing tip with the wings spread, world units.
    pub wingspan: f32,
    /// 0 lean to 1 stout.
    pub build: f32,
    /// Ear size, 1 usual.
    pub ears: f32,
    pub ear_shape: BatEars,
    pub nose: BatNose,
    pub colouring: BatColouring,
    pub membrane: BatMembrane,
}

/// Salt for the bat stream. Our own.
const BAT_SALT: u32 = 0x00BA_7B47;

impl BatState {
    /// A bat or giant bat from `seed` ([`crate::bat`]); one in five is a giant bat. Our own odds.
    pub fn roll(seed: u32) -> BatState {
        let mut rng = Mulberry32::new(seed ^ BAT_SALT ^ 0x7171);
        let species = if rng.chance(0.2) { Species::GiantBat } else { Species::Bat };
        BatState::roll_as(seed, species)
    }

    /// A bat of `species` (a bat or a giant bat) from `seed`.
    pub fn roll_as(seed: u32, species: Species) -> BatState {
        let species = if species.is_bat() { species } else { Species::Bat };
        let mut rng = Mulberry32::new(seed ^ BAT_SALT);
        let giant = species == Species::GiantBat;
        // A bat of 22 to 28 cm nose to rump with a wingspan of 70 to 110 cm, and a giant bat of
        // 65 to 80 cm with a wingspan of 2 to 3 m: our own departure from the SRD's sizes
        // ([`BatState::drawn_size`]), so a flock reads as a threat. Hit points and bite stay the
        // stat block's.
        let length = if giant { rng.range_f32(6.5, 8.0) } else { rng.range_f32(2.2, 2.8) };
        let wingspan = length * if giant { rng.range_f32(3.1, 3.7) } else { rng.range_f32(3.2, 4.0) };
        let colouring = if giant {
            *rng.weighted(&[(BatColouring::Dark, 5), (BatColouring::Brown, 4), (BatColouring::Grey, 3), (BatColouring::Russet, 1)])
        } else {
            *rng.weighted(&[(BatColouring::Brown, 6), (BatColouring::Dark, 4), (BatColouring::Grey, 3), (BatColouring::Russet, 2), (BatColouring::Pale, 1)])
        };
        let membrane = *rng.weighted(&[(BatMembrane::Dusky, 4), (BatMembrane::Smoky, 3), (BatMembrane::Leathery, 4), (BatMembrane::Rosy, 1)]);
        let ear_shape = *rng.weighted(&[(BatEars::Pointed, 4), (BatEars::Rounded, 3), (BatEars::Tall, 2)]);
        let nose = *rng.weighted(&[(BatNose::Plain, 4), (BatNose::Leaf, 2), (BatNose::Horseshoe, 2)]);
        BatState { species, seed, length, wingspan, build: rng.range_f32(0.0, 1.0), ears: rng.range_f32(0.85, 1.2), ear_shape, nose, colouring, membrane }
    }

    /// Its SRD size category.
    pub fn size(&self) -> Size {
        self.species.stat_block().size
    }

    /// The size its body is drawn at, one up from the SRD's where there is one: a bat Small
    /// where the SRD's is Tiny; a giant bat is Large already. Our own departure; its wings,
    /// spread, fit this size's space.
    pub fn drawn_size(&self) -> Size {
        if self.species == Species::GiantBat { Size::Large } else { Size::Small }
    }

    /// The circle it covers on the floor roosting, wings folded: about its body's length.
    pub fn radius(&self) -> f32 {
        0.5 * self.length
    }

    /// Its proportions, which the rig and its poses share.
    pub fn dims(&self) -> BatDims {
        let l = self.length;
        let girth = 0.9 + 0.25 * self.build;
        let half_width = 0.16 * l * girth;
        let half_height = 0.14 * l * (0.92 + 0.15 * self.build);
        let torso = 0.66 * l;
        let shoulder = [0.72 * half_width, 0.2 * torso, 0.5 * half_height];
        let unit = (0.5 * self.wingspan - shoulder[0]) / WING_REACH;
        BatDims {
            length: l,
            half_width,
            half_height,
            torso,
            head_length: 0.4 * l,
            neck: [0.0, 0.5 * torso - 0.06 * l, 0.1 * half_height],
            shoulder,
            hip: [0.62 * half_width, -0.36 * torso, -0.25 * half_height],
            wing: WingDims::new(unit),
            leg: 0.46 * l,
            tail: 0.3 * l,
        }
    }

    /// The body's trunk in its own spine frame, centred on the body bone.
    pub fn body_trunk(&self, d: &BatDims) -> Trunk {
        let (w, h) = (d.half_width, d.half_height);
        Trunk {
            start: -0.5 * d.torso,
            length: d.torso,
            width: vec![(0.0, 0.0), (0.04, 0.36 * w), (0.18, 0.86 * w), (0.38, w), (0.62, 0.84 * w), (0.84, 0.5 * w), (0.95, 0.3 * w), (1.0, 0.0)],
            height: vec![(0.0, 0.0), (0.04, 0.4 * h), (0.2, 0.9 * h), (0.4, h), (0.64, 0.84 * h), (0.86, 0.52 * h), (0.95, 0.3 * h), (1.0, 0.0)],
            lift: vec![(0.0, 0.0), (0.3, 0.06 * h), (1.0, -0.06 * h)],
            squareness: 2.1,
            belly: 0.8,
        }
    }

    /// The head's trunk in the head bone's frame (the neck at its origin, snout along +Y).
    pub fn head_trunk(&self, d: &BatDims) -> Trunk {
        let l = d.length;
        let (w, h) = (0.17 * l * (0.92 + 0.18 * self.build), 0.15 * l);
        Trunk {
            start: -0.1 * l,
            length: d.head_length,
            width: vec![(0.0, 0.0), (0.07, 0.62 * w), (0.24, w), (0.44, 0.9 * w), (0.64, 0.58 * w), (0.84, 0.44 * w), (0.96, 0.32 * w), (1.0, 0.0)],
            height: vec![(0.0, 0.0), (0.07, 0.62 * h), (0.26, h), (0.46, 0.86 * h), (0.66, 0.6 * h), (0.86, 0.46 * h), (0.96, 0.32 * h), (1.0, 0.0)],
            lift: vec![(0.0, 0.0), (0.3, 0.02 * h), (0.8, -0.08 * h), (1.0, -0.12 * h)],
            squareness: 2.2,
            belly: 0.75,
        }
    }

    /// The line the mouth cuts the head at (z in the head's frame) and the jaw's hinge.
    pub fn mouth(&self, d: &BatDims) -> (f32, V3) {
        let head = self.head_trunk(d);
        let line = -0.2 * head.half_height(0.3);
        (line, [0.0, head.start + 0.3 * head.length, line])
    }

    /// Its bones and meshes, with the wings spread at rest.
    pub fn build(&self) -> Rig {
        let d = self.dims();
        let mut rng = Mulberry32::new(self.seed ^ BAT_SALT ^ 0xB0D7);
        let c = self.colouring;
        let rest = crate::pose::bat(self, &crate::pose::BatPose::default());
        let mut rig = Rig::default();
        let l = d.length;

        // Body: furred, paler under the belly, with the tail membrane stretched behind.
        let trunk = self.body_trunk(&d);
        let mut fur = trunk.mesh(22, 28);
        fur.append(&trunk.tufts(110, 0.085 * l, &mut rng, |t, a| (0.05..0.95).contains(&t) && a.sin() > -0.35));
        let mut body = vec![part(fur, "fur", MaterialKind::Hair, c.fur(), true)];
        if let Some(m) = trunk.patch(0.1, 0.92, -FRAC_PI_2, |t| 1.1 * bell(t, 0.1, 0.92), 0.007 * l, false) {
            body.push(part(m, "belly", MaterialKind::Hair, c.belly(), false));
        }
        body.push(part(self.tail_membrane(&d), "membrane", MaterialKind::Skin, self.membrane.tint(), true));
        body.push(part(self.tail_bone(&d), "tail", MaterialKind::Skin, mix(c.skin(), c.fur(), 0.3), true));
        let body_bone = rig.add("body", None, rest[bone::BODY], body);

        // Head and jaw.
        let head_bone = rig.add("head", Some(body_bone), rest[bone::HEAD], self.head_parts(&d, &mut rng));
        rig.add("jaw", Some(head_bone), rest[bone::JAW], self.jaw_parts(&d, &mut rng));

        // Wings: the right, then the left, mirrored. Each bone's meshes are in its own space.
        let wing = self.wing_parts(&d);
        for (side, name) in ["right", "left"].into_iter().enumerate() {
            let [arm, fore, hand] = wing.clone();
            let mirror = |parts: Vec<MeshBuilder>, names: &[(&'static str, MaterialKind, Rgb, bool)]| -> Vec<Part> {
                parts
                    .into_iter()
                    .zip(names)
                    .map(|(mut m, &(n, material, tint, outline))| {
                        if side == 1 {
                            m.scale([-1.0, 1.0, 1.0]);
                        }
                        part(m, n, material, tint, outline)
                    })
                    .collect()
            };
            let (bone_tint, sheet) = (self.bone_tint(), self.membrane.tint());
            let first = bone::wing(side);
            let arm_bone = rig.add(
                format!("wing_{name}_arm"),
                Some(body_bone),
                rest[first],
                mirror(arm, &[("bone", MaterialKind::Skin, bone_tint, true), ("membrane", MaterialKind::Skin, sheet, true)]),
            );
            let fore_bone = rig.add(
                format!("wing_{name}_forearm"),
                Some(arm_bone),
                rest[first + 1],
                mirror(fore, &[("bone", MaterialKind::Skin, bone_tint, true), ("membrane", MaterialKind::Skin, sheet, true), ("claws", MaterialKind::Bone, CLAW, false), ("sinew", MaterialKind::Skin, bone_tint, true)]),
            );
            rig.add(
                format!("wing_{name}_hand"),
                Some(fore_bone),
                rest[first + 2],
                mirror(hand, &[("bone", MaterialKind::Skin, bone_tint, true), ("fingers", MaterialKind::Skin, bone_tint, true), ("membrane", MaterialKind::Skin, sheet, true)]),
            );
        }

        // Hind legs.
        for (side, name) in ["right", "left"].into_iter().enumerate() {
            let (thigh, foot, claws) = self.leg_parts(&d);
            let flip = |mut m: MeshBuilder| {
                if side == 1 {
                    m.scale([-1.0, 1.0, 1.0]);
                }
                m
            };
            rig.add(
                format!("leg_{name}"),
                Some(body_bone),
                rest[bone::LEGS + side],
                vec![
                    part(flip(thigh), "fur", MaterialKind::Hair, c.fur(), true),
                    part(flip(foot), "paw", MaterialKind::Skin, c.skin(), true),
                    part(flip(claws), "claws", MaterialKind::Bone, CLAW, false),
                ],
            );
        }
        debug_assert_eq!(rig.bones.len(), bone::COUNT);
        rig
    }

    /// How far `rig` reaches below and above the body's middle over every pose it flies in (a
    /// glide and a full beat, every phase of it, diving, climbing, banked and biting), world
    /// units: the least height above the floor it flies at, and how far under the ceiling. Never
    /// less than the pose it is in reaches; the bounds of each part's box, so a little over.
    pub fn flight_extent(&self, rig: &Rig) -> (f32, f32) {
        let corners: Vec<(usize, V3)> = rig
            .bones
            .iter()
            .enumerate()
            .flat_map(|(i, b)| {
                b.mesh.parts.iter().filter(|p| !p.positions.is_empty()).flat_map(move |p| {
                    let (lo, hi) = bounds_of(&p.positions);
                    (0..8).map(move |k| (i, [if k & 1 == 0 { lo[0] } else { hi[0] }, if k & 2 == 0 { lo[1] } else { hi[1] }, if k & 4 == 0 { lo[2] } else { hi[2] }]))
                })
            })
            .collect();
        let (mut below, mut above) = (0.0f32, 0.0f32);
        for k in 0..12 {
            for (power, bite) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)] {
                for (pitch, bank) in [(0.0, 0.0), (0.45, 0.0), (-0.45, 0.0), (0.0, 0.5), (0.0, -0.5)] {
                    let pose = crate::pose::BatPose { flap: TAU * k as f32 / 12.0, power, bite, pitch, bank, ..Default::default() };
                    let world = rig.world(&crate::pose::bat(self, &pose));
                    for &(i, p) in &corners {
                        let y = world[i].point(p)[1];
                        below = below.min(y);
                        above = above.max(y);
                    }
                }
            }
        }
        (below, above)
    }

    /// How far `rig` reaches below and above the body's middle hanging by its feet: the height
    /// of the body's middle under the ceiling is `above`, so the feet touch it.
    pub fn roost_extent(&self, rig: &Rig) -> (f32, f32) {
        let pose = crate::pose::BatPose { roost: 1.0, ..Default::default() };
        let world = rig.world(&crate::pose::bat(self, &pose));
        let (mut below, mut above) = (f32::MAX, f32::MIN);
        for (bone, place) in rig.bones.iter().zip(&world) {
            for part in &bone.mesh.parts {
                for &p in &part.positions {
                    let y = place.point(p)[1];
                    below = below.min(y);
                    above = above.max(y);
                }
            }
        }
        (below, above)
    }

    /// The colour of the wing's bones: between the membrane and the coat.
    fn bone_tint(&self) -> Rgb {
        mix(self.membrane.tint(), self.colouring.fur(), 0.45)
    }

    /// The membrane stretched between the hind legs and behind the rump, a triangle in the body
    /// bone's frame.
    fn tail_membrane(&self, d: &BatDims) -> MeshBuilder {
        let [hx, hy, hz] = d.hip;
        let apex = [0.0, hy - d.tail, hz];
        let b0 = [hx, hy + 0.02 * d.length, hz];
        let b1 = [-hx, hy + 0.02 * d.length, hz];
        fan(apex, b0, b1, 7, 12, 0.0, 0.0, 0.012 * d.length)
    }

    fn tail_bone(&self, d: &BatDims) -> MeshBuilder {
        let [_, hy, hz] = d.hip;
        let path = [[0.0, hy + 0.04 * d.length, hz + 0.02 * d.length], [0.0, hy - 0.5 * d.tail, hz], [0.0, hy - 0.95 * d.tail, hz]];
        let path = CatmullRom::centripetal(path.to_vec()).sample(4);
        tube(&path, 6, &SweepOptions::default(), |s| 0.02 * d.length * (1.0 - 0.7 * s))
    }

    fn head_parts(&self, d: &BatDims, rng: &mut Mulberry32) -> Vec<Part> {
        let c = self.colouring;
        let head = self.head_trunk(d);
        let l = d.length;
        let giant = self.species == Species::GiantBat;
        let (line, hinge) = self.mouth(d);
        // The head above the mouth's line.
        let mut skin = clamped(&head, 22, 24, |p| [p[0], p[1], p[2].max(line)]);
        skin.append(&head.tufts(34, 0.06 * l, rng, |t, a| (0.1..0.62).contains(&t) && a.sin() > -0.2));
        let mut parts = vec![part(skin, "fur", MaterialKind::Hair, c.fur(), true)];
        parts.push(part(mouth_lining(&head, line, 1.0), "mouth", MaterialKind::Skin, MOUTH, false));
        let _ = hinge;

        // Ears: large and cupped, standing from the crown.
        let mut ears = MeshBuilder::new();
        let mut inner = MeshBuilder::new();
        let (width, height) = (0.26 * l * self.ears, 0.4 * l * self.ears * if self.ear_shape == BatEars::Tall { 1.35 } else { 1.0 });
        let width = if self.ear_shape == BatEars::Tall { 0.72 * width } else { width };
        for side in [1.0f32, -1.0] {
            let a = FRAC_PI_2 - 0.78 * side;
            let t = 0.3;
            let n = head.normal(t, a);
            let root = head.point(t, a, -0.025 * l);
            let up = vec3::normalize_or(vec3::add(vec3::scale(n, 0.45), [0.2 * side, -0.05, 0.9]), n);
            let facing = vec3::normalize_or([0.5 * side, 1.0, 0.1], vec3::Y);
            ears.append(&ear_sheet(self.ear_shape, width, height, 0.012 * l, 1.0).place_basis(root, up, facing).clone());
            let inward = vec3::add(root, vec3::scale(facing, 0.012 * l));
            inner.append(&ear_sheet(self.ear_shape, 0.68 * width, 0.72 * height, 0.006 * l, 0.8).place_basis(inward, up, facing).clone());
        }
        parts.push(part(ears, "ears", MaterialKind::Skin, mix(c.skin(), c.fur(), 0.35), true));
        parts.push(part(inner, "ears_inner", MaterialKind::Skin, mix(c.skin(), Rgb::hex(0xc79a94), 0.5), false));

        // Eyes: small, black and bright, set high on the side of the head.
        let (mut balls, mut glints) = (MeshBuilder::new(), MeshBuilder::new());
        let r = if giant { 0.034 } else { 0.04 } * l;
        for a in [0.62f32, PI - 0.62] {
            let t = 0.52;
            let n = head.normal(t, a);
            let at = head.point(t, a, -0.35 * r);
            let (ball, glint) = eye(at, r, n, [0.0, 0.5, 0.85]);
            balls.append(&ball);
            glints.append(&glint);
        }
        parts.push(part(balls, "eyes", MaterialKind::Gem, c.eyes(), true));
        parts.push(part(glints, "glints", MaterialKind::Gem, GLINT, false).with_glow(0.6));

        // Nose.
        let nose_skin = mix(c.skin(), Rgb::hex(0x9a6a68), 0.4);
        let tip = head.point(0.985, FRAC_PI_2, 0.0);
        let mut pad = uv_sphere(0.045 * l, 12, 8);
        pad.scale([1.25, 0.8, 0.7]).translate(vec3::add(tip, [0.0, -0.004 * l, -0.012 * l]));
        let mut nose = pad;
        for side in [1.0f32, -1.0] {
            let mut hole = uv_sphere(0.012 * l, 8, 6);
            hole.scale([1.0, 0.7, 1.0]).translate(vec3::add(tip, [side * 0.028 * l, 0.022 * l, -0.012 * l]));
            nose.append(&hole);
        }
        parts.push(part(nose, "nose", MaterialKind::Skin, nose_skin, true));
        match self.nose {
            BatNose::Plain => {}
            BatNose::Leaf => {
                let mut leaf = ear_sheet(BatEars::Pointed, 0.1 * l, 0.15 * l, 0.01 * l, 0.2);
                leaf.place_basis(vec3::add(tip, [0.0, -0.012 * l, 0.0]), [0.0, 0.35, 1.0], [0.0, 1.0, -0.35]);
                parts.push(part(leaf, "leaf", MaterialKind::Skin, nose_skin, true));
            }
            BatNose::Horseshoe => {
                let r = 0.06 * l;
                let centre = vec3::add(tip, [0.0, -0.014 * l, -0.006 * l]);
                let arc: Vec<V3> = (0..=14)
                    .map(|k| {
                        let a = -0.1 * PI + 1.2 * PI * k as f32 / 14.0;
                        vec3::add(centre, [r * a.cos(), -0.02 * l * (a.sin() * 0.5 + 0.5), r * 0.8 * a.sin()])
                    })
                    .collect();
                let ridge = tube(&arc, 6, &SweepOptions { cap_start: true, cap_end: true, ..Default::default() }, |s| 0.016 * l * (0.6 + 0.4 * (PI * s).sin()));
                parts.push(part(ridge, "leaf", MaterialKind::Skin, nose_skin, true));
            }
        }

        // Fangs in the upper jaw, and a row of small incisors.
        let mut teeth = MeshBuilder::new();
        let fang = if giant { 0.085 } else { 0.07 } * l;
        for side in [1.0f32, -1.0] {
            let mut tooth = spike(0.014 * l, fang, 5, 0.7, Shading::SMOOTH);
            tooth.place([side * 0.55 * head.half_width(0.9), head.centre(0.9)[1], line + 0.004 * l], vec3::normalize_or([0.0, -0.18, -1.0], vec3::neg(vec3::Z)));
            teeth.append(&tooth);
            for k in 0..3 {
                let mut small = spike(0.007 * l, 0.03 * l, 4, 0.7, Shading::FLAT);
                let y = head.centre(0.93)[1] + 0.005 * l;
                small.place([side * (0.1 + 0.11 * k as f32) * head.half_width(0.93), y, line + 0.002 * l], vec3::neg(vec3::Z));
                teeth.append(&small);
            }
        }
        parts.push(part(teeth, "teeth", MaterialKind::Bone, FANG, false));

        // A few whiskers either side of the muzzle.
        let mut whiskers = MeshBuilder::new();
        for side in [1.0f32, -1.0] {
            for k in 0..4 {
                let t = 0.8 + 0.04 * (k % 2) as f32;
                let a = if side > 0.0 { 0.0 } else { PI } + side * (-0.25 + 0.2 * k as f32);
                let root = head.point(t, a, -0.004 * l);
                let path: Vec<V3> = (0..=4)
                    .map(|i| {
                        let s = i as f32 / 4.0;
                        vec3::add(root, vec3::scale([side * 0.9 * s, 0.1 * s - 0.4 * s * s, (-0.1 + 0.15 * k as f32 * 0.4) * s - 0.25 * s * s], 0.17 * l))
                    })
                    .collect();
                whiskers.append(&tube(&path, 4, &SweepOptions { cap_end: false, ..Default::default() }, |s| 0.004 * l * (1.0 - 0.7 * s)));
            }
        }
        parts.push(part(whiskers, "whiskers", MaterialKind::Hair, mix(c.fur(), Rgb::hex(0xd8d2c6), 0.4), false));
        parts
    }

    fn jaw_parts(&self, d: &BatDims, _rng: &mut Mulberry32) -> Vec<Part> {
        let c = self.colouring;
        let head = self.head_trunk(d);
        let l = d.length;
        let (line, hinge) = self.mouth(d);
        // The head below the mouth's line, narrower than the skull, as a short chin.
        let narrow = Trunk { width: head.width.iter().map(|&(t, w)| (t, 0.9 * w * if t > 0.9 { 0.7 } else { 1.0 })).collect(), ..head.clone() };
        let mut jaw = clamped(&narrow, 20, 22, |p| [p[0], p[1], p[2].min(line)]);
        jaw.translate(vec3::neg(hinge));
        let mut lining = mouth_lining(&head, line, -1.0);
        lining.translate(vec3::neg(hinge));
        let mut teeth = MeshBuilder::new();
        let fang = if self.species == Species::GiantBat { 0.06 } else { 0.05 } * l;
        for side in [1.0f32, -1.0] {
            let mut tooth = spike(0.012 * l, fang, 5, 0.7, Shading::SMOOTH);
            tooth.place([side * 0.4 * head.half_width(0.92), head.centre(0.92)[1], line - 0.002 * l], vec3::Z);
            tooth.translate(vec3::neg(hinge));
            teeth.append(&tooth);
        }
        vec![
            part(jaw, "jaw", MaterialKind::Hair, mix(c.fur(), c.belly(), 0.5), true),
            part(lining, "mouth", MaterialKind::Skin, MOUTH, false),
            part(teeth, "teeth", MaterialKind::Bone, FANG, false),
        ]
    }

    /// The thigh, the foot and its claws of the right hind leg, hanging along -Y from the hip.
    fn leg_parts(&self, d: &BatDims) -> (MeshBuilder, MeshBuilder, MeshBuilder) {
        let l = d.length;
        let r0 = 0.07 * l;
        let end = [0.0, -d.leg, -0.03 * l];
        let path = CatmullRom::centripetal(vec![[0.0, 0.0, 0.0], [0.02 * l, -0.5 * d.leg, -0.05 * l], end]).sample(5);
        let mut thigh = tube(&path, 8, &SweepOptions::default(), |s| r0 * (1.0 - 0.6 * s));
        thigh.append(&{
            let mut hip = uv_sphere(1.1 * r0, 10, 7);
            hip.scale([1.0, 1.2, 1.0]);
            hip
        });
        // The foot: five toes spread from the ankle, each ending in a hooked claw that curls
        // toward the belly (-Z), to grip a ridge of the ceiling.
        let mut foot = MeshBuilder::new();
        let mut claws = MeshBuilder::new();
        let mut ankle = uv_sphere(0.034 * l, 8, 6);
        ankle.translate(end);
        foot.append(&ankle);
        for k in 0..5 {
            let u = k as f32 / 4.0 * 2.0 - 1.0;
            let spread = 0.5 * u;
            let toe = 0.07 * l * (1.0 - 0.2 * u * u);
            let dir = vec3::normalize_or([spread.sin(), -spread.cos(), -0.15], vec3::neg(vec3::Y));
            let tip = vec3::add(end, vec3::scale(dir, toe));
            foot.append(&tube(&[end, vec3::lerp(end, tip, 0.5), tip], 5, &SweepOptions::default(), |s| 0.014 * l * (1.0 - 0.3 * s)));
            // A claw: a curved hook, built as a bent chain of spikes.
            let hook = CatmullRom::centripetal(vec![tip, vec3::add(tip, vec3::add(vec3::scale(dir, 0.025 * l), [0.0, 0.0, -0.012 * l])), vec3::add(tip, vec3::add(vec3::scale(dir, 0.04 * l), [0.0, 0.0, -0.035 * l]))]).sample(3);
            claws.append(&tube(&hook, 4, &SweepOptions::default(), |s| 0.0095 * l * (1.0 - 0.85 * s)));
        }
        (thigh, foot, claws)
    }

    /// The three bones' meshes of the right wing, each in its own bone's space (the arm's origin
    /// at the shoulder, the forearm's at the elbow, the hand's at the wrist), as the meshes of
    /// their parts in the order [`BatState::build`] names them: arm (bone, membrane), forearm
    /// (bone, membrane, claws), hand (bone, fingers, membrane).
    fn wing_parts(&self, d: &BatDims) -> [Vec<MeshBuilder>; 3] {
        let w = &d.wing;
        let u = w.unit;
        let off = |p: V3, o: V3| vec3::sub(p, o);
        let (e, wr, t1, h) = (w.elbow, w.wrist, w.trail, w.hip);
        let [f2, f3, f4, f5] = w.tips;
        let thick = 0.022 * u;
        let thin = |path: &[V3], r0: f32, r1: f32| tube(&CatmullRom::centripetal(path.to_vec()).sample(4), 7, &SweepOptions::default(), |s| r0 + (r1 - r0) * s);
        let bow = |a: V3, b: V3, k: f32| vec3::add(vec3::lerp(a, b, 0.5), [0.0, k * vec3::distance(a, b), 0.02 * u]);

        // Arm: the humerus from the shoulder, a knuckle at each end, and the membrane between
        // the body, the arm and the trailing edge to the elbow's hinge line.
        let s = vec3::ZERO;
        let mut arm = thin(&[s, bow(s, e, -0.05), e], 0.115 * u, 0.09 * u);
        arm.append(&uv_sphere(0.15 * u, 10, 7).translate(s).clone());
        arm.append(&uv_sphere(0.115 * u, 10, 7).translate(e).clone());
        let sheet_a = panel([s, e, t1, h], 8, 10, 0.1, 0.07 * u, thick);

        // Forearm: the radius from the elbow to the wrist and the thumb's claw; the membrane
        // between it, the hinge line and the fifth finger's.
        // The tube runs on a diagonal, which a hit capsule round it does not fit, so the elbow's
        // knuckle is its surface and the tube its own part.
        let mut sinew = thin(&[off(e, e), off(bow(e, wr, 0.0), e), off(wr, e)], 0.09 * u, 0.07 * u);
        sinew.append(&uv_sphere(0.07 * u, 10, 7).translate(off(wr, e)).clone());
        let fore = uv_sphere(0.09 * u, 10, 7);
        let sheet_b = panel([off(e, e), off(wr, e), off(f5, e), off(t1, e)], 8, 10, 0.1, 0.08 * u, thick);
        let mut thumb = spike(0.06 * u, 0.42 * u, 5, 0.7, Shading::SMOOTH);
        thumb.place(off(wr, e), vec3::normalize_or([0.5, 0.8, 0.55], vec3::Y));

        // Hand: four fingers fanned from the wrist (the longest, third, is the one blows
        // find), and the three membranes between them.
        let o = wr;
        let finger = |tip: V3, r0: f32, bend: f32| {
            let tip = off(tip, o);
            let mid = vec3::add(vec3::scale(tip, 0.5), [0.0, bend * vec3::length(tip), 0.015 * u]);
            thin(&[vec3::ZERO, mid, tip], r0, 0.025 * u)
        };
        let finger3 = finger(f3, 0.075 * u, 0.03);
        let mut fingers = finger(f2, 0.06 * u, 0.0);
        fingers.append(&finger(f4, 0.06 * u, 0.04));
        fingers.append(&finger(f5, 0.06 * u, 0.0));
        let mut hand_sheet = fan(vec3::ZERO, off(f2, o), off(f3, o), 8, 8, 0.16, 0.06 * u, thick);
        hand_sheet.append(&fan(vec3::ZERO, off(f3, o), off(f4, o), 9, 8, 0.2, 0.09 * u, thick));
        hand_sheet.append(&fan(vec3::ZERO, off(f4, o), off(f5, o), 8, 8, 0.2, 0.09 * u, thick));
        [vec![arm, sheet_a], vec![fore, sheet_b, thumb, sinew], vec![finger3, fingers, hand_sheet]]
    }
}

/// The reach of a spread wing from the shoulder along x, in the wing's own units: where the
/// third finger's tip lands.
const WING_REACH: f32 = 2.0 + 2.35 * 0.998;

/// A bat's proportions: everything its rig and poses share.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BatDims {
    pub length: f32,
    pub half_width: f32,
    pub half_height: f32,
    /// The body's length, shoulders to rump.
    pub torso: f32,
    pub head_length: f32,
    /// The head bone's origin, and the right shoulder and hip, in the body bone's spine frame
    /// (the left mirrors x).
    pub neck: V3,
    pub shoulder: V3,
    pub hip: V3,
    pub wing: WingDims,
    /// A hind leg's length, hip to ankle.
    pub leg: f32,
    pub tail: f32,
}

/// The points of a spread right wing, from its shoulder, in the wing's own frame (the body's
/// spine frame: +X out, +Y forward, +Z up; the wing lies in z = 0), and the lines it hinges on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WingDims {
    /// World units to a wing-unit.
    pub unit: f32,
    pub elbow: V3,
    pub wrist: V3,
    /// Where the trailing edge meets the line the forearm hinges on, and where it meets the body
    /// beside the hip.
    pub trail: V3,
    pub hip: V3,
    /// The tips of the second to fifth fingers.
    pub tips: [V3; 4],
}

impl WingDims {
    pub fn new(unit: f32) -> WingDims {
        let at = |x: f32, y: f32| [x * unit, y * unit, 0.0];
        let wrist = [2.0, 0.3];
        let finger = |len: f32, angle: f32| at(wrist[0] + len * angle.cos(), wrist[1] + len * angle.sin());
        WingDims {
            unit,
            elbow: at(0.95, -0.22),
            wrist: at(wrist[0], wrist[1]),
            trail: at(1.25, -1.35),
            hip: at(0.0, -1.55),
            tips: [finger(0.6, 0.55), finger(2.35, -0.06), finger(2.05, -0.62), finger(1.55, -1.05)],
        }
    }

    /// The axis the forearm folds on, from the elbow toward the trailing edge, a unit vector.
    pub fn elbow_axis(&self) -> V3 {
        vec3::normalize_or(vec3::sub(self.trail, self.elbow), vec3::neg(vec3::Y))
    }

    /// The axis the hand folds on, along the fifth finger, a unit vector.
    pub fn wrist_axis(&self) -> V3 {
        vec3::normalize_or(vec3::sub(self.tips[3], self.wrist), vec3::neg(vec3::Y))
    }
}

/// The box round `points`.
fn bounds_of(points: &[V3]) -> (V3, V3) {
    points.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| ([0, 1, 2].map(|i| lo[i].min(p[i])), [0, 1, 2].map(|i| hi[i].max(p[i]))))
}

/// A soft bump over `t0..t1`: 0 at both ends, 1 across the middle.
fn bell(t: f32, t0: f32, t1: f32) -> f32 {
    let s = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
    (4.0 * s * (1.0 - s)).clamp(0.0, 1.0).sqrt()
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
    for i in 0..=rows {
        let t = 0.12 + 0.84 * i as f32 / rows as f32;
        ring.push([0.86 * head.half_width(t), head.centre(t)[1], line - facing * 0.004]);
    }
    for i in (0..=rows).rev() {
        let t = 0.12 + 0.84 * i as f32 / rows as f32;
        ring.push([-0.86 * head.half_width(t), head.centre(t)[1], line - facing * 0.004]);
    }
    let mut m = MeshBuilder::new();
    m.push_fan_cap(&ring, [0.0, 0.0, -facing]);
    m
}

/// A thin sheet over the quadrilateral `q` (corners in order `q00`, `q10`, `q11`, `q01`, with
/// `u` from `q00` to `q10` and `v` toward `q01`/`q11`), `nu` by `nv` facets. Its far edge
/// (`v` = 1) is a free edge pulled in `sag` of the way in the middle, scalloped; the air bellies
/// it up by up to `camber`, never at an edge it shares with a bone. `thickness` thick.
fn panel(q: [V3; 4], nu: usize, nv: usize, sag: f32, camber: f32, thickness: f32) -> MeshBuilder {
    let [q00, q10, q11, q01] = q;
    let points: Vec<Vec<V3>> = (0..=nv)
        .map(|j| {
            let v = j as f32 / nv as f32;
            (0..=nu)
                .map(|i| {
                    let u = i as f32 / nu as f32;
                    // Pulled in at the middle of the free edge, none at its corners.
                    let vv = v * (1.0 - sag * (PI * u).sin() * v);
                    let lo = vec3::lerp(q00, q10, u);
                    let hi = vec3::lerp(q01, q11, u);
                    let mut p = vec3::lerp(lo, hi, vv);
                    p[2] += camber * (PI * u).sin() * (FRAC_PI_2 * v).sin();
                    p
                })
                .collect()
        })
        .collect();
    thick_sheet(&points, &GridOptions { wrap_around: false, wrap_along: false, shading: Shading::SMOOTH, row_v: None }, thickness)
}

/// A thin sheet in the triangle from `apex` to `b0` and `b1`, `rows` from the apex out and
/// `cols` across; its outer edge scalloped in by `sag` and its middle bellied up by `camber`,
/// but not along the two sides, which sit on bones.
#[allow(clippy::too_many_arguments)]
fn fan(apex: V3, b0: V3, b1: V3, rows: usize, cols: usize, sag: f32, camber: f32, thickness: f32) -> MeshBuilder {
    let points: Vec<Vec<V3>> = (0..=rows)
        .map(|j| {
            // Not quite to the apex: a point would collapse a row.
            let t = 0.03 + 0.97 * j as f32 / rows as f32;
            (0..=cols)
                .map(|i| {
                    let c = i as f32 / cols as f32;
                    let reach = t * (1.0 - sag * (PI * c).sin() * t * t);
                    let edge = vec3::lerp(b0, b1, c);
                    let mut p = vec3::add(apex, vec3::scale(vec3::sub(edge, apex), reach));
                    p[2] += camber * (PI * c).sin() * t;
                    p
                })
                .collect()
        })
        .collect();
    thick_sheet(&points, &GridOptions { wrap_around: false, wrap_along: false, shading: Shading::SMOOTH, row_v: None }, thickness)
}

/// An ear: a cupped sheet `width` across and `height` tall, `thickness` thick, standing along
/// +Y with its hollow toward -Z, its outline by `shape`. `cup` scales the hollow.
fn ear_sheet(shape: BatEars, width: f32, height: f32, thickness: f32, cup: f32) -> MeshBuilder {
    let (cols, rows) = (9usize, 10usize);
    let half = |v: f32| -> f32 {
        let w = 0.5 * width;
        match shape {
            BatEars::Pointed => w * (1.0 - v.powf(1.7)).max(0.0).powf(0.85),
            BatEars::Rounded => w * (1.0 - v.powf(2.8)).max(0.0).powf(0.5),
            BatEars::Tall => w * (1.0 - v.powf(1.25)).max(0.0).powf(0.8),
        }
    };
    let points: Vec<Vec<V3>> = (0..=rows)
        .map(|r| {
            let v = r as f32 / rows as f32;
            let h = half(v).max(0.0004 * height);
            (0..=cols)
                .map(|c| {
                    let u = c as f32 / cols as f32 * 2.0 - 1.0;
                    [u * h, v * height, -0.4 * cup * h * (1.0 - u * u) * (0.5 + 0.5 * (1.0 - v))]
                })
                .collect()
        })
        .collect();
    thick_sheet(&points, &GridOptions { wrap_around: false, wrap_along: false, shading: Shading::SMOOTH, row_v: None }, thickness)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pose::{self, BatPose};
    use std::collections::HashSet;

    fn both() -> [Species; 2] {
        [Species::Bat, Species::GiantBat]
    }

    #[test]
    fn bats_are_deterministic() {
        for seed in [0, 7, 1207, u32::MAX] {
            assert_eq!(BatState::roll(seed), BatState::roll(seed));
            assert_eq!(BatState::roll(seed).build(), BatState::roll(seed).build());
        }
        assert_ne!(BatState::roll(1), BatState::roll(2));
    }

    #[test]
    fn every_bat_part_validates() {
        for seed in 0..12 {
            for species in both() {
                let bat = BatState::roll_as(seed, species);
                let rig = bat.build();
                rig.validate().unwrap_or_else(|e| panic!("{species:?} {seed}: {e}"));
                assert_eq!(rig.bones.len(), bone::COUNT);
                for name in ["body", "head", "jaw", "wing_right_arm", "wing_right_forearm", "wing_right_hand", "wing_left_arm", "wing_left_forearm", "wing_left_hand", "leg_right", "leg_left"] {
                    assert!(rig.index(name).is_some(), "{name}");
                }
            }
        }
    }

    #[test]
    fn bats_have_fur_ears_eyes_and_wings_that_read() {
        let rig = BatState::roll_as(5, Species::Bat).build();
        let names = |bone: &str| -> HashSet<&str> { rig.bone(bone).unwrap().mesh.parts.iter().map(|p| p.name).collect() };
        for name in ["fur", "ears", "eyes", "glints", "nose", "teeth"] {
            assert!(names("head").contains(name), "{name}");
        }
        assert!(names("jaw").contains("teeth") && names("jaw").contains("mouth"));
        for bone in ["wing_right_arm", "wing_left_forearm", "wing_right_hand"] {
            assert!(names(bone).contains("membrane") && names(bone).contains("bone"), "{bone}");
        }
        // Fur tufts make the body much more than a smooth blob.
        let fur = &rig.bone("body").unwrap().mesh.parts[0];
        assert!(fur.indices.len() / 3 > 2000, "{} triangles", fur.indices.len() / 3);
        assert!(rig.part_count() > 30);
        let head = rig.bone("head").unwrap();
        assert!(head.mesh.parts.iter().any(|p| p.name == "glints" && p.glow > 0.0));
    }

    #[test]
    fn wings_spread_to_the_wingspan_and_fit_the_drawn_size() {
        for seed in 0..24 {
            for species in both() {
                let bat = BatState::roll_as(seed, species);
                let size = bat.drawn_size();
                assert_eq!(species == Species::GiantBat, size == Size::Large);
                let (lo, hi) = bat.build().rest_mesh().bounds().unwrap();
                let span = hi[0] - lo[0];
                assert!((span - bat.wingspan).abs() < 0.06 * bat.wingspan, "{species:?} {seed}: spread {span} for a wingspan of {}", bat.wingspan);
                assert!(bat.wingspan <= size.space(), "{species:?} {seed}: {} > {}", bat.wingspan, size.space());
                if let Some(smaller) = size.smaller() {
                    assert!(bat.wingspan > 0.8 * smaller.space() || species == Species::Bat, "{species:?} {seed}");
                }
                let (short, long) = if species == Species::GiantBat { (6.5, 8.0) } else { (2.2, 2.8) };
                assert!((short..=long).contains(&bat.length), "{species:?} {seed}: {}", bat.length);
            }
        }
    }

    #[test]
    fn colourings_and_shapes_vary_across_seeds() {
        let bats: Vec<BatState> = (0..300).map(BatState::roll).collect();
        assert_eq!(bats.iter().map(|b| b.colouring).collect::<HashSet<_>>().len(), BatColouring::ALL.len());
        assert_eq!(bats.iter().map(|b| b.membrane).collect::<HashSet<_>>().len(), BatMembrane::ALL.len());
        assert_eq!(bats.iter().map(|b| b.ear_shape).collect::<HashSet<_>>().len(), BatEars::ALL.len());
        assert_eq!(bats.iter().map(|b| b.nose).collect::<HashSet<_>>().len(), BatNose::ALL.len());
        let giants = bats.iter().filter(|b| b.species == Species::GiantBat).count();
        assert!((30..=90).contains(&giants), "{giants} giant bats in 300");
    }

    /// The posed rig's lowest and highest points, over a rig posed by `local` and lifted by
    /// `height`.
    fn extent(rig: &Rig, local: &[fb_body::Placement]) -> (f32, f32) {
        let b = rig.posed_mesh(local).bounds().unwrap();
        (b.0[1], b.1[1])
    }

    #[test]
    fn a_roosting_bat_hangs_by_its_feet_from_the_ceiling() {
        for seed in 0..8 {
            for species in both() {
                let bat = BatState::roll_as(seed, species);
                let rig = bat.build();
                let (below, above) = bat.roost_extent(&rig);
                assert!(above > 0.0 && below < 0.0, "{species:?} {seed}: {below} {above}");
                // Hung from a ceiling at 30, the feet reach it and nothing goes through.
                let ceiling = 30.0;
                let pose = BatPose { roost: 1.0, height: ceiling - above, ..BatPose::default() };
                let (lo, hi) = extent(&rig, &pose::bat(&bat, &pose));
                assert!((hi - ceiling).abs() < 1e-3, "{species:?} {seed}: top {hi}");
                assert!(lo > ceiling - 1.6 * bat.length * 2.0, "{species:?} {seed}: hangs down to {lo}");
                // Wings folded: it is no wider than its body is long.
                let b = rig.posed_mesh(&pose::bat(&bat, &pose)).bounds().unwrap();
                assert!(b.1[0] - b.0[0] < 1.1 * bat.length, "{species:?} {seed}: roosts {} wide", b.1[0] - b.0[0]);
            }
        }
    }

    #[test]
    fn a_flying_bat_stays_within_the_extent_it_reports() {
        for species in both() {
            let bat = BatState::roll_as(3, species);
            let rig = bat.build();
            let (below, above) = bat.flight_extent(&rig);
            assert!(below < 0.0 && above > 0.0);
            for k in 0..12 {
                for power in [0.0, 0.5, 1.0] {
                    let pose = BatPose { flap: TAU * k as f32 / 12.0, power, bite: if k % 3 == 0 { 1.0 } else { 0.0 }, pitch: 0.3 * (k as f32 - 5.5) / 5.5, bank: 0.4, ..BatPose::default() };
                    let (lo, hi) = extent(&rig, &pose::bat(&bat, &pose));
                    assert!(lo >= below - 1e-3 && hi <= above + 1e-3, "{species:?} k={k} power={power}: {lo}..{hi} vs {below}..{above}");
                }
            }
        }
    }

    #[test]
    fn a_wingbeat_moves_the_tips_and_never_opens_the_wing() {
        let bat = BatState::roll_as(4, Species::Bat);
        let rig = bat.build();
        let tip = |pose: &BatPose| {
            let local = pose::bat(&bat, pose);
            let world = rig.world(&local);
            let hand = bone::wing(0) + 2;
            world[hand].point(bat.dims().wing.tips[1])
        };
        let up = tip(&BatPose { flap: 0.0, power: 1.0, ..BatPose::default() });
        let down = tip(&BatPose { flap: PI, power: 1.0, ..BatPose::default() });
        assert!(up[1] - down[1] > 0.3 * bat.wingspan, "the tip travels {}", up[1] - down[1]);
        // Every wing bone stays joined to its parent: a hinge keeps the elbow and the wrist
        // where they were however the wing folds.
        let d = bat.dims();
        for k in 0..16 {
            let pose = BatPose { flap: TAU * k as f32 / 16.0, power: 1.0, roost: if k % 2 == 0 { 0.0 } else { 1.0 }, ..BatPose::default() };
            let local = pose::bat(&bat, &pose);
            let world = rig.world(&local);
            let arm = bone::wing(0);
            let elbow = world[arm].point(d.wing.elbow);
            assert!(vec3::distance(elbow, world[arm + 1].origin) < 1e-3, "k={k}");
            let wrist = world[arm].then(&local[arm + 1]).point(vec3::sub(d.wing.wrist, d.wing.elbow));
            assert!(vec3::distance(wrist, world[arm + 2].origin) < 1e-3, "k={k}");
        }
    }

    #[test]
    fn the_mirrored_wing_moves_as_a_mirror() {
        let bat = BatState::roll_as(2, Species::GiantBat);
        let local = pose::bat(&bat, &BatPose { flap: 1.1, power: 1.0, ..BatPose::default() });
        let rig = bat.build();
        let world = rig.world(&local);
        for k in 0..3 {
            let (r, l) = (world[bone::wing(0) + k].origin, world[bone::wing(1) + k].origin);
            assert!((r[0] + l[0]).abs() < 1e-3 && (r[1] - l[1]).abs() < 1e-3 && (r[2] - l[2]).abs() < 1e-3, "bone {k}: {r:?} {l:?}");
        }
    }
}
