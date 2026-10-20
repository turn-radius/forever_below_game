//! The kinds of creature gear can ward against, and later be a bane to.
//!
//! A [`Foe`] indexes the per-kind arrays of [`crate::stats::Stats`] (`ward`, `shun`), so the
//! next per-kind stat is one more array beside them. `fb_creatures::Species::foe` says which
//! kind each creature is.

use serde::{Deserialize, Serialize};

/// A kind of creature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Foe {
    /// Rats, bats and centipedes.
    Vermin,
    Undead,
    Spider,
    Serpent,
    /// Jackalopes.
    Beast,
    Troll,
}

impl Foe {
    pub const COUNT: usize = 6;

    pub const ALL: [Foe; Foe::COUNT] = [Foe::Vermin, Foe::Undead, Foe::Spider, Foe::Serpent, Foe::Beast, Foe::Troll];

    /// Its place in the per-kind arrays.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The kind as the card names it: "Undead", "Vermin".
    pub fn label(self) -> &'static str {
        match self {
            Foe::Vermin => "Vermin",
            Foe::Undead => "Undead",
            Foe::Spider => "Spiders",
            Foe::Serpent => "Serpents",
            Foe::Beast => "Beasts",
            Foe::Troll => "Trolls",
        }
    }
}

/// The most damage gear takes off a kind's blows: 75%, our own, so no ward makes a kind harmless.
pub const MAX_WARD: f32 = 0.75;

/// The largest chance gear gives that a kind never notices the delver: 90%, our own.
pub const MAX_SHUN: f32 = 0.9;

/// Two wards against one kind together: their fractions added, at most [`MAX_WARD`].
pub fn combine_ward(a: f32, b: f32) -> f32 {
    (a + b).clamp(0.0, MAX_WARD)
}

/// Two shun chances together: either one's roll can pass, so `1 - (1 - a)(1 - b)`, at most
/// [`MAX_SHUN`].
pub fn combine_shun(a: f32, b: f32) -> f32 {
    (1.0 - (1.0 - a) * (1.0 - b)).clamp(0.0, MAX_SHUN)
}

/// `damage` with a `ward` fraction taken off, rounded; a blow that lands for at least 1 still
/// does 1, and no blow becomes none.
pub fn warded_damage(damage: u32, ward: f32) -> u32 {
    if damage == 0 || ward <= 0.0 {
        return damage;
    }
    ((damage as f32 * (1.0 - ward.min(MAX_WARD))).round() as u32).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_are_in_order() {
        for (i, foe) in Foe::ALL.into_iter().enumerate() {
            assert_eq!(foe.index(), i);
        }
    }

    #[test]
    fn a_ward_rounds_and_keeps_one() {
        assert_eq!(warded_damage(6, 0.25), 5);
        assert_eq!(warded_damage(1, 0.25), 1);
        assert_eq!(warded_damage(0, 0.25), 0);
        assert_eq!(warded_damage(6, 0.0), 6);
        assert_eq!(warded_damage(4, 0.75), 1);
    }

    #[test]
    fn wards_add_and_shuns_combine() {
        assert_eq!(combine_ward(0.5, 0.5), MAX_WARD);
        assert!((combine_shun(0.5, 0.35) - 0.675).abs() < 1e-6);
        assert_eq!(combine_shun(0.0, 0.0), 0.0);
    }
}
