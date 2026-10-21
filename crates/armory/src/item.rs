//! [`Item`]: one rolled item of any ported family, so callers (the turntable, `fb_items`, the
//! loot tables) can roll, name and build items without matching on families themselves.
//!
//! Porting a family is one line in the `ported!` list at the bottom of this file, in
//! [`Family::ALL`] order. Every family state carries `seed`, `cond` and `palette` fields under
//! those names, which [`Item`]'s accessors rely on.

use crate::Family;
use crate::cond::Condition;
use crate::etch::Etching;
use crate::families::Generator;
use crate::geom::ItemMesh;
use crate::palette::Palette;

/// Generates [`Item`], [`Item::PORTED`], [`Item::roll`], [`Item::family`] and `dispatch!` from
/// one list of `Variant => module::Generator(State)` entries.
macro_rules! ported {
    ($($variant:ident => $module:ident :: $generator:ident ( $state_ty:ident )),+ $(,)?) => {
        /// One item's full state, tagged by family. States are plain values of a few hundred
        /// bytes, rolled and passed around rarely, so variants are not boxed to even out sizes.
        #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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

            /// The slider ranges of this item's family, as the JSON of its `LIMITS` (one
            /// object per section, one `{min, max, step}` per slider). Used by [`crate::reflect`].
            pub(crate) fn limits_value(&self) -> serde_json::Value {
                match self {
                    $(Item::$variant(_) => serde_json::to_value(crate::families::$module::LIMITS).unwrap_or_default(),)+
                }
            }
        }
    };
}


ported! {
    Staff => staff::Staff(StaffState),
    Mace => mace::Mace(MaceState),
    Sword => sword::Sword(SwordState),
    Spear => spear::Spear(SpearState),
    Dagger => dagger::Dagger(DaggerState),
    Club => club::Club(ClubState),
    Axe => axe::Axe(AxeState),
    Bow => bow::Bow(BowState),
    Crossbow => crossbow::Crossbow(CrossbowState),
    Helm => helm::Helm(HelmState),
    Hood => hood::Hoods(HoodState),
    Hat => hat::Hats(HatState),
    Garment => garment::Garment(GarmentState),
    Belt => belt::Belts(BeltState),
    Hands => hands::Hands(HandsState),
    Legs => legs::Legs(LegsState),
    Feet => feet::Feet(FeetState),
    Shield => shield::Shield(ShieldState),
    Ring => ring::Ring(RingState),
    Amulet => amulet::Amulet(AmuletState),
    Charm => charm::Charm(CharmState),
    Bags => bags::Bags(BagState),
    Chests => chests::Chests(ChestState),
    Food => food::Food(FoodState),
    Torch => torch::Torch(TorchState),
    Lantern => lantern::Lantern(LanternState),
    Sundry => sundry::Sundry(SundryState),
    Writing => writing::Writing(WritingState),
    Arrow => ammo::Arrows(AmmoState),
    Bolt => ammo::Bolts(AmmoState),
    Quiver => quiver::Quivers(QuiverState),
    Tent => tent::Tent(TentState),
    Campfire => campfire::Campfire(CampfireState),
    BoltCase => quiver::BoltCases(QuiverState),
    Banner => banner::Banner(BannerState),
}

impl Item {
    /// [`Item::roll`], but quivers and bolt cases come filled to `fullness` (0..=1) of their
    /// capacity with matching ammunition; every other family rolls as usual. For turntables
    /// and loot.
    pub fn roll_stocked(family: Family, seed: u32, fullness: f32) -> Option<Item> {
        match family {
            Family::Quiver => Some(Item::Quiver(crate::families::quiver::Quivers::stocked(seed, fullness))),
            Family::BoltCase => Some(Item::BoltCase(crate::families::quiver::BoltCases::stocked(seed, fullness))),
            _ => Item::roll(family, seed),
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

    /// Replaces the item's whole condition (a preset from [`Condition::of`], say).
    pub fn set_condition(&mut self, condition: Condition) {
        dispatch!(self, state => state.cond = condition);
    }

    /// The material colours "Forge new" picked for this item.
    pub fn palette(&self) -> Palette {
        dispatch!(self, state => state.palette)
    }

    /// The item's etching, carving, tooling or embroidery, if it has any. Food, torches,
    /// lanterns and sundries are never etched.
    pub fn etching(&self) -> Option<Etching> {
        let etch = match self {
            Item::Staff(s) => s.etch,
            Item::Mace(s) => s.etch,
            Item::Sword(s) => s.etch,
            Item::Spear(s) => s.etch,
            Item::Dagger(s) => s.etch,
            Item::Club(s) => s.etch,
            Item::Axe(s) => s.etch,
            Item::Bow(s) => s.etch,
            Item::Crossbow(s) => s.etch,
            Item::Helm(s) => s.etch,
            Item::Hood(s) => s.etch,
            Item::Hat(s) => s.etch,
            Item::Garment(s) => s.etch,
            Item::Belt(s) => s.etch,
            Item::Hands(s) => s.etch,
            Item::Legs(s) => s.etch,
            Item::Feet(s) => s.etch,
            Item::Shield(s) => s.etch,
            Item::Ring(s) => s.etch,
            Item::Amulet(s) => s.etch,
            Item::Charm(s) => s.etch,
            Item::Bags(s) => s.etch,
            Item::Chests(s) => s.etch,
            Item::Food(_) | Item::Torch(_) | Item::Lantern(_) | Item::Sundry(_) | Item::Writing(_) | Item::Arrow(_) | Item::Bolt(_) | Item::Quiver(_) | Item::Tent(_) | Item::Campfire(_) | Item::BoltCase(_) | Item::Banner(_) => return None,
        };
        etch.on.then_some(etch)
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
    fn etching_comes_from_the_state_when_on() {
        let etched = (0..64).filter_map(|seed| Item::roll(Family::Sword, seed)).find(|item| item.etching().is_some()).unwrap();
        let Item::Sword(state) = &etched else { unreachable!() };
        assert_eq!(etched.etching(), Some(state.etch));
        let plain = (0..64).filter_map(|seed| Item::roll(Family::Sword, seed)).find(|item| item.etching().is_none()).unwrap();
        let Item::Sword(state) = &plain else { unreachable!() };
        assert!(!state.etch.on);
        assert_eq!(Item::roll(Family::Food, 7).unwrap().etching(), None);
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
