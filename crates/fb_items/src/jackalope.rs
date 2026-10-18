//! What a jackalope leaves behind: [`jackalope_drop`] rolls the one or two things it sheds when
//! it is slain, from its seed. Our own; a jackalope of the tales is a prize, and this makes it
//! one.
//!
//! The drop is the level's own loot, drawn from the same gear families it leaves lying about
//! (goods such as food and torches are always plain, so they are not drawn), but never plain:
//! each piece is at least magic, and the odds of rare or better are those of a find a few
//! levels deeper ([`BOOST`]).

use armory::Family;
use procgen_rng::Mulberry32;

use crate::{Item, Odds, Rarity, roll_loot};

/// Mixed into a jackalope's seed so its drop does not repeat the numbers its body rolls from.
const DROP_SALT: u32 = 0x1ac4_a10e;

/// How many levels deeper than its own a jackalope's drop rolls its rarity, before plain pieces
/// are turned away. Our own.
pub const BOOST: u32 = 3;
/// How likely a second piece is. Our own.
const SECOND: f64 = 0.3;
/// The gear families of the level's loot (`fb_level`'s table), by the same weights.
const GEAR: &[(Family, u32)] = &[
    (Family::Dagger, 5),
    (Family::Club, 4),
    (Family::Sword, 4),
    (Family::Mace, 3),
    (Family::Axe, 3),
    (Family::Spear, 3),
    (Family::Staff, 2),
    (Family::Bow, 2),
    (Family::Crossbow, 1),
    (Family::Shield, 3),
    (Family::Helm, 3),
    (Family::Hood, 3),
    (Family::Hat, 2),
    (Family::Garment, 3),
    (Family::Belt, 2),
    (Family::Hands, 3),
    (Family::Legs, 2),
    (Family::Feet, 3),
    (Family::Ring, 1),
    (Family::Amulet, 1),
    (Family::Charm, 1),
];
/// How many seeds are tried for one piece before giving up on it.
const TRIES: u32 = 200;

/// The odds a jackalope's drop at `depth` is rolled against.
pub fn drop_odds(depth: u32) -> Odds {
    Odds::loot(depth).deeper(BOOST)
}

/// One or two pieces of the level's loot at `depth`, rolled from `seed`: none below magic, and
/// rare or better more often than loot lying on the floor. A pure function of its arguments.
pub fn jackalope_drop(seed: u32, depth: u32) -> Vec<Item> {
    let mut rng = Mulberry32::new(seed ^ DROP_SALT);
    let count = if rng.chance(SECOND) { 2 } else { 1 };
    let total: u32 = GEAR.iter().map(|(_, w)| w).sum();
    let mut items = Vec::new();
    for _ in 0..count {
        for _ in 0..TRIES {
            let mut at = rng.int(0, total - 1);
            let family = GEAR.iter().find(|(_, w)| if at < *w { true } else { at -= w; false }).map_or(Family::Dagger, |(f, _)| *f);
            let item_seed = (rng.next_f64() * 4_294_967_296.0) as u32;
            if let Some(item) = roll_loot(family, item_seed, drop_odds(depth)).filter(|i| i.rarity > Rarity::Common) {
                items.push(item);
                break;
            }
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_and_depth_drop_the_same_things() {
        for seed in 0..40 {
            for depth in [1, 4] {
                assert_eq!(jackalope_drop(seed, depth), jackalope_drop(seed, depth), "{seed} {depth}");
            }
        }
    }

    #[test]
    fn it_drops_one_or_two_things_and_never_a_common_one() {
        let (mut ones, mut twos) = (0, 0);
        for seed in 0..120 {
            for depth in [1, 2, 6] {
                let drop = jackalope_drop(seed, depth);
                assert!((1..=2).contains(&drop.len()), "{seed} {depth}: {}", drop.len());
                assert!(drop.iter().all(|i| i.rarity >= Rarity::Magic), "{seed} {depth}");
                if depth == 1 {
                    if drop.len() == 1 { ones += 1 } else { twos += 1 }
                }
            }
        }
        assert!(ones > twos && twos > 20, "{ones} ones and {twos} twos");
    }

    #[test]
    fn it_drops_rare_things_more_often_than_loot_lies_about() {
        for depth in [1, 5] {
            let drops: Vec<Item> = (0..150).flat_map(|seed| jackalope_drop(seed, depth)).collect();
            let rare = drops.iter().filter(|i| i.rarity >= Rarity::Rare).count() as f64 / drops.len() as f64;
            let [_, _, r, l] = Odds::loot(depth).shares();
            assert!(rare > 1.5 * (r + l), "depth {depth}: {rare} against {}", r + l);
        }
    }
}
