//! The slider panel the tweaking views share: the Forge's item parameters
//! ([`crate::forge_view`]) and the level generator's ([`crate::level_view`]). Anything that
//! lists its parameters as [`Field`]s and can set them ([`Tweak`]) gets a scrolling body of
//! sections, each a heading over rows of sliders, choices and toggles. Dragging a slider sets
//! it, clicking a choice cycles it (right click goes back), clicking a toggle flips it, and
//! scrolling over the body moves it. The panel follows the resource: rebuilt when its
//! [`Tweak::structure`] changes, its values refreshed whenever it changes.
//!
//! A view lays out its own header and calls [`panel_body`] where the sections go.

use std::marker::PhantomData;

use armory::reflect::{Field, FieldKind};
use armory::slider::Slider;
use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    window::PrimaryWindow,
};

use crate::menu::fraction_along;
use crate::ui::{BG, BRIGHT, CELL, GOLD, INK, LINE, MUTED, UiFonts, node_rect};

/// Something with parameters the panel can show and set.
pub trait Tweak: Resource + Component<Mutability = bevy::ecs::component::Mutable> {
    /// Every parameter, in the order the panel lists them, grouped by section.
    fn fields(&self) -> Vec<Field>;
    /// Sets the slider at `path` to `value` (already snapped to its step).
    fn set_slider(&mut self, path: &str, value: f32);
    /// Steps the choice at `path` by `by` options.
    fn cycle(&mut self, _path: &str, _by: isize) {}
    /// Flips the toggle at `path`.
    fn flip(&mut self, _path: &str) {}
    /// Sets the dye at `path` (a [`FieldKind::Colour`]) to `rgb`, one of [`dye_palette`].
    fn set_colour(&mut self, _path: &str, _rgb: [f32; 3]) {}
    /// Changes whenever the set of parameters does (a new family), so the panel is rebuilt.
    fn structure(&self) -> u32;
    /// The slider at `path`'s value now.
    fn slider(&self, path: &str) -> Option<f32> {
        self.fields().into_iter().find(|f| f.path == path).and_then(|f| match f.kind {
            FieldKind::Slider { value, .. } => Some(value),
            _ => None,
        })
    }
}

/// Adds the panel's systems for `T`.
pub struct TweakPanelPlugin<T: Tweak>(PhantomData<T>);

impl<T: Tweak> Default for TweakPanelPlugin<T> {
    fn default() -> Self {
        TweakPanelPlugin(PhantomData)
    }
}

/// The panel's systems, so a view can order its own around them.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TweakPanelSystems;

impl<T: Tweak> Plugin for TweakPanelPlugin<T> {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeldSlider>().add_systems(
            Update,
            (press_fields::<T>, drag_slider::<T>, scroll_panel, build_sections::<T>.run_if(resource_changed::<T>), refresh_fields::<T>.run_if(resource_changed::<T>))
                .chain()
                .in_set(TweakPanelSystems),
        );
    }
}

/// Pixels one scroll line moves the panel.
const SCROLL_LINE: f32 = 36.0;
const LABEL_WIDTH: f32 = 128.0;
const VALUE_WIDTH: f32 = 64.0;

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
pub fn option_label(option: &str) -> String {
    field_label(option)
}

/// The dyes a colour field offers as swatches: the faction livery's (a tincture's softer dye),
/// the garment dyes, and undyed wool, browns, greys and blacks, each once.
pub fn dye_palette() -> Vec<[f32; 3]> {
    let mut dyes: Vec<armory::material::Rgb> = armory::heraldry::Tincture::ALL.iter().map(|t| t.dye()).collect();
    dyes.extend(armory::fabric::Fabric::GARMENT_DYES);
    dyes.extend([0xe9e2cf, 0xb59b72, 0x8a5a2b, 0x6b4a2f, 0x3b2a1e, 0x8a8a86, 0x55524e, 0x1c1b1a].map(armory::material::Rgb::hex));
    let mut palette: Vec<[f32; 3]> = Vec::new();
    for dye in dyes {
        let rgb = [dye.0, dye.1, dye.2];
        if !palette.iter().any(|p| colour_distance(*p, rgb) < 0.01) {
            palette.push(rgb);
        }
    }
    palette
}

/// How far apart two colours are: the largest channel difference.
pub fn colour_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

fn srgb(rgb: [f32; 3]) -> Color {
    Color::srgb(rgb[0], rgb[1], rgb[2])
}

/// The slider being dragged, from the press that caught it until the button comes up.
#[derive(Resource, Default)]
struct HeldSlider(Option<(String, Slider)>);

/// The scrolling area of the panel.
#[derive(Component)]
pub struct PanelBody;

/// The node the sections are built under.
#[derive(Component)]
struct Sections;

/// What a click or drag on a field does.
#[derive(Component, Clone, Debug, PartialEq)]
enum FieldControl {
    /// A slider's bar, by field path.
    Slider(String, Slider),
    Cycle(String),
    Toggle(String),
    /// A dye swatch: set the colour field at the path to it.
    Swatch(String, [f32; 3]),
}

/// A swatch's border, lit when its dye is the field's now; or (no swatch) the field's own colour.
#[derive(Component)]
struct SwatchMark {
    path: String,
    rgb: Option<[f32; 3]>,
}

/// A field's value as text.
#[derive(Component, Clone, Debug, PartialEq)]
enum FieldReadout {
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

/// The ledger button: a cell-dark box ruled in tan, with `marker` (a view's own control) on it.
pub fn chip<'a>(parent: &'a mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, marker: impl Bundle) -> EntityCommands<'a> {
    let mut chip = parent.spawn((
        marker,
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

/// The panel's scrolling body, where the sections are built: spawned as a child of `parent`,
/// filling what the view's header leaves.
pub fn panel_body(parent: &mut ChildSpawnerCommands) {
    parent
        .spawn((
            PanelBody,
            ScrollPosition::default(),
            Node { flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), padding: UiRect::axes(px(14), px(6)), ..default() },
        ))
        .with_child((Sections, Node { flex_direction: FlexDirection::Column, row_gap: px(2), padding: UiRect::bottom(px(30)), ..default() }));
}

/// Whether the mouse is over the panel's body.
pub fn over_body(window: &Window, body: &Query<(&ComputedNode, &UiGlobalTransform), With<PanelBody>>) -> bool {
    let Some(point) = window.cursor_position() else { return false };
    body.iter().any(|(node, transform)| node_rect(node, transform).contains(point))
}

/// A press on a field does what it says; a slider starts being dragged ([`drag_slider`]).
fn press_fields<T: Tweak>(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    controls: Query<(&FieldControl, &ComputedNode, &UiGlobalTransform)>,
    body: Query<(&ComputedNode, &UiGlobalTransform), With<PanelBody>>,
    mut tweak: ResMut<T>,
    mut held: ResMut<HeldSlider>,
    ui_scale: Option<Res<UiScale>>,
) {
    if !buttons.pressed(MouseButton::Left) {
        held.0 = None;
    }
    let left = buttons.just_pressed(MouseButton::Left);
    let right = buttons.just_pressed(MouseButton::Right);
    if !(left || right) {
        return;
    }
    let Some(point) = window.and_then(|w| crate::ui::ui_cursor(&w, ui_scale.as_deref())) else { return };
    let Ok((node, transform)) = body.single() else { return };
    if !node_rect(node, transform).contains(point) {
        return;
    }
    let Some((control, ..)) = controls.iter().find(|(_, node, transform)| node_rect(node, transform).contains(point)) else { return };
    match control.clone() {
        FieldControl::Slider(path, range) if left => held.0 = Some((path, range)),
        FieldControl::Cycle(path) => tweak.cycle(&path, if left { 1 } else { -1 }),
        FieldControl::Toggle(path) if left => tweak.flip(&path),
        FieldControl::Swatch(path, rgb) if left => tweak.set_colour(&path, rgb),
        _ => {}
    }
}

/// While the button is down, the slider pressed on follows the mouse along its bar.
fn drag_slider<T: Tweak>(
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    held: Res<HeldSlider>,
    controls: Query<(&FieldControl, &ComputedNode, &UiGlobalTransform)>,
    mut tweak: ResMut<T>,
    ui_scale: Option<Res<UiScale>>,
) {
    let Some((path, range)) = held.0.clone() else { return };
    let Some(point) = window.and_then(|w| crate::ui::ui_cursor(&w, ui_scale.as_deref())) else { return };
    let Some((_, node, transform)) = controls.iter().find(|(c, ..)| matches!(c, FieldControl::Slider(p, _) if *p == path)) else { return };
    let target = value_at(&range, fraction_along(node_rect(node, transform), point.x));
    let snapped = if target >= range.max {
        range.max
    } else if target <= range.min {
        range.min
    } else {
        range.snap(target)
    };
    let current = tweak.bypass_change_detection().slider(&path);
    if current.is_none_or(|c| (c - snapped).abs() > f32::EPSILON) {
        tweak.set_slider(&path, snapped);
    }
}

/// Scrolling over the body moves it.
fn scroll_panel(scroll: Res<AccumulatedMouseScroll>, window: Option<Single<&Window, With<PrimaryWindow>>>, mut body: Query<(&mut ScrollPosition, &ComputedNode, &UiGlobalTransform), With<PanelBody>>, ui_scale: Option<Res<UiScale>>) {
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    let Some(point) = window.and_then(|w| crate::ui::ui_cursor(&w, ui_scale.as_deref())) else { return };
    if lines == 0.0 {
        return;
    }
    for (mut position, node, transform) in &mut body {
        if !node_rect(node, transform).contains(point) {
            continue;
        }
        let max = ((node.content_size() - node.size()) * node.inverse_scale_factor).y.max(0.0);
        position.y = (position.y - lines * SCROLL_LINE).clamp(0.0, max);
    }
}

/// Rebuilds the sections when the parameters change shape, and scrolls back to the top.
fn build_sections<T: Tweak>(
    mut commands: Commands,
    tweak: Res<T>,
    fonts: Res<UiFonts>,
    sections: Query<Entity, With<Sections>>,
    mut body: Query<&mut ScrollPosition, With<PanelBody>>,
    mut built: Local<Option<u32>>,
) {
    let Ok(root) = sections.single() else { return };
    if *built == Some(tweak.structure()) {
        return;
    }
    *built = Some(tweak.structure());
    for mut position in &mut body {
        position.0 = Vec2::ZERO;
    }
    commands.entity(root).despawn_children();
    let fields = tweak.fields();
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

fn field_row(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, field: &Field) {
    let row = Node { align_items: AlignItems::Center, column_gap: px(8), min_height: px(26), ..default() };
    parent.spawn(row).with_children(|row| {
        row.spawn((Text::new(field_label(&field.name)), UiFonts::style(&fonts.body, 17.0, MUTED), Node { width: px(LABEL_WIDTH), flex_shrink: 0.0, ..default() }));
        match &field.kind {
            FieldKind::Slider { value, range } => {
                slider_bar(row, &field.path, *range, fraction_of(range, *value));
                row.spawn((
                    FieldReadout::Value(field.path.clone(), range.step),
                    Text::new(format_value(*value, range.step)),
                    UiFonts::style(&fonts.mono, 15.0, BRIGHT),
                    TextLayout::justify(Justify::Right),
                    Node { width: px(VALUE_WIDTH), flex_shrink: 0.0, ..default() },
                ));
            }
            FieldKind::Choice { value, .. } => {
                row.spawn((
                    FieldControl::Cycle(field.path.clone()),
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
                .with_child((FieldReadout::Choice(field.path.clone()), Text::new(option_label(value)), UiFonts::style(&fonts.body_medium, 16.0, INK)));
            }
            FieldKind::Colour { value } => {
                row.spawn(Node { flex_grow: 1.0, flex_wrap: FlexWrap::Wrap, column_gap: px(3), row_gap: px(3), align_items: AlignItems::Center, padding: UiRect::vertical(px(3)), ..default() })
                    .with_children(|swatches| {
                        swatches.spawn((
                            SwatchMark { path: field.path.clone(), rgb: None },
                            Node { width: px(26), height: px(26), margin: UiRect::right(px(5)), border: UiRect::all(px(2)), border_radius: BorderRadius::all(px(4)), ..default() },
                            BackgroundColor(srgb(*value)),
                            BorderColor::all(INK),
                        ));
                        for rgb in dye_palette() {
                            swatches.spawn((
                                FieldControl::Swatch(field.path.clone(), rgb),
                                SwatchMark { path: field.path.clone(), rgb: Some(rgb) },
                                Node { width: px(18), height: px(18), border: UiRect::all(px(2)), border_radius: BorderRadius::all(px(3)), ..default() },
                                BackgroundColor(srgb(rgb)),
                                BorderColor::all(LINE),
                            ));
                        }
                    });
            }
            FieldKind::Toggle { value } => {
                row.spawn((
                    FieldControl::Toggle(field.path.clone()),
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
    row.spawn((FieldControl::Slider(path.to_string(), range), Node { flex_grow: 1.0, height: px(22), align_items: AlignItems::Center, ..default() }))
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

/// Keeps every value, slider and toggle showing the current state.
fn refresh_fields<T: Tweak>(
    tweak: Res<T>,
    mut readouts: Query<(&FieldReadout, &mut Text)>,
    mut graphics: Query<(&SliderGraphic, &mut Node)>,
    mut marks: Query<(&ToggleMark, &mut Visibility)>,
    mut swatches: Query<(&SwatchMark, &mut BackgroundColor, &mut BorderColor)>,
) {
    let fields = tweak.fields();
    let find = |path: &str| fields.iter().find(|f| f.path == path).map(|f| &f.kind);
    for (readout, mut text) in &mut readouts {
        let line = match readout {
            FieldReadout::Value(path, step) => match find(path) {
                Some(FieldKind::Slider { value, .. }) => format_value(*value, *step),
                _ => continue,
            },
            FieldReadout::Choice(path) => match find(path) {
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
    for (swatch, mut background, mut border) in &mut swatches {
        let Some(FieldKind::Colour { value }) = find(&swatch.path) else { continue };
        match swatch.rgb {
            // The field's own colour.
            None => background.0 = srgb(*value),
            Some(rgb) => *border = BorderColor::all(if colour_distance(rgb, *value) < 0.01 { INK } else { LINE }),
        }
    }
    for (mark, mut visibility) in &mut marks {
        if let Some(FieldKind::Toggle { value }) = find(&mark.0) {
            *visibility = if *value { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_dye_palette_has_the_livery_dyes_once() {
        let palette = dye_palette();
        assert!(palette.len() >= 18, "{}", palette.len());
        let gules = armory::heraldry::Tincture::Gules.dye();
        assert_eq!(palette.iter().filter(|p| colour_distance(**p, [gules.0, gules.1, gules.2]) < 0.01).count(), 1);
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
}
