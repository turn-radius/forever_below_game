//! Garment "Forge new": deterministic, in range, varied and coherent.

use armory::fabric::FabricPattern;
use armory::families::Generator;
use armory::families::garment::{Garment, GarmentState, LIMITS};
use armory::sections::garment::{GarmentStyle, Neckline, Sleeves};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<GarmentState> {
    range.map(Garment::roll).collect()
}

#[test]
fn same_seed_same_garment() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Garment::roll(seed), Garment::roll(seed));
        assert_eq!(Garment::roll(seed).seed, seed);
    }
    assert_ne!(Garment::roll(1), Garment::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Garment::roll(1207);
    let back: GarmentState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    let s = Garment::roll(42);
    assert_eq!(Garment::roll_as(42, s.cut.style), s);
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
        let (seed, g) = (s.seed, &s.cut);
        match g.style {
            GarmentStyle::Cloak | GarmentStyle::Cuirass | GarmentStyle::Brigandine => {
                assert_eq!(g.sleeves, Sleeves::None, "seed {seed}");
                assert!(!s.belt.on, "seed {seed}: belted {:?}", g.style);
            }
            GarmentStyle::Robe | GarmentStyle::Gambeson => assert_eq!(g.sleeves, Sleeves::Long, "seed {seed}"),
            _ => {}
        }
        if g.style == GarmentStyle::Gambeson {
            assert_eq!(g.neck, Neckline::High, "seed {seed}");
            assert_eq!(g.cloth.pattern, FabricPattern::Quilted, "seed {seed}");
        }
        if g.style == GarmentStyle::Brigandine {
            assert_eq!(g.cloth.pattern, FabricPattern::Rivets, "seed {seed}");
        }
        if g.style == GarmentStyle::Robe {
            assert!(g.length >= 6.0, "seed {seed}: short robe {}", g.length);
        }
        if s.plates.faulds > 0 {
            assert_eq!(g.style, GarmentStyle::Cuirass, "seed {seed}: faulds on {:?}", g.style);
        }
        if s.plates.pauldrons {
            assert!(matches!(g.style, GarmentStyle::Cuirass | GarmentStyle::Hauberk), "seed {seed}");
        }
        assert_ne!(g.cloth.color, g.cloth.color2, "seed {seed}: pattern invisible");
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&GarmentState) -> bool| all.iter().any(f);
    for style in GarmentStyle::ALL {
        assert!(any(&|s| s.cut.style == style), "no {style:?}");
    }
    for sleeves in [Sleeves::None, Sleeves::Short, Sleeves::Long] {
        assert!(any(&|s| s.cut.sleeves == sleeves), "no {sleeves:?} sleeves");
    }
    for neck in [Neckline::Round, Neckline::High, Neckline::Cowl] {
        assert!(any(&|s| s.cut.neck == neck), "no {neck:?} neck");
    }
    for pattern in FabricPattern::ALL {
        assert!(any(&|s| s.cut.cloth.pattern == pattern), "no {pattern:?} cloth");
    }
    assert!(any(&|s| s.belt.on), "no belt");
    assert!(any(&|s| s.hem.on), "no trim");
    assert!(any(&|s| s.hem.front), "no front trim");
    assert!(any(&|s| s.plates.pauldrons), "no pauldrons");
    assert!(any(&|s| s.etch.on), "no etching");
    for name in [
        "Tunic", "Long Tunic", "Robe", "Gambeson", "Mail Shirt", "Mail Hauberk", "Brigandine", "Scale Coat", "Cuirass",
        "Cuirass and Pauldrons", "Cloak",
    ] {
        assert!(all.iter().any(|s| Garment::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = GarmentState::default();
    assert_eq!(Garment::noun(&s), "Tunic");
    s.cut.length = 4.5;
    assert_eq!(Garment::noun(&s), "Long Tunic");
    s.cut.style = GarmentStyle::Hauberk;
    assert_eq!(Garment::noun(&s), "Mail Hauberk");
    s.cut.length = 1.5;
    assert_eq!(Garment::noun(&s), "Mail Shirt");
    s.cut.style = GarmentStyle::Cuirass;
    assert_eq!(Garment::noun(&s), "Cuirass");
    s.plates.pauldrons = true;
    assert_eq!(Garment::noun(&s), "Cuirass and Pauldrons");
    for (style, name) in [
        (GarmentStyle::Robe, "Robe"),
        (GarmentStyle::Gambeson, "Gambeson"),
        (GarmentStyle::Brigandine, "Brigandine"),
        (GarmentStyle::Scale, "Scale Coat"),
        (GarmentStyle::Cloak, "Cloak"),
    ] {
        s.cut.style = style;
        assert_eq!(Garment::noun(&s), name);
    }
}
