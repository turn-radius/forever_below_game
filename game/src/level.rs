//! The level's stone geometry, fixed lights and the entrance door that shuts behind the player.
//!
//! The shapes are plain data built in [`geometry`] (no Bevy); this file spawns them in the
//! house style (`docs/rendering.md`): banded toon stone weathered by the decay shader, ink
//! along the rock's edges, timber door frames, glowcaps and braziers as the only fixed light,
//! and an ambient fill of a few lux so carried light is what matters. The entrance door slams
//! and latches when it reaches shut ([`LevelSounds`]).
//!
//! `FB_DEBUG_CAMERA=start|back|shut|top|brazier|glowcaps|hall|vault` is for captures: a lit camera that stands in for the
//! [`PlayerCamera`] (which it deactivates), at the start looking in, a cell in looking back at
//! the open entrance, the same with the player walked a cell further in so the door shuts, or
//! high over the level looking down, or beside the first brazier or glowcaps, or at one end of
//! the first colonnaded hall looking down it (`hall`) or up at its vault (`vault`). Unset, nothing here touches the player's camera.
//! `FB_DEPTH=2` plays the stone halls of depth 2.

pub mod geometry;
pub mod stairs;

use armory::{
    Rgb,
    cond::{Condition, ConditionState},
    families::{
        Generator,
        food::{Food, FoodStyle},
    },
};
use bevy::{camera::Exposure, light::NotShadowCaster, prelude::*, render::view::Tonemapping};
use fb_level::{CELL, LightKind, StairKind};

use crate::capture::FrameCapturePlugin;
use crate::light::{Torch, cast_shadows, point_light};
use crate::play::{BlockedCells, CurrentLevel, EYE_HEIGHT, Player, PlayerCamera};
use crate::audio::sound_effect;
use crate::play::DelveSystems;
use crate::settings::GameSettings;
use crate::travel::{Abyss, Arrival};
use crate::tuning::{LightTuning, Tuning};
use crate::render::{ArmoryItem, ArmoryPart, InkOutline, ItemAppearance, MinWidth, OutlineMaterial, ToonMaterial, part_to_mesh};
use fb_level::LevelStyle;
use geometry::{CellView, Doorway, LevelMesh};

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tuning>()
            .insert_resource(abyss_ambient(&LightTuning::default()))
            .insert_resource(ClearColor(FOG_COLOR))
            // The level's stone is laid out for each level the delver comes to: the first of a
            // delve, then each one they travel to ([`crate::travel`]), as the folk and props are.
            .add_systems(
                Update,
                (clear_stone, spawn_stone, spawn_columns, spawn_fixtures, spawn_stairs, spawn_debug_camera).chain().run_if(resource_exists_and_changed::<CurrentLevel>).in_set(DelveSystems),
            )
            // Offscreen captures stay silent, as the music does.
            .add_systems(Startup, load_level_sounds.run_if(not(resource_exists::<FrameCapturePlugin>)))
            .add_systems(Update, (shut_the_entrance, fog_cameras, debug_camera_takes_over, unshadow_flames).in_set(DelveSystems))
            .add_systems(Update, apply_light_tuning.run_if(resource_changed::<Tuning>).in_set(DelveSystems));
    }
}

/// Cells of the level drawn as one mesh per surface, so far stone can be culled.
pub const CHUNK_CELLS: i32 = 16;

/// The exposure the level's light levels are set for: the camera's, `Exposure::INDOOR`.
pub const EXPOSURE: Exposure = Exposure::INDOOR;

/// How far the player must be from the entrance cell's centre, in cells, before the door
/// swings shut behind them.
pub const SHUT_DISTANCE_CELLS: f32 = 1.2;
/// Seconds the entrance door takes to swing shut.
pub const SHUT_SECONDS: f32 = 1.5;

/// The abyss's own light: a few lux (`ambient_lux`, [`LightTuning`]) of cold grey-blue, so
/// stone a torch has not reached is a shape and not a colour.
pub const AMBIENT_COLOR: Color = Color::srgb(0.55, 0.6, 0.72);
/// The dark that swallows far stone: a near-black blue, thickening from `fog_start` to
/// `fog_end` (4 m to 26 m as shipped).
pub const FOG_COLOR: Color = Color::srgb(0.012, 0.016, 0.024);

/// Glowcaps: faint cold cyan, a pool reaching about three metres (`glowcap_peak` and
/// `glowcap_reach`). Braziers burn in the torch colour, brighter and further than a torch in
/// hand (`brazier_peak` and `brazier_reach`).
pub const GLOWCAP_COLOR: Rgb = Rgb::hex(0x6ad0c8);
pub const GLOWCAP_LIGHT: Color = Color::srgb(0.45, 0.85, 0.88);

/// On each of the level's fixed lights: which kind it is, so it can be lit again when the
/// tuning changes.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedLight {
    Brazier,
    Glowcaps,
}

impl FixedLight {
    /// Its light, as tuned.
    pub fn light(self, tuning: &LightTuning) -> PointLight {
        match self {
            FixedLight::Brazier => cast_shadows(Torch::flickering(Torch::light(tuning.brazier_peak, tuning.brazier_reach, &EXPOSURE), tuning.flicker_depth)),
            FixedLight::Glowcaps => glowcap_light(tuning),
        }
    }
}

/// The stone's weathering: wear 0.5 on stone (decay type 9), its pattern a few metres across
/// in world space rather than an item's few centimetres.
pub const STONE_WEAR: f32 = 0.5;
const STONE_PATTERN_SCALE: f32 = 0.06;
/// How much finer the decay pattern runs over a stone hall's ceiling than over its walls.
const CEILING_PATTERN_FINER: f32 = 2.6;
/// The rock's ink: near black, a shade under the fog.
pub const STONE_INK: Color = Color::srgb(0.008, 0.009, 0.011);
/// The finer ink along dressed stone's courses and joints: the same dark, a little softer.
const STONE_JOINT_INK: Color = Color::srgb(0.02, 0.022, 0.026);

fn abyss_ambient(tuning: &LightTuning) -> GlobalAmbientLight {
    GlobalAmbientLight { color: AMBIENT_COLOR, brightness: tuning.ambient_lux, ..default() }
}

/// The abyss's fog, as tuned.
fn abyss_fog(tuning: &LightTuning) -> DistanceFog {
    DistanceFog {
        color: FOG_COLOR,
        directional_light_color: Color::NONE,
        falloff: FogFalloff::Linear { start: tuning.fog_start, end: tuning.fog_end },
        ..default()
    }
}

/// Brings the ambient, the fog on every camera and the fixed lights up to the tuning.
fn apply_light_tuning(
    tuning: Res<Tuning>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut fogs: Query<&mut DistanceFog>,
    mut lights: Query<(&FixedLight, &mut PointLight)>,
    debug: Query<&DebugCamera>,
) {
    let light = &tuning.light;
    // The view from above has its own bright fill.
    if debug.iter().all(|d| d.fog) {
        ambient.brightness = light.ambient_lux;
    }
    for mut fog in &mut fogs {
        fog.falloff = FogFalloff::Linear { start: light.fog_start, end: light.fog_end };
    }
    for (kind, mut point) in &mut lights {
        *point = kind.light(light);
    }
}

/// The look of the level's own stone and fixtures.
fn stone_appearance(seed: u32) -> ItemAppearance {
    let condition = Condition { wear: STONE_WEAR, rot: 1.0, ..Condition::of(ConditionState::Neglected) };
    ItemAppearance {
        condition,
        seed,
        decay_scale_factor: STONE_PATTERN_SCALE / condition.scale,
        // The rock's ink is drawn by its own ribbons, wider than an item's hull may grow, and
        // darker than the Armory's, so it never shows lighter than unlit stone in the dark.
        outline: OutlineMaterial { color: STONE_INK.into(), width_px: 1.8, max_width: 0.6, ..default() },
        ..default()
    }
}

/// The look of door frames, the entrance door, braziers and scenery: worn items of the level,
/// with hulls allowed to grow to a few centimetres, as befits things of their size.
pub fn fixture_appearance(seed: u32, state: ConditionState) -> ItemAppearance {
    ItemAppearance {
        condition: Condition::of(state),
        seed,
        outline: OutlineMaterial { color: STONE_INK.into(), max_width: 0.3, ..default() },
        ..default()
    }
}

/// The level's sound effects, loaded up front so none waits on the disk when it plays.
#[derive(Resource, Clone, Debug, Default)]
pub struct LevelSounds {
    /// The entrance door's slam and latch, synthesised by `tools/synth_door_shut.py`.
    pub door_shut: Handle<AudioSource>,
}

impl LevelSounds {
    pub const DOOR_SHUT: &str = "audio/door-shut.ogg";
}

fn load_level_sounds(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(LevelSounds { door_shut: assets.load(LevelSounds::DOOR_SHUT) });
}

/// Marks every entity the level spawns.
#[derive(Component, Debug, Default)]
#[require(DespawnOnExit<crate::launch::AppState> = DespawnOnExit(crate::launch::AppState::Delving))]
pub struct LevelPiece;

/// Marks the level's own stone, fixtures and stairs (not the props and folk that stand in it,
/// which have their own systems), so travelling to another level can clear them and lay out
/// the next ([`clear_stone`]).
#[derive(Component, Debug, Default)]
#[require(LevelPiece)]
pub struct StonePiece;

/// The entrance door's leaf: open into the level until the player walks away from it, then
/// swinging shut for good.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct EntranceDoor {
    /// The door's rotation when shut; the swing turns it about its hinge from there.
    pub shut: Quat,
    /// The entrance cell's centre on the floor.
    pub centre: Vec3,
    pub cell: (i32, i32),
    pub swing: DoorSwing,
}

/// A moment in a door's swing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorEvent {
    /// It began to swing shut.
    BeganToShut,
    /// It reached shut.
    Shut,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DoorSwing {
    Open,
    /// Seconds since it began to swing.
    Closing(f32),
    Shut,
}

impl EntranceDoor {
    /// The leaf's rotation at its current swing.
    pub fn rotation(&self) -> Quat {
        let progress = match self.swing {
            DoorSwing::Open => 0.0,
            DoorSwing::Closing(t) => t / SHUT_SECONDS,
            DoorSwing::Shut => 1.0,
        };
        self.shut * Quat::from_rotation_y(geometry::entrance_swing(progress))
    }

    /// Whether a player at `feet` has walked far enough in for the door to shut.
    pub fn player_is_clear(&self, feet: Vec3) -> bool {
        (feet - self.centre).xz().length() > SHUT_DISTANCE_CELLS * CELL
    }

    /// Advances the swing by `dt` seconds with the player at `feet` (if there is one).
    /// Returns what happened this step, if anything.
    pub fn step(&mut self, dt: f32, feet: Option<Vec3>) -> Option<DoorEvent> {
        match self.swing {
            DoorSwing::Open if feet.is_some_and(|f| self.player_is_clear(f)) => {
                self.swing = DoorSwing::Closing(0.0);
                Some(DoorEvent::BeganToShut)
            }
            DoorSwing::Closing(t) if t + dt >= SHUT_SECONDS => {
                self.swing = DoorSwing::Shut;
                Some(DoorEvent::Shut)
            }
            DoorSwing::Closing(t) => {
                self.swing = DoorSwing::Closing(t + dt);
                None
            }
            _ => None,
        }
    }
}

fn v3(p: [f32; 3]) -> Vec3 {
    Vec3::from_array(p)
}

fn doorway_transform(doorway: &Doorway) -> Transform {
    let [x, y, z] = doorway.basis().map(v3);
    let centre = v3(geometry::cell_centre(doorway.x, doorway.y));
    Transform::from_translation(centre).with_rotation(Quat::from_mat3(&Mat3::from_cols(x, y, z)))
}

/// Clears the stone, fixtures and stairs of the level the delver has left.
fn clear_stone(mut commands: Commands, old: Query<Entity, With<StonePiece>>) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
}

fn spawn_stone(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_materials: ResMut<Assets<ToonMaterial>>,
    mut outline_materials: ResMut<Assets<OutlineMaterial>>,
) {
    let level = &level.0;
    let view = CellView::new(level);
    let stone = stone_appearance(level.seed);

    // The rock: one entity per surface per chunk, each surface kind sharing one material.
    let fine_ink = outline_materials.add(OutlineMaterial { color: STONE_JOINT_INK.into(), width_px: 1.0, ..stone.outline });
    let ink = outline_materials.add(stone.outline);
    let mut materials: Vec<(&'static str, Handle<ToonMaterial>)> = Vec::new();
    for range in geometry::chunks(&view, CHUNK_CELLS) {
        let LevelMesh { surfaces, ink: lines, joints } = geometry::level_mesh(&view, range);
        for part in &surfaces.parts {
            let material = match materials.iter().find(|(name, _)| *name == part.name) {
                Some((_, handle)) => handle.clone(),
                None => {
                    let mut material = stone.material_for(part);
                    // Rock is never a fine part to fatten.
                    material.extension.min_width = MinWidth::default();
                    // A vault's ceiling is mottled finer than the walls are: the blotches over a
                    // soffit this size would otherwise read as flat patches.
                    if part.name == "ceiling" && level.style == LevelStyle::StoneHalls {
                        material.extension.decay.scale *= CEILING_PATTERN_FINER;
                    }
                    let handle = toon_materials.add(material);
                    materials.push((part.name, handle.clone()));
                    handle
                }
            };
            commands.spawn((StonePiece, Name::new(part.name), Mesh3d(meshes.add(part_to_mesh(part))), MeshMaterial3d(material)));
        }
        if !lines.indices.is_empty() {
            commands.spawn((StonePiece, Name::new("ink"), Mesh3d(meshes.add(part_to_mesh(&lines))), MeshMaterial3d(ink.clone())));
        }
        if !joints.indices.is_empty() {
            commands.spawn((StonePiece, Name::new("joints"), Mesh3d(meshes.add(part_to_mesh(&joints))), MeshMaterial3d(fine_ink.clone())));
        }
    }

}

/// The columns of a hall: one mesh for each shape (height and variant) shared by every column
/// of that shape, so a colonnade costs a few meshes however long it is. Each is turned a
/// quarter at a time so repeats do not wear alike.
fn spawn_columns(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_materials: ResMut<Assets<ToonMaterial>>,
    mut outline_materials: ResMut<Assets<OutlineMaterial>>,
) {
    let level = &level.0;
    let spots = geometry::stone::columns(level);
    if spots.is_empty() {
        return;
    }
    let stone = stone_appearance(level.seed);
    let ink = outline_materials.add(stone.outline);
    let mut shapes: std::collections::HashMap<(i32, i32, u32), Vec<(&'static str, Handle<Mesh>, Handle<ToonMaterial>, bool)>> = Default::default();
    // One material for every shape's part of a name: they differ only in their meshes.
    let mut materials: std::collections::HashMap<&'static str, Handle<ToonMaterial>> = Default::default();
    for spot in spots {
        let parts = shapes.entry((spot.seat as i32, spot.top as i32, spot.variant)).or_insert_with(|| {
            geometry::stone::column(spot.seat, spot.top, spot.variant, level.seed)
                .parts
                .iter()
                .map(|part| {
                    let material = materials
                        .entry(part.name)
                        .or_insert_with(|| {
                            let mut material = stone.material_for(part);
                            material.extension.min_width = MinWidth::default();
                            toon_materials.add(material)
                        })
                        .clone();
                    (part.name, meshes.add(part_to_mesh(part)), material, part.outline)
                })
                .collect()
        });
        let at = v3(geometry::cell_centre(spot.cell.0, spot.cell.1));
        let turn = Quat::from_rotation_y(spot.turn as f32 * std::f32::consts::FRAC_PI_2);
        let root = commands
            .spawn((StonePiece, ArmoryItem, Name::new("column"), Transform::from_translation(at).with_rotation(turn), Visibility::default()))
            .id();
        for (name, mesh, material, outlined) in parts.iter() {
            let part = commands.spawn((ArmoryPart, Name::new(*name), Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), ChildOf(root))).id();
            if *outlined {
                commands.spawn((InkOutline, Mesh3d(mesh.clone()), MeshMaterial3d(ink.clone()), ChildOf(part)));
            }
        }
    }
}

fn spawn_fixtures(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    abyss: Option<Res<Abyss>>,
    tuning: Res<Tuning>,
    mut blocked: ResMut<BlockedCells>,
    mut spawner: crate::render::ItemSpawner,
) {
    let level = &level.0;
    // The door shut behind the delver when they came in; coming back to it from below finds it
    // shut already, and it does not swing again.
    let shut_already = abyss.is_some_and(|a| a.arrival == Arrival::FromBelow);
    let view = CellView::new(level);

    // Door frames, and the entrance door hung in its own.
    let entrance = (level.entrance.x as i32, level.entrance.y as i32);
    for doorway in geometry::doorways(&view) {
        let seed = level.seed ^ ((doorway.x as u32) << 16 | doorway.y as u32);
        let frame = match level.style {
            LevelStyle::RoughRock => geometry::door_frame(level.ceiling, seed),
            LevelStyle::StoneHalls => geometry::stone::door_frame(level, (doorway.x, doorway.y), seed),
        };
        let transform = doorway_transform(&doorway);
        spawner.spawn(&frame, &fixture_appearance(seed, ConditionState::BattleWorn)).insert((StonePiece, transform));
        if (doorway.x, doorway.y) == entrance {
            let hinge = transform.transform_point(v3(geometry::entrance_hinge()));
            let swing = if shut_already { DoorSwing::Shut } else { DoorSwing::Open };
            if shut_already {
                blocked.0.insert(entrance);
            }
            let door = EntranceDoor { shut: transform.rotation, centre: transform.translation, cell: entrance, swing };
            let leaf = geometry::entrance_leaf(seed);
            spawner
                .spawn(&leaf, &fixture_appearance(seed, ConditionState::Serviceable))
                .insert((StonePiece, Name::new("entrance door"), door, Transform::from_translation(hinge).with_rotation(door.rotation())));
        }
    }

    // Fixed lights.
    for (i, spot) in level.lights.iter().enumerate() {
        let at = v3(fb_level::Level::world(spot.x, spot.y));
        let seed = level.seed.wrapping_add(0x11 * i as u32 + 1);
        match spot.kind {
            LightKind::Glowcaps => spawn_glowcaps(&mut commands, &mut spawner, at, seed, &tuning.light),
            LightKind::Brazier => {
                let (mesh, flame) = geometry::brazier(seed);
                spawner.spawn(&mesh, &fixture_appearance(seed, ConditionState::BattleWorn)).insert((StonePiece, Transform::from_translation(at)));
                let light = FixedLight::Brazier.light(&tuning.light);
                commands.spawn((StonePiece, Name::new("brazier light"), FixedLight::Brazier, light, Transform::from_translation(at + Vec3::Y * flame)));
            }
        }
    }
}

/// The level's stairs, each in its cell, which the delver cannot walk onto.
fn spawn_stairs(level: Res<CurrentLevel>, mut blocked: ResMut<BlockedCells>, mut spawner: crate::render::ItemSpawner) {
    let level = &level.0;
    for stair in &level.stairs {
        blocked.0.insert((stair.x as i32, stair.y as i32));
        let seed = level.seed ^ ((stair.x << 16 | stair.y) ^ 0x57A1);
        let mesh = stairs::stair_mesh(stair.kind, level.ceiling, seed);
        let (at, yaw) = stairs::stair_transform(stair);
        let name = if stair.kind == StairKind::Down { "stair down" } else { "stair up" };
        spawner
            .spawn(&mesh, &fixture_appearance(seed, ConditionState::BattleWorn))
            .insert((StonePiece, Name::new(name), Transform::from_translation(v3(at)).with_rotation(Quat::from_rotation_y(yaw))));
    }
}

/// A patch of glowcaps: three clusters round the spot, and one faint steady light over them.
fn spawn_glowcaps(commands: &mut Commands, spawner: &mut crate::render::ItemSpawner, at: Vec3, seed: u32, tuning: &LightTuning) {
    let mut light_at = Vec::new();
    for k in 0..3u32 {
        let mut state = Food::roll_as(seed.wrapping_add(k * 7919), FoodStyle::Mushroom);
        let p = &mut state.provision;
        p.glow = p.glow.max(0.6);
        p.outer = GLOWCAP_COLOR;
        let angle = seed as f32 * 0.37 + k as f32 * 2.2;
        let offset = if k == 0 { Vec3::ZERO } else { Vec3::new(angle.cos(), 0.0, angle.sin()) * (2.2 + k as f32 * 0.6) };
        let anchor = Food::light(&state).map(|a| v3(a.position)).unwrap_or(Vec3::Y * 2.0);
        light_at.push(at + offset + anchor);
        let appearance = ItemAppearance { seed: state.seed, decay_scale_factor: 3.0, ..default() };
        spawner
            .spawn(&Food::build(&state), &appearance)
            .insert((StonePiece, Transform::from_translation(at + offset).with_rotation(Quat::from_rotation_y(angle))));
    }
    let centre = light_at.iter().sum::<Vec3>() / light_at.len() as f32;
    commands.spawn((StonePiece, Name::new("glowcap light"), FixedLight::Glowcaps, glowcap_light(tuning), Transform::from_translation(centre + Vec3::Y * 1.5)));
}

/// The faint light over a patch of glowcaps. Shadowed like every light in the level, so it
/// stays in the cells it can see; the caps themselves cast none (`unshadow_flames`).
pub fn glowcap_light(tuning: &LightTuning) -> PointLight {
    cast_shadows(point_light(GLOWCAP_LIGHT, tuning.glowcap_peak, tuning.glowcap_reach, &EXPOSURE))
}

/// Flames and embers on the level's fixtures give the light, so they must not shadow it.
fn unshadow_flames(
    mut commands: Commands,
    parts: Query<(Entity, &Name, &ChildOf), Added<ArmoryPart>>,
    level_pieces: Query<(), With<LevelPiece>>,
) {
    for (part, name, parent) in &parts {
        let lit = matches!(name.as_str(), "flame" | "flame_core" | "embers" | "cap");
        if lit && level_pieces.contains(parent.parent()) {
            commands.entity(part).insert(NotShadowCaster);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn shut_the_entrance(
    mut commands: Commands,
    time: Res<Time>,
    sounds: Option<Res<LevelSounds>>,
    game: Option<Res<GameSettings>>,
    tuning: Option<Res<Tuning>>,
    players: Query<&GlobalTransform, With<Player>>,
    mut doors: Query<(&mut EntranceDoor, &mut Transform)>,
    mut blocked: ResMut<BlockedCells>,
) {
    let feet = players.iter().next().map(GlobalTransform::translation);
    for (mut door, mut transform) in &mut doors {
        if door.swing == DoorSwing::Shut {
            continue;
        }
        match door.step(time.delta_secs(), feet) {
            // Blocked from the moment it swings, so the player cannot slip back under it.
            Some(DoorEvent::BeganToShut) => {
                blocked.0.insert(door.cell);
            }
            Some(DoorEvent::Shut) => {
                if let Some(sounds) = &sounds {
                    let game = game.as_deref().copied().unwrap_or_default();
                    let tuning = tuning.as_deref().cloned().unwrap_or_default();
                    commands.spawn((Name::new("door shut"), AudioPlayer::new(sounds.door_shut.clone()), sound_effect(&game, &tuning), DespawnOnExit(crate::launch::AppState::Delving)));
                }
            }
            None => {}
        }
        transform.rotation = door.rotation();
    }
}

/// A first-person or capture camera just spawned, with no fog yet.
type NewUnfoggedCamera = (Or<(Added<PlayerCamera>, Added<DebugCamera>)>, Without<DistanceFog>);

/// Lays the abyss's dark over every first-person or debug camera.
fn fog_cameras(
    mut commands: Commands,
    tuning: Res<Tuning>,
    cameras: Query<(Entity, Option<&DebugCamera>), NewUnfoggedCamera>,
) {
    for (camera, debug) in &cameras {
        if debug.is_some_and(|d| !d.fog) {
            continue;
        }
        commands.entity(camera).insert(abyss_fog(&tuning.light));
    }
}

/// The capture camera from `FB_DEBUG_CAMERA`, and whether its mode walks the player in.
#[derive(Component, Debug, Default)]
pub struct DebugCamera {
    walk_in: bool,
    /// Off for the view from above, which looks from further than the fog lets anything through.
    fog: bool,
}

fn spawn_debug_camera(mut commands: Commands, level: Res<CurrentLevel>) {
    let Ok(mode) = std::env::var("FB_DEBUG_CAMERA") else { return };
    let level = &level.0;
    let (sx, sy, facing) = level.start();
    let inward = v3(geometry::facing_vector(facing));
    let start = v3(geometry::cell_centre(sx as i32, sy as i32));
    let eye = Vec3::Y * EYE_HEIGHT;
    let transform = match mode.as_str() {
        "back" | "shut" => {
            let at = start + inward * CELL * 0.9 + eye;
            Transform::from_translation(at).looking_at(at - inward * CELL * 2.0 - Vec3::Y * 3.0, Vec3::Y)
        }
        "top" => {
            let centre = Vec3::new(level.width as f32, 0.0, level.height as f32) * CELL * 0.5;
            let span = level.width.max(level.height) as f32 * CELL;
            Transform::from_translation(centre + Vec3::new(0.0, span * 0.9, span * 0.5)).looking_at(centre, Vec3::Y)
        }
        "hall" | "vault" => {
            // The first colonnaded hall, from one end of its long axis looking down it.
            let Some(room) = level.rooms.iter().find(|r| {
                (r.y..r.y + r.h).any(|y| (r.x..r.x + r.w).any(|x| level.cell(x as i32, y as i32) == fb_level::Cell::Column))
            }) else {
                return;
            };
            let (cx, cy) = room.centre();
            let (half_x, half_y) = (room.w as f32 * 0.5, room.h as f32 * 0.5);
            let along_x = room.w >= room.h;
            let (from, dir) = if along_x { ((cx - half_x + 0.7, cy), Vec3::X) } else { ((cx, cy - half_y + 0.7), Vec3::Z) };
            let at = v3(fb_level::Level::world(from.0, from.1)) + eye;
            let aim = if mode == "vault" { dir * 60.0 + Vec3::Y * 45.0 } else { dir * 60.0 };
            Transform::from_translation(at).looking_at(at + aim, Vec3::Y)
        }
        "brazier" | "glowcaps" => {
            let kind = if mode == "brazier" { LightKind::Brazier } else { LightKind::Glowcaps };
            let Some(spot) = level.lights.iter().find(|l| l.kind == kind) else { return };
            let at = v3(fb_level::Level::world(spot.x, spot.y));
            let (cx, cy) = (spot.x.floor() as i32, spot.y.floor() as i32);
            // Stand off toward the most open side, looking down at the light.
            let side = [fb_level::Facing::North, fb_level::Facing::East, fb_level::Facing::South, fb_level::Facing::West]
                .into_iter()
                .max_by_key(|f| (1..=2).take_while(|k| level.is_open(cx + f.step().0 * k, cy + f.step().1 * k)).count())
                .unwrap_or(facing);
            let from = at + v3(geometry::facing_vector(side)) * CELL * 1.4 + eye;
            Transform::from_translation(from).looking_at(at + Vec3::Y * 4.0, Vec3::Y)
        }
        _ => Transform::from_translation(start + eye).looking_to(inward, Vec3::Y),
    };
    commands
        .spawn((
            DebugCamera { walk_in: mode == "shut", fog: mode != "top" },
            StonePiece,
            Camera3d::default(),
            EXPOSURE,
            // The house style draws untonemapped; a tone curve would flatten the bands.
            Tonemapping::Linear,
            transform,
        ))
        // Something to see by until the player brings a torch.
        .with_child((Torch::light(1.0, 120.0, &EXPOSURE), Transform::from_xyz(4.0, -3.0, 2.0)));
    if mode == "top" {
        commands.insert_resource(GlobalAmbientLight { brightness: 120.0, ..abyss_ambient(&LightTuning::default()) });
    }
}

/// While a capture camera is up, the player's camera stands down, and in `shut` mode the
/// player is walked a cell further in (a stand-in [`Player`] on the camera if there is none),
/// so the entrance swings shut.
fn debug_camera_takes_over(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    mut debug: Query<(Entity, &mut DebugCamera)>,
    mut player_cameras: Query<&mut Camera, With<PlayerCamera>>,
    mut players: Query<&mut Transform, With<Player>>,
) {
    let Ok((entity, mut debug)) = debug.single_mut() else { return };
    for mut camera in &mut player_cameras {
        camera.is_active = false;
    }
    if std::mem::take(&mut debug.walk_in) {
        let inward = v3(geometry::facing_vector(level.0.start().2));
        match players.single_mut() {
            Ok(mut feet) => feet.translation += inward * CELL,
            Err(_) => {
                commands.entity(entity).insert(Player);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use armory::MaterialKind;

    fn door() -> EntranceDoor {
        EntranceDoor { shut: Quat::IDENTITY, centre: Vec3::new(110.0, 0.0, 190.0), cell: (5, 9), swing: DoorSwing::Open }
    }

    #[test]
    fn the_door_waits_for_the_player_to_walk_in() {
        let mut d = door();
        // No player yet, or one still in the doorway's reach: the door stands open.
        assert_eq!(d.step(1.0, None), None);
        assert_eq!(d.step(1.0, Some(d.centre + Vec3::new(0.0, 0.0, -CELL))), None);
        assert_eq!(d.swing, DoorSwing::Open);
        // A step further in and it begins to swing, then shuts after SHUT_SECONDS and stays.
        assert_eq!(d.step(0.1, Some(d.centre + Vec3::new(0.0, 0.0, -1.3 * CELL))), Some(DoorEvent::BeganToShut));
        let mut t = 0.0;
        let mut shut = 0;
        while d.swing != DoorSwing::Shut {
            shut += (d.step(0.1, None) == Some(DoorEvent::Shut)) as u32;
            t += 0.1;
        }
        assert_eq!(shut, 1, "it reaches shut once");
        assert!((t - SHUT_SECONDS).abs() < 0.11, "{t}");
        assert_eq!(d.step(1.0, Some(d.centre)), None);
        assert_eq!(d.swing, DoorSwing::Shut);
        assert!(d.rotation().abs_diff_eq(Quat::IDENTITY, 1e-6));
    }

    #[test]
    fn the_door_sounds_once_when_it_reaches_shut() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<BlockedCells>()
            .insert_resource(LevelSounds::default())
            .insert_resource(GameSettings { sound_volume: 0.5, ..default() })
            .init_resource::<Tuning>()
            .add_systems(Update, shut_the_entrance);
        let d = door();
        app.world_mut().spawn((d, Transform::default()));
        let player = app.world_mut().spawn((Player, GlobalTransform::from_translation(d.centre))).id();
        let sounds = |app: &mut App| app.world_mut().query::<&AudioPlayer>().iter(app.world()).count();
        let frame = |app: &mut App| {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(100));
            app.update();
        };
        frame(&mut app);
        assert_eq!(sounds(&mut app), 0, "open: silent");
        // The player walks in; the door swings, silent until it reaches shut.
        *app.world_mut().get_mut::<GlobalTransform>(player).unwrap() = GlobalTransform::from_translation(d.centre - Vec3::Z * 1.5 * CELL);
        let mut shut_at = None;
        for i in 0..40 {
            frame(&mut app);
            let swing = app.world_mut().query::<&EntranceDoor>().single(app.world()).unwrap().swing;
            if swing == DoorSwing::Shut && shut_at.is_none() {
                shut_at = Some(i);
                assert_eq!(sounds(&mut app), 1, "it sounds the frame it shuts");
            } else if shut_at.is_none() {
                assert_eq!(sounds(&mut app), 0, "frame {i}: silent while it swings");
            }
        }
        assert!(shut_at.is_some());
        assert_eq!(sounds(&mut app), 1, "and only once");
        let mut playback = app.world_mut().query::<&PlaybackSettings>();
        assert_eq!(playback.single(app.world()).unwrap().volume.to_linear(), 0.5, "at the sound volume");
    }

    #[test]
    fn the_door_sound_is_an_asset() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(LevelSounds::DOOR_SHUT);
        let duration = crate::audio::ogg_vorbis_duration(&std::fs::read(path).unwrap()).unwrap();
        assert!((0.3..2.0).contains(&duration), "{duration}");
    }

    #[test]
    fn the_open_door_stands_turned_into_the_level() {
        // The doorway's +Z is inward; the leaf runs along its +X when shut.
        let open = door().rotation() * Vec3::X;
        assert!(open.z > 0.95, "{open}");
    }

    #[test]
    fn a_grave_find_brazier_burns_as_bright_as_a_new_one() {
        let (brazier, _) = geometry::brazier(1207);
        let (new, grave) = (fixture_appearance(1207, ConditionState::Pristine), fixture_appearance(1207, ConditionState::GraveFind));
        let flames: Vec<_> = brazier.parts.iter().filter(|p| p.name.starts_with("flame")).collect();
        assert!(!flames.is_empty());
        for flame in flames {
            let (a, b) = (new.material_for(flame), grave.material_for(flame));
            assert_eq!(a.base.base_color, b.base.base_color, "{}", flame.name);
            assert_eq!(a.extension.glow, b.extension.glow, "{}", flame.name);
            assert_eq!(b.extension.decay.amount, 0.0, "{}", flame.name);
        }
    }

    #[test]
    fn glowcaps_light_casts_shadows() {
        assert!(glowcap_light(&LightTuning::default()).shadow_maps_enabled);
    }

    #[test]
    fn stone_wears_at_a_moderate_amount() {
        let stone = stone_appearance(1207);
        let part = armory::Part { material: Some(MaterialKind::Stone), ..default() };
        let decay = stone.material_for(&part).extension.decay;
        assert_eq!(decay.kind, 9);
        assert!((decay.amount - STONE_WEAR).abs() < 1e-6);
        assert!((decay.scale - STONE_PATTERN_SCALE).abs() < 1e-6);
    }
}
