//! Club geometry: deterministic, drawable, complete, and laid out where the state says.

use armory::families::Generator;
use armory::families::club::{Club, ClubState};
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (ClubState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Club::roll(seed);
        let mesh = Club::build(&state);
        (state, mesh)
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

/// `(min, max)` over a part's positions.
fn bounds(part: &Part) -> ([f32; 3], [f32; 3]) {
    part.positions.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Club::roll(seed);
        assert_eq!(Club::build(&state), Club::build(&state), "seed {seed}");
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
        let expected = [
            ("shaft", true),
            ("bands", s.shaft.bands > 0),
            ("grip", s.grip.on),
            ("wrap", s.wrap.on),
            ("pommel", s.pommel.on),
            ("head", s.has_knob()),
            ("spikes", s.spikes.on),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn the_striking_end_is_the_heavy_end() {
    for (s, mesh) in built() {
        let shaft = part(&mesh, "shaft").unwrap();
        let width_at = |y0: f32, y1: f32| {
            shaft.positions.iter().filter(|p| p[1] >= y0 && p[1] <= y1).map(|p| p[0].hypot(p[2])).fold(0.0f32, f32::max)
        };
        let l = s.shaft.length;
        let handle = width_at(0.1 * l, 0.25 * l);
        if s.swell.on {
            let end = width_at(0.75 * l, l);
            assert!(end > handle * 1.3, "seed {}: swelling {end} against a handle {handle}", s.seed);
            // The end is rounded off, not cut flat.
            let (_, hi) = bounds(shaft);
            assert!(hi[1] <= l + 0.05 && hi[1] > l - 0.1, "seed {}: shaft ends at {} for {l}", s.seed, hi[1]);
        } else {
            let (lo, hi) = bounds(part(&mesh, "head").unwrap());
            assert!(lo[1] < l && hi[1] > l + s.head.length * 0.9, "seed {}: knob from {} to {}", s.seed, lo[1], hi[1]);
            assert!(hi[0] - lo[0] > s.shaft.radius * 3.0, "seed {}: knob no wider than the shaft", s.seed);
        }
    }
}

#[test]
fn studs_stand_on_the_striking_end() {
    for (s, mesh) in built().filter(|(s, _)| s.spikes.on) {
        let (lo, hi) = bounds(part(&mesh, "spikes").unwrap());
        let l = s.shaft.length;
        assert!(lo[1] > l * s.swell.start - 0.1, "seed {}: studs start at {} below the swelling", s.seed, lo[1]);
        assert!(hi[1] < l + 0.2, "seed {}: studs reach {} past the end", s.seed, hi[1]);
        // How far the studs stand proud of the shaft's surface at their own height.
        let shaft = part(&mesh, "shaft").unwrap();
        let surface = |y: f32| shaft.positions.iter().filter(|p| (p[1] - y).abs() < 0.04).map(|p| p[0].hypot(p[2])).fold(0.0f32, f32::max);
        let spikes = part(&mesh, "spikes").unwrap();
        let proud = spikes.positions.iter().map(|p| p[0].hypot(p[2]) - surface(p[1])).fold(f32::MIN, f32::max);
        assert!(proud > s.spikes.length * 0.3, "seed {}: studs stand {proud} proud", s.seed);
        assert!(proud < s.spikes.length + 0.1, "seed {}: studs float {proud} off the shaft", s.seed);
    }
}

#[test]
fn bands_ring_the_striking_end() {
    for (s, mesh) in built().filter(|(s, _)| s.shaft.bands > 0) {
        let (lo, hi) = bounds(part(&mesh, "bands").unwrap());
        let l = s.shaft.length;
        assert!(lo[1] > l * s.swell.start && hi[1] < l, "seed {}: bands from {} to {}", s.seed, lo[1], hi[1]);
    }
}

#[test]
fn clubs_are_one_handed() {
    for (s, mesh) in built() {
        let (lo, hi) = mesh.parts.iter().map(bounds).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), (a, b)| {
            ([lo[0].min(a[0]), lo[1].min(a[1]), lo[2].min(a[2])], [hi[0].max(b[0]), hi[1].max(b[1]), hi[2].max(b[2])])
        });
        let floor = if s.pommel.on { -s.pommel.length } else { 0.0 } - 0.05;
        assert!(lo[1] >= floor, "seed {}: reaches y = {}, below {floor}", s.seed, lo[1]);
        let height = hi[1] - lo[1];
        assert!(height > 2.5 && height < 7.5, "seed {}: {height} tall", s.seed);
        for k in [0, 2] {
            assert!(hi[k] - lo[k] < 1.6, "seed {}: {} wide", s.seed, hi[k] - lo[k]);
        }
    }
}
