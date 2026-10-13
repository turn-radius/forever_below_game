//! The shooting half of a bow or crossbow: the limbs (a crossbow's prod), the string, and the
//! arrow or bolt laid on it. Shared by bows and crossbows. Ranges are the prototype's
//! `SECTIONS` (`limbs`, `string`, `arrow`); families narrow them.

use crate::material::MaterialKind;
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

/// The side profile of the limbs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimbShape {
    /// A simple arc bending toward the string.
    Longbow,
    /// Tips curve away from the string.
    Recurve,
    /// Limbs set back toward the archer at the handle (deflex), tips swept forward (reflex).
    Reflex,
    /// A horn-and-sinew bow: working limbs ending in stiff, angled tip levers ([`Limbs::siyah`]).
    Composite,
}

impl LimbShape {
    pub const ALL: [LimbShape; 4] = [LimbShape::Longbow, LimbShape::Recurve, LimbShape::Reflex, LimbShape::Composite];
}

/// What caps the limb tips where the string loops on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NockStyle {
    /// Grooves cut into the limb itself (self nocks).
    None,
    /// Turned horn or bone tips.
    Horn,
    /// Metal caps.
    Cap,
}

impl NockStyle {
    pub const ALL: [NockStyle; 3] = [NockStyle::None, NockStyle::Horn, NockStyle::Cap];
}

/// Both limbs of a bow, or a crossbow's prod, tip to tip through the handle.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Limbs {
    pub shape: LimbShape,
    /// Tip to tip, measured along the limbs.
    pub length: f32,
    /// Brace height: from the handle to the string at rest.
    pub brace: f32,
    /// Side to side, at the handle.
    pub width: f32,
    /// Back to belly, at the handle.
    pub thickness: f32,
    /// How much narrower and thinner the limbs get toward the tips.
    pub taper: f32,
    /// How strongly the tips curve away from the string.
    pub recurve: f32,
    /// Length of a composite bow's stiff tip levers, as a fraction of each limb.
    pub siyah: f32,
    pub material: MaterialKind,
    pub nocks: NockStyle,
    pub nock_material: MaterialKind,
}

sliders! {
    LimbsLimits for Limbs {
        length: 3.0, 22.0, 0.05;
        brace: 0.3, 3.0, 0.01;
        width: 0.04, 0.4, 0.005;
        thickness: 0.02, 0.25, 0.005;
        taper: 0.0, 1.0, 0.01;
        recurve: 0.0, 1.0, 0.01;
        siyah: 0.0, 0.4, 0.01;
    }
}

impl Default for Limbs {
    fn default() -> Self {
        Limbs {
            shape: LimbShape::Longbow,
            length: 18.0,
            brace: 1.6,
            width: 0.16,
            thickness: 0.12,
            taper: 0.55,
            recurve: 0.0,
            siyah: 0.0,
            material: MaterialKind::Wood,
            nocks: NockStyle::Horn,
            nock_material: MaterialKind::Brass,
        }
    }
}

/// The string, looped over both tips.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bowstring {
    /// Radius of the string.
    pub thickness: f32,
    /// A sleeve of tight thread over the middle, where the arrow nocks.
    pub serving: bool,
}

sliders! {
    BowstringLimits for Bowstring {
        thickness: 0.006, 0.05, 0.001;
    }
}

impl Default for Bowstring {
    fn default() -> Self {
        Bowstring { thickness: 0.016, serving: true }
    }
}

/// The arrowhead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrowHead {
    /// A narrow square-sectioned point for piercing mail.
    Bodkin,
    /// Two wide barbed blades for hunting.
    Broadhead,
    Leaf,
    /// A rounded knob for birds and small game.
    Blunt,
}

impl ArrowHead {
    pub const ALL: [ArrowHead; 4] = [ArrowHead::Bodkin, ArrowHead::Broadhead, ArrowHead::Leaf, ArrowHead::Blunt];
}

/// The outline of each fletching vane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FletchCut {
    Shield,
    Parabolic,
    Triangle,
}

impl FletchCut {
    pub const ALL: [FletchCut; 3] = [FletchCut::Shield, FletchCut::Parabolic, FletchCut::Triangle];
}

/// An arrow (or a crossbow's bolt) nocked on the string.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Arrow {
    pub on: bool,
    /// The shaft, nock end to the head's socket. The head adds `head_length`.
    pub length: f32,
    /// Shaft radius.
    pub radius: f32,
    pub head: ArrowHead,
    pub head_length: f32,
    pub head_width: f32,
    pub material: MaterialKind,
    /// Number of vanes.
    pub fletches: u32,
    pub fletch_cut: FletchCut,
    pub fletch_length: f32,
    pub fletch_height: f32,
}

sliders! {
    ArrowLimits for Arrow {
        length: 1.2, 9.0, 0.05;
        radius: 0.012, 0.06, 0.001;
        head_length: 0.1, 1.0, 0.01;
        head_width: 0.04, 0.5, 0.005;
        fletches: 2.0, 4.0, 1.0;
        fletch_length: 0.2, 1.6, 0.01;
        fletch_height: 0.04, 0.3, 0.005;
    }
}

impl Default for Arrow {
    fn default() -> Self {
        Arrow {
            on: false,
            length: 7.5,
            radius: 0.028,
            head: ArrowHead::Bodkin,
            head_length: 0.4,
            head_width: 0.12,
            material: MaterialKind::Wood,
            fletches: 3,
            fletch_cut: FletchCut::Shield,
            fletch_length: 1.1,
            fletch_height: 0.13,
        }
    }
}
