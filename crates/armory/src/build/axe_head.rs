//! Geometry for an [`AxeHead`]: the eye the haft passes through, the bit standing out of it
//! crosswise, and the poll behind it.
//!
//! Head space: y = 0 is the middle of the eye and the haft runs along Y; the bit reaches
//! toward +X, the poll toward -X, and the bit's cheeks face ±Z. A family places the meshes
//! on the haft with [`MeshBuilder::place`].
//!
//! The bit is a blade profile turned crosswise: its outline runs from the eye to a curved
//! cutting edge, rising to the toe and dropping to the heel or beard, and its thickness falls
//! away from the eye to a hard bevel line and on to the edge. Across its height it takes the
//! cross-section of a sword blade ([`crate::sections::blade::BladeSection`]: flat cheeks,
//! domed, ridged), eased so the top and bottom stay blunt. Nicks and a broken edge come from
//! the same [`BladeDamage`] swords use. The prototype's plate had none of this: its hand axe
//! was a flat extruded outline on a stick.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use procgen_rng::Mulberry32;

use super::blade::BladeDamage;
use super::hafted::{radius_on, smoothstep, turned};
use super::head::spike_mesh;
use crate::geom::{GridOptions, LatheOptions, MeshBuilder, Shading, V3, lathe, spline_profile, stitch_grid};
use crate::sections::axe_head::{AxeHead, AxePoll};
use crate::sections::socket::Socket;

/// How much wider than deep the eye is (eyes are oval, longer toward the edge).
const EYE_OVAL: f32 = 1.2;

/// An axe head built in head space.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AxeHeadMesh {
    /// The eye and its rings.
    pub eye: MeshBuilder,
    /// The bit, and the second bit of a double-bitted head.
    pub bit: MeshBuilder,
    /// A hammer or spike poll; empty otherwise.
    pub poll: MeshBuilder,
    /// The eye runs from `-eye_height / 2` to `eye_height / 2`.
    pub eye_height: f32,
    /// The eye's outer radius across the cheeks (along Z).
    pub eye_radius: f32,
    /// The farthest the edge reaches along +X.
    pub reach: f32,
}

/// A nick in the edge: `v` across the edge (0 heel, 1 toe), its depth in units and half
/// width in `v`.
#[derive(Clone, Copy, Debug)]
struct Nick {
    v: f32,
    depth: f32,
    width: f32,
}

/// The bit's outline and thickness as functions of `u` (0 at the eye, 1 at the edge) and `v`
/// (0 along the bottom, 1 along the top).
struct Bit<'a> {
    head: &'a AxeHead,
    /// Where the bit starts, inside the eye.
    x0: f32,
    /// Half thickness at the root.
    half: f32,
    /// The most the cheeks may swell to where they meet the eye.
    max_half: f32,
    /// Where the bevel begins, in `u`.
    bevel: f32,
    /// How far the edge bows out.
    bulge: f32,
    nicks: Vec<Nick>,
    /// Cap on the total depth of nicks at one point.
    max_nick: f32,
}

/// Half thickness at the very edge.
const EDGE_HALF: f32 = 0.004;

impl<'a> Bit<'a> {
    fn new(head: &'a AxeHead, x0: f32, max_half: f32, damage: &BladeDamage) -> Self {
        let length = (head.reach - x0).max(0.05);
        let mut rng = Mulberry32::new(damage.seed.wrapping_mul(89).wrapping_add(13));
        let n = damage.nicks.clamp(0.0, 1.0);
        let mut nicks: Vec<Nick> = (0..(n * 10.0).round() as usize)
            .map(|_| Nick {
                v: rng.range_f32(0.08, 0.92),
                depth: (0.2 + 0.8 * rng.next_f64() as f32) * n * 0.07,
                width: rng.range_f32(0.02, 0.05),
            })
            .collect();
        if damage.broken {
            // A chunk out of the edge.
            nicks.push(Nick { v: rng.range_f32(0.2, 0.8), depth: length * 0.3, width: 0.16 });
        }
        let mut bit = Bit {
            head,
            x0,
            half: (head.thickness * 0.5).min(max_half),
            max_half,
            bevel: 1.0 - head.bevel.clamp(0.04, 0.6),
            bulge: 0.0,
            nicks,
            max_nick: length * 0.45,
        };
        bit.bulge = (head.curve.max(0.0) * (bit.top(1.0) - bit.bottom(1.0))).min(length * 0.5);
        bit
    }

    /// How far a flank has run from the root height toward the edge corner at `u`.
    fn flank(&self, u: f32) -> f32 {
        u.powf(1.0 + 3.0 * self.head.waist.clamp(0.0, 1.0))
    }

    fn top(&self, u: f32) -> f32 {
        self.head.root * 0.5 + self.head.toe * self.flank(u) + self.head.droop * u * u
    }

    fn bottom(&self, u: f32) -> f32 {
        -self.head.root * 0.5 - self.head.beard * self.flank(u) + self.head.droop * u * u
    }

    fn nick(&self, v: f32) -> f32 {
        self.nicks.iter().map(|n| n.depth * (1.0 - (v - n.v).abs() / n.width).max(0.0)).sum::<f32>().min(self.max_nick)
    }

    /// The outline point at `(u, v)`.
    fn outline(&self, u: f32, v: f32) -> [f32; 2] {
        let edge = self.head.reach - self.bulge * (1.0 - (PI * v).sin());
        let x = self.x0 + (edge - self.x0) * u - self.nick(v) * smoothstep(self.bevel - 0.1, 1.0, u);
        [x, self.bottom(u) + (self.top(u) - self.bottom(u)) * v]
    }

    /// Half thickness at `(u, v)`: swelling into the eye, falling from the root to the bevel
    /// line, then to the edge; across the height, the blade section eased so the top and
    /// bottom stay blunt.
    fn half_thickness(&self, u: f32, v: f32) -> f32 {
        let along = if u <= self.bevel {
            let swell = (1.0 - u / 0.2).max(0.0);
            (self.half * (1.0 - 0.55 * u / self.bevel) * (1.0 + 0.6 * swell * swell)).min(self.max_half)
        } else {
            let k = (u - self.bevel) / (1.0 - self.bevel);
            self.half * 0.45 + (EDGE_HALF - self.half * 0.45) * k
        };
        along * (0.5 + 0.5 * self.head.section.thickness_at(2.0 * v - 1.0))
    }

    fn point(&self, u: f32, v: f32, side: f32) -> V3 {
        let [x, y] = self.outline(u, v);
        [x, y, side * self.half_thickness(u, v)]
    }

    fn mesh(&self) -> MeshBuilder {
        // Rows run from the eye to the edge, with a hard line where the bevel starts.
        let n_u = 36;
        let mut us: Vec<(f32, bool)> = (0..=n_u).map(|i| (i as f32 / n_u as f32, false)).collect();
        us.push((self.bevel, true));
        us.sort_by(|a, b| a.0.total_cmp(&b.0));
        us.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-4 && {
            b.1 |= a.1;
            true
        });
        // Columns run bottom to top, with hard lines at the section's creases.
        let n_v = if self.nicks.is_empty() { 24 } else { 48 };
        let creases: Vec<f32> =
            self.head.section.creases().iter().map(|c| (c + 1.0) * 0.5).filter(|&v| v > 1e-3 && v < 1.0 - 1e-3).collect();
        let mut vs: Vec<(f32, bool)> = (0..=n_v).map(|i| (i as f32 / n_v as f32, false)).collect();
        vs.extend(creases.iter().map(|&v| (v, true)));
        vs.sort_by(|a, b| a.0.total_cmp(&b.0));
        vs.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-4 && {
            b.1 |= a.1;
            true
        });

        let smooth = GridOptions { shading: Shading::SMOOTH, ..Default::default() };
        let mut mesh = MeshBuilder::new();
        let breaks = |list: &[(f32, bool)]| -> Vec<usize> {
            let mut b: Vec<usize> = vec![0];
            b.extend((1..list.len() - 1).filter(|&i| list[i].1));
            b.push(list.len() - 1);
            b
        };
        let (row_breaks, col_breaks) = (breaks(&us), breaks(&vs));
        for side in [1.0f32, -1.0] {
            for rw in row_breaks.windows(2) {
                for cw in col_breaks.windows(2) {
                    let grid: Vec<Vec<V3>> = us[rw[0]..=rw[1]]
                        .iter()
                        .map(|&(u, _)| {
                            let mut line: Vec<V3> = vs[cw[0]..=cw[1]].iter().map(|&(v, _)| self.point(u, v, side)).collect();
                            // The front (+Z) runs top to bottom so it faces +Z; the back the other way.
                            if side > 0.0 {
                                line.reverse();
                            }
                            line
                        })
                        .collect();
                    mesh.append(&stitch_grid(&grid, &smooth));
                }
            }
        }
        // Walls: the top and bottom (smooth along the bit), the edge and the root.
        let walls = GridOptions { shading: Shading::FACETED, ..Default::default() };
        let along = |v: f32, first: f32| -> Vec<Vec<V3>> { us.iter().map(|&(u, _)| vec![self.point(u, v, first), self.point(u, v, -first)]).collect() };
        mesh.append(&stitch_grid(&along(0.0, 1.0), &walls));
        mesh.append(&stitch_grid(&along(1.0, -1.0), &walls));
        let across = |u: f32, first: f32| -> Vec<Vec<V3>> { vs.iter().map(|&(v, _)| vec![self.point(u, v, first), self.point(u, v, -first)]).collect() };
        mesh.append(&stitch_grid(&across(1.0, 1.0), &walls));
        mesh.append(&stitch_grid(&across(0.0, -1.0), &walls));
        mesh
    }
}

/// Builds `head` through `eye` round a haft of radius `haft_radius`, with `damage` to the
/// edge. The bit's root is buried in the eye, and the cheeks are never thicker than the eye.
pub fn axe_head_mesh(head: &AxeHead, eye: &Socket, haft_radius: f32, damage: &BladeDamage) -> AxeHeadMesh {
    let he = eye.length.max(0.1) * 0.5;
    let re = haft_radius * eye.flare.max(1.15) + 0.01;
    let (sides, shading) = turned(eye.facets);

    // The eye: a short sleeve, waisted a little, eased at both ends; oval, longer toward the edge.
    let control = [[re * 0.88, -he], [re * 0.99, -he * 0.82], [re * 0.95, 0.0], [re * 0.99, he * 0.82], [re * 0.88, he]];
    let profile = spline_profile(&control, 24);
    let mut eye_mesh = lathe(&profile, &LatheOptions { facets: sides, shading, cap_start: true, cap_end: true, phase: 0.0 });
    let n = eye.rings;
    for k in 0..n {
        let y = if n == 1 { -he * 0.7 } else { -he * 0.78 + he * 1.56 * k as f32 / (n - 1) as f32 };
        let r = radius_on(&profile, y);
        let b = (he * 0.08).clamp(0.015, 0.035);
        let ring = [[r * 0.98, -b], [r + b * 0.6, -b * 0.55], [r + b * 0.75, 0.0], [r + b * 0.6, b * 0.55], [r * 0.98, b]];
        let options = LatheOptions { facets: sides, shading: Shading { smooth_around: shading.smooth_around, smooth_along: true }, ..Default::default() };
        let mut band = lathe(&spline_profile(&ring, 10), &options);
        band.translate([0.0, y, 0.0]);
        eye_mesh.append(&band);
    }
    eye_mesh.scale([EYE_OVAL, 1.0, 1.0]);

    // The bit: rooted halfway into the eye, no taller than the eye at the root.
    let x0 = re * EYE_OVAL * 0.5;
    let sized = AxeHead { root: head.root.min(he * 2.0), reach: head.reach.max(re * EYE_OVAL + 0.15), ..*head };
    let bit = Bit::new(&sized, x0, re * 0.82, damage);
    let mut bits = bit.mesh();
    let mut poll = MeshBuilder::new();
    match head.poll {
        AxePoll::Plain => {}
        AxePoll::Bit => {
            // The second bit is the first mirrored, undamaged.
            let mut back = Bit::new(&sized, x0, re * 0.82, &BladeDamage::NONE).mesh();
            back.scale([-1.0, 1.0, 1.0]);
            bits.append(&back);
        }
        AxePoll::Hammer => {
            // A squared block out of the back of the eye, swelling to a flat face.
            let ph = (he * 0.85).min(re * 1.05);
            let (y0, l) = (re * 0.5, re * EYE_OVAL * 0.5 + head.poll_length);
            let control = [[ph * 0.85, y0], [ph * 0.78, y0 + l * 0.45], [ph, y0 + l * 0.9], [ph * 0.94, y0 + l]];
            let options = LatheOptions { facets: 4, shading: Shading::FACETED, cap_start: false, cap_end: true, phase: FRAC_PI_4 };
            let mut block = lathe(&spline_profile(&control, 12), &options);
            block.rotate_z(FRAC_PI_2);
            poll.append(&block);
        }
        AxePoll::Spike => {
            // A four-sided spike out of the back of the eye, dipping a little.
            let base = he.min(re) * 0.8;
            let mut spike = spike_mesh(base, re * EYE_OVAL * 0.5 + head.poll_length, 4, 0.35);
            spike.translate([0.0, re * 0.5, 0.0]).rotate_z(FRAC_PI_2 + 0.12);
            poll.append(&spike);
        }
    }
    AxeHeadMesh { eye: eye_mesh, bit: bits, poll, eye_height: he * 2.0, eye_radius: re, reach: sized.reach }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;
    use crate::sections::blade::BladeSection;

    /// Signed volume of a closed mesh: positive when its faces point outward.
    fn volume(m: &MeshBuilder) -> f32 {
        m.indices.as_chunks::<3>().0.iter().map(|t| {
            let [a, b, c] = t.map(|i| m.positions[i as usize]);
            crate::geom::vec3::dot(a, crate::geom::vec3::cross(b, c)) / 6.0
        }).sum()
    }

    #[test]
    fn every_section_and_poll_builds_outward_facing() {
        let eye = Socket { length: 0.5, flare: 1.4, rings: 2, facets: 8, ..Socket::default() };
        for section in BladeSection::ALL {
            for poll in AxePoll::ALL {
                for damage in [BladeDamage::NONE, BladeDamage { nicks: 0.8, broken: true, seed: 5 }] {
                    let head = AxeHead { section, poll, beard: 0.6, toe: 0.3, curve: 0.3, waist: 0.7, ..AxeHead::default() };
                    let built = axe_head_mesh(&head, &eye, 0.15, &damage);
                    let v = volume(&built.bit);
                    assert!(v > 0.0, "{section:?} {poll:?}: bit volume {v}");
                    let (lo, hi) = built.bit.bounds().unwrap();
                    assert!(hi[0] <= head.reach + 1e-3 && hi[0] > head.reach * 0.6, "{section:?}: reaches {}", hi[0]);
                    assert!(hi[2] - lo[2] <= head.thickness * 1.6 + 1e-3, "swells past the eye");
                    if poll == AxePoll::Bit {
                        assert!(lo[0] < -head.reach * 0.6, "no second bit");
                    }
                    for (name, mesh) in [("eye", built.eye), ("bit", built.bit), ("poll", built.poll)] {
                        if !mesh.is_empty() {
                            mesh.into_part(name, OutlineMode::Welded).validate().unwrap();
                        }
                    }
                }
            }
        }
    }
}
