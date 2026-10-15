//! Staff "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::staff::{LIMITS, Staff, StaffArchetype, StaffState};
use armory::gem::GemCut;
use armory::sections::hafted::{PommelGem, PommelStyle};
use armory::sections::head::{FinialStyle, HeadStyle};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<StaffState> {
    range.map(Staff::roll).collect()
}

#[test]
fn same_seed_same_staff() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Staff::roll(seed), Staff::roll(seed));
        assert_eq!(Staff::roll(seed).seed, seed);
    }
    assert_ne!(Staff::roll(1), Staff::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Staff::roll(1207);
    let back: StaffState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 512] {
        let s = Staff::roll(seed);
        let found = StaffArchetype::WEIGHTS.iter().any(|(a, _)| Staff::roll_as(seed, *a) == s);
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
        if s.prongs.on {
            assert!(s.gem.on, "seed {seed}: prongs with nothing to hold");
            assert_eq!(s.finial.style, FinialStyle::None, "seed {seed}: finial inside the cradle");
        }
        if s.gem.on {
            assert!(s.gem.stone.suits(s.gem.cut), "seed {seed}: {:?} cut {:?}", s.gem.stone, s.gem.cut);
            if s.head.style == HeadStyle::Cup && !s.prongs.on && s.finial.style == FinialStyle::None {
                assert!(!s.gem.bob, "seed {seed}: a stone floating out of its cup");
            }
        }
        if s.pommel.gem != PommelGem::None {
            assert!(s.gem.on && s.pommel.gem_stone == s.gem.stone, "seed {seed}: butt stone without a matching head stone");
        }
        assert_ne!(s.pommel.gem, PommelGem::Ring, "seed {seed}: ring of stones on a butt cap");
        if s.spikes.on {
            assert!(!s.gem.on, "seed {seed}: spikes on a gemmed staff");
        }
        assert!(s.grip.position >= 0.3, "seed {seed}: a staff is held mid-shaft");
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&StaffState) -> bool| all.iter().any(f);
    for style in [HeadStyle::Orb, HeadStyle::Pear, HeadStyle::Drum, HeadStyle::Onion, HeadStyle::Cup, HeadStyle::None] {
        assert!(any(&|s| s.head.style == style), "no {style:?} head");
    }
    for style in [FinialStyle::None, FinialStyle::Spike, FinialStyle::Knob, FinialStyle::Spire] {
        assert!(any(&|s| s.finial.style == style), "no {style:?} finial");
    }
    for style in [PommelStyle::Ferrule, PommelStyle::Spike, PommelStyle::Ball, PommelStyle::Flared] {
        assert!(any(&|s| s.pommel.on && s.pommel.style == style), "no {style:?} butt cap");
    }
    for cut in [GemCut::Brilliant, GemCut::Rose, GemCut::Octahedron, GemCut::Shard, GemCut::Cabochon, GemCut::Orb] {
        assert!(any(&|s| s.gem.on && s.gem.cut == cut), "no {cut:?} stone");
    }
    assert!(any(&|s| s.gem.on && s.gem.bob), "no floating stone");
    assert!(any(&|s| s.gem.on && !s.gem.bob), "no held stone");
    assert!(any(&|s| s.prongs.on && s.head.style == HeadStyle::None), "no roots cradling a stone");
    assert!(any(&|s| s.spikes.on), "no spiked cap");
    assert!(any(&|s| s.flanges.on), "no flanges");
    assert!(any(&|s| s.collar.on) && any(&|s| !s.collar.on), "collar never varies");
    assert!(any(&|s| s.grip.on) && any(&|s| !s.grip.on), "grip never varies");
    assert!(any(&|s| s.wrap.on && !s.grip.on), "no wrap on a bare shaft");
    assert!(any(&|s| s.wrap.cross), "no cross-wound wrap");
    assert!(any(&|s| !s.pommel.on), "always a butt cap");
    assert!(any(&|s| s.pommel.gem == PommelGem::EndStone), "no butt stone");
    assert!(any(&|s| s.etch.on), "no etching");
    for name in ["Seer's Staff", "Focus Staff", "Gemmed Staff", "Orb Staff", "Wanderer's Staff", "Iron-shod Quarterstaff", "Quarterstaff"] {
        assert!(all.iter().any(|s| Staff::noun(s) == name), "no {name}");
    }
}

fn base() -> StaffState {
    let mut s = StaffState::default();
    s.prongs.on = false;
    s.gem.on = false;
    s.shaft.gnarl = 0.0;
    s.shaft.bands = 0;
    s
}

#[test]
fn nouns() {
    let mut s = base();
    assert_eq!(Staff::noun(&s), "Quarterstaff");
    s.shaft.bands = 2;
    assert_eq!(Staff::noun(&s), "Quarterstaff");
    s.shaft.bands = 3;
    assert_eq!(Staff::noun(&s), "Iron-shod Quarterstaff");
    s.shaft.gnarl = 0.41;
    assert_eq!(Staff::noun(&s), "Wanderer's Staff", "gnarl wins over bands");
    s.shaft.gnarl = 0.4;
    assert_eq!(Staff::noun(&s), "Iron-shod Quarterstaff");

    s.gem.on = true;
    assert_eq!(Staff::noun(&s), "Orb Staff");
    s.prongs.on = true;
    s.gem.bob = true;
    assert_eq!(Staff::noun(&s), "Seer's Staff");
    s.gem.bob = false;
    s.gem.glow = 0.6;
    assert_eq!(Staff::noun(&s), "Focus Staff");
    s.gem.glow = 0.59;
    assert_eq!(Staff::noun(&s), "Gemmed Staff");

    s.gem.on = false;
    assert_eq!(Staff::noun(&s), "Iron-shod Quarterstaff", "empty prongs name nothing");
}
