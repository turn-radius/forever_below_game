//! Boots and greaves: turnshoes, boots, poulaines, leg wraps, and sabatons with greaves,
//! drawn as a right foot in profile on the implied mannequin.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (`SECTIONS`
//! `boot` and `hem`, `PRESETS.feet`, the `fam==='feet'` branch of `randomize()`, and
//! `forgeNoun()`). Geometry is [`crate::build::boot`] on [`LegForm::mannequin`].
//!
//! The roll follows the prototype's per-style heights, toes, cuffs and straps; it also lets
//! low shoes take a trimmed rim, boots a wooden sole now and then, and a fifth of boots
//! come only to the ankle (the prototype's boots never do).

use crate::build::boot::boot_parts;
use crate::build::form::LegForm;
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::fabric::{Fabric, FabricLimits, FabricPattern};
use crate::families::Generator;
use crate::geom::ItemMesh;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::boot::{Boot, BootLimits, BootStyle};
use crate::sections::hem::{Hem, HemLimits};
use crate::slider::Sliders;
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Brass, Iron, Leather, Silver, Wood};

/// The boots and greaves generator.
pub struct Feet;

/// Every parameter of one pair of boots (one foot is drawn).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeetState {
    pub seed: u32,
    pub boot: Boot,
    pub hem: Hem,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for FeetState {
    /// The prototype's `BASE` boot, with the hem off as its boot presets have it.
    fn default() -> Self {
        FeetState {
            seed: 0,
            boot: Boot::default(),
            hem: Hem { on: false, ..Hem::default() },
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every feet section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeetLimits {
    pub boot: BootLimits,
    pub hem: HemLimits,
    pub trim: FabricLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges (it has no `RANGES.feet`).
pub const LIMITS: FeetLimits = FeetLimits {
    boot: BootLimits::SLIDERS,
    hem: HemLimits::SLIDERS,
    trim: FabricLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl FeetState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &FeetLimits) {
        self.boot.fit(&limits.boot);
        self.hem.fit(&limits.hem);
        self.hem.fabric.fit(&limits.trim);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    fn nudge(&mut self, rng: &mut Mulberry32, limits: &FeetLimits, odds: f64, spread: f32) {
        self.boot.nudge(rng, &limits.boot, odds, spread);
        self.hem.nudge(rng, &limits.hem, odds, spread);
        self.hem.fabric.nudge(rng, &limits.trim, odds, spread);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &FeetLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("boot", self.boot.out_of_range(&limits.boot)),
            tag("hem", self.hem.out_of_range(&limits.hem)),
            tag("hem.fabric", self.hem.fabric.out_of_range(&limits.trim)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

impl BootStyle {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(BootStyle, u32); 5] = [
        (BootStyle::Shoe, 17),
        (BootStyle::Boot, 33),
        (BootStyle::Poulaine, 14),
        (BootStyle::Wrapped, 14),
        (BootStyle::Sabaton, 22),
    ];
}

impl Generator for Feet {
    type State = FeetState;

    fn roll(seed: u32) -> FeetState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&BootStyle::WEIGHTS);
        compose(&mut rng, seed, style)
    }

    /// `forgeNoun()` for footwear.
    fn noun(s: &FeetState) -> String {
        let b = &s.boot;
        match b.style {
            BootStyle::Shoe => "Turnshoes",
            BootStyle::Boot if b.height > 3.0 => "Riding Boots",
            BootStyle::Boot if b.height > 1.2 => "Buckled Boots",
            BootStyle::Boot => "Ankle Boots",
            BootStyle::Poulaine => "Poulaines",
            BootStyle::Wrapped => "Leg Wraps",
            BootStyle::Sabaton if b.height > 2.5 => "Greaves and Sabatons",
            BootStyle::Sabaton => "Sabatons",
        }
        .to_string()
    }

    /// A right foot in profile, toe toward +X, standing on its sole or heel at y = 0.
    fn build(s: &FeetState) -> ItemMesh {
        let b = &s.boot;
        let toe = if b.style == BootStyle::Poulaine || b.toe > 0.05 { b.toe } else { 0.0 };
        let form = LegForm::mannequin(b.size, b.width, toe);
        let mut parts = boot_parts(&form, b, &s.hem);
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

impl Feet {
    /// "Forge new" forced to one style, for turntables and tests. Draws the same stream as
    /// [`Generator::roll`], so `roll(s)` equals `roll_as(s, style)` for the style `s` rolls.
    pub fn roll_as(seed: u32, style: BootStyle) -> FeetState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, style)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, style: BootStyle) -> FeetState {
    use BootStyle as S;
    let mut s = FeetState { seed, ..FeetState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    s.nudge(rng, &LIMITS, 0.75, 0.3);

    let b = &mut s.boot;
    b.style = style;
    b.size = rng.range_f32(0.92, 1.08);
    b.width = rng.range_f32(0.88, 1.12);
    let (lo, hi) = match style {
        S::Shoe => (0.3, 0.6),
        S::Boot => (1.4, 4.4),
        S::Poulaine => (0.4, 0.8),
        S::Wrapped => (2.4, 4.2),
        S::Sabaton => (2.0, 4.4),
    };
    b.height = rng.range_f32(lo, hi);
    // The prototype's boots never come lower than buckled boots; some are ankle boots here.
    if style == S::Boot && rng.chance(0.2) {
        b.height = rng.range_f32(0.8, 1.2);
    }
    b.toe = match style {
        S::Poulaine => rng.range_f32(0.5, 1.0),
        S::Sabaton => rng.range_f32(0.0, 0.5),
        _ => 0.0,
    };
    b.cuff = if style == S::Boot && rng.chance(0.5) { rng.range_f32(0.25, 0.65) } else { 0.0 };
    b.straps = match style {
        S::Boot => rng.int(0, 3),
        S::Shoe | S::Poulaine => 1,
        _ => 0,
    };
    b.lames = rng.int(4, 7);
    b.knee = style == S::Sabaton && rng.chance(0.7);
    b.material = Leather;
    b.sole = *rng.weighted(&[(Leather, 5), (Wood, 1)]);
    b.plate = *rng.weighted(&[(Iron, 4), (Brass, 1), (Silver, 1)]);
    b.leg_cloth = *rng.pick(&Boot::LEG_CLOTHS);
    b.wrap_cloth = *rng.pick(&Boot::WRAP_CLOTHS);

    s.hem.on = matches!(style, S::Shoe | S::Poulaine) && rng.chance(0.35) || style == S::Boot && rng.chance(0.1);
    s.hem.width = rng.range_f32(0.12, 0.3);
    s.hem.fabric = Fabric { pattern: FabricPattern::Plain, ..Fabric::plain(*rng.pick(&Fabric::TRIM_DYES)) };
    s.hem.hemline = true;
    s.hem.neck = false;
    s.hem.cuffs = false;
    s.hem.front = false;

    let targets = if style == S::Sabaton { vec![(EtchTarget::Metals, 3), (EtchTarget::Leather, 1)] } else { vec![(EtchTarget::Leather, 1)] };
    s.etch = Etching::roll(rng, if style == S::Sabaton { 0.3 } else { 0.15 }, &targets, 1);

    s.fit(&LIMITS);
    s
}
