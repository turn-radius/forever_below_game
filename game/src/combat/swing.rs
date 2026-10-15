//! Swinging what the main hand holds: the attack key starts a swing, the view model swings the
//! item in a shape that suits its hold, and partway through the blow lands on the nearest
//! creature in reach ([`super::target_in_reach`]), sent as a [`Struck`]. A creature's body is
//! where its bones are posed this moment ([`body_targets`]): a rat's body, head, legs and tail,
//! a snake's head and every segment of it, laid out along the floor or reared.
//!
//! Each swing is thrown as one of the strikes its kind allows ([`Strike`]): down from overhead,
//! down across from either high corner, flat across from either side, rising from below, or
//! a stab, and never the same strike twice running while there is another. It is drawn back,
//! thrown through and recovered ([`SwingPath`]); the blow lands at the end of the throw. As it
//! is drawn back the wrist turns the item into the strike's plane, edge leading, and it is
//! thrown in an arc through that plane to the creature in reach, where it stood on the screen
//! as the swing began, or to the crosshair. A stab draws back and hooks in point first along
//! the line to it. Each swing also wobbles a little of its own. The timings and poses are our own tuning against captures, as is the
//! small lean of the eye into each blow. With nothing in hand, the punch is that lean alone,
//! since no hand is drawn in first person yet.
//!
//! `FB_SWING=1` swings over and over, for captures of the swing mid-animation, and
//! [`SwingPractice`] (`--swings`, [`crate::swing_practice`]) can pin the strike.

use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use fb_body::Side;

use super::feedback::{Corpse, Struck};
use super::{Health, RunDice, SwingKind, Target, Weapon, target_in_reach};
use crate::carry::{HeldItem, ViewModelCamera};
use crate::view_model::ViewHold;
use crate::character::{Character, DelverStamina, modifier};
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
    /// Which way the blow comes in.
    pub strike: Strike,
    /// Where the blow heads, view model camera space: the creature in reach as it stood on the
    /// screen when the swing began, at [`CROSSHAIR_AIM`]'s depth, or the crosshair itself.
    pub toward: Vec3,
    /// This swing's own wobble on its kind's path, each part in -1..1.
    pub flourish: Vec3,
}

impl Swing {
    /// A swing at the crosshair with no wobble, thrown as its kind's commonest strike.
    pub fn new(weapon: Weapon) -> Swing {
        let strike = Strike::choices(weapon.kind)[0].0;
        Swing { weapon, t: 0.0, landed: false, strike, toward: CROSSHAIR_AIM, flourish: Vec3::ZERO }
    }

    /// The path an item held as `grip` follows.
    pub fn path(&self, grip: Grip) -> SwingPath {
        SwingPath::new(self.strike, grip, self.toward, self.flourish)
    }
}

/// The whoosh of a swing, synthesised by `tools/synth_combat.py`.
#[derive(Resource, Clone, Debug, Default)]
pub struct SwingSound(pub Handle<AudioSource>);

fn load_swing_sound(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    if let Some(assets) = assets {
        commands.insert_resource(SwingSound(assets.load("audio/whoosh.ogg")));
    }
}

/// For trying swings out: a strike every swing is thrown as instead of a rolled one, and
/// whether to swing over and over.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct SwingPractice {
    pub strike: Option<Strike>,
    pub repeat: bool,
}

/// What the toast says when a winded delver tries to swing.
const WINDED: &str = "Too winded to swing";

/// The toast a refused swing speaks through.
#[derive(bevy::ecs::system::SystemParam)]
struct Winded<'w> {
    toast: Option<ResMut<'w, crate::inventory::Toast>>,
    real: Option<Res<'w, Time<Real>>>,
}

impl Winded<'_> {
    /// Says so, but not again while the line still shows.
    fn say(&mut self) {
        let now = self.real.as_deref().map_or(0.0, |t| t.elapsed_secs());
        if let Some(toast) = self.toast.as_deref_mut()
            && (toast.text != WINDED || toast.until <= now)
        {
            toast.say(WINDED, now);
        }
    }
}

/// A delver with no swing under way.
type Ready = (With<Player>, Without<Swing>);

/// The attack key swings, if no swing is under way and the delver stands. A click while the
/// mouse is free only takes it back (`crate::player::take_cursor`), so it never swings too.
/// The swing heads for the creature in reach, if there is one ([`toward_target`]).
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
    mut dice: ResMut<RunDice>,
    mut last: Local<Option<Strike>>,
    practice: Option<Res<SwingPractice>>,
    mut winded: Winded,
    mut players: Query<(Entity, &Transform, Option<&Health>, &mut DelverStamina), Ready>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    creatures: Query<(Entity, &Critter, &Transform, Option<&Bones>), Without<Corpse>>,
    shapes: Query<(&GlobalTransform, &HitShape)>,
) {
    let looking = cursor.is_none_or(|c| c.grab_mode != CursorGrabMode::None);
    let practice = practice.as_deref().copied().unwrap_or_default();
    let wanted = (input.just_pressed(Action::Attack) && looking) || practice.repeat || crate::launch::env_set("FB_SWING");
    if !wanted {
        return;
    }
    let tuning = tuning.as_deref().cloned().unwrap_or_default();
    let practising = practice.repeat || crate::launch::env_set("FB_SWING");
    for (player, at, health, mut stamina) in &mut players {
        if health.is_some_and(Health::is_down) {
            continue;
        }
        // A winded delver cannot swing; each swing otherwise takes its bite of stamina.
        if !practising {
            if !stamina.0.can_exert() {
                winded.say();
                continue;
            }
            let mut spent = stamina.0;
            spent.spend(fb_character::SWING_COST);
            stamina.0 = spent;
        }
        let weapon = Weapon::in_hand(&inventory, &character);
        let toward = eyes.iter().next().and_then(|eye| {
            let from = at.translation.xz();
            let targets: Vec<Target> = creatures.iter().flat_map(|(entity, critter, t, bones)| body_targets(entity, critter, t, bones, &shapes)).collect();
            let combat = &tuning.combat;
            let target = target_in_reach(from, at.forward().as_vec3().xz(), weapon.reach * combat.reach_scale, combat.hit_cone(), &targets)?;
            let (_, critter, root, bones) = creatures.get(target).ok()?;
            toward_target(eye, body_points(critter, root, bones, &shapes), &tuning.view)
        });
        let flourish = Vec3::new(dice.0.range_f32(-1.0, 1.0), dice.0.range_f32(-1.0, 1.0), dice.0.range_f32(-1.0, 1.0));
        let strike = practice.strike.unwrap_or_else(|| Strike::roll(weapon.kind, *last, &mut dice.0));
        *last = Some(strike);
        commands.entity(player).insert(Swing { strike, toward: toward.unwrap_or(CROSSHAIR_AIM), flourish, ..Swing::new(weapon) });
        if let Some(sound) = &sound {
            let game = settings.as_deref().copied().unwrap_or_default();
            commands.spawn((Name::new("swing"), AudioPlayer::new(sound.0.clone()), crate::audio::sound_effect(&game, &tuning), DespawnOnExit(crate::launch::AppState::Delving)));
        }
    }
}

/// The points of a creature's body a blow might head for: the centres of its bones' spheres
/// where the last pose put them, or a little above its root with no shaped bones.
fn body_points(critter: &Critter, root: &Transform, bones: Option<&Bones>, shapes: &Query<(&GlobalTransform, &HitShape)>) -> Vec<Vec3> {
    let points: Vec<Vec3> = bones.iter().flat_map(|b| b.0.iter()).filter_map(|&bone| shapes.get(bone).ok()).flat_map(|(at, shape)| shape.spheres(at)).map(|(centre, _)| centre).collect();
    if points.is_empty() { vec![root.translation + Vec3::Y * body_radius(critter)] } else { points }
}

/// Where a blow at a creature heads in the view model: of `points` (world space), the one
/// nearest the crosshair as `eye` sees it, carried to the same place on the screen at
/// [`CROSSHAIR_AIM`]'s depth (the view model is drawn through a narrower field of view than
/// the world). Kept toward the middle, so a creature at the edge of the view only bends the
/// swing, never throws it off the screen. `None` if every point is behind the eye.
pub fn toward_target(eye: &GlobalTransform, points: Vec<Vec3>, view: &crate::tuning::ViewTuning) -> Option<Vec3> {
    let into_eye = eye.affine().inverse();
    let screen = points
        .into_iter()
        .filter_map(|p| crate::view_model::on_screen(into_eye.transform_point3(p), view.world_fov_degrees))
        .min_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))?;
    let screen = screen.clamp(Vec2::splat(-TOWARD_LIMIT), Vec2::splat(TOWARD_LIMIT));
    let depth = -CROSSHAIR_AIM.z;
    Some((screen * depth * (view.held_fov_degrees.to_radians() * 0.5).tan()).extend(-depth))
}

/// How far from the centre of the screen a blow still follows its creature, as a fraction of
/// the half height. Our own.
const TOWARD_LIMIT: f32 = 0.6;

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
    pub const REST: SwingPose = SwingPose { offset: Vec3::ZERO, turn: Quat::IDENTITY };

    /// Applied to `rest` (item space to view space) about `fist`, in view space.
    pub fn apply(&self, rest: Transform, fist: Vec3) -> Transform {
        Transform { translation: fist + self.offset + self.turn * (rest.translation - fist), rotation: self.turn * rest.rotation, ..rest }
    }
}

/// The point on the crosshair a blow heads for with no creature in reach, view model camera
/// space: 3 m straight ahead, about a long weapon's reach. Our own.
pub const CROSSHAIR_AIM: Vec3 = Vec3::new(0.0, 0.0, -30.0);

/// Which way a blow comes in, as the delver sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Strike {
    /// Straight down from above.
    Overhead,
    /// Down and across from the upper right: the forehand's natural blow.
    HighRight,
    /// Down and across from the upper left: backhand.
    HighLeft,
    /// Flat across from the right.
    FromRight,
    /// Flat across from the left: backhand.
    FromLeft,
    /// Up and across from the lower right.
    Rising,
    /// Point first, straight at it.
    Stab,
}

impl Strike {
    /// The way the blow travels across the screen (+X right, +Y up). A stab's is the way it
    /// hooks in as it drives.
    pub fn travel(self) -> Vec2 {
        let degrees: f32 = match self {
            Strike::Overhead => -90.0,
            Strike::HighRight => -140.0,
            Strike::HighLeft => -40.0,
            Strike::FromRight => 180.0,
            Strike::FromLeft => 0.0,
            Strike::Rising => 140.0,
            Strike::Stab => 150.0,
        };
        Vec2::from_angle(degrees.to_radians())
    }

    /// The strikes a swing of `kind` is thrown as, and how often each, commonest first. Our own:
    /// a club falls mostly from above, a sword cuts from every side, a staff swings flat, and a
    /// spear or dagger stabs.
    pub fn choices(kind: SwingKind) -> &'static [(Strike, u32)] {
        use Strike::*;
        match kind {
            SwingKind::Chop => &[(Overhead, 4), (HighRight, 3), (HighLeft, 2), (FromRight, 1)],
            SwingKind::Slash => &[(HighRight, 3), (FromRight, 3), (HighLeft, 2), (FromLeft, 2), (Overhead, 1), (Rising, 1)],
            SwingKind::Sweep => &[(FromRight, 3), (FromLeft, 3), (Overhead, 1), (Rising, 1)],
            SwingKind::Thrust | SwingKind::Punch => &[(Stab, 1)],
        }
    }

    /// A strike for a swing of `kind`: any but `last`, unless it is the only one.
    pub fn roll(kind: SwingKind, last: Option<Strike>, rng: &mut procgen_rng::Mulberry32) -> Strike {
        let all = Strike::choices(kind);
        let fresh: Vec<(Strike, u32)> = all.iter().copied().filter(|&(s, _)| Some(s) != last).collect();
        *rng.weighted(if fresh.is_empty() { all } else { &fresh })
    }
}

/// How the held item lies at rest, view model camera space: its fist, the way from the fist to
/// its point, and the way its edge faces. The doll fits a blade edge first and an axe bit first,
/// facing the way the body does ([`crate::view_model`]), so its edge faces the view's ahead as
/// the hold turns it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grip {
    pub fist: Vec3,
    pub pointing: Vec3,
    pub edge: Vec3,
    /// How far the point is from the fist; zero for a light, which has none.
    pub reach: f32,
}

impl Grip {
    /// The grip of an item resting at `rest` (its space to view space) in `hold`, with its point
    /// at `point` (its own space; `None` for a light, which is swung from its middle).
    pub fn of(rest: Transform, hold: ViewHold, point: Option<Vec3>) -> Grip {
        let tip = point.map_or(rest.translation, |p| rest.transform_point(p));
        let pointing = (tip - hold.fist).normalize_or(Vec3::Y);
        let reach = if point.is_some() { (tip - hold.fist).length() } else { 0.0 };
        let edge = (hold.turn * Vec3::NEG_Z).reject_from_normalized(pointing).normalize_or(pointing.any_orthonormal_vector());
        Grip { fist: hold.fist, pointing, edge, reach }
    }

    /// The turn about the fist that lays the point along `pointing` with the edge toward `edge`
    /// (both unit, and square to each other).
    fn turn_to(&self, pointing: Vec3, edge: Vec3) -> Quat {
        let from = Mat3::from_cols(self.pointing, self.edge, self.pointing.cross(self.edge));
        let to = Mat3::from_cols(pointing, edge, pointing.cross(edge));
        Quat::from_mat3(&(to * from.transpose())).normalize()
    }
}

/// A strike's sweep: where it starts and ends, as an angle from straight at where the blow
/// heads (radians, toward the way it travels): drawn back past the shoulder, and followed
/// through a little past the blow. Our own.
const WOUND: f32 = -1.6;
const FOLLOWED: f32 = 0.5;
/// How far the fist swings with its arc, view model units: across, the way the strike travels,
/// and in depth, drawn back toward the eye. Our own: wide enough to read, kept narrow in depth
/// so the grip, with no hand drawn round it, never looms at the eye.
const SWEEP_ACROSS: f32 = 3.5;
const SWEEP_DEPTH: f32 = 1.0;
/// How far toward the middle of the view a strike's arc is centred, as a share of the fist's
/// offset from it at rest, and how far toward where it heads. Our own.
const SWEEP_PULL: f32 = 0.15;
const SWEEP_REACH: f32 = 2.0;
/// How far a stab draws back along its line and drives along it, and how wide it hooks in,
/// view model units. Our own.
const STAB_BACK: f32 = 2.5;
const STAB_DRIVE: f32 = 7.0;
const STAB_HOOK: f32 = 1.5;

/// A strike swept through its plane: the plane through the fist's line to where the blow heads
/// (`aim`) and the way it travels across it (`travel`, square to `aim`). The item turns about
/// the plane's normal, edge leading, and the fist swings with it, across the way the strike
/// travels and a little back toward the eye.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sweep {
    aim: Vec3,
    travel: Vec3,
    /// The turn laying the item straight at where the blow heads, edge toward `travel`.
    square: Quat,
    /// The middle of the fist's arc, as an offset from where it rests.
    centre: Vec3,
    /// How far the fist swings across, [`SWEEP_ACROSS`] give or take the flourish.
    across: f32,
}

impl Sweep {
    /// The pose `angle` radians round the sweep from straight at where the blow heads.
    fn pose(&self, angle: f32) -> SwingPose {
        let turn = Quat::from_axis_angle(self.aim.cross(self.travel), angle) * self.square;
        SwingPose { offset: self.centre + self.travel * angle.sin() * self.across + self.aim * (angle.cos() - 1.0) * SWEEP_DEPTH, turn }
    }
}

/// The path of one swing: drawn back from rest into its strike, thrown through, recovered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwingPath {
    back: SwingPose,
    through: SwingPose,
    /// A strike swept through its plane, and the angles it is thrown between; `None` for a stab.
    sweep: Option<(Sweep, f32, f32)>,
    /// The controls of the curves the fist follows: drawing back, a stab's hook, recovering.
    draw: Vec3,
    hook: Vec3,
    settle: Vec3,
}

impl SwingPath {
    /// The path of a `strike` from an item held as `grip` to `toward` (view model camera space),
    /// with its own `flourish` (each part -1..1): the way it travels turned a little, drawn back
    /// a little more or less, swung a little wider or tighter.
    pub fn new(strike: Strike, grip: Grip, toward: Vec3, flourish: Vec3) -> SwingPath {
        let aim = (toward - grip.fist).normalize_or(Vec3::NEG_Z);
        let travel = Vec2::from_angle(flourish.x * 0.3).rotate(strike.travel()).extend(0.0);
        let travel = travel.reject_from_normalized(aim).normalize_or(aim.any_orthonormal_vector());
        let square = grip.turn_to(aim, travel);
        let (back, through, sweep, hook) = if strike == Strike::Stab {
            let back = SwingPose { offset: -aim * STAB_BACK - travel * STAB_HOOK, turn: square };
            // The fist is off to one side, so a point driven along the line from it lands off the
            // crosshair by the fist's offset; shift the throw so the point itself lands on it.
            let mut drive = aim * STAB_DRIVE;
            if grip.reach > 0.0 && toward.z < 0.0 {
                let point = grip.fist + drive + aim * grip.reach;
                drive += toward * (point.z / toward.z) - point;
            }
            let through = SwingPose { offset: drive, turn: square };
            (back, through, None, back.offset.lerp(through.offset, 0.5) - travel * STAB_HOOK)
        } else {
            let centre = Vec3::new(-grip.fist.x, -grip.fist.y, 0.0) * SWEEP_PULL + aim * SWEEP_REACH;
            let sweep = Sweep { aim, travel, square, centre, across: SWEEP_ACROSS * (1.0 + 0.2 * flourish.z) };
            let (from, to) = (WOUND + 0.25 * flourish.y, FOLLOWED);
            (sweep.pose(from), sweep.pose(to), Some((sweep, from, to)), Vec3::ZERO)
        };
        // Drawn back a little toward the eye first; recovered low.
        let draw = back.offset * 0.5 + Vec3::Z * 0.5;
        let settle = through.offset * 0.5 + Vec3::new(0.0, -1.5, 0.5);
        SwingPath { back, through, sweep, draw, hook, settle }
    }

    /// The pose `t` seconds into the swing: drawn back, thrown through, recovered, each eased.
    pub fn at(&self, t: f32) -> SwingPose {
        let u = t / SWING_TIME;
        let (rest, back, through) = (SwingPose::REST, self.back, self.through);
        if u < DRAWN {
            let s = smoothstep(u / DRAWN);
            SwingPose { offset: quadratic(rest.offset, self.draw, back.offset, s), turn: rest.turn.slerp(back.turn, s) }
        } else if u < THROWN {
            let s = smoothstep((u - DRAWN) / (THROWN - DRAWN));
            match self.sweep {
                Some((sweep, from, to)) => sweep.pose(from + (to - from) * s),
                None => SwingPose { offset: quadratic(back.offset, self.hook, through.offset, s), turn: back.turn.slerp(through.turn, s) },
            }
        } else {
            let s = smoothstep((u - THROWN) / (1.0 - THROWN));
            SwingPose { offset: quadratic(through.offset, self.settle, rest.offset, s), turn: through.turn.slerp(rest.turn, s) }
        }
    }
}

fn quadratic(a: Vec3, b: Vec3, c: Vec3, s: f32) -> Vec3 {
    a.lerp(b, s).lerp(b.lerp(c, s), s)
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
    let hold = crate::view_model::view_hold(hold, Side::Right);
    for (item, mut transform, parent) in &mut held {
        if item.hand == fb_items::Slot::Mainhand && view_models.contains(parent.parent()) {
            let grip = Grip::of(*transform, hold, item.point);
            *transform = swing.path(grip).at(swing.t).apply(*transform, hold.fist);
        }
    }
}

/// Leans the eye forward into each blow.
pub(crate) fn lean_into_blow(swings: Query<&Swing, With<Player>>, mut cameras: Query<&mut Transform, (With<PlayerCamera>, Without<Player>)>) {
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
    use crate::view_model::{held_in_view, view_hold};

    /// A delver at a table with nothing to hit, and the attack key pressed.
    fn swinger() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<crate::keymap::Keymap>()
            .init_resource::<Inventory>()
            .init_resource::<Character>()
            .init_resource::<RunDice>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<crate::inventory::Toast>()
            .add_systems(Update, start_swing);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        let delver = app.world_mut().spawn((Player, Transform::default())).id();
        (app, delver)
    }

    #[test]
    fn each_swing_takes_a_fixed_bite_of_stamina() {
        let (mut app, delver) = swinger();
        app.update();
        assert!(app.world().get::<Swing>(delver).is_some());
        let value = app.world().get::<DelverStamina>(delver).unwrap().0.value;
        assert!((value - (1.0 - fb_character::SWING_COST)).abs() < 1e-9, "{value}");
        app.world_mut().entity_mut(delver).remove::<Swing>();
        app.update();
        let value = app.world().get::<DelverStamina>(delver).unwrap().0.value;
        assert!((value - (1.0 - 2.0 * fb_character::SWING_COST)).abs() < 1e-9, "{value}");
    }

    #[test]
    fn a_winded_delver_cannot_swing() {
        let (mut app, delver) = swinger();
        app.world_mut().get_mut::<DelverStamina>(delver).unwrap().0 = fb_character::Stamina { value: 0.05, winded: true };
        app.update();
        assert!(app.world().get::<Swing>(delver).is_none());
        assert_eq!(app.world().get::<DelverStamina>(delver).unwrap().0.value, 0.05, "refused, so no bite");
        app.world_mut().get_mut::<DelverStamina>(delver).unwrap().0 = fb_character::Stamina { value: 0.25, winded: false };
        app.update();
        assert!(app.world().get::<Swing>(delver).is_some());
    }

    #[test]
    fn a_winded_swing_says_so_once_while_the_line_shows() {
        let (mut app, delver) = swinger();
        app.world_mut().get_mut::<DelverStamina>(delver).unwrap().0 = fb_character::Stamina { value: 0.05, winded: true };
        app.update();
        let (text, until) = {
            let toast = app.world().resource::<crate::inventory::Toast>();
            (toast.text.clone(), toast.until)
        };
        assert_eq!(text, WINDED);
        assert!(until > 0.0);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        assert_eq!(app.world().resource::<crate::inventory::Toast>().until, until, "not said again while showing");
    }

    const STRIKES: [Strike; 6] = [Strike::Overhead, Strike::HighRight, Strike::HighLeft, Strike::FromRight, Strike::FromLeft, Strike::Rising];

    /// An item of `base` held in the main hand: where it rests, its grip, and its point.
    fn held(base: &str, seed: u32) -> (Transform, Grip, Vec3) {
        let view = crate::tuning::Tuning::default().view;
        let spec = fb_body::BodySpec::roll(fb_body::Folk::Human, 1207);
        let item = fb_items::roll_item(base, seed).unwrap().shape;
        let held = &held_in_view(&spec, Some(&item), None, view.held_scale)[0];
        (held.transform, Grip::of(held.transform, view_hold(held.hold, Side::Right), Some(held.point)), held.point)
    }

    /// Where the item's point is at `t` seconds into `path`.
    fn tip(path: &SwingPath, t: f32, rest: Transform, grip: Grip, point: Vec3) -> Vec3 {
        path.at(t).apply(rest, grip.fist).transform_point(point)
    }

    #[test]
    fn every_strike_draws_back_throws_through_and_comes_to_rest() {
        let (_, grip, _) = held("Sword", 9);
        for strike in STRIKES.into_iter().chain([Strike::Stab]) {
            let path = SwingPath::new(strike, grip, CROSSHAIR_AIM, Vec3::new(0.7, -0.4, 0.5));
            assert_eq!(path.at(0.0), SwingPose::REST, "{strike:?}");
            let end = path.at(SWING_TIME);
            assert!(end.offset.length() < 1e-4 && end.turn.angle_between(Quat::IDENTITY) < 1e-3, "{strike:?}");
        }
        // The blow lands at the end of the throw.
        const { assert!(LANDS > DRAWN * SWING_TIME && LANDS <= THROWN * SWING_TIME) };
    }

    #[test]
    fn a_strike_comes_from_its_side_and_sweeps_across() {
        let (rest, grip, point) = held("Sword", 9);
        for strike in STRIKES {
            let path = SwingPath::new(strike, grip, CROSSHAIR_AIM, Vec3::ZERO);
            let (back, through) = (tip(&path, DRAWN * SWING_TIME, rest, grip, point), tip(&path, THROWN * SWING_TIME, rest, grip, point));
            // The point travels across the screen the strike's way, from drawn back to thrown.
            let moved = (through - back).xy().normalize();
            assert!(moved.dot(strike.travel()) > 0.6, "{strike:?}: moved {moved}");
            // It arcs: midway it is well off the straight line between.
            let mid = tip(&path, (DRAWN + THROWN) * 0.5 * SWING_TIME, rest, grip, point);
            assert!(mid.distance(back.lerp(through, 0.5)) > 2.0, "{strike:?}");
        }
    }

    #[test]
    fn the_edge_leads_through_the_sweep() {
        let (rest, grip, point) = held("Sword", 9);
        for strike in STRIKES {
            let path = SwingPath::new(strike, grip, CROSSHAIR_AIM, Vec3::ZERO);
            let t = (DRAWN + THROWN) * 0.5 * SWING_TIME;
            let going = (tip(&path, t + 0.005, rest, grip, point) - tip(&path, t, rest, grip, point)).normalize();
            let edge = path.at(t).turn * grip.edge;
            assert!(edge.dot(going) > 0.9, "{strike:?}: edge {edge} going {going}");
        }
    }

    #[test]
    fn a_strike_heads_for_its_target() {
        let (rest, grip, point) = held("Mace", 3);
        let left = Vec3::new(-8.0, 4.0, CROSSHAIR_AIM.z);
        for strike in STRIKES {
            let at = |toward| tip(&SwingPath::new(strike, grip, toward, Vec3::ZERO), LANDS, rest, grip, point);
            let (ahead, aside) = (at(CROSSHAIR_AIM), at(left));
            assert!(aside.x < ahead.x - 1.0 && aside.y > ahead.y, "{strike:?}: {aside} against {ahead}");
        }
    }

    #[test]
    fn a_stab_drives_the_point_at_the_crosshair() {
        let view = crate::tuning::Tuning::default().view;
        let on_screen = |p| crate::view_model::on_screen(p, view.held_fov_degrees);
        for (base, seed) in [("Spear", 4), ("Spear", 9), ("Dagger", 5), ("Dagger", 1207)] {
            let (rest, grip, point) = held(base, seed);
            let along = (CROSSHAIR_AIM - grip.fist).normalize();
            let off_line = |t: Transform, fist: Vec3| (t.transform_point(point) - fist).normalize().angle_between(along);
            if base == "Spear" {
                assert!(off_line(rest, grip.fist) > 0.5, "{base} {seed}: at rest {}", off_line(rest, grip.fist));
            }
            // Drawn back, it is already turned point first down the line; thrown through, the
            // point is driven along it, and a spear's reaches the crosshair.
            let path = SwingPath::new(Strike::Stab, grip, CROSSHAIR_AIM, Vec3::new(0.5, 0.5, 0.5));
            for phase in [DRAWN, THROWN] {
                let pose = path.at(phase * SWING_TIME);
                assert!(off_line(pose.apply(rest, grip.fist), grip.fist + pose.offset) < 0.02, "{base} {seed} at {phase}");
            }
            let thrown = tip(&path, THROWN * SWING_TIME, rest, grip, point);
            assert!(thrown.z < rest.transform_point(point).z - 4.0, "{base} {seed}: driven ahead");
            if base == "Spear" {
                let screen = on_screen(thrown).unwrap();
                assert!(screen.length() < 0.15, "{base} {seed}: point at {screen} on the screen");
            }
        }
    }

    #[test]
    fn strikes_suit_the_weapon_and_never_repeat() {
        let mut rng = procgen_rng::Mulberry32::new(3);
        for kind in [SwingKind::Chop, SwingKind::Slash, SwingKind::Sweep] {
            let mut last = None;
            let mut seen = std::collections::HashSet::new();
            for _ in 0..200 {
                let strike = Strike::roll(kind, last, &mut rng);
                assert_ne!(Some(strike), last, "{kind:?}");
                assert!(Strike::choices(kind).iter().any(|&(s, _)| s == strike));
                seen.insert(strike);
                last = Some(strike);
            }
            assert_eq!(seen.len(), Strike::choices(kind).len(), "{kind:?}: every strike thrown");
        }
        assert_eq!(Strike::roll(SwingKind::Thrust, Some(Strike::Stab), &mut rng), Strike::Stab);
    }

    #[test]
    fn a_blow_heads_for_the_creature_on_the_screen() {
        let view = crate::tuning::Tuning::default().view;
        let eye = GlobalTransform::from(Transform::from_xyz(0.0, 15.0, 0.0).looking_to(Vec3::NEG_Z, Vec3::Y));
        // Straight ahead is the crosshair.
        let ahead = toward_target(&eye, vec![Vec3::new(0.0, 15.0, -20.0)], &view).unwrap();
        assert!(ahead.abs_diff_eq(CROSSHAIR_AIM, 1e-3), "{ahead}");
        // Low and to the left is low and to the left, the nearest of a body's points chosen.
        let low = toward_target(&eye, vec![Vec3::new(-6.0, 9.0, -20.0), Vec3::new(-3.0, 12.0, -20.0)], &view).unwrap();
        let screen = crate::view_model::on_screen(low, view.held_fov_degrees).unwrap();
        let expected = crate::view_model::on_screen(Vec3::new(-3.0, -3.0, -20.0), view.world_fov_degrees).unwrap();
        assert!(screen.abs_diff_eq(expected, 1e-3), "{screen} against {expected}");
        // Far off to the side, it is held toward the middle; behind the eye, nothing.
        let wide = toward_target(&eye, vec![Vec3::new(-40.0, 15.0, -5.0)], &view).unwrap();
        assert!(crate::view_model::on_screen(wide, view.held_fov_degrees).unwrap().x >= -TOWARD_LIMIT - 1e-4);
        assert!(toward_target(&eye, vec![Vec3::new(0.0, 15.0, 10.0)], &view).is_none());
    }

    #[test]
    fn a_pose_turns_about_the_fist() {
        let fist = Vec3::new(5.0, -6.0, -9.5);
        let rest = Transform::from_translation(fist + Vec3::Y * 3.0);
        let tipped = SwingPose { offset: Vec3::ZERO, turn: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2) }.apply(rest, fist);
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
