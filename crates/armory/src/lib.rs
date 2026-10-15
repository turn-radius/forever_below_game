//! # armory
//!
//! The Rust port of Spline Armory. Two halves, kept apart on purpose:
//!
//! 1. **State**: an [`ItemState`] is every parameter of one item (family, seed, and each
//!    section's values: shaft, blade, guard, pommel, head, limbs, garment, helm, bag, food...).
//!    `roll(family, seed)` is the "Forge new" of the prototype, rebuilt: seeded, deterministic,
//!    and free to grow beyond the prototype's sliders.
//! 2. **Geometry**: `build(&ItemState) -> ItemMesh` turns a state into plain mesh data
//!    (positions, normals, indices, a material slot per part, and the smoothed outline
//!    normals the ink pass needs). The game crate converts that to Bevy meshes.
//!
//! Nothing here depends on Bevy, so Heroes' Hold and a future native Spline Armory can share it.
//!
//! Starting point: `reference/spline-armory.html` and `reference/spline_armory_engine.js`.
//! Example output: `fixtures/armory/<family>.json` (64 seeds per family), for reference only.

pub mod build;
pub mod cond;
pub mod etch;
pub mod fabric;
pub mod families;
pub mod gem;
pub mod geom;
pub mod heraldry;
pub mod item;
pub mod material;
pub mod palette;
pub mod reflect;
pub mod sections;
pub mod slider;

pub use geom::{ItemMesh, Part};
pub use item::Item;
pub use material::{MaterialKind, Rgb};

use serde::{Deserialize, Serialize};

/// The Armory families: the prototype's eighteen in its order, with clubs and axes (which its
/// plate faked outside the Armory) after daggers among the held weapons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Staff, Mace, Sword, Spear, Dagger, Club, Axe, Bow, Crossbow, Helm, Hood, Hat, Garment, Belt, Hands, Legs, Feet, Shield, Ring, Amulet, Charm, Bags, Chests, Food, Torch, Lantern, Sundry,
}

impl Family {
    pub const ALL: [Family; 27] = [
        Family::Staff, Family::Mace, Family::Sword, Family::Spear, Family::Dagger, Family::Club, Family::Axe, Family::Bow, Family::Crossbow, Family::Helm, Family::Hood, Family::Hat, Family::Garment, Family::Belt, Family::Hands, Family::Legs, Family::Feet, Family::Shield, Family::Ring, Family::Amulet, Family::Charm, Family::Bags, Family::Chests, Family::Food, Family::Torch, Family::Lantern, Family::Sundry,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Family::Staff => "staff",
            Family::Mace => "mace",
            Family::Sword => "sword",
            Family::Spear => "spear",
            Family::Dagger => "dagger",
            Family::Club => "club",
            Family::Axe => "axe",
            Family::Bow => "bow",
            Family::Crossbow => "crossbow",
            Family::Helm => "helm",
            Family::Hood => "hood",
            Family::Hat => "hat",
            Family::Garment => "garment",
            Family::Belt => "belt",
            Family::Hands => "hands",
            Family::Legs => "legs",
            Family::Feet => "feet",
            Family::Shield => "shield",
            Family::Ring => "ring",
            Family::Amulet => "amulet",
            Family::Charm => "charm",
            Family::Bags => "bags",
            Family::Chests => "chests",
            Family::Food => "food",
            Family::Torch => "torch",
            Family::Lantern => "lantern",
            Family::Sundry => "sundry",
        }
    }
    /// Whether the Spline Armory prototype had this family (and so example fixtures).
    pub fn in_armory_prototype(self) -> bool {
        !matches!(self, Family::Club | Family::Axe | Family::Hood | Family::Hat | Family::Belt | Family::Legs | Family::Torch | Family::Lantern | Family::Sundry)
    }
}

/// One item's full parameter set. Untyped for now, in the prototype's JSON shape so the
/// example fixtures load; replace with typed sections family by family.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemState {
    pub family: Family,
    pub seed: u32,
    #[serde(flatten)]
    pub sections: serde_json::Map<String, serde_json::Value>,
}

/// A row of an example fixture: the seed, the generated name, and the prototype's state.
#[derive(Clone, Debug, Deserialize)]
pub struct FixtureRow {
    pub seed: u32,
    pub name: String,
    pub state: ItemState,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Fixture {
    pub family: Family,
    pub count: usize,
    pub rows: Vec<FixtureRow>,
}

// TODO(port): pub fn roll(family: Family, seed: u32) -> ItemState
// TODO(port): pub fn noun(state: &ItemState) -> String   (forgeNoun in the reference)
// TODO(port): pub fn build(state: &ItemState) -> ItemMesh
