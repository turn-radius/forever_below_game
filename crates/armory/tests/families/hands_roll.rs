//! Gloves and gauntlets "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::hands::{Hands, HandsState, LIMITS};
use armory::material::MaterialKind;
use armory::sections::hand::HandStyle;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<HandsState> {
    range.map(Hands::roll).collect()
}

#[test]
fn same_seed_same_gloves() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Hands::roll(seed), Hands::roll(seed));
        assert_eq!(Hands::roll(seed).seed, seed);
    }
    assert_ne!(Hands::roll(1), Hands::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Hands::roll(1207);
    let back: HandsState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    let s = Hands::roll(42);
    assert_eq!(Hands::roll_as(42, s.hand.style), s);
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
        let (seed, h) = (s.seed, &s.hand);
        if h.gadlings {
            assert_eq!(h.style, HandStyle::Gauntlet, "seed {seed}: spikes on {:?}", h.style);
        }
        if s.hem.on {
            assert!(matches!(h.style, HandStyle::Glove | HandStyle::Mitten), "seed {seed}: trim on {:?}", h.style);
        }
        if h.style == HandStyle::Bracer {
            assert!(h.cuff >= 1.8, "seed {seed}: short bracer");
        }
        assert!(matches!(h.plate, MaterialKind::Iron | MaterialKind::Brass | MaterialKind::Silver), "seed {seed}");
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&HandsState) -> bool| all.iter().any(f);
    for style in HandStyle::ALL {
        assert!(any(&|s| s.hand.style == style), "no {style:?}");
    }
    assert!(any(&|s| s.hand.gadlings), "no gadlings");
    assert!(any(&|s| s.hem.on), "no trim");
    assert!(any(&|s| s.etch.on), "no etching");
    assert!(any(&|s| s.hand.material == MaterialKind::Cord), "no cord gloves");
    for name in ["Riding Gloves", "Leather Gloves", "Spiked Gauntlets", "Gauntlets", "Hourglass Gauntlets", "Mail Mittens", "Archer's Bracer"] {
        assert!(all.iter().any(|s| Hands::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = HandsState::default();
    assert_eq!(Hands::noun(&s), "Gauntlets");
    s.hand.gadlings = true;
    assert_eq!(Hands::noun(&s), "Spiked Gauntlets");
    s.hand.style = HandStyle::Glove;
    s.hand.flare = 0.3;
    assert_eq!(Hands::noun(&s), "Leather Gloves");
    s.hand.flare = 0.5;
    assert_eq!(Hands::noun(&s), "Riding Gloves");
    s.hand.style = HandStyle::Hourglass;
    assert_eq!(Hands::noun(&s), "Hourglass Gauntlets");
    s.hand.style = HandStyle::Mitten;
    assert_eq!(Hands::noun(&s), "Mail Mittens");
    s.hand.style = HandStyle::Bracer;
    assert_eq!(Hands::noun(&s), "Archer's Bracer");
}
