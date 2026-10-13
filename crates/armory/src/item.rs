//! [`Item`]: one rolled item of any ported family, so callers (the turntable, `fb_items`, the
//! loot tables) can roll, name and build items without matching on families themselves.
//!
//! Porting a family is one line in the `ported!` list at the bottom of this file, in
//! [`Family::ALL`] order. Every family state carries `seed`, `cond` and `palette` fields under
//! those names, which [`Item`]'s accessors rely on.

use crate::Family;
use crate::cond::Condition;
use crate::families::Generator;
use crate::geom::ItemMesh;
use crate::palette::Palette;

/// Generates [`Item`], [`Item::PORTED`], [`Item::roll`], [`Item::family`] and `dispatch!` from
/// one list of `Variant => module::Generator(State)` entries.
macro_rules! ported {
    ($($variant:ident => $module:ident :: $generator:ident ( $state_ty:ident )),+ $(,)?) => {
        /// One item's full state, tagged by family. States are plain values of a few hundred
        /// bytes, rolled and passed around rarely, so variants are not boxed to even out sizes.
        #[derive(Clone, Debug, PartialEq)]
        #[allow(clippy::large_enum_variant)]
        pub enum Item {
            $($variant(crate::families::$module::$state_ty),)+
        }

        /// Runs `$body` with `$state` bound to the variant's state and, in the first form,
        /// `$g` aliased to the family's [`Generator`].
        macro_rules! dispatch {
            ($item:expr, $g:ident, $s:ident => $body:expr) => {
                match $item {
                    $(Item::$variant($s) => {
                        type $g = crate::families::$module::$generator;
                        $body
                    })+
                }
            };
            ($item:expr, $s:ident => $body:expr) => {
                match $item {
                    $(Item::$variant($s) => $body,)+
                }
            };
        }

        impl Item {
            /// The families [`Item::roll`] can make, in [`Family::ALL`] order.
            pub const PORTED: &[Family] = &[$(Family::$variant),+];

            /// "Forge new" for any family: `None` until that family is ported.
            pub fn roll(family: Family, seed: u32) -> Option<Item> {
                match family {
                    $(Family::$variant => Some(Item::$variant(crate::families::$module::$generator::roll(seed))),)+
                    #[allow(unreachable_patterns)]
                    _ => None,
                }
            }

            pub fn family(&self) -> Family {
                match self {
                    $(Item::$variant(_) => Family::$variant,)+
                }
            }
        }
    };
}


ported! {
    Staff => staff::Staff(StaffState),
    Mace => mace::Mace(MaceState),
    Spear => spear::Spear(SpearState),
    Bow => bow::Bow(BowState),
}

impl Item {
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
    use crate::families::mace::Mace;

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
