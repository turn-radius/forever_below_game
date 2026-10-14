//! Poses: where each bone of a creature's [`crate::Rig`] sits, on its parent, for a moment of
//! its motion. The game works out the motion (how far it has walked, whether it is biting) and
//! copies these placements onto the bones' transforms.
//!
//! - [`rat`]: a trot. Diagonal legs move together; each paw is planted flat on the floor
//!   through its stance and lifted through its swing, the leg bent to reach it by a two-bone
//!   solve ([`two_bone`]), so no paw sinks into the floor or floats over it. The body bobs, the
//!   tail swings in a wave from its root and drags on the floor rather than passing through
//!   it, the head sniffs, turns and lunges to bite.
//! - [`snake`]: the spine laid along a curve given by its heading at each distance along it.
//!   Slithering is a serpenoid curve, whose heading is a sine of the distance along the body,
//!   travelling back down it (Shigeo Hirose, *Biologically Inspired Robots: Snake-like
//!   Locomotors and Manipulators*, Oxford University Press, 1993); coiling is an Archimedean
//!   spiral, each turn lying beside the last; rearing lifts the front off the floor in an S,
//!   which a strike throws straight. The belly rests on the floor wherever it is not raised.
//!
//! The gait's timing, the wave's shape and every amplitude here are our own tuning.

use armory::geom::vec3::{self, Mat3, V3};
use fb_body::Placement;
use std::f32::consts::{PI, TAU};

use crate::rat::{LEGS, RatState, TAIL_BONES, bone as rat_bone};
use crate::shape::spine_to_creature;
use crate::snake::{SEGMENTS, SnakeState, bone as snake_bone};

/// How a rat is moving at a moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RatPose {
    /// The step cycle, radians: one turn per [`rat_stride`] walked.
    pub stride: f32,
    /// Speed as a share of its walking speed: 0 standing, 1 walking, 2 running.
    pub pace: f32,
    /// Seconds, for what it does standing: sniffing, and the tail's idle sway.
    pub time: f32,
    /// A bite's lunge, 0 to 1 at full stretch.
    pub bite: f32,
    /// How far the head turns toward its left (+X), radians.
    pub look: f32,
}

/// The ground a rat covers in one full step cycle (two steps of each pair of legs): half its
/// length.
pub fn rat_stride(rat: &RatState) -> f32 {
    0.5 * rat.length
}

/// Share of the cycle each paw is on the floor.
const DUTY: f32 = 0.6;
/// Each leg's offset in the cycle, in [`LEGS`] order: a trot, diagonal pairs together.
const LEG_PHASE: [f32; 4] = [0.0, PI, PI, 0.0];

/// A rotation about +X (in the spine frame, tipping +Y up toward +Z).
fn rx(a: f32) -> Mat3 {
    Mat3::from_axis_angle([1.0, 0.0, 0.0], a)
}

/// A rotation about +Z (in the spine frame, turning +Y toward -X, the creature's left).
fn rz(a: f32) -> Mat3 {
    Mat3::from_axis_angle([0.0, 0.0, 1.0], a)
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A two-bone leg reaching from its joint to `target`, `(forward, up)` from the joint in its
/// plane: the turn of the upper bone from hanging straight down (positive swings it forward)
/// and the turn of the lower bone from straight on from the upper. With `knee_forward` the
/// middle joint bends forward (a hind leg's knee), else back (a foreleg's elbow). A target out
/// of reach is reached toward, straight-legged.
pub fn two_bone(upper: f32, lower: f32, target: [f32; 2], knee_forward: bool) -> (f32, f32) {
    let reach = (target[0] * target[0] + target[1] * target[1]).sqrt().clamp((upper - lower).abs() + 1e-4, (upper + lower) * 0.9999);
    let toward = target[0].atan2(-target[1]);
    let at_hip = ((upper * upper + reach * reach - lower * lower) / (2.0 * upper * reach)).clamp(-1.0, 1.0).acos();
    let at_knee = ((upper * upper + lower * lower - reach * reach) / (2.0 * upper * lower)).clamp(-1.0, 1.0).acos();
    if knee_forward { (toward + at_hip, -(PI - at_knee)) } else { (toward - at_hip, PI - at_knee) }
}

/// Every bone's placement on its parent for `pose`, in the rat's [`crate::Rig`] order.
pub fn rat(state: &RatState, pose: &RatPose) -> Vec<Placement> {
    let d = state.dims();
    let l = d.length;
    let moving = pose.pace.clamp(0.0, 1.0);
    let mut out = vec![Placement::IDENTITY; rat_bone::COUNT];

    // The body bobs twice a cycle, highest mid-stance, and lunges into a bite.
    let bob = 0.018 * l * pose.pace.min(1.5) * (0.5 - 0.5 * (2.0 * pose.stride).cos());
    let lunge = 0.07 * l * pose.bite;
    let height = d.centre_height + bob;
    out[rat_bone::BODY] = Placement { rotation: spine_to_creature(), origin: [0.0, height, lunge] };

    // The head: nose a little down, sniffing about when still, up and forward to bite.
    let t = pose.time;
    let still = 1.0 - moving;
    let sniff_pitch = still * (0.07 * (2.9 * t).sin() + 0.04 * (7.3 * t + 1.1).sin());
    let sniff_yaw = still * (0.22 * (0.83 * t).sin() + 0.08 * (2.1 * t + 0.4).sin());
    let pitch = -0.14 + sniff_pitch + 0.42 * pose.bite;
    out[rat_bone::HEAD] = Placement { rotation: rz(pose.look + sniff_yaw).mul_mat(&rx(pitch)), origin: vec3::add(d.neck, [0.0, 0.05 * l * pose.bite, 0.0]) };

    // Legs: each paw's ankle follows its stance and swing; the leg solves to reach it.
    let ground = -height;
    for k in 0..4 {
        let (_, _, hind) = LEGS[k];
        let leg = d.leg(k);
        let root = d.leg_root(k);
        let c = ((pose.stride + LEG_PHASE[k]) / TAU).rem_euclid(1.0);
        let sweep = DUTY * rat_stride(state);
        let (along, lift) = if c < DUTY {
            (sweep * (0.5 - c / DUTY), 0.0)
        } else {
            let u = (c - DUTY) / (1.0 - DUTY);
            (sweep * (smoothstep(u) - 0.5), 0.07 * l * (PI * u).sin())
        };
        let vertical = root[2] - (ground + leg.ankle);
        // Paws stay within reach: past it, a planted paw slides rather than leaving the floor.
        let reach = 0.92 * ((leg.upper + leg.lower).powi(2) - vertical * vertical).max(0.0).sqrt();
        let neutral = if hind { 0.06 * l } else { 0.02 * l };
        let along = (neutral + along * moving).clamp(-reach, reach);
        let target = [along, ground + leg.ankle + lift * moving - root[2]];
        let (upper, lower) = two_bone(leg.upper, leg.lower, target, hind);
        let i = rat_bone::LEGS + 3 * k;
        out[i] = Placement { rotation: rx(upper), origin: root };
        out[i + 1] = Placement { rotation: rx(lower), origin: [0.0, 0.0, -leg.upper] };
        // The paw stays level, flat on the floor.
        out[i + 2] = Placement { rotation: rx(-(upper + lower)), origin: [0.0, 0.0, -leg.lower] };
    }

    // The tail: drooping from the rump, swinging in a wave from its root, held off the floor.
    let segment = d.tail_length / TAIL_BONES as f32;
    let sway = (0.18 + 0.2 * moving) * if pose.pace > 0.0 { 1.0 } else { 0.7 };
    let rate = 1.4 + 2.2 * moving;
    let body = Placement { rotation: Mat3::IDENTITY, origin: [0.0, 0.0, 0.0] };
    let mut parent = body;
    let mut origin = d.tail_root;
    for k in 0..TAIL_BONES {
        let yaw = sway * (rate * t + pose.stride * 0.5 - 0.9 * k as f32).sin();
        let droop = if k == 0 { 0.6 } else { -0.16 };
        let r_end = d.tail_radius * (1.0 - 0.82 * (k + 1) as f32 / TAIL_BONES as f32);
        let local_origin = if k == 0 { origin } else { [0.0, -segment, 0.0] };
        let start = parent.point(local_origin);
        let end_of = |pitch: f32| {
            let rotation = parent.rotation.mul_mat(&rz(yaw)).mul_mat(&rx(pitch));
            vec3::add(start, rotation.transform([0.0, -segment, 0.0]))[2]
        };
        // Positive pitch tips the tail down; lift it until its end clears the floor.
        let floor = ground + 1.05 * r_end;
        let mut pitch = droop;
        if end_of(pitch) < floor {
            let (mut lo, mut hi) = (droop - PI * 0.6, droop);
            for _ in 0..20 {
                let mid = 0.5 * (lo + hi);
                if end_of(mid) < floor { hi = mid } else { lo = mid }
            }
            pitch = lo;
        }
        let local = Placement { rotation: rz(yaw).mul_mat(&rx(pitch)), origin: local_origin };
        out[rat_bone::TAIL + k] = local;
        parent = parent.then(&local);
        origin = [0.0, -segment, 0.0];
    }
    out
}

/// How a snake is lying or moving at a moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnakePose {
    /// The slither's wave, radians: one turn moves the wave a wavelength back down the body.
    pub phase: f32,
    /// How strongly it slithers: 0 lying in a lazy S, 1 slithering.
    pub pace: f32,
    /// 0 laid out, 1 coiled in a flat spiral.
    pub coil: f32,
    /// 0 flat on the floor, 1 the front of the body raised in an S, ready to strike.
    pub rear: f32,
    /// 0 drawn back, 1 thrown straight forward at full stretch.
    pub strike: f32,
    /// The jaw's gape, 0 shut to 1 wide.
    pub jaw: f32,
    /// The tongue, 0 in to 1 flicked out.
    pub tongue: f32,
    /// How far the head turns toward its left (+X), radians.
    pub look: f32,
}

impl SnakePose {
    /// Coiled, the head raised a little in the middle: how a snake rests and waits.
    pub const REST: SnakePose = SnakePose { phase: 0.0, pace: 0.0, coil: 1.0, rear: 0.45, strike: 0.0, jaw: 0.0, tongue: 0.0, look: 0.0 };
}

impl Default for SnakePose {
    fn default() -> Self {
        SnakePose::REST
    }
}

/// The slither's wavelength along the body, as a share of its length, and its greatest
/// heading off straight, radians.
pub const WAVELENGTH: f32 = 0.55;
const WAVE_ANGLE: f32 = 0.62;
/// The rearing S's greatest heading off straight, radians.
const NECK_ANGLE: f32 = 1.0;

/// Where the snake's spine lies for `pose`: the centre line's point and heading at `samples`
/// distances from the neck (0) to the tail tip, each `(point, toward the head)`, in creature
/// space with the body's middle (behind the neck) on the origin.
pub fn snake_spine(state: &SnakeState, pose: &SnakePose, samples: usize) -> Vec<(V3, V3)> {
    let d = state.dims();
    let body = d.body_length;
    let steps = samples.max(2) - 1;
    let step = body / steps as f32;
    let (a, b) = (d.coil_inner, d.coil_spacing);
    let lambda = WAVELENGTH * body;
    let amplitude = WAVE_ANGLE * (0.45 + 0.55 * pose.pace.clamp(0.0, 1.0));
    let neck = d.neck;
    let raise = pose.rear.clamp(0.0, 1.0);
    let strike = pose.strike.clamp(0.0, 1.0);
    // Heading at distance `s` from the neck: blend of the slither and the coil, with the
    // rearing S in the neck that a strike straightens.
    let heading = |s: f32| {
        let slither = amplitude * (TAU * s / lambda - pose.phase).sin();
        let spiral = state.hand * ((a * a + 2.0 * b * s).sqrt() - a) / b;
        let s_curve = if s < neck { NECK_ANGLE * raise * (1.0 - strike) * (TAU * s / neck).sin() * (1.0 - s / neck) } else { 0.0 };
        (1.0 - pose.coil) * slither + pose.coil * spiral + s_curve
    };
    // Height of the centre line: belly on the floor, the neck lifted when rearing.
    let lift_height = neck * if d.constrictor { 0.45 } else { 0.55 } * raise * (1.0 - 0.35 * strike);
    // The head is deeper than the neck: the neck rises to carry it clear of the floor.
    let head_clear = (state.head_depth() - SnakeState::belly_depth(state.radius_at(0.0))).max(0.0);
    let lift = |s: f32| (lift_height * (1.0 - smoothstep(s / neck))).max(head_clear * (1.0 - smoothstep(s / (1.5 * d.segment))));
    let mut points: Vec<V3> = Vec::with_capacity(samples);
    let mut p = [0.0f32, 0.0];
    for i in 0..=steps {
        let s = i as f32 * step;
        let r = state.radius_at(s / body);
        points.push([p[0], SnakeState::belly_depth(r) + lift(s), p[1]]);
        // Step back along the body, a level step shortened by the climb.
        let climb = lift(s + step) - lift(s);
        let level = (step * step - climb * climb).max(0.09 * step * step).sqrt();
        let h = heading(s + 0.5 * step);
        p = [p[0] - level * h.sin(), p[1] - level * h.cos()];
    }
    // A strike throws the neck forward along its own line, beyond what straightening gives.
    let throw = 1.6 * d.girth * strike;
    for (i, point) in points.iter_mut().enumerate() {
        let s = i as f32 * step;
        if s < neck {
            point[2] += throw * (1.0 - s / neck);
        }
    }
    // The body behind the neck holds its place: its middle on the origin.
    let behind: Vec<&V3> = points.iter().enumerate().filter(|(i, _)| *i as f32 * step >= neck).map(|(_, p)| p).collect();
    let mid = behind.iter().fold([0.0f32; 2], |m, p| [m[0] + p[0], m[1] + p[2]]);
    let mid = [mid[0] / behind.len().max(1) as f32, mid[1] / behind.len().max(1) as f32];
    for point in &mut points {
        point[0] -= mid[0];
        point[2] -= mid[1];
    }
    (0..points.len())
        .map(|i| {
            let ahead = points[i.saturating_sub(1)];
            let behind = points[(i + 1).min(points.len() - 1)];
            (points[i], vec3::normalize_or(vec3::sub(ahead, behind), [0.0, 0.0, 1.0]))
        })
        .collect()
}

/// A bone's rotation in the spine frame for a body running toward `forward`: +Y along it,
/// +X to its right on the level, +Z up.
fn along(forward: V3) -> Mat3 {
    let y = vec3::normalize_or(forward, [0.0, 0.0, 1.0]);
    let x = vec3::normalize_or(vec3::cross(y, [0.0, 1.0, 0.0]), [-1.0, 0.0, 0.0]);
    let z = vec3::cross(x, y);
    Mat3::from_cols(x, y, z)
}

/// Every bone's placement on its parent for `pose`, in the snake's [`crate::Rig`] order.
pub fn snake(state: &SnakeState, pose: &SnakePose) -> Vec<Placement> {
    let d = state.dims();
    let per = 4;
    let samples = SEGMENTS * per + 1;
    let spine = snake_spine(state, pose, samples);
    let mut out = vec![Placement::IDENTITY; snake_bone::COUNT];
    for k in 0..SEGMENTS {
        // The centre of segment k lies (k + 0.5) segments behind the neck.
        let i = k * per + per / 2;
        let (point, forward) = spine[i];
        let mut place = Placement { rotation: along(forward), origin: point };
        // A straight segment on a curving line can dip at its ends where the line bends up:
        // lift it until its belly, end to end, clears the floor.
        let half = 0.5 * d.segment;
        let s_mid = (k as f32 + 0.5) * d.segment;
        let belly = |y: f32| SnakeState::belly_depth(state.radius_at(((s_mid - y) / d.body_length).clamp(0.0, 1.0)));
        // Along the belly, and round each rounded end.
        let line = (0..=4).map(|i| -half + d.segment * i as f32 / 4.0).map(|y| [0.0, y, -belly(y)]);
        let ends = [-1.0f32, 1.0].into_iter().flat_map(|end| {
            let depth = belly(end * half);
            let reach = state.radius_at(((s_mid - end * half) / d.body_length).clamp(0.0, 1.0));
            (1..=8).map(move |i| {
                let a = i as f32 / 8.0 * std::f32::consts::FRAC_PI_2;
                [0.0, end * (half + reach * a.sin()), -depth * a.cos()]
            })
        });
        let low = line.chain(ends).map(|p| place.point(p)[1]).fold(f32::MAX, f32::min);
        if low < 0.0 {
            place.origin[1] -= low;
        }
        out[snake_bone::SPINE + k] = place;
    }
    // The head at the neck, carrying on its line, turned and dipped to strike.
    let (neck, forward) = spine[0];
    let rotation = along(forward).mul_mat(&rz(pose.look)).mul_mat(&rx(pose.rear * (0.1 - 0.35 * pose.strike)));
    let mut head_place = Placement { rotation, origin: neck };
    let (line, hinge) = state.mouth();
    let head = state.head_trunk();
    // The underside of the head and jaw, along the middle, in the head's frame.
    let chin: Vec<V3> = (0..=12).map(|i| i as f32 / 12.0).map(|t| [0.0, head.centre(t)[1], (head.centre(t)[2] - head.half_height(t)).min(line)]).collect();
    let jaw_at = |gape: f32| Placement { rotation: rx(-0.9 * gape), origin: hinge };
    let jaw_low = |head_place: &Placement, gape: f32| {
        let jaw = head_place.then(&jaw_at(gape));
        chin.iter().map(|&p| jaw.point(vec3::sub(p, hinge))[1]).fold(f32::MAX, f32::min)
    };
    // The chin never goes through the floor: the head rides up over it, and the jaw drops no
    // further than the floor lets it.
    let shut = jaw_low(&head_place, 0.0);
    if shut < 0.0 {
        head_place.origin[1] -= shut;
    }
    let mut gape = pose.jaw.clamp(0.0, 1.0);
    if jaw_low(&head_place, gape) < 0.0 {
        let (mut lo, mut hi) = (0.0, gape);
        for _ in 0..16 {
            let mid = 0.5 * (lo + hi);
            if jaw_low(&head_place, mid) < 0.0 { hi = mid } else { lo = mid }
        }
        gape = lo;
    }
    out[snake_bone::HEAD] = head_place;
    out[snake_bone::JAW] = jaw_at(gape);
    // The tongue lies along the mouth, its tips just behind the snout, and flicks out past it.
    let tongue_length = 0.85 * d.head_length;
    let tip = head.start + 0.97 * head.length;
    let out_by = 0.6 * d.head_length * pose.tongue.clamp(0.0, 1.0);
    out[snake_bone::TONGUE] = Placement { rotation: Mat3::IDENTITY, origin: [0.0, tip - tongue_length - 0.02 * d.head_length + out_by, line + 0.01 * d.girth] };
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Species;

    #[test]
    fn two_bones_reach_their_target() {
        for knee_forward in [true, false] {
            for target in [[0.0, -1.5], [0.4, -1.4], [-0.5, -1.2], [0.2, -0.9]] {
                let (a, b) = two_bone(1.0, 0.9, target, knee_forward);
                let knee = [a.sin(), -a.cos()];
                let ankle = [knee[0] + 0.9 * (a + b).sin(), knee[1] - 0.9 * (a + b).cos()];
                assert!((ankle[0] - target[0]).abs() < 1e-3 && (ankle[1] - target[1]).abs() < 1e-3, "{target:?} {ankle:?}");
                // The knee bends the way asked.
                let side = knee[0] * target[1] - knee[1] * target[0];
                assert!(if knee_forward { side < 0.0 } else { side > 0.0 }, "{target:?} {knee_forward}");
            }
        }
    }

    /// The lowest point of every part of a rig posed by `places`.
    fn lowest(rig: &crate::Rig, places: &[Placement]) -> f32 {
        rig.posed_mesh(places).bounds().unwrap().0[1]
    }

    #[test]
    fn a_trotting_rat_keeps_its_paws_on_the_floor() {
        for species in [Species::Rat, Species::GiantRat] {
            let state = RatState::roll_as(3, species);
            let rig = state.build();
            let l = state.length;
            for step in 0..24 {
                for pace in [0.0, 1.0, 2.0] {
                    let pose = RatPose { stride: step as f32 / 24.0 * TAU, pace, time: step as f32 * 0.21, bite: 0.0, look: 0.0 };
                    let places = rat(&state, &pose);
                    let low = lowest(&rig, &places);
                    assert!(low > -0.015 * l, "{species:?} sinks to {low} at step {step}, pace {pace}");
                    assert!(low < 0.015 * l, "{species:?} floats at {low} at step {step}, pace {pace}");
                    // The paws in stance are on the floor.
                    let world = rig.world(&places);
                    for (k, phase) in LEG_PHASE.iter().enumerate() {
                        let c = ((pose.stride + phase) / TAU).rem_euclid(1.0);
                        let paw = rat_bone::LEGS + 3 * k + 2;
                        let bottom = rig.bones[paw].mesh.parts.iter().flat_map(|p| &p.positions).map(|&q| world[paw].point(q)[1]).fold(f32::MAX, f32::min);
                        if c < DUTY || pace == 0.0 {
                            assert!(bottom.abs() < 0.015 * l, "{species:?} paw {k} at {bottom}, step {step}, pace {pace}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_biting_rat_lunges_forward() {
        let state = RatState::roll_as(3, Species::Rat);
        let rig = state.build();
        let nose = |bite: f32| {
            let places = rat(&state, &RatPose { bite, ..RatPose::default() });
            let world = rig.world(&places);
            world[rat_bone::HEAD].point([0.0, 0.3 * state.length, 0.0])
        };
        assert!(nose(1.0)[2] > nose(0.0)[2] + 0.05 * state.length);
    }

    #[test]
    fn a_snake_lies_on_the_floor_in_every_pose() {
        for species in [Species::PoisonousSnake, Species::ConstrictorSnake] {
            for seed in 0..6 {
                let state = SnakeState::roll_as(seed, species);
                let rig = state.build();
                let r = state.girth;
                for coil in [0.0, 0.5, 1.0] {
                    for rear in [0.0, 1.0] {
                        for strike in [0.0, 1.0] {
                            for k in 0..6 {
                                let pose = SnakePose { phase: k as f32, pace: 1.0, coil, rear, strike, jaw: strike * rear, tongue: 0.5, look: 0.0 };
                                let low = lowest(&rig, &snake(&state, &pose));
                                assert!(low > -0.06 * r, "{species:?} {seed} sinks to {low} in {pose:?}");
                                assert!(low < 0.06 * r, "{species:?} {seed} floats at {low} in {pose:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_spine_keeps_its_length_and_the_wave_travels() {
        let state = SnakeState::roll_as(2, Species::ConstrictorSnake);
        let d = state.dims();
        for pose in [SnakePose { coil: 0.0, rear: 0.0, ..SnakePose::REST }, SnakePose::REST, SnakePose { rear: 1.0, ..SnakePose::REST }] {
            let spine = snake_spine(&state, &pose, 57);
            let length: f32 = spine.windows(2).map(|w| vec3::distance(w[0].0, w[1].0)).sum();
            assert!((length - d.body_length).abs() < 0.03 * d.body_length, "{length} vs {} in {pose:?}", d.body_length);
        }
        // Moving the wave's phase moves the body side to side.
        let laid = |phase| snake_spine(&state, &SnakePose { phase, pace: 1.0, coil: 0.0, rear: 0.0, ..SnakePose::REST }, 57)[28].0;
        assert!((laid(0.0)[0] - laid(1.5)[0]).abs() > 0.5 * d.girth);
    }

    #[test]
    fn a_strike_throws_the_head_forward_and_rearing_lifts_it() {
        let state = SnakeState::roll_as(4, Species::PoisonousSnake);
        let head = |pose: SnakePose| snake(&state, &pose)[snake_bone::HEAD].origin;
        let drawn = head(SnakePose { rear: 1.0, strike: 0.0, ..SnakePose::REST });
        let struck = head(SnakePose { rear: 1.0, strike: 1.0, ..SnakePose::REST });
        let flat = head(SnakePose { rear: 0.0, ..SnakePose::REST });
        assert!(vec3::distance(drawn, struck) > state.girth, "{drawn:?} {struck:?}");
        assert!(drawn[1] > flat[1] + 2.0 * state.girth);
    }
}
