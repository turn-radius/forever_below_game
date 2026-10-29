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
//!
//! `poison` tracks the lasting Poisoned state a failed save against venom leaves.
//!
//! `recovery` mends a fed and watered character's hit points over game time.
//!
//! `conditions` reads all of it into the conditions a character is under right now, for the HUD.

pub mod check;
pub mod combat;
pub mod conditions;
pub mod effect;
pub mod explain;
pub mod poison;
pub mod recovery;
pub mod revival;
pub mod rules;

use rules::{Rules, defaults};

use fb_items::foe::{MAX_SHUN, MAX_WARD};
use fb_items::Foe;
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
        self.multiplier_with(defaults())
    }

    /// [`Activity::multiplier`] under `rules`.
    pub fn multiplier_with(self, rules: &Rules) -> f32 {
        rules.needs.activity_rate(self) as f32
    }
}

/// The label of a load, by SRD 5.1's "Variant: Encumbrance" (pp. 79-80) edges at 5, 10 and 15
/// times STR: Light up to 5, Burdened up to 10, Encumbered up to 15, Overloaded past that. The
/// band only names the load; pace and sprint drain come from the load itself ([`Burden`]).
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
        LoadBand::from_load_with(defaults(), load_lb, capacity_lb)
    }

    /// [`LoadBand::from_load`] under `rules`.
    pub fn from_load_with(rules: &Rules, load_lb: f64, capacity_lb: f64) -> Self {
        if capacity_lb <= 0.0 || load_lb > capacity_lb {
            LoadBand::Overloaded
        } else if load_lb > rules.load.encumbered_edge(capacity_lb) {
            LoadBand::Encumbered
        } else if load_lb > rules.load.burdened_edge(capacity_lb) {
            LoadBand::Burdened
        } else {
            LoadBand::Light
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

/// What a delver carries against what they can carry, in pounds: the load itself, from which
/// their [`LoadBand`], walking pace and sprint drain follow (the curve in [`LoadRules`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Burden {
    pub load_lb: f64,
    pub capacity_lb: f64,
}

impl Default for Burden {
    /// Carrying nothing.
    fn default() -> Self {
        Burden { load_lb: 0.0, capacity_lb: 1.0 }
    }
}

impl Burden {
    pub fn new(load_lb: f64, capacity_lb: f64) -> Self {
        Burden { load_lb, capacity_lb }
    }

    /// Their band.
    pub fn band(self) -> LoadBand {
        self.band_with(defaults())
    }

    /// [`Burden::band`] under `rules`.
    pub fn band_with(self, rules: &Rules) -> LoadBand {
        LoadBand::from_load_with(rules, self.load_lb, self.capacity_lb)
    }

    /// Walking pace multiple, 1 at Light, easing to 0.65 at capacity and 0 past it.
    pub fn walk_multiplier(self) -> f32 {
        self.walk_multiplier_with(defaults())
    }

    /// [`Burden::walk_multiplier`] under `rules`.
    pub fn walk_multiplier_with(self, rules: &Rules) -> f32 {
        rules.load.walk(self.load_lb, self.capacity_lb)
    }

    /// How far along the load curve they are, from 0 to 1 ([`LoadRules::strain`]).
    pub fn strain_with(self, rules: &Rules) -> f64 {
        rules.load.strain(self.load_lb, self.capacity_lb)
    }

    /// How many times as fast as usual sprinting drains stamina.
    pub fn sprint_drain(self) -> f64 {
        self.sprint_drain_with(defaults())
    }

    /// [`Burden::sprint_drain`] under `rules`.
    pub fn sprint_drain_with(self, rules: &Rules) -> f64 {
        rules.load.drain(self.load_lb, self.capacity_lb)
    }
}

/// Points worn items add to the five ability scores (an affix "of Might"'s +1 STR), summed.
/// The default adds nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityBonus {
    pub strength: i32,
    pub dexterity: i32,
    pub constitution: i32,
    pub intelligence: i32,
    pub wisdom: i32,
}

impl AbilityBonus {
    /// The scores `base` has with this added, each held within 1 to 30, the range a score can
    /// have ([SRD 5.1 p. 76](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=76),
    /// "Ability Scores and Modifiers").
    pub fn on(self, base: &Attributes) -> Attributes {
        let add = |score: i32, by: i32| (score + by).clamp(1, 30);
        Attributes {
            strength: add(base.strength, self.strength),
            dexterity: add(base.dexterity, self.dexterity),
            constitution: add(base.constitution, self.constitution),
            intelligence: add(base.intelligence, self.intelligence),
            wisdom: add(base.wisdom, self.wisdom),
        }
    }

    /// Each score's bonus with its name, for a sheet: `[("STR", 2), ...]`.
    pub fn named(self) -> [(&'static str, i32); 5] {
        [("STR", self.strength), ("DEX", self.dexterity), ("CON", self.constitution), ("INT", self.intelligence), ("WIS", self.wisdom)]
    }

    /// This and `other` added, score by score.
    pub fn plus(self, other: AbilityBonus) -> AbilityBonus {
        AbilityBonus {
            strength: self.strength + other.strength,
            dexterity: self.dexterity + other.dexterity,
            constitution: self.constitution + other.constitution,
            intelligence: self.intelligence + other.intelligence,
            wisdom: self.wisdom + other.wisdom,
        }
    }
}

/// Carrying capacity in pounds (SRD p. 79).
/// `capacity = STR × 15 + carry` where `carry` is gear-specific bonus capacity.
pub fn capacity_lb(strength: i32, carry_bonus_lb: f64) -> f64 {
    capacity_lb_with(defaults(), strength, carry_bonus_lb)
}

/// [`capacity_lb`] under `rules`.
pub fn capacity_lb_with(rules: &Rules, strength: i32, carry_bonus_lb: f64) -> f64 {
    (strength as f64) * rules.load.capacity_per_strength + carry_bonus_lb
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
    /// Points of Stealth: each makes the wearer a twentieth less visible ([`visibility`]).
    pub stealth: f64,
    /// Warmth the wearer is given.
    #[serde(default)]
    pub warmth: f64,
    /// Hit points the wearer regains on each kill (a weapon "of the Leech"'s). Only what the
    /// delver strikes with counts, which the game decides when it builds the [`Worn`].
    #[serde(default)]
    pub heal_on_kill: f64,
    /// Multiple of the wearer's pace.
    #[serde(default = "default_gear")]
    pub speed: f64,
    /// Chance to turn aside a blow that would hit: a held shield's, and what other gear adds to
    /// it. It counts only with a shield in the off hand, which the game decides when it builds
    /// the [`Worn`].
    #[serde(default)]
    pub block_chance: f64,
    /// Points added to the ability scores.
    #[serde(default)]
    pub abilities: AbilityBonus,
    /// Multiple of the chance of catching a disease (Mildewed, Lice-ridden, of Lesser Warding).
    #[serde(default = "default_gear")]
    pub disease_risk: f64,
    /// Points of luck: each rolls the loot the wearer finds a level deeper (of Chance, of
    /// Fortune, of Luck).
    #[serde(default)]
    pub luck: f64,
    /// Points of recovery: each mends the wearer faster ([`recovery::minutes_per_hit_point`]).
    #[serde(default)]
    pub recovery: f64,
    /// Points added to the wearer's saves against venom.
    #[serde(default)]
    pub poison_resistance: f64,
    /// Points added to the melee attack and the shot roll: worn gear's, not the weapon in hand,
    /// whose own to hit the weapon's roll already has.
    #[serde(default)]
    pub to_hit: f64,
    /// Points added to the shot roll only: any gear's, the bow's own included.
    #[serde(default)]
    pub ranged_to_hit: f64,
    /// The fraction of damage taken off each kind of creature's blows, by [`fb_items::Foe`] index.
    #[serde(default)]
    pub ward: [f64; Foe::COUNT],
    /// The chance each kind of creature never notices the wearer, by [`fb_items::Foe`] index.
    #[serde(default)]
    pub shun: [f64; Foe::COUNT],
    /// Multiple of the speed of a swing, a draw and a reload: the weapon in hand's and the
    /// gloves', which the game decides when it builds the [`Worn`].
    #[serde(default = "default_gear")]
    pub attack_rate: f64,
    /// Multiple of how fast stamina refills (Tireless, of Second Wind).
    #[serde(default = "default_gear")]
    pub stamina_refill: f64,
    /// Multiple of how fast mana refills (Attuned, of the Wellspring).
    #[serde(default = "default_gear")]
    pub mana_refill: f64,
    /// Multiple of what stamina costs: a swing, a block and a sprint's drain (Light-handed, of
    /// Endurance). Lower is better.
    #[serde(default = "default_gear")]
    pub stamina_cost: f64,
}

impl Default for Worn {
    fn default() -> Self {
        Worn { carry: 0.0, darkvision: 0.0, hunger_rate: 1.0, sleepiness_rate: 1.0, stealth: 0.0, warmth: 0.0, heal_on_kill: 0.0, speed: 1.0, block_chance: 0.0, abilities: AbilityBonus::default(), disease_risk: 1.0, luck: 0.0, recovery: 0.0, poison_resistance: 0.0, to_hit: 0.0, ranged_to_hit: 0.0, attack_rate: 1.0, stamina_refill: 1.0, mana_refill: 1.0, stamina_cost: 1.0, ward: [0.0; Foe::COUNT], shun: [0.0; Foe::COUNT] }
    }
}

/// The most any gear makes a raised shield block: 90%, our own, so no blow is certainly turned.
pub const MAX_BLOCK_CHANCE: f64 = 0.9;

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
    /// Points of Stealth: the worn items' added.
    pub stealth: f64,
    /// Feet the carried light reaches; 0 with none.
    pub light: f64,
    /// How long the carried light burns, as a multiple of its base's.
    pub burn_time: f64,
    /// Hit points regained on a kill: the worn items' added.
    pub heal_on_kill: f64,
    /// Multiple of the delver's pace: the items' multiplied.
    pub speed: f64,
    /// Chance to turn aside a blow that would hit: the items' added, at most [`MAX_BLOCK_CHANCE`].
    pub block_chance: f64,
    /// Points added to the ability scores: the items' added.
    pub abilities: AbilityBonus,
    /// Multiple of the chance of catching a disease: the items' multiplied.
    #[serde(default = "default_gear")]
    pub disease_risk: f64,
    /// Points of luck: the worn items' added.
    #[serde(default)]
    pub luck: f64,
    /// Points of recovery: the worn items' added.
    #[serde(default)]
    pub recovery: f64,
    /// Points added to saves against venom: the worn items' added.
    #[serde(default)]
    pub poison_resistance: f64,
    /// Points added to the melee attack and the shot roll: the worn items' added.
    #[serde(default)]
    pub to_hit: f64,
    /// Points added to the shot roll only: the items' added.
    #[serde(default)]
    pub ranged_to_hit: f64,
    /// The fraction of damage taken off each kind of creature's blows: the items' added, at most
    /// [`MAX_WARD`](fb_items::foe::MAX_WARD).
    #[serde(default)]
    pub ward: [f64; Foe::COUNT],
    /// The chance each kind of creature never notices the delver: the items' chances combined
    /// ([`fb_items::foe::combine_shun`], at most [`MAX_SHUN`](fb_items::foe::MAX_SHUN)).
    #[serde(default)]
    pub shun: [f64; Foe::COUNT],
    /// Multiple of the speed of a swing, a draw and a reload: the items' multiplied.
    #[serde(default = "default_gear")]
    pub attack_rate: f64,
    /// Multiple of how fast stamina refills: the items' multiplied.
    #[serde(default = "default_gear")]
    pub stamina_refill: f64,
    /// Multiple of how fast mana refills: the items' multiplied.
    #[serde(default = "default_gear")]
    pub mana_refill: f64,
    /// Multiple of what swinging, blocking and sprinting cost in stamina: the items' multiplied.
    #[serde(default = "default_gear")]
    pub stamina_cost: f64,
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
        let mut gear = Gear { carry: 0.0, darkvision: folk_darkvision, hunger_rate: 1.0, sleepiness_rate: 1.0, stealth: 0.0, light: 0.0, burn_time: 1.0, heal_on_kill: 0.0, speed: 1.0, block_chance: 0.0, abilities: AbilityBonus::default(), disease_risk: 1.0, luck: 0.0, recovery: 0.0, poison_resistance: 0.0, to_hit: 0.0, ranged_to_hit: 0.0, attack_rate: 1.0, stamina_refill: 1.0, mana_refill: 1.0, stamina_cost: 1.0, ward: [0.0; Foe::COUNT], shun: [0.0; Foe::COUNT] };
        for item in worn {
            gear.to_hit += item.to_hit;
            gear.ranged_to_hit += item.ranged_to_hit;
            gear.luck += item.luck;
            gear.recovery += item.recovery;
            gear.poison_resistance += item.poison_resistance;
            gear.carry += item.carry;
            gear.darkvision = gear.darkvision.max(item.darkvision);
            gear.hunger_rate *= item.hunger_rate;
            gear.sleepiness_rate *= item.sleepiness_rate;
            gear.stealth += item.stealth;
            gear.heal_on_kill += item.heal_on_kill;
            gear.speed *= item.speed;
            gear.attack_rate *= item.attack_rate;
            gear.stamina_refill *= item.stamina_refill;
            gear.mana_refill *= item.mana_refill;
            gear.stamina_cost *= item.stamina_cost;
            gear.block_chance = (gear.block_chance + item.block_chance).clamp(0.0, MAX_BLOCK_CHANCE);
            gear.abilities = gear.abilities.plus(item.abilities);
            gear.disease_risk *= item.disease_risk;
            for foe in Foe::ALL {
                let i = foe.index();
                gear.ward[i] = (gear.ward[i] + item.ward[i]).clamp(0.0, MAX_WARD as f64);
                gear.shun[i] = (1.0 - (1.0 - gear.shun[i]) * (1.0 - item.shun[i])).clamp(0.0, MAX_SHUN as f64);
            }
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

    /// [`Gear::capacity_lb`] under `rules`.
    pub fn capacity_lb_with(&self, rules: &Rules, strength: i32) -> f64 {
        capacity_lb_with(rules, strength, self.carry)
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
        self.band_with(defaults())
    }

    /// [`Meter::band`] under `rules`.
    pub fn band_with(&self, rules: &Rules) -> usize {
        rules.needs.band(self.value)
    }

    /// Returns the HUD word for this meter at its current band.
    pub fn word(&self) -> &'static str {
        self.kind.word(self.band())
    }

    /// [`Meter::word`] under `rules`, which may name the bands otherwise.
    pub fn word_with<'r>(&self, rules: &'r Rules) -> &'r str {
        rules.needs.word(self.kind, self.band_with(rules))
    }

    /// The meter's change per game hour at `activity`: the base rate scaled by activity and gear.
    /// Sleeping halves the rate of hunger and thirst, and sleepiness falls by 1/8 per hour instead.
    pub fn effective_rate(&self, activity: Activity) -> f64 {
        self.effective_rate_with(defaults(), activity)
    }

    /// [`Meter::effective_rate`] under `rules`. The base rate is the meter's own
    /// ([`Meter::rate_per_hour`]); [`Rules::apply_to`] puts the rules' on it.
    pub fn effective_rate_with(&self, rules: &Rules, activity: Activity) -> f64 {
        match (activity, self.kind) {
            (Activity::Sleeping, MeterKind::Hunger | MeterKind::Thirst) => self.rate_per_hour * rules.needs.sleeping_rate * self.gear,
            (Activity::Sleeping, MeterKind::Sleepiness) => -(1.0 / rules.needs.sleep_hours_to_rested),
            (_, _) => self.rate_per_hour * activity.multiplier_with(rules) as f64 * self.gear,
        }
    }

    /// Advances the meter by `delta_hours` of game time at its [`Meter::effective_rate`].
    pub fn step(&mut self, delta_hours: f64, activity: Activity) {
        self.step_with(defaults(), delta_hours, activity);
    }

    /// [`Meter::step`] under `rules`.
    pub fn step_with(&mut self, rules: &Rules, delta_hours: f64, activity: Activity) {
        self.value = (self.value + self.effective_rate_with(rules, activity) * delta_hours).clamp(0.0, 1.0);
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
    pub fn base_rate(self) -> f64 {
        defaults().needs.base_rate(self)
    }

    /// The default HUD word for this meter at the given band (0..=4); [`Rules`] may rename them.
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
        self.step_with(defaults(), delta_hours, activity);
    }

    /// [`Needs::step`] under `rules`.
    pub fn step_with(&mut self, rules: &Rules, delta_hours: f64, activity: Activity) {
        self.hunger.step_with(rules, delta_hours, activity);
        self.thirst.step_with(rules, delta_hours, activity);
        self.sleepiness.step_with(rules, delta_hours, activity);
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
    /// The level that is death.
    pub const DEATH: u32 = 6;

    /// Creates a new exhaustion level.
    pub fn new(level: u32) -> Self {
        Exhaustion { level: level.min(Self::DEATH) }
    }

    /// Speed multiplier for this exhaustion level (SRD p. 358).
    /// Level 2 is half speed, level 5 is speed 0.
    pub fn speed_multiplier(self) -> f32 {
        self.speed_multiplier_with(defaults())
    }

    /// [`Exhaustion::speed_multiplier`] under `rules`.
    pub fn speed_multiplier_with(self, rules: &Rules) -> f32 {
        if self.level >= rules.exhaustion.halted_from {
            0.0
        } else if self.level >= rules.exhaustion.slowed_from {
            rules.exhaustion.slowed_speed
        } else {
            1.0
        }
    }

    /// Returns true if this is death (level 6).
    pub fn is_dead(self) -> bool {
        self.level >= Self::DEATH
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
    #[allow(clippy::too_many_arguments)]
    fn advance(&mut self, rules: &Rules, before: f64, meter: &Meter, activity: Activity, hours: f64, first_hours: f64, every_hours: f64) -> u32 {
        self.hours += hours_at_worst(before, meter.effective_rate_with(rules, activity), hours);
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

/// The hours of a step of `hours` before a rising meter reaches `famished_at`
/// ([`rules::ExhaustionRules::famished_at`]), given its value `before` the step and its rate per
/// hour.
fn hours_below_famished(famished_at: f64, before: f64, rate: f64, hours: f64) -> f64 {
    if before >= famished_at {
        0.0
    } else if rate > 0.0 {
        hours.min((famished_at - before) / rate)
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
        self.step_with(defaults(), needs, exhaustion, activity, hours, con_modifier);
    }

    /// [`ExhaustionTimes::step`] under `rules`.
    pub fn step_with(&mut self, rules: &Rules, needs: &mut Needs, exhaustion: &mut Exhaustion, activity: Activity, hours: f64, con_modifier: i32) {
        let before = *needs;
        needs.step_with(rules, hours, activity);

        let r = &rules.exhaustion;
        let starving_days = (r.starving_first_days + con_modifier as f64).max(r.starving_first_days_least);
        let mut gained = self.starving.advance(rules, before.hunger.value, &needs.hunger, activity, hours, starving_days * 24.0, r.starving_every_hours);
        gained += self.dehydrated.advance(rules, before.thirst.value, &needs.thirst, activity, hours, r.dehydrated_first_hours, r.dehydrated_every_hours);
        gained += self.spent.advance(rules, before.sleepiness.value, &needs.sleepiness, activity, hours, r.spent_first_hours, r.spent_every_hours);
        exhaustion.level = (exhaustion.level + gained).min(6);

        self.sleep(rules, &before, needs, exhaustion, activity, hours);
    }

    /// Tracks the run of sleep through the step and removes a level once it reaches
    /// [`rules::ExhaustionRules::sleep_hours_to_recover`].
    fn sleep(&mut self, rules: &Rules, before: &Needs, needs: &Needs, exhaustion: &mut Exhaustion, activity: Activity, hours: f64) {
        if activity != Activity::Sleeping {
            self.sleep_hours = 0.0;
            self.slept_recovered = false;
            return;
        }
        let famished_at = rules.exhaustion.famished_at;
        let fed = hours_below_famished(famished_at, before.hunger.value, needs.hunger.effective_rate_with(rules, activity), hours);
        let watered = hours_below_famished(famished_at, before.thirst.value, needs.thirst.effective_rate_with(rules, activity), hours);
        let comfortable = fed.min(watered);
        self.sleep_hours += comfortable;
        if self.sleep_hours + THRESHOLD_SLACK >= rules.exhaustion.sleep_hours_to_recover && !self.slept_recovered && !exhaustion.is_dead() {
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

/// A winded delver may sprint and swing again from this much stamina: the default of
/// [`rules::StaminaRules::second_wind`].
pub const SECOND_WIND: f64 = 0.2;
/// What one swing costs, of a full pool: the default of [`rules::StaminaRules::swing_cost`].
pub const SWING_COST: f64 = 0.08;

/// The short-term pool that sprinting and swinging drain, from 1 (full) to 0 (empty), stepping
/// in real seconds. It drains faster the heavier the load and refills in seconds when the
/// delver walks or stands. Emptied, the delver is winded and cannot sprint or swing hard until
/// the pool is back to [`SECOND_WIND`].
///
/// Every step is closed form for a constant activity, so any step length gives the same result.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stamina {
    /// From 0 (empty) to 1 (full).
    pub value: f64,
    /// Emptied and not yet back to [`SECOND_WIND`].
    pub winded: bool,
}

impl Default for Stamina {
    fn default() -> Self {
        Stamina { value: 1.0, winded: false }
    }
}

impl Stamina {
    /// The change per real second at `activity` under `load`: negative drains. Only Exerting
    /// drains, by a full pool in [`SPRINT_SECONDS`] while Light, and as many times faster as the
    /// load asks ([`Burden::sprint_drain`]): 1 through the Burdened edge, easing to 2 at capacity.
    /// Sleeping refills twice as fast as standing.
    pub fn rate_per_second(activity: Activity, load: Burden) -> f64 {
        Stamina::rate_per_second_with(defaults(), activity, load)
    }

    /// [`Stamina::rate_per_second`] under `rules`.
    pub fn rate_per_second_with(rules: &Rules, activity: Activity, load: Burden) -> f64 {
        Stamina::rate_per_second_geared(rules, activity, load, 1.0, 1.0)
    }

    /// [`Stamina::rate_per_second_with`] for a delver whose gear multiplies the refill rates by
    /// `refill` and the sprint drain by `cost` ([`Gear::stamina_refill`], [`Gear::stamina_cost`]).
    pub fn rate_per_second_geared(rules: &Rules, activity: Activity, load: Burden, refill: f64, cost: f64) -> f64 {
        let s = &rules.stamina;
        match activity {
            Activity::Exerting => -load.sprint_drain_with(rules) * cost / s.sprint_seconds,
            // The refill fades out with the load's strain and a slow drain fades in, both on the
            // same curve as pace: continuous, refilling through the Burdened edge.
            Activity::Walking => {
                let strain = load.strain_with(rules);
                refill / s.walk_refill_seconds * (1.0 - strain) - strain / rules.load.walk_drain_seconds
            }
            Activity::Resting => refill / s.stand_refill_seconds,
            Activity::Sleeping => refill * s.sleep_refill_factor / s.stand_refill_seconds,
        }
    }

    /// Whether the delver may sprint or swing hard: not winded.
    pub fn can_exert(&self) -> bool {
        !self.winded
    }

    /// Advances by `seconds` of real time at `activity` under `load`.
    pub fn step(&mut self, seconds: f64, activity: Activity, load: Burden) {
        self.step_with(defaults(), seconds, activity, load);
    }

    /// [`Stamina::step`] under `rules`.
    pub fn step_with(&mut self, rules: &Rules, seconds: f64, activity: Activity, load: Burden) {
        self.step_geared(rules, seconds, activity, load, 1.0, 1.0);
    }

    /// [`Stamina::step_with`] under the gear's `refill` and `cost` multiples
    /// ([`Stamina::rate_per_second_geared`]).
    pub fn step_geared(&mut self, rules: &Rules, seconds: f64, activity: Activity, load: Burden, refill: f64, cost: f64) {
        self.value = (self.value + Stamina::rate_per_second_geared(rules, activity, load, refill, cost) * seconds).clamp(0.0, 1.0);
        self.settle(rules);
    }

    /// Takes `cost` off the pool, never below empty.
    pub fn spend(&mut self, cost: f64) {
        self.spend_with(defaults(), cost);
    }

    /// [`Stamina::spend`] under `rules`.
    pub fn spend_with(&mut self, rules: &Rules, cost: f64) {
        self.value = (self.value - cost).max(0.0);
        self.settle(rules);
    }

    /// Winds the delver at empty and lets them recover at the second wind.
    fn settle(&mut self, rules: &Rules) {
        // A drain that lands on empty can leave float dust.
        if self.value <= 1e-9 {
            self.value = 0.0;
            self.winded = true;
        } else if self.winded && self.value >= rules.stamina.second_wind {
            self.winded = false;
        }
    }
}

/// How a character casts: the pool a spell caster draws on and how big it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Casting {
    /// Fighters: a small flat pool.
    #[default]
    Martial,
    /// Mages: the pool follows INT.
    Arcane,
    /// Clerics: the pool follows WIS.
    Divine,
}

/// The pool spells will draw on, in points. Its size comes from INT for [`Casting::Arcane`]
/// or WIS for [`Casting::Divine`], through the SRD modifier ([`modifier`]), and it refills in
/// real seconds like stamina but slower, faster standing than walking and fastest asleep. Gear
/// speeds or slows the refill ([`Gear::mana_refill`]). Nothing spends it yet. Closed form for a
/// constant activity, so any step length gives the same result.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mana {
    /// Points in the pool now, from 0 to `max`.
    pub points: f64,
    /// The most the pool holds.
    pub max: f64,
}

impl Default for Mana {
    fn default() -> Self {
        Mana::full(defaults().mana.martial_pool)
    }
}

impl Mana {
    /// A full pool of `max` points.
    pub fn full(max: f64) -> Mana {
        Mana { points: max, max }
    }

    /// The pool size for a character who casts as `casting`, from their ability scores:
    /// 10 points plus 5 for each point of the modifier of INT (arcane) or WIS (divine), and
    /// never fewer than 4. A martial character has 4.
    pub fn max_for(casting: Casting, attributes: &Attributes) -> f64 {
        Mana::max_for_with(defaults(), casting, attributes)
    }

    /// [`Mana::max_for`] under `rules`.
    pub fn max_for_with(rules: &Rules, casting: Casting, attributes: &Attributes) -> f64 {
        let m = &rules.mana;
        let score = match casting {
            Casting::Martial => return m.martial_pool,
            Casting::Arcane => attributes.intelligence,
            Casting::Divine => attributes.wisdom,
        };
        (m.caster_base + m.caster_per_modifier * modifier(score) as f64).max(m.caster_floor)
    }

    /// How full the pool is, from 0 to 1.
    pub fn fraction(&self) -> f64 {
        if self.max <= 0.0 { 0.0 } else { (self.points / self.max).clamp(0.0, 1.0) }
    }

    /// Makes the pool `max` points, keeping how full it is.
    pub fn resize(&mut self, max: f64) {
        let fraction = self.fraction();
        self.max = max;
        self.points = max * fraction;
    }

    /// The share of a full pool regained each real second at `activity`: none while exerting.
    pub fn refill_per_second(activity: Activity) -> f64 {
        Mana::refill_per_second_with(defaults(), activity)
    }

    /// [`Mana::refill_per_second`] under `rules`.
    pub fn refill_per_second_with(rules: &Rules, activity: Activity) -> f64 {
        Mana::refill_per_second_geared(rules, activity, 1.0)
    }

    /// [`Mana::refill_per_second_with`] for a delver whose gear multiplies the refill by
    /// `refill` ([`Gear::mana_refill`]).
    pub fn refill_per_second_geared(rules: &Rules, activity: Activity, refill: f64) -> f64 {
        let m = &rules.mana;
        match activity {
            Activity::Exerting => 0.0,
            Activity::Walking => refill / m.walk_refill_seconds,
            Activity::Resting => refill / m.stand_refill_seconds,
            Activity::Sleeping => refill * m.sleep_refill_factor / m.stand_refill_seconds,
        }
    }

    /// Advances by `seconds` of real time at `activity`.
    pub fn step(&mut self, seconds: f64, activity: Activity) {
        self.step_with(defaults(), seconds, activity);
    }

    /// [`Mana::step`] under `rules`.
    pub fn step_with(&mut self, rules: &Rules, seconds: f64, activity: Activity) {
        self.step_geared(rules, seconds, activity, 1.0);
    }

    /// [`Mana::step_with`] under the gear's `refill` multiple ([`Mana::refill_per_second_geared`]).
    pub fn step_geared(&mut self, rules: &Rules, seconds: f64, activity: Activity, refill: f64) {
        self.points = (self.points + self.max * Mana::refill_per_second_geared(rules, activity, refill) * seconds).clamp(0.0, self.max);
    }

    /// Spends `cost` points if the pool holds them; returns whether it did.
    pub fn spend(&mut self, cost: f64) -> bool {
        if cost > self.points {
            return false;
        }
        self.points -= cost;
        true
    }
}

/// How easy the delver is to see, from 0 (not at all) to 1 (plain as day): a pure function of
/// the `light` falling on them (0 dark to 1 fully lit), their `pace` (a multiple of walking
/// pace, 0 still), the `load` they carry and their `stealth` (the sum of their gear's, each
/// point worth a twentieth). They are hardly seen standing in the dark; light is what shows
/// them most, then moving fast, then bulky gear. Creatures can read it as the delver does.
pub fn visibility(light: f64, pace: f64, load: LoadBand, stealth: f64) -> f64 {
    visibility_with(defaults(), light, pace, load, stealth)
}

/// [`visibility`] under `rules`.
pub fn visibility_with(rules: &Rules, light: f64, pace: f64, load: LoadBand, stealth: f64) -> f64 {
    let v = &rules.visibility;
    let lit = light.clamp(0.0, 1.0);
    let moving = pace.clamp(0.0, v.sprint_pace) / v.sprint_pace;
    let bulk = match load {
        LoadBand::Light => 0.0,
        LoadBand::Burdened => v.burdened_bulk,
        LoadBand::Encumbered | LoadBand::Overloaded => v.encumbered_bulk,
    };
    (v.in_dark + v.in_light * lit + v.moving * moving + bulk - v.stealth_step * stealth).clamp(0.0, 1.0)
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
        // STR 10: capacity 150, edges at 5 and 10 times STR.
        let cap = 150.0;
        assert_eq!(LoadBand::from_load(50.0, cap), LoadBand::Light, "Light up to 5 x STR");
        assert_eq!(LoadBand::from_load(50.01, cap), LoadBand::Burdened);
        assert_eq!(LoadBand::from_load(100.0, cap), LoadBand::Burdened, "Burdened up to 10 x STR");
        assert_eq!(LoadBand::from_load(100.01, cap), LoadBand::Encumbered);
        assert_eq!(LoadBand::from_load(150.0, cap), LoadBand::Encumbered, "Encumbered up to 15 x STR");
        assert_eq!(LoadBand::from_load(150.01, cap), LoadBand::Overloaded);
        assert_eq!(LoadBand::from_load(1.0, 0.0), LoadBand::Overloaded);
    }

    #[test]
    fn load_band_words() {
        assert_eq!(LoadBand::Light.word(), "Light");
        assert_eq!(LoadBand::Burdened.word(), "Burdened");
        assert_eq!(LoadBand::Encumbered.word(), "Encumbered");
        assert_eq!(LoadBand::Overloaded.word(), "Overloaded");
    }

    /// STR 11, capacity 165: the edges are 55, 110 and 165 lb.
    fn str_11(load_lb: f64) -> Burden {
        Burden::new(load_lb, capacity_lb(11, 0.0))
    }

    fn pace_percent(load_lb: f64) -> f64 {
        str_11(load_lb).walk_multiplier() as f64 * 100.0
    }

    #[test]
    fn the_issue_table_for_str_11() {
        // (load, sprint drain, walking pace %)
        let table = [(55.0, 1.0, 100.0), (82.5, 1.0625, 97.8125), (110.0, 1.25, 91.25), (137.5, 1.5625, 80.3125), (165.0, 2.0, 65.0)];
        for (lb, drain, pace) in table {
            assert!((str_11(lb).sprint_drain() - drain).abs() < 1e-9, "{lb} lb drain {}", str_11(lb).sprint_drain());
            assert!((pace_percent(lb) - pace).abs() < 1e-4, "{lb} lb pace {}", pace_percent(lb));
        }
        assert_eq!(format!("{:.1}", pace_percent(82.5)), "97.8");
        assert_eq!(format!("{:.1}", pace_percent(110.0)), "91.3");
        assert_eq!(format!("{:.1}", pace_percent(137.5)), "80.3");
    }

    #[test]
    fn the_issue_examples_for_str_11() {
        assert_eq!(pace_percent(55.0), 100.0, "full pace at 55 lb");
        assert_eq!(str_11(55.0).band(), LoadBand::Light);
        assert!(pace_percent(56.0) > 99.99 && pace_percent(56.0) < 100.0, "99.99% at 56 lb, not 85%: {}", pace_percent(56.0));
        let sixty_five = str_11(65.0);
        assert_eq!(sixty_five.band(), LoadBand::Burdened);
        assert_eq!(format!("{:.1}", pace_percent(65.0)), "99.7");
        assert_eq!(format!("{:.2}", sixty_five.sprint_drain()), "1.01");
        assert_eq!(str_11(110.0).band(), LoadBand::Burdened);
        assert_eq!(str_11(110.5).band(), LoadBand::Encumbered);
        assert_eq!(str_11(165.0).band(), LoadBand::Encumbered);
        assert_eq!(str_11(166.0).band(), LoadBand::Overloaded);
        assert_eq!(str_11(166.0).walk_multiplier(), 0.0, "rooted");
        assert_eq!(str_11(40.0).band(), LoadBand::Light);
    }

    #[test]
    fn pace_and_drain_never_jump_nor_turn_a_corner_below_overloaded() {
        let (mut last_pace, mut last_drain) = (1.0, 1.0);
        let (mut last_pace_step, mut last_drain_step) = (0.0, 0.0);
        let mut pounds = 0.0;
        while pounds <= 165.0 {
            let (pace, drain) = (str_11(pounds).walk_multiplier() as f64, str_11(pounds).sprint_drain());
            // One pound moves them by well under a point of pace or a tenth of drain.
            assert!(last_pace - pace < 0.01 && drain - last_drain < 0.025, "{pounds} lb: {last_pace} to {pace}, {last_drain} to {drain}");
            // A corner is a sudden change of slope: the fall per step only ever grows smoothly.
            let (pace_step, drain_step) = (last_pace - pace, drain - last_drain);
            assert!(pace_step + 1e-6 >= last_pace_step && drain_step + 1e-6 >= last_drain_step, "{pounds} lb bends the wrong way");
            assert!(pace_step - last_pace_step < 0.0005 && drain_step - last_drain_step < 0.0015, "{pounds} lb: a corner, {last_pace_step} to {pace_step}");
            (last_pace, last_drain, last_pace_step, last_drain_step) = (pace, drain, pace_step, drain_step);
            pounds += 1.0;
        }
        // Past the Burdened edge it starts from rest: no step at 55 lb.
        assert!(pace_percent(56.0) > 99.9 && str_11(56.0).sprint_drain() < 1.001);
    }

    #[test]
    fn a_straight_line_curve_is_a_line() {
        let mut rules = Rules::default();
        rules.load.load_curve = 1.0;
        let at_ten_times = Burden::new(110.0, 165.0);
        assert!((at_ten_times.sprint_drain_with(&rules) - 1.5).abs() < 1e-9);
        assert!((at_ten_times.walk_multiplier_with(&rules) as f64 - 0.825).abs() < 1e-6);
    }

    #[test]
    fn a_ring_of_strength_moves_every_edge_through_the_game_path() {
        // Capacity comes from the gear as the game derives it: STR 11 plus a ring of +2 STR.
        let bonus = AbilityBonus { strength: 2, ..AbilityBonus::default() };
        let attributes = bonus.on(&Attributes { strength: 11, dexterity: 10, constitution: 10, intelligence: 10, wisdom: 10 });
        let rules = defaults();
        let capacity = Gear::default().capacity_lb_with(rules, attributes.strength);
        assert_eq!(capacity, 195.0);
        assert_eq!((rules.load.burdened_edge(capacity), rules.load.encumbered_edge(capacity)), (65.0, 130.0));
        assert_eq!(LoadBand::from_load(65.0, capacity), LoadBand::Light);
        assert_eq!(LoadBand::from_load(66.0, capacity), LoadBand::Burdened);
        assert_eq!(LoadBand::from_load(131.0, capacity), LoadBand::Encumbered);
        assert_eq!(Burden::new(65.0, capacity).walk_multiplier(), 1.0, "65 lb is full pace for them, as 55 is without the ring");
        assert!((Burden::new(130.0, capacity).sprint_drain() - 1.25).abs() < 1e-9);
    }

    #[test]
    fn wards_add_to_a_cap_and_shuns_combine() {
        let ward = |foe: Foe, by: f64| {
            let mut w = Worn::default();
            w.ward[foe.index()] = by;
            w
        };
        let shun = |foe: Foe, by: f64| {
            let mut w = Worn::default();
            w.shun[foe.index()] = by;
            w
        };
        let gear = Gear::new(0.0, [ward(Foe::Undead, 0.25), ward(Foe::Undead, 0.25), ward(Foe::Vermin, 0.25), shun(Foe::Vermin, 0.5), shun(Foe::Vermin, 0.35)], []);
        assert_eq!(gear.ward[Foe::Undead.index()], 0.5);
        assert_eq!(gear.ward[Foe::Vermin.index()], 0.25);
        assert_eq!(gear.ward[Foe::Troll.index()], 0.0);
        assert!((gear.shun[Foe::Vermin.index()] - 0.675).abs() < 1e-9);
        let many = Gear::new(0.0, [ward(Foe::Spider, 0.5), ward(Foe::Spider, 0.5)], []);
        assert_eq!(many.ward[Foe::Spider.index()], 0.75, "capped");
        assert_eq!(Gear::default().ward, [0.0; Foe::COUNT]);
    }

    #[test]
    fn gear_adds_carry_multiplies_rates_and_takes_the_best_darkvision() {
        let worn = [
            Worn { carry: 25.0, darkvision: 10.0, hunger_rate: 0.85, ..Worn::default() },
            Worn { carry: 25.0, darkvision: 30.0, hunger_rate: 0.85, sleepiness_rate: 0.85, stealth: 0.0, warmth: 0.0, ..Worn::default() },
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


    fn scores(intelligence: i32, wisdom: i32) -> Attributes {
        Attributes { strength: 10, dexterity: 10, constitution: 10, intelligence, wisdom }
    }

    #[test]
    fn a_mage_with_int_16_has_a_bigger_pool_than_one_with_int_10() {
        let clever = Mana::max_for(Casting::Arcane, &scores(16, 10));
        let average = Mana::max_for(Casting::Arcane, &scores(10, 10));
        assert_eq!((clever, average), (25.0, 10.0));
    }

    #[test]
    fn a_clerics_pool_follows_wis_and_a_mages_ignores_it() {
        assert_eq!(Mana::max_for(Casting::Divine, &scores(10, 16)), 25.0);
        assert_eq!(Mana::max_for(Casting::Divine, &scores(18, 10)), 10.0);
        assert_eq!(Mana::max_for(Casting::Arcane, &scores(10, 18)), 10.0);
    }

    #[test]
    fn fighters_have_a_small_flat_pool_and_a_dull_caster_is_floored() {
        assert_eq!(Mana::max_for(Casting::Martial, &scores(18, 18)), 4.0);
        assert_eq!(Mana::max_for(Casting::Martial, &scores(3, 3)), 4.0);
        assert_eq!(Mana::max_for(Casting::Arcane, &scores(3, 3)), 4.0);
    }

    #[test]
    fn mana_refills_from_empty_in_under_a_minute_standing_and_slower_than_stamina() {
        let rules = defaults();
        let mut mana = Mana { points: 0.0, max: 25.0 };
        mana.step(rules.mana.stand_refill_seconds - 1.0, Activity::Resting);
        assert!(mana.fraction() < 1.0);
        mana.step(1.0, Activity::Resting);
        assert!((mana.points - 25.0).abs() < 1e-9);
        assert!(rules.mana.stand_refill_seconds < 60.0);
        assert!(rules.mana.stand_refill_seconds > rules.stamina.stand_refill_seconds, "slower than stamina");
        assert!(rules.mana.walk_refill_seconds > rules.stamina.walk_refill_seconds);
    }

    #[test]
    fn mana_refill_gear_scales_every_rate() {
        let rules = defaults();
        for activity in [Activity::Walking, Activity::Resting, Activity::Sleeping] {
            let usual = Mana::refill_per_second_with(rules, activity);
            assert!(usual > 0.0, "{activity:?}");
            assert!((Mana::refill_per_second_geared(rules, activity, 1.5) - usual * 1.5).abs() < 1e-12);
        }
        let mut quick = Mana { points: 0.0, max: 10.0 };
        quick.step_geared(rules, 5.0, Activity::Resting, 2.0);
        let mut usual = Mana { points: 0.0, max: 10.0 };
        usual.step(10.0, Activity::Resting);
        assert!((quick.points - usual.points).abs() < 1e-9);
        assert_eq!(Mana::refill_per_second_geared(rules, Activity::Exerting, 2.0), 0.0);
    }

    #[test]
    fn sleeping_refills_faster_than_resting_and_exerting_not_at_all() {
        let from_empty = |activity| {
            let mut mana = Mana { points: 0.0, max: 10.0 };
            mana.step(1.0, activity);
            mana.points
        };
        assert!(from_empty(Activity::Sleeping) > from_empty(Activity::Resting));
        assert!(from_empty(Activity::Resting) > from_empty(Activity::Walking));
        assert_eq!(from_empty(Activity::Exerting), 0.0);
    }

    #[test]
    fn mana_steps_the_same_in_any_step_size_and_resizing_keeps_how_full_it_is() {
        let (mut whole, mut parts) = (Mana { points: 2.0, max: 10.0 }, Mana { points: 2.0, max: 10.0 });
        whole.step(3.0, Activity::Resting);
        for _ in 0..30 {
            parts.step(0.1, Activity::Resting);
        }
        assert!((whole.points - parts.points).abs() < 1e-9);
        let mut mana = Mana { points: 5.0, max: 10.0 };
        mana.resize(25.0);
        assert_eq!((mana.points, mana.max), (12.5, 25.0));
    }

    #[test]
    fn mana_is_spent_only_when_the_pool_holds_it() {
        let mut mana = Mana::full(10.0);
        assert!(mana.spend(4.0));
        assert!(!mana.spend(7.0));
        assert_eq!(mana.points, 6.0);
    }
    #[test]
    fn a_delver_standing_in_the_dark_is_hardly_seen_and_a_torch_shows_them() {
        let dark = visibility(0.0, 0.0, LoadBand::Light, 0.0);
        let torch = visibility(1.0, 0.0, LoadBand::Light, 0.0);
        assert!(dark < 0.1, "{dark}");
        assert!(torch > 0.8, "{torch}");
    }

    #[test]
    fn light_speed_and_bulk_each_raise_visibility_and_stealth_lowers_it() {
        let base = visibility(0.4, 0.0, LoadBand::Light, 0.0);
        assert!(visibility(0.8, 0.0, LoadBand::Light, 0.0) > base);
        assert!(visibility(0.4, 1.0, LoadBand::Light, 0.0) > base);
        assert!(visibility(0.4, 1.5, LoadBand::Light, 0.0) > visibility(0.4, 1.0, LoadBand::Light, 0.0));
        assert!(visibility(0.4, 0.0, LoadBand::Burdened, 0.0) > base);
        assert!(visibility(0.4, 0.0, LoadBand::Encumbered, 0.0) > visibility(0.4, 0.0, LoadBand::Burdened, 0.0));
        let silent = visibility(0.4, 0.0, LoadBand::Light, 2.0);
        assert!((base - silent - 0.1).abs() < 1e-9, "Stealth +2 is a tenth less: {base} against {silent}");
        assert!(visibility(0.4, 0.0, LoadBand::Light, -2.0) > base, "a negative Stealth shows them more");
    }

    #[test]
    fn visibility_stays_between_none_and_plain_as_day() {
        assert_eq!(visibility(0.0, 0.0, LoadBand::Light, 20.0), 0.0);
        assert_eq!(visibility(5.0, 9.0, LoadBand::Overloaded, -20.0), 1.0);
        assert_eq!(visibility(-1.0, -1.0, LoadBand::Light, 0.0), visibility(0.0, 0.0, LoadBand::Light, 0.0));
    }

    #[test]
    fn gear_sums_its_stealth() {
        let worn = [Worn { stealth: 2.0, ..Worn::default() }, Worn { stealth: -1.0, ..Worn::default() }, Worn::default()];
        assert_eq!(Gear::new(0.0, worn, []).stealth, 1.0);
        assert_eq!(Gear::default().stealth, 0.0);
    }

    #[test]
    fn a_full_pool_lasts_thirty_seconds_of_sprinting_while_light() {
        let mut stamina = Stamina::default();
        stamina.step(29.0, Activity::Exerting, Burden::default());
        assert!(stamina.value > 0.0 && !stamina.winded, "{stamina:?}");
        stamina.step(1.0, Activity::Exerting, Burden::default());
        assert!(stamina.value.abs() < 1e-9 && stamina.winded, "{stamina:?}");
    }

    #[test]
    fn the_heavier_the_load_the_faster_sprinting_drains() {
        let after = |band| {
            let mut stamina = Stamina::default();
            stamina.step(10.0, Activity::Exerting, band);
            stamina.value
        };
        let (light, burdened, encumbered) = (after(Burden::default()), after(Burden::new(82.5, 165.0)), after(Burden::new(165.0, 165.0)));
        assert!(light > burdened && burdened > encumbered, "{light} {burdened} {encumbered}");
        assert!((1.0 - encumbered - 2.0 * (1.0 - light)).abs() < 1e-9);
    }

    #[test]
    fn walking_stamina_fades_from_a_refill_to_a_slow_drain_with_no_jump() {
        let rate = |lb: f64| Stamina::rate_per_second(Activity::Walking, Burden::new(lb, 165.0));
        // STR 11: refills a pool in 12 seconds up to 55 lb.
        assert!((rate(55.0) - 1.0 / 12.0).abs() < 1e-12);
        assert!((rate(56.0) - 0.083326).abs() < 1e-6, "{}", rate(56.0));
        assert!((rate(110.0) - 0.058333).abs() < 1e-6, "{}", rate(110.0));
        assert!((rate(165.0) + 1.0 / 60.0).abs() < 1e-12, "a full pool lasts a minute at capacity");
        let mut last = rate(0.0);
        let mut turned = None;
        for pounds in 1..=165 {
            let now = rate(pounds as f64);
            assert!((now - last).abs() < 0.004, "{pounds} lb: {last} to {now}");
            if turned.is_none() && now < 0.0 {
                turned = Some(pounds);
            }
            last = now;
        }
        assert_eq!(turned, Some(156), "only deep into Encumbered does walking drain");
    }

    #[test]
    fn standing_refills_from_empty_in_eight_seconds_and_walking_is_slower() {
        let empty = Stamina { value: 0.0, winded: true };
        let mut standing = empty;
        standing.step(8.0, Activity::Resting, Burden::default());
        assert!((standing.value - 1.0).abs() < 1e-9 && !standing.winded);
        let mut walking = empty;
        walking.step(8.0, Activity::Walking, Burden::default());
        assert!(walking.value > 0.5 && walking.value < 1.0);
    }

    #[test]
    fn gear_quickens_the_refill_and_slows_the_sprint_drain() {
        let rules = defaults();
        let empty = Stamina { value: 0.0, winded: true };
        let mut standing = empty;
        standing.step_geared(rules, 8.0 / 1.3, Activity::Resting, Burden::default(), 1.3, 1.0);
        assert!((standing.value - 1.0).abs() < 1e-9, "of Second Wind fills a bar in 8 / 1.3 s: {standing:?}");
        let mut sprinting = Stamina::default();
        sprinting.step_geared(rules, 10.0, Activity::Exerting, Burden::default(), 1.0, 0.8);
        assert!((sprinting.value - (1.0 - 10.0 * 0.8 / 30.0)).abs() < 1e-9, "{sprinting:?}");
        let mut plain = Stamina::default();
        plain.step_geared(rules, 10.0, Activity::Exerting, Burden::default(), 1.3, 1.0);
        let mut again = Stamina::default();
        again.step_with(rules, 10.0, Activity::Exerting, Burden::default());
        assert_eq!(plain, again, "refill never touches the drain");
    }

    #[test]
    fn a_winded_delver_cannot_exert_until_twenty_percent() {
        let mut stamina = Stamina { value: 0.0, winded: true };
        assert!(!stamina.can_exert());
        stamina.step(1.5, Activity::Resting, Burden::default());
        assert!(stamina.value < SECOND_WIND && !stamina.can_exert(), "{stamina:?}");
        stamina.step(0.2, Activity::Resting, Burden::default());
        assert!(stamina.value >= SECOND_WIND && stamina.can_exert(), "{stamina:?}");
    }

    #[test]
    fn a_swing_takes_a_fixed_bite_and_the_last_one_winds() {
        let mut stamina = Stamina::default();
        stamina.spend(SWING_COST);
        assert!((stamina.value - (1.0 - SWING_COST)).abs() < 1e-9);
        for _ in 0..20 {
            stamina.spend(SWING_COST);
        }
        assert_eq!(stamina.value, 0.0);
        assert!(stamina.winded);
    }

    #[test]
    fn stamina_steps_the_same_in_any_step_size() {
        let (mut whole, mut parts) = (Stamina::default(), Stamina::default());
        whole.step(12.0, Activity::Exerting, Burden::new(110.0, 165.0));
        for _ in 0..120 {
            parts.step(0.1, Activity::Exerting, Burden::new(110.0, 165.0));
        }
        assert!((whole.value - parts.value).abs() < 1e-9);
    }
}
