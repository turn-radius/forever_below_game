//! The numbers an item gives the character who carries or wears it, and the names affixes
//! use to change them.
//!
//! Stats split into two categories:
//! - Item properties: Weight, Armour, DamageBonus, ToHit, Light, BurnTime, BreakChance.
//!   These describe the item itself.
//! - Wearer stats: Warmth, Stealth, Carry, Darkvision, HungerRate, SleepinessRate, DiseaseRisk,
//!   Lockpicking, HealOnKill, Speed, BlockChance, Luck, the six ability scores, Recovery and
//!   PoisonResistance. These affect the
//!   character wearing the item.
//!
//! This is the minimal set the affixes in `data/afx.json` act on: the plate's item fields
//! (`wt`, `ac`, `dmg`, `dmgB`, `warmth`, `light`, `carry`) plus one stat for each affix the
//! plate only described in words (stealth, to hit, darkvision...). Deriving the real values
//! per family (the plate's `saMeta`) is a later task.

use crate::dice::Dice;
use crate::foe::Foe;
use serde::{Deserialize, Serialize};

/// An item's derived stats.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    /// Pounds.
    pub weight: f32,
    pub armour: f32,
    /// The weapon's damage dice; `None` for anything that is not a weapon.
    pub damage: Option<Dice>,
    /// Added to every damage roll (the plate's `dmgB`).
    pub damage_bonus: f32,
    pub to_hit: f32,
    /// Added to the shot roll only, from any gear including the bow itself; a sword swing never
    /// reads it.
    #[serde(default)]
    pub ranged_to_hit: f32,
    pub warmth: f32,
    /// How far its light reaches, in feet.
    pub light: f32,
    /// How long a light burns, as a multiple of its base's.
    pub burn_time: f32,
    pub stealth: f32,
    /// Extra pounds the wearer can carry.
    pub carry: f32,
    /// How far the wearer sees without light, in feet.
    pub darkvision: f32,
    /// How fast the wearer gets hungry, as a multiple of normal.
    pub hunger_rate: f32,
    /// How fast the wearer tires (sleepiness), as a multiple of normal.
    pub sleepiness_rate: f32,
    /// How likely the wearer is to catch a disease, as a multiple of normal.
    pub disease_risk: f32,
    pub lockpicking: f32,
    /// Health regained on a kill.
    pub heal_on_kill: f32,
    /// Chance of breaking on a hard blow, 0..=1.
    pub break_chance: f32,
    /// How fast the wearer moves, as a multiple of their pace.
    #[serde(default = "one")]
    pub speed: f32,
    /// Chance a shield turns aside a blow that would hit, 0..=1, while it is held.
    #[serde(default)]
    pub block_chance: f32,
    /// Points of luck: each rolls the loot the wearer finds a level deeper
    /// ([`crate::Odds::with_luck`]).
    #[serde(default)]
    pub luck: f32,
    /// Points added to the wearer's ability scores.
    #[serde(default)]
    pub strength: f32,
    #[serde(default)]
    pub dexterity: f32,
    #[serde(default)]
    pub constitution: f32,
    #[serde(default)]
    pub intelligence: f32,
    #[serde(default)]
    pub wisdom: f32,
    #[serde(default)]
    pub charisma: f32,
    /// Points of recovery: each mends the wearer's hit points a little faster while they recover
    /// (`fb_character::recovery::minutes_per_hit_point`).
    #[serde(default)]
    pub recovery: f32,
    /// Points of poison resistance: each adds 1 to the wearer's saves against venom.
    #[serde(default)]
    pub poison_resistance: f32,
    /// The fraction of damage taken off the blows of each kind of creature, by [`Foe`] index.
    #[serde(default)]
    pub ward: [f32; Foe::COUNT],
    /// The chance a creature of each kind never notices the wearer, by [`Foe`] index.
    #[serde(default)]
    pub shun: [f32; Foe::COUNT],
    /// The extra damage dice a weapon rolls against each kind of creature, by [`Foe`] index.
    #[serde(default)]
    pub bane: [Option<Dice>; Foe::COUNT],
    /// How fast the wearer swings, draws and reloads, as a multiple of normal: the weapon in
    /// hand's and the gloves' (`fb_character::Gear::attack_rate`).
    #[serde(default = "one")]
    pub attack_rate: f32,
    /// How fast the wearer's stamina refills, as a multiple of normal.
    #[serde(default = "one")]
    pub stamina_refill: f32,
    /// How fast the wearer's mana refills, as a multiple of normal.
    #[serde(default = "one")]
    pub mana_refill: f32,
    /// What swinging, blocking and sprinting cost the wearer in stamina, as a multiple of normal.
    #[serde(default = "one")]
    pub stamina_cost: f32,
    /// Rows a worn bag's grid gains (or loses, when negative) against a plain bag of its size
    /// ([`crate::profile::Profile::fit_capacity`], which rounds it to whole rows).
    #[serde(default)]
    pub grid_rows: f32,
}

fn one() -> f32 {
    1.0
}

impl Default for Stats {
    /// Weightless and inert: no bonuses, every rate normal.
    fn default() -> Self {
        Stats {
            weight: 0.0,
            armour: 0.0,
            damage: None,
            damage_bonus: 0.0,
            to_hit: 0.0,
            ranged_to_hit: 0.0,
            warmth: 0.0,
            light: 0.0,
            burn_time: 1.0,
            stealth: 0.0,
            carry: 0.0,
            darkvision: 0.0,
            hunger_rate: 1.0,
            sleepiness_rate: 1.0,
            disease_risk: 1.0,
            lockpicking: 0.0,
            heal_on_kill: 0.0,
            break_chance: 0.0,
            speed: 1.0,
            block_chance: 0.0,
            luck: 0.0,
            strength: 0.0,
            dexterity: 0.0,
            constitution: 0.0,
            intelligence: 0.0,
            wisdom: 0.0,
            charisma: 0.0,
            recovery: 0.0,
            poison_resistance: 0.0,
            ward: [0.0; Foe::COUNT],
            shun: [0.0; Foe::COUNT],
            bane: [None; Foe::COUNT],
            attack_rate: 1.0,
            stamina_refill: 1.0,
            mana_refill: 1.0,
            stamina_cost: 1.0,
            grid_rows: 0.0,
        }
    }
}

/// A numeric field of [`Stats`], as affixes name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stat {
    // Item properties: describe the item itself
    Weight,
    Armour,
    DamageBonus,
    ToHit,
    RangedToHit,
    Light,
    BurnTime,
    BreakChance,
    // Wearer stats: affect the character wearing the item
    Warmth,
    Stealth,
    Carry,
    Darkvision,
    HungerRate,
    SleepinessRate,
    DiseaseRisk,
    Lockpicking,
    HealOnKill,
    Speed,
    BlockChance,
    Luck,
    Strength,
    Dexterity,
    Constitution,
    Intelligence,
    Wisdom,
    Charisma,
    Recovery,
    PoisonResistance,
    AttackRate,
    StaminaRefill,
    ManaRefill,
    StaminaCost,
    GridRows,
}

impl Stat {
    pub const ALL: [Stat; 33] = [
        Stat::Weight, Stat::Armour, Stat::DamageBonus, Stat::ToHit, Stat::RangedToHit, Stat::Light, Stat::BurnTime, Stat::BreakChance,
        Stat::Warmth, Stat::Stealth, Stat::Carry, Stat::Darkvision, Stat::HungerRate, Stat::SleepinessRate, Stat::DiseaseRisk,
        Stat::Lockpicking, Stat::HealOnKill, Stat::Speed, Stat::BlockChance, Stat::Luck, Stat::Strength, Stat::Dexterity, Stat::Constitution,
        Stat::Intelligence, Stat::Wisdom, Stat::Charisma, Stat::Recovery, Stat::PoisonResistance, Stat::AttackRate,
        Stat::StaminaRefill, Stat::ManaRefill, Stat::StaminaCost, Stat::GridRows,
    ];

    /// Item properties: describe the item itself (Weight, Armour, DamageBonus, ToHit, Light, BurnTime, BreakChance).
    pub fn is_item_property(self) -> bool {
        matches!(self, Stat::Weight | Stat::Armour | Stat::DamageBonus | Stat::ToHit | Stat::RangedToHit | Stat::Light | Stat::BurnTime | Stat::BreakChance | Stat::GridRows)
    }

    /// Wearer stats: affect the character wearing the item (Warmth, Stealth, Carry, Darkvision, HungerRate, SleepinessRate, DiseaseRisk, Lockpicking, HealOnKill).
    pub fn is_wearer_stat(self) -> bool {
        !self.is_item_property()
    }

    /// Whether more of it is better for the wearer.
    pub fn higher_is_better(self) -> bool {
        !matches!(self, Stat::Weight | Stat::HungerRate | Stat::SleepinessRate | Stat::DiseaseRisk | Stat::BreakChance | Stat::StaminaCost)
    }

    /// Whether anything in the game reads it yet. Affixes whose every effect is on a stat
    /// nobody reads are held back from rolling ([`crate::tables::Affix::in_play`]) until the
    /// system that reads it exists: then it comes off this list, and its affixes roll again.
    /// Stealth only moves the HUD's eye so far, since no creature looks at how visible the
    /// delver is, so it waits for sneaking and break chance waits for breaking. Lockpicking is
    /// read since a door's pick rolls it as a line of its own (`game/src/doors.rs`). Disease risk is read since diseases arrived (sewer plague from rat bites).
    /// Charisma waits for a Charisma score, which the delver does not have yet.
    pub fn in_play(self) -> bool {
        !matches!(self, Stat::Stealth | Stat::BreakChance | Stat::Charisma)
    }

    /// The range it is kept in. Bonuses can go negative (the plate's `dmgB -= 1`); amounts,
    /// distances and rates cannot (its `Math.max(0, it.warmth - 1)`).
    pub fn range(self) -> (f32, f32) {
        match self {
            Stat::DamageBonus | Stat::ToHit | Stat::RangedToHit | Stat::Stealth | Stat::Lockpicking | Stat::GridRows => (f32::NEG_INFINITY, f32::INFINITY),
            Stat::Strength | Stat::Dexterity | Stat::Constitution | Stat::Intelligence | Stat::Wisdom | Stat::Charisma => (f32::NEG_INFINITY, f32::INFINITY),
            Stat::BreakChance | Stat::BlockChance => (0.0, 1.0),
            _ => (0.0, f32::INFINITY),
        }
    }
}

impl Stats {
    pub fn get(&self, stat: Stat) -> f32 {
        *self.field(stat)
    }

    /// Sets `stat`, kept in its [`Stat::range`].
    pub fn set(&mut self, stat: Stat, value: f32) {
        let (lo, hi) = stat.range();
        *self.field_mut(stat) = value.clamp(lo, hi);
    }

    /// Whether this is a weapon, which the plate tells by its damage dice.
    pub fn is_weapon(&self) -> bool {
        self.damage.is_some()
    }

    fn field(&self, stat: Stat) -> &f32 {
        match stat {
            Stat::Weight => &self.weight,
            Stat::Armour => &self.armour,
            Stat::DamageBonus => &self.damage_bonus,
            Stat::ToHit => &self.to_hit,
            Stat::RangedToHit => &self.ranged_to_hit,
            Stat::Light => &self.light,
            Stat::BurnTime => &self.burn_time,
            Stat::BreakChance => &self.break_chance,
            Stat::Warmth => &self.warmth,
            Stat::Stealth => &self.stealth,
            Stat::Carry => &self.carry,
            Stat::Darkvision => &self.darkvision,
            Stat::HungerRate => &self.hunger_rate,
            Stat::SleepinessRate => &self.sleepiness_rate,
            Stat::DiseaseRisk => &self.disease_risk,
            Stat::Lockpicking => &self.lockpicking,
            Stat::HealOnKill => &self.heal_on_kill,
            Stat::Speed => &self.speed,
            Stat::BlockChance => &self.block_chance,
            Stat::Luck => &self.luck,
            Stat::Strength => &self.strength,
            Stat::Dexterity => &self.dexterity,
            Stat::Constitution => &self.constitution,
            Stat::Intelligence => &self.intelligence,
            Stat::Wisdom => &self.wisdom,
            Stat::Charisma => &self.charisma,
            Stat::Recovery => &self.recovery,
            Stat::PoisonResistance => &self.poison_resistance,
            Stat::AttackRate => &self.attack_rate,
            Stat::StaminaRefill => &self.stamina_refill,
            Stat::ManaRefill => &self.mana_refill,
            Stat::StaminaCost => &self.stamina_cost,
            Stat::GridRows => &self.grid_rows,
        }
    }

    fn field_mut(&mut self, stat: Stat) -> &mut f32 {
        match stat {
            Stat::Weight => &mut self.weight,
            Stat::Armour => &mut self.armour,
            Stat::DamageBonus => &mut self.damage_bonus,
            Stat::ToHit => &mut self.to_hit,
            Stat::RangedToHit => &mut self.ranged_to_hit,
            Stat::Light => &mut self.light,
            Stat::BurnTime => &mut self.burn_time,
            Stat::BreakChance => &mut self.break_chance,
            Stat::Warmth => &mut self.warmth,
            Stat::Stealth => &mut self.stealth,
            Stat::Carry => &mut self.carry,
            Stat::Darkvision => &mut self.darkvision,
            Stat::HungerRate => &mut self.hunger_rate,
            Stat::SleepinessRate => &mut self.sleepiness_rate,
            Stat::DiseaseRisk => &mut self.disease_risk,
            Stat::Lockpicking => &mut self.lockpicking,
            Stat::HealOnKill => &mut self.heal_on_kill,
            Stat::Speed => &mut self.speed,
            Stat::BlockChance => &mut self.block_chance,
            Stat::Luck => &mut self.luck,
            Stat::Strength => &mut self.strength,
            Stat::Dexterity => &mut self.dexterity,
            Stat::Constitution => &mut self.constitution,
            Stat::Intelligence => &mut self.intelligence,
            Stat::Wisdom => &mut self.wisdom,
            Stat::Charisma => &mut self.charisma,
            Stat::Recovery => &mut self.recovery,
            Stat::PoisonResistance => &mut self.poison_resistance,
            Stat::AttackRate => &mut self.attack_rate,
            Stat::StaminaRefill => &mut self.stamina_refill,
            Stat::ManaRefill => &mut self.mana_refill,
            Stat::StaminaCost => &mut self.stamina_cost,
            Stat::GridRows => &mut self.grid_rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_keeps_stats_in_range() {
        let mut s = Stats::default();
        s.set(Stat::Warmth, -3.0);
        assert_eq!(s.warmth, 0.0);
        s.set(Stat::Stealth, -2.0);
        assert_eq!(s.stealth, -2.0);
        s.set(Stat::BreakChance, 4.0);
        assert_eq!(s.break_chance, 1.0);
    }

    #[test]
    fn stats_saved_without_wards_still_load() {
        let mut json = serde_json::to_value(Stats::default()).unwrap();
        json.as_object_mut().unwrap().remove("ward");
        json.as_object_mut().unwrap().remove("shun");
        json.as_object_mut().unwrap().remove("bane");
        let loaded: Stats = serde_json::from_value(json).unwrap();
        assert_eq!(loaded, Stats::default());
    }

    #[test]
    fn every_stat_has_its_own_field() {
        let mut s = Stats::default();
        for (i, stat) in Stat::ALL.into_iter().enumerate() {
            s.set(stat, 0.5 + i as f32 / 100.0);
        }
        for (i, stat) in Stat::ALL.into_iter().enumerate() {
            assert_eq!(s.get(stat), 0.5 + i as f32 / 100.0, "{stat:?}");
        }
    }
}
