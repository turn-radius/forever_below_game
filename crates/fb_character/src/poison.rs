//! Poisoned: what a failed save against venom leaves. It is an effect ([`crate::effect`],
//! `poisoned` in `data/effects.json`) that lasts [`VENOM_POISONED_HOURS`] of game time.
//!
//! The SRD's Poisoned condition gives disadvantage on attack rolls and ability checks
//! ([SRD 5.1 p. 358](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=358),
//! Appendix PH-A, "Poisoned"). Every swing rolls to hit, so a poisoned character's attack rolls
//! are thrown at disadvantage; there are no ability checks yet. Venom in the blood also stops
//! a character mending (our own, [`crate::recovery`]).

/// How long a failed save against a poisonous snake's venom poisons the delver, in game hours:
/// the `poisoned` effect's `hours` as shipped.
///
/// Our own. The SRD's poisonous snake (p. 386) does its venom once and leaves no condition. Its
/// lasting poisons run from 1 minute (the quasit's same 2d4, DC 10 venom, p. 273, with a save
/// each turn to end it) to 24 hours, and 1 hour is the commonest (drow poison, malice, truth
/// serum, "Poisons", p. 204). A game hour is five real minutes at the default clock, long
/// enough to be felt and seen without a save each turn.
pub const VENOM_POISONED_HOURS: f64 = 1.0;
