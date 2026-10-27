//! Bags and containers opened as windows on the inventory screen. Clicking a carried pack (in
//! its doll slot, or in a grid) pops its grid out of the left column into a window the delver
//! drags by its title bar and closes with the cross; clicking it again closes it, and a closed
//! bag goes back to the column. Windows float over the ledger, so two or more can sit side by
//! side and items go between them, each grid being a drop target by its laid-out rectangle
//! ([`super::target_under`]).
//!
//! The screen is rebuilt whenever the inventory changes, so what is open and where it sits is
//! kept in [`BagWindows`]. Positions are in the interface's logical pixels (the same as
//! [`crate::ui::node_rect`], so they scale with the window) and are kept inside the screen.

use std::collections::HashMap;

use bevy::{prelude::*, window::PrimaryWindow};

use super::{Dragging, Grid, Inventory, ItemId, Screen, ScreenOpen, grid_size};
use crate::keymap::{Action, ActionInput};
use crate::tuning::Tuning;
use crate::ui::{node_rect, ui_cursor};

/// Height of a window's title bar, logical pixels.
pub const TITLE_PX: f32 = 36.0;
/// Space between a window's border and what it holds.
pub const PAD_PX: f32 = 10.0;
/// A window's border.
pub const BORDER_PX: f32 = 2.0;
/// Space between the title bar and the grid.
pub const GAP_PX: f32 = 4.0;
/// The narrowest a window's contents are, so the name and the cross fit over a small pouch.
const MIN_CONTENT_PX: f32 = 200.0;
/// Where a cascade of windows starts when no free place is left.
const FIRST_AT: Vec2 = Vec2::new(24.0, 70.0);
/// The step the search for a free place moves by.
const SEARCH_STEP_PX: f32 = 24.0;
/// Space kept between a new window and what it must not cover.
const MARGIN_PX: f32 = 8.0;
/// How far a window is offset from the one before when there is no free space left.
const CASCADE_PX: f32 = 36.0;

/// Which containers are open as windows, and where each sits.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct BagWindows {
    /// Back to front: the last is on top.
    pub order: Vec<ItemId>,
    /// Each window's top left, in the interface's logical pixels.
    pub at: HashMap<ItemId, Vec2>,
    /// A click on a bag that has not yet been told from the first half of a double-click: the
    /// bag and when it was clicked. It opens or closes its window once the interval is over
    /// ([`settle_click`]).
    pub pending: Option<(ItemId, f32)>,
}

impl BagWindows {
    pub fn is_open(&self, id: ItemId) -> bool {
        self.order.contains(&id)
    }

    /// Opens `id` in front, or closes it if it is open. Its place is kept for next time.
    pub fn toggle(&mut self, id: ItemId) {
        if self.is_open(id) {
            self.close(id);
        } else {
            self.order.push(id);
        }
    }

    pub fn close(&mut self, id: ItemId) {
        self.order.retain(|&o| o != id);
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

/// A window's title bar, which drags it.
#[derive(Component, Debug)]
pub struct BagTitle(pub ItemId);

/// A window's close button.
#[derive(Component, Debug)]
pub struct BagClose(pub ItemId);

/// How big a window is for a grid, logical pixels.
pub(crate) fn window_size<T>(grid: &Grid<T>, cell: f32) -> Vec2 {
    let grid = grid_size(grid, cell);
    let edge = 2.0 * (PAD_PX + BORDER_PX);
    Vec2::new(grid.x.max(MIN_CONTENT_PX) + edge, grid.y + TITLE_PX + GAP_PX + edge)
}

/// `at` moved to keep a window of `size` inside a screen of `screen`.
pub fn clamp_window(at: Vec2, size: Vec2, screen: Vec2) -> Vec2 {
    at.clamp(Vec2::ZERO, (screen - size).max(Vec2::ZERO))
}

/// The size of the interface in its logical pixels: the window's, under `UiScale`.
pub(crate) fn ui_size(window: &Window, scale: Option<&UiScale>) -> Vec2 {
    Vec2::new(window.width(), window.height()) / scale.map_or(1.0, |s| s.0.max(f32::EPSILON))
}

/// A free place for a window of `size`, clear of the `obstacles` (the fixed panels, the docked
/// grids and the windows already placed), scanning from the top left; the first fit. With none,
/// the `n`th window cascades from the top left so every title bar stays showing.
pub fn free_place(size: Vec2, obstacles: &[Rect], screen: Vec2, n: usize) -> Vec2 {
    let mut y = 0.0;
    while y + size.y <= screen.y {
        let mut x = 0.0;
        while x + size.x <= screen.x {
            let at = Vec2::new(x, y);
            let rect = Rect::from_corners(at, at + size);
            if obstacles.iter().all(|o| o.inflate(MARGIN_PX).intersect(rect).is_empty()) {
                return at;
            }
            x += SEARCH_STEP_PX;
        }
        y += SEARCH_STEP_PX;
    }
    clamp_window(FIRST_AT + Vec2::splat(CASCADE_PX) * n as f32, size, screen)
}

/// Where the fixed panels of the inventory page are, in logical pixels: the header, the
/// doll with its quickslots, and every grid docked in the column.
pub fn fixed_rects(
    leaf: &Query<(&ComputedNode, &UiGlobalTransform), With<super::LeafPage>>,
    column: &Query<(&ComputedNode, &UiGlobalTransform), With<super::ScrollColumn>>,
    dolls: &Query<(&ComputedNode, &UiGlobalTransform), With<DollColumn>>,
    grids: &Query<(&super::GridView, &ComputedNode, &UiGlobalTransform)>,
    frames: &[Rect],
) -> Option<Vec<Rect>> {
    let leaf = leaf.iter().next().map(|(n, t)| node_rect(n, t))?;
    let column = column.iter().next().map(|(n, t)| node_rect(n, t))?;
    let doll = dolls.iter().next().map(|(n, t)| node_rect(n, t))?;
    if leaf.width() <= 0.0 || doll.width() <= 0.0 {
        return None;
    }
    let mut rects = vec![Rect::from_corners(leaf.min, Vec2::new(leaf.max.x, column.min.y)), doll];
    for (_, n, t) in grids {
        let rect = node_rect(n, t);
        if !frames.iter().any(|f| f.contains(rect.center())) {
            rects.push(rect);
        }
    }
    Some(rects)
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

/// Opens or closes the window of a bag clicked once, when the double-click interval has passed
/// without a second click, so a double-click (wear, take off) never toggles a window and
/// reflows the column under the second click.
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

/// Drops windows whose container is out of reach, places new ones where nothing sits, and keeps every
/// window inside the screen, whatever size it becomes.
pub fn place_windows(
    mut windows: ResMut<BagWindows>,
    inventory: Res<Inventory>,
    tuning: Res<Tuning>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    (underfoot, showing, barter): (Res<super::Underfoot>, Res<super::OpenContainer>, Res<crate::barter::Barter>),
    leaf: Query<(&ComputedNode, &UiGlobalTransform), With<super::LeafPage>>,
    column: Query<(&ComputedNode, &UiGlobalTransform), With<super::ScrollColumn>>,
    dolls: Query<(&ComputedNode, &UiGlobalTransform), With<DollColumn>>,
    grids: Query<(&super::GridView, &ComputedNode, &UiGlobalTransform)>,
    frames: Query<(&BagWindow, &ComputedNode, &UiGlobalTransform)>,
) {
    let cell = tuning.interface.cell_size;
    // The trader's shelf is not the delver's to reach into.
    let opened = showing.0.filter(|&c| !barter.session.as_ref().is_some_and(|s| s.shelf == c));
    let gone: Vec<ItemId> = windows.order.iter().copied().filter(|&id| inventory.grid(id).is_none() || !reachable(&inventory, &underfoot.bags, opened, id)).collect();
    for id in gone {
        windows.close(id);
    }
    let Some(window) = window else { return };
    let screen = ui_size(&window, ui_scale.as_deref());
    if screen.x < 1.0 || screen.y < 1.0 {
        return;
    }
    let frame_rects: Vec<Rect> = frames.iter().map(|(_, n, t)| node_rect(n, t)).collect();
    let fixed = fixed_rects(&leaf, &column, &dolls, &grids, &frame_rects);
    let size_of = |id: ItemId| inventory.grid(id).map(|g| window_size(g, cell));
    // New windows wait for the page to be laid out, to know where its panels are.
    let waiting = windows.order.iter().any(|id| !windows.at.contains_key(id));
    if waiting && fixed.is_none() {
        return;
    }
    let mut placed: Vec<Rect> = fixed.unwrap_or_default();
    placed.extend(windows.order.iter().filter_map(|id| Some(Rect::from_corners(*windows.at.get(id)?, *windows.at.get(id)? + size_of(*id)?))));
    let mut fixes: Vec<(ItemId, Vec2)> = Vec::new();
    let mut fresh = 0;
    for &id in &windows.order {
        let Some(size) = size_of(id) else { continue };
        let want = match windows.at.get(&id) {
            Some(&at) => clamp_window(at, size, screen),
            None => {
                let at = free_place(size, &placed, screen, fresh);
                fresh += 1;
                placed.push(Rect::from_corners(at, at + size));
                at
            }
        };
        if windows.at.get(&id) != Some(&want) {
            fixes.push((id, want));
        }
    }
    for (id, at) in fixes {
        windows.at.insert(id, at);
    }
}

/// Raises the window pressed, drags one by its title bar and closes one with its cross.
#[allow(clippy::too_many_arguments)]
pub fn drag_windows(
    open: Res<ScreenOpen>,
    input: ActionInput,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    dragging: Res<Dragging>,
    inventory: Res<Inventory>,
    tuning: Res<Tuning>,
    mut windows: ResMut<BagWindows>,
    frames: Query<(Entity, &BagWindow, &ComputedNode, &UiGlobalTransform)>,
    titles: Query<(&BagTitle, &ComputedNode, &UiGlobalTransform)>,
    closes: Query<(&BagClose, &ComputedNode, &UiGlobalTransform)>,
    mut nodes: Query<&mut Node, With<BagWindow>>,
    mut held: Local<Option<(ItemId, Vec2)>>,
) {
    if !open.is(Screen::Inventory) {
        *held = None;
        return;
    }
    let Some(window) = window else { return };
    let Some(point) = ui_cursor(&window, ui_scale.as_deref()) else { return };
    if input.just_pressed(Action::DragItem) && !dragging.0.is_some_and(|d| d.active) {
        // The front window under the mouse takes the press.
        let hit = windows.order.iter().rev().copied().find(|id| frames.iter().any(|(_, w, n, t)| w.0 == *id && node_rect(n, t).contains(point)));
        if let Some(id) = hit {
            if closes.iter().any(|(c, n, t)| c.0 == id && node_rect(n, t).contains(point)) {
                windows.close(id);
                return;
            }
            if let Some((_, n, t)) = titles.iter().find(|(b, n, t)| b.0 == id && node_rect(n, t).contains(point)) {
                *held = Some((id, point - node_rect(n, t).min));
            }
            // Only a change of order marks the windows changed, which rebuilds the screen.
            if windows.order.last() != Some(&id) {
                windows.raise(id);
            }
        }
    }
    let Some((id, grab)) = *held else { return };
    if !input.pressed(Action::DragItem) {
        *held = None;
        return;
    }
    let Some(grid) = inventory.grid(id) else { return };
    let Some((entity, ..)) = frames.iter().find(|(_, w, ..)| w.0 == id) else { return };
    let screen = ui_size(&window, ui_scale.as_deref());
    let size = window_size(grid, tuning.interface.cell_size);
    // The title bar starts inside the border and padding, so `grab` is from there.
    let to = clamp_window(point - grab - Vec2::splat(PAD_PX + BORDER_PX), size, screen);
    // Not through change detection: the screen is not rebuilt for a move, only the node moves.
    windows.bypass_change_detection().at.insert(id, to);
    if let Ok(mut node) = nodes.get_mut(entity) {
        node.left = px(to.x);
        node.top = px(to.y);
    }
}

/// `FB_OPEN_BAGS=1` opens every carried container as a window, for captures.
pub fn open_for_capture(mut windows: ResMut<BagWindows>, inventory: Res<Inventory>, mut done: Local<bool>) {
    if *done {
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

    /// What the page occupies: the header, the doll column and the docked grids.
    fn layout(screen: Vec2) -> Vec<Rect> {
        let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_corners(Vec2::new(x0, y0), Vec2::new(x1, y1));
        if screen.x > 1500.0 {
            vec![r(336.0, 60.0, 956.0, 230.0), r(984.0, 90.0, 1580.0, 920.0), r(336.0, 285.0, 770.0, 500.0), r(336.0, 560.0, 626.0, 850.0)]
        } else {
            vec![r(20.0, 20.0, 400.0, 130.0), r(800.0, 30.0, 1260.0, 690.0), r(20.0, 150.0, 330.0, 300.0)]
        }
    }

    #[test]
    fn new_windows_keep_off_the_panels_and_each_other() {
        let sizes = [Vec2::new(312.0, 300.0), Vec2::new(312.0, 220.0), Vec2::new(250.0, 200.0)];
        for screen in [Vec2::new(1920.0, 1080.0), Vec2::new(1280.0, 720.0)] {
            let fixed = layout(screen);
            for count in [2, 3] {
                let mut placed: Vec<Rect> = Vec::new();
                for (n, &size) in sizes.iter().take(count).enumerate() {
                    let mut obstacles = fixed.clone();
                    obstacles.extend(placed.iter().copied());
                    let at = free_place(size, &obstacles, screen, n);
                    let rect = Rect::from_corners(at, at + size);
                    assert!(Rect::from_corners(Vec2::ZERO, screen).contains(rect.min) && Rect::from_corners(Vec2::ZERO, screen).contains(rect.max), "{screen} #{n} on screen");
                    for o in &obstacles {
                        assert!(o.intersect(rect).is_empty(), "{screen} {count} windows: #{n} at {at} covers {o:?}");
                    }
                    placed.push(rect);
                }
            }
        }
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
    fn a_window_is_pulled_back_in_when_the_screen_shrinks_or_the_scale_grows() {
        let (inventory, pack, ..) = bags();
        let mut app = place_app(inventory);
        let mut window = Window::default();
        window.resolution.set(1920.0, 1080.0);
        let window = app.world_mut().spawn((window, PrimaryWindow)).id();
        let size = window_size(app.world().resource::<Inventory>().grid(pack).unwrap(), app.world().resource::<Tuning>().interface.cell_size);
        {
            let mut windows = app.world_mut().resource_mut::<BagWindows>();
            windows.order = vec![pack];
            windows.at.insert(pack, Vec2::new(900.0, 300.0));
        }
        app.update();
        assert_eq!(app.world().resource::<BagWindows>().at[&pack], Vec2::new(900.0, 300.0), "fits, stays put");
        app.world_mut().get_mut::<Window>(window).unwrap().resolution.set(1000.0, 600.0);
        app.update();
        assert_eq!(app.world().resource::<BagWindows>().at[&pack], clamp_window(Vec2::new(900.0, 300.0), size, Vec2::new(1000.0, 600.0)));
        assert_ne!(app.world().resource::<BagWindows>().at[&pack], Vec2::new(900.0, 300.0), "it had to move");
        app.world_mut().insert_resource(UiScale(2.0));
        app.update();
        let at = app.world().resource::<BagWindows>().at[&pack];
        assert!(at.x + size.x <= 500.0 + 0.01 || at == Vec2::ZERO, "{at}: inside a 500 by 300 interface, or in its corner");
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
