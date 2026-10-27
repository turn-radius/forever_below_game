//! Persuading folk: the rules of the Persuade topic, as plain data in and out. Nothing here knows
//! Bevy; [`super`] rolls the check on screen ([`crate::dice`]) and applies the outcome.
//!
//! The delver talks to a folk to win their faction over: a d20 check to raise the faction's
//! standing with them, with modifiers from the delver's Wisdom, their class, the charm of what
//! they wear and how the faction already stands with them ([`standing_bonus`]). The difficulty is
//! [`BASE_DC`], rising one for every [`DC_STANDING_STEP`] points of standing above neutral
//! ([`dc`]): a faction that already likes the delver is harder to move further. A success raises
//! standing by [`gain`], [`MIN_GAIN`] and up to [`MAX_GAIN`] by how far the roll beat the
//! difficulty, so a few successes can carry a faction over a band; a failure costs
//! [`FAIL_COST`]. A folk gets one try per rest. Any folk can be persuaded, a Hostile faction's
//! included: the worse the faction stands, the harder the roll, through the standing modifier
//! ([`standing_bonus`], -4 at the worst).

use fb_character::check::{Check, CheckRoll};
use fb_faction::ClassKind;

/// The roll that wins over a faction standing at neutral.
pub const BASE_DC: i32 = 12;
/// Standing points above neutral for each point the difficulty rises.
pub const DC_STANDING_STEP: i32 = 20;
/// What a failed persuasion costs the folk's faction's standing.
pub const FAIL_COST: i32 = 2;
/// Standing a success gains at the least, and at the most.
pub const MIN_GAIN: i32 = 3;
pub const MAX_GAIN: i32 = 8;
/// Standing points to each point of modifier, so the range of -100 to 100 is -4 to +4.
pub const STANDING_PER_POINT: i32 = 25;

/// The difficulty against a faction at `standing`: [`BASE_DC`], and one more for each
/// [`DC_STANDING_STEP`] above neutral.
pub fn dc(standing: i32) -> i32 {
    BASE_DC + standing.max(0) / DC_STANDING_STEP
}

/// The standing a successful `roll` gains: [`MIN_GAIN`], and one more for every two points it
/// beat the difficulty by, up to [`MAX_GAIN`]. A failed roll gains nothing.
pub fn gain(roll: &CheckRoll) -> i32 {
    if !roll.succeeded {
        return 0;
    }
    (MIN_GAIN + roll.margin() / 2).clamp(MIN_GAIN, MAX_GAIN)
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

/// The check to win over a faction that stands `standing` (-100 to 100) with the
/// delver. `wisdom` is the delver's Wisdom modifier and `charm` the points of charisma their
/// worn gear gives. A faction that stands badly (Wary, Hostile) is harder to move through the
/// negative standing modifier.
pub fn check(name: &str, standing: i32, wisdom: i32, class: ClassKind, charm: i32) -> Check {
    let (class_name, class) = class_bonus(class);
    Check::d20(name, dc(standing)).stat("WIS", wisdom).class(class_name, class).affix("Charm", charm).standing(standing_bonus(standing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_character::check::Source;

    #[test]
    fn a_hostile_faction_can_be_persuaded_at_the_standing_it_costs() {
        let of = |roll: &CheckRoll, s: Source| roll.modifiers.iter().filter(|m| m.source == s).map(|m| m.amount).sum::<i32>();
        let hostile = check("Win over", -80, 0, ClassKind::Fighter, 0).with_natural(12);
        assert_eq!((hostile.target, of(&hostile, Source::Standing), hostile.total, hostile.succeeded), (BASE_DC, -3, 9, false));
        // The same roll lands on a neutral faction, and a good roll still wins a hostile one.
        assert!(check("Win over", 0, 0, ClassKind::Fighter, 0).with_natural(12).succeeded);
        assert!(check("Win over", -80, 0, ClassKind::Fighter, 0).with_natural(15).succeeded);
        let wary = check("Win over", -30, 0, ClassKind::Fighter, 0).with_natural(12);
        assert_eq!(of(&wary, Source::Standing), -1);
    }

    #[test]
    fn standing_class_wisdom_and_charm_are_modifiers() {
        let roll = check("Win over", 60, 2, ClassKind::Cleric, 1).with_natural(10);
        let of = |s: Source| roll.modifiers.iter().filter(|m| m.source == s).map(|m| m.amount).sum::<i32>();
        assert_eq!((of(Source::Stat), of(Source::Class), of(Source::Affix), of(Source::Standing)), (2, 2, 1, 2));
        assert_eq!(roll.total, 17);
        assert_eq!(roll.target, dc(60));
    }

    #[test]
    fn a_faction_that_likes_the_delver_is_harder_to_move() {
        assert_eq!((dc(-50), dc(0), dc(19), dc(20), dc(100)), (12, 12, 12, 13, 17));
    }

    #[test]
    fn the_gain_scales_with_the_margin_and_a_failure_gains_nothing() {
        let at = |face: i32| check("p", 0, 0, ClassKind::Fighter, 0).with_natural(face);
        assert_eq!(gain(&at(11)), 0);
        assert_eq!((gain(&at(12)), gain(&at(13)), gain(&at(14)), gain(&at(18))), (3, 3, 4, 6));
        assert_eq!(gain(&at(20)), 7);
        let strong = check("p", 0, 5, ClassKind::Cleric, 4).with_natural(20);
        assert_eq!(gain(&strong), MAX_GAIN);
    }
}
