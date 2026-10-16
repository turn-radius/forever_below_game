//! The Time tab's run ([`crate::lab_view`]): a script of steps (pass some hours, pass until
//! something happens, eat, drink) played from a fresh character under the rules in force, as a
//! [`Timeline`] of samples for the chart and the answers the tiles read ([`answers`]).
//!
//! No Bevy and no randomness: time passes through [`TimeLab::pass_under`] in quarter hours, which
//! is the minute step the game's own meters take, so a 7 day run is exactly seven 1 day runs and the same
//! script under the saved rules gives the run the chart draws faint beside it. Eating and
//! drinking happen at the end of the run so far, so the next step goes on from there.

use fb_character::explain::number;
use fb_character::rules::Rules;
use fb_character::{Activity, MeterKind};

use crate::lab_time::{Crossing, Pace, TimeLab};
use crate::lab_view::LabState;

/// The hours a run advances between samples.
pub const CHUNK: f64 = 0.25;
/// The longest an "until" step goes on looking, in game hours: 14 days.
pub const LONGEST: f64 = 14.0 * 24.0;

/// What an "until" step waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    /// Hunger reaches its third band, Hungry.
    Hungry,
    /// Exhaustion reaches level 1.
    Exhaustion,
}

/// One step of a run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// Passes this many game hours under an activity.
    Pass(f64, Activity),
    /// Passes time under an activity until the goal is met, or [`LONGEST`].
    Until(Goal, Activity),
    Eat,
    Drink,
}

/// A moment of the run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub hours: f64,
    pub hunger: f64,
    pub thirst: f64,
    pub sleepiness: f64,
    pub exhaustion: u32,
}

/// Something done to the character in the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Eat,
    Drink,
}

/// A run played out.
#[derive(Clone, Debug, PartialEq)]
pub struct Timeline {
    pub samples: Vec<Sample>,
    pub events: Vec<(f64, Event)>,
    pub crossings: Vec<Crossing>,
    /// The character at the end of the run.
    pub lab: TimeLab,
}

impl Timeline {
    /// The game hours the run lasted.
    pub fn end(&self) -> f64 {
        self.samples.last().map_or(0.0, |s| s.hours)
    }

    /// When a need first reached `band`, or exhaustion `level`.
    fn first(&self, pick: impl Fn(&Crossing) -> bool) -> Option<f64> {
        self.crossings.iter().find(|c| pick(c)).map(|c| c.hours)
    }
}

fn sample(lab: &TimeLab) -> Sample {
    Sample { hours: lab.clock.hours(), hunger: lab.needs.hunger.value, thirst: lab.needs.thirst.value, sleepiness: lab.needs.sleepiness.value, exhaustion: lab.exhaustion.level }
}

fn reached(goal: Goal, lab: &TimeLab, rules: &Rules) -> bool {
    match goal {
        Goal::Hungry => lab.needs.hunger.band_with(rules) >= 2,
        Goal::Exhaustion => lab.exhaustion.level >= 1,
    }
}

/// Plays `steps` for `state`'s character from fresh, under the rules in `state`.
pub fn run(state: &LabState, steps: &[Step]) -> Timeline {
    let mut pace = Pace::new(state);
    let mut lab = TimeLab::default();
    let mut samples = vec![sample(&lab)];
    let mut events = Vec::new();
    for step in steps {
        match *step {
            Step::Pass(hours, activity) => {
                pace.activity = activity;
                for _ in 0..(hours / CHUNK).round() as u32 {
                    lab.pass_under(&pace, CHUNK);
                    samples.push(sample(&lab));
                }
            }
            Step::Until(goal, activity) => {
                pace.activity = activity;
                let start = lab.clock.hours();
                while lab.clock.hours() - start < LONGEST && !reached(goal, &lab, &state.ruleset.rules) {
                    lab.pass_under(&pace, CHUNK);
                    samples.push(sample(&lab));
                }
            }
            Step::Eat | Step::Drink => {
                let at = lab.clock.hours();
                if *step == Step::Eat {
                    lab.eat();
                    events.push((at, Event::Eat));
                } else {
                    lab.drink();
                    events.push((at, Event::Drink));
                }
                samples.push(sample(&lab));
            }
        }
    }
    Timeline { samples, events, crossings: lab.all.clone(), lab }
}

/// A plain answer: what happened, and when.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    /// `Hungry after`.
    pub label: String,
    /// `8 h`, or `not reached`.
    pub value: String,
    /// Whether it is a danger, drawn in red.
    pub danger: bool,
}

/// Game hours as the tiles write them: `8 h`, `2.5 h`, `4 days` from 3 days on.
pub fn hours_text(hours: f64) -> String {
    if hours >= 72.0 { format!("{} days", number(hours / 24.0)) } else { format!("{} h", number(hours)) }
}

/// To the minute, which the run steps in.
fn to_the_minute(hours: f64) -> f64 {
    (hours * 60.0).round() / 60.0
}

/// An hour of the run as a person counts days, the first being day 1: `day 5 (112 h)`.
pub fn day_text(hours: f64) -> String {
    let hours = to_the_minute(hours);
    format!("day {} ({} h)", (hours / 24.0).floor() + 1.0, number(hours))
}

/// The band at which a need starts to hurt: Hungry, Parched, Weary.
pub const HURTS_AT: usize = 2;
/// The exhaustion level that is death.
pub const DEATH: u32 = 6;

/// The answers of a run: when each need first reached the band where it starts to hurt, when
/// exhaustion first reached level 1, and when it reached death.
pub fn answers(run: &Timeline, rules: &Rules) -> Vec<Answer> {
    let after = |kind: MeterKind| {
        let when = run.first(|c| c.band == Some((kind, HURTS_AT)));
        Answer { label: format!("{} after", rules.needs.word(kind, HURTS_AT)), value: when.map_or_else(|| "not reached".to_string(), |h| hours_text(to_the_minute(h))), danger: false }
    };
    let level = |level: u32, label: &str, danger: bool| {
        let when = run.first(|c| c.level == Some(level));
        Answer { label: label.to_string(), value: when.map_or_else(|| "not reached".to_string(), day_text), danger }
    };
    vec![after(MeterKind::Hunger), after(MeterKind::Thirst), after(MeterKind::Sleepiness), level(1, "Exhaustion 1 on", false), level(DEATH, "Dies on", true)]
}

/// When the run reached death, if it did.
pub fn death(run: &Timeline) -> Option<f64> {
    run.first(|c| c.level == Some(DEATH))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab_rules::{Leaf, set_leaf};
    use fb_body::Folk;
    use fb_items::Slot;

    fn bare() -> LabState {
        let mut state = LabState::default();
        state.slots.iter_mut().for_each(|s| s.base = None);
        state
    }

    fn value<'a>(answers: &'a [Answer], label: &str) -> &'a str {
        &answers.iter().find(|a| a.label == label).unwrap_or_else(|| panic!("no {label} in {answers:?}")).value
    }

    #[test]
    fn a_walking_human_reads_hungry_at_8_hours() {
        let state = bare();
        let run = run(&state, &[Step::Pass(24.0, Activity::Walking)]);
        let answers = answers(&run, &state.ruleset.rules);
        assert_eq!(value(&answers, "Hungry after"), "8 h");
        assert_eq!(value(&answers, "Parched after"), "5 h");
        assert_eq!(value(&answers, "Weary after"), "16 h");
        assert_eq!(value(&answers, "Exhaustion 1 on"), "not reached");
    }

    #[test]
    fn sixteen_to_twenty_hours_to_full_moves_hungry_to_10_hours() {
        let mut state = bare();
        set_leaf(&mut state.ruleset, "needs/hunger_hours_to_full", Leaf::Float(20.0));
        let steps = [Step::Pass(24.0, Activity::Walking)];
        let answers = answers(&run(&state, &steps), &state.ruleset.rules);
        assert_eq!(value(&answers, "Hungry after"), "10 h");
        let saved = bare();
        assert_eq!(value(&crate::lab_timeline::answers(&run(&saved, &steps), &saved.ruleset.rules), "Hungry after"), "8 h");
    }

    #[test]
    fn a_con_13_dwarf_left_starving_has_exhaustion_1_on_day_4() {
        let mut state = bare();
        state.character.folk = Folk::Dwarf;
        state.character.attributes.constitution = 13;
        // Hunger alone: thirst and sleep never come.
        for key in ["needs/thirst_hours_to_full", "needs/sleepiness_hours_to_full"] {
            set_leaf(&mut state.ruleset, key, Leaf::Float(1e15));
        }
        // Eating cannot be skipped to start starving, so wait for it: starving is hunger at full, and
        // the first level comes 3 + CON days after that.
        let run = run(&state, &[Step::Until(Goal::Exhaustion, Activity::Walking)]);
        let answers = answers(&run, &state.ruleset.rules);
        let when = run.crossings.iter().find(|c| c.level == Some(1)).unwrap().hours;
        let starving_at = run.crossings.iter().find(|c| c.band == Some((MeterKind::Hunger, 4))).unwrap().hours;
        assert!((starving_at - 16.0).abs() < 1e-6, "hunger is full after 16 hours walking: {starving_at}");
        assert!((when - starving_at - 24.0 * 4.0).abs() < 0.1, "3 + 1 days starving: {}", when - starving_at);
        assert!(value(&answers, "Exhaustion 1 on").starts_with("day 5 ("), "{answers:?}");
    }

    #[test]
    fn one_seven_day_run_equals_seven_one_day_runs() {
        let state = LabState::default();
        let one = run(&state, &[Step::Pass(168.0, Activity::Walking)]);
        let seven = run(&state, &[Step::Pass(24.0, Activity::Walking); 7]);
        assert_eq!(one, seven);
        assert_eq!(one.end(), 168.0);
        assert!(one.lab.exhaustion.level > 0, "a week without food or water wears them down");
    }

    #[test]
    fn eating_and_drinking_are_events_at_the_end_of_the_run_so_far() {
        let state = bare();
        let run = run(&state, &[Step::Pass(12.0, Activity::Walking), Step::Eat, Step::Drink, Step::Pass(4.0, Activity::Walking)]);
        assert_eq!(run.events, vec![(12.0, Event::Eat), (12.0, Event::Drink)]);
        let at = run.samples.iter().position(|s| s.hours == 12.0).unwrap();
        let (before, after) = (run.samples[at], run.samples[at + 2]);
        assert!(after.hunger < before.hunger && after.thirst < before.thirst, "{before:?} {after:?}");
        assert_eq!(run.end(), 16.0);
    }

    #[test]
    fn until_stops_at_the_goal_or_after_14_days() {
        let state = bare();
        let hungry = run(&state, &[Step::Until(Goal::Hungry, Activity::Walking)]);
        assert!((8.0..8.5).contains(&hungry.end()), "{}", hungry.end());
        // Resting slowly, nothing is exhausted within the cap.
        let mut slow = bare();
        set_leaf(&mut slow.ruleset, "exhaustion/starving_first_days", Leaf::Float(1000.0));
        set_leaf(&mut slow.ruleset, "exhaustion/dehydrated_first_hours", Leaf::Float(100_000.0));
        set_leaf(&mut slow.ruleset, "exhaustion/spent_first_hours", Leaf::Float(100_000.0));
        assert_eq!(run(&slow, &[Step::Until(Goal::Exhaustion, Activity::Resting)]).end(), LONGEST);
    }

    #[test]
    fn gear_and_activity_change_the_run() {
        let mut state = bare();
        let walking = run(&state, &[Step::Pass(8.0, Activity::Walking)]);
        let resting = run(&state, &[Step::Pass(8.0, Activity::Resting)]);
        assert!(resting.lab.needs.hunger.value < walking.lab.needs.hunger.value);
        let plenty = (1..3000).find(|&s| fb_items::roll_item("Belt", s).is_some_and(|i| i.affixes.iter().any(|a| a.name == "of Plenty"))).unwrap();
        state.equip(Slot::Belt, "Belt", plenty);
        let fed = run(&state, &[Step::Pass(8.0, Activity::Walking)]);
        assert!(fed.lab.needs.hunger.value < walking.lab.needs.hunger.value);
    }

    #[test]
    fn hours_read_as_hours_then_days() {
        assert_eq!((hours_text(8.0).as_str(), hours_text(2.5).as_str(), hours_text(96.0).as_str()), ("8 h", "2.5 h", "4 days"));
    }

    #[test]
    fn days_count_from_one_and_death_is_an_answer() {
        assert_eq!(day_text(112.0), "day 5 (112 h)");
        assert_eq!(day_text(95.9999), "day 5 (96 h)");
        assert_eq!(day_text(0.0), "day 1 (0 h)");
        let state = bare();
        let run = run(&state, &[Step::Pass(LONGEST, Activity::Walking)]);
        let answers = answers(&run, &state.ruleset.rules);
        assert!(value(&answers, "Dies on").starts_with("day "), "{answers:?}");
        assert!(answers.last().unwrap().danger);
        assert!(death(&run).is_some());
        let short = crate::lab_timeline::run(&state, &[Step::Pass(24.0, Activity::Walking)]);
        assert_eq!((value(&crate::lab_timeline::answers(&short, &state.ruleset.rules), "Dies on"), death(&short)), ("not reached", None));
    }
}
