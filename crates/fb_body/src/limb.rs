//! Arms and legs: a centre line swept through Catmull-Rom joints with a tapering radius
//! (the plate prototype's `mkArm` and `mkLeg`).

use armory::geom::vec3::{self, V3};
use armory::geom::{CatmullRom, MeshBuilder, SweepOptions, tube_varying};

/// Which side of the body: the wearer's left is +X.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub const BOTH: [Side; 2] = [Side::Right, Side::Left];

    /// +1 for the left (+X), -1 for the right.
    pub fn sign(self) -> f32 {
        match self {
            Side::Left => 1.0,
            Side::Right => -1.0,
        }
    }
}

/// One limb's centre line and girth.
#[derive(Clone, Debug, PartialEq)]
pub struct Limb {
    pub side: Side,
    /// Where the line starts, inside the trunk, so a sleeve or a hem never shows a gap.
    pub root: V3,
    /// Shoulder, elbow and wrist, or hip, knee and ankle.
    pub joints: [V3; 3],
    /// The centre line sampled evenly by length, root to the last joint.
    pub path: Vec<V3>,
    /// Share of the length at the first and second joints.
    pub t_joints: [f32; 2],
    /// Radius at the first joint; `knots` scale it along the length.
    pub radius: f32,
    /// `(t, multiple of radius)`, rising in t from 0 to 1.
    pub knots: Vec<(f32, f32)>,
}

impl Limb {
    /// A limb through `root` and `joints`, its radius `radius` times the profile given by
    /// `profile(t_first_joint, t_second_joint)`.
    pub fn new(side: Side, root: V3, joints: [V3; 3], controls: Vec<V3>, radius: f32, profile: impl Fn(f32, f32) -> Vec<(f32, f32)>) -> Limb {
        let path = CatmullRom::centripetal(controls).sample_evenly(64);
        let lengths: Vec<f32> = std::iter::once(0.0)
            .chain(path.windows(2).scan(0.0, |acc, w| {
                *acc += vec3::distance(w[0], w[1]);
                Some(*acc)
            }))
            .collect();
        let total = lengths.last().copied().unwrap_or(1.0).max(1e-4);
        let t_at = |p: V3| {
            let i = (0..path.len()).min_by(|&a, &b| vec3::distance(path[a], p).total_cmp(&vec3::distance(path[b], p))).unwrap_or(0);
            lengths[i] / total
        };
        let t_joints = [t_at(joints[0]), t_at(joints[1])];
        Limb { side, root, joints, path, t_joints, radius, knots: profile(t_joints[0], t_joints[1]) }
    }

    /// Length of the centre line.
    pub fn length(&self) -> f32 {
        self.path.windows(2).map(|w| vec3::distance(w[0], w[1])).sum()
    }

    /// Radius at `t` (0 at the root, 1 at the last joint), smooth through the knots.
    pub fn radius_at(&self, t: f32) -> f32 {
        self.radius * smooth_knots(&self.knots, t)
    }

    /// The point a share `t` of the way along by length.
    pub fn point_at(&self, t: f32) -> V3 {
        let n = self.path.len() - 1;
        let f = t.clamp(0.0, 1.0) * n as f32;
        let i = (f.floor() as usize).min(n - 1);
        vec3::lerp(self.path[i], self.path[i + 1], f - i as f32)
    }

    /// The direction of the line at `t`.
    pub fn tangent_at(&self, t: f32) -> V3 {
        let d = 0.01;
        vec3::normalize_or(vec3::sub(self.point_at(t + d), self.point_at(t - d)), vec3::neg(vec3::Y))
    }

    /// The limb's skin as a closed tube. `radius(t, angle)` gives the radius at `t` along
    /// and `angle` round from `up` at the root, or from `end_up` at the far end when given
    /// (the frames are carried from the end inward so the section lines up with the hand).
    pub fn mesh(&self, sides: usize, start_up: V3, end_up: Option<V3>, radius: impl Fn(f32, f32) -> f32) -> MeshBuilder {
        let options = |up: V3| SweepOptions { up: Some(up), ..SweepOptions::default() };
        match end_up {
            Some(up) => {
                let path: Vec<V3> = self.path.iter().rev().copied().collect();
                tube_varying(&path, sides, &options(up), |t, a| radius(1.0 - t, a))
            }
            None => tube_varying(&self.path, sides, &options(start_up), radius),
        }
    }
}

/// Cubic Hermite interpolation through `(t, value)` knots with Catmull-Rom slopes, clamped
/// at both ends.
pub fn smooth_knots(knots: &[(f32, f32)], t: f32) -> f32 {
    let n = knots.len();
    if n == 0 {
        return 1.0;
    }
    if t <= knots[0].0 {
        return knots[0].1;
    }
    if t >= knots[n - 1].0 {
        return knots[n - 1].1;
    }
    let i = knots.partition_point(|k| k.0 <= t).clamp(1, n - 1) - 1;
    let (k0, k1) = (knots[i], knots[i + 1]);
    let h = (k1.0 - k0.0).max(1e-5);
    let s = (t - k0.0) / h;
    let slope = |j: usize| {
        let (a, b) = (knots[j.saturating_sub(1)], knots[(j + 1).min(n - 1)]);
        (b.1 - a.1) / (b.0 - a.0).max(1e-5)
    };
    let (m0, m1) = (slope(i) * h, slope(i + 1) * h);
    let (s2, s3) = (s * s, s * s * s);
    (2.0 * s3 - 3.0 * s2 + 1.0) * k0.1 + (s3 - 2.0 * s2 + s) * m0 + (-2.0 * s3 + 3.0 * s2) * k1.1 + (s3 - s2) * m1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knots_pass_through_their_values() {
        let k = [(0.0, 1.0), (0.3, 2.0), (1.0, 0.5)];
        for (t, v) in k {
            assert!((smooth_knots(&k, t) - v).abs() < 1e-5);
        }
        assert_eq!(smooth_knots(&k, -1.0), 1.0);
        assert_eq!(smooth_knots(&k, 2.0), 0.5);
    }
}
