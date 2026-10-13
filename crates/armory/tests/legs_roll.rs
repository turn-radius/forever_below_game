//! Trousers, breeches and leggings "Forge new": deterministic, in range, varied and coherent.

use armory::fabric::FabricPattern;
use armory::families::Generator;
use armory::families::legs::{LIMITS, Legs, LegsState};
use armory::sections::legwear::{LegwearStyle, WaistClosure};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<LegsState> {
    range.map(Legs::roll).collect()
}

#[test]
fn same_seed_same_legs() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Legs::roll(seed), Legs::roll(seed));
        assert_eq!(Legs::roll(seed).seed, seed);
    }
    assert_ne!(Legs::roll(1), Legs::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Legs::roll(1207);
    let back: LegsState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 1207] {
        let s = Legs::roll(seed);
        assert_eq!(Legs::roll_as(seed, s.legwear.style), s);
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
        let (seed, w) = (s.seed, &s.legwear);
        if w.style == LegwearStyle::Leggings {
            assert!(!w.puttees && w.ease <= 0.4 && !s.hem.on, "seed {seed}: loose or wrapped leggings");
        }
        if w.cuff > 0.0 {
            assert!(w.style == LegwearStyle::Trousers && !w.puttees, "seed {seed}: cuff on {:?}", w.style);
        }
        if w.patches {
            assert_eq!(w.style, LegwearStyle::Trousers, "seed {seed}");
        }
        if w.style == LegwearStyle::Breeches {
            assert!(w.ease >= 0.399, "seed {seed}: tight breeches");
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&LegsState) -> bool| all.iter().any(f);
    for style in LegwearStyle::ALL {
        assert!(any(&|s| s.legwear.style == style), "no {style:?}");
    }
    for closure in WaistClosure::ALL {
        assert!(any(&|s| s.legwear.closure == closure), "no {closure:?}");
    }
    assert!(any(&|s| s.legwear.puttees), "no puttees");
    assert!(any(&|s| s.legwear.cuff > 0.0), "no cuffs");
    assert!(any(&|s| s.legwear.patches), "no knee patches");
    assert!(any(&|s| s.hem.on), "no trim");
    assert!(any(&|s| s.legwear.cloth.pattern != FabricPattern::Plain), "no patterned cloth");
    for name in ["Trousers", "Wrapped Trousers", "Wide Trousers", "Cuffed Trousers", "Breeches", "Breeches and Puttees", "Leggings", "Hose"] {
        assert!(all.iter().any(|s| Legs::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = LegsState::default();
    assert_eq!(Legs::noun(&s), "Trousers");
    s.legwear.cuff = 0.3;
    assert_eq!(Legs::noun(&s), "Cuffed Trousers");
    s.legwear.ease = 0.9;
    s.legwear.taper = 0.1;
    assert_eq!(Legs::noun(&s), "Wide Trousers");
    s.legwear.puttees = true;
    assert_eq!(Legs::noun(&s), "Wrapped Trousers");
    s.legwear.style = LegwearStyle::Breeches;
    assert_eq!(Legs::noun(&s), "Breeches and Puttees");
    s.legwear.puttees = false;
    assert_eq!(Legs::noun(&s), "Breeches");
    s.legwear.style = LegwearStyle::Leggings;
    assert_eq!(Legs::noun(&s), "Leggings");
    s.legwear.cloth.pattern = FabricPattern::Hoops;
    assert_eq!(Legs::noun(&s), "Hose");
}
