//! Hoods, cowls and coifs "Forge new": deterministic, in range, varied and coherent.

use armory::fabric::FabricPattern;
use armory::families::Generator;
use armory::families::hood::{HoodState, Hoods, LIMITS};
use armory::sections::hood::HoodStyle;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<HoodState> {
    range.map(Hoods::roll).collect()
}

#[test]
fn same_seed_same_hood() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Hoods::roll(seed), Hoods::roll(seed));
        assert_eq!(Hoods::roll(seed).seed, seed);
    }
    assert_ne!(Hoods::roll(1), Hoods::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Hoods::roll(1207);
    let back: HoodState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 1207] {
        let s = Hoods::roll(seed);
        assert_eq!(Hoods::roll_as(seed, s.hood.style), s);
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
        let (seed, h) = (s.seed, &s.hood);
        if h.liripipe > 0.0 || h.dags > 0 {
            assert_eq!(h.style, HoodStyle::Hood, "seed {seed}: tail or dags on {:?}", h.style);
        }
        if h.liripipe > 0.0 {
            assert!(h.peak > 0.5, "seed {seed}: a tail without a point");
        }
        assert_eq!(h.ties, h.ties && h.style == HoodStyle::Coif, "seed {seed}: ties on {:?}", h.style);
        assert_eq!(h.ventail, h.ventail && h.style == HoodStyle::MailCoif, "seed {seed}: ventail on {:?}", h.style);
        match h.style {
            HoodStyle::Coif => assert_eq!(h.cape, 0.0, "seed {seed}: a coif with a cape"),
            HoodStyle::MailCoif => assert_eq!(h.cloth.pattern, FabricPattern::Plain, "seed {seed}"),
            _ => {}
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&HoodState) -> bool| all.iter().any(f);
    for style in HoodStyle::ALL {
        assert!(any(&|s| s.hood.style == style), "no {style:?}");
    }
    assert!(any(&|s| s.hood.liripipe > 0.0), "no liripipe");
    assert!(any(&|s| s.hood.dags > 0), "no dagged edge");
    assert!(any(&|s| s.hood.ties), "no ties");
    assert!(any(&|s| s.hood.ventail), "no ventail");
    assert!(any(&|s| s.hem.on && s.hem.hemline), "no trimmed cape");
    for name in ["Hood", "Liripipe Hood", "Dagged Hood", "Hooded Mantle", "Cowl", "Coif", "Arming Cap", "Mail Coif", "Ventailed Mail Coif"] {
        assert!(all.iter().any(|s| Hoods::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = HoodState::default();
    assert_eq!(Hoods::noun(&s), "Hood");
    s.hood.cape = 2.2;
    assert_eq!(Hoods::noun(&s), "Hooded Mantle");
    s.hood.dags = 8;
    assert_eq!(Hoods::noun(&s), "Dagged Hood");
    s.hood.liripipe = 3.0;
    assert_eq!(Hoods::noun(&s), "Liripipe Hood");
    s.hood.style = HoodStyle::Cowl;
    assert_eq!(Hoods::noun(&s), "Cowl");
    s.hood.style = HoodStyle::Coif;
    assert_eq!(Hoods::noun(&s), "Coif");
    s.hood.cloth.pattern = FabricPattern::Quilted;
    assert_eq!(Hoods::noun(&s), "Arming Cap");
    s.hood.style = HoodStyle::MailCoif;
    assert_eq!(Hoods::noun(&s), "Mail Coif");
    s.hood.ventail = true;
    assert_eq!(Hoods::noun(&s), "Ventailed Mail Coif");
}
