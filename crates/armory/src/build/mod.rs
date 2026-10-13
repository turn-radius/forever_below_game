//! Geometry builders shared between families. Each takes section structs from
//! [`crate::sections`] (never a family's whole state) and returns [`crate::geom::MeshBuilder`]s
//! in a local space the family places.
//!
//! - [`hafted`]: shaft, bands, grip, wrap, rivets and pommel (staves, maces, swords, spears,
//!   daggers).
//! - [`head`]: collar, head body, flanges, spikes, prongs and finial, and the whole
//!   [`head::head_assembly`] (maces, staves).
//! - [`gem`]: cut stones and bezels (head gems, pommels, guards, rings, amulets, charms).
//! - [`socket`]: a socket sleeved over a haft, its rings and langets (spears).
//! - [`spearhead`]: a spearhead blade and its wings or bar (spears).
//! - [`limb`]: bow limbs and crossbow prods, their nocks, the string, a cord wound on a handle.
//! - [`arrow`]: arrows and crossbow bolts.
//! - [`parts`]: [`parts::Parts`], collecting meshes into an item's parts.
//! - [`blade`]: blades of every profile and section (swords, daggers, spears).
//! - [`guard`]: crosses, bars, discs and lobes, with terminals, langets and stones (swords,
//!   daggers, spears).
//! - [`hilt`]: a whole hilted blade, handle to point (swords, daggers).

pub mod arrow;
pub mod blade;
pub mod gem;
pub mod guard;
pub mod hafted;
pub mod head;
pub mod hilt;
pub mod limb;
pub mod parts;
pub mod socket;
pub mod spearhead;
