//! What a troll carries: [`troll_outfit`] rolls its weapon, if it has one, from its seed. Our
//! own; the SRD's troll fights with its claws and bite alone, and this gives some of them a
//! crude weapon to hold in the right hand in place of a claw.
//!
//! About half of trolls are armed. A club is the commonest, then a mace or a hand axe. Each is
//! rolled through [`roll_item_with`], so it has a rarity, affixes and a history like loot, but
//! never better than magic: a troll picks up what it can, it does not keep treasure.

use procgen_rng::Mulberry32;

use crate::outfit::OutfitSet;
use crate::{Odds, Rarity, Slot, roll_item_with};

/// Mixed into a troll's seed so its weapon does not repeat the numbers its body rolls from.
const OUTFIT_SALT: u32 = 0x7a0c_1b57;

/// How likely a troll is to be armed, half of them. Our own.
const ARMED: f64 = 0.5;
/// What it carries, by how often: a club half the time, a mace or a hand axe a quarter each.
const WEAPONS: &[&str] = &["Club", "Club", "Club", "Club", "Mace", "Mace", "Hand axe", "Hand axe"];

/// Rolls the weapon for a troll from `seed`, its rarity against `odds` but at most magic. A
/// pure function of its arguments; an unarmed troll's set is empty. The weapon is in
/// [`Slot::Mainhand`].
pub fn troll_outfit(seed: u32, odds: Odds) -> OutfitSet {
    let mut rng = Mulberry32::new(seed ^ OUTFIT_SALT);
    let mut worn = Vec::new();
    if rng.chance(ARMED) {
        let base = *rng.pick(WEAPONS);
        let item_seed = (rng.next_f64() * 4_294_967_296.0) as u32;
        if let Some(item) = roll_item_with(base, item_seed, odds.capped(Rarity::Magic)) {
            worn.push((Slot::Mainhand, item));
        }
    }
    OutfitSet { worn, goods: Vec::new(), ground: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_rolls_the_same_weapon_or_none() {
        for seed in 0..60 {
            assert_eq!(troll_outfit(seed, Odds::default()), troll_outfit(seed, Odds::default()), "{seed}");
        }
    }

    #[test]
    fn about_half_are_armed_and_a_club_is_the_commonest() {
        let sets: Vec<OutfitSet> = (0..200).map(|seed| troll_outfit(seed, Odds::kit(5))).collect();
        let armed = sets.iter().filter(|s| !s.worn.is_empty()).count();
        assert!((80..=120).contains(&armed), "{armed} of 200 armed");
        let count = |base: &str| sets.iter().filter(|s| s.item(Slot::Mainhand).is_some_and(|w| w.base == base)).count();
        let (club, mace, axe) = (count("Club"), count("Mace"), count("Hand axe"));
        assert!(club > mace && club > axe && mace > 0 && axe > 0, "club {club}, mace {mace}, axe {axe}");
        assert_eq!(club + mace + axe, armed, "nothing else");
    }

    #[test]
    fn it_holds_one_weapon_in_the_main_hand_at_most_magic() {
        for seed in 0..200 {
            let set = troll_outfit(seed, Odds::loot(8));
            assert!(set.worn.len() <= 1 && set.goods.is_empty() && set.ground.is_empty(), "{seed}");
            for (slot, item) in &set.worn {
                assert_eq!(*slot, Slot::Mainhand, "{seed}");
                assert!(item.fits(*slot) && item.profile.stats.is_weapon(), "{seed}: {}", item.base);
                assert!(item.rarity <= Rarity::Magic, "{seed}: {:?}", item.rarity);
            }
        }
    }
}
