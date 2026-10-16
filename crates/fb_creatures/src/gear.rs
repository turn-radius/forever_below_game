//! Gear on a skeleton: [`SkeletonGear::fit`] fits what a skeleton wears and holds to the bones,
//! through `fb_body`'s own fitting, and splits it into pieces that each ride one bone.
//!
//! A skeleton's rig is laid on an `fb_body` human ([`crate::skeleton::SkeletonDims::body`]), so
//! worn gear is fitted ([`fb_body::fit_worn`]) to that body exactly as it is to folk: a helm
//! sized to the head, body armour round the trunk, gloves and boots on its hands and feet. The
//! body is leaner than a man's (the flesh is gone) but the gear keeps the shape of the man who
//! wore it, so it hangs off the ribs and limbs rather than lying on them; nothing of it comes
//! inside a bone, because the bones were built inside that body. Held gear is fitted
//! ([`fb_body::fit_hands`]) to the same body with its arms posed for what they hold.
//!
//! The rig is rigid (no skinning), so each fitted mesh is cut into pieces by triangle, each
//! piece riding the bone it lies nearest, much as plate armour is jointed: headgear on the
//! skull; a torso garment on the ribcage with sleeves cut off along the arms; gloves on the
//! hands; a boot's foot on the foot and its shaft on the shin; what is held on the hand. Each
//! piece is stored in its bone's own space, as a bone's mesh is, so a pose moves it with the
//! bone.
//!
//! An arm that holds something is posed by `fb_body` (the forearm raised to a level grip), where
//! the rig's arms hang at rest. The [`ArmStance`] of such an arm is the turns of its three bones
//! that bring the bones to the posed arm, and the gear is stored for the bones in that stance, so
//! posing the skeleton with its stance ([`crate::pose::skeleton_armed`]) sets the gear on the
//! bones as `fb_body` fitted it. The bony hand of an armed arm is closed into the grip the same
//! fitting assumes ([`SkeletonState::build_gripping`]), so what is held sits in the fingers.

use armory::geom::vec3::{self, Mat3, V3};
use armory::{ItemMesh, Part};
use fb_body::{Body, HandPose, Placement, Side, fit_hands, fit_worn};
use fb_items::outfit::OutfitSet;
use fb_items::undead::skeleton_outfit;
use fb_items::Slot;

use crate::pose::ArmStance;
use crate::rig::Rig;
use crate::skeleton::{SkeletonState, arm_bone, index, leg_bone};

/// One piece of gear on one bone.
#[derive(Clone, Debug, PartialEq)]
pub struct GearPiece {
    /// The slot the item is worn or held in.
    pub slot: Slot,
    /// The bone it rides ([`crate::skeleton::index`]).
    pub bone: usize,
    /// The item's name.
    pub name: String,
    /// The item as drawn (a held item's handle may be thinned to the fist), for its look.
    pub item: armory::Item,
    /// The mesh in the bone's space.
    pub mesh: ItemMesh,
}

/// A skeleton's gear fitted: its rig with the hands closed on what they hold and bare hands and
/// feet left out under gloves and boots, the pieces, and the stance of the arms that hold things.
#[derive(Clone, Debug)]
pub struct SkeletonGear {
    pub outfit: OutfitSet,
    pub rig: Rig,
    pub pieces: Vec<GearPiece>,
    /// Right arm then left; `Some` for a hand that holds something.
    pub stance: [Option<ArmStance>; 2],
    /// The grip each holding hand is closed in.
    pub grips: [Option<HandPose>; 2],
    /// Where a held item's grip is in the hand's own space, per hand (for the weapon's reach).
    pub grip_points: [Option<V3>; 2],
}

impl SkeletonGear {
    /// The gear a skeleton rolls from its own seed ([`skeleton_outfit`]), fitted.
    pub fn roll(state: &SkeletonState) -> SkeletonGear {
        SkeletonGear::fit(state, skeleton_outfit(state.seed))
    }

    /// `outfit` fitted to the skeleton `state`.
    pub fn fit(state: &SkeletonState, outfit: OutfitSet) -> SkeletonGear {
        let dims = state.dims();
        let rest = &dims.body;
        let spec = state.spec();
        let held = |slot| outfit.item(slot).map(|i| &i.shape);
        let hands = fit_hands(&spec, held(Slot::Mainhand), held(Slot::Offhand));
        let posed = &hands.body;
        let fits = [hands.main.as_ref(), hands.off.as_ref()];
        let worn: Vec<(Slot, armory::Item)> = outfit.worn.iter().filter(|(s, _)| !matches!(s, Slot::Mainhand | Slot::Offhand)).map(|(s, i)| (*s, i.shape.clone())).collect();
        let mut fit = fit_worn(posed, &worn);

        // Each bone's frame in the stance: where its origin is and how it is turned, in body space.
        let mut frames: Vec<(V3, Mat3)> = dims.origins.iter().map(|&o| (o, Mat3::IDENTITY)).collect();
        let mut stance = [None, None];
        let mut grips = [None, None];
        let mut grip_points = [None, None];
        for (k, side) in Side::BOTH.into_iter().enumerate() {
            if fits[k].is_none() {
                continue;
            }
            let (arm, frame) = arm_stance(rest, posed, side);
            let b = arm_bone(side);
            for j in 0..3 {
                frames[b + j] = frame[j];
            }
            stance[k] = Some(arm);
            grips[k] = Some(posed.hand_pose(side));
            let hand = frame[2];
            grip_points[k] = Some(to_local(&hand, posed.grip(side).origin));
        }

        let mut pieces = Vec::new();
        for layer in &mut fit.layers {
            let Some((_, item)) = outfit.worn.iter().find(|(s, _)| *s == layer.slot) else { continue };
            let mut by_bone: Vec<Vec<Part>> = (0..index::COUNT).map(|_| Vec::new()).collect();
            let seams = Seams::new(posed, &dims.legs, layer.slot);
            shape_for_bones(layer.slot, &mut layer.mesh, posed, &seams);
            for part in &layer.mesh.parts {
                for (bone, cut) in cut_part(part, |p| seams.bones_for(p), &frames) {
                    by_bone[bone].push(cut);
                }
            }
            for (bone, parts) in by_bone.into_iter().enumerate() {
                if !parts.is_empty() {
                    pieces.push(GearPiece { slot: layer.slot, bone, name: item.title().to_string(), item: item.shape.clone(), mesh: ItemMesh { parts } });
                }
            }
        }
        for (k, side) in Side::BOTH.into_iter().enumerate() {
            let Some(fit) = fits[k] else { continue };
            let slot = if k == 0 { Slot::Mainhand } else { Slot::Offhand };
            let Some((_, item)) = outfit.worn.iter().find(|(s, _)| *s == slot) else { continue };
            let bone = arm_bone(side) + 2;
            let mut mesh = ItemMesh::default();
            for part in &fit.mesh.parts {
                for (_, cut) in cut_part(part, |_| (bone, None), &frames) {
                    mesh.parts.push(cut);
                }
            }
            pieces.push(GearPiece { slot, bone, name: item.title().to_string(), item: fit.item.clone(), mesh });
        }

        let mut rig = state.build_gripping(&grips);
        // Gloves and boots hold the bones inside them: the bare hand and foot are not drawn.
        if fit.covered.hands {
            for side in Side::BOTH {
                rig.bones[arm_bone(side) + 2].mesh.parts.clear();
            }
        }
        if fit.covered.feet {
            for side in Side::BOTH {
                rig.bones[leg_bone(side) + 2].mesh.parts.clear();
            }
        }
        SkeletonGear { outfit, rig, pieces, stance, grips, grip_points }
    }

    /// Whether anything is worn or held.
    pub fn is_bare(&self) -> bool {
        self.pieces.is_empty()
    }

    /// Each piece in skeleton space, given every bone's placement on its parent (a pose from
    /// [`crate::pose::skeleton_armed`]): the mesh of piece `i` is for `pieces[i]`.
    pub fn posed(&self, local: &[Placement]) -> Vec<ItemMesh> {
        let world = self.rig.world(local);
        self.pieces
            .iter()
            .map(|piece| {
                let mut mesh = piece.mesh.clone();
                let w = world[piece.bone];
                for part in &mut mesh.parts {
                    part.transform(&w.rotation, w.origin);
                }
                mesh
            })
            .collect()
    }
}

/// The transpose of a rotation or reflection: its inverse.
fn transpose(m: &Mat3) -> Mat3 {
    let [x, y, z] = m.cols;
    Mat3::from_cols([x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]])
}

/// A point in body space as a bone in `frame` sees it.
fn to_local(frame: &(V3, Mat3), p: V3) -> V3 {
    transpose(&frame.1).transform(vec3::sub(p, frame.0))
}

/// The stance of `side`'s arm: the turns that bring the rest arm's bones to the arm `posed`
/// holds, and the frame each bone is then in.
fn arm_stance(rest: &Body, posed: &Body, side: Side) -> (ArmStance, [(V3, Mat3); 3]) {
    let [s0, e0, w0] = rest.arm(side).joints;
    let [s1, e1, w1] = posed.arm(side).joints;
    let dir = |a: V3, b: V3| vec3::normalize_or(vec3::sub(b, a), vec3::neg(vec3::Y));
    let upper = Mat3::from_rotation_arc(dir(s0, e0), dir(s1, e1));
    // The forearm's turn in the world, then on the upper arm.
    let fore_world = Mat3::from_rotation_arc(upper.transform(dir(e0, w0)), dir(e1, w1)).mul_mat(&upper);
    let fore = transpose(&upper).mul_mat(&fore_world);
    // The hand's frame turned from its rest frame to the posed one (both mirror for a left hand).
    let hand_world = posed.hand_placement(side).rotation.mul_mat(&transpose(&rest.hand_placement(side).rotation));
    let hand = transpose(&fore_world).mul_mat(&hand_world);
    (ArmStance { upper, fore, hand }, [(s1, upper), (e1, fore_world), (w1, hand_world)])
}

/// How far a piece reaches over the seam onto its neighbour's bone, so that a bent joint is
/// lapped like the lames of a plate joint instead of cracking open.
const LAP: f32 = 0.45;
/// A boot's foot is this far above the ankle, where the shin's turn about it is small.
const FOOT_TOP: f32 = 0.1;
/// A boot's toe box is this far ahead of the ankle.
const TOE_AT: f32 = 0.65;
/// The same at the ankle, where a boot is wide and the foot turns far.
const LAP_ANKLE: f32 = 0.9;
/// How far the lapped copy of a piece is set under the surface it hides beneath.
const LAP_SINK: f32 = 0.03;
/// A triangle of less than this twice-area (square world units) is a collapsed sliver and is dropped.
const SLIVER: f32 = 2e-5;
/// A bone's radius as the fit sees it, in the arm's shafts.
const ARM_BONE: f32 = 0.09;
/// The widest a gauntlet's cuff opens, as a multiple of the bone it wraps.
const CUFF_OPENING: f32 = 1.7;
/// The widest a sleeve's end opens round the forearm.
const SLEEVE_OPENING: f32 = 0.26;
/// The widest a helm's hem opens, as a multiple of the skull's half width.
const HEM_OPENING: f32 = 1.6;
/// How far sleeveless body armour draws in at the waist.
const WAIST: f32 = 0.07;
/// The hem is drawn fully in over this fraction of the skull's half height.
const HEM_DRAWN: f32 = 0.4;
/// A helm hangs below this fraction of the skull's half height under its centre before its hem
/// is drawn in (a brim at the temples is left alone).
const HEM_BELOW: f32 = 0.6;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Draws the vertices `pick` chooses, as they lie along the axis from `a` towards `b`, in from
/// the radius they start at to `rim` at their far end, so an opening closes in to the thin bone
/// it wraps instead of flaring like a funnel.
fn close_in(mesh: &mut ItemMesh, a: V3, b: V3, rim: f32, pick: impl Fn(V3) -> bool) {
    let u = vec3::normalize_or(vec3::sub(b, a), vec3::neg(vec3::Y));
    let along = |p: V3| vec3::dot(vec3::sub(p, a), u);
    let radial = |p: V3| vec3::sub(vec3::sub(p, a), vec3::scale(u, along(p)));
    let picked: Vec<V3> = mesh.parts.iter().flat_map(|part| part.positions.iter().copied()).filter(|&p| pick(p) && along(p) > 0.0).collect();
    let Some(far) = picked.iter().map(|&p| along(p)).max_by(f32::total_cmp) else { return };
    let start = picked.iter().filter(|&&p| along(p) <= 0.1 * far).map(|&p| vec3::length(radial(p))).fold(rim, f32::max);
    for part in &mut mesh.parts {
        for p in &mut part.positions {
            let s = along(*p);
            if s <= 0.0 || !pick(*p) {
                continue;
            }
            let r = radial(*p);
            let width = vec3::length(r);
            let cap = start + (rim - start) * smooth(s / far);
            if width > cap {
                *p = vec3::add(vec3::add(a, vec3::scale(u, s)), vec3::scale(r, cap / width));
            }
        }
    }
}

/// Fits a layer's mesh to the bones before it is cut: cuffs and sleeve ends close in on the
/// forearm and a helm's hem on the neck.
fn shape_for_bones(slot: Slot, mesh: &mut ItemMesh, posed: &Body, seams: &Seams) {
    let sides = [Side::Right, Side::Left];
    match slot {
        Slot::Hands => {
            for side in sides {
                let [_, elbow, wrist] = posed.arm(side).joints;
                close_in(mesh, wrist, elbow, CUFF_OPENING * ARM_BONE, |p| seams.hand_of(p) == side);
            }
        }
        Slot::Head => {
            let head = posed.head;
            let axis = [head.centre[0], 0.0, head.centre[2]];
            let below = head.centre[1] - HEM_BELOW * head.half_height;
            let cap = HEM_OPENING * head.half_width;
            for part in &mut mesh.parts {
                for p in &mut part.positions {
                    let down = (below - p[1]) / (HEM_DRAWN * head.half_height);
                    if down <= 0.0 {
                        continue;
                    }
                    let r = [p[0] - axis[0], 0.0, p[2] - axis[2]];
                    let width = vec3::length(r);
                    let allowed = 2.2 * head.half_width + (cap - 2.2 * head.half_width) * smooth(down);
                    if width > allowed {
                        let k = allowed / width;
                        p[0] = axis[0] + r[0] * k;
                        p[2] = axis[2] + r[2] * k;
                    }
                }
            }
        }
        Slot::Shirt | Slot::Body | Slot::Cloak => {
            for side in sides {
                let [_, elbow, wrist] = posed.arm(side).joints;
                close_in(mesh, elbow, wrist, SLEEVE_OPENING, |p| seams.bones_for(p).0 == arm_bone(side) + 1);
            }
            // Armour with no sleeves is shaped to the ribs rather than left a plain tube: it
            // swells over the chest, draws in at the waist and flares at the hem.
            let on_forearm = |p: V3| sides.iter().any(|&side| seams.bones_for(p).0 == arm_bone(side) + 1);
            let trunk: Vec<V3> = mesh.parts.iter().flat_map(|part| part.positions.iter().copied()).collect();
            if slot == Slot::Body && !trunk.is_empty() && trunk.iter().filter(|&&p| on_forearm(p)).count() * 100 < trunk.len() {
                let (hip, shoulder) = (posed.hip_y, posed.shoulder_y);
                let centre = trunk.iter().map(|p| p[2]).sum::<f32>() / trunk.len() as f32;
                let profile = |y: f32| {
                    let t = (y - hip) / (shoulder - hip).max(1e-3);
                    let k = 1.0 - WAIST * (-((t - 0.28) / 0.35).powi(2)).exp() + 0.25 * WAIST * (-((t + 0.1) / 0.15).powi(2)).exp();
                    k.max(0.92)
                };
                for part in &mut mesh.parts {
                    for p in &mut part.positions {
                        let k = profile(p[1]);
                        p[0] *= k;
                        p[2] = centre + (p[2] - centre) * k;
                    }
                }
            }
        }
        _ => {}
    }
}

/// Where a layer's triangles go: by slot, headgear to the skull and gloves to the nearest hand
/// (the cuff to the forearm); otherwise to whichever bone's segment each triangle lies nearest.
struct Seams {
    slot: Slot,
    /// Shoulder, elbow, wrist of each arm (right then left) and the arm's radius.
    arms: [([V3; 3], f32); 2],
    /// The trunk's axis from the waist to the base of the neck, and its half width.
    trunk: (V3, V3, f32),
    legs: [[V3; 3]; 2],
    /// A boot's foot is below this height.
    foot_top: f32,
}

impl Seams {
    fn new(posed: &Body, legs: &[[V3; 3]; 2], slot: Slot) -> Seams {
        let arm = |s: Side| (posed.arm(s).joints, posed.arm(s).radius);
        Seams {
            slot,
            arms: [arm(Side::Right), arm(Side::Left)],
            trunk: ([0.0, posed.hip_y, 0.0], [0.0, posed.shoulder_y, 0.0], posed.chest.half_width),
            legs: *legs,
            foot_top: legs[0][2][1] + FOOT_TOP,
        }
    }

    /// The hand a glove's vertex at `p` belongs to: the nearest wrist's.
    fn hand_of(&self, p: V3) -> Side {
        if vec3::distance(p, self.arms[0].0[2]) <= vec3::distance(p, self.arms[1].0[2]) { Side::Right } else { Side::Left }
    }

    /// The bone `p` rides, and the neighbouring bone it laps onto if it lies within [`LAP`] of
    /// the seam.
    fn bones_for(&self, p: V3) -> (usize, Option<usize>) {
        let side_of_x = |p: V3| if p[0] < 0.0 { 0 } else { 1 };
        let sides = [Side::Right, Side::Left];
        // Two bones either side of a seam `d` from it (negative on the first bone's side).
        let across = |first: usize, second: usize, d: f32, lap: f32| {
            let (own, other) = if d < 0.0 { (first, second) } else { (second, first) };
            (own, (d.abs() < lap).then_some(other))
        };
        match self.slot {
            Slot::Head => (index::SKULL, None),
            Slot::Hands => {
                let side = self.hand_of(p);
                let k = if side == Side::Right { 0 } else { 1 };
                let [_, elbow, wrist] = self.arms[k].0;
                // Behind the wrist is the cuff, on the forearm.
                let behind = vec3::dot(vec3::sub(p, wrist), vec3::normalize_or(vec3::sub(elbow, wrist), vec3::neg(vec3::Y)));
                across(arm_bone(side) + 2, arm_bone(side) + 1, behind, LAP)
            }
            Slot::Feet => {
                let k = side_of_x(p);
                // The toe box is the foot's, however high it stands.
                let ahead = p[2] - self.legs[k][2][2];
                across(leg_bone(sides[k]) + 2, leg_bone(sides[k]) + 1, (p[1] - self.foot_top).min(TOE_AT - ahead), LAP_ANKLE)
            }
            Slot::Legs => {
                let k = side_of_x(p);
                let knee = self.legs[k][1][1];
                across(leg_bone(sides[k]) + 1, leg_bone(sides[k]), p[1] - knee, LAP)
            }
            _ => {
                // The trunk's reach, against each arm's: sleeves go to the arm they lie along,
                // but the ball of the shoulder stays with the trunk.
                let (a, b, half) = self.trunk;
                let mut scored = vec![(distance_to_segment(p, a, b).0 - 0.9 * half, index::RIBCAGE)];
                for (k, (joints, radius)) in self.arms.iter().enumerate() {
                    let (d, t) = distance_to_segment(p, joints[0], joints[1]);
                    let reach = vec3::distance(joints[0], joints[1]);
                    if t * reach > 1.4 * radius {
                        scored.push((d - 1.3 * radius, arm_bone(sides[k])));
                    }
                    let (d, _) = distance_to_segment(p, joints[1], joints[2]);
                    scored.push((d - 1.3 * radius, arm_bone(sides[k]) + 1));
                }
                scored.sort_by(|x, y| x.0.total_cmp(&y.0));
                // A sleeve laid over the trunk or another bone of the arm is not lapped: its lapped
                // copy swings out of the cloth with the other bone and shows as a fringe of
                // spikes at the cuff, and the cloth is thick enough to hide a hairline seam.
                (scored[0].1, None)
            }
        }
    }
}

/// How far `p` is from the segment `a` to `b`, and how far along it the nearest point is (0 to 1).
fn distance_to_segment(p: V3, a: V3, b: V3) -> (f32, f32) {
    let ab = vec3::sub(b, a);
    let t = (vec3::dot(vec3::sub(p, a), ab) / vec3::dot(ab, ab).max(1e-9)).clamp(0.0, 1.0);
    (vec3::distance(p, vec3::add(a, vec3::scale(ab, t))), t)
}

/// `part` cut by triangle into one part for each bone `bones_for` sends the triangle's centre
/// to, each in that bone's own space (`frames` gives every bone's place in body space). A
/// triangle near a seam is also given to the neighbouring bone, sunk a little under the surface,
/// so the pieces overlap there.
fn cut_part(part: &Part, bones_for: impl Fn(V3) -> (usize, Option<usize>), frames: &[(V3, Mat3)]) -> Vec<(usize, Part)> {
    // Per bone: the triangles it owns, then the triangles it laps.
    let mut groups: Vec<(usize, Vec<[u32; 3]>, Vec<[u32; 3]>)> = Vec::new();
    let mut add = |bone: usize, tri: [u32; 3], lap: bool| {
        let at = match groups.iter().position(|g| g.0 == bone) {
            Some(at) => at,
            None => {
                groups.push((bone, Vec::new(), Vec::new()));
                groups.len() - 1
            }
        };
        if lap { groups[at].2.push(tri) } else { groups[at].1.push(tri) }
    };
    for tri in part.indices.chunks_exact(3) {
        let tri = [tri[0], tri[1], tri[2]];
        let corners = tri.map(|i| part.positions[i as usize]);
        // A sliver that closing in collapsed has no surface to show, only an outline spike.
        let area = vec3::length(vec3::cross(vec3::sub(corners[1], corners[0]), vec3::sub(corners[2], corners[0])));
        if area < SLIVER {
            continue;
        }
        let (bone, lap) = bones_for(vec3::centroid(&corners));
        add(bone, tri, false);
        if let Some(other) = lap {
            add(other, tri, true);
        }
    }
    groups.sort_by_key(|(b, ..)| *b);
    groups
        .into_iter()
        .map(|(bone, tris, laps)| {
            let back = transpose(&frames[bone].1);
            // Vertices are kept apart for owned and lapped triangles: a lapped one is sunk.
            let mut remap = [vec![u32::MAX; part.positions.len()], vec![u32::MAX; part.positions.len()]];
            let mut cut = Part { name: part.name, material: part.material, tint: part.tint, pattern: part.pattern, glow: part.glow, outline: part.outline, ..Part::default() };
            for (tri, sunk) in tris.into_iter().map(|t| (t, false)).chain(laps.into_iter().map(|t| (t, true))) {
                for i in tri {
                    let at = i as usize;
                    let map = &mut remap[sunk as usize];
                    if map[at] == u32::MAX {
                        map[at] = cut.positions.len() as u32;
                        let sink = if sunk { LAP_SINK } else { 0.0 };
                        let place = vec3::sub(part.positions[at], vec3::scale(part.normals[at], sink));
                        cut.positions.push(back.transform(vec3::sub(place, frames[bone].0)));
                        cut.normals.push(vec3::normalize_or(back.transform(part.normals[at]), part.normals[at]));
                        cut.uvs.push(part.uvs[at]);
                        cut.outline_normals.push(vec3::normalize_or(back.transform(part.outline_normals[at]), part.outline_normals[at]));
                    }
                    cut.indices.push(map[at]);
                }
            }
            (bone, cut)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pose::{SkeletonPose, skeleton_armed};
    use crate::rig::Rig;

    /// Every point of a mesh: its vertices and the middle of each triangle edge.
    fn points(mesh: &ItemMesh) -> Vec<V3> {
        let mut out = Vec::new();
        for part in &mesh.parts {
            out.extend(part.positions.iter().copied());
            for tri in part.indices.chunks_exact(3) {
                for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                    out.push(vec3::lerp(part.positions[tri[a] as usize], part.positions[tri[b] as usize], 0.5));
                }
            }
        }
        out
    }

    fn nearest(p: V3, cloud: &[V3]) -> f32 {
        cloud.iter().map(|&q| vec3::distance(p, q)).fold(f32::MAX, f32::min)
    }

    /// The gear and bones standing in the stance.
    fn standing(seed: u32) -> (SkeletonGear, Vec<ItemMesh>, Vec<V3>) {
        let state = SkeletonState::roll(seed);
        let gear = SkeletonGear::roll(&state);
        let local = skeleton_armed(&state.dims(), &SkeletonPose::default(), &gear.stance);
        let worn = gear.posed(&local);
        let bones = points(&gear.rig.posed_mesh(&local));
        (gear, worn, bones)
    }

    fn has(gear: &SkeletonGear, slots: &[Slot]) -> bool {
        slots.iter().all(|s| gear.outfit.item(*s).is_some())
    }

    const FULL: [Slot; 5] = [Slot::Head, Slot::Body, Slot::Hands, Slot::Mainhand, Slot::Offhand];

    #[test]
    fn the_same_seed_gives_the_same_gear() {
        for seed in [1, 2, 3, 4, 5, 6, 7, 8] {
            let state = SkeletonState::roll(seed);
            let (a, b) = (SkeletonGear::roll(&state), SkeletonGear::roll(&state));
            assert_eq!(a.pieces, b.pieces, "{seed}");
            assert_eq!(a.rig, b.rig, "{seed}");
            assert_eq!(a.stance, b.stance, "{seed}");
        }
    }

    #[test]
    fn a_skeleton_can_wear_a_helm_armour_and_gloves_and_hold_a_sword_and_shield() {
        let kitted = |s: u32| {
            let gear = SkeletonGear::roll(&SkeletonState::roll(s));
            (has(&gear, &FULL) && gear.outfit.item(Slot::Mainhand).is_some_and(|w| w.base == "Sword")).then_some(gear)
        };
        let gear = (0..200).find_map(kitted).expect("a kitted swordsman");
        for slot in FULL {
            let on: Vec<usize> = gear.pieces.iter().filter(|p| p.slot == slot).map(|p| p.bone).collect();
            assert!(!on.is_empty(), "{slot:?} has no piece");
            match slot {
                Slot::Head => assert_eq!(on, vec![index::SKULL]),
                // Gloves ride the hands, their cuffs the forearms.
                Slot::Hands => assert!(on.iter().all(|b| [arm_bone(Side::Right) + 1, arm_bone(Side::Right) + 2, arm_bone(Side::Left) + 1, arm_bone(Side::Left) + 2].contains(b)), "{on:?}"),
                Slot::Mainhand => assert_eq!(on, vec![arm_bone(Side::Right) + 2]),
                Slot::Offhand => assert_eq!(on, vec![arm_bone(Side::Left) + 2]),
                _ => {}
            }
        }
        assert!(gear.stance.iter().all(Option::is_some) && gear.grips.iter().all(Option::is_some));
        // Gloves hold the hands, so the bare bony hands are not drawn.
        assert!(gear.rig.bones[arm_bone(Side::Right) + 2].mesh.parts.is_empty());
    }

    #[test]
    fn some_wear_nothing_and_some_hold_only_a_spear() {
        let gears: Vec<SkeletonGear> = (0..40).map(|s| SkeletonGear::roll(&SkeletonState::roll(s))).collect();
        let bare = gears.iter().position(SkeletonGear::is_bare).expect("a bare skeleton");
        assert_eq!(gears[bare].rig, SkeletonState::roll(bare as u32).build());
        assert!(gears.iter().any(|g| g.outfit.worn.len() == 1 && g.outfit.item(Slot::Mainhand).is_some_and(|w| w.base == "Spear")));
    }

    #[test]
    fn the_stance_puts_the_hand_where_the_fit_held_it() {
        for seed in 0..30 {
            let state = SkeletonState::roll(seed);
            let gear = SkeletonGear::roll(&state);
            let dims = state.dims();
            let local = skeleton_armed(&dims, &SkeletonPose::default(), &gear.stance);
            let world = gear.rig.world(&local);
            let held = |slot| gear.outfit.item(slot).map(|i| &i.shape);
            let posed = fit_hands(&state.spec(), held(Slot::Mainhand), held(Slot::Offhand)).body;
            for (k, side) in Side::BOTH.into_iter().enumerate() {
                if gear.stance[k].is_none() {
                    continue;
                }
                let want = posed.arm(side).joints;
                for j in 0..3 {
                    let at = world[arm_bone(side) + j].origin;
                    assert!(vec3::distance(at, want[j]) < 0.15, "{seed} {side:?} joint {j}: {at:?} vs {:?}", want[j]);
                }
            }
        }
    }

    /// The rig with its hands closed, bare (a gear's rig leaves the hand out under a glove).
    struct BareHands(Rig);

    impl BareHands {
        fn rig_hand(&self, side: Side) -> ItemMesh {
            self.0.bones[arm_bone(side) + 2].mesh.clone()
        }
    }

    /// The closed hand lies round the grip and the held item passes through it.
    fn check_grips(seed: u32) {
        let (gear, worn, _) = standing(seed);
        let bones = BareHands(SkeletonState::roll(seed).build_gripping(&gear.grips));
        for (k, side) in Side::BOTH.into_iter().enumerate() {
            let Some(grip) = gear.grip_points[k] else { continue };
            let hand = arm_bone(side) + 2;
            // The fingers close round the grip: some of the hand is near its centre, none inside it.
            if !gear.rig.bones[hand].mesh.parts.is_empty() {
                let d = nearest(grip, &points(&gear.rig.bones[hand].mesh));
                assert!((0.03..0.45).contains(&d), "{seed} {side:?}: the fingers are {d} from the grip");
            }
            // The grip is in the hand: inside its bones' bounds, a little eased.
            let (lo, hi) = bones.rig_hand(side).bounds().expect("a hand");
            assert!((0..3).all(|i| grip[i] > lo[i] - 0.25 && grip[i] < hi[i] + 0.25), "{seed} {side:?}: the grip {grip:?} is outside the hand {lo:?} {hi:?}");
            // The held thing passes through the fist.
            let piece = gear.pieces.iter().position(|p| p.bone == hand && matches!(p.slot, Slot::Mainhand | Slot::Offhand)).expect("a held piece");
            let d = nearest(grip, &points(&gear.pieces[piece].mesh));
            // A shield's board is held clear of the fist, on its strap.
            let reach = if matches!(gear.pieces[piece].item, armory::Item::Shield(_)) { 1.8 } else { 0.3 };
            assert!(d < reach, "{seed} {side:?}: {} is {d} from the grip", gear.pieces[piece].name);
            assert!(!worn[piece].parts.is_empty());
        }
    }

    #[test]
    fn a_held_weapon_sits_in_the_closed_fingers_smoke() {
        for seed in 0..16 {
            check_grips(seed);
        }
    }

    #[test]
    #[ignore = "slow: 200 skeletons"]
    fn a_held_weapon_sits_in_the_closed_fingers() {
        for seed in 0..200 {
            check_grips(seed);
        }
    }

    /// No vertex of worn gear is further than this from the bones.
    const MAX_REACH: f32 = 3.2;
    /// At most this share of a piece's vertices lies inside a bone's volume.
    const MAX_INSIDE: f32 = 0.03;
    /// The same for a glove's or boot's share inside the hand or foot it covers.
    const MAX_INSIDE_END: f32 = 0.06;
    /// The share of the ribcage's and pelvis's bounds that is their volume.
    const VOLUME: f32 = 0.8;
    /// The same for a hand or a foot, which gloves and boots lie close on.
    const EXTREMITY: f32 = 0.45;

    /// Gear stays out of the long bones, the spine and the skull, and not far from the bones;
    /// `far` gathers the greatest distance of any vertex from a bone.
    fn check_fit(seed: u32, far: &mut f32, worst: &mut f32, worst_end: &mut f32) {
        let state = SkeletonState::roll(seed);
        let gear = SkeletonGear::roll(&state);
        let dims = state.dims();
        let local = skeleton_armed(&dims, &SkeletonPose::default(), &gear.stance);
        let world = gear.rig.world(&local);
        let worn = gear.posed(&local);
        let bones = points(&gear.rig.posed_mesh(&local));
        // A point of a bone's rest space, in the stance.
        let at = |bone: usize, p: V3| world[bone].point(vec3::sub(p, dims.origins[bone]));
        // The shafts of the long bones, and the column of the spine, as capsules.
        let mut capsules: Vec<(V3, V3, f32)> = Vec::new();
        for side in Side::BOTH {
            for j in 0..2 {
                // The shoulder's ball is where a mail collar lies on the trunk: the shaft starts below it.
                let (a, b) = (world[arm_bone(side) + j].origin, world[arm_bone(side) + j + 1].origin);
                capsules.push((if j == 0 { vec3::lerp(a, b, 0.25) } else { a }, b, 0.09));
                capsules.push((world[leg_bone(side) + j].origin, world[leg_bone(side) + j + 1].origin, 0.12));
            }
        }
        let [t12, _] = dims.thoracic_y;
        let [c7, _] = dims.cervical_y;
        let column: Vec<V3> = (0..=10)
            .map(|k| {
                let y = dims.lumbar_y + (c7 - dims.lumbar_y) * k as f32 / 10.0;
                let bone = if y < t12 { index::SPINE } else { index::RIBCAGE };
                at(bone, dims.spine_at(y))
            })
            .collect();
        capsules.extend(column.windows(2).map(|w| (w[0], w[1], 0.16)));
        // The skull, as an ellipsoid a little inside its bone.
        let head = dims.body.head;
        let (centre, turn) = (at(index::SKULL, head.centre), transpose(&world[index::SKULL].rotation));
        let half = [head.half_width, head.half_height, head.half_depth].map(|h| 0.9 * h);
        let intrudes = |p: V3| {
            let q = turn.transform(vec3::sub(p, centre));
            let inside_skull = (0..3).map(|i| (q[i] / half[i]).powi(2)).sum::<f32>() < 1.0;
            inside_skull || capsules.iter().any(|&(a, b, r)| distance_to_segment(p, a, b).0 < r)
        };
        // The ribcage, the pelvis and each hand and foot, as ellipsoids inside their bones' bounds.
        let bare = SkeletonState::roll(seed).build_gripping(&gear.grips);
        let volumes: Vec<(usize, V3, V3)> = [index::RIBCAGE, index::PELVIS, arm_bone(Side::Right) + 2, arm_bone(Side::Left) + 2, leg_bone(Side::Right) + 2, leg_bone(Side::Left) + 2]
            .into_iter()
            .filter_map(|bone| {
                let (lo, hi) = bare.bones[bone].mesh.bounds()?;
                let share = if bone == index::RIBCAGE || bone == index::PELVIS { VOLUME } else { EXTREMITY };
                Some((bone, vec3::lerp(lo, hi, 0.5), [0, 1, 2].map(|i| share * 0.5 * (hi[i] - lo[i]))))
            })
            .collect();
        let in_volumes = |p: V3, trunk: bool| {
            volumes.iter().any(|&(bone, centre, half)| {
                let q = transpose(&world[bone].rotation).transform(vec3::sub(p, world[bone].origin));
                (trunk == (bone == index::RIBCAGE || bone == index::PELVIS)) && (0..3).map(|i| ((q[i] - centre[i]) / half[i]).powi(2)).sum::<f32>() < 1.0
            })
        };
        let in_trunk = |p: V3| in_volumes(p, true);
        // Sleeves and trousers may hang over a hand or a foot; gloves and boots may not lie in it.
        let in_extremity = |p: V3, slot: Slot| matches!(slot, Slot::Hands | Slot::Feet) && in_volumes(p, false);
        for (piece, mesh) in gear.pieces.iter().zip(&worn) {
            if matches!(piece.slot, Slot::Mainhand | Slot::Offhand) {
                continue;
            }
            let (mut inside, mut inside_trunk, mut inside_end, mut total) = (0usize, 0usize, 0usize, 0usize);
            for p in mesh.parts.iter().flat_map(|part| part.positions.iter()) {
                total += 1;
                inside += intrudes(*p) as usize;
                inside_trunk += in_trunk(*p) as usize;
                inside_end += in_extremity(*p, piece.slot) as usize;
                let d = nearest(*p, &bones);
                assert!(d < MAX_REACH, "{seed}: {} floats {d} from the bones", piece.name);
                *far = far.max(d);
            }
            // A sleeve's root is a collapsed row of vertices along the arm's axis (no area, never drawn): a few lie inside.
            let share = inside.max(inside_trunk) as f32 / total.max(1) as f32;
            *worst = worst.max(share);
            assert!(share <= MAX_INSIDE, "{seed}: {} ({:?} on bone {}) has {inside} or {inside_trunk} of {total} vertices inside bones or the ribcage and pelvis", piece.name, piece.slot, piece.bone);
            // Gloves and boots lie close on a hand or foot, and lap over the joint beside it.
            let share = inside_end as f32 / total.max(1) as f32;
            *worst_end = worst_end.max(share);
            assert!(share <= MAX_INSIDE_END, "{seed}: {} ({:?} on bone {}) has {inside_end} of {total} vertices inside the hand or foot", piece.name, piece.slot, piece.bone);
        }
    }

    #[test]
    fn gear_sits_off_the_bones_and_not_far_from_them_smoke() {
        let (mut far, mut worst, mut worst_end) = (0.0, 0.0, 0.0);
        for seed in 0..16 {
            check_fit(seed, &mut far, &mut worst, &mut worst_end);
        }
        eprintln!("farthest from a bone {far}, most inside {worst}, most in a hand or foot {worst_end}");
    }

    #[test]
    #[ignore = "slow: 200 skeletons"]
    fn gear_sits_off_the_bones_and_not_far_from_them() {
        let (mut far, mut worst, mut worst_end) = (0.0, 0.0, 0.0);
        for seed in 0..200 {
            check_fit(seed, &mut far, &mut worst, &mut worst_end);
        }
        eprintln!("farthest from a bone {far}, most inside {worst}, most in a hand or foot {worst_end}");
    }

    /// The widest opening of a gauntlet's cuff and of a helm's hem, as multiples of the bone
    /// each wraps (the forearm's, the skull's half width): cuffs and hems close in on the bones
    /// instead of flaring like funnels.
    fn check_openings(seed: u32, cuff: &mut f32, hem: &mut f32) {
        let state = SkeletonState::roll(seed);
        let gear = SkeletonGear::roll(&state);
        let dims = state.dims();
        let local = skeleton_armed(&dims, &SkeletonPose::default(), &gear.stance);
        let world = gear.rig.world(&local);
        let worn = gear.posed(&local);
        let head = dims.body.head;
        let skull = world[index::SKULL].point(vec3::sub(head.centre, dims.origins[index::SKULL]));
        for (piece, mesh) in gear.pieces.iter().zip(&worn) {
            let points = || mesh.parts.iter().flat_map(|part| part.positions.iter().copied());
            if piece.slot == Slot::Hands && [arm_bone(Side::Right) + 1, arm_bone(Side::Left) + 1].contains(&piece.bone) {
                let (elbow, wrist) = (world[piece.bone].origin, world[piece.bone + 1].origin);
                let u = vec3::normalize_or(vec3::sub(elbow, wrist), vec3::neg(vec3::Y));
                let along = |p: V3| vec3::dot(vec3::sub(p, wrist), u);
                let Some(far) = points().map(along).max_by(f32::total_cmp) else { continue };
                // The rim is the last tenth of a unit of the cuff.
                let width = points().filter(|&p| along(p) > far - 0.1).map(|p| vec3::length(vec3::sub(vec3::sub(p, wrist), vec3::scale(u, along(p))))).fold(0.0, f32::max);
                *cuff = cuff.max(width / ARM_BONE);
                assert!(width <= 2.0 * ARM_BONE, "{seed}: {} opens {width} round a forearm bone {ARM_BONE}", piece.name);
            }
            if piece.slot == Slot::Head {
                // Only what hangs below the skull's hem is measured: a brim spreads wide above it by design.
                let below = skull[1] - (HEM_BELOW + 0.5 * HEM_DRAWN) * head.half_height;
                let width = points().filter(|p| p[1] < below).map(|p| (p[0] - skull[0]).hypot(p[2] - skull[2])).fold(0.0, f32::max);
                *hem = hem.max(width / head.half_width);
                assert!(width <= 2.0 * head.half_width, "{seed}: {} opens {width} below a skull {} wide", piece.name, head.half_width);
            }
        }
    }

    #[test]
    fn cuffs_and_hems_close_in_on_the_bones_smoke() {
        let (mut cuff, mut hem) = (0.0, 0.0);
        for seed in 0..16 {
            check_openings(seed, &mut cuff, &mut hem);
        }
        eprintln!("widest cuff {cuff} forearm bones, widest hem {hem} skull half widths");
    }

    #[test]
    #[ignore = "slow: 200 skeletons"]
    fn cuffs_and_hems_close_in_on_the_bones() {
        let (mut cuff, mut hem) = (0.0, 0.0);
        for seed in 0..200 {
            check_openings(seed, &mut cuff, &mut hem);
        }
        eprintln!("widest cuff {cuff} forearm bones, widest hem {hem} skull half widths");
    }

    /// No gap between a cuff and its hand or a boot's shaft and its foot opens wider than this
    /// as the skeleton walks and swings.
    const MAX_GAP: f32 = 0.3;

    /// Points over a mesh's surface: each triangle's corners, edge middles, centre and the middles between them.
    fn dense(mesh: &ItemMesh) -> Vec<V3> {
        let mut out = points(mesh);
        for part in &mesh.parts {
            for tri in part.indices.chunks_exact(3) {
                let corners = [0, 1, 2].map(|k| part.positions[tri[k] as usize]);
                let centre = vec3::centroid(&corners);
                out.push(centre);
                for corner in corners {
                    out.push(vec3::lerp(corner, centre, 0.5));
                }
            }
        }
        out
    }

    /// (A boot's toe box, `ahead` of the ankle, is the foot's own.)
    /// The widest gap at the seam between the pieces of `slot` on bone `outer` and on bone `inner`
    /// (the cuff on the forearm and the glove on the hand, or the boot's shaft and its foot), over
    /// a walk and a swing.
    fn check_seam(seed: u32, slot: Slot, outer: [usize; 2], inner: [usize; 2], seam_at: f32, ahead: f32, widest: &mut f32) {
        let state = SkeletonState::roll(seed);
        let gear = SkeletonGear::roll(&state);
        let dims = state.dims();
        let find = |bone: usize| gear.pieces.iter().position(|p| p.slot == slot && p.bone == bone);
        let positions = |mesh: &ItemMesh| -> Vec<V3> { mesh.parts.iter().flat_map(|part| part.positions.iter().copied()).collect() };
        let at_rest = skeleton_armed(&dims, &SkeletonPose::default(), &gear.stance);
        let rest = gear.posed(&at_rest);
        let world = gear.rig.world(&at_rest);
        let poses: Vec<SkeletonPose> = (0..8)
            .map(|k| SkeletonPose { stride: k as f32 * std::f32::consts::TAU / 8.0, pace: 1.0, ..SkeletonPose::default() })
            .chain((0..8).map(|k| SkeletonPose { swing: Some(k as f32 / 7.0), ..SkeletonPose::default() }))
            .collect();
        let moved: Vec<Vec<ItemMesh>> = poses.iter().map(|pose| gear.posed(&skeleton_armed(&dims, pose, &gear.stance))).collect();
        for k in 0..2 {
            let (Some(o), Some(i)) = (find(outer[k]), find(inner[k])) else { continue };
            // The seam: the outer piece's own vertices up to a tenth over the joint's plane at rest.
            let (joint, up) = (world[inner[k]].origin, vec3::normalize_or(vec3::sub(world[outer[k]].origin, world[inner[k]].origin), vec3::Y));
            let outside = positions(&rest[o]);
            // Only what lies near the joint can close its gap.
            let near: Vec<usize> = dense(&rest[i]).iter().enumerate().filter(|(_, q)| vec3::distance(**q, joint) < 2.0).map(|(n, _)| n).collect();
            let seam: Vec<usize> = (0..outside.len()).filter(|&v| (0.0..0.15).contains(&(vec3::dot(vec3::sub(outside[v], joint), up) - seam_at)) && outside[v][2] - joint[2] < ahead).collect();
            for mesh in &moved {
                let outside = positions(&mesh[o]);
                let all = dense(&mesh[i]);
                let inside: Vec<V3> = near.iter().map(|&n| all[n]).collect();
                for &v in &seam {
                    let gap = nearest(outside[v], &inside);
                    *widest = widest.max(gap);
                    assert!(gap <= MAX_GAP, "{seed}: {} gapes {gap} at its seam", gear.pieces[o].name);
                }
            }
        }
    }

    fn check_seams(seed: u32, cuff: &mut f32, shaft: &mut f32) {
        let arms = [Side::Right, Side::Left].map(arm_bone);
        check_seam(seed, Slot::Hands, arms.map(|b| b + 1), arms.map(|b| b + 2), 0.0, f32::MAX, cuff);
        let legs = [Side::Right, Side::Left].map(leg_bone);
        check_seam(seed, Slot::Feet, legs.map(|b| b + 1), legs.map(|b| b + 2), FOOT_TOP, TOE_AT, shaft);
    }

    #[test]
    fn walking_and_swinging_leave_no_gap_at_a_cuff_or_a_boot_smoke() {
        let (mut cuff, mut shaft) = (0.0, 0.0);
        for seed in 0..16 {
            check_seams(seed, &mut cuff, &mut shaft);
        }
        eprintln!("widest gap at a cuff {cuff}, at a boot {shaft}");
    }

    #[test]
    #[ignore = "slow: 200 skeletons"]
    fn walking_and_swinging_leave_no_gap_at_a_cuff_or_a_boot() {
        let (mut cuff, mut shaft) = (0.0, 0.0);
        for seed in 0..200 {
            check_seams(seed, &mut cuff, &mut shaft);
        }
        eprintln!("widest gap at a cuff {cuff}, at a boot {shaft}");
    }
}
