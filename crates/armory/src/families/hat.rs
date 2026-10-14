//! Hats of felt and wool: pointed hats, wide-brimmed hats and felt caps, drawn on the implied
//! mannequin's head (not drawn).
//!
//! The Armory prototype has no hat family: the plate prototype fakes its `Pointed hat` base
//! (`data/bases.json`) as a torus brim under a swept cone on the body's head. The sliders,
//! ranges, roll and nouns here are this port's own. Geometry is [`crate::build::hat`] on
//! [`HeadForm::mannequin`].
//!
//! Hats are a family of their own rather than hood styles: a hood is one draped sheet from
//! the crown to the shoulders, and none of its controls (cape, face opening, liripipe, ties,
//! ventail, mail) mean anything to a stiff crown and brim, while a hat's (crown height,
//! brim width and set, band, feather) mean nothing to a hood.
//!
//! The roll picks a style and sizes it: pointed hats stand tall and slump at the tip over a
//! modest brim; wide-brimmed hats have a rounded crown, a broad brim that may droop, often a
//! band and sometimes a feather; caps sit low over the crown, some with a turned-up brim, and
//! some fall forward at the top as Phrygian caps.

use crate::build::form::HeadForm;
use crate::build::hat::hat_parts;
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::fabric::{Fabric, FabricLimits, FabricPattern};
use crate::families::Generator;
use crate::geom::ItemMesh;
use crate::material::Rgb;
use crate::palette::Palette;
use crate::sections::hat::{Hat, HatLimits, HatStyle};
use crate::slider::Sliders;
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

/// The hat generator.
pub struct Hats;

/// Every parameter of one hat.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HatState {
    pub seed: u32,
    pub hat: Hat,
    /// Embroidery on the felt.
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every hat section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HatsLimits {
    pub hat: HatLimits,
    pub cloth: FabricLimits,
    pub band: FabricLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

pub const LIMITS: HatsLimits = HatsLimits {
    hat: HatLimits::SLIDERS,
    cloth: FabricLimits::SLIDERS,
    band: FabricLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

/// Felt colours: the garment dyes, with the black, grey and dun of undyed and darkened felt.
const FELT: [Rgb; 10] = [
    Rgb::hex(0x2c3e6b), Rgb::hex(0x5a3d6b), Rgb::hex(0x7a1f24), Rgb::hex(0x4a3a2e), Rgb::hex(0x6a7f3a),
    Rgb::hex(0x8a5a2b), Rgb::hex(0x3f5a5a), Rgb::hex(0x232323), Rgb::hex(0x4a4a52), Rgb::hex(0x8c7a5c),
];

/// Feathers: goose white, crow black, pheasant brown, dyed red and a jay's blue.
const FEATHERS: [Rgb; 5] = [Rgb::hex(0xe8e2d0), Rgb::hex(0x1e1e22), Rgb::hex(0x8a5a32), Rgb::hex(0xa3262a), Rgb::hex(0x3a6ab0)];

impl HatState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &HatsLimits) {
        self.hat.fit(&limits.hat);
        self.hat.cloth.fit(&limits.cloth);
        self.hat.band_cloth.fit(&limits.band);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &HatsLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("hat", self.hat.out_of_range(&limits.hat)),
            tag("hat.cloth", self.hat.cloth.out_of_range(&limits.cloth)),
            tag("hat.band_cloth", self.hat.band_cloth.out_of_range(&limits.band)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

impl HatStyle {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(HatStyle, u32); 3] = [(HatStyle::Pointed, 35), (HatStyle::WideBrim, 35), (HatStyle::Cap, 30)];
}

impl Generator for Hats {
    type State = HatState;

    fn roll(seed: u32) -> HatState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&HatStyle::WEIGHTS);
        compose(&mut rng, seed, style)
    }

    fn noun(s: &HatState) -> String {
        let h = &s.hat;
        match h.style {
            HatStyle::Pointed => "Pointed Hat",
            HatStyle::WideBrim if h.feather => "Plumed Hat",
            HatStyle::WideBrim if h.droop > 0.45 => "Slouch Hat",
            HatStyle::WideBrim => "Wide-brimmed Hat",
            HatStyle::Cap if h.bend > 0.4 => "Phrygian Cap",
            HatStyle::Cap if h.brim > 0.0 => "Turned-brim Cap",
            HatStyle::Cap => "Felt Cap",
        }
        .to_string()
    }

    /// The hat as worn, front toward +Z, standing on its lowest edge at y = 0.
    fn build(s: &HatState) -> ItemMesh {
        let mut parts = hat_parts(&HeadForm::mannequin(1.0), &s.hat, s.seed);
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

impl Hats {
    /// "Forge new" forced to one style, for turntables, bases and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, style)` for the style `s` rolls.
    pub fn roll_as(seed: u32, style: HatStyle) -> HatState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, style)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, style: HatStyle) -> HatState {
    use HatStyle as S;
    let mut s = HatState { seed, ..HatState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);

    let h = &mut s.hat;
    h.style = style;
    let range = |rng: &mut Mulberry32, (a, b): (f32, f32)| rng.range_f32(a, b);
    h.ease = rng.range_f32(0.0, 1.0);
    let phrygian = style == S::Cap && rng.chance(0.35);
    h.crown = range(rng, match style {
        S::Pointed => (2.6, 4.6),
        S::WideBrim => (1.0, 1.8),
        S::Cap if phrygian => (1.8, 2.5),
        S::Cap => (1.0, 1.3),
    });
    h.bend = range(rng, match style {
        S::Pointed => (0.15, 0.9),
        S::WideBrim => (0.0, 0.1),
        S::Cap if phrygian => (0.55, 1.0),
        S::Cap => (0.0, 0.15),
    });
    let turned_up = style == S::Cap && rng.chance(0.5);
    h.brim = match style {
        S::Pointed => rng.range_f32(0.6, 1.4),
        S::WideBrim => rng.range_f32(1.3, 2.5),
        S::Cap if turned_up => rng.range_f32(0.18, 0.4),
        S::Cap => 0.0,
    };
    h.droop = match style {
        S::Pointed => rng.range_f32(-0.05, 0.35),
        S::WideBrim => rng.range_f32(-0.2, 0.75),
        S::Cap if turned_up => rng.range_f32(-1.0, -0.8),
        S::Cap => 0.0,
    };
    h.crumple = rng.range_f32(0.05, 0.8);
    let banded = rng.chance(match style {
        S::Pointed => 0.4,
        S::WideBrim => 0.75,
        S::Cap => 0.15,
    });
    h.band = if banded { rng.range_f32(0.12, 0.3) } else { 0.0 };
    h.feather = banded && rng.chance(if style == S::WideBrim { 0.45 } else { 0.1 });
    h.feather_color = *rng.pick(&FEATHERS);

    h.cloth = Fabric::plain(*rng.pick(&FELT));
    h.cloth.pattern = match style {
        S::Pointed => *rng.weighted(&[(FabricPattern::Plain, 5), (FabricPattern::Powdered, 1)]),
        _ => FabricPattern::Plain,
    };
    let others: Vec<_> = Fabric::PATTERN_DYES.into_iter().filter(|c| *c != h.cloth.color).collect();
    h.cloth.color2 = *rng.pick(&others);
    h.cloth.scale = rng.range_f32(2.0, 5.0);
    h.band_cloth = Fabric::plain(*rng.pick(&Fabric::TRIM_DYES));

    s.etch = Etching::roll(rng, 0.1, &[(EtchTarget::Cloth, 1)], 1);
    s.fit(&LIMITS);
    s
}
