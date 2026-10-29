//! Blessings a cleric gives their gear. A blessed weapon carries "Blessed"; a cleric dedicated to
//! a kind of weapon can hallow one weapon of that kind, which carries "Hallowed" in its place
//! (both in `data/afx.json`, flagged `holy`, so they never roll and no enchanter sells them).
//! Weapons only for now; armour comes with its own dedication.
//!
//! Neither changes an item's rarity: it is what it was, with a blessing on it. Each is a change
//! recorded as the item is now, with a line of history ("Blessed by Maren Holt."), and it
//! round-trips through saves like any affix, by name.
//!
//! What a blessing costs the cleric (charges earned by kills) is the character's business
//! (`fb_character::devotion`).

use crate::item::{Item, ItemParts};
use crate::naming::{item_name, noun};
use crate::tables::{Affix, affix_named};
use crate::{Kind, Rarity, Tag};

pub const BLESSED: &str = "Blessed";
pub const HALLOWED: &str = "Hallowed";

/// Why a blessing cannot be given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not a weapon, or a stack, or a legend whose make is not to be touched.
    NotBlessable,
    /// Already blessed (or hallowed).
    AlreadyBlessed,
    /// Already hallowed.
    AlreadyHallowed,
    /// The weapon is not of the kind the cleric is dedicated to.
    NotDedicated,
}

impl Refusal {
    pub fn reason(self) -> &'static str {
        match self {
            Refusal::NotBlessable => "Only a single weapon can be blessed.",
            Refusal::AlreadyBlessed => "It is already blessed.",
            Refusal::AlreadyHallowed => "It is already hallowed.",
            Refusal::NotDedicated => "Only a weapon of the kind you are dedicated to can be hallowed.",
        }
    }
}

fn holy(item: &Item) -> Option<&'static Affix> {
    item.affixes.iter().copied().find(|a| a.holy)
}

pub fn is_blessed(item: &Item) -> bool {
    holy(item).is_some_and(|a| a.name == BLESSED)
}

pub fn is_hallowed(item: &Item) -> bool {
    holy(item).is_some_and(|a| a.name == HALLOWED)
}

fn blessable(item: &Item) -> bool {
    item.quantity == 1 && item.profile.kind == Kind::Gear && item.coins.is_none() && item.legend.is_none() && item.rarity != Rarity::Legendary && item.profile.tags.contains(&Tag::Weapon)
}

/// Whether `item` could be blessed now.
pub fn can_bless(item: &Item) -> Result<(), Refusal> {
    if !blessable(item) {
        return Err(Refusal::NotBlessable);
    }
    if holy(item).is_some() {
        return Err(Refusal::AlreadyBlessed);
    }
    Ok(())
}

/// Whether a cleric dedicated to `dedicated` could hallow `item` now.
pub fn can_hallow(item: &Item, dedicated: armory::Family) -> Result<(), Refusal> {
    if !blessable(item) {
        return Err(Refusal::NotBlessable);
    }
    if is_hallowed(item) {
        return Err(Refusal::AlreadyHallowed);
    }
    if item.shape.family() != dedicated {
        return Err(Refusal::NotDedicated);
    }
    Ok(())
}

/// `item` with its holy affix set to `name`, named for it, its stats worked out again and `line`
/// added to its history. Its rarity and everything else stays.
fn consecrate(item: &Item, name: &str, line: String) -> Item {
    let affix = affix_named(name).expect("the blessing is in data/afx.json");
    let mut parts = ItemParts::from(item.clone());
    parts.affixes.retain(|a| !a.holy);
    let plain = item_name(&noun(&parts.base, &parts.shape), &parts.affixes);
    parts.affixes.push(affix);
    parts.name = format!("{} {plain}", affix.name);
    parts.history.push(line);
    Item::from_parts(parts).expect("an item of a known base stays one")
}

/// `item` blessed by `by`: "Blessed by Maren Holt."
pub fn bless(item: &Item, by: &str) -> Result<Item, Refusal> {
    can_bless(item)?;
    Ok(consecrate(item, BLESSED, format!("Blessed by {by}.")))
}

/// `item` hallowed by `by`, a cleric dedicated to `dedicated`, in place of any blessing it had.
pub fn hallow(item: &Item, by: &str, dedicated: armory::Family) -> Result<Item, Refusal> {
    can_hallow(item, dedicated)?;
    Ok(consecrate(item, HALLOWED, format!("Hallowed by {by}.")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roll_item;
    use armory::Family;

    fn mace() -> Item {
        roll_item("Mace", 7).expect("a mace")
    }

    #[test]
    fn a_blessed_mace_gains_the_affix_and_its_history() {
        let plain = mace();
        let blessed = bless(&plain, "Maren").unwrap();
        assert!(is_blessed(&blessed));
        assert!(blessed.name.starts_with("Blessed "), "{}", blessed.name);
        assert_eq!(blessed.history.last().map(String::as_str), Some("Blessed by Maren."));
        assert_eq!(blessed.rarity, plain.rarity);
        assert_eq!(blessed.profile.stats.damage_bonus, plain.profile.stats.damage_bonus + 1.0);
        assert_eq!(bless(&blessed, "Maren"), Err(Refusal::AlreadyBlessed));
    }

    #[test]
    fn a_dedicated_cleric_hallows_a_mace_and_it_hits_harder() {
        let plain = mace();
        let blessed = bless(&plain, "Maren").unwrap();
        let hallowed = hallow(&blessed, "Maren", Family::Mace).unwrap();
        assert!(is_hallowed(&hallowed) && !is_blessed(&hallowed));
        assert_eq!(hallowed.affixes.iter().filter(|a| a.holy).count(), 1);
        assert!(hallowed.profile.stats.damage_bonus > blessed.profile.stats.damage_bonus);
        assert_eq!(hallowed.history, ["Blessed by Maren.", "Hallowed by Maren."]);
        assert_eq!(hallow(&hallowed, "Maren", Family::Mace), Err(Refusal::AlreadyHallowed));
    }

    #[test]
    fn only_the_dedicated_kind_is_hallowed() {
        assert_eq!(hallow(&mace(), "Maren", Family::Sword), Err(Refusal::NotDedicated));
        let sword = roll_item("Sword", 3).unwrap();
        assert_eq!(hallow(&sword, "Maren", Family::Mace), Err(Refusal::NotDedicated));
        assert!(hallow(&sword, "Maren", Family::Sword).is_ok());
    }

    #[test]
    fn only_a_single_weapon_is_blessed() {
        let boots = roll_item("Boots", 3).unwrap();
        assert_eq!(bless(&boots, "Maren"), Err(Refusal::NotBlessable));
    }

    #[test]
    fn blessings_never_roll_or_sell() {
        for seed in 0..300 {
            for base in ["Mace", "Sword", "Staff"] {
                let item = crate::roll_item_with(base, seed, crate::Odds::loot(10)).unwrap();
                assert!(item.affixes.iter().all(|a| !a.holy));
            }
        }
        assert!(crate::enchant::addable(&mace(), crate::enchant::Tier::Master).iter().all(|a| !a.holy));
    }

    #[test]
    fn rebuilt_from_its_parts_a_hallowed_mace_is_the_same() {
        let hallowed = hallow(&mace(), "Maren", Family::Mace).unwrap();
        assert_eq!(Item::from_parts(ItemParts::from(hallowed.clone())).unwrap(), hallowed);
    }

    fn rare_mace() -> Item {
        (0..20_000).filter_map(|s| crate::roll_item_with("Mace", s, crate::Odds::loot(8))).find(|i| i.rarity == Rarity::Rare && i.affixes.iter().all(|a| a.good)).expect("a rare mace")
    }

    fn master() -> crate::enchant::Enchanter {
        crate::enchant::Enchanter { name: "Ysolde".into(), tier: crate::enchant::Tier::Master, faction: "the Ash Rite".into() }
    }

    #[test]
    fn an_enchanter_cannot_strip_a_blessing() {
        let blessed = bless(&rare_mace(), "Maren").unwrap();
        assert!(crate::enchant::removable(&blessed, crate::enchant::Tier::Master).iter().all(|a| !a.holy));
        let holy = blessed.affixes.iter().copied().find(|a| a.holy).unwrap();
        assert_eq!(crate::enchant::remove(&blessed, holy, &master()), Err(crate::enchant::Refusal::Blessed));
    }

    #[test]
    fn reforging_keeps_the_blessing() {
        let hallowed = hallow(&rare_mace(), "Maren", Family::Mace).unwrap();
        let again = crate::enchant::reforge(&hallowed, 5, &master()).unwrap();
        assert!(is_hallowed(&again));
        assert_eq!(again.rarity, hallowed.rarity.max(again.rarity));
        assert!(again.history.iter().any(|l| l.starts_with("Hallowed")));
    }

    #[test]
    fn a_blessing_adds_nothing_to_what_scrap_gives() {
        let plain = rare_mace();
        let blessed = bless(&plain, "Maren").unwrap();
        assert_eq!(crate::scrap::scrap(&blessed), crate::scrap::scrap(&plain));
    }
}
