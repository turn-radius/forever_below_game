//! The handle half of a hafted item: shaft, grip, wrapping and pommel. Shared by staves,
//! maces, swords (the shaft is the handle), spears (the haft; the pommel is the butt cap)
//! and daggers. Ranges are the prototype's `SECTIONS`; families narrow them.

use crate::gem::Gemstone;
use crate::material::MaterialKind;
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

/// The haft. Item space runs +Y from the butt.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shaft {
    pub length: f32,
    pub radius: f32,
    /// Top radius over base radius.
    pub taper: f32,
    /// How knotted and bent the wood is.
    pub gnarl: f32,
    /// Knots per unit of length.
    pub gnarl_freq: f32,
    /// Raised rings along the shaft.
    pub bands: u32,
    pub band_size: f32,
    pub material: MaterialKind,
}

sliders! {
    ShaftLimits for Shaft {
        length: 0.8, 20.0, 0.05;
        radius: 0.06, 0.4, 0.005;
        taper: 0.5, 1.6, 0.01;
        gnarl: 0.0, 1.0, 0.01;
        gnarl_freq: 0.5, 8.0, 0.1;
        bands: 0.0, 10.0, 1.0;
        band_size: 0.01, 0.1, 0.005;
    }
}

impl Default for Shaft {
    fn default() -> Self {
        Shaft { length: 5.0, radius: 0.15, taper: 1.08, gnarl: 0.0, gnarl_freq: 3.0, bands: 0, band_size: 0.03, material: MaterialKind::Wood }
    }
}

/// A thicker hand-hold sleeve over the shaft.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Grip {
    pub on: bool,
    /// Where it starts, as a fraction of shaft length from the butt.
    pub position: f32,
    pub length: f32,
    /// Multiplier on the shaft radius.
    pub thickness: f32,
    /// Rivets through the grip scales (swords and daggers).
    pub rivets: u32,
    pub material: MaterialKind,
}

sliders! {
    GripLimits for Grip {
        position: 0.0, 0.85, 0.01;
        length: 0.4, 5.0, 0.05;
        thickness: 1.0, 1.7, 0.01;
        rivets: 0.0, 5.0, 1.0;
    }
}

impl Default for Grip {
    fn default() -> Self {
        Grip { on: true, position: 0.0, length: 1.5, thickness: 1.18, rivets: 0, material: MaterialKind::Wood }
    }
}

/// Cord or strap wound over the grip.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wrap {
    pub on: bool,
    pub turns: u32,
    /// Cord thickness.
    pub cord: f32,
    /// Fraction of the grip covered.
    pub coverage: f32,
    /// Cross-wound (diamond pattern) instead of a single helix.
    pub cross: bool,
    /// Bindings at each end.
    pub bind: bool,
    pub material: MaterialKind,
}

sliders! {
    WrapLimits for Wrap {
        turns: 2.0, 48.0, 1.0;
        cord: 0.008, 0.08, 0.001;
        coverage: 0.2, 1.0, 0.01;
    }
}

impl Default for Wrap {
    fn default() -> Self {
        Wrap { on: true, turns: 14, cord: 0.032, coverage: 0.9, cross: false, bind: true, material: MaterialKind::Leather }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PommelStyle {
    Ball,
    /// A metal sleeve capping the butt.
    Ferrule,
    Flared,
    Disc,
    /// A wheel pommel: a disc with raised faces.
    Wheel,
    /// Faceted, like a scent-bottle stopper.
    ScentStopper,
    Spike,
}

impl PommelStyle {
    /// Styles with a flat face that can carry a ring of stones.
    pub fn has_face(self) -> bool {
        matches!(self, PommelStyle::Disc | PommelStyle::Wheel)
    }
}

/// How stones are set in the pommel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PommelGem {
    None,
    /// One stone in the end.
    EndStone,
    /// A ring of stones around the face.
    Ring,
}

/// The end weight or butt cap.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pommel {
    pub on: bool,
    pub style: PommelStyle,
    pub length: f32,
    pub radius: f32,
    pub facets: u32,
    pub material: MaterialKind,
    pub gem: PommelGem,
    /// Stones in a [`PommelGem::Ring`].
    pub gem_count: u32,
    pub gem_size: f32,
    pub gem_stone: Gemstone,
}

sliders! {
    PommelLimits for Pommel {
        length: 0.15, 1.6, 0.01;
        radius: 0.08, 0.7, 0.005;
        facets: 3.0, 32.0, 1.0;
        gem_count: 2.0, 12.0, 1.0;
        gem_size: 0.02, 0.25, 0.005;
    }
}

impl Default for Pommel {
    fn default() -> Self {
        Pommel {
            on: true,
            style: PommelStyle::Ball,
            length: 0.45,
            radius: 0.24,
            facets: 16,
            material: MaterialKind::Iron,
            gem: PommelGem::None,
            gem_count: 6,
            gem_size: 0.05,
            gem_stone: Gemstone::Garnet,
        }
    }
}
