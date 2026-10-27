//! The small grids a pack and a belt come with: a pack's lashing and weapon strap, a belt's two
//! frogs and its potion pouch (`docs/items.md`, "Pack and belt accessories").
//!
//! They are rolled with the pack or belt ([`Item::accessories`]) and never found loose. Each is
//! a grid of its own beside the container's main grid, with its own label, that takes only what
//! its fitting holds ([`Item::fits_accessory`]) and at most [`FittingGrid::holds`] things.

use armory::families::bags::Accessory;

use crate::footprint::Grid;
use crate::item::Item;

/// How big a fitting's grid is and how much it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FittingGrid {
    pub accessory: Accessory,
    /// What the screen calls the grid.
    pub label: &'static str,
    pub cols: u8,
    pub rows: u8,
    /// The most things in it at once: one for a lashing, strap or frog, three for the pouch.
    pub holds: u8,
}

/// The grid of `accessory`; none for a belt's sack, which is a row more of the belt's own grid.
/// Sizes follow what each holds: a tent is the largest bundle (2 by 6), a spear or staff the
/// longest strapped thing (up to 9 long, 2 wide), a sword the longest in a frog (2 by 5), and
/// the pouch is a row of places for vials and small potions.
pub fn fitting_grid(accessory: Accessory) -> Option<FittingGrid> {
    let (label, cols, rows, holds) = match accessory {
        Accessory::Lashing => ("Lashing", 2, 6, 1),
        Accessory::WeaponStrap => ("Strap", 2, 9, 1),
        Accessory::LeftFrog => ("Left frog", 2, 5, 1),
        Accessory::RightFrog => ("Right frog", 2, 5, 1),
        Accessory::PotionPouch => ("Potion pouch", 3, 2, 3),
        Accessory::BeltSack => return None,
    };
    Some(FittingGrid { accessory, label, cols, rows, holds })
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
    fn each_grid_is_big_enough_for_the_largest_thing_it_takes() {
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
        // The strap takes long weapons up to nine cells and two wide.
        let strap = fitting_grid(Accessory::WeaponStrap).unwrap();
        let strapped: Vec<Item> = all(&["Spear", "Staff", "Sword", "Mace"]).into_iter().filter(|i| i.strappable()).collect();
        assert!(!strapped.is_empty() && strapped.iter().all(|i| fits(strap, i)));
    }
}
