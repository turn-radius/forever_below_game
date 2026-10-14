//! Forever Below. It opens on the first level; `--turntable` (or `--family`) opens the Armory
//! turntable instead.
//!
//! `cargo run -p forever_below -- --seed 1207` plays the level for that seed.
//! `cargo run -p forever_below -- --turntable --family mace --seed 1207 --wear grave_find`
//! `FB_SCREENSHOT=path.png` saves one offscreen frame and quits; `FB_WEAR=grave_find` sets the
//! starting condition (the `--wear` flag wins); `FB_CLOSEUP=<slot>` opens on a close-up of
//! grid slot 0 to 11.

use bevy::prelude::*;
use forever_below::{
    capture::FrameCapturePlugin,
    light::LightPlugin,
    render::ArmoryRenderPlugin,
    play::{CurrentLevel, PlayPlugin},
    settings::SettingsPlugin,
    turntable::{Focus, Turntable, TurntablePlugin},
};

fn main() -> AppExit {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--turntable" || a.starts_with("--family")) {
        return run_turntable();
    }
    let seed = match level_seed(&args) {
        Ok(seed) => seed,
        Err(error) => {
            eprintln!("forever_below: {error}");
            return AppExit::error();
        }
    };
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window { title: "Forever Below".into(), ..default() }),
        ..default()
    }))
    .add_plugins((SettingsPlugin, ArmoryRenderPlugin, LightPlugin, PlayPlugin))
    .insert_resource(CurrentLevel(fb_level::generate(seed)));
    if let Some(capture) = FrameCapturePlugin::from_env() {
        app.add_plugins(capture);
    }
    app.run()
}

/// `--seed N` or `--seed=N`; 1207 by default.
fn level_seed(args: &[String]) -> Result<u32, String> {
    let value = match args {
        [] => return Ok(1207),
        [flag, value] if flag == "--seed" => value.clone(),
        [arg] if arg.starts_with("--seed=") => arg["--seed=".len()..].to_string(),
        [arg, ..] => return Err(format!("unknown argument {arg}")),
    };
    value.parse().map_err(|_| format!("--seed {value} is not a number"))
}

fn run_turntable() -> AppExit {
    let (turntable, focus) = match turntable_from_env().and_then(|t| Ok((t, focus_from_env()?))) {
        Ok(view) => view,
        Err(error) => {
            eprintln!("forever_below: {error}");
            return AppExit::error();
        }
    };
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window { title: "Forever Below".into(), ..default() }),
        ..default()
    }))
    .add_plugins((ArmoryRenderPlugin, LightPlugin, TurntablePlugin))
    .insert_resource(turntable)
    .insert_resource(focus);
    if let Some(capture) = FrameCapturePlugin::from_env() {
        app.add_plugins(capture);
    }
    app.run()
}

fn turntable_from_env() -> Result<Turntable, String> {
    let args = std::env::args().skip(1).filter(|a| a != "--turntable");
    Turntable::from_wear_and_args(std::env::var("FB_WEAR").ok(), args)
}

fn focus_from_env() -> Result<Focus, String> {
    match std::env::var("FB_CLOSEUP") {
        Ok(slot) if !slot.is_empty() => Focus::close_up_on(&slot).map_err(|e| format!("FB_CLOSEUP: {e}")),
        _ => Ok(Focus::default()),
    }
}
