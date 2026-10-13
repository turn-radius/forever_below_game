//! Guard geometry, from the [`crate::sections::blade::Guard`] section: crosses and bars with
//! their terminals, centre blocks and langets, rondel discs, ballock lobes, and stones set
//! in the centre. Swords, daggers and spears (wings and lugs) build their guards here.
//!
//! Guard space: the guard is centred on the origin, the blade rises along +Y, the span runs
//! along X (the plane of the blade's width) and the faces look along ±Z (the blade's flats).
//! A hilt stacks it with [`Guard::height`].
//!
//! From the prototype's `buildGuard` in `reference/spline_armory_engine.js`, rebuilt at higher
//! fidelity: both quillons are one continuous swept bar through the centre (no seam), arms
//! without terminals end in rounded tips, bars and blocks are chamfered with hard edges,
//! langets are tapered tongues sitting on the blade's flats, and stones are seated on a face
//! the guard actually has (a block's face, a bezel on a round bar, a disc's rim, the lobes).

use super::gem::{bezel, cut_for, cut_stone};
use crate::gem::GemCut;
use crate::geom::polygon::P2;
use crate::geom::{
    CatmullRom, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, extrude, lathe, resample_by_length, spline_profile, sweep, tube,
    uv_sphere, vec3,
};
use crate::sections::blade::{Guard, GuardStyle, GuardTerminal};

/// What a guard fits round: the blade's base and the top of the grip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuardFit {
    /// Full width of the blade's base.
    pub blade_width: f32,
    /// Full thickness of the blade's base.
    pub blade_thickness: f32,
    /// Radius of the grip (or socket) under the guard.
    pub grip_radius: f32,
}

/// A guard's metal and its stones.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GuardMeshes {
    pub body: MeshBuilder,
    pub stones: MeshBuilder,
}

/// One end of an arm: where it is and which way it points.
type ArmEnd = (V3, V3);

/// Builds `guard` round `fit`. Empty when the guard is off.
pub fn guard_mesh(guard: &Guard, fit: &GuardFit) -> GuardMeshes {
    let mut out = GuardMeshes::default();
    if !guard.on {
        return out;
    }
    match guard.style {
        GuardStyle::Disc => {
            out.body.append(&disc(guard, fit));
        }
        GuardStyle::Lobes => {
            out.body.append(&lobes(guard));
        }
        GuardStyle::Cross | GuardStyle::Bar => {
            let (arms, ends, end_radius) = if guard.style == GuardStyle::Bar { bar(guard) } else { cross(guard) };
            out.body.append(&arms);
            for &(p, d) in &ends {
                out.body.append(&terminal(guard, p, d, end_radius));
            }
            if guard.block > 0.0 {
                out.body.append(&block(guard, fit));
            }
            if guard.langets > 0.0 {
                out.body.append(&langets(guard, fit));
            }
        }
    }
    if guard.gem {
        set_stones(guard, fit, &mut out);
    }
    out
}

/// The depth of the centre block along Z: the arms' thickness, and always enough to
/// enclose the blade's base.
fn block_depth(guard: &Guard, fit: &GuardFit) -> f32 {
    (guard.radius * 2.6).max(fit.blade_thickness * 1.3 + 0.01)
}

/// One quillon's centreline from the centre outward, `sign` +1 to +X, -1 to -X (the
/// prototype's arm points, curl included).
fn arm_points(guard: &Guard, sign: f32) -> Vec<V3> {
    let curve = guard.curve * if guard.s_curve && sign < 0.0 { -1.0 } else { 1.0 };
    let span = guard.span;
    let mut pts: Vec<V3> = (0..=4)
        .map(|k| {
            let f = k as f32 / 4.0;
            [sign * span * f * (1.0 - guard.curl * 0.25), curve * span * 0.45 * f * f, 0.0]
        })
        .collect();
    if guard.curl > 0.0 {
        let mut dir = vec3::normalize_or(vec3::sub(pts[4], pts[3]), [sign, 0.0, 0.0]);
        let mut p = pts[4];
        let mut step = span * 0.16;
        let turn = sign * if curve >= 0.0 { 1.0 } else { -1.0 };
        let base = dir;
        for m in 1..=4 {
            let angle = turn * guard.curl * std::f32::consts::PI * 0.42 * m as f32;
            let (s, c) = angle.sin_cos();
            dir = [base[0] * c - base[1] * s, base[0] * s + base[1] * c, 0.0];
            step *= 0.82;
            p = vec3::add(p, vec3::scale(dir, step));
            pts.push(p);
        }
    }
    pts
}

/// Both quillons as one bar through the centre, tapering toward the ends. Returns the mesh,
/// its two ends, and the radius there.
fn cross(guard: &Guard) -> (MeshBuilder, Vec<ArmEnd>, f32) {
    let r = guard.radius;
    let arm = |sign: f32| resample_by_length(&CatmullRom::centripetal(arm_points(guard, sign)).sample(12), 40, false);
    let (left, right) = (arm(-1.0), arm(1.0));
    let mut path: Vec<V3> = left.iter().rev().copied().collect();
    path.extend(right.iter().skip(1));
    let taper = guard.taper.clamp(0.0, 1.0) * 0.75;
    let end_radius = r * (1.0 - taper);
    let options = SweepOptions { up: Some(vec3::Z), ..Default::default() };
    let mut mesh = tube(&path, 18, &options, |t| r * (1.0 - taper * (2.0 * t - 1.0).abs()));
    let end = |p: &[V3]| -> ArmEnd {
        let n = p.len();
        (p[n - 1], vec3::normalize_or(vec3::sub(p[n - 1], p[n - 2]), vec3::X))
    };
    let ends = vec![end(&right), end(&left)];
    if guard.terminal == GuardTerminal::None {
        // Rounded tips rather than sawn-off ends.
        for &(p, _) in &ends {
            let mut tip = uv_sphere(end_radius, 18, 10);
            tip.translate(p);
            mesh.append(&tip);
        }
    }
    (mesh, ends, end_radius)
}

/// A chamfered octagon `half_a` by `half_b`, in a section's plane.
fn chamfered(half_a: f32, half_b: f32, chamfer: f32) -> Vec<P2> {
    let c = chamfer.min(half_a * 0.9).min(half_b * 0.9);
    vec![
        [half_a, -half_b + c],
        [half_a, half_b - c],
        [half_a - c, half_b],
        [-half_a + c, half_b],
        [-half_a, half_b - c],
        [-half_a, -half_b + c],
        [-half_a + c, -half_b],
        [half_a - c, -half_b],
    ]
}

/// A straight bar of chamfered section, hard edged.
fn bar(guard: &Guard) -> (MeshBuilder, Vec<ArmEnd>, f32) {
    let r = guard.radius;
    let path = vec![[-guard.span, 0.0, 0.0], [0.0, 0.0, 0.0], [guard.span, 0.0, 0.0]];
    // Section x along the frame normal (+Y here), y along the binormal (Z).
    let section = chamfered(r, r * 1.1, r * 0.35);
    let options = SweepOptions { shading: Shading::FACETED, up: Some(vec3::Y), ..Default::default() };
    let mesh = sweep(&path, &section, &options, |_| 1.0);
    let ends = vec![([guard.span, 0.0, 0.0], vec3::X), ([-guard.span, 0.0, 0.0], vec3::neg(vec3::X))];
    (mesh, ends, r)
}

/// A ball, disc or flare finishing an arm at `p`, pointing along `dir`.
fn terminal(guard: &Guard, p: V3, dir: V3, end_radius: f32) -> MeshBuilder {
    let e = guard.terminal_size.max(end_radius * 1.05);
    let turned = |control: &[[f32; 2]], at: f32| {
        let mut m = lathe(&spline_profile(control, 16), &LatheOptions::new(20, Shading::SMOOTH));
        m.place(vec3::add(p, vec3::scale(dir, at)), dir);
        m
    };
    match guard.terminal {
        GuardTerminal::None => MeshBuilder::new(),
        GuardTerminal::Ball => {
            let mut m = uv_sphere(e, 20, 14);
            m.translate(vec3::add(p, vec3::scale(dir, e * 0.6)));
            m
        }
        GuardTerminal::Disc => turned(&[[end_radius, -e * 0.1], [e * 1.1, e * 0.1], [e * 1.15, e * 0.35], [e, e * 0.5], [0.0, e * 0.55]], -e * 0.05),
        GuardTerminal::Flare => {
            turned(&[[end_radius * 0.9, -e * 0.1], [e * 0.6, e * 0.5], [e, e], [e * 1.02, e * 1.15], [0.0, e * 1.18]], -e * 0.1)
        }
    }
}

/// The centre block (escutcheon): a chamfered slab round the blade's base.
fn block(guard: &Guard, fit: &GuardFit) -> MeshBuilder {
    let (w, h) = (guard.block * 0.5, guard.block_height * 0.5);
    extrude(&chamfered(w, h, w.min(h) * 0.3), &[], block_depth(guard, fit))
}

/// Langets: a tapered tongue up each flat of the blade from the top of the guard.
fn langets(guard: &Guard, fit: &GuardFit) -> MeshBuilder {
    let r = guard.radius;
    let len = guard.langets;
    let half = (r * 1.3).max(0.04) * 0.5;
    let plate = (r * 0.5).max(0.012);
    let base = guard.height() * 0.5 - 0.02;
    let outline: Vec<P2> = vec![[-half, 0.0], [half, 0.0], [half * 0.75, len * 0.8], [0.0, len], [-half * 0.75, len * 0.8]];
    let mut out = MeshBuilder::new();
    for side in [1.0f32, -1.0] {
        let mut tongue = extrude(&outline, &[], plate);
        tongue.translate([0.0, base, side * (fit.blade_thickness * 0.5 + plate * 0.5)]);
        out.append(&tongue);
    }
    out
}

/// A rondel: a round plate with a rounded rim, pierced by the tang.
fn disc(guard: &Guard, fit: &GuardFit) -> MeshBuilder {
    let (r, span) = (guard.radius, guard.span.max(fit.grip_radius));
    let control = [[0.0, -r], [span * 0.88, -r * 0.85], [span, -r * 0.3], [span, r * 0.3], [span * 0.88, r * 0.85], [0.0, r]];
    lathe(&spline_profile(&control, 28), &LatheOptions::new(40, Shading::SMOOTH))
}

/// Ballock lobes: two rounded swellings either side of the grip.
fn lobes(guard: &Guard) -> MeshBuilder {
    let lr = guard.span * 0.5;
    let mut out = MeshBuilder::new();
    for side in [1.0f32, -1.0] {
        let mut lobe = uv_sphere(lr, 28, 18);
        lobe.scale([1.0, 0.9, 0.95]).translate([side * lr * 0.92, 0.0, 0.0]);
        out.append(&lobe);
    }
    out
}

/// Stones on both faces: on the block, in a bezel on a round bar, on the disc's rim, or on
/// each lobe.
fn set_stones(guard: &Guard, fit: &GuardFit, out: &mut GuardMeshes) {
    let cut = cut_for(guard.gem_stone, GemCut::Cabochon);
    let mut put = |center: V3, facing: V3, size: f32| {
        let mut stone = cut_stone(cut, size);
        stone.place(center, facing);
        out.stones.append(&stone);
    };
    let r = guard.radius;
    for side in [1.0f32, -1.0] {
        let facing = [0.0, 0.0, side];
        match guard.style {
            GuardStyle::Cross | GuardStyle::Bar if guard.block > 0.0 => {
                let size = guard.gem_size.min(guard.block.min(guard.block_height) * 0.42);
                put([0.0, 0.0, side * (block_depth(guard, fit) * 0.5 - size * 0.05)], facing, size);
            }
            GuardStyle::Cross | GuardStyle::Bar => {
                let size = guard.gem_size;
                let girdle = side * (r * 0.75 + size * 0.35);
                let mut setting = bezel(size, 20, Shading::SMOOTH);
                setting.place([0.0, 0.0, girdle], facing);
                out.body.append(&setting);
                put([0.0, 0.0, girdle], facing, size);
            }
            GuardStyle::Disc => {
                let size = guard.gem_size.min(r * 0.8);
                put([0.0, 0.0, side * (guard.span.max(fit.grip_radius) - size * 0.05)], facing, size);
            }
            GuardStyle::Lobes => {
                let lr = guard.span * 0.5;
                let size = guard.gem_size.min(lr * 0.4);
                for x in [1.0f32, -1.0] {
                    put([x * lr * 0.92, 0.0, side * (lr * 0.95 - size * 0.08)], facing, size);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    const FIT: GuardFit = GuardFit { blade_width: 0.5, blade_thickness: 0.07, grip_radius: 0.15 };

    #[test]
    fn every_style_and_terminal_builds() {
        for style in GuardStyle::ALL {
            for terminal in GuardTerminal::ALL {
                for (block, langets, gem, curl, s_curve) in [(0.0, 0.0, false, 0.0, false), (0.2, 0.4, true, 0.7, true)] {
                    let guard = Guard { style, terminal, block, langets, gem, curl, s_curve, ..Guard::default() };
                    let meshes = guard_mesh(&guard, &FIT);
                    assert!(!meshes.body.is_empty(), "{style:?} {terminal:?}");
                    assert_eq!(meshes.stones.is_empty(), !gem, "{style:?} stones");
                    for mesh in [meshes.body, meshes.stones] {
                        mesh.into_part("guard", OutlineMode::Welded).validate().unwrap();
                    }
                }
            }
        }
    }

    #[test]
    fn an_off_guard_is_empty() {
        let meshes = guard_mesh(&Guard { on: false, ..Guard::default() }, &FIT);
        assert!(meshes.body.is_empty() && meshes.stones.is_empty());
    }
}
