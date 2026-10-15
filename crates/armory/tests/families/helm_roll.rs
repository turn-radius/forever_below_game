//! Helmet "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::helm::{CrestStyle, Helm, HelmState, HelmStyle, LIMITS, STYLE_WEIGHTS, VisorStyle};
use armory::material::MaterialKind;

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<HelmState> {
    range.map(Helm::roll).collect()
}

#[test]
fn same_seed_same_helm() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Helm::roll(seed), Helm::roll(seed));
        assert_eq!(Helm::roll(seed).seed, seed);
    }
    assert_ne!(Helm::roll(1), Helm::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Helm::roll(1207);
    let back: HelmState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 1207] {
        let s = Helm::roll(seed);
        assert_eq!(Helm::roll_as(seed, s.shell.style), s, "seed {seed}");
        assert!(STYLE_WEIGHTS.iter().any(|(style, _)| *style == s.shell.style));
    }
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
        let style = s.shell.style;
        if s.shell.brim > 0.0 {
            assert_eq!(style, HelmStyle::Kettle, "seed {seed}: brim on a {style:?}");
        }
        if s.shell.tail > 0.0 {
            assert_eq!(style, HelmStyle::Sallet, "seed {seed}: tail on a {style:?}");
        }
        if s.fittings.nasal || s.cheeks.on || s.fittings.browband || s.fittings.bands > 0 {
            assert!(style.is_cap(), "seed {seed}: cap fittings on a {style:?}");
        }
        match s.visor.style {
            VisorStyle::Slit => assert!(matches!(style, HelmStyle::GreatHelm | HelmStyle::Sallet), "seed {seed}"),
            VisorStyle::Snout => assert_eq!(style, HelmStyle::Bascinet, "seed {seed}"),
            VisorStyle::None => {
                assert_ne!(style, HelmStyle::GreatHelm, "seed {seed}: a great helm needs its slits");
                assert_eq!(s.visor.breaths, 0, "seed {seed}: breaths with no visor");
            }
        }
        if style == HelmStyle::Nasal && s.shell.is_segmented() {
            assert_eq!(s.fittings.bands, s.shell.facets, "seed {seed}: spangen bands off the seams");
        }
        if s.hem.on {
            assert!(s.aventail.on, "seed {seed}: a hem with no aventail to trim");
        }
        if s.shell.material == MaterialKind::Leather {
            assert_ne!(s.fittings.material, MaterialKind::Leather, "seed {seed}: leather fittings on leather");
        }
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&HelmState) -> bool| all.iter().any(f);
    for style in HelmStyle::ALL {
        assert!(any(&|s| s.shell.style == style), "no {style:?}");
    }
    for visor in [VisorStyle::None, VisorStyle::Slit, VisorStyle::Snout] {
        assert!(any(&|s| s.visor.style == visor), "no {visor:?} visor");
    }
    for crest in [CrestStyle::None, CrestStyle::Plume, CrestStyle::Horns] {
        assert!(any(&|s| s.crest.style == crest), "no {crest:?} crest");
    }
    assert!(any(&|s| s.shell.is_segmented()) && any(&|s| !s.shell.is_segmented()), "segments never vary");
    assert!(any(&|s| s.fittings.browband), "no brow band");
    assert!(any(&|s| s.fittings.bands > 0), "no spangen bands");
    assert!(any(&|s| s.fittings.nasal), "no nasal");
    assert!(any(&|s| s.fittings.comb > 0.0), "no comb");
    assert!(any(&|s| s.cheeks.on), "no cheek plates");
    assert!(any(&|s| s.aventail.on), "no aventail");
    assert!(any(&|s| s.hem.on), "no aventail hem");
    assert!(any(&|s| s.etch.on), "no etching");
    for name in [
        "Kettle Hat", "Nasal Helm", "Spangenhelm", "Bascinet", "Hounskull", "Great Helm", "Sallet", "Skullcap", "Horned Helm", "Barbute",
    ] {
        assert!(all.iter().any(|s| Helm::noun(s) == name), "no {name}");
    }
    assert!(all.iter().any(|s| Helm::noun(s).starts_with("Plumed ")), "no plumed helm");
}

#[test]
fn nouns() {
    let mut s = HelmState::default();
    let cases = [
        (HelmStyle::Kettle, "Kettle Hat"),
        (HelmStyle::Nasal, "Nasal Helm"),
        (HelmStyle::Bascinet, "Bascinet"),
        (HelmStyle::GreatHelm, "Great Helm"),
        (HelmStyle::Sallet, "Sallet"),
        (HelmStyle::Skullcap, "Skullcap"),
        (HelmStyle::Barbute, "Barbute"),
    ];
    for (style, name) in cases {
        s.shell.style = style;
        assert_eq!(Helm::noun(&s), name);
    }
    s.shell.style = HelmStyle::Nasal;
    s.fittings.bands = 4;
    assert_eq!(Helm::noun(&s), "Spangenhelm");
    s.crest.style = CrestStyle::Plume;
    assert_eq!(Helm::noun(&s), "Plumed Spangenhelm");
    s.shell.style = HelmStyle::Skullcap;
    s.crest.style = CrestStyle::Horns;
    assert_eq!(Helm::noun(&s), "Horned Helm");
    s.shell.style = HelmStyle::Bascinet;
    s.crest.style = CrestStyle::None;
    s.visor.style = VisorStyle::Snout;
    assert_eq!(Helm::noun(&s), "Hounskull");
}
