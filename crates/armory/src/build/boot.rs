//! Shoes, boots, leg wraps, sabatons and greaves on a [`LegForm`]: a right foot in profile,
//! toe toward +X, sole on y = 0. Built in the form's space so `fb_body` can pass a folk leg.
//!
//! Departures from the prototype's `buildFeet`: the heel and toe close in a rounded point
//! rather than a vertical slit, the sole follows the rounded outline, and every sheet (shaft,
//! cuff, straps, lames, wraps) has thickness.

use std::f32::consts::{PI, TAU};

use crate::build::form::{FRONT, LegForm, ShellOptions, patch, rolled_edge};
use crate::build::garment::buckle_mesh;
use crate::build::hafted::smoothstep;
use crate::build::parts::{Look, Parts};
use crate::fabric::{Fabric, PatternKind, SurfacePattern};
use crate::geom::polygon::P2;
use crate::geom::vec3::{self, V3};
use crate::geom::{GridOptions, LatheOptions, MeshBuilder, Shading, extrude, lathe, spline_profile, stitch_grid, tube, SweepOptions};
use crate::material::{MaterialKind, Rgb};
use crate::sections::boot::{Boot, BootStyle};
use crate::sections::hem::Hem;

/// The foot's surface from heel to toe, `inflate` out, as a closed solid.
pub fn foot_mesh(form: &LegForm, inflate: f32) -> MeshBuilder {
    let rows = 80;
    let toe = form.toe_x();
    let points: Vec<Vec<V3>> =
        (0..=rows).map(|r| (0..48).map(|c| form.foot_point(TAU * c as f32 / 48.0, toe * r as f32 / rows as f32, inflate)).collect()).collect();
    stitch_grid(&points, &GridOptions { wrap_around: true, ..Default::default() })
}

/// A sole `thickness` deep under the foot's outline, to `end` along it. Cut short of the toe,
/// it ends square across.
pub fn sole_mesh(form: &LegForm, end: f32, thickness: f32) -> MeshBuilder {
    let margin = 0.015 * form.foot_length / 2.6;
    let half = |x: f32| {
        let (w, k) = form.half_width(x);
        w * k * 1.06 + margin * k
    };
    let mut outline: Vec<P2> = (0..=40).map(|i| end * i as f32 / 40.0).map(|x| [x, -half(x)]).collect();
    outline.extend((1..=40).rev().map(|i| end * i as f32 / 40.0).map(|x| [x, half(x)]));
    outline.dedup_by(|a, b| (a[0] - b[0]).abs() + (a[1] - b[1]).abs() < 1e-5);
    let mut sole = extrude(&outline, &[], thickness);
    sole.rotate_x(PI / 2.0).translate([0.0, -thickness / 2.0, 0.0]);
    sole
}

/// Every part of a piece of footwear on `form`: `"foot"`, `"sole"`, `"heel"`, `"shaft"`,
/// `"lining"`, `"rim"`, `"cuff"`, `"trim"`, `"straps"`, `"buckles"`, `"lames"`, `"edges"`,
/// `"knee"` and `"binding"`, as the style calls for them.
pub fn boot_parts(form: &LegForm, boot: &Boot, hem: &Hem) -> Parts {
    let mut parts = Parts::new();
    let s = boot.size;
    let style = boot.style;
    let l = form.foot_length;
    let toe = form.toe_x();
    let leather = Look::of(boot.material);
    let plate = Look::of(boot.plate);
    let mail = Look::patterned(boot.plate, SurfacePattern { kind: PatternKind::Mail, color: Rgb::hex(0x232323), scale: 9.0 / s });
    let trim = (hem.on && hem.hemline).then(|| Look::cloth(&hem.fabric));
    let sabaton = style == BootStyle::Sabaton;

    // Foot, sole and heel.
    parts.add_look("foot", foot_mesh(form, 0.0), if sabaton { mail } else { leather });
    let sole_end = if form.pointed() { toe.min(0.84 * l + 0.25 * (toe - l)) } else { toe };
    parts.add_look("sole", sole_mesh(form, sole_end, 0.08 * s), Look::of(boot.sole));
    if matches!(style, BootStyle::Boot | BootStyle::Sabaton) {
        // A stacked heel under the back of the sole, rounded as the sole is and cut square
        // across its breast.
        let mut heel = sole_mesh(form, 0.25 * l, 0.13 * s);
        heel.translate([0.0, -0.08 * s, 0.0]);
        parts.add_look("heel", heel, Look::of(boot.sole));
    }

    // Shaft up the leg.
    let height = boot.height * s;
    let y0 = 0.08 * s;
    let ridge = if sabaton { 0.07 } else { 0.0 };
    // The shaft takes in the foot where they meet: low on the leg its sides spread round the
    // foot's (smoothly, so no crease), or the foot pokes out of them beside the ankle.
    let shaft = |angle: f32, v: f32, inflate: f32| -> V3 {
        let p = form.shin_point(angle, height * v, height, ridge, inflate);
        let y = p[1] + y0;
        let width = form.foot_half_width_at(p[0], y);
        // Clear of the foot by a margin that fades out with it, so the shaft above is untouched.
        let foot = width + (inflate + 0.035 * s) * smoothstep(0.0, 0.05 * s, width);
        let side = if p[2].abs() > 1e-6 { p[2].signum() } else { -angle.sin().signum() };
        let (z, k) = (p[2].abs(), 0.04 * s);
        // A smooth maximum of the two, equal to `z` wherever the leg is already the wider.
        let z = (0.5 * (z + foot + ((z - foot).powi(2) + k * k).sqrt()) - 0.5 * k).max(z);
        [p[0], y, side * z]
    };
    let band = |v0: f32, v1: f32, rows: usize, inflate: f32, thickness: f32| -> MeshBuilder {
        patch(56, rows, &ShellOptions::tube(thickness), |u, v| shaft(TAU * u, v0 + (v1 - v0) * v, inflate))
    };
    let low = height < 0.65 && !matches!(style, BootStyle::Sabaton | BootStyle::Wrapped);
    let shaft_look = match style {
        BootStyle::Sabaton => plate,
        BootStyle::Wrapped => Look::cloth(&Fabric::plain(boot.leg_cloth)),
        _ => leather,
    };
    // Leg wraps are wound over a shoe that comes up past the instep, and start above where
    // the foot meets the leg, so the shoe never pokes out between their turns.
    let collar = if style == BootStyle::Wrapped { ((form.instep_height + 0.15 * s) / height).min(0.5) } else { 0.0 };
    if !low {
        // Rows enough to carry the shaft's spread round the foot without stepping the light.
        let rows = ((height * 14.0).round() as usize).max(12);
        if collar > 0.0 {
            parts.add_look("shaft", band(0.0, collar, 12, 0.0, 0.03 * s), leather);
            parts.add_look("shaft", band(collar, 1.0, rows, 0.0, 0.03 * s), shaft_look);
        } else {
            parts.add_look("shaft", band(0.0, 1.0, rows, 0.0, 0.03 * s), shaft_look);
        }
    } else {
        // The opening of a low shoe: a dark lining and a rolled rim.
        let (cx, rx, rz) = (form.ankle_x - 0.02 * s, 0.42 * s, 0.3 * s * boot.width);
        let top_at = |x: f32, z: f32| {
            let (w, _) = form.half_width(x);
            form.foot_point(PI / 2.0, x, 0.0)[1] * (1.0 - 0.22 * (z / w.max(0.05)).powi(2).min(1.0)) - 0.012 * s
        };
        let opening: Vec<Vec<V3>> = (0..=6)
            .map(|r| {
                let v = r as f32 / 6.0;
                (0..40)
                    .map(|c| {
                        let t = TAU * c as f32 / 40.0;
                        let (x, z) = (cx + t.cos() * rx * v, t.sin() * rz * v);
                        [x, top_at(x, z) + 0.02 * s, z]
                    })
                    .collect()
            })
            .collect();
        parts.add_look("lining", stitch_grid(&opening, &GridOptions { wrap_around: true, ..Default::default() }), leather.tinted(Rgb::hex(0x1a1512)));
        let rim: Vec<V3> = (0..64)
            .map(|i| {
                let t = TAU * i as f32 / 64.0;
                let (x, z) = (cx + t.cos() * rx, t.sin() * rz);
                [x, top_at(x, z) + 0.02 * s, z]
            })
            .collect();
        parts.add_look("rim", rolled_edge(&rim, 0.035 * s, true), trim.unwrap_or(leather));
    }
    if boot.cuff > 0.0 && !matches!(style, BootStyle::Sabaton | BootStyle::Wrapped) && !low {
        let ch = (boot.cuff * s).min(height * 0.6);
        let v0 = 1.0 - ch / height;
        parts.add_look("cuff", band(v0, 1.02, 4, 0.06 * s, 0.03 * s), trim.unwrap_or(leather));
    } else if let Some(trim) = trim.filter(|_| !low) {
        let v0 = (1.0 - hem.width * 0.5 / height).max(0.0);
        parts.add_look("trim", band(v0, 1.0, 3, 0.015 * s, 0.045 * s), trim);
    }

    // Straps with buckles: round the shaft of a boot, otherwise over the instep.
    let tall = height > 2.0 * s;
    for i in 0..boot.straps {
        let on_shaft = style == BootStyle::Boot && (tall || i > 0 && height > 1.0);
        if on_shaft {
            let v = (i as f32 + 0.6) / (boot.straps as f32 + 0.4) * 0.85;
            parts.add_look("straps", band(v - 0.03, v + 0.03, 2, 0.025 * s, 0.03 * s), leather);
            parts.add_look("buckles", buckle_mesh(0.16 * s, shaft(FRONT, v, 0.06 * s)), Look::of(MaterialKind::Iron));
        } else {
            let x = 0.48 * l + i as f32 * 0.13 * l;
            let strap = patch(48, 2, &ShellOptions::tube(0.03 * s), |u, v| form.foot_point(TAU * u, x - 0.05 * s + 0.1 * s * v, 0.025 * s));
            parts.add_look("straps", strap, leather);
            parts.add_look("buckles", buckle_mesh(0.14 * s, form.foot_point(PI, x, 0.05 * s)), Look::of(MaterialKind::Iron));
        }
    }

    // Sabaton lames over the foot, each lapping over the next toward the toe; the greave's
    // rolled top edge; and the knee cop.
    if sabaton {
        let n = boot.lames.max(1);
        let (x0, x1) = (0.36 * l, toe * 0.97);
        let lw = (x1 - x0) / n as f32;
        let sheet = ShellOptions::sheet(0.025 * s);
        for i in 0..n {
            let a = x0 + i as f32 * lw;
            let b = (a + lw * 1.3).min(x1);
            let inflate = (0.035 + 0.012 * (n - i) as f32) * s;
            let angle = |u: f32| -0.25 + (PI + 0.5) * u;
            parts.add_look("lames", patch(30, 4, &sheet, |u, v| form.foot_point(angle(u), a + (b - a) * v, inflate)), plate);
            let edge: Vec<V3> = (0..=20).map(|j| form.foot_point(angle(j as f32 / 20.0), b, inflate + 0.01 * s)).collect();
            parts.add_look("edges", rolled_edge(&edge, 0.022 * s, false), plate);
        }
        let top: Vec<V3> = (0..64).map(|i| shaft(TAU * i as f32 / 64.0, 1.0, 0.01 * s)).collect();
        parts.add_look("edges", rolled_edge(&top, 0.035 * s, true), plate);
        if boot.knee {
            // The cop over the front of the knee and the wing over its side, each seated on the
            // greave's surface and facing out along it, the cop's crown level with the rolled top.
            let seat = |angle: f32, v: f32| -> (V3, V3) {
                let (da, dv) = (1e-3, 1e-3);
                let around = vec3::sub(shaft(angle + da, v, 0.0), shaft(angle - da, v, 0.0));
                let up = vec3::sub(shaft(angle, v + dv, 0.0), shaft(angle, v - dv, 0.0));
                let at = shaft(angle, v, 0.0);
                let out = vec3::normalize_or(vec3::cross(up, around), vec3::Z);
                let centre = [form.ankle_x, at[1], 0.0];
                (at, if vec3::dot(out, vec3::sub(at, centre)) < 0.0 { vec3::neg(out) } else { out })
            };
            let kr = 0.42 * s;
            let v = 1.0 - kr * 0.9 / height;
            let profile = spline_profile(&[[kr, 0.0], [kr * 0.92, kr * 0.38], [kr * 0.6, kr * 0.7], [0.0, kr * 0.82]], 16);
            let mut cop = lathe(&profile, &LatheOptions::new(24, Shading::SMOOTH));
            let (at, out) = seat(0.0, v);
            cop.scale([1.0, 0.75, 1.0]).place(vec3::sub(at, vec3::scale(out, kr * 0.3)), out);
            parts.add_look("knee", cop, plate);
            let wing_profile = spline_profile(&[[kr * 0.7, 0.0], [kr * 0.62, kr * 0.2], [0.0, kr * 0.26]], 10);
            let mut wing = lathe(&wing_profile, &LatheOptions::new(24, Shading::SMOOTH));
            let (at, out) = seat(FRONT + 0.55, v);
            wing.place(vec3::sub(at, vec3::scale(out, kr * 0.08)), out);
            parts.add_look("knee", wing, plate);
        }
    }

    // Wrapped legs: a band wound up the leg, each turn lapping over the one below.
    if style == BootStyle::Wrapped && height > 0.5 {
        let bw = 0.2 * s;
        let start = (collar * height - bw * 0.25).max(0.05);
        let turns = ((height - start) / (bw * 1.05)).round().max(2.0);
        let rise = |u: f32| start + (height - 0.1 - start) * u;
        let ribbon = patch((turns * 36.0) as usize, 2, &ShellOptions::sheet(0.02 * s), |u, v| {
            let y = rise(u) + (v - 0.5) * bw;
            shaft(TAU * turns * u, y / height, (0.015 + 0.025 * (1.0 - v)) * s)
        });
        parts.add_look("binding", ribbon, Look::cloth(&Fabric::plain(boot.wrap_cloth)));
        let n = (turns * 40.0) as usize;
        let edge: Vec<V3> = (0..=n)
            .map(|i| {
                let u = i as f32 / n as f32;
                shaft(TAU * turns * u, (rise(u) - bw * 0.5) / height, 0.045 * s)
            })
            .collect();
        let dark = Rgb(boot.wrap_cloth.0 * 0.55, boot.wrap_cloth.1 * 0.55, boot.wrap_cloth.2 * 0.55);
        parts.add_look("binding", tube(&edge, 6, &SweepOptions::default(), |_| 0.02 * s), Look::cloth(&Fabric::plain(dark)));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    #[test]
    fn the_foot_closes_at_heel_and_toe() {
        for toe in [0.0, 0.8] {
            let form = LegForm::mannequin(1.0, 1.0, toe);
            let mesh = foot_mesh(&form, 0.0);
            let (lo, hi) = mesh.bounds().unwrap();
            assert!(lo[0] > -0.01 && hi[0] < form.toe_x() + 0.01, "{lo:?} {hi:?}");
            assert!(lo[1] > -0.01 && hi[1] < form.instep_height + 0.05);
            mesh.into_part("foot", OutlineMode::Welded).validate().unwrap();
        }
    }

    #[test]
    fn the_sole_lies_under_the_foot() {
        let form = LegForm::mannequin(1.0, 1.0, 0.0);
        let (lo, hi) = sole_mesh(&form, form.toe_x(), 0.08).bounds().unwrap();
        assert!(hi[1] < 0.001 && lo[1] > -0.081, "{lo:?} {hi:?}");
        assert!((hi[2] + lo[2]).abs() < 0.01 && hi[2] > 0.4);
    }
}
