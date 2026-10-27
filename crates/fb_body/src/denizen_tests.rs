//! The denizen parts and the minotaur built on them: the same body, rig, gait and fitting the
//! folk have, with horns, hooves, a tail and a muzzle turned on by the race row.

use armory::geom::vec3;
use armory::{Family, Item};
use fb_items::Slot;

use crate::body::Body;
use crate::folk::Folk;
use crate::gait::{Gait, IdleCycle};
use crate::held::fit_held;
use crate::limb::Side;
use crate::parts::Marking;
use crate::rig::{Joint, Rig, RigPose};
use crate::skin::{SkinRegion, Skinner};
use crate::spec::BodySpec;
use crate::worn::fit_worn;

fn minotaur(seed: u32) -> Body {
    Body::new(&BodySpec::roll(Folk::Minotaur, seed))
}

fn roll(family: Family, seed: u32) -> Item {
    Item::roll(family, seed).unwrap()
}

fn min_y(mesh: &armory::ItemMesh, part: &str) -> f32 {
    mesh.parts.iter().filter(|p| p.name == part).flat_map(|p| &p.positions).map(|p| p[1]).fold(f32::MAX, f32::min)
}

#[test]
fn the_same_seed_builds_the_same_minotaur_and_seeds_differ() {
    for seed in [1, 7] {
        assert_eq!(minotaur(seed).mesh(), minotaur(seed).mesh());
    }
    assert_ne!(minotaur(1).mesh(), minotaur(2).mesh());
    assert_eq!(Folk::of_denizen(Folk::Minotaur.denizen_body().unwrap()), Some(Folk::Minotaur));
    assert!(Folk::ALL.iter().all(|f| !f.is_denizen()) && Folk::DENIZENS.iter().all(|f| f.is_denizen()));
}

fn height_and_hooves_check(seeds: std::ops::Range<u32>) {
    let human = Body::new(&BodySpec::typical(Folk::Human)).height;
    for seed in seeds {
        let body = minotaur(seed);
        let k = body.height / human;
        assert!((1.25..1.5).contains(&k), "seed {seed}: {k} times a human");
        let skin = body.skin_mesh();
        assert!(min_y(&skin, "hooves").abs() < 0.05, "seed {seed}: hooves at {}", min_y(&skin, "hooves"));
        for part in body.mesh().parts.iter() {
            part.validate().unwrap_or_else(|e| panic!("seed {seed} {}: {e}", part.name));
        }
        let names: Vec<_> = skin.parts.iter().map(|p| p.name).collect();
        assert!(names.contains(&"hooves") && names.contains(&"tail") && names.contains(&"tail_hair"), "{names:?}");
    }
}

#[test]
fn a_minotaur_is_a_third_again_a_mans_height_with_hooves_on_the_floor_smoke() {
    height_and_hooves_check(0..4);
}

#[test]
#[ignore = "slow sweep: twenty seeds; run before merging fitting changes"]
fn a_minotaur_is_a_third_again_a_mans_height_with_hooves_on_the_floor() {
    height_and_hooves_check(0..20);
}

#[test]
fn the_legs_stand_on_their_toes_with_the_knee_forward_and_the_hock_high() {
    let body = minotaur(7);
    for side in Side::BOTH {
        let [hip, knee, hock] = body.leg(side).joints;
        assert!(knee[2] > hip[2] + 1.0, "the knee stands in front of the hip");
        assert!(hock[1] > 0.25 * hip[1] && hock[1] < 0.4 * hip[1], "the hock is high: {hock:?}");
        let foot = body.hooved_foot(side).unwrap();
        assert!(foot.coronet[2] > hock[2] + 0.5, "the pastern slopes forward");
    }
}

#[test]
fn horns_vary_by_seed_and_a_horn_may_be_broken() {
    let row = Folk::Minotaur.race().horns.clone().unwrap();
    let tr: Vec<_> = (0..200).map(crate::parts::Traits::roll).collect();
    let specs: Vec<_> = tr.iter().map(|t| t.horn_spec(&row, 0, 2.0)).collect();
    let (short, long) = specs.iter().fold((f32::MAX, 0.0f32), |(a, b), s| (a.min(s.length), b.max(s.length)));
    assert!(long > short * 1.5, "{short} {long}");
    let broken = specs.iter().filter(|s| s.broken.is_some()).count();
    assert!(broken > 0 && broken < 60, "{broken} of the right horns broken");
    assert!([Marking::Blaze, Marking::Socks, Marking::Pied, Marking::None].iter().all(|m| tr.iter().any(|t| t.marking == *m)));
    // Another race row with none has no horns, hooves, tail or muzzle.
    let human = Body::new(&BodySpec::roll(Folk::Human, 3));
    assert!(human.head_mesh().parts.iter().all(|p| p.name != "horns") && human.tail().is_none() && human.hooved_foot(Side::Left).is_none());
}

#[test]
fn socks_are_on_the_lower_legs_only() {
    let seed = (0..200).find(|&s| crate::parts::Traits::roll(s).marking == Marking::Socks).unwrap();
    let body = minotaur(seed);
    let socks = body.skin_mesh();
    let socks = socks.parts.iter().find(|p| p.name == "socks").expect("socks");
    let top = socks.positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
    assert!(top < body.leg(Side::Left).joints[1][1], "socks end well below the knee: {top}");
}

#[test]
fn the_tail_is_skinned_to_its_bones_and_swings_in_the_walk() {
    let body = minotaur(7);
    let rig = Rig::of(&body);
    assert!(body.tail_joints().len() == 3 && rig.rest_at(Joint::Tail2)[1] < rig.rest_at(Joint::Tail0)[1]);
    let skinner = Skinner::new(&body);
    let skin = body.skin_mesh();
    let tail = skin.parts.iter().find(|p| p.name == "tail").unwrap();
    let weights = skinner.part(tail, SkinRegion::of_skin("tail"));
    let on = |j: Joint| weights.joints.iter().zip(&weights.weights).any(|(js, ws)| js.iter().zip(ws).any(|(&k, &w)| k as usize == j.index() && w > 0.5));
    assert!(on(Joint::Tail0) && on(Joint::Tail1) && on(Joint::Tail2), "each bone carries part of the tail");
    let gait = Gait::new(&body, &rig);
    let tip = |phase: f32| {
        let pose = gait.walk(phase, gait.natural_speed());
        rig.world(&pose)[Joint::Tail2.index()].point([0.0, -1.0, 0.0])[0]
    };
    assert!((tip(0.0) - tip(0.25)).abs() > 0.1, "the tail swings");
    // A body with no tail leaves its tail joints alone.
    let human = Body::new(&BodySpec::roll(Folk::Human, 3));
    let (hrig, hgait) = {
        let r = Rig::of(&human);
        let g = Gait::new(&human, &r);
        (r, g)
    };
    let pose = hgait.walk(0.3, hgait.natural_speed());
    assert!(Joint::TAIL.iter().all(|j| pose.rotation[j.index()] == RigPose::REST.rotation[j.index()]) && hrig.rest_at(Joint::Tail0) == hrig.rest_at(Joint::Pelvis));
}

#[test]
fn the_hooves_stay_planted_in_the_walk_and_never_sink() {
    for seed in [1, 7] {
        let body = minotaur(seed);
        let rig = Rig::of(&body);
        let gait = Gait::new(&body, &rig);
        let skinner = Skinner::new(&body);
        let hooves = body.skin_mesh().parts.into_iter().find(|p| p.name == "hooves").unwrap();
        let weights = skinner.part(&hooves, SkinRegion::of_skin("hooves"));
        for i in 0..16 {
            let phase = i as f32 / 16.0;
            let pose = gait.pose(phase, gait.natural_speed(), &IdleCycle::of(5), 0.0);
            let frames = rig.skin_frames(&pose);
            let low = |side: Side| {
                hooves
                    .positions
                    .iter()
                    .enumerate()
                    .filter(|(k, p)| (p[0] > 0.0) == (side == Side::Left) && p[1] < 0.2 && weights.weights[*k][0] > 0.99)
                    .map(|(k, &p)| Rig::skin_point(&frames, p, weights.joints[k], weights.weights[k])[1])
                    .fold(f32::MAX, f32::min)
            };
            for side in Side::BOTH {
                let y = low(side);
                assert!(y > -0.08, "seed {seed} phase {phase}: a hoof sinks to {y}");
                if gait.in_stance(phase, side) {
                    assert!(y.abs() < 0.08, "seed {seed} phase {phase}: the stance hoof is at {y}");
                }
            }
        }
    }
}

#[test]
fn horns_come_out_through_headgear_and_the_muzzle_lifts_the_rim() {
    for seed in [1, 7, 40] {
        let body = minotaur(seed);
        let features = body.standing_features(Default::default());
        assert_eq!(features.horns.len(), 2, "both horns come out through headgear");
        assert!(features.face.is_some(), "the muzzle lifts the rim");
    }
}

#[test]
fn boots_are_skipped_for_hooves_and_trousers_end_at_the_hock() {
    let body = minotaur(7);
    let boots = roll(Family::Feet, 3);
    let legs = roll(Family::Legs, 3);
    let fit = fit_worn(&body, &[(Slot::Feet, boots), (Slot::Legs, legs)]);
    assert!(fit.layer(Slot::Feet).is_none(), "hooves take no boots");
    assert!(!fit.covered.feet, "the hooves stay bare");
    let legs = fit.layer(Slot::Legs).expect("trousers");
    let names: Vec<_> = legs.mesh.parts.iter().map(|p| (p.name, p.positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min))).collect();
    let low = legs.mesh.parts.iter().filter(|p| p.name == "legs").flat_map(|p| &p.positions).map(|p| p[1]).fold(f32::MAX, f32::min);
    assert!(low > 0.5 * body.ankle_y, "trousers hang to the hock, not the floor: {low} vs {} {names:?}", body.ankle_y);
}

fn grip_check(seeds: std::ops::Range<u32>) {
    let body = minotaur(7);
    let best = seeds
        .filter_map(|s| {
            let item = Item::roll(Family::Axe, s)?;
            let fit = fit_held(&body, Side::Right, &item)?;
            let (lo, hi) = fit.mesh.bounds()?;
            Some((hi[1] - lo[1], fit))
        })
        .fold(None::<(f32, crate::held::HeldFit)>, |a, b| match a {
            Some(a) if a.0 >= b.0 => Some(a),
            _ => Some(b),
        })
        .expect("an axe");
    let posed = Body::posed(&body.spec, best.1.pose);
    let grip = posed.grip(Side::Right);
    let wrist = posed.arm(Side::Right).joints[2];
    let palm = posed.hand_form().palm_length;
    let d = vec3::distance(grip.origin, wrist);
    assert!(d > 0.5 * palm && d < 1.6 * palm, "the grip is {d} from the wrist, palm {palm}");
    // The haft passes through the fist: the closest any vertex lies to the grip's axis (the line
    // through the grip along its `y`) is within a palm's width, so it is the haft and not a head
    // that happens to lie near the hand.
    let along = grip.y();
    let to_axis = |p: [f32; 3]| {
        let v = vec3::sub(p, grip.origin);
        vec3::length(vec3::sub(v, vec3::scale(along, vec3::dot(v, along))))
    };
    let closest = best.1.mesh.parts.iter().flat_map(|p| &p.positions).map(|&p| to_axis(p)).fold(f32::MAX, f32::min);
    assert!(closest < 0.5 * palm, "the haft is {closest} from the grip's axis, palm {palm}");
}

#[test]
fn a_two_handed_weapon_sits_in_the_hands_grip_smoke() {
    grip_check(0..12);
}

#[test]
#[ignore = "slow sweep: three hundred axes; run before merging fitting changes"]
fn a_two_handed_weapon_sits_in_the_hands_grip() {
    grip_check(0..300);
}

#[test]
fn every_outfit_fits_clear_of_the_skin() {
    use crate::worn::LAYER_ORDER;
    let mut measured = 0usize;
    for seed in [3, 1207] {
        let body = minotaur(seed);
        let trunk = body.trunk();
        for outfit_seed in [10u32, 11, 12, 13] {
            let worn: Vec<(Slot, Item)> = [(Slot::Shirt, Family::Garment), (Slot::Body, Family::Garment)].iter().filter_map(|&(slot, family)| Item::roll(family, outfit_seed).map(|i| (slot, i))).collect();
            let fit = fit_worn(&body, &worn);
            for layer in &fit.layers {
                assert!(LAYER_ORDER.contains(&layer.slot));
                // Cloth on the trunk stands outside the skin's section at every sampled point.
                let mut inside = 0usize;
                let mut total = 0usize;
                for p in layer.mesh.parts.iter().filter(|p| p.name == "body").flat_map(|p| &p.positions) {
                    if p[1] < body.hip_y || p[1] > body.shoulder_y || p[0].abs() > body.shoulder_half_width * 0.7 {
                        continue;
                    }
                    total += 1;
                    let (hw, hd) = trunk.at(p[1]);
                    let e = ((p[0] / hw).powi(2) + (p[2] / hd).powi(2)).sqrt();
                    if e < 0.9 {
                        inside += 1;
                    }
                }
                measured += total;
                assert!(inside * 50 <= total, "seed {seed} outfit {outfit_seed} {:?}: {inside} of {total} points inside the trunk", layer.slot);
            }
        }
    }
    assert!(measured > 0, "no cloth was measured");
}

// ── The faun ──

fn faun(seed: u32) -> Body {
    Body::new(&BodySpec::roll(Folk::Faun, seed))
}

#[test]
fn the_same_seed_builds_the_same_faun_and_it_is_a_denizen_of_the_folk_body() {
    for seed in [1, 7, 1207] {
        assert_eq!(faun(seed).mesh(), faun(seed).mesh());
    }
    assert_ne!(faun(1).mesh(), faun(2).mesh());
    assert_eq!(Folk::of_denizen(Folk::Faun.denizen_body().unwrap()), Some(Folk::Faun));
    assert!(Folk::Faun.is_denizen() && Folk::parse("faun") == Some(Folk::Faun));
    // The SRD satyr (p. 344): Str 12, Dex 16, Con 11, Int 12, Wis 10.
    assert_eq!(Folk::Faun.race().stats, [12, 16, 11, 12, 10]);
}

#[test]
fn a_faun_is_a_mans_height_with_hooves_on_the_floor_and_every_part_valid() {
    let human = Body::new(&BodySpec::typical(Folk::Human)).height;
    for seed in 0..12 {
        let body = faun(seed);
        let k = body.height / human;
        assert!((0.93..1.07).contains(&k), "seed {seed}: {k} times a human");
        let skin = body.skin_mesh();
        assert!(min_y(&skin, "hooves").abs() < 0.05, "seed {seed}: hooves at {}", min_y(&skin, "hooves"));
        for part in body.mesh().parts.iter() {
            part.validate().unwrap_or_else(|e| panic!("seed {seed} {}: {e}", part.name));
        }
        let names: Vec<_> = skin.parts.iter().map(|p| p.name).collect();
        assert!(names.contains(&"hooves") && names.contains(&"tail") && names.contains(&"leg_fur"), "{names:?}");
        assert!(body.head_mesh().parts.iter().any(|p| p.name == "horns"), "seed {seed}: horns");
    }
}

#[test]
fn a_fauns_horns_are_short_and_its_legs_are_furred_in_one_of_the_rows_colours() {
    let row = Folk::Faun.race().horns.clone().unwrap();
    let minotaur_row = Folk::Minotaur.race().horns.clone().unwrap();
    let lengths: Vec<f32> = (0..100).map(|s| faun(s).traits().horn_spec(&row, 0, 2.0).length).collect();
    assert!(lengths.iter().all(|l| *l <= row.length[1] * 2.0 + 1e-3) && row.length[1] * 1.5 < minotaur_row.length[1], "short horns");
    let fur = Folk::Faun.race().fur.clone().unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..60 {
        let body = faun(seed);
        let colour = body.fur_colour().unwrap();
        assert!(fur.colours.iter().any(|c| c.0 == colour));
        seen.insert(fur.colours.iter().position(|c| c.0 == colour).unwrap());
        // The skin is a human's, not the fur's.
        assert!(Folk::Faun.race().skins.iter().any(|c| c.0 == body.spec.skin));
        // No coat markings on a faun.
        assert!(body.skin_mesh().parts.iter().all(|p| p.name != "socks" && p.name != "markings"));
    }
    assert!(seen.len() >= 3, "{seen:?}");
    // Folk have no fur parts.
    assert!(Body::new(&BodySpec::roll(Folk::Human, 3)).skin_mesh().parts.iter().all(|p| p.name != "leg_fur"));
}

#[test]
fn the_fur_lies_on_the_legs_from_the_hip_to_the_hock_and_no_higher() {
    let body = faun(7);
    let fur = body.skin_mesh().parts.into_iter().find(|p| p.name == "leg_fur").unwrap();
    let top = fur.positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
    assert!(top < body.hip_y + 0.1 * body.trunk_length && top > body.leg(Side::Left).joints[1][1], "fur tops out at {top}");
    let skinner = Skinner::new(&body);
    let w = skinner.part(&fur, SkinRegion::of_skin("leg_fur"));
    assert_eq!(w.joints.len(), fur.positions.len());
}

#[test]
fn a_fauns_hooves_stay_planted_in_the_walk() {
    for seed in [1, 7] {
        let body = faun(seed);
        let rig = Rig::of(&body);
        let gait = Gait::new(&body, &rig);
        let skinner = Skinner::new(&body);
        let hooves = body.skin_mesh().parts.into_iter().find(|p| p.name == "hooves").unwrap();
        let weights = skinner.part(&hooves, SkinRegion::of_skin("hooves"));
        for i in 0..16 {
            let phase = i as f32 / 16.0;
            let pose = gait.pose(phase, gait.natural_speed(), &IdleCycle::of(5), 0.0);
            let frames = rig.skin_frames(&pose);
            let low = |side: Side| {
                hooves
                    .positions
                    .iter()
                    .enumerate()
                    .filter(|(k, p)| (p[0] > 0.0) == (side == Side::Left) && p[1] < 0.2 && weights.weights[*k][0] > 0.99)
                    .map(|(k, &p)| Rig::skin_point(&frames, p, weights.joints[k], weights.weights[k])[1])
                    .fold(f32::MAX, f32::min)
            };
            for side in Side::BOTH {
                let y = low(side);
                assert!(y > -0.08, "seed {seed} phase {phase}: a hoof sinks to {y}");
                if gait.in_stance(phase, side) {
                    assert!(y.abs() < 0.08, "seed {seed} phase {phase}: the stance hoof is at {y}");
                }
            }
        }
    }
}

#[test]
fn a_faun_wears_no_boots_or_trousers_and_its_horns_come_out_through_headgear() {
    let body = faun(7);
    let fit = fit_worn(&body, &[(Slot::Feet, roll(Family::Feet, 3)), (Slot::Legs, roll(Family::Legs, 3))]);
    assert!(fit.layer(Slot::Feet).is_none() && fit.layer(Slot::Legs).is_none(), "furred hooved legs stay bare");
    assert!(!fit.covered.feet && !fit.covered.legs);
    for seed in [1, 7, 40] {
        let features = faun(seed).standing_features(Default::default());
        assert_eq!(features.horns.len(), 2, "both horns come out through headgear");
    }
}

#[test]
fn a_fauns_clothes_clear_its_skin_and_a_weapon_sits_in_its_palm() {
    for seed in [3, 1207] {
        let body = faun(seed);
        for outfit_seed in [10u32, 11, 12, 13] {
            let worn: Vec<(Slot, Item)> = [(Slot::Shirt, Family::Garment), (Slot::Body, Family::Garment), (Slot::Belt, Family::Belt), (Slot::Hands, Family::Hands), (Slot::Head, Family::Hood)]
                .iter()
                .filter_map(|&(slot, family)| Item::roll(family, outfit_seed).map(|i| (slot, i)))
                .collect();
            let fit = fit_worn(&body, &worn);
            assert!(fit.layers.len() >= 3, "seed {seed} outfit {outfit_seed}");
            let trunk = body.trunk();
            for layer in fit.layers.iter().filter(|l| matches!(l.slot, Slot::Shirt | Slot::Body)) {
                let mut inside = 0usize;
                let mut total = 0usize;
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
                assert!(inside * 50 <= total.max(50), "seed {seed} outfit {outfit_seed} {:?}: {inside} of {total} inside the trunk", layer.slot);
            }
        }
    }
    let body = faun(7);
    for family in [Family::Spear, Family::Sword] {
        let item = (0..40).filter_map(|s| Item::roll(family, s)).next().unwrap();
        let fit = fit_held(&body, Side::Right, &item).expect("held");
        let posed = Body::posed(&body.spec, fit.pose);
        let grip = posed.grip(Side::Right);
        let palm = posed.hand_form().palm_length;
        let along = grip.y();
        let to_axis = |p: [f32; 3]| {
            let v = vec3::sub(p, grip.origin);
            vec3::length(vec3::sub(v, vec3::scale(along, vec3::dot(v, along))))
        };
        let closest = fit.mesh.parts.iter().flat_map(|p| &p.positions).map(|&p| to_axis(p)).fold(f32::MAX, f32::min);
        assert!(closest < 0.5 * palm, "{family:?}: the haft is {closest} from the grip's axis, palm {palm}");
    }
}

#[test]
fn the_trunk_stoops_in_the_rest_pose_and_the_arms_still_hang() {
    let body = minotaur(7);
    let rig = Rig::of(&body);
    let gait = Gait::new(&body, &rig);
    let pose = gait.idle(&IdleCycle::of(1), 0.0);
    let world = rig.world(&pose);
    let head = Joint::Head.index();
    let (rest, bent) = (rig.rest_at(Joint::Head), world[head].origin);
    assert!(bent[2] > rest[2] + 1.0, "the neck's base leans forward: {rest:?} to {bent:?}");
    let (shoulder, wrist) = (world[Joint::ShoulderL.index()].origin, world[Joint::WristL.index()].origin);
    assert!((wrist[2] - shoulder[2]).abs() < 0.2 * (shoulder[1] - wrist[1]), "the arm hangs: {shoulder:?} {wrist:?}");
    // A folk stands as it did.
    let human = Body::new(&BodySpec::roll(Folk::Human, 3));
    let hrig = Rig::of(&human);
    let hpose = Gait::new(&human, &hrig).idle(&IdleCycle::of(1), 0.0);
    assert!((hrig.world(&hpose)[head].origin[2] - hrig.rest_at(Joint::Head)[2]).abs() < 0.3);
}

#[test]
fn under_a_robe_a_fauns_fur_goes_but_the_pastern_and_hoof_show_below_the_hem() {
    use armory::families::garment::Garment;
    use armory::sections::garment::GarmentStyle;
    for seed in [3, 7] {
        let body = faun(seed);
        for kit in [1u32, 40] {
            let robe = Item::Garment(Garment::roll_as(kit, GarmentStyle::Robe));
            let fit = fit_worn(&body, &[(Slot::Body, robe)]);
            assert!(fit.covered.hides("leg_fur") && fit.covered.hides("legs") && !fit.covered.hides("feet"), "the leg tube and fur go under the robe, the feet stay");
            let hem = fit.layer(Slot::Body).unwrap().mesh.parts.iter().filter(|p| p.name == "body").flat_map(|p| &p.positions).map(|p| p[1]).fold(f32::MAX, f32::min);
            let skin = body.skin_mesh();
            let pastern = skin.parts.iter().filter(|p| p.name == "feet").flat_map(|p| &p.positions).filter(|p| p[1] < hem - 0.05 && p[1] > 0.4).count();
            assert!(pastern > 8, "seed {seed} robe {kit}: {pastern} pastern vertices between the hem ({hem}) and the hoof");
        }
    }
    // Trousers alone leave the fur showing.
    let bare = fit_worn(&faun(3), &[(Slot::Legs, roll(Family::Legs, 3))]);
    assert!(!bare.covered.hides("leg_fur"));
}

#[test]
fn a_fauns_tail_and_tuft_take_the_fur_colour() {
    let body = faun(7);
    let fur = body.fur_colour().unwrap();
    let skin = body.skin_mesh();
    for name in ["tail", "tail_hair", "legs"] {
        let part = skin.parts.iter().find(|p| p.name == name).unwrap();
        assert_eq!(part.tint, Some(fur), "{name}");
    }
}

#[test]
fn curly_hair_builds_under_a_hood_and_a_helm() {
    use crate::folk::HairStyle;
    for seed in [3, 7] {
        let mut spec = BodySpec::roll(Folk::Faun, seed);
        spec.hair = HairStyle::Curly;
        let body = Body::new(&spec);
        let open = body.hair_mesh(Default::default());
        assert!(open.parts.iter().any(|p| p.name == "hair"));
        for family in [Family::Hood, Family::Helm] {
            let fit = fit_worn(&body, &[(Slot::Head, roll(family, 3))]);
            assert!(fit.hair.covered, "{family:?}");
            let covered = body.hair_mesh(fit.hair);
            for part in &covered.parts {
                part.validate().unwrap_or_else(|e| panic!("{family:?} {}: {e}", part.name));
            }
            let count = |m: &armory::ItemMesh| m.parts.iter().filter(|p| p.name == "hair").map(|p| p.positions.len()).sum::<usize>();
            assert!(count(&covered) > 0 && count(&covered) < count(&open), "{family:?}: fewer curls under it");
        }
    }
}

#[test]
fn a_minotaurs_belt_hangs_no_pouch_and_its_tail_is_a_thin_cord() {
    let belt = roll(Family::Belt, 3);
    let pouch = |folk: Folk| {
        let body = Body::new(&BodySpec::roll(folk, 5));
        let fit = fit_worn(&body, &[(Slot::Belt, belt.clone())]);
        fit.layer(Slot::Belt).unwrap().mesh.parts.iter().any(|p| p.name == "pouch")
    };
    assert!(!pouch(Folk::Minotaur));
    let body = minotaur(5);
    let (_, radius) = body.jointed_tail().unwrap();
    assert!(radius(0.0) < 0.25, "tail root radius {} units", radius(0.0));
}
