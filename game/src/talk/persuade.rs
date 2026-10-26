//! Persuading folk: the rules of the Persuade topic, as plain data in and out. Nothing here knows
//! Bevy; [`super`] rolls the check on screen ([`crate::dice`]) and applies the outcome.
//!
//! A folk is asked for one of two things. If their faction holds a locked door on the level the
//! delver asks to be let through ([`Ask::Pass`]); if not, they are asked for a little something
//! ([`Ask::Favour`]). Either is a d20 check against a difficulty with modifiers from the delver's
//! Wisdom, their class, the charm of what they wear and how the folk's faction stands with them
//! ([`standing_bonus`]). A success gives what was asked, once per folk. A failure costs
//! [`FAIL_COST`] standing with the faction. A folk whose faction is Hostile cannot be persuaded:
//! no roll is made ([`check`] is `None`).

use fb_character::check::Check;
use fb_faction::{Band, ClassKind};

/// What the delver asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// To be let through a door of the folk's faction that is locked to them.
    Pass,
    /// For a few coins.
    Favour,
}

/// The roll to be let through.
pub const DC_PASS: i32 = 14;
/// The roll for a favour: folk part with coin less readily than with a latch.
pub const DC_FAVOUR: i32 = 16;
/// What a failed persuasion costs the folk's faction's standing.
pub const FAIL_COST: i32 = 2;
/// Standing points to each point of modifier, so the range of -100 to 100 is -4 to +4.
pub const STANDING_PER_POINT: i32 = 25;

/// The difficulty of `ask`.
pub fn dc(ask: Ask) -> i32 {
    match ask {
        Ask::Pass => DC_PASS,
        Ask::Favour => DC_FAVOUR,
    }
}

/// The modifier standing with the folk's faction gives: one per [`STANDING_PER_POINT`], toward zero.
pub fn standing_bonus(standing: i32) -> i32 {
    standing / STANDING_PER_POINT
}

/// What a class adds to a persuasion, and what the modifier is called: a cleric's calling is
/// persuading the faithful, a mage's is a smooth tongue. The others have none.
pub fn class_bonus(class: ClassKind) -> (&'static str, i32) {
    match class {
        ClassKind::Cleric => ("Cleric", 2),
        ClassKind::Mage => ("Mage", 1),
        ClassKind::Fighter => ("Fighter", 0),
        ClassKind::Ranger => ("Ranger", 0),
    }
}

/// The check for `ask` of a folk whose faction stands `standing` (-100 to 100) in `band` with
/// the delver. `wisdom` is the delver's Wisdom modifier and `charm` the points of charisma their
/// worn gear gives. `None` when the folk's faction is Hostile: they cannot be persuaded.
pub fn check(name: &str, ask: Ask, band: Band, standing: i32, wisdom: i32, class: ClassKind, charm: i32) -> Option<Check> {
    if band == Band::Hostile {
        return None;
    }
    let (class_name, class) = class_bonus(class);
    Some(Check::d20(name, dc(ask)).stat("WIS", wisdom).class(class_name, class).affix("Charm", charm).standing(standing_bonus(standing)))
}

/// The coin a favour is worth, in copper: a little more the deeper the folk stand.
pub fn favour_copper(depth: u32) -> u64 {
    20 + 10 * depth as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_character::check::Source;

    #[test]
    fn a_hostile_faction_cannot_be_persuaded() {
        assert!(check("Persuade", Ask::Pass, Band::Hostile, -80, 3, ClassKind::Cleric, 2).is_none());
        assert!(check("Persuade", Ask::Pass, Band::Wary, -30, 0, ClassKind::Fighter, 0).is_some());
    }

    #[test]
    fn standing_class_wisdom_and_charm_are_modifiers() {
        let roll = check("Persuade", Ask::Pass, Band::Friendly, 60, 2, ClassKind::Cleric, 1).unwrap().with_natural(10);
        let of = |s: Source| roll.modifiers.iter().filter(|m| m.source == s).map(|m| m.amount).sum::<i32>();
        assert_eq!((of(Source::Stat), of(Source::Class), of(Source::Affix), of(Source::Standing)), (2, 2, 1, 2));
        assert_eq!(roll.total, 17);
        assert!(roll.succeeded);
    }

    #[test]
    fn standing_moves_the_roll_both_ways() {
        assert_eq!(standing_bonus(100), 4);
        assert_eq!(standing_bonus(24), 0);
        assert_eq!(standing_bonus(-30), -1);
        let liked = check("p", Ask::Pass, Band::Allied, 80, 0, ClassKind::Ranger, 0).unwrap().with_natural(11);
        let disliked = check("p", Ask::Pass, Band::Wary, -40, 0, ClassKind::Ranger, 0).unwrap().with_natural(11);
        assert!(liked.succeeded && !disliked.succeeded);
    }

    #[test]
    fn a_favour_asks_more_than_a_pass() {
        assert!(dc(Ask::Favour) > dc(Ask::Pass));
        assert!(favour_copper(5) > favour_copper(1));
    }
}
