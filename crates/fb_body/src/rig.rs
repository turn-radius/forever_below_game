//! A folk's rig: sixteen joints, then a tail's four bones, with their rest frames, taken from the body's own limb joints,
//! and the maths to pose it ([`RigPose`]) and to skin points with it. Plain Rust, no Bevy.
//!
//! Every joint rests with an identity rotation, so a joint's frame is body space moved to its
//! origin: a rotation about +X is a swing in the vertical plane (negative swings a hanging limb
//! forward), about +Y a turn, about +Z a roll. A pose is a local rotation per joint and a local
//! offset per joint (the pelvis's carries the bob), applied in the order of [`Joint::ALL`],
//! which lists parents before children.
//!
//! The skin and every worn piece are weighted to these joints ([`crate::skin`]); held items ride
//! a joint rigidly.

use armory::geom::vec3::{self, Mat3, V3};

use crate::body::Body;
use crate::limb::Side;

/// How many joints a folk's rig has: sixteen of the body, then [`TAIL_JOINTS`] of a jointed tail
/// (a race without one has them at rest at the pelvis, and nothing is weighted to them).
pub const JOINTS: usize = 16 + TAIL_JOINTS;

/// The bones a jointed tail has at most.
pub const TAIL_JOINTS: usize = crate::parts::TAIL_BONES;

/// A joint of the rig. Right then left, as [`Side::BOTH`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Joint {
    /// The root, at the hips' centre: the body's bob, turn and roll.
    Pelvis,
    /// The waist.
    Spine,
    /// Mid-chest: the shoulders hang from it.
    Chest,
    /// The base of the neck, where the head turns (`Figure::neck`).
    Head,
    ShoulderR,
    ElbowR,
    WristR,
    HipR,
    KneeR,
    AnkleR,
    ShoulderL,
    ElbowL,
    WristL,
    HipL,
    KneeL,
    AnkleL,
    /// A jointed tail's bones, root first; each hangs from the one before and the first from the pelvis.
    Tail0,
    Tail1,
    Tail2,
    Tail3,
}

impl Joint {
    /// Parents before children.
    pub const ALL: [Joint; JOINTS] = [
        Joint::Pelvis,
        Joint::Spine,
        Joint::Chest,
        Joint::Head,
        Joint::ShoulderR,
        Joint::ElbowR,
        Joint::WristR,
        Joint::HipR,
        Joint::KneeR,
        Joint::AnkleR,
        Joint::ShoulderL,
        Joint::ElbowL,
        Joint::WristL,
        Joint::HipL,
        Joint::KneeL,
        Joint::AnkleL,
        Joint::Tail0,
        Joint::Tail1,
        Joint::Tail2,
        Joint::Tail3,
    ];

    /// The tail's bones, root first.
    pub const TAIL: [Joint; TAIL_JOINTS] = [Joint::Tail0, Joint::Tail1, Joint::Tail2, Joint::Tail3];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Joint::Pelvis => "pelvis",
            Joint::Spine => "spine",
            Joint::Chest => "chest",
            Joint::Head => "head",
            Joint::ShoulderR => "shoulder_r",
            Joint::ElbowR => "elbow_r",
            Joint::WristR => "wrist_r",
            Joint::HipR => "hip_r",
            Joint::KneeR => "knee_r",
            Joint::AnkleR => "ankle_r",
            Joint::ShoulderL => "shoulder_l",
            Joint::ElbowL => "elbow_l",
            Joint::WristL => "wrist_l",
            Joint::HipL => "hip_l",
            Joint::KneeL => "knee_l",
            Joint::AnkleL => "ankle_l",
            Joint::Tail0 => "tail_0",
            Joint::Tail1 => "tail_1",
            Joint::Tail2 => "tail_2",
            Joint::Tail3 => "tail_3",
        }
    }

    pub fn parent(self) -> Option<Joint> {
        Some(match self {
            Joint::Pelvis => return None,
            Joint::Spine | Joint::HipR | Joint::HipL => Joint::Pelvis,
            Joint::Chest => Joint::Spine,
            Joint::Head | Joint::ShoulderR | Joint::ShoulderL => Joint::Chest,
            Joint::ElbowR => Joint::ShoulderR,
            Joint::WristR => Joint::ElbowR,
            Joint::ElbowL => Joint::ShoulderL,
            Joint::WristL => Joint::ElbowL,
            Joint::KneeR => Joint::HipR,
            Joint::AnkleR => Joint::KneeR,
            Joint::KneeL => Joint::HipL,
            Joint::AnkleL => Joint::KneeL,
            Joint::Tail0 => Joint::Pelvis,
            Joint::Tail1 => Joint::Tail0,
            Joint::Tail2 => Joint::Tail1,
            Joint::Tail3 => Joint::Tail2,
        })
    }

    /// Shoulder, elbow, wrist of `side`'s arm.
    pub fn arm(side: Side) -> [Joint; 3] {
        match side {
            Side::Right => [Joint::ShoulderR, Joint::ElbowR, Joint::WristR],
            Side::Left => [Joint::ShoulderL, Joint::ElbowL, Joint::WristL],
        }
    }

    /// Hip, knee, ankle of `side`'s leg.
    pub fn leg(side: Side) -> [Joint; 3] {
        match side {
            Side::Right => [Joint::HipR, Joint::KneeR, Joint::AnkleR],
            Side::Left => [Joint::HipL, Joint::KneeL, Joint::AnkleL],
        }
    }

    /// The wrist a hand and what it holds ride on.
    pub fn wrist(side: Side) -> Joint {
        Joint::arm(side)[2]
    }
}

/// A rotation as a unit quaternion, `[x, y, z, w]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat(pub [f32; 4]);

impl Quat {
    pub const IDENTITY: Quat = Quat([0.0, 0.0, 0.0, 1.0]);

    pub fn from_axis_angle(axis: V3, angle: f32) -> Quat {
        let a = vec3::normalize_or(axis, vec3::Y);
        let (s, c) = (0.5 * angle).sin_cos();
        Quat([a[0] * s, a[1] * s, a[2] * s, c])
    }

    pub fn x(angle: f32) -> Quat {
        Quat::from_axis_angle(vec3::X, angle)
    }

    pub fn y(angle: f32) -> Quat {
        Quat::from_axis_angle(vec3::Y, angle)
    }

    pub fn z(angle: f32) -> Quat {
        Quat::from_axis_angle(vec3::Z, angle)
    }

    /// `self` applied after `other`.
    #[allow(clippy::should_implement_trait)]
    pub fn mul(self, other: Quat) -> Quat {
        let [ax, ay, az, aw] = self.0;
        let [bx, by, bz, bw] = other.0;
        Quat([
            aw * bx + ax * bw + ay * bz - az * by,
            aw * by - ax * bz + ay * bw + az * bx,
            aw * bz + ax * by - ay * bx + az * bw,
            aw * bw - ax * bx - ay * by - az * bz,
        ])
    }

    pub fn inverse(self) -> Quat {
        let [x, y, z, w] = self.0;
        Quat([-x, -y, -z, w])
    }

    /// Normalised linear blend taking the short way round: for the small differences between
    /// poses it is as good as a slerp.
    pub fn blend(self, other: Quat, k: f32) -> Quat {
        let sign = if self.0.iter().zip(other.0).map(|(a, b)| a * b).sum::<f32>() < 0.0 { -1.0 } else { 1.0 };
        let q: [f32; 4] = std::array::from_fn(|i| self.0[i] * (1.0 - k) + other.0[i] * sign * k);
        let n = q.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-12);
        Quat(q.map(|v| v / n))
    }

    pub fn to_mat3(self) -> Mat3 {
        let [x, y, z, w] = self.0;
        Mat3::from_cols(
            [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + z * w), 2.0 * (x * z - y * w)],
            [2.0 * (x * y - z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + x * w)],
            [2.0 * (x * z + y * w), 2.0 * (y * z - x * w), 1.0 - 2.0 * (x * x + y * y)],
        )
    }

    /// The rotation `m` (orthonormal, right handed) is.
    pub fn from_mat3(m: &Mat3) -> Quat {
        let [c0, c1, c2] = m.cols;
        let (m00, m10, m20) = (c0[0], c0[1], c0[2]);
        let (m01, m11, m21) = (c1[0], c1[1], c1[2]);
        let (m02, m12, m22) = (c2[0], c2[1], c2[2]);
        let trace = m00 + m11 + m22;
        let q = if trace > 0.0 {
            let s = (trace + 1.0).sqrt() * 2.0;
            [(m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s]
        } else if m00 > m11 && m00 > m22 {
            let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
            [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s]
        } else if m11 > m22 {
            let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
            [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s]
        } else {
            let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
            [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s]
        };
        let n = q.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-12);
        Quat(q.map(|v| v / n))
    }
}

/// A rigid transform: `rotation`, then `origin`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rigid {
    pub rotation: Mat3,
    pub origin: V3,
}

impl Rigid {
    pub const IDENTITY: Rigid = Rigid { rotation: Mat3::IDENTITY, origin: vec3::ZERO };

    pub fn point(&self, p: V3) -> V3 {
        vec3::add(self.origin, self.rotation.transform(p))
    }

    pub fn direction(&self, d: V3) -> V3 {
        self.rotation.transform(d)
    }

    /// `self` applied after `inner`.
    pub fn then(&self, inner: &Rigid) -> Rigid {
        Rigid { rotation: self.rotation.mul_mat(&inner.rotation), origin: self.point(inner.origin) }
    }
}

/// A pose: each joint's rotation and extra offset, both local to its parent's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigPose {
    pub rotation: [Quat; JOINTS],
    pub offset: [V3; JOINTS],
}

impl RigPose {
    /// Every joint at rest.
    pub const REST: RigPose = RigPose { rotation: [Quat::IDENTITY; JOINTS], offset: [vec3::ZERO; JOINTS] };

    pub fn rotation(&self, joint: Joint) -> Quat {
        self.rotation[joint.index()]
    }

    /// `self` turned `k` of the way to `other`.
    pub fn blend(&self, other: &RigPose, k: f32) -> RigPose {
        RigPose {
            rotation: std::array::from_fn(|i| self.rotation[i].blend(other.rotation[i], k)),
            offset: std::array::from_fn(|i| vec3::lerp(self.offset[i], other.offset[i], k)),
        }
    }
}

impl Default for RigPose {
    fn default() -> Self {
        RigPose::REST
    }
}

/// The rig of one body: where each joint rests, in body space.
#[derive(Clone, Debug, PartialEq)]
pub struct Rig {
    pub rest: [V3; JOINTS],
}

impl Rig {
    /// The rig of `body` as it is posed (its arms may hold things).
    pub fn of(body: &Body) -> Rig {
        let hips = [body.leg(Side::Right).joints[0], body.leg(Side::Left).joints[0]];
        let mut rest = [vec3::ZERO; JOINTS];
        rest[Joint::Pelvis.index()] = [0.0, 0.5 * (hips[0][1] + hips[1][1]), 0.0];
        rest[Joint::Spine.index()] = [0.0, body.waist_y, 0.0];
        rest[Joint::Chest.index()] = [0.0, body.chest_y, 0.0];
        rest[Joint::Head.index()] = body.neck_base().origin;
        for side in Side::BOTH {
            for (joint, at) in Joint::arm(side).into_iter().zip(body.arm(side).joints) {
                rest[joint.index()] = at;
            }
            for (joint, at) in Joint::leg(side).into_iter().zip(body.leg(side).joints) {
                rest[joint.index()] = at;
            }
        }
        // The tail's bones start where its joints are; with none they rest at the pelvis, a
        // step the previous one ended (so nothing moves).
        let tail = body.tail_joints();
        for (i, joint) in Joint::TAIL.into_iter().enumerate() {
            rest[joint.index()] = tail.get(i).copied().unwrap_or(rest[Joint::Pelvis.index()]);
        }
        Rig { rest }
    }

    pub fn rest_at(&self, joint: Joint) -> V3 {
        self.rest[joint.index()]
    }

    /// Where `joint` sits relative to its parent at rest (the rest pelvis's is its place in
    /// body space).
    pub fn local_offset(&self, joint: Joint) -> V3 {
        match joint.parent() {
            Some(parent) => vec3::sub(self.rest_at(joint), self.rest_at(parent)),
            None => self.rest_at(joint),
        }
    }

    /// Each joint's world (body space) frame in `pose`.
    pub fn world(&self, pose: &RigPose) -> [Rigid; JOINTS] {
        let mut out = [Rigid::IDENTITY; JOINTS];
        for joint in Joint::ALL {
            let i = joint.index();
            let local = Rigid { rotation: pose.rotation[i].to_mat3(), origin: vec3::add(self.local_offset(joint), pose.offset[i]) };
            out[i] = match joint.parent() {
                Some(parent) => out[parent.index()].then(&local),
                None => local,
            };
        }
        out
    }

    /// What moves a rest-pose point to where it is in `pose`, for each joint: the joint's world
    /// frame less its rest place (Bevy's inverse bind poses undo the rest place).
    pub fn skin_frames(&self, pose: &RigPose) -> [Rigid; JOINTS] {
        let world = self.world(pose);
        std::array::from_fn(|i| {
            let w = world[i];
            // w * translate(-rest): a point p goes to w.rotation * (p - rest) + w.origin.
            Rigid { rotation: w.rotation, origin: vec3::sub(w.origin, w.rotation.transform(self.rest[i])) }
        })
    }

    /// `p` (a rest-pose point weighted `joints`, `weights`) skinned by `frames` ([`Rig::skin_frames`]).
    pub fn skin_point(frames: &[Rigid; JOINTS], p: V3, joints: [u16; 4], weights: [f32; 4]) -> V3 {
        let mut out = vec3::ZERO;
        for (j, w) in joints.into_iter().zip(weights) {
            if w > 0.0 {
                out = vec3::add(out, vec3::scale(frames[j as usize].point(p), w));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BodySpec, Folk};

    #[test]
    fn quaternions_round_trip_through_matrices() {
        for (a, b, c) in [(0.3, -1.1, 2.0), (3.0, 0.2, -0.4), (-2.5, 1.4, 0.9), (0.0, 0.0, 0.0)] {
            let q = Quat::x(a).mul(Quat::y(b)).mul(Quat::z(c));
            let m = q.to_mat3();
            let back = Quat::from_mat3(&m);
            let m2 = back.to_mat3();
            for i in 0..3 {
                assert!(vec3::distance(m.cols[i], m2.cols[i]) < 1e-4, "{a} {b} {c}");
            }
            assert!((m.determinant() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn a_quaternion_product_is_the_matrix_product() {
        let (p, q) = (Quat::x(0.7), Quat::y(-0.4));
        let m = p.mul(q).to_mat3();
        let n = p.to_mat3().mul_mat(&q.to_mat3());
        for i in 0..3 {
            assert!(vec3::distance(m.cols[i], n.cols[i]) < 1e-5);
        }
    }

    #[test]
    fn the_rig_rests_on_the_body_and_the_rest_pose_moves_nothing() {
        for folk in Folk::ALL {
            let body = Body::new(&BodySpec::roll(folk, 1207));
            let rig = Rig::of(&body);
            for joint in Joint::ALL {
                if let Some(parent) = joint.parent() {
                    assert!(parent.index() < joint.index(), "{joint:?} before its parent");
                }
            }
            let frames = rig.skin_frames(&RigPose::REST);
            for f in frames {
                assert!(vec3::distance(f.point([1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]) < 1e-4);
            }
            // Ankles at the ankles, wrists at the wrists.
            assert_eq!(rig.rest_at(Joint::AnkleL), body.leg(Side::Left).joints[2]);
            assert_eq!(rig.rest_at(Joint::WristR), body.arm(Side::Right).joints[2]);
        }
    }

    #[test]
    fn a_rotation_swings_about_the_joint() {
        let body = Body::new(&BodySpec::typical(Folk::Human));
        let rig = Rig::of(&body);
        let mut pose = RigPose::REST;
        pose.rotation[Joint::ShoulderL.index()] = Quat::x(0.5);
        let frames = rig.skin_frames(&pose);
        let shoulder = rig.rest_at(Joint::ShoulderL);
        let moved = Rig::skin_point(&frames, shoulder, [Joint::ShoulderL.index() as u16, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]);
        assert!(vec3::distance(moved, shoulder) < 1e-4, "the pivot stays");
        let wrist = rig.rest_at(Joint::WristL);
        let swung = Rig::skin_point(&frames, wrist, [Joint::ShoulderL.index() as u16, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]);
        assert!(swung[2] < wrist[2] - 0.5 || swung[2] > wrist[2] + 0.5);
        assert!((vec3::distance(swung, shoulder) - vec3::distance(wrist, shoulder)).abs() < 1e-4);
    }
}
