//! The inventory screen: Tab opens a parchment ledger over the level, which keeps drawing
//! behind it, frees the mouse and stops the body. Each carried container shows as its grid,
//! with every item in it drawn as an icon at its footprint; items are dragged between cells
//! and grids, and R or the right mouse button turns the one being dragged (the plate's
//! `rotateDrag`). The layout is the plate's inventory leaf (`renderGrids`,
//! `reference/forever-below-plate.html`).
//!
//! - [`grid`]: cells and the rules for placing things in them, free of Bevy.
//! - [`items`]: the [`Inventory`], where every item is: worn, in a grid, or loose.
//! - [`icon`]: icons drawn from the items' Armory meshes.
//!
//! The delver starts with a belt whose pouch is their first grid (`docs/items.md`, "Slots";
//! the plate's belt is a container). `FB_OPEN_INVENTORY=1` opens the screen at the start, for
//! captures; `FB_DEBUG_KIT=1` starts the delver with a pack full of things.

pub mod grid;
pub mod icon;
pub mod items;

use bevy::{
    app::HierarchyPropagatePlugin,
    camera::visibility::RenderLayers,
    light::NotShadowCaster,
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_items::{Kind, Rarity, Slot, footprint::Footprint};

pub use grid::{Grid, Placement};
pub use icon::{IconCache, Icons};
pub use items::{Inventory, ItemId, Location, Refusal};

use crate::carry::ViewModelCamera;
use crate::play::{CurrentLevel, ScreenOpen};
use crate::player::set_grab;
use crate::ui::{ACCENT, BAD, BG, CELL, INK, LINE, MUTED, OK, PAPER, UiFonts, rarity_color};

/// Opens and closes the screen.
pub const OPEN_KEY: KeyCode = KeyCode::Tab;
/// Turns the item being dragged a quarter turn.
pub const TURN_KEY: KeyCode = KeyCode::KeyR;
/// Logical pixels from one grid line to the next (the plate's `--cs`).
pub const CELL_PX: f32 = 46.0;
/// Width of a grid line, in logical pixels.
const LINE_PX: f32 = 2.0;
/// How far the mouse moves with the button down before a press becomes a drag, in logical
/// pixels (the plate's 6).
const DRAG_START_PX: f32 = 6.0;
/// How long a refusal stays on the screen, in seconds (the plate's `toast`, 3.6 s).
const TOAST_SECONDS: f32 = 3.6;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<HierarchyPropagatePlugin<RenderLayers>>() {
            app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(Update));
        }
        if !app.is_plugin_added::<HierarchyPropagatePlugin<NotShadowCaster>>() {
            app.add_plugins(HierarchyPropagatePlugin::<NotShadowCaster>::new(Update));
        }
        app.init_resource::<Inventory>()
            .init_resource::<IconCache>()
            .init_resource::<UiFonts>()
            .init_resource::<ScreenOpen>()
            .init_resource::<Dragging>()
            .init_resource::<Hovered>()
            .init_resource::<Toast>()
            .add_systems(Startup, (give_starting_kit, give_debug_kit.run_if(debug_kit_wanted)).chain())
            .add_systems(PostStartup, open_for_capture.run_if(|| env_flag("FB_OPEN_INVENTORY")))
            .add_systems(Update, ((toggle_screen, build_screen, drag_items, show_drag, show_toast).chain(), icon::draw_icons));
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty() && v != "0")
}

fn debug_kit_wanted() -> bool {
    env_flag("FB_DEBUG_KIT")
}

/// A seed for the starting kit, from the level's seed so a delve always starts the same.
fn kit_seed(level: Option<&CurrentLevel>, salt: u32) -> u32 {
    level.map_or(1, |l| l.0.seed).wrapping_mul(0x2545_f491).wrapping_add(salt)
}

/// The delver starts wearing a belt, its pouch their first grid.
fn give_starting_kit(mut inventory: ResMut<Inventory>, level: Option<Res<CurrentLevel>>) {
    if inventory.worn(Slot::Belt).is_some() {
        return;
    }
    let Some(belt) = fb_items::roll_item("Belt", kit_seed(level.as_deref(), 0xbe17)) else { return };
    let belt = inventory.add(belt);
    inventory.wear(belt, Slot::Belt).expect("a belt goes on the belt slot");
}

/// For captures and trying the screen: a pack on the back, filled with one of most things.
fn give_debug_kit(mut inventory: ResMut<Inventory>, level: Option<Res<CurrentLevel>>) {
    let seed = |salt| kit_seed(level.as_deref(), salt);
    if let Some(pack) = fb_items::roll_item("Pack", seed(1)) {
        let pack = inventory.add(pack);
        let _ = inventory.wear(pack, Slot::Back);
    }
    let legendary = (1..4_000).filter_map(|s| fb_items::roll_item("Dagger", s)).find(|i| i.rarity == Rarity::Legendary);
    let things = ["Sword", "Healing draught", "Bread", "Ring", "Lantern", "Apple", "Lockpicks", "Gloves", "Candle", "Amulet", "Cheese"];
    for item in legendary.into_iter().chain(things.iter().enumerate().filter_map(|(i, base)| fb_items::roll_item(base, seed(10 + i as u32)))) {
        let id = inventory.add(item);
        if inventory.stow(id).is_err() {
            inventory.forget(id);
        }
    }
}

fn open_for_capture(mut open: ResMut<ScreenOpen>, cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>) {
    open.0 = true;
    if let Some(mut cursor) = cursor {
        set_grab(&mut cursor, false);
    }
}

/// The whole screen, over the level.
#[derive(Component, Debug)]
pub struct InventoryScreen;

/// The ledger page. Beyond it lies the level.
#[derive(Component, Debug)]
pub struct Ledger;

/// A grid on the screen, and whose it is.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridView {
    /// A container's own grid.
    Container(ItemId),
}

/// An item drawn in a grid.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemView {
    pub id: ItemId,
}

/// The icon inside an [`ItemView`].
#[derive(Component, Debug)]
struct ItemIcon;

/// The item following the mouse while it is dragged.
#[derive(Component, Debug)]
struct DragGhost;

/// The cells a dragged item would cover where it is now, green if it fits and red if not.
#[derive(Component, Debug)]
struct DropHint;

#[derive(Component, Debug)]
struct ToastText;

/// Where a dragged item would go if let go now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A cell of a grid; the item's top-left cell goes where the grab puts it.
    Cell { view: GridView, cell: IVec2 },
    /// The page, off every grid: nothing happens.
    Page,
    /// Off the page, over the level.
    Level,
}

/// An item being dragged, or pressed and about to be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub id: ItemId,
    /// The cell of the item under the mouse when it was taken, from its top-left cell.
    pub grab: IVec2,
    /// Its footprint as it lies now.
    pub size: Footprint,
    pub turned: bool,
    /// Where the button went down, in logical pixels.
    pub press: Vec2,
    /// Whether the mouse has moved far enough for this to be a drag.
    pub active: bool,
    /// Where it would go now, and whether it may.
    pub target: Option<(Target, Result<(), Refusal>)>,
}

impl Drag {
    /// Turns it a quarter turn about the cell it was grabbed by.
    pub fn turn(&mut self) {
        self.size = Footprint { cols: self.size.rows, rows: self.size.cols };
        self.turned = !self.turned;
        self.grab = IVec2::new(self.grab.y.min(self.size.cols as i32 - 1), self.grab.x.min(self.size.rows as i32 - 1));
    }

    /// Where its top-left cell lands with the grabbed cell over `cell`.
    pub fn placement_at(&self, cell: IVec2) -> Option<Placement> {
        let at = cell - self.grab;
        (at.x >= 0 && at.y >= 0 && at.x < 256 && at.y < 256).then(|| Placement::new(at.x as u8, at.y as u8, self.turned))
    }
}

#[derive(Resource, Debug, Default)]
pub struct Dragging(pub Option<Drag>);

/// The item under the mouse, and its rectangle on screen in logical pixels.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Hovered(pub Option<(ItemId, Rect)>);

/// A line saying why a drop was refused, for a few seconds.
#[derive(Resource, Debug, Default)]
pub struct Toast {
    pub text: String,
    pub until: f32,
}

/// What became of a drop, beyond the inventory itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    /// Moved within the delver's keeping.
    Moved,
    /// Nothing happened.
    Nothing,
}

/// Whether `drag` may be let go at `target`.
pub fn check_drop(inventory: &Inventory, drag: &Drag, target: Target) -> Result<(), Refusal> {
    match target {
        Target::Cell { view: GridView::Container(container), cell } => {
            let at = drag.placement_at(cell).ok_or(Refusal::NoRoom)?;
            inventory.check_put(drag.id, container, at)
        }
        Target::Page | Target::Level => Ok(()),
    }
}

/// Lets `drag` go at `target`.
pub fn drop_at(inventory: &mut Inventory, drag: &Drag, target: Target) -> Result<Outcome, Refusal> {
    match target {
        Target::Cell { view: GridView::Container(container), cell } => {
            let at = drag.placement_at(cell).ok_or(Refusal::NoRoom)?;
            inventory.put(drag.id, container, at).map(|()| Outcome::Moved)
        }
        Target::Page | Target::Level => Ok(Outcome::Nothing),
    }
}

/// A node's rectangle in logical pixels.
pub fn node_rect(node: &ComputedNode, transform: &UiGlobalTransform) -> Rect {
    Rect::from_center_size(transform.translation * node.inverse_scale_factor, node.size * node.inverse_scale_factor)
}

/// The grid cell under `point`, for a grid drawn at `rect`.
pub fn cell_under(rect: Rect, point: Vec2) -> IVec2 {
    let (x, y) = grid::cell_at((point.x - rect.min.x - LINE_PX * 0.5, point.y - rect.min.y - LINE_PX * 0.5), CELL_PX);
    IVec2::new(x, y)
}

/// Tab opens and closes the screen; Esc closes it. Open, the mouse is free and the body stops.
fn toggle_screen(
    keys: Res<ButtonInput<KeyCode>>,
    mut open: ResMut<ScreenOpen>,
    mut dragging: ResMut<Dragging>,
    cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
) {
    let toggle = keys.just_pressed(OPEN_KEY) || (open.0 && keys.just_pressed(KeyCode::Escape));
    if !toggle {
        return;
    }
    open.0 = !open.0;
    dragging.0 = None;
    if let Some(mut cursor) = cursor {
        set_grab(&mut cursor, !open.0);
    }
}

/// Builds the screen when it opens and rebuilds it whenever what is in it changes; takes it
/// down when it closes.
fn build_screen(
    mut commands: Commands,
    open: Res<ScreenOpen>,
    inventory: Res<Inventory>,
    screens: Query<Entity, With<InventoryScreen>>,
    cameras: Query<Entity, With<ViewModelCamera>>,
    fonts: Res<UiFonts>,
    mut icons: Icons,
) {
    let existing = screens.iter().next();
    if !open.0 {
        if let Some(screen) = existing {
            commands.entity(screen).despawn();
        }
        return;
    }
    if existing.is_some() && !inventory.is_changed() {
        return;
    }
    // Drawn by the camera that draws over the held torch, as the HUD is.
    let Some(camera) = cameras.iter().next() else { return };
    if let Some(screen) = existing {
        commands.entity(screen).despawn();
    }
    let screen = Screen { inventory: &inventory, fonts: &fonts };
    commands
        .spawn((
            Name::new("Inventory"),
            InventoryScreen,
            UiTargetCamera(camera),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            // The plate's `.scrim`: ink at 38% over what lies behind.
            BackgroundColor(INK.with_alpha(0.38)),
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((
                Ledger,
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: px(28),
                    padding: UiRect::all(px(22)),
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(14)),
                    max_width: percent(94),
                    max_height: percent(92),
                    ..default()
                },
                BackgroundColor(BG),
                BorderColor::all(LINE),
                BoxShadow::new(Color::srgba(0.04, 0.02, 0.01, 0.4), px(0), px(24), px(0), px(60)),
            ))
            .with_children(|ledger| {
                ledger
                    .spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(12), min_width: px(420), ..default() })
                    .with_children(|leaf| {
                        leaf.spawn(Node {
                            column_gap: px(16),
                            align_items: AlignItems::Baseline,
                            border: UiRect::bottom(px(1)),
                            padding: UiRect::bottom(px(6)),
                            ..default()
                        })
                        .insert(BorderColor::all(crate::ui::FAINT))
                        .with_children(|bar| {
                            bar.spawn((Text::new("inventory"), UiFonts::style(&fonts.display, 26.0, INK)));
                            bar.spawn((ToastText, Text::default(), UiFonts::style(&fonts.body, 15.0, ACCENT)));
                        });
                        leaf.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(18), row_gap: px(16), align_items: AlignItems::FlexStart, ..default() })
                            .with_children(|row| {
                                let carried = screen.inventory.carried_containers();
                                if carried.is_empty() {
                                    row.spawn((
                                        Text::new("Nothing to carry things in. Find a pack, a sack or a belt with a pouch."),
                                        UiFonts::style(&fonts.body, 14.0, MUTED),
                                    ));
                                }
                                for container in carried {
                                    screen.container(row, &mut icons, container, GridView::Container(container), "");
                                }
                            });
                        leaf.spawn((
                            Text::new("Drag things between your bags. Press R while dragging to turn an item. Tab or Esc to close."),
                            UiFonts::style(&fonts.body, 13.0, MUTED),
                        ));
                    });
            });
            root.spawn((
                DragGhost,
                Node { position_type: PositionType::Absolute, ..default() },
                ImageNode::default(),
                Visibility::Hidden,
                GlobalZIndex(30),
            ));
            root.spawn((
                DropHint,
                Node { position_type: PositionType::Absolute, border: UiRect::all(px(2)), border_radius: BorderRadius::all(px(6)), ..default() },
                Visibility::Hidden,
                GlobalZIndex(29),
            ));
        });
}

/// What the screen is built from.
struct Screen<'a> {
    inventory: &'a Inventory,
    fonts: &'a UiFonts,
}

impl Screen<'_> {
    /// A container's grid under its name, with a small picture of it.
    fn container(&self, parent: &mut ChildSpawnerCommands, icons: &mut Icons, id: ItemId, view: GridView, suffix: &str) {
        let (Some(item), Some(grid)) = (self.inventory.item(id), self.inventory.grid(id)) else { return };
        let label = item.profile.holds.as_ref().map_or_else(|| item.name.clone(), |h| h.label.clone());
        parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }).with_children(|column| {
            column.spawn(Node { align_items: AlignItems::Center, column_gap: px(8), height: px(36), ..default() }).with_children(|head| {
                let icon = icons.of(item, false);
                let size = icon::icon_size(item, false).as_vec2();
                let scale = 34.0 / size.max_element();
                head.spawn((Node { width: px(size.x * scale), height: px(size.y * scale), ..default() }, ImageNode::new(icon)));
                head.spawn((Text::new(format!("{label}{suffix}")), UiFonts::style(&self.fonts.display, 17.0, MUTED)));
            });
            self.grid(column, icons, view, grid, false);
        });
    }

    /// A grid of cells with its items drawn at their footprints. `faded` draws the cells paler,
    /// as for what lies underfoot.
    fn grid(&self, parent: &mut ChildSpawnerCommands, icons: &mut Icons, view: GridView, grid: &Grid<ItemId>, faded: bool) {
        let (cols, rows) = (grid.cols as f32, grid.rows as f32);
        parent
            .spawn((
                view,
                Node {
                    width: px(cols * CELL_PX + LINE_PX),
                    height: px(rows * CELL_PX + LINE_PX),
                    border_radius: BorderRadius::all(px(12)),
                    ..default()
                },
                BackgroundColor(LINE),
            ))
            .with_children(|cells| {
                let cell_colour = if faded { CELL.mix(&PAPER, 0.5) } else { CELL };
                for y in 0..grid.rows {
                    for x in 0..grid.cols {
                        let round = |at_x: bool, at_y: bool| CornerRadius::from(if at_x && at_y { px(10) } else { px(0) });
                        let (left, top, right, bottom) = (x == 0, y == 0, x + 1 == grid.cols, y + 1 == grid.rows);
                        cells.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(x as f32 * CELL_PX + LINE_PX),
                                top: px(y as f32 * CELL_PX + LINE_PX),
                                width: px(CELL_PX - LINE_PX),
                                height: px(CELL_PX - LINE_PX),
                                border_radius: BorderRadius {
                                    top_left: round(left, top),
                                    top_right: round(right, top),
                                    bottom_left: round(left, bottom),
                                    bottom_right: round(right, bottom),
                                },
                                ..default()
                            },
                            BackgroundColor(cell_colour),
                        ));
                    }
                }
                for entry in grid.entries() {
                    let Some(item) = self.inventory.item(entry.id) else { continue };
                    cells
                        .spawn((
                            ItemView { id: entry.id },
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(entry.at.x as f32 * CELL_PX + LINE_PX),
                                top: px(entry.at.y as f32 * CELL_PX + LINE_PX),
                                width: px(entry.size.cols as f32 * CELL_PX - LINE_PX),
                                height: px(entry.size.rows as f32 * CELL_PX - LINE_PX),
                                padding: UiRect::all(px(3)),
                                border_radius: BorderRadius::all(px(6)),
                                ..default()
                            },
                            BackgroundColor(Color::NONE),
                        ))
                        .with_children(|slot| {
                            slot.spawn((ItemIcon, Node { width: percent(100), height: percent(100), ..default() }, ImageNode::new(icons.of(item, entry.at.turned))));
                            if item.profile.kind == Kind::Gear && item.rarity != Rarity::Common {
                                slot.spawn((
                                    Node {
                                        position_type: PositionType::Absolute,
                                        left: px(4),
                                        top: px(4),
                                        width: px(7),
                                        height: px(7),
                                        border_radius: BorderRadius::MAX,
                                        ..default()
                                    },
                                    BackgroundColor(rarity_color(item.rarity)),
                                ));
                            }
                        });
                }
            });
    }
}

/// The grid under `point` and its cell, the page, or the level beyond it.
fn target_under(point: Vec2, grids: &Query<(&GridView, &ComputedNode, &UiGlobalTransform)>, ledger: Option<Rect>) -> Target {
    for (view, node, transform) in grids {
        let rect = node_rect(node, transform);
        if rect.contains(point) {
            return Target::Cell { view: *view, cell: cell_under(rect, point) };
        }
    }
    match ledger {
        Some(page) if !page.contains(point) => Target::Level,
        _ => Target::Page,
    }
}

/// The grid a view shows.
fn grid_of(inventory: &Inventory, view: GridView) -> Option<&Grid<ItemId>> {
    match view {
        GridView::Container(id) => inventory.grid(id),
    }
}

/// Presses, drags, turns and drops items, and notes which one the mouse is over.
#[allow(clippy::too_many_arguments)]
fn drag_items(
    open: Res<ScreenOpen>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    grids: Query<(&GridView, &ComputedNode, &UiGlobalTransform)>,
    ledgers: Query<(&ComputedNode, &UiGlobalTransform), With<Ledger>>,
    mut dragging: ResMut<Dragging>,
    mut hovered: ResMut<Hovered>,
    mut inventory: ResMut<Inventory>,
    mut toast: ResMut<Toast>,
    time: Res<Time<Real>>,
) {
    let cursor = window.and_then(|w| w.cursor_position());
    if !open.0 {
        dragging.0 = None;
        return;
    }
    let Some(point) = cursor else { return };
    let ledger = ledgers.iter().next().map(|(n, t)| node_rect(n, t));
    let target = target_under(point, &grids, ledger);

    // What the mouse is over, when nothing is being dragged.
    let over = match target {
        Target::Cell { view, cell } => grid_of(&inventory, view).and_then(|g| g.at_cell(cell.x, cell.y).copied()).map(|e| (view, e)),
        _ => None,
    };
    let over_rect = over.and_then(|(view, entry)| {
        grids.iter().find(|(v, ..)| **v == view).map(|(_, node, transform)| {
            let grid = node_rect(node, transform);
            let min = grid.min + Vec2::new(entry.at.x as f32, entry.at.y as f32) * CELL_PX + LINE_PX;
            Rect::from_corners(min, min + Vec2::new(entry.size.cols as f32, entry.size.rows as f32) * CELL_PX - LINE_PX)
        })
    });
    let now_over = if dragging.0.is_some_and(|d| d.active) { None } else { over.zip(over_rect).map(|((_, e), r)| (e.id, r)) };
    hovered.set_if_neq(Hovered(now_over));

    if mouse.just_pressed(MouseButton::Left)
        && let (Some((_, entry)), Target::Cell { cell, .. }) = (over, target)
    {
        dragging.0 = Some(Drag {
            id: entry.id,
            grab: cell - IVec2::new(entry.at.x as i32, entry.at.y as i32),
            size: entry.size,
            turned: entry.at.turned,
            press: point,
            active: false,
            target: None,
        });
    }
    let Some(drag) = dragging.0.as_mut() else { return };
    if !drag.active && point.distance(drag.press) >= DRAG_START_PX {
        drag.active = true;
    }
    if drag.active && (keys.just_pressed(TURN_KEY) || mouse.just_pressed(MouseButton::Right)) {
        drag.turn();
    }
    drag.target = drag.active.then(|| (target, check_drop(&inventory, drag, target)));
    if mouse.just_released(MouseButton::Left) {
        let drag = dragging.0.take().expect("a drag");
        if !drag.active {
            return;
        }
        if let Err(refusal) = drop_at(&mut inventory, &drag, target) {
            toast.text = refusal.reason().to_string();
            toast.until = time.elapsed_secs() + TOAST_SECONDS;
        }
    }
}

/// Draws the drag: the item following the mouse, the cells it would cover, and its old place
/// faded; and lightens the item the mouse is over.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn show_drag(
    dragging: Res<Dragging>,
    hovered: Res<Hovered>,
    inventory: Res<Inventory>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    grids: Query<(&GridView, &ComputedNode, &UiGlobalTransform)>,
    mut ghosts: Query<(&mut Node, &mut ImageNode, &mut Visibility), (With<DragGhost>, Without<DropHint>)>,
    mut hints: Query<(&mut Node, &mut BackgroundColor, &mut BorderColor, &mut Visibility), (With<DropHint>, Without<DragGhost>)>,
    mut views: Query<(&ItemView, &mut BackgroundColor, &Children), Without<DropHint>>,
    mut item_icons: Query<&mut ImageNode, (With<ItemIcon>, Without<DragGhost>)>,
    mut icons: Icons,
) {
    let drag = dragging.0.filter(|d| d.active);
    let cursor = window.and_then(|w| w.cursor_position());
    for (mut node, mut image, mut visibility) in &mut ghosts {
        let (Some(drag), Some(point), Some(item)) = (drag, cursor, drag.and_then(|d| inventory.item(d.id))) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let handle = icons.of(item, drag.turned);
        if image.image != handle {
            image.image = handle;
        }
        let at = point - (drag.grab.as_vec2() + 0.5) * CELL_PX;
        node.left = px(at.x);
        node.top = px(at.y);
        node.width = px(drag.size.cols as f32 * CELL_PX);
        node.height = px(drag.size.rows as f32 * CELL_PX);
        *visibility = Visibility::Inherited;
    }
    for (mut node, mut background, mut border, mut visibility) in &mut hints {
        let shown = drag.and_then(|d| d.target.map(|t| (d, t))).and_then(|(d, (target, fits))| match target {
            Target::Cell { view, cell } => grids.iter().find(|(v, ..)| **v == view).map(|(_, n, t)| (d, node_rect(n, t), cell, fits)),
            _ => None,
        });
        let Some((drag, rect, cell, fits)) = shown else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let at = (cell - drag.grab).max(IVec2::ZERO).as_vec2();
        let min = rect.min + at * CELL_PX + LINE_PX;
        node.left = px(min.x);
        node.top = px(min.y);
        node.width = px(drag.size.cols as f32 * CELL_PX - LINE_PX);
        node.height = px(drag.size.rows as f32 * CELL_PX - LINE_PX);
        let colour = if fits.is_ok() { OK } else { BAD };
        background.0 = colour.with_alpha(if fits.is_ok() { 0.3 } else { 0.26 });
        *border = BorderColor::all(colour);
        *visibility = Visibility::Inherited;
    }
    for (view, mut background, children) in &mut views {
        let dragged = drag.is_some_and(|d| d.id == view.id);
        let over = hovered.0.is_some_and(|(id, _)| id == view.id);
        let wanted = if over { PAPER.with_alpha(0.45) } else { Color::NONE };
        if background.0 != wanted {
            background.0 = wanted;
        }
        for child in children {
            if let Ok(mut image) = item_icons.get_mut(*child) {
                let alpha = if dragged { 0.3 } else { 1.0 };
                if image.color.alpha() != alpha {
                    image.color = Color::WHITE.with_alpha(alpha);
                }
            }
        }
    }
}

fn show_toast(toast: Res<Toast>, time: Res<Time<Real>>, mut texts: Query<&mut Text, With<ToastText>>) {
    let shown = if time.elapsed_secs() < toast.until { toast.text.as_str() } else { "" };
    for mut text in &mut texts {
        if text.0 != shown {
            text.0 = shown.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dagger() -> fb_items::Item {
        (1..500).filter_map(|s| fb_items::roll_item("Dagger", s)).find(|d| d.profile.footprint == Footprint { cols: 1, rows: 2 }).unwrap()
    }

    #[test]
    fn a_drag_turns_about_its_grabbed_cell() {
        let mut drag = Drag {
            id: ItemId(0),
            grab: IVec2::new(0, 2),
            size: Footprint { cols: 1, rows: 3 },
            turned: false,
            press: Vec2::ZERO,
            active: true,
            target: None,
        };
        drag.turn();
        assert_eq!((drag.size, drag.turned, drag.grab), (Footprint { cols: 3, rows: 1 }, true, IVec2::new(2, 0)));
        assert_eq!(drag.placement_at(IVec2::new(4, 1)), Some(Placement::new(2, 1, true)));
        assert_eq!(drag.placement_at(IVec2::new(1, 1)), None);
    }

    #[test]
    fn dropping_moves_between_grids_and_refuses_overlaps() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.wear(pack, Slot::Back).unwrap();
        let belt = inventory.add(fb_items::roll_item("Belt", 3).unwrap());
        inventory.wear(belt, Slot::Belt).unwrap();
        let (a, b) = (inventory.add(dagger()), inventory.add(dagger()));
        inventory.put(a, pack, Placement::new(0, 0, false)).unwrap();
        inventory.put(b, pack, Placement::new(3, 0, false)).unwrap();
        let drag = |id| Drag { id, grab: IVec2::new(0, 1), size: Footprint { cols: 1, rows: 2 }, turned: false, press: Vec2::ZERO, active: true, target: None };
        // Grabbed by its lower cell and let go over the belt pouch's cell (2, 1).
        let into_belt = Target::Cell { view: GridView::Container(belt), cell: IVec2::new(2, 1) };
        assert_eq!(drop_at(&mut inventory, &drag(a), into_belt), Ok(Outcome::Moved));
        assert_eq!(inventory.location(a), Some(Location::In { container: belt, at: Placement::new(2, 0, false) }));
        // Onto the other dagger: refused, and it stays where it was.
        let onto_b = Target::Cell { view: GridView::Container(pack), cell: IVec2::new(3, 1) };
        assert_eq!(check_drop(&inventory, &drag(a), onto_b), Err(Refusal::NoRoom));
        assert_eq!(drop_at(&mut inventory, &drag(a), onto_b), Err(Refusal::NoRoom));
        assert_eq!(inventory.location(a), Some(Location::In { container: belt, at: Placement::new(2, 0, false) }));
        // The pack cannot go into itself.
        let pack_drag = Drag { id: pack, grab: IVec2::ZERO, size: Footprint { cols: 2, rows: 3 }, turned: false, press: Vec2::ZERO, active: true, target: None };
        let into_itself = Target::Cell { view: GridView::Container(pack), cell: IVec2::new(5, 0) };
        assert_eq!(drop_at(&mut inventory, &pack_drag, into_itself), Err(Refusal::IntoItself));
        assert_eq!(drop_at(&mut inventory, &drag(b), Target::Page), Ok(Outcome::Nothing));
    }

    #[test]
    fn cells_are_read_from_the_grid_rectangle() {
        let rect = Rect::new(100.0, 50.0, 100.0 + 8.0 * CELL_PX + LINE_PX, 50.0 + 5.0 * CELL_PX + LINE_PX);
        assert_eq!(cell_under(rect, Vec2::new(103.0, 53.0)), IVec2::new(0, 0));
        assert_eq!(cell_under(rect, Vec2::new(100.0 + 7.5 * CELL_PX, 50.0 + 4.5 * CELL_PX)), IVec2::new(7, 4));
        assert_eq!(cell_under(rect, Vec2::new(100.0 + 2.0 * CELL_PX + 5.0, 60.0)), IVec2::new(2, 0));
    }

    /// The screen opens and closes over an app with the inventory plugin's state and systems,
    /// but no window, renderer or assets.
    fn screen_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Inventory>()
            .init_resource::<IconCache>()
            .init_resource::<UiFonts>()
            .init_resource::<ScreenOpen>()
            .init_resource::<Dragging>()
            .init_resource::<Hovered>()
            .init_resource::<Toast>()
            .init_resource::<Assets<Image>>()
            .add_systems(Update, (toggle_screen, build_screen, drag_items).chain());
        app.world_mut().spawn(ViewModelCamera { follows: Entity::PLACEHOLDER });
        app
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        // As Bevy's input system does at the start of the next frame.
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
        app.update();
    }

    #[test]
    fn the_layout_survives_closing_and_reopening() {
        let mut app = screen_app();
        let (pack, knife) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
            inventory.wear(pack, Slot::Back).unwrap();
            let knife = inventory.add(dagger());
            inventory.put(knife, pack, Placement::new(5, 2, true)).unwrap();
            (pack, knife)
        };
        let shown = |app: &mut App| {
            let mut views = app.world_mut().query::<(&ItemView, &Node)>();
            views.iter(app.world()).map(|(v, n)| (v.id, n.left, n.top, n.width)).collect::<Vec<_>>()
        };
        press(&mut app, OPEN_KEY);
        assert!(app.world().resource::<ScreenOpen>().0);
        let first = shown(&mut app);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0], (knife, px(5.0 * CELL_PX + LINE_PX), px(2.0 * CELL_PX + LINE_PX), px(2.0 * CELL_PX - LINE_PX)));
        press(&mut app, OPEN_KEY);
        assert!(!app.world().resource::<ScreenOpen>().0);
        assert!(shown(&mut app).is_empty());
        press(&mut app, OPEN_KEY);
        assert_eq!(shown(&mut app), first);
        assert_eq!(app.world().resource::<Inventory>().location(knife), Some(Location::In { container: pack, at: Placement::new(5, 2, true) }));
    }
}
