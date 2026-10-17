//! One module per Armory family. Each implements [`Generator`]: a typed state, a seeded
//! `roll`, a `noun`, and a `build`. [`crate::Item`] dispatches over the families that exist.

use crate::geom::ItemMesh;

pub mod ammo;
pub mod amulet;
pub mod axe;
pub mod bags;
pub mod belt;
pub mod bow;
pub mod charm;
pub mod chests;
pub mod club;
pub mod crossbow;
pub mod dagger;
pub mod feet;
pub mod food;
pub mod garment;
pub mod hands;
pub mod hat;
pub mod helm;
pub mod hood;
pub mod lantern;
pub mod legs;
pub mod mace;
pub mod quiver;
pub mod ring;
pub mod shield;
pub mod spear;
pub mod staff;
pub mod sundry;
pub mod sword;
pub mod torch;
pub mod writing;

pub(crate) mod hilted;


/// The contract every family module implements.
pub trait Generator {
    type State: Clone + PartialEq + std::fmt::Debug;
    /// "Forge new": every parameter from a seed, via `procgen_rng` only.
    fn roll(seed: u32) -> Self::State;
    /// The item's common name ("Morningstar", "Pernach").
    fn noun(state: &Self::State) -> String;
    /// Plain mesh data in item space.
    fn build(state: &Self::State) -> ItemMesh;
}
