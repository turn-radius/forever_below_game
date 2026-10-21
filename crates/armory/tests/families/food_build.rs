//! Food geometry: deterministic, drawable, complete, and sitting where food should sit.

use armory::families::Generator;
use armory::families::food::{Food, FoodState, FoodStyle, Portion};
use armory::{ItemMesh, MaterialKind, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (FoodState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Food::roll(seed);
        let mesh = Food::build(&state);
        (state, mesh)
    })
}

/// One of every style in every portion it comes in.
fn every_kind() -> impl Iterator<Item = (FoodState, ItemMesh)> {
    FoodStyle::ALL.into_iter().flat_map(|style| {
        style.portions().iter().map(move |&portion| {
            let mut s = Food::roll_as(9, style);
            s.provision.portion = portion;
            let mesh = Food::build(&s);
            (s, mesh)
        })
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

fn bounds(mesh: &ItemMesh) -> ([f32; 3], [f32; 3]) {
    mesh.parts.iter().flat_map(|p| &p.positions).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Food::roll(seed);
        assert_eq!(Food::build(&state), Food::build(&state), "seed {seed}");
    }
}

#[test]
fn every_part_is_drawable() {
    for (state, mesh) in built().chain(every_kind()) {
        assert!(!mesh.parts.is_empty(), "seed {}: no parts", state.seed);
        for p in &mesh.parts {
            p.validate().unwrap_or_else(|e| panic!("seed {} {:?}: {e}", state.seed, state.provision.style));
            assert!(!p.indices.is_empty(), "seed {}: {} has no triangles", state.seed, p.name);
            assert!(p.material.is_some(), "seed {}: {} has no material", state.seed, p.name);
        }
    }
}

#[test]
fn every_kind_has_its_parts() {
    use FoodStyle as F;
    for (s, mesh) in every_kind() {
        let p = &s.provision;
        let main = match p.style {
            F::Loaf | F::Boule | F::Pie => "crust",
            F::Cheese => "rind",
            F::Apple | F::Pear => "skin",
            F::Haunch => "meat",
            F::Fish => "body",
            F::Sausages => "casing",
            F::Mushroom => "cap",
            F::Tankard | F::Flask if p.vessel != MaterialKind::Glass => "vessel",
            F::Bottle | F::Potion | F::Flask => "glass",
            F::Waterskin => "body",
            F::Tankard => "vessel",
        };
        let label = format!("{:?} {:?}", p.style, p.portion);
        assert!(part(&mesh, main).is_some(), "{label}: no {main}");
        // Cut and bitten food shows its inside.
        let inside = match p.style {
            F::Loaf | F::Boule => "crumb",
            F::Cheese => "paste",
            F::Apple | F::Pear | F::Haunch => "flesh",
            F::Pie => "filling",
            _ => "",
        };
        if p.portion != Portion::Whole && !inside.is_empty() {
            assert!(part(&mesh, inside).is_some(), "{label}: no {inside}");
        }
        if p.portion == Portion::Whole && matches!(p.style, F::Cheese | F::Pie | F::Boule | F::Apple | F::Pear) {
            assert!(part(&mesh, inside).is_none(), "{label}: whole but shows {inside}");
        }
        assert_eq!(part(&mesh, "dish").is_some(), p.style == F::Pie && p.portion != Portion::Piece, "{label}: dish");
        assert_eq!(part(&mesh, "seal").is_some(), p.seal.is_some(), "{label}: seal");
        let holds_drink = matches!(p.style, F::Tankard | F::Bottle | F::Potion) && p.is_full();
        assert_eq!(part(&mesh, "drink").is_some(), holds_drink, "{label}: drink");
        if p.style == F::Cheese && p.portion != Portion::Whole {
            assert!(part(&mesh, "holes").is_some(), "{label}: no eyes in the paste");
        }
    }
}

#[test]
fn food_is_tinted_food_and_glass_is_tinted() {
    for (s, mesh) in built() {
        for p in &mesh.parts {
            if p.material == Some(MaterialKind::Food) {
                assert!(p.tint.is_some(), "seed {}: {} food without a colour", s.seed, p.name);
            }
        }
        if let Some(glass) = part(&mesh, "glass") {
            assert_eq!(glass.tint, Some(s.provision.outer), "seed {}", s.seed);
        }
        if let Some(drink) = part(&mesh, "drink") {
            assert_eq!(drink.tint, Some(s.provision.liquid), "seed {}", s.seed);
            assert!(!drink.outline, "seed {}: drink outlined", s.seed);
        }
    }
}

#[test]
fn glow_reaches_the_parts() {
    for (s, mesh) in built().filter(|(s, _)| s.provision.glow > 0.0) {
        let lit = match s.provision.style {
            FoodStyle::Potion if s.provision.is_full() => "drink",
            FoodStyle::Mushroom => "cap",
            _ => continue,
        };
        assert_eq!(part(&mesh, lit).unwrap().glow, s.provision.glow, "seed {}", s.seed);
    }
}

#[test]
fn sits_on_the_table_at_a_sensible_size() {
    for (s, mesh) in built().chain(every_kind()) {
        let (lo, hi) = bounds(&mesh);
        let label = format!("seed {} {:?} {:?}", s.seed, s.provision.style, s.provision.portion);
        assert!(lo[1].abs() < 1e-4, "{label}: rests at y = {}", lo[1]);
        for k in [0, 2] {
            assert!((lo[k] + hi[k]).abs() < 1e-3, "{label}: not centred on axis {k}");
        }
        let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f32, f32::max);
        // A wedge of cheese is a few centimetres; a long loaf or a string of sausages under 70.
        assert!(size > 0.3 && size < 7.0, "{label}: {size} units across");
    }
}

#[test]
fn glowcaps_give_light_from_their_caps() {
    use armory::families::food::{FoodStyle, GLOWCAP};
    let mut lit = 0;
    for seed in 1..=64 {
        let s = Food::roll_as(seed, FoodStyle::Mushroom);
        match Food::light(&s) {
            Some(light) => {
                lit += 1;
                assert!(s.provision.glow >= GLOWCAP, "seed {seed}");
                let mesh = Food::build(&s);
                let top = mesh.parts.iter().flat_map(|p| p.positions.iter()).fold(f32::MIN, |m, p| m.max(p[1]));
                assert!(light.position[1] > 0.0 && light.position[1] < top, "seed {seed}");
                assert!(light.radius > 0.0 && light.flicker == 0.0, "seed {seed}");
            }
            None => assert!(s.provision.glow < GLOWCAP, "seed {seed}"),
        }
    }
    assert!(lit > 0);
    assert!(Food::light(&Food::roll_as(1, FoodStyle::Loaf)).is_none());
}
