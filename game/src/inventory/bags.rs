//! Bags and containers opened as windows on the inventory screen. Double-clicking a bag the
//! delver carries (the pack on the back, the satchel, the belt pouch, a sack inside one) opens
//! its grid as a window to the right of the doll, and double-clicking it again, or its cross,
//! closes it. A bag set down underfoot or standing in the level opens on a single click. Windows
//! lay out in columns beside the doll, never over it or each other; with no room for another
//! the oldest closes ([`column_layout`]). Each grid is a drop target by its laid-out rectangle
//! ([`super::target_under`]), so items go between windows.
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

/// Which containers are open as windows, and where each sits.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct BagWindows {
    /// Everything opened and not closed, oldest first: the remembered set.
    pub order: Vec<ItemId>,
    /// Those of `order` the screen had no room for at the last layout, which come back by themselves
    /// when room frees (another window closes, or a smaller one is in the way).
    pub hidden: Vec<ItemId>,
    /// How much the windows' cells are shrunk from the screen's, 1 down to what lets them all fit
    /// ([`fit_windows`]).
    pub scale: f32,
    /// Each shown window's top left, in the interface's logical pixels, as [`place_windows`] last laid it out.
    pub at: HashMap<ItemId, Vec2>,
    /// A click on a bag lying underfoot or in the level that has not yet been told from the first
    /// half of a double-click (which takes it up): the bag and when it was clicked. It opens or
    /// closes its window once the interval is over ([`settle_click`]).
    pub pending: Option<(ItemId, f32)>,
}

impl Default for BagWindows {
    fn default() -> Self {
        BagWindows { order: Vec::new(), hidden: Vec::new(), scale: 1.0, at: HashMap::new(), pending: None }
    }
}

impl BagWindows {
    pub fn is_open(&self, id: ItemId) -> bool {
        self.order.contains(&id)
    }

    /// Whether `id` is open and has a window on the screen.
    pub fn is_shown(&self, id: ItemId) -> bool {
        self.is_open(id) && !self.hidden.contains(&id)
    }

    /// Opens `id` as the newest, or closes it if its window is showing. One with no room on the screen
    /// opens afresh, as the newest.
    pub fn toggle(&mut self, id: ItemId) {
        if self.is_shown(id) {
            self.close(id);
        } else {
            self.close(id);
            self.order.push(id);
        }
    }

    pub fn close(&mut self, id: ItemId) {
        self.order.retain(|&o| o != id);
        self.hidden.retain(|&o| o != id);
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

/// `at` moved to keep a window of `size` inside a screen of `screen`.
pub fn clamp_window(at: Vec2, size: Vec2, screen: Vec2) -> Vec2 {
    at.clamp(Vec2::ZERO, (screen - size).max(Vec2::ZERO))
}

/// The size of the interface in its logical pixels: that of the view it is drawn in (the held
/// torch's camera, as [`crate::ui::fit_interface`] reads it) under `UiScale`, else the
/// window's. They are one in play; a capture draws to a view of another size than its window.
fn ui_size(window: &Window, scale: Option<&UiScale>, cameras: &Query<&Camera, Or<(With<crate::carry::ViewModelCamera>, With<bevy::ui::IsDefaultUiCamera>)>>) -> Vec2 {
    let view = cameras.iter().filter(|c| c.is_active).find_map(Camera::logical_viewport_size).filter(|v| v.min_element() > 0.0);
    view.unwrap_or_else(|| Vec2::new(window.width(), window.height())) / scale.map_or(1.0, |s| s.0.max(f32::EPSILON))
}

/// Where the windows' columns lie on a screen of `screen` interface pixels: from just right of the
/// doll (the ledger sits at the screen's left in the inventory, [`super::LEDGER_PAD_PX`] in, its leaf and
/// the doll one after the other) to the screen's right edge, and the screen's height less its margins.
pub fn windows_area(screen: Vec2) -> Rect {
    let doll_right = super::LEDGER_PAD_PX + super::SOLO_LEAF_PX + super::LEDGER_GAP_PX + super::doll::FRAME.x;
    Rect::from_corners(Vec2::new(doll_right + AREA_MARGIN_PX, AREA_MARGIN_PX), Vec2::new(screen.x - AREA_MARGIN_PX, (screen.y - AREA_MARGIN_PX).max(AREA_MARGIN_PX)))
}

/// The shares of a cell the windows' grids are drawn at, biggest first: the first that lets every open
/// window fit is used, so a pack, a satchel and a belt with frogs all show on the smallest screen.
const WINDOW_SCALES: [f32; 6] = [1.0, 0.85, 0.75, 0.65, 0.55, 0.45];

/// Lays out the windows `ids` (oldest first) with cells of `cell` pixels in `area`, shrinking the cells
/// through [`WINDOW_SCALES`] until all fit; at the smallest, the oldest are left out as [`column_layout`]
/// says. Gives the scale, how many were left out and where the rest sit.
pub fn fit_windows(inventory: &Inventory, ids: &[ItemId], cell: f32, area: Rect) -> (f32, usize, Vec<(ItemId, Vec2)>) {
    let mut result = (1.0, 0, Vec::new());
    for scale in WINDOW_SCALES {
        let sizes: Vec<(ItemId, Vec2)> = ids.iter().filter_map(|&id| Some((id, window_size(inventory, id, cell * scale)?))).collect();
        let (skipped, placed) = column_layout(&sizes, area);
        result = (scale, skipped, placed);
        if skipped == 0 {
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
    for skip in 0..sizes.len().saturating_sub(1) {
        if let Some(placed) = fill(&sizes[skip..]) {
            return (skip, placed);
        }
    }
    let last = sizes.len().saturating_sub(1);
    (last, sizes.get(last).map(|&(id, _)| (id, area.min)).into_iter().collect())
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

/// Drops windows whose container is out of reach and, while the inventory page shows them, lays
/// the open ones out in columns beside the doll, shrinking their cells to fit them and, when they
/// still do not all fit, hiding the oldest.
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
    let opened = showing.0.filter(|&c| !barter.session.as_ref().is_some_and(|s| s.shelf == c));
    // A fallen body's things are in reach while the delver stands near the body
    // ([`crate::remains`]), like a bag set down underfoot.
    let roots: Vec<ItemId> = underfoot.bags.iter().copied().chain(bodies.iter().flat_map(|b| b.0.iter().copied())).collect();
    let gone: Vec<ItemId> = windows.order.iter().copied().filter(|&id| inventory.grid(id).is_none() || !reachable(&inventory, &roots, opened, id)).collect();
    for id in gone {
        windows.close(id);
    }
    // Windows are laid out only where they are drawn: the inventory page, with no barter.
    if !open.is(Screen::Inventory) || *tab != SheetTab::Inventory || barter.active() {
        return;
    }
    let Some(window) = window else { return };
    let screen = ui_size(&window, ui_scale.as_deref(), &cameras);
    if screen.x < 1.0 || screen.y < 1.0 {
        return;
    }
    // What has no room is hidden, not closed: it is still remembered and comes back when room frees.
    let (scale, _, placed) = fit_windows(&inventory, &windows.order, cell, windows_area(screen));
    let hidden: Vec<ItemId> = windows.order.iter().copied().filter(|id| !placed.iter().any(|(p, _)| p == id)).collect();
    if windows.hidden != hidden {
        windows.hidden = hidden;
    }
    if windows.scale != scale {
        windows.scale = scale;
    }
    for (id, at) in placed {
        if windows.at.get(&id) != Some(&at) {
            windows.at.insert(id, at);
        }
    }
}

/// Closes a window with its cross.
pub fn close_windows(
    open: Res<ScreenOpen>,
    input: ActionInput,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    dragging: Res<Dragging>,
    mut windows: ResMut<BagWindows>,
    closes: Query<(&BagClose, &ComputedNode, &UiGlobalTransform)>,
) {
    if !open.is(Screen::Inventory) || !input.just_pressed(Action::DragItem) || dragging.0.is_some_and(|d| d.active) {
        return;
    }
    let Some(window) = window else { return };
    let Some(point) = ui_cursor(&window, ui_scale.as_deref()) else { return };
    if let Some((close, ..)) = closes.iter().find(|(_, n, t)| node_rect(n, t).contains(point)) {
        windows.close(close.0);
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
            let area = windows_area(screen);
            assert!(area.min.x > super::super::SOLO_LEAF_PX + super::super::doll::FRAME.x, "{screen}: right of the doll");
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
        app.init_resource::<BagWindows>()
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
        let area = windows_area(Vec2::new(1920.0, 1080.0));
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
        let area = windows_area(screen);
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
            no_overlap(&placed, &sizes, windows_area(Vec2::new(1920.0, 1080.0)));
        }
    }

    #[test]
    fn windows_pushed_out_for_room_come_back_when_room_frees() {
        let (inventory, pack, inner, loose) = bags();
        let mut app = place_app(inventory);
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        app.world_mut().resource_mut::<Underfoot>().bags.push(loose);
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
}
