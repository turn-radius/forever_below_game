//! Forever Below character attributes: five core stats and their modifiers for gameplay.
//!
//! This crate provides the character's five D&D 5e attributes (Strength, Dexterity, Constitution,
//! Intelligence, Wisdom) and the ability modifier formula. Each attribute starts from a folk's
//! baseline in `data/races.json` and is nudged by a pseudo-random distribution and clamped to 3–18.

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
}
