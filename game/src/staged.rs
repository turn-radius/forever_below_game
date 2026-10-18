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
}

impl Staged {
    /// Keeps `staging` as the parts of `level`, in place of whatever was kept.
    pub fn set(&mut self, level: &Level, staging: Staging) {
        let Staging { creatures, items, props, fixtures } = staging;
        *self = Staged { for_level: Some((level.seed, level.depth)), creatures: Some(creatures), items: Some(items), props: Some(props), fixtures: Some(fixtures) };
    }

    fn is_for(&self, level: &Level) -> bool {
        self.for_level == Some((level.seed, level.depth))
    }

    /// The creatures made ahead for `level`, once, if that is the level they were made for.
    pub fn take_creatures(&mut self, level: &Level) -> Option<Vec<Built>> {
        if self.is_for(level) { self.creatures.take().filter(|c| c.len() == level.creatures.len()) } else { None }
    }

    /// The floor items made ahead for `level`, once.
    pub fn take_items(&mut self, level: &Level) -> Option<Vec<Option<(Item, ItemMesh)>>> {
        if self.is_for(level) { self.items.take().filter(|i| i.len() == level.items.len()) } else { None }
    }

    /// The props made ahead for `level`, once.
    pub fn take_props(&mut self, level: &Level) -> Option<Vec<StagedProp>> {
        if self.is_for(level) { self.props.take().filter(|p| p.len() == level.props.len()) } else { None }
    }

    /// The door and brazier shapes made ahead for `level`, once.
    pub fn take_fixtures(&mut self, level: &Level) -> Option<FixtureShapes> {
        if self.is_for(level) { self.fixtures.take() } else { None }
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
    fn staged_loot_is_the_loot_rolled_in_place() {
        let level = fb_level::generate(fb_level::depth_seed(1207, 2), 2);
        let staging = Staging::make(&level);
        for (spot, staged) in level.items.iter().zip(&staging.items) {
            let rolled = fb_items::roll_loot(spot.family, spot.seed, Odds::loot(level.depth));
            assert_eq!(rolled.as_ref().map(|i| i.title().to_string()), staged.as_ref().map(|(i, _)| i.title().to_string()));
        }
    }
}
