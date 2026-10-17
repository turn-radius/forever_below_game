//! Effects: things that happen to a character's body for a while, one data-driven type the
//! delver, folk and monsters all share ([#213]). Poison, stinging eyes, a fever, the cold: each
//! is an [`EffectDef`] in `data/effects.json`, and a character carries the ones on them now as
//! [`Effects`].
//!
//! Modelled on Cataclysm's effects ([EFFECTS_JSON.md]): an effect has an intensity from 0 to 1
//! read through named stages, a duration or none, a rate at which its intensity falls (or
//! rises) by itself, an optional body part, and influences on what the simulation reads that
//! scale with its intensity. A cause that keeps acting (a smoky torch held, lice worn, the
//! cold) is an exposure passed to [`Effects::step`], adding to that rate while it lasts.
//!
//! Every rate is constant over a step and intensity is clamped to 0..=1 in the direction it is
//! moving, so stepping once by an hour gives the same effects as stepping 3,600 times by a
//! second, as the needs do ([`crate::Needs::step`]).
//!
//! [#213]: https://github.com/turn-radius/forever_below_game/issues/213
//! [EFFECTS_JSON.md]: https://github.com/CleverRaven/Cataclysm-DDA/blob/master/doc/JSON/EFFECTS_JSON.md

use std::collections::BTreeMap;

use fb_items::Slot;
use serde::{Deserialize, Serialize};

use crate::MeterKind;
use crate::combat::Odds;
use crate::conditions::Severity;

/// Slack for float error when a step lands exactly on an end.
const EPSILON: f64 = 1e-9;

/// One kind of effect, as `data/effects.json` defines it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffectDef {
    /// The name the HUD and the sheet show ("Poisoned", "Stinging eyes").
    pub name: String,
    /// The stages, lowest first: each begins at its `at` intensity.
    pub stages: Vec<Stage>,
    /// Game hours it lasts from its onset; `None` lasts until its intensity falls to 0.
    pub hours: Option<f64>,
    /// How its intensity changes by itself, per game hour: negative fades, positive worsens.
    pub per_hour: f64,
    /// What a cause that keeps acting (a smoky light held, lice worn) adds per game hour while
    /// it acts, on top of `per_hour`: the [`Exposure`] the game passes for it.
    pub exposure_per_hour: f64,
    /// Game hours after onset during which it shows nothing and does nothing (a disease's
    /// incubation).
    pub incubation_hours: f64,
    /// Whether it sits on one body part (named by the equipment slot that covers it).
    pub on_a_part: bool,
    /// What it does while it shows.
    pub influences: Vec<Influence>,
}

impl Default for EffectDef {
    fn default() -> Self {
        EffectDef { name: String::new(), stages: vec![], hours: None, per_hour: 0.0, exposure_per_hour: 0.0, incubation_hours: 0.0, on_a_part: false, influences: vec![] }
    }
}

/// A stage of an effect: from `at` intensity it reads `word`, at `severity`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stage {
    pub at: f64,
    pub word: String,
    pub severity: Severity,
}

/// Something an effect does to what the simulation reads. A share scales with intensity, from
/// no change at intensity 0 to `at_full` at intensity 1; the rest act from `from_stage` on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Influence {
    /// Attack rolls are thrown at disadvantage.
    AttackDisadvantage { #[serde(default)] from_stage: usize },
    /// Hit points do not mend.
    NoRecovery { #[serde(default)] from_stage: usize },
    /// A need's rate is multiplied by a share.
    NeedRate { meter: MeterKind, at_full: f64 },
    /// How far the character sees (light reach and darkvision) is multiplied by a share.
    Sight { at_full: f64 },
    /// Walking pace is multiplied by a share.
    Pace { at_full: f64 },
    /// How easily the character is noticed rises by up to this much (0 to 1).
    Noticed { at_full: f64 },
    /// Hit points lost per game hour, at full intensity.
    HitPointsPerHour { at_full: f64 },
}

/// Where an effect came from, as the sheet tells it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// A creature's bite or blow (its stat block name).
    Bite { by: String },
    /// Something worn or held (the item's name).
    Item { name: String },
    /// The place: the cold, the dark, the light.
    Surroundings,
    /// Something eaten or drunk.
    Consumed { what: String },
}

/// One effect on a character now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Active {
    /// Its id in `data/effects.json`.
    pub id: String,
    /// 0 to 1, read through its stages.
    pub intensity: f64,
    /// Game hours until it ends, for an effect with a duration.
    pub hours_left: Option<f64>,
    /// Game hours since its onset.
    pub age: f64,
    pub part: Option<Slot>,
    pub source: Source,
}

impl Active {
    /// Whether it is past its incubation, so it shows and acts.
    fn shows(&self, def: &EffectDef) -> bool {
        self.age + EPSILON >= def.incubation_hours
    }

    /// The stage it is at, if it shows: the last stage whose `at` it has reached.
    pub fn stage<'a>(&self, def: &'a EffectDef) -> Option<(usize, &'a Stage)> {
        if !self.shows(def) {
            return None;
        }
        def.stages.iter().enumerate().rev().find(|(_, s)| self.intensity + EPSILON >= s.at)
    }
}

/// A cause that keeps acting on a character for the length of a step: it adds `per_hour` to
/// the effect's intensity (onsetting it if absent), on top of the effect's own rate.
#[derive(Clone, Debug, PartialEq)]
pub struct Exposure {
    pub id: String,
    pub per_hour: f64,
    pub part: Option<Slot>,
    pub source: Source,
}

/// An effect's stage changed over a step (or it began or ended).
#[derive(Clone, Debug, PartialEq)]
pub struct StageChange {
    pub id: String,
    pub part: Option<Slot>,
    /// The stage's word before, `None` when it did not show.
    pub was: Option<String>,
    /// The stage's word now, `None` when it no longer shows.
    pub now: Option<String>,
}

/// The effects on one character. Effects end when the character dies: dropping this is all it
/// takes. An effect an item brings is the item's affix, so it stays with the item.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Effects {
    pub active: Vec<Active>,
}

impl Effects {
    fn find_mut(&mut self, id: &str, part: Option<Slot>) -> Option<&mut Active> {
        self.active.iter_mut().find(|a| a.id == id && a.part == part)
    }

    /// The effect `id` (on `part`), if the character has it.
    pub fn get(&self, id: &str, part: Option<Slot>) -> Option<&Active> {
        self.active.iter().find(|a| a.id == id && a.part == part)
    }

    /// Gives the character `id` at `intensity` (on `part`). An effect they already have keeps
    /// the stronger intensity and the longer time left: doses do not add up ("each instance of
    /// the condition has its own duration, but the condition's effects don't get worse", SRD 5.1
    /// p. 358), and the onset keeps its age, so a running incubation is not restarted.
    pub fn add(&mut self, defs: &BTreeMap<String, EffectDef>, id: &str, intensity: f64, part: Option<Slot>, source: Source) {
        let Some(def) = defs.get(id) else { return };
        let part = if def.on_a_part { part } else { None };
        let intensity = intensity.clamp(0.0, 1.0);
        match self.find_mut(id, part) {
            Some(a) => {
                a.intensity = a.intensity.max(intensity);
                if let (Some(left), Some(full)) = (a.hours_left.as_mut(), def.hours) {
                    *left = left.max(full);
                }
            }
            None => self.active.push(Active { id: id.to_string(), intensity, hours_left: def.hours, age: 0.0, part, source }),
        }
    }

    /// Removes `id` (on `part`) at once (a cure, a bandage).
    pub fn remove(&mut self, id: &str, part: Option<Slot>) {
        self.active.retain(|a| !(a.id == id && a.part == part));
    }

    /// Advances every effect by `hours` of game time under `exposures`, and returns the stage
    /// changes. An exposure to an effect the character lacks begins it at intensity 0. An effect
    /// ends when its time runs out, or, without a duration, when its intensity has fallen to 0
    /// with nothing exposing it. Exact for any split of the same hours.
    pub fn step(&mut self, defs: &BTreeMap<String, EffectDef>, hours: f64, exposures: &[Exposure]) -> Vec<StageChange> {
        let hours = hours.max(0.0);
        let before: Vec<(String, Option<Slot>, Option<String>)> = self.active.iter().map(|a| (a.id.clone(), a.part, defs.get(&a.id).and_then(|d| a.stage(d)).map(|(_, s)| s.word.clone()))).collect();
        for e in exposures {
            let Some(def) = defs.get(&e.id) else { continue };
            let part = if def.on_a_part { e.part } else { None };
            if e.per_hour > 0.0 && self.get(&e.id, part).is_none() {
                self.active.push(Active { id: e.id.clone(), intensity: 0.0, hours_left: def.hours, age: 0.0, part, source: e.source.clone() });
            }
        }
        for a in &mut self.active {
            let Some(def) = defs.get(&a.id) else { continue };
            let pushed: f64 = exposures.iter().filter(|e| e.id == a.id && (!def.on_a_part || e.part == a.part)).map(|e| e.per_hour).sum();
            // Incubation holds the intensity where it was onset; the clock still runs.
            let incubating = (def.incubation_hours - a.age).clamp(0.0, hours);
            let rate = def.per_hour + pushed;
            a.intensity = (a.intensity + rate * (hours - incubating)).clamp(0.0, 1.0);
            a.age += hours;
            if let Some(left) = a.hours_left.as_mut() {
                *left = (*left - hours).max(0.0);
            }
        }
        self.active.retain(|a| {
            let Some(def) = defs.get(&a.id) else { return false };
            let exposed = exposures.iter().any(|e| e.id == a.id && e.per_hour > 0.0 && (!def.on_a_part || e.part == a.part));
            let timed_out = a.hours_left.is_some_and(|left| left <= EPSILON);
            let faded = def.hours.is_none() && a.intensity <= EPSILON && !exposed && a.age > def.incubation_hours;
            !(timed_out || faded)
        });
        let word = |a: &Active| defs.get(&a.id).and_then(|d| a.stage(d)).map(|(_, s)| s.word.clone());
        let mut changes = vec![];
        for (id, part, was) in &before {
            let now = self.get(id, *part).and_then(word);
            if *was != now {
                changes.push(StageChange { id: id.clone(), part: *part, was: was.clone(), now });
            }
        }
        // Effects an exposure began during the step, that already show.
        for a in &self.active {
            if !before.iter().any(|(id, part, _)| *id == a.id && *part == a.part) {
                if let Some(now) = word(a) {
                    changes.push(StageChange { id: a.id.clone(), part: a.part, was: None, now: Some(now) });
                }
            }
        }
        changes
    }

    /// Every effect that shows, with its definition and stage, in the order they began.
    pub fn showing<'a>(&'a self, defs: &'a BTreeMap<String, EffectDef>) -> impl Iterator<Item = (&'a Active, &'a EffectDef, usize, &'a Stage)> + 'a {
        self.active.iter().filter_map(move |a| {
            let def = defs.get(&a.id)?;
            let (i, stage) = a.stage(def)?;
            Some((a, def, i, stage))
        })
    }

    /// Every influence that acts now, with the intensity of the effect it comes from.
    fn acting<'a>(&'a self, defs: &'a BTreeMap<String, EffectDef>) -> impl Iterator<Item = (&'a Influence, f64)> + 'a {
        self.showing(defs).flat_map(|(a, def, stage, _)| {
            def.influences.iter().filter(move |i| match i {
                Influence::AttackDisadvantage { from_stage } | Influence::NoRecovery { from_stage } => stage >= *from_stage,
                _ => true,
            })
            .map(move |i| (i, a.intensity))
        })
    }

    /// The product of every `share` influence picked out by `pick`, each scaled by intensity.
    fn share(&self, defs: &BTreeMap<String, EffectDef>, pick: impl Fn(&Influence) -> Option<f64>) -> f64 {
        self.acting(defs).filter_map(|(i, intensity)| pick(i).map(|at_full| 1.0 + (at_full - 1.0) * intensity)).product()
    }

    /// How attack rolls are thrown: at disadvantage while anything gives it.
    pub fn attack_odds(&self, defs: &BTreeMap<String, EffectDef>) -> Odds {
        if self.acting(defs).any(|(i, _)| matches!(i, Influence::AttackDisadvantage { .. })) { Odds::Disadvantage } else { Odds::Even }
    }

    /// Whether anything stops hit points mending.
    pub fn stops_recovery(&self, defs: &BTreeMap<String, EffectDef>) -> bool {
        self.acting(defs).any(|(i, _)| matches!(i, Influence::NoRecovery { .. }))
    }

    /// What the effects multiply `meter`'s rate by.
    pub fn need_rate(&self, defs: &BTreeMap<String, EffectDef>, meter: MeterKind) -> f64 {
        self.share(defs, |i| match i {
            Influence::NeedRate { meter: m, at_full } if *m == meter => Some(*at_full),
            _ => None,
        })
    }

    /// What the effects multiply how far the character sees by.
    pub fn sight(&self, defs: &BTreeMap<String, EffectDef>) -> f64 {
        self.share(defs, |i| match i {
            Influence::Sight { at_full } => Some(*at_full),
            _ => None,
        })
    }

    /// What the effects multiply walking pace by.
    pub fn pace(&self, defs: &BTreeMap<String, EffectDef>) -> f64 {
        self.share(defs, |i| match i {
            Influence::Pace { at_full } => Some(*at_full),
            _ => None,
        })
    }

    /// How much the effects add to how easily the character is noticed, 0 to 1.
    pub fn noticed(&self, defs: &BTreeMap<String, EffectDef>) -> f64 {
        self.acting(defs).filter_map(|(i, intensity)| match i {
            Influence::Noticed { at_full } => Some(at_full * intensity),
            _ => None,
        }).sum::<f64>().min(1.0)
    }

    /// Hit points the effects take per game hour now.
    pub fn hit_points_per_hour(&self, defs: &BTreeMap<String, EffectDef>) -> f64 {
        self.acting(defs).filter_map(|(i, intensity)| match i {
            Influence::HitPointsPerHour { at_full } => Some(at_full * intensity),
            _ => None,
        }).sum()
    }
}

/// The effects `data/effects.json` defines, by id: what [`crate::rules::Rules::default`] holds.
pub fn shipped() -> BTreeMap<String, EffectDef> {
    serde_json::from_str(include_str!("../../../data/effects.json")).expect("data/effects.json parses")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A catalog that exercises every kind of rate: a timed dose, one that fades by itself, one
    /// that worsens after an incubation, and one on a body part.
    fn defs() -> BTreeMap<String, EffectDef> {
        let stage = |at: f64, word: &str, severity: Severity| Stage { at, word: word.into(), severity };
        let mut d = shipped();
        d.insert("stinging".into(), EffectDef {
            name: "Stinging eyes".into(),
            stages: vec![stage(0.0, "Smarting", Severity::Mild), stage(0.5, "Stinging", Severity::Moderate)],
            per_hour: -0.5,
            influences: vec![Influence::Sight { at_full: 0.5 }],
            ..EffectDef::default()
        });
        d.insert("plague".into(), EffectDef {
            name: "Sewer plague".into(),
            stages: vec![stage(0.0, "Feverish", Severity::Moderate), stage(0.6, "Wasting", Severity::Serious)],
            hours: Some(72.0),
            per_hour: 1.0 / 48.0,
            incubation_hours: 12.0,
            influences: vec![Influence::NeedRate { meter: MeterKind::Sleepiness, at_full: 2.0 }, Influence::NoRecovery { from_stage: 1 }],
            ..EffectDef::default()
        });
        d.insert("cold".into(), EffectDef {
            name: "Cold".into(),
            stages: vec![stage(0.0, "Chilly", Severity::Mild), stage(0.34, "Cold", Severity::Moderate), stage(0.67, "Freezing", Severity::Serious)],
            per_hour: -0.25,
            on_a_part: true,
            influences: vec![Influence::HitPointsPerHour { at_full: 2.0 }],
            ..EffectDef::default()
        });
        d
    }

    fn bitten() -> Source {
        Source::Bite { by: "Giant Rat".into() }
    }

    fn start() -> Effects {
        let d = defs();
        let mut e = Effects::default();
        e.add(&d, "poisoned", 1.0, None, Source::Bite { by: "Poisonous Snake".into() });
        e.add(&d, "stinging", 0.9, None, Source::Item { name: "Smoky Torch".into() });
        e.add(&d, "plague", 0.1, None, bitten());
        e.add(&d, "cold", 0.2, Some(Slot::Feet), Source::Surroundings);
        e
    }

    fn exposures() -> Vec<Exposure> {
        vec![Exposure { id: "cold".into(), per_hour: 0.4, part: Some(Slot::Feet), source: Source::Surroundings }]
    }

    fn word(e: &Effects, d: &BTreeMap<String, EffectDef>, id: &str) -> Option<String> {
        e.get(id, None).and_then(|a| a.stage(&d[id])).map(|(_, s)| s.word.clone())
    }

    #[test]
    fn one_big_step_equals_many_small_ones_for_every_effect() {
        let d = defs();
        for (hours, pieces) in [(8.0, 8), (8.0, 480), (30.0, 30), (0.75, 45)] {
            let (mut once, mut split) = (start(), start());
            once.step(&d, hours, &exposures());
            for _ in 0..pieces {
                split.step(&d, hours / pieces as f64, &exposures());
            }
            assert_eq!(once.active.len(), split.active.len(), "{hours} h in {pieces}");
            for (a, b) in once.active.iter().zip(&split.active) {
                assert_eq!((&a.id, a.part), (&b.id, b.part));
                assert!((a.intensity - b.intensity).abs() < 1e-9, "{} after {hours} h in {pieces}: {} vs {}", a.id, a.intensity, b.intensity);
                assert!((a.age - b.age).abs() < 1e-9);
                assert_eq!(a.hours_left.map(|h| (h * 1e6).round()), b.hours_left.map(|h| (h * 1e6).round()));
            }
        }
    }

    #[test]
    fn a_timed_effect_ends_when_its_hours_run_out() {
        let d = defs();
        let mut e = start();
        e.step(&d, 0.99, &[]);
        assert!(e.get("poisoned", None).is_some());
        let changes = e.step(&d, 0.01, &[]);
        assert!(e.get("poisoned", None).is_none());
        assert!(changes.iter().any(|c| c.id == "poisoned" && c.was.as_deref() == Some("Poisoned") && c.now.is_none()), "{changes:?}");
    }

    #[test]
    fn a_dose_keeps_the_stronger_intensity_and_the_longer_time() {
        let d = defs();
        let mut e = Effects::default();
        e.add(&d, "poisoned", 1.0, None, bitten());
        e.step(&d, 0.5, &[]);
        e.add(&d, "poisoned", 0.3, None, bitten());
        let p = e.get("poisoned", None).unwrap();
        assert_eq!((p.intensity, p.hours_left), (1.0, Some(1.0)));
        assert_eq!(e.active.len(), 1);
    }

    #[test]
    fn an_effect_without_a_duration_fades_away_by_itself() {
        let d = defs();
        let mut e = Effects::default();
        e.add(&d, "stinging", 0.9, None, Source::Surroundings);
        assert_eq!(word(&e, &d, "stinging").as_deref(), Some("Stinging"));
        let changes = e.step(&d, 1.0, &[]);
        assert_eq!(word(&e, &d, "stinging").as_deref(), Some("Smarting"));
        assert!(changes.iter().any(|c| c.was.as_deref() == Some("Stinging") && c.now.as_deref() == Some("Smarting")));
        e.step(&d, 1.0, &[]);
        assert!(e.get("stinging", None).is_none(), "faded out");
    }

    #[test]
    fn an_incubating_effect_shows_and_does_nothing_until_it_has_run_its_course() {
        let d = defs();
        let mut e = Effects::default();
        e.add(&d, "plague", 0.1, None, bitten());
        assert!(e.showing(&d).next().is_none());
        assert_eq!(e.need_rate(&d, MeterKind::Sleepiness), 1.0);
        e.step(&d, 11.0, &[]);
        assert!(e.showing(&d).next().is_none(), "still incubating at 11 h");
        assert!((e.get("plague", None).unwrap().intensity - 0.1).abs() < 1e-12, "incubation holds the intensity");
        let changes = e.step(&d, 2.0, &[]);
        assert_eq!(changes.len(), 1, "{changes:?}");
        assert_eq!(changes[0].now.as_deref(), Some("Feverish"));
        assert!(e.need_rate(&d, MeterKind::Sleepiness) > 1.0);
        assert!(!e.stops_recovery(&d), "only from Wasting");
        e.step(&d, 24.0, &[]);
        assert_eq!(word(&e, &d, "plague").as_deref(), Some("Wasting"));
        assert!(e.stops_recovery(&d));
    }

    #[test]
    fn an_exposure_begins_an_effect_and_holds_it_while_it_lasts() {
        let d = defs();
        let mut e = Effects::default();
        let changes = e.step(&d, 1.0, &exposures());
        let cold = e.get("cold", Some(Slot::Feet)).expect("the cold began");
        assert!((cold.intensity - 0.15).abs() < 1e-12, "0.4 exposure less 0.25 fading over an hour: {}", cold.intensity);
        assert_eq!(changes[0].now.as_deref(), Some("Chilly"));
        assert!(e.get("cold", Some(Slot::Hands)).is_none(), "the cold is on the feet");
        e.step(&d, 2.0, &exposures());
        assert_eq!(e.showing(&d).next().unwrap().3.word, "Cold", "0.15 + 2 h at 0.15 is 0.45");
        e.step(&d, 2.0, &exposures());
        assert_eq!(e.showing(&d).next().unwrap().3.word, "Freezing", "0.75");
        e.step(&d, 10.0, &[]);
        assert!(e.get("cold", Some(Slot::Feet)).is_none(), "warm again");
    }

    #[test]
    fn influences_scale_with_intensity_and_stack_by_multiplying() {
        let d = defs();
        let mut e = Effects::default();
        e.add(&d, "stinging", 0.5, None, Source::Surroundings);
        assert!((e.sight(&d) - 0.75).abs() < 1e-12, "halfway to half sight");
        e.add(&d, "poisoned", 1.0, None, bitten());
        assert_eq!(e.attack_odds(&d), Odds::Disadvantage);
        assert!(e.stops_recovery(&d));
        e.add(&d, "cold", 0.5, Some(Slot::Feet), Source::Surroundings);
        e.add(&d, "cold", 0.25, Some(Slot::Hands), Source::Surroundings);
        assert!((e.hit_points_per_hour(&d) - 1.5).abs() < 1e-12, "two parts add up");
        assert_eq!(Effects::default().attack_odds(&d), Odds::Even);
        assert_eq!(Effects::default().pace(&d), 1.0);
    }

    #[test]
    fn effects_round_trip_through_serde_with_their_stage_and_time_left() {
        let d = defs();
        let mut e = start();
        e.step(&d, 13.0, &exposures());
        let back: Effects = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(back, e);
        let defs_back: BTreeMap<String, EffectDef> = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(defs_back, d);
    }

    #[test]
    fn the_shipped_file_parses_and_poison_lasts_its_hour() {
        let shipped = shipped();
        let poisoned = &shipped["poisoned"];
        assert_eq!((poisoned.name.as_str(), poisoned.hours), ("Poisoned", Some(crate::poison::VENOM_POISONED_HOURS)));
        assert_eq!(crate::rules::Rules::default().effects, shipped);
    }

    #[test]
    fn a_rules_file_changes_only_the_effect_keys_it_names() {
        let rules: crate::rules::Rules = toml::from_str("[effects.poisoned]\nhours = 2.5\n").unwrap();
        assert_eq!(rules.effects["poisoned"].hours, Some(2.5));
        assert_eq!(rules.effects["poisoned"].influences, shipped()["poisoned"].influences);
        let none: crate::rules::Rules = toml::from_str("").unwrap();
        assert_eq!(none.effects, shipped());
        let written = toml::to_string(&crate::rules::Rules::default()).unwrap();
        assert!(!written.contains("[effects"), "the shipped effects are not written into a rules file");
    }
}
