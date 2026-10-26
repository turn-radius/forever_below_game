//! The level's stone geometry, fixed lights and the entrance door that shuts behind the player.
//!
//! The shapes are plain data built in [`geometry`] (no Bevy); this file spawns them in the
//! house style (`docs/rendering.md`): banded toon stone weathered by the decay shader, ink
//! along the rock's edges, timber door frames, glowcaps and braziers as the only fixed light,
//! and an ambient fill of a few lux so carried light is what matters. The entrance door slams
//! and latches when it reaches shut ([`LevelSounds`]).
//!
//! `FB_DEBUG_CAMERA=start|back|shut|top|brazier|glowcaps|hall|vault|role|banner|cells|wide` is for captures: a lit camera that stands in for the
//! [`PlayerCamera`] (which it deactivates), at the start looking in, a cell in looking back at
//! the open entrance, the same with the player walked a cell further in so the door shuts, or
//! high over the level looking down, or beside the first brazier or glowcaps, or at one end of
//! the first colonnaded hall looking down it (`hall`) or up at its vault (`vault`), or in the first room with the role `FB_DEBUG_ROLE` (`hall`, `hearth`, `sleeping`, `market`, `store`) or the room `FB_DEBUG_ROOM` looking at its middle (`role`), or the banner `FB_DEBUG_BANNER=<n>` names from in front of it (`banner`), or in the aisle of the first cell block looking at its bars (`cells`), or at the end of the longest stretch of corridor two cells wide or more, looking down it (`wide`). Unset, nothing here touches the player's camera.
//! `FB_DEPTH=2` plays the stone halls of depth 2.

pub mod geometry;
pub mod stairs;

use armory::{
    ItemMesh, MaterialKind, Rgb,
    cond::{Condition, ConditionState},
    families::{
        Generator,
        food::{Food, FoodStyle},
    },
};
use bevy::{
    camera::{
        Exposure, ShadowLodOrigin,
        visibility::{RenderLayers, VisibilityRange},
    },
    light::NotShadowCaster,
    prelude::*,
    render::view::Tonemapping,
};
use fb_level::{CELL, Level, LightKind, StairKind};

use crate::audio::sound_effect;
use crate::capture::FrameCapturePlugin;
use crate::light::{Torch, cast_shadows, point_light};
use crate::play::DelveSystems;
use crate::play::{BlockedCells, CurrentLevel, EYE_HEIGHT, Player, PlayerCamera};
use crate::render::{
    ArmoryItem, ArmoryPart, InkOutline, ItemAppearance, MinWidth, OutlineMaterial, ToonMaterial,
    part_to_mesh,
};
use crate::settings::GameSettings;
use crate::travel::{Abyss, Arrival};
use crate::tuning::{LightTuning, Tuning};
use fb_level::LevelStyle;
use geometry::{CellView, Doorway, LevelMesh};

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::doors::DoorsPlugin)
            .init_resource::<Tuning>()
            .init_resource::<PreparedStone>()
            .insert_resource(abyss_ambient(&LightTuning::default()))
            .insert_resource(ClearColor(FOG_COLOR))
            // The level's stone is laid out for each level the delver comes to: the first of a
            // delve, then each one they travel to ([`crate::travel`]), as the folk and props are.
            .add_systems(
                Update,
                (
                    clear_stone,
                    spawn_stone,
                    spawn_columns,
                    spawn_bars,
                    spawn_corridor_arches,
                    spawn_fixtures,
                    spawn_stairs,
                    spawn_debug_camera,
                )
                    .chain()
                    .run_if(resource_exists_and_changed::<CurrentLevel>)
                    .in_set(DelveSystems),
            )
            // Offscreen captures stay silent, as the music does.
            .add_systems(
                Startup,
                load_level_sounds.run_if(not(resource_exists::<FrameCapturePlugin>)),
            )
            .add_systems(
                Update,
                (
                    shut_the_entrance,
                    fog_cameras,
                    debug_camera_takes_over,
                    unshadow_flames,
                )
                    .in_set(DelveSystems),
            )
            // After the render layers have propagated down, which `Update` does.
            .add_systems(
                PostUpdate,
                limit_far_parts
                    .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
            )
            .add_systems(
                Update,
                apply_light_tuning
                    .run_if(resource_changed::<Tuning>)
                    .in_set(DelveSystems),
            );
    }
}

/// Cells of the level drawn as one mesh per surface, so far stone can be culled.
pub const CHUNK_CELLS: i32 = 16;

/// The stone of `level` as plain mesh data, one [`LevelMesh`] per chunk. This is the costly part
/// of laying a level out (corner shading included), and it has no Bevy in it, so it can be made
/// on another thread ahead of time ([`crate::travel`]).
pub fn stone_chunks(level: &Level) -> Vec<LevelMesh> {
    let view = CellView::new(level);
    geometry::chunks(&view, CHUNK_CELLS)
        .into_iter()
        .map(|range| geometry::level_mesh(&view, range))
        .collect()
}

/// The stone of a level made ahead of time: [`spawn_stone`] uses it when it is the stone of the
/// level coming in, and makes its own otherwise.
#[derive(Resource, Debug, Default)]
pub struct PreparedStone {
    ready: Option<(u32, u32, Vec<LevelMesh>)>,
    /// How many times it was used, which tests read to see the stone was made ahead.
    pub used: u32,
}

impl PreparedStone {
    /// Keeps `chunks` as the stone of the level of `seed` at `depth`.
    pub fn set(&mut self, seed: u32, depth: u32, chunks: Vec<LevelMesh>) {
        self.ready = Some((seed, depth, chunks));
    }

    /// The stone made ahead for `level`, if that is the level it was made for. Whatever was
    /// kept is used up either way.
    fn take_for(&mut self, level: &Level) -> Option<Vec<LevelMesh>> {
        let chunks = self
            .ready
            .take()
            .filter(|(seed, depth, _)| (*seed, *depth) == (level.seed, level.depth))
            .map(|(_, _, chunks)| chunks);
        self.used += u32::from(chunks.is_some());
        chunks
    }

    /// Forgets what was kept.
    pub fn clear(&mut self) {
        self.ready = None;
    }

    /// Whether stone is kept, not yet used.
    pub fn is_kept(&self) -> bool {
        self.ready.is_some()
    }
}

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

/// How far past the fog's end (world units, 3 m) a thing's centre may lie and still be drawn:
/// room for a big prop or a creature whose near edge is inside the fog while its middle is not.
const DRAW_MARGIN: f32 = 30.0;

/// The distance past which nothing is drawn: the fog's end (`fog_end`, 26 m as shipped) and a
/// margin. The player camera's far plane sits here and so do the [`VisibilityRange`]s of
/// props, items, creatures and folk, so what the fog has swallowed is not drawn at all.
pub fn draw_end(light: &LightTuning) -> f32 {
    light.fog_end + DRAW_MARGIN
}

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
    /// A hearth's lit campfire (`fb_level::Prop::Campfire` stands at it).
    Campfire,
}

impl FixedLight {
    /// Its light, as tuned.
    pub fn light(self, tuning: &LightTuning) -> PointLight {
        match self {
            // A hearth's campfire lights like a hall's brazier, so the two fires match.
            FixedLight::Brazier | FixedLight::Campfire => cast_shadows(Torch::flickering(
                Torch::light(tuning.brazier_peak, tuning.brazier_reach, &EXPOSURE),
                tuning.flicker_depth,
            )),
            FixedLight::Glowcaps => glowcap_light(tuning),
        }
    }
}

/// The stone's weathering: wear 0.5 on stone (decay type 9), its pattern a few metres across
/// in world space rather than an item's few centimetres.
pub const STONE_WEAR: f32 = 0.5;
/// Decay types for cave rock and layered cave rock (`decay.wesl`): stone that also shades as
/// facets or beds.
const DECAY_ROCK: u32 = 14;
const DECAY_STRATA: u32 = 15;
const STONE_PATTERN_SCALE: f32 = 0.06;
/// How much finer the decay pattern runs over a stone hall's ceiling than over its walls.
const CEILING_PATTERN_FINER: f32 = 2.6;
/// The rock's ink: near black, a shade under the fog.
pub const STONE_INK: Color = Color::srgb(0.008, 0.009, 0.011);
/// The finer ink along dressed stone's courses and joints: the same dark, a little softer.
const STONE_JOINT_INK: Color = Color::srgb(0.02, 0.022, 0.026);

fn abyss_ambient(tuning: &LightTuning) -> GlobalAmbientLight {
    GlobalAmbientLight {
        color: AMBIENT_COLOR,
        brightness: tuning.ambient_lux,
        ..default()
    }
}

/// The abyss's fog, as tuned.
fn abyss_fog(tuning: &LightTuning) -> DistanceFog {
    DistanceFog {
        color: FOG_COLOR,
        directional_light_color: Color::NONE,
        falloff: FogFalloff::Linear {
            start: tuning.fog_start,
            end: tuning.fog_end,
        },
        ..default()
    }
}

/// Brings the ambient, the fog on every camera and the fixed lights up to the tuning.
fn apply_light_tuning(
    tuning: Res<Tuning>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut fogs: Query<&mut DistanceFog>,
    mut lights: Query<(&FixedLight, &mut PointLight)>,
    mut ranges: Query<&mut VisibilityRange>,
    debug: Query<&DebugCamera>,
) {
    let light = &tuning.light;
    // The view from above has its own bright fill.
    if debug.iter().all(|d| d.fog) {
        ambient.brightness = light.ambient_lux;
    }
    for mut fog in &mut fogs {
        fog.falloff = FogFalloff::Linear {
            start: light.fog_start,
            end: light.fog_end,
        };
    }
    for (kind, mut point) in &mut lights {
        *point = kind.light(light);
    }
    for mut range in &mut ranges {
        *range = draw_range(light);
    }
}

/// Where a part is drawn: from the camera out to [`draw_end`], measured to the middle of its
/// box. The edge is abrupt, since the fog has already faded what lies there to nothing.
fn draw_range(light: &LightTuning) -> VisibilityRange {
    VisibilityRange {
        use_aabb: true,
        ..VisibilityRange::abrupt(0.0, draw_end(light))
    }
}

/// A part of the level's world: not drawn by a camera of its own (an inventory icon, the doll
/// or the held item's stage), which a part's [`RenderLayers`] tell once they have propagated
/// down to it.
type WorldPart = (
    Or<(With<ArmoryPart>, With<InkOutline>)>,
    Without<RenderLayers>,
);

/// Bevy tracks the draw ranges of only the first 32 cameras (`check_visibility_ranges` takes 32),
/// and culls a ranged thing for any view past them.
const RANGED_VIEWS: usize = 32;

/// Gives each part of a prop, item, creature or folk in the world a [`draw_range`], so a camera
/// does not queue what the fog hides. The level's own stone is left to the camera's far plane.
/// It leaves out:
/// - the parts drawn by other cameras (icons, the doll, the held item), by their render layers.
///   Those arrive a frame after a part does, so a part is given its range a frame after it
///   appears;
/// - everything while the view from above (`FB_DEBUG_CAMERA=top`), which looks from further
///   than the fog, is up;
/// - everything while there are more cameras than Bevy tracks ranges for (a full inventory
///   makes up to 64 icon cameras), so the player's view is never one of those that culls them.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn limit_far_parts(
    mut commands: Commands,
    tuning: Res<Tuning>,
    new: Query<Entity, Or<(Added<ArmoryPart>, Added<InkOutline>)>>,
    parts: Query<(Entity, Has<VisibilityRange>), WorldPart>,
    views: Query<(), Or<(With<Camera>, With<ShadowLodOrigin>)>>,
    debug: Query<&DebugCamera>,
    mut waiting: Local<Vec<Entity>>,
    mut limiting: Local<bool>,
) {
    let limit = views.iter().count() <= RANGED_VIEWS && debug.iter().all(|d| d.fog);
    let due = std::mem::take(&mut *waiting);
    waiting.extend(new.iter());
    if limit != *limiting {
        *limiting = limit;
        for (part, ranged) in &parts {
            if limit && !ranged && !waiting.contains(&part) && !due.contains(&part) {
                commands.entity(part).insert(draw_range(&tuning.light));
            } else if !limit && ranged {
                commands.entity(part).remove::<VisibilityRange>();
            }
        }
    }
    if limit {
        for part in due {
            if let Ok((part, false)) = parts.get(part) {
                commands.entity(part).insert(draw_range(&tuning.light));
            }
        }
    }
}

/// The look of the level's own stone and fixtures.
fn stone_appearance(seed: u32) -> ItemAppearance {
    let condition = Condition {
        wear: STONE_WEAR,
        rot: 1.0,
        ..Condition::of(ConditionState::Neglected)
    };
    ItemAppearance {
        condition,
        seed,
        decay_scale_factor: STONE_PATTERN_SCALE / condition.scale,
        // The rock's ink is drawn by its own ribbons, wider than an item's hull may grow, and
        // darker than the Armory's, so it never shows lighter than unlit stone in the dark.
        outline: OutlineMaterial {
            color: STONE_INK.into(),
            width_px: 1.8,
            max_width: 0.6,
            ..default()
        },
        ..default()
    }
}

/// The look of door frames, the entrance door, braziers and scenery: worn items of the level,
/// with hulls allowed to grow to a few centimetres, as befits things of their size.
pub fn fixture_appearance(seed: u32, state: ConditionState) -> ItemAppearance {
    ItemAppearance {
        condition: Condition::of(state),
        seed,
        outline: OutlineMaterial {
            color: STONE_INK.into(),
            max_width: 0.3,
            ..default()
        },
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
    commands.insert_resource(LevelSounds {
        door_shut: assets.load(LevelSounds::DOOR_SHUT),
    });
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
    mut prepared: ResMut<PreparedStone>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_materials: ResMut<Assets<ToonMaterial>>,
    mut outline_materials: ResMut<Assets<OutlineMaterial>>,
) {
    let level = &level.0;
    let chunks = prepared
        .take_for(level)
        .unwrap_or_else(|| stone_chunks(level));
    let stone = stone_appearance(level.seed);

    // The rock: one entity per surface per chunk, each surface kind sharing one material.
    let fine_ink = outline_materials.add(OutlineMaterial {
        color: STONE_JOINT_INK.into(),
        width_px: 1.0,
        ..stone.outline
    });
    let ink = outline_materials.add(stone.outline);
    let mut materials: Vec<(&'static str, Handle<ToonMaterial>)> = Vec::new();
    for LevelMesh {
        surfaces,
        ink: lines,
        joints,
    } in &chunks
    {
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
                    // Only hewn walls break into facets under the light, and layered ones into beds.
                    // Plain cave rock keeps plain stone's wear and shading, as it did before the
                    // levels' characters, and so do laid stones, which read apart from the rock.
                    if level.style == LevelStyle::RoughRock {
                        match part.name {
                            "strata" => material.extension.decay.kind = DECAY_STRATA,
                            "hewn" => material.extension.decay.kind = DECAY_ROCK,
                            _ => {}
                        }
                    }
                    let handle = toon_materials.add(material);
                    materials.push((part.name, handle.clone()));
                    handle
                }
            };
            commands.spawn((
                StonePiece,
                Name::new(part.name),
                Mesh3d(meshes.add(part_to_mesh(part))),
                MeshMaterial3d(material),
            ));
        }
        if !lines.indices.is_empty() {
            commands.spawn((
                StonePiece,
                Name::new("ink"),
                Mesh3d(meshes.add(part_to_mesh(lines))),
                MeshMaterial3d(ink.clone()),
            ));
        }
        if !joints.indices.is_empty() {
            commands.spawn((
                StonePiece,
                Name::new("joints"),
                Mesh3d(meshes.add(part_to_mesh(joints))),
                MeshMaterial3d(fine_ink.clone()),
            ));
        }
    }
}

/// The columns of a hall: one mesh for each shape (height and variant) shared by every column
/// of that shape, so a colonnade costs a few meshes however long it is. Each is turned a
/// quarter at a time so repeats do not wear alike.
#[allow(clippy::type_complexity)]
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
    let finishes = geometry::level_finishes(level);
    let ink = outline_materials.add(stone.outline);
    let mut shapes: std::collections::HashMap<
        (i32, i32, u32, u32),
        Vec<(&'static str, Handle<Mesh>, Handle<ToonMaterial>, bool)>,
    > = Default::default();
    // One material for every shape's part of a name: they differ only in their meshes.
    let mut materials: std::collections::HashMap<&'static str, Handle<ToonMaterial>> =
        Default::default();
    for spot in spots {
        // A column is one moss all over, in quarters, so columns of a shape still share a mesh.
        let [cx, _, cz] = geometry::cell_centre(spot.cell.0, spot.cell.1);
        let moss = (finishes.moss_at(cx, cz) * 4.0).round() as u32;
        let parts = shapes
            .entry((spot.seat as i32, spot.top as i32, spot.variant, moss))
            .or_insert_with(|| {
                mossy_column(&spot, level.seed, moss)
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
                        (
                            part.name,
                            meshes.add(part_to_mesh(part)),
                            material,
                            part.outline,
                        )
                    })
                    .collect()
            });
        let at = v3(geometry::cell_centre(spot.cell.0, spot.cell.1));
        let turn = Quat::from_rotation_y(spot.turn as f32 * std::f32::consts::FRAC_PI_2);
        let root = commands
            .spawn((
                StonePiece,
                ArmoryItem,
                Name::new("column"),
                Transform::from_translation(at).with_rotation(turn),
                Visibility::default(),
            ))
            .id();
        for (name, mesh, material, outlined) in parts.iter() {
            let part = commands
                .spawn((
                    ArmoryPart,
                    Name::new(*name),
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    ChildOf(root),
                ))
                .id();
            if *outlined {
                commands.spawn((
                    InkOutline,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(ink.clone()),
                    ChildOf(part),
                ));
            }
        }
    }
}

/// The iron bars of the cell blocks, and their gates standing ajar. Each Bars cell and each gate
/// frame is a piece of its own, since the wear and the vault over them differ cell by cell.
fn spawn_bars(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_materials: ResMut<Assets<ToonMaterial>>,
    mut outline_materials: ResMut<Assets<OutlineMaterial>>,
) {
    let level = &level.0;
    let (bars, gates) = (geometry::bars::bars(level), geometry::bars::gates(level));
    if bars.is_empty() && gates.is_empty() {
        return;
    }
    let ceiling = geometry::bars::Ceiling::new(level);
    let ink =
        outline_materials.add(fixture_appearance(level.seed, ConditionState::BattleWorn).outline);
    // One set of materials for each keep of iron: a cell block's bars all match.
    let mut materials: std::collections::HashMap<
        (geometry::bars::Keep, &'static str),
        Handle<ToonMaterial>,
    > = Default::default();
    let mut attach = |commands: &mut Commands,
                      item: &armory::ItemMesh,
                      root: Entity,
                      keep: geometry::bars::Keep| {
        let iron = ItemAppearance {
            condition: keep.condition(),
            ..fixture_appearance(level.seed, ConditionState::Serviceable)
        };
        for part in &item.parts {
            if part.positions.is_empty() {
                continue;
            }
            let material = materials
                .entry((keep, part.name))
                .or_insert_with(|| {
                    let mut material = iron.material_for(part);
                    material.extension.min_width = MinWidth::default();
                    toon_materials.add(material)
                })
                .clone();
            let mesh = meshes.add(part_to_mesh(part));
            let entity = commands
                .spawn((
                    ArmoryPart,
                    Name::new(part.name),
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material),
                    ChildOf(root),
                ))
                .id();
            if part.outline {
                commands.spawn((
                    InkOutline,
                    Mesh3d(mesh),
                    MeshMaterial3d(ink.clone()),
                    ChildOf(entity),
                ));
            }
        }
    };
    for spot in bars {
        let item = geometry::bars::bars_mesh(&spot, level, &ceiling);
        let at = v3(geometry::cell_centre(spot.cell.0, spot.cell.1));
        let root = commands
            .spawn((
                StonePiece,
                ArmoryItem,
                Name::new("bars"),
                Transform::from_translation(at),
                Visibility::default(),
            ))
            .id();
        attach(
            &mut commands,
            &item,
            root,
            geometry::bars::Keep::of(level, spot.cell),
        );
    }
    for spot in gates {
        let gate = geometry::bars::gate_mesh(&spot, level, &ceiling);
        let at = v3(geometry::cell_centre(spot.cell.0, spot.cell.1));
        let turn = Quat::from_rotation_y(geometry::bars::gate_turn(&spot));
        let root = commands
            .spawn((
                StonePiece,
                ArmoryItem,
                Name::new("gate"),
                Transform::from_translation(at).with_rotation(turn),
                Visibility::default(),
            ))
            .id();
        let keep = geometry::bars::Keep::of(level, spot.cell);
        attach(&mut commands, &gate.frame, root, keep);
        let leaf = commands
            .spawn((
                Name::new("gate leaf"),
                Transform::from_translation(v3(gate.hinge))
                    .with_rotation(Quat::from_rotation_y(geometry::bars::leaf_swing(&spot))),
                Visibility::default(),
                ChildOf(root),
            ))
            .id();
        attach(&mut commands, &gate.leaf, leaf, keep);
    }
}

/// The arches that span the corridors of the halls' vaulted quarters: one mesh per rib shape,
/// shared by every arch, turned to run across its corridor, so a long corridor of arches costs
/// the same as one.
fn spawn_corridor_arches(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_materials: ResMut<Assets<ToonMaterial>>,
    mut outline_materials: ResMut<Assets<OutlineMaterial>>,
) {
    let level = &level.0;
    let arches = geometry::arch::corridor_arches(level, &geometry::level_finishes(level));
    let Some(soffit) = arches.first().map(|a| a.soffit) else {
        return;
    };
    let stone = stone_appearance(level.seed);
    let ink = outline_materials.add(stone.outline);
    let mut shapes = Vec::new();
    for pointed in [false, true] {
        let Some(shape) = geometry::arch::corridor_arch(soffit, level.seed, pointed) else {
            continue;
        };
        let parts: Vec<_> = shape
            .parts
            .iter()
            .map(|part| {
                let mut material = stone.material_for(part);
                material.extension.min_width = MinWidth::default();
                (
                    part.name,
                    meshes.add(part_to_mesh(part)),
                    toon_materials.add(material),
                    part.outline,
                )
            })
            .collect();
        shapes.push((pointed, parts));
    }
    for arch in arches {
        let Some((_, parts)) = shapes.iter().find(|(pointed, _)| *pointed == arch.pointed) else {
            continue;
        };
        let at = v3(geometry::cell_centre(arch.cell.0, arch.cell.1));
        // The shape runs along z; a corridor along x turns it a quarter.
        let turn = Quat::from_rotation_y(if arch.along_x {
            std::f32::consts::FRAC_PI_2
        } else {
            0.0
        });
        let root = commands
            .spawn((
                StonePiece,
                ArmoryItem,
                Name::new("corridor arch"),
                Transform::from_translation(at).with_rotation(turn),
                Visibility::default(),
            ))
            .id();
        for (name, mesh, material, outlined) in parts.iter() {
            let part = commands
                .spawn((
                    ArmoryPart,
                    Name::new(*name),
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    ChildOf(root),
                ))
                .id();
            if *outlined {
                commands.spawn((
                    InkOutline,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(ink.clone()),
                    ChildOf(part),
                ));
            }
        }
    }
}

/// The column at `spot` with `quarters` quarters of full moss on every vertex.
fn mossy_column(spot: &geometry::stone::ColumnSpot, seed: u32, quarters: u32) -> ItemMesh {
    let mut column = geometry::stone::column(spot.seat, spot.top, spot.variant, seed);
    for part in &mut column.parts {
        part.uvs_b = vec![[quarters as f32 / 4.0, 0.0]; part.positions.len()];
    }
    column
}

/// The shapes of a level's doorways and braziers: each doorway with its frame and, at the
/// entrance, the door's leaf; for each fixed light a brazier's body and flame height, `None` for
/// the other kinds. Plain data, so it can be made ahead of time ([`crate::staged`]).
#[derive(Debug, PartialEq)]
pub struct FixtureShapes {
    pub doors: Vec<(Doorway, ItemMesh, Option<ItemMesh>, Option<ClaimFit>)>,
    pub braziers: Vec<Option<(ItemMesh, f32)>>,
}

/// A claim door's shapes, built to its doorway's opening: the leaf, what stays fixed beside it,
/// and the switch on the claim's side.
#[derive(Debug, PartialEq)]
pub struct ClaimFit {
    pub kind: fb_level::DoorKind,
    pub switch: fb_level::SwitchKind,
    pub leaf: geometry::door::ClaimLeaf,
    /// The lever's plate or the chain's housing, and the lever's handle or the chain and ring.
    pub fixture: ItemMesh,
    pub moving: ItemMesh,
}

impl ClaimFit {
    pub fn make(
        level: &Level,
        door: &fb_level::ClaimDoor,
        style: fb_level::DoorStyle,
        seed: u32,
    ) -> Self {
        let spec = geometry::door::DoorSpec::rolled(door.kind, style, level.style, seed);
        let (fixture, moving) = match door.switch {
            fb_level::SwitchKind::Lever => (
                geometry::door::lever_plate(seed),
                geometry::door::lever_handle(),
            ),
            fb_level::SwitchKind::Chain => (
                geometry::door::chain_housing(seed),
                geometry::door::chain_pull(seed),
            ),
        };
        ClaimFit {
            kind: door.kind,
            switch: door.switch,
            leaf: geometry::door::claim_leaf(&spec),
            fixture,
            moving,
        }
    }
}

impl FixtureShapes {
    pub fn make(level: &Level) -> Self {
        let view = CellView::new(level);
        let entrance = (level.entrance.x as i32, level.entrance.y as i32);
        let claim_doors = level.claim_doors();
        let doors = geometry::doorways(&view)
            .into_iter()
            .map(|doorway| {
                let seed = door_seed(level, &doorway);
                let style = view
                    .finishes()
                    .door_style(doorway.x, doorway.y)
                    .unwrap_or(fb_level::DoorStyle::Timber);
                let mut frame = match level.style {
                    LevelStyle::RoughRock => geometry::door_frame(level.ceiling, style, seed),
                    LevelStyle::StoneHalls => {
                        geometry::stone::door_frame(level, (doorway.x, doorway.y), style, seed)
                    }
                };
                // The masonry round the frame grows moss as the wall beside it does; the timber does not.
                let [across, up, through] = doorway.basis();
                let centre = geometry::cell_centre(doorway.x, doorway.y);
                for part in frame
                    .parts
                    .iter_mut()
                    .filter(|p| p.material == Some(MaterialKind::Stone))
                {
                    part.uvs_b = part
                        .positions
                        .iter()
                        .map(|p| {
                            let world = [0, 1, 2].map(|k| {
                                centre[k] + across[k] * p[0] + up[k] * p[1] + through[k] * p[2]
                            });
                            [view.finishes().moss_at(world[0], world[2]), 0.0]
                        })
                        .collect();
                }
                // The entrance hangs a door; each door on the edge of a claim hangs one of its kind.
                let leaf =
                    ((doorway.x, doorway.y) == entrance).then(|| geometry::entrance_leaf(seed));
                let claim = claim_doors
                    .iter()
                    .find(|d| (d.x, d.y) == (doorway.x, doorway.y))
                    .map(|d| ClaimFit::make(level, d, style, seed));
                (doorway, frame, leaf, claim)
            })
            .collect();
        let braziers = level
            .lights
            .iter()
            .enumerate()
            .map(|(i, spot)| {
                (spot.kind == LightKind::Brazier).then(|| geometry::brazier(light_seed(level, i)))
            })
            .collect();
        FixtureShapes { doors, braziers }
    }
}

fn door_seed(level: &Level, doorway: &Doorway) -> u32 {
    level.seed ^ ((doorway.x as u32) << 16 | doorway.y as u32)
}

fn light_seed(level: &Level, index: usize) -> u32 {
    level.seed.wrapping_add(0x11 * index as u32 + 1)
}

fn spawn_fixtures(
    mut commands: Commands,
    level: Res<CurrentLevel>,
    abyss: Option<Res<Abyss>>,
    tuning: Res<Tuning>,
    mut staged: Option<ResMut<crate::staged::Staged>>,
    mut blocked: ResMut<BlockedCells>,
    mut hung: Option<ResMut<crate::doors::Doors>>,
    record: Option<Res<crate::record::WorldRecord>>,
    mut spawner: crate::render::ItemSpawner,
) {
    let level = &level.0;
    let claim_doors = level.claim_doors();
    let saved = record.as_deref().and_then(|r| r.level(level.depth));
    if let Some(hung) = hung.as_deref_mut() {
        *hung = crate::doors::Doors::default();
    }
    // The door shut behind the delver when they came in; coming back to it from below finds it
    // shut already, and it does not swing again.
    let shut_already = abyss.is_some_and(|a| a.arrival == Arrival::FromBelow);
    let relief = geometry::Relief::new(level);
    let FixtureShapes { doors, braziers } = staged
        .as_mut()
        .and_then(|s| s.take_fixtures(level))
        .unwrap_or_else(|| FixtureShapes::make(level));

    // Door frames, and the entrance door hung in its own.
    let entrance = (level.entrance.x as i32, level.entrance.y as i32);
    for (doorway, frame, leaf, claim) in doors {
        let seed = door_seed(level, &doorway);
        let transform = doorway_transform(&doorway);
        spawner
            .spawn(
                &frame,
                &fixture_appearance(seed, ConditionState::BattleWorn),
            )
            .insert((StonePiece, transform));
        if (doorway.x, doorway.y) == entrance {
            let hinge = transform.transform_point(v3(geometry::entrance_hinge()));
            let swing = if shut_already {
                DoorSwing::Shut
            } else {
                DoorSwing::Open
            };
            if shut_already {
                blocked.0.insert(entrance);
            }
            let door = EntranceDoor {
                shut: transform.rotation,
                centre: transform.translation,
                cell: entrance,
                swing,
            };
            let leaf = leaf.expect("the entrance doorway has a leaf");
            spawner
                .spawn(
                    &leaf,
                    &fixture_appearance(seed, ConditionState::Serviceable),
                )
                .insert((
                    StonePiece,
                    Name::new("entrance door"),
                    door,
                    Transform::from_translation(hinge).with_rotation(door.rotation()),
                ));
        } else if let (Some(claim_door), Some(fit)) = (
            claim_doors
                .iter()
                .find(|d| (d.x, d.y) == (doorway.x, doorway.y)),
            claim,
        ) {
            // A door on the edge of a claim, of its kind: locked by how the faction stands (`crate::doors`).
            let cell = (doorway.x, doorway.y);
            let mut state = crate::doors::Doors::default();
            state.add(claim_door, &doorway, saved);
            let swing = state.0[&cell].swing;
            let door_state = &state.0[&cell];
            // The switch on the claim's side lets the delver out of a locked door.
            if !door_state.shared {
                if let Some(at) = crate::doors::switch_mount(
                    &transform,
                    Some(&relief),
                    level.style,
                    door_state.switch,
                    door_state.through,
                    door_state.inward,
                ) {
                    let switch = crate::doors::Switch {
                        target: crate::doors::SwitchTarget::Door(cell),
                        kind: door_state.switch,
                    };
                    spawner
                        .spawn(
                            &fit.fixture,
                            &fixture_appearance(seed, ConditionState::BattleWorn),
                        )
                        .insert((StonePiece, Name::new("door switch"), switch, at));
                    match door_state.switch {
                        fb_level::SwitchKind::Lever => {
                            let pivot = at.mul_transform(Transform::from_translation(Vec3::new(
                                0.0,
                                0.0,
                                geometry::door::LEVER_PIVOT,
                            )));
                            let pull = crate::doors::lever_target(door_state);
                            let handle = crate::doors::LeverHandle {
                                cell,
                                rest: at.rotation,
                                pull,
                            };
                            spawner
                                .spawn(
                                    &fit.moving,
                                    &fixture_appearance(seed, ConditionState::BattleWorn),
                                )
                                .insert((
                                    StonePiece,
                                    Name::new("door lever handle"),
                                    handle,
                                    pivot.with_rotation(handle.rotation(pull)),
                                ));
                        }
                        fb_level::SwitchKind::Chain => {
                            let rest = at.transform_point(v3(geometry::door::chain_exit()));
                            let chain = crate::doors::ChainPull {
                                cell,
                                rest,
                                down: at.rotation * Vec3::NEG_Y,
                            };
                            spawner
                                .spawn(
                                    &fit.moving,
                                    &fixture_appearance(seed, ConditionState::BattleWorn),
                                )
                                .insert((
                                    StonePiece,
                                    Name::new("door chain"),
                                    chain,
                                    Transform::from_translation(chain.translation(swing))
                                        .with_rotation(at.rotation),
                                ));
                        }
                    }
                }
            }
            // The pintles or guides stay put on the frame.
            spawner
                .spawn(
                    &fit.leaf.fixed,
                    &fixture_appearance(seed, ConditionState::BattleWorn),
                )
                .insert((StonePiece, transform));
            let hinge = transform.transform_point(v3(fit.leaf.hinge));
            let leaf_door = crate::doors::DoorLeaf {
                cell,
                shut: transform.rotation,
                hinge,
                lift: fit.leaf.lift,
                squash: fit.leaf.squash_for(level.ceiling),
            };
            spawner
                .spawn(
                    &fit.leaf.leaf,
                    &fixture_appearance(seed, ConditionState::Serviceable),
                )
                .insert((
                    StonePiece,
                    Name::new(format!("claim door, {}", fit.kind.name())),
                    leaf_door,
                    leaf_door.transform(swing),
                ));
            if let Some(hung) = hung.as_deref_mut() {
                hung.0.extend(state.0);
            }
        }
    }

    // Fixed lights.
    for (i, (spot, brazier)) in level.lights.iter().zip(braziers).enumerate() {
        let at = v3(fb_level::Level::world(spot.x, spot.y));
        let seed = light_seed(level, i);
        match spot.kind {
            LightKind::Glowcaps => {
                spawn_glowcaps(&mut commands, &mut spawner, at, seed, &tuning.light)
            }
            // The fire itself is the prop standing here; this is its light, hung at the heart of the flame.
            LightKind::Campfire => {
                let state = fb_level::props::campfire_state(fb_level::props::campfire_seed(
                    level.seed, spot.x, spot.y,
                ));
                let flame = armory::families::campfire::Campfire::light(&state)
                    .map_or(Vec3::Y * 2.0, |anchor| v3(anchor.position));
                commands.spawn((
                    StonePiece,
                    Name::new("campfire light"),
                    FixedLight::Campfire,
                    FixedLight::Campfire.light(&tuning.light),
                    Transform::from_translation(at + flame),
                ));
            }
            LightKind::Brazier => {
                let (mesh, flame) = brazier.expect("a brazier has a shape");
                spawner
                    .spawn(&mesh, &fixture_appearance(seed, ConditionState::BattleWorn))
                    .insert((StonePiece, Transform::from_translation(at)));
                let light = FixedLight::Brazier.light(&tuning.light);
                commands.spawn((
                    StonePiece,
                    Name::new("brazier light"),
                    FixedLight::Brazier,
                    light,
                    Transform::from_translation(at + Vec3::Y * flame),
                ));
            }
        }
    }
}

/// The level's stairs, each in its cell, which the delver cannot walk onto.
fn spawn_stairs(
    level: Res<CurrentLevel>,
    mut blocked: ResMut<BlockedCells>,
    mut spawner: crate::render::ItemSpawner,
) {
    let level = &level.0;
    for stair in &level.stairs {
        blocked.0.insert((stair.x as i32, stair.y as i32));
        let seed = level.seed ^ ((stair.x << 16 | stair.y) ^ 0x57A1);
        let mesh = stairs::stair_mesh(stair.kind, level.ceiling, seed);
        let (at, yaw) = stairs::stair_transform(stair);
        let name = if stair.kind == StairKind::Down {
            "stair down"
        } else {
            "stair up"
        };
        spawner
            .spawn(&mesh, &fixture_appearance(seed, ConditionState::BattleWorn))
            .insert((
                StonePiece,
                Name::new(name),
                Transform::from_translation(v3(at)).with_rotation(Quat::from_rotation_y(yaw)),
            ));
    }
}

/// A patch of glowcaps: three clusters round the spot, and one faint steady light over them.
fn spawn_glowcaps(
    commands: &mut Commands,
    spawner: &mut crate::render::ItemSpawner,
    at: Vec3,
    seed: u32,
    tuning: &LightTuning,
) {
    let mut light_at = Vec::new();
    for k in 0..3u32 {
        let mut state = Food::roll_as(seed.wrapping_add(k * 7919), FoodStyle::Mushroom);
        let p = &mut state.provision;
        p.glow = p.glow.max(0.6);
        p.outer = GLOWCAP_COLOR;
        let angle = seed as f32 * 0.37 + k as f32 * 2.2;
        let offset = if k == 0 {
            Vec3::ZERO
        } else {
            Vec3::new(angle.cos(), 0.0, angle.sin()) * (2.2 + k as f32 * 0.6)
        };
        let anchor = Food::light(&state)
            .map(|a| v3(a.position))
            .unwrap_or(Vec3::Y * 2.0);
        light_at.push(at + offset + anchor);
        let appearance = ItemAppearance {
            seed: state.seed,
            decay_scale_factor: 3.0,
            ..default()
        };
        spawner.spawn(&Food::build(&state), &appearance).insert((
            StonePiece,
            Transform::from_translation(at + offset).with_rotation(Quat::from_rotation_y(angle)),
        ));
    }
    let centre = light_at.iter().sum::<Vec3>() / light_at.len() as f32;
    commands.spawn((
        StonePiece,
        Name::new("glowcap light"),
        FixedLight::Glowcaps,
        glowcap_light(tuning),
        Transform::from_translation(centre + Vec3::Y * 1.5),
    ));
}

/// The faint light over a patch of glowcaps. Shadowed like every light in the level, so it
/// stays in the cells it can see; the caps themselves cast none (`unshadow_flames`).
pub fn glowcap_light(tuning: &LightTuning) -> PointLight {
    cast_shadows(point_light(
        GLOWCAP_LIGHT,
        tuning.glowcap_peak,
        tuning.glowcap_reach,
        &EXPOSURE,
    ))
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
                    commands.spawn((
                        Name::new("door shut"),
                        AudioPlayer::new(sounds.door_shut.clone()),
                        sound_effect(&game, &tuning),
                        DespawnOnExit(crate::launch::AppState::Delving),
                    ));
                }
            }
            None => {}
        }
        transform.rotation = door.rotation();
    }
}

/// A first-person or capture camera just spawned, with no fog yet.
type NewUnfoggedCamera = (
    Or<(Added<PlayerCamera>, Added<DebugCamera>)>,
    Without<DistanceFog>,
);

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

/// Where `FB_DEBUG_CAMERA=arch` stands to look at `doorway`: on whichever side has the longer open
/// run, two and a half cells back or as far as that run goes. Never in rock, where the one-sided
/// walls show the level from behind (#575).
fn arch_eye(level: &fb_level::Level, doorway: &geometry::Doorway) -> Vec3 {
    let eye = Vec3::Y * EYE_HEIGHT;
    let centre = v3(geometry::cell_centre(doorway.x, doorway.y));
    let through = v3(doorway.through);
    let step = |dir: Vec3, k: i32| {
        (
            doorway.x + (dir.x * k as f32).round() as i32,
            doorway.y + (dir.z * k as f32).round() as i32,
        )
    };
    let run = |dir: Vec3| {
        (1..=3)
            .take_while(|&k| level.is_open(step(dir, k).0, step(dir, k).1))
            .count()
    };
    let score = |dir: Vec3| {
        run(dir) * 2 + level.room_at(step(dir, 1).0, step(dir, 1).1).is_some() as usize * 10
    };
    let from_dir = if score(-through) >= score(through) {
        -through
    } else {
        through
    };
    centre + from_dir * CELL * (run(from_dir) as f32).min(2.5) + eye
}

/// The capture camera from `FB_DEBUG_CAMERA`, and whether its mode walks the player in.
#[derive(Component, Debug, Default)]
pub struct DebugCamera {
    walk_in: bool,
    /// Off for the view from above, which looks from further than the fog lets anything through.
    fog: bool,
}

fn spawn_debug_camera(mut commands: Commands, level: Res<CurrentLevel>) {
    let Ok(mode) = std::env::var("FB_DEBUG_CAMERA") else {
        return;
    };
    let level = &level.0;
    let (sx, sy, facing) = level.start();
    let inward = v3(geometry::facing_vector(facing));
    let start = v3(geometry::cell_centre(sx as i32, sy as i32));
    let eye = Vec3::Y * EYE_HEIGHT;
    let transform = match mode.as_str() {
        "back" | "shut" => {
            let at = start + inward * CELL * 0.9 + eye;
            Transform::from_translation(at)
                .looking_at(at - inward * CELL * 2.0 - Vec3::Y * 3.0, Vec3::Y)
        }
        "top" => {
            let centre = Vec3::new(level.width as f32, 0.0, level.height as f32) * CELL * 0.5;
            let span = level.width.max(level.height) as f32 * CELL;
            Transform::from_translation(centre + Vec3::new(0.0, span * 0.9, span * 0.5))
                .looking_at(centre, Vec3::Y)
        }
        "hall" | "vault" => {
            // The first colonnaded hall, from one end of its long axis looking down it.
            let Some(room) = level.rooms.iter().find(|r| {
                (r.y..r.y + r.h).any(|y| {
                    (r.x..r.x + r.w)
                        .any(|x| level.cell(x as i32, y as i32) == fb_level::Cell::Column)
                })
            }) else {
                return;
            };
            let (cx, cy) = room.centre();
            let (half_x, half_y) = (room.w as f32 * 0.5, room.h as f32 * 0.5);
            let along_x = room.w >= room.h;
            let (from, dir) = if along_x {
                ((cx - half_x + 0.7, cy), Vec3::X)
            } else {
                ((cx, cy - half_y + 0.7), Vec3::Z)
            };
            let at = v3(fb_level::Level::world(from.0, from.1)) + eye;
            let aim = if mode == "vault" {
                dir * 60.0 + Vec3::Y * 45.0
            } else {
                dir * 60.0
            };
            Transform::from_translation(at).looking_at(at + aim, Vec3::Y)
        }
        mode if mode.starts_with("arch") => {
            // The first doorway of the style after `arch:` (round, pointed or rock), from the
            // room side two and a half cells back, looking at the opening.
            let want = match mode.strip_prefix("arch:") {
                Some("pointed") => fb_level::DoorStyle::PointedArch,
                Some("rock") => fb_level::DoorStyle::RockCut,
                _ => fb_level::DoorStyle::RoundArch,
            };
            let finishes =
                fb_level::finishes(level, &fb_level::LevelParams::for_style(level.style));
            let view = geometry::CellView::new(level);
            // Prefer one with a room on a side, so the view is of a hall and not a passage.
            let styled: Vec<_> = geometry::doorways(&view)
                .into_iter()
                .filter(|d| finishes.door_style(d.x, d.y) == Some(want))
                .collect();
            let roomy = |d: &geometry::Doorway| {
                [-1, 1].iter().any(|&s| {
                    level
                        .room_at(
                            d.x + (d.through[0] as i32) * s,
                            d.y + (d.through[2] as i32) * s,
                        )
                        .is_some()
                })
            };
            let Some(doorway) = styled
                .iter()
                .copied()
                .find(roomy)
                .or(styled.first().copied())
            else {
                return;
            };
            let centre = v3(geometry::cell_centre(doorway.x, doorway.y));
            let at = arch_eye(level, &doorway);
            Transform::from_translation(at).looking_at(centre + Vec3::Y * 11.0, Vec3::Y)
        }
        "cells" => {
            // The aisle of the first cell block, from in front of the middle of its row of
            // gates, looking at the bars.
            let gates = geometry::bars::gates(level);
            let Some(first) = gates.first() else { return };
            let room = level.room_at(first.cell.0, first.cell.1);
            let row: Vec<_> = gates
                .iter()
                .filter(|g| level.room_at(g.cell.0, g.cell.1) == room)
                .collect();
            let gate = row[row.len() / 2];
            let centre = v3(geometry::cell_centre(gate.cell.0, gate.cell.1));
            let out = -Vec3::new(gate.into.0 as f32, 0.0, gate.into.1 as f32);
            // As far back in the aisle as it runs, up to three cells.
            let back = (1..=3)
                .take_while(|&k| {
                    level.is_open(
                        gate.cell.0 + gate.into.0 * -k,
                        gate.cell.1 + gate.into.1 * -k,
                    )
                })
                .count() as f32;
            let at = centre + out * CELL * (back - 0.45).max(0.8) + eye;
            Transform::from_translation(at).looking_at(centre + Vec3::Y * 9.0, Vec3::Y)
        }
        "corridor" => {
            // The corridor run with the most arches, from its end looking down it.
            let arches = geometry::arch::corridor_arches(level, &geometry::level_finishes(level));
            let runs = geometry::arch::corridor_runs(level);
            let count = |run: &geometry::arch::CorridorRun| {
                arches
                    .iter()
                    .filter(|a| run.cells.contains(&a.cell))
                    .count()
            };
            let Some(run) = runs.iter().max_by_key(|r| (count(r), r.cells.len())) else {
                return;
            };
            let (from, to) = (run.cells[0], *run.cells.last().unwrap());
            let dir = Vec3::new(
                (to.0 - from.0).signum() as f32,
                0.0,
                (to.1 - from.1).signum() as f32,
            );
            let dir = if dir == Vec3::ZERO {
                if run.along_x { Vec3::X } else { Vec3::Z }
            } else {
                dir
            };
            let at = v3(geometry::cell_centre(from.0, from.1)) - dir * CELL * 0.5 + eye;
            Transform::from_translation(at).looking_to(dir, Vec3::Y)
        }
        "wide" => {
            // The longest straight stretch of corridor at least two cells wide, from its end
            // looking down it along the middle of its width.
            let corridor = |x: i32, y: i32| {
                level.cell(x, y) == fb_level::Cell::Floor && level.room_at(x, y).is_none()
            };
            let mut best: Option<(usize, (i32, i32), bool)> = None;
            for along_x in [true, false] {
                let (lines, length) = if along_x {
                    (level.height as i32, level.width as i32)
                } else {
                    (level.width as i32, level.height as i32)
                };
                for line in 0..lines - 1 {
                    let mut run = 0;
                    for at in 0..=length {
                        let (x, y, nx, ny) = if along_x {
                            (at, line, at, line + 1)
                        } else {
                            (line, at, line + 1, at)
                        };
                        if at < length && corridor(x, y) && corridor(nx, ny) {
                            run += 1;
                        } else {
                            if best.is_none_or(|b| run > b.0) && run > 0 {
                                let start = if along_x {
                                    (at - run as i32, line)
                                } else {
                                    (line, at - run as i32)
                                };
                                best = Some((run, start, along_x));
                            }
                            run = 0;
                        }
                    }
                }
            }
            let Some((_, (sx, sy), along_x)) = best else {
                return;
            };
            let dir = if along_x { Vec3::X } else { Vec3::Z };
            // Between the band's two lines, at the run's first cell.
            let side = if along_x { Vec3::Z } else { Vec3::X };
            let at = v3(geometry::cell_centre(sx, sy)) + side * CELL * 0.5 - dir * CELL * 0.4 + eye;
            Transform::from_translation(at).looking_to(dir, Vec3::Y)
        }
        "cobble" | "flagstone" | "rubble" => {
            // The first room with cobbled or flagged floor, or rubble walls, looking east: down
            // at the floor ahead for the floors, level at the far wall for rubble.
            let finishes =
                fb_level::finishes(level, &fb_level::LevelParams::for_style(level.style));
            let wanted = |r: &fb_level::Room| {
                (r.y..r.y + r.h)
                    .flat_map(|y| (r.x..r.x + r.w).map(move |x| (x, y)))
                    .find_map(|(x, y)| finishes.at(x as i32, y as i32))
                    .is_some_and(|f| match mode.as_str() {
                        "cobble" => f.floor == fb_level::FloorFinish::Cobble,
                        "flagstone" => f.floor == fb_level::FloorFinish::Flagstone,
                        _ => f.walls == fb_level::WallFinish::Rubble,
                    })
            };
            let Some(room) = level.rooms.iter().find(|r| wanted(r)) else {
                return;
            };
            let (cx, cy) = room.centre();
            let from = match mode.as_str() {
                "cobble" => (room.x as f32 + 0.8, cy),
                "flagstone" => (cx - 0.7, cy),
                _ => (cx, cy),
            };
            let at = v3(fb_level::Level::world(from.0, from.1)) + eye;
            let pitch = if mode == "rubble" { -1.0 } else { -9.0 };
            Transform::from_translation(at)
                .looking_at(at + Vec3::X * 30.0 + Vec3::Y * pitch, Vec3::Y)
        }
        "web" => {
            // The first web of the kind `FB_DEBUG_WEB` names (corner, ceiling or floor), from a
            // few steps off in the direction it faces, looking at it.
            let kind = match std::env::var("FB_DEBUG_WEB").as_deref() {
                Ok("ceiling") => fb_level::WebKind::Ceiling,
                Ok("floor") => fb_level::WebKind::Floor,
                _ => fb_level::WebKind::Corner,
            };
            let Some(web) = level.webs.iter().find(|w| w.kind == kind) else {
                return;
            };
            let at = v3(fb_level::Level::world(web.x, web.y));
            let facing = Quat::from_rotation_y(web.yaw) * Vec3::NEG_Z;
            let (target, back) = match kind {
                fb_level::WebKind::Corner => (
                    at + facing * web.size * 0.35 + Vec3::Y * web.height * 0.55,
                    facing * CELL * 1.5,
                ),
                fb_level::WebKind::Ceiling => (
                    at + Vec3::Y * web.height,
                    Quat::from_rotation_y(web.yaw + 1.57) * Vec3::NEG_Z * CELL * 0.5,
                ),
                fb_level::WebKind::Floor => (
                    at + Vec3::Y * 1.0,
                    Quat::from_rotation_y(web.yaw + 1.57) * Vec3::NEG_Z * CELL * 0.45,
                ),
            };
            let from = if kind == fb_level::WebKind::Corner {
                at + back + eye
            } else {
                target
                    + back
                    + Vec3::Y
                        * if kind == fb_level::WebKind::Floor {
                            9.0
                        } else {
                            -4.0
                        }
            };
            Transform::from_translation(from).looking_at(target, Vec3::Y)
        }
        "banner" => {
            // The banner `FB_DEBUG_BANNER` names (the first if unset), from five cells out in
            // front of it, looking at its middle.
            let index = std::env::var("FB_DEBUG_BANNER")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0);
            let Some(spot) = level.banners.get(index) else {
                return;
            };
            let (_, top) = crate::props::banner_hang(spot, level);
            let hang = crate::props::banner_transform(spot, top);
            let out = hang.rotation * Vec3::Z;
            let target = hang.translation - Vec3::Y * 9.0;
            let from =
                Vec3::new(hang.translation.x, EYE_HEIGHT, hang.translation.z) + out * CELL * 2.5;
            Transform::from_translation(from).looking_at(target, Vec3::Y)
        }
        "role" => {
            // The first room with the role `FB_DEBUG_ROLE` names (hall, hearth, sleeping, market
            // or store; hearth if unset), from the free floor farthest from its middle, looking
            // at the middle. `FB_DEBUG_ROOM=<index>` picks that room of the level instead.
            let want = match std::env::var("FB_DEBUG_ROLE").as_deref() {
                Ok("hall") => fb_level::RoomRole::Hall,
                Ok("sleeping") => fb_level::RoomRole::Sleeping,
                Ok("market") => fb_level::RoomRole::Market,
                Ok("store") => fb_level::RoomRole::Store,
                _ => fb_level::RoomRole::Hearth,
            };
            let by_index = std::env::var("FB_DEBUG_ROOM")
                .ok()
                .and_then(|v| v.parse::<usize>().ok());
            let Some(index) = by_index
                .or_else(|| (0..level.rooms.len()).find(|&r| level.roles_of(r).contains(&want)))
            else {
                return;
            };
            let room = level.rooms[index];
            // At the room's middle, or for a sleeping room at the middle of its bedrolls.
            let beds: Vec<(f32, f32)> = level
                .props
                .iter()
                .filter(|p| {
                    matches!(
                        p.prop,
                        fb_level::Prop::Scenery {
                            kind: armory::build::scenery::Scenery::Bedroll,
                            ..
                        }
                    ) && room.contains(p.x as i32, p.y as i32)
                })
                .map(|p| (p.x, p.y))
                .collect();
            let (cx, cy) = if want == fb_level::RoomRole::Sleeping && !beds.is_empty() {
                (
                    beds.iter().map(|b| b.0).sum::<f32>() / beds.len() as f32,
                    beds.iter().map(|b| b.1).sum::<f32>() / beds.len() as f32,
                )
            } else {
                room.centre()
            };
            let free = |x: f32, y: f32| level.props.iter().all(|p| (p.x - x).hypot(p.y - y) > 0.9);
            let stand = (room.y..room.y + room.h)
                .flat_map(|y| {
                    (room.x..room.x + room.w).map(move |x| (x as f32 + 0.5, y as f32 + 0.5))
                })
                .filter(|&(x, y)| {
                    level.cell(x as i32, y as i32) == fb_level::Cell::Floor && free(x, y)
                })
                // Far from the middle, or for beds about two cells off them so they fill the view.
                .min_by(|a, b| {
                    let score = |p: &(f32, f32)| {
                        let d = (p.0 - cx).hypot(p.1 - cy);
                        if want == fb_level::RoomRole::Sleeping {
                            (d - 2.0).abs()
                        } else {
                            -d
                        }
                    };
                    score(a).total_cmp(&score(b))
                })
                .unwrap_or((cx, cy));
            let at = v3(fb_level::Level::world(stand.0, stand.1)) + eye;
            let aim = v3(fb_level::Level::world(cx, cy));
            Transform::from_translation(at).looking_at(aim, Vec3::Y)
        }
        "denizen" => {
            // The level's denizen members that are stood in the game: a guard if
            // `FB_DEBUG_ROLE=guard`, otherwise the middle of the biggest knot of them, from the
            // most open side a few cells off, looking at them.
            let shown: Vec<&fb_level::FolkSpot> = level
                .folk
                .iter()
                .filter(|s| crate::folk::has_figure(s) && s.denizen.is_some())
                .collect();
            let guard = std::env::var("FB_DEBUG_ROLE").as_deref() == Ok("guard");
            let near = |a: &fb_level::FolkSpot| {
                shown
                    .iter()
                    .filter(|b| (a.x - b.x).hypot(a.y - b.y) < 4.0)
                    .count()
            };
            let Some(anchor) = shown
                .iter()
                .copied()
                .filter(|s| {
                    !guard
                        || s.trade
                            .is_some_and(|t| t.profession == fb_faction::Profession::Guard)
                })
                .max_by_key(|s| near(s))
            else {
                return;
            };
            let group: Vec<_> = shown
                .iter()
                .filter(|b| (anchor.x - b.x).hypot(anchor.y - b.y) < 4.0)
                .collect();
            let (mx, my) = if guard {
                (anchor.x, anchor.y)
            } else {
                (
                    group.iter().map(|g| g.x).sum::<f32>() / group.len() as f32,
                    group.iter().map(|g| g.y).sum::<f32>() / group.len() as f32,
                )
            };
            let at = v3(fb_level::Level::world(mx, my));
            let (cx, cy) = (mx.floor() as i32, my.floor() as i32);
            let reach = if guard { 3 } else { 4 };
            let side = [
                fb_level::Facing::North,
                fb_level::Facing::East,
                fb_level::Facing::South,
                fb_level::Facing::West,
            ]
            .into_iter()
            .max_by_key(|f| {
                (1..=reach)
                    .take_while(|k| level.is_open(cx + f.step().0 * k, cy + f.step().1 * k))
                    .count()
            })
            .unwrap_or(facing);
            // A guard is seen from the front, the way they face, if that side is open.
            let side = if guard {
                let (fx, fy) = anchor.facing();
                let ahead = if fx.abs() > fy.abs() {
                    if fx > 0.0 {
                        fb_level::Facing::East
                    } else {
                        fb_level::Facing::West
                    }
                } else if fy > 0.0 {
                    fb_level::Facing::South
                } else {
                    fb_level::Facing::North
                };
                if (1..=2).all(|k| level.is_open(cx + ahead.step().0 * k, cy + ahead.step().1 * k))
                {
                    ahead
                } else {
                    side
                }
            } else {
                side
            };
            let back = (1..=reach)
                .take_while(|k| level.is_open(cx + side.step().0 * k, cy + side.step().1 * k))
                .count() as f32;
            let from = at
                + v3(geometry::facing_vector(side))
                    * CELL
                    * back.max(if guard { 2.0 } else { 1.0 })
                    * 0.9
                + eye;
            Transform::from_translation(from).looking_at(at + Vec3::Y * 11.0, Vec3::Y)
        }
        "brazier" | "glowcaps" => {
            let kind = if mode == "brazier" {
                LightKind::Brazier
            } else {
                LightKind::Glowcaps
            };
            let Some(spot) = level.lights.iter().find(|l| l.kind == kind) else {
                return;
            };
            let at = v3(fb_level::Level::world(spot.x, spot.y));
            let (cx, cy) = (spot.x.floor() as i32, spot.y.floor() as i32);
            // Stand off toward the most open side, looking down at the light.
            let side = [
                fb_level::Facing::North,
                fb_level::Facing::East,
                fb_level::Facing::South,
                fb_level::Facing::West,
            ]
            .into_iter()
            .max_by_key(|f| {
                (1..=2)
                    .take_while(|k| level.is_open(cx + f.step().0 * k, cy + f.step().1 * k))
                    .count()
            })
            .unwrap_or(facing);
            let from = at + v3(geometry::facing_vector(side)) * CELL * 1.4 + eye;
            Transform::from_translation(from).looking_at(at + Vec3::Y * 4.0, Vec3::Y)
        }
        "moss" | "rock" | "rock-far" | "strata" | "hewn" => {
            let finishes = geometry::level_finishes(level);
            let (back, wanted): (f32, Box<dyn Fn(i32, i32) -> f32>) = match mode.as_str() {
                // Where the moss is thickest, a few steps off, taking in floor and wall foot.
                "moss" => (
                    CELL * 1.6,
                    Box::new(|x, y| {
                        finishes.moss_at((x as f32 + 0.5) * CELL, (y as f32 + 0.5) * CELL)
                    }),
                ),
                "strata" => (
                    CELL * 1.0,
                    Box::new(|x, y| {
                        finishes.at(x, y).map_or(0.0, |f| {
                            (f.walls == fb_level::WallFinish::Strata) as u32 as f32
                        })
                    }),
                ),
                // A hewn quarter's craggy rock.
                "hewn" => (
                    CELL * 1.0,
                    Box::new(|x, y| {
                        (finishes.character_at(x, y) == Some(fb_level::Character::Hewn)) as u32
                            as f32
                    }),
                ),
                // Close under the torch, or from across the level.
                _ => (
                    if mode == "rock" { CELL } else { CELL * 10.0 },
                    Box::new(|x, y| {
                        finishes.at(x, y).map_or(0.0, |f| {
                            (f.walls == fb_level::WallFinish::Rock) as u32 as f32
                        })
                    }),
                ),
            };
            // A wall seen from an open run of cells: the best score, then the longest run.
            let mut best: Option<(f32, i32, i32, fb_level::Facing)> = None;
            for y in 0..level.height as i32 {
                for x in 0..level.width as i32 {
                    for facing in [
                        fb_level::Facing::North,
                        fb_level::Facing::East,
                        fb_level::Facing::South,
                        fb_level::Facing::West,
                    ] {
                        let (dx, dy) = facing.step();
                        if !level.is_open(x, y) || level.is_open(x + dx, y + dy) {
                            continue;
                        }
                        let run = (1..=12)
                            .take_while(|k| level.is_open(x - dx * k, y - dy * k))
                            .count() as f32;
                        let score = if mode == "rock-far" {
                            run
                        } else {
                            wanted(x, y) * 100.0 + run.min(back / CELL)
                        };
                        if best.is_none_or(|(b, ..)| score > b) {
                            best = Some((score, x, y, facing));
                        }
                    }
                }
            }
            let Some((_, x, y, facing)) = best else {
                return;
            };
            let (dx, dy) = facing.step();
            let run = (1..=12)
                .take_while(|k| level.is_open(x - dx * k, y - dy * k))
                .count() as f32;
            let d = v3(geometry::facing_vector(facing));
            let wall = v3(geometry::cell_centre(x, y)) + d * CELL * 0.5;
            let stand = (back).min(run * CELL + CELL * 0.5 - 2.0);
            let drop = if mode == "moss" { 12.0 } else { 1.0 };
            let from = wall - d * stand + eye;
            Transform::from_translation(from)
                .looking_at(wall + Vec3::Y * (EYE_HEIGHT - drop), Vec3::Y)
        }
        _ => Transform::from_translation(start + eye).looking_to(inward, Vec3::Y),
    };
    commands
        .spawn((
            DebugCamera {
                walk_in: mode == "shut",
                fog: mode != "top",
            },
            StonePiece,
            Camera3d::default(),
            EXPOSURE,
            // The house style draws untonemapped; a tone curve would flatten the bands.
            Tonemapping::Linear,
            transform,
        ))
        // Something to see by until the player brings a torch; `FB_DEBUG_LIGHT=cool` swaps its
        // warmth for a neutral light, to judge colours the torch's orange would hide, and
        // `FB_DEBUG_LIGHT=lantern` for a steady, whiter lantern twice as bright and reaching
        // twice as far, to show a room whole.
        .with_child((
            if std::env::var("FB_DEBUG_LIGHT").is_ok_and(|v| v == "cool") {
                Torch::flickering(
                    point_light(Color::srgb(0.85, 0.95, 1.0), 1.0, 120.0, &EXPOSURE),
                    0.0,
                )
            } else if std::env::var("FB_DEBUG_LIGHT").is_ok_and(|v| v == "lantern") {
                Torch::flickering(
                    point_light(Color::srgb(1.0, 0.9, 0.74), 2.0, 240.0, &EXPOSURE),
                    0.0,
                )
            } else if mode == "corridor" {
                // A corridor's arches stand further apart than the usual light reaches.
                Torch::light(2.5, 260.0, &EXPOSURE)
            } else {
                Torch::light(1.0, 120.0, &EXPOSURE)
            },
            Transform::from_xyz(4.0, -3.0, 2.0),
        ));
    if mode == "top" {
        commands.insert_resource(GlobalAmbientLight {
            brightness: 120.0,
            ..abyss_ambient(&LightTuning::default())
        });
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
    let Ok((entity, mut debug)) = debug.single_mut() else {
        return;
    };
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

    #[test]
    fn door_masonry_and_columns_carry_moss_and_timber_does_not() {
        let level = fb_level::generate(1207, 2);
        let doors = FixtureShapes::make(&level).doors;
        assert!(!doors.is_empty());
        let mut stone = 0;
        for (_, frame, _, _) in &doors {
            for part in &frame.parts {
                if part.material == Some(MaterialKind::Stone) {
                    stone += 1;
                    assert_eq!(part.uvs_b.len(), part.positions.len(), "{}", part.name);
                    assert!(!part.uvs_b.is_empty());
                } else {
                    assert!(part.uvs_b.is_empty(), "{} is not stone", part.name);
                }
            }
        }
        assert!(stone > 0, "doorway masonry");
        let spot = geometry::stone::columns(&level)
            .into_iter()
            .next()
            .expect("a hall with columns at depth 2");
        for part in &mossy_column(&spot, level.seed, 3).parts {
            assert_eq!(part.uvs_b.len(), part.positions.len(), "{}", part.name);
            assert!(part.uvs_b.iter().all(|uv| uv[0] == 0.75));
        }
    }

    #[test]
    fn the_arch_camera_stands_on_open_floor() {
        // Seed 4 at depth 2 once put it in the rock east of the claim door at 28,15 (#575), and the
        // one-sided walls then showed the level from behind as bars and slabs.
        for seed in [4, 1207, 7, 11, 23] {
            let level = crate::travel::Abyss::new(seed).level_at(2);
            for doorway in geometry::doorways(&geometry::CellView::new(&level)) {
                let at = arch_eye(&level, &doorway);
                let (x, y) = (
                    (at.x / fb_level::CELL).floor() as i32,
                    (at.z / fb_level::CELL).floor() as i32,
                );
                assert!(
                    level.is_open(x, y) || (x, y) == (doorway.x, doorway.y),
                    "seed {seed}: the eye for the doorway at {},{} stands at {x},{y}",
                    doorway.x,
                    doorway.y
                );
            }
        }
    }

    #[test]
    fn the_rock_decay_types_match_the_shader() {
        let shader = include_str!("render/decay.wesl");
        assert!(shader.contains(&format!("const DECAY_ROCK: u32 = {DECAY_ROCK}u;")));
        assert!(shader.contains(&format!("const DECAY_STRATA: u32 = {DECAY_STRATA}u;")));
    }

    fn door() -> EntranceDoor {
        EntranceDoor {
            shut: Quat::IDENTITY,
            centre: Vec3::new(110.0, 0.0, 190.0),
            cell: (5, 9),
            swing: DoorSwing::Open,
        }
    }

    /// An app that limits new parts, and one part that has just appeared.
    fn limiting() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Tuning>()
            .add_systems(Update, limit_far_parts);
        let part = app.world_mut().spawn(ArmoryPart).id();
        (app, part)
    }

    /// Frames enough for a part's range to be given.
    fn settle(app: &mut App) {
        for _ in 0..3 {
            app.update();
        }
    }

    #[test]
    fn parts_on_another_cameras_layer_get_no_range() {
        // An icon, the doll or the held item, whose layer reaches the part a frame after it
        // appears, as `Propagate<RenderLayers>` does.
        for layer in [
            crate::inventory::icon::ICON_LAYER,
            crate::inventory::doll::DOLL_LAYER,
            crate::carry::VIEW_MODEL_LAYER,
        ] {
            let (mut app, part) = limiting();
            app.update();
            app.world_mut()
                .entity_mut(part)
                .insert(RenderLayers::layer(layer));
            settle(&mut app);
            assert!(
                app.world().get::<VisibilityRange>(part).is_none(),
                "layer {layer}"
            );
        }
    }

    #[test]
    fn a_crowd_of_cameras_leaves_every_part_unranged_and_the_world_gets_it_back() {
        let (mut app, part) = limiting();
        settle(&mut app);
        assert!(app.world().get::<VisibilityRange>(part).is_some());
        let cameras: Vec<Entity> = (0..RANGED_VIEWS + 1)
            .map(|_| app.world_mut().spawn(Camera::default()).id())
            .collect();
        settle(&mut app);
        assert!(app.world().get::<VisibilityRange>(part).is_none());
        for camera in cameras {
            app.world_mut().entity_mut(camera).despawn();
        }
        settle(&mut app);
        assert!(app.world().get::<VisibilityRange>(part).is_some());
    }

    #[test]
    fn a_new_part_is_drawn_out_to_the_fogs_end_and_a_margin() {
        let (mut app, part) = limiting();
        settle(&mut app);
        let range = app
            .world()
            .get::<VisibilityRange>(part)
            .expect("the part has a range")
            .clone();
        let fog_end = app.world().resource::<Tuning>().light.fog_end;
        assert!(
            range.is_visible_at_all(fog_end),
            "a part at the fog's end is drawn"
        );
        assert!(
            range.is_culled(fog_end + DRAW_MARGIN + 1.0),
            "a part past the margin is not"
        );
        assert!(range.is_abrupt());
    }

    #[test]
    fn the_range_follows_the_fog_when_it_is_tuned() {
        let (mut app, part) = limiting();
        app.add_systems(Update, apply_light_tuning.after(limit_far_parts));
        app.init_resource::<GlobalAmbientLight>();
        settle(&mut app);
        app.world_mut().resource_mut::<Tuning>().light.fog_end = 100.0;
        app.update();
        let range = app.world().get::<VisibilityRange>(part).unwrap();
        assert!(range.is_visible_at_all(100.0 + DRAW_MARGIN - 1.0));
        assert!(range.is_culled(100.0 + DRAW_MARGIN + 1.0));
    }

    #[test]
    fn the_view_from_above_draws_every_part() {
        let (mut app, part) = limiting();
        app.world_mut().spawn(DebugCamera {
            walk_in: false,
            fog: false,
        });
        settle(&mut app);
        assert!(app.world().get::<VisibilityRange>(part).is_none());
    }

    #[test]
    fn the_door_waits_for_the_player_to_walk_in() {
        let mut d = door();
        // No player yet, or one still in the doorway's reach: the door stands open.
        assert_eq!(d.step(1.0, None), None);
        assert_eq!(
            d.step(1.0, Some(d.centre + Vec3::new(0.0, 0.0, -CELL))),
            None
        );
        assert_eq!(d.swing, DoorSwing::Open);
        // A step further in and it begins to swing, then shuts after SHUT_SECONDS and stays.
        assert_eq!(
            d.step(0.1, Some(d.centre + Vec3::new(0.0, 0.0, -1.3 * CELL))),
            Some(DoorEvent::BeganToShut)
        );
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
            .insert_resource(GameSettings {
                sound_volume: 0.5,
                ..default()
            })
            .init_resource::<Tuning>()
            .add_systems(Update, shut_the_entrance);
        let d = door();
        app.world_mut().spawn((d, Transform::default()));
        let player = app
            .world_mut()
            .spawn((Player, GlobalTransform::from_translation(d.centre)))
            .id();
        let sounds = |app: &mut App| {
            app.world_mut()
                .query::<&AudioPlayer>()
                .iter(app.world())
                .count()
        };
        let frame = |app: &mut App| {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(100));
            app.update();
        };
        frame(&mut app);
        assert_eq!(sounds(&mut app), 0, "open: silent");
        // The player walks in; the door swings, silent until it reaches shut.
        *app.world_mut().get_mut::<GlobalTransform>(player).unwrap() =
            GlobalTransform::from_translation(d.centre - Vec3::Z * 1.5 * CELL);
        let mut shut_at = None;
        for i in 0..40 {
            frame(&mut app);
            let swing = app
                .world_mut()
                .query::<&EntranceDoor>()
                .single(app.world())
                .unwrap()
                .swing;
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
        assert_eq!(
            playback.single(app.world()).unwrap().volume.to_linear(),
            0.5,
            "at the sound volume"
        );
    }

    #[test]
    fn the_door_sound_is_an_asset() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join(LevelSounds::DOOR_SHUT);
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
        let (new, grave) = (
            fixture_appearance(1207, ConditionState::Pristine),
            fixture_appearance(1207, ConditionState::GraveFind),
        );
        let flames: Vec<_> = brazier
            .parts
            .iter()
            .filter(|p| p.name.starts_with("flame"))
            .collect();
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
        let part = armory::Part {
            material: Some(MaterialKind::Stone),
            ..default()
        };
        let decay = stone.material_for(&part).extension.decay;
        assert_eq!(decay.kind, 9);
        assert!((decay.amount - STONE_WEAR).abs() < 1e-6);
        assert!((decay.scale - STONE_PATTERN_SCALE).abs() < 1e-6);
    }

    #[test]
    fn doorways_are_built_in_their_finish_style_and_deterministically() {
        let mut arched = [0, 0];
        for (depth, slot) in [(5, 0), (4, 1)] {
            for seed in 1..=40 {
                let level = fb_level::generate(seed, depth);
                let (a, b) = (FixtureShapes::make(&level), FixtureShapes::make(&level));
                assert_eq!(a, b, "seed {seed} depth {depth}");
                let finishes =
                    fb_level::finishes(&level, &fb_level::LevelParams::for_style(level.style));
                for (doorway, frame, _, _) in &a.doors {
                    let timber = frame.parts.iter().any(|p| p.name == "frame");
                    let style = finishes.door_style(doorway.x, doorway.y);
                    assert_eq!(
                        timber,
                        style == Some(fb_level::DoorStyle::Timber),
                        "seed {seed} depth {depth} at {},{}",
                        doorway.x,
                        doorway.y
                    );
                    arched[slot] += !timber as u32;
                }
            }
        }
        assert!(
            arched[0] > 0 && arched[1] > 0,
            "arched doorways at depth 5 and 4: {arched:?}"
        );
    }
}
