//! What a level's creatures, floor items and props are made of, worked out ahead of time.
//!
//! Rolling a creature (a skeleton is fitted its gear), a floor item or a chest's contents, and
//! building the meshes they are drawn with, is plain data (no Bevy) and takes a few hundred
//! milliseconds a level. [`Staging::make`] does it for a level, off the main thread
//! ([`crate::travel`]); [`Staged`] holds the result until the systems that lay the level out
//! take their part ([`Staged::take_creatures`] and the like), which then only spawn. A part is
//! used only for the level it was made for, and a system that finds none makes its own.

use armory::ItemMesh;
use bevy::prelude::*;
use fb_items::{Item, Odds};
use fb_level::Level;

use crate::creatures::{Built, build_creatures};
use crate::level::FixtureShapes;
use crate::props::{banner_piece, crystal_piece, prop_mesh};
use crate::render::ItemAppearance;

/// A prop's shut shape, and what a container holds.
#[derive(Debug)]
pub struct StagedProp {
    pub mesh: ItemMesh,
    pub appearance: ItemAppearance,
    /// What a container's loot rolls, empty for scenery.
    pub contents: Vec<Item>,
}

/// A banner or crystal, shaped and set where it hangs or grows.
#[derive(Debug)]
pub struct StagedPiece {
    pub mesh: ItemMesh,
    pub appearance: ItemAppearance,
    pub name: String,
    pub transform: Transform,
}

/// The parts of a level made ahead of time.
pub struct Staging {
    pub creatures: Vec<Built>,
    /// Parallel to the level's items: the loot each rolls and the shape it is drawn with.
    pub items: Vec<Option<(Item, ItemMesh)>>,
    /// Parallel to the level's props.
    pub props: Vec<StagedProp>,
    pub fixtures: FixtureShapes,
    /// Parallel to the level's banners and crystals.
    pub banners: Vec<StagedPiece>,
    pub crystals: Vec<StagedPiece>,
    /// What the loot was rolled against: the level's odds with the delver's luck then.
    pub odds: Odds,
}

impl Staging {
    /// The parts of `level`, its floor items and containers' loot rolled against `odds`.
    pub fn make(level: &Level, odds: Odds) -> Self {
        let creatures = build_creatures(&level.creatures, level.depth);
        let lore = level.lore();
        let items = level
            .items
            .iter()
            .map(|spot| {
                let item = level.floor_item(spot, odds, &lore)?;
                let mesh = item.shape.build();
                Some((item, mesh))
            })
            .collect();
        let props = level
            .props
            .iter()
            .map(|spot| {
                let (mesh, appearance) = prop_mesh(spot, level, false);
                StagedProp { mesh, appearance, contents: level.container_contents(spot, odds, &lore) }
            })
            .collect();
        let banners = level.banners.iter().map(|spot| banner_piece(level, spot)).collect();
        let crystals = level.crystals.iter().map(|spot| crystal_piece(level, spot)).collect();
        Staging { creatures, items, props, fixtures: FixtureShapes::make(level), banners, crystals, odds }
    }
}

/// A level's [`Staging`], with the seed and depth of the level it is for.
#[derive(Resource, Default)]
pub struct Staged {
    for_level: Option<(u32, u32)>,
    /// The odds its loot was rolled against.
    odds: Odds,
    creatures: Option<Vec<Built>>,
    items: Option<Vec<Option<(Item, ItemMesh)>>>,
    props: Option<Vec<StagedProp>>,
    fixtures: Option<FixtureShapes>,
    banners: Option<Vec<StagedPiece>>,
    crystals: Option<Vec<StagedPiece>>,
    /// How many parts have been taken, which tests read to see the staging was used.
    pub taken: u32,
}

impl Staged {
    /// Keeps `staging` as the parts of `level`, in place of whatever was kept.
    pub fn set(&mut self, level: &Level, staging: Staging) {
        let Staging { creatures, items, props, fixtures, banners, crystals, odds } = staging;
        *self = Staged {
            for_level: Some((level.seed, level.depth)),
            odds,
            creatures: Some(creatures),
            items: Some(items),
            props: Some(props),
            fixtures: Some(fixtures),
            banners: Some(banners),
            crystals: Some(crystals),
            taken: 0,
        };
    }

    fn is_for(&self, level: &Level) -> bool {
        self.for_level == Some((level.seed, level.depth))
    }

    /// Takes the part `slot` picks, if it is for `level` and `fits` it (a part that does not is
    /// dropped, and the system makes its own).
    fn take_with<T>(&mut self, level: &Level, slot: impl FnOnce(&mut Self) -> &mut Option<T>, fits: impl FnOnce(&T) -> bool) -> Option<T> {
        if !self.is_for(level) {
            return None;
        }
        let part = slot(self).take().filter(fits);
        self.taken += u32::from(part.is_some());
        part
    }

    /// The creatures made ahead for `level`, once, if that is the level they were made for.
    pub fn take_creatures(&mut self, level: &Level) -> Option<Vec<Built>> {
        self.take_with(level, |s| &mut s.creatures, |c| c.len() == level.creatures.len())
    }

    /// The floor items made ahead for `level`, once, if they were rolled against `odds` (the
    /// delver's luck may have changed since).
    pub fn take_items(&mut self, level: &Level, odds: Odds) -> Option<Vec<Option<(Item, ItemMesh)>>> {
        let rolled_at = self.odds;
        self.take_with(level, |s| &mut s.items, |i| i.len() == level.items.len() && rolled_at == odds)
    }

    /// The props made ahead for `level`, once, their containers holding what rolls against
    /// `odds`: rolled again here if the luck changed since they were made.
    pub fn take_props(&mut self, level: &Level, odds: Odds) -> Option<Vec<StagedProp>> {
        let mut props = self.take_with(level, |s| &mut s.props, |p| p.len() == level.props.len())?;
        if self.odds != odds {
            let lore = level.lore();
            for (staged, spot) in props.iter_mut().zip(&level.props) {
                staged.contents = level.container_contents(spot, odds, &lore);
            }
        }
        Some(props)
    }

    /// The banners made ahead for `level`, once.
    pub fn take_banners(&mut self, level: &Level) -> Option<Vec<StagedPiece>> {
        self.take_with(level, |s| &mut s.banners, |b| b.len() == level.banners.len())
    }

    /// The crystals made ahead for `level`, once.
    pub fn take_crystals(&mut self, level: &Level) -> Option<Vec<StagedPiece>> {
        self.take_with(level, |s| &mut s.crystals, |c| c.len() == level.crystals.len())
    }

    /// The door and brazier shapes made ahead for `level`, once.
    pub fn take_fixtures(&mut self, level: &Level) -> Option<FixtureShapes> {
        self.take_with(level, |s| &mut s.fixtures, |f| f.braziers.len() == level.lights.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_is_used_once_and_only_for_its_level() {
        let level = fb_level::generate(fb_level::depth_seed(1207, 2), 2);
        let other = fb_level::generate(fb_level::depth_seed(1207, 3), 3);
        let odds = Odds::loot(2);
        let mut staged = Staged::default();
        staged.set(&level, Staging::make(&level, odds));
        assert!(staged.take_items(&other, odds).is_none(), "not the level it was made for");
        let items = staged.take_items(&level, odds).expect("made for this level");
        assert_eq!(items.len(), level.items.len());
        assert!(staged.take_items(&level, odds).is_none(), "used up");
        assert_eq!(staged.take_props(&level, odds).map(|p| p.len()), Some(level.props.len()));
        assert_eq!(staged.take_creatures(&level).map(|c| c.len()), Some(level.creatures.len()));
        assert!(staged.take_fixtures(&level).is_some());
        assert_eq!(staged.take_banners(&level).map(|b| b.len()), Some(level.banners.len()));
        assert_eq!(staged.take_crystals(&level).map(|c| c.len()), Some(level.crystals.len()));
        assert_eq!(staged.taken, 6, "every part was taken once: {}", staged.taken);
    }

    #[test]
    fn loot_staged_before_the_luck_changed_is_rolled_again() {
        let level = fb_level::generate(fb_level::depth_seed(1207, 2), 2);
        let (before, after) = (Odds::loot(2), Odds::loot(2).with_luck(3.0));
        let mut staged = Staged::default();
        staged.set(&level, Staging::make(&level, before));
        assert!(staged.take_items(&level, after).is_none(), "the floor is rolled in place");
        let props = staged.take_props(&level, after).expect("the shapes still serve");
        for (spot, prop) in level.props.iter().zip(&props) {
            assert_eq!(prop.contents, level.container_contents(spot, after, &level.lore()), "{spot:?}");
        }
    }

    #[test]
    fn staged_parts_are_the_parts_made_in_place() {
        use crate::creatures::build_creature;
        use crate::inventory::Inventory;
        use crate::props::{stock_container, stock_container_with};
        use crate::record::record_contents;

        let (mut items, mut creatures, mut containers, mut banners) = (0, 0, 0, 0);
        for depth in 1..=4 {
            let level = fb_level::generate(fb_level::depth_seed(1207, depth), depth);
            let staging = Staging::make(&level, Odds::loot(depth));

            assert_eq!(staging.items.len(), level.items.len());
            for (spot, staged) in level.items.iter().zip(&staging.items) {
                let rolled = level.floor_item(spot, Odds::loot(depth), &level.lore());
                assert_eq!(rolled.as_ref(), staged.as_ref().map(|(item, _)| item), "depth {depth}: the loot");
                if let (Some(item), Some((_, mesh))) = (&rolled, staged) {
                    assert_eq!(&item.shape.build(), mesh, "depth {depth}: its shape");
                    items += 1;
                }
            }

            assert_eq!(staging.creatures.len(), level.creatures.len());
            for (spot, staged) in level.creatures.iter().zip(&staging.creatures) {
                assert_eq!(format!("{staged:?}"), format!("{:?}", build_creature(spot, depth)), "depth {depth}: the creature");
                creatures += 1;
            }

            assert_eq!(staging.props.len(), level.props.len());
            for (spot, staged) in level.props.iter().zip(&staging.props) {
                let (mesh, appearance) = prop_mesh(spot, &level, false);
                assert_eq!((&mesh, &appearance), (&staged.mesh, &staged.appearance), "depth {depth}: the prop");
                // A container stocked from the staged contents holds what one stocked in place does.
                let (mut ahead, mut here) = (Inventory::default(), Inventory::default());
                let stocked_ahead = stock_container_with(&mut ahead, spot.prop, staged.contents.clone());
                let stocked_here = stock_container(&mut here, spot, Odds::loot(depth), &level, &level.lore());
                assert_eq!(stocked_ahead.is_some(), stocked_here.is_some());
                if let (Some(a), Some(h)) = (stocked_ahead, stocked_here) {
                    let (a, h) = (record_contents(&ahead, a), record_contents(&here, h));
                    assert_eq!(format!("{a:?}"), format!("{h:?}"), "depth {depth}: the contents");
                    containers += 1;
                }
            }

            assert_eq!(staging.fixtures, FixtureShapes::make(&level), "depth {depth}: the fixtures");

            assert_eq!(staging.banners.len(), level.banners.len());
            for (spot, staged) in level.banners.iter().zip(&staging.banners) {
                assert_eq!(format!("{staged:?}"), format!("{:?}", crate::props::banner_piece(&level, spot)), "depth {depth}: the banner");
                banners += 1;
            }
            assert_eq!(staging.crystals.len(), level.crystals.len());
            for (spot, staged) in level.crystals.iter().zip(&staging.crystals) {
                assert_eq!(format!("{staged:?}"), format!("{:?}", crate::props::crystal_piece(&level, spot)), "depth {depth}: the crystal");
            }
        }
        assert!(items > 0 && creatures > 0 && containers > 0 && banners > 0, "the comparison covered something: {items} items, {creatures} creatures, {containers} containers, {banners} banners");
    }
}
