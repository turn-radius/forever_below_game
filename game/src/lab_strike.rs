//! Who the lab's character fights ([`crate::lab_fight`]): a creature from [`fb_creatures`],
//! striking with its bite, or a folk holding a melee base. The base is rolled from the strike seed.

use std::collections::BTreeSet;

use fb_creatures::Species;
use fb_items::armory_base::ARMORY_BASES;
use fb_items::tables::bases;
use fb_items::{Tag, roll_item};

use crate::combat::is_melee;
use crate::lab_view::LabState;

/// Who strikes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Attacker {
    /// A creature, with its bite.
    Creature(Species),
    /// A folk holding the melee base of this name.
    Folk(String),
}

impl Attacker {
    /// What the attacker is called: "Giant Rat", "folk with Sword".
    pub fn label(&self) -> String {
        match self {
            Attacker::Creature(species) => species.stat_block().name.to_string(),
            Attacker::Folk(base) => format!("folk with {base}"),
        }
    }

    /// Every attacker: each creature, then a folk with each melee base.
    pub fn all() -> Vec<Attacker> {
        Species::ALL.iter().map(|&s| Attacker::Creature(s)).chain(melee_bases().into_iter().map(Attacker::Folk)).collect()
    }
}

/// The bases a folk attacker can hold: weapons whose shape is swung in melee.
pub fn melee_bases() -> Vec<String> {
    use std::sync::LazyLock;
    static BASES: LazyLock<Vec<String>> = LazyLock::new(|| {
        let mut names: BTreeSet<&str> = ARMORY_BASES.iter().map(|(n, _)| *n).collect();
        names.extend(bases().keys().map(String::as_str));
        names.into_iter().filter(|n| roll_item(n, 1).is_some_and(|i| i.profile.tags.contains(&Tag::Weapon) && is_melee(i.shape.family()))).map(String::from).collect()
    });
    BASES.clone()
}

/// The lab character's armour class, as the sheet shows it.
pub fn armour_class(state: &LabState) -> i32 {
    state.character.armour_class_with(&state.ruleset.rules, &state.inventory()).value().round() as i32
}

/// The lab character's hit points at full.
pub fn full_hit_points(state: &LabState) -> i32 {
    crate::combat::first_level_hit_points_with(&state.ruleset.rules, &state.character)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_creature_and_every_melee_base_is_an_attacker() {
        let all = Attacker::all();
        for species in Species::ALL {
            assert!(all.contains(&Attacker::Creature(species)));
        }
        assert!(all.contains(&Attacker::Folk("Sword".into())));
        assert_eq!(Attacker::Folk("Sword".into()).label(), "folk with Sword");
        assert_eq!(Attacker::Creature(Species::GiantRat).label(), "Giant Rat");
    }

    #[test]
    fn the_character_has_the_sheets_armour_and_life() {
        let state = LabState::default();
        assert_eq!((armour_class(&state), full_hit_points(&state)), (15, 10));
    }
}
