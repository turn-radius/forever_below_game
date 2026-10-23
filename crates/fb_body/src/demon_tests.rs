//! The three demons (winged, horned, succubus) and the parts they turn on beyond the minotaur's:
//! bat wings, a spade tail end and claws.

use armory::geom::vec3;
use armory::{Family, Item};
use fb_items::Slot;

use crate::body::Body;
use crate::folk::Folk;
use crate::gait::{Gait, IdleCycle};
use crate::held::fit_held;
use crate::limb::Side;
use crate::rig::{Joint, Rig, RigPose};
use crate::skin::{SkinRegion, Skinner};
use crate::spec::BodySpec;
use crate::worn::fit_worn;

const DEMONS: [Folk; 3] = [Folk::WingedDemon, Folk::HornedDemon, Folk::Succubus];

fn demon(folk: Folk, seed: u32) -> Body {
    Body::new(&BodySpec::roll(folk, seed))
}

fn human_height() -> f32 {
    Body::new(&BodySpec::typical(Folk::Human)).height
}

fn part<'a>(mesh: &'a armory::ItemMesh, name: &str) -> Option<&'a armory::Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

#[test]
fn the_demons_are_denizens_built_from_their_seeds_and_the_kinds_differ() {
    for folk in DEMONS {
        assert!(folk.is_denizen() && Folk::parse(folk.key()) == Some(folk) && Folk::parse(&folk.race().label) == Some(folk));
        assert_eq!(Folk::of_denizen(folk.denizen_body().unwrap()), Some(folk));
        for seed in [1, 7, 1207] {
            assert_eq!(demon(folk, seed).mesh(), demon(folk, seed).mesh(), "{folk:?} {seed}");
        }
        assert_ne!(demon(folk, 1).mesh(), demon(folk, 2).mesh());
        let srd = folk.race().srd.as_ref().expect("an SRD block");
        let (creature, page, cr, ac, hp, stats, dv) = match folk {
            Folk::WingedDemon => ("Vrock", 273, "6", 15, 104, [17, 15, 18, 8, 13], 120),
            Folk::HornedDemon => ("Glabrezu", 271, "9", 17, 157, [20, 15, 21, 19, 17], 120),
            _ => ("Succubus/Incubus", 349, "4", 15, 66, [8, 17, 13, 15, 12], 60),
        };
        assert_eq!((srd.creature.as_str(), srd.page, srd.cr.as_str(), srd.ac, srd.hp), (creature, page, cr, ac, hp), "{folk:?}");
        assert_eq!((folk.race().stats, folk.race().dv), (stats, dv), "{folk:?}");
    }
    let kinds: Vec<Vec<&str>> = DEMONS.iter().map(|&f| demon(f, 7).skin_mesh().parts.iter().map(|p| p.name).collect()).collect();
    assert!(kinds[0].contains(&"wing_membrane") && kinds[2].contains(&"wing_membrane") && !kinds[1].contains(&"wing_membrane"));
    for k in &kinds {
        assert!(k.contains(&"tail_spade") && k.contains(&"tail"));
    }
    assert!(kinds[0].contains(&"hooves") && kinds[1].contains(&"hooves") && !kinds[2].contains(&"hooves"), "the succubus walks on feet");
    // The wings are larger on the winged demon than on the succubus.
    let reach = |f: Folk| demon(f, 7).wing(Side::Left, 0.0).unwrap().tips.iter().map(|t| t[1]).fold(f32::MAX, f32::min);
    let (w, s) = (demon(Folk::WingedDemon, 7), demon(Folk::Succubus, 7));
    assert!(w.height - reach(Folk::WingedDemon) > s.height - reach(Folk::Succubus) + 1.0);
}

#[test]
fn the_demons_stand_at_their_heights_and_every_part_is_valid() {
    let human = human_height();
    for seed in 0..12 {
        for (folk, lo, hi) in [(Folk::WingedDemon, 1.0, 1.25), (Folk::HornedDemon, 1.3, 1.5), (Folk::Succubus, 0.9, 1.1)] {
            let body = demon(folk, seed);
            let k = body.height / human;
            assert!((lo..hi).contains(&k), "{folk:?} {seed}: {k} times a human");
            for part in body.mesh().parts.iter() {
                part.validate().unwrap_or_else(|e| panic!("{folk:?} {seed} {}: {e}", part.name));
            }
        }
    }
}

#[test]
fn wings_fold_at_rest_and_stay_clear_of_the_body() {
    for folk in [Folk::WingedDemon, Folk::Succubus] {
        for seed in 0..4 {
            let body = demon(folk, seed);
            for side in Side::BOTH {
                let wing = body.wing(side, 0.0).unwrap();
                assert!(wing.open < 0.5, "{folk:?} {seed}: rests folded or half open");
                // Folded, the wing hangs down the back: its fingers end well below the wrist.
                assert!(wing.tips.iter().all(|t| t[1] < wing.wrist[1] - 3.0), "{folk:?} {seed}: fingers hang");
                let sg = side.sign();
                assert!(wing.tips.iter().all(|t| t[0] * sg > 0.0), "{folk:?} {seed}: on its own side");
            }
            let skin = body.skin_mesh();
            for name in ["wing_bones", "wing_membrane"] {
                let p = part(&skin, name).unwrap();
                let root = body.wing(Side::Left, 0.0).unwrap().root_skin;
                for q in p.positions.iter().filter(|q| vec3::distance(**q, [root[0].abs() * q[0].signum(), root[1], root[2]]) > 1.0) {
                    let floor = body.back_floor(q[1]);
                    assert!(q[2] < floor + 0.06, "{folk:?} {seed} {name}: a point at {q:?} is in front of the back's {floor}");
                }
            }
        }
    }
}


#[test]
fn wings_spread_a_little_in_the_walk_and_the_idle() {
    for folk in [Folk::WingedDemon, Folk::Succubus] {
        let body = demon(folk, 7);
        let rig = Rig::of(&body);
        let gait = Gait::new(&body, &rig);
        let skinner = Skinner::new(&body);
        let skin = body.skin_mesh();
        let bones = part(&skin, "wing_bones").unwrap();
        let weights = skinner.part(bones, SkinRegion::of_skin("wing_bones"));
        // The farthest finger tip of the left wing, spread of the whole wing sideways.
        let width = |pose: &RigPose| {
            let frames = rig.skin_frames(pose);
            bones.positions.iter().enumerate().filter(|(_, p)| p[0] > 0.0).map(|(k, &p)| Rig::skin_point(&frames, p, weights.joints[k], weights.weights[k])[0]).fold(f32::MIN, f32::max)
        };
        let rest = width(&RigPose::REST);
        let walking = width(&gait.walk(0.3, gait.natural_speed()));
        let idle = width(&gait.idle(&IdleCycle::of(1), 0.0));
        assert!(walking > rest + 0.3, "{folk:?}: the walk spreads the wings: {rest} to {walking}");
        assert!(idle > rest + 0.1, "{folk:?}: the idle holds them a little open: {rest} to {idle}");
        assert!(walking < rest + 0.5 * body.height, "{folk:?}: only a little");
        // The rest pose moves nothing; a body without wings leaves the joints alone.
        let human = Body::new(&BodySpec::roll(Folk::Human, 3));
        let (hrig, hgait) = {
            let r = Rig::of(&human);
            let g = Gait::new(&human, &r);
            (r, g)
        };
        let pose = hgait.walk(0.3, hgait.natural_speed());
        for joint in [Joint::WingR0, Joint::WingR1, Joint::WingL0, Joint::WingL1] {
            assert_eq!(pose.rotation[joint.index()], RigPose::REST.rotation[joint.index()]);
            assert_eq!(hrig.rest_at(joint), hrig.rest_at(Joint::Chest));
        }
    }
}

#[test]
fn wing_weights_follow_the_bones_and_the_trunk() {
    let body = demon(Folk::WingedDemon, 7);
    let skinner = Skinner::new(&body);
    let skin = body.skin_mesh();
    let rig = Rig::of(&body);
    for name in ["wing_bones", "wing_membrane"] {
        let p = part(&skin, name).unwrap();
        let w = skinner.part(p, SkinRegion::of_skin(name));
        let mut by_joint = [0.0f32; crate::rig::JOINTS];
        for (js, ws) in w.joints.iter().zip(&w.weights) {
            assert!((ws.iter().sum::<f32>() - 1.0).abs() < 1e-3);
            for (&j, &x) in js.iter().zip(ws) {
                by_joint[j as usize] += x;
            }
        }
        assert!(by_joint[Joint::WingL1.index()] > 10.0 && by_joint[Joint::WingR1.index()] > 10.0, "{name}: the wrists carry the fingers");
        if name == "wing_membrane" {
            assert!(by_joint[Joint::Chest.index()] + by_joint[Joint::Spine.index()] + by_joint[Joint::Pelvis.index()] > 10.0, "the sail follows the trunk where it is fixed");
        }
    }
    assert!(rig.rest_at(Joint::WingL1)[1] > rig.rest_at(Joint::WingL0)[1], "the wrist is above the root");
}

/// A point of the rest body as `pose` carries it with the chest, taken back into the rest frame:
/// what is fixed to the chest stays where it was.
fn in_chest_frame(rig: &Rig, pose: &RigPose, p: [f32; 3]) -> [f32; 3] {
    let w = rig.world(pose)[Joint::Chest.index()];
    let d = vec3::sub(p, w.origin);
    let c = w.rotation.cols;
    vec3::add(rig.rest_at(Joint::Chest), [vec3::dot(c[0], d), vec3::dot(c[1], d), vec3::dot(c[2], d)])
}

fn wing_stalk(body: &Body) -> [f32; 3] {
    body.wing(Side::Left, 0.0).unwrap().root_skin
}

#[test]
fn animated_wings_never_swing_into_the_back() {
    for folk in [Folk::WingedDemon, Folk::Succubus] {
        for seed in [7, 40] {
            let body = demon(folk, seed);
            let rig = Rig::of(&body);
            let gait = Gait::new(&body, &rig);
            let skinner = Skinner::new(&body);
            let skin = body.skin_mesh();
            let stalk = wing_stalk(&body);
            let mut poses: Vec<RigPose> = (0..8).map(|i| gait.walk(i as f32 / 8.0, gait.natural_speed() * 1.3)).collect();
            poses.extend([0.0f32, 0.7, 1.9, 3.1].map(|t| gait.idle(&IdleCycle::of(3), t)));
            let mut checked = 0usize;
            for name in ["wing_bones", "wing_membrane"] {
                let p = part(&skin, name).unwrap();
                let w = skinner.part(p, SkinRegion::of_skin(name));
                for (pi, pose) in poses.iter().enumerate() {
                    let frames = rig.skin_frames(pose);
                    for (k, &q) in p.positions.iter().enumerate() {
                        // What hangs from the chest and the wings, above the hips.
                        if q[1] < body.chest_y - 0.5 || vec3::distance(q, [stalk[0].abs() * q[0].signum(), stalk[1], stalk[2]]) < 1.0 {
                            continue;
                        }
                        let on_chest: f32 = w.joints[k].iter().zip(w.weights[k]).filter(|(j, _)| matches!(**j as usize, 2 | 16..=23)).map(|(_, x)| x).sum();
                        if on_chest < 0.99 {
                            continue;
                        }
                        let local = in_chest_frame(&rig, pose, Rig::skin_point(&frames, q, w.joints[k], w.weights[k]));
                        let floor = body.back_floor(local[1]);
                        checked += 1;
                        assert!(local[2] < floor + 0.06, "{folk:?} {seed} {name} pose {pi}: a point at {local:?} swings in front of the back's {floor}");
                    }
                }
            }
            assert!(checked > 1000, "{folk:?}: {checked} points checked");
        }
    }
}

#[test]
fn a_cloaked_demons_posed_wings_stay_outside_the_gear() {
    for folk in [Folk::WingedDemon, Folk::Succubus] {
        for outfit_seed in [10u32, 12] {
            let body = demon(folk, 3);
            let worn: Vec<(Slot, Item)> = [(Slot::Body, Family::Garment), (Slot::Cloak, Family::Garment), (Slot::Back, Family::Bags)].iter().filter_map(|&(slot, family)| Item::roll(family, outfit_seed).map(|i| (slot, i))).collect();
            let fit = fit_worn(&body, &worn);
            assert!(fit.wing_lift > 0.0, "{folk:?}: gear on the back lifts the wings");
            let rig = Rig::of_lifted(&body, fit.wing_lift);
            let bare = Rig::of(&body);
            assert!(rig.rest_at(Joint::WingL0)[2] < bare.rest_at(Joint::WingL0)[2] - 0.3, "the wing pivots where it stands");
            let gait = Gait::new(&body, &rig);
            let skinner = Skinner::with_wing_lift(&body, fit.wing_lift);
            let lifted = body.skin_mesh_lifted(fit.wing_lift);
            let membrane = part(&lifted, "wing_membrane").unwrap();
            let wm = skinner.part(membrane, SkinRegion::Wing);
            let gear: Vec<(armory::Part, crate::skin::PartSkin)> =
                fit.layers.iter().flat_map(|l| l.mesh.parts.iter().map(move |p| (p.clone(), l.slot))).map(|(p, slot)| { let w = skinner.part(&p, SkinRegion::of_worn(slot, p.name)); (p, w) }).collect();
            for (pi, phase) in [0.0f32, 0.2, 0.45, 0.7].into_iter().enumerate() {
                let pose = gait.walk(phase, gait.natural_speed());
                let frames = rig.skin_frames(&pose);
                let back = |p: &armory::Part, w: &crate::skin::PartSkin, f: &dyn Fn(&[f32; 3]) -> bool| -> Vec<[f32; 3]> {
                    p.positions.iter().enumerate().filter(|(_, q)| f(q)).map(|(k, &q)| in_chest_frame(&rig, &pose, Rig::skin_point(&frames, q, w.joints[k], w.weights[k]))).collect()
                };
                // Cloth on the back above the waist, where it hangs from the shoulders and moves with the chest.
                let band = |q: &[f32; 3]| q[1] > body.chest_y && q[1] < body.chin_y && q[0].abs() < 0.5 * body.shoulder_half_width;
                let gear_back = gear.iter().flat_map(|(p, w)| back(p, w, &band)).map(|q| q[2]).fold(0.0f32, f32::min);
                let wing_front = back(membrane, &wm, &|q| q[1] > body.chest_y && q[1] < body.chin_y && q[0].abs() < 2.5 * body.shoulder_half_width).into_iter().map(|q| q[2]).fold(f32::MIN, f32::max);
                assert!(wing_front < gear_back + 0.1, "{folk:?} outfit {outfit_seed} phase {pi}: the wing's front {wing_front} is inside the gear's back {gear_back}");
            }
        }
    }
}

#[test]
fn the_wings_clear_collars_pauldrons_and_a_hoods_cape() {
    let mut seen = 0;
    for folk in [Folk::WingedDemon, Folk::Succubus] {
        for seed in [10u32, 11, 12, 13, 14, 15] {
            let body = demon(folk, 3);
            for kit in [vec![(Slot::Head, Family::Hood), (Slot::Cloak, Family::Garment)], vec![(Slot::Body, Family::Garment), (Slot::Neck, Family::Amulet)]] {
                let worn: Vec<(Slot, Item)> = kit.iter().filter_map(|&(slot, family)| Item::roll(family, seed).map(|i| (slot, i))).collect();
                let fit = fit_worn(&body, &worn);
                let lifted = body.skin_mesh_lifted(fit.wing_lift);
                let high = |p: &&[f32; 3]| p[1] > body.shoulder_y && p[1] < body.chin_y && p[0].abs() < 0.8 * body.shoulder_half_width && p[2] < 0.0;
                let gear_back = fit.layers.iter().flat_map(|l| l.mesh.parts.iter().flat_map(|p| p.positions.iter())).filter(high).map(|p| p[2]).fold(0.0f32, f32::min);
                let membrane = part(&lifted, "wing_membrane").unwrap();
                let wing_front = membrane.positions.iter().filter(|p| p[1] > body.shoulder_y && p[1] < body.chin_y && p[0].abs() < 2.5 * body.shoulder_half_width).map(|p| p[2]).fold(f32::MIN, f32::max);
                if gear_back < -0.1 {
                    seen += 1;
                }
                assert!(wing_front < gear_back.min(body.back_floor(body.shoulder_y)) + 0.02, "{folk:?} {seed}: the wing ({wing_front}) is in gear behind the neck ({gear_back})");
            }
        }
    }
    assert!(seen > 6, "gear was measured behind the neck {seen} times");
}

#[test]
fn the_spade_is_skinned_to_the_tail_and_swings_with_it() {
    for folk in DEMONS {
        let body = demon(folk, 7);
        let rig = Rig::of(&body);
        assert_eq!(body.tail_joints().len(), folk.race().tail_shape.unwrap().bones);
        let skinner = Skinner::new(&body);
        let skin = body.skin_mesh();
        let spade = part(&skin, "tail_spade").expect("a spade");
        let w = skinner.part(spade, SkinRegion::of_skin("tail_spade"));
        let last = Joint::TAIL[body.tail_joints().len() - 1];
        let heavy = w.joints.iter().zip(&w.weights).filter(|(js, ws)| js.iter().zip(*ws).any(|(&j, &x)| j as usize == last.index() && x > 0.5)).count();
        assert!(heavy * 2 > w.joints.len(), "{folk:?}: the spade rides the last bone");
        let gait = Gait::new(&body, &rig);
        let tip = |phase: f32| {
            let frames = rig.skin_frames(&gait.walk(phase, gait.natural_speed()));
            let k = (0..spade.positions.len()).min_by(|&a, &b| spade.positions[a][2].total_cmp(&spade.positions[b][2])).unwrap();
            Rig::skin_point(&frames, spade.positions[k], w.joints[k], w.weights[k])[0]
        };
        assert!((tip(0.0) - tip(0.25)).abs() > 0.1, "{folk:?}: the tail swings");
        // The blade is wider than the cord and ends well behind the body.
        let (lo, hi) = spade.positions.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(l, h), p| (std::array::from_fn(|i| l[i].min(p[i])), std::array::from_fn(|i| h[i].max(p[i]))));
        assert!(hi[0] - lo[0] > 3.0 * body.race().tail_shape.unwrap().radius * body.leg(Side::Left).radius * 0.5, "{folk:?}: a blade");
    }
}

#[test]
fn claws_are_longer_than_nails() {
    let claws = demon(Folk::WingedDemon, 7).skin_mesh();
    let nails = Body::new(&BodySpec::roll(Folk::Human, 7)).skin_mesh();
    // The longest nail or claw: the farthest a vertex of the part is from the wrist along the hand.
    let len = |m: &armory::ItemMesh, body: &Body| {
        let wrist = body.arm(Side::Left).joints[2];
        let p = part(m, "nails").unwrap();
        p.positions.iter().filter(|q| q[0] > 0.0).map(|q| vec3::distance(*q, wrist)).fold(0.0f32, f32::max) / body.hand_form().palm_length
    };
    let (demon_body, human) = (demon(Folk::WingedDemon, 7), Body::new(&BodySpec::roll(Folk::Human, 7)));
    let (c, n) = (len(&claws, &demon_body), len(&nails, &human));
    assert!(c > n * 1.08, "claws reach {c} palms from the wrist, a human's nails {n}");
    assert!(part(&demon(Folk::Succubus, 7).skin_mesh(), "nails").is_some());
    // A fist closed on a grip keeps short claws, so they do not poke into the haft: the claws' surface
    // (which grows with their length) is a good deal smaller than on a relaxed hand.
    let area = |m: &armory::geom::MeshBuilder| -> f32 {
        m.indices.chunks(3).map(|t| vec3::length(vec3::cross(vec3::sub(m.positions[t[1] as usize], m.positions[t[0] as usize]), vec3::sub(m.positions[t[2] as usize], m.positions[t[0] as usize])))).sum()
    };
    let form = demon_body.hand_form();
    let open = area(&crate::hand::hand_meshes_with(&form, &crate::hand::HandPose::RELAXED, true).1);
    let fist = area(&crate::hand::hand_meshes_with(&form, &crate::hand::HandPose::gripping(&form, 0.1), true).1);
    let plain = area(&crate::hand::hand_meshes_with(&form, &crate::hand::HandPose::RELAXED, false).1);
    assert!(open > 1.5 * plain, "claws are larger than nails ({open} against {plain})");
    assert!(fist < 0.6 * open, "claws shorten on a gripping hand: {fist} against {open}");
}

#[test]
fn hooves_take_no_boots_but_a_succubus_wears_them() {
    for folk in [Folk::WingedDemon, Folk::HornedDemon] {
        let body = demon(folk, 7);
        let fit = fit_worn(&body, &[(Slot::Feet, Item::roll(Family::Feet, 3).unwrap()), (Slot::Legs, Item::roll(Family::Legs, 3).unwrap())]);
        assert!(fit.layer(Slot::Feet).is_none() && !fit.covered.feet, "{folk:?}");
        let low = fit.layer(Slot::Legs).unwrap().mesh.parts.iter().flat_map(|p| &p.positions).map(|p| p[1]).fold(f32::MAX, f32::min);
        assert!(low > 0.5 * body.ankle_y, "{folk:?}: trousers end at the hock");
    }
    let body = demon(Folk::Succubus, 7);
    let fit = fit_worn(&body, &[(Slot::Feet, Item::roll(Family::Feet, 3).unwrap())]);
    assert!(fit.layer(Slot::Feet).is_some(), "a succubus's feet take boots");
}

#[test]
fn gear_clears_the_skin_and_the_wings_stand_over_it() {
    let mut measured = 0usize;
    for folk in DEMONS {
        for seed in [3, 1207] {
            let body = demon(folk, seed);
            let trunk = body.trunk();
            for outfit_seed in [10u32, 11, 12, 13] {
                let worn: Vec<(Slot, Item)> =
                    [(Slot::Shirt, Family::Garment), (Slot::Body, Family::Garment), (Slot::Cloak, Family::Garment), (Slot::Head, Family::Hood)].iter().filter_map(|&(slot, family)| Item::roll(family, outfit_seed).map(|i| (slot, i))).collect();
                let fit = fit_worn(&body, &worn);
                for layer in &fit.layers {
                    let (mut inside, mut total) = (0usize, 0usize);
                    for p in layer.mesh.parts.iter().filter(|p| p.name == "body").flat_map(|p| &p.positions) {
                        if p[1] < body.hip_y || p[1] > body.shoulder_y || p[0].abs() > body.shoulder_half_width * 0.7 {
                            continue;
                        }
                        total += 1;
                        let (hw, hd) = trunk.at(p[1]);
                        if ((p[0] / hw).powi(2) + (p[2] / hd).powi(2)).sqrt() < 0.9 {
                            inside += 1;
                        }
                    }
                    measured += total;
                    assert!(inside * 50 <= total, "{folk:?} {seed} outfit {outfit_seed} {:?}: {inside} of {total} points inside the trunk", layer.slot);
                }
                // The wings stand out past everything on the back.
                let lifted = body.skin_mesh_lifted(fit.wing_lift);
                if body.race().wings.is_none() {
                    assert_eq!(fit.wing_lift, 0.0);
                    continue;
                }
                let gear_back = fit.layers.iter().flat_map(|l| l.mesh.parts.iter().flat_map(|p| p.positions.iter())).filter(|p| p[1] > body.hip_y && p[1] < body.chin_y && p[0].abs() < 0.5 * body.shoulder_half_width).map(|p| p[2]).fold(0.0f32, f32::min);
                let edge = part(&lifted, "wing_membrane").unwrap().positions.iter().filter(|p| p[1] > body.hip_y + 1.0 && p[1] < body.chin_y && p[0].abs() < 2.5 * body.shoulder_half_width).map(|p| p[2]).fold(f32::MIN, f32::max);
                assert!(edge < gear_back + 0.02, "{folk:?} {seed} outfit {outfit_seed}: the membrane ({edge}) reaches into the gear on the back ({gear_back})");
            }
        }
    }
    assert!(measured > 0);
}

#[test]
fn a_weapon_sits_in_a_demons_fist() {
    for folk in DEMONS {
        let body = demon(folk, 7);
        let item = Item::roll(Family::Sword, 4).unwrap();
        let fit = fit_held(&body, Side::Right, &item).expect("a sword is held");
        let posed = Body::posed(&body.spec, fit.pose);
        let grip = posed.grip(Side::Right);
        let palm = posed.hand_form().palm_length;
        let d = vec3::distance(grip.origin, posed.arm(Side::Right).joints[2]);
        assert!(d > 0.5 * palm && d < 1.6 * palm, "{folk:?}: the grip is {d} from the wrist, palm {palm}");
        let along = grip.y();
        let closest = fit.mesh.parts.iter().flat_map(|p| &p.positions).map(|&p| {
            let v = vec3::sub(p, grip.origin);
            vec3::length(vec3::sub(v, vec3::scale(along, vec3::dot(v, along))))
        }).fold(f32::MAX, f32::min);
        assert!(closest < 0.5 * palm, "{folk:?}: the handle is {closest} from the grip's axis");
    }
}
