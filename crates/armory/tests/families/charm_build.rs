//! Charm geometry: deterministic, drawable, complete, and hung from its hanger.

use armory::build::key::{KeyBow, key};
use armory::families::Generator;
use armory::geom::OutlineMode;
use armory::families::charm::{Charm, CharmShape, CharmState, HangerStyle};
use armory::sections::device::Device;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (CharmState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Charm::roll(seed);
        let mesh = Charm::build(&state);
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
        let state = Charm::roll(seed);
        assert_eq!(Charm::build(&state), Charm::build(&state), "seed {seed}");
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
        let c = &s.charm;
        let h = &s.hanger;
        let expected = [
            ("charm", true),
            ("cap", c.shape == CharmShape::Acorn),
            ("clapper", c.shape == CharmShape::Bell),
            ("bindings", c.bindings > 0),
            ("stones", c.inset),
            ("device", c.device != Device::None),
            ("hanger", true),
            ("hanger_ring", h.style == HangerStyle::ChainStub),
            ("knot", h.style == HangerStyle::CordLoop),
            ("beads", h.style == HangerStyle::CordLoop && h.beads > 0),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn the_charm_hangs_below_its_hanger() {
    for (s, mesh) in built() {
        let size = s.charm.size;
        let (lo, hi) = bounds(part(&mesh, "charm").unwrap());
        assert!(lo[1] > -size * 1.15 && lo[1] < -size * 0.6, "seed {}: {:?} bottom at {}", s.seed, s.charm.shape, lo[1]);
        assert!(hi[1] < size * 0.2, "seed {}: {:?} top at {}", s.seed, s.charm.shape, hi[1]);
        assert!(hi[0] - lo[0] < size * 1.2 && hi[2] - lo[2] < size * 1.2, "seed {}: too wide", s.seed);
        // The top of the charm proper (an acorn's cap sits over its nut).
        let top = ["charm", "cap"].iter().filter_map(|n| part(&mesh, n)).map(|p| bounds(p).1[1]).fold(f32::MIN, f32::max);
        let hi = [hi[0], top, hi[2]];
        let (hlo, hhi) = bounds(part(&mesh, "hanger").unwrap());
        assert!(hlo[1] < hi[1] + 0.01, "seed {}: hanger floats above the charm", s.seed);
        assert!(hhi[1] > hi[1], "seed {}: hanger does not rise above the charm", s.seed);
    }
}

#[test]
fn small_parts_skip_the_outline() {
    for (s, mesh) in built() {
        for p in &mesh.parts {
            if matches!(p.name, "stones" | "device" | "clapper") {
                assert!(!p.outline, "seed {}: {} outlined", s.seed, p.name);
            }
        }
    }
}

#[test]
fn key_charms_are_shared_keys() {
    for seed in 1..=64 {
        let s = Charm::roll_as(seed, CharmShape::Key);
        let c = &s.charm;
        let mesh = Charm::build(&s);
        let body = part(&mesh, "charm").unwrap();
        let wards = vec![0.5; c.detail.clamp(1, 4) as usize];
        let shared = key(c.size, c.girth, KeyBow::Ring, &wards).into_part("charm", OutlineMode::Welded);
        assert_eq!(body.positions.len(), shared.positions.len(), "seed {seed}: not the shared key");
        let (lo, hi) = (bounds(body), bounds(&shared));
        assert!((lo.0[1] - hi.0[1]).abs() < 1e-4 && (lo.1[1] - hi.1[1]).abs() < 1e-4, "seed {seed}: {lo:?} vs {hi:?}");
    }
}
