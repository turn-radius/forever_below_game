//! Box hardware: drop handles, lock plates and keyholes, hasps and padlocks. Chests and
//! strongboxes today; doors, coffers and cabinets later.
//!
//! Each piece is built on a wall in local space: the wall is the plane z = 0, the piece
//! stands out of it toward +Z, and +Y is up the wall. [`MeshBuilder::place_basis`] sets it on
//! any wall.

use crate::geom::{CatmullRom, MeshBuilder, P2, Shading, SweepOptions, extrude, extrude_smooth, tube};

fn arc(cx: f32, cy: f32, r: f32, a0: f32, a1: f32, n: usize) -> Vec<P2> {
    (0..=n).map(|i| {
        let a = a0 + (a1 - a0) * i as f32 / n as f32;
        [cx + a.cos() * r, cy + a.sin() * r]
    }).collect()
}

/// A rectangle with rounded corners, `w` by `h`, centred on (`cx`, `cy`).
fn rounded_rect(cx: f32, cy: f32, w: f32, h: f32, r: f32) -> Vec<P2> {
    use std::f32::consts::FRAC_PI_2;
    let r = r.min(w * 0.5).min(h * 0.5);
    let (x, y) = (w * 0.5 - r, h * 0.5 - r);
    let mut out = Vec::new();
    for (k, (sx, sy)) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)].into_iter().enumerate() {
        let a0 = k as f32 * FRAC_PI_2;
        out.extend(arc(cx + sx * x, cy + sy * y, r, a0, a0 + FRAC_PI_2, 4));
    }
    out
}

/// A keyhole outline `size` tall, centred on the origin: a round head over a slot that
/// widens toward the bottom.
pub fn keyhole_outline(size: f32) -> Vec<P2> {
    let r = size * 0.24;
    let cy = size * 0.5 - r;
    // The slot meets the circle where the circle is `0.35 * r` either side of the middle.
    let a_right = -(0.35f32).acos();
    let a_left = std::f32::consts::PI - a_right;
    let mut out = vec![[-0.55 * r, -size * 0.5], [0.55 * r, -size * 0.5]];
    out.extend(arc(0.0, cy, r, a_right, a_left, 20));
    out
}

/// A shield-shaped lock plate `width` by `height`, `thickness` thick, its back on z = 0,
/// pierced by a keyhole. Returns the plate and a thin dark recess that sits in the keyhole.
pub fn lock_plate(width: f32, height: f32, thickness: f32) -> (MeshBuilder, MeshBuilder) {
    // The prototype's `pendantPts('shield')`, unit height.
    let half = CatmullRom::centripetal(vec![[0.38, 0.42, 0.0], [0.37, 0.08, 0.0], [0.24, -0.26, 0.0], [0.0, -0.5, 0.0]]).sample(6);
    let mut outline: Vec<P2> = vec![[-0.38, 0.46], [0.38, 0.46]];
    outline.extend(half.iter().map(|p| [p[0], p[1]]));
    outline.extend(half.iter().rev().skip(1).take(half.len() - 2).map(|p| [-p[0], p[1]]));
    let outline: Vec<P2> = outline.iter().map(|&[x, y]| [x / 0.76 * width, y * height]).collect();
    let hole_size = height * 0.42;
    let hole: Vec<P2> = keyhole_outline(hole_size).into_iter().map(|[x, y]| [x, y + height * 0.02]).collect();
    let mut plate = extrude(&outline, std::slice::from_ref(&hole), thickness);
    plate.translate([0.0, 0.0, thickness * 0.5]);
    let mut recess = extrude(&hole, &[], thickness * 0.3);
    recess.translate([0.0, 0.0, thickness * 0.15]);
    (plate, recess)
}

/// A drop handle on a backplate: a half-round bail of round bar `wire` thick and `span`
/// wide, hanging below two staples. The plate is `0.9 * wire` thick on z = 0.
pub fn drop_handle(span: f32, wire: f32) -> MeshBuilder {
    let plate_t = wire * 0.9;
    let mut mesh = extrude(&rounded_rect(0.0, -span * 0.12, span * 1.3, span * 0.5, span * 0.1), &[], plate_t);
    mesh.translate([0.0, 0.0, plate_t * 0.5]);
    let z = plate_t + wire * 1.4;
    let half = span * 0.5;
    // The bail: up from one staple, round under, and back to the other, hanging flat.
    let bail: Vec<[f32; 3]> = (0..=24)
        .map(|i| {
            let a = std::f32::consts::PI * i as f32 / 24.0;
            [-half * a.cos(), -half * 0.85 * a.sin(), z + wire * 0.4 * a.sin()]
        })
        .collect();
    mesh.append(&tube(&bail, 10, &SweepOptions::default(), |_| wire));
    for x in [-half, half] {
        // A staple: a small hoop through the plate, round the bail's end.
        let staple: Vec<[f32; 3]> = (0..=12)
            .map(|i| {
                let a = std::f32::consts::PI * i as f32 / 12.0;
                [x, wire * 1.6 * a.cos(), plate_t * 0.5 + wire * 2.2 * a.sin()]
            })
            .collect();
        mesh.append(&tube(&staple, 8, &SweepOptions::default(), |_| wire * 0.55));
    }
    mesh
}

/// A hasp: a strap `width` wide hanging `length` down from a hinge knuckle at the origin,
/// with a rounded end and a slot near it for a staple or a padlock. Back on z = 0.
pub fn hasp(width: f32, length: f32, thickness: f32) -> MeshBuilder {
    let w = width * 0.5;
    let mut outline: Vec<P2> = vec![[w, 0.0]];
    outline.extend(arc(0.0, -length + w, w, 0.0, -std::f32::consts::PI, 10));
    outline.push([-w, 0.0]);
    let slot = rounded_rect(0.0, -length + w * 1.1, width * 0.32, width * 0.62, width * 0.12);
    let mut mesh = extrude(&outline, &[slot], thickness);
    mesh.translate([0.0, 0.0, thickness * 0.5]);
    let knuckle_r = thickness * 1.4;
    let mut knuckle = crate::geom::cylinder(knuckle_r, width * 1.1, 10, Shading::BANDED);
    knuckle.translate([0.0, -width * 0.55, 0.0]).rotate_z(std::f32::consts::FRAC_PI_2).translate([0.0, 0.0, knuckle_r]);
    mesh.append(&knuckle);
    mesh
}

/// A padlock `size` wide hanging from its shackle, whose top is at the origin. The body is a
/// rounded block facing +Z with its middle on z = 0.
pub fn padlock(size: f32) -> MeshBuilder {
    let wire = size * 0.09;
    let shackle_r = size * 0.28;
    let body_top = -shackle_r * 1.6;
    // The body: a round-bottomed block, smooth round its sides.
    let mut outline: Vec<P2> = vec![[size * 0.5, body_top], [-size * 0.5, body_top]];
    outline.extend(arc(0.0, body_top - size * 0.35, size * 0.5, std::f32::consts::PI, std::f32::consts::TAU, 16));
    let n = outline.len();
    let sharp: Vec<bool> = (0..n).map(|i| i <= 2 || i == n - 1).collect();
    let mut mesh = extrude_smooth(&outline, &sharp, size * 0.36);
    // The shackle: an arch from inside the body, up over the top, and back in.
    let shackle: Vec<[f32; 3]> = (0..=20)
        .map(|i| {
            let a = std::f32::consts::PI * i as f32 / 20.0;
            if i == 0 || i == 20 {
                [shackle_r * a.cos(), body_top - wire * 2.0, 0.0]
            } else {
                [shackle_r * a.cos(), -shackle_r + shackle_r * a.sin() - wire, 0.0]
            }
        })
        .collect();
    mesh.append(&tube(&shackle, 10, &SweepOptions::default(), |_| wire));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    #[test]
    fn hardware_is_drawable_and_stands_off_its_wall() {
        let (plate, recess) = lock_plate(0.6, 0.7, 0.05);
        for (name, mesh) in [("plate", plate), ("recess", recess), ("handle", drop_handle(0.5, 0.045)), ("hasp", hasp(0.2, 0.5, 0.04))] {
            let (lo, _) = mesh.bounds().unwrap();
            assert!(lo[2] >= -1e-4, "{name} goes behind its wall: {lo:?}");
            mesh.into_part("fitting", OutlineMode::Welded).validate().unwrap();
        }
        let lock = padlock(0.4);
        let (_, hi) = lock.bounds().unwrap();
        assert!(hi[1] < 0.03, "padlock reaches {hi:?} above its hanging point");
        lock.into_part("padlock", OutlineMode::Welded).validate().unwrap();
    }

    #[test]
    fn keyhole_is_a_simple_polygon_inside_its_size() {
        let k = keyhole_outline(1.0);
        assert!(k.len() > 10);
        assert!(k.iter().all(|p| p[0].abs() <= 0.5 && p[1].abs() <= 0.5 + 1e-5), "{k:?}");
    }
}
