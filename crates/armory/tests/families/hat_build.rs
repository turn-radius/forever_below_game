//! Hat geometry: deterministic, drawable, complete, and head sized, with a crown above a brim.

use armory::families::Generator;
use armory::families::hat::{HatState, Hats};
use armory::material::MaterialKind;
use armory::sections::hat::HatStyle;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (HatState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Hats::roll(seed);
        let mesh = Hats::build(&state);
        (state, mesh)
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

fn bounds<'a>(parts: impl IntoIterator<Item = &'a Part>) -> ([f32; 3], [f32; 3]) {
    parts.into_iter().flat_map(|p| &p.positions).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Hats::roll(seed);
        assert_eq!(Hats::build(&state), Hats::build(&state), "seed {seed}");
    }
}

#[test]
fn every_part_is_drawable() {
    for (state, mesh) in built() {
        assert!(!mesh.parts.is_empty(), "seed {}: no parts", state.seed);
        for p in &mesh.parts {
            p.validate().unwrap_or_else(|e| panic!("seed {}: {e}", state.seed));
            assert!(!p.indices.is_empty(), "seed {}: {} has no triangles", state.seed, p.name);
            assert_eq!(p.material, Some(MaterialKind::Cloth), "seed {}: {}", state.seed, p.name);
        }
    }
}

#[test]
fn every_enabled_section_has_a_part() {
    for (s, mesh) in built() {
        let h = &s.hat;
        let expected = [("crown", true), ("brim", h.brim > 0.05), ("band", h.band > 0.0), ("feather", h.feather), ("quill", h.feather)];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn hats_are_head_sized_with_the_crown_above_the_brim() {
    for (s, mesh) in built() {
        let h = &s.hat;
        let (lo, hi) = bounds(&mesh.parts);
        assert!(lo[1].abs() < 1e-4, "seed {}: floor at {}", s.seed, lo[1]);
        let width = hi[0] - lo[0];
        // A head is about 1.8 units across; the brim reaches out either side.
        assert!(width > 1.8 + 1.4 * h.brim && width < 2.8 + 2.4 * h.brim, "seed {}: {width} wide, brim {}", s.seed, h.brim);
        assert!((hi[0] + lo[0]).abs() < 0.8, "seed {}: off centre", s.seed);
        let (_, crown_top) = bounds([part(&mesh, "crown").unwrap()]);
        if let Some(brim) = part(&mesh, "brim") {
            let (_, brim_top) = bounds([brim]);
            assert!(crown_top[1] > brim_top[1] + 0.5, "seed {}: the crown does not rise above the brim", s.seed);
        }
        if h.style == HatStyle::Pointed {
            assert!(hi[1] - lo[1] > 2.0, "seed {}: a pointed hat only {} tall", s.seed, hi[1] - lo[1]);
        }
    }
}
