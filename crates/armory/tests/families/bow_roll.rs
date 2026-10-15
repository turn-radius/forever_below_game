//! Bow "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::bow::{Bow, BowArchetype, BowState, LIMITS};
use armory::material::MaterialKind;
use armory::sections::ranged::{ArrowHead, FletchCut, LimbShape, NockStyle};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<BowState> {
    range.map(Bow::roll).collect()
}

#[test]
fn same_seed_same_bow() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Bow::roll(seed), Bow::roll(seed));
        assert_eq!(Bow::roll(seed).seed, seed);
    }
    assert_ne!(Bow::roll(1), Bow::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Bow::roll(1207);
    let back: BowState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    let s = Bow::roll(42);
    assert!(BowArchetype::WEIGHTS.iter().any(|(a, _)| Bow::roll_as(42, *a) == s));
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
        let l = &s.limbs;
        assert_eq!(l.siyah > 0.0, l.shape == LimbShape::Composite, "seed {seed}: tip levers only on composite bows");
        if l.shape == LimbShape::Longbow && Bow::noun(&s) == "Longbow" {
            assert!(l.thickness / l.width > 0.6, "seed {seed}: a longbow should be deep, not flat");
        }
        if l.nocks == NockStyle::Cap {
            assert!(matches!(l.nock_material, MaterialKind::Brass | MaterialKind::Iron), "seed {seed}: cap of {:?}", l.nock_material);
        }
        assert!(s.riser.width <= l.width + 1e-3, "seed {seed}: handle wider than the limbs");
        assert!(s.riser.thickness >= l.thickness, "seed {seed}: handle thinner than the limbs");
        // The arrow is drawn to about two fifths of the bow.
        assert!(s.arrow.length < l.length * 0.6, "seed {seed}: arrow {} on a {} bow", s.arrow.length, l.length);
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&BowState) -> bool| all.iter().any(f);
    for shape in LimbShape::ALL {
        assert!(any(&|s| s.limbs.shape == shape), "no {shape:?} limbs");
    }
    for nocks in NockStyle::ALL {
        assert!(any(&|s| s.limbs.nocks == nocks), "no {nocks:?} nocks");
    }
    for head in ArrowHead::ALL {
        assert!(any(&|s| s.arrow.on && s.arrow.head == head), "no {head:?} arrowhead");
    }
    for cut in FletchCut::ALL {
        assert!(any(&|s| s.arrow.on && s.arrow.fletch_cut == cut), "no {cut:?} fletching");
    }
    for fletches in 2..=4 {
        assert!(any(&|s| s.arrow.on && s.arrow.fletches == fletches), "no {fletches}-vane arrow");
    }
    assert!(any(&|s| s.arrow.on) && any(&|s| !s.arrow.on), "arrows always or never");
    assert!(any(&|s| s.riser.wrap && s.riser.wrap_material == MaterialKind::Leather), "no leather grip");
    assert!(any(&|s| s.riser.wrap && s.riser.wrap_material == MaterialKind::Cord), "no cord grip");
    assert!(any(&|s| !s.riser.wrap), "always a grip wrap");
    assert!(any(&|s| s.riser.material != s.limbs.material), "never a separate handle");
    assert!(any(&|s| s.string.serving) && any(&|s| !s.string.serving), "serving always or never");
    assert!(any(&|s| s.etch.on), "no etching");
    for name in ["Longbow", "Self Bow", "Recurve Bow", "Reflex Bow", "Horse Bow", "Horn Bow"] {
        assert!(all.iter().any(|s| Bow::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = BowState::default();
    let mut noun = |shape, length, recurve| {
        s.limbs.shape = shape;
        s.limbs.length = length;
        s.limbs.recurve = recurve;
        Bow::noun(&s)
    };
    assert_eq!(noun(LimbShape::Longbow, 18.0, 0.0), "Longbow");
    assert_eq!(noun(LimbShape::Longbow, 15.0, 0.0), "Self Bow");
    assert_eq!(noun(LimbShape::Recurve, 14.0, 0.7), "Recurve Bow");
    assert_eq!(noun(LimbShape::Reflex, 14.0, 0.6), "Reflex Bow");
    assert_eq!(noun(LimbShape::Reflex, 14.0, 0.2), "Self Bow");
    assert_eq!(noun(LimbShape::Composite, 11.0, 0.7), "Horse Bow");
    assert_eq!(noun(LimbShape::Composite, 15.0, 0.7), "Horn Bow");
}
