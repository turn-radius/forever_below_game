//! Boots and greaves geometry: deterministic, drawable, complete, and foot sized.

use armory::fabric::PatternKind;
use armory::families::Generator;
use armory::families::feet::{Feet, FeetState};
use armory::sections::boot::BootStyle;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (FeetState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Feet::roll(seed);
        let mesh = Feet::build(&state);
        (state, mesh)
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

fn bounds(parts: &[Part]) -> ([f32; 3], [f32; 3]) {
    parts.iter().flat_map(|p| &p.positions).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Feet::roll(seed);
        assert_eq!(Feet::build(&state), Feet::build(&state), "seed {seed}");
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
        let b = &s.boot;
        let low = b.height * b.size < 0.65 && !matches!(b.style, BootStyle::Sabaton | BootStyle::Wrapped);
        let sabaton = b.style == BootStyle::Sabaton;
        let expected = [
            ("foot", true),
            ("sole", true),
            ("heel", matches!(b.style, BootStyle::Boot | BootStyle::Sabaton)),
            ("shaft", !low),
            ("lining", low),
            ("straps", b.straps > 0),
            ("buckles", b.straps > 0),
            ("lames", sabaton),
            ("knee", b.knee),
            ("binding", b.style == BootStyle::Wrapped),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
        if sabaton {
            assert_eq!(part(&mesh, "foot").unwrap().pattern.map(|p| p.kind), Some(PatternKind::Mail), "seed {}", s.seed);
        }
    }
}

#[test]
fn footwear_is_foot_sized_and_stands_on_the_ground() {
    for (s, mesh) in built() {
        let b = &s.boot;
        let (lo, hi) = bounds(&mesh.parts);
        assert!(lo[1].abs() < 1e-4, "seed {}: floor at {}", s.seed, lo[1]);
        let length = hi[0] - lo[0];
        assert!(length > 2.4 * b.size && length < 2.9 * b.size + 1.9 * b.toe * b.size, "seed {}: {length} long", s.seed);
        let height = hi[1] - lo[1];
        assert!(height > b.height * b.size * 0.9 && height < b.height * b.size + 1.6, "seed {}: {height} tall", s.seed);
        assert!(hi[2] - lo[2] < 1.6, "seed {}: {} wide", s.seed, hi[2] - lo[2]);
    }
}
