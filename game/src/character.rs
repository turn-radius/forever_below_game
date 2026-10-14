//! The delver: who they are (folk, body, name, class and the four ability scores), rolled from
//! a seed as the plate's `rollSpec` rolls a character (`reference/forever-below-plate.html`).
//! What they wear and carry is the [`crate::inventory::Inventory`]'s.
//!
//! `FB_FOLK=<folk>` makes the delver one of that folk, for captures.

use bevy::prelude::*;
use fb_body::{BodySpec, Folk};
use procgen_rng::Mulberry32;

use crate::carry::carried_light;
use crate::inventory::Inventory;

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

/// What the delver's gear makes of them (the plate's `derive`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Derived {
    /// 10, plus every worn item's armour, plus the dexterity modifier.
    pub armour_class: i32,
    /// Pounds worn and carried: everything worn and everything inside it.
    pub load: f32,
    /// Pounds they can carry: strength times 15 plus what worn items add.
    pub capacity: f32,
    pub warmth: f32,
    /// How far the carried light reaches, in feet; 0 with none.
    pub light: f32,
    /// How far they see without light, in feet: their folk's, or a worn item's if further.
    pub darkvision: f32,
}

impl Character {
    /// What everything equipped adds to the delver, as the plate's `derive` adds it up; the
    /// light is the carried light's ([`carried_light`]) where the plate took the brightest of
    /// all its items, so an unlit torch in the pack gives none.
    pub fn derive(&self, inventory: &Inventory) -> Derived {
        let equipped = inventory.equipped();
        let sum = |f: fn(&fb_items::stats::Stats) -> f32| equipped.iter().map(|(_, item)| f(&item.profile.stats)).sum::<f32>();
        let load = sum(|s| s.weight)
            + equipped.iter().filter_map(|(slot, _)| inventory.worn(*slot)).map(|id| inventory.contents_weight(id)).sum::<f32>();
        let light = carried_light(inventory).and_then(|(_, id)| inventory.item(id)).map_or(0.0, |item| item.profile.stats.light);
        let darkvision = equipped.iter().map(|(_, item)| item.profile.stats.darkvision).fold(self.folk.race().dv as f32, f32::max);
        Derived {
            armour_class: 10 + sum(|s| s.armour).round() as i32 + modifier(self.dexterity()),
            load,
            capacity: (self.strength() * 15) as f32 + sum(|s| s.carry),
            warmth: sum(|s| s.warmth),
            light,
            darkvision,
        }
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
    fn equipped_stats_add_to_the_delver() {
        use fb_items::Slot;
        let delver = Character { abilities: [11, 12, 10, 10], folk: Folk::Human, ..Character::roll(4) };
        let mut inventory = Inventory::default();
        let bare = delver.derive(&inventory);
        assert_eq!((bare.armour_class, bare.load, bare.capacity, bare.light), (11, 0.0, 165.0, 0.0));
        let mail = (1..300).filter_map(|s| fb_items::roll_item("Mail", s)).find(|i| i.profile.stats.armour > 0.0).unwrap();
        let (armour, weight) = (mail.profile.stats.armour, mail.profile.stats.weight);
        let id = inventory.add(mail);
        inventory.wear(id, Slot::Body).unwrap();
        let torch = inventory.add(crate::carry::lit_torch(2));
        inventory.wear(torch, Slot::Offhand).unwrap();
        let worn = delver.derive(&inventory);
        assert_eq!(worn.armour_class, 11 + armour.round() as i32);
        assert!(worn.load >= weight && worn.light > 0.0, "{worn:?}");
    }

    #[test]
    fn modifiers_follow_the_srd() {
        assert_eq!([modifier(3), modifier(9), modifier(10), modifier(11), modifier(18)], [-4, -1, 0, 0, 4]);
    }
}
