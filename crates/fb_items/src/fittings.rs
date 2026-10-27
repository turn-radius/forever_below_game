//! The item slots a pack and a belt come with: a pack's lashing and weapon strap, a belt's two
//! frogs and its potion pouch (`docs/items.md`, "Pack and belt accessories").
//!
//! They are rolled with the pack or belt ([`Item::accessories`]) and never found loose. Each is
//! a framed slot beside the container's main grid, with its own label, that takes one whole item
//! of its fitting's kind ([`Item::fits_accessory`]) whatever its size, and the pouch three:
//! [`FittingGrid::holds`] things at most. Behind the slots is a hidden container whose grid is
//! only room enough for anything the slots accept; it is never drawn.

use armory::families::bags::Accessory;

use crate::footprint::Grid;
use crate::item::Item;

/// What a fitting is: how its slots are labelled and drawn, and how much it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FittingGrid {
    pub accessory: Accessory,
    /// What the screen calls the fitting.
    pub label: &'static str,
    /// The hidden grid behind the slots, big enough for the largest thing the fitting accepts, so
    /// a slot never refuses for room. Never drawn.
    pub cols: u8,
    pub rows: u8,
    /// The most things in it at once: one for a lashing, strap or frog, three for the pouch.
    pub holds: u8,
    /// Whether its slots are drawn small, as the doll's small slots, rather than big.
    pub small: bool,
}

/// The fitting `accessory` makes; none for a belt's sack, which is a row more of the belt's own grid.
/// Each slot holds one whole item: a tent is the largest bundle (2 by 6), a bow the widest strapped
/// thing (5 by 9 at most), a sword the longest in a frog, and the pouch takes three vials or small potions.
pub fn fitting_grid(accessory: Accessory) -> Option<FittingGrid> {
    let (label, cols, rows, holds, small) = match accessory {
        Accessory::Lashing => ("Lashing", 10, 10, 1, false),
        Accessory::WeaponStrap => ("Strap", 10, 10, 1, false),
        Accessory::LeftFrog => ("Left frog", 10, 10, 1, false),
        Accessory::RightFrog => ("Right frog", 10, 10, 1, false),
        Accessory::PotionPouch => ("Potion pouch", 6, 4, 3, true),
        Accessory::BeltSack => return None,
    };
    Some(FittingGrid { accessory, label, cols, rows, holds, small })
}

/// The grids `item` has, in the order it was rolled with them: none for anything but a pack or belt.
pub fn fitting_grids(item: &Item) -> Vec<FittingGrid> {
    item.accessories().into_iter().filter_map(fitting_grid).collect()
}

/// The container item that stands for `grid` on `host`: the host's own shape and seed, called by the
/// fitting's label, weighing nothing and holding the grid. The inventory keeps it out of sight; only its
/// grid shows, in the host's window.
pub fn fitting_item(host: &Item, grid: &FittingGrid) -> Item {
    let mut item = host.clone();
    item.base = grid.label.to_string();
    item.name = grid.label.to_string();
    item.affixes = vec![];
    item.legend = None;
    item.history = vec![];
    item.quantity = 1;
    item.profile.stats.weight = 0.0;
    item.profile.holds = Some(Grid { cols: grid.cols, rows: grid.rows, label: grid.label.to_string(), accepts: vec![] });
    item
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roll_item;

    #[test]
    fn a_pack_and_a_belt_have_the_grids_they_were_rolled_with() {
        let packs: Vec<Item> = (1..80).filter_map(|s| roll_item("Pack", s)).collect();
        let laced = packs.iter().find(|p| p.has_accessory(Accessory::Lashing) && p.has_accessory(Accessory::WeaponStrap)).expect("a pack with both");
        assert_eq!(fitting_grids(laced).iter().map(|g| g.label).collect::<Vec<_>>(), ["Lashing", "Strap"]);
        assert!(packs.iter().filter(|p| p.accessories().is_empty()).all(|p| fitting_grids(p).is_empty()));
        let belts: Vec<Item> = (1..200).filter_map(|s| roll_item("Belt", s)).collect();
        assert!(belts.iter().any(|b| fitting_grids(b).iter().any(|g| g.accessory == Accessory::PotionPouch)));
        assert!(belts.iter().all(|b| fitting_grids(b).iter().all(|g| g.accessory != Accessory::BeltSack)), "the sack is a row of the belt's grid");
        assert!(roll_item("Sword", 3).is_some_and(|s| fitting_grids(&s).is_empty()));
    }

    #[test]
    fn every_item_a_slot_accepts_fits_the_room_behind_it() {
        let fits = |grid: FittingGrid, item: &Item| {
            let f = item.profile.footprint;
            (f.cols <= grid.cols && f.rows <= grid.rows) || (f.rows <= grid.cols && f.cols <= grid.rows)
        };
        let all = |bases: &[&str]| -> Vec<Item> { bases.iter().flat_map(|b| (1..300).filter_map(|s| roll_item(b, s))).collect() };
        for (accessory, bases) in [
            (Accessory::Lashing, &["Bedroll", "Tent"][..]),
            (Accessory::LeftFrog, &["Dagger", "Sword", "Mace", "Club", "Axe", "Hand axe"][..]),
            (Accessory::PotionPouch, &["Vial", "Small potion", "Healing vial", "Small healing draught"][..]),
        ] {
            let grid = fitting_grid(accessory).unwrap();
            for item in all(bases).iter().filter(|i| i.fits_accessory(accessory)) {
                assert!(fits(grid, item), "{accessory:?}: {} {:?}", item.base, item.profile.footprint);
            }
        }
        // Three of the largest pouch potion take no more than the pouch's room.
        let pouch = fitting_grid(Accessory::PotionPouch).unwrap();
        let widest = all(&["Vial", "Small potion", "Healing vial", "Small healing draught"]).iter().filter(|i| i.fits_accessory(Accessory::PotionPouch)).map(|i| i.profile.footprint.cols as u32 * i.profile.footprint.rows as u32).max().unwrap();
        assert!(3 * widest <= pouch.cols as u32 * pouch.rows as u32);
        // The strap takes every long, thin weapon the rule allows, the widest bows included, and
        // never a crossbow.
        let strap = fitting_grid(Accessory::WeaponStrap).unwrap();
        let strapped: Vec<Item> = all(&["Spear", "Staff", "Sword", "Mace", "Bow", "Crossbow", "Axe", "Hand axe"]).into_iter().filter(|i| i.strappable()).collect();
        for base in ["Bow", "Spear", "Staff"] {
            assert!(strapped.iter().any(|i| i.base == base), "some {base} is strappable");
        }
        assert!(strapped.iter().all(|i| i.base != "Crossbow"), "a crossbow is never strapped");
        assert!(!strapped.is_empty() && strapped.iter().all(|i| fits(strap, i)));
    }
}
