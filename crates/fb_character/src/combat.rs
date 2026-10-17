//! The combat dice, as pure functions of a seeded RNG: weapon damage, first-level hit points,
//! the Constitution save against venom and a creature's bite. Each returns the roll as an
//! [`Explained`] with a term for every die, so the lab and the game roll the same way and the
//! lab can show how.
//!
//! The functions take plain numbers (dice counts and sides, ability modifiers), so this crate
//! needs neither the item nor the creature tables. Each draws from the RNG exactly as the
//! game's combat did before the dice moved here, so a fixed seed rolls the same results.
//!
//! Ability terms are labelled with the ability and carry the modifier as their amount, so
//! `STR` with a modifier of 2 reads `STR (+2)`.

use procgen_rng::Mulberry32;

use crate::explain::Explained;
use crate::rules::{Rules, defaults};

/// The die term's label: `d8`.
fn die_label(sides: u32) -> String {
    format!("d{sides}")
}

/// Holds a damage-like value at 1 or more, with a term only when it bit.
fn at_least_one(e: Explained) -> Explained {
    if e.value() < 1.0 { e.clamp("at least 1", 1.0, f64::INFINITY) } else { e }
}

/// One blow's damage (SRD 5.1 p. 96, "Damage Rolls"): `count` dice of `sides` (`None` for a
/// bare fist, which rolls nothing and counts as 1), plus `bonus` and the Strength modifier,
/// and never below 1. One draw per die, in order.
pub fn weapon_damage(dice: Option<(u32, u32)>, bonus: i32, strength_modifier: i32, rng: &mut Mulberry32) -> Explained {
    let mut e = Explained::new("Damage", "");
    match dice {
        None => e = e.base("fist", 1.0),
        Some((count, sides)) => {
            for i in 0..count {
                let roll = rng.int(1, sides.max(1)) as f64;
                e = if i == 0 { e.base(die_label(sides), roll) } else { e.add(die_label(sides), roll) };
            }
        }
    }
    if bonus != 0 {
        e = e.add("bonus", bonus as f64);
    }
    at_least_one(e.add("STR", strength_modifier as f64))
}

/// The range a blow's damage falls in: the fewest, the mean and the most.
#[derive(Clone, Debug, PartialEq)]
pub struct DamageRange {
    pub min: Explained,
    pub mean: Explained,
    pub max: Explained,
}

/// The [`DamageRange`] of [`weapon_damage`] for the same arguments. The mean is the dice's
/// average plus the bonuses, held at 1 as a blow is.
pub fn damage_range(dice: Option<(u32, u32)>, bonus: i32, strength_modifier: i32) -> DamageRange {
    let (lo, mean, hi, label) = match dice {
        Some((count, sides)) => (count as f64, count as f64 * (sides as f64 + 1.0) / 2.0, (count * sides) as f64, format!("{count}d{sides}")),
        None => (1.0, 1.0, 1.0, "fist".to_string()),
    };
    let build = |name: &str, dice_term: String, value: f64| {
        let mut e = Explained::new(name, "").base(dice_term, value);
        if bonus != 0 {
            e = e.add("bonus", bonus as f64);
        }
        at_least_one(e.add("STR", strength_modifier as f64))
    };
    DamageRange {
        min: build("Least damage", format!("{label} least"), lo),
        mean: build("Mean damage", format!("{label} mean"), mean),
        max: build("Most damage", format!("{label} most"), hi),
    }
}

/// A first-level delver's hit points (SRD 5.1 "Hit Points at 1st Level"): the class's hit die
/// at its highest plus the Constitution modifier, and never below 1. Nothing is rolled.
pub fn first_level_hit_points(hit_die: u32, constitution_modifier: i32) -> Explained {
    at_least_one(Explained::new("Hit points", "").base(die_label(hit_die), hit_die as f64).add("CON", constitution_modifier as f64))
}

/// A saving throw: a d20 plus an ability modifier against a difficulty class.
#[derive(Clone, Debug, PartialEq)]
pub struct Save {
    /// The d20 and the modifier, `d20 9 + CON 1 = 10`.
    pub roll: Explained,
    pub dc: i32,
    /// Whether the roll met the difficulty class.
    pub succeeded: bool,
}

/// Rolls a saving throw of `ability` (its name, `CON`) with `modifier` against `dc`. One draw.
pub fn saving_throw(ability: &str, modifier: i32, dc: i32, rng: &mut Mulberry32) -> Save {
    let roll = Explained::new(format!("{ability} save"), "").base("d20", rng.int(1, 20) as f64).add(ability, modifier as f64);
    let succeeded = roll.value() >= dc as f64;
    Save { roll, dc, succeeded }
}

/// A creature's venom after the delver's save.
#[derive(Clone, Debug, PartialEq)]
pub struct Venom {
    /// The save, or `None` when there was no venom to save against (nothing was drawn).
    pub save: Option<Save>,
    /// The venom that gets through: all of it, or half rounded down after a successful save.
    pub damage: Explained,
}

impl Venom {
    /// How many game hours the venom poisons the delver for ([`crate::poison::Poisoned`]):
    /// [`crate::poison::VENOM_POISONED_HOURS`] after a failed save, none after a successful one
    /// or with no venom. The lasting poison is our own; see that constant.
    pub fn poisons_for(&self) -> Option<f64> {
        self.poisons_for_with(defaults())
    }

    /// [`Venom::poisons_for`] under `rules`, which set the hours.
    pub fn poisons_for_with(&self, rules: &Rules) -> Option<f64> {
        self.save.as_ref().filter(|save| !save.succeeded).map(|_| rules.combat.venom_poisoned_hours)
    }
}

/// A poisonous snake's venom after a Constitution save (d20 + modifier) against DC 10 halves
/// it, rounded down (SRD 5.1 p. 386, "Poisonous Snake"; rounding down is our own). With no
/// venom nothing is rolled.
pub fn venom_after_save(poison: u32, constitution_modifier: i32, rng: &mut Mulberry32) -> Venom {
    venom_after_save_with(defaults(), poison, constitution_modifier, rng)
}

/// [`venom_after_save`] under `rules`, which set the difficulty class.
pub fn venom_after_save_with(rules: &Rules, poison: u32, constitution_modifier: i32, rng: &mut Mulberry32) -> Venom {
    let damage = Explained::new("Venom", "").base("venom", poison as f64);
    if poison == 0 {
        return Venom { save: None, damage };
    }
    let save = saving_throw("CON", constitution_modifier, rules.combat.venom_save_dc, rng);
    let damage = if save.succeeded { damage.add("halved by the save", -((poison - poison / 2) as f64)) } else { damage };
    Venom { save: Some(save), damage }
}

/// A creature's bite on the delver.
#[derive(Clone, Debug, PartialEq)]
pub struct Bite {
    /// The bite's damage with its venom after the save, as one number.
    pub damage: Explained,
    pub venom: Venom,
}

/// What a bite of `damage` and `poison` does a delver with `constitution_modifier`: the bite's
/// damage plus [`venom_after_save`].
pub fn bite_damage(damage: u32, poison: u32, constitution_modifier: i32, rng: &mut Mulberry32) -> Bite {
    bite_damage_with(defaults(), damage, poison, constitution_modifier, rng)
}

/// [`bite_damage`] under `rules`.
pub fn bite_damage_with(rules: &Rules, damage: u32, poison: u32, constitution_modifier: i32, rng: &mut Mulberry32) -> Bite {
    let venom = venom_after_save_with(rules, poison, constitution_modifier, rng);
    let mut total = Explained::new("Bite", "").base("bite", damage as f64);
    if poison > 0 {
        total = total.add("venom", venom.damage.value());
    }
    Bite { damage: total, venom }
}

/// How the d20 of an attack roll is thrown: once, or twice keeping the higher (advantage) or the
/// lower (disadvantage)
/// ([SRD 5.1 p. 76](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=76),
/// "Advantage and Disadvantage").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Odds {
    #[default]
    Even,
    Advantage,
    Disadvantage,
}

/// An attack roll against an armour class.
#[derive(Clone, Debug, PartialEq)]
pub struct Attack {
    /// The d20 and the to-hit terms, `d20 (14) + STR (+2) + ToHit (+1)`.
    pub roll: Explained,
    /// The d20 kept.
    pub natural: u32,
    /// The other d20, set aside, when the roll had advantage or disadvantage.
    pub set_aside: Option<u32>,
    /// The armour class the attack met.
    pub armour_class: i32,
    pub hit: bool,
}

impl Attack {
    /// The roll, as one line for a log: `d20 (14) + STR (+2) + ToHit (+1) = 17 vs AC 15: hit`.
    /// A natural 20 or 1 that decided the blow against the total says so, and a roll with
    /// advantage or disadvantage names the die it set aside.
    pub fn line(&self) -> String {
        let by_total = self.roll.value() >= self.armour_class as f64;
        let verdict = match (self.hit, self.natural) {
            (true, natural) if !by_total => format!("hit, natural {natural}"),
            (false, natural) if by_total => format!("miss, natural {natural}"),
            (true, _) => "hit".to_string(),
            (false, _) => "miss".to_string(),
        };
        let aside = self.set_aside.map_or(String::new(), |d| format!(", {d} set aside"));
        format!("{} = {} vs AC {}: {verdict}{aside}", self.roll.terms_line(), self.roll.value(), self.armour_class)
    }
}

/// Rolls an attack: a d20 plus `to_hit` terms (a label and an amount each) against
/// `armour_class` (SRD 5.1 p. 96, "Attack Rolls"). A natural 20 always hits and a natural 1
/// always misses; otherwise the total must reach the armour class. One draw.
pub fn attack_roll(to_hit: &[(&str, i32)], armour_class: i32, rng: &mut Mulberry32) -> Attack {
    attack_roll_with(defaults(), to_hit, armour_class, Odds::Even, rng)
}

/// [`attack_roll`] under `rules` (which say which d20 always hits and which always misses),
/// thrown with `odds` ([`Odds`]): one draw when even, two otherwise, keeping the higher with
/// advantage and the lower with disadvantage.
pub fn attack_roll_with(rules: &Rules, to_hit: &[(&str, i32)], armour_class: i32, odds: Odds, rng: &mut Mulberry32) -> Attack {
    let first = rng.int(1, 20);
    let (natural, set_aside, label) = match odds {
        Odds::Even => (first, None, "d20"),
        Odds::Advantage => {
            let second = rng.int(1, 20);
            (first.max(second), Some(first.min(second)), "d20 with advantage")
        }
        Odds::Disadvantage => {
            let second = rng.int(1, 20);
            (first.min(second), Some(first.max(second)), "d20 with disadvantage")
        }
    };
    let mut roll = Explained::new("Attack", "").base(label, natural as f64);
    for (label, amount) in to_hit {
        roll = roll.add(*label, *amount as f64);
    }
    // A natural 1 is checked first, so a file that sets both to the same number misses.
    let hit = if natural <= rules.combat.natural_miss {
        false
    } else if natural >= rules.combat.natural_hit {
        true
    } else {
        roll.value() >= armour_class as f64
    };
    Attack { roll, natural, set_aside, armour_class, hit }
}

/// The proficiency bonus the delver and folk add to their attack rolls: +2, a character's from
/// 1st to 4th level (SRD 5.1, "Beyond 1st Level", the Character Advancement table; "Attack
/// Rolls", p. 96). Everyone counts as proficient with whatever they swing, as everyone is with
/// unarmed strikes in the SRD; that every weapon counts is our own simplification until classes
/// carry weapon proficiencies. This is the default of [`crate::rules::CombatRules::proficiency_bonus`].
pub const PROFICIENCY_BONUS: i32 = 2;

/// What an attacker adds to the d20. The delver, folk and creatures all roll through it against
/// the armour class of whatever they swing at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToHit {
    /// Someone with abilities and a weapon (the delver, a folk): the Strength modifier, the
    /// proficiency bonus ([`PROFICIENCY_BONUS`]) and the weapon's `ToHit`, rounded and left out
    /// when 0 (SRD 5.1 p. 96, "Attack Rolls").
    Strength { modifier: i32, weapon: i32 },
    /// A creature: the attack bonus its SRD stat block prints, and the `ToHit` of a weapon it
    /// carries (a skeleton's), left out when 0.
    StatBlock { attack_bonus: i32, weapon: i32 },
}

impl ToHit {
    /// The terms it adds to the d20, each with its label.
    pub fn terms(self) -> Vec<(&'static str, i32)> {
        self.terms_with(defaults())
    }

    /// [`ToHit::terms`] under `rules`, which set the proficiency bonus.
    pub fn terms_with(self, rules: &Rules) -> Vec<(&'static str, i32)> {
        let (first, weapon) = match self {
            ToHit::Strength { modifier, weapon } => (("STR", modifier), weapon),
            ToHit::StatBlock { attack_bonus, weapon } => (("attack bonus", attack_bonus), weapon),
        };
        let mut terms = vec![first];
        if matches!(self, ToHit::Strength { .. }) {
            terms.push(("proficiency", rules.combat.proficiency_bonus));
        }
        if weapon != 0 {
            terms.push(("ToHit", weapon));
        }
        terms
    }

    /// An attack with these terms against `armour_class`, thrown with `odds` ([`attack_roll_with`]).
    pub fn roll(self, armour_class: i32, odds: Odds, rng: &mut Mulberry32) -> Attack {
        self.roll_with(defaults(), armour_class, odds, rng)
    }

    /// [`ToHit::roll`] under `rules`.
    pub fn roll_with(self, rules: &Rules, armour_class: i32, odds: Odds, rng: &mut Mulberry32) -> Attack {
        attack_roll_with(rules, &self.terms_with(rules), armour_class, odds, rng)
    }
}

/// A folk attacker's roll: the Strength modifier and the weapon's `ToHit` ([`ToHit::Strength`]).
pub fn folk_attack(strength_modifier: i32, weapon_to_hit: i32, armour_class: i32, rng: &mut Mulberry32) -> Attack {
    folk_attack_with(defaults(), strength_modifier, weapon_to_hit, armour_class, rng)
}

/// [`folk_attack`] under `rules`.
pub fn folk_attack_with(rules: &Rules, strength_modifier: i32, weapon_to_hit: i32, armour_class: i32, rng: &mut Mulberry32) -> Attack {
    ToHit::Strength { modifier: strength_modifier, weapon: weapon_to_hit }.roll_with(rules, armour_class, Odds::Even, rng)
}

/// A creature's roll: the attack bonus its SRD stat block prints ([`ToHit::StatBlock`]).
pub fn creature_attack(attack_bonus: i32, armour_class: i32, rng: &mut Mulberry32) -> Attack {
    creature_attack_with(defaults(), attack_bonus, armour_class, rng)
}

/// [`creature_attack`] under `rules`.
pub fn creature_attack_with(rules: &Rules, attack_bonus: i32, armour_class: i32, rng: &mut Mulberry32) -> Attack {
    ToHit::StatBlock { attack_bonus, weapon: 0 }.roll_with(rules, armour_class, Odds::Even, rng)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier;

    /// A seed whose first d20 is `natural`.
    fn seed_for(natural: u32) -> u32 {
        (0..10_000).find(|s| Mulberry32::new(*s).int(1, 20) == natural).unwrap()
    }

    #[test]
    fn a_14_plus_str_2_plus_proficiency_plus_to_hit_1_hits_ac_15() {
        let attack = folk_attack(2, 1, 15, &mut Mulberry32::new(seed_for(14)));
        assert_eq!((attack.natural, attack.roll.value(), attack.hit), (14, 19.0, true));
        assert_eq!(attack.line(), "d20 (14) + STR (+2) + proficiency (+2) + ToHit (+1) = 19 vs AC 15: hit");
        let miss = folk_attack(2, 1, 20, &mut Mulberry32::new(seed_for(14)));
        assert_eq!((miss.hit, miss.line().ends_with("vs AC 20: miss")), (false, true));
    }

    #[test]
    fn a_natural_1_always_misses_and_a_natural_20_always_hits() {
        let one = creature_attack(30, 5, &mut Mulberry32::new(seed_for(1)));
        assert!(!one.hit, "{}", one.line());
        assert_eq!(one.line(), "d20 (1) + attack bonus (+30) = 31 vs AC 5: miss, natural 1");
        let twenty = folk_attack(-4, 0, 30, &mut Mulberry32::new(seed_for(20)));
        assert!(twenty.hit, "{}", twenty.line());
        assert_eq!(twenty.line(), "d20 (20) + STR (-4) + proficiency (+2) = 18 vs AC 30: hit, natural 20");
        let plain = folk_attack(0, 0, 10, &mut Mulberry32::new(seed_for(20)));
        assert!(plain.line().ends_with(": hit"), "a 20 that also beats the AC is just a hit");
    }

    #[test]
    fn plus_3_against_ac_13_hits_55_percent_of_10000_rolls() {
        let mut rng = Mulberry32::new(2024);
        let hits = (0..10_000).filter(|_| creature_attack(3, 13, &mut rng).hit).count();
        assert!((hits as f64 / 10_000.0 - 0.55).abs() < 0.01, "{hits} hits");
    }

    #[test]
    fn an_attack_draws_one_die() {
        let mut rng = Mulberry32::new(5);
        attack_roll(&[("STR", 1)], 12, &mut rng);
        let mut reference = Mulberry32::new(5);
        reference.next_f64();
        assert_eq!(rng.next_f64(), reference.next_f64());
    }

    #[test]
    fn an_ordinary_swing_hits_ac_30_only_on_a_natural_20_and_ac_0_but_for_a_natural_1() {
        let to_hit = ToHit::Strength { modifier: 2, weapon: 1 };
        let mut rng = Mulberry32::new(77);
        let (mut high_hits, mut low_misses) = (0, 0);
        for _ in 0..2_000 {
            let high = to_hit.roll(30, Odds::Even, &mut rng);
            assert_eq!(high.hit, high.natural == 20, "{}", high.line());
            high_hits += high.hit as usize;
            let low = to_hit.roll(0, Odds::Even, &mut rng);
            assert_eq!(low.hit, low.natural != 1, "{}", low.line());
            low_misses += !low.hit as usize;
        }
        // One in twenty each way.
        assert!((60..140).contains(&high_hits) && (60..140).contains(&low_misses), "{high_hits} {low_misses}");
    }

    #[test]
    fn disadvantage_keeps_the_lower_die_and_hits_less_often() {
        let seed = (0..10_000u32).find(|s| {
            let mut r = Mulberry32::new(*s);
            (r.int(1, 20), r.int(1, 20)) == (15, 4)
        });
        let attack = ToHit::Strength { modifier: 1, weapon: 0 }.roll(12, Odds::Disadvantage, &mut Mulberry32::new(seed.unwrap()));
        assert_eq!((attack.natural, attack.set_aside, attack.hit), (4, Some(15), false));
        assert_eq!(attack.line(), "d20 with disadvantage (4) + STR (+1) + proficiency (+2) = 7 vs AC 12: miss, 15 set aside");
        let rate = |odds| {
            let mut rng = Mulberry32::new(31);
            (0..20_000).filter(|_| ToHit::StatBlock { attack_bonus: 4, weapon: 0 }.roll(13, odds, &mut rng).hit).count() as f64 / 20_000.0
        };
        // +4 against 13 needs a 9: 60% straight, 36% at disadvantage, 84% with advantage.
        assert!((rate(Odds::Even) - 0.60).abs() < 0.015, "{}", rate(Odds::Even));
        assert!((rate(Odds::Disadvantage) - 0.36).abs() < 0.015, "{}", rate(Odds::Disadvantage));
        assert!((rate(Odds::Advantage) - 0.84).abs() < 0.015, "{}", rate(Odds::Advantage));
    }

    #[test]
    fn a_skeletons_weapon_adds_its_to_hit_and_a_zero_is_left_out() {
        assert_eq!(ToHit::StatBlock { attack_bonus: 4, weapon: 1 }.terms(), [("attack bonus", 4), ("ToHit", 1)]);
        assert_eq!(ToHit::Strength { modifier: -1, weapon: 0 }.terms(), [("STR", -1), ("proficiency", 2)]);
    }

    /// The game's combat as it was before the dice moved here, for the same draws.
    mod old {
        use procgen_rng::Mulberry32;

        pub fn roll(dice: Option<(u32, u32)>, bonus: i32, strength: i32, rng: &mut Mulberry32) -> u32 {
            let dice = dice.map_or(1, |(count, sides)| (0..count).map(|_| rng.int(1, sides.max(1)) as i32).sum());
            (dice + bonus + strength).max(1) as u32
        }

        pub fn range(dice: Option<(u32, u32)>, bonus: i32, strength: i32) -> (u32, u32) {
            let (lo, hi) = match dice {
                Some((count, sides)) => (count as i32, (count as i32) * (sides as i32)),
                None => (1, 1),
            };
            (((lo + bonus + strength).max(1)) as u32, ((hi + bonus + strength).max(1)) as u32)
        }

        pub fn hit_points(die: i32, con_modifier: i32) -> i32 {
            (die + con_modifier).max(1)
        }

        pub fn venom(poison: u32, con_modifier: i32, rng: &mut Mulberry32) -> u32 {
            if poison == 0 {
                return 0;
            }
            let save = rng.int(1, 20) as i32 + con_modifier;
            if save >= 10 { poison / 2 } else { poison }
        }

        pub fn bite(damage: u32, poison: u32, con_modifier: i32, rng: &mut Mulberry32) -> u32 {
            damage + venom(poison, con_modifier, rng)
        }
    }

    fn amounts(e: &Explained) -> Vec<f64> {
        e.terms().iter().map(|t| t.amount).collect()
    }

    #[test]
    fn a_1d8_plus_1_weapon_with_str_14_explains_as_die_plus_1_plus_2() {
        // Find a seed whose d8 is 5, as the spec's example rolls.
        let seed = (0..200).find(|s| Mulberry32::new(*s).int(1, 8) == 5).unwrap();
        let e = weapon_damage(Some((1, 8)), 1, modifier(14), &mut Mulberry32::new(seed));
        assert_eq!(e.value(), 8.0);
        assert_eq!(amounts(&e), [5.0, 1.0, 2.0]);
        assert_eq!(e.line(), "Damage 8 = d8 (5) + bonus (+1) + STR (+2)");
        let range = damage_range(Some((1, 8)), 1, 2);
        assert_eq!((range.min.value(), range.max.value()), (4.0, 11.0));
        assert_eq!(range.mean.value(), 7.5);
        assert_eq!((range.min.value() as u32, range.max.value() as u32), old::range(Some((1, 8)), 1, 2));
    }

    #[test]
    fn a_fist_and_a_clamped_blow_read_plainly() {
        assert_eq!(weapon_damage(None, 0, 2, &mut Mulberry32::new(1)).line(), "Damage 3 = fist (1) + STR (+2)");
        let seed = (0..200).find(|s| Mulberry32::new(*s).int(1, 4) == 2).unwrap();
        let e = weapon_damage(Some((1, 4)), 0, -4, &mut Mulberry32::new(seed));
        assert_eq!(e.line(), "Damage 1 = d4 (2) + STR (-4) at least 1");
        assert_eq!(bite_damage(2, 0, 0, &mut Mulberry32::new(1)).damage.line(), "Bite 2 = bite (2)");
    }

    #[test]
    fn every_die_is_a_term() {
        let e = weapon_damage(Some((3, 6)), 0, 0, &mut Mulberry32::new(9));
        assert_eq!(e.terms().len(), 4);
        assert_eq!(e.terms()[..3].iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), ["d6", "d6", "d6"]);
    }

    #[test]
    fn a_mage_with_con_8_has_5_hit_points() {
        let e = first_level_hit_points(6, modifier(8));
        assert_eq!(e.value(), 5.0);
        assert_eq!(amounts(&e), [6.0, -1.0]);
        assert_eq!(e.line(), "Hit points 5 = d6 (6) + CON (-1)");
        assert_eq!(first_level_hit_points(6, -4).value(), 2.0);
        assert_eq!(first_level_hit_points(6, -9).value(), 1.0, "never below 1");
    }

    #[test]
    fn a_venom_save_of_9_plus_1_succeeds_and_halves_the_damage() {
        let seed = (0..500).find(|s| Mulberry32::new(*s).int(1, 20) == 9).unwrap();
        let venom = venom_after_save(7, 1, &mut Mulberry32::new(seed));
        let save = venom.save.unwrap();
        assert_eq!((save.roll.value(), save.dc, save.succeeded), (10.0, 10, true));
        assert_eq!(save.roll.line(), "CON save 10 = d20 (9) + CON (+1)");
        assert_eq!(venom.damage.value(), 3.0, "7 halved, rounded down");
        let seed = (0..500).find(|s| Mulberry32::new(*s).int(1, 20) == 9).unwrap();
        assert_eq!(venom_after_save(7, 0, &mut Mulberry32::new(seed)).damage.value(), 7.0, "9 fails");
    }

    #[test]
    fn a_failed_save_against_venom_poisons_for_an_hour_and_a_good_one_does_not() {
        let seed = (0..500).find(|s| Mulberry32::new(*s).int(1, 20) == 9).unwrap();
        assert_eq!(venom_after_save(5, 0, &mut Mulberry32::new(seed)).poisons_for(), Some(1.0), "9 fails DC 10");
        assert_eq!(venom_after_save(5, 1, &mut Mulberry32::new(seed)).poisons_for(), None, "10 saves");
        assert_eq!(venom_after_save(0, -5, &mut Mulberry32::new(seed)).poisons_for(), None, "no venom, no poison");
        assert_eq!(bite_damage(1, 5, 0, &mut Mulberry32::new(seed)).venom.poisons_for(), Some(1.0));
    }

    #[test]
    fn no_venom_rolls_nothing() {
        let mut rng = Mulberry32::new(4);
        let venom = venom_after_save(0, 3, &mut rng);
        assert_eq!((venom.save, venom.damage.value()), (None, 0.0));
        assert_eq!(rng.next_f64(), Mulberry32::new(4).next_f64());
    }

    #[test]
    fn the_new_rolls_draw_as_the_old_ones_did() {
        for seed in 1..=8u32 {
            let (mut new, mut old_rng) = (Mulberry32::new(seed), Mulberry32::new(seed));
            for i in 0..300i32 {
                let dice = [None, Some((1, 4)), Some((1, 8)), Some((2, 6)), Some((3, 4))][i as usize % 5];
                let (bonus, strength) = (i % 3 - 1, i % 9 - 4);
                let rolled = weapon_damage(dice, bonus, strength, &mut new);
                assert_eq!(rolled.value() as u32, old::roll(dice, bonus, strength, &mut old_rng), "seed {seed} roll {i}");
                let (poison, con) = ([0, 2, 5, 8][i as usize % 4], i % 7 - 3);
                let bitten = bite_damage(damage_of(i), poison, con, &mut new);
                assert_eq!(bitten.damage.value() as u32, old::bite(damage_of(i), poison, con, &mut old_rng), "seed {seed} bite {i}");
                assert_eq!(venom_after_save(poison, con, &mut new).damage.value() as u32, old::venom(poison, con, &mut old_rng));
            }
            assert_eq!(new.next_f64(), old_rng.next_f64(), "the streams end together");
        }
        for (die, con) in [(10, 2), (6, -1), (8, -4), (6, -9)] {
            assert_eq!(first_level_hit_points(die as u32, con).value() as i32, old::hit_points(die, con));
        }
        for strength in -4..=4 {
            let range = damage_range(Some((2, 6)), 1, strength);
            assert_eq!((range.min.value() as u32, range.max.value() as u32), old::range(Some((2, 6)), 1, strength));
        }
    }

    fn damage_of(i: i32) -> u32 {
        (i % 4) as u32
    }
}
