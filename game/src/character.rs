//! The delver: who they are (folk, body, name, class and the four ability scores), rolled from
//! a seed as the plate's `rollSpec` rolls a character (`reference/forever-below-plate.html`).
//! What they wear and carry is the [`crate::inventory::Inventory`]'s.
//!
//! `FB_FOLK=<folk>` makes the delver one of that folk, for captures.

use bevy::prelude::*;
use fb_body::{BodySpec, Folk};
use procgen_rng::Mulberry32;

/// The four classes (`docs/design.md`); each has its outfit in `data/outfits.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Fighter,
    Ranger,
    Mage,
    Cleric,
}

impl Class {
    pub const ALL: [Class; 4] = [Class::Fighter, Class::Ranger, Class::Mage, Class::Cleric];

    /// The class's outfit key in `data/outfits.json`.
    pub fn outfit_key(self) -> &'static str {
        match self {
            Class::Fighter => "fighter",
            Class::Ranger => "ranger",
            Class::Mage => "mage",
            Class::Cleric => "cleric",
        }
    }

    /// The word the header uses, lower case as the plate's `cline` writes it.
    pub fn word(self) -> &'static str {
        self.outfit_key()
    }
}

/// Strength, dexterity, constitution and intelligence, in that order (the plate's `C.stats`).
pub type Abilities = [i32; 4];

/// The ability modifier for a score, `floor((score - 10) / 2)` (the plate's `md`, as in the
/// [D&D 5e SRD 5.1](https://media.wizards.com/2016/downloads/DND/SRD-OGL_V5.1.pdf), "Ability
/// Scores and Modifiers").
pub fn modifier(score: i32) -> i32 {
    (score - 10).div_euclid(2)
}

/// Who the delver is.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Character {
    pub folk: Folk,
    /// The seed their body is rolled from ([`BodySpec::roll`]).
    pub body_seed: u32,
    /// Rolled from the folk's syllables in `data/syl.json` (the plate's `genName`).
    pub name: String,
    pub class: Class,
    pub abilities: Abilities,
    /// How dirty they are, 0 clean to 1 filthy (the plate's `C.dirt`).
    pub dirt: f32,
}

impl Default for Character {
    fn default() -> Self {
        Character::roll(1)
    }
}

impl Character {
    /// A delver rolled from `seed`: the same seed always gives the same delver.
    ///
    /// As the plate's `rollSpec`: each ability is the folk's score in `data/races.json` moved
    /// by `round((r + r + r - 1.5) * 3)` and held within 3 to 18, and a freshly fallen
    /// delver's dirt is 0.3 to 0.5. The class is one of the four, where the plate picked
    /// among the outfits.
    pub fn roll(seed: u32) -> Character {
        let mut rng = Mulberry32::new(seed.wrapping_mul(0x2c1b_3c6d) ^ 0x297a_2d39);
        let folk = *rng.pick(&Folk::ALL);
        Character::roll_as(folk, &mut rng)
    }

    /// [`Character::roll`] as one of `folk`.
    pub fn roll_folk(folk: Folk, seed: u32) -> Character {
        let mut rng = Mulberry32::new(seed.wrapping_mul(0x2c1b_3c6d) ^ 0x297a_2d39);
        rng.next_f64();
        Character::roll_as(folk, &mut rng)
    }

    fn roll_as(folk: Folk, rng: &mut Mulberry32) -> Character {
        let body_seed = (rng.next_f64() * 4_294_967_296.0) as u32;
        let class = *rng.pick(&Class::ALL);
        let name = fb_items::naming::folk_name(rng, folk.key());
        let race = folk.race();
        let abilities = race.stats.map(|v| {
            let nudge = ((rng.next_f64() + rng.next_f64() + rng.next_f64() - 1.5) * 3.0).round() as i32;
            (v + nudge).clamp(3, 18)
        });
        let dirt = 0.3 + rng.next_f64() as f32 * 0.2;
        Character { folk, body_seed, name, class, abilities, dirt }
    }

    /// Their body's spec.
    pub fn spec(&self) -> BodySpec {
        BodySpec::roll(self.folk, self.body_seed)
    }

    pub fn strength(&self) -> i32 {
        self.abilities[0]
    }

    pub fn dexterity(&self) -> i32 {
        self.abilities[1]
    }
}

/// Rolls the delver from `seed`, or as one of the folk `FB_FOLK` names.
pub fn character_for(seed: u32) -> Character {
    match std::env::var("FB_FOLK").ok().and_then(|f| Folk::parse(&f)) {
        Some(folk) => Character::roll_folk(folk, seed),
        None => Character::roll(seed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_delver_is_a_pure_function_of_their_seed() {
        for seed in [1, 7, 1207] {
            let delver = Character::roll(seed);
            assert_eq!(delver, Character::roll(seed));
            assert!(delver.abilities.iter().all(|a| (3..=18).contains(a)));
            assert!((0.3..0.5).contains(&delver.dirt));
            assert!(!delver.name.is_empty());
        }
        let folk: std::collections::HashSet<Folk> = (0..60).map(|s| Character::roll(s).folk).collect();
        assert_eq!(folk.len(), Folk::ALL.len(), "every folk turns up");
    }

    #[test]
    fn goblins_and_lizardfolk_have_one_name() {
        assert!(!Character::roll_folk(Folk::Goblin, 3).name.contains(' '));
        assert!(Character::roll_folk(Folk::Human, 3).name.contains(' '));
        assert_eq!(Character::roll_folk(Folk::Dwarf, 9).folk, Folk::Dwarf);
    }

    #[test]
    fn modifiers_follow_the_srd() {
        assert_eq!([modifier(3), modifier(9), modifier(10), modifier(11), modifier(18)], [-4, -1, 0, 0, 4]);
    }
}
