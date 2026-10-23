//! Poses: where each bone of a creature's [`crate::Rig`] sits, on its parent, for a moment of
//! its motion. The game works out the motion (how far it has walked, whether it is biting) and
//! copies these placements onto the bones' transforms.
//!
//! - [`centipede`]: the plates laid along a shallow wave, as the snake's spine is, with a pair of
//!   legs hung from each. Every leg stands its claw on the floor through its stance and lifts
//!   through its swing, each plate's legs a little later in the cycle than the one ahead's, so a
//!   wave of stepping ripples down the body (the metachronal wave), the two sides half a cycle
//!   apart. The front rises in an arc to rear, its legs waving, and the head lunges with the
//!   forcipules thrown wide to bite.
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
//! - [`bat`]: a wingbeat, a glide, a hang and a bite. The arm turns up and down on the shoulder
//!   while the forearm and the hand fold on hinges along the membrane's own seams, so the wing
//!   folds in on the upstroke and spreads on the downstroke without opening a gap; a roosting
//!   bat hangs head down by its feet with its wings folded flat against it.
//!
//! - [`troll`]: the skeleton's walk and swing on a troll's stooped joints. The stoop is in the
//!   troll's rest pose ([`crate::troll`] moves the joints), so the legs, pelvis and trunk are the
//!   skeleton's; a swing is a two-claw rake, both arms raised and brought down one just behind
//!   the other.
//!
//! - [`minotaur_with`]: a denizen's stand, walk on digitigrade legs, two-handed chop, gore and
//!   talking gesture ([`DenizenPose`]), with a tail that swings and lifts.
//!
//! The gait's timing, the wave's shape and every amplitude here are our own tuning.

use armory::geom::vec3::{self, Mat3, V3};
use fb_body::{Placement, Side};
use std::f32::consts::{PI, TAU};

use crate::bat::{BatState, bone as bat_bone};
use crate::centipede::{CentipedeState, bone as centipede_bone};
use crate::jackalope::{EAR_SPLIT, JackalopeState, REST_PITCH, bone as jackalope_bone, ear_bow};
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

    #[test]
    fn a_felled_skeleton_falls_into_a_heap_of_bones_at_its_feet() {
        use crate::skeleton::{SkeletonState, parent_of};
        let world = |local: &[Placement]| {
            let mut out: Vec<Placement> = Vec::new();
            for (b, place) in local.iter().enumerate() {
                out.push(parent_of(b).map_or(*place, |p| out[p].then(place)));
            }
            out
        };
        for seed in [1, 5, 77, 4040] {
            let state = SkeletonState::roll(seed);
            let d = state.dims();
            let h = d.body.height;
            // Not yet falling, it stands as it did.
            let standing = skeleton_with(&d, &SkeletonPose::default());
            for (a, b) in skeleton_heap(&d, seed, 0.0).iter().zip(&standing) {
                assert!(vec3::distance(a.origin, b.origin) < 1e-4, "{seed}");
            }
            // Down, every bone lies low and near the feet, the long bones flat along the floor.
            let pieces = heap_pieces(&d);
            let down = world(&skeleton_heap(&d, seed, 1.0));
            for (b, (place, piece)) in down.iter().zip(&pieces).enumerate() {
                let centre = place.point(piece.centre);
                assert!(centre[1] < 0.15 * h && centre[1] > 0.0, "{seed} bone {b} at {centre:?}");
                assert!(centre[0].hypot(centre[2]) < 0.22 * h, "{seed} bone {b} at {centre:?}");
                let along = vec3::normalize_or(place.direction(piece.along), vec3::Y);
                assert!(along[1].abs() < 1e-3, "{seed} bone {b} not flat: {along:?}");
            }
            // The same seed makes the same heap, and another seed another.
            assert_eq!(skeleton_heap(&d, seed, 1.0), skeleton_heap(&d, seed, 1.0));
            assert_ne!(skeleton_heap(&d, seed, 1.0), skeleton_heap(&d, seed + 1, 1.0));
            // Partway down, the feet have landed before the skull.
            let mid = world(&skeleton_heap(&d, seed, 0.4));
            let foot = leg_bone(Side::Right) + 2;
            let skull = crate::skeleton::index::SKULL;
            assert!(mid[foot].point(pieces[foot].centre)[1] < 0.05 * h);
            assert!(mid[skull].point(pieces[skull].centre)[1] > 0.3 * h);
        }
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

/// A skeleton `share` of the way (0 to 1) through falling to pieces, for the bones: with
/// nothing left to hold it together every joint gives at once, and each bone drops straight
/// down, tumbling to lie flat in a heap round where its feet stood. The feet and shins land
/// first and the skull last (a bone falls for as long as it was high), each lying over those
/// that fell before it; the jaw is knocked loose and the skull rolls off to one side. Where
/// each bone comes to rest, and how it lies, is rolled from `seed`, so a skeleton always makes
/// the same heap. The body's root stays standing where it fell: the heap is all in the bones.
/// Our own.
pub fn skeleton_heap(d: &SkeletonDims, seed: u32, share: f32) -> Vec<Placement> {
    use skeleton_bone as bone;
    let share = share.clamp(0.0, 1.0);
    let h = d.body.height;
    let standing = skeleton_with(d, &SkeletonPose::default());
    let mut start: Vec<Placement> = Vec::with_capacity(bone::COUNT);
    for (b, place) in standing.iter().enumerate() {
        let world = match crate::skeleton::parent_of(b) {
            Some(p) => start[p].then(place),
            None => *place,
        };
        start.push(world);
    }
    let pieces = heap_pieces(d);
    let mut rng = procgen_rng::Mulberry32::new(seed ^ HEAP_SALT);
    let mut world = Vec::with_capacity(bone::COUNT);
    for (b, piece) in pieces.iter().enumerate() {
        // Where it comes to rest: anywhere in a small ring round the feet, the ribcage and
        // pelvis near the middle and the skull rolled out to the edge, over what landed first.
        let (near, far) = match b {
            bone::RIBCAGE | bone::PELVIS => (0.0, 0.04),
            bone::SKULL => (0.12, 0.2),
            bone::JAW => (0.06, 0.16),
            _ => (0.0, HEAP_RADIUS),
        };
        let angle = rng.range_f32(0.0, TAU);
        let out = h * rng.range_f32(near, far);
        let order = HEAP_ORDER.iter().position(|&o| o == b).unwrap_or(0) as f32 / bone::COUNT as f32;
        let lift = if b == bone::SKULL { 0.0 } else { HEAP_HEIGHT * h * order };
        let rest_at = [out * angle.sin(), piece.radius + lift, out * angle.cos()];
        // How it lies: along the floor some way round, turned about its own length.
        let heading = rng.range_f32(0.0, TAU);
        let lie = [heading.sin(), 0.0, heading.cos()];
        let roll = rng.range_f32(-PI, PI);
        // A bone falls for as long as it was high, so the lowest have landed well before the
        // skull: under gravity the time to fall goes as the root of the height.
        let from = start[b];
        let centre_from = from.point(piece.centre);
        let height_share = (centre_from[1] / h).clamp(0.0, 1.0);
        let t = (share / (0.35 + 0.65 * height_share.sqrt())).min(1.0);
        let turn = t * t * (3.0 - 2.0 * t);
        let along = vec3::normalize_or(from.direction(piece.along), vec3::Y);
        let axis = vec3::cross(along, lie);
        let tip = vec3::dot(along, lie).clamp(-1.0, 1.0).acos();
        let tilt = if vec3::length(axis) < 1e-5 { Mat3::IDENTITY } else { Mat3::from_axis_angle(axis, tip * turn) };
        let rotation = Mat3::from_axis_angle(lie, roll * turn).mul_mat(&tilt).mul_mat(&from.rotation);
        // Its middle drops as anything dropped does, slow then fast, and slides across to
        // where it lands.
        let centre = [
            centre_from[0] + (rest_at[0] - centre_from[0]) * turn,
            centre_from[1] + (rest_at[1] - centre_from[1]) * t * t,
            centre_from[2] + (rest_at[2] - centre_from[2]) * turn,
        ];
        world.push(Placement { rotation, origin: vec3::sub(centre, rotation.transform(piece.centre)) });
    }
    // Back onto each bone's parent, as the rig takes it.
    (0..bone::COUNT)
        .map(|b| match crate::skeleton::parent_of(b) {
            Some(p) => {
                let to_parent = transpose(&world[p].rotation);
                Placement { rotation: to_parent.mul_mat(&world[b].rotation), origin: to_parent.transform(vec3::sub(world[b].origin, world[p].origin)) }
            }
            None => world[b],
        })
        .collect()
}

/// Mixed into a skeleton's seed for the heap it falls into.
const HEAP_SALT: u32 = 0x4ea9_b0e5;
/// How far from the feet a bone may come to rest, as a share of the skeleton's height.
const HEAP_RADIUS: f32 = 0.1;
/// How high the heap is piled, as a share of the skeleton's height: the last bone down lies
/// about this far over the floor.
const HEAP_HEIGHT: f32 = 0.05;
/// The order the bones land in, bottom of the heap first.
const HEAP_ORDER: [usize; skeleton_bone::COUNT] = {
    use skeleton_bone::*;
    [
        LEGS + 2, LEGS + 5, LEGS + 1, LEGS + 4, LEGS, LEGS + 3,
        PELVIS, SPINE,
        ARMS + 2, ARMS + 5, ARMS + 1, ARMS + 4, ARMS, ARMS + 3,
        RIBCAGE, NECK, JAW, SKULL,
    ]
};

/// A bone measured for lying in a heap: its middle and the direction of its length, in its
/// own space, and how high its middle lies over the floor (half its thickness).
#[derive(Clone, Copy, Debug)]
struct HeapPiece {
    centre: V3,
    along: V3,
    radius: f32,
}

/// Each bone of a skeleton measured for lying in a heap, in rig order. A bone's rest rotation
/// is the identity, so its own space is body space about its joint.
fn heap_pieces(d: &SkeletonDims) -> [HeapPiece; skeleton_bone::COUNT] {
    use skeleton_bone as bone;
    let h = d.body.height;
    let o = &d.origins;
    let head = d.body.head;
    let between = |b: usize, a: V3, z: V3, radius: f32| HeapPiece { centre: vec3::sub(vec3::scale(vec3::add(a, z), 0.5), o[b]), along: vec3::sub(z, a), radius };
    let mut pieces = [HeapPiece { centre: vec3::ZERO, along: vec3::Y, radius: 0.0 }; bone::COUNT];
    pieces[bone::PELVIS] = HeapPiece { centre: vec3::ZERO, along: vec3::X, radius: 0.05 * h };
    pieces[bone::SPINE] = between(bone::SPINE, o[bone::SPINE], o[bone::RIBCAGE], 0.02 * h);
    pieces[bone::RIBCAGE] = HeapPiece {
        centre: [0.0, 0.5 * (o[bone::NECK][1] - o[bone::RIBCAGE][1]), -o[bone::RIBCAGE][2]],
        along: vec3::Y,
        radius: 0.85 * d.body.chest.half_depth,
    };
    pieces[bone::NECK] = between(bone::NECK, o[bone::NECK], o[bone::SKULL], 0.02 * h);
    pieces[bone::SKULL] = HeapPiece { centre: vec3::sub(head.centre, o[bone::SKULL]), along: vec3::Y, radius: 0.85 * head.half_width.min(head.half_depth) };
    pieces[bone::JAW] = HeapPiece { centre: [0.0, -0.35 * head.half_height, 0.5 * head.half_depth], along: vec3::Z, radius: 0.025 * h };
    for (k, side) in Side::BOTH.into_iter().enumerate() {
        let [shoulder, elbow, wrist] = d.arms[k];
        let a = arm_bone(side);
        pieces[a] = between(a, shoulder, elbow, 0.018 * h);
        pieces[a + 1] = between(a + 1, elbow, wrist, 0.015 * h);
        let reach = vec3::normalize_or(vec3::sub(wrist, elbow), [0.0, -1.0, 0.0]);
        pieces[a + 2] = between(a + 2, wrist, vec3::add(wrist, vec3::scale(reach, 0.1 * h)), 0.015 * h);
        let [hip, knee, ankle] = d.legs[k];
        let l = leg_bone(side);
        pieces[l] = between(l, hip, knee, 0.02 * h);
        pieces[l + 1] = between(l + 1, knee, ankle, 0.017 * h);
        pieces[l + 2] = HeapPiece { centre: [0.0, -0.5 * ankle[1], 0.35 * d.foot_length], along: vec3::Z, radius: 0.02 * h };
    }
    pieces
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
    for (k, &phase_offset) in SPIDER_PHASE.iter().enumerate() {
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
        let c = ((pose.stride + phase_offset) / TAU).rem_euclid(1.0);
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
                for (k, &phase_offset) in SPIDER_PHASE.iter().enumerate() {
                    let c = ((pose.stride + phase_offset) / TAU).rem_euclid(1.0);
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

/// The ground a troll covers in one full step cycle: a skeleton's, a step of 0.7 of its leg.
pub fn troll_stride(dims: &SkeletonDims) -> f32 {
    skeleton_stride(dims)
}

/// Every bone's placement on its parent for a troll moving as `pose` says (`swing` is a rake of
/// both claws). The walk is the skeleton's; a swing keeps its lunge and the turn of the trunk
/// and gives both arms their own throw, the left a beat behind the right. Our own timings.
pub fn troll_with(d: &SkeletonDims, pose: &SkeletonPose) -> Vec<Placement> {
    troll_armed(d, pose, &[None, None])
}

/// [`troll_with`] with the arms that hold something (right then left) in their stance, as
/// [`skeleton_armed`] does: a holding arm does not swing as it walks, and in a swing its throw
/// carries the weapon while the hand keeps its wrist turned, so the weapon stays in the fist.
pub fn troll_armed(d: &SkeletonDims, pose: &SkeletonPose, stance: &[Option<ArmStance>; 2]) -> Vec<Placement> {
    let mut out = skeleton_armed(d, pose, stance);
    let Some(p) = pose.swing.map(|p| p.clamp(0.0, 1.0)) else { return out };
    for (k, side) in Side::BOTH.into_iter().enumerate() {
        // The left arm is a beat behind.
        let p = (p - 0.08 * k as f32).clamp(0.0, 1.0);
        let forward = keyed(&[(0.0, 0.15), (0.14, 0.5), (0.3, 2.3), (0.45, 2.45), (0.62, 0.55), (0.78, 0.1), (1.0, 0.15)], p);
        let bend = keyed(&[(0.0, 0.25), (0.3, 1.3), (0.45, 1.5), (0.62, 0.3), (0.78, 0.25), (1.0, 0.25)], p);
        let b = arm_bone(side);
        out[b].rotation = rot_x(-forward);
        out[b + 1].rotation = rot_x(-bend);
    }
    out
}

/// A troll `share` of the way through collapsing onto its back: the skeleton's fall, which lays
/// the limbs out and lets the head drop back.
pub fn troll_fallen(d: &SkeletonDims, share: f32) -> Vec<Placement> {
    skeleton_fallen(d, share)
}

// ---------------------------------------------------------------------------------------------
// Denizens: the minotaur's stand, walk, chop, gore and talking gesture, on a denizen frame.
//
// - Standing breathes, the head swaying a little and the tail swinging from its root.
// - Walking is the skeleton's gait on digitigrade legs: the hock is the leg's "ankle", the
//   pastern keeps the hoof level, so the planted hoof's sole is on the floor, and a lifted foot
//   flexes its toe up to clear it.
// - A chop is a two-handed blow over the shoulder, as a greataxe is swung: both arms raised
//   together, thrown down through the blow with the trunk and the weight going forward onto the
//   planted front foot.
// - A gore lowers the head and shoulders, draws back, and drives the horns forward with the
//   weight, the tail lifting.
// - A gesture is how a denizen talks until the delver knows its tongue: an open hand raised and
//   turned palm up, beckoning, the other hand answering lower, the head nodding and the jaw
//   moving in time.
// ---------------------------------------------------------------------------------------------

/// How a denizen is moving at a moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DenizenPose {
    /// The step cycle, radians: one turn per [`minotaur_stride`] walked.
    pub stride: f32,
    /// Speed as a share of its walking speed: 0 standing, 1 walking.
    pub pace: f32,
    /// Seconds, for what it does standing: breathing, the head's sway and the tail.
    pub time: f32,
    /// How far through a two-handed chop it is, 0 at the guard, through the wind-up and the
    /// blow to 1 back at the guard; `None` for no chop.
    pub swing: Option<f32>,
    /// How far through a gore it is, 0 at rest through the draw back and the thrust to 1; `None`
    /// for none.
    pub gore: Option<f32>,
    /// How far through a gesture of speech it is, 0 to 1 over the whole phrase; `None` for none.
    pub gesture: Option<f32>,
    /// How far the head turns toward its left (+X), radians.
    pub look: f32,
    /// The jaw's gape, 0 shut to 1 wide.
    pub jaw: f32,
}

/// The ground a minotaur covers in one full step cycle: the skeleton's, a step of 0.7 of its leg.
pub fn minotaur_stride(dims: &SkeletonDims) -> f32 {
    skeleton_stride(dims)
}

fn rot_z(a: f32) -> Mat3 {
    Mat3::from_axis_angle([0.0, 0.0, 1.0], a)
}

/// Both legs solved to hold the hocks at `feet` (z in body space, right then left) and `lift`
/// above the floor with the pelvis at z `pelvis_z` and `crouch` below its highest: the pelvis
/// rides at the lower of what the two reaches allow. The feet are kept level. The part of
/// [`skeleton_armed`] a blow needs, with the feet where the caller says.
fn plant_legs(d: &SkeletonDims, out: &mut [Placement], pelvis_z: f32, feet: [f32; 2], lift: [f32; 2], crouch: f32) {
    use skeleton_bone as bone;
    let leg_len = d.thigh + d.shin;
    let reach = 0.985 * leg_len;
    let a0 = d.ankles[0][1];
    let dz = [feet[0] - pelvis_z, feet[1] - pelvis_z];
    let limit = |k: usize| a0 + lift[k] + (reach * reach - dz[k] * dz[k]).max(0.0).sqrt();
    let height = limit(0).min(limit(1)).min(d.legs[0][0][1]) - crouch;
    out[bone::PELVIS] = Placement { rotation: Mat3::IDENTITY, origin: [0.0, height, pelvis_z] };
    for (k, side) in Side::BOTH.into_iter().enumerate() {
        let (thigh, shin) = leg_turns(d, k, [dz[k], a0 + lift[k] - height]);
        let b = leg_bone(side);
        out[b].rotation = rot_x(thigh);
        out[b + 1].rotation = rot_x(shin);
        out[b + 2].rotation = rot_x(-(thigh + shin));
    }
}

/// Every bone's placement on its parent for a minotaur moving as `pose` says: the skeleton's
/// eighteen, then the tail's three ([`crate::denizen::TAIL_FIRST`]). Our own timings.
pub fn minotaur_with(d: &SkeletonDims, pose: &DenizenPose) -> Vec<Placement> {
    use crate::denizen::{TAIL_BONES, TAIL_FIRST, TAIL_REST, tail_offsets};
    use skeleton_bone as bone;
    let attacking = pose.swing.is_some() || pose.gore.is_some();
    let moving = if attacking { 0.0 } else { pose.pace.clamp(0.0, 1.0) };
    let t = pose.time;
    let base = SkeletonPose { stride: pose.stride, pace: moving, time: t, swing: None, look: pose.look, jaw: pose.jaw };
    let mut out = skeleton_armed(d, &base, &[None, None]);
    let leg_len = d.thigh + d.shin;
    let u = 0.1 * leg_len;
    let arm_pose = |out: &mut Vec<Placement>, k: usize, forward: f32, bend: f32, abduct: f32, twist: f32| {
        let side = Side::BOTH[k];
        let out_way = if k == 0 { -1.0 } else { 1.0 };
        let b = arm_bone(side);
        out[b].rotation = rot_x(-forward).mul_mat(&rot_z(out_way * abduct)).mul_mat(&rot_y(twist));
        out[b + 1].rotation = rot_x(-bend);
    };

    // The walk's toes: a lifted hoof flexes up through its swing, to clear the floor.
    if moving > 0.0 {
        for (k, side) in Side::BOTH.into_iter().enumerate() {
            let c = (pose.stride / TAU + if k == 0 { 0.5 } else { 0.0 }).rem_euclid(1.0);
            if c >= DUTY {
                let swing = (PI * (c - DUTY) / (1.0 - DUTY)).sin();
                let b = leg_bone(side) + 2;
                out[b].rotation = out[b].rotation.mul_mat(&rot_x(-0.4 * swing * moving));
            }
        }
    }

    let mut jaw = pose.jaw;
    let mut tail_lift = 0.0;
    if let Some(p) = pose.swing.map(|p| p.clamp(0.0, 1.0)) {
        // A chop: weight back, the axe up over the shoulder, then through it and onto the front foot.
        let pelvis_z = u * keyed(&[(0.0, 0.0), (0.3, -0.5), (0.5, 0.0), (0.62, 1.2), (0.85, 0.8), (1.0, 0.0)], p);
        let crouch = u * keyed(&[(0.0, 0.0), (0.3, 0.3), (0.62, 0.9), (0.85, 0.5), (1.0, 0.0)], p);
        plant_legs(d, &mut out, pelvis_z, [-1.0 * u, 1.1 * u], [0.0, 0.0], crouch);
        let lean = keyed(&[(0.0, 0.05), (0.3, -0.2), (0.45, -0.3), (0.62, 0.6), (0.8, 0.45), (1.0, 0.05)], p);
        let twist = keyed(&[(0.0, 0.0), (0.3, -0.35), (0.45, -0.4), (0.62, 0.3), (0.8, 0.3), (1.0, 0.0)], p);
        out[bone::SPINE].rotation = rot_x(0.5 * lean).mul_mat(&rot_y(0.3 * twist));
        out[bone::RIBCAGE].rotation = rot_x(0.5 * lean).mul_mat(&rot_y(0.7 * twist));
        out[bone::NECK].rotation = rot_x(-0.4 * lean);
        out[bone::SKULL].rotation = rot_y(pose.look - twist).mul_mat(&rot_x(-0.3 * lean));
        jaw = jaw.max(keyed(&[(0.0, 0.0), (0.3, 0.3), (0.45, 0.7), (0.62, 0.9), (0.8, 0.3), (1.0, 0.0)], p));
        let forward = keyed(&[(0.0, 0.55), (0.14, 0.9), (0.3, 2.6), (0.45, 2.85), (0.62, 0.85), (0.8, 0.6), (1.0, 0.55)], p);
        let bend = keyed(&[(0.0, 1.15), (0.3, 1.2), (0.45, 1.0), (0.62, 0.35), (0.8, 0.45), (1.0, 1.15)], p);
        for k in 0..2 {
            // Both hands on the haft: the arms drawn in toward the middle.
            arm_pose(&mut out, k, forward, bend, -0.32, 0.0);
        }
        tail_lift = keyed(&[(0.0, 0.0), (0.45, 0.5), (0.62, 0.9), (1.0, 0.0)], p);
    } else if let Some(p) = pose.gore.map(|p| p.clamp(0.0, 1.0)) {
        // A gore: draw back and down, then drive the head forward with the whole body.
        let pelvis_z = u * keyed(&[(0.0, 0.0), (0.28, -0.5), (0.5, 1.7), (0.7, 1.9), (1.0, 0.0)], p);
        let crouch = u * keyed(&[(0.0, 0.0), (0.28, 0.6), (0.5, 1.2), (0.8, 0.8), (1.0, 0.0)], p);
        plant_legs(d, &mut out, pelvis_z, [-1.3 * u, 1.0 * u], [0.0, 0.0], crouch);
        let lean = keyed(&[(0.0, 0.03), (0.28, -0.1), (0.5, 0.65), (0.7, 0.6), (1.0, 0.03)], p);
        out[bone::SPINE].rotation = rot_x(0.5 * lean);
        out[bone::RIBCAGE].rotation = rot_x(0.5 * lean);
        // The head throws up as it draws back, then drops to bring the horns level.
        let dip = keyed(&[(0.0, 0.0), (0.28, -0.3), (0.5, 0.55), (0.7, 0.5), (1.0, 0.0)], p);
        out[bone::NECK].rotation = rot_x(0.4 * dip);
        out[bone::SKULL].rotation = rot_y(pose.look).mul_mat(&rot_x(0.6 * dip));
        jaw = jaw.max(keyed(&[(0.0, 0.0), (0.28, 0.5), (0.5, 0.7), (0.7, 0.3), (1.0, 0.0)], p));
        let forward = keyed(&[(0.0, 0.1), (0.28, -0.5), (0.5, 0.9), (0.7, 0.8), (1.0, 0.1)], p);
        let bend = keyed(&[(0.0, 0.15), (0.28, 0.5), (0.5, 1.0), (0.7, 0.9), (1.0, 0.15)], p);
        for k in 0..2 {
            arm_pose(&mut out, k, forward, bend, 0.25, 0.0);
        }
        tail_lift = keyed(&[(0.0, 0.0), (0.4, 0.6), (0.7, 1.0), (1.0, 0.0)], p);
    } else if let Some(p) = pose.gesture.map(|p| p.clamp(0.0, 1.0)) {
        // Speech by gesture: raise the open hand, turn it palm up and beckon, the other hand
        // answering lower; the head nods and the jaw works with it.
        let e = smoothstep(p / 0.15) * smoothstep((1.0 - p) / 0.15);
        let beat = (p * TAU * 3.0).sin();
        arm_pose(&mut out, 0, 1.15 * e, (1.2 + 0.3 * beat) * e + 0.12 * (1.0 - e), 0.12 * e, -0.9 * e);
        arm_pose(&mut out, 1, 0.6 * e + 0.05 * beat * e, 0.95 * e + 0.2 * (1.0 - e), 0.3 * e, 0.6 * e);
        let nod = 0.08 * (p * TAU * 2.0).sin() * e;
        out[bone::SPINE].rotation = rot_x(0.04 * e).mul_mat(&rot_y(-0.06 * e));
        out[bone::RIBCAGE].rotation = rot_x(0.02 * e).mul_mat(&rot_y(-0.1 * e));
        out[bone::NECK].rotation = rot_x(nod);
        out[bone::SKULL].rotation = rot_y(pose.look + 0.12 * (p * TAU).sin() * e + 0.1 * e).mul_mat(&rot_x(-0.04 + nod + 0.04 * e));
        jaw = jaw.max(0.18 * e * (0.5 + 0.5 * (p * TAU * 6.0).sin()));
        tail_lift = 0.15 * e;
    }
    out[bone::JAW].rotation = rot_x(0.5 * jaw);

    // The tail: hung from behind the seat, swinging from its root in a wave, lifting in a blow.
    let offsets = tail_offsets(d);
    let wave = if moving > 0.0 { 0.22 * moving } else { 0.14 };
    for i in 0..TAIL_BONES {
        let phase = if moving > 0.0 { pose.stride - 0.9 * i as f32 } else { 1.3 * t - 0.9 * i as f32 };
        let yaw = wave * phase.sin();
        let pitch = (if i == 0 { TAIL_REST } else { 0.0 }) + 0.06 * (0.9 * t - i as f32).sin() + (0.7 - 0.2 * i as f32) * tail_lift;
        out.push(Placement { rotation: rot_x(pitch).mul_mat(&rot_y(yaw)), origin: offsets[i] });
    }
    debug_assert_eq!(out.len(), TAIL_FIRST + TAIL_BONES);
    out
}

/// A minotaur `share` of the way through collapsing onto its back: the skeleton's fall, with
/// the tail left hanging.
pub fn minotaur_fallen(d: &SkeletonDims, share: f32) -> Vec<Placement> {
    use crate::denizen::{TAIL_BONES, TAIL_REST, tail_offsets};
    let mut out = skeleton_fallen(d, share);
    let offsets = tail_offsets(d);
    for (i, &origin) in offsets.iter().enumerate().take(TAIL_BONES) {
        out.push(Placement { rotation: rot_x(if i == 0 { TAIL_REST } else { 0.0 }), origin });
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Jackalope: a hop, a sit up with the ears turning, and a lowered head to gore.
//
// - Hopping is a bound. Both hind paws plant together and push off, the body stretching out and
//   rising; the forepaws land together a little later and the body gathers over them; then the
//   body is in the air with all four paws clear until the hind paws land ahead again. Each
//   paw is planted on the floor through its stance and carried through its swing, the leg bent
//   to reach it by [`two_bone`], so no paw sinks into the floor; only in flight, when no paw is
//   down, does the body leave it.
// - Sitting up tips the body about the hips until it stands on its haunches, the hind feet laid
//   flat in front of it, the forepaws drawn up to the chest and the ears standing and turning
//   to catch a sound.
// - A gore lowers the head until the antlers point ahead, drops the chest and throws the body
//   forward over its planted feet, the ears pinned back.
// ---------------------------------------------------------------------------------------------

/// How a jackalope is moving at a moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JackalopePose {
    /// The hop cycle, radians: one turn per [`jackalope_stride`] covered.
    pub stride: f32,
    /// Speed as a share of its walking speed: 0 standing, 1 hopping, 2 bolting.
    pub pace: f32,
    /// Seconds, for what it does standing: sniffing, the ears' sway and the tail's twitch.
    pub time: f32,
    /// 0 on all fours to 1 sat up on its haunches; only when standing still.
    pub sit: f32,
    /// A gore's lunge, 0 to 1 at full stretch with the head down.
    pub gore: f32,
    /// How far the head turns toward its left (+X), radians.
    pub look: f32,
}

/// The ground a jackalope covers in one hop cycle, as a share of its length. Our own.
pub const HOP_STRIDE: f32 = 1.3;
/// The shares of the cycle the hind and the fore paws are on the floor, and when the fore paws
/// land. The rest is flight. Our own.
const HIND_DUTY: f32 = 0.28;
const FORE_DUTY: f32 = 0.22;
const FORE_LANDS: f32 = 0.4;
/// How high the body rises in flight, and the paws in their swing, as shares of its length.
const HOP_HEIGHT: f32 = 0.26;
const HIND_LIFT: f32 = 0.16;
const FORE_LIFT: f32 = 0.11;
/// How far the body tips at full hop (radians), tips up sitting, and drops its chest to gore.
const HOP_PITCH: f32 = 0.3;
const SIT_PITCH: f32 = 1.05;
const GORE_PITCH: f32 = -0.04;

/// The ground a jackalope covers in one full hop cycle: [`HOP_STRIDE`] of its length.
pub fn jackalope_stride(jackalope: &JackalopeState) -> f32 {
    HOP_STRIDE * jackalope.length
}

/// The rotation of a vector from creature space into the spine frame of a body pitched by
/// `pitch`: the transpose of the body's rotation.
fn into_spine(pitch: f32) -> Mat3 {
    let [x, y, z] = spine_to_creature().mul_mat(&rx(pitch)).cols;
    Mat3::from_cols([x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]])
}

/// How high the hips of a jackalope sitting up must be for its haunches to rest on the floor:
/// the lowest point of its body, with the body pitched by [`SIT_PITCH`] about the hips, lies on
/// the floor.
fn sit_hip_height(state: &JackalopeState) -> f32 {
    let d = state.dims();
    let trunk = state.body_trunk(&d);
    let r = spine_to_creature().mul_mat(&rx(SIT_PITCH));
    let hip = [0.0, d.hip[1], d.hip[2]];
    let mut low = f32::MAX;
    for i in 0..=12 {
        for j in 0..24 {
            let p = trunk.point(0.65 * i as f32 / 12.0, j as f32 / 24.0 * TAU, 0.0);
            low = low.min(r.transform(vec3::sub(p, hip))[1]);
        }
    }
    -low
}

/// The cycle's lift and pitch at cycle position `c` (0 as the hind paws land), as `(lift, pitch)`:
/// the body's rise in flight, 0 to 1, and its tip, nose up +1 to nose down -1.
fn hop_shape(c: f32) -> (f32, f32) {
    let bump = |u: f32| 4.0 * u * (1.0 - u);
    let fore_leaves = FORE_LANDS + FORE_DUTY;
    let lift = if (HIND_DUTY..FORE_LANDS).contains(&c) {
        0.2 * bump((c - HIND_DUTY) / (FORE_LANDS - HIND_DUTY))
    } else if c >= fore_leaves {
        bump((c - fore_leaves) / (1.0 - fore_leaves))
    } else {
        0.0
    };
    (lift, (TAU * (c - 0.15)).cos())
}

/// Every bone's placement on its parent for `pose`, in the jackalope's [`crate::Rig`] order.
pub fn jackalope(state: &JackalopeState, pose: &JackalopePose) -> Vec<Placement> {
    let d = state.dims();
    let l = d.length;
    let moving = pose.pace.clamp(0.0, 1.0);
    let amplitude = pose.pace.clamp(0.0, 1.4);
    let sit = pose.sit.clamp(0.0, 1.0) * (1.0 - moving);
    let gore = pose.gore.clamp(0.0, 1.0);
    let t = pose.time;
    let still = 1.0 - moving;
    let c = (pose.stride / TAU).rem_euclid(1.0);
    let (lift, hop_pitch) = hop_shape(c);
    let mut out = vec![Placement::IDENTITY; jackalope_bone::COUNT];

    // The body: tipped by its rest, the hop, the sit and the gore; carried so the hips are
    // where they should be, whatever it is tipped to.
    let pitch = REST_PITCH + amplitude * HOP_PITCH * hop_pitch + sit * (SIT_PITCH - REST_PITCH) + gore * GORE_PITCH;
    let rotation = spine_to_creature().mul_mat(&rx(pitch));
    let hip_centre = [0.0, d.hip[1], d.hip[2]];
    let rest_hip = spine_to_creature().mul_mat(&rx(REST_PITCH)).transform(hip_centre);
    let stand_hip_height = d.hind.ankle + 0.8 * (d.hind.upper + d.hind.lower);
    let breathe = 0.004 * l * (1.8 * t).sin() * still;
    let lunge = 0.2 * l * gore;
    let hip_height = (1.0 - sit) * (stand_hip_height - 0.04 * l * gore) + sit * (sit_hip_height(state) + 0.05 * l) + HOP_HEIGHT * l * amplitude * lift + breathe;
    let hip_forward = rest_hip[2] + lunge - 0.1 * l * sit;
    let offset = rotation.transform(hip_centre);
    let origin = [0.0, hip_height - offset[1], hip_forward - offset[2]];
    out[jackalope_bone::BODY] = Placement { rotation, origin };

    // The head: sniffing about when still, up and alert when sat, down with the antlers ahead to
    // gore. Its tip is set in the world, not on the body.
    let sniff_pitch = still * (1.0 - 0.7 * sit) * (0.06 * (2.9 * t).sin() + 0.035 * (7.3 * t + 1.1).sin());
    let sniff_yaw = still * (0.2 * (0.83 * t).sin() + 0.07 * (2.1 * t + 0.4).sin());
    let head_pitch = 0.06 + 0.3 * sit + 0.2 * amplitude * moving + sniff_pitch - 0.62 * gore;
    out[jackalope_bone::HEAD] = Placement {
        rotation: rz(pose.look + sniff_yaw).mul_mat(&rx(head_pitch - pitch)),
        origin: vec3::add(d.neck, [0.0, 0.1 * l * gore, 0.0]),
    };

    // The ears: standing and swaying, laid back to run, pinned to gore, and turning one way and
    // the other to listen when sat. Each ear's tip follows its base a little late.
    let alert = sit + 0.35 * still * (1.0 - sit);
    for (k, side) in [-1.0f32, 1.0].into_iter().enumerate() {
        let (root, rest) = d.ear_root(state, side);
        let phase = if side < 0.0 { 0.0 } else { 2.3 };
        let listen = alert * (0.5 * (1.3 * t + phase).sin() + 0.2 * (3.1 * t + 2.0 * phase).sin());
        let flop = 0.12 * still * (0.7 * t + phase).sin() + 0.25 * moving * amplitude.min(1.0) - 0.4 * gore - 0.1 * hop_pitch * amplitude + 0.12 * sit;
        let twist = listen + 0.08 * still * (0.9 * t + 3.0 * phase).sin();
        // Laid back and raised about the head's own side to side axis, not the ear's: the ear's
        // hollow faces out as well as forward, so turning it about its own width would swing it
        // out sideways rather than back along the neck.
        let lay = rx(-flop);
        let place = Placement { rotation: lay.mul_mat(&rest).mul_mat(&ry(twist)), origin: root };
        let tip_flop = -0.15 * moving * amplitude.min(1.0) - 0.1 * gore + 0.08 * hop_pitch * amplitude + 0.12 * (2.2 * t + phase).sin() * still;
        // The tip bends about that same axis, carried into the base's frame.
        let tip_turn = transpose(&place.rotation).mul_mat(&rx(-tip_flop)).mul_mat(&place.rotation);
        let tip = Placement { rotation: tip_turn, origin: [0.0, EAR_SPLIT * d.ear_length, ear_bow(EAR_SPLIT) * d.ear_length] };
        out[jackalope_bone::EAR_LEFT + 2 * k] = place;
        out[jackalope_bone::EAR_LEFT + 2 * k + 1] = tip;
    }

    // Legs: each paw's ankle follows its stance and swing, in the world; the leg solves to reach
    // it from where its joint is, however the body is tipped.
    let spine_from_creature = into_spine(pitch);
    for (k, &(_, _, hind)) in LEGS.iter().enumerate() {
        let leg = d.leg(k);
        let root = d.leg_root(k);
        let joint = vec3::add(origin, rotation.transform(root));
        let (duty, comes, swing_lift) = if hind { (HIND_DUTY, 0.0, HIND_LIFT) } else { (FORE_DUTY, FORE_LANDS, FORE_LIFT) };
        let phase = ((pose.stride / TAU) - comes).rem_euclid(1.0);
        let sweep = duty * jackalope_stride(state);
        let (along, up) = if phase < duty {
            (sweep * (0.5 - phase / duty), 0.0)
        } else {
            let u = (phase - duty) / (1.0 - duty);
            (sweep * (smoothstep(u) - 0.5), swing_lift * l * (PI * u).sin())
        };
        // Where it stands when still: the hind paws a little ahead of the hips, the fore under
        // the shoulders; sat, the hind feet lie in front of the haunches.
        let neutral = if hind { 0.07 * l + sit * 0.06 * l } else { 0.02 * l };
        let vertical = joint[1] - (leg.ankle + up * moving);
        let reach = 0.95 * ((leg.upper + leg.lower).powi(2) - vertical * vertical).max(0.0).sqrt();
        let forward = (neutral + along * moving - 0.6 * lunge).clamp(-reach, reach);
        let mut target = [0.0, leg.ankle + up * moving, joint[2] + forward];
        if !hind && sit > 0.0 {
            // Drawn up to the chest, hanging loose.
            let tucked = [0.0, joint[1] - 0.78 * (leg.upper + leg.lower), joint[2] + 0.07 * l];
            target = vec3::lerp(target, tucked, sit);
        }
        let local = spine_from_creature.transform(vec3::sub(target, joint));
        let (upper, lower) = two_bone(leg.upper, leg.lower, [local[1], local[2]], hind);
        let i = jackalope_bone::LEGS + 3 * k;
        out[i] = Placement { rotation: rx(upper), origin: root };
        out[i + 1] = Placement { rotation: rx(lower), origin: [0.0, 0.0, -leg.upper] };
        // The paw stays level, flat on the floor; a sat jackalope's forepaws curl.
        let curl = if hind { 0.0 } else { 0.9 * sit };
        out[i + 2] = Placement { rotation: rx(-(upper + lower + pitch + curl)), origin: [0.0, 0.0, -leg.lower] };
    }

    // The tail: a tuft held low standing, flicked up to run, clear of the floor when sat.
    let twitch = 0.08 * still * (2.4 * t).sin().max(0.0);
    let raise = 0.12 + 0.7 * moving.max(gore) + 1.0 * sit + twitch;
    out[jackalope_bone::TAIL] = Placement { rotation: rx(-raise), origin: d.tail_root };
    out
}

/// A rotation about +Y (in a bone's own frame, twisting it about its length).
fn ry(a: f32) -> Mat3 {
    Mat3::from_axis_angle([0.0, 1.0, 0.0], a)
}

#[cfg(test)]
mod jackalope_tests {
    use super::*;

    /// The lowest point of every part of a rig posed by `places`.
    fn lowest(rig: &crate::Rig, places: &[Placement]) -> f32 {
        rig.posed_mesh(places).bounds().unwrap().0[1]
    }

    /// The lowest point of one paw bone's parts, in creature space.
    fn paw_bottom(rig: &crate::Rig, places: &[Placement], paw: usize) -> f32 {
        let world = rig.world(places);
        rig.bones[paw].mesh.parts.iter().flat_map(|p| &p.positions).map(|&q| world[paw].point(q)[1]).fold(f32::MAX, f32::min)
    }

    #[test]
    fn a_hopping_jackalope_keeps_its_paws_on_the_floor_and_leaves_it_only_in_flight() {
        for seed in [3, 11] {
            let state = JackalopeState::roll(seed);
            let rig = state.build();
            let l = state.length;
            for step in 0..48 {
                for pace in [0.0, 0.5, 1.0, 2.0] {
                    let stride = step as f32 / 48.0 * TAU;
                    let places = jackalope(&state, &JackalopePose { stride, pace, time: step as f32 * 0.21, ..JackalopePose::default() });
                    let low = lowest(&rig, &places);
                    assert!(low > -0.02 * l, "seed {seed} sinks to {low} at step {step}, pace {pace}");
                    let c = (stride / TAU).rem_euclid(1.0);
                    let hind_down = c < HIND_DUTY;
                    let fore_down = (FORE_LANDS..FORE_LANDS + FORE_DUTY).contains(&c);
                    if pace == 0.0 || hind_down || fore_down {
                        // A paw in stance is on the floor, and the body is where it carries it.
                        let bottoms: Vec<f32> = (0..4)
                            .filter(|&k| pace == 0.0 || if LEGS[k].2 { hind_down } else { fore_down })
                            .map(|k| paw_bottom(&rig, &places, jackalope_bone::LEGS + 3 * k + 2))
                            .collect();
                        assert!(bottoms.iter().all(|b| b.abs() < 0.03 * l), "seed {seed} step {step} pace {pace}: paws at {bottoms:?}");
                    } else {
                        // In flight, the body is above the floor by no more than the hop's height.
                        assert!(low < (HOP_HEIGHT + 0.12) * l * 1.4, "seed {seed} step {step} pace {pace}: {low}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_hopping_jackalope_stretches_and_gathers() {
        let state = JackalopeState::roll(3);
        let rig = state.build();
        let spans: Vec<f32> = (0..24)
            .map(|k| {
                let places = jackalope(&state, &JackalopePose { stride: k as f32 / 24.0 * TAU, pace: 1.0, ..JackalopePose::default() });
                let (lo, hi) = rig.posed_mesh(&places).bounds().unwrap();
                hi[2] - lo[2]
            })
            .collect();
        let (min, max) = (spans.iter().cloned().fold(f32::MAX, f32::min), spans.iter().cloned().fold(f32::MIN, f32::max));
        assert!(max > 1.12 * min, "the body's length varies from {min} to {max}");
    }

    #[test]
    fn a_sitting_jackalope_stands_up_on_its_haunches_and_rests_on_the_floor() {
        for seed in [3, 8] {
            let state = JackalopeState::roll(seed);
            let rig = state.build();
            let l = state.length;
            let stand = jackalope(&state, &JackalopePose::default());
            let sat = jackalope(&state, &JackalopePose { sit: 1.0, ..JackalopePose::default() });
            let (stand_hi, sat_hi) = (rig.posed_mesh(&stand).bounds().unwrap().1[1], rig.posed_mesh(&sat).bounds().unwrap().1[1]);
            assert!(sat_hi > 1.2 * stand_hi, "sat to {sat_hi}, standing to {stand_hi}");
            let low = lowest(&rig, &sat);
            assert!(low > -0.03 * l && low < 0.04 * l, "seed {seed}: sat on the floor at {low}");
            // The hind feet are flat on it.
            for k in [2, 3] {
                let bottom = paw_bottom(&rig, &sat, jackalope_bone::LEGS + 3 * k + 2);
                assert!(bottom.abs() < 0.03 * l, "seed {seed}: hind paw {k} at {bottom}");
            }
            // Its ears turn: they are not where they were a moment later.
            let later = jackalope(&state, &JackalopePose { sit: 1.0, time: 1.7, ..JackalopePose::default() });
            let turned = (0..2).any(|k| {
                let i = jackalope_bone::EAR_LEFT + 2 * k;
                (0..3).any(|j| vec3::distance(sat[i].rotation.cols[j], later[i].rotation.cols[j]) > 0.1)
            });
            assert!(turned, "seed {seed}: the ears do not turn");
        }
    }

    #[test]
    fn a_goring_jackalope_lowers_its_head_and_lunges() {
        let state = JackalopeState::roll(3);
        let rig = state.build();
        let tip = |gore: f32| {
            let places = jackalope(&state, &JackalopePose { gore, ..JackalopePose::default() });
            let world = rig.world(&places);
            // The antlers' highest point, and a point on the muzzle, in creature space.
            let head = &rig.bones[jackalope_bone::HEAD];
            let antlers = head.mesh.parts.iter().find(|p| p.name == "antlers").unwrap();
            let top = antlers.positions.iter().map(|&p| world[jackalope_bone::HEAD].point(p)).fold([0.0, f32::MIN, 0.0], |a, p| if p[1] > a[1] { p } else { a });
            (top, world[jackalope_bone::HEAD].point([0.0, 0.3 * state.length, 0.0]))
        };
        let (rest_antler, rest_nose) = tip(0.0);
        let (gore_antler, gore_nose) = tip(1.0);
        assert!(gore_nose[2] > rest_nose[2] + 0.15 * state.length, "the lunge carries the head forward");
        assert!(gore_nose[1] < rest_nose[1] - 0.08 * state.length, "the head comes down");
        assert!(gore_antler[1] < rest_antler[1], "the antlers come down");
        let low = lowest(&rig, &jackalope(&state, &JackalopePose { gore: 1.0, ..JackalopePose::default() }));
        assert!(low > -0.03 * state.length, "a goring jackalope does not sink into the floor: {low}");
    }
}

/// How a giant centipede is moving at a moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CentipedePose {
    /// The step cycle, radians: one turn per [`centipede_stride`] walked.
    pub stride: f32,
    /// Speed as a share of its walking speed: 0 standing, 1 walking, 2 running.
    pub pace: f32,
    /// Seconds, for what it does standing: the antennae's search and the forcipules' twitch.
    pub time: f32,
    /// 0 flat on the floor, 1 the front of the body raised off it, the first legs waving.
    pub rear: f32,
    /// A bite's lunge, 0 to 1 at full stretch. The forcipules snap shut as it lands.
    pub bite: f32,
    /// How much wider the forcipules gape as the bite is thrown, 0 to 1; at 0 they stand open.
    pub jaw: f32,
    /// How far the head turns toward its left (+X), radians.
    pub look: f32,
    /// 0 alive, 1 curled into a tight flat spiral on the floor, the head at its middle, the legs
    /// drawn in against the belly: how a centipede lies dead.
    pub curl: f32,
}

impl CentipedePose {
    /// Lying flat, still.
    pub const REST: CentipedePose = CentipedePose { stride: 0.0, pace: 0.0, time: 0.0, rear: 0.0, bite: 0.0, jaw: 0.0, look: 0.0, curl: 0.0 };
}

/// Share of the cycle each leg is on the floor. Lower than most walkers': a centipede's many
/// legs each spend about half the cycle in the air.
pub const LEG_DUTY: f32 = 0.55;
/// How much later in the cycle each plate's legs are than the plate ahead's, radians: a wave of
/// stepping runs down the body, from the tail toward the head (the metachronal wave), a wave
/// about thirteen plates long.
const LEG_LAG: f32 = 0.48;
/// How far a leg swings either side of its neutral heading through its stance, radians, and how
/// far it lifts (a roll about the leg's own axis) through its swing.
const LEG_SWEEP: f32 = 0.5;
const LEG_LIFT: f32 = 0.6;
/// The body's wave: its greatest heading off straight, radians, and its wavelength as a share of
/// the body's length. A shallow wave.
const BODY_WAVE: f32 = 0.2;
const BODY_WAVELENGTH: f32 = 0.6;
/// How many times the legs' own sweep the ground covered per cycle is stretched: at a walking
/// 15 units a second the bare sweep (under two units) had the legs stepping eight times a second,
/// a blur. Stretched, a walk is about three steps a second. Our own number.
const STRIDE_STRETCH: f32 = 3.0;
/// The body's wave moves at this share of a radian per second, not with the legs, so it rolls
/// along slowly (about a cycle every six seconds). Our own number.
const WAVE_RATE: f32 = 1.0;
/// How far the forcipules stand open at rest and walking, radians; they gape wider as the bite
/// is thrown, then snap shut as it lands. Our own numbers.
const FORCIPULE_GAPE: f32 = 0.3;
const FORCIPULE_WIND_UP: f32 = 0.35;
/// The rearing front's greatest pitch off the floor, radians, and how many plates long the
/// raised front is.
const RAISE_PITCH: f32 = 1.1;
const RAISED_PLATES: f32 = 6.0;
/// A coiled centipede's spiral, in girths: the radius the head sits at, and how far apart the
/// turns lie (a little over the body's width, which is two girths). Our own numbers.
const COIL_CORE: f32 = 1.3;
const COIL_PITCH: f32 = 2.2;
/// How far a coiled centipede's legs droop in against its flanks (`yaw`, about the body's
/// length) and sweep fore and aft (`roll`), radians.
const COIL_LEG_YAW: f32 = -1.1;
const COIL_LEG_ROLL: f32 = 0.3;

/// The ground a centipede covers in one full step cycle: what its legs sweep through their
/// stance, over the share of the cycle they spend in it.
pub fn centipede_stride(state: &CentipedeState) -> f32 {
    let mid = state.segments / 2;
    STRIDE_STRETCH * 2.0 * state.leg_reach(mid) * LEG_SWEEP.sin() / LEG_DUTY
}

/// Where plate `k`'s leg on `side` (1 right, -1 left) is in the cycle at `stride`, 0 to 1: on
/// the floor before [`LEG_DUTY`], in the air after. The two sides step half a cycle apart.
pub fn leg_cycle(k: usize, side: f32, stride: f32) -> f32 {
    ((stride + LEG_LAG * k as f32) / TAU + if side < 0.0 { 0.5 } else { 0.0 }).rem_euclid(1.0)
}

/// Every bone's placement on its parent for `pose`, in the giant centipede's [`crate::Rig`]
/// order. The plates are laid along a shallow wave (a serpenoid curve, as [`snake`]'s, with a
/// small heading) the way the snake's spine is, centred on the origin; the front rises in an
/// arc to rear. Each leg stands its claw on the floor through its stance, swinging back about
/// its hip with the plate level so the claw neither slides up nor sinks, and lifts through its
/// swing; the legs of the raised plates wave in the air.
pub fn centipede(state: &CentipedeState, pose: &CentipedePose) -> Vec<Placement> {
    let d = state.dims();
    let n = state.segments;
    let l = d.segment;
    let per = 4;
    let samples = n * per + 1;
    let step = l / per as f32;
    let moving = pose.pace.clamp(0.0, 1.0);
    let curl = pose.curl.clamp(0.0, 1.0);
    let raise = pose.rear.clamp(0.0, 1.0) * (1.0 - curl);
    let t = pose.time;
    let lambda = BODY_WAVELENGTH * d.body_length;
    let amplitude = BODY_WAVE * (0.3 + 0.7 * moving);
    let neck = (RAISED_PLATES * l).min(d.body_length);
    let pitch_at = |s: f32| RAISE_PITCH * raise * (1.0 - smoothstep(s / neck));
    // A coil's heading at each sample: the turn so far, which tightens toward the head as the
    // radius there is small, so the body winds in a flat spiral with the head at its middle.
    let (core, pitch) = (COIL_CORE * d.girth, COIL_PITCH * d.girth);
    let mut coil = Vec::with_capacity(samples);
    let mut turned = 0.0f32;
    for _ in 0..samples {
        let turn = step / (core + pitch * turned / TAU);
        coil.push(turned + 0.5 * turn);
        turned += turn;
    }
    let heading = |s: f32| {
        let wave = amplitude * (TAU * s / lambda + WAVE_RATE * t).sin();
        if curl > 0.0 { wave + curl * (coil[((s / step) as usize).min(samples - 1)] - wave) } else { wave }
    };
    // Height of the centre line, built up from the tail so every step keeps its length.
    let mut lift = vec![0.0f32; samples + 1];
    for j in (0..samples).rev() {
        lift[j] = lift[j + 1] + step * pitch_at((j as f32 + 0.5) * step).sin();
    }
    let mut points: Vec<V3> = Vec::with_capacity(samples);
    let mut p = [0.0f32, 0.0];
    for (j, &height) in lift.iter().take(samples).enumerate() {
        points.push([p[0], height, p[1]]);
        let mid = (j as f32 + 0.5) * step;
        let level = step * pitch_at(mid).cos();
        let h = heading(mid);
        p = [p[0] - level * h.sin(), p[1] - level * h.cos()];
    }
    let mean = points.iter().fold([0.0f32; 2], |m, q| [m[0] + q[0], m[1] + q[2]]);
    for q in &mut points {
        q[0] -= mean[0] / samples as f32;
        q[2] -= mean[1] / samples as f32;
    }
    let mut out = vec![Placement::IDENTITY; state.bone_count()];

    // The plates, each on the line through its neighbours' points.
    for k in 0..n {
        let j = k * per + per / 2;
        let forward = vec3::sub(points[j - 1], points[j + 1]);
        let mut origin = points[j];
        origin[1] += state.centre_height(k);
        out[state.segment_bone(k)] = Placement { rotation: along(forward), origin };
    }

    // The head at the neck, carried on the line, turned to look, lunging and tipped down to bite.
    let forward = vec3::normalize_or(vec3::sub(points[0], points[1]), [0.0, 0.0, 1.0]);
    let mut head_at = points[0];
    head_at[1] += d.clearance + state.head_depth();
    head_at = vec3::add(head_at, vec3::scale(forward, 0.9 * d.girth * pose.bite));
    let sniff = 0.12 * (1.0 - moving) * (0.9 * t).sin();
    let tip = -0.5 * raise - 0.3 * pose.bite - 0.35 * curl;
    let rotation = along(forward).mul_mat(&rz(pose.look + sniff)).mul_mat(&rx(tip));
    let mut head_place = Placement { rotation, origin: head_at };
    // Forcipules: held open and apart, twitching a little, gaping wider as the bite is thrown and
    // snapping shut as it lands (the last of the lunge), then opening again as it draws back.
    let mut forcipules = [Placement::IDENTITY; 2];
    let shut = smoothstep(((pose.bite - 0.6) / 0.3).clamp(0.0, 1.0));
    for (i, side) in [1.0f32, -1.0].into_iter().enumerate() {
        let held = FORCIPULE_GAPE + FORCIPULE_WIND_UP * pose.jaw.clamp(0.0, 1.0) + 0.04 * (1.0 * t + 2.0 * i as f32).sin();
        let open = held * (1.0 - shut);
        let rotation = rz(-side * open).mul_mat(&rx(-0.2 * pose.jaw));
        forcipules[i] = Placement { rotation, origin: state.forcipule_hinge(side) };
        out[centipede_bone::FORCIPULE + i] = forcipules[i];
    }
    // Neither its chin nor a fang's tip goes through the floor.
    let head = state.head_trunk();
    let chin = (0..=12).flat_map(|i| (0..=6).map(move |j| (i as f32 / 12.0, j as f32 / 6.0))).map(|(u, v)| head_place.point(head.point(u, PI + v * PI, 0.0))[1]);
    let fangs = [1.0f32, -1.0].into_iter().enumerate().flat_map(|(i, side)| {
        let at = head_place.then(&forcipules[i]);
        let path = state.forcipule_path(side);
        // The tip carries on past the curve's end by a third of a girth, and the fang is thick.
        let end = path[path.len() - 1];
        let tip = vec3::add(end, vec3::scale(vec3::normalize_or(vec3::sub(end, path[path.len() - 3]), vec3::Y), 0.34 * d.girth));
        path.into_iter().step_by(4).chain([tip]).map(move |p| at.point(p)[1] - 0.04 * d.girth)
    });
    let low = chin.chain(fangs).fold(f32::MAX, f32::min);
    if low < 0.0 {
        head_place.origin[1] -= low;
    }
    out[centipede_bone::HEAD] = head_place;

    // Antennae: held out and forward, feeling about; they sweep a little faster on the move.
    for (i, side) in [1.0f32, -1.0].into_iter().enumerate() {
        let rate = 0.9 + 0.8 * moving;
        let sway = (0.16 + 0.1 * moving) * (rate * t + 1.7 * i as f32).sin() + 0.1 * (0.5 * t + 3.0 * i as f32).sin();
        let flick = 0.12 * (rate * 1.3 * t + 0.6 * i as f32).sin();
        out[centipede_bone::ANTENNA + i] = Placement { rotation: rz(-side * (0.3 + sway)).mul_mat(&rx(0.08 + flick - 0.15 * pose.bite)), origin: state.antenna_root(side) };
    }

    // The legs.
    for k in 0..n {
        let s = (k as f32 + 0.5) * l;
        let front = (1.0 - smoothstep(s / neck)) * raise;
        let along_body = k as f32 / (n - 1).max(1) as f32;
        for side in [1.0f32, -1.0] {
            let i = state.leg_bone(k, side);
            let (yaw, roll) = if state.is_last(k) {
                // The trailing pair trail, swaying a little with the wave.
                (0.1 * (0.8 * t + if side < 0.0 { PI } else { 0.0 }).sin() * (0.4 + 0.6 * moving), 0.0)
            } else {
                // Neutral: the front legs reach forward, the rear legs back.
                let neutral = 0.2 - 0.5 * along_body;
                let c = leg_cycle(k, side, pose.stride);
                let (sweep, up) = if c < LEG_DUTY {
                    (LEG_SWEEP * (1.0 - 2.0 * c / LEG_DUTY), 0.0)
                } else {
                    let u = (c - LEG_DUTY) / (1.0 - LEG_DUTY);
                    (LEG_SWEEP * (2.0 * smoothstep(u) - 1.0), LEG_LIFT * (PI * u).sin())
                };
                let (mut yaw, mut up) = (neutral + moving * sweep, moving * up);
                // The raised front's legs wave in the air, clawing forward.
                if front > 0.0 {
                    up += front * (0.7 + 0.25 * (3.5 * t + 1.1 * k as f32 + side).sin());
                    yaw += front * (0.5 + 0.3 * (2.5 * t + 0.8 * k as f32).sin());
                }
                (yaw, up)
            };
            // Coiled, the legs draw in against the flanks.
            let (yaw, roll) = if state.is_last(k) { (yaw, roll) } else { (yaw + curl * (COIL_LEG_YAW - yaw), roll + curl * (COIL_LEG_ROLL - roll)) };
            out[i] = Placement { rotation: rz(side * yaw).mul_mat(&rot_y(-side * roll)), origin: state.hip(k, side) };
        }
    }
    out
}

#[cfg(test)]
mod centipede_tests {
    use super::*;
    use crate::centipede::CentipedeState;

    fn lowest(rig: &crate::Rig, places: &[Placement]) -> f32 {
        rig.posed_mesh(places).bounds().unwrap().0[1]
    }

    /// The lowest point of one leg's mesh in creature space.
    fn claw(rig: &crate::Rig, world: &[Placement], bone: usize) -> f32 {
        rig.bones[bone].mesh.parts.iter().flat_map(|p| &p.positions).map(|&q| world[bone].point(q)[1]).fold(f32::MAX, f32::min)
    }

    #[test]
    fn a_walking_centipede_keeps_its_belly_off_and_its_claws_on_the_floor() {
        for seed in [3, 8, 21] {
            let state = CentipedeState::roll(seed);
            let rig = state.build();
            let g = state.girth;
            for step in 0..24 {
                for pace in [0.0, 1.0, 2.0] {
                    let pose = CentipedePose { stride: step as f32 / 24.0 * TAU, pace, time: step as f32 * 0.21, ..CentipedePose::REST };
                    let places = centipede(&state, &pose);
                    let low = lowest(&rig, &places);
                    assert!(low > -0.03 * g, "{seed}: sinks to {low} at step {step}, pace {pace}");
                    assert!(low < 0.03 * g, "{seed}: floats at {low} at step {step}, pace {pace}");
                    let world = rig.world(&places);
                    for k in 0..state.segments - 1 {
                        for side in [1.0, -1.0] {
                            let c = leg_cycle(k, side, pose.stride);
                            let bottom = claw(&rig, &world, state.leg_bone(k, side));
                            if c < LEG_DUTY || pace == 0.0 {
                                assert!(bottom.abs() < 0.03 * g, "{seed}: leg {k}/{side} claw at {bottom}, step {step}, pace {pace}");
                            } else {
                                assert!(bottom > -0.03 * g, "{seed}: leg {k}/{side} sinks to {bottom} in its swing");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_legs_ripple_down_the_body_in_a_wave() {
        let state = CentipedeState::roll(5);
        let at = |k: usize| leg_cycle(k, 1.0, 1.0);
        // Neighbours are a little apart in the cycle and the two sides half a cycle apart.
        for k in 0..state.segments - 1 {
            let lag = (at(k + 1) - at(k)).rem_euclid(1.0);
            assert!((lag - LEG_LAG / TAU).abs() < 1e-4, "{lag}");
            let across = (leg_cycle(k, -1.0, 1.0) - at(k)).rem_euclid(1.0);
            assert!((across - 0.5).abs() < 1e-4, "{across}");
        }
        // At one moment some legs are planted and others lifted.
        let planted = (0..state.segments).filter(|&k| at(k) < LEG_DUTY).count();
        assert!(planted > 0 && planted < state.segments, "{planted} of {}", state.segments);
        // A lifted leg rolls up; a planted one hangs where it did.
        let pose = CentipedePose { stride: 1.0, pace: 1.0, ..CentipedePose::REST };
        let places = centipede(&state, &pose);
        let tip_up = |k: usize| places[state.leg_bone(k, 1.0)].rotation.transform([1.0, 0.0, 0.0])[2];
        let lifted = (0..state.segments - 1).find(|&k| at(k) > LEG_DUTY + 0.2).unwrap();
        let down = (0..state.segments - 1).find(|&k| at(k) < LEG_DUTY - 0.1).unwrap();
        assert!(tip_up(lifted) > 0.05 && tip_up(down).abs() < 1e-5, "{} {}", tip_up(lifted), tip_up(down));
    }

    #[test]
    fn the_body_follows_a_shallow_wave_and_keeps_its_length() {
        let state = CentipedeState::roll(2);
        let d = state.dims();
        let walk = |time: f32| centipede(&state, &CentipedePose { time, pace: 1.0, ..CentipedePose::REST });
        let a = walk(0.0);
        let b = walk(1.6);
        let sway = (0..state.segments).map(|k| (a[state.segment_bone(k)].origin[0] - b[state.segment_bone(k)].origin[0]).abs()).fold(0.0, f32::max);
        assert!(sway > 0.02 * d.girth && sway < 2.0 * d.girth, "{sway}");
        // Plates keep their spacing along the body.
        for places in [&a, &b] {
            for k in 0..state.segments - 1 {
                let gap = vec3::distance(places[state.segment_bone(k)].origin, places[state.segment_bone(k + 1)].origin);
                assert!((gap - d.segment).abs() < 0.08 * d.segment, "plate {k}: {gap} vs {}", d.segment);
            }
        }
        // And the whole is centred on the origin.
        let centre = (0..state.segments).fold([0.0f32; 2], |m, k| {
            let o = a[state.segment_bone(k)].origin;
            [m[0] + o[0], m[1] + o[2]]
        });
        assert!(centre[0].abs() < 0.1 * state.segments as f32 && centre[1].abs() < 0.1 * state.segments as f32, "{centre:?}");
    }

    #[test]
    fn rearing_lifts_the_front_and_a_bite_lunges() {
        let state = CentipedeState::roll(4);
        let rig = state.build();
        let head = |pose: CentipedePose| centipede(&state, &pose)[centipede_bone::HEAD].origin;
        let flat = head(CentipedePose::REST);
        let reared = head(CentipedePose { rear: 1.0, ..CentipedePose::REST });
        assert!(reared[1] > flat[1] + 1.5 * state.girth, "{flat:?} {reared:?}");
        // The rest of the body stays down, the back plates on the floor.
        let places = centipede(&state, &CentipedePose { rear: 1.0, ..CentipedePose::REST });
        let last = state.segment_bone(state.segments - 1);
        assert!((places[last].origin[1] - state.centre_height(state.segments - 1)).abs() < 1e-3);
        assert!(lowest(&rig, &places) > -0.03 * state.girth);
        // A bite throws the head forward along the body.
        let drawn = head(CentipedePose::REST);
        let lunged = head(CentipedePose { bite: 1.0, jaw: 1.0, ..CentipedePose::REST });
        assert!(lunged[2] > drawn[2] + 0.5 * state.girth, "{drawn:?} {lunged:?}");
        let places = centipede(&state, &CentipedePose { bite: 1.0, jaw: 1.0, ..CentipedePose::REST });
        assert!(lowest(&rig, &places) > -0.03 * state.girth);
        // The forcipules open apart.
        let tips = |pose: CentipedePose| {
            let places = centipede(&state, &pose);
            let world = rig.world(&places);
            let tip = |bone: &str| {
                let i = rig.index(bone).unwrap();
                rig.bones[i].mesh.parts.iter().flat_map(|p| &p.positions).map(|&q| world[i].point(q)).fold([0.0f32, 0.0, f32::MIN], |m, q| if q[2] > m[2] { q } else { m })
            };
            (tip("forcipule_right"), tip("forcipule_left"))
        };
        let (shut, wide) = (tips(CentipedePose::REST), tips(CentipedePose { jaw: 1.0, ..CentipedePose::REST }));
        // Creature space has the left toward +X: shut, the fangs cross; open, each is on its own side.
        assert!(wide.1[0] - wide.0[0] > shut.1[0] - shut.0[0] + 0.5 * state.girth, "{shut:?} {wide:?}");
    }

    #[test]
    fn the_forcipules_stand_apart_and_close_only_on_the_bite() {
        for seed in 0..20 {
            let state = CentipedeState::roll(seed);
            // The tip of each fang in the head's frame, the sharp point past the curve's end.
            let tips = |pose: CentipedePose| {
                let places = centipede(&state, &pose);
                [1.0f32, -1.0].map(|side| {
                    let i = if side > 0.0 { 0 } else { 1 };
                    let path = state.forcipule_path(side);
                    let end = path[path.len() - 1];
                    let dir = vec3::normalize_or(vec3::sub(end, path[path.len() - 3]), vec3::Y);
                    let tip = vec3::add(end, vec3::scale(dir, 0.34 * state.girth));
                    places[centipede_bone::FORCIPULE + i].point(tip)[0] * side
                })
            };
            for pose in [CentipedePose::REST, CentipedePose { stride: 1.0, pace: 1.0, time: 2.0, ..CentipedePose::REST }, CentipedePose { bite: 0.3, jaw: 0.3, ..CentipedePose::REST }] {
                let t = tips(pose);
                assert!(t[0] > 0.1 * state.girth && t[1] > 0.1 * state.girth, "seed {seed}: the tips stay apart: {t:?}");
            }
            let shut = tips(CentipedePose { bite: 1.0, jaw: 1.0, ..CentipedePose::REST });
            assert!(shut[0] > 0.0 && shut[1] > 0.0 && shut[0] < 0.25 * state.girth, "seed {seed}: shut, the tips meet without crossing: {shut:?}");
        }
    }

    #[test]
    fn a_coiled_centipede_lies_in_a_tight_flat_spiral() {
        for seed in [3, 8, 21, 40] {
            let state = CentipedeState::roll(seed);
            let rig = state.build();
            let rest = centipede(&state, &CentipedePose::REST);
            assert_eq!(centipede(&state, &CentipedePose { curl: 0.0, ..CentipedePose::REST }), rest);
            let coiled = centipede(&state, &CentipedePose { curl: 1.0, ..CentipedePose::REST });
            let centres: Vec<V3> = (0..state.segments).map(|k| coiled[state.segment_bone(k)].origin).collect();
            let mid = centres.iter().fold([0.0f32; 2], |m, c| [m[0] + c[0] / centres.len() as f32, m[1] + c[2] / centres.len() as f32]);
            let far = centres.iter().map(|c| ((c[0] - mid[0]).powi(2) + (c[2] - mid[1]).powi(2)).sqrt()).fold(0.0, f32::max);
            assert!(far < state.length / 3.0, "seed {seed}: the coil spans {far} of a body {} long", state.length);
            for (k, c) in centres.iter().enumerate() {
                assert!((c[1] - state.centre_height(k)).abs() < 1e-3, "plate {k} stays on the floor");
            }
            let low = lowest(&rig, &coiled);
            assert!(low > -0.05 * state.girth, "seed {seed}: the coil does not sink into the floor: {low}");
            // The head lies at the middle of the coil, nearer it than the tail.
            let head = coiled[centipede_bone::HEAD].origin;
            let tail = centres[state.segments - 1];
            let dist = |a: V3| ((a[0] - mid[0]).powi(2) + (a[2] - mid[1]).powi(2)).sqrt();
            assert!(dist(head) < dist(tail), "seed {seed}: the head is tucked in");
            // Partway, it is between.
            let half = centipede(&state, &CentipedePose { curl: 0.5, ..CentipedePose::REST });
            assert_ne!(half, rest);
            assert_ne!(half, coiled);
        }
    }

    #[test]
    fn a_centipede_is_a_pure_function_of_its_pose() {
        let state = CentipedeState::roll(9);
        let pose = CentipedePose { stride: 2.0, pace: 0.7, time: 3.0, rear: 0.4, bite: 0.2, jaw: 0.3, look: 0.2, curl: 0.5 };
        assert_eq!(centipede(&state, &pose), centipede(&state, &pose));
        assert_eq!(centipede(&state, &pose).len(), state.bone_count());
        assert!(centipede_stride(&state) > state.girth);
    }
}

// Bats

/// How a bat is flying or roosting at a moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BatPose {
    /// The wingbeat's phase, radians: 0 the top of the upstroke, half a turn the bottom of the
    /// downstroke. One turn per beat ([`bat_beat_hz`] beats a second).
    pub flap: f32,
    /// How hard it beats: 0 gliding on spread wings, 1 beating.
    pub power: f32,
    /// 0 flying, 1 hanging head down by its feet with its wings folded round it.
    pub roost: f32,
    /// How folded the wings are: 0 spread, 1 folded (a roosting bat's, whatever its roost).
    pub tuck: f32,
    /// Height of the body's middle over the creature's root, world units.
    pub height: f32,
    /// A bite's lunge, 0 to 1 at full stretch: the head thrown forward and the jaw open.
    pub bite: f32,
    /// How far the nose is raised, radians (negative dives).
    pub pitch: f32,
    /// How far it banks toward its left (+X), radians; turned over (a fallen bat) at pi.
    pub bank: f32,
    /// How far the head turns toward its left, radians.
    pub look: f32,
    /// Seconds, for what it does hanging and gliding: breathing, the head's sway.
    pub time: f32,
}

impl Default for BatPose {
    /// Gliding level on spread wings: the rest pose.
    fn default() -> Self {
        BatPose { flap: 0.0, power: 0.0, roost: 0.0, tuck: 0.0, height: 0.0, bite: 0.0, pitch: 0.0, bank: 0.0, look: 0.0, time: 0.0 }
    }
}

/// Wingbeats a second when it beats flat out: the small bat's quick, the giant bat's slow. Our
/// own, set so a beat reads at sixty frames a second.
pub fn bat_beat_hz(state: &BatState) -> f32 {
    if state.species == crate::Species::GiantBat { 2.2 } else { 4.2 }
}

/// How far the nose points down hanging: just short of straight down, radians.
const ROOST_PITCH: f32 = 1.5;
/// A gliding wing's rise from level, radians: a shallow V.
const GLIDE_DIHEDRAL: f32 = 0.2;
/// A beating wing's middle rise from level and its swing either side, radians.
const BEAT_MIDDLE: f32 = 0.1;
const BEAT_SWING: f32 = 0.85;

/// Reflects a rotation across the plane x = 0: the left side's twin of a right side's turn.
fn mirrored(m: &Mat3) -> Mat3 {
    let flip = Mat3::from_scale([-1.0, 1.0, 1.0]);
    flip.mul_mat(m).mul_mat(&flip)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Every bone's placement on its parent for `pose`, in the bat's [`crate::Rig`] order.
///
/// A wing beats about the line of the body: the arm turns up and down on the shoulder while the
/// forearm and the hand fold on their hinges ([`crate::bat::WingDims`]), so the upstroke folds
/// the wing in and the downstroke spreads it, the hand lagging a little behind the arm. Gliding,
/// the wings are held in a shallow V. Roosting, the body hangs head down by its feet, the head
/// curled up to look out, and the wings fold flat against the body.
pub fn bat(state: &BatState, pose: &BatPose) -> Vec<Placement> {
    let d = state.dims();
    let l = d.length;
    let roost = pose.roost.clamp(0.0, 1.0);
    let tuck = pose.tuck.clamp(0.0, 1.0).max(roost);
    let power = pose.power.clamp(0.0, 1.0) * (1.0 - tuck);
    let t = pose.time;
    let (s, c) = pose.flap.sin_cos();
    let mut out = vec![Placement::IDENTITY; bat_bone::COUNT];

    // The body: heaved up by the downstroke, nosing about its flight path, or hung upside down.
    let heave = 0.05 * l * power * s;
    let nose = lerp(pose.pitch + 0.05 * power * c, -ROOST_PITCH, roost);
    let sway = roost * 0.05 * (0.9 * t).sin() + (1.0 - roost) * (1.0 - power) * 0.03 * (0.7 * t).sin();
    // Nose up is a turn about creature X that takes +Z toward +Y, a negative one.
    let rotation = Mat3::from_axis_angle(vec3::Z, -(pose.bank + sway)).mul_mat(&Mat3::from_axis_angle(vec3::X, -nose)).mul_mat(&spine_to_creature());
    out[bat_bone::BODY] = Placement { rotation, origin: [0.0, pose.height + heave, 0.0] };

    // The head: nose a little down flying, thrown forward to bite, curled up hanging.
    let breath = roost * 0.05 * (1.7 * t + 0.6).sin();
    let head_pitch = lerp(-0.12 - 0.3 * pose.bite, 1.2, roost) + breath;
    let look = pose.look + roost * 0.5 * (0.31 * t).sin();
    out[bat_bone::HEAD] = Placement { rotation: Mat3::from_axis_angle(vec3::Z, look).mul_mat(&Mat3::from_axis_angle(vec3::X, head_pitch)), origin: vec3::add(d.neck, [0.0, 0.12 * l * pose.bite, 0.0]) };
    let (_, hinge) = state.mouth(&d);
    let gape = 0.06 + 0.85 * pose.bite.clamp(0.0, 1.0).sqrt();
    out[bat_bone::JAW] = Placement { rotation: Mat3::from_axis_angle(vec3::X, -gape), origin: hinge };

    // The right wing; the left is its mirror.
    let w = d.wing;
    let flight_flap = lerp(GLIDE_DIHEDRAL + 0.04 * (1.3 * t).sin(), BEAT_MIDDLE + BEAT_SWING * c, power);
    let flight_sweep = lerp(0.15, 0.18 - 0.2 * s, power);
    let lag = (pose.flap - 0.7).sin();
    let flight_elbow = lerp(-0.06, 0.3 * s.max(0.0) + 0.9 * s.min(0.0), power);
    let flight_wrist = lerp(-0.12, 0.4 * lag.max(0.0) + 1.1 * lag.min(0.0), power);
    let up = lerp(flight_flap, -1.45, tuck);
    let back = lerp(flight_sweep, 1.1, tuck);
    let elbow = lerp(flight_elbow, 2.9, tuck);
    let wrist = lerp(flight_wrist, 2.9, tuck);
    let arm = Mat3::from_axis_angle(vec3::Y, -up).mul_mat(&Mat3::from_axis_angle(vec3::Z, -back));
    let right = [
        Placement { rotation: arm, origin: d.shoulder },
        Placement { rotation: Mat3::from_axis_angle(w.elbow_axis(), elbow), origin: w.elbow },
        Placement { rotation: Mat3::from_axis_angle(w.wrist_axis(), wrist), origin: vec3::sub(w.wrist, w.elbow) },
    ];
    for (k, p) in right.iter().enumerate() {
        out[bat_bone::wing(0) + k] = *p;
        out[bat_bone::wing(1) + k] = Placement { rotation: mirrored(&p.rotation), origin: [-p.origin[0], p.origin[1], p.origin[2]] };
    }

    // The hind legs: trailing behind, splayed a little, or gripping above.
    let flutter = (1.0 - roost) * 0.06 * power * (pose.flap + 0.5).sin();
    let splay = lerp(0.28, 0.5, roost);
    let tilt = lerp(0.3, -0.15, roost) + flutter;
    let leg = Mat3::from_axis_angle(vec3::Z, splay).mul_mat(&Mat3::from_axis_angle(vec3::X, tilt));
    out[bat_bone::LEGS] = Placement { rotation: leg, origin: d.hip };
    out[bat_bone::LEGS + 1] = Placement { rotation: mirrored(&leg), origin: [-d.hip[0], d.hip[1], d.hip[2]] };
    out
}

#[cfg(test)]
mod minotaur_tests {
    use super::*;
    use crate::skeleton::index as skeleton_bone;
    use crate::denizen::TAIL_FIRST;
    use crate::minotaur::MinotaurState;

    /// The lowest y of the parts named `name` on the foot bones of a rig posed by `places`.
    fn hoof_sole(rig: &crate::Rig, places: &[Placement], side: Side) -> f32 {
        let world = rig.world(places);
        let foot = leg_bone(side) + 2;
        rig.bones[foot].mesh.parts.iter().filter(|p| p.name == "hoof").flat_map(|p| &p.positions).map(|&q| world[foot].point(q)[1]).fold(f32::MAX, f32::min)
    }

    fn walking(stride: f32, time: f32) -> DenizenPose {
        DenizenPose { stride, pace: 1.0, time, ..DenizenPose::default() }
    }

    #[test]
    fn a_walking_minotaur_keeps_its_hooves_on_the_floor_and_never_sinks() {
        for seed in [1, 7, 30, 211] {
            let state = MinotaurState::roll(seed);
            let (rig, d) = (state.build(), state.dims());
            let h = state.height();
            for step in 0..32 {
                let pose = walking(step as f32 / 32.0 * TAU, step as f32 * 0.13);
                let places = minotaur_with(&d, &pose);
                assert_eq!(places.len(), rig.bones.len());
                let low = rig.posed_mesh(&places).bounds().unwrap().0[1];
                assert!(low > -0.02 * h, "seed {seed} sinks to {low} at step {step}");
                for (k, side) in Side::BOTH.into_iter().enumerate() {
                    // A foot is down through its stance, and then its sole is on the floor.
                    let c = (pose.stride / TAU + if k == 0 { 0.5 } else { 0.0 }).rem_euclid(1.0);
                    let sole = hoof_sole(&rig, &places, side);
                    if c < DUTY {
                        assert!(sole.abs() < 0.02 * h, "seed {seed}: hoof {k} at {sole}, step {step}");
                    } else {
                        assert!(sole > -0.02 * h, "seed {seed}: hoof {k} sinks to {sole}, step {step}");
                    }
                }
            }
        }
    }

    #[test]
    fn standing_and_every_blow_and_gesture_keep_the_body_off_the_floor_and_the_feet_on_it() {
        for seed in [3, 7, 44] {
            let state = MinotaurState::roll(seed);
            let (rig, d) = (state.build(), state.dims());
            let h = state.height();
            for step in 0..=20 {
                let p = step as f32 / 20.0;
                let poses = [
                    DenizenPose { time: p * 3.0, ..DenizenPose::default() },
                    DenizenPose { swing: Some(p), ..DenizenPose::default() },
                    DenizenPose { gore: Some(p), ..DenizenPose::default() },
                    DenizenPose { gesture: Some(p), time: p * 4.0, ..DenizenPose::default() },
                ];
                for (which, pose) in poses.iter().enumerate() {
                    let places = minotaur_with(&d, pose);
                    let low = rig.posed_mesh(&places).bounds().unwrap().0[1];
                    assert!(low > -0.02 * h, "seed {seed} pose {which} sinks to {low} at {p}");
                    // Both hooves stay planted through a blow, or a gesture, or standing.
                    for side in Side::BOTH {
                        let sole = hoof_sole(&rig, &places, side);
                        assert!(sole.abs() < 0.02 * h, "seed {seed} pose {which}: hoof at {sole} at {p}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_gore_drives_the_head_forward_and_a_chop_raises_the_hands() {
        let state = MinotaurState::roll(7);
        let (rig, d) = (state.build(), state.dims());
        let skull = |pose: &DenizenPose| rig.world(&minotaur_with(&d, pose))[skeleton_bone::SKULL];
        let hand = |pose: &DenizenPose| rig.world(&minotaur_with(&d, pose))[arm_bone(Side::Right) + 2].origin;
        let rest = DenizenPose::default();
        let thrust = skull(&DenizenPose { gore: Some(0.6), ..rest }).origin;
        assert!(thrust[2] > skull(&rest).origin[2] + 3.0, "{thrust:?}");
        // Drawn back first.
        assert!(skull(&DenizenPose { gore: Some(0.28), ..rest }).origin[2] < skull(&rest).origin[2]);
        let up = hand(&DenizenPose { swing: Some(0.45), ..rest });
        assert!(up[1] > skull(&rest).origin[1], "the axe hand is over the head: {up:?}");
        assert!(hand(&DenizenPose { swing: Some(0.7), ..rest })[1] < up[1] - 5.0, "and comes down through the blow");
    }

    #[test]
    fn a_gesture_raises_a_hand_and_returns_it() {
        let state = MinotaurState::roll(7);
        let (rig, d) = (state.build(), state.dims());
        let hand = |p: Option<f32>| rig.world(&minotaur_with(&d, &DenizenPose { gesture: p, ..DenizenPose::default() }))[arm_bone(Side::Right) + 2].origin;
        let (start, mid, end) = (hand(Some(0.0)), hand(Some(0.5)), hand(Some(1.0)));
        assert!(mid[2] > start[2] + 2.5, "held out in front of it: {mid:?} from {start:?}");
        assert!(mid[1] > start[1] + 4.0, "and lifted: {mid:?} from {start:?}");
        assert!(vec3::distance(start, end) < 0.5, "back at its side: {start:?} {end:?}");
        assert!(vec3::distance(hand(None), start) < 0.5, "a gesture begins where standing is");
    }

    #[test]
    fn the_tail_swings_and_every_placement_is_a_proper_turn() {
        let state = MinotaurState::roll(7);
        let d = state.dims();
        let at = |t: f32| minotaur_with(&d, &DenizenPose { time: t, ..DenizenPose::default() });
        let (a, b) = (at(0.0), at(1.7));
        assert_ne!(a[TAIL_FIRST].rotation, b[TAIL_FIRST].rotation);
        assert_ne!(a[TAIL_FIRST + 2].rotation, b[TAIL_FIRST + 2].rotation);
        for pose in [walking(1.0, 0.5), DenizenPose { swing: Some(0.5), ..DenizenPose::default() }, DenizenPose { gore: Some(0.5), ..DenizenPose::default() }, DenizenPose { gesture: Some(0.4), ..DenizenPose::default() }] {
            for place in minotaur_with(&d, &pose) {
                assert!((place.rotation.determinant() - 1.0).abs() < 1e-3, "{pose:?}");
            }
        }
        // Fallen, it has every bone too.
        assert_eq!(minotaur_fallen(&d, 0.7).len(), TAIL_FIRST + crate::denizen::TAIL_BONES);
    }
}
