//! Bow geometry: deterministic, drawable, complete, and laid out where the state says.

use armory::families::Generator;
use armory::families::bow::{Bow, BowState};
use armory::sections::ranged::NockStyle;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (BowState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Bow::roll(seed);
        let mesh = Bow::build(&state);
        (state, mesh)
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

fn bounds(points: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    points.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

fn item_bounds(mesh: &ItemMesh) -> ([f32; 3], [f32; 3]) {
    let all: Vec<[f32; 3]> = mesh.parts.iter().flat_map(|p| p.positions.iter().copied()).collect();
    bounds(&all)
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Bow::roll(seed);
        assert_eq!(Bow::build(&state), Bow::build(&state), "seed {seed}");
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
            ("limbs", true),
            ("riser", true),
            ("grip_wrap", s.riser.wrap),
            ("nocks", s.limbs.nocks != NockStyle::None),
            ("string", true),
            ("serving", s.string.serving),
            ("arrow_shaft", s.arrow.on),
            ("arrow_nock", s.arrow.on),
            ("arrow_head", s.arrow.on),
            ("fletching", s.arrow.on),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn fletching_carries_the_palette_colour() {
    for (s, mesh) in built().filter(|(s, _)| s.arrow.on) {
        assert_eq!(part(&mesh, "fletching").unwrap().tint, Some(s.palette.fletching), "seed {}", s.seed);
    }
}

#[test]
fn bow_stands_about_its_length_with_the_string_behind() {
    for (s, mesh) in built() {
        let (lo, hi) = bounds(&part(&mesh, "limbs").unwrap().positions);
        let height = hi[1] - lo[1];
        let l = s.limbs.length;
        assert!(height > l * 0.6 && height < l * 1.1, "seed {}: limbs {height} tall for length {l}", s.seed);
        assert!((hi[1] + lo[1]).abs() < 0.05 * l, "seed {}: handle not centred", s.seed);
        // The string is on the +X side, about the brace height from the handle.
        let (slo, shi) = bounds(&part(&mesh, "string").unwrap().positions);
        assert!(slo[0] > 0.0, "seed {}: string reaches the handle side, x = {}", s.seed, slo[0]);
        assert!(shi[0] < s.limbs.brace * 1.6, "seed {}: string {} too far from the handle", s.seed, shi[0]);
        // Nothing much wider than a hand, side to side, except a nocked arrow.
        if !s.arrow.on {
            let (ilo, ihi) = item_bounds(&mesh);
            assert!(ihi[2] - ilo[2] < 0.6, "seed {}: {} wide", s.seed, ihi[2] - ilo[2]);
        }
    }
}

#[test]
fn arrow_points_away_from_the_string() {
    for (s, mesh) in built().filter(|(s, _)| s.arrow.on) {
        let (lo, _) = bounds(&part(&mesh, "arrow_head").unwrap().positions);
        let (_, nock_hi) = bounds(&part(&mesh, "arrow_nock").unwrap().positions);
        assert!(lo[0] < -s.arrow.length * 0.5, "seed {}: head at x = {}", s.seed, lo[0]);
        assert!(nock_hi[0] > 0.0, "seed {}: nock not at the string", s.seed);
    }
}
