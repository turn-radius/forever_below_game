//! Bows: two limbs through a handle, a string, and sometimes an arrow on the string.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (`SECTIONS`
//! `limbs`, `string`, `riser` and `arrow`; `FAM_RANGE.bow`; `PRESETS.bow`; the
//! `fam==='bow'` branch of `randomize()`; `buildBow`; and `forgeNoun()`).
//!
//! Departures from the prototype:
//! - The roll picks a weighted [`BowArchetype`] (one per noun) and builds the limbs, handle
//!   and arrow to suit it, instead of a random preset with every slider nudged, so a horse bow
//!   is short with long tip levers and a longbow is long with a D-section.
//! - The prototype keeps an `arrow` section but never draws it. Here an arrow with `on`
//!   lies nocked on the string and rests against the side of the handle.
//! - The prototype's `string.draw` is always forced to 0 when building, so it is dropped:
//!   bows are drawn at brace.
//!
//! Item space: the middle of the handle at the origin, the limbs along Y (the upper tip at
//! +Y), the string on the +X side, the arrow pointing toward -X and resting on the -Z side
//! of the handle (for a right-handed archer).

use crate::build::arrow::arrow_meshes;
use crate::build::limb::{LimbGrip, LimbPlane, Stave, bowstring_mesh, point_along, serving_mesh, stave_wrap_mesh};
use crate::build::parts::Parts;
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::geom::{ItemMesh, vec3};
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::ranged::{
    Arrow, ArrowHead, ArrowLimits, Bowstring, BowstringLimits, FletchCut, LimbShape, Limbs, LimbsLimits, NockStyle,
};
use crate::slider::{Sliders, sliders};
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Bone, Brass, Cloth, Cord, Iron, Leather, Wood};

/// The bow generator.
pub struct Bow;

/// The handle between the limbs, and what is wound round it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Riser {
    /// Grip length.
    pub length: f32,
    /// Back to belly.
    pub thickness: f32,
    /// Side to side.
    pub width: f32,
    pub material: MaterialKind,
    /// A cord or thong wound round the grip.
    pub wrap: bool,
    pub turns: u32,
    pub wrap_material: MaterialKind,
}

sliders! {
    RiserLimits for Riser {
        length: 0.4, 2.5, 0.01;
        thickness: 0.05, 0.4, 0.005;
        width: 0.05, 0.4, 0.005;
        turns: 3.0, 30.0, 1.0;
    }
}

impl Default for Riser {
    fn default() -> Self {
        Riser { length: 1.2, thickness: 0.2, width: 0.16, material: Wood, wrap: true, turns: 12, wrap_material: Leather }
    }
}

/// Every parameter of one bow.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BowState {
    pub seed: u32,
    pub limbs: Limbs,
    pub string: Bowstring,
    pub riser: Riser,
    pub arrow: Arrow,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every bow section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BowLimits {
    pub limbs: LimbsLimits,
    pub string: BowstringLimits,
    pub riser: RiserLimits,
    pub arrow: ArrowLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges with `FAM_RANGE.bow` applied.
pub const LIMITS: BowLimits = BowLimits {
    limbs: LimbsLimits { length: LimbsLimits::SLIDERS.length.narrowed(9.0, 22.0), ..LimbsLimits::SLIDERS },
    string: BowstringLimits::SLIDERS,
    riser: RiserLimits::SLIDERS,
    arrow: ArrowLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl BowState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &BowLimits) {
        self.limbs.fit(&limits.limbs);
        self.string.fit(&limits.string);
        self.riser.fit(&limits.riser);
        self.arrow.fit(&limits.arrow);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &BowLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("limbs", self.limbs.out_of_range(&limits.limbs)),
            tag("string", self.string.out_of_range(&limits.string)),
            tag("riser", self.riser.out_of_range(&limits.riser)),
            tag("arrow", self.arrow.out_of_range(&limits.arrow)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

/// The kinds of bow "Forge new" composes, one per noun. Not stored: the noun reads the state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BowArchetype {
    /// A tall D-section stave.
    Longbow,
    /// A plain wooden bow, shorter than a longbow, straight or slightly reflexed.
    SelfBow,
    /// Wide flat limbs whose tips curl away from the archer.
    Recurve,
    /// Limbs set back at the handle and swept forward toward the tips.
    Reflex,
    /// A short composite bow with long tip levers, for shooting from the saddle.
    HorseBow,
    /// A long composite bow.
    HornBow,
}

impl BowArchetype {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(BowArchetype, u32); 6] = [
        (BowArchetype::Longbow, 24),
        (BowArchetype::SelfBow, 20),
        (BowArchetype::Recurve, 18),
        (BowArchetype::Reflex, 10),
        (BowArchetype::HorseBow, 16),
        (BowArchetype::HornBow, 12),
    ];
}

impl Generator for Bow {
    type State = BowState;

    fn roll(seed: u32) -> BowState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let archetype = *rng.weighted(&BowArchetype::WEIGHTS);
        compose(&mut rng, seed, archetype)
    }

    /// `forgeNoun()` for bows.
    fn noun(s: &BowState) -> String {
        let l = &s.limbs;
        match l.shape {
            LimbShape::Composite if l.length < 13.0 => "Horse Bow",
            LimbShape::Composite => "Horn Bow",
            LimbShape::Recurve => "Recurve Bow",
            LimbShape::Reflex if l.recurve > 0.4 => "Reflex Bow",
            LimbShape::Reflex => "Self Bow",
            LimbShape::Longbow if l.length > 16.0 => "Longbow",
            LimbShape::Longbow => "Self Bow",
        }
        .to_string()
    }

    /// The stave from [`crate::build::limb`] split into limbs and handle, nocks and string on
    /// the tips, a cord wound on the grip, and the arrow from [`crate::build::arrow`].
    fn build(s: &BowState) -> ItemMesh {
        let mut parts = Parts::new();
        let (l, rs) = (&s.limbs, &s.riser);
        let half = l.length * 0.5;
        let plane = LimbPlane { origin: vec3::ZERO, along: vec3::Y, toward_string: vec3::X };
        let grip = LimbGrip { half_span: rs.length / l.length, width: rs.width, thickness: rs.thickness };
        let stave = Stave::new(l, plane, grip, half, l.brace);

        let (g0, g1) = stave.grip_span(1.1);
        let mut limbs = stave.mesh(0.0, g0, 1.0);
        limbs.append(&stave.mesh(g1, 1.0, 1.0));
        parts.add("limbs", limbs, l.material);
        parts.add("riser", stave.mesh(g0, g1, 1.03), rs.material);
        if rs.wrap {
            let (w0, w1) = stave.grip_span(0.9);
            parts.add("grip_wrap", stave_wrap_mesh(&stave, w0, w1, 1.03, rs.turns, 0.022, rs.wrap_material == Leather), rs.wrap_material);
        }
        parts.add("nocks", stave.nocks_mesh(), l.nock_material);

        // The string lies along the limbs' string side; the arrow nocks on it above the hand.
        let anchors = stave.string_anchors();
        let path = stave.string_path(s.string.thickness, &[]);
        let nock_y = rs.length * 0.45;
        let nock = point_along(&path, &plane, nock_y);
        parts.add_unoutlined("string", bowstring_mesh(&anchors, &path, &s.string), Cord);
        if s.string.serving {
            parts.add_unoutlined("serving", serving_mesh(&path, nock, 0.6, &s.string), Cord);
        }

        if s.arrow.on {
            let a = &s.arrow;
            let side = rs.width * 0.5 * 1.03 + a.radius + if rs.wrap { 0.04 } else { 0.0 };
            let rest = [0.0, nock_y + a.radius, -side];
            let dir = vec3::normalize_or(vec3::sub(rest, nock), vec3::neg(vec3::X));
            let arrow = arrow_meshes(a, true).placed(nock, dir, 0.0);
            parts.add("arrow_shaft", arrow.shaft, a.material);
            parts.add("arrow_nock", arrow.nock, Bone);
            parts.add("arrow_head", arrow.head, Iron);
            parts.add_tinted("fletching", arrow.fletching, Cloth, s.palette.fletching);
        }
        parts.finish()
    }
}

impl BowState {
    /// Lays `ammo` as this bow's arrow on the string, in its own vane colour. The rolled
    /// bow is unchanged until this is called.
    pub fn load(&mut self, ammo: &crate::families::ammo::AmmoState) {
        self.arrow = ammo.arrow;
        self.arrow.on = true;
        self.palette.fletching = ammo.palette.fletching;
    }

    /// Takes the arrow on the string off.
    pub fn unload(&mut self) {
        self.arrow.on = false;
    }
}

impl Bow {
    /// "Forge new" forced to one archetype, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, a)` when `a` is what `s` rolls.
    pub fn roll_as(seed: u32, archetype: BowArchetype) -> BowState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, archetype)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, a: BowArchetype) -> BowState {
    use BowArchetype as A;
    let mut s = BowState { seed, ..BowState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    roll_limbs(rng, a, &mut s.limbs);
    roll_riser(rng, a, &s.limbs, &mut s.riser);
    s.string = Bowstring {
        thickness: rng.range_f32(0.012, 0.02) * if s.limbs.length > 16.0 { 1.15 } else { 1.0 },
        serving: rng.chance(0.7),
    };
    roll_arrow(rng, a, &s.limbs, &mut s.arrow);

    let mut targets = vec![];
    if [s.limbs.material, s.riser.material].contains(&Wood) {
        targets.push((EtchTarget::Wood, 3));
    }
    if s.riser.material == Bone || (s.limbs.nocks == NockStyle::Horn && s.limbs.nock_material == Bone) {
        targets.push((EtchTarget::Bone, 2));
    }
    if s.riser.wrap && s.riser.wrap_material == Leather {
        targets.push((EtchTarget::Leather, 1));
    }
    if targets.is_empty() {
        targets.push((EtchTarget::Wood, 1));
    }
    s.etch = Etching::roll(rng, if matches!(a, A::HornBow | A::HorseBow) { 0.25 } else { 0.15 }, &targets, 1);
    s.fit(&LIMITS);
    s
}

fn roll_limbs(rng: &mut Mulberry32, a: BowArchetype, l: &mut Limbs) {
    use BowArchetype as A;
    use LimbShape as S;
    l.shape = match a {
        A::Longbow => S::Longbow,
        A::SelfBow => *rng.weighted(&[(S::Longbow, 3), (S::Reflex, 2)]),
        A::Recurve => S::Recurve,
        A::Reflex => S::Reflex,
        A::HorseBow | A::HornBow => S::Composite,
    };
    l.length = match a {
        A::Longbow => rng.range_f32(16.5, 20.5),
        A::SelfBow => rng.range_f32(11.0, 15.8),
        A::Recurve => rng.range_f32(12.0, 16.0),
        A::Reflex => rng.range_f32(12.0, 16.0),
        A::HorseBow => rng.range_f32(9.5, 12.8),
        A::HornBow => rng.range_f32(13.5, 16.5),
    };
    l.recurve = match a {
        A::Longbow => rng.range_f32(0.0, 0.08),
        A::SelfBow => rng.range_f32(0.0, 0.35),
        A::Recurve => rng.range_f32(0.55, 0.9),
        A::Reflex => rng.range_f32(0.45, 0.9),
        A::HorseBow | A::HornBow => rng.range_f32(0.5, 0.9),
    };
    l.siyah = match a {
        A::HorseBow => rng.range_f32(0.22, 0.32),
        A::HornBow => rng.range_f32(0.15, 0.24),
        _ => 0.0,
    };
    l.brace = match a {
        A::Longbow => rng.range_f32(1.3, 1.7),
        A::SelfBow => rng.range_f32(1.3, 1.6),
        _ => rng.range_f32(1.5, 2.0),
    };
    // A longbow is deep and narrow; flat bows are wide and thin.
    (l.width, l.thickness) = match a {
        A::Longbow => (rng.range_f32(0.13, 0.17), rng.range_f32(0.11, 0.15)),
        A::SelfBow => (rng.range_f32(0.17, 0.26), rng.range_f32(0.08, 0.11)),
        A::Recurve | A::Reflex => (rng.range_f32(0.2, 0.26), rng.range_f32(0.07, 0.09)),
        A::HorseBow | A::HornBow => (rng.range_f32(0.18, 0.22), rng.range_f32(0.07, 0.09)),
    };
    l.taper = rng.range_f32(0.45, 0.65);
    l.material = Wood;
    l.nocks = *rng.weighted(match a {
        A::Longbow => &[(NockStyle::Horn, 6), (NockStyle::None, 3), (NockStyle::Cap, 1)][..],
        A::SelfBow | A::Reflex => &[(NockStyle::None, 6), (NockStyle::Horn, 2)],
        A::Recurve => &[(NockStyle::None, 5), (NockStyle::Horn, 3), (NockStyle::Cap, 1)],
        A::HorseBow | A::HornBow => &[(NockStyle::Horn, 4), (NockStyle::None, 1)],
    });
    l.nock_material = match l.nocks {
        NockStyle::Cap => *rng.weighted(&[(Brass, 2), (Iron, 1)]),
        _ => *rng.weighted(&[(Bone, 4), (Brass, 1)]),
    };
}

fn roll_riser(rng: &mut Mulberry32, a: BowArchetype, l: &Limbs, r: &mut Riser) {
    use BowArchetype as A;
    r.length = match a {
        A::HorseBow => rng.range_f32(0.8, 1.0),
        A::Longbow => rng.range_f32(1.0, 1.25),
        _ => rng.range_f32(0.9, 1.4),
    };
    // The handle is the stiff middle of the stave: as wide as the limbs, a little deeper.
    r.width = l.width * rng.range_f32(0.85, 1.0);
    r.thickness = l.thickness * rng.range_f32(1.4, 1.9);
    r.material = if matches!(a, A::HorseBow | A::HornBow) && rng.chance(0.3) { Bone } else { l.material };
    r.wrap = rng.chance(if a == A::SelfBow { 0.45 } else { 0.8 });
    r.wrap_material = *rng.weighted(&[(Leather, 3), (Cord, 2)]);
    // A thong winds edge to edge; cord winds tighter.
    let per_unit = if r.wrap_material == Leather { rng.range_f32(9.0, 14.0) } else { rng.range_f32(18.0, 26.0) };
    r.turns = (r.length * 0.9 * per_unit).round() as u32;
}

fn roll_arrow(rng: &mut Mulberry32, a: BowArchetype, l: &Limbs, ar: &mut Arrow) {
    use ArrowHead as H;
    use BowArchetype as A;
    ar.on = rng.chance(0.6);
    // Draw length follows the bow: about two fifths of its length.
    ar.length = (l.length * 0.42 + rng.range_f32(0.0, 1.0)).min(8.6);
    ar.radius = rng.range_f32(0.022, 0.03);
    ar.head = *rng.weighted(match a {
        A::Longbow => &[(H::Bodkin, 4), (H::Broadhead, 2), (H::Leaf, 1)][..],
        A::Recurve => &[(H::Broadhead, 3), (H::Leaf, 1), (H::Blunt, 1)],
        A::HorseBow | A::HornBow => &[(H::Leaf, 3), (H::Bodkin, 2), (H::Broadhead, 1)],
        A::SelfBow | A::Reflex => &[(H::Blunt, 2), (H::Leaf, 2), (H::Broadhead, 1)],
    });
    (ar.head_length, ar.head_width) = match ar.head {
        H::Bodkin => (rng.range_f32(0.3, 0.5), rng.range_f32(0.08, 0.12)),
        H::Broadhead => (rng.range_f32(0.4, 0.55), rng.range_f32(0.22, 0.32)),
        H::Leaf => (rng.range_f32(0.35, 0.5), rng.range_f32(0.14, 0.2)),
        H::Blunt => (rng.range_f32(0.2, 0.3), rng.range_f32(0.1, 0.14)),
    };
    ar.material = Wood;
    ar.fletches = *rng.weighted(&[(3, 8), (4, 1), (2, 1)]);
    ar.fletch_cut = *rng.weighted(&[(FletchCut::Shield, 3), (FletchCut::Parabolic, 3), (FletchCut::Triangle, 1)]);
    ar.fletch_length = rng.range_f32(0.8, 1.2);
    ar.fletch_height = rng.range_f32(0.1, 0.14);
}
