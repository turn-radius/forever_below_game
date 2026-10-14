//! Belts worn as their own item: plain, studded, sword belts, girdles and pouch belts, drawn
//! buckled round the implied mannequin's hips (the body is not drawn).
//!
//! The Armory prototype has only a garment's `belt` section (width, material, buckle), which
//! this family reuses for the strap; the plate prototype fakes its `Belt` base
//! (`data/bases.json`, a container holding a belt pouch) as a band with a torus buckle and an
//! ellipsoid pouch. Everything else here (the fittings, the roll, the nouns) is this port's
//! own. Geometry is [`crate::build::belt`] on [`TorsoForm::MANNEQUIN`]; the pouch is the
//! bags family's belt pouch, hung behind the right hip.

use crate::build::belt::{belt_parts, belt_pouch_mount};
use crate::build::form::TorsoForm;
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::families::bags::{Bag, BagLimits, BagState, BagStyle, Bags};
use crate::geom::vec3::{self, Mat3};
use crate::geom::ItemMesh;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::belt::{Belt, BeltFittings, BeltFittingsLimits, BeltLimits, BeltStyle};
use crate::slider::Sliders;
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Brass, Gold, Iron, Leather, Silver};

/// The belt generator.
pub struct Belts;

/// Every parameter of one belt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeltState {
    pub seed: u32,
    /// The strap and its buckle.
    pub belt: Belt,
    pub fittings: BeltFittings,
    /// The belt pouch, when [`BeltFittings::pouch`] is set (a bag of style `beltpouch`).
    pub pouch: Bag,
    /// Tooling on the leather, engraving on the buckle and mounts.
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for BeltState {
    fn default() -> Self {
        BeltState {
            seed: 0,
            belt: Belt::default(),
            fittings: BeltFittings::default(),
            pouch: Bag { style: BagStyle::BeltPouch, bedroll: false, pocket: false, ..Bag::default() },
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every belt section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeltsLimits {
    pub belt: BeltLimits,
    pub fittings: BeltFittingsLimits,
    pub pouch: BagLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// Full ranges.
pub const LIMITS: BeltsLimits = BeltsLimits {
    belt: BeltLimits::SLIDERS,
    fittings: BeltFittingsLimits::SLIDERS,
    pouch: BagLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl BeltState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &BeltsLimits) {
        self.belt.fit(&limits.belt);
        self.fittings.fit(&limits.fittings);
        self.pouch.fit(&limits.pouch);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &BeltsLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("belt", self.belt.out_of_range(&limits.belt)),
            tag("fittings", self.fittings.out_of_range(&limits.fittings)),
            tag("pouch", self.pouch.out_of_range(&limits.pouch)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

impl BeltStyle {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(BeltStyle, u32); 5] =
        [(BeltStyle::Plain, 30), (BeltStyle::Studded, 18), (BeltStyle::Sword, 20), (BeltStyle::Girdle, 14), (BeltStyle::Pouch, 18)];
}

impl Generator for Belts {
    type State = BeltState;

    fn roll(seed: u32) -> BeltState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&BeltStyle::WEIGHTS);
        compose(&mut rng, seed, style)
    }

    fn noun(s: &BeltState) -> String {
        let f = &s.fittings;
        match f.style {
            BeltStyle::Plain if s.belt.width > 0.5 => "Wide Belt",
            BeltStyle::Plain => "Belt",
            BeltStyle::Studded => "Studded Belt",
            BeltStyle::Sword if f.frogs > 1 => "Sword and Dagger Belt",
            BeltStyle::Sword => "Sword Belt",
            BeltStyle::Girdle => "Girdle",
            BeltStyle::Pouch => "Pouch Belt",
        }
        .to_string()
    }

    /// The belt buckled as worn, front toward +Z, standing on whatever hangs lowest.
    fn build(s: &BeltState) -> ItemMesh {
        let form = TorsoForm::MANNEQUIN;
        let mut mesh = belt_parts(&form, &s.belt, &s.fittings).finish();
        if s.fittings.pouch {
            let mut pouch = Bags::build(&BagState { seed: s.seed, bag: s.pouch, etch: Etching::default(), cond: s.cond, palette: s.palette });
            if let Some((lo, hi)) = pouch.bounds() {
                let (at, out) = belt_pouch_mount(&form, &s.belt, &s.fittings);
                let rotation = Mat3::from_cols(vec3::cross(vec3::Y, out), vec3::Y, out);
                // Its back on the belt, the loops on its back straddling the belt.
                let anchor = rotation.transform([0.0, lo[1] + (hi[1] - lo[1]) * 0.72, lo[2]]);
                pouch.transform(&rotation, vec3::sub(at, anchor));
            }
            for part in &mut pouch.parts {
                part.name = "pouch";
            }
            mesh.parts.extend(pouch.parts);
        }
        if let Some((lo, _)) = mesh.bounds() {
            mesh.transform(&Mat3::IDENTITY, [0.0, -lo[1], 0.0]);
        }
        mesh
    }
}

impl Belts {
    /// "Forge new" forced to one style, for turntables and tests. Draws the same stream as
    /// [`Generator::roll`], so `roll(s)` equals `roll_as(s, style)` for the style `s` rolls.
    pub fn roll_as(seed: u32, style: BeltStyle) -> BeltState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, style)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, style: BeltStyle) -> BeltState {
    use BeltStyle as S;
    let mut s = BeltState { seed, ..BeltState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    let range = |rng: &mut Mulberry32, (a, b): (f32, f32)| rng.range_f32(a, b);

    let b = &mut s.belt;
    b.on = true;
    b.width = range(rng, match style {
        S::Girdle => (0.15, 0.26),
        S::Sword => (0.4, 0.7),
        S::Studded => (0.35, 0.6),
        _ => (0.25, 0.55),
    });
    b.material = Leather;
    b.buckle = match style {
        S::Girdle => *rng.weighted(&[(Brass, 3), (Silver, 2), (Gold, 1)]),
        _ => *rng.weighted(&[(Iron, 4), (Brass, 3), (Silver, 1)]),
    };

    let f = &mut s.fittings;
    f.style = style;
    f.girth = rng.range_f32(0.92, 1.12);
    f.slant = if style == S::Sword { rng.range_f32(0.15, 0.45) } else { rng.range_f32(0.0, 0.08) };
    f.tail = range(rng, if style == S::Girdle { (1.6, 3.4) } else { (0.25, 0.7) });
    f.studs = match style {
        S::Studded => rng.int(12, 24),
        S::Girdle => rng.int(5, 12),
        S::Sword if rng.chance(0.3) => rng.int(4, 10),
        _ => 0,
    };
    f.frogs = if style == S::Sword { rng.int(1, 2) } else { 0 };
    f.loops = match style {
        S::Pouch => rng.int(2, 4),
        S::Plain if rng.chance(0.3) => rng.int(1, 2),
        _ => 0,
    };
    f.rings = match style {
        S::Pouch => rng.int(1, 3),
        S::Sword | S::Plain if rng.chance(0.25) => 1,
        _ => 0,
    };
    f.keeper = style == S::Girdle || rng.chance(0.75);
    f.tip = style == S::Girdle || rng.chance(if style == S::Studded { 0.6 } else { 0.3 });
    f.pouch = rng.chance(match style {
        S::Pouch => 0.9,
        S::Girdle => 0.25,
        _ => 0.4,
    });
    s.pouch = Bags::roll_as(seed ^ 0x0b17_5eed, BagStyle::BeltPouch).bag;

    let targets: &[(EtchTarget, u32)] = &[(EtchTarget::Leather, 2), (EtchTarget::Metals, 1)];
    s.etch = Etching::roll(rng, if style == S::Girdle { 0.35 } else { 0.15 }, targets, 1);
    s.fit(&LIMITS);
    s
}
