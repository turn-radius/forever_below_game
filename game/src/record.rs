//! The world record: what happened on every level the delver has visited (#285).
//!
//! Each depth is always the same level, rebuilt from its seed ([`crate::travel::Abyss::level_at`]).
//! The record holds, for each visited depth, how that level stands now where it differs from how
//! it was generated: the items lying on its floor (dropped, moved, worn down, with the full
//! contents of every bag set down), the containers (opened or not, and what is left in them) and
//! the creatures (hit points, where they stand, or the body where they fell). A depth never
//! visited has no entry and is exactly its generated state.
//!
//! Leaving a level writes it into the record ([`record_level_left`], in
//! [`crate::travel::LeavingLevel`]), and the systems that lay a level out ([`crate::carry`],
//! [`crate::props`], [`crate::creatures`]) take the record into account, so a level is built once,
//! as it was left, rather than built and then changed. A new delve starts a new record.
//!
//! The data types are plain serde structs, free of Bevy (only [`WorldRecord`] derives `Resource`,
//! to be held by the app), so saving the record to disk, and keeping it across death, can build
//! directly on them. An item is its base and seed, which roll it ([`fb_items::roll_item`]), with
//! the Armory state it has now (its wear, how far it has burned, what was eaten) and what is in
//! it, so rebuilding it gives the item as it was.
//!
//! Not recorded, because the game has no such state yet: folk (they do not move, fight, change
//! mood or earn standing), the level's own lights (glowcaps and braziers burn without end), and
//! what a creature is doing this moment (it wakes afresh).

use std::collections::BTreeMap;

use bevy::prelude::*;
use fb_items::Coins;
use fb_level::{ItemSpot, Level};
use serde::{Deserialize, Serialize};

use crate::carry::FloorItem;
use crate::combat::Health;
use crate::combat::feedback::Corpse;
use crate::creatures::Critter;
use crate::inventory::{Inventory, ItemId, Placement};
use crate::play::CurrentLevel;
use crate::props::{Container, PropPiece};

/// What happened on every level visited this run, by depth.
#[derive(Resource, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldRecord {
    pub levels: BTreeMap<u32, LevelRecord>,
}

/// One level as it was left.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LevelRecord {
    /// Every item lying on the floor, in place of the level's generated ones.
    pub floor: Vec<FloorRecord>,
    /// Each container prop, by its index in the level's props.
    pub containers: BTreeMap<usize, ContainerRecord>,
    /// Each creature, by its index in the level's creatures.
    pub creatures: BTreeMap<usize, CreatureRecord>,
}

/// An item lying where it was left.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FloorRecord {
    pub spot: ItemSpot,
    pub item: ItemRecord,
}

/// An item as it is now: what rolls it, the Armory state it has, and what it holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemRecord {
    pub base: String,
    pub seed: u32,
    pub shape: armory::Item,
    /// A purse's gold, silver and copper.
    pub coins: Option<[u32; 3]>,
    pub contents: Vec<Stowed>,
    /// The odds it was rolled against; records from before odds roll at the first level's.
    #[serde(default)]
    pub odds: fb_items::Odds,
}

/// An item in a container's grid.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stowed {
    pub x: u8,
    pub y: u8,
    pub turned: bool,
    pub item: ItemRecord,
}

/// A container prop as it was left.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContainerRecord {
    pub opened: bool,
    pub contents: Vec<Stowed>,
}

/// A creature as it was left.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureRecord {
    pub hp: i32,
    pub max: i32,
    /// Where it stands, or where it stood as it fell (world units), turned by this quaternion.
    pub at: [f32; 3],
    pub turn: [f32; 4],
    /// How much larger than rolled it is drawn.
    pub scale: f32,
    /// Fallen: this is its body.
    pub dead: bool,
}

impl CreatureRecord {
    /// The transform it stands at (or stood at, as it fell).
    pub fn transform(&self) -> Transform {
        Transform { translation: Vec3::from_array(self.at), rotation: Quat::from_array(self.turn), scale: Vec3::splat(self.scale) }
    }
}

impl WorldRecord {
    /// The record of the level at `depth`, if it has been visited.
    pub fn level(&self, depth: u32) -> Option<&LevelRecord> {
        self.levels.get(&depth)
    }

    /// Makes `level`, just rebuilt from its seed, the level as it was left: the items on its
    /// floor are those recorded. A level never visited is left as generated.
    pub fn apply_to(&self, level: &mut Level) {
        if let Some(saved) = self.level(level.depth) {
            level.items = saved.floor.iter().map(|f| f.spot).collect();
        }
    }
}

/// `id` as it is now, with everything in it, if the inventory knows it.
pub fn record_item(inventory: &Inventory, id: ItemId) -> Option<ItemRecord> {
    let item = inventory.item(id)?;
    Some(ItemRecord {
        base: item.base.clone(),
        seed: item.seed,
        shape: item.shape.clone(),
        coins: item.coins.map(|c| [c.gp, c.sp, c.cp]),
        contents: record_contents(inventory, id),
        odds: item.odds,
    })
}

/// What is in `container`'s grid.
pub fn record_contents(inventory: &Inventory, container: ItemId) -> Vec<Stowed> {
    let Some(grid) = inventory.grid(container) else { return vec![] };
    grid.entries()
        .iter()
        .filter_map(|e| Some(Stowed { x: e.at.x, y: e.at.y, turned: e.at.turned, item: record_item(inventory, e.id)? }))
        .collect()
}

/// Keeps track of the item `record` describes in `inventory`, loose, with its contents where
/// they lay. `None` for a base no table knows.
pub fn restore_item(inventory: &mut Inventory, record: &ItemRecord) -> Option<ItemId> {
    let mut item = fb_items::roll_item_with(&record.base, record.seed, record.odds)?;
    item.shape = record.shape.clone();
    item.coins = record.coins.map(|[gp, sp, cp]| Coins { gp, sp, cp });
    let id = inventory.add(item);
    restore_contents(inventory, id, &record.contents);
    Some(id)
}

/// Puts `contents` back into `container`'s grid, each where it lay.
pub fn restore_contents(inventory: &mut Inventory, container: ItemId, contents: &[Stowed]) {
    for stowed in contents {
        let Some(id) = restore_item(inventory, &stowed.item) else { continue };
        if inventory.put(id, container, Placement::new(stowed.x, stowed.y, stowed.turned)).is_err() {
            inventory.forget(id);
        }
    }
}

/// Writes the level being left into the record. Runs in [`crate::travel::LeavingLevel`], when
/// the level is still whole.
pub fn record_level_left(
    level: Res<CurrentLevel>,
    inventory: Res<Inventory>,
    mut record: ResMut<WorldRecord>,
    floor: Query<&FloorItem>,
    containers: Query<(&PropPiece, &Container)>,
    creatures: Query<(&Critter, &Transform, Option<&Health>, Option<&Corpse>)>,
) {
    let mut left = LevelRecord::default();
    let mut lying: Vec<&FloorItem> = floor.iter().collect();
    lying.sort_by_key(|f| f.id);
    left.floor = lying.into_iter().filter_map(|f| Some(FloorRecord { spot: f.spot, item: record_item(&inventory, f.id)? })).collect();
    for (piece, container) in &containers {
        left.containers.insert(piece.index, ContainerRecord { opened: container.opened, contents: record_contents(&inventory, container.id) });
    }
    for (critter, transform, health, corpse) in &creatures {
        let Some(index) = level.0.creatures.iter().position(|s| *s == critter.spot) else { continue };
        let stood = corpse.map_or(*transform, |c| c.fell);
        let Some(health) = health else { continue };
        left.creatures.insert(
            index,
            CreatureRecord {
                hp: health.hp,
                max: health.max,
                at: stood.translation.to_array(),
                turn: stood.rotation.to_array(),
                scale: stood.scale.x,
                dead: corpse.is_some(),
            },
        );
    }
    record.levels.insert(level.0.depth, left);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_with_contents_round_trips_through_its_record() {
        let mut inventory = Inventory::default();
        let sack = inventory.add(fb_items::roll_item("Sack", 3).unwrap());
        let sword = inventory.add(fb_items::roll_item("Sword", 9).unwrap());
        let at = inventory.grid(sack).unwrap().first_free(inventory.footprint(sword).unwrap()).unwrap();
        inventory.put(sword, sack, at).unwrap();
        let record = record_item(&inventory, sack).unwrap();
        assert_eq!(record.contents.len(), 1);
        let json = serde_json::to_string(&record).unwrap();
        let record: ItemRecord = serde_json::from_str(&json).unwrap();
        let mut other = Inventory::default();
        let again = restore_item(&mut other, &record).unwrap();
        assert_eq!(record_item(&other, again), Some(record));
        assert_eq!(other.item(again), inventory.item(sack));
    }

    #[test]
    fn a_deep_find_restores_with_the_rarity_it_rolled() {
        let odds = fb_items::Odds::loot(8);
        let seed = (1..500).find(|&s| fb_items::roll_item_with("Sword", s, odds).unwrap().rarity != fb_items::roll_item("Sword", s).unwrap().rarity).unwrap();
        let mut inventory = Inventory::default();
        let sword = inventory.add(fb_items::roll_item_with("Sword", seed, odds).unwrap());
        let json = serde_json::to_string(&record_item(&inventory, sword).unwrap()).unwrap();
        let mut other = Inventory::default();
        let again = restore_item(&mut other, &serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(other.item(again), inventory.item(sword));
    }

    #[test]
    fn the_world_record_round_trips_through_serde_unchanged() {
        let mut record = WorldRecord::default();
        let item = fb_items::roll_item("Purse", 11).unwrap();
        let mut level = LevelRecord::default();
        level.floor.push(FloorRecord {
            spot: ItemSpot { x: 3.25, y: 4.5, yaw: 0.3, family: item.shape.family(), seed: item.seed },
            item: ItemRecord { base: item.base.clone(), seed: item.seed, shape: item.shape.clone(), coins: Some([1, 2, 3]), contents: vec![], odds: fb_items::Odds::kit(4) },
        });
        level.containers.insert(2, ContainerRecord { opened: true, contents: vec![] });
        level.creatures.insert(1, CreatureRecord { hp: 2, max: 5, at: [1.5, 0.0, 2.25], turn: [0.0, 0.7, 0.0, 0.7], scale: 1.25, dead: false });
        record.levels.insert(1, level);
        record.levels.insert(4, LevelRecord::default());
        let json = serde_json::to_string(&record).unwrap();
        assert_eq!(serde_json::from_str::<WorldRecord>(&json).unwrap(), record);
    }
}
