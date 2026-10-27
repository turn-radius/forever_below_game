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
/// Where the first window goes, and the steps to look for a free place from there.
const FIRST_AT: Vec2 = Vec2::new(24.0, 70.0);
const SEARCH_STEP_PX: f32 = 24.0;

/// Which containers are open as windows, and where each sits.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct BagWindows {
    /// Back to front: the last is on top.
    pub order: Vec<ItemId>,
    /// Each window's top left, in the interface's logical pixels.
    pub at: HashMap<ItemId, Vec2>,
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
pub fn window_size<T>(grid: &Grid<T>, cell: f32) -> Vec2 {
    let grid = grid_size(grid, cell);
    let edge = 2.0 * (PAD_PX + BORDER_PX);
    Vec2::new(grid.x.max(MIN_CONTENT_PX) + edge, grid.y + TITLE_PX + GAP_PX + edge)
}

/// `at` moved to keep a window of `size` inside a screen of `screen`.
pub fn clamp_window(at: Vec2, size: Vec2, screen: Vec2) -> Vec2 {
    at.clamp(Vec2::ZERO, (screen - size).max(Vec2::ZERO))
}

/// The size of the interface in its logical pixels: the window's, under `UiScale`.
pub fn ui_size(window: &Window, scale: Option<&UiScale>) -> Vec2 {
    Vec2::new(window.width(), window.height()) / scale.map_or(1.0, |s| s.0.max(f32::EPSILON))
}

/// A free place for a window of `size` among `placed`, scanning rows from the top left; the
/// first fit, else the top left.
fn free_place(size: Vec2, placed: &[Rect], screen: Vec2) -> Vec2 {
    let mut y = FIRST_AT.y;
    while y + size.y <= screen.y {
        let mut x = FIRST_AT.x;
        while x + size.x <= screen.x {
            let at = Vec2::new(x, y);
            let rect = Rect::from_corners(at, at + size);
            if placed.iter().all(|p| p.intersect(rect).is_empty()) {
                return at;
            }
            x += SEARCH_STEP_PX;
        }
        y += SEARCH_STEP_PX;
    }
    clamp_window(FIRST_AT, size, screen)
}

/// Drops windows whose container is gone, places new ones where nothing sits, and keeps every
/// window inside the screen, whatever size it becomes.
pub fn place_windows(
    mut windows: ResMut<BagWindows>,
    inventory: Res<Inventory>,
    tuning: Res<Tuning>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
) {
    let cell = tuning.interface.cell_size;
    let gone: Vec<ItemId> = windows.order.iter().copied().filter(|&id| inventory.grid(id).is_none()).collect();
    for id in gone {
        windows.close(id);
    }
    let Some(window) = window else { return };
    let screen = ui_size(&window, ui_scale.as_deref());
    if screen.x < 1.0 || screen.y < 1.0 {
        return;
    }
    let size_of = |id: ItemId| inventory.grid(id).map(|g| window_size(g, cell));
    let mut placed: Vec<Rect> = windows.order.iter().filter_map(|id| Some(Rect::from_corners(*windows.at.get(id)?, *windows.at.get(id)? + size_of(*id)?))).collect();
    let mut fixes: Vec<(ItemId, Vec2)> = Vec::new();
    for &id in &windows.order {
        let Some(size) = size_of(id) else { continue };
        let want = match windows.at.get(&id) {
            Some(&at) => clamp_window(at, size, screen),
            None => {
                let at = free_place(size, &placed, screen);
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
            windows.raise(id);
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

    #[test]
    fn new_windows_are_placed_side_by_side() {
        let screen = Vec2::new(1280.0, 720.0);
        let size = Vec2::new(300.0, 200.0);
        let first = free_place(size, &[], screen);
        let second = free_place(size, &[Rect::from_corners(first, first + size)], screen);
        assert!(Rect::from_corners(first, first + size).intersect(Rect::from_corners(second, second + size)).is_empty());
    }
}
