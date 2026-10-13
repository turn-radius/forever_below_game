//! Belts "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::bags::BagStyle;
use armory::families::belt::{BeltState, Belts, LIMITS};
use armory::material::MaterialKind;
use armory::sections::belt::BeltStyle;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<BeltState> {
    range.map(Belts::roll).collect()
}

#[test]
fn same_seed_same_belt() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Belts::roll(seed), Belts::roll(seed));
        assert_eq!(Belts::roll(seed).seed, seed);
    }
    assert_ne!(Belts::roll(1), Belts::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Belts::roll(1207);
    let back: BeltState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 1207] {
        let s = Belts::roll(seed);
        assert_eq!(Belts::roll_as(seed, s.fittings.style), s);
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
        let (seed, f) = (s.seed, &s.fittings);
        assert!(s.belt.on, "seed {seed}");
        assert_eq!(s.belt.material, MaterialKind::Leather, "seed {seed}");
        assert_eq!(s.pouch.style, BagStyle::BeltPouch, "seed {seed}");
        assert_eq!(f.frogs > 0, f.style == BeltStyle::Sword, "seed {seed}: frogs on {:?}", f.style);
        match f.style {
            BeltStyle::Girdle => assert!(f.tail > 1.5 && f.tip && s.belt.width < 0.3, "seed {seed}: girdle"),
            BeltStyle::Studded => assert!(f.studs >= 12, "seed {seed}: few studs"),
            BeltStyle::Pouch => assert!(f.loops >= 2 && f.rings >= 1, "seed {seed}: pouch belt without loops"),
            _ => assert!(f.tail < 1.0, "seed {seed}: long tail on {:?}", f.style),
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&BeltState) -> bool| all.iter().any(f);
    for style in BeltStyle::ALL {
        assert!(any(&|s| s.fittings.style == style), "no {style:?}");
    }
    assert!(any(&|s| s.fittings.frogs == 2), "no second frog");
    assert!(any(&|s| s.fittings.pouch), "no pouch");
    assert!(any(&|s| !s.fittings.pouch), "always a pouch");
    assert!(any(&|s| s.fittings.tip && s.fittings.style != BeltStyle::Girdle), "no strap end on a short tail");
    assert!(any(&|s| !s.fittings.keeper), "always a keeper");
    assert!(any(&|s| s.etch.on), "no etching");
    for name in ["Belt", "Wide Belt", "Studded Belt", "Sword Belt", "Sword and Dagger Belt", "Girdle", "Pouch Belt"] {
        assert!(all.iter().any(|s| Belts::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = BeltState::default();
    assert_eq!(Belts::noun(&s), "Belt");
    s.belt.width = 0.6;
    assert_eq!(Belts::noun(&s), "Wide Belt");
    for (style, name) in [(BeltStyle::Studded, "Studded Belt"), (BeltStyle::Sword, "Sword Belt"), (BeltStyle::Girdle, "Girdle"), (BeltStyle::Pouch, "Pouch Belt")] {
        s.fittings.style = style;
        assert_eq!(Belts::noun(&s), name);
    }
    s.fittings.style = BeltStyle::Sword;
    s.fittings.frogs = 2;
    assert_eq!(Belts::noun(&s), "Sword and Dagger Belt");
}
