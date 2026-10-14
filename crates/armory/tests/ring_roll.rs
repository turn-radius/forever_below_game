//! Ring "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::ring::{BandProfile, LIMITS, Ring, RingArchetype, RingState, SettingStyle, SignetFace};
use armory::gem::{GemCut, Gemstone};
use armory::material::MaterialKind;
use armory::sections::device::Device;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<RingState> {
    range.map(Ring::roll).collect()
}

#[test]
fn same_seed_same_ring() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Ring::roll(seed), Ring::roll(seed));
        assert_eq!(Ring::roll(seed).seed, seed);
    }
    assert_ne!(Ring::roll(1), Ring::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Ring::roll(1207);
    let back: RingState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 77] {
        let s = Ring::roll(seed);
        assert!(RingArchetype::WEIGHTS.iter().any(|(a, _)| Ring::roll_as(seed, *a) == s), "seed {seed}");
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
        assert!(s.stone.stone.suits(s.stone.cut), "seed {seed}: {:?} cut {:?}", s.stone.stone, s.stone.cut);
        assert!(s.stone.size <= s.band.inner * 0.7 + 1e-6, "seed {seed}: stone wider than the finger allows");
        assert!(s.band.thickness <= s.band.inner * 0.25 + 1e-6, "seed {seed}: band too thick");
        assert!(matches!(s.band.material, MaterialKind::Gold | MaterialKind::Silver | MaterialKind::Brass | MaterialKind::Iron));
        if s.band.strands > 1 {
            assert_eq!(s.setting.style, SettingStyle::None, "seed {seed}: twisted band with a setting");
            assert_eq!(s.band.side_stones, 0, "seed {seed}: stones on a twisted band");
        }
        if s.setting.style == SettingStyle::Signet {
            assert_eq!(s.band.side_stones, 0, "seed {seed}: stones beside a signet");
            assert!(s.setting.face_size >= s.band.width * 1.1 - 1e-6, "seed {seed}: signet face narrower than the band");
            assert!(s.band.shoulder >= 0.8 - 1e-4, "seed {seed}: signet without shoulders");
        }
        if s.setting.style == SettingStyle::Bezel {
            assert!(matches!(s.stone.cut, GemCut::Cabochon | GemCut::Rose | GemCut::Orb | GemCut::Brilliant), "seed {seed}");
        }
        assert_ne!(s.stone.cut, GemCut::Shard, "seed {seed}: rings take no raw shards");
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&RingState) -> bool| all.iter().any(f);
    for style in SettingStyle::ALL {
        assert!(any(&|s| s.setting.style == style), "no {style:?} setting");
    }
    for profile in BandProfile::ALL {
        assert!(any(&|s| s.band.strands == 1 && s.band.profile == profile), "no {profile:?} band");
    }
    for face in SignetFace::ALL {
        assert!(any(&|s| s.setting.style == SettingStyle::Signet && s.setting.face == face), "no {face:?} signet");
    }
    for device in Device::SIGNS {
        assert!(any(&|s| s.setting.style == SettingStyle::Signet && s.setting.device == device), "no {device:?} signet");
    }
    for cut in [GemCut::Brilliant, GemCut::Rose, GemCut::Cabochon, GemCut::Octahedron, GemCut::Orb] {
        assert!(any(&|s| s.setting.style != SettingStyle::None && s.setting.style != SettingStyle::Signet && s.stone.cut == cut), "no {cut:?} stone");
    }
    for stone in Gemstone::ALL {
        assert!(any(&|s| s.stone.stone == stone), "no {stone:?}");
    }
    for metal in [MaterialKind::Gold, MaterialKind::Silver, MaterialKind::Brass, MaterialKind::Iron] {
        assert!(any(&|s| s.band.material == metal), "no {metal:?} band");
    }
    assert!(any(&|s| s.band.strands == 2) && any(&|s| s.band.strands == 3), "missing a strand count");
    assert!(any(&|s| s.band.side_stones > 0), "no side stones");
    assert!(any(&|s| s.setting.material != s.band.material), "no two-tone ring");
    assert!(any(&|s| s.etch.on), "no etching");
}

fn base() -> RingState {
    RingState::default()
}

#[test]
fn nouns() {
    let mut s = base();
    s.stone.stone = Gemstone::Ruby;
    s.setting.style = SettingStyle::Claws;
    assert_eq!(Ring::noun(&s), "Ruby Ring");
    s.setting.style = SettingStyle::Bezel;
    assert_eq!(Ring::noun(&s), "Ruby Ring");
    s.setting.style = SettingStyle::Cluster;
    assert_eq!(Ring::noun(&s), "Ruby Cluster Ring");
    s.setting.style = SettingStyle::Signet;
    assert_eq!(Ring::noun(&s), "Signet Ring");
    s.setting.style = SettingStyle::None;
    s.band.material = MaterialKind::Gold;
    assert_eq!(Ring::noun(&s), "Gold Band");
    s.band.material = MaterialKind::Brass;
    assert_eq!(Ring::noun(&s), "Bronze Band", "brass is called bronze, as in the prototype");
    s.band.material = MaterialKind::Iron;
    s.band.strands = 3;
    assert_eq!(Ring::noun(&s), "Twisted Iron Band");
    s.stone.stone = Gemstone::Diamond;
    s.band.strands = 1;
    s.setting.style = SettingStyle::Claws;
    assert_eq!(Ring::noun(&s), "Diamond Ring");
}

#[test]
fn rolled_nouns_cover_the_prototype_names() {
    let names: Vec<String> = rolls(1..=256).iter().map(Ring::noun).collect();
    for name in ["Signet Ring", "Gold Band", "Silver Band", "Iron Band", "Twisted Gold Band"] {
        assert!(names.iter().any(|n| n == name), "no {name}");
    }
    assert!(names.iter().any(|n| n.ends_with(" Cluster Ring")));
}
