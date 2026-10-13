//! Etching: engraved, carved, tooled or embroidered decoration over chosen materials.
//! Shared by every family. From the prototype's `etch` section and the etch roll in
//! `randomize()` (`reference/spline_armory_engine.js`).

use crate::material::Rgb;
use crate::slider::sliders;
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EtchPattern {
    Scrollwork,
    BorderBands,
    Knotwork,
    Chevrons,
    DiaperLattice,
    Runes,
}

impl EtchPattern {
    pub const ALL: [EtchPattern; 6] = [
        EtchPattern::Scrollwork, EtchPattern::BorderBands, EtchPattern::Knotwork, EtchPattern::Chevrons,
        EtchPattern::DiaperLattice, EtchPattern::Runes,
    ];
}

/// Which parts get the pattern, by material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EtchTarget {
    /// Every metal part.
    Metals,
    Iron,
    Brass,
    Gold,
    Silver,
    /// Carved.
    Wood,
    /// Carved.
    Bone,
    /// Tooled.
    Leather,
    /// Embroidered.
    Cloth,
}

/// How the engraved lines are filled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EtchFinish {
    Blackened,
    Inlaid,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Etching {
    pub on: bool,
    pub pattern: EtchPattern,
    pub target: EtchTarget,
    pub scale: f32,
    /// Line weight.
    pub weight: f32,
    pub coverage: f32,
    pub finish: EtchFinish,
    /// Colour of the inlay when `finish` is [`EtchFinish::Inlaid`].
    pub inlay: Rgb,
}

sliders! {
    EtchingLimits for Etching {
        scale: 1.0, 16.0, 0.1;
        weight: 0.03, 0.25, 0.005;
        coverage: 0.1, 1.0, 0.01;
    }
}

impl Default for Etching {
    fn default() -> Self {
        Etching {
            on: false,
            pattern: EtchPattern::Scrollwork,
            target: EtchTarget::Metals,
            scale: 4.0,
            weight: 0.08,
            coverage: 1.0,
            finish: EtchFinish::Blackened,
            inlay: Rgb::hex(0xd4a838),
        }
    }
}

impl Etching {
    /// The prototype's etch roll: on with probability `odds`, a uniform pattern, a target
    /// from the weighted `targets` (a family passes only materials it actually has), mostly
    /// blackened, full coverage 60% of the time. `inlaid_weight` against a blackened weight
    /// of 2 sets how often lines are inlaid.
    pub fn roll(rng: &mut Mulberry32, odds: f64, targets: &[(EtchTarget, u32)], inlaid_weight: u32) -> Self {
        let on = rng.chance(odds);
        let pattern = *rng.pick(&EtchPattern::ALL);
        let target = *rng.weighted(targets);
        let finish = *rng.weighted(&[(EtchFinish::Blackened, 2), (EtchFinish::Inlaid, inlaid_weight)]);
        let inlay = *rng.pick(&[Rgb::hex(0xd4a838), Rgb::hex(0xd4a838), Rgb::hex(0xc3c8cc)]);
        let scale = rng.range_f32(3.0, 6.0);
        let weight = rng.range_f32(0.05, 0.11);
        let coverage = if rng.chance(0.6) { 1.0 } else { rng.range_f32(0.4, 0.9) };
        Etching { on, pattern, target, scale, weight, coverage, finish, inlay }
    }
}
