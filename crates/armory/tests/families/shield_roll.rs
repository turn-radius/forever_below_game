//! Shield "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::shield::{BossStyle, LIMITS, Shield, ShieldArchetype, ShieldBend, ShieldShape, ShieldState};
use armory::heraldry::Division;
use armory::sections::device::Device;
use armory::material::MaterialKind;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<ShieldState> {
    range.map(Shield::roll).collect()
}

#[test]
fn same_seed_same_shield() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Shield::roll(seed), Shield::roll(seed));
        assert_eq!(Shield::roll(seed).seed, seed);
    }
    assert_ne!(Shield::roll(1), Shield::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Shield::roll(1207);
    let back: ShieldState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 1207] {
        let s = Shield::roll(seed);
        let found = ShieldArchetype::WEIGHTS.iter().any(|(a, _)| Shield::roll_as(seed, *a) == s);
        assert!(found, "roll({seed}) should equal roll_as({seed}, its archetype)");
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
        let seed = s.seed;
        let b = s.body;
        if b.shape.is_round() {
            assert!((b.width - 1.0).abs() < 1e-6, "seed {seed}: round shield not round");
            assert_eq!(b.bend, ShieldBend::Dome, "seed {seed}");
        }
        if b.shape == ShieldShape::Pavise {
            assert_eq!(b.bend, ShieldBend::Ridge, "seed {seed}");
        }
        if b.shape == ShieldShape::Buckler {
            assert!(b.size <= 4.0 && s.boss.on, "seed {seed}: buckler too big or bossless");
        }
        if s.field.painted && s.field.charge != Device::None {
            assert!(s.field.charge_contrasts(), "seed {seed}: charge breaks the rule of tincture: {:?}", s.field);
        }
        if s.field.painted && s.field.division != Division::Plain {
            assert_ne!(s.field.tincture.is_metal(), s.field.tincture2.is_metal(), "seed {seed}: like on like");
        }
        if !s.field.painted {
            assert_eq!(s.field.division, Division::Plain, "seed {seed}: division on a bare face");
        }
        if b.material == MaterialKind::Leather {
            assert!(!s.field.painted, "seed {seed}: painted leather");
        }
        if s.boss.on {
            assert!(s.field.charge == Device::None, "seed {seed}: charge hidden under the boss");
        }
        // The boss fits the board.
        if s.boss.on {
            assert!(s.boss.size < b.breadth().min(b.height()) * 0.5, "seed {seed}: boss {} on a {} board", s.boss.size, b.breadth());
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&ShieldState) -> bool| all.iter().any(f);
    for shape in ShieldShape::ALL {
        assert!(any(&|s| s.body.shape == shape), "no {shape:?}");
    }
    for bend in [ShieldBend::Dome, ShieldBend::Cylinder, ShieldBend::Ridge] {
        assert!(any(&|s| s.body.bend == bend), "no {bend:?}");
    }
    for division in Division::ALL {
        assert!(any(&|s| s.field.painted && s.field.division == division), "no {division:?}");
    }
    for device in Device::SIGNS {
        assert!(any(&|s| s.field.charge == device), "no {device:?} charge");
    }
    for style in BossStyle::ALL {
        assert!(any(&|s| s.boss.on && s.boss.style == style), "no {style:?} boss");
    }
    assert!(any(&|s| !s.field.painted), "always painted");
    assert!(any(&|s| s.field.charge == Device::None), "always charged");
    assert!(any(&|s| s.rim.on) && any(&|s| !s.rim.on), "rim never varies");
    assert!(any(&|s| s.studs.count > 0), "no studs");
    assert!(any(&|s| s.etch.on), "no etching");
    for name in ["Round Shield", "Targe", "Buckler", "Heater Shield", "Kite Shield", "Tower Shield", "Pavise", "Oval Shield"] {
        assert!(all.iter().any(|s| Shield::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = ShieldState::default();
    let cases = [
        (ShieldShape::Round, "Round Shield"),
        (ShieldShape::Buckler, "Buckler"),
        (ShieldShape::Heater, "Heater Shield"),
        (ShieldShape::Kite, "Kite Shield"),
        (ShieldShape::Tower, "Tower Shield"),
        (ShieldShape::Pavise, "Pavise"),
        (ShieldShape::Oval, "Oval Shield"),
    ];
    for (shape, name) in cases {
        s.body.shape = shape;
        assert_eq!(Shield::noun(&s), name);
    }
    s.body.shape = ShieldShape::Round;
    s.body.material = MaterialKind::Leather;
    assert_eq!(Shield::noun(&s), "Targe");
    s.body.shape = ShieldShape::Heater;
    assert_eq!(Shield::noun(&s), "Heater Shield", "only a round leather shield is a targe");
}
