//! The level view (`--level`): a whole level drawn from above as the map draws it
//! ([`crate::map::plan`]), every cell shown, beside a panel of the generator's parameters
//! ([`fb_level::LevelParams`], in the shared [`crate::tweak_panel`]). Moving a slider lays the
//! level out again from the same seed ([`fb_level::generate_with`]); "Forge new" rolls a new
//! seed and Reset goes back to the defaults, which lay out every seed as the game does.
//!
//! Keys: Left and Right step the seed, N rolls a new one, R resets the parameters. Mouse:
//! scrolling over the plan zooms about the mouse and dragging pans; on the panel, sliders drag
//! and the panel scrolls. Command line: `--level --seed 7`.

use armory::reflect::Field;
use bevy::{
    asset::RenderAssetUsages,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::PrimaryWindow,
};
use fb_level::sight::Explored;
use fb_level::{Cell, FloorFinish, Level, LevelParams, LevelStyle, WallFinish, finishes, generate_with};

use crate::forge_view::fresh_seed;
use crate::map::plan::{MARGIN, PlanCanvas, PlanFrame, draw_plan_with};
use crate::map::screen::{MapView, ZOOM};
use crate::tweak_panel::{PanelBody, Tweak, TweakPanelPlugin, TweakPanelSystems, chip, over_body, panel_body};
use crate::ui::{BG, BRIGHT, FAINT, INK, LINE, MUTED, UiFonts, node_rect};

/// Width of the panel on the right, in logical pixels.
pub const PANEL_WIDTH: f32 = 430.0;
const DEFAULT_SEED: u32 = 1207;
/// The plan is drawn with about this many pixels across its longer side, however big the
/// level, so dragging a slider stays quick. Our own.
const PLAN_PIXELS: f32 = 1600.0;
/// How near and far the level view zooms, pixels to a cell: further out than the map, to take
/// in a big level whole.
const LEVEL_ZOOM: (f32, f32) = (ZOOM.0 * 0.25, ZOOM.1);

pub struct LevelViewPlugin;

impl Plugin for LevelViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LevelForge>()
            .init_resource::<UiFonts>()
            .init_resource::<PlanView>()
            .insert_resource(ClearColor(BG))
            .add_plugins(TweakPanelPlugin::<LevelForge>::default())
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    (keys, press_controls).before(TweakPanelSystems),
                    (draw_level.run_if(resource_changed::<LevelForge>), refresh_header.run_if(resource_changed::<LevelForge>), refresh_quarter_legend.run_if(resource_changed::<LevelForge>), zoom_and_pan, place_plan).chain().after(TweakPanelSystems),
                ),
            );
    }
}

/// The level being shaped: its seed, its parameters and the level they lay out.
#[derive(Resource, Clone, Debug)]
pub struct LevelForge {
    pub seed: u32,
    /// How deep the level lies; the depth picks its style ([`fb_level::LevelStyle::for_depth`]).
    pub depth: u32,
    pub params: LevelParams,
    pub level: Level,
    /// Counts new seeds, so the view frames the new level.
    pub framing: u32,
}

impl Default for LevelForge {
    fn default() -> Self {
        LevelForge::new(DEFAULT_SEED, LevelParams::default())
    }
}

impl LevelForge {
    pub fn new(seed: u32, params: LevelParams) -> Self {
        LevelForge::at_depth(seed, 1, params)
    }

    pub fn at_depth(seed: u32, depth: u32, params: LevelParams) -> Self {
        LevelForge { seed, depth, params, level: generate_with(seed, depth, &params), framing: 0 }
    }

    /// Parses `--seed <n>` and `--depth <n>` (also `--seed=n`); `--level` itself is skipped.
    /// The parameters start as the depth's style's own.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<LevelForge, String> {
        let mut seed = DEFAULT_SEED;
        let mut depth = 1;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (arg, None),
            };
            match flag.as_str() {
                "--level" => {}
                "--seed" => {
                    let text = inline.or_else(|| args.next()).ok_or("--seed needs a value")?;
                    seed = text.parse().map_err(|_| format!("--seed: {text:?} is not a u32"))?;
                }
                "--depth" => {
                    let text = inline.or_else(|| args.next()).ok_or("--depth needs a value")?;
                    depth = text.parse().ok().filter(|&d| d >= 1).ok_or_else(|| format!("--depth: {text:?} is not a depth from 1"))?;
                }
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        Ok(LevelForge::at_depth(seed, depth, LevelParams::for_style(LevelStyle::for_depth(depth))))
    }

    /// Lays the level out again from the seed and parameters.
    fn relay(&mut self) {
        self.level = generate_with(self.seed, self.depth, &self.params);
    }

    /// Moves to `depth` (from 1), taking that depth's style's own parameters.
    pub fn set_depth(&mut self, depth: u32) {
        self.depth = depth.max(1);
        self.reset();
    }

    pub fn set_seed(&mut self, seed: u32) {
        self.seed = seed;
        self.framing += 1;
        self.relay();
    }

    /// Back to the generator's own parameters for this depth, as the game lays levels out.
    pub fn reset(&mut self) {
        self.params = LevelParams::for_style(LevelStyle::for_depth(self.depth));
        self.framing += 1;
        self.relay();
    }

    /// A line of what the level holds: "9 rooms · 412 floor cells · 31 items · 6 lights".
    pub fn summary(&self) -> String {
        let l = &self.level;
        let floor = l.cells.iter().filter(|c| c.is_open()).count();
        let doors = l.cells.iter().filter(|&&c| c == Cell::Door).count();
        format!(
            "{} rooms \u{b7} {floor} floor cells \u{b7} {doors} doors\n{} items \u{b7} {} lights \u{b7} {} props \u{b7} {} creatures \u{b7} {} folk",
            l.rooms.len(),
            l.items.len(),
            l.lights.len(),
            l.props.len(),
            l.creatures.len(),
            l.folk.len()
        )
    }
}

impl Tweak for LevelForge {
    fn fields(&self) -> Vec<Field> {
        self.params.fields()
    }

    fn set_slider(&mut self, path: &str, value: f32) {
        if self.params.set_slider(path, value).is_ok() {
            self.relay();
        }
    }

    /// The parameters never change shape.
    fn structure(&self) -> u32 {
        0
    }

    fn slider(&self, path: &str) -> Option<f32> {
        self.params.slider(path)
    }
}

/// What part of the plan is shown, and the framing it was last fitted for.
#[derive(Resource, Debug, Default)]
struct PlanView {
    view: Option<MapView>,
    fitted: Option<u32>,
    image: Handle<Image>,
}

#[derive(Component)]
struct PlanNode;

/// The legend's column of quarters, rebuilt for each level: an outline colour and a character.
#[derive(Component)]
struct QuarterLegend;

/// The stage the plan is drawn on: the window less the panel.
#[derive(Component)]
struct Stage;

#[derive(Component, Clone, Copy, PartialEq)]
enum Control {
    SeedStep(i64),
    DepthStep(i32),
    ForgeNew,
    Reset,
}

#[derive(Component, Clone, Copy, PartialEq)]
enum Readout {
    Seed,
    Depth,
    Summary,
}

fn setup(mut commands: Commands, fonts: Res<UiFonts>, mut images: ResMut<Assets<Image>>, mut plan: ResMut<PlanView>) {
    commands.spawn((Camera2d, bevy::ui::IsDefaultUiCamera));
    plan.image = images.add(Image::new(
        Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![0; 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    ));
    commands
        .spawn((Stage, Node { position_type: PositionType::Absolute, left: px(0), top: px(0), right: px(PANEL_WIDTH), bottom: px(0), overflow: Overflow::clip(), ..default() }))
        .with_children(|stage| {
            stage.spawn((PlanNode, ImageNode::new(plan.image.clone()), Node { position_type: PositionType::Absolute, ..default() }));
            stage.spawn((
                Text::new("Scroll zoom   Drag pan   Left Right seed   Up Down depth   N new seed   R reset"),
                UiFonts::style(&fonts.body_medium, 17.0, MUTED),
                Node { position_type: PositionType::Absolute, left: px(20), bottom: px(16), ..default() },
            ));
            stage.spawn(Node { position_type: PositionType::Absolute, left: px(20), top: px(16), flex_direction: FlexDirection::Column, row_gap: px(5), ..default() }).with_children(|legend| {
                for (tint, label) in legend_rows() {
                    legend.spawn(Node { align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|row| {
                        row.spawn((Node { width: px(14), height: px(14), ..default() }, BackgroundColor(Color::srgb(tint[0], tint[1], tint[2]))));
                        row.spawn((Text::new(label), UiFonts::style(&fonts.body, 15.0, MUTED)));
                    });
                }
                legend.spawn((QuarterLegend, Node { flex_direction: FlexDirection::Column, row_gap: px(5), margin: UiRect::top(px(8)), ..default() }));
            });
        });
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
                    header.spawn((Text::new("The Level"), UiFonts::style(&fonts.display_bold, 28.0, INK)));
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
                        chip(row, &fonts, "New seed", Control::ForgeNew);
                        chip(row, &fonts, "Reset", Control::Reset);
                    });
                    header.spawn(Node { align_items: AlignItems::Center, column_gap: px(6), ..default() }).with_children(|row| {
                        row.spawn((Text::new("Depth"), UiFonts::style(&fonts.body, 17.0, MUTED), Node { margin: UiRect::right(px(4)), ..default() }));
                        chip(row, &fonts, "-", Control::DepthStep(-1));
                        row.spawn((
                            Readout::Depth,
                            Text::new(""),
                            UiFonts::style(&fonts.mono_bold, 16.0, BRIGHT),
                            TextLayout::justify(Justify::Center),
                            Node { min_width: px(190), ..default() },
                        ));
                        chip(row, &fonts, "+", Control::DepthStep(1));
                    });
                    header.spawn((Readout::Summary, Text::new(""), UiFonts::style(&fonts.body, 16.0, MUTED)));
                });
            panel_body(panel);
        });
}

fn clock() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64)
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut forge: ResMut<LevelForge>) {
    if keys.just_pressed(KeyCode::ArrowRight) {
        let seed = forge.seed.wrapping_add(1);
        forge.set_seed(seed);
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        let seed = forge.seed.wrapping_sub(1);
        forge.set_seed(seed);
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        let depth = forge.depth + 1;
        forge.set_depth(depth);
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        let depth = forge.depth.saturating_sub(1);
        forge.set_depth(depth);
    }
    if keys.just_pressed(KeyCode::KeyN) {
        forge.set_seed(fresh_seed(clock()));
    }
    if keys.just_pressed(KeyCode::KeyR) {
        forge.reset();
    }
}

fn press_controls(buttons: Res<ButtonInput<MouseButton>>, window: Option<Single<&Window, With<PrimaryWindow>>>, controls: Query<(&Control, &ComputedNode, &UiGlobalTransform)>, mut forge: ResMut<LevelForge>) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(point) = window.and_then(|w| w.cursor_position()) else { return };
    let Some((control, ..)) = controls.iter().find(|(_, node, transform)| node_rect(node, transform).contains(point)) else { return };
    match *control {
        Control::SeedStep(by) => {
            let seed = forge.seed.wrapping_add_signed(by as i32);
            forge.set_seed(seed);
        }
        Control::DepthStep(by) => {
            let depth = forge.depth.saturating_add_signed(by);
            forge.set_depth(depth);
        }
        Control::ForgeNew => forge.set_seed(fresh_seed(clock())),
        Control::Reset => forge.reset(),
    }
}

/// Pixels to a cell the plan is drawn at for `level`: about [`PLAN_PIXELS`] across.
pub fn plan_px(level: &Level) -> f32 {
    (PLAN_PIXELS / (level.width.max(level.height) as f32 + 2.0 * MARGIN)).clamp(6.0, 32.0)
}

/// Draws the whole level into the plan's picture.
fn draw_level(forge: Res<LevelForge>, plan: Res<PlanView>, mut images: ResMut<Assets<Image>>) {
    let canvas = draw_plan_with(&forge.level, &Explored::all(&forge.level), plan_px(&forge.level), |canvas, frame| draw_finishes(canvas, frame, &forge.level, &forge.params));
    let image = Image::new(
        Extent3d { width: canvas.width, height: canvas.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        canvas.to_rgba8(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    let _ = images.insert(&plan.image, image);
}

/// What a floor finish is tinted on the plan.
const fn floor_tint(floor: FloorFinish) -> [f32; 3] {
    match floor {
        FloorFinish::Rock => [0.55, 0.42, 0.30],
        FloorFinish::Flagstone => [0.42, 0.52, 0.66],
        FloorFinish::Cobble => [0.80, 0.62, 0.25],
    }
}

const MOSS_TINT: [f32; 3] = [0.30, 0.78, 0.30];
const MARK_INK: [f32; 3] = [0.902, 0.863, 0.776];

/// The level's finishes over the plan's floor: each floor tinted by how it is laid, moss in
/// green by how thick it grows, and a mark at the middle of each room for its walls.
fn draw_finishes(canvas: &mut PlanCanvas, frame: &PlanFrame, level: &Level, params: &LevelParams) {
    let f = finishes(level, params);
    for y in 0..level.height as i32 {
        for x in 0..level.width as i32 {
            let Some(finish) = f.at(x, y) else { continue };
            let (a, b) = (frame.at(x as f32, y as f32), frame.at(x as f32 + 1.0, y as f32 + 1.0));
            canvas.rect(a, b, floor_tint(finish.floor), 0.5);
            canvas.rect(a, b, MOSS_TINT, finish.moss * 0.8);
        }
    }
    // Each quarter outlined in its own colour along the edges where it meets another quarter, or
    // rock, so the groups of rooms read at a glance.
    let edge = (frame.px * 0.14).max(2.0);
    for y in 0..level.height as i32 {
        for x in 0..level.width as i32 {
            let Some(q) = f.quarter(x, y) else { continue };
            let (a, b) = (frame.at(x as f32, y as f32), frame.at(x as f32 + 1.0, y as f32 + 1.0));
            // Corridors are tinted harder than rooms, so a quarter's corridors read as its own.
            canvas.rect(a, b, quarter_tint(q), if level.room_at(x, y).is_some() { 0.13 } else { 0.30 });
            // Each side of a cell that meets another quarter, drawn a little inside so both
            // quarters show their own colour. Rock has no quarter; its walls are drawn already.
            let inset = edge * 0.5;
            let sides = [((0, -1), [a[0], a[1] + inset], [b[0], a[1] + inset]), ((0, 1), [a[0], b[1] - inset], [b[0], b[1] - inset]), ((-1, 0), [a[0] + inset, a[1]], [a[0] + inset, b[1]]), ((1, 0), [b[0] - inset, a[1]], [b[0] - inset, b[1]])];
            for ((dx, dy), p, r) in sides {
                if f.quarter(x + dx, y + dy).is_some_and(|n| n != q) {
                    canvas.line(p, r, edge, quarter_tint(q), 0.95);
                }
            }
        }
    }
    // Each room outlined in its quarter's colour.
    for (q, quarter) in f.quarters().iter().enumerate() {
        for &r in &quarter.rooms {
            let room = level.rooms[r];
            let (a, b) = (frame.at(room.x as f32, room.y as f32), frame.at((room.x + room.w) as f32, (room.y + room.h) as f32));
            let (a, b) = ([a[0] + edge * 0.5, a[1] + edge * 0.5], [b[0] - edge * 0.5, b[1] - edge * 0.5]);
            for (p, s) in [(a, [b[0], a[1]]), ([b[0], a[1]], b), (b, [a[0], b[1]]), ([a[0], b[1]], a)] {
                canvas.line(p, s, edge, quarter_tint(q), 0.9);
            }
        }
    }
    // A square in each room's corner in its purpose's colour, named in the legend.
    for (r, room) in level.rooms.iter().enumerate() {
        let Some(purpose) = level.purpose_of(r) else { continue };
        let c = frame.at(room.x as f32 + 0.5, room.y as f32 + 0.5);
        let h = frame.px * 0.3;
        canvas.rect([c[0] - h, c[1] - h], [c[0] + h, c[1] + h], purpose_tint(purpose), 1.0);
    }
    let ink = (frame.px * 0.09).max(1.2);
    for room in &level.rooms {
        let (cx, cy) = room.centre();
        let c = frame.at(cx, cy);
        let r = frame.px * 0.32;
        let Some(finish) = f.at(cx as i32, cy as i32).or_else(|| f.at(room.x as i32, room.y as i32)) else { continue };
        match finish.walls {
            WallFinish::Rock => canvas.ring(c, r, ink, MARK_INK, 0.9),
            WallFinish::Strata => {
                for k in [-0.5, 0.0, 0.5] {
                    canvas.line([c[0] - r, c[1] + k * r], [c[0] + r, c[1] + k * r], ink, MARK_INK, 0.9);
                }
            }
            WallFinish::Ashlar => {
                let (a, b) = ([c[0] - r, c[1] - r], [c[0] + r, c[1] + r]);
                for (p, q) in [(a, [b[0], a[1]]), ([b[0], a[1]], b), (b, [a[0], b[1]]), ([a[0], b[1]], a), ([a[0], c[1]], [b[0], c[1]])] {
                    canvas.line(p, q, ink, MARK_INK, 0.9);
                }
            }
            WallFinish::Rubble => {
                for (dx, dy) in [(-0.6, 0.4), (0.6, 0.4), (0.0, -0.5)] {
                    canvas.disc([c[0] + dx * r, c[1] + dy * r], ink * 1.4, MARK_INK, 0.9);
                }
            }
        }
    }
}

/// The colour of a room's purpose square on the plan.
fn purpose_tint(purpose: fb_level::past::Purpose) -> [f32; 3] {
    use fb_level::past::Purpose::*;
    match purpose {
        Quarters => [0.55, 0.75, 0.55],
        CellBlock => [0.85, 0.30, 0.30],
        ThroneRoom => [0.95, 0.80, 0.25],
        Shrine => [0.90, 0.90, 0.98],
        Library => [0.45, 0.55, 0.95],
        Ossuary => [0.80, 0.72, 0.60],
        Guardroom => [0.95, 0.55, 0.20],
    }
}

/// The outline colour of quarter `q`, cycling through colours that stand apart from the floors'.
fn quarter_tint(q: usize) -> [f32; 3] {
    const TINTS: [[f32; 3]; 6] = [[0.95, 0.35, 0.35], [0.35, 0.85, 0.95], [0.95, 0.55, 0.95], [0.95, 0.95, 0.40], [0.55, 0.60, 1.0], [1.0, 0.65, 0.30]];
    TINTS[q % TINTS.len()]
}

/// Rebuilds the legend's quarters: each outline colour with its character.
fn refresh_quarter_legend(mut commands: Commands, forge: Res<LevelForge>, fonts: Res<UiFonts>, legend: Query<(Entity, Option<&Children>), With<QuarterLegend>>) {
    let Ok((entity, children)) = legend.single() else { return };
    for child in children.into_iter().flatten() {
        commands.entity(*child).despawn();
    }
    let f = finishes(&forge.level, &forge.params);
    commands.entity(entity).with_children(|column| {
        let past = forge.level.past.map_or("none".to_string(), |p| p.name().to_string());
        column.spawn((Text::new(format!("past: {past}")), UiFonts::style(&fonts.body, 15.0, MUTED)));
        for purpose in fb_level::past::Purpose::ALL {
            let rooms = forge.level.rooms_for(purpose).count();
            if rooms == 0 {
                continue;
            }
            let tint = purpose_tint(purpose);
            column.spawn(Node { align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|row| {
                row.spawn((Node { width: px(14), height: px(14), ..default() }, BackgroundColor(Color::srgb(tint[0], tint[1], tint[2]))));
                row.spawn((Text::new(format!("{} x {rooms}", purpose.name())), UiFonts::style(&fonts.body, 15.0, MUTED)));
            });
        }
        column.spawn((Text::new(format!("quarters, leaning {}", f.lean().name())), UiFonts::style(&fonts.body, 15.0, MUTED)));
        for (q, quarter) in f.quarters().iter().enumerate() {
            let tint = quarter_tint(q);
            let door = match quarter.door {
                fb_level::DoorStyle::Timber => "timber doors",
                fb_level::DoorStyle::RoundArch => "round arches",
                fb_level::DoorStyle::PointedArch => "pointed arches",
                fb_level::DoorStyle::RockCut => "rock-cut arches",
            };
            column.spawn(Node { align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|row| {
                row.spawn((Node { width: px(14), height: px(4), ..default() }, BackgroundColor(Color::srgb(tint[0], tint[1], tint[2]))));
                row.spawn((Text::new(format!("{}, {} rooms, {door}", quarter.character.name(), quarter.rooms.len())), UiFonts::style(&fonts.body, 15.0, MUTED)));
            });
        }
    });
}

/// The legend's rows: a swatch colour and what it shows.
fn legend_rows() -> [([f32; 3], &'static str); 7] {
    [
        (floor_tint(FloorFinish::Rock), "bare rock floor"),
        (floor_tint(FloorFinish::Flagstone), "flagstone floor"),
        (floor_tint(FloorFinish::Cobble), "cobble floor"),
        (MOSS_TINT, "moss"),
        (MARK_INK, "ring rock   lines strata"),
        (MARK_INK, "square ashlar   dots rubble"),
        (MARK_INK, "walls are marked at each room's middle"),
    ]
}

fn refresh_header(forge: Res<LevelForge>, mut readouts: Query<(&Readout, &mut Text)>) {
    for (readout, mut text) in &mut readouts {
        let line = match readout {
            Readout::Seed => forge.seed.to_string(),
            Readout::Depth => format!("{} \u{b7} {}", forge.depth, match forge.level.style {
                LevelStyle::RoughRock => "rough rock",
                LevelStyle::StoneHalls => "stone halls",
            }),
            Readout::Summary => forge.summary(),
        };
        if text.0 != line {
            text.0 = line;
        }
    }
}

/// The view that fits all of `level` in a stage of `size`, a little in from its edges.
pub fn fitted(level: &Level, size: Vec2) -> MapView {
    let extent = Vec2::new(level.width as f32, level.height as f32) + Vec2::splat(2.0 * MARGIN);
    let scale = ((size - Vec2::splat(40.0)) / extent).min_element().clamp(LEVEL_ZOOM.0, LEVEL_ZOOM.1);
    MapView { centre: Vec2::new(level.width as f32, level.height as f32) * 0.5, scale }
}

/// The stage's size in its own logical pixels, and how many of them to a window pixel (they
/// differ when the view is drawn to an image of another size, as in a capture).
fn stage_size(window: &Window, stage: &ComputedNode) -> (Vec2, f32) {
    let size = stage.size * stage.inverse_scale_factor;
    let per_window = (size.y / window.height().max(1.0)).max(1e-3);
    (size, per_window)
}

/// Scrolling over the plan zooms about the mouse; dragging it pans.
#[allow(clippy::too_many_arguments)]
fn zoom_and_pan(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    body: Query<(&ComputedNode, &UiGlobalTransform), With<PanelBody>>,
    stages: Query<&ComputedNode, With<Stage>>,
    mut plan: ResMut<PlanView>,
    mut dragging: Local<bool>,
) {
    let (Some(window), Some(stage)) = (window, stages.iter().next()) else { return };
    let Some(mut view) = plan.view else { return };
    let (size, per_window) = stage_size(&window, stage);
    let cursor = window.cursor_position().map(|c| c * per_window);
    let over_stage = cursor.is_some_and(|c| c.x < size.x) && !over_body(&window, &body);
    if buttons.just_pressed(MouseButton::Left) {
        *dragging = over_stage;
    }
    if !buttons.pressed(MouseButton::Left) {
        *dragging = false;
    }
    if *dragging && motion.delta != Vec2::ZERO {
        view.pan(motion.delta * per_window);
    }
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    if lines != 0.0
        && over_stage
        && let Some(at) = cursor
    {
        view.zoom_within(1.12f32.powf(lines), at, size, LEVEL_ZOOM);
    }
    plan.view = Some(view);
}

/// Lays the plan where the view puts it, fitting it to the stage for each new seed.
fn place_plan(
    forge: Res<LevelForge>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    stages: Query<&ComputedNode, With<Stage>>,
    mut plan: ResMut<PlanView>,
    mut nodes: Query<&mut Node, With<PlanNode>>,
) {
    let (Some(window), Some(stage)) = (window, stages.iter().next()) else { return };
    let (size, _) = stage_size(&window, stage);
    if size.x <= 1.0 {
        return;
    }
    if plan.fitted != Some(forge.framing) || plan.view.is_none() {
        plan.view = Some(fitted(&forge.level, size));
        plan.fitted = Some(forge.framing);
    }
    let Some(view) = plan.view else { return };
    let (corner, extent) = view.plan_rect(&forge.level, size);
    for mut node in &mut nodes {
        node.left = px(corner.x);
        node.top = px(corner.y);
        node.width = px(extent.x);
        node.height = px(extent.y);
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
        let forge = LevelForge::from_args(args("--level --seed 7")).unwrap();
        assert_eq!(forge.seed, 7);
        assert_eq!(forge.level, fb_level::generate(7, 1), "the defaults lay out the game's level");
        assert_eq!(LevelForge::from_args(args("--level")).unwrap().seed, DEFAULT_SEED);
        assert!(LevelForge::from_args(args("--seed x")).is_err());
        assert!(LevelForge::from_args(args("--wat")).is_err());
    }

    #[test]
    fn a_slider_lays_the_level_out_again() {
        let mut forge = LevelForge::new(3, LevelParams::default());
        forge.set_slider("grid.width", 80.0);
        assert_eq!(forge.level.width, 80);
        assert_eq!(forge.level, generate_with(3, 1, &forge.params), "the same seed and parameters, the same level");
        assert_eq!(Tweak::slider(&forge, "grid.width"), Some(80.0));
        forge.reset();
        assert_eq!(forge.level, fb_level::generate(3, 1), "reset is the game's level again");
    }

    #[test]
    fn depth_picks_the_styles_own_parameters() {
        let forge = LevelForge::from_args(args("--level --seed 7 --depth 2")).unwrap();
        assert_eq!((forge.depth, forge.level.style), (2, LevelStyle::StoneHalls));
        assert_eq!(forge.params, LevelParams::for_style(LevelStyle::StoneHalls));
        assert_eq!(forge.level, fb_level::generate(7, 2));
        assert!(LevelForge::from_args(args("--depth 0")).is_err());
        let mut forge = LevelForge::new(7, LevelParams::default());
        forge.set_depth(2);
        assert_eq!(forge.params, LevelParams::for_style(LevelStyle::StoneHalls));
        forge.set_slider("grid.width", 80.0);
        forge.reset();
        assert_eq!(forge.level, fb_level::generate(7, 2), "reset at an even depth is the stone halls again");
        forge.set_depth(1);
        assert_eq!(forge.level, fb_level::generate(7, 1));
    }

    #[test]
    fn a_new_seed_reframes_and_the_summary_counts_the_level() {
        let mut forge = LevelForge::default();
        let framing = forge.framing;
        forge.set_seed(9);
        assert_ne!(forge.framing, framing);
        let summary = forge.summary();
        assert!(summary.starts_with(&format!("{} rooms", forge.level.rooms.len())), "{summary}");
    }

    #[test]
    fn the_whole_level_fits_the_stage() {
        let level = fb_level::generate(1207, 1);
        let size = Vec2::new(1490.0, 1080.0);
        let view = fitted(&level, size);
        let (corner, extent) = view.plan_rect(&level, size);
        assert!(corner.x >= 0.0 && corner.y >= 0.0 && corner.x + extent.x <= size.x + 1e-3 && corner.y + extent.y <= size.y + 1e-3, "{corner} {extent}");
        assert!(plan_px(&level) > 10.0);
    }
}
