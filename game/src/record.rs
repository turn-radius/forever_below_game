//! The world record: what happened on every level the delver has visited (#285).
//!
//! Each depth is always the same level, rebuilt from its seed ([`crate::travel::Abyss::level_at`]).
//! The record holds, for each visited depth, how that level stands now where it differs from how
//! it was generated: the items lying on its floor (dropped, moved, worn down, with the full
//! contents of every bag set down), the containers (opened or not, and what is left in them) and
//! the creatures (hit points, where they stand, or the body where they fell) and the folk (hit
//! points, where they stand or the body where they fell, and whether they have turned on the
//! delver). A depth never
//! visited has no entry and is exactly its generated state.
//!
//! Leaving a level writes it into the record ([`record_level_left`], in
//! [`crate::travel::LeavingLevel`]), and the systems that lay a level out ([`crate::carry`],
//! [`crate::props`], [`crate::creatures`]) take the record into account, so a level is built once,
//! as it was left, rather than built and then changed. A new delve starts a new record.
//!
//! The data types are plain serde structs, free of Bevy (only [`WorldRecord`] derives `Resource`,
//! to be held by the app), so saving the record to disk, and keeping it across death, can build
//! directly on them. An item is its base and seed, with everything it is now: the Armory state
//! (its wear, how far it has burned, what was eaten), the rarity, affixes (by name), name, legend
//! and history lines, and what is in it. Rebuilding it ([`rebuild_item`], by
//! [`fb_items::Item::from_parts`]) rolls nothing again and works the stats out from the recorded
//! affixes, so an item that was changed after its roll (enchanted, commissioned, given a maker's
//! line) comes back as changed. A record lasts one delve, so none of it is kept compatible with
//! an older shape.
//!
//! Not recorded, because the game has no such state yet: folk standing beyond hostility (moods,
//! earned favour), the level's own lights (glowcaps and braziers burn without end), and
//! what a creature is doing this moment (it wakes afresh).

use std::collections::BTreeMap;

use bevy::prelude::*;
use fb_items::Coins;
use fb_level::{CELL, Claim, ItemSpot, Level, settle_claims};
use serde::{Deserialize, Serialize};

use crate::carry::FloorItem;
use crate::combat::Health;
use crate::combat::feedback::Corpse;
use crate::creatures::Critter;
use crate::folk::Denizen;
use crate::hostility::Hostility;
use crate::inventory::{Inventory, ItemId, Placement};
use crate::play::CurrentLevel;
use crate::props::{Container, PropPiece};

/// What happened on every level visited this run, by depth.
#[derive(Resource, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldRecord {
    pub levels: BTreeMap<u32, LevelRecord>,
    /// The delver's standing with every faction of the world, shared by each level and lasting
    /// the delve (#542).
    #[serde(default)]
    pub standings: fb_faction::Standings,
    /// The items the delver has commissioned and not yet collected, held by their makers for the
    /// delve (#554). The finished item is worked out from the order when it is handed over.
    #[serde(default)]
    pub orders: Vec<crate::commission::Commission>,
    /// The factions that have given the delver their key, which they give once (#553).
    #[serde(default)]
    pub keys_given: Vec<fb_faction::Faction>,
    /// The depths where the delver has met each faction, for the character screen's factions tab
    /// (#571).
    #[serde(default)]
    pub met_on: BTreeMap<fb_faction::Faction, std::collections::BTreeSet<u32>>,
    /// The words of each denizen tongue the delver has learned from books and scrolls, for the
    /// rest of the delve (#629). Talk reads it ([`WorldRecord::lexicon`]) to know which lines are
    /// readable; a new delve starts with none.
    #[serde(default)]
    pub lexicon: fb_denizens::tongue::Lexicon,
    /// How many times each folk, by (depth, seed), has been persuaded at, so each try rolls afresh.
    #[serde(default, with = "crate::map_pairs")]
    pub persuasions: BTreeMap<(u32, u32), u32>,
    /// How many times the delver has woken from a sleep in a camp. Failed identifying and
    /// persuading wait for the next one.
    #[serde(default)]
    pub rests: u32,
    /// Potions of insight the delver has drunk. Like a rest, one lets items that fell short be
    /// identified again; unlike a rest it does not lift a folk's refusal to be persuaded.
    #[serde(default)]
    pub insights: u32,
    /// The folk who refused to be persuaded, by (depth, seed), with the rests then: they refuse
    /// again until the delver has rested.
    #[serde(default, with = "crate::map_pairs")]
    pub refused: BTreeMap<(u32, u32), u32>,
    /// The hostile groups won over by persuasion, by (depth, faction), with the rests then: the
    /// faction's folk on that level do not attack the delver until a blow ends the truce or the
    /// delver rests ([`WorldRecord::truce_holds`]).
    #[serde(default, with = "crate::map_pairs")]
    pub truces: BTreeMap<(u32, fb_faction::Faction), u32>,
}

impl WorldRecord {
    /// The denizen words the delver has learned.
    pub fn lexicon(&self) -> &fb_denizens::tongue::Lexicon {
        &self.lexicon
    }

    /// How many times the folk `who` (depth, seed) has been persuaded at.
    pub fn persuasion_tries(&self, who: (u32, u32)) -> u32 {
        self.persuasions.get(&who).copied().unwrap_or(0)
    }

    /// What identify tries are counted against: rests and potions of insight together. An item
    /// that fell short waits for this to pass the count it was tried at.
    pub fn identify_epoch(&self) -> u32 {
        self.rests + self.insights
    }

    /// Whether `who` still refuses to be persuaded: they refused and the delver has not rested since.
    pub fn refuses(&self, who: (u32, u32)) -> bool {
        self.refused.get(&who).is_some_and(|&at| self.rests <= at)
    }

    /// Whether `faction`'s folk on level `depth` stand down: they were won over and the delver has
    /// neither struck one of them nor rested since.
    pub fn truce_holds(&self, depth: u32, faction: fb_faction::Faction) -> bool {
        self.truces.get(&(depth, faction)).is_some_and(|&at| self.rests <= at)
    }

    /// Has `faction`'s folk on level `depth` stand down until the delver strikes one of them or rests.
    pub fn make_truce(&mut self, depth: u32, faction: fb_faction::Faction) {
        self.truces.insert((depth, faction), self.rests);
    }

    /// Ends the truce with `faction` on level `depth`, as a blow from the delver does.
    pub fn end_truce(&mut self, depth: u32, faction: fb_faction::Faction) {
        self.truces.remove(&(depth, faction));
    }

    /// Notes one more try at persuading `who`.
    pub fn note_persuasion(&mut self, who: (u32, u32)) {
        *self.persuasions.entry(who).or_default() += 1;
    }

    /// Whether `faction` is already noted as met on level `depth`.
    pub fn was_met_on(&self, faction: fb_faction::Faction, depth: u32) -> bool {
        self.met_on.get(&faction).is_some_and(|d| d.contains(&depth))
    }

    /// Notes that `faction` was met on level `depth`. Callers check [`Self::was_met_on`] first,
    /// so the record changes only when there is something new.
    pub fn note_met_on(&mut self, faction: fb_faction::Faction, depth: u32) {
        self.met_on.entry(faction).or_default().insert(depth);
    }
}

/// One level as it was left.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LevelRecord {
    /// Every item lying on the floor, in place of the level's generated ones.
    pub floor: Vec<FloorRecord>,
    /// Each container prop, by its index in the level's props.
    pub containers: BTreeMap<usize, ContainerRecord>,
    /// Each urn standing in a wall niche, by its index in the level's niches (#832).
    #[serde(default)]
    pub urns: BTreeMap<usize, ContainerRecord>,
    /// Each creature, by its index in the level's creatures.
    pub creatures: BTreeMap<usize, CreatureRecord>,
    /// Each folk, by their index in the level's folk.
    #[serde(default)]
    pub folk: BTreeMap<usize, FolkRecord>,
    /// The ground each faction holds, as the delver left it: a faction down to one member holds
    /// the one room they hold, and one with no one left holds none. `None` for a record from
    /// before claims, which leaves the level's own.
    #[serde(default)]
    pub claims: Option<Vec<Claim>>,
    /// What each tradesperson had on the shelf when the level was left, by their index in the
    /// level's folk. Back on a revisit the same game day; a new day restocks it (#548).
    #[serde(default)]
    pub shelves: BTreeMap<usize, ShelfRecord>,
    /// Each claim door as the delver left it (#553).
    #[serde(default)]
    pub doors: Vec<DoorRecord>,
}

/// A tradesperson's shelf as it was left.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShelfRecord {
    /// The game day it was stocked on.
    pub day: u32,
    pub wares: Vec<ItemRecord>,
    /// What the delver sold the trader, which a restock keeps.
    #[serde(default)]
    pub sold: Vec<ItemRecord>,
    /// The trader's coin in copper; none for a record from before they had any.
    #[serde(default)]
    pub purse: Option<u64>,
}

/// A claim door as it was left: swung open or shut, and unlocked by key or picks or not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoorRecord {
    pub x: i32,
    pub y: i32,
    pub open: bool,
    pub unlocked: bool,
}

/// A folk as they were left.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FolkRecord {
    pub hp: i32,
    pub max: i32,
    /// Where they stand, or where they stood as they fell (world units), turned by this
    /// quaternion.
    pub at: [f32; 3],
    pub turn: [f32; 4],
    /// They have turned on the delver, and stay so.
    pub hostile: bool,
    /// Fallen: this is their body, bare (what they wore was set down where they fell, and is on
    /// the floor, or gone, as the record has it).
    #[serde(default)]
    pub dead: bool,
    /// What is left on the body, still wearing it: none for a body from before bodies kept their
    /// gear (its things lie on the floor as the record has them) or one looted bare.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyRecord>,
}

impl FolkRecord {
    /// The transform they stand at.
    pub fn transform(&self) -> Transform {
        Transform { translation: Vec3::from_array(self.at), rotation: Quat::from_array(self.turn), scale: Vec3::ONE }
    }
}

/// An item lying where it was left.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FloorRecord {
    pub spot: ItemSpot,
    pub item: ItemRecord,
}

/// An item as it is now: its base and seed, the Armory state it has, the rarity, affixes, name,
/// legend and history it has (as rolled, or as since changed), and what it holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemRecord {
    pub base: String,
    pub seed: u32,
    pub shape: armory::Item,
    pub rarity: fb_items::Rarity,
    /// Its affixes, by name ([`fb_items::tables::affix_named`]).
    pub affixes: Vec<String>,
    pub name: String,
    pub legend: Option<fb_items::naming::Legend>,
    /// Lines of history beyond the legend's ("Made by ...").
    pub history: Vec<String>,
    /// A purse's gold, silver and copper.
    pub coins: Option<[u32; 3]>,
    pub contents: Vec<Stowed>,
    /// The odds it was rolled against.
    pub odds: fb_items::Odds,
    /// What is written on it.
    #[serde(default)]
    pub words: Option<String>,
    /// The words it teaches, if it is written in a denizen tongue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teaching: Option<fb_denizens::tongue::Teaching>,
    /// How much of its magic is known. Absent in a record from before items were unknown, which
    /// reads as fully known.
    #[serde(default, skip_serializing_if = "fb_items::Known::is_full")]
    pub known: fb_items::Known,
    /// The rests the delver had had when an attempt to identify it fell short.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest_tried: Option<u32>,
    /// How many it stands for, a stack of arrows. Absent in a record from before stacks: one.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub quantity: u32,
}

fn one() -> u32 {
    1
}

fn is_one(n: &u32) -> bool {
    *n == 1
}

/// An item in a container's grid.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stowed {
    pub x: u8,
    pub y: u8,
    pub turned: bool,
    pub item: ItemRecord,
    /// The small grid of the pack or belt it lies in, if it hangs on one (the tent in a lashing, the
    /// spear on a strap); `x` and `y` are then its cell in that grid. Absent for the container's main grid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fitting: Option<fb_items::Accessory>,
}

/// What is left on a fallen body: its things, each where it lay in the body's grid, and which
/// were worn or held in what slot, so the body is drawn in them again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyRecord {
    /// How deep the body's grid is.
    pub rows: u8,
    pub contents: Vec<BodyStowed>,
}

/// One thing on a body: where it lay and, if it was worn or held, in what slot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyStowed {
    pub slot: Option<fb_items::Slot>,
    pub stowed: Stowed,
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
    /// What is left on the body ([`FolkRecord::body`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyRecord>,
    /// How charred a troll's skin is, 0 to 1 (`crate::creatures::scorch`); 0 for the rest.
    #[serde(default)]
    pub charred: f32,
}

impl CreatureRecord {
    /// The transform it stands at (or stood at, as it fell).
    pub fn transform(&self) -> Transform {
        Transform { translation: Vec3::from_array(self.at), rotation: Quat::from_array(self.turn), scale: Vec3::splat(self.scale) }
    }
}

impl WorldRecord {
    /// The record of a new delve in the world of `run_seed`: no level visited, and standing with
    /// every faction at the start a delver of `alignment` has.
    pub fn new_delve(alignment: fb_faction::Alignment, run_seed: u32) -> WorldRecord {
        WorldRecord { levels: BTreeMap::new(), standings: fb_faction::Standings::start_from_alignment(alignment, &fb_faction::World::new(run_seed)), orders: Vec::new(), keys_given: Vec::new(), met_on: BTreeMap::new(), lexicon: fb_denizens::tongue::Lexicon::new(), persuasions: Default::default(), rests: 0, insights: 0, refused: Default::default(), truces: Default::default() }
    }

    /// The record of a new delve as `delver`: alignment sets the start, and a faction that likes or
    /// hates the delver's class or folk starts apart from it (`Standings::start_as`, #547).
    pub fn new_delve_as(delver: &fb_faction::Delver, run_seed: u32) -> WorldRecord {
        WorldRecord { levels: BTreeMap::new(), standings: fb_faction::Standings::start_as(&fb_faction::StandingRules::default(), delver, &fb_faction::World::new(run_seed)), orders: Vec::new(), keys_given: Vec::new(), met_on: BTreeMap::new(), lexicon: fb_denizens::tongue::Lexicon::new(), persuasions: Default::default(), rests: 0, insights: 0, refused: Default::default(), truces: Default::default() }
    }

    /// The record of the level at `depth`, if it has been visited.
    pub fn level(&self, depth: u32) -> Option<&LevelRecord> {
        self.levels.get(&depth)
    }

    /// Makes `level`, just rebuilt from its seed, the level as it was left: the items on its
    /// floor are those recorded. A level never visited is left as generated.
    pub fn apply_to(&self, level: &mut Level) {
        if let Some(saved) = self.level(level.depth) {
            saved.apply_to(level);
        }
    }
}

impl LevelRecord {
    /// Makes `level`, just rebuilt from its seed, the level as it was left by this record.
    pub fn apply_to(&self, level: &mut Level) {
        level.items = self.floor.iter().map(|f| f.spot).collect();
        if let Some(claims) = &self.claims {
            level.claims = claims.clone();
        }
    }
}

/// `id` as it is now, with everything in it, if the inventory knows it.
pub fn record_item(inventory: &Inventory, id: ItemId) -> Option<ItemRecord> {
    let item = inventory.item(id)?;
    Some(ItemRecord { contents: record_contents(inventory, id), ..item_record(item) })
}

/// `item` as it is now, without any contents.
pub fn item_record(item: &fb_items::Item) -> ItemRecord {
    ItemRecord {
        base: item.base.clone(),
        seed: item.seed,
        shape: item.shape.clone(),
        rarity: item.rarity,
        affixes: item.affixes.iter().map(|a| a.name.clone()).collect(),
        name: item.name.clone(),
        legend: item.legend.clone(),
        history: item.history.clone(),
        coins: item.coins.map(|c| [c.gp, c.sp, c.cp]),
        contents: vec![],
        odds: item.odds,
        words: item.words.clone(),
        teaching: item.teaching.clone(),
        known: item.known,
        rest_tried: item.rest_tried,
        quantity: item.quantity,
    }
}

/// What is in `container`'s grid.
pub fn record_contents(inventory: &Inventory, container: ItemId) -> Vec<Stowed> {
    let Some(grid) = inventory.grid(container) else { return vec![] };
    let stowed = |grid: &crate::inventory::Grid<ItemId>, fitting| -> Vec<Stowed> {
        grid.entries().iter().filter_map(|e| Some(Stowed { x: e.at.x, y: e.at.y, turned: e.at.turned, item: record_item(inventory, e.id)?, fitting })).collect()
    };
    // What hangs on a pack or belt lies in its small grids, after the main grid's.
    let mut all = stowed(grid, None);
    for &(accessory, fitting) in inventory.fittings_of(container) {
        if let Some(grid) = inventory.grid(fitting) {
            all.extend(stowed(grid, Some(accessory)));
        }
    }
    all
}

/// Keeps track of the item `record` describes in `inventory`, loose, with its contents where
/// they lay. Nothing is rolled again: the rarity, affixes, name, legend and history are the
/// record's, and the stats are worked out from the affixes. `None` for a base no table
/// knows.
pub fn restore_item(inventory: &mut Inventory, record: &ItemRecord) -> Option<ItemId> {
    let item = rebuild_item(record)?;
    let id = inventory.add(item);
    restore_contents(inventory, id, &record.contents);
    Some(id)
}

/// The item `record` describes, without its contents. An affix no table knows is left off, with
/// a warning; only an unknown base gives `None` (also with a warning).
pub fn rebuild_item(record: &ItemRecord) -> Option<fb_items::Item> {
    let affixes = record
        .affixes
        .iter()
        .filter_map(|n| {
            let found = fb_items::tables::affix_named(n);
            if found.is_none() {
                warn!("recorded item {} (seed {}) has unknown affix {n:?}; leaving it off", record.base, record.seed);
            }
            found
        })
        .collect();
    let item = fb_items::Item::from_parts(fb_items::ItemParts {
        base: record.base.clone(),
        seed: record.seed,
        shape: record.shape.clone(),
        rarity: record.rarity,
        affixes,
        name: record.name.clone(),
        legend: record.legend.clone(),
        history: record.history.clone(),
        coins: record.coins.map(|[gp, sp, cp]| Coins { gp, sp, cp }),
        odds: record.odds,
        words: record.words.clone(),
        teaching: record.teaching.clone(),
        known: record.known,
        rest_tried: record.rest_tried,
        quantity: record.quantity,
    });
    if item.is_none() {
        warn!("recorded item has unknown base {:?} (seed {}); dropping it", record.base, record.seed);
    }
    item
}

/// Puts `contents` back into `container`'s grid, each where it lay and turned as it was. One
/// that cannot go there (an old quiver's twenty loose arrows, its grid since shrunk to a row of
/// stack places) folds into a stack of its kind or takes a free place; one that still has no
/// room is never forgotten: it stays loose in `inventory`, listed in [`Inventory::spill`] for the
/// game to set down underfoot, and is logged once.
pub fn restore_contents(inventory: &mut Inventory, container: ItemId, contents: &[Stowed]) {
    let mut spilled = 0;
    for stowed in contents {
        let Some(id) = restore_item(inventory, &stowed.item) else { continue };
        // Back in the small grid of the pack or belt it hung on, where it hung.
        let into = match stowed.fitting {
            Some(accessory) => inventory.fitting(container, accessory),
            None => Some(container),
        };
        if let Some(into) = into
            && inventory.put(id, into, Placement::new(stowed.x, stowed.y, stowed.turned)).is_ok()
            // A stack put in a fitting gives it one piece and keeps the rest.
            && inventory.location(id) != Some(crate::inventory::items::Location::Loose)
        {
            continue;
        }
        let _ = inventory.stow_in(id, container);
        if inventory.item(id).is_some() && inventory.location(id) == Some(crate::inventory::items::Location::Loose) {
            inventory.spill.push(id);
            spilled += 1;
        }
    }
    if spilled > 0 {
        warn!("{spilled} saved items found no room in their container and are set down underfoot");
    }
}

/// Writes the level being left into the record. Runs in [`crate::travel::LeavingLevel`], when
/// the level is still whole.
#[allow(clippy::type_complexity)]
pub fn record_level_left(
    level: Res<CurrentLevel>,
    inventory: Res<Inventory>,
    mut record: ResMut<WorldRecord>,
    floor: Query<&FloorItem>,
    containers: Query<(&PropPiece, &Container)>,
    urns: Query<(&crate::niches::NichePiece, &Container)>,
    creatures: Query<(&Critter, &Transform, Option<&Health>, Option<&Corpse>, Option<&crate::creatures::scorch::Scorch>, Option<&crate::remains::BodyStock>)>,
    folk: Query<(&Denizen, &Transform, Option<&Health>, Option<&Hostility>, Option<&Corpse>, Option<&crate::shelf::Shelf>, Option<&crate::remains::BodyStock>)>,
    doors: Option<Res<crate::doors::Doors>>,
) {
    let mut left = LevelRecord::default();
    left.doors = doors.map_or_else(Vec::new, |d| d.records());
    let mut lying: Vec<&FloorItem> = floor.iter().collect();
    lying.sort_by_key(|f| f.id);
    left.floor = lying.into_iter().filter_map(|f| Some(FloorRecord { spot: f.spot, item: record_item(&inventory, f.id)? })).collect();
    for (piece, container) in &containers {
        left.containers.insert(piece.index, ContainerRecord { opened: container.opened, contents: record_contents(&inventory, container.id) });
    }
    for (piece, container) in &urns {
        left.urns.insert(piece.index, ContainerRecord { opened: container.opened, contents: record_contents(&inventory, container.id) });
    }
    for (critter, transform, health, corpse, scorch, body) in &creatures {
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
                body: body.and_then(|b| crate::remains::record_body(&inventory, b)),
                charred: scorch.map_or(0.0, |s| s.level),
            },
        );
    }
    for (denizen, transform, health, hostility, corpse, shelf, body) in &folk {
        let Some(index) = level.0.folk.iter().position(|s| *s == denizen.person.spot) else { continue };
        if let Some(shelf) = shelf.filter(|_| corpse.is_none()) {
            left.shelves.insert(index, ShelfRecord { day: shelf.day, wares: shelf.wares.iter().map(item_record).collect(), sold: shelf.sold.iter().map(item_record).collect(), purse: Some(shelf.purse) });
        }
        let Some(health) = health else { continue };
        let stood = corpse.map_or(*transform, |c| c.fell);
        left.folk.insert(
            index,
            FolkRecord {
                hp: health.hp,
                max: health.max,
                at: stood.translation.to_array(),
                turn: stood.rotation.to_array(),
                hostile: hostility.is_some_and(|h| h.is_hostile()),
                dead: corpse.is_some(),
                body: body.and_then(|b| crate::remains::record_body(&inventory, b)),
            },
        );
    }
    // Folk the record has no entry for are as generated. `level.0.claims` are the claims as the
    // level was last left, so settling them only ever shrinks them.
    let alive: Vec<Option<(f32, f32)>> = (0..level.0.folk.len())
        .map(|i| match left.folk.get(&i) {
            Some(f) if f.dead => None,
            Some(f) => Some((f.at[0] / CELL, f.at[2] / CELL)),
            None => Some((level.0.folk[i].x, level.0.folk[i].y)),
        })
        .collect();
    left.claims = Some(settle_claims(&level.0, &alive));
    record.levels.insert(level.0.depth, left);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `item` recorded, loose.
    fn record_of(item: &fb_items::Item) -> ItemRecord {
        let mut inventory = Inventory::default();
        let id = inventory.add(item.clone());
        record_item(&inventory, id).unwrap()
    }

    /// `item` through a record, as JSON, and back.
    fn through_record(item: &fb_items::Item) -> fb_items::Item {
        let json = serde_json::to_string(&record_of(item)).unwrap();
        rebuild_item(&serde_json::from_str(&json).unwrap()).unwrap()
    }

    #[test]
    fn the_depths_where_factions_were_met_survive_a_save() {
        let mut record = WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, 4);
        let (a, b) = (fb_faction::Faction(1), fb_faction::Faction(3));
        record.note_met_on(a, 2);
        record.note_met_on(a, 5);
        record.note_met_on(b, 1);
        let text = serde_json::to_string(&record).unwrap();
        let back: WorldRecord = serde_json::from_str(&text).unwrap();
        assert_eq!(back.met_on, record.met_on);
        assert_eq!(back.met_on[&a].iter().copied().collect::<Vec<_>>(), vec![2, 5]);
    }

    #[test]
    fn folk_persuaded_refused_and_at_truce_survive_a_save() {
        let mut record = WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, 4);
        record.note_persuasion((2, 77));
        record.note_persuasion((2, 77));
        record.refused.insert((3, 9), 1);
        record.make_truce(3, fb_faction::Faction(2));
        let text = serde_json::to_string(&record).unwrap();
        let back: WorldRecord = serde_json::from_str(&text).unwrap();
        assert_eq!(back.persuasion_tries((2, 77)), 2);
        assert_eq!(back.refused, record.refused);
        assert!(back.truce_holds(3, fb_faction::Faction(2)));
    }

    #[test]
    fn a_record_saved_with_empty_persuasions_as_objects_still_loads() {
        let record = WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, 4);
        let mut value = serde_json::to_value(&record).unwrap();
        value["persuasions"] = serde_json::json!({});
        value["refused"] = serde_json::json!({});
        value["truces"] = serde_json::json!({});
        let back: WorldRecord = serde_json::from_value(value).unwrap();
        assert!(back.persuasions.is_empty() && back.refused.is_empty() && back.truces.is_empty());
    }

    #[test]
    fn a_record_saved_before_met_on_loads_with_none_noted() {
        let record = WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, 4);
        let mut value = serde_json::to_value(&record).unwrap();
        value.as_object_mut().unwrap().remove("met_on").expect("the field is written");
        let back: WorldRecord = serde_json::from_value(value).unwrap();
        assert!(back.met_on.is_empty());
    }

    #[test]
    fn an_unchanged_rolled_item_rebuilds_identically() {
        let odds = fb_items::Odds::loot(8);
        let legend = (1..2_000).filter_map(|s| fb_items::roll_item_with("Sword", s, odds)).find(|i| i.rarity == fb_items::Rarity::Legendary).unwrap();
        for item in [fb_items::roll_item("Cloak", 4).unwrap(), fb_items::roll_item("Purse", 11).unwrap(), legend] {
            assert_eq!(through_record(&item), item);
        }
    }

    #[test]
    fn an_item_whose_affixes_changed_keeps_the_new_ones_and_their_stats() {
        let mut parts = fb_items::ItemParts::from(fb_items::roll_item("Dagger", 6).unwrap());
        parts.affixes = vec![];
        let plain = fb_items::Item::from_parts(parts.clone()).unwrap();
        let quick = fb_items::tables::affix_named("Quick").unwrap();
        parts.affixes = vec![quick];
        parts.name = fb_items::naming::item_name(&fb_items::naming::noun(&parts.base, &parts.shape), &parts.affixes);
        let changed = fb_items::Item::from_parts(parts).unwrap();
        assert!(changed.profile.stats.attack_rate > plain.profile.stats.attack_rate);
        let again = through_record(&changed);
        assert_eq!(again, changed);
        assert_eq!(again.affixes, vec![quick]);
        assert!(again.name.starts_with("Quick"));
    }

    #[test]
    fn a_teaching_scroll_and_the_lexicon_keep_through_a_record() {
        let scroll = crate::reading::teaching_for_capture("faun:book:9", 0).unwrap();
        assert!(scroll.teaching().is_some());
        let rebuilt = rebuild_item(&item_record(&scroll)).unwrap();
        assert_eq!(rebuilt, scroll);
        let mut record = WorldRecord::default();
        record.lexicon.learn_teaching(scroll.teaching().unwrap());
        let reloaded: WorldRecord = serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
        assert_eq!(reloaded.lexicon(), record.lexicon());
        assert!(reloaded.lexicon().known_share(fb_denizens::DenizenKind::Faun) > 0.0);
        let fresh = WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, 5);
        assert_eq!(fresh.lexicon().known_share(fb_denizens::DenizenKind::Faun), 0.0, "a new delve starts with none");
    }

    #[test]
    fn a_read_scroll_stays_read_through_a_record() {
        let mut scroll = crate::reading::teaching_for_capture("minotaur:scroll:7", 0).unwrap();
        let other = crate::reading::teaching_for_capture("minotaur:scroll:8", 0).unwrap();
        let mut lexicon = fb_denizens::tongue::Lexicon::new();
        let mut parts = fb_items::ItemParts::from(scroll.clone());
        assert!(crate::reading::study(&mut lexicon, parts.teaching.as_mut().unwrap()).starts_with("You learn "));
        scroll = fb_items::Item::from_parts(parts).unwrap();
        let json = serde_json::to_string(&item_record(&scroll)).unwrap();
        let mut back = rebuild_item(&serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(back, scroll);
        let again = crate::reading::study(&mut lexicon, back.teaching.as_mut().unwrap());
        assert_eq!(again, "You have learned all this writing can teach.");
        let mut other = other;
        assert!(crate::reading::study(&mut lexicon, other.teaching.as_mut().unwrap()).starts_with("You learn "));
    }

    #[test]
    fn a_note_keeps_its_words_through_a_record() {
        let level = fb_level::generate(5, 2);
        let note = fb_items::roll_item("Note", 31).unwrap().in_world(&level.lore());
        assert!(note.words.is_some());
        assert_eq!(through_record(&note), note);
    }

    #[test]
    fn an_unknown_affix_is_left_off_and_the_rest_of_the_item_restores() {
        let mut item = fb_items::roll_item("Sword", 9).unwrap();
        item.history.push("Made by Brenna Holt".into());
        let mut record = record_of(&item);
        record.affixes = vec!["Nope".into()];
        let again = rebuild_item(&record).unwrap();
        assert!(again.affixes.is_empty());
        assert_eq!((again.name.as_str(), again.rarity, again.history.clone(), again.shape.clone()), (item.name.as_str(), item.rarity, item.history.clone(), item.shape.clone()));
        let mut inventory = Inventory::default();
        assert!(restore_item(&mut inventory, &record).is_some());
        record.base = "Nothing".into();
        assert!(rebuild_item(&record).is_none());
    }

    #[test]
    fn a_shelf_round_trips_through_the_level_record() {
        let odds = fb_items::Odds::loot(8);
        let mut wares = vec![fb_items::roll_item_with("Mail", 4, odds).unwrap(), fb_items::roll_item_with("Sword", 5, odds).unwrap()];
        wares[0].history.push("Made by Bera, master mailwright of the Ash Choir.".into());
        let mut world = WorldRecord::default();
        let mut level = LevelRecord::default();
        level.shelves.insert(3, ShelfRecord { day: 2, wares: wares.iter().map(item_record).collect(), sold: vec![], purse: None });
        world.levels.insert(8, level);
        let json = serde_json::to_string(&world).unwrap();
        let again: WorldRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(again, world);
        let shelf = &again.level(8).unwrap().shelves[&3];
        assert_eq!(shelf.day, 2);
        let rebuilt: Vec<_> = shelf.wares.iter().map(|r| rebuild_item(r).unwrap()).collect();
        assert_eq!(rebuilt, wares);
        // A record from before shelves has none.
        let old: LevelRecord = serde_json::from_str(r#"{"floor":[],"containers":{},"creatures":{}}"#).unwrap();
        assert!(old.shelves.is_empty());
    }

    #[test]
    fn a_history_line_round_trips() {
        let mut item = fb_items::roll_item("Sword", 9).unwrap();
        item.history.push("Made by Brenna Holt".into());
        let again = through_record(&item);
        assert_eq!(again.history, vec!["Made by Brenna Holt".to_string()]);
        assert_eq!(again, item);
    }

    #[test]
    fn a_hallowed_mace_stays_hallowed_through_a_save() {
        let mace = fb_items::roll_item("Mace", 7).unwrap();
        let hallowed = fb_items::blessing::hallow(&mace, "Maren", armory::Family::Mace).unwrap();
        let again = through_record(&hallowed);
        assert!(fb_items::blessing::is_hallowed(&again));
        assert_eq!(again.history, vec!["Hallowed by Maren.".to_string()]);
        assert_eq!(again, hallowed);
        assert!(again.profile.stats.damage_bonus > mace.profile.stats.damage_bonus);
    }

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
    fn liveried_gear_keeps_its_colours_through_a_record() {
        use armory::heraldry::{Field, Tincture};
        use fb_items::outfit::{ArmourLean, Livery, outfit};
        let field = Field { tincture: Tincture::Vert, tincture2: Tincture::Or, ..Field::default() };
        let livery = Livery::from_field(field, ArmourLean::Plate);
        let robe = outfit("mage").unwrap().roll_in(1, "human", fb_items::Odds::default(), Some(&livery)).worn.into_iter().find(|(_, i)| i.base == "Robe").unwrap().1;
        let shield = (0..40)
            .filter_map(|s| outfit("fighter").unwrap().roll_in(s, "human", fb_items::Odds::default(), Some(&livery)).worn.into_iter().find(|(_, i)| i.base == "Shield"))
            .next()
            .unwrap()
            .1;
        let mut inventory = Inventory::default();
        let mut other = Inventory::default();
        for item in [robe, shield] {
            let want = item.shape.clone();
            let id = inventory.add(item);
            let record: ItemRecord = serde_json::from_str(&serde_json::to_string(&record_item(&inventory, id).unwrap()).unwrap()).unwrap();
            let again = restore_item(&mut other, &record).unwrap();
            assert_eq!(other.item(again).unwrap().shape, want);
            if let armory::Item::Shield(s) = &other.item(again).unwrap().shape {
                assert_eq!(s.field, field);
            }
        }
    }

    #[test]
    fn an_items_known_state_survives_a_record_and_an_old_record_reads_as_known() {
        use fb_items::Known;
        let ring = (1..20_000).filter_map(|s| fb_items::roll_item_with("Ring", s, fb_items::Odds::loot(10))).find(|i| i.rarity >= fb_items::Rarity::Rare && i.affixes.len() >= 2).unwrap();
        let mut unknown = ring.clone().unidentified();
        assert_eq!(unknown.known, Known::Unknown);
        let back = |item: &fb_items::Item| rebuild_item(&serde_json::from_str(&serde_json::to_string(&record_of(item)).unwrap()).unwrap()).unwrap();
        assert_eq!(back(&unknown).known, Known::Unknown);
        unknown.reveal(unknown.identify_dc() - 3);
        let partly = unknown.known;
        assert!(matches!(partly, Known::Partly(_)));
        assert_eq!(back(&unknown).known, partly);
        unknown.reveal(unknown.identify_dc());
        assert_eq!(back(&unknown).known, Known::Full);
        // A record from before items could be unknown has no such field, and reads as known.
        let mut json: serde_json::Value = serde_json::to_value(record_of(&ring.unidentified())).unwrap();
        json.as_object_mut().unwrap().remove("known");
        let old: ItemRecord = serde_json::from_value(json).unwrap();
        assert_eq!(rebuild_item(&old).unwrap().known, Known::Full);
    }

    #[test]
    fn a_quiver_and_its_arrows_round_trip_and_a_loose_arrow_rolls_again() {
        let mut inventory = Inventory::default();
        let quiver = inventory.add(fb_items::roll_item("Quiver", 5).unwrap());
        for _ in 0..20 {
            let arrow = inventory.add(fb_items::roll_item("Arrow", 3).unwrap());
            inventory.stow_in(arrow, quiver).unwrap();
        }
        let json = serde_json::to_string(&record_item(&inventory, quiver).unwrap()).unwrap();
        let mut other = Inventory::default();
        let again = restore_item(&mut other, &serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!((other.contents_count(again), other.pieces_in(again)), (2, 20));
        assert_eq!(other.item(again), inventory.item(quiver));
        // A shot that came to rest is a level spot of its family and seed, which rolls the arrow again.
        let arrow = fb_items::roll_item("Arrow", 77).unwrap();
        assert_eq!(fb_items::roll_loot(arrow.shape.family(), arrow.seed, arrow.odds), Some(arrow));
    }

    fn stowed(inventory: &Inventory, id: ItemId, x: u8, y: u8, turned: bool) -> Stowed {
        Stowed { x, y, turned, item: record_item(inventory, id).unwrap(), fitting: None }
    }

    #[test]
    fn an_old_quiver_of_twenty_kinds_of_arrow_loses_nothing() {
        let mut inventory = Inventory::default();
        let quiver = inventory.add(fb_items::roll_item("Quiver", 5).unwrap());
        let mut loose = Inventory::default();
        // The old 5 by 4 grid: twenty arrows, each its own seed, most beyond today's row.
        let contents: Vec<Stowed> = (0..20u8)
            .map(|i| {
                let arrow = loose.add(fb_items::roll_item("Arrow", 100 + u32::from(i)).unwrap());
                stowed(&loose, arrow, i % 5, i / 5, false)
            })
            .collect();
        restore_contents(&mut inventory, quiver, &contents);
        let spilled: u32 = inventory.spill.iter().filter_map(|&id| inventory.item(id)).map(|i| i.quantity).sum();
        assert_eq!(inventory.pieces_in(quiver) as u32 + spilled, 20);
        assert!(spilled > 0 && inventory.spill.iter().all(|&id| inventory.location(id) == Some(crate::inventory::items::Location::Loose)));
    }

    #[test]
    fn identical_old_arrows_fold_into_the_quivers_stacks() {
        let mut inventory = Inventory::default();
        let quiver = inventory.add(fb_items::roll_item("Quiver", 5).unwrap());
        let mut loose = Inventory::default();
        let arrow = loose.add(fb_items::roll_item("Arrow", 3).unwrap());
        let contents: Vec<Stowed> = (0..20u8).map(|i| stowed(&loose, arrow, i % 5, i / 5, false)).collect();
        restore_contents(&mut inventory, quiver, &contents);
        assert_eq!((inventory.pieces_in(quiver), inventory.spill.len()), (20, 0));
    }

    #[test]
    fn a_pack_and_a_chest_restore_their_stacks_where_and_as_they_were_saved() {
        let mut source = Inventory::default();
        let pack = source.add(fb_items::roll_item("Pack", 1).unwrap());
        let stack = source.add(fb_items::roll_item("Arrow", 9).unwrap().with_quantity(7));
        source.put(stack, pack, Placement::new(3, 2, false)).unwrap();
        let sword = source.add(fb_items::roll_item("Sword", 2).unwrap());
        source.put(sword, pack, Placement::new(0, 1, true)).unwrap();
        let record = record_item(&source, pack).unwrap();
        let mut other = Inventory::default();
        let again = restore_item(&mut other, &record).unwrap();
        let at: Vec<(String, u32, u8, u8, bool)> = other
            .grid(again)
            .unwrap()
            .entries()
            .iter()
            .map(|e| {
                let i = other.item(e.id).unwrap();
                (i.base.clone(), i.quantity, e.at.x, e.at.y, e.at.turned)
            })
            .collect();
        assert!(at.contains(&("Arrow".into(), 7, 3, 2, false)), "{at:?}");
        assert!(at.contains(&("Sword".into(), 1, 0, 1, true)), "{at:?}");
        assert!(other.spill.is_empty());
        // A chest restores the same way.
        let chest = other.add(fb_items::container_item(fb_level::chest_state(armory::families::chests::ChestStyle::Chest, 4, false)));
        restore_contents(&mut other, chest, &[stowed(&source, stack, 2, 1, false)]);
        let there = other.grid(chest).unwrap().entries()[0];
        assert_eq!((there.at.x, there.at.y), (2, 1));
    }

    #[test]
    fn a_container_too_full_spills_rather_than_forgets() {
        let mut inventory = Inventory::default();
        let case = inventory.add(fb_items::roll_item("Bolt case", 1).unwrap());
        let places = inventory.item(case).unwrap().profile.holds.as_ref().unwrap().cols;
        let mut loose = Inventory::default();
        let contents: Vec<Stowed> = (0..u32::from(places) + 2)
            .map(|k| {
                let bolt = loose.add(fb_items::roll_item("Bolt", 40 + k).unwrap());
                stowed(&loose, bolt, 0, 0, false)
            })
            .collect();
        restore_contents(&mut inventory, case, &contents);
        assert_eq!((inventory.contents_count(case), inventory.spill.len()), (usize::from(places), 2));
        assert!(inventory.spill.iter().all(|&id| inventory.item(id).is_some()));
    }

    #[test]
    fn a_stack_keeps_its_count_and_an_old_record_has_one() {
        let mut inventory = Inventory::default();
        let stack = inventory.add(fb_items::roll_item("Arrow", 9).unwrap().with_quantity(7));
        let json = serde_json::to_string(&record_item(&inventory, stack).unwrap()).unwrap();
        let mut other = Inventory::default();
        let again = restore_item(&mut other, &serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(other.item(again).unwrap().quantity, 7);
        let single = serde_json::to_string(&item_record(&fb_items::roll_item("Arrow", 9).unwrap())).unwrap();
        assert!(!single.contains("quantity"), "a lone piece writes no count, so older saves read the same");
        let mut old: serde_json::Value = serde_json::from_str(&json).unwrap();
        old.as_object_mut().unwrap().remove("quantity");
        let again = restore_item(&mut other, &serde_json::from_value(old).unwrap()).unwrap();
        assert_eq!(other.item(again).unwrap().quantity, 1);
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
    fn standing_is_shared_by_a_factions_camps_on_every_level() {
        let seed = 1207;
        let params = fb_level::LevelParams::default();
        let levels: Vec<_> = (1..=12).map(|d| fb_level::generate_in(seed, d, &params)).collect();
        let faction = levels
            .iter()
            .flat_map(|l| l.factions.iter().map(|f| f.id))
            .find(|id| levels.iter().filter(|l| l.folk.iter().any(|s| s.faction == *id)).count() >= 2)
            .expect("a faction with camps on two levels");
        let camps: Vec<_> = levels.iter().filter(|l| l.folk.iter().any(|s| s.faction == faction)).collect();
        let mut record = WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, seed);
        let before = record.standings.get(faction).value();
        // Standing raised at a camp on one level reads the same at a camp on another.
        record.standings.shift(faction, 25);
        for level in camps {
            let spot = level.folk.iter().find(|s| s.faction == faction).unwrap();
            assert_eq!(record.standings.get(spot.faction).value(), before + 25, "depth {}", level.depth);
        }
    }

    #[test]
    fn a_new_delve_starts_standing_afresh_from_the_alignment() {
        let mut record = WorldRecord::new_delve(fb_faction::Alignment::NeutralGood, 5);
        let world = fb_faction::World::new(5);
        let first = world.factions()[0].id;
        let start = record.standings.get(first);
        record.standings.shift(first, 60);
        record.standings.turn_hostile(world.factions()[1].id);
        assert_ne!(record.standings.get(first), start);
        let fresh = WorldRecord::new_delve(fb_faction::Alignment::NeutralGood, 5);
        assert_eq!(fresh.standings.get(first), start);
        assert_eq!(fresh.standings.met().count(), 0);
        assert!(fresh.levels.is_empty());
    }

    #[test]
    fn a_faction_that_turns_reads_hostile_and_the_record_keeps_it() {
        let mut record = WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, 9);
        let faction = fb_faction::World::new(9).factions().iter().find(|f| f.alignment == fb_faction::Alignment::LawfulGood).map_or(fb_faction::Faction(0), |f| f.id);
        record.standings.turn_hostile(faction);
        assert_eq!(record.standings.band(faction), fb_faction::Band::Hostile);
        let json = serde_json::to_string(&record).unwrap();
        assert_eq!(serde_json::from_str::<WorldRecord>(&json).unwrap(), record);
    }

    #[test]
    fn claims_round_trip_and_come_back_as_left() {
        let level = fb_level::generate(1207, 1);
        let faction = level.claims[0].faction;
        let mut alive: Vec<Option<(f32, f32)>> = level.folk.iter().map(|f| Some((f.x, f.y))).collect();
        let members: Vec<usize> = (0..level.folk.len()).filter(|&i| level.folk[i].faction == faction).collect();
        members[1..].iter().for_each(|&i| alive[i] = None);
        let mut saved = LevelRecord::default();
        saved.claims = Some(settle_claims(&level, &alive));
        assert_eq!(saved.claims.as_ref().unwrap()[0].rooms.len(), 1);
        let mut record = WorldRecord::default();
        record.levels.insert(1, saved.clone());
        let json = serde_json::to_string(&record).unwrap();
        let record: WorldRecord = serde_json::from_str(&json).unwrap();
        // Rebuilt from the seed, then made as it was left.
        let mut again = fb_level::generate(1207, 1);
        record.apply_to(&mut again);
        assert_eq!(Some(&again.claims), saved.claims.as_ref());
        // A level never visited keeps the generated claims, and a record from before claims has none to apply.
        let mut fresh = fb_level::generate(1207, 1);
        WorldRecord::default().apply_to(&mut fresh);
        assert_eq!(fresh.claims, level.claims);
        let old: LevelRecord = serde_json::from_str(r#"{"floor":[],"containers":{},"creatures":{}}"#).unwrap();
        assert_eq!(old.claims, None);
    }

    #[test]
    fn the_world_record_round_trips_through_serde_unchanged() {
        let mut record = WorldRecord::default();
        let item = fb_items::roll_item("Purse", 11).unwrap();
        let mut level = LevelRecord::default();
        level.floor.push(FloorRecord {
            spot: ItemSpot { x: 3.25, y: 4.5, yaw: 0.3, family: item.shape.family(), seed: item.seed },
            item: ItemRecord { coins: Some([1, 2, 3]), odds: fb_items::Odds::kit(4), ..record_of(&item) },
        });
        level.containers.insert(2, ContainerRecord { opened: true, contents: vec![] });
        level.creatures.insert(1, CreatureRecord { hp: 2, max: 5, at: [1.5, 0.0, 2.25], turn: [0.0, 0.7, 0.0, 0.7], scale: 1.25, dead: false, body: None, charred: 0.0 });
        level.folk.insert(0, FolkRecord { hp: 3, max: 9, at: [4.0, 0.0, 6.5], turn: [0.0, 0.0, 0.0, 1.0], hostile: true, dead: false, body: None });
        record.levels.insert(1, level);
        record.levels.insert(4, LevelRecord::default());
        let json = serde_json::to_string(&record).unwrap();
        assert_eq!(serde_json::from_str::<WorldRecord>(&json).unwrap(), record);
    }

    #[test]
    fn what_hangs_on_a_pack_or_belt_is_recorded_with_it_wherever_it_lies() {
        use fb_items::Accessory::{LeftFrog, Lashing, WeaponStrap};
        let mut inventory = Inventory::default();
        let pack = (1..4000).filter_map(|s| fb_items::roll_item("Pack", s)).find(|p| p.has_accessory(Lashing) && p.has_accessory(WeaponStrap)).unwrap();
        let pack = inventory.add(pack);
        let (tent, spear, loaf) = (inventory.add(fb_items::roll_item("Tent", 3).unwrap()), inventory.add(fb_items::roll_item("Spear", 3).unwrap()), inventory.add(fb_items::roll_item("Bread", 3).unwrap()));
        let (lashing, strap) = (inventory.fitting(pack, Lashing).unwrap(), inventory.fitting(pack, WeaponStrap).unwrap());
        inventory.put(tent, lashing, Placement::new(0, 0, false)).unwrap();
        inventory.put(spear, strap, Placement::new(0, 0, false)).unwrap();
        inventory.put(loaf, pack, Placement::new(2, 1, false)).unwrap();
        // A belt with a dagger in its frog, packed in a sack.
        let belt = (1..4000).filter_map(|s| fb_items::roll_item("Belt", s)).find(|b| b.has_accessory(LeftFrog)).unwrap();
        let belt = inventory.add(belt);
        let dagger = inventory.add(fb_items::roll_item("Dagger", 3).unwrap());
        inventory.put(dagger, inventory.fitting(belt, LeftFrog).unwrap(), Placement::new(0, 0, false)).unwrap();
        let sack = inventory.add(fb_items::roll_item("Sack", 3).unwrap());
        inventory.put(belt, sack, Placement::new(0, 0, false)).unwrap();
        for (host, weight) in [(pack, inventory.contents_weight(pack)), (sack, inventory.contents_weight(sack))] {
            // Down to the level record's json and back, as a bag set down or a chest's contents are.
            let json = serde_json::to_string(&record_item(&inventory, host).unwrap()).unwrap();
            assert!(json.contains("\"fitting\""), "the fitting grids are recorded");
            let mut other = Inventory::default();
            let again = restore_item(&mut other, &serde_json::from_str(&json).unwrap()).unwrap();
            assert_eq!(other.contents_weight(again), weight, "the same load");
            assert_eq!(record_item(&other, again), record_item(&inventory, host), "and the same record");
        }
        let mut other = Inventory::default();
        let again = restore_item(&mut other, &record_item(&inventory, pack).unwrap()).unwrap();
        assert_eq!(other.hung_on(again).iter().map(|&(a, id)| (a, other.item(id).unwrap().base.clone())).collect::<Vec<_>>(), vec![(Lashing, "Tent".to_string()), (WeaponStrap, "Spear".to_string())]);
        assert_eq!(other.contents_count(again), 1, "and the loaf in the main grid");
        // Counted without duplicates: the items are the pack, its three things and nothing else.
        assert_eq!(other.ids().count(), 4);
        // A save that put a fitting's contents where its grid is gone puts them in the main grid, never loses them.
        let mut record = record_item(&inventory, pack).unwrap();
        record.contents[1].fitting = Some(fb_items::Accessory::PotionPouch);
        let mut third = Inventory::default();
        let kept = restore_item(&mut third, &record).unwrap();
        assert_eq!(third.ids().count(), 4, "{:?}", third.spill);
        assert!(third.holds(kept, third.ids().find(|&id| third.item(id).is_some_and(|i| i.base == "Tent" || i.base == "Spear")).unwrap()) || !third.spill.is_empty());
    }
}
