//! How folk move: a walk cycle and an idle stance as poses of the [`Rig`] ([`Gait::walk`],
//! [`Gait::idle`]), blended by speed ([`Gait::pose`]). Plain Rust, no Bevy, deterministic.
//!
//! **Walk.** The cycle's phase runs 0 to 1 over two steps and advances by distance, so a
//! longer step needs a longer walk ([`Gait::stride_length`]) and the feet never slide: the
//! stance foot is planted on the floor and held flat, moving back through the body's frame
//! at exactly the body's speed, and the swing foot lifts and carries through to meet it
//! (matching its velocity there). Legs are solved by two-bone inverse kinematics to those
//! foot targets, the knee forward, so the bob of the pelvis, the bend of the knees and the
//! reach of the stride all follow from the leg's own length: a dwarf's short legs take short
//! steps, a goblin's long ones long. The pelvis turns and rolls a little and the spine
//! counter-twists, the arms swing opposite the legs (less, or not at all, for an arm that
//! holds something), and the head stays level. Our own; the prototype had no walk.
//!
//! **Idle.** Feet stay planted at rest; the chest rises and falls with the breath and the
//! pelvis now and then shifts across to one leg, the other relaxing, which replaces the whole
//! figure's sway the folk had before they had a skeleton.

use std::f32::consts::{PI, TAU};

use armory::build::hafted::smoothstep;
use armory::geom::vec3::{self, Mat3, V3};
use procgen_rng::Mulberry32;

use crate::body::Body;
use crate::limb::Side;
use crate::rig::{Joint, Quat, Rig, RigPose};

/// Steps a second at the natural walk (a stroll).
pub const STEPS_PER_SECOND: f32 = 1.8;
/// A step's length as a share of the leg's length (ankle to hip).
const STEP: f32 = 0.7;
/// How high the swing foot lifts, as a share of the leg's length.
const LIFT: f32 = 0.1;
/// How far the stance leg stays from straight: its reach as a share of its length.
const REACH: f32 = 0.975;
/// Arm swing at the natural walk, radians.
const ARM_SWING: f32 = 0.42;
/// Pelvis turn and roll at the natural walk, radians.
const PELVIS_YAW: f32 = 0.075;
const PELVIS_ROLL: f32 = 0.05;
/// The pelvis sways toward the stance leg by this share of the hips' half spacing.
const SWAY: f32 = 0.3;

/// Breaths per second at rest: 15 a minute, inside the 12 to 18 the Cleveland Clinic gives for
/// a resting adult (<https://my.clevelandclinic.org/health/articles/10881-vital-signs>), each
/// person a little faster or slower.
pub const BREATHS_PER_SECOND: f32 = 0.25;
/// How far a breath lifts the chest, as a share of the height. Our own tuning: enough to see
/// close up, never enough to read as bobbing.
const BREATH_LIFT: f32 = 0.006;
/// Weight shifts from foot to foot: about one every ten seconds. Our own tuning.
pub const SHIFTS_PER_SECOND: f32 = 0.05;
const SHIFT_SWAY: f32 = 0.16;
const SHIFT_ROLL: f32 = 0.03;

/// Salt for a person's idle stream. Our own.
const IDLE_SALT: u32 = 0x1D1E_0B0D;

/// How someone stands still: each person's breathing and weight-shift rates and phases are
/// their own, from their seed, so a group never moves in step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IdleCycle {
    pub breath_rate: f32,
    pub breath_phase: f32,
    pub shift_rate: f32,
    pub shift_phase: f32,
}

impl IdleCycle {
    pub fn of(seed: u32) -> IdleCycle {
        let mut rng = Mulberry32::new(seed ^ IDLE_SALT);
        IdleCycle {
            breath_rate: BREATHS_PER_SECOND * rng.range_f32(0.85, 1.15),
            breath_phase: rng.range_f32(0.0, TAU),
            shift_rate: SHIFTS_PER_SECOND * rng.range_f32(0.7, 1.3),
            shift_phase: rng.range_f32(0.0, TAU),
        }
    }

    /// The breath at `t` seconds, 0 (out) to 1 (in).
    pub fn breath(&self, t: f32) -> f32 {
        (TAU * self.breath_rate * t + self.breath_phase).sin() * 0.5 + 0.5
    }

    /// The weight shift at `t`, -1 (over the right foot) to 1 (over the left). It dwells on
    /// each foot (a sine through a soft clamp) rather than swaying evenly.
    pub fn shift(&self, t: f32) -> f32 {
        ((TAU * self.shift_rate * t + self.shift_phase).sin() * 2.0).clamp(-1.0, 1.0)
    }
}

/// One leg's rest geometry.
#[derive(Clone, Copy, Debug)]
struct Leg {
    hip: V3,
    ankle: V3,
    upper: f32,
    lower: f32,
    /// Hip to knee and knee to ankle at rest.
    rest: [V3; 2],
}

/// A body's gait: its rig and its legs' reach, which set its stride.
#[derive(Clone, Debug)]
pub struct Gait {
    rig: Rig,
    /// Right then left.
    legs: [Leg; 2],
    scale: f32,
    height: f32,
    leg_length: f32,
    /// How much each arm holds (0 free, 1 holding), right then left.
    hold: [f32; 2],
    /// How many bones the jointed tail has, if the body has one.
    tail: usize,
    /// How much of the foot's toe-down pitch a swing keeps: the pitch swings the foot about the
    /// ankle, so a foot hanging far below it (a hoof under a high hock) pitches less (squared)
    /// or the toe would dig into the floor. 1 for every folk.
    pitch_k: f32,
}

/// A frame whose `y` is `y` and whose `z` is as near forward as it can be.
fn frame(y: V3) -> Mat3 {
    let z = vec3::normalize_or(vec3::sub(vec3::Z, vec3::scale(y, y[2])), vec3::X);
    Mat3::from_cols(vec3::cross(y, z), y, z)
}

fn transpose(m: &Mat3) -> Mat3 {
    let [a, b, c] = m.cols;
    Mat3::from_cols([a[0], b[0], c[0]], [a[1], b[1], c[1]], [a[2], b[2], c[2]])
}

fn rot(axis: V3, angle: f32) -> Mat3 {
    Mat3::from_axis_angle(axis, angle)
}

impl Gait {
    /// The gait of `body` (posed for what its hands hold), on `rig` (`Rig::of(body)`).
    pub fn new(body: &Body, rig: &Rig) -> Gait {
        let leg = |side: Side| {
            let [hip, knee, ankle] = Joint::leg(side).map(|j| rig.rest_at(j));
            Leg {
                hip,
                ankle,
                upper: vec3::distance(hip, knee),
                lower: vec3::distance(knee, ankle),
                rest: [vec3::normalize_or(vec3::sub(knee, hip), vec3::neg(vec3::Y)), vec3::normalize_or(vec3::sub(ankle, knee), vec3::neg(vec3::Y))],
            }
        };
        let legs = [leg(Side::Right), leg(Side::Left)];
        let held = Side::BOTH.map(|side| body.pose.arm(side).grip.is_some());
        // One hand full: that arm swings a little. Both: the hands stay where they are.
        let hold = match held {
            [true, true] => [1.0, 1.0],
            [true, false] => [0.8, 0.0],
            [false, true] => [0.0, 0.8],
            [false, false] => [0.0, 0.0],
        };
        Gait { rig: rig.clone(), legs, scale: body.scale, height: body.height, leg_length: legs[0].hip[1] - legs[0].ankle[1], hold, tail: body.tail_joints().len(), pitch_k: ((0.048 * body.height) / body.ankle_y).min(1.0).powi(2) }
    }

    /// How far a step reaches.
    pub fn step_length(&self) -> f32 {
        STEP * self.leg_length
    }

    /// The distance a whole cycle (two steps) covers: the phase advances by distance over this.
    pub fn stride_length(&self) -> f32 {
        2.0 * self.step_length()
    }

    /// A comfortable walking speed, world units a second: a step [`STEPS_PER_SECOND`] times a
    /// second.
    pub fn natural_speed(&self) -> f32 {
        self.step_length() * STEPS_PER_SECOND
    }

    /// How much of the walk a speed shows in [`Gait::pose`]: none standing, all by a third of
    /// the natural speed.
    pub fn walk_share(&self, speed: f32) -> f32 {
        smoothstep(0.02 * self.natural_speed(), 0.35 * self.natural_speed(), speed)
    }

    /// Whether leg `side` is in stance (its foot on the floor) at `phase`.
    pub fn in_stance(&self, phase: f32, side: Side) -> bool {
        self.leg_phase(phase, side) < 0.5
    }

    fn leg_phase(&self, phase: f32, side: Side) -> f32 {
        (phase + if side == Side::Right { 0.0 } else { 0.5 }).rem_euclid(1.0)
    }

    /// The foot's ankle relative to its rest place, in the body's frame, for a leg at `u`
    /// (0 heel strike, 0.5 toe off, 1 heel strike): along, up, and the foot's pitch (toe down).
    fn foot_path(&self, u: f32) -> (f32, f32, f32) {
        let s = self.step_length();
        if u < 0.5 {
            (0.5 * s - 2.0 * s * u, 0.0, 0.0)
        } else {
            let v = (u - 0.5) * 2.0;
            // A Hermite ease that leaves and meets the floor at the stance's own speed.
            let ease = -4.0 * v * v * v + 6.0 * v * v - v;
            (-0.5 * s + s * ease, LIFT * self.leg_length * (PI * v).sin(), 0.4 * self.pitch_k * (TAU * v).sin())
        }
    }

    /// Two-bone IK: the leg from `hip` (world) to the ankle at `target`, the knee forward.
    /// Returns the thigh's and shin's world rotations.
    fn solve(&self, leg: &Leg, hip: V3, target: V3) -> (Mat3, Mat3) {
        let to = vec3::sub(target, hip);
        let max = (leg.upper + leg.lower) * 0.9995;
        let min = (leg.upper - leg.lower).abs() + 0.05;
        let d = vec3::length(to).clamp(min, max);
        let dir = vec3::normalize_or(to, vec3::neg(vec3::Y));
        let a = (leg.upper * leg.upper - leg.lower * leg.lower + d * d) / (2.0 * d);
        let h = (leg.upper * leg.upper - a * a).max(0.0).sqrt();
        let pole = vec3::normalize_or(vec3::sub(vec3::Z, vec3::scale(dir, dir[2])), vec3::X);
        let knee = vec3::add(vec3::add(hip, vec3::scale(dir, a)), vec3::scale(pole, h));
        let ankle = vec3::add(hip, vec3::scale(dir, d));
        let y1 = vec3::normalize_or(vec3::sub(knee, hip), vec3::neg(vec3::Y));
        let y2 = vec3::normalize_or(vec3::sub(ankle, knee), vec3::neg(vec3::Y));
        let r1 = frame(y1).mul_mat(&transpose(&frame(leg.rest[0])));
        let r2 = frame(y2).mul_mat(&transpose(&frame(leg.rest[1])));
        (r1, r2)
    }

    /// How far the stance leg reaches from its hip: nearly straight, but no further than a leg
    /// folded at rest (one on its toes, with the knee well forward and the hock back) stands
    /// at, so it keeps its bend and the body keeps its height.
    fn reach(leg: &Leg) -> f32 {
        (REACH * (leg.upper + leg.lower)).min(1.04 * vec3::distance(leg.hip, leg.ankle))
    }

    /// Sets the legs of `pose` for the pelvis at `at` turned by `pelvis`, each ankle at
    /// `targets` and each foot turned by `feet` (world rotations).
    fn set_legs(&self, pose: &mut RigPose, at: V3, pelvis: &Mat3, targets: [V3; 2], feet: [Mat3; 2]) {
        let pelvis_rest = self.rig.rest_at(Joint::Pelvis);
        for (i, side) in Side::BOTH.into_iter().enumerate() {
            let leg = &self.legs[i];
            let hip = vec3::add(at, pelvis.transform(vec3::sub(leg.hip, pelvis_rest)));
            let (r1, r2) = self.solve(leg, hip, targets[i]);
            let [hj, kj, aj] = Joint::leg(side);
            pose.rotation[hj.index()] = Quat::from_mat3(&transpose(pelvis).mul_mat(&r1));
            pose.rotation[kj.index()] = Quat::from_mat3(&transpose(&r1).mul_mat(&r2));
            pose.rotation[aj.index()] = Quat::from_mat3(&transpose(&r2).mul_mat(&feet[i]));
        }
    }

    /// The pelvis height that puts the ankle of `leg` at `target` at exactly `reach` from its
    /// hip, the pelvis `across` and `along` from its rest place and turned by `pelvis`.
    fn pelvis_height(&self, leg: &Leg, target: V3, x: f32, pelvis: &Mat3, reach: f32) -> f32 {
        let o = pelvis.transform(vec3::sub(leg.hip, self.rig.rest_at(Joint::Pelvis)));
        let horizontal2 = (target[0] - (x + o[0])).powi(2) + (target[2] - o[2]).powi(2);
        let h = (reach * reach - horizontal2).max(0.01).sqrt();
        target[1] + h - o[1]
    }

    /// The walk at `phase` (0 to 1 over two steps, right heel strike at 0) and `speed` (world
    /// units a second), which sets how much the arms swing and the pelvis turns: 1 at
    /// [`Gait::natural_speed`]. Meant to be advanced by distance, `phase += distance /
    /// stride_length`.
    pub fn walk(&self, phase: f32, speed: f32) -> RigPose {
        let k = (speed / self.natural_speed()).clamp(0.5, 1.35);
        let a = phase.rem_euclid(1.0) * TAU;
        let (s, c) = a.sin_cos();
        let mut pose = RigPose::REST;

        let yaw = PELVIS_YAW * k * c;
        let roll = -PELVIS_ROLL * k * s;
        let pelvis = rot(vec3::Y, yaw).mul_mat(&rot(vec3::Z, roll));
        let hip_x = self.legs[0].hip[0].abs();
        let x = -SWAY * hip_x * k * s;

        let mut targets = [vec3::ZERO; 2];
        let mut feet = [Mat3::IDENTITY; 2];
        for (i, side) in Side::BOTH.into_iter().enumerate() {
            let (along, up, pitch) = self.foot_path(self.leg_phase(phase, side));
            let ankle = self.legs[i].ankle;
            targets[i] = [ankle[0], ankle[1] + up, ankle[2] + along];
            feet[i] = rot(vec3::X, pitch);
        }
        let stance = usize::from(!self.in_stance(phase, Side::Right));
        let leg = &self.legs[stance];
        let reach = Gait::reach(leg);
        let y = self.pelvis_height(leg, targets[stance], x, &pelvis, reach);
        let at = [x, y, 0.0];
        self.set_legs(&mut pose, at, &pelvis, targets, feet);
        pose.offset[Joint::Pelvis.index()] = vec3::sub(at, self.rig.rest_at(Joint::Pelvis));
        pose.rotation[Joint::Pelvis.index()] = Quat::from_mat3(&pelvis);

        // The spine counter-twists and levels; the head stays square to the way ahead.
        let (spine_yaw, chest_yaw) = (-0.9 * yaw, -0.6 * yaw);
        pose.rotation[Joint::Spine.index()] = Quat::y(spine_yaw).mul(Quat::z(-0.6 * roll)).mul(Quat::x(0.015));
        pose.rotation[Joint::Chest.index()] = Quat::y(chest_yaw).mul(Quat::z(-0.3 * roll)).mul(Quat::x(0.015));
        let net = yaw + spine_yaw + chest_yaw;
        pose.rotation[Joint::Head.index()] = Quat::y(-0.8 * net).mul(Quat::z(0.5 * roll)).mul(Quat::x(-0.02));

        // Arms swing opposite the legs.
        for (i, side) in Side::BOTH.into_iter().enumerate() {
            let sign = if side == Side::Right { 1.0 } else { -1.0 };
            let free = 1.0 - self.hold[i] * if self.hold[1 - i] > 0.9 { 1.0 } else { 0.85 };
            let swing = sign * ARM_SWING * k * c * free;
            let forward = (-sign * c).max(0.0);
            let [shoulder, elbow, _] = Joint::arm(side);
            pose.rotation[shoulder.index()] = Quat::x(swing);
            pose.rotation[elbow.index()] = Quat::x(-(0.12 + 0.4 * forward) * k * free);
        }
        // A tail swings against the hips, each bone a little behind the one before.
        for i in 0..self.tail {
            pose.rotation[Joint::TAIL[i].index()] = Quat::y(0.2 * k * (a + 1.2 - 0.8 * i as f32).sin());
        }
        pose
    }

    /// Standing at `t` seconds: breathing, and now and then shifting weight between the feet.
    pub fn idle(&self, cycle: &IdleCycle, t: f32) -> RigPose {
        let breath = cycle.breath(t);
        let shift = cycle.shift(t);
        let mut pose = RigPose::REST;

        let roll = SHIFT_ROLL * shift;
        let pelvis = rot(vec3::Z, roll);
        let x = SHIFT_SWAY * self.scale * shift;
        // Over the left foot (shift > 0) the right relaxes: forward and a little out.
        let relax = shift.abs();
        let mut targets = [self.legs[0].ankle, self.legs[1].ankle];
        let loose = usize::from(shift < 0.0);
        let side = if loose == 0 { -1.0 } else { 1.0 };
        targets[loose] = vec3::add(targets[loose], [side * 0.05 * self.scale * relax, 0.0, 0.22 * self.scale * relax]);
        let loaded = 1 - loose;
        let leg = &self.legs[loaded];
        let reach = vec3::distance(leg.hip, leg.ankle);
        let y = self.pelvis_height(leg, targets[loaded], x, &pelvis, reach);
        let at = [x, y, 0.0];
        self.set_legs(&mut pose, at, &pelvis, targets, [Mat3::IDENTITY; 2]);
        pose.offset[Joint::Pelvis.index()] = vec3::sub(at, self.rig.rest_at(Joint::Pelvis));
        pose.rotation[Joint::Pelvis.index()] = Quat::from_mat3(&pelvis);

        pose.rotation[Joint::Spine.index()] = Quat::z(-0.7 * roll);
        pose.rotation[Joint::Chest.index()] = Quat::z(-0.2 * roll).mul(Quat::x(-0.012 * breath));
        pose.rotation[Joint::Head.index()] = Quat::z(-0.1 * roll);
        // A breath lifts the chest and the shoulders hanging from it.
        pose.offset[Joint::Chest.index()] = [0.0, BREATH_LIFT * self.height * breath, 0.0];
        // A tail sways slowly, now and then flicking.
        for i in 0..self.tail {
            pose.rotation[Joint::TAIL[i].index()] = Quat::y(0.09 * (TAU * 0.35 * t + cycle.shift_phase - 0.9 * i as f32).sin());
        }
        pose
    }

    /// The pose of someone moving at `speed` (world units a second), `phase` through the
    /// walk, standing otherwise: the walk and the idle blended by [`Gait::walk_share`].
    pub fn pose(&self, phase: f32, speed: f32, cycle: &IdleCycle, t: f32) -> RigPose {
        let share = self.walk_share(speed);
        let idle = self.idle(cycle, t);
        if share <= 0.0 {
            return idle;
        }
        let walk = self.walk(phase, speed);
        if share >= 1.0 { walk } else { idle.blend(&walk, share) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BodySpec, Folk};

    fn rigged(folk: Folk, seed: u32) -> (Body, Rig, Gait) {
        let body = Body::new(&BodySpec::roll(folk, seed));
        let rig = Rig::of(&body);
        let gait = Gait::new(&body, &rig);
        (body, rig, gait)
    }

    fn ankle(rig: &Rig, pose: &RigPose, side: Side) -> V3 {
        rig.world(pose)[Joint::leg(side)[2].index()].origin
    }

    #[test]
    fn the_idle_at_no_shift_is_the_rest_pose() {
        let (_, rig, gait) = rigged(Folk::Human, 1207);
        // A cycle that does not shift or breathe at t = 0.
        let cycle = IdleCycle { breath_rate: 0.0, breath_phase: -PI / 2.0, shift_rate: 0.0, shift_phase: 0.0 };
        let pose = gait.idle(&cycle, 0.0);
        let frames = rig.skin_frames(&pose);
        for (joint, f) in Joint::ALL.into_iter().zip(frames) {
            assert!(vec3::distance(f.point([1.0, 5.0, 2.0]), [1.0, 5.0, 2.0]) < 0.02, "{joint:?} {:?} {:?}", f.origin, pose.rotation[joint.index()]);
        }
    }

    #[test]
    fn the_stance_foot_is_planted_flat_on_the_floor() {
        for folk in Folk::ALL {
            let (_, rig, gait) = rigged(folk, 1207);
            let stride = gait.stride_length();
            for side in Side::BOTH {
                let rest = gait.legs[usize::from(side == Side::Left)].ankle;
                let mut planted: Option<V3> = None;
                for i in 0..200 {
                    let phase = i as f32 / 200.0;
                    if !gait.in_stance(phase, side) {
                        planted = None;
                        continue;
                    }
                    let pose = gait.walk(phase, gait.natural_speed());
                    let a = ankle(&rig, &pose, side);
                    // The ankle holds its height, and in the world (the body's frame carried on
                    // by the distance walked) it does not move.
                    assert!((a[1] - rest[1]).abs() < 0.02, "{folk:?} {side:?} {phase}: ankle {a:?} rest {rest:?}");
                    let world = [a[0], a[1], a[2] + stride * phase];
                    if let Some(p) = planted {
                        assert!(vec3::distance(p, world) < 0.03, "{folk:?} {side:?} {phase}: slid {p:?} -> {world:?}");
                    }
                    planted = Some(world);
                    // And flat: the foot's own rotation leaves the sole level.
                    let foot = rig.world(&pose)[Joint::leg(side)[2].index()].rotation;
                    assert!(foot.cols[2][1].abs() < 0.02 && foot.cols[1][1] > 0.999, "{folk:?} {side:?} {phase}: foot {:?}", foot.cols);
                }
            }
        }
    }

    #[test]
    fn the_swing_foot_clears_the_floor_and_the_legs_stay_reachable() {
        for folk in Folk::ALL {
            let (_, rig, gait) = rigged(folk, 1207);
            let mut highest = 0.0f32;
            for i in 0..100 {
                let phase = i as f32 / 100.0;
                let pose = gait.walk(phase, gait.natural_speed());
                for side in Side::BOTH {
                    let leg = &gait.legs[usize::from(side == Side::Left)];
                    if !gait.in_stance(phase, side) {
                        highest = highest.max(ankle(&rig, &pose, side)[1] - leg.ankle[1]);
                    }
                    // Never below the floor.
                    assert!(ankle(&rig, &pose, side)[1] > leg.ankle[1] - 0.02, "{folk:?} {phase}");
                }
            }
            assert!(highest > 0.05 * gait.leg_length, "{folk:?}: lifts {highest}");
        }
    }

    #[test]
    fn a_short_legged_folk_takes_a_short_step() {
        let (_, _, human) = rigged(Folk::Human, 7);
        let (_, _, dwarf) = rigged(Folk::Dwarf, 7);
        let (_, _, goblin) = rigged(Folk::Goblin, 7);
        assert!(dwarf.step_length() < human.step_length());
        assert!(dwarf.leg_length < human.leg_length);
        // A step is a fixed share of the leg whatever the folk.
        for g in [&human, &dwarf, &goblin] {
            assert!((g.step_length() / g.leg_length - STEP).abs() < 1e-4);
        }
    }

    #[test]
    fn opposite_arms_swing_and_a_held_arm_swings_less() {
        let (_, rig, gait) = rigged(Folk::Human, 3);
        let hand = |pose: &RigPose, side| rig.world(pose)[Joint::wrist(side).index()].origin[2];
        let (a, b) = (gait.walk(0.0, gait.natural_speed()), gait.walk(0.5, gait.natural_speed()));
        let right = hand(&b, Side::Right) - hand(&a, Side::Right);
        let left = hand(&b, Side::Left) - hand(&a, Side::Left);
        assert!(right * left < 0.0 && right.abs() > 0.5, "right {right}, left {left}");
        let spec = BodySpec::roll(Folk::Human, 3);
        let sword = fb_items::roll_item("Sword", 3).unwrap();
        let hands = crate::fit_hands(&spec, Some(&sword.shape), None);
        let rig = Rig::of(&hands.body);
        let gait = Gait::new(&hands.body, &rig);
        let hand = |pose: &RigPose, side| rig.world(pose)[Joint::wrist(side).index()].origin[2];
        let (a, b) = (gait.walk(0.0, gait.natural_speed()), gait.walk(0.5, gait.natural_speed()));
        let held = (hand(&b, Side::Right) - hand(&a, Side::Right)).abs();
        let free = (hand(&b, Side::Left) - hand(&a, Side::Left)).abs();
        assert!(held < free * 0.5, "held {held}, free {free}");
    }

    #[test]
    fn the_pelvis_bobs_twice_a_cycle() {
        let (_, _, gait) = rigged(Folk::Human, 1207);
        let height = |phase: f32| gait.walk(phase, gait.natural_speed()).offset[Joint::Pelvis.index()][1];
        // Highest mid-stance (a quarter and three quarters), lowest at the heel strikes.
        assert!(height(0.25) > height(0.0) + 0.1 * gait.scale * 0.1);
        assert!((height(0.25) - height(0.75)).abs() < 0.02);
        assert!((height(0.0) - height(0.5)).abs() < 0.02);
    }

    #[test]
    fn the_same_inputs_give_the_same_pose() {
        let (_, _, a) = rigged(Folk::Gnome, 9);
        let (_, _, b) = rigged(Folk::Gnome, 9);
        let cycle = IdleCycle::of(9);
        assert_eq!(a.pose(0.3, 5.0, &cycle, 2.0), b.pose(0.3, 5.0, &cycle, 2.0));
        assert_eq!(IdleCycle::of(9), IdleCycle::of(9));
    }

    #[test]
    fn standing_shows_none_of_the_walk_and_speed_blends_it_in() {
        let (_, _, gait) = rigged(Folk::Human, 1);
        let cycle = IdleCycle::of(1);
        assert_eq!(gait.walk_share(0.0), 0.0);
        assert_eq!(gait.walk_share(gait.natural_speed()), 1.0);
        assert_eq!(gait.pose(0.4, 0.0, &cycle, 1.0), gait.idle(&cycle, 1.0));
        assert_eq!(gait.pose(0.4, gait.natural_speed(), &cycle, 1.0), gait.walk(0.4, gait.natural_speed()));
        let mid = gait.pose(0.4, 0.15 * gait.natural_speed(), &cycle, 1.0);
        assert_ne!(mid, gait.idle(&cycle, 1.0));
    }
}
