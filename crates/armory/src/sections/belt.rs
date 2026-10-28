//! What hangs on and is fixed to a belt worn as its own item: its cut, the hanging tail,
//! studs, sword frogs, pouch loops, D-rings and a strap end. The strap itself (width,
//! leather, buckle) is the garment's [`Belt`] section, shared with belted garments.
//!
//! The Armory prototype's `belt` section is only that strap; everything here is this port's
//! own, from the plate prototype's `Belt` base (`data/bases.json`), which carries a pouch.

pub use crate::sections::garment::{Belt, BeltLimits};

use crate::slider::sliders;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeltStyle {
    /// A plain strap and buckle.
    Plain,
    /// Set with domed studs all round.
    Studded,
    /// A broad belt slung toward the left hip with frogs for a sword and a dagger.
    Sword,
    /// A narrow belt with a long hanging tail, set with bar mounts and a metal strap end.
    Girdle,
    /// Loops, rings and a pouch for carrying kit.
    Pouch,
}

impl BeltStyle {
    pub const ALL: [BeltStyle; 5] = [BeltStyle::Plain, BeltStyle::Studded, BeltStyle::Sword, BeltStyle::Girdle, BeltStyle::Pouch];
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeltFittings {
    pub style: BeltStyle,
    /// The waist it is buckled round, relative to the mannequin's.
    pub girth: f32,
    /// How far it drops toward the wearer's left hip.
    pub slant: f32,
    /// Length of the free end past the buckle; a long one hangs down the front.
    pub tail: f32,
    /// Studs (or a girdle's bar mounts) set round the belt.
    pub studs: u32,
    /// Hangers for a scabbard: the first on the left hip, the second on the right.
    pub frogs: u32,
    /// Loops round the belt to hang pouches and tools from.
    pub loops: u32,
    pub rings: u32,
    /// A loop that holds the free end down.
    pub keeper: bool,
    /// A metal end on the tail.
    pub tip: bool,
    /// A belt pouch hung at the right hip.
    pub pouch: bool,
    /// Three loops that each hold a small potion, in front of the right hip. One of the belt's
    /// [`crate::families::bags::Accessory`]s.
    #[serde(default)]
    pub potion_pouch: bool,
    /// A small sack hung behind the left hip. One of the belt's accessories.
    #[serde(default)]
    pub sack: bool,
}

sliders! {
    BeltFittingsLimits for BeltFittings {
        girth: 0.85, 1.3, 0.01;
        slant: 0.0, 0.6, 0.01;
        tail: 0.2, 3.5, 0.05;
        studs: 0.0, 24.0, 1.0;
        frogs: 0.0, 2.0, 1.0;
        loops: 0.0, 4.0, 1.0;
        rings: 0.0, 3.0, 1.0;
    }
}

impl Default for BeltFittings {
    /// A plain belt with a keeper and a pouch.
    fn default() -> Self {
        BeltFittings {
            style: BeltStyle::Plain,
            girth: 1.0,
            slant: 0.0,
            tail: 0.4,
            studs: 0,
            frogs: 0,
            loops: 0,
            rings: 0,
            keeper: true,
            tip: false,
            pouch: true,
            potion_pouch: false,
            sack: false,
        }
    }
}
