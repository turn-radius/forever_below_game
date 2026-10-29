//! What a container standing in a level holds: the container itself as an [`Item`] with its
//! inner grid, and its contents rolled from a loot seed.
//!
//! Neither the seed doc nor the plate fills a chest, so what each kind holds is our own
//! proposal: chests keep coin and gear, barrels and amphorae keep food and drink, crates keep
//! supplies. Each is a weighted table of bases, rolled through [`roll_item`] like any loot, so
//! gear among the contents takes rarity and affixes as it would anywhere. Chests keep quivers
//! and bolt cases, and chests and crates keep arrows and bolts by the sheaf ([`SHEAF`]), since
//! one arrow is no use to anyone and a bow shoots only from a quiver.

use armory::families::chests::{ChestState, ChestStyle};
use procgen_rng::Mulberry32;

use crate::armory_base::held_back;
use crate::item::{Item, roll_item_with};
use crate::profile::profile;
use crate::Rarity;

/// The base a container standing in a level is (`data/goods.json` has none; `Chest` is the
/// plate's own Armory base for the family).
const CONTAINER_BASE: &str = "Chest";

/// How many pieces one pick of ammunition is, inclusive: a sheaf tied together. Our own.
const SHEAF: (u32, u32) = (6, 12);

/// Whether a pick of `base` is a sheaf of [`SHEAF`] pieces rather than one thing.
fn by_the_sheaf(base: &str) -> bool {
    matches!(base, "Arrow" | "Bolt")
}

/// A container's shape as an item: common, plain, too heavy to carry, and holding its grid
/// ([`crate::profile::chest_grid`]). Its shape is used as it is, shut or open.
pub fn container_item(state: ChestState) -> Item {
    let shape = armory::Item::Chests(state);
    let seed = shape.seed();
    let profile = profile(CONTAINER_BASE, &shape);
    let name = shape.noun();
    Item { base: CONTAINER_BASE.to_string(), seed, shape, rarity: Rarity::Common, affixes: vec![], name, legend: None, profile, coins: None, odds: crate::Odds::default(), history: vec![], words: None, teaching: None, known: crate::identify::Known::Full, rest_tried: None, quantity: 1 }
}

/// What each kind of container holds, by weight, and how many things, inclusive.
fn stock(style: ChestStyle) -> (&'static [(&'static str, u32)], (u32, u32)) {
    match style {
        ChestStyle::Chest => (
            &[
                ("Purse", 6),
                ("Dagger", 3),
                ("Sword", 2),
                ("Hand axe", 2),
                ("Ring", 2),
                ("Amulet", 2),
                ("Charm", 2),
                ("Gloves", 2),
                ("Boots", 2),
                ("Hood", 2),
                ("Cloak", 2),
                ("Healing vial", 2),
                ("Small healing draught", 2),
                ("Healing draught", 1),
                ("Small potion", 1),
                ("Candle", 2),
                ("Lockpicks", 1),
                ("Quiver", 1),
                ("Bolt case", 1),
                ("Arrow", 2),
                ("Bolt", 1),
                // Rough gems: the common stones now and then, the dear ones rarely.
                ("Rough garnet", 1),
                ("Rough amethyst", 1),
                ("Rough citrine", 1),
                ("Rough sapphire", 1),
                ("Rough emerald", 1),
            ],
            (2, 5),
        ),
        ChestStyle::Strongbox | ChestStyle::Coffer => (
            &[
                ("Purse", 6),
                ("Ring", 3),
                ("Amulet", 2),
                ("Charm", 2),
                ("Lockpicks", 1),
                ("Rough garnet", 1),
                ("Rough amethyst", 1),
                ("Rough citrine", 1),
                ("Rough sapphire", 1),
                ("Rough emerald", 1),
                ("Rough ruby", 1),
                ("Rough diamond", 1),
            ],
            (1, 3),
        ),
        ChestStyle::Barrel => (
            &[("Bread", 4), ("Cheese", 3), ("Apple", 4), ("Smoked fish", 3), ("Sausages", 3), ("Tankard", 2), ("Wine", 2), ("Waterskin", 2)],
            (2, 5),
        ),
        ChestStyle::Crate => (
            &[
                ("Spare torch", 4),
                ("Rope", 3),
                ("Candle", 3),
                ("Lamp oil", 2),
                ("Bandages", 3),
                ("Waterskin", 2),
                ("Bread", 2),
                ("Shoes", 1),
                ("Trousers", 1),
                ("Hand axe", 1),
                ("Lantern", 1),
                ("Arrow", 3),
                ("Bolt", 2),
            ],
            (2, 4),
        ),
        ChestStyle::Amphora => (&[("Wine", 4), ("Vial", 2), ("Small potion", 2), ("Potion", 1), ("Healing vial", 1), ("Small healing draught", 1), ("Healing draught", 1), ("Large potion", 1), ("Lamp oil", 2), ("Purse", 1)], (1, 3)),
    }
}

/// The depth from which containers hold rough gems, and from which the dear stones (ruby, diamond,
/// sapphire, emerald) are among them: a depth-1 coffer never holds a 250 gp ruby. Our own.
pub const GEMS_FROM_DEPTH: u32 = 5;
pub const DEAR_GEMS_FROM_DEPTH: u32 = 9;

/// Whether a container at `depth` may hold `base`: anything but a rough gem always, a rough gem
/// from [`GEMS_FROM_DEPTH`], a dear one from [`DEAR_GEMS_FROM_DEPTH`].
fn gem_in_reach(base: &str, depth: u32) -> bool {
    use armory::gem::Gemstone::{Diamond, Emerald, Ruby, Sapphire};
    match crate::armory_base::rough_stone(base) {
        None => true,
        Some(Ruby | Diamond | Sapphire | Emerald) => depth >= DEAR_GEMS_FROM_DEPTH,
        Some(_) => depth >= GEMS_FROM_DEPTH,
    }
}

/// What a container of `style` holds, rolled from `loot`: a few picks from its kind's table,
/// each rolled from its own seed off the loot stream against `odds`, and ammunition by the
/// sheaf. Pure in its arguments.
pub fn roll_contents(style: ChestStyle, loot: u32, odds: crate::Odds) -> Vec<Item> {
    let mut rng = Mulberry32::new(loot ^ 0xC07E_5755);
    let (table, count) = stock(style);
    let table: Vec<(&str, u32)> = table.iter().copied().filter(|(base, _)| !held_back(base) && gem_in_reach(base, odds.depth)).collect();
    let mut contents = Vec::new();
    for _ in 0..rng.int(count.0, count.1) {
        let base = *rng.weighted(&table);
        let seed = (rng.next_f64() * 4_294_967_296.0) as u32;
        if by_the_sheaf(base) {
            // One kind of arrow, one seed, in stacks of the most that share a cell.
            let mut count = Mulberry32::new(seed).int(SHEAF.0, SHEAF.1);
            while count > 0 {
                let Some(stack) = roll_item_with(base, seed, odds) else { break };
                let n = count.min(stack.stack_limit());
                contents.push(stack.with_quantity(n));
                count -= n;
            }
        } else {
            contents.extend(roll_item_with(base, seed, odds));
        }
    }
    contents
}

/// What an urn in a wall niche holds, from the niche's `seed`: nothing at all, a few coins in a
/// purse or a ring, each rolled through [`roll_item_with`] against `odds` like any loot. (The
/// ashes of the dead are not a good of ours, so an urn holds none.) Pure in its arguments.
pub fn roll_urn_contents(seed: u32, odds: crate::Odds) -> Vec<Item> {
    let mut rng = Mulberry32::new(seed ^ 0x0A5E_5BE1);
    let base = match rng.next_f64() {
        p if p < URN_EMPTY => return Vec::new(),
        p if p < URN_EMPTY + URN_COINS => "Purse",
        _ => "Ring",
    };
    let item_seed = (rng.next_f64() * 4_294_967_296.0) as u32;
    roll_item_with(base, item_seed, odds).into_iter().collect()
}

/// The shares of urns that hold nothing and that hold coin; the rest hold a ring.
const URN_EMPTY: f64 = 0.3;
const URN_COINS: f64 = 0.45;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armory_base::armory_base;
    use crate::Kind;
    use armory::families::chests::Chests;

    /// Whether any of `contents` is coin or gear above common.
    fn holds_treasure(contents: &[Item]) -> bool {
        contents.iter().any(|i| (i.profile.kind == Kind::Gear && i.rarity != Rarity::Common) || i.coins.is_some())
    }

    #[test]
    fn every_stocked_base_is_a_base() {
        for style in ChestStyle::ALL {
            for (base, _) in stock(style).0 {
                assert!(armory_base(base).is_some(), "{style:?}: {base}");
            }
        }
    }

    #[test]
    fn contents_are_deterministic_and_fit_their_kind() {
        for style in ChestStyle::ALL {
            let (table, count) = stock(style);
            for loot in [1, 1207, 65_537, u32::MAX] {
                let contents = roll_contents(style, loot, crate::Odds::default());
                assert_eq!(contents, roll_contents(style, loot, crate::Odds::default()), "{style:?} {loot}");
                assert!((count.0..=count.1 * SHEAF.1).contains(&contents.iter().map(|i| i.quantity).sum::<u32>()), "{style:?} {loot}: {}", contents.len());
                assert!(contents.iter().all(|i| table.iter().any(|(b, _)| *b == i.base) && i.profile.packable), "{style:?}");
            }
        }
        assert_ne!(roll_contents(ChestStyle::Chest, 1, crate::Odds::default()), roll_contents(ChestStyle::Chest, 2, crate::Odds::default()));
    }

    /// Whether any chest or crate of `seeds` loot seeds, at each depth from 1 to 10, holds a
    /// held-back base.
    fn holds_held_back(seeds: u32) -> bool {
        (1..=10).any(|depth| {
            [ChestStyle::Chest, ChestStyle::Crate].into_iter().any(|style| {
                (0..seeds).any(|loot| roll_contents(style, loot, crate::Odds::loot(depth)).iter().any(|i| held_back(&i.base)))
            })
        })
    }

    #[test]
    fn no_crate_holds_a_held_back_base_smoke() {
        assert!(!holds_held_back(3));
    }

    #[test]
    #[ignore = "slow sweep: 300 loot seeds at each depth; run before merging"]
    fn no_crate_holds_a_held_back_base() {
        assert!(!holds_held_back(300));
    }

    #[test]
    fn chests_now_and_then_hold_treasure() {
        let rich = (1..200).filter(|&loot| holds_treasure(&roll_contents(ChestStyle::Chest, loot, crate::Odds::default()))).count();
        assert!(rich > 100, "only {rich} of 199 chests hold coin or magic");
    }

    #[test]
    fn chests_and_crates_now_and_then_hold_something_to_shoot() {
        let shoots = |style: ChestStyle, base: &str| (1..200).filter(|&loot| roll_contents(style, loot, crate::Odds::default()).iter().any(|i| i.base == base)).count();
        for (style, base) in [(ChestStyle::Chest, "Quiver"), (ChestStyle::Chest, "Bolt case"), (ChestStyle::Chest, "Arrow"), (ChestStyle::Crate, "Arrow"), (ChestStyle::Crate, "Bolt")] {
            let found = shoots(style, base);
            assert!((10..120).contains(&found), "{style:?}: {base} in {found} of 199");
        }
    }

    #[test]
    fn arrows_and_bolts_come_by_the_sheaf() {
        for loot in 1..200 {
            let contents = roll_contents(ChestStyle::Crate, loot, crate::Odds::default());
            let arrows = contents.iter().filter(|i| i.base == "Arrow").map(|i| i.quantity).sum::<u32>();
            assert!(arrows == 0 || arrows >= SHEAF.0, "crate {loot}: {arrows} arrows");
        }
    }

    #[test]
    fn a_container_item_opens_into_its_kind_of_grid() {
        for style in ChestStyle::ALL {
            let item = container_item(Chests::roll_as(9, style));
            let grid = item.profile.holds.as_ref().unwrap();
            let (cols, rows, label) = crate::profile::chest_grid(style);
            assert_eq!((grid.cols, grid.rows, grid.label.as_str()), (cols, rows, label));
            assert!(!item.profile.packable && item.rarity == Rarity::Common);
            assert_eq!(item.shape, armory::Item::Chests(Chests::roll_as(9, style)));
        }
    }

    #[test]
    fn chests_and_strongboxes_now_and_then_hold_a_rough_gem() {
        for style in [ChestStyle::Chest, ChestStyle::Strongbox] {
            let found = (1..300).filter(|&loot| roll_contents(style, loot, crate::Odds::loot(9)).iter().any(|i| crate::armory_base::rough_stone(&i.base).is_some())).count();
            assert!((10..150).contains(&found), "{style:?}: a rough gem in {found} of 299");
        }
        // Shallow containers never hold them, and the dear stones wait for depth 9.
        for depth in 1..GEMS_FROM_DEPTH {
            for style in ChestStyle::ALL {
                assert!((0..300).all(|loot| roll_contents(style, loot, crate::Odds::loot(depth)).iter().all(|i| crate::armory_base::rough_stone(&i.base).is_none())), "{style:?} at depth {depth}");
            }
        }
        for depth in GEMS_FROM_DEPTH..DEAR_GEMS_FROM_DEPTH {
            let dear = (0..400).any(|loot| [ChestStyle::Chest, ChestStyle::Strongbox].iter().any(|&st| roll_contents(st, loot, crate::Odds::loot(depth)).iter().any(|i| matches!(i.base.as_str(), "Rough ruby" | "Rough diamond" | "Rough sapphire" | "Rough emerald"))));
            assert!(!dear, "a dear stone at depth {depth}");
            assert!((0..400).any(|loot| roll_contents(ChestStyle::Chest, loot, crate::Odds::loot(depth)).iter().any(|i| crate::armory_base::rough_stone(&i.base).is_some())), "no gem at depth {depth}");
        }
        // Barrels hold food and no stone.
        assert!((1..200).all(|loot| roll_contents(ChestStyle::Barrel, loot, crate::Odds::default()).iter().all(|i| crate::armory_base::rough_stone(&i.base).is_none())));
    }

    #[test]
    fn an_urn_holds_nothing_coins_or_a_ring_and_the_same_every_time() {
        let (mut empty, mut purses, mut rings) = (0, 0, 0);
        for seed in 0..300u32 {
            let a = roll_urn_contents(seed.wrapping_mul(2_654_435_761), crate::Odds::default());
            assert_eq!(a, roll_urn_contents(seed.wrapping_mul(2_654_435_761), crate::Odds::default()));
            match a.as_slice() {
                [] => empty += 1,
                [one] if one.coins.is_some() => purses += 1,
                [one] if one.base == "Ring" => rings += 1,
                other => panic!("{other:?}"),
            }
        }
        assert!(empty > 40 && purses > 80 && rings > 40, "{empty} empty, {purses} purses, {rings} rings");
    }
}
