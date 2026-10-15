//! Lantern geometry: deterministic, drawable, complete, hung from its ring, and lit inside.

use armory::families::Generator;
use armory::families::lantern::{Lantern, LanternFuel, LanternState, LanternStyle};
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (LanternState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Lantern::roll(seed);
        let mesh = Lantern::build(&state);
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
        let state = Lantern::roll(seed);
        assert_eq!(Lantern::build(&state), Lantern::build(&state), "seed {seed}");
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
        let (b, f) = (&s.body, &s.flame);
        let expected = [
            ("ring", true),
            ("chain", s.bail.links > 0),
            ("bail", true),
            ("frame", true),
            ("panes", b.style == LanternStyle::Horn),
            ("holes", b.style == LanternStyle::Pierced),
            ("glass", b.style == LanternStyle::Cage),
            ("candle", f.fuel == LanternFuel::Candle),
            ("socket", f.fuel == LanternFuel::Candle),
            ("font", f.fuel == LanternFuel::Oil),
            ("wick", true),
            ("flame", f.lit),
            ("flame_core", f.lit),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn hangs_below_its_ring() {
    for (s, mesh) in built() {
        let (rlo, rhi) = bounds(part(&mesh, "ring").unwrap());
        assert!(rlo[1] < 0.0 && rhi[1] > 0.0, "seed {}: the hand misses the ring", s.seed);
        let (lo, hi) = bounds(part(&mesh, "frame").unwrap());
        assert!(hi[1] < rlo[1] + 0.05, "seed {}: the body rises into the ring", s.seed);
        assert!(lo[1] > -4.5 && hi[0] - lo[0] < 1.3, "seed {}: frame {lo:?}..{hi:?}", s.seed);
        let (blo, bhi) = bounds(part(&mesh, "bail").unwrap());
        assert!(bhi[1] > hi[1] && blo[1] < hi[1], "seed {}: the bail does not reach the hood", s.seed);
    }
}

#[test]
fn the_flame_and_its_light_are_inside_the_body() {
    for (s, mesh) in built() {
        let (lo, hi) = bounds(part(&mesh, "frame").unwrap());
        match Lantern::light(&s) {
            Some(light) => {
                let (flo, fhi) = bounds(part(&mesh, "flame").unwrap());
                assert!(fhi[1] < hi[1] - s.body.width * 0.3 && flo[1] > lo[1], "seed {}: flame {flo:?}..{fhi:?} outside", s.seed);
                assert!(light.position[1] > flo[1] && light.position[1] < fhi[1], "seed {}", s.seed);
                assert!(light.radius > 70.0 && light.radius < 110.0, "seed {}: radius {}", s.seed, light.radius);
            }
            None => assert!(!s.flame.lit, "seed {}", s.seed),
        }
    }
}
