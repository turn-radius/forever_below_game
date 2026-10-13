//! Trousers, breeches and leggings geometry: deterministic, drawable, complete, and leg sized.

use armory::families::Generator;
use armory::families::legs::{Legs, LegsState};
use armory::material::MaterialKind;
use armory::sections::legwear::{LegwearStyle, WaistClosure};
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (LegsState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Legs::roll(seed);
        let mesh = Legs::build(&state);
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
        let state = Legs::roll(seed);
        assert_eq!(Legs::build(&state), Legs::build(&state), "seed {seed}");
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
        let w = &s.legwear;
        let breeches = w.style == LegwearStyle::Breeches;
        let cuffed = w.style == LegwearStyle::Trousers && w.cuff > 0.0 && !w.puttees;
        let expected = [
            ("legs", true),
            ("waistband", true),
            ("ties", w.closure != WaistClosure::Laces),
            ("laces", w.closure == WaistClosure::Laces),
            ("toggles", w.closure == WaistClosure::Toggles),
            ("cuffs", cuffed),
            ("trim", s.hem.on && !breeches && !cuffed && !w.puttees),
            ("kneebands", breeches),
            ("buckles", breeches),
            ("puttees", w.puttees),
            ("patches", w.patches),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
        let legs = part(&mesh, "legs").unwrap();
        assert_eq!(legs.material, Some(MaterialKind::Cloth));
        assert_eq!(legs.pattern.map(|p| p.kind), w.cloth.pattern.kind(), "seed {}", s.seed);
    }
}

#[test]
fn legwear_is_leg_sized_and_stands_on_its_hems() {
    for (s, mesh) in built() {
        let (lo, hi) = bounds(&mesh.parts);
        assert!(lo[1].abs() < 1e-4, "seed {}: floor at {}", s.seed, lo[1]);
        let height = hi[1] - lo[1];
        // Puttees wind from the ankle whatever the hem.
        let (min, max) = match s.legwear.style {
            LegwearStyle::Breeches if !s.legwear.puttees => (5.5, 7.0),
            _ => (9.6, 10.4),
        };
        assert!(height > min && height < max, "seed {}: {:?} {height} tall", s.seed, s.legwear.style);
        let width = hi[0] - lo[0];
        assert!(width > 3.0 && width < 5.0, "seed {}: {width} wide", s.seed);
        assert!((hi[0] + lo[0]).abs() < 0.3, "seed {}: off centre", s.seed);
        assert!(hi[2] - lo[2] < 3.6, "seed {}: {} deep", s.seed, hi[2] - lo[2]);
    }
}
