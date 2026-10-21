//! Boots and greaves "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::feet::{Feet, FeetState, LIMITS};
use armory::material::MaterialKind;
use armory::sections::boot::BootStyle;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<FeetState> {
    range.map(Feet::roll).collect()
}

#[test]
fn same_seed_same_boots() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Feet::roll(seed), Feet::roll(seed));
        assert_eq!(Feet::roll(seed).seed, seed);
    }
    assert_ne!(Feet::roll(1), Feet::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Feet::roll(1207);
    let back: FeetState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    let s = Feet::roll(42);
    assert_eq!(Feet::roll_as(42, s.boot.style), s);
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
        let (seed, b) = (s.seed, &s.boot);
        match b.style {
            BootStyle::Shoe | BootStyle::Poulaine => assert!(b.height < 1.0, "seed {seed}: tall {:?}", b.style),
            BootStyle::Wrapped | BootStyle::Sabaton => assert!(b.height >= 2.0, "seed {seed}: short {:?}", b.style),
            BootStyle::Boot => {}
        }
        if b.style == BootStyle::Poulaine {
            assert!(b.toe >= 0.5, "seed {seed}: blunt poulaine");
        }
        if b.knee {
            assert_eq!(b.style, BootStyle::Sabaton, "seed {seed}: knee cop on {:?}", b.style);
        }
        if b.cuff > 0.0 {
            assert_eq!(b.style, BootStyle::Boot, "seed {seed}: cuff on {:?}", b.style);
        }
        assert_eq!(b.material, MaterialKind::Leather, "seed {seed}");
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&FeetState) -> bool| all.iter().any(f);
    for style in BootStyle::ALL {
        assert!(any(&|s| s.boot.style == style), "no {style:?}");
    }
    assert!(any(&|s| s.boot.knee), "no knee cop");
    assert!(any(&|s| s.boot.cuff > 0.0), "no turned cuff");
    assert!(any(&|s| s.boot.straps >= 2), "no strapped boots");
    assert!(any(&|s| s.hem.on), "no trim");
    assert!(any(&|s| s.boot.sole == MaterialKind::Wood), "no wooden sole");
    assert!(any(&|s| s.etch.on), "no etching");
    for name in ["Turnshoes", "Riding Boots", "Buckled Boots", "Ankle Boots", "Poulaines", "Leg Wraps", "Greaves and Sabatons", "Sabatons"] {
        assert!(all.iter().any(|s| Feet::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = FeetState::default();
    assert_eq!(Feet::noun(&s), "Riding Boots");
    s.boot.height = 2.0;
    assert_eq!(Feet::noun(&s), "Buckled Boots");
    s.boot.height = 1.0;
    assert_eq!(Feet::noun(&s), "Ankle Boots");
    s.boot.style = BootStyle::Sabaton;
    assert_eq!(Feet::noun(&s), "Sabatons");
    s.boot.height = 3.0;
    assert_eq!(Feet::noun(&s), "Greaves and Sabatons");
    for (style, name) in [(BootStyle::Shoe, "Turnshoes"), (BootStyle::Poulaine, "Poulaines"), (BootStyle::Wrapped, "Leg Wraps")] {
        s.boot.style = style;
        assert_eq!(Feet::noun(&s), name);
    }
}
