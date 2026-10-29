// The scenario through the plain functions: the ones that existed before the rules file, and
// now run over `Rules::default()`.

use fb_character::combat::{self, Attack, Bite, Venom};
use fb_character::explain;
use fb_character::{Activity, Attributes, Casting, Exhaustion, ExhaustionTimes, Burden, LoadBand, Mana, Needs, Stamina, capacity_lb, visibility};
use procgen_rng::Mulberry32;

use super::scenario::Book;

pub struct Plain;

impl Book for Plain {
    fn creature_attack(&self, bonus: i32, ac: i32, rng: &mut Mulberry32) -> Attack {
        combat::creature_attack(bonus, ac, rng)
    }
    fn folk_attack(&self, strength: i32, to_hit: i32, ac: i32, rng: &mut Mulberry32) -> Attack {
        combat::folk_attack(strength, to_hit, ac, rng)
    }
    fn bite(&self, damage: u32, poison: u32, con: i32, rng: &mut Mulberry32) -> Bite {
        combat::bite_damage(damage, poison, con, rng)
    }
    fn venom(&self, poison: u32, con: i32, rng: &mut Mulberry32) -> Venom {
        combat::venom_after_save(poison, con, rng)
    }
    fn armour_class(&self, dexterity: i32, armour: &[f32]) -> f64 {
        explain::armour_class(dexterity, armour.iter().map(|a| ("armour", *a))).value()
    }
    fn new_needs(&self) -> Needs {
        Needs::default()
    }
    fn step_times(&self, times: &mut ExhaustionTimes, needs: &mut Needs, exhaustion: &mut Exhaustion, activity: Activity, hours: f64, con: i32) {
        times.step(needs, exhaustion, activity, hours, con);
    }
    fn band_word(&self, needs: &Needs) -> [usize; 3] {
        [needs.hunger.band(), needs.thirst.band(), needs.sleepiness.band()]
    }
    fn capacity(&self, strength: i32, carry: f64) -> f64 {
        capacity_lb(strength, carry)
    }
    fn load_band(&self, load: f64, capacity: f64) -> LoadBand {
        LoadBand::from_load(load, capacity)
    }
    fn walk(&self, load: f64, capacity: f64) -> f32 {
        Burden::new(load, capacity).walk_multiplier()
    }
    fn speed(&self, exhaustion: Exhaustion) -> f32 {
        exhaustion.speed_multiplier()
    }
    fn stamina_step(&self, stamina: &mut Stamina, seconds: f64, activity: Activity, load: Burden) {
        stamina.step(seconds, activity, load);
    }
    fn stamina_spend(&self, stamina: &mut Stamina, cost: f64) {
        stamina.spend(cost);
    }
    fn mana_max(&self, casting: Casting, attributes: &Attributes) -> f64 {
        Mana::max_for(casting, attributes)
    }
    fn mana_step(&self, mana: &mut Mana, seconds: f64, activity: Activity) {
        mana.step(seconds, activity);
    }
    fn visibility(&self, light: f64, pace: f64, load: LoadBand, stealth: f64) -> f64 {
        visibility(light, pace, load, stealth)
    }
}
