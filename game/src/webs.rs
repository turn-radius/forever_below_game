//! The webs hung in a spider's nest (`fb_level::WebSpot`), drawn from `fb_level::webs`.
//!
//! - Each web is one Armory part of thin cord threads, set where its spot says: a corner
//!   sheet from the floor up to its top, a ceiling strand anchored in the vault
//!   (`geometry::stone::ceiling_at`), a floor strand low across the floor
//!   (`geometry::Relief::floor_height`).
//! - They are drawn pale and a little see-through, toon-lit by the level's lights like
//!   everything else, with an ink line thin and light enough not to blacken a 3 mm thread
//!   ([`web_appearance`]).
//! - They are laid out with the level and again when the level changes, and scoped to the
//!   delve as every level piece is. They are fixed by the level's seed, so they stand as they
//!   were when the delver leaves and returns.
//! - What they do to a walker's pace is `fb_level::webs::web_pace`, read where the delver
//!   (`crate::player::walk`) and the creatures (`crate::creatures::behaviour`) move.

use bevy::prelude::*;
use fb_level::webs::web_mesh;
use fb_level::{Level, WebSpot};

use crate::level::{
    LevelPiece,
    geometry::{Relief, stone::ceiling_at},
};
use crate::play::{CurrentLevel, DelveSystems};
use crate::render::{ArmoryPart, ItemAppearance, ItemSpawner, OutlineMaterial, ToonMaterial};

pub struct WebsPlugin;

impl Plugin for WebsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                hang_webs.run_if(resource_exists_and_changed::<CurrentLevel>),
                veil_webs,
            )
                .chain()
                .in_set(DelveSystems),
        );
    }
}

/// A web hanging in the level: which of `CurrentLevel.webs` it is.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct WebPiece {
    pub index: usize,
    pub spot: WebSpot,
}

/// How much of the light behind a thread it stops: our own pick, thin pale cord that lets the
/// stone show through.
pub const WEB_OPACITY: f32 = 0.6;

/// The look of a web: plain Armory material with a thin, light ink line, our own, so the
/// threads stay pale (the house ink would swallow a thread 3 mm across).
pub fn web_appearance(seed: u32) -> ItemAppearance {
    ItemAppearance {
        seed,
        outline: OutlineMaterial {
            color: Color::srgb(0.34, 0.37, 0.40).into(),
            width_px: 0.5,
            max_width: 0.02,
            see_through: false,
        },
        ..default()
    }
}

/// The webs' transform: on the floor under the spot, turned to its yaw.
pub fn web_transform(relief: &Relief, spot: &WebSpot) -> Transform {
    let [x, _, z] = Level::world(spot.x, spot.y);
    Transform::from_xyz(x, relief.floor_height(x, z), z)
        .with_rotation(Quat::from_rotation_y(spot.yaw))
}

/// Spawns every web the level has, clearing the last level's.
fn hang_webs(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    old: Query<Entity, With<WebPiece>>,
    mut spawner: ItemSpawner,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let level = &level.0;
    let relief = Relief::new(level);
    for (index, spot) in level.webs.iter().enumerate() {
        let transform = web_transform(&relief, spot);
        // A little into the vault, so a strand is anchored in the stone and not under it.
        let roof = ceiling_at(level, transform.translation.x, transform.translation.z)
            - transform.translation.y
            + 0.3;
        let mesh = web_mesh(spot, roof);
        spawner.spawn(&mesh, &web_appearance(index as u32)).insert((
            LevelPiece,
            WebPiece { index, spot: *spot },
            Name::new(format!("{:?} web", spot.kind)),
            transform,
        ));
    }
}

/// Makes the threads of newly laid webs see-through.
fn veil_webs(
    parts: Query<(&MeshMaterial3d<ToonMaterial>, &ChildOf), Added<ArmoryPart>>,
    webs: Query<(), With<WebPiece>>,
    mut materials: ResMut<Assets<ToonMaterial>>,
) {
    for (material, parent) in &parts {
        if !webs.contains(parent.parent()) {
            continue;
        }
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.base.base_color = material.base.base_color.with_alpha(WEB_OPACITY);
            material.base.alpha_mode = AlphaMode::Blend;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A level's webs are spawned once, for levels that have them.
    fn webbed(level: fb_level::Level) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<ToonMaterial>()
            .init_asset::<OutlineMaterial>()
            .init_resource::<crate::render::OutlineMaterials>()
            .insert_resource(CurrentLevel(level))
            .add_systems(
                Update,
                hang_webs.run_if(resource_exists_and_changed::<CurrentLevel>),
            );
        app.update();
        app
    }

    fn count(app: &mut App) -> usize {
        app.world_mut()
            .query::<&WebPiece>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn webs_spawn_only_for_levels_that_have_them() {
        let nest = (1..40)
            .map(|seed| fb_level::generate(seed, 3))
            .find(|l| !l.webs.is_empty())
            .expect("a nest within 40 seeds");
        let webs = nest.webs.len();
        let bare = (1..40)
            .map(|seed| fb_level::generate(seed, 1))
            .find(|l| l.webs.is_empty())
            .expect("a level with no webs");
        assert_eq!(count(&mut webbed(nest)), webs);
        assert_eq!(count(&mut webbed(bare)), 0);
    }

    #[test]
    fn a_web_is_scoped_to_the_level() {
        let nest = (1..40)
            .map(|seed| fb_level::generate(seed, 3))
            .find(|l| !l.webs.is_empty())
            .unwrap();
        let mut app = webbed(nest);
        let mut pieces = app.world_mut().query_filtered::<&bevy::state::state_scoped::DespawnOnExit<crate::launch::AppState>, With<WebPiece>>();
        assert!(pieces.iter(app.world()).count() > 0);
    }
}
