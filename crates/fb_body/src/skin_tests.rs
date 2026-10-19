//! Specs for the rig, its weights and its gait together: gear stays on the limbs it covers, held
//! items stay in the fists, cloth is not torn, posing is cheap.

use std::collections::HashMap;

use armory::Part;
use armory::geom::vec3::{self, V3};
use fb_items::Slot;

use crate::body::Body;
use crate::gait::{Gait, IdleCycle};
use crate::limb::Side;
use crate::rig::{JOINTS, Joint, Rig, RigPose, Rigid};
use crate::skin::HEM_SHARE;

/// Seconds of CPU this thread has used. Timing checks read it rather than the wall clock, so a
/// machine busy with other tests (or anything else) does not fail them.
pub(crate) fn thread_cpu_seconds() -> f64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `t` is a valid timespec for the call to write into.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
    t.tv_sec as f64 + t.tv_nsec as f64 * 1e-9
}
use crate::skin::{PartSkin, SkinRegion, Skinner};
use crate::worn::fit_worn;
use crate::{BodySpec, Folk, fit_hands};

/// A body dressed in an outfit as the game dresses it, with everything needed to pose it.
struct Dressed {
    body: Body,
    rig: Rig,
    gait: Gait,
    skinner: Skinner,
    /// The bare skin parts with their weights.
    skin: Vec<(Part, PartSkin)>,
    /// Each worn layer's parts with their weights.
    layers: Vec<(Slot, Part, PartSkin)>,
}

fn dress(folk: Folk, seed: u32, outfit: &str) -> Dressed {
    let spec = BodySpec::roll(folk, seed);
    let worn = fb_items::outfit::outfit(outfit).unwrap().roll(seed, folk.key()).worn;
    let held = |slot| worn.iter().find(|(s, _)| *s == slot).map(|(_, item)| &item.shape);
    let hands = fit_hands(&spec, held(Slot::Mainhand), held(Slot::Offhand));
    let body = hands.body;
    let shapes: Vec<(Slot, armory::Item)> = worn.iter().map(|(slot, item)| (*slot, item.shape.clone())).collect();
    let fit = fit_worn(&body, &shapes);
    let skinner = Skinner::new(&body);
    let skin = body.skin_mesh().parts.into_iter().map(|p| (p.clone(), skinner.part(&p, SkinRegion::of_skin(p.name)))).collect();
    let mut layers = vec![];
    for layer in &fit.layers {
        for part in &layer.mesh.parts {
            layers.push((layer.slot, part.clone(), skinner.part(part, SkinRegion::of_worn(layer.slot, part.name))));
        }
    }
    let rig = Rig::of(&body);
    let gait = Gait::new(&body, &rig);
    Dressed { body, rig, gait, skinner, skin, layers }
}

fn skinned(frames: &[Rigid; JOINTS], part: &Part, skin: &PartSkin) -> Vec<V3> {
    part.positions.iter().zip(skin.joints.iter().zip(&skin.weights)).map(|(&p, (&j, &w))| Rig::skin_point(frames, p, j, w)).collect()
}

/// Points in a hash grid, for nearest-point distances.
struct Grid {
    cells: HashMap<[i32; 3], Vec<V3>>,
}

const CELL: f32 = 0.5;

impl Grid {
    fn new(points: impl IntoIterator<Item = V3>) -> Grid {
        let mut cells: HashMap<[i32; 3], Vec<V3>> = HashMap::new();
        for p in points {
            cells.entry(p.map(|v| (v / CELL).floor() as i32)).or_default().push(p);
        }
        Grid { cells }
    }

    /// The distance from `q` to the nearest point within `reach` cells (`f32::MAX` if none).
    fn nearest(&self, q: V3, reach: i32) -> f32 {
        let c = q.map(|v| (v / CELL).floor() as i32);
        let mut best = f32::MAX;
        for dx in -reach..=reach {
            for dy in -reach..=reach {
                for dz in -reach..=reach {
                    if let Some(points) = self.cells.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        for &p in points {
                            best = best.min(vec3::distance(p, q));
                        }
                    }
                }
            }
        }
        best
    }
}

fn poses(d: &Dressed) -> Vec<(String, RigPose)> {
    let speed = d.gait.natural_speed();
    let mut out: Vec<(String, RigPose)> = (0..8).map(|i| (format!("walk {i}/8"), d.gait.walk(i as f32 / 8.0, speed))).collect();
    out.push(("idle".into(), d.gait.idle(&IdleCycle::of(1207), 3.0)));
    out
}

fn all_parts(d: &Dressed) -> impl Iterator<Item = (&Part, &PartSkin)> {
    d.skin.iter().map(|(p, s)| (p, s)).chain(d.layers.iter().map(|(_, p, s)| (p, s)))
}

#[test]
fn weights_are_valid_and_the_same_for_the_same_seed() {
    for folk in Folk::ALL {
        let a = dress(folk, 1207, "fighter");
        let b = dress(folk, 1207, "fighter");
        assert_eq!(a.rig, b.rig);
        for ((_, sa), (_, sb)) in a.skin.iter().zip(&b.skin) {
            assert_eq!(sa, sb, "{folk:?}");
        }
        for ((_, _, sa), (_, _, sb)) in a.layers.iter().zip(&b.layers) {
            assert_eq!(sa, sb, "{folk:?}");
        }
        for (part, skin) in all_parts(&a) {
            assert_eq!(skin.joints.len(), part.positions.len());
            for (j, w) in skin.joints.iter().zip(&skin.weights) {
                assert!(j.iter().all(|&j| (j as usize) < JOINTS));
                assert!(w.iter().all(|&w| (0.0..=1.0 + 1e-5).contains(&w)), "{folk:?} {}: {w:?}", part.name);
                assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-4);
            }
        }
    }
}

#[test]
fn the_rest_pose_leaves_everything_where_it_was() {
    let d = dress(Folk::Human, 1207, "fighter");
    let frames = d.rig.skin_frames(&RigPose::REST);
    for (part, skin) in all_parts(&d) {
        for (a, b) in part.positions.iter().zip(skinned(&frames, part, skin)) {
            assert!(vec3::distance(*a, b) < 1e-3);
        }
    }
}

fn share(part: &Part, skin: &PartSkin, joints: &[Joint]) -> f32 {
    let mut total = 0.0;
    for (j, w) in skin.joints.iter().zip(&skin.weights) {
        total += j.iter().zip(w).filter(|(j, _)| joints.iter().any(|x| x.index() == **j as usize)).map(|(_, w)| w).sum::<f32>();
    }
    total / part.positions.len().max(1) as f32
}

#[test]
fn each_part_follows_its_own_limb() {
    let d = dress(Folk::Human, 1207, "fighter");
    let arms: Vec<Joint> = [Side::Right, Side::Left].into_iter().flat_map(Joint::arm).collect();
    let legs: Vec<Joint> = [Side::Right, Side::Left].into_iter().flat_map(Joint::leg).collect();
    for (part, skin) in d.skin.iter().map(|(p, s)| (p, s)) {
        match part.name {
            "feet" => assert!(share(part, skin, &[Joint::AnkleL, Joint::AnkleR]) > 0.85, "feet"),
            "hands" => assert!(share(part, skin, &[Joint::WristL, Joint::WristR]) > 0.99, "hands"),
            "arms" => assert!(share(part, skin, &arms) > 0.6, "arms"),
            "legs" => assert!(share(part, skin, &legs) > 0.6, "legs"),
            _ => {}
        }
    }
    // The head is on the head joint alone.
    let head = d.skinner.vertex(d.body.head.centre, SkinRegion::Head);
    assert_eq!((head.0[0], head.1[0]), (Joint::Head.index() as u16, 1.0));
    // The skin of the neck blends the chest into the head.
    let neck = d.skinner.vertex([0.0, 0.5 * (d.body.shoulder_y + d.body.neck_base().origin[1] + 0.05 * d.body.trunk_length), 0.0], SkinRegion::Trunk);
    let heads: f32 = neck.0.iter().zip(neck.1).filter(|(j, _)| **j as usize == Joint::Head.index()).map(|(_, w)| w).sum();
    assert!(heads > 0.05 && heads < 0.9, "{heads}");
}

#[test]
fn loose_cloth_follows_the_trunk_with_a_share_of_the_thighs() {
    let d = dress(Folk::Human, 1207, "fighter");
    let legs: Vec<Joint> = [Side::Right, Side::Left].into_iter().flat_map(Joint::leg).collect();
    let leg_share = |p: V3| -> f32 {
        let (j, w) = d.skinner.vertex(p, SkinRegion::Loose);
        j.iter().zip(w).filter(|(j, _)| legs.iter().any(|l| l.index() == **j as usize)).map(|(_, w)| w).sum()
    };
    for y in [d.body.hip_y, d.body.hip_y * 0.7, d.body.hip_y * 0.3] {
        for x in [-2.0, -0.2, 0.2, 2.0] {
            let leg = leg_share([x, y, 0.0]);
            assert!(leg <= HEM_SHARE + 0.01, "a share only: {leg} at ({x}, {y})");
            if y < d.body.hip_y * 0.5 {
                assert!(leg > 0.1, "some of the thighs toward the hem: {leg} at ({x}, {y})");
            }
        }
    }
    // Down the midline the two legs pull alike, so the cloth is not torn between them.
    let (j, w) = d.skinner.vertex([0.0, d.body.hip_y * 0.3, 0.0], SkinRegion::Loose);
    let side = |legs: [Joint; 3]| j.iter().zip(w).filter(|(j, _)| legs.iter().any(|l| l.index() == **j as usize)).map(|(_, w)| w).sum::<f32>();
    assert!((side(Joint::leg(Side::Left)) - side(Joint::leg(Side::Right))).abs() < 0.02);
}

/// Whether this layer's part covers a limb: legwear, boots, gloves and sleeves.
fn covers_a_limb(slot: Slot, part: &Part) -> bool {
    matches!(slot, Slot::Legs | Slot::Feet | Slot::Hands) || (matches!(slot, Slot::Shirt | Slot::Body) && part.name == "sleeves")
}

/// Whether vertex `v` follows one limb (at least 95% of its weight on one arm's or one leg's joints).
fn clings(skin: &PartSkin, v: usize) -> bool {
    [Side::Right, Side::Left].into_iter().flat_map(|side| [Joint::arm(side), Joint::leg(side)]).any(|limb| {
        let on: f32 = skin.joints[v].iter().zip(skin.weights[v]).filter(|(j, _)| limb.iter().any(|l| l.index() == **j as usize)).map(|(_, w)| w).sum();
        on >= 0.95
    })
}

/// The bare skin a layer's part covers, by the skin parts' names: gloves and sleeves the arms and
/// hands, boots the legs and feet, legwear those and the hips (the trunk).
fn covered_skin(slot: Slot) -> &'static [&'static str] {
    match slot {
        Slot::Legs => &["legs", "feet", "trunk"],
        Slot::Feet => &["legs", "feet"],
        _ => &["arms", "hands"],
    }
}

/// Posed gear stays as near the skin it covers as it was at rest.
fn gear_stays_on_the_skin(folk: Folk, seed: u32, outfit: &str) {
    let d = dress(folk, seed, outfit);
    // How far from the skin gear may drift past where it sat: about a vertex spacing.
    const TOLERANCE: f32 = 0.3;
    let surface = |slot: Slot, frames: Option<&[Rigid; JOINTS]>| {
        Grid::new(d.skin.iter().filter(|(p, _)| covered_skin(slot).contains(&p.name)).flat_map(|(p, s)| match frames {
            Some(frames) => skinned(frames, p, s),
            None => p.positions.clone(),
        }))
    };
    let at_rest: Vec<(Slot, Grid)> = [Slot::Legs, Slot::Feet, Slot::Hands].map(|slot| (slot, surface(slot, None))).into();
    for (name, pose) in poses(&d) {
        let frames = d.rig.skin_frames(&pose);
        let posed: Vec<(Slot, Grid)> = [Slot::Legs, Slot::Feet, Slot::Hands].map(|slot| (slot, surface(slot, Some(&frames)))).into();
        for (slot, part, skin) in &d.layers {
            if !covers_a_limb(*slot, part) {
                continue;
            }
            let kind = if matches!(slot, Slot::Legs | Slot::Feet) { *slot } else { Slot::Hands };
            let (rest_grid, posed_grid) = (&at_rest.iter().find(|(s, _)| *s == kind).unwrap().1, &posed.iter().find(|(s, _)| *s == kind).unwrap().1);
            for (v, (rest, now)) in part.positions.iter().zip(skinned(&frames, part, skin)).enumerate() {
                let before = rest_grid.nearest(*rest, 2);
                // Gear well off the skin (a flared sleeve, a baggy leg) is loose, not fitted; and
                // cloth shared between the legs (the seam of baggy trousers) clings to neither.
                if before > 0.6 || !clings(skin, v) {
                    continue;
                }
                let after = posed_grid.nearest(now, 3);
                // Over a shoulder or a hip, where the trunk blends into the limb, the cloth and
                // the skin under it blend alike but not to the same vertex: twice the room.
                let at_a_root = [Joint::ShoulderR, Joint::ShoulderL, Joint::HipR, Joint::HipL].iter().any(|j| vec3::distance(*rest, d.rig.rest_at(*j)) < 1.5);
                let room = if at_a_root { 2.0 * TOLERANCE } else { TOLERANCE };
                assert!(after <= before + room, "{folk:?} {outfit} {slot:?}/{} in {name}: {before:.2} from the skin at rest, {after:.2} posed, at {rest:?} {:?} {:?}", part.name, skin.joints[v], skin.weights[v]);
            }
        }
    }
}

#[test]
#[ignore = "slow sweep: a fighter's gear on the skin through the walk for every folk; run before merging fitting changes"]
fn legwear_boots_sleeves_and_gloves_stay_on_the_skin_through_the_walk() {
    for folk in Folk::ALL {
        gear_stays_on_the_skin(folk, 1207, "fighter");
    }
}

/// [`legwear_boots_sleeves_and_gloves_stay_on_the_skin_through_the_walk`] on the two builds
/// furthest from a human's: the broad, tailed lizardfolk and the small gnome.
#[test]
fn legwear_boots_sleeves_and_gloves_stay_on_the_skin_through_the_walk_smoke() {
    for folk in [Folk::Lizardfolk, Folk::Gnome] {
        gear_stays_on_the_skin(folk, 1207, "fighter");
    }
}

#[test]
#[ignore = "slow sweep: gear on the skin for every outfit and folk, seed 1207; run before merging fitting changes"]
fn gear_stays_on_the_skin_in_every_outfit() {
    for folk in Folk::ALL {
        for outfit in fb_items::outfit::outfits() {
            gear_stays_on_the_skin(folk, 1207, &outfit.key);
        }
    }
}

#[test]
fn cloth_is_not_torn_by_the_walk() {
    for folk in Folk::ALL {
        let d = dress(folk, 1207, "fighter");
        for (name, pose) in poses(&d) {
            let frames = d.rig.skin_frames(&pose);
            for (slot, part, skin) in &d.layers {
                if !matches!(slot, Slot::Shirt | Slot::Body | Slot::Cloak | Slot::Legs) {
                    continue;
                }
                let moved = skinned(&frames, part, skin);
                let mut stretch: Vec<f32> = vec![];
                for tri in part.indices.chunks(3) {
                    for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                        let (i, j) = (tri[a] as usize, tri[b] as usize);
                        let rest = vec3::distance(part.positions[i], part.positions[j]);
                        if rest > 0.05 {
                            let k = vec3::distance(moved[i], moved[j]) / rest;
                            stretch.push(k);
                        }
                    }
                }
                if stretch.is_empty() {
                    continue;
                }
                stretch.sort_by(f32::total_cmp);
                let at = |q: f32| stretch[((stretch.len() - 1) as f32 * q) as usize];
                // Nearly all of the cloth keeps its shape. Two legs, though, cannot both hold a
                // surface between them (the seams of baggy trousers, the wraps down the inner
                // shins of fat legs): of trousers, three edges in four hold and none tears right through.
                if *slot == Slot::Legs {
                    assert!(at(0.75) < 1.6, "{folk:?} {slot:?}/{} in {name}: 1 edge in 4 stretched {:.1} times", part.name, at(0.75));
                    assert!(at(1.0) < 25.0, "{folk:?} {slot:?}/{} in {name}: an edge stretched {:.1} times", part.name, at(1.0));
                } else {
                    assert!(at(0.99) < 4.0, "{folk:?} {slot:?}/{} in {name}: 1 edge in 100 stretched {:.1} times", part.name, at(0.99));
                }
            }
        }
    }
}

#[test]
fn held_items_stay_in_the_fists_through_the_walk() {
    for folk in Folk::ALL {
        let d = dress(folk, 1207, "fighter");
        let hands = d.skin.iter().find(|(p, _)| p.name == "hands").unwrap();
        let mut held = 0;
        for side in Side::BOTH {
            if d.body.pose.arm(side).grip.is_none() {
                continue;
            }
            held += 1;
            let grip = d.body.grip(side).origin;
            let wrist = Joint::wrist(side).index();
            // The hand's own vertex nearest the grip at rest.
            let nearest = (0..hands.0.positions.len()).min_by(|&a, &b| vec3::distance(hands.0.positions[a], grip).total_cmp(&vec3::distance(hands.0.positions[b], grip))).unwrap();
            let rest = vec3::distance(hands.0.positions[nearest], grip);
            for (name, pose) in poses(&d) {
                let frames = d.rig.skin_frames(&pose);
                // A held item rides the wrist joint, rigidly: the fist stays the same distance
                // from the hand's skin.
                let fist = frames[wrist].point(grip);
                let hand = skinned(&frames, &hands.0, &hands.1)[nearest];
                assert!((vec3::distance(hand, fist) - rest).abs() < 0.02, "{folk:?} {side:?} in {name}: fist {fist:?}, hand {hand:?}");
            }
        }
        assert!(held > 0, "{folk:?} holds something");
    }
}

#[test]
fn weighting_a_dressed_folk_is_quick() {
    let d = dress(Folk::Human, 1207, "fighter");
    let vertices: usize = d.skin.iter().map(|(p, _)| p.positions.len()).sum::<usize>() + d.layers.iter().map(|(_, p, _)| p.positions.len()).sum::<usize>();
    let start = thread_cpu_seconds();
    for (part, _) in &d.skin {
        d.skinner.part(part, SkinRegion::of_skin(part.name));
    }
    for (slot, part, _) in &d.layers {
        d.skinner.part(part, SkinRegion::of_worn(*slot, part.name));
    }
    let took = thread_cpu_seconds() - start;
    // Done once when a folk is built (off the main thread, with the fitting), a few tens of
    // thousands of vertices.
    assert!(took < 1.0, "{vertices} vertices took {took:.2} s");
}

#[test]
fn posing_every_folk_each_frame_is_cheap() {
    let all: Vec<(Gait, IdleCycle)> = Folk::ALL
        .into_iter()
        .map(|folk| {
            let d = dress(folk, 1207, "fighter");
            (d.gait, IdleCycle::of(1207))
        })
        .collect();
    // A frame's worth for a level of a hundred folk, a hundred frames over.
    let start = thread_cpu_seconds();
    let mut sum = 0.0;
    for frame in 0..100 {
        for person in 0..100 {
            let (gait, cycle) = &all[person % all.len()];
            let pose = gait.pose(frame as f32 * 0.013 + person as f32 * 0.1, gait.natural_speed(), cycle, frame as f32 * 0.016);
            sum += pose.offset[0][1];
        }
    }
    let per_frame = (thread_cpu_seconds() - start) / 100.0;
    assert!(sum.is_finite());
    assert!(per_frame < 0.004, "{:.2} ms to pose a hundred folk", per_frame * 1000.0);
}
