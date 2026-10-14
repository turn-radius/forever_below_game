//! Chest geometry: deterministic, drawable, complete, standing on the floor, lids where the
//! state says, and holding what its container space says.

use armory::families::Generator;
use armory::families::chests::{ChestState, ChestStyle, Chests};
use armory::material::MaterialKind;
use armory::sections::device::Device;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (ChestState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Chests::roll(seed);
        let mesh = Chests::build(&state);
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
        let state = Chests::roll(seed);
        assert_eq!(Chests::build(&state), Chests::build(&state), "seed {seed}");
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
        let c = &s.chest;
        let boxed = c.style.is_lidded_box();
        let expected: Vec<(&str, bool)> = match c.style {
            ChestStyle::Chest | ChestStyle::Strongbox | ChestStyle::Coffer => vec![
                ("body", true),
                ("interior", true),
                ("lid", true),
                ("hinges", true),
                ("bands", c.bands > 0 || c.style == ChestStyle::Strongbox),
                ("studs", c.studs),
                ("handles", c.handles),
                ("lock", c.lock),
                ("keyhole", c.lock),
                ("feet", c.style == ChestStyle::Coffer),
                ("gem", c.style == ChestStyle::Coffer),
                ("treasure", Chests::is_open(&s) && c.treasure),
                ("treasure_gem", Chests::is_open(&s) && c.treasure),
            ],
            ChestStyle::Barrel => vec![("body", true), ("head", true), ("bands", true), ("bung", true), ("device", c.device != Device::None)],
            ChestStyle::Crate => vec![
                ("interior", true),
                ("boards", true),
                ("slats", true),
                ("bands", c.bands > 0),
                ("handles", c.handles),
                ("device", c.device != Device::None),
            ],
            ChestStyle::Amphora => vec![("body", true), ("handles", c.handles), ("lid", c.lock)],
        };
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {} ({:?}, boxed {boxed}): part {name} present should be {on}", s.seed, c.style);
        }
    }
}

#[test]
fn paint_and_glaze_carry_their_colour() {
    for (s, mesh) in built() {
        if let Some(device) = part(&mesh, "device") {
            assert_eq!(device.material, Some(MaterialKind::Paint), "seed {}", s.seed);
            assert!(device.tint.is_some() && !device.outline, "seed {}", s.seed);
        }
        if s.chest.style == ChestStyle::Amphora {
            assert_eq!(part(&mesh, "body").unwrap().tint, Some(s.chest.color), "seed {}", s.seed);
        }
    }
}

#[test]
fn chests_stand_on_the_floor_at_a_sensible_size() {
    for (s, mesh) in built() {
        let (lo, hi) = bounds(&mesh.parts);
        assert!(lo[1] > -0.03 && lo[1] < 0.03, "seed {}: lowest point y = {}", s.seed, lo[1]);
        let span = (hi[0] - lo[0]).max(hi[2] - lo[2]);
        let tall = hi[1] - lo[1];
        assert!(span > 1.5 && span < 10.0, "seed {}: {:?} spans {span}", s.seed, s.chest.style);
        assert!(tall > 1.2 && tall < 10.0, "seed {}: {:?} is {tall} tall", s.seed, s.chest.style);
    }
}

#[test]
fn lids_sit_shut_or_stand_open() {
    for (s, mesh) in built().filter(|(s, _)| s.chest.style.is_lidded_box()) {
        let top = bounds(part(&mesh, "body")).1[1];
        let (lid_lo, lid_hi) = bounds(part(&mesh, "lid"));
        if Chests::is_open(&s) {
            let body = bounds(part(&mesh, "body"));
            assert!(lid_lo[2] < body.0[2] + 0.2 * (body.1[2] - body.0[2]), "seed {}: an open lid should swing back", s.seed);
        } else {
            assert!(lid_lo[1] > top - 1e-3, "seed {}: a shut lid sinks into the box ({} < {top})", s.seed, lid_lo[1]);
            assert!(lid_hi[1] < top + 2.0, "seed {}", s.seed);
        }
    }
}

#[test]
fn treasure_stays_in_its_box() {
    for (s, mesh) in built().filter(|(s, _)| Chests::is_open(s) && s.chest.treasure) {
        let space = Chests::container(&s);
        let (lo, hi) = bounds(mesh.parts.iter().filter(|p| p.name.starts_with("treasure")));
        for k in [0, 2] {
            assert!(lo[k] > space.interior_min[k] - 0.15 && hi[k] < space.interior_max[k] + 0.15, "seed {}: hoard spills along axis {k}", s.seed);
        }
        assert!(lo[1] >= space.interior_min[1] - 0.05, "seed {}", s.seed);
    }
}

#[test]
fn the_container_space_sits_inside_the_body() {
    for (s, mesh) in built() {
        let space = Chests::container(&s);
        let shell = bounds(mesh.parts.iter().filter(|p| matches!(p.name, "body" | "boards")));
        for k in 0..3 {
            assert!(space.interior_min[k] >= shell.0[k] - 1e-3 && space.interior_max[k] <= shell.1[k] + 1e-3, "seed {}: interior leaves the body on axis {k}", s.seed);
            assert!(space.interior_min[k] < space.interior_max[k], "seed {}", s.seed);
        }
        assert!((space.opening.normal[1] - 1.0).abs() < 1e-6, "seed {}: mouth not facing up", s.seed);
        let (min, max) = match s.chest.style {
            ChestStyle::Chest => (15.0, 200.0),
            ChestStyle::Strongbox => (4.0, 50.0),
            ChestStyle::Coffer => (0.8, 12.0),
            ChestStyle::Barrel => (30.0, 300.0),
            ChestStyle::Crate => (25.0, 200.0),
            ChestStyle::Amphora => (5.0, 80.0),
        };
        assert!(space.litres > min && space.litres < max, "seed {}: {:?} holds {} litres", s.seed, s.chest.style, space.litres);
    }
}
