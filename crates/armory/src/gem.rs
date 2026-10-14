//! Cut stones, shared by head gems, pommel stones, guard stones, rings, amulets and charms.
//!
//! The prototype stores a stone as a hex colour from `GEMS` and names it after the nearest
//! entry in `GEM_NAMES` (`gemName` in `reference/spline_armory_engine.js`), calling a clear
//! faceted stone a diamond and a clear cabochon a moonstone. Here the stone is the value and
//! the colour comes from it, with Diamond and Moonstone split into their own stones.

use crate::material::Rgb;
use serde::{Deserialize, Serialize};

/// How a stone is cut (the prototype's `cut` select).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GemCut {
    Brilliant,
    Rose,
    Octahedron,
    /// A raw crystal splinter (head gems only in the prototype).
    Shard,
    Cabochon,
    Orb,
}

impl GemCut {
    /// Cut with flat facets, as opposed to polished smooth (cabochon, orb).
    pub fn is_faceted(self) -> bool {
        !matches!(self, GemCut::Cabochon | GemCut::Orb)
    }
}

/// The stone itself, which fixes its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gemstone {
    Garnet,
    Amethyst,
    Turquoise,
    Sapphire,
    Emerald,
    Citrine,
    Ruby,
    Moonstone,
    Diamond,
}

impl Gemstone {
    pub const ALL: [Gemstone; 9] = [
        Gemstone::Garnet, Gemstone::Amethyst, Gemstone::Turquoise, Gemstone::Sapphire, Gemstone::Emerald,
        Gemstone::Citrine, Gemstone::Ruby, Gemstone::Moonstone, Gemstone::Diamond,
    ];

    /// Colour from the prototype's `GEM_NAMES`.
    pub const fn rgb(self) -> Rgb {
        match self {
            Gemstone::Garnet => Rgb::hex(0xa8323a),
            Gemstone::Amethyst => Rgb::hex(0x6d4bb3),
            Gemstone::Turquoise => Rgb::hex(0x2f8f86),
            Gemstone::Sapphire => Rgb::hex(0x3c6fb4),
            Gemstone::Emerald => Rgb::hex(0x4c9a52),
            Gemstone::Citrine => Rgb::hex(0xc58a23),
            Gemstone::Ruby => Rgb::hex(0x9b2d3f),
            Gemstone::Moonstone | Gemstone::Diamond => Rgb::hex(0xd9dfe6),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Gemstone::Garnet => "Garnet",
            Gemstone::Amethyst => "Amethyst",
            Gemstone::Turquoise => "Turquoise",
            Gemstone::Sapphire => "Sapphire",
            Gemstone::Emerald => "Emerald",
            Gemstone::Citrine => "Citrine",
            Gemstone::Ruby => "Ruby",
            Gemstone::Moonstone => "Moonstone",
            Gemstone::Diamond => "Diamond",
        }
    }

    /// Opaque stones are polished smooth, never faceted.
    pub fn is_opaque(self) -> bool {
        matches!(self, Gemstone::Turquoise | Gemstone::Moonstone)
    }

    /// Whether this stone is plausibly cut this way.
    pub fn suits(self, cut: GemCut) -> bool {
        match self {
            Gemstone::Turquoise | Gemstone::Moonstone => !cut.is_faceted(),
            Gemstone::Diamond => cut.is_faceted() && cut != GemCut::Shard,
            _ => true,
        }
    }
}
