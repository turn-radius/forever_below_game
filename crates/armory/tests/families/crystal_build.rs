//! Crystals: deterministic, drawable, within their sliders, faceted, and every habit and growth
//! rolls.

use armory::families::Generator;
use armory::families::crystal::{self, CrystalState, Crystals, Growth, Habit};
use armory::gem::Gemstone;
use armory::{Family, Item, ItemMesh};

const GROWTHS: [Growth; 3] = [Growth::Single, Growth::Cluster, Growth::Geode];

fn drawable(mesh: &ItemMesh, what: &str) {
    assert!(!mesh.parts.is_empty(), "{what}: no parts");
    for p in &mesh.parts {
        p.validate().unwrap_or_else(|e| panic!("{what}: {e}"));
        assert!(!p.indices.is_empty(), "{what}: {} has no triangles", p.name);
        assert!(p.material.is_some(), "{what}: {} has no material", p.name);
    }
    let (lo, _) = mesh.bounds().unwrap();
    assert!(lo[1].abs() < 1e-3, "{what}: rests {} off the ground", lo[1]);
}

fn size(mesh: &ItemMesh) -> [f32; 3] {
    let (lo, hi) = mesh.bounds().unwrap();
    [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]
}

fn every() -> impl Iterator<Item = (CrystalState, ItemMesh)> {
    Habit::ALL.into_iter().flat_map(|habit| {
        GROWTHS.into_iter().flat_map(move |growth| {
            (1..=3).map(move |seed| {
                let s = Crystals::roll_as(seed, habit, growth);
                let mesh = Crystals::build(&s);
                (s, mesh)
            })
        })
    })
}

#[test]
fn same_seed_same_crystal_and_mesh() {
    for seed in [1, 9, 1207] {
        let s = Crystals::roll(seed);
        assert_eq!(s, Crystals::roll(seed));
        assert_eq!(Crystals::build(&s), Crystals::build(&s));
    }
    assert_ne!(Crystals::roll(1), Crystals::roll(2));
}

#[test]
fn every_habit_in_every_growth_rolls_and_draws() {
    for (s, mesh) in every() {
        let what = format!("crystal {} {:?} {:?}", s.seed, s.crystal.habit, s.crystal.growth);
        drawable(&mesh, &what);
        assert!(mesh.parts.iter().any(|p| p.name == "crystal"), "{what}: no crystal part");
    }
}

#[test]
fn the_roll_reaches_every_habit_and_growth() {
    let rolled: Vec<_> = (1..=200).map(|seed| Crystals::roll(seed).crystal).collect();
    for habit in Habit::ALL {
        assert!(rolled.iter().any(|c| c.habit == habit), "{habit:?} never rolls");
    }
    for growth in GROWTHS {
        assert!(rolled.iter().any(|c| c.growth == growth), "{growth:?} never rolls");
    }
    assert!(rolled.iter().any(|c| c.twinned) && rolled.iter().any(|c| c.scepter) && rolled.iter().any(|c| c.double_ended));
    assert!(rolled.iter().any(|c| c.cracks > 0.0) && rolled.iter().any(|c| c.inclusions > 0.0) && rolled.iter().any(|c| c.cloud > 0.0));
}

#[test]
fn crystal_sliders_hold() {
    for seed in 1..=200 {
        let s = Crystals::roll(seed);
        let bad = s.out_of_range(&crystal::LIMITS);
        assert!(bad.is_empty(), "seed {seed}: {bad:?}");
    }
    let mut s = Crystals::roll(3);
    s.crystal.size = 999.0;
    s.crystal.sides = 40;
    s.fit(&crystal::LIMITS);
    assert_eq!((s.crystal.size, s.crystal.sides), (80.0, 8));
}

#[test]
fn a_crystal_carries_its_stones_colour() {
    for stone in Gemstone::ALL {
        let mut s = Crystals::roll_as(5, Habit::Prismatic, Growth::Single);
        s.crystal.stone = stone;
        s.crystal.cloud = 0.0;
        let mesh = Crystals::build(&s);
        let body = mesh.parts.iter().find(|p| p.name == "crystal").unwrap();
        assert_eq!(body.tint, Some(stone.rgb()), "{stone:?}");
        assert!(Crystals::noun(&s).contains(stone.name()));
    }
}

#[test]
fn a_crystals_faces_are_flat_and_face_outward() {
    // Every triangle of a single crystal has the one normal its vertices share, pointing away
    // from the middle of the crystal.
    for habit in Habit::ALL {
        let mut s = Crystals::roll_as(4, habit, Growth::Single);
        s.crystal.twinned = false;
        s.crystal.scepter = false;
        let mesh = Crystals::build(&s);
        let body = mesh.parts.iter().find(|p| p.name == "crystal").unwrap();
        let (lo, hi) = mesh.bounds().unwrap();
        let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        for tri in body.indices.chunks(3) {
            let p: Vec<[f32; 3]> = tri.iter().map(|&i| body.positions[i as usize]).collect();
            let n = body.normals[tri[0] as usize];
            assert!(tri.iter().all(|&i| body.normals[i as usize] == n), "{habit:?}: a facet is not flat");
            let c = [0, 1, 2].map(|k| (p[0][k] + p[1][k] + p[2][k]) / 3.0 - mid[k]);
            assert!(n[0] * c[0] + n[1] * c[1] + n[2] * c[2] > -1e-3, "{habit:?}: a facet faces inward");
        }
    }
}

#[test]
fn habits_differ_in_proportion() {
    let dims = |habit| {
        let mut s = Crystals::roll_as(2, habit, Growth::Single);
        s.crystal.twinned = false;
        s.crystal.scepter = false;
        s.crystal.size = 10.0;
        size(&Crystals::build(&s))
    };
    let column = dims(Habit::Columnar);
    let needle = dims(Habit::Acicular);
    let plate = dims(Habit::Tabular);
    let blade = dims(Habit::Bladed);
    assert!(needle[1] / needle[0].max(needle[2]) > column[1] / column[0].max(column[2]), "needles are slimmer than columns");
    assert!(plate[0].max(plate[2]) > plate[1], "a plate is wider than tall");
    assert!(column[1] > column[0].max(column[2]) * 2.5, "a column is long");
    assert!(blade[0].min(blade[2]) < blade[0].max(blade[2]) * 0.6, "a blade is thin one way");
}

#[test]
fn two_seeds_of_one_habit_look_different() {
    for habit in Habit::ALL {
        let a = Crystals::build(&Crystals::roll_as(11, habit, Growth::Single));
        let b = Crystals::build(&Crystals::roll_as(12, habit, Growth::Single));
        assert_ne!(a, b, "{habit:?}");
    }
}

#[test]
fn a_cluster_rises_from_its_rock_and_a_geode_opens_upward() {
    let cluster = Crystals::build(&Crystals::roll_as(6, Habit::Prismatic, Growth::Cluster));
    assert!(cluster.parts.iter().any(|p| p.name == "matrix"));
    let s = Crystals::roll_as(6, Habit::Prismatic, Growth::Geode);
    let geode = Crystals::build(&s);
    assert!(geode.parts.iter().any(|p| p.name == "rind") && geode.parts.iter().any(|p| p.name == "lining"));
    // A geode is about as wide as it says, and no taller than it is wide.
    let d = size(&geode);
    assert!((d[0].max(d[2]) - s.crystal.size).abs() < s.crystal.size * 0.25, "{d:?} vs {}", s.crystal.size);
    assert!(d[1] < d[0].max(d[2]));
}

#[test]
fn a_single_stands_about_its_size() {
    for habit in Habit::ALL {
        let mut s = Crystals::roll_as(8, habit, Growth::Single);
        s.crystal.twinned = false;
        s.crystal.scepter = false;
        s.crystal.size = 20.0;
        let d = size(&Crystals::build(&s));
        let h = d[0].max(d[1]).max(d[2]);
        assert!(h > 8.0 && h < 30.0, "{habit:?} stands {h}");
    }
}

#[test]
fn the_crystal_family_is_an_armory_item() {
    assert!(Family::Crystal.is_ported());
    let mut item = Item::roll(Family::Crystal, 7).unwrap();
    assert_eq!(item.family(), Family::Crystal);
    assert!(item.noun().contains(' '));
    assert!(item.etching().is_none());
    assert!(item.sliders().iter().any(|f| f.path == "crystal.size"));
    item.set_slider("crystal.size", 3.0).unwrap();
    assert_eq!(item.slider("crystal.size"), Some(3.0));
    assert!(!item.build().parts.is_empty());
    // The habit and the stone are choices in the lab.
    assert_eq!(item.choice_options("crystal.habit").len(), 8);
    assert_eq!(item.choice_options("crystal.stone").len(), Gemstone::ALL.len());
}
