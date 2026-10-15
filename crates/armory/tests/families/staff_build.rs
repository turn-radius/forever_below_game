//! Staff geometry: deterministic, drawable, complete, and laid out where the state says.

use armory::families::Generator;
use armory::families::staff::{Staff, StaffState};
use armory::sections::hafted::PommelGem;
use armory::sections::head::{FinialStyle, HeadStyle};
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (StaffState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Staff::roll(seed);
        let mesh = Staff::build(&state);
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

fn item_bounds(mesh: &ItemMesh) -> ([f32; 3], [f32; 3]) {
    mesh.parts.iter().map(bounds).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), (a, b)| {
        ([lo[0].min(a[0]), lo[1].min(a[1]), lo[2].min(a[2])], [hi[0].max(b[0]), hi[1].max(b[1]), hi[2].max(b[2])])
    })
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Staff::roll(seed);
        assert_eq!(Staff::build(&state), Staff::build(&state), "seed {seed}");
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
        let in_cup = s.head.style == HeadStyle::Cup && s.finial.style == FinialStyle::None;
        let expected = [
            ("shaft", true),
            ("bands", s.shaft.bands > 0 && part(&mesh, "bands").is_some()),
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
            ("bezel", s.gem.on && !s.prongs.on && !s.gem.bob && !in_cup),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
        if !s.grip.on && !s.wrap.on && s.shaft.bands > 0 {
            assert!(part(&mesh, "bands").is_some(), "seed {}: bands missing on a bare shaft", s.seed);
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
fn cradled_stones_sit_inside_their_prongs() {
    for (s, mesh) in built().filter(|(s, _)| s.prongs.on) {
        let (glo, ghi) = bounds(part(&mesh, "gem").unwrap());
        let (plo, phi) = bounds(part(&mesh, "prongs").unwrap());
        let girdle_y = (glo[1] + ghi[1]) * 0.5;
        assert!(girdle_y > plo[1] && girdle_y < phi[1] + s.gem.size, "seed {}: stone at {girdle_y} outside prongs {}..{}", s.seed, plo[1], phi[1]);
        for k in [0, 2] {
            let centre = (glo[k] + ghi[k]) * 0.5;
            assert!(centre > plo[k] && centre < phi[k], "seed {}: stone off the cradle's axis", s.seed);
        }
    }
}

#[test]
fn the_head_crowns_the_shaft() {
    for (s, mesh) in built() {
        let top = s.shaft.length;
        let shaft_hi = bounds(part(&mesh, "shaft").unwrap()).1[1];
        assert!(shaft_hi < top + s.shaft.radius * s.shaft.taper + 0.05, "seed {}: shaft reaches {shaft_hi} past its length {top}", s.seed);
        if let Some(head) = part(&mesh, "head") {
            let (lo, hi) = bounds(head);
            assert!(lo[1] > top - 0.6, "seed {}: head bottom {} far below shaft top {top}", s.seed, lo[1]);
            assert!(hi[1] > top + s.head.length * 0.8, "seed {}: head top {} too low", s.seed, hi[1]);
        }
    }
}

#[test]
fn staves_are_long_and_slender() {
    for (s, mesh) in built() {
        let (lo, hi) = item_bounds(&mesh);
        let floor = if s.pommel.on { -s.pommel.length } else { 0.0 } - 0.02 - s.shaft.radius * s.shaft.gnarl * 0.5;
        assert!(lo[1] >= floor, "seed {}: reaches y = {}, below {floor}", s.seed, lo[1]);
        let height = hi[1] - lo[1];
        assert!(height > s.shaft.length && height < s.shaft.length + 5.0, "seed {}: {height} tall on a {} shaft", s.seed, s.shaft.length);
        for k in [0, 2] {
            assert!(hi[k] - lo[k] < 3.0, "seed {}: {} wide", s.seed, hi[k] - lo[k]);
        }
    }
}
