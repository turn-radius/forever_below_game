//! Tuning: how the game looks and feels, read from `assets/tuning.toml` so it can be changed
//! without touching the code. How large held and dropped things are drawn, how fast the delver
//! walks, how big creatures are and how they fight, how bright the lights burn, how loud the
//! mix is and how the inventory is laid out.
//!
//! The file is a Bevy asset ([`TuningLoader`]). In a debug build the asset server watches it
//! (`main` sets `AssetPlugin::watch_for_changes_override`; the `file_watcher` feature is on), so
//! saving it while the game runs reloads it and the [`Tuning`] resource changes within the
//! frame; every system that reads it follows, and those that spawned something from it (held
//! items, lights, creatures, the inventory screen) bring it up to date. Bevy's `hot_asset_reloading`
//! example does the same with `file_watcher`
//! (<https://github.com/bevyengine/bevy/blob/v0.20.0/examples/asset/hot_asset_reloading.rs>).
//!
//! A key the file leaves out, gives the wrong kind of value or does not know is passed over
//! with one warning naming it, and keeps its default ([`Tuning::from_toml`]); a file that is
//! not TOML at all gives every default with a warning. Nothing in it can stop the game.
//!
//! The defaults live in one place, the `Default` impls below, and `assets/tuning.toml` ships
//! with exactly those values (a test holds the two together). TOML is the format because it is
//! "a minimal configuration file format that's easy to read due to obvious semantics"
//! (<https://toml.io/en/>), with comments and named tables, which suits a file the player edits
//! by hand.
//!
//! Generators never read this: it changes how the world is drawn and played, never what a
//! seed makes.

use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};
use fb_creatures::Species;
use serde::{Deserialize, Serialize};

/// Where the tuning file is, under the asset root (`game/assets`).
pub const TUNING_PATH: &str = "tuning.toml";

/// Every presentation and feel value the game reads from [`TUNING_PATH`], one table each.
/// Units are the game's: 1 world unit is 10 cm, times are seconds.
#[derive(Asset, Resource, TypePath, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tuning {
    pub view: ViewTuning,
    pub moving: MovingTuning,
    pub creatures: CreatureTuning,
    pub folk: FolkTuning,
    pub combat: CombatTuning,
    pub light: LightTuning,
    pub sound: SoundTuning,
    pub interface: InterfaceTuning,
    pub input: InputTuning,
    pub clock: ClockTuning,
    pub recovery: RecoveryTuning,
}

/// How the first-person view draws.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewTuning {
    /// How much larger than its own size each held item is drawn, about its grip: the user's
    /// call ("scaled up 40%"), on top of the narrower field of view. Uniform, so the item keeps
    /// its proportions.
    pub held_scale: f32,
    /// Vertical field of view of the view model camera that draws what is held, degrees,
    /// whatever the world camera uses. Drawing what is held through a narrower field of view
    /// than the world's is the usual first-person technique: Bevy's `first_person_view_model`
    /// example gives its view model camera a fixed field of view of its own, because "the view
    /// model is typically designed and animated with a very specific FOV in mind"
    /// (<https://github.com/bevyengine/bevy/blob/main/examples/camera/first_person_view_model.rs>),
    /// and Unreal Engine's camera has a "First Person Field of View" for first-person
    /// primitives (<https://dev.epicgames.com/documentation/en-us/unreal-engine/first-person-rendering>).
    /// At 46 degrees an item at the same depth is drawn 1.36 times as tall as at the old 60
    /// (tan 30° / tan 23°). The value is our own tuning against captures.
    pub held_fov_degrees: f32,
    /// Vertical field of view of the world camera, degrees (about 100 across at 16:9).
    pub world_fov_degrees: f32,
    /// How much the view and what is held bob with each step, as a multiple of the usual (0
    /// holds them still).
    pub head_bob: f32,
    /// How much larger than its own size an item lying in the level is drawn, about where it
    /// rests: the user's call ("scaled up by 15%"), so things on the floor read from standing
    /// height. Its pickup box and floor radius grow with it.
    pub floor_item_scale: f32,
    /// How far off the crosshair's line, degrees, a floor item, container or person can lie
    /// and still be aimed at (`crate::carry::nearest_aimed`), so the crosshair need not sit
    /// exactly on it: the user's call ("there should be a buffer around it"); 2 degrees is our
    /// own tuning. Of several in the buffer, the one nearest the line is aimed at.
    pub aim_buffer_degrees: f32,
    /// The least that buffer is, world units off the line, however near the thing is, so a
    /// ring at your feet is as easy to catch as a chest further off. Our own tuning (6 cm).
    pub aim_min_radius: f32,
    /// How strongly a legendary lying on the floor shimmers: the peak of its glow pulse as a
    /// fraction of the legendary colour, its glints brighter (`render::Shimmer`). Self-light
    /// only, so it shows in the dark and lights nothing. The user's call ("a faint shimmer");
    /// the value is our own tuning against captures. 0 turns it off.
    pub legendary_shimmer: f32,
    /// How much larger than its own size a container standing in the level (`crate::props`)
    /// is drawn, about where it stands: the user's call ("containers look small"). Its look
    /// bounds and the floor it blocks grow with it. The level lays containers out for
    /// `fb_level::CONTAINER_SCALE`, which this ships at.
    pub container_scale: f32,
}

impl Default for ViewTuning {
    fn default() -> Self {
        ViewTuning {
            held_scale: 1.4,
            held_fov_degrees: 46.0,
            world_fov_degrees: 70.0,
            head_bob: 1.0,
            floor_item_scale: 1.15,
            aim_buffer_degrees: 2.0,
            aim_min_radius: 0.6,
            legendary_shimmer: 0.25,
            container_scale: fb_level::CONTAINER_SCALE,
        }
    }
}

/// How the delver's body moves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MovingTuning {
    /// Walking pace, world units per second (1.4 m/s).
    pub walk_speed: f32,
    /// Hurrying pace with Shift held, world units per second: a run of 4.5 m/s (our own).
    pub hurry_speed: f32,
    /// How quickly velocity closes on the wanted velocity, per second (`StableInterpolate`).
    pub acceleration: f32,
    /// The body's radius on the floor, world units (35 cm).
    pub body_radius: f32,
    /// The share of its pace a walker keeps in a floor web in a spider's nest, other than a
    /// spider (`fb_level::webs::WEB_PACE`, our own).
    pub web_pace: f32,
}

impl Default for MovingTuning {
    fn default() -> Self {
        MovingTuning { walk_speed: 14.0, hurry_speed: 45.0, acceleration: 10.0, body_radius: 3.5, web_pace: fb_level::webs::WEB_PACE }
    }
}

/// How creatures are drawn and behave. Their shapes and stat blocks come from their seeds
/// (`fb_creatures`); these only scale how the game draws and moves them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CreatureTuning {
    /// How far creatures notice the player, as a multiple of their stat block's senses (at
    /// least 20 ft; `crate::creatures::behaviour::notice_range`).
    pub sense_scale: f32,
    /// Seconds between a rat's bites, and between a snake's strikes. Our own: the SRD's round
    /// is 6 seconds, and a bite every one and a half to two and a half keeps a real-time fight
    /// moving.
    pub rat_cooldown: f32,
    pub snake_cooldown: f32,
    /// How fast a creature comes on once it has seen the player, as a multiple of its speed:
    /// rats at twice it, as the SRD's Dash action gives
    /// ([SRD 5.1 p. 93](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=93)),
    /// snakes at their speed.
    pub rat_chase_speed: f32,
    pub snake_chase_speed: f32,
    /// How large each kind of creature is drawn, and the floor it covers.
    pub size: CreatureSizes,
}

impl Default for CreatureTuning {
    fn default() -> Self {
        CreatureTuning { sense_scale: 1.0, rat_cooldown: 1.5, snake_cooldown: 2.5, rat_chase_speed: 2.0, snake_chase_speed: 1.0, size: CreatureSizes::default() }
    }
}

/// How folk go about the level (`crate::folk_routine`). Where each goes is rolled from its
/// seed; these only set how far, how fast and how long they wait.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FolkTuning {
    /// An unhurried walking pace for a human, world units per second (1.1 m/s, a stroll under
    /// the delver's 1.4). Shorter folk go slower, in proportion to their height. Our own.
    pub walk_speed: f32,
    /// The least and most seconds folk wait between walks. Our own.
    pub pause_min: f32,
    pub pause_max: f32,
    /// How far from its spot a person strolls, world units (9 m). Our own.
    pub wander_radius: f32,
    /// How near the player must come, in sight, for a person to stop and face them, world units
    /// (2.8 m). Our own.
    pub notice_distance: f32,
    /// How far the player must go before a person who faced them carries on, world units (4 m;
    /// beyond the notice distance so they do not flicker). Our own.
    pub release_distance: f32,
    /// Seconds the player must stay away before a person carries on. Our own.
    pub release_delay: f32,
    /// Seconds a hostile folk goes without sight of the delver before giving up the fight and
    /// going back to their routine, friendly again (`crate::folk_combat::fight`). Our own: long
    /// enough to search a room or two for someone who ducked round a corner, short enough that
    /// slipping away works.
    pub lose_interest_after: f32,
}

impl Default for FolkTuning {
    fn default() -> Self {
        FolkTuning { walk_speed: 14.0, pause_min: 3.0, pause_max: 8.0, wander_radius: 90.0, notice_distance: 28.0, release_distance: 40.0, release_delay: 1.0, lose_interest_after: 25.0 }
    }
}

/// A size multiplier for each species, 1 as rolled.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CreatureSizes {
    pub rat: f32,
    pub giant_rat: f32,
    /// The poisonous snake.
    pub viper: f32,
    /// The constrictor snake.
    pub constrictor: f32,
    /// The skeleton.
    pub skeleton: f32,
    /// The giant spider.
    pub giant_spider: f32,
    /// The troll.
    pub troll: f32,
    /// The jackalope.
    pub jackalope: f32,
    /// The giant centipede.
    pub giant_centipede: f32,
    pub bat: f32,
    pub giant_bat: f32,
}

impl Default for CreatureSizes {
    fn default() -> Self {
        CreatureSizes { rat: 1.0, giant_rat: 1.0, viper: 1.0, constrictor: 1.0, skeleton: 1.0, giant_spider: 1.0, troll: 1.0, jackalope: 1.0, giant_centipede: 1.0, bat: 1.0, giant_bat: 1.0 }
    }
}

impl CreatureSizes {
    /// The multiplier for `species`.
    pub fn of(&self, species: Species) -> f32 {
        match species {
            Species::Rat => self.rat,
            Species::GiantRat => self.giant_rat,
            Species::PoisonousSnake => self.viper,
            Species::ConstrictorSnake => self.constrictor,
            Species::Skeleton => self.skeleton,
            Species::GiantSpider => self.giant_spider,
            Species::Troll => self.troll,
            Species::Jackalope => self.jackalope,
            Species::GiantCentipede => self.giant_centipede,
            Species::Bat => self.bat,
            Species::GiantBat => self.giant_bat,
        }
    }
}

/// How blows land and show.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CombatTuning {
    /// How far a blow reaches, as a multiple of the weapon's own reach (the arm and the
    /// weapon's length, `crate::combat::Weapon`).
    pub reach_scale: f32,
    /// Half the angle of the cone ahead a blow can land in, degrees either side of the
    /// crosshair. Our own: wide enough for a creature at your feet, which is below the view.
    pub hit_cone_degrees: f32,
    /// How long a struck creature's flash holds at full, then fades out, seconds. A felled
    /// creature stands for the hold before it collapses. Our own.
    pub flash_hold: f32,
    pub flash_fade: f32,
    /// How far a blow knocks a creature back, as a multiple of the usual (a rat a third of a
    /// metre, a constrictor hardly at all; `crate::combat::feedback::knockback`).
    pub knockback_scale: f32,
    /// How much the camera kicks when hit, as a multiple of the usual. 0 turns it off. Our own.
    pub hit_kick: f32,
}

impl Default for CombatTuning {
    fn default() -> Self {
        CombatTuning { reach_scale: 1.0, hit_cone_degrees: 50.0, flash_hold: 0.12, flash_fade: 0.2, knockback_scale: 1.0, hit_kick: 1.0 }
    }
}

impl CombatTuning {
    /// The cone's half angle, radians.
    pub fn hit_cone(&self) -> f32 {
        self.hit_cone_degrees.to_radians()
    }

    /// How long a flash lasts in all, seconds.
    pub fn flash_time(&self) -> f32 {
        self.flash_hold.max(0.0) + self.flash_fade.max(0.0)
    }
}

/// How bright the level's lights burn and how far they reach. Peaks are multiples of full light
/// close up (`crate::light::peak_lumens`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LightTuning {
    /// A lit torch, lantern or other burning thing, in hand or on the floor: its strength close
    /// up. Over 1 widens the fully lit core; the shader caps it at full.
    pub torch_peak: f32,
    /// How far its light reaches, as a multiple of its bright radius (its `LightAnchor`
    /// radius): dim light runs as far again beyond the bright, as a torch's does in the D&D 5e
    /// SRD 5.1 ("Adventuring Gear": bright light in a 20-foot radius and dim light for an
    /// additional 20 feet).
    pub torch_reach: f32,
    /// A brazier: brighter and further than a torch in hand. Reach in world units.
    pub brazier_peak: f32,
    pub brazier_reach: f32,
    /// Glowcaps: a faint cold pool of about three metres. Reach in world units.
    pub glowcap_peak: f32,
    pub glowcap_reach: f32,
    /// A crystal that lights its cavern, in its own colour: strength close up, and the reach of a
    /// crystal 3 m tall (a taller or shorter one reaches a little further or less). They cast no
    /// shadows, so they take none of the shadow budget (`shadowed_lights`).
    pub crystal_peak: f32,
    pub crystal_reach: f32,
    /// How deeply flames flicker, as a multiple of each flame's own (0 holds them steady, 1 the
    /// prototype's curve, `crate::light::Torch::flicker`).
    pub flicker_depth: f32,
    /// The abyss's own light where no flame reaches, lux.
    pub ambient_lux: f32,
    /// Where the dark starts to swallow far stone, and where it is complete, world units.
    pub fog_start: f32,
    pub fog_end: f32,
    /// How many of the level's lights nearest the camera cast shadows at once; the rest shine
    /// without (`crate::light::ShadowBudgetPlugin`). The player's own lights (a torch in hand,
    /// a lantern at the hip, a lit thing underfoot) always cast and are not counted. Each shadowed point light draws the level six
    /// times a frame.
    pub shadowed_lights: usize,
    /// Width and height of each face of a point light's shadow cube, in texels. Banded light
    /// hides the soft edge a smaller map gives, and each step down quarters the fill.
    pub shadow_map_size: usize,
}

impl Default for LightTuning {
    fn default() -> Self {
        LightTuning {
            torch_peak: 1.0,
            torch_reach: 2.0,
            brazier_peak: 1.5,
            brazier_reach: 150.0,
            glowcap_peak: 0.5,
            glowcap_reach: 30.0,
            crystal_peak: 0.3,
            crystal_reach: 55.0,
            flicker_depth: 1.0,
            ambient_lux: 4.0,
            fog_start: 40.0,
            fog_end: 260.0,
            shadowed_lights: 4,
            shadow_map_size: 512,
        }
    }
}

/// How the game's sound is mixed, before the player's own volumes (`crate::settings`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundTuning {
    /// The music's linear volume at full, 0 to 1.
    pub music_mix: f32,
    /// The silence between tracks, drawn evenly between these, seconds.
    pub silence_min: f32,
    pub silence_max: f32,
    /// Sound effects' linear volume at full, 0 to 1.
    pub effects_volume: f32,
    /// Ambient sounds' linear volume at full, 0 to 1: drips and rumbles.
    pub ambience_volume: f32,
    /// The shortest time between stone rumbles, seconds.
    pub rumble_interval_min: f32,
    /// The longest time between stone rumbles, seconds.
    pub rumble_interval_max: f32,
    /// How many rooms off a troll is heard in the music: the troll theme loops while one is
    /// this many rooms away or fewer.
    pub troll_rooms: u32,
    /// How long the troll theme plays on after the last troll is out of range, seconds.
    pub troll_linger: f32,
}

impl Default for SoundTuning {
    fn default() -> Self {
        SoundTuning {
            music_mix: 0.35,
            silence_min: 45.0,
            silence_max: 75.0,
            effects_volume: 1.0,
            ambience_volume: 0.3,
            rumble_interval_min: 60.0,
            rumble_interval_max: 180.0,
            troll_rooms: 2,
            troll_linger: 10.0,
        }
    }
}

/// How the interface is laid out: every screen is drawn for a 1920 by 1080 view and scaled
/// to fit the one it is in ([`InterfaceTuning::scale`]), and the inventory's cells are sized
/// from there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InterfaceTuning {
    /// Logical pixels from one grid line to the next (the plate's `--cs`) in a view
    /// [`InterfaceTuning::REFERENCE_HEIGHT`] lines high. The whole interface scales with the
    /// view from there ([`InterfaceTuning::scale`]).
    pub cell_size: f32,
    /// The fewest logical pixels a cell is drawn at, however small the window: the floor
    /// under the whole interface's scale, so text stays readable.
    pub min_cell_size: f32,
    /// How long the mouse rests on an item before its card shows, seconds.
    pub hover_delay: f32,
}

impl Default for InterfaceTuning {
    fn default() -> Self {
        InterfaceTuning { cell_size: 72.0, min_cell_size: 36.0, hover_delay: 0.0 }
    }
}

impl InterfaceTuning {
    /// The view, in logical pixels, every screen is laid out for and drawn at its own size in.
    pub const REFERENCE_HEIGHT: f32 = 1080.0;
    pub const REFERENCE_WIDTH: f32 = 1920.0;

    /// How much the interface is scaled (Bevy's `UiScale`) in a view `view` logical pixels
    /// across: by the height or the width over the reference's, whichever is tighter, so each
    /// screen keeps one layout and fits at any size and shape, but never so far down that a
    /// cell is drawn under `min_cell_size`. Our own.
    pub fn scale(&self, view: Vec2) -> f32 {
        let fit = (view.y / Self::REFERENCE_HEIGHT).min(view.x / Self::REFERENCE_WIDTH);
        fit.max(self.min_cell_size / self.cell_size.max(1.0))
    }

    /// The logical pixels a cell is drawn at in a view `view` logical pixels across.
    pub fn cell_px(&self, view: Vec2) -> f32 {
        self.cell_size * self.scale(view)
    }
}

/// How the keys and buttons are read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputTuning {
    /// How long a key or button is held before a Hold binding fires, seconds. A press let go
    /// sooner is a tap.
    pub hold_time: f32,
}

impl Default for InputTuning {
    fn default() -> Self {
        InputTuning { hold_time: 0.25 }
    }
}

/// How game time advances.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockTuning {
    /// How many game seconds pass per real second: 12 means 1 game hour takes 300 real seconds
    /// (5 minutes). The clock only advances while delving and not paused.
    pub game_seconds_per_second: f32,
}

impl Default for ClockTuning {
    fn default() -> Self {
        ClockTuning { game_seconds_per_second: 12.0 }
    }
}

/// How the delver mends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecoveryTuning {
    /// Game minutes a fed and watered delver takes to regain one hit point
    /// ([`fb_character::recovery`]). Our own.
    pub minutes_per_hit_point: f32,
    /// How much faster each point of recovery from gear mends, as a fraction of the base rate
    /// ([`fb_character::recovery::minutes_per_hit_point`]). Our own.
    pub faster_per_point: f32,
}

impl RecoveryTuning {
    /// Game minutes per hit point for a delver with `points` of recovery from their gear.
    pub fn minutes_for(&self, points: f64) -> f64 {
        fb_character::recovery::minutes_per_hit_point(self.minutes_per_hit_point as f64, self.faster_per_point as f64, points)
    }
}

impl Default for RecoveryTuning {
    fn default() -> Self {
        RecoveryTuning {
            minutes_per_hit_point: fb_character::recovery::MINUTES_PER_HIT_POINT as f32,
            faster_per_point: fb_character::recovery::FASTER_PER_POINT as f32,
        }
    }
}

impl Tuning {
    /// The tuning `text` gives, and a warning for each key that kept its default: missing,
    /// of the wrong kind, or not a key at all. Text that is not TOML gives every default.
    /// A whole number is read as a decimal where one is wanted.
    pub fn from_toml(text: &str) -> (Tuning, Vec<String>) {
        let mut warnings = Vec::new();
        let file = match text.parse::<toml::Table>() {
            Ok(table) => table,
            Err(error) => {
                let line = error.span().map_or(1, |span| text[..span.start.min(text.len())].matches('\n').count() + 1);
                warnings.push(format!("not valid TOML at line {line} ({}), so every value is its default", error.message().trim()));
                return (Tuning::default(), warnings);
            }
        };
        let defaults = Tuning::default().table();
        let merged = merge(defaults, file, "", &mut warnings);
        match toml::Value::Table(merged).try_into() {
            Ok(tuning) => (tuning, warnings),
            Err(error) => {
                warnings.push(format!("could not be read, so every value is its default: {error}"));
                (Tuning::default(), warnings)
            }
        }
    }

    /// The tuning as a TOML table.
    pub fn table(&self) -> toml::Table {
        toml::Table::try_from(self).expect("tuning is plain numbers")
    }

    /// Every key, dotted from its table (`view.held_scale`, `creatures.size.rat`), in the
    /// order of the file's tables.
    pub fn keys() -> Vec<String> {
        fn walk(table: &toml::Table, prefix: &str, out: &mut Vec<String>) {
            for (key, value) in table {
                let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
                match value {
                    toml::Value::Table(inner) => walk(inner, &path, out),
                    _ => out.push(path),
                }
            }
        }
        let mut keys = Vec::new();
        walk(&Tuning::default().table(), "", &mut keys);
        keys
    }
}

/// `file` laid over `defaults`, key by key: a value of the default's kind replaces it, and every
/// other case keeps the default with a warning naming the key.
fn merge(defaults: toml::Table, mut file: toml::Table, prefix: &str, warnings: &mut Vec<String>) -> toml::Table {
    let mut merged = toml::Table::new();
    for (key, default) in defaults {
        let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
        let value = match (default, file.remove(&key)) {
            (default, None) => {
                warnings.push(format!("`{path}` is missing, so it is {}", shown(&default)));
                default
            }
            (toml::Value::Table(inner), Some(toml::Value::Table(given))) => toml::Value::Table(merge(inner, given, &path, warnings)),
            (toml::Value::Float(_), Some(toml::Value::Integer(n))) => toml::Value::Float(n as f64),
            (default, Some(given)) if std::mem::discriminant(&default) == std::mem::discriminant(&given) => given,
            (default, Some(given)) => {
                warnings.push(format!("`{path}` should be {}, not {}, so it is {}", kind(&default), shown(&given), shown(&default)));
                default
            }
        };
        merged.insert(key, value);
    }
    for key in file.keys() {
        let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
        warnings.push(format!("`{path}` is not a tuning key, so it does nothing"));
    }
    merged
}

fn kind(value: &toml::Value) -> &'static str {
    match value {
        toml::Value::Float(_) | toml::Value::Integer(_) => "a number",
        toml::Value::Table(_) => "a table",
        toml::Value::Boolean(_) => "true or false",
        toml::Value::String(_) => "text",
        _ => "a value",
    }
}

/// A value as the file would write it; decimals as the `f32` they are read into.
fn shown(value: &toml::Value) -> String {
    match value {
        toml::Value::Float(f) => format!("{}", *f as f32),
        toml::Value::Table(_) => "its table of defaults".to_string(),
        other => other.to_string(),
    }
}

/// Reads `tuning.toml` into a [`Tuning`], warning about each key it could not use and never
/// failing on what the file says.
#[derive(Default, TypePath)]
pub struct TuningLoader;

impl AssetLoader for TuningLoader {
    type Asset = Tuning;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(&self, reader: &mut dyn Reader, _settings: &(), load_context: &mut LoadContext<'_>) -> Result<Tuning, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let (tuning, warnings) = Tuning::from_toml(&String::from_utf8_lossy(&bytes));
        let path = load_context.path().to_string();
        for warning in warnings {
            warn!("{path}: {warning}");
        }
        Ok(tuning)
    }

    fn extensions(&self) -> &[&str] {
        &["toml"]
    }
}

/// The `AssetPlugin::file_path` to watch: `assets`, made absolute when `BEVY_ASSET_ROOT` is a
/// relative path. Bevy 0.20's file watcher resolves a relative root twice and panics
/// ("Failed to create file watcher from path \"game/assets\" ... paths: [\"game/game/assets\"]",
/// `bevy_asset-0.20.0/src/io/source.rs`, `get_default_watcher`), which a relative
/// `BEVY_ASSET_ROOT=game` set off. An absolute `file_path` replaces the root when joined, so
/// both loading and watching see the same folder.
pub fn asset_file_path() -> String {
    match std::env::var_os("BEVY_ASSET_ROOT").map(std::path::PathBuf::from) {
        Some(root) if root.is_relative() => std::path::absolute(root.join("assets")).map_or_else(|_| "assets".to_string(), |p| p.to_string_lossy().into_owned()),
        _ => "assets".to_string(),
    }
}

/// Where [`Tuning`] takes the file's values, in `PreUpdate`: systems that copy it elsewhere
/// before `Update` run after it.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TuningSet;

/// The loaded tuning file, kept so it stays loaded and is reloaded when it changes.
#[derive(Resource, Debug)]
pub struct TuningFile(pub Handle<Tuning>);

/// Loads [`TUNING_PATH`] and keeps [`Tuning`] to what it says. Without an asset server (in
/// tests) the resource keeps its defaults.
pub struct TuningPlugin;

impl Plugin for TuningPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tuning>();
        if app.world().contains_resource::<AssetServer>() {
            app.init_asset::<Tuning>()
                .init_asset_loader::<TuningLoader>()
                .add_systems(Startup, load_tuning.run_if(not(resource_exists::<TuningFile>)))
                .add_systems(PreUpdate, apply_tuning_file.in_set(TuningSet));
        }
    }
}

fn load_tuning(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(TuningFile(assets.load(TUNING_PATH)));
}

/// Copies the file into [`Tuning`] when it loads and each time it is saved.
fn apply_tuning_file(mut events: MessageReader<AssetEvent<Tuning>>, file: Option<Res<TuningFile>>, files: Res<Assets<Tuning>>, mut tuning: ResMut<Tuning>) {
    let Some(file) = file else { return };
    for event in events.read() {
        let (AssetEvent::Added { id } | AssetEvent::Modified { id }) = *event else { continue };
        if id != file.0.id() {
            continue;
        }
        if let Some(loaded) = files.get(id) {
            if *tuning != *loaded {
                info!("tuning: applied {TUNING_PATH}");
            }
            tuning.set_if_neq(loaded.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> String {
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(TUNING_PATH)).unwrap()
    }

    #[test]
    fn the_shipped_file_is_every_default() {
        let (tuning, warnings) = Tuning::from_toml(&shipped());
        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(tuning, Tuning::default());
    }

    #[test]
    fn every_key_in_the_shipped_file_says_what_it_does() {
        let text = shipped();
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            assert!(i > 0 && lines[i - 1].trim().starts_with('#'), "no comment above `{line}`");
        }
    }

    #[test]
    fn the_readme_lists_every_key() {
        let readme = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("README.md")).unwrap();
        let section = readme.split("### Tuning").nth(1).expect("a Tuning section in the README");
        for key in Tuning::keys() {
            let leaf = key.rsplit('.').next().unwrap();
            assert!(section.contains(&format!("`{leaf}`")), "the README's Tuning section does not list `{key}`");
        }
    }

    #[test]
    fn a_bad_key_keeps_its_default_and_says_so() {
        let text = "[view]\nheld_scale = \"big\"\nfloor_item_scale = 2\n[moving]\nwalk_speed = 20.0\nwalkspeed = 3.0\n";
        let (tuning, warnings) = Tuning::from_toml(text);
        assert_eq!(tuning.view.held_scale, ViewTuning::default().held_scale, "the wrong kind keeps the default");
        assert_eq!(tuning.view.floor_item_scale, 2.0, "a whole number reads as a decimal");
        assert_eq!(tuning.moving.walk_speed, 20.0);
        assert_eq!(tuning.light, LightTuning::default(), "a missing table is all defaults");
        let about = |key: &str| warnings.iter().filter(|w| w.contains(&format!("`{key}`"))).count();
        assert_eq!(about("view.held_scale"), 1, "{warnings:#?}");
        assert!(warnings.iter().any(|w| w.contains("`view.held_scale` should be a number, not \"big\", so it is 1.4")), "{warnings:#?}");
        assert_eq!(about("moving.walkspeed"), 1, "an unknown key is named");
        assert_eq!(about("moving.hurry_speed"), 1, "a missing key is named");
        assert_eq!(about("light"), 1, "a missing table is named once");
        assert_eq!(about("view.floor_item_scale") + about("moving.walk_speed"), 0, "good keys say nothing");
    }

    #[test]
    fn a_file_that_is_not_toml_gives_every_default() {
        let (tuning, warnings) = Tuning::from_toml("[view\nheld_scale = = 3");
        assert_eq!(tuning, Tuning::default());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("not valid TOML"), "{warnings:?}");
    }

    #[test]
    fn a_table_given_as_a_value_keeps_its_defaults() {
        let (tuning, warnings) = Tuning::from_toml(&shipped().replace("[interface]", "creatures = 3\n[interface]"));
        // The shipped file already has a [creatures] table, so a second `creatures` is not TOML.
        assert_eq!(tuning, Tuning::default());
        assert!(!warnings.is_empty());
        let (tuning, warnings) = Tuning::from_toml("[creatures]\nsize = 2\n");
        assert_eq!(tuning.creatures.size, CreatureSizes::default());
        assert!(warnings.iter().any(|w| w.contains("`creatures.size` should be a table")), "{warnings:#?}");
    }

    #[test]
    fn every_species_has_a_size() {
        let sizes = CreatureSizes { rat: 1.0, giant_rat: 2.0, viper: 3.0, constrictor: 4.0, skeleton: 5.0, giant_spider: 6.0, troll: 7.0, jackalope: 8.0, giant_centipede: 9.0, bat: 10.0, giant_bat: 11.0 };
        // Every species, so one added without a size fails here.
        let all = Species::ALL;
        assert_eq!(all.map(|s| sizes.of(s)).to_vec(), (1..=all.len()).map(|n| n as f32).collect::<Vec<_>>());
    }

    #[test]
    fn saving_the_file_changes_the_resource() {
        use bevy::asset::AssetPlugin;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default())).add_plugins(TuningPlugin);
        let mut files = app.world_mut().resource_mut::<Assets<Tuning>>();
        let mut faster = Tuning::default();
        faster.moving.walk_speed = 30.0;
        let handle = files.add(Tuning::default());
        app.insert_resource(TuningFile(handle.clone()));
        app.update();
        assert_eq!(*app.world().resource::<Tuning>(), Tuning::default());
        // As a reload does: the asset's value is replaced, and an event says so.
        let _ = app.world_mut().resource_mut::<Assets<Tuning>>().insert(&handle, faster.clone());
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Tuning>().moving.walk_speed, 30.0);
    }
}
