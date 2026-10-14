//! The stone a ring, amulet or charm sets: the prototype's `stone` section, shared by those
//! three families.

use crate::gem::{GemCut, Gemstone};
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

/// A set stone. Its colour comes from [`Gemstone`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SetStone {
    pub cut: GemCut,
    /// Girdle diameter (the prototype's `stone.size`). Builders halve it for
    /// [`crate::build::gem::cut_stone`], which takes a radius.
    pub size: f32,
    pub stone: Gemstone,
}

sliders! {
    SetStoneLimits for SetStone {
        size: 0.015, 0.14, 0.001;
    }
}

impl Default for SetStone {
    /// The prototype's `BASE.stone`.
    fn default() -> Self {
        SetStone { cut: GemCut::Brilliant, size: 0.05, stone: Gemstone::Garnet }
    }
}

impl SetStone {
    /// The cuts the prototype's `stone` select offers (no raw shard).
    pub const CUTS: [GemCut; 5] = [GemCut::Brilliant, GemCut::Rose, GemCut::Cabochon, GemCut::Octahedron, GemCut::Orb];

    /// Girdle radius.
    pub fn radius(&self) -> f32 {
        self.size * 0.5
    }
}
