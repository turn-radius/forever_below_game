//! Falling unconscious and coming to: what a delver brought down to 0 hit points can survive,
//! and what it costs them. Our own; the SRD's death saving throws (p. 97) are left out.
//!
//! - A world (one run's abyss) lets the delver come to [`RevivalRules::revives`] times. After
//!   that the next fall ends the run as before.
//! - Each time leaves a [`Wound`] that never heals: a permanent effect on one body part, never
//!   the same one twice while another is left ([`pick_wound`]).
//! - If a faction on the level is still friendly, its folk carry the delver to their hall
//!   ([`choose_hall`]), tend them back to [`RevivalRules::hall_hit_points`] of their maximum and
//!   take [`RevivalRules::hall_fee`] of the coin they carry ([`fee`]).
//! - With no friend on the level they come to where they fell, alone, hours later
//!   ([`RevivalRules::hours_alone`]), with [`RevivalRules::alone_hit_points`] of their maximum.

use fb_items::{Coins, Slot};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

/// The numbers falling unconscious runs on (`[revival]` in `data/rules.toml`). All our own,
/// a first guess to play against.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RevivalRules {
    /// Times a delver may come to in one world.
    pub revives: u32,
    /// Share of the maximum hit points a faction's healers bring the delver back to.
    pub hall_hit_points: f64,
    /// Share of the coin the delver carries that the faction takes for its trouble.
    pub hall_fee: f64,
    /// Share of the maximum hit points a delver who comes to alone has.
    pub alone_hit_points: f64,
    /// Game hours a delver lies unconscious alone before coming to; hunger and thirst go on.
    pub hours_alone: f64,
}

impl Default for RevivalRules {
    fn default() -> Self {
        RevivalRules { revives: 2, hall_hit_points: 1.0, hall_fee: 0.5, alone_hit_points: 0.25, hours_alone: 4.0 }
    }
}

impl RevivalRules {
    /// How many more times a delver who has come to `used` times may.
    pub fn left(&self, used: u32) -> u32 {
        self.revives.saturating_sub(used)
    }

    /// The hit points a delver of `max` comes to with, in a hall or alone: at least 1.
    pub fn hit_points(&self, max: i32, in_a_hall: bool) -> i32 {
        let share = if in_a_hall { self.hall_hit_points } else { self.alone_hit_points };
        ((max as f64 * share.clamp(0.0, 1.0)).round() as i32).clamp(1, max.max(1))
    }
}

/// A wound that never heals: the effect it gives (`data/effects.json`) and where it sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wound {
    pub effect: &'static str,
    pub part: Slot,
    /// How the delver is told of it on coming to.
    pub line: &'static str,
}

/// Every lasting wound, one per body part.
pub const WOUNDS: [Wound; 3] = [
    Wound { effect: "lame_leg", part: Slot::Legs, line: "Your leg was broken and set badly. You will limp from now on." },
    Wound { effect: "ruined_eye", part: Slot::Head, line: "One eye did not survive the fall. The dark reaches closer now." },
    Wound { effect: "cracked_ribs", part: Slot::Body, line: "Your ribs knitted crooked. Every breath costs more, and you tire sooner." },
];

/// The wound a fall leaves, from those the delver does not have yet (`taken` holds effect ids);
/// once every one is taken, any of them again.
pub fn pick_wound(taken: &[&str], rng: &mut Mulberry32) -> Wound {
    let fresh: Vec<&Wound> = WOUNDS.iter().filter(|w| !taken.contains(&w.effect)).collect();
    let pool: Vec<&Wound> = if fresh.is_empty() { WOUNDS.iter().collect() } else { fresh };
    *pool[(rng.next_f64() * pool.len() as f64) as usize % pool.len()]
}

/// Which faction carries the delver to its hall, and which room that is: of the friendly folk
/// standing on the level (`friendly` holds each one's faction and the room they stand in), the
/// faction with the most, the lower id on a tie; its hall is the room most of them stand in.
/// `None` with no friendly folk in any room.
pub fn choose_hall(friendly: &[(u8, Option<usize>)]) -> Option<(u8, usize)> {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<u8, BTreeMap<usize, u32>> = BTreeMap::new();
    for &(faction, room) in friendly {
        if let Some(room) = room {
            *counts.entry(faction).or_default().entry(room).or_default() += 1;
        }
    }
    let (faction, rooms) = counts.iter().max_by(|(fa, a), (fb, b)| a.values().sum::<u32>().cmp(&b.values().sum::<u32>()).then(fb.cmp(fa)))?;
    let (room, _) = rooms.iter().max_by(|(ra, a), (rb, b)| a.cmp(b).then(rb.cmp(ra)))?;
    Some((*faction, *room))
}

/// Coins as copper pieces (1 gp = 10 sp = 100 cp, SRD 5.1 p. 62).
pub fn in_copper(coins: Coins) -> u32 {
    coins.gp * 100 + coins.sp * 10 + coins.cp
}

/// What a faction takes of `carried` copper: `share` of it, rounded up, never more than there is.
pub fn fee(carried: u32, share: f64) -> u32 {
    ((carried as f64 * share.clamp(0.0, 1.0)).ceil() as u32).min(carried)
}

/// Takes up to `owed` copper out of `purse`, changing gold and silver down as needed, and
/// returns what is still owed after it.
pub fn pay_from(purse: &mut Coins, owed: u32) -> u32 {
    let held = in_copper(*purse);
    let paid = owed.min(held);
    let rest = held - paid;
    // Keep as much gold as there was; the change comes back as silver and copper.
    let gp = (rest / 100).min(purse.gp);
    let sp = (rest - gp * 100) / 10;
    *purse = Coins { gp, sp, cp: rest - gp * 100 - sp * 10 };
    owed - paid
}

/// Copper as the fewest coins, for telling the delver what was taken ("12 gp 5 sp").
pub fn coin_words(copper: u32) -> String {
    let parts: Vec<String> = [(copper / 100, "gp"), (copper / 10 % 10, "sp"), (copper % 10, "cp")].iter().filter(|(n, _)| *n > 0).map(|(n, unit)| format!("{n} {unit}")).collect();
    if parts.is_empty() { "nothing".to_string() } else { parts.join(" ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_revives_then_none() {
        let rules = RevivalRules::default();
        assert_eq!((rules.left(0), rules.left(1), rules.left(2), rules.left(5)), (2, 1, 0, 0));
    }

    #[test]
    fn a_hall_heals_fully_and_alone_leaves_a_quarter_but_never_zero() {
        let rules = RevivalRules::default();
        assert_eq!(rules.hit_points(12, true), 12);
        assert_eq!(rules.hit_points(12, false), 3);
        assert_eq!(rules.hit_points(2, false), 1);
    }

    #[test]
    fn a_second_wound_is_never_the_first_again() {
        for seed in 0..64 {
            let mut rng = Mulberry32::new(seed);
            let first = pick_wound(&[], &mut rng);
            let second = pick_wound(&[first.effect], &mut rng);
            assert_ne!(first.effect, second.effect, "seed {seed}");
        }
        let all: Vec<&str> = WOUNDS.iter().map(|w| w.effect).collect();
        assert!(WOUNDS.contains(&pick_wound(&all, &mut Mulberry32::new(3))));
    }

    #[test]
    fn every_wound_is_a_shipped_effect_on_a_part_that_never_ends() {
        let defs = crate::effect::shipped();
        for wound in WOUNDS {
            let def = defs.get(wound.effect).unwrap_or_else(|| panic!("{} missing", wound.effect));
            assert!(def.on_a_part && def.hours.is_none() && def.per_hour == 0.0, "{}", wound.effect);
        }
    }

    #[test]
    fn the_hall_is_the_biggest_friendly_factions_busiest_room() {
        assert_eq!(choose_hall(&[]), None);
        assert_eq!(choose_hall(&[(1, None)]), None);
        assert_eq!(choose_hall(&[(2, Some(4)), (2, Some(4)), (2, Some(7)), (5, Some(1)), (5, Some(1))]), Some((2, 4)));
        // A tie goes to the lower faction.
        assert_eq!(choose_hall(&[(5, Some(1)), (3, Some(9))]), Some((3, 9)));
    }

    #[test]
    fn the_fee_is_half_rounded_up_and_paid_out_of_gold_with_change() {
        assert_eq!(fee(125, 0.5), 63);
        assert_eq!(fee(0, 0.5), 0);
        let mut purse = Coins { gp: 1, sp: 2, cp: 5 };
        assert_eq!(pay_from(&mut purse, 63), 0);
        assert_eq!(in_copper(purse), 62);
        let mut thin = Coins { gp: 0, sp: 1, cp: 0 };
        assert_eq!(pay_from(&mut thin, 25), 15);
        assert_eq!(thin, Coins::default());
        assert_eq!(coin_words(1250), "12 gp 5 sp");
        assert_eq!(coin_words(0), "nothing");
    }
}
