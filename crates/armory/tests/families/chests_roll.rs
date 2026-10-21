//! Chest "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::chests::{ChestState, ChestStyle, Chests, LIMITS};
use armory::material::MaterialKind;
use armory::sections::device::Device;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<ChestState> {
    range.map(Chests::roll).collect()
}

#[test]
fn same_seed_same_chest() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Chests::roll(seed), Chests::roll(seed));
        assert_eq!(Chests::roll(seed).seed, seed);
    }
    assert_ne!(Chests::roll(1), Chests::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Chests::roll(1207);
    let back: ChestState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 1207] {
        let s = Chests::roll(seed);
        assert_eq!(Chests::roll_as(seed, s.chest.style), s, "seed {seed}");
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
        let (c, seed) = (&s.chest, s.seed);
        if c.open > 0.0 {
            assert!(c.style.is_lidded_box(), "seed {seed}: an open {:?}", c.style);
        }
        if c.treasure {
            assert!(Chests::is_open(&s), "seed {seed}: treasure in a shut box");
        }
        if c.studs {
            assert!(c.bands > 0, "seed {seed}: studs with no bands");
        }
        if c.device != Device::None {
            assert!(matches!(c.style, ChestStyle::Crate | ChestStyle::Barrel), "seed {seed}: stencil on a {:?}", c.style);
        }
        match c.style {
            ChestStyle::Barrel => assert!(c.bands >= 2, "seed {seed}: barrel with {} hoops", c.bands),
            ChestStyle::Strongbox => assert_eq!(c.material, MaterialKind::Iron, "seed {seed}"),
            ChestStyle::Amphora => assert!(c.handles && c.bands == 0, "seed {seed}"),
            ChestStyle::Coffer => assert!(!c.handles && c.fittings != c.material, "seed {seed}"),
            _ => {}
        }
        assert!(c.stone.suits(armory::gem::GemCut::Cabochon), "seed {seed}");
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&ChestState) -> bool| all.iter().any(f);
    for style in ChestStyle::ALL {
        assert!(any(&|s| s.chest.style == style), "no {style:?}");
    }
    for device in Device::ALL {
        assert!(any(&|s| s.chest.device == device), "no {device:?}");
    }
    assert!(any(&|s| Chests::is_open(s) && s.chest.treasure), "no hoard");
    assert!(any(&|s| Chests::is_open(s) && !s.chest.treasure), "every open box full");
    assert!(any(&|s| s.chest.style == ChestStyle::Chest && s.chest.bands == 0), "every chest banded");
    assert!(any(&|s| s.chest.studs), "no studs");
    assert!(any(&|s| s.chest.style == ChestStyle::Chest && !s.chest.lock), "every chest locked");
    assert!(any(&|s| s.chest.style == ChestStyle::Amphora && s.chest.lock), "no amphora lid");
    assert!(any(&|s| s.chest.style == ChestStyle::Crate && s.chest.handles), "no rope handles");
    assert!(any(&|s| s.chest.style == ChestStyle::Crate && s.chest.bands > 0), "no shipping straps");
    assert!(any(&|s| s.etch.on), "no etching");
    for m in [MaterialKind::Wood, MaterialKind::Iron, MaterialKind::Brass, MaterialKind::Gold, MaterialKind::Silver, MaterialKind::Stone] {
        assert!(any(&|s| s.chest.material == m), "no {m:?} body");
    }
    for name in ["Treasure Chest", "Iron-bound Chest", "Strongbox", "Jewel Coffer", "Barrel", "Supply Crate", "Amphora"] {
        assert!(all.iter().any(|s| Chests::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = ChestState::default();
    assert_eq!(Chests::noun(&s), "Iron-bound Chest");
    s.chest.treasure = true;
    assert_eq!(Chests::noun(&s), "Iron-bound Chest", "a hoard behind a shut lid does not show");
    s.chest.open = 0.5;
    assert_eq!(Chests::noun(&s), "Treasure Chest");
    for (style, noun) in [
        (ChestStyle::Strongbox, "Strongbox"),
        (ChestStyle::Coffer, "Jewel Coffer"),
        (ChestStyle::Barrel, "Barrel"),
        (ChestStyle::Crate, "Supply Crate"),
        (ChestStyle::Amphora, "Amphora"),
    ] {
        s.chest.style = style;
        assert_eq!(Chests::noun(&s), noun);
    }
}
