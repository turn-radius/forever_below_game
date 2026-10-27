//! What an item is worth in coins: its base's price in `data/values.json` (copper), times its
//! rarity, its affixes and its condition. See `docs/items.md`, "Value".

use crate::item::{Coins, Item};
use crate::{Rarity, tables::Affix};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// `data/values.json`: each base's price in copper when new and common.
static VALUES: LazyLock<BTreeMap<String, u32>> =
    LazyLock::new(|| serde_json::from_str(include_str!("../../../data/values.json")).unwrap_or_else(|e| panic!("data/values.json: {e}")));

/// A base's price in copper, new and common; `None` for a name the table does not know.
pub fn base_copper(base: &str) -> Option<u32> {
    VALUES.get(base).copied()
}

/// One factor per rarity.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ByRarity {
    pub common: f64,
    pub magic: f64,
    pub rare: f64,
    pub legendary: f64,
}

impl ByRarity {
    fn of(&self, rarity: Rarity) -> f64 {
        match rarity {
            Rarity::Common => self.common,
            Rarity::Magic => self.magic,
            Rarity::Rare => self.rare,
            Rarity::Legendary => self.legendary,
        }
    }
}

/// What one affix multiplies the price by.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct AffixRules {
    /// A flaw's factor, below one.
    pub flaw: f64,
    /// A good affix's factor by the least rarity it rolls on.
    pub good_by_least_rarity: ByRarity,
}

/// The least and most all of an item's affixes may multiply the price by together.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct AffixProduct {
    pub min: f64,
    pub max: f64,
}

/// How wear and damage cut the price.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ConditionRules {
    /// The share of the price full wear takes.
    pub wear: f64,
    /// The share of the price full damage takes.
    pub damage: f64,
    /// What a snapped blade keeps of the rest.
    pub broken: f64,
}

/// Every factor of the price, from `data/value_rules.json`, so the trade lab can tune them.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ValueRules {
    pub rarity: ByRarity,
    pub affix: AffixRules,
    pub affix_product: AffixProduct,
    pub condition: ConditionRules,
    /// How much richer each level below the first makes a purse: its coins times
    /// `1 + step * (depth - 1)`.
    pub coin_depth_step: f64,
}

/// `data/value_rules.json`.
pub static RULES: LazyLock<ValueRules> =
    LazyLock::new(|| serde_json::from_str(include_str!("../../../data/value_rules.json")).unwrap_or_else(|e| panic!("data/value_rules.json: {e}")));

/// What rarity multiplies the price by (our own).
pub fn rarity_factor(rarity: Rarity) -> f64 {
    RULES.rarity.of(rarity)
}

/// What one affix multiplies the price by: a good one by more the rarer the tier it rolls from,
/// a flaw by less than one (our own). [`affix_product`] caps them together.
pub fn affix_factor(affix: &Affix) -> f64 {
    if !affix.good {
        return RULES.affix.flaw;
    }
    RULES.affix.good_by_least_rarity.of(affix.least)
}

/// What all of an item's affixes multiply the price by, held between the rules' `min` and `max`
/// so a heap of good affixes cannot run the top end away.
pub fn affix_product(affixes: &[&Affix]) -> f64 {
    affixes.iter().map(|a| affix_factor(a)).product::<f64>().clamp(RULES.affix_product.min, RULES.affix_product.max)
}

/// What wear and damage leave of the price: wear 0.7 leaves 58%, a snapped blade a quarter.
pub fn condition_factor(c: &armory::cond::Condition) -> f64 {
    let r = &RULES.condition;
    let f = (1.0 - r.wear * f64::from(c.wear.clamp(0.0, 1.0))) * (1.0 - r.damage * f64::from(c.damage.clamp(0.0, 1.0)));
    if c.broken { f * r.broken } else { f }
}

/// How many times richer a purse is at `depth`: 1 on the first level.
pub fn coin_depth_factor(depth: u32) -> f64 {
    1.0 + RULES.coin_depth_step * f64::from(depth.saturating_sub(1))
}

/// What a brew's price is of its size's plain potion (our own): healing and mana at the plain
/// price, an antidote half, insight half as much again, an elixir and holy water the plain price.
pub fn brew_factor(brew: armory::families::food::Brew) -> f64 {
    use armory::families::food::Brew;
    match brew {
        Brew::Antidote => 0.5,
        Brew::Insight => 1.5,
        Brew::Healing | Brew::Mana | Brew::Elixir | Brew::HolyWater => 1.0,
    }
}

impl Item {
    /// What it is worth, in copper: the base's price, times rarity, affixes and condition, at
    /// least 1 cp. A container's value leaves out what it holds, and a purse is worth its coins.
    pub fn value_copper(&self) -> u64 {
        if let Some(coins) = self.coins {
            return coins.total_copper();
        }
        let base = f64::from(base_copper(&self.base).unwrap_or(1));
        // A potion is worth its brew at its size, whichever base rolled it.
        let base = match (self.potion_size(), self.brew()) {
            (Some(size), Some(brew)) => f64::from(base_copper(crate::provision::plain_base(size)).unwrap_or(1)) * brew_factor(brew),
            _ => base,
        };
        // Until it is fully known an item is worth what a plain one is: no one pays for magic
        // that has not been found out.
        let (rarity, affixes): (_, &[&Affix]) = if self.is_known() { (self.rarity, &self.affixes) } else { (Rarity::Common, &[]) };
        let worth = base * rarity_factor(rarity) * affix_product(affixes) * condition_factor(&self.shape.condition());
        (worth.round() as u64).max(1)
    }

    /// What the whole stack is worth, in copper: [`Item::value_copper`] for each piece.
    pub fn stack_value_copper(&self) -> u64 {
        self.value_copper() * u64::from(self.quantity)
    }

    /// [`Item::stack_value_copper`] in coins, with change.
    pub fn stack_value(&self) -> Coins {
        Coins::from_copper(self.stack_value_copper())
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
        assert!(sword.value().gp >= 30, "{}", sword.value());
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

    /// The pristine price of every `rarity` sword among `seeds` seeds at `odds`.
    fn sword_prices(rarity: Rarity, odds: Odds, seeds: u32) -> Vec<u64> {
        (0..seeds)
            .filter_map(|s| roll_item_with("Sword", s, odds))
            .filter(|i| i.rarity == rarity)
            .map(|mut i| {
                let mut c = i.shape.condition();
                c.wear = 0.0;
                c.damage = 0.0;
                c.broken = false;
                i.shape.set_condition(c);
                i.value_copper()
            })
            .collect()
    }

    #[test]
    fn a_master_rare_sword_costs_a_few_hundred_gold_at_most() {
        let master = Odds { depth: 8, cap: Rarity::Rare };
        let prices = sword_prices(Rarity::Rare, master, 400);
        assert!(prices.len() > 20);
        let max = *prices.iter().max().unwrap();
        assert!(max <= 300 * 100, "{} gp", max / 100);
        assert!(*prices.iter().min().unwrap() >= 20 * 100);
    }

    #[test]
    fn a_magic_piece_is_a_small_multiple_of_its_common_base() {
        let common = base_copper("Sword").unwrap() as f64;
        let prices = sword_prices(Rarity::Magic, Odds::loot(4), 300);
        assert!(prices.len() > 20);
        let mean = prices.iter().sum::<u64>() as f64 / prices.len() as f64;
        assert!(mean <= 3.0 * common, "mean {mean} against {common}");
        assert!(prices.iter().all(|&p| p as f64 <= 4.0 * common), "{:?}", prices.iter().max());
    }

    #[test]
    fn a_legendary_piece_is_bounded() {
        let prices = sword_prices(Rarity::Legendary, Odds::loot(20), 600);
        assert!(!prices.is_empty());
        let top = rarity_factor(Rarity::Legendary) * RULES.affix_product.max * f64::from(base_copper("Sword").unwrap());
        assert!(prices.iter().all(|&p| p as f64 <= top + 1.0), "{:?} against {top}", prices.iter().max());
        assert!(top <= 1_000.0 * 100.0);
    }

    #[test]
    fn the_affix_product_is_held_between_its_bounds() {
        let good = |least| Affix { good: true, least, ..sample_affix() };
        let heap: Vec<Affix> = (0..12).map(|_| good(Rarity::Legendary)).collect();
        assert_eq!(affix_product(&heap.iter().collect::<Vec<_>>()), RULES.affix_product.max);
        let flaws: Vec<Affix> = (0..12).map(|_| Affix { good: false, ..sample_affix() }).collect();
        assert_eq!(affix_product(&flaws.iter().collect::<Vec<_>>()), RULES.affix_product.min);
        assert_eq!(affix_product(&[]), 1.0);
        assert!(affix_product(&[&good(Rarity::Magic)]) > 1.0);
    }

    #[test]
    fn values_are_deterministic() {
        for seed in 0..50 {
            let a = roll_item_with("Mail", seed, Odds::loot(6)).unwrap();
            let b = roll_item_with("Mail", seed, Odds::loot(6)).unwrap();
            assert_eq!(a.value_copper(), b.value_copper());
        }
    }

    #[test]
    fn purses_are_richer_deeper_and_unchanged_on_the_first_level() {
        let first = roll_item_with("Purse", 4, Odds::loot(1)).unwrap().coins.unwrap().total_copper();
        assert_eq!(first, roll_item("Purse", 4).unwrap().coins.unwrap().total_copper());
        let deep = roll_item_with("Purse", 4, Odds::loot(9)).unwrap().coins.unwrap().total_copper();
        assert!(deep as f64 > first as f64 * 2.5, "{first} -> {deep}");
    }

    /// Any affix, for a test to flip `good` and `least` on.
    fn sample_affix() -> Affix {
        (0..500).filter_map(|s| roll_item_with("Sword", s, Odds::loot(6))).find_map(|i| i.affixes.first().map(|a| (*a).clone())).unwrap()
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
