//! The numbers an item gives the character who carries or wears it, and the names affixes
//! use to change them.
//!
//! Stats split into two categories:
//! - Item properties: Weight, Armour, DamageBonus, ToHit, Light, BurnTime, BreakChance.
//!   These describe the item itself.
//! - Wearer stats: Warmth, Stealth, Carry, Darkvision, HungerRate, SleepinessRate, DiseaseRisk,
//!   Lockpicking, HealOnKill. These affect the character wearing the item.
//!
//! This is the minimal set the affixes in `data/afx.json` act on: the plate's item fields
//! (`wt`, `ac`, `dmg`, `dmgB`, `warmth`, `light`, `carry`) plus one stat for each affix the
//! plate only described in words (stealth, to hit, darkvision...). Deriving the real values
//! per family (the plate's `saMeta`) is a later task.

use crate::dice::Dice;
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
}

impl Stat {
    pub const ALL: [Stat; 16] = [
        Stat::Weight, Stat::Armour, Stat::DamageBonus, Stat::ToHit, Stat::Light, Stat::BurnTime, Stat::BreakChance,
        Stat::Warmth, Stat::Stealth, Stat::Carry, Stat::Darkvision, Stat::HungerRate, Stat::SleepinessRate, Stat::DiseaseRisk,
        Stat::Lockpicking, Stat::HealOnKill,
    ];

    /// Item properties: describe the item itself (Weight, Armour, DamageBonus, ToHit, Light, BurnTime, BreakChance).
    pub fn is_item_property(self) -> bool {
        matches!(self, Stat::Weight | Stat::Armour | Stat::DamageBonus | Stat::ToHit | Stat::Light | Stat::BurnTime | Stat::BreakChance)
    }

    /// Wearer stats: affect the character wearing the item (Warmth, Stealth, Carry, Darkvision, HungerRate, SleepinessRate, DiseaseRisk, Lockpicking, HealOnKill).
    pub fn is_wearer_stat(self) -> bool {
        !self.is_item_property()
    }

    /// Whether more of it is better for the wearer.
    pub fn higher_is_better(self) -> bool {
        !matches!(self, Stat::Weight | Stat::HungerRate | Stat::SleepinessRate | Stat::DiseaseRisk | Stat::BreakChance)
    }

    /// The range it is kept in. Bonuses can go negative (the plate's `dmgB -= 1`); amounts,
    /// distances and rates cannot (its `Math.max(0, it.warmth - 1)`).
    pub fn range(self) -> (f32, f32) {
        match self {
            Stat::DamageBonus | Stat::ToHit | Stat::Stealth | Stat::Lockpicking => (f32::NEG_INFINITY, f32::INFINITY),
            Stat::BreakChance => (0.0, 1.0),
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
        }
    }

    fn field_mut(&mut self, stat: Stat) -> &mut f32 {
        match stat {
            Stat::Weight => &mut self.weight,
            Stat::Armour => &mut self.armour,
            Stat::DamageBonus => &mut self.damage_bonus,
            Stat::ToHit => &mut self.to_hit,
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
