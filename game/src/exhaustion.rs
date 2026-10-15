//! The delver's exhaustion (#223). The level stays at zero until #229 steps the delver's needs
//! on the game clock. It slows the delver where walking speed is worked out (`player::walk`),
//! and at level 6 it ends the run through the same fall as 0 hit points (`combat::vitals`).

use bevy::prelude::*;

/// The delver's exhaustion, on the [`Player`](crate::play::Player) from their spawn.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Exhaustion(pub fb_character::Exhaustion);
