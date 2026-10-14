//! The first-person body: camera, mouse look, walking and collision with the level's rock.
//!
//! The body is a circle on the floor plane that slides along rock ([`slide`]). Yaw turns the
//! [`Player`]; pitch tilts its [`PlayerCamera`] child, which also carries the head bob.

use bevy::{
    camera::Exposure,
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    render::view::Tonemapping,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use fb_level::{CELL, Level};

use crate::light::point_light;
use crate::play::{BlockedCells, CurrentLevel, EYE_HEIGHT, Player, PlayerCamera};

/// The body's radius on the floor in world units (35 cm).
pub const BODY_RADIUS: f32 = 3.5;
/// Walking pace in world units per second (1.4 m/s).
pub const WALK_SPEED: f32 = 14.0;
/// Hurrying pace with Shift held (2.5 m/s).
pub const HURRY_SPEED: f32 = 25.0;
/// How quickly velocity closes on the wanted velocity, per second ([`StableInterpolate`]).
const ACCELERATION: f32 = 10.0;
/// Radians of turn per pixel of mouse motion.
const LOOK_SENSITIVITY: f32 = 0.0022;
/// Pitch stops short of straight up and down.
const PITCH_LIMIT: f32 = 85.0 * std::f32::consts::PI / 180.0;
/// Vertical field of view (about 100 degrees across at 16:9).
const FOV: f32 = 70.0 * std::f32::consts::PI / 180.0;
/// Near clip in world units (5 mm), so items held at the face and the torch in hand stay whole.
const NEAR: f32 = 0.05;
/// Floor distance covered by one full bob cycle: two steps.
const STRIDE: f32 = 14.0;
/// Bob height and sway at walking pace, in world units (1.5 cm and 1 cm).
const BOB_HEIGHT: f32 = 0.15;
const BOB_SWAY: f32 = 0.1;
/// Frames longer than this are walked as this long, so a hitch does not fling the body.
const MAX_STEP_SECONDS: f32 = 0.1;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_player, grab_cursor))
            .add_systems(Update, (toggle_cursor_grab, look, walk, bob_head).chain());
    }
}

/// How the body is moving and looking. Yaw and pitch are kept here so the transforms can be
/// rebuilt from them without drift.
#[derive(Component, Debug, Default)]
#[require(Gait)]
struct Locomotion {
    yaw: f32,
    pitch: f32,
    /// Floor velocity, world units per second, as `(x, z)`.
    velocity: Vec2,
}

/// The player's step cycle, for whatever moves with their steps: the head bob here, the torch
/// in hand in [`crate::carry`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Gait {
    /// Step cycle, radians; one turn per [`STRIDE`] walked (two steps).
    pub phase: f32,
    /// Speed as a multiple of walking pace, 0 standing, up to 1.5 hurrying.
    pub pace: f32,
}

impl Gait {
    /// The step's bob as `(sideways, down)`: down at each footfall, swaying toward the stepping
    /// foot once a stride, both 1 at their peak at walking pace and scaling with speed, so the
    /// bob fades as the body slows and is still when it stands.
    pub fn bob(&self) -> Vec2 {
        Vec2::new(self.pace * self.phase.sin(), self.pace * (1.0 - (2.0 * self.phase).cos()) * 0.5)
    }
}

fn spawn_player(mut commands: Commands, level: Res<CurrentLevel>) {
    let (x, y, facing) = level.0.start();
    let [wx, wy, wz] = Level::world(x as f32 + 0.5, y as f32 + 0.5);
    let yaw = facing.yaw();
    let exposure = Exposure::INDOOR;
    commands
        .spawn((
            Player,
            Locomotion { yaw, ..default() },
            Transform::from_xyz(wx, wy, wz).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
        ))
        .with_children(|body| {
            let mut camera = body.spawn((
                PlayerCamera,
                Camera3d::default(),
                Projection::from(PerspectiveProjection { fov: FOV, near: NEAR, ..default() }),
                exposure,
                // Untonemapped, as the turntable draws, so the toon bands stay crisp.
                Tonemapping::Linear,
                // UI follows this camera even when frame capture retargets it to an image.
                IsDefaultUiCamera,
                Transform::from_xyz(0.0, EYE_HEIGHT, 0.0),
            ));
            // A dim light at the eye for looking at the level before the torch exists.
            if std::env::var_os("FB_DEBUG_LIGHT").is_some() {
                camera.with_child(point_light(Color::WHITE, 0.5, 200.0, &exposure));
            }
        });
}

fn grab_cursor(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    set_grab(&mut cursor, true);
}

/// Click to take the mouse, Esc to let it go.
fn toggle_cursor_grab(
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if mouse.just_pressed(MouseButton::Left) {
        set_grab(&mut cursor, true);
    }
    if keys.just_pressed(KeyCode::Escape) {
        set_grab(&mut cursor, false);
    }
}

fn set_grab(cursor: &mut CursorOptions, grabbed: bool) {
    cursor.visible = !grabbed;
    cursor.grab_mode = if grabbed { CursorGrabMode::Locked } else { CursorGrabMode::None };
}

fn look(
    motion: Res<AccumulatedMouseMotion>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    mut body: Single<(&mut Locomotion, &mut Transform), With<Player>>,
    mut camera: Single<&mut Transform, (With<PlayerCamera>, Without<Player>)>,
) {
    let (locomotion, transform) = &mut *body;
    if cursor.grab_mode != CursorGrabMode::None && motion.delta != Vec2::ZERO {
        locomotion.yaw -= motion.delta.x * LOOK_SENSITIVITY;
        locomotion.pitch = (locomotion.pitch - motion.delta.y * LOOK_SENSITIVITY).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }
    transform.rotation = Quat::from_rotation_y(locomotion.yaw);
    camera.rotation = Quat::from_rotation_x(locomotion.pitch);
}

fn walk(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    mut body: Single<(&mut Locomotion, &mut Gait, &mut Transform), With<Player>>,
) {
    let dt = time.delta_secs().min(MAX_STEP_SECONDS);
    if dt <= 0.0 {
        return;
    }
    let (locomotion, gait, transform) = &mut *body;
    let axis = |plus: KeyCode, minus: KeyCode| keys.pressed(plus) as i32 as f32 - keys.pressed(minus) as i32 as f32;
    let input = Vec2::new(axis(KeyCode::KeyD, KeyCode::KeyA), axis(KeyCode::KeyW, KeyCode::KeyS));
    let speed = if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) { HURRY_SPEED } else { WALK_SPEED };
    let (forward, right) = (floor(transform.forward().as_vec3()), floor(transform.right().as_vec3()));
    let wanted = (forward * input.y + right * input.x).normalize_or_zero() * speed;
    locomotion.velocity.smooth_nudge(&wanted, ACCELERATION, dt);

    let from = floor(transform.translation);
    let blocked = blocked_by(&level.0, &blocked_cells);
    let to = slide(from, from + locomotion.velocity * dt, BODY_RADIUS, blocked);
    // Walking into a wall leaves only the speed along it.
    locomotion.velocity = (to - from) / dt;
    transform.translation.x = to.x;
    transform.translation.z = to.y;
    gait.phase = (gait.phase + (to - from).length() / STRIDE * std::f32::consts::TAU) % std::f32::consts::TAU;
    gait.pace = (locomotion.velocity.length() / WALK_SPEED).min(1.5);
}

/// A small step bob of the head ([`Gait::bob`]).
fn bob_head(body: Single<&Gait, With<Player>>, mut camera: Single<&mut Transform, (With<PlayerCamera>, Without<Player>)>) {
    let bob = body.bob();
    camera.translation = Vec3::new(BOB_SWAY * bob.x, EYE_HEIGHT - BOB_HEIGHT * bob.y, 0.0);
}

/// A world position on the floor plane, as `(x, z)`.
fn floor(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

/// Whether grid cell `(x, y)` stops the body: rock in the level, or a cell in `blocked_cells`.
pub fn blocked_by<'a>(level: &'a Level, blocked_cells: &'a BlockedCells) -> impl Fn(i32, i32) -> bool + 'a {
    move |x, y| !level.is_open(x, y) || blocked_cells.0.contains(&(x, y))
}

/// Moves a circle of `radius` on the floor plane from `from` toward `to` (world `(x, z)`),
/// sliding along cells where `blocked(cell_x, cell_y)` holds and rounding their corners.
/// Returns where it ends up, never overlapping a blocked cell if `from` did not.
///
/// The move is cut into steps shorter than half the radius, so no speed carries it through a
/// wall. Each step is pushed back out of every blocked cell it overlaps, along the line from
/// the cell's nearest point; a step that cannot be resolved is dropped.
pub fn slide(from: Vec2, to: Vec2, radius: f32, blocked: impl Fn(i32, i32) -> bool) -> Vec2 {
    let delta = to - from;
    let steps = (delta.length() / (radius * 0.5)).ceil().max(1.0) as u32;
    let step = delta / steps as f32;
    let mut at = from;
    for _ in 0..steps {
        let Some(next) = resolve(at + step, at, radius, &blocked) else { break };
        at = next;
    }
    at
}

/// Pushes `at` out of the blocked cells around it, or `None` if it cannot get clear.
/// `previous` is a clear spot nearby, used to choose a way out when the centre is inside rock.
fn resolve(mut at: Vec2, previous: Vec2, radius: f32, blocked: &impl Fn(i32, i32) -> bool) -> Option<Vec2> {
    for _ in 0..4 {
        let mut pushed = false;
        for (min, max) in blocked_cells_near(at, radius, blocked) {
            let nearest = at.clamp(min, max);
            let offset = at - nearest;
            let distance = offset.length();
            if distance >= radius {
                continue;
            }
            let normal = if distance > 1e-5 { offset / distance } else { exit_normal(previous, min, max) };
            at = nearest + normal * radius;
            pushed = true;
        }
        if !pushed {
            return Some(at);
        }
    }
    (!overlaps_blocked(at, radius, blocked)).then_some(at)
}

/// The blocked cells a circle at `at` could touch, as world-space `(min, max)` rectangles.
fn blocked_cells_near(at: Vec2, radius: f32, blocked: &impl Fn(i32, i32) -> bool) -> impl Iterator<Item = (Vec2, Vec2)> {
    let low = ((at - radius) / CELL).floor().as_ivec2();
    let high = ((at + radius) / CELL).floor().as_ivec2();
    let cells: Vec<IVec2> = (low.y..=high.y)
        .flat_map(|y| (low.x..=high.x).map(move |x| IVec2::new(x, y)))
        .filter(|cell| blocked(cell.x, cell.y))
        .collect();
    cells.into_iter().map(|cell| (cell.as_vec2() * CELL, (cell + 1).as_vec2() * CELL))
}

fn overlaps_blocked(at: Vec2, radius: f32, blocked: &impl Fn(i32, i32) -> bool) -> bool {
    blocked_cells_near(at, radius, blocked).any(|(min, max)| at.distance(at.clamp(min, max)) < radius - 1e-3)
}

/// For a centre that has ended up inside a cell: the face nearest `previous`, which was clear.
fn exit_normal(previous: Vec2, min: Vec2, max: Vec2) -> Vec2 {
    let faces = [
        (previous.x - min.x, Vec2::NEG_X),
        (max.x - previous.x, Vec2::X),
        (previous.y - min.y, Vec2::NEG_Y),
        (max.y - previous.y, Vec2::Y),
    ];
    // The face `previous` is furthest outside of.
    faces.into_iter().map(|(inside, normal)| (-inside, normal)).max_by(|a, b| a.0.total_cmp(&b.0)).map_or(Vec2::Y, |f| f.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map from rows of `#` (rock) and `.` (floor); outside it is rock.
    fn map<'a>(rows: &'a [&'a str]) -> impl Fn(i32, i32) -> bool + 'a {
        move |x, y| {
            if x < 0 || y < 0 {
                return true;
            }
            rows.get(y as usize).and_then(|row| row.as_bytes().get(x as usize)).is_none_or(|&c| c == b'#')
        }
    }

    fn centre(x: i32, y: i32) -> Vec2 {
        Vec2::new((x as f32 + 0.5) * CELL, (y as f32 + 0.5) * CELL)
    }

    fn clear(at: Vec2, blocked: &impl Fn(i32, i32) -> bool) -> bool {
        !overlaps_blocked(at, BODY_RADIUS, blocked)
    }

    const ROOM: &[&str] = &["#####", "#...#", "#...#", "#...#", "#####"];

    #[test]
    fn the_bob_is_still_standing_and_dips_at_each_footfall() {
        assert_eq!(Gait { phase: 1.3, pace: 0.0 }.bob(), Vec2::ZERO);
        let walking = |phase: f32| Gait { phase, pace: 1.0 }.bob();
        assert!(walking(0.0).abs_diff_eq(Vec2::ZERO, 1e-6));
        // Lowest twice a stride, swaying to one side then the other.
        assert!((walking(std::f32::consts::FRAC_PI_2).y - 1.0).abs() < 1e-6);
        assert!((walking(std::f32::consts::FRAC_PI_2).x - 1.0).abs() < 1e-6);
        assert!((walking(3.0 * std::f32::consts::FRAC_PI_2).x + 1.0).abs() < 1e-6);
    }

    #[test]
    fn free_moves_are_untouched() {
        let blocked = map(ROOM);
        let (from, to) = (centre(2, 2), centre(2, 2) + Vec2::new(3.0, -4.0));
        let end = slide(from, to, BODY_RADIUS, &blocked);
        assert!(end.abs_diff_eq(to, 1e-4), "{end}");
    }

    #[test]
    fn walls_stop_the_body_from_every_angle() {
        let blocked = map(ROOM);
        for i in 0..72 {
            let angle = i as f32 * 5f32.to_radians();
            let to = centre(2, 2) + Vec2::from_angle(angle) * 100.0;
            let end = slide(centre(2, 2), to, BODY_RADIUS, &blocked);
            assert!(clear(end, &blocked), "angle {i}: {end} overlaps rock");
            let inside = Vec2::splat(CELL + BODY_RADIUS - 1e-3);
            assert!(end.cmpge(inside).all() && end.cmple(Vec2::splat(4.0 * CELL - BODY_RADIUS + 1e-3)).all(), "angle {i}: {end}");
        }
    }

    #[test]
    fn diagonal_moves_slide_along_a_wall() {
        let blocked = map(ROOM);
        // Up against the north wall, moving north-east: the east part survives.
        let from = Vec2::new(centre(2, 1).x, CELL + BODY_RADIUS);
        let end = slide(from, from + Vec2::new(5.0, -5.0), BODY_RADIUS, &blocked);
        assert!((end.x - (from.x + 5.0)).abs() < 1e-3, "{end}");
        assert!((end.y - from.y).abs() < 1e-3, "{end}");
    }

    #[test]
    fn inner_corners_hold_on_both_walls() {
        let blocked = map(ROOM);
        let end = slide(centre(1, 1), centre(1, 1) - Vec2::splat(30.0), BODY_RADIUS, &blocked);
        assert!(end.abs_diff_eq(Vec2::splat(CELL + BODY_RADIUS), 1e-3), "{end}");
    }

    #[test]
    fn outer_corners_are_rounded_not_snagged() {
        // A pillar at (2, 2); brush past its north-west corner going east.
        let blocked = map(&["#####", "#...#", "#.#.#", "#...#", "#####"]);
        let from = Vec2::new(centre(1, 1).x, 2.0 * CELL - 1.0);
        let end = slide(from, from + Vec2::new(30.0, 0.0), BODY_RADIUS, &blocked);
        assert!(clear(end, &blocked), "{end}");
        assert!(end.x > from.x + 25.0, "snagged on the corner at {end}");
    }

    #[test]
    fn corridors_one_cell_wide_are_walkable() {
        let blocked = map(&["###", "#.#", "#.#", "#.#", "###"]);
        let end = slide(centre(1, 3), centre(1, 1), BODY_RADIUS, &blocked);
        assert!(end.abs_diff_eq(centre(1, 1), 1e-3), "{end}");
    }

    #[test]
    fn diagonal_gaps_between_rock_corners_do_not_let_the_body_through() {
        let blocked = map(&["####", "#..#", "#.##", "##.#", "####"]);
        // (1, 2) and (2, 3) meet only at a corner, between rock (2, 2) and (1, 3).
        let end = slide(centre(1, 2), centre(2, 3), BODY_RADIUS, &blocked);
        assert!(clear(end, &blocked), "{end}");
        assert!(end.x < 2.0 * CELL && end.y < 3.0 * CELL, "squeezed through to {end}");
    }

    #[test]
    fn high_speed_does_not_tunnel_through_rock() {
        // One cell of rock between two rooms.
        let blocked = map(&["#####", "#.#.#", "#####"]);
        let end = slide(centre(1, 1), centre(3, 1), BODY_RADIUS, &blocked);
        assert!(end.x <= 2.0 * CELL - BODY_RADIUS + 1e-3, "tunnelled to {end}");
        let far = slide(centre(1, 1), centre(1, 1) + Vec2::new(10_000.0, 7.0), BODY_RADIUS, &blocked);
        assert!(clear(far, &blocked) && far.x < 2.0 * CELL, "tunnelled to {far}");
    }

    #[test]
    fn blocked_cells_close_open_floor() {
        let level = fb_level::generate(1207);
        let (x, y, _) = level.start();
        let mut cells = BlockedCells::default();
        assert!(!blocked_by(&level, &cells)(x as i32, y as i32));
        cells.0.insert((x as i32, y as i32));
        assert!(blocked_by(&level, &cells)(x as i32, y as i32));
    }

    #[test]
    fn the_shut_entrance_keeps_the_body_in() {
        let level = fb_level::generate(1207);
        let (x, y, _) = level.start();
        let door = (level.entrance.x as i32, level.entrance.y as i32);
        let cells = BlockedCells([door].into_iter().collect());
        let from = centre(x as i32, y as i32);
        let to = centre(door.0, door.1);
        let end = slide(from, to, BODY_RADIUS, blocked_by(&level, &cells));
        assert!(end.distance(from + (to - from).normalize() * (CELL / 2.0 - BODY_RADIUS)) < 1e-3, "{end}");
    }
}
