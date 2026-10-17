//! What the risen dead wear and carry: [`skeleton_outfit`] rolls a skeleton's gear from its
//! seed. Our own; the plate has no undead, so this is the shape of
//! [`crate::outfit::Outfit::roll`] with a table of its own.
//!
//! A skeleton is kitted (helm, body armour, gloves, boots, a weapon and usually a shield),
//! part dressed (each piece by chance) or bare-boned (a weapon, or nothing at all). Every
//! piece is rolled through [`roll_item_with`], so it has a rarity, affixes and a history like loot,
//! and then it is worn hard: the dead have lain in it for ages, so its wear is high, iron
//! rusts, cloth and leather rot, and weapons are nicked.
//!
//! The dead of the first stone halls ([`FIRST_HALLS`]) are poorer: they wear no armour at all,
//! only rags, and hold a dagger or a club if anything, so a fresh delver with what the
//! first level gives can stand against them (#354). Our own.

use procgen_rng::Mulberry32;

use crate::dress::condition_mut;
use crate::outfit::OutfitSet;
use crate::{Item, Odds, Slot, Tag, roll_item_with};

/// Mixed into a skeleton's seed so its gear does not repeat the numbers its bones roll from.
const OUTFIT_SALT: u32 = 0x0d3a_dc10;

/// Weapons for the kitted dead, and for the rest (which add the spear, the bow and the club).
const KITTED_WEAPONS: &[&str] = &["Sword", "Sword", "Sword", "Hand axe", "Mace"];
const WEAPONS: &[&str] = &["Sword", "Sword", "Hand axe", "Mace", "Spear", "Spear", "Spear", "Bow", "Club"];
const KITTED_BODY: &[&str] = &["Mail", "Brigandine", "Scale coat", "Cuirass"];
const BODY: &[&str] = &["Gambeson", "Mail", "Brigandine", "Scale coat", "Cuirass", "Leather jerkin"];
const HANDS: &[&str] = &["Gloves", "Gauntlets", "Gauntlets"];
/// The depth of the first stone halls, the shallowest level the dead stand in
/// (`fb_level::LevelStyle::for_depth`). Our own.
pub const FIRST_HALLS: u32 = 2;
/// What the dead of the first halls hold, whether they would have been kitted or not: light
/// things, 1d4 each (a dagger or a club), or, three times in five, nothing
/// but their claws after all. Our own.
const FIRST_HALLS_WEAPONS: &[Option<&str>] = &[Some("Dagger"), Some("Club"), None, None, None];

/// Rolls the gear for a skeleton from `seed`, each piece's rarity against `odds` (a skeleton
/// met at a depth carries [`Odds::kit`] for it). A pure function of its arguments; slots left
/// empty are not in [`OutfitSet::worn`].
pub fn skeleton_outfit(seed: u32, odds: Odds) -> OutfitSet {
    let mut rng = Mulberry32::new(seed ^ OUTFIT_SALT);
    // Kitted 2 in 10, part dressed 4 in 10, bare-boned 4 in 10.
    let kit = rng.int(0, 9);
    let (kitted, bare) = (kit < 2, kit >= 6);
    // How likely each piece is: kitted, part dressed, bare-boned.
    let chance = |kitted_p: f64, mixed_p: f64, bare_p: f64| if kitted { kitted_p } else if bare { bare_p } else { mixed_p };

    // The first halls' dead draw as the others do, so a deeper skeleton's gear does not
    // change, and then go without the armour.
    let poor = odds.depth == FIRST_HALLS;
    let weapon = if !rng.chance(chance(1.0, 0.9, 0.6)) {
        None
    } else if poor {
        *rng.pick(FIRST_HALLS_WEAPONS)
    } else {
        Some(*rng.pick(if kitted { KITTED_WEAPONS } else { WEAPONS }))
    };
    // A shield needs a hand free: not with a bow.
    let shield = weapon.is_some_and(|w| w != "Bow") && rng.chance(chance(0.9, 0.4, 0.1)) && !poor;
    let helm = rng.chance(chance(1.0, 0.45, 0.08)) && !poor;
    let body = rng.chance(chance(1.0, 0.5, 0.08)) && !poor;
    let gloves = rng.chance(chance(1.0, 0.3, 0.04)) && !poor;
    let boots = rng.chance(chance(1.0, 0.35, 0.05)) && !poor;
    let rags = !body && rng.chance(chance(0.0, 0.3, 0.15));
    let body_base = *rng.pick(if kitted { KITTED_BODY } else { BODY });
    let hands_base = *rng.pick(HANDS);

    let mut bases: Vec<&str> = Vec::new();
    for (wanted, base) in [(boots, "Boots"), (rags, "Tunic"), (body, body_base), (gloves, hands_base), (helm, "Helm")] {
        if wanted {
            bases.push(base);
        }
    }
    bases.extend(weapon);
    if shield {
        bases.push("Shield");
    }
    let mut worn: Vec<(Slot, Item)> = Vec::new();
    for base in bases {
        let item_seed = (rng.next_f64() * 4_294_967_296.0) as u32;
        if let Some(mut item) = roll_item_with(base, item_seed, odds)
            && let Some(slot) = item.profile.slot
        {
            wear_hard(&mut item, &mut rng);
            worn.push((slot, item));
        }
    }
    OutfitSet { worn, goods: Vec::new(), ground: Vec::new() }
}

/// Wears `item` as a thing the dead have lain in: heavy wear, rust on iron, rot and fray on
/// the rest, and nicks and chips. Never lessens what the item already shows.
fn wear_hard(item: &mut Item, rng: &mut Mulberry32) {
    let metal = item.profile.tags.contains(&Tag::Metal);
    let weapon = item.profile.tags.contains(&Tag::Weapon);
    let c = condition_mut(&mut item.shape);
    c.wear = c.wear.max(rng.range_f32(0.55, 0.95));
    if metal {
        c.rust = c.rust.max(rng.range_f32(1.0, 1.5));
    } else {
        c.fray = c.fray.max(rng.range_f32(1.0, 1.5));
        c.rot = c.rot.max(rng.range_f32(1.0, 1.5));
    }
    c.damage = c.damage.max(rng.range_f32(0.15, if weapon { 0.7 } else { 0.5 }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armory_base::ARMORY_BASES;
    use crate::Rarity;

    #[test]
    fn the_same_seed_rolls_the_same_gear() {
        for seed in 0..60 {
            assert_eq!(skeleton_outfit(seed, Odds::default()), skeleton_outfit(seed, Odds::default()), "{seed}");
        }
    }

    #[test]
    fn every_base_it_names_exists() {
        let known = |base: &&str| ARMORY_BASES.iter().any(|(name, _)| name == base);
        for base in KITTED_WEAPONS.iter().chain(WEAPONS).chain(KITTED_BODY).chain(BODY).chain(HANDS).chain(&["Boots", "Helm", "Shield", "Tunic"]) {
            assert!(known(base) && crate::roll_item(base, 1).is_some(), "{base}");
        }
    }

    #[test]
    fn gear_goes_in_the_slot_it_is_made_for_once() {
        for seed in 0..200 {
            let set = skeleton_outfit(seed, Odds::default());
            for (slot, item) in &set.worn {
                assert!(item.fits(*slot), "{seed}: {} in {slot:?}", item.base);
                assert_eq!(set.worn.iter().filter(|(s, _)| s == slot).count(), 1, "{seed}: two in {slot:?}");
            }
            if let Some(w) = set.item(Slot::Mainhand) {
                assert!(w.profile.stats.is_weapon(), "{seed}: {}", w.base);
            }
            if let Some(s) = set.item(Slot::Offhand) {
                assert_eq!(s.base, "Shield");
                assert!(set.item(Slot::Mainhand).is_some_and(|w| w.base != "Bow"), "{seed}: shield with a bow");
            }
        }
    }

    #[test]
    fn some_are_kitted_some_armed_only_and_some_bare() {
        let sets: Vec<OutfitSet> = (0..200).map(|seed| skeleton_outfit(seed, Odds::default())).collect();
        let has = |set: &OutfitSet, slots: &[Slot]| slots.iter().all(|s| set.item(*s).is_some());
        let full = [Slot::Head, Slot::Body, Slot::Hands, Slot::Mainhand, Slot::Offhand];
        assert!(sets.iter().filter(|s| has(s, &full)).count() >= 8);
        assert!(sets.iter().any(|s| s.worn.is_empty()));
        assert!(sets.iter().any(|s| s.worn.len() == 1 && s.item(Slot::Mainhand).is_some_and(|w| w.base == "Spear")));
        assert!(sets.iter().any(|s| has(s, &full) && s.item(Slot::Mainhand).is_some_and(|w| w.base == "Sword")));
    }

    #[test]
    fn the_first_levels_dead_carry_nothing_rare() {
        let mut magic = 0;
        for seed in 0..200 {
            for (_, item) in &skeleton_outfit(seed, Odds::kit(1)).worn {
                assert!(item.rarity <= Rarity::Magic, "{seed}: {} is {:?}", item.base, item.rarity);
                magic += usize::from(item.rarity == Rarity::Magic);
            }
        }
        assert!(magic > 0, "no magic gear on 200 skeletons");
        let deep = (0..50).flat_map(|seed| skeleton_outfit(seed, Odds::kit(5)).worn).filter(|(_, i)| i.rarity >= Rarity::Rare).count();
        assert!(deep > 0, "nothing rare on 50 skeletons at depth 5");
    }

    #[test]
    fn the_first_halls_dead_wear_no_armour_and_hold_light_things() {
        let mut armed = 0;
        for seed in 0..300 {
            let set = skeleton_outfit(seed, Odds::kit(FIRST_HALLS));
            for (slot, item) in &set.worn {
                assert!(item.base == "Tunic" || *slot == Slot::Mainhand, "{seed}: {} in {slot:?}, rags only", item.base);
                assert_eq!(item.profile.stats.armour, 0.0, "{seed}: {}", item.base);
            }
            if let Some(w) = set.item(Slot::Mainhand) {
                assert!(FIRST_HALLS_WEAPONS.contains(&Some(w.base.as_str())), "{seed}: {}", w.base);
                armed += 1;
            }
        }
        assert!((60..200).contains(&armed), "some still hold something: {armed}");
        // Deeper, the same seeds wear what they always did.
        assert_eq!(skeleton_outfit(7, Odds::kit(4)), skeleton_outfit(7, Odds::kit(4)));
        assert!((0..100).any(|seed| skeleton_outfit(seed, Odds::kit(4)).item(Slot::Body).is_some_and(|b| b.profile.stats.armour > 0.0)));
    }

    #[test]
    fn it_is_worn_hard() {
        for seed in 0..100 {
            for (_, item) in &skeleton_outfit(seed, Odds::default()).worn {
                let c = item.shape.condition();
                assert!(c.wear >= 0.55, "{seed}: {} wear {}", item.base, c.wear);
                assert!(c.damage >= 0.15, "{seed}: {} damage {}", item.base, c.damage);
            }
        }
    }
}
