//! The delver's enchanting resources for one delve: what scrapping gear gave them
//! (`fb_items::scrap`), kept until the run ends. The enchanter's talk (#350) calls
//! [`scrap_item`] and spends the stock.

use crate::inventory::{Inventory, ItemId};
use bevy::prelude::*;
use fb_items::scrap::{EnchantingStock, Salvage, scrap};

/// The delver's essence and materials. Reset with the run (`travel::begin_the_run`).
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct Enchanting(pub EnchantingStock);

/// Scraps item `id` into `stock`: the item is gone and its salvage is added. `None`, and
/// nothing changes, for what is not gear or a bag with something in it.
pub fn scrap_item(inventory: &mut Inventory, stock: &mut EnchantingStock, id: ItemId) -> Option<Salvage> {
    if inventory.contents_count(id) > 0 {
        return None;
    }
    let salvage = scrap(inventory.item(id)?)?;
    inventory.forget(id);
    stock.add(&salvage);
    Some(salvage)
}

/// For captures (`FB_SCRAP`): a few frames in, a rare sword and a legendary one are scrapped
/// into the delver's stock, so the character sheet has something to show.
pub fn scrap_for_capture(mut inventory: ResMut<Inventory>, mut stock: ResMut<Enchanting>, mut frames: Local<u32>) {
    *frames += 1;
    if *frames != 4 {
        return;
    }
    for rarity in [fb_items::Rarity::Rare, fb_items::Rarity::Legendary] {
        let sword = (0..6_000).filter_map(|s| fb_items::roll_item_with("Sword", s, fb_items::Odds::loot(8))).find(|i| i.rarity == rarity);
        if let Some(sword) = sword {
            let id = inventory.add(sword);
            scrap_item(&mut inventory, &mut stock.0, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_items::roll_item;

    #[test]
    fn scrapping_gear_removes_it_and_fills_the_stock() {
        let mut inventory = Inventory::default();
        let mut stock = EnchantingStock::default();
        let cloak = (0..200).filter_map(|s| roll_item("Cloak", s)).find(|i| i.affixes.is_empty()).unwrap();
        let id = inventory.add(cloak);
        let salvage = scrap_item(&mut inventory, &mut stock, id).unwrap();
        assert_eq!(stock.essence, salvage.essence);
        assert!(inventory.item(id).is_none());
        let purse = inventory.add(roll_item("Purse", 4).unwrap());
        assert!(scrap_item(&mut inventory, &mut stock, purse).is_none());
        assert!(inventory.item(purse).is_some());
    }
}
