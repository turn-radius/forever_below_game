//! What a container standing in a level holds: the container itself as an [`Item`] with its
//! inner grid, and its contents rolled from a loot seed.
//!
//! Neither the seed doc nor the plate fills a chest, so what each kind holds is our own
//! proposal: chests keep coin and gear, barrels and amphorae keep food and drink, crates keep
//! supplies. Each is a weighted table of bases, rolled through [`roll_item`] like any loot, so
//! gear among the contents takes rarity and affixes as it would anywhere.

use armory::families::chests::{ChestState, ChestStyle};
use procgen_rng::Mulberry32;

use crate::item::{Item, roll_item};
use crate::profile::profile;
use crate::Rarity;

/// The base a container standing in a level is (`data/goods.json` has none; `Chest` is the
/// plate's own Armory base for the family).
const CONTAINER_BASE: &str = "Chest";

/// A container's shape as an item: common, plain, too heavy to carry, and holding its grid
/// ([`crate::profile::chest_grid`]). Its shape is used as it is, shut or open.
pub fn container_item(state: ChestState) -> Item {
    let shape = armory::Item::Chests(state);
    let seed = shape.seed();
    let profile = profile(CONTAINER_BASE, &shape);
    let name = shape.noun();
    Item { base: CONTAINER_BASE.to_string(), seed, shape, rarity: Rarity::Common, affixes: vec![], name, legend: None, profile, coins: None, odds: crate::Odds::default() }
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
                ("Healing draught", 3),
                ("Candle", 2),
                ("Lockpicks", 1),
            ],
            (2, 5),
        ),
        ChestStyle::Strongbox | ChestStyle::Coffer => (&[("Purse", 6), ("Ring", 3), ("Amulet", 2), ("Charm", 2), ("Lockpicks", 1)], (1, 3)),
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
            ],
            (2, 4),
        ),
        ChestStyle::Amphora => (&[("Wine", 4), ("Potion", 3), ("Healing draught", 2), ("Lamp oil", 2), ("Purse", 1)], (1, 3)),
    }
}

/// What a container of `style` holds, rolled from `loot`: a few things from its kind's table,
/// each rolled from its own seed off the loot stream. Pure in its arguments.
pub fn roll_contents(style: ChestStyle, loot: u32) -> Vec<Item> {
    let mut rng = Mulberry32::new(loot ^ 0xC07E_5755);
    let (table, count) = stock(style);
    (0..rng.int(count.0, count.1))
        .filter_map(|_| {
            let base = *rng.weighted(table);
            let seed = (rng.next_f64() * 4_294_967_296.0) as u32;
            roll_item(base, seed)
        })
        .collect()
}

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
                let contents = roll_contents(style, loot);
                assert_eq!(contents, roll_contents(style, loot), "{style:?} {loot}");
                assert!((count.0..=count.1).contains(&(contents.len() as u32)), "{style:?} {loot}: {}", contents.len());
                assert!(contents.iter().all(|i| table.iter().any(|(b, _)| *b == i.base) && i.profile.packable), "{style:?}");
            }
        }
        assert_ne!(roll_contents(ChestStyle::Chest, 1), roll_contents(ChestStyle::Chest, 2));
    }

    #[test]
    fn chests_now_and_then_hold_treasure() {
        let rich = (1..200).filter(|&loot| holds_treasure(&roll_contents(ChestStyle::Chest, loot))).count();
        assert!(rich > 100, "only {rich} of 199 chests hold coin or magic");
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
}
