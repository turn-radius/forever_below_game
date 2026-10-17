//! Rarity shows on the object: grime and rust on the common stuff, etching and gold inlay on
//! the fine things. The plate's `dressState` (`docs/items.md`, "Rarity shows on the object").

use crate::Rarity;
use crate::tables::Affix;
use armory::Item;
use armory::cond::{Condition, ConditionState};
use armory::etch::{EtchFinish, EtchPattern, Etching};
use armory::Rgb;
use procgen_rng::Mulberry32;

/// Legendary inlay: the plate's `#d4a838`.
pub const GOLD_INLAY: Rgb = Rgb::hex(0xd4a838);

/// Every family state's `cond`. `armory::Item` only hands out a copy, so the match is here.
pub fn condition_mut(item: &mut Item) -> &mut Condition {
    match item {
        Item::Staff(s) => &mut s.cond,
        Item::Mace(s) => &mut s.cond,
        Item::Sword(s) => &mut s.cond,
        Item::Spear(s) => &mut s.cond,
        Item::Dagger(s) => &mut s.cond,
        Item::Club(s) => &mut s.cond,
        Item::Axe(s) => &mut s.cond,
        Item::Bow(s) => &mut s.cond,
        Item::Crossbow(s) => &mut s.cond,
        Item::Helm(s) => &mut s.cond,
        Item::Hood(s) => &mut s.cond,
        Item::Hat(s) => &mut s.cond,
        Item::Garment(s) => &mut s.cond,
        Item::Belt(s) => &mut s.cond,
        Item::Hands(s) => &mut s.cond,
        Item::Legs(s) => &mut s.cond,
        Item::Feet(s) => &mut s.cond,
        Item::Shield(s) => &mut s.cond,
        Item::Ring(s) => &mut s.cond,
        Item::Amulet(s) => &mut s.cond,
        Item::Charm(s) => &mut s.cond,
        Item::Bags(s) => &mut s.cond,
        Item::Chests(s) => &mut s.cond,
        Item::Food(s) => &mut s.cond,
        Item::Torch(s) => &mut s.cond,
        Item::Lantern(s) => &mut s.cond,
        Item::Sundry(s) => &mut s.cond,
        Item::Writing(s) => &mut s.cond,
        Item::Arrow(s) | Item::Bolt(s) => &mut s.cond,
        Item::Quiver(s) | Item::BoltCase(s) => &mut s.cond,
    }
}

/// The state's etching, for the families that have one.
pub fn etching_mut(item: &mut Item) -> Option<&mut Etching> {
    match item {
        Item::Staff(s) => Some(&mut s.etch),
        Item::Mace(s) => Some(&mut s.etch),
        Item::Sword(s) => Some(&mut s.etch),
        Item::Spear(s) => Some(&mut s.etch),
        Item::Dagger(s) => Some(&mut s.etch),
        Item::Club(s) => Some(&mut s.etch),
        Item::Axe(s) => Some(&mut s.etch),
        Item::Bow(s) => Some(&mut s.etch),
        Item::Crossbow(s) => Some(&mut s.etch),
        Item::Helm(s) => Some(&mut s.etch),
        Item::Hood(s) => Some(&mut s.etch),
        Item::Hat(s) => Some(&mut s.etch),
        Item::Garment(s) => Some(&mut s.etch),
        Item::Belt(s) => Some(&mut s.etch),
        Item::Hands(s) => Some(&mut s.etch),
        Item::Legs(s) => Some(&mut s.etch),
        Item::Feet(s) => Some(&mut s.etch),
        Item::Shield(s) => Some(&mut s.etch),
        Item::Ring(s) => Some(&mut s.etch),
        Item::Amulet(s) => Some(&mut s.etch),
        Item::Charm(s) => Some(&mut s.etch),
        Item::Bags(s) => Some(&mut s.etch),
        Item::Chests(s) => Some(&mut s.etch),
        Item::Food(_) | Item::Torch(_) | Item::Lantern(_) | Item::Sundry(_) | Item::Writing(_) | Item::Arrow(_) | Item::Bolt(_) | Item::Quiver(_) | Item::BoltCase(_) => None,
    }
}

/// Wear for `rarity` from one or two draws: common 0.15–0.50, magic 0.06–0.26, rare
/// 0.02–0.14; a legendary is pristine (0.02) or old and worn (0.4–0.7), even odds.
pub fn wear_for(rarity: Rarity, r: &mut Mulberry32) -> f32 {
    match rarity {
        Rarity::Common => r.range_f32(0.15, 0.5),
        Rarity::Magic => r.range_f32(0.06, 0.26),
        Rarity::Rare => r.range_f32(0.02, 0.14),
        Rarity::Legendary => if r.chance(0.5) { 0.02 } else { r.range_f32(0.4, 0.7) },
    }
}

/// Dresses `shape` for its rarity and affixes, from its own stream (the plate's
/// `mulberry32(seed * 3 + 1)`): wear by rarity, then each affix's condition effects (a
/// flaw's rust, fray and rot), then etching on rare and legendary items, gold-inlaid on a
/// legendary.
pub fn dress(shape: &mut Item, rarity: Rarity, affixes: &[&Affix], seed: u32) {
    let mut r = Mulberry32::new(seed.wrapping_mul(3).wrapping_add(1));
    let c = condition_mut(shape);
    c.state = ConditionState::Custom;
    c.wear = wear_for(rarity, &mut r);
    for a in affixes {
        a.apply_to_condition(c);
    }
    if matches!(rarity, Rarity::Rare | Rarity::Legendary) {
        let pattern = *r.pick(&EtchPattern::ALL);
        if let Some(e) = etching_mut(shape) {
            e.on = true;
            e.pattern = pattern;
            if rarity == Rarity::Legendary {
                e.finish = EtchFinish::Inlaid;
                e.inlay = GOLD_INLAY;
                e.coverage = 1.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armory_base::roll_shape;
    use crate::tables::affixes;

    fn affix(name: &str) -> &'static Affix {
        affixes().iter().find(|a| a.name == name).unwrap()
    }

    fn dressed(base: &str, seed: u32, rarity: Rarity, chosen: &[&Affix]) -> Item {
        let mut shape = roll_shape(base, seed).unwrap();
        dress(&mut shape, rarity, chosen, seed);
        shape
    }

    #[test]
    fn wear_follows_rarity() {
        for seed in 1..300 {
            let wear = |r| dressed("Sword", seed, r, &[]).condition().wear;
            assert!((0.15..0.5).contains(&wear(Rarity::Common)));
            assert!((0.06..0.26).contains(&wear(Rarity::Magic)));
            assert!((0.02..0.14).contains(&wear(Rarity::Rare)));
            let w = wear(Rarity::Legendary);
            assert!(w == 0.02 || (0.4..0.7).contains(&w), "{w}");
        }
    }

    #[test]
    fn fine_things_are_etched_and_legendaries_inlaid() {
        for seed in 1..100 {
            let Item::Sword(rare) = dressed("Sword", seed, Rarity::Rare, &[]) else { panic!() };
            assert!(rare.etch.on);
            let Item::Sword(legend) = dressed("Sword", seed, Rarity::Legendary, &[]) else { panic!() };
            assert!(legend.etch.on);
            assert_eq!((legend.etch.finish, legend.etch.inlay, legend.etch.coverage), (EtchFinish::Inlaid, GOLD_INLAY, 1.0));
        }
    }

    #[test]
    fn flaws_raise_decay() {
        let rusted = dressed("Sword", 5, Rarity::Common, &[affix("Rusted")]).condition();
        assert_eq!(rusted.rust, 1.5);
        assert!(rusted.wear >= 0.75);
        let moth = dressed("Cloak", 5, Rarity::Common, &[affix("Moth-eaten")]).condition();
        assert_eq!((moth.fray, moth.rot), (1.4, 1.2));
        assert!(moth.wear >= 0.6);
        let cracked = dressed("Staff", 5, Rarity::Common, &[affix("Cracked")]).condition();
        assert_eq!(cracked.rot, 1.5);
        let clean = dressed("Sword", 5, Rarity::Common, &[]).condition();
        assert!(clean.wear < 0.5 && clean.rust == 1.0);
    }

    #[test]
    fn dressing_is_deterministic_and_works_on_every_family() {
        for (name, _) in crate::armory_base::ARMORY_BASES {
            let a = dressed(name, 11, Rarity::Legendary, &[]);
            assert_eq!(a, dressed(name, 11, Rarity::Legendary, &[]), "{name}");
            assert_eq!(a.condition().state, ConditionState::Custom);
        }
    }
}
