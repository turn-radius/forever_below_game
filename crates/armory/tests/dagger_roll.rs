//! Dagger "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::dagger::{Dagger, DaggerArchetype, DaggerState, LIMITS};
use armory::sections::blade::{BladeSection, GuardStyle, GuardTerminal};
use armory::sections::hafted::{PommelGem, PommelStyle};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<DaggerState> {
    range.map(Dagger::roll).collect()
}

#[test]
fn same_seed_same_dagger() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Dagger::roll(seed), Dagger::roll(seed));
        assert_eq!(Dagger::roll(seed).seed, seed);
    }
    assert_ne!(Dagger::roll(1), Dagger::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Dagger::roll(1207);
    let back: DaggerState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 777] {
        let s = Dagger::roll(seed);
        let found = DaggerArchetype::WEIGHTS.iter().any(|(a, _)| Dagger::roll_as(seed, *a) == s);
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
        assert!(s.grip.on && s.pommel.on, "seed {seed}: no grip or pommel");
        assert!((s.grip.length - s.shaft.length).abs() < 1e-3, "seed {seed}: grip shorter than the handle");
        if s.wrap.on {
            assert_eq!(s.grip.rivets, 0, "seed {seed}: rivets under a wrap");
        }
        if s.pommel.gem == PommelGem::Ring {
            assert!(s.pommel.style.has_face(), "seed {seed}: stone ring on {:?}", s.pommel.style);
        }
        assert!(s.blade.width <= s.blade.length * 0.32 + 0.003, "seed {seed}: blade too wide");
        assert_eq!(s.blade.waves, 0, "seed {seed}: flame-bladed dagger");
        if s.guard.on && s.guard.style == GuardStyle::Disc {
            assert_eq!(s.pommel.style, PommelStyle::Disc, "seed {seed}: a rondel guard with no rondel pommel");
        }
        if s.blade.spine > 0.5 {
            assert!(!s.guard.on || s.guard.style.has_arms(), "seed {seed}: a knife with a {:?} guard", s.guard.style);
        }
        assert!(s.cond.wear == 0.0 && s.cond.damage == 0.0, "seed {seed}: forged worn");
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&DaggerState) -> bool| all.iter().any(f);
    for section in BladeSection::ALL {
        assert!(any(&|s| s.blade.section == section), "no {section:?} blade");
    }
    for style in GuardStyle::ALL {
        assert!(any(&|s| s.guard.on && s.guard.style == style), "no {style:?} guard");
    }
    for terminal in GuardTerminal::ALL {
        assert!(any(&|s| s.guard.on && s.guard.style.has_arms() && s.guard.terminal == terminal), "no {terminal:?} terminals");
    }
    for style in [PommelStyle::Disc, PommelStyle::Ball, PommelStyle::ScentStopper, PommelStyle::Ferrule, PommelStyle::Wheel, PommelStyle::Flared] {
        assert!(any(&|s| s.pommel.style == style), "no {style:?} pommel");
    }
    assert!(any(&|s| !s.guard.on), "always a guard");
    assert!(any(&|s| s.guard.on && s.guard.langets > 0.0), "no langets");
    assert!(any(&|s| s.guard.on && s.guard.block > 0.0), "no centre block");
    assert!(any(&|s| s.guard.on && s.guard.gem), "no guard stone");
    assert!(any(&|s| s.pommel.gem == PommelGem::EndStone), "no pommel end stone");
    assert!(any(&|s| s.wrap.on), "never wrapped");
    assert!(any(&|s| s.grip.rivets > 0), "no riveted grip");
    assert!(any(&|s| s.blade.fullers > 1 && s.blade.fuller > 0.0), "no stacked fullers");
    assert!(any(&|s| s.etch.on), "no etching");
    for name in ["Ballock Dagger", "Rondel Dagger", "Stiletto", "Seax", "Knife", "Bauernwehr", "Cinquedea", "Quillon Dagger", "Dirk"] {
        assert!(all.iter().any(|s| Dagger::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = DaggerState::default();
    assert_eq!(Dagger::noun(&s), "Quillon Dagger");
    s.guard.on = false;
    assert_eq!(Dagger::noun(&s), "Dirk");
    s.guard.on = true;
    s.guard.style = GuardStyle::Lobes;
    assert_eq!(Dagger::noun(&s), "Ballock Dagger");
    s.guard.style = GuardStyle::Disc;
    assert_eq!(Dagger::noun(&s), "Quillon Dagger", "a rondel needs both discs");
    s.pommel.style = PommelStyle::Disc;
    assert_eq!(Dagger::noun(&s), "Rondel Dagger");

    let mut s = DaggerState::default();
    s.blade.width = 0.12;
    assert_eq!(Dagger::noun(&s), "Stiletto");

    let mut s = DaggerState::default();
    s.blade.spine = 1.0;
    s.blade.clip = 0.6;
    assert_eq!(Dagger::noun(&s), "Seax");
    s.blade.clip = 0.75;
    s.blade.length = 2.2;
    assert_eq!(Dagger::noun(&s), "Knife");
    s.blade.length = 3.4;
    assert_eq!(Dagger::noun(&s), "Bauernwehr");

    let mut s = DaggerState::default();
    s.blade.shoulder = 1.15;
    s.blade.point_start = 0.1;
    assert_eq!(Dagger::noun(&s), "Cinquedea");
}
