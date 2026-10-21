//! Which cells of the level the delver has seen ([`fb_level::sight`]), for the map. A few
//! times a second the eye looks: from the player camera, as wide as it sees, with every light in
//! the level as a lamp (the carried lights, braziers, glowcaps and lit things on the floor),
//! each lighting the cells within most of its range. What is seen stays seen for the run on
//! that level.

use std::collections::HashMap;

use bevy::{camera::visibility::RenderLayers, prelude::*};
use fb_level::sight::{Explored, Eye, Lamp};
use fb_level::{CELL, Level};

use crate::play::{CurrentLevel, DelveSystems, PlayerCamera};

/// How often the eye looks, seconds. Our own: often enough that a glance into a room maps it.
pub const LOOK_EVERY: f32 = 0.12;
/// How far the eye sees a lit cell, in cells (a brazier across a long hall). Our own.
pub const EYE_RANGE: f32 = 30.0;
/// The share of a light's range that counts as lit: its range runs out into the dim fringe.
/// Our own.
pub const LIT_SHARE: f32 = 0.9;

/// The cells seen of each level visited this run, by level seed.
#[derive(Resource, Clone, Debug, Default)]
pub struct ExploredLevels(pub HashMap<u32, Explored>);

impl ExploredLevels {
    /// What has been seen of `level`, nothing yet if it is new.
    pub fn of(&mut self, level: &Level) -> &mut Explored {
        self.0.entry(level.seed).or_insert_with(|| Explored::new(level))
    }

    pub fn get(&self, level: &Level) -> Option<&Explored> {
        self.0.get(&level.seed)
    }
}

pub(super) fn add(app: &mut App) {
    app.init_resource::<ExploredLevels>()
        .add_systems(OnEnter(crate::launch::AppState::Delving), forget_the_last_run)
        .add_systems(Update, (look, explore_for_capture.run_if(|| std::env::var("FB_EXPLORE_ALL").is_ok())).in_set(DelveSystems));
}

/// For captures: `FB_EXPLORE_ALL=1` marks every open cell of the level as seen.
fn explore_for_capture(level: Option<Res<CurrentLevel>>, mut explored: ResMut<ExploredLevels>) {
    let Some(level) = level else { return };
    explored.0.insert(level.0.seed, Explored::all(&level.0));
}

/// A new delve is a new run: nothing of any level has been seen.
fn forget_the_last_run(mut explored: ResMut<ExploredLevels>) {
    explored.0.clear();
}

/// A point in the world as a point on the level's grid, in cells.
pub fn to_grid(at: Vec3) -> (f32, f32) {
    (at.x / CELL, at.z / CELL)
}

/// Half the camera's horizontal field of view, radians, for its vertical `fov` and `aspect`.
pub fn half_horizontal_fov(fov: f32, aspect: f32) -> f32 {
    ((fov * 0.5).tan() * aspect).atan()
}

/// The lamp a light in the world is, on the grid; `None` for one off the level (on the view
/// model's stage or another stage).
pub fn lamp_of(at: Vec3, range: f32, level: &Level) -> Option<Lamp> {
    let (x, y) = to_grid(at);
    let on_level = x >= 0.0 && y >= 0.0 && x < level.width as f32 && y < level.height as f32 && at.y.abs() < level.ceiling * 4.0;
    on_level.then_some(Lamp { x, y, reach: range * LIT_SHARE / CELL })
}

#[allow(clippy::type_complexity)]
fn look(
    time: Res<Time>,
    level: Option<Res<CurrentLevel>>,
    mut explored: ResMut<ExploredLevels>,
    eyes: Query<(&GlobalTransform, Option<&Projection>), With<PlayerCamera>>,
    lights: Query<(&GlobalTransform, &PointLight, Option<&RenderLayers>)>,
    mut since: Local<f32>,
) {
    *since += time.delta_secs();
    if *since < LOOK_EVERY {
        return;
    }
    *since = 0.0;
    let Some(level) = level else { return };
    let Some((eye, projection)) = eyes.iter().next() else { return };
    let half_fov = match projection {
        Some(Projection::Perspective(p)) => half_horizontal_fov(p.fov, p.aspect_ratio),
        _ => 1.0,
    };
    let (x, y) = to_grid(eye.translation());
    let forward = eye.forward().as_vec3();
    let eye = Eye { x, y, forward: (forward.x, forward.z), half_fov, range: EYE_RANGE };
    let lamps: Vec<Lamp> = lights
        .iter()
        .filter(|(_, _, layers)| layers.is_none_or(|l| l.intersects(&RenderLayers::default())))
        .filter_map(|(at, light, _)| lamp_of(at.translation(), light.range, &level.0))
        .collect();
    explored.of(&level.0).look(&level.0, &eye, &lamps);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_view_sees_wider_than_it_is_tall() {
        let half = half_horizontal_fov(70f32.to_radians(), 16.0 / 9.0);
        assert!(half > 35f32.to_radians() && half < 60f32.to_radians(), "{}", half.to_degrees());
    }

    #[test]
    fn lights_off_the_level_are_no_lamps() {
        let level = fb_level::generate(1207, 1);
        let inside = Vec3::new(10.0 * CELL, 15.0, 10.0 * CELL);
        let lamp = lamp_of(inside, 100.0, &level).unwrap();
        assert_eq!((lamp.x, lamp.y), (10.0, 10.0));
        assert!((lamp.reach - 100.0 * LIT_SHARE / CELL).abs() < 1e-5);
        assert!(lamp_of(crate::carry::VIEW_MODEL_STAGE, 100.0, &level).is_none(), "the view model's stage");
        assert!(lamp_of(Vec3::new(-50.0, 0.0, 10.0), 100.0, &level).is_none());
    }

    #[test]
    fn standing_at_the_start_with_a_torch_maps_the_first_room() {
        let mut app = App::new();
        let level = fb_level::generate(1207, 1);
        let (sx, sy, inward) = level.start();
        let (dx, dy) = inward.step();
        // Three cells in from the door, looking in, a torch in hand.
        let at = Vec3::new((sx as f32 + 0.5 + dx as f32 * 3.0) * CELL, 15.0, (sy as f32 + 0.5 + dy as f32 * 3.0) * CELL);
        let looking = at + Vec3::new(dx as f32, 0.0, dy as f32);
        app.add_plugins(MinimalPlugins)
            .add_plugins(bevy::state::app::StatesPlugin)
            .insert_state(crate::launch::AppState::Delving)
            .insert_resource(CurrentLevel(level.clone()))
            .init_resource::<ExploredLevels>()
            .add_systems(Update, look);
        app.world_mut().spawn((PlayerCamera, GlobalTransform::from(Transform::from_translation(at).looking_at(looking, Vec3::Y))));
        app.world_mut().spawn((GlobalTransform::from_translation(at), PointLight { range: 7.0 * CELL, ..default() }));
        for _ in 0..3 {
            std::thread::sleep(std::time::Duration::from_secs_f32(LOOK_EVERY));
            app.update();
        }
        let explored = app.world().resource::<ExploredLevels>().get(&level).unwrap();
        let room = level.rooms[level.start_room().unwrap()];
        let (cx, cy) = (room.x as i32 + room.w as i32 / 2, room.y as i32 + room.h as i32 / 2);
        assert!(explored.count() > 4, "{}", explored.count());
        assert!(explored.is_seen(cx, cy) || explored.is_seen((at.x / CELL) as i32, (at.z / CELL) as i32));
    }
}
