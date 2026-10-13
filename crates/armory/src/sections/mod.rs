//! Sections shared between families, each with its sliders (the prototype's `SECTIONS` entries
//! whose `fam` list names more than one family).
//!
//! - [`hafted`]: the handle half of staves, maces, swords, spears, daggers, clubs and axes,
//!   and a club's swelling.
//! - [`head`]: the striking or crowning end of maces and staves.
//! - [`spearhead`]: the blade and the wings or lugs of a thrusting polearm head (spears).
//! - [`socket`]: the socket that seats a head on a haft, with its rings and langets (spears).
//! - [`ranged`]: limbs, string and arrow of bows and crossbows.
//! - [`blade`]: the blade and guard of swords, daggers and spears.
//! - [`fabric`]: textiles and fabric patterns (bags, garments, hems, a chest's glaze).
//! - [`device`]: emblems engraved, painted or stencilled (signets, shields, crates).
//! - [`garment`]: the cut, belt and plates of body armour and clothing.
//! - [`hem`]: hems and trims on garments, helmets, gloves and footwear.
//! - [`hand`]: gloves, gauntlets, mittens and bracers.
//! - [`boot`]: shoes, boots, leg wraps, sabatons and greaves.
//! - [`jewel`]: the set stone of rings, amulets and charms.
//! - [`emblem`]: plaque outlines (signet faces, pendants).
//! - [`axe_head`]: an axe's bit, crosswise through an eye, and its poll (axes).
//! - [`legwear`]: trousers, breeches and leggings.

pub mod axe_head;
pub mod blade;
pub mod boot;
pub mod device;
pub mod emblem;
pub mod fabric;
pub mod garment;
pub mod hafted;
pub mod hand;
pub mod head;
pub mod hem;
pub mod jewel;
pub mod legwear;
pub mod ranged;
pub mod socket;
pub mod spearhead;
