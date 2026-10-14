//! Forever Below's item layer. An [`Item`] wraps an `armory::ItemState` (the shape) with
//! everything the game adds: rarity, affixes that change any parameter (including survival
//! ones like warmth and carry weight), derived stats, the grid footprint, and a history.
//!
//! Data tables come from the prototype: `data/afx.json`, `data/bases.json`, `data/goods.json`
//! ([`tables`]). The Armory family and styles each base is rolled from are typed Rust in
//! [`armory_base`]. See `docs/items.md`.

pub mod armory_base;
pub mod dice;
pub mod footprint;
pub mod modifier;
pub mod rarity;
pub mod stats;
pub mod tables;

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity { Common, Magic, Rare, Legendary }

/// Equipment slots on the doll, as in the prototype.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Slot { Head, Neck, Shirt, Cloak, Body, Mainhand, Offhand, Hands, Belt, Legs, Feet, Back, Ring1, Ring2, Charm }

/// What an item is made of and how it is used. Affixes roll only on items with their tag
/// (`docs/items.md`, "Rarity and affixes").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tag { Cloth, Leather, Metal, Wood, Wear, Armor, Quiet, Weapon, Light, Trinket, Container, Food }

/// The game layer's random stream for an item rolled from `seed`: rarity, affixes, names and
/// history draw from it in that order. Kept apart from the Armory's own stream for the shape,
/// so the same seed gives the same shape whatever the game layer does. The mixing constants
/// are this port's own.
pub fn game_rng(seed: u32) -> Mulberry32 {
    Mulberry32::new(seed.wrapping_mul(0x85EB_CA6B) ^ 0xC2B2_AE35)
}

// TODO(port): Item, roll_item(base, rng), derived stats, footprint from real size (about 24 cm per cell).
