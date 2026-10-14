//! Swinging what the main hand holds: the attack key starts a swing, the view model swings the
//! item in a shape that suits its hold, and partway through the blow lands on the nearest
//! creature in reach ([`super::target_in_reach`]), sent as a [`Struck`]. A creature's body is
//! where its bones are posed this moment ([`body_targets`]): a rat's body, head, legs and tail,
//! a snake's head and every segment of it, laid out along the floor or reared.
//!
//! A swing is drawn back, thrown through and recovered ([`swing_pose`]); the blow lands at the
//! end of the throw. The timings and poses are our own tuning against captures, as is the
//! small lean of the eye into each blow. With nothing in hand, the punch is that lean alone,
//! since no hand is drawn in first person yet.
//!
//! `FB_SWING=1` swings over and over, for captures of the swing mid-animation.

use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use fb_body::Side;

use super::feedback::{Corpse, Struck};
use super::{Health, RunDice, SwingKind, Target, Weapon, target_in_reach};
use crate::carry::{HeldItem, ViewModelCamera};
use crate::character::{Character, modifier};
use crate::creatures::{Bones, Critter, HitShape};
use crate::creatures::behaviour::body_radius;
use crate::inventory::Inventory;
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, Player, PlayerCamera, level_has_focus};
use crate::settings::GameSettings;

pub struct SwingPlugin;

impl Plugin for SwingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_swing_sound)
            .add_systems(
                Update,
                (
                    start_swing.run_if(level_has_focus).before(crate::player::take_cursor),
                    land_blows,
                    (swing_held.after(crate::carry::sway_held), lean_into_blow.after(crate::player::bob_head)),
                )
                    .chain()
                    .in_set(DelveSystems),
            );
    }
}

/// How long a swing takes, seconds, and when in it the blow lands. Our own: about two swings a
/// second, quicker than the SRD's one attack in a 6-second round (p. 90), as a real-time fight
/// wants.
pub const SWING_TIME: f32 = 0.5;
pub const LANDS: f32 = 0.24;
/// The swing's phases as shares of [`SWING_TIME`]: drawn back by the first, thrown through by
/// the second, then recovered.
const DRAWN: f32 = 0.34;
const THROWN: f32 = 0.52;

/// A swing under way, on the [`Player`].
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Swing {
    pub weapon: Weapon,
    /// Seconds into it.
    pub t: f32,
    /// Whether its blow has landed (or missed).
    pub landed: bool,
}

/// The whoosh of a swing, synthesised by `tools/synth_combat.py`.
#[derive(Resource, Clone, Debug, Default)]
pub struct SwingSound(pub Handle<AudioSource>);

fn load_swing_sound(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    if let Some(assets) = assets {
        commands.insert_resource(SwingSound(assets.load("audio/whoosh.ogg")));
    }
}

/// A delver with no swing under way.
type Ready = (With<Player>, Without<Swing>);

/// The attack key swings, if no swing is under way and the delver stands. A click while the
/// mouse is free only takes it back (`crate::player::take_cursor`), so it never swings too.
#[allow(clippy::too_many_arguments)]
fn start_swing(
    mut commands: Commands,
    input: ActionInput,
    cursor: Option<Single<&CursorOptions, With<PrimaryWindow>>>,
    inventory: Res<Inventory>,
    character: Res<Character>,
    sound: Option<Res<SwingSound>>,
    settings: Option<Res<GameSettings>>,
    tuning: Option<Res<crate::tuning::Tuning>>,
    players: Query<(Entity, Option<&Health>), Ready>,
) {
    let looking = cursor.is_none_or(|c| c.grab_mode != CursorGrabMode::None);
    let wanted = (input.just_pressed(Action::Attack) && looking) || crate::launch::env_set("FB_SWING");
    if !wanted {
        return;
    }
    for (player, health) in &players {
        if health.is_some_and(Health::is_down) {
            continue;
        }
        let weapon = Weapon::in_hand(&inventory, &character);
        commands.entity(player).insert(Swing { weapon, t: 0.0, landed: false });
        if let Some(sound) = &sound {
            let game = settings.as_deref().copied().unwrap_or_default();
            let tuning = tuning.as_deref().cloned().unwrap_or_default();
            commands.spawn((Name::new("swing"), AudioPlayer::new(sound.0.clone()), crate::audio::sound_effect(&game, &tuning)));
        }
    }
}

/// Moves each swing on; at [`LANDS`] its blow finds the nearest creature in reach and ahead,
/// and rolls its damage.
#[allow(clippy::too_many_arguments)]
fn land_blows(
    mut commands: Commands,
    time: Res<Time>,
    character: Res<Character>,
    tuning: Res<crate::tuning::Tuning>,
    mut dice: ResMut<RunDice>,
    mut players: Query<(Entity, &Transform, &mut Swing), With<Player>>,
    creatures: Query<(Entity, &Critter, &Transform, Option<&Bones>), Without<Corpse>>,
    shapes: Query<(&GlobalTransform, &HitShape)>,
    mut struck: MessageWriter<Struck>,
) {
    let dt = time.delta_secs();
    for (player, at, mut swing) in &mut players {
        swing.t += dt;
        if !swing.landed && swing.t >= LANDS {
            swing.landed = true;
            let from = at.translation.xz();
            let forward = at.forward().as_vec3().xz();
            let targets: Vec<Target> = creatures.iter().flat_map(|(entity, critter, t, bones)| body_targets(entity, critter, t, bones, &shapes)).collect();
            let combat = &tuning.combat;
            if let Some(target) = target_in_reach(from, forward, swing.weapon.reach * combat.reach_scale, combat.hit_cone(), &targets) {
                let damage = swing.weapon.roll(modifier(character.strength()), &mut dice.0);
                struck.write(Struck { target, damage, from });
            }
        }
        if swing.t >= SWING_TIME {
            commands.entity(player).remove::<Swing>();
        }
    }
}

/// A creature's body as a blow finds it: a circle on the floor under each sphere of each bone's
/// [`HitShape`], where the last pose put it. A creature with no shaped bones is the circle it
/// keeps clear ([`body_radius`]) about its root.
pub fn body_targets(entity: Entity, critter: &Critter, root: &Transform, bones: Option<&Bones>, shapes: &Query<(&GlobalTransform, &HitShape)>) -> Vec<Target> {
    let targets: Vec<Target> = bones
        .iter()
        .flat_map(|b| b.0.iter())
        .filter_map(|&bone| shapes.get(bone).ok())
        .flat_map(|(at, shape)| shape.spheres(at))
        .map(|(centre, radius)| Target { entity, centre: centre.xz(), radius })
        .collect();
    if targets.is_empty() { vec![Target { entity, centre: root.translation.xz(), radius: body_radius(critter) }] } else { targets }
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A pose of the held item relative to where it rests: moved by `offset` (view model camera
/// space: +X right, +Y up, -Z ahead) and turned by `turn` about the fist.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwingPose {
    pub offset: Vec3,
    pub turn: Quat,
}

impl SwingPose {
    const REST: SwingPose = SwingPose { offset: Vec3::ZERO, turn: Quat::IDENTITY };

    fn new(offset: Vec3, x: f32, y: f32, z: f32) -> SwingPose {
        SwingPose { offset, turn: Quat::from_euler(EulerRot::YXZ, y, x, z) }
    }

    fn lerp(self, other: SwingPose, t: f32) -> SwingPose {
        SwingPose { offset: self.offset.lerp(other.offset, t), turn: self.turn.slerp(other.turn, t) }
    }

    /// Applied to `rest` (item space to view space) about `fist`, in view space.
    pub fn apply(&self, rest: Transform, fist: Vec3) -> Transform {
        Transform { translation: fist + self.offset + self.turn * (rest.translation - fist), rotation: self.turn * rest.rotation, ..rest }
    }
}

/// Where a thrust drives a held item's point: the turn about the fist that lays its point on
/// the line to the crosshair, and that line's direction (view model camera space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    pub turn: Quat,
    pub along: Vec3,
}

/// The point on the crosshair a thrust aims at, view model camera space: 3 m straight ahead,
/// about a long weapon's reach. Our own.
pub const CROSSHAIR_AIM: Vec3 = Vec3::new(0.0, 0.0, -30.0);
/// How far a thrust draws back along its line, and drives forward, world units. Our own.
const THRUST_BACK: f32 = 2.5;
const THRUST_DRIVE: f32 = 7.0;

impl Aim {
    /// The aim at [`CROSSHAIR_AIM`] for an item resting at `rest` (its space to view space)
    /// with its point at `point` (its own space), turned about `fist`.
    pub fn at_crosshair(rest: Transform, fist: Vec3, point: Vec3) -> Aim {
        let pointing = (rest.transform_point(point) - fist).normalize_or(Vec3::NEG_Z);
        let along = (CROSSHAIR_AIM - fist).normalize();
        Aim { turn: Quat::from_rotation_arc(pointing, along), along }
    }
}

/// The two key poses of a swing, drawn back and thrown through, for the right hand. A rotation
/// about +X tips the top toward the eye, about +Z leans it left, about +Y turns it to point
/// left. A thrust with an [`Aim`] first turns the point onto the line to the crosshair as it
/// draws back along it, then drives straight down it.
fn key_poses(kind: SwingKind, aim: Option<Aim>) -> (SwingPose, SwingPose) {
    if let (SwingKind::Thrust, Some(aim)) = (kind, aim) {
        return (SwingPose { offset: -aim.along * THRUST_BACK, turn: aim.turn }, SwingPose { offset: aim.along * THRUST_DRIVE, turn: aim.turn });
    }
    match kind {
        // Raised and tipped back over the shoulder, then brought down and forward.
        SwingKind::Chop => (SwingPose::new(Vec3::new(0.5, 3.0, 1.5), 0.9, 0.0, -0.1), SwingPose::new(Vec3::new(-1.5, -2.5, -3.0), -1.3, 0.0, 0.35)),
        // Drawn out to the right, then cut across to the left.
        SwingKind::Slash => (SwingPose::new(Vec3::new(3.0, 1.2, 1.0), 0.2, -0.6, -0.8), SwingPose::new(Vec3::new(-6.5, -1.0, -2.5), -0.4, 0.8, 1.3)),
        // Drawn back, then driven straight ahead, point first.
        SwingKind::Thrust => (SwingPose::new(Vec3::new(0.5, -0.5, 2.5), 0.15, 0.0, 0.0), SwingPose::new(Vec3::new(-1.5, 1.0, -7.0), -0.7, 0.25, 0.0)),
        // Swung out to the right, then round in a flat arc to the left.
        SwingKind::Sweep => (SwingPose::new(Vec3::new(2.0, 0.5, 1.0), 0.0, -0.5, 0.0), SwingPose::new(Vec3::new(-6.0, 1.0, -3.0), 0.0, 1.0, 0.3)),
        SwingKind::Punch => (SwingPose::REST, SwingPose::REST),
    }
}

/// How far into its swing a held item of `kind` is at `t` seconds: drawn back, thrown through,
/// recovered, each eased; a thrust along `aim` if it has one.
pub fn swing_pose(kind: SwingKind, t: f32, aim: Option<Aim>) -> SwingPose {
    let (back, through) = key_poses(kind, aim);
    let u = t / SWING_TIME;
    if u < DRAWN {
        SwingPose::REST.lerp(back, smoothstep(u / DRAWN))
    } else if u < THROWN {
        back.lerp(through, smoothstep((u - DRAWN) / (THROWN - DRAWN)))
    } else {
        through.lerp(SwingPose::REST, smoothstep((u - THROWN) / (1.0 - THROWN)))
    }
}

/// How far the eye leans into a blow at `t` seconds: forward, world units, peaking as the blow
/// lands; a punch leans further, since nothing else shows it.
pub fn lean(kind: SwingKind, t: f32) -> f32 {
    let peak = if kind == SwingKind::Punch { 1.2 } else { 0.35 };
    let u = t / SWING_TIME;
    let rise = smoothstep(u / THROWN);
    let fall = 1.0 - smoothstep((u - THROWN) / (1.0 - THROWN));
    peak * rise.min(fall)
}

/// Swings the main hand's item in the view, about its fist.
fn swing_held(swings: Query<&Swing, With<Player>>, mut held: Query<(&HeldItem, &mut Transform, &ChildOf)>, view_models: Query<(), With<ViewModelCamera>>) {
    let Some(swing) = swings.iter().next() else { return };
    let Some(hold) = swing.weapon.hold else { return };
    let fist = crate::view_model::view_hold(hold, Side::Right).fist;
    for (item, mut transform, parent) in &mut held {
        if item.hand == fb_items::Slot::Mainhand && view_models.contains(parent.parent()) {
            let aim = item.point.map(|point| Aim::at_crosshair(*transform, fist, point));
            *transform = swing_pose(swing.weapon.kind, swing.t, aim).apply(*transform, fist);
        }
    }
}

/// Leans the eye forward into each blow.
fn lean_into_blow(swings: Query<&Swing, With<Player>>, mut cameras: Query<&mut Transform, (With<PlayerCamera>, Without<Player>)>) {
    let Some(swing) = swings.iter().next() else { return };
    let forward = lean(swing.weapon.kind, swing.t);
    for mut camera in &mut cameras {
        let ahead = camera.rotation * Vec3::NEG_Z;
        camera.translation += Vec3::new(ahead.x, 0.0, ahead.z).normalize_or_zero() * forward;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_swing_draws_back_throws_through_and_comes_to_rest() {
        for kind in [SwingKind::Chop, SwingKind::Slash, SwingKind::Thrust, SwingKind::Sweep] {
            assert_eq!(swing_pose(kind, 0.0, None), SwingPose::REST, "{kind:?}");
            let end = swing_pose(kind, SWING_TIME, None);
            assert!(end.offset.length() < 1e-4 && end.turn.angle_between(Quat::IDENTITY) < 1e-3, "{kind:?}");
            // Thrown through, it is ahead of where it was drawn back to.
            let (back, through) = (swing_pose(kind, DRAWN * SWING_TIME, None), swing_pose(kind, THROWN * SWING_TIME, None));
            assert!(through.offset.z < back.offset.z, "{kind:?}");
        }
        // A chop comes down, a slash goes across to the left, a thrust goes straight ahead.
        let through = |kind| swing_pose(kind, THROWN * SWING_TIME, None).offset;
        assert!(through(SwingKind::Chop).y < -1.0);
        assert!(through(SwingKind::Slash).x < -4.0);
        assert!(through(SwingKind::Thrust).z < -5.0);
        // The blow lands at the end of the throw.
        const { assert!(LANDS > DRAWN * SWING_TIME && LANDS <= THROWN * SWING_TIME) };
    }

    #[test]
    fn a_thrust_drives_the_point_at_the_crosshair() {
        use crate::view_model::view_hold;
        let view = crate::tuning::Tuning::default().view;
        let on_screen = |p| crate::view_model::on_screen(p, view.held_fov_degrees);
        let spec = fb_body::BodySpec::roll(fb_body::Folk::Human, 1207);
        for (base, seed) in [("Spear", 4), ("Spear", 9), ("Dagger", 5), ("Dagger", 1207)] {
            let item = fb_items::roll_item(base, seed).unwrap().shape;
            let held = &crate::view_model::held_in_view(&spec, Some(&item), None, view.held_scale)[0];
            let fist = view_hold(held.hold, Side::Right).fist;
            let rest = held.transform;
            let aim = Aim::at_crosshair(rest, fist, held.point);
            // At rest a spear lies across the view, its point well off the crosshair's line.
            let off_line = |t: Transform, base_at: Vec3| (t.transform_point(held.point) - base_at).normalize().angle_between(aim.along);
            if base == "Spear" {
                assert!(off_line(rest, fist) > 0.5, "{base} {seed}: at rest {}", off_line(rest, fist));
            }
            // Drawn back, it is already turned point first down the line; thrown through, the
            // point is driven along it, and a spear's reaches the crosshair.
            for phase in [DRAWN, THROWN] {
                let pose = swing_pose(SwingKind::Thrust, phase * SWING_TIME, Some(aim));
                let posed = pose.apply(rest, fist);
                let fist_now = fist + pose.offset;
                assert!(off_line(posed, fist_now) < 0.02, "{base} {seed} at {phase}: {}", off_line(posed, fist_now));
            }
            let thrown = swing_pose(SwingKind::Thrust, THROWN * SWING_TIME, Some(aim));
            let tip = thrown.apply(rest, fist).transform_point(held.point);
            assert!(tip.z < rest.transform_point(held.point).z - 4.0, "{base} {seed}: driven ahead");
            if base == "Spear" {
                let screen = on_screen(tip).unwrap();
                assert!(screen.length() < 0.15, "{base} {seed}: point at {screen} on the screen");
            }
        }
    }

    #[test]
    fn a_pose_turns_about_the_fist() {
        let fist = Vec3::new(5.0, -6.0, -9.5);
        let rest = Transform::from_translation(fist + Vec3::Y * 3.0);
        let tipped = SwingPose::new(Vec3::ZERO, -std::f32::consts::FRAC_PI_2, 0.0, 0.0).apply(rest, fist);
        // A quarter turn forward lays the head out ahead of the fist.
        assert!(tipped.translation.abs_diff_eq(fist + Vec3::NEG_Z * 3.0, 1e-4), "{}", tipped.translation);
    }

    #[test]
    fn the_eye_leans_into_the_blow_and_back() {
        assert_eq!(lean(SwingKind::Chop, 0.0), 0.0);
        assert!(lean(SwingKind::Punch, LANDS) > lean(SwingKind::Chop, LANDS));
        assert!(lean(SwingKind::Slash, SWING_TIME) < 1e-4);
    }
}
