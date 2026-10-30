//! Bags and containers opened as windows on the inventory screen. Double-clicking a bag the
//! delver carries (the pack on the back, the satchel, the belt pouch, a sack inside one) opens
//! its grid as a window on the right, and double-clicking it again, or its cross,
//! closes it. A bag set down underfoot or standing in the level opens on a single click. Windows
//! lay out in columns, never over the doll or each other; with no room for another
//! the oldest is left out ([`column_layout`]). Each grid is a drop target by its laid-out rectangle
//! ([`super::target_under`]), so items go between windows.
//!
//! Every inventory screen is the same three regions ([`regions`]). The left is the world side: the
//! floor underfoot alone, with a trader's card over the wares, a body, or a container in front of
//! it, and any bag of theirs opened from them, laid out by [`fit_sizes`]. The middle is the
//! delver's header, doll, quickslots and help, at one size and place. The right is the delver's own
//! windows, laid out in columns the same way. Opening a barrel or starting a barter fills the left
//! only.
//!
//! Every bag the delver wears opens as a window the first time it is carried ([`BagWindows::seen`]),
//! at the start of a delve and when one is put on later; after that what the delver closes stays
//! closed. Their windows are dragged by their title bar ([`drag_windows`]) anywhere on the right
//! region ([`drag_area`]). A dragged window comes out of the columns and is remembered where it was left
//! ([`BagWindows::moved`]); the ones not dragged still lay out in the columns, and may sit under a
//! dragged one, which is raised to the front when taken.
//!
//! The screen is rebuilt whenever the inventory changes, so what is open and where it sits is
//! kept in [`BagWindows`], which lives for the delve: closing the inventory and opening it again
//! shows the same windows. Positions are in the interface's logical pixels (the same as
//! [`crate::ui::node_rect`], so they scale with the window).

use std::collections::HashMap;

use bevy::{prelude::*, window::PrimaryWindow};

use super::{Dragging, Inventory, ItemId, Screen, ScreenOpen, grid_size};
use crate::play::SheetTab;
use crate::keymap::{Action, ActionInput};
use crate::tuning::Tuning;
use crate::ui::{node_rect, ui_cursor};

// A window's make, logical pixels.
/// Height of a window's title bar.
pub(super) const TITLE_PX: f32 = 36.0;
/// Space between a window's border and what it holds.
pub(super) const PAD_PX: f32 = 10.0;
/// A window's border.
pub(super) const BORDER_PX: f32 = 2.0;
/// Space between the title bar and the grid.
pub(super) const GAP_PX: f32 = 4.0;
/// The narrowest a window's contents are, so the name and the cross fit over a small pouch.
const MIN_CONTENT_PX: f32 = 200.0;

// Where the windows go, logical pixels.
/// Space between one window and the next, in a column and between columns.
const WINDOW_GAP_PX: f32 = 12.0;
/// Space kept between the doll and the first column, and between the last column and the screen's edge.
const AREA_MARGIN_PX: f32 = 16.0;

// The two-sided screen, logical pixels.
/// The window standing for the floor underfoot on the left side, which has no item of its own.
pub const FLOOR: ItemId = ItemId(u32::MAX);
/// The width of the world side on the left, which holds the floor, a trader's card over the wares, a body
/// or a container, whichever the screen is for.
pub const LEFT_PX: f32 = 580.0;
/// The width of the delver's column (header, doll, quickslots and help) in the middle: the doll's frame.
pub const MIDDLE_PX: f32 = super::doll::FRAME.x;
/// How tall the trader's card with its Accept and Cancel is, over the wares.
const BARTER_PANEL_PX: f32 = 300.0;
/// The narrowest the trader's wares are, so the card's lines fit over a small shelf.
const SHELF_MIN_CONTENT_PX: f32 = 340.0;

/// Which containers are open as windows, and where each sits.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct BagWindows {
    /// Everything opened and not closed, oldest first: the remembered set.
    pub order: Vec<ItemId>,
    /// The bags the delver carries that have been opened once already, on their own as they were
    /// first carried ([`place_windows`]); one in `order` is open, one not is shut and stays so until
    /// the delver opens it. A bag put away and taken up again is new.
    pub seen: Vec<ItemId>,
    /// The bags set down underfoot whose windows the delver shut, which stay shut while the bag lies
    /// there (one set down shows on the left by itself); a click on the bag opens it again.
    pub shut: Vec<ItemId>,
    /// Those of `order` the screen had no room for at the last layout, which come back by themselves
    /// when room frees (another window closes, or a smaller one is in the way).
    pub hidden: Vec<ItemId>,
    /// How much the windows' cells are shrunk from the screen's, 1 down to what lets them all fit
    /// ([`fit_windows`]).
    pub scale: f32,
    /// The windows of the world side on the left, in the order they were laid out: the trader's shelf,
    /// container or body, the floor ([`FLOOR`]), bags set down and the other party's own bags.
    pub left: Vec<ItemId>,
    /// [`BagWindows::scale`] for the left half.
    pub left_scale: f32,
    /// Each shown window's top left, in the interface's logical pixels, as [`place_windows`] last laid it out.
    pub at: HashMap<ItemId, Vec2>,
    /// Where the delver left each window they dragged, kept for the delve whether the window is open or
    /// not, so it opens there again. Pulled back inside [`drag_area`] when laid out, never edited by it.
    pub moved: HashMap<ItemId, Vec2>,
    /// The window being dragged now, which the screen draws at the mouse until it is let go.
    pub grab: Option<WindowGrab>,
    /// A click on a bag lying underfoot or in the level that has not yet been told from the first
    /// half of a double-click (which takes it up): the bag and when it was clicked. It opens or
    /// closes its window once the interval is over ([`settle_click`]).
    pub pending: Option<(ItemId, f32)>,
}

impl Default for BagWindows {
    fn default() -> Self {
        BagWindows { order: Vec::new(), seen: Vec::new(), shut: Vec::new(), hidden: Vec::new(), scale: 1.0, left: Vec::new(), left_scale: 1.0, at: HashMap::new(), moved: HashMap::new(), grab: None, pending: None }
    }
}

impl BagWindows {
    pub fn is_open(&self, id: ItemId) -> bool {
        self.order.contains(&id)
    }

    /// Whether `id` has a window on the screen: laid out on the left, or open and not left out.
    pub fn is_shown(&self, id: ItemId) -> bool {
        self.left.contains(&id) || (self.is_open(id) && !self.hidden.contains(&id))
    }

    /// How much a grid's cells are shrunk to fit its window ([`fit_sizes`]); 1 for one not in a window.
    pub fn cell_scale(&self, view: super::GridView) -> f32 {
        match view {
            super::GridView::Container(id) if self.left.contains(&id) => self.left_scale,
            super::GridView::Container(id) if self.is_shown(id) => self.scale,
            super::GridView::Underfoot if self.left.contains(&FLOOR) => self.left_scale,
            _ => 1.0,
        }
    }

    /// Where the screen draws `id`'s window: at the mouse while it is being dragged, else as laid out.
    pub fn position(&self, id: ItemId) -> Option<Vec2> {
        self.grab.filter(|g| g.id == id).map(|g| g.live).or_else(|| self.at.get(&id).copied())
    }

    /// Lets go of the window being dragged. One moved further than a click is kept where it is, out of the columns.
    pub fn end_grab(&mut self) {
        if let Some(grab) = self.grab.take()
            && grab.live.distance(grab.from) >= WINDOW_CLICK_PX
        {
            self.moved.insert(grab.id, grab.live);
        }
    }

    /// Opens `id` as the newest, or closes it if its window is showing. One with no room on the screen
    /// opens afresh, as the newest.
    pub fn toggle(&mut self, id: ItemId) {
        if self.is_shown(id) {
            self.close(id);
        } else {
            self.order.retain(|&o| o != id);
            self.hidden.retain(|&o| o != id);
            self.shut.retain(|&o| o != id);
            self.order.push(id);
        }
    }

    /// Shuts `id`'s window. One on the left (a bag set down) is kept shut, as it would show by itself.
    pub fn close(&mut self, id: ItemId) {
        self.order.retain(|&o| o != id);
        self.hidden.retain(|&o| o != id);
        if id != FLOOR && self.left.contains(&id) && !self.shut.contains(&id) {
            self.shut.push(id);
        }
    }

    /// Brings `id` in front of the others. False if it already was, or is not open.
    pub fn raise(&mut self, id: ItemId) -> bool {
        if self.order.last() == Some(&id) || !self.is_open(id) {
            return false;
        }
        self.close(id);
        self.order.push(id);
        true
    }
}

/// A window taken by its title bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowGrab {
    pub id: ItemId,
    /// Where the window's corner lay when taken.
    pub from: Vec2,
    /// The mouse less the window's corner when taken, so it follows without jumping.
    pub offset: Vec2,
    /// Where the window's corner is now, inside [`drag_area`].
    pub live: Vec2,
    /// Whether the mouse holds it; else a capture drives it ([`drag_for_capture`]).
    pub by_mouse: bool,
}

/// A window on the screen.
#[derive(Component, Debug)]
pub struct BagWindow(pub ItemId);

/// A window's title bar.
#[derive(Component, Debug)]
pub struct BagTitle(pub ItemId);

/// A window's close button.
#[derive(Component, Debug)]
pub struct BagClose(pub ItemId);

/// Height of the label under a fitting's slots ("Lashing", "Potion pouch").
pub(super) const FITTING_LABEL_PX: f32 = 18.0;
/// Space between the main grid and the fitting slots beside or under it, and between one fitting and the next.
pub(super) const FITTING_GAP_PX: f32 = 10.0;
/// Space between the slots of one fitting (the pouch's three).
pub(super) const SLOT_GAP_PX: f32 = 6.0;

/// A fitting's slots, drawn like the doll's: big for a lashing, strap or frog, small for the pouch's three.
pub(super) fn fitting_slot_px(small: bool) -> f32 {
    if small { super::doll::SMALL_SLOT } else { super::doll::BIG_SLOT }
}

/// Where the fitting slots of a pack or belt go and how much room they take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FittingsLayout {
    /// The slots and their labels together, logical pixels.
    pub size: Vec2,
    /// In a column beside the main grid; else in a row under it.
    pub beside: bool,
}

/// How the slots of `host`'s fittings (a pack's lashing and strap, a belt's frogs and potion pouch)
/// sit against its main grid, which is `main` pixels: in a column beside it if that is no taller than
/// the grid, else in a row under it. `None` for a container with none.
pub fn fittings_layout(inventory: &Inventory, host: ItemId, main: Vec2) -> Option<FittingsLayout> {
    let groups: Vec<Vec2> = inventory
        .fittings_of(host)
        .iter()
        .filter_map(|&(accessory, _)| fb_items::fitting_grid(accessory))
        .map(|f| {
            let slot = fitting_slot_px(f.small);
            Vec2::new(f.holds as f32 * slot + (f.holds as f32 - 1.0) * SLOT_GAP_PX, slot + FITTING_LABEL_PX)
        })
        .collect();
    if groups.is_empty() {
        return None;
    }
    let gaps = FITTING_GAP_PX * (groups.len() - 1) as f32;
    let column = Vec2::new(groups.iter().map(|g| g.x).fold(0.0, f32::max), groups.iter().map(|g| g.y).sum::<f32>() + gaps);
    let row = Vec2::new(groups.iter().map(|g| g.x).sum::<f32>() + gaps, groups.iter().map(|g| g.y).fold(0.0, f32::max));
    Some(if column.y <= main.y { FittingsLayout { size: column, beside: true } } else { FittingsLayout { size: row, beside: false } })
}

/// How big a window is for the container `id`, logical pixels: its main grid with the slots of a
/// pack's or belt's fittings beside or under it. `None` for something that holds nothing.
pub fn window_size(inventory: &Inventory, id: ItemId, cell: f32) -> Option<Vec2> {
    let main = grid_size(inventory.grid(id)?, cell);
    let (width, height) = match fittings_layout(inventory, id, main) {
        Some(FittingsLayout { size, beside: true }) => (main.x + FITTING_GAP_PX + size.x, main.y),
        Some(FittingsLayout { size, beside: false }) => (main.x.max(size.x), main.y + FITTING_GAP_PX + size.y),
        None => (main.x, main.y),
    };
    let edge = 2.0 * (PAD_PX + BORDER_PX);
    Some(Vec2::new(width.max(MIN_CONTENT_PX) + edge, height + TITLE_PX + GAP_PX + edge))
}

/// How big the window of the left half's `id` is with cells of `cell` pixels: the floor's grid under its
/// name, the trader's card over the wares (`shelf`), or a container's window.
pub fn party_size(inventory: &Inventory, floor: &super::Underfoot, shelf: Option<ItemId>, id: ItemId, cell: f32) -> Option<Vec2> {
    if id == FLOOR {
        return Some(floor_size(floor, cell));
    }
    if shelf == Some(id) {
        return shelf_size(inventory, id, cell);
    }
    window_size(inventory, id, cell)
}

/// How big the floor's window is: its grid under its name.
pub fn floor_size(floor: &super::Underfoot, cell: f32) -> Vec2 {
    let grid = grid_size(&floor.grid, cell);
    Vec2::new(grid.x.max(MIN_CONTENT_PX) + 2.0 * (PAD_PX + BORDER_PX), grid.y + TITLE_PX + GAP_PX + 2.0 * (PAD_PX + BORDER_PX))
}

/// How big the trader's window is: their card over the wares, which scroll when they are many.
pub fn shelf_size(inventory: &Inventory, shelf: ItemId, cell: f32) -> Option<Vec2> {
    let grid = grid_size(inventory.grid(shelf)?, cell);
    let wares = TITLE_PX + GAP_PX + super::shelf_view_height(grid.y);
    let edge = 2.0 * (PAD_PX + BORDER_PX);
    Some(Vec2::new(shelf_content_width(grid.x) + edge, BARTER_PANEL_PX + 10.0 + wares + edge))
}

/// The width the trader's card is drawn at over wares whose grid is `grid` pixels across.
pub fn shelf_content_width(grid: f32) -> f32 {
    grid.max(SHELF_MIN_CONTENT_PX)
}

/// `at` moved to keep a window of `size` inside a screen of `screen`.
pub fn clamp_window(at: Vec2, size: Vec2, screen: Vec2) -> Vec2 {
    at.clamp(Vec2::ZERO, (screen - size).max(Vec2::ZERO))
}

/// How far a title bar is dragged before it is a move and not a click, logical pixels.
const WINDOW_CLICK_PX: f32 = 3.0;

/// `at` moved to keep a window of `size` inside `area` (at its corner if it is too big for it).
pub fn clamp_into(at: Vec2, size: Vec2, area: Rect) -> Vec2 {
    at.clamp(area.min, (area.max - size).max(area.min))
}

/// Where the delver's windows may be dragged on a screen of `screen` interface pixels: the whole right
/// region, where the window columns start, so a window never covers the doll, quickslots and help
/// whose drops they would swallow, nor the world side.
pub fn drag_area(screen: Vec2) -> Rect {
    Rect::from_corners(Vec2::new((regions(screen).2.min.x - AREA_MARGIN_PX).clamp(0.0, screen.x), 0.0), screen)
}

/// The size of the interface in its logical pixels: that of the view it is drawn in (the held
/// torch's camera, as [`crate::ui::fit_interface`] reads it) under `UiScale`, else the
/// window's. They are one in play; a capture draws to a view of another size than its window.
fn ui_size(window: &Window, scale: Option<&UiScale>, cameras: &Query<&Camera, Or<(With<crate::carry::ViewModelCamera>, With<bevy::ui::IsDefaultUiCamera>)>>) -> Vec2 {
    let view = cameras.iter().filter(|c| c.is_active).find_map(Camera::logical_viewport_size).filter(|v| v.min_element() > 0.0);
    view.unwrap_or_else(|| Vec2::new(window.width(), window.height())) / scale.map_or(1.0, |s| s.0.max(f32::EPSILON))
}

/// Where the windows lie on a screen of `screen` interface pixels, the same for every inventory screen:
/// the left for the world side (the floor, a trader's card over the wares, a body, a container), the
/// middle beside it for the delver's column (header, doll, quickslots, help) and the rest for the
/// delver's windows. Each runs the screen's height less its margins; the middle is no window's area,
/// so it is the column's own rectangle.
pub fn regions(screen: Vec2) -> (Rect, Rect, Rect) {
    let (top, bottom) = (AREA_MARGIN_PX, (screen.y - AREA_MARGIN_PX).max(AREA_MARGIN_PX));
    let left = Rect::from_corners(Vec2::new(AREA_MARGIN_PX, top), Vec2::new(LEFT_PX - AREA_MARGIN_PX, bottom));
    let middle = Rect::from_corners(Vec2::new(LEFT_PX, top), Vec2::new(LEFT_PX + MIDDLE_PX, bottom));
    let right_from = LEFT_PX + MIDDLE_PX + AREA_MARGIN_PX;
    (left, middle, Rect::from_corners(Vec2::new(right_from, top), Vec2::new((screen.x - AREA_MARGIN_PX).max(right_from), bottom)))
}

/// The shares of a cell the windows' grids are drawn at, biggest first: the first that lets every open
/// window fit is used, so a pack, a satchel and a belt with frogs all show on the smallest screen.
const WINDOW_SCALES: [f32; 6] = [1.0, 0.85, 0.75, 0.65, 0.55, 0.45];

/// Lays out the windows `ids` (oldest first) with cells of `cell` pixels in `area`, shrinking the cells
/// through [`WINDOW_SCALES`] until all fit; at the smallest, the oldest are left out as [`column_layout`]
/// says. Gives the scale, how many were left out and where the rest sit.
pub fn fit_windows(inventory: &Inventory, ids: &[ItemId], cell: f32, area: Rect) -> (f32, usize, Vec<(ItemId, Vec2)>) {
    fit_sizes(|id, cell| window_size(inventory, id, cell), ids, 0, cell, area)
}

/// [`fit_windows`] for windows sized by `size_of` (an id and the cell, as [`window_size`] is), the first
/// `keep` of which are never left out.
pub fn fit_sizes(size_of: impl Fn(ItemId, f32) -> Option<Vec2>, ids: &[ItemId], keep: usize, cell: f32, area: Rect) -> (f32, usize, Vec<(ItemId, Vec2)>) {
    let mut result = (1.0, 0, Vec::new());
    for scale in WINDOW_SCALES {
        let sizes: Vec<(ItemId, Vec2)> = ids.iter().filter_map(|&id| Some((id, size_of(id, cell * scale)?))).collect();
        let (skipped, placed) = column_layout_keeping(&sizes, keep, area);
        // A window alone and too big for the area is placed at its corner all the same, which is no fit.
        let over = placed.iter().any(|&(id, at)| sizes.iter().any(|&(s, size)| s == id && (at.x + size.x > area.max.x || at.y + size.y > area.max.y)));
        result = (scale, skipped, placed);
        if skipped == 0 && !over {
            break;
        }
    }
    result
}

/// Lays windows of the given `sizes`, oldest first, in columns inside `area`: each window under the
/// one before it, and a new column to the right when the next would run past the bottom. When they
/// do not all fit the oldest are left out, one at a time, until the rest do; the count left out
/// comes first, then where each of the others sits. The newest alone, too big for the area or
/// not, is always placed, at its corner.
pub fn column_layout(sizes: &[(ItemId, Vec2)], area: Rect) -> (usize, Vec<(ItemId, Vec2)>) {
    column_layout_keeping(sizes, 0, area)
}

/// [`column_layout`] that never leaves out the first `keep` of `sizes`: when they do not all fit, the
/// oldest of the rest are left out first.
pub fn column_layout_keeping(sizes: &[(ItemId, Vec2)], keep: usize, area: Rect) -> (usize, Vec<(ItemId, Vec2)>) {
    let fill = |sizes: &[(ItemId, Vec2)]| {
        let mut placed = Vec::with_capacity(sizes.len());
        let (mut x, mut y, mut wide) = (area.min.x, area.min.y, 0.0f32);
        for &(id, size) in sizes {
            if y > area.min.y && y + size.y > area.max.y {
                x += wide + WINDOW_GAP_PX;
                (y, wide) = (area.min.y, 0.0);
            }
            if x + size.x > area.max.x || y + size.y > area.max.y {
                return None;
            }
            placed.push((id, Vec2::new(x, y)));
            y += size.y + WINDOW_GAP_PX;
            wide = wide.max(size.x);
        }
        Some(placed)
    };
    let keep = keep.min(sizes.len());
    // The newest is always left, when none are kept.
    let droppable = if keep == 0 { sizes.len().saturating_sub(1) } else { sizes.len() - keep };
    for skip in 0..=droppable {
        let rest: Vec<(ItemId, Vec2)> = sizes[..keep].iter().chain(&sizes[keep + skip..]).copied().collect();
        if rest.is_empty() {
            break;
        }
        if let Some(placed) = fill(&rest) {
            return (skip, placed);
        }
    }
    // Nothing fits: the first kept window, else the newest, alone at the area's corner.
    let last = sizes.len().saturating_sub(1);
    let shown = if keep == 0 { last } else { 0 };
    (last, sizes.get(shown).map(|&(id, _)| (id, area.min)).into_iter().collect())
}

/// Whether a double-click on `id` opens its window rather than wearing or taking it off: any
/// container the delver carries, worn or inside a worn one. Wearing and taking off a bag is by dragging.
pub fn opens_by_double_click(inventory: &Inventory, id: ItemId) -> bool {
    inventory.grid(id).is_some() && inventory.is_carried(id)
}

/// The doll, quickslots and help column of the page, which windows keep off.
#[derive(Component, Debug)]
pub struct DollColumn;

/// Whether `id` is within the delver's reach: carried (worn, or inside something worn), or one of
/// the bags set down underfoot, or the container opened in the level, or inside one of those at
/// any depth. A bag that was set down and left, given away, or on the trader's shelf is not.
pub fn reachable(inventory: &Inventory, underfoot: &[ItemId], opened: Option<ItemId>, id: ItemId) -> bool {
    inventory.is_carried(id) || underfoot.iter().chain(opened.iter()).any(|&root| inventory.holds(root, id))
}

/// Opens or closes the window of a bag lying underfoot or in the level, clicked once, when the
/// double-click interval has passed without a second click, so a double-click (take it up) never
/// toggles a window. A bag the delver carries opens on a double-click instead ([`opens_by_double_click`]).
pub fn settle_click(time: Res<Time<Real>>, mut windows: ResMut<BagWindows>, inventory: Res<Inventory>, barter: Res<crate::barter::Barter>) {
    let Some((id, at)) = windows.pending else { return };
    if time.elapsed_secs() - at <= super::DOUBLE_CLICK_SECONDS {
        return;
    }
    windows.bypass_change_detection().pending = None;
    if inventory.grid(id).is_some() && !barter.active() {
        windows.toggle(id);
    }
}

/// Opens every bag the delver newly carries, drops windows whose container is out of reach and, while
/// the inventory page shows them, lays them out: the world side's (the trader's shelf, an opened
/// container or body, the floor, bags set down and their own bags) in the left region and the delver's
/// in the right, each in columns by the same code ([`fit_sizes`]), shrinking cells to fit and, when
/// they still do not all fit, hiding the oldest.
#[allow(clippy::too_many_arguments)]
pub fn place_windows(
    mut windows: ResMut<BagWindows>,
    inventory: Res<Inventory>,
    tuning: Res<Tuning>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    (underfoot, showing, barter, bodies): (Res<super::Underfoot>, Res<super::OpenContainer>, Res<crate::barter::Barter>, Option<Res<crate::remains::NearBodies>>),
    (open, tab): (Res<ScreenOpen>, Res<SheetTab>),
    cameras: Query<&Camera, Or<(With<crate::carry::ViewModelCamera>, With<bevy::ui::IsDefaultUiCamera>)>>,
) {
    let cell = tuning.interface.cell_size;
    // The trader's shelf is not the delver's to reach into.
    let shelf = barter.session.as_ref().map(|s| s.shelf).filter(|&s| showing.0 == Some(s));
    let opened = showing.0.filter(|&c| !barter.session.as_ref().is_some_and(|s| s.shelf == c));
    // A fallen body's things are in reach while the delver stands near the body
    // ([`crate::remains`]), like a bag set down underfoot.
    let near: Vec<ItemId> = bodies.iter().flat_map(|b| b.0.iter().copied()).collect();
    let roots: Vec<ItemId> = underfoot.bags.iter().copied().chain(near.iter().copied()).collect();
    let gone: Vec<ItemId> = windows.order.iter().copied().filter(|&id| inventory.grid(id).is_none() || !reachable(&inventory, &roots, opened, id)).collect();
    for id in gone {
        windows.close(id);
    }
    // Where a window was left is kept only for a bag still in reach.
    let lost: Vec<ItemId> = windows.moved.keys().copied().filter(|&id| inventory.grid(id).is_none() || !reachable(&inventory, &roots, opened, id)).collect();
    for id in lost {
        windows.moved.remove(&id);
    }
    // Every bag the delver wears or carries opens as a window when it is first carried: the first time
    // the inventory opens in a delve, and again for one newly worn or picked up. After that the
    // remembered set rules, so a bag the delver closed stays shut.
    let carried = inventory.carried_containers();
    if windows.seen.iter().any(|id| !carried.contains(id)) {
        windows.seen.retain(|id| carried.contains(id));
    }
    for id in carried {
        if !windows.seen.contains(&id) {
            windows.seen.push(id);
            if !windows.is_open(id) {
                windows.order.push(id);
            }
        }
    }
    // A bag set down that was shut is forgotten once it leaves the floor.
    if windows.shut.iter().any(|id| !underfoot.bags.contains(id)) {
        windows.shut.retain(|id| underfoot.bags.contains(id));
    }
    // Windows are laid out only where they are drawn: the inventory page.
    if !open.is(Screen::Inventory) || *tab != SheetTab::Inventory {
        return;
    }
    let Some(window) = window else { return };
    let screen = ui_size(&window, ui_scale.as_deref(), &cameras);
    if screen.x < 1.0 || screen.y < 1.0 {
        return;
    }
    // The world side: what the screen was opened on, then what of it is open (a body's bag, a bag set
    // down), none of which is the delver's. The floor sits under it; a trader's stall has none.
    let mut left: Vec<ItemId> = showing.0.into_iter().chain(windows.order.iter().copied().filter(|id| near.contains(id))).collect();
    let mut keep = left.len();
    if !barter.active() {
        left.push(FLOOR);
        let set_down: Vec<ItemId> = underfoot.bags.iter().copied().filter(|b| !left.contains(b) && !windows.shut.contains(b)).collect();
        left.extend(set_down);
        keep += 1;
    }
    let theirs: Vec<ItemId> = windows.order.iter().copied().filter(|&id| !inventory.is_carried(id) && !left.contains(&id)).collect();
    left.extend(theirs);
    let mine: Vec<ItemId> = windows.order.iter().copied().filter(|&id| inventory.is_carried(id)).collect();
    let (left_area, _, right_area) = regions(screen);
    let (left_scale, _, mut placed) = fit_sizes(|id, cell| party_size(&inventory, &underfoot, shelf, id, cell), &left, keep, cell, left_area);
    let left: Vec<ItemId> = placed.iter().map(|p| p.0).collect();
    // What has no room is hidden, not closed: it is still remembered and comes back when room frees.
    // The windows the delver dragged are kept where they were left, inside their side, and the rest lay out in columns.
    let (dragged, free): (Vec<ItemId>, Vec<ItemId>) = mine.iter().partition(|id| windows.moved.contains_key(id));
    let (free_scale, _, mine_placed) = fit_windows(&inventory, &free, cell, right_area);
    placed.extend(mine_placed);
    // A dragged window shrinks with the others, and more if it would not fit its side otherwise.
    let area = drag_area(screen);
    let alone = WINDOW_SCALES
        .into_iter()
        .find(|&s| dragged.iter().all(|&id| window_size(&inventory, id, cell * s).is_none_or(|size| size.x <= area.width() && size.y <= area.height())))
        .unwrap_or(WINDOW_SCALES[WINDOW_SCALES.len() - 1]);
    let scale = free_scale.min(alone);
    for &id in &dragged {
        let size = window_size(&inventory, id, cell * scale).unwrap_or_default();
        placed.push((id, clamp_into(windows.moved[&id], size, area)));
    }
    let hidden: Vec<ItemId> = windows.order.iter().copied().filter(|id| !placed.iter().any(|(p, _)| p == id)).collect();
    if windows.hidden != hidden {
        windows.hidden = hidden;
    }
    if windows.left != left {
        windows.left = left;
    }
    if windows.scale != scale {
        windows.scale = scale;
    }
    if windows.left_scale != left_scale {
        windows.left_scale = left_scale;
    }
    for (id, at) in placed {
        if windows.at.get(&id) != Some(&at) {
            windows.at.insert(id, at);
        }
    }
}

/// Drags the delver's own windows by their title bar. A press on a title bar takes its window (the
/// one in front there, not by the cross, not one of the other party's) and raises it; it follows the
/// mouse, kept inside [`drag_area`] at any window size, until let go, and is then remembered where
/// it was left. Overlapping another is allowed: the one dragged last is in front. The window moves
/// as drawn; the screen is only rebuilt when it is taken and let go.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn drag_windows(
    open: Res<ScreenOpen>,
    tab: Res<SheetTab>,
    input: ActionInput,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    dragging: Res<Dragging>,
    inventory: Res<Inventory>,
    mut windows: ResMut<BagWindows>,
    cameras: Query<&Camera, Or<(With<crate::carry::ViewModelCamera>, With<bevy::ui::IsDefaultUiCamera>)>>,
    (titles, closes, frames): (Query<(&BagTitle, &ComputedNode, &UiGlobalTransform)>, Query<(&BagClose, &ComputedNode, &UiGlobalTransform)>, Query<(&BagWindow, &ComputedNode, &UiGlobalTransform, &ZIndex)>),
    mut nodes: Query<(&BagWindow, &mut Node)>,
) {
    if !open.is(Screen::Inventory) || *tab != SheetTab::Inventory {
        if windows.grab.is_some() {
            windows.grab = None;
        }
        return;
    }
    let Some(window) = window else { return };
    let point = ui_cursor(&window, ui_scale.as_deref());
    let screen = ui_size(&window, ui_scale.as_deref(), &cameras);
    if windows.grab.is_none()
        && let Some(point) = point
        && input.just_pressed(Action::DragItem)
        && dragging.0.is_none()
    {
        // The window in front at the mouse must be the one whose title bar it is over.
        let front = frames.iter().filter(|(_, n, t, _)| node_rect(n, t).contains(point)).max_by_key(|(.., z)| z.0).map(|(w, ..)| w.0);
        let title = titles.iter().find(|(_, n, t)| node_rect(n, t).contains(point)).map(|(t, ..)| t.0);
        if let Some(id) = title.filter(|&id| front == Some(id) && inventory.is_carried(id) && windows.is_shown(id) && !windows.left.contains(&id))
            && !closes.iter().any(|(_, n, t)| node_rect(n, t).contains(point))
            && let Some(at) = windows.position(id)
        {
            windows.grab = Some(WindowGrab { id, from: at, offset: point - at, live: at, by_mouse: true });
        }
    }
    let Some(grab) = windows.grab else { return };
    let size = frames.iter().find(|(w, ..)| w.0 == grab.id).map(|(_, n, t, _)| node_rect(n, t).size());
    let mut live = grab.live;
    if grab.by_mouse
        && let Some(point) = point
    {
        live = point - grab.offset;
    }
    if let Some(size) = size {
        live = clamp_into(live, size, drag_area(screen));
    }
    // Moved past a click, it comes to the front; a click leaves the order, and so the columns, alone.
    if live.distance(grab.from) >= WINDOW_CLICK_PX {
        windows.raise(grab.id);
    }
    if live != grab.live {
        windows.bypass_change_detection().grab = Some(WindowGrab { live, ..grab });
    }
    if grab.by_mouse && input.just_released(Action::DragItem) {
        windows.end_grab();
        return;
    }
    for (w, mut node) in &mut nodes {
        if w.0 == grab.id && (node.left != px(live.x) || node.top != px(live.y)) {
            (node.left, node.top) = (px(live.x), px(live.y));
        }
    }
}

/// `FB_DRAG_WINDOW=satchel:dx,dy` drags the carried bag with that word in its name by that offset,
/// in interface pixels, over a second a moment after the inventory opens, then closes the inventory
/// and opens it again to show the window where it was left, for captures.
pub fn drag_for_capture(mut windows: ResMut<BagWindows>, inventory: Res<Inventory>, mut open: ResMut<ScreenOpen>, tab: Res<SheetTab>, mut frames: Local<u32>) {
    const START: u32 = 30;
    const LENGTH: u32 = 60;
    const SHUT: u32 = START + LENGTH + 20;
    const REOPEN: u32 = SHUT + 20;
    let Ok(spec) = std::env::var("FB_DRAG_WINDOW") else { return };
    let Some((name, by)) = spec.split_once(':') else { return };
    let Some(by) = by.split_once(',').and_then(|(x, y)| Some(Vec2::new(x.trim().parse().ok()?, y.trim().parse().ok()?))) else { return };
    let name = name.to_lowercase();
    let Some(id) = inventory.carried_containers().into_iter().find(|&id| inventory.item(id).is_some_and(|i| i.name.to_lowercase().contains(&name))) else { return };
    if *frames > REOPEN {
        return;
    }
    if (SHUT..REOPEN).contains(&*frames) {
        *frames += 1;
        if open.is(Screen::Inventory) {
            open.0 = None;
        }
        return;
    }
    if *frames == REOPEN {
        *frames += 1;
        open.0 = Some(Screen::Inventory);
        return;
    }
    if !open.is(Screen::Inventory) || *tab != SheetTab::Inventory || !windows.at.contains_key(&id) {
        return;
    }
    *frames += 1;
    let Some(at) = windows.position(id) else { return };
    if *frames == START {
        windows.grab = Some(WindowGrab { id, from: at, offset: Vec2::ZERO, live: at, by_mouse: false });
    } else if *frames > START && *frames <= START + LENGTH {
        let Some(grab) = windows.grab.filter(|g| g.id == id) else { return };
        let t = (*frames - START) as f32 / LENGTH as f32;
        let eased = t * t * (3.0 - 2.0 * t);
        windows.bypass_change_detection().grab = Some(WindowGrab { live: grab.from + by * eased, ..grab });
        if *frames == START + LENGTH {
            windows.end_grab();
        }
    }
}

/// Closes a window with its cross; that of a container standing in the level shuts it.
#[allow(clippy::too_many_arguments)]
pub fn close_windows(
    open: Res<ScreenOpen>,
    input: ActionInput,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    dragging: Res<Dragging>,
    mut windows: ResMut<BagWindows>,
    mut showing: ResMut<super::OpenContainer>,
    closes: Query<(&BagClose, &ComputedNode, &UiGlobalTransform)>,
) {
    if !open.is(Screen::Inventory) || !input.just_pressed(Action::DragItem) || dragging.0.is_some_and(|d| d.active) {
        return;
    }
    let Some(window) = window else { return };
    let Some(point) = ui_cursor(&window, ui_scale.as_deref()) else { return };
    if let Some((close, ..)) = closes.iter().find(|(_, n, t)| node_rect(n, t).contains(point)) {
        windows.close(close.0);
        if showing.0 == Some(close.0) {
            showing.0 = None;
        }
    }
}

/// `FB_OPEN_BAGS=1` opens every carried container as a window, for captures.
pub fn open_for_capture(mut windows: ResMut<BagWindows>, inventory: Res<Inventory>, mut done: Local<bool>, mut frames: Local<u32>) {
    if *done {
        return;
    }
    // Let the floor's bags settle into the page first, so the windows are placed around them.
    *frames += 1;
    if *frames < 20 {
        return;
    }
    let carried = inventory.carried_containers();
    if carried.is_empty() {
        return;
    }
    *done = true;
    for id in carried {
        if !windows.is_open(id) {
            windows.order.push(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_kept_inside_the_screen() {
        let screen = Vec2::new(1280.0, 720.0);
        let size = Vec2::new(300.0, 200.0);
        assert_eq!(clamp_window(Vec2::new(-50.0, -9.0), size, screen), Vec2::ZERO);
        assert_eq!(clamp_window(Vec2::new(2000.0, 900.0), size, screen), Vec2::new(980.0, 520.0));
        assert_eq!(clamp_window(Vec2::new(40.0, 60.0), size, screen), Vec2::new(40.0, 60.0));
        assert_eq!(clamp_window(Vec2::new(40.0, 60.0), Vec2::new(2000.0, 2000.0), screen), Vec2::ZERO, "bigger than the screen sits at the corner");
    }

    #[test]
    fn windows_open_in_front_and_close_by_clicking_again() {
        let (a, b) = (ItemId(1), ItemId(2));
        let mut windows = BagWindows::default();
        windows.toggle(a);
        windows.toggle(b);
        assert_eq!(windows.order, vec![a, b]);
        assert!(windows.raise(a));
        assert_eq!(windows.order, vec![b, a]);
        assert!(!windows.raise(a));
        windows.toggle(b);
        assert_eq!(windows.order, vec![a]);
    }

    fn ids(n: u32) -> Vec<ItemId> {
        (1..=n).map(ItemId).collect()
    }

    fn no_overlap(placed: &[(ItemId, Vec2)], sizes: &[(ItemId, Vec2)], area: Rect) {
        let rects: Vec<Rect> = placed.iter().map(|(id, at)| Rect::from_corners(*at, *at + sizes.iter().find(|(s, _)| s == id).unwrap().1)).collect();
        for (n, rect) in rects.iter().enumerate() {
            assert!(rect.min.x >= area.min.x && rect.min.y >= area.min.y && rect.max.x <= area.max.x && rect.max.y <= area.max.y, "#{n} {rect:?} inside {area:?}");
            for other in &rects[..n] {
                assert!(other.intersect(*rect).is_empty(), "{rect:?} overlaps {other:?}");
            }
        }
    }

    #[test]
    fn windows_stack_down_a_column_and_start_the_next_when_it_is_full() {
        let area = Rect::from_corners(Vec2::new(1000.0, 100.0), Vec2::new(1900.0, 900.0));
        let sizes: Vec<(ItemId, Vec2)> = ids(3).into_iter().zip([Vec2::new(312.0, 300.0), Vec2::new(250.0, 400.0), Vec2::new(280.0, 200.0)]).collect();
        let (skipped, placed) = column_layout(&sizes, area);
        assert_eq!(skipped, 0);
        assert_eq!(placed[0].1, Vec2::new(1000.0, 100.0), "the first at the area's corner");
        assert_eq!(placed[1].1, Vec2::new(1000.0, 100.0 + 300.0 + WINDOW_GAP_PX), "the next under it");
        assert_eq!(placed[2].1, Vec2::new(1000.0 + 312.0 + WINDOW_GAP_PX, 100.0), "the third does not fit under, so starts a column");
        no_overlap(&placed, &sizes, area);
    }

    #[test]
    fn the_oldest_window_closes_when_there_is_no_room_for_another() {
        let area = Rect::from_corners(Vec2::new(1000.0, 100.0), Vec2::new(1700.0, 500.0));
        let sizes: Vec<(ItemId, Vec2)> = ids(4).into_iter().map(|id| (id, Vec2::new(300.0, 350.0))).collect();
        // Two columns of one fit.
        let (skipped, placed) = column_layout(&sizes[..2], area);
        assert_eq!((skipped, placed.len()), (0, 2));
        // A third has nowhere to go: the first closes and the others shift into its place.
        let (skipped, placed) = column_layout(&sizes[..3], area);
        assert_eq!(skipped, 1);
        assert_eq!(placed.iter().map(|p| p.0).collect::<Vec<_>>(), vec![ItemId(2), ItemId(3)]);
        no_overlap(&placed, &sizes, area);
        let (skipped, _) = column_layout(&sizes, area);
        assert_eq!(skipped, 2);
    }

    #[test]
    fn a_window_too_big_for_the_area_is_still_shown_alone() {
        let area = Rect::from_corners(Vec2::new(1000.0, 100.0), Vec2::new(1100.0, 200.0));
        let sizes = vec![(ItemId(1), Vec2::new(50.0, 50.0)), (ItemId(2), Vec2::new(500.0, 500.0))];
        assert_eq!(column_layout(&sizes, area), (1, vec![(ItemId(2), area.min)]));
        assert_eq!(column_layout(&[], area), (0, vec![]));
    }

    #[test]
    fn the_area_beside_the_doll_fits_a_pack_a_satchel_and_a_pouch_at_any_window_size() {
        let cell = 72.0;
        let (inventory, pack, inner, _) = bags();
        let sizes: Vec<(ItemId, Vec2)> = [pack, inner, pack].into_iter().filter_map(|id| Some((id, window_size(&inventory, id, cell)?))).collect();
        // The interface is laid out in 1920 by 1080 whatever the window (1280 by 720, 1920 by 1080).
        for screen in [Vec2::new(1920.0, 1080.0), Vec2::new(2560.0, 1080.0), Vec2::new(1920.0, 1440.0)] {
            let area = regions(screen).2;
            assert!(area.min.x > LEFT_PX + MIDDLE_PX, "{screen}: right of the doll");
            let (skipped, placed) = column_layout(&sizes[..2], area);
            assert_eq!((skipped, placed.len()), (0, 2), "{screen}: {sizes:?} in {area:?}");
            no_overlap(&placed, &sizes, area);
        }
    }

    #[test]
    fn only_a_carried_bag_opens_on_a_double_click() {
        let (mut inventory, pack, inner, loose) = bags();
        assert!(opens_by_double_click(&inventory, pack), "a worn pack");
        assert!(opens_by_double_click(&inventory, inner), "a sack in the pack");
        assert!(!opens_by_double_click(&inventory, loose), "a sack lying about is taken up");
        inventory.take_out(pack);
        assert!(!opens_by_double_click(&inventory, pack), "a pack taken off is not carried");
        assert!(!opens_by_double_click(&inventory, inner), "nor is what is inside it");
    }

    #[test]
    fn the_kept_windows_are_never_the_ones_left_out() {
        let area = Rect::from_corners(Vec2::new(0.0, 0.0), Vec2::new(300.0, 400.0));
        let sizes: Vec<(ItemId, Vec2)> = ids(4).into_iter().map(|id| (id, Vec2::new(300.0, 190.0))).collect();
        // Two fit one over the other; the first is the one the others give way to.
        let (skipped, placed) = column_layout_keeping(&sizes, 1, area);
        assert_eq!((skipped, placed.iter().map(|p| p.0).collect::<Vec<_>>()), (2, vec![ItemId(1), ItemId(4)]));
        let (skipped, placed) = column_layout_keeping(&sizes, 2, area);
        assert_eq!((skipped, placed.iter().map(|p| p.0).collect::<Vec<_>>()), (2, vec![ItemId(1), ItemId(2)]));
        // Without keeping any, the oldest go, as ever.
        let (skipped, placed) = column_layout_keeping(&sizes, 0, area);
        assert_eq!((skipped, placed.iter().map(|p| p.0).collect::<Vec<_>>()), (2, vec![ItemId(3), ItemId(4)]));
    }

    #[test]
    fn a_window_alone_that_is_too_big_shrinks_until_it_fits() {
        let (inventory, pack, ..) = bags();
        let area = Rect::from_corners(Vec2::new(0.0, 0.0), Vec2::new(400.0, 600.0));
        let (scale, skipped, _) = fit_windows(&inventory, &[pack], 72.0, area);
        assert!(scale < 1.0 && skipped == 0, "scale {scale}");
        let size = window_size(&inventory, pack, 72.0 * scale).unwrap();
        assert!(size.x <= 400.0 && size.y <= 600.0, "{size:?} fits {area:?}");
    }

    #[test]
    fn the_three_regions_run_left_to_right_the_same_whatever_is_on_the_screen() {
        for screen in [Vec2::new(1920.0, 1080.0), Vec2::new(2560.0, 1080.0)] {
            let (left, middle, right) = regions(screen);
            assert!(left.max.x <= middle.min.x && middle.max.x < right.min.x, "{screen}: {left:?} {middle:?} {right:?}");
            assert_eq!(middle.width(), super::super::doll::FRAME.x, "the middle holds the whole doll");
            assert_eq!((left.min.y, left.max.y), (right.min.y, right.max.y), "both run the screen's height");
            assert!(right.max.x <= screen.x && left.min.x >= 0.0);
        }
        assert_eq!(regions(Vec2::new(1920.0, 1080.0)).2.width(), 1920.0 - LEFT_PX - MIDDLE_PX - 2.0 * AREA_MARGIN_PX);
    }

    #[test]
    fn a_grid_is_shrunk_by_the_side_its_window_is_on() {
        use crate::inventory::GridView;
        let (a, b, c) = (ItemId(1), ItemId(2), ItemId(3));
        let mut windows = BagWindows { order: vec![a, b], left: vec![FLOOR, c], scale: 0.75, left_scale: 0.55, ..default() };
        assert_eq!(windows.cell_scale(GridView::Container(a)), 0.75, "the delver's window");
        assert_eq!(windows.cell_scale(GridView::Container(c)), 0.55, "the other party's");
        assert_eq!(windows.cell_scale(GridView::Underfoot), 0.55, "the floor, in its window");
        assert_eq!(windows.cell_scale(GridView::Container(ItemId(9))), 1.0, "not in a window");
        windows.hidden.push(b);
        assert_eq!(windows.cell_scale(GridView::Container(b)), 1.0, "left out");
        windows.left.clear();
        assert_eq!(windows.cell_scale(GridView::Underfoot), 1.0, "the floor on the page");
    }

    use crate::inventory::{OpenContainer, Placement, Underfoot};
    use fb_items::Slot;

    /// A worn pack with a sack inside it, and a sack lying loose.
    fn bags() -> (Inventory, ItemId, ItemId, ItemId) {
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.wear(pack, Slot::Back).unwrap();
        let inner = inventory.add(fb_items::roll_item("Sack", 3).unwrap());
        inventory.put(inner, pack, Placement::new(0, 0, false)).unwrap();
        let loose = inventory.add(fb_items::roll_item("Sack", 4).unwrap());
        (inventory, pack, inner, loose)
    }

    #[test]
    fn a_bag_is_in_reach_only_carried_underfoot_or_in_the_opened_container() {
        let (inventory, pack, inner, loose) = bags();
        assert!(reachable(&inventory, &[], None, pack), "worn");
        assert!(reachable(&inventory, &[], None, inner), "nested in a carried bag");
        assert!(!reachable(&inventory, &[], None, loose), "set down and left, or given away");
        assert!(reachable(&inventory, &[loose], None, loose), "set down underfoot");
        assert!(reachable(&inventory, &[], Some(loose), loose), "an opened chest");
    }

    fn place_app(inventory: Inventory) -> App {
        let mut app = App::new();
        // Every bag the inventory is made with has been offered already, so none opens on its own here.
        app.insert_resource(BagWindows { seen: inventory.carried_containers(), ..default() })
            .init_resource::<Tuning>()
            .init_resource::<Underfoot>()
            .init_resource::<OpenContainer>()
            .init_resource::<crate::barter::Barter>()
            .init_resource::<ScreenOpen>()
            .init_resource::<SheetTab>()
            .insert_resource(inventory)
            .add_systems(Update, place_windows);
        app
    }

    #[test]
    fn windows_close_when_their_bag_is_out_of_reach() {
        let (inventory, pack, inner, loose) = bags();
        let mut app = place_app(inventory);
        app.world_mut().resource_mut::<BagWindows>().order = vec![pack, inner, loose];
        app.update();
        assert_eq!(app.world().resource::<BagWindows>().order, vec![pack, inner], "the loose sack closed, the nested one stayed");
        // Set it down underfoot and it can be opened.
        app.world_mut().resource_mut::<Underfoot>().bags.push(loose);
        app.world_mut().resource_mut::<BagWindows>().order.push(loose);
        app.update();
        assert!(app.world().resource::<BagWindows>().is_open(loose));
        // Walk away from it.
        app.world_mut().resource_mut::<Underfoot>().bags.clear();
        app.update();
        assert!(!app.world().resource::<BagWindows>().is_open(loose));
    }

    #[test]
    fn windows_sit_beside_the_doll_and_stay_open_while_the_inventory_is_closed() {
        let (inventory, pack, inner, _) = bags();
        let mut app = place_app(inventory);
        let mut window = Window::default();
        window.resolution.set(1280.0, 720.0);
        app.world_mut().spawn((window, PrimaryWindow));
        // 1280 wide at two thirds is the 1920 by 1080 the interface is laid out in.
        app.world_mut().insert_resource(UiScale(1280.0 / 1920.0));
        app.world_mut().resource_mut::<BagWindows>().order = vec![pack, inner];
        app.update();
        assert!(app.world().resource::<BagWindows>().at.is_empty(), "nothing is laid out while the inventory is closed");
        assert_eq!(app.world().resource::<BagWindows>().order, vec![pack, inner], "but they stay open for when it opens");
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        app.update();
        let windows = app.world().resource::<BagWindows>();
        let area = regions(Vec2::new(1920.0, 1080.0)).2;
        assert_eq!(windows.order, vec![pack, inner]);
        assert_eq!(windows.at[&pack], area.min, "the first window is the area's corner, right of the doll");
        assert_ne!(windows.at[&inner], windows.at[&pack]);
    }

    #[test]
    fn the_oldest_window_is_hidden_when_the_screen_has_no_room_beside_the_doll() {
        let (inventory, pack, inner, _) = bags();
        let mut app = place_app(inventory);
        let mut window = Window::default();
        window.resolution.set(1280.0, 720.0);
        app.world_mut().spawn((window, PrimaryWindow));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        // At a scale of 1, 1280 pixels leave nothing right of the doll.
        app.world_mut().resource_mut::<BagWindows>().order = vec![pack, inner];
        app.update();
        let windows = app.world().resource::<BagWindows>();
        assert_eq!((windows.order.clone(), windows.hidden.clone()), (vec![pack, inner], vec![pack]), "the oldest is left out but remembered");
    }

    /// A delver wearing a pack, a satchel and a belt rolled from the three seeds.
    fn three_bags(seeds: (u32, u32, u32)) -> (Inventory, [ItemId; 3]) {
        let mut inventory = Inventory::default();
        let mut wear = |base: &str, seed: u32, slot: Slot| {
            let id = inventory.add(fb_items::roll_item(base, seed).unwrap());
            inventory.wear(id, slot).unwrap();
            id
        };
        let bags = [wear("Pack", seeds.0, Slot::Back), wear("Satchel", seeds.1, Slot::Satchel), wear("Belt", seeds.2, Slot::Belt)];
        (inventory, bags)
    }

    #[test]
    fn a_pack_a_satchel_and_a_belt_all_fit_beside_the_doll_at_any_seed() {
        let screen = Vec2::new(1920.0, 1080.0);
        let area = regions(screen).2;
        for seed in 1..400u32 {
            let (inventory, bags) = three_bags((seed, seed * 7 % 400 + 1, seed * 13 % 400 + 1));
            let (scale, skipped, placed) = fit_windows(&inventory, &bags, 72.0, area);
            assert_eq!((skipped, placed.len()), (0, 3), "seed {seed}: scale {scale}");
            let sizes: Vec<(ItemId, Vec2)> = bags.iter().map(|&id| (id, window_size(&inventory, id, 72.0 * scale).unwrap())).collect();
            no_overlap(&placed, &sizes, area);
        }
    }

    #[test]
    fn three_bags_open_at_1280_by_720_all_show_and_none_overlap() {
        for seed in [1u32, 7, 23, 91, 140, 333] {
            let (inventory, bags) = three_bags((seed, seed + 3, seed + 11));
            let mut app = place_app(inventory);
            let mut window = Window::default();
            window.resolution.set(1280.0, 720.0);
            app.world_mut().spawn((window, PrimaryWindow));
            app.world_mut().insert_resource(UiScale(1280.0 / 1920.0));
            app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
            app.world_mut().resource_mut::<BagWindows>().order = bags.to_vec();
            app.update();
            let windows = app.world().resource::<BagWindows>();
            assert!(windows.hidden.is_empty(), "seed {seed}");
            let inventory = app.world().resource::<Inventory>();
            let sizes: Vec<(ItemId, Vec2)> = bags.iter().map(|&id| (id, window_size(inventory, id, 72.0 * windows.scale).unwrap())).collect();
            let placed: Vec<(ItemId, Vec2)> = bags.iter().map(|id| (*id, windows.at[id])).collect();
            no_overlap(&placed, &sizes, regions(Vec2::new(1920.0, 1080.0)).2);
        }
    }

    #[test]
    fn windows_pushed_out_for_room_come_back_when_room_frees() {
        let (mut inventory, pack, inner, _) = bags();
        let loose = inventory.add(fb_items::roll_item("Satchel", 3).unwrap());
        inventory.wear(loose, Slot::Satchel).unwrap();
        let mut app = place_app(inventory);
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        // A screen just tall enough for two of the three.
        let hidden_at = |app: &mut App, height: f32| {
            app.world_mut().query::<&mut Window>().single_mut(app.world_mut()).unwrap().resolution.set(1920.0, height);
            app.world_mut().resource_mut::<BagWindows>().order = vec![pack, inner, loose];
            app.update();
            app.world().resource::<BagWindows>().hidden.clone()
        };
        let height = (100..1080).step_by(5).map(|h| h as f32).find(|&h| hidden_at(&mut app, h).len() == 1).expect("some height has room for two of the three");
        let hidden = hidden_at(&mut app, height);
        assert_eq!(hidden, vec![pack], "the oldest is the one pushed out");
        assert_eq!(app.world().resource::<BagWindows>().order, vec![pack, inner, loose], "but it is still remembered");
        // Close the newest: the pushed out one comes back.
        app.world_mut().resource_mut::<BagWindows>().close(loose);
        app.update();
        let windows = app.world().resource::<BagWindows>();
        assert_eq!(windows.order, vec![pack, inner]);
        assert!(windows.hidden.is_empty(), "{:?}", windows.hidden);
        assert!(windows.is_shown(pack) && windows.at.contains_key(&pack));
    }

    #[test]
    fn a_click_settles_into_a_toggle_after_the_double_click_interval() {
        let (inventory, pack, ..) = bags();
        let mut app = place_app(inventory);
        app.init_resource::<Time<Real>>().add_systems(Update, settle_click.before(place_windows));
        app.world_mut().resource_mut::<BagWindows>().pending = Some((pack, 0.0));
        app.update();
        assert!(!app.world().resource::<BagWindows>().is_open(pack), "waiting for a second click");
        app.world_mut().resource_mut::<Time<Real>>().advance_to(std::time::Duration::from_secs(2));
        app.update();
        assert!(app.world().resource::<BagWindows>().is_open(pack));
        assert_eq!(app.world().resource::<BagWindows>().pending, None);
        app.world_mut().resource_mut::<BagWindows>().pending = Some((pack, 0.0));
        app.update();
        assert!(!app.world().resource::<BagWindows>().is_open(pack), "clicked again, closed");
    }

    #[test]
    fn a_window_dragged_to_a_place_opens_there_again_and_a_new_one_takes_the_next_free_place() {
        let (inventory, pack, inner, _) = bags();
        let mut app = place_app(inventory);
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        app.world_mut().insert_resource(UiScale(1280.0 / 1920.0));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        let area = regions(Vec2::new(1920.0, 1080.0)).2;
        let left_at = Vec2::new(1200.0, 120.0);
        app.world_mut().resource_mut::<BagWindows>().moved.insert(pack, left_at);
        app.world_mut().resource_mut::<BagWindows>().order = vec![pack];
        app.update();
        assert_eq!(app.world().resource::<BagWindows>().at[&pack], left_at, "it is where it was left, not at the corner");
        app.world_mut().resource_mut::<BagWindows>().close(pack);
        app.world_mut().resource_mut::<BagWindows>().toggle(inner);
        app.update();
        assert_eq!(app.world().resource::<BagWindows>().at[&inner], area.min, "the new one takes the first place in the columns, which the dragged one left free");
        app.world_mut().resource_mut::<BagWindows>().toggle(pack);
        app.update();
        let windows = app.world().resource::<BagWindows>();
        assert_eq!(windows.at[&pack], left_at, "opened again, it is back where it was left");
        assert_eq!(windows.at[&inner], area.min, "and the other did not move");
    }

    #[test]
    fn a_dragged_window_is_pulled_back_inside_when_the_screen_shrinks() {
        let (inventory, pack, ..) = bags();
        let mut app = place_app(inventory);
        let mut window = Window::default();
        window.resolution.set(1920.0, 1080.0);
        let window = app.world_mut().spawn((window, PrimaryWindow)).id();
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        let left_at = Vec2::new(1500.0, 700.0);
        app.world_mut().resource_mut::<BagWindows>().moved.insert(pack, left_at);
        app.world_mut().resource_mut::<BagWindows>().order = vec![pack];
        app.update();
        let size = window_size(app.world().resource::<Inventory>(), pack, 72.0).unwrap();
        let at = app.world().resource::<BagWindows>().at[&pack];
        assert!(at.x + size.x <= 1920.01 && at.y + size.y <= 1080.01, "{at} {size}: pulled in even at full size");
        for screen in [Vec2::new(1920.0, 1080.0), Vec2::new(1920.0, 1200.0), Vec2::new(2200.0, 1080.0), Vec2::new(1920.0, 900.0)] {
            app.world_mut().get_mut::<Window>(window).unwrap().resolution.set(screen.x, screen.y);
            app.update();
            let windows = app.world().resource::<BagWindows>();
            let size = window_size(app.world().resource::<Inventory>(), pack, 72.0 * windows.scale).unwrap();
            let at = windows.at[&pack];
            assert!(at.x >= drag_area(screen).min.x && at.y >= 0.0, "{screen}: {at}");
            assert!(at.x + size.x <= screen.x + 0.01 && at.y + size.y <= screen.y + 0.01, "{screen}: {at} {size}");
        }
        let windows = app.world().resource::<BagWindows>();
        assert_eq!(windows.moved[&pack], left_at, "what was left is kept, only the drawing is pulled in");
        assert!(windows.hidden.is_empty() && windows.is_shown(pack), "a moved window is placed, never hidden");
    }

    #[test]
    fn the_delver_s_windows_are_dragged_where_the_columns_start_clear_of_the_doll_and_quickslots() {
        let screen = Vec2::new(1920.0, 1080.0);
        let party = drag_area(screen);
        assert_eq!((party.min, party.max), (Vec2::new(regions(screen).2.min.x - AREA_MARGIN_PX, 0.0), screen));
        assert!(party.min.x >= LEFT_PX + MIDDLE_PX, "right of the delver's column, never on the world side");
        let size = Vec2::new(300.0, 200.0);
        for at in [Vec2::splat(-500.0), Vec2::splat(9999.0), Vec2::new(1000.0, 500.0)] {
            let put = clamp_into(at, size, party);
            assert!(put.x >= party.min.x && put.y >= 0.0 && put.x + size.x <= screen.x && put.y + size.y <= screen.y, "{at} -> {put}");
        }
        assert_eq!(clamp_into(Vec2::splat(50.0), Vec2::splat(5000.0), party), party.min, "one too big sits at the corner");
    }

    #[test]
    fn where_a_window_was_left_is_forgotten_once_its_bag_is_out_of_reach() {
        let (inventory, pack, _, loose) = bags();
        let mut app = place_app(inventory);
        app.world_mut().resource_mut::<Underfoot>().bags.push(loose);
        app.world_mut().resource_mut::<BagWindows>().moved.extend([(pack, Vec2::splat(700.0)), (loose, Vec2::splat(800.0))]);
        app.update();
        assert_eq!(app.world().resource::<BagWindows>().moved.len(), 2, "both in reach");
        app.world_mut().resource_mut::<Underfoot>().bags.clear();
        app.update();
        let moved = &app.world().resource::<BagWindows>().moved;
        assert!(moved.contains_key(&pack) && !moved.contains_key(&loose), "the bag left behind is forgotten");
    }
}
