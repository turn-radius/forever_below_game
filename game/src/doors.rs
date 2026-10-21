//! Doors at the edge of a faction's ground, locked by how the faction stands with the delver (#553).
//!
//! Every door cell on the edge of a claim ([`fb_level::Level::claim_doors`]) is hung with a
//! leaf (`crate::level`) that swings: shut, it stops the delver as rock does, open it does not.
//! F opens and closes it. Whether it is locked is read live from the band the delver stands in
//! with the claim's faction ([`locked_to`]), as the table in `docs/design.md` says:
//!
//! | Band | The claim's main door (its hall's) | Its other doors |
//! |---|---|---|
//! | Hostile, Wary | locked | locked |
//! | Neutral | open | locked |
//! | Friendly | open | open |
//! | Allied | open, and the faction gives the delver its key once | open |
//!
//! "Open" here is unlocked: the leaf is still shut until F swings it. A locked door is opened
//! with a carried key of the faction (G, "Unlock with key"), or picked with lockpicks (R, held
//! for [`PICK_SECONDS`]): a roll of d20 plus the Dexterity modifier, the lockpicking of worn
//! gear and [`LOCKPICKS_BONUS`] for the picks, against a difficulty that rises as the faction
//! likes the delver less ([`pick_dc`]). A miss is noise, which carries through a shut door: a
//! folk of the faction within [`NOISE_RANGE`] hears it and the faction's standing drops by
//! [`NOISE_COST`]; unheard, a miss costs only the hold. A door unlocked by key or picks stays unlocked.
//!
//! A shut door is a wall, and opaque to sight, for everyone but the folk of its own faction
//! ([`solid_for`], read by the delver's walk, folk routines and combat, creatures, and every
//! line of sight). The leaf swings open for a folk of the faction who walks up to it.
//!
//! All the numbers (the difficulties, the pick time, the noise) are our own. Each door's state,
//! open or shut and unlocked or not, goes into the level's record (`crate::record`).

use std::collections::BTreeMap;

use bevy::prelude::*;
use fb_faction::{Band, Faction, FactionSpec};
use fb_items::Item;
use fb_level::{CELL, ClaimDoor};
use procgen_rng::Mulberry32;

use crate::carry::{Aimed, REACH};
use crate::character::{Character, modifier};
use crate::combat::RunDice;
use crate::combat::feedback::Corpse;
use crate::combat::vitals::{Dying, Fallen};
use crate::folk::{AimedFolk, Denizen};
use crate::interact::{CrosshairPrompt, Delver, Lock, Target, key_line, verbs_for};
use crate::inventory::{Inventory, ItemId, Toasts};
use crate::keymap::{Action, ActionInput, Keymap};
use crate::level::geometry::{self, DOOR_HEIGHT, DOOR_WIDTH, Doorway};
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Player, PlayerCamera, level_has_focus};
use crate::player::blocked_by;
use crate::record::{DoorRecord, LevelRecord, WorldRecord};

/// Seconds a door leaf takes to swing open or shut.
pub const SWING_SECONDS: f32 = 0.6;
/// Seconds the pick is held before the roll.
pub const PICK_SECONDS: f32 = 1.5;
/// What lockpicks add to the roll.
pub const LOCKPICKS_BONUS: i32 = 1;
/// The roll a Neutral faction's locked door asks for.
pub const DC_NEUTRAL: i32 = 12;
/// The roll a Wary faction's door asks for.
pub const DC_WARY: i32 = 15;
/// The roll a Hostile faction's door asks for.
pub const DC_HOSTILE: i32 = 18;
/// How far a missed pick is heard by a folk of the faction, world units (8 cells).
pub const NOISE_RANGE: f32 = 8.0 * CELL;
/// What standing a heard miss costs.
pub const NOISE_COST: i32 = 6;
/// How near a folk of the faction swings its door open for itself, world units.
pub const FOLK_OPENS: f32 = 1.6 * CELL;
/// How far past the door's own cell a body still stops it shutting, world units (a body's radius).
const IN_THE_WAY: f32 = 4.0;
/// How far past the door's opening a look still counts as aimed at it, world units.
const AIM_SLACK: f32 = 1.0;

pub struct DoorsPlugin;

impl Plugin for DoorsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Doors>().init_resource::<AimedDoor>().add_systems(
            Update,
            (aim_door, prompt_door.after(crate::travel::prompt_stair), use_door.run_if(level_has_focus), open_for_capture, swing_leaves, give_keys.run_if(level_has_focus)).chain().in_set(DelveSystems),
        );
    }
}

/// Whether a claim's door is locked to a delver who stands in `band` with its faction. `main`
/// is the claim's main door.
pub fn locked_to(band: Band, main: bool) -> bool {
    match band {
        Band::Hostile | Band::Wary => true,
        Band::Neutral => !main,
        Band::Friendly | Band::Allied => false,
    }
}

/// The roll that picks a lock on a door of a faction in `band`.
pub fn pick_dc(band: Band) -> i32 {
    match band {
        Band::Hostile => DC_HOSTILE,
        Band::Wary => DC_WARY,
        _ => DC_NEUTRAL,
    }
}

/// Whether a pick succeeds: the die, the Dexterity modifier of the delver's score, the
/// lockpicking of what they wear, and the picks, against `dc`.
pub fn picks(d20: u32, dexterity: i32, worn_lockpicking: i32, dc: i32) -> bool {
    d20 as i32 + modifier(dexterity) + worn_lockpicking + LOCKPICKS_BONUS >= dc
}

/// One claim door as it stands.
#[derive(Clone, Debug, PartialEq)]
pub struct DoorState {
    pub faction: Faction,
    /// The claim's main door.
    pub main: bool,
    /// Whether the delver has opened it.
    pub open: bool,
    /// Whether the delver has unlocked it for good, by key or picks.
    pub unlocked: bool,
    /// How far the leaf has swung, 0 shut to 1 open.
    pub swing: f32,
    /// The doorway's centre on the floor and the way through it (world `(x, z)`).
    pub centre: Vec2,
    pub through: Vec2,
}

impl DoorState {
    /// Whether it is locked to a delver in `band` with its faction.
    pub fn locked(&self, band: Band) -> bool {
        !self.unlocked && locked_to(band, self.main)
    }
}

/// The claim doors of the level, by cell.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Doors(pub BTreeMap<(i32, i32), DoorState>);

impl Doors {
    /// Hangs the door `door` in `doorway`, as the level's `saved` record left it.
    pub fn add(&mut self, door: &ClaimDoor, doorway: &Doorway, saved: Option<&LevelRecord>) {
        let mut state = DoorState {
            faction: door.faction,
            main: door.main,
            open: false,
            unlocked: false,
            swing: 0.0,
            centre: Vec2::new(door.x as f32 + 0.5, door.y as f32 + 0.5) * CELL,
            through: Vec2::new(doorway.through[0], doorway.through[2]),
        };
        if let Some(record) = saved.and_then(|l| l.doors.iter().find(|d| (d.x, d.y) == (door.x, door.y))) {
            state.open = record.open;
            state.unlocked = record.unlocked;
            state.swing = f32::from(record.open);
        }
        self.0.insert((door.x, door.y), state);
    }

    /// Whether cell `(x, y)` holds a shut door, which stops the delver.
    pub fn shuts(&self, x: i32, y: i32) -> bool {
        self.0.get(&(x, y)).is_some_and(|d| !d.open)
    }

    /// Whether cell `(x, y)` holds a shut door that stops someone of `faction` (`None` for the
    /// delver and creatures): a shut door stops everyone but the folk of its own faction.
    pub fn stops(&self, x: i32, y: i32, faction: Option<Faction>) -> bool {
        self.0.get(&(x, y)).is_some_and(|d| !d.open && Some(d.faction) != faction)
    }

    /// What the record keeps of every door.
    pub fn records(&self) -> Vec<DoorRecord> {
        self.0.iter().map(|(&(x, y), d)| DoorRecord { x, y, open: d.open, unlocked: d.unlocked }).collect()
    }
}

/// Whether cell `(x, y)` stops someone of `faction` (`None` for creatures): rock, a blocked
/// cell, or a shut door that is not their own faction's. One test for walking, pathing and
/// sight, so a shut door is a wall to everyone but the door's own folk and opaque to them too.
pub fn solid_for<'a>(level: &'a fb_level::Level, cells: &'a BlockedCells, doors: Option<&'a Doors>, faction: Option<Faction>) -> impl Fn(i32, i32) -> bool + 'a {
    let rock = blocked_by(level, cells);
    move |x, y| rock(x, y) || doors.is_some_and(|d| d.stops(x, y, faction))
}

/// The leaf of a claim door, hung at its hinge.
#[derive(Component, Clone, Copy, Debug)]
pub struct DoorLeaf {
    pub cell: (i32, i32),
    /// The leaf's rotation when shut; the swing turns it about its hinge from there.
    pub shut: Quat,
}

impl DoorLeaf {
    /// The leaf's rotation with the door `swing` open (0 shut, 1 open).
    pub fn rotation(&self, swing: f32) -> Quat {
        self.shut * Quat::from_rotation_y(geometry::entrance_swing(1.0 - swing))
    }
}

/// The door under the crosshair and in reach, if any, by cell.
#[derive(Resource, Debug, Default, PartialEq, Eq)]
pub struct AimedDoor(pub Option<(i32, i32)>);

/// The door a look from `eye` along `forward` falls on within [`REACH`]: the nearest whose
/// opening the ray crosses.
pub fn door_in_reach(doors: &Doors, eye: Vec3, forward: Vec3) -> Option<(i32, i32)> {
    let mut best: Option<(f32, (i32, i32))> = None;
    for (&cell, door) in &doors.0 {
        let normal = Vec3::new(door.through.x, 0.0, door.through.y);
        let across = Vec3::new(door.through.y, 0.0, -door.through.x);
        let centre = Vec3::new(door.centre.x, 0.0, door.centre.y);
        let slant = forward.dot(normal);
        if slant.abs() < 1e-3 {
            continue;
        }
        let t = (centre - eye).dot(normal) / slant;
        if t <= 0.0 || t > REACH || best.is_some_and(|(b, _)| b <= t) {
            continue;
        }
        let at = eye + forward * t;
        if (at - centre).dot(across).abs() <= DOOR_WIDTH * 0.5 + AIM_SLACK && (0.0..=DOOR_HEIGHT + AIM_SLACK).contains(&at.y) {
            best = Some((t, cell));
        }
    }
    best.map(|(_, cell)| cell)
}

/// The seed of the key of `faction`, so the same key is the same item wherever it is rolled.
pub fn key_seed(faction: Faction) -> u32 {
    0x4B45_5900 | u32::from(faction.0)
}

/// The key `faction` gives an Allied delver: a ring of keys named for it.
pub fn key_of(faction: &FactionSpec) -> Option<Item> {
    let mut key = fb_items::roll_item(fb_items::armory_base::KEY, key_seed(faction.id))?;
    key.name = format!("Key of {}", faction.name);
    Some(key)
}

/// Whether `item` is the key of `faction`.
pub fn is_key_of(item: &Item, faction: Faction) -> bool {
    item.base == fb_items::armory_base::KEY && item.seed == key_seed(faction)
}

/// The things the delver carries, worn or in a bag they wear.
fn carried<'a>(inventory: &'a Inventory) -> impl Iterator<Item = (ItemId, &'a Item)> + 'a {
    inventory.ids().filter(|&id| inventory.is_carried(id)).filter_map(|id| Some((id, inventory.item(id)?)))
}

/// Whether the delver carries the key of `faction`.
pub fn carries_key(inventory: &Inventory, faction: Faction) -> bool {
    carried(inventory).any(|(_, item)| is_key_of(item, faction))
}

/// Whether the delver carries lockpicks.
pub fn carries_lockpicks(inventory: &Inventory) -> bool {
    carried(inventory).any(|(_, item)| item.base == "Lockpicks")
}

/// The lockpicking the delver's worn and held gear gives.
pub fn worn_lockpicking(inventory: &Inventory) -> i32 {
    inventory.equipped().iter().map(|(_, item)| item.profile.stats.lockpicking).sum::<f32>().round() as i32
}

/// Hands the delver the key of `faction`, if they do not carry one: into a bag if there is room,
/// else loose underfoot. Returns whether a key was given.
pub fn hand_over_key(inventory: &mut Inventory, faction: &FactionSpec) -> bool {
    if carries_key(inventory, faction.id) {
        return false;
    }
    let Some(key) = key_of(faction) else { return false };
    let id = inventory.add(key);
    let _ = inventory.pick_up(id);
    true
}

/// `name` with a capital: "the Ash Choir" to "The Ash Choir".
fn capital(name: &str) -> String {
    let mut letters = name.chars();
    letters.next().map(|c| c.to_uppercase().chain(letters).collect()).unwrap_or_default()
}

fn band_of(record: Option<&WorldRecord>, faction: Faction) -> Band {
    record.map_or(Band::Neutral, |r| r.standings.band(faction))
}

/// Finds the door under the crosshair, unless an item or a folk is aimed at instead.
#[allow(clippy::too_many_arguments)]
fn aim_door(
    doors: Res<Doors>,
    mut aimed_door: ResMut<AimedDoor>,
    aimed: Option<Res<Aimed>>,
    aimed_folk: Option<Res<AimedFolk>>,
    fallen: Option<Res<Fallen>>,
    dying: Option<Res<Dying>>,
    cameras: Query<&GlobalTransform, With<PlayerCamera>>,
) {
    let busy = aimed.is_some_and(|a| a.0.is_some()) || aimed_folk.is_some_and(|a| a.0.is_some()) || fallen.is_some() || dying.is_some();
    let found = if busy || doors.0.is_empty() { None } else { cameras.iter().next().and_then(|eye| door_in_reach(&doors, eye.translation(), eye.forward().as_vec3())) };
    if aimed_door.0 != found {
        aimed_door.0 = found;
    }
}

/// What the prompt over the crosshair says for a door that is `open` and `locked`, holding
/// `key` and `picks`: "F Open", "F Close", or "Locked \u{b7} G Unlock \u{b7} R Pick the lock".
pub fn door_line(keymap: &Keymap, open: bool, locked: bool, key: bool, picks: bool) -> String {
    let lock = Lock { has_lock: true, locked };
    let delver = Delver { has_key: key && locked && !open, has_lockpicks: picks, hidden: false };
    let mut verbs = verbs_for(&Target::Door { open, lock }, &delver);
    if locked && !open {
        verbs.primary = None;
        let rest = key_line(keymap, &verbs);
        return if rest.is_empty() { "Locked".to_string() } else { format!("Locked \u{b7} {rest}") };
    }
    verbs.secondary = None;
    verbs.tertiary = None;
    key_line(keymap, &verbs)
}

/// Says the aimed door's verbs over the crosshair. Clears only its own line.
#[allow(clippy::too_many_arguments)]
fn prompt_door(
    aimed: Res<AimedDoor>,
    doors: Res<Doors>,
    record: Option<Res<WorldRecord>>,
    inventory: Option<Res<Inventory>>,
    keymap: Option<Res<Keymap>>,
    mut prompt: ResMut<CrosshairPrompt>,
    mut said: Local<Option<String>>,
) {
    let line = aimed.0.and_then(|cell| doors.0.get(&cell)).map(|door| {
        let locked = door.locked(band_of(record.as_deref(), door.faction));
        let (key, picks) = inventory.as_deref().map_or((false, false), |i| (carries_key(i, door.faction), carries_lockpicks(i)));
        let fallback = Keymap::default();
        door_line(keymap.as_deref().unwrap_or(&fallback), door.open, locked, key, picks)
    });
    if *said != line {
        if line.is_some() || prompt.0 == *said {
            prompt.0.clone_from(&line);
        }
        *said = line;
    }
}

/// F opens and closes the aimed door, G unlocks it with a carried key, and R held picks it.
#[allow(clippy::too_many_arguments)]
fn use_door(
    input: ActionInput,
    time: Res<Time>,
    aimed: Res<AimedDoor>,
    level: Res<CurrentLevel>,
    mut doors: ResMut<Doors>,
    inventory: Option<Res<Inventory>>,
    mut record: Option<ResMut<WorldRecord>>,
    character: Option<Res<Character>>,
    dice: Option<ResMut<RunDice>>,
    players: Query<&Transform, With<Player>>,
    folk: Query<(&Transform, &Denizen), Without<Corpse>>,
    mut toasts: Toasts,
    mut picking: Local<Option<((i32, i32), f32)>>,
    mut spare_rolls: Local<u32>,
) {
    let Some(cell) = aimed.0 else {
        *picking = None;
        return;
    };
    let Some(door) = doors.0.get_mut(&cell) else { return };
    let band = band_of(record.as_deref(), door.faction);
    let locked = door.locked(band);
    let (key, lockpicks) = inventory.as_deref().map_or((false, false), |i| (carries_key(i, door.faction), carries_lockpicks(i)));
    let spec = level.0.factions.iter().find(|f| f.id == door.faction);
    let name = spec.map_or_else(|| "they".to_string(), |f| f.name.clone());

    if input.just_pressed(Action::Primary) {
        if door.open {
            // A door is not shut on someone standing in it.
            let feet = players.iter().next().map(|t| t.translation.xz());
            if feet.is_some_and(|f| (f - door.centre).abs().max_element() < CELL * 0.5 + IN_THE_WAY) {
                toasts.say("Something is in the way");
            } else {
                door.open = false;
            }
        } else if locked {
            toasts.say("The door is locked");
        } else {
            door.open = true;
        }
    }
    if input.just_pressed(Action::Secondary) && locked && !door.open && key {
        door.unlocked = true;
        toasts.say(format!("You unlock the door with the key of {name}"));
    }

    // Picking: a press starts it, holding it runs the clock, the roll comes at the end.
    let locked = door.locked(band);
    if !(locked && lockpicks && !door.open && input.pressed(Action::Tertiary)) {
        *picking = None;
        return;
    }
    let held = match *picking {
        Some((at, held)) if at == cell => held,
        Some(_) => 0.0,
        None if input.just_pressed(Action::Tertiary) => 0.0,
        None => return,
    };
    let held = held + time.delta_secs();
    if held < PICK_SECONDS {
        *picking = Some((cell, held));
        toasts.say_once("Picking the lock...");
        return;
    }
    *picking = None;
    // Without the run's dice (a bare app), a counter of its own keeps each roll different.
    *spare_rolls += 1;
    let mut fallback = Mulberry32::new(*spare_rolls ^ cell.0 as u32 ^ (cell.1 as u32) << 8);
    let mut dice = dice;
    let roll = match dice.as_mut() {
        Some(dice) => dice.0.int(1, 20),
        None => fallback.int(1, 20),
    };
    let dexterity = character.as_deref().map_or(10, Character::dexterity);
    let worn = inventory.as_deref().map_or(0, worn_lockpicking);
    if picks(roll, dexterity, worn, pick_dc(band)) {
        door.unlocked = true;
        toasts.say("You pick the lock");
        return;
    }
    // A miss is noise, and sound carries through a shut door: a folk of the faction within
    // `NOISE_RANGE` hears it, whatever lies between.
    let faction = door.faction;
    let at = door.centre;
    let heard = folk.iter().any(|(transform, denizen)| denizen.person.faction.id == faction && transform.translation.xz().distance(at) <= NOISE_RANGE);
    if heard {
        if let Some(record) = record.as_deref_mut() {
            record.standings.shift(faction, -NOISE_COST);
        }
        toasts.say(format!("{} heard the picks", capital(&name)));
    } else {
        toasts.say("The picks slip");
    }
}

/// For captures (`FB_DOORS=open`): a few frames in, opens every door that is not locked to the
/// delver as they stand with its faction.
fn open_for_capture(mut doors: ResMut<Doors>, record: Option<Res<WorldRecord>>, mut frames: Local<u32>) {
    if std::env::var("FB_DOORS").as_deref() != Ok("open") || doors.0.is_empty() {
        return;
    }
    *frames += 1;
    if *frames != 8 {
        return;
    }
    for door in doors.0.values_mut() {
        door.open = !door.locked(band_of(record.as_deref(), door.faction));
    }
}

/// Swings each leaf toward where its door stands: open if the delver opened it or a folk of the
/// faction is at it.
fn swing_leaves(time: Res<Time>, mut doors: ResMut<Doors>, folk: Query<(&Transform, &Denizen), Without<Corpse>>, mut leaves: Query<(&DoorLeaf, &mut Transform), Without<Denizen>>) {
    let step = time.delta_secs() / SWING_SECONDS;
    for (leaf, mut transform) in &mut leaves {
        let Some(door) = doors.bypass_change_detection().0.get_mut(&leaf.cell) else { continue };
        let folk_at = folk.iter().any(|(t, d)| d.person.faction.id == door.faction && t.translation.xz().distance(door.centre) < FOLK_OPENS);
        let want = f32::from(door.open || folk_at);
        let swing = if door.swing < want { (door.swing + step).min(want) } else { (door.swing - step).max(want) };
        if swing != door.swing {
            door.swing = swing;
            transform.rotation = leaf.rotation(swing);
        }
    }
}

/// An Allied faction's key goes to the delver, once.
fn give_keys(level: Res<CurrentLevel>, record: Option<ResMut<WorldRecord>>, inventory: Option<ResMut<Inventory>>, mut toasts: Toasts) {
    let (Some(mut record), Some(mut inventory)) = (record, inventory) else { return };
    for spec in &level.0.factions {
        if record.standings.band(spec.id) != Band::Allied || record.keys_given.contains(&spec.id) {
            continue;
        }
        record.keys_given.push(spec.id);
        if hand_over_key(&mut inventory, spec) {
            toasts.add(&format!("{} give you their key", capital(&spec.name)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::slide_around;
    use fb_level::path::line_of_sight;

    /// A level with a claim door whose faction is `Faction`, and that door.
    fn claimed() -> (fb_level::Level, ClaimDoor) {
        (1..60)
            .filter_map(|seed| {
                let level = fb_level::generate(seed, 1);
                let door = level.claim_doors().into_iter().find(|d| geometry::doorways(&geometry::CellView::new(&level)).iter().any(|w| (w.x, w.y) == (d.x, d.y)))?;
                Some((level, door))
            })
            .next()
            .expect("some level has a claim door")
    }

    fn hung(level: &fb_level::Level, door: &ClaimDoor, main: bool) -> Doors {
        let doorway = geometry::doorways(&geometry::CellView::new(level)).into_iter().find(|w| (w.x, w.y) == (door.x, door.y)).unwrap();
        let mut doors = Doors::default();
        doors.add(&ClaimDoor { main, ..*door }, &doorway, None);
        doors
    }

    #[test]
    fn standing_locks_the_doors_by_the_band_table() {
        let table = [
            (Band::Hostile, true, true),
            (Band::Wary, true, true),
            (Band::Neutral, true, false),
            (Band::Friendly, false, false),
            (Band::Allied, false, false),
        ];
        for (band, inner, main) in table {
            assert_eq!(locked_to(band, false), inner, "{band:?} inner door");
            assert_eq!(locked_to(band, true), main, "{band:?} main door");
        }
        // Read (band, inner door, main door): only Neutral tells them apart.
        assert!(locked_to(Band::Hostile, true) && !locked_to(Band::Neutral, true));
    }

    #[test]
    fn a_wary_factions_door_is_locked_to_the_delver_and_a_friendly_ones_is_open() {
        let (level, door) = claimed();
        let doors = hung(&level, &door, false);
        let state = &doors.0[&(door.x, door.y)];
        assert!(state.locked(Band::Wary) && state.locked(Band::Hostile));
        assert!(!state.locked(Band::Friendly) && !state.locked(Band::Allied));
        // Unlocked once, it stays so whatever the band.
        let mut state = state.clone();
        state.unlocked = true;
        assert!(!state.locked(Band::Hostile));
    }

    #[test]
    fn the_prompt_says_locked_and_what_opens_it() {
        let keymap = Keymap::default();
        assert_eq!(door_line(&keymap, false, false, false, false), "F Open");
        assert_eq!(door_line(&keymap, true, false, false, false), "F Close");
        assert_eq!(door_line(&keymap, false, true, false, false), "Locked");
        assert_eq!(door_line(&keymap, false, true, true, false), "Locked \u{b7} G Unlock with key");
        assert_eq!(door_line(&keymap, false, true, false, true), "Locked \u{b7} R Pick the lock");
        assert_eq!(door_line(&keymap, false, true, true, true), "Locked \u{b7} G Unlock with key \u{b7} R Pick the lock");
    }

    #[test]
    fn a_shut_door_stops_the_delver_but_not_the_folk() {
        let (level, door) = claimed();
        let mut doors = hung(&level, &door, false);
        let state = doors.0[&(door.x, door.y)].clone();
        let (from, to) = (state.centre - state.through * CELL, state.centre + state.through * CELL);
        let none = BlockedCells::default();
        // What the folk's routines read: rock and blocked cells, never the doors.
        let folk = slide_around(from, to, 1.5, blocked_by(&level, &none), &[]);
        // What the delver's walk reads as well.
        let shut = |x, y| blocked_by(&level, &none)(x, y) || doors.shuts(x, y);
        let delver = slide_around(from, to, 1.5, shut, &[]);
        assert!(folk.distance(to) < delver.distance(to), "the folk got to {folk}, the delver to {delver}");
        assert!(delver.distance(state.centre) > CELL * 0.4, "the delver stopped at {delver}");
        // Open, it lets the delver through.
        doors.0.get_mut(&(door.x, door.y)).unwrap().open = true;
        let shut = |x, y| blocked_by(&level, &none)(x, y) || doors.shuts(x, y);
        assert!(slide_around(from, to, 1.5, shut, &[]).distance(to) < 1.0);
    }

    #[test]
    fn a_look_at_the_doorway_aims_at_the_door() {
        let (level, door) = claimed();
        let doors = hung(&level, &door, false);
        let state = &doors.0[&(door.x, door.y)];
        let eye = Vec3::new(state.centre.x, 16.0, state.centre.y) - Vec3::new(state.through.x, 0.0, state.through.y) * 12.0;
        let toward = Vec3::new(state.through.x, -0.2, state.through.y).normalize();
        assert_eq!(door_in_reach(&doors, eye, toward), Some((door.x, door.y)));
        assert_eq!(door_in_reach(&doors, eye, -toward), None);
        // Too far to reach.
        assert_eq!(door_in_reach(&doors, eye - Vec3::new(state.through.x, 0.0, state.through.y) * 20.0, toward), None);
    }

    #[test]
    fn a_pick_rolls_against_the_factions_mood() {
        // d20 + Dexterity modifier + picks (+1) against 12, 15 and 18.
        assert!(picks(11, 10, 0, DC_NEUTRAL) && !picks(10, 10, 0, DC_NEUTRAL));
        assert!(picks(14, 10, 0, DC_WARY) && !picks(13, 10, 0, DC_WARY));
        assert!(picks(17, 10, 0, DC_HOSTILE) && !picks(16, 10, 0, DC_HOSTILE));
        // A nimble hand (Dexterity 16, +3) and worn lockpicking +2 need less.
        assert!(picks(12, 16, 2, DC_HOSTILE) && !picks(11, 16, 2, DC_HOSTILE));
        assert!(pick_dc(Band::Hostile) > pick_dc(Band::Wary) && pick_dc(Band::Wary) > pick_dc(Band::Neutral));
    }

    fn spec_of(level: &fb_level::Level, faction: Faction) -> FactionSpec {
        level.factions.iter().find(|f| f.id == faction).unwrap().clone()
    }

    #[test]
    fn a_key_is_the_factions_and_is_given_once() {
        let (level, door) = claimed();
        let spec = spec_of(&level, door.faction);
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.equip(pack, fb_items::Slot::Back).unwrap();
        assert!(!carries_key(&inventory, door.faction));
        assert!(hand_over_key(&mut inventory, &spec));
        assert!(carries_key(&inventory, door.faction));
        assert!(!hand_over_key(&mut inventory, &spec), "a second key is not given");
        let name = carried(&inventory).find(|(_, i)| is_key_of(i, door.faction)).map(|(_, i)| i.name.clone());
        assert_eq!(name, Some(format!("Key of {}", spec.name)));
        // Another faction's key is not this one's.
        assert!(!carries_key(&inventory, Faction(door.faction.0.wrapping_add(1))));
    }

    #[test]
    fn an_allied_delver_is_handed_the_key_once_and_it_is_theirs_to_unlock_with() {
        let (level, door) = claimed();
        let mut app = App::new();
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.equip(pack, fb_items::Slot::Back).unwrap();
        let mut record = WorldRecord::default();
        let now = record.standings.get(door.faction).value();
        record.standings.shift(door.faction, 75 - now);
        assert_eq!(record.standings.band(door.faction), Band::Allied);
        app.insert_resource(CurrentLevel(level)).insert_resource(record).insert_resource(inventory).add_systems(Update, give_keys);
        app.update();
        app.update();
        assert!(carries_key(app.world().resource::<Inventory>(), door.faction));
        assert_eq!(app.world().resource::<WorldRecord>().keys_given, vec![door.faction]);
        let keys = carried(app.world().resource::<Inventory>()).filter(|(_, i)| is_key_of(i, door.faction)).count();
        assert_eq!(keys, 1);
        // Even once the key is set down, the faction does not give another.
        let id = carried(app.world().resource::<Inventory>()).find(|(_, i)| is_key_of(i, door.faction)).map(|(id, _)| id).unwrap();
        app.world_mut().resource_mut::<Inventory>().forget(id);
        app.update();
        assert!(!carries_key(app.world().resource::<Inventory>(), door.faction));
    }

    #[test]
    fn a_carried_key_unlocks_a_locked_door_and_lockpicks_pick_it() {
        let (level, door) = claimed();
        let spec = spec_of(&level, door.faction);
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.equip(pack, fb_items::Slot::Back).unwrap();
        assert!(!carries_lockpicks(&inventory));
        let picks_id = inventory.add(fb_items::roll_item("Lockpicks", 3).unwrap());
        inventory.pick_up(picks_id).unwrap();
        assert!(carries_lockpicks(&inventory));
        hand_over_key(&mut inventory, &spec);
        let mut doors = hung(&level, &door, false);
        let state = doors.0.get_mut(&(door.x, door.y)).unwrap();
        assert!(state.locked(Band::Wary));
        // With the picks: some roll of the die gets through a Wary door, and some does not.
        let rolls: Vec<bool> = (1..=20).map(|d| picks(d, 12, worn_lockpicking(&inventory), pick_dc(Band::Wary))).collect();
        assert!(rolls.iter().any(|&r| r) && rolls.iter().any(|&r| !r));
        state.unlocked = true;
        assert!(!state.locked(Band::Wary));
    }

    #[test]
    fn doors_record_and_restore() {
        let (level, door) = claimed();
        let mut doors = hung(&level, &door, true);
        {
            let state = doors.0.get_mut(&(door.x, door.y)).unwrap();
            state.open = true;
            state.unlocked = true;
            state.swing = 1.0;
        }
        let left = LevelRecord { doors: doors.records(), ..LevelRecord::default() };
        let json = serde_json::to_string(&left).unwrap();
        let left: LevelRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(left.doors, vec![DoorRecord { x: door.x, y: door.y, open: true, unlocked: true }]);
        let doorway = geometry::doorways(&geometry::CellView::new(&level)).into_iter().find(|w| (w.x, w.y) == (door.x, door.y)).unwrap();
        let mut again = Doors::default();
        again.add(&ClaimDoor { main: true, ..door }, &doorway, Some(&left));
        assert_eq!(again, doors);
        assert!(!again.shuts(door.x, door.y));
        // A record from before doors has none, and the door stands shut and locked.
        let old: LevelRecord = serde_json::from_str(r#"{"floor":[],"containers":{},"creatures":{}}"#).unwrap();
        let mut fresh = Doors::default();
        fresh.add(&door, &doorway, Some(&old));
        assert!(fresh.shuts(door.x, door.y) && !fresh.0[&(door.x, door.y)].unlocked);
    }

    #[test]
    fn a_shut_door_stops_creatures_and_other_factions_and_blocks_their_sight_but_not_its_own() {
        let (level, door) = claimed();
        let mut doors = hung(&level, &door, false);
        let state = doors.0[&(door.x, door.y)].clone();
        let other = Faction(door.faction.0.wrapping_add(1));
        let (from, to) = (state.centre - state.through * CELL, state.centre + state.through * CELL);
        let none = BlockedCells::default();
        let reach = |who: Option<Faction>, doors: &Doors| slide_around(from, to, 1.5, solid_for(&level, &none, Some(doors), who), &[]).distance(to);
        let sees = |who: Option<Faction>, doors: &Doors| line_of_sight((from.x / CELL, from.y / CELL), (to.x / CELL, to.y / CELL), solid_for(&level, &none, Some(doors), who));
        // A creature and a folk of another faction are stopped, and cannot see through.
        for who in [None, Some(other)] {
            assert!(reach(who, &doors) > CELL * 0.4, "{who:?} got through");
            assert!(!sees(who, &doors), "{who:?} saw through");
        }
        // The door's own folk pass, and see.
        assert!(reach(Some(door.faction), &doors) < 1.0);
        assert!(sees(Some(door.faction), &doors));
        // Open, it stops no one.
        doors.0.get_mut(&(door.x, door.y)).unwrap().open = true;
        assert!(reach(None, &doors) < 1.0 && sees(None, &doors));
    }

    #[test]
    fn leaving_a_level_records_its_doors() {
        use bevy::ecs::system::RunSystemOnce;
        let (level, door) = claimed();
        let depth = level.depth;
        let mut doors = hung(&level, &door, false);
        doors.0.get_mut(&(door.x, door.y)).unwrap().unlocked = true;
        let mut app = App::new();
        app.insert_resource(CurrentLevel(level)).insert_resource(Inventory::default()).insert_resource(WorldRecord::default()).insert_resource(doors.clone());
        // `record_level_left` runs in `LeavingLevel`, with the old level and its doors still
        // in place; `spawn_fixtures` resets `Doors` only once the level has been swapped.
        app.world_mut().run_system_once(crate::record::record_level_left).unwrap();
        assert_eq!(app.world().resource::<WorldRecord>().levels[&depth].doors, doors.records());
    }

    /// An app that runs `use_door` with the delver aimed at a door of its faction, holding
    /// lockpicks and R, the faction standing in `band_value`, and the dice seeded to roll `seed`.
    /// `folk_near` stands a folk of the faction at the door, `folk_far` one out of earshot.
    fn picking(band_value: i32, seed: u32, folk_near: bool, folk_far: bool) -> (App, (i32, i32), Faction) {
        let (level, door) = claimed();
        let doors = hung(&level, &door, false);
        let spec = level.faction(door.faction).clone();
        let mut spot = level.folk[0];
        spot.faction = door.faction;
        let person = crate::folk::Person::of(&spot, &spec, 1);
        let at = doors.0[&(door.x, door.y)].centre;
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.equip(pack, fb_items::Slot::Back).unwrap();
        let picks = inventory.add(fb_items::roll_item("Lockpicks", 3).unwrap());
        inventory.pick_up(picks).unwrap();
        let mut record = WorldRecord::default();
        let now = record.standings.get(door.faction).value();
        record.standings.shift(door.faction, band_value - now);
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyR);
        let mut app = App::new();
        app.insert_resource(CurrentLevel(level))
            .insert_resource(doors)
            .insert_resource(AimedDoor(Some((door.x, door.y))))
            .insert_resource(inventory)
            .insert_resource(record)
            .insert_resource(RunDice(Mulberry32::new(seed)))
            .insert_resource(Keymap::default())
            .insert_resource(keys)
            .insert_resource(Time::<()>::default())
            .add_systems(Update, use_door);
        for (here, offset) in [(folk_near, 3.0 * CELL), (folk_far, NOISE_RANGE * 2.0)] {
            if here {
                let denizen = Denizen { person: person.clone(), radius: 2.5, min: Vec3::ZERO, max: Vec3::ZERO };
                app.world_mut().spawn((denizen, Transform::from_xyz(at.x, 0.0, at.y + offset)));
            }
        }
        (app, (door.x, door.y), door.faction)
    }

    /// Holds R for two seconds, the pick taking 1.5.
    fn hold_the_pick(app: &mut App) {
        for _ in 0..4 {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(500));
            app.update();
        }
    }

    fn seed_rolling(first: u32) -> u32 {
        (1..10_000).find(|&s| Mulberry32::new(s).int(1, 20) == first).unwrap()
    }

    #[test]
    fn holding_the_pick_opens_a_lock_when_the_roll_succeeds() {
        // A natural 20 beats the hardest door (18 against 20 and the picks' 1).
        let (mut app, cell, _) = picking(-75, seed_rolling(20), false, false);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(500));
        app.update();
        assert!(!app.world().resource::<Doors>().0[&cell].unlocked, "the roll waits for the hold");
        hold_the_pick(&mut app);
        assert!(app.world().resource::<Doors>().0[&cell].unlocked);
    }

    #[test]
    fn a_heard_miss_costs_standing_and_an_unheard_one_costs_nothing() {
        let miss = seed_rolling(1);
        let standing = |app: &App, f: Faction| app.world().resource::<WorldRecord>().standings.get(f).value();
        // A folk of the faction at the door hears it, though the door is shut between them.
        let (mut app, cell, faction) = picking(-30, miss, true, false);
        hold_the_pick(&mut app);
        assert!(!app.world().resource::<Doors>().0[&cell].unlocked);
        assert_eq!(standing(&app, faction), -30 - NOISE_COST);
        // One out of earshot does not.
        let (mut app, cell, faction) = picking(-30, miss, false, true);
        hold_the_pick(&mut app);
        assert!(!app.world().resource::<Doors>().0[&cell].unlocked);
        assert_eq!(standing(&app, faction), -30);
    }

    #[test]
    fn a_leaf_swings_from_shut_to_open() {
        let leaf = DoorLeaf { cell: (1, 1), shut: Quat::IDENTITY };
        assert!(leaf.rotation(0.0).angle_between(Quat::IDENTITY) < 1e-4);
        let open = leaf.rotation(1.0).angle_between(Quat::IDENTITY);
        assert!((open - 85f32.to_radians()).abs() < 1e-3, "{open}");
    }
}
