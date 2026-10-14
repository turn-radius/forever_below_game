//! Forever Below. For now it opens on the Armory turntable.
//!
//! `cargo run -p forever_below -- --family mace --seed 1207 --wear grave_find`
//! `FB_SCREENSHOT=path.png` saves one offscreen frame and quits; `FB_WEAR=grave_find` sets the
//! starting condition (the `--wear` flag wins); `FB_CLOSEUP=<slot>` opens on a close-up of
//! grid slot 0 to 11.

use bevy::prelude::*;
use forever_below::{
    capture::FrameCapturePlugin,
    light::LightPlugin,
    render::ArmoryRenderPlugin,
    turntable::{Focus, Turntable, TurntablePlugin, parse_wear},
};

fn main() -> AppExit {
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
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if let Ok(wear) = std::env::var("FB_WEAR") {
        parse_wear(&wear).map_err(|e| format!("FB_WEAR: {e}"))?;
        args.splice(0..0, ["--wear".to_string(), wear]);
    }
    Turntable::from_args(args)
}

fn focus_from_env() -> Result<Focus, String> {
    match std::env::var("FB_CLOSEUP") {
        Ok(slot) if !slot.is_empty() => Focus::close_up_on(&slot).map_err(|e| format!("FB_CLOSEUP: {e}")),
        _ => Ok(Focus::default()),
    }
}
