//! The map screen (M): the level as far as it has been seen, drawn as an inked plan
//! ([`super::plan`]) on the ledger's dark page, with the delver's marker showing where they
//! stand and which way they face. It opens centred on the delver; scrolling zooms about the
//! mouse and dragging pans. Like the inventory, it holds the body still while it is open.

use bevy::{
    asset::RenderAssetUsages,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::PrimaryWindow,
};
use fb_level::{CELL, Level};

use super::explore::{ExploredLevels, to_grid};
use super::plan::{MARGIN, PlanCanvas, draw_plan};
use crate::carry::ViewModelCamera;
use crate::play::{CurrentLevel, DelveSystems, PlayerCamera, Screen, ScreenOpen};
use crate::ui::{BG, MUTED, UiFonts};

/// Pixels to a cell the plan is drawn at; the screen scales the picture from there.
pub const PLAN_PX: f32 = 24.0;
/// How near and far the map zooms, in screen pixels to a cell, and where it opens. Our own.
pub const ZOOM: (f32, f32) = (8.0, 72.0);
pub const OPENING_ZOOM: f32 = 40.0;
/// The marker's size on screen, pixels.
pub const MARKER_PX: f32 = 22.0;
/// The marker's size as a share of a cell, once cells are big enough for it. Our own.
pub const MARKER_CELLS: f32 = 0.75;

pub(super) fn add(app: &mut App) {
    if let Some(frame) = std::env::var("FB_OPEN_MAP").ok().and_then(|f| f.trim().parse::<u32>().ok()) {
        app.add_systems(Update, open_for_capture(frame).in_set(DelveSystems));
    }
    app.init_resource::<MapView>()
        .add_systems(Startup, make_marker)
        .add_systems(Update, (build_screen, (zoom_and_pan, redraw_plan, place_plan).chain().after(build_screen)).in_set(DelveSystems));
}

/// For captures: `FB_OPEN_MAP=<frame>` opens the map that many frames into the delve, once the
/// eye has had time to look round.
fn open_for_capture(frame: u32) -> impl FnMut(ResMut<ScreenOpen>, Local<u32>) {
    move |mut open, mut frames| {
        *frames += 1;
        if *frames == frame {
            open.0 = Some(Screen::Map);
        }
    }
}

/// What part of the plan the screen shows: the cell at the middle of the window, and how many
/// screen pixels a cell takes.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct MapView {
    pub centre: Vec2,
    pub scale: f32,
}

impl Default for MapView {
    fn default() -> Self {
        MapView { centre: Vec2::ZERO, scale: OPENING_ZOOM }
    }
}

impl MapView {
    /// Where grid point `cell` lands in a window of `size`, in logical pixels.
    pub fn to_screen(&self, cell: Vec2, size: Vec2) -> Vec2 {
        size * 0.5 + (cell - self.centre) * self.scale
    }

    /// The grid point under screen point `at` in a window of `size`.
    pub fn to_cell(&self, at: Vec2, size: Vec2) -> Vec2 {
        self.centre + (at - size * 0.5) / self.scale
    }

    /// Zooms by `factor` about screen point `at`, keeping the cell under it where it is, within
    /// the map's [`ZOOM`].
    pub fn zoom_about(&mut self, factor: f32, at: Vec2, size: Vec2) {
        self.zoom_within(factor, at, size, ZOOM);
    }

    /// As [`MapView::zoom_about`], within `(nearest, furthest)` pixels to a cell.
    pub fn zoom_within(&mut self, factor: f32, at: Vec2, size: Vec2, range: (f32, f32)) {
        let under = self.to_cell(at, size);
        self.scale = (self.scale * factor).clamp(range.0, range.1);
        self.centre = under - (at - size * 0.5) / self.scale;
    }

    /// Drags the plan by `delta` screen pixels.
    pub fn pan(&mut self, delta: Vec2) {
        self.centre -= delta / self.scale;
    }

    /// The plan picture's rectangle on screen for `level`: its top left corner and size.
    pub fn plan_rect(&self, level: &Level, size: Vec2) -> (Vec2, Vec2) {
        let corner = self.to_screen(Vec2::splat(-MARGIN), size);
        let extent = (Vec2::new(level.width as f32, level.height as f32) + Vec2::splat(2.0 * MARGIN)) * self.scale;
        (corner, extent)
    }
}

/// The turn of a marker drawn pointing north (up the screen) to point along `forward` on the
/// grid, clockwise on screen.
pub fn facing_turn(forward: Vec2) -> f32 {
    forward.x.atan2(-forward.y)
}

/// The map screen's root.
#[derive(Component, Debug)]
#[require(DespawnOnExit<crate::launch::AppState> = DespawnOnExit(crate::launch::AppState::Delving))]
pub struct MapScreen;

/// The plan picture on the screen.
#[derive(Component, Debug)]
pub struct PlanNode;

/// The delver's marker on the plan.
#[derive(Component, Debug)]
pub struct Marker;

/// The plan's picture, and what it was last drawn from: the level's seed and how many cells
/// had been seen.
#[derive(Resource, Debug)]
pub struct PlanImage {
    pub image: Handle<Image>,
    pub drawn: Option<(u32, usize)>,
}

/// The marker's picture: an arrowhead pointing up.
#[derive(Resource, Debug)]
pub struct MarkerImage(pub Handle<Image>);

fn image_of(canvas: &PlanCanvas) -> Image {
    Image::new(
        Extent3d { width: canvas.width, height: canvas.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        canvas.to_rgba8(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
}

/// The marker: an amber arrowhead pointing up, a notch in its tail.
pub fn marker_canvas(size: u32) -> PlanCanvas {
    let mut canvas = PlanCanvas::new(size, size);
    let s = size as f32;
    let (tip, left, right, notch) = ([s * 0.5, s * 0.08], [s * 0.18, s * 0.9], [s * 0.82, s * 0.9], [s * 0.5, s * 0.68]);
    // Filled by stacking lines across it, then outlined in the page's dark.
    for k in 0..=24 {
        let t = k as f32 / 24.0;
        let a = [left[0] + (notch[0] - left[0]) * t, left[1] + (notch[1] - left[1]) * t];
        let b = [right[0] + (notch[0] - right[0]) * t, right[1] + (notch[1] - right[1]) * t];
        canvas.line(tip, a, s * 0.06, [0.886, 0.631, 0.329], 1.0);
        canvas.line(tip, b, s * 0.06, [0.886, 0.631, 0.329], 1.0);
    }
    for (a, b) in [(tip, left), (left, notch), (notch, right), (right, tip)] {
        canvas.line(a, b, s * 0.05, [0.078, 0.071, 0.063], 0.9);
    }
    canvas
}

fn make_marker(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let marker = images.add(image_of(&marker_canvas(64)));
    let plan = images.add(image_of(&PlanCanvas::new(1, 1)));
    commands.insert_resource(MarkerImage(marker));
    commands.insert_resource(PlanImage { image: plan, drawn: None });
}

/// Builds the screen when the map opens, centred on the delver, and takes it down when it
/// closes.
#[allow(clippy::too_many_arguments)]
fn build_screen(
    mut commands: Commands,
    open: Res<ScreenOpen>,
    screens: Query<Entity, With<MapScreen>>,
    cameras: Query<Entity, With<ViewModelCamera>>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    fonts: Res<UiFonts>,
    plan: Option<Res<PlanImage>>,
    marker: Option<Res<MarkerImage>>,
    mut view: ResMut<MapView>,
) {
    let existing = screens.iter().next();
    if !open.is(Screen::Map) {
        if let Some(screen) = existing {
            commands.entity(screen).despawn();
        }
        return;
    }
    let (Some(plan), Some(marker)) = (plan, marker) else { return };
    if existing.is_some() {
        return;
    }
    let Some(camera) = cameras.iter().next() else { return };
    if let Some(eye) = eyes.iter().next() {
        let (x, y) = to_grid(eye.translation());
        *view = MapView { centre: Vec2::new(x, y), scale: OPENING_ZOOM };
    }
    commands
        .spawn((
            Name::new("Map"),
            MapScreen,
            UiTargetCamera(camera),
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), overflow: Overflow::clip(), ..default() },
            BackgroundColor(BG),
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((PlanNode, ImageNode::new(plan.image.clone()), Node { position_type: PositionType::Absolute, ..default() })).with_child((
                Marker,
                ImageNode::new(marker.0.clone()),
                Node { position_type: PositionType::Absolute, width: px(MARKER_PX), height: px(MARKER_PX), ..default() },
                UiTransform::IDENTITY,
            ));
            root.spawn((
                Text::new("Map"),
                UiFonts::style(&fonts.display, 30.0, crate::ui::INK),
                Node { position_type: PositionType::Absolute, left: px(28), top: px(18), ..default() },
            ));
            root.spawn((
                Text::new("Scroll to zoom \u{b7} drag to pan \u{b7} M or Esc to close"),
                UiFonts::style(&fonts.body, 15.0, MUTED),
                Node { position_type: PositionType::Absolute, left: px(28), bottom: px(18), ..default() },
            ));
        });
}

/// Scrolling zooms about the mouse; dragging with the left button pans.
fn zoom_and_pan(
    open: Res<ScreenOpen>,
    scroll: Res<AccumulatedMouseScroll>,
    motion: Res<AccumulatedMouseMotion>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    screens: Query<&ComputedNode, With<MapScreen>>,
    mut view: ResMut<MapView>,
) {
    if !open.is(Screen::Map) {
        return;
    }
    let (Some(window), Some(size)) = (windows.iter().next(), screens.iter().next().map(screen_size)) else { return };
    // The window's mouse, carried into the screen's own pixels (the same, unless the screen is
    // drawn to an image of another size, as in a capture).
    let to_screen = size / window.size().max(Vec2::ONE);
    if scroll.delta.y != 0.0 {
        let lines = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        let at = window.cursor_position().map_or(size * 0.5, |c| c * to_screen);
        view.zoom_about(1.12f32.powf(lines), at, size);
    }
    if buttons.pressed(MouseButton::Left) && motion.delta != Vec2::ZERO {
        view.pan(motion.delta * to_screen);
    }
}

/// The map screen's size in its own logical pixels.
fn screen_size(node: &ComputedNode) -> Vec2 {
    node.size * node.inverse_scale_factor
}

/// Draws the plan again when more of the level has been seen since it was last drawn.
fn redraw_plan(open: Res<ScreenOpen>, level: Option<Res<CurrentLevel>>, explored: Res<ExploredLevels>, plan: Option<ResMut<PlanImage>>, mut images: ResMut<Assets<Image>>) {
    if !open.is(Screen::Map) {
        return;
    }
    let (Some(level), Some(mut plan)) = (level, plan) else { return };
    let empty = fb_level::sight::Explored::new(&level.0);
    let seen = explored.get(&level.0).unwrap_or(&empty);
    let key = (level.0.seed, seen.count());
    if plan.drawn == Some(key) {
        return;
    }
    plan.drawn = Some(key);
    let _ = images.insert(&plan.image, image_of(&draw_plan(&level.0, seen, PLAN_PX)));
}

/// Lays the plan and the marker where the view puts them.
#[allow(clippy::type_complexity)]
fn place_plan(
    open: Res<ScreenOpen>,
    view: Res<MapView>,
    level: Option<Res<CurrentLevel>>,
    screens: Query<&ComputedNode, With<MapScreen>>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    mut plans: Query<&mut Node, (With<PlanNode>, Without<Marker>)>,
    mut markers: Query<(&mut Node, &mut UiTransform), With<Marker>>,
) {
    if !open.is(Screen::Map) {
        return;
    }
    let (Some(level), Some(size)) = (level, screens.iter().next().map(screen_size)) else { return };
    if size.x <= 0.0 {
        return;
    }
    let (corner, extent) = view.plan_rect(&level.0, size);
    for mut node in &mut plans {
        node.left = px(corner.x);
        node.top = px(corner.y);
        node.width = px(extent.x);
        node.height = px(extent.y);
    }
    let Some(eye) = eyes.iter().next() else { return };
    let (x, y) = to_grid(eye.translation());
    let size = (view.scale * MARKER_CELLS).max(MARKER_PX);
    let at = (Vec2::new(x, y) + Vec2::splat(MARGIN)) * view.scale - Vec2::splat(size * 0.5);
    let forward = eye.forward().as_vec3();
    for (mut node, mut turn) in &mut markers {
        node.left = px(at.x);
        node.top = px(at.y);
        node.width = px(size);
        node.height = px(size);
        *turn =UiTransform::from_rotation(Rot2::radians(facing_turn(Vec2::new(forward.x, forward.z))));
    }
}

/// The cell the delver stands in, for tests and the marker: the camera's point over the grid.
pub fn player_cell(at: Vec3) -> (i32, i32) {
    ((at.x / CELL).floor() as i32, (at.z / CELL).floor() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_opens_centred_and_zooms_about_the_mouse() {
        let size = Vec2::new(1600.0, 900.0);
        let mut view = MapView { centre: Vec2::new(10.0, 12.0), scale: OPENING_ZOOM };
        assert_eq!(view.to_screen(Vec2::new(10.0, 12.0), size), size * 0.5, "the delver's cell in the middle");
        let at = Vec2::new(1200.0, 300.0);
        let under = view.to_cell(at, size);
        view.zoom_about(1.5, at, size);
        assert!(view.to_cell(at, size).abs_diff_eq(under, 1e-4), "the cell under the mouse stays put");
        assert!((view.scale - OPENING_ZOOM * 1.5).abs() < 1e-4);
        view.zoom_about(100.0, at, size);
        assert_eq!(view.scale, ZOOM.1, "zoom is bounded");
    }

    #[test]
    fn dragging_moves_the_plan_with_the_mouse() {
        let size = Vec2::new(800.0, 600.0);
        let mut view = MapView { centre: Vec2::new(5.0, 5.0), scale: 20.0 };
        let before = view.to_screen(Vec2::new(7.0, 3.0), size);
        view.pan(Vec2::new(40.0, -20.0));
        assert!(view.to_screen(Vec2::new(7.0, 3.0), size).abs_diff_eq(before + Vec2::new(40.0, -20.0), 1e-4));
    }

    #[test]
    fn the_marker_points_the_way_the_delver_faces() {
        use std::f32::consts::{FRAC_PI_2, PI};
        assert!(facing_turn(Vec2::new(0.0, -1.0)).abs() < 1e-6, "north: up");
        assert!((facing_turn(Vec2::new(1.0, 0.0)) - FRAC_PI_2).abs() < 1e-6, "east: a quarter turn clockwise");
        assert!((facing_turn(Vec2::new(0.0, 1.0)).abs() - PI).abs() < 1e-6, "south: down");
    }

    #[test]
    fn the_plan_covers_the_level_and_its_margin() {
        let level = fb_level::generate(1207, 1);
        let view = MapView { centre: Vec2::new(level.width as f32 * 0.5, level.height as f32 * 0.5), scale: 10.0 };
        let size = Vec2::new(1000.0, 800.0);
        let (corner, extent) = view.plan_rect(&level, size);
        assert!(corner.abs_diff_eq(view.to_screen(Vec2::splat(-MARGIN), size), 1e-4));
        assert!((extent.x - (level.width as f32 + 2.0 * MARGIN) * 10.0).abs() < 1e-3);
    }

    /// An app with the map screen's systems, the level for 1207 and the eye at `at` looking
    /// along `look`, with the map open on a screen 1600 by 900.
    fn open_map(at: Vec3, look: Vec3) -> (App, Level, Entity) {
        let level = fb_level::generate(1207, 1);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .init_resource::<UiFonts>()
            .init_resource::<MapView>()
            .init_resource::<ExploredLevels>()
            .insert_resource(CurrentLevel(level.clone()))
            .insert_resource(ScreenOpen(Some(Screen::Map)))
            .add_systems(Startup, make_marker)
            .add_systems(Update, (build_screen, redraw_plan, place_plan).chain());
        app.world_mut().spawn(ViewModelCamera { follows: Entity::PLACEHOLDER });
        let eye = app.world_mut().spawn((PlayerCamera, GlobalTransform::from(Transform::from_translation(at).looking_to(look, Vec3::Y)))).id();
        app.update();
        // The screen is laid out at 1600 by 900, as the UI would lay it out.
        let mut screens = app.world_mut().query_filtered::<Entity, With<MapScreen>>();
        let screen = screens.iter(app.world()).next().expect("the map is built");
        app.world_mut().entity_mut(screen).insert(ComputedNode { size: Vec2::new(1600.0, 900.0), inverse_scale_factor: 1.0, ..default() });
        app.update();
        (app, level, eye)
    }

    /// The cell under the marker's middle, and its turn.
    fn marker(app: &mut App) -> (Vec2, f32) {
        let view = *app.world().resource::<MapView>();
        let mut markers = app.world_mut().query_filtered::<(&Node, &UiTransform), With<Marker>>();
        let (node, turn) = markers.single(app.world()).map(|(n, t)| (n.clone(), *t)).expect("one marker");
        let (Val::Px(left), Val::Px(top), Val::Px(size)) = (node.left, node.top, node.width) else { panic!("placed in pixels") };
        let middle = Vec2::new(left, top) + Vec2::splat(size * 0.5);
        (middle / view.scale - Vec2::splat(MARGIN), turn.rotation.as_radians())
    }

    #[test]
    fn the_marker_follows_the_delver_while_the_map_is_open() {
        let (sx, sy, inward) = fb_level::generate(1207, 1).start();
        let (dx, dy) = inward.step();
        let cell_at = |k: f32| Vec3::new((sx as f32 + 0.5 + dx as f32 * k) * CELL, 15.0, (sy as f32 + 0.5 + dy as f32 * k) * CELL);
        let ahead = Vec3::new(dx as f32, 0.0, dy as f32);
        let (mut app, _, eye) = open_map(cell_at(2.0), ahead);
        let (cell, turn) = marker(&mut app);
        assert!(cell.abs_diff_eq(Vec2::new(sx as f32 + 0.5 + dx as f32 * 2.0, sy as f32 + 0.5 + dy as f32 * 2.0), 1e-3), "on the delver's cell: {cell}");
        assert!((turn - facing_turn(Vec2::new(dx as f32, dy as f32))).abs() < 1e-4, "facing in");
        // Moved two cells on and turned round, the marker goes with them.
        *app.world_mut().get_mut::<GlobalTransform>(eye).unwrap() = GlobalTransform::from(Transform::from_translation(cell_at(4.0)).looking_to(-ahead, Vec3::Y));
        app.update();
        let (cell, turn) = marker(&mut app);
        assert!(cell.abs_diff_eq(Vec2::new(sx as f32 + 0.5 + dx as f32 * 4.0, sy as f32 + 0.5 + dy as f32 * 4.0), 1e-3), "{cell}");
        assert!((Rot2::radians(turn) * Rot2::radians(-facing_turn(Vec2::new(-dx as f32, -dy as f32)))).as_radians().abs() < 1e-4, "facing out");
    }

    #[test]
    fn the_plan_shows_everything_seen_and_grows_while_open() {
        let level = fb_level::generate(1207, 1);
        let (mut app, _, _) = open_map(Vec3::ZERO, Vec3::NEG_Z);
        let drawn = |app: &App| app.world().resource::<PlanImage>().drawn;
        assert_eq!(drawn(&app), Some((level.seed, 0)), "nothing seen yet");
        // Seen while the map is open: drawn at once.
        let room = level.rooms[0];
        let (cx, cy) = (room.x as f32 + room.w as f32 * 0.5, room.y as f32 + room.h as f32 * 0.5);
        let eye = fb_level::sight::Eye { x: cx, y: cy, forward: (1.0, 0.0), half_fov: 3.2, range: 30.0 };
        let lamp = [fb_level::sight::Lamp { x: cx, y: cy, reach: 20.0 }];
        let seen = app.world_mut().resource_mut::<ExploredLevels>().of(&level).look(&level, &eye, &lamp);
        assert!(seen > 4);
        app.update();
        assert_eq!(drawn(&app), Some((level.seed, seen)));
        let plan = app.world().resource::<PlanImage>().image.clone();
        let image = app.world().resource::<Assets<Image>>().get(&plan).unwrap();
        let frame = super::super::plan::PlanFrame::of(PLAN_PX);
        assert_eq!((image.width(), image.height()), frame.size(&level));
        // Closed and opened again, everything seen so far is there.
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        app.update();
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Map);
        app.update();
        assert_eq!(drawn(&app), Some((level.seed, seen)));
    }

    #[test]
    fn the_marker_is_drawn() {
        let marker = marker_canvas(64);
        assert!(marker.alpha(32, 40) > 0.9, "filled");
        assert_eq!(marker.alpha(2, 2), 0.0, "clear round it");
    }
}
