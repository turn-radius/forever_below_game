//! The map of the level, in the spirit of Ultima Underworld's automap: what the delver has
//! seen of the level ([`explore`]), drawn as an inked plan on dark paper ([`plan`]) on the map
//! screen (M, [`screen`]).

use bevy::prelude::*;

pub mod explore;
pub mod plan;
pub mod screen;

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        explore::add(app);
        screen::add(app);
    }
}
