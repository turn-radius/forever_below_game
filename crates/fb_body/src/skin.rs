//! Skin weights: every vertex of the bare skin and of worn gear gets up to four joints and
//! their weights ([`PartSkin`]), so legwear, boots, sleeves, gloves and cloaks bend with the
//! limbs they cover. Plain Rust, no Bevy.
//!
//! The weight of a vertex is a function of where it rests and of the region it belongs to
//! ([`SkinRegion`]), never of its mesh, so gear fitted a little off the skin gets almost the
//! weights of the skin under it, and stays on it however the rig is posed.
//!
//! - **Along a limb** the weights blend smoothly over each joint: the trunk into the upper arm
//!   or thigh over the shoulder or hip, then the forearm or shin over the elbow or knee, then
//!   the hand or foot over the wrist or ankle (a hand is stiff, so it follows the forearm).
//! - **Across the body** the trunk's weights follow its height: pelvis, spine, chest, and the
//!   head up the neck.
//! - **Near or far.** A vertex follows a limb the more it hugs it (its distance to the limb's
//!   centre line over the limb's radius there). Loose cloth, which hangs well off the limbs,
//!   follows the pelvis and chest, with a share of the thighs that grows toward the hem and
//!   splits across the midline, so a skirt or cloak neither tears between the legs nor stays
//!   still through them. Our own scheme; the prototype had no skeleton.
//! - **Held items** have no weights: they ride a joint rigidly (`crate::rig::Joint::wrist`).

use armory::build::form::{BACK, FRONT, TorsoProfile};
use armory::build::hafted::smoothstep;
use armory::geom::vec3::{self, V3};
use armory::{ItemMesh, Part};
use fb_items::Slot;

use crate::body::{Body, TRUNK_EXPONENT};
use crate::limb::{Limb, Side};
use crate::rig::{JOINTS, Joint};

/// The share of the legs loose cloth follows at the ankles, growing from none at the crotch.
/// It was 0.4 while skirts ended at the knee; a robe that hangs to the feet needs most of its
/// hem to go with the shins, or the shins stride out through it.
pub(crate) const HEM_SHARE: f32 = 0.75;

/// Four joint indices and weights for each vertex of one part, parallel to its positions.
/// Weights are non-negative and sum to 1; unused slots are joint 0 with weight 0.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartSkin {
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
}

/// Which joints a vertex may follow, and how it picks them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkinRegion {
    /// Gear on trunk and limbs alike: it follows whatever it hugs (a shirt, armour).
    Auto,
    /// Gear on the legs and hips (trousers): the trunk and the legs only.
    Legs,
    /// The trunk alone, by height (the bare trunk, a belt, a pack, a pendant).
    Trunk,
    /// Loose cloth (a cloak): the trunk, and the thighs a little toward the hem.
    Loose,
    /// A tube round an arm, whichever arm it is nearest (bare arms, sleeves).
    Arm,
    /// A hand or glove: the nearest wrist.
    Hand,
    /// A tube round a leg (the bare legs).
    Leg,
    /// A foot or boot: the nearest ankle.
    Foot,
    /// The head, which turns about the neck.
    Head,
    /// Rigid on the pelvis (a tail).
    Pelvis,
    /// Rigid on the chest (a hood and the head inside it, which turn with the body).
    Chest,
}

impl SkinRegion {
    /// The region of a bare-skin part ([`Body::skin_mesh`]), by its name.
    pub fn of_skin(part: &str) -> SkinRegion {
        match part {
            "arms" => SkinRegion::Arm,
            "hands" | "nails" => SkinRegion::Hand,
            "legs" => SkinRegion::Leg,
            "feet" | "claws" => SkinRegion::Foot,
            "tail" => SkinRegion::Pelvis,
            _ => SkinRegion::Trunk,
        }
    }

    /// The region of a part of the item worn in `slot`.
    pub fn of_worn(slot: Slot, part: &str) -> SkinRegion {
        match slot {
            Slot::Legs => SkinRegion::Legs,
            Slot::Feet => SkinRegion::Foot,
            Slot::Hands => SkinRegion::Hand,
            Slot::Belt | Slot::Neck | Slot::Back => SkinRegion::Trunk,
            Slot::Head => SkinRegion::Head,
            Slot::Cloak => SkinRegion::Loose,
            _ if part == "sleeves" => SkinRegion::Arm,
            _ => SkinRegion::Auto,
        }
    }
}

/// One limb as the weights see it.
#[derive(Clone, Debug)]
struct LimbFit {
    limb: Limb,
    /// Length along the path at each point.
    cum: Vec<f32>,
    len: f32,
    /// Length at the first two joints.
    at: [f32; 2],
    /// Half widths of the blends over each joint.
    zone: [f32; 3],
    joints: [Joint; 3],
    parent: Joint,
    end: V3,
    end_dir: V3,
    /// The hand or foot beyond the end of the limb, as a line: gear on it hugs the limb too.
    extension: Vec<V3>,
    arm: bool,
}

impl LimbFit {
    fn new(limb: &Limb, arm: bool, reach: f32) -> LimbFit {
        let side = limb.side;
        let mut cum = vec![0.0];
        for w in limb.path.windows(2) {
            cum.push(cum.last().copied().unwrap_or(0.0) + vec3::distance(w[0], w[1]));
        }
        let len = cum.last().copied().unwrap_or(1.0).max(1e-4);
        let at = [limb.t_joints[0] * len, limb.t_joints[1] * len];
        let r = |t: f32| limb.radius_at(t);
        let zone = if arm {
            [1.0 * r(limb.t_joints[0]), 1.1 * r(limb.t_joints[1]), 0.5 * r(1.0)]
        } else {
            [1.1 * r(limb.t_joints[0]), 1.2 * r(limb.t_joints[1]), 0.9 * r(1.0)]
        };
        let n = limb.path.len();
        let end = limb.path[n - 1];
        let end_dir = vec3::normalize_or(vec3::sub(end, limb.path[n - 3]), vec3::neg(vec3::Y));
        let extension = if arm {
            vec![end, vec3::add(end, vec3::scale(end_dir, reach))]
        } else {
            // Down to the sole and out along the foot.
            let sole = [end[0], 0.15, end[2]];
            vec![end, sole, vec3::add(sole, [0.0, 0.0, reach])]
        };
        LimbFit {
            limb: limb.clone(),
            cum,
            len,
            at,
            zone,
            joints: if arm { Joint::arm(side) } else { Joint::leg(side) },
            parent: if arm { Joint::Chest } else { Joint::Pelvis },
            end,
            end_dir,
            extension,
            arm,
        }
    }

    /// Where `p` projects onto the limb: the length along it (past the end, along the last
    /// segment's line) and its distance to the nearest point.
    fn locate(&self, p: V3) -> (f32, f32) {
        let path = &self.limb.path;
        let mut best = (f32::MAX, 0.0);
        for (i, w) in path.windows(2).enumerate() {
            let seg = vec3::sub(w[1], w[0]);
            let l2 = vec3::length_squared(seg).max(1e-12);
            let s = (vec3::dot(vec3::sub(p, w[0]), seg) / l2).clamp(0.0, 1.0);
            let d = vec3::distance(p, vec3::add(w[0], vec3::scale(seg, s)));
            if d < best.0 {
                best = (d, self.cum[i] + s * (self.cum[i + 1] - self.cum[i]));
            }
        }
        let (mut d, mut u) = best;
        // The hand or foot beyond: nearer than the limb's end, it counts as past it.
        for w in self.extension.windows(2) {
            let seg = vec3::sub(w[1], w[0]);
            let l2 = vec3::length_squared(seg).max(1e-12);
            let s = (vec3::dot(vec3::sub(p, w[0]), seg) / l2).clamp(0.0, 1.0);
            let e = vec3::distance(p, vec3::add(w[0], vec3::scale(seg, s)));
            if e < d {
                (d, u) = (e, self.len + 1.0);
            }
        }
        if u >= self.len - 1e-4 {
            u = self.len + vec3::dot(vec3::sub(p, self.end), self.end_dir);
        }
        (u, d)
    }

    /// How closely `p` hugs the limb: its distance over the limb's radius there.
    fn hug(&self, u: f32, d: f32) -> f32 {
        // A hand or foot is broader than the wrist or ankle it hangs from.
        let wide = if u > self.len { 1.6 } else { 1.0 };
        d / (wide * self.limb.radius_at((u / self.len).clamp(0.0, 1.0))).max(1e-3)
    }

    /// The weights along the limb at length `u`.
    fn weights(&self, u: f32, out: &mut [f32; JOINTS], k: f32) {
        let s0 = smoothstep(self.at[0] - self.zone[0], self.at[0] + self.zone[0], u);
        let s1 = smoothstep(self.at[1] - self.zone[1], self.at[1] + self.zone[1], u);
        let s2 = if self.arm { smoothstep(self.len - 2.0 * self.zone[2], self.len, u) } else { smoothstep(self.len - self.zone[2], self.len + self.zone[2], u) };
        out[self.parent.index()] += k * (1.0 - s0);
        out[self.joints[0].index()] += k * s0 * (1.0 - s1);
        out[self.joints[1].index()] += k * s0 * s1 * (1.0 - s2);
        out[self.joints[2].index()] += k * s0 * s1 * s2;
    }
}

/// Weights vertices of one body: build it once, then weight every part.
#[derive(Clone, Debug)]
pub struct Skinner {
    /// Right arm, left arm, right leg, left leg.
    limbs: [LimbFit; 4],
    trunk: TorsoProfile,
    hip_y: f32,
    waist_y: f32,
    chest_y: f32,
    crotch_y: f32,
    ankle_y: f32,
    /// Where the neck starts to give way to the head, and where it is all head.
    neck_from: f32,
    neck_to: f32,
    trunk_length: f32,
    hip_half_width: f32,
}

impl Skinner {
    pub fn new(body: &Body) -> Skinner {
        let hand = 0.105 * body.height * body.race().arm;
        let foot = 0.148 * body.height * body.race().foot;
        let arms = [LimbFit::new(body.arm(Side::Right), true, hand), LimbFit::new(body.arm(Side::Left), true, hand)];
        let legs = [LimbFit::new(body.leg(Side::Right), false, foot), LimbFit::new(body.leg(Side::Left), false, foot)];
        let [a0, a1] = arms;
        let [l0, l1] = legs;
        Skinner {
            limbs: [a0, a1, l0, l1],
            trunk: body.trunk().clone(),
            hip_y: body.hip_y,
            waist_y: body.waist_y,
            chest_y: body.chest_y,
            crotch_y: body.hip_y - 0.14 * body.trunk_length - 0.25 * body.scale,
            ankle_y: body.ankle_y,
            neck_from: body.shoulder_y + 0.05 * body.trunk_length,
            neck_to: body.neck_base().origin[1] + 0.05 * body.trunk_length,
            trunk_length: body.trunk_length,
            hip_half_width: body.hip.half_width,
        }
    }

    /// Weights for every part of `mesh`, `region` giving each part's region by its name.
    pub fn mesh(&self, mesh: &ItemMesh, region: impl Fn(&str) -> SkinRegion) -> Vec<PartSkin> {
        mesh.parts.iter().map(|part| self.part(part, region(part.name))).collect()
    }

    /// Weights for every vertex of `part`.
    pub fn part(&self, part: &Part, region: SkinRegion) -> PartSkin {
        let mut skin = PartSkin::default();
        for &p in &part.positions {
            let (joints, weights) = self.vertex(p, region);
            skin.joints.push(joints);
            skin.weights.push(weights);
        }
        skin
    }

    /// The weights of a vertex resting at `p`.
    pub fn vertex(&self, p: V3, region: SkinRegion) -> ([u16; 4], [f32; 4]) {
        let mut acc = [0.0f32; JOINTS];
        match region {
            SkinRegion::Head => acc[Joint::Head.index()] = 1.0,
            SkinRegion::Pelvis => acc[Joint::Pelvis.index()] = 1.0,
            SkinRegion::Chest => acc[Joint::Chest.index()] = 1.0,
            SkinRegion::Trunk => self.trunk_weights(p[1], 1.0, &mut acc),
            _ => self.limbed(p, region, &mut acc),
        }
        top_four(&acc)
    }

    /// The trunk's weights at height `y`.
    fn trunk_weights(&self, y: f32, k: f32, out: &mut [f32; JOINTS]) {
        let t = self.trunk_length;
        let a = smoothstep(self.hip_y, self.waist_y + 0.15 * t, y);
        let b = smoothstep(self.waist_y, self.chest_y + 0.1 * t, y);
        let head = smoothstep(self.neck_from, self.neck_to, y);
        let body = 1.0 - head;
        out[Joint::Pelvis.index()] += k * body * (1.0 - a);
        out[Joint::Spine.index()] += k * body * a * (1.0 - b);
        out[Joint::Chest.index()] += k * body * a * b;
        out[Joint::Head.index()] += k * head;
    }

    /// How far outside the trunk's skin `p` is, as a ratio (1 on the skin).
    fn outside_trunk(&self, p: V3) -> f32 {
        let y = p[1].clamp(self.hip_y, self.neck_from);
        let hw = self.trunk.section(0.0, y, 0.0)[0].abs().max(1e-3);
        let front = self.trunk.section(FRONT, y, 0.0)[1].abs().max(1e-3);
        let back = self.trunk.section(BACK, y, 0.0)[1].abs().max(1e-3);
        let hd = if p[2] >= 0.0 { front } else { back };
        let e = 2.0 / TRUNK_EXPONENT;
        ((p[0].abs() / hw).powf(e) + (p[2].abs() / hd).powf(e)).sqrt()
    }

    fn limbed(&self, p: V3, region: SkinRegion, acc: &mut [f32; JOINTS]) {
        let located: [(f32, f32); 4] = std::array::from_fn(|i| self.limbs[i].locate(p));
        let hug: [f32; 4] = std::array::from_fn(|i| self.limbs[i].hug(located[i].0, located[i].1));
        // How much each limb pair claims the vertex (0 to 1), and which limb of the pair.
        let pair = |a: usize, b: usize| -> (f32, f32) {
            let (na, nb) = (hug[a], hug[b]);
            let near = na.min(nb);
            let claim = 1.0 - smoothstep(1.6, 3.4, near);
            // Gear clinging to one limb takes that limb alone (its distance to the other is
            // much the greater); across the middle of a pair it shares them evenly, so nothing
            // is torn between two limbs that move apart.
            let (da, db) = (located[a].1.max(1e-4), located[b].1.max(1e-4));
            let share_a = 1.0 / (1.0 + (da / db).powi(12));
            (claim, share_a)
        };
        // The legs part ways: cloth across the midline between them (the seam of baggy
        // trousers) shares them evenly there, and gives way to whichever it clings to a little
        // off it, so no seam is pulled apart by two legs going opposite ways.
        let legs = |p: V3| -> (f32, f32) {
            let (claim, share_right) = pair(2, 3);
            let m = 0.06 * self.hip_half_width;
            let by_x = 0.5 - 0.5 * (p[0] / m).clamp(-1.0, 1.0);
            let mid = 1.0 - smoothstep(0.4 * m, m, p[0].abs());
            (claim, share_right + (by_x - share_right) * mid)
        };
        let nearest_end = |a: usize, b: usize| -> f32 {
            if vec3::distance(p, self.limbs[a].end) <= vec3::distance(p, self.limbs[b].end) { 1.0 } else { 0.0 }
        };
        let (mut arm_claim, mut arm_a) = (0.0, 0.5);
        let (mut leg_claim, mut leg_a) = (0.0, 0.5);
        match region {
            SkinRegion::Auto => {
                let (c, s) = pair(0, 1);
                let inside = 1.0 - smoothstep(1.03, 1.18, self.outside_trunk(p));
                (arm_claim, arm_a) = (c * (1.0 - inside), s);
                (leg_claim, leg_a) = legs(p);
            }
            SkinRegion::Legs => (leg_claim, leg_a) = legs(p),
            SkinRegion::Arm => {
                // A tube round one arm: the nearest by distance, not softly.
                arm_claim = 1.0;
                arm_a = if hug[0] <= hug[1] { 1.0 } else { 0.0 };
            }
            SkinRegion::Hand => {
                arm_claim = 1.0;
                arm_a = nearest_end(0, 1);
            }
            SkinRegion::Leg => {
                leg_claim = 1.0;
                leg_a = if hug[2] <= hug[3] { 1.0 } else { 0.0 };
            }
            SkinRegion::Foot => {
                leg_claim = 1.0;
                leg_a = nearest_end(2, 3);
            }
            SkinRegion::Trunk | SkinRegion::Loose | SkinRegion::Head | SkinRegion::Pelvis | SkinRegion::Chest => {}
        }
        let total = arm_claim + leg_claim;
        if total > 1.0 {
            arm_claim /= total;
            leg_claim /= total;
        }
        let rest = 1.0 - arm_claim - leg_claim;
        if rest > 1e-5 {
            // Loose cloth and anything hanging off the legs: a share of the thighs grows toward
            // the hem, split across the midline.
            let hangs = matches!(region, SkinRegion::Auto | SkinRegion::Legs | SkinRegion::Loose);
            let share = if hangs { HEM_SHARE * smoothstep(self.crotch_y, self.ankle_y, p[1]) } else { 0.0 };
            self.trunk_weights(p[1], rest * (1.0 - share), acc);
            if share > 1e-5 {
                let xm = 0.5 * self.hip_half_width;
                let left = smoothstep(-xm, xm, p[0]);
                self.limbs[2].weights(located[2].0, acc, rest * share * (1.0 - left));
                self.limbs[3].weights(located[3].0, acc, rest * share * left);
            }
        }
        if arm_claim > 1e-5 {
            self.limbs[0].weights(located[0].0, acc, arm_claim * arm_a);
            self.limbs[1].weights(located[1].0, acc, arm_claim * (1.0 - arm_a));
        }
        if leg_claim > 1e-5 {
            self.limbs[2].weights(located[2].0, acc, leg_claim * leg_a);
            self.limbs[3].weights(located[3].0, acc, leg_claim * (1.0 - leg_a));
        }
    }
}

/// The four heaviest joints, renormalised to sum to 1. A joint left out gives its weight to
/// its nearest ancestor that is kept (a knee's to its hip), or else to the heaviest kept, so
/// dropping a small weight moves a vertex as little as it can.
fn top_four(acc: &[f32; JOINTS]) -> ([u16; 4], [f32; 4]) {
    let mut order: [usize; JOINTS] = std::array::from_fn(|i| i);
    order.sort_by(|&a, &b| acc[b].total_cmp(&acc[a]).then(a.cmp(&b)));
    let kept = &order[..4];
    let mut weights = [0.0f32; 4];
    for (i, &j) in order.iter().enumerate() {
        let w = acc[j].max(0.0);
        if w <= 0.0 {
            continue;
        }
        if i < 4 {
            weights[i] += w;
            continue;
        }
        let mut at = Joint::ALL[j].parent();
        let slot = loop {
            match at {
                Some(p) => match kept.iter().position(|&k| k == p.index()) {
                    Some(slot) => break slot,
                    None => at = p.parent(),
                },
                None => break 0,
            }
        };
        weights[slot] += w;
    }
    let joints = std::array::from_fn(|i| kept[i] as u16);
    let sum: f32 = weights.iter().sum();
    if sum > 1e-9 {
        weights = weights.map(|w| w / sum);
    } else {
        weights = [1.0, 0.0, 0.0, 0.0];
    }
    (joints, weights)
}
