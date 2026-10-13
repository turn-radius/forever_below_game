//! Amulet geometry: deterministic, drawable, complete, and hung where it should be.

use armory::families::Generator;
use armory::families::amulet::{Amulet, AmuletState, NecklaceStyle, PendantCentre};
use armory::sections::emblem::PlaqueShape;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (AmuletState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Amulet::roll(seed);
        let mesh = Amulet::build(&state);
        (state, mesh)
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

fn bounds(p: &Part) -> ([f32; 3], [f32; 3]) {
    p.positions.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Amulet::roll(seed);
        assert_eq!(Amulet::build(&state), Amulet::build(&state), "seed {seed}");
    }
}

#[test]
fn every_part_is_drawable() {
    for (state, mesh) in built() {
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
        let p = &s.pendant;
        let n = &s.necklace;
        let expected = [
            ("pendant", true),
            ("bail", true),
            ("rim", p.rim > 0.0 && !matches!(p.shape, PlaqueShape::Crescent | PlaqueShape::Ankh)),
            ("setting", p.centre == PendantCentre::Stone),
            ("stone", p.centre == PendantCentre::Stone),
            ("device", p.centre == PendantCentre::Device),
            ("drop_rings", p.drops > 0),
            ("drops", p.drops > 0),
            ("chain", n.style == NecklaceStyle::Cable),
            ("cord", n.style == NecklaceStyle::Cord),
            ("thong", n.style == NecklaceStyle::Thong),
            ("string", n.style == NecklaceStyle::Beads),
            ("beads", n.style == NecklaceStyle::Beads),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn the_pendant_hangs_below_the_bail_and_the_necklace_rises_above() {
    for (s, mesh) in built() {
        let (plo, phi) = bounds(part(&mesh, "pendant").unwrap());
        let h = s.pendant.size;
        assert!(phi[1] <= 0.01 && phi[1] > -h * 0.2, "seed {}: pendant top at {}", s.seed, phi[1]);
        assert!(plo[1] > -h * 1.1, "seed {}: pendant reaches {}", s.seed, plo[1]);
        assert!(phi[2] - plo[2] < s.pendant.thickness * 1.01, "seed {}: pendant thicker than its slider", s.seed);
        let necklace = ["chain", "cord", "thong", "string"].iter().find_map(|n| part(&mesh, n)).unwrap();
        let (nlo, nhi) = bounds(necklace);
        assert!(nlo[1] > -0.05 && nlo[1] < h * 0.3, "seed {}: necklace bottom at {}", s.seed, nlo[1]);
        assert!((nhi[1] - s.necklace.drop).abs() < 0.25, "seed {}: necklace top {} for drop {}", s.seed, nhi[1], s.necklace.drop);
        assert!(nlo[2] < -s.necklace.drop * 0.3, "seed {}: necklace does not loop back", s.seed);
    }
}

#[test]
fn small_parts_skip_the_outline() {
    for (s, mesh) in built() {
        for p in &mesh.parts {
            let fine = matches!(p.name, "chain" | "cord" | "thong" | "string" | "device" | "drop_rings" | "drops");
            assert_eq!(p.outline, !fine, "seed {}: {} outline", s.seed, p.name);
        }
    }
}
