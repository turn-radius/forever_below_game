//! The torch in hand, Armory items lying in the level, picking them up, and the HUD.
//!
//! - The held torch is a view model: drawn by a second camera on [`VIEW_MODEL_LAYER`], a child
//!   of whatever [`PlayerCamera`] exists, as in Bevy's `first_person_view_model` example. It
//!   has its own soft light, so the flame does not burn its own handle white, and its own
//!   field of view, so it never sinks into a wall.
//! - Its light in the world is a separate point light near the eye, on the default layer, with
//!   the torch's colour, reach and flicker from its [`LightAnchor`]. It is the main light.
//! - Items lie where `CurrentLevel.items` puts them, resting on the floor; long ones lie down.
//! - A ray from the camera picks the nearest item in reach; E picks it up into [`Carried`].
//!
//! With no player in the app a stand-in camera is spawned at the level's start
//! ([`StandInCamera`]). For captures, `FB_DEBUG_POSE` holds the player's view at a pose
//! ([`DebugPose`]).

use armory::{
    Family, Item,
    build::light::LightAnchor,
    families::{Generator, food::Food, lantern::Lantern, sundry::Sundry, torch::Torch as TorchGenerator},
};
use bevy::{
    app::{HierarchyPropagatePlugin, Propagate},
    camera::{Exposure, RenderTarget, visibility::RenderLayers},
    prelude::*,
    render::view::{Msaa, Tonemapping},
};
use fb_level::{CELL, ItemSpot, Level};

use crate::light::{Torch, cast_shadows, point_light};
use crate::play::{Carried, CurrentLevel, EYE_HEIGHT, Player, PlayerCamera};
use crate::render::{ItemAppearance, ItemPalette, ItemSpawner};

/// Render layer of the view model: what the player holds, drawn over the world.
pub const VIEW_MODEL_LAYER: usize = 1;

/// How far the player can reach to pick something up, in world units (2 m).
pub const REACH: f32 = 20.0;

/// The torch's flame in the view model, in camera space: low on the right, about a metre out
/// as the view model camera sees it.
const HELD_FLAME: Vec3 = Vec3::new(5.4, -2.6, -11.0);

/// Where the view model stands in world space: far below any level, out of reach of every
/// light in it. Bevy 0.20's GPU light clustering does not filter lights by render layer, so
/// keeping the world's lights off the held torch takes distance, not layers.
pub const VIEW_MODEL_STAGE: Vec3 = Vec3::new(0.0, -100_000.0, 0.0);

/// Where the torch's light hangs in the world, in camera space: level with the eye, a little right of it
/// and hardly ahead of it, so it stays on the player's side of a wall they stand
/// against and does not swing down onto the floor when they look down.
const WORLD_FLAME: Vec3 = Vec3::new(1.5, 0.0, -1.5);

/// Where the view model's light sits relative to [`HELD_FLAME`]: up, left and toward the eye.
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

/// Field of view of the view model camera, fixed whatever the world camera uses.
const VIEW_MODEL_FOV_DEGREES: f32 = 60.0;

/// UI palette (`docs/rendering.md`, "Palette and UI").
const PAPER: Color = Color::srgb(0xeb as f32 / 255.0, 0xe5 as f32 / 255.0, 0xd5 as f32 / 255.0);
const INK: Color = Color::srgb(0x2b as f32 / 255.0, 0x24 as f32 / 255.0, 0x20 as f32 / 255.0);
const ACCENT: Color = Color::srgb(0x6f as f32 / 255.0, 0x45 as f32 / 255.0, 0x26 as f32 / 255.0);

pub struct CarryPlugin;

impl Plugin for CarryPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<HierarchyPropagatePlugin<RenderLayers>>() {
            app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(Update));
        }
        app.init_resource::<Carried>()
            .init_resource::<Aimed>()
            .add_systems(PostStartup, spawn_stand_in_camera.run_if(no_player_camera))
            .add_systems(PostUpdate, hold_debug_pose.before(TransformSystems::Propagate))
            .add_systems(
                Update,
                (
                    lay_out_items.run_if(resource_exists_and_changed::<CurrentLevel>),
                    equip_player_camera,
                    follow_player_camera,
                    (aim, pick_up, show_aimed, show_carried).chain(),
                ),
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

/// The first burning torch at or after `seed`.
pub fn lit_torch(seed: u32) -> (Item, LightAnchor) {
    (0..u32::MAX)
        .map(|k| seed.wrapping_add(k))
        .find_map(|s| {
            let state = TorchGenerator::roll(s);
            TorchGenerator::light(&state).map(|light| (Item::Torch(state), light))
        })
        .expect("some seed rolls a burning torch")
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

/// An Armory item lying in the level, with its item-space bounds for picking.
#[derive(Component, Debug)]
pub struct FloorItem {
    pub item: Item,
    pub spot: ItemSpot,
    pub min: Vec3,
    pub max: Vec3,
}

/// The torch in the player's hand (the view model's root).
#[derive(Component, Debug)]
pub struct HeldTorch {
    pub item: Item,
}

/// The camera that draws the view model over the world. It stands on [`VIEW_MODEL_STAGE`],
/// with the held torch and its light as children.
#[derive(Component, Debug)]
pub struct ViewModelCamera {
    /// The [`PlayerCamera`] it draws over.
    pub follows: Entity,
}

/// A [`PlayerCamera`] that already has its torch and HUD.
#[derive(Component, Debug)]
struct Equipped;

/// The floor item under the crosshair and in reach, if any.
#[derive(Resource, Debug, Default)]
pub struct Aimed(pub Option<Entity>);

#[derive(Component)]
struct AimedLabel;

#[derive(Component)]
struct CarriedPanel;

#[derive(Component)]
struct CarriedList;

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

/// Spawns (or respawns, when the level changes) every item the level lays out.
fn lay_out_items(mut commands: Commands, level: Res<CurrentLevel>, old: Query<Entity, With<FloorItem>>, mut spawner: ItemSpawner) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    for spot in &level.0.items {
        let Some(item) = Item::roll(spot.family, spot.seed) else { continue };
        let mesh = item.build();
        let points: Vec<Vec3> = mesh.parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p)).collect();
        let at = Vec3::from_array(Level::world(spot.x, spot.y));
        let (Some(transform), Some((min, max))) = (rest_on_floor(&points, at, spot.yaw), bounds(points.iter().copied())) else {
            continue;
        };
        let light = item_light(&item);
        let mut root = spawner.spawn(&mesh, &appearance_of(&item));
        root.insert((transform, Name::new(item.noun()), FloorItem { item, spot: *spot, min, max }));
        if let Some(anchor) = light {
            // A lit thing on the floor lights its corner, from a little above the flame so the
            // stone under it does not burn white; shadows only for the light in hand.
            let mut flame = transform.transform_point(Vec3::from_array(anchor.position));
            flame.y = flame.y.max(FLOOR_LIGHT_HEIGHT);
            let at = transform.compute_affine().inverse().transform_point3(flame);
            root.with_child((world_light(&anchor, &Exposure::INDOOR), Transform::from_translation(at)));
        }
    }
}

/// A flickering point light in `anchor`'s colour and flicker, `peak` times full close up and
/// reaching `range` units.
fn anchor_light(anchor: &LightAnchor, peak: f32, range: f32, exposure: &Exposure) -> (Torch, PointLight) {
    let color = Color::srgb(anchor.color.0, anchor.color.1, anchor.color.2);
    Torch::flickering(point_light(color, peak, range, exposure), anchor.flicker)
}

/// An anchor's light in the world: bright out to its radius, dim to [`DIM_REACH`] times it.
fn world_light(anchor: &LightAnchor, exposure: &Exposure) -> (Torch, PointLight) {
    anchor_light(anchor, WORLD_PEAK, anchor.radius * DIM_REACH, exposure)
}

/// How a camera draws, which the view model camera copies from the player camera.
type CameraLook = (Option<&'static Exposure>, Option<&'static Tonemapping>, Option<&'static Msaa>);

/// Gives each new [`PlayerCamera`] the torch's light in the world, the view model (its own
/// camera, the held torch and that torch's light, on the stage) and the HUD.
fn equip_player_camera(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    cameras: Query<(Entity, CameraLook), (With<PlayerCamera>, Without<Equipped>)>,
    mut spawner: ItemSpawner,
) {
    for (camera, (exposure, tonemapping, msaa)) in &cameras {
        let exposure = exposure.copied().unwrap_or_default();
        let (torch, anchor) = lit_torch(level.0.seed.wrapping_mul(0x9e37_79b9).wrapping_add(0x70c4));
        let view_layer = RenderLayers::layer(VIEW_MODEL_LAYER);

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
                view_layer.clone(),
                Transform::from_translation(VIEW_MODEL_STAGE),
            ))
            .id();

        // Held upright and leaning in and away, as a torch is carried; its flame at HELD_FLAME.
        let rotation = Quat::from_euler(EulerRot::XYZ, -0.32, 0.0, 0.22);
        let held_at = HELD_FLAME - rotation * Vec3::from_array(anchor.position);
        let mesh = torch.build();
        let mut root = spawner.spawn(&mesh, &appearance_of(&torch));
        root.insert((
            Name::new("Held torch"),
            HeldTorch { item: torch },
            Transform::from_translation(held_at).with_rotation(rotation),
            Propagate(view_layer.clone()),
            ChildOf(view_model),
        ));
        commands.entity(view_model).with_child((
            // Off the flame toward the eye, so the whole torch sits at much the same distance
            // from it and reads firelit rather than burnt white.
            anchor_light(&anchor, HELD_PEAK, HELD_REACH, &exposure),
            Transform::from_translation(HELD_FLAME + HELD_LIGHT_OFFSET),
            view_layer,
        ));
        let (torch_light, light) = world_light(&anchor, &exposure);
        commands.entity(camera).insert(Equipped).with_child((torch_light, cast_shadows(light), Transform::from_translation(WORLD_FLAME)));
        spawn_hud(&mut commands, view_model);
    }
}

fn spawn_hud(commands: &mut Commands, camera: Entity) {
    commands
        .spawn((
            Name::new("HUD"),
            UiTargetCamera(camera),
            Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() },
            Pickable::IGNORE,
        ))
        .with_children(|hud| {
            // The crosshair: a small paper dot ringed in ink.
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
                BackgroundColor(PAPER.with_alpha(0.85)),
                BorderColor::all(INK.with_alpha(0.7)),
            ));
            // The aimed item's noun, centred under the crosshair.
            hud.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: percent(50),
                    width: percent(100),
                    margin: UiRect::top(px(18)),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
            ))
            .with_child((
                AimedLabel,
                Text::default(),
                TextFont::from_font_size(20.0),
                TextColor(PAPER),
                TextShadow { offset: Vec2::splat(1.5), color: INK.with_alpha(0.9) },
            ));
            // The carried list: a parchment ledger in the top left corner.
            hud.spawn((
                CarriedPanel,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(18),
                    top: px(18),
                    min_width: px(180),
                    padding: UiRect::axes(px(12), px(8)),
                    border: UiRect::all(px(2)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2),
                    ..default()
                },
                BackgroundColor(PAPER.with_alpha(0.92)),
                BorderColor::all(ACCENT),
                Visibility::Hidden,
            ))
            .with_children(|panel| {
                panel.spawn((Text::new("Carried"), TextFont::from_font_size(16.0), TextColor(ACCENT)));
                panel.spawn((CarriedList, Text::default(), TextFont::from_font_size(15.0), TextColor(INK)));
            });
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

fn aim(
    camera: Query<&GlobalTransform, (With<PlayerCamera>, With<Equipped>)>,
    items: Query<(Entity, &GlobalTransform, &FloorItem)>,
    mut aimed: ResMut<Aimed>,
) {
    let Some(eye) = camera.iter().next() else {
        aimed.0 = None;
        return;
    };
    let targets = items.iter().map(|(entity, transform, floor)| (entity, transform.compute_transform(), floor.min, floor.max));
    let hit = nearest_hit(eye.translation(), eye.forward().as_vec3(), REACH, targets).map(|(entity, _)| entity);
    if aimed.0 != hit {
        aimed.0 = hit;
    }
}

fn pick_up(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut aimed: ResMut<Aimed>,
    items: Query<&FloorItem>,
    mut carried: ResMut<Carried>,
    mut level: ResMut<CurrentLevel>,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    let Some(entity) = aimed.0.take() else { return };
    let Ok(floor) = items.get(entity) else { return };
    // Out of the level data too, without respawning everything that is still there.
    let spots = &mut level.bypass_change_detection().0.items;
    if let Some(index) = spots.iter().position(|spot| *spot == floor.spot) {
        spots.remove(index);
    }
    carried.0.push(floor.item.clone());
    commands.entity(entity).despawn();
}

fn show_aimed(aimed: Res<Aimed>, items: Query<&FloorItem>, mut labels: Query<&mut Text, With<AimedLabel>>) {
    if !aimed.is_changed() {
        return;
    }
    let noun = aimed.0.and_then(|entity| items.get(entity).ok()).map(|floor| floor.item.noun()).unwrap_or_default();
    for mut label in &mut labels {
        label.0.clone_from(&noun);
    }
}

fn show_carried(
    carried: Res<Carried>,
    mut lists: Query<&mut Text, With<CarriedList>>,
    mut panels: Query<&mut Visibility, With<CarriedPanel>>,
    added: Query<(), Added<CarriedList>>,
) {
    if !carried.is_changed() && added.is_empty() {
        return;
    }
    let lines: Vec<String> = carried.0.iter().map(Item::noun).collect();
    for mut list in &mut lists {
        list.0 = lines.join("\n");
    }
    for mut visibility in &mut panels {
        *visibility = if lines.is_empty() { Visibility::Hidden } else { Visibility::Inherited };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_held_torch_is_lit() {
        for seed in [0, 1, 1207, u32::MAX] {
            let (item, light) = lit_torch(seed);
            assert_eq!(item.family(), Family::Torch);
            assert_eq!(item_light(&item), Some(light));
            assert!(light.radius > 20.0 && (0.0..=1.0).contains(&light.flicker));
        }
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
    fn e_picks_up_the_item_under_the_crosshair() {
        let mut level = fb_level::generate(1207);
        let spot = level.items[0];
        level.items.truncate(1);
        let mut app = App::new();
        app.insert_resource(CurrentLevel(level))
            .init_resource::<Carried>()
            .init_resource::<Aimed>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, (aim, pick_up).chain());
        let item = Item::roll(spot.family, spot.seed).unwrap();
        let noun = item.noun();
        // An item a metre ahead of an eye looking down at it.
        let at = Transform::from_xyz(0.0, 0.0, -10.0);
        let floor = app.world_mut().spawn((at, GlobalTransform::from(at), FloorItem { item, spot, min: Vec3::splat(-1.0), max: Vec3::splat(1.0) })).id();
        let eye = Transform::from_xyz(0.0, EYE_HEIGHT, 0.0).looking_at(Vec3::new(0.0, 0.0, -10.0), Vec3::Y);
        app.world_mut().spawn((PlayerCamera, Equipped, eye, GlobalTransform::from(eye)));

        app.update();
        assert_eq!(app.world().resource::<Aimed>().0, Some(floor));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyE);
        app.update();
        assert!(app.world().get_entity(floor).is_err());
        assert_eq!(app.world().resource::<Carried>().0.iter().map(Item::noun).collect::<Vec<_>>(), vec![noun]);
        assert!(app.world().resource::<CurrentLevel>().0.items.is_empty());
    }
}
