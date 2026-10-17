//! Loose ammunition: arrows for a bow and bolts for a crossbow, lying on the ground.
//!
//! An arrow is the shared [`Arrow`] section from [`crate::sections::ranged`], built by
//! [`crate::build::arrow`] with a turned nock for the string and three or four vanes. A bolt is
//! the same section, shorter and stouter, with a square butt and two vanes. The prototypes have
//! no loose ammunition (a bow's arrow was never drawn), so the sliders are the section's own.
//!
//! One state, [`AmmoState`], serves both families ([`Arrows`] and [`Bolts`] roll it differently
//! and the item says which it is). The same state loads onto a bow or crossbow
//! ([`crate::families::bow::BowState::load`], [`crate::families::crossbow::CrossbowState::load`])
//! and into a quiver or bolt case ([`crate::families::quiver`]), so the arrow on the string, in
//! the hand and in the quiver are one thing.
//!
//! Item space: lying along +X, nock end toward -X, centred over the origin and resting on the
//! ground (y = 0).

use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use crate::build::arrow::{ArrowMeshes, arrow_meshes, arrow_reach};
use crate::build::parts::Parts;
use crate::cond::{Condition, ConditionLimits};
use crate::families::Generator;
use crate::geom::{ItemMesh, vec3};
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::sections::ranged::{Arrow, ArrowHead, ArrowLimits, FletchCut};
use crate::slider::Sliders;

use MaterialKind::{Bone, Cloth, Iron, Wood};

/// The arrow generator.
pub struct Arrows;

/// The crossbow bolt generator.
pub struct Bolts;

/// Vane colours: undyed goose, and a few dyed feathers.
pub const FLETCHINGS: [Rgb; 6] =
    [Rgb::hex(0xe3ddcc), Rgb::hex(0xf0ece0), Rgb::hex(0x7a1f24), Rgb::hex(0x2c3e6b), Rgb::hex(0x3d3a33), Rgb::hex(0x6a7f3a)];

/// Every parameter of one arrow or bolt. `palette.fletching` is the vanes' colour.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AmmoState {
    pub seed: u32,
    pub arrow: Arrow,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every ammunition section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AmmoLimits {
    pub arrow: ArrowLimits,
    pub cond: ConditionLimits,
}

pub const LIMITS: AmmoLimits = AmmoLimits { arrow: ArrowLimits::SLIDERS, cond: ConditionLimits::SLIDERS };

impl AmmoState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &AmmoLimits) {
        self.arrow.fit(&limits.arrow);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &AmmoLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [tag("arrow", self.arrow.out_of_range(&limits.arrow)), tag("cond", self.cond.out_of_range(&limits.cond))].concat()
    }

    /// The pieces of this ammunition along +Y, nock end at the origin. `nocked` turns a nock
    /// for a bowstring (arrows); a bolt's butt is square.
    pub fn meshes(&self, nocked: bool) -> ArrowMeshes {
        arrow_meshes(&self.arrow, nocked)
    }

    /// Adds the pieces of `meshes` (already placed) to `parts`, with this ammunition's
    /// materials and vane colour.
    pub fn add_to(&self, meshes: ArrowMeshes, parts: &mut Parts) {
        parts.add("shaft", meshes.shaft, self.arrow.material);
        parts.add("nock", meshes.nock, Bone);
        parts.add("head", meshes.head, Iron);
        parts.add_tinted("fletching", meshes.fletching, Cloth, self.palette.fletching);
    }

    /// How far the point reaches from the nock end.
    pub fn reach(&self) -> f32 {
        arrow_reach(&self.arrow)
    }
}

impl Generator for Arrows {
    type State = AmmoState;

    fn roll(seed: u32) -> AmmoState {
        roll_ammo(seed, false)
    }

    fn noun(s: &AmmoState) -> String {
        noun(s, "Arrow")
    }

    fn build(s: &AmmoState) -> ItemMesh {
        build_ammo(s, true)
    }
}

impl Generator for Bolts {
    type State = AmmoState;

    fn roll(seed: u32) -> AmmoState {
        roll_ammo(seed, true)
    }

    fn noun(s: &AmmoState) -> String {
        noun(s, "Bolt")
    }

    fn build(s: &AmmoState) -> ItemMesh {
        build_ammo(s, false)
    }
}

fn noun(s: &AmmoState, what: &str) -> String {
    let head = match s.arrow.head {
        ArrowHead::Bodkin => "Bodkin",
        ArrowHead::Broadhead => "Broadhead",
        ArrowHead::Leaf => "Leaf-headed",
        ArrowHead::Blunt => "Blunt",
    };
    format!("{head} {what}")
}

fn roll_ammo(seed: u32, bolt: bool) -> AmmoState {
    let mut rng = Mulberry32::new(armory_stream_seed(seed));
    let mut s = AmmoState { seed, ..AmmoState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    s.palette.fletching = *rng.weighted(&[(FLETCHINGS[0], 5), (FLETCHINGS[1], 3), (FLETCHINGS[2], 2), (FLETCHINGS[3], 1), (FLETCHINGS[4], 1), (FLETCHINGS[5], 1)]);
    let ar = &mut s.arrow;
    ar.on = true;
    ar.material = Wood;
    if bolt {
        ar.length = rng.range_f32(2.6, 3.6);
        ar.radius = rng.range_f32(0.035, 0.045);
        ar.head = *rng.weighted(&[(ArrowHead::Bodkin, 5), (ArrowHead::Broadhead, 2), (ArrowHead::Blunt, 1), (ArrowHead::Leaf, 1)]);
    } else {
        ar.length = rng.range_f32(6.4, 8.4);
        ar.radius = rng.range_f32(0.022, 0.03);
        ar.head = *rng.weighted(&[(ArrowHead::Bodkin, 4), (ArrowHead::Broadhead, 3), (ArrowHead::Leaf, 2), (ArrowHead::Blunt, 1)]);
    }
    let k = if bolt { 0.85 } else { 1.0 };
    (ar.head_length, ar.head_width) = match ar.head {
        ArrowHead::Bodkin => (rng.range_f32(0.3, 0.45) * k, rng.range_f32(0.09, 0.13) * k),
        ArrowHead::Broadhead => (rng.range_f32(0.38, 0.5) * k, rng.range_f32(0.22, 0.3) * k),
        ArrowHead::Leaf => (rng.range_f32(0.32, 0.45) * k, rng.range_f32(0.14, 0.19) * k),
        ArrowHead::Blunt => (rng.range_f32(0.2, 0.28) * k, rng.range_f32(0.11, 0.15) * k),
    };
    if bolt {
        ar.fletches = 2;
        ar.fletch_cut = *rng.weighted(&[(FletchCut::Triangle, 5), (FletchCut::Shield, 1), (FletchCut::Parabolic, 1)]);
        ar.fletch_length = rng.range_f32(0.5, 0.7);
        ar.fletch_height = rng.range_f32(0.09, 0.12);
    } else {
        ar.fletches = *rng.weighted(&[(3, 8), (4, 1)]);
        ar.fletch_cut = *rng.weighted(&[(FletchCut::Shield, 3), (FletchCut::Parabolic, 3), (FletchCut::Triangle, 1)]);
        ar.fletch_length = rng.range_f32(0.8, 1.2);
        ar.fletch_height = rng.range_f32(0.1, 0.14);
    }
    s.fit(&LIMITS);
    s
}

fn build_ammo(s: &AmmoState, nocked: bool) -> ItemMesh {
    let reach = s.reach();
    let meshes = s.meshes(nocked).placed([-reach * 0.5, 0.0, 0.0], vec3::X, 0.0);
    let mut parts = Parts::new();
    s.add_to(meshes, &mut parts);
    parts.rest_on_ground();
    parts.finish()
}
