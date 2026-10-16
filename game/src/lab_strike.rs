//! The lab's strikes ([`crate::lab_view`]): something strikes the lab's character and each blow
//! is read. The state is plain data and every roll goes through [`fb_character::combat`] (the
//! attack resolver, [`Weapon::roll_explained`], [`rules::bite_damage`]), the same functions the
//! game calls, so it runs without Bevy.
//!
//! The attacker is a creature from [`fb_creatures`], striking with its bite, or a folk holding a
//! melee base: that folk has Strength 10 and its weapon is the base rolled from the strike
//! seed. The strike seed sets every roll: the same seed gives the same blows, and the same
//! hundred.

use std::collections::{BTreeSet, VecDeque};

use fb_character::combat::{self as rules, Attack, Save, creature_attack, folk_attack};
use fb_character::explain::{Explained, number};
use fb_character::modifier;
use fb_creatures::Species;
use fb_items::armory_base::ARMORY_BASES;
use fb_items::tables::bases;
use fb_items::{Tag, roll_item};
use procgen_rng::Mulberry32;

use crate::combat::{Weapon, is_melee};
use crate::lab_view::LabState;

/// How many log lines are kept.
pub const LOG_LINES: usize = 12;
/// How many blows a hundred is.
pub const HUNDRED: u32 = 100;
/// The Strength a folk attacker has.
const FOLK_STRENGTH: i32 = 10;

/// Who strikes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Attacker {
    /// A creature, with its bite.
    Creature(Species),
    /// A folk holding the melee base of this name.
    Folk(String),
}

impl Attacker {
    /// What the attacker is called: "Giant Rat", "folk with Sword".
    pub fn label(&self) -> String {
        match self {
            Attacker::Creature(species) => species.stat_block().name.to_string(),
            Attacker::Folk(base) => format!("folk with {base}"),
        }
    }

    /// Every attacker: each creature, then a folk with each melee base.
    pub fn all() -> Vec<Attacker> {
        Species::ALL.iter().map(|&s| Attacker::Creature(s)).chain(melee_bases().into_iter().map(Attacker::Folk)).collect()
    }
}

/// The bases a folk attacker can hold: weapons whose shape is swung in melee.
pub fn melee_bases() -> Vec<String> {
    use std::sync::LazyLock;
    static BASES: LazyLock<Vec<String>> = LazyLock::new(|| {
        let mut names: BTreeSet<&str> = ARMORY_BASES.iter().map(|(n, _)| *n).collect();
        names.extend(bases().keys().map(String::as_str));
        names.into_iter().filter(|n| roll_item(n, 1).is_some_and(|i| i.profile.tags.contains(&Tag::Weapon) && is_melee(i.shape.family()))).map(String::from).collect()
    });
    BASES.clone()
}

/// A blow: its attack roll, and if it hit, its damage and the save against venom.
#[derive(Clone, Debug, PartialEq)]
pub struct Blow {
    pub attack: Attack,
    pub damage: Option<Explained>,
    pub save: Option<Save>,
}

/// What an attacker strikes with, made ready from the lab state.
enum Arms {
    Creature(Species),
    Folk { weapon: Weapon, to_hit: i32 },
}

impl Arms {
    fn of(state: &LabState) -> Arms {
        match &state.attacker {
            Attacker::Creature(species) => Arms::Creature(*species),
            Attacker::Folk(base) => {
                let item = roll_item(base, state.strike_seed);
                let to_hit = item.as_ref().map_or(0, |i| i.profile.stats.to_hit.round() as i32);
                Arms::Folk { weapon: Weapon::of_stats(item.as_ref()), to_hit }
            }
        }
    }

    /// One blow against `armour_class` on a defender with `constitution_modifier`.
    fn blow(&self, armour_class: i32, constitution_modifier: i32, rng: &mut Mulberry32) -> Blow {
        match self {
            Arms::Creature(species) => {
                let block = species.stat_block();
                let attack = creature_attack(block.attack_bonus, armour_class, rng);
                if !attack.hit {
                    return Blow { attack, damage: None, save: None };
                }
                let damage = block.bite.roll(rng);
                let poison = block.venom.map_or(0, |v| v.roll(rng));
                let bite = rules::bite_damage(damage, poison, constitution_modifier, rng);
                Blow { attack, damage: Some(bite.damage), save: bite.venom.save }
            }
            Arms::Folk { weapon, to_hit } => {
                let attack = folk_attack(modifier(FOLK_STRENGTH), *to_hit, armour_class, rng);
                let damage = attack.hit.then(|| weapon.roll_explained(modifier(FOLK_STRENGTH), rng));
                Blow { attack, damage, save: None }
            }
        }
    }
}

/// The lab character's armour class, as the sheet shows it.
pub fn armour_class(state: &LabState) -> i32 {
    state.character.armour_class(&state.inventory()).value().round() as i32
}

/// The lab character's hit points at full.
pub fn full_hit_points(state: &LabState) -> i32 {
    crate::combat::first_level_hit_points(&state.character)
}

/// What a hundred blows came to.
#[derive(Clone, Debug, PartialEq)]
pub struct Hundred {
    pub armour_class: i32,
    pub hits: u32,
    /// The fewest, mean and most damage over the blows that hit; `None` with no hit.
    pub damage: Option<(u32, f64, u32)>,
    /// The mean blows a kill took from full hit points, over the kills made; `None` with none.
    /// The last fight, if the hundred ended before it did, is dropped.
    pub blows_to_kill: Option<f64>,
    pub kills: u32,
}

impl Hundred {
    /// The summary as one line.
    pub fn line(&self) -> String {
        let damage = match self.damage {
            Some((min, mean, max)) => format!("damage {min} / {} / {max} (min / mean / max)", number(mean)),
            None => "no damage".to_string(),
        };
        let kill = match self.blows_to_kill {
            Some(blows) => format!("{} kills in 100 · {} blows each", self.kills, format!("{blows:.1}")),
            None => "no kill in 100 blows".to_string(),
        };
        format!("100 blows vs AC {}: {} hit · {damage} · {kill}", self.armour_class, self.hits)
    }
}

/// A hundred blows from `state.strike_seed`, a pure function of the state. The hit points are
/// the character's at full, and each kill starts them full again.
pub fn hundred(state: &LabState) -> Hundred {
    let ac = armour_class(state);
    let (arms, con, full) = (Arms::of(state), modifier(state.character.constitution()), full_hit_points(state));
    let mut rng = Mulberry32::new(state.strike_seed);
    let (mut hits, mut total, mut least, mut most) = (0, 0u32, u32::MAX, 0);
    let (mut hp, mut since, mut fights) = (full, 0u32, Vec::new());
    for _ in 0..HUNDRED {
        let blow = arms.blow(ac, con, &mut rng);
        since += 1;
        if let Some(damage) = &blow.damage {
            let d = damage.value() as u32;
            hits += 1;
            total += d;
            least = least.min(d);
            most = most.max(d);
            hp -= d as i32;
            if hp <= 0 {
                fights.push(since);
                (hp, since) = (full, 0);
            }
        }
    }
    let kills = fights.len() as u32;
    Hundred {
        armour_class: ac,
        hits,
        damage: (hits > 0).then(|| (least, total as f64 / hits as f64, most)),
        blows_to_kill: (kills > 0).then(|| fights.iter().sum::<u32>() as f64 / kills as f64),
        kills,
    }
}

/// The strike log and the character's hit points under it.
#[derive(bevy::prelude::Resource, Clone, Debug, Default)]
pub struct Strikes {
    /// Hit points now; `None` until the first blow lands, so they follow the sheet's.
    hp: Option<i32>,
    /// The latest lines, oldest first, at most [`LOG_LINES`].
    pub log: VecDeque<String>,
    /// The seed the stream was started from, and the stream.
    rng: Option<(u32, Mulberry32)>,
    /// The last hundred's summary.
    pub summary: Option<String>,
}

impl Strikes {
    /// Hit points now and at full.
    pub fn hit_points(&self, state: &LabState) -> (i32, i32) {
        let full = full_hit_points(state);
        (self.hp.map_or(full, |hp| hp.min(full)), full)
    }

    fn say(&mut self, line: String) {
        self.log.push_back(line);
        while self.log.len() > LOG_LINES {
            self.log.pop_front();
        }
    }

    /// One blow on the character, logged with its attack roll and, on a hit, its damage and
    /// the hit points it took them from and to.
    pub fn strike(&mut self, state: &LabState) {
        let (hp, _) = self.hit_points(state);
        if hp <= 0 {
            self.say("The character is down. Heal to strike again.".to_string());
            return;
        }
        if self.rng.as_ref().is_none_or(|(seed, _)| *seed != state.strike_seed) {
            self.rng = Some((state.strike_seed, Mulberry32::new(state.strike_seed)));
        }
        let rng = &mut self.rng.as_mut().expect("seeded above").1;
        let blow = Arms::of(state).blow(armour_class(state), modifier(state.character.constitution()), rng);
        let mut line = format!("{}: {}", state.attacker.label(), blow.attack.line());
        if let Some(damage) = &blow.damage {
            if let Some(save) = &blow.save {
                line.push_str(&format!(" · {} vs DC {}: {}", save.roll.line(), save.dc, if save.succeeded { "saved" } else { "failed" }));
            }
            let after = hp - damage.value() as i32;
            line.push_str(&format!(" · {} · HP {hp} → {after}{}", damage.line(), if after <= 0 { ", down" } else { "" }));
            self.hp = Some(after);
        }
        self.say(line);
    }

    /// Strikes a hundred times for the summary. Leaves hit points and the log alone.
    pub fn strike_hundred(&mut self, state: &LabState) {
        self.summary = Some(hundred(state).line());
    }

    /// Hit points back to full, the log and the summary cleared, the stream back to its start.
    pub fn heal(&mut self) {
        *self = Strikes::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A human with AC 12 (10 + DEX 14's +2, nothing worn) struck by `attacker`.
    fn state(attacker: Attacker) -> LabState {
        let mut state = LabState::default();
        state.slots.iter_mut().for_each(|s| s.base = None);
        state.character.attributes.dexterity = 14;
        state.attacker = attacker;
        state
    }

    #[test]
    fn a_rat_striking_ac_12_logs_the_roll_and_its_bite() {
        let state = state(Attacker::Creature(Species::Rat));
        assert_eq!(armour_class(&state), 12);
        let (mut hits, mut misses) = (0, 0);
        for seed in 0..40 {
            let mut state = state.clone();
            state.strike_seed = seed;
            let mut strikes = Strikes::default();
            strikes.strike(&state);
            let line = strikes.log.back().unwrap();
            assert!(line.starts_with("Rat: d20 ("), "{line}");
            assert!(line.contains("vs AC 12"), "{line}");
            if line.contains(": hit") {
                hits += 1;
                assert!(line.contains("Bite 1 = bite (1)") && line.contains("HP 10 → 9"), "{line}");
            } else {
                misses += 1;
                assert!(!line.contains("HP"), "{line}");
            }
        }
        assert!(hits > 0 && misses > 0, "{hits} hits, {misses} misses");
    }

    #[test]
    fn a_hundred_in_between_does_not_change_the_next_blow() {
        for attacker in [Attacker::Creature(Species::GiantRat), Attacker::Folk("Sword".into())] {
            let mut state = state(attacker);
            state.strike_seed = 9;
            let (mut plain, mut between) = (Strikes::default(), Strikes::default());
            plain.strike(&state);
            plain.strike(&state);
            between.strike(&state);
            between.strike_hundred(&state);
            between.strike(&state);
            assert_eq!(plain.log, between.log);
            assert_eq!(plain.hp, between.hp);
        }
    }

    #[test]
    fn a_hit_takes_hit_points_and_heal_restores_them() {
        let mut state = state(Attacker::Creature(Species::GiantRat));
        state.strike_seed = 1;
        let mut strikes = Strikes::default();
        let full = full_hit_points(&state);
        for _ in 0..30 {
            strikes.strike(&state);
        }
        let (hp, _) = strikes.hit_points(&state);
        assert!(hp < full, "{hp}");
        assert!(strikes.log.len() <= LOG_LINES);
        strikes.strike_hundred(&state);
        strikes.heal();
        assert_eq!((strikes.hit_points(&state).0, strikes.log.len(), strikes.summary.clone()), (full, 0, None));
    }

    #[test]
    fn the_same_seed_gives_the_same_blows_and_the_same_hundred() {
        for attacker in [Attacker::Creature(Species::PoisonousSnake), Attacker::Folk("Sword".into())] {
            let mut state = state(attacker);
            state.strike_seed = 77;
            assert_eq!(hundred(&state), hundred(&state));
            let (mut a, mut b) = (Strikes::default(), Strikes::default());
            for _ in 0..5 {
                a.strike(&state);
                b.strike(&state);
            }
            assert_eq!(a.log, b.log);
            state.strike_seed = 78;
            assert_ne!(hundred(&state), hundred(&LabState { strike_seed: 77, ..state.clone() }));
        }
    }

    #[test]
    fn a_saved_venom_is_halved_and_the_hit_points_drop_by_the_total() {
        let mut state = state(Attacker::Creature(Species::PoisonousSnake));
        state.character.attributes.dexterity = 3;
        let block = Species::PoisonousSnake.stat_block();
        let full = full_hit_points(&state);
        let (mut saved, mut failed) = (0, 0);
        for seed in 0..300 {
            state.strike_seed = seed;
            let blow = Arms::of(&state).blow(armour_class(&state), 0, &mut Mulberry32::new(seed));
            let Some(save) = blow.save else { continue };
            // The same draws by hand: the d20, the bite, the venom.
            let mut rng = Mulberry32::new(seed);
            rng.int(1, 20);
            let (bite, venom) = (block.bite.roll(&mut rng), block.venom.unwrap().roll(&mut rng));
            let through = if save.succeeded {
                saved += 1;
                venom / 2
            } else {
                failed += 1;
                venom
            };
            assert_eq!(blow.damage.unwrap().value() as u32, bite + through, "seed {seed}");
            let mut strikes = Strikes::default();
            strikes.strike(&state);
            assert_eq!(strikes.hit_points(&state).0, full - (bite + through) as i32, "seed {seed}: {:?}", strikes.log);
            assert!(strikes.log.back().unwrap().contains("CON save"));
        }
        assert!(saved > 0 && failed > 0, "{saved} saved, {failed} failed");
    }

    #[test]
    fn raising_dex_from_10_to_14_adds_2_to_ac_and_lowers_the_hit_rate() {
        let mut low = state(Attacker::Creature(Species::GiantRat));
        low.character.attributes.dexterity = 10;
        let mut high = low.clone();
        high.character.attributes.dexterity = 14;
        assert_eq!(armour_class(&high), armour_class(&low) + 2);
        let (a, b) = (hundred(&low), hundred(&high));
        assert!(b.hits < a.hits, "{} against {}", b.hits, a.hits);
        assert!(b.line().contains(&format!("vs AC {}", armour_class(&high))));
    }

    #[test]
    fn a_folk_attacker_swings_the_base_rolled_from_the_seed() {
        let mut state = state(Attacker::Folk("Mace".into()));
        state.strike_seed = 5;
        let h = hundred(&state);
        let (min, _, max) = h.damage.expect("a hundred blows at AC 12 hit");
        let mace = Weapon::of_stats(roll_item("Mace", 5).as_ref());
        assert_eq!((min, max), mace.damage_range(0), "STR 10 adds nothing");
        assert!(Attacker::all().contains(&Attacker::Folk("Mace".into())));
        assert!(!melee_bases().iter().any(|b| b == "Bow"));
    }

    #[test]
    fn every_attacker_strikes() {
        let mut state = state(Attacker::Creature(Species::Rat));
        for attacker in Attacker::all() {
            state.attacker = attacker;
            assert!(hundred(&state).hits <= HUNDRED);
        }
        assert!(Attacker::all().len() > Species::ALL.len());
    }
}
