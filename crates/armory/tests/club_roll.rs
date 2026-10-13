//! Club "Forge new": deterministic, in range, varied and coherent.

use armory::MaterialKind;
use armory::families::Generator;
use armory::families::club::{Club, ClubArchetype, ClubState, LIMITS};
use armory::sections::hafted::PommelGem;
use armory::sections::head::{HeadStyle, SpikePattern};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<ClubState> {
    range.map(Club::roll).collect()
}

#[test]
fn same_seed_same_club() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Club::roll(seed), Club::roll(seed));
        assert_eq!(Club::roll(seed).seed, seed);
    }
    assert_ne!(Club::roll(1), Club::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Club::roll(1207);
    let back: ClubState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 512] {
        let s = Club::roll(seed);
        let found = ClubArchetype::WEIGHTS.iter().any(|(a, _)| Club::roll_as(seed, *a) == s);
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
        assert_eq!(s.shaft.material, MaterialKind::Wood, "seed {seed}: a club not of wood");
        assert!(s.swell.on != s.has_knob(), "seed {seed}: a club needs a swelling or a knob, not both");
        if s.has_knob() {
            assert_eq!(s.head.material, s.shaft.material, "seed {seed}: a knob of another material");
        }
        assert_eq!(s.pommel.gem, PommelGem::None, "seed {seed}: a stone in a club");
        if s.spikes.on {
            assert!(s.swell.on, "seed {seed}: studs without a swelling");
        }
        if s.wrap.on && s.wrap.material == MaterialKind::Cord {
            assert!(s.wrap.cord <= 0.025, "seed {seed}: cord {} thick", s.wrap.cord);
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&ClubState) -> bool| all.iter().any(f);
    for style in [HeadStyle::Orb, HeadStyle::Pear, HeadStyle::Drum] {
        assert!(any(&|s| s.head.style == style), "no {style:?} knob");
    }
    for pattern in [SpikePattern::Aligned, SpikePattern::Staggered, SpikePattern::Spiral, SpikePattern::GoldenAngle] {
        assert!(any(&|s| s.spikes.on && s.spikes.pattern == pattern), "no {pattern:?} studs");
    }
    for m in [MaterialKind::Iron, MaterialKind::Brass, MaterialKind::Bone] {
        assert!(any(&|s| s.spikes.on && s.spikes.material == m), "no {m:?} studs");
    }
    assert!(any(&|s| s.grip.on) && any(&|s| s.wrap.on) && any(&|s| s.wrap.cross), "never held by a grip or wrap");
    assert!(any(&|s| s.pommel.on) && any(&|s| !s.pommel.on), "butts never vary");
    assert!(any(&|s| s.etch.on), "no carving");
    for name in ["Bone-studded Club", "Spiked Club", "Studded Club", "Shillelagh", "Knobbed Club", "Iron-bound Club", "Gnarled Club", "Cudgel", "Club"] {
        assert!(all.iter().any(|s| Club::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = ClubState::default();
    s.shaft.length = 4.5;
    assert_eq!(Club::noun(&s), "Club");
    s.shaft.length = 3.0;
    assert_eq!(Club::noun(&s), "Cudgel");
    s.shaft.gnarl = 0.3;
    assert_eq!(Club::noun(&s), "Gnarled Club", "knots win over length");
    s.shaft.bands = 2;
    assert_eq!(Club::noun(&s), "Iron-bound Club", "bands win over knots");
    s.head.style = HeadStyle::Orb;
    assert_eq!(Club::noun(&s), "Shillelagh", "a knotted shaft with a knob");
    s.shaft.gnarl = 0.0;
    assert_eq!(Club::noun(&s), "Knobbed Club");

    s.spikes.on = true;
    s.spikes.material = MaterialKind::Iron;
    s.spikes.base = 0.05;
    s.spikes.length = 0.06;
    assert_eq!(Club::noun(&s), "Studded Club", "studs win over everything");
    s.spikes.length = 0.2;
    assert_eq!(Club::noun(&s), "Spiked Club");
    s.spikes.material = MaterialKind::Bone;
    assert_eq!(Club::noun(&s), "Bone-studded Club");
}
