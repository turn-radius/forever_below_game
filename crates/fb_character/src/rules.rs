//! The character rules as one table of numbers: every constant the combat dice, the needs, the
//! exhaustion, the stamina and mana pools, the load and the visibility run on, plus the
//! creatures' stat blocks. `data/rules.toml` holds the defaults, and the game and the lab read
//! the same [`Rules`], so a designer changes a number in one place and sees it play.
//!
//! Every struct is `#[serde(default)]`, so a file that names only the numbers it changes works:
//! the rest stay as [`Rules::default`] has them. The rule functions of this crate each have a
//! `_with` variant that takes `&Rules`; the plain functions are those over [`Rules::default`],
//! so the code that never loads a file plays exactly as before.
//!
//! Units: rates are written the way a designer thinks of them. A need's rate is the game hours
//! of walking from empty to full (`hunger_hours_to_full = 16`), whose reciprocal is the per hour
//! the meters step by. The stamina is in real seconds, the mana's refill a share of the pool per
//! game hour, the exhaustion in game hours.
//!
//! Creatures are keyed by the name of their stat block (`"Giant Rat"`). The defaults come from
//! [`fb_creatures::Species::stat_block`], which stays where it is, so a species added there has
//! its rules without touching this file; the file only overrides ([`Rules::creature`]).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use fb_creatures::{Dice, Species, StatBlock};
use serde::{Deserialize, Deserializer, Serialize};

use crate::effect::EffectDef;
use crate::{Activity, LoadBand, MeterKind};

/// Every number the character rules run on. See the [module docs](self).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    pub combat: CombatRules,
    pub needs: NeedsRules,
    pub exhaustion: ExhaustionRules,
    pub stamina: StaminaRules,
    pub mana: ManaRules,
    pub load: LoadRules,
    pub visibility: VisibilityRules,
    /// The creatures' numbers by stat block name. A partial entry keeps the rest of that
    /// creature's defaults; a name no species has is a creature of its own, flat zeros where
    /// the entry is silent.
    #[serde(deserialize_with = "creatures_over_defaults")]
    pub creatures: BTreeMap<String, CreatureRules>,
    /// The effects by id ([`crate::effect`]), from `data/effects.json`. An entry here changes
    /// only the keys it names; saving writes the table only when it differs from the file.
    #[serde(deserialize_with = "effects_over_defaults", skip_serializing_if = "is_shipped")]
    pub effects: BTreeMap<String, EffectDef>,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            combat: CombatRules::default(),
            needs: NeedsRules::default(),
            exhaustion: ExhaustionRules::default(),
            stamina: StaminaRules::default(),
            mana: ManaRules::default(),
            load: LoadRules::default(),
            visibility: VisibilityRules::default(),
            creatures: default_creatures(),
            effects: crate::effect::shipped(),
        }
    }
}

/// The rules every plain (non `_with`) function runs on: [`Rules::default`], built once.
pub fn defaults() -> &'static Rules {
    static DEFAULTS: LazyLock<Rules> = LazyLock::new(Rules::default);
    &DEFAULTS
}

impl Rules {
    /// The SRD stat block of `species` with this file's numbers over it.
    pub fn creature(&self, species: Species) -> StatBlock {
        let mut block = species.stat_block();
        if let Some(rules) = self.creatures.get(block.name) {
            block.armour_class = rules.armour_class;
            block.attack_bonus = rules.attack_bonus;
            block.hit_points = rules.hit_points;
            block.hit_dice = rules.hit_dice;
            block.bite = rules.bite;
            block.venom = rules.venom();
        }
        block
    }

    /// Puts this file's meter rates on `needs`, keeping their values and gear multipliers. The
    /// rate rides on the meter ([`crate::Meter::rate_per_hour`]), so the game does this when
    /// the rules change.
    pub fn apply_to(&self, needs: &mut crate::Needs) {
        needs.hunger.rate_per_hour = self.needs.base_rate(MeterKind::Hunger);
        needs.thirst.rate_per_hour = self.needs.base_rate(MeterKind::Thirst);
        needs.sleepiness.rate_per_hour = self.needs.base_rate(MeterKind::Sleepiness);
    }
}

/// One creature's numbers: what its SRD stat block prints and combat uses.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureRules {
    /// Armour Class: what an attack roll must reach to hit it.
    pub armour_class: i32,
    /// What the bite adds to its attack roll.
    pub attack_bonus: i32,
    /// The average hit points the block prints.
    pub hit_points: u32,
    /// The dice those come from.
    pub hit_dice: Dice,
    /// The bite's damage on a hit.
    pub bite: Dice,
    /// Poison the bite adds, halved on a successful save; no dice and no bonus is none.
    pub venom: Dice,
}

impl CreatureRules {
    /// The numbers of a stat block.
    pub fn of(block: &StatBlock) -> CreatureRules {
        CreatureRules {
            armour_class: block.armour_class,
            attack_bonus: block.attack_bonus,
            hit_points: block.hit_points,
            hit_dice: block.hit_dice,
            bite: block.bite,
            venom: block.venom.unwrap_or(Dice::flat(0)),
        }
    }

    /// The venom, or `None` when it is no dice and no bonus.
    pub fn venom(&self) -> Option<Dice> {
        (self.venom.count > 0 || self.venom.bonus != 0).then_some(self.venom)
    }
}

fn default_creatures() -> BTreeMap<String, CreatureRules> {
    Species::ALL.iter().map(|s| (s.stat_block().name.to_string(), CreatureRules::of(&s.stat_block()))).collect()
}

/// A creature entry as a file may write it: any of its numbers.
#[derive(Deserialize)]
struct PartialCreature {
    armour_class: Option<i32>,
    attack_bonus: Option<i32>,
    hit_points: Option<u32>,
    hit_dice: Option<Dice>,
    bite: Option<Dice>,
    venom: Option<Dice>,
}

/// Reads the creatures table over the defaults: a creature the file leaves out keeps its
/// numbers, and one it names only in part keeps the rest.
fn creatures_over_defaults<'de, D: Deserializer<'de>>(deserializer: D) -> Result<BTreeMap<String, CreatureRules>, D::Error> {
    let file = BTreeMap::<String, PartialCreature>::deserialize(deserializer)?;
    let mut creatures = default_creatures();
    for (name, part) in file {
        let own = creatures.entry(name).or_insert(CreatureRules {
            armour_class: 10,
            attack_bonus: 0,
            hit_points: 1,
            hit_dice: Dice::flat(0),
            bite: Dice::flat(0),
            venom: Dice::flat(0),
        });
        own.armour_class = part.armour_class.unwrap_or(own.armour_class);
        own.attack_bonus = part.attack_bonus.unwrap_or(own.attack_bonus);
        own.hit_points = part.hit_points.unwrap_or(own.hit_points);
        own.hit_dice = part.hit_dice.unwrap_or(own.hit_dice);
        own.bite = part.bite.unwrap_or(own.bite);
        own.venom = part.venom.unwrap_or(own.venom);
    }
    Ok(creatures)
}

/// Reads the effects table over `data/effects.json`: an effect the file leaves out keeps its
/// definition, one it names only in part keeps the rest, and a new id is an effect of its own.
fn effects_over_defaults<'de, D: Deserializer<'de>>(deserializer: D) -> Result<BTreeMap<String, EffectDef>, D::Error> {
    use serde::de::Error;
    let file = BTreeMap::<String, serde_json::Value>::deserialize(deserializer)?;
    let mut effects = crate::effect::shipped();
    for (id, part) in file {
        let mut merged = effects.get(&id).map_or(serde_json::json!({}), |d| serde_json::to_value(d).expect("an effect serialises"));
        if let (Some(into), serde_json::Value::Object(keys)) = (merged.as_object_mut(), part) {
            into.extend(keys);
        }
        effects.insert(id.clone(), serde_json::from_value(merged).map_err(|e| D::Error::custom(format!("effect {id}: {e}")))?);
    }
    Ok(effects)
}

/// Whether `effects` is what `data/effects.json` ships, so a saved rules file leaves it out.
fn is_shipped(effects: &BTreeMap<String, EffectDef>) -> bool {
    *effects == crate::effect::shipped()
}

/// Armour, the natural dice and the hit points of each class.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CombatRules {
    /// What every armour class starts from, before the DEX modifier and worn armour.
    pub armour_base: i32,
    /// A d20 that rolls this or more always hits (SRD 5.1 p. 96, "Attack Rolls").
    pub natural_hit: u32,
    /// A d20 that rolls this or less always misses.
    pub natural_miss: u32,
    /// The Constitution save against venom: the difficulty class (SRD 5.1 p. 386, "Poisonous
    /// Snake").
    pub venom_save_dc: i32,
    /// The hit die of each class, taken at its highest for a first-level delver (SRD 5.1
    /// "Hit Points at 1st Level": fighter p. 24, ranger p. 35, cleric p. 15, wizard p. 52 for
    /// the mage).
    pub hit_dice: HitDice,
    /// What a delver's or a folk's attack roll adds for proficiency with the weapon, a 1st-level
    /// character's +2 (SRD 5.1 p. 12, "Proficiency Bonus").
    pub proficiency_bonus: i32,
}

impl Default for CombatRules {
    fn default() -> Self {
        CombatRules {
            armour_base: 10,
            natural_hit: 20,
            natural_miss: 1,
            venom_save_dc: 10,
            hit_dice: HitDice::default(),
            proficiency_bonus: crate::combat::PROFICIENCY_BONUS,
        }
    }
}

/// The sides of each class's hit die.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HitDice {
    pub fighter: u32,
    pub ranger: u32,
    pub mage: u32,
    pub cleric: u32,
}

impl Default for HitDice {
    fn default() -> Self {
        HitDice { fighter: 10, ranger: 10, mage: 6, cleric: 8 }
    }
}

/// How quickly hunger, thirst and sleepiness rise, how activity and sleep change that, where
/// the bands begin and what each is called.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NeedsRules {
    /// Game hours of walking for hunger to go from empty to full.
    pub hunger_hours_to_full: f64,
    /// Game hours of walking for thirst to go from empty to full.
    pub thirst_hours_to_full: f64,
    /// Game hours of walking for sleepiness to go from empty to full.
    pub sleepiness_hours_to_full: f64,
    /// Multiple of the rate while resting.
    pub resting_rate: f64,
    /// Multiple of the rate while walking.
    pub walking_rate: f64,
    /// Multiple of the rate while exerting.
    pub exerting_rate: f64,
    /// Multiple of hunger's and thirst's rate while asleep.
    pub sleeping_rate: f64,
    /// Game hours of sleep for sleepiness to fall from full to empty.
    pub sleep_hours_to_rested: f64,
    /// Where each band after the first begins: the meter's value at the start of bands 1 to 4.
    pub band_edges: [f64; 4],
    pub hunger_words: [String; 5],
    pub thirst_words: [String; 5],
    pub sleepiness_words: [String; 5],
}

impl Default for NeedsRules {
    fn default() -> Self {
        let words = |kind: MeterKind| [0, 1, 2, 3, 4].map(|band| kind.word(band).to_string());
        NeedsRules {
            hunger_hours_to_full: 16.0,
            thirst_hours_to_full: 10.0,
            sleepiness_hours_to_full: 32.0,
            resting_rate: 0.5,
            walking_rate: 1.0,
            exerting_rate: 2.0,
            sleeping_rate: 0.5,
            sleep_hours_to_rested: 8.0,
            band_edges: [0.25, 0.5, 0.75, 1.0],
            hunger_words: words(MeterKind::Hunger),
            thirst_words: words(MeterKind::Thirst),
            sleepiness_words: words(MeterKind::Sleepiness),
        }
    }
}

impl NeedsRules {
    /// The meter's rise per game hour while walking, before gear.
    pub fn base_rate(&self, kind: MeterKind) -> f64 {
        1.0 / match kind {
            MeterKind::Hunger => self.hunger_hours_to_full,
            MeterKind::Thirst => self.thirst_hours_to_full,
            MeterKind::Sleepiness => self.sleepiness_hours_to_full,
        }
    }

    /// The multiple of the rate at `activity`.
    pub fn activity_rate(&self, activity: Activity) -> f64 {
        match activity {
            Activity::Resting => self.resting_rate,
            Activity::Walking => self.walking_rate,
            Activity::Exerting => self.exerting_rate,
            Activity::Sleeping => self.walking_rate,
        }
    }

    /// The band (0 to 4) a meter at `value` is in.
    pub fn band(&self, value: f64) -> usize {
        self.band_edges.iter().filter(|&&edge| value >= edge).count()
    }

    /// The word for a `kind` meter at `band` (0 to 4).
    pub fn word(&self, kind: MeterKind, band: usize) -> &str {
        let words = match kind {
            MeterKind::Hunger => &self.hunger_words,
            MeterKind::Thirst => &self.thirst_words,
            MeterKind::Sleepiness => &self.sleepiness_words,
        };
        &words[band.min(4)]
    }
}

/// When time in the worst bands earns exhaustion, what sleep takes off, and how it slows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExhaustionRules {
    /// The meter value where Famished and Dry begin: a sleep is broken from here (our own).
    pub famished_at: f64,
    /// Game hours of continuous sleep that remove one level.
    pub sleep_hours_to_recover: f64,
    /// Starving: the first level comes after this many days plus the CON modifier ...
    pub starving_first_days: f64,
    /// ... but never after fewer than this many days.
    pub starving_first_days_least: f64,
    /// Starving: one more level every this many game hours.
    pub starving_every_hours: f64,
    /// Dehydrated: the first level after this many game hours ...
    pub dehydrated_first_hours: f64,
    /// ... and one more every this many.
    pub dehydrated_every_hours: f64,
    /// Spent: the first level after this many game hours ...
    pub spent_first_hours: f64,
    /// ... and one more every this many.
    pub spent_every_hours: f64,
    /// From this level the delver moves at [`slowed_speed`](Self::slowed_speed) (SRD 5.1
    /// p. 358: level 2, speed halved).
    pub slowed_from: u32,
    /// The speed multiple from [`slowed_from`](Self::slowed_from).
    pub slowed_speed: f32,
    /// From this level the delver cannot move (SRD 5.1 p. 358: level 5, speed 0).
    pub halted_from: u32,
}

impl Default for ExhaustionRules {
    fn default() -> Self {
        ExhaustionRules {
            famished_at: 0.75,
            sleep_hours_to_recover: 8.0,
            starving_first_days: 3.0,
            starving_first_days_least: 1.0,
            starving_every_hours: 24.0,
            dehydrated_first_hours: 24.0,
            dehydrated_every_hours: 12.0,
            spent_first_hours: 24.0,
            spent_every_hours: 24.0,
            slowed_from: 2,
            slowed_speed: 0.5,
            halted_from: 5,
        }
    }
}

/// The short-term pool that sprinting and swinging drain, in real seconds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StaminaRules {
    /// Seconds a full pool lasts sprinting while Light.
    pub sprint_seconds: f64,
    /// Sprinting drains this many times as fast Burdened ...
    pub burdened_drain: f64,
    /// ... and Encumbered or Overloaded.
    pub encumbered_drain: f64,
    /// Seconds to refill from empty standing still.
    pub stand_refill_seconds: f64,
    /// Seconds to refill from empty while walking.
    pub walk_refill_seconds: f64,
    /// Sleeping refills this many times as fast as standing.
    pub sleep_refill_factor: f64,
    /// A winded delver may sprint and swing again from this much stamina.
    pub second_wind: f64,
    /// What one swing costs, of a full pool.
    pub swing_cost: f64,
}

impl Default for StaminaRules {
    fn default() -> Self {
        StaminaRules {
            sprint_seconds: 30.0,
            burdened_drain: 1.5,
            encumbered_drain: 2.0,
            stand_refill_seconds: 8.0,
            walk_refill_seconds: 12.0,
            sleep_refill_factor: 2.0,
            second_wind: 0.2,
            swing_cost: 0.08,
        }
    }
}

/// The pool spells draw on, in mana points, and how it refills.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ManaRules {
    /// The small pool every martial character has.
    pub martial_pool: f64,
    /// The pool of a caster with an ability modifier of 0.
    pub caster_base: f64,
    /// What each point of the ability modifier adds to a caster's pool.
    pub caster_per_modifier: f64,
    /// A caster's pool is never smaller than this.
    pub caster_floor: f64,
    /// Of a full pool, what a Resting delver regains each game hour.
    pub rest_per_hour: f64,
    /// Of a full pool, what a walking delver regains each game hour.
    pub walk_per_hour: f64,
    /// Of a full pool, what a sleeping delver regains each game hour.
    pub sleep_per_hour: f64,
}

impl Default for ManaRules {
    fn default() -> Self {
        ManaRules { martial_pool: 4.0, caster_base: 10.0, caster_per_modifier: 5.0, caster_floor: 4.0, rest_per_hour: 1.0 / 8.0, walk_per_hour: 1.0 / 32.0, sleep_per_hour: 3.0 / 8.0 }
    }
}

/// What a delver can carry and how the weight slows them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoadRules {
    /// Pounds of capacity for each point of STR (SRD 5.1 p. 79).
    pub capacity_per_strength: f64,
    /// Burdened from this share of the capacity.
    pub burdened_from: f64,
    /// Encumbered from this share of the capacity.
    pub encumbered_from: f64,
    /// Overloaded above this share of the capacity.
    pub overloaded_over: f64,
    /// Walking pace multiple while Burdened.
    pub burdened_walk: f32,
    /// Walking pace multiple while Encumbered.
    pub encumbered_walk: f32,
    /// Walking pace multiple while Overloaded: 0 is rooted.
    pub overloaded_walk: f32,
}

impl Default for LoadRules {
    fn default() -> Self {
        LoadRules { capacity_per_strength: 15.0, burdened_from: 0.34, encumbered_from: 0.67, overloaded_over: 1.0, burdened_walk: 0.85, encumbered_walk: 0.65, overloaded_walk: 0.0 }
    }
}

impl LoadRules {
    /// The walking pace multiple of `band`.
    pub fn walk(&self, band: LoadBand) -> f32 {
        match band {
            LoadBand::Light => 1.0,
            LoadBand::Burdened => self.burdened_walk,
            LoadBand::Encumbered => self.encumbered_walk,
            LoadBand::Overloaded => self.overloaded_walk,
        }
    }
}

/// How easy the delver is to see, from 0 to 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VisibilityRules {
    /// How visible a delver is in the dark: the eye is nearly closed.
    pub in_dark: f64,
    /// What full light on the delver adds.
    pub in_light: f64,
    /// What sprinting adds (at the sprint pace).
    pub moving: f64,
    /// The pace, as a multiple of walking pace, the gait is capped at.
    pub sprint_pace: f64,
    /// What each point of Stealth takes off.
    pub stealth_step: f64,
    /// What a Burdened load adds.
    pub burdened_bulk: f64,
    /// What an Encumbered or Overloaded load adds.
    pub encumbered_bulk: f64,
}

impl Default for VisibilityRules {
    fn default() -> Self {
        VisibilityRules { in_dark: 0.05, in_light: 0.8, moving: 0.15, sprint_pace: 1.5, stealth_step: 0.05, burdened_bulk: 0.04, encumbered_bulk: 0.08 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> Rules {
        toml::from_str(include_str!("../../../data/rules.toml")).unwrap()
    }

    #[test]
    fn the_shipped_file_is_every_default() {
        assert_eq!(shipped(), Rules::default());
    }

    #[test]
    fn the_defaults_write_to_toml_and_read_back_equal() {
        let text = toml::to_string_pretty(&Rules::default()).unwrap();
        let back: Rules = toml::from_str(&text).unwrap();
        assert_eq!(back, Rules::default());
        let mut changed = Rules::default();
        changed.needs.thirst_hours_to_full = 7.5;
        changed.creatures.get_mut("Rat").unwrap().armour_class = 14;
        changed.creatures.get_mut("Poisonous Snake").unwrap().venom = Dice::flat(0);
        assert_eq!(toml::from_str::<Rules>(&toml::to_string_pretty(&changed).unwrap()).unwrap(), changed);
    }

    #[test]
    fn a_partial_file_keeps_the_rest_of_the_defaults() {
        let rules: Rules = toml::from_str("[needs]\nhunger_hours_to_full = 12.0\n[creatures.\"Giant Rat\"]\narmour_class = 14\n").unwrap();
        let mut want = Rules::default();
        want.needs.hunger_hours_to_full = 12.0;
        want.creatures.get_mut("Giant Rat").unwrap().armour_class = 14;
        assert_eq!(rules, want);
        assert_eq!(toml::from_str::<Rules>("").unwrap(), Rules::default());
    }

    #[test]
    fn the_creature_lookup_is_the_stat_block_with_the_file_over_it() {
        for species in Species::ALL {
            assert_eq!(Rules::default().creature(species), species.stat_block());
        }
        let rules: Rules = toml::from_str("[creatures.\"Poisonous Snake\"]\narmour_class = 9\nvenom = { count = 0, sides = 0, bonus = 0 }\n").unwrap();
        let snake = rules.creature(Species::PoisonousSnake);
        assert_eq!((snake.armour_class, snake.venom, snake.hit_points, snake.attack_bonus), (9, None, 2, 5));
        assert_eq!(snake.name, "Poisonous Snake");
        assert_eq!(snake.speed_feet, 30.0, "what the rules do not hold stays the stat block's");
    }

    #[test]
    fn hours_to_full_are_the_per_hour_rates_of_before() {
        let needs = NeedsRules::default();
        assert_eq!(needs.base_rate(MeterKind::Hunger), 1.0 / 16.0);
        assert_eq!(needs.base_rate(MeterKind::Thirst), 1.0 / 10.0);
        assert_eq!(needs.base_rate(MeterKind::Sleepiness), 1.0 / 32.0);
    }

    #[test]
    fn editing_the_hours_changes_the_meters_rates() {
        let mut rules = Rules::default();
        rules.needs.hunger_hours_to_full = 8.0;
        let mut needs = crate::Needs::default();
        rules.apply_to(&mut needs);
        assert_eq!(needs.hunger.rate_per_hour, 0.125);
        needs.step_with(&rules, 4.0, Activity::Walking);
        assert_eq!(needs.hunger.value, 0.5);
        assert_eq!(needs.thirst.value, 0.4, "the others stay as they were");
    }

    #[test]
    fn the_words_and_edges_come_from_the_rules() {
        let mut rules = Rules::default();
        rules.needs.hunger_words[2] = "Ravenous".to_string();
        rules.needs.band_edges = [0.1, 0.2, 0.3, 0.4];
        let mut meter = crate::Meter::new(MeterKind::Hunger);
        meter.value = 0.25;
        assert_eq!(meter.band_with(&rules), 2);
        assert_eq!(meter.word_with(&rules), "Ravenous");
        assert_eq!(meter.word(), "Peckish", "the plain word is the default's");
    }
}
