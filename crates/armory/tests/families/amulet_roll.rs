//! Amulet "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::amulet::{Amulet, AmuletArchetype, AmuletState, LIMITS, NecklaceStyle, PendantCentre};
use armory::gem::{GemCut, Gemstone};
use armory::material::MaterialKind;
use armory::sections::device::Device;
use armory::sections::emblem::PlaqueShape;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<AmuletState> {
    range.map(Amulet::roll).collect()
}

#[test]
fn same_seed_same_amulet() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Amulet::roll(seed), Amulet::roll(seed));
        assert_eq!(Amulet::roll(seed).seed, seed);
    }
    assert_ne!(Amulet::roll(1), Amulet::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Amulet::roll(1207);
    let back: AmuletState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 77] {
        let s = Amulet::roll(seed);
        assert!(AmuletArchetype::WEIGHTS.iter().any(|(a, _)| Amulet::roll_as(seed, *a) == s), "seed {seed}");
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
        let (p, n) = (&s.pendant, &s.necklace);
        assert!(s.stone.stone.suits(s.stone.cut), "seed {seed}: {:?} cut {:?}", s.stone.stone, s.stone.cut);
        assert!(s.stone.size <= p.size * 0.5 + 1e-6, "seed {seed}: stone bigger than half the pendant");
        assert!(n.link >= n.thickness * 4.0 - 1e-6, "seed {seed}: links shorter than four wire radii");
        if p.centre == PendantCentre::Device {
            assert_ne!(p.device, Device::None, "seed {seed}: device centre with no device");
        }
        if matches!(p.shape, PlaqueShape::Crescent | PlaqueShape::Ankh | PlaqueShape::Cross) {
            assert_eq!(p.rim, 0.0, "seed {seed}: rim on a {:?}", p.shape);
        }
        match n.style {
            NecklaceStyle::Cord => assert_eq!(n.material, MaterialKind::Cord, "seed {seed}"),
            NecklaceStyle::Thong => assert_eq!(n.material, MaterialKind::Leather, "seed {seed}"),
            NecklaceStyle::Cable => assert!(matches!(n.material, MaterialKind::Iron | MaterialKind::Brass | MaterialKind::Gold | MaterialKind::Silver), "seed {seed}"),
            NecklaceStyle::Beads => {}
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&AmuletState) -> bool| all.iter().any(f);
    for shape in PlaqueShape::ALL {
        assert!(any(&|s| s.pendant.shape == shape), "no {shape:?} pendant");
    }
    for centre in PendantCentre::ALL {
        assert!(any(&|s| s.pendant.centre == centre), "no {centre:?} centre");
    }
    for style in NecklaceStyle::ALL {
        assert!(any(&|s| s.necklace.style == style), "no {style:?} necklace");
    }
    for device in Device::SIGNS {
        assert!(any(&|s| s.pendant.centre == PendantCentre::Device && s.pendant.device == device), "no {device:?} device");
    }
    for cut in [GemCut::Cabochon, GemCut::Brilliant, GemCut::Rose] {
        assert!(any(&|s| s.pendant.centre == PendantCentre::Stone && s.stone.cut == cut), "no {cut:?} stone");
    }
    for m in [MaterialKind::Gold, MaterialKind::Silver, MaterialKind::Brass, MaterialKind::Bone, MaterialKind::Wood, MaterialKind::Stone] {
        assert!(any(&|s| s.pendant.material == m), "no {m:?} pendant");
    }
    assert!(any(&|s| s.pendant.drops > 0), "no drops");
    assert!(any(&|s| s.pendant.rim > 0.0), "no rim");
    assert!(any(&|s| s.necklace.drop > 2.4), "no long necklace");
    assert!(any(&|s| s.etch.on), "no etching");
}

#[test]
fn nouns() {
    let mut s = AmuletState::default();
    s.stone.stone = Gemstone::Emerald;
    let mut noun = |shape: PlaqueShape, centre: PendantCentre| {
        s.pendant.shape = shape;
        s.pendant.centre = centre;
        Amulet::noun(&s)
    };
    use PendantCentre::{Device as D, Plain, Stone};
    assert_eq!(noun(PlaqueShape::Disc, D), "Medallion");
    assert_eq!(noun(PlaqueShape::Octagon, Stone), "Medallion");
    assert_eq!(noun(PlaqueShape::Oval, Stone), "Emerald Amulet");
    assert_eq!(noun(PlaqueShape::Oval, D), "Oval Medallion");
    assert_eq!(noun(PlaqueShape::Shield, D), "Shield Pendant");
    assert_eq!(noun(PlaqueShape::Teardrop, Stone), "Emerald Drop");
    assert_eq!(noun(PlaqueShape::Teardrop, Plain), "Teardrop Pendant");
    assert_eq!(noun(PlaqueShape::Quatrefoil, Stone), "Quatrefoil Pendant");
    assert_eq!(noun(PlaqueShape::Star, D), "Star Amulet");
    assert_eq!(noun(PlaqueShape::Cross, Stone), "Pendant Cross");
    assert_eq!(noun(PlaqueShape::Crescent, Plain), "Lunula");
    assert_eq!(noun(PlaqueShape::Lozenge, D), "Lozenge Amulet");
    assert_eq!(noun(PlaqueShape::Ankh, Stone), "Jewelled Ankh");
    assert_eq!(noun(PlaqueShape::Ankh, Plain), "Ankh");
}
