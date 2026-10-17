//! Offscreen frame capture: render every camera to an image, save one frame, quit.
//! Used to check scenes visually where window screenshots come back black.
//!
//! `FB_SCREENSHOT=path.png cargo run -p forever_below` turns it on via [`FrameCapturePlugin::from_env`].
//! Adding `FB_SCREENSHOT_FRAMES=120..136` saves a run of consecutive frames instead, as
//! `path_120.png` up to `path_135.png`, to compare frame by frame; `FB_SCREENSHOT_EVERY=30` saves
//! only every 30th of them, for a long run such as a fall, the title and the next delve.
//!
//! A capture shows exactly what its command line and environment ask for. The window still
//! opens, and on macOS it takes focus, so keys typed meant for a terminal (two W presses step
//! the turntable to battle worn) would otherwise reach the scene; [`IgnoreInput`] drops them.

use std::ops::Range;

use bevy::{
    camera::{CameraUpdateSystems, RenderTarget, ShadowLodOrigin},
    input::{
        InputSystems,
        mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    },
    prelude::*,
    render::{
        render_resource::TextureFormat,
        view::screenshot::{Screenshot, save_to_disk},
    },
};

/// Retargets the cameras that draw to the window to an offscreen image, saves each frame in
/// `frames` and exits at `exit_frame`. Frames before the capture let assets load and shaders
/// compile.
#[derive(Resource, Clone, Debug)]
pub struct FrameCapturePlugin {
    pub path: String,
    pub size: UVec2,
    /// The frames to save. One frame is saved to `path` itself; a longer run saves each frame
    /// `n` to `path` with `_n` before its extension ([`FrameCapturePlugin::path_for`]).
    pub frames: Range<u32>,
    /// Of `frames`, save only every this many (1 saves them all).
    pub every: u32,
    pub exit_frame: u32,
}

impl FrameCapturePlugin {
    /// Frames left after the last capture for its readback to reach the disk.
    const SETTLE_FRAMES: u32 = 60;

    pub fn new(path: impl Into<String>) -> Self {
        FrameCapturePlugin::frames(path, 120..121)
    }

    /// Saves every frame in `frames` (at least one).
    pub fn frames(path: impl Into<String>, frames: Range<u32>) -> Self {
        let frames = frames.start..frames.end.max(frames.start + 1);
        let exit_frame = frames.end + Self::SETTLE_FRAMES;
        FrameCapturePlugin { path: path.into(), size: UVec2::new(1920, 1080), frames, every: 1, exit_frame }
    }

    /// The plugin when `FB_SCREENSHOT` is set, saving the frames `FB_SCREENSHOT_FRAMES` names.
    pub fn from_env() -> Option<Self> {
        let path = std::env::var("FB_SCREENSHOT").ok().filter(|path| !path.is_empty())?;
        let frames = std::env::var("FB_SCREENSHOT_FRAMES").ok().filter(|f| !f.is_empty());
        let every = std::env::var("FB_SCREENSHOT_EVERY").ok().and_then(|n| n.trim().parse::<u32>().ok()).unwrap_or(1).max(1);
        let plugin = match frames.as_deref().map(parse_frames) {
            None => FrameCapturePlugin::new(path),
            Some(Ok(frames)) => FrameCapturePlugin { every, ..FrameCapturePlugin::frames(path, frames) },
            Some(Err(error)) => {
                warn!("FB_SCREENSHOT_FRAMES: {error}; saving one frame");
                FrameCapturePlugin::new(path)
            }
        };
        let size = std::env::var("FB_SCREENSHOT_SIZE").ok().filter(|s| !s.is_empty()).and_then(|spec| {
            let size = parse_size(&spec);
            if size.is_none() {
                warn!("FB_SCREENSHOT_SIZE: {spec:?} is not <width>x<height>; capturing at 1920x1080");
            }
            size
        });
        Some(FrameCapturePlugin { size: size.unwrap_or(plugin.size), ..plugin })
    }

    /// Whether frame `frame` is one to save.
    pub fn saves(&self, frame: u32) -> bool {
        self.frames.contains(&frame) && (frame - self.frames.start).is_multiple_of(self.every.max(1))
    }

    /// Where frame `frame` is saved: `path` for a single frame, else `path` with `_<frame>`
    /// before its extension (`shot.png` frame 120 is `shot_120.png`).
    pub fn path_for(&self, frame: u32) -> String {
        if self.frames.len() <= 1 {
            return self.path.clone();
        }
        let path = std::path::Path::new(&self.path);
        let stem = path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
        let name = match path.extension() {
            Some(ext) => format!("{stem}_{frame}.{}", ext.to_string_lossy()),
            None => format!("{stem}_{frame}"),
        };
        path.with_file_name(name).to_string_lossy().into_owned()
    }
}

/// A capture size written `<width>x<height>` (`1280x720`), each at least 1.
pub fn parse_size(spec: &str) -> Option<UVec2> {
    let (width, height) = spec.trim().split_once(['x', 'X'])?;
    let size = UVec2::new(width.trim().parse().ok()?, height.trim().parse().ok()?);
    (size.min_element() > 0).then_some(size)
}

/// Frames to capture written as a Rust range: `120..136` (120 to 135), `120..=135`, or one
/// frame, `120`.
pub fn parse_frames(spec: &str) -> Result<Range<u32>, String> {
    let number = |s: &str| s.trim().parse::<u32>().map_err(|_| format!("{s:?} is not a frame number"));
    let range = if let Some((start, end)) = spec.split_once("..=") {
        number(start)?..number(end)? + 1
    } else if let Some((start, end)) = spec.split_once("..") {
        number(start)?..number(end)?
    } else {
        let frame = number(spec)?;
        frame..frame + 1
    };
    if range.is_empty() {
        return Err(format!("{spec:?} holds no frames"));
    }
    Ok(range)
}

impl Plugin for FrameCapturePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.clone())
            .add_plugins(IgnoreInput)
            .add_systems(PostStartup, retarget_cameras)
            // After `Update`, so a camera spawned there never draws a frame to the window, and
            // before cameras size their views to their targets.
            .add_systems(PostUpdate, retarget_cameras.before(CameraUpdateSystems))
            .add_systems(Update, capture_and_exit);
    }
}

/// Clears each frame's keyboard and mouse input once Bevy has gathered it, so no system in
/// `Update` sees a key, button, drag or scroll.
pub struct IgnoreInput;

impl Plugin for IgnoreInput {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, ignore_input.after(InputSystems));
    }
}

fn ignore_input(
    mut keys: Option<ResMut<ButtonInput<KeyCode>>>,
    mut buttons: Option<ResMut<ButtonInput<MouseButton>>>,
    mut motion: Option<ResMut<AccumulatedMouseMotion>>,
    mut scroll: Option<ResMut<AccumulatedMouseScroll>>,
) {
    if let Some(keys) = keys.as_deref_mut() {
        keys.reset_all();
    }
    if let Some(buttons) = buttons.as_deref_mut() {
        buttons.reset_all();
    }
    if let Some(motion) = motion.as_deref_mut() {
        *motion = default();
    }
    if let Some(scroll) = scroll.as_deref_mut() {
        *scroll = default();
    }
}

#[derive(Resource)]
struct CaptureTarget(Handle<Image>);

/// Points every camera that draws to the window at the capture image, as cameras appear: at
/// startup, and those a delve begun from the title spawns later.
fn retarget_cameras(
    mut commands: Commands,
    settings: Res<FrameCapturePlugin>,
    target: Option<Res<CaptureTarget>>,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<(Entity, Option<&RenderTarget>), Added<Camera>>,
    lod_origins: Query<(), With<ShadowLodOrigin>>,
) {
    // A camera already drawing to an image (the title's lettering) keeps drawing there.
    let on_window: Vec<Entity> = cameras.iter().filter(|(_, target)| !matches!(target, Some(RenderTarget::Image(_)))).map(|(camera, _)| camera).collect();
    if on_window.is_empty() {
        return;
    }
    let image = match target {
        Some(target) => target.0.clone(),
        None => {
            let image = images.add(Image::new_target_texture(settings.size.x, settings.size.y, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb)));
            commands.insert_resource(CaptureTarget(image.clone()));
            image
        }
    };
    for &camera in &on_window {
        commands.entity(camera).insert(RenderTarget::Image(image.clone().into()));
    }
    // Point-light shadow LOD otherwise looks for a camera on the window and warns when none is.
    if lod_origins.is_empty()
        && let Some(&camera) = on_window.iter().min()
    {
        commands.entity(camera).insert(ShadowLodOrigin);
    }
}

fn capture_and_exit(
    mut commands: Commands,
    settings: Res<FrameCapturePlugin>,
    target: Option<Res<CaptureTarget>>,
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(target) = target else { return };
    *frame += 1;
    if settings.saves(*frame) {
        commands.spawn(Screenshot::image(target.0.clone())).observe(save_to_disk(settings.path_for(*frame)));
    }
    if *frame == settings.exit_frame {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_as_width_by_height() {
        assert_eq!(parse_size("1280x720"), Some(UVec2::new(1280, 720)));
        assert_eq!(parse_size(" 800 X 600 "), Some(UVec2::new(800, 600)));
        assert_eq!(parse_size("0x720"), None);
        assert_eq!(parse_size("1280"), None);
    }

    #[test]
    fn frames_read_as_rust_ranges() {
        assert_eq!(parse_frames("120..136"), Ok(120..136));
        assert_eq!(parse_frames("120..=135"), Ok(120..136));
        assert_eq!(parse_frames("150"), Ok(150..151));
        assert!(parse_frames("136..120").is_err());
        assert!(parse_frames("soon").is_err());
    }

    #[test]
    fn a_run_of_frames_numbers_each_file() {
        let one = FrameCapturePlugin::new("out/shot.png");
        assert_eq!(one.path_for(120), "out/shot.png");
        let sparse = FrameCapturePlugin { every: 30, ..FrameCapturePlugin::frames("shot.png", 100..200) };
        assert_eq!((100..200).filter(|&f| sparse.saves(f)).collect::<Vec<_>>(), vec![100, 130, 160, 190]);
        let run = FrameCapturePlugin::frames("out/shot.png", 120..136);
        assert_eq!(run.path_for(120), "out/shot_120.png");
        assert_eq!(run.path_for(135), "out/shot_135.png");
        assert_eq!(run.exit_frame, 136 + FrameCapturePlugin::SETTLE_FRAMES);
    }
}
