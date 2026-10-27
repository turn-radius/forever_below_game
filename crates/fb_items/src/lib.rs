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
pub mod foe;
pub mod footprint;
pub mod identify;
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
pub use item::{FROG_CELLS, STRAP_CELLS, fits_potion_pouch};
pub use naming::Sex;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Slot { Head, Neck, Shirt, Cloak, Body, Mainhand, Offhand, Hands, Belt, Legs, Feet, Back, Ring1, Ring2, Charm, Hip, Quiver, Satchel, Strap, Frog1, Frog2, Potion1, Potion2, Potion3 }

impl Slot {
    /// The slots that are a worn pack's or belt's own fittings: the weapon strap, a belt's two
    /// frogs and the three loops of its potion pouch. Each shows only while what is worn was
    /// rolled with that fitting ([`Item::accessories`]) and holds its one item outside any grid.
    ///
    /// To add a fitting: an [`Accessory`] variant rolled by the host's family; a slot here, in
    /// [`Slot::host`] and [`Slot::needs`]; its row in `data/slots.json`; what it takes in
    /// [`Item::fits_fitting`]; and, if it is drawn on the body, a fit in `fb_body` and a piece in
    /// `Figure::dress`. The inventory, saves, barter and the load follow from `FITTINGS`.
    pub const FITTINGS: [Slot; 6] = [Slot::Strap, Slot::Frog1, Slot::Frog2, Slot::Potion1, Slot::Potion2, Slot::Potion3];

    /// Whether it is a fitting of a worn pack or belt ([`Slot::FITTINGS`]).
    pub fn is_fitting(self) -> bool {
        Slot::FITTINGS.contains(&self)
    }

    /// The slot that carries the fitting: the pack's back for the strap, else the belt.
    pub fn host(self) -> Option<Slot> {
        match self {
            Slot::Strap => Some(Slot::Back),
            Slot::Frog1 | Slot::Frog2 | Slot::Potion1 | Slot::Potion2 | Slot::Potion3 => Some(Slot::Belt),
            _ => None,
        }
    }

    /// The accessory its host must have been rolled with for this fitting slot to exist.
    pub fn needs(self) -> Option<Accessory> {
        match self {
            Slot::Strap => Some(Accessory::WeaponStrap),
            Slot::Frog1 => Some(Accessory::LeftFrog),
            Slot::Frog2 => Some(Accessory::RightFrog),
            Slot::Potion1 | Slot::Potion2 | Slot::Potion3 => Some(Accessory::PotionPouch),
            _ => None,
        }
    }
}

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
