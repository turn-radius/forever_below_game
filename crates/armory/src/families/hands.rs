//! Gloves and gauntlets: leather gloves, fingered and hourglass gauntlets, mail mittens and
//! archer's bracers, drawn as a right hand on the implied mannequin.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (`SECTIONS`
//! `hand` and `hem`, `PRESETS.hands`, the `fam==='hands'` branch of `randomize()`, and
//! `forgeNoun()`). Geometry is [`crate::build::hand`] on [`HandForm::mannequin`].
//!
//! The roll sizes the cuff, flare and curl per style (a bracer is long and straight, an
//! hourglass cuff bells wide) rather than nudging them blindly, so every style reads as
//! itself.

use crate::build::form::HandForm;
use crate::build::hand::hand_parts;
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::fabric::{Fabric, FabricLimits, FabricPattern};
use crate::families::Generator;
use crate::geom::ItemMesh;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::hand::{Hand, HandLimits, HandStyle};
use crate::sections::hem::{Hem, HemLimits};
use crate::slider::Sliders;
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Brass, Cord, Iron, Leather, Silver};

/// The gloves and gauntlets generator.
pub struct Hands;

/// Every parameter of one pair of gloves (one hand is drawn).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandsState {
    pub seed: u32,
    pub hand: Hand,
    pub hem: Hem,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for HandsState {
    /// The prototype's `BASE` hand, with the hem off as its gauntlet presets have it.
    fn default() -> Self {
        HandsState {
            seed: 0,
            hand: Hand::default(),
            hem: Hem { on: false, ..Hem::default() },
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every hands section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandsLimits {
    pub hand: HandLimits,
    pub hem: HemLimits,
    pub trim: FabricLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges (it has no `RANGES.hands`).
pub const LIMITS: HandsLimits = HandsLimits {
    hand: HandLimits::SLIDERS,
    hem: HemLimits::SLIDERS,
    trim: FabricLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl HandsState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &HandsLimits) {
        self.hand.fit(&limits.hand);
        self.hem.fit(&limits.hem);
        self.hem.fabric.fit(&limits.trim);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    fn nudge(&mut self, rng: &mut Mulberry32, limits: &HandsLimits, odds: f64, spread: f32) {
        self.hand.nudge(rng, &limits.hand, odds, spread);
        self.hem.nudge(rng, &limits.hem, odds, spread);
        self.hem.fabric.nudge(rng, &limits.trim, odds, spread);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &HandsLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("hand", self.hand.out_of_range(&limits.hand)),
            tag("hem", self.hem.out_of_range(&limits.hem)),
            tag("hem.fabric", self.hem.fabric.out_of_range(&limits.trim)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

impl HandStyle {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(HandStyle, u32); 5] = [
        (HandStyle::Glove, 24),
        (HandStyle::Gauntlet, 30),
        (HandStyle::Hourglass, 15),
        (HandStyle::Mitten, 14),
        (HandStyle::Bracer, 17),
    ];
}

impl Generator for Hands {
    type State = HandsState;

    fn roll(seed: u32) -> HandsState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&HandStyle::WEIGHTS);
        compose(&mut rng, seed, style)
    }

    /// `forgeNoun()` for gloves.
    fn noun(s: &HandsState) -> String {
        let h = &s.hand;
        match h.style {
            HandStyle::Glove if h.flare > 0.4 => "Riding Gloves",
            HandStyle::Glove => "Leather Gloves",
            HandStyle::Gauntlet if h.gadlings => "Spiked Gauntlets",
            HandStyle::Gauntlet => "Gauntlets",
            HandStyle::Hourglass => "Hourglass Gauntlets",
            HandStyle::Mitten => "Mail Mittens",
            HandStyle::Bracer => "Archer's Bracer",
        }
        .to_string()
    }

    /// A right hand, back toward +Z, fingers up, standing on the open end of its cuff
    /// (the wrist is at y = cuff length).
    fn build(s: &HandsState) -> ItemMesh {
        let form = HandForm::mannequin(s.hand.size, s.hand.spread);
        let mut parts = hand_parts(&form, &s.hand, &s.hem);
        let lift = s.hand.cuff * s.hand.size;
        for m in parts.meshes_mut() {
            m.translate([0.0, lift, 0.0]);
        }
        parts.finish()
    }
}

impl Hands {
    /// "Forge new" forced to one style, for turntables and tests. Draws the same stream as
    /// [`Generator::roll`], so `roll(s)` equals `roll_as(s, style)` for the style `s` rolls.
    pub fn roll_as(seed: u32, style: HandStyle) -> HandsState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, style)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, style: HandStyle) -> HandsState {
    use HandStyle as S;
    let mut s = HandsState { seed, ..HandsState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    s.nudge(rng, &LIMITS, 0.75, 0.3);

    let h = &mut s.hand;
    h.style = style;
    let range = |rng: &mut Mulberry32, (a, b): (f32, f32)| rng.range_f32(a, b);
    h.size = rng.range_f32(0.92, 1.08);
    h.curl = range(rng, if style == S::Bracer { (0.1, 0.3) } else { (0.15, 0.45) });
    h.spread = rng.range_f32(0.1, 0.4);
    h.cuff = range(rng, match style {
        S::Glove => (0.6, 1.4),
        S::Gauntlet => (1.0, 1.6),
        S::Hourglass => (1.1, 1.6),
        S::Mitten => (0.8, 1.2),
        S::Bracer => (1.8, 2.6),
    });
    h.flare = range(rng, match style {
        S::Glove => (0.15, 0.8),
        S::Gauntlet => (0.4, 0.8),
        S::Hourglass => (0.7, 1.0),
        S::Mitten => (0.15, 0.35),
        S::Bracer => (0.05, 0.25),
    });
    h.lames = rng.int(3, 5);
    h.gadlings = style == S::Gauntlet && rng.chance(0.3);
    h.material = *rng.weighted(&[(Leather, 5), (Cord, 1)]);
    h.plate = *rng.weighted(&[(Iron, 2), (Brass, 1), (Silver, 1)]);

    let soft = matches!(style, S::Glove | S::Mitten);
    s.hem.on = soft && rng.chance(0.6);
    s.hem.width = rng.range_f32(0.12, 0.25);
    s.hem.fabric = Fabric { pattern: FabricPattern::Plain, ..Fabric::plain(*rng.pick(&Fabric::TRIM_DYES)) };
    s.hem.hemline = true;
    s.hem.neck = false;
    s.hem.cuffs = false;
    s.hem.front = false;

    let mut targets = vec![(EtchTarget::Leather, 1)];
    if style.is_plated() {
        targets.push((EtchTarget::Metals, 3));
    }
    s.etch = Etching::roll(rng, if style == S::Bracer { 0.4 } else { 0.25 }, &targets, 1);

    s.fit(&LIMITS);
    s
}
