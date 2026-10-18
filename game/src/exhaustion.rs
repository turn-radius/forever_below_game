//! The delver's exhaustion (#223). The level stays at zero until #229 steps the delver's needs
//! on the game clock. It slows the delver where walking speed is worked out (`player::walk`),
//! and at level 6 it ends the run through the same fall as 0 hit points (`combat::vitals`).

use bevy::prelude::*;

/// The delver's exhaustion, on the [`Player`](crate::play::Player) from their spawn.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Exhaustion(#[reflect(remote = crate::inspect::remote::ExhaustionDef)] pub fb_character::Exhaustion);

/// How long the delver has spent in each meter's worst band, and their run of sleep, for turning
/// their needs into exhaustion (`crate::simulate`). On the [`Player`](crate::play::Player) from
/// their spawn, so a new delve starts with nothing counted.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
pub struct ExhaustionTimes(#[reflect(remote = crate::inspect::remote::ExhaustionTimesDef)] pub fb_character::ExhaustionTimes);
