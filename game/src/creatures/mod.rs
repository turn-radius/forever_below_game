//! Creatures in the level (`fb_level::CreatureSpot`): rats, snakes, giant spiders, skeletons ([`skeletons`], in
//! their gear) and trolls ([`trolls`]) from `fb_creatures`, drawn
//! in the house style (toon bands, ink and wear) and moved bone by bone without a skeleton.
//!
//! - Each creature is a root on the floor at its spot, facing its yaw, with one entity per bone
//!   of its [`fb_creatures::Rig`] (each on its parent bone, as the rig says) carrying that bone's
//!   Armory parts, spawned by the [`ItemSpawner`] so every part has its own toon material, ink
//!   hull and wear. Skin and fur wear with a little grime, as a folk's skin does.
//! - [`animate`] turns how the root has moved and what the creature means to do ([`Intent`])
//!   into a pose (`fb_creatures::pose`) each frame and copies it onto the bones: a rat trots
//!   with its body bobbing, legs stepping and tail swinging, sniffs about when still and lunges
//!   to bite; a snake slithers with a wave travelling down its spine, coils when it rests,
//!   rears, strikes, gapes and flicks its tongue; a giant centipede scuttles on a wave of
//!   stepping legs down its body, feels about with its antennae, rears and bites with
//!   its forcipules. Snake segments follow the floor's relief, and
//!   a laid-out snake's body follows the way it came ([`Trail`]) rather than swinging round
//!   as a whole when it turns.
//!   A giant spider walks on all eight legs in an alternating tetrapod, each foot planted on the
//!   floor, rears up on its back legs with the front pair held high, and stabs with its fangs.
//! - Standing about, a creature idles ([`idle`]): rats sniff and sway their tails, snakes lie
//!   coiled, now and then raising their heads and tasting the air.
//! - Each bone carries the capsule it presents to a blow ([`HitShape`], from
//!   `fb_creatures::Bone::hit`), so a blow finds a rat's head or tail and every segment of a
//!   laid-out snake as they are posed, not one circle at the root.
//! - Only each bone's main surface casts a shadow (`fb_creatures::rig::SURFACE_PARTS`), and a creature beyond
//!   [`POSE_RANGE`] of the player holds its pose, so a level's creatures cost the frame little.

use armory::cond::{Condition, ConditionState};
use bevy::{app::Propagate, light::NotShadowCaster, prelude::*};
use fb_body::Placement;
use fb_creatures::pose::{self, BatPose, CentipedePose, JackalopePose, RatPose, SkeletonPose, SnakePose, SpiderPose};
use fb_creatures::Creature;
use fb_creatures::rig::{Capsule, SURFACE_PARTS};
use fb_level::{CreatureSpot, Level};
use procgen_rng::Mulberry32;

use crate::combat::{Health, feedback::Corpse};
use crate::level::geometry::Relief;
use crate::play::{CurrentLevel, DelveSystems, Player};
use crate::record::WorldRecord;
use crate::render::{ItemAppearance, ItemSpawner};
use crate::tuning::Tuning;

pub mod behaviour;
pub mod skeletons;
pub mod trolls;

use std::sync::Arc;
use crate::belongings::Belongings;
use skeletons::{Rattle, SkeletonKit, WornGear};

pub struct CreaturesPlugin;

impl Plugin for CreaturesPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::belongings::BelongingsPlugin>() {
            app.add_plugins(crate::belongings::BelongingsPlugin);
        }
        app.add_plugins((behaviour::BehaviourPlugin, skeletons::SkeletonsPlugin, trolls::TrollsPlugin))
            .init_resource::<Tuning>()
            .add_systems(Update, (spawn_creatures.run_if(resource_exists_and_changed::<CurrentLevel>), size_creatures, idle, animate).chain().in_set(DelveSystems));
    }
}

/// A creature in the level: which spot it came from and what it is.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component, from_reflect = false)]
#[require(DespawnOnExit<crate::launch::AppState> = DespawnOnExit(crate::launch::AppState::Delving))]
pub struct Critter {
    #[reflect(ignore)]
    pub spot: CreatureSpot,
    #[reflect(ignore)]
    pub creature: Creature,
    /// The circle it covers on the floor, world units, at the size it is drawn.
    pub radius: f32,
    /// How much larger than rolled it is drawn: its species' tuned size ([`size_creatures`]).
    /// The creature itself stays as its seed made it.
    pub size: f32,
    /// A skeleton's measures, weapon, armour and gear, worked out once when it appears, so
    /// nothing rebuilds its body each frame. `None` for the other creatures.
    #[reflect(ignore)]
    pub kit: Option<Arc<SkeletonKit>>,
    /// What a troll holds, if it holds anything ([`trolls::TrollArms`]). `None` for the rest.
    #[reflect(ignore)]
    pub arms: Option<Arc<trolls::TrollArms>>,
}

impl Critter {
    /// `creature`, from `spot`, at the size it was rolled. A skeleton has nothing worn or held
    /// until given a kit ([`Critter::with_kit`]).
    pub fn new(spot: CreatureSpot, creature: Creature) -> Critter {
        let kit = match &creature {
            Creature::Skeleton(skeleton) => Some(Arc::new(SkeletonKit::bare(skeleton))),
            _ => None,
        };
        let radius = kit.as_ref().map_or_else(|| creature.radius(), |k| k.radius);
        Critter { spot, radius, creature, size: 1.0, kit, arms: None }
    }

    /// This skeleton in its `kit` of gear.
    pub fn with_kit(mut self, kit: SkeletonKit) -> Critter {
        self.radius = kit.radius * self.size;
        self.kit = Some(Arc::new(kit));
        self
    }

    /// This troll holding `arms`.
    pub fn with_arms(mut self, arms: trolls::TrollArms) -> Critter {
        self.arms = Some(Arc::new(arms));
        self
    }

    /// Its Armor Class: a skeleton's from what it wears ([`SkeletonKit`]), any other creature's
    /// as its SRD 5.1 stat block prints it ([`fb_creatures::StatBlock`]: rat 10, p. 387; giant
    /// rat 12, p. 378; poisonous snake 13, p. 386; constrictor snake 12, p. 369).
    pub fn armour_class(&self) -> i32 {
        self.armour_class_with(fb_character::rules::defaults())
    }

    /// [`Critter::armour_class`] with the stat block as `rules` have it (`data/rules.toml`).
    pub fn armour_class_with(&self, rules: &fb_character::rules::Rules) -> i32 {
        self.kit.as_ref().map_or_else(|| rules.creature(self.spot.species).armour_class, |k| k.armour_class)
    }

    /// What its bite or blow adds to the d20: the stat block's attack bonus, and the `ToHit` of a
    /// skeleton's weapon.
    pub fn to_hit(&self) -> fb_character::combat::ToHit {
        self.to_hit_with(fb_character::rules::defaults())
    }

    /// [`Critter::to_hit`] with the attack bonus as `rules` have it.
    pub fn to_hit_with(&self, rules: &fb_character::rules::Rules) -> fb_character::combat::ToHit {
        let weapon = self.kit.as_ref().map_or(0, |k| k.to_hit);
        fb_character::combat::ToHit::StatBlock { attack_bonus: rules.creature(self.spot.species).attack_bonus, weapon }
    }

    /// Draws it `size` times as large as rolled, covering that much more floor.
    pub fn resize(&mut self, size: f32) {
        self.size = size;
        self.radius = self.kit.as_ref().map_or_else(|| self.creature.radius(), |k| k.radius) * size;
    }
}

/// Draws each creature at its species' tuned size (`creatures.size` in [`Tuning`]): its root
/// scaled about its feet, so every bone, part and hit shape grows with it, and the floor it
/// covers grown to match. New creatures take it as they appear, and all of them when the
/// tuning changes.
pub fn size_creatures(tuning: Res<Tuning>, mut creatures: Query<(&mut Critter, &mut Transform)>) {
    for (mut critter, mut transform) in &mut creatures {
        let size = tuning.creatures.size.of(critter.spot.species);
        if critter.size != size || transform.scale != Vec3::splat(size) {
            critter.resize(size);
            transform.scale = Vec3::splat(size);
        }
    }
}

/// On a bone: the capsule it presents to a blow, in its own space ([`fb_creatures::Bone::hit`]).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct HitShape(pub Capsule);

impl HitShape {
    /// Spheres that cover the capsule placed by `at` (a bone's world transform): as many along
    /// it as its length holds radii, so neighbours overlap. Each is its centre and radius in
    /// world units. Our own stand-in for a capsule test.
    pub fn spheres(&self, at: &GlobalTransform) -> impl Iterator<Item = (Vec3, f32)> + use<> {
        let (a, b) = (at.transform_point(Vec3::from_array(self.0.a)), at.transform_point(Vec3::from_array(self.0.b)));
        let radius = self.0.radius * at.scale().max_element();
        let count = ((a.distance(b) / radius.max(1e-3)).ceil() as usize).clamp(1, 32);
        (0..=count).map(move |i| (a.lerp(b, i as f32 / count as f32), radius))
    }
}

/// The bone entities of a creature, in its rig's order.
#[derive(Component, Clone, Debug)]
pub struct Bones(pub Vec<Entity>);

/// What a creature means to do this moment, which the animation eases toward. Each is 0 to 1
/// except `look`, a head turn in radians toward its left.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Intent {
    /// A snake coiled up (1) or laid out to move (0).
    pub coil: f32,
    /// A snake's front raised.
    pub rear: f32,
    /// A snake's strike thrown, a rat's bite lunged.
    pub strike: f32,
    /// A snake's jaw open.
    pub jaw: f32,
    /// A snake's tongue out.
    pub tongue: f32,
    pub look: f32,
    /// A skeleton's swing, how far through it (0 to 1, as `fb_creatures::pose::SkeletonPose`
    /// takes it), used as it is, not eased.
    pub swing: Option<f32>,
    /// A skeleton's collapse, how far through it (0 to 1), used as it is.
    pub fallen: f32,
    /// A bat's wings and body in the air.
    pub wings: Wings,
}

/// How a bat holds itself in the air: what [`Intent`] asks of the pose (`BatPose`). Everything
/// is eased from frame to frame except the altitude, which is where the bat is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct Wings {
    /// The height of the body's middle over the floor under the creature's root, world units.
    pub altitude: f32,
    /// 0 flying, 1 hanging head down by its feet.
    pub roost: f32,
    /// 0 gliding on spread wings, 1 beating.
    pub power: f32,
    /// How far the nose is raised, radians.
    pub pitch: f32,
    /// How far it banks toward its left, radians; pi turned over.
    pub bank: f32,
    /// How folded the wings are, 0 spread to 1 folded.
    pub tuck: f32,
}

/// A bat's flight: where it is in the air and what it is doing there, on the bat beside its
/// [`behaviour::Brain`]. The root stays on the floor; the bat's height over it is
/// [`Flight::altitude`], which the pose raises its body by.
#[derive(Component, Clone, Debug)]
pub struct Flight {
    /// The height of the body's middle over the floor under the root, world units.
    pub altitude: f32,
    /// How fast it is climbing, world units a second (negative diving).
    pub climb: f32,
    /// What it is doing.
    pub phase: Wing,
    /// Seconds left in a wheel, a pull away or a swoop.
    pub timer: f32,
    /// Where it is on its circle about the delver, radians.
    pub angle: f32,
    /// Which way it circles: 1 or -1.
    pub spin: f32,
    /// How far it turns a second, radians, signed toward its left (for banking).
    pub turning: f32,
    /// How far its pose reaches below the body's middle in flight, and above it in flight and
    /// roosting, world units (`BatState::flight_extent`, `roost_extent`).
    pub below: f32,
    pub above: f32,
    pub roost_above: f32,
    /// The height it dropped from, once it has fallen.
    pub fell_from: Option<f32>,
    /// The height of its body's middle lying fallen on the floor, folded and turned over.
    pub lie: f32,
}

/// What a bat is doing in the air.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wing {
    /// Hanging from the ceiling at its spot.
    Roosting,
    /// Flying back to its spot to hang there again.
    Returning,
    /// Wheeling round the delver between head height and the ceiling.
    Wheeling,
    /// Diving at the delver to bite.
    Swooping,
    /// Pulling up and away after a bite.
    Away,
}

impl Flight {
    /// A bat of `rig` hanging from the ceiling `ceiling` over its root (world units): its
    /// reaches flying and roosting, and how it lies when it falls, measured from its rig.
    pub fn roosting(bat: &fb_creatures::BatState, rig: &fb_creatures::Rig, ceiling: f32, seed: u32) -> Flight {
        let (below, above) = bat.flight_extent(rig);
        let roost_above = bat.roost_extent(rig).1;
        // Lying on its back with its wings folded: how far its lowest point is under its middle.
        let lying = pose::bat(bat, &BatPose { bank: std::f32::consts::PI, tuck: 1.0, ..default() });
        let lowest = rig.world(&lying).iter().zip(&rig.bones).flat_map(|(place, bone)| bone.mesh.parts.iter().flat_map(move |part| part.positions.iter().map(move |&p| place.point(p)[1]))).fold(f32::MAX, f32::min);
        Flight {
            altitude: ceiling - roost_above,
            climb: 0.0,
            phase: Wing::Roosting,
            timer: 0.0,
            angle: (seed % 628) as f32 * 0.01,
            spin: if seed & 1 == 0 { 1.0 } else { -1.0 },
            turning: 0.0,
            below: -below,
            above,
            roost_above,
            fell_from: None,
            lie: -lowest,
        }
    }

    /// The wings and body it holds this moment, beating its wings as hard as `power`.
    pub fn wings(&self, power: f32) -> Wings {
        let roosting = self.phase == Wing::Roosting;
        let pitch = if roosting { 0.0 } else { (self.climb * 0.08).clamp(-0.7, 0.5) };
        Wings { altitude: self.altitude, roost: if roosting { 1.0 } else { 0.0 }, power, pitch, bank: (self.turning * 0.12).clamp(-0.6, 0.6), tuck: if roosting { 1.0 } else { 0.0 } }
    }
}

/// The pose's moving parts, eased from frame to frame.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct Motion {
    /// The rat's step cycle or the snake's wave, radians.
    pub cycle: f32,
    /// Speed as a share of the creature's walking speed, smoothed.
    pub pace: f32,
    /// Where the root stood last frame.
    pub last: Option<Vec2>,
    /// The intent, eased.
    pub now: Intent,
    /// Seconds since it was spawned, for idle motions.
    pub time: f32,
}

/// Where a snake's head has been: its root's recent places on the floor (world `(x, z)`),
/// newest first, a point every [`TRAIL_STEP`] world units, as far back as its body is long.
/// Laid out, a snake's neck is at its root (`fb_creatures::pose::snake_spine`) and its body is
/// laid along this trail ([`trail_bend`]), so a long snake turning a corner keeps to the floor
/// its head crossed instead of sweeping its tail through the wall. Our own approach: the
/// "follow the leader" of snake games, on a [`SnakePose::bend`].
#[derive(Component, Clone, Debug, Default)]
pub struct Trail(pub std::collections::VecDeque<Vec2>);

/// How far apart the points of a [`Trail`] are, world units.
pub const TRAIL_STEP: f32 = 1.0;

impl Trail {
    /// Records the root standing at `at`, keeping `length` world units of the way behind it.
    pub fn record(&mut self, at: Vec2, length: f32) {
        match self.0.front() {
            Some(&last) if last.distance(at) < TRAIL_STEP => {}
            _ => self.0.push_front(at),
        }
        let keep = (length / TRAIL_STEP).ceil() as usize + 2;
        self.0.truncate(keep);
    }

    /// The way the trail runs forward (toward the root) at `back` world units behind the
    /// root standing at `at`, if it reaches that far.
    pub fn direction(&self, at: Vec2, back: f32) -> Option<Vec2> {
        let mut ahead = at;
        let mut walked = 0.0;
        for &point in &self.0 {
            let piece = ahead.distance(point);
            if piece < 1e-4 {
                continue;
            }
            walked += piece;
            if walked >= back {
                return Some((ahead - point) / piece);
            }
            ahead = point;
        }
        None
    }
}

/// The [`SnakePose::bend`] that lays a snake's body along its trail: each knot takes the
/// trail's heading that far behind the head, turned into the root's frame, or the last heading
/// the trail has where it runs out (straight on, with no trail yet).
pub fn trail_bend(snake: &fb_creatures::SnakeState, trail: &Trail, root: &Transform) -> [f32; pose::BEND_KNOTS] {
    // As long as it is drawn.
    let body = snake.dims().body_length * root.scale.x;
    let at = root.translation.xz();
    let mut bend = [0.0; pose::BEND_KNOTS];
    let mut last = 0.0;
    for (k, b) in bend.iter_mut().enumerate() {
        let back = k as f32 / (pose::BEND_KNOTS - 1) as f32 * body;
        if let Some(way) = trail.direction(at, back.max(TRAIL_STEP)) {
            // Into the root's frame, where the body runs toward +Z; a heading toward +X is a
            // turn to its left.
            let local = root.rotation.inverse() * Vec3::new(way.x, 0.0, way.y);
            last = local.x.atan2(local.z);
        }
        *b = last;
    }
    bend
}

/// How a creature idles: its own rates, from its seed.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component)]
pub struct Idle {
    pub phase: f32,
    pub rate: f32,
}

/// How quickly each part of the intent is reached, per second ([`StableInterpolate`]). Our own
/// tuning: strikes and bites snap, coiling and rearing take a moment.
const EASE_FAST: f32 = 18.0;
const EASE_SLOW: f32 = 2.5;
/// How quickly the measured pace settles, per second.
const PACE_EASE: f32 = 8.0;
/// Grime on a creature's skin and fur (as a folk's dirt, 0 to 1). Our own.
const GRIME: (f32, f32) = (0.15, 0.4);

/// The world transform of a creature at `spot`: on the drawn floor, facing its yaw. Creatures
/// face +Z and the spot's yaw is a camera's (0 faces -Z), as for folk.
pub fn stand_at(level: &Level, spot: &CreatureSpot) -> Transform {
    let [x, _, z] = Level::world(spot.x, spot.y);
    let y = Relief::new(level).floor_height(x, z);
    Transform::from_xyz(x, y, z).with_rotation(Quat::from_rotation_y(spot.yaw + std::f32::consts::PI))
}

/// A rig placement as a transform.
pub fn transform_of(place: &Placement) -> Transform {
    let [x, y, z] = place.rotation.cols;
    let rotation = Quat::from_mat3(&Mat3::from_cols(Vec3::from_array(x), Vec3::from_array(y), Vec3::from_array(z))).normalize();
    Transform { translation: Vec3::from_array(place.origin), rotation, scale: Vec3::ONE }
}

/// Spawns every creature of the level once, and again only if the spots change.
fn spawn_creatures(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    old: Query<Entity, With<Critter>>,
    record: Option<Res<WorldRecord>>,
    mut staged: Option<ResMut<crate::staged::Staged>>,
    mut spawner: ItemSpawner,
    mut shown: Local<Option<Vec<CreatureSpot>>>,
) {
    // Again only if the spots change, or every creature was cleared away (a new delve).
    if shown.as_deref() == Some(&level.0.creatures[..]) && !old.is_empty() {
        return;
    }
    for entity in &old {
        commands.entity(entity).despawn();
    }
    debug!("spawning creatures: {:?}", level.0.creatures.iter().map(|c| c.species).collect::<Vec<_>>());
    // On a level that was left, each creature is as it was left (`crate::record`).
    let saved = record.as_deref().and_then(|r| r.level(level.0.depth));
    // A skeleton takes some hundred milliseconds to roll and fit, so the level's are built side by
    // side before any is spawned, or ahead of time (`crate::staged`).
    let built = staged.as_mut().and_then(|s| s.take_creatures(&level.0)).unwrap_or_else(|| build_creatures(&level.0.creatures, level.0.depth));
    for (index, (spot, built)) in level.0.creatures.iter().zip(built).enumerate() {
        let left = saved.and_then(|l| l.creatures.get(&index));
        // A body from the record lies bare: what it wore was set down where it fell, and is on
        // the floor (or gone) as the record has it.
        let root = spawn_built(&mut commands, &mut spawner, &level.0, spot, built, left.is_some_and(|l| l.dead));
        if let Some(left) = left {
            let at = left.transform();
            commands.entity(root).insert((Health { hp: left.hp, max: left.max }, at));
            if left.dead {
                // Long fallen: lying as it lies once it has collapsed.
                commands.entity(root).insert((Corpse { since: LONG_FALLEN, fell: at }, skeletons::Looted));
            }
        }
    }
    let dev = dev_spots(&level.0);
    for (spot, built) in dev.iter().zip(build_creatures(&dev, level.0.depth)) {
        spawn_built(&mut commands, &mut spawner, &level.0, spot, built, false);
    }
    *shown = Some(level.0.creatures.clone());
}

/// Creatures `FB_SPAWN` asks for, for captures and for trying a fight: `skeleton`, `troll`, `jackalope`
/// or `centipede`, or `skeleton:<seed>[,<seed>...]` (`skeleton` alone is seed 1207), each [`DEV_AHEAD`] cells
/// ahead of the level's start and spread across the way, facing the start. None when unset.
pub fn dev_spots(level: &Level) -> Vec<CreatureSpot> {
    let Some(spec) = std::env::var("FB_SPAWN").ok().filter(|s| !s.is_empty()) else { return Vec::new() };
    dev_spots_for(level, &spec)
}

/// How far ahead of the start `FB_SPAWN` stands a creature, cells.
const DEV_AHEAD: f32 = 2.5;

/// [`dev_spots`] for the value `spec`.
pub fn dev_spots_for(level: &Level, spec: &str) -> Vec<CreatureSpot> {
    let (name, seeds) = spec.split_once(':').unwrap_or((spec, ""));
    if name.trim() == "folk" {
        // Folk are stood by [`dev_folk_for`].
        return Vec::new();
    }
    let species = match name.trim() {
        "skeleton" => fb_creatures::Species::Skeleton,
        "troll" => fb_creatures::Species::Troll,
        "bat" => fb_creatures::Species::Bat,
        "giant_bat" => fb_creatures::Species::GiantBat,
        "jackalope" => fb_creatures::Species::Jackalope,
        "centipede" => fb_creatures::Species::GiantCentipede,
        other => {
            warn!("FB_SPAWN: no creature called {other:?} (try skeleton, troll, bat, giant_bat, jackalope or centipede)");
            return Vec::new();
        }
    };
    let seeds: Vec<u32> = seeds.split(',').filter_map(|n| n.trim().parse().ok()).collect();
    let seeds = if seeds.is_empty() { vec![1207] } else { seeds };
    let (sx, sy, facing) = level.start();
    let yaw = facing.yaw();
    info!("FB_SPAWN: the start is cell ({sx}, {sy}) facing {:.0} degrees", yaw.to_degrees());
    // The way the start faces, in cells (yaw 0 faces -y, positive turns toward -x), and across.
    let ahead = Vec2::new(-yaw.sin(), -yaw.cos());
    let across = Vec2::new(-ahead.y, ahead.x);
    let count = seeds.len() as f32;
    seeds
        .into_iter()
        .enumerate()
        .map(|(i, seed)| {
            let at = Vec2::new(sx as f32 + 0.5, sy as f32 + 0.5) + ahead * DEV_AHEAD + across * (i as f32 - 0.5 * (count - 1.0)) * 1.2;
            CreatureSpot { x: at.x, y: at.y, yaw: yaw + std::f32::consts::PI, species, seed, pack: 0 }
        })
        .collect()
}

/// A trade `FB_SPAWN` names after a folk: `weaponsmith`, `master-alchemist`, `guard` (a tier
/// then a dash, apprentice if left out; the profession's label without spaces or its first word).
type TradeWord = (fb_faction::Tier, fb_faction::Profession);

fn parse_trade_word(word: &str) -> Option<TradeWord> {
    use fb_faction::{Profession, Tier};
    let word = word.to_lowercase();
    let (tier, name) = match word.split_once('-') {
        Some((t, n)) => (Tier::ALL.into_iter().find(|x| x.label() == t)?, n),
        None => (Tier::Apprentice, word.as_str()),
    };
    let profession = if name == "guard" {
        Profession::Guard
    } else {
        Profession::TRADES.into_iter().find(|p| p.label().replace(' ', "") == name || p.label().split(' ').next() == Some(name))?
    };
    Some((tier, profession))
}

/// How far ahead a lone folk stands, cells: within the delver's reach (a cell), so the card shows.
const FOLK_AHEAD: f32 = 1.0;

/// Folk `FB_SPAWN` asks for: `folk` (the level's first drawn faction), `folk:<faction index>`, or
/// several such, `folk:3,folk:7`, each [`DEV_AHEAD`] cells ahead of the start and spread across
/// the way, facing the start. A faction index is a faction id of the level's drawn factions if
/// it holds one, otherwise a place in that list (wrapping). A trade may follow, `folk:3:weaponsmith`
/// or `folk:master-alchemist` or `folk:guard` (see [`parse_trade_word`]). None when unset or not about folk.
pub fn dev_folk(level: &Level) -> Vec<fb_level::FolkSpot> {
    let Some(spec) = std::env::var("FB_SPAWN").ok().filter(|s| !s.is_empty()) else { return Vec::new() };
    dev_folk_for(level, &spec)
}

/// [`dev_folk`] for the value `spec`. A denizen faction's spot made here is stood like any other,
/// unless its body is not built yet ([`crate::folk::has_figure`]).
pub fn dev_folk_for(level: &Level, spec: &str) -> Vec<fb_level::FolkSpot> {
    if level.factions.is_empty() {
        return Vec::new();
    }
    let picks: Vec<(Option<u32>, Option<TradeWord>)> = spec
        .split(',')
        .filter_map(|part| {
            let rest = part.trim().strip_prefix("folk")?;
            let mut pieces = rest.split(':').map(str::trim).filter(|p| !p.is_empty());
            let (mut pick, mut trade) = (None, None);
            for piece in pieces.by_ref() {
                match piece.parse() {
                    Ok(n) => pick = Some(n),
                    Err(_) => trade = parse_trade_word(piece),
                }
            }
            Some((pick, trade))
        })
        .collect();
    let (sx, sy, facing) = level.start();
    let yaw = facing.yaw();
    let ahead = Vec2::new(-yaw.sin(), -yaw.cos());
    let across = Vec2::new(-ahead.y, ahead.x);
    let count = picks.len() as f32;
    picks
        .into_iter()
        .enumerate()
        .map(|(i, (pick, word))| {
            let spec = match pick {
                Some(n) => level.factions.iter().find(|f| f.id.0 as u32 == n).unwrap_or(&level.factions[n as usize % level.factions.len()]),
                None => &level.factions[0],
            };
            // The way out of the start is a corridor a cell wide: several stand in a staggered file.
            let sway = if i % 2 == 0 { -0.2 } else { 0.2 } * (count - 1.0).min(1.0);
            let at = Vec2::new(sx as f32 + 0.5, sy as f32 + 0.5) + ahead * (FOLK_AHEAD + 0.9 * i as f32) + across * sway;
            let mut rng = procgen_rng::Mulberry32::new(1207 + i as u32);
            let (folk, denizen) = match spec.member_body(&mut rng) {
                Some(body) => (fb_body::Folk::Human, Some(body)),
                None => (spec.member_folk(&mut rng), None),
            };
            let trade = word.map(|(tier, profession)| fb_level::Trade {
                profession,
                tier,
                style: match profession {
                    fb_faction::Profession::Armorer => Some(fb_faction::Style::of_lean(spec.armour)),
                    fb_faction::Profession::Weaponsmith => Some(fb_faction::Style::Blades),
                    _ => None,
                },
            });
            fb_level::FolkSpot { x: at.x, y: at.y, yaw: yaw + std::f32::consts::PI, folk, denizen, seed: 1207 + i as u32, sex: spec.member_sex(1207 + i as u32), faction: spec.id, trade }
        })
        .collect()
}

/// Seconds a restored body has lain, long past its collapse.
const LONG_FALLEN: f32 = 60.0;

/// Spawns one creature: a root at its spot and a tree of bones carrying its parts. A skeleton
/// is drawn in the gear its seed rolls, each piece on the bone it rides ([`WornGear`]).
pub fn spawn_creature(commands: &mut Commands, spawner: &mut ItemSpawner, level: &Level, spot: &CreatureSpot) -> Entity {
    spawn_creature_as(commands, spawner, level, spot, false)
}

/// [`spawn_creature`], a skeleton `fallen` drawn as bare bones: its gear is not put on it.
pub fn spawn_creature_as(commands: &mut Commands, spawner: &mut ItemSpawner, level: &Level, spot: &CreatureSpot, fallen: bool) -> Entity {
    spawn_built(commands, spawner, level, spot, build_creature(spot, level.depth), fallen)
}

/// A creature worked out from its spot, ready to spawn: pure data, Bevy-free.
#[derive(Debug)]
pub struct Built {
    pub creature: Creature,
    /// A skeleton's fitted gear and the kit worked out from it.
    pub gear: Option<fb_creatures::SkeletonGear>,
    pub kit: Option<SkeletonKit>,
    /// What a troll holds, when it holds a weapon.
    pub arms: Option<trolls::TrollArms>,
}

/// Rolls the creature at `spot` on a level `depth` down, and fits a skeleton's gear (rolled
/// against [`fb_items::Odds::kit`] for the depth) and measures its kit: the slow part of
/// spawning one.
pub fn build_creature(spot: &CreatureSpot, depth: u32) -> Built {
    let creature = spot.creature();
    let gear = match &creature {
        Creature::Skeleton(skeleton) => Some(fb_creatures::SkeletonGear::roll(skeleton, fb_items::Odds::kit(depth))),
        Creature::Troll(troll) => Some(fb_creatures::SkeletonGear::roll_troll(troll, fb_items::Odds::kit(depth))),
        _ => None,
    };
    let kit = match (&creature, &gear) {
        (Creature::Skeleton(skeleton), Some(gear)) => Some(SkeletonKit::armed(skeleton, gear)),
        _ => None,
    };
    let arms = match (&creature, &gear) {
        (Creature::Troll(_), Some(gear)) => trolls::TrollArms::of(gear),
        _ => None,
    };
    Built { creature, gear, kit, arms }
}

/// [`build_creature`] for every spot, on as many threads as there are cores.
pub fn build_creatures(spots: &[CreatureSpot], depth: u32) -> Vec<Built> {
    if spots.len() < 2 {
        return spots.iter().map(|spot| build_creature(spot, depth)).collect();
    }
    std::thread::scope(|scope| {
        let jobs: Vec<_> = spots.iter().map(|spot| scope.spawn(move || build_creature(spot, depth))).collect();
        jobs.into_iter().map(|job| job.join().expect("a creature builds")).collect()
    })
}

/// Spawns `built`, the creature of `spot` ([`spawn_creature_as`]).
pub fn spawn_built(commands: &mut Commands, spawner: &mut ItemSpawner, level: &Level, spot: &CreatureSpot, built: Built, fallen: bool) -> Entity {
    let Built { creature, gear, kit, arms } = built;
    let rig = gear.as_ref().map_or_else(|| creature.rig(), |g| g.rig.clone());
    let (root, bones) = spawn_creature_with(commands, level, spot, &rig, kit, arms.filter(|_| !fallen));
    let appearance = ItemAppearance {
        condition: Condition { state: ConditionState::Custom, wear: Mulberry32::new(spot.seed ^ GRIME_SALT).range_f32(GRIME.0, GRIME.1), ..Condition::default() },
        seed: spot.seed,
        ..default()
    };
    for (bone, &entity) in rig.bones.iter().zip(&bones) {
        // Only a bone's main surface casts a shadow: eyes, whiskers, claws, teeth, markings
        // and bellies inside its shadow add nothing to it but draw into every nearby light's
        // shadow maps (a snake has three parts to a segment).
        let (casts, details): (Vec<_>, Vec<_>) = bone.mesh.parts.iter().cloned().partition(|p| SURFACE_PARTS.contains(&p.name));
        for (parts, shadow) in [(casts, true), (details, false)] {
            if parts.is_empty() {
                continue;
            }
            let mut item = spawner.spawn(&armory::ItemMesh { parts }, &appearance);
            item.insert(ChildOf(entity));
            if !shadow {
                item.insert(Propagate(NotShadowCaster));
            }
        }
    }
    if let Some(gear) = gear.as_ref().filter(|_| !fallen) {
        for piece in &gear.pieces {
            spawner.spawn(&piece.mesh, &crate::carry::appearance_of(&piece.item)).insert((Name::new(piece.name.clone()), WornGear, ChildOf(bones[piece.bone])));
        }
    }
    root
}

/// How near the player a creature must be for its pose to change, world units (20 m): well
/// past a torch's reach and short of the fog's end (`fog_end`, 26 m as shipped). Our own.
const POSE_RANGE: f32 = 200.0;

/// Salt for a creature's grime. Our own.
const GRIME_SALT: u32 = 0x6121_3E00;

/// Spawns a creature's root at its spot and one entity per bone of `rig` on its parent, with
/// no parts on them: everything but the drawing. Returns the root and the bones.
pub fn spawn_skeleton(commands: &mut Commands, level: &Level, spot: &CreatureSpot, rig: &fb_creatures::Rig) -> (Entity, Vec<Entity>) {
    spawn_skeleton_with(commands, level, spot, rig, None)
}

/// [`spawn_skeleton`] for a skeleton in `kit` (a skeleton given none has nothing worn or held).
pub fn spawn_skeleton_with(commands: &mut Commands, level: &Level, spot: &CreatureSpot, rig: &fb_creatures::Rig, kit: Option<SkeletonKit>) -> (Entity, Vec<Entity>) {
    spawn_creature_with(commands, level, spot, rig, kit, None)
}

/// [`spawn_skeleton_with`] for a troll holding `arms`.
pub fn spawn_creature_with(commands: &mut Commands, level: &Level, spot: &CreatureSpot, rig: &fb_creatures::Rig, kit: Option<SkeletonKit>, arms: Option<trolls::TrollArms>) -> (Entity, Vec<Entity>) {
    let creature = spot.creature();
    let mut critter = Critter::new(*spot, creature.clone());
    if let Some(kit) = kit {
        critter = critter.with_kit(kit);
    }
    if let Some(arms) = arms {
        critter = critter.with_arms(arms);
    }
    let belongings = Belongings::of(&critter, level.depth);
    let mut rng = Mulberry32::new(spot.seed ^ 0x1D1E);
    let snake = spot.species.is_snake();
    let mut rest = Intent { coil: if snake { 1.0 } else { 0.0 }, rear: if snake { 0.45 } else { 0.0 }, ..default() };
    // A bat starts hanging from the ceiling over its spot.
    let flight = if let Creature::Bat(bat) = &creature {
        let [x, _, z] = Level::world(spot.x, spot.y);
        let relief = Relief::new(level);
        let vaults = (level.style == fb_level::LevelStyle::StoneHalls).then(|| crate::level::geometry::stone::Vaults::new(level));
        let ceiling = behaviour::ceiling_over(level, vaults.as_ref(), &relief, Vec2::new(x, z));
        let flight = Flight::roosting(bat, rig, ceiling, spot.seed);
        rest.wings = flight.wings(0.0);
        Some(flight)
    } else {
        None
    };
    let root = commands
        .spawn((
            Name::new(spot.species.stat_block().name),
            critter,
            stand_at(level, spot),
            Visibility::default(),
            rest,
            Motion { now: rest, ..default() },
            Idle { phase: rng.range_f32(0.0, std::f32::consts::TAU), rate: rng.range_f32(0.8, 1.25) },
        ))
        .id();
    let mut bones: Vec<Entity> = Vec::with_capacity(rig.bones.len());
    for bone in &rig.bones {
        let parent = bone.parent.map_or(root, |p| bones[p]);
        let mut entity = commands.spawn((Name::new(bone.name.clone()), transform_of(&bone.rest), Visibility::default(), ChildOf(parent)));
        if let Some(hit) = bone.hit() {
            entity.insert(HitShape(hit));
        }
        bones.push(entity.id());
    }
    commands.entity(root).insert(Bones(bones.clone()));
    if let Some(belongings) = belongings {
        commands.entity(root).insert(belongings);
    }
    if let Some(flight) = flight {
        commands.entity(root).insert(flight);
    }
    if snake {
        commands.entity(root).insert(Trail::default());
    }
    if spot.species.is_skeleton() {
        commands.entity(root).insert(Rattle::new(spot.seed));
    }
    if let Creature::Troll(troll) = &creature {
        commands.entity(root).insert(trolls::Troll::new(troll));
    }
    (root, bones)
}

/// Idling: what a creature does standing still, when nothing else drives it. Snakes lie
/// coiled and now and then raise their heads; every few seconds they taste the air with a
/// flick or two of the tongue. Rats' sniffing and a centipede's feeling about with its
/// antennae are in their poses.
pub fn idle(mut creatures: Query<(&Critter, &Idle, &Motion, &mut Intent), Without<Driven>>) {
    for (critter, idle, motion, mut intent) in &mut creatures {
        let t = motion.time * idle.rate + idle.phase;
        // A centipede feels about with its antennae (in its pose), turns its head now and then and
        // lifts its front a little.
        if critter.spot.species.is_centipede() {
            intent.look = 0.3 * (t * 0.31).sin();
            intent.rear = 0.12 * (0.5 + 0.5 * (t * 0.23).sin());
            continue;
        }
        // Only a snake idles like this: rats sniff in their pose, and a skeleton stands.
        if !critter.spot.species.is_snake() {
            continue;
        }
        intent.coil = 1.0;
        intent.rear = 0.3 + 0.35 * (0.5 + 0.5 * (t * 0.35).sin());
        intent.tongue = tongue_flick(t);
        intent.look = 0.35 * (t * 0.23).sin();
    }
}

/// On a creature whose intent something else sets (its behaviour), so [`idle`] leaves it be.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct Driven;

/// A snake's tongue at `t` seconds: two quick flicks every three seconds or so, in between.
pub fn tongue_flick(t: f32) -> f32 {
    let cycle = (t / 3.1).fract() * 3.1;
    if cycle < 0.5 { (cycle / 0.25 * std::f32::consts::PI).sin().abs() } else { 0.0 }
}

/// What [`animate`] reads and writes on each creature.
type Posed = (&'static Critter, &'static Transform, &'static Intent, &'static mut Motion, &'static Bones, Option<&'static mut Trail>, Has<crate::combat::feedback::Corpse>, Option<&'static trolls::Troll>);

/// Poses every creature for this frame from how it moved and what it means to do.
pub fn animate(
    time: Res<Time>,
    level: Res<CurrentLevel>,
    mut creatures: Query<Posed>,
    mut transforms: Query<&mut Transform, Without<Critter>>,
    players: Query<&GlobalTransform, With<Player>>,
) {
    let dt = time.delta_secs();
    let relief = Relief::new(&level.0);
    let player = players.iter().next().map(GlobalTransform::translation);
    for (critter, root, intent, mut motion, bones, mut trail, dead, troll_body) in &mut creatures {
        let at = root.translation.xz();
        if let (Some(trail), Creature::Snake(snake)) = (trail.as_deref_mut(), &critter.creature) {
            trail.record(at, snake.dims().body_length * root.scale.x);
        }
        let moved = motion.last.map_or(0.0, |last| at.distance(last));
        // How far it moved for its size: a creature drawn twice as large takes strides twice
        // as long.
        let strode = moved / root.scale.x.max(1e-3);
        motion.last = Some(at);
        // A body lies still: its idle motions stop where they were.
        if !dead {
            motion.time += dt;
        }
        // Out of sight across the level, the pose holds: changing every bone's transform
        // re-propagates and re-extracts each of its parts, a few hundred entities a frame.
        if player.is_some_and(|p| p.xz().distance(at) > POSE_RANGE) {
            continue;
        }
        if dt > 0.0 {
            let pace = (moved / dt) / critter.spot.species.speed();
            motion.pace.smooth_nudge(&pace.min(2.5), PACE_EASE, dt);
            let now = &mut motion.now;
            now.coil.smooth_nudge(&intent.coil, EASE_SLOW, dt);
            now.rear.smooth_nudge(&intent.rear, EASE_SLOW, dt);
            now.strike.smooth_nudge(&intent.strike, EASE_FAST, dt);
            now.jaw.smooth_nudge(&intent.jaw, EASE_FAST, dt);
            now.tongue.smooth_nudge(&intent.tongue, EASE_FAST, dt);
            now.look.smooth_nudge(&intent.look, 4.0, dt);
            // A bat's height is where it is; the rest of how it holds itself is eased.
            now.wings.altitude = intent.wings.altitude;
            now.wings.roost.smooth_nudge(&intent.wings.roost, 4.0, dt);
            now.wings.tuck.smooth_nudge(&intent.wings.tuck, 6.0, dt);
            now.wings.power.smooth_nudge(&intent.wings.power, 5.0, dt);
            now.wings.pitch.smooth_nudge(&intent.wings.pitch, 5.0, dt);
            now.wings.bank.smooth_nudge(&intent.wings.bank, 4.0, dt);
        }
        let now = motion.now;
        let places = match &critter.creature {
            Creature::Rat(rat) => {
                motion.cycle = (motion.cycle + strode / pose::rat_stride(rat) * std::f32::consts::TAU) % std::f32::consts::TAU;
                pose::rat(rat, &RatPose { stride: motion.cycle, pace: motion.pace, time: motion.time, bite: now.strike, look: now.look })
            }
            Creature::Jackalope(jackalope) => {
                motion.cycle = (motion.cycle + strode / pose::jackalope_stride(jackalope) * std::f32::consts::TAU) % std::f32::consts::TAU;
                // Rearing sits it up; a strike gores.
                pose::jackalope(jackalope, &JackalopePose { stride: motion.cycle, pace: motion.pace, time: motion.time, sit: now.rear, gore: now.strike, look: now.look })
            }
            Creature::Snake(snake) => {
                motion.cycle += strode / (pose::WAVELENGTH * snake.dims().body_length) * std::f32::consts::TAU;
                let bend = trail.as_deref().map_or([0.0; pose::BEND_KNOTS], |trail| trail_bend(snake, trail, root));
                let mut places = pose::snake(
                    snake,
                    &SnakePose { phase: motion.cycle, pace: motion.pace.min(1.0), coil: now.coil, rear: now.rear, strike: now.strike, jaw: now.jaw, tongue: now.tongue, look: now.look, bend },
                );
                // Each segment, and the head, rests on the floor under it rather than the floor
                // at the root.
                use fb_creatures::snake::bone::{HEAD, SPINE};
                for i in std::iter::once(HEAD).chain(SPINE..places.len()) {
                    let place = &mut places[i];
                    let world = root.transform_point(Vec3::from_array(place.origin));
                    place.origin[1] += (relief.floor_height(world.x, world.z) - root.translation.y) / root.scale.y.max(1e-3);
                }
                places
            }
            Creature::Skeleton(skeleton) => {
                // Measured once, when it appeared.
                let measured;
                let (dims, stance) = match &critter.kit {
                    Some(kit) => (&kit.dims, kit.stance),
                    None => {
                        measured = skeleton.dims();
                        (&measured, [None, None])
                    }
                };
                if intent.fallen > 0.0 {
                    pose::skeleton_heap(dims, skeleton.seed, intent.fallen)
                } else {
                    motion.cycle = (motion.cycle + strode / pose::skeleton_stride(dims) * std::f32::consts::TAU) % std::f32::consts::TAU;
                    pose::skeleton_armed(dims, &SkeletonPose { stride: motion.cycle, pace: motion.pace.min(1.0), time: motion.time, look: now.look, jaw: now.jaw, swing: intent.swing }, &stance)
                }
            }
            Creature::Spider(spider) => {
                motion.cycle = (motion.cycle + strode / pose::spider_stride(spider) * std::f32::consts::TAU) % std::f32::consts::TAU;
                // A fallen spider's legs draw in under it as it is turned over ([`Intent::fallen`]).
                pose::spider(spider, &SpiderPose { stride: motion.cycle, pace: motion.pace.min(2.0), time: motion.time, rear: now.rear, bite: now.strike, curl: intent.fallen })
            }
            Creature::Troll(troll) => {
                // Measured once, when it appeared.
                let measured;
                let dims = match troll_body {
                    Some(body) => &body.dims,
                    None => {
                        measured = troll.dims();
                        &measured
                    }
                };
                if intent.fallen > 0.0 {
                    pose::troll_fallen(dims, intent.fallen)
                } else {
                    motion.cycle = (motion.cycle + strode / pose::troll_stride(dims) * std::f32::consts::TAU) % std::f32::consts::TAU;
                    let stance = critter.arms.as_ref().map_or([None, None], |a| a.stance);
                    pose::troll_armed(dims, &SkeletonPose { stride: motion.cycle, pace: motion.pace.min(1.0), time: motion.time, look: now.look, jaw: now.jaw, swing: intent.swing }, &stance)
                }
            }
            Creature::Centipede(centipede) => {
                // The legs step once a stride, and the body's wave goes with them.
                motion.cycle = (motion.cycle + strode / pose::centipede_stride(centipede) * std::f32::consts::TAU) % std::f32::consts::TAU;
                let mut places = pose::centipede(
                    centipede,
                    &CentipedePose { stride: motion.cycle, pace: motion.pace, time: motion.time, rear: now.rear, bite: now.strike, jaw: now.jaw, look: now.look, curl: intent.fallen },
                );
                // A felled one curls up ([`Intent::fallen`]). Each plate, and the head, rests on the floor under it; the legs hang from the
                // plates and go with them.
                for i in std::iter::once(fb_creatures::centipede::bone::HEAD).chain((0..centipede.segments).map(|k| centipede.segment_bone(k))) {
                    let place = &mut places[i];
                    let world = root.transform_point(Vec3::from_array(place.origin));
                    place.origin[1] += (relief.floor_height(world.x, world.z) - root.translation.y) / root.scale.y.max(1e-3);
                }
                places
            }
            // Roosting, wheeling, gliding or fallen, at the height its flight puts it.
            Creature::Bat(bat) => {
                let flap = (motion.time * pose::bat_beat_hz(bat) * std::f32::consts::TAU) % std::f32::consts::TAU;
                let w = now.wings;
                pose::bat(bat, &BatPose { flap, power: w.power, roost: w.roost, tuck: w.tuck, height: w.altitude, pitch: w.pitch, bank: w.bank, bite: now.strike, look: now.look, time: motion.time })
            }
        };
        for (entity, place) in bones.0.iter().zip(&places) {
            if let Ok(mut transform) = transforms.get_mut(*entity) {
                *transform = transform_of(place);
            }
        }
    }
}

/// The first seed whose first level holds each of `species`, for a test that needs them
/// placed on a real level, so it does not hang on what one seed holds.
#[cfg(test)]
pub fn seed_holding(species: &[fb_creatures::Species]) -> u32 {
    (1..=500).find(|&s| {
        let level = fb_level::generate(s, 1);
        species.iter().all(|&k| level.creatures.iter().any(|c| c.species == k))
    }).expect("some first level holds them all")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placements_become_transforms() {
        let place = Placement { rotation: fb_creatures::shape::spine_to_creature(), origin: [1.0, 2.0, 3.0] };
        let t = transform_of(&place);
        assert_eq!(t.translation, Vec3::new(1.0, 2.0, 3.0));
        assert!((t.rotation * Vec3::Y).abs_diff_eq(Vec3::Z, 1e-5), "the spine frame's forward is the creature's");
        assert!((t.rotation * Vec3::Z).abs_diff_eq(Vec3::Y, 1e-5));
    }

    #[test]
    fn the_tongue_flicks_now_and_then() {
        let samples: Vec<f32> = (0..310).map(|i| tongue_flick(i as f32 * 0.01)).collect();
        assert!(samples.iter().any(|&t| t > 0.9));
        assert!(samples.iter().filter(|&&t| t == 0.0).count() > 200, "mostly in");
    }

    /// An app with every creature of seed 1207 spawned as bones (no drawing), idling and
    /// animating, stepped 1/60 s a frame.
    fn level_app() -> (App, Vec<Entity>) {
        level_app_of(1207, 1)
    }

    fn level_app_of(seed: u32, depth: u32) -> (App, Vec<Entity>) {
        use bevy::time::TimeUpdateStrategy;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .add_systems(Update, (idle, animate).chain());
        let level = fb_level::generate(seed, depth);
        app.insert_resource(CurrentLevel(level.clone()));
        let roots = {
            let mut commands = app.world_mut().commands();
            level.creatures.iter().map(|spot| spawn_skeleton(&mut commands, &level, spot, &spot.creature().rig()).0).collect()
        };
        app.world_mut().flush();
        (app, roots)
    }

    fn bone_transforms(app: &App, root: Entity) -> Vec<Transform> {
        let bones = &app.world().get::<Bones>(root).unwrap().0;
        bones.iter().map(|&b| *app.world().get::<Transform>(b).unwrap()).collect()
    }

    /// Each newer species spawns and animates without a panic on a level that holds it: the first
    /// seed in 1..=60 at a depth where it is placed (deep enough for the troll and the giant bats).
    #[test]
    fn each_newer_species_spawns_and_animates_on_a_level_that_holds_it() {
        use fb_creatures::Species::*;
        for (species, depth) in [(GiantCentipede, 2), (Bat, 1), (Jackalope, 1), (GiantSpider, 3), (GiantBat, 4), (Troll, 7)] {
            let seed = (1..=60).find(|&s| fb_level::generate(s, depth).creatures.iter().any(|c| c.species == species)).unwrap_or_else(|| panic!("no level holds {species:?} at depth {depth}"));
            let (mut app, roots) = level_app_of(seed, depth);
            assert!(roots.iter().any(|&r| app.world().get::<Critter>(r).unwrap().spot.species == species));
            for _ in 0..30 {
                app.update();
            }
        }
    }

    #[test]
    fn every_creature_on_1207_animates() {
        let (mut app, roots) = level_app();
        // Two since #662, which gave every wild group a room of its own, taking it from the snake and rats where the level is short; the newer species are covered by the test above.
        assert!(roots.len() >= 2);
        for _ in 0..10 {
            app.update();
        }
        let before: Vec<Vec<Transform>> = roots.iter().map(|&r| bone_transforms(&app, r)).collect();
        for _ in 0..90 {
            app.update();
        }
        for (i, &root) in roots.iter().enumerate() {
            let after = bone_transforms(&app, root);
            let moved = before[i].iter().zip(&after).any(|(a, b)| !a.translation.abs_diff_eq(b.translation, 1e-4) || !a.rotation.abs_diff_eq(b.rotation, 1e-4));
            assert!(moved, "{:?} stands frozen", app.world().get::<Critter>(root).unwrap().spot.species);
        }
    }

    #[test]
    fn creatures_on_1207_neither_float_nor_sink_standing_or_moving() {
        let (mut app, roots) = level_app();
        let level = fb_level::generate(1207, 1);
        let relief = Relief::new(&level);
        let rigs: Vec<fb_creatures::Rig> = roots.iter().map(|&r| app.world().get::<Critter>(r).unwrap().creature.rig()).collect();
        for frame in 0..150 {
            // Standing for a second, then everyone moves off forward at its walking speed.
            if frame >= 60 {
                for &root in &roots {
                    let speed = app.world().get::<Critter>(root).unwrap().spot.species.speed();
                    let mut t = app.world_mut().get_mut::<Transform>(root).unwrap();
                    // Creatures face +Z.
                    let ahead = t.rotation * Vec3::Z;
                    t.translation += ahead * speed / 60.0;
                    t.translation.y = relief.floor_height(t.translation.x, t.translation.z);
                }
            }
            app.update();
            if frame % 15 != 14 {
                continue;
            }
            for (&root, rig) in roots.iter().zip(&rigs) {
                let critter = app.world().get::<Critter>(root).unwrap().clone();
                // What flies is meant to be off the floor.
                if critter.spot.species.is_flying() {
                    continue;
                }
                let bones = app.world().get::<Bones>(root).unwrap().0.clone();
                let size = match &critter.creature {
                    Creature::Rat(r) => r.length,
                    Creature::Snake(s) => s.girth * 4.0,
                    Creature::Skeleton(s) => s.height(),
                    Creature::Spider(s) => s.span,
                    Creature::Troll(t) => t.height(),
                    Creature::Jackalope(j) => j.length,
                    Creature::Centipede(c) => c.girth * 4.0,
                    Creature::Bat(b) => b.length,
                };
                let (mut deepest, mut nearest) = (f32::MIN, f32::MAX);
                for (bone, &entity) in rig.bones.iter().zip(&bones) {
                    let at = *app.world().get::<GlobalTransform>(entity).unwrap();
                    for part in &bone.mesh.parts {
                        for p in part.positions.iter().step_by(3) {
                            let w = at.transform_point(Vec3::from_array(*p));
                            let gap = w.y - relief.floor_height(w.x, w.z);
                            deepest = deepest.max(-gap);
                            nearest = nearest.min(gap);
                        }
                    }
                }
                // The floor's relief moves under a long body a little: within 4% of its size.
                let tolerance = 0.04 * size + 0.03;
                assert!(deepest < tolerance, "{:?} sinks {deepest} at frame {frame}", critter.spot.species);
                assert!(nearest < tolerance, "{:?} floats {nearest} at frame {frame}", critter.spot.species);
            }
        }
    }

    #[test]
    fn a_skeleton_stands_idle_where_a_snake_would_coil_and_rear() {
        use bevy::time::TimeUpdateStrategy;
        let level = fb_level::generate(1207, 1);
        let skeleton = dev_spots_for(&level, "skeleton:5").remove(0);
        assert_eq!(skeleton.species, fb_creatures::Species::Skeleton);
        let snakes = fb_level::generate(seed_holding(&[fb_creatures::Species::PoisonousSnake]), 1);
        let snake = *snakes.creatures.iter().find(|c| c.species.is_snake()).unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .add_systems(Update, idle);
        let (skeleton, snake) = {
            let mut commands = app.world_mut().commands();
            (spawn_skeleton(&mut commands, &level, &skeleton, &skeleton.creature().rig()).0, spawn_skeleton(&mut commands, &snakes, &snake, &snake.creature().rig()).0)
        };
        app.world_mut().flush();
        for _ in 0..90 {
            app.update();
        }
        let intent = |e| *app.world().get::<Intent>(e).unwrap();
        assert_eq!(intent(skeleton), Intent::default(), "a skeleton neither coils, rears nor flicks a tongue");
        assert!(intent(snake).coil > 0.9 && intent(snake).tongue >= 0.0 && intent(snake).rear > 0.2, "while a snake does");
    }

    #[test]
    fn a_skeletons_bones_are_measured_once_and_its_dev_spot_asks_by_name() {
        let level = fb_level::generate(1207, 1);
        assert!(dev_spots_for(&level, "unicorn").is_empty());
        assert!(dev_spots_for(&level, "folk:3").is_empty(), "folk are not creatures");
        let folk = dev_folk_for(&level, "folk:3,folk");
        assert_eq!(folk.len(), 2);
        assert!(level.factions.iter().any(|f| f.id == folk[0].faction));
        assert_eq!(folk[1].faction, level.factions[0].id);
        assert!(dev_folk_for(&level, "skeleton").is_empty());
        let trades: Vec<_> = dev_folk_for(&level, "folk:3:weaponsmith,folk::master-alchemist,folk:guard,folk").iter().map(|f| f.trade.map(|t| t.label())).collect();
        assert_eq!(trades, [Some("Apprentice weaponsmith (blades)".to_string()), Some("Master alchemist".to_string()), Some("Guard".to_string()), None]);
        let spots = dev_spots_for(&level, "skeleton:1,2,3");
        assert_eq!(spots.iter().map(|s| s.seed).collect::<Vec<_>>(), vec![1, 2, 3]);
        let spot = spots[0];
        let creature = spot.creature();
        let critter = Critter::new(spot, creature);
        let kit = critter.kit.as_ref().unwrap();
        let Creature::Skeleton(state) = &critter.creature else { unreachable!() };
        assert_eq!(kit.height, state.height());
        // The kit stays put as the creature is resized: nothing is measured again.
        let mut big = critter.clone();
        big.resize(2.0);
        assert!(std::sync::Arc::ptr_eq(big.kit.as_ref().unwrap(), critter.kit.as_ref().unwrap()));
        assert_eq!(big.radius, 2.0 * critter.radius);
    }

    #[test]
    fn creatures_stand_on_the_drawn_floor() {
        let level = fb_level::generate(1207, 1);
        let relief = Relief::new(&level);
        for spot in &level.creatures {
            let t = stand_at(&level, spot);
            assert!((t.translation.y - relief.floor_height(t.translation.x, t.translation.z)).abs() < 1e-5);
        }
    }
}
