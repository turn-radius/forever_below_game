//! Plaque outlines: signet faces, pendants, badges. From the prototype's `pendantPts` and
//! `pendantHoles` (`reference/spline_armory_engine.js`). The prototype extruded them with
//! three.js bevels that faceted every curve; families make them
//! [`crate::geom::bevelled_slab`]s with smooth walls. Devices are in
//! [`crate::build::device`].

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::geom::polygon::P2;
use crate::build::device::crescent;
use crate::geom::CatmullRom;
use crate::sections::emblem::PlaqueShape;

/// One closed outline and the holes cut through it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape2 {
    pub outline: Vec<P2>,
    pub holes: Vec<Vec<P2>>,
}

impl Shape2 {
    /// Scaled about the origin.
    pub fn scaled(&self, sx: f32, sy: f32) -> Shape2 {
        let f = |v: &Vec<P2>| v.iter().map(|p| [p[0] * sx, p[1] * sy]).collect::<Vec<P2>>();
        Shape2 { outline: f(&self.outline), holes: self.holes.iter().map(f).collect() }
    }
}

/// `n + 1` points on an arc of radius `r` about `(cx, cy)` from angle `a0` to `a1`.
pub fn arc_points(cx: f32, cy: f32, r: f32, a0: f32, a1: f32, n: usize) -> Vec<P2> {
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            [cx + a.cos() * r, cy + a.sin() * r]
        })
        .collect()
}

/// `n` points round a circle, starting at angle `phase`.
pub fn circle_points(cx: f32, cy: f32, r: f32, n: usize, phase: f32) -> Vec<P2> {
    let mut pts = arc_points(cx, cy, r, phase, phase + TAU, n);
    pts.pop();
    pts
}

fn rotated(points: &[P2], angle: f32) -> Vec<P2> {
    let (s, c) = angle.sin_cos();
    points.iter().map(|&[x, y]| [x * c - y * s, x * s + y * c]).collect()
}

/// A plaque's outline at unit height (y from -0.5 to 0.5), centred on the origin.
pub fn plaque_outline(shape: PlaqueShape) -> Vec<P2> {
    match shape {
        PlaqueShape::Disc => circle_points(0.0, 0.0, 0.5, 72, 0.0),
        PlaqueShape::Oval => circle_points(0.0, 0.0, 1.0, 72, 0.0).into_iter().map(|[x, y]| [x * 0.36, y * 0.5]).collect(),
        PlaqueShape::Octagon => circle_points(0.0, 0.0, 0.52, 8, PI / 8.0),
        PlaqueShape::Shield => {
            let half: Vec<P2> = CatmullRom::centripetal(vec![[0.38, 0.42, 0.0], [0.37, 0.08, 0.0], [0.24, -0.26, 0.0], [0.0, -0.5, 0.0]])
                .sample(8)
                .into_iter()
                .map(|p| [p[0], p[1]])
                .collect();
            let mut pts = vec![[-0.38, 0.46], [0.38, 0.46]];
            pts.extend_from_slice(&half);
            pts.extend(half[..half.len() - 1].iter().rev().map(|&[x, y]| [-x, y]));
            pts
        }
        PlaqueShape::Teardrop => (0..72)
            .map(|i| {
                let t = i as f32 / 72.0 * TAU;
                [0.42 * t.sin() * (t / 2.0).sin().abs().powf(1.3), 0.5 * t.cos()]
            })
            .collect(),
        PlaqueShape::Quatrefoil => (0..96)
            .map(|i| {
                let a = i as f32 / 96.0 * TAU;
                let r = 0.33 + 0.17 * (2.0 * a).cos().abs().sqrt();
                [a.cos() * r, a.sin() * r]
            })
            .collect(),
        PlaqueShape::Star => (0..16)
            .map(|i| {
                let a = FRAC_PI_2 + i as f32 * PI / 8.0;
                let r = if i % 2 == 1 { 0.3 } else { 0.5 };
                [a.cos() * r, a.sin() * r]
            })
            .collect(),
        PlaqueShape::Cross => vec![
            [-0.11, -0.5], [0.11, -0.5], [0.08, 0.1], [0.32, 0.06], [0.32, 0.3], [0.08, 0.26], [0.11, 0.5], [-0.11, 0.5],
            [-0.08, 0.26], [-0.32, 0.3], [-0.32, 0.06], [-0.08, 0.1],
        ],
        PlaqueShape::Crescent => rotated(&crescent(), -FRAC_PI_2).into_iter().map(|[x, y]| [x * 0.55, y * 0.55]).collect(),
        PlaqueShape::Lozenge => vec![[0.0, 0.5], [0.32, 0.0], [0.0, -0.5], [-0.32, 0.0]],
        PlaqueShape::Ankh => {
            let a0 = ((0.08f32 - 0.28) / 0.22).asin();
            let xj = 0.19 * a0.cos();
            let lp: Vec<P2> = arc_points(0.0, 0.0, 1.0, a0, PI - a0, 40).into_iter().map(|[x, y]| [x * 0.19, 0.28 + y * 0.22]).collect();
            let mut pts = vec![[0.15, -0.5], [0.07, -0.04], [0.3, -0.04], [0.35, -0.09], [0.35, 0.13], [0.3, 0.08], [xj, 0.08]];
            pts.extend_from_slice(&lp[1..lp.len() - 1]);
            pts.extend_from_slice(&[[-xj, 0.08], [-0.3, 0.08], [-0.35, 0.13], [-0.35, -0.09], [-0.3, -0.04], [-0.07, -0.04], [-0.15, -0.5]]);
            pts
        }
    }
}

/// Holes cut through a plaque at unit height: the eye of an ankh.
pub fn plaque_holes(shape: PlaqueShape) -> Vec<Vec<P2>> {
    match shape {
        PlaqueShape::Ankh => vec![circle_points(0.0, 0.0, 1.0, 48, 0.0).into_iter().map(|[x, y]| [x * 0.118, 0.285 + y * 0.15]).collect()],
        _ => Vec::new(),
    }
}

/// A plaque's outline and holes at unit height.
pub fn plaque_shape(shape: PlaqueShape) -> Shape2 {
    Shape2 { outline: plaque_outline(shape), holes: plaque_holes(shape) }
}

/// Where a plaque's centre piece (a stone or a device) sits, as a fraction of its height
/// from the middle, and how big it may be as a fraction of the height.
pub fn plaque_centre(shape: PlaqueShape) -> (f32, f32) {
    match shape {
        PlaqueShape::Crescent => (0.385, 0.22),
        PlaqueShape::Cross => (0.18, 0.3),
        PlaqueShape::Ankh => (0.02, 0.16),
        PlaqueShape::Disc | PlaqueShape::Octagon => (0.0, 0.62),
        PlaqueShape::Oval | PlaqueShape::Shield => (0.0, 0.55),
        PlaqueShape::Teardrop => (-0.08, 0.45),
        PlaqueShape::Quatrefoil => (0.0, 0.5),
        PlaqueShape::Star | PlaqueShape::Lozenge => (0.0, 0.42),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{MeshBuilder, SlabOptions, bevelled_slab, signed_area};

    #[test]
    fn plaques_fit_unit_height_and_have_area() {
        for shape in PlaqueShape::ALL {
            let pts = plaque_outline(shape);
            let (lo, hi) = pts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
            assert!(lo >= -0.56 && hi <= 0.56 && hi - lo > 0.6, "{shape:?}: {lo}..{hi}");
            assert!(signed_area(&pts).abs() > 0.05, "{shape:?}");
        }
    }

    /// The area of a slab's front face (triangles at its greatest z, facing +Z).
    fn front_area(mesh: &MeshBuilder) -> f32 {
        let z = mesh.positions.iter().fold(f32::MIN, |m, p| m.max(p[2]));
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| t.map(|i| mesh.positions[i as usize]))
            .filter(|t| t.iter().all(|p| (p[2] - z).abs() < 1e-6))
            .map(|[a, b, c]| ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) * 0.5)
            .sum()
    }

    #[test]
    fn plaque_slabs_keep_their_whole_face() {
        for shape in PlaqueShape::ALL {
            let s = plaque_shape(shape);
            let holes: f32 = s.holes.iter().map(|h| signed_area(h).abs()).sum();
            let area = signed_area(&s.outline).abs() - holes;
            let perimeter: f32 = std::iter::once(&s.outline)
                .chain(s.holes.iter())
                .map(|r| (0..r.len()).map(|i| (r[(i + 1) % r.len()][0] - r[i][0]).hypot(r[(i + 1) % r.len()][1] - r[i][1])).sum::<f32>())
                .sum();
            let bevel = 0.02;
            // The face is the outline inset by the bevel: about the area less a perimeter strip.
            let expected = area - perimeter * bevel;
            for options in [SlabOptions::rounded(0.08, bevel), SlabOptions::chamfered(0.08, bevel)] {
                let face = front_area(&bevelled_slab(&s.outline, &s.holes, &options));
                assert!((face - expected).abs() < area * 0.06, "{shape:?}: face {face}, expected about {expected} of {area}");
            }
        }
    }
}
