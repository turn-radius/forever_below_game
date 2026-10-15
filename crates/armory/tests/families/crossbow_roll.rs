//! Crossbow "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::crossbow::{Crossbow, CrossbowArchetype, CrossbowState, LIMITS};
use armory::material::MaterialKind;
use armory::sections::ranged::{ArrowHead, FletchCut, LimbShape, NockStyle};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<CrossbowState> {
    range.map(Crossbow::roll).collect()
}

#[test]
fn same_seed_same_crossbow() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Crossbow::roll(seed), Crossbow::roll(seed));
        assert_eq!(Crossbow::roll(seed).seed, seed);
    }
    assert_ne!(Crossbow::roll(1), Crossbow::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Crossbow::roll(1207);
    let back: CrossbowState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    let s = Crossbow::roll(42);
    assert!(CrossbowArchetype::WEIGHTS.iter().any(|(a, _)| Crossbow::roll_as(42, *a) == s));
}

#[test]
fn every_slider_within_range() {
    for s in rolls(1..=1024) {
        let bad = s.out_of_range(&LIMITS);
        assert!(bad.is_empty(), "seed {}: {bad:?}", s.seed);
    }
}

#[test]
fn rolls_are_coherent() {
    for s in rolls(1..=1024) {
        let seed = s.seed;
        let (st, l) = (&s.stock, &s.limbs);
        assert_eq!(l.siyah > 0.0, l.shape == LimbShape::Composite, "seed {seed}: tip levers only on composite prods");
        if l.material == MaterialKind::Iron {
            assert_eq!(l.nocks, NockStyle::Cap, "seed {seed}: a steel prod has capped tips");
            assert_ne!(l.shape, LimbShape::Composite, "seed {seed}: a steel prod is not composite");
        }
        if st.stirrup {
            assert!(st.length >= 5.5, "seed {seed}: stirrup on a {} stock", st.length);
        }
        // The nut sits behind the prod with room for the string at rest in front of it.
        let prod_y = st.length - st.prod_set;
        assert!(prod_y - st.length * st.nut > l.brace, "seed {seed}: nut under the string at rest");
        // A bolt reaches past the nut but not far beyond the front.
        if s.arrow.on {
            let tip = st.length * st.nut + s.arrow.length + s.arrow.head_length;
            assert!(tip > prod_y - 0.5 && tip < st.length + 1.5, "seed {seed}: bolt tip at {tip}, stock {}", st.length);
        }
        assert!(st.butt_width >= st.mid_width, "seed {seed}: butt narrower than the waist");
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&CrossbowState) -> bool| all.iter().any(f);
    for shape in [LimbShape::Longbow, LimbShape::Recurve, LimbShape::Composite] {
        assert!(any(&|s| s.limbs.shape == shape), "no {shape:?} prod");
    }
    for nocks in NockStyle::ALL {
        assert!(any(&|s| s.limbs.nocks == nocks), "no {nocks:?} nocks");
    }
    for head in ArrowHead::ALL {
        assert!(any(&|s| s.arrow.on && s.arrow.head == head), "no {head:?} bolt");
    }
    for cut in FletchCut::ALL {
        assert!(any(&|s| s.arrow.on && s.arrow.fletch_cut == cut), "no {cut:?} fletching");
    }
    for fletches in 2..=3 {
        assert!(any(&|s| s.arrow.on && s.arrow.fletches == fletches), "no {fletches}-vane bolt");
    }
    for (name, f) in [
        ("spanned", (|s: &CrossbowState| s.spanned) as fn(&CrossbowState) -> bool),
        ("stirrup", |s| s.stock.stirrup),
        ("inlay", |s| s.stock.inlay),
        ("trigger", |s| s.stock.trigger),
        ("bolt", |s| s.arrow.on),
        ("serving", |s| s.string.serving),
        ("lashings", |s| s.stock.lashings > 0),
    ] {
        assert!(any(&f) && any(&|s| !f(s)), "{name} always or never");
    }
    assert!(any(&|s| s.etch.on), "no etching");
    for name in ["Crossbow", "Steel Crossbow", "Arbalest", "Latchbow"] {
        assert!(all.iter().any(|s| Crossbow::noun(s) == name), "no {name}");
    }
}

#[test]
fn nouns() {
    let mut s = CrossbowState::default();
    let mut noun = |length, material| {
        s.stock.length = length;
        s.limbs.material = material;
        Crossbow::noun(&s)
    };
    assert_eq!(noun(4.0, MaterialKind::Iron), "Latchbow");
    assert_eq!(noun(4.0, MaterialKind::Wood), "Latchbow");
    assert_eq!(noun(8.5, MaterialKind::Iron), "Arbalest");
    assert_eq!(noun(6.0, MaterialKind::Iron), "Steel Crossbow");
    assert_eq!(noun(8.5, MaterialKind::Wood), "Crossbow");
}
