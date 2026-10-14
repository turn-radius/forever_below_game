//! Offscreen frame capture: render every camera to an image, save one frame, quit.
//! Used to check scenes visually where window screenshots come back black.
//!
//! `FB_SCREENSHOT=path.png cargo run -p forever_below` turns it on via [`FrameCapturePlugin::from_env`].
//!
//! A capture shows exactly what its command line and environment ask for. The window still
//! opens, and on macOS it takes focus, so keys typed meant for a terminal (two W presses step
//! the turntable to battle worn) would otherwise reach the scene; [`IgnoreInput`] drops them.

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

/// Retargets the cameras that draw to the window to an offscreen image, saves frame `capture_frame` to `path` and exits
/// at `exit_frame`. Frames before the capture let assets load and shaders compile.
#[derive(Resource, Clone, Debug)]
pub struct FrameCapturePlugin {
    pub path: String,
    pub size: UVec2,
    pub capture_frame: u32,
    pub exit_frame: u32,
}

impl FrameCapturePlugin {
    pub fn new(path: impl Into<String>) -> Self {
        FrameCapturePlugin { path: path.into(), size: UVec2::new(1920, 1080), capture_frame: 120, exit_frame: 180 }
    }

    /// The plugin when `FB_SCREENSHOT` is set.
    pub fn from_env() -> Option<Self> {
        std::env::var("FB_SCREENSHOT").ok().filter(|path| !path.is_empty()).map(FrameCapturePlugin::new)
    }
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
    if *frame == settings.capture_frame {
        commands.spawn(Screenshot::image(target.0.clone())).observe(save_to_disk(settings.path.clone()));
    }
    if *frame == settings.exit_frame {
        exit.write(AppExit::Success);
    }
}
