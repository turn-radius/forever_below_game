//! Folk met in the level: each [`FolkSpot`] drawn as a dressed `fb_body` figure, standing on
//! the floor at its spot, lit by the level's lights.
//!
//! - [`Person::of`] says who stands at a spot: their body ([`BodySpec::roll`]), a name from
//!   `data/syl.json`, and an outfit from their faction's leaning ([`fb_level::FACTIONS`])
//!   rolled through `fb_items` ([`fb_items::outfit::Outfit::roll`], which also gives a bare-headed gnome a pointed hat as the plate does). Everyone holds something: a weapon, a light or
//!   both.
//! - [`Figure::of`] fits it all to the body: held gear first ([`fit_held`], each hand posing
//!   its arm, [`Body::posed`]), then what is worn ([`fit_worn`]). Its meshes split into the
//!   body and the head (the head, hair, and a helm or hat), which turns about the neck.
//! - Each item is drawn with its own look (palette, condition, etching) by the
//!   [`ItemSpawner`], in the house style: toon bands, ink and decay. Skin carries the person's
//!   dirt. A lit torch or lantern in hand lights the room round them.
//!
//! - Standing, they breathe and now and then shift their weight ([`Idle`]): the whole figure
//!   moves a little about its feet, since there is no skeleton. A player within
//!   [`LOOK_RANGE`] draws their head round ([`look_at`]), turning about the neck; under a hood,
//!   whose cape lies on the shoulders, the body turns a little instead.
//! - Each stands in a circle on the floor that the player cannot walk into ([`Obstacles`]).
//!
//! Figures are built once per spot, in parallel, and kept in [`FolkFigures`], so the same
//! level shows the same people without building them again.

use std::sync::Arc;

use armory::{ItemMesh, build::light::LightAnchor, cond::Condition};
use bevy::{app::Propagate, light::NotShadowCaster, platform::collections::HashMap, prelude::*, tasks::ComputeTaskPool};
use fb_body::{Body, BodySpec, HeldFit, Side, WornFit, fit_held, fit_worn};
use fb_items::{Item, Slot, roll_item};
use fb_level::{FolkSpot, Level};
use procgen_rng::Mulberry32;

use crate::carry::{appearance_of, item_light, world_light};
use crate::level::EXPOSURE;
use crate::play::{CurrentLevel, Obstacles, PlayerCamera};
use crate::render::{ItemAppearance, ItemSpawner};

pub struct FolkPlugin;

impl Plugin for FolkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FolkFigures>()
            .init_resource::<Obstacles>()
            .add_systems(Update, (spawn_folk.run_if(resource_exists_and_changed::<CurrentLevel>), idle_folk, turn_heads, block_the_way));
    }
}

/// Salts for a person's name and dirt, drawn apart from their body and outfit. Our own.
const NAME_SALT: u32 = 0x4E41_4D45;
const DIRT_SALT: u32 = 0xD127_0001;

/// How far above a held flame its light hangs, in world units (10 cm).
const HELD_LIGHT_LIFT: f32 = 1.0;

/// Who stands at a spot.
#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    pub spot: FolkSpot,
    pub name: String,
    pub spec: BodySpec,
    /// The key of the outfit they dress from (`data/outfits.json`).
    pub outfit: &'static str,
    /// Everything worn and held, by slot.
    pub gear: Vec<(Slot, Item)>,
    /// How dirty their skin is, 0 clean to about 0.85 (the plate's `C.dirt`).
    pub dirt: f32,
}

impl Person {
    /// The person at `spot`: a pure function of it.
    pub fn of(spot: &FolkSpot) -> Person {
        let faction = spot.faction.row();
        let mut rng = Mulberry32::new(spot.seed ^ NAME_SALT);
        let name = fb_items::naming::folk_name(&mut rng, spot.folk.key());
        let outfit = *rng.pick(faction.outfits);
        let mut gear = fb_items::outfit::outfit(outfit).map_or_else(Vec::new, |o| o.roll(spot.seed, spot.folk.key()).worn);
        let has = |gear: &[(Slot, Item)], slot| gear.iter().any(|(s, _)| *s == slot);
        // Nobody stands about empty-handed: a torch if the outfit gave nothing to hold.
        if !has(&gear, Slot::Mainhand) && !has(&gear, Slot::Offhand) {
            gear.extend(roll_item("Torch", spot.seed.wrapping_add(2)).map(|torch| (Slot::Mainhand, torch)));
        }
        // The plate's `rollChar`: scavengers are filthy, the freshly fallen grubby, the rest
        // a little dirty.
        let mut dirt_rng = Mulberry32::new(spot.seed ^ DIRT_SALT);
        let dirt = match outfit {
            "scavenger" => dirt_rng.range_f32(0.55, 0.85),
            "fallen" => dirt_rng.range_f32(0.3, 0.5),
            _ => dirt_rng.range_f32(0.08, 0.38),
        };
        Person { spot: *spot, name, spec: BodySpec::roll(spot.folk, spot.seed), outfit, gear, dirt }
    }

    /// What they wear, innermost slots first as listed.
    pub fn worn(&self) -> impl Iterator<Item = &(Slot, Item)> {
        self.gear.iter().filter(|(slot, _)| !matches!(slot, Slot::Mainhand | Slot::Offhand))
    }

    /// What they hold.
    pub fn held(&self) -> impl Iterator<Item = &(Slot, Item)> {
        self.gear.iter().filter(|(slot, _)| matches!(slot, Slot::Mainhand | Slot::Offhand))
    }

    pub fn item(&self, slot: Slot) -> Option<&Item> {
        self.gear.iter().find(|(s, _)| *s == slot).map(|(_, item)| item)
    }
}

/// The hand each holding slot is held in.
pub fn hand_of(slot: Slot) -> Option<Side> {
    match slot {
        Slot::Mainhand => Some(Side::Right),
        Slot::Offhand => Some(Side::Left),
        _ => None,
    }
}

/// One piece of a figure: a mesh in body space and how it looks.
#[derive(Clone, Debug)]
pub struct Piece {
    pub name: String,
    pub mesh: ItemMesh,
    pub appearance: ItemAppearance,
    /// Whether it casts a shadow. A lit torch or lantern does not, or its own flame and body
    /// would black out its light (as for lit things on the floor, `crate::carry`).
    pub shadow: bool,
}

/// A light a figure carries: where its heart is in body space, and what light it gives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeldLight {
    pub at: Vec3,
    pub anchor: LightAnchor,
}

/// A person fitted and built, in body space (soles on y = 0, face +Z).
#[derive(Clone, Debug)]
pub struct Figure {
    pub body: Body,
    /// What each hand holds, as fitted.
    pub held: Vec<(Slot, HeldFit)>,
    pub worn: WornFit,
    /// Skin, held gear and everything worn below the head.
    pub torso: Vec<Piece>,
    /// What turns with the head: head, hair, and a helm or hat. Empty under a hood, whose cape
    /// lies on the shoulders, so the whole figure turns a little instead.
    pub head: Vec<Piece>,
    /// Where the head turns, in body space: the base of the neck.
    pub neck: Vec3,
    /// The lights its hands hold.
    pub lights: Vec<HeldLight>,
    /// The circle on the floor it stands in, which the player cannot walk into.
    pub radius: f32,
}

impl Figure {
    /// Fits `person`'s gear to their body and builds every mesh.
    pub fn of(person: &Person) -> Figure {
        let spec = &person.spec;
        let mut body = Body::new(spec);
        let mut held = Vec::new();
        for (slot, item) in person.held() {
            let Some(side) = hand_of(*slot) else { continue };
            if let Some(fit) = fit_held(&body, side, &item.shape) {
                body = Body::posed(spec, fit.pose);
                held.push((*slot, fit));
            }
        }
        let worn_items: Vec<(Slot, armory::Item)> = person.worn().map(|(slot, item)| (*slot, item.shape.clone())).collect();
        let worn = fit_worn(&body, &worn_items);

        let flesh = ItemAppearance {
            seed: spec.seed,
            condition: Condition { wear: person.dirt, ..Condition::default() },
            ..default()
        };
        let mut skin = body.skin_mesh();
        skin.parts.retain(|p| !worn.covered.hides(p.name));
        let mut torso = vec![Piece { name: "skin".into(), mesh: skin, appearance: flesh, shadow: true }];
        let hooded = matches!(person.item(Slot::Head).map(|i| &i.shape), Some(armory::Item::Hood(_)));
        let mut head = vec![
            Piece { name: "head".into(), mesh: body.head_mesh(), appearance: flesh, shadow: true },
            Piece { name: "hair".into(), mesh: body.hair_mesh(worn.hair), appearance: flesh, shadow: true },
        ];
        for layer in &worn.layers {
            let Some(item) = person.item(layer.slot) else { continue };
            let piece = Piece { name: item.title().to_string(), mesh: layer.mesh.clone(), appearance: appearance_of(&item.shape), shadow: true };
            if layer.slot == Slot::Head && !hooded { head.push(piece) } else { torso.push(piece) }
        }
        let mut lights = Vec::new();
        for (slot, fit) in &held {
            let name = person.item(*slot).map_or_else(String::new, |i| i.title().to_string());
            let light = item_light(&fit.item);
            lights.extend(light.map(|anchor| HeldLight { at: Vec3::from_array(fit.to_body(anchor.position)), anchor }));
            torso.push(Piece { name, mesh: fit.mesh.clone(), appearance: appearance_of(&fit.item), shadow: light.is_none() });
        }
        if hooded {
            torso.append(&mut head);
        }
        // A bald head has no hair to draw.
        torso.retain(|p| !p.mesh.parts.is_empty());
        head.retain(|p| !p.mesh.parts.is_empty());
        let neck = Vec3::from_array(body.neck_base().origin);
        // Shoulders and a layer or two of clothes. Our own allowance.
        let radius = body.shoulder_half_width * 1.2 + 0.3;
        Figure { body, held, worn, torso, head, neck, lights, radius }
    }

    /// The figure's bounds in body space, every piece included.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let points = self.torso.iter().chain(&self.head).flat_map(|p| &p.mesh.parts).flat_map(|part| &part.positions).map(|&p| Vec3::from_array(p));
        points.fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(p), hi.max(p)))
    }
}

/// Figures already built, by spot seed, so a level drawn again reuses them.
#[derive(Resource, Default)]
pub struct FolkFigures(HashMap<u32, Arc<(Person, Figure)>>);

impl FolkFigures {
    /// The person and figure for every spot in `spots`, building (in parallel) those not yet
    /// built.
    pub fn for_spots(&mut self, spots: &[FolkSpot]) -> Vec<Arc<(Person, Figure)>> {
        let missing: Vec<FolkSpot> = spots.iter().filter(|s| !self.0.get(&s.seed).is_some_and(|built| built.0.spot == **s)).copied().collect();
        let built = ComputeTaskPool::get_or_init(Default::default).scope(|scope| {
            for spot in &missing {
                scope.spawn(async move {
                    let person = Person::of(spot);
                    let figure = Figure::of(&person);
                    (person, figure)
                });
            }
        });
        for pair in built {
            self.0.insert(pair.0.spot.seed, Arc::new(pair));
        }
        spots.iter().filter_map(|s| self.0.get(&s.seed).cloned()).collect()
    }
}

/// A person standing in the level: the root of their figure, on the floor at their spot.
#[derive(Component, Clone, Debug)]
pub struct Denizen {
    pub person: Person,
    /// The circle on the floor they stand in ([`Figure::radius`]).
    pub radius: f32,
}

/// The figure under its [`Denizen`] root, moving as someone standing does ([`Idle`]).
#[derive(Component, Clone, Copy, Debug)]
pub struct Stance {
    pub idle: Idle,
    /// How far the whole body has turned toward someone near, yaw radians: only when the head
    /// cannot turn on its own (under a hood).
    pub turn: f32,
}

/// The part of a figure that turns with the head, pivoting at the neck: where it looks now as
/// `(yaw, pitch)` radians, the yaw turning the face (+Z) toward +X, the pitch up.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FolkHead {
    pub look: Vec2,
}

/// Breaths per second at rest: 15 a minute, inside the 12 to 18 the Cleveland Clinic gives for
/// a resting adult (<https://my.clevelandclinic.org/health/articles/10881-vital-signs>), each
/// person a little faster or slower.
const BREATHS_PER_SECOND: f32 = 0.25;
/// How far a breath lifts and fills the figure, as a share of its height and girth. Our own
/// tuning: enough to see close up, never enough to read as bobbing.
const BREATH_LIFT: f32 = 0.006;
const BREATH_FILL: f32 = 0.004;
/// Weight shifts from foot to foot: about one every ten seconds, leaning up to this many
/// radians over the standing foot and moving the hips this far across. Our own tuning.
const SHIFTS_PER_SECOND: f32 = 0.05;
const SHIFT_LEAN: f32 = 0.012;
const SHIFT_SWAY: f32 = 0.18;

/// How someone stands still: breathing, and now and then shifting their weight. Each person's
/// rates and phases are their own, from their seed, so a group never moves in step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Idle {
    pub breath_rate: f32,
    pub breath_phase: f32,
    pub shift_rate: f32,
    pub shift_phase: f32,
}

/// Salt for a person's idle stream. Our own.
const IDLE_SALT: u32 = 0x1D1E_0B0D;

impl Idle {
    pub fn of(seed: u32) -> Idle {
        let mut rng = Mulberry32::new(seed ^ IDLE_SALT);
        Idle {
            breath_rate: BREATHS_PER_SECOND * rng.range_f32(0.85, 1.15),
            breath_phase: rng.range_f32(0.0, std::f32::consts::TAU),
            shift_rate: SHIFTS_PER_SECOND * rng.range_f32(0.7, 1.3),
            shift_phase: rng.range_f32(0.0, std::f32::consts::TAU),
        }
    }

    /// The figure's transform at `t` seconds, about its feet: a breath lifts and fills it, and
    /// a weight shift leans it over one foot and moves it across. The shift dwells on each foot
    /// (a sine through a soft clamp) rather than swaying evenly.
    pub fn at(&self, t: f32) -> Transform {
        let tau = std::f32::consts::TAU;
        let breath = (tau * self.breath_rate * t + self.breath_phase).sin() * 0.5 + 0.5;
        let shift = ((tau * self.shift_rate * t + self.shift_phase).sin() * 2.0).clamp(-1.0, 1.0);
        Transform {
            translation: Vec3::new(shift * SHIFT_SWAY, 0.0, 0.0),
            rotation: Quat::from_rotation_z(-shift * SHIFT_LEAN),
            scale: Vec3::new(1.0 + breath * BREATH_FILL, 1.0 + breath * BREATH_LIFT, 1.0 + breath * BREATH_FILL),
        }
    }
}

/// How near the player must come before folk turn to look, in world units (4 m).
pub const LOOK_RANGE: f32 = 40.0;
/// How far a head turns to either side, and tilts up or down, radians (70 and 25 degrees).
/// Our own limits, short of what a neck can do.
const HEAD_YAW_MAX: f32 = 70.0 * std::f32::consts::PI / 180.0;
const HEAD_PITCH_MAX: f32 = 25.0 * std::f32::consts::PI / 180.0;
/// Past this angle off the way they face, someone does not turn to look at all.
const NOTICE_ANGLE: f32 = 110.0 * std::f32::consts::PI / 180.0;
/// How quickly a head turns to its target, per second ([`StableInterpolate`]).
const HEAD_TURN_RATE: f32 = 4.0;
/// Under a hood the whole body turns instead, this share of the way, at most this far.
const BODY_TURN_SHARE: f32 = 0.35;
const BODY_TURN_MAX: f32 = 25.0 * std::f32::consts::PI / 180.0;

/// Where a head at `head` (in its figure's root space, the face +Z) looks to see an eye at
/// `eye` (the same space): `(yaw, pitch)` radians, yaw turning toward +X and pitch up, held to
/// the neck's limits. `None` if the eye is out of [`LOOK_RANGE`] or behind them.
pub fn look_at(head: Vec3, eye: Vec3) -> Option<Vec2> {
    let to = eye - head;
    let across = Vec2::new(to.x, to.z).length();
    if !(1e-3..=LOOK_RANGE).contains(&across) {
        return None;
    }
    let yaw = to.x.atan2(to.z);
    if yaw.abs() > NOTICE_ANGLE {
        return None;
    }
    let pitch = to.y.atan2(across);
    Some(Vec2::new(yaw.clamp(-HEAD_YAW_MAX, HEAD_YAW_MAX), pitch.clamp(-HEAD_PITCH_MAX, HEAD_PITCH_MAX)))
}

/// The head's rotation about the neck for a look of `(yaw, pitch)`: turned, then tipped (a
/// rotation about +X tips the face down, so the pitch goes in negated).
pub fn head_rotation(look: Vec2) -> Quat {
    Quat::from_euler(EulerRot::YXZ, look.x, -look.y, 0.0)
}

/// The world transform of someone standing at `spot`: on the floor, facing the spot's yaw.
/// The body faces +Z and [`FolkSpot::yaw`] is a camera's yaw (0 faces -Z), so it turns a half
/// turn more.
pub fn stand_at(spot: &FolkSpot) -> Transform {
    let at = Vec3::from_array(Level::world(spot.x, spot.y));
    Transform::from_translation(at).with_rotation(Quat::from_rotation_y(spot.yaw + std::f32::consts::PI))
}

/// Spawns every folk spot of the level once, and again only if the spots change.
fn spawn_folk(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    mut figures: ResMut<FolkFigures>,
    old: Query<Entity, With<Denizen>>,
    mut spawner: ItemSpawner,
    mut shown: Local<Option<Vec<FolkSpot>>>,
) {
    if shown.as_deref() == Some(&level.0.folk[..]) {
        return;
    }
    for entity in &old {
        commands.entity(entity).despawn();
    }
    for built in figures.for_spots(&level.0.folk) {
        let (person, figure) = &*built;
        spawn_figure(&mut commands, &mut spawner, person, figure);
    }
    *shown = Some(level.0.folk.clone());
}

/// Spawns one figure: a root at the spot, a [`Stance`] under it carrying the body's pieces,
/// and the head's pieces under a pivot at the neck.
pub fn spawn_figure(commands: &mut Commands, spawner: &mut ItemSpawner, person: &Person, figure: &Figure) -> Entity {
    let root = commands
        .spawn((
            Name::new(person.name.clone()),
            Denizen { person: person.clone(), radius: figure.radius },
            stand_at(&person.spot),
            Visibility::default(),
        ))
        .id();
    let stance = commands.spawn((Stance { idle: Idle::of(person.spot.seed), turn: 0.0 }, Transform::default(), Visibility::default(), ChildOf(root))).id();
    for piece in &figure.torso {
        let mut entity = spawner.spawn(&piece.mesh, &piece.appearance);
        entity.insert((Name::new(piece.name.clone()), ChildOf(stance)));
        if !piece.shadow {
            entity.insert(Propagate(NotShadowCaster));
        }
    }
    if !figure.head.is_empty() {
        let head = commands.spawn((FolkHead::default(), Transform::from_translation(figure.neck), Visibility::default(), ChildOf(stance))).id();
        for piece in &figure.head {
            spawner.spawn(&piece.mesh, &piece.appearance).insert((Name::new(piece.name.clone()), Transform::from_translation(-figure.neck), ChildOf(head)));
        }
    }
    for light in &figure.lights {
        // A little above the flame, so the hand and the stone under it do not burn white.
        commands.spawn((world_light(&light.anchor, &EXPOSURE), Transform::from_translation(light.at + Vec3::Y * HELD_LIGHT_LIFT), ChildOf(stance)));
    }
    root
}

/// Breathes and shifts every figure's weight ([`Idle::at`]), turned toward the player as far
/// as [`Stance::turn`] says.
fn idle_folk(time: Res<Time>, mut stances: Query<(&Stance, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (stance, mut transform) in &mut stances {
        let idle = stance.idle.at(t);
        *transform = Transform { rotation: Quat::from_rotation_y(stance.turn) * idle.rotation, ..idle };
    }
}

/// Turns heads toward a player within [`LOOK_RANGE`] and back once they leave; under a hood,
/// the whole body turns a little instead.
fn turn_heads(
    time: Res<Time>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    folk: Query<(&GlobalTransform, &Children), With<Denizen>>,
    mut stances: Query<(&mut Stance, &Children)>,
    mut heads: Query<(&mut FolkHead, &mut Transform)>,
) {
    let eye = eyes.iter().next().map(GlobalTransform::translation);
    let dt = time.delta_secs();
    for (root, children) in &folk {
        let from_world = root.affine().inverse();
        let eye = eye.map(|eye| from_world.transform_point3(eye));
        for &child in children {
            let Ok((mut stance, parts)) = stances.get_mut(child) else { continue };
            match parts.iter().find(|&e| heads.contains(e)) {
                Some(head) => {
                    let Ok((mut head, mut transform)) = heads.get_mut(head) else { continue };
                    let target = eye.and_then(|eye| look_at(transform.translation, eye)).unwrap_or(Vec2::ZERO);
                    head.look.smooth_nudge(&target, HEAD_TURN_RATE, dt);
                    transform.rotation = head_rotation(head.look);
                }
                None => {
                    let target = eye.and_then(|eye| look_at(Vec3::ZERO, eye)).map_or(0.0, |look| (look.x * BODY_TURN_SHARE).clamp(-BODY_TURN_MAX, BODY_TURN_MAX));
                    stance.turn.smooth_nudge(&target, HEAD_TURN_RATE, dt);
                }
            }
        }
    }
}

/// Keeps one circle per folk in the player's collision ([`Obstacles`]).
fn block_the_way(folk: Query<(&Denizen, &GlobalTransform)>, mut obstacles: ResMut<Obstacles>) {
    let circles: Vec<(Vec2, f32)> = folk.iter().map(|(denizen, at)| (at.translation().xz(), denizen.radius)).collect();
    if obstacles.0 != circles {
        obstacles.0 = circles;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folk_1207() -> Vec<FolkSpot> {
        fb_level::generate(1207).folk
    }

    #[test]
    fn the_same_spot_is_the_same_person() {
        for spot in folk_1207() {
            assert_eq!(Person::of(&spot), Person::of(&spot));
        }
    }

    #[test]
    fn everyone_on_1207_is_dressed_and_holds_something() {
        for spot in folk_1207() {
            let person = Person::of(&spot);
            let figure = Figure::of(&person);
            assert!(!figure.held.is_empty(), "{} holds nothing: {:?}", person.name, person.gear.iter().map(|g| &g.1.name).collect::<Vec<_>>());
            assert!(!figure.worn.layers.is_empty(), "{} wears nothing", person.name);
            assert!(figure.head.iter().chain(&figure.torso).all(|p| !p.mesh.parts.is_empty()), "{}", person.name);
            // Standing on the floor, about as tall as their folk.
            let (lo, hi) = figure.bounds();
            assert!(lo.y > -0.5 && lo.y < 0.5, "{}: lowest point {}", person.name, lo.y);
            assert!(hi.y > spot.folk.race().h * 0.85, "{}: {} tall", person.name, hi.y);
        }
    }

    #[test]
    fn faction_outfits_name_real_outfits() {
        for row in fb_level::FACTIONS {
            for key in row.outfits {
                assert!(fb_items::outfit::outfit(key).is_some(), "{}: {key}", row.name);
            }
        }
    }

    #[test]
    fn figures_are_built_once_per_spot() {
        let spots = folk_1207();
        let mut figures = FolkFigures::default();
        let first = figures.for_spots(&spots[..2]);
        let again = figures.for_spots(&spots[..2]);
        assert!(first.iter().zip(&again).all(|(a, b)| Arc::ptr_eq(a, b)));
        assert_eq!(first[0].0.spot, spots[0]);
    }

    #[test]
    fn folk_stand_on_their_spot_facing_its_yaw() {
        let spot = folk_1207()[0];
        let t = stand_at(&spot);
        assert_eq!(t.translation, Vec3::new(spot.x * fb_level::CELL, 0.0, spot.y * fb_level::CELL));
        // The body's face (+Z) points the way the spot faces.
        let face = t.rotation * Vec3::Z;
        let (dx, dy) = spot.facing();
        assert!(face.abs_diff_eq(Vec3::new(dx, 0.0, dy), 1e-5), "{face} vs ({dx}, {dy})");
    }

    #[test]
    fn idle_motion_is_subtle_and_each_persons_own() {
        let idles: Vec<Idle> = folk_1207().iter().map(|s| Idle::of(s.seed)).collect();
        for (i, idle) in idles.iter().enumerate() {
            let frames: Vec<Transform> = (0..600).map(|k| idle.at(k as f32 * 0.1)).collect();
            // It moves...
            let lift = frames.iter().map(|t| t.scale.y).fold(f32::MIN, f32::max) - frames.iter().map(|t| t.scale.y).fold(f32::MAX, f32::min);
            let sway = frames.iter().map(|t| t.translation.x).fold(f32::MIN, f32::max) - frames.iter().map(|t| t.translation.x).fold(f32::MAX, f32::min);
            assert!(lift > 0.003 && sway > 0.1, "{i}: lift {lift} sway {sway}");
            // ...but only a little: under 2 cm across and a degree of lean, the feet staying put.
            for t in &frames {
                assert!(t.translation.length() <= 0.2 && t.translation.y == 0.0, "{t:?}");
                assert!(t.rotation.angle_between(Quat::IDENTITY) < 1f32.to_radians());
                assert!((t.scale - Vec3::ONE).abs().max_element() < 0.01);
            }
            for other in &idles[i + 1..] {
                assert_ne!(idle, other);
                assert!((1..50).any(|k| idle.at(k as f32 * 0.37) != other.at(k as f32 * 0.37)), "two folk move in step");
            }
        }
    }

    #[test]
    fn heads_look_toward_someone_near_in_front() {
        let head = Vec3::new(0.0, 14.0, 0.0);
        // Ahead and to the right (+X), at eye height: turns right, looks a little up.
        let look = look_at(head, Vec3::new(10.0, 16.0, 20.0)).unwrap();
        assert!(look.x > 0.3 && look.x < 0.6 && look.y > 0.0, "{look}");
        // Far off, or right behind: no look.
        assert_eq!(look_at(head, Vec3::new(0.0, 16.0, 60.0)), None);
        assert_eq!(look_at(head, Vec3::new(0.0, 16.0, -20.0)), None);
        // Off to the side the turn is held to the neck's limit.
        let side = look_at(head, Vec3::new(20.0, 14.0, -2.0)).unwrap();
        assert!((side.x - HEAD_YAW_MAX).abs() < 1e-5, "{side}");
        // A head turned right by a positive yaw: its face points toward +X.
        assert!((head_rotation(Vec2::new(0.5, 0.0)) * Vec3::Z).x > 0.4);
        assert!((head_rotation(Vec2::new(0.0, 0.3)) * Vec3::Z).y > 0.2, "a positive pitch looks up");
    }

    /// An app with one stand-in figure (no meshes) facing +Z from the origin, and a camera.
    fn looking_app(eye: Vec3) -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(50)))
            .init_resource::<Obstacles>()
            .add_systems(Update, (turn_heads, block_the_way));
        let person = Person::of(&folk_1207()[0]);
        let root = app.world_mut().spawn((Denizen { person, radius: 2.5 }, Transform::default())).id();
        let stance = app.world_mut().spawn((Stance { idle: Idle::of(1), turn: 0.0 }, Transform::default(), ChildOf(root))).id();
        let head = app.world_mut().spawn((FolkHead::default(), Transform::from_xyz(0.0, 14.0, 0.0), ChildOf(stance))).id();
        let camera = app.world_mut().spawn((PlayerCamera, Transform::from_translation(eye))).id();
        (app, head, camera)
    }

    fn settle(app: &mut App) {
        // Four and a half seconds at 20 frames a second.
        for _ in 0..90 {
            app.update();
        }
    }

    #[test]
    fn heads_turn_toward_a_near_player_and_back() {
        let (mut app, head, camera) = looking_app(Vec3::new(15.0, 16.0, 25.0));
        settle(&mut app);
        let look = app.world().get::<FolkHead>(head).unwrap().look;
        assert!(look.x > 0.1, "turned toward the player: {look}");
        let face = app.world().get::<Transform>(head).unwrap().rotation * Vec3::Z;
        assert!(face.x > 0.1, "{face}");
        // The player walks off: the head comes back round.
        app.world_mut().get_mut::<Transform>(camera).unwrap().translation = Vec3::new(200.0, 16.0, 0.0);
        settle(&mut app);
        let look = app.world().get::<FolkHead>(head).unwrap().look;
        assert!(look.length() < 0.05, "back to straight ahead: {look}");
    }

    #[test]
    fn folk_block_the_way_where_they_stand() {
        let (mut app, _, _) = looking_app(Vec3::new(0.0, 16.0, 30.0));
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Obstacles>().0, vec![(Vec2::ZERO, 2.5)]);
        // And on 1207 every folk stands in a circle wide enough to bump into.
        for spot in folk_1207().iter().take(2) {
            let figure = Figure::of(&Person::of(spot));
            assert!(figure.radius > 1.5 && figure.radius < 5.0, "{}", figure.radius);
        }
    }
}
