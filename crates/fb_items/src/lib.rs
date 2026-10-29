//! Forever Below's item layer. An [`Item`] wraps an `armory::Item` (the shape) with
//! everything the game adds: rarity, affixes that change any parameter (including survival
//! ones like warmth and carry weight), derived stats, the grid footprint, and a history.
//!
//! Data tables come from the prototype: `data/afx.json`, `data/bases.json`, `data/goods.json`
//! ([`tables`]). The Armory family and styles each base is rolled from are typed Rust in
//! [`armory_base`]. [`roll_item`] rolls a whole item from a base name and a seed. See
//! `docs/items.md`.

pub mod armory_base;
pub mod camp;
pub mod commission;
pub mod contents;
pub mod dice;
pub mod dress;
pub mod enchant;
pub mod fittings;
pub mod foe;
pub mod footprint;
pub mod identify;
pub mod incense;
pub mod item;
pub mod jackalope;
pub mod lesson;
pub mod modifier;
pub mod naming;
pub mod notes;
pub mod oil;
pub mod outfit;
pub mod profile;
pub mod provision;
pub mod range;
pub mod rarity;
pub mod scrap;
pub mod stats;
pub mod stock;
pub mod tables;
pub mod tongues;
pub mod troll;
pub mod undead;
pub mod value;

pub use foe::Foe;
pub use identify::Known;
pub use contents::{container_item, roll_contents};
pub use item::{Coins, Item, ItemParts, roll_item, roll_item_with, roll_loot, stack_up};
pub use armory::families::bags::Accessory;
pub use fittings::{FittingGrid, fitting_grid, fitting_grids, fitting_item};
pub use item::{FROG_CELLS, STRAP_CELLS, fits_potion_pouch};
pub use naming::Sex;
pub use incense::{CENSER, INCENSE};
pub use oil::LAMP_OIL;
pub use rarity::Odds;
pub use provision::{Consume, ELIXIRS, Elixir, Heal, POTIONS, Provision, SWALLOW, elixir, healing_base, provision};

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

/// Ordered from common to legendary, so a cap is a `min`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity { Common, Magic, Rare, Legendary }

/// Worn or held gear, which takes affixes, or goods that are only carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind { Gear, Good }

/// Equipment slots on the doll, as in the prototype, the hip a lantern hangs at from the belt,
/// the quiver slot a quiver or bolt case is slung in, and the satchel slot a satchel is slung in
/// at the hip, beside a pack or sack on the back (all our own; the plate had none). `Satchel` is
/// last so the slots keep their order in saves; a save from before it holds a satchel in `Back`
/// (the game moves it on load).
///
/// A pack's lashing and weapon strap and a belt's frogs and potion pouch are not slots: each is a
/// small grid of its own that belongs to its pack or belt ([`fittings`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Slot { Head, Neck, Shirt, Cloak, Body, Mainhand, Offhand, Hands, Belt, Legs, Feet, Back, Ring1, Ring2, Charm, Hip, Quiver, Satchel }

/// What an item is made of and how it is used. Affixes roll only on items with their tag
/// (`docs/items.md`, "Rarity and affixes").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tag { Cloth, Leather, Metal, Wood, Wear, Armor, Quiet, Weapon, Light, Trinket, Container, Bag, Food, Ammunition, Ranged, Blunt, Blade }

/// The game layer's random stream for an item rolled from `seed`: rarity, affixes, names and
/// history draw from it in that order. Kept apart from the Armory's own stream for the shape,
/// so the same seed gives the same shape whatever the game layer does. The mixing constants
/// are this port's own.
pub fn game_rng(seed: u32) -> Mulberry32 {
    Mulberry32::new(seed.wrapping_mul(0x85EB_CA6B) ^ 0xC2B2_AE35)
}
