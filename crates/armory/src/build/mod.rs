//! Geometry builders shared between families. Each takes section structs from
//! [`crate::sections`] (never a family's whole state) and returns [`crate::geom::MeshBuilder`]s
//! in a local space the family places.
//!
//! - [`hafted`]: shaft, bands, grip, wrap and pommel (staves, maces, swords, spears, daggers).
//! - [`head`]: collar, head body, flanges, spikes, prongs and finial (maces, staves).
//! - [`gem`]: cut stones and bezels (head gems, pommels, guards, rings, amulets, charms).

pub mod gem;
pub mod hafted;
pub mod head;
