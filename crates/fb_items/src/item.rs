//! A whole Forever Below item from a base name and a seed: the plate's `rollSA` and
//! `makeGood`, tying together the shape, profile, rarity, affixes, name, legend and dress.

use crate::armory_base::{base_for, roll_shape};
use crate::dress::dress;
use crate::naming::{Legend, item_name, legendize, noun};
use crate::profile::{Profile, profile};
use crate::rarity::{Odds, roll_rarity};
use crate::tables::Affix;
use crate::{Kind, Rarity, Slot, Tag, game_rng};
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
    /// The odds its rarity was rolled against, so its base and seed roll it again the same.
    pub odds: Odds,
}

impl Item {
    /// The name shown first: a legendary's title, otherwise its name.
    pub fn title(&self) -> &str {
        self.legend.as_ref().map_or(&self.name, |l| &l.title)
    }

    /// Every slot it can be worn or held in, the one it is made for first; none for goods. A
    /// ring goes on either ring slot (the plate's `targetSlot`), and a weapon in either hand,
    /// the main hand first (the user's call: the plate held weapons only in the main hand). A
    /// lantern goes in either hand, the off hand first, or hangs from the belt at the hip
    /// (the user's call), whose belt the inventory checks.
    pub fn slots(&self) -> Vec<Slot> {
        let Some(made_for) = self.profile.slot else { return vec![] };
        if self.shape.family() == armory::Family::Lantern {
            return vec![made_for, Slot::Mainhand, Slot::Hip];
        }
        match made_for {
            Slot::Ring1 | Slot::Ring2 => vec![Slot::Ring1, Slot::Ring2],
            Slot::Mainhand if self.profile.tags.contains(&Tag::Weapon) => vec![Slot::Mainhand, Slot::Offhand],
            _ => vec![made_for],
        }
    }

    /// Game seconds a full torch or lantern of this item burns: one hour for a torch and six for
    /// a lantern, times its `BurnTime`; `None` for anything else.
    pub fn fuel_capacity(&self) -> Option<f32> {
        self.shape.fuel_capacity().map(|seconds| seconds * self.profile.stats.burn_time)
    }

    /// Game seconds of burning it has left; `None` unless it is a torch or a lantern.
    pub fn fuel_left(&self) -> Option<f32> {
        self.shape.fuel_left()
    }

    /// Burns `seconds` of game time off it if it is lit ([`armory::Item::burn`]).
    pub fn burn(&mut self, seconds: f32) {
        if let Some(capacity) = self.fuel_capacity() {
            self.shape.burn(seconds, capacity.max(1.0));
        }
    }

    /// Whether it can be worn or held in `slot` ([`Item::slots`]).
    pub fn fits(&self, slot: Slot) -> bool {
        self.slots().contains(&slot)
    }
}

/// Rolls base `base` from `seed` at the first level's odds ([`roll_item_with`]).
pub fn roll_item(base: &str, seed: u32) -> Option<Item> {
    roll_item_with(base, seed, Odds::default())
}

/// Rolls base `base` from `seed`, its rarity against `odds`. `None` for a name no table knows.
/// Pure in its arguments.
///
/// Gear rolls rarity and affixes from its tags, applies the affixes to its stats, is named
/// and, if legendary, titled with a history, and is dressed for its rarity. Goods are common
/// and plain, as in the plate; a purse that rolled coins gets a count of them.
pub fn roll_item_with(base: &str, seed: u32, odds: Odds) -> Option<Item> {
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
        return Some(Item { base: base.to_string(), seed, shape, rarity: Rarity::Common, affixes: vec![], name: noun, legend: None, profile, coins, odds });
    }

    let (rarity, affixes) = roll_rarity(&mut rng, &profile.tags, &odds);
    for a in &affixes {
        a.apply_to_stats(&mut profile.stats);
    }
    let name = item_name(&noun, &affixes);
    let legend = (rarity == Rarity::Legendary).then(|| legendize(&mut rng, &name, &profile.tags));
    dress(&mut shape, rarity, &affixes, seed);
    // The fuel it starts with is what the item's `BurnTime` makes of a full one.
    shape.scale_fuel(profile.stats.burn_time);
    Some(Item { base: base.to_string(), seed, shape, rarity, affixes, name, legend, profile, coins: None, odds })
}

/// Loot that names only an Armory family and a seed (the level's item spots): a base of that
/// family chosen from the seed ([`base_for`]), rolled from the same seed.
pub fn roll_loot(family: armory::Family, seed: u32) -> Option<Item> {
    roll_item(base_for(family, seed)?, seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armory_base::ARMORY_BASES;

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
    fn the_same_base_seed_and_depth_roll_the_same_item() {
        for depth in [1, 3, 5, 8] {
            for seed in [1, 2, 1207, 65_537] {
                let item = roll_item_with("Sword", seed, Odds::loot(depth)).unwrap();
                assert_eq!(item, roll_item_with("Sword", seed, Odds::loot(depth)).unwrap(), "depth {depth} seed {seed}");
                assert_eq!(item.odds, Odds::loot(depth));
            }
        }
        for seed in 0..300 {
            assert_eq!(roll_item("Sword", seed), roll_item_with("Sword", seed, Odds::loot(1)), "{seed}");
        }
    }

    /// A fresh torch whose `BurnTime` is `burn_time`, with the fuel that makes of a full one.
    fn torch_burning(burn_time: f32) -> Item {
        let mut item = roll_item("Spare torch", 3).unwrap();
        item.profile.stats.burn_time = burn_time;
        item.shape.scale_fuel(burn_time);
        item.shape.set_lit(true).unwrap();
        item
    }

    fn state(item: &Item) -> armory::families::torch::BurnState {
        let armory::Item::Torch(s) = &item.shape else { panic!("not a torch") };
        s.burn.state
    }

    #[test]
    fn a_plain_torch_lit_for_an_hour_is_spent() {
        use armory::families::torch::BurnState;
        let mut torch = torch_burning(1.0);
        assert_eq!(torch.fuel_left(), Some(3_600.0));
        torch.burn(3_599.0);
        assert_eq!(state(&torch), BurnState::Burning);
        torch.burn(1.0);
        assert_eq!((state(&torch), torch.fuel_left()), (BurnState::Spent, Some(0.0)));
    }

    #[test]
    fn a_long_burning_torch_burns_for_two_hours() {
        use armory::families::torch::BurnState;
        let mut torch = torch_burning(2.0);
        torch.burn(3_600.0);
        assert_eq!((state(&torch), torch.fuel_left()), (BurnState::Burning, Some(3_600.0)));
        torch.burn(3_600.0);
        assert_eq!(state(&torch), BurnState::Spent);
    }

    #[test]
    fn dousing_at_twenty_minutes_and_relighting_leaves_forty() {
        let mut torch = torch_burning(1.0);
        torch.burn(20.0 * 60.0);
        torch.shape.set_lit(false).unwrap();
        torch.burn(10.0 * 3_600.0);
        torch.shape.set_lit(true).unwrap();
        assert_eq!(torch.fuel_left(), Some(40.0 * 60.0));
    }

    #[test]
    fn the_char_follows_the_fuel_used() {
        let mut torch = torch_burning(1.0);
        let armory::Item::Torch(before) = &torch.shape else { panic!() };
        let before = before.burn.char;
        torch.burn(1_800.0);
        let armory::Item::Torch(after) = &torch.shape else { panic!() };
        assert!(after.burn.char > before && after.burn.char <= 1.0);
        assert!((after.burn.char - 0.5).abs() < 1e-3);
    }

    #[test]
    fn fuel_left_survives_a_serde_round_trip() {
        let mut torch = torch_burning(1.0);
        torch.burn(1_234.0);
        let json = serde_json::to_string(&torch.shape).unwrap();
        let back: armory::Item = serde_json::from_str(&json).unwrap();
        assert_eq!(back.fuel_left(), Some(3_600.0 - 1_234.0));
        let mut lantern = roll_item("Lantern", 7).unwrap();
        lantern.shape.set_lit(true).unwrap();
        lantern.burn(100.0);
        let back: armory::Item = serde_json::from_str(&serde_json::to_string(&lantern.shape).unwrap()).unwrap();
        assert_eq!(back.fuel_left(), lantern.fuel_left());
    }

    #[test]
    fn a_rolled_light_starts_with_its_burn_time_of_fuel() {
        let torch = roll_item("Spare torch", 3).unwrap();
        assert_eq!(torch.fuel_left(), Some(3_600.0));
        assert_eq!(torch.fuel_capacity(), Some(3_600.0));
        assert_eq!(roll_item("Sword", 1).unwrap().fuel_left(), None);
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
    fn loot_rolls_a_base_of_its_family() {
        for &family in armory::Item::PORTED {
            for seed in [5, 1207] {
                let item = roll_loot(family, seed).unwrap();
                assert_eq!(item.shape.family(), family);
                assert_eq!(Some(item.clone()), roll_loot(family, seed));
            }
        }
    }

    #[test]
    fn every_weapon_goes_in_either_hand_and_nothing_else_does() {
        for base in ["Sword", "Dagger", "Mace", "Club", "Hand axe", "Spear", "Staff", "Bow", "Crossbow"] {
            for seed in [1, 7, 1207] {
                let item = roll_item(base, seed).unwrap();
                assert_eq!(item.slots(), vec![Slot::Mainhand, Slot::Offhand], "{base} {seed}");
                assert!(item.fits(Slot::Offhand) && !item.fits(Slot::Belt), "{base} {seed}");
            }
        }
        assert_eq!(roll_item("Shield", 1).unwrap().slots(), vec![Slot::Offhand]);
        assert_eq!(roll_item("Helm", 1).unwrap().slots(), vec![Slot::Head]);
        assert_eq!(roll_item("Ring", 1).unwrap().slots(), vec![Slot::Ring1, Slot::Ring2]);
        assert!(roll_item("Bread", 1).unwrap().slots().is_empty());
        assert_eq!(roll_item("Lantern", 1).unwrap().slots(), vec![Slot::Offhand, Slot::Mainhand, Slot::Hip]);
        assert!(!roll_item("Torch", 1).unwrap().fits(Slot::Hip));
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
