//! Where what the delver holds is drawn in first person: the view model's placement, free of
//! the ECS so it can be tested. `crate::carry` spawns it on the view model's stage.
//!
//! Each hand's item is drawn exactly as rolled, at its own size and proportions, as its
//! inventory icon and the floor show it. Only its turn in the fist comes from the doll
//! ([`fit_hands`]): a blade's edge leading, an axe's bit forward, a bow's string back, a shield
//! strapped along the forearm. The doll also thins a handle to the fist and draws blades,
//! bows and shields smaller, so they sit in its hand; no hand is drawn in first person, so
//! neither is done here. The item, turned as in the doll's fist, is carried into the view:
//! turned to face the way the eye does, its grip moved to where that kind of hold sits in the
//! view ([`view_hold`]) and turned a little more there, so it reads from behind the eye rather
//! than from the front of the doll. The main hand's is low on the right, the off hand's low on
//! the left ([`side_of`]).
//!
//! The placements per hold are our own tuning against captures of the level, not a rule from a
//! source: grips stay just below the bottom edge (no hands are drawn yet), and nothing reaches
//! the middle of the view where the crosshair is ([`CLEAR_CENTRE`]).

use armory::Item;
use bevy::prelude::*;
use fb_body::{BodySpec, Hold, Side, fit_hands};
use fb_items::Slot;

/// Field of view of the view model camera, fixed whatever the world camera uses.
pub const VIEW_MODEL_FOV_DEGREES: f32 = 60.0;

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

/// Where `hold` sits in the view for the hand on `side`.
///
/// - One-handed weapons are held low on the right, standing out of the fist with the head or
///   point tipped away and leaning in, as a weapon is carried at the ready.
/// - Staves, spears and other long hafts lie across the view, low, rising toward the other
///   side and running away from the eye, as a quarterstaff is carried in both hands.
/// - A bow stands low on its side, canted in at the top.
/// - A shield stays strapped along the forearm as on the doll, low at the edge of the view, its
///   face out toward that edge and turned a little forward, so its back is seen edge-on.
/// - A torch stands low, leaning in and away, as the view model always held it; a lantern
///   hangs plumb.
/// - A crossbow is carried with its stock along the forearm, pointing ahead.
pub fn view_hold(hold: Hold, side: Side) -> ViewHold {
    const SPIN: f32 = 0.7;
    let right = match hold {
        Hold::OneHanded => ViewHold { fist: Vec3::new(5.0, -6.0, -9.5), turn: turn(SPIN, -0.3, -0.3, 0.0) },
        Hold::Upright => ViewHold { fist: Vec3::new(5.5, -6.8, -10.0), turn: turn(0.0, 1.2, -0.25, -0.35) },
        Hold::Bow => ViewHold { fist: Vec3::new(4.6, -5.0, -11.0), turn: turn(0.0, 0.3, -0.1, 0.7) },
        Hold::Shield => ViewHold { fist: Vec3::new(8.6, -6.6, -10.0), turn: turn(0.0, 0.0, 0.0, 0.15) },
        Hold::Torch => ViewHold { fist: Vec3::new(6.4, -7.4, -9.6), turn: turn(0.0, 0.22, -0.32, 0.0) },
        Hold::Lantern => ViewHold { fist: Vec3::new(5.4, -1.2, -11.0), turn: Quat::IDENTITY },
        Hold::Crossbow => ViewHold { fist: Vec3::new(5.6, -6.6, -12.5), turn: turn(0.0, 0.0, 1.3, 0.0) },
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
    /// From the item's own space into view model camera space, at the item's own size.
    pub transform: Transform,
}

/// What each hand holds, placed in the view for a delver of body `spec`: the main hand's first,
/// then the off hand's. A hand whose item is not held (or that holds nothing) is left out.
pub fn held_in_view(spec: &BodySpec, main: Option<&Item>, off: Option<&Item>) -> Vec<HeldInView> {
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
            let grip = placed.inverse() * (fist - Vec3::from_array(fit.placement.origin)) / fit.scale;
            let at = view_hold(fit.hold, side);
            let rotation = at.turn * BODY_TO_VIEW * placed;
            let transform = Transform { translation: at.fist - rotation * grip, rotation, scale: Vec3::ONE };
            Some(HeldInView { hand, hold: fit.hold, item: item.clone(), transform })
        })
        .collect()
}

/// Where a point in view model camera space lands on the screen, as a fraction of the half
/// height from the centre (+Y up, +X right; at the edges `y` is ±1 and `x` ± the aspect).
/// `None` behind the eye.
pub fn on_screen(p: Vec3) -> Option<Vec2> {
    (p.z < 0.0).then(|| p.xy() / (-p.z * (VIEW_MODEL_FOV_DEGREES.to_radians() * 0.5).tan()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use armory::Family;
    use fb_body::Folk;

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
                assert_eq!(held.transform.scale, Vec3::ONE, "{folk:?} {base}: at its own size");
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
    fn long_hafts_lie_across_the_view_low() {
        for base in ["Staff", "Spear"] {
            let item = roll(base, 4);
            let held = &held_in_view(&spec(Folk::Human), Some(&item), None)[0];
            assert_eq!(held.hold, Hold::Upright);
            // Across: its length runs more sideways than up.
            let along = (held.transform.rotation * Vec3::Y).normalize();
            assert!(along.x.abs() > along.y.abs(), "{base}: {along}");
            // Low: all of it below the middle of the view.
            for p in points(held) {
                assert!(on_screen(p).is_some_and(|s| s.y < 0.0), "{base}: {p} above the middle");
            }
        }
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
}
