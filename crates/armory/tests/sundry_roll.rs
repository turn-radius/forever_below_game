//! Sundry "Forge new": deterministic, in range, varied and coherent.

use armory::build::key::KeyBow;
use armory::families::Generator;
use armory::families::sundry::{LIMITS, Sundry, SundryKind, SundryState, WAXES};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<SundryState> {
    range.map(Sundry::roll).collect()
}

#[test]
fn same_seed_same_sundry() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Sundry::roll(seed), Sundry::roll(seed));
        assert_eq!(Sundry::roll(seed).seed, seed);
    }
    assert_ne!(Sundry::roll(1), Sundry::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Sundry::roll(1207);
    let back: SundryState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 77] {
        let s = Sundry::roll(seed);
        assert_eq!(Sundry::roll_as(seed, s.article.kind), s, "seed {seed}");
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
        let a = &s.article;
        if a.kind != SundryKind::Candle {
            assert!(!a.lit && !a.holder && a.burnt == 0.0, "seed {}: a {:?} burns", s.seed, a.kind);
        } else {
            assert!(!a.lit || a.burnt > 0.0, "seed {}: lit but never burnt", s.seed);
        }
        if a.kind != SundryKind::Keys {
            assert_eq!(a.bow, KeyBow::Ring, "seed {}", s.seed);
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&SundryState) -> bool| all.iter().any(f);
    for kind in SundryKind::ALL {
        assert!(any(&|s| s.article.kind == kind), "no {kind:?}");
    }
    for bow in KeyBow::ALL {
        assert!(any(&|s| s.article.kind == SundryKind::Keys && s.article.bow == bow), "no {bow:?} keys");
    }
    for wax in WAXES {
        assert!(any(&|s| s.article.kind == SundryKind::Candle && s.article.color == wax), "no {wax:?} candle");
    }
    let candle = |f: &dyn Fn(&SundryState) -> bool| any(&|s| s.article.kind == SundryKind::Candle && f(s));
    assert!(candle(&|s| s.article.lit) && candle(&|s| !s.article.lit && s.article.burnt > 0.0) && candle(&|s| s.article.burnt == 0.0));
    assert!(candle(&|s| s.article.holder) && candle(&|s| !s.article.holder));
    assert!((1..=2).all(|n| any(&|s| s.article.kind == SundryKind::Tinderbox && s.article.count == n)));
    assert!(any(&|s| s.article.kind == SundryKind::Keys && s.article.count == 1));
}

#[test]
fn nouns() {
    let mut s = SundryState::default();
    let cases = [
        (SundryKind::Rope, "Coil of Rope"),
        (SundryKind::Lockpicks, "Lockpicks"),
        (SundryKind::Tinderbox, "Tinderbox"),
        (SundryKind::Bedroll, "Bedroll"),
        (SundryKind::Bandages, "Bandages"),
    ];
    for (kind, name) in cases {
        s.article.kind = kind;
        assert_eq!(Sundry::noun(&s), name);
    }
    s.article.kind = SundryKind::Keys;
    s.article.count = 3;
    assert_eq!(Sundry::noun(&s), "Ring of Keys");
    s.article.count = 1;
    assert_eq!(Sundry::noun(&s), "Key on a Ring");
    s.article.kind = SundryKind::Candle;
    for (wax, name) in [(WAXES[0], "Tallow Candle"), (WAXES[1], "Beeswax Candle"), (WAXES[2], "Candle")] {
        s.article.color = wax;
        assert_eq!(Sundry::noun(&s), name);
    }
    s.article.holder = true;
    assert_eq!(Sundry::noun(&s), "Chamberstick");
}
