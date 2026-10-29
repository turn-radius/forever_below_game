// A fixed scenario that rolls and steps every rule of the crate, written against a [`Book`] so
// the same run can go through the plain functions (which the rules file replaced) and the
// `_with` ones under `Rules::default()`. Its digest was taken from the plain functions before
// the rules moved into a file; `tests/parity.rs` holds it.

use fb_character::combat::{Attack, Bite, Venom};
use fb_character::{Activity, Attributes, Casting, Exhaustion, ExhaustionTimes, Burden, Gear, LoadBand, Mana, Needs, Stamina};
use procgen_rng::Mulberry32;

/// The rule functions the scenario calls.
pub trait Book {
    fn creature_attack(&self, bonus: i32, ac: i32, rng: &mut Mulberry32) -> Attack;
    fn folk_attack(&self, strength: i32, to_hit: i32, ac: i32, rng: &mut Mulberry32) -> Attack;
    fn bite(&self, damage: u32, poison: u32, con: i32, rng: &mut Mulberry32) -> Bite;
    fn venom(&self, poison: u32, con: i32, rng: &mut Mulberry32) -> Venom;
    fn armour_class(&self, dexterity: i32, armour: &[f32]) -> f64;
    fn new_needs(&self) -> Needs;
    fn step_times(&self, times: &mut ExhaustionTimes, needs: &mut Needs, exhaustion: &mut Exhaustion, activity: Activity, hours: f64, con: i32);
    fn band_word(&self, needs: &Needs) -> [usize; 3];
    fn capacity(&self, strength: i32, carry: f64) -> f64;
    fn load_band(&self, load: f64, capacity: f64) -> LoadBand;
    fn walk(&self, load: f64, capacity: f64) -> f32;
    fn speed(&self, exhaustion: Exhaustion) -> f32;
    fn stamina_step(&self, stamina: &mut Stamina, seconds: f64, activity: Activity, load: Burden);
    fn stamina_spend(&self, stamina: &mut Stamina, cost: f64);
    fn mana_max(&self, casting: Casting, attributes: &Attributes) -> f64;
    fn mana_step(&self, mana: &mut Mana, seconds: f64, activity: Activity);
    fn visibility(&self, light: f64, pace: f64, load: LoadBand, stealth: f64) -> f64;
}

/// Everything the scenario computed, in order, and a digest of it.
pub struct Run {
    pub values: Vec<f64>,
}

impl Run {
    /// FNV-1a over the bits of every value, so one changed bit changes the digest.
    pub fn digest(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for v in &self.values {
            for byte in v.to_bits().to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        }
        hash
    }
}

const ACTIVITIES: [Activity; 4] = [Activity::Resting, Activity::Walking, Activity::Exerting, Activity::Sleeping];
const BURDENS: [Burden; 4] = [Burden { load_lb: 0.0, capacity_lb: 100.0 }, Burden { load_lb: 40.0, capacity_lb: 100.0 }, Burden { load_lb: 80.0, capacity_lb: 100.0 }, Burden { load_lb: 120.0, capacity_lb: 100.0 }];
const LOADS: [LoadBand; 4] = [LoadBand::Light, LoadBand::Burdened, LoadBand::Encumbered, LoadBand::Overloaded];

pub fn run<B: Book>(book: &B) -> Run {
    let mut out = Vec::new();
    combat(book, &mut out);
    needs_and_exhaustion(book, &mut out);
    stamina(book, &mut out);
    mana(book, &mut out);
    load_and_visibility(book, &mut out);
    Run { values: out }
}

fn combat<B: Book>(book: &B, out: &mut Vec<f64>) {
    for seed in 1..=6_u32 {
        let mut rng = Mulberry32::new(seed);
        for ac in [8, 12, 15, 21] {
            let a = book.creature_attack(4, ac, &mut rng);
            out.extend([a.natural as f64, a.hit as u8 as f64, a.roll.value()]);
            let a = book.folk_attack(2, 1, ac, &mut rng);
            out.extend([a.natural as f64, a.hit as u8 as f64, a.roll.value()]);
            let a = book.folk_attack(-1, 0, ac, &mut rng);
            out.extend([a.natural as f64, a.hit as u8 as f64, a.roll.value()]);
        }
        for con in [-2, 0, 3] {
            let b = book.bite(4, 5, con, &mut rng);
            out.extend([b.damage.value(), b.venom.damage.value(), b.venom.save.as_ref().map_or(-1.0, |s| s.roll.value())]);
            let v = book.venom(0, con, &mut rng);
            out.push(v.damage.value());
            let v = book.venom(9, con, &mut rng);
            out.extend([v.damage.value(), v.save.map_or(-1.0, |s| s.succeeded as u8 as f64)]);
        }
    }
    for dexterity in [8, 10, 15] {
        for armour in [&[][..], &[0.3, 0.3, 0.4], &[1.5], &[2.5, 0.5], &[3.0, 2.0]] {
            out.push(book.armour_class(dexterity, armour));
        }
    }
}

fn needs_and_exhaustion<B: Book>(book: &B, out: &mut Vec<f64>) {
    // Whole days of every activity in turn, with the gear that slows hunger and sleepiness on
    // some, so every worst band, every band edge and every sleep rule is crossed.
    for (con, gear_hunger) in [(0, 1.0), (-2, 1.0), (3, 0.85), (1, 0.7225)] {
        let mut needs = book.new_needs();
        needs.hunger.gear = gear_hunger;
        let mut exhaustion = Exhaustion::default();
        let mut times = ExhaustionTimes::default();
        let schedule: [(Activity, f64); 9] = [
            (Activity::Walking, 5.5),
            (Activity::Exerting, 1.25),
            (Activity::Resting, 3.0),
            (Activity::Sleeping, 8.0),
            (Activity::Walking, 20.0),
            (Activity::Sleeping, 9.0),
            (Activity::Resting, 30.0),
            (Activity::Exerting, 40.0),
            (Activity::Sleeping, 12.0),
        ];
        for round in 0..4 {
            for (activity, hours) in schedule {
                // One big step and the same hours in small ones.
                let pieces = 1 + round * 3;
                for _ in 0..pieces {
                    book.step_times(&mut times, &mut needs, &mut exhaustion, activity, hours / pieces as f64, con);
                }
                out.extend([needs.hunger.value, needs.thirst.value, needs.sleepiness.value, exhaustion.level as f64, times.sleep_hours, times.slept_recovered as u8 as f64]);
                out.extend(book.band_word(&needs).map(|b| b as f64));
                out.extend([times.starving.hours, times.dehydrated.hours, times.spent.hours]);
                out.push(book.speed(exhaustion) as f64);
                // A meal and a drink part way, so the bands come back down.
                if round % 2 == 1 {
                    needs.consume(0.3, 0.35);
                }
            }
        }
    }
    for level in 0..=6 {
        out.push(book.speed(Exhaustion::new(level)) as f64);
    }
    let gear = Gear::new(0.0, [], []);
    out.push(gear.hunger_rate);
}

fn stamina<B: Book>(book: &B, out: &mut Vec<f64>) {
    for load in BURDENS {
        let mut stamina = Stamina::default();
        for step in 0..400 {
            let activity = ACTIVITIES[(step / 7 + step / 31) % 4];
            let seconds = if step % 5 == 0 { 0.016 } else { 0.25 };
            book.stamina_step(&mut stamina, seconds, activity, load);
            if step % 23 == 0 {
                book.stamina_spend(&mut stamina, 0.08);
            }
            if step % 11 == 0 {
                out.extend([stamina.value, stamina.winded as u8 as f64]);
            }
        }
        book.stamina_spend(&mut stamina, 5.0);
        out.extend([stamina.value, stamina.winded as u8 as f64]);
    }
}

fn mana<B: Book>(book: &B, out: &mut Vec<f64>) {
    for casting in [Casting::Martial, Casting::Arcane, Casting::Divine] {
        for score in [3, 8, 10, 14, 18] {
            let attributes = Attributes { strength: 10, dexterity: 10, constitution: 10, intelligence: score, wisdom: 21 - score };
            let max = book.mana_max(casting, &attributes);
            out.push(max);
            let mut mana = Mana { points: max * 0.1, max };
            for (i, activity) in ACTIVITIES.iter().cycle().take(12).enumerate() {
                book.mana_step(&mut mana, 7.0 * (i + 1) as f64, *activity);
                out.push(mana.points);
            }
        }
    }
}

fn load_and_visibility<B: Book>(book: &B, out: &mut Vec<f64>) {
    for strength in [3, 10, 18] {
        let capacity = book.capacity(strength, 25.0);
        out.push(capacity);
        for percent in [0.0, 0.2, 0.339, 0.34, 0.5, 0.669, 0.67, 0.99, 1.0, 1.01, 3.0] {
            let band = book.load_band(capacity * percent, capacity);
            out.extend([LOADS.iter().position(|b| *b == band).unwrap() as f64, book.walk(capacity * percent, capacity) as f64]);
        }
    }
    out.push(book.load_band(5.0, 0.0) as u8 as f64);
    for light in [-0.5, 0.0, 0.3, 1.0, 2.0] {
        for pace in [0.0, 0.7, 1.5, 4.0] {
            for load in LOADS {
                for stealth in [0.0, 2.0, 30.0] {
                    out.push(book.visibility(light, pace, load, stealth));
                }
            }
        }
    }
}
