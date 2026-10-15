//! The game's clock, which measures elapsed time during a delve. It runs at a configurable ratio to
//! real time (e.g., 12× real time) and holds still while any screen is open. The clock is the
//! single source of time for every character simulation.

use bevy::prelude::*;
use fb_character::GameTime;

use crate::launch::AppState;
use crate::play::{DelveSystems, ScreenOpen};
use crate::tuning::Tuning;

/// The elapsed game time during the current delve, advanced each fixed step by the step's length
/// times the ratio from tuning. It starts at zero when the delve begins and holds still while a
/// screen is open (the inventory and the pause menu both set [`ScreenOpen`]).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct GameClock {
    /// The elapsed game time.
    pub time: GameTime,
}

impl GameClock {
    /// Advances the clock by `delta_seconds` game seconds.
    pub fn advance(&mut self, delta_seconds: f64) {
        self.time.advance(delta_seconds);
    }

    /// The elapsed game time in seconds.
    pub const fn game_seconds(&self) -> f64 {
        self.time.seconds()
    }
}

/// Advances the game clock each `FixedUpdate` step, scaling the step by the configured ratio.
/// Runs only while delving ([`DelveSystems`]) and with no screen open.
fn advance_game_clock(
    mut clock: ResMut<GameClock>,
    tuning: Res<Tuning>,
    time: Res<Time<Fixed>>,
    screen: Res<ScreenOpen>,
) {
    // Hold still while any screen is open. Only the pause menu also pauses `Time<Virtual>`, so
    // the inventory relies on this check.
    if screen.0.is_some() {
        return;
    }

    let delta_real_seconds = time.delta_secs() as f64;
    let game_seconds = delta_real_seconds * tuning.clock.game_seconds_per_second as f64;
    clock.advance(game_seconds);
}

/// Resets the game clock when a delve starts.
fn reset_clock_on_delve_start(mut clock: ResMut<GameClock>) {
    *clock = GameClock::default();
}

/// Clears any open screen when a delve ends, so a stale value doesn't freeze the next run.
fn clear_screen_on_delve_end(mut screen: ResMut<ScreenOpen>) {
    screen.0 = None;
}

/// Manages the game clock throughout the delve.
pub struct GameClockPlugin;

impl Plugin for GameClockPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameClock>()
            .add_systems(OnEnter(AppState::Delving), reset_clock_on_delve_start)
            .add_systems(OnExit(AppState::Delving), clear_screen_on_delve_end)
            .add_systems(FixedUpdate, advance_game_clock.in_set(DelveSystems));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::state::app::StatesPlugin;

    use super::*;
    use crate::play::{Screen, run_delve_systems_while_delving};

    fn app_in(state: AppState) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(state)
            .init_resource::<Tuning>()
            .init_resource::<ScreenOpen>()
            .add_plugins(GameClockPlugin);
        run_delve_systems_while_delving(&mut app);
        app.update();
        app
    }

    /// Runs `seconds` of fixed time, one second per step.
    fn run_fixed(app: &mut App, seconds: u32) {
        for _ in 0..seconds {
            app.world_mut().resource_mut::<Time<Fixed>>().advance_by(Duration::from_secs(1));
            app.world_mut().run_schedule(FixedUpdate);
        }
    }

    fn hours(app: &App) -> f64 {
        app.world().resource::<GameClock>().time.hours()
    }

    fn delve() -> App {
        let mut app = app_in(AppState::Delving);
        app.update();
        app
    }

    #[test]
    fn at_ratio_12_five_minutes_of_fixed_time_is_a_game_hour() {
        let mut app = delve();
        run_fixed(&mut app, 300);
        assert!((hours(&app) - 1.0).abs() < 1e-3, "{}", hours(&app));
    }

    #[test]
    fn changing_the_tuning_keeps_the_time_and_changes_the_rate() {
        let mut app = delve();
        run_fixed(&mut app, 300);
        app.world_mut().resource_mut::<Tuning>().clock.game_seconds_per_second = 6.0;
        assert!((hours(&app) - 1.0).abs() < 1e-3, "no jump");
        run_fixed(&mut app, 300);
        assert!((hours(&app) - 1.5).abs() < 1e-3, "{}", hours(&app));
    }

    #[test]
    fn an_open_screen_freezes_the_clock() {
        let mut app = delve();
        run_fixed(&mut app, 60);
        let before = hours(&app);
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        run_fixed(&mut app, 60);
        assert_eq!(hours(&app), before);
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        run_fixed(&mut app, 60);
        assert!(hours(&app) > before);
    }

    #[test]
    fn the_title_does_not_advance_the_clock() {
        let mut app = app_in(AppState::Title);
        run_fixed(&mut app, 300);
        assert_eq!(hours(&app), 0.0);
    }

    #[test]
    fn entering_the_delve_again_resets_the_clock() {
        let mut app = delve();
        run_fixed(&mut app, 300);
        assert!(hours(&app) > 0.9);
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Title);
        app.update();
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Delving);
        app.update();
        assert_eq!(hours(&app), 0.0);
    }

    #[test]
    fn a_stale_screen_open_from_a_previous_delve_does_not_freeze_a_new_run() {
        let mut app = delve();
        run_fixed(&mut app, 60);
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);

        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Title);
        app.update();
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Delving);
        app.update();

        assert_eq!(app.world().resource::<ScreenOpen>().0, None, "stale screen should be cleared");
        run_fixed(&mut app, 60);
        assert!(hours(&app) > 0.01, "clock should advance despite previous screen state");
    }
}
