//! Geometry builders shared between families. Each takes section structs from
//! [`crate::sections`] (never a family's whole state) and returns [`crate::geom::MeshBuilder`]s
//! in a local space the family places.
//!
//! - [`hafted`]: shaft, bands, grip, wrap and pommel (staves, maces, swords, spears, daggers).
//! - [`head`]: collar, head body, flanges, spikes, prongs and finial, and the whole
//!   [`head::head_assembly`] (maces, staves).
//! - [`gem`]: cut stones and bezels (head gems, pommels, guards, rings, amulets, charms).
//! - [`socket`]: a socket sleeved over a haft, its rings and langets (spears).
//! - [`spearhead`]: a spearhead blade and its wings or bar (spears).
//! - [`parts`]: [`parts::PartList`], named meshes with their materials, finished into an item.

pub mod gem;
pub mod hafted;
pub mod head;
pub mod parts;
pub mod socket;
pub mod spearhead;
