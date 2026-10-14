//! The delve: one level of the abyss, played first person. This module holds what the play
//! plugins share; each plugin owns its own file.
//!
//! - [`crate::level`]: the level's stone geometry, its fixed lights and the entrance door.
//! - [`crate::player`]: the first-person body, camera, movement and collision.
//! - [`crate::carry`]: the torch in hand, items lying in the level, picking them up, the HUD.
//! - [`crate::folk`]: the folk standing in the level.
//! - [`crate::inventory`]: the inventory screen over the level (I).
//! - [`crate::keymap`]: every key and button, read by action.
//! - [`crate::menu`]: the pause menu over the level (Esc).
//! - [`crate::audio`]: the level's music.

use bevy::{
    platform::collections::HashSet,
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_level::Level;

use crate::keymap::{Action, ActionInput, Keymap};

/// The level being played. Inserted before the play plugins run.
#[derive(Resource, Clone, Debug)]
pub struct CurrentLevel(pub Level);

/// The player's body. Its `Transform` is at the feet, on the floor; the camera is a child.
#[derive(Component, Debug, Default)]
pub struct Player;

/// The first-person camera, a child of [`Player`].
#[derive(Component, Debug, Default)]
pub struct PlayerCamera;

/// Eye height above the floor in world units (1.6 m, a human's eyes at 17 units tall).
pub const EYE_HEIGHT: f32 = 16.0;

/// Grid cells that are open floor in the level data but closed to walking now, such as the
/// entrance once its door shuts. Player collision treats them as rock.
#[derive(Resource, Clone, Debug, Default)]
pub struct BlockedCells(pub HashSet<(i32, i32)>);

/// A screen drawn over the level, which takes the mouse and keys while it is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Screen {
    /// The inventory ([`crate::inventory`]).
    Inventory,
    /// The pause menu ([`crate::menu`]).
    Menu,
}

/// Which screen, if any, is open over the level. The level keeps drawing behind it, but the
/// body stands still and does not look round.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScreenOpen(pub Option<Screen>);

impl ScreenOpen {
    /// Whether `screen` is the one open.
    pub fn is(&self, screen: Screen) -> bool {
        self.0 == Some(screen)
    }
}

/// Run condition: no screen is open over the level.
pub fn level_has_focus(screen: Option<Res<ScreenOpen>>) -> bool {
    !screen.is_some_and(|s| s.0.is_some())
}

/// The screen open after this frame's presses, from `open`: Esc closes an open screen first,
/// and with none open brings up the menu; the inventory key opens and closes the inventory,
/// but not over the menu.
pub fn next_screen(open: Option<Screen>, inventory: bool, back: bool) -> Option<Screen> {
    match open {
        Some(_) if back => None,
        None if back => Some(Screen::Menu),
        Some(Screen::Inventory) if inventory => None,
        None if inventory => Some(Screen::Inventory),
        unchanged => unchanged,
    }
}

/// Opens `screen` (or closes every screen with `None`), freeing the mouse for a screen and
/// taking it back for the level.
pub fn show_screen(open: &mut ScreenOpen, screen: Option<Screen>, cursor: Option<&mut CursorOptions>) {
    open.0 = screen;
    if let Some(cursor) = cursor {
        crate::player::set_grab(cursor, screen.is_none());
    }
}

/// Opens and closes screens from the keyboard ([`next_screen`]). Systems that draw a screen
/// run after it.
pub fn switch_screens(input: ActionInput, mut open: ResMut<ScreenOpen>, mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>) {
    let next = next_screen(open.0, input.just_pressed(Action::Inventory), input.just_pressed(Action::Back));
    if next != open.0 {
        show_screen(&mut open, next, cursor.as_deref_mut().map(|c| &mut **c));
    }
}

/// Everything needed to play a level.
pub struct PlayPlugin;

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockedCells>()
            .init_resource::<ScreenOpen>()
            .init_resource::<Keymap>()
            .add_systems(Update, switch_screens)
            .add_plugins((
            crate::level::LevelPlugin,
            crate::player::PlayerPlugin,
            crate::carry::CarryPlugin,
            crate::folk::FolkPlugin,
            crate::inventory::InventoryPlugin,
            crate::audio::MusicPlugin,
            crate::menu::MenuPlugin,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_closes_a_screen_before_it_opens_the_menu() {
        use Screen::{Inventory, Menu};
        assert_eq!(next_screen(Some(Inventory), false, true), None);
        assert_eq!(next_screen(None, false, true), Some(Menu));
        assert_eq!(next_screen(Some(Menu), false, true), None);
        assert_eq!(next_screen(None, true, false), Some(Inventory));
        assert_eq!(next_screen(Some(Inventory), true, false), None);
        assert_eq!(next_screen(Some(Menu), true, false), Some(Menu), "no inventory over the menu");
        assert_eq!(next_screen(None, false, false), None);
    }
}
