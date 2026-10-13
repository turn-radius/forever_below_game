//! Garment geometry: deterministic, drawable, complete, and the size of a garment.

use armory::families::Generator;
use armory::families::garment::{Garment, GarmentState};
use armory::fabric::PatternKind;
use armory::material::MaterialKind;
use armory::sections::garment::{GarmentStyle, Neckline};
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (GarmentState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Garment::roll(seed);
        let mesh = Garment::build(&state);
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
        let state = Garment::roll(seed);
        assert_eq!(Garment::build(&state), Garment::build(&state), "seed {seed}");
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
        let style = s.cut.style;
        let rigid = style == GarmentStyle::Cuirass;
        let cloak = style == GarmentStyle::Cloak;
        let sleeved = s.cut.sleeves.length() > 0.0 && !cloak && !rigid && style != GarmentStyle::Brigandine;
        let expected = [
            ("body", true),
            ("sleeves", sleeved),
            ("trim", s.hem.on && s.hem.hemline),
            ("collar", s.cut.neck == Neckline::High),
            ("cowl", s.cut.neck == Neckline::Cowl),
            ("hood", s.cut.neck == Neckline::Cowl),
            ("belt", s.belt.on && !cloak && !rigid),
            ("buckle", s.belt.on && !cloak && !rigid),
            ("faulds", rigid && s.plates.faulds > 0),
            ("pauldrons", s.plates.pauldrons),
            ("edges", rigid),
            ("brooch", cloak),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn cloth_carries_its_dye_and_metal_its_pattern() {
    for (s, mesh) in built() {
        let body = part(&mesh, "body").unwrap();
        match s.cut.style {
            GarmentStyle::Hauberk => assert_eq!(body.pattern.map(|p| p.kind), Some(PatternKind::Mail), "seed {}", s.seed),
            GarmentStyle::Scale => assert_eq!(body.pattern.map(|p| p.kind), Some(PatternKind::Scales), "seed {}", s.seed),
            GarmentStyle::Cuirass => assert_eq!(body.material, Some(s.cut.material), "seed {}", s.seed),
            GarmentStyle::Gambeson => assert_eq!(body.pattern.map(|p| p.kind), Some(PatternKind::Quilted), "seed {}", s.seed),
            _ => {
                assert_eq!(body.material, Some(MaterialKind::Cloth), "seed {}", s.seed);
                assert_eq!(body.tint, Some(s.cut.cloth.color), "seed {}", s.seed);
            }
        }
    }
}

#[test]
fn garments_are_the_size_of_a_garment() {
    for (s, mesh) in built() {
        let (lo, hi) = bounds(&mesh.parts);
        let height = hi[1] - lo[1];
        let width = hi[0] - lo[0];
        assert!(lo[1] > -3.5, "seed {}: hangs to {}", s.seed, lo[1]);
        assert!((4.5..16.0).contains(&height), "seed {}: {height} tall", s.seed);
        assert!((3.0..14.0).contains(&width), "seed {}: {width} wide", s.seed);
        // Left-right symmetric enough to stand square.
        assert!((hi[0] + lo[0]).abs() < 0.6, "seed {}: off centre {} {}", s.seed, lo[0], hi[0]);
        // The hem is at the bottom: the body reaches within a fold of y = 0.
        let body_lo = bounds(std::slice::from_ref(part(&mesh, "body").unwrap())).0[1];
        let rigid = s.cut.style == GarmentStyle::Cuirass;
        let expected = if rigid { s.cut.length - 0.15 } else { 0.0 };
        assert!((body_lo - expected).abs() < 0.3, "seed {}: body bottom {body_lo}, expected {expected}", s.seed);
    }
}

#[test]
fn longer_cuts_hang_lower() {
    let mut s = Garment::roll_as(5, GarmentStyle::Tunic);
    s.cut.length = 2.0;
    let short = bounds(&Garment::build(&s).parts);
    s.cut.length = 6.0;
    let long = bounds(&Garment::build(&s).parts);
    assert!(long.1[1] - long.0[1] > short.1[1] - short.0[1] + 3.5);
}
