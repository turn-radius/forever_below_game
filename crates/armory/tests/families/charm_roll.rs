//! Charm "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::charm::{Charm, CharmShape, CharmState, HangerStyle, LIMITS};
use armory::gem::GemCut;
use armory::material::MaterialKind;
use armory::sections::device::Device;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<CharmState> {
    range.map(Charm::roll).collect()
}

#[test]
fn same_seed_same_charm() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Charm::roll(seed), Charm::roll(seed));
        assert_eq!(Charm::roll(seed).seed, seed);
    }
    assert_ne!(Charm::roll(1), Charm::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Charm::roll(1207);
    let back: CharmState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 77] {
        let s = Charm::roll(seed);
        assert_eq!(Charm::roll_as(seed, s.charm.shape), s, "seed {seed}");
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
        let (seed, c) = (s.seed, &s.charm);
        assert!(s.stone.size <= c.size * 0.25 + 1e-6, "seed {seed}: stone bigger than a quarter of the charm");
        assert!(s.stone.stone.suits(s.stone.cut), "seed {seed}");
        if c.inset {
            assert!(c.shape.takes_stone(), "seed {seed}: stone in a {:?}", c.shape);
            assert_eq!(c.device, Device::None, "seed {seed}: carving and stone both");
        }
        if !c.shape.takes_device() {
            assert_eq!(c.device, Device::None, "seed {seed}: carving on a {:?}", c.shape);
        }
        if !c.shape.takes_bindings() {
            assert_eq!(c.bindings, 0, "seed {seed}: bindings on a {:?}", c.shape);
        }
        if c.shape == CharmShape::Fang {
            assert_eq!(c.material, MaterialKind::Bone, "seed {seed}");
        }
        match s.hanger.style {
            HangerStyle::CordLoop => assert!(matches!(s.hanger.material, MaterialKind::Cord | MaterialKind::Leather), "seed {seed}"),
            _ => {
                assert!(matches!(s.hanger.material, MaterialKind::Iron | MaterialKind::Brass | MaterialKind::Gold | MaterialKind::Silver), "seed {seed}");
                assert_eq!(s.hanger.beads, 0, "seed {seed}: beads without a cord");
            }
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&CharmState) -> bool| all.iter().any(f);
    for shape in CharmShape::ALL {
        assert!(any(&|s| s.charm.shape == shape), "no {shape:?}");
        if shape.takes_stone() {
            assert!(any(&|s| s.charm.shape == shape && s.charm.inset), "no {shape:?} with a stone");
        }
    }
    for style in HangerStyle::ALL {
        assert!(any(&|s| s.hanger.style == style), "no {style:?}");
    }
    for device in [Device::Rune, Device::Star, Device::Cross, Device::Wheel] {
        assert!(any(&|s| s.charm.device == device), "no {device:?} carving");
    }
    for m in [MaterialKind::Bone, MaterialKind::Wood, MaterialKind::Stone, MaterialKind::Iron, MaterialKind::Brass, MaterialKind::Gold, MaterialKind::Silver] {
        assert!(any(&|s| s.charm.material == m), "no {m:?} charm");
    }
    assert!((1..=8).all(|d| any(&|s| s.charm.shape == CharmShape::Knot && s.charm.detail == d)), "a knot missing");
    assert!(any(&|s| s.charm.bindings > 0), "no bindings");
    assert!(any(&|s| s.hanger.beads > 0), "no beads");
    assert!(all.iter().all(|s| s.stone.cut == GemCut::Cabochon));
}

#[test]
fn nouns() {
    let names = [
        (CharmShape::Fang, "Fang Charm"),
        (CharmShape::Hammer, "Hammer Amulet"),
        (CharmShape::Wheel, "Sun Wheel"),
        (CharmShape::Knot, "Knot Charm"),
        (CharmShape::Idol, "Carved Idol"),
        (CharmShape::RuneStone, "Rune Stone"),
        (CharmShape::Acorn, "Acorn Charm"),
        (CharmShape::Bell, "Bell Charm"),
        (CharmShape::Key, "Key Charm"),
    ];
    let mut s = CharmState::default();
    for (shape, name) in names {
        s.charm.shape = shape;
        assert_eq!(Charm::noun(&s), name);
    }
}
