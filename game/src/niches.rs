//! What the level's niches hold (`fb_level::NicheSpot`): a lit candle, a skull, a scatter of
//! bones or a small urn, standing on the sill of the recess the stone cuts into the wall
//! (`level::geometry::stone`).
//!
//! Each is built by the generator that already draws it: the Armory's candle, the skull and
//! bones of the level's scenery, and the amphora of its containers, fitted to the recess and
//! turned to face out of it. They are scenery: they block nothing and cannot be taken. A candle
//! lights its recess with a small point light of its own that casts no shadow, so it takes none
//! of the shadow budget.

use armory::{
    Item,
    build::scenery::{Scenery, lone_skull},
    cond::ConditionState,
    families::{chests::ChestStyle, sundry::{Sundry, SundryKind}},
};
use bevy::prelude::*;
use fb_level::{Level, NicheContents, NicheSpot, chest_state, niches};

use crate::carry::appearance_of;
use crate::level::{EXPOSURE, LevelPiece, fixture_appearance};
use crate::light::{Torch, point_light};
use crate::play::CurrentLevel;
use crate::render::{ItemAppearance, ItemSpawner};
use crate::tuning::{LightTuning, Tuning};

/// A niche's contents standing in the level: which of `CurrentLevel.niches` it belongs to.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct NichePiece {
    pub index: usize,
}

/// The room a thing has in a niche, in world units: across, up and deep.
const ROOM: [f32; 3] = [4.6, 4.2, 3.2];
/// How long the longest side of what a niche holds is drawn, in world units, whatever its own
/// size: an urn, a candle with its holder, and a skull.
const URN_HEIGHT: f32 = 4.0;
const CANDLE_HEIGHT: f32 = 3.4;
const SKULL_LENGTH: f32 = 3.0;
/// How far in front of the wall's plane a candle's light hangs, so it reaches the wall round the niche.
const LIGHT_OUT: f32 = 0.8;

/// What a niche holds, built: the mesh and its look, the Armory candle's flame (item space) if
/// it is lit, and how it is fitted to the sill.
fn contents_of(spot: &NicheSpot, ceiling: f32) -> Option<(armory::ItemMesh, ItemAppearance, Option<Vec3>, Option<f32>)> {
    Some(match spot.contents {
        NicheContents::Empty => return None,
        NicheContents::Candle => {
            let mut state = Sundry::roll_as(spot.seed, SundryKind::Candle);
            state.article.lit = true;
            state.article.count = 1;
            let flame = Sundry::light(&state).map(|anchor| Vec3::from_array(anchor.position));
            let item = Item::Sundry(state);
            (item.build(), appearance_of(&item), flame, Some(CANDLE_HEIGHT))
        }
        NicheContents::Skull => (lone_skull(spot.seed), fixture_appearance(spot.seed, ConditionState::Neglected), None, Some(SKULL_LENGTH)),
        NicheContents::Bones => (Scenery::Bones.build(spot.seed, ceiling), fixture_appearance(spot.seed, ConditionState::Neglected), None, None),
        NicheContents::Urn => {
            let item = Item::Chests(chest_state(ChestStyle::Amphora, spot.seed, false));
            (item.build(), appearance_of(&item), None, Some(URN_HEIGHT))
        }
    })
}

/// Where and how a mesh with item-space `bounds` stands in `spot`'s recess: its transform
/// (resting on the sill, in the middle, turned to face out) and the scale it was fitted by.
/// `longest` fixes how long its longest side is drawn; otherwise it is only ever shrunk to fit.
pub fn on_the_sill(spot: &NicheSpot, bounds: ([f32; 3], [f32; 3]), longest: Option<f32>) -> Transform {
    let (lo, hi) = (Vec3::from_array(bounds.0), Vec3::from_array(bounds.1));
    let size = hi - lo;
    let fit = (ROOM[0] / size.x.max(1e-3)).min(ROOM[1] / size.y.max(1e-3)).min(ROOM[2] / size.z.max(1e-3));
    let scale = longest.map_or(fit.min(1.0), |l| (l / size.max_element().max(1e-3)).min(fit)).max(0.05);
    let (dx, dy) = spot.facing.step();
    // Out of the wall, into the room.
    let out = Vec3::new(-dx as f32, 0.0, -dy as f32);
    let yaw = Quat::from_rotation_y(out.x.atan2(out.z));
    let [px, _, pz] = Level::world(spot.x as f32 + 0.5 + dx as f32 * 0.5, spot.y as f32 + 0.5 + dy as f32 * 0.5);
    let wall = Vec3::new(px, 0.0, pz);
    let middle = Vec3::new((lo.x + hi.x) * 0.5, lo.y, (lo.z + hi.z) * 0.5) * scale;
    // The middle of the recess, half its depth in.
    let seat = wall - out * niches::DEPTH * 0.5 + Vec3::Y * niches::SILL;
    Transform { translation: seat - yaw * middle, rotation: yaw, scale: Vec3::splat(scale) }
}

/// The light of a candle burning in a niche: warm, flickering as every flame does, small, and
/// unshadowed.
pub fn niche_light(tuning: &LightTuning) -> PointLight {
    Torch::flickering(point_light(Torch::COLOR, tuning.niche_peak, tuning.niche_reach, &EXPOSURE), tuning.flicker_depth)
}

/// Spawns (or respawns, when the level changes) what the level's niches hold.
pub fn lay_out_niches(mut commands: Commands, level: Res<CurrentLevel>, tuning: Option<Res<Tuning>>, old: Query<Entity, With<NichePiece>>, mut spawner: ItemSpawner) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let light = tuning.map(|t| t.light.clone()).unwrap_or_default();
    for (index, spot) in level.0.niches.iter().enumerate() {
        let Some((mesh, appearance, flame, longest)) = contents_of(spot, level.0.ceiling) else { continue };
        let Some(bounds) = mesh.bounds() else { continue };
        let transform = on_the_sill(spot, bounds, longest);
        spawner.spawn(&mesh, &appearance).insert((LevelPiece, Name::new(format!("niche {:?}", spot.contents)), transform, NichePiece { index }));
        if let Some(flame) = flame {
            let (dx, dy) = spot.facing.step();
            let out = Vec3::new(-dx as f32, 0.0, -dy as f32);
            // The flame, then drawn out to just in front of the wall's plane.
            let at = transform.transform_point(flame);
            let plane = Vec3::from_array(Level::world(spot.x as f32 + 0.5 + dx as f32 * 0.5, spot.y as f32 + 0.5 + dy as f32 * 0.5));
            let depth_in = (at - plane).dot(-out);
            let at = at + out * (depth_in + LIGHT_OUT);
            commands.spawn((LevelPiece, Name::new("niche candle light"), NichePiece { index }, niche_light(&light), Transform::from_translation(at)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A niche of each kind of contents, from stone halls levels of the game's own delve.
    fn one_of_each() -> HashMap<NicheContents, (NicheSpot, Level)> {
        let mut found = HashMap::new();
        for seed in 1..=80 {
            let level = crate::travel::Abyss::new(seed).level_at(2);
            for spot in &level.niches {
                found.entry(spot.contents).or_insert((*spot, level.clone()));
            }
        }
        found
    }

    #[test]
    fn what_a_niche_holds_rests_on_the_sill_inside_the_recess_and_faces_out() {
        let found = one_of_each();
        assert_eq!(found.len(), 5, "every kind of niche turns up");
        for (contents, (spot, level)) in &found {
            let Some((mesh, _, flame, _)) = contents_of(spot, level.ceiling) else {
                assert_eq!(*contents, NicheContents::Empty);
                continue;
            };
            assert_eq!(flame.is_some(), *contents == NicheContents::Candle, "{contents:?}: only a candle is lit");
            let longest = match contents {
                NicheContents::Candle => Some(CANDLE_HEIGHT),
                NicheContents::Skull => Some(SKULL_LENGTH),
                NicheContents::Urn => Some(URN_HEIGHT),
                _ => None,
            };
            let bounds = mesh.bounds().expect("it has a shape");
            let at = on_the_sill(spot, bounds, longest);
            let (dx, dy) = spot.facing.step();
            let out = Vec3::new(-dx as f32, 0.0, -dy as f32);
            let [px, _, pz] = Level::world(spot.face().0, spot.face().1);
            let wall = Vec3::new(px, 0.0, pz);
            let across = Vec3::Y.cross(out);
            for corner in 0..8 {
                let c = Vec3::new(if corner & 1 == 0 { bounds.0[0] } else { bounds.1[0] }, if corner & 2 == 0 { bounds.0[1] } else { bounds.1[1] }, if corner & 4 == 0 { bounds.0[2] } else { bounds.1[2] });
                let p = at.transform_point(c) - wall;
                let (side, up, deep) = (p.dot(across), p.y, -p.dot(out));
                assert!(side.abs() <= niches::WIDTH * 0.5, "{contents:?}: {side} across");
                assert!(up >= niches::SILL - 0.05 && up <= niches::SPRING, "{contents:?}: {up} up");
                assert!((-0.05..=niches::DEPTH).contains(&deep), "{contents:?}: {deep} deep");
            }
        }
    }

    #[test]
    fn a_candle_lights_its_recess_and_the_wall_round_it_from_just_off_the_wall() {
        let (spot, level) = &one_of_each()[&NicheContents::Candle];
        let (mesh, _, flame, _) = contents_of(spot, level.ceiling).unwrap();
        let at = on_the_sill(spot, mesh.bounds().unwrap(), Some(CANDLE_HEIGHT)).transform_point(flame.unwrap());
        let (dx, dy) = spot.facing.step();
        let [px, _, pz] = Level::world(spot.face().0, spot.face().1);
        // The flame stands in the recess, between sill and arch.
        let deep = -(at - Vec3::new(px, 0.0, pz)).dot(Vec3::new(-dx as f32, 0.0, -dy as f32));
        assert!(deep > 0.0 && deep < niches::DEPTH && at.y > niches::SILL && at.y < niches::SPRING, "{at:?}");
        let tuning = LightTuning::default();
        let light = niche_light(&tuning);
        assert!(!light.shadow_maps_enabled, "a niche's candle takes no slot of the shadow budget");
        assert!(light.range >= 12.0 && light.range <= fb_level::CELL * 1.5, "reaches a cell at most: {}", light.range);
    }
}
