//! Scenery: what lies about a level that nobody carries off. Heaps of rubble, scattered
//! bones and stone pillars, built from the shared geometry like every item, so the level
//! never draws a one-off shape of its own (`CLAUDE.md`: "The armory crate makes the shape of
//! every item").
//!
//! Scenery is a set of builders rather than an Armory [`crate::Family`]: nothing here is ever
//! an item, so it has no state to roll, no noun and no place in the turntable or the loot
//! tables. Each builder is a pure function of a seed.

use serde::{Deserialize, Serialize};

/// What a piece of scenery is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scenery {
    /// A heap of broken stone fallen from the walls and ceiling.
    Rubble,
    /// Someone who did not get out: a skull and a scatter of long bones and ribs.
    Bones,
    /// A dressed stone column, whole and bedded in the ceiling, or broken off short.
    Pillar,
}

impl Scenery {
    pub const ALL: [Scenery; 3] = [Scenery::Rubble, Scenery::Bones, Scenery::Pillar];

    /// The furthest any part of it reaches from its centre across the floor, in world units,
    /// whatever its seed. A pillar's square plinth is this far to a corner.
    pub fn radius(self) -> f32 {
        match self {
            Scenery::Rubble => 4.6,
            Scenery::Bones => 3.6,
            Scenery::Pillar => 4.0,
        }
    }
}
