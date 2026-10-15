//! Forever Below. It opens on the title screen; `--turntable` (or `--family`) opens the Armory
//! turntable instead, and `--doll` the doll view (a dressed body turning on the turntable's
//! plate: `--doll --folk dwarf --seed 7 --outfit cleric`), and `--creature` the creature view
//! (a rat or snake on the plate: `--creature --species viper --seed 7 --motion moving`).
//! `--forge` the Forge view (an item beside a panel of sliders for every parameter of its
//! state: `--forge --family lantern --seed 7`). `--swings` plays the level with keys for trying every melee weapon's swings
//! ([`forever_below::swing_practice`]).
//!
//! `cargo run -p forever_below -- --seed 1207` plays the level for that seed, skipping the
//! title, as the debug switches that look at the level do; `--title` keeps the title
//! ([`forever_below::launch`]).
//! `cargo run -p forever_below -- --turntable --family mace --seed 1207 --wear grave_find`
//! `FB_SCREENSHOT=path.png` saves one offscreen frame and quits; `FB_WEAR=grave_find` sets the
//! starting condition (the `--wear` flag wins); `FB_CLOSEUP=<slot>` opens on a close-up of
//! grid slot 0 to 11.

use bevy::prelude::*;
use forever_below::{
    capture::FrameCapturePlugin,
    creature_view::{CreatureChoice, CreatureViewPlugin},
    frame_time::FrameTimeProbe,
    doll_view::{DollChoice, DollViewPlugin},
    forge_view::{Forge, ForgeViewPlugin},
    launch::{AppState, Launch, env_set},
    render::ArmoryRenderPlugin,
    play::{CurrentLevel, PlayPlugin},
    settings::SettingsPlugin,
    soak::SoakPlugin,
    swing_practice::SwingPracticePlugin,
    title::{NewDelveSeed, TitlePlugin},
    tuning::TuningPlugin,
    turntable::{Focus, Turntable, TurntablePlugin},
};

fn main() -> AppExit {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--forge") {
        return run_forge(&args);
    }
    if args.iter().any(|a| a == "--turntable" || a.starts_with("--family")) {
        return run_turntable();
    }
    if args.iter().any(|a| a == "--doll") {
        return run_doll(&args);
    }
    if args.iter().any(|a| a == "--creature") {
        return run_creature(&args);
    }
    let swings = args.iter().any(|a| a == "--swings");
    let args: Vec<String> = args.into_iter().filter(|a| a != "--swings").collect();
    let launch = match Launch::parse(&args) {
        Ok(launch) => launch,
        Err(error) => {
            eprintln!("forever_below: {error}");
            return AppExit::error();
        }
    };
    let opening = if swings { AppState::Delving } else { launch.opening(env_set) };
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin { primary_window: Some(Window { title: "Forever Below".into(), ..default() }), ..default() })
            // Debug builds watch the assets, so saving `assets/tuning.toml` applies it live.
            .set(AssetPlugin { file_path: forever_below::tuning::asset_file_path(), watch_for_changes_override: Some(cfg!(debug_assertions)), ..default() }),
    )
    .insert_state(opening)
    .add_plugins((SettingsPlugin, TuningPlugin, ArmoryRenderPlugin, PlayPlugin, TitlePlugin))
    .insert_resource(NewDelveSeed(launch.seed));
    if opening == AppState::Delving {
        app.insert_resource(CurrentLevel(fb_level::generate(launch.seed.unwrap_or(Launch::DEFAULT_SEED))));
    }
    if swings {
        app.add_plugins(SwingPracticePlugin);
    }
    if let Some(capture) = FrameCapturePlugin::from_env() {
        app.add_plugins(capture);
    }
    if let Some(probe) = FrameTimeProbe::from_env() {
        app.add_plugins(probe);
    }
    if let Some(soak) = SoakPlugin::from_env() {
        app.add_plugins(soak);
    }
    app.run()
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
    .add_plugins((ArmoryRenderPlugin, TurntablePlugin))
    .insert_resource(turntable)
    .insert_resource(focus);
    if let Some(capture) = FrameCapturePlugin::from_env() {
        app.add_plugins(capture);
    }
    app.run()
}

fn run_doll(args: &[String]) -> AppExit {
    let choice = match DollChoice::from_args(args.iter().cloned()) {
        Ok(choice) => choice,
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
    .add_plugins((ArmoryRenderPlugin, DollViewPlugin))
    .insert_resource(choice);
    if let Some(capture) = FrameCapturePlugin::from_env() {
        app.add_plugins(capture);
    }
    app.run()
}

fn run_creature(args: &[String]) -> AppExit {
    let choice = match CreatureChoice::from_args(args.iter().cloned()) {
        Ok(choice) => choice,
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
    .add_plugins((ArmoryRenderPlugin, CreatureViewPlugin))
    .insert_resource(choice);
    if let Some(capture) = FrameCapturePlugin::from_env() {
        app.add_plugins(capture);
    }
    app.run()
}

fn run_forge(args: &[String]) -> AppExit {
    let forge = match Forge::from_args(args.iter().cloned()) {
        Ok(forge) => forge,
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
    .add_plugins((ArmoryRenderPlugin, ForgeViewPlugin))
    .insert_resource(forge);
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
