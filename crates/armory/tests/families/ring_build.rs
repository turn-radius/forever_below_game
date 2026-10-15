//! Ring geometry: deterministic, drawable, complete, and the size of a ring.

use armory::families::Generator;
use armory::families::ring::{Ring, RingState, SettingStyle};
use armory::sections::device::Device;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (RingState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Ring::roll(seed);
        let mesh = Ring::build(&state);
        (state, mesh)
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

fn bounds(parts: &[&Part]) -> ([f32; 3], [f32; 3]) {
    parts.iter().flat_map(|p| p.positions.iter()).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Ring::roll(seed);
        assert_eq!(Ring::build(&state), Ring::build(&state), "seed {seed}");
    }
}

#[test]
fn every_part_is_drawable() {
    for (state, mesh) in built() {
        assert!(!mesh.parts.is_empty(), "seed {}: no parts", state.seed);
        for p in &mesh.parts {
            p.validate().unwrap_or_else(|e| panic!("seed {}: {e}", state.seed));
            assert!(!p.indices.is_empty(), "seed {}: {} has no triangles", state.seed, p.name);
            assert!(p.material.is_some(), "seed {}: {} has no material", state.seed, p.name);
        }
    }
}

#[test]
fn every_enabled_section_has_a_part() {
    for (s, mesh) in built() {
        let style = s.setting.style;
        let stoned = matches!(style, SettingStyle::Bezel | SettingStyle::Claws | SettingStyle::Cluster);
        // Pristine rolls lose no stones.
        let expected = [
            ("band", true),
            ("setting", style != SettingStyle::None),
            ("stone", stoned),
            ("claws", matches!(style, SettingStyle::Claws | SettingStyle::Cluster)),
            ("halo_settings", style == SettingStyle::Cluster),
            ("accent_stones", style == SettingStyle::Cluster),
            ("device", style == SettingStyle::Signet && s.setting.device != Device::None),
            ("side_settings", s.band.side_stones > 0 && s.band.strands == 1 && style != SettingStyle::Signet),
            ("side_stones", s.band.side_stones > 0 && s.band.strands == 1 && style != SettingStyle::Signet),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn stones_carry_their_colour_and_small_parts_skip_the_outline() {
    for (s, mesh) in built() {
        for p in &mesh.parts {
            if p.name.ends_with("stones") || p.name == "stone" {
                assert_eq!(p.tint, Some(s.stone.stone.rgb()), "seed {}: {}", s.seed, p.name);
            }
            if matches!(p.name, "claws" | "accent_stones" | "side_stones" | "side_settings" | "halo_settings" | "device") {
                assert!(!p.outline, "seed {}: {} should not be outlined", s.seed, p.name);
            }
        }
    }
}

#[test]
fn the_finger_hole_stays_clear_and_the_ring_is_ring_sized() {
    for (s, mesh) in built() {
        let r = s.band.inner * 0.5;
        let band = part(&mesh, "band").unwrap();
        // Nothing of the band inside the finger (the comfort dome dips a hair under).
        for p in &band.positions {
            let d = p[0].hypot(p[1]);
            assert!(d > r * 0.97, "seed {}: band reaches {d} inside the finger radius {r}", s.seed);
            assert!(p[2].abs() < s.band.width * 1.5 + 0.01, "seed {}: band too wide", s.seed);
        }
        let all: Vec<&Part> = mesh.parts.iter().collect();
        let (lo, hi) = bounds(&all);
        let outer = r + s.band.thickness * (1.0 + 1.4 * s.band.shoulder);
        assert!(lo[1] > -outer - 0.01 && lo[0] > -outer - 0.05 && hi[0] < outer + 0.05, "seed {}: {lo:?}", s.seed);
        assert!(hi[1] < outer + 0.15, "seed {}: setting stands {} above the band", s.seed, hi[1] - outer);
        // The setting sits on top.
        if let Some(setting) = part(&mesh, "setting") {
            let (slo, _) = bounds(&[setting]);
            assert!(slo[1] > 0.0, "seed {}: setting below the finger", s.seed);
        }
    }
}

#[test]
fn a_worn_ring_thins_on_the_palm_side_and_a_damaged_one_loses_stones() {
    let mut s = (1..).map(Ring::roll).find(|s| s.setting.style == SettingStyle::Cluster).unwrap();
    let fresh = Ring::build(&s);
    let low = |m: &ItemMesh| bounds(&[part(m, "band").unwrap()]).0[1];
    s.cond.wear = 1.0;
    assert!(low(&Ring::build(&s)) > low(&fresh), "wear should thin the palm side");
    s.cond.damage = 1.0;
    let mut lost_any = false;
    for seed in 0..20 {
        s.seed = seed;
        let m = Ring::build(&s);
        lost_any |= part(&m, "stone").is_none();
    }
    assert!(lost_any, "heavy damage should sometimes lose the centre stone");
}
