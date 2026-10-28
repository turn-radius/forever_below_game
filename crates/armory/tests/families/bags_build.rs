//! Bag geometry: deterministic, drawable, complete, standing on the floor, and holding
//! what its container space says.

use armory::families::Generator;
use armory::families::bags::{BagState, BagStyle, Bags};
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (BagState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Bags::roll(seed);
        let mesh = Bags::build(&state);
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
        let state = Bags::roll(seed);
        assert_eq!(Bags::build(&state), Bags::build(&state), "seed {seed}");
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
        let b = &s.bag;
        let flapped = !b.style.is_drawstring();
        let expected = [
            ("body", true),
            ("flap", flapped),
            ("straps", flapped),
            ("cord", b.style.is_drawstring()),
            ("fittings", true),
            ("coins", b.coins),
            ("pocket", b.pocket),
            ("lashing", b.lashing),
            ("weapon_strap", b.weapon_strap),
            ("patch", b.patch),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn fabric_parts_carry_their_colour() {
    for (s, mesh) in built() {
        assert_eq!(part(&mesh, "body").unwrap().tint, Some(s.bag.cloth.color), "seed {}", s.seed);
        assert_eq!(part(&mesh, "body").unwrap().material, Some(s.bag.fabric.material()), "seed {}", s.seed);
        if let Some(flap) = part(&mesh, "flap") {
            assert_eq!(flap.tint, Some(s.bag.cloth.color2), "seed {}", s.seed);
        }
    }
}

#[test]
fn patterned_bodies_carry_their_surface_pattern() {
    let mut patterned = 0;
    for s in (1..=512).map(Bags::roll) {
        let mesh = Bags::build(&s);
        let expected = s.bag.cloth.surface();
        patterned += u32::from(expected.is_some());
        assert_eq!(part(&mesh, "body").unwrap().pattern, expected, "seed {}", s.seed);
        if let Some(pocket) = part(&mesh, "pocket") {
            assert_eq!(pocket.pattern, expected, "seed {}", s.seed);
        }
        for plain in ["flap", "patch", "lashing", "straps"] {
            if let Some(p) = part(&mesh, plain) {
                assert_eq!(p.pattern, None, "seed {}: patterned {plain}", s.seed);
            }
        }
    }
    assert!(patterned > 0, "no patterned bag in 512 seeds");
}

#[test]
fn bags_stand_on_the_floor_at_a_sensible_size() {
    for (s, mesh) in built() {
        let (lo, hi) = bounds(&mesh.parts);
        assert!(lo[1] > -0.05 && lo[1] < 0.05, "seed {}: lowest point y = {}", s.seed, lo[1]);
        let tall = hi[1] - lo[1];
        let (min, max) = match s.bag.style {
            BagStyle::Purse => (0.5, 2.0),
            BagStyle::Pouch | BagStyle::BeltPouch => (0.8, 3.5),
            BagStyle::Satchel => (3.0, 9.0),
            BagStyle::Backpack => (3.0, 9.0),
            BagStyle::Sack => (3.0, 8.0),
        };
        assert!(tall > min && tall < max, "seed {}: {:?} is {tall} tall", s.seed, s.bag.style);
        // Nothing strays far from the bag's own footprint.
        let body = bounds(part(&mesh, "body"));
        for k in [0, 2] {
            let reach = (body.1[k] - body.0[k]) * 1.2;
            assert!(lo[k] > body.0[k] - reach && hi[k] < body.1[k] + reach, "seed {}: parts stray along axis {k}", s.seed);
        }
    }
}

#[test]
fn the_container_space_sits_inside_the_body() {
    for (s, mesh) in built() {
        let space = Bags::container(&s);
        let (lo, hi) = bounds(part(&mesh, "body"));
        for k in 0..3 {
            assert!(space.interior_min[k] >= lo[k] - 1e-3 && space.interior_max[k] <= hi[k] + 1e-3, "seed {}: interior leaves the body on axis {k}", s.seed);
        }
        assert!(space.contains(space.opening.center) || (space.opening.center[1] - space.interior_max[1]).abs() < 1e-3, "seed {}", s.seed);
        assert!((space.opening.normal[1] - 1.0).abs() < 1e-6, "seed {}: mouth not facing up", s.seed);
        let (min, max) = match s.bag.style {
            BagStyle::Purse => (0.1, 3.0),
            BagStyle::Pouch | BagStyle::BeltPouch => (0.5, 8.0),
            BagStyle::Satchel => (3.0, 15.0),
            BagStyle::Backpack => (10.0, 40.0),
            BagStyle::Sack => (20.0, 80.0),
        };
        assert!(space.litres > min && space.litres < max, "seed {}: {:?} holds {} litres", s.seed, s.bag.style, space.litres);
    }
}
