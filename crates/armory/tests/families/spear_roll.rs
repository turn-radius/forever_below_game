//! Spear "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::spear::{LIMITS, Spear, SpearArchetype, SpearState};
use armory::sections::hafted::{PommelGem, PommelStyle};
use armory::sections::blade::{BladeSection, GuardMount, GuardTerminal};
use armory::sections::spearhead::LugStyle;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<SpearState> {
    range.map(Spear::roll).collect()
}

#[test]
fn same_seed_same_spear() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Spear::roll(seed), Spear::roll(seed));
        assert_eq!(Spear::roll(seed).seed, seed);
    }
    assert_ne!(Spear::roll(1), Spear::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Spear::roll(1207);
    let back: SpearState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 512] {
        let s = Spear::roll(seed);
        let found = SpearArchetype::WEIGHTS.iter().any(|(a, _)| Spear::roll_as(seed, *a) == s);
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
        assert!(s.socket.on, "seed {seed}: a spearhead without a socket");
        let r = s.shaft.radius;
        assert!(s.pommel.radius >= r * 0.95 - 0.003 && s.pommel.radius <= r * 1.4 + 0.003, "seed {seed}: butt cap out of proportion");
        assert!(matches!(s.pommel.style, PommelStyle::Ferrule | PommelStyle::Spike), "seed {seed}: {:?} butt", s.pommel.style);
        assert_eq!(s.pommel.gem, PommelGem::None, "seed {seed}: a stone in a spear butt");
        if s.lugs.on {
            assert!(s.blade.width >= 0.22, "seed {seed}: lugs under a needle head");
        }
        if s.blade.width < 0.22 {
            assert!(s.blade.thickness >= s.blade.width * 0.7, "seed {seed}: a flat needle");
        }
        if s.wrap.on && s.wrap.material != armory::MaterialKind::Leather {
            let pitch = s.grip.length * s.wrap.coverage / s.wrap.turns as f32 / if s.wrap.cross { 2.0 } else { 1.0 };
            assert!(pitch < s.wrap.cord * 3.0, "seed {seed}: cord wound loose");
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&SpearState) -> bool| all.iter().any(f);
    for section in [BladeSection::Lenticular, BladeSection::Diamond, BladeSection::Hexagonal, BladeSection::Flat] {
        assert!(any(&|s| s.blade.section == section), "no {section:?} blade");
    }
    for style in [LugStyle::Wings, LugStyle::Bar] {
        assert!(any(&|s| s.lugs.on && s.lugs.style == style), "no {style:?}");
    }
    for mount in [GuardMount::Socket, GuardMount::Blade] {
        assert!(any(&|s| s.lugs.on && s.lugs.mount == mount), "no lugs on the {mount:?}");
    }
    for ends in [GuardTerminal::None, GuardTerminal::Ball, GuardTerminal::Disc, GuardTerminal::Flare] {
        assert!(any(&|s| s.lugs.on && s.lugs.ends == ends), "no {ends:?} lug ends");
    }
    for style in [PommelStyle::Ferrule, PommelStyle::Spike] {
        assert!(any(&|s| s.pommel.on && s.pommel.style == style), "no {style:?} butt cap");
    }
    assert!(any(&|s| !s.pommel.on), "always a butt cap");
    assert!(any(&|s| s.blade.waves > 0), "no flame blade");
    assert!(any(&|s| s.blade.midrib > 0.0), "no midrib");
    assert!(any(&|s| s.blade.fuller > 0.0 && s.blade.fullers == 2), "no twin fullers");
    assert!(any(&|s| s.socket.langets > 0), "no langets");
    assert!(any(&|s| s.socket.rings > 0) && any(&|s| s.socket.rings == 0), "socket rings never vary");
    assert!(any(&|s| s.socket.facets <= 12) && any(&|s| s.socket.facets > 12), "socket faceting never varies");
    assert!(any(&|s| s.grip.on) && any(&|s| s.wrap.on), "never held by a grip or wrap");
    assert!(any(&|s| s.etch.on), "no etching");
    for name in ["Awl Pike", "Bodkin Spear", "Partisan", "Boar Spear", "Winged Spear", "Lugged Spear", "Pike", "Javelin", "Leaf-bladed Spear", "Spear"] {
        assert!(all.iter().any(|s| Spear::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = SpearState::default();
    s.lugs.on = false;
    s.blade.width = 0.5;
    s.blade.belly = 0.2;
    s.shaft.length = 15.0;
    assert_eq!(Spear::noun(&s), "Spear");
    s.blade.belly = 0.26;
    assert_eq!(Spear::noun(&s), "Leaf-bladed Spear");
    s.shaft.length = 10.9;
    assert_eq!(Spear::noun(&s), "Javelin");
    s.shaft.length = 18.1;
    assert_eq!(Spear::noun(&s), "Pike");

    s.lugs.on = true;
    s.lugs.style = LugStyle::Wings;
    s.lugs.mount = GuardMount::Socket;
    s.lugs.curve = 0.2;
    assert_eq!(Spear::noun(&s), "Winged Spear", "lugs win over length");
    s.lugs.mount = GuardMount::Blade;
    assert_eq!(Spear::noun(&s), "Lugged Spear");
    s.lugs.style = LugStyle::Bar;
    assert_eq!(Spear::noun(&s), "Boar Spear");
    s.lugs.curve = 0.51;
    assert_eq!(Spear::noun(&s), "Partisan", "upswept lugs are flukes");

    s.blade.width = 0.2;
    s.shaft.length = 15.0;
    assert_eq!(Spear::noun(&s), "Bodkin Spear", "a needle head wins over everything");
    s.shaft.length = 18.0;
    assert_eq!(Spear::noun(&s), "Bodkin Spear");
    s.shaft.length = 18.05;
    assert_eq!(Spear::noun(&s), "Awl Pike");
}
