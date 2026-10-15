//! Sword geometry: deterministic, drawable, complete, and laid out where the state says.

use armory::cond::{Condition, ConditionState};
use armory::families::Generator;
use armory::families::sword::{Sword, SwordState};
use armory::sections::hafted::PommelGem;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (SwordState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Sword::roll(seed);
        let mesh = Sword::build(&state);
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
        let state = Sword::roll(seed);
        assert_eq!(Sword::build(&state), Sword::build(&state), "seed {seed}");
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
            // The grip covers the whole handle, so the core under it is not built.
            ("shaft", !s.grip.on),
            ("grip", s.grip.on),
            ("wrap", s.wrap.on),
            ("rivets", s.grip.on && s.grip.rivets > 0),
            ("pommel", s.pommel.on),
            ("pommel_stones", s.pommel.on && s.pommel.gem != PommelGem::None),
            ("guard", s.guard.on),
            ("guard_stones", s.guard.on && s.guard.gem),
            ("blade", true),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn blade_rises_from_the_guard_to_its_length() {
    for (s, mesh) in built() {
        let top = s.shaft.length;
        let (lo, hi) = bounds(part(&mesh, "blade").unwrap());
        assert!(lo[1] > top - 0.05, "seed {}: blade base {} below the handle top {top}", s.seed, lo[1]);
        let reach = hi[1] - lo[1];
        // A sabre's curve shortens its reach a little.
        assert!(reach > s.blade.length * 0.9 && reach < s.blade.length + 0.01, "seed {}: blade reach {reach} for length {}", s.seed, s.blade.length);
        assert!(hi[0] - lo[0] < s.blade.width * 2.0 + s.blade.curve.abs() * s.blade.length * 0.2, "seed {}: blade too wide", s.seed);
        let thickest = s.blade.thickness * (1.0 + 0.55 * s.blade.spine + 0.35 * f32::from(s.blade.ricasso > 0.0));
        assert!(hi[2] - lo[2] <= thickest + 1e-3, "seed {}: blade thicker than {thickest}", s.seed);
        if let Some(guard) = part(&mesh, "guard") {
            let (glo, ghi) = bounds(guard);
            assert!(glo[1] < lo[1] + 0.05 && ghi[1] > top - 0.1, "seed {}: guard not at the blade's base", s.seed);
            if s.guard.style.has_arms() {
                assert!(ghi[0] - glo[0] > s.guard.span, "seed {}: guard narrower than its span", s.seed);
            }
        }
    }
}

#[test]
fn nothing_hangs_below_the_pommel() {
    for (s, mesh) in built() {
        let floor = -s.pommel.length - 0.02;
        for p in &mesh.parts {
            let lo = bounds(p).0[1];
            assert!(lo >= floor, "seed {}: {} reaches y = {lo}, below {floor}", s.seed, p.name);
        }
    }
}

#[test]
fn damage_nicks_and_breaks_the_blade() {
    let mut s = Sword::roll(5);
    let clean = bounds(part(&Sword::build(&s), "blade").unwrap());
    s.cond = Condition { broken: true, ..Condition::of(ConditionState::GraveFind) };
    let mesh = Sword::build(&s);
    for p in &mesh.parts {
        p.validate().unwrap();
    }
    let broken = bounds(part(&mesh, "blade").unwrap());
    assert!(broken.1[1] < clean.1[1] - s.blade.length * 0.15, "a broken point should shorten the blade");
}
