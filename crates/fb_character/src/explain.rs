//! Numbers that say where they come from.
//!
//! An [`Explained`] is a value with the ordered terms that made it, each a label, an amount and
//! how it combined with the terms before it. A lab or a tooltip prints the terms; the game reads
//! the value. The terms are generic (a label and a number), so a dice roll explains the same way
//! with a term per die.
//!
//! [`Loadout`] holds the same gear as [`Gear`] but keeps each item's name, and explains what
//! `Gear` only sums: capacity, the meter rates, darkvision and light. [`armour_class`] is the
//! armour class rule. Each explained value equals the plain function's.
//!
//! Terms apply in the order they are added: a leading `times` with no base multiplies 0, and a
//! `clamp` holds the value as it stands at its position. [`Explained::line`] prints the best-of
//! terms last, after the others. The fields are private and built through the constructor, so
//! a term can gain a field (a target the number is rolled against) without a break.

use std::fmt;

use crate::{Activity, CarriedLight, Gear, MeterKind, Worn, capacity_lb, modifier};

/// How a term combines with the value before it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Op {
    /// Starts the value.
    Base,
    /// Adds its amount.
    Add,
    /// Multiplies by its amount.
    Multiply,
    /// Takes its amount if that is more than the value so far.
    BestOf,
    /// Holds the value within the range; the term's amount is the value after.
    Clamp { min: f64, max: f64 },
}

/// One step in an [`Explained`] value.
#[derive(Clone, Debug, PartialEq)]
pub struct Term {
    pub label: String,
    pub amount: f64,
    pub op: Op,
}

/// A value and the terms that make it up, in the order they apply.
#[derive(Clone, Debug, PartialEq)]
pub struct Explained {
    /// What the number is: "Capacity", "Darkvision".
    pub name: String,
    /// Its unit, shown after the value: "lb", "ft"; empty for none.
    pub unit: String,
    value: f64,
    terms: Vec<Term>,
}

impl Explained {
    /// A value with no terms yet, at 0.
    pub fn new(name: impl Into<String>, unit: impl Into<String>) -> Self {
        Explained { name: name.into(), unit: unit.into(), value: 0.0, terms: Vec::new() }
    }

    /// Starts the value at `amount`.
    pub fn base(mut self, label: impl Into<String>, amount: f64) -> Self {
        self.value = amount;
        self.terms.push(Term { label: label.into(), amount, op: Op::Base });
        self
    }

    /// Adds `amount`.
    pub fn add(mut self, label: impl Into<String>, amount: f64) -> Self {
        self.value += amount;
        self.terms.push(Term { label: label.into(), amount, op: Op::Add });
        self
    }

    /// Multiplies by `factor`.
    pub fn times(mut self, label: impl Into<String>, factor: f64) -> Self {
        self.value *= factor;
        self.terms.push(Term { label: label.into(), amount: factor, op: Op::Multiply });
        self
    }

    /// Takes `amount` if it is more than the value so far. As the first term it starts the
    /// value, the best of one thing being that thing.
    pub fn best_of(mut self, label: impl Into<String>, amount: f64) -> Self {
        self.value = if self.terms.is_empty() { amount } else { self.value.max(amount) };
        self.terms.push(Term { label: label.into(), amount, op: Op::BestOf });
        self
    }

    /// Holds the value within `min` to `max`.
    pub fn clamp(mut self, label: impl Into<String>, min: f64, max: f64) -> Self {
        self.value = self.value.clamp(min, max);
        self.terms.push(Term { label: label.into(), amount: self.value, op: Op::Clamp { min, max } });
        self
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn terms(&self) -> &[Term] {
        &self.terms
    }

    /// For a best-of value, the term that won: the first to offer the most. `None` when no
    /// term is a best-of.
    pub fn winner(&self) -> Option<&Term> {
        let mut best: Option<&Term> = None;
        for term in self.terms.iter().filter(|t| t.op == Op::BestOf) {
            if best.is_none_or(|b| term.amount > b.amount) {
                best = Some(term);
            }
        }
        best
    }

    /// One line: `Capacity 175 lb = STR 10 × 15 (150) + Belt of the Ox (+25)`. A best-of value
    /// reads `Darkvision 60 ft = best of Goblin 60, Belt of the Mole 10 (Goblin)`.
    pub fn line(&self) -> String {
        let unit = if self.unit.is_empty() { String::new() } else { format!(" {}", self.unit) };
        let mut out = format!("{} {}{unit} =", self.name, number(self.value));
        let mut best = Vec::new();
        for term in &self.terms {
            match term.op {
                Op::Base => out.push_str(&format!(" {} ({})", term.label, number(term.amount))),
                Op::Add => out.push_str(&format!(" + {} ({}{})", term.label, if term.amount < 0.0 { "" } else { "+" }, number(term.amount))),
                Op::Multiply => out.push_str(&format!(" × {} ({})", term.label, number(term.amount))),
                Op::BestOf => best.push(format!("{} {}", term.label, number(term.amount))),
                Op::Clamp { min, max } => out.push_str(&format!(" held {} within {} to {}", term.label, number(min), number(max))),
            }
        }
        if !best.is_empty() {
            out.push_str(&format!(" best of {}", best.join(", ")));
            if let Some(w) = self.winner() {
                out.push_str(&format!(" ({})", w.label));
            }
        }
        out
    }
}

impl fmt::Display for Explained {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.line())
    }
}

/// A number as short as it can be: no trailing zeros, at most four places.
fn number(n: f64) -> String {
    let s = format!("{n:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_string() } else { s.to_string() }
}

/// The delver's gear with each item's name, so what [`Gear`] sums can be explained.
#[derive(Clone, Debug, PartialEq)]
pub struct Loadout {
    /// What the folk's darkvision is called: "Goblin".
    pub folk: String,
    pub folk_darkvision: f64,
    pub worn: Vec<(String, Worn)>,
    pub lights: Vec<(String, CarriedLight)>,
}

impl Loadout {
    /// A delver of `folk`, who sees `folk_darkvision` feet in the dark, wearing nothing.
    pub fn new(folk: impl Into<String>, folk_darkvision: f64) -> Self {
        Loadout { folk: folk.into(), folk_darkvision, worn: Vec::new(), lights: Vec::new() }
    }

    /// Wears or holds `item`, named `label`.
    pub fn wearing(mut self, label: impl Into<String>, item: Worn) -> Self {
        self.worn.push((label.into(), item));
        self
    }

    /// Carries `light`, named `label`.
    pub fn carrying(mut self, label: impl Into<String>, light: CarriedLight) -> Self {
        self.lights.push((label.into(), light));
        self
    }

    /// The summed [`Gear`] these explain.
    pub fn gear(&self) -> Gear {
        Gear::new(self.folk_darkvision, self.worn.iter().map(|(_, w)| *w), self.lights.iter().map(|(_, l)| *l))
    }

    /// Capacity in pounds: `STR × 15` plus each item's carry. Equals [`Gear::capacity_lb`].
    pub fn capacity(&self, strength: i32) -> Explained {
        let mut e = Explained::new("Capacity", "lb").base(format!("STR {strength} × 15"), capacity_lb(strength, 0.0));
        for (label, item) in self.worn.iter().filter(|(_, w)| w.carry != 0.0) {
            e = e.add(label, item.carry);
        }
        e
    }

    /// The change per game hour of the `kind` meter at `activity`, with the items that slow it.
    /// Equals [`crate::Meter::effective_rate`] of a meter given this gear, to rounding: the
    /// items multiply one at a time here and together there.
    pub fn rate(&self, kind: MeterKind, activity: Activity) -> Explained {
        let (name, per_hour) = match kind {
            MeterKind::Hunger => ("Hunger rate", "1/16"),
            MeterKind::Thirst => ("Thirst rate", "1/10"),
            MeterKind::Sleepiness => ("Sleepiness rate", "1/32"),
        };
        let e = Explained::new(name, "per hour");
        if let (Activity::Sleeping, MeterKind::Sleepiness) = (activity, kind) {
            return e.base("sleeping recovers 1/8 per hour", -0.125);
        }
        let mut e = e.base(format!("{per_hour} per hour"), kind.base_rate());
        let (word, multiplier) = match activity {
            Activity::Sleeping => ("sleeping", 0.5),
            _ => (activity_word(activity), activity.multiplier() as f64),
        };
        if multiplier != 1.0 {
            e = e.times(word, multiplier);
        }
        for (label, item) in &self.worn {
            let rate = match kind {
                MeterKind::Hunger => item.hunger_rate,
                MeterKind::Sleepiness => item.sleepiness_rate,
                MeterKind::Thirst => 1.0,
            };
            if rate != 1.0 {
                e = e.times(label, rate);
            }
        }
        e
    }

    /// Feet seen without light: the best of the folk's and each item's, naming the winner.
    /// Equals [`Gear::darkvision`].
    pub fn darkvision(&self) -> Explained {
        let mut e = Explained::new("Darkvision", "ft").best_of(&self.folk, self.folk_darkvision);
        for (label, item) in self.worn.iter().filter(|(_, w)| w.darkvision > 0.0) {
            e = e.best_of(label, item.darkvision);
        }
        e
    }

    /// Feet the carried light reaches: the farthest of the lights, 0 with none. Equals
    /// [`Gear::light`].
    pub fn light(&self) -> Explained {
        // `Gear` takes a light only if it reaches past 0, so with none the light is 0.
        let mut e = Explained::new("Light", "ft").best_of("no light", 0.0);
        for (label, light) in &self.lights {
            e = e.best_of(label, light.reach);
        }
        e
    }
}

fn activity_word(activity: Activity) -> &'static str {
    match activity {
        Activity::Resting => "resting",
        Activity::Walking => "walking",
        Activity::Exerting => "exerting",
        Activity::Sleeping => "sleeping",
    }
}

/// Armour class: 10, plus the DEX modifier, plus each worn item's armour (`items` are a name
/// and the armour it gives). The armour is summed in `f32` in order and rounded once, as the
/// game always has, shown as a term of its own when the sum is not whole. The value is whole
/// to within rounding of the sum in `f64`, so read it with `.value().round()`.
pub fn armour_class<S: Into<String>>(dexterity: i32, items: impl IntoIterator<Item = (S, f32)>) -> Explained {
    let mut e = Explained::new("Armour class", "").base("base", 10.0).add(format!("DEX {dexterity}"), modifier(dexterity) as f64);
    let mut sum = 0.0_f32;
    for (label, armour) in items {
        sum += armour;
        e = e.add(label, armour as f64);
    }
    let whole = (10 + sum.round() as i32 + modifier(dexterity)) as f64;
    if e.value() != whole {
        let by = whole - e.value();
        e = e.add("rounded", by);
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Needs;

    fn ox() -> Worn {
        Worn { carry: 25.0, ..Worn::default() }
    }

    fn plenty() -> Worn {
        Worn { hunger_rate: 0.85, ..Worn::default() }
    }

    fn mole() -> Worn {
        Worn { darkvision: 10.0, ..Worn::default() }
    }

    #[test]
    fn a_belt_of_the_ox_explains_capacity_as_150_plus_25() {
        let bare = Loadout::new("Human", 0.0);
        assert_eq!(bare.capacity(10).line(), "Capacity 150 lb = STR 10 × 15 (150)");
        let belted = bare.wearing("Belt of the Ox", ox());
        let e = belted.capacity(10);
        assert_eq!(e.line(), "Capacity 175 lb = STR 10 × 15 (150) + Belt of the Ox (+25)");
        assert_eq!(e.terms().iter().map(|t| t.amount).collect::<Vec<_>>(), [150.0, 25.0]);
        assert_eq!(e.value(), belted.gear().capacity_lb(10));
    }

    #[test]
    fn two_of_plenty_explain_hunger_as_base_times_085_twice() {
        let loadout = Loadout::new("Human", 0.0).wearing("Belt of Plenty", plenty()).wearing("Ring of Plenty", plenty());
        let e = loadout.rate(MeterKind::Hunger, Activity::Walking);
        let amounts: Vec<f64> = e.terms().iter().map(|t| t.amount).collect();
        assert_eq!(amounts, [1.0 / 16.0, 0.85, 0.85]);
        assert!((e.value() - 1.0 / 16.0 * 0.7225).abs() < 1e-12);
        assert_eq!(e.line(), "Hunger rate 0.0452 per hour = 1/16 per hour (0.0625) × Belt of Plenty (0.85) × Ring of Plenty (0.85)");
    }

    #[test]
    fn a_goblin_in_the_mole_sees_the_best_not_the_sum() {
        let e = Loadout::new("Goblin", 60.0).wearing("Belt of the Mole", mole()).darkvision();
        assert_eq!(e.value(), 60.0);
        assert_eq!(e.winner().unwrap().label, "Goblin");
        assert_eq!(e.line(), "Darkvision 60 ft = best of Goblin 60, Belt of the Mole 10 (Goblin)");
        let human = Loadout::new("Human", 0.0).wearing("Belt of the Mole", mole()).darkvision();
        assert_eq!((human.value(), human.winner().unwrap().label.as_str()), (10.0, "Belt of the Mole"));
    }

    #[test]
    fn a_helm_explains_armour_class_as_10_plus_2_plus_3() {
        let e = armour_class(14, [("Helm", 3.0_f32)]);
        assert_eq!(e.value(), 15.0);
        assert_eq!(e.terms().iter().map(|t| t.amount).collect::<Vec<_>>(), [10.0, 2.0, 3.0]);
    }

    #[test]
    fn armour_class_matches_the_old_formula_for_fractional_armour() {
        for dex in [3, 8, 10, 14, 18] {
            for armour in [vec![0.3_f32, 0.3, 0.4], vec![1.5], vec![0.25, 0.25], vec![2.5, 0.5], vec![], vec![3.0], vec![-1.5, 0.7]] {
                let old = 10 + armour.iter().fold(0.0_f32, |a, b| a + b).round() as i32 + modifier(dex);
                let e = armour_class(dex, armour.iter().map(|a| ("item", *a)));
                assert_eq!(e.value().round() as i32, old, "dex {dex} armour {armour:?}");
                assert!((e.value() - old as f64).abs() < 1e-9, "the terms add up: {}", e.line());
            }
        }
    }

    #[test]
    fn a_negative_add_prints_with_its_sign() {
        let e = Explained::new("Armour class", "").base("base", 10.0).add("Cursed helm", -2.0);
        assert_eq!(e.line(), "Armour class 8 = base (10) + Cursed helm (-2)");
    }

    #[test]
    fn explained_values_equal_the_plain_functions() {
        let worn = [("A", Worn { carry: 10.0, darkvision: 30.0, hunger_rate: 0.85, sleepiness_rate: 0.9 }), ("B", Worn { carry: 5.0, darkvision: 70.0, hunger_rate: 0.85, sleepiness_rate: 1.0 })];
        let lights = [("Torch", CarriedLight { reach: 30.0, burn_time: 1.0 }), ("Lantern", CarriedLight { reach: 50.0, burn_time: 2.0 })];
        for with in [0, 1, 2] {
            let mut loadout = Loadout::new("Dwarf", 60.0);
            for (n, w) in worn.iter().take(with) {
                loadout = loadout.wearing(*n, *w);
            }
            for (n, l) in lights.iter().take(with) {
                loadout = loadout.carrying(*n, *l);
            }
            let gear = loadout.gear();
            for strength in [3, 10, 18] {
                assert_eq!(loadout.capacity(strength).value(), gear.capacity_lb(strength));
            }
            assert_eq!(loadout.darkvision().value(), gear.darkvision);
            assert_eq!(loadout.light().value(), gear.light);
            let mut needs = Needs::default();
            gear.apply_to(&mut needs);
            for activity in [Activity::Resting, Activity::Walking, Activity::Exerting, Activity::Sleeping] {
                for (kind, meter) in [(MeterKind::Hunger, needs.hunger), (MeterKind::Thirst, needs.thirst), (MeterKind::Sleepiness, needs.sleepiness)] {
                    let (explained, plain) = (loadout.rate(kind, activity).value(), meter.effective_rate(activity));
                    assert!((explained - plain).abs() < 1e-12, "{kind:?} {activity:?}: {explained} against {plain}");
                }
            }
        }
        assert_eq!(Loadout::new("Human", 0.0).light().value(), Gear::default().light);
    }

    #[test]
    fn a_clamp_holds_the_value() {
        let e = Explained::new("Roll", "").base("2d6", 14.0).clamp("cap", 0.0, 12.0);
        assert_eq!(e.value(), 12.0);
        assert_eq!(e.terms().last().unwrap().op, Op::Clamp { min: 0.0, max: 12.0 });
    }
}
