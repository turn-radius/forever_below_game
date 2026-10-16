//! Poisoned: the lasting state a failed save against venom leaves ([`Poisoned`]), stepped by game
//! time until it wears off.
//!
//! The SRD's Poisoned condition gives disadvantage on attack rolls and ability checks
//! ([SRD 5.1 p. 358](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=358),
//! Appendix PH-A, "Poisoned"). The delver makes neither roll yet (every swing that finds a
//! creature hits, and there are no ability checks), so that is not applied yet; the state is
//! tracked and shown so the rule has somewhere to land.

use serde::{Deserialize, Serialize};

/// How long a failed save against a poisonous snake's venom poisons the delver, in game hours.
///
/// Our own. The SRD's poisonous snake (p. 386) does its venom once and leaves no condition. Its
/// lasting poisons run from 1 minute (the quasit's same 2d4, DC 10 venom, p. 273, with a save
/// each turn to end it) to 24 hours, and 1 hour is the commonest (drow poison, malice, truth
/// serum, "Poisons", p. 204). A game hour is five real minutes at the default clock, long
/// enough to be felt and seen without a save each turn.
pub const VENOM_POISONED_HOURS: f64 = 1.0;

/// Slack for float error when a step lands exactly on the end.
const EPSILON: f64 = 1e-9;

/// Whether, and for how much longer, a character is poisoned. The default is not poisoned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Poisoned {
    /// Game hours until it wears off; 0 when not poisoned.
    pub hours_left: f64,
}

impl Poisoned {
    /// Poisoned for `hours`.
    pub fn for_hours(hours: f64) -> Poisoned {
        Poisoned { hours_left: hours.max(0.0) }
    }

    /// Whether the poison is still working.
    pub fn is_active(&self) -> bool {
        self.hours_left > EPSILON
    }

    /// A fresh dose of `hours`: the longer of what is left and the new dose. Doses do not add
    /// up: "each instance of the condition has its own duration, but the condition's effects
    /// don't get worse" (SRD 5.1 p. 358), so the delver is poisoned until the longest ends.
    pub fn dose(&mut self, hours: f64) {
        self.hours_left = self.hours_left.max(hours);
    }

    /// Advances by `hours` of game time; it wears off at 0.
    pub fn step(&mut self, hours: f64) {
        self.hours_left = (self.hours_left - hours.max(0.0)).max(0.0);
        if self.hours_left <= EPSILON {
            self.hours_left = 0.0;
        }
    }

    /// Whole game minutes left, rounded up, so "1 minute" shows until it is gone.
    pub fn minutes_left(&self) -> u32 {
        (self.hours_left * 60.0 - EPSILON).ceil().max(0.0) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dose_wears_off_after_its_hours_of_game_time() {
        let mut poisoned = Poisoned::for_hours(VENOM_POISONED_HOURS);
        assert!(poisoned.is_active());
        assert_eq!(poisoned.minutes_left(), 60);
        for _ in 0..59 {
            poisoned.step(1.0 / 60.0);
        }
        assert!(poisoned.is_active());
        assert_eq!(poisoned.minutes_left(), 1);
        poisoned.step(1.0 / 60.0);
        assert!(!poisoned.is_active(), "{poisoned:?}");
        assert_eq!(poisoned, Poisoned::default());
    }

    #[test]
    fn any_step_length_gives_the_same_result() {
        let mut fine = Poisoned::for_hours(1.0);
        for _ in 0..1000 {
            fine.step(0.0003);
        }
        let mut coarse = Poisoned::for_hours(1.0);
        coarse.step(0.3);
        assert!((fine.hours_left - coarse.hours_left).abs() < 1e-9);
    }

    #[test]
    fn a_second_dose_keeps_the_longer_and_does_not_add_up() {
        let mut poisoned = Poisoned::for_hours(1.0);
        poisoned.step(0.75);
        poisoned.dose(1.0);
        assert_eq!(poisoned.hours_left, 1.0);
        poisoned.dose(0.1);
        assert_eq!(poisoned.hours_left, 1.0, "a shorter dose changes nothing");
        assert!(!Poisoned::default().is_active());
    }
}
