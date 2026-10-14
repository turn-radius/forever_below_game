//! The props standing along the level's walls (`fb_level::PropSpot`): containers and scenery.
//!
//! - Containers are Armory chests, barrels, crates and amphorae (`fb_level::chest_state`),
//!   drawn in their own palette, condition and etching as any item is. Scenery (rubble, bones,
//!   pillars) is built by `armory::build::scenery` and drawn as the level's fixtures are.
//! - Each rests on the drawn floor (`geometry::Relief::floor_height`) at its lowest point
//!   under it, turned to its spot's yaw, and is lit by the level's lights like everything else.
//! - Each blocks the way ([`BlocksTheWay`]) with its `fb_level::FloorShape`: a circle for
//!   round things, a rectangle turned with it for chests, crates and pillars.

use armory::{Item, build::scenery::Scenery, cond::ConditionState};
use bevy::prelude::*;
use fb_level::{CELL, Level, Prop, PropSpot, chest_state};

use crate::carry::{appearance_of, item_points};
use crate::level::{LevelPiece, fixture_appearance, geometry::Relief};
use crate::play::{BlocksTheWay, CurrentLevel};
use crate::render::ItemSpawner;

pub struct PropsPlugin;

impl Plugin for PropsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, lay_out_props.run_if(resource_exists_and_changed::<CurrentLevel>));
    }
}

/// A prop standing in the level: which of `CurrentLevel.props` it is, and its item-space
/// bounds for looking at it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PropPiece {
    pub index: usize,
    pub spot: PropSpot,
    pub min: Vec3,
    pub max: Vec3,
}

/// How far below the floor's lowest point under it a prop is set, so no seam of light shows
/// under it where the floor's relief rises and falls across its base.
const SINK: f32 = 0.05;

/// The prop's mesh and look. A container is shut until `opened`.
pub fn prop_mesh(prop: Prop, level: &Level, opened: bool) -> (armory::ItemMesh, crate::render::ItemAppearance) {
    match prop {
        Prop::Container { style, seed, .. } => {
            let item = Item::Chests(chest_state(style, seed, opened));
            (item.build(), appearance_of(&item))
        }
        Prop::Scenery { kind, seed } => {
            let state = if kind == Scenery::Bones { ConditionState::Neglected } else { ConditionState::BattleWorn };
            (kind.build(seed, level.ceiling), fixture_appearance(seed, state))
        }
    }
}

/// Where a prop of item-space `points` stands at `spot`: turned to its yaw, with its lowest
/// point at the lowest the drawn floor falls under its footprint, less [`SINK`].
pub fn prop_transform(level: &Level, spot: &PropSpot, points: &[Vec3]) -> Option<Transform> {
    let low = points.iter().map(|p| p.y).reduce(f32::min)?;
    let relief = Relief::new(level);
    let centre = Vec3::from_array(Level::world(spot.x, spot.y));
    let reach = spot.prop.floor_shape().radius();
    let floor = std::iter::once(Vec2::ZERO)
        .chain((0..8).map(|k| Vec2::from_angle(k as f32 * std::f32::consts::TAU / 8.0) * reach * 0.8))
        .map(|offset| relief.floor_height(centre.x + offset.x, centre.z + offset.y))
        .fold(f32::INFINITY, f32::min);
    Some(Transform::from_translation(Vec3::new(centre.x, floor - low - SINK, centre.z)).with_rotation(Quat::from_rotation_y(spot.yaw)))
}

/// Spawns prop `index` of the level, shut or `opened`. `None` for a prop with no vertices.
pub fn spawn_prop(spawner: &mut ItemSpawner, level: &Level, index: usize, opened: bool) -> Option<Entity> {
    let spot = *level.props.get(index)?;
    let (mesh, appearance) = prop_mesh(spot.prop, level, opened);
    let points = item_points(&mesh);
    let transform = prop_transform(level, &spot, &points)?;
    let (min, max) = points.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), &p| (lo.min(p), hi.max(p)));
    let name = match spot.prop {
        Prop::Container { style, .. } => format!("{style:?}"),
        Prop::Scenery { kind, .. } => format!("{kind:?}"),
    };
    let mut root = spawner.spawn(&mesh, &appearance);
    root.insert((LevelPiece, Name::new(name), transform, PropPiece { index, spot, min, max }, BlocksTheWay(spot.prop.floor_shape())));
    Some(root.id())
}

/// Spawns (or respawns, when the level changes) every prop the level lays out.
fn lay_out_props(mut commands: Commands, level: Res<CurrentLevel>, old: Query<Entity, With<PropPiece>>, mut spawner: ItemSpawner) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    for index in 0..level.0.props.len() {
        spawn_prop(&mut spawner, &level.0, index, false);
    }
}


/// A prop's centre on the floor plane (world `(x, z)`).
pub fn floor_centre(spot: &PropSpot) -> Vec2 {
    Vec2::new(spot.x, spot.y) * CELL
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::{Obstacle, Obstacles, gather_obstacles};
    use fb_level::FloorShape;
    use crate::player::{BODY_RADIUS, blocked_by, slide_around};

    /// An app that lays out the props of 1207, with no renderer.
    fn props_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::transform::TransformPlugin)
            .insert_resource(CurrentLevel(fb_level::generate(1207)))
            .init_resource::<Obstacles>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
            .init_resource::<crate::render::OutlineMaterials>()
            .add_plugins(PropsPlugin)
            .add_systems(PostUpdate, gather_obstacles.after(TransformSystems::Propagate));
        app.update();
        app
    }

    #[test]
    fn every_prop_on_1207_is_drawn() {
        let mut app = props_app();
        let level = fb_level::generate(1207);
        let mut pieces = app.world_mut().query::<(&PropPiece, &Children)>();
        let drawn: Vec<(usize, usize)> = pieces.iter(app.world()).map(|(p, children)| (p.index, children.len())).collect();
        assert_eq!(drawn.len(), level.props.len());
        assert!(drawn.iter().all(|&(_, parts)| parts > 0), "{drawn:?}");
        let mut meshes = app.world_mut().query::<&Mesh3d>();
        assert!(meshes.iter(app.world()).count() >= level.props.len());
    }

    #[test]
    fn no_prop_on_1207_floats_or_sinks() {
        let level = fb_level::generate(1207);
        let relief = Relief::new(&level);
        for (index, spot) in level.props.iter().enumerate() {
            let (mesh, _) = prop_mesh(spot.prop, &level, false);
            let points = item_points(&mesh);
            let transform = prop_transform(&level, spot, &points).unwrap();
            let bottom = points.iter().map(|&p| transform.transform_point(p).y).fold(f32::INFINITY, f32::min);
            let floor = relief.floor_height(transform.translation.x, transform.translation.z);
            assert!(bottom <= floor + 1e-3, "prop {index} floats: base {bottom}, floor {floor}");
            assert!(bottom > floor - 0.4, "prop {index} sinks: base {bottom}, floor {floor}");
        }
    }

    #[test]
    fn the_player_cannot_walk_through_props() {
        let app = props_app();
        let level = fb_level::generate(1207);
        let obstacles = app.world().resource::<Obstacles>().0.clone();
        assert_eq!(obstacles.len(), level.props.len());
        let blocked = crate::play::BlockedCells::default();
        for (spot, obstacle) in level.props.iter().zip(&obstacles) {
            let centre = floor_centre(spot);
            match (spot.prop.floor_shape(), obstacle) {
                (FloorShape::Round(r), Obstacle::Circle { centre: c, radius }) => assert!(c.abs_diff_eq(centre, 1e-3) && *radius == r),
                (FloorShape::Square { .. }, Obstacle::Rect { centre: c, .. }) => assert!(c.abs_diff_eq(centre, 1e-3)),
                other => panic!("{other:?}"),
            }
            // Walking straight at it from a step into the room: stopped short, never inside.
            let into_room = Vec2::new(spot.yaw.sin(), spot.yaw.cos());
            let mut at = centre + into_room * (spot.prop.floor_shape().radius() + BODY_RADIUS + 4.0);
            for _ in 0..120 {
                at = slide_around(at, at - into_room * 0.5, BODY_RADIUS, blocked_by(&level, &blocked), &obstacles);
                assert!(!obstacle.overlaps(at, BODY_RADIUS), "{spot:?}: walked into it at {at}");
            }
        }
    }
}
