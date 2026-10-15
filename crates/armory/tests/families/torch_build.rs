//! Torch geometry: deterministic, drawable, complete, held at the origin, and lit where it
//! burns.

use armory::families::Generator;
use armory::families::torch::{BurnState, HandleGrip, Torch, TorchState, TorchStyle};
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (TorchState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Torch::roll(seed);
        let mesh = Torch::build(&state);
        (state, mesh)
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

fn bounds(p: &Part) -> ([f32; 3], [f32; 3]) {
    p.positions.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Torch::roll(seed);
        assert_eq!(Torch::build(&state), Torch::build(&state), "seed {seed}");
    }
}

#[test]
fn every_part_is_drawable() {
    for (state, mesh) in built() {
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
        let burning = s.burn.state == BurnState::Burning;
        let expected = [
            ("handle", true),
            ("grip", s.handle.grip != HandleGrip::Bare),
            ("ferrule", s.handle.ferrule),
            ("head", true),
            ("char", s.burn.char > 0.0),
            ("cage", s.head.style == TorchStyle::Cresset),
            ("rag_tail", s.head.style == TorchStyle::Rag && s.burn.state != BurnState::Spent),
            ("flame", burning),
            ("flame_core", burning),
            ("embers", burning),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
        if s.head.bindings > 0 && s.burn.state == BurnState::Fresh {
            assert!(part(&mesh, "bindings").is_some(), "seed {}: bindings missing", s.seed);
        }
    }
}

#[test]
fn held_at_the_origin_with_the_head_up() {
    for (s, mesh) in built() {
        let (lo, hi) = bounds(part(&mesh, "handle").unwrap());
        assert!(lo[1] < -0.3 * s.handle.length && hi[1] > 0.6 * s.handle.length, "seed {}: handle {lo:?}..{hi:?}", s.seed);
        assert!(lo[0] < 0.0 && hi[0] > 0.0, "seed {}: the hand misses the handle", s.seed);
        let head = ["head", "char"].iter().filter_map(|n| part(&mesh, n)).map(bounds).reduce(|(a, b), (c, d)| {
            ([a[0].min(c[0]), a[1].min(c[1]), a[2].min(c[2])], [b[0].max(d[0]), b[1].max(d[1]), b[2].max(d[2])])
        }).unwrap();
        assert!(head.1[1] > hi[1] - 0.1 && head.0[1] > 0.0, "seed {}: head {head:?}", s.seed);
        assert!(head.1[0] - head.0[0] < 0.9, "seed {}: head too wide", s.seed);
        if let Some(flame) = part(&mesh, "flame") {
            assert!(bounds(flame).1[1] > head.1[1], "seed {}: flame below the head", s.seed);
        }
    }
}

#[test]
fn light_only_while_burning_and_inside_the_flame() {
    for (s, mesh) in built() {
        match Torch::light(&s) {
            Some(light) => {
                let (lo, hi) = bounds(part(&mesh, "flame").unwrap());
                assert!((0..3).all(|k| light.position[k] >= lo[k] && light.position[k] <= hi[k]), "seed {}: light outside the flame", s.seed);
                assert!(light.radius > 40.0 && light.radius < 90.0, "seed {}: radius {}", s.seed, light.radius);
            }
            None => assert_ne!(s.burn.state, BurnState::Burning, "seed {}", s.seed),
        }
    }
}

#[test]
fn flames_and_embers_glow_without_outlines() {
    for (s, mesh) in built() {
        for p in &mesh.parts {
            if matches!(p.name, "flame" | "flame_core" | "embers") {
                assert!(p.glow > 0.5 && !p.outline, "seed {}: {}", s.seed, p.name);
            } else {
                assert_eq!(p.glow, 0.0, "seed {}: {} glows", s.seed, p.name);
            }
        }
    }
}
