//! Sundry geometry: deterministic, drawable, complete, resting on the ground, and a lit
//! candle's light in its flame.

use armory::families::Generator;
use armory::families::sundry::{Sundry, SundryKind, SundryState};
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (SundryState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Sundry::roll(seed);
        let mesh = Sundry::build(&state);
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
        let state = Sundry::roll(seed);
        assert_eq!(Sundry::build(&state), Sundry::build(&state), "seed {seed}");
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
        let a = &s.article;
        let k = a.kind;
        let expected = [
            ("candle", k == SundryKind::Candle),
            ("wick", k == SundryKind::Candle),
            ("holder", a.holder),
            ("flame", a.lit),
            ("rope", k == SundryKind::Rope),
            ("ties", k == SundryKind::Rope),
            ("picks", k == SundryKind::Lockpicks),
            ("handles", k == SundryKind::Lockpicks),
            ("keys", k == SundryKind::Keys),
            ("box", k == SundryKind::Tinderbox),
            ("steel", k == SundryKind::Tinderbox),
            ("flint", k == SundryKind::Tinderbox),
            ("bedroll", k == SundryKind::Bedroll),
            ("straps", k == SundryKind::Bedroll),
            ("buckles", k == SundryKind::Bedroll),
            ("strip", k == SundryKind::Bandages),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn rests_on_the_ground_at_a_sensible_size() {
    for (s, mesh) in built() {
        let (lo, hi) = bounds(&mesh.parts);
        assert!(lo[1].abs() < 0.03, "seed {}: {:?} bottom at {}", s.seed, s.article.kind, lo[1]);
        let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]);
        let most = if s.article.kind == SundryKind::Bedroll { 6.5 } else { 2.5 };
        assert!(span > 0.3 && span < most, "seed {}: {:?} spans {span}", s.seed, s.article.kind);
    }
}

#[test]
fn a_lit_candle_gives_light_from_its_flame() {
    for (s, mesh) in built() {
        match Sundry::light(&s) {
            Some(light) => {
                let (lo, hi) = bounds(part(&mesh, "flame"));
                assert!((0..3).all(|k| light.position[k] >= lo[k] && light.position[k] <= hi[k]), "seed {}", s.seed);
                assert!((light.radius - 5.0 * 3.048).abs() < 1e-3);
            }
            None => assert!(!s.article.lit, "seed {}", s.seed),
        }
    }
}

#[test]
fn a_laid_bedroll_is_the_same_bedroll_unrolled_on_the_floor() {
    use armory::families::sundry::LAID_BEDROLL_HALF;
    let mut seen = 0;
    for (state, rolled) in built().filter(|(s, _)| s.article.kind == SundryKind::Bedroll) {
        seen += 1;
        let mut laid_state = state.clone();
        laid_state.article.laid = true;
        let laid = Sundry::build(&laid_state);
        assert_eq!(laid, Sundry::build(&laid_state), "seed {}: deterministic", state.seed);
        assert_ne!(laid, rolled, "seed {}: drawn differently", state.seed);
        for p in &laid.parts {
            p.validate().unwrap_or_else(|e| panic!("seed {}: {e}", state.seed));
        }
        let (lo, hi) = bounds(&laid.parts);
        assert!(lo[1].abs() < 1e-3, "seed {}: rests on the floor at {}", state.seed, lo[1]);
        assert!(hi[1] < 1.3, "seed {}: {} high", state.seed, hi[1]);
        let parts: Vec<_> = laid.parts.iter().map(|p| (p.name.clone(), bounds([p]))).collect();
        assert!(hi[0].max(-lo[0]) <= LAID_BEDROLL_HALF.0 && hi[2].max(-lo[2]) <= LAID_BEDROLL_HALF.1, "seed {}: {lo:?} {hi:?} {parts:?}", state.seed);
        assert!(hi[0] - lo[0] > 12.0, "seed {}: about a person long", state.seed);
        // Laying it out does not change what it is: the rolled one comes back from the same state.
        laid_state.article.laid = false;
        assert_eq!(Sundry::build(&laid_state), rolled);
    }
    assert!(seen > 5, "{seen} bedrolls in the sweep");
}
