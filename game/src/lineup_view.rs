//! The lineup view (`--lineup`): a faction's members standing side by side facing the viewer,
//! the faction's hall banner hanging behind them, so anyone can see at a glance whether the
//! members match the faction: its colours, garments, armour, folk and sex.
//!
//! Each member is made as the level makes them: a [`FolkSpot`](fb_level::FolkSpot) drawn as a
//! camp's are ([`roll_members`]), a [`Person::of`] for their faction, a [`Figure::dress`] fitted
//! off the main thread ([`Fitting`], kept in the [`FitCache`]) and rigged as the folk of a level
//! ([`spawn_rigged`]). Beside the row a panel lists what the faction expects of them
//! ([`crate::lineup_check`]) and which members miss it, and why.
//!
//! Keys: Left and Right step through the factions of the world, Up and Down change the world
//! seed, B toggles the plain light backdrop and the faction's hall (a stone wall and braziers,
//! lit as in play), Esc leaves. Command line: `--lineup --seed 113 --faction 3 --hall`
//! ([`LineupChoice::from_args`]). `FB_SCREENSHOT` and `FB_SCREENSHOT_FRAMES` capture it as they
//! do the doll view ([`crate::capture`]); give the fitting a few hundred frames.

use std::sync::Arc;

use armory::families::{Generator, banner::{Banner, BannerMount}};
use bevy::{camera::Exposure, mesh::skinning::SkinnedMeshInverseBindposes, prelude::*, render::view::Tonemapping, ui::IsDefaultUiCamera};
use fb_faction::{FactionSpec, World};

use crate::carry::{appearance_of, given_light};
use crate::figure::Figure;
use crate::fitting::{FitCache, Fitting, figure_key};
use crate::folk::{Person, Stride};
use crate::folk_pose::{FolkPosePlugin, spawn_rigged};
use crate::light::{StudioLight, Torch};
use crate::lineup_check::{self, DEPTH, MEMBERS, Miss, Rule};
use crate::props::fly_colours;
use crate::render::ItemSpawner;
use crate::turntable::PLATE;
use crate::ui::{BAD, BG, BRIGHT, INK, InterfaceScalePlugin, LINE, MUTED, OK, UiFonts};

/// Distance between the members' middles, in world units.
const SPACING: f32 = 6.0;
/// Where the row stands and the wall behind it: the banners hang between.
const WALL_Z: f32 = -9.0;
const BANNER_Z: f32 = -6.5;
/// The frame the scene is fitted to, in world units up: banner tops to the labels' room below.
const TOP: f32 = 27.0;
const BOTTOM: f32 = -4.0;
/// The panel's width and margin in the interface's own pixels (a 1920 by 1080 layout).
const PANEL_WIDTH: f32 = 560.0;
const MARGIN: f32 = 16.0;
/// How far a banner hangs off the floor.
const BANNER_OFF_FLOOR: f32 = 4.0;
/// How strongly a brazier lights the row from where it stands (our own tuning).
const BRAZIER_LEVEL: f32 = 0.1;

/// The state that scopes the view's entities: leaving it despawns them, with the meshes and
/// materials they held.
#[derive(States, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineupOn {
    #[default]
    Showing,
    Left,
}

/// What the lineup shows. Changing it rebuilds the row.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct LineupChoice {
    /// The world seed.
    pub seed: u32,
    /// The faction's id in the world's pool.
    pub faction: u8,
    /// The hall behind them, rather than the plain backdrop.
    pub hall: bool,
}

impl Default for LineupChoice {
    fn default() -> Self {
        LineupChoice { seed: 1207, faction: 0, hall: false }
    }
}

impl LineupChoice {
    /// Parses `--seed <n>`, `--faction <id>` and `--hall` (also `--flag=value`); `--lineup`
    /// itself is skipped. `args` excludes the program name.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<LineupChoice, String> {
        let mut choice = LineupChoice::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (arg, None),
            };
            let mut value = || inline.clone().or_else(|| args.next()).ok_or(format!("{flag} needs a value"));
            match flag.as_str() {
                "--lineup" => {}
                "--hall" => choice.hall = true,
                "--seed" => {
                    let seed = value()?;
                    choice.seed = seed.parse().map_err(|_| format!("--seed: {seed:?} is not a u32"))?;
                }
                "--faction" => {
                    let id = value()?;
                    choice.faction = id.parse().map_err(|_| format!("--faction: {id:?} is not a faction id"))?;
                }
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        Ok(choice)
    }

    /// The faction's place in `world`'s pool, wrapped into it.
    pub fn slot(&self, world: &World) -> usize {
        self.faction as usize % world.factions().len()
    }

    /// Steps `by` factions through the world's pool, wrapping.
    pub fn step_faction(&mut self, by: isize, world: &World) {
        let count = world.factions().len() as isize;
        self.faction = (self.slot(world) as isize + by).rem_euclid(count) as u8;
    }

    /// Steps the world seed, keeping the faction if the new world has one there.
    pub fn step_seed(&mut self, by: i64) {
        self.seed = (self.seed as i64 + by).rem_euclid(1 << 32) as u32;
    }
}

pub struct LineupViewPlugin;

impl Plugin for LineupViewPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FolkPosePlugin>() {
            app.add_plugins(FolkPosePlugin);
        }
        app.init_state::<LineupOn>()
            .init_resource::<LineupChoice>()
            .init_resource::<Lineup>()
            .init_resource::<FitCache>()
            .init_resource::<UiFonts>()
            .insert_resource(ClearColor(PLATE))
            .insert_resource(StudioLight::ARMORY.ambient_light(&Exposure::INDOOR))
            .add_plugins(InterfaceScalePlugin)
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    (keys, rebuild.run_if(resource_changed::<LineupChoice>), set_backdrop.run_if(resource_changed::<LineupChoice>)).chain(),
                    dress,
                    (frame_camera, place_labels).chain(),
                )
                    .run_if(in_state(LineupOn::Showing)),
            )
            .add_systems(OnEnter(LineupOn::Left), leave)
            .add_systems(OnExit(LineupOn::Showing), release);
    }
}

/// One member of the row.
struct Member {
    person: Person,
    misses: Vec<Miss>,
}

/// The row on show: who stands in it, and the fittings still running.
#[derive(Resource, Default)]
pub struct Lineup {
    world: Option<World>,
    members: Vec<Member>,
    fittings: Vec<Option<Fitting<String, Arc<Figure>>>>,
    /// Which members have been spawned.
    spawned: Vec<bool>,
}

#[derive(Component)]
struct LineupCamera;

#[derive(Component)]
struct KeyLight;

/// The hall's wall, floor and braziers, shown on the hall backdrop.
#[derive(Component)]
struct HallPiece;

/// Everything that belongs to the faction on show, despawned when the choice changes.
#[derive(Component)]
struct OfFaction;

/// A member's label under their feet.
#[derive(Component)]
struct MemberLabel {
    index: usize,
}

fn row_x(index: usize, count: usize) -> f32 {
    (index as f32 - (count as f32 - 1.0) * 0.5) * SPACING
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let exposure = Exposure::INDOOR;
    commands
        .spawn((
            LineupCamera,
            Name::new("Lineup camera"),
            Camera3d::default(),
            exposure,
            // As on the turntable: a tone curve would flatten the bands.
            Tonemapping::Linear,
            IsDefaultUiCamera,
            Transform::from_xyz(0.0, 10.0, 90.0),
            DespawnOnExit(LineupOn::Showing),
        ))
        .with_child((KeyLight, StudioLight::ARMORY.key_light(&exposure)));

    // The hall: a stone wall and flagged floor, and four braziers well out to the sides, two behind
    // the row and two in front, each lighting the row at [`BRAZIER_LEVEL`].
    let stone = materials.add(StandardMaterial { base_color: Color::srgb(0.30, 0.28, 0.26), perceptual_roughness: 1.0, ..default() });
    let flags = materials.add(StandardMaterial { base_color: Color::srgb(0.22, 0.2, 0.19), perceptual_roughness: 1.0, ..default() });
    let wall = meshes.add(Cuboid::new(200.0, 60.0, 1.0));
    let floor = meshes.add(Cuboid::new(200.0, 1.0, 120.0));
    let hall = |piece: (Handle<Mesh>, Handle<StandardMaterial>, Transform)| (HallPiece, Mesh3d(piece.0), MeshMaterial3d(piece.1), piece.2, Visibility::Hidden, DespawnOnExit(LineupOn::Showing));
    commands.spawn(hall((wall, stone, Transform::from_xyz(0.0, 30.0, WALL_Z - 0.5))));
    commands.spawn(hall((floor, flags, Transform::from_xyz(0.0, -0.5, WALL_Z + 59.0))));
    let tuning = crate::tuning::LightTuning::default();
    for (x, y, z) in [(-44.0, 12.0, -3.0), (44.0, 12.0, -3.0), (-30.0, 10.0, 24.0), (30.0, 10.0, 24.0)] {
        // Strength set for the row's distance, as the lab figure's key is, so it lights a face at
        // about this level rather than burning the near side out.
        let (range, distance) = (tuning.brazier_reach * 2.0, 45.0);
        let light = Torch::flickering(crate::light::point_light(Torch::COLOR, BRAZIER_LEVEL / crate::light::reach_falloff(distance, range), range, &exposure), 0.5);
        commands.spawn((HallPiece, Name::new("Brazier"), light, Transform::from_xyz(x, y, z), Visibility::Hidden, DespawnOnExit(LineupOn::Showing)));
    }
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut choice: ResMut<LineupChoice>, lineup: Res<Lineup>, mut next: ResMut<NextState<LineupOn>>) {
    let pressed = |key| keys.just_pressed(key);
    let world = lineup.world.as_ref().filter(|w| w.seed == choice.seed);
    let mut next_choice = *choice;
    if let Some(world) = world {
        if pressed(KeyCode::ArrowRight) {
            next_choice.step_faction(1, world);
        }
        if pressed(KeyCode::ArrowLeft) {
            next_choice.step_faction(-1, world);
        }
    }
    if pressed(KeyCode::ArrowUp) {
        next_choice.step_seed(1);
    }
    if pressed(KeyCode::ArrowDown) {
        next_choice.step_seed(-1);
    }
    if pressed(KeyCode::KeyB) {
        next_choice.hall = !next_choice.hall;
    }
    if next_choice != *choice {
        *choice = next_choice;
    }
    if pressed(KeyCode::Escape) {
        next.set(LineupOn::Left);
    }
}

/// The plain light backdrop or the hall: the clear colour, the light and the hall's pieces.
fn set_backdrop(
    choice: Res<LineupChoice>,
    mut commands: Commands,
    mut hall: Query<&mut Visibility, (With<HallPiece>, Without<KeyLight>)>,
    mut key: Query<&mut Visibility, With<KeyLight>>,
) {
    let exposure = Exposure::INDOOR;
    let (shown, hidden) = (Visibility::Inherited, Visibility::Hidden);
    for mut visibility in &mut hall {
        *visibility = if choice.hall { shown } else { hidden };
    }
    for mut visibility in &mut key {
        *visibility = if choice.hall { hidden } else { shown };
    }
    if choice.hall {
        commands.insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.03)));
        commands.insert_resource(GlobalAmbientLight { color: crate::level::AMBIENT_COLOR, brightness: crate::tuning::LightTuning::default().ambient_lux, ..default() });
    } else {
        commands.insert_resource(ClearColor(PLATE));
        commands.insert_resource(StudioLight::ARMORY.ambient_light(&exposure));
    }
}

/// Rolls the row for the choice: the world, the members, their checks, the fittings; and lays
/// out the banners and the interface for it.
#[allow(clippy::too_many_arguments)]
fn rebuild(
    mut commands: Commands,
    choice: Res<LineupChoice>,
    mut lineup: ResMut<Lineup>,
    mut cache: ResMut<FitCache>,
    old: Query<Entity, With<OfFaction>>,
    fonts: Res<UiFonts>,
    mut spawner: ItemSpawner,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    if lineup.world.as_ref().is_none_or(|w| w.seed != choice.seed) {
        lineup.world = Some(World::new(choice.seed));
    }
    let world = lineup.world.clone().expect("the world was just made");
    let faction = world.factions()[choice.slot(&world)].clone();
    let people: Vec<Person> = lineup_check::roll_members(&faction, choice.seed, MEMBERS).iter().map(|spot| Person::of(spot, &faction, DEPTH)).collect();
    lineup.members = people
        .into_iter()
        .map(|person| {
            let misses = lineup_check::check_member(world.factions(), &person);
            Member { person, misses }
        })
        .collect();
    // Fit each one's figure off the main thread, or take it from the cache.
    lineup.fittings = lineup
        .members
        .iter()
        .map(|member| {
            let person = &member.person;
            let key = figure_key(&person.spec, &person.gear, person.dirt);
            Some(match cache.figure(&key) {
                Some(figure) => Fitting::ready(key, figure),
                None => {
                    let (spec, gear, dirt) = (person.spec.clone(), person.gear.clone(), person.dirt);
                    Fitting::start(key, move || Arc::new(Figure::dress(&spec, &gear, dirt)))
                }
            })
        })
        .collect();
    lineup.spawned = vec![false; lineup.members.len()];

    spawn_banners(&mut spawner, &faction, choice.seed, lineup.members.len());
    spawn_interface(&mut commands, &fonts, &choice, &world, &faction, &lineup.members);
}

/// The faction's hall banner, from its arms, hung behind the row between each pair of members
/// (the 1st and 2nd, 3rd and 4th and so on, so the middle of the row is never hidden).
fn spawn_banners(spawner: &mut ItemSpawner, faction: &FactionSpec, world_seed: u32, count: usize) {
    let seed = world_seed.wrapping_mul(0x9E37_79B1) ^ (faction.id.0 as u32).wrapping_mul(0x85EB_CA6B);
    // As the level hangs a hall banner (`crate::props::banner_hang`): rolled wide and long from
    // its seed, painted with exactly its faction's field.
    let rolled = Banner::roll(seed);
    let mut state = Banner::roll_hall(seed, BannerMount::Rod, rolled.cloth.end);
    fly_colours(&mut state, faction.field);
    let top = state.cloth.length + BANNER_OFF_FLOOR;
    let item = armory::Item::Banner(state);
    let (mesh, appearance) = (item.build(), appearance_of(&item));
    for pair in 0..count / 2 {
        let x = row_x(pair * 2, count) + SPACING * 0.5;
        spawner.spawn(&mesh, &appearance).insert((OfFaction, Name::new(format!("Banner of {}", faction.name)), Transform::from_xyz(x, top, BANNER_Z), DespawnOnExit(LineupOn::Showing)));
    }
}

/// Polls the fittings and stands each member in the row as theirs finishes.
fn dress(
    mut commands: Commands,
    mut lineup: ResMut<Lineup>,
    mut cache: ResMut<FitCache>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut spawner: ItemSpawner,
) {
    let lineup = &mut *lineup;
    let count = lineup.members.len();
    for index in 0..count {
        let Some(fitting) = lineup.fittings.get_mut(index).and_then(Option::as_mut) else { continue };
        let Some(figure) = fitting.poll() else { continue };
        cache.store_figure(fitting.key(), figure.clone());
        lineup.fittings[index] = None;
        lineup.spawned[index] = true;
        let person = &lineup.members[index].person;
        let root = commands
            .spawn((
                OfFaction,
                Name::new(person.name.clone()),
                Transform::from_xyz(row_x(index, count), 0.0, 0.0),
                Visibility::default(),
                Stride::default(),
                DespawnOnExit(LineupOn::Showing),
            ))
            .id();
        let anim = spawn_rigged(&mut spawner, &mut bindposes, &figure, root, false, person.spot.seed);
        if let Some(light) = &figure.light {
            // As the level lights a held flame (`crate::folk::spawn_denizen`).
            let at = anim.in_joint(light.joint, light.at + Vec3::Y * 1.0);
            spawner.commands().spawn((given_light(light.anchor, crate::level::EXPOSURE, &crate::tuning::LightTuning::default()), Transform::from_translation(at), ChildOf(anim.joint(light.joint))));
        }
        commands.entity(root).insert(anim);
    }
}

fn label_text(commands: &mut ChildSpawnerCommands, text: String, font: &Handle<Font>, size: f32, colour: Color) {
    commands.spawn((Text::new(text), UiFonts::style(font, size, colour), TextLayout::justify(Justify::Center)));
}

/// The caption, the member labels, the help line and the side panel.
fn spawn_interface(commands: &mut Commands, fonts: &UiFonts, choice: &LineupChoice, world: &World, faction: &FactionSpec, members: &[Member]) {
    let chip = |extra: Node| Node { padding: UiRect::axes(px(12), px(8)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(6)), ..extra };

    // The caption, top left.
    let folk = faction.folk.race().label.to_lowercase();
    let described = format!(
        "{}{} folk   {}   classes: {}\ncolours: {}",
        if faction.insular { "wholly " } else { "mostly " },
        folk,
        lineup_check::sex_line(faction.sex),
        lineup_check::classes_line(faction),
        lineup_check::colours_line(faction),
    );
    let slot = choice.slot(world);
    commands
        .spawn((
            OfFaction,
            DespawnOnExit(LineupOn::Showing),
            chip(Node { position_type: PositionType::Absolute, left: px(MARGIN), top: px(MARGIN), flex_direction: FlexDirection::Column, row_gap: px(4), max_width: px(900), ..default() }),
            BackgroundColor(BG.with_alpha(0.88)),
            BorderColor::all(LINE),
        ))
        .with_children(|caption| {
            caption.spawn((Text::new(faction.name.clone()), UiFonts::style(&fonts.display_bold, 34.0, BRIGHT)));
            caption.spawn((Text::new(described), UiFonts::style(&fonts.body, 19.0, INK)));
            let help = format!("world {}   faction {} of {}   Left/Right faction   Up/Down seed   B backdrop   Esc leave", choice.seed, slot + 1, world.factions().len());
            caption.spawn((Text::new(help), UiFonts::style(&fonts.body, 15.0, MUTED)));
        });

    // A label under each member's feet; the view moves them each frame.
    for (index, member) in members.iter().enumerate() {
        let flagged = !member.misses.is_empty();
        let border = if flagged { BAD } else { LINE };
        commands
            .spawn((
                OfFaction,
                DespawnOnExit(LineupOn::Showing),
                MemberLabel { index },
                chip(Node { position_type: PositionType::Absolute, flex_direction: FlexDirection::Column, align_items: AlignItems::Center, padding: UiRect::axes(px(4), px(4)), ..default() }),
                BackgroundColor(BG.with_alpha(0.88)),
                BorderColor::all(border),
                Visibility::Hidden,
            ))
            .with_children(|label| {
                let name = if flagged { format!("! {}", member.person.name) } else { member.person.name.clone() };
                label_text(label, name, &fonts.display_bold, 16.0, if flagged { BAD } else { BRIGHT });
                let sex = if member.person.spot.sex == fb_items::Sex::Female { "woman" } else { "man" };
                let line = format!("{} {} {}", member.person.spot.folk.race().label.to_lowercase(), sex, lineup_check::class_label(&member.person).to_lowercase());
                label_text(label, line, &fonts.body, 13.0, MUTED);
            });
    }

    // The side panel: what the faction expects, then each member's checks.
    commands
        .spawn((
            OfFaction,
            DespawnOnExit(LineupOn::Showing),
            chip(Node {
                position_type: PositionType::Absolute,
                right: px(MARGIN),
                top: px(MARGIN),
                bottom: px(MARGIN),
                width: px(PANEL_WIDTH),
                flex_direction: FlexDirection::Column,
                row_gap: px(3),
                padding: UiRect::all(px(14)),
                overflow: Overflow::clip(),
                ..default()
            }),
            BackgroundColor(BG.with_alpha(0.94)),
            BorderColor::all(LINE),
        ))
        .with_children(|panel| {
            let line = |panel: &mut ChildSpawnerCommands, text: String, size: f32, colour: Color| {
                panel.spawn((Text::new(text), UiFonts::style(&fonts.body, size, colour)));
            };
            panel.spawn((Text::new("The faction expects"), UiFonts::style(&fonts.display_bold, 24.0, BRIGHT)));
            for (heading, text) in lineup_check::expectations(faction, world.factions()) {
                line(panel, format!("{heading}: {text}"), 16.0, INK);
            }
            for flag in lineup_check::name_colour_flags(faction) {
                line(panel, flag, 16.0, BAD);
            }
            panel.spawn((Text::new("Members"), UiFonts::style(&fonts.display_bold, 24.0, BRIGHT), Node { margin: UiRect::top(px(10)), ..default() }));
            for rule in Rule::ALL {
                let met = members.iter().filter(|m| lineup_check::passes(&m.misses, rule)).count();
                let colour = if met == members.len() { OK } else { BAD };
                line(panel, format!("{}: {met} of {}", rule.label(), members.len()), 16.0, colour);
            }
            let flagged: Vec<&Member> = members.iter().filter(|m| !m.misses.is_empty()).collect();
            if flagged.is_empty() {
                line(panel, "Every member meets what the faction expects.".to_string(), 16.0, OK);
            }
            for member in flagged {
                for miss in &member.misses {
                    line(panel, lineup_check::describe(&member.person, miss), 15.0, BAD);
                }
            }
            let clean: Vec<String> = members.iter().filter(|m| m.misses.is_empty()).map(|m| m.person.name.clone()).collect();
            if !clean.is_empty() {
                line(panel, format!("Meet it: {}", clean.join(", ")), 15.0, OK);
            }
        });
}

/// Stands the camera back so the row fills the scene's share of the view (the part the panel
/// leaves), and shifts it so the row sits in that part.
fn frame_camera(
    lineup: Res<Lineup>,
    scale: Res<UiScale>,
    mut cameras: Query<(&Camera, &Projection, &mut Transform), With<LineupCamera>>,
) {
    for (camera, projection, mut transform) in &mut cameras {
        let (Some(view), Projection::Perspective(perspective)) = (camera.logical_viewport_size(), projection) else { continue };
        if view.min_element() <= 0.0 {
            continue;
        }
        let target = camera_for(view, perspective, scale.0, lineup.members.len().max(1));
        if *transform != target {
            *transform = target;
        }
    }
}

/// Where the camera stands to show a row of `count` in a view `view` logical pixels across, when
/// the interface is drawn at `ui_scale`: pure, so it can be tested.
pub fn camera_for(view: Vec2, perspective: &PerspectiveProjection, ui_scale: f32, count: usize) -> Transform {
    let half_tan = (perspective.fov * 0.5).tan();
    let aspect = view.x / view.y;
    // The panel's share of the width; the scene gets the rest, less a margin.
    let panel = (PANEL_WIDTH + 2.0 * MARGIN) * ui_scale / view.x;
    let share = (1.0 - panel).clamp(0.4, 1.0);
    let width = count as f32 * SPACING + 2.0 * SPACING;
    let height = TOP - BOTTOM;
    let by_width = width / (share * 0.96 * 2.0 * half_tan * aspect);
    let by_height = height / (0.94 * 2.0 * half_tan);
    let distance = by_width.max(by_height);
    // The row's middle lands in the middle of the scene's part of the view.
    let half_view = distance * half_tan * aspect;
    let centre_ndc = share - 1.0;
    let x = -centre_ndc * half_view;
    let y = (TOP + BOTTOM) * 0.5;
    Transform::from_xyz(x, y, distance)
}

/// Puts each label under its member's feet, as wide as the gap between members.
fn place_labels(
    lineup: Res<Lineup>,
    scale: Res<UiScale>,
    cameras: Query<(&Camera, &Transform), With<LineupCamera>>,
    mut labels: Query<(&MemberLabel, &mut Node, &mut Visibility)>,
) {
    let Ok((camera, transform)) = cameras.single() else { return };
    let at = GlobalTransform::from(*transform);
    let count = lineup.members.len();
    for (label, mut node, mut visibility) in &mut labels {
        let project = |x: f32, y: f32| camera.world_to_viewport(&at, Vec3::new(x, y, 0.0)).ok();
        let x = row_x(label.index, count);
        let (Some(feet), Some(left), Some(right)) = (project(x, 0.0), project(x - SPACING * 0.5, 0.0), project(x + SPACING * 0.5, 0.0)) else { continue };
        let ui = scale.0.max(f32::EPSILON);
        let width = ((right.x - left.x) * 0.98 / ui).max(40.0);
        let (left_px, top_px) = ((feet.x - (right.x - left.x) * 0.49) / ui, feet.y / ui + 6.0);
        let (want_left, want_top, want_width) = (px(left_px), px(top_px), px(width));
        if node.left != want_left || node.top != want_top || node.width != want_width {
            node.left = want_left;
            node.top = want_top;
            node.width = want_width;
        }
        if *visibility != Visibility::Inherited {
            *visibility = Visibility::Inherited;
        }
    }
}

/// Esc: leave the view and end the run.
fn leave(mut exit: MessageWriter<AppExit>) {
    exit.write(AppExit::Success);
}

/// Leaving the view lets go of what it held: the fits, the running fittings and the row. (The
/// scoped entities, with their cameras and meshes, despawn with the state.)
fn release(mut lineup: ResMut<Lineup>, mut cache: ResMut<FitCache>) {
    *lineup = Lineup::default();
    *cache = FitCache::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_the_command_line() {
        let choice = LineupChoice::from_args(args("--lineup --seed 113 --faction 3 --hall")).unwrap();
        assert_eq!(choice, LineupChoice { seed: 113, faction: 3, hall: true });
        assert_eq!(LineupChoice::from_args(args("--lineup --seed=7")).unwrap(), LineupChoice { seed: 7, faction: 0, hall: false });
        assert!(LineupChoice::from_args(args("--seed")).is_err());
        assert!(LineupChoice::from_args(args("--faction x")).is_err());
        assert!(LineupChoice::from_args(args("--bare")).is_err());
    }

    #[test]
    fn left_and_right_wrap_round_the_pool() {
        let world = World::new(113);
        let mut choice = LineupChoice { seed: 113, faction: 0, hall: false };
        choice.step_faction(-1, &world);
        assert_eq!(choice.faction as usize, world.factions().len() - 1);
        choice.step_faction(1, &world);
        assert_eq!(choice.faction, 0);
        let mut seed = LineupChoice { seed: 0, ..choice };
        seed.step_seed(-1);
        assert_eq!(seed.seed, u32::MAX);
    }

    #[test]
    fn the_row_is_centred_and_apart() {
        let xs: Vec<f32> = (0..MEMBERS).map(|i| row_x(i, MEMBERS)).collect();
        assert!((xs.iter().sum::<f32>()).abs() < 1e-4);
        assert!(xs.windows(2).all(|w| (w[1] - w[0] - SPACING).abs() < 1e-4));
    }

    #[test]
    fn the_camera_fits_the_row_in_any_window() {
        let perspective = PerspectiveProjection::default();
        for view in [Vec2::new(1920.0, 1080.0), Vec2::new(1280.0, 720.0), Vec2::new(800.0, 800.0), Vec2::new(2560.0, 1080.0), Vec2::new(640.0, 1000.0)] {
            let scale = (view.y / 1080.0).min(view.x / 1920.0);
            let at = camera_for(view, &perspective, scale, MEMBERS);
            let half_tan = (perspective.fov * 0.5).tan();
            let half_width = at.translation.z * half_tan * view.x / view.y;
            let row = (MEMBERS as f32 * SPACING) * 0.5;
            // The row's ends are inside the part of the view the panel leaves.
            let panel_left = view.x - (PANEL_WIDTH + MARGIN) * scale;
            let to_px = |x: f32| (x - at.translation.x) / half_width * 0.5 * view.x + view.x * 0.5;
            assert!(to_px(-row) >= 0.0 && to_px(row) <= panel_left + 1.0, "{view}: {} to {} of {panel_left}", to_px(-row), to_px(row));
            // The frame's top and bottom are inside the view.
            let half_height = at.translation.z * half_tan;
            assert!(at.translation.y - half_height <= BOTTOM && at.translation.y + half_height >= TOP, "{view}");
        }
    }
}
