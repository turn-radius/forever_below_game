//! The Armory turntable: a grid of items from consecutive seeds of one family, each spinning
//! slowly over a name label. Works for every family [`Item`] can roll.
//!
//! Keys: Right/Left page the seeds by a grid, Up/Down or Tab switch family, W steps the
//! condition from pristine to grave find, Space pauses the spin.
//! Command line: `--family mace --seed 1207 --wear grave_find` ([`Turntable::from_args`]).

use armory::{
    Family, Item,
    cond::{Condition, ConditionState},
};
use bevy::{camera::Exposure, prelude::*, shape::BoundingVolume, ui::IsDefaultUiCamera};

use crate::light::Torch;
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
const LABEL_WIDTH: f32 = 320.0;
const INK: Color = Color::srgb(0.92, 0.898, 0.835);
const BACKDROP: Color = Color::srgb(0.035, 0.04, 0.06);

/// Shows the [`Turntable`] page: add with the resource set from [`Turntable::from_args`] or
/// left at its default.
pub struct TurntablePlugin;

impl Plugin for TurntablePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Turntable>()
            .init_resource::<Spin>()
            .init_resource::<PageLayout>()
            .insert_resource(ClearColor(BACKDROP))
            // Low ambient so the torch matters (`docs/rendering.md`, "Light"); the tint is the
            // plate's ambient `#b6b0c8`.
            .insert_resource(GlobalAmbientLight { color: Color::srgb_u8(0xb6, 0xb0, 0xc8), brightness: 12.0, ..default() })
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    controls,
                    (show_page, show_status).run_if(resource_changed::<Turntable>),
                    spin,
                    frame_camera,
                    place_labels,
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
        Turntable { family: Item::PORTED[0], first_seed: DEFAULT_SEED, wear: ConditionState::Pristine }
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

    /// Centre of grid slot `index` (row-major from top left).
    fn slot(&self, index: usize) -> Vec3 {
        let (column, row) = ((index % COLUMNS) as f32, (index / COLUMNS) as f32);
        let x = (column - (COLUMNS - 1) as f32 / 2.0) * self.cell.x;
        let y = ((ROWS - 1) as f32 / 2.0 - row) * self.cell.y;
        // Items sit high in their cell; the label fills the bottom.
        Vec3::new(x, y + self.cell.y * 0.06, 0.0)
    }

    fn grid(&self) -> Vec2 {
        self.cell * Vec2::new(COLUMNS as f32, ROWS as f32)
    }
}

/// Everything [`show_page`] spawns, cleared when the page changes.
#[derive(Component)]
struct OnPage;

/// A spinning pivot holding one item, centred on the item's bounds.
#[derive(Component)]
struct Slot(usize);

/// A name label under a slot.
#[derive(Component)]
struct Label {
    anchor: Vec3,
}

#[derive(Component)]
struct StatusLine;

#[derive(Component)]
struct TurntableCamera;

fn setup(mut commands: Commands) {
    commands
        .spawn((
            TurntableCamera,
            Camera3d::default(),
            Exposure::INDOOR,
            // UI follows this camera even when frame capture retargets it to an image.
            IsDefaultUiCamera,
            Transform::from_xyz(0.0, 0.0, 60.0).looking_at(Vec3::ZERO, Vec3::Y),
        ))
        // The torch hangs front right of the camera, so it keeps its place as the framing changes.
        .with_child((Torch::light(1.5e6, 400.0), Transform::from_xyz(14.0, 4.0, -18.0)));
    // The plate's key: `#fff1dc` from (-6, 10, 9).
    commands.spawn((
        DirectionalLight { color: Color::srgb_u8(0xff, 0xf1, 0xdc), illuminance: 350.0, ..default() },
        Transform::from_xyz(-6.0, 10.0, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        StatusLine,
        Text::default(),
        TextFont::from_font_size(18.0),
        TextColor(INK),
        Node { position_type: PositionType::Absolute, top: px(16.0), left: px(20.0), ..default() },
    ));
}

fn controls(keys: Res<ButtonInput<KeyCode>>, mut turntable: ResMut<Turntable>, mut spin: ResMut<Spin>) {
    if keys.just_pressed(KeyCode::ArrowRight) {
        turntable.page(1);
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        turntable.page(-1);
    }
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::Tab) {
        turntable.step_family(1);
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        turntable.step_family(-1);
    }
    if keys.just_pressed(KeyCode::KeyW) {
        turntable.step_wear();
    }
    if keys.just_pressed(KeyCode::Space) {
        spin.paused = !spin.paused;
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
        let name = item.as_ref().map_or_else(|| format!("{} (not ported)", turntable.family.key()), Item::noun);
        commands.spawn((
                OnPage,
                Label { anchor: centre - Vec3::Y * (layout.item_height * 0.5 + layout.cell.y * 0.04) },
                Text::new(format!("{name}\n{seed}")),
                TextFont::from_font_size(17.0),
                TextColor(INK),
                TextLayout::justify(Justify::Center),
                Node { position_type: PositionType::Absolute, width: px(LABEL_WIDTH), ..default() },
                // Hidden until `place_labels` has a camera position for it.
                Visibility::Hidden,
            ));
        let Some(item) = item else { continue };
        let slot = commands.spawn((OnPage, Slot(index), Transform::from_translation(centre), Visibility::default())).id();
        let offset = bounds.map_or(Vec3::Y * -FALLBACK_ITEM_SIZE.y * 0.5, |b| -Vec3::from(b.center()));
        spawner.spawn(mesh, &turntable.appearance(item)).insert((Transform::from_translation(offset), ChildOf(slot)));
    }
}

fn show_status(turntable: Res<Turntable>, mut status: Query<&mut Text, With<StatusLine>>) {
    let last = turntable.first_seed.wrapping_add(PAGE - 1);
    let line = format!(
        "{}   seeds {}-{}   {}\nLeft/Right page   Up/Down or Tab family   W wear   Space pause",
        turntable.family.key(),
        turntable.first_seed,
        last,
        wear_name(turntable.wear),
    );
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
        transform.rotation = Quat::from_rotation_y(state.angle + slot.0 as f32 * 0.7);
    }
}

/// Backs the camera off until the whole grid fits the view.
fn frame_camera(layout: Res<PageLayout>, mut cameras: Query<(&Projection, &mut Transform), With<TurntableCamera>>) {
    for (projection, mut transform) in &mut cameras {
        let Projection::Perspective(perspective) = projection else { continue };
        let half_tan = (perspective.fov * 0.5).tan();
        // Leave room at the top for the status line.
        let grid = layout.grid() * Vec2::new(1.04, 1.18);
        let distance = (grid.y * 0.5 / half_tan).max(grid.x * 0.5 / (half_tan * perspective.aspect_ratio));
        let lift = Vec3::Y * grid.y * 0.04;
        let target = Transform::from_translation(Vec3::Z * distance + lift).looking_at(lift, Vec3::Y);
        if *transform != target {
            *transform = target;
        }
    }
}

/// Pins each label under its slot in screen space.
fn place_labels(
    cameras: Query<(&Camera, &GlobalTransform), With<TurntableCamera>>,
    mut labels: Query<(&Label, &mut Node, &mut Visibility)>,
) {
    let Ok((camera, camera_transform)) = cameras.single() else { return };
    for (label, mut node, mut visibility) in &mut labels {
        let Ok(at) = camera.world_to_viewport(camera_transform, label.anchor) else { continue };
        node.left = px(at.x - LABEL_WIDTH * 0.5);
        node.top = px(at.y);
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
        assert!(Turntable::from_args(args("--family sword")).unwrap_err().contains("not ported"));
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
}
