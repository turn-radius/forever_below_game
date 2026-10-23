//! Saving the delve, and resuming it (#689).
//!
//! A delve is saved in one place only: when the delver lies down in a set bedroll or tent
//! ([`save_in_camp`], called by [`crate::camp`]). It is written to `delve.json` beside
//! `settings.json` ([`SaveFile::default_path`]), as one JSON document, [`SavedDelve`], versioned
//! ([`VERSION`]) and written atomically: to `delve.json.partial` first, flushed, then renamed over
//! the old file, so a crash never leaves half a save. A save that cannot be read is renamed to
//! `delve.unreadable.json` ([`set_aside`]) and the title says so in a line; it never panics.
//!
//! What a delve carries, and where it is kept:
//!
//! | state | kept as |
//! |---|---|
//! | the world | [`SavedDelve::run_seed`]: every level is rebuilt from it ([`Abyss::level_at`]) |
//! | the depth the delver is on | [`SavedDelve::depth`] |
//! | every level visited, the current one recorded as if just left | [`SavedDelve::record`], the [`WorldRecord`]: floor items, containers with their contents, creatures, folk, shelves, doors, claims, tents and bedrolls still pitched |
//! | faction standings, orders (commissions), keys given, where factions were met | in the [`WorldRecord`] |
//! | who the delver is, what books taught them | [`SavedDelve::delver`] |
//! | everything worn and carried, every bag's contents, the coin purse | [`SavedDelve::pack`] |
//! | the quickslots | [`SavedPack::quickslots`], as positions in the pack's walk ([`walk`]) |
//! | hit points, hunger, thirst, sleepiness, stamina, mana, exhaustion, effects, healing progress | [`SavedBody`] |
//! | where the delver stands and looks | [`SavedBody`] |
//! | the game clock | [`SavedDelve::clock`] |
//! | the times the delver has come to | [`SavedDelve::revives`] |
//! | the enchanting stock scrapping gave | [`SavedDelve::enchanting`] |
//! | what the map has seen of each level | [`SavedDelve::explored`] |
//! | the camp they lie in | [`SavedDelve::camp`] |
//!
//! Not kept, because it is rebuilt or is only the moment: combat dice (drawn afresh each run), a
//! nocked arrow, the hunt of every creature, open screens, toasts and the picture of the world
//! (meshes, lights, sounds). `tests/save_delve.rs` lists every resource and every component of the
//! delver and says which, so a new piece of state cannot be forgotten silently.
//!
//! Resuming rebuilds the world from its seed and the levels from the record: the title
//! ([`begin_resume`]) lays the saved depth out as travel does and enters the delve, and the first
//! frame of the delve ([`restore_delve`]) puts back everything above, before any system plays.
//! The delver then lies in their camp again ([`crate::camp::PendingBed`]) and wakes as any sleeper.
//!
//! Death deletes the save ([`forget_save_on_death`]). Only a delve begun from the title's creator,
//! Continue or Resume ([`Saveable`]) writes or deletes a save: one begun by `--seed`, by
//! `FB_NEW_DELVE` or by a debug switch never touches it, so captures cannot overwrite a real save.
//! `FB_SAVE_FILE=<path>` is the exception for captures: it names the file and lets any delve use it.

use std::io::Write;
use std::path::{Path, PathBuf};

use bevy::{ecs::system::RunSystemOnce, prelude::*};
use fb_character::GameTime;
use fb_items::Coins;
use fb_items::scrap::EnchantingStock;
use serde::{Deserialize, Serialize};

use crate::camp::{PendingBed, SetCamp};
use crate::carry::FloorItem;
use crate::character::{Character, ChosenDelver, DelverEffects, DelverMana, DelverNeeds, DelverRecovery, DelverStamina};
use crate::combat::Health;
use crate::combat::revival::Revives;
use crate::combat::vitals::Fallen;
use crate::enchanting::Enchanting;
use crate::exhaustion::{Exhaustion, ExhaustionTimes};
use crate::game_clock::GameClock;
use crate::inventory::{Inventory, ItemId};
use crate::launch::AppState;
use crate::map::explore::ExploredLevels;
use crate::play::{CurrentLevel, DelveSystems, Player};
use crate::player::Locomotion;
use crate::quickslots::{Quickslots, SLOTS};
use crate::record::{ItemRecord, WorldRecord, record_item, record_level_left, restore_item};
#[cfg(test)]
use crate::record::{Stowed, item_record};
use crate::settings::SettingsFile;
use crate::title::last_delver::Saved as SavedDelver;
use crate::travel::{Abyss, Arrival};

/// The file's name, beside `settings.json`.
pub const FILE_NAME: &str = "delve.json";

/// What a save that could not be read is renamed to.
pub const ASIDE_NAME: &str = "delve.unreadable.json";

/// The save format. A file of any other version is set aside rather than guessed at.
pub const VERSION: u32 = 1;

/// Where the saved delve is kept; `None` keeps nothing (tests).
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveFile(pub Option<PathBuf>);

impl SaveFile {
    /// `FB_SAVE_FILE` if set, else beside the settings file ([`SettingsFile::default_path`]).
    pub fn default_path() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("FB_SAVE_FILE").filter(|p| !p.is_empty()) {
            return Some(path.into());
        }
        SettingsFile::default_path().map(|settings| settings.with_file_name(FILE_NAME))
    }
}

/// Whether this delve may write and delete the save: set by the title when the player begins a
/// delve from the creator, Continue or Resume with no `--seed`. A delve begun any other way
/// leaves the save alone.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Saveable(pub bool);

/// Whether a delve with `saveable` may touch the save file: `FB_SAVE_FILE` lets any delve, for
/// captures.
pub fn may_touch(saveable: Option<&Saveable>) -> bool {
    saveable.is_some_and(|s| s.0) || crate::launch::env_set("FB_SAVE_FILE")
}

/// The delve as the file holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedDelve {
    pub version: u32,
    /// The seed of the world, which is the first level's.
    pub run_seed: u32,
    /// The depth the delver was on.
    pub depth: u32,
    pub delver: SavedDelver,
    /// Every level visited, the one the delver was on included as it stood, and the standings,
    /// orders, keys and meetings that last the delve.
    pub record: WorldRecord,
    pub pack: SavedPack,
    pub body: SavedBody,
    pub clock: GameTime,
    pub revives: u32,
    pub enchanting: EnchantingStock,
    pub explored: Vec<SavedExplored>,
    /// The camp they lay down in.
    pub camp: fb_level::ItemSpot,
}

/// What the delver wears and carries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedPack {
    /// Each worn or held item with its slot, in the slots' order, with everything in it.
    pub worn: Vec<(fb_items::Slot, ItemRecord)>,
    /// The coin purse: gold, silver, copper.
    pub coins: [u32; 3],
    /// The item each quickslot is bound to, as its position in [`walk`] of `worn`.
    pub quickslots: [Option<u32>; SLOTS],
}

/// The delver's own body: where it stands and what state it is in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedBody {
    pub at: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub hit_points: i32,
    pub max_hit_points: i32,
    pub needs: fb_character::Needs,
    pub stamina: fb_character::Stamina,
    pub mana: fb_character::Mana,
    pub exhaustion: fb_character::Exhaustion,
    pub exhaustion_times: fb_character::ExhaustionTimes,
    pub recovery: fb_character::recovery::Recovery,
    pub effects: Option<fb_character::effect::Effects>,
}

/// The cells of one level the map has seen, packed as bits and written in hex.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedExplored {
    pub level_seed: u32,
    pub width: u32,
    pub height: u32,
    pub bits: String,
}

/// Why a save could not be written or read.
#[derive(Debug)]
pub enum SaveError {
    Io(std::io::Error),
    /// The text is not a delve: truncated, garbled or of a shape this version does not know.
    Unreadable(String),
    /// A save of another version.
    Version(u32),
    /// Something the save needs is not in the game now.
    Missing(&'static str),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Io(error) => write!(f, "{error}"),
            SaveError::Unreadable(why) => write!(f, "{why}"),
            SaveError::Version(version) => write!(f, "it is version {version}, and this game reads version {VERSION}"),
            SaveError::Missing(what) => write!(f, "there is no {what} to save"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<std::io::Error> for SaveError {
    fn from(error: std::io::Error) -> Self {
        SaveError::Io(error)
    }
}

/// Writes `delve` to `path`, making its folder if need be: to a `.partial` file beside it,
/// flushed to disk, then renamed over `path`, so `path` is always the old save or the new one.
pub fn write(path: &Path, delve: &SavedDelve) -> Result<(), SaveError> {
    if let Some(folder) = path.parent().filter(|f| !f.as_os_str().is_empty()) {
        std::fs::create_dir_all(folder)?;
    }
    let text = serde_json::to_vec(delve).map_err(|e| SaveError::Unreadable(e.to_string()))?;
    let partial = path.with_extension("json.partial");
    let mut file = std::fs::File::create(&partial)?;
    file.write_all(&text)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&partial, path)?;
    Ok(())
}

/// Reads the delve saved at `path`. A missing file is an `Io` error of kind `NotFound`; any file
/// that is not a delve of this [`VERSION`] is an error and never a panic.
pub fn read(path: &Path) -> Result<SavedDelve, SaveError> {
    let text = std::fs::read(path)?;
    #[derive(Deserialize)]
    struct Header {
        version: u32,
    }
    let header: Header = serde_json::from_slice(&text).map_err(|e| SaveError::Unreadable(e.to_string()))?;
    if header.version != VERSION {
        return Err(SaveError::Version(header.version));
    }
    let delve: SavedDelve = serde_json::from_slice(&text).map_err(|e| SaveError::Unreadable(e.to_string()))?;
    if delve.delver.name.trim().is_empty() || delve.depth == 0 {
        return Err(SaveError::Unreadable("the delver has no name or the depth is nought".into()));
    }
    Ok(delve)
}

/// Renames the unreadable file at `path` to [`ASIDE_NAME`] beside it, replacing an earlier one,
/// and returns the new place, or `None` if it could not be moved (then it is left where it is).
pub fn set_aside(path: &Path) -> Option<PathBuf> {
    let aside = path.with_file_name(ASIDE_NAME);
    std::fs::rename(path, &aside).ok().map(|()| aside)
}

/// Deletes the save at `path`, if there is one.
pub fn forget(path: &Path) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        warn!("save: could not delete {}: {error}", path.display());
    }
}

/// `bytes` as lowercase hex.
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What [`to_hex`] wrote; `None` for anything else.
fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect()
}

/// The items of `record`, in the order [`walk`] numbers them: it, then what is in it, depth first.
fn count_nodes(record: &ItemRecord) -> usize {
    1 + record.contents.iter().map(|s| count_nodes(&s.item)).sum::<usize>()
}

/// Numbers the item `record` describes, which the inventory keeps as `id`, and everything in it:
/// the item first, then each thing it holds in the order recorded, depth first. `None` stands for
/// an item that is in the record but not in the inventory (one that could not be restored), so
/// the numbering is the record's whichever side it is read on. Containers are matched to their
/// contents by the cell they lie in.
pub fn walk(inventory: &Inventory, id: Option<ItemId>, record: &ItemRecord, out: &mut Vec<Option<ItemId>>) {
    out.push(id);
    for stowed in &record.contents {
        let child = id.and_then(|id| inventory.grid(id)).and_then(|grid| grid.entries().iter().find(|e| e.at.x == stowed.x && e.at.y == stowed.y)).map(|e| e.id);
        if child.is_none() {
            // Everything under a missing item is missing too.
            out.extend(std::iter::repeat_n(None, count_nodes(&stowed.item)));
        } else {
            walk(inventory, child, &stowed.item, out);
        }
    }
}

/// Everything about the delve that is not in the world record, from the live game; the world
/// record is `record` (the current level just recorded). `bed` is the set camp they lie in.
pub fn gather(world: &mut World, bed: Entity) -> Result<SavedDelve, SaveError> {
    world.get::<SetCamp>(bed).ok_or(SaveError::Missing("set camp"))?;
    let camp = world.get::<FloorItem>(bed).map(|f| f.spot).ok_or(SaveError::Missing("camp on the floor"))?;
    // The current level is recorded as if just left. The record the game goes on with is the one
    // it had: leaving the level writes it again.
    let before = world.resource::<WorldRecord>().clone();
    let recorded = world.run_system_once(record_level_left);
    let record = std::mem::replace(&mut *world.resource_mut::<WorldRecord>(), before);
    recorded.map_err(|e| SaveError::Unreadable(format!("could not record the level: {e}")))?;
    let abyss = *world.resource::<Abyss>();
    let delver = SavedDelver::from(world.resource::<Character>());
    let pack = {
        let inventory = world.resource::<Inventory>();
        let mut worn = Vec::new();
        let mut walked: Vec<Option<ItemId>> = Vec::new();
        for (slot, id) in inventory.worn_ids() {
            if let Some(item) = record_item(inventory, id) {
                walk(inventory, Some(id), &item, &mut walked);
                worn.push((slot, item));
            }
        }
        let slots = world.resource::<Quickslots>().0;
        let quickslots = slots.map(|bound| bound.and_then(|id| walked.iter().position(|w| *w == Some(id))).map(|n| n as u32));
        let coins = inventory.coins();
        SavedPack { worn, coins: [coins.gp, coins.sp, coins.cp], quickslots }
    };
    let body = {
        let mut players = world.query_filtered::<(&Transform, &Locomotion, Option<&Health>, &DelverNeeds, &DelverStamina, &DelverMana, &Exhaustion, &ExhaustionTimes, &DelverRecovery, Option<&DelverEffects>), With<Player>>();
        let (at, locomotion, health, needs, stamina, mana, exhaustion, times, recovery, effects) = players.single(world).map_err(|_| SaveError::Missing("delver"))?;
        let health = health.ok_or(SaveError::Missing("hit points"))?;
        SavedBody {
            at: at.translation.to_array(),
            yaw: locomotion.yaw,
            pitch: locomotion.pitch,
            hit_points: health.hp,
            max_hit_points: health.max,
            needs: needs.0,
            stamina: stamina.0,
            mana: mana.0,
            exhaustion: exhaustion.0,
            exhaustion_times: times.0,
            recovery: recovery.0,
            effects: effects.map(|e| e.0.clone()),
        }
    };
    let mut explored: Vec<SavedExplored> =
        world.resource::<ExploredLevels>().0.iter().map(|(&level_seed, seen)| SavedExplored { level_seed, width: seen.width, height: seen.height, bits: to_hex(&seen.to_bits()) }).collect();
    explored.sort_by_key(|e| e.level_seed);
    Ok(SavedDelve {
        version: VERSION,
        run_seed: abyss.run_seed,
        depth: abyss.depth,
        delver,
        record,
        pack,
        body,
        clock: world.resource::<GameClock>().time,
        revives: world.resource::<Revives>().0,
        enchanting: world.resource::<Enchanting>().0.clone(),
        explored,
        camp,
    })
}

/// A command: saves the delve now that the delver has lain down in `bed`, if this delve may
/// save, and says so in the toast (a failed write says that, and sleep goes on).
pub fn save_in_camp(bed: Entity) -> impl FnOnce(&mut World) {
    move |world: &mut World| {
        if !may_touch(world.get_resource::<Saveable>()) {
            return;
        }
        let Some(path) = world.get_resource::<SaveFile>().and_then(|f| f.0.clone()) else { return };
        let saved = gather(world, bed).and_then(|delve| write(&path, &delve));
        let now = world.get_resource::<Time<Real>>().map_or(0.0, |t| t.elapsed_secs());
        let line = match saved {
            Ok(()) => {
                info!("save: the delve is saved to {}", path.display());
                "Your delve is saved."
            }
            Err(error) => {
                warn!("save: could not save the delve to {}: {error}", path.display());
                "Your delve could not be saved."
            }
        };
        if let Some(mut toast) = world.get_resource_mut::<crate::inventory::Toast>() {
            toast.say(line, now);
        }
    }
}

/// What the title finds on disk: the saved delve it may resume, and what it has to say about a
/// save it could not read.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct SavedGame {
    pub delve: Option<Box<SavedDelve>>,
    /// One line for the title when the save was unreadable.
    pub unreadable: Option<&'static str>,
}

/// The title's line when an unreadable save was renamed aside.
pub const SET_ASIDE_LINE: &str = "Your saved delve could not be read, and was set aside.";

/// The title's line when an unreadable save could not even be moved.
pub const UNREADABLE_LINE: &str = "Your saved delve could not be read.";

impl SavedGame {
    /// What is saved at `path`: the delve, or nothing for no file, or a line about a file that is
    /// not a delve, which is renamed aside ([`set_aside`]) so it is not met again. Never panics.
    pub fn find(path: &Path) -> SavedGame {
        match read(path) {
            Ok(delve) => SavedGame { delve: Some(Box::new(delve)), unreadable: None },
            Err(SaveError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => SavedGame::default(),
            Err(error) => {
                warn!("save: {} cannot be read ({error})", path.display());
                let aside = set_aside(path);
                SavedGame { delve: None, unreadable: Some(if aside.is_some() { SET_ASIDE_LINE } else { UNREADABLE_LINE }) }
            }
        }
    }
}

/// Looks for a saved delve each time the title shows.
fn find_saved_game(file: Res<SaveFile>, mut found: ResMut<SavedGame>) {
    *found = file.0.as_deref().map(SavedGame::find).unwrap_or_default();
}

/// A command: the player is beginning a delve of their own from the title (the creator, or
/// Continue as). It may save from now on, and it replaces any saved delve, which the title asked
/// about first.
pub fn begin_players_delve(world: &mut World) {
    world.insert_resource(Saveable(true));
    if let Some(path) = world.get_resource::<SaveFile>().and_then(|f| f.0.clone()) {
        forget(&path);
    }
    world.insert_resource(SavedGame::default());
}

/// The saved delve a delve being begun resumes: the title puts it here ([`begin_resume`]) and the
/// first frame of the delve takes it ([`restore_delve`]).
#[derive(Resource, Clone, Debug)]
pub struct ResumeDelve(pub Box<SavedDelve>);

/// Begins the delve `saved` describes: its depth laid out from the seed and the record as travel
/// lays one out, its delver chosen, and the delve entered. The rest is put back on the delve's
/// first frame.
pub fn begin_resume(commands: &mut Commands, next: &mut NextState<AppState>, saved: SavedDelve) {
    let mut level = Abyss::new(saved.run_seed).level_at(saved.depth);
    saved.record.apply_to(&mut level);
    info!("save: {} resumes on depth {}", saved.delver.name, saved.depth);
    commands.insert_resource(CurrentLevel(level));
    commands.insert_resource(ChosenDelver(Character::from(saved.delver.clone())));
    commands.insert_resource(Saveable(true));
    commands.insert_resource(ResumeDelve(Box::new(saved)));
    next.set(AppState::Delving);
}

/// Hit points the first frame of a resumed delve puts back after the delver's maximum has been
/// fitted to their Constitution, which would otherwise count a worn bonus twice.
#[derive(Resource, Clone, Copy, Debug)]
struct SettleHitPoints(Health);

/// Puts back everything a [`SavedDelve`] holds that [`begin_resume`] did not, on the first frame
/// of the delve and before any system that plays runs ([`ResumeSet`]).
pub fn restore_delve(world: &mut World) {
    let Some(ResumeDelve(saved)) = world.remove_resource::<ResumeDelve>() else { return };
    let saved = *saved;
    // The world and where in it.
    *world.resource_mut::<Abyss>() = Abyss { run_seed: saved.run_seed, depth: saved.depth, arrival: Arrival::FromBelow };
    *world.resource_mut::<WorldRecord>() = saved.record;
    world.resource_mut::<Enchanting>().0 = saved.enchanting;
    world.resource_mut::<GameClock>().jump_to(saved.clock);
    world.resource_mut::<Revives>().0 = saved.revives;
    let mut explored = world.resource_mut::<ExploredLevels>();
    explored.0.clear();
    for seen in &saved.explored {
        match from_hex(&seen.bits).and_then(|bits| fb_level::sight::Explored::from_bits(seen.width, seen.height, &bits)) {
            Some(cells) => {
                explored.0.insert(seen.level_seed, cells);
            }
            None => warn!("save: what the map saw of level {} is unreadable; it is forgotten", seen.level_seed),
        }
    }
    // The delver and what they carry.
    *world.resource_mut::<Character>() = Character::from(saved.delver);
    let mut inventory = Inventory::default();
    inventory.set_coins(Coins { gp: saved.pack.coins[0], sp: saved.pack.coins[1], cp: saved.pack.coins[2] });
    let mut walked: Vec<Option<ItemId>> = Vec::new();
    for (slot, record) in &saved.pack.worn {
        let Some(id) = restore_item(&mut inventory, record) else {
            walked.extend(std::iter::repeat_n(None, count_nodes(record)));
            continue;
        };
        if inventory.wear(id, *slot).is_err() {
            warn!("save: {} would not go back on at {slot:?}; it is dropped", record.name);
            inventory.forget(id);
            walked.extend(std::iter::repeat_n(None, count_nodes(record)));
            continue;
        }
        walk(&inventory, Some(id), record, &mut walked);
    }
    let mut slots = Quickslots::default();
    for (slot, at) in saved.pack.quickslots.iter().enumerate() {
        slots.0[slot] = at.and_then(|n| walked.get(n as usize).copied().flatten());
    }
    let worn = world.resource::<Character>().gear(&inventory).abilities;
    world.resource_mut::<Character>().worn = worn;
    *world.resource_mut::<Inventory>() = inventory;
    *world.resource_mut::<Quickslots>() = slots;
    // The body.
    let body = saved.body;
    let health = Health { hp: body.hit_points, max: body.max_hit_points };
    let mut players = world.query_filtered::<Entity, With<Player>>();
    if let Ok(player) = players.single(world) {
        let mut player = world.entity_mut(player);
        player.insert((
            Transform::from_translation(Vec3::from_array(body.at)).with_rotation(Quat::from_rotation_y(body.yaw)),
            health,
            DelverNeeds(body.needs),
            DelverStamina(body.stamina),
            DelverMana(body.mana),
            Exhaustion(body.exhaustion),
            ExhaustionTimes(body.exhaustion_times),
            DelverRecovery(body.recovery),
        ));
        match body.effects {
            Some(effects) => {
                player.insert(DelverEffects(effects));
            }
            None => {
                player.remove::<DelverEffects>();
            }
        }
        if let Some(mut locomotion) = player.get_mut::<Locomotion>() {
            *locomotion = Locomotion { yaw: body.yaw, pitch: body.pitch, velocity: Vec2::ZERO };
        }
    }
    world.insert_resource(SettleHitPoints(health));
    world.insert_resource(PendingBed { spot: saved.camp, waited: 0 });
}

/// Puts the delver's hit points back once the fitting of their maximum to their Constitution has
/// had its look at the restored character.
fn settle_hit_points(mut commands: Commands, settle: Res<SettleHitPoints>, mut players: Query<&mut Health, With<Player>>) {
    for mut health in &mut players {
        *health = settle.0;
    }
    commands.remove_resource::<SettleHitPoints>();
}

/// Deletes the save when the delver falls for good ([`Fallen`] is inserted), so death cannot be
/// undone by resuming. Only a delve that may touch the save ([`Saveable`]) does.
fn forget_save_on_death(saveable: Option<Res<Saveable>>, file: Option<Res<SaveFile>>) {
    if !may_touch(saveable.as_deref()) {
        return;
    }
    if let Some(path) = file.and_then(|f| f.0.clone()) {
        forget(&path);
    }
}

/// A delve no longer may touch the save once it is over.
fn end_saveable(mut saveable: ResMut<Saveable>) {
    saveable.0 = false;
}

/// First on the first frame of a resumed delve, ahead of everything that plays.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResumeSet;

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<SaveFile>() {
            app.insert_resource(SaveFile(SaveFile::default_path()));
        }
        app.init_resource::<Saveable>()
            .init_resource::<SavedGame>()
            .add_systems(OnEnter(AppState::Title), find_saved_game)
            .configure_sets(Update, ResumeSet.before(DelveSystems).run_if(in_state(AppState::Delving)).run_if(resource_exists::<ResumeDelve>))
            .add_systems(Update, restore_delve.in_set(ResumeSet))
            .add_systems(Update, settle_hit_points.after(crate::combat::vitals::fit_hit_points).run_if(resource_exists::<SettleHitPoints>).in_set(DelveSystems))
            .add_systems(Update, forget_save_on_death.run_if(resource_added::<Fallen>).in_set(DelveSystems))
            .add_systems(OnExit(AppState::Delving), end_saveable);
    }
}

/// A delve to test with, on depth 3 of the world of 1207.
#[cfg(test)]
pub(crate) fn sample_delve() -> SavedDelve {
    let character = Character::roll(5);
    let bed = fb_items::roll_item("Tent", 9).unwrap();
    let sack = ItemRecord {
        contents: vec![Stowed { x: 1, y: 0, turned: true, item: item_record(&fb_items::roll_item("Dagger", 4).unwrap()) }],
        ..item_record(&fb_items::roll_item("Sack", 3).unwrap())
    };
    SavedDelve {
        version: VERSION,
        run_seed: 1207,
        depth: 3,
        delver: SavedDelver::from(&character),
        record: WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, 1207),
        pack: SavedPack { worn: vec![(fb_items::Slot::Belt, item_record(&fb_items::roll_item("Belt", 3).unwrap())), (fb_items::Slot::Back, sack)], coins: [1, 2, 3], quickslots: [None; SLOTS] },
        body: SavedBody {
            at: [12.5, 0.0, 33.25],
            yaw: 1.5,
            pitch: -0.25,
            hit_points: 7,
            max_hit_points: 11,
            needs: default(),
            stamina: default(),
            mana: default(),
            exhaustion: default(),
            exhaustion_times: default(),
            recovery: default(),
            effects: None,
        },
        clock: GameTime::from_seconds(123_456.789),
        revives: 1,
        enchanting: EnchantingStock { essence: 4, materials: [("iron".to_string(), 2)].into() },
        explored: vec![SavedExplored { level_seed: 7, width: 8, height: 8, bits: to_hex(&[0xff, 0x00, 0x0f, 0, 0, 0, 0, 1]) }],
        camp: fb_level::ItemSpot { x: 4.5, y: 9.25, yaw: 0.5, family: bed.shape.family(), seed: bed.seed },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("fb-save-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder.join(FILE_NAME)
    }

    fn sample() -> SavedDelve {
        sample_delve()
    }

    #[test]
    fn a_saved_delve_reads_back_equal() {
        let path = scratch("round");
        let delve = sample();
        write(&path, &delve).unwrap();
        assert_eq!(read(&path).unwrap(), delve);
        // Writing again replaces it, and no temporary file is left.
        let mut again = sample();
        again.depth = 4;
        write(&path, &again).unwrap();
        assert_eq!(read(&path).unwrap().depth, 4);
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from(FILE_NAME)]);
    }

    #[test]
    fn the_version_is_written_first() {
        let path = scratch("version");
        write(&path, &sample()).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("{\"version\":1,"));
    }

    #[test]
    fn a_truncated_garbled_empty_or_foreign_file_is_an_error_and_never_a_panic() {
        let path = scratch("bad");
        write(&path, &sample()).unwrap();
        let whole = std::fs::read(&path).unwrap();
        for (what, bytes) in [
            ("truncated", whole[..whole.len() / 2].to_vec()),
            ("cut at the first byte", whole[..1].to_vec()),
            ("empty", vec![]),
            ("garbled", {
                let mut garbled = whole.clone();
                garbled.iter_mut().skip(40).step_by(7).for_each(|b| *b = b'#');
                garbled
            }),
            ("not text", vec![0xff, 0xfe, 0x00, 0x9f]),
            ("another document", br#"{"hello":"world"}"#.to_vec()),
            ("no depth", String::from_utf8(whole.clone()).unwrap().replacen("\"depth\":3", "\"depth\":0", 1).into_bytes()),
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert!(matches!(read(&path), Err(SaveError::Unreadable(_))), "{what}");
        }
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(read(&path), Err(SaveError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound));
    }

    #[test]
    fn a_save_of_another_version_is_refused_not_guessed_at() {
        let path = scratch("other");
        write(&path, &sample()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap().replacen("\"version\":1", "\"version\":2", 1);
        std::fs::write(&path, text).unwrap();
        assert!(matches!(read(&path), Err(SaveError::Version(2))));
    }

    #[test]
    fn an_unreadable_save_is_set_aside_beside_itself() {
        let path = scratch("aside");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not a delve").unwrap();
        let aside = set_aside(&path).expect("it is moved");
        assert_eq!(aside.file_name().unwrap(), ASIDE_NAME);
        assert!(!path.exists());
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), "{ not a delve");
        // A later one replaces it.
        std::fs::write(&path, "again").unwrap();
        assert!(set_aside(&path).is_some());
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), "again");
        assert_eq!(set_aside(&path), None, "nothing left to move");
    }

    #[test]
    fn forgetting_a_save_that_is_not_there_is_fine() {
        let path = scratch("forget");
        forget(&path);
        write(&path, &sample()).unwrap();
        forget(&path);
        assert!(!path.exists());
    }

    #[test]
    fn hex_comes_back_and_nonsense_does_not() {
        assert_eq!(from_hex(&to_hex(&[0, 1, 0xab, 0xff])), Some(vec![0, 1, 0xab, 0xff]));
        assert_eq!(from_hex("abc"), None);
        assert_eq!(from_hex("zz"), None);
        assert_eq!(from_hex("é1"), None);
    }

    /// A sack with a dagger and a smaller sack in it, in an inventory.
    fn bags() -> (Inventory, ItemId, ItemRecord, ItemId, ItemId) {
        let mut inventory = Inventory::default();
        let outer = inventory.add(fb_items::roll_item("Sack", 3).unwrap());
        let inner = inventory.add(fb_items::roll_item("Sack", 4).unwrap());
        let dagger = inventory.add(fb_items::roll_item("Dagger", 5).unwrap());
        let at = inventory.grid(outer).unwrap().first_free(inventory.footprint(inner).unwrap()).unwrap();
        inventory.put(inner, outer, at).unwrap();
        let at = inventory.grid(inner).unwrap().first_free(inventory.footprint(dagger).unwrap()).unwrap();
        inventory.put(dagger, inner, at).unwrap();
        let record = record_item(&inventory, outer).unwrap();
        (inventory, outer, record, inner, dagger)
    }

    #[test]
    fn a_pack_is_numbered_the_same_whichever_side_reads_it() {
        let (inventory, outer, record, inner, dagger) = bags();
        let mut walked = Vec::new();
        walk(&inventory, Some(outer), &record, &mut walked);
        assert_eq!(walked, vec![Some(outer), Some(inner), Some(dagger)]);
        // Rebuilt, the new ids are different and the numbering is not.
        let mut other = Inventory::default();
        other.add(fb_items::roll_item("Dagger", 1).unwrap());
        let rebuilt = crate::record::restore_item(&mut other, &record).unwrap();
        let mut again = Vec::new();
        walk(&other, Some(rebuilt), &record, &mut again);
        assert_eq!(again.len(), 3);
        assert!(again.iter().all(Option::is_some));
        assert_ne!(again[0], walked[0]);
        let names: Vec<_> = again.iter().map(|id| other.item(id.unwrap()).unwrap().base.clone()).collect();
        assert_eq!(names, ["Sack", "Sack", "Dagger"]);
        // What could not be put back leaves its place in the numbering.
        let mut missing = Vec::new();
        walk(&other, None, &record, &mut missing);
        assert_eq!(missing, vec![None, None, None]);
        assert_eq!(count_nodes(&record), 3);
    }

    #[test]
    fn a_save_goes_beside_the_settings_file() {
        if std::env::var_os("FB_SAVE_FILE").is_none() {
            assert_eq!(SaveFile::default_path(), SettingsFile::default_path().map(|s| s.with_file_name(FILE_NAME)));
        }
    }

    #[test]
    fn only_a_delve_begun_from_the_title_or_a_capture_that_names_a_file_may_touch_the_save() {
        assert!(may_touch(Some(&Saveable(true))));
        if !crate::launch::env_set("FB_SAVE_FILE") {
            assert!(!may_touch(None));
            assert!(!may_touch(Some(&Saveable(false))));
        }
    }

    #[test]
    fn death_deletes_the_save_of_a_delve_that_may_touch_it_and_no_other() {
        for (saveable, gone) in [(true, true), (false, false)] {
            let path = scratch(&format!("death-{saveable}"));
            write(&path, &sample()).unwrap();
            let mut app = App::new();
            app.insert_resource(SaveFile(Some(path.clone()))).insert_resource(Saveable(saveable)).add_systems(Update, forget_save_on_death.run_if(resource_added::<Fallen>));
            app.update();
            assert!(path.exists(), "nothing has fallen");
            app.insert_resource(Fallen(1.0));
            app.update();
            assert_eq!(!path.exists(), gone, "saveable {saveable}");
        }
    }

    #[test]
    fn a_save_is_found_for_the_title_or_set_aside_with_one_line() {
        let path = scratch("find");
        assert_eq!(SavedGame::find(&path), SavedGame::default(), "no file, no save, no line");
        write(&path, &sample()).unwrap();
        let found = SavedGame::find(&path);
        assert_eq!(found.delve.as_deref(), Some(&sample()));
        assert_eq!(found.unreadable, None);
        std::fs::write(&path, "{ \"version\": 1, \"depth\"").unwrap();
        let found = SavedGame::find(&path);
        assert_eq!((found.delve, found.unreadable), (None, Some(SET_ASIDE_LINE)));
        assert!(!path.exists() && path.with_file_name(ASIDE_NAME).exists());
        assert_eq!(SavedGame::find(&path), SavedGame::default(), "and it is not met again");
    }
}
