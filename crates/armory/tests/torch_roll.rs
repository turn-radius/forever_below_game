//! Torch "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::torch::{BurnState, Fuel, HandleGrip, LIMITS, Torch, TorchState, TorchStyle};
use armory::material::MaterialKind;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<TorchState> {
    range.map(Torch::roll).collect()
}

#[test]
fn same_seed_same_torch() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Torch::roll(seed), Torch::roll(seed));
        assert_eq!(Torch::roll(seed).seed, seed);
    }
    assert_ne!(Torch::roll(1), Torch::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Torch::roll(1207);
    let back: TorchState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 77] {
        let s = Torch::roll(seed);
        assert_eq!(Torch::roll_as(seed, s.head.style), s, "seed {seed}");
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
        let (seed, b) = (s.seed, &s.burn);
        match b.state {
            BurnState::Fresh => assert_eq!(b.char, 0.0, "seed {seed}: a fresh torch is charred"),
            BurnState::Spent => assert!(b.char >= 0.85, "seed {seed}: a spent torch is barely burnt"),
            _ => assert!(b.char > 0.0, "seed {seed}: lit once but not charred"),
        }
        match s.head.style {
            TorchStyle::Cresset => {
                assert!(s.handle.ferrule, "seed {seed}: a cresset without its socket");
                assert_eq!(s.head.bindings, 0, "seed {seed}");
            }
            TorchStyle::Bundle => {
                assert!(s.head.bindings > 0, "seed {seed}: an unbound bundle");
                assert!(matches!(s.head.binding, MaterialKind::Cord | MaterialKind::Leather), "seed {seed}");
            }
            TorchStyle::Rag => assert_ne!(s.head.fuel, Fuel::Resin, "seed {seed}: a resin rag"),
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&TorchState) -> bool| all.iter().any(f);
    for style in TorchStyle::ALL {
        assert!(any(&|s| s.head.style == style), "no {style:?}");
        for state in BurnState::ALL {
            assert!(any(&|s| s.head.style == style && s.burn.state == state), "no {state:?} {style:?}");
        }
    }
    for fuel in Fuel::ALL {
        assert!(any(&|s| s.head.fuel == fuel), "no {fuel:?}");
    }
    for grip in HandleGrip::ALL {
        assert!(any(&|s| s.handle.grip == grip), "no {grip:?} grip");
    }
    assert!(any(&|s| s.handle.facets <= 8) && any(&|s| s.handle.facets > 8));
    assert!(any(&|s| s.head.style == TorchStyle::Rag && s.handle.ferrule));
    assert!(any(&|s| s.head.style == TorchStyle::Rag && s.head.binding == MaterialKind::Iron && s.head.bindings > 0));
}

#[test]
fn nouns() {
    let mut s = TorchState::default();
    let cases = [
        (TorchStyle::Rag, Fuel::Pitch, "Pitch Torch"),
        (TorchStyle::Rag, Fuel::Tallow, "Tallow Torch"),
        (TorchStyle::Rag, Fuel::Resin, "Torch"),
        (TorchStyle::Bundle, Fuel::Resin, "Splint Torch"),
        (TorchStyle::Cresset, Fuel::Pitch, "Cresset Torch"),
    ];
    for (style, fuel, name) in cases {
        s.head.style = style;
        s.head.fuel = fuel;
        assert_eq!(Torch::noun(&s), name);
    }
    s.burn.state = BurnState::Spent;
    assert_eq!(Torch::noun(&s), "Torch Stub");
}
