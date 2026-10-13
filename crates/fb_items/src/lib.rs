//! Forever Below's item layer. An [`Item`] wraps an `armory::ItemState` (the shape) with
//! everything the game adds: rarity, affixes that change any parameter (including survival
//! ones like warmth and carry weight), derived stats, the grid footprint, and a history.
//!
//! Data tables come from the prototype: `data/afx.json`, `data/bases.json`, `data/goods.json`,
//! `data/sab.json` (Armory family per base, with the styles it accepts). See `docs/items.md`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity { Common, Magic, Rare, Legendary }

/// Equipment slots on the doll, as in the prototype.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Slot { Head, Neck, Shirt, Cloak, Body, Mainhand, Offhand, Hands, Belt, Legs, Feet, Back, Ring1, Ring2, Charm }

// TODO(port): Item, Affix, roll_item(base, rng), derived stats, footprint from real size (about 24 cm per cell).
