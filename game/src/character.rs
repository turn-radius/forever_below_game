//! The delver: who they are (folk, body, name, class and the four ability scores), rolled from
//! a seed as the plate's `rollSpec` rolls a character (`reference/forever-below-plate.html`).
//! What they wear and carry is the [`crate::inventory::Inventory`]'s.
//!
//! `FB_FOLK=<folk>` makes the delver one of that folk, for captures.

use bevy::prelude::*;
use fb_body::{BodyShape, BodySpec, Folk};
use fb_character::explain::{Explained, Loadout, armour_class_with};
use fb_character::rules::Rules;
use fb_character::{AbilityBonus, Activity, Attributes, CarriedLight, Casting, Gear, LoadBand, Mana, Worn, modifier as ability_modifier};
use procgen_rng::Mulberry32;

use crate::carry::carried_lights;
use crate::inventory::Inventory;

/// Whether a delver is a woman or a man: what the creator's Sex choice sets. Folk have one too.
pub use fb_body::Sex;

/// The four classes (`docs/design.md`); each has its outfit in `data/outfits.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Class {
    Fighter,
    Ranger,
    Mage,
    Cleric,
}

impl Class {
    pub const ALL: [Class; 4] = [Class::Fighter, Class::Ranger, Class::Mage, Class::Cleric];

    /// The class as the factions' likes and hates name it.
    pub fn faction_class(self) -> fb_faction::ClassKind {
        match self {
            Class::Fighter => fb_faction::ClassKind::Fighter,
            Class::Ranger => fb_faction::ClassKind::Ranger,
            Class::Mage => fb_faction::ClassKind::Mage,
            Class::Cleric => fb_faction::ClassKind::Cleric,
        }
    }

    /// The class's outfit key in `data/outfits.json`.
    pub fn outfit_key(self) -> &'static str {
        match self {
            Class::Fighter => "fighter",
            Class::Ranger => "ranger",
            Class::Mage => "mage",
            Class::Cleric => "cleric",
        }
    }

    /// The alignment a delver of this class starts with, which sets where each faction's
    /// standing begins (`fb_faction::Standings::start_from_alignment`). Not chosen in the
    /// creator yet: it is the class's typical one.
    pub fn alignment(self) -> fb_faction::Alignment {
        use fb_faction::Alignment::*;
        match self {
            Class::Fighter => LawfulNeutral,
            Class::Ranger => NeutralGood,
            Class::Mage => TrueNeutral,
            Class::Cleric => LawfulGood,
        }
    }

    /// How the class casts: Mages from INT, Clerics from WIS, Fighters and Rangers hardly at all.
    pub fn casting(self) -> Casting {
        match self {
            Class::Fighter | Class::Ranger => Casting::Martial,
            Class::Mage => Casting::Arcane,
            Class::Cleric => Casting::Divine,
        }
    }

    /// The class's hit die under the default rules: d10 for fighters and rangers, d8 for
    /// clerics, d6 for mages (SRD 5.1 "Hit Points": fighter p. 24, ranger p. 35, cleric p. 15,
    /// wizard, our mage, p. 52).
    pub fn hit_die(self) -> u32 {
        self.hit_die_with(fb_character::rules::defaults())
    }

    /// The class's hit die from `rules` (`combat.hit_dice` in `data/rules.toml`).
    pub fn hit_die_with(self, rules: &fb_character::rules::Rules) -> u32 {
        let dice = &rules.combat.hit_dice;
        match self {
            Class::Fighter => dice.fighter,
            Class::Ranger => dice.ranger,
            Class::Cleric => dice.cleric,
            Class::Mage => dice.mage,
        }
    }

    /// The class whose outfit is `key` in `data/outfits.json`, or `None` for a look that is no
    /// class (the freshly fallen, the scavenger).
    pub fn of_outfit(key: &str) -> Option<Class> {
        Class::ALL.into_iter().find(|c| c.outfit_key() == key)
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
    /// The body's shape as the creator's sliders set it, over the rolled one; `None` keeps the
    /// rolled shape (folk met below, and delvers saved before the sliders).
    pub shape: Option<BodyShape>,
    /// As the creator chose; `None` for folk met below and delvers saved before the choice.
    pub sex: Option<Sex>,
    /// Rolled from the folk's syllables in `data/syl.json` (the plate's `genName`).
    pub name: String,
    pub class: Class,
    /// The ability scores they were rolled with; [`Character::scores`] adds what they have
    /// learned and what they wear.
    pub attributes: Attributes,
    /// How dirty they are, 0 clean to 1 filthy (the plate's `C.dirt`).
    pub dirt: f32,
    /// What their worn items add to their ability scores, kept up from the inventory
    /// ([`wear_abilities`]).
    pub worn: AbilityBonus,
    /// What the books they have read add to their ability scores, for good ([`Character::read`]).
    pub learned: AbilityBonus,
    /// The seeds of the books they have read: each teaches them once.
    pub books_read: Vec<u32>,
    /// What a cleric has devoted themselves to, chosen on the character sheet and changeable
    /// ([`fb_character::devotion`]); `None` for the other classes and until they choose.
    pub devotion: Option<fb_character::devotion::Devotion>,
    /// What they have done: kills by kind, family and weapon, hits taken in each armour, offers answered ([`fb_character::deeds`]).
    pub deeds: fb_character::deeds::Tally,
}

/// What reading a book did ([`Character::read`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    /// They learned from it: `stat` is now `score`.
    Learned { stat: fb_items::stats::Stat, score: i32 },
    /// They have read it before, and it has nothing more to teach them.
    AlreadyRead,
    /// It is not a book.
    NothingToLearn,
}

impl Default for Character {
    fn default() -> Self {
        Character::roll(1)
    }
}

/// Where a fitting holds what it holds, for the load's breakdown ("on the strap").
fn fitting_place(accessory: fb_items::Accessory) -> &'static str {
    match accessory {
        fb_items::Accessory::WeaponStrap => "on the strap",
        fb_items::Accessory::Lashing => "on the lashing",
        fb_items::Accessory::LeftFrog | fb_items::Accessory::RightFrog => "in a frog",
        _ => "in the potion pouch",
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
        let name = fb_items::naming::folk_name(rng, folk.key(), None);
        // WIS is drawn last, so existing seeds produce the same first four scores; only the WIS
        // roll and dirt shift.
        let nudges = Character::roll_nudges(rng);
        let attributes = Character::scores_from(folk, nudges);
        let dirt = 0.3 + rng.next_f64() as f32 * 0.2;
        Character { folk, body_seed, shape: None, sex: None, name, class, attributes, dirt, worn: AbilityBonus::default(), learned: AbilityBonus::default(), books_read: Vec::new(), devotion: None, deeds: Default::default() }
    }

    /// Five nudges to the folk's ability scores, in order strength, dexterity, constitution,
    /// intelligence, wisdom: each `round((r + r + r - 1.5) * 3)`, so -5 to 4, mostly near 0.
    pub fn roll_nudges(rng: &mut Mulberry32) -> [i32; 5] {
        std::array::from_fn(|_| ((rng.next_f64() + rng.next_f64() + rng.next_f64() - 1.5) * 3.0).round() as i32)
    }

    /// The ability scores `nudges` give one of `folk`: its scores in `data/races.json`, each
    /// moved by its nudge and held within 3 to 18.
    pub fn scores_from(folk: Folk, nudges: [i32; 5]) -> Attributes {
        let [strength, dexterity, constitution, intelligence, wisdom] = std::array::from_fn(|i| (folk.race().stats[i] + nudges[i]).clamp(3, 18));
        Attributes { strength, dexterity, constitution, intelligence, wisdom }
    }

    /// Their body's spec.
    pub fn spec(&self) -> BodySpec {
        let rolled = BodySpec::roll(self.folk, self.body_seed);
        let mut spec = match self.shape {
            Some(shape) => rolled.with_shape(shape),
            None => rolled,
        };
        // Women are beardless.
        if self.sex == Some(Sex::Female) {
            (spec.beard, spec.beard_length) = (fb_body::Beard::None, 0.0);
        }
        spec
    }

    /// The warmth the delver needs from their gear: 6, or 8 for the cold-blooded lizardfolk.
    pub fn warmth_need(&self) -> f32 {
        if self.folk == Folk::Lizardfolk { 8.0 } else { 6.0 }
    }

    /// Their ability scores as they stand: the rolled ones with what they have learned and what
    /// they wear added, within 1 to 30. Every rule that uses a score (to hit, damage, Armor
    /// Class, hit points, saves, the mana pool, capacity) reads these.
    pub fn scores(&self) -> Attributes {
        self.worn.plus(self.learned).on(&self.attributes)
    }

    /// Reads `book`: the score it teaches ([`fb_items::Item::lesson`]) rises by
    /// [`fb_items::lesson::LESSON_POINTS`] for good, unless they have read this book before.
    pub fn read(&mut self, book: &fb_items::Item) -> Reading {
        use fb_items::stats::Stat;
        let Some(stat) = book.lesson() else { return Reading::NothingToLearn };
        if self.books_read.contains(&book.seed) {
            return Reading::AlreadyRead;
        }
        self.books_read.push(book.seed);
        let by = fb_items::lesson::LESSON_POINTS;
        let learned = &mut self.learned;
        let field = match stat {
            Stat::Strength => &mut learned.strength,
            Stat::Dexterity => &mut learned.dexterity,
            Stat::Constitution => &mut learned.constitution,
            Stat::Intelligence => &mut learned.intelligence,
            _ => &mut learned.wisdom,
        };
        *field += by;
        let a = self.scores();
        let score = match stat {
            Stat::Strength => a.strength,
            Stat::Dexterity => a.dexterity,
            Stat::Constitution => a.constitution,
            Stat::Intelligence => a.intelligence,
            _ => a.wisdom,
        };
        Reading::Learned { stat, score }
    }

    pub fn strength(&self) -> i32 {
        self.scores().strength
    }

    pub fn dexterity(&self) -> i32 {
        self.scores().dexterity
    }

    pub fn constitution(&self) -> i32 {
        self.scores().constitution
    }

    /// Mana points their pool holds: from their class's casting ability.
    pub fn mana_pool(&self) -> f64 {
        self.mana_pool_with(fb_character::rules::defaults())
    }

    /// [`Character::mana_pool`] under `rules`.
    pub fn mana_pool_with(&self, rules: &Rules) -> f64 {
        Mana::max_for_with(rules, self.class.casting(), &self.scores())
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
    /// Multiples of the usual hunger and sleepiness rates, from what they wear.
    pub hunger_rate: f32,
    pub sleepiness_rate: f32,
    /// Hit points regained on each kill, from the weapon in the main hand.
    pub heal_on_kill: f32,
    /// Multiple of their pace, from what they wear.
    pub speed: f32,
    /// Chance the shield in their off hand, raised, turns aside a blow that would hit: the
    /// shield's own and what worn gloves and charms add, at most 90%; 0 with no shield.
    pub block_chance: f32,
    /// Points of luck from what they wear: each rolls the loot of a level they come to a level
    /// deeper.
    pub luck: f32,
    /// What worn items add to their ability scores.
    pub abilities: AbilityBonus,
    /// What worn gear adds to a melee or a missile hit roll, the weapon in hand apart.
    pub to_hit: f32,
    /// What worn gear adds to a missile hit roll alone.
    pub ranged_to_hit: f32,
    /// The fraction of damage worn gear takes off each kind of creature's blows, by
    /// [`fb_items::Foe`] index.
    pub ward: [f32; fb_items::Foe::COUNT],
    /// The chance each kind of creature never notices them, by [`fb_items::Foe`] index.
    pub shun: [f32; fb_items::Foe::COUNT],
    /// Multiple of the speed of their swings, draws and reloads, from the weapon in hand and
    /// their gloves.
    pub attack_rate: f32,
    /// Multiple of how fast their stamina refills, from what they wear.
    pub stamina_refill: f32,
    /// Multiple of what swinging, blocking and sprinting cost them in stamina, from what they
    /// wear; below 1 is a saving.
    pub stamina_cost: f32,
}

/// The odds the loot of a level `depth` down rolls against for the delver: the level's, with the
/// luck `character` wears in `inventory` ([`fb_items::Odds::with_luck`]); the level's alone
/// with no character.
pub fn loot_odds(character: Option<&Character>, inventory: Option<&Inventory>, depth: u32) -> fb_items::Odds {
    let luck = character.zip(inventory).map_or(0.0, |(c, i)| c.gear(i).luck);
    fb_items::Odds::loot(depth).with_luck(luck as f32)
}

impl Character {
    /// What they wear and carry means for the simulation: the one place capacity, the meter
    /// rates, darkvision and the carried light come from. Their folk's darkvision counts with
    /// the items' as the best of them, and the carried light is the farthest reaching of the
    /// lit things carried ([`carried_lights`]).
    pub fn gear(&self, inventory: &Inventory) -> Gear {
        self.loadout(inventory).gear()
    }

    /// [`Character::gear`] with each item's name, so the numbers it sums can be explained
    /// ([`Loadout::capacity`] and the rest).
    pub fn loadout(&self, inventory: &Inventory) -> Loadout {
        let mut loadout = Loadout::new(self.folk.race().label.to_string(), self.folk.race().dv as f64);
        let equipped = inventory.equipped();
        // Block chance counts only with a shield held in the off hand: the gloves and charms that
        // add to it do nothing alone.
        let shielded = equipped.iter().any(|(slot, item)| is_shield(*slot, item));
        for (slot, item) in equipped {
            let s = &item.profile.stats;
            // Only the main hand strikes, so only its weapon heals on a kill.
            let heal_on_kill = if slot == fb_items::Slot::Mainhand { s.heal_on_kill as f64 } else { 0.0 };
            let block_chance = if shielded { s.block_chance as f64 } else { 0.0 };
            // A weapon's own to hit is on the roll already ([`crate::combat::Weapon::of_stats`]), so
            // only the worn gear's adds to it; the ranged part counts whatever it is on.
            let to_hit = if slot == fb_items::Slot::Mainhand || s.is_weapon() { 0.0 } else { s.to_hit as f64 };
            // Only the weapon in hand and the gloves quicken a swing; a torch or a cloak does not.
            let attack_rate = if matches!(slot, fb_items::Slot::Mainhand | fb_items::Slot::Hands) { s.attack_rate as f64 } else { 1.0 };
            let point = |v: f32| v.round() as i32;
            let abilities = AbilityBonus { strength: point(s.strength), dexterity: point(s.dexterity), constitution: point(s.constitution), intelligence: point(s.intelligence), wisdom: point(s.wisdom) };
            loadout = loadout.wearing(
                &item.name,
                Worn { carry: s.carry as f64, darkvision: s.darkvision as f64, hunger_rate: s.hunger_rate as f64, sleepiness_rate: s.sleepiness_rate as f64, stealth: s.stealth as f64, warmth: s.warmth as f64, heal_on_kill, speed: s.speed as f64, block_chance, abilities, disease_risk: s.disease_risk as f64, luck: s.luck as f64, recovery: s.recovery as f64, poison_resistance: s.poison_resistance as f64, to_hit, ranged_to_hit: s.ranged_to_hit as f64, attack_rate, stamina_refill: s.stamina_refill as f64, stamina_cost: s.stamina_cost as f64, ward: s.ward.map(f64::from), shun: s.shun.map(f64::from), favored: None, favored_ward: 0.0, favored_bane: None, web_walker: false },
            );
        }
        // A cleric's devotion counts like one more thing worn, so the sheet can explain it and
        // choosing another drops the old one's bonuses.
        if let Some(devotion) = self.devotion.filter(|_| self.class == Class::Cleric) {
            loadout = loadout.wearing(devotion.label(self.body_seed), devotion.worn(&fb_character::rules::defaults().devotion));
        }
        for item in carried_lights(inventory).into_iter().filter_map(|(_, id)| inventory.item(id)) {
            loadout = loadout.carrying(&item.name, CarriedLight { reach: item.profile.stats.light as f64, burn_time: item.profile.stats.burn_time as f64 });
        }
        loadout
    }

    /// Their armour class with its terms: 10, the dexterity modifier and each worn item's armour.
    pub fn armour_class(&self, inventory: &Inventory) -> Explained {
        self.armour_class_with(fb_character::rules::defaults(), inventory)
    }

    /// [`Character::armour_class`] under `rules`.
    pub fn armour_class_with(&self, rules: &Rules, inventory: &Inventory) -> Explained {
        armour_class_with(rules, self.dexterity(), inventory.equipped().iter().map(|(_, item)| (item.name.clone(), item.profile.stats.armour)))
    }

    /// Pounds worn and carried with their terms: a term for each worn item's weight and one for
    /// what a worn container holds, counted through every bag in it.
    pub fn load_explained(&self, inventory: &Inventory) -> Explained {
        let mut e = Explained::new("Load", "lb");
        let add = |e: Explained, label: String, pounds: f64| if e.terms().is_empty() { e.base(label, pounds) } else { e.add(label, pounds) };
        for (slot, item) in inventory.equipped() {
            if item.profile.stats.weight != 0.0 {
                e = add(e, item.name.clone(), item.profile.stats.weight as f64);
            }
            let contents = inventory.worn(slot).map_or(0.0, |id| inventory.contents_weight(id));
            if contents != 0.0 {
                e = add(e, format!("{} contents", item.name), contents as f64);
            }
        }
        if e.terms().is_empty() { e.base("nothing worn", 0.0) } else { e }
    }

    /// Where the load's numbers come from, one line each, for the sheet's tooltip on the Load
    /// row: the strength and what raised it, the capacity, the two band edges, what is carried
    /// ([`Character::load_explained`]) and the pace the load gives now. For STR 11 and 65 lb:
    ///
    /// ```text
    /// Capacity: STR 11 x 15 = 165 lb
    /// Burdened past STR 11 x 5 = 55 lb
    /// Encumbered past STR 11 x 10 = 110 lb
    /// Carried: Mail (25) + ... = 65 lb
    /// Pace now: 99.7%
    /// ```
    ///
    /// The edges are shares of the capacity ([`fb_character::rules::LoadRules`]), so a bag that
    /// adds capacity moves them too, and then they read against the capacity, not the STR.
    pub fn load_breakdown(&self, rules: &Rules, inventory: &Inventory) -> Vec<String> {
        use fb_character::explain::number;
        let mut lines = Vec::new();
        let strength = self.strength();
        // What raised the strength: each worn item, then lessons learned.
        let mut raised: Vec<String> = inventory
            .equipped()
            .iter()
            .filter(|(_, item)| item.profile.stats.strength.round() != 0.0)
            .map(|(_, item)| format!("{} ({:+})", item.name, item.profile.stats.strength.round() as i32))
            .collect();
        if self.learned.strength != 0 {
            raised.push(format!("lessons ({:+})", self.learned.strength));
        }
        if !raised.is_empty() {
            lines.push(format!("STR {strength}: {} base + {}", self.attributes.strength, raised.join(" + ")));
        }
        let loadout = self.loadout(inventory);
        let capacity = loadout.capacity_with(rules, strength).value();
        let per = rules.load.capacity_per_strength;
        // Weights are to one decimal and signed with a plus where they add, throughout.
        let carries: Vec<(&String, f64)> = loadout.worn.iter().filter(|(_, w)| w.carry != 0.0).map(|(label, w)| (label, w.carry)).collect();
        let added: String = carries.iter().map(|(label, carry)| format!(" + {label} ({carry:+.1})")).collect();
        lines.push(format!("Capacity: STR {strength} \u{d7} {}{added} = {} lb", number(per), number(capacity)));
        let moved_by = carries.iter().map(|(label, _)| label.as_str()).collect::<Vec<_>>().join(", ");
        for (word, multiple, edge) in [("Burdened", rules.load.burdened_multiple, rules.load.burdened_edge(capacity)), ("Encumbered", rules.load.encumbered_multiple, rules.load.encumbered_edge(capacity))] {
            let edge = number((edge * 10.0).round() / 10.0);
            lines.push(if carries.is_empty() {
                format!("{word} past STR {strength} \u{d7} {} = {edge} lb", number(multiple))
            } else {
                // The edges are shares of the whole capacity, so what adds to it moves them.
                format!("{word} past capacity {} lb \u{d7} {}/{} = {edge} lb (moved by {moved_by})", number(capacity), number(multiple), number(per))
            });
        }
        let carried = self.load_explained(inventory);
        lines.push(format!("Carried: {} lb", number((carried.value() * 10.0).round() / 10.0)));
        for (slot, item) in inventory.equipped() {
            lines.push(format!("  {}  {:.1} lb", item.name, item.profile.stats.weight));
            let Some(container) = inventory.worn(slot) else { continue };
            for entry in inventory.grid(container).map(|g| g.entries()).unwrap_or_default() {
                let Some(inside) = inventory.item(entry.id) else { continue };
                // What a bag inside the bag holds counts with it.
                lines.push(format!("    {}  {:.1} lb", inside.name, inside.profile.stats.weight * inside.quantity as f32 + inventory.contents_weight(entry.id)));
            }
            // What hangs on a pack or belt, in its small grids, is carried outside the main grid and weighs as it does.
            for (accessory, id) in inventory.hung_on(container) {
                if let Some(item) = inventory.item(id) {
                    lines.push(format!("    {} ({})  {:.1} lb", item.name, fitting_place(accessory), item.profile.stats.weight * item.quantity as f32));
                }
            }
        }
        let band = LoadBand::from_load_with(rules, carried.value(), capacity);
        lines.push(format!("{} now", band.word()));
        let pace = fb_character::Burden::new(carried.value(), capacity).walk_multiplier_with(rules) as f64;
        lines.push(format!("Pace now: {}", fb_character::conditions::pace_percent(pace)));
        lines
    }

    /// What everything equipped adds to the delver, as the plate's `derive` adds it up; the
    /// light is the farthest reaching of the lit things carried ([`carried_lights`]), where the
    /// plate took the brightest of all its items, so an unlit torch in the pack gives none.
    pub fn derive(&self, inventory: &Inventory) -> Derived {
        self.derive_with(fb_character::rules::defaults(), inventory)
    }

    /// [`Character::derive`] under `rules`.
    pub fn derive_with(&self, rules: &Rules, inventory: &Inventory) -> Derived {
        let equipped = inventory.equipped();
        // Folded from +0, since an empty `Sum` of floats is -0 and would read "-0".
        let sum = |f: fn(&fb_items::stats::Stats) -> f32| equipped.iter().map(|(_, item)| f(&item.profile.stats)).fold(0.0, |a, b| a + b);
        let load = self.load_explained(inventory).value() as f32;
        let gear = self.gear(inventory);
        let capacity = gear.capacity_lb_with(rules, self.strength()) as f32;
        let load_band = LoadBand::from_load_with(rules, load as f64, capacity as f64);
        Derived {
            armour_class: self.armour_class_with(rules, inventory).value().round() as i32,
            load,
            capacity,
            load_band,
            warmth: sum(|s| s.warmth),
            light: gear.light as f32,
            darkvision: gear.darkvision as f32,
            hunger_rate: gear.hunger_rate as f32,
            sleepiness_rate: gear.sleepiness_rate as f32,
            heal_on_kill: gear.heal_on_kill as f32,
            speed: gear.speed as f32,
            block_chance: gear.block_chance as f32,
            luck: gear.luck as f32,
            abilities: gear.abilities,
            to_hit: gear.to_hit as f32,
            ranged_to_hit: gear.ranged_to_hit as f32,
            ward: gear.ward.map(|w| w as f32),
            shun: gear.shun.map(|w| w as f32),
            attack_rate: gear.attack_rate as f32,
            stamina_refill: gear.stamina_refill as f32,
            stamina_cost: gear.stamina_cost as f32,
        }
    }
}

/// Whether `item` in `slot` is a shield held: armour in the off hand (a torch or a lantern there
/// is not armour).
pub fn is_shield(slot: fb_items::Slot, item: &fb_items::Item) -> bool {
    slot == fb_items::Slot::Offhand && item.profile.tags.contains(&fb_items::Tag::Armor)
}

/// Keeps [`Character::worn`] to what the worn items add to the ability scores, changing the
/// character only when it differs, so every rule that reads a score sees it.
pub fn wear_abilities(inventory: Option<Res<Inventory>>, character: Option<ResMut<Character>>) {
    let (Some(inventory), Some(mut character)) = (inventory, character) else { return };
    if !inventory.is_changed() && !character.is_added() {
        return;
    }
    let worn = character.gear(&inventory).abilities;
    if character.worn != worn {
        character.worn = worn;
    }
}

/// The delver the next delve begins as, made in the creator or continued from the last delve
/// ([`crate::title`]); the delve takes it up as it begins. Without one, a delve rolls its delver
/// from the level ([`character_for`]).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct ChosenDelver(pub Character);

/// Rolls the delver from `seed`, or as one of the folk `FB_FOLK` names.
pub fn character_for(seed: u32) -> Character {
    let mut character = match std::env::var("FB_FOLK").ok().and_then(|f| Folk::parse(&f)) {
        Some(folk) => Character::roll_folk(folk, seed),
        None => Character::roll(seed),
    };
    // `FB_CLASS=ranger` picks the class, for trying a class's kit with `FB_DEBUG_KIT`.
    if let Some(class) = std::env::var("FB_CLASS").ok().and_then(|c| Class::ALL.into_iter().find(|k| k.outfit_key().eq_ignore_ascii_case(c.trim()))) {
        character.class = class;
    }
    // `FB_DEVOTION=foe:goblin` or `aligned:spider` devotes a cleric, for captures.
    if let Ok(spec) = std::env::var("FB_DEVOTION") {
        let kin = |name: &str| fb_character::kin::Kin::all().into_iter().find(|k| k.key().eq_ignore_ascii_case(name));
        character.devotion = spec.trim().split_once(':').and_then(|(how, name)| match how {
            "foe" => kin(name).map(fb_character::devotion::Devotion::FavoredFoe),
            "aligned" => kin(name).map(fb_character::devotion::Devotion::Aligned),
            _ => None,
        });
    }
    character
}

/// What the delver carries against what they can carry, kept on the [`Player`](crate::play::Player)
/// so walking need not work out [`Character::derive`] every frame. Their band, walking pace and
/// sprint drain are worked out from it under the rules ([`fb_character::Burden`]).
/// `player::weigh_the_delver` refreshes it when the character or the inventory changes.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Load(#[reflect(remote = crate::inspect::remote::BurdenDef)] pub fb_character::Burden);

impl Load {
    /// Carrying `load_lb` of a capacity of `capacity_lb`.
    pub fn of(load_lb: f32, capacity_lb: f32) -> Load {
        Load(fb_character::Burden::new(load_lb as f64, capacity_lb as f64))
    }

    /// A load in `band`, for captures and tests: a typical weight for a capacity of 165 lb.
    pub fn sample(band: LoadBand) -> Load {
        Load::of(
            match band {
                LoadBand::Light => 0.0,
                LoadBand::Burdened => 82.5,
                LoadBand::Encumbered => 137.5,
                LoadBand::Overloaded => 200.0,
            },
            165.0,
        )
    }
}

/// What the delver's gear makes of their stamina: how fast it refills and what sprinting,
/// swinging and blocking cost, as multiples of the usual. Kept up with [`Load`] by
/// `player::weigh_the_delver`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component)]
pub struct GearStamina {
    pub refill: f32,
    pub cost: f32,
}

impl Default for GearStamina {
    fn default() -> Self {
        GearStamina { refill: 1.0, cost: 1.0 }
    }
}

/// The delver's hunger, thirst and sleepiness, on the [`Player`](crate::play::Player) from their
/// spawn, so a new delve starts Fed. Eating and drinking take from it (`crate::eat`); the game
/// clock steps it in #229.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
pub struct DelverNeeds(#[reflect(remote = crate::inspect::remote::NeedsDef)] pub fb_character::Needs);

/// The delver's stamina, on the [`Player`](crate::play::Player) from their spawn, so a new delve
/// starts with a full pool. Sprinting and swinging drain it, and it refills in seconds
/// (`crate::simulate::step_stamina`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
pub struct DelverStamina(#[reflect(remote = crate::inspect::remote::StaminaDef)] pub fb_character::Stamina);

/// The delver's mana, on the [`Player`](crate::play::Player) from their spawn. The pool's size
/// is set from the character when the delver is new and again whenever the character changes,
/// keeping how full it is; the game clock refills it (`crate::simulate::step_the_delver`).
/// Nothing spends it yet.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
pub struct DelverMana(#[reflect(remote = crate::inspect::remote::ManaDef)] pub Mana);

/// The effects on the delver ([`fb_character::effect`]), on the [`Player`](crate::play::Player)
/// only while they have any: a failed save against a snake's venom poisons them
/// (`crate::combat::vitals`), the game clock steps them, and the component comes off when the
/// last one ends (`crate::simulate::step_the_delver`). What they do is read with the loaded
/// rules' definitions (`GameRules::effects`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct DelverEffects(pub fb_character::effect::Effects);

impl DelverEffects {
    /// How the delver's attack rolls are thrown under `rules`, with or without effects on them.
    pub fn attack_odds(effects: Option<&DelverEffects>, rules: &fb_character::rules::Rules) -> fb_character::combat::Odds {
        effects.map_or(fb_character::combat::Odds::Even, |e| e.0.attack_odds(&rules.effects))
    }

    /// Poisoned for `hours`, as venom poisons (for the status debug override and tests).
    pub fn poisoned_for(rules: &fb_character::rules::Rules, hours: f64) -> DelverEffects {
        let mut effects = fb_character::effect::Effects::default();
        effects.add(&rules.effects, "poisoned", 1.0, None, fb_character::effect::Source::Bite { by: "venom".to_string() });
        if let Some(poisoned) = effects.active.first_mut() {
            poisoned.hours_left = Some(hours);
        }
        DelverEffects(effects)
    }

    /// Game hours the poison has left, while it works.
    pub fn poisoned_hours(&self) -> Option<f64> {
        self.0.get("poisoned", None).and_then(|poisoned| poisoned.hours_left)
    }
}

/// Game time the delver has put toward their next hit point while fed and watered, on the
/// [`Player`](crate::play::Player) from their spawn; the game clock steps it
/// (`crate::simulate::step_the_delver`, [`fb_character::recovery`]).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct DelverRecovery(pub fb_character::recovery::Recovery);

/// How easy the delver is to see, 0 to 1 ([`fb_character::visibility`]): from the light on them,
/// their pace, load and Stealth. Kept on the [`Player`](crate::play::Player) so the eye can show
/// it and, later, creatures read it (`crate::combat::eye`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
pub struct DelverVisibility(pub f32);

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

    /// A STR 11 delver wearing mail of `weight` lb (no carry of its own), and a ring of +2 STR
    /// besides when `ring`.
    fn laden_str_11(weight: f32, ring: bool) -> (Character, Inventory) {
        let attributes = fb_character::Attributes { strength: 11, dexterity: 10, constitution: 10, intelligence: 10, wisdom: 10 };
        let delver = Character { attributes, ..Character::roll(4) };
        let mut inventory = Inventory::default();
        let mut mail = fb_items::roll_item("Mail", 1).unwrap();
        mail.profile.stats.weight = weight;
        mail.profile.stats.carry = 0.0;
        mail.profile.stats.strength = 0.0;
        let mail = inventory.add(mail);
        inventory.wear(mail, fb_items::Slot::Body).unwrap();
        if ring {
            let mut ring = fb_items::roll_item("Ring", 1).unwrap();
            ring.profile.stats.weight = 0.0;
            ring.profile.stats.carry = 0.0;
            ring.profile.stats.strength = 2.0;
            let ring = inventory.add(ring);
            inventory.wear(ring, fb_items::Slot::Ring1).unwrap();
        }
        (delver, inventory)
    }

    #[test]
    fn the_load_breakdown_lists_the_capacity_the_edges_what_is_carried_and_the_pace() {
        let (delver, inventory) = laden_str_11(65.0, false);
        let lines = delver.load_breakdown(fb_character::rules::defaults(), &inventory);
        let mail = &inventory.equipped()[0].1.name;
        assert_eq!(
            lines,
            vec![
                "Capacity: STR 11 \u{d7} 15 = 165 lb".to_string(),
                "Burdened past STR 11 \u{d7} 5 = 55 lb".to_string(),
                "Encumbered past STR 11 \u{d7} 10 = 110 lb".to_string(),
                "Carried: 65 lb".to_string(),
                format!("  {mail}  65.0 lb"),
                "Burdened now".to_string(),
                "Pace now: 99.7%".to_string(),
            ]
        );
    }

    #[test]
    fn a_ring_of_strength_moves_every_edge_and_the_breakdown_names_it() {
        let (mut delver, inventory) = laden_str_11(65.0, true);
        // The game keeps what is worn on the character as `sync_worn` does (`wear_abilities`).
        delver.worn = delver.gear(&inventory).abilities;
        let lines = delver.load_breakdown(fb_character::rules::defaults(), &inventory);
        let ring = inventory.equipped().iter().find(|(slot, _)| *slot == fb_items::Slot::Ring1).unwrap().1.name.clone();
        assert_eq!(lines[0], format!("STR 13: 11 base + {ring} (+2)"));
        assert_eq!(&lines[1..4], ["Capacity: STR 13 \u{d7} 15 = 195 lb", "Burdened past STR 13 \u{d7} 5 = 65 lb", "Encumbered past STR 13 \u{d7} 10 = 130 lb"]);
        assert_eq!(&lines[lines.len() - 2..], ["Light now", "Pace now: 100%"], "65 lb is the new Burdened edge, so full pace");
        // The game's own derivation agrees with the breakdown.
        let derived = delver.derive(&inventory);
        assert_eq!((derived.capacity, derived.load_band), (195.0, fb_character::LoadBand::Light));
    }

    #[test]
    fn the_breakdown_marks_the_band_the_load_is_in() {
        let now = |lb: f32| {
            let (delver, inventory) = laden_str_11(lb, false);
            let lines = delver.load_breakdown(fb_character::rules::defaults(), &inventory);
            lines[lines.len() - 2].clone()
        };
        assert_eq!([40.0, 55.0, 65.0, 110.0, 137.5, 165.0, 172.0].map(now), ["Light now", "Light now", "Burdened now", "Burdened now", "Encumbered now", "Encumbered now", "Overloaded now"]);
    }

    #[test]
    fn a_pack_lists_what_it_holds_under_it_and_a_belt_says_it_moved_the_edges() {
        let (delver, mut inventory) = laden_str_11(10.0, false);
        let mut pack = fb_items::roll_item("Pack", 3).unwrap();
        pack.profile.stats.weight = 2.0;
        pack.profile.stats.carry = 0.0;
        let pack_name = pack.name.clone();
        let pack = inventory.add(pack);
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let mut dagger = fb_items::roll_item("Dagger", 1).unwrap();
        dagger.profile.stats.weight = 3.25;
        let dagger_name = dagger.name.clone();
        let dagger = inventory.add(dagger);
        inventory.put(dagger, pack, crate::inventory::Placement::new(0, 0, false)).unwrap();
        let lines = delver.load_breakdown(fb_character::rules::defaults(), &inventory);
        assert!(lines.contains(&"Carried: 15.3 lb".to_string()), "{lines:?}");
        let at = lines.iter().position(|l| l == &format!("  {pack_name}  2.0 lb")).unwrap_or_else(|| panic!("{lines:?}"));
        assert_eq!(lines[at + 1], format!("    {dagger_name}  3.2 lb"), "contents sit under their bag, one step in");
        // A belt that adds capacity moves both edges and the lines say so.
        let mut belt = fb_items::roll_item("Belt", 2).unwrap();
        belt.profile.stats.weight = 0.0;
        belt.profile.stats.carry = 30.0;
        let belt_name = belt.name.clone();
        let belt = inventory.add(belt);
        inventory.wear(belt, fb_items::Slot::Belt).unwrap();
        let lines = delver.load_breakdown(fb_character::rules::defaults(), &inventory);
        assert_eq!(lines[0], format!("Capacity: STR 11 \u{d7} 15 + {belt_name} (+30.0) = 195 lb"));
        assert_eq!(lines[1], format!("Burdened past capacity 195 lb \u{d7} 5/15 = 65 lb (moved by {belt_name})"));
        assert_eq!(lines[2], format!("Encumbered past capacity 195 lb \u{d7} 10/15 = 130 lb (moved by {belt_name})"));
    }

    #[test]
    fn the_load_breakdown_pace_follows_the_load() {
        let (delver, inventory) = laden_str_11(137.5, false);
        let lines = delver.load_breakdown(fb_character::rules::defaults(), &inventory);
        assert_eq!(lines.last().unwrap(), "Pace now: 80%");
        let (delver, inventory) = laden_str_11(172.0, false);
        assert_eq!(delver.load_breakdown(fb_character::rules::defaults(), &inventory).last().unwrap(), "Pace now: 0%");
    }

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
        with_affix_to(fb_items::roll_item(base, seed).unwrap(), name)
    }

    fn with_affix_to(mut item: fb_items::Item, name: &str) -> fb_items::Item {
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
        let belt = inventory.add(with_affix("Belt", 4, "of Plenty"));
        inventory.wear(belt, Slot::Belt).unwrap();
        let ring = inventory.add(with_affix("Ring", 4, "of Wakefulness"));
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
    fn a_leech_weapon_heals_on_a_kill_only_in_the_main_hand() {
        use fb_items::Slot;
        let delver = Character::roll(4);
        let mut inventory = Inventory::default();
        assert_eq!(delver.derive(&inventory).heal_on_kill, 0.0);
        let sword = inventory.add(with_affix("Sword", 5, "of the Leech"));
        inventory.wear(sword, Slot::Offhand).unwrap();
        assert_eq!(delver.derive(&inventory).heal_on_kill, 0.0, "the off hand does not strike");
        inventory.take_out(sword);
        inventory.wear(sword, Slot::Mainhand).unwrap();
        assert_eq!(delver.derive(&inventory).heal_on_kill, 1.0);
        assert_eq!(delver.loadout(&inventory).heal_on_kill().value(), 1.0);
    }

    #[test]
    fn a_ring_of_the_giant_raises_strength_and_its_modifier() {
        use fb_items::Slot;
        let attributes = Attributes { strength: 13, dexterity: 10, constitution: 10, intelligence: 10, wisdom: 10 };
        let mut delver = Character { attributes, ..Character::roll(4) };
        let mut inventory = Inventory::default();
        let ring = inventory.add(with_affix("Ring", 4, "of the Giant"));
        inventory.wear(ring, Slot::Ring2).unwrap();
        delver.worn = delver.gear(&inventory).abilities;
        assert_eq!((delver.attributes.strength, delver.strength(), modifier(delver.strength())), (13, 15, 2), "+1 to +2");
        assert_eq!(delver.derive(&inventory).capacity, 15.0 * 15.0, "capacity follows");
        let frail = inventory.add(with_affix("Amulet", 4, "Frail"));
        inventory.wear(frail, Slot::Neck).unwrap();
        delver.worn = delver.gear(&inventory).abilities;
        assert_eq!(delver.constitution(), 9);
    }

    #[test]
    fn worn_luck_rolls_the_loot_deeper_but_never_legendary_on_the_first_level() {
        use fb_items::{Odds, Rarity, Slot};
        let delver = Character::roll(4);
        let mut inventory = Inventory::default();
        assert_eq!(loot_odds(Some(&delver), Some(&inventory), 3), Odds::loot(3));
        let ring = inventory.add(with_affix("Ring", 4, "of Fortune"));
        inventory.wear(ring, Slot::Ring1).unwrap();
        let charm = inventory.add(with_affix("Charm", 4, "of Chance"));
        inventory.wear(charm, Slot::Charm).unwrap();
        assert_eq!(delver.derive(&inventory).luck, 3.0);
        assert_eq!(loot_odds(Some(&delver), Some(&inventory), 3), Odds::loot(6));
        assert_eq!(loot_odds(Some(&delver), Some(&inventory), 1).cap, Rarity::Rare);
        assert_eq!(loot_odds(None, Some(&inventory), 3), Odds::loot(3), "no character, no luck");
    }

    #[test]
    fn only_a_held_shield_blocks_and_boots_of_haste_quicken() {
        use fb_items::Slot;
        let delver = Character::roll(4);
        let mut inventory = Inventory::default();
        // The shield's own chance, by its size, with the affix's 20% on top.
        let rampart = with_affix("Shield", 4, "of the Rampart");
        let chance = rampart.profile.stats.block_chance;
        assert!(chance > 0.2, "{chance}");
        let shield = inventory.add(rampart);
        inventory.wear(shield, Slot::Offhand).unwrap();
        assert!((delver.derive(&inventory).block_chance - chance).abs() < 1e-6);
        let boots = inventory.add(with_affix("Boots", 4, "of Haste"));
        inventory.wear(boots, Slot::Feet).unwrap();
        assert!((delver.derive(&inventory).speed - 1.2).abs() < 1e-6);
        inventory.take_out(shield);
        assert_eq!(delver.derive(&inventory).block_chance, 0.0);
    }

    #[test]
    fn only_the_weapon_and_the_gloves_quicken_an_attack_and_the_rates_multiply() {
        use fb_items::Slot;
        let delver = Character::roll(4);
        let mut inventory = Inventory::default();
        assert_eq!(delver.derive(&inventory).attack_rate, 1.0);
        let sword = inventory.add(with_affix("Sword", 4, "Quick"));
        inventory.wear(sword, Slot::Mainhand).unwrap();
        assert!((delver.derive(&inventory).attack_rate - 1.1).abs() < 1e-6);
        let gloves = inventory.add(with_affix("Gloves", 4, "of the Duellist"));
        inventory.wear(gloves, Slot::Hands).unwrap();
        let derived = delver.derive(&inventory);
        assert!((derived.attack_rate - 1.155).abs() < 1e-6, "{}", derived.attack_rate);
        assert!(delver.loadout(&inventory).attack_rate().line().contains("× "));
        // A torch in the off hand or a ring does nothing for it, whatever it carries.
        let torch = inventory.add(with_affix("Torch", 4, "of Swiftness"));
        inventory.wear(torch, Slot::Offhand).unwrap();
        let ring = inventory.add(with_affix("Ring", 4, "Quick"));
        inventory.wear(ring, Slot::Ring1).unwrap();
        assert!((delver.derive(&inventory).attack_rate - 1.155).abs() < 1e-6);
    }

    #[test]
    fn stamina_refill_and_cost_compose_multiplicatively_across_items() {
        use fb_items::Slot;
        let delver = Character::roll(4);
        let mut inventory = Inventory::default();
        let plain = delver.derive(&inventory);
        assert_eq!((plain.stamina_refill, plain.stamina_cost), (1.0, 1.0));
        for (base, slot, affix) in [("Boots", Slot::Feet, "Tireless"), ("Charm", Slot::Charm, "of Second Wind"), ("Sword", Slot::Mainhand, "Light-handed"), ("Belt", Slot::Belt, "of Endurance")] {
            let id = inventory.add(with_affix(base, 4, affix));
            inventory.wear(id, slot).unwrap();
        }
        let derived = delver.derive(&inventory);
        assert!((derived.stamina_refill - 1.2 * 1.3).abs() < 1e-6, "{}", derived.stamina_refill);
        assert!((derived.stamina_cost - 0.75 * 0.8).abs() < 1e-6, "{}", derived.stamina_cost);
        assert!(delver.loadout(&inventory).stamina_cost().line().contains("× "));
    }

    #[test]
    fn gloves_and_charms_add_to_a_held_shields_block_up_to_90_percent() {
        use fb_items::Slot;
        let delver = Character::roll(4);
        let of_chance = |chance: f32| (1..).map(|s| fb_items::roll_item("Shield", s).unwrap()).find(|i| i.profile.stats.block_chance == chance).unwrap();
        let mut inventory = Inventory::default();
        let gloves = inventory.add(with_affix("Gloves", 4, "of Deflection"));
        inventory.wear(gloves, Slot::Hands).unwrap();
        assert_eq!(delver.derive(&inventory).block_chance, 0.0, "gloves alone block nothing");
        assert_eq!(delver.loadout(&inventory).block_chance().value(), 0.0);
        let buckler = inventory.add(of_chance(0.3));
        inventory.wear(buckler, Slot::Offhand).unwrap();
        assert!((delver.derive(&inventory).block_chance - 0.4).abs() < 1e-6, "a buckler's 30% and 10% more");
        assert!(delver.loadout(&inventory).block_chance().line().contains("(+0.1)"));
        inventory.take_out(buckler);
        assert_eq!(delver.derive(&inventory).block_chance, 0.0, "the shield put away, the gloves block nothing again");
        let tower = inventory.add(with_affix_to(of_chance(0.6), "of the Rampart"));
        inventory.wear(tower, Slot::Offhand).unwrap();
        let charm = inventory.add(with_affix("Charm", 4, "of the Rampart"));
        inventory.wear(charm, Slot::Charm).unwrap();
        let rampart = inventory.add(with_affix("Gauntlets", 4, "of the Rampart"));
        inventory.take_out(gloves);
        inventory.wear(rampart, Slot::Hands).unwrap();
        assert!((delver.derive(&inventory).block_chance - 0.9).abs() < 1e-6, "60% and three 20%s stop at 90%");
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
    fn a_frogged_weapon_and_a_pouch_potion_weigh_on_the_load() {
        use fb_items::{Accessory, Slot};
        let delver = Character::roll(4);
        let mut inventory = Inventory::default();
        let belt = (1..3000).filter_map(|s| fb_items::roll_item("Belt", s)).find(|b| b.has_accessory(Accessory::LeftFrog) && b.has_accessory(Accessory::PotionPouch)).unwrap();
        let belt = inventory.add(belt);
        inventory.wear(belt, Slot::Belt).unwrap();
        let before = delver.derive(&inventory);
        let (mace, vial) = (inventory.add(fb_items::roll_item("Mace", 3).unwrap()), inventory.add(fb_items::roll_item("Vial", 3).unwrap()));
        let (frog, pouch) = (inventory.fitting(belt, Accessory::LeftFrog).unwrap(), inventory.fitting(belt, Accessory::PotionPouch).unwrap());
        inventory.put(mace, frog, crate::inventory::Placement::new(0, 0, false)).unwrap();
        inventory.put(vial, pouch, crate::inventory::Placement::new(0, 0, false)).unwrap();
        let after = delver.derive(&inventory);
        let weight = inventory.item(mace).unwrap().profile.stats.weight + inventory.item(vial).unwrap().profile.stats.weight;
        assert!(weight > 0.0 && (after.load - before.load - weight).abs() < 1e-3, "load {} to {} for {weight}", before.load, after.load);
        let lines = delver.load_breakdown(fb_character::rules::defaults(), &inventory);
        assert!(lines.iter().any(|l| l.contains("in a frog")) && lines.iter().any(|l| l.contains("potion pouch")), "{lines:?}");
        assert_eq!(after.armour_class, before.armour_class);
    }

    #[test]
    fn a_strapped_weapon_weighs_on_the_load_but_gives_nothing_else() {
        use fb_items::Slot;
        let delver = Character::roll(4);
        let mut inventory = Inventory::default();
        let pack = (1..2000).filter_map(|s| fb_items::roll_item("Pack", s)).find(|p| p.has_accessory(fb_items::Accessory::WeaponStrap)).unwrap();
        let pack = inventory.add(pack);
        inventory.wear(pack, Slot::Back).unwrap();
        let before = delver.derive(&inventory);
        let spear = inventory.add(fb_items::roll_item("Spear", 3).unwrap());
        let weight = inventory.item(spear).unwrap().profile.stats.weight;
        let strap = inventory.fitting(pack, fb_items::Accessory::WeaponStrap).unwrap();
        inventory.put(spear, strap, crate::inventory::Placement::new(0, 0, false)).unwrap();
        let after = delver.derive(&inventory);
        assert!(weight > 0.0 && (after.load - before.load - weight).abs() < 1e-3, "{} against {} plus {weight}", after.load, before.load);
        assert_eq!((after.armour_class, after.warmth, after.to_hit), (before.armour_class, before.warmth, before.to_hit));
        assert!(delver.load_breakdown(fb_character::rules::defaults(), &inventory).iter().any(|l| l.contains("on the strap")));
    }

    #[test]
    fn a_lashed_bundle_weighs_on_the_load_but_gives_nothing_else() {
        use fb_items::Slot;
        let delver = Character::roll(4);
        let mut inventory = Inventory::default();
        let pack = (1..2000).filter_map(|s| fb_items::roll_item("Pack", s)).find(|p| p.has_accessory(fb_items::Accessory::Lashing)).unwrap();
        let pack = inventory.add(pack);
        inventory.wear(pack, Slot::Back).unwrap();
        let before = delver.derive(&inventory);
        let tent = inventory.add(fb_items::roll_item("Tent", 3).unwrap());
        let weight = inventory.item(tent).unwrap().profile.stats.weight;
        inventory.lash(tent).unwrap();
        let after = delver.derive(&inventory);
        assert!(weight > 0.0 && (after.load - before.load - weight).abs() < 1e-3, "{} against {} plus {weight}", after.load, before.load);
        assert_eq!((after.armour_class, after.warmth, after.to_hit), (before.armour_class, before.warmth, before.to_hit));
        assert!(delver.load_breakdown(fb_character::rules::defaults(), &inventory).iter().any(|l| l.contains("on the lashing")));
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
