//! The creature view (`--creature`): one rat, snake, skeleton, troll or minotaur on the Armory turntable's plate
//! under the turntable's studio light, posed and animated as in the level, so creatures can be
//! judged close up as items and bodies are. `--skeleton` is the same view opened on a skeleton.
//!
//! Keys: Tab and Shift+Tab change the species, Left and Right the seed, M the motion (still,
//! moving, rearing, striking; a skeleton or a troll stands, walks and swings; a minotaur also gores and gestures), G a skeleton's gear or a troll's weapon on or
//! off, Space pauses the turn (`FB_VIEW_ANGLE=90` holds it side on). Command line: `--creature --species giant_rat --seed 7 --motion
//! moving`, or `--skeleton --seed 7 --motion swing [--bare]` ([`CreatureChoice::from_args`]).

use bevy::{camera::Exposure, prelude::*, render::view::Tonemapping, ui::IsDefaultUiCamera};
use fb_creatures::pose::{self, ArmStance, BatPose, CentipedePose, DenizenPose, JackalopePose, RatPose, SkeletonPose, SnakePose, SpiderPose};
use fb_creatures::skeleton::SkeletonDims;
use fb_creatures::{Creature, SkeletonGear, Species};

use crate::carry::appearance_of;
use crate::creatures::{transform_of, tongue_flick};
use crate::light::StudioLight;
use crate::render::{ItemAppearance, ItemSpawner};
use crate::turntable::{PLATE, PLATE_INK, SPIN_SPEED, framing_distance};

/// Share of the view the creature fills.
const FILL: f32 = 0.8;
/// How far above level the camera looks down, radians.
const ELEVATION: f32 = 0.42;
/// How much more room a moving minotaur is framed in than it needs at rest, across and up: its
/// blows and gestures reach well past where it stands.
const MINOTAUR_MOTION_ROOM: Vec3 = Vec3::new(1.5, 1.1, 1.5);

pub struct CreatureViewPlugin;

impl Plugin for CreatureViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CreatureChoice>()
            .init_resource::<ViewSpin>()
            .insert_resource(ClearColor(PLATE))
            .insert_resource(StudioLight::ARMORY.ambient_light(&Exposure::INDOOR))
            .add_systems(Startup, setup)
            .add_systems(Update, (keys, show_creature.run_if(resource_changed::<CreatureChoice>), spin, move_creature, frame_camera, show_status).chain());
    }
}

/// How the shown creature moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMotion {
    Still,
    Moving,
    Rearing,
    Striking,
    /// A denizen's gore with its horns.
    Goring,
    /// A denizen's gesture of speech.
    Gesturing,
}

impl ViewMotion {
    pub const ALL: [ViewMotion; 4] = [ViewMotion::Still, ViewMotion::Moving, ViewMotion::Rearing, ViewMotion::Striking];

    fn parse(name: &str) -> Option<ViewMotion> {
        let alias = match name {
            "stand" => Some(ViewMotion::Still),
            "walk" => Some(ViewMotion::Moving),
            "swing" | "swinging" | "bite" => Some(ViewMotion::Striking),
            "gore" => Some(ViewMotion::Goring),
            "gesture" | "talk" | "speak" => Some(ViewMotion::Gesturing),
            "hang" | "roost" => Some(ViewMotion::Still),
            "glide" => Some(ViewMotion::Rearing),
            "flap" | "fly" => Some(ViewMotion::Moving),
            _ => None,
        };
        alias.or_else(|| ViewMotion::ALL.into_iter().find(|m| m.key() == name))
    }

    /// The motions `species` has: a skeleton or a troll stands, walks and swings; the rest as listed.
    pub fn of(species: Species) -> &'static [ViewMotion] {
        if species.is_minotaur() {
            &[ViewMotion::Still, ViewMotion::Moving, ViewMotion::Striking, ViewMotion::Goring, ViewMotion::Gesturing]
        } else if species.is_skeleton() || species.is_troll() {
            &[ViewMotion::Still, ViewMotion::Moving, ViewMotion::Striking]
        } else {
            &ViewMotion::ALL
        }
    }

    fn key(self) -> &'static str {
        match self {
            ViewMotion::Still => "still",
            ViewMotion::Moving => "moving",
            ViewMotion::Rearing => "rearing",
            ViewMotion::Striking => "striking",
            ViewMotion::Goring => "goring",
            ViewMotion::Gesturing => "gesturing",
        }
    }
}

/// What the creature view shows.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatureChoice {
    pub species: Species,
    pub seed: u32,
    pub motion: ViewMotion,
    /// Whether a skeleton wears and holds the gear its seed rolls.
    pub gear: bool,
}

impl Default for CreatureChoice {
    fn default() -> Self {
        CreatureChoice { species: Species::Rat, seed: 1207, motion: ViewMotion::Still, gear: true }
    }
}

/// A species from its command-line name.
pub fn parse_species(name: &str) -> Option<Species> {
    match name.to_ascii_lowercase().replace('-', "_").as_str() {
        "rat" => Some(Species::Rat),
        "giant_rat" | "giant" => Some(Species::GiantRat),
        "poisonous_snake" | "snake" | "viper" => Some(Species::PoisonousSnake),
        "constrictor_snake" | "constrictor" => Some(Species::ConstrictorSnake),
        "skeleton" => Some(Species::Skeleton),
        "giant_spider" | "spider" => Some(Species::GiantSpider),
        "troll" => Some(Species::Troll),
        "minotaur" => Some(Species::Minotaur),
        "jackalope" => Some(Species::Jackalope),
        "giant_centipede" | "centipede" => Some(Species::GiantCentipede),
        "bat" => Some(Species::Bat),
        "giant_bat" => Some(Species::GiantBat),
        _ => None,
    }
}

impl CreatureChoice {
    /// Parses `--species <name>`, `--seed <n>` and `--motion <still|moving|rearing|striking>`
    /// (also `--flag=value`; a skeleton's motions also go by stand, walk and swing);
    /// `--creature` itself is skipped, `--skeleton` chooses a skeleton and `--bare` takes its
    /// gear off.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<CreatureChoice, String> {
        let mut choice = CreatureChoice::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (arg, None),
            };
            let mut value = || inline.clone().or_else(|| args.next()).ok_or(format!("{flag} needs a value"));
            match flag.as_str() {
                "--creature" => {}
                "--skeleton" => choice.species = Species::Skeleton,
                "--bare" => choice.gear = false,
                "--species" => {
                    let name = value()?;
                    choice.species = parse_species(&name).ok_or(format!("unknown species {name:?}; one of rat, giant_rat, viper, constrictor, skeleton, giant_spider, troll, minotaur, jackalope, giant_centipede, bat, giant_bat"))?;
                }
                "--seed" => {
                    let seed = value()?;
                    choice.seed = seed.parse().map_err(|_| format!("--seed: {seed:?} is not a u32"))?;
                }
                "--motion" => {
                    let name = value()?;
                    choice.motion = ViewMotion::parse(&name).ok_or(format!("unknown motion {name:?}; one of still, moving, rearing, striking, goring, gesturing (stand, walk, swing, gore, gesture)"))?;
                }
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        Ok(choice)
    }

    pub fn creature(&self) -> Creature {
        Creature::of(self.species, self.seed)
    }
}

#[derive(Resource)]
struct ViewSpin {
    angle: f32,
    paused: bool,
}

impl Default for ViewSpin {
    /// Turning from the front, unless `FB_VIEW_ANGLE` (degrees) holds it at an angle, for a
    /// capture from the side or the back.
    fn default() -> Self {
        match std::env::var("FB_VIEW_ANGLE").ok().and_then(|v| v.trim().parse::<f32>().ok()) {
            Some(degrees) => ViewSpin { angle: degrees.to_radians(), paused: true },
            None => ViewSpin { angle: 0.0, paused: false },
        }
    }
}

/// The turning pivot, the shown creature's bone entities, and the size to frame.
#[derive(Component)]
struct Shown {
    creature: Creature,
    bones: Vec<Entity>,
    size: Vec3,
    time: f32,
    cycle: f32,
    /// A skeleton's or a troll's measured dimensions, kept so a pose each frame does not rebuild its body.
    dims: Option<SkeletonDims>,
    /// The stance of the arms of a skeleton that holds something.
    stance: [Option<ArmStance>; 2],
}

#[derive(Component)]
struct ViewCamera;

#[derive(Component)]
struct StatusLine;

fn setup(mut commands: Commands) {
    let exposure = Exposure::INDOOR;
    commands
        .spawn((ViewCamera, Camera3d::default(), exposure, Tonemapping::Linear, IsDefaultUiCamera, Transform::from_xyz(0.0, 10.0, 60.0)))
        .with_child(StudioLight::ARMORY.key_light(&exposure));
    commands.spawn((
        StatusLine,
        Text::default(),
        TextFont::from_font_size(18.0),
        TextColor(PLATE_INK),
        Node { position_type: PositionType::Absolute, top: px(16.0), left: px(20.0), right: px(20.0), ..default() },
    ));
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut choice: ResMut<CreatureChoice>, mut spin: ResMut<ViewSpin>) {
    let pressed = |key| keys.just_pressed(key);
    if pressed(KeyCode::Tab) {
        let back = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
        let at = Species::ALL.iter().position(|&s| s == choice.species).unwrap_or(0) as isize;
        choice.species = Species::ALL[(at + if back { -1 } else { 1 }).rem_euclid(Species::ALL.len() as isize) as usize];
        if !ViewMotion::of(choice.species).contains(&choice.motion) {
            choice.motion = ViewMotion::Still;
        }
    }
    if pressed(KeyCode::ArrowRight) {
        choice.seed = choice.seed.wrapping_add(1);
    }
    if pressed(KeyCode::ArrowLeft) {
        choice.seed = choice.seed.wrapping_sub(1);
    }
    if pressed(KeyCode::KeyM) {
        let motions = ViewMotion::of(choice.species);
        let at = motions.iter().position(|&m| m == choice.motion).unwrap_or(0);
        choice.motion = motions[(at + 1) % motions.len()];
    }
    if pressed(KeyCode::KeyG) && (choice.species.is_skeleton() || choice.species.is_troll()) {
        choice.gear = !choice.gear;
    }
    if pressed(KeyCode::Space) {
        spin.paused = !spin.paused;
    }
}

fn show_creature(mut commands: Commands, choice: Res<CreatureChoice>, old: Query<Entity, With<Shown>>, mut spawner: ItemSpawner) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let creature = choice.creature();
    // A skeleton wears and holds what its seed rolls: its rig is the gear's, with the hands closed.
    let gear = match &creature {
        Creature::Skeleton(skeleton) if choice.gear => Some(SkeletonGear::roll(skeleton, fb_items::Odds::kit(1))),
        // A troll holds the weapon its seed rolls, if any.
        Creature::Troll(troll) if choice.gear => Some(SkeletonGear::roll_troll(troll, fb_items::Odds::kit(1))),
        _ => None,
    };
    let rig = gear.as_ref().map_or_else(|| creature.rig(), |g| g.rig.clone());
    let stance = gear.as_ref().map_or([None, None], |g| g.stance);
    let Some((lo, hi)) = rig.rest_mesh().bounds() else { return };
    // Gear in the stance can reach beyond the bones: a raised shield, a weapon's length.
    let (lo, hi) = match (&gear, &creature) {
        (Some(gear), Creature::Skeleton(skeleton)) => {
            let at_rest = pose::skeleton_armed(&skeleton.dims(), &SkeletonPose::default(), &gear.stance);
            let mut meshes = gear.posed(&at_rest);
            meshes.push(rig.posed_mesh(&at_rest));
            meshes.iter().filter_map(|m| m.bounds()).fold((lo, hi), |(lo, hi), (l, h)| ([0, 1, 2].map(|i| lo[i].min(l[i])), [0, 1, 2].map(|i| hi[i].max(h[i]))))
        }
        (Some(gear), Creature::Troll(troll)) => {
            let at_rest = pose::troll_armed(&troll.dims(), &SkeletonPose::default(), &gear.stance);
            let mut meshes = gear.posed(&at_rest);
            meshes.push(rig.posed_mesh(&at_rest));
            meshes.iter().filter_map(|m| m.bounds()).fold((lo, hi), |(lo, hi), (l, h)| ([0, 1, 2].map(|i| lo[i].min(l[i])), [0, 1, 2].map(|i| hi[i].max(h[i]))))
        }
        _ => (lo, hi),
    };
    let reach = Vec2::new(lo[0].abs().max(hi[0]), lo[2].abs().max(hi[2]));
    let mut size = Vec3::new(reach.x * 2.0, hi[1] - lo[1], reach.y * 2.0);
    if let Creature::Snake(snake) = &creature
        && choice.motion != ViewMotion::Still
    {
        // Laid out to move or rearing, the snake reaches further than coiled.
        let laid = rig.posed_mesh(&pose::snake(snake, &SnakePose { coil: 0.0, rear: 1.0, ..SnakePose::REST })).bounds().unwrap_or((lo, hi));
        let reach = Vec2::new(laid.0[0].abs().max(laid.1[0]), laid.0[2].abs().max(laid.1[2]));
        size = size.max(Vec3::new(reach.x * 2.0, laid.1[1] - laid.0[1], reach.y * 2.0));
    }
    // A minotaur's blows and gestures reach well past where it stands at rest.
    if matches!(creature, Creature::Minotaur(_)) && choice.motion != ViewMotion::Still {
        size *= MINOTAUR_MOTION_ROOM;
    }
    if matches!(creature, Creature::Jackalope(_)) && choice.motion == ViewMotion::Rearing {
        // Sat up it stands well taller than at rest.
        size.y *= 1.7;
    }
    // A bat is shown in the air, its middle at 1.5 body lengths up.
    if let Creature::Bat(bat) = &creature {
        size.y = 3.0 * bat.length;
    }
    let appearance = ItemAppearance { seed: choice.seed, ..default() };
    let pivot = commands.spawn((Transform::from_xyz(0.0, 0.0, 0.0), Visibility::default())).id();
    let mut bones: Vec<Entity> = Vec::new();
    for bone in &rig.bones {
        let parent = bone.parent.map_or(pivot, |p| bones[p]);
        let entity = commands.spawn((transform_of(&bone.rest), Visibility::default(), ChildOf(parent))).id();
        if !bone.mesh.parts.is_empty() {
            spawner.spawn(&bone.mesh, &appearance).insert(ChildOf(entity));
        }
        bones.push(entity);
    }
    if let Some(gear) = &gear {
        for piece in &gear.pieces {
            spawner.spawn(&piece.mesh, &appearance_of(&piece.item)).insert((Name::new(piece.name.clone()), ChildOf(bones[piece.bone])));
        }
    }
    let dims = match &creature {
        Creature::Skeleton(skeleton) => Some(skeleton.dims()),
        Creature::Troll(troll) => Some(troll.dims()),
        Creature::Minotaur(minotaur) => Some(minotaur.dims()),
        Creature::Rat(_) | Creature::Snake(_) | Creature::Spider(_) | Creature::Jackalope(_) | Creature::Centipede(_) | Creature::Bat(_) => None,
    };
    commands.entity(pivot).insert(Shown { creature, bones, size, time: 0.0, cycle: 0.0, dims, stance });
}

fn spin(time: Res<Time>, mut state: ResMut<ViewSpin>, mut pivots: Query<&mut Transform, With<Shown>>) {
    if !state.paused {
        state.angle = (state.angle + SPIN_SPEED * time.delta_secs()) % std::f32::consts::TAU;
    }
    for mut transform in &mut pivots {
        transform.rotation = Quat::from_rotation_y(state.angle);
    }
}

/// Poses the creature as its motion says: walking in place at its own speed, rearing, or
/// striking over and over.
fn move_creature(time: Res<Time>, choice: Res<CreatureChoice>, mut shown: Query<&mut Shown>, mut transforms: Query<&mut Transform, Without<Shown>>) {
    let dt = time.delta_secs();
    for shown in &mut shown {
        let shown = shown.into_inner();
        shown.time += dt;
        let t = shown.time;
        let moving = choice.motion == ViewMotion::Moving;
        let speed = if moving { choice.species.speed() } else { 0.0 };
        // A strike every two seconds: drawn back, thrown in a tenth of a second, held, back.
        let strike = if choice.motion == ViewMotion::Striking { strike_curve(t % 2.0) } else { 0.0 };
        let places = match &shown.creature {
            Creature::Rat(rat) => {
                shown.cycle = (shown.cycle + speed * dt / pose::rat_stride(rat) * std::f32::consts::TAU) % std::f32::consts::TAU;
                pose::rat(rat, &RatPose { stride: shown.cycle, pace: if moving { 1.0 } else { 0.0 }, time: t, bite: strike, look: 0.0 })
            }
            Creature::Snake(snake) => {
                shown.cycle += speed * dt / (pose::WAVELENGTH * snake.dims().body_length) * std::f32::consts::TAU;
                let (coil, rear) = match choice.motion {
                    ViewMotion::Still => (1.0, 0.45),
                    ViewMotion::Moving => (0.0, 0.0),
                    ViewMotion::Rearing | ViewMotion::Striking | ViewMotion::Goring | ViewMotion::Gesturing => (0.65, 1.0),
                };
                pose::snake(snake, &SnakePose { phase: shown.cycle, pace: if moving { 1.0 } else { 0.0 }, coil, rear, strike, jaw: strike, tongue: tongue_flick(t), look: 0.0, ..SnakePose::REST })
            }
            Creature::Spider(spider) => {
                shown.cycle = (shown.cycle + speed * dt / pose::spider_stride(spider) * std::f32::consts::TAU) % std::f32::consts::TAU;
                let rear = if matches!(choice.motion, ViewMotion::Rearing | ViewMotion::Striking) { 1.0 } else { 0.0 };
                pose::spider(spider, &SpiderPose { stride: shown.cycle, pace: if moving { 1.0 } else { 0.0 }, time: t, rear, bite: strike, curl: 0.0 })
            }
            Creature::Jackalope(jackalope) => {
                shown.cycle = (shown.cycle + speed * dt / pose::jackalope_stride(jackalope) * std::f32::consts::TAU) % std::f32::consts::TAU;
                // Rearing sits it up with the ears turning; striking gores.
                let sit = if choice.motion == ViewMotion::Rearing { 1.0 } else { 0.0 };
                pose::jackalope(jackalope, &JackalopePose { stride: shown.cycle, pace: if moving { 1.0 } else { 0.0 }, time: t, sit, gore: strike, look: 0.0 })
            }
            Creature::Minotaur(minotaur) => {
                let dims = shown.dims.get_or_insert_with(|| minotaur.dims());
                shown.cycle = (shown.cycle + speed * dt / pose::denizen_stride(dims) * std::f32::consts::TAU) % std::f32::consts::TAU;
                // A chop or a gore every 2.4 seconds, wound up and thrown over 1.6 and then held
                // at the guard; a gesture a phrase of four seconds, over and over.
                let blow = (t % 2.4 / 1.6).min(1.0);
                let (swing, gore) = match choice.motion {
                    ViewMotion::Striking => (Some(blow), None),
                    ViewMotion::Goring => (None, Some(blow)),
                    _ => (None, None),
                };
                let gesture = (choice.motion == ViewMotion::Gesturing).then(|| t % 4.0 / 4.0);
                pose::minotaur_with(dims, &DenizenPose { stride: shown.cycle, pace: if moving { 1.0 } else { 0.0 }, time: t, swing, gore, gesture, look: 0.0, jaw: 0.0 })
            }
            Creature::Centipede(centipede) => {
                shown.cycle = (shown.cycle + speed * dt / pose::centipede_stride(centipede) * std::f32::consts::TAU) % std::f32::consts::TAU;
                let rear = match choice.motion {
                    ViewMotion::Still | ViewMotion::Moving | ViewMotion::Gesturing => 0.0,
                    ViewMotion::Rearing => 0.9,
                    ViewMotion::Striking | ViewMotion::Goring => 0.6,
                };
                pose::centipede(centipede, &CentipedePose { stride: shown.cycle, pace: if moving { 1.0 } else { 0.0 }, time: t, rear, bite: strike, jaw: strike, look: 0.0, curl: 0.0 })
            }
            Creature::Bat(bat) => {
                // Still hangs, moving beats its wings, rearing glides, striking beats and bites.
                let hz = pose::bat_beat_hz(bat);
                shown.cycle = (shown.cycle + dt * hz * std::f32::consts::TAU) % std::f32::consts::TAU;
                let (roost, power) = match choice.motion {
                    ViewMotion::Still => (1.0, 0.0),
                    ViewMotion::Moving | ViewMotion::Striking | ViewMotion::Goring | ViewMotion::Gesturing => (0.0, 1.0),
                    ViewMotion::Rearing => (0.0, 0.0),
                };
                let height = 1.5 * bat.length;
                pose::bat(bat, &BatPose { flap: shown.cycle, power, roost, height, bite: strike, time: t, ..default() })
            }
            Creature::Skeleton(skeleton) => {
                let dims = shown.dims.get_or_insert_with(|| skeleton.dims());
                shown.cycle = (shown.cycle + speed * dt / pose::skeleton_stride(dims) * std::f32::consts::TAU) % std::f32::consts::TAU;
                // A blow every 2.4 seconds: wound up and thrown over 1.6, then held at the guard.
                let swing = (choice.motion == ViewMotion::Striking).then(|| (t % 2.4 / 1.6).min(1.0));
                pose::skeleton_armed(dims, &SkeletonPose { stride: shown.cycle, pace: if moving { 1.0 } else { 0.0 }, time: t, swing, look: 0.0, jaw: 0.0 }, &shown.stance)
            }
            Creature::Troll(troll) => {
                let dims = shown.dims.get_or_insert_with(|| troll.dims());
                shown.cycle = (shown.cycle + speed * dt / pose::troll_stride(dims) * std::f32::consts::TAU) % std::f32::consts::TAU;
                // A rake every 2.4 seconds, as a skeleton's blow.
                let swing = (choice.motion == ViewMotion::Striking).then(|| (t % 2.4 / 1.6).min(1.0));
                pose::troll_armed(dims, &SkeletonPose { stride: shown.cycle, pace: if moving { 1.0 } else { 0.0 }, time: t, swing, look: 0.0, jaw: swing.unwrap_or(0.0) }, &shown.stance)
            }
        };
        for (entity, place) in shown.bones.iter().zip(&places) {
            if let Ok(mut transform) = transforms.get_mut(*entity) {
                *transform = transform_of(place);
            }
        }
    }
}

/// A strike's reach over two seconds: thrown fast, held a moment, drawn back slowly.
pub fn strike_curve(t: f32) -> f32 {
    match t {
        t if t < 1.0 => 0.0,
        t if t < 1.1 => (t - 1.0) / 0.1,
        t if t < 1.35 => 1.0,
        t => (1.0 - (t - 1.35) / 0.5).max(0.0),
    }
}

/// Stands the camera back and above the pivot so the turning creature fills [`FILL`] of the
/// view.
fn frame_camera(shown: Query<&Shown>, mut cameras: Query<(&Projection, &mut Transform), With<ViewCamera>>) {
    let Some(shown) = shown.iter().next() else { return };
    for (projection, mut transform) in &mut cameras {
        let Projection::Perspective(perspective) = projection else { continue };
        // The turntable's framing allows for the whole sweep; a creature is lower and longer.
        // A skeleton stands tall rather than long, so it is framed as the turntable frames an item.
        let share = match shown.creature {
            Creature::Skeleton(_) | Creature::Spider(_) | Creature::Troll(_) | Creature::Minotaur(_) => 0.8,
            // Ears and antlers make it tall as well as long.
            Creature::Jackalope(_) => 0.75,
            _ => 0.6,
        };
        let distance = share * framing_distance(shown.size, perspective, FILL);
        let centre = Vec3::new(0.0, shown.size.y * 0.5, 0.0);
        let target = Transform::from_translation(centre + Vec3::new(0.0, ELEVATION.sin(), ELEVATION.cos()) * distance).looking_at(centre, Vec3::Y);
        if *transform != target {
            *transform = target;
        }
    }
}

fn show_status(choice: Res<CreatureChoice>, mut status: Query<&mut Text, With<StatusLine>>) {
    if !choice.is_changed() {
        return;
    }
    let creature = choice.creature();
    let block = choice.species.stat_block();
    let look = match &creature {
        Creature::Rat(r) => format!("{:?}, {:?}, {:.0} cm", r.colouring, r.pattern, r.length * 10.0),
        Creature::Snake(s) => format!("{:?}, {:?}, {:.1} m", s.colouring, s.pattern, s.length / 10.0),
        Creature::Skeleton(s) => skeleton_look(s, choice.gear),
        Creature::Spider(s) => format!("{:?}, {:?}, {:.1} m across", s.colouring, s.marking, s.span / 10.0),
        Creature::Troll(t) => troll_look(t, choice.gear),
        Creature::Minotaur(m) => minotaur_look(m),
        Creature::Jackalope(j) => format!("{:?}, {:.0} cm, antlers {}+{} tines", j.colouring, j.length * 10.0, j.tines[0], j.tines[1]),
        Creature::Centipede(c) => format!("{:?}, {:?}, {} plates, {:.0} cm", c.colouring, c.banding, c.segments, c.length * 10.0),
        Creature::Bat(b) => format!("{:?}, {:?} wings, {:?} ears, {:?} nose, wingspan {:.0} cm", b.colouring, b.membrane, b.ear_shape, b.nose, b.wingspan * 10.0),
    };
    let motion = if choice.species.is_skeleton() || choice.species.is_troll() || choice.species.is_minotaur() { skeleton_motion(choice.motion) } else { format!("{:?}", choice.motion) };
    let line = format!(
        "{}   seed {}   {}   {}\nArmor Class {}   Hit Points {} ({})   {} {}\nTab species   Left/Right seed   M motion   G gear   Space pause",
        block.name,
        choice.seed,
        look,
        motion,
        block.armour_class,
        block.hit_points,
        block.hit_dice,
        if choice.species.is_skeleton() { "Shortsword" } else if choice.species.is_jackalope() { "Antlers" } else if choice.species.is_minotaur() { "Gore" } else { "Bite" },
        block.bite
    );
    let armed = choice.gear && choice.species.is_troll() && fb_items::troll::troll_outfit(choice.seed, fb_items::Odds::kit(1)).item(fb_items::Slot::Mainhand).is_some();
    let line = if choice.species.is_minotaur() {
        format!("{line}\nGreataxe {}   a denizen: it talks by gesture", fb_creatures::minotaur::GREATAXE)
    } else if choice.species.is_troll() {
        let hands = if armed { format!("A claw {} and its weapon's dice + {}", fb_creatures::troll::CLAW, crate::creatures::trolls::STRENGTH_BONUS) } else { format!("Two claws {} each", fb_creatures::troll::CLAW) };
        format!("{line}\n{hands}, Regeneration {} hit points a turn unless burned", fb_creatures::troll::REGAIN)
    } else {
        line
    };
    for mut text in &mut status {
        text.0.clone_from(&line);
    }
}

/// What is special about a skeleton, for the status line: its bone colour, height, cracks and
/// what is missing.
fn skeleton_look(s: &fb_creatures::SkeletonState, gear: bool) -> String {
    let colour = match s.tone {
        t if t < 0.25 => "bleached",
        t if t < 0.65 => "yellowed",
        _ => "dark",
    };
    let mut missing = Vec::new();
    if s.jaw_gone {
        missing.push("jaw".to_string());
    }
    let ribs = s.ribs_gone[0].count_ones() + s.ribs_gone[1].count_ones();
    if ribs > 0 {
        missing.push(format!("{ribs} ribs"));
    }
    let fingers = s.fingers_gone[0].count_ones() + s.fingers_gone[1].count_ones();
    if fingers > 0 {
        missing.push(format!("{fingers} fingers"));
    }
    let missing = if missing.is_empty() { String::new() } else { format!(", missing {}", missing.join(", ")) };
    let outfit = fb_items::undead::skeleton_outfit(s.seed, fb_items::Odds::kit(1));
    let kit = if !gear {
        ", gear off".to_string()
    } else if outfit.worn.is_empty() {
        ", bare-boned".to_string()
    } else {
        format!(", {}", outfit.worn.iter().map(|(_, item)| item.title().to_string()).collect::<Vec<_>>().join(", "))
    };
    format!("{colour}, {:.0} cm, {} cracks{missing}{kit}", s.height() * 10.0, s.cracks)
}

/// What is special about a troll, for the status line: its skin, height and the marks on it.
fn troll_look(t: &fb_creatures::TrollState, gear: bool) -> String {
    let skin = match t.tone {
        v if v < 0.33 => "moss green",
        v if v < 0.66 => "grey green",
        _ => "stone grey",
    };
    let tusks = if t.tusks > 0.0 { "tusked" } else { "tuskless" };
    let weapon = match fb_items::troll::troll_outfit(t.seed, fb_items::Odds::kit(1)).item(fb_items::Slot::Mainhand) {
        _ if !gear => ", weapon off".to_string(),
        Some(weapon) => format!(", {}", weapon.title()),
        None => ", unarmed".to_string(),
    };
    format!("{skin}, {:.0} cm, {} warts, {tusks}{weapon}", t.height() * 10.0, t.warts)
}

/// What is special about a minotaur, for the status line: its coat, height, horns and ring.
fn minotaur_look(m: &fb_creatures::MinotaurState) -> String {
    let horns = match (m.horns.broken, m.horns.length) {
        (Some(_), _) => "a broken horn",
        (None, l) if l < 0.33 => "short horns",
        (None, l) if l < 0.66 => "horns",
        _ => "long horns",
    };
    let ring = if m.ring.is_some() { ", ringed" } else { "" };
    format!("{:?} {:?}, {:.0} cm, {horns}{ring}", m.coat, m.marking, m.height() * 10.0)
}

fn skeleton_motion(motion: ViewMotion) -> String {
    match motion {
        ViewMotion::Still | ViewMotion::Rearing => "standing",
        ViewMotion::Moving => "walking",
        ViewMotion::Striking => "swinging",
        ViewMotion::Goring => "goring",
        ViewMotion::Gesturing => "gesturing",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_the_command_line() {
        let choice = CreatureChoice::from_args(args("--creature --species giant_rat --seed 7 --motion moving")).unwrap();
        assert_eq!((choice.species, choice.seed, choice.motion), (Species::GiantRat, 7, ViewMotion::Moving));
        assert_eq!(CreatureChoice::from_args(args("--species=constrictor")).unwrap().species, Species::ConstrictorSnake);
        assert!(CreatureChoice::from_args(args("--species dragon")).is_err());
        assert!(CreatureChoice::from_args(args("--motion flying")).is_err());
        let skeleton = CreatureChoice::from_args(args("--skeleton --seed 9 --motion swing")).unwrap();
        assert_eq!((skeleton.species, skeleton.seed, skeleton.motion), (Species::Skeleton, 9, ViewMotion::Striking));
    }

    #[test]
    fn a_strike_is_thrown_and_drawn_back() {
        assert_eq!(strike_curve(0.5), 0.0);
        assert_eq!(strike_curve(1.2), 1.0);
        assert_eq!(strike_curve(1.95), 0.0);
    }
}
