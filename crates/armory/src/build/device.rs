//! Device outlines and their raised or painted shapes (`devicePts` and `deviceGeo` in
//! `reference/spline_armory_engine.js`). A crate's stencil today; a shield's charge, a
//! signet's engraving and a medallion's relief can use the same outlines.

use procgen_rng::Mulberry32;

use crate::geom::{MeshBuilder, P2, extrude};
use crate::sections::device::Device;

/// One filled shape of a device: an outline and the holes cut from it.
pub type DeviceShape = (Vec<P2>, Vec<Vec<P2>>);

fn arc(cx: f32, cy: f32, r: f32, a0: f32, a1: f32, n: usize) -> Vec<P2> {
    (0..=n).map(|i| {
        let a = a0 + (a1 - a0) * i as f32 / n as f32;
        [cx + a.cos() * r, cy + a.sin() * r]
    }).collect()
}

fn circle(r: f32, n: usize) -> Vec<P2> {
    let mut c = arc(0.0, 0.0, r, 0.0, std::f32::consts::TAU, n);
    c.pop();
    c
}

fn rotated_rect(cx: f32, cy: f32, w: f32, h: f32, angle: f32) -> Vec<P2> {
    let (s, c) = angle.sin_cos();
    [[-w / 2.0, -h / 2.0], [w / 2.0, -h / 2.0], [w / 2.0, h / 2.0], [-w / 2.0, h / 2.0]]
        .iter()
        .map(|&[x, y]| [cx + x * c - y * s, cy + x * s + y * c])
        .collect()
}

/// A crescent opening to the right, unit outer radius (the prototype's `crescentPts`).
fn crescent() -> Vec<P2> {
    let outer = arc(0.0, 0.0, 1.0, 0.863, std::f32::consts::TAU - 0.863, 30);
    let inner = arc(0.4, 0.0, 0.8, std::f32::consts::TAU - 1.253, 1.253, 24);
    outer.into_iter().chain(inner[1..inner.len() - 1].iter().copied()).collect()
}

/// The device's shapes at unit size: each fits a circle of radius 0.5 about the origin.
/// Runes differ per `seed`; the rest ignore it.
pub fn device_shapes(device: Device, seed: u32) -> Vec<DeviceShape> {
    match device {
        Device::None => Vec::new(),
        Device::Cross => {
            let (a, b, r) = (0.07, 0.17, 0.46);
            vec![(vec![[a, a], [r, b], [r, -b], [a, -a], [b, -r], [-b, -r], [-a, -a], [-r, -b], [-r, b], [-a, a], [-b, r], [b, r]], vec![])]
        }
        Device::Star => {
            let points = (0..12)
                .map(|i| {
                    let a = std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 6.0;
                    let r = if i % 2 == 1 { 0.2 } else { 0.48 };
                    [a.cos() * r, a.sin() * r]
                })
                .collect();
            vec![(points, vec![])]
        }
        Device::Crescent => {
            // Turned to open upward.
            let points = crescent().into_iter().map(|[x, y]| [-y * 0.46, x * 0.46 + 0.06]).collect();
            vec![(points, vec![])]
        }
        Device::Tower => vec![(
            vec![
                [-0.27, -0.45], [0.27, -0.45], [0.27, 0.42], [0.15, 0.42], [0.15, 0.3], [0.06, 0.3], [0.06, 0.42], [-0.06, 0.42],
                [-0.06, 0.3], [-0.15, 0.3], [-0.15, 0.42], [-0.27, 0.42],
            ],
            vec![vec![[-0.07, -0.38], [0.07, -0.38], [0.07, -0.18], [0.0, -0.11], [-0.07, -0.18]]],
        )],
        Device::Wheel => {
            let mut out = vec![(circle(0.46, 40), vec![circle(0.36, 40)]), (circle(0.1, 20), vec![])];
            for k in 0..3 {
                out.push((rotated_rect(0.0, 0.0, 0.06, 0.76, k as f32 * std::f32::consts::PI / 3.0), vec![]));
            }
            out
        }
        Device::Rune => {
            let mut rng = Mulberry32::new(seed);
            let mut out = vec![(rotated_rect(0.0, 0.0, 0.09, 0.92, 0.0), vec![])];
            for _ in 0..rng.int(2, 3) {
                let y = 0.36 - rng.range_f32(0.0, 0.62);
                let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
                let up = if rng.chance(0.5) { 1.0 } else { -1.0 };
                let len = rng.range_f32(0.3, 0.44);
                out.push((rotated_rect(side * len * 0.35, y + up * len * 0.35, 0.08, len, (-side).atan2(up)), vec![]));
            }
            out
        }
    }
}

/// The device `size` across, raised `depth` off z = 0 toward +Z (its back on z = 0). Empty
/// for [`Device::None`].
pub fn device_mesh(device: Device, size: f32, depth: f32, seed: u32) -> MeshBuilder {
    let mut mesh = MeshBuilder::new();
    for (outline, holes) in device_shapes(device, seed) {
        let scale = |p: &[f32; 2]| [p[0] * size, p[1] * size];
        let outline: Vec<P2> = outline.iter().map(scale).collect();
        let holes: Vec<Vec<P2>> = holes.iter().map(|h| h.iter().map(scale).collect()).collect();
        let mut slab = extrude(&outline, &holes, depth);
        slab.translate([0.0, 0.0, depth * 0.5]);
        mesh.append(&slab);
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    #[test]
    fn every_device_fits_its_circle_and_is_drawable() {
        for device in Device::ALL {
            let mesh = device_mesh(device, 2.0, 0.05, 7);
            assert_eq!(mesh.is_empty(), device == Device::None, "{device:?}");
            if let Some((lo, hi)) = mesh.bounds() {
                for k in 0..2 {
                    assert!(lo[k] >= -1.0 && hi[k] <= 1.0, "{device:?} leaves its circle: {lo:?} {hi:?}");
                }
                assert!(lo[2] >= -1e-5 && hi[2] <= 0.05 + 1e-5);
                mesh.into_part("device", OutlineMode::Welded).validate().unwrap();
            }
        }
    }
}
