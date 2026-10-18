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
use fb_level::{Level, Prop};

use crate::creatures::{Built, build_creatures};
use crate::level::FixtureShapes;
use crate::props::prop_mesh;
use crate::render::ItemAppearance;

/// A prop's shut shape, and what a container holds.
#[derive(Debug)]
pub struct StagedProp {
    pub mesh: ItemMesh,
    pub appearance: ItemAppearance,
    /// What a container's loot rolls, empty for scenery.
    pub contents: Vec<Item>,
}

/// The parts of a level made ahead of time.
pub struct Staging {
    pub creatures: Vec<Built>,
    /// Parallel to the level's items: the loot each rolls and the shape it is drawn with.
    pub items: Vec<Option<(Item, ItemMesh)>>,
    /// Parallel to the level's props.
    pub props: Vec<StagedProp>,
    pub fixtures: FixtureShapes,
}

impl Staging {
    pub fn make(level: &Level) -> Self {
        let creatures = build_creatures(&level.creatures, level.depth);
        let items = level
            .items
            .iter()
            .map(|spot| {
                let item = fb_items::roll_loot(spot.family, spot.seed, Odds::loot(level.depth))?;
                let mesh = item.shape.build();
                Some((item, mesh))
            })
            .collect();
        let props = level
            .props
            .iter()
            .map(|spot| {
                let (mesh, appearance) = prop_mesh(spot, level, false);
                let contents = match spot.prop {
                    Prop::Container { style, loot, .. } => fb_items::roll_contents(style, loot, Odds::loot(level.depth)),
                    Prop::Scenery { .. } => Vec::new(),
                };
                StagedProp { mesh, appearance, contents }
            })
            .collect();
        Staging { creatures, items, props, fixtures: FixtureShapes::make(level) }
    }
}

/// A level's [`Staging`], with the seed and depth of the level it is for.
#[derive(Resource, Default)]
pub struct Staged {
    for_level: Option<(u32, u32)>,
    creatures: Option<Vec<Built>>,
    items: Option<Vec<Option<(Item, ItemMesh)>>>,
    props: Option<Vec<StagedProp>>,
    fixtures: Option<FixtureShapes>,
    /// How many parts have been taken, which tests read to see the staging was used.
    pub taken: u32,
}

impl Staged {
    /// Keeps `staging` as the parts of `level`, in place of whatever was kept.
    pub fn set(&mut self, level: &Level, staging: Staging) {
        let Staging { creatures, items, props, fixtures } = staging;
        *self = Staged { for_level: Some((level.seed, level.depth)), creatures: Some(creatures), items: Some(items), props: Some(props), fixtures: Some(fixtures), taken: 0 };
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

    /// The floor items made ahead for `level`, once.
    pub fn take_items(&mut self, level: &Level) -> Option<Vec<Option<(Item, ItemMesh)>>> {
        self.take_with(level, |s| &mut s.items, |i| i.len() == level.items.len())
    }

    /// The props made ahead for `level`, once.
    pub fn take_props(&mut self, level: &Level) -> Option<Vec<StagedProp>> {
        self.take_with(level, |s| &mut s.props, |p| p.len() == level.props.len())
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
        let mut staged = Staged::default();
        staged.set(&level, Staging::make(&level));
        assert!(staged.take_items(&other).is_none(), "not the level it was made for");
        let items = staged.take_items(&level).expect("made for this level");
        assert_eq!(items.len(), level.items.len());
        assert!(staged.take_items(&level).is_none(), "used up");
        assert_eq!(staged.take_props(&level).map(|p| p.len()), Some(level.props.len()));
        assert_eq!(staged.take_creatures(&level).map(|c| c.len()), Some(level.creatures.len()));
        assert!(staged.take_fixtures(&level).is_some());
    }

    #[test]
    fn staged_parts_are_the_parts_made_in_place() {
        use crate::creatures::build_creature;
        use crate::inventory::Inventory;
        use crate::props::{stock_container, stock_container_with};
        use crate::record::record_contents;

        let (mut items, mut creatures, mut containers) = (0, 0, 0);
        for depth in 1..=4 {
            let level = fb_level::generate(fb_level::depth_seed(1207, depth), depth);
            let staging = Staging::make(&level);

            assert_eq!(staging.items.len(), level.items.len());
            for (spot, staged) in level.items.iter().zip(&staging.items) {
                let rolled = fb_items::roll_loot(spot.family, spot.seed, Odds::loot(depth));
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
                let stocked_here = stock_container(&mut here, spot.prop, depth);
                assert_eq!(stocked_ahead.is_some(), stocked_here.is_some());
                if let (Some(a), Some(h)) = (stocked_ahead, stocked_here) {
                    let (a, h) = (record_contents(&ahead, a), record_contents(&here, h));
                    assert_eq!(format!("{a:?}"), format!("{h:?}"), "depth {depth}: the contents");
                    containers += 1;
                }
            }

            assert_eq!(staging.fixtures, FixtureShapes::make(&level), "depth {depth}: the fixtures");
        }
        assert!(items > 0 && creatures > 0 && containers > 0, "the comparison covered something: {items} items, {creatures} creatures, {containers} containers");
    }
}
