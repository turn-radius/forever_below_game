//! `FB_FRAME_TIME=240,270,300`: prints Bevy's smoothed frame time
//! ([`FrameTimeDiagnosticsPlugin`]) at each listed frame, with the median frame time since the
//! frame listed before it, and quits after the last, to measure
//! a scene's frame rate in the window, vsync and all. Pair it with `FB_DEBUG_POSE` to hold the
//! view still. `FB_NO_VSYNC=1` presents without waiting for the display, so a frame time under
//! 16.7 ms (60 Hz) shows.

use std::any::TypeId;

use bevy::{
    camera::visibility::{CubemapVisibleEntities, VisibleEntities},
    diagnostic::{DiagnosticsStore, FrameCount, FrameTimeDiagnosticsPlugin},
    prelude::*,
    window::PresentMode,
};

/// The window's present mode: vsync, or none with `FB_NO_VSYNC` set.
pub fn present_mode() -> PresentMode {
    if std::env::var_os("FB_NO_VSYNC").is_some_and(|v| !v.is_empty()) { PresentMode::AutoNoVsync } else { PresentMode::AutoVsync }
}

/// Logs the smoothed frame time at [`FrameTimeProbe::frames`] and exits after the last.
pub struct FrameTimeProbe {
    pub frames: Vec<u32>,
}

impl FrameTimeProbe {
    /// The probe when `FB_FRAME_TIME` lists frames, comma separated.
    pub fn from_env() -> Option<FrameTimeProbe> {
        let value = std::env::var("FB_FRAME_TIME").ok()?;
        let frames = parse_frames(&value)?;
        Some(FrameTimeProbe { frames })
    }
}

/// The frames in `value` ("240,270,300"), ascending, or `None` if there are none.
pub fn parse_frames(value: &str) -> Option<Vec<u32>> {
    let mut frames: Vec<u32> = value.split(',').filter_map(|f| f.trim().parse().ok()).collect();
    frames.sort_unstable();
    (!frames.is_empty()).then_some(frames)
}

#[derive(Resource)]
struct ProbeFrames(Vec<u32>);

impl Plugin for FrameTimeProbe {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        app.insert_resource(ProbeFrames(self.frames.clone())).add_systems(Last, report);
    }
}

fn report(
    frame: Res<FrameCount>,
    frames: Res<ProbeFrames>,
    store: Res<DiagnosticsStore>,
    time: Res<Time<Real>>,
    views: Query<&VisibleEntities, With<crate::play::PlayerCamera>>,
    shadows: Query<&CubemapVisibleEntities>,
    mut since: Local<Vec<f32>>,
    mut exit: MessageWriter<AppExit>,
) {
    since.push(time.delta_secs() * 1000.0);
    if !frames.0.contains(&frame.0) {
        return;
    }
    let smoothed = store.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME).and_then(|d| d.smoothed());
    since.sort_by(f32::total_cmp);
    let median = since[since.len() / 2];
    since.clear();
    // What the frame queued, which a vsynced frame time hides: meshes in the player camera's
    // view, and meshes across every face of every point light's shadow map.
    let drawn: usize = views.iter().map(|v| v.get(TypeId::of::<Mesh3d>()).len()).sum();
    let shadowed: usize = shadows.iter().flat_map(|faces| faces.iter()).map(|face| face.entities.len()).sum();
    println!(
        "frame {}: smoothed frame time {}, median since the last listed frame {median:.1} ms, {drawn} meshes in view, {shadowed} in shadow maps",
        frame.0,
        smoothed.map_or("unknown".into(), |ms| format!("{ms:.1} ms"))
    );
    if frames.0.last() == Some(&frame.0) {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_parse_in_order() {
        assert_eq!(parse_frames("300, 240,270"), Some(vec![240, 270, 300]));
        assert_eq!(parse_frames(""), None);
        assert_eq!(parse_frames("x"), None);
    }
}
