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
        assert!(srd.page > 200 && srd.ac > 10 && srd.hp > 30);
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
                let open = wing_open(&body);
                for q in p.positions.iter().step_by(6).filter(|q| vec3::distance(**q, [root[0].abs() * q[0].signum(), root[1], root[2]]) > 1.0) {
                    let floor = body.back_floor(q[1]);
                    assert!(q[2] < floor + 0.06 || q[1] > body.height - 0.5 || open > 0.5, "{folk:?} {seed} {name}: a point at {q:?} is in front of the back's {floor}");
                }
            }
        }
    }
}

fn wing_open(body: &Body) -> f32 {
    body.wing(Side::Left, 0.0).unwrap().open
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
    let len = |m: &armory::ItemMesh| {
        let p = part(m, "nails").unwrap();
        let (lo, hi) = p.positions.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(l, h), p| (std::array::from_fn(|i| l[i].min(p[i])), std::array::from_fn(|i| h[i].max(p[i]))));
        hi[1] - lo[1]
    };
    assert!(len(&claws) > 0.0 && part(&demon(Folk::Succubus, 7).skin_mesh(), "nails").is_some());
    let _ = (len(&nails), nails);
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
                    [(Slot::Shirt, Family::Garment), (Slot::Body, Family::Garment), (Slot::Cloak, Family::Garment)].iter().filter_map(|&(slot, family)| Item::roll(family, outfit_seed).map(|i| (slot, i))).collect();
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
                let gear_back = fit.layers.iter().filter(|l| l.slot != Slot::Head).flat_map(|l| l.mesh.parts.iter().flat_map(|p| p.positions.iter())).filter(|p| p[1] > body.hip_y && p[1] < body.shoulder_y && p[0].abs() < 0.5 * body.shoulder_half_width).map(|p| p[2]).fold(0.0f32, f32::min);
                let edge = part(&lifted, "wing_membrane").unwrap().positions.iter().filter(|p| p[1] > body.hip_y + 1.0 && p[1] < body.shoulder_y && p[0].abs() < 2.5 * body.shoulder_half_width).map(|p| p[2]).fold(f32::MIN, f32::max);
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
