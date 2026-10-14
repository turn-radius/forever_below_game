//! Typed effects: what an affix does to an item's [`Stats`] or to its Armory condition.
//!
//! Ported from each affix's `fx` in the plate's `AFX`, from the card text of the affixes the
//! plate never gave an `fx`, and from the condition `dressState` sets for flaws (Rusted,
//! Moth-eaten, Mildewed, Lice-ridden, Cracked). See `docs/items.md`, "Affix effects".

use crate::stats::{Stat, Stats};
use armory::cond::Condition;
use serde::{Deserialize, Serialize};

/// A condition slider an effect can raise (the multipliers and wear of
/// [`armory::cond::Condition`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionSlider {
    Wear,
    Rust,
    Patina,
    Rot,
    Fray,
    Damage,
}

impl ConditionSlider {
    pub const ALL: [ConditionSlider; 6] =
        [ConditionSlider::Wear, ConditionSlider::Rust, ConditionSlider::Patina, ConditionSlider::Rot, ConditionSlider::Fray, ConditionSlider::Damage];

    pub fn get(self, c: &Condition) -> f32 {
        match self {
            ConditionSlider::Wear => c.wear,
            ConditionSlider::Rust => c.rust,
            ConditionSlider::Patina => c.patina,
            ConditionSlider::Rot => c.rot,
            ConditionSlider::Fray => c.fray,
            ConditionSlider::Damage => c.damage,
        }
    }

    fn get_mut(self, c: &mut Condition) -> &mut f32 {
        match self {
            ConditionSlider::Wear => &mut c.wear,
            ConditionSlider::Rust => &mut c.rust,
            ConditionSlider::Patina => &mut c.patina,
            ConditionSlider::Rot => &mut c.rot,
            ConditionSlider::Fray => &mut c.fray,
            ConditionSlider::Damage => &mut c.damage,
        }
    }
}

/// One effect on an item.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    /// Adds `by` to a stat (`it.warmth += 2`).
    Add { stat: Stat, by: f32 },
    /// Multiplies a stat by `by` (`it.wt = it.wt * .6`).
    Scale { stat: Stat, by: f32 },
    /// Adds `by` to the damage bonus of a weapon, or to the armour of anything else (the
    /// plate's Rusted: `if(it.dmg) it.dmgB -= 1; else it.ac = max(0, it.ac - 1)`).
    DamageOrArmour { by: f32 },
    /// Raises a condition slider to at least `at_least` (`c.wear = max(c.wear, .75)`).
    Condition { slider: ConditionSlider, at_least: f32 },
}

impl Modifier {
    pub fn apply(&self, stats: &mut Stats, condition: &mut Condition) {
        self.apply_to_stats(stats);
        self.apply_to_condition(condition);
    }

    /// The stat part of the effect; condition effects do nothing here.
    pub fn apply_to_stats(&self, stats: &mut Stats) {
        match *self {
            Modifier::Add { stat, by } => stats.set(stat, stats.get(stat) + by),
            Modifier::Scale { stat, by } => stats.set(stat, stats.get(stat) * by),
            Modifier::DamageOrArmour { by } => {
                let stat = if stats.is_weapon() { Stat::DamageBonus } else { Stat::Armour };
                stats.set(stat, stats.get(stat) + by);
            }
            Modifier::Condition { .. } => {}
        }
    }

    /// The condition part of the effect; stat effects do nothing here.
    pub fn apply_to_condition(&self, condition: &mut Condition) {
        if let Modifier::Condition { slider, at_least } = *self {
            let v = slider.get_mut(condition);
            *v = v.max(at_least);
        }
    }

    /// Whether this is good for the item's owner. A stat change is good when it moves the
    /// stat the way [`Stat::higher_is_better`] says; raising a condition slider never is.
    pub fn is_improvement(&self) -> bool {
        let up = |stat: Stat, grows: bool| grows == stat.higher_is_better();
        match *self {
            Modifier::Add { stat, by } => up(stat, by > 0.0),
            Modifier::Scale { stat, by } => up(stat, by > 1.0),
            Modifier::DamageOrArmour { by } => by > 0.0,
            Modifier::Condition { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::Dice;

    #[test]
    fn rusted_weakens_a_weapon_or_armour() {
        let rust = Modifier::DamageOrArmour { by: -1.0 };
        let mut cond = Condition::default();
        let mut sword = Stats { damage: Some(Dice::new(1, 8)), armour: 1.0, ..Stats::default() };
        rust.apply(&mut sword, &mut cond);
        assert_eq!((sword.damage_bonus, sword.armour), (-1.0, 1.0));
        let mut mail = Stats { armour: 0.5, ..Stats::default() };
        rust.apply(&mut mail, &mut cond);
        assert_eq!((mail.damage_bonus, mail.armour), (0.0, 0.0));
    }

    #[test]
    fn condition_only_rises() {
        let mut cond = Condition { wear: 0.9, ..Condition::default() };
        Modifier::Condition { slider: ConditionSlider::Wear, at_least: 0.6 }.apply(&mut Stats::default(), &mut cond);
        assert_eq!(cond.wear, 0.9);
        Modifier::Condition { slider: ConditionSlider::Rust, at_least: 1.5 }.apply(&mut Stats::default(), &mut cond);
        assert_eq!(cond.rust, 1.5);
    }

    #[test]
    fn reads_from_json() {
        let m: Modifier = serde_json::from_str(r#"{"scale": {"stat": "weight", "by": 0.6}}"#).unwrap();
        assert_eq!(m, Modifier::Scale { stat: Stat::Weight, by: 0.6 });
        assert!(m.is_improvement(), "lighter is better");
    }
}
