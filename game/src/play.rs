//! The delve: one level of the abyss, played first person. This module holds what the play
//! plugins share; each plugin owns its own file.
//!
//! - [`crate::level`]: the level's stone geometry, its fixed lights and the entrance door.
//! - [`crate::player`]: the first-person body, camera, movement and collision.
//! - [`crate::carry`]: the torch in hand, items lying in the level, picking them up, the HUD.
//! - [`crate::audio`]: the level's music.

use bevy::{platform::collections::HashSet, prelude::*};
use fb_level::Level;

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

/// What the player carries, in the order it was picked up.
#[derive(Resource, Clone, Debug, Default)]
pub struct Carried(pub Vec<fb_items::Item>);

/// Everything needed to play a level.
pub struct PlayPlugin;

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockedCells>().add_plugins((
            crate::level::LevelPlugin,
            crate::player::PlayerPlugin,
            crate::carry::CarryPlugin,
            crate::audio::MusicPlugin,
        ));
    }
}
