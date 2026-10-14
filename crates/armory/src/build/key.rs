//! Keys: a bow to turn it by, a round shank, and a bit cut in steps to pass the wards of a
//! lock. Rings of keys today; a key in a chest's lock or on a gaoler's belt later.

use std::f32::consts::{FRAC_PI_2, TAU};

use serde::{Deserialize, Serialize};

use crate::geom::polygon::P2;
use crate::geom::{MeshBuilder, Shading, SlabOptions, bevelled_slab, cuboid, cylinder, torus, uv_sphere};

/// The shape of a key's bow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyBow {
    /// A plain ring of round bar.
    Ring,
    /// Three lobes round a hole, cast or filed.
    Trefoil,
    /// A pierced diamond.
    Lozenge,
}

impl KeyBow {
    pub const ALL: [KeyBow; 3] = [KeyBow::Ring, KeyBow::Trefoil, KeyBow::Lozenge];
}

fn circle(cx: f32, cy: f32, r: f32, n: usize) -> Vec<P2> {
    (0..n).map(|i| {
        let a = TAU * i as f32 / n as f32;
        [cx + a.cos() * r, cy + a.sin() * r]
    }).collect()
}

/// A key `length` long lying in the XY plane, thickness along Z: the top of its bow at the
/// origin, its shank down -Y, and its bit at the far end standing out along +X. `wards` are
/// the bit's steps, each a share (0..1) of the bit's full reach, from the tip up.
pub fn key(length: f32, bow: KeyBow, wards: &[f32]) -> MeshBuilder {
    let l = length;
    let wire = l * 0.035;
    let br = l * 0.14;
    let mut mesh = match bow {
        KeyBow::Ring => {
            let mut ring = torus(br, wire * 1.1, 36, 10, Shading::SMOOTH);
            ring.rotate_x(FRAC_PI_2).translate([0.0, -br - wire, 0.0]);
            ring
        }
        KeyBow::Trefoil => {
            // Three lobes round a middle that lies inside each: the union's rim, found along
            // rays from the middle (the farthest lobe edge on each ray).
            let lobe = br * 0.55;
            let mid = [0.0, -lobe * 1.8];
            let centres = [[0.0, lobe * 0.8], [-lobe * 0.85, -lobe * 0.45], [lobe * 0.85, -lobe * 0.45]];
            let outline: Vec<P2> = (0..48)
                .map(|i| {
                    let a = TAU * i as f32 / 48.0;
                    let d = [a.cos(), a.sin()];
                    let t = centres
                        .iter()
                        .map(|c| {
                            let dc = d[0] * c[0] + d[1] * c[1];
                            dc + (dc * dc - (c[0] * c[0] + c[1] * c[1]) + lobe * lobe).max(0.0).sqrt()
                        })
                        .fold(0.0f32, f32::max);
                    [mid[0] + d[0] * t, mid[1] + d[1] * t]
                })
                .collect();
            let hole = circle(0.0, -lobe * 1.8, lobe * 0.45, 16);
            bevelled_slab(&outline, &[hole.into_iter().rev().collect()], &SlabOptions::rounded(wire * 1.6, wire * 0.4))
        }
        KeyBow::Lozenge => {
            let (h, w) = (br * 2.1, br * 1.5);
            let outline: Vec<P2> = vec![[0.0, 0.0], [-w * 0.5, -h * 0.5], [0.0, -h], [w * 0.5, -h * 0.5]];
            let hole: Vec<P2> = circle(0.0, -h * 0.45, br * 0.32, 14).into_iter().rev().collect();
            bevelled_slab(&outline, &[hole], &SlabOptions::chamfered(wire * 1.6, wire * 0.35))
        }
    };
    let bow_bottom = match bow {
        KeyBow::Ring => -2.0 * br - wire,
        KeyBow::Trefoil => -br * 0.55 * 3.1,
        KeyBow::Lozenge => -br * 2.1,
    };
    let mut shank = cylinder(wire, l + bow_bottom + wire, 12, Shading::SMOOTH);
    shank.translate([0.0, -l, 0.0]);
    mesh.append(&shank);
    let mut collar = torus(wire * 1.15, wire * 0.45, 16, 6, Shading::SMOOTH);
    collar.translate([0.0, bow_bottom - wire * 1.5, 0.0]);
    mesh.append(&collar);
    let mut tip = uv_sphere(wire * 1.05, 10, 6);
    tip.translate([0.0, -l, 0.0]);
    mesh.append(&tip);
    let reach = l * 0.2;
    let step = l * 0.055;
    for (k, w) in wards.iter().enumerate() {
        let out = reach * (0.35 + 0.65 * w.clamp(0.0, 1.0));
        let mut bit = cuboid([out, step, wire * 1.4]);
        bit.translate([out * 0.5, -l + wire * 0.5 + step * (k as f32 + 0.5), 0.0]);
        mesh.append(&bit);
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    #[test]
    fn keys_are_drawable_and_as_long_as_asked() {
        for bow in KeyBow::ALL {
            let k = key(1.0, bow, &[0.2, 0.9, 0.5]);
            let (lo, hi) = k.bounds().unwrap();
            assert!(hi[1] < 0.06 && (lo[1] + 1.0).abs() < 0.05, "{bow:?}: {lo:?}..{hi:?}");
            k.into_part("key", OutlineMode::Welded).validate().unwrap();
        }
    }
}
