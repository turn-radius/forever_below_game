//! [`Item`]: one rolled item of any ported family, so callers (the turntable, `fb_items`, the
//! loot tables) can roll, name and build items without matching on families themselves.
//!
//! Porting a family adds one variant to [`Item`], one entry to [`Item::PORTED`], and one arm
//! each to [`Item::roll`], [`Item::family`] and both forms of `dispatch!`. Every family state
//! carries `seed`, `cond` and `palette` fields under those names, which `dispatch!` relies on.

use crate::Family;
use crate::cond::Condition;
use crate::families::Generator;
use crate::families::mace::{Mace, MaceState};
use crate::families::staff::{Staff, StaffState};
use crate::geom::ItemMesh;
use crate::palette::Palette;

/// One item's full state, tagged by family.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Staff(StaffState),
    Mace(MaceState),
}

/// Runs `$body` with `$state` bound to the variant's state and, in the first form,
/// `$generator` aliased to the family's [`Generator`].
macro_rules! dispatch {
    ($item:expr, $generator:ident, $state:ident => $body:expr) => {
        match $item {
            Item::Staff($state) => {
                type $generator = Staff;
                $body
            }
            Item::Mace($state) => {
                type $generator = Mace;
                $body
            }
        }
    };
    ($item:expr, $state:ident => $body:expr) => {
        match $item {
            Item::Staff($state) => $body,
            Item::Mace($state) => $body,
        }
    };
}

impl Item {
    /// The families [`Item::roll`] can make, in [`Family::ALL`] order.
    pub const PORTED: &[Family] = &[Family::Staff, Family::Mace];

    /// "Forge new" for any family: `None` until that family is ported.
    pub fn roll(family: Family, seed: u32) -> Option<Item> {
        match family {
            Family::Staff => Some(Item::Staff(Staff::roll(seed))),
            Family::Mace => Some(Item::Mace(Mace::roll(seed))),
            _ => None,
        }
    }

    pub fn family(&self) -> Family {
        match self {
            Item::Staff(_) => Family::Staff,
            Item::Mace(_) => Family::Mace,
        }
    }

    pub fn seed(&self) -> u32 {
        dispatch!(self, state => state.seed)
    }

    /// The item's common name ("Morningstar", "Pernach").
    pub fn noun(&self) -> String {
        dispatch!(self, G, state => G::noun(state))
    }

    /// Plain mesh data in item space. May be empty while a family's geometry is unported.
    pub fn build(&self) -> ItemMesh {
        dispatch!(self, G, state => G::build(state))
    }

    pub fn condition(&self) -> Condition {
        dispatch!(self, state => state.cond)
    }

    /// The material colours "Forge new" picked for this item.
    pub fn palette(&self) -> Palette {
        dispatch!(self, state => state.palette)
    }
}

impl Family {
    /// Whether [`Item::roll`] can make this family yet.
    pub fn is_ported(self) -> bool {
        Item::PORTED.contains(&self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolls_are_deterministic() {
        for seed in [0, 1, 1207, u32::MAX] {
            let a = Item::roll(Family::Mace, seed).unwrap();
            let b = Item::roll(Family::Mace, seed).unwrap();
            assert_eq!(a, b);
            assert_eq!(a.noun(), b.noun());
            assert_eq!(a.build(), b.build());
        }
    }

    #[test]
    fn item_matches_its_generator() {
        let item = Item::roll(Family::Mace, 1207).unwrap();
        let state = Mace::roll(1207);
        assert_eq!(item.family(), Family::Mace);
        assert_eq!(item.seed(), 1207);
        assert_eq!(item.noun(), Mace::noun(&state));
        assert_eq!(item.condition(), state.cond);
        assert_eq!(item.palette(), state.palette);
    }

    #[test]
    fn only_ported_families_roll() {
        for family in Family::ALL {
            assert_eq!(Item::roll(family, 7).is_some(), family.is_ported(), "{family:?}");
            if let Some(item) = Item::roll(family, 7) {
                assert_eq!(item.family(), family);
            }
        }
    }

    #[test]
    fn ported_is_in_family_order() {
        let order: Vec<Family> = Family::ALL.into_iter().filter(|f| f.is_ported()).collect();
        assert_eq!(order, Item::PORTED);
    }
}
