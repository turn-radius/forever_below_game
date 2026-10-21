//! A faction's crest as a UI image, shared by the factions tab and the map.
//!
//! The crest is the armory's own ([`armory::crest`]): the faction's heraldic field painted on a
//! shield by the same pattern shader and device shapes as its banners in the level, drawn once
//! off-screen like an item's icon ([`crate::inventory::icon`]) and kept. There is no second
//! drawing of the field here. A faction not yet met has no arms to show, only a
//! [`plain_field`]: a bare shield with no tinctures and no device.

use armory::heraldry::Field;
use bevy::prelude::*;

use crate::inventory::icon::{CREST_PX, Icons};

/// A crest's height over its width, as the image is drawn.
pub const ASPECT: f32 = CREST_PX.y as f32 / CREST_PX.x as f32;

/// The field of a shield with no arms: bare, so the cloth shows undyed, and no charge.
pub fn plain_field() -> Field {
    Field { painted: false, charge: armory::sections::device::Device::None, ..Field::default() }
}

/// A node `w` wide showing the crest of `field`, a plain shield standing in until it is drawn.
pub fn crest<'a>(parent: &'a mut ChildSpawnerCommands, icons: &mut Icons, field: &Field, w: f32) -> EntityCommands<'a> {
    let mut crest = icons.spawn_crest(parent, field, Node { width: px(w), height: px(w * ASPECT), flex_shrink: 0.0, ..default() });
    crest.insert(Name::new("Crest"));
    crest
}
