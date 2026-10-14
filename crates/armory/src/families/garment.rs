//! Body armour and clothing: tunics, robes, gambesons, mail, brigandines, scale coats,
//! cuirasses and cloaks, drawn on the implied mannequin.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (`SECTIONS`
//! `garment`, `hem`, `belt` and `plates`, `PRESETS.garment`, the `fam==='garment'` branch of
//! `randomize()`, and `forgeNoun()`). Geometry is [`crate::build::garment`] on
//! [`TorsoForm::MANNEQUIN`].
//!
//! The roll follows the prototype's per-style choices (cut length, sleeves, neckline, belt,
//! pauldrons), but sizes the drape per style instead of nudging every slider blindly: robes
//! and cloaks flare and fold deeply, mail and scale hang stiffer, plate does not fold.

use crate::build::form::TorsoForm;
use crate::build::garment::garment_parts;
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::fabric::{Fabric, FabricLimits, FabricPattern};
use crate::families::Generator;
use crate::geom::ItemMesh;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::garment::{Belt, BeltLimits, GarmentCut, GarmentCutLimits, GarmentStyle, Neckline, Plates, PlatesLimits, Sleeves};
use crate::sections::hem::{Hem, HemLimits};
use crate::slider::Sliders;
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Brass, Cord, Gold, Iron, Leather, Silver};

/// The garment generator.
pub struct Garment;

/// Every parameter of one garment.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GarmentState {
    pub seed: u32,
    pub cut: GarmentCut,
    pub hem: Hem,
    pub belt: Belt,
    pub plates: Plates,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every garment section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GarmentLimits {
    pub cut: GarmentCutLimits,
    /// The garment's cloth (`garment.patScale`).
    pub cloth: FabricLimits,
    pub hem: HemLimits,
    /// The trim's cloth (`hem.patScale`).
    pub trim: FabricLimits,
    pub belt: BeltLimits,
    pub plates: PlatesLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges (it has no `RANGES.garment`).
pub const LIMITS: GarmentLimits = GarmentLimits {
    cut: GarmentCutLimits::SLIDERS,
    cloth: FabricLimits { scale: FabricLimits::SLIDERS.scale.narrowed(0.5, 6.0) },
    hem: HemLimits::SLIDERS,
    trim: FabricLimits::SLIDERS,
    belt: BeltLimits::SLIDERS,
    plates: PlatesLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl GarmentState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &GarmentLimits) {
        self.cut.fit(&limits.cut);
        self.cut.cloth.fit(&limits.cloth);
        self.hem.fit(&limits.hem);
        self.hem.fabric.fit(&limits.trim);
        self.belt.fit(&limits.belt);
        self.plates.fit(&limits.plates);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    fn nudge(&mut self, rng: &mut Mulberry32, limits: &GarmentLimits, odds: f64, spread: f32) {
        self.cut.nudge(rng, &limits.cut, odds, spread);
        self.cut.cloth.nudge(rng, &limits.cloth, odds, spread);
        self.hem.nudge(rng, &limits.hem, odds, spread);
        self.hem.fabric.nudge(rng, &limits.trim, odds, spread);
        self.belt.nudge(rng, &limits.belt, odds, spread);
        self.plates.nudge(rng, &limits.plates, odds, spread);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &GarmentLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("cut", self.cut.out_of_range(&limits.cut)),
            tag("cut.cloth", self.cut.cloth.out_of_range(&limits.cloth)),
            tag("hem", self.hem.out_of_range(&limits.hem)),
            tag("hem.fabric", self.hem.fabric.out_of_range(&limits.trim)),
            tag("belt", self.belt.out_of_range(&limits.belt)),
            tag("plates", self.plates.out_of_range(&limits.plates)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

impl GarmentStyle {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(GarmentStyle, u32); 8] = [
        (GarmentStyle::Tunic, 16),
        (GarmentStyle::Robe, 13),
        (GarmentStyle::Gambeson, 12),
        (GarmentStyle::Hauberk, 14),
        (GarmentStyle::Brigandine, 9),
        (GarmentStyle::Scale, 10),
        (GarmentStyle::Cuirass, 12),
        (GarmentStyle::Cloak, 14),
    ];
}

impl Generator for Garment {
    type State = GarmentState;

    fn roll(seed: u32) -> GarmentState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&GarmentStyle::WEIGHTS);
        compose(&mut rng, seed, style)
    }

    /// `forgeNoun()` for garments.
    fn noun(s: &GarmentState) -> String {
        let g = &s.cut;
        match g.style {
            GarmentStyle::Tunic if g.length > 4.0 => "Long Tunic",
            GarmentStyle::Tunic => "Tunic",
            GarmentStyle::Robe => "Robe",
            GarmentStyle::Gambeson => "Gambeson",
            GarmentStyle::Hauberk if g.length < 2.0 => "Mail Shirt",
            GarmentStyle::Hauberk => "Mail Hauberk",
            GarmentStyle::Brigandine => "Brigandine",
            GarmentStyle::Scale => "Scale Coat",
            GarmentStyle::Cuirass if s.plates.pauldrons => "Cuirass and Pauldrons",
            GarmentStyle::Cuirass => "Cuirass",
            GarmentStyle::Cloak => "Cloak",
        }
        .to_string()
    }

    /// The garment on the mannequin, standing on its hem: the waist is at y = `length`
    /// (the prototype's layout), front toward +Z.
    fn build(s: &GarmentState) -> ItemMesh {
        let mut parts = garment_parts(&TorsoForm::MANNEQUIN, &s.cut, &s.hem, &s.belt, &s.plates, s.seed);
        let lift = s.cut.length;
        for m in parts.meshes_mut() {
            m.translate([0.0, lift, 0.0]);
        }
        parts.finish()
    }
}

impl Garment {
    /// "Forge new" forced to one style, for turntables and tests. Draws the same stream as
    /// [`Generator::roll`], so `roll(s)` equals `roll_as(s, style)` for the style `s` rolls.
    pub fn roll_as(seed: u32, style: GarmentStyle) -> GarmentState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, style)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, style: GarmentStyle) -> GarmentState {
    use GarmentStyle as S;
    let mut s = GarmentState { seed, ..GarmentState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    s.nudge(rng, &LIMITS, 0.75, 0.3);

    let g = &mut s.cut;
    g.style = style;
    g.cloth.color = *rng.pick(&Fabric::GARMENT_DYES);
    let others: Vec<_> = Fabric::PATTERN_DYES.into_iter().filter(|c| *c != g.cloth.color).collect();
    g.cloth.color2 = *rng.pick(&others);
    g.cloth.pattern = match style {
        S::Brigandine => FabricPattern::Rivets,
        S::Gambeson => FabricPattern::Quilted,
        S::Hauberk | S::Scale | S::Cuirass => FabricPattern::Plain,
        _ => *rng.weighted(&[
            (FabricPattern::Plain, 6),
            (FabricPattern::Stripes, 1),
            (FabricPattern::Hoops, 1),
            (FabricPattern::Checks, 1),
            (FabricPattern::Tartan, 1),
            (FabricPattern::Lozenges, 1),
            (FabricPattern::Powdered, 1),
            (FabricPattern::Chevrons, 1),
        ]),
    };
    g.cloth.scale = rng.range_f32(1.5, 4.0);
    let (lo, hi) = match style {
        S::Tunic => (2.0, 3.5),
        S::Robe => (6.0, 8.5),
        S::Gambeson => (1.8, 3.0),
        S::Hauberk => (1.4, 4.0),
        S::Brigandine => (0.8, 1.8),
        S::Scale => (2.0, 3.5),
        S::Cuirass => (0.8, 2.0),
        S::Cloak => (5.0, 8.0),
    };
    g.length = rng.range_f32(lo, hi);
    // A long tunic now and then.
    if style == S::Tunic && rng.chance(0.2) {
        g.length = rng.range_f32(4.2, 5.5);
    }
    g.sleeves = match style {
        S::Cloak | S::Cuirass | S::Brigandine => Sleeves::None,
        S::Robe | S::Gambeson => Sleeves::Long,
        _ => *rng.weighted(&[(Sleeves::Short, 2), (Sleeves::Long, 2), (Sleeves::None, 1)]),
    };
    g.neck = match style {
        S::Cloak | S::Robe => *rng.weighted(&[(Neckline::Cowl, 3), (Neckline::Round, 2)]),
        S::Gambeson => Neckline::High,
        S::Cuirass => Neckline::Round,
        _ => *rng.weighted(&[(Neckline::Round, 2), (Neckline::High, 1)]),
    };
    g.material = match style {
        S::Scale => *rng.weighted(&[(Brass, 2), (Iron, 2), (Leather, 1)]),
        _ => *rng.weighted(&[(Iron, 4), (Silver, 1), (Brass, 1)]),
    };
    let range = |rng: &mut Mulberry32, (a, b): (f32, f32)| rng.range_f32(a, b);
    g.fit = range(rng, match style {
        S::Cuirass => (1.0, 1.08),
        S::Gambeson | S::Brigandine => (1.08, 1.18),
        S::Cloak => (0.95, 1.1),
        _ => (1.0, 1.15),
    });
    g.flare = range(rng, match style {
        S::Robe => (0.5, 0.85),
        S::Cloak => (0.4, 0.8),
        S::Tunic => (0.2, 0.5),
        S::Gambeson | S::Scale => (0.1, 0.3),
        S::Hauberk => (0.15, 0.4),
        S::Brigandine | S::Cuirass => (0.0, 0.15),
    });
    g.folds = range(rng, match style {
        S::Robe => (0.5, 0.9),
        S::Cloak => (0.6, 1.0),
        S::Tunic => (0.3, 0.7),
        S::Cuirass | S::Brigandine => (0.0, 0.1),
        _ => (0.1, 0.35),
    });
    g.sleeve_flare = range(rng, match style {
        S::Robe => (0.4, 1.0),
        S::Tunic => (0.0, 0.4),
        _ => (0.0, 0.2),
    });
    g.arm_angle = rng.range_f32(20.0, 34.0);

    let cloth_style = !style.is_armour();
    s.hem.on = rng.chance(match style {
        S::Gambeson | S::Cuirass => 0.2,
        _ => 0.7,
    });
    s.hem.width = rng.range_f32(0.15, 0.45);
    s.hem.fabric = Fabric {
        color: *rng.pick(&Fabric::TRIM_DYES),
        pattern: *rng.weighted(&[
            (FabricPattern::Plain, 3),
            (FabricPattern::Chevrons, 1),
            (FabricPattern::Powdered, 1),
            (FabricPattern::Stripes, 1),
            (FabricPattern::Checks, 1),
        ]),
        color2: *rng.pick(&Fabric::PATTERN_DYES),
        scale: rng.range_f32(2.0, 6.0),
    };
    s.hem.hemline = true;
    s.hem.neck = cloth_style && rng.chance(0.6);
    s.hem.cuffs = rng.chance(0.7);
    s.hem.front = matches!(style, S::Robe | S::Cloak | S::Brigandine);

    s.belt.on = !matches!(style, S::Cloak | S::Cuirass | S::Brigandine) && rng.chance(0.6);
    s.belt.width = rng.range_f32(0.2, 0.45);
    s.belt.material = *rng.weighted(&[(Leather, 4), (Cord, 1)]);
    s.belt.buckle = *rng.weighted(&[(Brass, 3), (Iron, 2), (Gold, 1)]);

    s.plates.pauldrons = style == S::Cuirass && rng.chance(0.7) || style == S::Hauberk && rng.chance(0.3);
    s.plates.faulds = if style == S::Cuirass { *rng.weighted(&[(0, 1), (2, 2), (3, 3), (4, 2), (5, 1)]) } else { 0 };
    s.plates.ridge = rng.range_f32(0.3, 0.9);
    s.plates.lames = rng.int(2, 4);
    s.plates.ring_size = match style {
        S::Scale => rng.range_f32(3.5, 6.0),
        _ => rng.range_f32(5.0, 9.0),
    };

    let mut targets = Vec::new();
    if style.is_armour() || s.plates.pauldrons {
        targets.push((EtchTarget::Metals, 3));
    }
    if cloth_style {
        targets.push((EtchTarget::Cloth, 2));
    }
    if s.belt.on && s.belt.material == Leather {
        targets.push((EtchTarget::Leather, 1));
    }
    s.etch = Etching::roll(rng, 0.25, &targets, if style == S::Cuirass { 2 } else { 1 });
    s.etch.scale = rng.range_f32(1.5, 3.5);

    s.fit(&LIMITS);
    s
}
