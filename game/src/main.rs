//! Forever Below. It opens on the title screen; `--turntable` (or `--family`) opens the Armory
//! turntable instead, and `--doll` the doll view (a dressed body turning on the turntable's
//! plate: `--doll --folk dwarf --seed 7 --outfit cleric`), and `--creature` the creature view
//! (a rat, snake or skeleton on the plate: `--creature --species viper --seed 7 --motion
//! moving`), and `--skeleton` the same view opened on a skeleton (`--skeleton --seed 7 --motion
//! swing`).
//! `--doors` the doors view (a claim door and its switch in a doorway beside a panel of the
//! kind's parameters: `--doors --kind portcullis --level stone_halls`),
//! `--scenery` the scenery view (a grid of one kind of scenery from consecutive seeds:
//! `--scenery --kind bunk --seed 7`),
//! `--forge` the Forge view (an item beside a panel of sliders for every parameter of its
//! state: `--forge --family lantern --seed 7`), and `--level` the level view (a whole level
//! from above beside a panel of the generator's parameters: `--level --seed 7`), and `--lab`
//! the character lab (a character and their gear beside their sheet of explained numbers:
//! `--lab --seed 7`). `--swings`
//! plays the level with keys for trying every melee weapon's swings
//! ([`forever_below::swing_practice`]).
//!
//! `cargo run -p forever_below -- --seed 1207` plays the level for that seed, skipping the
//! title, as the debug switches that look at the level do; `--title` keeps the title
//! ([`forever_below::launch`]).
//! `cargo run -p forever_below -- --turntable --family mace --seed 1207 --wear grave_find`
//! `FB_SCREENSHOT=path.png` saves one offscreen frame and quits; `FB_WEAR=grave_find` sets the
//! starting condition (the `--wear` flag wins); `FB_CLOSEUP=<slot>` opens on a close-up of
//! grid slot 0 to 11.

// A release build on Windows opens as a window alone, without a console behind it; debug
// builds keep the console for the log.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bevy::{audio::AudioPlugin, prelude::*};
use forever_below::{
    capture::FrameCapturePlugin,
    creature_view::{CreatureChoice, CreatureViewPlugin},
    frame_time::FrameTimeProbe,
    door_view::{DoorLab, DoorViewPlugin},
    doll_view::{DollChoice, DollMotion, DollSpin, DollViewPlugin},
    forge_view::{Forge, ForgeViewPlugin},
    level_view::{LevelForge, LevelViewPlugin},
    lab_figure::LabFigurePlugin,
    lab_view::{LabState, LabViewPlugin},
    launch::{AppState, Launch, env_set},
    render::ArmoryRenderPlugin,
    play::{CurrentLevel, PlayPlugin},
    rules::RulesPlugin,
    keybindings::KeybindingsPlugin,
    settings::SettingsPlugin,
    soak::SoakPlugin,
    swing_practice::SwingPracticePlugin,
    title::{NewDelveSeed, TitlePlugin},
    tuning::TuningPlugin,
    notes_view::{NotesView, NotesViewPlugin},
    scenery_view::{SceneryView, SceneryViewPlugin},
    turntable::{Focus, Turntable, TurntablePlugin},
};

fn main() -> AppExit {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--forge") {
        return run_forge(&args);
    }
    if args.iter().any(|a| a == "--doors") {
        return run_doors(&args);
    }
    if args.iter().any(|a| a == "--lab") {
        return run_lab(&args);
    }
    if args.iter().any(|a| a == "--scenery") {
        return run_scenery(&args);
    }
    if args.iter().any(|a| a == "--notes") {
        return run_notes(&args);
    }
    if args.iter().any(|a| a == "--level") {
        return run_level_view(&args);
    }
    if args.iter().any(|a| a == "--turntable" || a.starts_with("--family")) {
        return run_turntable();
    }
    if args.iter().any(|a| a == "--doll") {
        return run_doll(&args);
    }
    if args.iter().any(|a| a == "--creature" || a == "--skeleton") {
        return run_creature(&args);
    }
    let swings = args.iter().any(|a| a == "--swings");
    let inspect = args.iter().any(|a| a == "--inspect");
    let args: Vec<String> = args.into_iter().filter(|a| a != "--swings" && a != "--inspect").collect();
    let launch = match Launch::parse(&args) {
        Ok(launch) => launch,
        Err(error) => {
            eprintln!("forever_below: {error}");
            return AppExit::error();
        }
    };
    let opening = if swings { AppState::Delving } else { launch.opening(env_set) };
    let mut app = App::new();
    // `data/rules.toml` is read, and watched, as an asset source of its own.
    forever_below::rules::use_data_folder(&mut app);
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin { primary_window: Some(Window { title: "Forever Below".into(), present_mode: forever_below::frame_time::present_mode(), ..default() }), ..default() })
            // Debug builds watch the assets, so saving `assets/tuning.toml` or `data/rules.toml` applies it live.
            .set(AssetPlugin { file_path: forever_below::tuning::asset_file_path(), watch_for_changes_override: Some(cfg!(debug_assertions)), ..default() })
            // Spatial audio scale: positions are scaled to metres. 1 world unit is 10 cm = 0.1 m.
            .set(AudioPlugin { default_spatial_scale: bevy::audio::SpatialScale::new(0.1), ..default() }),
    )
    .insert_state(opening)
    .add_plugins((SettingsPlugin, KeybindingsPlugin, TuningPlugin, RulesPlugin, ArmoryRenderPlugin, PlayPlugin, TitlePlugin))
    .insert_resource(NewDelveSeed(launch.seed));
    if opening == AppState::Delving {
        // `FB_DEPTH=2` plays the stone halls: the level's depth picks its style.
        let depth = std::env::var("FB_DEPTH").ok().and_then(|d| d.parse().ok()).unwrap_or(1);
        // The level travel builds for that depth of the delve (`Abyss::level_at`).
        app.insert_resource(CurrentLevel(forever_below::travel::Abyss::new(launch.seed.unwrap_or(Launch::DEFAULT_SEED)).level_at(depth)));
    }
    if swings {
        app.add_plugins(SwingPracticePlugin);
    }
    // F12 (or `--inspect`) opens the component inspector, in debug builds only.
    #[cfg(all(feature = "inspector", debug_assertions))]
    app.add_plugins(forever_below::inspect::InspectPlugin { open: inspect });
    #[cfg(not(all(feature = "inspector", debug_assertions)))]
    let _ = inspect;
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

/// The plugins of the single-view tools (`--turntable`, `--doll`, `--creature`, `--forge`,
/// `--level`): a debug build watches the assets, shaders included, and a release build does not.
fn view_plugins() -> bevy::app::PluginGroupBuilder {
    DefaultPlugins
        .set(WindowPlugin { primary_window: Some(Window { title: "Forever Below".into(), ..default() }), ..default() })
        .set(AssetPlugin { watch_for_changes_override: Some(cfg!(debug_assertions)), ..default() })
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
    app.add_plugins(view_plugins())
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
    let motion = match DollMotion::from_args(args.iter().cloned()) {
        Ok(motion) => motion,
        Err(error) => {
            eprintln!("forever_below: {error}");
            return AppExit::error();
        }
    };
    let spin = match DollSpin::from_args(args.iter().cloned()) {
        Ok(spin) => spin,
        Err(error) => {
            eprintln!("forever_below: {error}");
            return AppExit::error();
        }
    };
    let mut app = App::new();
    app.add_plugins(view_plugins())
    .add_plugins((ArmoryRenderPlugin, DollViewPlugin))
    .insert_resource(choice)
    .insert_resource(motion)
    .insert_resource(spin);
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
    app.add_plugins(view_plugins())
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
    app.add_plugins(view_plugins())
    .add_plugins((ArmoryRenderPlugin, ForgeViewPlugin))
    .insert_resource(forge);
    if let Some(capture) = FrameCapturePlugin::from_env() {
        app.add_plugins(capture);
    }
    app.run()
}

fn run_lab(args: &[String]) -> AppExit {
    let lab = match LabState::from_args(args.iter().cloned()) {
        Ok(lab) => lab,
        Err(error) => {
            eprintln!("forever_below: {error}");
            return AppExit::error();
        }
    };
    let mut app = App::new();
    // The lab reads the rules the game runs on, and watches them in a debug build.
    forever_below::rules::use_data_folder(&mut app);
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin { primary_window: Some(forever_below::lab_view::window()), ..default() })
            .set(AssetPlugin { file_path: forever_below::tuning::asset_file_path(), watch_for_changes_override: Some(cfg!(debug_assertions)), ..default() }),
    )
    .add_plugins((RulesPlugin, ArmoryRenderPlugin, LabFigurePlugin, LabViewPlugin))
    .insert_resource(lab);
    if let Some(capture) = FrameCapturePlugin::from_env() {
        app.add_plugins(capture);
    }
    app.run()
}

fn run_doors(args: &[String]) -> AppExit {
    let lab = match DoorLab::from_args(args.iter().cloned()) {
        Ok(lab) => lab,
        Err(error) => {
            eprintln!("forever_below: {error}");
            return AppExit::error();
        }
    };
    let mut app = App::new();
    app.add_plugins(view_plugins()).add_plugins((ArmoryRenderPlugin, DoorViewPlugin)).insert_resource(lab);
    if let Some(capture) = FrameCapturePlugin::from_env() {
        app.add_plugins(capture);
    }
    app.run()
}

fn run_scenery(args: &[String]) -> AppExit {
    let view = match SceneryView::from_args(args.iter().cloned()) {
        Ok(view) => view,
        Err(error) => {
            eprintln!("forever_below: {error}");
            return AppExit::error();
        }
    };
    let mut app = App::new();
    app.add_plugins(view_plugins()).add_plugins((ArmoryRenderPlugin, SceneryViewPlugin)).insert_resource(view);
    if let Some(capture) = FrameCapturePlugin::from_env() {
        app.add_plugins(capture);
    }
    app.run()
}

fn run_notes(args: &[String]) -> AppExit {
    let view = match NotesView::from_args(args.iter().cloned()) {
        Ok(view) => view,
        Err(error) => {
            eprintln!("forever_below: {error}");
            return AppExit::error();
        }
    };
    let mut app = App::new();
    app.add_plugins(view_plugins()).add_plugins(NotesViewPlugin).insert_resource(view);
    if let Some(capture) = FrameCapturePlugin::from_env() {
        app.add_plugins(capture);
    }
    app.run()
}

fn run_level_view(args: &[String]) -> AppExit {
    let forge = match LevelForge::from_args(args.iter().cloned()) {
        Ok(forge) => forge,
        Err(error) => {
            eprintln!("forever_below: {error}");
            return AppExit::error();
        }
    };
    let mut app = App::new();
    app.add_plugins(view_plugins())
    .add_plugins(LevelViewPlugin)
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
