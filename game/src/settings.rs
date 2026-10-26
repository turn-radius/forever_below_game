//! The player's own settings, kept in a small JSON file between runs.
//!
//! [`GameSettings`] is read once when [`SettingsPlugin`] is added and written back whenever it
//! changes. The file lives where each platform keeps an application's support files
//! ([`SettingsFile::default_path`]); `FB_SETTINGS=path` puts it elsewhere. A missing or
//! unreadable file gives the defaults; a value out of range is pulled back into it.

use std::path::{Path, PathBuf};

use bevy::{
    prelude::*,
    window::{
        Monitor, MonitorSelection, OnMonitor, PrimaryMonitor, PrimaryWindow, VideoMode,
        VideoModeSelection, WindowMode, WindowResolution,
    },
};
use serde::{Deserialize, Serialize};

use crate::render::InkStyle;

/// Settings the player chooses, saved between runs. Fields missing from the file take their
/// defaults, so older files keep loading as settings are added.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    /// How loud the music plays, 0 (silent) to 1 (as mixed). Scales `audio::MusicSettings`.
    pub music_volume: f32,
    /// How loud sound effects play, 0 (silent) to 1 (as recorded): the door's slam.
    pub sound_volume: f32,
    /// Mouse look speed as a multiple of the usual, 0.25 to 3.
    pub mouse_sensitivity: f32,
    /// Moving the mouse up looks down.
    pub invert_mouse_y: bool,
    /// Borderless fullscreen on the window's monitor, rather than a window.
    pub fullscreen: bool,
    /// The picture's size in pixels, one of [`RESOLUTIONS`]; `None` is the desktop's. A window
    /// is opened this size; fullscreen switches the monitor to its closest video mode.
    pub resolution: Option<[u32; 2]>,
    /// Ink outlines drawn as one screen-space pass over the lit image, rather than as a hull
    /// round every part (`render::InkStyle`).
    pub screen_ink: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        GameSettings {
            music_volume: 1.0,
            sound_volume: 1.0,
            mouse_sensitivity: 1.0,
            invert_mouse_y: false,
            fullscreen: false,
            resolution: None,
            screen_ink: false,
        }
    }
}

/// The resolutions the settings offer besides the desktop's, smallest first: the common 16:9
/// and 16:10 sizes.
pub const RESOLUTIONS: [[u32; 2]; 10] = [
    [1280, 720],
    [1280, 800],
    [1440, 900],
    [1600, 900],
    [1680, 1050],
    [1920, 1080],
    [1920, 1200],
    [2560, 1440],
    [2560, 1600],
    [3840, 2160],
];

/// How a resolution reads in the menu: "1920\u{d7}1080", or "Desktop" for `None`.
pub fn resolution_label(resolution: Option<[u32; 2]>) -> String {
    match resolution {
        Some([width, height]) => format!("{width}\u{d7}{height}"),
        None => "Desktop".into(),
    }
}

/// One setting the menu shows: a slider over a range, a toggle, or a dropdown of choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Setting {
    MusicVolume,
    SoundVolume,
    MouseSensitivity,
    InvertMouseY,
    Fullscreen,
    Resolution,
    ScreenInk,
}

impl Setting {
    /// Every setting, in the order the menu lists them.
    pub const ALL: [Setting; 7] = [
        Setting::MusicVolume,
        Setting::SoundVolume,
        Setting::MouseSensitivity,
        Setting::InvertMouseY,
        Setting::Fullscreen,
        Setting::Resolution,
        Setting::ScreenInk,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Setting::MusicVolume => "Music volume",
            Setting::SoundVolume => "Sound volume",
            Setting::MouseSensitivity => "Mouse sensitivity",
            Setting::InvertMouseY => "Invert mouse Y",
            Setting::Fullscreen => "Fullscreen",
            Setting::Resolution => "Resolution",
            Setting::ScreenInk => "Screen ink",
        }
    }

    /// A dropdown's choices as the menu lists them; `None` for a slider or a toggle. Its value
    /// is the index of the chosen one.
    pub fn choices(self) -> Option<Vec<String>> {
        match self {
            Setting::Resolution => Some(
                std::iter::once(None)
                    .chain(RESOLUTIONS.map(Some))
                    .map(resolution_label)
                    .collect(),
            ),
            _ => None,
        }
    }

    /// Whether it is a toggle: neither a slider nor a dropdown.
    pub fn is_toggle(self) -> bool {
        self.range().is_none() && self.choices().is_none()
    }

    /// A slider's range; `None` for a toggle.
    pub fn range(self) -> Option<(f32, f32)> {
        match self {
            Setting::MusicVolume | Setting::SoundVolume => Some((0.0, 1.0)),
            Setting::MouseSensitivity => Some((0.25, 3.0)),
            Setting::InvertMouseY
            | Setting::Fullscreen
            | Setting::Resolution
            | Setting::ScreenInk => None,
        }
    }

    /// Its value in `settings`; a toggle is 0 or 1, a dropdown the index of its choice.
    pub fn get(self, settings: &GameSettings) -> f32 {
        match self {
            Setting::MusicVolume => settings.music_volume,
            Setting::SoundVolume => settings.sound_volume,
            Setting::MouseSensitivity => settings.mouse_sensitivity,
            Setting::InvertMouseY => settings.invert_mouse_y as u8 as f32,
            Setting::Fullscreen => settings.fullscreen as u8 as f32,
            Setting::Resolution => resolution_index(settings.resolution) as f32,
            Setting::ScreenInk => settings.screen_ink as u8 as f32,
        }
    }

    /// Sets it to `value`, pulled into its range (a non-number becomes the default); a toggle
    /// is on from 0.5, and a dropdown takes the nearest of its choices.
    pub fn set(self, settings: &mut GameSettings, value: f32) {
        let value = match self.range() {
            Some(_) if value.is_nan() => self.get(&GameSettings::default()),
            Some((low, high)) => value.clamp(low, high),
            None => value,
        };
        match self {
            Setting::MusicVolume => settings.music_volume = value,
            Setting::SoundVolume => settings.sound_volume = value,
            Setting::MouseSensitivity => settings.mouse_sensitivity = value,
            Setting::InvertMouseY => settings.invert_mouse_y = value >= 0.5,
            Setting::Fullscreen => settings.fullscreen = value >= 0.5,
            Setting::Resolution => {
                let index = if value.is_nan() {
                    0.0
                } else {
                    value.round().clamp(0.0, RESOLUTIONS.len() as f32)
                };
                settings.resolution = (index >= 1.0).then(|| RESOLUTIONS[index as usize - 1]);
            }
            Setting::ScreenInk => settings.screen_ink = value >= 0.5,
        }
    }

    /// How far along its range it is, 0 to 1 (a toggle: off 0, on 1).
    pub fn fraction(self, settings: &GameSettings) -> f32 {
        let (low, high) = self.range().unwrap_or((0.0, 1.0));
        ((self.get(settings) - low) / (high - low)).clamp(0.0, 1.0)
    }

    /// Sets it `fraction` (0 to 1) of the way along its range.
    pub fn set_fraction(self, settings: &mut GameSettings, fraction: f32) {
        let (low, high) = self.range().unwrap_or((0.0, 1.0));
        self.set(settings, low + fraction.clamp(0.0, 1.0) * (high - low));
    }

    /// Its value as the menu shows it: "70%", "1.25\u{d7}", "On".
    pub fn shown(self, settings: &GameSettings) -> String {
        let value = self.get(settings);
        match self {
            Setting::MusicVolume | Setting::SoundVolume => format!("{:.0}%", value * 100.0),
            Setting::MouseSensitivity => format!("{value:.2}\u{d7}"),
            Setting::Resolution => resolution_label(settings.resolution),
            Setting::InvertMouseY | Setting::Fullscreen | Setting::ScreenInk => {
                if value >= 0.5 { "On" } else { "Off" }.into()
            }
        }
    }
}

/// Where `resolution` is in the Resolution dropdown: 0 for the desktop's, else 1 past its place
/// in [`RESOLUTIONS`]. A size not offered takes the place of the one nearest it.
fn resolution_index(resolution: Option<[u32; 2]>) -> usize {
    let Some([width, height]) = resolution else {
        return 0;
    };
    let off = |[w, h]: [u32; 2]| w.abs_diff(width) + h.abs_diff(height);
    (0..RESOLUTIONS.len())
        .min_by_key(|&i| off(RESOLUTIONS[i]))
        .map_or(0, |i| i + 1)
}

impl GameSettings {
    /// These settings with every value pulled into its range (a non-number becomes the default).
    pub fn clamped(mut self) -> Self {
        for setting in Setting::ALL {
            let value = setting.get(&self);
            setting.set(&mut self, value);
        }
        self
    }

    /// The settings saved at `path`, or the defaults if there are none or they can't be read.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<GameSettings>(&text)
                .map(GameSettings::clamped)
                .unwrap_or_else(|error| {
                    warn!(
                        "settings: {} is unreadable ({error}); using the defaults",
                        path.display()
                    );
                    GameSettings::default()
                }),
            Err(_) => GameSettings::default(),
        }
    }

    /// Writes these settings to `path`, making its folder if need be. Written beside it and
    /// renamed into place, so a crash mid-write never leaves half a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let partial = path.with_extension("json.partial");
        std::fs::write(&partial, text + "\n")?;
        std::fs::rename(&partial, path)
    }
}

/// Where [`GameSettings`] are saved; `None` keeps them for this run only.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsFile(pub Option<PathBuf>);

impl SettingsFile {
    /// `FB_SETTINGS` if set, else the platform's place for an application's own files:
    /// - macOS: `~/Library/Application Support/Forever Below/settings.json`, Apple's
    ///   Application Support directory ("File System Programming Guide", File System Basics:
    ///   <https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemOverview/FileSystemOverview.html>).
    /// - Windows: `%APPDATA%\Forever Below\settings.json` (`FOLDERID_RoamingAppData`:
    ///   <https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid>).
    /// - Elsewhere: `$XDG_CONFIG_HOME/forever-below/settings.json`, falling back to
    ///   `~/.config` (XDG Base Directory Specification:
    ///   <https://specifications.freedesktop.org/basedir-spec/latest/>).
    pub fn default_path() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("FB_SETTINGS").filter(|p| !p.is_empty()) {
            return Some(path.into());
        }
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from);
        if cfg!(target_os = "macos") {
            home.map(|h| {
                h.join("Library")
                    .join("Application Support")
                    .join("Forever Below")
                    .join("settings.json")
            })
        } else if cfg!(windows) {
            std::env::var_os("APPDATA")
                .map(|a| PathBuf::from(a).join("Forever Below").join("settings.json"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .filter(|c| !c.is_empty())
                .map(PathBuf::from)
                .or_else(|| home.map(|h| h.join(".config")))
                .map(|c| c.join("forever-below").join("settings.json"))
        }
    }
}

/// Loads [`GameSettings`] from a [`SettingsFile`] and saves them when they change. Without a
/// `SettingsFile` resource already in the app it uses [`SettingsFile::default_path`].
pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<SettingsFile>() {
            app.insert_resource(SettingsFile(SettingsFile::default_path()));
        }
        let file = app.world().resource::<SettingsFile>().clone();
        let settings = file
            .0
            .as_deref()
            .map(GameSettings::load)
            .unwrap_or_default();
        app.insert_resource(settings)
            .add_systems(Update, (apply_window_settings, apply_ink_setting))
            .add_systems(Last, save_settings);
    }
}

/// The window mode for the fullscreen and resolution settings. Fullscreen at the desktop's
/// resolution is borderless on the window's own monitor, which switches without a mode change;
/// at a chosen one it is exclusive, in the monitor's video mode `closest_mode` finds among
/// `modes`. Borderless, too, while the monitor's modes are not yet known.
pub fn window_mode(
    fullscreen: bool,
    resolution: Option<[u32; 2]>,
    modes: &[VideoMode],
) -> WindowMode {
    if !fullscreen {
        return WindowMode::Windowed;
    }
    match resolution.and_then(|size| closest_mode(size, modes)) {
        Some(mode) => WindowMode::Fullscreen(
            MonitorSelection::Current,
            VideoModeSelection::Specific(mode),
        ),
        None => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
    }
}

/// The video mode in `modes` nearest `size` in pixels, at its fastest refresh and deepest colour.
pub fn closest_mode([width, height]: [u32; 2], modes: &[VideoMode]) -> Option<VideoMode> {
    modes.iter().copied().min_by_key(|mode| {
        let off = mode.physical_size.x.abs_diff(width) + mode.physical_size.y.abs_diff(height);
        (
            off,
            std::cmp::Reverse(mode.refresh_rate_millihertz),
            std::cmp::Reverse(mode.bit_depth),
        )
    })
}

/// Switches the ink outlines to the style the settings ask for, at the start and whenever they
/// change. `FB_INK` for the run (`InkStyle::from_env`) wins over the setting.
fn apply_ink_setting(settings: Res<GameSettings>, style: Option<ResMut<InkStyle>>) {
    let Some(mut style) = style else { return };
    if settings.is_changed() && InkStyle::from_env().is_none() {
        style.set_if_neq(if settings.screen_ink {
            InkStyle::Screen
        } else {
            InkStyle::Hull
        });
    }
}

/// Puts the window in the mode and size the settings ask for, at the start, whenever they change
/// and when its monitor's video modes come in. A window is sized only when the resolution
/// changes, so one dragged to a new size keeps it; going back to the desktop's resolution puts
/// a window back at its usual size.
#[allow(clippy::type_complexity)]
fn apply_window_settings(
    settings: Res<GameSettings>,
    window: Option<Single<(&mut Window, Option<Ref<OnMonitor>>), With<PrimaryWindow>>>,
    monitors: Query<(Ref<Monitor>, Has<PrimaryMonitor>)>,
    mut sized: Local<Option<[u32; 2]>>,
) {
    let Some(window) = window else { return };
    let (mut window, on) = window.into_inner();
    let monitor = on
        .as_ref()
        .and_then(|on| monitors.get(on.0).ok())
        .or_else(|| monitors.iter().find(|(_, primary)| *primary));
    let monitor_news = on.as_ref().is_some_and(Ref::is_changed)
        || monitor.as_ref().is_some_and(|(m, _)| m.is_changed());
    if !settings.is_changed() && !window.is_added() && !monitor_news {
        return;
    }
    let modes = monitor
        .as_ref()
        .map_or(&[][..], |(m, _)| &m.video_modes[..]);
    let mode = window_mode(settings.fullscreen, settings.resolution, modes);
    if window.mode != mode {
        window.mode = mode;
    }
    if !settings.fullscreen && settings.resolution != *sized {
        match settings.resolution {
            Some([width, height]) => window.resolution.set_physical_resolution(width, height),
            None => {
                let usual = WindowResolution::default();
                window.resolution.set(usual.width(), usual.height());
            }
        }
        *sized = settings.resolution;
    }
}

fn save_settings(settings: Res<GameSettings>, file: Res<SettingsFile>) {
    if !settings.is_changed() || settings.is_added() {
        return;
    }
    if let Some(path) = &file.0
        && let Err(error) = settings.save(path)
    {
        warn!("settings: could not save {}: {error}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A settings path of its own in the temp folder, cleared first.
    fn scratch(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("forever-below-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder.join("settings.json")
    }

    #[test]
    fn missing_or_broken_files_give_the_defaults() {
        let path = scratch("missing");
        assert_eq!(GameSettings::load(&path), GameSettings::default());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(GameSettings::load(&path), GameSettings::default());
        // Unknown and missing keys are fine; out-of-range values are pulled in.
        std::fs::write(&path, r#"{"music_volume": 3.0, "something_new": true}"#).unwrap();
        assert_eq!(GameSettings::load(&path).music_volume, 1.0);
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(GameSettings::load(&path), GameSettings::default());
    }

    #[test]
    fn older_files_keep_their_music_and_take_defaults_for_the_rest() {
        let path = scratch("older");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"music_volume": 0.4}"#).unwrap();
        assert_eq!(
            GameSettings::load(&path),
            GameSettings {
                music_volume: 0.4,
                ..GameSettings::default()
            }
        );
        std::fs::write(
            &path,
            r#"{"sound_volume": -1, "mouse_sensitivity": 9, "invert_mouse_y": true}"#,
        )
        .unwrap();
        let loaded = GameSettings::load(&path);
        assert_eq!(
            (
                loaded.sound_volume,
                loaded.mouse_sensitivity,
                loaded.invert_mouse_y,
                loaded.fullscreen
            ),
            (0.0, 3.0, true, false)
        );
    }

    #[test]
    fn sliders_and_toggles_stay_in_range() {
        let mut settings = GameSettings::default();
        Setting::MouseSensitivity.set_fraction(&mut settings, 0.0);
        assert_eq!(settings.mouse_sensitivity, 0.25);
        Setting::MouseSensitivity.set_fraction(&mut settings, 2.0);
        assert_eq!(settings.mouse_sensitivity, 3.0);
        assert_eq!(Setting::MouseSensitivity.fraction(&settings), 1.0);
        Setting::SoundVolume.set(&mut settings, f32::NAN);
        assert_eq!(settings.sound_volume, 1.0);
        Setting::SoundVolume.set_fraction(&mut settings, 0.25);
        assert_eq!(Setting::SoundVolume.shown(&settings), "25%");
        Setting::Fullscreen.set_fraction(&mut settings, 1.0);
        assert!(settings.fullscreen);
        assert_eq!(Setting::Fullscreen.shown(&settings), "On");
        assert_eq!(
            Setting::MouseSensitivity.shown(&GameSettings::default()),
            "1.00\u{d7}"
        );
        for setting in Setting::ALL {
            let default = GameSettings::default();
            let mut moved = default;
            setting.set_fraction(&mut moved, setting.fraction(&default));
            assert_eq!(
                moved, default,
                "{setting:?} round-trips through its fraction"
            );
        }
    }

    #[test]
    fn fullscreen_applies_to_the_window_within_the_frame() {
        let mut app = App::new();
        app.insert_resource(SettingsFile(None))
            .add_plugins(SettingsPlugin);
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Window>(window).unwrap().mode,
            WindowMode::Windowed
        );
        app.world_mut().resource_mut::<GameSettings>().fullscreen = true;
        app.update();
        assert_eq!(
            app.world().get::<Window>(window).unwrap().mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        );
    }

    #[test]
    fn the_resolution_is_one_of_its_choices() {
        let choices = Setting::Resolution.choices().unwrap();
        assert_eq!(choices.len(), RESOLUTIONS.len() + 1);
        assert_eq!(
            (
                choices[0].as_str(),
                Setting::Resolution.shown(&GameSettings::default())
            ),
            ("Desktop", "Desktop".into())
        );
        let mut settings = GameSettings::default();
        for (index, choice) in choices.iter().enumerate() {
            Setting::Resolution.set(&mut settings, index as f32);
            assert_eq!(Setting::Resolution.get(&settings), index as f32);
            assert_eq!(&Setting::Resolution.shown(&settings), choice);
        }
        assert_eq!(settings.resolution, Some([3840, 2160]));
        // A size not offered, from an older or hand-edited file, takes the nearest one.
        let path = scratch("resolution");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"resolution": [1910, 1075]}"#).unwrap();
        assert_eq!(GameSettings::load(&path).resolution, Some([1920, 1080]));
        std::fs::write(&path, r#"{"resolution": null}"#).unwrap();
        assert_eq!(GameSettings::load(&path).resolution, None);
    }

    fn mode(width: u32, height: u32, hz: u32) -> VideoMode {
        VideoMode {
            physical_size: UVec2::new(width, height),
            bit_depth: 32,
            refresh_rate_millihertz: hz * 1000,
        }
    }

    #[test]
    fn fullscreen_takes_the_closest_video_mode() {
        let modes = [
            mode(1920, 1080, 60),
            mode(1920, 1080, 120),
            mode(2560, 1600, 60),
            mode(1280, 800, 60),
        ];
        assert_eq!(
            closest_mode([1920, 1080], &modes),
            Some(mode(1920, 1080, 120)),
            "the fastest of the exact size"
        );
        assert_eq!(
            closest_mode([2560, 1440], &modes),
            Some(mode(2560, 1600, 60))
        );
        assert_eq!(closest_mode([1280, 720], &[]), None);
        assert_eq!(
            window_mode(false, Some([1920, 1080]), &modes),
            WindowMode::Windowed
        );
        assert_eq!(
            window_mode(true, None, &modes),
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        );
        assert_eq!(
            window_mode(true, Some([1920, 1080]), &[]),
            WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            "until the modes are known"
        );
        assert_eq!(
            window_mode(true, Some([1920, 1080]), &modes),
            WindowMode::Fullscreen(
                MonitorSelection::Current,
                VideoModeSelection::Specific(mode(1920, 1080, 120))
            )
        );
    }

    #[test]
    fn the_resolution_sizes_the_window_and_its_monitor_modes() {
        let mut app = App::new();
        app.insert_resource(SettingsFile(None))
            .add_plugins(SettingsPlugin);
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.update();
        let size = |app: &App| {
            app.world()
                .get::<Window>(window)
                .unwrap()
                .resolution
                .physical_size()
        };
        assert_eq!(
            size(&app),
            UVec2::new(1280, 720),
            "the desktop's resolution leaves the window as it opened"
        );

        app.world_mut().resource_mut::<GameSettings>().resolution = Some([1600, 900]);
        app.update();
        assert_eq!(size(&app), UVec2::new(1600, 900));
        // Dragged to a new size, it keeps it through other changes.
        app.world_mut()
            .get_mut::<Window>(window)
            .unwrap()
            .resolution
            .set_physical_resolution(1000, 700);
        app.world_mut().resource_mut::<GameSettings>().music_volume = 0.5;
        app.update();
        assert_eq!(size(&app), UVec2::new(1000, 700));

        // Fullscreen goes borderless until the monitor's modes come in, then takes the closest.
        app.world_mut().resource_mut::<GameSettings>().fullscreen = true;
        app.update();
        assert_eq!(
            app.world().get::<Window>(window).unwrap().mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        );
        let monitor = app
            .world_mut()
            .spawn(Monitor {
                name: None,
                physical_height: 1080,
                physical_width: 1920,
                physical_position: IVec2::ZERO,
                refresh_rate_millihertz: Some(60_000),
                scale_factor: 1.0,
                video_modes: vec![mode(1920, 1080, 60), mode(1600, 900, 60)],
            })
            .id();
        app.world_mut()
            .entity_mut(window)
            .insert(OnMonitor(monitor));
        app.update();
        assert_eq!(
            app.world().get::<Window>(window).unwrap().mode,
            WindowMode::Fullscreen(
                MonitorSelection::Current,
                VideoModeSelection::Specific(mode(1600, 900, 60))
            )
        );

        app.world_mut().resource_mut::<GameSettings>().fullscreen = false;
        app.world_mut().resource_mut::<GameSettings>().resolution = None;
        app.update();
        assert_eq!(
            app.world().get::<Window>(window).unwrap().mode,
            WindowMode::Windowed
        );
        assert_eq!(
            size(&app),
            UVec2::new(1280, 720),
            "back to the desktop's: the window's usual size"
        );
    }

    #[test]
    fn screen_ink_switches_the_ink_style() {
        let mut app = App::new();
        app.insert_resource(SettingsFile(None))
            .insert_resource(InkStyle::Hull)
            .add_plugins(SettingsPlugin);
        app.update();
        assert_eq!(*app.world().resource::<InkStyle>(), InkStyle::Hull);
        app.world_mut().resource_mut::<GameSettings>().screen_ink = true;
        app.update();
        assert_eq!(*app.world().resource::<InkStyle>(), InkStyle::Screen);
        assert_eq!(
            Setting::ScreenInk.shown(app.world().resource::<GameSettings>()),
            "On"
        );
        app.world_mut().resource_mut::<GameSettings>().screen_ink = false;
        app.update();
        assert_eq!(*app.world().resource::<InkStyle>(), InkStyle::Hull);
    }

    #[test]
    fn settings_survive_a_restart() {
        let path = scratch("restart");
        let file = SettingsFile(Some(path.clone()));
        let mut first = App::new();
        first
            .insert_resource(file.clone())
            .add_plugins(SettingsPlugin);
        first.update();
        assert!(!path.exists(), "nothing written until something changes");
        let chosen = GameSettings {
            music_volume: 0.3,
            sound_volume: 0.6,
            mouse_sensitivity: 1.5,
            invert_mouse_y: true,
            fullscreen: true,
            resolution: Some([2560, 1440]),
            screen_ink: true,
        };
        *first.world_mut().resource_mut::<GameSettings>() = chosen;
        first.update();
        assert!(path.exists());

        let mut second = App::new();
        second.insert_resource(file).add_plugins(SettingsPlugin);
        assert_eq!(*second.world().resource::<GameSettings>(), chosen);
    }

    #[test]
    fn the_default_path_is_the_platforms() {
        // Only the shape: the environment decides the rest.
        if std::env::var_os("FB_SETTINGS").is_none()
            && let Some(path) = SettingsFile::default_path()
        {
            assert!(path.ends_with("settings.json"), "{}", path.display());
            if cfg!(target_os = "macos") {
                assert!(
                    path.ends_with("Library/Application Support/Forever Below/settings.json"),
                    "{}",
                    path.display()
                );
            }
        }
    }
}
