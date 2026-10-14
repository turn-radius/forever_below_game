//! A dressed figure: one folk's body wearing its gear and holding its main- and off-hand items,
//! ready to draw in the house style. Used by the doll view (`--doll`, [`crate::doll_view`])
//! and by the inventory's doll in the runic seal ([`crate::inventory::doll`]).
//!
//! The fitting is all `fb_body`'s: [`fit_hands`] poses the arms for what they hold, and
//! [`fit_worn`] dresses that posed body, so sleeves follow the arms that hold the items and
//! hands close round their grips. This module only keeps each item's own look with its mesh
//! (its palette, condition and etching, as on the floor and in its icon), so every worn layer
//! and held item wears as the item itself does, and the skin wears with the character's dirt.

use armory::{
    ItemMesh,
    build::light::LightAnchor,
    cond::{Condition, ConditionState},
};
use bevy::prelude::*;
use fb_body::{Body, BodySpec, Side, fit_hands, fit_worn};
use fb_items::Slot;

use crate::carry::{appearance_of, item_light};
use crate::render::{ItemAppearance, ItemSpawner};

/// One drawable piece of a figure: a mesh in body space and how it looks.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub name: String,
    pub mesh: ItemMesh,
    pub appearance: ItemAppearance,
}

/// A lit thing a figure holds: where its light is in body space, the light, and the hand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeldLight {
    pub at: Vec3,
    pub anchor: LightAnchor,
    pub side: Side,
}

/// A body dressed and holding its items, as pieces to draw.
#[derive(Clone, Debug)]
pub struct Figure {
    /// The body posed for what its hands hold.
    pub body: Body,
    /// The bare skin, head and hair first, then each worn layer innermost first, then the main
    /// hand's item and the off hand's.
    pub pieces: Vec<Piece>,
    /// The light it holds, the main hand's before the off hand's.
    pub light: Option<HeldLight>,
}

impl Figure {
    /// The body for `spec` wearing `worn` (each item with its slot; the main and off hand's
    /// items are held, rings and charms are not drawn), its skin worn with `dirt` (0 clean to
    /// 1 filthy, the plate's `C.dirt`).
    pub fn dress(spec: &BodySpec, worn: &[(Slot, fb_items::Item)], dirt: f32) -> Figure {
        let held = |slot| worn.iter().find(|(s, _)| *s == slot).map(|(_, item)| &item.shape);
        let hands = fit_hands(spec, held(Slot::Mainhand), held(Slot::Offhand));
        let body = hands.body;
        let shapes: Vec<(Slot, armory::Item)> = worn.iter().map(|(slot, item)| (*slot, item.shape.clone())).collect();
        let fit = fit_worn(&body, &shapes);

        let mut skin = body.skin_mesh();
        skin.parts.retain(|p| !fit.covered.hides(p.name));
        skin.parts.extend(body.head_mesh().parts);
        skin.parts.extend(body.hair_mesh(fit.hair).parts);
        let condition = Condition { state: ConditionState::Custom, wear: dirt.clamp(0.0, 1.0), ..Condition::default() };
        let mut pieces = vec![Piece { name: "Body".into(), mesh: skin, appearance: ItemAppearance { condition, seed: spec.seed, ..default() } }];
        for layer in &fit.layers {
            let Some((_, item)) = worn.iter().find(|(slot, _)| *slot == layer.slot) else { continue };
            pieces.push(Piece { name: item.title().to_string(), mesh: layer.mesh.clone(), appearance: appearance_of(&item.shape) });
        }
        let mut light = None;
        for (slot, fit) in [(Slot::Mainhand, &hands.main), (Slot::Offhand, &hands.off)] {
            let (Some(fit), Some((_, item))) = (fit, worn.iter().find(|(s, _)| *s == slot)) else { continue };
            pieces.push(Piece { name: item.title().to_string(), mesh: fit.mesh.clone(), appearance: appearance_of(&fit.item) });
            if light.is_none()
                && let Some(anchor) = item_light(&fit.item)
            {
                let side = if slot == Slot::Mainhand { fb_body::MAIN_HAND } else { fb_body::OFF_HAND };
                light = Some(HeldLight { at: Vec3::from_array(fit.to_body(anchor.position)), anchor, side });
            }
        }
        Figure { body, pieces, light }
    }

    /// The figure's bounds in body space, every piece included.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        self.pieces
            .iter()
            .flat_map(|p| p.mesh.parts.iter().flat_map(|part| &part.positions))
            .map(|&p| Vec3::from_array(p))
            .fold(None, |acc, p| Some(acc.map_or((p, p), |(lo, hi): (Vec3, Vec3)| (lo.min(p), hi.max(p)))))
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
    fn the_same_folk_seed_and_outfit_look_the_same() {
        let dress = || {
            let figure = Figure::dress(&BodySpec::roll(Folk::Dwarf, 7), &fighter(Folk::Dwarf, 7), 0.3);
            figure.pieces
        };
        assert_eq!(dress(), dress());
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
}
