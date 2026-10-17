//! The conditions a character is under right now, worked out from what the simulation already
//! tracks: their needs, exhaustion, stamina, load, poison and whether something holds them
//! ([`conditions`]). Nothing here steps or changes the simulation; it only reads it, so the HUD
//! can show what is wrong without opening the inventory.
//!
//! Each condition has a [`Severity`] and a one-line description of what it does, with the
//! numbers taken from the rules that apply it, so the line stays true when the rules change.

use crate::poison::Poisoned;
use crate::{Exhaustion, LoadBand, Meter, MeterKind, Needs, SECOND_WIND, Stamina};

/// How much a condition matters, from a nudge to a danger to the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// It is doing the character good (recovering).
    Good,
    /// Worth knowing; no effect yet.
    Mild,
    /// It is starting to cost something.
    Moderate,
    /// It is costing something now.
    Serious,
    /// It is ending the run, or will soon.
    Grave,
}

/// What kind of condition it is, one per thing the simulation tracks, so a HUD can pick an icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConditionKind {
    Hunger,
    Thirst,
    Sleepiness,
    Exhaustion,
    Winded,
    Load,
    Held,
    Poisoned,
    Recovering,
}

/// One condition the character is under.
#[derive(Clone, Debug, PartialEq)]
pub struct Condition {
    pub kind: ConditionKind,
    /// Its name, as the HUD shows it ("Famished", "Winded").
    pub name: String,
    pub severity: Severity,
    /// What it does, in one line ("Sleep no longer eases exhaustion").
    pub effect: String,
}

/// What the conditions are worked out from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circumstances {
    pub needs: Needs,
    pub exhaustion: Exhaustion,
    pub stamina: Stamina,
    pub load: LoadBand,
    /// The character's Constitution modifier, which sets how long starving takes to tire them.
    pub con_modifier: i32,
    /// The share of their pace a held character moves at, while something holds them.
    pub held_pace: Option<f64>,
    /// Venom still in the blood, if any.
    pub poisoned: Poisoned,
    /// Game minutes a hit point takes to mend ([`crate::recovery`]), while the character is
    /// below their maximum hit points; `None` when whole.
    pub wounded_mending: Option<f64>,
}

impl Default for Circumstances {
    fn default() -> Self {
        Circumstances { needs: Needs::default(), exhaustion: Exhaustion::default(), stamina: Stamina::default(), load: LoadBand::Light, con_modifier: 0, held_pace: None, poisoned: Poisoned::default(), wounded_mending: None }
    }
}

/// A share as a whole percent: 0.85 is "85%".
fn percent(share: f64) -> String {
    format!("{}%", (share * 100.0).round() as i64)
}

/// Days or hours, whichever reads better: 24 is "a day", 12 "12 hours", 72 "3 days".
fn span(hours: f64) -> String {
    if hours == 24.0 {
        "a day".to_string()
    } else if hours >= 24.0 && hours % 24.0 == 0.0 {
        format!("{} days", hours / 24.0)
    } else {
        format!("{hours} hours")
    }
}

/// The band from which a need shows as a condition: once it bites, not at its mildest. Hunger
/// from Hungry (band 2), not Peckish; thirst from Thirsty (band 1), its first band already being
/// thirsty; sleepiness from Drowsy (band 3), the band that means sleepy, not Tired or Weary.
/// Our own, at the user's call.
pub const fn first_felt_band(kind: MeterKind) -> usize {
    match kind {
        MeterKind::Hunger => 2,
        MeterKind::Thirst => 1,
        MeterKind::Sleepiness => 3,
    }
}

/// The condition one need's meter puts the character under, once it reaches its
/// [`first_felt_band`].
///
/// Famished and Dry (band 3) break the run of sleep that removes a level of exhaustion; the worst
/// band adds levels of exhaustion after a while ([`crate::ExhaustionTimes::step`]). Below those
/// the meter has no effect yet, and the line says what is coming.
fn need(meter: &Meter, con_modifier: i32) -> Option<Condition> {
    let band = meter.band();
    if band == 0 || band < first_felt_band(meter.kind) {
        return None;
    }
    let (kind, remedy) = match meter.kind {
        MeterKind::Hunger => (ConditionKind::Hunger, "eat"),
        MeterKind::Thirst => (ConditionKind::Thirst, "drink"),
        MeterKind::Sleepiness => (ConditionKind::Sleepiness, "sleep"),
    };
    // The same thresholds `ExhaustionTimes::step` counts from.
    let (first, every) = match meter.kind {
        MeterKind::Hunger => (((3 + con_modifier).max(1) as f64) * 24.0, 24.0),
        MeterKind::Thirst => (24.0, 12.0),
        MeterKind::Sleepiness => (24.0, 24.0),
    };
    let breaks_sleep = meter.kind != MeterKind::Sleepiness;
    let worst = MeterKind::word(meter.kind, 4);
    let (severity, effect) = match band {
        1 => (Severity::Mild, format!("No effect yet; {remedy} before you are {worst}")),
        2 => (Severity::Moderate, format!("No effect yet; {remedy} before you are {worst}")),
        3 if breaks_sleep => (Severity::Serious, "Sleep no longer eases exhaustion".to_string()),
        3 => (Severity::Serious, format!("{remedy} soon: once {worst}, a level of exhaustion after {}", span(first))),
        _ => (Severity::Grave, format!("A level of exhaustion after {}, then one every {}", span(first), span(every).trim_start_matches("a "))),
    };
    let effect = capitalise(&effect);
    Some(Condition { kind, name: meter.word().to_string(), severity, effect })
}

fn capitalise(line: &str) -> String {
    let mut chars = line.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

/// Exhaustion, at any level above 0, and what its level does as the game applies it
/// ([`Exhaustion::speed_multiplier`], death at 6).
fn exhaustion(exhaustion: Exhaustion) -> Option<Condition> {
    let level = exhaustion.level;
    if level == 0 {
        return None;
    }
    let speed = exhaustion.speed_multiplier() as f64;
    let (severity, effect) = if exhaustion.is_dead() {
        (Severity::Grave, "The run ends".to_string())
    } else if speed <= 0.0 {
        (Severity::Grave, "You cannot move; one more level ends the run".to_string())
    } else if speed < 1.0 {
        (Severity::Serious, format!("Moving at {} pace; level 6 ends the run", percent(speed)))
    } else {
        (Severity::Moderate, "No effect yet; level 2 halves your pace".to_string())
    };
    Some(Condition { kind: ConditionKind::Exhaustion, name: format!("Exhausted {level}"), severity, effect })
}

/// Winded, while the stamina pool has emptied and is not yet back to [`SECOND_WIND`].
fn winded(stamina: Stamina) -> Option<Condition> {
    stamina.winded.then(|| Condition {
        kind: ConditionKind::Winded,
        name: "Winded".to_string(),
        severity: Severity::Serious,
        effect: format!("Cannot sprint or swing hard until stamina is back to {}", percent(SECOND_WIND)),
    })
}

/// A load band above Light, and what it does to walking ([`LoadBand::walk_multiplier`]) and to
/// stamina ([`Stamina::rate_per_second`]).
fn load(band: LoadBand) -> Option<Condition> {
    let (severity, effect) = match band {
        LoadBand::Light => return None,
        LoadBand::Overloaded => (Severity::Grave, "Too heavy to move; set something down".to_string()),
        heavier => {
            let drain = Stamina::rate_per_second(crate::Activity::Exerting, heavier) / Stamina::rate_per_second(crate::Activity::Exerting, LoadBand::Light);
            let severity = if heavier == LoadBand::Burdened { Severity::Mild } else { Severity::Moderate };
            (severity, format!("Walking at {} pace; sprinting tires you {drain}x as fast", percent(heavier.walk_multiplier() as f64)))
        }
    };
    Some(Condition { kind: ConditionKind::Load, name: band.word().to_string(), severity, effect })
}

/// Held, while something holds the character at a share of their pace.
fn held(pace: Option<f64>) -> Option<Condition> {
    pace.map(|pace| Condition {
        kind: ConditionKind::Held,
        name: "Held".to_string(),
        severity: Severity::Serious,
        effect: format!("Moving at {} pace until you strike what holds you", percent(pace)),
    })
}

/// Game minutes, as a span: 1 is "a minute", 52 "52 minutes".
fn minutes(n: u32) -> String {
    if n == 1 { "a minute".to_string() } else { format!("{n} minutes") }
}

/// Poisoned, while venom from a failed save is still working ([`Poisoned`]), and how long it has
/// left. Its attack rolls are thrown at disadvantage ([`Poisoned::attack_odds`]).
fn poisoned(poisoned: Poisoned) -> Option<Condition> {
    poisoned.is_active().then(|| Condition {
        kind: ConditionKind::Poisoned,
        name: "Poisoned".to_string(),
        severity: Severity::Moderate,
        effect: format!("Attacking at disadvantage; wears off in {}", minutes(poisoned.minutes_left())),
    })
}

/// Recovering, while a wounded character is fed, watered and unpoisoned
/// ([`crate::recovery::recovers`]), and how fast.
fn recovering(now: &Circumstances) -> Option<Condition> {
    let per = now.wounded_mending?;
    crate::recovery::recovers(&now.needs, &now.poisoned).then(|| Condition {
        kind: ConditionKind::Recovering,
        name: "Recovering".to_string(),
        severity: Severity::Good,
        effect: format!("+1 HP every {} while fed and watered", minutes(per.round().max(1.0) as u32)),
    })
}

/// The conditions `now` puts a character under, worst first (and in a fixed order among equals,
/// so a HUD row does not shuffle). A fresh character is under none.
pub fn conditions(now: &Circumstances) -> Vec<Condition> {
    let mut all: Vec<Condition> = [
        held(now.held_pace),
        winded(now.stamina),
        exhaustion(now.exhaustion),
        poisoned(now.poisoned),
        need(&now.needs.hunger, now.con_modifier),
        need(&now.needs.thirst, now.con_modifier),
        need(&now.needs.sleepiness, now.con_modifier),
        load(now.load),
        recovering(now),
    ]
    .into_iter()
    .flatten()
    .collect();
    // A stable sort keeps the order above among equals.
    all.sort_by_key(|condition| std::cmp::Reverse(condition.severity));
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(now: &Circumstances) -> Vec<ConditionKind> {
        conditions(now).iter().map(|c| c.kind).collect()
    }

    fn hungry(value: f64) -> Circumstances {
        let mut now = Circumstances::default();
        now.needs.hunger.value = value;
        now
    }

    #[test]
    fn a_fresh_delver_is_under_no_conditions() {
        assert_eq!(conditions(&Circumstances::default()), vec![]);
    }

    #[test]
    fn an_emptied_stamina_pool_is_winded_until_it_is_back_to_its_second_wind() {
        let mut now = Circumstances::default();
        now.stamina.spend(1.0);
        let winded = conditions(&now);
        assert_eq!(winded.iter().map(|c| c.kind).collect::<Vec<_>>(), vec![ConditionKind::Winded]);
        assert_eq!(winded[0].effect, "Cannot sprint or swing hard until stamina is back to 20%");
        now.stamina.step(SECOND_WIND * 8.0, crate::Activity::Resting, LoadBand::Light);
        assert_eq!(kinds(&now), vec![], "standing refills a full pool in 8 seconds");
    }

    #[test]
    fn hunger_shows_from_hungry_and_a_worse_band_is_more_severe() {
        assert_eq!(kinds(&hungry(0.2)), vec![], "Fed");
        assert_eq!(kinds(&hungry(0.3)), vec![], "Peckish does not bite yet");
        let hungry_now = &conditions(&hungry(0.55))[0];
        assert_eq!((hungry_now.kind, hungry_now.name.as_str()), (ConditionKind::Hunger, "Hungry"));
        let famished = &conditions(&hungry(0.8))[0];
        assert_eq!(famished.name, "Famished");
        assert_eq!(famished.effect, "Sleep no longer eases exhaustion");
        assert!(famished.severity > hungry_now.severity);
    }

    #[test]
    fn thirst_shows_from_thirsty_and_sleepiness_from_drowsy() {
        let at = |kind: MeterKind, value: f64| {
            let mut now = Circumstances::default();
            match kind {
                MeterKind::Hunger => now.needs.hunger.value = value,
                MeterKind::Thirst => now.needs.thirst.value = value,
                MeterKind::Sleepiness => now.needs.sleepiness.value = value,
            }
            conditions(&now).into_iter().map(|c| c.name).collect::<Vec<_>>()
        };
        assert_eq!(at(MeterKind::Thirst, 0.1), Vec::<String>::new(), "Quenched");
        assert_eq!(at(MeterKind::Thirst, 0.3), vec!["Thirsty"]);
        assert_eq!(at(MeterKind::Sleepiness, 0.3), Vec::<String>::new(), "Tired");
        assert_eq!(at(MeterKind::Sleepiness, 0.6), Vec::<String>::new(), "Weary");
        assert_eq!(at(MeterKind::Sleepiness, 0.8), vec!["Drowsy"]);
        assert_eq!(at(MeterKind::Sleepiness, 1.0), vec!["Spent"]);
        assert_eq!(at(MeterKind::Hunger, 0.55), vec!["Hungry"]);
    }

    #[test]
    fn starving_says_when_exhaustion_comes_from_the_constitution_modifier() {
        let mut now = hungry(1.0);
        assert_eq!(conditions(&now)[0].effect, "A level of exhaustion after 3 days, then one every day");
        assert_eq!(conditions(&now)[0].severity, Severity::Grave);
        now.con_modifier = -3;
        assert_eq!(conditions(&now)[0].effect, "A level of exhaustion after a day, then one every day", "at least a day");
        let mut parched = Circumstances::default();
        parched.needs.thirst.value = 1.0;
        assert_eq!(conditions(&parched)[0].effect, "A level of exhaustion after a day, then one every 12 hours");
    }

    #[test]
    fn exhaustion_names_its_level_and_what_it_does_to_pace() {
        let at = |level| {
            let now = Circumstances { exhaustion: Exhaustion::new(level), ..Circumstances::default() };
            conditions(&now).first().cloned()
        };
        assert_eq!(at(0), None);
        assert_eq!(at(1).unwrap().severity, Severity::Moderate);
        let two = at(2).unwrap();
        assert_eq!((two.name.as_str(), two.effect.as_str()), ("Exhausted 2", "Moving at 50% pace; level 6 ends the run"));
        assert_eq!(at(5).unwrap().severity, Severity::Grave);
    }

    #[test]
    fn a_load_above_light_shows_its_band_and_its_pace() {
        let at = |load| conditions(&Circumstances { load, ..Circumstances::default() });
        assert_eq!(at(LoadBand::Light), vec![]);
        let burdened = &at(LoadBand::Burdened)[0];
        assert_eq!((burdened.name.as_str(), burdened.effect.as_str()), ("Burdened", "Walking at 85% pace; sprinting tires you 1.5x as fast"));
        assert_eq!(at(LoadBand::Encumbered)[0].effect, "Walking at 65% pace; sprinting tires you 2x as fast");
        assert_eq!(at(LoadBand::Overloaded)[0].severity, Severity::Grave);
    }

    #[test]
    fn poisoned_shows_until_it_wears_off_with_the_minutes_left() {
        let mut now = Circumstances { poisoned: Poisoned::for_hours(crate::poison::VENOM_POISONED_HOURS), ..Circumstances::default() };
        let shown = &conditions(&now)[0];
        assert_eq!((shown.kind, shown.name.as_str(), shown.severity), (ConditionKind::Poisoned, "Poisoned", Severity::Moderate));
        assert_eq!(shown.effect, "Attacking at disadvantage; wears off in 60 minutes");
        now.poisoned.step(0.99);
        assert_eq!(conditions(&now)[0].effect, "Attacking at disadvantage; wears off in a minute");
        now.poisoned.step(0.01);
        assert_eq!(kinds(&now), vec![]);
    }

    #[test]
    fn held_shows_the_pace_and_the_worst_comes_first() {
        let now = Circumstances { held_pace: Some(0.25), load: LoadBand::Burdened, exhaustion: Exhaustion::new(5), ..Circumstances::default() };
        assert_eq!(kinds(&now), vec![ConditionKind::Exhaustion, ConditionKind::Held, ConditionKind::Load]);
        assert_eq!(conditions(&now)[1].effect, "Moving at 25% pace until you strike what holds you");
    }

    #[test]
    fn a_wounded_fed_and_watered_delver_shows_recovering_last() {
        let wounded = Circumstances { wounded_mending: Some(crate::recovery::MINUTES_PER_HIT_POINT), ..Circumstances::default() };
        let shown = conditions(&wounded);
        assert_eq!(shown.len(), 1);
        assert_eq!((shown[0].kind, shown[0].name.as_str(), shown[0].severity), (ConditionKind::Recovering, "Recovering", Severity::Good));
        assert_eq!(shown[0].effect, "+1 HP every 10 minutes while fed and watered");
        assert_eq!(kinds(&Circumstances::default()), vec![], "whole, nothing to mend");
        let mut thirsty = wounded;
        thirsty.needs.thirst.value = 0.3;
        assert_eq!(kinds(&thirsty), vec![ConditionKind::Thirst]);
        let poisoned = Circumstances { poisoned: Poisoned::for_hours(1.0), ..wounded };
        assert_eq!(kinds(&poisoned), vec![ConditionKind::Poisoned]);
        let burdened = Circumstances { load: LoadBand::Burdened, ..wounded };
        assert_eq!(kinds(&burdened), vec![ConditionKind::Load, ConditionKind::Recovering], "good news comes last");
    }
}
