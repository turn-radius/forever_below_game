//! Axe geometry: deterministic, drawable, complete, and laid out where the state says.

use armory::families::Generator;
use armory::families::axe::{Axe, AxeState};
use armory::sections::axe_head::AxePoll;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (AxeState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Axe::roll(seed);
        let mesh = Axe::build(&state);
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
        let state = Axe::roll(seed);
        assert_eq!(Axe::build(&state), Axe::build(&state), "seed {seed}");
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
            ("grip", s.grip.on),
            ("wrap", s.wrap.on),
            ("pommel", s.pommel.on),
            ("eye", true),
            ("bit", true),
            ("poll", matches!(s.head.poll, AxePoll::Hammer | AxePoll::Spike)),
            ("langets", s.eye.langets > 0),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn the_head_sits_at_the_top_of_the_haft() {
    for (s, mesh) in built() {
        let l = s.shaft.length;
        let (elo, ehi) = bounds(part(&mesh, "eye").unwrap());
        assert!(ehi[1] < l && ehi[1] > l - 0.15, "seed {}: eye tops out at {} on a haft of {l}", s.seed, ehi[1]);
        assert!((ehi[1] - elo[1] - s.eye.length).abs() < 0.08, "seed {}: eye {} tall for {}", s.seed, ehi[1] - elo[1], s.eye.length);
        let (blo, bhi) = bounds(part(&mesh, "bit").unwrap());
        let mid = (elo[1] + ehi[1]) * 0.5;
        assert!(bhi[1] > mid && blo[1] < mid, "seed {}: bit does not cross the eye", s.seed);
        // The bit stands out toward +X, crosswise to the haft; a second bit toward -X.
        assert!(bhi[0] > s.head.reach * 0.9 && bhi[0] < s.head.reach + 0.05, "seed {}: bit reaches {} for {}", s.seed, bhi[0], s.head.reach);
        if s.head.poll == AxePoll::Bit {
            assert!(blo[0] < -s.head.reach * 0.9, "seed {}: no second bit", s.seed);
        } else {
            assert!(blo[0] > -0.3, "seed {}: bit reaches back to {}", s.seed, blo[0]);
        }
        let edge = s.head.edge_height();
        assert!(bhi[1] - blo[1] > edge * 0.7 && bhi[1] - blo[1] < edge + s.head.droop.abs() + 0.2, "seed {}: bit {} tall for an edge of {edge}", s.seed, bhi[1] - blo[1]);
        assert!(bhi[2] - blo[2] <= s.head.thickness * 1.6 + 0.01, "seed {}: bit {} thick", s.seed, bhi[2] - blo[2]);
        if let Some(poll) = part(&mesh, "poll") {
            let (plo, _) = bounds(poll);
            assert!(plo[0] < -s.head.poll_length * 0.8, "seed {}: poll reaches only {}", s.seed, plo[0]);
        }
    }
}

#[test]
fn langets_run_down_from_the_eye() {
    for (s, mesh) in built().filter(|(s, _)| s.eye.langets > 0) {
        let (llo, lhi) = bounds(part(&mesh, "langets").unwrap());
        let (elo, _) = bounds(part(&mesh, "eye").unwrap());
        assert!(lhi[1] <= elo[1] + 0.1 && lhi[1] > elo[1] - 0.1, "seed {}: langets start at {} under an eye at {}", s.seed, lhi[1], elo[1]);
        assert!((lhi[1] - llo[1] - s.eye.langet_length).abs() < 0.2, "seed {}: langets {} long", s.seed, lhi[1] - llo[1]);
    }
}

#[test]
fn a_broken_edge_loses_a_chunk() {
    for seed in [2, 9, 15] {
        let mut s = Axe::roll(seed);
        let whole = Axe::build(&s);
        s.cond.broken = true;
        let mesh = Axe::build(&s);
        let bit = part(&mesh, "bit").unwrap();
        bit.validate().unwrap();
        assert_ne!(bit.positions, part(&whole, "bit").unwrap().positions, "seed {seed}: breaking changed nothing");
    }
}

#[test]
fn axes_are_their_haft_long() {
    for (s, mesh) in built() {
        let (lo, hi) = mesh.parts.iter().map(bounds).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), (a, b)| {
            ([lo[0].min(a[0]), lo[1].min(a[1]), lo[2].min(a[2])], [hi[0].max(b[0]), hi[1].max(b[1]), hi[2].max(b[2])])
        });
        let floor = if s.pommel.on { -s.pommel.length } else { 0.0 } - 0.05;
        assert!(lo[1] >= floor, "seed {}: reaches y = {}, below {floor}", s.seed, lo[1]);
        let height = hi[1] - lo[1];
        let l = s.shaft.length;
        assert!(height > l - 0.1 && height < l + s.head.toe + s.head.droop.max(0.0) + 0.9, "seed {}: {height} tall on a haft of {l}", s.seed);
        assert!(hi[2] - lo[2] < 0.8, "seed {}: {} deep", s.seed, hi[2] - lo[2]);
    }
}
