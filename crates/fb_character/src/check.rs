//! A check: one die rolled with named modifiers against a target.
//!
//! The modifiers come from the delver's stats, class, item affixes and faction standing, each
//! carried with its name and where it came from. [`Check::roll`] draws one die from the seeded
//! RNG and returns a [`CheckRoll`]: the natural face, the total as an [`Explained`] (so a tooltip
//! prints it like any other roll) and whether it met the target. The screen draws the same
//! breakdown, so what the player sees is what was rolled.

use crate::explain::Explained;
use procgen_rng::Mulberry32;

/// Where a modifier comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Stat,
    Class,
    Affix,
    Standing,
}

/// One named modifier.
#[derive(Clone, Debug, PartialEq)]
pub struct Modifier {
    pub name: String,
    pub amount: i32,
    pub source: Source,
}

/// A check to roll: what it is, the die, the number to meet and the modifiers.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub name: String,
    pub sides: u32,
    pub target: i32,
    pub modifiers: Vec<Modifier>,
}

/// The result of a roll.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckRoll {
    pub name: String,
    pub sides: u32,
    /// The face the die landed on.
    pub natural: i32,
    pub target: i32,
    /// Modifiers that applied, in order. Zero modifiers are left out.
    pub modifiers: Vec<Modifier>,
    pub total: i32,
    pub succeeded: bool,
    /// The die plus every modifier, as terms.
    pub roll: Explained,
}

impl Check {
    /// A check of one die against `target`, with no modifiers yet.
    pub fn new(name: impl Into<String>, sides: u32, target: i32) -> Self {
        Check { name: name.into(), sides: sides.max(2), target, modifiers: Vec::new() }
    }

    /// A d20 check.
    pub fn d20(name: impl Into<String>, target: i32) -> Self {
        Self::new(name, 20, target)
    }

    pub fn with(mut self, source: Source, name: impl Into<String>, amount: i32) -> Self {
        self.modifiers.push(Modifier { name: name.into(), amount, source });
        self
    }

    /// A stat's modifier, named for the stat: `CHA`, `WIS`.
    pub fn stat(self, name: impl Into<String>, amount: i32) -> Self {
        self.with(Source::Stat, name, amount)
    }

    pub fn class(self, name: impl Into<String>, amount: i32) -> Self {
        self.with(Source::Class, name, amount)
    }

    /// An item affix's bonus, named for the item or affix.
    pub fn affix(self, name: impl Into<String>, amount: i32) -> Self {
        self.with(Source::Affix, name, amount)
    }

    /// Faction standing.
    pub fn standing(self, amount: i32) -> Self {
        self.with(Source::Standing, "standing", amount)
    }

    /// Rolls the die once and totals it against the target. Meeting the target succeeds.
    pub fn roll(&self, rng: &mut Mulberry32) -> CheckRoll {
        let natural = rng.int(1, self.sides) as i32;
        self.with_natural(natural)
    }

    /// The check with a given face, for tests and replays.
    pub fn with_natural(&self, natural: i32) -> CheckRoll {
        let mut roll = Explained::new(self.name.clone(), "").base(format!("d{}", self.sides), natural as f64);
        let mut modifiers = Vec::new();
        for m in self.modifiers.iter().filter(|m| m.amount != 0) {
            roll = roll.add(m.name.clone(), m.amount as f64);
            modifiers.push(m.clone());
        }
        let total = roll.value() as i32;
        CheckRoll {
            name: self.name.clone(),
            sides: self.sides,
            natural,
            target: self.target,
            modifiers,
            total,
            succeeded: total >= self.target,
            roll,
        }
    }
}

impl CheckRoll {
    /// The die and its modifiers as one line: `Persuade 14 vs 15 = d20 (12) + CHA (+2)`.
    pub fn line(&self) -> String {
        format!("{} {} vs {} = {}", self.name, self.total, self.target, self.roll.terms_line())
    }

    /// How far over the target the total is; negative when it fell short.
    pub fn margin(&self) -> i32 {
        self.total - self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn persuade() -> Check {
        Check::d20("Persuade", 15).stat("CHA", 2).standing(1)
    }

    #[test]
    fn succeeds_on_twelve_or_more() {
        assert!(!persuade().with_natural(11).succeeded);
        assert!(persuade().with_natural(12).succeeded);
        assert_eq!(persuade().with_natural(12).total, 15);
    }

    #[test]
    fn breakdown_names_every_modifier() {
        let r = Check::d20("Identify", 12).stat("INT", 1).class("Mage", 2).affix("Ring of Sight", 3).standing(-1).with_natural(5);
        let names: Vec<_> = r.modifiers.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["INT", "Mage", "Ring of Sight", "standing"]);
        let line = r.line();
        for n in names {
            assert!(line.contains(n), "{line}");
        }
        assert_eq!(r.total, 10);
    }

    #[test]
    fn zero_modifiers_are_left_out() {
        assert_eq!(Check::d20("x", 10).stat("STR", 0).with_natural(3).modifiers.len(), 0);
    }

    #[test]
    fn same_seed_rolls_the_same() {
        let a = persuade().roll(&mut Mulberry32::new(7));
        let b = persuade().roll(&mut Mulberry32::new(7));
        assert_eq!(a, b);
        assert!((1..=20).contains(&a.natural));
    }
}
