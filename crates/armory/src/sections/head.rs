//! The head end of a mace or staff: the head body, the collar under it, flanges, spikes,
//! prongs, a finial on top, and a set stone. Ranges are the prototype's `SECTIONS`;
//! families narrow them.

use crate::gem::{GemCut, Gemstone};
use crate::material::MaterialKind;
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

/// The lathe profile of the head body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadStyle {
    Orb,
    Pear,
    Drum,
    Onion,
    /// Open on top, to cradle a stone.
    Cup,
    /// No head body: the shaft ends in its fittings (staves).
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Head {
    pub style: HeadStyle,
    pub length: f32,
    pub radius: f32,
    /// Height of the widest point, as a fraction of `length`.
    pub belly: f32,
    pub facets: u32,
    pub material: MaterialKind,
}

sliders! {
    HeadLimits for Head {
        length: 0.2, 3.2, 0.01;
        radius: 0.1, 1.3, 0.01;
        belly: 0.15, 0.85, 0.01;
        facets: 3.0, 32.0, 1.0;
    }
}

impl Default for Head {
    fn default() -> Self {
        Head { style: HeadStyle::Orb, length: 1.4, radius: 0.45, belly: 0.5, facets: 16, material: MaterialKind::Iron }
    }
}

/// Raised bands where the head meets the shaft.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Collar {
    pub on: bool,
    pub bands: u32,
    /// Height of each band.
    pub height: f32,
    /// Multiplier on the shaft radius.
    pub radius: f32,
    pub facets: u32,
    pub material: MaterialKind,
}

sliders! {
    CollarLimits for Collar {
        bands: 1.0, 6.0, 1.0;
        height: 0.03, 0.5, 0.005;
        radius: 1.0, 3.0, 0.01;
        facets: 3.0, 32.0, 1.0;
    }
}

impl Default for Collar {
    fn default() -> Self {
        Collar { on: true, bands: 2, height: 0.12, radius: 1.45, facets: 16, material: MaterialKind::Iron }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlangeShape {
    Blade,
    Crescent,
    Stepped,
    Leaf,
    Fan,
}

/// Radial blades around the head.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flanges {
    pub on: bool,
    pub count: u32,
    /// Rotation of the first flange, degrees.
    pub phase: f32,
    /// Fraction of the head length they span.
    pub height: f32,
    /// Shift of their centre along the head, fraction of head length.
    pub bias: f32,
    pub shape: FlangeShape,
    /// How far they reach out from the head.
    pub depth: f32,
    pub thickness: f32,
    /// Depth of the notch cut into the outer edge.
    pub notch: f32,
    /// Pierced with a window.
    pub pierce: bool,
    /// Lean out of the radial plane, degrees.
    pub lean: f32,
    pub material: MaterialKind,
}

sliders! {
    FlangesLimits for Flanges {
        count: 2.0, 12.0, 1.0;
        phase: 0.0, 360.0, 1.0;
        height: 0.2, 1.0, 0.01;
        bias: -0.4, 0.4, 0.01;
        depth: 0.05, 1.4, 0.01;
        thickness: 0.015, 0.25, 0.005;
        notch: 0.0, 1.0, 0.01;
        lean: -35.0, 35.0, 1.0;
    }
}

impl Default for Flanges {
    fn default() -> Self {
        Flanges {
            on: false,
            count: 6,
            phase: 0.0,
            height: 0.9,
            bias: 0.0,
            shape: FlangeShape::Blade,
            depth: 0.5,
            thickness: 0.06,
            notch: 0.0,
            pierce: false,
            lean: 0.0,
            material: MaterialKind::Iron,
        }
    }
}

/// How spikes are placed around each ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpikePattern {
    /// Every ring lines up.
    Aligned,
    /// Alternate rings offset by half a step.
    Staggered,
    /// Each ring turned by the twist.
    Spiral,
    /// Spikes placed by the golden angle, no rings.
    GoldenAngle,
}

/// Which half of the head carries spikes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hemisphere {
    Both,
    Upper,
    Lower,
}

/// Spikes in rings around the head's centre plane.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spikes {
    pub on: bool,
    /// Spikes per ring.
    pub count: u32,
    pub rings: u32,
    pub pattern: SpikePattern,
    /// Vertical spread of the rings over the head.
    pub spread: f32,
    /// Offset of the centre plane, fraction of head length.
    pub bias: f32,
    pub hemisphere: Hemisphere,
    /// Turn per ring, degrees.
    pub twist: f32,
    /// Degrees.
    pub phase: f32,
    pub jitter: f32,
    pub length: f32,
    /// Radius at the root.
    pub base: f32,
    pub sides: u32,
    /// How far the flanks curve in.
    pub concave: f32,
    /// Degrees.
    pub tilt: f32,
    /// How much smaller spikes get away from the centre plane.
    pub falloff: f32,
    pub material: MaterialKind,
}

sliders! {
    SpikesLimits for Spikes {
        count: 1.0, 16.0, 1.0;
        rings: 1.0, 7.0, 1.0;
        spread: 0.0, 1.0, 0.01;
        bias: -0.5, 0.5, 0.01;
        twist: -90.0, 90.0, 1.0;
        phase: 0.0, 360.0, 1.0;
        jitter: 0.0, 1.0, 0.01;
        length: 0.05, 1.4, 0.01;
        base: 0.02, 0.35, 0.005;
        sides: 3.0, 16.0, 1.0;
        concave: 0.0, 1.0, 0.01;
        tilt: -60.0, 60.0, 1.0;
        falloff: 0.0, 1.0, 0.01;
    }
}

impl Default for Spikes {
    fn default() -> Self {
        Spikes {
            on: false,
            count: 7,
            rings: 3,
            pattern: SpikePattern::Staggered,
            spread: 0.72,
            bias: 0.0,
            hemisphere: Hemisphere::Both,
            twist: 0.0,
            phase: 0.0,
            jitter: 0.0,
            length: 0.4,
            base: 0.1,
            sides: 6,
            concave: 0.3,
            tilt: 0.0,
            falloff: 0.25,
            material: MaterialKind::Iron,
        }
    }
}

/// Curling arms rising from the head, usually cradling a stone.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prongs {
    pub on: bool,
    pub count: u32,
    pub height: f32,
    pub spread: f32,
    pub curl: f32,
    pub thickness: f32,
    /// Turns about the axis over the prong's height.
    pub twist: f32,
    /// Root circle, multiplier on the head radius.
    pub base: f32,
    /// Degrees.
    pub phase: f32,
    pub material: MaterialKind,
}

sliders! {
    ProngsLimits for Prongs {
        count: 2.0, 9.0, 1.0;
        height: 0.3, 3.5, 0.05;
        spread: 0.05, 1.4, 0.01;
        curl: 0.0, 1.0, 0.01;
        thickness: 0.015, 0.16, 0.005;
        twist: -1.0, 1.0, 0.01;
        base: 0.4, 1.4, 0.01;
        phase: 0.0, 360.0, 1.0;
    }
}

impl Default for Prongs {
    fn default() -> Self {
        Prongs {
            on: false,
            count: 4,
            height: 1.6,
            spread: 0.5,
            curl: 0.65,
            thickness: 0.06,
            twist: 0.12,
            base: 0.9,
            phase: 45.0,
            material: MaterialKind::Wood,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinialStyle {
    None,
    Spike,
    Knob,
    Spire,
}

/// The ornament on top of the head.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finial {
    pub style: FinialStyle,
    pub length: f32,
    pub radius: f32,
    pub sides: u32,
    pub material: MaterialKind,
}

sliders! {
    FinialLimits for Finial {
        length: 0.1, 1.6, 0.01;
        radius: 0.04, 0.4, 0.005;
        sides: 3.0, 24.0, 1.0;
    }
}

impl Default for Finial {
    fn default() -> Self {
        Finial { style: FinialStyle::None, length: 0.45, radius: 0.1, sides: 8, material: MaterialKind::Iron }
    }
}

/// A stone set at the top of the head (the prototype's "Head gem").
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeadGem {
    pub on: bool,
    pub cut: GemCut,
    pub size: f32,
    pub stone: Gemstone,
    /// Height in the prong cradle, 0..=1 (staves).
    pub lift: f32,
    /// Emissive strength, 0..=1.
    pub glow: f32,
    /// Floats and turns above the head (staves).
    pub bob: bool,
}

sliders! {
    HeadGemLimits for HeadGem {
        size: 0.08, 0.9, 0.01;
        lift: 0.0, 1.0, 0.01;
        glow: 0.0, 1.0, 0.01;
    }
}

impl Default for HeadGem {
    fn default() -> Self {
        HeadGem { on: false, cut: GemCut::Brilliant, size: 0.34, stone: Gemstone::Amethyst, lift: 0.55, glow: 0.5, bob: true }
    }
}
