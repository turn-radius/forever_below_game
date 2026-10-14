//! A whole Forever Below item from a base name and a seed: the plate's `rollSA` and
//! `makeGood`, tying together the shape, profile, rarity, affixes, name, legend and dress.

use crate::armory_base::roll_shape;
use crate::dress::dress;
use crate::naming::{Legend, item_name, legendize, noun};
use crate::profile::{Profile, profile};
use crate::rarity::roll_rarity;
use crate::tables::Affix;
use crate::{Kind, Rarity, game_rng};
use std::fmt;

/// D&D coinage in a purse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Coins {
    pub gp: u32,
    pub sp: u32,
    pub cp: u32,
}

impl fmt::Display for Coins {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} gp · {} sp · {} cp", self.gp, self.sp, self.cp)
    }
}

/// One rolled item.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub base: String,
    pub seed: u32,
    /// The Armory shape, dressed for its rarity and flaws.
    pub shape: armory::Item,
    pub rarity: Rarity,
    pub affixes: Vec<&'static Affix>,
    pub name: String,
    /// A legendary's title, old name and history.
    pub legend: Option<Legend>,
    /// Slot, tags, footprint and the like; its stats include the affixes.
    pub profile: Profile,
    /// What a purse or pouch holds.
    pub coins: Option<Coins>,
}

impl Item {
    /// The name shown first: a legendary's title, otherwise its name.
    pub fn title(&self) -> &str {
        self.legend.as_ref().map_or(&self.name, |l| &l.title)
    }
}

/// Rolls base `base` from `seed`. `None` for a name no table knows. Pure in its arguments.
///
/// Gear rolls rarity and affixes from its tags, applies the affixes to its stats, is named
/// and, if legendary, titled with a history, and is dressed for its rarity. Goods are common
/// and plain, as in the plate; a purse that rolled coins gets a count of them.
pub fn roll_item(base: &str, seed: u32) -> Option<Item> {
    let mut shape = roll_shape(base, seed)?;
    let mut profile = profile(base, &shape);
    let mut rng = game_rng(seed);
    let noun = noun(base, &shape);

    if profile.kind == Kind::Good {
        let coins = match &shape {
            armory::Item::Bags(s) if s.bag.coins => {
                let gp = (rng.next_f64() * rng.next_f64() * 14.0) as u32;
                let sp = rng.int(0, 29);
                let cp = rng.int(5, 64);
                Some(Coins { gp, sp, cp })
            }
            _ => None,
        };
        if let Some(c) = coins {
            profile.description = c.to_string();
        }
        return Some(Item { base: base.to_string(), seed, shape, rarity: Rarity::Common, affixes: vec![], name: noun, legend: None, profile, coins });
    }

    let (rarity, affixes) = roll_rarity(&mut rng, &profile.tags);
    for a in &affixes {
        a.apply_to_stats(&mut profile.stats);
    }
    let name = item_name(&noun, &affixes);
    let legend = (rarity == Rarity::Legendary).then(|| legendize(&mut rng, &name, &profile.tags));
    dress(&mut shape, rarity, &affixes, seed);
    Some(Item { base: base.to_string(), seed, shape, rarity, affixes, name, legend, profile, coins: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armory_base::ARMORY_BASES;
    use crate::Tag;

    #[test]
    fn every_base_rolls() {
        for (base, _) in ARMORY_BASES {
            for seed in [1, 2, 3, 1207, 65_537] {
                let item = roll_item(base, seed).unwrap_or_else(|| panic!("{base}"));
                assert_eq!(item, roll_item(base, seed).unwrap(), "{base} {seed}: not deterministic");
                assert!(!item.name.is_empty());
                for a in &item.affixes {
                    assert!(a.fits(&item.profile.tags), "{base}: {} on {:?}", a.name, item.profile.tags);
                }
                assert_eq!(item.legend.is_some(), item.rarity == Rarity::Legendary, "{base}");
                if item.profile.kind == Kind::Good {
                    assert_eq!((item.rarity, item.affixes.len()), (Rarity::Common, 0), "{base}");
                }
                if item.profile.tags.contains(&Tag::Container) {
                    assert_ne!(item.rarity, Rarity::Legendary, "{base}");
                }
            }
        }
        assert_eq!(roll_item("Spoon", 1), None);
    }

    #[test]
    fn affixes_show_in_name_stats_and_shape() {
        let (warm, base_warmth) = (1..2_000)
            .filter_map(|seed| {
                let item = roll_item("Cloak", seed)?;
                item.affixes.iter().any(|a| a.name == "Warm").then(|| {
                    let plain = crate::profile::profile("Cloak", &crate::armory_base::roll_shape("Cloak", seed).unwrap());
                    (item, plain.stats.warmth)
                })
            })
            .next()
            .expect("no warm cloak");
        assert!(warm.name.starts_with("Warm "));
        assert!(warm.profile.stats.warmth >= base_warmth + 2.0);

        let rusted = (1..3_000).filter_map(|seed| roll_item("Sword", seed)).find(|i| i.affixes.iter().any(|a| a.name == "Rusted")).expect("no rusted sword");
        assert_eq!(rusted.profile.stats.damage_bonus, -1.0);
        assert_eq!(rusted.shape.condition().rust, 1.5);
    }

    #[test]
    fn legendaries_have_titles() {
        let legend = (1..2_000).filter_map(|seed| roll_item("Sword", seed)).find(|i| i.rarity == Rarity::Legendary).expect("no legendary sword");
        let l = legend.legend.as_ref().unwrap();
        assert_eq!(legend.title(), l.title);
        assert_eq!(l.subtitle, legend.name);
    }

    #[test]
    fn purses_hold_coins() {
        for seed in 1..50 {
            let purse = roll_item("Purse", seed).unwrap();
            let coins = purse.coins.expect("a purse always holds coins");
            assert!(coins.gp < 14 && coins.sp < 30 && (5..65).contains(&coins.cp));
            assert_eq!(purse.profile.description, coins.to_string());
        }
    }
}
