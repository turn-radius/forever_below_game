//! Passing time in the lab ([`crate::lab_view`]): the character's clock, needs and exhaustion
//! step through game time under the chosen activity, with the lab's gear applied. The state is
//! plain data and every step is [`ExhaustionTimes::step`], which steps the needs itself, as
//! `simulate` steps the delver's, so it runs without Bevy.
//!
//! Time passes in whole minutes (the meters rounded to a trillionth after each, so a meter lands
//! on a band's edge when it should), so a step of any length is the same minutes in a row and one
//! 8 hour step gives exactly the meters eight 1 hour steps do. Inside each minute a band is
//! crossed where the meter's rate puts it, in closed form, and a level of exhaustion is logged
//! at the end of the minute it was earned in. Days are counted from 0, so a log line at "Day 4
//! 00:00" is after four days.

use std::collections::VecDeque;

use fb_character::{Activity, Exhaustion, ExhaustionTimes, GameTime, Meter, MeterKind, Needs, modifier};
use fb_items::provision;

use crate::lab_view::LabState;

/// How long a step is cut into, in game hours: a minute.
pub const MINUTE: f64 = 1.0 / 60.0;
/// How many crossings the log keeps.
pub const LOG_LINES: usize = 16;
/// What Eat eats: the bread's nutrition ([`fb_items::provision`]).
pub const RATION: &str = "Bread";
/// What Drink drinks: the tankard's water.
pub const DRAUGHT: &str = "Tankard";
/// The time buttons' steps, in game hours.
pub const STEPS: [(&str, f64); 4] = [("+10 min", 10.0 / 60.0), ("+1 h", 1.0), ("+8 h", 8.0), ("+1 day", 24.0)];

/// The activities the lab offers, in the panel's order.
pub const ACTIVITIES: [Activity; 4] = [Activity::Resting, Activity::Walking, Activity::Exerting, Activity::Sleeping];

/// An activity's word, lower case.
pub fn activity_word(activity: Activity) -> &'static str {
    match activity {
        Activity::Resting => "resting",
        Activity::Walking => "walking",
        Activity::Exerting => "exerting",
        Activity::Sleeping => "sleeping",
    }
}

/// A game time as `Day 4 08:30`: days elapsed, then the hour and minute.
pub fn clock_text(hours: f64) -> String {
    let seconds = (hours * 3600.0).round() as u64;
    format!("Day {} {:02}:{:02}", seconds / 86_400, seconds / 3600 % 24, seconds / 60 % 60)
}

/// One line of the log: when a band or a level was crossed.
#[derive(Clone, Debug, PartialEq)]
pub struct Crossing {
    /// Game hours since the clock started.
    pub hours: f64,
    /// What crossed: `Hunger → Hungry`, `Exhaustion 1`.
    pub what: String,
}

impl Crossing {
    pub fn line(&self) -> String {
        format!("{} {}", clock_text(self.hours), self.what)
    }
}

/// The character's clock, needs and exhaustion, and where they crossed a band or a level.
#[derive(bevy::prelude::Resource, Clone, Debug, PartialEq)]
pub struct TimeLab {
    pub clock: GameTime,
    pub needs: Needs,
    pub exhaustion: Exhaustion,
    pub times: ExhaustionTimes,
    /// The latest crossings, oldest first, at most [`LOG_LINES`].
    pub log: VecDeque<Crossing>,
}

impl Default for TimeLab {
    /// Fresh: the clock at 0, every meter at 0 and no exhaustion.
    fn default() -> Self {
        TimeLab { clock: GameTime::default(), needs: Needs::default(), exhaustion: Exhaustion::default(), times: ExhaustionTimes::default(), log: VecDeque::new() }
    }
}

impl TimeLab {
    /// Sets the clock and needs back to fresh and clears the log.
    pub fn reset(&mut self) {
        *self = TimeLab::default();
    }

    fn note(&mut self, hours: f64, what: String) {
        self.log.push_back(Crossing { hours, what });
        while self.log.len() > LOG_LINES {
            self.log.pop_front();
        }
    }

    /// Passes `hours` of game time (to the nearest minute) under the lab's activity, with the
    /// lab's gear applied to the needs and its Constitution deciding how long Starving lasts.
    pub fn pass(&mut self, state: &LabState, hours: f64) {
        state.character.gear(&state.inventory()).apply_to(&mut self.needs);
        let (activity, con) = (state.activity, modifier(state.character.constitution()));
        for _ in 0..(hours / MINUTE).round() as u32 {
            let (before, level, start) = (self.needs, self.exhaustion.level, self.clock.hours());
            self.times.step(&mut self.needs, &mut self.exhaustion, activity, MINUTE, con);
            self.clock.advance(MINUTE * 3600.0);
            // A minute is not exact in binary, so a meter that should land on a band's edge (0.5
            // after 8 hours) is held to a trillionth, as the edge itself is.
            for meter in [&mut self.needs.hunger, &mut self.needs.thirst, &mut self.needs.sleepiness] {
                meter.value = (meter.value * 1e12).round() / 1e12;
            }
            for (was, now) in [(before.hunger, self.needs.hunger), (before.thirst, self.needs.thirst), (before.sleepiness, self.needs.sleepiness)] {
                for (at, band) in band_crossings(&was, &now, activity) {
                    self.note(start + at, format!("{} → {}", name(now.kind), now.kind.word(band)));
                }
            }
            if self.exhaustion.level != level {
                self.note(start + MINUTE, if self.exhaustion.is_dead() { "Exhaustion 6, dead".to_string() } else { format!("Exhaustion {}", self.exhaustion.level) });
            }
        }
    }

    /// Eats a ration, taking its nutrition off hunger.
    pub fn eat(&mut self) {
        if let Some(p) = provision(RATION) {
            self.needs.consume(p.nutrition, p.water);
        }
    }

    /// Drinks a draught, taking its water off thirst.
    pub fn drink(&mut self) {
        if let Some(p) = provision(DRAUGHT) {
            self.needs.consume(p.nutrition, p.water);
        }
    }
}

fn name(kind: MeterKind) -> &'static str {
    match kind {
        MeterKind::Hunger => "Hunger",
        MeterKind::Thirst => "Thirst",
        MeterKind::Sleepiness => "Sleepiness",
    }
}

/// The bands `was` crossed on the way to `now` in one minute, each with the hours into the
/// minute it crossed at: where the meter's value, rising or falling at its rate, meets the
/// band's edge. The band reached is with each, in the order crossed.
fn band_crossings(was: &Meter, now: &Meter, activity: Activity) -> Vec<(f64, usize)> {
    let (from, to) = (was.band(), now.band());
    let rate = now.effective_rate(activity);
    let at = |band: usize| ((band as f64 * 0.25 - was.value) / rate).clamp(0.0, MINUTE);
    if to > from {
        (from + 1..=to).map(|band| (at(band), band)).collect()
    } else {
        // Falling: the band below k is entered as the value reaches k's edge.
        (to..from).rev().map(|band| (at(band + 1), band)).collect()
    }
}

/// A meter's value as a percent, `37%`.
pub fn percent_text(meter: &Meter) -> String {
    format!("{:.0}%", meter.value * 100.0)
}

/// What a level of exhaustion does: `full speed`, `speed ×0.5`, `cannot move`, `dead`.
pub fn exhaustion_effect(exhaustion: Exhaustion) -> String {
    if exhaustion.is_dead() {
        return "dead".to_string();
    }
    match exhaustion.speed_multiplier() {
        m if m >= 1.0 => "full speed".to_string(),
        m if m <= 0.0 => "cannot move".to_string(),
        m => format!("speed ×{m}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_body::Folk;
    use fb_items::Slot;

    fn bare() -> LabState {
        let mut state = LabState::default();
        state.slots.iter_mut().for_each(|s| s.base = None);
        state
    }

    /// The hours at which `what` was logged first, if it was.
    fn crossed(lab: &TimeLab, what: &str) -> Option<f64> {
        lab.log.iter().find(|c| c.what == what).map(|c| c.hours)
    }

    #[test]
    fn a_human_walking_from_fed_is_hungry_when_the_simulation_says() {
        let state = bare();
        let mut lab = TimeLab::default();
        lab.pass(&state, 7.0);
        lab.pass(&state, 1.0 - 10.0 / 60.0);
        assert_eq!(lab.needs.hunger.word(), "Peckish");
        lab.pass(&state, 10.0 / 60.0);
        assert_eq!(lab.needs.hunger.word(), "Hungry");
        // The simulation's own step reads Hungry after the same game time.
        let mut needs = Needs::default();
        needs.step(8.0 - 0.001, Activity::Walking);
        assert_eq!(needs.hunger.word(), "Peckish");
        needs.step(0.002, Activity::Walking);
        assert_eq!(needs.hunger.word(), "Hungry");
        assert!((crossed(&lab, "Hunger → Hungry").unwrap() - 8.0).abs() < 1e-6, "{:?}", lab.log);
        assert_eq!(lab.log.iter().find(|c| c.what == "Hunger → Hungry").unwrap().line(), "Day 0 08:00 Hunger → Hungry");
    }

    #[test]
    fn eight_one_hour_steps_equal_one_eight_hour_step() {
        for activity in ACTIVITIES {
            let mut state = bare();
            state.activity = activity;
            let (mut one, mut eight) = (TimeLab::default(), TimeLab::default());
            one.pass(&state, 8.0);
            (0..8).for_each(|_| eight.pass(&state, 1.0));
            assert_eq!(one, eight, "{activity:?}");
            assert_eq!(one.clock.hours(), 8.0);
        }
        // Over days, through exhaustion.
        let state = bare();
        let (mut one, mut many) = (TimeLab::default(), TimeLab::default());
        one.pass(&state, 24.0 * 6.0);
        (0..6).for_each(|_| many.pass(&state, 24.0));
        assert_eq!(one, many);
        assert!(one.exhaustion.level > 0);
    }

    #[test]
    fn a_dwarf_with_con_13_left_starving_gains_exhaustion_at_day_4() {
        let mut state = bare();
        state.character.folk = Folk::Dwarf;
        state.character.attributes.constitution = 13;
        let mut lab = TimeLab::default();
        lab.needs.hunger.value = 1.0;
        // Thirst and sleep are kept out of it.
        lab.needs.thirst.rate_per_hour = 0.0;
        lab.needs.sleepiness.rate_per_hour = 0.0;
        lab.pass(&state, 95.0);
        assert_eq!(lab.exhaustion.level, 0, "none through day 4");
        lab.pass(&state, 1.0);
        assert_eq!(lab.exhaustion.level, 1);
        assert_eq!(crossed(&lab, "Exhaustion 1"), Some(96.0));
        assert_eq!(lab.log.back().unwrap().line(), "Day 4 00:00 Exhaustion 1");
        assert_eq!(exhaustion_effect(lab.exhaustion), "full speed");
    }

    #[test]
    fn two_of_plenty_push_hungry_later_by_1_over_07225() {
        let plenty = |state: &mut LabState| {
            let seed = (1..3000).find(|&s| fb_items::roll_item("Belt", s).is_some_and(|i| i.affixes.iter().any(|a| a.name == "of Plenty"))).unwrap();
            state.equip(Slot::Belt, "Belt", seed);
            let ring = (1..3000).find(|&s| fb_items::roll_item("Ring", s).is_some_and(|i| i.affixes.iter().any(|a| a.name == "of Plenty"))).unwrap();
            state.equip(Slot::Ring1, "Ring", ring);
        };
        let mut state = bare();
        let hungry = |state: &LabState| {
            let mut lab = TimeLab::default();
            lab.pass(state, 24.0);
            crossed(&lab, "Hunger → Hungry").unwrap()
        };
        let plain = hungry(&state);
        plenty(&mut state);
        // Other affixes on the same items may slow hunger too, so switch the rest off.
        for lab in state.slots.iter_mut().filter(|s| s.base.is_some()) {
            let kept = lab.roll().unwrap().affixes.iter().filter(|a| a.name != "of Plenty").map(|a| a.name.clone()).collect();
            lab.off = kept;
        }
        let slowed = hungry(&state);
        assert!((slowed / plain - 1.0 / 0.7225).abs() < 1e-3, "{slowed} against {plain}");
    }

    #[test]
    fn sleeping_clears_sleepiness_and_logs_the_bands_it_falls_through() {
        let mut state = bare();
        state.activity = Activity::Sleeping;
        let mut lab = TimeLab::default();
        lab.needs.sleepiness.value = 0.6;
        lab.pass(&state, 2.0);
        // 0.6 falls 1/8 an hour: to 0.5 at 0.8 h (Weary → Tired), to 0.25 after 2.8 h.
        assert!((crossed(&lab, "Sleepiness → Tired").unwrap() - 0.8).abs() < 1e-6, "{:?}", lab.log);
        assert!((lab.needs.sleepiness.value - 0.35).abs() < 1e-9);
    }

    #[test]
    fn eating_and_drinking_set_the_needs_back_and_reset_starts_over() {
        let state = bare();
        let mut lab = TimeLab::default();
        lab.pass(&state, 24.0);
        let (hunger, thirst) = (lab.needs.hunger.value, lab.needs.thirst.value);
        lab.eat();
        lab.drink();
        assert!((lab.needs.hunger.value - (hunger - provision(RATION).unwrap().nutrition)).abs() < 1e-12);
        assert!((lab.needs.thirst.value - (thirst - provision(DRAUGHT).unwrap().water).max(0.0)).abs() < 1e-12);
        assert!(!lab.log.is_empty());
        lab.reset();
        assert_eq!(lab, TimeLab::default());
    }

    #[test]
    fn the_clock_reads_days_and_hours() {
        assert_eq!(clock_text(0.0), "Day 0 00:00");
        assert_eq!(clock_text(24.0 * 4.0 + 8.5), "Day 4 08:30");
        assert_eq!(exhaustion_effect(Exhaustion::new(2)), "speed ×0.5");
        assert_eq!(exhaustion_effect(Exhaustion::new(5)), "cannot move");
        assert_eq!(exhaustion_effect(Exhaustion::new(6)), "dead");
    }
}
