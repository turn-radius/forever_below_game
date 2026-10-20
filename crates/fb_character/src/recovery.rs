//! Recovering: a fed and watered character slowly regains hit points over game time
//! ([`Recovery`]), stepped as the needs are.
//!
//! Our own rule. The SRD heals by resting, spending Hit Dice on a short rest and getting every
//! hit point back on a long one (SRD 5.1 p. 86, "Resting"); a real-time crawl with no rests yet
//! needs something gentler between fights, and tying it to food and water gives the needs a
//! reward as well as a cost. The rate is the game's `recovery.minutes_per_hit_point` tuning,
//! [`MINUTES_PER_HIT_POINT`] as shipped.

use serde::{Deserialize, Serialize};

use crate::Needs;

/// Game minutes a recovering character takes to regain one hit point. Our own: at the default
/// clock (12 game seconds a second) that is one hit point every 50 real seconds, so a fresh
/// delver of 8 hit points mends from half in under four real minutes, slow enough that a fight
/// still costs something.
pub const MINUTES_PER_HIT_POINT: f64 = 10.0;

/// How much faster each point of recovery from gear (of Mending, of the Troll) mends: a quarter,
/// so +2 takes a hit point in 6.7 minutes instead of 10. Our own. The game's
/// `recovery.faster_per_point` tuning, this as shipped.
pub const FASTER_PER_POINT: f64 = 0.25;

/// Game minutes per hit point at `base` for a character with `points` of recovery, each
/// `faster_per_point` faster. Points below 0 never slow mending past the base.
pub fn minutes_per_hit_point(base: f64, faster_per_point: f64, points: f64) -> f64 {
    base / (1.0 + faster_per_point * points).max(1.0)
}

/// Whether hunger and thirst are both in their best band: Fed and Quenched.
pub fn fed_and_watered(needs: &Needs) -> bool {
    needs.hunger.band() == 0 && needs.thirst.band() == 0
}

/// Whether a character with `needs` is recovering: fed and watered, unless an effect stops their
/// mending (`mending_stopped`, [`crate::effect::Effects::stops_recovery`]; venom in the blood
/// does).
pub fn recovers(needs: &Needs, mending_stopped: bool) -> bool {
    fed_and_watered(needs) && !mending_stopped
}

/// Game time put toward the next hit point. It starts over whenever the character stops
/// recovering or is whole, so a meal does not bank time spent hungry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Recovery {
    /// Game minutes toward the next hit point.
    pub minutes: f64,
}

impl Recovery {
    /// Advances by `hours` of game time for a character at `hp` of `max` who `recovering` or
    /// not, at `minutes_per_point`, and returns the hit points regained: never more than
    /// would bring them to `max`. Any step length gives the same total.
    pub fn step(&mut self, hours: f64, minutes_per_point: f64, recovering: bool, hp: i32, max: i32) -> u32 {
        let missing = (max - hp).max(0) as u32;
        if !recovering || missing == 0 || minutes_per_point <= 0.0 || hours <= 0.0 {
            self.minutes = 0.0;
            return 0;
        }
        self.minutes += hours * 60.0;
        let points = ((self.minutes / minutes_per_point) + 1e-9).floor() as u32;
        self.minutes = (self.minutes - points as f64 * minutes_per_point).max(0.0);
        let gained = points.min(missing);
        if gained == missing {
            self.minutes = 0.0;
        }
        gained
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: f64 = 1.0 / 60.0;

    fn fed() -> Needs {
        Needs::default()
    }

    #[test]
    fn a_fed_and_watered_character_regains_a_point_every_ten_minutes() {
        let needs = fed();
        assert!(recovers(&needs, false));
        let mut recovery = Recovery::default();
        let mut hp = 3;
        for _ in 0..9 {
            hp += recovery.step(MINUTE, MINUTES_PER_HIT_POINT, true, hp, 8) as i32;
        }
        assert_eq!(hp, 3, "nothing before ten minutes");
        hp += recovery.step(MINUTE, MINUTES_PER_HIT_POINT, true, hp, 8) as i32;
        assert_eq!(hp, 4);
        hp += recovery.step(30.0 * MINUTE, MINUTES_PER_HIT_POINT, true, hp, 8) as i32;
        assert_eq!(hp, 7, "three more in half an hour");
    }

    #[test]
    fn never_above_the_maximum() {
        let mut recovery = Recovery::default();
        assert_eq!(recovery.step(10.0, MINUTES_PER_HIT_POINT, true, 6, 8), 2);
        assert_eq!(recovery.step(10.0, MINUTES_PER_HIT_POINT, true, 8, 8), 0);
        assert_eq!(recovery, Recovery::default(), "whole, nothing is banked");
    }

    #[test]
    fn hungry_or_thirsty_regains_nothing() {
        let mut hungry = fed();
        hungry.hunger.value = 0.3;
        let mut thirsty = fed();
        thirsty.thirst.value = 0.3;
        for needs in [hungry, thirsty] {
            assert!(!recovers(&needs, false), "{needs:?}");
            let mut recovery = Recovery::default();
            assert_eq!(recovery.step(5.0, MINUTES_PER_HIT_POINT, recovers(&needs, false), 1, 8), 0);
        }
    }

    #[test]
    fn each_point_of_recovery_mends_a_quarter_faster() {
        assert_eq!(minutes_per_hit_point(MINUTES_PER_HIT_POINT, FASTER_PER_POINT, 0.0), 10.0);
        assert_eq!(minutes_per_hit_point(MINUTES_PER_HIT_POINT, FASTER_PER_POINT, 1.0), 8.0);
        let two = minutes_per_hit_point(MINUTES_PER_HIT_POINT, FASTER_PER_POINT, 2.0);
        assert!((two - 6.667).abs() < 0.01, "{two}");
        assert_eq!(minutes_per_hit_point(MINUTES_PER_HIT_POINT, FASTER_PER_POINT, -1.0), 10.0);
        let mut recovery = Recovery::default();
        assert_eq!(recovery.step(7.0 / 60.0, two, true, 6, 8), 1, "a point in seven minutes at +2");
    }

    #[test]
    fn poisoned_regains_nothing() {
        let poisoned = true;
        assert!(!recovers(&fed(), poisoned));
        let mut recovery = Recovery::default();
        assert_eq!(recovery.step(0.5, MINUTES_PER_HIT_POINT, recovers(&fed(), poisoned), 1, 8), 0);
    }

    #[test]
    fn stopping_starts_the_count_over_and_any_step_length_gives_the_same_total() {
        let mut recovery = Recovery::default();
        recovery.step(9.0 * MINUTE, MINUTES_PER_HIT_POINT, true, 1, 8);
        recovery.step(MINUTE, MINUTES_PER_HIT_POINT, false, 1, 8);
        assert_eq!(recovery.step(MINUTE, MINUTES_PER_HIT_POINT, true, 1, 8), 0, "the nine minutes are gone");
        let (mut fine, mut coarse) = (Recovery::default(), Recovery::default());
        let mut fine_hp = 1;
        for _ in 0..3600 {
            fine_hp += fine.step(1.0 / 3600.0, MINUTES_PER_HIT_POINT, true, fine_hp, 20) as i32;
        }
        let coarse_hp = 1 + coarse.step(1.0, MINUTES_PER_HIT_POINT, true, 1, 20) as i32;
        assert_eq!((fine_hp, coarse_hp), (7, 7));
    }
}
