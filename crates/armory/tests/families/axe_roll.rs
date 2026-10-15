//! Axe "Forge new": deterministic, in range, varied and coherent.

use armory::MaterialKind;
use armory::families::Generator;
use armory::families::axe::{Axe, AxeArchetype, AxeState, LIMITS};
use armory::sections::axe_head::AxePoll;
use armory::sections::blade::BladeSection;
use armory::sections::hafted::{PommelGem, PommelStyle};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<AxeState> {
    range.map(Axe::roll).collect()
}

#[test]
fn same_seed_same_axe() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Axe::roll(seed), Axe::roll(seed));
        assert_eq!(Axe::roll(seed).seed, seed);
    }
    assert_ne!(Axe::roll(1), Axe::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Axe::roll(1207);
    let back: AxeState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 512] {
        let s = Axe::roll(seed);
        let found = AxeArchetype::WEIGHTS.iter().any(|(a, _)| Axe::roll_as(seed, *a) == s);
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
        assert!(s.eye.on, "seed {seed}: a head with no eye");
        assert!(s.eye.length >= s.head.root - 1e-4, "seed {seed}: eye {} shorter than the bit's root {}", s.eye.length, s.head.root);
        assert!(s.head.reach > s.shaft.radius * s.eye.flare * 2.0, "seed {seed}: the bit does not clear the eye");
        assert_eq!(s.pommel.gem, PommelGem::None, "seed {seed}: a stone in an axe");
        if s.pommel.on && s.pommel.style == PommelStyle::Ferrule {
            assert_eq!(s.pommel.material, MaterialKind::Iron, "seed {seed}: a ferrule not of iron");
        }
        if s.shaft.material == MaterialKind::Iron {
            assert_eq!(s.head.poll, AxePoll::Spike, "seed {seed}: an iron haft on a {:?} axe", s.head.poll);
        }
        if s.eye.langets > 0 {
            assert_eq!(s.eye.langets, 2, "seed {seed}: langets not in pairs");
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&AxeState) -> bool| all.iter().any(f);
    for section in BladeSection::ALL {
        assert!(any(&|s| s.head.section == section), "no {section:?} bit");
    }
    for poll in AxePoll::ALL {
        assert!(any(&|s| s.head.poll == poll), "no {poll:?} poll");
    }
    for style in [PommelStyle::Flared, PommelStyle::Ball, PommelStyle::Ferrule] {
        assert!(any(&|s| s.pommel.on && s.pommel.style == style), "no {style:?} butt");
    }
    assert!(any(&|s| !s.pommel.on), "always a butt knob");
    assert!(any(&|s| s.eye.langets > 0), "no langets");
    assert!(any(&|s| s.eye.rings > 0) && any(&|s| s.eye.rings == 0), "eye rings never vary");
    assert!(any(&|s| s.eye.facets <= 12) && any(&|s| s.eye.facets > 12), "eye faceting never varies");
    assert!(any(&|s| s.shaft.material == MaterialKind::Iron), "no iron haft");
    assert!(any(&|s| s.grip.on) && any(&|s| s.wrap.on), "never held by a grip or wrap");
    assert!(any(&|s| s.etch.on), "no etching");
    for name in ["Battle Axe", "Horseman's Axe", "Long Axe", "Throwing Axe", "Bearded Axe", "Broad Axe", "Hatchet", "Hand Axe"] {
        assert!(all.iter().any(|s| Axe::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = AxeState::default();
    assert_eq!(Axe::noun(&s), "Hand Axe");
    s.shaft.length = 3.0;
    assert_eq!(Axe::noun(&s), "Hatchet");
    s.head.toe = 0.5;
    s.head.beard = 0.4;
    assert_eq!(Axe::noun(&s), "Broad Axe", "a broad edge wins over a short haft");
    s.head.toe = 0.1;
    assert_eq!(Axe::noun(&s), "Bearded Axe", "a beard with little toe");
    s.head.droop = 0.3;
    assert_eq!(Axe::noun(&s), "Throwing Axe");
    s.shaft.length = 10.0;
    assert_eq!(Axe::noun(&s), "Long Axe", "a long haft wins over the head");
    s.head.poll = AxePoll::Spike;
    assert_eq!(Axe::noun(&s), "Horseman's Axe", "the poll wins over everything");
    s.head.poll = AxePoll::Bit;
    assert_eq!(Axe::noun(&s), "Battle Axe");
    s.head.poll = AxePoll::Hammer;
    assert_eq!(Axe::noun(&s), "Long Axe", "a hammer poll names nothing");
}
