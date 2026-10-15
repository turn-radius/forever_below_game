//! The map of the level, in the spirit of Ultima Underworld's automap: what the delver has
//! seen of the level ([`explore`]), drawn as an inked plan on dark paper.

use bevy::prelude::*;

pub mod explore;

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        explore::add(app);
    }
}
