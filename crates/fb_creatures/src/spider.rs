//! Giant spiders: [`SpiderState`] from a seed, and its [`Rig`].
//!
//! The rig is a cephalothorax (the carapace, with its eight eyes), an abdomen, two fangs on
//! stout bases, two pedipalps and eight legs of three bones each (femur, tibia and tarsus), so
//! the game can walk it on all eight, rear it up on the back four and stab with its fangs
//! without a skeleton. Each leg is a long hairy tube with bristles standing out of it, a knob
//! at each joint so the bones overlap where they bend, and a tarsus ending in two hooked
//! claws; the knees stand high over the body, as a spider's do. The carapace is a smooth
//! shield; the abdomen carries the hair and the markings laid over it as patches.
//!
//! Only the drawing departs from the SRD's: the stat block is Large ([SRD 5.1 p.
//! 379](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=379)), a leg span
//! of 1.7 to 2.3 m with the body about a metre long, which fills a Large creature's square
//! (3 m) with its legs out. Proportions, colourings and markings are our own proposal, after
//! the huntsman and wolf spiders for the build (a body low between long legs, front legs
//! longest), the orb weavers and garden spiders for the abdomen's markings, and the black
//! widow's red for the black one's underside.

use armory::geom::vec3::{self, V3};
use armory::geom::{CatmullRom, MeshBuilder, SweepOptions, spike, tube, uv_sphere};
use armory::{MaterialKind, Part, Rgb};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, PI};

use crate::rat::{mix, part};
use crate::rig::Rig;
use crate::shape::{Trunk, eye};
use crate::{Size, Species};

/// Pairs of legs, and legs.
pub const LEG_PAIRS: usize = 4;
pub const LEG_COUNT: usize = 2 * LEG_PAIRS;

/// Bone indices in a spider's [`Rig`].
pub mod bone {
    /// The cephalothorax.
    pub const BODY: usize = 0;
    pub const ABDOMEN: usize = 1;
    /// The left fang; the right one follows it.
    pub const FANG: usize = 2;
    /// The left pedipalp; the right one follows it.
    pub const PALP: usize = 4;
    /// The first leg's femur; leg `k` has its femur at `LEGS + 3k`, its tibia after it and its
    /// tarsus after that. Legs run front pair to back pair, the left leg before the right.
    pub const LEGS: usize = 6;
    pub const COUNT: usize = LEGS + 3 * super::LEG_COUNT;
}

/// Which side leg `k` (in bone order) is on, in the spine frame's x: -1 the left, +1 the right.
pub fn leg_side(k: usize) -> f32 {
    if k % 2 == 0 { -1.0 } else { 1.0 }
}

/// A spider's colouring: carapace, abdomen, legs, pale hair, markings, eyes and fangs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpiderColouring {
    /// Earth browns with cream and dark brown markings; amber eyes.
    Brown,
    /// Glossy black with red markings; red eyes.
    Black,
    /// Ash grey, with green eyes.
    Ash,
    /// Rust red, with yellow eyes.
    Rust,
    /// Pale as a cave spider, with milky blue eyes.
    Cave,
}

impl SpiderColouring {
    pub const ALL: [SpiderColouring; 5] = [SpiderColouring::Brown, SpiderColouring::Black, SpiderColouring::Ash, SpiderColouring::Rust, SpiderColouring::Cave];

    /// Carapace, abdomen, legs, pale hair, markings, eyes and fangs. Our own picks.
    pub fn colours(self) -> [Rgb; 7] {
        match self {
            SpiderColouring::Brown => [Rgb::hex(0x4d3826), Rgb::hex(0x6a4e35), Rgb::hex(0x5a4230), Rgb::hex(0xc2a678), Rgb::hex(0xc9b283), Rgb::hex(0xe0a42a), Rgb::hex(0x2a1a12)],
            SpiderColouring::Black => [Rgb::hex(0x1d1b1c), Rgb::hex(0x262224), Rgb::hex(0x2c2729), Rgb::hex(0x8a7f78), Rgb::hex(0xb02a22), Rgb::hex(0xe8402e), Rgb::hex(0x120e0e)],
            SpiderColouring::Ash => [Rgb::hex(0x5e5b57), Rgb::hex(0x77736c), Rgb::hex(0x66625c), Rgb::hex(0xc9c3b6), Rgb::hex(0x34322f), Rgb::hex(0x9bd45a), Rgb::hex(0x2c2926)],
            SpiderColouring::Rust => [Rgb::hex(0x6b3320), Rgb::hex(0x8a4528), Rgb::hex(0x7a3d25), Rgb::hex(0xd7b48a), Rgb::hex(0x2e170d), Rgb::hex(0xf0cc48), Rgb::hex(0x2a140d)],
            SpiderColouring::Cave => [Rgb::hex(0xb9ae96), Rgb::hex(0xcfc6b0), Rgb::hex(0xc2b89f), Rgb::hex(0xefe8d6), Rgb::hex(0x948f82), Rgb::hex(0xa8dcea), Rgb::hex(0xd8ccb0)],
        }
    }

    pub fn carapace(self) -> Rgb {
        self.colours()[0]
    }
    pub fn abdomen(self) -> Rgb {
        self.colours()[1]
    }
    pub fn legs(self) -> Rgb {
        self.colours()[2]
    }
    pub fn pale(self) -> Rgb {
        self.colours()[3]
    }
    pub fn markings(self) -> Rgb {
        self.colours()[4]
    }
    pub fn eyes(self) -> Rgb {
        self.colours()[5]
    }
    pub fn fangs(self) -> Rgb {
        self.colours()[6]
    }
}

/// The markings on a spider's abdomen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpiderMarking {
    Plain,
    /// One stripe down the back.
    Stripe,
    /// A row of diamonds down the back, shrinking toward the rear.
    Diamonds,
    /// Pairs of spots either side of the spine.
    Spots,
    /// Rings across the back.
    Bands,
}

impl SpiderMarking {
    pub const ALL: [SpiderMarking; 5] = [SpiderMarking::Plain, SpiderMarking::Stripe, SpiderMarking::Diamonds, SpiderMarking::Spots, SpiderMarking::Bands];
}

/// One giant spider, every choice the seed makes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpiderState {
    pub seed: u32,
    /// Leg span, world units: across the legs when planted, tip to tip.
    pub span: f32,
    /// The abdomen's size, 1 usual.
    pub abdomen: f32,
    /// 0 a round abdomen to 1 a long egg.
    pub elongation: f32,
    /// 0 lean to 1 stout, in the legs and the carapace.
    pub build: f32,
    /// How much hair it grows: 0 smooth and glossy like a widow or a huntsman, about 0.15 to 0.45
    /// lightly haired, 0.6 to 1 a heavy coat like a tarantula's.
    pub hair: f32,
    pub colouring: SpiderColouring,
    pub marking: SpiderMarking,
    /// Whether pale rings mark its legs at the knees.
    pub banded: bool,
}

/// The hairiness a roll of `h` in 0..1 gives: nearly half of spiders are smooth (45%), about two in
/// five lightly haired (40%), and the rest (15%) heavily haired.
fn hairiness(h: f32) -> f32 {
    if h < 0.45 {
        0.0
    } else if h < 0.85 {
        0.15 + 0.3 * (h - 0.45) / 0.4
    } else {
        0.6 + 0.4 * (h - 0.85) / 0.15
    }
}

/// Salt for the spider stream, so a spider and a rat from one seed are unrelated. Our own.
const SPIDER_SALT: u32 = 0x5B1D_E7A3;

impl SpiderState {
    /// A giant spider from `seed` ([`crate::spider`]).
    pub fn roll(seed: u32) -> SpiderState {
        let mut rng = Mulberry32::new(seed ^ SPIDER_SALT);
        SpiderState {
            seed,
            span: rng.range_f32(17.0, 23.0),
            abdomen: rng.range_f32(0.88, 1.15),
            elongation: rng.range_f32(0.0, 1.0),
            build: rng.range_f32(0.0, 1.0),
            hair: hairiness(rng.range_f32(0.0, 1.0)),
            colouring: *rng.weighted(&[(SpiderColouring::Brown, 5), (SpiderColouring::Black, 3), (SpiderColouring::Ash, 3), (SpiderColouring::Rust, 2), (SpiderColouring::Cave, 1)]),
            marking: *rng.weighted(&[(SpiderMarking::Plain, 3), (SpiderMarking::Stripe, 2), (SpiderMarking::Diamonds, 3), (SpiderMarking::Spots, 2), (SpiderMarking::Bands, 2)]),
            banded: rng.chance(0.5),
        }
    }

    /// Its proportions, which the rig and its poses share.
    pub fn dims(&self) -> SpiderDims {
        let s = self.span;
        let girth = 0.9 + 0.2 * self.build;
        let carapace = Trunk {
            start: -0.5 * 0.2 * s,
            length: 0.2 * s,
            width: vec![(0.0, 0.0), (0.04, 0.5), (0.15, 0.85), (0.4, 1.0), (0.65, 0.84), (0.85, 0.55), (0.96, 0.3), (1.0, 0.0)].into_iter().map(|(t, w)| (t, w * 0.082 * s * girth)).collect(),
            height: vec![(0.0, 0.0), (0.05, 0.6), (0.25, 0.9), (0.45, 1.0), (0.75, 0.85), (0.93, 0.6), (1.0, 0.0)].into_iter().map(|(t, h)| (t, h * 0.05 * s)).collect(),
            lift: vec![(0.0, -0.004 * s), (0.5, 0.008 * s), (1.0, -0.006 * s)],
            squareness: 2.3,
            belly: 0.45,
        };
        let a = 0.27 * s * self.abdomen * (0.95 + 0.25 * self.elongation);
        let abdomen = Trunk {
            start: -0.5 * a,
            length: a,
            width: vec![(0.0, 0.0), (0.03, 0.35), (0.12, 0.75), (0.38, 1.0), (0.62, 0.92), (0.82, 0.6), (0.95, 0.27), (1.0, 0.0)].into_iter().map(|(t, w)| (t, w * 0.11 * s * self.abdomen * (1.05 - 0.25 * self.elongation) * girth)).collect(),
            height: vec![(0.0, 0.0), (0.03, 0.38), (0.12, 0.8), (0.38, 1.0), (0.62, 0.93), (0.82, 0.62), (0.95, 0.28), (1.0, 0.0)].into_iter().map(|(t, h)| (t, h * 0.105 * s * self.abdomen * (1.0 - 0.2 * self.elongation))).collect(),
            lift: vec![(0.0, 0.0)],
            squareness: 2.0,
            belly: 1.0,
        };
        // The legs: the front pair longest, then the back, the middle two shorter; the hips along
        // the carapace's sides, low on it.
        let reach = [1.05, 0.9, 0.95, 1.12];
        let hip_at = [0.74, 0.56, 0.38, 0.2];
        let legs = reach.map(|k| SpiderLeg { femur: 0.21 * s * k, tibia: 0.29 * s * k, tarsus: 0.08 * s * k, reach: 0.34 * s * k, girth: (0.85 + 0.3 * self.build) * k.sqrt() });
        let hips = hip_at.map(|t| {
            let p = carapace.point(t, -0.5, 0.0);
            // Inside the surface, so a leg's first ball is hidden in the carapace.
            [p[0] * 0.9, p[1], p[2] - 0.004 * s]
        });
        let centre_height = 0.15 * s;
        SpiderDims {
            span: s,
            abdomen_trunk: abdomen,
            carapace_trunk: carapace,
            centre_height,
            hips,
            azimuth: [0.62, 1.22, 1.9, 2.5],
            legs,
            abdomen_root: [0.0, -0.5 * 0.2 * s - 0.5 * a + 0.1 * s * 0.2, 0.03 * s],
            abdomen_tilt: -0.12,
            fang_root: [0.24 * 0.082 * s * girth, 0.1 * s - 0.012 * s, -0.026 * s],
            palp_root: [0.4 * 0.082 * s * girth, 0.5 * 0.2 * s - 0.004 * s, -0.012 * s],
        }
    }

    /// Its SRD size category, which its hit points and bite go with.
    pub fn size(&self) -> Size {
        self.species().stat_block().size
    }

    /// The size it is drawn at: the SRD's own, Large, its legs filling a 3 m square.
    pub fn drawn_size(&self) -> Size {
        self.size()
    }

    pub fn species(&self) -> Species {
        Species::GiantSpider
    }

    /// The circle it covers on the floor, about its centre: its legs, planted.
    pub fn radius(&self) -> f32 {
        0.5 * self.span
    }

    /// Its bones and meshes, standing at rest.
    pub fn build(&self) -> Rig {
        let d = self.dims();
        let mut rng = Mulberry32::new(self.seed ^ SPIDER_SALT ^ 0xB0D7);
        let c = self.colouring;
        let s = self.span;
        let rest = crate::pose::spider(self, &crate::pose::SpiderPose::default());
        let mut rig = Rig::default();

        // The cephalothorax: a carapace, bristled only on a hairy spider, and its eight eyes.
        let body = rig.add("body", None, rest[bone::BODY], self.carapace_parts(&d, &mut rng));

        // The abdomen: hair over the skin if it has any, with the markings laid on it.
        rig.add("abdomen", Some(body), rest[bone::ABDOMEN], self.abdomen_parts(&d, &mut rng));

        // Fangs and pedipalps, left then right.
        for side in [-1.0f32, 1.0] {
            let name = if side < 0.0 { "left" } else { "right" };
            let k = if side < 0.0 { 0 } else { 1 };
            rig.add(format!("fang_{name}"), Some(body), rest[bone::FANG + k], self.fang_parts(&d, side));
        }
        for side in [-1.0f32, 1.0] {
            let name = if side < 0.0 { "left" } else { "right" };
            let k = if side < 0.0 { 0 } else { 1 };
            rig.add(format!("palp_{name}"), Some(body), rest[bone::PALP + k], self.palp_parts(&d, side, &mut rng));
        }

        // Legs: femur, tibia and tarsus, each bone a tube along its own +Y with a knob at its end.
        let leg_tint = c.legs();
        let pale = c.pale();
        // A smooth spider's legs are glossy shell, a hairy one's are furred.
        let leg_material = if self.hair > 0.0 { MaterialKind::Hair } else { MaterialKind::Skin };
        for k in 0..LEG_COUNT {
            let pair = k / 2;
            let leg = d.legs[pair];
            let name = format!("leg_{}_{}", pair + 1, if k % 2 == 0 { "left" } else { "right" });
            let r0 = 0.03 * s * leg.girth;
            let (femur_r, tibia_r, tarsus_r) = ((r0, 0.7 * r0), (0.7 * r0, 0.38 * r0), (0.38 * r0, 0.2 * r0));
            let bands = self.banded;

            let (fur, ring) = limb(leg.femur, femur_r.0, femur_r.1, 0.2, self.hair, s, &mut rng, bands);
            let mut parts = vec![part(fur, "fur", leg_material, leg_tint, true)];
            if let Some(ring) = ring {
                parts.push(part(ring, "bands", MaterialKind::Hair, pale, false));
            }
            let femur = rig.add(name.clone(), Some(body), rest[bone::LEGS + 3 * k], parts);

            let (fur, ring) = limb(leg.tibia, tibia_r.0, tibia_r.1, 0.08, self.hair, s, &mut rng, bands);
            let mut parts = vec![part(fur, "fur", leg_material, leg_tint, true)];
            if let Some(ring) = ring {
                parts.push(part(ring, "bands", MaterialKind::Hair, pale, false));
            }
            let tibia = rig.add(format!("{name}_tibia"), Some(femur), rest[bone::LEGS + 3 * k + 1], parts);

            let (tarsus, claws) = tarsus_mesh(leg.tarsus, tarsus_r.0, tarsus_r.1);
            rig.add(
                format!("{name}_tarsus"),
                Some(tibia),
                rest[bone::LEGS + 3 * k + 2],
                vec![part(tarsus, "paw", MaterialKind::Skin, mix(leg_tint, c.fangs(), 0.45), true), part(claws, "claws", MaterialKind::Bone, c.fangs(), false)],
            );
        }
        debug_assert_eq!(rig.bones.len(), bone::COUNT);
        rig
    }

    /// The carapace and its eyes, in the body bone's spine frame.
    fn carapace_parts(&self, d: &SpiderDims, rng: &mut Mulberry32) -> Vec<Part> {
        let c = self.colouring;
        let s = self.span;
        let trunk = &d.carapace_trunk;
        let mut shell = trunk.mesh(26, 30);
        if self.hair > 0.0 {
            shell.append(&trunk.tufts((70.0 * self.hair) as usize + 6, (0.012 + 0.008 * self.hair) * s, rng, |t, a| (0.1..0.95).contains(&t) && a.sin() > 0.1));
        }
        let mut parts = vec![part(shell, "head", MaterialKind::Skin, c.carapace(), true)];

        // A pale band of hair round the front of the carapace, and the groove down the middle of
        // its back.
        let off = 0.004 * s;
        if let Some(m) = trunk.patch(0.06, 0.3, FRAC_PI_2, |t| 0.5 * bell(t, 0.06, 0.3).sqrt(), off, false) {
            parts.push(part(m, "markings", MaterialKind::Hair, mix(c.carapace(), c.pale(), 0.45), false));
        }
        if let Some(m) = trunk.patch(0.34, 0.62, FRAC_PI_2, |t| 0.1 * bell(t, 0.34, 0.62).sqrt(), off * 1.3, false) {
            parts.push(part(m, "markings", MaterialKind::Skin, c.carapace().scaled(0.5), false));
        }

        // Eight eyes in three rows on the front of the carapace: two large ones at the front,
        // a smaller pair either side of them, and two more pairs behind, wide apart.
        let (mut balls, mut glints) = (MeshBuilder::new(), MeshBuilder::new());
        let eyes = [
            // (t along, turn off the back, radius)
            (0.9, 0.2, 0.0235),
            (0.9, 0.52, 0.0135),
            (0.78, 0.62, 0.0135),
            (0.66, 0.9, 0.0125),
        ];
        for &(t, turn, r) in &eyes {
            for side in [1.0f32, -1.0] {
                let a = FRAC_PI_2 + side * turn;
                let n = trunk.normal(t, a);
                let r = r * s;
                let at = trunk.point(t, a, -0.3 * r);
                let (ball, glint) = eye(at, r, n, [0.0, 0.6, 0.8]);
                balls.append(&ball);
                glints.append(&glint);
            }
        }
        parts.push(part(balls, "eyes", MaterialKind::Gem, c.eyes(), true).with_glow(0.35));
        parts.push(part(glints, "glints", MaterialKind::Gem, GLINT, false).with_glow(0.6));
        parts
    }

    /// The abdomen: skin and hair, markings, and the spinnerets at its tip.
    fn abdomen_parts(&self, d: &SpiderDims, rng: &mut Mulberry32) -> Vec<Part> {
        let c = self.colouring;
        let s = self.span;
        let trunk = &d.abdomen_trunk;
        let mut fur = trunk.mesh(28, 34);
        if self.hair > 0.0 {
            let count = (40.0 + 240.0 * self.hair) as usize;
            fur.append(&trunk.tufts(count, (0.013 + 0.009 * self.hair) * s, rng, |t, a| (0.05..0.96).contains(&t) && a.sin() > -0.45));
        }
        let skin = if self.hair > 0.0 { MaterialKind::Hair } else { MaterialKind::Skin };
        let mut parts = vec![part(fur, "fur", skin, c.abdomen(), true)];

        let off = 0.005 * s;
        let mark = c.markings();
        let mut marks = MeshBuilder::new();
        match self.marking {
            SpiderMarking::Plain => {}
            SpiderMarking::Stripe => {
                marks.append(&trunk.patch(0.1, 0.93, FRAC_PI_2, |t| 0.3 * bell(t, 0.1, 0.93).sqrt(), off, false).unwrap_or_default());
            }
            SpiderMarking::Diamonds => {
                let mut t0 = 0.1;
                for i in 0..5 {
                    let len = 0.2 - 0.02 * i as f32;
                    let (mid, half) = (t0 + 0.5 * len, 0.5 * len);
                    let reach = 0.62 - 0.07 * i as f32;
                    marks.append(&trunk.patch(t0, t0 + len, FRAC_PI_2, |t| reach * (1.0 - ((t - mid).abs() / half)).max(0.0), off * 1.2, false).unwrap_or_default());
                    t0 += len * 0.9;
                }
            }
            SpiderMarking::Spots => {
                for i in 0..4 {
                    let t0 = 0.14 + 0.19 * i as f32;
                    for side in [1.0f32, -1.0] {
                        let width = 0.34 - 0.04 * i as f32;
                        marks.append(&trunk.patch(t0, t0 + 0.12, FRAC_PI_2 + side * (0.62 + 0.1 * i as f32), |t| width * bell(t, t0, t0 + 0.12).sqrt(), off * 1.2, false).unwrap_or_default());
                    }
                }
            }
            SpiderMarking::Bands => {
                for i in 0..4 {
                    let t0 = 0.16 + 0.19 * i as f32;
                    marks.append(&trunk.patch(t0, t0 + 0.1, FRAC_PI_2, |t| 1.1 * bell(t, t0, t0 + 0.1).sqrt(), off * 1.2, false).unwrap_or_default());
                }
            }
        }
        if !marks.is_empty() {
            parts.push(part(marks, "markings", MaterialKind::Hair, mark, false));
        }
        // The black one carries its red on the underside, a long hourglass seen when it rears.
        if self.colouring == SpiderColouring::Black {
            let mut under = MeshBuilder::new();
            under.append(&trunk.patch(0.2, 0.55, -FRAC_PI_2, |t| 0.38 * bell(t, 0.2, 0.55).sqrt(), off, false).unwrap_or_default());
            under.append(&trunk.patch(0.5, 0.78, -FRAC_PI_2, |t| 0.3 * bell(t, 0.5, 0.78).sqrt(), off, false).unwrap_or_default());
            if !under.is_empty() {
                parts.push(part(under, "markings", MaterialKind::Hair, mark, false));
            }
        }

        // Spinnerets: two stubby cones side by side at the rear tip.
        let mut spinnerets = MeshBuilder::new();
        let tip = trunk.centre(0.0);
        for side in [1.0f32, -1.0] {
            let mut cone = spike(0.017 * s, 0.05 * s, 8, -0.4, armory::geom::Shading::SMOOTH);
            cone.place(vec3::add(tip, [side * 0.012 * s, 0.006 * s, -0.004 * s]), vec3::normalize_or([side * 0.2, -1.0, 0.25], vec3::neg(vec3::Y)));
            spinnerets.append(&cone);
        }
        parts.push(part(spinnerets, "spinnerets", MaterialKind::Skin, mix(c.abdomen(), c.fangs(), 0.4), true));
        parts
    }

    /// A fang on its base, hanging from the front of the carapace: a stout base and a curved,
    /// pointed fang bending in toward the other.
    fn fang_parts(&self, d: &SpiderDims, side: f32) -> Vec<Part> {
        let c = self.colouring;
        let s = self.span;
        let _ = d;
        let rb = 0.021 * s;
        let base_len = 0.042 * s;
        // The base: swollen toward its middle.
        let base_path: Vec<V3> = (0..=5).map(|i| [0.0, 0.004 * s * i as f32 / 5.0, -base_len * i as f32 / 5.0]).collect();
        let mut base = tube(&base_path, 10, &SweepOptions::default(), |t| rb * (0.8 + 0.4 * (PI * t).sin().max(0.0) * 0.9 + 0.0 * t) * (1.0 - 0.18 * t));
        let mut ball = uv_sphere(rb * 0.95, 12, 8);
        ball.translate([0.0, 0.0, -0.1 * rb]);
        base.append(&ball);
        // The fang: curved in toward the midline and a little forward, to a point.
        let end = [0.0, 0.004 * s, -base_len];
        let tip = vec3::add(end, [-side * 0.026 * s, 0.02 * s, -0.04 * s]);
        let mid = vec3::add(end, [-side * 0.005 * s, 0.012 * s, -0.024 * s]);
        let path = CatmullRom::centripetal(vec![end, mid, tip]).sample(5);
        let fang = tube(&path, 8, &SweepOptions { cap_start: false, cap_end: false, ..Default::default() }, |t| 0.62 * rb * (1.0 - t).powf(0.85) + 0.0006 * s);
        let mut shell = MeshBuilder::new();
        shell.append(&base);
        shell.append(&fang);
        let tint = mix(c.carapace(), c.fangs(), 0.5);
        let mut tip_mesh = spike(0.0022 * s, 0.006 * s, 6, 0.3, armory::geom::Shading::SMOOTH);
        tip_mesh.place(tip, vec3::normalize_or(vec3::sub(tip, mid), vec3::neg(vec3::Z)));
        vec![part(shell, "jaw", MaterialKind::Skin, tint, true), part(tip_mesh, "fang tips", MaterialKind::Bone, mix(c.pale(), c.fangs(), 0.4), false)]
    }

    /// A pedipalp: a short curved feeler ahead of the mouth, ending in a bulb.
    fn palp_parts(&self, d: &SpiderDims, side: f32, rng: &mut Mulberry32) -> Vec<Part> {
        let c = self.colouring;
        let s = self.span;
        let _ = d;
        let path = CatmullRom::centripetal(vec![[0.0, 0.0, 0.0], [side * 0.004 * s, 0.03 * s, -0.012 * s], [side * 0.002 * s, 0.056 * s, -0.036 * s], [0.0, 0.07 * s, -0.056 * s]]).sample(5);
        let r = 0.0125 * s;
        let mut m = tube(&path, 8, &SweepOptions::default(), |t| r * (1.0 - 0.3 * t));
        let mut bulb = uv_sphere(0.0145 * s, 10, 8);
        bulb.scale([1.0, 1.2, 1.0]).translate([0.0, 0.074 * s, -0.062 * s]);
        m.append(&bulb);
        let mut hair = MeshBuilder::new();
        let tufts = if self.hair > 0.0 { (6.0 + 12.0 * self.hair) as usize } else { 0 };
        for _ in 0..tufts {
            let i = rng.range_f32(0.1, 1.0);
            let p = CatmullRom::centripetal(vec![[0.0, 0.0, 0.0], [side * 0.004 * s, 0.03 * s, -0.012 * s], [side * 0.002 * s, 0.056 * s, -0.036 * s], [0.0, 0.07 * s, -0.056 * s]]).point(i * 2.9);
            let a = rng.range_f32(0.0, PI * 2.0);
            let dir = vec3::normalize_or([a.cos(), 0.7, a.sin() - 0.5], vec3::Y);
            let mut tuft = spike(0.0013 * s, rng.range_f32(0.008, 0.014) * s, 4, 0.6, armory::geom::Shading::SMOOTH);
            tuft.place(p, dir);
            hair.append(&tuft);
        }
        m.append(&hair);
        vec![part(m, "fur", if self.hair > 0.0 { MaterialKind::Hair } else { MaterialKind::Skin }, mix(c.legs(), c.pale(), 0.2), true)]
    }
}

/// White of the glints on the eyes.
const GLINT: Rgb = Rgb::hex(0xf4f1ea);

/// One leg's lengths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpiderLeg {
    /// Hip to knee.
    pub femur: f32,
    /// Knee to ankle: the patella, tibia and metatarsus together.
    pub tibia: f32,
    /// Ankle to the claws' tips.
    pub tarsus: f32,
    /// How far from its hip, across the floor, the foot is planted at rest.
    pub reach: f32,
    /// How stout the leg is against the usual, 1 usual.
    pub girth: f32,
}

/// A spider's proportions: everything its rig and poses share.
#[derive(Clone, Debug, PartialEq)]
pub struct SpiderDims {
    pub span: f32,
    /// The carapace's and the abdomen's shapes, each along +Y about the middle of its own bone.
    pub carapace_trunk: Trunk,
    pub abdomen_trunk: Trunk,
    /// Height of the body bone over the floor at rest.
    pub centre_height: f32,
    /// The right hip of each pair, front to back, in the body bone's spine frame (left mirrors x).
    pub hips: [V3; LEG_PAIRS],
    /// The way each pair's foot lies from its hip across the floor: radians off straight ahead,
    /// toward the side.
    pub azimuth: [f32; LEG_PAIRS],
    pub legs: [SpiderLeg; LEG_PAIRS],
    /// Where the abdomen's bone sits in the body's spine frame, and how far it is tipped
    /// (negative lifts its rear).
    pub abdomen_root: V3,
    pub abdomen_tilt: f32,
    /// The right fang's and pedipalp's joints in the spine frame (left mirrors x).
    pub fang_root: V3,
    pub palp_root: V3,
}

impl SpiderDims {
    /// The leg in bone order `k`.
    pub fn leg(&self, k: usize) -> SpiderLeg {
        self.legs[k / 2]
    }

    /// The hip of the leg in bone order `k`, in the body's spine frame.
    pub fn hip(&self, k: usize) -> V3 {
        let h = self.hips[k / 2];
        [h[0] * leg_side(k), h[1], h[2]]
    }

    /// How far above the body bone the abdomen's back reaches in the spine frame, at rest: the
    /// body's highest point on its dorsal side, for laying a fallen spider on its back.
    pub fn back_height(&self) -> f32 {
        let carapace = self.carapace_trunk.half_height(0.45) + 0.008 * self.span;
        let a = &self.abdomen_trunk;
        let abdomen = self.abdomen_root[2] + a.half_height(0.38);
        carapace.max(abdomen)
    }
}

/// Mixes the soft bump of `t` over `t0..t1`: 0 at both ends, 1 across the middle.
fn bell(t: f32, t0: f32, t1: f32) -> f32 {
    let s = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
    (4.0 * s * (1.0 - s)).clamp(0.0, 1.0).sqrt()
}

/// A limb segment along +Y from its joint to `len`, radius `r0` at the joint falling to `r1`,
/// swelling `swell` of the radius in the middle, with a knob at each end and, when `hair` is above
/// 0, fine bristles standing out of it combed toward the tip. With `bands`, also returns pale rings round the far end.
#[allow(clippy::too_many_arguments)]
fn limb(len: f32, r0: f32, r1: f32, swell: f32, hair: f32, span: f32, rng: &mut Mulberry32, bands: bool) -> (MeshBuilder, Option<MeshBuilder>) {
    let radius = |t: f32| (r0 + (r1 - r0) * t.powf(0.8)) * (1.0 + swell * (PI * t).sin().max(0.0));
    let path: Vec<V3> = (0..=10).map(|i| [0.0, len * i as f32 / 10.0, 0.0]).collect();
    let mut m = tube(&path, 12, &SweepOptions { cap_start: false, cap_end: false, ..Default::default() }, radius);
    // Knobs at the joints, so a bent leg stays whole.
    let mut start = uv_sphere(radius(0.0) * 1.02, 12, 8);
    start.translate(path[0]);
    let mut end = uv_sphere(radius(1.0) * 1.1, 12, 8);
    end.translate(path[10]);
    m.append(&start);
    m.append(&end);
    // Bristles, none at all on a smooth spider.
    let count = if hair > 0.0 { ((12.0 + 36.0 * hair) * len / (0.25 * span)) as usize } else { 0 };
    for _ in 0..count {
        let t = rng.range_f32(0.04, 0.98);
        let a = rng.range_f32(0.0, 2.0 * PI);
        let r = radius(t);
        let out = [a.cos(), 0.0, a.sin()];
        let root = vec3::add([0.0, len * t, 0.0], vec3::scale(out, 0.8 * r));
        let dir = vec3::normalize_or(vec3::add(vec3::scale(out, 1.0), [0.0, 0.7, 0.0]), out);
        let length = rng.range_f32(0.7, 1.3) * (0.009 + 0.008 * hair) * span;
        let mut bristle = spike(0.0022 * span, length, 4, 0.7, armory::geom::Shading::SMOOTH);
        bristle.place(root, dir);
        m.append(&bristle);
    }
    let ring = bands.then(|| {
        let mut rings = MeshBuilder::new();
        for (t0, t1) in [(0.9, 1.0), (0.0, 0.07)] {
            let path: Vec<V3> = (0..=3).map(|i| [0.0, len * (t0 + (t1 - t0) * i as f32 / 3.0), 0.0]).collect();
            rings.append(&tube(&path, 12, &SweepOptions::default(), |t| radius(t0 + (t1 - t0) * t) * 1.16));
        }
        rings
    });
    (m, ring)
}

/// A tarsus along +Y from the ankle to the tip at `len`, with two hooked claws at the tip.
fn tarsus_mesh(len: f32, r0: f32, r1: f32) -> (MeshBuilder, MeshBuilder) {
    let path: Vec<V3> = (0..=6).map(|i| [0.0, 0.9 * len * i as f32 / 6.0, 0.0]).collect();
    let mut m = tube(&path, 10, &SweepOptions { cap_start: false, ..Default::default() }, |t| r0 + (r1 - r0) * t);
    let mut ankle = uv_sphere(r0 * 1.03, 10, 7);
    ankle.translate([0.0, 0.0, 0.0]);
    m.append(&ankle);
    // A pad of hair under the foot.
    let mut claws = MeshBuilder::new();
    for side in [1.0f32, -1.0] {
        let base = [side * 0.25 * r1, 0.86 * len, 0.0];
        let mut claw = spike(0.5 * r1, 0.14 * len, 5, 0.9, armory::geom::Shading::SMOOTH);
        claw.place(base, vec3::normalize_or([side * 0.5, 1.0, 0.0], vec3::Y));
        claws.append(&claw);
    }
    (m, claws)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn spiders_are_deterministic() {
        for seed in [0, 7, 1207, u32::MAX] {
            assert_eq!(SpiderState::roll(seed), SpiderState::roll(seed));
            assert_eq!(SpiderState::roll(seed).build(), SpiderState::roll(seed).build());
        }
        assert_ne!(SpiderState::roll(1), SpiderState::roll(2));
    }

    #[test]
    fn every_spider_part_validates() {
        for seed in 0..16 {
            let spider = SpiderState::roll(seed);
            let rig = spider.build();
            rig.validate().unwrap_or_else(|e| panic!("{seed}: {e}"));
            assert_eq!(rig.bones.len(), bone::COUNT);
            for name in ["body", "abdomen", "fang_left", "fang_right", "palp_left", "palp_right", "leg_1_left", "leg_4_right_tarsus", "leg_2_left_tibia"] {
                assert!(rig.index(name).is_some(), "{name}");
            }
        }
    }

    #[test]
    fn a_spider_has_eight_legs_eight_eyes_and_readable_hair() {
        let spider = SpiderState::roll(5);
        let rig = spider.build();
        let legs = rig.bones.iter().filter(|b| b.name.starts_with("leg_") && !b.name.ends_with("_tibia") && !b.name.ends_with("_tarsus")).count();
        assert_eq!(legs, 8);
        let body = rig.bone("body").unwrap();
        let eyes = body.mesh.parts.iter().find(|p| p.name == "eyes").unwrap();
        // Each eye is a sphere of the same tessellation: eight of them.
        let one = uv_sphere(1.0, 14, 10).vertex_count();
        assert_eq!(eyes.positions.len(), 8 * one);
        assert!(body.mesh.parts.iter().any(|p| p.name == "glints" && p.glow > 0.0));
        // Bristles make a hairy leg much more than a smooth tube.
        let hairy = (0..200).map(SpiderState::roll).find(|s| s.hair > 0.6).unwrap();
        let femur = &hairy.build().bone("leg_1_left").unwrap().mesh.parts[0].indices.len() / 3;
        assert!(femur > 700, "{femur} triangles");
        let names: HashSet<&str> = rig.bone("abdomen").unwrap().mesh.parts.iter().map(|p| p.name).collect();
        assert!(names.contains("fur") && names.contains("spinnerets"));
    }

    #[test]
    fn many_spiders_are_smooth_some_light_and_few_heavy() {
        let spiders: Vec<SpiderState> = (0..400).map(SpiderState::roll).collect();
        let smooth = spiders.iter().filter(|s| s.hair == 0.0).count();
        let heavy = spiders.iter().filter(|s| s.hair >= 0.6).count();
        let light = spiders.len() - smooth - heavy;
        assert!((140..220).contains(&smooth), "{smooth} smooth");
        assert!((100..220).contains(&light), "{light} light");
        assert!((30..90).contains(&heavy), "{heavy} heavy");
        // A smooth spider has no bristle on its legs: its femur is a plain tube and two knobs.
        let plain = spiders.iter().find(|s| s.hair == 0.0).unwrap();
        let femur = &plain.build().bone("leg_1_left").unwrap().mesh.parts[0].indices.len() / 3;
        let hairy = spiders.iter().find(|s| s.hair >= 0.6).unwrap();
        let fur = &hairy.build().bone("leg_1_left").unwrap().mesh.parts[0].indices.len() / 3;
        assert!(fur > femur + 300, "{fur} against {femur} triangles");
    }

    #[test]
    fn a_spider_is_a_large_creature_with_its_legs_out() {
        for seed in 0..24 {
            let spider = SpiderState::roll(seed);
            assert_eq!(spider.size(), Size::Large);
            assert!((17.0..=23.0).contains(&spider.span));
            let rig = spider.build();
            let (lo, hi) = rig.rest_mesh().bounds().unwrap();
            let across = (hi[0] - lo[0]).max(hi[2] - lo[2]);
            // It fits a Large creature's 3 m square, and is well over a metre across.
            assert!(across <= Size::Large.space(), "{seed}: {across}");
            assert!(across > 12.0, "{seed}: {across}");
            // Standing on the floor, not in it or over it.
            assert!(lo[1].abs() < 0.02 * spider.span, "{seed}: lowest point {}", lo[1]);
        }
    }

    #[test]
    fn colourings_and_markings_vary_across_seeds() {
        let spiders: Vec<SpiderState> = (0..200).map(SpiderState::roll).collect();
        let colourings: HashSet<_> = spiders.iter().map(|s| s.colouring).collect();
        let markings: HashSet<_> = spiders.iter().map(|s| s.marking).collect();
        assert_eq!(colourings.len(), SpiderColouring::ALL.len());
        assert_eq!(markings.len(), SpiderMarking::ALL.len());
        assert!(spiders.iter().any(|s| s.banded) && spiders.iter().any(|s| !s.banded));
    }
}
