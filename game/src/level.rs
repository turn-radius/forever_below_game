//! The level's stone geometry, fixed lights and the entrance door that shuts behind the player.
//!
//! The shapes are plain data built in [`geometry`] (no Bevy); this file spawns them in the
//! house style (`docs/rendering.md`): banded toon stone weathered by the decay shader, ink
//! along the rock's edges, timber door frames, glowcaps and braziers as the only fixed light,
//! and an ambient fill of a few lux so carried light is what matters.
//!
//! `FB_DEBUG_CAMERA=start|back|shut|top|brazier|glowcaps` is for captures: a lit camera that stands in for the
//! [`PlayerCamera`] (which it deactivates), at the start looking in, a cell in looking back at
//! the open entrance, the same with the player walked a cell further in so the door shuts, or
//! high over the level looking down, or beside the first brazier or glowcaps. Unset, nothing here touches the player's camera.

pub mod geometry;

use armory::{
    Rgb,
    cond::{Condition, ConditionState},
    families::{
        Generator,
        food::{Food, FoodStyle},
    },
};
use bevy::{camera::Exposure, light::NotShadowCaster, prelude::*, render::view::Tonemapping};
use fb_level::{CELL, LightKind};

use crate::light::{Torch, cast_shadows, point_light};
use crate::play::{BlockedCells, CurrentLevel, EYE_HEIGHT, Player, PlayerCamera};
use crate::render::{ArmoryPart, ItemAppearance, MinWidth, OutlineMaterial, ToonMaterial, part_to_mesh};
use geometry::{CellView, Doorway, LevelMesh};

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(abyss_ambient())
            .insert_resource(ClearColor(FOG_COLOR))
            .add_systems(Startup, (spawn_stone, spawn_fixtures, spawn_debug_camera))
            .add_systems(Update, (shut_the_entrance, fog_cameras, debug_camera_takes_over, unshadow_flames));
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

/// The abyss's own light: a few lux of cold grey-blue, so stone a torch has not reached is
/// a shape and not a colour.
pub const AMBIENT_COLOR: Color = Color::srgb(0.55, 0.6, 0.72);
pub const AMBIENT_LUX: f32 = 4.0;
/// The dark that swallows far stone: a near-black blue, thickening from 4 m to 26 m.
pub const FOG_COLOR: Color = Color::srgb(0.012, 0.016, 0.024);
pub const FOG_START: f32 = 40.0;
pub const FOG_END: f32 = 260.0;

/// Glowcaps: faint cold cyan, a pool reaching about three metres. Peaks are multiples of full
/// light close up (`light::peak_lumens`), reaches in world units.
pub const GLOWCAP_COLOR: Rgb = Rgb::hex(0x6ad0c8);
pub const GLOWCAP_LIGHT: Color = Color::srgb(0.45, 0.85, 0.88);
pub const GLOWCAP_PEAK: f32 = 0.5;
pub const GLOWCAP_REACH: f32 = 30.0;
/// Brazier light: the torch colour, brighter and further than a torch in hand.
pub const BRAZIER_PEAK: f32 = 1.5;
pub const BRAZIER_REACH: f32 = 150.0;

/// The stone's weathering: wear 0.5 on stone (decay type 9), its pattern a few metres across
/// in world space rather than an item's few centimetres.
pub const STONE_WEAR: f32 = 0.5;
const STONE_PATTERN_SCALE: f32 = 0.06;
/// The rock's ink: near black, a shade under the fog.
pub const STONE_INK: Color = Color::srgb(0.008, 0.009, 0.011);

fn abyss_ambient() -> GlobalAmbientLight {
    GlobalAmbientLight { color: AMBIENT_COLOR, brightness: AMBIENT_LUX, ..default() }
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
        outline: OutlineMaterial { color: STONE_INK.into(), width_px: 1.8, max_width: 0.6 },
        ..default()
    }
}

/// The look of door frames, the entrance door and braziers: worn items of the level, with
/// hulls allowed to grow to a few centimetres, as befits things of their size.
fn fixture_appearance(seed: u32, state: ConditionState) -> ItemAppearance {
    ItemAppearance {
        condition: Condition::of(state),
        seed,
        outline: OutlineMaterial { color: STONE_INK.into(), max_width: 0.3, ..default() },
        ..default()
    }
}

/// Marks every entity the level spawns.
#[derive(Component, Debug, Default)]
pub struct LevelPiece;

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
    /// Returns true on the frame it begins to shut.
    pub fn step(&mut self, dt: f32, feet: Option<Vec3>) -> bool {
        match self.swing {
            DoorSwing::Open if feet.is_some_and(|f| self.player_is_clear(f)) => {
                self.swing = DoorSwing::Closing(0.0);
                true
            }
            DoorSwing::Closing(t) if t + dt >= SHUT_SECONDS => {
                self.swing = DoorSwing::Shut;
                false
            }
            DoorSwing::Closing(t) => {
                self.swing = DoorSwing::Closing(t + dt);
                false
            }
            _ => false,
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
    let ink = outline_materials.add(stone.outline);
    let mut materials: Vec<(&'static str, Handle<ToonMaterial>)> = Vec::new();
    for range in geometry::chunks(&view, CHUNK_CELLS) {
        let LevelMesh { surfaces, ink: lines } = geometry::level_mesh(&view, range);
        for part in &surfaces.parts {
            let material = match materials.iter().find(|(name, _)| *name == part.name) {
                Some((_, handle)) => handle.clone(),
                None => {
                    let mut material = stone.material_for(part);
                    // Rock is never a fine part to fatten.
                    material.extension.min_width = MinWidth::default();
                    let handle = toon_materials.add(material);
                    materials.push((part.name, handle.clone()));
                    handle
                }
            };
            commands.spawn((LevelPiece, Name::new(part.name), Mesh3d(meshes.add(part_to_mesh(part))), MeshMaterial3d(material)));
        }
        if !lines.indices.is_empty() {
            commands.spawn((LevelPiece, Name::new("ink"), Mesh3d(meshes.add(part_to_mesh(&lines))), MeshMaterial3d(ink.clone())));
        }
    }

}

fn spawn_fixtures(mut commands: Commands, level: Res<CurrentLevel>, mut spawner: crate::render::ItemSpawner) {
    let level = &level.0;
    let view = CellView::new(level);

    // Door frames, and the entrance door hung in its own.
    let entrance = (level.entrance.x as i32, level.entrance.y as i32);
    for doorway in geometry::doorways(&view) {
        let seed = level.seed ^ ((doorway.x as u32) << 16 | doorway.y as u32);
        let frame = geometry::door_frame(level.ceiling, seed);
        let transform = doorway_transform(&doorway);
        spawner.spawn(&frame, &fixture_appearance(seed, ConditionState::BattleWorn)).insert((LevelPiece, transform));
        if (doorway.x, doorway.y) == entrance {
            let hinge = transform.transform_point(v3(geometry::entrance_hinge()));
            let door = EntranceDoor { shut: transform.rotation, centre: transform.translation, cell: entrance, swing: DoorSwing::Open };
            let leaf = geometry::entrance_leaf(seed);
            spawner
                .spawn(&leaf, &fixture_appearance(seed, ConditionState::Serviceable))
                .insert((LevelPiece, Name::new("entrance door"), door, Transform::from_translation(hinge).with_rotation(door.rotation())));
        }
    }

    // Fixed lights.
    for (i, spot) in level.lights.iter().enumerate() {
        let at = v3(fb_level::Level::world(spot.x, spot.y));
        let seed = level.seed.wrapping_add(0x11 * i as u32 + 1);
        match spot.kind {
            LightKind::Glowcaps => spawn_glowcaps(&mut commands, &mut spawner, at, seed),
            LightKind::Brazier => {
                let (mesh, flame) = geometry::brazier(seed);
                spawner.spawn(&mesh, &fixture_appearance(seed, ConditionState::BattleWorn)).insert((LevelPiece, Transform::from_translation(at)));
                let (torch, light) = Torch::light(BRAZIER_PEAK, BRAZIER_REACH, &EXPOSURE);
                let light = cast_shadows(light);
                commands.spawn((LevelPiece, Name::new("brazier light"), torch, light, Transform::from_translation(at + Vec3::Y * flame)));
            }
        }
    }
}

/// A patch of glowcaps: three clusters round the spot, and one faint steady light over them.
fn spawn_glowcaps(commands: &mut Commands, spawner: &mut crate::render::ItemSpawner, at: Vec3, seed: u32) {
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
            .insert((LevelPiece, Transform::from_translation(at + offset).with_rotation(Quat::from_rotation_y(angle))));
    }
    let centre = light_at.iter().sum::<Vec3>() / light_at.len() as f32;
    commands.spawn((LevelPiece, Name::new("glowcap light"), glowcap_light(), Transform::from_translation(centre + Vec3::Y * 1.5)));
}

/// The faint light over a patch of glowcaps. Shadowed like every light in the level, so it
/// stays in the cells it can see; the caps themselves cast none (`unshadow_flames`).
pub fn glowcap_light() -> PointLight {
    cast_shadows(point_light(GLOWCAP_LIGHT, GLOWCAP_PEAK, GLOWCAP_REACH, &EXPOSURE))
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

fn shut_the_entrance(
    time: Res<Time>,
    players: Query<&GlobalTransform, With<Player>>,
    mut doors: Query<(&mut EntranceDoor, &mut Transform)>,
    mut blocked: ResMut<BlockedCells>,
) {
    let feet = players.iter().next().map(GlobalTransform::translation);
    for (mut door, mut transform) in &mut doors {
        if door.swing == DoorSwing::Shut {
            continue;
        }
        if door.step(time.delta_secs(), feet) {
            // Blocked from the moment it swings, so the player cannot slip back under it.
            blocked.0.insert(door.cell);
        }
        transform.rotation = door.rotation();
    }
}

/// A first-person or capture camera just spawned, with no fog yet.
type NewUnfoggedCamera = (Or<(Added<PlayerCamera>, Added<DebugCamera>)>, Without<DistanceFog>);

/// Lays the abyss's dark over every first-person or debug camera.
fn fog_cameras(
    mut commands: Commands,
    cameras: Query<(Entity, Option<&DebugCamera>), NewUnfoggedCamera>,
) {
    for (camera, debug) in &cameras {
        if debug.is_some_and(|d| !d.fog) {
            continue;
        }
        commands.entity(camera).insert(DistanceFog {
            color: FOG_COLOR,
            directional_light_color: Color::NONE,
            falloff: FogFalloff::Linear { start: FOG_START, end: FOG_END },
            ..default()
        });
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
            LevelPiece,
            Camera3d::default(),
            EXPOSURE,
            // The house style draws untonemapped; a tone curve would flatten the bands.
            Tonemapping::Linear,
            transform,
        ))
        // Something to see by until the player brings a torch.
        .with_child((Torch::light(1.0, 120.0, &EXPOSURE), Transform::from_xyz(4.0, -3.0, 2.0)));
    if mode == "top" {
        commands.insert_resource(GlobalAmbientLight { brightness: 120.0, ..abyss_ambient() });
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
        assert!(!d.step(1.0, None));
        assert!(!d.step(1.0, Some(d.centre + Vec3::new(0.0, 0.0, -CELL))));
        assert_eq!(d.swing, DoorSwing::Open);
        // A step further in and it begins to swing, then shuts after SHUT_SECONDS and stays.
        assert!(d.step(0.1, Some(d.centre + Vec3::new(0.0, 0.0, -1.3 * CELL))));
        let mut t = 0.0;
        while d.swing != DoorSwing::Shut {
            assert!(!d.step(0.1, None));
            t += 0.1;
        }
        assert!((t - SHUT_SECONDS).abs() < 0.11, "{t}");
        assert!(!d.step(1.0, Some(d.centre)));
        assert_eq!(d.swing, DoorSwing::Shut);
        assert!(d.rotation().abs_diff_eq(Quat::IDENTITY, 1e-6));
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
        assert!(glowcap_light().shadow_maps_enabled);
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
