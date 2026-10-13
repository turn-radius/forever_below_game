//! Spear geometry: deterministic, drawable, complete, and laid out where the state says.

use armory::families::Generator;
use armory::families::spear::{Spear, SpearState};
use armory::sections::blade::GuardMount;
use armory::sections::spearhead::LugStyle;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (SpearState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Spear::roll(seed);
        let mesh = Spear::build(&state);
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
        let state = Spear::roll(seed);
        assert_eq!(Spear::build(&state), Spear::build(&state), "seed {seed}");
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
            ("socket", s.socket.on),
            ("blade", true),
            ("lugs", s.lugs.on),
            ("langets", s.socket.on && s.socket.langets > 0),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn the_blade_is_as_long_and_wide_as_the_state_says() {
    for (s, mesh) in built() {
        let (lo, hi) = bounds(part(&mesh, "blade").unwrap());
        let top = s.shaft.length;
        let length = hi[1] - lo[1];
        assert!((length - s.blade.length).abs() < 0.05, "seed {}: blade {length} long, state says {}", s.seed, s.blade.length);
        assert!(lo[1] > top - 0.2 && lo[1] < top + s.socket.length, "seed {}: blade base {} not on the socket over {top}", s.seed, lo[1]);
        let width = hi[0] - lo[0];
        let widest = s.blade.width * s.blade.belly_width.max(s.blade.shoulder).max(1.0) + s.blade.wave_depth * s.blade.width;
        assert!(width > s.blade.width * 0.6 && width < widest + 0.02, "seed {}: blade {width} wide for width {}", s.seed, s.blade.width);
        let thick = hi[2] - lo[2];
        assert!(thick < s.blade.thickness.max(s.blade.width) + 0.01, "seed {}: blade {thick} thick", s.seed);
    }
}

#[test]
fn the_socket_sleeves_the_haft() {
    for (s, mesh) in built() {
        let (lo, hi) = bounds(part(&mesh, "socket").unwrap());
        let top = s.shaft.length;
        assert!(lo[1] < top - s.socket.length * 0.4, "seed {}: socket mouth {} does not reach down the haft", s.seed, lo[1]);
        assert!(hi[1] > top, "seed {}: socket stops below the haft's end", s.seed);
        if let Some(langets) = part(&mesh, "langets") {
            let (llo, lhi) = bounds(langets);
            assert!(lhi[1] <= lo[1] + 0.1, "seed {}: langets start above the mouth", s.seed);
            assert!((lhi[1] - llo[1] - s.socket.langet_length).abs() < 0.2, "seed {}: langets {} long", s.seed, lhi[1] - llo[1]);
        }
    }
}

#[test]
fn lugs_stand_out_where_they_are_mounted() {
    for (s, mesh) in built().filter(|(s, _)| s.lugs.on) {
        let (lo, hi) = bounds(part(&mesh, "lugs").unwrap());
        let (blo, _) = bounds(part(&mesh, "blade").unwrap());
        let (slo, _) = bounds(part(&mesh, "socket").unwrap());
        let reach = (hi[0] - lo[0]) * 0.5;
        assert!(reach > s.lugs.span * 0.8, "seed {}: lugs reach {reach} for a span of {}", s.seed, s.lugs.span);
        assert!(reach < s.lugs.span + 0.5, "seed {}: lugs reach {reach}", s.seed);
        let mid = (lo[1] + hi[1]) * 0.5;
        match s.lugs.mount {
            GuardMount::Socket => assert!(mid > slo[1] && mid < blo[1] + 0.05, "seed {}: socket lugs at {mid}", s.seed),
            GuardMount::Blade => assert!(mid > blo[1] - 0.1, "seed {}: blade lugs at {mid} below the blade {}", s.seed, blo[1]),
        }
        if s.lugs.style == LugStyle::Wings {
            // Flat in the blade's plane.
            assert!(hi[2] - lo[2] < s.lugs.thickness * 1.3 + s.lugs.end_size * 2.4, "seed {}: wings {} thick", s.seed, hi[2] - lo[2]);
        }
    }
}

#[test]
fn spears_are_long_and_slender() {
    for (s, mesh) in built() {
        let (lo, hi) = mesh.parts.iter().map(bounds).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), (a, b)| {
            ([lo[0].min(a[0]), lo[1].min(a[1]), lo[2].min(a[2])], [hi[0].max(b[0]), hi[1].max(b[1]), hi[2].max(b[2])])
        });
        let floor = if s.pommel.on { -s.pommel.length } else { 0.0 } - 0.03;
        assert!(lo[1] >= floor, "seed {}: reaches y = {}, below {floor}", s.seed, lo[1]);
        let height = hi[1] - lo[1];
        let expected = s.shaft.length + s.blade.length;
        assert!(height > expected - 0.5 && height < expected + s.socket.length + 1.3, "seed {}: {height} tall, expected about {expected}", s.seed);
        for k in [0, 2] {
            assert!(hi[k] - lo[k] < 2.5, "seed {}: {} wide", s.seed, hi[k] - lo[k]);
        }
    }
}

#[test]
fn a_broken_point_shortens_the_blade() {
    for seed in [2, 9, 15] {
        let mut s = Spear::roll(seed);
        let whole = bounds(part(&Spear::build(&s), "blade").unwrap());
        s.cond.broken = true;
        let mesh = Spear::build(&s);
        let blade = part(&mesh, "blade").unwrap();
        blade.validate().unwrap();
        let snapped = bounds(blade);
        let lost = whole.1[1] - snapped.1[1];
        assert!(lost > s.blade.length * 0.15, "seed {seed}: only {lost} of {} lost", s.blade.length);
    }
}
