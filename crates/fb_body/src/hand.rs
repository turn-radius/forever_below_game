//! Bare hands on an [`HandForm`]: the palm, curling fingers, a thumb on its fleshy pad, and
//! nails. Built in the form's space (a right hand, back toward +Z, fingers up +Y, wrist at
//! y = 0); [`crate::Body::hand_placement`] sets each on its wrist.
//!
//! Departures from the prototype's `hand`, which was an ellipsoid with a thumb: the palm is
//! the glove's [`palm_mesh`], four fingers and a thumb are [`digit_mesh`] tubes that curl
//! joint by joint ([`armory::build::form::Digit::joints`]), the ball of the thumb is a pad
//! blending it into the palm, and each digit has a nail.

use armory::build::form::{Digit, HandForm};
use armory::build::hand::{digit_mesh, palm_mesh};
use armory::geom::vec3::{self, V3};
use armory::geom::{MeshBuilder, uv_sphere};

/// How a hand is held: the curl of each finger (index to little) and the thumb, in radians
/// per joint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandPose {
    pub fingers: [f32; 4],
    pub thumb: f32,
}

impl HandPose {
    /// Hanging loose: a slight curl, more toward the little finger.
    pub const RELAXED: HandPose = HandPose { fingers: [0.22, 0.28, 0.34, 0.4], thumb: 0.18 };

    /// Closed round a grip of radius `grip`: each finger bends at each joint by about its
    /// phalanx's length over the radius it wraps, so the chain of phalanges rounds the grip.
    pub fn gripping(form: &HandForm, grip: f32) -> HandPose {
        let curl = |d: &Digit| {
            let mean = d.length * d.phalanges.iter().filter(|f| **f > 0.0).sum::<f32>() / d.phalanges.iter().filter(|f| **f > 0.0).count() as f32;
            (mean / (grip + d.radius)).clamp(0.6, 1.5)
        };
        HandPose { fingers: form.fingers.map(|f| curl(&f)), thumb: 0.75 }
    }
}

/// The centre of the channel a curled finger closes round, in the hand's space: the
/// circle through the middle finger's first three joints, at the middle of the knuckles.
pub fn grip_centre(form: &HandForm, pose: &HandPose) -> V3 {
    let j = form.fingers[1].joints(pose.fingers[1]);
    let [a, b, c] = [j[0], j[1], j[2]].map(|p| [p[1], p[2]]);
    // Circumcentre in the finger's (y, z) plane.
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-6 {
        return [0.0, form.palm_length, -form.knuckle_half_depth * 2.0];
    }
    let sq = |p: [f32; 2]| p[0] * p[0] + p[1] * p[1];
    let y = (sq(a) * (b[1] - c[1]) + sq(b) * (c[1] - a[1]) + sq(c) * (a[1] - b[1])) / d;
    let z = (sq(a) * (c[0] - b[0]) + sq(b) * (a[0] - c[0]) + sq(c) * (b[0] - a[0])) / d;
    [0.0, y, z]
}

/// The skin of one hand and, separately, its nails.
pub fn hand_meshes(form: &HandForm, pose: &HandPose) -> (MeshBuilder, MeshBuilder) {
    hand_meshes_with(form, pose, false)
}

/// The skin of one hand and its nails, or its claws: a long hooked point from the end of each
/// digit instead of a nail.
pub fn hand_meshes_with(form: &HandForm, pose: &HandPose, claws: bool) -> (MeshBuilder, MeshBuilder) {
    let mut skin = MeshBuilder::new();
    // The palm rounds off just past the knuckles, under the fingers' roots.
    skin.append(&palm_mesh(form, form.knuckle_half_depth * 1.2, 0.0));
    let mut nails = MeshBuilder::new();
    let digits = form.fingers.iter().zip(pose.fingers).chain(std::iter::once((&form.thumb, pose.thumb)));
    for (digit, curl) in digits {
        skin.append(&digit_mesh(digit, curl, 0.0));
        nails.append(&if claws { claw(digit, curl) } else { nail(digit, curl) });
    }
    // The ball of the thumb: a flattened pad from the wrist toward the thumb's root.
    let t = &form.thumb;
    let mut pad = uv_sphere(1.0, 16, 10);
    let w = form.wrist_half_width;
    pad.scale([w * 0.55, form.palm_length * 0.36, form.wrist_half_depth * 0.85]);
    let centre = vec3::lerp([t.base[0] * 0.55, form.palm_length * 0.22, -form.wrist_half_depth * 0.25], t.base, 0.35);
    pad.place_basis(centre, vec3::normalize_or([-0.35, 1.0, 0.0], vec3::Y), vec3::Z);
    skin.append(&pad);
    (skin, nails)
}

/// A claw: a curved point from the tip of a digit, along its last phalanx and hooking in toward the palm.
fn claw(digit: &Digit, curl: f32) -> MeshBuilder {
    let joints = digit.joints(curl);
    let n = joints.len();
    let (a, b) = (joints[n - 2], joints[n - 1]);
    let along = vec3::normalize_or(vec3::sub(b, a), vec3::Y);
    let back = vec3::normalize_or(vec3::sub(vec3::scale(along, vec3::dot(digit.bend, along)), digit.bend), vec3::Z);
    let r = digit.radius;
    // A digit closed on a grip (curled past a relaxed hand's) keeps a short claw, so it does not
    // poke into the haft it holds.
    let hold = ((curl - 0.42) / 0.18).clamp(0.0, 1.0);
    let len = 1.5 * vec3::distance(a, b).max(r * 2.0) * (1.0 - 0.65 * hold * hold * (3.0 - 2.0 * hold));
    // The base sits on the tip; the claw leaves along the phalanx, tipped a little toward the palm.
    let dir = vec3::normalize_or(vec3::sub(vec3::scale(along, 1.0), vec3::scale(back, 0.22)), along);
    let mut m = armory::geom::spike(0.62 * r, len, 7, 0.0, armory::geom::Shading::SMOOTH);
    m.place(vec3::add(b, vec3::scale(along, -0.2 * r)), dir);
    m
}

/// A nail on the back of a digit's last phalanx.
fn nail(digit: &Digit, curl: f32) -> MeshBuilder {
    let joints = digit.joints(curl);
    let n = joints.len();
    let (a, b) = (joints[n - 2], joints[n - 1]);
    let along = vec3::normalize_or(vec3::sub(b, a), vec3::Y);
    // The back of the phalanx: away from the way it bends.
    let back = vec3::normalize_or(vec3::sub(vec3::scale(along, vec3::dot(digit.bend, along)), digit.bend), vec3::Z);
    let r = digit.radius * 0.82;
    let centre = vec3::add(vec3::lerp(a, b, 0.72), vec3::scale(back, r * 0.88));
    let mut m = uv_sphere(1.0, 10, 6);
    m.scale([r * 0.62, vec3::distance(a, b) * 0.3, r * 0.2]);
    m.place_basis(centre, along, back);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gripping_hand_closes_round_its_grip() {
        let form = HandForm::mannequin(1.0, 0.1);
        let pose = HandPose::gripping(&form, 0.13);
        let c = grip_centre(&form, &pose);
        // The channel sits in front of the palm, about a grip's radius from each phalanx.
        assert!(c[2] < -form.knuckle_half_depth * 0.5, "{c:?}");
        for p in form.fingers[1].joints(pose.fingers[1]).iter().take(3) {
            let d = vec3::distance(*p, c);
            assert!(d > 0.1 && d < 0.5, "{d}");
        }
    }
}
