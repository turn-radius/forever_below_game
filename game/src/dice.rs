//! A check's roll on screen: a d20 tumbles and lands on its rolled face, the modifiers count up
//! beside it, the total meets the target, and the panel says success or failure.
//!
//! Anything that rolls a [`fb_character::check::Check`] sends a [`RollRequest`] with the
//! [`CheckRoll`] and what it is for ([`RollPurpose`]); the roll is already made, so the screen
//! only shows it. When the panel closes, a [`RollDone`] carries the roll and purpose back and the
//! requester applies the outcome. Esc ([`Action::Back`]) jumps to the verdict,
//! and again closes the panel (the screen under it cannot be left meanwhile, [`crate::play::switch_screens`]); left alone it closes itself a couple of seconds after the verdict.
//! Requests queue, so two rolls show one after the other. The panel pauses nothing: it sits over
//! the talk or the inventory, which are already screens.
//!
//! The die is a 3D icosahedron drawn into an image by a camera on its own layer
//! ([`DICE_LAYER`]), gold numerals engraved from strokes on each face, opposite faces adding to
//! 21. Any die up to d20 tumbles as the d20 and shows the face it rolled. The panel's layout is
//! laid out for the 1920 by 1080 reference and scaled with the window like every screen ([`crate::ui`]).
//!
//! `FB_DEBUG_CHECK=<face>[,<seconds>]` rolls a sample persuasion check once the delve is under
//! way, for captures: `FB_DEBUG_CHECK=14,0.7` shows the roll frozen 0.7 seconds in. `FB_DEBUG_ROLL_AT=4.5`
//! freezes any roll at that time, such as a persuasion chosen with `FB_TALK_TOPIC=persuade`.

use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};

use bevy::{
    asset::RenderAssetUsages,
    camera::{Exposure, RenderTarget, visibility::RenderLayers},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::view::Tonemapping,
};
use fb_character::check::{Check, CheckRoll, Source};
use procgen_rng::Mulberry32;

use crate::carry::ViewModelCamera;
use crate::inventory::ItemId;
use crate::keymap::{Action, ActionInput, Keymap};
use crate::launch::AppState;
use crate::light::StudioLight;
use crate::ui::{ACCENT, BAD, BG, BRIGHT, FAINT, FRAME_GOLD, INK, LINE, MUTED, OK, UiFonts};

/// Render layer the die and its camera are on (icons 2, doll 3, commission 4, trader 5).
pub const DICE_LAYER: usize = 6;
/// Far from the level and the other stages.
const STAGE: Vec3 = Vec3::new(-30_000.0, -60_000.0, 30_000.0);
const STAGE_PIXELS: u32 = 512;
/// How far the die stands from its camera, with the die's radius 1.
const CAMERA_DISTANCE: f32 = 3.7;

/// Seconds the die tumbles before it lands.
pub const TUMBLE_SECONDS: f32 = 1.5;
/// Seconds between a landed face and each modifier, and the total after the last.
pub const STEP_SECONDS: f32 = 0.5;
/// Seconds the verdict shows before the panel closes itself.
pub const LINGER_SECONDS: f32 = 2.2;

/// What a roll is for, carried through the panel so the requester finds its outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RollPurpose {
    /// Persuading the folk in this entity.
    Persuade(Entity),
    /// Identifying this item.
    Identify(ItemId),
    /// A sample roll for captures.
    Sample,
}

/// Show this roll.
#[derive(Message, Clone, Debug)]
pub struct RollRequest {
    pub roll: CheckRoll,
    pub purpose: RollPurpose,
    /// What the verdict adds under Success or Failure: "+5 standing with the Ash Choir".
    pub result: Option<String>,
}

/// The panel for this roll has closed.
#[derive(Message, Clone, Debug)]
pub struct RollDone {
    pub roll: CheckRoll,
    pub purpose: RollPurpose,
}

/// The roll showing now and those waiting.
#[derive(Resource, Default)]
pub struct Dice {
    queue: VecDeque<RollRequest>,
    active: Option<Active>,
}

struct Active {
    request: RollRequest,
    /// Seconds since the roll began.
    t: f32,
    /// A capture freezes the roll at this time.
    hold: Option<f32>,
    /// What the panel was last drawn for.
    drawn: Option<(Stage, usize)>,
    /// The camera and the die, while the roll shows.
    scene: Vec<Entity>,
}

impl Dice {
    /// Whether a roll is showing or waiting: what rolled for should wait for its outcome.
    pub fn busy(&self) -> bool {
        self.active.is_some() || !self.queue.is_empty()
    }
}

/// How far a roll has played.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Tumbling,
    /// The face is down and modifiers are counting in.
    Counting,
    Total,
    Verdict,
}

impl Active {
    fn modifiers(&self) -> usize {
        self.request.roll.modifiers.len()
    }

    /// When the total shows.
    fn total_at(&self) -> f32 {
        TUMBLE_SECONDS + STEP_SECONDS * (self.modifiers() as f32 + 1.0)
    }

    fn verdict_at(&self) -> f32 {
        self.total_at() + STEP_SECONDS
    }

    /// The stage at the roll's clock, and the modifiers showing.
    fn stage(&self) -> (Stage, usize) {
        let t = self.t;
        if t < TUMBLE_SECONDS {
            (Stage::Tumbling, 0)
        } else if t < self.total_at() {
            (Stage::Counting, (((t - TUMBLE_SECONDS) / STEP_SECONDS) as usize).min(self.modifiers()))
        } else if t < self.verdict_at() {
            (Stage::Total, self.modifiers())
        } else {
            (Stage::Verdict, self.modifiers())
        }
    }
}

pub struct DicePlugin;

impl Plugin for DicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Dice>().init_resource::<DiceStage>().add_message::<RollRequest>().add_message::<RollDone>();
        if std::env::var_os("FB_DEBUG_CHECK").is_some() {
            app.add_systems(Update, sample_for_capture.run_if(in_state(AppState::Delving)));
        }
        app.add_systems(OnExit(AppState::Delving), reset_dice);
        app.add_systems(Update, (take_requests, run_dice, tumble_die).chain().run_if(in_state(AppState::Delving)));
    }
}

fn reset_dice(mut dice: ResMut<Dice>) {
    *dice = Dice::default();
}

/// The image the die is drawn into.
#[derive(Resource)]
struct DiceStage {
    image: Handle<Image>,
}

impl FromWorld for DiceStage {
    fn from_world(world: &mut World) -> Self {
        let mut image = Image::new_target_texture(STAGE_PIXELS, STAGE_PIXELS, bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb, None);
        image.asset_usage = RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD;
        DiceStage { image: world.resource_mut::<Assets<Image>>().add(image) }
    }
}

/// The die's camera, which holds the scene.
#[derive(Component)]
struct DiceRoot;

/// The die's body, turning.
#[derive(Component)]
struct Die {
    /// Turns the rolled face to the camera, upright.
    landed: Quat,
    axis: Vec3,
    /// Radians of tumble left at the start.
    spin: f32,
}

/// For captures (`FB_DEBUG_CHECK=14,0.7`): rolls a sample persuasion once.
fn sample_for_capture(mut frames: Local<u32>, mut requests: MessageWriter<RollRequest>) {
    *frames += 1;
    if *frames != 30 {
        return;
    }
    let spec = std::env::var("FB_DEBUG_CHECK").unwrap_or_default();
    let face = spec.split(',').next().and_then(|v| v.trim().parse::<i32>().ok()).unwrap_or(14).clamp(1, 20);
    let check = Check::d20("Persuade the guard", 15).stat("WIS", 1).class("Cleric", 2).standing(1);
    requests.write(RollRequest { roll: check.with_natural(face), purpose: RollPurpose::Sample, result: Some("+5 standing with the Ash Choir".into()) });
}

fn take_requests(mut requests: MessageReader<RollRequest>, mut dice: ResMut<Dice>) {
    for request in requests.read() {
        dice.queue.push_back(request.clone());
    }
}

/// The panel's root.
#[derive(Component)]
struct DicePanel;

/// Starts the next roll, advances the showing one, redraws its panel when it changes and ends it.
#[allow(clippy::too_many_arguments)]
fn run_dice(
    mut commands: Commands,
    time: Res<Time<Real>>,
    input: ActionInput,
    keymap: Option<Res<Keymap>>,
    mut dice: ResMut<Dice>,
    stage: Res<DiceStage>,
    fonts: Option<Res<UiFonts>>,
    cameras: Query<Entity, With<ViewModelCamera>>,
    mut done: MessageWriter<RollDone>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    panels: Query<Entity, With<DicePanel>>,
) {
    if dice.active.is_none() {
        let Some(next) = dice.queue.pop_front() else { return };
        let hold = std::env::var("FB_DEBUG_ROLL_AT").ok().and_then(|v| v.trim().parse::<f32>().ok()).or_else(|| std::env::var("FB_DEBUG_CHECK").ok().and_then(|v| v.split(',').nth(1).and_then(|s| s.trim().parse::<f32>().ok())));
        dice.active = Some(Active { request: next, t: 0.0, hold, drawn: None, scene: Vec::new() });
    }
    let (Some(fonts), Some(camera)) = (fonts, cameras.iter().next()) else { return };
    let active = dice.active.as_mut().expect("set above");
    if active.scene.is_empty() {
        active.scene = spawn_stage_scene(&mut commands, &stage, &mut meshes, &mut materials, &active.request.roll);
    }
    let delta = time.delta_secs();
    match active.hold {
        // A capture shows the roll at exactly this time, whatever the frame rate.
        Some(hold) => active.t = hold,
        None => active.t += delta,
    }
    if active.hold.is_none() && input.just_pressed(Action::Back) {
        let verdict = active.verdict_at();
        active.t = if active.t < verdict { verdict } else { verdict + LINGER_SECONDS };
    }
    if active.hold.is_none() && active.t >= active.verdict_at() + LINGER_SECONDS {
        let finished = dice.active.take().expect("active");
        for panel in &panels {
            commands.entity(panel).despawn();
        }
        for entity in finished.scene {
            commands.entity(entity).despawn();
        }
        done.write(RollDone { roll: finished.request.roll, purpose: finished.request.purpose });
        return;
    }
    let state = active.stage();
    if active.drawn != Some(state) || panels.is_empty() {
        for panel in &panels {
            commands.entity(panel).despawn();
        }
        active.drawn = Some(state);
        let fallback = Keymap::default();
        let skip = keymap.as_deref().unwrap_or(&fallback).label(Action::Back);
        let clear = match active.request.purpose {
            // The talk panel stands on the right: the roll sits in the room left of it.
            RollPurpose::Persuade(_) => crate::talk::PANEL_WIDTH + crate::talk::PANEL_MARGIN + 24.0,
            _ => 0.0,
        };
        spawn_panel(&mut commands, &fonts, camera, &stage, &active.request.roll, active.request.result.as_deref(), state, &skip, clear);
    }
}

fn signed(n: i32) -> String {
    if n < 0 { format!("\u{2212}{}", -n) } else { format!("+{n}") }
}

fn source_label(source: Source) -> &'static str {
    match source {
        Source::Stat => "ability",
        Source::Class => "class",
        Source::Affix => "gear",
        Source::Standing => "standing",
    }
}

fn rule(col: &mut ChildSpawnerCommands) {
    col.spawn((Node { height: px(1), margin: UiRect::vertical(px(4)), ..default() }, BackgroundColor(FAINT)));
}

fn spawn_panel(commands: &mut Commands, fonts: &UiFonts, camera: Entity, stage: &DiceStage, roll: &CheckRoll, result: Option<&str>, (phase, shown): (Stage, usize), skip: &str, clear_right: f32) {
    let landed = phase != Stage::Tumbling;
    let running: i32 = roll.natural + roll.modifiers.iter().take(shown).map(|m| m.amount).sum::<i32>();
    let verdict = phase == Stage::Verdict;
    let totalled = matches!(phase, Stage::Total | Stage::Verdict);
    commands
        .spawn((
            Name::new("Dice roll"),
            DicePanel,
            DespawnOnExit(AppState::Delving),
            UiTargetCamera(camera),
            GlobalZIndex(40),
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, padding: UiRect::right(px(clear_right)), ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        ))
        .with_children(|screen| {
            screen
                .spawn((
                    Node {
                        width: px(760),
                        max_width: percent(96),
                        max_height: percent(94),
                        column_gap: px(26),
                        padding: UiRect::new(px(24), px(30), px(22), px(22)),
                        border: UiRect::all(px(1.5)),
                        border_radius: BorderRadius::all(px(14)),
                        ..default()
                    },
                    BackgroundColor(BG),
                    BorderColor::all(LINE),
                    BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.55), px(0), px(14), px(0), px(34)),
                ))
                .with_children(|panel| {
                    panel.spawn((ImageNode::new(stage.image.clone()), Node { width: px(300), height: px(300), flex_shrink: 0.0, border_radius: BorderRadius::all(px(10)), ..default() }));
                    panel.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(7), flex_grow: 1.0, min_width: px(0), ..default() }).with_children(|col| {
                        col.spawn((Text::new(roll.name.clone()), UiFonts::style(&fonts.display, 32.0, INK)));
                        col.spawn((Text::new(format!("d{} against {}", roll.sides, roll.target)), UiFonts::style(&fonts.body_italic, 18.0, MUTED)));
                        rule(col);
                        let row = |col: &mut ChildSpawnerCommands, label: String, value: String, tag: &str, colour: Color| {
                            col.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Baseline, column_gap: px(12), ..default() }).with_children(|row| {
                                row.spawn(Node { column_gap: px(10), align_items: AlignItems::Baseline, ..default() }).with_children(|left| {
                                    left.spawn((Text::new(label), UiFonts::style(&fonts.body, 22.0, INK)));
                                    left.spawn((Text::new(tag.to_string()), UiFonts::style(&fonts.mono, 13.0, MUTED)));
                                });
                                row.spawn((Text::new(value), UiFonts::style(&fonts.mono_bold, 24.0, colour)));
                            });
                        };
                        if landed {
                            row(col, format!("d{}", roll.sides), roll.natural.to_string(), "the die", BRIGHT);
                        } else {
                            col.spawn((Text::new("Rolling..."), UiFonts::style(&fonts.body_italic, 22.0, MUTED)));
                        }
                        for m in roll.modifiers.iter().take(shown) {
                            row(col, m.name.clone(), signed(m.amount), if m.name.eq_ignore_ascii_case(source_label(m.source)) { "" } else { source_label(m.source) }, if m.amount < 0 { BAD } else { ACCENT });
                        }
                        if landed {
                            rule(col);
                            let colour = match (verdict, roll.succeeded) {
                                (true, true) => OK,
                                (true, false) => BAD,
                                _ => BRIGHT,
                            };
                            col.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Baseline, ..default() }).with_children(|row| {
                                row.spawn((Text::new(if totalled { "Total" } else { "So far" }), UiFonts::style(&fonts.body_medium, 22.0, INK)));
                                row.spawn((Text::new(format!("{}", if totalled { roll.total } else { running })), UiFonts::style(&fonts.mono_bold, 40.0, colour)));
                            });
                            if totalled {
                                col.spawn((Text::new(format!("needs {}", roll.target)), UiFonts::style(&fonts.mono, 14.0, MUTED)));
                            }
                        }
                        if verdict {
                            let (word, colour) = if roll.succeeded { ("Success", OK) } else { ("Failure", BAD) };
                            col.spawn((
                                Node { padding: UiRect::axes(px(16), px(6)), border: UiRect::all(px(1.5)), border_radius: BorderRadius::all(px(8)), margin: UiRect::top(px(6)), align_self: AlignSelf::FlexStart, ..default() },
                                BorderColor::all(colour),
                                BackgroundColor(colour.with_alpha(0.14)),
                            ))
                            .with_children(|b| {
                                b.spawn((Text::new(word), UiFonts::style(&fonts.display_bold, 34.0, colour)));
                            });
                        }
                        if let (true, Some(result)) = (verdict, result) {
                            col.spawn((Text::new(result.to_string()), UiFonts::style(&fonts.body_medium, 18.0, if roll.succeeded { OK } else { BAD })));
                        }
                        let foot = if verdict { format!("{skip} continue") } else { format!("{skip} skip") };
                        col.spawn((Node { margin: UiRect::top(px(8)), ..default() }, Text::new(foot), UiFonts::style(&fonts.mono, 13.0, MUTED)));
                    });
                });
        });
}

// The die.

const PHI: f32 = 1.618_034;

/// The icosahedron's 12 corners, on the unit sphere, and its 20 faces, wound outward.
fn icosahedron() -> (Vec<Vec3>, Vec<[usize; 3]>) {
    let corners = [
        (-1.0, PHI, 0.0),
        (1.0, PHI, 0.0),
        (-1.0, -PHI, 0.0),
        (1.0, -PHI, 0.0),
        (0.0, -1.0, PHI),
        (0.0, 1.0, PHI),
        (0.0, -1.0, -PHI),
        (0.0, 1.0, -PHI),
        (PHI, 0.0, -1.0),
        (PHI, 0.0, 1.0),
        (-PHI, 0.0, -1.0),
        (-PHI, 0.0, 1.0),
    ]
    .map(|(x, y, z)| Vec3::new(x, y, z).normalize());
    let faces: [[usize; 3]; 20] = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    let wound = faces
        .map(|[a, b, c]| {
            let n = (corners[b] - corners[a]).cross(corners[c] - corners[a]);
            if n.dot(corners[a] + corners[b] + corners[c]) < 0.0 { [a, c, b] } else { [a, b, c] }
        })
        .to_vec();
    (corners.to_vec(), wound)
}

fn face_normals(corners: &[Vec3], faces: &[[usize; 3]]) -> Vec<Vec3> {
    faces.iter().map(|f| (corners[f[1]] - corners[f[0]]).cross(corners[f[2]] - corners[f[0]]).normalize()).collect()
}

/// Which number sits on each face: opposite faces add to 21.
fn numbering(normals: &[Vec3]) -> Vec<u32> {
    let mut number = vec![0u32; normals.len()];
    let mut next = 1;
    for i in 0..normals.len() {
        if number[i] != 0 {
            continue;
        }
        let opposite = (0..normals.len()).min_by(|&a, &b| normals[i].dot(normals[a]).total_cmp(&normals[i].dot(normals[b]))).expect("faces");
        number[i] = next;
        number[opposite] = 21 - next;
        next += 1;
    }
    number
}

/// A digit as strokes on a 2 by 4 grid, the points of each run joined in order.
fn digit_strokes(digit: u32) -> &'static [&'static [(f32, f32)]] {
    match digit {
        0 => &[&[(0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0), (0.0, 0.0)]],
        1 => &[&[(1.0, 0.0), (1.0, 4.0)], &[(0.3, 3.2), (1.0, 4.0)]],
        2 => &[&[(0.0, 4.0), (2.0, 4.0), (2.0, 2.0), (0.0, 2.0), (0.0, 0.0), (2.0, 0.0)]],
        3 => &[&[(0.0, 4.0), (2.0, 4.0), (2.0, 0.0), (0.0, 0.0)], &[(0.4, 2.0), (2.0, 2.0)]],
        4 => &[&[(0.0, 4.0), (0.0, 2.0), (2.0, 2.0)], &[(2.0, 4.0), (2.0, 0.0)]],
        5 => &[&[(2.0, 4.0), (0.0, 4.0), (0.0, 2.0), (2.0, 2.0), (2.0, 0.0), (0.0, 0.0)]],
        6 => &[&[(2.0, 4.0), (0.0, 4.0), (0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]],
        7 => &[&[(0.0, 4.0), (2.0, 4.0), (1.0, 0.0)]],
        8 => &[&[(0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0), (0.0, 0.0)], &[(0.0, 2.0), (2.0, 2.0)]],
        _ => &[&[(0.0, 0.0), (2.0, 0.0), (2.0, 4.0), (0.0, 4.0), (0.0, 2.0), (2.0, 2.0)]],
    }
}

type Segment = ((f32, f32), (f32, f32));

/// The strokes of `n` as segments on the grid, digits side by side, with an underline for the
/// 6 and the 9 so they read either way up. Returns the segments and the width.
fn number_strokes(n: u32) -> (Vec<Segment>, f32) {
    let digits: Vec<u32> = n.to_string().chars().filter_map(|c| c.to_digit(10)).collect();
    let mut segments = Vec::new();
    for (i, d) in digits.iter().enumerate() {
        let dx = i as f32 * 3.0;
        for run in digit_strokes(*d) {
            for pair in run.windows(2) {
                segments.push(((pair[0].0 + dx, pair[0].1), (pair[1].0 + dx, pair[1].1)));
            }
        }
    }
    let width = digits.len() as f32 * 3.0 - 1.0;
    if n == 6 || n == 9 {
        segments.push(((0.0, -1.2), (width, -1.2)));
    }
    (segments, width)
}

/// The die's body (flat-shaded faces) and its numerals (strokes laid just over each face), as
/// meshes, and the rotation that brings the face of each number to the front, upright.
struct DieShape {
    body: Mesh,
    numerals: Mesh,
    /// The face with number `n` lands with `fronts[n - 1]`.
    fronts: Vec<Quat>,
}

fn mesh_of(positions: Vec<[f32; 3]>, normals: Vec<[f32; 3]>, indices: Vec<u32>) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(indices))
}

fn build_die() -> DieShape {
    let (corners, faces) = icosahedron();
    let normals = face_normals(&corners, &faces);
    let number = numbering(&normals);
    let (mut positions, mut shading, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    let (mut ink_positions, mut ink_shading, mut ink_indices) = (Vec::new(), Vec::new(), Vec::new());
    let mut fronts = vec![Quat::IDENTITY; faces.len()];
    for (i, face) in faces.iter().enumerate() {
        let n = normals[i];
        let base = positions.len() as u32;
        for &c in face {
            positions.push(corners[c].to_array());
            shading.push(n.to_array());
        }
        indices.extend([base, base + 1, base + 2]);
        let centre = (corners[face[0]] + corners[face[1]] + corners[face[2]]) / 3.0;
        let up = (corners[face[0]] - centre).normalize();
        let right = up.cross(n).normalize();
        let up = n.cross(right);
        fronts[number[i] as usize - 1] = Quat::from_mat3(&Mat3::from_cols(right, up, n).transpose());
        // Numerals: a stroke is a thin quad in the face's plane, just proud of it.
        let edge = (corners[face[1]] - corners[face[0]]).length();
        let (segments, width) = number_strokes(number[i]);
        let unit = edge * 0.30 / 4.0;
        let thick = unit * 0.34;
        let origin = centre + n * 0.012 - right * (width * unit * 0.5) - up * (2.0 * unit);
        for (a, b) in segments {
            let pa = origin + right * (a.0 * unit) + up * (a.1 * unit);
            let pb = origin + right * (b.0 * unit) + up * (b.1 * unit);
            let along = (pb - pa).normalize();
            let across = n.cross(along) * (thick * 0.5);
            let ext = along * (thick * 0.5);
            let q = ink_positions.len() as u32;
            for p in [pa - ext - across, pb + ext - across, pb + ext + across, pa - ext + across] {
                ink_positions.push(p.to_array());
                ink_shading.push(n.to_array());
            }
            ink_indices.extend([q, q + 1, q + 2, q, q + 2, q + 3]);
        }
    }
    DieShape { body: mesh_of(positions, shading, indices), numerals: mesh_of(ink_positions, ink_shading, ink_indices), fronts }
}

/// The die and its camera, with the die set to tumble to `roll`'s face. Returns the camera and the die.
fn spawn_stage_scene(commands: &mut Commands, stage: &DiceStage, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, roll: &CheckRoll) -> Vec<Entity> {
    let shape = build_die();
    let layer = RenderLayers::layer(DICE_LAYER);
    let exposure = Exposure::INDOOR;
    let ambient = StudioLight::ARMORY;
    let face = (roll.natural.clamp(1, 20) - 1) as usize;
    let mut rng = Mulberry32::new((roll.natural as u32).wrapping_mul(0x9E37_79B1) ^ roll.target as u32 ^ (roll.total as u32) << 8);
    let axis = Vec3::new(rng.range_f32(-1.0, 1.0), rng.range_f32(-1.0, 1.0), rng.range_f32(-0.6, 0.6)).try_normalize().unwrap_or(Vec3::Y);
    let spin = 3.0 * TAU + rng.range_f32(0.0, TAU);
    let landed = shape.fronts[face];
    let body = materials.add(StandardMaterial { base_color: Color::srgb(0.10, 0.14, 0.30), perceptual_roughness: 0.38, metallic: 0.05, reflectance: 0.6, ..default() });
    let gold = materials.add(StandardMaterial { base_color: FRAME_GOLD, emissive: FRAME_GOLD.to_linear() * 0.55, perceptual_roughness: 0.3, metallic: 0.6, ..default() });
    let root = commands
        .spawn((
            Name::new("Dice camera"),
            DiceRoot,
            crate::render::ScreenInkExempt,
            DespawnOnExit(AppState::Delving),
            Camera3d::default(),
            Camera { order: -4, clear_color: ClearColorConfig::Custom(Color::srgb(0.055, 0.07, 0.13)), is_active: true, ..default() },
            RenderTarget::Image(stage.image.clone().into()),
            Projection::Perspective(PerspectiveProjection { fov: 0.62, near: 0.1, far: 100.0, ..default() }),
            exposure,
            Tonemapping::Linear,
            AmbientLight { color: ambient.ambient_color, brightness: ambient.ambient * 1.3 / exposure.exposure(), ..default() },
            layer.clone(),
            Transform::from_translation(STAGE + Vec3::new(0.0, 0.0, CAMERA_DISTANCE)).looking_at(STAGE, Vec3::Y),
        ))
        .with_children(|camera| {
            let (light, at) = ambient.key_light(&exposure);
            camera.spawn((light, at, layer.clone()));
        })
        .id();
    let die = commands
        .spawn((Name::new("Die"), Die { landed, axis, spin }, Transform::from_translation(STAGE), Visibility::default(), layer.clone()))
        .with_children(|die| {
            die.spawn((Mesh3d(meshes.add(shape.body)), MeshMaterial3d(body), layer.clone()));
            die.spawn((Mesh3d(meshes.add(shape.numerals)), MeshMaterial3d(gold), layer.clone()));
        })
        .id();
    vec![root, die]
}

/// Turns the die: tumbling, slowing, and landing square on its rolled face.
fn tumble_die(dice: Res<Dice>, mut dies: Query<(&Die, &mut Transform)>) {
    let Some(active) = dice.active.as_ref() else { return };
    let p = (active.t / TUMBLE_SECONDS).clamp(0.0, 1.0);
    let left = (1.0 - p).powi(2);
    for (die, mut at) in &mut dies {
        at.rotation = Quat::from_axis_angle(die.axis, die.spin * left) * die.landed;
        // A hop that dies away as it settles.
        let hop = (p * PI * 3.0).sin().abs() * (1.0 - p).powi(2) * 0.45;
        at.translation = STAGE + Vec3::new(0.0, hop, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_faces_add_to_twenty_one_and_every_number_appears_once() {
        let (corners, faces) = icosahedron();
        let normals = face_normals(&corners, &faces);
        let number = numbering(&normals);
        let mut seen = number.clone();
        seen.sort();
        assert_eq!(seen, (1..=20).collect::<Vec<_>>());
        for i in 0..20 {
            let opposite = (0..20).find(|&j| normals[i].dot(normals[j]) < -0.99).unwrap();
            assert_eq!(number[i] + number[opposite], 21);
        }
    }

    #[test]
    fn every_face_lands_toward_the_camera_upright() {
        let shape = build_die();
        let (corners, faces) = icosahedron();
        let normals = face_normals(&corners, &faces);
        let number = numbering(&normals);
        for (i, n) in normals.iter().enumerate() {
            let q = shape.fronts[number[i] as usize - 1];
            assert!((q * *n - Vec3::Z).length() < 1e-4, "face {i}");
        }
    }

    #[test]
    fn the_roll_stages_follow_the_clock() {
        let check = Check::d20("x", 10).stat("WIS", 1).standing(1);
        let mut active = Active { request: RollRequest { roll: check.with_natural(5), purpose: RollPurpose::Sample, result: None }, t: 0.0, hold: None, drawn: None, scene: Vec::new() };
        assert_eq!(active.stage(), (Stage::Tumbling, 0));
        active.t = TUMBLE_SECONDS + STEP_SECONDS * 1.5;
        assert_eq!(active.stage(), (Stage::Counting, 1));
        active.t = active.total_at();
        assert_eq!(active.stage(), (Stage::Total, 2));
        active.t = active.verdict_at();
        assert_eq!(active.stage(), (Stage::Verdict, 2));
    }
}
