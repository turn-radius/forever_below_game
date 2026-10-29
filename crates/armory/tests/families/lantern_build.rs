//! Lantern geometry: deterministic, drawable, complete, hung from its ring, and lit inside.

use armory::families::Generator;
use armory::families::lantern::{Lantern, LanternFuel, LanternState, LanternStyle};
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (LanternState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Lantern::roll(seed);
        let mesh = Lantern::build(&state);
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
        let state = Lantern::roll(seed);
        assert_eq!(Lantern::build(&state), Lantern::build(&state), "seed {seed}");
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
        let (b, f) = (&s.body, &s.flame);
        let expected = [
            ("ring", true),
            ("chain", s.bail.links > 0),
            ("bail", true),
            ("frame", true),
            ("panes", b.style == LanternStyle::Horn),
            ("holes", b.style == LanternStyle::Pierced),
            ("glass", b.style == LanternStyle::Cage),
            ("candle", f.fuel == LanternFuel::Candle),
            ("socket", f.fuel == LanternFuel::Candle),
            ("font", f.fuel == LanternFuel::Oil),
            ("wick", true),
            ("flame", f.lit),
            ("flame_core", f.lit),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn hangs_below_its_ring() {
    for (s, mesh) in built() {
        let (rlo, rhi) = bounds(part(&mesh, "ring").unwrap());
        assert!(rlo[1] < 0.0 && rhi[1] > 0.0, "seed {}: the hand misses the ring", s.seed);
        let (lo, hi) = bounds(part(&mesh, "frame").unwrap());
        assert!(hi[1] < rlo[1] + 0.05, "seed {}: the body rises into the ring", s.seed);
        assert!(lo[1] > -4.5 && hi[0] - lo[0] < 1.3, "seed {}: frame {lo:?}..{hi:?}", s.seed);
        let (blo, bhi) = bounds(part(&mesh, "bail").unwrap());
        assert!(bhi[1] > hi[1] && blo[1] < hi[1], "seed {}: the bail does not reach the hood", s.seed);
    }
}

#[test]
fn the_flame_and_its_light_are_inside_the_body() {
    for (s, mesh) in built() {
        let (lo, hi) = bounds(part(&mesh, "frame").unwrap());
        match Lantern::light(&s) {
            Some(light) => {
                let (flo, fhi) = bounds(part(&mesh, "flame").unwrap());
                assert!(fhi[1] < hi[1] - s.body.width * 0.3 && flo[1] > lo[1], "seed {}: flame {flo:?}..{fhi:?} outside", s.seed);
                assert!(light.position[1] > flo[1] && light.position[1] < fhi[1], "seed {}", s.seed);
                assert!(light.radius > 70.0 && light.radius < 110.0, "seed {}: radius {}", s.seed, light.radius);
            }
            None => assert!(!s.flame.lit, "seed {}", s.seed),
        }
    }
}

// ───────────────────────── The censer ─────────────────────────

use armory::incense::IncenseKind;
use armory::kindle::INCENSE_SECONDS;

fn censer(seed: u32, kind: IncenseKind, lit: bool) -> LanternState {
    let mut s = Lantern::roll_censer(seed, kind);
    s.flame.lit = lit;
    s
}

fn glow(m: &ItemMesh, name: &str) -> f32 {
    part(m, name).unwrap().glow
}

#[test]
fn a_censer_is_a_pierced_bowl_on_chains_and_deterministic() {
    for seed in 1..=64 {
        for kind in IncenseKind::ALL {
            let s = censer(seed, kind, seed % 2 == 0);
            assert_eq!((s.body.style, s.flame.fuel, s.flame.incense), (LanternStyle::Censer, LanternFuel::Incense, kind));
            let mesh = Lantern::build(&s);
            assert_eq!(mesh, Lantern::build(&s), "seed {seed}");
            for p in &mesh.parts {
                p.validate().unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            }
            for name in ["ring", "chain", "chains", "frame", "holes", "coals", "incense"] {
                assert!(part(&mesh, name).is_some(), "seed {seed}: no {name}");
            }
            assert!(part(&mesh, "flame").is_none() && part(&mesh, "bail").is_none());
            // A dense pierced lid: many holes, each a real cylinder.
            assert!(part(&mesh, "holes").unwrap().positions.len() > 300, "seed {seed}: too few holes");
            let (ring_lo, ring_hi) = bounds(part(&mesh, "ring").unwrap());
            let (lo, hi) = bounds(part(&mesh, "frame").unwrap());
            assert!(ring_lo[1] < 0.0 && ring_hi[1] > 0.0 && hi[1] < ring_lo[1] + 0.05 && lo[1] > -4.5, "seed {seed}: {lo:?}..{hi:?}");
        }
    }
}

#[test]
fn a_lit_censer_smokes_and_glows_a_small_warm_light() {
    for kind in IncenseKind::ALL {
        let lit = censer(3, kind, true);
        let smoke = Lantern::smoke(&lit).expect("a lit censer smokes");
        assert_eq!(smoke.color, kind.smoke());
        let light = Lantern::light(&lit).expect("a lit censer glows");
        // Warmer and smaller than any lantern's 30 ft.
        assert!(light.radius < 40.0 && light.color.0 > light.color.2, "{light:?}");
        let mesh = Lantern::build(&lit);
        let (flo, fhi) = bounds(part(&mesh, "frame").unwrap());
        assert!(light.position[1] > flo[1] && light.position[1] < fhi[1]);
        assert!(smoke.position[1] > light.position[1] && smoke.position[1] <= fhi[1] + 0.2);
        assert!(glow(&mesh, "holes") > 0.5 && glow(&mesh, "coals") > 0.5);

        let out = censer(3, kind, false);
        assert!(Lantern::smoke(&out).is_none() && Lantern::light(&out).is_none());
        let dark = Lantern::build(&out);
        assert!(glow(&dark, "holes") == 0.0 && glow(&dark, "coals") == 0.0);
    }
}

#[test]
fn each_incense_is_visible_on_the_censer() {
    let nouns: Vec<String> = IncenseKind::ALL.iter().map(|&k| Lantern::noun(&censer(5, k, false))).collect();
    assert_eq!(nouns, ["Cedar Censer", "Myrrh Censer", "Frankincense Censer"]);
    let tint = |k| Lantern::build(&censer(5, k, false)).parts.iter().find(|p| p.name == "incense").unwrap().tint;
    assert_ne!(tint(IncenseKind::Cedar), tint(IncenseKind::Frankincense));
    assert_ne!(tint(IncenseKind::Myrrh), tint(IncenseKind::Frankincense));
}

#[test]
fn a_censer_burns_its_incense_down_and_goes_out() {
    use armory::Item;
    let mut item = Item::Lantern(censer(7, IncenseKind::Myrrh, true));
    assert_eq!(item.fuel_capacity(), Some(INCENSE_SECONDS));
    assert!(item.load_incense(IncenseKind::Myrrh));
    assert_eq!(item.fuel_left(), Some(INCENSE_SECONDS));
    item.burn(INCENSE_SECONDS * 0.5, INCENSE_SECONDS);
    assert!((item.fuel_left().unwrap() - INCENSE_SECONDS * 0.5).abs() < 1.0);
    item.burn(INCENSE_SECONDS, INCENSE_SECONDS);
    assert_eq!((item.lit(), item.fuel_left(), item.incense()), (Some(false), Some(0.0), None), "used up, it goes out");
    assert!(item.set_lit(true).is_err(), "no incense, no light");
    // Oil does not fill it; a load of incense does, and it burns what was loaded.
    assert_eq!(item.refuel(INCENSE_SECONDS, INCENSE_SECONDS), 0.0);
    assert!(item.load_incense(IncenseKind::Cedar));
    assert_eq!(item.incense(), Some(IncenseKind::Cedar));
    assert!(item.set_lit(true).is_ok());
    // A plain lantern takes none.
    assert!(!Item::Lantern(Lantern::roll(2)).load_incense(IncenseKind::Cedar));
}
