//! Helmet geometry: deterministic, drawable, complete, and laid out where the state says.

use armory::families::Generator;
use armory::families::helm::{CrestStyle, Helm, HelmState, HelmStyle, VisorStyle};
use armory::fabric::PatternKind;
use armory::material::MaterialKind;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (HelmState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Helm::roll(seed);
        let mesh = Helm::build(&state);
        (state, mesh)
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

fn bounds(part: &Part) -> ([f32; 3], [f32; 3]) {
    part.positions.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

#[test]
fn same_state_same_mesh() {
    for seed in [1, 4, 9, 21, 1207] {
        let state = Helm::roll(seed);
        assert_eq!(Helm::build(&state), Helm::build(&state), "seed {seed}");
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
        let style = s.shell.style;
        let expected = [
            ("shell", true),
            ("rolled_edge", !style.is_cap()),
            ("brim", s.shell.brim > 0.0),
            ("cheeks", s.cheeks.on),
            ("visor", s.visor.style == VisorStyle::Snout),
            ("aventail", s.aventail.on),
            ("hem", s.aventail.on && s.hem.on && s.hem.hemline),
            ("crest", s.crest.style != CrestStyle::None),
            ("crest_holder", s.crest.style == CrestStyle::Plume),
            (
                "fittings",
                s.fittings.browband || s.fittings.bands > 0 || s.fittings.nasal || s.fittings.comb > 0.0 || style == HelmStyle::GreatHelm,
            ),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
        if s.fittings.rivets > 0 && style.is_cap() {
            assert!(part(&mesh, "rivets").is_some(), "seed {}: no rivets", s.seed);
        }
    }
}

#[test]
fn plumes_carry_their_colour() {
    for (s, mesh) in built().filter(|(s, _)| s.crest.style == CrestStyle::Plume) {
        let crest = part(&mesh, "crest").unwrap();
        assert_eq!(crest.material, Some(MaterialKind::Cloth));
        assert_eq!(crest.tint, Some(s.crest.color.rgb()), "seed {}", s.seed);
    }
}

#[test]
fn the_aventail_is_mail() {
    for (s, mesh) in built().filter(|(s, _)| s.aventail.on) {
        let av = part(&mesh, "aventail").unwrap();
        assert_eq!(av.pattern.map(|p| (p.kind, p.scale)), Some((PatternKind::Mail, s.aventail.ring_size)), "seed {}", s.seed);
        if let Some(hem) = part(&mesh, "hem") {
            assert_eq!(hem.material, Some(MaterialKind::Cloth));
            assert_eq!(hem.tint, Some(s.hem.fabric.color), "seed {}", s.seed);
        }
    }
}

#[test]
fn the_shell_fits_a_head() {
    for (s, mesh) in built() {
        let sh = s.shell;
        let (lo, hi) = bounds(part(&mesh, "shell").unwrap());
        let width = hi[0] - lo[0];
        assert!(width > sh.radius * 1.8 && width < sh.radius * 2.6, "seed {}: width {width} for radius {}", s.seed, sh.radius);
        // Deeper front to back than across.
        assert!(hi[2] - lo[2] > width * 1.02, "seed {}: not oval", s.seed);
        // The crown is above the brow, and the head fits under it.
        assert!(hi[1] > sh.height * 0.65, "seed {}: crown at {}", s.seed, hi[1]);
        assert!(lo[1] <= 0.01, "seed {}: shell stops above the brow at {}", s.seed, lo[1]);
        // Closed helms come down past the chin.
        if matches!(sh.style, HelmStyle::GreatHelm | HelmStyle::Barbute) {
            assert!(lo[1] < -sh.radius, "seed {}: {:?} ends at {}", s.seed, sh.style, lo[1]);
        }
    }
}

#[test]
fn the_shell_faces_out() {
    // Shell faces mostly point away from the bowl's axis.
    for (s, mesh) in built().take(64) {
        let p = part(&mesh, "shell").unwrap();
        let mut out = 0.0f32;
        let mut total = 0.0f32;
        for (pos, n) in p.positions.iter().zip(&p.normals) {
            let radial = [pos[0], 0.0, pos[2]];
            let d = radial[0] * n[0] + radial[2] * n[2];
            total += d.abs();
            out += d;
        }
        // Outer and inner skins cancel; the outer skin is a little larger, so the sum leans out.
        assert!(out > 0.0, "seed {}: shell faces inward ({out} of {total})", s.seed);
    }
}
