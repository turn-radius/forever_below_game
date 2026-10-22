//! The carried light, items lying in the level, picking them up, and the HUD.
//!
//! - What each hand holds is drawn as a view model: by a second camera on [`VIEW_MODEL_LAYER`],
//!   a child of whatever [`PlayerCamera`] exists, as in Bevy's `first_person_view_model`
//!   example, with its own field of view, so it never sinks into a wall. The main hand's item
//!   is low on the right, the off hand's low on the left, each fitted and turned in the fist as
//!   the doll holds it (`crate::view_model`). It follows the inventory ([`show_held`]): equip a
//!   sword and the view holds a sword.
//! - The carried lights are the lit torches and lanterns the delver holds or hangs at the hip
//!   ([`carried_lights`]), each giving its own light; the first of them, the main hand's, else
//!   the off hand's, else the hip's, is the carried light ([`carried_light`]). Equip a lit one
//!   and it is carried, take it off or put it out ([`crate::kindle`]) and the delver walks in
//!   the dark, lit only by the level's own lights (braziers, glowcaps, lit things on the floor). On the view model's stage it has its own
//!   soft light, so the flame does not burn its own handle white, and lights what the other
//!   hand holds.
//! - A lantern hung from the belt at the hip hangs from the player's body in the level, low on
//!   the left, seen when looking down, and swings with the steps (`crate::view_model::hip_in_view`).
//!   Lit, it is the carried light when no hand holds one: its light rises from the hip, lower
//!   than a held one, and lights what the hands hold from below.
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
//! - The crosshair's line picks the item or container in reach nearest it, within a small
//!   buffer round it ([`nearest_aimed`], [`AimBuffer`]), as folk are picked ([`Aimed`]), which the
//!   hover card describes (`crate::inventory::card`); F picks an item up into the inventory
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
use fb_body::BodySpec;
use fb_level::{CELL, ItemSpot, Level};

use crate::character::Character;
use crate::fitting::Fitting;
use crate::inventory::{FloorChanges, Inventory, ItemId, Toast, doll::lit_slot, equip_from_floor, equip_from_floor_in, header::ToastText, underfoot::apply_floor_changes};
use crate::light::{Torch, cast_shadows, point_light};
use crate::keymap::{Action, ActionInput, Keymap};
use crate::launch::AppState;
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, EYE_HEIGHT, Obstacle, Obstacles, Player, PlayerCamera, ScreenOpen, level_has_focus};
use crate::player::{Gait, blocked_by, slide_around};
use crate::props::{Container, PropPiece};
use crate::record::{WorldRecord, restore_item};
use crate::render::{ItemAppearance, ItemPalette, ItemSpawner, Shimmer, ToonMaterial};
use crate::tuning::{LightTuning, Tuning};
use crate::ui::{BG, BRIGHT, UiFonts, rarity_color};
use crate::view_model::{HipInView, held_in_view, hip_in_view, hip_swing, side_of};

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
/// it is low on the left. Torches and lanterns are placed by `crate::view_model`. Where it was
/// through the old 60-degree view, drawn larger through the narrower one (`held_fov_degrees`,
/// [`crate::tuning::ViewTuning`]).
const HELD_FLAME: Vec3 = Vec3::new(4.03, -1.94, -11.0);

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

/// The hip lantern's light on the view model's stage, in camera space, when it is the carried
/// light: low on the left and a little ahead, so it lights what the hands hold from below, as
/// the lantern at the hip does. Our own tuning.
const HIP_STAGE_LIGHT: Vec3 = Vec3::new(-3.0, -14.0, -5.0);

/// Lowest a light given by an item on the floor hangs, in world units (60 cm).
const FLOOR_LIGHT_HEIGHT: f32 = 6.0;

/// The held torch's own light on the stage: close-up strength and reach, set so the whole
/// torch sits in its upper steps and reads firelit.
const HELD_PEAK: f32 = 1.8;
const HELD_REACH: f32 = 40.0;

/// How far what is held bobs at walking pace, in view model units, as `(sideways, down)`: a
/// hand swings further than the head bobs (our own tuning, about 3 cm and 2.5 cm at a metre),
/// at a `head_bob` of 1 ([`crate::tuning::ViewTuning`]).
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
            .init_resource::<Tuning>()
            .init_resource::<crate::fitting::FitCache>()
            .init_resource::<Toast>()
            .init_resource::<FloorChanges>()
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
                    (widen_view_models, rescale_floor_items, reshimmer_floor_items).run_if(resource_changed::<Tuning>),
                    relight_given_lights,
                    follow_player_camera,
                    sway_held,
                    upright_held_flames.after(sway_held),
                    swing_hip,
                    (aim, (pick_up, equip_aimed.before(apply_floor_changes)).run_if(level_has_focus)).chain(),
                    hide_hud_under_screens.run_if(resource_exists::<ScreenOpen>),
                )
                    .in_set(DelveSystems),
            );
    }
}

/// The light an Armory item gives while it burns or glows, if it is that kind of item.
/// How high the toast line sits above the bottom of the view, in HUD units ([`crate::ui::hud`]): just clear of the orb
/// cluster and its quick bar ([`crate::combat::orbs::cluster_layout`]).
pub fn toast_lift() -> f32 {
    crate::combat::orbs::MARGIN + crate::combat::orbs::cluster_layout().size.y + 1.5
}

pub fn item_light(item: &Item) -> Option<LightAnchor> {
    match item {
        Item::Torch(state) => TorchGenerator::light(state),
        Item::Lantern(state) => Lantern::light(state),
        Item::Sundry(state) => Sundry::light(state),
        Item::Food(state) => Food::light(state),
        _ => None,
    }
}

/// [`item_light`] of a rolled item, its reach scaled by its light affixes (Gleaming, Smoky:
/// [`fb_items::Item::light_factor`]), as it lights the level.
pub fn rolled_light(item: &fb_items::Item) -> Option<LightAnchor> {
    item_light(&item.shape).map(|anchor| LightAnchor { radius: anchor.radius * item.light_factor(), ..anchor })
}

/// The [`fb_items::Item::light_factor`] of what is worn in `slot`, 1 with nothing there.
fn light_factor_in(inventory: &Inventory, slot: fb_items::Slot) -> f32 {
    inventory.worn(slot).and_then(|id| inventory.item(id)).map_or(1.0, fb_items::Item::light_factor)
}

/// The first burning torch rolled at or after `seed` ([`fb_items::roll_item`]).
pub fn lit_torch(seed: u32) -> fb_items::Item {
    (0..u32::MAX)
        .map(|k| seed.wrapping_add(k))
        .find_map(|s| fb_items::roll_item("Torch", s).filter(|item| item_light(&item.shape).is_some()))
        .expect("some seed rolls a burning torch")
}

/// The carried light: the lit thing in the main hand, or else in the off hand, or else a lit
/// lantern hung at the hip, with its slot.
pub fn carried_light(inventory: &Inventory) -> Option<(fb_items::Slot, ItemId)> {
    let slot = lit_slot(inventory)?;
    Some((slot, inventory.worn(slot)?))
}

/// Every lit thing carried, each of which gives its own light: in the main hand, the off hand
/// and at the hip, in that order.
pub fn carried_lights(inventory: &Inventory) -> Vec<(fb_items::Slot, ItemId)> {
    use fb_items::Slot;
    [Slot::Mainhand, Slot::Offhand, Slot::Hip]
        .into_iter()
        .filter_map(|slot| Some((slot, inventory.worn(slot)?)))
        .filter(|&(_, id)| inventory.item(id).is_some_and(|item| item_light(&item.shape).is_some()))
        .collect()
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

/// How an item of item-space `size` lies on the floor. Something that `stands` on a base of
/// its own (`fb_items::profile::stands_on_base`: boots, bags, bottles, lanterns) stands as it
/// was made. Anything else lies flat on its largest face, its thinnest extent up: a sword or a
/// bow on its side, a crossbow on its stock and prod, a shield on its back, face up.
pub fn lie_down(size: Vec3, stands: bool) -> Quat {
    if stands || (size.y <= size.x && size.y <= size.z) {
        return Quat::IDENTITY;
    }
    if size.z <= size.x {
        // Item +Z (its front) goes up, so its depth (the thin side) stands upright.
        Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)
    } else {
        // Item +X goes up: it lies on its side.
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
    }
}

/// Where an item rests: laid down by [`lie_down`] (standing as made if it `stands`), turned
/// `yaw` about +Y, drawn `scale` times its size (`floor_item_scale`,
/// [`crate::tuning::ViewTuning`]), centred over `at` on the floor (y = 0) with its lowest point
/// touching it. `points` are its item-space vertices. `None` for an item with no vertices.
pub fn rest_on_floor(points: &[Vec3], stands: bool, at: Vec3, yaw: f32, scale: f32) -> Option<Transform> {
    let (min, max) = bounds(points.iter().copied())?;
    let rotation = Quat::from_rotation_y(yaw) * lie_down(max - min, stands);
    let (min, max) = bounds(points.iter().map(|&p| rotation * p * scale))?;
    let centre = (min + max) * 0.5;
    Some(Transform { translation: Vec3::new(at.x - centre.x, -min.y, at.z - centre.z), rotation, scale: Vec3::splat(scale) })
}

/// `rest` (from [`rest_on_floor`] over `at`) drawn at `scale` instead: everything it places
/// grows or shrinks about `at` on the floor, so the item stays centred there with its lowest
/// point on the floor.
pub fn rescale_on_floor(rest: Transform, at: Vec3, scale: f32) -> Transform {
    let anchor = Vec3::new(at.x, 0.0, at.z);
    let ratio = if rest.scale.x.abs() > 1e-6 { scale / rest.scale.x } else { 1.0 };
    Transform { translation: anchor + (rest.translation - anchor) * ratio, scale: Vec3::splat(scale), ..rest }
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

/// How far off the crosshair's line something can lie and still be aimed at: within `angle`
/// (radians) of it, or within `min_radius` world units of the line, whichever is wider, so a
/// ring at your feet is as easy to catch as a chest across the room. From the tuning's
/// `aim_buffer_degrees` and `aim_min_radius` ([`crate::tuning::ViewTuning`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AimBuffer {
    pub angle: f32,
    pub min_radius: f32,
}

impl AimBuffer {
    /// Only what the line itself passes through.
    pub const EXACT: AimBuffer = AimBuffer { angle: 0.0, min_radius: 0.0 };

    /// The buffer the tuning gives.
    pub fn of(tuning: &Tuning) -> AimBuffer {
        AimBuffer { angle: tuning.view.aim_buffer_degrees.max(0.0).to_radians(), min_radius: tuning.view.aim_min_radius.max(0.0) }
    }

    /// How far off the line, in world units, something `along` units out may lie.
    pub fn radius_at(&self, along: f32) -> f32 {
        (along * self.angle.tan()).max(self.min_radius)
    }
}

/// The point of the box `min..max` closest to the segment from `origin` along unit `dir` for
/// `length`. Found by alternating projections, each onto the other set, which converges to a
/// closest pair for two closed convex sets (Cheney and Goldstein, "Proximity maps for convex
/// sets", Proc. AMS 10, 1959, <https://doi.org/10.1090/S0002-9939-1959-0105008-8>).
fn segment_box_closest(origin: Vec3, dir: Vec3, length: f32, min: Vec3, max: Vec3) -> Vec3 {
    let mut t = ((min + max) * 0.5 - origin).dot(dir).clamp(0.0, length);
    let mut q = (origin + dir * t).clamp(min, max);
    for _ in 0..24 {
        let next = (q - origin).dot(dir).clamp(0.0, length);
        q = (origin + dir * next).clamp(min, max);
        if (next - t).abs() < 1e-5 {
            break;
        }
        t = next;
    }
    q
}

/// How well a target is aimed at: how far off the crosshair's line it lies, as an angle in
/// radians (0 if the line passes through it), and how far out along the line, world units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AimScore {
    pub angle: f32,
    pub distance: f32,
}

/// How well the line from `origin` along unit `dir` aims at the box `min..max` placed by
/// `world_from_item` (rigid or evenly scaled): an exact hit within `reach`, or else the box's
/// nearest point to the line, if that is ahead, within `reach` of the eye and within `buffer`
/// of the line. `None` if it is not aimed at.
pub fn aim_score(origin: Vec3, dir: Vec3, reach: f32, buffer: AimBuffer, world_from_item: Transform, min: Vec3, max: Vec3) -> Option<AimScore> {
    let world_from_local = world_from_item.compute_affine();
    let inverse = world_from_local.inverse();
    let local_origin = inverse.transform_point3(origin);
    // Left unnormalised, so the distance along it stays in world units under a scale.
    let local_dir = inverse.transform_vector3(dir);
    if let Some(t) = ray_box(local_origin, local_dir, min, max).filter(|&t| t <= reach) {
        return Some(AimScore { angle: 0.0, distance: t });
    }
    if buffer == AimBuffer::EXACT {
        return None;
    }
    let world_per_local = local_dir.length().recip();
    let local_q = segment_box_closest(local_origin, local_dir * world_per_local, reach / world_per_local, min, max);
    let q = world_from_local.transform_point3(local_q);
    let along = (q - origin).dot(dir);
    let off = (q - origin - dir * along).length();
    if along <= 0.0 || q.distance(origin) > reach || off > buffer.radius_at(along) {
        return None;
    }
    Some(AimScore { angle: off.atan2(along), distance: along })
}

/// Which of `targets` (each a world-from-item transform, rigid or evenly scaled, and its
/// item-space bounds) the crosshair's line from `origin` along unit `dir` aims at within
/// `reach` ([`aim_score`]): the one nearest the line by angle, and of those it passes through,
/// the nearest. One rule for floor items, containers and folk alike.
pub fn nearest_aimed<T: Copy>(
    origin: Vec3,
    dir: Vec3,
    reach: f32,
    buffer: AimBuffer,
    targets: impl IntoIterator<Item = (T, Transform, Vec3, Vec3)>,
) -> Option<(T, AimScore)> {
    targets
        .into_iter()
        .filter_map(|(id, world_from_item, min, max)| aim_score(origin, dir, reach, buffer, world_from_item, min, max).map(|score| (id, score)))
        .min_by(|a, b| a.1.angle.total_cmp(&b.1.angle).then(a.1.distance.total_cmp(&b.1.distance)))
}

/// An item lying in the level, with its item-space bounds for picking. The item itself is in
/// the [`Inventory`], loose.
#[derive(Component, Debug)]
#[require(DespawnOnExit<crate::launch::AppState> = DespawnOnExit(crate::launch::AppState::Delving))]
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
    /// Its point in its own space, for a thrust (`crate::view_model::HeldInView::point`);
    /// `None` for a light held by its flame.
    pub point: Option<Vec3>,
}

/// What a [`PlayerCamera`]'s view model was last drawn for: the item in each hand, the carried
/// lights among them (so lighting or putting one out draws it again), and whose hands hold them
/// (the body they are fitted to).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldKey {
    pub main: Option<ItemId>,
    pub off: Option<ItemId>,
    /// The lit things carried in the main hand, the off hand and at the hip ([`carried_lights`]).
    pub lights: [Option<ItemId>; 3],
    /// How burnt each of those lights looks, in twentieths of a torch's char, so a held torch
    /// is fitted again about every 0.05 char as it burns down.
    pub burn: [u8; 3],
    /// The lantern hung at the hip, and the belt it hangs from.
    pub hip: Option<ItemId>,
    pub belt: Option<ItemId>,
    /// What the main hand's bow or crossbow has loaded: 1 for an arrow on the string, a bolt on
    /// the deck, +2 for a spanned crossbow, so the view model is fitted again when it changes.
    /// (The draw and a reload's span bend the model in place: `combat::shoot::bend_held`.)
    pub loaded: u8,
    pub body: (fb_body::Folk, u32),
}

impl HeldKey {
    pub fn of(inventory: &Inventory, character: &Character) -> HeldKey {
        HeldKey {
            main: inventory.worn(fb_items::Slot::Mainhand),
            off: inventory.worn(fb_items::Slot::Offhand),
            lights: {
                let lit = carried_lights(inventory);
                [fb_items::Slot::Mainhand, fb_items::Slot::Offhand, fb_items::Slot::Hip].map(|slot| lit.iter().find(|(s, _)| *s == slot).map(|(_, id)| *id))
            },
            burn: [fb_items::Slot::Mainhand, fb_items::Slot::Offhand, fb_items::Slot::Hip]
                .map(|slot| inventory.worn(slot).and_then(|id| inventory.item(id)).filter(|i| i.shape.lit() == Some(true)).map_or(0, |i| crate::inventory::char_bucket(&i.shape))),
            hip: inventory.worn(fb_items::Slot::Hip),
            belt: inventory.worn(fb_items::Slot::Belt),
            loaded: inventory.worn(fb_items::Slot::Mainhand).and_then(|id| inventory.item(id)).map_or(0, |i| match &i.shape {
                armory::Item::Bow(s) => u8::from(s.arrow.on),
                armory::Item::Crossbow(s) => u8::from(s.arrow.on) + 2 * u8::from(s.spanned),
                _ => 0,
            }),
            body: (character.folk, character.body_seed),
        }
    }
}

/// What a view model is fitted for: what the hands and hip hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldWanted {
    key: HeldKey,
}

/// Components needed to place a held item at any scale
#[derive(Clone, Debug)]
struct HeldPlacement {
    /// The position in view space where the grip should rest
    fist: Vec3,
    /// The grip point in item space
    grip: Vec3,
    /// The rotation from item space to view space
    rotation: Quat,
}

impl HeldPlacement {
    /// Compute the world translation for a given scale
    fn translation_at_scale(&self, scale: f32) -> Vec3 {
        self.fist - self.rotation * self.grip * scale
    }
}

/// One hand's item fitted and built, ready to spawn.
#[derive(Clone, Debug)]
struct BuiltHand {
    hand: fb_items::Slot,
    item: Item,
    placement: HeldPlacement,
    point: Option<Vec3>,
    mesh: armory::ItemMesh,
}

/// A view model's items fitted and built off the main thread: each hand's, and the lantern
/// at the hip with the body's fitted belt it hangs from. Shared (and kept in the
/// [`crate::fitting::FitCache`]), since what it holds depends on the items and the body, not
/// on the ids the inventory gave them.
#[derive(Clone, Debug)]
pub struct HeldFit {
    hands: Vec<BuiltHand>,
    hip: Option<(HipInView, armory::ItemMesh)>,
}

/// What a [`HeldFit`] was fitted for, with it.
#[derive(Clone, Debug)]
pub struct HeldBuilt {
    wanted: HeldWanted,
    fit: std::sync::Arc<HeldFit>,
}

impl std::ops::Deref for HeldBuilt {
    type Target = HeldFit;

    fn deref(&self) -> &HeldFit {
        &self.fit
    }
}

/// Everything [`HeldFit::fit`] depends on: the body, the items in the hands, and with a lantern
/// at the hip the whole worn kit it is fitted over (each shape as [`crate::fitting::shape_key`]).
pub fn held_key(spec: &BodySpec, worn: &[(fb_items::Slot, Item)]) -> String {
    use fb_items::Slot;
    let mut key = format!("held|{spec:?}");
    let hung = worn.iter().any(|(slot, _)| *slot == Slot::Hip);
    for (slot, item) in worn {
        if hung || matches!(slot, Slot::Mainhand | Slot::Offhand) {
            key.push('|');
            key.push_str(&format!("{slot:?}/{}", crate::fitting::shape_key(item)));
        }
    }
    key
}

impl HeldBuilt {
    /// This view model drawn for `wanted`, if only what the main hand's bow or crossbow has
    /// loaded differs ([`HeldKey::loaded`]): the same fitting with that weapon's mesh built
    /// again, which is quick, where fitting from scratch takes a while. `None` if anything
    /// else differs.
    fn with_loaded(&self, wanted: HeldWanted, inventory: &Inventory) -> Option<HeldBuilt> {
        let same_but_loaded = HeldKey { loaded: wanted.key.loaded, ..self.wanted.key } == wanted.key;
        if !same_but_loaded || self.wanted.key.loaded == wanted.key.loaded {
            return None;
        }
        let shape = inventory.item(wanted.key.main?)?.shape.clone();
        let mut fit = (*self.fit).clone();
        let hand = fit.hands.iter_mut().find(|h| h.hand == fb_items::Slot::Mainhand)?;
        hand.mesh = shape.build();
        hand.item = shape;
        Some(HeldBuilt { wanted, fit: std::sync::Arc::new(fit) })
    }

    /// Fits and builds what `wanted` holds on a body of `spec` wearing `worn` (a list that
    /// holds the items in the hands and at the hip).
    fn fit(wanted: HeldWanted, spec: &BodySpec, worn: &[(fb_items::Slot, Item)]) -> HeldBuilt {
        HeldBuilt { wanted, fit: std::sync::Arc::new(HeldFit::fit(spec, worn)) }
    }
}

impl HeldFit {
    /// Fits and builds what `wanted` holds on a body of `spec` wearing `worn` (a list that
    /// holds the items in the hands and at the hip).
    pub fn fit(spec: &BodySpec, worn: &[(fb_items::Slot, Item)]) -> HeldFit {
        use fb_items::Slot;
        let held = |slot| worn.iter().find(|(s, _)| *s == slot).map(|(_, item)| item.clone());

        // Fit hands by extracting fist, grip, and rotation from fitting (scale-independent placement)
        let hands = {
            let fitted = crate::view_model::held_in_view(spec, held(Slot::Mainhand).as_ref(), held(Slot::Offhand).as_ref(), 1.0);
            let mut result = Vec::new();

            // Weapons fitted by held_in_view
            for h in fitted {
                let hand = h.hand;
                let item = h.item.clone();
                let mesh = item.build();
                let point = Some(h.point);
                let placement = HeldPlacement { fist: h.fist, grip: h.grip, rotation: h.transform.rotation };
                result.push((hand, item, placement, point, mesh));
            }

            // Lights that don't fit through held_in_view
            for slot in [Slot::Mainhand, Slot::Offhand] {
                if result.iter().any(|(hand, _, _, _, _)| *hand == slot) {
                    continue; // Already handled as a weapon
                }
                let item = match held(slot) {
                    Some(it) => it,
                    None => continue,
                };
                let anchor = match item_light(&item) {
                    Some(a) => a,
                    None => continue, // Not a light
                };
                let (flame, rotation) = held_pose(&item, slot);
                let mesh = item.build();
                let grip = Vec3::from_array(anchor.position);
                let placement = HeldPlacement { fist: flame, grip, rotation };
                result.push((slot, item, placement, None, mesh));
            }

            result.into_iter()
                .map(|(hand, item, placement, point, mesh)| {
                    BuiltHand { hand, item, placement, point, mesh }
                })
                .collect()
        };
        let hip = hip_in_view(spec, worn, EYE_HEIGHT).map(|hip| {
            let mesh = hip.item.build();
            (hip, mesh)
        });
        HeldFit { hands, hip }
    }
}

/// On a [`PlayerCamera`]: what its view model holds, if drawn yet, and what was spawned to draw
/// it (each hand's item, and the carried light's light on the stage and in the world).
#[derive(Component, Debug, Default)]
pub struct Carrying {
    pub held: Option<HeldKey>,
    entities: Vec<Entity>,
    /// What was fitted for the view model last, kept to respawn from when only the tuning moves.
    built: Option<std::sync::Arc<HeldBuilt>>,
    /// What is being fitted off the main thread, while `built` stays on show, and the
    /// [`crate::fitting::FitCache`] key to keep it under when it finishes.
    fitting: Option<Fitting<HeldWanted, std::sync::Arc<HeldFit>>>,
    fit_key: String,
}

impl Carrying {
    /// The carried light, if the view model holds one: the first of its lights.
    pub fn light(&self) -> Option<ItemId> {
        self.held.and_then(|h| h.lights.into_iter().flatten().next())
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
    /// [`HELD_BOB`] times `bob` (the tuning's `head_bob`), then turned about the eye against the
    /// lag, so a head turning left leaves it trailing right and a head tipping up leaves it
    /// trailing low.
    pub fn transform(&self, gait: Gait, bob: f32) -> Transform {
        let bob = gait.bob() * HELD_BOB * bob;
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
#[require(DespawnOnExit<crate::launch::AppState> = DespawnOnExit(crate::launch::AppState::Delving))]
pub struct ViewModelCamera {
    /// The [`PlayerCamera`] it draws over.
    pub follows: Entity,
}

/// A [`PlayerCamera`] that already has its torch and HUD.
#[derive(Component, Debug)]
pub(crate) struct Equipped;

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
#[require(DespawnOnExit<crate::launch::AppState> = DespawnOnExit(crate::launch::AppState::Delving))]
pub struct StandInCamera;

fn no_player_camera(cameras: Query<(), With<PlayerCamera>>) -> bool {
    cameras.is_empty()
}

fn spawn_stand_in_camera(mut commands: Commands, level: Res<CurrentLevel>, tuning: Res<Tuning>) {
    warn!("no PlayerCamera after startup; spawning a stand-in at the level's start");
    commands.spawn((
        StandInCamera,
        PlayerCamera,
        Camera3d::default(),
        crate::player::world_projection(&tuning),
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn lay_out_items(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    old: Query<(Entity, &FloorItem)>,
    mut inventory: ResMut<Inventory>,
    tuning: Option<Res<Tuning>>,
    record: Option<Res<WorldRecord>>,
    mut staged: Option<ResMut<crate::staged::Staged>>,
    character: Option<Res<crate::character::Character>>,
    mut spawner: ItemSpawner,
) {
    for (entity, floor) in &old {
        inventory.forget(floor.id);
        commands.entity(entity).despawn();
    }
    let tuning = tuning.as_deref().cloned().unwrap_or_default();
    // A level that was left lays out what lay on its floor then (`crate::record`), else what it
    // generated. Once a depth is recorded its record is the truth: rolling its loot again would
    // bring back what was taken.
    if let Some(saved) = record.as_deref().and_then(|r| r.level(level.0.depth)) {
        if saved.floor.iter().map(|f| f.spot).ne(level.0.items.iter().copied()) {
            warn!("the floor of depth {} disagrees with its record; laying out the record", level.0.depth);
        }
        for lying in &saved.floor {
            if let Some(id) = restore_item(&mut inventory, &lying.item) {
                spawn_floor_item(&mut spawner, &inventory, id, lying.spot, &tuning);
            }
        }
        return;
    }
    // (A level with a record returned above: it is never staged, as its record sets its floor.)
    // The loot and its shapes were rolled ahead of time if the level was made in the background
    // (`crate::staged`).
    // It rolls for the luck the delver wears as they come to the level.
    let odds = crate::character::loot_odds(character.as_deref(), Some(&inventory), level.0.depth);
    let mut ahead = staged.as_mut().and_then(|s| s.take_items(&level.0, odds)).map(Vec::into_iter);
    let lore = level.0.lore();
    for spot in &level.0.items {
        let rolled = match ahead.as_mut() {
            Some(ahead) => ahead.next().flatten(),
            None => fb_items::roll_loot(spot.family, spot.seed, odds).map(|item| {
                let item = item.in_world(&lore);
                let mesh = item.shape.build();
                (item, mesh)
            }),
        };
        if let Some((item, mesh)) = rolled {
            let id = inventory.add(item);
            spawn_floor_item_shaped(&mut spawner, &inventory, id, mesh, *spot, &tuning);
        }
    }
}

/// The item's item-space vertices.
pub fn item_points(mesh: &armory::ItemMesh) -> Vec<Vec3> {
    mesh.parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p)).collect()
}

/// Spawns item `id` resting on the floor at `spot` as a [`FloorItem`], drawn with its own
/// palette, dressed condition and etching at the tuning's floor scale, with its light if it
/// gives one. `None` for an item with no vertices or one the inventory does not know.
pub fn spawn_floor_item(spawner: &mut ItemSpawner, inventory: &Inventory, id: ItemId, spot: ItemSpot, tuning: &Tuning) -> Option<Entity> {
    let mesh = inventory.item(id)?.shape.build();
    spawn_floor_item_shaped(spawner, inventory, id, mesh, spot, tuning)
}

/// [`spawn_floor_item`] for an item whose `mesh` is built already.
pub fn spawn_floor_item_shaped(spawner: &mut ItemSpawner, inventory: &Inventory, id: ItemId, mesh: armory::ItemMesh, spot: ItemSpot, tuning: &Tuning) -> Option<Entity> {
    let item = inventory.item(id)?;
    let points = item_points(&mesh);
    let at = Vec3::from_array(Level::world(spot.x, spot.y));
    let transform = rest_on_floor(&points, fb_items::profile::stands_on_base(&item.shape), at, spot.yaw, tuning.view.floor_item_scale)?;
    let (min, max) = bounds(points.iter().copied())?;
    let light = rolled_light(item);
    let appearance = ItemAppearance { shimmer: floor_shimmer(item, tuning), ..appearance_of(&item.shape) };
    let mut root = spawner.spawn(&mesh, &appearance);
    root.insert((transform, Name::new(item.title().to_string()), FloorItem { id, spot, min, max }));
    if let Some(anchor) = light {
        // A lit thing on the floor lights its corner, from a little above the flame so the
        // stone under it does not burn white. Its light is shadowed so it stops at the rock,
        // and the thing itself casts none, so its own flame and body do not hide its light.
        let mut flame = transform.transform_point(Vec3::from_array(anchor.position));
        flame.y = flame.y.max(FLOOR_LIGHT_HEIGHT);
        let at = transform.compute_affine().inverse().transform_point3(flame);
        root.insert(Propagate(NotShadowCaster)).with_child((given_light(anchor, Exposure::INDOOR, &tuning.light), Transform::from_translation(at)));
    }
    Some(root.id())
}

/// The shimmer an item lying on the floor is drawn with: a legendary's in the legendary colour
/// (`ui::rarity_color`) at the tuning's `legendary_shimmer`, so it catches the eye even in the
/// dark, the user's call ("a faint shimmer when they're on the ground"); none for anything
/// else. Only floor items are drawn with it, so it stops once picked up.
pub fn floor_shimmer(item: &fb_items::Item, tuning: &Tuning) -> Shimmer {
    if item.rarity == fb_items::Rarity::Legendary {
        Shimmer::new(rarity_color(fb_items::Rarity::Legendary), tuning.view.legendary_shimmer.max(0.0))
    } else {
        Shimmer::default()
    }
}

/// Redraws every floor item's shimmer at the tuning's strength ([`floor_shimmer`]).
fn reshimmer_floor_items(
    tuning: Res<Tuning>,
    inventory: Res<Inventory>,
    items: Query<(&FloorItem, &Children)>,
    parts: Query<&MeshMaterial3d<ToonMaterial>>,
    mut materials: ResMut<Assets<ToonMaterial>>,
) {
    for (floor, children) in &items {
        let Some(item) = inventory.item(floor.id) else { continue };
        let shimmer = floor_shimmer(item, &tuning);
        for part in children.iter().filter_map(|child| parts.get(child).ok()) {
            if materials.get(&part.0).is_some_and(|m| m.extension.shimmer != shimmer)
                && let Some(mut material) = materials.get_mut(&part.0)
            {
                material.extension.shimmer = shimmer;
            }
        }
    }
}

/// An item's footprint radius on the floor once laid down ([`lie_down`], standing as made if
/// it `stands`) and drawn `scale` times its size: half the larger of its level extents. 0 for
/// an item with no vertices.
pub fn floor_radius(points: &[Vec3], stands: bool, scale: f32) -> f32 {
    bounds(points.iter().copied()).map_or(0.0, |(min, max)| {
        let size = (lie_down(max - min, stands) * (max - min)).abs();
        size.x.max(size.z) * 0.5 * scale
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
    tuning: &Tuning,
) -> Option<Entity> {
    let item = inventory.item(id)?;
    // Straight down, the top of the head points the way the body faces.
    let forward = eye.forward().as_vec3().xz().try_normalize().unwrap_or_else(|| eye.up().as_vec3().xz().normalize_or(Vec2::NEG_Y));
    let radius = floor_radius(&item_points(&item.shape.build()), fb_items::profile::stands_on_base(&item.shape), tuning.view.floor_item_scale);
    let at = drop_spot(eye.translation().xz(), forward, aside, radius, blocked_by(level, blocked_cells), &obstacles.0) / CELL;
    // Laid square to the player's view.
    let yaw = (-forward.x).atan2(-forward.y);
    set_down_at(spawner, inventory, level, id, at, yaw, tuning)
}

/// Sets loose item `id` down at `at` on the floor (cells), turned `yaw`, into the level data
/// too so it stays where it was left, and spawns it there. `None` if it has no vertices.
pub fn set_down_at(spawner: &mut ItemSpawner, inventory: &Inventory, level: &mut Level, id: ItemId, at: Vec2, yaw: f32, tuning: &Tuning) -> Option<Entity> {
    let item = inventory.item(id)?;
    // For anything that came from a level spot, the spot rolls the same item again
    // ([`fb_items::roll_loot`]); what is spawned is the item itself.
    let spot = ItemSpot { x: at.x, y: at.y, yaw, family: item.shape.family(), seed: item.seed };
    level.items.push(spot);
    spawn_floor_item(spawner, inventory, id, spot, tuning)
}

/// Lifts a floor item out of the level: out of the level data, without respawning what is
/// still there, and its entity despawned. The item stays in the inventory.
pub fn lift(commands: &mut Commands, level: &mut Level, entity: Entity, floor: &FloorItem) {
    if let Some(index) = level.items.iter().position(|spot| *spot == floor.spot) {
        level.items.remove(index);
    }
    commands.entity(entity).despawn();
}

/// A flickering point light in `anchor`'s colour and flicker (times the tuning's
/// `flicker_depth`), `peak` times full close up and reaching `range` units.
fn anchor_light(anchor: &LightAnchor, peak: f32, range: f32, exposure: &Exposure, light: &LightTuning) -> PointLight {
    let color = Color::srgb(anchor.color.0, anchor.color.1, anchor.color.2);
    Torch::flickering(point_light(color, peak, range, exposure), anchor.flicker * light.flicker_depth)
}

/// An anchor's light in the world: `torch_peak` close up, bright out to its radius and dim to
/// `torch_reach` times it ([`LightTuning`]), and shadowed, so it never reaches through rock into
/// the next corridor.
pub fn world_light(anchor: &LightAnchor, exposure: &Exposure, light: &LightTuning) -> PointLight {
    cast_shadows(anchor_light(anchor, light.torch_peak, anchor.radius * light.torch_reach, exposure, light))
}

/// On a lit thing's light in the world ([`world_light`]): what it is the light of, so it can be
/// lit again when the tuning changes ([`relight_given_lights`]).
#[derive(Component, Clone, Copy)]
pub struct GivenLight {
    pub anchor: LightAnchor,
    pub exposure: Exposure,
}

/// A lit thing's light in the world, with what it is the light of.
pub fn given_light(anchor: LightAnchor, exposure: Exposure, light: &LightTuning) -> (PointLight, GivenLight) {
    (world_light(&anchor, &exposure, light), GivenLight { anchor, exposure })
}

/// Lights every lit thing's light again when the tuning changes (in hand, on the floor, held by
/// folk), and each new one as it appears, so one spawned without the tuning to hand follows it.
fn relight_given_lights(tuning: Res<Tuning>, mut lights: Query<(Ref<GivenLight>, &mut PointLight)>) {
    for (given, mut light) in &mut lights {
        if !tuning.is_changed() && !given.is_added() {
            continue;
        }
        *light = world_light(&given.anchor, &given.exposure, &tuning.light);
    }
}

/// Redraws every floor item at the tuning's floor scale, about where it rests.
fn rescale_floor_items(tuning: Res<Tuning>, mut items: Query<(&FloorItem, &mut Transform)>) {
    let scale = tuning.view.floor_item_scale;
    for (floor, mut transform) in &mut items {
        if transform.scale.x != scale {
            let at = Vec3::from_array(Level::world(floor.spot.x, floor.spot.y));
            *transform = rescale_on_floor(*transform, at, scale);
        }
    }
}

/// The view model camera's projection for `tuning`.
fn view_model_projection(tuning: &Tuning) -> Projection {
    Projection::from(PerspectiveProjection { fov: tuning.view.held_fov_degrees.to_radians(), near: 0.05, ..default() })
}

/// Keeps every view model camera's field of view to the tuning's.
fn widen_view_models(tuning: Res<Tuning>, mut cameras: Query<&mut Projection, With<ViewModelCamera>>) {
    let fov = tuning.view.held_fov_degrees.to_radians();
    for mut projection in &mut cameras {
        if let Projection::Perspective(perspective) = &mut *projection
            && perspective.fov != fov
        {
            perspective.fov = fov;
        }
    }
}

/// How a camera draws, which the view model camera copies from the player camera.
type CameraLook = (Option<&'static Exposure>, Option<&'static Tonemapping>, Option<&'static Msaa>);

/// Gives each new [`PlayerCamera`] its view model camera, on the stage, and the HUD. What it
/// carries is [`carry_light`]'s.
fn equip_player_camera(
    mut commands: Commands,
    tuning: Res<Tuning>,
    fonts: Option<Res<UiFonts>>,
    cameras: Query<(Entity, CameraLook), (With<PlayerCamera>, Without<Equipped>)>,
) {
    for (camera, (exposure, tonemapping, msaa)) in &cameras {
        let exposure = exposure.copied().unwrap_or_default();
        let view_model = commands
            .spawn((
                ViewModelCamera { follows: camera },
                crate::render::ScreenInkExempt,
                Camera3d::default(),
                Camera { order: 1, clear_color: ClearColorConfig::None, ..default() },
                view_model_projection(&tuning),
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
        spawn_hud(&mut commands, view_model, fonts.as_deref());
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

/// Where a held light's own light sits on the view model's stage, in camera space, for the
/// light held at `rest` in `hand`: off its flame toward the eye, up and in, so the whole light
/// sits at much the same distance from it and reads firelit rather than burnt white.
pub fn stage_light_at(anchor: &LightAnchor, rest: Transform, hand: fb_items::Slot) -> Vec3 {
    let scale = rest.scale.x;
    let flame = rest.transform_point(Vec3::from_array(anchor.position));
    let toward_eye = if side_of(hand) == fb_body::OFF_HAND { HELD_LIGHT_OFFSET * Vec3::new(-1.0, 1.0, 1.0) } else { HELD_LIGHT_OFFSET };
    // As far off the flame, for the item's size, as before it was drawn larger.
    flame + toward_eye * scale
}

/// What each hand holds, placed in the view model camera's space and drawn `scale` times its
/// size: each item fitted to the delver's body as on the doll and carried into the view
/// (`crate::view_model`), or, for a lit thing no hold fits, held by its flame ([`held_pose`]).
/// Other things a hand might hold that no hold fits are not drawn.
pub fn held_items(inventory: &Inventory, character: &Character, scale: f32) -> Vec<(fb_items::Slot, Item, Transform, Option<Vec3>)> {
    use fb_items::Slot;
    let shape = |slot| inventory.worn(slot).and_then(|id| inventory.item(id)).map(|item| item.shape.clone());
    held_items_of(&character.spec(), shape(Slot::Mainhand), shape(Slot::Offhand), scale)
}

/// [`held_items`] for the items in the main and off hand of a body of `spec`.
fn held_items_of(spec: &BodySpec, main: Option<Item>, off: Option<Item>, scale: f32) -> Vec<(fb_items::Slot, Item, Transform, Option<Vec3>)> {
    use fb_items::Slot;
    let fitted = held_in_view(spec, main.as_ref(), off.as_ref(), scale);
    [(Slot::Mainhand, main), (Slot::Offhand, off)]
        .into_iter()
        .filter_map(|(hand, item)| {
            if let Some(held) = fitted.iter().find(|h| h.hand == hand) {
                return Some((hand, held.item.clone(), held.transform, Some(held.point)));
            }
            let item = item?;
            let anchor = item_light(&item)?;
            let (flame, rotation) = held_pose(&item, hand);
            Some((hand, item, Transform { translation: flame - rotation * Vec3::from_array(anchor.position) * scale, rotation, scale: Vec3::splat(scale) }, None))
        })
        .collect()
}

/// Keeps each player camera's view model in step with the hands ([`held_items`]) and the
/// carried lights ([`carried_lights`]): when either changes, or the tuning does, the old view
/// model and its lights go and the new ones are spawned, each lit thing's light in the world
/// from its [`LightAnchor`], every one shadowed. With nothing lit there is no light; with
/// nothing in hand, nothing is drawn. A lantern at the hip hangs from the player's body instead ([`HipSwing`]).
///
/// Fitting the items takes long enough to hitch a frame (a lantern at the hip fits the whole
/// worn kit), so it runs off the main thread ([`Fitting`]) and the old view model stays until
/// the new one is ready. A fitting for hands that have changed since is dropped. A tuning
/// change alone respawns from what was last fitted.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn show_held(
    mut commands: Commands,
    inventory: Res<Inventory>,
    character: Res<Character>,
    tuning: Res<Tuning>,
    mut cache: ResMut<crate::fitting::FitCache>,
    mut cameras: Query<(Entity, &mut Carrying, Option<&Exposure>, Option<&ChildOf>), With<PlayerCamera>>,
    view_models: Query<(Entity, &ViewModelCamera)>,
    mut spawner: ItemSpawner,
) {
    let wanted = HeldWanted { key: HeldKey::of(&inventory, &character) };
    let scale = tuning.view.held_scale;
    for (camera, mut carrying, exposure, body) in &mut cameras {
        let Some((view_model, _)) = view_models.iter().find(|(_, v)| v.follows == camera) else { continue };
        if carrying.fitting.as_ref().is_some_and(|f| *f.key() != wanted) {
            carrying.fitting = None;
        }
        let quick = carrying.built.as_ref().filter(|b| b.wanted != wanted).and_then(|b| b.with_loaded(wanted, &inventory));
        if quick.is_some() {
            carrying.fitting = None;
        }
        let mut fresh = quick;
        if fresh.is_none() && carrying.built.as_ref().is_none_or(|b| b.wanted != wanted) && carrying.fitting.is_none() {
            let worn: Vec<(fb_items::Slot, Item)> = inventory.equipped().into_iter().map(|(slot, item)| (slot, item.shape.clone())).collect();
            let spec = character.spec();
            if wanted.key.main.is_none() && wanted.key.off.is_none() && wanted.key.hip.is_none() {
                // Nothing to fit: no wait.
                fresh = Some(HeldBuilt::fit(wanted, &spec, &worn));
            } else {
                // What was held a moment ago is kept: no job.
                let key = held_key(&spec, &worn);
                if let Some(fit) = cache.held(&key) {
                    fresh = Some(HeldBuilt { wanted, fit });
                } else {
                    carrying.fit_key = key;
                    carrying.fitting = Some(Fitting::start(wanted, move || std::sync::Arc::new(HeldFit::fit(&spec, &worn))));
                }
            }
        }
        if fresh.is_none()
            && let Some(fit) = carrying.fitting.as_mut().and_then(|f| f.poll())
        {
            cache.store_held(&carrying.fit_key, fit.clone());
            fresh = Some(HeldBuilt { wanted, fit });
        }
        if fresh.is_some() {
            carrying.fitting = None;
        } else if !tuning.is_changed() || carrying.built.is_none() {
            continue;
        }
        if let Some(fresh) = fresh {
            carrying.built = Some(std::sync::Arc::new(fresh));
        }
        let Some(built) = carrying.built.clone() else { continue };
        for entity in carrying.entities.drain(..) {
            commands.entity(entity).try_despawn();
        }
        carrying.held = Some(built.wanted.key);
        let exposure = exposure.copied().unwrap_or_default();
        let view_layer = RenderLayers::layer(VIEW_MODEL_LAYER);
        let lit: Vec<fb_items::Slot> = carried_lights(&inventory).into_iter().map(|(slot, _)| slot).collect();
        for hand in &built.hands {
            let mut root = spawner.spawn(&hand.mesh, &appearance_of(&hand.item));
            let transform = Transform {
                translation: hand.placement.translation_at_scale(scale),
                rotation: hand.placement.rotation,
                scale: Vec3::splat(scale),
            };
            root.insert((
                Name::new(format!("Held {}", hand.item.family().key())),
                HeldItem { item: hand.item.clone(), hand: hand.hand, point: hand.point },
                crate::render::flame::FlameUp::default(),
                crate::render::flame::FlameFacing(Vec3::NEG_Z),
                HeldSway::new(transform),
                transform,
                Propagate(view_layer.clone()),
                ChildOf(view_model),
            ));
            carrying.entities.push(root.id());
            if !lit.contains(&hand.hand) {
                continue;
            }
            let Some(anchor) = item_light(&hand.item) else { continue };
            let at = stage_light_at(&anchor, transform, hand.hand);
            let stage_light = commands.spawn((anchor_light(&anchor, HELD_PEAK, HELD_REACH, &exposure, &tuning.light), Transform::from_translation(at), view_layer.clone(), ChildOf(view_model))).id();
            let reach = LightAnchor { radius: anchor.radius * light_factor_in(&inventory, hand.hand), ..anchor };
            let world = commands.spawn((given_light(reach, exposure, &tuning.light), Transform::from_translation(WORLD_FLAME), ChildOf(camera))).id();
            carrying.entities.extend([stage_light, world]);
        }
        // A lantern at the hip hangs from the body in the level, not on the stage; lit and
        // carried, its light rises from it, and a light of its own on the stage lights what the
        // hands hold from below.
        if let Some((hip, mesh)) = &built.hip {
            let (parent, hook) = match body {
                Some(body) => (body.parent(), hip.hook),
                None => (camera, hip.hook - Vec3::Y * EYE_HEIGHT),
            };
            let pivot = commands.spawn((Name::new("Hip lantern"), HipSwing, Transform::from_translation(hook), Visibility::default(), ChildOf(parent))).id();
            let carried = lit.contains(&fb_items::Slot::Hip).then(|| item_light(&hip.item)).flatten();
            // The lantern and its light go through the spawner's own commands, after the pivot's.
            let mut lantern = spawner.spawn(mesh, &appearance_of(&hip.item));
            lantern.insert((hip.transform, Propagate(NotShadowCaster), ChildOf(pivot)));
            if let Some(anchor) = carried {
                let reach = LightAnchor { radius: anchor.radius * light_factor_in(&inventory, fb_items::Slot::Hip), ..anchor };
                lantern.with_child((given_light(reach, exposure, &tuning.light),Transform::from_translation(Vec3::from_array(anchor.position))));
            }
            carrying.entities.push(pivot);
            let Some(anchor) = carried else { continue };
            let stage_light = commands.spawn((anchor_light(&anchor, HELD_PEAK, HELD_REACH, &exposure, &tuning.light), Transform::from_translation(HIP_STAGE_LIGHT), view_layer, ChildOf(view_model))).id();
            carrying.entities.push(stage_light);
        }
    }
}

/// The pivot a hip lantern hangs from ([`crate::view_model::HipInView`]), at its hook.
#[derive(Component, Debug)]
pub struct HipSwing;

/// Swings each hip lantern about its hook with the step of the body it hangs from
/// ([`hip_swing`]).
fn swing_hip(mut pivots: Query<(&mut Transform, &ChildOf), With<HipSwing>>, gaits: Query<&Gait>) {
    for (mut transform, parent) in &mut pivots {
        let gait = gaits.get(parent.parent()).copied().unwrap_or_default();
        let swing = hip_swing(gait.bob());
        if transform.rotation != swing {
            transform.rotation = swing;
        }
    }
}

/// The HUD over the level: the crosshair. Hidden while a screen is open.
#[derive(Component, Debug)]
#[require(DespawnOnExit<crate::launch::AppState> = DespawnOnExit(crate::launch::AppState::Delving))]
pub struct Hud;

pub(crate) fn hide_hud_under_screens(open: Res<ScreenOpen>, mut huds: Query<&mut Visibility, With<Hud>>) {
    let shown = if open.0.is_some() { Visibility::Hidden } else { Visibility::Inherited };
    for mut visibility in &mut huds {
        visibility.set_if_neq(shown);
    }
}

fn spawn_hud(commands: &mut Commands, camera: Entity, fonts: Option<&UiFonts>) {
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
            // The toast (`crate::inventory::Toast`): a line near the foot of the view, clear of
            // the card under the crosshair, saying what F equipped or why it could not. Cream
            // with a dark shadow so it reads on lit stone and in the dark alike. It sits just above
            // the orb cluster and quick bar, so the two never overlap at any window size.
            hud.spawn((
                Node { position_type: PositionType::Absolute, width: percent(100), bottom: crate::ui::hud(toast_lift()), justify_content: JustifyContent::Center, ..default() },
                Pickable::IGNORE,
            ))
            .with_children(|row| {
                let font = fonts.map_or_else(TextFont::default, |f| TextFont::from_font_size(20.0).with_font(f.body.clone()));
                row.spawn((ToastText, Text::default(), font, TextColor(BRIGHT), TextShadow { offset: Vec2::new(1.5, 1.5), color: BG.with_alpha(0.9) }));
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

/// Bobs what is held with the player's steps and trails it behind turns of the head.
/// Keeps the flames of what is held rising as fire does in the level: on the view model's
/// stage the world's up is where the eye's look puts it, so looking down tips a flame toward
/// the eye and looking up away from it.
fn upright_held_flames(
    view_models: Query<&ViewModelCamera>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    mut held: Query<(&HeldItem, &ChildOf, &mut crate::render::flame::FlameUp, &mut crate::render::flame::FlameFacing)>,
) {
    for (item, parent, mut up, mut facing) in &mut held {
        let Ok(view_model) = view_models.get(parent.parent()) else { continue };
        let Ok(eye) = eyes.get(view_model.follows) else { continue };
        let world_up = eye.rotation().inverse() * Vec3::Y;
        if up.0 != world_up {
            up.0 = world_up;
        }
        // A flame trails back toward the delver and out toward its hand's screen edge: against
        // the horizontal of where the eye looks, blended with that side. Looking straight up
        // or down leaves it where it was.
        let look = eye.forward().as_vec3();
        let ahead = Vec3::new(look.x, 0.0, look.z);
        let side = eye.right().as_vec3();
        let right = Vec3::new(side.x, 0.0, side.z);
        if ahead.length() > 1e-3 && right.length() > 1e-3 {
            let outward = if item.hand == fb_items::Slot::Offhand { -1.0 } else { 1.0 };
            let ahead = eye.rotation().inverse() * crate::render::flame::held_facing(ahead.normalize(), right.normalize(), outward);
            if facing.0 != ahead {
                facing.0 = ahead;
            }
        }
    }
}

pub(crate) fn sway_held(
    time: Res<Time>,
    tuning: Res<Tuning>,
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
        *transform = sway.transform(gait, tuning.view.head_bob);
    }
}

pub(crate) fn aim(
    tuning: Res<Tuning>,
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
    let hit = nearest_aimed(eye.translation(), eye.forward().as_vec3(), REACH, AimBuffer::of(&tuning), targets).map(|(entity, _)| entity);
    if aimed.0 != hit {
        aimed.0 = hit;
    }
}

/// What `slot` does to the floor item `id`, from the Interactable lookup ([`crate::interact`]).
fn floor_verb(inventory: &Inventory, id: ItemId, slot: crate::interact::Slot) -> Option<crate::interact::Verb> {
    let target = crate::interact::item_target(inventory, id)?;
    crate::interact::verbs_for(&target, &crate::interact::Delver::default()).get(slot)
}

/// Primary takes the item under the crosshair if its verb is Take ([`Inventory::pick_up`]).
/// Something with no room for it stays where it lies.
pub(crate) fn pick_up(
    mut commands: Commands,
    input: ActionInput,
    mut aimed: ResMut<Aimed>,
    items: Query<&FloorItem>,
    mut inventory: ResMut<Inventory>,
    mut level: ResMut<CurrentLevel>,
) {
    if !input.just_pressed(Action::Primary) {
        return;
    }
    let Some(entity) = aimed.0 else { return };
    let Ok(floor) = items.get(entity) else { return };
    if floor_verb(&inventory, floor.id, crate::interact::Slot::Primary) != Some(crate::interact::Verb::Take) {
        return;
    }
    match inventory.pick_up(floor.id) {
        Ok(()) => {
            aimed.0 = None;
            lift(&mut commands, &mut level.bypass_change_detection().0, entity, floor);
        }
        Err(refusal) => info!("{}", refusal.reason()),
    }
}

/// Secondary, if its verb is Equip, equips the item under the crosshair straight from the floor, without the inventory
/// screen ([`equip_from_floor`]): into its slot, a hand or a free ring slot, swapping out what
/// was there into a bag, or onto the floor where the new item lay. Special, if its verb is Hang
/// at hip, does the same into the hip ([`crate::inventory::equip_from_floor_in`]). The toast says
/// what happened, or why not.
#[allow(clippy::too_many_arguments)]
pub(crate) fn equip_aimed(
    input: ActionInput,
    mut aimed: ResMut<Aimed>,
    items: Query<&FloorItem>,
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
) {
    use crate::interact::{Slot as Act, Verb};
    let hang = input.just_pressed(Action::Special);
    if !hang && !input.just_pressed(Action::Secondary) {
        return;
    }
    let Some(floor) = aimed.0.and_then(|entity| items.get(entity).ok()) else { return };
    let (act, verb) = if hang { (Act::Special, Verb::HangAtHip) } else { (Act::Secondary, Verb::Equip) };
    if floor_verb(&inventory, floor.id, act) != Some(verb) {
        return;
    }
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    let (at, yaw) = (Vec2::new(floor.spot.x, floor.spot.y), floor.spot.yaw);
    let equipped = if hang { equip_from_floor_in(&mut inventory, floor.id, fb_items::Slot::Hip, at, yaw) } else { equip_from_floor(&mut inventory, floor.id, at, yaw) };
    match equipped {
        Ok(equipped) => {
            aimed.0 = None;
            toast.say(equipped.message(&inventory, floor.id), now);
            changes.0.extend(equipped.changes);
        }
        Err(refusal) => toast.say(refusal.reason(), now),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_items::Rarity;

    /// The floor scale as shipped.
    fn floor_scale() -> f32 {
        Tuning::default().view.floor_item_scale
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn items_lie_on_their_largest_face_unless_they_stand_on_a_base() {
        // A sword: tall, wide at the guard, thin through the blade.
        let sword = lie_down(Vec3::new(2.5, 9.0, 0.4), false);
        let size = (sword * Vec3::new(2.5, 9.0, 0.4)).abs();
        assert!(close(size.y, 0.4), "{size}");
        // A spear thin both ways still lies flat.
        let spear = (lie_down(Vec3::new(0.5, 20.0, 0.6), false) * Vec3::new(0.5, 20.0, 0.6)).abs();
        assert!(spear.y < 1.0, "{spear}");
        // A crossbow, as wide across its prod as it is long: flat, not stood on its stock.
        let crossbow = (lie_down(Vec3::new(7.0, 7.5, 1.2), false) * Vec3::new(7.0, 7.5, 1.2)).abs();
        assert!(close(crossbow.y, 1.2), "{crossbow}");
        // A shield lies on its back, face up.
        assert!((lie_down(Vec3::new(6.0, 6.5, 1.0), false) * Vec3::Z).y > 0.99);
        // Something already flat stays as it is; a bag stands as made.
        assert_eq!(lie_down(Vec3::new(3.0, 0.5, 2.0), false), Quat::IDENTITY);
        assert_eq!(lie_down(Vec3::new(3.0, 4.0, 3.0), true), Quat::IDENTITY);
    }

    /// The floor extents of `item` laid down as the game lays it, and its transform.
    fn laid_size(item: &fb_items::Item) -> (Vec3, Transform) {
        let points = item_points(&item.shape.build());
        let at = Vec3::new(30.0, 0.0, 30.0);
        let transform = rest_on_floor(&points, fb_items::profile::stands_on_base(&item.shape), at, 0.4, floor_scale()).unwrap();
        let (min, max) = bounds(points.iter().map(|&p| transform.transform_point(p))).unwrap();
        assert!(close(min.y, 0.0), "{}: floats or sinks to {}", item.title(), min.y);
        (max - min, transform)
    }

    #[test]
    fn crossbows_bows_and_shields_rest_flat_and_bags_stand() {
        for seed in 1..=16u32 {
            for family in [Family::Crossbow, Family::Bow, Family::Shield] {
                let item = fb_items::roll_loot(family, seed, fb_items::Odds::default()).unwrap();
                let (size, _) = laid_size(&item);
                assert!(size.y <= size.x.min(size.z) + 1e-3, "{family:?} {seed}: stands {size}");
            }
            for family in [Family::Bags, Family::Feet, Family::Lantern] {
                let item = fb_items::roll_loot(family, seed, fb_items::Odds::default()).unwrap();
                let (_, transform) = laid_size(&item);
                assert!((transform.rotation * Vec3::Y).y > 0.99, "{family:?} {seed}: laid down");
            }
        }
    }

    #[test]
    fn items_rest_on_the_floor_at_their_spot() {
        let box_points: Vec<Vec3> = (0..8)
            .map(|i| Vec3::new(if i & 1 == 0 { -1.0 } else { 1.5 }, if i & 2 == 0 { -2.0 } else { 7.0 }, if i & 4 == 0 { -0.2 } else { 0.3 }))
            .collect();
        let at = Vec3::new(40.0, 0.0, 60.0);
        let transform = rest_on_floor(&box_points, false, at, 0.7, floor_scale()).unwrap();
        let placed: Vec<Vec3> = box_points.iter().map(|&p| transform.transform_point(p)).collect();
        let (min, max) = bounds(placed.into_iter()).unwrap();
        assert!(close(min.y, 0.0), "{min}");
        assert!(max.y < 1.0, "a long thing lies down: {max}");
        assert!(close((min.x + max.x) * 0.5, at.x) && close((min.z + max.z) * 0.5, at.z));
        assert!(rest_on_floor(&[], false, at, 0.0, floor_scale()).is_none());
        // Drawn larger than its own size, still resting on the floor at its spot: the box's
        // thin side (0.5 deep) lies down, that much times the scale high.
        assert_eq!(transform.scale, Vec3::splat(floor_scale()));
        assert!(close(max.y - min.y, 0.5 * floor_scale()), "{min} {max}");
        assert!(close(floor_radius(&box_points, false, floor_scale()), 4.5 * floor_scale()));
        // Drawn at another scale it still rests at its spot, as if it had been laid there so.
        let twice = rescale_on_floor(transform, at, 2.0);
        let laid = rest_on_floor(&box_points, false, at, 0.7, 2.0).unwrap();
        assert!(twice.translation.abs_diff_eq(laid.translation, 1e-4) && twice.scale == laid.scale, "{twice:?}");
        // The pickup box grows with it, and a ray onto its top lands there, in world units.
        let top = Vec3::new((min.x + max.x) * 0.5, max.y, (min.z + max.z) * 0.5);
        let from = top + Vec3::Y * 5.0;
        let hit = nearest_aimed(from, Vec3::NEG_Y, REACH, AimBuffer::EXACT, [((), transform, Vec3::new(-1.0, -2.0, -0.2), Vec3::new(1.5, 7.0, 0.3))]);
        assert!(hit.is_some_and(|(_, s)| (s.distance - 5.0).abs() < 1e-3 && s.angle == 0.0), "{hit:?}");
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
        let buffer = AimBuffer::of(&Tuning::default());
        assert_eq!(nearest_aimed(Vec3::ZERO, Vec3::NEG_Z, REACH, buffer, targets).map(|h| h.0), Some(2));
        assert_eq!(nearest_aimed(Vec3::ZERO, Vec3::NEG_Z, REACH, buffer, [targets[2]]), None);
    }

    /// A ring resting on the floor `at`, as it is laid out: its world transform and bounds.
    fn ring_on_floor(at: Vec3) -> (Transform, Vec3, Vec3) {
        let ring = fb_items::roll_item("Ring", 2).unwrap();
        let points = item_points(&ring.shape.build());
        let (min, max) = bounds(points.iter().copied()).unwrap();
        (rest_on_floor(&points, fb_items::profile::stands_on_base(&ring.shape), at, 0.0, floor_scale()).unwrap(), min, max)
    }

    /// The direction from `eye` to `target`, turned `degrees` to the right about +Y.
    fn off_by(eye: Vec3, target: Vec3, degrees: f32) -> Vec3 {
        Quat::from_rotation_y(-degrees.to_radians()) * (target - eye).normalize()
    }

    #[test]
    fn a_ring_a_few_pixels_off_the_crosshair_is_aimed_at() {
        let eye = Vec3::new(0.0, EYE_HEIGHT, 0.0);
        let (rest, min, max) = ring_on_floor(Vec3::new(0.0, 0.0, -10.0));
        let centre = rest.transform_point((min + max) * 0.5);
        let buffer = AimBuffer::of(&Tuning::default());
        // Looking straight at it, it is aimed at either way.
        assert!(nearest_aimed(eye, (centre - eye).normalize(), REACH, AimBuffer::EXACT, [((), rest, min, max)]).is_some());
        // A degree and a half off is about twenty pixels on a 1080-pixel, 70-degree view: past
        // the ring's edge, so an exact ray misses it, but the buffer still catches it.
        let dir = off_by(eye, centre, 1.5);
        assert_eq!(nearest_aimed(eye, dir, REACH, AimBuffer::EXACT, [((), rest, min, max)]), None, "the ray itself misses the ring");
        let caught = nearest_aimed(eye, dir, REACH, buffer, [((), rest, min, max)]);
        assert!(caught.is_some_and(|(_, s)| s.angle > 0.0), "{caught:?}");
        // Well off to the side, it is not.
        assert_eq!(nearest_aimed(eye, off_by(eye, centre, 12.0), REACH, buffer, [((), rest, min, max)]), None);
    }

    #[test]
    fn of_two_items_side_by_side_the_nearer_the_crosshair_is_aimed_at() {
        let eye = Vec3::new(0.0, EYE_HEIGHT, 0.0);
        let left = ring_on_floor(Vec3::new(-1.2, 0.0, -10.0));
        let right = ring_on_floor(Vec3::new(1.2, 0.0, -10.0));
        let targets = [("left", left.0, left.1, left.2), ("right", right.0, right.1, right.2)];
        let between = Vec3::new(0.0, 0.3, -10.0);
        let buffer = AimBuffer::of(&Tuning::default());
        // Straight between them, both are off the line and neither is near enough.
        assert_eq!(nearest_aimed(eye, (between - eye).normalize(), REACH, buffer, targets), None);
        // Nudged toward one, that one; nudged toward the other, the other.
        let toward = |x: f32| (Vec3::new(x, 0.3, -10.0) - eye).normalize();
        assert_eq!(nearest_aimed(eye, toward(0.75), REACH, buffer, targets).map(|h| h.0), Some("right"));
        assert_eq!(nearest_aimed(eye, toward(-0.75), REACH, buffer, targets).map(|h| h.0), Some("left"));
        // One the line passes through beats one merely near it.
        let big = (Transform::from_xyz(0.9, 0.0, -10.0), Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.0, 0.5));
        let with_big = [("right", right.0, right.1, right.2), ("big", big.0, big.1, big.2)];
        assert_eq!(nearest_aimed(eye, toward(0.75), REACH, buffer, with_big).map(|h| h.0), Some("big"));
    }

    #[test]
    fn nothing_beyond_reach_is_aimed_at() {
        let eye = Vec3::new(0.0, EYE_HEIGHT, 0.0);
        let buffer = AimBuffer::of(&Tuning::default());
        for z in [-30.0, -21.0] {
            let (rest, min, max) = ring_on_floor(Vec3::new(0.0, 0.0, z));
            let centre = rest.transform_point((min + max) * 0.5);
            for degrees in [0.0, 0.5, 1.5] {
                assert_eq!(nearest_aimed(eye, off_by(eye, centre, degrees), REACH, buffer, [((), rest, min, max)]), None, "{z} {degrees}");
            }
        }
        // Nor anything behind the eye, however near.
        let (rest, min, max) = ring_on_floor(Vec3::new(0.0, 0.0, 3.0));
        assert_eq!(nearest_aimed(eye, Vec3::NEG_Z, REACH, buffer, [((), rest, min, max)]), None);
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

    /// Wait for held items fitting to complete by forcing it to execute.
    fn wait_for_held_fitting(app: &mut App) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            app.update();
            let mut query = app.world_mut().query::<&Carrying>();
            let carrying = query.iter(app.world()).next().unwrap();
            if carrying.built.is_some() && carrying.fitting.is_none() {
                return;
            }
            if std::time::Instant::now() > deadline {
                panic!("Held items fitting did not complete within 60 seconds");
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
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
            .init_resource::<Tuning>()
            .init_resource::<crate::fitting::FitCache>()
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
        wait_for_held_fitting(&mut app);
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
        wait_for_held_fitting(&mut app);
        assert_eq!(held(&mut app).iter().map(|h| (h.0, h.1)).collect::<Vec<_>>(), vec![(Slot::Mainhand, Family::Sword), (Slot::Offhand, Family::Torch)]);
        // Empty hands hold nothing, and nothing lights the level.
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            for slot in [Slot::Mainhand, Slot::Offhand] {
                let id = inventory.worn(slot).unwrap();
                inventory.take_out(id);
            }
        }
        wait_for_held_fitting(&mut app);
        assert!(held(&mut app).is_empty());
        let mut lights = app.world_mut().query::<&PointLight>();
        assert_eq!(lights.iter(app.world()).count(), 0);
    }

    #[test]
    fn re_equipping_a_sword_held_a_moment_ago_starts_no_fitting() {
        use fb_items::Slot;
        let mut app = App::new();
        app.init_resource::<Inventory>()
            .init_resource::<Character>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
            .init_resource::<crate::render::OutlineMaterials>()
            .init_resource::<Tuning>()
            .init_resource::<crate::fitting::FitCache>()
            .add_systems(Update, show_held);
        let camera = app.world_mut().spawn((PlayerCamera, Carrying::default())).id();
        let view_model = app.world_mut().spawn(ViewModelCamera { follows: camera }).id();
        let (sword, mace) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let sword = inventory.add(fb_items::roll_item("Sword", 3).unwrap());
            let mace = inventory.add((1..400).filter_map(|s| fb_items::roll_item("Mace", s)).find(|i| i.shape.family() == Family::Mace).unwrap());
            inventory.equip(sword, Slot::Mainhand).unwrap();
            (sword, mace)
        };
        let drawn = |app: &mut App| {
            let mut query = app.world_mut().query::<(&HeldItem, &ChildOf)>();
            query.iter(app.world()).filter(|(_, c)| c.parent() == view_model).map(|(h, _)| h.item.family()).collect::<Vec<_>>()
        };
        wait_for_held_fitting(&mut app);
        assert_eq!(drawn(&mut app), vec![Family::Sword]);
        app.world_mut().resource_mut::<Inventory>().equip(mace, Slot::Mainhand).unwrap();
        wait_for_held_fitting(&mut app);
        assert_eq!(drawn(&mut app), vec![Family::Mace]);
        let misses = app.world().resource::<crate::fitting::FitCache>().misses;

        // The sword again: drawn on the next frame, from the cache, with no job.
        app.world_mut().resource_mut::<Inventory>().equip(sword, Slot::Mainhand).unwrap();
        app.update();
        let carrying = app.world().get::<Carrying>(camera).unwrap();
        assert!(carrying.fitting.is_none(), "no fitting job started");
        let cache = app.world().resource::<crate::fitting::FitCache>();
        assert_eq!((cache.misses, cache.hits), (misses, 1));
        assert_eq!(drawn(&mut app), vec![Family::Sword]);
    }

    #[test]
    fn a_cached_held_kit_is_the_one_a_fresh_fit_makes() {
        let spec = Character::default().spec();
        let worn = vec![(fb_items::Slot::Mainhand, fb_items::roll_item("Sword", 3).unwrap().shape), (fb_items::Slot::Offhand, lit_torch(5).shape)];
        let key = held_key(&spec, &worn);
        assert_eq!(key, held_key(&spec, &worn));
        let mut cache = crate::fitting::FitCache::default();
        cache.store_held(&key, std::sync::Arc::new(HeldFit::fit(&spec, &worn)));
        let cached = cache.held(&key).unwrap();
        assert_eq!(format!("{:?}", *cached), format!("{:?}", HeldFit::fit(&spec, &worn)));
    }

    #[test]
    fn a_held_kit_whose_item_state_changed_has_another_key() {
        let spec = Character::default().spec();
        let mut torch = lit_torch(5);
        let before = held_key(&spec, &[(fb_items::Slot::Offhand, torch.shape.clone())]);
        if let armory::Item::Torch(s) = &mut torch.shape {
            s.burn.char += if s.burn.char > 0.5 { -0.1 } else { 0.1 };
        }
        assert_ne!(held_key(&spec, &[(fb_items::Slot::Offhand, torch.shape)]), before, "a torch burning down");
        let mut sword = fb_items::roll_item("Sword", 3).unwrap();
        let before = held_key(&spec, &[(fb_items::Slot::Mainhand, sword.shape.clone())]);
        let mut condition = sword.shape.condition();
        condition.wear += if condition.wear > 0.5 { -0.1 } else { 0.1 };
        sword.shape.set_condition(condition);
        assert_ne!(held_key(&spec, &[(fb_items::Slot::Mainhand, sword.shape)]), before, "a sword worn");
    }

    #[test]
    fn every_lit_carried_light_shines_and_putting_one_out_takes_its_light() {
        use fb_items::Slot;
        let mut app = App::new();
        app.init_resource::<Inventory>()
            .init_resource::<Character>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
            .init_resource::<crate::render::OutlineMaterials>()
            .init_resource::<Tuning>()
            .init_resource::<crate::fitting::FitCache>()
            .add_systems(Update, show_held);
        let body = app.world_mut().spawn(Transform::default()).id();
        let camera = app.world_mut().spawn((PlayerCamera, Carrying::default(), ChildOf(body))).id();
        app.world_mut().spawn(ViewModelCamera { follows: camera });
        let (torch, lantern) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let belt = inventory.add(fb_items::roll_item("Belt", 1).unwrap());
            inventory.wear(belt, Slot::Belt).unwrap();
            let lantern = inventory.add((1..500).filter_map(|s| fb_items::roll_item("Lantern", s)).find(|i| item_light(&i.shape).is_some()).unwrap());
            inventory.wear(lantern, Slot::Hip).unwrap();
            let torch = inventory.add(lit_torch(5));
            inventory.wear(torch, Slot::Offhand).unwrap();
            (torch, lantern)
        };
        let world_lights = |app: &mut App| app.world_mut().query::<&GivenLight>().iter(app.world()).count();
        wait_for_held_fitting(&mut app);
        assert_eq!(world_lights(&mut app), 2, "a lit torch in hand and a lit lantern at the hip: two lights");
        let character = Character::default();
        let derived = character.derive(app.world().resource::<Inventory>());
        let inventory = app.world().resource::<Inventory>();
        let reach = |id| inventory.item(id).unwrap().profile.stats.light;
        assert_eq!(derived.light, reach(torch).max(reach(lantern)), "the header shows the larger");

        // The hip lantern put out gives none; the torch still does.
        app.world_mut().resource_mut::<Inventory>().toggle_light(lantern).unwrap();
        wait_for_held_fitting(&mut app);
        assert_eq!(world_lights(&mut app), 1);
        // The torch put out: dark, and the torch drawn doused.
        app.world_mut().resource_mut::<Inventory>().toggle_light(torch).unwrap();
        wait_for_held_fitting(&mut app);
        assert_eq!(world_lights(&mut app), 0);
        let mut held = app.world_mut().query::<&HeldItem>();
        assert!(held.iter(app.world()).any(|h| h.item.family() == Family::Torch && h.item.lit() == Some(false)));
        // Lit again from its card, the lantern's light comes back.
        app.world_mut().resource_mut::<Inventory>().toggle_light(lantern).unwrap();
        wait_for_held_fitting(&mut app);
        assert_eq!(world_lights(&mut app), 1);
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

    #[test]
    fn a_held_lights_stage_light_follows_its_flame_and_its_world_light_stays_by_the_eye() {
        use fb_body::{BodySpec, Folk};
        let view = Tuning::default().view;
        let on_screen = |p| crate::view_model::on_screen(p, view.held_fov_degrees);
        use fb_items::Slot;
        let lantern = (1..500).filter_map(|s| fb_items::roll_item("Lantern", s)).find(|i| item_light(&i.shape).is_some()).unwrap().shape;
        for folk in [Folk::Human, Folk::Gnome] {
            let spec = BodySpec::roll(folk, 1207);
            for light in [lit_torch(4).shape, lantern.clone()] {
                for hand in [Slot::Mainhand, Slot::Offhand] {
                    let (main, off) = if hand == Slot::Mainhand { (Some(&light), None) } else { (None, Some(&light)) };
                    let held = &held_in_view(&spec, main, off, view.held_scale)[0];
                    let anchor = item_light(&light).unwrap();
                    let flame = held.transform.transform_point(Vec3::from_array(anchor.position));
                    let at = stage_light_at(&anchor, held.transform, hand);
                    let label = format!("{folk:?} {:?} {hand:?}", light.family());
                    // The stage light keeps its place off the drawn flame, up and toward the eye
                    // and to the inside of the view, where the world light is.
                    let side = if hand == Slot::Offhand { -1.0 } else { 1.0 };
                    assert!((at - flame).abs_diff_eq(HELD_LIGHT_OFFSET * Vec3::new(side, 1.0, 1.0) * view.held_scale, 1e-4), "{label}");
                    assert!(at.x * side < flame.x * side && at.y > flame.y && at.z > flame.z, "{label}");
                    // The flame shows on its hand's side of the view, in it.
                    let screen = on_screen(flame).unwrap();
                    assert!(screen.x * side > 0.0 && screen.y.abs() < 1.0, "{label}: flame at {screen}");
                    // The whole held light is within the stage light's reach.
                    for p in held.item.build().parts.iter().flat_map(|p| p.positions.clone()) {
                        assert!(held.transform.transform_point(Vec3::from_array(p)).distance(at) < HELD_REACH, "{label}");
                    }
                }
            }
        }
        // The world light hangs by the eye whatever the view model's field of view.
        assert_eq!(WORLD_FLAME, Vec3::new(1.5, 0.0, -1.5));
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
        assert_eq!(sway.transform(Gait { phase: 1.1, pace: 0.0 }, 1.0), rest());
    }

    #[test]
    fn the_held_torch_moves_with_the_bob_while_walking() {
        let sway = HeldSway::new(rest());
        let step = |phase: f32| sway.transform(Gait { phase, pace: 1.0 }, 1.0).translation;
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
        let turning = sway.transform(Gait::default(), 1.0).translation;
        assert!(turning.x > HELD_FLAME.x + 0.2, "trails right: {turning}");
        assert!(turning.x < HELD_FLAME.x + 2.0, "only slightly: {turning}");
        // The look stops; the torch comes back to rest.
        for _ in 0..120 {
            sway.follow(Vec2::new(29.0 * dt, 0.0), dt);
        }
        assert!(sway.transform(Gait::default(), 1.0).translation.abs_diff_eq(HELD_FLAME, 1e-3));
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
        let tuning = LightTuning::default();
        let light = world_light(&anchor, &Exposure::INDOOR, &tuning);
        assert!(light.shadow_maps_enabled);
        assert_eq!(light.range, anchor.radius * tuning.torch_reach);
    }

    #[test]
    fn the_debug_pose_is_the_start_or_where_asked() {
        let level = fb_level::generate(1207, 1);
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
        let mut level = fb_level::generate(1207, 1);
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
            .init_resource::<Tuning>()
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
                    set_down(&mut spawner, &inventory, &mut level.0, &blocked, &Obstacles::default(), &GlobalTransform::from(eye), id, 0.0, &Tuning::default());
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
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyF);
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
        // F picks nothing up: the chest is never carried off (`crate::props` opens it).
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyF);
        app.update();
        assert!(app.world().get_entity(chest).is_ok());
        assert_eq!(app.world().resource::<Inventory>().location(id), Some(crate::inventory::Location::Loose));
    }

    /// [`delving_app`] with F equipping what is aimed at, and the level's floor changes made.
    fn equipping_app() -> (App, ItemId) {
        let (mut app, pack) = delving_app();
        app.init_resource::<Toast>().init_resource::<FloorChanges>().add_systems(Update, (equip_aimed, apply_floor_changes).chain().after(pick_up));
        (app, pack)
    }

    /// Lays `item` on the floor a metre ahead of an eye looking down at it, as the level does,
    /// and aims at it. Returns its entity, id and spot.
    fn lay_ahead(app: &mut App, item: fb_items::Item) -> (Entity, ItemId, ItemSpot) {
        use bevy::ecs::system::RunSystemOnce;
        let id = app.world_mut().resource_mut::<Inventory>().add(item.clone());
        let spot = ItemSpot { x: 0.0, y: -0.5, yaw: 0.3, family: item.shape.family(), seed: item.seed };
        app.world_mut().resource_mut::<CurrentLevel>().0.items.push(spot);
        let entity = app
            .world_mut()
            .run_system_once(move |mut spawner: ItemSpawner, inventory: Res<Inventory>| spawn_floor_item(&mut spawner, &inventory, id, spot, &Tuning::default()).unwrap())
            .unwrap();
        let eye = Transform::from_xyz(0.0, EYE_HEIGHT, 0.0).looking_at(Vec3::new(0.0, 0.3, -10.0), Vec3::Y);
        app.world_mut().spawn((PlayerCamera, Equipped, eye));
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Aimed>().0, Some(entity));
        (entity, id, spot)
    }

    fn press_g(app: &mut App) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::KeyG);
        keys.clear();
        keys.press(KeyCode::KeyG);
        app.update();
    }

    #[test]
    fn f_puts_a_sword_in_the_main_hand_and_the_old_weapon_in_the_pack() {
        use fb_items::Slot;
        let (mut app, pack) = equipping_app();
        let mace = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let mace = inventory.add((1..400).filter_map(|s| fb_items::roll_item("Mace", s)).find(|i| i.profile.packable).unwrap());
            inventory.wear(mace, Slot::Mainhand).unwrap();
            // The off hand is full too, so the sword swaps out the main hand's mace.
            let torch = inventory.add(lit_torch(2));
            inventory.wear(torch, Slot::Offhand).unwrap();
            mace
        };
        let character = Character::default();
        let before = HeldKey::of(app.world().resource::<Inventory>(), &character);
        let (entity, sword, _) = lay_ahead(&mut app, fb_items::roll_item("Sword", 3).unwrap());
        press_g(&mut app);
        let inventory = app.world().resource::<Inventory>();
        assert_eq!(inventory.worn(Slot::Mainhand), Some(sword));
        assert!(matches!(inventory.location(mace), Some(crate::inventory::Location::In { container, .. }) if container == pack));
        // Lifted from the level, and the view model's key changes, so the view holds the sword.
        assert!(app.world().get_entity(entity).is_err());
        assert!(app.world().resource::<CurrentLevel>().0.items.is_empty());
        let after = HeldKey::of(inventory, &character);
        assert_eq!((before.main, after.main), (Some(mace), Some(sword)));
        let toast = &app.world().resource::<Toast>().text;
        assert!(toast.starts_with("Holding ") && toast.contains("went into the"), "{toast}");
    }

    #[test]
    fn f_puts_a_weapon_in_the_off_hand_while_only_that_is_free() {
        use fb_items::Slot;
        let (mut app, _) = equipping_app();
        let sword = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let sword = inventory.add(fb_items::roll_item("Sword", 3).unwrap());
            inventory.wear(sword, Slot::Mainhand).unwrap();
            sword
        };
        let (_, dagger, _) = lay_ahead(&mut app, fb_items::roll_item("Dagger", 4).unwrap());
        press_g(&mut app);
        let inventory = app.world().resource::<Inventory>();
        assert_eq!((inventory.worn(Slot::Mainhand), inventory.worn(Slot::Offhand)), (Some(sword), Some(dagger)));
        let toast = &app.world().resource::<Toast>().text;
        assert!(toast.starts_with("Holding ") && !toast.contains("went"), "{toast}");
    }

    #[test]
    fn f_on_a_helm_swaps_helms_and_sets_the_old_one_down_where_the_new_one_lay() {
        use fb_items::Slot;
        let (mut app, pack) = equipping_app();
        let old = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            // No room left in the pack, so the old helm cannot be stowed.
            inventory.take_out(pack);
            let old = inventory.add(fb_items::roll_item("Helm", 5).unwrap());
            inventory.wear(old, Slot::Head).unwrap();
            old
        };
        let (_, new, spot) = lay_ahead(&mut app, fb_items::roll_item("Helm", 9).unwrap());
        press_g(&mut app);
        let inventory = app.world().resource::<Inventory>();
        assert_eq!(inventory.worn(Slot::Head), Some(new));
        assert_eq!(inventory.location(old), Some(crate::inventory::Location::Loose));
        let old_item = inventory.item(old).unwrap();
        let items = &app.world().resource::<CurrentLevel>().0.items;
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!((items[0].x, items[0].y, items[0].yaw, items[0].seed), (spot.x, spot.y, spot.yaw, old_item.seed));
        let mut floor = app.world_mut().query::<&FloorItem>();
        assert_eq!(floor.iter(app.world()).map(|f| f.id).collect::<Vec<_>>(), vec![old]);
        let toast = &app.world().resource::<Toast>().text;
        assert!(toast.starts_with("Wearing ") && toast.ends_with("was set down."), "{toast}");
    }

    #[test]
    fn g_on_something_with_no_slot_does_nothing() {
        let (mut app, _) = equipping_app();
        let (entity, bread, _) = lay_ahead(&mut app, fb_items::roll_item("Bread", 1).unwrap());
        press_g(&mut app);
        assert!(app.world().get_entity(entity).is_ok());
        assert_eq!(app.world().resource::<Inventory>().location(bread), Some(crate::inventory::Location::Loose));
        assert_eq!(app.world().resource::<CurrentLevel>().0.items.len(), 1);
        assert_eq!(app.world().resource::<Toast>().text, "");
    }

    #[test]
    fn tertiary_and_special_do_nothing_to_an_item_and_nothing_aimed_does_nothing() {
        let (mut app, _) = equipping_app();
        let (entity, sword, _) = lay_ahead(&mut app, fb_items::roll_item("Shortsword", 1).or_else(|| fb_items::roll_item("Dagger", 1)).unwrap());
        for key in [KeyCode::KeyR, KeyCode::KeyT] {
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
            app.update();
        }
        assert!(app.world().get_entity(entity).is_ok());
        assert_eq!(app.world().resource::<Inventory>().location(sword), Some(crate::inventory::Location::Loose));
        // Looking away, F and G have nothing to act on either.
        let camera = app.world_mut().query_filtered::<Entity, With<PlayerCamera>>().single(app.world()).unwrap();
        *app.world_mut().get_mut::<Transform>(camera).unwrap() = Transform::from_xyz(0.0, EYE_HEIGHT, 0.0).looking_at(Vec3::new(0.0, EYE_HEIGHT, 10.0), Vec3::Y);
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Aimed>().0, None);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
        for key in [KeyCode::KeyF, KeyCode::KeyG] {
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        }
        app.update();
        app.update();
        assert!(app.world().get_entity(entity).is_ok());
        assert_eq!(app.world().resource::<Inventory>().location(sword), Some(crate::inventory::Location::Loose));
    }

    #[test]
    fn f_puts_a_ring_in_a_free_ring_slot_and_a_torch_in_the_off_hand() {
        use fb_items::Slot;
        let mut inventory = Inventory::default();
        let first = inventory.add(fb_items::roll_item("Ring", 2).unwrap());
        inventory.wear(first, Slot::Ring1).unwrap();
        let second = inventory.add(fb_items::roll_item("Ring", 3).unwrap());
        let equipped = equip_from_floor(&mut inventory, second, Vec2::ZERO, 0.0).unwrap();
        assert_eq!((equipped.slot, equipped.displaced), (Slot::Ring2, None));
        assert_eq!(inventory.worn(Slot::Ring1), Some(first));
        let torch = inventory.add(lit_torch(4));
        assert_eq!(equip_from_floor(&mut inventory, torch, Vec2::ZERO, 0.0).unwrap().slot, Slot::Offhand);
        let shield = inventory.add(fb_items::roll_item("Shield", 12).unwrap());
        let equipped = equip_from_floor(&mut inventory, shield, Vec2::new(2.0, 3.0), 0.5).unwrap();
        assert_eq!(equipped.slot, Slot::Offhand);
        // Nowhere to stow the torch: it goes down where the shield lay.
        assert_eq!(equipped.displaced, Some((torch, None)));
        assert_eq!(equipped.changes, vec![crate::inventory::FloorChange::Lifted(shield), crate::inventory::FloorChange::SetDownAt(torch, Vec2::new(2.0, 3.0), 0.5)]);
        assert_eq!(carried_light(&inventory), None, "the carried light follows the hands");
    }

    #[test]
    fn hang_at_hip_sends_a_floor_lantern_to_the_hip_and_swaps_out_what_hung_there() {
        use fb_items::Slot;
        let mut inventory = Inventory::default();
        let belt = inventory.add(fb_items::roll_item("Belt", 1).unwrap());
        inventory.wear(belt, Slot::Belt).unwrap();
        let old = inventory.add(fb_items::roll_item("Lantern", 2).unwrap());
        inventory.wear(old, Slot::Hip).unwrap();
        let lantern = inventory.add(fb_items::roll_item("Lantern", 3).unwrap());
        assert_eq!(floor_verb(&inventory, lantern, crate::interact::Slot::Special), Some(crate::interact::Verb::HangAtHip));
        let hung = equip_from_floor_in(&mut inventory, lantern, Slot::Hip, Vec2::new(1.0, 2.0), 0.3).unwrap();
        assert_eq!((hung.slot, inventory.worn(Slot::Hip), inventory.worn(Slot::Offhand)), (Slot::Hip, Some(lantern), None));
        // The belt's own pouch takes the lantern that hung there.
        assert_eq!(hung.displaced, Some((old, Some(belt))));
        let title = |id| inventory.item(id).unwrap().title().to_string();
        assert_eq!(hung.message(&inventory, lantern), format!("{} hangs from the belt. {} went into the {}.", title(lantern), title(old), title(belt)));
    }

    #[test]
    fn legendary_floor_items_shimmer_and_others_do_not() {
        let drops = crate::inventory::underfoot::drops_for_capture("Sword:legendary,Sword:common");
        let [legendary, common] = [&drops[0], &drops[1]];
        assert_eq!((legendary.rarity, common.rarity), (Rarity::Legendary, Rarity::Common));
        let (mut app, _) = delving_app();
        let mut spawned = Vec::new();
        for (x, item) in [(0.0, legendary.clone()), (1.0, common.clone())] {
            use bevy::ecs::system::RunSystemOnce;
            let id = app.world_mut().resource_mut::<Inventory>().add(item.clone());
            let spot = ItemSpot { x, y: 0.0, yaw: 0.0, family: item.shape.family(), seed: item.seed };
            let entity = app
                .world_mut()
                .run_system_once(move |mut spawner: ItemSpawner, inventory: Res<Inventory>| spawn_floor_item(&mut spawner, &inventory, id, spot, &Tuning::default()).unwrap())
                .unwrap();
            spawned.push(entity);
        }
        let shimmers = |app: &mut App, entity: Entity| {
            let children: Vec<Entity> = app.world().get::<Children>(entity).unwrap().iter().collect();
            let materials = app.world().resource::<Assets<ToonMaterial>>();
            children
                .iter()
                .filter_map(|&c| app.world().get::<MeshMaterial3d<ToonMaterial>>(c))
                .map(|m| materials.get(&m.0).unwrap().extension.shimmer)
                .collect::<Vec<_>>()
        };
        let strength = Tuning::default().view.legendary_shimmer;
        let gold = shimmers(&mut app, spawned[0]);
        assert!(!gold.is_empty() && gold.iter().all(|s| s.strength == strength && s.color == Shimmer::new(rarity_color(Rarity::Legendary), 1.0).color));
        assert!(shimmers(&mut app, spawned[1]).iter().all(|s| s.strength == 0.0));
        // Held or drawn anywhere else, it does not shimmer.
        assert_eq!(appearance_of(&legendary.shape).shimmer, Shimmer::default());
        // A change of tuning reaches the floor items already lying there.
        app.add_systems(Update, reshimmer_floor_items);
        app.world_mut().resource_mut::<Tuning>().view.legendary_shimmer = 0.5;
        app.update();
        assert!(shimmers(&mut app, spawned[0]).iter().all(|s| s.strength == 0.5));
    }

    #[test]
    fn e_picks_up_the_item_under_the_crosshair() {
        let (mut app, pack) = delving_app();
        let level = fb_level::generate(1207, 1);
        let (spot, item) = level
            .items
            .iter()
            .map(|s| (*s, fb_items::roll_loot(s.family, s.seed, fb_items::Odds::loot(level.depth)).unwrap()))
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
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyF);
        app.update();
        assert!(app.world().get_entity(floor).is_err());
        let inventory = app.world().resource::<Inventory>();
        let packed: Vec<String> = inventory.grid(pack).unwrap().entries().iter().map(|e| inventory.item(e.id).unwrap().title().to_string()).collect();
        assert_eq!(packed, vec![name]);
        assert!(app.world().resource::<CurrentLevel>().0.items.is_empty());
    }

    #[test]
    fn floor_items_on_1207_are_rolled_with_rarity() {
        // Four levels down, as the first rolls nearly everything common.
        let level = fb_level::generate(fb_level::depth_seed(1207, 4), 4);
        let items: Vec<fb_items::Item> = level.items.iter().map(|s| fb_items::roll_loot(s.family, s.seed, fb_items::Odds::loot(level.depth)).unwrap()).collect();
        assert_eq!(items.len(), level.items.len());
        for (item, spot) in items.iter().zip(&level.items) {
            assert_eq!(item.shape.family(), spot.family);
            if item.profile.kind == fb_items::Kind::Gear {
                assert_eq!(item.shape.condition().state, armory::cond::ConditionState::Custom, "{}: not dressed", item.name);
            }
        }
        assert!(items.iter().any(|i| i.rarity != Rarity::Common), "a level of nothing but common items");
        let first = fb_level::generate(1207, 1);
        let firsts = first.items.iter().filter_map(|s| fb_items::roll_loot(s.family, s.seed, fb_items::Odds::loot(1)));
        assert!(firsts.into_iter().all(|i| i.rarity < Rarity::Legendary), "a legendary on the first level");
    }

    #[test]
    fn every_torch_and_lantern_lying_in_a_level_can_be_held_in_the_off_hand() {
        let mut lights = 0;
        for seed in 1..=32 {
            for spot in fb_level::generate(seed, 1).items.iter().filter(|s| matches!(s.family, Family::Torch | Family::Lantern)) {
                let item = fb_items::roll_loot(spot.family, spot.seed, fb_items::Odds::default()).unwrap();
                let (base, name) = (item.base.clone(), item.title().to_string());
                let mut inventory = Inventory::default();
                let id = inventory.add(item);
                assert_eq!(inventory.target_slot(id), Some(fb_items::Slot::Offhand), "seed {seed}: {base} ({name})");
                inventory.equip(id, fb_items::Slot::Offhand).unwrap_or_else(|r| panic!("seed {seed}: {base} ({name}): {}", r.reason()));
                assert_eq!(inventory.worn(fb_items::Slot::Offhand), Some(id));
                lights += 1;
            }
        }
        assert!(lights > 0, "no torch or lantern lies in levels 1 to 32");
    }

    #[test]
    fn floor_items_wear_their_own_condition() {
        let item = (1..500).filter_map(|seed| fb_items::roll_item("Sword", seed)).find(|i| i.rarity == Rarity::Common).unwrap();
        assert_eq!(appearance_of(&item.shape).condition, item.shape.condition());
        assert!(item.shape.condition().wear >= 0.15);
    }

    #[test]
    fn grip_point_stays_at_hand_regardless_of_held_scale() {
        let spec = Character::default().spec();
        let sword = (1..400).filter_map(|s| fb_items::roll_item("Sword", s)).find(|i| i.shape.family() == Family::Sword).unwrap().shape;
        let torch = lit_torch(1207).shape;

        for (name, item) in &[("Sword", sword.clone()), ("Torch", torch.clone())] {
            for scale in [1.0, 1.4] {
                // Fit the item at this scale
                let placed = crate::view_model::held_in_view(&spec, Some(item), None, scale);
                let held = &placed[0];

                // The grip point in item space (stored in HeldBuilt)
                let grip = held.grip;

                // Compute where the grip is in world space from the fitted transform
                let hand_transform = held.transform;
                let grip_world = hand_transform.transform_point(grip);

                // The grip should be at the same world location regardless of scale,
                // because we're scaling about the grip point
                if scale == 1.0 {
                    // On first iteration, just record the position
                    continue;
                }
                // On second iteration, compare with scale 1.0
                let placed_1 = crate::view_model::held_in_view(&spec, Some(item), None, 1.0)[0].transform;
                let grip_world_1 = placed_1.transform_point(grip);
                assert!(
                    grip_world.abs_diff_eq(grip_world_1, 1e-4),
                    "{name} at scale {scale}: grip moved from {grip_world_1} to {grip_world}"
                );
            }
        }
    }

    #[test]
    fn the_held_key_changes_as_a_lit_torch_crosses_a_char_bucket() {
        use fb_items::Slot;
        let mut inventory = Inventory::default();
        let mut torch = lit_torch(2);
        if let armory::Item::Torch(s) = &mut torch.shape {
            s.burn.char = 0.0;
            s.burn.fuel_left = armory::kindle::TORCH_SECONDS;
        }
        let id = inventory.add(torch);
        inventory.wear(id, Slot::Offhand).unwrap();
        let character = Character::default();
        let key = |inventory: &Inventory| HeldKey::of(inventory, &character);
        let start = key(&inventory);
        inventory.burn_lights(60.0);
        assert_eq!(key(&inventory), start, "a minute is under a bucket");
        inventory.burn_lights(armory::kindle::TORCH_SECONDS * 0.06);
        assert_ne!(key(&inventory), start, "past 0.05 char the held torch is fitted again");
    }

    #[test]
    fn the_toast_sits_above_the_orbs_and_quick_bar() {
        let layout = crate::combat::orbs::cluster_layout();
        let cluster_top = crate::combat::orbs::MARGIN + layout.size.y;
        assert!(toast_lift() > cluster_top, "{} over {}", toast_lift(), cluster_top);
        assert!(toast_lift() < 40.0, "still near the foot of the view");
    }
}
