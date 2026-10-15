//! Gloves and gauntlets geometry: deterministic, drawable, complete, and hand sized.

use armory::fabric::PatternKind;
use armory::families::Generator;
use armory::families::hands::{Hands, HandsState};
use armory::sections::hand::HandStyle;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (HandsState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Hands::roll(seed);
        let mesh = Hands::build(&state);
        (state, mesh)
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

fn bounds(parts: &[Part]) -> ([f32; 3], [f32; 3]) {
    parts.iter().flat_map(|p| &p.positions).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Hands::roll(seed);
        assert_eq!(Hands::build(&state), Hands::build(&state), "seed {seed}");
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
        let style = s.hand.style;
        let bracer = style == HandStyle::Bracer;
        let expected = [
            ("cuff", true),
            ("hand", !bracer),
            ("thumb", !bracer),
            ("fingers", matches!(style, HandStyle::Glove | HandStyle::Gauntlet)),
            ("plates", style.is_plated()),
            ("gadlings", s.hand.gadlings),
            ("straps", bracer),
            ("buckles", bracer),
            ("trim", s.hem.on && s.hem.hemline),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
        if style == HandStyle::Mitten {
            assert_eq!(part(&mesh, "hand").unwrap().pattern.map(|p| p.kind), Some(PatternKind::Mail), "seed {}", s.seed);
        }
    }
}

#[test]
fn gloves_are_hand_sized_and_stand_on_their_cuff() {
    for (s, mesh) in built() {
        let (lo, hi) = bounds(&mesh.parts);
        assert!(lo[1] > -0.1 && lo[1] < 0.1, "seed {}: bottom at {}", s.seed, lo[1]);
        let height = hi[1] - lo[1];
        let reach = if s.hand.style == HandStyle::Bracer { 0.1 } else { 1.4 * s.hand.size };
        assert!(height > s.hand.cuff * s.hand.size + reach, "seed {}: only {height} tall", s.seed);
        assert!(height < 5.5, "seed {}: {height} tall", s.seed);
        assert!(hi[0] - lo[0] < 2.6, "seed {}: {} wide", s.seed, hi[0] - lo[0]);
    }
}
