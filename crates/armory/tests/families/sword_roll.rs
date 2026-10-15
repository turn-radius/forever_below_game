//! Sword "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::sword::{LIMITS, Sword, SwordArchetype, SwordState};
use armory::sections::blade::{BladeSection, GuardStyle, GuardTerminal};
use armory::sections::hafted::{PommelGem, PommelStyle};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<SwordState> {
    range.map(Sword::roll).collect()
}

#[test]
fn same_seed_same_sword() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Sword::roll(seed), Sword::roll(seed));
        assert_eq!(Sword::roll(seed).seed, seed);
    }
    assert_ne!(Sword::roll(1), Sword::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Sword::roll(1207);
    let back: SwordState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 777] {
        let s = Sword::roll(seed);
        let found = SwordArchetype::WEIGHTS.iter().any(|(a, _)| Sword::roll_as(seed, *a) == s);
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
        assert!(s.grip.on, "seed {seed}: no grip");
        assert!((s.grip.length - s.shaft.length).abs() < 1e-3, "seed {seed}: grip shorter than the handle");
        assert!(s.pommel.on, "seed {seed}: no pommel");
        if s.wrap.on {
            assert_eq!(s.grip.rivets, 0, "seed {seed}: rivets under a wrap");
        }
        if s.pommel.gem == PommelGem::Ring {
            assert!(s.pommel.style.has_face(), "seed {seed}: stone ring on {:?}", s.pommel.style);
        }
        assert_ne!(s.guard.style, GuardStyle::Lobes, "seed {seed}: ballock lobes on a sword");
        assert!(s.blade.width <= s.blade.length * 0.12 + 0.003, "seed {seed}: blade too wide");
        if s.blade.spine <= 0.5 {
            assert!(s.blade.curve <= 0.06, "seed {seed}: curved two-edged blade");
        }
        if s.guard.on && s.guard.style.has_arms() {
            assert!(s.guard.span >= s.blade.width * 0.6, "seed {seed}: guard narrower than the blade");
        }
        assert!(s.cond.wear == 0.0 && s.cond.damage == 0.0, "seed {seed}: forged worn");
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&SwordState) -> bool| all.iter().any(f);
    for section in BladeSection::ALL {
        assert!(any(&|s| s.blade.section == section), "no {section:?} blade");
    }
    for style in [GuardStyle::Cross, GuardStyle::Bar, GuardStyle::Disc] {
        assert!(any(&|s| s.guard.on && s.guard.style == style), "no {style:?} guard");
    }
    for terminal in GuardTerminal::ALL {
        assert!(any(&|s| s.guard.on && s.guard.style.has_arms() && s.guard.terminal == terminal), "no {terminal:?} terminals");
    }
    for style in [PommelStyle::Wheel, PommelStyle::ScentStopper, PommelStyle::Ball, PommelStyle::Disc, PommelStyle::Flared, PommelStyle::Ferrule] {
        assert!(any(&|s| s.pommel.style == style), "no {style:?} pommel");
    }
    assert!(any(&|s| !s.guard.on), "always a guard");
    assert!(any(&|s| s.guard.on && s.guard.langets > 0.0), "no langets");
    assert!(any(&|s| s.guard.on && s.guard.block > 0.0), "no centre block");
    assert!(any(&|s| s.guard.on && s.guard.curl > 0.0), "no curled quillons");
    assert!(any(&|s| s.guard.on && s.guard.s_curve), "no s-curved guard");
    assert!(any(&|s| s.guard.on && s.guard.gem), "no guard stone");
    assert!(any(&|s| s.pommel.gem == PommelGem::EndStone), "no pommel end stone");
    assert!(any(&|s| s.pommel.gem == PommelGem::Ring), "no pommel stone ring");
    assert!(any(&|s| s.wrap.on && s.wrap.cross), "no cross-wound grip");
    assert!(any(&|s| !s.wrap.on), "always wrapped");
    assert!(any(&|s| s.grip.rivets > 0), "no riveted grip");
    assert!(any(&|s| s.blade.ricasso > 0.0), "no ricasso");
    assert!(any(&|s| s.blade.fuller > 0.0 && s.blade.fullers > 1), "no double fuller");
    assert!(any(&|s| s.etch.on), "no etching");
    for name in [
        "Arming Sword", "Longsword", "Greatsword", "Estoc", "Short Sword", "Gladius", "Falchion", "Messer", "Sabre", "Flamberge",
        "Flame-bladed Sword",
    ] {
        assert!(all.iter().any(|s| Sword::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = SwordState::default();
    assert_eq!(Sword::noun(&s), "Arming Sword");
    s.shaft.length = 2.3;
    assert_eq!(Sword::noun(&s), "Longsword");
    s.shaft.length = 3.2;
    assert_eq!(Sword::noun(&s), "Greatsword");
    s.shaft.length = 1.3;
    s.blade.length = 11.0;
    assert_eq!(Sword::noun(&s), "Greatsword");
    s.blade.length = 8.0;
    s.blade.width = 0.25;
    s.blade.taper = 0.7;
    assert_eq!(Sword::noun(&s), "Estoc");

    let mut s = SwordState::default();
    s.blade.length = 5.0;
    assert_eq!(Sword::noun(&s), "Short Sword");
    s.blade.belly = 0.5;
    s.blade.belly_width = 1.1;
    assert_eq!(Sword::noun(&s), "Gladius");

    let mut s = SwordState::default();
    s.blade.spine = 0.9;
    assert_eq!(Sword::noun(&s), "Messer");
    s.blade.belly_width = 1.4;
    assert_eq!(Sword::noun(&s), "Falchion");
    s.blade.curve = 0.5;
    assert_eq!(Sword::noun(&s), "Sabre", "a curve outranks a broad belly");

    let mut s = SwordState::default();
    s.blade.waves = 8;
    s.blade.wave_depth = 0.1;
    s.blade.length = 11.0;
    assert_eq!(Sword::noun(&s), "Flamberge");
    s.blade.length = 7.0;
    assert_eq!(Sword::noun(&s), "Flame-bladed Sword");
    s.blade.wave_depth = 0.02;
    assert_eq!(Sword::noun(&s), "Arming Sword", "shallow waves are not flames");
}
