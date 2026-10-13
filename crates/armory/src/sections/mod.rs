//! Sections shared between families, each with its sliders (the prototype's `SECTIONS` entries
//! whose `fam` list names more than one family).
//!
//! - [`hafted`]: the handle half of staves, maces, swords, spears and daggers.
//! - [`head`]: the striking or crowning end of maces and staves.
//! - [`spearhead`]: the blade and the wings or lugs of a thrusting polearm head (spears).
//! - [`socket`]: the socket that seats a head on a haft, with its rings and langets (spears).

pub mod hafted;
pub mod head;
pub mod socket;
pub mod spearhead;
