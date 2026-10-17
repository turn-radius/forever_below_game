//! Writing geometry: deterministic, drawable, complete, resting on the ground, centred, and
//! a scroll the size of a scroll.

use armory::cond::{Condition, ConditionState};
use armory::families::Generator;
use armory::families::writing::{Tie, Writing, WritingKind, WritingState};
use armory::{ItemMesh, Part};

/// Every third seed: each build costs milliseconds.
fn built(kind: WritingKind) -> impl Iterator<Item = (WritingState, ItemMesh)> {
    (1..=256u32).step_by(3).map(move |seed| {
        let state = Writing::roll_as(seed, kind);
        let mesh = Writing::build(&state);
        (state, mesh)
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

fn bounds<'a>(parts: impl IntoIterator<Item = &'a Part>) -> ([f32; 3], [f32; 3]) {
    parts.into_iter().flat_map(|p| p.positions.iter()).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Writing::roll(seed);
        assert_eq!(Writing::build(&state), Writing::build(&state), "seed {seed}");
    }
}

#[test]
fn every_part_is_drawable_with_outline_normals() {
    for kind in WritingKind::ALL {
        for (state, mesh) in built(kind) {
            assert!(!mesh.parts.is_empty(), "seed {}: {kind:?} is empty", state.seed);
            for p in &mesh.parts {
                p.validate().unwrap_or_else(|e| panic!("seed {}: {e}", state.seed));
                assert!(!p.indices.is_empty() && p.material.is_some(), "seed {}: {} is incomplete", state.seed, p.name);
                assert!(p.positions.iter().flatten().all(|v| v.is_finite()), "seed {}: {} not finite", state.seed, p.name);
                assert_eq!(p.outline_normals.len(), p.positions.len(), "seed {}: {} has no outline normals", state.seed, p.name);
                assert!(p.outline_normals.iter().flatten().all(|v| v.is_finite()), "seed {}: {} outline normals", state.seed, p.name);
            }
        }
    }
}

#[test]
fn every_enabled_section_has_a_part() {
    for (s, mesh) in built(WritingKind::Scroll) {
        let sc = &s.scroll;
        let shut = sc.is_shut();
        let expected = [
            ("sheet", true),
            ("rollers", sc.rollers > 0),
            ("knobs", sc.rollers > 0),
            ("ribbon", shut && sc.tie == Tie::Ribbon),
            ("cord", shut && sc.tie == Tie::Cord),
            ("seal", shut && sc.seal),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
        if shut {
            assert!(part(&mesh, "ink").is_none(), "seed {}: ink on a shut scroll", s.seed);
        }
    }
}

#[test]
fn an_unrolled_scroll_shows_ink_on_its_face() {
    let seen: Vec<_> = built(WritingKind::Scroll).filter(|(s, _)| s.scroll.unroll > 0.35 && s.piece.lines > 0).collect();
    assert!(!seen.is_empty());
    for (s, mesh) in seen {
        let ink = part(&mesh, "ink").unwrap_or_else(|| panic!("seed {}: no ink", s.seed));
        let (ilo, _) = bounds([ink]);
        let (_, shi) = bounds([part(&mesh, "sheet").unwrap()]);
        assert!(ilo[1] > 0.0 && ilo[1] < shi[1] + 0.05, "seed {}", s.seed);
        let up = ink.normals.iter().filter(|n| n[1] > 0.0).count();
        assert!(up * 10 > ink.normals.len() * 9, "seed {}: ink faces down", s.seed);
    }
}

#[test]
fn rests_on_the_ground_centred_at_scroll_size() {
    for (s, mesh) in built(WritingKind::Scroll) {
        let (lo, hi) = bounds(&mesh.parts);
        assert!(lo[1].abs() < 0.03, "seed {}: bottom at {}", s.seed, lo[1]);
        let (dx, dz) = (hi[0] - lo[0], hi[2] - lo[2]);
        let long = dx.max(dz);
        assert!((2.0..4.0).contains(&long), "seed {}: {:?} long side {long}", s.seed, s.scroll);
        assert!(dx >= dz - 1e-3, "seed {}: long side is not along X ({dx} x {dz})", s.seed);
        assert!(((lo[0] + hi[0]) * 0.5).abs() < 0.05 && ((lo[2] + hi[2]) * 0.5).abs() < 0.05, "seed {}: off centre", s.seed);
    }
}

#[test]
fn wear_ragged_the_edges_and_stains_the_sheet() {
    let mut s = Writing::roll_as(5, WritingKind::Scroll);
    s.scroll.unroll = 0.7;
    s.scroll.tie = Tie::None;
    s.scroll.seal = false;
    s.piece.lines = 8;
    let clean = Writing::build(&s);
    s.cond = Condition::of(ConditionState::GraveFind);
    let worn = Writing::build(&s);
    let (c, w) = (part(&clean, "sheet").unwrap(), part(&worn, "sheet").unwrap());
    assert_ne!(c.positions, w.positions);
    assert_ne!(c.tint, w.tint, "paper does not fox");
    assert!(part(&clean, "foxing").is_none() && part(&clean, "scorch").is_none());
    assert!(part(&worn, "foxing").is_some() && part(&worn, "scorch").is_some());
}

#[test]
fn note_and_book_stand_ins_build() {
    for kind in [WritingKind::Note, WritingKind::Book] {
        for (s, mesh) in built(kind).take(6) {
            let (lo, hi) = bounds(&mesh.parts);
            assert!(lo[1].abs() < 0.03 && hi[1] > 0.0, "seed {}", s.seed);
        }
    }
}
