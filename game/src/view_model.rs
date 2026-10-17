//! Where what the delver holds is drawn in first person: the view model's placement, free of
//! the ECS so it can be tested. `crate::carry` spawns it on the view model's stage.
//!
//! Each hand's item is drawn as rolled, in its own proportions, `held_scale` times its size
//! ([`crate::tuning::ViewTuning`]), as its inventory icon and the floor show it. Only its turn in the fist comes from the doll
//! ([`fit_hands`]): a blade's edge leading, an axe's bit forward, a bow's string back, a shield
//! strapped along the forearm. The doll also thins a handle to the fist and draws blades,
//! bows and shields smaller, so they sit in its hand; no hand is drawn in first person, so
//! neither is done here. The item, turned as in the doll's fist, is carried into the view:
//! turned to face the way the eye does, its grip moved to where that kind of hold sits in the
//! view ([`view_hold`]) and turned a little more there, so it reads from behind the eye rather
//! than from the front of the doll. The main hand's is low on the right, the off hand's low on
//! the left ([`side_of`]).
//!
//! The view model camera sees through a narrower field of view than the world's
//! (`held_fov_degrees` in [`crate::tuning::ViewTuning`]), so what is held reads large while drawn
//! at its true size. The
//! placements per hold are our own tuning against captures of the level through it, not a rule
//! from a source: grips stay below the bottom edge (no hands are drawn yet), and nothing reaches
//! the middle of the view where the crosshair is ([`CLEAR_CENTRE`]).

use armory::Item;
use bevy::prelude::*;
use fb_body::{BodySpec, Hold, Side, fit_hands};
use fb_items::Slot;

/// The middle of the view that nothing held reaches: a circle about the crosshair, its radius
/// a fraction of the half height, where the player looks at what is ahead. Our own.
pub const CLEAR_CENTRE: f32 = 0.3;

/// The nearest a held thing comes to the eye, in view model units, well past the view model
/// camera's near plane (0.05) so nothing is ever cut by it.
pub const NEAREST: f32 = 3.0;

/// Where a kind of hold sits in the view, for the right hand: the fist's position in view model
/// camera space and the turn given to the doll's fitted item about it, after facing it the way
/// the eye looks. The left hand's is its mirror ([`ViewHold::mirrored`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewHold {
    pub fist: Vec3,
    pub turn: Quat,
}

impl ViewHold {
    /// The same hold in the other hand: mirrored across the view's middle.
    pub fn mirrored(self) -> ViewHold {
        let [x, y, z, w] = self.turn.to_array();
        ViewHold { fist: self.fist * Vec3::new(-1.0, 1.0, 1.0), turn: Quat::from_xyzw(x, -y, -z, w) }
    }
}

/// The hand a hand slot holds in: the main hand is the right, as on the doll
/// ([`fb_body::MAIN_HAND`]).
pub fn side_of(hand: Slot) -> Side {
    if hand == Slot::Offhand { fb_body::OFF_HAND } else { fb_body::MAIN_HAND }
}

/// A turn of `spin` about the view's up axis (for an item standing upright in the fist, about
/// its own length; positive turns its right side away), then `roll` about the view's forward
/// axis (positive leans the top to the left), then `tip` about its right axis (negative tips the
/// top away from the eye), then `yaw` about its up axis (positive turns the right side away).
fn turn(spin: f32, roll: f32, tip: f32, yaw: f32) -> Quat {
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(tip) * Quat::from_rotation_z(roll) * Quat::from_rotation_y(spin)
}

/// How far ahead of the eye a ranged weapon in view is aimed to cross the line of sight, view
/// units: the crosshair at about where shots land in a room.
pub const AIM_DISTANCE: f32 = 300.0;

/// The turn that points straight ahead (-Z) from `from` to the crosshair at [`AIM_DISTANCE`].
pub fn aimed_from(from: Vec3) -> Quat {
    Quat::from_rotation_arc(Vec3::NEG_Z, (Vec3::new(0.0, 0.0, -AIM_DISTANCE) - from).normalize())
}

/// Where `hold` sits in the view for the hand on `side`.
///
/// - One-handed weapons are held low on the right, standing out of the fist with the head or
///   point tipped away and leaning in, as a weapon is carried at the ready.
/// - Staves, spears and other long hafts lie low and angled to the right, running away from the
///   eye and clear of a held torch or lantern in the other hand, as a quarterstaff is carried
///   in both hands.
/// - A bow stands low on its side, canted in at the top.
/// - A shield stays strapped along the forearm as on the doll, low at the edge of the view, its
///   face out toward that edge and turned a little forward, so its back is seen edge-on.
/// - A torch stands low, leaning in and away, as the view model always held it; a lantern
///   hangs plumb.
/// - A crossbow is shouldered low on the right: its butt (not the grip) at `fist`, its stock
///   pointing at the crosshair (the turn is set in [`held_in_view`], not here).
pub fn view_hold(hold: Hold, side: Side) -> ViewHold {
    const SPIN: f32 = 0.7;
    let right = match hold {
        Hold::OneHanded => ViewHold { fist: Vec3::new(4.03, -5.55, -9.5), turn: turn(SPIN, -0.3, -0.3, 0.0) },
        Hold::Upright => ViewHold { fist: Vec3::new(4.3, -7.0, -11.5), turn: turn(0.0, -0.3, -0.5, 0.15) },
        Hold::Bow => ViewHold { fist: Vec3::new(3.61, -5.14, -11.0), turn: turn(0.0, 0.3, -0.1, 0.7) },
        Hold::Shield => ViewHold { fist: Vec3::new(6.25, -5.28, -11.0), turn: turn(0.0, 0.0, 0.0, 0.15) },
        Hold::Torch => ViewHold { fist: Vec3::new(5.0, -5.83, -9.6), turn: turn(0.0, 0.22, -0.32, 0.0) },
        Hold::Lantern => ViewHold { fist: Vec3::new(5.0, 0.56, -11.0), turn: Quat::IDENTITY },
        Hold::Crossbow => ViewHold { fist: Vec3::new(1.3, -2.3, -3.2), turn: Quat::IDENTITY },
    };
    if side == Side::Left { right.mirrored() } else { right }
}

/// Body space (soles on y = 0, the face toward +Z, the wearer's left +X) turned to face the way
/// the eye looks in view space (-Z ahead, +X to the right): half a turn about the vertical, so
/// the wearer's left stays on the left.
const BODY_TO_VIEW: Quat = Quat::from_xyzw(0.0, 1.0, 0.0, 0.0);

/// One hand's item placed in the view.
#[derive(Clone, Debug, PartialEq)]
pub struct HeldInView {
    pub hand: Slot,
    pub hold: Hold,
    /// The item as drawn: the one held, as rolled (not the doll's thinned [`fb_body::HeldFit::item`]).
    pub item: Item,
    /// From the item's own space into view model camera space, at `scale` times its size.
    pub transform: Transform,
    /// Its point, in its own space: the end of it farthest from the fist (a spear's head, a
    /// dagger's tip), which a thrust drives at the crosshair.
    pub point: Vec3,
    /// The grip point in item space, for scale-independent placement
    pub grip: Vec3,
    /// The fist position in view space, for scale-independent placement
    pub fist: Vec3,
}

/// The point of `item` held at `grip` (both in its own space): the vertex farthest from it.
pub fn point_of(item: &Item, grip: Vec3) -> Vec3 {
    item.build().parts.iter().flat_map(|p| p.positions.iter()).map(|&p| Vec3::from_array(p)).max_by(|a, b| a.distance_squared(grip).total_cmp(&b.distance_squared(grip))).unwrap_or(grip)
}

/// What each hand holds, placed in the view for a delver of body `spec` and drawn `scale` times
/// its size about the grip: the main hand's first, then the off hand's. A hand whose item is
/// not held (or that holds nothing) is left out.
pub fn held_in_view(spec: &BodySpec, main: Option<&Item>, off: Option<&Item>, scale: f32) -> Vec<HeldInView> {
    let hands = fit_hands(spec, main, off);
    [(Slot::Mainhand, main, hands.main), (Slot::Offhand, off, hands.off)]
        .into_iter()
        .filter_map(|(hand, item, fit)| {
            let (item, fit) = (item?, fit?);
            let side = side_of(hand);
            let [x, y, z] = fit.placement.rotation.cols.map(Vec3::from_array);
            let placed = Quat::from_mat3(&Mat3::from_cols(x, y, z));
            // The fist in the item's own space: the doll's grip, undone of its placement and
            // scale. Thinning narrows a handle about its axis, so the grip sits at the same
            // point on the item as rolled.
            let fist = Vec3::from_array(hands.body.grip(side).origin);
            let grip = match fit.hold {
                // Placed by its butt, at the shoulder.
                Hold::Crossbow => Vec3::ZERO,
                _ => placed.inverse() * (fist - Vec3::from_array(fit.placement.origin)) / fit.scale,
            };
            let at = view_hold(fit.hold, side);
            let rotation = match fit.hold {
                // Shouldered and aimed: the stock level, turned in toward the line of sight,
                // the deck up, whatever the doll's grip. Level rather than at the crosshair, so
                // the prod stays out of the middle of the view.
                Hold::Crossbow => aimed_from(at.fist * Vec3::new(1.0, 0.0, 1.0)) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
                _ => at.turn * BODY_TO_VIEW * placed,
            };
            let transform = Transform { translation: at.fist - rotation * grip * scale, rotation, scale: Vec3::splat(scale) };
            Some(HeldInView { hand, hold: fit.hold, item: item.clone(), transform, point: point_of(item, grip), grip, fist: at.fist })
        })
        .collect()
}

/// How far a lantern hung at the hip swings about its hook at walking pace, radians, as
/// `(side to side, fore and aft)`: with the stride's sway and each footfall ([`hip_swing`]). Our
/// own tuning.
pub const HIP_SWING: Vec2 = Vec2::new(0.14, 0.08);

/// A lantern hung at the hip, placed for first person. Unlike what the hands hold it is not on
/// the view model's stage but in the level, hung from the player's body, so it is seen at the
/// bottom edge of the view only when looking down, and its light rises from the hip.
#[derive(Clone, Debug, PartialEq)]
pub struct HipInView {
    /// The lantern as rolled.
    pub item: Item,
    /// Its hook in the player body's space: feet at y = 0, -Z ahead, +X to the right.
    pub hook: Vec3,
    /// From the lantern's own space into the hook's (the hook at the origin, unturned).
    pub transform: Transform,
}

/// The lantern `worn` hangs at its hip (a [`Slot::Hip`] item, hung from its [`Slot::Belt`]
/// over what else is worn: [`fb_body::fit_hip`]) for a delver of body `spec` whose eye is
/// `eye_height` above the feet, as on the doll: the body's eye is moved to the player's, so the
/// lantern hangs as far below the eye as the doll's hip is below its head. `None` with no
/// lantern at the hip, or no belt.
pub fn hip_in_view(spec: &BodySpec, worn: &[(Slot, Item)], eye_height: f32) -> Option<HipInView> {
    let item = worn.iter().find(|(slot, _)| *slot == Slot::Hip)?.1.clone();
    let body = fb_body::Body::new(spec);
    let fit = fb_body::fit_worn(&body, worn);
    let hip = fb_body::fit_hip(&body, &item, &fit.layer(Slot::Belt)?.mesh)?;
    let [x, y, z] = hip.placement.rotation.cols.map(Vec3::from_array);
    let placed = Quat::from_mat3(&Mat3::from_cols(x, y, z));
    let hook = Vec3::from_array(hip.hook);
    let transform = Transform { translation: BODY_TO_VIEW * (Vec3::from_array(hip.placement.origin) - hook), rotation: BODY_TO_VIEW * placed, scale: Vec3::splat(hip.scale) };
    Some(HipInView { item, hook: BODY_TO_VIEW * hook + Vec3::Y * (eye_height - body.head.centre[1]), transform })
}

/// The hip lantern's turn about its hook at the step's `bob` ([`crate::player::Gait::bob`],
/// `(sideways, down)`): swaying side to side with the stride and back at each footfall, still
/// when standing.
pub fn hip_swing(bob: Vec2) -> Quat {
    Quat::from_rotation_z(HIP_SWING.x * bob.x) * Quat::from_rotation_x(-HIP_SWING.y * bob.y)
}

/// Where a point in view model camera space lands on the screen through a vertical field of
/// view of `fov_degrees`, as a fraction of the half height from the centre (+Y up, +X right; at
/// the edges `y` is ±1 and `x` ± the aspect). `None` behind the eye.
pub fn on_screen(p: Vec3, fov_degrees: f32) -> Option<Vec2> {
    (p.z < 0.0).then(|| p.xy() / (-p.z * (fov_degrees.to_radians() * 0.5).tan()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use armory::Family;
    use fb_body::Folk;
    use crate::tuning::ViewTuning;

    /// The held scale and field of view as shipped.
    fn view() -> ViewTuning {
        ViewTuning::default()
    }

    fn held_in_view(spec: &BodySpec, main: Option<&Item>, off: Option<&Item>) -> Vec<HeldInView> {
        super::held_in_view(spec, main, off, view().held_scale)
    }

    fn on_screen(p: Vec3) -> Option<Vec2> {
        super::on_screen(p, view().held_fov_degrees)
    }

    fn roll(base: &str, seed: u32) -> Item {
        fb_items::roll_item(base, seed).unwrap().shape
    }

    /// Every vertex of a placed item in view space.
    fn points(held: &HeldInView) -> Vec<Vec3> {
        held.item.build().parts.iter().flat_map(|p| p.positions.clone()).map(|p| held.transform.transform_point(Vec3::from_array(p))).collect()
    }

    fn centroid(held: &HeldInView) -> Vec3 {
        let all = points(held);
        all.iter().sum::<Vec3>() / all.len() as f32
    }

    fn spec(folk: Folk) -> BodySpec {
        BodySpec::roll(folk, 1207)
    }

    #[test]
    fn the_main_hand_is_on_the_right_and_the_off_hand_on_the_left() {
        let (mace, torch) = (roll("Mace", 3), crate::carry::lit_torch(4).shape);
        let held = held_in_view(&spec(Folk::Human), Some(&mace), Some(&torch));
        assert_eq!(held.iter().map(|h| (h.hand, h.hold)).collect::<Vec<_>>(), vec![(Slot::Mainhand, Hold::OneHanded), (Slot::Offhand, Hold::Torch)]);
        assert!(centroid(&held[0]).x > 2.0, "the mace on the right: {}", centroid(&held[0]));
        assert!(centroid(&held[1]).x < -2.0, "the torch on the left: {}", centroid(&held[1]));
        // Each fist is where its hold puts it, the off hand's mirrored.
        assert_eq!(view_hold(Hold::Torch, Side::Left).fist.x, -view_hold(Hold::Torch, Side::Right).fist.x);
        assert_eq!(side_of(Slot::Mainhand), Side::Right);
        assert_eq!(side_of(Slot::Offhand), Side::Left);
    }

    #[test]
    fn every_weapon_family_in_the_off_hand_is_held_on_the_left() {
        let human = spec(Folk::Human);
        for base in ["Sword", "Dagger", "Mace", "Club", "Hand axe", "Spear", "Staff", "Bow", "Crossbow"] {
            let (sword, item) = (roll("Sword", 3), roll(base, 8));
            let held = held_in_view(&human, Some(&sword), Some(&item));
            let off = held.iter().find(|h| h.hand == Slot::Offhand).unwrap_or_else(|| panic!("{base} not held"));
            let main = held.iter().find(|h| h.hand == Slot::Mainhand).unwrap();
            assert!(view_hold(off.hold, Side::Left).fist.x < 0.0, "{base}");
            assert!(centroid(off).x < 0.0 && centroid(main).x > 0.0, "{base}: off {} main {}", centroid(off), centroid(main));
        }
    }

    #[test]
    fn a_hip_lantern_hangs_low_on_the_left_and_swings_only_when_walking() {
        use crate::play::EYE_HEIGHT;
        let lantern = roll("Lantern", 2);
        for folk in Folk::ALL {
            let worn = [(Slot::Belt, roll("Belt", 3)), (Slot::Hip, lantern.clone())];
            let hip = hip_in_view(&spec(folk), &worn, EYE_HEIGHT).unwrap_or_else(|| panic!("{folk:?}"));
            // On the left, a little ahead, well below the eye and above the feet.
            assert!(hip.hook.x < 0.0 && hip.hook.z < 0.0, "{folk:?}: {}", hip.hook);
            assert!(hip.hook.y > 0.0 && hip.hook.y < EYE_HEIGHT * 0.75, "{folk:?}: {}", hip.hook);
            // Every part hangs below the hook.
            for p in lantern.build().parts.iter().flat_map(|p| &p.positions) {
                assert!(hip.transform.transform_point(Vec3::from_array(*p)).y <= 0.05, "{folk:?}");
            }
        }
        assert!(hip_in_view(&spec(Folk::Human), &[(Slot::Hip, lantern.clone())], EYE_HEIGHT).is_none(), "no belt, no hip");
        assert_eq!(hip_swing(Vec2::ZERO), Quat::IDENTITY);
        assert!(hip_swing(Vec2::new(1.0, 0.5)).angle_between(Quat::IDENTITY) > 0.1);
    }

    #[test]
    fn a_mirrored_hold_is_a_reflection() {
        for hold in [Hold::OneHanded, Hold::Upright, Hold::Bow, Hold::Shield, Hold::Torch, Hold::Lantern, Hold::Crossbow] {
            let (right, left) = (view_hold(hold, Side::Right), view_hold(hold, Side::Left));
            let flip = Vec3::new(-1.0, 1.0, 1.0);
            for v in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(0.3, 0.8, -0.5)] {
                assert!((left.turn * (v * flip)).abs_diff_eq((right.turn * v) * flip, 1e-5), "{hold:?}");
            }
            assert_eq!(left.mirrored(), right);
        }
    }

    #[test]
    fn items_are_drawn_as_rolled_and_turned_as_on_the_doll() {
        for folk in [Folk::Human, Folk::Gnome] {
            let spec = spec(folk);
            for base in ["Sword", "Mace", "Shield", "Bow", "Staff"] {
                let item = roll(base, 7);
                let held = &held_in_view(&spec, Some(&item), None)[0];
                let fit = fb_body::fit_held(&fb_body::Body::new(&spec), Side::Right, &item).unwrap();
                assert_eq!(held.item, item, "{folk:?} {base}: the item as rolled, not thinned");
                assert_eq!(held.transform.scale, Vec3::splat(view().held_scale), "{folk:?} {base}: scaled evenly");
                // The doll's turn in the fist, then the hold's turn into the view.
                let rot = Quat::from_mat3(&Mat3::from_cols_array_2d(&fit.placement.rotation.cols));
                let at = view_hold(fit.hold, Side::Right);
                assert!(held.transform.rotation.abs_diff_eq(at.turn * BODY_TO_VIEW * rot, 1e-5), "{folk:?} {base}");
                // The grip sits at the hold's fist.
                let fist = Vec3::from_array(fb_body::fit_hands(&spec, Some(&item), None).body.grip(Side::Right).origin);
                let grip = rot.inverse() * (fist - Vec3::from_array(fit.placement.origin)) / fit.scale;
                assert!(held.transform.transform_point(grip).abs_diff_eq(at.fist, 1e-4), "{folk:?} {base}");
            }
        }
    }

    #[test]
    fn one_handed_weapons_stand_up_and_away_and_lean_in() {
        let human = spec(Folk::Human);
        for base in ["Mace", "Sword", "Hand axe", "Dagger", "Club"] {
            for hand in [Slot::Mainhand, Slot::Offhand] {
                let item = roll(base, 5);
                let held = if hand == Slot::Mainhand { held_in_view(&human, Some(&item), None) } else { held_in_view(&human, None, Some(&item)) };
                let Some(held) = held.first() else { panic!("{base} not held") };
                if held.hold != Hold::OneHanded {
                    continue;
                }
                let fist = view_hold(Hold::OneHanded, side_of(hand)).fist;
                let out = (centroid(held) - fist).normalize();
                assert!(out.y > 0.3, "{base}: up {out}");
                assert!(out.z < 0.0, "{base}: away {out}");
                assert!(out.x * fist.x < 0.0, "{base} {hand:?}: leans in {out}");
            }
        }
    }

    #[test]
    fn a_shield_shows_its_face_to_the_edge_of_the_view() {
        let shield = roll("Shield", 2);
        for folk in [Folk::Human, Folk::Dwarf, Folk::Gnome] {
            let held = &held_in_view(&spec(folk), None, Some(&shield))[0];
            assert_eq!(held.hold, Hold::Shield);
            // The face (item +Z) looks out to the left, not back at the eye.
            let face = held.transform.rotation * Vec3::Z;
            assert!(face.x < -0.9 && face.z < 0.1, "{folk:?}: face {face}");
            assert!(centroid(held).x < -4.0, "{folk:?}: on the left");
        }
    }

    #[test]
    fn long_hafts_stay_low_and_to_the_right_of_a_held_torch() {
        for base in ["Staff", "Spear"] {
            let item = roll(base, 4);
            let torch = roll("Torch", 4);
            let held = held_in_view(&spec(Folk::Human), Some(&item), Some(&torch));
            let haft_held = held.iter().find(|h| h.hand == Slot::Mainhand).unwrap();
            let torch_held = held.iter().find(|h| h.hand == Slot::Offhand).unwrap();
            assert_eq!(haft_held.hold, Hold::Upright);

            let haft_points = points(haft_held);
            let torch_points = points(torch_held);

            // All haft points stay to the right of all torch points on screen.
            let min_torch_x = torch_points.iter().filter_map(|p| on_screen(*p)).map(|s| s.x).fold(f32::INFINITY, f32::min);
            for p in &haft_points {
                if let Some(screen) = on_screen(*p) {
                    assert!(screen.x >= min_torch_x, "{base}: haft at {p} has screen_x {}, left of torch min {}", screen.x, min_torch_x);
                }
            }
        }
    }

    #[test]
    fn long_hafts_clear_an_off_hand_torch() {
        const MARGIN: f32 = 1.0;
        let mut failures = vec![];
        for folk in Folk::ALL {
            for seed in [1u32, 5, 9, 1207] {
                for haft_base in ["Staff", "Spear"] {
                    for torch_base in ["Torch", "Lantern"] {
                        let haft = roll(haft_base, seed);
                        let torch = roll(torch_base, seed);
                        let held = held_in_view(&BodySpec::roll(folk, seed), Some(&haft), Some(&torch));
                        let haft_held = held.iter().find(|h| h.hand == Slot::Mainhand).unwrap_or_else(|| panic!("{haft_base} not held"));
                        let torch_held = held.iter().find(|h| h.hand == Slot::Offhand).unwrap_or_else(|| panic!("{torch_base} not held"));
                        let haft_points = points(haft_held);
                        let torch_points = points(torch_held);
                        let mut min_distance = f32::INFINITY;
                        for &haft_p in &haft_points {
                            for &torch_p in &torch_points {
                                let dist = haft_p.distance(torch_p);
                                min_distance = min_distance.min(dist);
                            }
                        }
                        if min_distance < MARGIN {
                            failures.push(format!("{folk:?} seed {seed}: {haft_base}-{torch_base} min distance {min_distance:.3} < {MARGIN}"));
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "Long hafts too close to off-hand torch/lantern:\n{}", failures.join("\n"));
    }

    #[test]
    fn nothing_held_reaches_the_eye_or_the_middle_of_the_view() {
        let pairs: [(Option<&str>, Option<&str>); 9] = [
            (Some("Mace"), Some("Torch")),
            (Some("Sword"), Some("Shield")),
            (Some("Staff"), Some("Lantern")),
            (Some("Bow"), None),
            (Some("Spear"), None),
            (Some("Hand axe"), Some("Dagger")),
            (Some("Club"), Some("Torch")),
            (Some("Crossbow"), None),
            (None, Some("Lantern")),
        ];
        let mut failures = vec![];
        for folk in Folk::ALL {
            for seed in [1u32, 5, 9, 1207] {
                for (main, off) in pairs {
                    let (main, off) = (main.map(|b| roll(b, seed)), off.map(|b| roll(b, seed)));
                    for held in held_in_view(&BodySpec::roll(folk, seed), main.as_ref(), off.as_ref()) {
                        for p in points(&held) {
                            let screen = on_screen(p).unwrap_or(Vec2::ZERO);
                            if -p.z < NEAREST || screen.length() < CLEAR_CENTRE {
                                failures.push(format!("{folk:?} {seed}: {:?} at {p}", held.item.family()));
                                break;
                            }
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn placement_is_deterministic() {
        let (bow, family) = (roll("Bow", 3), Family::Bow);
        let a = held_in_view(&spec(Folk::Goblin), Some(&bow), None);
        assert_eq!(a, held_in_view(&spec(Folk::Goblin), Some(&bow), None));
        assert_eq!(a[0].item.family(), family);
    }

    #[test]
    fn worn_things_are_not_held() {
        assert!(held_in_view(&spec(Folk::Human), Some(&roll("Helm", 1)), None).is_empty());
    }

    /// How tall a placed item is drawn, as a fraction of the half height of the view: the whole
    /// of it, including any part past the edge.
    fn screen_height(held: &HeldInView) -> f32 {
        let ys: Vec<f32> = points(held).into_iter().filter_map(on_screen).map(|s| s.y).collect();
        ys.iter().copied().fold(f32::MIN, f32::max) - ys.iter().copied().fold(f32::MAX, f32::min)
    }

    #[test]
    fn the_view_model_sees_narrower_than_the_world() {
        assert_eq!(view().world_fov_degrees, 70.0, "the world's field of view is unchanged");
        assert!(view().held_fov_degrees < view().world_fov_degrees);
    }

    #[test]
    fn held_items_read_half_again_as_large_as_through_the_old_field_of_view() {
        // The main hand's item's drawn height, averaged over seeds 1, 5, 9 and 1207, through
        // the old 60-degree field of view with the old holds (measured before the change).
        let before = [
            (Folk::Human, "Mace", 1.268),
            (Folk::Human, "Sword", 1.482),
            (Folk::Human, "Staff", 2.148),
            (Folk::Gnome, "Mace", 0.774),
            (Folk::Gnome, "Sword", 1.482),
            (Folk::Gnome, "Staff", 1.667),
        ];
        for (folk, base, old) in before {
            let heights: Vec<f32> = [1u32, 5, 9, 1207].iter().map(|&seed| screen_height(&held_in_view(&spec(folk), Some(&roll(base, seed)), None)[0])).collect();
            let now = heights.iter().sum::<f32>() / heights.len() as f32;
            assert!(now >= 1.5 * old, "{folk:?} {base}: {now:.3} against {old:.3} before");
        }
    }
}
