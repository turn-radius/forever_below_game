//! Shield geometry: deterministic, drawable, complete, and laid out where the state says.

use armory::families::Generator;
use armory::families::shield::{Shield, ShieldState};
use armory::material::MaterialKind;
use armory::sections::device::Device;
use armory::{ItemMesh, Part};

const SEEDS: std::ops::RangeInclusive<u32> = 1..=256;

fn built() -> impl Iterator<Item = (ShieldState, ItemMesh)> {
    SEEDS.map(|seed| {
        let state = Shield::roll(seed);
        let mesh = Shield::build(&state);
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
        let state = Shield::roll(seed);
        assert_eq!(Shield::build(&state), Shield::build(&state), "seed {seed}");
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
        let behind = if part(&mesh, "grip").is_some() { "grip" } else { "straps" };
        let expected = [
            ("body", true),
            ("face", s.field.painted),
            ("charge", s.field.charge != Device::None),
            ("rim", s.rim.on),
            ("boss", s.boss.on),
            ("boss_rivets", s.boss.on),
            ("studs", s.studs.count > 0),
            (behind, true),
        ];
        for (name, on) in expected {
            assert_eq!(part(&mesh, name).is_some(), on, "seed {}: part {name} present should be {on}", s.seed);
        }
    }
}

#[test]
fn painted_parts_carry_their_tinctures() {
    for (s, mesh) in built().filter(|(s, _)| s.field.painted) {
        let face = part(&mesh, "face").unwrap();
        assert_eq!(face.material, Some(MaterialKind::Paint));
        assert_eq!(face.tint, Some(s.field.tincture.rgb()), "seed {}", s.seed);
        assert_eq!(face.pattern, s.field.surface(), "seed {}: the division reaches the shader", s.seed);
        if let Some(charge) = part(&mesh, "charge") {
            assert_eq!(charge.tint, Some(s.field.charge_tincture.rgb()), "seed {}", s.seed);
        }
        // The face's uv spans the field, for the shader's division, centred on the fess point.
        let (lo, hi) = face.uvs.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), u| {
            ([lo[0].min(u[0]), lo[1].min(u[1])], [hi[0].max(u[0]), hi[1].max(u[1])])
        });
        assert!(lo[0] > -0.01 && hi[0] < 1.01 && (hi[1] - lo[1] - 1.0).abs() < 0.05, "seed {}: uv {lo:?}..{hi:?}", s.seed);
        if let Some(charge) = part(&mesh, "charge") {
            // The charge sits where the field's lines cross: uv 0.5, 0.5 under its middle.
            let (clo, chi) = bounds(charge);
            let middle = [(clo[0] + chi[0]) * 0.5, (clo[1] + chi[1]) * 0.5];
            let nearest = (0..face.positions.len())
                .min_by(|&a, &b| {
                    let d = |i: usize| (face.positions[i][0] - middle[0]).hypot(face.positions[i][1] - middle[1]);
                    d(a).total_cmp(&d(b))
                })
                .unwrap();
            let uv = face.uvs[nearest];
            assert!((uv[0] - 0.5).abs() < 0.08 && (uv[1] - 0.5).abs() < 0.08, "seed {}: charge at uv {uv:?}", s.seed);
        }
    }
}

#[test]
fn quarterly_fields_and_bare_boards_reach_the_shader() {
    use armory::fabric::PatternKind;
    use armory::heraldry::Division;
    let mut quarterly = 0;
    let mut boards = 0;
    for (s, mesh) in built() {
        if s.field.painted && s.field.division == Division::Quarterly {
            let face = part(&mesh, "face").unwrap();
            let pattern = face.pattern.unwrap();
            // Four quarters: the first tincture as the face's own colour, the second drawn over it.
            assert_eq!(pattern.kind, PatternKind::Quarterly);
            assert_eq!(face.tint, Some(s.field.tincture.rgb()));
            assert_eq!(pattern.color, s.field.tincture2.rgb());
            quarterly += 1;
        }
        if !s.field.painted && s.body.material == MaterialKind::Wood && s.body.planks > 0 {
            let body = part(&mesh, "body").unwrap();
            let pattern = body.pattern.unwrap();
            assert_eq!(pattern.kind, PatternKind::Planks);
            assert_eq!(pattern.scale, s.body.planks as f32, "seed {}: one seam per board", s.seed);
            boards += 1;
        }
    }
    assert!(quarterly > 0 && boards > 0, "{quarterly} quarterly fields, {boards} plank boards");
}

#[test]
fn the_board_has_its_size_and_faces_forward() {
    for (s, mesh) in built() {
        let b = s.body;
        let (lo, hi) = bounds(part(&mesh, "body").unwrap());
        let height = hi[1] - lo[1];
        let width = hi[0] - lo[0];
        // The face keeps its length along the curve, so the chord is a little shorter.
        assert!(height > b.size * 0.8 && height < b.size * 1.02, "seed {}: height {height} for size {}", s.seed, b.size);
        assert!(width > b.breadth() * 0.6 && width < b.breadth() * 1.02, "seed {}: width {width} for {}", s.seed, b.breadth());
        // Curved back from the face: nothing of the board sits in front of the face's centre.
        assert!(hi[2] < b.thickness * 0.5 + 0.01, "seed {}: board reaches z {}", s.seed, hi[2]);
    }
}

#[test]
fn fittings_stay_on_their_side() {
    for (s, mesh) in built() {
        let th = s.body.thickness;
        // The boss and the charge stand proud of the face's centre.
        for name in ["boss", "charge"] {
            if let Some(p) = part(&mesh, name) {
                let (_, hi) = bounds(p);
                assert!(hi[2] > th * 0.5, "seed {}: {name} is not on the face", s.seed);
            }
        }
        let (body_lo, body_hi) = bounds(part(&mesh, "body").unwrap());
        if let Some(p) = part(&mesh, "studs") {
            let (lo, hi) = bounds(p);
            assert!(lo[0] >= body_lo[0] && hi[0] <= body_hi[0] && lo[1] >= body_lo[1] && hi[1] <= body_hi[1], "seed {}: studs off the board", s.seed);
        }
        for name in ["grip", "straps"] {
            if let Some(p) = part(&mesh, name) {
                let (lo, hi) = bounds(p);
                assert!(hi[2] < th * 0.5, "seed {}: {name} pokes through the face", s.seed);
                assert!(lo[0] >= body_lo[0] && hi[0] <= body_hi[0] && lo[1] >= body_lo[1] && hi[1] <= body_hi[1], "seed {}: {name} hangs off the board", s.seed);
            }
        }
    }
}
