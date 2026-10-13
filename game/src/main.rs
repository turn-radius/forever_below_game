//! Forever Below. For now it opens on the Armory turntable.
//!
//! `cargo run -p forever_below -- --family mace --seed 1207 --wear grave_find`
//! `FB_SCREENSHOT=path.png` saves one offscreen frame and quits; `FB_WEAR=grave_find` sets the
//! starting condition (the `--wear` flag wins).

use bevy::prelude::*;
use forever_below::{
    capture::FrameCapturePlugin,
    light::LightPlugin,
    render::ArmoryRenderPlugin,
    turntable::{Turntable, TurntablePlugin, parse_wear},
};

fn main() -> AppExit {
    let turntable = match turntable_from_env() {
        Ok(turntable) => turntable,
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
    .insert_resource(turntable);
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
