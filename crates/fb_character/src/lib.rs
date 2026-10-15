//! Forever Below character attributes: five core stats and their modifiers for gameplay, game time,
//! and simulation meters (hunger, thirst, sleepiness).
//!
//! This crate provides the character's five D&D 5e attributes (Strength, Dexterity, Constitution,
//! Intelligence, Wisdom) and the ability modifier formula. Each attribute starts from a folk's
//! baseline in `data/races.json` and is nudged by a pseudo-random distribution and clamped to 3–18.
//!
//! `GameTime` measures elapsed game seconds. The game clock runs at a configurable ratio to real time
//! (e.g., 12× real time means 1 game hour passes every 300 real seconds), and pauses while screens
//! are open over the level.
//!
//! `Meter` tracks hunger, thirst, and sleepiness from 0 (best) to 1 (worst), advancing each turn by
//! a base rate scaled by activity and gear modifiers. `Character` combines these with activity state.

pub mod explain;

use serde::{Deserialize, Serialize};

/// The five core attributes of a character (D&D 5e SRD 5.1, p. 76).
///
/// Each ability score ranges from 3 (incapacitated) to 18 (superhuman). A score of 10 or 11
/// represents human average and gives a modifier of 0.
///
/// The attributes are in order: strength, dexterity, constitution, intelligence, wisdom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attributes {
    /// Strength (STR): physical power, carrying capacity, melee damage.
    pub strength: i32,
    /// Dexterity (DEX): coordination and reflexes; affects Armour Class and initiative.
    pub dexterity: i32,
    /// Constitution (CON): endurance and health; affects hit points and saving throws.
    pub constitution: i32,
    /// Intelligence (INT): reasoning and memory; affects skills and magical abilities.
    pub intelligence: i32,
    /// Wisdom (WIS): awareness and instinct; affects perception and saving throws.
    pub wisdom: i32,
}

/// Game time in seconds. The clock starts at zero for each delve and ticks at a ratio
/// determined by the tuning file's `clock.game_seconds_per_second` setting.
/// At the default ratio of 12, 1 real second is 12 game seconds; 300 real seconds is 1 game hour.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct GameTime {
    /// Elapsed game seconds.
    pub game_seconds: f64,
}

impl GameTime {
    /// Creates a game time from a given number of game seconds.
    pub const fn from_seconds(game_seconds: f64) -> Self {
        GameTime { game_seconds }
    }

    /// Returns the elapsed game time in seconds.
    pub const fn seconds(&self) -> f64 {
        self.game_seconds
    }

    /// Returns the elapsed game time in minutes (60 game seconds per minute).
    pub fn minutes(&self) -> f64 {
        self.game_seconds / 60.0
    }

    /// Returns the elapsed game time in hours (3600 game seconds per hour).
    pub fn hours(&self) -> f64 {
        self.game_seconds / 3600.0
    }

    /// Returns the elapsed game time in days (86400 game seconds per day).
    pub fn days(&self) -> f64 {
        self.game_seconds / 86400.0
    }

    /// Advances the game time by the given number of game seconds.
    pub fn advance(&mut self, delta_seconds: f64) {
        self.game_seconds += delta_seconds;
    }
}

/// Current activity affecting simulation rates: resting, walking, exerting, or sleeping.
/// Hunger and thirst have base rates, activity multipliers (0.5 for resting, 1 for walking,
/// 2 for exerting), and gear multipliers. Sleeping has special effects: hunger and thirst
/// rates are halved, and sleepiness decreases by 1/8 per hour.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Activity {
    Resting,
    Walking,
    Exerting,
    Sleeping,
}

impl Activity {
    /// Multiplier on meter rates for this activity.
    pub fn multiplier(self) -> f32 {
        match self {
            Activity::Resting => 0.5,
            Activity::Walking => 1.0,
            Activity::Exerting => 2.0,
            Activity::Sleeping => 1.0,
        }
    }
}

/// Load band based on carrying capacity percentage (SRD p. 79).
/// Light: under 34%. Burdened: 34% to under 67%. Encumbered: 67% to 100%. Overloaded: above 100%.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LoadBand {
    #[default]
    Light,
    Burdened,
    Encumbered,
    Overloaded,
}

impl LoadBand {
    /// Creates a load band from load and capacity (in pounds).
    pub fn from_load(load_lb: f64, capacity_lb: f64) -> Self {
        if capacity_lb <= 0.0 {
            return LoadBand::Overloaded;
        }
        let percent = load_lb / capacity_lb;
        if percent > 1.0 {
            LoadBand::Overloaded
        } else if percent >= 0.67 {
            LoadBand::Encumbered
        } else if percent >= 0.34 {
            LoadBand::Burdened
        } else {
            LoadBand::Light
        }
    }

    /// Walk speed multiplier for this load band.
    pub fn walk_multiplier(self) -> f32 {
        match self {
            LoadBand::Light => 1.0,
            LoadBand::Burdened => 0.85,
            LoadBand::Encumbered => 0.65,
            LoadBand::Overloaded => 0.0,
        }
    }

    /// HUD word for this load band.
    pub fn word(self) -> &'static str {
        match self {
            LoadBand::Light => "Light",
            LoadBand::Burdened => "Burdened",
            LoadBand::Encumbered => "Encumbered",
            LoadBand::Overloaded => "Overloaded",
        }
    }
}

/// Carrying capacity in pounds (SRD p. 79).
/// `capacity = STR × 15 + carry` where `carry` is gear-specific bonus capacity.
pub fn capacity_lb(strength: i32, carry_bonus_lb: f64) -> f64 {
    (strength as f64) * 15.0 + carry_bonus_lb
}

/// What one worn or held item does for its wearer, as plain numbers (an item's stats of the
/// same names). The default changes nothing.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Worn {
    /// Extra pounds the wearer can carry.
    pub carry: f64,
    /// Feet the wearer sees without light.
    pub darkvision: f64,
    /// Multiple of the normal hunger rate.
    pub hunger_rate: f64,
    /// Multiple of the normal sleepiness rate.
    pub sleepiness_rate: f64,
}

impl Default for Worn {
    fn default() -> Self {
        Worn { carry: 0.0, darkvision: 0.0, hunger_rate: 1.0, sleepiness_rate: 1.0 }
    }
}

/// A light the delver carries, as plain numbers.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CarriedLight {
    /// How far it reaches, in feet.
    pub reach: f64,
    /// How long it burns, as a multiple of its base's.
    pub burn_time: f64,
}

/// What the delver wears and carries, summed for the simulation (#226): the one place the
/// capacity, the meter rates, the darkvision and the light are worked out from gear.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gear {
    /// Extra pounds of capacity: the worn items' bonuses added.
    pub carry: f64,
    /// Feet seen without light: the best of the folk's and every item's, never the sum.
    pub darkvision: f64,
    /// Multiple of the hunger rate: the items' rates multiplied.
    pub hunger_rate: f64,
    /// Multiple of the sleepiness rate: the items' rates multiplied.
    pub sleepiness_rate: f64,
    /// Feet the carried light reaches; 0 with none.
    pub light: f64,
    /// How long the carried light burns, as a multiple of its base's.
    pub burn_time: f64,
}

impl Default for Gear {
    fn default() -> Self {
        Gear::new(0.0, [], [])
    }
}

impl Gear {
    /// The gear of a delver whose folk sees `folk_darkvision` feet in the dark, wearing `worn`
    /// and carrying `lights`. The light that reaches farthest is the carried one, and its
    /// burn time is the one that counts.
    pub fn new(folk_darkvision: f64, worn: impl IntoIterator<Item = Worn>, lights: impl IntoIterator<Item = CarriedLight>) -> Gear {
        let mut gear = Gear { carry: 0.0, darkvision: folk_darkvision, hunger_rate: 1.0, sleepiness_rate: 1.0, light: 0.0, burn_time: 1.0 };
        for item in worn {
            gear.carry += item.carry;
            gear.darkvision = gear.darkvision.max(item.darkvision);
            gear.hunger_rate *= item.hunger_rate;
            gear.sleepiness_rate *= item.sleepiness_rate;
        }
        for light in lights {
            if light.reach > gear.light {
                gear.light = light.reach;
                gear.burn_time = light.burn_time;
            }
        }
        gear
    }

    /// Carrying capacity in pounds for a delver of `strength` in this gear.
    pub fn capacity_lb(&self, strength: i32) -> f64 {
        capacity_lb(strength, self.carry)
    }

    /// Sets the hunger and sleepiness meters' gear multipliers to this gear's rates.
    pub fn apply_to(&self, needs: &mut Needs) {
        needs.hunger.gear = self.hunger_rate;
        needs.sleepiness.gear = self.sleepiness_rate;
    }
}

/// A meter from 0 (best) to 1 (worst), advancing by a base rate per game hour,
/// scaled by activity and gear multipliers. Clamped at 0 and 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meter {
    /// Current value from 0 to 1.
    pub value: f64,
    /// Base rate of change per game hour. Not serialized (rates come from code).
    pub rate_per_hour: f64,
    /// Gear multiplier on this meter's rate.
    pub gear: f64,
    /// The kind of meter, determining special rules (e.g., sleeping effects).
    pub kind: MeterKind,
}

impl Default for Meter {
    fn default() -> Self {
        Meter {
            value: 0.0,
            rate_per_hour: 1.0 / 16.0,
            gear: 1.0,
            kind: MeterKind::Hunger,
        }
    }
}

fn default_gear() -> f64 {
    1.0
}

impl Serialize for Meter {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Meter", 3)?;
        state.serialize_field("value", &self.value)?;
        state.serialize_field("kind", &self.kind)?;
        state.serialize_field("gear", &self.gear)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Meter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct MeterDe {
            #[serde(default)]
            value: f64,
            #[serde(default)]
            kind: MeterKind,
            #[serde(default = "default_gear")]
            gear: f64,
        }
        let m = MeterDe::deserialize(deserializer)?;
        Ok(Meter {
            value: m.value,
            rate_per_hour: m.kind.base_rate(),
            gear: m.gear,
            kind: m.kind,
        })
    }
}

impl Meter {
    /// Creates a meter at 0 with the given kind and base rate from the kind.
    pub const fn new(kind: MeterKind) -> Self {
        Meter {
            value: 0.0,
            rate_per_hour: 0.0,
            gear: 1.0,
            kind,
        }
    }

    /// Returns the band (0..=4) for this meter's current value.
    /// Bands progress at quarters: 0, 0.25, 0.5, 0.75, then max at 1.0.
    pub fn band(&self) -> usize {
        if self.value >= 1.0 {
            4
        } else {
            ((self.value * 4.0) as usize).min(4)
        }
    }

    /// Returns the HUD word for this meter at its current band.
    pub fn word(&self) -> &'static str {
        self.kind.word(self.band())
    }

    /// The meter's change per game hour at `activity`: the base rate scaled by activity and gear.
    /// Sleeping halves the rate of hunger and thirst, and sleepiness falls by 1/8 per hour instead.
    pub fn effective_rate(&self, activity: Activity) -> f64 {
        match (activity, self.kind) {
            (Activity::Sleeping, MeterKind::Hunger | MeterKind::Thirst) => self.rate_per_hour * 0.5 * self.gear,
            (Activity::Sleeping, MeterKind::Sleepiness) => -0.125,
            (_, _) => self.rate_per_hour * activity.multiplier() as f64 * self.gear,
        }
    }

    /// Advances the meter by `delta_hours` of game time at its [`Meter::effective_rate`].
    pub fn step(&mut self, delta_hours: f64, activity: Activity) {
        self.value = (self.value + self.effective_rate(activity) * delta_hours).clamp(0.0, 1.0);
    }

    /// Reduces the meter by the given amount (e.g., from eating or drinking). Clamped at 0.
    pub fn reduce(&mut self, by: f64) {
        self.value = (self.value - by).max(0.0);
    }
}

/// The kind of meter, used to apply special sleeping effects and hold base rates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MeterKind {
    #[default]
    Hunger,
    Thirst,
    Sleepiness,
}

impl MeterKind {
    /// The base rate of change per game hour for this meter.
    pub const fn base_rate(self) -> f64 {
        match self {
            MeterKind::Hunger => 1.0 / 16.0,
            MeterKind::Thirst => 1.0 / 10.0,
            MeterKind::Sleepiness => 1.0 / 32.0,
        }
    }

    /// The HUD word for this meter at the given band (0..=4).
    pub const fn word(self, band: usize) -> &'static str {
        match (self, band) {
            (MeterKind::Hunger, 0) => "Fed",
            (MeterKind::Hunger, 1) => "Peckish",
            (MeterKind::Hunger, 2) => "Hungry",
            (MeterKind::Hunger, 3) => "Famished",
            (MeterKind::Hunger, _) => "Starving",
            (MeterKind::Thirst, 0) => "Quenched",
            (MeterKind::Thirst, 1) => "Thirsty",
            (MeterKind::Thirst, 2) => "Parched",
            (MeterKind::Thirst, 3) => "Dry",
            (MeterKind::Thirst, _) => "Dehydrated",
            (MeterKind::Sleepiness, 0) => "Rested",
            (MeterKind::Sleepiness, 1) => "Tired",
            (MeterKind::Sleepiness, 2) => "Weary",
            (MeterKind::Sleepiness, 3) => "Drowsy",
            (MeterKind::Sleepiness, _) => "Spent",
        }
    }
}

/// Simulation state of a character: hunger, thirst, and sleepiness meters.
/// Activity is passed to `step()`, not stored, so it can change between updates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Needs {
    #[serde(default)]
    pub hunger: Meter,
    #[serde(default)]
    pub thirst: Meter,
    #[serde(default)]
    pub sleepiness: Meter,
}

impl Default for Needs {
    fn default() -> Self {
        let mut hunger = Meter::new(MeterKind::Hunger);
        hunger.rate_per_hour = MeterKind::Hunger.base_rate();
        let mut thirst = Meter::new(MeterKind::Thirst);
        thirst.rate_per_hour = MeterKind::Thirst.base_rate();
        let mut sleepiness = Meter::new(MeterKind::Sleepiness);
        sleepiness.rate_per_hour = MeterKind::Sleepiness.base_rate();

        Needs { hunger, thirst, sleepiness }
    }
}

impl Needs {
    /// Eats and drinks: takes `nutrition` off hunger and `water` off thirst, neither below 0.
    pub fn consume(&mut self, nutrition: f64, water: f64) {
        self.hunger.reduce(nutrition);
        self.thirst.reduce(water);
    }

    /// Advances all meters by `delta_hours` of game time, with the given activity.
    pub fn step(&mut self, delta_hours: f64, activity: Activity) {
        self.hunger.step(delta_hours, activity);
        self.thirst.step(delta_hours, activity);
        self.sleepiness.step(delta_hours, activity);
    }
}

/// Cumulative exhaustion from prolonged deprivation (D&D 5e SRD 5.1, p. 358).
/// Each level represents time spent in a meter's worst band.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exhaustion {
    /// Level 0–6, where 6 is death.
    pub level: u32,
}

impl Exhaustion {
    /// Creates a new exhaustion level.
    pub fn new(level: u32) -> Self {
        Exhaustion { level: level.min(6) }
    }

    /// Speed multiplier for this exhaustion level (SRD p. 358).
    /// Level 2 is half speed, level 5 is speed 0.
    pub fn speed_multiplier(self) -> f32 {
        match self.level {
            2..=4 => 0.5,
            5.. => 0.0,
            _ => 1.0,
        }
    }

    /// Returns true if this is death (level 6).
    pub fn is_dead(self) -> bool {
        self.level >= 6
    }

    /// Increases the exhaustion level by 1 (clamped at 6).
    pub fn increase(&mut self) {
        self.level = (self.level + 1).min(6);
    }

    /// Decreases the exhaustion level by 1 (clamped at 0).
    pub fn decrease(&mut self) {
        self.level = self.level.saturating_sub(1);
    }
}

/// Slack for float error when a time lands exactly on a threshold.
const THRESHOLD_SLACK: f64 = 1e-9;

/// Game hours of continuous sleep that remove one level.
const SLEEP_HOURS_TO_RECOVER: f64 = 8.0;

/// The meter value where Famished and Dry begin (band 3).
const FAMISHED_AT: f64 = 0.75;

/// Time one meter has spent in its worst band during the current stay, and how many levels
/// that stay has already added. Both start over when the meter leaves the worst band; the
/// levels it added stay on the [`Exhaustion`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorstBand {
    /// Game hours in the worst band since the meter entered it.
    pub hours: f64,
    /// Levels this stay has added to the exhaustion so far.
    pub levels: u32,
}

impl WorstBand {
    /// Counts the part of a step of `hours` that the meter spent at its worst, given its value
    /// `before` the step and its state after. Returns the levels newly earned: the first once
    /// `first_hours` have passed and one more every `every_hours`.
    fn advance(&mut self, before: f64, meter: &Meter, activity: Activity, hours: f64, first_hours: f64, every_hours: f64) -> u32 {
        self.hours += hours_at_worst(before, meter.effective_rate(activity), hours);
        let earned = if self.hours + THRESHOLD_SLACK >= first_hours {
            1 + ((self.hours + THRESHOLD_SLACK - first_hours) / every_hours).floor() as u32
        } else {
            0
        };
        let gained = earned.saturating_sub(self.levels);
        self.levels = earned;
        if meter.value < 1.0 {
            *self = WorstBand::default();
        }
        gained
    }
}

/// The hours of a step of `hours` that a meter spends at 1, given its value `before` the step
/// and its rate per hour. A meter that crosses 1 inside the step counts only from the crossing,
/// at `(1 - before) / rate` hours in; a falling meter never counts once it starts to fall.
fn hours_at_worst(before: f64, rate: f64, hours: f64) -> f64 {
    if before >= 1.0 {
        if rate >= 0.0 { hours } else { 0.0 }
    } else if rate > 0.0 {
        (hours - (1.0 - before) / rate).max(0.0)
    } else {
        0.0
    }
}

/// The hours of a step of `hours` before a rising meter reaches [`FAMISHED_AT`], given its value
/// `before` the step and its rate per hour.
fn hours_below_famished(before: f64, rate: f64, hours: f64) -> f64 {
    if before >= FAMISHED_AT {
        0.0
    } else if rate > 0.0 {
        hours.min((FAMISHED_AT - before) / rate)
    } else {
        hours
    }
}

/// What a character carries between steps to turn their needs into exhaustion: time in each
/// meter's worst band and the run of sleep so far. Step it with [`ExhaustionTimes::step`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExhaustionTimes {
    /// Starving: the first level after 3 + CON modifier days (at least 1), then one a day.
    pub starving: WorstBand,
    /// Dehydrated: the first level after a day, then one every 12 hours.
    pub dehydrated: WorstBand,
    /// Spent: the first level after a day, then one a day.
    pub spent: WorstBand,
    /// Continuous game hours slept while not Famished, Starving, Dry or Dehydrated.
    pub sleep_hours: f64,
    /// Whether this sleep has already removed its level.
    pub slept_recovered: bool,
}

impl ExhaustionTimes {
    /// Steps `needs` by `hours` of game time at `activity`, then adds the levels of exhaustion
    /// that time in the worst bands has earned and removes one if a night's sleep was
    /// completed. The result is the same for one big step or many small ones.
    pub fn step(&mut self, needs: &mut Needs, exhaustion: &mut Exhaustion, activity: Activity, hours: f64, con_modifier: i32) {
        let before = *needs;
        needs.step(hours, activity);

        let starving_days = (3 + con_modifier).max(1) as f64;
        let mut gained = self.starving.advance(before.hunger.value, &needs.hunger, activity, hours, starving_days * 24.0, 24.0);
        gained += self.dehydrated.advance(before.thirst.value, &needs.thirst, activity, hours, 24.0, 12.0);
        gained += self.spent.advance(before.sleepiness.value, &needs.sleepiness, activity, hours, 24.0, 24.0);
        exhaustion.level = (exhaustion.level + gained).min(6);

        self.sleep(&before, needs, exhaustion, activity, hours);
    }

    /// Tracks the run of sleep through the step and removes a level once it reaches 8 hours.
    fn sleep(&mut self, before: &Needs, needs: &Needs, exhaustion: &mut Exhaustion, activity: Activity, hours: f64) {
        if activity != Activity::Sleeping {
            self.sleep_hours = 0.0;
            self.slept_recovered = false;
            return;
        }
        let fed = hours_below_famished(before.hunger.value, needs.hunger.effective_rate(activity), hours);
        let watered = hours_below_famished(before.thirst.value, needs.thirst.effective_rate(activity), hours);
        let comfortable = fed.min(watered);
        self.sleep_hours += comfortable;
        if self.sleep_hours + THRESHOLD_SLACK >= SLEEP_HOURS_TO_RECOVER && !self.slept_recovered && !exhaustion.is_dead() {
            exhaustion.decrease();
            self.slept_recovered = true;
        }
        if comfortable < hours {
            // Famished, Starving, Dry or Dehydrated broke the run; the next sleep starts over.
            self.sleep_hours = 0.0;
            self.slept_recovered = false;
        }
    }
}

/// The ability modifier for a score: `floor((score - 10) / 2)` (D&D 5e SRD 5.1, p. 76).
///
/// A score of 3 gives -4, a score of 10 or 11 gives 0, and a score of 18 gives +4.
/// Uses `div_euclid` which floors rather than truncating towards zero.
pub fn modifier(score: i32) -> i32 {
    (score - 10).div_euclid(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_follow_the_srd() {
        assert_eq!(modifier(3), -4);
        assert_eq!(modifier(9), -1);
        assert_eq!(modifier(10), 0);
        assert_eq!(modifier(11), 0);
        assert_eq!(modifier(18), 4);
    }

    #[test]
    fn attributes_serde_round_trip() {
        let attrs = Attributes { strength: 15, dexterity: 12, constitution: 14, intelligence: 9, wisdom: 13 };
        let json = serde_json::to_string(&attrs).unwrap();
        let parsed: Attributes = serde_json::from_str(&json).unwrap();
        assert_eq!(attrs, parsed);
    }

    #[test]
    fn game_time_conversions_are_correct() {
        let time = GameTime::from_seconds(3661.0);
        assert_eq!(time.seconds(), 3661.0);
        assert!((time.minutes() - 61.01666).abs() < 0.001);
        assert!((time.hours() - 1.0169).abs() < 0.001);
        assert!((time.days() - 0.04235).abs() < 0.001);
    }

    #[test]
    fn game_time_advances() {
        let mut time = GameTime::from_seconds(100.0);
        time.advance(50.0);
        assert_eq!(time.seconds(), 150.0);
    }

    #[test]
    fn game_time_starts_at_zero() {
        let time = GameTime::default();
        assert_eq!(time.seconds(), 0.0);
    }

    #[test]
    fn game_time_serde_round_trip() {
        let time = GameTime::from_seconds(1234.5);
        let json = serde_json::to_string(&time).unwrap();
        let parsed: GameTime = serde_json::from_str(&json).unwrap();
        assert_eq!(time, parsed);
    }

    #[test]
    fn activity_multipliers_are_correct() {
        assert_eq!(Activity::Resting.multiplier(), 0.5);
        assert_eq!(Activity::Walking.multiplier(), 1.0);
        assert_eq!(Activity::Exerting.multiplier(), 2.0);
        assert_eq!(Activity::Sleeping.multiplier(), 1.0);
    }

    #[test]
    fn band_formula_is_correct() {
        let mut meter = Meter::new(MeterKind::Hunger);
        meter.rate_per_hour = 1.0 / 16.0;

        meter.value = 0.0;
        assert_eq!(meter.band(), 0);
        meter.value = 0.25;
        assert_eq!(meter.band(), 1);
        meter.value = 0.5;
        assert_eq!(meter.band(), 2);
        meter.value = 0.75;
        assert_eq!(meter.band(), 3);
        meter.value = 0.999;
        assert_eq!(meter.band(), 3);
        meter.value = 1.0;
        assert_eq!(meter.band(), 4);
    }

    #[test]
    fn hunger_words_match_bands() {
        let kind = MeterKind::Hunger;
        assert_eq!(kind.word(0), "Fed");
        assert_eq!(kind.word(1), "Peckish");
        assert_eq!(kind.word(2), "Hungry");
        assert_eq!(kind.word(3), "Famished");
        assert_eq!(kind.word(4), "Starving");
    }

    #[test]
    fn thirst_words_match_bands() {
        let kind = MeterKind::Thirst;
        assert_eq!(kind.word(0), "Quenched");
        assert_eq!(kind.word(1), "Thirsty");
        assert_eq!(kind.word(2), "Parched");
        assert_eq!(kind.word(3), "Dry");
        assert_eq!(kind.word(4), "Dehydrated");
    }

    #[test]
    fn sleepiness_words_match_bands() {
        let kind = MeterKind::Sleepiness;
        assert_eq!(kind.word(0), "Rested");
        assert_eq!(kind.word(1), "Tired");
        assert_eq!(kind.word(2), "Weary");
        assert_eq!(kind.word(3), "Drowsy");
        assert_eq!(kind.word(4), "Spent");
    }

    #[test]
    fn from_zero_walking_hunger_reads_hungry_at_eight_hours() {
        let mut meter = Meter::new(MeterKind::Hunger);
        meter.rate_per_hour = 1.0 / 16.0;
        meter.step(8.0, Activity::Walking);
        assert!((meter.value - 0.5).abs() < 1e-9, "expected 0.5, got {}", meter.value);
        assert_eq!(meter.word(), "Hungry");
    }

    #[test]
    fn exerting_reaches_hungry_in_four_hours() {
        let mut meter = Meter::new(MeterKind::Hunger);
        meter.rate_per_hour = 1.0 / 16.0;
        meter.step(4.0, Activity::Exerting);
        assert!((meter.value - 0.5).abs() < 1e-9, "expected 0.5, got {}", meter.value);
    }

    #[test]
    fn eight_hours_asleep_from_spent_gives_rested() {
        let mut meter = Meter::new(MeterKind::Sleepiness);
        meter.rate_per_hour = 1.0 / 32.0;
        meter.value = 1.0;
        meter.step(8.0, Activity::Sleeping);
        assert!(meter.value < 1e-9, "expected ~0, got {}", meter.value);
    }

    #[test]
    fn meter_clamps_at_zero_and_one() {
        // Walking hunger for 16 hours: 16 * (1/16) = 1.0, clamped at 1.0
        let mut meter = Meter::new(MeterKind::Hunger);
        meter.rate_per_hour = 1.0 / 16.0;
        meter.step(16.0, Activity::Walking);
        assert_eq!(meter.value, 1.0);

        // Sleeping sleepiness: -0.125/hour * 10 hours = -1.25, clamped at 0.0
        let mut meter = Meter::new(MeterKind::Sleepiness);
        meter.rate_per_hour = 1.0 / 32.0;
        meter.value = 0.5;
        meter.step(10.0, Activity::Sleeping);
        assert_eq!(meter.value, 0.0);
    }

    #[test]
    fn sleeping_halves_hunger_rate() {
        let mut meter_walk = Meter::new(MeterKind::Hunger);
        meter_walk.rate_per_hour = 1.0 / 16.0;
        meter_walk.step(8.0, Activity::Walking);

        let mut meter_sleep = Meter::new(MeterKind::Hunger);
        meter_sleep.rate_per_hour = 1.0 / 16.0;
        meter_sleep.step(8.0, Activity::Sleeping);

        assert!((meter_walk.value - 0.5).abs() < 1e-9);
        assert!((meter_sleep.value - 0.25).abs() < 1e-9, "sleeping halves rate, expected 0.25, got {}", meter_sleep.value);
    }

    #[test]
    fn sleeping_halves_thirst_rate() {
        let mut meter_walk = Meter::new(MeterKind::Thirst);
        meter_walk.rate_per_hour = 1.0 / 10.0;
        meter_walk.step(10.0, Activity::Walking);

        let mut meter_sleep = Meter::new(MeterKind::Thirst);
        meter_sleep.rate_per_hour = 1.0 / 10.0;
        meter_sleep.step(10.0, Activity::Sleeping);

        assert!((meter_walk.value - 1.0).abs() < 1e-9);
        assert!((meter_sleep.value - 0.5).abs() < 1e-9, "sleeping halves rate, expected 0.5, got {}", meter_sleep.value);
    }

    #[test]
    fn per_meter_gear_multiplier() {
        let mut meter_hunger = Meter::new(MeterKind::Hunger);
        meter_hunger.rate_per_hour = 1.0 / 16.0;
        meter_hunger.gear = 2.0;
        meter_hunger.step(8.0, Activity::Walking);

        let mut meter_thirst = Meter::new(MeterKind::Thirst);
        meter_thirst.rate_per_hour = 1.0 / 10.0;
        meter_thirst.gear = 1.0;
        meter_thirst.step(8.0, Activity::Walking);

        assert!((meter_hunger.value - 1.0).abs() < 1e-9, "hunger with 2× gear: expected 1.0, got {}", meter_hunger.value);
        assert!((meter_thirst.value - 0.8).abs() < 1e-9, "thirst with 1× gear: expected 0.8, got {}", meter_thirst.value);
    }

    #[test]
    fn thirst_from_0_95_walking_over_2h() {
        let mut meter = Meter::new(MeterKind::Thirst);
        meter.rate_per_hour = 1.0 / 10.0;
        meter.value = 0.95;
        meter.step(2.0, Activity::Walking);
        assert!((meter.value - 1.0).abs() < 1e-9, "expected 1.0, got {}", meter.value);
    }

    #[test]
    fn sleepiness_from_0_1_sleeping() {
        // Sleeping sleepiness: 0.1 - (0.125 * 2) = 0.1 - 0.25 = -0.15, clamped to 0.0
        let mut meter = Meter::new(MeterKind::Sleepiness);
        meter.rate_per_hour = 1.0 / 32.0;
        meter.value = 0.1;
        meter.step(2.0, Activity::Sleeping);
        assert_eq!(meter.value, 0.0);
    }

    #[test]
    fn exerting_walking_and_sleeping_over_2h_from_nonzero() {
        // Exerting hunger: 0.2 + (2 * 2 * 1/16) = 0.2 + 0.25 = 0.45
        let mut meter_exert = Meter::new(MeterKind::Hunger);
        meter_exert.rate_per_hour = 1.0 / 16.0;
        meter_exert.value = 0.2;
        meter_exert.step(2.0, Activity::Exerting);
        assert!((meter_exert.value - 0.45).abs() < 1e-4);

        // Walking hunger: 0.2 + (1 * 2 * 1/16) = 0.2 + 0.125 = 0.325
        let mut meter_walk = Meter::new(MeterKind::Hunger);
        meter_walk.rate_per_hour = 1.0 / 16.0;
        meter_walk.value = 0.2;
        meter_walk.step(2.0, Activity::Walking);
        assert!((meter_walk.value - 0.325).abs() < 1e-4);

        // Sleeping hunger (halved): 0.5 + (0.5 * 2 * 1/16) = 0.5 + 0.0625 = 0.5625
        let mut meter_sleep = Meter::new(MeterKind::Hunger);
        meter_sleep.rate_per_hour = 1.0 / 16.0;
        meter_sleep.value = 0.5;
        meter_sleep.step(2.0, Activity::Sleeping);
        assert!((meter_sleep.value - 0.5625).abs() < 1e-4);
    }

    #[test]
    fn one_hour_against_3600_one_second_steps() {
        let mut meter1 = Meter::new(MeterKind::Hunger);
        meter1.rate_per_hour = 1.0 / 16.0;
        meter1.step(1.0, Activity::Walking);

        let mut meter2 = Meter::new(MeterKind::Hunger);
        meter2.rate_per_hour = 1.0 / 16.0;
        for _ in 0..3600 {
            meter2.step(1.0 / 3600.0, Activity::Walking);
        }

        assert!((meter1.value - meter2.value).abs() < 1e-4, "1h={}, 3600×1s={}", meter1.value, meter2.value);
    }

    #[test]
    fn meter_reduce() {
        let mut meter = Meter::new(MeterKind::Hunger);
        meter.value = 0.8;
        meter.reduce(0.3);
        assert!((meter.value - 0.5).abs() < 1e-9);

        meter.reduce(0.6);
        assert_eq!(meter.value, 0.0);
    }

    #[test]
    fn needs_default_has_sensible_rates() {
        let needs = Needs::default();
        assert_eq!(needs.hunger.rate_per_hour, 1.0 / 16.0);
        assert_eq!(needs.thirst.rate_per_hour, 1.0 / 10.0);
        assert_eq!(needs.sleepiness.rate_per_hour, 1.0 / 32.0);
    }

    #[test]
    fn needs_step_with_activity() {
        let mut needs = Needs::default();
        needs.step(8.0, Activity::Walking);
        assert!((needs.hunger.value - 0.5).abs() < 1e-9);
        assert_eq!(needs.hunger.word(), "Hungry");
    }

    #[test]
    fn one_2_hour_step_matches_7200_one_second_steps() {
        let mut needs1 = Needs::default();
        needs1.step(2.0, Activity::Walking);

        let mut needs2 = Needs::default();
        for _ in 0..7200 {
            needs2.step(1.0 / 3600.0, Activity::Walking);
        }

        assert!((needs1.hunger.value - needs2.hunger.value).abs() < 1e-4, "hunger mismatch: 2h={}, 7200×1s={}", needs1.hunger.value, needs2.hunger.value);
        assert!((needs1.thirst.value - needs2.thirst.value).abs() < 1e-4, "thirst mismatch: 2h={}, 7200×1s={}", needs1.thirst.value, needs2.thirst.value);
        assert!((needs1.sleepiness.value - needs2.sleepiness.value).abs() < 1e-4, "sleepiness mismatch: 2h={}, 7200×1s={}", needs1.sleepiness.value, needs2.sleepiness.value);
    }

    #[test]
    fn needs_serde_round_trip() {
        let mut needs = Needs::default();
        needs.hunger.value = 0.3;
        needs.hunger.gear = 1.5;
        needs.thirst.value = 0.7;
        needs.sleepiness.value = 0.1;

        let json = serde_json::to_string(&needs).unwrap();
        let parsed: Needs = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.hunger.value, 0.3);
        assert_eq!(parsed.thirst.value, 0.7);
        assert_eq!(parsed.sleepiness.value, 0.1);
        assert_eq!(parsed.hunger.gear, 1.5, "gear should be preserved in serialization");
    }


    #[test]
    fn consuming_takes_off_hunger_and_thirst_and_stops_at_zero() {
        let mut needs = Needs::default();
        needs.hunger.value = 0.55;
        needs.thirst.value = 0.3;
        assert_eq!(needs.hunger.word(), "Hungry");
        needs.consume(0.4, 0.0);
        assert!((needs.hunger.value - 0.15).abs() < 1e-9, "{}", needs.hunger.value);
        assert_eq!(needs.thirst.value, 0.3, "no water in a loaf");
        needs.consume(0.0, 0.5);
        assert_eq!(needs.thirst.value, 0.0);
        let mut fed = Needs::default();
        fed.consume(0.4, 0.5);
        assert_eq!((fed.hunger.value, fed.thirst.value), (0.0, 0.0), "eating while Fed stays at 0");
    }

    /// Needs where only `kind` moves, starting at `value`; the others stay at 0.
    fn only(kind: MeterKind, value: f64) -> Needs {
        let mut needs = Needs::default();
        for meter in [&mut needs.hunger, &mut needs.thirst, &mut needs.sleepiness] {
            if meter.kind == kind {
                meter.value = value;
            } else {
                meter.gear = 0.0;
            }
        }
        needs
    }

    /// Steps `count` times by `hours` each, returning the level after each step.
    fn levels(needs: &mut Needs, times: &mut ExhaustionTimes, exh: &mut Exhaustion, activity: Activity, hours: f64, count: usize, con: i32) -> Vec<u32> {
        (0..count)
            .map(|_| {
                times.step(needs, exh, activity, hours, con);
                exh.level
            })
            .collect()
    }

    #[test]
    fn exhaustion_speed_multipliers() {
        let speeds: Vec<f32> = (0..=6).map(|level| Exhaustion::new(level).speed_multiplier()).collect();
        assert_eq!(speeds, [1.0, 1.0, 0.5, 0.5, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn exhaustion_is_dead_at_level_6() {
        assert!(!Exhaustion::new(5).is_dead());
        assert!(Exhaustion::new(6).is_dead());
        assert_eq!(Exhaustion::new(40).level, 6);
    }

    #[test]
    fn exhaustion_increase_and_decrease_clamp() {
        let mut exh = Exhaustion::default();
        for _ in 0..9 {
            exh.increase();
        }
        assert_eq!(exh.level, 6);
        for _ in 0..9 {
            exh.decrease();
        }
        assert_eq!(exh.level, 0);
    }

    #[test]
    fn a_con_13_dwarf_kept_starving_reaches_level_1_at_exactly_4_days() {
        let (mut needs, mut times, mut exh) = (only(MeterKind::Hunger, 1.0), ExhaustionTimes::default(), Exhaustion::default());
        let seen = levels(&mut needs, &mut times, &mut exh, Activity::Walking, 1.0, 120, 1);
        assert_eq!(seen[94], 0, "95 hours");
        assert_eq!(seen[95], 1, "96 hours is 4 days");
        assert_eq!(seen[118], 1, "119 hours");
        assert_eq!(seen[119], 2, "120 hours is 5 days");
    }

    #[test]
    fn a_con_3_character_still_waits_1_day_to_starve() {
        let (mut needs, mut times, mut exh) = (only(MeterKind::Hunger, 1.0), ExhaustionTimes::default(), Exhaustion::default());
        let seen = levels(&mut needs, &mut times, &mut exh, Activity::Walking, 1.0, 24, -4);
        assert_eq!(seen[22], 0);
        assert_eq!(seen[23], 1);
    }

    #[test]
    fn the_dehydrated_ladder_is_1_day_then_every_12_hours() {
        let (mut needs, mut times, mut exh) = (only(MeterKind::Thirst, 1.0), ExhaustionTimes::default(), Exhaustion::default());
        let seen = levels(&mut needs, &mut times, &mut exh, Activity::Walking, 1.0, 60, 0);
        let at = |hours: usize| seen[hours - 1];
        assert_eq!([at(23), at(24), at(35), at(36), at(47), at(48), at(60)], [0, 1, 1, 2, 2, 3, 4]);
    }

    #[test]
    fn the_spent_ladder_is_1_day_then_daily() {
        let (mut needs, mut times, mut exh) = (only(MeterKind::Sleepiness, 1.0), ExhaustionTimes::default(), Exhaustion::default());
        let seen = levels(&mut needs, &mut times, &mut exh, Activity::Walking, 1.0, 72, 0);
        let at = |hours: usize| seen[hours - 1];
        assert_eq!([at(23), at(24), at(36), at(47), at(48), at(72)], [0, 1, 1, 1, 2, 3]);
    }

    #[test]
    fn one_big_step_equals_many_small_steps() {
        let (mut big_needs, mut big, mut big_exh) = (Needs::default(), ExhaustionTimes::default(), Exhaustion::default());
        big_needs.hunger.value = 1.0;
        big_needs.thirst.value = 1.0;
        big_needs.sleepiness.value = 1.0;
        let (mut small_needs, mut small, mut small_exh) = (big_needs, ExhaustionTimes::default(), Exhaustion::default());
        big.step(&mut big_needs, &mut big_exh, Activity::Walking, 100.0, 0);
        levels(&mut small_needs, &mut small, &mut small_exh, Activity::Walking, 1.0 / 60.0, 6000, 0);
        assert_eq!(big_exh, small_exh);
        assert_eq!(big.starving.levels, small.starving.levels);
        assert!((big.starving.hours - small.starving.hours).abs() < 1e-6);
        assert!((big.spent.hours - small.spent.hours).abs() < 1e-6);
    }

    #[test]
    fn crossing_inside_a_step_counts_from_the_crossing_to_1e_6() {
        // From 0.9, hunger needs 0.1 / rate hours to reach 1, so a 10 hour step spends the rest at 1.
        for (activity, gear, rate) in [(Activity::Exerting, 1.0, 1.0 / 8.0), (Activity::Resting, 1.0, 1.0 / 32.0), (Activity::Sleeping, 1.0, 1.0 / 32.0), (Activity::Walking, 2.0, 1.0 / 8.0)] {
            let mut needs = only(MeterKind::Hunger, 0.9);
            needs.hunger.gear = gear;
            let mut times = ExhaustionTimes::default();
            times.step(&mut needs, &mut Exhaustion::default(), activity, 10.0, 0);
            let expected = 10.0 - 0.1 / rate;
            assert!((times.starving.hours - expected).abs() < 1e-6, "{activity:?} x{gear}: {} against {expected}", times.starving.hours);

            let mut needs = only(MeterKind::Hunger, 0.9);
            needs.hunger.gear = gear;
            let mut small = ExhaustionTimes::default();
            levels(&mut needs, &mut small, &mut Exhaustion::default(), activity, 1.0 / 60.0, 600, 0);
            assert!((small.starving.hours - expected).abs() < 1e-6, "{activity:?} x{gear} in small steps: {} against {expected}", small.starving.hours);
        }
    }

    #[test]
    fn a_falling_meter_never_enters_the_worst_band() {
        let (mut needs, mut times, mut exh) = (only(MeterKind::Sleepiness, 0.99), ExhaustionTimes::default(), Exhaustion::default());
        times.step(&mut needs, &mut exh, Activity::Sleeping, 30.0, 0);
        assert_eq!(times.spent, WorstBand::default());
        assert_eq!(exh.level, 0);
    }

    #[test]
    fn leaving_the_worst_band_keeps_the_levels_and_starts_the_count_over() {
        // CON 0 waits 3 days.
        let (mut needs, mut times, mut exh) = (only(MeterKind::Hunger, 1.0), ExhaustionTimes::default(), Exhaustion::default());
        times.step(&mut needs, &mut exh, Activity::Walking, 72.0, 0);
        assert_eq!(exh.level, 1);
        needs.hunger.value = 0.5;
        times.step(&mut needs, &mut exh, Activity::Walking, 1.0, 0);
        assert_eq!(exh.level, 1, "eating does not undo a level");
        assert_eq!(times.starving, WorstBand::default());
        // Starving again counts a fresh 3 days, and the old level is not added twice.
        needs.hunger.value = 1.0;
        times.step(&mut needs, &mut exh, Activity::Walking, 71.0, 0);
        assert_eq!(exh.level, 1);
        times.step(&mut needs, &mut exh, Activity::Walking, 1.0, 0);
        assert_eq!(exh.level, 2);
    }

    #[test]
    fn a_spent_delver_who_sleeps_keeps_the_level_and_time_they_had_earned() {
        let (mut needs, mut times, mut exh) = (only(MeterKind::Sleepiness, 1.0), ExhaustionTimes::default(), Exhaustion::default());
        times.step(&mut needs, &mut exh, Activity::Walking, 30.0, 0);
        assert_eq!((exh.level, times.spent.hours), (1, 30.0));
        // Starting to sleep adds no worst-band time but takes none away before the meter falls.
        times.step(&mut needs, &mut exh, Activity::Sleeping, 0.0, 0);
        assert_eq!((exh.level, times.spent.hours), (1, 30.0));
        times.step(&mut needs, &mut exh, Activity::Sleeping, 1.0, 0);
        assert_eq!(exh.level, 1);
        assert!(needs.sleepiness.value < 1.0);
        assert_eq!(times.spent, WorstBand::default());
    }

    /// Level 3, fed and watered, and ready to sleep.
    fn weary() -> (Needs, ExhaustionTimes, Exhaustion) {
        let mut needs = Needs::default();
        needs.sleepiness.value = 0.8;
        (needs, ExhaustionTimes::default(), Exhaustion::new(3))
    }

    #[test]
    fn eight_hours_of_sleep_fed_and_watered_take_one_level() {
        let (mut needs, mut times, mut exh) = weary();
        let seen = levels(&mut needs, &mut times, &mut exh, Activity::Sleeping, 1.0, 16, 0);
        assert_eq!(seen[6], 3, "7 hours");
        assert_eq!(seen[7], 2, "8 hours");
        assert_eq!(seen[15], 2, "once per sleep, and the next update does not undo it");
    }

    #[test]
    fn eight_hours_of_sleep_in_one_step_take_one_level() {
        let (mut needs, mut times, mut exh) = weary();
        times.step(&mut needs, &mut exh, Activity::Sleeping, 8.0, 0);
        assert_eq!(exh.level, 2);
    }

    #[test]
    fn a_broken_sleep_starts_over() {
        let (mut needs, mut times, mut exh) = weary();
        times.step(&mut needs, &mut exh, Activity::Sleeping, 7.0, 0);
        times.step(&mut needs, &mut exh, Activity::Walking, 0.5, 0);
        needs.hunger.value = 0.0;
        needs.thirst.value = 0.0;
        times.step(&mut needs, &mut exh, Activity::Sleeping, 7.0, 0);
        assert_eq!(exh.level, 3);
        times.step(&mut needs, &mut exh, Activity::Sleeping, 1.0, 0);
        assert_eq!(exh.level, 2);
        // A second night takes a second level.
        times.step(&mut needs, &mut exh, Activity::Walking, 0.5, 0);
        needs.hunger.value = 0.0;
        needs.thirst.value = 0.0;
        times.step(&mut needs, &mut exh, Activity::Sleeping, 8.0, 0);
        assert_eq!(exh.level, 1);
    }

    #[test]
    fn sleeping_while_famished_dry_or_worse_gives_no_level() {
        for (hunger, thirst) in [(0.75, 0.0), (1.0, 0.0), (0.0, 0.75), (0.0, 1.0)] {
            let (mut needs, mut times, mut exh) = weary();
            needs.hunger.value = hunger;
            needs.thirst.value = thirst;
            levels(&mut needs, &mut times, &mut exh, Activity::Sleeping, 1.0, 12, 0);
            assert_eq!(exh.level, 3, "hunger {hunger}, thirst {thirst}");
        }
    }

    #[test]
    fn getting_famished_during_the_night_breaks_the_run() {
        // Sleeping hunger rises 1/32 an hour: 0.7 reaches 0.75 after 1.6 hours.
        let (mut needs, mut times, mut exh) = weary();
        needs.hunger.value = 0.7;
        levels(&mut needs, &mut times, &mut exh, Activity::Sleeping, 1.0, 12, 0);
        assert_eq!(exh.level, 3);
    }

    #[test]
    fn exhaustion_and_times_round_trip_through_json() {
        let (mut needs, mut times, mut exh) = (only(MeterKind::Hunger, 1.0), ExhaustionTimes::default(), Exhaustion::default());
        times.step(&mut needs, &mut exh, Activity::Walking, 80.0, 0);
        let parsed: ExhaustionTimes = serde_json::from_str(&serde_json::to_string(&times).unwrap()).unwrap();
        assert_eq!(parsed, times);
        let parsed: Exhaustion = serde_json::from_str(&serde_json::to_string(&exh).unwrap()).unwrap();
        assert_eq!(parsed, exh);
    }

    #[test]
    fn capacity_str_10_with_0_bonus() {
        assert_eq!(capacity_lb(10, 0.0), 150.0);
    }

    #[test]
    fn str_14_with_60_lb_is_light() {
        let cap = capacity_lb(14, 0.0);
        let band = LoadBand::from_load(60.0, cap);
        assert_eq!(band, LoadBand::Light);
    }

    #[test]
    fn str_10_with_155_lb_is_overloaded() {
        let cap = capacity_lb(10, 0.0);
        let band = LoadBand::from_load(155.0, cap);
        assert_eq!(band, LoadBand::Overloaded);
    }

    #[test]
    fn load_band_boundaries() {
        let cap = 150.0; // STR 10: capacity

        // Exactly 34% (51 lb) is Burdened
        assert_eq!(LoadBand::from_load(51.0, cap), LoadBand::Burdened);

        // Just under 34% (50.99 lb) is Light
        assert_eq!(LoadBand::from_load(50.99, cap), LoadBand::Light);

        // Exactly 67% (100.5 lb) is Encumbered
        assert_eq!(LoadBand::from_load(100.5, cap), LoadBand::Encumbered);

        // Just under 67% (100.49 lb) is Burdened
        assert_eq!(LoadBand::from_load(100.49, cap), LoadBand::Burdened);

        // Exactly 100% (150 lb) is Encumbered
        assert_eq!(LoadBand::from_load(150.0, cap), LoadBand::Encumbered);

        // Just over 100% (150.01 lb) is Overloaded
        assert_eq!(LoadBand::from_load(150.01, cap), LoadBand::Overloaded);
    }

    #[test]
    fn load_band_words() {
        assert_eq!(LoadBand::Light.word(), "Light");
        assert_eq!(LoadBand::Burdened.word(), "Burdened");
        assert_eq!(LoadBand::Encumbered.word(), "Encumbered");
        assert_eq!(LoadBand::Overloaded.word(), "Overloaded");
    }

    #[test]
    fn load_band_walk_multipliers() {
        assert_eq!(LoadBand::Light.walk_multiplier(), 1.0);
        assert_eq!(LoadBand::Burdened.walk_multiplier(), 0.85);
        assert_eq!(LoadBand::Encumbered.walk_multiplier(), 0.65);
        assert_eq!(LoadBand::Overloaded.walk_multiplier(), 0.0);
    }

    #[test]
    fn gear_adds_carry_multiplies_rates_and_takes_the_best_darkvision() {
        let worn = [
            Worn { carry: 25.0, darkvision: 10.0, hunger_rate: 0.85, ..Worn::default() },
            Worn { carry: 25.0, darkvision: 30.0, hunger_rate: 0.85, sleepiness_rate: 0.85 },
        ];
        let gear = Gear::new(20.0, worn, []);
        assert_eq!(gear.carry, 50.0);
        assert_eq!(gear.darkvision, 30.0, "the best, not the sum");
        assert!((gear.hunger_rate - 0.7225).abs() < 1e-12);
        assert_eq!(gear.sleepiness_rate, 0.85);
        assert_eq!(Gear::new(60.0, [Worn { darkvision: 10.0, ..Worn::default() }], []).darkvision, 60.0);
        assert_eq!(gear.capacity_lb(10), 200.0);
        assert_eq!(Gear::default(), Gear::new(0.0, [], []));
    }

    #[test]
    fn the_brightest_carried_light_gives_the_reach_and_burn_time() {
        let lights = [CarriedLight { reach: 25.0, burn_time: 1.0 }, CarriedLight { reach: 35.0, burn_time: 2.0 }];
        let gear = Gear::new(0.0, [], lights);
        assert_eq!((gear.light, gear.burn_time), (35.0, 2.0));
        assert_eq!((Gear::default().light, Gear::default().burn_time), (0.0, 1.0));
    }

    #[test]
    fn plenty_makes_hungry_arrive_at_8_over_0_85_hours() {
        let mut needs = Needs::default();
        Gear::new(0.0, [Worn { hunger_rate: 0.85, ..Worn::default() }], []).apply_to(&mut needs);
        let hungry_at = 8.0 / 0.85;
        needs.step(hungry_at - 0.001, Activity::Walking);
        assert_eq!(needs.hunger.word(), "Peckish");
        needs.step(0.002, Activity::Walking);
        assert_eq!(needs.hunger.word(), "Hungry");
        assert_eq!(needs.thirst.gear, 1.0, "thirst is not geared");
    }
}
