//! The player's own settings, kept in a small JSON file between runs.
//!
//! [`GameSettings`] is read once when [`SettingsPlugin`] is added and written back whenever it
//! changes. The file lives where each platform keeps an application's support files
//! ([`SettingsFile::default_path`]); `FB_SETTINGS=path` puts it elsewhere. A missing or
//! unreadable file gives the defaults; a value out of range is pulled back into it.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Settings the player chooses, saved between runs. Fields missing from the file take their
/// defaults, so older files keep loading as settings are added.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    /// How loud the music plays, 0 (silent) to 1 (as mixed). Scales `audio::MusicSettings`.
    pub music_volume: f32,
}

impl Default for GameSettings {
    fn default() -> Self {
        GameSettings { music_volume: 1.0 }
    }
}

impl GameSettings {
    /// These settings with every value pulled into its range (a non-number becomes the default).
    pub fn clamped(self) -> Self {
        let music_volume = if self.music_volume.is_nan() { GameSettings::default().music_volume } else { self.music_volume.clamp(0.0, 1.0) };
        GameSettings { music_volume }
    }

    /// The settings saved at `path`, or the defaults if there are none or they can't be read.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<GameSettings>(&text)
                .map(GameSettings::clamped)
                .unwrap_or_else(|error| {
                    warn!("settings: {} is unreadable ({error}); using the defaults", path.display());
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
        let home = std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from);
        if cfg!(target_os = "macos") {
            home.map(|h| h.join("Library").join("Application Support").join("Forever Below").join("settings.json"))
        } else if cfg!(windows) {
            std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Forever Below").join("settings.json"))
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
        let settings = file.0.as_deref().map(GameSettings::load).unwrap_or_default();
        app.insert_resource(settings).add_systems(Last, save_settings);
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
        let folder = std::env::temp_dir().join(format!("forever-below-test-{}-{name}", std::process::id()));
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
    fn settings_survive_a_restart() {
        let path = scratch("restart");
        let file = SettingsFile(Some(path.clone()));
        let mut first = App::new();
        first.insert_resource(file.clone()).add_plugins(SettingsPlugin);
        first.update();
        assert!(!path.exists(), "nothing written until something changes");
        first.world_mut().resource_mut::<GameSettings>().music_volume = 0.3;
        first.update();
        assert!(path.exists());

        let mut second = App::new();
        second.insert_resource(file).add_plugins(SettingsPlugin);
        assert_eq!(second.world().resource::<GameSettings>().music_volume, 0.3);
    }

    #[test]
    fn the_default_path_is_the_platforms() {
        // Only the shape: the environment decides the rest.
        if std::env::var_os("FB_SETTINGS").is_none()
            && let Some(path) = SettingsFile::default_path()
        {
            assert!(path.ends_with("settings.json"), "{}", path.display());
            if cfg!(target_os = "macos") {
                assert!(path.ends_with("Library/Application Support/Forever Below/settings.json"), "{}", path.display());
            }
        }
    }
}
