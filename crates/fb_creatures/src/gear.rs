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
        let fit = fit_worn(posed, &worn);

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
        for layer in &fit.layers {
            let Some((_, item)) = outfit.worn.iter().find(|(s, _)| *s == layer.slot) else { continue };
            let mut by_bone: Vec<Vec<Part>> = (0..index::COUNT).map(|_| Vec::new()).collect();
            let seams = Seams::new(posed, &dims.legs, layer.slot);
            for part in &layer.mesh.parts {
                for (bone, cut) in cut_part(part, |p| seams.bone_for(p), &frames) {
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
                for (_, cut) in cut_part(part, |_| bone, &frames) {
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

/// Where a layer's triangles go: by slot, headgear to the skull and gloves to the nearest hand;
/// otherwise to whichever bone's segment each triangle lies nearest.
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
            foot_top: legs[0][2][1] + 0.6,
        }
    }

    fn bone_for(&self, p: V3) -> usize {
        let side_of_x = |p: V3| if p[0] < 0.0 { 0 } else { 1 };
        let sides = [Side::Right, Side::Left];
        match self.slot {
            Slot::Head => index::SKULL,
            Slot::Hands => {
                let k = (0..2).min_by(|&a, &b| vec3::distance(p, self.arms[a].0[2]).total_cmp(&vec3::distance(p, self.arms[b].0[2]))).unwrap_or(0);
                arm_bone(sides[k]) + 2
            }
            Slot::Feet => {
                let k = side_of_x(p);
                if p[1] < self.foot_top { leg_bone(sides[k]) + 2 } else { leg_bone(sides[k]) + 1 }
            }
            Slot::Legs => {
                let k = side_of_x(p);
                let knee = self.legs[k][1][1];
                if p[1] < knee { leg_bone(sides[k]) + 1 } else { leg_bone(sides[k]) }
            }
            _ => {
                // The trunk's reach, against each arm's: sleeves go to the arm they lie along,
                // but the ball of the shoulder stays with the trunk.
                let (a, b, half) = self.trunk;
                let mut best = (distance_to_segment(p, a, b).0 - 0.9 * half, index::RIBCAGE);
                for (k, (joints, radius)) in self.arms.iter().enumerate() {
                    let (d, t) = distance_to_segment(p, joints[0], joints[1]);
                    let reach = vec3::distance(joints[0], joints[1]);
                    if t * reach > 1.4 * radius && d - 1.3 * radius < best.0 {
                        best = (d - 1.3 * radius, arm_bone(sides[k]));
                    }
                    let (d, _) = distance_to_segment(p, joints[1], joints[2]);
                    if d - 1.3 * radius < best.0 {
                        best = (d - 1.3 * radius, arm_bone(sides[k]) + 1);
                    }
                }
                best.1
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

/// `part` cut by triangle into one part for each bone `bone_for` sends the triangle's centre
/// to, each in that bone's own space (`frames` gives every bone's place in body space).
fn cut_part(part: &Part, bone_for: impl Fn(V3) -> usize, frames: &[(V3, Mat3)]) -> Vec<(usize, Part)> {
    let mut groups: Vec<(usize, Vec<[u32; 3]>)> = Vec::new();
    for tri in part.indices.chunks_exact(3) {
        let corners = [tri[0], tri[1], tri[2]].map(|i| part.positions[i as usize]);
        let bone = bone_for(vec3::centroid(&corners));
        match groups.iter_mut().find(|(b, _)| *b == bone) {
            Some((_, tris)) => tris.push([tri[0], tri[1], tri[2]]),
            None => groups.push((bone, vec![[tri[0], tri[1], tri[2]]])),
        }
    }
    groups.sort_by_key(|(b, _)| *b);
    groups
        .into_iter()
        .map(|(bone, tris)| {
            let back = transpose(&frames[bone].1);
            let mut remap = vec![u32::MAX; part.positions.len()];
            let mut cut = Part { name: part.name, material: part.material, tint: part.tint, pattern: part.pattern, glow: part.glow, outline: part.outline, ..Part::default() };
            for tri in tris {
                for i in tri {
                    let at = i as usize;
                    if remap[at] == u32::MAX {
                        remap[at] = cut.positions.len() as u32;
                        cut.positions.push(back.transform(vec3::sub(part.positions[at], frames[bone].0)));
                        cut.normals.push(back.transform(part.normals[at]));
                        cut.uvs.push(part.uvs[at]);
                        cut.outline_normals.push(back.transform(part.outline_normals[at]));
                    }
                    cut.indices.push(remap[at]);
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
                Slot::Hands => assert!(on.iter().all(|b| [arm_bone(Side::Right) + 2, arm_bone(Side::Left) + 2].contains(b)), "{on:?}"),
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

    /// The closed hand lies round the grip and the held item passes through it.
    fn check_grips(seed: u32) {
        let (gear, worn, _) = standing(seed);
        for (k, side) in Side::BOTH.into_iter().enumerate() {
            let Some(grip) = gear.grip_points[k] else { continue };
            let hand = arm_bone(side) + 2;
            // The fingers close round the grip: some of the hand is near its centre, none inside it.
            if !gear.rig.bones[hand].mesh.parts.is_empty() {
                let d = nearest(grip, &points(&gear.rig.bones[hand].mesh));
                assert!((0.03..0.45).contains(&d), "{seed} {side:?}: the fingers are {d} from the grip");
            }
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

    /// Gear stays out of the long bones, the spine and the skull, and not far from the bones;
    /// `far` gathers the greatest distance of any vertex from a bone.
    fn check_fit(seed: u32, far: &mut f32, worst: &mut f32) {
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
        for (piece, mesh) in gear.pieces.iter().zip(&worn) {
            if matches!(piece.slot, Slot::Mainhand | Slot::Offhand) {
                continue;
            }
            let (mut inside, mut total) = (0usize, 0usize);
            for p in mesh.parts.iter().flat_map(|part| part.positions.iter()) {
                total += 1;
                inside += intrudes(*p) as usize;
                let d = nearest(*p, &bones);
                assert!(d < MAX_REACH, "{seed}: {} floats {d} from the bones", piece.name);
                *far = far.max(d);
            }
            // A sleeve's root is a collapsed row of vertices along the arm's axis (no area, never drawn): a few lie inside.
            let share = inside as f32 / total.max(1) as f32;
            *worst = worst.max(share);
            assert!(share <= MAX_INSIDE, "{seed}: {} ({:?} on bone {}) has {inside} of {total} vertices inside bones", piece.name, piece.slot, piece.bone);
        }
    }

    #[test]
    fn gear_sits_off_the_bones_and_not_far_from_them_smoke() {
        let (mut far, mut worst) = (0.0, 0.0);
        for seed in 0..16 {
            check_fit(seed, &mut far, &mut worst);
        }
        eprintln!("farthest from a bone {far}, most inside {worst}");
    }

    #[test]
    #[ignore = "slow: 200 skeletons"]
    fn gear_sits_off_the_bones_and_not_far_from_them() {
        let (mut far, mut worst) = (0.0, 0.0);
        for seed in 0..200 {
            check_fit(seed, &mut far, &mut worst);
        }
        eprintln!("farthest from a bone {far}, most inside {worst}");
    }
}
