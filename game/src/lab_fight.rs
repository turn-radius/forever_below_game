//! Fights in the lab ([`crate::lab_view`]'s Fight tab): the character and an attacker take turns
//! until one is down, once ([`fight`]) or a thousand times ([`fights`]) for the answers and
//! charts of [`FightSummary`]. No Bevy: every roll is the game's own, from [`fb_character::combat`],
//! [`Weapon`] and [`Rules::creature`], under the rules it is given, so what is tested is what is
//! played, and the same seed gives the same thousand.
//!
//! The attacker is a creature striking with its bite, or a folk (Strength 10, hit die and
//! armour from the rules) holding a melee base rolled from the strike seed. The attacker strikes
//! first in a round unless sides are swapped, which has the character strike first. A fight ends when
//! one side is at 0, or at [`MAX_ROUNDS`] with neither. Rounds are what the charts count.

use fb_character::combat::{self as rules, Attack, creature_attack_with, folk_attack_with};
use fb_character::explain::Explained;
use fb_body::Folk;
use fb_character::{Attributes, modifier};
use fb_character::rules::Rules;
use fb_creatures::Dice;
use fb_items::{Slot, roll_item};
use procgen_rng::Mulberry32;

use crate::character::{Character, Class};
use crate::combat::{Weapon, first_level_hit_points_with};
use crate::inventory::Inventory;
use crate::lab_strike::Attacker;
use crate::lab_view::LabState;

/// A fight that has gone this many rounds is called a draw.
pub const MAX_ROUNDS: u32 = 100;
/// The rounds the life chart and the histogram show; longer fights count in the last column.
pub const CHART_ROUNDS: usize = 24;
/// How many fights "1,000 fights" is.
pub const THOUSAND: u32 = 1000;
/// Every score a folk attacker has.
const FOLK_SCORE: i32 = 10;

/// Who won.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The lab's character.
    Them,
    /// The attacker.
    Attacker,
}

/// One blow: its attack roll, its damage if it hit, the life each side had after it, and a line.
#[derive(Clone, Debug, PartialEq)]
pub struct Blow {
    /// Whether the attacker struck (else the character).
    pub by_attacker: bool,
    pub attack: Attack,
    pub damage: Option<Explained>,
    /// The character's life and the attacker's after the blow.
    pub life: (i32, i32),
    /// The blow as plain words; empty unless the fight was played with lines.
    pub line: String,
}

/// One fight.
#[derive(Clone, Debug, PartialEq)]
pub struct FightRecord {
    pub blows: Vec<Blow>,
    pub rounds: u32,
    pub winner: Option<Side>,
    /// The character's life at the start and after each round.
    pub life: Vec<i32>,
    /// The attacker's name.
    pub attacker: String,
}

/// The middle half of fights at one point of the life chart, and the median.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LifeBand {
    pub low: i32,
    pub median: i32,
    pub high: i32,
}

/// What a run of fights came to.
#[derive(Clone, Debug, PartialEq)]
pub struct FightSummary {
    pub fights: u32,
    pub attacker: String,
    /// The character's life at full.
    pub full_life: i32,
    /// Of the attacker's blows, the share that hit them.
    pub attacker_hit_rate: f64,
    /// Of the character's blows, the share that hit.
    pub their_hit_rate: f64,
    /// What a blow that hit them did, on average; `None` if none did.
    pub damage_per_hit: Option<f64>,
    /// The attacker's blows it took to down them, over the fights it did; `None` if it never did.
    pub blows_to_down: Option<f64>,
    /// The share of fights the character won, and the attacker won.
    pub they_win: f64,
    pub attacker_wins: f64,
    /// Their life at the start and after each round, up to [`CHART_ROUNDS`].
    pub life: Vec<LifeBand>,
    /// How many fights took each number of rounds; the last holds [`CHART_ROUNDS`] and more.
    /// Draws, which reached [`MAX_ROUNDS`] with neither down, are counted apart.
    pub rounds: Vec<u32>,
    pub draws: u32,
}

/// Everything about a fight that is the same each time, worked out once from the lab state.
pub struct Setup<'r> {
    rules: &'r Rules,
    foe: Foe,
    their_armour: i32,
    their_life: i32,
    their_constitution: i32,
    their_strength: i32,
    weapon: Weapon,
    weapon_name: String,
    to_hit: i32,
    attacker_first: bool,
}

struct Foe {
    name: String,
    armour: i32,
    life: i32,
    strike: Strike,
}

enum Strike {
    Bite { bonus: i32, bite: Dice, venom: Option<Dice> },
    Weapon { weapon: Weapon, to_hit: i32, strength: i32 },
}

impl<'r> Setup<'r> {
    pub fn new(state: &LabState, rules: &'r Rules) -> Setup<'r> {
        let inventory = state.inventory();
        let character = &state.character;
        let held = inventory.worn(Slot::Mainhand).and_then(|id| inventory.item(id));
        let foe = match &state.attacker {
            Attacker::Creature(species) => {
                let block = rules.creature(*species);
                Foe { name: block.name.to_string(), armour: block.armour_class, life: block.hit_points as i32, strike: Strike::Bite { bonus: block.attack_bonus, bite: block.bite, venom: block.venom } }
            }
            Attacker::Folk(base) => {
                // A real character: a human Fighter with every score 10, wearing nothing and
                // holding the base, so armour class and hit points come from the same rules.
                let folk = Character {
                    folk: Folk::Human,
                    body_seed: 0,
                    name: "Folk".into(),
                    class: Class::Fighter,
                    attributes: Attributes { strength: FOLK_SCORE, dexterity: FOLK_SCORE, constitution: FOLK_SCORE, intelligence: FOLK_SCORE, wisdom: FOLK_SCORE },
                    dirt: 0.0,
                };
                let mut hands = Inventory::default();
                if let Some(item) = roll_item(base, state.strike_seed) {
                    let id = hands.add(item);
                    let _ = hands.wear(id, Slot::Mainhand);
                }
                let held = hands.worn(Slot::Mainhand).and_then(|id| hands.item(id));
                let to_hit = held.map_or(0, |i| i.profile.stats.to_hit.round() as i32);
                Foe {
                    name: format!("Folk with {base}"),
                    armour: folk.armour_class_with(rules, &hands).value().round() as i32,
                    life: first_level_hit_points_with(rules, &folk),
                    strike: Strike::Weapon { weapon: Weapon::of_stats(held), to_hit, strength: modifier(folk.strength()) },
                }
            }
        };
        Setup {
            rules,
            foe,
            their_armour: character.armour_class_with(rules, &inventory).value().round() as i32,
            their_life: first_level_hit_points_with(rules, character),
            their_constitution: modifier(character.constitution()),
            their_strength: modifier(character.strength()),
            weapon: Weapon::of_stats(held),
            weapon_name: held.map_or_else(|| "fists".to_string(), |i| i.name.clone()),
            to_hit: held.map_or(0, |i| i.profile.stats.to_hit.round() as i32),
            attacker_first: !state.swapped,
        }
    }

    /// The character's life at full.
    pub fn full_life(&self) -> i32 {
        self.their_life
    }

    /// The attacker's blow on the character.
    fn attacker_blow(&self, rng: &mut Mulberry32) -> (Attack, Option<Explained>) {
        match &self.foe.strike {
            Strike::Bite { bonus, bite, venom } => {
                let attack = creature_attack_with(self.rules, *bonus, self.their_armour, rng);
                if !attack.hit {
                    return (attack, None);
                }
                let damage = bite.roll(rng);
                let poison = venom.map_or(0, |v| v.roll(rng));
                let hurt = rules::bite_damage_with(self.rules, damage, poison, self.their_constitution, rng);
                (attack, Some(hurt.damage))
            }
            Strike::Weapon { weapon, to_hit, strength } => {
                let attack = folk_attack_with(self.rules, *strength, *to_hit, self.their_armour, rng);
                let damage = attack.hit.then(|| weapon.roll_explained(*strength, rng));
                (attack, damage)
            }
        }
    }

    /// The character's blow on the attacker.
    fn their_blow(&self, rng: &mut Mulberry32) -> (Attack, Option<Explained>) {
        let attack = folk_attack_with(self.rules, self.their_strength, self.to_hit, self.foe.armour, rng);
        let damage = attack.hit.then(|| self.weapon.roll_explained(self.their_strength, rng));
        (attack, damage)
    }

    fn line(&self, by_attacker: bool, attack: &Attack, damage: &Option<Explained>, before: i32, after: i32) -> String {
        let bonus = (attack.roll.value() - attack.natural as f64).round() as i32;
        let needs = attack.armour_class - bonus;
        let term = if bonus >= 0 { format!("\u{2212} {bonus}") } else { format!("+ {}", -bonus) };
        let outcome = match damage {
            Some(d) => format!("hits for {}", d.value()),
            None => "misses".to_string(),
        };
        let (who, verb) = if by_attacker {
            (self.foe.name.clone(), if matches!(self.foe.strike, Strike::Bite { .. }) { "bites".to_string() } else { "strikes".to_string() })
        } else {
            ("They".to_string(), format!("swing the {}", self.weapon_name))
        };
        let life = if by_attacker {
            format!("life {before} \u{2192} {after}")
        } else {
            format!("{} {before} \u{2192} {after}{}", self.foe.name, if after <= 0 { ", down" } else { "" })
        };
        let down = if by_attacker && after <= 0 { ", down" } else { "" };
        format!("{who} {verb}: rolls {}, needs {needs} to hit ({} armour {term} attack) \u{2192} {outcome} \u{b7} {life}{down}", attack.natural, attack.armour_class)
    }

    /// One fight from `rng`, with each blow's line when `lines`.
    pub fn play(&self, rng: &mut Mulberry32, lines: bool) -> FightRecord {
        let (mut them, mut foe) = (self.their_life, self.foe.life);
        let mut blows = Vec::new();
        let mut life = vec![them];
        let mut rounds = 0;
        while them > 0 && foe > 0 && rounds < MAX_ROUNDS {
            rounds += 1;
            for by_attacker in if self.attacker_first { [true, false] } else { [false, true] } {
                if them <= 0 || foe <= 0 {
                    break;
                }
                let (attack, damage) = if by_attacker { self.attacker_blow(rng) } else { self.their_blow(rng) };
                let (before, hurt) = (if by_attacker { them } else { foe }, damage.as_ref().map_or(0, |d| d.value() as i32));
                if by_attacker {
                    them -= hurt;
                } else {
                    foe -= hurt;
                }
                let after = before - hurt;
                let line = if lines { self.line(by_attacker, &attack, &damage, before, after) } else { String::new() };
                blows.push(Blow { by_attacker, attack, damage, life: (them, foe), line });
            }
            life.push(them.max(0));
        }
        let winner = if them <= 0 {
            Some(Side::Attacker)
        } else if foe <= 0 {
            Some(Side::Them)
        } else {
            None
        };
        FightRecord { blows, rounds, winner, life, attacker: self.foe.name.clone() }
    }
}

/// One fight under `rules`, with its lines.
pub fn fight(state: &LabState, rules: &Rules, rng: &mut Mulberry32) -> FightRecord {
    Setup::new(state, rules).play(rng, true)
}

/// The share at `percent` of the sorted `values`.
fn quantile(sorted: &[i32], percent: usize) -> i32 {
    sorted[(sorted.len() - 1) * percent / 100]
}

/// `n` fights under `rules` from `seed`, in one stream: the same seed gives the same summary.
pub fn fights(state: &LabState, rules: &Rules, seed: u32, n: u32) -> FightSummary {
    let setup = Setup::new(state, rules);
    let mut rng = Mulberry32::new(seed);
    let records: Vec<FightRecord> = (0..n).map(|_| setup.play(&mut rng, false)).collect();
    summarise(&setup, &records)
}

fn summarise(setup: &Setup, records: &[FightRecord]) -> FightSummary {
    let n = records.len().max(1) as f64;
    let (mut attacks, mut hits, mut damage, mut theirs, mut their_hits) = (0u32, 0u32, 0.0, 0u32, 0u32);
    let (mut downed, mut blows_when_downed, mut they_win, mut attacker_wins) = (0u32, 0u32, 0u32, 0u32);
    for r in records {
        let mut foe_blows = 0;
        for b in &r.blows {
            if b.by_attacker {
                foe_blows += 1;
                attacks += 1;
                if let Some(d) = &b.damage {
                    hits += 1;
                    damage += d.value();
                }
            } else {
                theirs += 1;
                their_hits += b.damage.is_some() as u32;
            }
        }
        match r.winner {
            Some(Side::Attacker) => {
                attacker_wins += 1;
                downed += 1;
                blows_when_downed += foe_blows;
            }
            Some(Side::Them) => they_win += 1,
            None => {}
        }
    }
    let longest = records.iter().map(|r| r.rounds as usize).max().unwrap_or(0).clamp(1, CHART_ROUNDS);
    let life = (0..=longest)
        .map(|round| {
            let mut at: Vec<i32> = records.iter().map(|r| *r.life.get(round).or(r.life.last()).unwrap_or(&setup.their_life)).collect();
            at.sort_unstable();
            if at.is_empty() {
                return LifeBand { low: setup.their_life, median: setup.their_life, high: setup.their_life };
            }
            LifeBand { low: quantile(&at, 25), median: quantile(&at, 50), high: quantile(&at, 75) }
        })
        .collect();
    let mut rounds = vec![0u32; CHART_ROUNDS + 1];
    let mut draws = 0;
    for r in records {
        if r.winner.is_none() {
            draws += 1;
        } else {
            rounds[(r.rounds as usize).min(CHART_ROUNDS)] += 1;
        }
    }
    let ratio = |a: u32, b: u32| if b == 0 { 0.0 } else { a as f64 / b as f64 };
    FightSummary {
        fights: records.len() as u32,
        attacker: setup.foe.name.clone(),
        full_life: setup.their_life,
        attacker_hit_rate: ratio(hits, attacks),
        their_hit_rate: ratio(their_hits, theirs),
        damage_per_hit: (hits > 0).then(|| damage / hits as f64),
        blows_to_down: (downed > 0).then(|| blows_when_downed as f64 / downed as f64),
        they_win: they_win as f64 / n,
        attacker_wins: attacker_wins as f64 / n,
        life,
        rounds,
        draws,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_creatures::Species;

    /// The default Fighter (AC 15) against a giant rat.
    fn rat() -> LabState {
        LabState::default()
    }

    fn with_bonus(bonus: i32) -> LabState {
        let mut state = rat();
        crate::lab_rules::set_leaf(&mut state.ruleset, "creatures/Giant Rat/attack_bonus", crate::lab_rules::Leaf::Int(bonus as i64));
        state
    }

    #[test]
    fn a_giant_rat_at_plus_4_against_ac_15_hits_half_the_time() {
        let state = with_bonus(4);
        let summary = fights(&state, &state.ruleset.rules, 1, THOUSAND);
        assert!((summary.attacker_hit_rate - 0.5).abs() < 0.02, "{}", summary.attacker_hit_rate);
        assert_eq!(summary.attacker, "Giant Rat");
    }

    #[test]
    fn at_plus_2_it_hits_40_percent() {
        let state = with_bonus(2);
        let summary = fights(&state, &state.ruleset.rules, 1, THOUSAND);
        assert!((summary.attacker_hit_rate - 0.4).abs() < 0.02, "{}", summary.attacker_hit_rate);
    }

    #[test]
    fn life_starts_full_and_the_bands_nest() {
        let state = rat();
        let summary = fights(&state, &state.ruleset.rules, 7, 500);
        assert_eq!(summary.life[0], LifeBand { low: 10, median: 10, high: 10 });
        assert_eq!(summary.full_life, 10);
        assert!(summary.life.iter().all(|b| b.low <= b.median && b.median <= b.high));
        assert!(summary.life.last().unwrap().median <= 10);
        assert_eq!(summary.rounds.iter().sum::<u32>() + summary.draws, 500);
        assert!(summary.life.iter().all(|b| b.low >= 0), "life is kept at 0 or more");
        assert!(summary.they_win + summary.attacker_wins <= 1.0 + 1e-9);
    }

    #[test]
    fn the_same_seed_gives_the_same_thousand_and_another_does_not() {
        let state = rat();
        let a = fights(&state, &state.ruleset.rules, 3, THOUSAND);
        assert_eq!(a, fights(&state, &state.ruleset.rules, 3, THOUSAND));
        assert_ne!(a, fights(&state, &state.ruleset.rules, 4, THOUSAND));
    }

    #[test]
    fn a_fight_is_played_in_turns_until_one_is_down() {
        let state = rat();
        let record = fight(&state, &state.ruleset.rules, &mut Mulberry32::new(5));
        assert!(record.winner.is_some());
        assert!(record.blows.first().unwrap().by_attacker, "the attacker strikes first");
        let last = record.blows.last().unwrap();
        assert!(last.life.0 <= 0 || last.life.1 <= 0);
        assert!(record.blows.iter().all(|b| !b.line.is_empty()));
        let first = &record.blows[0].line;
        assert!(first.starts_with("Giant Rat bites: rolls "), "{first}");
        assert!(first.contains(" to hit (15 armour \u{2212} 4 attack) "), "{first}");
        assert_eq!(record.life.len(), record.rounds as usize + 1);
        let swapped = LabState { swapped: true, ..state.clone() };
        let record = fight(&swapped, &swapped.ruleset.rules, &mut Mulberry32::new(5));
        assert!(!record.blows[0].by_attacker, "swapped, the character strikes first");
        assert!(record.blows[0].line.starts_with("They swing the Messer: rolls "), "{}", record.blows[0].line);
    }

    #[test]
    fn a_folk_attacker_holds_a_base_at_strength_10() {
        let mut state = rat();
        state.attacker = Attacker::Folk("Sword".into());
        let summary = fights(&state, &state.ruleset.rules, 1, 200);
        assert_eq!(summary.attacker, "Folk with Sword");
        assert!(summary.attacker_hit_rate > 0.0);
        let record = fight(&state, &state.ruleset.rules, &mut Mulberry32::new(2));
        assert!(record.blows[0].line.starts_with("Folk with Sword strikes: rolls "), "{}", record.blows[0].line);
    }

    #[test]
    fn the_rules_decide_the_fight() {
        let mut state = rat();
        let before = fights(&state, &state.ruleset.rules, 1, 500);
        crate::lab_rules::set_leaf(&mut state.ruleset, "creatures/Giant Rat/hit_points", crate::lab_rules::Leaf::Int(1));
        let after = fights(&state, &state.ruleset.rules, 1, 500);
        assert!(after.they_win > before.they_win);
        // A venomous bite still has a damage per hit.
        state.attacker = Attacker::Creature(Species::PoisonousSnake);
        assert!(fights(&state, &state.ruleset.rules, 1, 200).damage_per_hit.is_some());
    }

    /// 1,000 fights take well under a second even unoptimised, and a few tens of milliseconds
    /// in a release build (measured: see the bound).
    #[test]
    fn a_thousand_fights_are_quick() {
        let state = rat();
        let started = std::time::Instant::now();
        fights(&state, &state.ruleset.rules, 1, THOUSAND);
        let took = started.elapsed();
        eprintln!("1,000 fights took {took:?}");
        assert!(took < std::time::Duration::from_millis(1500), "{took:?}");
    }

    #[test]
    fn a_folk_attacker_is_a_real_character_with_armour_and_hit_points_from_the_rules() {
        let mut state = rat();
        state.attacker = Attacker::Folk("Sword".into());
        let setup = Setup::new(&state, &state.ruleset.rules);
        assert_eq!((setup.foe.armour, setup.foe.life), (10, 10), "armour base 10 + DEX 0, d10 + CON 0");
        crate::lab_rules::set_leaf(&mut state.ruleset, "combat/armour_base", crate::lab_rules::Leaf::Int(13));
        crate::lab_rules::set_leaf(&mut state.ruleset, "combat/hit_dice/fighter", crate::lab_rules::Leaf::Int(12));
        let setup = Setup::new(&state, &state.ruleset.rules);
        assert_eq!((setup.foe.armour, setup.foe.life), (13, 12));
    }

    #[test]
    fn a_fight_that_never_ends_is_a_draw_of_its_own() {
        let mut state = rat();
        // Nobody can hurt anybody: the rat bites for nothing and the sword needs a natural 20 on armour 40.
        crate::lab_rules::set_leaf(&mut state.ruleset, "creatures/Giant Rat/bite", crate::lab_rules::Leaf::Dice([0, 0, 0]));
        crate::lab_rules::set_leaf(&mut state.ruleset, "creatures/Giant Rat/armour_class", crate::lab_rules::Leaf::Int(40));
        crate::lab_rules::set_leaf(&mut state.ruleset, "combat/natural_hit", crate::lab_rules::Leaf::Int(21));
        let summary = fights(&state, &state.ruleset.rules, 1, 20);
        assert_eq!(summary.draws, 20);
        assert_eq!(summary.rounds.iter().sum::<u32>(), 0);
    }
}
