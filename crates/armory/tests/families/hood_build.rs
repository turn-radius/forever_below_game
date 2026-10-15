//! Hoods, cowls and coifs geometry: deterministic, drawable, complete, and head sized.

use armory::fabric::PatternKind;
use armory::families::Generator;
use armory::families::hood::{HoodState, Hoods};
use armory::material::MaterialKind;
use armory::sections::hood::HoodStyle;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (HoodState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Hoods::roll(seed);
        let mesh = Hoods::build(&state);
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
        let state = Hoods::roll(seed);
        assert_eq!(Hoods::build(&state), Hoods::build(&state), "seed {seed}");
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
        let h = &s.hood;
        let mail = h.style == HoodStyle::MailCoif;
        let expected = [
            ("hood", true),
            ("liripipe", h.liripipe > 0.0),
            ("trim", s.hem.on && s.hem.hemline && h.style.has_cape()),
            ("binding", mail || s.hem.on && s.hem.neck),
            ("ties", h.ties),
            ("ventail", h.ventail),
            ("buckle", h.ventail),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
        let body = part(&mesh, "hood").unwrap();
        if mail {
            assert_eq!(body.pattern.map(|p| p.kind), Some(PatternKind::Mail), "seed {}", s.seed);
            assert_eq!(body.material, Some(h.material));
        } else {
            assert_eq!(body.material, Some(MaterialKind::Cloth));
        }
    }
}

#[test]
fn hoods_are_head_and_shoulder_sized() {
    for (s, mesh) in built() {
        let h = &s.hood;
        let (lo, hi) = bounds(&mesh.parts);
        assert!(lo[1].abs() < 1e-4, "seed {}: floor at {}", s.seed, lo[1]);
        let (height, width) = (hi[1] - lo[1], hi[0] - lo[0]);
        if h.style.has_cape() {
            // Crown to the cape's hem, across the shoulders.
            assert!(height > 2.4 + h.cape && height < 3.9 + h.cape, "seed {}: {height} tall", s.seed);
            assert!(width > 3.4 && width < 6.5, "seed {}: {width} wide", s.seed);
        } else {
            // Crown to jaw, and the ties hanging below the chin.
            let ties = if h.ties { 0.8 } else { 0.0 };
            assert!(height > 1.6 && height < 2.6 + ties, "seed {}: coif {height} tall", s.seed);
            assert!(width > 1.7 && width < 2.4, "seed {}: coif {width} wide", s.seed);
        }
        assert!((hi[0] + lo[0]).abs() < 0.3, "seed {}: off centre", s.seed);
    }
}
