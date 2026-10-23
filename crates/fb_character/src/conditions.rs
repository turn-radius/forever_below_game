//! The conditions a character is under right now, worked out from what the simulation already
//! tracks: their needs, exhaustion, stamina, load, whether something holds them, and every
//! effect that shows ([`crate::effect`]) ([`conditions`]). Nothing here steps or changes the
//! simulation; it only reads it, so the HUD can show what is wrong without opening the inventory.
//!
//! Each condition has a [`Severity`] and a one-line description of what it does, with the
//! numbers taken from the rules that apply it, so the line stays true when the rules change.

use std::collections::BTreeMap;

use crate::effect::{EffectDef, Effects, Influence};
use crate::{Burden, Exhaustion, LoadBand, Meter, MeterKind, Needs, SECOND_WIND, Stamina};

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
    /// Any other effect ([`crate::effect`]): its name says which.
    Effect,
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
    pub load: Burden,
    /// The character's Constitution modifier, which sets how long starving takes to tire them.
    pub con_modifier: i32,
    /// The share of their pace a held character moves at, while something holds them.
    pub held_pace: Option<f64>,
    /// Game minutes a hit point takes to mend ([`crate::recovery`]), while the character is
    /// below their maximum hit points; `None` when whole.
    pub wounded_mending: Option<f64>,
}

impl Default for Circumstances {
    fn default() -> Self {
        Circumstances { needs: Needs::default(), exhaustion: Exhaustion::default(), stamina: Stamina::default(), load: Burden { load_lb: 0.0, capacity_lb: 1.0 }, con_modifier: 0, held_pace: None, wounded_mending: None }
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

/// A walking pace as a percent, rounded so it never reads as more than it is. A whole percent,
/// except where that would round up to 100 for a pace short of full: those get one decimal
/// (99.7%), and never show 100 (99.99% reads 99.9%).
fn pace_percent(pace: f64) -> String {
    let whole = (pace * 100.0).round();
    if whole < 100.0 || pace >= 1.0 {
        return format!("{whole}%");
    }
    let tenth = (pace * 1000.0).round() / 10.0;
    if tenth < 100.0 { format!("{tenth}%") } else { "99.9%".to_string() }
}

/// A sprint drain multiple to two decimals, trailing zeros dropped: 1.5625 is "1.56", 2 is "2".
fn drain_times(drain: f64) -> String {
    let text = format!("{drain:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A load above Light: how heavy it is and how much of the capacity, the edge it nears, and the
/// pace and sprint drain it gives now ([`Burden::walk_multiplier`], [`Burden::sprint_drain`]).
///
/// Pounds and shares are rounded to whole numbers (137.5 lb is "138 lb"); the pace by
/// [`pace_percent`]; the drain by [`drain_times`]. An Overloaded load says how many whole pounds
/// to set down: the overshoot rounded up, since a fraction still roots them.
fn load(burden: Burden) -> Option<Condition> {
    let rules = crate::rules::defaults();
    let band = burden.band_with(rules);
    let lb = |pounds: f64| format!("{} lb", pounds.round());
    let (severity, effect) = match band {
        LoadBand::Light => return None,
        LoadBand::Overloaded => {
            let over = (burden.load_lb - burden.capacity_lb).max(0.0);
            let share = if burden.capacity_lb > 0.0 { format!("{}%", (burden.load_lb / burden.capacity_lb * 100.0).round()) } else { "none".to_string() };
            (Severity::Grave, format!("{}, {share} of your capacity. Set down {} to move.", lb(burden.load_lb), lb(over.ceil().max(1.0))))
        }
        heavier => {
            let (next, edge) = if heavier == LoadBand::Burdened { (LoadBand::Encumbered, rules.load.encumbered_edge(burden.capacity_lb)) } else { (LoadBand::Overloaded, burden.capacity_lb) };
            let severity = if heavier == LoadBand::Burdened { Severity::Mild } else { Severity::Moderate };
            let share = (burden.load_lb / burden.capacity_lb * 100.0).round();
            let pace = pace_percent(burden.walk_multiplier_with(rules) as f64);
            let drain = drain_times(burden.sprint_drain_with(rules));
            (severity, format!("{}, {share}% of your capacity. {} {} {}. Walking at {pace} pace; sprinting tires you {drain}x as fast.", lb(burden.load_lb), next.word(), if next == LoadBand::Overloaded { "past" } else { "from" }, lb(edge)))
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

/// One condition for each effect that shows: its stage's word and severity, what it does now
/// (each influence at the effect's intensity), and how long it has left. Poison keeps its own
/// kind, so the HUD keeps its flask.
fn effect_conditions(effects: &Effects, defs: &BTreeMap<String, EffectDef>) -> Vec<Condition> {
    effects
        .showing(defs)
        .map(|(active, def, _, stage)| {
            let mut lines: Vec<String> = def.influences.iter().filter_map(|influence| influence_line(influence, active.intensity)).collect();
            if let Some(left) = active.hours_left {
                lines.push(format!("wears off in {}", minutes((left * 60.0 - 1e-9).ceil().max(0.0) as u32)));
            }
            if lines.is_empty() && !def.note.is_empty() {
                lines.push(def.note.clone());
            }
            let mut effect = lines.join("; ");
            if let Some(first) = effect.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            Condition { kind: if active.id == "poisoned" { ConditionKind::Poisoned } else { ConditionKind::Effect }, name: stage.word.clone(), severity: stage.severity, effect }
        })
        .collect()
}

/// What `influence` does at `intensity`, in a few words, or `None` for one another condition
/// already tells (mending stopped shows as no Recovering).
fn influence_line(influence: &Influence, intensity: f64) -> Option<String> {
    let share = |at_full: f64| 1.0 + (at_full - 1.0) * intensity;
    Some(match influence {
        Influence::AttackDisadvantage { .. } => "Attacking at disadvantage".to_string(),
        Influence::NoRecovery { .. } => return None,
        Influence::NeedRate { meter, at_full } => {
            let need = match meter {
                MeterKind::Hunger => "hunger",
                MeterKind::Thirst => "thirst",
                MeterKind::Sleepiness => "sleepiness",
            };
            format!("{need} rising at {} the rate", percent(share(*at_full)))
        }
        Influence::Sight { at_full } => format!("seeing {} as far", percent(share(*at_full))),
        Influence::Pace { at_full } => format!("moving at {} pace", percent(share(*at_full))),
        Influence::Noticed { at_full } => format!("{} easier to notice", percent(at_full * intensity)),
        Influence::HitPointsPerHour { at_full, .. } => format!("losing {:.1} HP an hour", at_full * intensity),
        Influence::Glare { .. } => return None,
    })
}

/// Recovering, while a wounded character is fed, watered and nothing stops their mending
/// ([`crate::recovery::recovers`]), and how fast.
fn recovering(now: &Circumstances, mending_stopped: bool) -> Option<Condition> {
    let per = now.wounded_mending?;
    crate::recovery::recovers(&now.needs, mending_stopped).then(|| Condition {
        kind: ConditionKind::Recovering,
        name: "Recovering".to_string(),
        severity: Severity::Good,
        effect: format!("+1 HP every {} while fed and watered", minutes(per.round().max(1.0) as u32)),
    })
}

/// The conditions `now` and `effects` (read by `defs`) put a character under, worst first (and in
/// a fixed order among equals, so a HUD row does not shuffle). A fresh character is under none.
pub fn conditions(now: &Circumstances, effects: &Effects, defs: &BTreeMap<String, EffectDef>) -> Vec<Condition> {
    let mut all: Vec<Condition> = [held(now.held_pace), winded(now.stamina), exhaustion(now.exhaustion)].into_iter().flatten().collect();
    all.extend(effect_conditions(effects, defs));
    all.extend(
        [
            need(&now.needs.hunger, now.con_modifier),
            need(&now.needs.thirst, now.con_modifier),
            need(&now.needs.sleepiness, now.con_modifier),
            load(now.load),
            recovering(now, effects.stops_recovery(defs)),
        ]
        .into_iter()
        .flatten(),
    );
    // A stable sort keeps the order above among equals.
    all.sort_by_key(|condition| std::cmp::Reverse(condition.severity));
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::Source;

    /// The conditions with no effects on the character.
    fn conditions(now: &Circumstances) -> Vec<Condition> {
        super::conditions(now, &Effects::default(), &crate::effect::shipped())
    }

    /// Poisoned by a snake's bite, as venom poisons.
    fn bitten() -> Effects {
        let mut effects = Effects::default();
        effects.add(&crate::effect::shipped(), "poisoned", 1.0, None, Source::Bite { by: "Poisonous Snake".into() });
        effects
    }

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
        now.stamina.step(SECOND_WIND * 8.0, crate::Activity::Resting, Burden::default());
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
    fn a_load_above_light_shows_its_band_and_where_it_sits() {
        // STR 11: capacity 165 lb, edges at 55 and 110.
        let at = |lb| conditions(&Circumstances { load: Burden::new(lb, 165.0), ..Circumstances::default() });
        assert_eq!(at(40.0), vec![], "Light shows no load condition");
        assert_eq!(at(55.0), vec![]);
        let line = |lb| {
            let all = at(lb);
            format!("{}: {}", all[0].name, all[0].effect)
        };
        assert_eq!(line(65.0), "Burdened: 65 lb, 39% of your capacity. Encumbered from 110 lb. Walking at 99.7% pace; sprinting tires you 1.01x as fast.");
        assert_eq!(line(137.5), "Encumbered: 138 lb, 83% of your capacity. Overloaded past 165 lb. Walking at 80% pace; sprinting tires you 1.56x as fast.");
        assert_eq!(line(172.0), "Overloaded: 172 lb, 104% of your capacity. Set down 7 lb to move.");
        assert_eq!(at(65.0)[0].severity, Severity::Mild);
        assert_eq!(at(137.5)[0].severity, Severity::Moderate);
        assert_eq!(at(172.0)[0].severity, Severity::Grave);
    }

    #[test]
    fn the_pace_and_drain_round_so_they_never_read_as_better_than_they_are() {
        assert_eq!([0.997107, 0.9999, 0.995, 0.803125, 0.65, 1.0].map(pace_percent), ["99.7%", "99.9%", "99.5%", "80%", "65%", "100%"]);
        assert_eq!([1.0, 1.0082, 1.25, 1.5, 1.5625, 2.0].map(drain_times), ["1", "1.01", "1.25", "1.5", "1.56", "2"]);
        // A fraction over capacity still roots them, so it rounds up to a whole pound to set down.
        let line = |lb| load(Burden::new(lb, 165.0)).unwrap().effect;
        assert_eq!(line(165.2), "165 lb, 100% of your capacity. Set down 1 lb to move.");
    }

    #[test]
    fn poisoned_shows_until_it_wears_off_with_the_minutes_left() {
        let defs = crate::effect::shipped();
        let now = Circumstances::default();
        let mut effects = bitten();
        let shown = &super::conditions(&now, &effects, &defs)[0];
        assert_eq!((shown.kind, shown.name.as_str(), shown.severity), (ConditionKind::Poisoned, "Poisoned", Severity::Moderate));
        assert_eq!(shown.effect, "Attacking at disadvantage; wears off in 60 minutes");
        effects.step(&defs, 0.99, &[]);
        assert_eq!(super::conditions(&now, &effects, &defs)[0].effect, "Attacking at disadvantage; wears off in a minute");
        effects.step(&defs, 0.01, &[]);
        assert_eq!(super::conditions(&now, &effects, &defs), vec![]);
    }

    #[test]
    fn any_effect_shows_its_stage_and_what_it_does_now() {
        let mut defs = crate::effect::shipped();
        defs.insert("stinging".into(), crate::effect::EffectDef {
            name: "Stinging eyes".into(),
            stages: vec![crate::effect::Stage { at: 0.0, word: "Stinging eyes".into(), severity: Severity::Mild }],
            per_hour: -1.0,
            influences: vec![Influence::Sight { at_full: 0.5 }],
            ..crate::effect::EffectDef::default()
        });
        let mut effects = Effects::default();
        effects.add(&defs, "stinging", 0.5, None, Source::Surroundings);
        let shown = super::conditions(&Circumstances::default(), &effects, &defs);
        assert_eq!(shown.len(), 1);
        assert_eq!((shown[0].kind, shown[0].name.as_str(), shown[0].effect.as_str()), (ConditionKind::Effect, "Stinging eyes", "Seeing 75% as far"));
    }

    #[test]
    fn held_shows_the_pace_and_the_worst_comes_first() {
        let now = Circumstances { held_pace: Some(0.25), load: Burden::new(80.0, 165.0), exhaustion: Exhaustion::new(5), ..Circumstances::default() };
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
        let poisoned = super::conditions(&wounded, &bitten(), &crate::effect::shipped());
        assert_eq!(poisoned.iter().map(|c| c.kind).collect::<Vec<_>>(), vec![ConditionKind::Poisoned], "poison stops the mending");
        let burdened = Circumstances { load: Burden::new(80.0, 165.0), ..wounded };
        assert_eq!(kinds(&burdened), vec![ConditionKind::Load, ConditionKind::Recovering], "good news comes last");
    }
}
