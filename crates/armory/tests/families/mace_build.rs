//! Mace geometry: deterministic, drawable, complete, and laid out where the state says.

use armory::families::Generator;
use armory::families::mace::{Mace, MaceState};
use armory::sections::hafted::PommelGem;
use armory::sections::head::{FinialStyle, HeadStyle};
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (MaceState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Mace::roll(seed);
        let mesh = Mace::build(&state);
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
        let state = Mace::roll(seed);
        assert_eq!(Mace::build(&state), Mace::build(&state), "seed {seed}");
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
            ("shaft", true),
            ("grip", s.grip.on),
            ("wrap", s.wrap.on),
            ("pommel", s.pommel.on),
            ("pommel_stones", s.pommel.on && s.pommel.gem != PommelGem::None),
            ("collar", s.collar.on),
            ("head", s.head.style != HeadStyle::None),
            ("flanges", s.flanges.on),
            ("spikes", s.spikes.on),
            ("prongs", s.prongs.on),
            ("finial", s.finial.style != FinialStyle::None),
            ("gem", s.gem.on),
            ("bezel", s.gem.on && !s.prongs.on),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn gem_parts_carry_the_stone() {
    for (s, mesh) in built().filter(|(s, _)| s.gem.on) {
        let gem = part(&mesh, "gem").unwrap();
        assert_eq!(gem.tint, Some(s.gem.stone.rgb()), "seed {}", s.seed);
        assert_eq!(gem.glow, s.gem.glow, "seed {}", s.seed);
    }
}

#[test]
fn head_sits_on_top_of_the_shaft() {
    for (s, mesh) in built() {
        let top = s.shaft.length;
        let (lo, hi) = bounds(part(&mesh, "head").unwrap());
        assert!(lo[1] > top - 0.1, "seed {}: head bottom {} below shaft top {top}", s.seed, lo[1]);
        assert!(hi[1] > top + s.head.length * 0.8, "seed {}: head top {} too low", s.seed, hi[1]);
        let shaft_hi = bounds(part(&mesh, "shaft").unwrap()).1[1];
        assert!(shaft_hi < top + 0.05, "seed {}: shaft reaches {shaft_hi} past its length {top}", s.seed);
    }
}

#[test]
fn spikes_and_flanges_stay_near_the_head() {
    for (s, mesh) in built() {
        let (lo, hi) = bounds(part(&mesh, "head").unwrap());
        let near = |name: &str, out: f32, up: f32| {
            if let Some(p) = part(&mesh, name) {
                let (plo, phi) = bounds(p);
                for k in [0, 2] {
                    assert!(plo[k] > lo[k] - out && phi[k] < hi[k] + out, "seed {}: {name} leaves the head sideways", s.seed);
                }
                assert!(plo[1] > lo[1] - up && phi[1] < hi[1] + up, "seed {}: {name} leaves the head vertically", s.seed);
            }
        };
        near("spikes", s.spikes.length + s.spikes.base, s.spikes.length + s.spikes.base);
        near("flanges", s.flanges.depth + s.flanges.thickness, s.head.length * 0.3);
    }
}

#[test]
fn nothing_hangs_below_the_pommel() {
    for (s, mesh) in built() {
        let floor = if s.pommel.on { -s.pommel.length } else { 0.0 } - 0.02;
        for p in &mesh.parts {
            let lo = bounds(p).0[1];
            assert!(lo >= floor, "seed {}: {} reaches y = {lo}, below {floor}", s.seed, p.name);
        }
    }
}
