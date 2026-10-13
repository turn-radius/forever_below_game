//! Plaques: the outlines pendants and signet faces are cut to (the prototype's `pendantPts`
//! in `reference/spline_armory_engine.js`). The devices they bear are
//! [`crate::sections::device::Device`].

use serde::{Deserialize, Serialize};

/// The outline of a flat plaque: a pendant, a signet face, a badge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaqueShape {
    Disc,
    Oval,
    Octagon,
    /// A heater shield.
    Shield,
    Teardrop,
    Quatrefoil,
    Star,
    Cross,
    Crescent,
    Lozenge,
    Ankh,
}

impl PlaqueShape {
    pub const ALL: [PlaqueShape; 11] = [
        PlaqueShape::Disc, PlaqueShape::Oval, PlaqueShape::Octagon, PlaqueShape::Shield, PlaqueShape::Teardrop,
        PlaqueShape::Quatrefoil, PlaqueShape::Star, PlaqueShape::Cross, PlaqueShape::Crescent, PlaqueShape::Lozenge,
        PlaqueShape::Ankh,
    ];
}
