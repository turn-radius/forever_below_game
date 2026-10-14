//! Rolled sheets: a blanket, a bandage or a hide rolled up, its spiral showing at each end,
//! and the straps that hold a roll shut. A backpack's bedroll, a bedroll on its own, a roll
//! of linen bandage.

use std::f32::consts::{PI, TAU};

use crate::build::strap::{buckle, strap};
use crate::geom::{LatheOptions, MeshBuilder, Shading, SweepOptions, V3, lathe, spline_profile, tube, vec3};

/// A roll of `radius` and `length` lying along X, centred on `centre`. Returns the roll and
/// the rolled edge at each end: an Archimedean spiral of `turns` turns of wire `edge` thick,
/// just proud of the ends.
pub fn rolled_sheet(centre: V3, radius: f32, length: f32, edge: f32, turns: f32) -> (MeshBuilder, MeshBuilder) {
    let (r, len) = (radius, length);
    let control = [[0.0, -len * 0.5], [r * 0.9, -len * 0.5], [r, -len * 0.5 + r * 0.2], [r, len * 0.5 - r * 0.2], [r * 0.9, len * 0.5], [0.0, len * 0.5]];
    let mut roll = lathe(&spline_profile(&control, 24), &LatheOptions::new(40, Shading::SMOOTH));
    roll.rotate_z(-PI * 0.5).translate(centre);

    let mut spirals = MeshBuilder::new();
    for side in [-1.0f32, 1.0] {
        let path: Vec<V3> = (0..=96)
            .map(|i| {
                let k = i as f32 / 96.0;
                let (s, c) = (k * turns * TAU).sin_cos();
                let rr = r * (0.12 + 0.75 * k);
                vec3::add(centre, [side * len * 0.5, s * rr, c * rr])
            })
            .collect();
        spirals.append(&tube(&path, 6, &SweepOptions::default(), |_| edge));
    }
    (roll, spirals)
}

/// Straps round a [`rolled_sheet`] at each of `xs` (offsets along the roll from `centre`),
/// `width` wide and `thickness` thick, each buckled at the front (+Z) with a buckle of round
/// wire `wire` thick. Returns `(straps, buckles)`.
pub fn roll_straps(centre: V3, radius: f32, xs: &[f32], width: f32, thickness: f32, wire: f32) -> (MeshBuilder, MeshBuilder) {
    let r = radius;
    let (mut straps, mut buckles) = (MeshBuilder::new(), MeshBuilder::new());
    for &x in xs {
        let ring: Vec<V3> = (0..48)
            .map(|i| {
                let a = TAU * i as f32 / 48.0;
                vec3::add(centre, [x, a.sin() * r * 0.98, a.cos() * r * 0.98])
            })
            .collect();
        let faces: Vec<V3> = ring.iter().map(|&p| vec3::sub(p, [x, centre[1], centre[2]])).collect();
        straps.append(&strap(&ring, &faces, width, thickness, true));
        let mut bk = buckle(width, wire);
        bk.place_basis(vec3::add(centre, [x, 0.0, r * 0.98 + thickness]), vec3::Y, vec3::Z);
        buckles.append(&bk);
    }
    (straps, buckles)
}
