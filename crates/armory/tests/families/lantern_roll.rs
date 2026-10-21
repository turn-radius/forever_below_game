//! Lantern "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::lantern::{LIMITS, Lantern, LanternFuel, LanternMetal, LanternState, LanternStyle};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<LanternState> {
    range.map(Lantern::roll).collect()
}

#[test]
fn same_seed_same_lantern() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Lantern::roll(seed), Lantern::roll(seed));
        assert_eq!(Lantern::roll(seed).seed, seed);
    }
    assert_ne!(Lantern::roll(1), Lantern::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Lantern::roll(1207);
    let back: LanternState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 77] {
        let s = Lantern::roll(seed);
        assert_eq!(Lantern::roll_as(seed, s.body.style), s, "seed {seed}");
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
        let b = &s.body;
        match b.style {
            LanternStyle::Horn => assert!(matches!(b.sides, 4 | 6 | 8), "seed {}: {} panes", s.seed, b.sides),
            LanternStyle::Pierced => assert!(b.sides >= 8, "seed {}", s.seed),
            LanternStyle::Cage => assert!((4..=8).contains(&b.sides) && s.bail.links <= 3, "seed {}", s.seed),
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&LanternState) -> bool| all.iter().any(f);
    for style in LanternStyle::ALL {
        for fuel in LanternFuel::ALL {
            assert!(any(&|s| s.body.style == style && s.flame.fuel == fuel), "no {fuel:?} {style:?}");
        }
        for metal in LanternMetal::ALL {
            assert!(any(&|s| s.body.style == style && s.body.metal == metal), "no {metal:?} {style:?}");
        }
        assert!(any(&|s| s.body.style == style && s.flame.lit) && any(&|s| s.body.style == style && !s.flame.lit));
    }
    for sides in [4, 6, 8] {
        assert!(any(&|s| s.body.style == LanternStyle::Horn && s.body.sides == sides), "no {sides}-paned horn lantern");
    }
    assert!((0..=5).all(|n| any(&|s| s.bail.links == n)), "a chain length missing");
    assert!(any(&|s| s.body.feet) && any(&|s| !s.body.feet));
}

#[test]
fn nouns() {
    let mut s = LanternState::default();
    let cases = [
        (LanternStyle::Horn, LanternMetal::Brass, LanternFuel::Candle, "Horn Lantern"),
        (LanternStyle::Pierced, LanternMetal::Tin, LanternFuel::Candle, "Pierced Tin Lantern"),
        (LanternStyle::Pierced, LanternMetal::Iron, LanternFuel::Candle, "Pierced Lantern"),
        (LanternStyle::Cage, LanternMetal::Iron, LanternFuel::Oil, "Oil Lantern"),
        (LanternStyle::Cage, LanternMetal::Brass, LanternFuel::Candle, "Cage Lantern"),
    ];
    for (style, metal, fuel, name) in cases {
        s.body.style = style;
        s.body.metal = metal;
        s.flame.fuel = fuel;
        assert_eq!(Lantern::noun(&s), name);
    }
}
