//! A cleric blesses and hallows weapons from the inventory ([`fb_items::blessing`]). They earn a
//! blessing charge for every few kills ([`fb_character::devotion::DevotionRules::blessing_kills`]),
//! shown on the character sheet. With the mouse over a weapon in the inventory, B blesses it for
//! one charge, and N hallows it for several if the cleric is dedicated to its kind of weapon. The
//! item card lists whichever the weapon is eligible for.
//!
//! `FB_DEBUG_BLESS=1` makes the delver a cleric dedicated to maces with charges to spend and a
//! mace in the pack, for captures; `=2` has the mace already blessed and `=3` already hallowed.

use bevy::prelude::*;
use fb_character::devotion::Devotion;
use fb_items::{Item, blessing};

use crate::character::{Character, Class};
use crate::inventory::{Hovered, Inventory, Toasts};
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, inventory_open};
use crate::rules::GameRules;

pub struct BlessingPlugin;

impl Plugin for BlessingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, give_the_blessing.run_if(inventory_open).in_set(DelveSystems));
        if std::env::var_os("FB_DEBUG_BLESS").is_some() {
            app.add_systems(Update, set_up_for_capture.in_set(DelveSystems));
        }
    }
}

/// The two blessings a cleric can give.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rite {
    Bless,
    Hallow,
}

impl Rite {
    pub fn action(self) -> Action {
        match self {
            Rite::Bless => Action::Bless,
            Rite::Hallow => Action::Hallow,
        }
    }

    pub fn verb(self) -> &'static str {
        match self {
            Rite::Bless => "Bless",
            Rite::Hallow => "Hallow",
        }
    }

    /// The charges it costs.
    pub fn cost(self, rules: &fb_character::devotion::DevotionRules) -> u32 {
        match self {
            Rite::Bless => 1,
            Rite::Hallow => rules.hallow_blessings,
        }
    }
}

/// Whether `character` can do `rite` on `item` now, or why not. `None` for what is not theirs to
/// offer at all: not a cleric, not a weapon, or (hallowing) not a weapon of the kind they are
/// dedicated to.
pub fn can(character: &Character, rules: &fb_character::rules::Rules, item: &Item, rite: Rite) -> Option<Result<(), String>> {
    if character.class != Class::Cleric {
        return None;
    }
    let allowed = match (rite, character.devotion) {
        (Rite::Bless, _) => blessing::can_bless(item),
        (Rite::Hallow, Some(Devotion::Dedicated(family))) => blessing::can_hallow(item, family),
        (Rite::Hallow, _) => return None,
    };
    match allowed {
        Err(blessing::Refusal::NotBlessable | blessing::Refusal::NotDedicated) => None,
        Err(refusal) => Some(Err(refusal.reason().to_string())),
        Ok(()) => {
            let (cost, left) = (rite.cost(&rules.devotion), rules.devotion.blessings_left(&character.deeds));
            Some(if left >= cost { Ok(()) } else { Err(format!("It takes {cost} blessing{}; you have {left}.", if cost == 1 { "" } else { "s" })) })
        }
    }
}

/// The key labels and verbs the card of `item` lists for `character`.
pub fn card_actions(character: &Character, rules: &fb_character::rules::Rules, item: &Item, keymap: &crate::keymap::Keymap) -> Vec<(String, &'static str)> {
    [Rite::Bless, Rite::Hallow]
        .into_iter()
        .filter(|rite| can(character, rules, item, *rite).is_some_and(|r| r.is_ok()))
        .filter_map(|rite| keymap.inputs(rite.action()).first().map(|key| (key.label(), rite.verb())))
        .collect()
}

/// Does `rite` on `item`: the item as it is now, with the charges spent.
pub fn perform(character: &mut Character, rules: &fb_character::rules::Rules, item: &Item, rite: Rite) -> Result<Item, String> {
    can(character, rules, item, rite).unwrap_or_else(|| Err("That cannot be blessed.".to_string()))?;
    let done = match (rite, character.devotion) {
        (Rite::Hallow, Some(Devotion::Dedicated(family))) => blessing::hallow(item, &character.name, family),
        _ => blessing::bless(item, &character.name),
    }
    .map_err(|r| r.reason().to_string())?;
    character.deeds.spend_blessings(rite.cost(&rules.devotion));
    Ok(done)
}

fn give_the_blessing(input: ActionInput, hovered: Res<Hovered>, mut inventory: ResMut<Inventory>, character: Option<ResMut<Character>>, rules: Option<Res<GameRules>>, mut toasts: Toasts) {
    let Some(mut character) = character else { return };
    let rite = if input.just_pressed(Action::Bless) {
        Rite::Bless
    } else if input.just_pressed(Action::Hallow) {
        Rite::Hallow
    } else {
        return;
    };
    let Some((id, _)) = hovered.0 else { return };
    let Some(item) = inventory.item(id).cloned() else { return };
    match perform(&mut character, GameRules::of(&rules), &item, rite) {
        Ok(done) => {
            let line = format!("You {} the {}", rite.verb().to_lowercase(), done.title().to_lowercase());
            if inventory.replace_item(id, done) {
                toasts.say(line);
            }
        }
        Err(why) => toasts.say(why),
    }
}

/// For captures (`FB_DEBUG_BLESS`).
fn set_up_for_capture(mut inventory: ResMut<Inventory>, character: Option<ResMut<Character>>, mut frames: Local<u32>) {
    *frames += 1;
    if *frames != 3 {
        return;
    }
    let Some(mut character) = character else { return };
    let mode = std::env::var("FB_DEBUG_BLESS").ok().and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(1);
    character.class = Class::Cleric;
    character.devotion = Some(Devotion::Dedicated(armory::Family::Mace));
    for _ in 0..(25 * 8) {
        character.deeds.kill("creature:giant rat", fb_character::kin::Kin::Foe(fb_items::Foe::Vermin), Some(armory::Family::Mace));
    }
    let mut mace = fb_items::roll_item("Mace", 7).expect("a mace");
    let name = character.name.clone();
    if mode == 2 {
        mace = blessing::bless(&mace, &name).expect("blessable");
    } else if mode == 3 {
        mace = blessing::hallow(&mace, &name, armory::Family::Mace).expect("hallowable");
    }
    let id = inventory.add(mace);
    inventory.pick_up(id).ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_character::kin::Kin;

    fn cleric(devotion: Option<Devotion>, kills: u32) -> Character {
        let mut c = Character::default();
        c.class = Class::Cleric;
        c.devotion = devotion;
        for _ in 0..kills {
            c.deeds.kill("creature:giant rat", Kin::Foe(fb_items::Foe::Vermin), None);
        }
        c
    }

    fn mace() -> Item {
        fb_items::roll_item("Mace", 7).unwrap()
    }

    #[test]
    fn a_blessing_costs_a_charge_and_is_recorded() {
        let rules = fb_character::rules::defaults();
        let mut c = cleric(None, 25);
        let blessed = perform(&mut c, rules, &mace(), Rite::Bless).unwrap();
        assert!(blessing::is_blessed(&blessed));
        assert_eq!(rules.devotion.blessings_left(&c.deeds), 0);
        assert!(perform(&mut c, rules, &mace(), Rite::Bless).is_err(), "no charge left");
    }

    #[test]
    fn only_the_dedicated_hallow_and_it_costs_several_charges() {
        let rules = fb_character::rules::defaults();
        let mut plain = cleric(None, 200);
        assert_eq!(can(&plain, rules, &mace(), Rite::Hallow), None);
        assert!(perform(&mut plain, rules, &mace(), Rite::Hallow).is_err());
        let mut c = cleric(Some(Devotion::Dedicated(armory::Family::Mace)), 200);
        let before = rules.devotion.blessings_left(&c.deeds);
        let hallowed = perform(&mut c, rules, &mace(), Rite::Hallow).unwrap();
        assert!(blessing::is_hallowed(&hallowed));
        assert_eq!(before - rules.devotion.blessings_left(&c.deeds), rules.devotion.hallow_blessings);
        assert_eq!(can(&c, rules, &fb_items::roll_item("Sword", 3).unwrap(), Rite::Hallow), None);
    }

    #[test]
    fn only_a_cleric_blesses() {
        let rules = fb_character::rules::defaults();
        let mut fighter = cleric(None, 200);
        fighter.class = Class::Fighter;
        assert!(perform(&mut fighter, rules, &mace(), Rite::Bless).is_err());
    }
}
