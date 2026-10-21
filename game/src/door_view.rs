//! The doors view (`--doors`): a claim door of one kind hung in a doorway frame beside its
//! switch on the side wall, with a panel of the kind's parameters, like the Forge's. Pick the
//! kind (wood, steel, portcullis), the doorway's shape (a timber frame or one of the arches),
//! the level style (the portcullis is timber in rough rock and iron in the halls) and the
//! switch (a lever or a chain); drag a slider and the door rebuilds live. "Open" works the door
//! as the game does (swung or raised at its own pace, its lever thrown or its chain drawn
//! down) and "Pull switch" works it from the switch.
//!
//! Keys: Space opens and shuts the door, P pulls the switch (the same), N rolls a new seed, R
//! rolls the parameters again, Left and Right step the seed. Mouse: dragging the stage orbits
//! and scrolling zooms; on the panel dragging a slider sets it and clicking a choice cycles it.
//! Command line: `--doors --kind steel --doorway round_arch --level stone_halls --switch chain
//! --seed 7 --open` ([`DoorLab::from_args`]).

use armory::{
    ItemMesh, MaterialKind,
    build::parts::Parts,
    cond::ConditionState,
    geom::cuboid,
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
use fb_level::{DoorKind, DoorStyle, LevelStyle, SwitchKind};

use crate::doors::{ChainPull, DoorLeaf, LeverHandle, door_seconds, switch_mount};
use crate::forge_view::fresh_seed;
use crate::level::geometry::{self, door::DoorSpec};
use crate::level::{StonePiece, fixture_appearance};
use crate::light::StudioLight;
use crate::render::ItemSpawner;
use crate::turntable::{Orbit, PLATE, PLATE_INK, framing_distance};
use crate::tweak_panel::{PanelBody, Tweak, TweakPanelPlugin, TweakPanelSystems, chip, over_body, panel_body};
use crate::ui::{BG, BRIGHT, FAINT, INK, LINE, MUTED, UiFonts, node_rect};

/// Width of the panel on the right, in logical pixels.
pub const PANEL_WIDTH: f32 = 430.0;
const DEFAULT_SEED: u32 = 7;
/// Share of the stage's height the doorway fills.
const FILL: f32 = 0.85;
/// Seconds a lever's handle takes to be thrown.
const LEVER_SECONDS: f32 = crate::doors::LEVER_SECONDS;

pub struct DoorViewPlugin;

impl Plugin for DoorViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DoorLab>()
            .init_resource::<UiFonts>()
            .init_resource::<Pose>()
            .insert_resource(StageOrbit(Orbit { yaw: -0.6, pitch: 0.1, zoom: 0.9 }))
            .add_plugins(TweakPanelPlugin::<DoorLab>::default())
            .insert_resource(ClearColor(PLATE))
            .insert_resource(StudioLight::ARMORY.ambient_light(&Exposure::INDOOR))
            .add_systems(Startup, (setup_stage, setup_panel))
            .add_systems(
                Update,
                (
                    (keys, press_controls).before(TweakPanelSystems),
                    (orbit_and_zoom, show.run_if(resource_changed::<DoorLab>), refresh_header.run_if(resource_changed::<DoorLab>), animate, frame_camera).chain().after(TweakPanelSystems),
                ),
            );
    }
}

/// The door on the stage, and what it is made of.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct DoorLab {
    pub seed: u32,
    pub doorway: DoorStyle,
    pub level: LevelStyle,
    pub switch: SwitchKind,
    pub spec: DoorSpec,
    /// Whether the door opens as the view starts.
    pub start_open: bool,
    structure: u32,
}

impl Default for DoorLab {
    fn default() -> Self {
        DoorLab::new(DoorKind::Wood, DoorStyle::Timber, LevelStyle::RoughRock, SwitchKind::Lever, DEFAULT_SEED)
    }
}

impl DoorLab {
    pub fn new(kind: DoorKind, doorway: DoorStyle, level: LevelStyle, switch: SwitchKind, seed: u32) -> Self {
        DoorLab { seed, doorway, level, switch, spec: DoorSpec::rolled(kind, doorway, level, seed), start_open: false, structure: 0 }
    }

    pub fn kind(&self) -> DoorKind {
        self.spec.kind
    }

    /// Parses `--kind`, `--doorway`, `--level`, `--switch`, `--seed` and `--open` (also
    /// `--flag=value`); `--doors` itself is skipped. `args` excludes the program name.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<DoorLab, String> {
        let mut lab = DoorLab::default();
        let (mut kind, mut doorway, mut level, mut switch, mut seed, mut open) = (lab.kind(), lab.doorway, lab.level, lab.switch, lab.seed, false);
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (arg, None),
            };
            let mut value = || inline.clone().or_else(|| args.next()).ok_or(format!("{flag} needs a value"));
            match flag.as_str() {
                "--doors" => {}
                "--open" => open = true,
                "--kind" => kind = option(&DOOR_KINDS, &value()?, "--kind")?,
                "--doorway" => doorway = option(&DOORWAYS, &value()?, "--doorway")?,
                "--level" => level = option(&LEVELS, &value()?, "--level")?,
                "--switch" => switch = option(&SWITCHES, &value()?, "--switch")?,
                "--seed" => {
                    let text = value()?;
                    seed = text.parse().map_err(|_| format!("--seed: {text:?} is not a u32"))?;
                }
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        lab = DoorLab::new(kind, doorway, level, switch, seed);
        lab.start_open = open;
        Ok(lab)
    }

    /// Rolls the parameters again from the seed, dropping every edit.
    pub fn reset(&mut self) {
        self.spec = DoorSpec::rolled(self.kind(), self.doorway, self.level, self.seed);
        self.structure += 1;
    }

    pub fn set_seed(&mut self, seed: u32) {
        self.seed = seed;
        self.reset();
    }

    fn set_kind(&mut self, kind: DoorKind) {
        self.spec = DoorSpec::rolled(kind, self.doorway, self.level, self.seed);
        self.structure += 1;
    }
}

const DOOR_KINDS: [(&str, DoorKind); 3] = [("wood", DoorKind::Wood), ("steel", DoorKind::Steel), ("portcullis", DoorKind::Portcullis)];
const DOORWAYS: [(&str, DoorStyle); 4] =
    [("timber", DoorStyle::Timber), ("round_arch", DoorStyle::RoundArch), ("pointed_arch", DoorStyle::PointedArch), ("rock_cut", DoorStyle::RockCut)];
const LEVELS: [(&str, LevelStyle); 2] = [("rough_rock", LevelStyle::RoughRock), ("stone_halls", LevelStyle::StoneHalls)];
const SWITCHES: [(&str, SwitchKind); 2] = [("lever", SwitchKind::Lever), ("chain", SwitchKind::Chain)];

fn option<T: Copy>(table: &[(&str, T)], text: &str, flag: &str) -> Result<T, String> {
    table.iter().find(|(name, _)| *name == text).map(|&(_, v)| v).ok_or_else(|| format!("{flag}: {text:?} is not one of {}", table.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")))
}

fn name_of<T: Copy + PartialEq>(table: &[(&'static str, T)], value: T) -> &'static str {
    table.iter().find(|(_, v)| *v == value).map_or("", |&(n, _)| n)
}

fn choice<T: Copy + PartialEq>(section: &str, name: &str, table: &[(&'static str, T)], value: T) -> Field {
    Field {
        path: format!("{section}.{name}"),
        section: section.to_string(),
        name: name.to_string(),
        kind: FieldKind::Choice { value: name_of(table, value).to_string(), options: table.iter().map(|(n, _)| n.to_string()).collect() },
    }
}

fn slider(section: &str, name: &str, value: f32, range: Slider) -> Field {
    Field { path: format!("{section}.{name}"), section: section.to_string(), name: name.to_string(), kind: FieldKind::Slider { value, range } }
}

/// The next option of `table` `by` steps on from `value`, wrapping.
fn step<T: Copy + PartialEq>(table: &[(&'static str, T)], value: T, by: isize) -> T {
    let at = table.iter().position(|(_, v)| *v == value).unwrap_or(0) as isize;
    table[(at + by).rem_euclid(table.len() as isize) as usize].1
}

impl Tweak for DoorLab {
    fn fields(&self) -> Vec<Field> {
        let s = &self.spec;
        let mut fields = vec![
            choice("door", "kind", &DOOR_KINDS, s.kind),
            choice("door", "doorway", &DOORWAYS, self.doorway),
            choice("door", "level", &LEVELS, self.level),
            choice("door", "switch", &SWITCHES, self.switch),
        ];
        match s.kind {
            DoorKind::Wood => fields.extend([
                slider("wood", "boards", s.boards as f32, Slider::new(3.0, 10.0, 1.0)),
                slider("wood", "straps", s.straps as f32, Slider::new(2.0, 4.0, 1.0)),
                slider("wood", "thickness", s.thickness, Slider::new(0.6, 1.6, 0.05)),
            ]),
            DoorKind::Steel => fields.extend([
                slider("steel", "bands", s.bands as f32, Slider::new(2.0, 6.0, 1.0)),
                slider("steel", "ribs", s.ribs as f32, Slider::new(0.0, 2.0, 1.0)),
                slider("steel", "rivet_gap", s.rivet_gap, Slider::new(0.8, 3.0, 0.1)),
                slider("steel", "thickness", s.thickness, Slider::new(0.8, 1.6, 0.05)),
            ]),
            DoorKind::Portcullis => {
                fields.extend([
                    slider("portcullis", "bars", s.bars as f32, Slider::new(3.0, 14.0, 1.0)),
                    slider("portcullis", "rails", s.rails as f32, Slider::new(2.0, 9.0, 1.0)),
                    slider("portcullis", "bar", s.bar, Slider::new(0.15, 0.6, 0.01)),
                ]);
                fields.push(Field { path: "portcullis.timber".into(), section: "portcullis".into(), name: "timber".into(), kind: FieldKind::Toggle { value: s.timber } });
            }
        }
        fields
    }

    fn set_slider(&mut self, path: &str, value: f32) {
        let s = &mut self.spec;
        match path {
            "wood.boards" => s.boards = value as u32,
            "wood.straps" => s.straps = value as u32,
            "wood.thickness" | "steel.thickness" => s.thickness = value,
            "steel.bands" => s.bands = value as u32,
            "steel.ribs" => s.ribs = value as u32,
            "steel.rivet_gap" => s.rivet_gap = value,
            "portcullis.bars" => s.bars = value as u32,
            "portcullis.rails" => s.rails = value as u32,
            "portcullis.bar" => s.bar = value,
            _ => {}
        }
    }

    fn cycle(&mut self, path: &str, by: isize) {
        match path {
            "door.kind" => self.set_kind(step(&DOOR_KINDS, self.kind(), by)),
            "door.doorway" => {
                self.doorway = step(&DOORWAYS, self.doorway, by);
                self.reset();
            }
            "door.level" => {
                self.level = step(&LEVELS, self.level, by);
                self.reset();
            }
            "door.switch" => {
                self.switch = step(&SWITCHES, self.switch, by);
                self.structure += 1;
            }
            _ => {}
        }
    }

    fn flip(&mut self, path: &str) {
        if path == "portcullis.timber" {
            self.spec.timber = !self.spec.timber;
            self.spec.bar = if self.spec.timber { 0.42 } else { 0.28 };
        }
    }

    fn structure(&self) -> u32 {
        self.structure
    }
}

/// How far the door and its switch are moved: the door's `swing` (0 shut, 1 open) follows
/// whether it is `open`, and the lever's `pull` follows the door.
#[derive(Resource, Debug, Default)]
pub struct Pose {
    pub open: bool,
    pub swing: f32,
    pub pull: f32,
}

#[derive(Resource, Default)]
struct StageOrbit(Orbit);

#[derive(Component)]
struct StageCamera;

/// Everything built for the current door, rebuilt with it.
#[derive(Component)]
struct Shown;

#[derive(Component)]
struct Hint;

#[derive(Component, Clone, Debug, PartialEq)]
enum Control {
    Toggle,
    Pull,
    SeedStep(i64),
    New,
}

#[derive(Component, Clone, Debug, PartialEq)]
enum Readout {
    Noun,
    Seed,
}

fn setup_stage(mut commands: Commands, fonts: Res<UiFonts>, lab: Res<DoorLab>, mut pose: ResMut<Pose>) {
    let exposure = Exposure::INDOOR;
    commands
        .spawn((StageCamera, Camera3d::default(), exposure, Tonemapping::Linear, IsDefaultUiCamera, Transform::from_xyz(0.0, 12.0, 60.0)))
        .with_child(StudioLight::ARMORY.key_light(&exposure));
    commands.spawn((
        Hint,
        Text::new("Drag orbit   Scroll zoom   Space open or shut   P pull the switch   N new seed   R reset"),
        UiFonts::style(&fonts.body_medium, 17.0, PLATE_INK),
        Node { position_type: PositionType::Absolute, left: px(20), bottom: px(16), ..default() },
    ));
    if lab.start_open {
        *pose = Pose { open: true, swing: 1.0, pull: 1.0 };
    }
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
                .spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(9), padding: UiRect::all(px(14)), border: UiRect::bottom(px(1)), ..default() }, BorderColor::all(FAINT)))
                .with_children(|header| {
                    header.spawn((Text::new("The Doors"), UiFonts::style(&fonts.display_bold, 28.0, INK)));
                    header.spawn((Readout::Noun, Text::new(""), UiFonts::style(&fonts.display, 20.0, BRIGHT)));
                    header.spawn(Node { align_items: AlignItems::Center, flex_wrap: FlexWrap::Wrap, column_gap: px(6), row_gap: px(5), ..default() }).with_children(|row| {
                        chip(row, &fonts, "Open / close", Control::Toggle);
                        chip(row, &fonts, "Pull switch", Control::Pull);
                    });
                    header.spawn(Node { align_items: AlignItems::Center, column_gap: px(6), ..default() }).with_children(|row| {
                        row.spawn((Text::new("Seed"), UiFonts::style(&fonts.body, 17.0, MUTED), Node { margin: UiRect::right(px(4)), ..default() }));
                        chip(row, &fonts, "-", Control::SeedStep(-1));
                        row.spawn((Readout::Seed, Text::new(""), UiFonts::style(&fonts.mono_bold, 16.0, BRIGHT), TextLayout::justify(Justify::Center), Node { min_width: px(92), ..default() }));
                        chip(row, &fonts, "+", Control::SeedStep(1));
                        chip(row, &fonts, "New", Control::New);
                    });
                });
            panel_body(panel);
        });
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut lab: ResMut<DoorLab>, mut pose: ResMut<Pose>) {
    let pressed = |key| keys.just_pressed(key);
    if pressed(KeyCode::Space) || pressed(KeyCode::KeyP) {
        pose.open = !pose.open;
    }
    if pressed(KeyCode::ArrowRight) {
        let seed = lab.seed.wrapping_add(1);
        lab.set_seed(seed);
    }
    if pressed(KeyCode::ArrowLeft) {
        let seed = lab.seed.wrapping_sub(1);
        lab.set_seed(seed);
    }
    if pressed(KeyCode::KeyN) {
        let seed = fresh_seed(clock());
        lab.set_seed(seed);
    }
    if pressed(KeyCode::KeyR) {
        lab.reset();
    }
}

fn clock() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64)
}

fn press_controls(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    controls: Query<(&Control, &ComputedNode, &UiGlobalTransform)>,
    mut lab: ResMut<DoorLab>,
    mut pose: ResMut<Pose>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(point) = window.and_then(|w| w.cursor_position()) else { return };
    let Some((control, ..)) = controls.iter().find(|(_, node, transform)| node_rect(node, transform).contains(point)) else { return };
    match control.clone() {
        Control::Toggle | Control::Pull => pose.open = !pose.open,
        Control::SeedStep(by) => {
            let seed = lab.seed.wrapping_add_signed(by as i32);
            lab.set_seed(seed);
        }
        Control::New => lab.set_seed(fresh_seed(clock())),
    }
}

fn orbit_and_zoom(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    body: Query<(&ComputedNode, &UiGlobalTransform), With<PanelBody>>,
    mut orbit: ResMut<StageOrbit>,
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
        orbit.0.drag(motion.delta);
    }
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    if lines == 0.0 || window.cursor_position().is_none() || over_panel || over_body(&window, &body) {
        return;
    }
    orbit.0.zoom_by(lines);
}

/// The doorway's ceiling in the level style, for the masonry over it.
fn ceiling_of(level: LevelStyle) -> f32 {
    match level {
        LevelStyle::RoughRock => 28.0,
        LevelStyle::StoneHalls => 34.0,
    }
}

/// A block of stone `size` centred at `at`, as a mesh.
fn slab(size: [f32; 3], at: [f32; 3]) -> ItemMesh {
    let mut block = cuboid(size);
    block.translate(at);
    let mut parts = Parts::new();
    parts.add("wall", block, MaterialKind::Stone);
    parts.finish()
}

/// The stone beside the doorway the switch is mounted on, and the floor.
fn surroundings() -> [ItemMesh; 2] {
    // The cell's side wall at +X (its face at x = 10), running through the doorway's depth.
    [slab([4.0, 30.0, 24.0], [12.0, 15.0, 0.0]), slab([30.0, 1.0, 26.0], [0.0, -0.5, 4.0])]
}

/// Rebuilds the doorway, the door and its switch for the current state.
fn show(mut commands: Commands, lab: Res<DoorLab>, pose: Res<Pose>, old: Query<Entity, With<Shown>>, mut spawner: ItemSpawner) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let seed = lab.seed;
    let mut frame = geometry::door_frame(ceiling_of(lab.level), lab.doorway, seed);
    for part in frame.parts.iter_mut().filter(|p| p.material == Some(MaterialKind::Stone)) {
        part.uvs_b = vec![[0.2, 0.0]; part.positions.len()];
    }
    let stone = |spawner: &mut ItemSpawner, mesh: &ItemMesh, state| {
        spawner.spawn(mesh, &fixture_appearance(seed, state)).insert((Shown, StonePiece)).id()
    };
    stone(&mut spawner, &frame, ConditionState::BattleWorn);
    for mesh in surroundings() {
        let mut mesh = mesh;
        for part in &mut mesh.parts {
            part.uvs_b = vec![[0.2, 0.0]; part.positions.len()];
        }
        stone(&mut spawner, &mesh, ConditionState::BattleWorn);
    }
    let fit = geometry::door::claim_leaf(&lab.spec);
    stone(&mut spawner, &fit.fixed, ConditionState::BattleWorn);
    let leaf = DoorLeaf { cell: (0, 0), shut: Quat::IDENTITY, hinge: Vec3::from(fit.hinge), lift: fit.lift, squash: fit.squash_for(ceiling_of(lab.level)) };
    spawner.spawn(&fit.leaf, &fixture_appearance(seed, ConditionState::Serviceable)).insert((Shown, Name::new("door leaf"), leaf, leaf.transform(pose.swing)));

    // The switch on the side wall, on the +Z side, as the game mounts it.
    let at_door = Transform::IDENTITY;
    let Some(mount) = switch_mount(&at_door, None, lab.level, lab.switch, Vec2::new(0.0, 1.0), Vec2::new(0.0, 1.0)) else { return };
    let (fixture, moving) = match lab.switch {
        SwitchKind::Lever => (geometry::door::lever_plate(seed), geometry::door::lever_handle()),
        SwitchKind::Chain => (geometry::door::chain_housing(seed), geometry::door::chain_pull(seed)),
    };
    spawner.spawn(&fixture, &fixture_appearance(seed, ConditionState::BattleWorn)).insert((Shown, Name::new("switch"), mount));
    match lab.switch {
        SwitchKind::Lever => {
            let pivot = mount.mul_transform(Transform::from_translation(Vec3::new(0.0, 0.0, geometry::door::LEVER_PIVOT)));
            let handle = LeverHandle { cell: (0, 0), rest: mount.rotation, pull: pose.pull };
            spawner.spawn(&moving, &fixture_appearance(seed, ConditionState::BattleWorn)).insert((Shown, Name::new("lever handle"), handle, pivot.with_rotation(handle.rotation(pose.pull))));
        }
        SwitchKind::Chain => {
            let rest = mount.transform_point(Vec3::from(geometry::door::chain_exit()));
            let chain = ChainPull { cell: (0, 0), rest, down: mount.rotation * Vec3::NEG_Y };
            spawner.spawn(&moving, &fixture_appearance(seed, ConditionState::BattleWorn)).insert((Shown, Name::new("chain"), chain, Transform::from_translation(chain.translation(pose.swing)).with_rotation(mount.rotation)));
        }
    }
}

/// Moves the door and its switch toward where `open` says.
fn animate(
    time: Res<Time>,
    lab: Res<DoorLab>,
    mut pose: ResMut<Pose>,
    mut leaves: Query<(&DoorLeaf, &mut Transform), (Without<LeverHandle>, Without<ChainPull>)>,
    mut levers: Query<(&LeverHandle, &mut Transform), (Without<DoorLeaf>, Without<ChainPull>)>,
    mut chains: Query<(&ChainPull, &mut Transform), (Without<DoorLeaf>, Without<LeverHandle>)>,
) {
    let want = f32::from(pose.open);
    let dt = time.delta_secs();
    let swing = move_toward(pose.swing, want, dt / door_seconds(lab.kind()));
    let pull = move_toward(pose.pull, want, dt / LEVER_SECONDS);
    if swing != pose.swing {
        pose.swing = swing;
    }
    if pull != pose.pull {
        pose.pull = pull;
    }
    for (leaf, mut transform) in &mut leaves {
        *transform = leaf.transform(swing);
    }
    for (handle, mut transform) in &mut levers {
        transform.rotation = handle.rotation(pull);
    }
    for (chain, mut transform) in &mut chains {
        transform.translation = chain.translation(swing);
    }
}

fn move_toward(from: f32, to: f32, step: f32) -> f32 {
    if from < to { (from + step).min(to) } else { (from - step).max(to) }
}

fn refresh_header(lab: Res<DoorLab>, mut readouts: Query<(&Readout, &mut Text)>) {
    for (readout, mut text) in &mut readouts {
        let line = match readout {
            Readout::Noun => format!("{} door, {} switch", name_of(&DOOR_KINDS, lab.kind()), name_of(&SWITCHES, lab.switch)),
            Readout::Seed => lab.seed.to_string(),
        };
        if text.0 != line {
            text.0 = line;
        }
    }
}

/// Orbits the camera round the doorway, shifted so it sits in the middle of the space the
/// panel leaves.
fn frame_camera(orbit: Res<StageOrbit>, mut cameras: Query<(&Camera, &Projection, &mut Transform), With<StageCamera>>) {
    let centre = Vec3::new(3.0, 11.0, 2.0);
    for (camera, projection, mut transform) in &mut cameras {
        let Projection::Perspective(perspective) = projection else { continue };
        let width = camera.logical_viewport_size().map_or(1280.0, |s| s.x).max(1.0);
        let panel = (PANEL_WIDTH / width).clamp(0.0, 0.8);
        let visible = PerspectiveProjection { aspect_ratio: perspective.aspect_ratio * (1.0 - panel), ..*perspective };
        let distance = framing_distance(Vec3::new(30.0, 26.0, 6.0), &visible, FILL);
        let mut target = Transform::from_translation(centre + orbit.0.offset(distance)).looking_at(centre, Vec3::Y);
        let half_width = distance * orbit.0.zoom * (perspective.fov * 0.5).tan() * perspective.aspect_ratio;
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
        let lab = DoorLab::from_args(args("--doors --kind steel --doorway round_arch --level stone_halls --switch chain --seed 9 --open")).unwrap();
        assert_eq!((lab.kind(), lab.doorway, lab.level, lab.switch, lab.seed, lab.start_open), (DoorKind::Steel, DoorStyle::RoundArch, LevelStyle::StoneHalls, SwitchKind::Chain, 9, true));
        assert_eq!(lab.spec, DoorSpec::rolled(DoorKind::Steel, DoorStyle::RoundArch, LevelStyle::StoneHalls, 9));
        assert!(DoorLab::from_args(args("--kind brass")).is_err());
        assert!(DoorLab::from_args(args("--switch")).is_err());
        assert!(DoorLab::from_args(args("--wat")).is_err());
    }

    #[test]
    fn every_kind_has_sliders_and_the_choices_cycle_every_option() {
        for (_, kind) in DOOR_KINDS {
            let lab = DoorLab::new(kind, DoorStyle::Timber, LevelStyle::RoughRock, SwitchKind::Lever, 3);
            let fields = lab.fields();
            assert!(fields.iter().filter(|f| matches!(f.kind, FieldKind::Slider { .. })).count() >= 3, "{kind:?}");
            assert_eq!(fields.iter().filter(|f| f.section == "door").count(), 4);
        }
        let mut lab = DoorLab::default();
        let before = lab.structure();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..3 {
            lab.cycle("door.kind", 1);
            seen.insert(lab.kind());
        }
        assert_eq!(seen.len(), 3);
        assert!(lab.structure() > before, "a new kind is a new panel");
        for _ in 0..4 {
            lab.cycle("door.doorway", 1);
        }
        assert_eq!(lab.doorway, DoorStyle::Timber);
    }

    #[test]
    fn dragging_a_slider_rebuilds_the_door_and_reset_goes_back() {
        let mut lab = DoorLab::new(DoorKind::Portcullis, DoorStyle::PointedArch, LevelStyle::StoneHalls, SwitchKind::Chain, 5);
        let rolled = lab.clone();
        let before = geometry::door::claim_leaf(&lab.spec);
        lab.set_slider("portcullis.bars", 12.0);
        lab.set_slider("portcullis.rails", 8.0);
        let after = geometry::door::claim_leaf(&lab.spec);
        assert_ne!(before, after);
        assert_eq!(lab.slider("portcullis.bars"), Some(12.0));
        // Every slider value stays within its range and fits the doorway whatever it is set to.
        for field in lab.fields() {
            if let FieldKind::Slider { range, .. } = field.kind {
                for value in [range.min, range.max] {
                    lab.set_slider(&field.path, value);
                    for part in geometry::door::claim_leaf(&lab.spec).leaf.parts {
                        part.validate().unwrap();
                    }
                }
            }
        }
        lab.reset();
        assert_eq!(lab.spec, rolled.spec);
    }

    #[test]
    fn the_door_moves_at_its_own_pace_toward_where_it_is_told() {
        assert_eq!(move_toward(0.0, 1.0, 0.25), 0.25);
        assert_eq!(move_toward(0.9, 1.0, 0.25), 1.0);
        assert_eq!(move_toward(0.1, 0.0, 0.25), 0.0);
        assert!(door_seconds(DoorKind::Wood) < door_seconds(DoorKind::Portcullis));
    }
}
