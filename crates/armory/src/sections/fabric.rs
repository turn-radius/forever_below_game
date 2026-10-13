//! What soft goods are made of and how they are patterned. From the prototype's `FABRIC`
//! list and the bag's `fabric` select (`reference/spline_armory_engine.js`); garments, hems,
//! boots and a chest's glaze use the same pattern list.
//!
//! The pattern itself is drawn by the shader (`docs/rendering.md`, "Wear, rust and grime"),
//! so it lives in the state and the item's parts carry only the base colour.

use crate::material::MaterialKind;
use serde::{Deserialize, Serialize};

/// The stuff a bag, hood or garment is cut from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Textile {
    Leather,
    /// Woven wool or linen.
    Cloth,
    /// Coarse jute or hemp sacking.
    Burlap,
}

impl Textile {
    pub const ALL: [Textile; 3] = [Textile::Leather, Textile::Cloth, Textile::Burlap];

    /// The material a part cut from it renders and decays as.
    pub fn material(self) -> MaterialKind {
        match self {
            Textile::Leather => MaterialKind::Leather,
            Textile::Cloth | Textile::Burlap => MaterialKind::Cloth,
        }
    }

    /// How thick a cut panel is, in units (1 unit = 10 cm).
    pub fn thickness(self) -> f32 {
        match self {
            Textile::Leather => 0.035,
            Textile::Cloth => 0.02,
            Textile::Burlap => 0.03,
        }
    }
}

/// A woven, printed, painted or glazed pattern (the prototype's `FABRIC`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FabricPattern {
    /// Plain.
    None,
    Stripes,
    Hoops,
    Checks,
    Tartan,
    Lozenge,
    /// Powdered: small dots.
    Dots,
    Chevron,
    Quilted,
    Rivets,
}

impl FabricPattern {
    pub const ALL: [FabricPattern; 10] = [
        FabricPattern::None, FabricPattern::Stripes, FabricPattern::Hoops, FabricPattern::Checks, FabricPattern::Tartan,
        FabricPattern::Lozenge, FabricPattern::Dots, FabricPattern::Chevron, FabricPattern::Quilted, FabricPattern::Rivets,
    ];
}
