//! What an item is worth in coins: its base's price in `data/values.json` (copper), times its
//! rarity, its affixes and its condition. See `docs/items.md`, "Value".

use crate::item::{Coins, Item};
use crate::{Rarity, tables::Affix};
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// `data/values.json`: each base's price in copper when new and common.
static VALUES: LazyLock<BTreeMap<String, u32>> =
    LazyLock::new(|| serde_json::from_str(include_str!("../../../data/values.json")).unwrap_or_else(|e| panic!("data/values.json: {e}")));

/// A base's price in copper, new and common; `None` for a name the table does not know.
pub fn base_copper(base: &str) -> Option<u32> {
    VALUES.get(base).copied()
}

/// What rarity multiplies the price by (our own).
pub fn rarity_factor(rarity: Rarity) -> f64 {
    match rarity {
        Rarity::Common => 1.0,
        Rarity::Magic => 4.0,
        Rarity::Rare => 20.0,
        Rarity::Legendary => 200.0,
    }
}

/// What one affix multiplies the price by: a good one by more the rarer the tier it rolls from,
/// a flaw by less than one (our own).
pub fn affix_factor(affix: &Affix) -> f64 {
    if !affix.good {
        return 0.7;
    }
    match affix.least {
        Rarity::Common => 1.15,
        Rarity::Magic => 1.3,
        Rarity::Rare => 1.5,
        Rarity::Legendary => 1.8,
    }
}

/// What wear and damage leave of the price: wear 0.7 leaves 58%, a snapped blade a quarter.
pub fn condition_factor(c: &armory::cond::Condition) -> f64 {
    let f = (1.0 - 0.6 * f64::from(c.wear.clamp(0.0, 1.0))) * (1.0 - 0.4 * f64::from(c.damage.clamp(0.0, 1.0)));
    if c.broken { f * 0.25 } else { f }
}

impl Item {
    /// What it is worth, in copper: the base's price, times rarity, affixes and condition, at
    /// least 1 cp. A container's value leaves out what it holds, and a purse is worth its coins.
    pub fn value_copper(&self) -> u64 {
        if let Some(coins) = self.coins {
            return coins.total_copper();
        }
        let base = f64::from(base_copper(&self.base).unwrap_or(1));
        let affixes: f64 = self.affixes.iter().map(|a| affix_factor(a)).product();
        let worth = base * rarity_factor(self.rarity) * affixes * condition_factor(&self.shape.condition());
        (worth.round() as u64).max(1)
    }

    /// [`Item::value_copper`] in coins, with change.
    pub fn value(&self) -> Coins {
        Coins::from_copper(self.value_copper())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armory_base::ARMORY_BASES;
    use crate::{roll_item, roll_item_with, Odds};

    #[test]
    fn every_base_has_a_price() {
        for (base, _) in ARMORY_BASES {
            assert!(base_copper(base).is_some(), "{base} has no price in data/values.json");
        }
    }

    #[test]
    fn a_common_torch_is_a_few_copper() {
        let torch = (0..200).filter_map(|s| roll_item("Torch", s)).find(|i| i.rarity == Rarity::Common && i.affixes.is_empty()).unwrap();
        assert_eq!(torch.value(), Coins { gp: 0, sp: 0, cp: 1 });
        let lit_flawed = (0..400).filter_map(|s| roll_item("Torch", s)).find(|i| i.rarity == Rarity::Common).unwrap();
        assert!(lit_flawed.value_copper() < 10);
    }

    #[test]
    fn a_rare_sword_is_gold_and_wear_cuts_it() {
        let mut sword = (0..2_000).filter_map(|s| roll_item_with("Sword", s, Odds::loot(3))).find(|i| i.rarity == Rarity::Rare).unwrap();
        assert!(sword.value().gp >= 100, "{}", sword.value());
        let mut c = sword.shape.condition();
        c.wear = 0.0;
        c.damage = 0.0;
        sword.shape.set_condition(c);
        let pristine = sword.value_copper();
        c.wear = 0.7;
        sword.shape.set_condition(c);
        let worn = sword.value_copper();
        assert!(worn < pristine && worn as f64 > 0.5 * pristine as f64 * 0.9, "{pristine} -> {worn}");
        c.broken = true;
        sword.shape.set_condition(c);
        assert!(sword.value_copper() < worn / 3);
    }

    #[test]
    fn a_plain_sword_is_fifteen_gold_and_a_flaw_lowers_it() {
        let plain = (0..200).filter_map(|s| roll_item("Sword", s)).find(|i| i.rarity == Rarity::Common && i.affixes.is_empty()).unwrap();
        let mut c = plain.shape.condition();
        let mut sword = plain.clone();
        c.wear = 0.0;
        c.damage = 0.0;
        sword.shape.set_condition(c);
        assert_eq!(sword.value(), Coins { gp: 15, sp: 0, cp: 0 });
        let flaw = (0..2_000).filter_map(|s| roll_item("Sword", s)).find(|i| i.rarity == Rarity::Common && i.affixes.len() == 1 && !i.affixes[0].good).unwrap();
        assert!(flaw.value_copper() < 1500);
    }

    #[test]
    fn coins_make_change_and_fail_when_short() {
        let two_gp = Coins { gp: 2, sp: 0, cp: 0 };
        let fifteen_sp = Coins { gp: 0, sp: 15, cp: 0 };
        assert_eq!(two_gp.minus(fifteen_sp), Some(Coins { gp: 0, sp: 5, cp: 0 }));
        assert_eq!(fifteen_sp.minus(two_gp), None);
        assert_eq!(Coins { gp: 0, sp: 9, cp: 15 } + Coins { gp: 0, sp: 1, cp: 5 }, Coins { gp: 1, sp: 2, cp: 0 });
        assert_eq!([two_gp, fifteen_sp, Coins { gp: 0, sp: 0, cp: 7 }].iter().sum::<Coins>().total_copper(), 357);
        assert_eq!(Coins { gp: 1, sp: 5, cp: 0 }.short(), "1 gp 5 sp");
        assert_eq!(Coins::default().short(), "0 cp");
    }

    #[test]
    fn a_purse_is_worth_its_coins() {
        let purse = roll_item("Purse", 4).unwrap();
        assert_eq!(purse.value().total_copper(), purse.coins.unwrap().total_copper());
    }
}
