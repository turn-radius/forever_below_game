//! What the headless-game tests share.

#![allow(dead_code)]

use std::time::{Duration, Instant};

use bevy::prelude::*;
use forever_below::{
    loading::StartingDelve,
    travel::{Journey, Phase},
};

/// Whether a level is being made off the main thread: a delve begun from the title, or a
/// journey waiting for the level to come.
pub fn loading(app: &App) -> bool {
    app.world().get_resource::<StartingDelve>().is_some() || app.world().get_resource::<Journey>().is_some_and(|journey| journey.phase == Phase::Loading)
}

/// Steps `frames` frames, a 60th of a second each. A frame spent waiting for a level to be made
/// on the async compute pool does not count, as the game's own clock does not move for it: the
/// wait takes as long as the work does.
pub fn run(app: &mut App, frames: usize) {
    let started = Instant::now();
    let mut counted = 0;
    while counted < frames {
        let waiting = loading(app);
        app.update();
        if waiting {
            assert!(started.elapsed() < Duration::from_secs(300), "the level was not made in five minutes");
            std::thread::sleep(Duration::from_millis(2));
        } else {
            counted += 1;
        }
    }
}
