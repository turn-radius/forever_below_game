//! Trousers, breeches and leggings, drawn on the implied mannequin from the soles to the
//! waist (the feet are not drawn).
//!
//! The Armory prototype has no legwear family: the plate prototype fakes its `Trousers`,
//! `Breeches` and `Leggings` bases (`data/bases.json`) as a tube swept down each leg of the
//! body. The sliders, ranges, roll and nouns here are this port's own. Geometry is
//! [`crate::build::legwear`] on [`LowerBodyForm::mannequin`].
//!
//! The roll picks a style, then sizes the ease, taper and folds to it: trousers range from
//! close and tapered to baggy and straight, breeches always balloon over the thigh, leggings
//! cling. Puttees come with trousers and breeches, turned cuffs and knee patches with
//! trousers only.

use crate::build::form::LowerBodyForm;
use crate::build::legwear::legwear_parts;
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::fabric::{Fabric, FabricLimits, FabricPattern};
use crate::families::Generator;
use crate::geom::ItemMesh;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::hem::{Hem, HemLimits};
use crate::sections::legwear::{Legwear, LegwearLimits, LegwearStyle, WaistClosure};
use crate::slider::Sliders;
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Bone, Brass, Iron, Wood};

/// The legwear generator.
pub struct Legs;

/// Every parameter of one pair of trousers, breeches or leggings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LegsState {
    pub seed: u32,
    pub legwear: Legwear,
    /// Trim along the hems.
    pub hem: Hem,
    /// Embroidery on the cloth or tooling on the leather.
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for LegsState {
    fn default() -> Self {
        LegsState {
            seed: 0,
            legwear: Legwear::default(),
            hem: Hem { on: false, neck: false, cuffs: false, front: false, ..Hem::default() },
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every legwear section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LegsLimits {
    pub legwear: LegwearLimits,
    pub cloth: FabricLimits,
    pub hem: HemLimits,
    pub trim: FabricLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// Full ranges; the trim is narrower than a garment's.
pub const LIMITS: LegsLimits = LegsLimits {
    legwear: LegwearLimits::SLIDERS,
    cloth: FabricLimits::SLIDERS,
    hem: HemLimits { width: HemLimits::SLIDERS.width.narrowed(0.1, 0.5) },
    trim: FabricLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl LegsState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &LegsLimits) {
        self.legwear.fit(&limits.legwear);
        self.legwear.cloth.fit(&limits.cloth);
        self.hem.fit(&limits.hem);
        self.hem.fabric.fit(&limits.trim);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &LegsLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("legwear", self.legwear.out_of_range(&limits.legwear)),
            tag("legwear.cloth", self.legwear.cloth.out_of_range(&limits.cloth)),
            tag("hem", self.hem.out_of_range(&limits.hem)),
            tag("hem.fabric", self.hem.fabric.out_of_range(&limits.trim)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

impl LegwearStyle {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(LegwearStyle, u32); 3] = [(LegwearStyle::Trousers, 50), (LegwearStyle::Breeches, 28), (LegwearStyle::Leggings, 22)];
}

impl Generator for Legs {
    type State = LegsState;

    fn roll(seed: u32) -> LegsState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&LegwearStyle::WEIGHTS);
        compose(&mut rng, seed, style)
    }

    fn noun(s: &LegsState) -> String {
        let w = &s.legwear;
        match w.style {
            LegwearStyle::Trousers if w.puttees => "Wrapped Trousers",
            LegwearStyle::Trousers if w.ease > 0.7 && w.taper < 0.4 => "Wide Trousers",
            LegwearStyle::Trousers if w.cuff > 0.0 => "Cuffed Trousers",
            LegwearStyle::Trousers => "Trousers",
            LegwearStyle::Breeches if w.puttees => "Breeches and Puttees",
            LegwearStyle::Breeches => "Breeches",
            LegwearStyle::Leggings if w.cloth.pattern != FabricPattern::Plain => "Hose",
            LegwearStyle::Leggings => "Leggings",
        }
        .to_string()
    }

    /// Both legs standing, front toward +Z, the hems on y = 0.
    fn build(s: &LegsState) -> ItemMesh {
        let form = LowerBodyForm::mannequin();
        let mut parts = legwear_parts(&form, &s.legwear, &s.hem, s.seed);
        let mut floor = f32::MAX;
        for m in parts.meshes_mut() {
            floor = floor.min(m.positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min));
        }
        for m in parts.meshes_mut() {
            m.translate([0.0, -floor, 0.0]);
        }
        parts.finish()
    }
}

impl Legs {
    /// "Forge new" forced to one style, for turntables and tests. Draws the same stream as
    /// [`Generator::roll`], so `roll(s)` equals `roll_as(s, style)` for the style `s` rolls.
    pub fn roll_as(seed: u32, style: LegwearStyle) -> LegsState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, style)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, style: LegwearStyle) -> LegsState {
    use LegwearStyle as S;
    let mut s = LegsState { seed, ..LegsState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);

    let w = &mut s.legwear;
    w.style = style;
    let range = |rng: &mut Mulberry32, (a, b): (f32, f32)| rng.range_f32(a, b);
    w.ease = range(rng, match style {
        S::Trousers => (0.1, 1.0),
        S::Breeches => (0.4, 1.0),
        S::Leggings => (0.0, 0.4),
    });
    w.taper = range(rng, match style {
        S::Trousers => (0.0, 1.0),
        _ => (0.5, 1.0),
    });
    w.folds = range(rng, match style {
        S::Leggings => (0.1, 0.5),
        _ => (0.3, 1.0),
    });
    w.band = rng.range_f32(0.2, 0.45);
    w.closure = *rng.weighted(&[(WaistClosure::Drawstring, 3), (WaistClosure::Laces, 2), (WaistClosure::Toggles, 1)]);
    w.puttees = style != S::Leggings && rng.chance(if style == S::Breeches { 0.35 } else { 0.2 });
    w.cuff = if style == S::Trousers && !w.puttees && rng.chance(0.3) { rng.range_f32(0.2, 0.5) } else { 0.0 };
    w.patches = style == S::Trousers && rng.chance(0.25);
    w.cloth.color = *rng.pick(&Legwear::DYES);
    let others: Vec<_> = Fabric::PATTERN_DYES.into_iter().filter(|c| *c != w.cloth.color).collect();
    w.cloth.color2 = *rng.pick(&others);
    w.cloth.pattern = match style {
        // Particoloured and striped hose.
        S::Leggings => *rng.weighted(&[(FabricPattern::Plain, 5), (FabricPattern::Hoops, 2), (FabricPattern::Stripes, 1), (FabricPattern::Checks, 1)]),
        _ => *rng.weighted(&[(FabricPattern::Plain, 10), (FabricPattern::Stripes, 1), (FabricPattern::Checks, 1), (FabricPattern::Tartan, 1)]),
    };
    w.cloth.scale = rng.range_f32(2.0, 5.0);
    w.wrap_cloth = *rng.pick(&Legwear::WRAP_CLOTHS);
    w.fittings = *rng.weighted(&[(Wood, 3), (Bone, 2), (Iron, 2), (Brass, 1)]);

    s.hem.on = style != S::Leggings && rng.chance(0.2);
    s.hem.width = rng.range_f32(0.15, 0.35);
    s.hem.fabric = Fabric::plain(*rng.pick(&Fabric::TRIM_DYES));
    s.hem.hemline = true;

    s.etch = Etching::roll(rng, 0.08, &[(EtchTarget::Cloth, 1)], 1);
    s.fit(&LIMITS);
    s
}
