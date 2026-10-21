//! Crossbow geometry: deterministic, drawable, complete, and laid out where the state says.

use armory::families::Generator;
use armory::families::crossbow::{Crossbow, CrossbowState};
use armory::sections::ranged::NockStyle;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (CrossbowState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Crossbow::roll(seed);
        let mesh = Crossbow::build(&state);
        (state, mesh)
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

fn bounds(points: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    points.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Crossbow::roll(seed);
        assert_eq!(Crossbow::build(&state), Crossbow::build(&state), "seed {seed}");
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
        let st = &s.stock;
        let expected = [
            ("stock", true),
            ("prod", true),
            ("nocks", s.limbs.nocks != NockStyle::None),
            ("string", true),
            ("serving", s.string.serving),
            ("nut", true),
            ("inlay", st.inlay),
            ("lashings", st.lashings > 0),
            ("stirrup", st.stirrup),
            ("trigger", st.trigger),
            ("arrow_shaft", s.arrow.on),
            ("arrow_head", s.arrow.on),
            ("fletching", s.arrow.on),
            ("arrow_nock", false),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn stock_runs_from_the_butt_with_the_prod_across_its_front() {
    for (s, mesh) in built() {
        let st = &s.stock;
        let (lo, hi) = bounds(&part(&mesh, "stock").unwrap().positions);
        assert!(lo[1].abs() < 1e-3 && (hi[1] - st.length).abs() < 1e-3, "seed {}: stock spans {}..{}", s.seed, lo[1], hi[1]);
        assert!(hi[2] <= st.deck() + 1e-3, "seed {}: stock above its deck", s.seed);
        let (plo, phi) = bounds(&part(&mesh, "prod").unwrap().positions);
        let span = phi[0] - plo[0];
        assert!(span > s.limbs.length * 0.6 && span < s.limbs.length * 1.05, "seed {}: prod spans {span} for {}", s.seed, s.limbs.length);
        assert!(phi[1] < st.length + 0.05 && plo[1] > st.length * st.nut, "seed {}: prod outside the front of the stock", s.seed);
        assert!(phi[2] < st.deck(), "seed {}: prod stands above the deck", s.seed);
    }
}

#[test]
fn string_rides_over_the_deck_and_spans_to_the_nut() {
    for (s, mesh) in built() {
        let st = &s.stock;
        let string = part(&mesh, "string").unwrap();
        let (lo, _) = bounds(&string.positions);
        let nut_y = st.length * st.nut;
        if s.spanned {
            assert!(lo[1] < nut_y + st.depth, "seed {}: spanned string stops at {}, nut at {nut_y}", s.seed, lo[1]);
        } else {
            assert!(lo[1] > nut_y, "seed {}: string at rest reaches back to {}", s.seed, lo[1]);
        }
        // Where the string crosses the stock it is on the deck, not through the wood.
        for p in string.positions.iter().filter(|p| p[0].abs() < st.width_at(p[1] / st.length) * 0.3) {
            assert!(p[2] > st.deck() - 0.001, "seed {}: string inside the stock at {p:?}", s.seed);
        }
    }
}

#[test]
fn bolt_lies_on_the_deck_pointing_forward() {
    for (s, mesh) in built().filter(|(s, _)| s.arrow.on) {
        let (lo, hi) = bounds(&part(&mesh, "arrow_shaft").unwrap().positions);
        assert!(lo[2] > s.stock.deck() - 0.001, "seed {}: bolt sunk in the deck", s.seed);
        let (_, head_hi) = bounds(&part(&mesh, "arrow_head").unwrap().positions);
        assert!(head_hi[1] > hi[1], "seed {}: head behind the shaft", s.seed);
    }
}
