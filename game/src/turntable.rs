//! The Armory turntable: a grid of items from consecutive seeds of one family, each spinning
//! slowly over a name label, and a close-up of any one of them. Works for every family
//! [`Item`] can roll.
//!
//! Keys: the arrows move the selection (stepping off either end of the page turns it),
//! Page Up/Down turn the page, Tab and Shift+Tab switch family, W steps the condition from
//! pristine to grave find, Space pauses the spin, Enter or F closes in on the selected item
//! and back out, Escape leaves the close-up. Mouse: hovering selects, a click closes in; in
//! the close-up, dragging orbits and scrolling zooms.
//! Command line: `--family mace --seed 1207 --wear grave_find` ([`Turntable::from_args`]).

use armory::{
    Family, Item,
    cond::{Condition, ConditionState},
};
use bevy::{
    camera::Exposure,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    render::view::Tonemapping,
    shape::BoundingVolume,
    ui::IsDefaultUiCamera,
    window::PrimaryWindow,
};

use crate::light::StudioLight;
use crate::render::{ItemAppearance, ItemPalette, ItemSpawner, item_bounds};

pub const COLUMNS: usize = 4;
pub const ROWS: usize = 3;
/// Items per page.
pub const PAGE: u32 = (COLUMNS * ROWS) as u32;
pub const DEFAULT_SEED: u32 = 1207;

/// The condition presets W steps through, newest to oldest.
pub const WEAR_STEPS: [ConditionState; 5] = [
    ConditionState::Pristine,
    ConditionState::Serviceable,
    ConditionState::BattleWorn,
    ConditionState::Neglected,
    ConditionState::GraveFind,
];

/// Spin speed in radians per second.
const SPIN_SPEED: f32 = 0.45;
/// Cell size for a page whose items have no geometry yet: a mace-sized slot.
const FALLBACK_ITEM_SIZE: Vec3 = Vec3::new(3.0, 8.0, 3.0);
/// Share of the view height (or width) a close-up item fills.
const CLOSE_UP_FILL: f32 = 0.84;
const LABEL_WIDTH: f32 = 320.0;
/// The Armory's plate, ink and accent (`--plate`, `--plate-ink` and `--accent` in
/// `reference/spline-armory.html`), so items read against it as they do there.
const PLATE: Color = Color::srgb(0xdf as f32 / 255.0, 0xe3 as f32 / 255.0, 0xe0 as f32 / 255.0);
const PLATE_INK: Color = Color::srgb(0x1c as f32 / 255.0, 0x23 as f32 / 255.0, 0x26 as f32 / 255.0);
const ACCENT: Color = Color::srgb(0x2e as f32 / 255.0, 0x6b as f32 / 255.0, 0x62 as f32 / 255.0);

/// Shows the [`Turntable`] page: add with the resource set from [`Turntable::from_args`] or
/// left at its default, and optionally a [`Focus`] to open on a close-up.
pub struct TurntablePlugin;

impl Plugin for TurntablePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Turntable>()
            .init_resource::<Focus>()
            .init_resource::<Spin>()
            .init_resource::<PageLayout>()
            .insert_resource(ClearColor(PLATE))
            .insert_resource(StudioLight::ARMORY.ambient_light(&Exposure::INDOOR))
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    (keys, orbit, hover).chain(),
                    show_page.run_if(resource_changed::<Turntable>),
                    show_status,
                    spin,
                    show_focus,
                    frame_camera,
                    (place_labels, place_selection),
                )
                    .chain(),
            );
    }
}

/// What the turntable shows. Changing it rebuilds the page.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Turntable {
    pub family: Family,
    /// Seed of the top-left item; the page runs `first_seed..first_seed + PAGE`, wrapping.
    pub first_seed: u32,
    pub wear: ConditionState,
}

impl Default for Turntable {
    fn default() -> Self {
        Turntable { family: Family::Mace, first_seed: DEFAULT_SEED, wear: ConditionState::Pristine }
    }
}

impl Turntable {
    /// Parses `--family <key>`, `--seed <n>` and `--wear <state>` (also `--flag=value`).
    /// `args` excludes the program name.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<Turntable, String> {
        let mut turntable = Turntable::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (arg, None),
            };
            let mut value = || inline.clone().or_else(|| args.next()).ok_or(format!("{flag} needs a value"));
            match flag.as_str() {
                "--family" => turntable.family = parse_family(&value()?)?,
                "--seed" => {
                    let seed = value()?;
                    turntable.first_seed = seed.parse().map_err(|_| format!("--seed: {seed:?} is not a u32"))?;
                }
                "--wear" => turntable.wear = parse_wear(&value()?)?,
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        Ok(turntable)
    }

    /// The seeds on this page, in grid order.
    pub fn seeds(&self) -> impl Iterator<Item = u32> + use<> {
        let first = self.first_seed;
        (0..PAGE).map(move |i| first.wrapping_add(i))
    }

    /// The seed in grid slot `slot`.
    pub fn seed(&self, slot: usize) -> u32 {
        self.first_seed.wrapping_add(slot as u32)
    }

    /// Moves `pages` grids forward (negative: back).
    pub fn page(&mut self, pages: i32) {
        self.first_seed = self.first_seed.wrapping_add_signed(pages.wrapping_mul(PAGE as i32));
    }

    /// Steps `by` places through [`Item::PORTED`], wrapping.
    pub fn step_family(&mut self, by: isize) {
        let ported = Item::PORTED;
        let at = ported.iter().position(|&f| f == self.family).unwrap_or(0) as isize;
        self.family = ported[(at + by).rem_euclid(ported.len() as isize) as usize];
    }

    /// The next of [`WEAR_STEPS`], back to pristine after a grave find.
    pub fn step_wear(&mut self) {
        let at = WEAR_STEPS.iter().position(|&w| w == self.wear).map_or(0, |i| i + 1);
        self.wear = WEAR_STEPS[at % WEAR_STEPS.len()];
    }

    /// How `item` looks on the turntable: its own palette and wear pattern, at the chosen
    /// condition preset.
    pub fn appearance(&self, item: &Item) -> ItemAppearance {
        let own = item.condition();
        ItemAppearance {
            palette: ItemPalette { materials: item.palette(), ..ItemPalette::ARMORY },
            condition: Condition { scale: own.scale, broken: own.broken, ..Condition::of(self.wear) },
            seed: item.seed(),
            ..default()
        }
    }
}

/// The selected slot, and whether the view has closed in on it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Focus {
    /// Grid slot, row-major from the top left.
    pub slot: usize,
    pub close_up: bool,
    pub orbit: Orbit,
}

impl Focus {
    /// A close-up of grid slot `slot`, given as text (the `FB_CLOSEUP` variable).
    pub fn close_up_on(slot: &str) -> Result<Focus, String> {
        let index: usize = slot.trim().parse().map_err(|_| format!("{slot:?} is not a slot number"))?;
        if index >= PAGE as usize {
            return Err(format!("slot {index} is off the page; slots run 0 to {}", PAGE - 1));
        }
        Ok(Focus { slot: index, close_up: true, ..default() })
    }

    /// Closes in on the selected slot, or goes back to the grid. Each close-up starts square
    /// on.
    pub fn toggle_close_up(&mut self) {
        self.close_up = !self.close_up;
        self.orbit = Orbit::default();
    }

    /// Moves the selection `by` slots in grid order. Stepping off either end of the page
    /// turns `turntable` to the next or previous page.
    pub fn step(&mut self, by: i32, turntable: &mut Turntable) {
        let at = self.slot as i32 + by;
        let pages = at.div_euclid(PAGE as i32);
        if pages != 0 {
            turntable.page(pages);
        }
        self.slot = at.rem_euclid(PAGE as i32) as usize;
    }

    /// Moves the selection `rows` rows up (negative) or down, stopping at the top and bottom.
    pub fn step_row(&mut self, rows: i32) {
        let row = (self.slot / COLUMNS) as i32 + rows;
        self.slot = row.clamp(0, ROWS as i32 - 1) as usize * COLUMNS + self.slot % COLUMNS;
    }
}

/// Where the close-up camera sits around the item.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    /// Radians around the item, positive to the camera's right.
    pub yaw: f32,
    /// Radians above level.
    pub pitch: f32,
    /// Distance as a multiple of the framing distance; below 1 is closer.
    pub zoom: f32,
}

impl Default for Orbit {
    fn default() -> Self {
        Orbit { yaw: 0.0, pitch: 0.0, zoom: 1.0 }
    }
}

impl Orbit {
    pub const MAX_PITCH: f32 = 1.4;
    pub const MIN_ZOOM: f32 = 0.2;
    pub const MAX_ZOOM: f32 = 3.0;
    /// Radians turned per pixel dragged.
    const DRAG_SPEED: f32 = 0.008;

    /// Turns the camera around the item by a mouse drag of `pixels`; dragging down looks from
    /// above.
    pub fn drag(&mut self, pixels: Vec2) {
        self.yaw -= pixels.x * Self::DRAG_SPEED;
        self.pitch = (self.pitch + pixels.y * Self::DRAG_SPEED).clamp(-Self::MAX_PITCH, Self::MAX_PITCH);
    }

    /// Zooms in `steps` scroll lines (negative: out), 10% a line.
    pub fn zoom_by(&mut self, steps: f32) {
        self.zoom = (self.zoom * 0.9f32.powf(steps)).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
    }

    /// The camera's offset from the item for a framing distance of `distance`.
    pub fn offset(&self, distance: f32) -> Vec3 {
        Quat::from_euler(EulerRot::YXZ, self.yaw, -self.pitch, 0.0) * Vec3::Z * distance * self.zoom
    }
}

/// A family by its [`Family::key`]; it must be ported.
pub fn parse_family(key: &str) -> Result<Family, String> {
    let ported = || Item::PORTED.iter().map(|f| f.key()).collect::<Vec<_>>().join(", ");
    let family = Family::ALL
        .into_iter()
        .find(|f| f.key().eq_ignore_ascii_case(key))
        .ok_or_else(|| format!("unknown family {key:?}; ported: {}", ported()))?;
    if family.is_ported() { Ok(family) } else { Err(format!("{} is not ported yet; ported: {}", family.key(), ported())) }
}

/// A condition preset by name, ignoring case, `_`, `-` and spaces: `gravefind`, `grave_find`,
/// `Battle-worn`.
pub fn parse_wear(name: &str) -> Result<ConditionState, String> {
    let wanted: String = name.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    WEAR_STEPS
        .into_iter()
        .find(|&w| wear_name(w).chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase() == wanted)
        .ok_or_else(|| format!("unknown wear {name:?}; one of pristine, serviceable, battle_worn, neglected, grave_find"))
}

/// The display name of a condition preset.
pub fn wear_name(state: ConditionState) -> &'static str {
    match state {
        ConditionState::Custom => "Custom",
        ConditionState::Pristine => "Pristine",
        ConditionState::Serviceable => "Serviceable",
        ConditionState::BattleWorn => "Battle-worn",
        ConditionState::Neglected => "Neglected",
        ConditionState::GraveFind => "Grave find",
    }
}

/// How far back a perspective camera sits from an item of `size` spinning about its vertical
/// axis so it fills `fill` of the view.
pub fn framing_distance(size: Vec3, perspective: &PerspectiveProjection, fill: f32) -> f32 {
    let radius = size.x.hypot(size.z) * 0.5;
    let half_tan = (perspective.fov * 0.5).tan();
    let fit_height = size.y * 0.5 / (fill * half_tan);
    let fit_width = radius / (fill * half_tan * perspective.aspect_ratio);
    fit_height.max(fit_width) + radius
}

/// Spin angle shared by every slot, so rebuilding a page keeps the motion continuous.
#[derive(Resource, Default)]
struct Spin {
    angle: f32,
    paused: bool,
}

/// Grid cell size for the current page, in world units.
#[derive(Resource)]
struct PageLayout {
    cell: Vec2,
    /// Height of the tallest item, for placing labels under it.
    item_height: f32,
}

impl Default for PageLayout {
    fn default() -> Self {
        PageLayout::fit(FALLBACK_ITEM_SIZE)
    }
}

impl PageLayout {
    /// Cells that hold an item of `size` (the largest on the page) through a full turn, with
    /// room for its label.
    fn fit(size: Vec3) -> Self {
        let swept = size.x.hypot(size.z);
        let height = size.y * 1.3;
        // Tall, thin items still get cells wide enough for their labels.
        PageLayout { cell: Vec2::new((swept * 1.25).max(height * 0.85), height), item_height: size.y }
    }

    /// Centre of grid cell `index` (row-major from top left).
    fn cell_centre(&self, index: usize) -> Vec3 {
        let (column, row) = ((index % COLUMNS) as f32, (index / COLUMNS) as f32);
        let x = (column - (COLUMNS - 1) as f32 / 2.0) * self.cell.x;
        let y = ((ROWS - 1) as f32 / 2.0 - row) * self.cell.y;
        Vec3::new(x, y, 0.0)
    }

    /// Where the item in cell `index` turns: high in the cell, with the label below.
    fn slot(&self, index: usize) -> Vec3 {
        self.cell_centre(index) + Vec3::Y * self.cell.y * 0.06
    }

    fn grid(&self) -> Vec2 {
        self.cell * Vec2::new(COLUMNS as f32, ROWS as f32)
    }

    /// The cell under viewport point `cursor`, if any.
    fn cell_at(&self, cursor: Vec2, camera: &Camera, camera_transform: &GlobalTransform) -> Option<usize> {
        let half = (self.cell * 0.5).extend(0.0);
        (0..PAGE as usize).find(|&index| {
            let centre = self.cell_centre(index);
            let corners = [centre - half, centre + half].map(|c| camera.world_to_viewport(camera_transform, c));
            let [Ok(bottom_left), Ok(top_right)] = corners else { return false };
            Rect::from_corners(bottom_left, top_right).contains(cursor)
        })
    }
}

/// Everything [`show_page`] spawns, cleared when the page changes.
#[derive(Component)]
struct OnPage;

/// A spinning pivot holding one item, centred on the item's bounds.
#[derive(Component)]
struct Slot {
    index: usize,
    /// Size of the item's bounds.
    size: Vec3,
}

/// A name label under a slot.
#[derive(Component)]
struct Label {
    anchor: Vec3,
}

/// The frame drawn around the selected cell.
#[derive(Component)]
struct SelectionFrame;

#[derive(Component)]
struct StatusLine;

#[derive(Component)]
struct TurntableCamera;

fn setup(mut commands: Commands) {
    let exposure = Exposure::INDOOR;
    commands
        .spawn((
            TurntableCamera,
            Camera3d::default(),
            exposure,
            // The prototype draws untonemapped; a tone curve would flatten the bands.
            Tonemapping::Linear,
            // UI follows this camera even when frame capture retargets it to an image.
            IsDefaultUiCamera,
            Transform::from_xyz(0.0, 0.0, 60.0).looking_at(Vec3::ZERO, Vec3::Y),
        ))
        // The key rides on the camera, so it keeps its place as the view moves, as in the Armory.
        .with_child(StudioLight::ARMORY.key_light(&exposure));
    commands.spawn((
        StatusLine,
        Text::default(),
        TextFont::from_font_size(18.0),
        TextColor(PLATE_INK),
        Node { position_type: PositionType::Absolute, top: px(16.0), left: px(20.0), ..default() },
    ));
    commands.spawn((
        SelectionFrame,
        Node { position_type: PositionType::Absolute, border: UiRect::all(px(2.0)), ..default() },
        BorderColor::all(ACCENT),
        Visibility::Hidden,
    ));
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut turntable: ResMut<Turntable>, mut focus: ResMut<Focus>, mut spin: ResMut<Spin>) {
    let pressed = |key| keys.just_pressed(key);
    if pressed(KeyCode::ArrowRight) {
        focus.step(1, &mut turntable);
    }
    if pressed(KeyCode::ArrowLeft) {
        focus.step(-1, &mut turntable);
    }
    if pressed(KeyCode::ArrowDown) {
        focus.step_row(1);
    }
    if pressed(KeyCode::ArrowUp) {
        focus.step_row(-1);
    }
    if pressed(KeyCode::PageDown) {
        turntable.page(1);
    }
    if pressed(KeyCode::PageUp) {
        turntable.page(-1);
    }
    if pressed(KeyCode::Tab) {
        let back = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
        turntable.step_family(if back { -1 } else { 1 });
    }
    if pressed(KeyCode::KeyW) {
        turntable.step_wear();
    }
    if pressed(KeyCode::Space) {
        spin.paused = !spin.paused;
    }
    if pressed(KeyCode::Enter) || pressed(KeyCode::NumpadEnter) || pressed(KeyCode::KeyF) {
        focus.toggle_close_up();
    }
    if pressed(KeyCode::Escape) && focus.close_up {
        focus.toggle_close_up();
    }
}

/// In the close-up, dragging orbits and scrolling zooms.
fn orbit(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut focus: ResMut<Focus>,
) {
    if !focus.close_up {
        return;
    }
    if buttons.pressed(MouseButton::Left) && motion.delta != Vec2::ZERO {
        focus.orbit.drag(motion.delta);
    }
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    if lines != 0.0 {
        focus.orbit.zoom_by(lines);
    }
}

/// On the grid, hovering over a cell selects it and a click closes in on it.
fn hover(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<TurntableCamera>>,
    layout: Res<PageLayout>,
    mut focus: ResMut<Focus>,
    mut last_cursor: Local<Option<Vec2>>,
) {
    if focus.close_up {
        return;
    }
    let cursor = windows.single().ok().and_then(Window::cursor_position);
    let moved = cursor != *last_cursor;
    *last_cursor = cursor;
    let clicked = buttons.just_pressed(MouseButton::Left);
    let (Some(cursor), Ok((camera, camera_transform))) = (cursor, cameras.single()) else { return };
    if !(moved || clicked) {
        return;
    }
    let Some(cell) = layout.cell_at(cursor, camera, camera_transform) else { return };
    if focus.slot != cell {
        focus.slot = cell;
    }
    if clicked {
        focus.toggle_close_up();
    }
}

/// Rebuilds every slot and label for the current page.
fn show_page(
    mut commands: Commands,
    turntable: Res<Turntable>,
    mut layout: ResMut<PageLayout>,
    old: Query<Entity, With<OnPage>>,
    mut spawner: ItemSpawner,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let items: Vec<(u32, Option<Item>)> = turntable.seeds().map(|seed| (seed, Item::roll(turntable.family, seed))).collect();
    let meshes: Vec<_> = items.iter().map(|(_, item)| item.as_ref().map(Item::build).unwrap_or_default()).collect();
    let bounds: Vec<_> = meshes.iter().map(item_bounds).collect();
    let largest = bounds.iter().flatten().map(|b| Vec3::from(b.max - b.min)).reduce(Vec3::max);
    *layout = PageLayout::fit(largest.unwrap_or(FALLBACK_ITEM_SIZE));

    for (index, (((seed, item), mesh), bounds)) in items.iter().zip(&meshes).zip(&bounds).enumerate() {
        let centre = layout.slot(index);
        commands.spawn((
            OnPage,
            Label { anchor: centre - Vec3::Y * (layout.item_height * 0.5 + layout.cell.y * 0.04) },
            Text::new(format!("{}\n{seed}", item_name(turntable.family, item.as_ref()))),
            TextFont::from_font_size(17.0),
            TextColor(PLATE_INK),
            TextLayout::justify(Justify::Center),
            Node { position_type: PositionType::Absolute, width: px(LABEL_WIDTH), ..default() },
            // Hidden until `place_labels` has a camera position for it.
            Visibility::Hidden,
        ));
        let Some(item) = item else { continue };
        let size = bounds.map_or(FALLBACK_ITEM_SIZE, |b| Vec3::from(b.max - b.min));
        let slot =
            commands.spawn((OnPage, Slot { index, size }, Transform::from_translation(centre), Visibility::default())).id();
        let offset = bounds.map_or(Vec3::Y * -FALLBACK_ITEM_SIZE.y * 0.5, |b| -Vec3::from(b.center()));
        spawner.spawn(mesh, &turntable.appearance(item)).insert((Transform::from_translation(offset), ChildOf(slot)));
    }
}

fn item_name(family: Family, item: Option<&Item>) -> String {
    item.map_or_else(|| format!("{} (not ported)", family.key()), Item::noun)
}

fn show_status(turntable: Res<Turntable>, focus: Res<Focus>, mut status: Query<&mut Text, With<StatusLine>>) {
    if !(turntable.is_changed() || focus.is_changed()) {
        return;
    }
    let line = if focus.close_up {
        let seed = turntable.seed(focus.slot);
        format!(
            "{}   seed {seed}   {}\nLeft/Right item   Drag orbit   Scroll zoom   W wear   Space pause   Enter or Esc back",
            item_name(turntable.family, Item::roll(turntable.family, seed).as_ref()),
            wear_name(turntable.wear),
        )
    } else {
        format!(
            "{}   seeds {}-{}   {}\nArrows select   PgUp/PgDn page   Tab family   W wear   Space pause   Enter or F close-up",
            turntable.family.key(),
            turntable.first_seed,
            turntable.seed(PAGE as usize - 1),
            wear_name(turntable.wear),
        )
    };
    for mut text in &mut status {
        text.0.clone_from(&line);
    }
}

fn spin(time: Res<Time>, mut state: ResMut<Spin>, mut slots: Query<(&Slot, &mut Transform)>) {
    if !state.paused {
        state.angle = (state.angle + SPIN_SPEED * time.delta_secs()) % std::f32::consts::TAU;
    }
    for (slot, mut transform) in &mut slots {
        // Offset each slot so the grid shows items at different angles.
        transform.rotation = Quat::from_rotation_y(state.angle + slot.index as f32 * 0.7);
    }
}

/// In the close-up, hides every slot but the selected one.
fn show_focus(focus: Res<Focus>, mut slots: Query<(&Slot, &mut Visibility)>) {
    for (slot, mut visibility) in &mut slots {
        let shown = !focus.close_up || slot.index == focus.slot;
        visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
    }
}

/// Backs the camera off until the whole grid fits the view, or, in the close-up, orbits the
/// selected item close enough that it fills most of the view.
fn frame_camera(
    layout: Res<PageLayout>,
    focus: Res<Focus>,
    slots: Query<(&Slot, &Transform), Without<TurntableCamera>>,
    mut cameras: Query<(&Projection, &mut Transform), With<TurntableCamera>>,
) {
    let selected = slots.iter().find(|(slot, _)| focus.close_up && slot.index == focus.slot);
    for (projection, mut transform) in &mut cameras {
        let Projection::Perspective(perspective) = projection else { continue };
        let target = match selected {
            Some((slot, pivot)) => {
                let centre = pivot.translation;
                let distance = framing_distance(slot.size, perspective, CLOSE_UP_FILL);
                Transform::from_translation(centre + focus.orbit.offset(distance)).looking_at(centre, Vec3::Y)
            }
            None => {
                let half_tan = (perspective.fov * 0.5).tan();
                // Leave room at the top for the status line.
                let grid = layout.grid() * Vec2::new(1.04, 1.18);
                let distance = (grid.y * 0.5 / half_tan).max(grid.x * 0.5 / (half_tan * perspective.aspect_ratio));
                let lift = Vec3::Y * grid.y * 0.04;
                Transform::from_translation(Vec3::Z * distance + lift).looking_at(lift, Vec3::Y)
            }
        };
        if *transform != target {
            *transform = target;
        }
    }
}

/// Pins each label under its slot in screen space; the close-up has none.
fn place_labels(
    focus: Res<Focus>,
    cameras: Query<(&Camera, &GlobalTransform), With<TurntableCamera>>,
    mut labels: Query<(&Label, &mut Node, &mut Visibility)>,
) {
    let Ok((camera, camera_transform)) = cameras.single() else { return };
    for (label, mut node, mut visibility) in &mut labels {
        if focus.close_up {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        let Ok(at) = camera.world_to_viewport(camera_transform, label.anchor) else { continue };
        node.left = px(at.x - LABEL_WIDTH * 0.5);
        node.top = px(at.y);
        visibility.set_if_neq(Visibility::Inherited);
    }
}

/// Frames the selected cell on the grid.
fn place_selection(
    focus: Res<Focus>,
    layout: Res<PageLayout>,
    cameras: Query<(&Camera, &GlobalTransform), With<TurntableCamera>>,
    mut frames: Query<(&mut Node, &mut Visibility), With<SelectionFrame>>,
) {
    let Ok((camera, camera_transform)) = cameras.single() else { return };
    let centre = layout.cell_centre(focus.slot);
    let half = (layout.cell * 0.47).extend(0.0);
    let corners = [centre - half, centre + half].map(|c| camera.world_to_viewport(camera_transform, c));
    for (mut node, mut visibility) in &mut frames {
        let [Ok(bottom_left), Ok(top_right)] = corners else { continue };
        if focus.close_up {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        let rect = Rect::from_corners(bottom_left, top_right);
        node.left = px(rect.min.x);
        node.top = px(rect.min.y);
        node.width = px(rect.width());
        node.height = px(rect.height());
        visibility.set_if_neq(Visibility::Inherited);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn defaults_to_maces_from_1207() {
        let turntable = Turntable::from_args(Vec::new()).unwrap();
        assert_eq!(turntable, Turntable { family: Family::Mace, first_seed: 1207, wear: ConditionState::Pristine });
        assert_eq!(turntable.seeds().collect::<Vec<_>>(), (1207..1219).collect::<Vec<_>>());
    }

    #[test]
    fn parses_the_command_line() {
        let turntable = Turntable::from_args(args("--family mace --seed=42 --wear grave_find")).unwrap();
        assert_eq!(turntable, Turntable { family: Family::Mace, first_seed: 42, wear: ConditionState::GraveFind });
        assert!(Turntable::from_args(args("--family wand")).is_err());
        assert!(Turntable::from_args(args("--seed")).is_err());
        assert!(Turntable::from_args(args("--seed -3")).is_err());
        assert!(Turntable::from_args(args("--colour red")).is_err());
    }

    #[test]
    fn wear_names_round_trip() {
        for state in WEAR_STEPS {
            assert_eq!(parse_wear(wear_name(state)), Ok(state));
        }
        assert_eq!(parse_wear("GRAVEFIND"), Ok(ConditionState::GraveFind));
        assert_eq!(parse_wear("battle_worn"), Ok(ConditionState::BattleWorn));
    }

    #[test]
    fn paging_back_returns_the_same_items() {
        let start = Turntable::default();
        let mut turntable = start;
        turntable.page(1);
        assert_eq!(turntable.first_seed, 1207 + PAGE);
        turntable.page(-1);
        assert_eq!(turntable, start);
        let rolled = |t: &Turntable| t.seeds().map(|s| Item::roll(t.family, s)).collect::<Vec<_>>();
        assert_eq!(rolled(&turntable), rolled(&start));
    }

    #[test]
    fn paging_wraps_at_the_ends_of_the_seed_range() {
        let mut turntable = Turntable { first_seed: 3, ..default() };
        turntable.page(-1);
        assert_eq!(turntable.first_seed, 3u32.wrapping_sub(PAGE));
        turntable.page(1);
        assert_eq!(turntable.first_seed, 3);
    }

    #[test]
    fn switching_family_and_back_restores_the_view() {
        let start = Turntable { first_seed: 900, ..default() };
        let mut turntable = start;
        turntable.step_family(1);
        turntable.step_family(-1);
        assert_eq!(turntable, start);
        assert!(turntable.family.is_ported());
    }

    #[test]
    fn wear_cycles_through_every_preset() {
        let mut turntable = Turntable::default();
        let mut seen = Vec::new();
        for _ in 0..WEAR_STEPS.len() {
            seen.push(turntable.wear);
            turntable.step_wear();
        }
        assert_eq!(seen, WEAR_STEPS);
        assert_eq!(turntable.wear, ConditionState::Pristine);
    }

    #[test]
    fn appearance_keeps_the_items_own_look() {
        let item = Item::roll(Family::Mace, 1207).unwrap();
        let turntable = Turntable { wear: ConditionState::GraveFind, ..default() };
        let appearance = turntable.appearance(&item);
        assert_eq!(appearance.palette.materials, item.palette());
        assert_eq!(appearance.seed, 1207);
        assert_eq!(appearance.condition.state, ConditionState::GraveFind);
        assert_eq!(appearance.condition.scale, item.condition().scale);
    }

    #[test]
    fn selection_turns_the_page_off_either_end() {
        let mut turntable = Turntable::default();
        let mut focus = Focus { slot: PAGE as usize - 1, ..default() };
        focus.step(1, &mut turntable);
        assert_eq!((focus.slot, turntable.first_seed), (0, 1207 + PAGE));
        focus.step(-1, &mut turntable);
        assert_eq!((focus.slot, turntable.first_seed), (PAGE as usize - 1, 1207));
        focus.step(-5, &mut turntable);
        assert_eq!((focus.slot, turntable.first_seed), (6, 1207));
    }

    #[test]
    fn rows_stop_at_the_top_and_bottom() {
        let mut focus = Focus { slot: 1, ..default() };
        focus.step_row(-1);
        assert_eq!(focus.slot, 1);
        focus.step_row(1);
        assert_eq!(focus.slot, 1 + COLUMNS);
        focus.step_row(5);
        assert_eq!(focus.slot, 1 + COLUMNS * (ROWS - 1));
    }

    #[test]
    fn close_up_toggles_back_to_the_same_page_square_on() {
        let mut focus = Focus { slot: 4, ..default() };
        focus.toggle_close_up();
        focus.orbit.drag(Vec2::new(120.0, -40.0));
        focus.orbit.zoom_by(3.0);
        focus.toggle_close_up();
        assert_eq!(focus, Focus { slot: 4, ..default() });
        focus.toggle_close_up();
        assert_eq!(focus.orbit, Orbit::default());
    }

    #[test]
    fn close_up_slot_comes_from_the_environment() {
        assert_eq!(Focus::close_up_on("0"), Ok(Focus { slot: 0, close_up: true, ..default() }));
        assert_eq!(Focus::close_up_on(" 11 ").map(|f| f.slot), Ok(11));
        assert!(Focus::close_up_on("12").is_err());
        assert!(Focus::close_up_on("first").is_err());
    }

    #[test]
    fn orbit_stays_in_range() {
        let mut orbit = Orbit::default();
        assert!((orbit.offset(10.0) - Vec3::Z * 10.0).length() < 1e-5);
        orbit.drag(Vec2::new(0.0, 10_000.0));
        assert_eq!(orbit.pitch, Orbit::MAX_PITCH);
        assert!(orbit.offset(10.0).y > 9.0, "dragging down looks from above");
        orbit.zoom_by(100.0);
        assert_eq!(orbit.zoom, Orbit::MIN_ZOOM);
        orbit.zoom_by(-100.0);
        assert_eq!(orbit.zoom, Orbit::MAX_ZOOM);
    }

    #[test]
    fn close_up_fills_the_view() {
        let perspective = PerspectiveProjection { aspect_ratio: 16.0 / 9.0, ..default() };
        let size = Vec3::new(1.0, 8.0, 1.0);
        let distance = framing_distance(size, &perspective, 0.8);
        let radius = size.x.hypot(size.z) * 0.5;
        // At the near side of the swept cylinder the item's height is 80% of the view.
        let view_height = 2.0 * (distance - radius) * (perspective.fov * 0.5).tan();
        assert!((size.y / view_height - 0.8).abs() < 1e-4);
    }
}
