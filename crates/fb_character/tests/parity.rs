//! `Rules::default()` plays exactly as the constants did before they moved into a rules file.
//!
//! `scenario` rolls fixed seeds through the combat dice and steps the needs and exhaustion
//! through days of every activity, the stamina, the mana, the load and the visibility, and
//! digests every number it produced down to the bit. `GOLDEN` was taken by running it on the
//! functions as they were with the constants in the code; the plain functions and the `_with`
//! ones under `Rules::default()` must both still give it.

#[path = "parity/plain.rs"]
mod plain;
#[path = "parity/scenario.rs"]
mod scenario;

use fb_character::combat::{self, Attack, Bite, Venom};
use fb_character::explain;
use fb_character::rules::Rules;
use fb_character::{Activity, Attributes, Casting, Exhaustion, ExhaustionTimes, Burden, LoadBand, Mana, Needs, Stamina, capacity_lb_with, visibility_with};
use procgen_rng::Mulberry32;
use scenario::{Book, run};

/// The digest of the scenario's 3028 numbers on the code before the move. Taken again from
/// main's plain functions when main added the proficiency bonus to attacks (#359): the rules
/// file must play as main does, not as the code did before that change. Taken again when load pace
/// and sprint drain became a curve of the load rather than steps by band, and again when walking stamina followed the load.
const GOLDEN: u64 = 0xff34_485f_05d4_5411;
const GOLDEN_LEN: usize = 3028;

/// The scenario through the `_with` functions, under a rules table.
struct With(Rules);

impl Book for With {
    fn creature_attack(&self, bonus: i32, ac: i32, rng: &mut Mulberry32) -> Attack {
        combat::creature_attack_with(&self.0, bonus, ac, rng)
    }
    fn folk_attack(&self, strength: i32, to_hit: i32, ac: i32, rng: &mut Mulberry32) -> Attack {
        combat::folk_attack_with(&self.0, strength, to_hit, ac, rng)
    }
    fn bite(&self, damage: u32, poison: u32, con: i32, rng: &mut Mulberry32) -> Bite {
        combat::bite_damage_with(&self.0, damage, poison, con, rng)
    }
    fn venom(&self, poison: u32, con: i32, rng: &mut Mulberry32) -> Venom {
        combat::venom_after_save_with(&self.0, poison, con, rng)
    }
    fn armour_class(&self, dexterity: i32, armour: &[f32]) -> f64 {
        explain::armour_class_with(&self.0, dexterity, armour.iter().map(|a| ("armour", *a))).value()
    }
    fn new_needs(&self) -> Needs {
        // Built the way the game builds a delver's: the rules put their rates on the meters.
        let mut needs = Needs::default();
        self.0.apply_to(&mut needs);
        needs
    }
    fn step_times(&self, times: &mut ExhaustionTimes, needs: &mut Needs, exhaustion: &mut Exhaustion, activity: Activity, hours: f64, con: i32) {
        times.step_with(&self.0, needs, exhaustion, activity, hours, con);
    }
    fn band_word(&self, needs: &Needs) -> [usize; 3] {
        [needs.hunger.band_with(&self.0), needs.thirst.band_with(&self.0), needs.sleepiness.band_with(&self.0)]
    }
    fn capacity(&self, strength: i32, carry: f64) -> f64 {
        capacity_lb_with(&self.0, strength, carry)
    }
    fn load_band(&self, load: f64, capacity: f64) -> LoadBand {
        LoadBand::from_load_with(&self.0, load, capacity)
    }
    fn walk(&self, load: f64, capacity: f64) -> f32 {
        Burden::new(load, capacity).walk_multiplier_with(&self.0)
    }
    fn speed(&self, exhaustion: Exhaustion) -> f32 {
        exhaustion.speed_multiplier_with(&self.0)
    }
    fn stamina_step(&self, stamina: &mut Stamina, seconds: f64, activity: Activity, load: Burden) {
        stamina.step_with(&self.0, seconds, activity, load);
    }
    fn stamina_spend(&self, stamina: &mut Stamina, cost: f64) {
        stamina.spend_with(&self.0, cost);
    }
    fn mana_max(&self, casting: Casting, attributes: &Attributes) -> f64 {
        Mana::max_for_with(&self.0, casting, attributes)
    }
    fn mana_step(&self, mana: &mut Mana, hours: f64, activity: Activity) {
        mana.step_with(&self.0, hours, activity);
    }
    fn visibility(&self, light: f64, pace: f64, load: LoadBand, stealth: f64) -> f64 {
        visibility_with(&self.0, light, pace, load, stealth)
    }
}

#[test]
fn the_plain_functions_give_the_numbers_from_before_the_move() {
    let run = run(&plain::Plain);
    assert_eq!(run.values.len(), GOLDEN_LEN);
    assert_eq!(run.digest(), GOLDEN);
}

#[test]
fn the_default_rules_give_the_numbers_from_before_the_move() {
    let run = run(&With(Rules::default()));
    assert_eq!(run.values.len(), GOLDEN_LEN);
    assert_eq!(run.digest(), GOLDEN);
}

#[test]
fn the_shipped_file_gives_the_numbers_from_before_the_move() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules.toml")).unwrap();
    let rules: Rules = toml::from_str(&text).unwrap();
    assert_eq!(run(&With(rules)).digest(), GOLDEN);
}

#[test]
fn a_changed_rule_changes_the_numbers() {
    let mut rules = Rules::default();
    rules.needs.hunger_hours_to_full = 12.0;
    assert_ne!(run(&With(rules)).digest(), GOLDEN);
    let mut rules = Rules::default();
    rules.stamina.sprint_seconds = 20.0;
    assert_ne!(run(&With(rules)).digest(), GOLDEN);
    let mut rules = Rules::default();
    rules.combat.natural_hit = 10;
    assert_ne!(run(&With(rules)).digest(), GOLDEN);
}
