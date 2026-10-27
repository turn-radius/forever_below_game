//! Belt geometry: deterministic, drawable, complete, and waist sized.

use armory::families::Generator;
use armory::families::belt::{BeltState, Belts};
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (BeltState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Belts::roll(seed);
        let mesh = Belts::build(&state);
        (state, mesh)
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Belts::roll(seed);
        assert_eq!(Belts::build(&state), Belts::build(&state), "seed {seed}");
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
        let f = &s.fittings;
        let expected = [
            ("belt", true),
            ("tail", true),
            ("buckle", true),
            ("keeper", f.keeper),
            ("tip", f.tip),
            ("studs", f.studs > 0),
            ("frogs", f.frogs > 0),
            ("loops", f.loops > 0 || f.rings > 0),
            ("rings", f.rings > 0),
            ("pouch", f.pouch),
            ("potion_pouch", f.potion_pouch),
            ("vials", f.potion_pouch),
            ("sack", f.sack),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
        assert_eq!(part(&mesh, "belt").unwrap().material, Some(s.belt.material));
        assert_eq!(part(&mesh, "buckle").unwrap().material, Some(s.belt.buckle));
    }
}

#[test]
fn belts_are_waist_sized_and_stand_on_what_hangs_lowest() {
    for (s, mesh) in built() {
        let f = &s.fittings;
        let (lo, hi) = mesh.bounds().unwrap();
        assert!(lo[1].abs() < 1e-4, "seed {}: floor at {}", s.seed, lo[1]);
        // Round the hips: the mannequin's waist is about 3 units across, 2 deep. A pouch
        // stands out behind the right hip.
        let pouch = if f.pouch { 1.6 } else { 0.0 } + if f.sack { 1.0 } else { 0.0 };
        let width = hi[0] - lo[0];
        assert!(width > 2.8 * f.girth && width < 3.6 * f.girth + 0.8 + pouch, "seed {}: {width} wide", s.seed);
        let depth = hi[2] - lo[2];
        assert!(depth > 1.9 * f.girth && depth < 2.6 * f.girth + 0.8 + pouch, "seed {}: {depth} deep", s.seed);
        let height = hi[1] - lo[1];
        let hanging = (f.tail - 0.45).max(0.0) + if f.frogs > 0 { 0.8 } else { 0.0 } + if f.pouch { 2.0 } else { 0.0 } + if f.sack { 1.6 } else { 0.0 } + if f.potion_pouch { 0.6 } else { 0.0 };
        assert!(height > s.belt.width && height < s.belt.width + f.slant + hanging + 0.4, "seed {}: {height} tall", s.seed);
    }
}
