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

use crate::rules::{Rules, defaults};
use crate::{Activity, CarriedLight, Gear, MeterKind, Worn, capacity_lb_with, modifier};
use fb_items::Foe;

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
        format!("{} {}{unit} = {}", self.name, number(self.value), self.terms_line()).trim_end().to_string()
    }

    /// The terms alone, as [`Explained::line`] prints them after the `=`: `d20 (14) + STR (+2)`.
    pub fn terms_line(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut best = Vec::new();
        for term in &self.terms {
            match term.op {
                Op::Base => parts.push(format!("{} ({})", term.label, number(term.amount))),
                Op::Add => parts.push(format!("+ {} ({}{})", term.label, if term.amount < 0.0 { "" } else { "+" }, number(term.amount))),
                Op::Multiply => parts.push(format!("× {} ({})", term.label, number(term.amount))),
                Op::BestOf => best.push(format!("{} {}", term.label, number(term.amount))),
                Op::Clamp { min, max } if max.is_infinite() => parts.push(format!("at least {}", number(min))),
                Op::Clamp { min, max } => parts.push(format!("held {} within {} to {}", term.label, number(min), number(max))),
            }
        }
        if !best.is_empty() {
            let mut text = format!("best of {}", best.join(", "));
            if let Some(w) = self.winner() {
                text.push_str(&format!(" ({})", w.label));
            }
            parts.push(text);
        }
        parts.join(" ")
    }
}

impl fmt::Display for Explained {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.line())
    }
}

/// A number as short as it can be: no trailing zeros, at most four places.
pub fn number(n: f64) -> String {
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
        self.capacity_with(defaults(), strength)
    }

    /// [`Loadout::capacity`] under `rules`.
    pub fn capacity_with(&self, rules: &Rules, strength: i32) -> Explained {
        let per = rules.load.capacity_per_strength;
        let mut e = Explained::new("Capacity", "lb").base(format!("STR {strength} × {}", number(per)), capacity_lb_with(rules, strength, 0.0));
        for (label, item) in self.worn.iter().filter(|(_, w)| w.carry != 0.0) {
            e = e.add(label, item.carry);
        }
        e
    }

    /// The change per game hour of the `kind` meter at `activity`, with the items that slow it.
    /// Equals [`crate::Meter::effective_rate`] of a meter given this gear, to rounding: the
    /// items multiply one at a time here and together there.
    pub fn rate(&self, kind: MeterKind, activity: Activity) -> Explained {
        self.rate_with(defaults(), kind, activity)
    }

    /// [`Loadout::rate`] under `rules`.
    pub fn rate_with(&self, rules: &Rules, kind: MeterKind, activity: Activity) -> Explained {
        let (name, hours) = match kind {
            MeterKind::Hunger => ("Hunger rate", rules.needs.hunger_hours_to_full),
            MeterKind::Thirst => ("Thirst rate", rules.needs.thirst_hours_to_full),
            MeterKind::Sleepiness => ("Sleepiness rate", rules.needs.sleepiness_hours_to_full),
        };
        let e = Explained::new(name, "per hour");
        if let (Activity::Sleeping, MeterKind::Sleepiness) = (activity, kind) {
            let recover = rules.needs.sleep_hours_to_rested;
            return e.base(format!("sleeping recovers {} per hour", per_hour(recover)), -(1.0 / recover));
        }
        let mut e = e.base(format!("{} per hour", per_hour(hours)), rules.needs.base_rate(kind));
        let (word, multiplier) = match activity {
            Activity::Sleeping => ("sleeping", rules.needs.sleeping_rate),
            _ => (activity_word(activity), activity.multiplier_with(rules) as f64),
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

    /// Hit points regained on a kill: the sum of what each item gives. Equals
    /// [`Gear::heal_on_kill`].
    pub fn heal_on_kill(&self) -> Explained {
        let mut e = Explained::new("Heal on a kill", "HP").base("nothing", 0.0);
        for (label, item) in self.worn.iter().filter(|(_, w)| w.heal_on_kill != 0.0) {
            e = e.add(label, item.heal_on_kill);
        }
        e
    }

    /// Pace as a multiple of the usual: each item's speed multiplied. Equals [`Gear::speed`].
    pub fn speed(&self) -> Explained {
        let mut e = Explained::new("Speed", "").base("usual pace", 1.0);
        for (label, item) in self.worn.iter().filter(|(_, w)| w.speed != 1.0) {
            e = e.times(label, item.speed);
        }
        e
    }

    /// How fast a swing, a draw and a reload go, as a multiple of the usual: each item's rate
    /// multiplied. Equals [`Gear::attack_rate`].
    pub fn attack_rate(&self) -> Explained {
        let mut e = Explained::new("Attack rate", "").base("usual speed", 1.0);
        for (label, item) in self.worn.iter().filter(|(_, w)| w.attack_rate != 1.0) {
            e = e.times(label, item.attack_rate);
        }
        e
    }

    /// How fast stamina refills, as a multiple of the usual: each item's multiplied. Equals
    /// [`Gear::stamina_refill`].
    pub fn stamina_refill(&self) -> Explained {
        let mut e = Explained::new("Stamina refill", "").base("usual rate", 1.0);
        for (label, item) in self.worn.iter().filter(|(_, w)| w.stamina_refill != 1.0) {
            e = e.times(label, item.stamina_refill);
        }
        e
    }

    /// How fast mana refills, as a multiple of the usual: each item's multiplied. Equals
    /// [`Gear::mana_refill`].
    pub fn mana_refill(&self) -> Explained {
        let mut e = Explained::new("Mana refill", "").base("usual rate", 1.0);
        for (label, item) in self.worn.iter().filter(|(_, w)| w.mana_refill != 1.0) {
            e = e.times(label, item.mana_refill);
        }
        e
    }

    /// What swinging, blocking and sprinting cost in stamina, as a multiple of the usual: each
    /// item's multiplied. Equals [`Gear::stamina_cost`].
    pub fn stamina_cost(&self) -> Explained {
        let mut e = Explained::new("Stamina cost", "").base("usual cost", 1.0);
        for (label, item) in self.worn.iter().filter(|(_, w)| w.stamina_cost != 1.0) {
            e = e.times(label, item.stamina_cost);
        }
        e
    }

    /// The chance a held shield turns aside a blow that would hit, the items' added, at most
    /// [`crate::MAX_BLOCK_CHANCE`]. Equals [`Gear::block_chance`].
    pub fn block_chance(&self) -> Explained {
        let mut e = Explained::new("Block chance", "").base("no shield", 0.0);
        for (label, item) in self.worn.iter().filter(|(_, w)| w.block_chance != 0.0) {
            e = e.add(label, item.block_chance);
        }
        if e.value() > crate::MAX_BLOCK_CHANCE { e.clamp("at most 0.9", 0.0, crate::MAX_BLOCK_CHANCE) } else { e }
    }

    /// Points added to the melee attack and the shot roll by worn gear, item by item. Equals
    /// [`Gear::to_hit`].
    pub fn to_hit(&self) -> Explained {
        let mut e = Explained::new("To hit", "").base("no gear", 0.0);
        for (label, item) in self.worn.iter().filter(|(_, w)| w.to_hit != 0.0) {
            e = e.add(label, item.to_hit);
        }
        e
    }

    /// Points added to the shot roll only, item by item. Equals [`Gear::ranged_to_hit`].
    pub fn ranged_to_hit(&self) -> Explained {
        let mut e = Explained::new("Ranged to hit", "").base("no gear", 0.0);
        for (label, item) in self.worn.iter().filter(|(_, w)| w.ranged_to_hit != 0.0) {
            e = e.add(label, item.ranged_to_hit);
        }
        e
    }

    /// The fraction of a kind's damage worn gear takes off, item by item, at most
    /// [`fb_items::foe::MAX_WARD`]. Equals [`Gear::ward`] for `foe`.
    pub fn ward(&self, foe: Foe) -> Explained {
        let mut e = Explained::new(format!("Ward against {}", foe.label().to_lowercase()), "").base("no ward", 0.0);
        for (label, item) in self.worn.iter().filter(|(_, w)| w.ward[foe.index()] != 0.0) {
            e = e.add(label, item.ward[foe.index()]);
        }
        let max = fb_items::foe::MAX_WARD as f64;
        if e.value() > max { e.clamp("at most 0.75", 0.0, max) } else { e }
    }

    /// The chance a kind never notices the delver, as a line: each item's chance, then the whole
    /// (the chances combined, so two charms of 50% and 35% give 67.5%). Equals [`Gear::shun`]
    /// for `foe`.
    pub fn shun_line(&self, foe: Foe) -> String {
        let parts: Vec<String> = self.worn.iter().filter(|(_, w)| w.shun[foe.index()] != 0.0).map(|(label, w)| format!("{label} {}%", number(w.shun[foe.index()] * 100.0))).collect();
        let whole = self.gear().shun[foe.index()];
        format!("{} that never notice you: {}% ({})", foe.label(), number(whole * 100.0), parts.join(", "))
    }

    /// Points of luck, the items' added: each rolls the loot found a level deeper. Equals
    /// [`Gear::luck`].
    pub fn luck(&self) -> Explained {
        let mut e = Explained::new("Luck", "").base("no luck", 0.0);
        for (label, item) in self.worn.iter().filter(|(_, w)| w.luck != 0.0) {
            e = e.add(label, item.luck);
        }
        e
    }

    /// What worn items add to the ability score named `ability` ("STR", "DEX", "CON", "INT" or
    /// "WIS"), item by item. Equals that score's part of [`Gear::abilities`].
    pub fn ability(&self, ability: &str) -> Explained {
        let mut e = Explained::new(format!("{ability} from gear"), "").base("nothing worn", 0.0);
        for (label, item) in &self.worn {
            let by = item.abilities.named().into_iter().find(|(name, _)| *name == ability).map_or(0, |(_, by)| by);
            if by != 0 {
                e = e.add(label, by as f64);
            }
        }
        e
    }

    /// Warmth: the sum of what each worn item gives.
    pub fn warmth(&self) -> Explained {
        let mut e = Explained::new("Warmth", "");
        for (label, item) in self.worn.iter().filter(|(_, w)| w.warmth != 0.0) {
            e = if e.terms().is_empty() { e.base(label, item.warmth) } else { e.add(label, item.warmth) };
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

/// A rate of one in `hours`, as a designer reads it: `1/16` for a whole number of hours.
fn per_hour(hours: f64) -> String {
    if hours.fract() == 0.0 { format!("1/{}", hours as i64) } else { format!("1/{}", number(hours)) }
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
    armour_class_with(defaults(), dexterity, items)
}

/// [`armour_class`] under `rules`, which set the base.
pub fn armour_class_with<S: Into<String>>(rules: &Rules, dexterity: i32, items: impl IntoIterator<Item = (S, f32)>) -> Explained {
    let base = rules.combat.armour_base;
    let mut e = Explained::new("Armour class", "").base("base", base as f64).add(format!("DEX {dexterity}"), modifier(dexterity) as f64);
    let mut sum = 0.0_f32;
    for (label, armour) in items {
        sum += armour;
        e = e.add(label, armour as f64);
    }
    let whole = (base + sum.round() as i32 + modifier(dexterity)) as f64;
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
    fn to_hit_and_ranged_to_hit_add_item_by_item() {
        let hawk = Worn { ranged_to_hit: 2.0, ..Worn::default() };
        let steady = Worn { to_hit: 1.0, ..Worn::default() };
        let loadout = Loadout::new("Human", 0.0).wearing("Ring of the Hawk", hawk).wearing("Gloves of the Steady Hand", steady);
        assert_eq!(loadout.to_hit().line(), "To hit 1 = no gear (0) + Gloves of the Steady Hand (+1)");
        assert_eq!(loadout.ranged_to_hit().line(), "Ranged to hit 2 = no gear (0) + Ring of the Hawk (+2)");
        let gear = loadout.gear();
        assert_eq!((gear.to_hit, gear.ranged_to_hit), (1.0, 2.0));
    }

    #[test]
    fn block_chance_adds_item_by_item_and_stops_at_90_percent() {
        let block = |by: f64| Worn { block_chance: by, ..Worn::default() };
        let shielded = Loadout::new("Human", 0.0).wearing("Buckler", block(0.3)).wearing("Gloves of Deflection", block(0.1));
        assert_eq!(shielded.gear().block_chance, 0.3 + 0.1);
        assert_eq!(shielded.block_chance().line(), "Block chance 0.4 = no shield (0) + Buckler (+0.3) + Gloves of Deflection (+0.1)");
        let walled = shielded.wearing("Tower shield", block(0.6)).wearing("Charm of the Rampart", block(0.2));
        assert_eq!(walled.gear().block_chance, 0.9);
        assert_eq!(walled.block_chance().value(), 0.9);
    }

    #[test]
    fn attack_rate_multiplies_item_by_item() {
        let quick = Worn { attack_rate: 1.1, ..Worn::default() };
        let duellist = Worn { attack_rate: 1.05, ..Worn::default() };
        let loadout = Loadout::new("Human", 0.0).wearing("Quick Sword", quick).wearing("Gloves of the Duellist", duellist);
        assert_eq!(loadout.attack_rate().line(), "Attack rate 1.155 = usual speed (1) × Quick Sword (1.1) × Gloves of the Duellist (1.05)");
        assert!((loadout.gear().attack_rate - 1.155).abs() < 1e-12);
    }

    #[test]
    fn stamina_refill_and_cost_multiply_item_by_item() {
        let tireless = Worn { stamina_refill: 1.2, ..Worn::default() };
        let wind = Worn { stamina_refill: 1.3, ..Worn::default() };
        let light = Worn { stamina_cost: 0.75, ..Worn::default() };
        let endurance = Worn { stamina_cost: 0.8, ..Worn::default() };
        let loadout = Loadout::new("Human", 0.0).wearing("Tireless Vest", tireless).wearing("Amulet of Second Wind", wind).wearing("Light-handed Sword", light).wearing("Belt of Endurance", endurance);
        assert_eq!(loadout.stamina_refill().line(), "Stamina refill 1.56 = usual rate (1) × Tireless Vest (1.2) × Amulet of Second Wind (1.3)");
        assert_eq!(loadout.stamina_cost().line(), "Stamina cost 0.6 = usual cost (1) × Light-handed Sword (0.75) × Belt of Endurance (0.8)");
        let gear = loadout.gear();
        assert!((gear.stamina_refill - 1.56).abs() < 1e-12 && (gear.stamina_cost - 0.6).abs() < 1e-12);
    }

    #[test]
    fn mana_refill_multiplies_item_by_item() {
        let attuned = Worn { mana_refill: 1.25, ..Worn::default() };
        let wellspring = Worn { mana_refill: 1.2, ..Worn::default() };
        let loadout = Loadout::new("Human", 0.0).wearing("Attuned Helm", attuned).wearing("Ring of the Wellspring", wellspring);
        assert_eq!(loadout.mana_refill().line(), "Mana refill 1.5 = usual rate (1) × Attuned Helm (1.25) × Ring of the Wellspring (1.2)");
        assert!((loadout.gear().mana_refill - 1.5).abs() < 1e-12);
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
        let worn = [("A", Worn { carry: 10.0, darkvision: 30.0, hunger_rate: 0.85, sleepiness_rate: 0.9, stealth: 0.0, warmth: 2.0, heal_on_kill: 1.0, speed: 1.1, block_chance: 0.1, abilities: crate::AbilityBonus { strength: 2, ..Default::default() }, disease_risk: 1.0, luck: 2.0, to_hit: 1.0, ranged_to_hit: 1.0, ..Worn::default() }), ("B", Worn { carry: 5.0, darkvision: 70.0, hunger_rate: 0.85, sleepiness_rate: 1.0, stealth: 0.0, warmth: 1.5, luck: 1.0, ranged_to_hit: 2.0, ..Worn::default() })];
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
            assert_eq!(loadout.heal_on_kill().value(), gear.heal_on_kill);
            assert!((loadout.speed().value() - gear.speed).abs() < 1e-12);
            assert_eq!(loadout.block_chance().value(), gear.block_chance);
            assert_eq!(loadout.luck().value(), gear.luck);
            assert_eq!(loadout.to_hit().value(), gear.to_hit);
            assert_eq!(loadout.ranged_to_hit().value(), gear.ranged_to_hit);
            assert!((loadout.attack_rate().value() - gear.attack_rate).abs() < 1e-12);
            assert!((loadout.stamina_refill().value() - gear.stamina_refill).abs() < 1e-12);
            assert!((loadout.mana_refill().value() - gear.mana_refill).abs() < 1e-12);
            assert!((loadout.stamina_cost().value() - gear.stamina_cost).abs() < 1e-12);
            assert_eq!(loadout.ability("STR").value(), gear.abilities.strength as f64);
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
    fn warmth_is_the_sum_of_what_is_worn() {
        let bare = Loadout::new("Human", 0.0).warmth();
        assert_eq!((bare.value(), bare.line()), (0.0, "Warmth 0 =".to_string()));
        let worn = Loadout::new("Human", 0.0)
            .wearing("Wool cloak", Worn { warmth: 2.0, ..Worn::default() })
            .wearing("Boots", Worn::default())
            .wearing("Warm hood", Worn { warmth: 1.5, ..Worn::default() });
        let e = worn.warmth();
        assert_eq!(e.value(), 3.5);
        assert_eq!(e.line(), "Warmth 3.5 = Wool cloak (2) + Warm hood (+1.5)");
    }

    #[test]
    fn a_clamp_holds_the_value() {
        let e = Explained::new("Roll", "").base("2d6", 14.0).clamp("cap", 0.0, 12.0);
        assert_eq!(e.value(), 12.0);
        assert_eq!(e.terms().last().unwrap().op, Op::Clamp { min: 0.0, max: 12.0 });
    }
}
