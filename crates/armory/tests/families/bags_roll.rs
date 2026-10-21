//! Bag "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::bags::{BagState, BagStyle, Bags, LIMITS};
use armory::fabric::{FabricPattern, Textile};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<BagState> {
    range.map(Bags::roll).collect()
}

#[test]
fn same_seed_same_bag() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Bags::roll(seed), Bags::roll(seed));
        assert_eq!(Bags::roll(seed).seed, seed);
    }
    assert_ne!(Bags::roll(1), Bags::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Bags::roll(1207);
    let back: BagState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 1207] {
        let s = Bags::roll(seed);
        assert_eq!(Bags::roll_as(seed, s.bag.style), s, "seed {seed}");
    }
}

#[test]
fn every_slider_within_range() {
    for s in rolls(1..=1024) {
        let bad = s.out_of_range(&LIMITS);
        assert!(bad.is_empty(), "seed {}: {bad:?}", s.seed);
    }
}

#[test]
fn rolls_are_coherent() {
    for s in rolls(1..=1024) {
        let (b, seed) = (&s.bag, s.seed);
        if b.pocket || b.bedroll {
            assert_eq!(b.style, BagStyle::Backpack, "seed {seed}: pocket or bedroll on a {:?}", b.style);
        }
        if b.coins {
            assert!(matches!(b.style, BagStyle::Pouch | BagStyle::Purse), "seed {seed}: coins spilled from a {:?}", b.style);
        }
        if b.fabric == Textile::Burlap {
            assert_eq!(b.style, BagStyle::Sack, "seed {seed}: burlap {:?}", b.style);
            assert_eq!(b.cloth.pattern, FabricPattern::Plain, "seed {seed}: patterned burlap");
            assert!(!s.etch.on, "seed {seed}: embroidered burlap");
        }
        if b.fabric == Textile::Leather {
            assert!(matches!(b.cloth.pattern, FabricPattern::Plain | FabricPattern::Rivets), "seed {seed}: woven pattern on leather");
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&BagState) -> bool| all.iter().any(f);
    for style in BagStyle::ALL {
        assert!(any(&|s| s.bag.style == style), "no {style:?}");
    }
    for fabric in Textile::ALL {
        assert!(any(&|s| s.bag.fabric == fabric), "no {fabric:?}");
    }
    assert!(any(&|s| s.bag.coins), "no coins");
    assert!(any(&|s| s.bag.pocket), "no pocket");
    assert!(any(&|s| s.bag.style == BagStyle::Backpack && !s.bag.pocket), "always a pocket");
    assert!(any(&|s| s.bag.bedroll), "no bedroll");
    assert!(any(&|s| s.bag.patch), "no patch");
    assert!(any(&|s| s.etch.on), "no tooling or embroidery");
    for name in ["Drawstring Pouch", "Coin Purse", "Belt Pouch", "Satchel", "Adventurer's Pack", "Rucksack", "Grain Sack", "Sack"] {
        assert!(all.iter().any(|s| Bags::noun(s) == name), "no {name}");
    }
}

#[test]
fn every_pattern_turns_up() {
    let all = rolls(1..=1024);
    for pattern in FabricPattern::ALL {
        assert!(all.iter().any(|s| s.bag.cloth.pattern == pattern), "no {pattern:?}");
    }
}

#[test]
fn nouns() {
    let mut s = BagState::default();
    let cases = [
        (BagStyle::Pouch, "Drawstring Pouch"),
        (BagStyle::Purse, "Coin Purse"),
        (BagStyle::BeltPouch, "Belt Pouch"),
        (BagStyle::Satchel, "Satchel"),
    ];
    for (style, noun) in cases {
        s.bag.style = style;
        assert_eq!(Bags::noun(&s), noun);
    }
    s.bag.style = BagStyle::Backpack;
    s.bag.bedroll = true;
    assert_eq!(Bags::noun(&s), "Adventurer's Pack");
    s.bag.bedroll = false;
    assert_eq!(Bags::noun(&s), "Rucksack");
    s.bag.style = BagStyle::Sack;
    s.bag.fabric = Textile::Burlap;
    assert_eq!(Bags::noun(&s), "Grain Sack");
    s.bag.fabric = Textile::Cloth;
    assert_eq!(Bags::noun(&s), "Sack");
}
