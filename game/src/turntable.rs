//! The Armory turntable: a grid of items from consecutive seeds of one family, each spinning
//! slowly over a name label, and a close-up of any one of them. Works for every family
//! [`Item`] can roll. On the grid, flat items tip toward the camera and small ones are
//! magnified ([`GridPose`]); labels give each item's true size.
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
    camera::{Exposure, ScalingMode},
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
/// Grid margins in logical pixels: the status line takes the top, and every item keeps a
/// share of its size clear around it, so neighbours never touch as they turn.
const STATUS_HEIGHT: f32 = 76.0;
const EDGE_MARGIN: f32 = 16.0;
const ITEM_MARGIN: f32 = 0.12;
/// The band under each item for its two-line label, the narrowest cell, so neighbouring
/// labels never run together, and the width a label may take before it wraps. Logical pixels.
const LABEL_HEIGHT: f32 = 48.0;
const MIN_CELL_WIDTH: f32 = 240.0;
const LABEL_WIDTH: f32 = 320.0;
/// How far from the page the grid camera sits; orthographic, so only the clip range cares.
const GRID_CAMERA_DISTANCE: f32 = 100.0;
/// On the grid, an item shorter than this share of the width it sweeps lies flat (keys,
/// lockpicks, a coiled rope), so it is tipped this far toward the camera to show its face
/// rather than its edge. The close-up shows it level.
const FLAT_SHARE: f32 = 0.3;
const FLAT_TILT: f32 = 0.6;
/// On the grid, an item filling less than this share of a cell sized for the page's largest
/// item is magnified to this share, and its label says so.
const MIN_FILL: f32 = 0.5;
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
                    fit_page,
                    show_status,
                    (place_slots, spin),
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
            etching: item.etching(),
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

/// How an item stands in its grid cell: tipped toward the camera if it lies flat, and
/// magnified if it would be lost in a cell sized for the largest item on the page. The
/// close-up ignores both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridPose {
    /// Radians the top tips toward the camera.
    pub tilt: f32,
    pub scale: f32,
}

impl GridPose {
    /// The pose for an item of bounds `size` on a page whose largest grid box is `largest`.
    pub fn of(size: Vec3, largest: Vec2) -> Self {
        let tilt = grid_tilt(size);
        let own = grid_box(size, tilt);
        let fit = (largest / own.max(Vec2::splat(f32::EPSILON))).min_element();
        // Within a few percent of true size is left at true size, so the label's note means
        // something.
        let scale = fit * MIN_FILL;
        GridPose { tilt, scale: if scale > 1.05 { scale } else { 1.0 } }
    }

    /// Whether the grid shows the item larger than life.
    pub fn magnified(&self) -> bool {
        self.scale > 1.0
    }

    /// The pivot's rotation on the grid at spin angle `angle`.
    pub fn rotation(&self, angle: f32) -> Quat {
        Quat::from_rotation_x(self.tilt) * Quat::from_rotation_y(angle)
    }
}

/// How far an item of bounds `size` tips toward the grid camera: [`FLAT_TILT`] if it lies
/// flat, otherwise none.
pub fn grid_tilt(size: Vec3) -> f32 {
    if size.y < FLAT_SHARE * size.x.hypot(size.z) { FLAT_TILT } else { 0.0 }
}

/// The width and height an item of bounds `size` takes on screen as it spins about its
/// vertical axis tipped `tilt` toward an orthographic camera: the swept cylinder's
/// silhouette.
pub fn grid_box(size: Vec3, tilt: f32) -> Vec2 {
    let swept = size.x.hypot(size.z);
    Vec2::new(swept, size.y * tilt.cos() + swept * tilt.sin())
}

/// An item's true size for its label: its longest side, in centimetres up to a metre and in
/// metres beyond (1 world unit = 10 cm).
pub fn true_size(size: Vec3) -> String {
    let cm = size.max_element() * 10.0;
    if cm < 100.0 { format!("{cm:.0} cm") } else { format!("{:.1} m", cm / 100.0) }
}

/// Spin angle shared by every slot, so rebuilding a page keeps the motion continuous.
#[derive(Resource, Default)]
struct Spin {
    angle: f32,
    paused: bool,
}

/// Where everything on the grid goes for the current page and viewport, in world units.
///
/// The grid camera is orthographic, as the Armory's is, so a logical pixel is the same size in
/// world units everywhere on the page: items fill their cells exactly through a full turn,
/// and labels get a band of their own in pixels under them.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
struct PageLayout {
    /// The largest [`grid_box`] on the page, axis by axis, before magnification.
    item: Vec2,
    /// The viewport, in logical pixels.
    viewport: Vec2,
    /// World units per logical pixel.
    pixel: f32,
    cell: Vec2,
}

impl Default for PageLayout {
    fn default() -> Self {
        PageLayout::fit(grid_box(FALLBACK_ITEM_SIZE, 0.0), Vec2::new(1920.0, 1080.0))
    }
}

impl PageLayout {
    /// The layout that fits a page whose largest item takes `item` on screen (its
    /// [`grid_box`]) into `viewport` (logical pixels): each cell holds that box plus a
    /// margin, above a label band.
    fn fit(item: Vec2, viewport: Vec2) -> Self {
        let grid = Vec2::new(COLUMNS as f32, ROWS as f32);
        let item_box = item * (1.0 + ITEM_MARGIN);
        // Pixels left for the items once margins and labels are taken.
        let room = viewport - Vec2::new(2.0 * EDGE_MARGIN, STATUS_HEIGHT + EDGE_MARGIN + ROWS as f32 * LABEL_HEIGHT);
        let pixel = (item_box * grid / room.max(Vec2::ONE)).max_element().max(f32::EPSILON);
        // A narrow window gives up label room before the grid runs off the sides.
        let min_cell_width = MIN_CELL_WIDTH.min(room.x / grid.x);
        let cell = Vec2::new(item_box.x.max(min_cell_width * pixel), item_box.y + LABEL_HEIGHT * pixel);
        PageLayout { item, viewport, pixel, cell }
    }

    /// Centre of grid cell `index` (row-major from top left).
    fn cell_centre(&self, index: usize) -> Vec3 {
        let (column, row) = ((index % COLUMNS) as f32, (index / COLUMNS) as f32);
        let x = (column - (COLUMNS - 1) as f32 / 2.0) * self.cell.x;
        let y = ((ROWS - 1) as f32 / 2.0 - row) * self.cell.y;
        Vec3::new(x, y, 0.0)
    }

    /// Where the item in cell `index` turns: centred in the cell above its label band.
    fn slot(&self, index: usize) -> Vec3 {
        self.cell_centre(index) + Vec3::Y * LABEL_HEIGHT * self.pixel * 0.5
    }

    /// Top centre of the label band in cell `index`.
    fn label(&self, index: usize) -> Vec3 {
        self.cell_centre(index) - Vec3::Y * (self.cell.y * 0.5 - LABEL_HEIGHT * self.pixel)
    }

    /// The grid camera's view: its height in world units and the point it looks at, set so
    /// the grid sits centred below the status line.
    fn view(&self) -> (f32, Vec3) {
        let lift = (STATUS_HEIGHT - EDGE_MARGIN) * 0.5 * self.pixel;
        (self.viewport.y * self.pixel, Vec3::Y * lift)
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
    /// How it stands on the grid.
    pose: GridPose,
}

/// The name label under grid slot `index`.
#[derive(Component)]
struct Label {
    index: usize,
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
    let sizes: Vec<Vec3> = bounds.iter().map(|b| b.map_or(FALLBACK_ITEM_SIZE, |b| Vec3::from(b.max - b.min))).collect();
    let largest = page_box(&sizes);
    *layout = PageLayout::fit(largest, layout.viewport);

    for (index, (((seed, item), mesh), bounds)) in items.iter().zip(&meshes).zip(&bounds).enumerate() {
        let centre = layout.slot(index);
        let size = sizes[index];
        let pose = GridPose::of(size, largest);
        let mut caption = format!("{}\n{seed}", item_name(turntable.family, item.as_ref()));
        if item.is_some() {
            caption += &format!("   {}", true_size(size));
            if pose.magnified() {
                caption += &format!("   shown x{:.1}", pose.scale);
            }
        }
        commands.spawn((
            OnPage,
            Label { index },
            Text::new(caption),
            TextFont::from_font_size(17.0),
            TextColor(PLATE_INK),
            TextLayout::justify(Justify::Center),
            Node { position_type: PositionType::Absolute, width: px(LABEL_WIDTH), ..default() },
            // Hidden until `place_labels` has a camera position for it.
            Visibility::Hidden,
        ));
        let Some(item) = item else { continue };
        let slot = commands
            .spawn((OnPage, Slot { index, size, pose }, Transform::from_translation(centre), Visibility::default()))
            .id();
        let offset = bounds.map_or(Vec3::Y * -FALLBACK_ITEM_SIZE.y * 0.5, |b| -Vec3::from(b.center()));
        spawner.spawn(mesh, &turntable.appearance(item)).insert((Transform::from_translation(offset), ChildOf(slot)));
    }
}

/// Refits the grid when the viewport changes size.
fn fit_page(cameras: Query<&Camera, With<TurntableCamera>>, mut layout: ResMut<PageLayout>) {
    let Some(viewport) = cameras.single().ok().and_then(Camera::logical_viewport_size) else { return };
    if viewport != layout.viewport {
        *layout = PageLayout::fit(layout.item, viewport);
    }
}

/// Keeps every slot in its cell as the layout changes.
fn place_slots(layout: Res<PageLayout>, mut slots: Query<(&Slot, &mut Transform)>) {
    if !layout.is_changed() {
        return;
    }
    for (slot, mut transform) in &mut slots {
        transform.translation = layout.slot(slot.index);
    }
}

/// The largest [`grid_box`] among items of bounds `sizes`, axis by axis, each tipped as
/// [`grid_tilt`] says.
fn page_box(sizes: &[Vec3]) -> Vec2 {
    sizes.iter().map(|&s| grid_box(s, grid_tilt(s))).reduce(Vec2::max).unwrap_or(grid_box(FALLBACK_ITEM_SIZE, 0.0))
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

/// Turns every slot; on the grid each also takes its [`GridPose`], in the close-up it stands
/// level at true size.
fn spin(time: Res<Time>, focus: Res<Focus>, mut state: ResMut<Spin>, mut slots: Query<(&Slot, &mut Transform)>) {
    if !state.paused {
        state.angle = (state.angle + SPIN_SPEED * time.delta_secs()) % std::f32::consts::TAU;
    }
    for (slot, mut transform) in &mut slots {
        // Offset each slot so the grid shows items at different angles.
        let angle = state.angle + slot.index as f32 * 0.7;
        let pose = if focus.close_up { GridPose { tilt: 0.0, scale: 1.0 } } else { slot.pose };
        transform.rotation = pose.rotation(angle);
        transform.scale = Vec3::splat(pose.scale);
    }
}

/// In the close-up, hides every slot but the selected one.
fn show_focus(focus: Res<Focus>, mut slots: Query<(&Slot, &mut Visibility)>) {
    for (slot, mut visibility) in &mut slots {
        let shown = !focus.close_up || slot.index == focus.slot;
        visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
    }
}

/// Shows the whole grid square on through an orthographic camera, as the Armory does, or, in
/// the close-up, orbits the selected item in perspective close enough that it fills most of
/// the view.
fn frame_camera(
    layout: Res<PageLayout>,
    focus: Res<Focus>,
    slots: Query<(&Slot, &Transform), Without<TurntableCamera>>,
    mut cameras: Query<(&mut Projection, &mut Transform), With<TurntableCamera>>,
) {
    let selected = slots.iter().find(|(slot, _)| focus.close_up && slot.index == focus.slot);
    for (mut projection, mut transform) in &mut cameras {
        let target = match selected {
            Some((slot, pivot)) => {
                if !matches!(*projection, Projection::Perspective(_)) {
                    *projection = Projection::Perspective(default());
                }
                let Projection::Perspective(perspective) = &*projection else { continue };
                let centre = pivot.translation;
                let distance = framing_distance(slot.size, perspective, CLOSE_UP_FILL);
                Transform::from_translation(centre + focus.orbit.offset(distance)).looking_at(centre, Vec3::Y)
            }
            None => {
                let (height, centre) = layout.view();
                let current = match &*projection {
                    Projection::Orthographic(OrthographicProjection {
                        scaling_mode: ScalingMode::FixedVertical { viewport_height },
                        ..
                    }) => Some(*viewport_height),
                    _ => None,
                };
                if current != Some(height) {
                    *projection = Projection::Orthographic(OrthographicProjection {
                        scaling_mode: ScalingMode::FixedVertical { viewport_height: height },
                        ..OrthographicProjection::default_3d()
                    });
                }
                Transform::from_translation(centre + Vec3::Z * GRID_CAMERA_DISTANCE).looking_at(centre, Vec3::Y)
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
    layout: Res<PageLayout>,
    cameras: Query<(&Camera, &GlobalTransform), With<TurntableCamera>>,
    mut labels: Query<(&Label, &mut Node, &mut Visibility)>,
) {
    let Ok((camera, camera_transform)) = cameras.single() else { return };
    for (label, mut node, mut visibility) in &mut labels {
        if focus.close_up {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        let Ok(at) = camera.world_to_viewport(camera_transform, layout.label(label.index)) else { continue };
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
    // The whole cell, label and all, inset a few pixels so neighbouring frames don't touch.
    let half = (layout.cell * 0.5 - 3.0 * layout.pixel).extend(0.0);
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
    fn grid_keeps_items_clear_of_their_labels() {
        for (item, viewport) in [
            (Vec3::new(0.3, 0.4, 0.2), Vec2::new(1920.0, 1080.0)),
            (Vec3::new(1.2, 17.0, 0.4), Vec2::new(1920.0, 1080.0)),
            (Vec3::new(6.0, 2.0, 4.0), Vec2::new(1280.0, 720.0)),
        ] {
            let layout = PageLayout::fit(grid_box(item, 0.0), viewport);
            let swept = item.x.hypot(item.z);
            assert!(swept <= layout.cell.x, "{item}: the turning item is wider than its cell");
            for index in 0..PAGE as usize {
                let centre = layout.cell_centre(index);
                let (slot, label) = (layout.slot(index), layout.label(index));
                assert!(slot.y + item.y * 0.5 <= centre.y + layout.cell.y * 0.5 + 1e-4, "{item}: top leaves the cell");
                assert!(slot.y - item.y * 0.5 >= label.y - 1e-4, "{item}: item runs into its label");
                let label_bottom = label.y - LABEL_HEIGHT * layout.pixel;
                assert!(label_bottom >= centre.y - layout.cell.y * 0.5 - 1e-4, "{item}: label leaves the cell");
            }
        }
    }

    #[test]
    fn grid_fits_under_the_status_line() {
        for viewport in [Vec2::new(1920.0, 1080.0), Vec2::new(800.0, 1000.0)] {
            let layout = PageLayout::fit(grid_box(Vec3::new(2.0, 8.0, 2.0), 0.0), viewport);
            let (height, centre) = layout.view();
            let half = Vec2::new(height * viewport.x / viewport.y, height) * 0.5;
            let grid = layout.cell * Vec2::new(COLUMNS as f32, ROWS as f32) * 0.5;
            let eps = 1e-3;
            assert!(grid.y <= centre.y + half.y - STATUS_HEIGHT * layout.pixel + eps, "{viewport}: grid under the status line");
            assert!(-grid.y >= centre.y - half.y + EDGE_MARGIN * layout.pixel - eps, "{viewport}: grid off the bottom");
            assert!(grid.x <= half.x - EDGE_MARGIN * layout.pixel + eps, "{viewport}: grid off the sides");
        }
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

    /// Bounds sizes of the items on `turntable`'s page.
    fn page_sizes(turntable: &Turntable) -> Vec<Vec3> {
        turntable
            .seeds()
            .map(|seed| {
                let mesh = Item::roll(turntable.family, seed).unwrap().build();
                let b = item_bounds(&mesh).unwrap();
                Vec3::from(b.max - b.min)
            })
            .collect()
    }

    #[test]
    fn small_sundries_fill_a_share_of_their_cells() {
        let turntable = Turntable { family: Family::Sundry, ..default() };
        let sizes = page_sizes(&turntable);
        let largest = page_box(&sizes);
        let layout = PageLayout::fit(largest, Vec2::new(1920.0, 1080.0));
        let room = Vec2::new(layout.cell.x, layout.cell.y - LABEL_HEIGHT * layout.pixel);
        for (index, size) in sizes.into_iter().enumerate() {
            let pose = GridPose::of(size, largest);
            let shown = grid_box(size, pose.tilt) * pose.scale;
            let share = shown / layout.cell;
            assert!(share.max_element() >= 0.25, "slot {index} ({size}) fills {share} of its cell");
            assert!(shown.x <= room.x + 1e-4 && shown.y <= room.y + 1e-4, "slot {index} ({size}) overflows its cell");
        }
    }

    #[test]
    fn flat_items_tip_toward_the_camera_and_upright_ones_stay_level() {
        let flat = Vec3::new(3.0, 0.2, 1.0);
        let upright = Vec3::new(1.0, 8.0, 1.0);
        assert_eq!(grid_tilt(flat), FLAT_TILT);
        assert_eq!(grid_tilt(upright), 0.0);
        // Tipped, the flat item shows its face: far taller on screen than its edge.
        assert!(grid_box(flat, FLAT_TILT).y > flat.y * 5.0);
        assert_eq!(grid_box(upright, 0.0), Vec2::new(2f32.sqrt(), 8.0));
    }

    #[test]
    fn the_largest_item_keeps_its_true_size() {
        let sizes = [Vec3::new(1.0, 8.0, 1.0), Vec3::new(0.3, 0.4, 0.2), Vec3::new(1.0, 6.0, 1.0)];
        let largest = page_box(&sizes);
        assert_eq!(GridPose::of(sizes[0], largest), GridPose { tilt: 0.0, scale: 1.0 });
        // Three quarters the height: still true to scale.
        assert!(!GridPose::of(sizes[2], largest).magnified());
        let small = GridPose::of(sizes[1], largest);
        assert!(small.magnified(), "{small:?}");
        let share = (grid_box(sizes[1], small.tilt) * small.scale / largest).max_element();
        assert!((share - MIN_FILL).abs() < 1e-4, "magnified to {share} of the largest");
    }

    #[test]
    fn labels_give_true_sizes() {
        assert_eq!(true_size(Vec3::new(0.4, 1.8, 0.2)), "18 cm");
        assert_eq!(true_size(Vec3::new(1.0, 17.0, 3.0)), "1.7 m");
    }
}
