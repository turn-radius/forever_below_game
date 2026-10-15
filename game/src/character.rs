//! The delver: who they are (folk, body, name, class and the four ability scores), rolled from
//! a seed as the plate's `rollSpec` rolls a character (`reference/forever-below-plate.html`).
//! What they wear and carry is the [`crate::inventory::Inventory`]'s.
//!
//! `FB_FOLK=<folk>` makes the delver one of that folk, for captures.

use bevy::prelude::*;
use fb_body::{BodySpec, Folk};
use fb_character::{Activity, Attributes, CarriedLight, Gear, LoadBand, Worn, modifier as ability_modifier};
use procgen_rng::Mulberry32;

use crate::carry::carried_lights;
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

/// The ability modifier for a score, `floor((score - 10) / 2)` (the plate's `md`, as in the
/// [D&D 5e SRD 5.1](https://media.wizards.com/2016/downloads/DND/SRD-OGL_V5.1.pdf), "Ability
/// Scores and Modifiers").
pub fn modifier(score: i32) -> i32 {
    ability_modifier(score)
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
    pub attributes: Attributes,
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
        let [str_base, dex_base, con_base, int_base, wis_base] = race.stats;
        // Roll each attribute: nudge and clamp to 3–18. WIS is drawn last, so existing seeds
        // produce the same first four scores; only the WIS roll and dirt shift.
        let mut roll = |base: i32| {
            let nudge = ((rng.next_f64() + rng.next_f64() + rng.next_f64() - 1.5) * 3.0).round() as i32;
            (base + nudge).clamp(3, 18)
        };
        // Fields evaluated in written order: strength, dexterity, constitution, intelligence, wisdom.
        let attributes = Attributes {
            strength: roll(str_base),
            dexterity: roll(dex_base),
            constitution: roll(con_base),
            intelligence: roll(int_base),
            wisdom: roll(wis_base),
        };
        let dirt = 0.3 + rng.next_f64() as f32 * 0.2;
        Character { folk, body_seed, name, class, attributes, dirt }
    }

    /// Their body's spec.
    pub fn spec(&self) -> BodySpec {
        BodySpec::roll(self.folk, self.body_seed)
    }

    pub fn strength(&self) -> i32 {
        self.attributes.strength
    }

    pub fn dexterity(&self) -> i32 {
        self.attributes.dexterity
    }

    pub fn constitution(&self) -> i32 {
        self.attributes.constitution
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
    /// Load band (Light, Burdened, Encumbered, Overloaded).
    pub load_band: LoadBand,
    pub warmth: f32,
    /// How far the carried light reaches, in feet; 0 with none.
    pub light: f32,
    /// How far they see without light, in feet: their folk's, or a worn item's if further.
    pub darkvision: f32,
}

impl Character {
    /// What they wear and carry means for the simulation: the one place capacity, the meter
    /// rates, darkvision and the carried light come from. Their folk's darkvision counts with
    /// the items' as the best of them, and the carried light is the farthest reaching of the
    /// lit things carried ([`carried_lights`]).
    pub fn gear(&self, inventory: &Inventory) -> Gear {
        let worn = inventory.equipped().into_iter().map(|(_, item)| {
            let s = &item.profile.stats;
            Worn { carry: s.carry as f64, darkvision: s.darkvision as f64, hunger_rate: s.hunger_rate as f64, sleepiness_rate: s.sleepiness_rate as f64 }
        });
        let lights = carried_lights(inventory)
            .into_iter()
            .filter_map(|(_, id)| inventory.item(id))
            .map(|item| CarriedLight { reach: item.profile.stats.light as f64, burn_time: item.profile.stats.burn_time as f64 });
        Gear::new(self.folk.race().dv as f64, worn, lights)
    }

    /// What everything equipped adds to the delver, as the plate's `derive` adds it up; the
    /// light is the farthest reaching of the lit things carried ([`carried_lights`]), where the
    /// plate took the brightest of all its items, so an unlit torch in the pack gives none.
    pub fn derive(&self, inventory: &Inventory) -> Derived {
        let equipped = inventory.equipped();
        // Folded from +0, since an empty `Sum` of floats is -0 and would read "-0".
        let sum = |f: fn(&fb_items::stats::Stats) -> f32| equipped.iter().map(|(_, item)| f(&item.profile.stats)).fold(0.0, |a, b| a + b);
        let load = sum(|s| s.weight)
            + equipped.iter().filter_map(|(slot, _)| inventory.worn(*slot)).map(|id| inventory.contents_weight(id)).fold(0.0, |a, b| a + b);
        let gear = self.gear(inventory);
        let capacity = gear.capacity_lb(self.strength()) as f32;
        let load_band = LoadBand::from_load(load as f64, capacity as f64);
        Derived {
            armour_class: 10 + sum(|s| s.armour).round() as i32 + modifier(self.dexterity()),
            load,
            capacity,
            load_band,
            warmth: sum(|s| s.warmth),
            light: gear.light as f32,
            darkvision: gear.darkvision as f32,
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

/// The delver's load band, kept on the [`Player`](crate::play::Player) so walking need not
/// work out [`Character::derive`] every frame. `player::weigh_the_delver` refreshes it when
/// the character or the inventory changes.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Load(pub LoadBand);

/// The delver's hunger, thirst and sleepiness, on the [`Player`](crate::play::Player) from their
/// spawn, so a new delve starts Fed. Eating and drinking take from it (`crate::eat`); the game
/// clock steps it in #229.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct DelverNeeds(pub fb_character::Needs);

/// The delver's stamina, on the [`Player`](crate::play::Player) from their spawn, so a new delve
/// starts with a full pool. Sprinting and swinging drain it, and it refills in seconds
/// (`crate::simulate::step_stamina`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct DelverStamina(pub fb_character::Stamina);

/// Determines the delver's activity from movement and load state.
/// Moving while Burdened or Encumbered counts as Exerting. An Overloaded delver cannot move
/// (`player::walk`), so wanting to move while Overloaded is reported as Resting, as is
/// standing still.
pub fn activity_for(moving: bool, sprinting: bool, swinging: bool, load_band: LoadBand) -> Activity {
    match (moving, sprinting, swinging) {
        (false, _, _) => Activity::Resting,
        (true, _, true) => Activity::Exerting,
        (true, true, _) => Activity::Exerting,
        (true, false, false) => {
            // Moving while Burdened or Encumbered counts as Exerting
            match load_band {
                LoadBand::Light => Activity::Walking,
                LoadBand::Burdened | LoadBand::Encumbered => Activity::Exerting,
                LoadBand::Overloaded => Activity::Resting, // Can't move anyway
            }
        }
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
            for attr in [delver.attributes.strength, delver.attributes.dexterity, delver.attributes.constitution, delver.attributes.intelligence, delver.attributes.wisdom] {
                assert!((3..=18).contains(&attr));
            }
            assert!((0.3..0.5).contains(&delver.dirt));
            assert!(!delver.name.is_empty());
        }
        let folk: std::collections::HashSet<Folk> = (0..60).map(|s| Character::roll(s).folk).collect();
        assert_eq!(folk.len(), Folk::ALL.len(), "every folk turns up");
    }

    #[test]
    fn every_folk_has_five_attributes() {
        for folk in Folk::ALL {
            let race = folk.race();
            assert_eq!(race.stats.len(), 5, "folk {folk:?} has stat array of length {}", race.stats.len());
            for stat in race.stats {
                assert!((3..=18).contains(&stat), "folk {folk:?} stat {stat} out of range");
            }
        }
    }

    #[test]
    fn rolling_preserves_rng_stream_with_wis_last() {
        // Pinning the first four scores ensures WIS is drawn last: existing seeds give the
        // same STR, DEX, CON, INT but different WIS and dirt (shifted by the WIS roll).
        let delver1 = Character::roll(1);
        assert_eq!((delver1.attributes.strength, delver1.attributes.dexterity, delver1.attributes.constitution, delver1.attributes.intelligence), (7, 12, 16, 7));
        let delver7 = Character::roll(7);
        assert_eq!((delver7.attributes.strength, delver7.attributes.dexterity, delver7.attributes.constitution, delver7.attributes.intelligence), (12, 9, 8, 12));
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
        let test_attrs = Attributes { strength: 11, dexterity: 12, constitution: 10, intelligence: 10, wisdom: 10 };
        let delver = Character { attributes: test_attrs, folk: Folk::Human, ..Character::roll(4) };
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
    fn a_lit_hip_lantern_is_the_light_when_no_hand_holds_one() {
        use fb_items::Slot;
        let delver = Character::roll(4);
        let mut inventory = Inventory::default();
        let belt = inventory.add(fb_items::roll_item("Belt", 2).unwrap());
        inventory.wear(belt, Slot::Belt).unwrap();
        let lantern = (1..200).filter_map(|s| fb_items::roll_item("Lantern", s)).find(|i| crate::carry::item_light(&i.shape).is_some()).unwrap();
        let reach = lantern.profile.stats.light;
        let lantern = inventory.add(lantern);
        inventory.wear(lantern, Slot::Hip).unwrap();
        // Both hands hold weapons, and the hip lantern lights the way.
        for (base, slot) in [("Sword", Slot::Mainhand), ("Dagger", Slot::Offhand)] {
            let id = inventory.add(fb_items::roll_item(base, 3).unwrap());
            inventory.wear(id, slot).unwrap();
        }
        assert_eq!(delver.derive(&inventory).light, reach);
        assert_eq!(crate::carry::carried_light(&inventory), Some((Slot::Hip, lantern)));
        // A lit torch in hand is carried before it.
        let dagger = inventory.worn(Slot::Offhand).unwrap();
        inventory.take_out(dagger);
        let torch = inventory.add(crate::carry::lit_torch(2));
        inventory.wear(torch, Slot::Offhand).unwrap();
        assert_eq!(crate::carry::carried_light(&inventory).map(|(slot, _)| slot), Some(Slot::Offhand));
    }

    /// A rolled `base` with the real affix `name` applied to its stats.
    fn with_affix(base: &str, seed: u32, name: &str) -> fb_items::Item {
        let mut item = fb_items::roll_item(base, seed).unwrap();
        let affix = fb_items::tables::affixes().iter().find(|a| a.name == name).unwrap();
        affix.apply_to_stats(&mut item.profile.stats);
        item.affixes.push(affix);
        item
    }

    #[test]
    fn a_belt_of_the_ox_lifts_a_human_to_175_lb() {
        use fb_items::Slot;
        let attributes = Attributes { strength: 10, dexterity: 10, constitution: 10, intelligence: 10, wisdom: 10 };
        let human = Character { folk: Folk::Human, attributes, ..Character::roll(4) };
        let mut inventory = Inventory::default();
        let belt = inventory.add(with_affix("Belt", 2, "of the Ox"));
        inventory.wear(belt, Slot::Belt).unwrap();
        let derived = human.derive(&inventory);
        assert_eq!(derived.capacity, 175.0);
        assert_eq!(derived.capacity as f64, human.gear(&inventory).capacity_lb(10), "one source");
    }

    #[test]
    fn the_mole_gives_a_human_10_ft_of_darkvision_and_a_goblin_60() {
        use fb_items::Slot;
        for (folk, feet) in [(Folk::Human, 10.0), (Folk::Goblin, 60.0)] {
            let delver = Character { folk, ..Character::roll(4) };
            let mut inventory = Inventory::default();
            assert_eq!(delver.derive(&inventory).darkvision, folk.race().dv as f32);
            let belt = inventory.add(with_affix("Belt", 2, "of the Mole"));
            inventory.wear(belt, Slot::Belt).unwrap();
            assert_eq!(delver.derive(&inventory).darkvision, feet, "{folk:?}: the best, not the sum");
        }
    }

    #[test]
    fn plenty_and_wakefulness_slow_the_meters_through_the_gear() {
        use fb_items::Slot;
        let delver = Character::roll(4);
        let mut inventory = Inventory::default();
        let belt = inventory.add(with_affix("Belt", 2, "of Plenty"));
        inventory.wear(belt, Slot::Belt).unwrap();
        let ring = inventory.add(with_affix("Ring", 3, "of Wakefulness"));
        inventory.wear(ring, Slot::Ring1).unwrap();
        let gear = delver.gear(&inventory);
        assert!((gear.hunger_rate - 0.85).abs() < 1e-6 && (gear.sleepiness_rate - 0.85).abs() < 1e-6, "{gear:?}");
        let mut needs = fb_character::Needs::default();
        gear.apply_to(&mut needs);
        needs.step(8.0 / 0.85 - 0.001, Activity::Walking);
        assert_eq!(needs.hunger.word(), "Peckish");
        needs.step(0.002, Activity::Walking);
        assert_eq!(needs.hunger.word(), "Hungry");
    }

    #[test]
    fn light_affixes_change_the_carried_light_and_its_burn_time() {
        use fb_items::Slot;
        let delver = Character::roll(4);
        let reach_with = |affix: Option<&str>| {
            let mut torch = crate::carry::lit_torch(2);
            if let Some(name) = affix {
                let a = fb_items::tables::affixes().iter().find(|a| a.name == name).unwrap();
                a.apply_to_stats(&mut torch.profile.stats);
            }
            let mut inventory = Inventory::default();
            let id = inventory.add(torch);
            inventory.wear(id, Slot::Offhand).unwrap();
            delver.gear(&inventory)
        };
        let plain = reach_with(None);
        assert!(plain.light > 0.0 && plain.burn_time == 1.0);
        assert_eq!(reach_with(Some("Gleaming")).light, plain.light + 10.0);
        assert_eq!(reach_with(Some("Smoky")).light, plain.light - 5.0);
        let long = reach_with(Some("Long-burning"));
        assert_eq!((long.burn_time, long.light), (2.0, plain.light));
    }

    #[test]
    fn featherlight_lowers_the_load_through_weight() {
        use fb_items::Slot;
        let delver = Character::roll(4);
        let load = |affix: Option<&str>| {
            let item = match affix {
                Some(name) => with_affix("Mail", 1, name),
                None => fb_items::roll_item("Mail", 1).unwrap(),
            };
            let mut inventory = Inventory::default();
            let id = inventory.add(item);
            inventory.wear(id, Slot::Body).unwrap();
            delver.derive(&inventory).load
        };
        let (plain, light) = (load(None), load(Some("Featherlight")));
        assert!(plain > 0.0 && (light - plain * 0.6).abs() < 1e-3, "{light} against {plain}");
    }

    #[test]
    fn modifiers_follow_the_srd() {
        assert_eq!([modifier(3), modifier(9), modifier(10), modifier(11), modifier(18)], [-4, -1, 0, 0, 4]);
    }

    #[test]
    fn activity_for_determines_exertion_from_load() {
        assert_eq!(activity_for(false, false, false, LoadBand::Light), Activity::Resting);
        assert_eq!(activity_for(true, false, false, LoadBand::Light), Activity::Walking);
        assert_eq!(activity_for(true, false, false, LoadBand::Burdened), Activity::Exerting);
        assert_eq!(activity_for(true, false, false, LoadBand::Encumbered), Activity::Exerting);
        assert_eq!(activity_for(true, false, false, LoadBand::Overloaded), Activity::Resting);
        assert_eq!(activity_for(true, true, false, LoadBand::Light), Activity::Exerting);
        assert_eq!(activity_for(true, false, true, LoadBand::Light), Activity::Exerting);
    }
}
