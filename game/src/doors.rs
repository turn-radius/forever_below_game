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
//! for [`PICK_SECONDS`]): a [`pick_check`] rolled on the dice panel ([`crate::dice`]), d20 plus
//! the Dexterity modifier, the lockpicking of each worn item (a line named by the item) and
//! [`LOCKPICKS_BONUS`] for the picks, against a difficulty that rises as the faction likes the
//! delver less ([`pick_dc`]). The delver stands still while the dice show, and the door
//! unlocks, or the noise is paid for, only when the panel closes ([`finish_pick`]). A miss is
//! noise, which carries through a shut door: a folk of the faction within [`NOISE_RANGE`] hears
//! it and the faction's standing drops by [`NOISE_COST`]; unheard, a miss costs only the hold. A door unlocked by key or picks stays unlocked.
//!
//! A shut door is a wall, and opaque to sight, for everyone but the folk of its own faction
//! ([`solid_for`], read by the delver's walk, folk routines and combat, creatures, and every
//! line of sight). A folk of the faction who walks up to it opens it for themselves, and it
//! swings shut behind them after [`CLOSE_BEHIND`] seconds, so a locked door never stays open to the
//! delver; a door the delver opened stays as they left it. If standing drops while a door is
//! open it stays open (and passable) until closed with F; then the lock applies. An open door
//! never reads "Locked": its prompt is "F Close".
//!
//! A lock holds from outside only. From the claim's own side of the door ([`DoorState::inside`])
//! a lever on the side wall beside the frame ([`Switch`], F "Pull the lever") swings the door open
//! whatever the band, and pulling it again shuts the door; the door itself says "Locked" with a
//! pointer to the lever, and the key and picks are for the outside. The lever's handle follows the
//! door (thrown while the door is open), so the door's record keeps the lever. Every member of a
//! faction carries its key (`crate::folk`), so it drops with their belongings when they are slain.
//!
//! All the numbers (the difficulties, the pick time, the noise) are our own. Each door's state,
//! open or shut and unlocked or not, goes into the level's record (`crate::record`).

use std::collections::BTreeMap;

use bevy::prelude::*;
use fb_character::check::Check;
use fb_faction::{Band, Faction, FactionSpec};
use fb_items::Item;
use fb_level::{CELL, ClaimDoor, DoorKind, SwitchKind};
use procgen_rng::Mulberry32;

use crate::carry::{Aimed, REACH};
use crate::character::{Character, modifier};
use crate::combat::RunDice;
use crate::combat::feedback::Corpse;
use crate::combat::vitals::{Dying, Fallen};
use crate::dice::{RollDone, RollPurpose, RollRequest};
use crate::folk::{AimedFolk, Denizen};
use crate::folk_routine::Routine;
use crate::hostility::Hostility;
use crate::interact::{CrosshairPrompt, Delver, Lock, Target, key_line, verbs_for};
use crate::inventory::{Inventory, ItemId, Toasts};
use crate::keymap::{Action, ActionInput, Keymap};
use crate::level::geometry::{self, DOOR_HEIGHT, DOOR_WIDTH, Doorway};
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Player, PlayerCamera, level_has_focus};
use crate::player::blocked_by;
use crate::record::{DoorRecord, LevelRecord, WorldRecord};

/// Seconds a door leaf takes to swing open or shut.
pub const SWING_SECONDS: f32 = 0.6;
/// Seconds a door of `kind` takes to move: a plank door is quick, a steel one heavy, a grate slow.
pub fn door_seconds(kind: DoorKind) -> f32 {
    match kind {
        DoorKind::Wood => SWING_SECONDS,
        DoorKind::Steel => SWING_SECONDS * 1.4,
        DoorKind::Portcullis => PORTCULLIS_SECONDS,
    }
}

/// How far a rising grate has come when it clears the delver's head (its feet above 17 units
/// by about three quarters of the rise), and so stops blocking the body. Falling, it blocks at once.
pub const GRATE_CLEAR: f32 = 0.75;
/// Seconds a portcullis takes to rise or drop.
pub const PORTCULLIS_SECONDS: f32 = 1.6;
/// Seconds a lever's handle takes to be thrown.
pub const LEVER_SECONDS: f32 = 0.35;
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
/// Seconds after the last folk of its faction leaves a door they opened before it swings shut.
pub const CLOSE_BEHIND: f32 = 1.5;
/// How far past the door's own cell a body still stops it shutting, world units (a body's radius).
const IN_THE_WAY: f32 = 4.0;
/// How far past the door's opening a look still counts as aimed at it, world units.
const AIM_SLACK: f32 = 1.0;

pub struct DoorsPlugin;

impl Plugin for DoorsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(crate::launch::AppState::Delving), |mut pick: ResMut<PickInFlight>| pick.0 = None);
        app.init_resource::<Doors>().init_resource::<AimedDoor>().init_resource::<AimedSwitch>().init_resource::<PickInFlight>().add_systems(
            Update,
            (
                aim_door,
                prompt_door.after(crate::travel::prompt_stair),
                pick_for_capture,
                use_door.run_if(level_has_focus),
                use_switch.run_if(level_has_focus),
                open_for_capture,
                swing_leaves,
                throw_levers,
                pull_chains,
                give_keys.run_if(level_has_focus),
                finish_pick,
            )
                .chain()
                .in_set(DelveSystems),
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

/// The roll that picks a lock on a door of a faction in `band` and of `kind`, or `None` for a
/// door that cannot be picked (a portcullis). A steel door asks [`fb_level::STEEL_PICK_PENALTY`] more.
pub fn pick_dc_for(kind: DoorKind, band: Band) -> Option<i32> {
    kind.pick_penalty().map(|extra| pick_dc(band) + extra)
}

/// The roll that picks a lock on a wooden door of a faction in `band`.
pub fn pick_dc(band: Band) -> i32 {
    match band {
        Band::Hostile => DC_HOSTILE,
        Band::Wary => DC_WARY,
        _ => DC_NEUTRAL,
    }
}

/// The check that picks a lock of difficulty `dc`: a d20 with the Dexterity modifier of the
/// delver's score, the lockpicking of each worn item named by the item, and the picks.
pub fn pick_check(dc: i32, dexterity: i32, worn: &[(String, i32)]) -> Check {
    let mut check = Check::d20("Pick the lock", dc).stat("DEX", modifier(dexterity));
    for (name, amount) in worn {
        check = check.affix(name.clone(), *amount);
    }
    check.affix("Lockpicks", LOCKPICKS_BONUS)
}

/// The pick being rolled on the dice panel: the door's cell, its faction, and whether a miss was heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pick {
    cell: (i32, i32),
    faction: Faction,
    heard: bool,
}

/// The pick whose dice are showing. While it is, the delver stands still ([`level_has_focus`])
/// and no second pick starts.
#[derive(Resource, Default, Debug)]
pub struct PickInFlight(Option<Pick>);

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
    /// Whether a folk of the faction opened it for themselves rather than the delver. Such a
    /// door closes behind them after [`CLOSE_BEHIND`] seconds; one the delver opened stays as left.
    pub folk_held: bool,
    /// Seconds since a folk of the faction was last at a `folk_held` door.
    pub idle: f32,
    /// The doorway's centre on the floor and the way through it (world `(x, z)`).
    pub centre: Vec2,
    pub through: Vec2,
    /// The way from the door into the claim (world `(x, z)`, a unit step), or zero if unknown.
    pub inward: Vec2,
    /// Whether it has no outside to lock against: its far side is another claim's ground, or it
    /// has no claim side at all. It opens from either side, so no one is shut in by it.
    pub shared: bool,
    /// What it is made of, and the switch on its claim's side (`fb_level::ClaimDoor`).
    pub kind: DoorKind,
    pub switch: SwitchKind,
}

impl DoorState {
    /// Seconds its leaf takes to move: a plank door is quick, a steel one heavy, a grate slow.
    pub fn seconds(&self) -> f32 {
        door_seconds(self.kind)
    }

    /// Whether it is locked to a delver in `band` with its faction, from outside.
    pub fn locked(&self, band: Band) -> bool {
        !self.unlocked && !self.shared && locked_to(band, self.main)
    }

    /// Whether the door has a switch on its claim's side: not a door with no outside.
    pub fn switched(&self) -> bool {
        !self.shared && self.inward != Vec2::ZERO
    }

    /// Whether `feet` stand on the claim's side of the door, where the switch is.
    pub fn inside(&self, feet: Option<Vec2>) -> bool {
        feet.is_some_and(|f| self.switched() && (f - self.centre).dot(self.inward) > 0.0)
    }

    /// Whether the leaf stops a body: a shut door does, and a grate still on its way up does
    /// until it has risen clear of a head ([`GRATE_CLEAR`]); one coming down does from the moment
    /// it is told to (`open` is false then).
    pub fn blocks_body(&self) -> bool {
        !self.open || (self.kind == DoorKind::Portcullis && self.swing < GRATE_CLEAR)
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
            folk_held: false,
            idle: 0.0,
            centre: Vec2::new(door.x as f32 + 0.5, door.y as f32 + 0.5) * CELL,
            through: Vec2::new(doorway.through[0], doorway.through[2]),
            inward: Vec2::new(door.inward.0 as f32, door.inward.1 as f32),
            shared: door.shared || door.inward == (0, 0),
            kind: door.kind,
            switch: door.switch,
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
        self.0.get(&(x, y)).is_some_and(DoorState::blocks_body)
    }

    /// Whether cell `(x, y)` holds a shut door that stops someone of `faction` (`None` for the
    /// delver and creatures): a shut door stops everyone but the folk of its own faction.
    pub fn stops(&self, x: i32, y: i32, faction: Option<Faction>) -> bool {
        self.0.get(&(x, y)).is_some_and(|d| d.blocks_body() && Some(d.faction) != faction)
    }

    /// Whether cell `(x, y)` holds a shut door that blocks the sight of someone of `faction`:
    /// [`Doors::stops`] but for a grate, which is seen through.
    pub fn blinds(&self, x: i32, y: i32, faction: Option<Faction>) -> bool {
        self.0.get(&(x, y)).is_some_and(|d| !d.open && !d.kind.see_through() && Some(d.faction) != faction)
    }

    /// What the record keeps of every door.
    pub fn records(&self) -> Vec<DoorRecord> {
        // A door a folk is holding open for themselves is kept shut: it would close behind them.
        self.0.iter().map(|(&(x, y), d)| DoorRecord { x, y, open: d.open && !d.folk_held, unlocked: d.unlocked }).collect()
    }
}

/// Whether cell `(x, y)` blocks the sight of someone of `faction` (`None` for creatures) as a
/// wall: rock, a blocked cell, or a shut door that is not their own faction's and is not a
/// grate. A shut portcullis stops the body ([`solid_for`]) but not the eye.
pub fn opaque_for<'a>(level: &'a fb_level::Level, cells: &'a BlockedCells, doors: Option<&'a Doors>, faction: Option<Faction>) -> impl Fn(i32, i32) -> bool + 'a {
    // Bars stop the body but not the eye.
    move |x, y| !level.cell(x, y).is_clear() || cells.0.contains(&(x, y)) || doors.is_some_and(|d| d.blinds(x, y, faction))
}

/// Whether cell `(x, y)` stops someone of `faction` (`None` for creatures): rock, a blocked
/// cell, or a shut door that is not their own faction's. One test for walking, pathing and
/// sight, so a shut door is a wall to everyone but the door's own folk and opaque to them too.
pub fn solid_for<'a>(level: &'a fb_level::Level, cells: &'a BlockedCells, doors: Option<&'a Doors>, faction: Option<Faction>) -> impl Fn(i32, i32) -> bool + 'a {
    let rock = blocked_by(level, cells);
    move |x, y| rock(x, y) || doors.is_some_and(|d| d.stops(x, y, faction))
}

/// The leaf of a claim door: hung at its hinge and swinging, or a grate that rises.
#[derive(Component, Clone, Copy, Debug)]
pub struct DoorLeaf {
    pub cell: (i32, i32),
    /// The leaf's rotation when shut; the swing turns it about its hinge from there.
    pub shut: Quat,
    /// Where it is when shut (its hinge, or a grate's origin), in the world.
    pub hinge: Vec3,
    /// How far a grate rises when open, `0` for a leaf that swings.
    pub lift: f32,
    /// How much a grate's height shrinks, by the time it is fully raised, so what has risen into
    /// the wall never stands above the ceiling (`1.0` for no shrinking).
    pub squash: f32,
}

impl DoorLeaf {
    /// A swinging leaf hung at `hinge`.
    pub fn swinging(cell: (i32, i32), shut: Quat, hinge: Vec3) -> Self {
        DoorLeaf { cell, shut, hinge, lift: 0.0, squash: 1.0 }
    }

    /// The leaf's rotation with the door `swing` open (0 shut, 1 open).
    pub fn rotation(&self, swing: f32) -> Quat {
        if self.lift > 0.0 { self.shut } else { self.shut * Quat::from_rotation_y(geometry::door::open_angle(swing)) }
    }

    /// Where the leaf is with the door `swing` open: a grate has risen by its lift, eased.
    pub fn translation(&self, swing: f32) -> Vec3 {
        let t = swing.clamp(0.0, 1.0);
        self.hinge + Vec3::Y * self.lift * (t * t * (3.0 - 2.0 * t))
    }

    /// The grate's height scale with the door `swing` open: it telescopes as it rises, so the
    /// part that has gone up into the wall stays under the ceiling.
    pub fn height_scale(&self, swing: f32) -> f32 {
        let t = swing.clamp(0.0, 1.0);
        1.0 + (self.squash - 1.0) * (t * t * (3.0 - 2.0 * t))
    }

    pub fn transform(&self, swing: f32) -> Transform {
        Transform::from_translation(self.translation(swing)).with_rotation(self.rotation(swing)).with_scale(Vec3::new(1.0, self.height_scale(swing), 1.0))
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

/// The lockpicking each worn or held item gives, named by the item, for the pick's modifiers.
pub fn worn_lockpicking_by_item(inventory: &Inventory) -> Vec<(String, i32)> {
    inventory.equipped().iter().map(|(_, item)| (item.title().to_string(), item.profile.stats.lockpicking.round() as i32)).filter(|(_, n)| *n != 0).collect()
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
    switches: Query<(Entity, &GlobalTransform, &Switch)>,
    mut aimed_switch: ResMut<AimedSwitch>,
) {
    let busy = aimed.is_some_and(|a| a.0.is_some()) || aimed_folk.is_some_and(|a| a.0.is_some()) || fallen.is_some() || dying.is_some();
    let eye = cameras.iter().next();
    let lever = if busy { None } else { eye.and_then(|eye| switch_in_reach(switches.iter(), eye.translation(), eye.forward().as_vec3())) };
    // A lever under the crosshair wins over the door beside it.
    let found = if busy || doors.0.is_empty() || lever.is_some() { None } else { eye.and_then(|eye| door_in_reach(&doors, eye.translation(), eye.forward().as_vec3())) };
    if aimed_door.0 != found {
        aimed_door.0 = found;
    }
    if aimed_switch.0 != lever {
        aimed_switch.0 = lever;
    }
}

/// What the prompt over the crosshair says for a door of `kind` that is `open` and `locked`,
/// holding `key` and `picks`: "F Open", "F Close", or "Locked \u{b7} G Unlock \u{b7} R Pick the lock".
/// A grate is raised and lowered, is not picked, and from inside is worked by its `switch`.
pub fn door_line(keymap: &Keymap, kind: DoorKind, switch: SwitchKind, open: bool, locked: bool, key: bool, picks: bool, inside: bool) -> String {
    if kind == DoorKind::Portcullis {
        if inside {
            return format!("Worked by the {} beside the gate", switch.name());
        }
        let name = |action| keymap.inputs(action).first().map(|input| input.label()).unwrap_or_default();
        return match (open, locked) {
            (true, _) => format!("{} Lower", name(Action::Primary)),
            (false, false) => format!("{} Raise", name(Action::Primary)),
            (false, true) if key => format!("Locked \u{b7} {} Raise with key", name(Action::Secondary)),
            (false, true) => "Locked".to_string(),
        };
    }
    let lock = Lock { has_lock: true, locked };
    // The key and picks are for the outside; inside, the lever is the way.
    let delver = Delver { has_key: key && locked && !open && !inside, has_lockpicks: picks && !inside, hidden: false };
    let mut verbs = verbs_for(&Target::Door { open, lock }, &delver);
    if locked && !open {
        if inside {
            return format!("Locked \u{b7} the {} is by the door", switch.name());
        }
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
    aimed_switch: Res<AimedSwitch>,
    doors: Res<Doors>,
    record: Option<Res<WorldRecord>>,
    inventory: Option<Res<Inventory>>,
    keymap: Option<Res<Keymap>>,
    players: Query<&Transform, With<Player>>,
    mut prompt: ResMut<CrosshairPrompt>,
    mut said: Local<Option<String>>,
) {
    let feet = players.iter().next().map(|t| t.translation.xz());
    let lever = aimed_switch.0.map(|_| key_line(keymap.as_deref().unwrap_or(&Keymap::default()), &verbs_for(&Target::Switch, &Delver::default())));
    let line = lever.or_else(|| aimed.0.and_then(|cell| doors.0.get(&cell)).map(|door| {
        let locked = door.locked(band_of(record.as_deref(), door.faction));
        let (key, picks) = inventory.as_deref().map_or((false, false), |i| (carries_key(i, door.faction), carries_lockpicks(i)));
        let fallback = Keymap::default();
        door_line(keymap.as_deref().unwrap_or(&fallback), door.kind, door.switch, door.open, locked, key, picks, door.inside(feet))
    }));
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
    record: Option<ResMut<WorldRecord>>,
    character: Option<Res<Character>>,
    dice: Option<ResMut<RunDice>>,
    players: Query<&Transform, With<Player>>,
    folk: Query<(&Transform, &Denizen), Without<Corpse>>,
    mut toasts: Toasts,
    mut picking: Local<Option<((i32, i32), f32)>>,
    mut spare_rolls: Local<u32>,
    mut pending: ResMut<PickInFlight>,
    mut rolls: MessageWriter<RollRequest>,
) {
    let Some(cell) = aimed.0 else {
        *picking = None;
        return;
    };
    let Some(door) = doors.0.get_mut(&cell) else { return };
    let band = band_of(record.as_deref(), door.faction);
    let feet = players.iter().next().map(|t| t.translation.xz());
    let inside = door.inside(feet);
    // A lock holds from outside only: the key and picks are for that side.
    let locked = door.locked(band) && !inside;
    let (key, lockpicks) = inventory.as_deref().map_or((false, false), |i| (carries_key(i, door.faction), carries_lockpicks(i)));
    let spec = level.0.factions.iter().find(|f| f.id == door.faction);
    let name = spec.map_or_else(|| "they".to_string(), |f| f.name.clone());

    let grate = door.kind == DoorKind::Portcullis;
    if input.just_pressed(Action::Primary) {
        if grate && inside {
            // The claim's side works its grate by the switch beside it, not the grate itself.
            toasts.say(format!("The {} beside the gate works it", door.switch.name()));
        } else if door.open {
            // A door is not shut on someone standing in it.
            if feet.is_some_and(|f| (f - door.centre).abs().max_element() < CELL * 0.5 + IN_THE_WAY) {
                toasts.say("Something is in the way");
            } else {
                door.open = false;
                door.folk_held = false;
            }
        } else if door.locked(band) && inside {
            toasts.say(format!("The door is locked, the {} is by the door", door.switch.name()));
        } else if locked {
            toasts.say(if grate { "The gate is down, it needs the key at the gate post" } else { "The door is locked" });
        } else {
            door.open = true;
            door.folk_held = false;
        }
    }
    if input.just_pressed(Action::Secondary) && locked && !door.open && key {
        door.unlocked = true;
        if grate {
            // The key turns in the gate post, and the grate rises.
            door.open = true;
            door.folk_held = false;
            toasts.say(format!("You raise the gate with the key of {name}"));
        } else {
            toasts.say(format!("You unlock the door with the key of {name}"));
        }
    }
    if grate && locked && lockpicks && !door.open && input.just_pressed(Action::Tertiary) {
        toasts.say("The grate can't be picked");
    }

    // Picking: a press starts it, holding it runs the clock, the roll comes at the end.
    let locked = door.locked(band) && !inside;
    if pending.0.is_some() || !(locked && lockpicks && !grate && !door.open && input.pressed(Action::Tertiary)) {
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
    let Some(dc) = pick_dc_for(door.kind, band) else { return };
    // Without the run's dice (a bare app), a counter of its own keeps each roll different.
    *spare_rolls += 1;
    let mut fallback = Mulberry32::new(*spare_rolls ^ cell.0 as u32 ^ (cell.1 as u32) << 8);
    let mut dice = dice;
    let dexterity = character.as_deref().map_or(10, Character::dexterity);
    let worn = inventory.as_deref().map_or_else(Vec::new, worn_lockpicking_by_item);
    let check = pick_check(dc, dexterity, &worn);
    let roll = match dice.as_mut() {
        Some(dice) => check.roll(&mut dice.0),
        None => check.roll(&mut fallback),
    };
    // A miss is noise, and sound carries through a shut door: a folk of the faction within
    // `NOISE_RANGE` hears it, whatever lies between. Who hears is settled as the picks are
    // worked, so the verdict can say so; the standing drops when the panel closes.
    let faction = door.faction;
    let at = door.centre;
    let heard = !roll.succeeded && folk.iter().any(|(transform, denizen)| denizen.person.faction.id == faction && transform.translation.xz().distance(at) <= NOISE_RANGE);
    let result = if roll.succeeded {
        "The lock gives".to_string()
    } else if heard {
        format!("{} heard the picks", capital(&name))
    } else {
        "The picks slip".to_string()
    };
    *pending = PickInFlight(Some(Pick { cell, faction, heard }));
    rolls.write(RollRequest { roll, purpose: RollPurpose::PickLock(cell), result: Some(result) });
}

/// Applies a pick once its dice have closed: a success unlocks the door, a heard miss costs the
/// faction's standing, an unheard one costs only the hold.
fn finish_pick(mut done: MessageReader<RollDone>, mut pending: ResMut<PickInFlight>, mut doors: ResMut<Doors>, mut record: Option<ResMut<WorldRecord>>) {
    for finished in done.read() {
        let RollPurpose::PickLock(cell) = finished.purpose else { continue };
        let Some(pick) = pending.0.take().filter(|p| p.cell == cell) else { continue };
        if finished.roll.succeeded {
            if let Some(door) = doors.0.get_mut(&cell) {
                door.unlocked = true;
            }
        } else if pick.heard {
            if let Some(record) = record.as_deref_mut() {
                record.standings.shift(pick.faction, -NOISE_COST);
            }
        }
    }
}

/// What a [`Switch`] works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchTarget {
    /// The claim door in this cell.
    Door((i32, i32)),
}

/// A wall switch wired to something, aimed at and pulled with F. Its entity is the lever's plate
/// or the chain's housing; the handle ([`LeverHandle`]) or the chain ([`ChainPull`]) moves on it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Switch {
    pub target: SwitchTarget,
    pub kind: SwitchKind,
}

/// The switch under the crosshair and in reach, if any.
#[derive(Resource, Debug, Default, PartialEq, Eq)]
pub struct AimedSwitch(pub Option<Entity>);

/// The handle of the lever of a door: it follows the door, thrown down while the door stands open.
#[derive(Component, Clone, Copy, Debug)]
pub struct LeverHandle {
    pub cell: (i32, i32),
    /// The handle's rotation unthrown, in world space.
    pub rest: Quat,
    /// How far it is thrown, 0 up to 1 thrown.
    pub pull: f32,
}

impl LeverHandle {
    /// The handle's rotation with the lever `pull` thrown: the arm swings up and down.
    pub fn rotation(&self, pull: f32) -> Quat {
        self.rest * Quat::from_rotation_x(geometry::door::lever_swing(pull))
    }
}

/// The chain and ring of a door: drawn down while the door moves and let spring back as it settles.
#[derive(Component, Clone, Copy, Debug)]
pub struct ChainPull {
    pub cell: (i32, i32),
    /// Where the chain hangs at rest, and the way down, in the world.
    pub rest: Vec3,
    pub down: Vec3,
}

impl ChainPull {
    /// Where the chain is with the door `swing` of the way open.
    pub fn translation(&self, swing: f32) -> Vec3 {
        self.rest + self.down * geometry::door::chain_dip(swing)
    }
}

/// How far along the way through the doorway the switch stands from the door's plane.
pub const SWITCH_BESIDE: f32 = 5.5;

/// Where the switch of a door stands on the claim side `inward` (a world `(x, z)` step into the
/// claim): on the side wall of the doorway's cell, at hand height, off the leaf's sweep, laid
/// on the drawn wall (the rock's relief moves it, and dressed stone stands proud by
/// [`geometry::door::switch_proud`]) and facing out of it. `None` where the door has no claim side.
pub fn switch_mount(doorway_at: &Transform, relief: Option<&geometry::Relief>, style: fb_level::LevelStyle, kind: SwitchKind, through: Vec2, inward: Vec2) -> Option<Transform> {
    if inward == Vec2::ZERO {
        return None;
    }
    let face = if inward.dot(through) >= 0.0 { 1.0 } else { -1.0 };
    let nominal = doorway_at.transform_point(Vec3::new(CELL * 0.5, geometry::door::switch_height(kind), face * SWITCH_BESIDE));
    let into = doorway_at.rotation * Vec3::NEG_X;
    let along = doorway_at.rotation * Vec3::Z;
    let arr = |v: Vec3| v.to_array();
    let wall = relief.map_or(nominal, |r| Vec3::from(r.displace(arr(nominal))));
    let normal = relief.map_or(into, |r| (Vec3::from(r.normal(arr(nominal), arr(along), arr(Vec3::Y), arr(into))) + into).normalize_or(into));
    let x = Vec3::Y.cross(normal).normalize_or(along);
    let y = normal.cross(x);
    Some(Transform::from_translation(wall + normal * geometry::door::switch_proud(style)).with_rotation(Quat::from_mat3(&Mat3::from_cols(x, y, normal))))
}

/// How far the lever of `door` is thrown: while the door stands open.
pub fn lever_target(door: &DoorState) -> f32 {
    f32::from(door.open)
}

/// The switch whose face lies under the look from `eye` along `forward`, if any, nearest first.
fn switch_in_reach<'a>(switches: impl Iterator<Item = (Entity, &'a GlobalTransform, &'a Switch)>, eye: Vec3, forward: Vec3) -> Option<Entity> {
    let mut best: Option<(f32, Entity)> = None;
    for (entity, at, switch) in switches {
        let normal = at.rotation() * Vec3::Z;
        let slant = forward.dot(normal);
        if slant > -1e-3 {
            continue;
        }
        let t = (at.translation() - eye).dot(normal) / slant;
        if t <= 0.0 || t > REACH || best.is_some_and(|(b, _)| b <= t) {
            continue;
        }
        let local = at.affine().inverse().transform_point3a((eye + forward * t).into());
        let (centre, half) = geometry::door::switch_aim(switch.kind);
        if (local.x - centre[0]).abs() <= half[0] + AIM_SLACK && (local.y - centre[1]).abs() <= half[1] + AIM_SLACK {
            best = Some((t, entity));
        }
    }
    best.map(|(_, entity)| entity)
}

/// What pulling a door's lever did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pulled {
    Opened,
    Shut,
    /// Someone stands in the doorway, so the door will not shut.
    InTheWay,
}

/// Pulls the lever of `door`: opens it whatever the band, or shuts it again.
pub fn pull_lever(door: &mut DoorState, feet: Option<Vec2>) -> Pulled {
    door.folk_held = false;
    if !door.open {
        door.open = true;
        Pulled::Opened
    } else if delver_in_doorway(door, feet) {
        Pulled::InTheWay
    } else {
        door.open = false;
        Pulled::Shut
    }
}

/// F on the aimed lever works what it is wired to.
fn use_switch(input: ActionInput, aimed: Res<AimedSwitch>, switches: Query<&Switch>, mut doors: ResMut<Doors>, players: Query<&Transform, With<Player>>, mut toasts: Toasts) {
    let Some(switch) = aimed.0.and_then(|e| switches.get(e).ok()) else { return };
    if !input.just_pressed(Action::Primary) {
        return;
    }
    match switch.target {
        SwitchTarget::Door(cell) => {
            let Some(door) = doors.0.get_mut(&cell) else { return };
            let feet = players.iter().next().map(|t| t.translation.xz());
            if pull_lever(door, feet) == Pulled::InTheWay {
                toasts.say("Something is in the way");
            }
        }
    }
}

/// Turns each lever's handle toward where its door stands.
fn throw_levers(time: Res<Time>, doors: Res<Doors>, mut handles: Query<(&mut LeverHandle, &mut Transform)>) {
    let step = time.delta_secs() / LEVER_SECONDS;
    for (mut handle, mut transform) in &mut handles {
        let Some(door) = doors.0.get(&handle.cell) else { continue };
        let want = lever_target(door);
        let pull = if handle.pull < want { (handle.pull + step).min(want) } else { (handle.pull - step).max(want) };
        if pull != handle.pull {
            handle.pull = pull;
            transform.rotation = handle.rotation(pull);
        }
    }
}

/// Draws each chain down while its door moves and lets it spring back as the door settles.
fn pull_chains(doors: Res<Doors>, mut chains: Query<(&ChainPull, &mut Transform)>) {
    for (chain, mut transform) in &mut chains {
        let Some(door) = doors.0.get(&chain.cell) else { continue };
        let at = chain.translation(door.swing);
        if transform.translation != at {
            transform.translation = at;
        }
    }
}

/// For captures (`FB_DOORS=open`): a few frames in, opens every door that is not locked to the
/// delver as they stand with its faction; `FB_DOORS=all` opens every claim door.
fn open_for_capture(mut doors: ResMut<Doors>, record: Option<Res<WorldRecord>>, mut frames: Local<u32>) {
    let mode = std::env::var("FB_DOORS").unwrap_or_default();
    if !matches!(mode.as_str(), "open" | "all") || doors.0.is_empty() {
        return;
    }
    *frames += 1;
    if *frames != 8 {
        return;
    }
    for door in doors.0.values_mut() {
        door.open = mode == "all" || !door.locked(band_of(record.as_deref(), door.faction));
    }
}

/// For captures (`FB_PICK=<face>[:<band>]`): stands the delver at the first locked door they can
/// pick, within reach outside and facing it, hands them lockpicks (a door with no folk within two cells, so none opens it), holds the faction at the middle
/// of `band` (`hostile`, `wary` by default or `neutral`) and rolls `face` on the pick, which the
/// next frames hold R for, so the dice panel shows over the door as in play.
/// `FB_DEBUG_ROLL_AT=<seconds>` freezes the roll.
fn pick_for_capture(
    mut doors: ResMut<Doors>,
    mut record: Option<ResMut<WorldRecord>>,
    mut inventory: Option<ResMut<Inventory>>,
    mut dice: Option<ResMut<RunDice>>,
    mut keys: Option<ResMut<ButtonInput<KeyCode>>>,
    mut players: Query<(&mut Transform, &mut crate::player::Locomotion), With<Player>>,
    folk: Query<&Transform, (With<Denizen>, Without<Player>)>,
    pending: Res<PickInFlight>,
    mut frames: Local<u32>,
    mut target: Local<Option<(i32, i32)>>,
    mut pressed_once: Local<bool>,
) {
    let Ok(spec) = std::env::var("FB_PICK") else { return };
    if doors.0.is_empty() {
        return;
    }
    *frames += 1;
    let (face, band) = spec.split_once(':').map_or((spec.as_str(), "wary"), |(f, b)| (f, b));
    let face = face.trim().parse::<u32>().unwrap_or(14).clamp(1, 20);
    if *frames == 30 {
        let (Some(record), Some(inventory), Some(dice)) = (record.as_deref_mut(), inventory.as_deref_mut(), dice.as_deref_mut()) else { return };
        let (middle, band) = match band.trim() {
            "hostile" => (-75, Band::Hostile),
            "neutral" => (0, Band::Neutral),
            _ => (-30, Band::Wary),
        };
        let Some((cell, door)) = doors.0.iter_mut().find(|(_, d)| d.kind != DoorKind::Portcullis && d.switched() && d.inward != Vec2::ZERO && (d.main || band != Band::Neutral) && !folk.iter().any(|t| t.translation.xz().distance(d.centre) < CELL * 2.0)) else { return };
        let now = record.standings.get(door.faction).value();
        record.standings.shift(door.faction, middle - now);
        door.unlocked = false;
        door.open = false;
        let outward = -door.inward;
        let stand = door.centre + outward * CELL * 0.85;
        if let Ok((mut at, mut locomotion)) = players.single_mut() {
            at.translation.x = stand.x;
            at.translation.z = stand.y;
            locomotion.yaw = (-door.inward.x).atan2(-door.inward.y);
            locomotion.pitch = 0.0;
        }
        if carries_lockpicks(inventory) {
            // Already carried.
        } else if let Some(picks) = fb_items::roll_item("Lockpicks", 3) {
            let id = inventory.add(picks);
            let _ = inventory.pick_up(id);
        }
        // The seed whose first d20 is `face`: the pick's check draws one die.
        if let Some(seed) = (1..100_000u32).find(|&s| Mulberry32::new(s).int(1, 20) == face) {
            dice.0 = Mulberry32::new(seed);
        }
        *target = Some(*cell);
    }
    // The capture drops every key each frame, so R is pressed anew until the pick is under way.
    *pressed_once |= pending.0.is_some();
    if *frames >= 60 && target.is_some() && !*pressed_once {
        if let Some(keys) = keys.as_deref_mut() {
            keys.press(KeyCode::KeyR);
        }
    }
}

/// Whether the delver stands in the doorway of `door`.
fn delver_in_doorway(door: &DoorState, feet: Option<Vec2>) -> bool {
    feet.is_some_and(|f| (f - door.centre).abs().max_element() < CELL * 0.5 + IN_THE_WAY)
}

/// One step of a door's own state: a folk of its faction at it opens it for themselves, and it
/// shuts behind them [`CLOSE_BEHIND`] seconds after the last one leaves (never on the delver in
/// the doorway). A door the delver opened is left as it is.
pub fn tend_door(door: &mut DoorState, folk_at: bool, delver_in_way: bool, dt: f32) {
    if folk_at {
        if !door.open {
            door.open = true;
            door.folk_held = true;
        }
        door.idle = 0.0;
    } else if door.folk_held {
        door.idle += dt;
        if door.idle >= CLOSE_BEHIND && !delver_in_way {
            door.open = false;
            door.folk_held = false;
        }
    }
}

/// Opens and shuts doors for the folk of their faction, then swings each leaf toward where its
/// door stands. A leaf that is swung open is a door that is open, so the prompt and the walk
/// agree with what the delver sees.
fn swing_leaves(
    time: Res<Time>,
    mut doors: ResMut<Doors>,
    folk: Query<(&Transform, &Denizen, Option<&Hostility>, Option<&Routine>), Without<Corpse>>,
    players: Query<&Transform, (With<Player>, Without<DoorLeaf>, Without<Denizen>)>,
    mut leaves: Query<(&DoorLeaf, &mut Transform), Without<Denizen>>,
) {
    let feet = players.iter().next().map(|t| t.translation.xz());
    for (leaf, mut transform) in &mut leaves {
        let Some(door) = doors.bypass_change_detection().0.get_mut(&leaf.cell) else { continue };
        // Folk raise a wooden or steel door for themselves as they walk up to it. A grate is not
        // raised for a guard standing at their post beside it, only for folk walking through its
        // cell on their way or hostile ones going out after the delver.
        let folk_at = folk.iter().any(|(t, d, hostility, routine)| {
            d.person.faction.id == door.faction
                && t.translation.xz().distance(door.centre) < FOLK_OPENS
                && (door.kind != DoorKind::Portcullis || hostility.is_some_and(|h| h.is_hostile()) || routine.is_some_and(|r| r.heads_through(leaf.cell)))
        });
        let in_way = delver_in_doorway(door, feet);
        tend_door(door, folk_at, in_way, time.delta_secs());
        let want = f32::from(door.open);
        let step = time.delta_secs() / door.seconds();
        let swing = if door.swing < want { (door.swing + step).min(want) } else { (door.swing - step).max(want) };
        if swing != door.swing {
            door.swing = swing;
            *transform = leaf.transform(swing);
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
    /// Whether a pick of this natural face succeeds, the worn lockpicking one line.
    fn picks(d20: u32, dexterity: i32, worn: i32, dc: i32) -> bool {
        pick_check(dc, dexterity, &[("Gloves of Thieves".to_string(), worn)]).with_natural(d20 as i32).succeeded
    }

    use super::*;
    use crate::player::slide_around;
    use fb_level::path::line_of_sight;

    /// A level with a claim door whose faction is `Faction`, and that door.
    fn claimed() -> (fb_level::Level, ClaimDoor) {
        (1..60)
            .filter_map(|seed| {
                let level = fb_level::generate(seed, 1);
                let door = level.claim_doors().into_iter().filter(|d| !d.shared && d.inward != (0, 0)).find(|d| geometry::doorways(&geometry::CellView::new(&level)).iter().any(|w| (w.x, w.y) == (d.x, d.y)))?;
                Some((level, ClaimDoor { kind: DoorKind::Wood, switch: SwitchKind::Lever, ..door }))
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
        assert_eq!(door_line(&keymap, DoorKind::Wood, SwitchKind::Lever, false, false, false, false, false), "F Open");
        assert_eq!(door_line(&keymap, DoorKind::Wood, SwitchKind::Lever, true, false, false, false, false), "F Close");
        assert_eq!(door_line(&keymap, DoorKind::Wood, SwitchKind::Lever, false, true, false, false, false), "Locked");
        assert_eq!(door_line(&keymap, DoorKind::Wood, SwitchKind::Lever, false, true, true, false, false), "Locked \u{b7} G Unlock with key");
        assert_eq!(door_line(&keymap, DoorKind::Wood, SwitchKind::Lever, false, true, false, true, false), "Locked \u{b7} R Pick the lock");
        assert_eq!(door_line(&keymap, DoorKind::Wood, SwitchKind::Lever, false, true, true, true, false), "Locked \u{b7} G Unlock with key \u{b7} R Pick the lock");
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
        let rolls: Vec<bool> = (1..=20).map(|d| picks(d, 12, worn_lockpicking_by_item(&inventory).iter().map(|(_, n)| n).sum(), pick_dc(Band::Wary))).collect();
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
            .init_resource::<PickInFlight>()
            .add_message::<RollRequest>()
            .add_message::<RollDone>()
            .add_systems(Update, (use_door, finish_pick).chain());
        for (here, offset) in [(folk_near, 3.0 * CELL), (folk_far, NOISE_RANGE * 2.0)] {
            if here {
                let denizen = Denizen { person: person.clone(), radius: 2.5, min: Vec3::ZERO, max: Vec3::ZERO };
                app.world_mut().spawn((denizen, Transform::from_xyz(at.x, 0.0, at.y + offset)));
            }
        }
        (app, (door.x, door.y), door.faction)
    }

    /// Holds R for two seconds, the pick taking 1.5, and returns the rolls the pick sent to the
    /// dice panel. Nothing has closed them yet.
    fn hold_the_pick(app: &mut App) -> Vec<RollRequest> {
        let mut sent = Vec::new();
        for _ in 0..4 {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(500));
            app.update();
            sent.extend(app.world_mut().resource_mut::<Messages<RollRequest>>().drain());
        }
        sent
    }

    /// The panel closing on `request`.
    fn close_the_panel(app: &mut App, request: &RollRequest) {
        app.world_mut().write_message(RollDone { roll: request.roll.clone(), purpose: request.purpose });
        app.update();
    }

    fn seed_rolling(first: u32) -> u32 {
        (1..10_000).find(|&s| Mulberry32::new(s).int(1, 20) == first).unwrap()
    }

    #[test]
    fn the_pick_is_a_check_with_a_line_for_each_helper_against_the_doors_difficulty() {
        let check = pick_check(DC_WARY + 4, 16, &[("Gloves of Thieves".to_string(), 2)]);
        assert_eq!((check.sides, check.target), (20, 19));
        let lines: Vec<_> = check.modifiers.iter().map(|m| (m.name.as_str(), m.amount)).collect();
        assert_eq!(lines, [("DEX", 3), ("Gloves of Thieves", 2), ("Lockpicks", 1)]);
        // On a door, the target is the faction's mood and the kind's penalty.
        let (mut app, cell, faction) = picking(-30, seed_rolling(7), false, false);
        let sent = hold_the_pick(&mut app);
        assert_eq!(sent.len(), 1);
        let band = band_of(app.world().get_resource::<WorldRecord>(), faction);
        let kind = app.world().resource::<Doors>().0[&cell].kind;
        assert_eq!((sent[0].roll.target, sent[0].roll.natural), (pick_dc_for(kind, band).unwrap(), 7));
        assert!(sent[0].roll.modifiers.iter().any(|m| m.name == "Lockpicks" && m.amount == LOCKPICKS_BONUS));
        assert_eq!(sent[0].purpose, RollPurpose::PickLock(cell));
    }

    #[test]
    fn holding_the_pick_opens_a_lock_only_once_the_panel_closes_on_a_success() {
        // A natural 20 beats the hardest door (18 against 20 and the picks' 1).
        let (mut app, cell, _) = picking(-75, seed_rolling(20), false, false);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(500));
        app.update();
        assert!(app.world().resource::<Messages<RollRequest>>().is_empty(), "the roll waits for the hold");
        let sent = hold_the_pick(&mut app);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].result.as_deref(), Some("The lock gives"));
        assert!(!app.world().resource::<Doors>().0[&cell].unlocked, "the dice are still showing");
        close_the_panel(&mut app, &sent[0]);
        assert!(app.world().resource::<Doors>().0[&cell].unlocked);
    }

    #[test]
    fn a_heard_miss_costs_standing_when_the_panel_closes_and_an_unheard_one_costs_nothing() {
        let miss = seed_rolling(1);
        let standing = |app: &App, f: Faction| app.world().resource::<WorldRecord>().standings.get(f).value();
        // A folk of the faction at the door hears it, though the door is shut between them.
        let (mut app, cell, faction) = picking(-30, miss, true, false);
        let sent = hold_the_pick(&mut app);
        assert_eq!(sent.len(), 1);
        assert!(sent[0].result.as_deref().is_some_and(|r| r.ends_with("heard the picks")), "{:?}", sent[0].result);
        assert_eq!(standing(&app, faction), -30, "not before the panel closes");
        close_the_panel(&mut app, &sent[0]);
        assert!(!app.world().resource::<Doors>().0[&cell].unlocked);
        assert_eq!(standing(&app, faction), -30 - NOISE_COST);
        // One out of earshot does not.
        let (mut app, cell, faction) = picking(-30, miss, false, true);
        let sent = hold_the_pick(&mut app);
        assert_eq!(sent[0].result.as_deref(), Some("The picks slip"));
        close_the_panel(&mut app, &sent[0]);
        assert!(!app.world().resource::<Doors>().0[&cell].unlocked);
        assert_eq!(standing(&app, faction), -30);
    }

    #[test]
    fn no_second_pick_starts_while_the_panel_shows() {
        let (mut app, _, _) = picking(-30, seed_rolling(1), false, false);
        let sent = hold_the_pick(&mut app);
        assert_eq!(sent.len(), 1);
        // R is still held and the panel is still up: another two seconds sends nothing.
        assert!(hold_the_pick(&mut app).is_empty());
        // Once it closes, a fresh press picks again.
        close_the_panel(&mut app, &sent[0]);
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::KeyR);
            keys.clear();
            keys.press(KeyCode::KeyR);
        }
        assert_eq!(hold_the_pick(&mut app).len(), 1);
    }

    #[test]
    fn the_delver_stands_still_while_the_dice_show() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.init_resource::<crate::dice::Dice>();
        assert!(app.world_mut().run_system_once(level_has_focus).unwrap());
        let request = RollRequest { roll: Check::d20("x", 10).with_natural(5), purpose: RollPurpose::Sample, result: None };
        app.world_mut().resource_mut::<crate::dice::Dice>().queue_for_test(request);
        assert!(!app.world_mut().run_system_once(level_has_focus).unwrap());
    }

    #[test]
    fn a_door_a_folk_opened_closes_behind_them() {
        let (level, door) = claimed();
        let mut doors = hung(&level, &door, true);
        let d = doors.0.get_mut(&(door.x, door.y)).unwrap();
        tend_door(d, true, false, 0.1);
        assert!(d.open && d.folk_held);
        tend_door(d, false, false, CLOSE_BEHIND * 0.5);
        assert!(d.open);
        tend_door(d, false, true, CLOSE_BEHIND);
        assert!(d.open, "not on the delver in the doorway");
        tend_door(d, false, false, 0.1);
        assert!(!d.open && !d.folk_held);
        assert!(doors.shuts(door.x, door.y));
    }

    #[test]
    fn a_door_the_delver_opened_stays_open_after_folk_pass() {
        let (level, door) = claimed();
        let mut doors = hung(&level, &door, false);
        let d = doors.0.get_mut(&(door.x, door.y)).unwrap();
        d.open = true;
        tend_door(d, true, false, 0.1);
        tend_door(d, false, false, CLOSE_BEHIND * 4.0);
        assert!(d.open && !d.folk_held);
    }

    #[test]
    fn an_open_door_is_never_locked_and_stays_passable_after_the_band_drops() {
        let (level, door) = claimed();
        let mut doors = hung(&level, &door, true);
        let keymap = Keymap::default();
        let d = doors.0.get_mut(&(door.x, door.y)).unwrap();
        assert!(!d.locked(Band::Friendly));
        d.open = true;
        assert!(d.locked(Band::Wary));
        for (key, picks) in [(false, false), (true, false), (false, true), (true, true)] {
            let line = door_line(&keymap, DoorKind::Wood, SwitchKind::Lever, d.open, d.locked(Band::Wary), key, picks, false);
            assert!(!line.contains("Locked") && line.contains("Close"), "{line}");
        }
        assert!(!doors.shuts(door.x, door.y));
        assert!(!doors.stops(door.x, door.y, None));
        let d = doors.0.get_mut(&(door.x, door.y)).unwrap();
        d.open = false;
        assert!(door_line(&keymap, DoorKind::Wood, SwitchKind::Lever, d.open, d.locked(Band::Wary), false, false, false).starts_with("Locked"));
    }

    #[test]
    fn records_keep_open_and_locked_consistent() {
        let (level, door) = claimed();
        let mut doors = hung(&level, &door, true);
        let d = doors.0.get_mut(&(door.x, door.y)).unwrap();
        tend_door(d, true, false, 0.1);
        assert_eq!(doors.records(), vec![DoorRecord { x: door.x, y: door.y, open: false, unlocked: false }]);
        doors.0.get_mut(&(door.x, door.y)).unwrap().folk_held = false;
        assert!(doors.records()[0].open);
    }

    /// The feet a step inside and outside of the claim door `state`.
    fn sides(state: &DoorState) -> (Vec2, Vec2) {
        (state.centre + state.inward * CELL, state.centre - state.inward * CELL)
    }

    #[test]
    fn from_inside_a_locked_door_points_to_the_lever_and_from_outside_stays_locked() {
        let (level, door) = claimed();
        let doors = hung(&level, &door, true);
        let state = &doors.0[&(door.x, door.y)];
        let (inside, outside) = sides(state);
        let keymap = Keymap::default();
        assert!(state.inside(Some(inside)) && !state.inside(Some(outside)) && !state.inside(None));
        let line = |feet, key| door_line(&keymap, DoorKind::Wood, SwitchKind::Lever, false, state.locked(Band::Wary), key, key, state.inside(Some(feet)));
        assert_eq!(line(inside, true), "Locked \u{b7} the lever is by the door");
        assert_eq!(line(outside, false), "Locked");
        assert_eq!(line(outside, true), "Locked \u{b7} G Unlock with key \u{b7} R Pick the lock");
    }

    #[test]
    fn pulling_the_lever_opens_a_locked_door_at_any_standing_and_pulling_again_shuts_it() {
        let (level, door) = claimed();
        let mut doors = hung(&level, &door, true);
        let state = doors.0.get_mut(&(door.x, door.y)).unwrap();
        let (inside, _) = sides(state);
        assert!(state.locked(Band::Hostile) && state.locked(Band::Wary));
        assert_eq!(pull_lever(state, Some(inside)), Pulled::Opened);
        assert!(state.open && lever_target(state) == 1.0);
        assert!(!doors.shuts(door.x, door.y));
        // Standing falls while the delver is inside: the door stays open, and the lever still works.
        let state = doors.0.get_mut(&(door.x, door.y)).unwrap();
        assert_eq!(pull_lever(state, Some(inside)), Pulled::Shut);
        assert!(!state.open && lever_target(state) == 0.0);
        assert_eq!(pull_lever(state, Some(inside)), Pulled::Opened, "reachable again from inside");
        // Not shut on the delver in the doorway.
        assert_eq!(pull_lever(state, Some(state.centre)), Pulled::InTheWay);
        assert!(state.open);
    }

    fn doorway_transform_of(doorway: &geometry::Doorway) -> Transform {
        let [x, y, z] = doorway.basis().map(|v| Vec3::new(v[0], v[1], v[2]));
        let c = geometry::cell_centre(doorway.x, doorway.y);
        Transform::from_translation(Vec3::new(c[0], c[1], c[2])).with_rotation(Quat::from_mat3(&Mat3::from_cols(x, y, z)))
    }

    #[test]
    fn a_switch_sits_on_the_claim_side_on_the_wall_and_clear_of_the_leaf() {
        let mut checked = [0u32; 2];
        for seed in 1..40 {
            for depth in [1, 2] {
                let level = fb_level::generate(seed, depth);
                let relief = geometry::Relief::new(&level);
                let view = geometry::CellView::new(&level);
                for door in level.claim_doors().into_iter().filter(|d| !d.shared && d.inward != (0, 0)) {
                    let Some(doorway) = geometry::doorways(&view).into_iter().find(|w| (w.x, w.y) == (door.x, door.y)) else { continue };
                    let mut doors = Doors::default();
                    doors.add(&door, &doorway, None);
                    let state = &doors.0[&(door.x, door.y)];
                    let at_door = doorway_transform_of(&doorway);
                    for kind in SwitchKind::ALL {
                        let mount = switch_mount(&at_door, Some(&relief), level.style, kind, state.through, state.inward).expect("the claim door has a switch");
                        let at = mount.translation;
                        // On the claim's side of the door's plane, in the door's cell, at a hand's height.
                        assert!((at.xz() - state.centre).dot(state.inward) > 0.0, "seed {seed}");
                        assert!((at.xz() - state.centre).abs().max_element() <= CELL * 0.5 + 2.5);
                        let wanted = geometry::door::switch_height(kind);
                        assert!((at.y - wanted).abs() < 2.0, "{kind:?} hangs at {} not about {wanted}", at.y);
                        // It faces out of the side wall, into the cell, and stands upright.
                        let normal = mount.rotation * Vec3::Z;
                        assert!(normal.dot(at_door.rotation * Vec3::NEG_X) > 0.8, "faces {normal}");
                        assert!((mount.rotation * Vec3::Y).y > 0.9);
                        // The leaf, at every angle of its swing and over its whole span, is clear of the
                        // switch's face: it never reaches the side wall.
                        let local = at_door.compute_affine().inverse().transform_point3(at);
                        let spec = geometry::door::DoorSpec::rolled(DoorKind::Steel, fb_level::DoorStyle::Timber, level.style, 1);
                        let leaf = geometry::door::claim_leaf(&spec);
                        let reach = leaf.leaf.parts.iter().flat_map(|p| p.positions.iter()).map(|p| p[0]).fold(0.0f32, f32::max);
                        for step in 0..=20 {
                            let angle = geometry::door::open_angle(step as f32 / 20.0);
                            let x = leaf.hinge[0] + reach * angle.cos();
                            assert!(local.x - x > 1.5, "leaf at {angle} reaches x {x}, the switch stands at {}", local.x);
                        }
                        checked[(kind == SwitchKind::Chain) as usize] += 1;
                    }
                    // The record keeps the door, and the switch follows it.
                    let mut open = doors.clone();
                    open.0.get_mut(&(door.x, door.y)).unwrap().open = true;
                    let left = LevelRecord { doors: open.records(), ..LevelRecord::default() };
                    let mut again = Doors::default();
                    again.add(&door, &doorway, Some(&left));
                    let restored = &again.0[&(door.x, door.y)];
                    assert!(restored.open && lever_target(restored) == 1.0);
                    assert_eq!((restored.kind, restored.switch), (door.kind, door.switch), "the record keeps the door's kind");
                }
            }
        }
        assert!(checked[0] > 10 && checked[1] > 10, "{checked:?}");
    }

    #[test]
    fn a_door_with_no_outside_is_never_locked_so_no_one_is_trapped() {
        let (level, door) = claimed();
        // Shared with another claim, or with no claim side to put a lever on: opens from either side.
        for shut_in in [ClaimDoor { shared: true, ..door }, ClaimDoor { inward: (0, 0), ..door }] {
            let doors = hung(&level, &shut_in, true);
            let state = &doors.0[&(door.x, door.y)];
            for band in [Band::Hostile, Band::Wary, Band::Neutral] {
                assert!(!state.locked(band), "{shut_in:?} {band:?}");
            }
            let keymap = Keymap::default();
            assert_eq!(door_line(&keymap, DoorKind::Wood, SwitchKind::Lever, false, state.locked(Band::Hostile), false, false, false), "F Open");
        }
        // The level's own shared doors, wherever it has them, are the same.
        for seed in 1..40 {
            for d in fb_level::generate(seed, 1).claim_doors().into_iter().filter(|d| d.shared) {
                assert!(d.inward == (0, 0) || d.shared);
            }
        }
    }

    #[test]
    fn a_door_does_not_shut_on_the_delver_in_the_doorway_or_re_lock_behind_them() {
        let (level, door) = claimed();
        let mut doors = hung(&level, &door, true);
        let state = doors.0.get_mut(&(door.x, door.y)).unwrap();
        tend_door(state, true, false, 0.1);
        // The folk have gone but the delver stands in the doorway: it stays open however long.
        tend_door(state, false, true, CLOSE_BEHIND * 10.0);
        assert!(state.open);
        // A door the delver opened and left unlocked stays so, and open, as the band falls.
        state.unlocked = true;
        assert!(!state.locked(Band::Hostile));
    }

    #[test]
    fn a_key_taken_from_a_slain_member_opens_the_door_from_outside() {
        let (level, door) = claimed();
        let faction = level.faction(door.faction).clone();
        let spot = level.folk.iter().find(|s| s.faction == door.faction).expect("the claim has folk");
        let person = crate::folk::Person::of(spot, &faction, 1);
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.equip(pack, fb_items::Slot::Back).unwrap();
        assert!(!carries_key(&inventory, faction.id));
        for item in person.goods.iter().filter(|i| is_key_of(i, faction.id)) {
            let id = inventory.add(item.clone());
            inventory.pick_up(id).unwrap();
        }
        assert!(carries_key(&inventory, faction.id));
        let keymap = Keymap::default();
        assert_eq!(door_line(&keymap, DoorKind::Wood, SwitchKind::Lever, false, true, carries_key(&inventory, faction.id), false, false), "Locked \u{b7} G Unlock with key");
    }

    #[test]
    fn every_faction_member_carries_the_factions_key() {
        for world in 1..=4 {
            let level = fb_level::generate(world, 1);
            for spot in &level.folk {
                let faction = level.faction(spot.faction).clone();
                let person = crate::folk::Person::of(spot, &faction, 1);
                assert!(person.goods.iter().any(|i| is_key_of(i, faction.id)), "{} has no key", person.name);
            }
        }
    }

    #[test]
    fn a_leaf_swings_from_shut_to_open_and_a_grate_rises() {
        let leaf = DoorLeaf::swinging((1, 1), Quat::IDENTITY, Vec3::new(2.0, 0.0, 3.0));
        assert!(leaf.rotation(0.0).angle_between(Quat::IDENTITY) < 1e-4);
        let open = leaf.rotation(1.0).angle_between(Quat::IDENTITY);
        assert!((open - 85f32.to_radians()).abs() < 1e-3, "{open}");
        assert_eq!(leaf.translation(1.0), leaf.hinge, "a swinging leaf stays on its hinge");
        let grate = DoorLeaf { lift: 23.0, ..leaf };
        assert_eq!(grate.translation(0.0), grate.hinge);
        assert!((grate.translation(1.0).y - 23.0).abs() < 1e-4 && grate.rotation(1.0).angle_between(Quat::IDENTITY) < 1e-4);
        assert!(grate.translation(0.5).y > 0.0 && grate.translation(0.5).y < 23.0);
    }

    /// The level, a claim door of it and the doors hung with that door made of `kind`.
    fn hung_of(kind: DoorKind) -> (fb_level::Level, ClaimDoor, Doors) {
        let (level, door) = claimed();
        let door = ClaimDoor { kind, ..door };
        let doors = hung(&level, &door, false);
        (level, door, doors)
    }

    #[test]
    fn a_shut_portcullis_stops_the_body_but_not_the_eye() {
        let (level, door, mut doors) = hung_of(DoorKind::Portcullis);
        let state = doors.0[&(door.x, door.y)].clone();
        let (from, to) = (state.centre - state.through * CELL, state.centre + state.through * CELL);
        let none = BlockedCells::default();
        let other = Faction(door.faction.0.wrapping_add(1));
        for who in [None, Some(other), Some(door.faction)] {
            let reach = slide_around(from, to, 1.5, solid_for(&level, &none, Some(&doors), who), &[]).distance(to);
            let sees = line_of_sight((from.x / CELL, from.y / CELL), (to.x / CELL, to.y / CELL), opaque_for(&level, &none, Some(&doors), who));
            if who == Some(door.faction) {
                assert!(reach < 1.0, "its own folk pass");
            } else {
                assert!(reach > CELL * 0.4, "{who:?} got through a shut grate");
            }
            assert!(sees, "{who:?} cannot see through the grate");
        }
        assert!(doors.shuts(door.x, door.y), "the delver is stopped");
        // A shut wooden or steel door is opaque to the same eyes.
        for kind in [DoorKind::Wood, DoorKind::Steel] {
            doors.0.get_mut(&(door.x, door.y)).unwrap().kind = kind;
            let sees = line_of_sight((from.x / CELL, from.y / CELL), (to.x / CELL, to.y / CELL), opaque_for(&level, &none, Some(&doors), None));
            assert!(!sees, "{kind:?} is see-through");
        }
    }

    #[test]
    fn a_steel_door_asks_four_more_of_a_pick_and_a_grate_cannot_be_picked() {
        for band in [Band::Neutral, Band::Wary, Band::Hostile] {
            assert_eq!(pick_dc_for(DoorKind::Wood, band), Some(pick_dc(band)));
            assert_eq!(pick_dc_for(DoorKind::Steel, band), Some(pick_dc(band) + 4));
            assert_eq!(pick_dc_for(DoorKind::Portcullis, band), None);
        }
        // The same die and hand that picks a wooden Wary door (15) fails the steel one (19) on a 14.
        assert!(picks(14, 10, 0, pick_dc_for(DoorKind::Wood, Band::Wary).unwrap()));
        assert!(!picks(14, 10, 0, pick_dc_for(DoorKind::Steel, Band::Wary).unwrap()));
        assert!(picks(18, 10, 0, pick_dc_for(DoorKind::Steel, Band::Wary).unwrap()));
    }

    #[test]
    fn holding_the_pick_on_a_portcullis_does_nothing() {
        // Holds the pick, shows the dice to their end, and says whether the door unlocked.
        fn picked(app: &mut App, cell: (i32, i32)) -> bool {
            for request in hold_the_pick(app) {
                close_the_panel(app, &request);
            }
            app.world().resource::<Doors>().0[&cell].unlocked
        }
        let (mut app, cell, _) = picking(-75, seed_rolling(20), false, false);
        app.world_mut().resource_mut::<Doors>().0.get_mut(&cell).unwrap().kind = DoorKind::Portcullis;
        assert!(hold_the_pick(&mut app).is_empty(), "no dice for a grate");
        let door = &app.world().resource::<Doors>().0[&cell];
        assert!(!door.unlocked && !door.open, "a natural 20 picked a grate");
        // And a steel door shrugs off the roll that a wooden one yields to: 14 + 1 (picks) is 15.
        let (mut app, cell, _) = picking(-30, seed_rolling(14), false, false);
        app.world_mut().resource_mut::<Doors>().0.get_mut(&cell).unwrap().kind = DoorKind::Steel;
        assert!(!picked(&mut app, cell));
        let (mut app, cell, _) = picking(-30, seed_rolling(14), false, false);
        app.world_mut().resource_mut::<Doors>().0.get_mut(&cell).unwrap().kind = DoorKind::Wood;
        assert!(picked(&mut app, cell), "the same roll picks a wooden one");
    }

    #[test]
    fn the_prompts_for_a_portcullis_offer_no_picking() {
        let keymap = Keymap::default();
        let line = |open, locked, key, picks, inside| door_line(&keymap, DoorKind::Portcullis, SwitchKind::Chain, open, locked, key, picks, inside);
        assert_eq!(line(false, true, false, true, false), "Locked");
        assert_eq!(line(false, true, true, true, false), "Locked \u{b7} G Raise with key");
        assert!(!line(false, true, true, true, false).contains("R Pick"), "no pick offered");
        assert_eq!(line(false, false, false, false, false), "F Raise");
        assert_eq!(line(true, false, false, false, false), "F Lower");
        assert_eq!(line(false, true, true, true, true), "Worked by the chain beside the gate");
        // A wooden door keeps its verbs, and names its lever or chain from inside.
        assert_eq!(door_line(&keymap, DoorKind::Steel, SwitchKind::Chain, false, true, true, true, true), "Locked \u{b7} the chain is by the door");
        assert_eq!(door_line(&keymap, DoorKind::Steel, SwitchKind::Lever, false, true, true, true, false), "Locked \u{b7} G Unlock with key \u{b7} R Pick the lock");
    }

    #[test]
    fn the_key_raises_a_portcullis_from_outside() {
        let (level, door, doors) = hung_of(DoorKind::Portcullis);
        let spec = spec_of(&level, door.faction);
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.equip(pack, fb_items::Slot::Back).unwrap();
        hand_over_key(&mut inventory, &spec);
        let state = doors.0[&(door.x, door.y)].clone();
        let outside = state.centre - state.inward * CELL;
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyG);
        let mut app = App::new();
        app.insert_resource(CurrentLevel(level))
            .insert_resource(doors)
            .insert_resource(AimedDoor(Some((door.x, door.y))))
            .insert_resource(inventory)
            .insert_resource(WorldRecord::default())
            .insert_resource(RunDice(Mulberry32::new(1)))
            .insert_resource(Keymap::default())
            .insert_resource(keys)
            .insert_resource(Time::<()>::default())
            .init_resource::<PickInFlight>().add_message::<RollRequest>().add_message::<RollDone>().add_systems(Update, use_door);
        app.world_mut().spawn((Player, Transform::from_xyz(outside.x, 0.0, outside.y)));
        app.update();
        let after = &app.world().resource::<Doors>().0[&(door.x, door.y)];
        // At Neutral the claim's other doors are locked; the key lifts the grate.
        assert!(after.unlocked && after.open, "{after:?}");
    }

    #[test]
    fn each_kind_moves_at_its_own_pace() {
        let (_, door, doors) = hung_of(DoorKind::Wood);
        let mut state = doors.0[&(door.x, door.y)].clone();
        let times: Vec<f32> = DoorKind::ALL
            .iter()
            .map(|&kind| {
                state.kind = kind;
                state.seconds()
            })
            .collect();
        assert!(times[0] < times[1] && times[1] < times[2], "{times:?}");
    }

    #[test]
    fn a_rising_grate_blocks_until_clear_and_a_falling_one_blocks_at_once() {
        let (_, door, mut doors) = hung_of(DoorKind::Portcullis);
        let cell = (door.x, door.y);
        let set = |doors: &mut Doors, open: bool, swing: f32| {
            let d = doors.0.get_mut(&cell).unwrap();
            d.open = open;
            d.swing = swing;
        };
        // Shut, and told to rise: still a wall for the first stretch of the rise.
        set(&mut doors, true, 0.0);
        assert!(doors.shuts(cell.0, cell.1) && doors.stops(cell.0, cell.1, None));
        set(&mut doors, true, 0.5);
        assert!(doors.shuts(cell.0, cell.1), "half up, a head would still meet it");
        set(&mut doors, true, GRATE_CLEAR + 0.01);
        assert!(!doors.shuts(cell.0, cell.1) && !doors.stops(cell.0, cell.1, None), "risen clear");
        // Told to drop: a wall from that moment, though it is still up.
        set(&mut doors, false, 1.0);
        assert!(doors.shuts(cell.0, cell.1) && doors.stops(cell.0, cell.1, None));
        // The door's own folk pass either way, and a swinging door follows its flag alone.
        set(&mut doors, true, 0.1);
        assert!(!doors.stops(cell.0, cell.1, Some(door.faction)));
        doors.0.get_mut(&cell).unwrap().kind = DoorKind::Wood;
        assert!(!doors.shuts(cell.0, cell.1), "an open wooden door lets the delver by at once");
    }

    #[test]
    fn a_shared_door_opens_with_f_from_both_sides_and_promises_no_switch() {
        let (level, door) = claimed();
        for kind in [DoorKind::Wood, DoorKind::Steel] {
            let shared = ClaimDoor { shared: true, kind, ..door };
            let doors = hung(&level, &shared, false);
            let state = doors.0[&(door.x, door.y)].clone();
            assert!(!state.switched(), "a door with no outside has no switch");
            for side in [state.inward, -state.inward] {
                let feet = state.centre + side * CELL;
                assert!(!state.inside(Some(feet)), "no claim side to be inside of");
                for band in [Band::Hostile, Band::Wary, Band::Neutral] {
                    assert!(!state.locked(band));
                }
                let mut app = App::new();
                let mut keys = ButtonInput::<KeyCode>::default();
                keys.press(KeyCode::KeyF);
                app.insert_resource(CurrentLevel(level.clone()))
                    .insert_resource(doors.clone())
                    .insert_resource(AimedDoor(Some((door.x, door.y))))
                    .insert_resource(Inventory::default())
                    .insert_resource(WorldRecord::default())
                    .insert_resource(RunDice(Mulberry32::new(1)))
                    .insert_resource(Keymap::default())
                    .insert_resource(keys)
                    .insert_resource(Time::<()>::default())
                    .init_resource::<PickInFlight>().add_message::<RollRequest>().add_message::<RollDone>().add_systems(Update, use_door);
                app.world_mut().spawn((Player, Transform::from_xyz(feet.x, 0.0, feet.y)));
                app.update();
                assert!(app.world().resource::<Doors>().0[&(door.x, door.y)].open, "{kind:?}: F from {side} did not open it");
            }
        }
        // A grate with no switch never says it has one.
        let keymap = Keymap::default();
        let line = door_line(&keymap, DoorKind::Portcullis, SwitchKind::Chain, false, false, false, false, false);
        assert!(!line.contains("chain"), "{line}");
    }

    #[test]
    fn a_guard_at_its_post_leaves_a_shut_grate_shut_and_a_walker_through_it_raises_it() {
        use crate::folk_routine::Routine;
        let (level, door, doors) = hung_of(DoorKind::Portcullis);
        let spec = level.faction(door.faction).clone();
        let mut spot = level.folk[0];
        spot.faction = door.faction;
        let person = crate::folk::Person::of(&spot, &spec, 1);
        let state = doors.0[&(door.x, door.y)].clone();
        let beside = state.centre + state.inward * CELL * 0.6;
        let run = |routine: Routine| {
            let mut app = App::new();
            app.insert_resource(doors.clone()).insert_resource(Time::<()>::default()).add_systems(Update, swing_leaves);
            let shut = Quat::IDENTITY;
            app.world_mut().spawn((DoorLeaf { cell: (door.x, door.y), shut, hinge: Vec3::ZERO, lift: 23.0, squash: 1.0 }, Transform::default()));
            let denizen = Denizen { person: person.clone(), radius: 2.5, min: Vec3::ZERO, max: Vec3::ZERO };
            app.world_mut().spawn((denizen, Transform::from_xyz(beside.x, 0.0, beside.y), routine));
            for _ in 0..5 {
                app.update();
            }
            app.world().resource::<Doors>().0[&(door.x, door.y)].open
        };
        let post = || Routine::placed(beside, state.through, 5.0, Mulberry32::new(1));
        assert!(!run(post()), "a guard standing at its post held the grate up");
        let walking = post().walking(state.centre + state.through * CELL, vec![(door.x, door.y)]);
        assert!(run(walking), "a folk walking through the door cell did not raise it");
        // A walker whose way does not go through the door leaves it shut.
        let elsewhere = post().walking(beside, vec![(door.x + 5, door.y + 5)]);
        assert!(!run(elsewhere));
    }

    #[test]
    fn a_door_with_no_switch_is_never_inside_of() {
        let (level, door) = claimed();
        let cut_off = ClaimDoor { inward: (0, 0), kind: DoorKind::Wood, ..door };
        let doors = hung(&level, &cut_off, false);
        let state = &doors.0[&(door.x, door.y)];
        assert!(!state.switched() && !state.inside(Some(state.centre + Vec2::X * CELL)));
    }
    /// The rule `fb_level` keeps guards clear by is the one the game draws: for every claim door
    /// with a swinging leaf, in rock caves and stone halls alike, the hinge and the open leaf's far
    /// edge, taken from the leaf as the game builds and hangs it, are the sweep's hinge and its
    /// far end. A portcullis has no sweep.
    #[test]
    fn the_leaf_sweep_is_where_the_game_hangs_the_leaf() {
        let (mut checked, mut styles, mut grates) = (0, std::collections::HashSet::new(), 0);
        for seed in 1..40 {
            for depth in 1..=10 {
                let level = fb_level::generate(seed, depth);
                let finishes = geometry::level_finishes(&level);
                let doorways = geometry::doorways(&geometry::CellView::new(&level));
                for door in level.claim_doors() {
                    let Some(doorway) = doorways.iter().find(|w| (w.x, w.y) == (door.x, door.y)) else { continue };
                    let style = finishes.door_style(door.x, door.y).unwrap_or(fb_level::DoorStyle::Timber);
                    let sweep = fb_level::door::LeafSweep::of(&level, &door, style);
                    if door.kind == DoorKind::Portcullis {
                        assert!(sweep.is_none(), "seed {seed} depth {depth}: a portcullis sweeps nothing");
                        grates += 1;
                        continue;
                    }
                    let sweep = sweep.expect("a framed swinging claim door has a sweep");
                    let fit = crate::level::ClaimFit::make(&level, &door, style, 1);
                    assert!(!fit.leaf.rises());
                    let [x, y, z] = doorway.basis().map(Vec3::from_array);
                    let shut = Quat::from_mat3(&Mat3::from_cols(x, y, z));
                    let at = Transform::from_translation(Vec3::from_array(geometry::cell_centre(door.x, door.y))).with_rotation(shut);
                    let hinge = at.transform_point(Vec3::from_array(fit.leaf.hinge));
                    let leaf = DoorLeaf::swinging((door.x, door.y), shut, hinge);
                    // The leaf's far edge: the greatest reach of its mesh from the pin along +X.
                    let reach = fit.leaf.leaf.parts.iter().flat_map(|p| p.positions.iter()).map(|p| p[0]).fold(0.0f32, f32::max);
                    let free = hinge + leaf.rotation(1.0) * Vec3::X * reach;
                    let cells = |v: Vec3| (v.x / CELL, v.z / CELL);
                    let (h, f) = (cells(hinge), cells(free));
                    assert!((h.0 - sweep.hinge.0).abs() < 1e-3 && (h.1 - sweep.hinge.1).abs() < 1e-3, "seed {seed} depth {depth} {style:?}: hinge {h:?} vs {:?}", sweep.hinge);
                    assert!((reach / CELL - sweep.width).abs() < 0.01, "seed {seed} depth {depth} {style:?}: leaf reaches {} cells, sweep {}", reach / CELL, sweep.width);
                    let far = (sweep.hinge.0 + sweep.through.0 as f32 * sweep.width, sweep.hinge.1 + sweep.through.1 as f32 * sweep.width);
                    // Open to 85 degrees: within a few percent of straight through.
                    assert!((f.0 - far.0).abs() < 0.07 && (f.1 - far.1).abs() < 0.07, "seed {seed} depth {depth} {style:?}: free edge {f:?} vs {far:?}");
                    styles.insert(style);
                    checked += 1;
                }
            }
        }
        assert!(checked > 40, "{checked} swinging claim doors");
        assert!(styles.contains(&fb_level::DoorStyle::RoundArch) || styles.contains(&fb_level::DoorStyle::PointedArch), "no stone-hall arch covered, only {styles:?}");
        assert!(grates > 0, "no portcullis seen");
    }

    /// No guard's post lies in any claim door's open leaf, taking the folk as 0.3 cell wide.
    #[test]
    fn no_guard_stands_in_an_open_door_leaf() {
        let mut guards = 0;
        for seed in 1..80 {
            for depth in 1..=3 {
                let level = fb_level::generate(seed, depth);
                let sweeps = fb_level::door::LeafSweep::all(&level);
                for g in level.folk.iter().filter(|f| f.trade == Some(fb_faction::Trade::guard())) {
                    guards += 1;
                    for (cell, sweep) in &sweeps {
                        assert!(!sweep.sweeps(g.x, g.y, fb_level::door::FOLK_RADIUS), "seed {seed} depth {depth}: guard at ({}, {}) in the leaf of {cell:?}", g.x, g.y);
                    }
                }
            }
        }
        assert!(guards > 100, "{guards} guards");
    }
}
