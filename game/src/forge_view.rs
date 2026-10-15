//! The Forge view (`--forge`): one Armory item turning on the turntable's plate, and beside it
//! a panel of every parameter of its state, like the Forge sliders of
//! `reference/spline-armory.html`. Pick a family, roll a seed with "Forge new", and drag a
//! slider, cycle a choice or flip a toggle: the mesh rebuilds live.
//!
//! The panel is generic over families. [`armory::Item::fields`] lists each section's sliders
//! (with the family's own ranges and steps), choices and toggles, so a family needs nothing
//! here. Edits change only the displayed state; the seed still rolls the same item
//! ([`Forge::reset`] goes back to it).
//!
//! Keys: Tab and Shift+Tab change the family, N forges a new seed, R resets the item to its
//! seed's roll, Left and Right step the seed, Space pauses the turn. Mouse: dragging the stage
//! orbits and scrolling zooms; on the panel, dragging a slider sets it, clicking a choice
//! cycles it (right click goes back), and scrolling moves the panel. Command line:
//! `--forge --family lantern --seed 7` ([`Forge::from_args`]).

use armory::{
    Family, Item,
    cond::{Condition, ConditionState},
    reflect::{Field, FieldKind},
    slider::Slider,
};
use bevy::{
    camera::Exposure,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    render::view::Tonemapping,
    ui::IsDefaultUiCamera,
    window::PrimaryWindow,
};

use crate::light::StudioLight;
use crate::menu::fraction_along;
use crate::render::{ItemAppearance, ItemPalette, ItemSpawner, item_bounds};
use crate::turntable::{Orbit, PLATE, PLATE_INK, SPIN_SPEED, framing_distance, parse_family, wear_name};
use crate::ui::{BG, BRIGHT, CELL, FAINT, GOLD, INK, LINE, MUTED, UiFonts, node_rect};

/// Width of the panel on the right, in logical pixels.
pub const PANEL_WIDTH: f32 = 430.0;
/// Share of the stage's height the item fills.
const FILL: f32 = 0.8;
const DEFAULT_SEED: u32 = 1207;
/// Pixels one scroll line moves the panel.
const SCROLL_LINE: f32 = 36.0;
/// The condition presets, newest to oldest.
const WEAR_PRESETS: [ConditionState; 5] =
    [ConditionState::Pristine, ConditionState::Serviceable, ConditionState::BattleWorn, ConditionState::Neglected, ConditionState::GraveFind];

pub struct ForgeViewPlugin;

impl Plugin for ForgeViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Forge>()
            .init_resource::<UiFonts>()
            .init_resource::<Spin>()
            .init_resource::<HeldSlider>()
            .init_resource::<Stage>()
            .insert_resource(ClearColor(PLATE))
            .insert_resource(StudioLight::ARMORY.ambient_light(&Exposure::INDOOR))
            .add_systems(Startup, (setup_stage, setup_panel))
            .add_systems(
                Update,
                (
                    keys,
                    press_controls,
                    drag_slider,
                    orbit_and_scroll,
                    show_item.run_if(resource_changed::<Forge>),
                    build_sections.run_if(resource_changed::<Forge>),
                    refresh_panel.run_if(resource_changed::<Forge>),
                    spin,
                    frame_camera,
                )
                    .chain(),
            );
    }
}

/// The item being forged. Changing it shows the new state; `structure` counts the changes that
/// give the item a new shape of panel (a new family or seed), where the others only move
/// values.
#[derive(Resource, Clone, Debug)]
pub struct Forge {
    pub family: Family,
    pub seed: u32,
    pub item: Item,
    structure: u32,
}

impl Default for Forge {
    fn default() -> Self {
        Forge::new(Family::Mace, DEFAULT_SEED).expect("mace is ported")
    }
}

impl Forge {
    /// The item `family` rolls from `seed`, or `None` for a family that is not ported yet.
    pub fn new(family: Family, seed: u32) -> Option<Forge> {
        Some(Forge { family, seed, item: Item::roll(family, seed)?, structure: 0 })
    }

    /// Parses `--family <key>` and `--seed <n>` (also `--flag=value`); `--forge` itself is
    /// skipped. `args` excludes the program name.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<Forge, String> {
        let (mut family, mut seed) = (Family::Mace, DEFAULT_SEED);
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (arg, None),
            };
            let mut value = || inline.clone().or_else(|| args.next()).ok_or(format!("{flag} needs a value"));
            match flag.as_str() {
                "--forge" => {}
                "--family" => family = parse_family(&value()?)?,
                "--seed" => {
                    let text = value()?;
                    seed = text.parse().map_err(|_| format!("--seed: {text:?} is not a u32"))?;
                }
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        Forge::new(family, seed).ok_or_else(|| format!("{} is not ported yet", family.key()))
    }

    /// Rolls the item again from the current seed, dropping every edit.
    pub fn reset(&mut self) {
        if let Some(item) = Item::roll(self.family, self.seed) {
            self.item = item;
        }
        self.structure += 1;
    }

    /// Moves to `seed` (the same family), rolling its item.
    pub fn set_seed(&mut self, seed: u32) {
        self.seed = seed;
        self.reset();
    }

    /// "Forge new": a seed picked from `entropy` (the clock, in the app), as the prototype's
    /// button rolls a random one.
    pub fn forge_new(&mut self, entropy: u64) {
        self.set_seed(fresh_seed(entropy));
    }

    /// Steps `by` ported families through [`Item::PORTED`], wrapping, and rolls the seed in
    /// the new one.
    pub fn step_family(&mut self, by: isize) {
        let ported = Item::PORTED;
        let at = ported.iter().position(|&f| f == self.family).unwrap_or(0) as isize;
        self.set_family(ported[(at + by).rem_euclid(ported.len() as isize) as usize]);
    }

    pub fn set_family(&mut self, family: Family) {
        if let Some(item) = Item::roll(family, self.seed) {
            self.family = family;
            self.item = item;
            self.structure += 1;
        }
    }

    /// Applies a condition preset to the item, keeping its own wear pattern scale.
    pub fn set_wear(&mut self, state: ConditionState) {
        let condition = wear_preset(self.item.condition(), state);
        self.item.set_condition(condition);
    }

    /// How the item looks on the stage: its own palette, wear pattern and etching.
    pub fn appearance(&self) -> ItemAppearance {
        ItemAppearance {
            palette: ItemPalette { materials: self.item.palette(), ..ItemPalette::ARMORY },
            condition: self.item.condition(),
            seed: self.item.seed(),
            etching: self.item.etching(),
            ..default()
        }
    }
}

/// A seed spread over all of `u32` from a counter or a clock reading (SplitMix64's finaliser).
pub fn fresh_seed(entropy: u64) -> u32 {
    let mut z = entropy.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (z ^ (z >> 31)) as u32
}

/// `condition` set to the named preset's sliders, keeping the pattern scale it had.
pub fn wear_preset(condition: Condition, state: ConditionState) -> Condition {
    Condition { scale: condition.scale, ..Condition::of(state) }
}

/// A field's name as a label: `gnarl_freq` is "gnarl freq".
pub fn field_label(name: &str) -> String {
    name.replace('_', " ")
}

/// A section's heading: `cond` is "Condition", `head.gem` is "Head / gem".
pub fn section_title(section: &str) -> String {
    let words: Vec<String> = section
        .split('.')
        .enumerate()
        .map(|(i, part)| match part {
            "cond" => "Condition".to_string(),
            _ => {
                let part = field_label(part);
                if i == 0 { capitalise(&part) } else { part }
            }
        })
        .collect();
    words.join(" / ")
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

/// A slider value with as many decimals as its step has: `0.005` shows three.
pub fn format_value(value: f32, step: f32) -> String {
    let decimals = if step >= 1.0 { 0 } else { (-step.log10()).ceil().clamp(0.0, 4.0) as usize };
    format!("{value:.decimals$}")
}

/// The slider value a third of the way along a bar, say: `fraction` 0 to 1 over `range`.
pub fn value_at(range: &Slider, fraction: f32) -> f32 {
    range.min + fraction.clamp(0.0, 1.0) * (range.max - range.min)
}

/// How far along its bar `value` sits, 0 to 1.
pub fn fraction_of(range: &Slider, value: f32) -> f32 {
    if range.max <= range.min { 0.0 } else { ((value - range.min) / (range.max - range.min)).clamp(0.0, 1.0) }
}

/// A choice's option as a label: `pierced_tin` is "pierced tin".
fn option_label(option: &str) -> String {
    field_label(option)
}

/// The slider being dragged, from the press that caught it until the button comes up.
#[derive(Resource, Default)]
struct HeldSlider(Option<(String, Slider)>);

/// Spin angle of the stage's turntable.
#[derive(Resource, Default)]
struct Spin {
    angle: f32,
    paused: bool,
    orbit: Orbit,
}

/// How the item is framed: set when a new family or seed arrives, so dragging a slider that
/// grows the item does not keep re-fitting the camera.
#[derive(Resource, Default)]
struct Stage {
    size: Vec3,
    /// Where the item's bounds centre was, so the item turns about it.
    centre: Vec3,
    structure: Option<u32>,
}

#[derive(Component)]
struct StageCamera;

/// The turning pivot the item hangs on.
#[derive(Component)]
struct Pivot;

#[derive(Component)]
struct Hint;

/// The scrolling area of the panel.
#[derive(Component)]
struct Body;

/// What a click or drag on a control does.
#[derive(Component, Clone, Debug, PartialEq)]
enum Control {
    Family(Family),
    ForgeNew,
    Reset,
    SeedStep(i64),
    Wear(ConditionState),
    /// A slider's bar, by field path.
    Slider(String, Slider),
    Cycle(String),
    Toggle(String),
}

/// Whether a control sits in the scrolling body (and so can only be hit within its view).
#[derive(Component)]
struct Scrolls;

/// Panel text that follows the forge.
#[derive(Component, Clone, Debug, PartialEq)]
enum Readout {
    Noun,
    Seed,
    Value(String, f32),
    Choice(String),
}

/// A slider's fill or knob.
#[derive(Component)]
struct SliderGraphic {
    path: String,
    range: Slider,
    knob: bool,
}

#[derive(Component)]
struct ToggleMark(String);

/// A family or condition button, lit when it is the current one.
#[derive(Component, Clone, Copy, PartialEq)]
enum Chip {
    Family(Family),
    Wear(ConditionState),
}

/// The node the sections are built under.
#[derive(Component)]
struct Sections;

fn setup_stage(mut commands: Commands, fonts: Res<UiFonts>) {
    let exposure = Exposure::INDOOR;
    commands
        .spawn((
            StageCamera,
            Camera3d::default(),
            exposure,
            // As on the turntable: a tone curve would flatten the bands.
            Tonemapping::Linear,
            IsDefaultUiCamera,
            Transform::from_xyz(0.0, 0.0, 60.0),
        ))
        .with_child(StudioLight::ARMORY.key_light(&exposure));
    commands.spawn((
        Hint,
        Text::new("Drag orbit   Scroll zoom   Tab family   N forge new   R reset   Space pause"),
        UiFonts::style(&fonts.body_medium, 17.0, PLATE_INK),
        Node { position_type: PositionType::Absolute, left: px(20), bottom: px(16), ..default() },
    ));
}

/// The ledger button: a cell-dark box ruled in tan.
fn chip<'a>(parent: &'a mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, control: Control) -> EntityCommands<'a> {
    let mut chip = parent.spawn((
        control,
        Node {
            padding: UiRect::axes(px(9), px(3)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(CELL),
        BorderColor::all(LINE),
    ));
    chip.with_child((Text::new(label), UiFonts::style(&fonts.body_medium, 16.0, INK)));
    chip
}

fn setup_panel(mut commands: Commands, fonts: Res<UiFonts>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(0),
                top: px(0),
                bottom: px(0),
                width: px(PANEL_WIDTH),
                flex_direction: FlexDirection::Column,
                border: UiRect::left(px(1)),
                ..default()
            },
            BackgroundColor(BG),
            BorderColor::all(LINE),
        ))
        .with_children(|panel| {
            panel
                .spawn((
                    Node { flex_direction: FlexDirection::Column, row_gap: px(9), padding: UiRect::all(px(14)), border: UiRect::bottom(px(1)), ..default() },
                    BorderColor::all(FAINT),
                ))
                .with_children(|header| {
                    header.spawn((Text::new("The Forge"), UiFonts::style(&fonts.display_bold, 28.0, INK)));
                    header.spawn((Readout::Noun, Text::new(""), UiFonts::style(&fonts.display, 20.0, BRIGHT)));
                    header
                        .spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(5), row_gap: px(5), ..default() })
                        .with_children(|row| {
                            for family in Item::PORTED {
                                chip(row, &fonts, family.key(), Control::Family(*family)).insert(Chip::Family(*family));
                            }
                        });
                    header.spawn(Node { align_items: AlignItems::Center, column_gap: px(6), ..default() }).with_children(|row| {
                        row.spawn((Text::new("Seed"), UiFonts::style(&fonts.body, 17.0, MUTED), Node { margin: UiRect::right(px(4)), ..default() }));
                        chip(row, &fonts, "-", Control::SeedStep(-1));
                        row.spawn((
                            Readout::Seed,
                            Text::new(""),
                            UiFonts::style(&fonts.mono_bold, 16.0, BRIGHT),
                            TextLayout::justify(Justify::Center),
                            Node { min_width: px(92), ..default() },
                        ));
                        chip(row, &fonts, "+", Control::SeedStep(1));
                        chip(row, &fonts, "Forge new", Control::ForgeNew);
                        chip(row, &fonts, "Reset", Control::Reset);
                    });
                    header.spawn(Node { align_items: AlignItems::Center, flex_wrap: FlexWrap::Wrap, column_gap: px(5), row_gap: px(5), ..default() }).with_children(|row| {
                        row.spawn((Text::new("Wear"), UiFonts::style(&fonts.body, 17.0, MUTED), Node { margin: UiRect::right(px(4)), ..default() }));
                        for state in WEAR_PRESETS {
                            chip(row, &fonts, wear_name(state), Control::Wear(state)).insert(Chip::Wear(state));
                        }
                    });
                });
            panel
                .spawn((
                    Body,
                    ScrollPosition::default(),
                    Node {
                        flex_grow: 1.0,
                        min_height: px(0),
                        flex_direction: FlexDirection::Column,
                        overflow: Overflow::scroll_y(),
                        padding: UiRect::axes(px(14), px(6)),
                        ..default()
                    },
                ))
                .with_child((Sections, Node { flex_direction: FlexDirection::Column, row_gap: px(2), padding: UiRect::bottom(px(30)), ..default() }));
        });
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut forge: ResMut<Forge>, mut spin: ResMut<Spin>) {
    let pressed = |key| keys.just_pressed(key);
    if pressed(KeyCode::Tab) {
        let back = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
        forge.step_family(if back { -1 } else { 1 });
    }
    if pressed(KeyCode::ArrowRight) {
        let seed = forge.seed.wrapping_add(1);
        forge.set_seed(seed);
    }
    if pressed(KeyCode::ArrowLeft) {
        let seed = forge.seed.wrapping_sub(1);
        forge.set_seed(seed);
    }
    if pressed(KeyCode::KeyN) {
        forge.forge_new(clock());
    }
    if pressed(KeyCode::KeyR) {
        forge.reset();
    }
    if pressed(KeyCode::Space) {
        spin.paused = !spin.paused;
    }
}

fn clock() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64)
}

/// The control under `point`, topmost last. Controls in the scrolling body count only inside
/// `body`.
fn control_at<'a>(
    point: Vec2,
    body: Rect,
    controls: impl IntoIterator<Item = (&'a Control, &'a ComputedNode, &'a UiGlobalTransform, bool)>,
) -> Option<&'a Control> {
    controls
        .into_iter()
        .filter(|(_, node, transform, scrolls)| node_rect(node, transform).contains(point) && (!scrolls || body.contains(point)))
        .map(|(control, ..)| control)
        .next()
}

/// A press on a control does what it says. A slider starts being dragged ([`drag_slider`]).
#[allow(clippy::too_many_arguments)]
fn press_controls(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    controls: Query<(&Control, &ComputedNode, &UiGlobalTransform, Has<Scrolls>)>,
    body: Query<(&ComputedNode, &UiGlobalTransform), With<Body>>,
    mut forge: ResMut<Forge>,
    mut held: ResMut<HeldSlider>,
) {
    if !buttons.pressed(MouseButton::Left) {
        held.0 = None;
    }
    let left = buttons.just_pressed(MouseButton::Left);
    let right = buttons.just_pressed(MouseButton::Right);
    if !(left || right) {
        return;
    }
    let Some(point) = window.and_then(|w| w.cursor_position()) else { return };
    let Ok((node, transform)) = body.single() else { return };
    let body = node_rect(node, transform);
    let Some(control) = control_at(point, body, controls.iter()) else { return };
    match control.clone() {
        Control::Family(family) if left => forge.set_family(family),
        Control::ForgeNew if left => forge.forge_new(clock()),
        Control::Reset if left => forge.reset(),
        Control::SeedStep(by) if left => {
            let seed = forge.seed.wrapping_add_signed(by as i32);
            forge.set_seed(seed);
        }
        Control::Wear(state) if left => forge.set_wear(state),
        Control::Slider(path, range) if left => held.0 = Some((path, range)),
        Control::Cycle(path) => {
            let _ = forge.item.cycle_choice(&path, if left { 1 } else { -1 });
        }
        Control::Toggle(path) if left => {
            let on = forge.item.toggle(&path).unwrap_or(false);
            let _ = forge.item.set_toggle(&path, !on);
        }
        _ => {}
    }
}

/// While the button is down, the slider pressed on follows the mouse along its bar.
fn drag_slider(
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    held: Res<HeldSlider>,
    controls: Query<(&Control, &ComputedNode, &UiGlobalTransform)>,
    mut forge: ResMut<Forge>,
) {
    let Some((path, range)) = held.0.clone() else { return };
    let Some(point) = window.and_then(|w| w.cursor_position()) else { return };
    let Some((_, node, transform)) = controls.iter().find(|(c, ..)| matches!(c, Control::Slider(p, _) if *p == path)) else { return };
    let target = value_at(&range, fraction_along(node_rect(node, transform), point.x));
    let current = forge.bypass_change_detection().item.slider(&path);
    let snapped = if target >= range.max {
        range.max
    } else if target <= range.min {
        range.min
    } else {
        range.snap(target)
    };
    if current.is_none_or(|c| (c - snapped).abs() > f32::EPSILON) {
        let _ = forge.item.set_slider(&path, snapped);
    }
}

/// Dragging the stage orbits and scrolling over it zooms; scrolling over the panel moves it.
fn orbit_and_scroll(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mut spin: ResMut<Spin>,
    mut body: Query<(&mut ScrollPosition, &ComputedNode), With<Body>>,
    mut dragging_stage: Local<bool>,
) {
    let Some(window) = window else { return };
    let over_panel = window.cursor_position().is_some_and(|c| c.x >= window.width() - PANEL_WIDTH);
    if buttons.just_pressed(MouseButton::Left) {
        *dragging_stage = !over_panel;
    }
    if !buttons.pressed(MouseButton::Left) {
        *dragging_stage = false;
    }
    if *dragging_stage && motion.delta != Vec2::ZERO {
        spin.orbit.drag(motion.delta);
    }
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    if lines == 0.0 || window.cursor_position().is_none() {
        return;
    }
    if over_panel {
        for (mut position, node) in &mut body {
            let max = ((node.content_size() - node.size()) * node.inverse_scale_factor).y.max(0.0);
            position.y = (position.y - lines * SCROLL_LINE).clamp(0.0, max);
        }
    } else {
        spin.orbit.zoom_by(lines);
    }
}

/// Rebuilds the item on its pivot for the current state.
fn show_item(
    mut commands: Commands,
    forge: Res<Forge>,
    mut stage: ResMut<Stage>,
    old: Query<Entity, With<Pivot>>,
    mut spawner: ItemSpawner,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let mesh = forge.item.build();
    let bounds = item_bounds(&mesh);
    if stage.structure != Some(forge.structure)
        && let Some(bounds) = bounds
    {
        stage.size = Vec3::from(bounds.max - bounds.min);
        stage.centre = Vec3::from(bounds.center());
        stage.structure = Some(forge.structure);
    }
    let pivot = commands.spawn((Pivot, Transform::default(), Visibility::default())).id();
    let centre = stage.centre;
    spawner.spawn(&mesh, &forge.appearance()).insert((Transform::from_translation(-centre), ChildOf(pivot)));
}

/// Rebuilds the panel's sections when a family or seed gives the item new ones, and scrolls
/// back to the top.
fn build_sections(
    mut commands: Commands,
    forge: Res<Forge>,
    fonts: Res<UiFonts>,
    sections: Query<Entity, With<Sections>>,
    mut body: Query<&mut ScrollPosition, With<Body>>,
    mut built: Local<Option<u32>>,
) {
    if *built == Some(forge.structure) {
        return;
    }
    *built = Some(forge.structure);
    for mut position in &mut body {
        position.0 = Vec2::ZERO;
    }
    let Ok(root) = sections.single() else { return };
    commands.entity(root).despawn_children();
    let fields: Vec<Field> = forge.item.fields().into_iter().filter(|f| f.path != "cond.state").collect();
    commands.entity(root).with_children(|root| {
        let mut current = "";
        for field in &fields {
            if field.section != current {
                current = &field.section;
                section_heading(root, &fonts, &field.section);
            }
            field_row(root, &fonts, field);
        }
    });
}

fn section_heading(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, section: &str) {
    parent.spawn((
        Text::new(section_title(section)),
        UiFonts::style(&fonts.display, 20.0, INK),
        Node { margin: UiRect::top(px(14)), padding: UiRect::bottom(px(3)), border: UiRect::bottom(px(1)), width: percent(100), ..default() },
        BorderColor::all(GOLD),
    ));
}

const LABEL_WIDTH: f32 = 128.0;
const VALUE_WIDTH: f32 = 64.0;

fn field_row(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, field: &Field) {
    let row = Node { align_items: AlignItems::Center, column_gap: px(8), min_height: px(26), ..default() };
    parent.spawn(row).with_children(|row| {
        row.spawn((
            Text::new(field_label(&field.name)),
            UiFonts::style(&fonts.body, 17.0, MUTED),
            Node { width: px(LABEL_WIDTH), flex_shrink: 0.0, ..default() },
        ));
        match &field.kind {
            FieldKind::Slider { value, range } => {
                slider_bar(row, &field.path, *range, fraction_of(range, *value));
                row.spawn((
                    Readout::Value(field.path.clone(), range.step),
                    Text::new(format_value(*value, range.step)),
                    UiFonts::style(&fonts.mono, 15.0, BRIGHT),
                    TextLayout::justify(Justify::Right),
                    Node { width: px(VALUE_WIDTH), flex_shrink: 0.0, ..default() },
                ));
            }
            FieldKind::Choice { value, .. } => {
                row.spawn((
                    Control::Cycle(field.path.clone()),
                    Scrolls,
                    Node {
                        flex_grow: 1.0,
                        padding: UiRect::axes(px(9), px(2)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(4)),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(CELL),
                    BorderColor::all(LINE),
                ))
                .with_child((
                    Readout::Choice(field.path.clone()),
                    Text::new(option_label(value)),
                    UiFonts::style(&fonts.body_medium, 16.0, INK),
                ));
            }
            FieldKind::Toggle { value } => {
                row.spawn((
                    Control::Toggle(field.path.clone()),
                    Scrolls,
                    Node {
                        width: px(20),
                        height: px(20),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(4)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(CELL),
                    BorderColor::all(LINE),
                ))
                .with_child((
                    ToggleMark(field.path.clone()),
                    Node { width: px(10), height: px(10), border_radius: BorderRadius::all(px(2)), ..default() },
                    BackgroundColor(INK),
                    if *value { Visibility::Inherited } else { Visibility::Hidden },
                ));
            }
        }
    });
}

/// A slider in the ledger's colours, as the settings page draws them: a ruled bar filled in
/// muted tan with a cream knob, in a taller node that is easy to catch.
fn slider_bar(row: &mut ChildSpawnerCommands, path: &str, range: Slider, fraction: f32) {
    row.spawn((
        Control::Slider(path.to_string(), range),
        Scrolls,
        Node { flex_grow: 1.0, height: px(22), align_items: AlignItems::Center, ..default() },
    ))
    .with_children(|bar| {
        bar.spawn((
            Node { width: percent(100), height: px(6), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(3)), ..default() },
            BackgroundColor(CELL),
            BorderColor::all(LINE),
        ))
        .with_child((
            SliderGraphic { path: path.to_string(), range, knob: false },
            Node { width: percent(fraction * 100.0), height: percent(100), border_radius: BorderRadius::all(px(3)), ..default() },
            BackgroundColor(MUTED),
        ));
        bar.spawn((
            SliderGraphic { path: path.to_string(), range, knob: true },
            Node {
                position_type: PositionType::Absolute,
                left: percent(fraction * 100.0),
                width: px(14),
                height: px(14),
                margin: UiRect::left(px(-7)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(INK),
            BorderColor::all(BG),
        ));
    });
}

/// Keeps every readout, slider, toggle and button showing the current state.
fn refresh_panel(
    forge: Res<Forge>,
    mut readouts: Query<(&Readout, &mut Text)>,
    mut graphics: Query<(&SliderGraphic, &mut Node)>,
    mut marks: Query<(&ToggleMark, &mut Visibility)>,
    mut chips: Query<(&Chip, &mut BackgroundColor)>,
) {
    let fields = forge.item.fields();
    let find = |path: &str| fields.iter().find(|f| f.path == path).map(|f| &f.kind);
    for (readout, mut text) in &mut readouts {
        let line = match readout {
            Readout::Noun => forge.item.noun(),
            Readout::Seed => forge.seed.to_string(),
            Readout::Value(path, step) => match find(path) {
                Some(FieldKind::Slider { value, .. }) => format_value(*value, *step),
                _ => continue,
            },
            Readout::Choice(path) => match find(path) {
                Some(FieldKind::Choice { value, .. }) => option_label(value),
                _ => continue,
            },
        };
        if text.0 != line {
            text.0 = line;
        }
    }
    for (graphic, mut node) in &mut graphics {
        let Some(FieldKind::Slider { value, .. }) = find(&graphic.path) else { continue };
        let at = percent(fraction_of(&graphic.range, *value) * 100.0);
        if graphic.knob {
            node.left = at;
        } else {
            node.width = at;
        }
    }
    for (mark, mut visibility) in &mut marks {
        if let Some(FieldKind::Toggle { value }) = find(&mark.0) {
            *visibility = if *value { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
    let wear = forge.item.condition().state;
    for (chip, mut background) in &mut chips {
        let on = match chip {
            Chip::Family(family) => *family == forge.family,
            Chip::Wear(state) => *state == wear,
        };
        background.0 = if on { LINE } else { CELL };
    }
}

fn spin(time: Res<Time>, mut state: ResMut<Spin>, mut pivots: Query<&mut Transform, With<Pivot>>) {
    if !state.paused {
        state.angle = (state.angle + SPIN_SPEED * time.delta_secs()) % std::f32::consts::TAU;
    }
    for mut transform in &mut pivots {
        transform.rotation = Quat::from_rotation_y(state.angle);
    }
}

/// Orbits the camera round the pivot at the framing distance, shifted so the item sits in the
/// middle of the space the panel leaves.
fn frame_camera(stage: Res<Stage>, spin: Res<Spin>, mut cameras: Query<(&Camera, &Projection, &mut Transform), With<StageCamera>>) {
    for (camera, projection, mut transform) in &mut cameras {
        let Projection::Perspective(perspective) = projection else { continue };
        if stage.size == Vec3::ZERO {
            continue;
        }
        let width = camera.logical_viewport_size().map_or(1280.0, |s| s.x).max(1.0);
        let panel = (PANEL_WIDTH / width).clamp(0.0, 0.8);
        // Fit the item to the part of the view the panel leaves.
        let visible = PerspectiveProjection { aspect_ratio: perspective.aspect_ratio * (1.0 - panel), ..*perspective };
        let distance = framing_distance(stage.size, &visible, FILL);
        let mut target = Transform::from_translation(spin.orbit.offset(distance)).looking_at(Vec3::ZERO, Vec3::Y);
        // Slide the camera sideways so the item lands at the middle of the free part.
        let half_width = distance * spin.orbit.zoom * (perspective.fov * 0.5).tan() * perspective.aspect_ratio;
        target.translation += *target.right() * half_width * panel;
        if *transform != target {
            *transform = target;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_the_command_line() {
        let forge = Forge::from_args(args("--forge --family lantern --seed 7")).unwrap();
        assert_eq!((forge.family, forge.seed), (Family::Lantern, 7));
        assert_eq!(forge.item, Item::roll(Family::Lantern, 7).unwrap());
        let forge = Forge::from_args(args("--forge --family=sword")).unwrap();
        assert_eq!((forge.family, forge.seed), (Family::Sword, DEFAULT_SEED));
        assert!(Forge::from_args(args("--family nothing")).is_err());
        assert!(Forge::from_args(args("--seed")).is_err());
        assert!(Forge::from_args(args("--seed x")).is_err());
        assert!(Forge::from_args(args("--wat")).is_err());
    }

    #[test]
    fn families_cycle_back_round() {
        let start = Forge::default();
        let mut forge = start.clone();
        for _ in 0..Item::PORTED.len() {
            forge.step_family(1);
        }
        assert_eq!(forge.family, start.family);
        forge.step_family(-1);
        assert_eq!(forge.family, Family::Staff);
    }

    #[test]
    fn forge_new_changes_the_seed_and_the_item() {
        let mut forge = Forge::default();
        let before = forge.clone();
        forge.forge_new(1);
        assert_ne!(forge.seed, before.seed);
        assert_ne!(forge.item, before.item);
        assert_eq!(forge.item, Item::roll(forge.family, forge.seed).unwrap());
        assert_ne!(forge.structure, before.structure);
        assert_ne!(fresh_seed(1), fresh_seed(2));
    }

    #[test]
    fn edits_show_and_reset_goes_back_to_the_seeds_roll() {
        let mut forge = Forge::new(Family::Lantern, 7).unwrap();
        let rolled = forge.item.clone();
        forge.item.set_slider("flame.flame", 1.3).unwrap();
        forge.set_wear(ConditionState::GraveFind);
        assert_ne!(forge.item, rolled);
        assert_eq!(forge.appearance().condition.wear, 1.0);
        forge.reset();
        assert_eq!(forge.item, rolled);
        // Editing never touched what the seed rolls.
        assert_eq!(Item::roll(Family::Lantern, 7).unwrap(), rolled);
    }

    #[test]
    fn wear_presets_keep_the_pattern_scale() {
        let own = Condition { scale: 2.5, ..Condition::default() };
        let worn = wear_preset(own, ConditionState::BattleWorn);
        assert_eq!((worn.scale, worn.wear, worn.state), (2.5, 0.45, ConditionState::BattleWorn));
    }

    #[test]
    fn labels_read_as_words() {
        assert_eq!(field_label("gnarl_freq"), "gnarl freq");
        assert_eq!(section_title("cond"), "Condition");
        assert_eq!(section_title("shaft"), "Shaft");
        assert_eq!(section_title("head.gem_cut"), "Head / gem cut");
        assert_eq!(option_label("pierced_tin"), "pierced tin");
    }

    #[test]
    fn values_show_the_decimals_of_their_step() {
        assert_eq!(format_value(5.0, 1.0), "5");
        assert_eq!(format_value(0.5, 0.1), "0.5");
        assert_eq!(format_value(1.0, 0.01), "1.00");
        assert_eq!(format_value(0.105, 0.005), "0.105");
        assert_eq!(format_value(0.0125, 0.0005), "0.0125");
    }

    #[test]
    fn bars_map_to_ranges_and_back() {
        let range = Slider::new(0.6, 1.3, 0.01);
        assert_eq!(value_at(&range, 0.0), 0.6);
        assert_eq!(value_at(&range, 1.0), 1.3);
        assert_eq!(value_at(&range, 2.0), 1.3);
        assert!((value_at(&range, 0.5) - 0.95).abs() < 1e-6);
        assert!((fraction_of(&range, 0.95) - 0.5).abs() < 1e-6);
        assert_eq!(fraction_of(&range, 5.0), 1.0);
        assert_eq!(fraction_of(&Slider::new(1.0, 1.0, 1.0), 1.0), 0.0);
    }

    #[test]
    fn every_ported_family_forges_and_has_a_panel() {
        for &family in Item::PORTED {
            let forge = Forge::new(family, 7).unwrap();
            assert!(forge.item.fields().iter().any(|f| matches!(f.kind, FieldKind::Slider { .. })), "{family:?}");
        }
    }
}
