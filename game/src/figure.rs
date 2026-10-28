//! A dressed figure: one folk's body wearing its gear and holding its main- and off-hand items,
//! ready to draw in the house style. Used by the doll view (`--doll`, [`crate::doll_view`])
//! and by the inventory's doll in the runic seal ([`crate::inventory::doll`]).
//!
//! The fitting is all `fb_body`'s: [`fit_hands`] poses the arms for what they hold, and
//! [`fb_body::fit_worn`] dresses that posed body, so sleeves follow the arms that hold the items and
//! hands close round their grips. This module only keeps each item's own look with its mesh
//! (its palette, condition and etching, as on the floor and in its icon), so every worn layer
//! and held item wears as the item itself does, and the skin wears with the character's dirt.

use armory::{
    ItemMesh,
    build::light::LightAnchor,
    cond::{Condition, ConditionState},
};
use bevy::prelude::*;
use fb_body::{Body, BodySpec, Gait, Joint, PartSkin, Rig, Side, SkinRegion, Skinner, fit_hands, fit_hip};
use fb_items::{Accessory, Slot};

use crate::carry::{appearance_of, item_light};
use crate::render::{ItemAppearance, ItemSpawner};

/// One drawable piece of a figure: a mesh in body space and how it looks.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub name: String,
    pub mesh: ItemMesh,
    pub appearance: ItemAppearance,
    /// Whether it turns with the head about the neck ([`Figure::neck`]): the head and hair, and
    /// a helm or hat. Not under a hood, whose cape lies on the shoulders.
    pub on_head: bool,
    /// Whether it gives light (a lit torch or lantern in hand). Drawn without a shadow, or its
    /// own flame and body would black out its light, as for lit things on the floor.
    pub lit: bool,
    /// How each part's vertices follow the rig's joints (`fb_body::Skinner`), parallel to the
    /// mesh's parts. `None` for what rides a joint rigidly ([`Piece::attach`]).
    pub skin: Option<Vec<PartSkin>>,
    /// The joint a held item rides on, rigidly, in place of weights: the wrist of its hand, or
    /// the pelvis for something hung at the hip.
    pub attach: Option<Joint>,
}

/// A lit thing a figure holds: where its light is in body space, the light, and the hand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeldLight {
    pub at: Vec3,
    pub anchor: LightAnchor,
    pub side: Side,
    /// The joint it rides on: the hand's wrist, or the pelvis for a lantern at the hip.
    pub joint: Joint,
}

/// A body dressed and holding its items, as pieces to draw.
#[derive(Clone, Debug)]
pub struct Figure {
    /// The body posed for what its hands hold.
    pub body: Body,
    /// The bare skin, head and hair first, then each worn layer innermost first, then the main
    /// hand's item and the off hand's.
    pub pieces: Vec<Piece>,
    /// The light it holds, the main hand's before the off hand's, or else a lantern's at its hip.
    pub light: Option<HeldLight>,
    /// Where the head turns, in body space: the base of the neck.
    pub neck: Vec3,
    /// The body's joints at rest, and how it walks and stands ([`crate::folk_pose`] poses them).
    pub rig: Rig,
    pub gait: Gait,
    /// Whether a hood is worn: the head and hood then turn with the body, not about the neck.
    pub hooded: bool,
}

impl Figure {
    /// The body for `spec` wearing `worn` (each item with its slot; the main and off hand's
    /// items are held, rings and charms are not drawn), its skin worn with `dirt` (0 clean to
    /// 1 filthy, the plate's `C.dirt`).
    pub fn dress(spec: &BodySpec, worn: &[(Slot, fb_items::Item)], dirt: f32) -> Figure {
        Self::dress_hung(spec, worn, &[], dirt)
    }

    /// [`Figure::dress`] with what hangs on the worn pack and belt: the bedroll or tent in the pack's
    /// lashing, the long weapon on its strap and the weapons in the belt's frogs
    /// ([`crate::inventory::Inventory::worn_hung`]), each drawn on the pack or belt.
    pub fn dress_hung(spec: &BodySpec, worn: &[(Slot, fb_items::Item)], hung: &[(Accessory, fb_items::Item)], dirt: f32) -> Figure {
        let held = |slot| worn.iter().find(|(s, _)| *s == slot).map(|(_, item)| &item.shape);
        let hands = fit_hands(spec, held(Slot::Mainhand), held(Slot::Offhand));
        let body = hands.body;
        let shapes: Vec<(Slot, armory::Item)> = worn.iter().map(|(slot, item)| (*slot, item.shape.clone())).collect();
        let shapes_hung: Vec<(Accessory, armory::Item)> = hung.iter().map(|(accessory, item)| (*accessory, item.shape.clone())).collect();
        let fit = fb_body::fit_worn_hung(&body, &shapes, &shapes_hung);

        let mut skin = body.skin_mesh_lifted(fit.wing_lift);
        skin.parts.retain(|p| !fit.covered.hides(p.name));
        let mut head = body.head_mesh_fitted(fit.head);
        head.parts.extend(body.hair_mesh(fit.hair).parts);
        let condition = Condition { state: ConditionState::Custom, wear: dirt.clamp(0.0, 1.0), ..Condition::default() };
        let flesh = ItemAppearance { condition, seed: spec.seed, ..default() };
        let hooded = matches!(held(Slot::Head), Some(armory::Item::Hood(_)));
        // Every vertex of the skin and the gear follows the rig's joints, by where it rests; what
        // is held rides a joint rigidly. The head and the hood turn with the body under a hood.
        let skinner = Skinner::with_wing_lift(&body, fit.wing_lift);
        let head_region = if hooded { SkinRegion::Chest } else { SkinRegion::Head };
        let weights = |mesh: &ItemMesh, region: &dyn Fn(&str) -> SkinRegion| Some(skinner.mesh(mesh, region));
        let mut pieces = vec![
            Piece { name: "Body".into(), appearance: flesh, on_head: false, lit: false, skin: weights(&skin, &SkinRegion::of_skin), attach: None, mesh: skin },
            Piece { name: "Head".into(), appearance: flesh, on_head: !hooded, lit: false, skin: weights(&head, &|_| head_region), attach: None, mesh: head },
        ];
        for layer in &fit.layers {
            let Some((_, item)) = worn.iter().find(|(slot, _)| *slot == layer.slot) else { continue };
            let on_head = layer.slot == Slot::Head && !hooded;
            let region = |name: &str| if layer.slot == Slot::Head && hooded { SkinRegion::Chest } else { SkinRegion::of_worn(layer.slot, name) };
            pieces.push(Piece {
                name: item.title().to_string(),
                mesh: layer.mesh.clone(),
                appearance: appearance_of(&item.shape),
                on_head,
                lit: false,
                skin: weights(&layer.mesh, &region),
                attach: None,
            });
        }
        let mut light = None;
        for (slot, fit) in [(Slot::Mainhand, &hands.main), (Slot::Offhand, &hands.off)] {
            let (Some(fit), Some((_, item))) = (fit, worn.iter().find(|(s, _)| *s == slot)) else { continue };
            let anchor = item_light(&fit.item);
            let side = if slot == Slot::Mainhand { fb_body::MAIN_HAND } else { fb_body::OFF_HAND };
            pieces.push(Piece {
                name: item.title().to_string(),
                mesh: fit.mesh.clone(),
                appearance: appearance_of(&fit.item),
                on_head: false,
                lit: anchor.is_some(),
                skin: None,
                attach: Some(Joint::wrist(side)),
            });
            if light.is_none()
                && let Some(anchor) = anchor
            {
                light = Some(HeldLight { at: Vec3::from_array(fit.to_body(anchor.position)), anchor, side, joint: Joint::wrist(side) });
            }
        }
        // A lantern hung at the hip, from the belt; the carried light if no hand holds one.
        let at_hip = worn.iter().find(|(s, _)| *s == Slot::Hip);
        if let (Some((_, item)), Some(belt)) = (at_hip, fit.layer(Slot::Belt))
            && let Some(hip) = fit_hip(&body, &item.shape, &belt.mesh)
        {
            let anchor = item_light(&hip.item);
            pieces.push(Piece {
                name: item.title().to_string(),
                mesh: hip.mesh.clone(),
                appearance: appearance_of(&hip.item),
                on_head: false,
                lit: anchor.is_some(),
                skin: None,
                attach: Some(Joint::Pelvis),
            });
            if light.is_none()
                && let Some(anchor) = anchor
            {
                light = Some(HeldLight { at: Vec3::from_array(hip.to_body(anchor.position)), anchor, side: hip.side, joint: Joint::Pelvis });
            }
        }
        // A long weapon lashed to the pack's weapon strap, riding the trunk with the pack.
        if let (Some((_, item)), Some(pack)) = (hung.iter().find(|(a, _)| *a == Accessory::WeaponStrap), fit.layer(Slot::Back))
            && let Some(lashed) = fb_body::fit_strapped(&item.shape, &pack.mesh)
        {
            pieces.push(Piece {
                name: item.title().to_string(),
                appearance: appearance_of(&lashed.item),
                on_head: false,
                lit: false,
                skin: weights(&lashed.mesh, &|_| SkinRegion::Trunk),
                mesh: lashed.mesh,
                attach: None,
            });
        }
        // A weapon in each of the belt's frogs, riding the pelvis with the belt.
        for (frog, left) in [(Accessory::LeftFrog, true), (Accessory::RightFrog, false)] {
            if let (Some((_, item)), Some(belt)) = (hung.iter().find(|(a, _)| *a == frog), fit.layer(Slot::Belt))
                && let Some(hung) = fb_body::fit_frogged(&item.shape, &belt.mesh, left)
            {
                pieces.push(Piece {
                    name: item.title().to_string(),
                    appearance: appearance_of(&hung.item),
                    on_head: false,
                    lit: false,
                    skin: None,
                    mesh: hung.mesh,
                    attach: Some(Joint::Pelvis),
                });
            }
        }
        // A bedroll or tent tied in the pack's lashing, riding the trunk with the pack.
        if let (Some((_, item)), Some(pack)) = (hung.iter().find(|(a, _)| *a == Accessory::Lashing), fit.layer(Slot::Back))
            && let Some(tied) = fb_body::fit_lashed(&item.shape, &pack.mesh)
        {
            pieces.push(Piece {
                name: item.title().to_string(),
                appearance: appearance_of(&tied.item),
                on_head: false,
                lit: false,
                skin: weights(&tied.mesh, &|_| SkinRegion::Trunk),
                mesh: tied.mesh,
                attach: None,
            });
        }
        let neck = Vec3::from_array(body.neck_base().origin);
        let rig = Rig::of_lifted(&body, fit.wing_lift);
        let gait = Gait::new(&body, &rig);
        Figure { body, pieces, light, neck, rig, gait, hooded }
    }

    /// The figure's bounds in body space, every piece included.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        self.pieces
            .iter()
            .flat_map(|p| p.mesh.parts.iter().flat_map(|part| &part.positions))
            .map(|&p| Vec3::from_array(p))
            .fold(None, |acc, p| Some(acc.map_or((p, p), |(lo, hi): (Vec3, Vec3)| (lo.min(p), hi.max(p)))))
    }

    /// The top of what is worn on the head, in body space: the highest vertex of any unlit piece
    /// within one and a half head radii of the head's centre, sideways, and above it, among the gear (not the body, head and hair, which come first). A hood or a
    /// tall hat rises above the skull; the bare head's own top is returned for a bare head. Held
    /// lights are left out.
    pub fn head_top(&self) -> f32 {
        let (centre, r) = self.head();
        let reach = 1.5 * r;
        self.pieces
            .iter()
            .skip(2)
            .filter(|p| !p.lit)
            .flat_map(|p| {
                // What rides the head (a helm or hat) counts whole, horns and brim included.
                let on_head = p.on_head;
                p.mesh.parts.iter().flat_map(|part| &part.positions).filter(move |v| on_head || ((v[0] - centre.x).abs() <= reach && (v[2] - centre.z).abs() <= reach)).map(|v| v[1])
            })
            .filter(|&y| y >= centre.y)
            .fold(centre.y + self.body.head.half_height, f32::max)
    }

    /// The head's centre and its radius (its largest half extent), in body space.
    pub fn head(&self) -> (Vec3, f32) {
        let head = &self.body.head;
        (Vec3::from_array(head.centre), head.half_width.max(head.half_height).max(head.half_depth))
    }
}

/// Spawns `figure` at the origin as one root entity with every piece under it, each piece
/// keeping an identity transform so its wear stays where its mesh puts it. Returns the root.
pub fn spawn_figure(commands: &mut Commands, spawner: &mut ItemSpawner, figure: &Figure) -> Entity {
    let root = commands.spawn((Name::new("Figure"), Transform::default(), Visibility::default())).id();
    for piece in &figure.pieces {
        spawner.spawn(&piece.mesh, &piece.appearance).insert((Name::new(piece.name.clone()), ChildOf(root)));
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_body::Folk;

    fn fighter(folk: Folk, seed: u32) -> Vec<(Slot, fb_items::Item)> {
        fb_items::outfit::outfit("fighter").unwrap().roll(seed, folk.key()).worn
    }

    #[test]
    fn every_folk_is_dressed_and_holds_a_weapon() {
        for folk in Folk::ALL {
            let worn = fighter(folk, 7);
            let figure = Figure::dress(&BodySpec::roll(folk, 7), &worn, 0.2);
            let weapon = worn.iter().find(|(s, _)| *s == Slot::Mainhand).map(|(_, i)| i.title().to_string()).unwrap();
            let names: Vec<&str> = figure.pieces.iter().map(|p| p.name.as_str()).collect();
            assert!(names.contains(&weapon.as_str()), "{folk:?}: no {weapon} in {names:?}");
            // Skin, at least a shirt and trousers, and the weapon.
            assert!(figure.pieces.len() >= 4, "{folk:?}: {names:?}");
            assert!(figure.pieces.iter().all(|p| !p.mesh.parts.is_empty()), "{folk:?}");
            // The weapon is in the main hand's fist.
            let fist = Vec3::from_array(figure.body.grip(fb_body::MAIN_HAND).origin);
            let held = &figure.pieces.iter().find(|p| p.name == weapon).unwrap().mesh;
            let nearest = held.parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p).distance(fist)).fold(f32::MAX, f32::min);
            assert!(nearest < 0.5, "{folk:?}: weapon {nearest} from the fist");
        }
    }

    #[test]
    fn a_minotaur_is_dressed_and_holds_a_weapon_in_its_fist() {
        for outfit in ["fighter", "ranger", "mage", "cleric"] {
            let worn = fb_items::outfit::outfit(outfit).unwrap().roll(7, Folk::Minotaur.key()).worn;
            let figure = Figure::dress(&BodySpec::roll(Folk::Minotaur, 7), &worn, 0.2);
            let names: Vec<&str> = figure.pieces.iter().map(|p| p.name.as_str()).collect();
            assert!(figure.pieces.len() >= 4 && figure.pieces.iter().all(|p| !p.mesh.parts.is_empty()), "{outfit}: {names:?}");
            // Hooves are bare, the horns are drawn, the tail is skinned to its bones.
            let parts = |piece: &str| figure.pieces.iter().find(|p| p.name == piece).unwrap().mesh.parts.iter().map(|p| p.name).collect::<Vec<_>>();
            assert!(parts("Body").contains(&"hooves") && parts("Head").contains(&"horns"), "{outfit}");
            let body = figure.pieces.iter().find(|p| p.name == "Body").unwrap();
            assert!(body.skin.as_ref().is_some_and(|s| s.len() == body.mesh.parts.len()));
            if let Some((_, weapon)) = worn.iter().find(|(s, _)| *s == Slot::Mainhand) {
                let fist = Vec3::from_array(figure.body.grip(fb_body::MAIN_HAND).origin);
                let held = &figure.pieces.iter().find(|p| p.name == weapon.title()).unwrap().mesh;
                let nearest = held.parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p).distance(fist)).fold(f32::MAX, f32::min);
                assert!(nearest < 0.7, "{outfit}: weapon {nearest} from the fist");
            }
        }
    }

    #[test]
    fn each_demon_is_dressed_with_wings_over_the_gear_and_holds_a_weapon() {
        for folk in [Folk::WingedDemon, Folk::HornedDemon, Folk::Succubus] {
            for outfit in ["fighter", "mage"] {
                let worn = fb_items::outfit::outfit(outfit).unwrap().roll(7, folk.key()).worn;
                let figure = Figure::dress(&BodySpec::roll(folk, 7), &worn, 0.2);
                let body = figure.pieces.iter().find(|p| p.name == "Body").unwrap();
                let names: Vec<&str> = body.mesh.parts.iter().map(|p| p.name).collect();
                assert!(figure.pieces.len() >= 4 && figure.pieces.iter().all(|p| !p.mesh.parts.is_empty()), "{folk:?} {outfit}");
                assert!(names.contains(&"tail_spade"), "{folk:?} {outfit}: {names:?}");
                // The wings are skinned (every vertex to joints) and the body has them only where the race does.
                assert_eq!(names.contains(&"wing_membrane"), folk != Folk::HornedDemon, "{folk:?} {outfit}");
                assert!(body.skin.as_ref().is_some_and(|s| s.len() == body.mesh.parts.len()));
                if let Some((_, weapon)) = worn.iter().find(|(s, _)| *s == Slot::Mainhand) {
                    let fist = Vec3::from_array(figure.body.grip(fb_body::MAIN_HAND).origin);
                    let held = &figure.pieces.iter().find(|p| p.name == weapon.title()).unwrap().mesh;
                    let nearest = held.parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p).distance(fist)).fold(f32::MAX, f32::min);
                    assert!(nearest < 0.7, "{folk:?} {outfit}: weapon {nearest} from the fist");
                }
            }
        }
    }

    #[test]
    fn the_same_folk_seed_and_outfit_look_the_same() {
        let dress = || {
            let figure = Figure::dress(&BodySpec::roll(Folk::Dwarf, 7), &fighter(Folk::Dwarf, 7), 0.3);
            figure.pieces
        };
        assert_eq!(dress(), dress());
    }

    #[test]
    fn an_off_hand_weapon_is_held_in_the_left_fist() {
        let spec = BodySpec::roll(Folk::Human, 5);
        for base in ["Sword", "Dagger", "Mace", "Club", "Hand axe", "Spear", "Staff", "Bow", "Crossbow"] {
            let (main, off) = (fb_items::roll_item("Sword", 3).unwrap(), fb_items::roll_item(base, 8).unwrap());
            let name = off.title().to_string();
            let figure = Figure::dress(&spec, &[(Slot::Mainhand, main), (Slot::Offhand, off)], 0.0);
            let held = &figure.pieces.last().unwrap();
            assert_eq!(held.name, name, "{base}: the off hand's is drawn last");
            let points: Vec<Vec3> = held.mesh.parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p)).collect();
            let fist = Vec3::from_array(figure.body.grip(fb_body::OFF_HAND).origin);
            let nearest = points.iter().map(|p| p.distance(fist)).fold(f32::MAX, f32::min);
            assert!(nearest < 0.5, "{base}: {nearest} from the left fist");
            // On the wearer's left (+X), as the main hand's sword is on the right.
            let centre = points.iter().sum::<Vec3>() / points.len() as f32;
            assert!(fist.x > 0.0 && centre.x > 0.0, "{base}: fist {fist}, centre {centre}");
        }
    }

    #[test]
    fn a_hip_lantern_hangs_from_the_belt_and_lights_when_no_hand_does() {
        let lantern = (1..200).filter_map(|s| fb_items::roll_item("Lantern", s)).find(|i| item_light(&i.shape).is_some()).unwrap();
        let name = lantern.title().to_string();
        let (sword, dagger, belt) = (fb_items::roll_item("Sword", 3).unwrap(), fb_items::roll_item("Dagger", 4).unwrap(), fb_items::roll_item("Belt", 2).unwrap());
        let spec = BodySpec::roll(Folk::Human, 5);
        let worn = [(Slot::Belt, belt), (Slot::Mainhand, sword), (Slot::Offhand, dagger), (Slot::Hip, lantern.clone())];
        let figure = Figure::dress(&spec, &worn, 0.0);
        let hung = figure.pieces.iter().find(|p| p.name == name).expect("the lantern is drawn");
        assert!(hung.lit);
        let (lo, hi) = hung.mesh.bounds().unwrap();
        // At the hip, on the off hand's side (+X): from the belt line down, above the knee.
        let body = &figure.body;
        assert!(lo[0] > 0.0 && hi[1] <= body.waist_y + 0.5 && lo[1] > body.hip_y * 0.5, "{lo:?}..{hi:?}");
        let light = figure.light.expect("the hip lantern lights the doll");
        assert_eq!(light.side, fb_body::OFF_HAND);
        assert!(light.at.y < body.waist_y, "{}", light.at);
        // Without a belt nothing hangs there.
        let unbelted = Figure::dress(&spec, &[(Slot::Hip, lantern)], 0.0);
        assert!(unbelted.pieces.iter().all(|p| p.name != name) && unbelted.light.is_none());
    }

    #[test]
    fn a_held_lantern_lights_from_its_hand() {
        let lantern = (1..200).filter_map(|s| fb_items::roll_item("Lantern", s)).find(|i| item_light(&i.shape).is_some()).unwrap();
        let mace = fb_items::roll_item("Mace", 3).unwrap();
        let spec = BodySpec::roll(Folk::Human, 5);
        let figure = Figure::dress(&spec, &[(Slot::Mainhand, mace), (Slot::Offhand, lantern)], 0.0);
        let light = figure.light.expect("a lit lantern");
        assert_eq!(light.side, fb_body::OFF_HAND);
        // On the off hand's side (+X, the wearer's left), below the shoulder.
        assert!(light.at.x > 0.0 && light.at.y < figure.body.shoulder_y, "{}", light.at);
        assert!(Figure::dress(&spec, &[], 0.0).light.is_none());
    }

    #[test]
    fn async_fitting_produces_identical_output() {
        use crate::fitting::Fitting;

        let spec = BodySpec::roll(Folk::Human, 42);
        let worn = fighter(Folk::Human, 42);

        // Direct fit
        let direct = Figure::dress(&spec, &worn, 0.3);

        // Async fit
        let spec_clone = spec.clone();
        let worn_clone = worn.clone();
        let mut fitting = Fitting::start((), move || Figure::dress(&spec_clone, &worn_clone, 0.3));

        // Poll until done. This checks the result, not the speed: a machine running every test
        // binary at once can take far longer than the usual moment, so it waits up to two minutes.
        let async_result = std::iter::repeat_with(|| {
            std::thread::sleep(std::time::Duration::from_millis(5));
            fitting.poll()
        })
        .take(24_000)
        .flatten()
        .next()
        .expect("async fitting should complete");

        // Compare key properties
        assert_eq!(direct.pieces.len(), async_result.pieces.len(), "piece count differs");
        for (i, (d, a)) in direct.pieces.iter().zip(async_result.pieces.iter()).enumerate() {
            assert_eq!(d.name, a.name, "piece {i} name differs");
            assert_eq!(d.mesh.parts.len(), a.mesh.parts.len(), "piece {i} part count differs");
            for (j, (dp, ap)) in d.mesh.parts.iter().zip(a.mesh.parts.iter()).enumerate() {
                assert_eq!(dp.positions, ap.positions, "piece {i} part {j} positions differ");
                assert_eq!(dp.indices, ap.indices, "piece {i} part {j} indices differ");
                assert_eq!(dp.normals, ap.normals, "piece {i} part {j} normals differ");
            }
        }
        assert_eq!(direct.bounds(), async_result.bounds(), "bounds differ");
    }
}
