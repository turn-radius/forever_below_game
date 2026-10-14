//! Mace "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::mace::{LIMITS, Mace, MaceArchetype, MaceState};
use armory::gem::Gemstone;
use armory::sections::hafted::PommelGem;
use armory::sections::head::{FinialStyle, HeadStyle};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<MaceState> {
    range.map(Mace::roll).collect()
}

#[test]
fn same_seed_same_mace() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Mace::roll(seed), Mace::roll(seed));
        assert_eq!(Mace::roll(seed).seed, seed);
    }
    assert_ne!(Mace::roll(1), Mace::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Mace::roll(1207);
    let back: MaceState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    let s = Mace::roll(42);
    let found = MaceArchetype::WEIGHTS.iter().any(|(a, _)| Mace::roll_as(42, *a) == s);
    assert!(found, "roll(42) should equal roll_as(42, its archetype)");
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
        assert_ne!(s.head.style, HeadStyle::None, "seed {seed}");
        if s.gem.on {
            assert_eq!(s.finial.style, FinialStyle::None, "seed {seed}: finial over a set stone");
            assert!(s.gem.size <= s.head.radius + 0.01, "seed {seed}: stone wider than head");
            assert!(s.gem.stone.suits(s.gem.cut), "seed {seed}: {:?} cut {:?}", s.gem.stone, s.gem.cut);
        }
        if s.head.style == HeadStyle::Cup {
            assert!(s.gem.on && s.prongs.on, "seed {seed}: empty cup");
        }
        if s.prongs.on {
            assert!(s.gem.on, "seed {seed}: prongs with nothing to hold");
        }
        if s.wrap.on {
            assert!(s.grip.on, "seed {seed}: wrap without grip");
        }
        if s.pommel.gem == PommelGem::Ring {
            assert!(s.pommel.style.has_face(), "seed {seed}: stone ring on {:?}", s.pommel.style);
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&MaceState) -> bool| all.iter().any(f);
    for style in [HeadStyle::Orb, HeadStyle::Pear, HeadStyle::Drum, HeadStyle::Onion, HeadStyle::Cup] {
        assert!(any(&|s| s.head.style == style), "no {style:?} head");
    }
    assert!(any(&|s| s.flanges.on && !s.spikes.on), "no flanges");
    assert!(any(&|s| s.spikes.on && !s.flanges.on), "no spikes");
    assert!(any(&|s| s.spikes.on && s.flanges.on), "no spiked flanges");
    assert!(any(&|s| s.prongs.on && s.gem.on), "no prongs with gem");
    assert!(any(&|s| s.collar.on), "no collar");
    assert!(any(&|s| !s.collar.on), "always a collar");
    assert!(any(&|s| s.wrap.on), "no wrap");
    assert!(any(&|s| s.pommel.gem == PommelGem::EndStone), "no pommel end stone");
    assert!(any(&|s| s.pommel.gem == PommelGem::Ring), "no pommel stone ring");
    assert!(any(&|s| s.etch.on), "no etching");
    for name in ["Shestopyor", "Pernach", "Flanged Mace", "Morningstar", "Spiked Flanged Mace", "Ceremonial Mace", "Mace"] {
        assert!(all.iter().any(|s| Mace::noun(s) == name), "no {name}");
    }
}

fn base() -> MaceState {
    let mut s = MaceState::default();
    s.flanges.on = false;
    s.spikes.on = false;
    s.prongs.on = false;
    s.gem.on = false;
    s
}

#[test]
fn nouns() {
    let mut s = base();
    assert_eq!(Mace::noun(&s), "Mace");

    s.flanges.on = true;
    s.flanges.count = 6;
    assert_eq!(Mace::noun(&s), "Shestopyor");
    for n in 7..=12 {
        s.flanges.count = n;
        assert_eq!(Mace::noun(&s), "Pernach");
    }
    s.flanges.count = 5;
    assert_eq!(Mace::noun(&s), "Flanged Mace");

    s.spikes.on = true;
    assert_eq!(Mace::noun(&s), "Spiked Flanged Mace");

    let mut s = base();
    s.spikes.on = true;
    assert_eq!(Mace::noun(&s), "Morningstar");

    let mut s = base();
    s.gem.on = true;
    s.gem.stone = Gemstone::Ruby;
    s.prongs.on = true;
    assert_eq!(Mace::noun(&s), "Ceremonial Mace");
    s.flanges.on = true;
    s.flanges.count = 8;
    assert_eq!(Mace::noun(&s), "Ceremonial Mace", "gem and prongs win over flanges");
}
