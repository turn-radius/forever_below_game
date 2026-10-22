//! SPIKE (`FB_LOOK=pulp`): static standing poses, plain data. A [`StandPose`] is a weight
//! shift (contrapposto), a torso and head turn, and a carriage for each arm chosen by what the
//! hand holds. [`crate::Gait::idle`] uses one when the gait was given it
//! ([`crate::Gait::stood`]); walking is untouched. Picked from the outfit and the seed by
//! [`StandPose::choose`]. `FB_STAND=key=value,...` adds to any number while tuning (see
//! [`StandPose::tune`]).

use crate::held::Hold;
use crate::rig::Quat;

/// One arm's carriage: radians added to the rig pose, on top of the arm as fitted to its grip.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Carry {
    /// Shoulder swing about x (negative forward), turn about y, roll about z.
    pub shoulder: [f32; 3],
    /// Elbow bend about x (negative bends the forearm forward).
    pub elbow: f32,
    /// Wrist turn about x, y, z: reorients what the hand holds.
    pub wrist: [f32; 3],
}

impl Carry {
    pub const NONE: Carry = Carry { shoulder: [0.0; 3], elbow: 0.0, wrist: [0.0; 3] };

    pub fn shoulder_q(&self) -> Quat {
        Quat::y(self.shoulder[1]).mul(Quat::z(self.shoulder[2])).mul(Quat::x(self.shoulder[0]))
    }
    pub fn elbow_q(&self) -> Quat {
        Quat::x(self.elbow)
    }
    pub fn wrist_q(&self) -> Quat {
        Quat::y(self.wrist[1]).mul(Quat::z(self.wrist[2])).mul(Quat::x(self.wrist[0]))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StandPose {
    pub name: &'static str,
    /// Which foot bears the weight: -1 right, 1 left (as [`crate::IdleCycle::shift`]).
    pub weight: f32,
    /// Pelvis roll, radians: the loaded hip rises.
    pub roll: f32,
    /// The pelvis slides over the loaded foot, as a share of the hips' half spacing.
    pub sway: f32,
    /// The loose foot's ankle relative to its rest, in leg lengths: out, up, forward.
    pub loose: [f32; 3],
    /// The loose foot's toe-down pitch.
    pub loose_pitch: f32,
    /// How far each foot turns out (loaded, loose), radians.
    pub turn: [f32; 2],
    /// Yaw, roll, pitch of the spine, the chest and the head.
    pub spine: [f32; 3],
    pub chest: [f32; 3],
    pub head: [f32; 3],
    /// Right then left.
    pub arms: [Carry; 2],
}

fn digit(c: Option<char>) -> usize {
    c.map_or(0, |c| (c as usize).wrapping_sub('0' as usize) % 3)
}

impl StandPose {
    /// Weight on the left leg, the right knee soft and turned out, the shoulders turned a
    /// little against the hips and the head to the right: the classical stance.
    pub const CONTRAPPOSTO: StandPose = StandPose {
        name: "contrapposto",
        weight: 1.0,
        roll: 0.11,
        sway: 0.55,
        loose: [0.06, 0.02, 0.06],
        loose_pitch: 0.25,
        turn: [0.14, 0.5],
        spine: [-0.10, -0.08, 0.0],
        chest: [-0.12, -0.05, -0.02],
        head: [0.22, 0.06, 0.03],
        arms: [Carry::NONE; 2],
    };

    /// The pose for someone holding `main` and `off`, a little different for each `seed`.
    pub fn choose(main: Option<Hold>, off: Option<Hold>, seed: u32) -> StandPose {
        let variant = (seed.wrapping_mul(2654435761) >> 28) as usize;
        let mut pose = StandPose::CONTRAPPOSTO;
        if variant % 2 == 1 {
            pose.head[0] = -pose.head[0];
        }
        let mut right = Carry::NONE;
        let mut left = Carry::NONE;
        match main {
            Some(Hold::OneHanded) => {
                pose.name = if variant % 3 == 0 { "sword shoulder" } else { "sword point down" };
                right = if variant % 3 == 0 { SWORD_SHOULDER } else { SWORD_DOWN };
            }
            Some(Hold::Upright) => {
                pose.name = "staff grounded";
                right = STAFF;
            }
            _ => {}
        }
        if off == Some(Hold::Shield) {
            left = SHIELD;
            if main.is_none() {
                pose.name = "shield braced";
            }
        }
        pose.arms = [right, left];
        pose.tune()
    }

    /// Adds `FB_STAND`'s `key=value` pairs (comma separated), for tuning: `weight`, `roll`,
    /// `sway`, `lo`, `lu`, `lf`, `lp`, `t0`, `t1`, `s0..2`, `c0..2`, `h0..2`, and per arm
    /// `r`/`l` + `s0..2` (shoulder), `e` (elbow), `w0..2` (wrist), e.g. `rs0=-0.4,le=0.3`.
    pub fn tune(mut self) -> StandPose {
        let Ok(text) = std::env::var("FB_STAND") else { return self };
        for pair in text.split(',') {
            let Some((k, v)) = pair.split_once('=') else { continue };
            let Ok(v) = v.trim().parse::<f32>() else { continue };
            let k = k.trim();
            let mut ch = k.chars();
            let (first, second) = (ch.next(), ch.next());
            match k {
                "weight" => self.weight = v,
                "roll" => self.roll += v,
                "sway" => self.sway += v,
                "lo" => self.loose[0] += v,
                "lu" => self.loose[1] += v,
                "lf" => self.loose[2] += v,
                "lp" => self.loose_pitch += v,
                "t0" => self.turn[0] += v,
                "t1" => self.turn[1] += v,
                _ if k.len() == 2 && first == Some('s') => self.spine[digit(second)] += v,
                _ if k.len() == 2 && first == Some('c') => self.chest[digit(second)] += v,
                _ if k.len() == 2 && first == Some('h') => self.head[digit(second)] += v,
                _ if k.len() >= 2 && (first == Some('r') || first == Some('l')) => {
                    let arm = &mut self.arms[usize::from(first == Some('l'))];
                    let rest = &k[1..];
                    let mut rc = rest.chars();
                    match (rc.next(), rc.next()) {
                        (Some('e'), None) => arm.elbow += v,
                        (Some('s'), d) => arm.shoulder[digit(d)] += v,
                        (Some('w'), d) => arm.wrist[digit(d)] += v,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        self
    }
}

// Arm carriages; values tuned by eye against the captures.
pub const SWORD_SHOULDER: Carry = Carry { shoulder: [0.0, 0.0, 0.35], elbow: -0.5, wrist: [-0.9, 0.0, 0.0] };
pub const SWORD_DOWN: Carry = Carry { shoulder: [-0.3, 0.0, -0.08], elbow: -0.3, wrist: [2.95, 0.0, 0.0] };
pub const STAFF: Carry = Carry { shoulder: [0.5, 0.0, -0.1], elbow: -0.2, wrist: [-0.2, 0.0, -0.05] };
pub const SHIELD: Carry = Carry { shoulder: [0.0, 0.0, -0.3], elbow: 0.0, wrist: [0.0; 3] };
