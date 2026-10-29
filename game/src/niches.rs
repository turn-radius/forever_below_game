//! What the level's niches hold (`fb_level::NicheSpot`): a lit candle, a skull, a scatter of
//! bones or a small urn, standing on the sill of the recess the stone cuts into the wall
//! (`level::geometry::stone`).
//!
//! An urn is a container, opened with the pick-up key like a chest: it is kept in the
//! [`Inventory`] as the amphora it is, holding what its niche's seed rolls
//! (`fb_items::roll_urn_contents`), and what is taken stays taken in the level record
//! (`record::LevelRecord::urns`), across leaving and returning and across a save.
//!
//! Each is built by the generator that already draws it: the Armory's candle, the skull and
//! bones of the level's scenery, and the amphora of its containers, fitted to the recess and
//! turned to face out of it. None blocks the way, and only an urn can be opened. A candle
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
use crate::inventory::Inventory;
use crate::level::{EXPOSURE, LevelPiece, fixture_appearance};
use crate::light::{Torch, point_light};
use crate::play::CurrentLevel;
use crate::props::Container;
use crate::record::{WorldRecord, restore_contents};
use crate::render::{ItemAppearance, ItemSpawner};
use crate::tuning::{LightTuning, Tuning};

/// A niche's contents standing in the level: which of `CurrentLevel.niches` it belongs to.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct NichePiece {
    pub index: usize,
}

/// On an urn in a niche, which is a [`Container`] too: its item-space bounds, for looking at it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct NicheUrn {
    pub min: Vec3,
    pub max: Vec3,
}

/// The room a thing has in a niche, in world units: across, up and deep.
const ROOM: [f32; 3] = [4.6, 4.2, 3.2];
/// How long the longest side of what a niche holds is drawn, in world units, whatever its own
/// size: an urn, a candle with its holder, and a skull.
const URN_HEIGHT: f32 = 4.0;
const CANDLE_HEIGHT: f32 = 3.4;
const SKULL_LENGTH: f32 = 3.6;
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
    let fit = (ROOM[0] / size.x.max(1e-3)).min((spot.spring - spot.sill).clamp(2.5, ROOM[1]) / size.y.max(1e-3)).min(ROOM[2] / size.z.max(1e-3));
    let scale = longest.map_or(fit.min(1.0), |l| (l / size.max_element().max(1e-3)).min(fit)).max(0.05);
    let (dx, dy) = spot.facing.step();
    // Out of the wall, into the room.
    let out = Vec3::new(-dx as f32, 0.0, -dy as f32);
    let yaw = Quat::from_rotation_y(out.x.atan2(out.z));
    let [px, _, pz] = Level::world(spot.x as f32 + 0.5 + dx as f32 * 0.5, spot.y as f32 + 0.5 + dy as f32 * 0.5);
    let wall = Vec3::new(px, 0.0, pz);
    let middle = Vec3::new((lo.x + hi.x) * 0.5, lo.y, (lo.z + hi.z) * 0.5) * scale;
    // The middle of the recess, half its depth in.
    let seat = wall - out * niches::DEPTH * 0.5 + Vec3::Y * spot.sill;
    Transform { translation: seat - yaw * middle, rotation: yaw, scale: Vec3::splat(scale) }
}

/// The light of a candle burning in a niche: warm, flickering as every flame does, small, and
/// unshadowed.
pub fn niche_light(tuning: &LightTuning) -> PointLight {
    Torch::flickering(point_light(Torch::COLOR, tuning.niche_peak, tuning.niche_reach, &EXPOSURE), tuning.flicker_depth)
}

/// Keeps track of the urn in `spot` in `inventory`, loose, holding what was `left` in it if the
/// level was visited before, and else what its niche's seed rolls against `odds`.
pub fn stock_urn(inventory: &mut Inventory, spot: &NicheSpot, odds: fb_items::Odds, left: Option<&[crate::record::Stowed]>) -> crate::inventory::ItemId {
    let container = inventory.add(fb_items::container_item(chest_state(ChestStyle::Amphora, spot.seed, false)));
    match left {
        Some(left) => restore_contents(inventory, container, left),
        None => crate::props::fill_container(inventory, container, fb_items::roll_urn_contents(spot.seed, odds)),
    }
    container
}

/// Spawns (or respawns, when the level changes) what the level's niches hold.
#[allow(clippy::too_many_arguments)]
pub fn lay_out_niches(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    tuning: Option<Res<Tuning>>,
    old: Query<(Entity, Option<&Container>), With<NichePiece>>,
    mut inventory: ResMut<Inventory>,
    record: Option<Res<WorldRecord>>,
    character: Option<Res<crate::character::Character>>,
    mut spawner: ItemSpawner,
) {
    for (entity, container) in &old {
        if let Some(container) = container {
            inventory.forget(container.id);
        }
        commands.entity(entity).despawn();
    }
    let light = tuning.map(|t| t.light.clone()).unwrap_or_default();
    let odds = crate::character::loot_odds(character.as_deref(), Some(&inventory), level.0.depth);
    let saved = record.as_deref().and_then(|r| r.level(level.0.depth));
    for (index, spot) in level.0.niches.iter().enumerate() {
        let Some((mesh, appearance, flame, longest)) = contents_of(spot, level.0.ceiling) else { continue };
        let Some(bounds) = mesh.bounds() else { continue };
        let transform = on_the_sill(spot, bounds, longest);
        let mut root = spawner.spawn(&mesh, &appearance);
        root.insert((LevelPiece, Name::new(format!("niche {:?}", spot.contents)), transform, NichePiece { index }));
        if spot.contents == NicheContents::Urn {
            let id = stock_urn(&mut inventory, spot, odds, saved.and_then(|l| l.urns.get(&index)).map(|c| c.contents.as_slice()));
            root.insert((Container { id, opened: false }, NicheUrn { min: Vec3::from_array(bounds.0), max: Vec3::from_array(bounds.1) }));
        }
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
    use crate::travel::Abyss;
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
                assert!(up >= spot.sill - 0.05 && up <= spot.spring, "{contents:?}: {up} up");
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
        assert!(deep > 0.0 && deep < niches::DEPTH && at.y > spot.sill && at.y < spot.spring, "{at:?}");
        let tuning = LightTuning::default();
        let light = niche_light(&tuning);
        assert!(!light.shadow_maps_enabled, "a niche's candle takes no slot of the shadow budget");
        assert!(light.range >= 12.0 && light.range <= fb_level::CELL * 1.5, "reaches a cell at most: {}", light.range);
    }

    #[test]
    fn an_urn_holds_what_its_niche_rolls_the_same_each_time_and_what_was_left_replaces_it() {
        let odds = fb_items::Odds::loot(2);
        let mut found = None;
        'seeds: for seed in 1..=120 {
            for spot in Abyss::new(seed).level_at(2).niches.iter().filter(|n| n.contents == NicheContents::Urn) {
                if !fb_items::roll_urn_contents(spot.seed, odds).is_empty() {
                    found = Some(*spot);
                    break 'seeds;
                }
            }
        }
        let spot = found.expect("an urn that holds something turns up");
        let mut inventory = Inventory::default();
        let kept = |inventory: &Inventory, urn| {
            inventory.grid(urn).unwrap().entries().iter().map(|e| (inventory.item(e.id).unwrap().base.clone(), inventory.item(e.id).unwrap().seed)).collect::<Vec<_>>()
        };
        let (a, b) = (stock_urn(&mut inventory, &spot, odds, None), stock_urn(&mut inventory, &spot, odds, None));
        assert!(!kept(&inventory, a).is_empty());
        assert_eq!(kept(&inventory, a), kept(&inventory, b), "the same niche rolls the same goods");
        let emptied = stock_urn(&mut inventory, &spot, odds, Some(&[]));
        assert!(kept(&inventory, emptied).is_empty(), "what was left is what is there, not what it rolled");
    }
}
