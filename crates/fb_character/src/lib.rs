//! Forever Below character attributes: five core stats and their modifiers for gameplay, and game time.
//!
//! This crate provides the character's five D&D 5e attributes (Strength, Dexterity, Constitution,
//! Intelligence, Wisdom) and the ability modifier formula. Each attribute starts from a folk's
//! baseline in `data/races.json` and is nudged by a pseudo-random distribution and clamped to 3–18.
//!
//! `GameTime` measures elapsed game seconds. The game clock runs at a configurable ratio to real time
//! (e.g., 12× real time means 1 game hour passes every 300 real seconds), and pauses while screens
//! are open over the level.

use serde::{Deserialize, Serialize};

/// The five core attributes of a character (D&D 5e SRD 5.1, p. 76).
///
/// Each ability score ranges from 3 (incapacitated) to 18 (superhuman). A score of 10 or 11
/// represents human average and gives a modifier of 0.
///
/// The attributes are in order: strength, dexterity, constitution, intelligence, wisdom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attributes {
    /// Strength (STR): physical power, carrying capacity, melee damage.
    pub strength: i32,
    /// Dexterity (DEX): coordination and reflexes; affects Armour Class and initiative.
    pub dexterity: i32,
    /// Constitution (CON): endurance and health; affects hit points and saving throws.
    pub constitution: i32,
    /// Intelligence (INT): reasoning and memory; affects skills and magical abilities.
    pub intelligence: i32,
    /// Wisdom (WIS): awareness and instinct; affects perception and saving throws.
    pub wisdom: i32,
}

/// Game time in seconds. The clock starts at zero for each delve and ticks at a ratio
/// determined by the tuning file's `clock.game_seconds_per_second` setting.
/// At the default ratio of 12, 1 real second is 12 game seconds; 300 real seconds is 1 game hour.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct GameTime {
    /// Elapsed game seconds.
    pub game_seconds: f64,
}

impl GameTime {
    /// Creates a game time from a given number of game seconds.
    pub const fn from_seconds(game_seconds: f64) -> Self {
        GameTime { game_seconds }
    }

    /// Returns the elapsed game time in seconds.
    pub const fn seconds(&self) -> f64 {
        self.game_seconds
    }

    /// Returns the elapsed game time in minutes (60 game seconds per minute).
    pub fn minutes(&self) -> f64 {
        self.game_seconds / 60.0
    }

    /// Returns the elapsed game time in hours (3600 game seconds per hour).
    pub fn hours(&self) -> f64 {
        self.game_seconds / 3600.0
    }

    /// Returns the elapsed game time in days (86400 game seconds per day).
    pub fn days(&self) -> f64 {
        self.game_seconds / 86400.0
    }

    /// Advances the game time by the given number of game seconds.
    pub fn advance(&mut self, delta_seconds: f64) {
        self.game_seconds += delta_seconds;
    }
}

/// The ability modifier for a score: `floor((score - 10) / 2)` (D&D 5e SRD 5.1, p. 76).
///
/// A score of 3 gives -4, a score of 10 or 11 gives 0, and a score of 18 gives +4.
/// Uses `div_euclid` which floors rather than truncating towards zero.
pub fn modifier(score: i32) -> i32 {
    (score - 10).div_euclid(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_follow_the_srd() {
        assert_eq!(modifier(3), -4);
        assert_eq!(modifier(9), -1);
        assert_eq!(modifier(10), 0);
        assert_eq!(modifier(11), 0);
        assert_eq!(modifier(18), 4);
    }

    #[test]
    fn attributes_serde_round_trip() {
        let attrs = Attributes { strength: 15, dexterity: 12, constitution: 14, intelligence: 9, wisdom: 13 };
        let json = serde_json::to_string(&attrs).unwrap();
        let parsed: Attributes = serde_json::from_str(&json).unwrap();
        assert_eq!(attrs, parsed);
    }

    #[test]
    fn game_time_conversions_are_correct() {
        let time = GameTime::from_seconds(3661.0);
        assert_eq!(time.seconds(), 3661.0);
        assert!((time.minutes() - 61.01666).abs() < 0.001);
        assert!((time.hours() - 1.0169).abs() < 0.001);
        assert!((time.days() - 0.04235).abs() < 0.001);
    }

    #[test]
    fn game_time_advances() {
        let mut time = GameTime::from_seconds(100.0);
        time.advance(50.0);
        assert_eq!(time.seconds(), 150.0);
    }

    #[test]
    fn game_time_starts_at_zero() {
        let time = GameTime::default();
        assert_eq!(time.seconds(), 0.0);
    }

    #[test]
    fn game_time_serde_round_trip() {
        let time = GameTime::from_seconds(1234.5);
        let json = serde_json::to_string(&time).unwrap();
        let parsed: GameTime = serde_json::from_str(&json).unwrap();
        assert_eq!(time, parsed);
    }
}
