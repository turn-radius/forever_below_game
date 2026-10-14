//! Hoods, cowls and coifs, cloth and mail, drawn on the implied mannequin's head and
//! shoulders (neither is drawn).
//!
//! The Armory prototype has no hood family: the plate prototype fakes its `Hood` and
//! `Mail coif` bases (`data/bases.json`) as offsets of the body's head. The sliders, ranges,
//! roll and nouns here are this port's own. Geometry is [`crate::build::hood`] on
//! [`HeadForm::mannequin`] and [`TorsoForm::MANNEQUIN`].
//!
//! The roll picks a style and sizes it: hoods draw their crown to a point and sometimes a
//! long liripipe, with a cape that may be dagged; cowls are loose and deep; coifs sit close
//! and tie under the chin; mail coifs carry a short camail and sometimes a ventail.

use crate::build::form::{HeadForm, TorsoForm};
use crate::build::hood::hood_parts;
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::fabric::{Fabric, FabricLimits, FabricPattern};
use crate::families::Generator;
use crate::geom::ItemMesh;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::hem::{Hem, HemLimits};
use crate::sections::hood::{Hood, HoodLimits, HoodStyle};
use crate::slider::Sliders;
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Brass, Iron, Silver};

/// The hood generator.
pub struct Hoods;

/// Every parameter of one hood, cowl or coif.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HoodState {
    pub seed: u32,
    pub hood: Hood,
    /// Trim round the cape's edge (`hemline`) and the face opening (`neck`).
    pub hem: Hem,
    /// Embroidery on cloth, engraving on a mail coif's buckle.
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for HoodState {
    fn default() -> Self {
        HoodState {
            seed: 0,
            hood: Hood::default(),
            hem: Hem { on: false, cuffs: false, front: false, ..Hem::default() },
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every hood section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HoodsLimits {
    pub hood: HoodLimits,
    pub cloth: FabricLimits,
    pub hem: HemLimits,
    pub trim: FabricLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// Full ranges; the trim is narrower than a garment's.
pub const LIMITS: HoodsLimits = HoodsLimits {
    hood: HoodLimits::SLIDERS,
    cloth: FabricLimits::SLIDERS,
    hem: HemLimits { width: HemLimits::SLIDERS.width.narrowed(0.1, 0.5) },
    trim: FabricLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl HoodState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &HoodsLimits) {
        self.hood.fit(&limits.hood);
        self.hood.cloth.fit(&limits.cloth);
        self.hem.fit(&limits.hem);
        self.hem.fabric.fit(&limits.trim);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &HoodsLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("hood", self.hood.out_of_range(&limits.hood)),
            tag("hood.cloth", self.hood.cloth.out_of_range(&limits.cloth)),
            tag("hem", self.hem.out_of_range(&limits.hem)),
            tag("hem.fabric", self.hem.fabric.out_of_range(&limits.trim)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

impl HoodStyle {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(HoodStyle, u32); 4] = [(HoodStyle::Hood, 40), (HoodStyle::Cowl, 20), (HoodStyle::Coif, 18), (HoodStyle::MailCoif, 22)];
}

impl Generator for Hoods {
    type State = HoodState;

    fn roll(seed: u32) -> HoodState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&HoodStyle::WEIGHTS);
        compose(&mut rng, seed, style)
    }

    fn noun(s: &HoodState) -> String {
        let h = &s.hood;
        match h.style {
            HoodStyle::Hood if h.liripipe > 1.2 => "Liripipe Hood",
            HoodStyle::Hood if h.dags > 0 => "Dagged Hood",
            HoodStyle::Hood if h.cape > 1.8 => "Hooded Mantle",
            HoodStyle::Hood => "Hood",
            HoodStyle::Cowl => "Cowl",
            HoodStyle::Coif if h.cloth.pattern == FabricPattern::Quilted => "Arming Cap",
            HoodStyle::Coif => "Coif",
            HoodStyle::MailCoif if h.ventail => "Ventailed Mail Coif",
            HoodStyle::MailCoif => "Mail Coif",
        }
        .to_string()
    }

    /// The hood as worn, front toward +Z, standing on its lowest edge at y = 0.
    fn build(s: &HoodState) -> ItemMesh {
        let mut parts = hood_parts(&HeadForm::mannequin(1.0), &TorsoForm::MANNEQUIN, &s.hood, &s.hem, s.seed);
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

impl Hoods {
    /// "Forge new" forced to one style, for turntables and tests. Draws the same stream as
    /// [`Generator::roll`], so `roll(s)` equals `roll_as(s, style)` for the style `s` rolls.
    pub fn roll_as(seed: u32, style: HoodStyle) -> HoodState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, style)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, style: HoodStyle) -> HoodState {
    use HoodStyle as S;
    let mut s = HoodState { seed, ..HoodState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);

    let h = &mut s.hood;
    h.style = style;
    let range = |rng: &mut Mulberry32, (a, b): (f32, f32)| rng.range_f32(a, b);
    h.ease = rng.range_f32(0.0, 1.0);
    h.peak = match style {
        S::Hood => rng.range_f32(0.2, 1.0),
        S::Cowl => rng.range_f32(0.0, 0.25),
        _ => 0.0,
    };
    h.liripipe = if style == S::Hood && h.peak > 0.5 && rng.chance(0.4) { rng.range_f32(2.0, 4.0) } else { 0.0 };
    h.cape = match style {
        S::Hood => {
            let mantle = rng.chance(0.2);
            range(rng, if mantle { (1.9, 3.0) } else { (0.3, 1.4) })
        }
        S::Cowl => rng.range_f32(0.6, 2.2),
        S::Coif => 0.0,
        S::MailCoif => rng.range_f32(0.2, 1.2),
    };
    h.opening = range(rng, match style {
        S::Cowl => (0.6, 1.0),
        S::MailCoif => (0.15, 0.6),
        _ => (0.3, 0.9),
    });
    h.folds = range(rng, match style {
        S::MailCoif => (0.1, 0.4),
        S::Coif => (0.0, 0.2),
        _ => (0.3, 1.0),
    });
    h.dags = if style == S::Hood && rng.chance(0.25) { rng.int(6, 14) } else { 0 };
    h.ties = style == S::Coif && rng.chance(0.8);
    h.ventail = style == S::MailCoif && rng.chance(0.4);
    h.cloth.color = *rng.pick(&Fabric::GARMENT_DYES);
    let others: Vec<_> = Fabric::PATTERN_DYES.into_iter().filter(|c| *c != h.cloth.color).collect();
    h.cloth.color2 = *rng.pick(&others);
    h.cloth.pattern = match style {
        S::Coif => *rng.weighted(&[(FabricPattern::Plain, 3), (FabricPattern::Quilted, 1)]),
        S::MailCoif => FabricPattern::Plain,
        _ => *rng.weighted(&[(FabricPattern::Plain, 8), (FabricPattern::Stripes, 1), (FabricPattern::Checks, 1), (FabricPattern::Powdered, 1)]),
    };
    if h.cloth.pattern == FabricPattern::Quilted {
        h.cloth.color = *rng.pick(&[Fabric::GARMENT_DYES[4], Fabric::PATTERN_DYES[0]]);
        h.cloth.color2 = h.cloth.color;
    }
    h.cloth.scale = rng.range_f32(2.0, 5.0);
    h.material = *rng.weighted(&[(Iron, 5), (Silver, 1), (Brass, 1)]);
    h.ring_size = rng.range_f32(5.0, 10.0);

    s.hem.on = match style {
        S::Hood | S::Cowl => rng.chance(0.3),
        S::Coif => false,
        S::MailCoif => rng.chance(0.4),
    };
    s.hem.width = rng.range_f32(0.15, 0.4);
    s.hem.fabric = Fabric::plain(*rng.pick(&Fabric::TRIM_DYES));
    s.hem.hemline = style != S::MailCoif && rng.chance(0.8);
    s.hem.neck = style == S::MailCoif || rng.chance(0.5);

    let targets: &[(EtchTarget, u32)] = if style == S::MailCoif { &[(EtchTarget::Metals, 1)] } else { &[(EtchTarget::Cloth, 1)] };
    s.etch = Etching::roll(rng, 0.1, targets, 1);
    s.fit(&LIMITS);
    s
}
