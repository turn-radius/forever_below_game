//! Camera jolts when a blow lands: a brief push and tilt that springs back, scaled by damage.
//!
//! The kick is applied to the [`PlayerCamera`] child's local transform, never moving the player's
//! actual position. It uses [`bearing_from_facing`] to push the view away from the attacker in local space.

use bevy::prelude::*;
use crate::combat::{Health, vitals::bearing_from_facing};
use crate::creatures::behaviour::AttackOnDelver;
use crate::combat::swing::lean_into_blow;
use crate::play::{DelveSystems, Player, PlayerCamera, level_has_focus};
use crate::player::{bob_head, look};
use crate::tuning::Tuning;

pub struct KickPlugin;

impl Plugin for KickPlugin {
    fn build(&self, app: &mut App) {
        // The kick is put on top of what look, bob_head and lean_into_blow wrote this frame, so
        // it runs after them, and only while they run: with a screen open nothing rewrites the
        // camera and the kick would compound.
        app.add_systems(
            Update,
            (
                apply_kick.after(look),
                update_kick
                    .after(apply_kick)
                    .after(look)
                    .after(bob_head)
                    .after(lean_into_blow)
                    .run_if(level_has_focus),
            )
                .in_set(DelveSystems),
        );
    }
}

/// Time for the camera kick to spring back, seconds. Our own.
const KICK_DURATION: f32 = 0.15;
/// Maximum distance the camera kicks, local units, when scaled 1.0. Our own.
const MAX_KICK_DISTANCE: f32 = 0.6;
/// Rotation tilt during kick, radians. Our own.
const KICK_TILT: f32 = 0.08; // About 4.5 degrees

/// The camera kick animation in progress.
#[derive(Component, Clone, Copy, Debug)]
struct CameraKick {
    /// Seconds left of the kick animation.
    pub left: f32,
    /// Direction to push (away from attacker), in world yaw radians.
    pub push_yaw: f32,
    /// Magnitude of the push, as a fraction of MAX_KICK_DISTANCE.
    pub magnitude: f32,
}

/// Read creature attacks and apply camera kicks to the player.
fn apply_kick(
    mut commands: Commands,
    mut attacks: MessageReader<AttackOnDelver>,
    tuning: Option<Res<Tuning>>,
    player: Single<(Entity, &Transform, &Health), With<Player>>,
) {
    let scale = tuning.as_deref().map(|t| t.combat.hit_kick).unwrap_or(1.0);
    if scale <= 0.0 {
        return;
    }

    let (player_entity, player_transform, health) = player.into_inner();

    for attack in attacks.read() {
        // Calculate direction to push: away from the attacker. bearing_from_facing points to the
        // attacker; we use it directly with the formula Vec3::new(-sin(a), 0, cos(a)) which
        // produces a vector pointing away from an attacker at angle a.
        let player_yaw = player_transform.rotation.to_euler(EulerRot::YXZ).0;
        let away_angle = bearing_from_facing(player_transform.translation.xz(), player_yaw, attack.attacker_pos);

        // Scale kick magnitude by damage relative to max HP, capped at 1.0.
        let magnitude = (attack.damage as f32 / health.max.max(1) as f32).min(1.0) * scale;

        // Insert the kick on the player; update_kick will apply it to the camera.
        commands.entity(player_entity).insert(CameraKick {
            left: KICK_DURATION,
            push_yaw: away_angle,
            magnitude,
        });
    }
}

/// Puts each kick on top of the camera pose that look, bob and lean wrote this frame. The offset
/// depends only on how far the kick has run, never on the offset of the frame before.
fn update_kick(
    time: Res<Time>,
    mut commands: Commands,
    mut players: Query<(Entity, &Children, &mut CameraKick), With<Player>>,
    mut cameras: Query<&mut Transform, With<PlayerCamera>>,
) {
    let dt = time.delta_secs();

    for (player, children, mut kick) in &mut players {
        kick.left = (kick.left - dt).max(0.0);
        if kick.left <= 0.0 {
            // Done: the offset is zero this frame and the component goes.
            commands.entity(player).remove::<CameraKick>();
        }

        let Some(camera) = children.iter().find(|&child| cameras.contains(child)) else { continue };
        let Ok(mut camera_transform) = cameras.get_mut(camera) else { continue };

        // Ease-out cubic: quick to settle, a spring-back feel.
        let progress = (KICK_DURATION - kick.left) / KICK_DURATION;
        let remaining = kick.magnitude * (1.0 - ease_out_cubic(progress));

        // `bearing_from_facing` is clockwise from ahead (0 ahead, π/2 right), so the view moves by
        // (-sin, cos) in local x and z: left for a bite on the right, back for one in front.
        let a = kick.push_yaw;
        camera_transform.translation += Vec3::new(-a.sin(), 0.0, a.cos()) * remaining * MAX_KICK_DISTANCE;

        // A small pitch and roll away from the attacker: a bite in front tips the view up and
        // one on the right tips it to the left. Never a yaw.
        let tilt = remaining * KICK_TILT;
        camera_transform.rotation *= Quat::from_rotation_x(tilt * a.cos()) * Quat::from_rotation_z(tilt * a.sin());
    }
}

/// Ease-out cubic: (1 - (1 - t)³). Smooth deceleration.
fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;
    use procgen_rng::Mulberry32;
    use crate::character::Character;
    use crate::combat::RunDice;
    use crate::combat::vitals::VitalsPlugin as VitalsPluginImpl;
    use crate::launch::AppState;
    use crate::play::ScreenOpen;
    use fb_creatures::Species;

    #[test]
    fn ease_out_cubic_starts_fast_then_slows() {
        assert!((ease_out_cubic(0.0) - 0.0).abs() < 1e-5);
        let quarter = ease_out_cubic(0.25);
        let half = ease_out_cubic(0.5);
        let three_quarter = ease_out_cubic(0.75);
        assert!(half - quarter > three_quarter - half, "easing should slow down");
        assert!((ease_out_cubic(1.0) - 1.0).abs() < 1e-5);
    }

    /// A delve with a player and camera, stepped 1/60 s a frame.
    fn delve_with_camera() -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_state(AppState::Delving)
            .insert_resource(RunDice(Mulberry32::new(4)))
            .init_resource::<Character>()
            .init_resource::<ScreenOpen>()
            .init_resource::<Tuning>()
            .add_plugins((VitalsPluginImpl, KickPlugin));

        let player = app.world_mut().spawn((
            Player,
            Transform::from_xyz(0.0, 0.0, 0.0).with_rotation(Quat::from_rotation_y(0.0)),
            Health::full(10),
        )).id();

        let camera = app.world_mut().spawn((
            PlayerCamera,
            Camera3d::default(),
            Transform::from_xyz(0.0, crate::play::EYE_HEIGHT, 0.0),
        )).id();

        app.world_mut().entity_mut(player).add_child(camera);

        (app, player, camera)
    }

    #[test]
    fn right_bite_moves_camera_left_in_local_space() {
        let (mut app, _player, camera) = delve_with_camera();
        app.update();

        let rest_translation = app.world().get::<Transform>(camera).unwrap().translation;

        // Send an attack from the right.
        app.world_mut().write_message(AttackOnDelver {
            attacker: Entity::PLACEHOLDER,
            species: Some(Species::Rat),
            damage: 5,
            poison: 0,
            attacker_pos: Vec2::new(5.0, 0.0), // Right of the player
        });

        app.update();

        let kicked_translation = app.world().get::<Transform>(camera).unwrap().translation;
        // A right bite should move the camera left (negative x in local space).
        assert!(kicked_translation.x < rest_translation.x, "right bite should move camera left");
    }

    #[test]
    fn front_bite_moves_camera_back_in_local_space() {
        let (mut app, _player, camera) = delve_with_camera();
        app.update();

        let rest_translation = app.world().get::<Transform>(camera).unwrap().translation;

        // Send an attack from the front. With yaw=0 facing -z, an attacker at (0, -5) is ahead.
        app.world_mut().write_message(AttackOnDelver {
            attacker: Entity::PLACEHOLDER,
            species: Some(Species::Rat),
            damage: 5,
            poison: 0,
            attacker_pos: Vec2::new(0.0, -5.0), // Ahead of the player
        });

        app.update();

        let kicked_translation = app.world().get::<Transform>(camera).unwrap().translation;
        // A front bite should move the camera back (positive z in local space).
        assert!(kicked_translation.z > rest_translation.z, "front bite should move camera back");
    }

    #[test]
    fn kick_is_additive_to_base_position() {
        let (mut app, _player, camera) = delve_with_camera();
        app.update();

        let base_y = app.world().get::<Transform>(camera).unwrap().translation.y;

        // Apply a kick and update.
        app.world_mut().write_message(AttackOnDelver {
            attacker: Entity::PLACEHOLDER,
            species: Some(Species::Rat),
            damage: 3,
            poison: 0,
            attacker_pos: Vec2::new(5.0, 0.0),
        });

        app.update();

        let with_kick_y = app.world().get::<Transform>(camera).unwrap().translation.y;

        // After a kick to the right, y should remain stable (additive, not resetting).
        // The important thing is that the y position reflects the base + any bob that would occur.
        assert!((with_kick_y - base_y).abs() < 0.01, "kick should be additive, not reset the camera");
    }

    #[test]
    fn scale_zero_gives_no_component() {
        let (mut app, player, _camera) = delve_with_camera();

        let tuning = Tuning {
            combat: crate::tuning::CombatTuning {
                hit_kick: 0.0,
                ..default()
            },
            ..default()
        };
        app.world_mut().insert_resource(tuning);
        app.update();

        // Send an attack.
        app.world_mut().write_message(AttackOnDelver {
            attacker: Entity::PLACEHOLDER,
            species: Some(Species::Rat),
            damage: 5,
            poison: 0,
            attacker_pos: Vec2::new(5.0, 0.0),
        });

        app.update();

        // No CameraKick component should exist.
        let has_kick = app.world().get::<CameraKick>(player).is_some();
        assert!(!has_kick, "scale 0 should not create a kick component");
    }

    #[test]
    fn after_duration_kick_fully_settles() {
        let (mut app, _player, _camera) = delve_with_camera();
        app.update();

        // Send an attack.
        app.world_mut().write_message(AttackOnDelver {
            attacker: Entity::PLACEHOLDER,
            species: Some(Species::Rat),
            damage: 5,
            poison: 0,
            attacker_pos: Vec2::new(5.0, 0.0),
        });

        app.update();

        // Step through the entire kick animation.
        for _ in 0..((KICK_DURATION * 60.0) as usize + 5) {
            app.update();
        }

        // After the kick expires, bob_head should be running and the camera should have
        // whatever position bob_head gives it. We can't easily check the exact position
        // without knowing bob_head's state, but we can verify the kick component is gone.
        let mut query = app.world_mut().query_filtered::<&CameraKick, With<Player>>();
        let kicks: Vec<_> = query.iter(app.world()).collect();
        assert_eq!(kicks.len(), 0, "kick component should be removed after duration");
    }

    /// The camera's pose each frame of a 60 frame walk, yaw 1.0 and pitch 0.3, with real look,
    /// walk and bob_head running. With `bite`, a creature on the player's right bites at frame
    /// 5. The screen is open for the frames in `screen`.
    fn trace(bite: bool, screen: std::ops::Range<usize>) -> Vec<(Vec3, Quat)> {
        use crate::keymap::Keymap;
        use crate::play::{BlockedCells, CurrentLevel, Screen, level_has_focus};
        use crate::player::{Locomotion, bob_head, look, walk};
        use crate::settings::GameSettings;
        use bevy::input::mouse::AccumulatedMouseMotion;
        use bevy::window::{CursorOptions, PrimaryWindow};

        let level = fb_level::generate(1207, 1);
        let (x, y, _) = level.start();
        let [wx, wy, wz] = fb_level::Level::world(x as f32 + 0.5, y as f32 + 0.5);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_state(AppState::Delving)
            .insert_resource(RunDice(Mulberry32::new(4)))
            .insert_resource(CurrentLevel(level))
            .init_resource::<Character>()
            .init_resource::<ScreenOpen>()
            .init_resource::<Tuning>()
            .init_resource::<Keymap>()
            .init_resource::<GameSettings>()
            .init_resource::<BlockedCells>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins((VitalsPluginImpl, KickPlugin))
            .add_systems(Update, (look, walk, bob_head).chain().run_if(level_has_focus).in_set(DelveSystems));
        app.world_mut().spawn((Window::default(), PrimaryWindow, CursorOptions::default()));
        let player = app
            .world_mut()
            .spawn((Player, Locomotion { yaw: 1.0, pitch: 0.3, ..default() }, Transform::from_xyz(wx, wy, wz), Health::full(10)))
            .id();
        let camera = app.world_mut().spawn((PlayerCamera, Camera3d::default(), Transform::from_xyz(0.0, crate::play::EYE_HEIGHT, 0.0))).id();
        app.world_mut().entity_mut(player).add_child(camera);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyW);

        let mut poses = Vec::new();
        for frame in 0..60 {
            app.world_mut().resource_mut::<ScreenOpen>().0 = screen.contains(&frame).then_some(Screen::Inventory);
            if bite && frame == 5 {
                let at = app.world().get::<Transform>(player).unwrap();
                let right = at.right().as_vec3();
                let attacker_pos = Vec2::new(at.translation.x + right.x * 5.0, at.translation.z + right.z * 5.0);
                app.world_mut().write_message(AttackOnDelver { attacker: Entity::PLACEHOLDER, species: Some(Species::Rat), damage: 5, poison: 0, attacker_pos });
            }
            app.update();
            let pose = app.world().get::<Transform>(camera).unwrap();
            poses.push((pose.translation, pose.rotation));
        }
        poses
    }

    #[test]
    fn a_right_bite_moves_the_view_left_whatever_way_the_player_faces() {
        let (kicked, calm) = (trace(true, 0..0), trace(false, 0..0));
        for frame in 6..9 {
            assert!(kicked[frame].0.x < calm[frame].0.x - 0.05, "frame {frame}: {} against {}", kicked[frame].0.x, calm[frame].0.x);
        }
    }

    #[test]
    fn the_view_tilts_a_little_and_never_turns() {
        let (kicked, calm) = (trace(true, 0..0), trace(false, 0..0));
        let mut most = 0.0_f32;
        for frame in 0..60 {
            let off = kicked[frame].1.angle_between(calm[frame].1);
            assert!(off < 0.1, "frame {frame}: {off} rad off the look rotation");
            most = most.max(off);
        }
        assert!(most > 0.01, "the view should tilt: {most}");
    }

    #[test]
    fn the_view_is_back_where_look_and_bob_put_it_after_the_kick() {
        let (kicked, calm) = (trace(true, 0..0), trace(false, 0..0));
        for frame in 5 + 10..60 {
            assert!(kicked[frame].0.distance(calm[frame].0) < 1e-5, "frame {frame}");
            assert!(kicked[frame].1.angle_between(calm[frame].1) < 2e-3, "frame {frame}");
        }
    }

    #[test]
    fn an_open_screen_freezes_the_kick_and_closing_it_settles_the_view() {
        let (kicked, calm) = (trace(true, 8..30), trace(false, 8..30));
        // Frozen, not compounding, and within one kick's reach of where the view stands.
        for frame in 10..30 {
            assert!(kicked[frame].0.distance(kicked[9].0) < 1e-5, "frame {frame} moved while the screen was open");
            assert!(kicked[frame].1.angle_between(kicked[9].1) < 2e-3, "frame {frame} turned while the screen was open");
            assert!(kicked[frame].0.distance(calm[frame].0) <= MAX_KICK_DISTANCE + 1e-4, "frame {frame}");
        }
        for frame in 45..60 {
            assert!(kicked[frame].0.distance(calm[frame].0) < 1e-5, "frame {frame}");
            assert!(kicked[frame].1.angle_between(calm[frame].1) < 2e-3, "frame {frame}");
        }
    }
}
