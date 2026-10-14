//! The carried light, items lying in the level, picking them up, and the HUD.
//!
//! - What each hand holds is drawn as a view model: by a second camera on [`VIEW_MODEL_LAYER`],
//!   a child of whatever [`PlayerCamera`] exists, as in Bevy's `first_person_view_model`
//!   example, with its own field of view, so it never sinks into a wall. The main hand's item
//!   is low on the right, the off hand's low on the left, each fitted and turned in the fist as
//!   the doll holds it (`crate::view_model`). It follows the inventory ([`show_held`]): equip a
//!   sword and the view holds a sword.
//! - The carried light is the lit torch or lantern the delver holds: the main hand's, or the
//!   off hand's if the main hand holds none lit ([`carried_light`]). Equip a lit one and it is
//!   carried, take it off and the delver walks in the dark, lit only by the level's own lights
//!   (braziers, glowcaps, lit things on the floor). On the view model's stage it has its own
//!   soft light, so the flame does not burn its own handle white, and lights what the other
//!   hand holds.
//! - What is held moves with the player's steps ([`Gait`]) and lags a little behind a turn of
//!   the head ([`HeldSway`]).
//! - Its light in the world is a separate point light near the eye, on the default layer, with
//!   its colour, reach and flicker from its [`LightAnchor`]. It is the main light.
//! - Items lie where `CurrentLevel.items` puts them, resting on the floor; long ones lie down.
//!   Each spot names an Armory family and a seed; [`fb_items::roll_loot`] makes a whole item of
//!   it (a base of that family, rarity, affixes, name, dressed wear and etching).
//!   A lit one lights its corner with a shadowed light, so it stays on its side of the rock;
//!   the item itself casts no shadow, or its own flame and handle would black out its light.
//! - Every item lying in the level is kept track of in the [`Inventory`], loose, so a bag set
//!   down keeps its contents and the inventory screen can show what lies underfoot.
//! - A ray from the camera picks the nearest item or container in reach ([`Aimed`]), which the
//!   hover card describes (`crate::inventory::card`); E picks an item up into the inventory
//!   ([`Inventory::pick_up`]) and opens a container (`crate::props`). Things are set down from the
//!   inventory screen ([`set_down`], [`drop_spot`]), and rest like any other floor item.
//!
//! With no player in the app a stand-in camera is spawned at the level's start
//! ([`StandInCamera`]). For captures, `FB_DEBUG_POSE` holds the player's view at a pose
//! ([`DebugPose`]).

use armory::{
    Family, Item,
    build::light::LightAnchor,
    families::{food::Food, lantern::Lantern, sundry::Sundry, torch::Torch as TorchGenerator},
};
use bevy::{
    app::{HierarchyPropagatePlugin, Propagate},
    camera::{Exposure, RenderTarget, visibility::RenderLayers},
    light::NotShadowCaster,
    prelude::*,
    render::view::{Msaa, Tonemapping},
};
use fb_level::{CELL, ItemSpot, Level};

use crate::character::Character;
use crate::inventory::{Inventory, ItemId, doll::lit_hand};
use crate::light::{Torch, cast_shadows, point_light};
use crate::keymap::{Action, ActionInput, Keymap};
use crate::launch::AppState;
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, EYE_HEIGHT, Obstacle, Obstacles, Player, PlayerCamera, ScreenOpen, level_has_focus};
use crate::player::{Gait, blocked_by, slide_around};
use crate::props::{Container, PropPiece};
use crate::render::{ItemAppearance, ItemPalette, ItemSpawner};
use crate::ui::{BG, BRIGHT};
use crate::view_model::{VIEW_MODEL_FOV_DEGREES, held_in_view, side_of};

/// Render layer of the view model: what the player holds, drawn over the world.
pub const VIEW_MODEL_LAYER: usize = 1;

/// How far the player can reach to pick something up, in world units (2 m).
pub const REACH: f32 = 20.0;

/// How far in front of the player's feet a dropped item is set down, in world units (80 cm):
/// a metre and three quarters from the eye, inside [`REACH`].
pub const DROP_DISTANCE: f32 = 8.0;

/// Furthest to either side of straight ahead a dropped item is set down, in world units
/// (50 cm), so it is still within [`REACH`] of the eye.
pub const DROP_SPREAD: f32 = 5.0;

/// The flame of a lit thing no hold fits ([`held_pose`]) in the view model, in camera space:
/// low on the right, about a metre out as the view model camera sees it. Held in the off hand,
/// it is low on the left. Torches and lanterns are placed by `crate::view_model`.
const HELD_FLAME: Vec3 = Vec3::new(5.4, -2.6, -11.0);

/// Where the view model stands in world space: far below any level, out of reach of every
/// light in it. Bevy 0.20's GPU light clustering does not filter lights by render layer, so
/// keeping the world's lights off what is held takes distance, not layers.
pub const VIEW_MODEL_STAGE: Vec3 = Vec3::new(0.0, -100_000.0, 0.0);

/// Where the carried light hangs in the world, in camera space: level with the eye, a little right of it
/// and hardly ahead of it, so it stays on the player's side of a wall they stand
/// against and does not swing down onto the floor when they look down.
const WORLD_FLAME: Vec3 = Vec3::new(1.5, 0.0, -1.5);

/// Where the carried light's light on the view model's stage sits relative to its flame: up,
/// left and toward the eye (mirrored in the off hand).
const HELD_LIGHT_OFFSET: Vec3 = Vec3::new(-3.0, 3.0, 7.0);

/// Lowest a light given by an item on the floor hangs, in world units (60 cm).
const FLOOR_LIGHT_HEIGHT: f32 = 6.0;

/// How far a lit thing's light reaches in the world, in multiples of its [`LightAnchor`]
/// radius: the radius is its bright light, and dim light runs as far again beyond it, as a
/// torch's does in the D&D 5e SRD 5.1 ("Adventuring Gear": bright light in a 20-foot radius and
/// dim light for an additional 20 feet).
const DIM_REACH: f32 = 2.0;
/// How bright a lit thing's light is close up, as a multiple of full. Over 1 widens the fully
/// lit core; the shader caps it at full, so nothing near burns white.
const WORLD_PEAK: f32 = 1.0;
/// The held torch's own light on the stage: close-up strength and reach, set so the whole
/// torch sits in its upper steps and reads firelit.
const HELD_PEAK: f32 = 1.8;
const HELD_REACH: f32 = 40.0;

/// How far what is held bobs at walking pace, in view model units, as `(sideways, down)`: a
/// hand swings further than the head bobs (our own tuning, about 3 cm and 2.5 cm at a metre).
const HELD_BOB: Vec2 = Vec2::new(0.3, 0.25);
/// How far behind a turn of the head what is held trails: the angle it lags is the turn rate
/// times this many seconds, up to [`HELD_LAG_MAX`] radians either way.
const HELD_LAG_SECONDS: f32 = 0.05;
const HELD_LAG_MAX: f32 = 0.12;
/// How quickly the lag closes on its target, per second ([`StableInterpolate`]).
const HELD_LAG_RATE: f32 = 12.0;

pub struct CarryPlugin;

impl Plugin for CarryPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<HierarchyPropagatePlugin<RenderLayers>>() {
            app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(Update));
        }
        if !app.is_plugin_added::<HierarchyPropagatePlugin<NotShadowCaster>>() {
            app.add_plugins(HierarchyPropagatePlugin::<NotShadowCaster>::new(Update));
        }
        app.init_resource::<Inventory>()
            .init_resource::<Character>()
            .init_resource::<Keymap>()
            .init_resource::<Aimed>()
            // Only a delve that opens at startup can lack a player; one begun from the title
            // spawns its own on entering.
            .add_systems(PostStartup, spawn_stand_in_camera.run_if(in_state(AppState::Delving)).run_if(no_player_camera))
            .add_systems(PostUpdate, hold_debug_pose.before(TransformSystems::Propagate).in_set(DelveSystems))
            .add_systems(
                Update,
                (
                    lay_out_items.run_if(resource_exists_and_changed::<CurrentLevel>),
                    equip_player_camera,
                    show_held.after(equip_player_camera),
                    follow_player_camera,
                    sway_held,
                    (aim, pick_up.run_if(level_has_focus)).chain(),
                    hide_hud_under_screens.run_if(resource_exists::<ScreenOpen>),
                )
                    .in_set(DelveSystems),
            );
    }
}

/// The light an Armory item gives while it burns or glows, if it is that kind of item.
pub fn item_light(item: &Item) -> Option<LightAnchor> {
    match item {
        Item::Torch(state) => TorchGenerator::light(state),
        Item::Lantern(state) => Lantern::light(state),
        Item::Sundry(state) => Sundry::light(state),
        Item::Food(state) => Food::light(state),
        _ => None,
    }
}

/// The first burning torch rolled at or after `seed` ([`fb_items::roll_item`]).
pub fn lit_torch(seed: u32) -> fb_items::Item {
    (0..u32::MAX)
        .map(|k| seed.wrapping_add(k))
        .find_map(|s| fb_items::roll_item("Torch", s).filter(|item| item_light(&item.shape).is_some()))
        .expect("some seed rolls a burning torch")
}

/// The carried light: the lit thing in the main hand, or else in the off hand, with its hand.
pub fn carried_light(inventory: &Inventory) -> Option<(fb_items::Slot, ItemId)> {
    let hand = lit_hand(inventory)?;
    Some((hand, inventory.worn(hand)?))
}

/// The item's own look: its palette, condition and seed, with the wear scale the prototype
/// gives jewellery and food.
pub fn appearance_of(item: &Item) -> ItemAppearance {
    let decay_scale_factor = match item.family() {
        Family::Ring | Family::Amulet | Family::Charm => 8.0,
        Family::Food => 3.0,
        _ => 1.0,
    };
    ItemAppearance {
        palette: ItemPalette { materials: item.palette(), ..ItemPalette::ARMORY },
        condition: item.condition(),
        seed: item.seed(),
        etching: item.etching(),
        decay_scale_factor,
        ..default()
    }
}

/// How an item of item-space `size` lies on the floor: long things (more than twice as tall
/// as they are wide or deep, such as swords, spears, staves, bows and torches) are laid on
/// their side, turned about whichever level axis keeps their thinnest side down. Anything
/// else stands as it was made.
pub fn lie_down(size: Vec3) -> Quat {
    if size.y <= 2.0 * size.x.max(size.z) {
        return Quat::IDENTITY;
    }
    if size.z <= size.x {
        // Item +Y goes to +Z, so its depth (the thin side) faces down.
        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)
    } else {
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
    }
}

/// Where an item rests: laid down by [`lie_down`], turned `yaw` about +Y, centred over `at`
/// on the floor (y = 0) with its lowest point touching it. `points` are its item-space
/// vertices. `None` for an item with no vertices.
pub fn rest_on_floor(points: &[Vec3], at: Vec3, yaw: f32) -> Option<Transform> {
    let (min, max) = bounds(points.iter().copied())?;
    let rotation = Quat::from_rotation_y(yaw) * lie_down(max - min);
    let (min, max) = bounds(points.iter().map(|&p| rotation * p))?;
    let centre = (min + max) * 0.5;
    Some(Transform { translation: Vec3::new(at.x - centre.x, -min.y, at.z - centre.z), rotation, ..default() })
}

fn bounds(points: impl Iterator<Item = Vec3>) -> Option<(Vec3, Vec3)> {
    points.fold(None, |acc, p| Some(acc.map_or((p, p), |(lo, hi): (Vec3, Vec3)| (lo.min(p), hi.max(p)))))
}

/// Distance along a ray from `origin` in unit direction `dir` to where it enters the box
/// `min..max`, 0 if it starts inside, `None` if it misses or the box is behind.
pub fn ray_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let (mut near, mut far) = (0.0f32, f32::INFINITY);
    for axis in 0..3 {
        let (o, d) = (origin[axis], dir[axis]);
        if d.abs() < 1e-8 {
            if o < min[axis] || o > max[axis] {
                return None;
            }
            continue;
        }
        let (a, b) = ((min[axis] - o) / d, (max[axis] - o) / d);
        near = near.max(a.min(b));
        far = far.min(a.max(b));
        if near > far {
            return None;
        }
    }
    Some(near)
}

/// The ray's nearest hit among `targets` (each a rigid world-from-item transform and its
/// item-space bounds) within `reach`.
pub fn nearest_hit<T: Copy>(
    origin: Vec3,
    dir: Vec3,
    reach: f32,
    targets: impl IntoIterator<Item = (T, Transform, Vec3, Vec3)>,
) -> Option<(T, f32)> {
    targets
        .into_iter()
        .filter_map(|(id, world_from_item, min, max)| {
            let inverse = world_from_item.compute_affine().inverse();
            let local_origin = inverse.transform_point3(origin);
            let local_dir = inverse.transform_vector3(dir).normalize_or_zero();
            ray_box(local_origin, local_dir, min, max).filter(|&t| t <= reach).map(|t| (id, t))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// An item lying in the level, with its item-space bounds for picking. The item itself is in
/// the [`Inventory`], loose.
#[derive(Component, Debug)]
pub struct FloorItem {
    pub id: ItemId,
    pub spot: ItemSpot,
    pub min: Vec3,
    pub max: Vec3,
}

/// What one hand holds, drawn in the view (a root of the view model).
#[derive(Component, Debug)]
#[require(HeldSway)]
pub struct HeldItem {
    pub item: Item,
    pub hand: fb_items::Slot,
}

/// What a [`PlayerCamera`]'s view model was last drawn for: the item in each hand, the carried
/// light among them, and whose hands hold them (the body they are fitted to).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldKey {
    pub main: Option<ItemId>,
    pub off: Option<ItemId>,
    pub light: Option<ItemId>,
    pub body: (fb_body::Folk, u32),
}

impl HeldKey {
    pub fn of(inventory: &Inventory, character: &Character) -> HeldKey {
        HeldKey {
            main: inventory.worn(fb_items::Slot::Mainhand),
            off: inventory.worn(fb_items::Slot::Offhand),
            light: carried_light(inventory).map(|(_, id)| id),
            body: (character.folk, character.body_seed),
        }
    }
}

/// On a [`PlayerCamera`]: what its view model holds, if drawn yet, and what was spawned to draw
/// it (each hand's item, and the carried light's light on the stage and in the world).
#[derive(Component, Debug, Default)]
pub struct Carrying {
    pub held: Option<HeldKey>,
    entities: Vec<Entity>,
}

impl Carrying {
    /// The carried light, if the view model holds one.
    pub fn light(&self) -> Option<ItemId> {
        self.held.and_then(|h| h.light)
    }
}

/// How a held thing moves in the view: it bobs with the player's steps and trails a turn of the
/// head, then settles back to `rest`. Moving the view model with the same cycle as the eye is
/// what Quake does (`V_CalcRefdef` adds `V_CalcBob`'s bob to the view model's origin in
/// `WinQuake/view.c`, <https://github.com/id-Software/Quake>); the sideways swing and the lag
/// behind a turn are our own.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct HeldSway {
    /// Where it is held when the player stands still, in view model camera space.
    pub rest: Transform,
    /// The look direction last frame, as `(yaw, pitch)` in radians.
    pub look: Option<Vec2>,
    /// How far it trails the look now, as `(yaw, pitch)` in radians.
    pub lag: Vec2,
}

impl HeldSway {
    pub fn new(rest: Transform) -> Self {
        HeldSway { rest, look: None, lag: Vec2::ZERO }
    }

    /// Follows the look turning to `look` (`(yaw, pitch)`, radians) over `dt` seconds: the lag
    /// eases toward the turn rate times [`HELD_LAG_SECONDS`], capped, and back to 0 once the
    /// look stops.
    pub fn follow(&mut self, look: Vec2, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let turned = self.look.map_or(Vec2::ZERO, |last| {
            let d = look - last;
            // Yaw wraps; take the short way round.
            Vec2::new((d.x + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI, d.y)
        });
        self.look = Some(look);
        let target = (turned / dt * HELD_LAG_SECONDS).clamp(Vec2::splat(-HELD_LAG_MAX), Vec2::splat(HELD_LAG_MAX));
        self.lag.smooth_nudge(&target, HELD_LAG_RATE, dt);
    }

    /// Where it is held this frame with the player's step at `gait`: its rest bobbed by
    /// [`HELD_BOB`], then turned about the eye against the lag, so a head turning left leaves
    /// it trailing right and a head tipping up leaves it trailing low.
    pub fn transform(&self, gait: Gait) -> Transform {
        let bob = gait.bob() * HELD_BOB;
        let trail = Quat::from_euler(EulerRot::YXZ, -self.lag.x, -self.lag.y, 0.0);
        let held = self.rest.translation + Vec3::new(bob.x, -bob.y, 0.0);
        Transform { translation: trail * held, rotation: trail * self.rest.rotation, ..self.rest }
    }
}

/// The look direction of a world transform, as `(yaw, pitch)`: yaw turning left from -Z about
/// +Y, pitch up from level.
fn yaw_pitch(eye: &GlobalTransform) -> Vec2 {
    let forward = eye.forward().as_vec3();
    Vec2::new((-forward.x).atan2(-forward.z), forward.y.clamp(-1.0, 1.0).asin())
}

/// The camera that draws the view model over the world. It stands on [`VIEW_MODEL_STAGE`],
/// with what is held and the carried light's light as children.
#[derive(Component, Debug)]
pub struct ViewModelCamera {
    /// The [`PlayerCamera`] it draws over.
    pub follows: Entity,
}

/// A [`PlayerCamera`] that already has its torch and HUD.
#[derive(Component, Debug)]
struct Equipped;

/// The floor item, or container standing in the level (`crate::props`), under the crosshair
/// and in reach, if any.
#[derive(Resource, Debug, Default)]
pub struct Aimed(pub Option<Entity>);

/// Where `FB_DEBUG_POSE` holds the player's view, for captures: `1` at the level's start,
/// `x,y[,yaw[,pitch]]` at a grid point (cells), turned `yaw` degrees from north (positive turns
/// west) and tilted `pitch` degrees (negative looks down). While it is set the view is held
/// there every frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugPose {
    /// On the floor, in world units.
    pub at: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

impl DebugPose {
    fn from_env() -> Option<String> {
        std::env::var("FB_DEBUG_POSE").ok().filter(|v| !v.is_empty())
    }

    /// The pose for `FB_DEBUG_POSE`'s value `spec` in `level`.
    pub fn parse(level: &Level, spec: &str) -> DebugPose {
        let (sx, sy, facing) = level.start();
        let mut values = spec.split(',').map(|v| v.trim().parse::<f32>().ok());
        let (x, y, yaw, pitch) = match (values.next().flatten(), values.next().flatten()) {
            (Some(x), Some(y)) if spec.contains(',') => {
                let yaw = values.next().flatten().map_or(facing.yaw(), f32::to_radians);
                (x, y, yaw, values.next().flatten().unwrap_or(0.0).to_radians())
            }
            _ => (sx as f32 + 0.5, sy as f32 + 0.5, facing.yaw(), 0.0),
        };
        DebugPose { at: Vec3::new(x * CELL, 0.0, y * CELL), yaw, pitch }
    }

    /// The eye's world transform in this pose.
    pub fn eye(&self) -> Transform {
        Transform::from_translation(self.at + Vec3::Y * EYE_HEIGHT).with_rotation(Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0))
    }
}

/// A camera spawned because no [`PlayerCamera`] existed after startup, so the torch and items
/// can be seen without the player plugin.
#[derive(Component, Debug)]
pub struct StandInCamera;

fn no_player_camera(cameras: Query<(), With<PlayerCamera>>) -> bool {
    cameras.is_empty()
}

fn spawn_stand_in_camera(mut commands: Commands, level: Res<CurrentLevel>) {
    warn!("no PlayerCamera after startup; spawning a stand-in at the level's start");
    commands.spawn((
        StandInCamera,
        PlayerCamera,
        Camera3d::default(),
        Projection::from(PerspectiveProjection { fov: 70f32.to_radians(), near: 0.05, ..default() }),
        Exposure::INDOOR,
        // As in the turntable: a tone curve would flatten the bands.
        Tonemapping::Linear,
        IsDefaultUiCamera,
        DebugPose::parse(&level.0, "1").eye(),
    ));
}

/// Holds the view at `FB_DEBUG_POSE`'s pose: the player's body at the spot and turned, its
/// camera tilted; or a camera with no body placed outright.
fn hold_debug_pose(
    level: Res<CurrentLevel>,
    mut cameras: Query<(&mut Transform, Option<&ChildOf>), With<PlayerCamera>>,
    mut bodies: Query<&mut Transform, (With<Player>, Without<PlayerCamera>)>,
) {
    let Some(spec) = DebugPose::from_env() else { return };
    let pose = DebugPose::parse(&level.0, &spec);
    for (mut camera, parent) in &mut cameras {
        match parent.and_then(|p| bodies.get_mut(p.parent()).ok()) {
            Some(mut body) => {
                body.translation = pose.at;
                body.rotation = Quat::from_rotation_y(pose.yaw);
                camera.rotation = Quat::from_rotation_x(pose.pitch);
            }
            None => *camera = pose.eye(),
        }
    }
}

/// Spawns (or respawns, when the level changes) every item the level lays out, each kept
/// track of in the inventory, loose.
pub(crate) fn lay_out_items(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    old: Query<(Entity, &FloorItem)>,
    mut inventory: ResMut<Inventory>,
    mut spawner: ItemSpawner,
) {
    for (entity, floor) in &old {
        inventory.forget(floor.id);
        commands.entity(entity).despawn();
    }
    for spot in &level.0.items {
        if let Some(item) = fb_items::roll_loot(spot.family, spot.seed) {
            let id = inventory.add(item);
            spawn_floor_item(&mut spawner, &inventory, id, *spot);
        }
    }
}

/// The item's item-space vertices.
pub fn item_points(mesh: &armory::ItemMesh) -> Vec<Vec3> {
    mesh.parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p)).collect()
}

/// Spawns item `id` resting on the floor at `spot` as a [`FloorItem`], drawn with its own
/// palette, dressed condition and etching, with its light if it gives one. `None` for an item
/// with no vertices or one the inventory does not know.
pub fn spawn_floor_item(spawner: &mut ItemSpawner, inventory: &Inventory, id: ItemId, spot: ItemSpot) -> Option<Entity> {
    let item = inventory.item(id)?;
    let mesh = item.shape.build();
    let points = item_points(&mesh);
    let at = Vec3::from_array(Level::world(spot.x, spot.y));
    let transform = rest_on_floor(&points, at, spot.yaw)?;
    let (min, max) = bounds(points.iter().copied())?;
    let light = item_light(&item.shape);
    let mut root = spawner.spawn(&mesh, &appearance_of(&item.shape));
    root.insert((transform, Name::new(item.title().to_string()), FloorItem { id, spot, min, max }));
    if let Some(anchor) = light {
        // A lit thing on the floor lights its corner, from a little above the flame so the
        // stone under it does not burn white. Its light is shadowed so it stops at the rock,
        // and the thing itself casts none, so its own flame and body do not hide its light.
        let mut flame = transform.transform_point(Vec3::from_array(anchor.position));
        flame.y = flame.y.max(FLOOR_LIGHT_HEIGHT);
        let at = transform.compute_affine().inverse().transform_point3(flame);
        root.insert(Propagate(NotShadowCaster)).with_child((world_light(&anchor, &Exposure::INDOOR), Transform::from_translation(at)));
    }
    Some(root.id())
}

/// An item's footprint radius on the floor once laid down ([`lie_down`]): half the larger of
/// its level extents. 0 for an item with no vertices.
pub fn floor_radius(points: &[Vec3]) -> f32 {
    bounds(points.iter().copied()).map_or(0.0, |(min, max)| {
        let size = (lie_down(max - min) * (max - min)).abs();
        size.x.max(size.z) * 0.5
    })
}

/// Where an item of floor `radius` set down by a player at `feet` (world `(x, z)`) facing
/// `forward` (a unit floor direction) comes to rest: [`DROP_DISTANCE`] ahead, or short of that
/// where rock, a `blocked` cell or one of `obstacles` (a prop, someone standing) is in the
/// way, clear of it by `radius`. Found by sliding the item's footprint out from the feet as the
/// body slides along walls and round obstacles ([`slide_around`]). The radius is
/// held under half a cell so an item fits down a one-cell corridor. `aside` moves the spot to
/// the right (negative to the left), held within [`DROP_SPREAD`].
pub fn drop_spot(feet: Vec2, forward: Vec2, aside: f32, radius: f32, blocked: impl Fn(i32, i32) -> bool, obstacles: &[Obstacle]) -> Vec2 {
    let radius = radius.min(CELL * 0.5 - 0.5);
    let right = Vec2::new(-forward.y, forward.x);
    slide_around(feet, feet + forward * DROP_DISTANCE + right * aside.clamp(-DROP_SPREAD, DROP_SPREAD), radius, blocked, obstacles)
}

/// Sets loose item `id` down in front of the eye ([`drop_spot`]), into the level data too so it
/// stays where it was left, and spawns it there. `None` if it has no vertices.
#[allow(clippy::too_many_arguments)]
pub fn set_down(
    spawner: &mut ItemSpawner,
    inventory: &Inventory,
    level: &mut Level,
    blocked_cells: &BlockedCells,
    obstacles: &Obstacles,
    eye: &GlobalTransform,
    id: ItemId,
    aside: f32,
) -> Option<Entity> {
    let item = inventory.item(id)?;
    // Straight down, the top of the head points the way the body faces.
    let forward = eye.forward().as_vec3().xz().try_normalize().unwrap_or_else(|| eye.up().as_vec3().xz().normalize_or(Vec2::NEG_Y));
    let radius = floor_radius(&item_points(&item.shape.build()));
    let at = drop_spot(eye.translation().xz(), forward, aside, radius, blocked_by(level, blocked_cells), &obstacles.0) / CELL;
    // Laid square to the player's view.
    let yaw = (-forward.x).atan2(-forward.y);
    // For anything that came from a level spot, the spot rolls the same item again
    // ([`fb_items::roll_loot`]); what is spawned is the item itself.
    let spot = ItemSpot { x: at.x, y: at.y, yaw, family: item.shape.family(), seed: item.seed };
    level.items.push(spot);
    spawn_floor_item(spawner, inventory, id, spot)
}

/// Lifts a floor item out of the level: out of the level data, without respawning what is
/// still there, and its entity despawned. The item stays in the inventory.
pub fn lift(commands: &mut Commands, level: &mut Level, entity: Entity, floor: &FloorItem) {
    if let Some(index) = level.items.iter().position(|spot| *spot == floor.spot) {
        level.items.remove(index);
    }
    commands.entity(entity).despawn();
}

/// A flickering point light in `anchor`'s colour and flicker, `peak` times full close up and
/// reaching `range` units.
fn anchor_light(anchor: &LightAnchor, peak: f32, range: f32, exposure: &Exposure) -> PointLight {
    let color = Color::srgb(anchor.color.0, anchor.color.1, anchor.color.2);
    Torch::flickering(point_light(color, peak, range, exposure), anchor.flicker)
}

/// An anchor's light in the world: bright out to its radius, dim to [`DIM_REACH`] times it,
/// and shadowed, so it never reaches through rock into the next corridor.
pub fn world_light(anchor: &LightAnchor, exposure: &Exposure) -> PointLight {
    cast_shadows(anchor_light(anchor, WORLD_PEAK, anchor.radius * DIM_REACH, exposure))
}

/// How a camera draws, which the view model camera copies from the player camera.
type CameraLook = (Option<&'static Exposure>, Option<&'static Tonemapping>, Option<&'static Msaa>);

/// Gives each new [`PlayerCamera`] its view model camera, on the stage, and the HUD. What it
/// carries is [`carry_light`]'s.
fn equip_player_camera(mut commands: Commands, cameras: Query<(Entity, CameraLook), (With<PlayerCamera>, Without<Equipped>)>) {
    for (camera, (exposure, tonemapping, msaa)) in &cameras {
        let exposure = exposure.copied().unwrap_or_default();
        let view_model = commands
            .spawn((
                ViewModelCamera { follows: camera },
                Camera3d::default(),
                Camera { order: 1, clear_color: ClearColorConfig::None, ..default() },
                Projection::from(PerspectiveProjection { fov: VIEW_MODEL_FOV_DEGREES.to_radians(), near: 0.05, ..default() }),
                exposure,
                tonemapping.copied().unwrap_or(Tonemapping::Linear),
                msaa.copied().unwrap_or_default(),
                // The level's ambient does not reach the stage; a little warm fill of its own.
                AmbientLight { color: Torch::COLOR, brightness: 0.06 / exposure.exposure(), ..default() },
                RenderLayers::layer(VIEW_MODEL_LAYER),
                Transform::from_translation(VIEW_MODEL_STAGE),
            ))
            .id();
        if std::env::var_os("FB_DEBUG_LIGHT").is_some() {
            // The player camera's debug light (`crate::player`), on the stage too, so what is
            // held can be seen without a light in hand.
            commands.spawn((point_light(Color::WHITE, 0.5, 200.0, &exposure), RenderLayers::layer(VIEW_MODEL_LAYER), ChildOf(view_model)));
        }
        commands.entity(camera).insert((Equipped, Carrying::default()));
        spawn_hud(&mut commands, view_model);
    }
}

/// How a lit thing that no hold fits (a candle, a glowing mushroom) is carried in the view
/// model: where its flame sits in camera space and its turn. A torch is held upright and
/// leaning in and away, as a torch is carried; a lantern hangs plumb from the fist. In the off
/// hand both are mirrored to the left. Torches and lanterns themselves are fitted as the doll
/// holds them (`crate::view_model`).
pub fn held_pose(item: &Item, hand: fb_items::Slot) -> (Vec3, Quat) {
    let left = hand == fb_items::Slot::Offhand;
    let flame = if left { HELD_FLAME * Vec3::new(-1.0, 1.0, 1.0) } else { HELD_FLAME };
    let lean = match item {
        Item::Lantern(_) => 0.0,
        _ => 0.22,
    };
    let lean = if left { -lean } else { lean };
    let tip = if matches!(item, Item::Lantern(_)) { 0.0 } else { -0.32 };
    (flame, Quat::from_euler(EulerRot::XYZ, tip, 0.0, lean))
}

/// What each hand holds, placed in the view model camera's space: each item fitted to the
/// delver's body as on the doll and carried into the view (`crate::view_model`), or, for a lit
/// thing no hold fits, held by its flame ([`held_pose`]). Other things a hand might hold that
/// no hold fits are not drawn.
pub fn held_items(inventory: &Inventory, character: &Character) -> Vec<(fb_items::Slot, Item, Transform)> {
    use fb_items::Slot;
    let shape = |slot| inventory.worn(slot).and_then(|id| inventory.item(id)).map(|item| item.shape.clone());
    let (main, off) = (shape(Slot::Mainhand), shape(Slot::Offhand));
    let fitted = held_in_view(&character.spec(), main.as_ref(), off.as_ref());
    [(Slot::Mainhand, main), (Slot::Offhand, off)]
        .into_iter()
        .filter_map(|(hand, item)| {
            if let Some(held) = fitted.iter().find(|h| h.hand == hand) {
                return Some((hand, held.item.clone(), held.transform));
            }
            let item = item?;
            let anchor = item_light(&item)?;
            let (flame, rotation) = held_pose(&item, hand);
            Some((hand, item, Transform::from_translation(flame - rotation * Vec3::from_array(anchor.position)).with_rotation(rotation)))
        })
        .collect()
}

/// Keeps each player camera's view model in step with the hands ([`held_items`]) and its
/// carried light ([`carried_light`]): when either changes, the old view model and its lights go
/// and the new ones are spawned, the carried light's light in the world from its
/// [`LightAnchor`]. With nothing lit in hand there is no light; with nothing in hand, nothing is
/// drawn.
fn show_held(
    mut commands: Commands,
    inventory: Res<Inventory>,
    character: Res<Character>,
    mut cameras: Query<(Entity, &mut Carrying, Option<&Exposure>), With<PlayerCamera>>,
    view_models: Query<(Entity, &ViewModelCamera)>,
    mut spawner: ItemSpawner,
) {
    let wanted = HeldKey::of(&inventory, &character);
    for (camera, mut carrying, exposure) in &mut cameras {
        if carrying.held == Some(wanted) {
            continue;
        }
        let Some((view_model, _)) = view_models.iter().find(|(_, v)| v.follows == camera) else { continue };
        for entity in carrying.entities.drain(..) {
            commands.entity(entity).try_despawn();
        }
        carrying.held = Some(wanted);
        let exposure = exposure.copied().unwrap_or_default();
        let view_layer = RenderLayers::layer(VIEW_MODEL_LAYER);
        let light_hand = carried_light(&inventory).map(|(hand, _)| hand);
        for (hand, item, rest) in held_items(&inventory, &character) {
            let mut root = spawner.spawn(&item.build(), &appearance_of(&item));
            root.insert((
                Name::new(format!("Held {}", item.family().key())),
                HeldItem { item: item.clone(), hand },
                HeldSway::new(rest),
                rest,
                Propagate(view_layer.clone()),
                ChildOf(view_model),
            ));
            carrying.entities.push(root.id());
            if light_hand != Some(hand) {
                continue;
            }
            let Some(anchor) = item_light(&item) else { continue };
            let flame = rest.transform_point(Vec3::from_array(anchor.position));
            // Off the flame toward the eye, so the whole light sits at much the same distance
            // from it and reads firelit rather than burnt white.
            let toward_eye = if side_of(hand) == fb_body::OFF_HAND { HELD_LIGHT_OFFSET * Vec3::new(-1.0, 1.0, 1.0) } else { HELD_LIGHT_OFFSET };
            let stage_light =
                commands.spawn((anchor_light(&anchor, HELD_PEAK, HELD_REACH, &exposure), Transform::from_translation(flame + toward_eye), view_layer.clone(), ChildOf(view_model))).id();
            let world = commands.spawn((world_light(&anchor, &exposure), Transform::from_translation(WORLD_FLAME), ChildOf(camera))).id();
            carrying.entities.extend([stage_light, world]);
        }
    }
}

/// The HUD over the level: the crosshair. Hidden while a screen is open.
#[derive(Component, Debug)]
pub struct Hud;

fn hide_hud_under_screens(open: Res<ScreenOpen>, mut huds: Query<&mut Visibility, With<Hud>>) {
    let shown = if open.0.is_some() { Visibility::Hidden } else { Visibility::Inherited };
    for mut visibility in &mut huds {
        visibility.set_if_neq(shown);
    }
}

fn spawn_hud(commands: &mut Commands, camera: Entity) {
    commands
        .spawn((
            Name::new("HUD"),
            Hud,
            UiTargetCamera(camera),
            Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() },
            Pickable::IGNORE,
        ))
        .with_children(|hud| {
            // The crosshair: a small cream dot ringed in the page's near black.
            hud.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(50),
                    top: percent(50),
                    width: px(6),
                    height: px(6),
                    margin: UiRect { left: px(-3), top: px(-3), ..default() },
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(BRIGHT.with_alpha(0.85)),
                BorderColor::all(BG.with_alpha(0.7)),
            ));
        });
}

/// Keeps the view model camera drawing where the player camera draws, e.g. once frame
/// capture retargets it to an image.
fn follow_player_camera(
    mut commands: Commands,
    players: Query<Ref<RenderTarget>, With<PlayerCamera>>,
    view_models: Query<(Entity, Ref<ViewModelCamera>)>,
) {
    for (entity, view_model) in &view_models {
        match players.get(view_model.follows) {
            Ok(target) if target.is_changed() || view_model.is_added() => {
                commands.entity(entity).insert(RenderTarget::clone(&target));
            }
            Ok(_) => {}
            // The player camera is gone: so is its view model, torch and HUD with it.
            Err(_) => {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Bobs what is held with the player's steps and trails it behind turns of the head.
pub(crate) fn sway_held(
    time: Res<Time>,
    mut held: Query<(&mut HeldSway, &mut Transform, &ChildOf)>,
    view_models: Query<&ViewModelCamera>,
    eyes: Query<(&GlobalTransform, Option<&ChildOf>), With<PlayerCamera>>,
    gaits: Query<&Gait>,
) {
    for (mut sway, mut transform, parent) in &mut held {
        let Ok(view_model) = view_models.get(parent.parent()) else { continue };
        let Ok((eye, body)) = eyes.get(view_model.follows) else { continue };
        let gait = body.and_then(|b| gaits.get(b.parent()).ok()).copied().unwrap_or_default();
        sway.follow(yaw_pitch(eye), time.delta_secs());
        *transform = sway.transform(gait);
    }
}

fn aim(
    camera: Query<&GlobalTransform, (With<PlayerCamera>, With<Equipped>)>,
    items: Query<(Entity, &GlobalTransform, &FloorItem)>,
    containers: Query<(Entity, &GlobalTransform, &PropPiece), With<Container>>,
    mut aimed: ResMut<Aimed>,
) {
    let Some(eye) = camera.iter().next() else {
        aimed.0 = None;
        return;
    };
    let targets = items
        .iter()
        .map(|(entity, transform, floor)| (entity, transform.compute_transform(), floor.min, floor.max))
        .chain(containers.iter().map(|(entity, transform, piece)| (entity, transform.compute_transform(), piece.min, piece.max)));
    let hit = nearest_hit(eye.translation(), eye.forward().as_vec3(), REACH, targets).map(|(entity, _)| entity);
    if aimed.0 != hit {
        aimed.0 = hit;
    }
}

/// E picks up the item under the crosshair ([`Inventory::pick_up`]). Something with no room for
/// it stays where it lies.
pub(crate) fn pick_up(
    mut commands: Commands,
    input: ActionInput,
    mut aimed: ResMut<Aimed>,
    items: Query<&FloorItem>,
    mut inventory: ResMut<Inventory>,
    mut level: ResMut<CurrentLevel>,
) {
    if !input.just_pressed(Action::PickUp) {
        return;
    }
    let Some(entity) = aimed.0 else { return };
    let Ok(floor) = items.get(entity) else { return };
    match inventory.pick_up(floor.id) {
        Ok(()) => {
            aimed.0 = None;
            lift(&mut commands, &mut level.bypass_change_detection().0, entity, floor);
        }
        Err(refusal) => info!("{}", refusal.reason()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_items::Rarity;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn long_items_lie_on_their_thin_side() {
        // A sword: tall, wide at the guard, thin through the blade.
        let sword = lie_down(Vec3::new(2.5, 9.0, 0.4));
        let size = (sword * Vec3::new(2.5, 9.0, 0.4)).abs();
        assert!(close(size.y, 0.4), "{size}");
        // A spear thin both ways still lies flat.
        let spear = (lie_down(Vec3::new(0.5, 20.0, 0.6)) * Vec3::new(0.5, 20.0, 0.6)).abs();
        assert!(spear.y < 1.0, "{spear}");
        // A bag stands as made.
        assert_eq!(lie_down(Vec3::new(3.0, 4.0, 3.0)), Quat::IDENTITY);
    }

    #[test]
    fn items_rest_on_the_floor_at_their_spot() {
        let box_points: Vec<Vec3> = (0..8)
            .map(|i| Vec3::new(if i & 1 == 0 { -1.0 } else { 1.5 }, if i & 2 == 0 { -2.0 } else { 7.0 }, if i & 4 == 0 { -0.2 } else { 0.3 }))
            .collect();
        let at = Vec3::new(40.0, 0.0, 60.0);
        let transform = rest_on_floor(&box_points, at, 0.7).unwrap();
        let placed: Vec<Vec3> = box_points.iter().map(|&p| transform.transform_point(p)).collect();
        let (min, max) = bounds(placed.into_iter()).unwrap();
        assert!(close(min.y, 0.0), "{min}");
        assert!(max.y < 1.0, "a long thing lies down: {max}");
        assert!(close((min.x + max.x) * 0.5, at.x) && close((min.z + max.z) * 0.5, at.z));
        assert!(rest_on_floor(&[], at, 0.0).is_none());
    }

    #[test]
    fn rays_hit_boxes_in_front_only() {
        let (min, max) = (Vec3::splat(-1.0), Vec3::splat(1.0));
        assert!(close(ray_box(Vec3::new(0.0, 0.0, 5.0), Vec3::NEG_Z, min, max).unwrap(), 4.0));
        assert_eq!(ray_box(Vec3::new(0.0, 0.0, 5.0), Vec3::Z, min, max), None);
        assert_eq!(ray_box(Vec3::new(3.0, 0.0, 5.0), Vec3::NEG_Z, min, max), None);
        assert_eq!(ray_box(Vec3::ZERO, Vec3::X, min, max), Some(0.0));
    }

    #[test]
    fn the_nearest_item_in_reach_is_picked() {
        let (min, max) = (Vec3::splat(-1.0), Vec3::splat(1.0));
        let at = |z: f32| Transform::from_xyz(0.0, 0.0, z).with_rotation(Quat::from_rotation_y(0.6));
        let targets = [(1, at(-15.0), min, max), (2, at(-8.0), min, max), (3, at(-40.0), min, max)];
        assert_eq!(nearest_hit(Vec3::ZERO, Vec3::NEG_Z, REACH, targets).map(|h| h.0), Some(2));
        assert_eq!(nearest_hit(Vec3::ZERO, Vec3::NEG_Z, REACH, [targets[2]]), None);
    }

    #[test]
    fn the_starting_torch_is_lit() {
        for seed in [0, 1, 1207, u32::MAX] {
            let item = lit_torch(seed);
            assert_eq!(item.shape.family(), Family::Torch);
            let light = item_light(&item.shape).unwrap();
            assert!(light.radius > 20.0 && (0.0..=1.0).contains(&light.flicker));
        }
    }

    #[test]
    fn the_carried_light_follows_the_hands() {
        use fb_items::Slot;
        let mut inventory = Inventory::default();
        assert_eq!(carried_light(&inventory), None, "nothing lit in hand: no carried light");
        let torch = inventory.add(lit_torch(3));
        inventory.wear(torch, Slot::Offhand).unwrap();
        assert_eq!(carried_light(&inventory), Some((Slot::Offhand, torch)));
        // A sword in the main hand is no light; the off hand's torch still is.
        let sword = inventory.add(fb_items::roll_item("Sword", 4).unwrap());
        inventory.wear(sword, Slot::Mainhand).unwrap();
        assert_eq!(carried_light(&inventory), Some((Slot::Offhand, torch)));
        inventory.take_out(torch);
        assert_eq!(carried_light(&inventory), None);
        // A torch that is out gives none.
        let spare = inventory.add((1..400).filter_map(|s| fb_items::roll_item("Torch", s)).find(|i| item_light(&i.shape).is_none()).unwrap());
        inventory.wear(spare, Slot::Offhand).unwrap();
        assert_eq!(carried_light(&inventory), None);
    }

    #[test]
    fn the_view_model_follows_the_hands() {
        use fb_items::Slot;
        let mut app = App::new();
        app.init_resource::<Inventory>()
            .init_resource::<Character>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
            .init_resource::<crate::render::OutlineMaterials>()
            .add_systems(Update, show_held);
        let camera = app.world_mut().spawn((PlayerCamera, Carrying::default())).id();
        let view_model = app.world_mut().spawn(ViewModelCamera { follows: camera }).id();
        let (mace, torch) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let mace = inventory.add((1..400).filter_map(|s| fb_items::roll_item("Mace", s)).find(|i| i.shape.family() == Family::Mace).unwrap());
            let torch = inventory.add(lit_torch(5));
            inventory.wear(mace, Slot::Mainhand).unwrap();
            inventory.wear(torch, Slot::Offhand).unwrap();
            (mace, torch)
        };
        let held = |app: &mut App| {
            let mut query = app.world_mut().query::<(&HeldItem, &Transform, &ChildOf)>();
            let mut held: Vec<(Slot, Family, f32)> = query.iter(app.world()).map(|(h, t, c)| { assert_eq!(c.parent(), view_model); (h.hand, h.item.family(), t.translation.x) }).collect();
            held.sort_by_key(|h| h.0 as u8);
            held
        };
        app.update();
        let both = held(&mut app);
        assert_eq!(both.iter().map(|h| (h.0, h.1)).collect::<Vec<_>>(), vec![(Slot::Mainhand, Family::Mace), (Slot::Offhand, Family::Torch)]);
        assert!(both[0].2 > 0.0 && both[1].2 < 0.0, "the mace on the right, the torch on the left: {both:?}");
        // The torch still lights the level: a light on the player camera.
        let mut lights = app.world_mut().query::<(&PointLight, &ChildOf)>();
        assert_eq!(lights.iter(app.world()).filter(|(_, c)| c.parent() == camera).count(), 1);
        assert_eq!(app.world().get::<Carrying>(camera).unwrap().light(), Some(torch));

        // A sword equipped in its place swaps the view model; the torch stays.
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let sword = inventory.add(fb_items::roll_item("Sword", 3).unwrap());
            inventory.equip(sword, Slot::Mainhand).unwrap();
            assert_ne!(inventory.worn(Slot::Mainhand), Some(mace));
        }
        app.update();
        assert_eq!(held(&mut app).iter().map(|h| (h.0, h.1)).collect::<Vec<_>>(), vec![(Slot::Mainhand, Family::Sword), (Slot::Offhand, Family::Torch)]);
        // Empty hands hold nothing, and nothing lights the level.
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            for slot in [Slot::Mainhand, Slot::Offhand] {
                let id = inventory.worn(slot).unwrap();
                inventory.take_out(id);
            }
        }
        app.update();
        assert!(held(&mut app).is_empty());
        let mut lights = app.world_mut().query::<&PointLight>();
        assert_eq!(lights.iter(app.world()).count(), 0);
    }

    #[test]
    fn a_light_in_the_off_hand_is_held_on_the_left() {
        let torch = lit_torch(1).shape;
        let (right, lean) = held_pose(&torch, fb_items::Slot::Mainhand);
        let (left, mirrored) = held_pose(&torch, fb_items::Slot::Offhand);
        assert!(right.x > 0.0 && left.x < 0.0 && right.y == left.y);
        assert!(lean.abs_diff_eq(Quat::from_euler(EulerRot::XYZ, -0.32, 0.0, 0.22), 1e-6));
        assert!(mirrored.abs_diff_eq(Quat::from_euler(EulerRot::XYZ, -0.32, 0.0, -0.22), 1e-6));
    }

    fn rest() -> Transform {
        Transform::from_translation(HELD_FLAME).with_rotation(Quat::from_rotation_z(0.2))
    }

    #[test]
    fn the_held_torch_is_still_when_standing() {
        let mut sway = HeldSway::new(rest());
        for _ in 0..30 {
            sway.follow(Vec2::new(0.4, -0.1), 1.0 / 60.0);
        }
        assert_eq!(sway.transform(Gait { phase: 1.1, pace: 0.0 }), rest());
    }

    #[test]
    fn the_held_torch_moves_with_the_bob_while_walking() {
        let sway = HeldSway::new(rest());
        let step = |phase: f32| sway.transform(Gait { phase, pace: 1.0 }).translation;
        let quarter = std::f32::consts::FRAC_PI_2;
        // At the footfall it is low and to one side; a step later, low and to the other.
        assert!(step(quarter).y < HELD_FLAME.y - 0.2 && step(quarter).x > HELD_FLAME.x + 0.2, "{}", step(quarter));
        assert!(step(3.0 * quarter).x < HELD_FLAME.x - 0.2, "{}", step(3.0 * quarter));
        assert!(step(0.0).abs_diff_eq(HELD_FLAME, 1e-5));
    }

    #[test]
    fn the_held_torch_lags_a_turn_then_settles() {
        let mut sway = HeldSway::new(rest());
        let dt = 1.0 / 60.0;
        // Turning left at a radian a second, steadily.
        for frame in 0..30 {
            sway.follow(Vec2::new(frame as f32 * dt, 0.0), dt);
        }
        let turning = sway.transform(Gait::default()).translation;
        assert!(turning.x > HELD_FLAME.x + 0.2, "trails right: {turning}");
        assert!(turning.x < HELD_FLAME.x + 2.0, "only slightly: {turning}");
        // The look stops; the torch comes back to rest.
        for _ in 0..120 {
            sway.follow(Vec2::new(29.0 * dt, 0.0), dt);
        }
        assert!(sway.transform(Gait::default()).translation.abs_diff_eq(HELD_FLAME, 1e-3));
        // Across the yaw wrap the turn is the short way round.
        let mut wrap = HeldSway::new(rest());
        wrap.follow(Vec2::new(std::f32::consts::PI - 0.01, 0.0), dt);
        wrap.follow(Vec2::new(-std::f32::consts::PI + 0.01, 0.0), dt);
        assert!(wrap.lag.x > 0.0 && wrap.lag.x < HELD_LAG_MAX, "{}", wrap.lag);
    }

    #[test]
    fn yaw_and_pitch_read_from_the_eye() {
        let eye = GlobalTransform::from(Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.7, -0.3, 0.0)));
        assert!(yaw_pitch(&eye).abs_diff_eq(Vec2::new(0.7, -0.3), 1e-5));
    }

    #[test]
    fn lights_in_the_world_cast_shadows() {
        let anchor = item_light(&lit_torch(1207).shape).unwrap();
        let light = world_light(&anchor, &Exposure::INDOOR);
        assert!(light.shadow_maps_enabled);
        assert_eq!(light.range, anchor.radius * DIM_REACH);
    }

    #[test]
    fn the_debug_pose_is_the_start_or_where_asked() {
        let level = fb_level::generate(1207);
        let (x, y, facing) = level.start();
        let start = DebugPose::parse(&level, "1");
        assert_eq!(start.at, Vec3::new((x as f32 + 0.5) * CELL, 0.0, (y as f32 + 0.5) * CELL));
        assert_eq!(start.yaw, facing.yaw());
        let there = DebugPose::parse(&level, "4.5, 6.2, 0, -30");
        assert_eq!(there.at, Vec3::new(90.0, 0.0, 124.0));
        assert_eq!(there.eye().translation.y, EYE_HEIGHT);
        assert!(there.eye().forward().y < -0.4);
    }

    #[test]
    fn dropped_items_stop_short_of_rock() {
        // A corridor running east, one cell wide, rock at x = 3 cells.
        let blocked = |x: i32, y: i32| !(y == 0 && (0..3).contains(&x));
        let feet = Vec2::new(10.0, 10.0);
        assert!(drop_spot(feet, Vec2::X, 0.0, 2.0, blocked, &[]).abs_diff_eq(Vec2::new(10.0 + DROP_DISTANCE, 10.0), 1e-3));
        // Up against the end wall it lands clear of it by its radius.
        let near_end = Vec2::new(55.0, 10.0);
        assert!(drop_spot(near_end, Vec2::X, 0.0, 2.0, blocked, &[]).x <= 3.0 * CELL - 2.0 + 1e-3);
        // A long thing still fits down the corridor, its radius held under half a cell.
        let spot = drop_spot(feet, Vec2::Y, 0.0, 40.0, blocked, &[]);
        assert!(spot.y >= 9.5 && spot.y <= 10.5, "{spot}");
    }


    #[test]
    fn dropped_items_stop_short_of_props() {
        let open = |_: i32, _: i32| false;
        let chest = [Obstacle::Rect { centre: Vec2::new(50.0, 50.0 - DROP_DISTANCE), half: Vec2::new(4.5, 2.4), yaw: 0.0 }];
        let spot = drop_spot(Vec2::new(50.0, 50.0), Vec2::NEG_Y, 0.0, 1.0, open, &chest);
        assert!(!chest[0].overlaps(spot, 1.0), "{spot}");
        assert!(spot.y > 50.0 - DROP_DISTANCE + 2.4, "{spot}");
    }

    #[test]
    fn dropped_items_go_aside_within_reach() {
        let open = |_: i32, _: i32| false;
        let feet = Vec2::new(50.0, 50.0);
        // Facing -Z, the right is +X.
        let right = drop_spot(feet, Vec2::NEG_Y, 3.0, 1.0, open, &[]);
        assert!(right.abs_diff_eq(Vec2::new(53.0, 50.0 - DROP_DISTANCE), 1e-3), "{right}");
        let far = drop_spot(feet, Vec2::NEG_Y, -40.0, 1.0, open, &[]);
        assert!((far.x - (50.0 - DROP_SPREAD)).abs() < 1e-3, "{far}");
        let eye = Vec3::new(feet.x, EYE_HEIGHT, feet.y);
        assert!(Vec3::new(far.x, 0.0, far.y).distance(eye) < REACH);
    }

    /// An app with what picking up and setting down need, the level emptied of loot, and a
    /// pack worn on the back.
    fn delving_app() -> (App, ItemId) {
        let mut level = fb_level::generate(1207);
        level.items.clear();
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let mut app = App::new();
        app.add_plugins(bevy::transform::TransformPlugin)
            .insert_resource(CurrentLevel(level))
            .insert_resource(inventory)
            .init_resource::<Aimed>()
            .init_resource::<BlockedCells>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Keymap>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
            .init_resource::<crate::render::OutlineMaterials>()
            .add_systems(Update, (aim, pick_up).chain());
        (app, pack)
    }

    #[test]
    fn a_set_down_item_rests_in_reach_and_can_be_picked_up_again() {
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, pack) = delving_app();
        let start = DebugPose::parse(&app.world().resource::<CurrentLevel>().0, "1");
        let item = fb_items::roll_item("Mace", 1207).unwrap();
        let id = app.world_mut().resource_mut::<Inventory>().add(item.clone());
        let eye = start.eye();
        let camera = app.world_mut().spawn((PlayerCamera, Equipped, eye)).id();
        app.world_mut()
            .run_system_once(
                move |mut spawner: ItemSpawner, inventory: Res<Inventory>, mut level: ResMut<CurrentLevel>, blocked: Res<BlockedCells>| {
                    set_down(&mut spawner, &inventory, &mut level.0, &blocked, &Obstacles::default(), &GlobalTransform::from(eye), id, 0.0);
                },
            )
            .unwrap();
        let mut floor = app.world_mut().query::<(Entity, &Transform, &FloorItem)>();
        let (dropped, at, spot, floor_id) = floor.single(app.world()).map(|(e, t, f)| (e, *t, f.spot, f.id)).unwrap();
        assert_eq!(floor_id, id);
        assert_eq!(app.world().resource::<CurrentLevel>().0.items, vec![spot]);
        // In front of the player, on open floor, within reach.
        let ahead = (at.translation - eye.translation).xz();
        assert!(ahead.dot(eye.forward().as_vec3().xz()) > 0.0, "{ahead}");
        let reach = Vec3::new(spot.x * CELL, 0.0, spot.y * CELL).distance(eye.translation);
        assert!(reach < REACH, "{reach}");
        assert!(app.world().resource::<CurrentLevel>().0.is_open(spot.x.floor() as i32, spot.y.floor() as i32));
        // Resting like a spawned item: lowest point on the floor.
        let points = item_points(&item.shape.build());
        let low = points.iter().map(|&p| at.transform_point(p).y).fold(f32::INFINITY, f32::min);
        assert!(low.abs() < 1e-3, "{low}");

        // Look down at it and pick it up again with E: into the pack.
        let target = Vec3::new(spot.x * CELL, 0.5, spot.y * CELL);
        *app.world_mut().get_mut::<Transform>(camera).unwrap() = eye.looking_at(target, Vec3::Y);
        // One frame to carry the turn to its global transform, one to aim with it.
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Aimed>().0, Some(dropped));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyE);
        app.update();
        let inventory = app.world().resource::<Inventory>();
        assert!(matches!(inventory.location(id), Some(crate::inventory::Location::In { container, .. }) if container == pack));
        assert!(app.world().resource::<CurrentLevel>().0.items.is_empty());
        assert!(app.world().get_entity(dropped).is_err());
    }

    #[test]
    fn a_container_in_reach_is_under_the_crosshair_and_e_leaves_it_standing() {
        let (mut app, _) = delving_app();
        // A chest a metre ahead of an eye looking down at it.
        let at = Transform::from_xyz(0.0, 0.0, -10.0);
        let spot = fb_level::PropSpot { x: 0.0, y: -0.5, yaw: 0.0, prop: fb_level::Prop::Container { style: armory::families::chests::ChestStyle::Chest, seed: 1, loot: 1 } };
        let piece = PropPiece { index: 0, spot, min: Vec3::new(-3.0, 0.0, -2.0), max: Vec3::new(3.0, 4.0, 2.0) };
        let id = app.world_mut().resource_mut::<Inventory>().add(fb_items::container_item(fb_level::chest_state(armory::families::chests::ChestStyle::Chest, 1, false)));
        let chest = app.world_mut().spawn((at, GlobalTransform::from(at), piece, Container { id, opened: false })).id();
        let eye = Transform::from_xyz(0.0, EYE_HEIGHT, 0.0).looking_at(Vec3::new(0.0, 2.0, -10.0), Vec3::Y);
        app.world_mut().spawn((PlayerCamera, Equipped, eye, GlobalTransform::from(eye)));
        app.update();
        assert_eq!(app.world().resource::<Aimed>().0, Some(chest));
        // E picks nothing up: the chest is never carried off (`crate::props` opens it).
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyE);
        app.update();
        assert!(app.world().get_entity(chest).is_ok());
        assert_eq!(app.world().resource::<Inventory>().location(id), Some(crate::inventory::Location::Loose));
    }

    #[test]
    fn e_picks_up_the_item_under_the_crosshair() {
        let (mut app, pack) = delving_app();
        let level = fb_level::generate(1207);
        let (spot, item) = level
            .items
            .iter()
            .map(|s| (*s, fb_items::roll_loot(s.family, s.seed).unwrap()))
            .find(|(_, i)| i.profile.packable && i.profile.holds.is_none())
            .unwrap();
        app.world_mut().resource_mut::<CurrentLevel>().0.items.push(spot);
        let name = item.title().to_string();
        let id = app.world_mut().resource_mut::<Inventory>().add(item);
        // An item a metre ahead of an eye looking down at it.
        let at = Transform::from_xyz(0.0, 0.0, -10.0);
        let floor = app.world_mut().spawn((at, GlobalTransform::from(at), FloorItem { id, spot, min: Vec3::splat(-1.0), max: Vec3::splat(1.0) })).id();
        let eye = Transform::from_xyz(0.0, EYE_HEIGHT, 0.0).looking_at(Vec3::new(0.0, 0.0, -10.0), Vec3::Y);
        app.world_mut().spawn((PlayerCamera, Equipped, eye, GlobalTransform::from(eye)));

        app.update();
        assert_eq!(app.world().resource::<Aimed>().0, Some(floor));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyE);
        app.update();
        assert!(app.world().get_entity(floor).is_err());
        let inventory = app.world().resource::<Inventory>();
        let packed: Vec<String> = inventory.grid(pack).unwrap().entries().iter().map(|e| inventory.item(e.id).unwrap().title().to_string()).collect();
        assert_eq!(packed, vec![name]);
        assert!(app.world().resource::<CurrentLevel>().0.items.is_empty());
    }

    #[test]
    fn floor_items_on_1207_are_rolled_with_rarity() {
        let level = fb_level::generate(1207);
        let items: Vec<fb_items::Item> = level.items.iter().map(|s| fb_items::roll_loot(s.family, s.seed).unwrap()).collect();
        assert_eq!(items.len(), level.items.len());
        for (item, spot) in items.iter().zip(&level.items) {
            assert_eq!(item.shape.family(), spot.family);
            if item.profile.kind == fb_items::Kind::Gear {
                assert_eq!(item.shape.condition().state, armory::cond::ConditionState::Custom, "{}: not dressed", item.name);
            }
        }
        assert!(items.iter().any(|i| i.rarity != Rarity::Common), "a level of nothing but common items");
    }

    #[test]
    fn floor_items_wear_their_own_condition() {
        let item = (1..500).filter_map(|seed| fb_items::roll_item("Sword", seed)).find(|i| i.rarity == Rarity::Common).unwrap();
        assert_eq!(appearance_of(&item.shape).condition, item.shape.condition());
        assert!(item.shape.condition().wear >= 0.15);
    }
}
