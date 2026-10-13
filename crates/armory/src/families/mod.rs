//! One module per Armory family. Each implements [`Generator`]: a typed state, a seeded
//! `roll`, a `noun`, and a `build`. [`crate::Item`] dispatches over the families that exist.

use crate::geom::ItemMesh;

pub mod bow;
pub mod crossbow;
pub mod dagger;
pub mod food;
pub mod mace;
pub mod spear;
pub mod staff;
pub mod sword;

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
