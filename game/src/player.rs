//! The first-person body: camera, mouse look, walking and collision with the level's rock.
//!
//! Its pace, acceleration, radius, field of view and head bob are tuned ([`Tuning`]).
//!
//! The body is a circle on the floor plane that slides along rock and round [`Obstacles`] such
//! as folk and props ([`slide_around`]). Yaw turns the
//! [`Player`]; pitch tilts its [`PlayerCamera`] child, which also carries the head bob.

use bevy::{
    audio::SpatialListener,
    camera::Exposure,
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    render::view::Tonemapping,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use fb_level::{CELL, Level};

use crate::keymap::{Action, ActionInput, Keymap};
use crate::level::draw_end;
use crate::light::point_light;
use crate::settings::GameSettings;
use crate::tuning::Tuning;
use crate::launch::AppState;
use crate::character::{Character, DelverStamina, Load};
use crate::exhaustion::Exhaustion;
use crate::inventory::Inventory;
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, EYE_HEIGHT, Obstacle, Obstacles, Player, PlayerCamera, level_has_focus};

/// Radians of turn per pixel of mouse motion at a mouse sensitivity of 1.
const LOOK_SENSITIVITY: f32 = 0.0022;
/// Pitch stops short of straight up and down.
const PITCH_LIMIT: f32 = 85.0 * std::f32::consts::PI / 180.0;
/// Near clip in world units (5 mm), so items held at the face and the torch in hand stay whole.
const NEAR: f32 = 0.05;
/// Floor distance covered by one full bob cycle: two steps.
const STRIDE: f32 = 14.0;
/// Bob height and sway at walking pace, in world units (1.5 cm and 1 cm), at a `head_bob` of 1
/// ([`crate::tuning::ViewTuning`]).
const BOB_HEIGHT: f32 = 0.15;
const BOB_SWAY: f32 = 0.1;
/// Frames longer than this are walked as this long, so a hitch does not fling the body.
const MAX_STEP_SECONDS: f32 = 0.1;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Keymap>()
            // Kept as loaded if `SettingsPlugin` came first.
            .init_resource::<GameSettings>()
            .init_resource::<Tuning>()
            .add_systems(OnEnter(AppState::Delving), (spawn_player, grab_cursor))
            .add_systems(Update, (take_cursor, look, walk, bob_head).chain().run_if(level_has_focus).after(crate::play::switch_screens).in_set(DelveSystems))
            // The load follows the pack even with the sheet open, where the pack is changed and the
            // load condition is read.
            .add_systems(Update, weigh_the_delver.before(walk).after(crate::play::switch_screens).in_set(DelveSystems))
            .add_systems(Update, widen_player_view.run_if(resource_changed::<Tuning>).in_set(DelveSystems));
    }
}

/// On the [`Player`] while something slows them: their pace is multiplied by this (0 stops
/// them). A constrictor's hold, or a fall (`crate::combat`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Slowed(pub f32);

/// How the body is moving and looking. Yaw and pitch are kept here so the transforms can be
/// rebuilt from them without drift.
#[derive(Component, Debug, Default, Reflect)]
#[reflect(Component)]
#[require(Gait)]
pub struct Locomotion {
    pub yaw: f32,
    pub pitch: f32,
    /// Floor velocity, world units per second, as `(x, z)`.
    pub velocity: Vec2,
}

/// The player's step cycle, for whatever moves with their steps: the head bob here, what is held
/// in the hands in [`crate::carry`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Gait {
    /// Step cycle, radians; one turn per [`STRIDE`] walked (two steps).
    pub phase: f32,
    /// Speed as a multiple of walking pace, 0 standing, capped at 1.5 so bob and sway stay calm
    /// when hurrying.
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

/// The world camera's projection for `tuning`.
pub fn world_projection(tuning: &Tuning) -> Projection {
    Projection::from(PerspectiveProjection { fov: tuning.view.world_fov_degrees.to_radians(), near: NEAR, far: draw_end(&tuning.light), ..default() })
}

/// Keeps every player camera's field of view and far plane to the tuning's.
fn widen_player_view(tuning: Res<Tuning>, mut cameras: Query<&mut Projection, With<PlayerCamera>>) {
    let fov = tuning.view.world_fov_degrees.to_radians();
    let far = draw_end(&tuning.light);
    for mut projection in &mut cameras {
        if let Projection::Perspective(perspective) = &mut *projection
            && (perspective.fov != fov || perspective.far != far)
        {
            perspective.fov = fov;
            perspective.far = far;
        }
    }
}

fn spawn_player(mut commands: Commands, level: Res<CurrentLevel>, tuning: Res<Tuning>) {
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
                world_projection(&tuning),
                exposure,
                // Untonemapped, as the turntable draws, so the toon bands stay crisp.
                Tonemapping::Linear,
                // UI follows this camera even when frame capture retargets it to an image.
                IsDefaultUiCamera,
                Transform::from_xyz(0.0, EYE_HEIGHT, 0.0),
                // Ears 20 cm apart, for sounds from the creatures round them.
                SpatialListener::new(2.0),
                // The point lights' shadow maps cull from this camera, not from whichever
                // window camera has the lowest id (the view model's, far below the level).
                bevy::camera::ShadowLodOrigin,
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

/// A click in the level takes the mouse back, if it was let go (say, by switching windows).
/// Esc frees it by opening the menu ([`crate::play::switch_screens`]).
pub(crate) fn take_cursor(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>, input: ActionInput) {
    if input.just_pressed(Action::TakeMouse) {
        set_grab(&mut cursor, true);
    }
}

/// Takes the mouse for looking round (`true`) or frees it for a screen (`false`).
pub fn set_grab(cursor: &mut CursorOptions, grabbed: bool) {
    cursor.visible = !grabbed;
    cursor.grab_mode = if grabbed { CursorGrabMode::Locked } else { CursorGrabMode::None };
}

/// The turn for a mouse movement of `delta` pixels, as `(yaw, pitch)` radians: right turns
/// right and up looks up (down with [`GameSettings::invert_mouse_y`]), scaled by
/// [`GameSettings::mouse_sensitivity`].
pub fn look_turn(delta: Vec2, settings: &GameSettings) -> Vec2 {
    let speed = LOOK_SENSITIVITY * settings.mouse_sensitivity;
    let up = if settings.invert_mouse_y { -1.0 } else { 1.0 };
    Vec2::new(-delta.x * speed, -delta.y * speed * up)
}

pub(crate) fn look(
    motion: Res<AccumulatedMouseMotion>,
    settings: Res<GameSettings>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    mut body: Single<(&mut Locomotion, &mut Transform), With<Player>>,
    mut camera: Single<&mut Transform, (With<PlayerCamera>, Without<Player>)>,
) {
    let (locomotion, transform) = &mut *body;
    if cursor.grab_mode != CursorGrabMode::None && motion.delta != Vec2::ZERO {
        let turn = look_turn(motion.delta, &settings);
        locomotion.yaw += turn.x;
        locomotion.pitch = (locomotion.pitch + turn.y).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }
    transform.rotation = Quat::from_rotation_y(locomotion.yaw);
    camera.rotation = Quat::from_rotation_x(locomotion.pitch);
}

/// Keeps the delver's [`Load`] to what they wear and carry, working it out again only when the
/// character or the inventory has changed, or for a delver just spawned.
fn weigh_the_delver(
    mut commands: Commands,
    character: Option<Res<Character>>,
    inventory: Option<Res<Inventory>>,
    rules: Option<Res<crate::rules::GameRules>>,
    mut players: Query<(Entity, &mut Load, &mut GearPace, &mut crate::character::GearStamina, Has<WebWalker>), With<Player>>,
) {
    let (Some(character), Some(inventory)) = (character, inventory) else { return };
    let changed = character.is_changed() || inventory.is_changed() || rules.as_ref().is_some_and(|r| r.is_changed());
    let mut derived = None;
    for (player, mut load, mut pace, mut stamina, web_walker) in &mut players {
        if changed || load.is_added() {
            // An alignment with spiders lets them through webs.
            let walks_webs = character.gear(&inventory).web_walker;
            if walks_webs && !web_walker {
                commands.entity(player).insert(WebWalker);
            } else if !walks_webs && web_walker {
                commands.entity(player).remove::<WebWalker>();
            }
            let derived = *derived.get_or_insert_with(|| character.derive_with(crate::rules::GameRules::of(&rules), &inventory));
            load.set_if_neq(Load::of(derived.load, derived.capacity));
            pace.set_if_neq(GearPace(derived.speed));
            stamina.set_if_neq(crate::character::GearStamina { refill: derived.stamina_refill, cost: derived.stamina_cost });
        }
    }
}

/// What the delver's gear makes of their pace, as a multiple (boots "of Haste" 1.2), kept up
/// with their [`Load`] by `weigh_the_delver`. It scales walking and sprinting alike.
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component)]
pub struct GearPace(pub f32);

impl Default for GearPace {
    fn default() -> Self {
        GearPace(1.0)
    }
}

/// The delver walks through webs unhindered, as the spiders do: a cleric aligned with them
/// ([`fb_character::devotion::Devotion::Aligned`]). Kept up by `weigh_the_delver`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WebWalker;

/// What walking moves and reads on the delver.
type Walker<'a> = (&'a mut Locomotion, &'a mut Gait, &'a mut Transform, Option<&'a Slowed>, &'a Exhaustion, &'a Load, &'a DelverStamina, Option<&'a GearPace>, Has<crate::combat::guard::Guard>);

#[allow(clippy::too_many_arguments)]
pub(crate) fn walk(
    time: Res<Time>,
    tuning: Res<Tuning>,
    rules: Option<Res<crate::rules::GameRules>>,
    input: ActionInput,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Option<Res<Obstacles>>,
    doors: Option<Res<crate::doors::Doors>>,
    mut body: Single<Walker<'_>, With<Player>>,
    effects: Query<&crate::character::DelverEffects, With<Player>>,
    web_walker: Query<(), (With<Player>, With<WebWalker>)>,
) {
    let dt = time.delta_secs().min(MAX_STEP_SECONDS);
    if dt <= 0.0 {
        return;
    }
    let (locomotion, gait, transform, slowed, exhaustion, load, stamina, gear_pace, guarding) = &mut *body;
    let moving = &tuning.moving;
    let rules = crate::rules::GameRules::of(&rules);
    // What they wear and what is on them (the cold in a cold-blooded body) set their pace.
    let pace = gear_pace.map_or(1.0, |p| p.0) * effects.iter().next().map_or(1.0, |e| e.0.pace(&rules.effects) as f32);
    let web = fb_level::webs::web_pace(&level.0.webs, transform.translation.x, transform.translation.z, moving.body_radius, web_walker.iter().next().map(|()| fb_creatures::Species::GiantSpider), moving.web_pace);
    // A raised shield slows them to a guarded walk, with no sprinting (`crate::combat::guard`).
    let guard = if *guarding { crate::combat::guard::GUARD_PACE } else { 1.0 };
    let sprinting = input.pressed(Action::Sprint) && stamina.0.can_exert() && !*guarding;
    let speed = if sprinting { moving.hurry_speed } else { moving.walk_speed } * pace * web * guard * slowed.map_or(1.0, |s| s.0) * exhaustion.0.speed_multiplier_with(rules) * load.0.walk_multiplier_with(rules);
    let input = Vec2::new(input.axis(Action::StepRight, Action::StepLeft), input.axis(Action::WalkForward, Action::WalkBack));
    let (forward, right) = (floor(transform.forward().as_vec3()), floor(transform.right().as_vec3()));
    let wanted = (forward * input.y + right * input.x).normalize_or_zero() * speed;
    locomotion.velocity.smooth_nudge(&wanted, moving.acceleration, dt);

    let from = floor(transform.translation);
    // A shut claim door stops the delver as rock does; the folk's routines never read it.
    let rock = blocked_by(&level.0, &blocked_cells);
    let blocked = |x: i32, y: i32| rock(x, y) || doors.as_deref().is_some_and(|d| d.shuts(x, y));
    let obstacles = obstacles.as_deref().map_or(&[][..], |o| &o.0[..]);
    let to = slide_around(from, from + locomotion.velocity * dt, moving.body_radius, blocked, obstacles);
    // Walking into a wall leaves only the speed along it.
    locomotion.velocity = (to - from) / dt;
    transform.translation.x = to.x;
    transform.translation.z = to.y;
    // Up the steps of a dais and down into a sunken floor: the camera follows the body.
    transform.translation.y = level.0.dais_height(to.x, to.y);
    gait.phase = (gait.phase + (to - from).length() / STRIDE * std::f32::consts::TAU) % std::f32::consts::TAU;
    gait.pace = (locomotion.velocity.length() / moving.walk_speed.max(1e-3)).min(1.5);
}

/// A small step bob of the head ([`Gait::bob`]).
pub(crate) fn bob_head(tuning: Res<Tuning>, body: Single<&Gait, With<Player>>, mut camera: Single<&mut Transform, (With<PlayerCamera>, Without<Player>)>) {
    let bob = body.bob() * tuning.view.head_bob;
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
    slide_around(from, to, radius, blocked, &[])
}

/// [`slide`], and round `obstacles` as well: the body keeps out of them as it keeps out of
/// rock, sliding round their edge.
pub fn slide_around(from: Vec2, to: Vec2, radius: f32, blocked: impl Fn(i32, i32) -> bool, obstacles: &[Obstacle]) -> Vec2 {
    let delta = to - from;
    let steps = (delta.length() / (radius * 0.5)).ceil().max(1.0) as u32;
    let step = delta / steps as f32;
    let mut at = from;
    for _ in 0..steps {
        let Some(next) = resolve(at + step, at, radius, &blocked, obstacles) else { break };
        at = next;
    }
    at
}

/// Pushes `at` out of the blocked cells and obstacles around it, or `None` if it cannot get
/// clear. `previous` is a clear spot nearby, used to choose a way out when the centre is
/// inside rock or an obstacle.
fn resolve(mut at: Vec2, previous: Vec2, radius: f32, blocked: &impl Fn(i32, i32) -> bool, obstacles: &[Obstacle]) -> Option<Vec2> {
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
        for obstacle in obstacles {
            if let Some(out) = obstacle.push_out(at, radius, previous) {
                at = out;
                pushed = true;
            }
        }
        if !pushed {
            return Some(at);
        }
    }
    let in_obstacle = obstacles.iter().any(|o| o.overlaps(at, radius));
    (!overlaps_blocked(at, radius, blocked) && !in_obstacle).then_some(at)
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

    /// The body's radius as shipped.
    fn body_radius() -> f32 {
        Tuning::default().moving.body_radius
    }

    /// How far the body walks forward in `frames` frames of 1/60 s with W held, after
    /// `tune` changes the tuning.
    fn walked(frames: u32, tune: impl Fn(&mut Tuning)) -> f32 {
        walked_as(frames, tune, ())
    }

    /// As [`walked`], for a delver spawned with the extra components `delver`.
    fn walked_as(frames: u32, tune: impl Fn(&mut Tuning), delver: impl Bundle) -> f32 {
        let (mut app, body, start) = walking(tune, delver);
        for _ in 0..frames {
            app.update();
        }
        app.world().get::<Transform>(body).unwrap().translation.distance(start)
    }

    /// A level with W held and a delver spawned with the extra components `delver`: the app, the
    /// delver and where they started.
    fn walking(tune: impl Fn(&mut Tuning), delver: impl Bundle) -> (App, Entity, Vec3) {
        use bevy::time::TimeUpdateStrategy;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_resource(CurrentLevel(fb_level::generate(1207, 1)))
            .init_resource::<Keymap>()
            .init_resource::<BlockedCells>()
            .init_resource::<Tuning>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, (weigh_the_delver, walk).chain());
        app.update();
        tune(&mut app.world_mut().resource_mut::<Tuning>());
        let (x, y, facing) = app.world().resource::<CurrentLevel>().0.start();
        let [wx, wy, wz] = Level::world(x as f32 + 0.5, y as f32 + 0.5);
        let start = Vec3::new(wx, wy, wz);
        let body = app.world_mut().spawn((Player, Locomotion { yaw: facing.yaw(), ..default() }, Transform::from_translation(start).with_rotation(Quat::from_rotation_y(facing.yaw())), delver)).id();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyW);
        (app, body, start)
    }

    #[test]
    fn the_far_plane_sits_just_past_the_fogs_end() {
        let mut tuning = Tuning::default();
        let far = |tuning: &Tuning| match world_projection(tuning) {
            Projection::Perspective(p) => p.far,
            other => panic!("not a perspective: {other:?}"),
        };
        assert!(far(&tuning) > tuning.light.fog_end && far(&tuning) < tuning.light.fog_end + 100.0);
        // And it follows the tuning on a camera already spawned.
        let mut app = App::new();
        app.insert_resource(tuning.clone()).add_systems(Update, widen_player_view);
        let camera = app.world_mut().spawn((PlayerCamera, world_projection(&tuning))).id();
        tuning.light.fog_end = 120.0;
        app.insert_resource(tuning.clone());
        app.update();
        let Projection::Perspective(p) = app.world().get::<Projection>(camera).unwrap() else { panic!("not a perspective") };
        assert_eq!(p.far, far(&tuning));
    }

    #[test]
    fn the_delver_climbs_onto_a_dais_and_down_into_a_sunken_floor() {
        use bevy::time::TimeUpdateStrategy;
        for (seed, sunk) in [(8, false), (3, true)] {
            let level = fb_level::generate(seed, 2);
            let dais = *level.daises.iter().find(|d| d.sunk == sunk).unwrap();
            let [x0, x1, z0, z1] = dais.rect();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
                .insert_resource(CurrentLevel(level))
                .init_resource::<Keymap>()
                .init_resource::<BlockedCells>()
                .init_resource::<Tuning>()
                .init_resource::<ButtonInput<KeyCode>>()
                .add_systems(Update, (weigh_the_delver, walk).chain());
            app.update();
            let body = app.world_mut().spawn((Player, Locomotion::default(), Transform::from_xyz((x0 + x1) * 0.5, 0.0, (z0 + z1) * 0.5))).id();
            app.update();
            let y = app.world().get::<Transform>(body).unwrap().translation.y;
            assert!((y - dais.top_height()).abs() < 1e-4, "stands at {y}, not {}", dais.top_height());
        }
    }

    #[test]
    fn boots_of_haste_walk_a_fifth_farther() {
        let (mut usual_app, usual_body, start) = walking(|_| {}, ());
        let (mut quick_app, quick_body, _) = walking(|_| {}, GearPace(1.2));
        // The gear's pace is kept from the inventory; with none it is put back to 1, so hold it.
        for _ in 0..10 {
            usual_app.update();
            quick_app.world_mut().get_mut::<GearPace>(quick_body).unwrap().0 = 1.2;
            quick_app.update();
        }
        let usual = usual_app.world().get::<Transform>(usual_body).unwrap().translation.distance(start);
        let quick = quick_app.world().get::<Transform>(quick_body).unwrap().translation.distance(start);
        assert!((quick / usual - 1.2).abs() < 0.03, "{quick} against {usual}");
    }

    /// How far the delver walks in `frames` frames, with a floor web laid along their way if
    /// `web` and none otherwise.
    fn walked_through(frames: u32, web: bool) -> f32 {
        let (mut app, body, start) = walking(|_| {}, ());
        if web {
            let level = &mut app.world_mut().resource_mut::<CurrentLevel>().0;
            let (x, y, facing) = level.start();
            // Along the way they are walking, from before their feet to well past them.
            let spot = fb_level::WebSpot { x: x as f32 + 0.5, y: y as f32 + 0.5, height: 0.0, yaw: facing.yaw(), size: 400.0, kind: fb_level::WebKind::Floor };
            level.webs.push(spot);
        }
        for _ in 0..frames {
            app.update();
        }
        app.world().get::<Transform>(body).unwrap().translation.distance(start)
    }

    #[test]
    fn a_delver_in_a_floor_web_covers_less_ground() {
        let (free, webbed) = (walked_through(30, false), walked_through(30, true));
        assert!(webbed < free, "{webbed} in the web, {free} out of it");
        assert!((webbed / free - Tuning::default().moving.web_pace).abs() < 0.1, "{webbed} against {free}");
    }

    #[test]
    fn the_walking_pace_follows_the_tuning() {
        let usual = walked(10, |_| {});
        let half = walked(10, |t| t.moving.walk_speed *= 0.5);
        assert!(usual > 1.0, "{usual}");
        assert!((half / usual - 0.5).abs() < 0.02, "half the pace walks half as far: {half} against {usual}");
    }

    /// How far the body walks in 30 frames with W and Hurry held, for a delver with `stamina`.
    fn hurried(stamina: fb_character::Stamina) -> f32 {
        let (mut app, body, start) = walking(|_| {}, DelverStamina(stamina));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ShiftLeft);
        for _ in 0..30 {
            app.update();
        }
        app.world().get::<Transform>(body).unwrap().translation.distance(start)
    }

    #[test]
    fn a_winded_delver_cannot_sprint_until_they_have_caught_their_breath() {
        let walked = walked(30, |_| {});
        let fresh = hurried(fb_character::Stamina::default());
        assert!(fresh > walked * 1.3, "sprinting outpaces walking: {fresh} against {walked}");
        let winded = hurried(fb_character::Stamina { value: 0.1, winded: true });
        assert!((winded / walked - 1.0).abs() < 0.02, "winded, hurry only walks: {winded} against {walked}");
        let recovered = hurried(fb_character::Stamina { value: 0.2, winded: false });
        assert!((recovered / fresh - 1.0).abs() < 0.02, "{recovered} against {fresh}");
    }

    fn exhausted(level: u32) -> Exhaustion {
        Exhaustion(fb_character::Exhaustion::new(level))
    }

    #[test]
    fn exhaustion_slows_a_delver_who_is_not_otherwise_slowed() {
        for frames in [10, 30, 60] {
            let usual = walked(frames, |_| {});
            let level_2 = walked_as(frames, |_| {}, exhausted(2));
            assert!((level_2 / usual - 0.5).abs() < 0.02, "{frames} frames: {level_2} against {usual}");
            assert!(walked_as(frames, |_| {}, exhausted(1)) / usual > 0.99, "level 1 does not slow");
        }
    }

    #[test]
    fn a_raised_shield_slows_the_delver_and_stops_a_sprint() {
        use crate::combat::guard::{GUARD_PACE, Guard};
        let guard = Guard { at: Vec2::ZERO, facing: Vec2::NEG_Y };
        let usual = walked(30, |_| {});
        let guarded = walked_as(30, |_| {}, guard);
        assert!((guarded / usual - GUARD_PACE).abs() < 0.02, "{guarded} against {usual}");
        let (mut app, body, start) = walking(|_| {}, guard);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ShiftLeft);
        for _ in 0..30 {
            app.update();
        }
        let sprinted = app.world().get::<Transform>(body).unwrap().translation.distance(start);
        assert!((sprinted / usual - GUARD_PACE).abs() < 0.02, "no sprint with the guard up: {sprinted} against {usual}");
    }

    #[test]
    fn exhaustion_multiplies_with_a_constrictors_hold_and_stays_steady() {
        use crate::combat::vitals::HELD_PACE;
        for frames in [10, 30, 60] {
            let usual = walked(frames, |_| {});
            let held = walked_as(frames, |_| {}, (exhausted(2), Slowed(HELD_PACE)));
            assert!((held / usual - 0.5 * HELD_PACE).abs() < 0.01, "{frames} frames: {held} against {usual}");
        }
    }

    #[test]
    fn at_exhaustion_level_5_the_delver_cannot_move() {
        assert!(walked(10, |_| {}) > 1.0);
        assert_eq!(walked_as(30, |_| {}, exhausted(5)), 0.0);
    }

    use fb_character::LoadBand;

    /// STR 11's capacity of 165 lb, with `lb` of it carried.
    fn loaded(lb: f32) -> Load {
        Load::of(lb, 165.0)
    }

    #[test]
    fn load_slows_the_walk_and_multiplies_with_exhaustion() {
        for frames in [10, 30] {
            let usual = walked(frames, |_| {});
            let burdened = walked_as(frames, |_| {}, loaded(110.0));
            let encumbered = walked_as(frames, |_| {}, loaded(165.0));
            let both = walked_as(frames, |_| {}, (loaded(165.0), exhausted(2)));
            assert!((burdened / usual - 0.9125).abs() < 0.01, "{burdened} against {usual}");
            assert!((encumbered / usual - 0.65).abs() < 0.01, "{encumbered} against {usual}");
            assert!((both / usual - 0.325).abs() < 0.01, "{both} against {usual}");
        }
    }

    #[test]
    fn an_overloaded_delver_does_not_move() {
        assert_eq!(walked_as(30, |_| {}, loaded(166.0)), 0.0);
    }

    /// A delver of strength 10 wearing a mail shirt made as heavy as `fraction` of what they can
    /// carry, and the shirt.
    fn laden(fraction: f32) -> (Character, Inventory, crate::inventory::ItemId) {
        let attributes = fb_character::Attributes { strength: 10, dexterity: 10, constitution: 10, intelligence: 10, wisdom: 10 };
        let delver = Character { attributes, ..Character::roll(4) };
        let mut mail = fb_items::roll_item("Mail", 1).unwrap();
        mail.profile.stats.weight = (150.0 + mail.profile.stats.carry) * fraction;
        let mut inventory = Inventory::default();
        let mail = inventory.add(mail);
        inventory.wear(mail, fb_items::Slot::Body).unwrap();
        (delver, inventory, mail)
    }

    #[test]
    fn the_real_load_sets_the_pace_and_dropping_it_lets_the_delver_move_again() {
        let usual = walked(30, |_| {});
        // Edges at a third and two thirds of capacity: half of it is t = 0.25, 97.8% pace; 0.8 of
        // it is t = 0.7, 82.85%.
        for (band, fraction, pace) in [(LoadBand::Burdened, 0.5, 0.978125), (LoadBand::Encumbered, 0.8, 0.8285)] {
            let (delver, inventory, _) = laden(fraction);
            let (mut app, body, start) = walking(|_| {}, ());
            app.insert_resource(delver).insert_resource(inventory);
            for _ in 0..30 {
                app.update();
            }
            let walked = app.world().get::<Transform>(body).unwrap().translation.distance(start);
            assert!((walked / usual - pace).abs() < 0.01, "{band:?}: {walked} against {usual}");
            assert_eq!(app.world().get::<Load>(body).map(|l| l.0.band()), Some(band));
        }

        let (delver, inventory, mail) = laden(1.2);
        let (mut app, body, start) = walking(|_| {}, ());
        app.insert_resource(delver).insert_resource(inventory);
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(app.world().get::<Load>(body).map(|l| l.0.band()), Some(LoadBand::Overloaded));
        assert_eq!(app.world().get::<Transform>(body).unwrap().translation.distance(start), 0.0, "overloaded");
        app.world_mut().resource_mut::<Inventory>().forget(mail);
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(app.world().get::<Load>(body).map(|l| l.0.band()), Some(LoadBand::Light));
        let moved = app.world().get::<Transform>(body).unwrap().translation.distance(start);
        assert!((moved / usual - 1.0).abs() < 0.05, "{moved} against {usual}");
    }

    #[test]
    fn sensitivity_scales_mouse_look_and_invert_flips_it() {
        let usual = GameSettings::default();
        let turn = look_turn(Vec2::new(10.0, -5.0), &usual);
        assert!((turn - Vec2::new(-10.0, 5.0) * LOOK_SENSITIVITY).length() < 1e-7, "right turns right, up looks up");
        let quick = GameSettings { mouse_sensitivity: 2.0, ..usual };
        assert!((look_turn(Vec2::new(10.0, -5.0), &quick) - turn * 2.0).length() < 1e-7);
        let inverted = GameSettings { invert_mouse_y: true, ..usual };
        assert_eq!(look_turn(Vec2::new(10.0, -5.0), &inverted), Vec2::new(turn.x, -turn.y));
    }

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
        !overlaps_blocked(at, body_radius(), blocked)
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
        let end = slide(from, to, body_radius(), &blocked);
        assert!(end.abs_diff_eq(to, 1e-4), "{end}");
    }

    #[test]
    fn walls_stop_the_body_from_every_angle() {
        let blocked = map(ROOM);
        for i in 0..72 {
            let angle = i as f32 * 5f32.to_radians();
            let to = centre(2, 2) + Vec2::from_angle(angle) * 100.0;
            let end = slide(centre(2, 2), to, body_radius(), &blocked);
            assert!(clear(end, &blocked), "angle {i}: {end} overlaps rock");
            let inside = Vec2::splat(CELL + body_radius() - 1e-3);
            assert!(end.cmpge(inside).all() && end.cmple(Vec2::splat(4.0 * CELL - body_radius() + 1e-3)).all(), "angle {i}: {end}");
        }
    }

    #[test]
    fn diagonal_moves_slide_along_a_wall() {
        let blocked = map(ROOM);
        // Up against the north wall, moving north-east: the east part survives.
        let from = Vec2::new(centre(2, 1).x, CELL + body_radius());
        let end = slide(from, from + Vec2::new(5.0, -5.0), body_radius(), &blocked);
        assert!((end.x - (from.x + 5.0)).abs() < 1e-3, "{end}");
        assert!((end.y - from.y).abs() < 1e-3, "{end}");
    }

    #[test]
    fn inner_corners_hold_on_both_walls() {
        let blocked = map(ROOM);
        let end = slide(centre(1, 1), centre(1, 1) - Vec2::splat(30.0), body_radius(), &blocked);
        assert!(end.abs_diff_eq(Vec2::splat(CELL + body_radius()), 1e-3), "{end}");
    }

    #[test]
    fn outer_corners_are_rounded_not_snagged() {
        // A pillar at (2, 2); brush past its north-west corner going east.
        let blocked = map(&["#####", "#...#", "#.#.#", "#...#", "#####"]);
        let from = Vec2::new(centre(1, 1).x, 2.0 * CELL - 1.0);
        let end = slide(from, from + Vec2::new(30.0, 0.0), body_radius(), &blocked);
        assert!(clear(end, &blocked), "{end}");
        assert!(end.x > from.x + 25.0, "snagged on the corner at {end}");
    }

    #[test]
    fn corridors_one_cell_wide_are_walkable() {
        let blocked = map(&["###", "#.#", "#.#", "#.#", "###"]);
        let end = slide(centre(1, 3), centre(1, 1), body_radius(), &blocked);
        assert!(end.abs_diff_eq(centre(1, 1), 1e-3), "{end}");
    }

    #[test]
    fn diagonal_gaps_between_rock_corners_do_not_let_the_body_through() {
        let blocked = map(&["####", "#..#", "#.##", "##.#", "####"]);
        // (1, 2) and (2, 3) meet only at a corner, between rock (2, 2) and (1, 3).
        let end = slide(centre(1, 2), centre(2, 3), body_radius(), &blocked);
        assert!(clear(end, &blocked), "{end}");
        assert!(end.x < 2.0 * CELL && end.y < 3.0 * CELL, "squeezed through to {end}");
    }

    #[test]
    fn high_speed_does_not_tunnel_through_rock() {
        // One cell of rock between two rooms.
        let blocked = map(&["#####", "#.#.#", "#####"]);
        let end = slide(centre(1, 1), centre(3, 1), body_radius(), &blocked);
        assert!(end.x <= 2.0 * CELL - body_radius() + 1e-3, "tunnelled to {end}");
        let far = slide(centre(1, 1), centre(1, 1) + Vec2::new(10_000.0, 7.0), body_radius(), &blocked);
        assert!(clear(far, &blocked) && far.x < 2.0 * CELL, "tunnelled to {far}");
    }

    #[test]
    fn the_body_cannot_walk_through_a_circle_and_slides_round_it() {
        let open = |_: i32, _: i32| false;
        let folk = [Obstacle::Circle { centre: Vec2::new(0.0, -20.0), radius: 2.5 }];
        let clear = |at: Vec2| !folk[0].overlaps(at, body_radius());
        // Straight at them: stopped short.
        let end = slide_around(Vec2::ZERO, Vec2::new(0.0, -40.0), body_radius(), open, &folk);
        assert!(clear(end) && end.y > -20.0, "{end}");
        // Walking at them all the way, frame by frame, never gets through.
        let mut at = Vec2::ZERO;
        for _ in 0..200 {
            at = slide_around(at, at + Vec2::new(0.0, -0.5), body_radius(), open, &folk);
            assert!(clear(at), "{at}");
        }
        // A glancing step slides round their edge and on past.
        let mut at = Vec2::new(1.0, 0.0);
        for _ in 0..200 {
            at = slide_around(at, at + Vec2::new(0.0, -0.5), body_radius(), open, &folk);
            assert!(clear(at), "{at}");
        }
        assert!(at.y < -30.0, "snagged at {at}");
        // Far from them nothing changes.
        assert_eq!(slide_around(Vec2::ZERO, Vec2::new(5.0, 0.0), body_radius(), open, &folk), Vec2::new(5.0, 0.0));
    }

    #[test]
    fn the_body_cannot_walk_through_a_turned_chest_and_slides_along_it() {
        let open = |_: i32, _: i32| false;
        let chest = [Obstacle::Rect { centre: Vec2::new(0.0, -20.0), half: Vec2::new(4.5, 2.4), yaw: 0.3 }];
        let mut at = Vec2::ZERO;
        for _ in 0..200 {
            at = slide_around(at, at + Vec2::new(0.0, -0.5), body_radius(), open, &chest);
            assert!(!chest[0].overlaps(at, body_radius()), "{at}");
        }
        // Pressing on into its slanted face slid the body along it and round its end, wide of
        // the chest's half width.
        assert!(at.y < -20.0 && at.x.abs() > 4.5, "snagged at {at}");
    }

    #[test]
    fn blocked_cells_close_open_floor() {
        let level = fb_level::generate(1207, 1);
        let (x, y, _) = level.start();
        let mut cells = BlockedCells::default();
        assert!(!blocked_by(&level, &cells)(x as i32, y as i32));
        cells.0.insert((x as i32, y as i32));
        assert!(blocked_by(&level, &cells)(x as i32, y as i32));
    }

    #[test]
    fn the_shut_entrance_keeps_the_body_in() {
        let level = fb_level::generate(1207, 1);
        let (x, y, _) = level.start();
        let door = (level.entrance.x as i32, level.entrance.y as i32);
        let cells = BlockedCells([door].into_iter().collect());
        let from = centre(x as i32, y as i32);
        let to = centre(door.0, door.1);
        let end = slide(from, to, body_radius(), blocked_by(&level, &cells));
        assert!(end.distance(from + (to - from).normalize() * (CELL / 2.0 - body_radius())) < 1e-3, "{end}");
    }
}
