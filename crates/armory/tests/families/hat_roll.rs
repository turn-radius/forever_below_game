//! Hats "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::hat::{HatState, Hats, LIMITS};
use armory::sections::hat::HatStyle;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<HatState> {
    range.map(Hats::roll).collect()
}

#[test]
fn same_seed_same_hat() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Hats::roll(seed), Hats::roll(seed));
        assert_eq!(Hats::roll(seed).seed, seed);
    }
    assert_ne!(Hats::roll(1), Hats::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Hats::roll(1207);
    let back: HatState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 1207] {
        let s = Hats::roll(seed);
        assert_eq!(Hats::roll_as(seed, s.hat.style), s);
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
        let (seed, h) = (s.seed, &s.hat);
        if h.feather {
            assert!(h.band > 0.0, "seed {seed}: a feather with no band to hold it");
        }
        match h.style {
            HatStyle::Pointed => {
                assert!(h.crown > 2.5, "seed {seed}: a low pointed hat");
                assert!(h.brim > 0.5, "seed {seed}: a pointed hat with no brim");
            }
            HatStyle::WideBrim => assert!(h.brim > 1.2 && h.crown < 2.0, "seed {seed}"),
            HatStyle::Cap => {
                assert!(h.brim < 0.5, "seed {seed}: a cap with a broad brim");
                if h.brim > 0.0 {
                    assert!(h.droop < -0.7, "seed {seed}: a cap's brim not turned up");
                }
            }
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&HatState) -> bool| all.iter().any(f);
    for style in HatStyle::ALL {
        assert!(any(&|s| s.hat.style == style), "no {style:?}");
    }
    assert!(any(&|s| s.hat.band > 0.0), "no band");
    assert!(any(&|s| s.hat.feather), "no feather");
    for name in ["Pointed Hat", "Plumed Hat", "Slouch Hat", "Wide-brimmed Hat", "Phrygian Cap", "Turned-brim Cap", "Felt Cap"] {
        assert!(all.iter().any(|s| Hats::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = HatState::default();
    assert_eq!(Hats::noun(&s), "Pointed Hat");
    s.hat.style = HatStyle::WideBrim;
    s.hat.droop = 0.1;
    assert_eq!(Hats::noun(&s), "Wide-brimmed Hat");
    s.hat.droop = 0.6;
    assert_eq!(Hats::noun(&s), "Slouch Hat");
    s.hat.feather = true;
    assert_eq!(Hats::noun(&s), "Plumed Hat");
    s.hat.style = HatStyle::Cap;
    s.hat.bend = 0.1;
    s.hat.brim = 0.0;
    assert_eq!(Hats::noun(&s), "Felt Cap");
    s.hat.brim = 0.3;
    assert_eq!(Hats::noun(&s), "Turned-brim Cap");
    s.hat.bend = 0.8;
    assert_eq!(Hats::noun(&s), "Phrygian Cap");
}
