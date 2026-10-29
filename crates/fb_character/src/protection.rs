//! Divine Protection: a cleric brought to 0 hit points may be saved before they fall. Our own,
//! beside [`crate::revival`]; the spell-like feel follows the Cleric's Divine Intervention
//! (SRD 5.1 p. 30), but it is a saving throw, not a percentage.
//!
//! - Only a cleric calls on it. When they would fall, they roll a Wisdom saving throw (d20 + the
//!   Wisdom modifier, [`crate::combat::saving_throw`]) against [`ProtectionRules::dc`].
//! - A passed save stands them up instead of falling, and spends no revive. How much it does
//!   depends on the [`Tier`], which their Wisdom modifier sets ([`ProtectionRules::tier`]):
//!   one revives them, two also heals three quarters of their life, three also clears the
//!   effects that ail them.
//! - A prayer is answered or not once between sleeps ([`ProtectionRules::per_rest`]), pass or
//!   fail, so it is not an endless shield.
//! - A failed save, or no prayer left, falls as before, to the revive rules.
//!
//! Encumbrance is not cleared: a burden follows from what the cleric carries, so it is not an
//! effect that can be lifted.

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::combat::{Save, saving_throw};
use crate::conditions::Severity;
use crate::effect::Effects;
use crate::rules::Rules;

/// What a passed save does, from least to most.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tier {
    /// Stands the cleric up.
    Revive,
    /// Also heals most of their life.
    Heal,
    /// Also clears what ails them.
    Cleanse,
}

/// The numbers Divine Protection runs on (`[protection]` in `data/rules.toml`). Our own, a
/// first guess to play against.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProtectionRules {
    /// The difficulty class of the Wisdom saving throw.
    pub dc: i32,
    /// The Wisdom modifier from which the second tier acts.
    pub heal_from: i32,
    /// The Wisdom modifier from which the third tier acts.
    pub cleanse_from: i32,
    /// Share of their maximum hit points the second and third tiers heal them to.
    pub heal_share: f64,
    /// Prayers a cleric may make between sleeps, pass or fail.
    pub per_rest: u32,
}

impl Default for ProtectionRules {
    fn default() -> Self {
        ProtectionRules { dc: 12, heal_from: 2, cleanse_from: 3, heal_share: 0.75, per_rest: 1 }
    }
}

impl ProtectionRules {
    /// The tier a Wisdom modifier of `wisdom` gives.
    pub fn tier(&self, wisdom: i32) -> Tier {
        if wisdom >= self.cleanse_from {
            Tier::Cleanse
        } else if wisdom >= self.heal_from {
            Tier::Heal
        } else {
            Tier::Revive
        }
    }

    /// The hit points a saved cleric of `max` stands with: 1 at tier one, `heal_share` of
    /// their maximum (at least 1) from tier two.
    pub fn hit_points(&self, tier: Tier, max: i32) -> i32 {
        match tier {
            Tier::Revive => 1.min(max.max(1)),
            _ => ((max as f64 * self.heal_share.clamp(0.0, 1.0)).round() as i32).clamp(1, max.max(1)),
        }
    }
}

/// How a prayer went.
#[derive(Clone, Debug, PartialEq)]
pub struct Prayer {
    pub save: Save,
    /// The tier that acts, when the save passed.
    pub tier: Option<Tier>,
}

/// A cleric of Wisdom modifier `wisdom` who has prayed `prayed` times since they last slept
/// would fall. `None` when no prayer is left (nothing is rolled); otherwise the save, one draw.
pub fn pray(rules: &Rules, wisdom: i32, prayed: u32, rng: &mut Mulberry32) -> Option<Prayer> {
    let protection = &rules.protection;
    if prayed >= protection.per_rest {
        return None;
    }
    let save = saving_throw("WIS", wisdom, protection.dc, rng);
    let tier = save.succeeded.then(|| protection.tier(wisdom));
    Some(Prayer { save, tier })
}

/// Clears every effect that is showing at `Mild` or worse, wounds that never heal included,
/// and returns how many it cleared. Ones that do the cleric good stay.
pub fn cleanse(effects: &mut Effects, rules: &Rules) -> usize {
    let ailing: Vec<_> = effects.showing(&rules.effects).filter(|(.., stage)| stage.severity >= Severity::Mild).map(|(a, ..)| (a.id.clone(), a.part)).collect();
    for (id, part) in &ailing {
        effects.remove(id, *part);
    }
    ailing.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::Source;

    fn rules() -> Rules {
        Rules::default()
    }

    /// A seed whose first d20 is at least / at most `bound`.
    fn seed_with(pass: bool, wisdom: i32) -> u32 {
        let dc = rules().protection.dc;
        (0..1000).find(|&s| saving_throw("WIS", wisdom, dc, &mut Mulberry32::new(s)).succeeded == pass).expect("a seed")
    }

    #[test]
    fn wisdom_sets_the_tier() {
        let p = ProtectionRules::default();
        assert_eq!([-1, 0, 1].map(|w| p.tier(w)), [Tier::Revive; 3]);
        assert_eq!(p.tier(2), Tier::Heal);
        assert_eq!([3, 4].map(|w| p.tier(w)), [Tier::Cleanse; 2]);
    }

    #[test]
    fn tier_one_stands_them_up_on_one_hit_point() {
        assert_eq!(ProtectionRules::default().hit_points(Tier::Revive, 20), 1);
    }

    #[test]
    fn tiers_two_and_three_heal_three_quarters() {
        let p = ProtectionRules::default();
        assert_eq!((p.hit_points(Tier::Heal, 20), p.hit_points(Tier::Cleanse, 20), p.hit_points(Tier::Heal, 1)), (15, 15, 1));
    }

    #[test]
    fn a_passed_save_gives_the_tier() {
        for (wisdom, tier) in [(1, Tier::Revive), (2, Tier::Heal), (3, Tier::Cleanse)] {
            let prayer = pray(&rules(), wisdom, 0, &mut Mulberry32::new(seed_with(true, wisdom))).expect("a prayer");
            assert!(prayer.save.succeeded);
            assert_eq!(prayer.tier, Some(tier));
        }
    }

    #[test]
    fn a_failed_save_gives_nothing() {
        let prayer = pray(&rules(), 3, 0, &mut Mulberry32::new(seed_with(false, 3))).expect("a prayer");
        assert!(!prayer.save.succeeded);
        assert_eq!(prayer.tier, None);
    }

    #[test]
    fn one_prayer_between_sleeps_and_none_rolled_after() {
        let mut rng = Mulberry32::new(seed_with(true, 3));
        let mut untouched = rng.clone();
        assert!(pray(&rules(), 3, 1, &mut rng).is_none());
        assert_eq!(rng.next_f64(), untouched.next_f64(), "no draw was spent");
    }

    #[test]
    fn cleansing_clears_what_ails_and_keeps_what_helps() {
        let rules = rules();
        let mut effects = Effects::default();
        effects.add(&rules.effects, "lame_leg", 1.0, Some(fb_items::Slot::Legs), Source::Fall);
        effects.add(&rules.effects, "poisoned", 1.0, None, Source::Fall);
        let helps: Vec<_> = rules.effects.iter().filter(|(_, d)| d.stages.iter().all(|s| s.severity == Severity::Good)).map(|(id, _)| id.clone()).collect();
        if let Some(id) = helps.first() {
            effects.add(&rules.effects, id, 1.0, None, Source::Fall);
        }
        let cleared = cleanse(&mut effects, &rules);
        assert!(cleared >= 2);
        assert!(effects.get("lame_leg", Some(fb_items::Slot::Legs)).is_none() && effects.get("poisoned", None).is_none());
        assert_eq!(effects.active.len(), usize::from(!helps.is_empty()));
    }
}
