//! The Forge view (`--forge`): one Armory item turning on the turntable's plate, and beside it
//! a panel of every parameter of its state, like the Forge sliders of
//! `reference/spline-armory.html`. Pick a family, roll a seed with "Forge new", and drag a
//! slider, cycle a choice or flip a toggle: the mesh rebuilds live.
//!
//! The panel is generic over families. [`armory::Item::fields`] lists each section's sliders
//! (with the family's own ranges and steps), choices and toggles, so a family needs nothing
//! here; the sliders themselves are the shared [`crate::tweak_panel`]. Edits change only the displayed state; the seed still rolls the same item
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
    reflect::Field,
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
use crate::render::{ItemAppearance, ItemPalette, ItemSpawner, item_bounds};
use crate::turntable::{Orbit, PLATE, PLATE_INK, SPIN_SPEED, framing_distance, parse_family, wear_name};
use crate::tweak_panel::{PanelBody, Tweak, TweakPanelPlugin, TweakPanelSystems, chip, over_body, panel_body};
use crate::ui::{BG, BRIGHT, CELL, FAINT, INK, LINE, MUTED, UiFonts, node_rect};

/// Width of the panel on the right, in logical pixels.
pub const PANEL_WIDTH: f32 = 430.0;
/// Share of the stage's height the item fills.
const FILL: f32 = 0.8;
const DEFAULT_SEED: u32 = 1207;
/// The condition presets, newest to oldest.
const WEAR_PRESETS: [ConditionState; 5] =
    [ConditionState::Pristine, ConditionState::Serviceable, ConditionState::BattleWorn, ConditionState::Neglected, ConditionState::GraveFind];

pub struct ForgeViewPlugin;

impl Plugin for ForgeViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Forge>()
            .init_resource::<UiFonts>()
            .init_resource::<Spin>()
            .init_resource::<Stage>()
            .add_plugins(TweakPanelPlugin::<Forge>::default())
            .insert_resource(ClearColor(PLATE))
            .insert_resource(StudioLight::ARMORY.ambient_light(&Exposure::INDOOR))
            .add_systems(Startup, (setup_stage, setup_panel))
            .add_systems(
                Update,
                (
                    (keys, press_controls).before(TweakPanelSystems),
                    (orbit_and_zoom, show_item.run_if(resource_changed::<Forge>), refresh_header.run_if(resource_changed::<Forge>), spin, frame_camera).chain().after(TweakPanelSystems),
                ),
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

impl Tweak for Forge {
    /// Every field but the condition's preset, which the wear buttons set.
    fn fields(&self) -> Vec<Field> {
        self.item.fields().into_iter().filter(|f| f.path != "cond.state").collect()
    }

    fn set_slider(&mut self, path: &str, value: f32) {
        let _ = self.item.set_slider(path, value);
    }

    fn cycle(&mut self, path: &str, by: isize) {
        let _ = self.item.cycle_choice(path, by);
    }

    fn set_colour(&mut self, path: &str, rgb: [f32; 3]) {
        let _ = self.item.set_colour(path, rgb);
    }

    fn flip(&mut self, path: &str) {
        let on = self.item.toggle(path).unwrap_or(false);
        let _ = self.item.set_toggle(path, !on);
    }

    fn structure(&self) -> u32 {
        self.structure
    }

    fn slider(&self, path: &str) -> Option<f32> {
        self.item.slider(path)
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

/// What a click or drag on a control does.
#[derive(Component, Clone, Debug, PartialEq)]
enum Control {
    Family(Family),
    ForgeNew,
    Reset,
    SeedStep(i64),
    Wear(ConditionState),
}

/// Panel text that follows the forge.
#[derive(Component, Clone, Debug, PartialEq)]
enum Readout {
    Noun,
    Seed,
}

/// A family or condition button, lit when it is the current one.
#[derive(Component, Clone, Copy, PartialEq)]
enum Chip {
    Family(Family),
    Wear(ConditionState),
}

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
            panel_body(panel);
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

/// A click on a header button does what it says.
fn press_controls(buttons: Res<ButtonInput<MouseButton>>, window: Option<Single<&Window, With<PrimaryWindow>>>, controls: Query<(&Control, &ComputedNode, &UiGlobalTransform)>, mut forge: ResMut<Forge>) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(point) = window.and_then(|w| w.cursor_position()) else { return };
    let Some((control, ..)) = controls.iter().find(|(_, node, transform)| node_rect(node, transform).contains(point)) else { return };
    match control.clone() {
        Control::Family(family) => forge.set_family(family),
        Control::ForgeNew => forge.forge_new(clock()),
        Control::Reset => forge.reset(),
        Control::SeedStep(by) => {
            let seed = forge.seed.wrapping_add_signed(by as i32);
            forge.set_seed(seed);
        }
        Control::Wear(state) => forge.set_wear(state),
    }
}

/// Dragging the stage orbits and scrolling over it zooms (the panel scrolls itself).
fn orbit_and_zoom(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    body: Query<(&ComputedNode, &UiGlobalTransform), With<PanelBody>>,
    mut spin: ResMut<Spin>,
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
    if lines == 0.0 || window.cursor_position().is_none() || over_panel || over_body(&window, &body) {
        return;
    }
    spin.orbit.zoom_by(lines);
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

/// Keeps the header's name, seed and lit buttons showing the current state.
fn refresh_header(forge: Res<Forge>, mut readouts: Query<(&Readout, &mut Text)>, mut chips: Query<(&Chip, &mut BackgroundColor)>) {
    for (readout, mut text) in &mut readouts {
        let line = match readout {
            Readout::Noun => forge.item.noun(),
            Readout::Seed => forge.seed.to_string(),
        };
        if text.0 != line {
            text.0 = line;
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
    fn every_ported_family_forges_and_has_a_panel() {
        for &family in Item::PORTED {
            let forge = Forge::new(family, 7).unwrap();
            assert!(forge.fields().iter().any(|f| matches!(f.kind, armory::reflect::FieldKind::Slider { .. })), "{family:?}");
            assert!(forge.fields().iter().all(|f| f.path != "cond.state"), "the wear buttons set the preset");
        }
    }
}
