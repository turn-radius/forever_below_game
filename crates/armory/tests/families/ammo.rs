//! Arrows, bolts, quivers and bolt cases: deterministic, drawable, and honest about what they hold.

use armory::families::Generator;
use armory::families::ammo::{Arrows, Bolts};
use armory::families::bow::Bow;
use armory::families::crossbow::Crossbow;
use armory::families::quiver::{BoltCases, Quivers};
use armory::{Family, Item, ItemMesh};

fn names(mesh: &ItemMesh) -> Vec<&str> {
    mesh.parts.iter().map(|p| p.name).collect()
}

fn triangles(mesh: &ItemMesh) -> usize {
    mesh.parts.iter().map(|p| p.indices.len() / 3).sum()
}

fn drawable(mesh: &ItemMesh, seed: u32) {
    assert!(!mesh.parts.is_empty());
    for p in &mesh.parts {
        p.validate().unwrap_or_else(|e| panic!("seed {seed}: {e}"));
    }
}

#[test]
fn same_seed_same_state() {
    for seed in [0, 1, 7, 1207, u32::MAX] {
        assert_eq!(Arrows::roll(seed), Arrows::roll(seed));
        assert_eq!(Bolts::roll(seed), Bolts::roll(seed));
        assert_eq!(Quivers::roll(seed), Quivers::roll(seed));
        assert_eq!(BoltCases::roll(seed), BoltCases::roll(seed));
        assert_eq!(Quivers::stocked(seed, 0.5), Quivers::stocked(seed, 0.5));
        assert_eq!(BoltCases::stocked(seed, 1.0), BoltCases::stocked(seed, 1.0));
        for family in [Family::Arrow, Family::Bolt, Family::Quiver, Family::BoltCase] {
            assert_eq!(Item::roll(family, seed), Item::roll(family, seed));
        }
    }
}

#[test]
fn bolts_are_shorter_stouter_and_unnocked() {
    for seed in 1..=32 {
        let (a, b) = (Arrows::roll(seed), Bolts::roll(seed));
        assert!(b.arrow.length < a.arrow.length && b.arrow.radius > a.arrow.radius, "seed {seed}");
        assert_eq!(b.arrow.fletches, 2);
        assert!((3..=4).contains(&a.arrow.fletches));
        assert!(!names(&Bolts::build(&b)).contains(&"nock"));
        assert!(names(&Arrows::build(&a)).contains(&"nock"));
    }
}

#[test]
fn ammunition_builds_drawable_meshes() {
    for seed in 1..=64 {
        for mesh in [Arrows::build(&Arrows::roll(seed)), Bolts::build(&Bolts::roll(seed))] {
            drawable(&mesh, seed);
            assert!(names(&mesh).contains(&"fletching") && names(&mesh).contains(&"head"));
        }
    }
}

#[test]
fn a_quiver_shows_its_arrows() {
    for seed in 1..=24 {
        let (empty, half, full) = (Quivers::stocked(seed, 0.0), Quivers::stocked(seed, 0.5), Quivers::stocked(seed, 1.0));
        assert!(Quivers::capacity(&empty) >= 6, "seed {seed}");
        assert!(empty.contents.is_empty());
        assert!(!half.contents.is_empty() && half.contents.len() < full.contents.len());
        let (me, mh, mf) = (Quivers::build(&empty), Quivers::build(&half), Quivers::build(&full));
        assert!(me.parts.len() < mf.parts.len(), "seed {seed}");
        assert!(triangles(&me) < triangles(&mh) && triangles(&mh) < triangles(&mf), "seed {seed}");
        assert!(names(&mf).contains(&"ammo_nocks") && names(&mf).contains(&"ammo_fletching"));
        assert!(!names(&me).contains(&"ammo_fletching"));
        drawable(&me, seed);
        drawable(&mf, seed);
    }
}

#[test]
fn a_bolt_case_shows_its_bolts() {
    for seed in 1..=24 {
        let (empty, half, full) = (BoltCases::stocked(seed, 0.0), BoltCases::stocked(seed, 0.5), BoltCases::stocked(seed, 1.0));
        assert!(BoltCases::capacity(&empty) >= 6, "seed {seed}");
        assert!(!half.contents.is_empty() && half.contents.len() < full.contents.len());
        let (me, mh, mf) = (BoltCases::build(&empty), BoltCases::build(&half), BoltCases::build(&full));
        assert!(me.parts.len() < mf.parts.len(), "seed {seed}");
        assert!(triangles(&me) < triangles(&mh) && triangles(&mh) < triangles(&mf), "seed {seed}");
        assert!(names(&mf).contains(&"ammo_fletching") && !names(&me).contains(&"ammo_fletching"));
        assert!(!names(&mf).contains(&"ammo_nocks"), "bolts have no nocks");
        drawable(&me, seed);
        drawable(&mf, seed);
    }
}

#[test]
fn ammunition_loads_onto_a_bow_and_a_crossbow() {
    let (arrow, bolt) = (Arrows::roll(5), Bolts::roll(5));
    let mut bow = Bow::roll(3);
    bow.arrow.on = false;
    assert!(!names(&Bow::build(&bow)).contains(&"arrow_shaft"));
    bow.load(&arrow);
    assert_eq!(bow.arrow.length, arrow.arrow.length);
    assert!(names(&Bow::build(&bow)).contains(&"arrow_shaft"));
    bow.unload();
    assert!(!names(&Bow::build(&bow)).contains(&"arrow_shaft"));
    let mut crossbow = Crossbow::roll(3);
    crossbow.load(&bolt);
    assert!(names(&Crossbow::build(&crossbow)).contains(&"arrow_shaft"));
    assert_eq!(crossbow.palette.fletching, bolt.palette.fletching);
}
