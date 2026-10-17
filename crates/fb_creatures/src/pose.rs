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
//! - [`skeleton`]: a human's walk and a swing of the right arm. Each leg is solved with a
//!   two-bone reach ([`two_bone`]'s cousin in the walking plane) so the planted foot rests on
//!   the floor exactly; the pelvis rides at the height the lower of the two reaches allows (a smooth minimum), so
//!   it dips as the weight comes onto a leading leg and rises over a standing one. The arms
//!   swing against the legs, the ribcage turns against the hips; a swing winds the right arm up
//!   over the shoulder, throws it through the blow and brings it back, turning the ribcage and
//!   leaning into it with the feet planted.
//!
//! The gait's timing, the wave's shape and every amplitude here are our own tuning.

use armory::geom::vec3::{self, Mat3, V3};
use fb_body::{Placement, Side};
use std::f32::consts::{PI, TAU};

use crate::rat::{LEGS, RatState, TAIL_BONES, bone as rat_bone};
use crate::shape::{keyed, spine_to_creature};
use crate::skeleton::{SkeletonDims, SkeletonState, arm_bone, index as skeleton_bone, leg_bone};
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
    /// The way it came: how far the body's heading (the way it runs toward the head) turns
    /// toward its left (+X) from straight ahead, radians, at [`BEND_KNOTS`] even steps from the neck (first) to the tail's tip
    /// (last), so a laid-out body follows the path its front took rather than swinging round
    /// as a whole. Ignored as it coils.
    pub bend: [f32; BEND_KNOTS],
}

/// Knots along the body in [`SnakePose::bend`].
pub const BEND_KNOTS: usize = 9;

impl SnakePose {
    /// Coiled, the head raised a little in the middle: how a snake rests and waits.
    pub const REST: SnakePose = SnakePose { phase: 0.0, pace: 0.0, coil: 1.0, rear: 0.45, strike: 0.0, jaw: 0.0, tongue: 0.0, look: 0.0, bend: [0.0; BEND_KNOTS] };

    /// The bend at `u` along the body (0 the neck, 1 the tail's tip), between its knots.
    pub fn bend_at(&self, u: f32) -> f32 {
        let x = u.clamp(0.0, 1.0) * (BEND_KNOTS - 1) as f32;
        let k = (x.floor() as usize).min(BEND_KNOTS - 2);
        let f = x - k as f32;
        self.bend[k] * (1.0 - f) + self.bend[k + 1] * f
    }
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
/// How high the front rears at full height, as a share of the neck's length: a viper stands
/// tall, a constrictor's heavier body less so. Our own, set so both raise their head past a
/// human's knee ([`KNEE`]).
const VIPER_REAR: f32 = 0.82;
const CONSTRICTOR_REAR: f32 = 0.6;
/// The steepest the rearing neck climbs, as the sine of its angle off the floor (about 72°):
/// a column, not a vertical stick.
const MAX_CLIMB: f32 = 0.95;
/// A human's knee height in world units: 0.285 of a 17-unit stature (Drillis and Contini's
/// segment proportions, as tabled in D. A. Winter, *Biomechanics and Motor Control of Human
/// Movement*, 4th ed., Wiley 2009, fig. 4.1). A rearing snake raises its head at least this
/// high.
pub const KNEE: f32 = 0.285 * 17.0;

/// Where the snake's spine lies for `pose`: the centre line's point and heading at `samples`
/// distances from the neck (0) to the tail tip, each `(point, toward the head)`, in creature
/// space: coiled, the body's middle (behind the neck) on the origin; laid out, the neck on it
/// and the body trailing behind.
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
    // Heading at distance `s` from the neck, laid out (the slither along the way it came) or
    // coiled (the spiral), with the rearing S in the neck that a strike straightens.
    let heading = |s: f32, coiled: bool| {
        let s_curve = if s < neck { NECK_ANGLE * raise * (1.0 - strike) * (TAU * s / neck).sin() * (1.0 - s / neck) } else { 0.0 };
        let lie = if coiled {
            state.hand * ((a * a + 2.0 * b * s).sqrt() - a) / b
        } else {
            amplitude * (TAU * s / lambda - pose.phase).sin() + pose.bend_at(s / body)
        };
        lie + s_curve
    };
    // Height of the centre line: belly on the floor, the neck lifted when rearing, in an
    // S of its own: level under the head, a steep column, then down to the floor.
    let lift_height = neck * if d.constrictor { CONSTRICTOR_REAR } else { VIPER_REAR } * raise * (1.0 - 0.35 * strike);
    let profile = |s: f32| lift_height * (1.0 - smoothstep(s / neck));
    // The head is deeper than the neck: the neck rises to carry it clear of the floor.
    let head_clear = (state.head_depth() - SnakeState::belly_depth(state.radius_at(0.0))).max(0.0);
    let clear = |s: f32| head_clear * (1.0 - smoothstep(s / (1.5 * d.segment)));
    // Built up from the tail toward the head, never climbing steeper than [`MAX_CLIMB`], so
    // every step keeps its length however high the front rears.
    let mut lift = vec![0.0f32; steps + 2];
    for i in (0..=steps).rev() {
        let (s, behind) = (i as f32 * step, (i + 1) as f32 * step);
        let climb = (profile(s) - profile(behind)).clamp(-MAX_CLIMB * step, MAX_CLIMB * step);
        lift[i] = lift[i + 1] + climb;
    }
    for (i, l) in lift.iter_mut().enumerate() {
        *l = l.max(clear(i as f32 * step));
    }
    // The centre line walked back from the neck along one way of lying.
    let walk = |coiled: bool| {
        let mut points: Vec<V3> = Vec::with_capacity(samples);
        let mut p = [0.0f32, 0.0];
        for i in 0..=steps {
            let s = i as f32 * step;
            let r = state.radius_at(s / body);
            points.push([p[0], SnakeState::belly_depth(r) + lift[i], p[1]]);
            // Step back along the body, a level step shortened by the climb.
            let climb = lift[i + 1] - lift[i];
            let level = (step * step - climb * climb).max(0.09 * step * step).sqrt();
            let h = heading(s + 0.5 * step, coiled);
            p = [p[0] - level * h.sin(), p[1] - level * h.cos()];
        }
        points
    };
    // Laid out, the neck leads from the origin and the body trails behind it, so whatever
    // moves the creature moves its head, and the body follows the way the head went
    // ([`SnakePose::bend`]). Coiled, the body behind the neck holds its place: its middle on
    // the origin.
    let laid = walk(false);
    let mut coiled = walk(true);
    let behind = (neck / step).ceil() as usize;
    let mid = coiled[behind.min(steps)..].iter().fold([0.0f32; 2], |m, p| [m[0] + p[0], m[1] + p[2]]);
    let n = (steps + 1).saturating_sub(behind).max(1) as f32;
    for point in &mut coiled {
        point[0] -= mid[0] / n;
        point[2] -= mid[1] / n;
    }
    // Between the two, each point slides from one to the other, so a body coiling or uncoiling
    // stays between where it lay and where it is going rather than sweeping round as its
    // headings blend. Our own choice.
    let coil = pose.coil.clamp(0.0, 1.0);
    let mut points: Vec<V3> = laid.iter().zip(&coiled).map(|(l, c)| [l[0] + coil * (c[0] - l[0]), l[1], l[2] + coil * (c[2] - l[2])]).collect();
    // A strike throws the neck forward along its own line, beyond what straightening gives.
    let throw = 1.6 * d.girth * strike;
    for (i, point) in points.iter_mut().enumerate() {
        let s = i as f32 * step;
        if s < neck {
            point[2] += throw * (1.0 - s / neck);
        }
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
                                let pose = SnakePose { phase: k as f32, pace: 1.0, coil, rear, strike, jaw: strike * rear, tongue: 0.5, look: 0.0, bend: [0.0; BEND_KNOTS] };
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

    #[test]
    fn a_rearing_snake_raises_its_head_past_the_knee() {
        for species in [Species::PoisonousSnake, Species::ConstrictorSnake] {
            for seed in 0..24 {
                let state = SnakeState::roll_as(seed, species);
                for coil in [0.0, 1.0] {
                    let head = snake(&state, &SnakePose { rear: 1.0, coil, ..SnakePose::REST })[snake_bone::HEAD];
                    assert!(head.origin[1] >= KNEE, "{species:?} {seed} (coil {coil}): head at {}", head.origin[1]);
                    // Held level, hooded over the column of its neck, not pointing at the roof.
                    let snout = head.rotation.transform([0.0, 1.0, 0.0]);
                    assert!(snout[1].abs() < 0.5, "{species:?} {seed}: snout {snout:?}");
                }
            }
        }
    }

    #[test]
    fn a_laid_out_body_follows_the_bend_it_is_given() {
        let state = SnakeState::roll_as(3, Species::ConstrictorSnake);
        let laid = SnakePose { coil: 0.0, rear: 0.0, pace: 0.0, ..SnakePose::REST };
        // The way it came curves off to its left behind the neck.
        let mut bend = [0.0; BEND_KNOTS];
        for (k, b) in bend.iter_mut().enumerate() {
            *b = 0.9 * k as f32 / (BEND_KNOTS - 1) as f32;
        }
        let straight = snake_spine(&state, &laid, 41);
        let curved = snake_spine(&state, &SnakePose { bend, ..laid }, 41);
        let tail = |spine: &[(V3, V3)]| vec3::sub(spine[40].0, spine[0].0);
        // The body behind ran toward its left (+X) to reach the neck, so the tail lies off to
        // its right (-X).
        assert!(tail(&straight)[0] - tail(&curved)[0] > 0.2 * state.length, "{:?} vs {:?}", tail(&curved), tail(&straight));
        // Coiled, the bend is ignored.
        let (bent, rest) = (snake_spine(&state, &SnakePose { bend, ..SnakePose::REST }, 41), snake_spine(&state, &SnakePose::REST, 41));
        assert!(bent.iter().zip(&rest).all(|(a, b)| vec3::distance(a.0, b.0) < 1e-3));
    }

    #[test]
    fn a_fallen_skeleton_has_its_knees_up_and_its_arms_out() {
        use crate::skeleton::SkeletonState;
        let state = SkeletonState::roll(5);
        let d = state.dims();
        let standing = skeleton_with(&d, &SkeletonPose::default());
        // Not yet falling, it stands as it did, to within its breathing and sway.
        let start = skeleton_fallen(&d, 0.0);
        for (a, b) in start.iter().zip(&standing) {
            let off = (0..3).flat_map(|i| (0..3).map(move |j| (i, j))).map(|(i, j)| (a.rotation.cols[i][j] - b.rotation.cols[i][j]).abs()).fold(0.0, f32::max);
            assert!(off < 0.08, "{off}");
            assert_eq!(a.origin, b.origin);
        }
        let down = skeleton_fallen(&d, 1.0);
        assert_eq!(down.len(), standing.len());
        let thigh = leg_bone(Side::Right);
        // The right thigh is drawn up and forward from hanging straight down.
        let thigh_dir = down[thigh].rotation.transform([0.0, -1.0, 0.0]);
        assert!(thigh_dir[2] > 0.5, "{thigh_dir:?}");
        let arm = arm_bone(Side::Right);
        let arm_dir = down[arm].rotation.transform([0.0, -1.0, 0.0]);
        assert!(arm_dir[0] < -0.3, "flung out to its right, {arm_dir:?}");
    }
}


/// How a skeleton is moving at a moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SkeletonPose {
    /// The step cycle, radians: one turn per [`skeleton_stride`] walked.
    pub stride: f32,
    /// Speed as a share of its walking speed: 0 standing, 1 walking.
    pub pace: f32,
    /// Seconds, for what it does standing: breathing and the head's idle sway.
    pub time: f32,
    /// How far through a swing of the right arm it is, 0 at the guard, through the wind-up and
    /// the blow to 1 back at the guard; `None` for no swing. A swing plants both feet.
    pub swing: Option<f32>,
    /// How far the head turns toward its left (+X), radians.
    pub look: f32,
    /// The jaw's gape, 0 shut to 1 wide.
    pub jaw: f32,
}

/// The ground a skeleton covers in one full step cycle (two steps): a step is 0.7 of its leg.
pub fn skeleton_stride(dims: &SkeletonDims) -> f32 {
    2.0 * 0.7 * (dims.thigh + dims.shin)
}

/// A rotation about +X in body axes: positive swings a limb hanging down toward the back.
fn rot_x(a: f32) -> Mat3 {
    Mat3::from_axis_angle([1.0, 0.0, 0.0], a)
}

fn rot_y(a: f32) -> Mat3 {
    Mat3::from_axis_angle([0.0, 1.0, 0.0], a)
}

/// The turns about X of a leg's thigh and shin, relative to their rest, that bring the ankle
/// to `target`, `(forward, up)` from the hip in the walking plane. The knee bends forward.
fn leg_turns(dims: &SkeletonDims, k: usize, target: [f32; 2]) -> (f32, f32) {
    let [hip, knee, ankle] = dims.legs[k];
    let (l1, l2) = (dims.thigh, dims.shin);
    let rest1 = (knee[2] - hip[2]).atan2(hip[1] - knee[1]);
    let rest2 = (ankle[2] - knee[2]).atan2(knee[1] - ankle[1]);
    let reach = target[0].hypot(target[1]).clamp((l1 - l2).abs() + 1e-3, (l1 + l2) * 0.9999);
    let toward = target[0].atan2(-target[1]);
    let at_hip = ((l1 * l1 + reach * reach - l2 * l2) / (2.0 * l1 * reach)).clamp(-1.0, 1.0).acos();
    let at_ankle = ((l2 * l2 + reach * reach - l1 * l1) / (2.0 * l2 * reach)).clamp(-1.0, 1.0).acos();
    let (thigh, shin) = (toward + at_hip, toward - at_ankle);
    (rest1 - thigh, rest2 - shin - (rest1 - thigh))
}

/// Every bone's placement on its parent for `pose`, in the skeleton's [`crate::Rig`] order.
/// Builds the skeleton's dimensions each call; the game keeps them ([`SkeletonState::dims`])
/// and calls [`skeleton_with`].
pub fn skeleton(state: &SkeletonState, pose: &SkeletonPose) -> Vec<Placement> {
    skeleton_with(&state.dims(), pose)
}

/// [`skeleton`] on dimensions already measured.
pub fn skeleton_with(d: &SkeletonDims, pose: &SkeletonPose) -> Vec<Placement> {
    skeleton_armed(d, pose, &[None, None])
}

/// A skeleton `share` of the way (0 to 1) through collapsing onto its back, for the bones: the
/// standing pose with the knees drawn up, the arms flung out, the head fallen back and the
/// jaw slack. The body as a whole is laid back by whatever holds it (the game turns the root
/// about the feet); this is how its limbs go as it falls. Our own.
pub fn skeleton_fallen(d: &SkeletonDims, share: f32) -> Vec<Placement> {
    use skeleton_bone as bone;
    let s = share.clamp(0.0, 1.0);
    let s = s * s * (3.0 - 2.0 * s);
    let mut out = skeleton_with(d, &SkeletonPose::default());
    let rot_z = |a: f32| Mat3::from_axis_angle([0.0, 0.0, 1.0], a);
    for (k, side) in Side::BOTH.into_iter().enumerate() {
        // The right side lies toward -X: its limbs turn that way, the left's the other.
        let out_way = if k == 0 { -1.0 } else { 1.0 };
        let l = leg_bone(side);
        out[l].rotation = rot_x(-(0.9 + 0.25 * k as f32) * s).mul_mat(&rot_z(out_way * 0.25 * s));
        out[l + 1].rotation = rot_x((1.6 - 0.3 * k as f32) * s);
        out[l + 2].rotation = rot_x(-0.3 * s);
        let a = arm_bone(side);
        out[a].rotation = rot_x(-0.25 * s).mul_mat(&rot_z(out_way * (0.7 + 0.3 * k as f32) * s));
        out[a + 1].rotation = rot_x(-(0.3 + 0.4 * k as f32) * s);
    }
    out[bone::SPINE].rotation = rot_x(-0.12 * s);
    out[bone::NECK].rotation = rot_x(0.35 * s);
    out[bone::SKULL].rotation = rot_y(0.4 * s).mul_mat(&rot_x(0.25 * s));
    out[bone::JAW].rotation = rot_x(0.5 * 0.7 * s);
    out
}

/// How an arm that holds something is set: the turns (each on its parent, from the rest pose) of
/// its upper arm, forearm and hand that bring the hand to where `fb_body` fitted what it holds,
/// with the fingers closed on it. See `crate::gear`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArmStance {
    pub upper: Mat3,
    pub fore: Mat3,
    pub hand: Mat3,
}

/// [`skeleton_with`] with the arms that hold something (right then left) in their stance. A
/// holding arm does not swing as it walks; the right arm's swing still drives its upper arm and
/// forearm (the hand keeps its wrist, so the weapon stays in the fist), and the left arm braces
/// its shield through a swing, held where it is.
pub fn skeleton_armed(d: &SkeletonDims, pose: &SkeletonPose, stance: &[Option<ArmStance>; 2]) -> Vec<Placement> {
    use skeleton_bone as bone;
    let mut out: Vec<Placement> = (0..bone::COUNT).map(|b| Placement::translation(d.offset(b))).collect();
    let swinging = pose.swing.map(|p| p.clamp(0.0, 1.0));
    let moving = if swinging.is_some() { 0.0 } else { pose.pace.clamp(0.0, 1.0) };
    let t = pose.time;
    let leg_len = d.thigh + d.shin;
    let stride_len = skeleton_stride(d);
    let sweep = DUTY * stride_len;
    let clearance = 0.1 * leg_len;

    // Where each ankle is: forward of its hip, and lifted off the floor. Right then left.
    let mut dz = [0.0f32; 2];
    let mut lift = [0.0f32; 2];
    let mut pelvis_z = 0.0;
    match swinging {
        Some(p) => {
            // A lunge: the left foot forward, the right back, the weight going back then onto the front foot.
            pelvis_z = keyed(&[(0.0, 0.0), (0.3, -0.35), (0.5, 0.2), (0.62, 0.75), (0.85, 0.4), (1.0, 0.0)], p);
            let feet = [-1.3, 1.1];
            dz = [feet[0] - pelvis_z, feet[1] - pelvis_z];
        }
        None => {
            for k in 0..2 {
                // The right and left legs half a cycle apart.
                let c = (pose.stride / TAU + if k == 0 { 0.5 } else { 0.0 }).rem_euclid(1.0);
                let (along, up) = if c < DUTY {
                    (sweep * (0.5 - c / DUTY), 0.0)
                } else {
                    let u = (c - DUTY) / (1.0 - DUTY);
                    (sweep * (smoothstep(u) - 0.5), clearance * (PI * u).sin())
                };
                dz[k] = along * moving;
                lift[k] = up * moving;
            }
        }
    }

    // The pelvis rides where both ankles are within reach, a soft minimum so the dip as the
    // weight changes legs is rounded.
    let bend = if swinging.is_some() { 0.04 } else { 0.015 * moving };
    let reach = (1.0 - bend) * leg_len;
    let a0 = d.ankles[0][1];
    let limit = |k: usize| a0 + lift[k] + (reach * reach - dz[k] * dz[k]).max(0.0).sqrt();
    let hip_y = d.legs[0][0][1];
    let (h0, h1) = (limit(0), limit(1));
    // The polynomial smooth minimum (Inigo Quilez), never above either reach.
    let round = 0.4 * moving;
    let blend = if round > 0.0 { (round - (h0 - h1).abs()).max(0.0) / round } else { 0.0 };
    let height = (h0.min(h1) - blend * blend * round * 0.25).min(hip_y);
    out[bone::PELVIS] = Placement { rotation: Mat3::IDENTITY, origin: [0.0, height, pelvis_z] };

    // Legs: solve each to its ankle, the foot kept level.
    for (k, side) in Side::BOTH.into_iter().enumerate() {
        let target = [dz[k], a0 + lift[k] - height];
        let (thigh, shin) = leg_turns(d, k, target);
        let b = leg_bone(side);
        out[b].rotation = rot_x(thigh);
        out[b + 1].rotation = rot_x(shin);
        out[b + 2].rotation = rot_x(-(thigh + shin));
    }

    // Trunk, head and arms.
    let breathe = (1.7 * t).sin();
    let mut lean = 0.02 * breathe + 0.05 * moving;
    let mut twist = 0.1 * moving * (pose.stride).sin();
    let mut jaw = pose.jaw;
    let mut arm_forward = [0.0f32; 2];
    let mut arm_bend = [0.0f32; 2];
    match swinging {
        Some(p) => {
            lean = keyed(&[(0.0, 0.05), (0.3, -0.08), (0.45, -0.1), (0.62, 0.32), (0.78, 0.22), (1.0, 0.05)], p);
            twist = keyed(&[(0.0, 0.0), (0.3, -0.55), (0.45, -0.62), (0.62, 0.5), (0.78, 0.5), (1.0, 0.0)], p);
            jaw = jaw.max(keyed(&[(0.0, 0.0), (0.3, 0.25), (0.45, 0.4), (0.62, 0.7), (0.8, 0.2), (1.0, 0.0)], p));
            arm_forward[0] = keyed(&[(0.0, 0.5), (0.14, 0.9), (0.3, 2.5), (0.45, 2.6), (0.62, 0.95), (0.78, 0.35), (1.0, 0.5)], p);
            arm_bend[0] = keyed(&[(0.0, 1.5), (0.3, 1.9), (0.45, 2.0), (0.62, 0.35), (0.78, 0.3), (1.0, 1.5)], p);
            // The left arm braces across the chest.
            arm_forward[1] = 0.25 + 0.1 * (p * TAU).sin();
            arm_bend[1] = 0.7;
        }
        None => {
            // Arms swing against the legs: the right arm forward as the left leg is.
            for k in 0..2 {
                let n = (dz[1 - k] / (0.5 * sweep.max(1e-3))).clamp(-1.0, 1.0);
                arm_forward[k] = 0.4 * n * moving + 0.02 * (1.3 * t + k as f32 * 2.0).sin() * (1.0 - moving);
                arm_bend[k] = (0.12 + 0.25 * n.max(0.0)) * moving + 0.03 * (1.0 - moving);
            }
        }
    }
    out[bone::SPINE].rotation = rot_x(0.5 * lean).mul_mat(&rot_y(0.3 * twist));
    out[bone::RIBCAGE].rotation = rot_x(0.5 * lean).mul_mat(&rot_y(0.7 * twist));
    // The head keeps to where it looks, whatever the trunk turns.
    let sway = 0.05 * (0.7 * t).sin() * (1.0 - moving);
    out[bone::NECK].rotation = rot_x(-0.5 * lean);
    out[bone::SKULL].rotation = rot_y(pose.look + sway - twist).mul_mat(&rot_x(-0.04 + 0.01 * breathe - 0.3 * lean));
    out[bone::JAW].rotation = rot_x(0.5 * jaw);
    for (k, side) in Side::BOTH.into_iter().enumerate() {
        let b = arm_bone(side);
        out[b].rotation = rot_x(-arm_forward[k]);
        out[b + 1].rotation = rot_x(-arm_bend[k]);
        if let Some(held) = &stance[k] {
            if swinging.is_none() || k == 1 {
                out[b].rotation = held.upper;
                out[b + 1].rotation = held.fore;
            }
            out[b + 2].rotation = held.hand;
        }
    }
    out
}

use crate::spider::{LEG_COUNT, LEG_PAIRS, SpiderState, bone as spider_bone, leg_side};

/// How a giant spider is moving at a moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpiderPose {
    /// The step cycle, radians: one turn per [`spider_stride`] walked.
    pub stride: f32,
    /// Speed as a share of its walking speed: 0 standing, 1 walking, 2 running.
    pub pace: f32,
    /// Seconds, for what it does standing: breathing, and the pedipalps' fidgeting.
    pub time: f32,
    /// 0 on all eight legs, 1 reared up on the back four with the front legs raised.
    pub rear: f32,
    /// A bite's stab, 0 to 1 at full stretch: the body lunges, the fangs open and swing in.
    pub bite: f32,
    /// 0 alive, 1 dead: the legs drawn in under the belly.
    pub curl: f32,
}

/// The ground a spider covers in one full step cycle: a third of its leg span.
pub fn spider_stride(state: &SpiderState) -> f32 {
    0.3 * state.span
}

/// Share of the cycle each foot is on the floor, so at least four are down at any moment.
const SPIDER_DUTY: f32 = 0.62;
/// Each leg's offset in the cycle, in bone order (front pair first, left before right): an
/// alternating tetrapod, legs L1 R2 L3 R4 moving together and R1 L2 R3 L4 against them.
pub const SPIDER_PHASE: [f32; LEG_COUNT] = [0.0, PI, PI, 0.0, 0.0, PI, PI, 0.0];
/// How far each pair is raised by a full rear: the front pair held up, the rest planted.
const RAISED: [f32; LEG_PAIRS] = [1.0, 0.0, 0.0, 0.0];
/// How far a planted tarsus leans outward from straight down, radians.
const TARSUS_LEAN: f32 = 0.43;
/// How high the body is tipped, nose up, when fully reared, radians.
const REAR_PITCH: f32 = 0.6;
/// The belly and abdomen never come nearer the floor than this share of the leg span.
const BELLY_CLEARANCE: f32 = 0.02;

/// The transpose of `m`: its inverse for a rotation.
fn transpose(m: &Mat3) -> Mat3 {
    let [x, y, z] = m.cols;
    Mat3::from_cols([x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]])
}

/// A bone's rotation with its +Y along `dir` and its +Z as near `hint` as that allows.
fn aim(dir: V3, hint: V3) -> Mat3 {
    let y = vec3::normalize_or(dir, vec3::Y);
    let z = vec3::normalize_or(vec3::sub(hint, vec3::scale(y, vec3::dot(hint, y))), vec3::any_perpendicular(y));
    Mat3::from_cols(vec3::cross(y, z), y, z)
}

/// The knee of a leg of two bones `upper` and `lower` long reaching from `hip` toward `ankle`,
/// bent toward `hint`, and the ankle itself pulled in to what the bones can reach: a spider's
/// three-dimensional counterpart of [`two_bone`].
pub fn knee_for(hip: V3, ankle: V3, upper: f32, lower: f32, hint: V3) -> (V3, V3) {
    let to = vec3::sub(ankle, hip);
    let d = vec3::length(to).clamp((upper - lower).abs() + 1e-3, (upper + lower) * 0.999);
    let dir = vec3::normalize_or(to, vec3::Y);
    let cos_a = ((upper * upper + d * d - lower * lower) / (2.0 * upper * d)).clamp(-1.0, 1.0);
    let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();
    let n = vec3::normalize_or(vec3::sub(hint, vec3::scale(dir, vec3::dot(hint, dir))), vec3::any_perpendicular(dir));
    let knee = vec3::add(hip, vec3::add(vec3::scale(dir, upper * cos_a), vec3::scale(n, upper * sin_a)));
    (knee, vec3::add(hip, vec3::scale(dir, d)))
}

/// The depth below its centre of an ellipsoid with half axes `half` along the axes of a frame
/// turned by `rotation`: its lowest point under the world's up.
fn ellipsoid_drop(rotation: &Mat3, half: V3) -> f32 {
    let up = transpose(rotation).transform([0.0, 1.0, 0.0]);
    ((half[0] * up[0]).powi(2) + (half[1] * up[1]).powi(2) + (half[2] * up[2]).powi(2)).sqrt()
}

/// Every bone's placement on its parent for `pose`, in the giant spider's [`crate::Rig`] order.
///
/// Each foot's planted place on the floor is worked out in creature space (a rest place under
/// its hip, swept fore and aft by the gait), the ankle above it found from the tarsus's lean,
/// and the femur and tibia solved to reach it with the knee bent up ([`knee_for`]): so a planted
/// foot rests on the floor exactly while the body bobs, rears or lunges over it. Rearing tips
/// the body up about the third pair's hips, lifts the front pair and waves it in the air;
/// the abdomen hangs behind, lifted so it clears the floor.
pub fn spider(state: &SpiderState, pose: &SpiderPose) -> Vec<Placement> {
    let d = state.dims();
    let s = d.span;
    let moving = pose.pace.clamp(0.0, 1.0);
    let (rear, bite, curl) = (pose.rear.clamp(0.0, 1.0), pose.bite.clamp(0.0, 1.0), pose.curl.clamp(0.0, 1.0));
    let t = pose.time;
    let still = 1.0 - moving;
    let mut out = vec![Placement::IDENTITY; spider_bone::COUNT];

    // The body bobs twice a cycle, breathes standing, tips up to rear and dives into a bite.
    let bob = 0.012 * s * pose.pace.min(1.5) * (0.5 - 0.5 * (2.0 * pose.stride).cos()) + 0.004 * s * still * (1.7 * t).sin();
    let pitch = REAR_PITCH * rear * (1.0 - 0.55 * bite);
    let lunge = 0.07 * s * bite;
    let rotation = spine_to_creature().mul_mat(&rx(pitch));
    let rest_body = Placement { rotation: spine_to_creature(), origin: [0.0, d.centre_height + bob, 0.0] };
    // It tips about the third pair's hips, which stay where they were.
    let pivot = d.hip(4);
    let sway = 0.05 * (1.3 * t).sin() * still + 0.05 * moving * pose.stride.sin();
    let abdomen_local = Placement { rotation: rz(sway).mul_mat(&rx(d.abdomen_tilt - 0.45 * pitch + 0.03 * (2.1 * t).sin() * still)), origin: d.abdomen_root };
    let body_at = |lift: f32| {
        let pivot_c = vec3::add(rest_body.point(pivot), [0.0, lift, lunge]);
        Placement { rotation, origin: vec3::sub(pivot_c, rotation.transform(pivot)) }
    };
    // Lifted just enough that neither the abdomen nor the carapace comes down into the floor.
    let low = |body: &Placement| {
        let abdomen = body.then(&abdomen_local);
        let a = &d.abdomen_trunk;
        let a_drop = ellipsoid_drop(&abdomen.rotation, [a.half_width(0.38), 0.5 * a.length, a.half_height(0.38)]);
        let c = &d.carapace_trunk;
        let c_drop = ellipsoid_drop(&body.rotation, [c.half_width(0.4), 0.5 * c.length, 0.5 * c.half_height(0.45)]);
        (abdomen.origin[1] - a_drop).min(body.origin[1] - c_drop)
    };
    let lift = (BELLY_CLEARANCE * s - low(&body_at(0.0))).max(0.0);
    let body = body_at(lift);
    out[spider_bone::BODY] = body;
    out[spider_bone::ABDOMEN] = abdomen_local;
    let inverse = transpose(&body.rotation);
    let to_body = |p: V3| inverse.transform(vec3::sub(p, body.origin));

    // Fangs open and swing in to bite; pedipalps tap, and spread as it bites.
    for (k, side) in [-1.0f32, 1.0].into_iter().enumerate() {
        let flex = 0.04 * (2.3 * t + k as f32 * 1.9).sin() * still;
        let open = 0.05 + 0.55 * bite + flex;
        let forward = 0.1 + 0.8 * bite;
        out[spider_bone::FANG + k] = Placement { rotation: Mat3::from_axis_angle(vec3::Y, -side * open).mul_mat(&rx(forward)), origin: [side * d.fang_root[0], d.fang_root[1], d.fang_root[2]] };
        let tap = 0.12 * (3.1 * t + 2.0 * k as f32).sin() * still + 0.05 * (9.0 * pose.stride + k as f32 * PI).sin() * moving;
        out[spider_bone::PALP + k] = Placement { rotation: rz(-side * 0.3 * bite).mul_mat(&rx(0.05 + tap + 0.3 * bite)), origin: [side * d.palp_root[0], d.palp_root[1], d.palp_root[2]] };
    }

    // Legs.
    for k in 0..LEG_COUNT {
        let pair = k / 2;
        let side = leg_side(k);
        let leg = d.legs[pair];
        let reach_total = leg.femur + leg.tibia;
        let hip = d.hip(k);
        let hip_c = body.point(hip);
        // The foot's rest place under the hip (the body upright), and the way it points across
        // the floor: creature space has +X to the left, where the spine frame has it to the right.
        let phi = d.azimuth[pair];
        let out_c = [-side * phi.sin(), 0.0, phi.cos()];
        let hip_rest = rest_body.point(hip);
        let c = ((pose.stride + SPIDER_PHASE[k]) / TAU).rem_euclid(1.0);
        let sweep = SPIDER_DUTY * spider_stride(state);
        let (along, up) = if c < SPIDER_DUTY {
            (sweep * (0.5 - c / SPIDER_DUTY), 0.0)
        } else {
            let u = (c - SPIDER_DUTY) / (1.0 - SPIDER_DUTY);
            (sweep * (smoothstep(u) - 0.5), 0.06 * s * (PI * u).sin())
        };
        let foot = [hip_rest[0] + out_c[0] * leg.reach, up * moving, hip_rest[2] + out_c[2] * leg.reach + along * moving];
        let tip = [out_c[0] * TARSUS_LEAN.sin(), -TARSUS_LEAN.cos(), out_c[2] * TARSUS_LEAN.sin()];
        let mut ankle_c = vec3::sub(foot, vec3::scale(tip, leg.tarsus));
        // Feet stay within reach: past it a planted foot slides in across the floor rather than
        // leaving it.
        let dy = ankle_c[1] - hip_c[1];
        let far = (0.985 * reach_total).powi(2) - dy * dy;
        let max_h = far.max(0.0).sqrt();
        let (dx, dz) = (ankle_c[0] - hip_c[0], ankle_c[2] - hip_c[2]);
        let h = dx.hypot(dz);
        if h > max_h && h > 1e-6 {
            ankle_c[0] = hip_c[0] + dx * max_h / h;
            ankle_c[2] = hip_c[2] + dz * max_h / h;
        }
        let planted = to_body(ankle_c);
        let planted_dir = inverse.transform(tip);

        // Raised: held up and forward in front of it, waving.
        let raise = rear * RAISED[pair];
        let wave = 0.05 * s * (2.3 * t + 1.7 * k as f32).sin();
        let raised = vec3::add(hip, [side * 0.3 * reach_total, (0.58 + 0.25 * bite) * reach_total, 0.4 * reach_total + wave]);
        let raised_dir = vec3::normalize_or([side * 0.3, 0.45, -0.85], vec3::neg(vec3::Z));
        // Dead: drawn in under the belly.
        let drawn = [side * 0.3 * reach_total, hip[1] * 0.7, -0.5 * reach_total];
        let drawn_dir = vec3::normalize_or([-side * 0.6, 0.0, 0.5], vec3::Z);

        let ankle = vec3::lerp(vec3::lerp(planted, raised, raise), drawn, curl);
        let tarsus_dir = vec3::normalize_or(vec3::lerp(vec3::lerp(planted_dir, raised_dir, raise), drawn_dir, curl), planted_dir);
        let hint = vec3::normalize_or([side * 0.35, 0.0, 1.0 - 2.0 * curl], vec3::Z);
        let (knee, ankle) = knee_for(hip, ankle, leg.femur, leg.tibia, hint);
        let (femur, tibia, tarsus) = (aim(vec3::sub(knee, hip), hint), aim(vec3::sub(ankle, knee), hint), aim(tarsus_dir, hint));
        let i = spider_bone::LEGS + 3 * k;
        out[i] = Placement { rotation: femur, origin: hip };
        out[i + 1] = Placement { rotation: transpose(&femur).mul_mat(&tibia), origin: [0.0, leg.femur, 0.0] };
        out[i + 2] = Placement { rotation: transpose(&tibia).mul_mat(&tarsus), origin: [0.0, leg.tibia, 0.0] };
    }
    out
}

#[cfg(test)]
mod spider_tests {
    use super::*;
    use crate::spider::bone;

    fn rig_and_state(seed: u32) -> (crate::Rig, SpiderState) {
        let state = SpiderState::roll(seed);
        (state.build(), state)
    }

    /// The lowest point of the tarsus of leg `k`, in creature space.
    fn tip_height(rig: &crate::Rig, places: &[Placement], k: usize) -> f32 {
        let world = rig.world(places);
        let tarsus = bone::LEGS + 3 * k + 2;
        rig.bones[tarsus].mesh.parts.iter().flat_map(|p| &p.positions).map(|&q| world[tarsus].point(q)[1]).fold(f32::MAX, f32::min)
    }

    #[test]
    fn a_walking_spider_keeps_eight_feet_on_the_floor_in_stance() {
        let (rig, state) = rig_and_state(3);
        let s = state.span;
        for step in 0..24 {
            for pace in [0.0, 1.0, 2.0] {
                let pose = SpiderPose { stride: step as f32 / 24.0 * TAU, pace, time: step as f32 * 0.21, ..SpiderPose::default() };
                let places = spider(&state, &pose);
                let low = rig.posed_mesh(&places).bounds().unwrap().0[1];
                assert!(low > -0.012 * s, "sinks to {low} at step {step}, pace {pace}");
                assert!(low < 0.012 * s, "floats at {low} at step {step}, pace {pace}");
                let mut down = 0;
                for k in 0..LEG_COUNT {
                    let c = ((pose.stride + SPIDER_PHASE[k]) / TAU).rem_euclid(1.0);
                    let planted = c < SPIDER_DUTY || pace == 0.0;
                    let bottom = tip_height(&rig, &places, k);
                    if planted {
                        assert!(bottom.abs() < 0.012 * s, "leg {k} at {bottom}, step {step}, pace {pace}");
                        down += 1;
                    } else {
                        assert!(bottom > -0.012 * s, "leg {k} sinks to {bottom}");
                    }
                }
                assert!(down >= 4, "only {down} feet down at step {step}");
            }
        }
    }

    #[test]
    fn the_gait_is_an_alternating_tetrapod() {
        // L1 R2 L3 R4 together (bone order: L1 R1 L2 R2 L3 R3 L4 R4), the others half a cycle on.
        let group = |k: usize| SPIDER_PHASE[k] == 0.0;
        for k in [0, 3, 4, 7] {
            assert!(group(k), "leg {k}");
        }
        for k in [1, 2, 5, 6] {
            assert!(!group(k), "leg {k}");
        }
        assert_eq!((0..8).filter(|&k| group(k)).count(), 4);
    }

    #[test]
    fn legs_are_solved_to_their_bones_lengths() {
        let (_, state) = rig_and_state(5);
        let d = state.dims();
        for pose in [SpiderPose::default(), SpiderPose { stride: 1.0, pace: 1.0, ..SpiderPose::default() }, SpiderPose { rear: 1.0, ..SpiderPose::default() }] {
            let places = spider(&state, &pose);
            for k in 0..LEG_COUNT {
                let i = bone::LEGS + 3 * k;
                assert!((places[i + 1].origin[1] - d.leg(k).femur).abs() < 1e-4);
                for m in 0..3 {
                    let r = places[i + m].rotation;
                    assert!((r.determinant() - 1.0).abs() < 1e-3, "leg {k} bone {m} is not a proper rotation");
                }
            }
        }
    }

    #[test]
    fn a_standing_spider_has_its_knees_over_its_body() {
        let (rig, state) = rig_and_state(7);
        let places = spider(&state, &SpiderPose::default());
        let world = rig.world(&places);
        for k in 0..LEG_COUNT {
            let tibia = bone::LEGS + 3 * k + 1;
            let knee = world[tibia].origin;
            assert!(knee[1] > world[bone::BODY].origin[1] + 0.06 * state.span, "leg {k}'s knee is at {}", knee[1]);
        }
    }

    #[test]
    fn rearing_raises_the_front_legs_and_keeps_the_back_six_planted() {
        let (rig, state) = rig_and_state(3);
        let s = state.span;
        let places = spider(&state, &SpiderPose { rear: 1.0, time: 0.4, ..SpiderPose::default() });
        let low = rig.posed_mesh(&places).bounds().unwrap().0[1];
        assert!(low > -0.012 * s && low < 0.012 * s, "lowest point {low}");
        for k in 0..LEG_COUNT {
            let bottom = tip_height(&rig, &places, k);
            if k < 2 {
                assert!(bottom > 0.2 * s, "front leg {k} is only {bottom} up");
            } else {
                assert!(bottom.abs() < 0.012 * s, "leg {k} at {bottom}");
            }
        }
        // The nose is up: the front of the body is higher than its back.
        let world = rig.world(&places);
        let front = world[bone::BODY].point([0.0, 0.1 * s, 0.0])[1];
        let back = world[bone::BODY].point([0.0, -0.1 * s, 0.0])[1];
        assert!(front > back + 0.05 * s, "{front} vs {back}");
    }

    #[test]
    fn a_biting_spider_lunges_and_swings_its_fangs_in() {
        let (rig, state) = rig_and_state(3);
        let fang_tip = |bite: f32| {
            let places = spider(&state, &SpiderPose { rear: 1.0, bite, ..SpiderPose::default() });
            let world = rig.world(&places);
            world[bone::FANG + 1].point([0.0, 0.0, -0.1 * state.span])
        };
        let shut = fang_tip(0.0);
        let struck = fang_tip(1.0);
        assert!(struck[2] > shut[2] + 0.03 * state.span, "{struck:?} vs {shut:?}");
        // Dived: below where the reared head was.
        assert!(struck[1] < shut[1], "{struck:?} vs {shut:?}");
    }

    #[test]
    fn the_belly_and_abdomen_clear_the_floor_in_every_pose() {
        for seed in 0..6 {
            let (rig, state) = rig_and_state(seed);
            for rear in [0.0, 0.5, 1.0] {
                for bite in [0.0, 1.0] {
                    let places = spider(&state, &SpiderPose { rear, bite, stride: 2.0, pace: 1.0, ..SpiderPose::default() });
                    let world = rig.world(&places);
                    for b in [bone::BODY, bone::ABDOMEN] {
                        let low = rig.bones[b].mesh.parts.iter().flat_map(|p| &p.positions).map(|&q| world[b].point(q)[1]).fold(f32::MAX, f32::min);
                        assert!(low > 0.0, "{seed}: bone {b} at {low}, rear {rear}, bite {bite}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_dead_spider_draws_its_legs_in_under_the_belly() {
        let (rig, state) = rig_and_state(3);
        let alive = rig.posed_mesh(&spider(&state, &SpiderPose::default())).bounds().unwrap();
        let dead = rig.posed_mesh(&spider(&state, &SpiderPose { curl: 1.0, ..SpiderPose::default() })).bounds().unwrap();
        let across = |b: ([f32; 3], [f32; 3])| (b.1[0] - b.0[0]).max(b.1[2] - b.0[2]);
        assert!(across(dead) < 0.9 * across(alive), "{} vs {}", across(dead), across(alive));
    }
}
