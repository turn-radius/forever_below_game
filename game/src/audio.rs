//! The level's music: a playlist that loops and fades between tracks.
//!
//! One track plays at a time. Near the end of a track the next one fades in while the current
//! one fades out, over [`MusicSettings::crossfade`] seconds, with equal-power curves. The
//! playlist wraps around. Bevy's Vorbis decoder doesn't report a duration, so it's read from
//! the Ogg stream ([`ogg_vorbis_duration`]); if that fails the next track starts when the
//! current one runs dry, fading in from silence.
//!
//! The player's music volume ([`GameSettings::music_volume`], 0 to 1) scales the mix; minus
//! and equals turn it down and up a tenth at a time ([`Action::MusicQuieter`], [`Action::MusicLouder`]),
//! and the change is heard the same frame.
//!
//! Sound effects play at [`GameSettings::sound_volume`] ([`sound_effect`]), and turning it
//! down turns down the ones already playing.
//!
//! Offscreen captures (`FB_SCREENSHOT`) stay silent. With no audio device Bevy logs a warning
//! and never creates sinks, so the music systems simply idle.

use std::f32::consts::FRAC_PI_2;

use bevy::{
    audio::{AudioSinkPlayback, Volume},
    prelude::*,
};

use crate::capture::FrameCapturePlugin;
use crate::keymap::{Action, ActionInput, Keymap};
use crate::settings::GameSettings;

/// How far one press moves the music volume.
pub const MUSIC_VOLUME_STEP: f32 = 0.1;

/// The tracks to play, as asset paths, in order. Loops back to the first after the last.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct Playlist {
    pub tracks: Vec<String>,
}

impl Default for Playlist {
    fn default() -> Self {
        Playlist {
            tracks: [
                "audio/forever-below.ogg",
                "audio/the-wizards-tower.ogg",
                "audio/shadowed-pixel.ogg",
                "audio/cavernous-arpeggios.ogg",
                "audio/unreachable-home.ogg",
                "audio/cavern-descent.ogg",
                "audio/im-not-alone.ogg",
                "audio/untitled.ogg",
            ]
            .map(String::from)
            .into(),
        }
    }
}

/// The index after `index` in a list of `len`, wrapping to 0. `None` when the list is empty.
pub fn next_index(index: usize, len: usize) -> Option<usize> {
    (len > 0).then(|| (index + 1) % len)
}

/// How loud the music is mixed and how long tracks take to fade into each other.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct MusicSettings {
    /// Linear volume at full gain, before the player's [`GameSettings::music_volume`].
    pub volume: f32,
    /// Fade length in seconds, both into a track and between tracks.
    pub crossfade: f32,
}

impl Default for MusicSettings {
    fn default() -> Self {
        MusicSettings { volume: 0.35, crossfade: 3.0 }
    }
}

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Playlist>()
            .init_resource::<Keymap>()
            .init_resource::<MusicSettings>()
            // Kept as loaded if `SettingsPlugin` came first.
            .init_resource::<GameSettings>()
            .add_systems(Startup, start_music.run_if(not(resource_exists::<FrameCapturePlugin>)))
            .add_systems(Update, ((turn_music_volume, play_music.run_if(resource_exists::<PlaylistAudio>)).chain(), turn_sound_volume));
    }
}

/// One playing track of the [`Playlist`].
#[derive(Component, Debug)]
pub struct MusicTrack {
    /// Its place in the playlist.
    pub index: usize,
    /// Length in seconds, if it could be read from the stream.
    pub duration: Option<f32>,
    /// Whether the length has been looked for. It's read once, when the sink first exists.
    pub measured: bool,
    /// Whether the next track has been started.
    pub handed_off: bool,
}

/// Handles to every playlist track, loaded up front so a hand-off never waits on the disk.
#[derive(Resource, Debug)]
struct PlaylistAudio(Vec<Handle<AudioSource>>);

fn start_music(mut commands: Commands, assets: Res<AssetServer>, playlist: Res<Playlist>) {
    let audio = PlaylistAudio(playlist.tracks.iter().map(|path| assets.load(path)).collect());
    if !audio.0.is_empty() {
        spawn_track(&mut commands, &audio, 0);
    }
    commands.insert_resource(audio);
}

fn spawn_track(commands: &mut Commands, audio: &PlaylistAudio, index: usize) {
    commands.spawn((
        Name::new("Music"),
        MusicTrack { index, duration: None, measured: false, handed_off: false },
        AudioPlayer::new(audio.0[index].clone()),
        // Silent until `play_music` sets the fade-in gain on the first frame its sink exists.
        PlaybackSettings::ONCE.with_volume(Volume::SILENT),
    ));
}

/// The music volume `volume` turned by `steps` presses, kept to 0..=1 and to whole tenths so
/// repeated presses land on round values.
pub fn turned_volume(volume: f32, steps: i32) -> f32 {
    let tenths = 1.0 / MUSIC_VOLUME_STEP;
    ((volume * tenths).round() + steps as f32).clamp(0.0, tenths) / tenths
}

pub fn turn_music_volume(input: ActionInput, mut settings: ResMut<GameSettings>) {
    let steps = input.just_pressed(Action::MusicLouder) as i32 - input.just_pressed(Action::MusicQuieter) as i32;
    if steps != 0 {
        let volume = turned_volume(settings.music_volume, steps);
        if settings.music_volume != volume {
            settings.music_volume = volume;
            info!("music: volume {volume:.1}");
        }
    }
}

/// A sound effect that plays once at the player's sound volume.
#[derive(Component, Debug, Default)]
pub struct SoundEffect;

/// What a sound effect's [`AudioPlayer`] is spawned with: played once and despawned, at
/// [`GameSettings::sound_volume`].
pub fn sound_effect(game: &GameSettings) -> (SoundEffect, PlaybackSettings) {
    (SoundEffect, PlaybackSettings::DESPAWN.with_volume(Volume::Linear(game.sound_volume.clamp(0.0, 1.0))))
}

/// Sound effects already playing follow a change of sound volume the same frame.
fn turn_sound_volume(game: Res<GameSettings>, global: Res<GlobalVolume>, mut sinks: Query<&mut AudioSink, With<SoundEffect>>) {
    if game.is_changed() {
        for mut sink in &mut sinks {
            // Bevy applies `GlobalVolume` only when it creates a sink, so it's folded in here.
            sink.set_volume(Volume::Linear(global.volume.to_linear() * game.sound_volume.clamp(0.0, 1.0)));
        }
    }
}

/// The linear gain a music sink plays at: Bevy's global volume, the mix level, the player's
/// music volume and the track's fade.
pub fn music_gain(global: f32, settings: &MusicSettings, game: &GameSettings, fade: f32) -> f32 {
    global * settings.volume * game.music_volume.clamp(0.0, 1.0) * fade
}

fn play_music(
    mut commands: Commands,
    sources: Res<Assets<AudioSource>>,
    audio: Res<PlaylistAudio>,
    settings: Res<MusicSettings>,
    game: Res<GameSettings>,
    global: Res<GlobalVolume>,
    mut tracks: Query<(Entity, &mut MusicTrack, &AudioPlayer, &mut AudioSink)>,
) {
    for (entity, mut track, player, mut sink) in &mut tracks {
        if !track.measured {
            track.measured = true;
            track.duration = sources.get(&player.0).and_then(|source| ogg_vorbis_duration(&source.bytes));
            let path = player.0.path().map(ToString::to_string).unwrap_or_default();
            info!("music: playing {path}, {:?} s", track.duration);
        }
        let position = sink.position().as_secs_f32();
        let ended = sink.empty();
        if !track.handed_off && hand_off_due(position, track.duration, ended, settings.crossfade) {
            track.handed_off = true;
            if let Some(next) = next_index(track.index, audio.0.len()) {
                spawn_track(&mut commands, &audio, next);
            }
        }
        if ended {
            commands.entity(entity).despawn();
        } else {
            let gain = track_gain(position, track.duration, settings.crossfade);
            // Bevy applies `GlobalVolume` only when it creates a sink, so it's folded in here.
            sink.set_volume(Volume::Linear(music_gain(global.volume.to_linear(), &settings, &game, gain)));
        }
    }
}

/// Whether the next track should start: inside the closing crossfade when the length is
/// known, or once the track has run dry.
pub fn hand_off_due(position: f32, duration: Option<f32>, ended: bool, crossfade: f32) -> bool {
    ended || duration.is_some_and(|duration| position >= duration - crossfade)
}

/// A track's gain from 0 to 1 at `position` seconds: an equal-power fade in over the first
/// `crossfade` seconds and, when the length is known, out over the last. During a crossfade
/// the outgoing and incoming gains squared sum to 1, so loudness holds steady.
pub fn track_gain(position: f32, duration: Option<f32>, crossfade: f32) -> f32 {
    let fade_in = fade_weight(position, crossfade);
    let fade_out = duration.map_or(1.0, |duration| fade_weight(duration - position, crossfade));
    fade_in.min(fade_out)
}

/// Equal-power ramp from 0 at `t = 0` to 1 at `t = length`.
fn fade_weight(t: f32, length: f32) -> f32 {
    if length <= 0.0 {
        return 1.0;
    }
    (FRAC_PI_2 * (t / length).clamp(0.0, 1.0)).sin()
}

/// The length in seconds of an Ogg Vorbis stream: the last page's granule position (the
/// sample count at the end of the stream) over the sample rate in the identification header.
/// `None` if the bytes aren't Ogg Vorbis.
pub fn ogg_vorbis_duration(bytes: &[u8]) -> Option<f32> {
    const PAGE: &[u8] = b"OggS";
    const IDENTIFICATION: &[u8] = b"\x01vorbis";
    if !bytes.starts_with(PAGE) {
        return None;
    }
    let ident = bytes.windows(IDENTIFICATION.len()).position(|w| w == IDENTIFICATION)?;
    // After the packet tag: version (u32), channels (u8), sample rate (u32).
    let rate_at = ident + IDENTIFICATION.len() + 5;
    let sample_rate = u32::from_le_bytes(bytes.get(rate_at..rate_at + 4)?.try_into().ok()?);
    let last_page = bytes.windows(PAGE.len()).rposition(|w| w == PAGE)?;
    // Page header: capture pattern (4), version (1), header type (1), granule position (i64).
    let granule = i64::from_le_bytes(bytes.get(last_page + 6..last_page + 14)?.try_into().ok()?);
    (sample_rate > 0 && granule > 0).then(|| granule as f32 / sample_rate as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FADE: f32 = 3.0;

    #[test]
    fn playlist_opens_on_forever_below_and_plays_all_eight_before_repeating() {
        let tracks = Playlist::default().tracks;
        assert_eq!(
            tracks,
            [
                "audio/forever-below.ogg",
                "audio/the-wizards-tower.ogg",
                "audio/shadowed-pixel.ogg",
                "audio/cavernous-arpeggios.ogg",
                "audio/unreachable-home.ogg",
                "audio/cavern-descent.ogg",
                "audio/im-not-alone.ogg",
                "audio/untitled.ogg",
            ]
        );
        let mut order = vec![0];
        while order.len() < 9 {
            order.push(next_index(*order.last().unwrap(), tracks.len()).unwrap());
        }
        assert_eq!(order, [0, 1, 2, 3, 4, 5, 6, 7, 0]);
        assert_eq!(next_index(0, 1), Some(0));
        assert_eq!(next_index(0, 0), None);
    }

    #[test]
    fn every_track_is_an_ogg_vorbis_asset() {
        for track in Playlist::default().tracks {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(&track);
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{track}: {e}"));
            let duration = ogg_vorbis_duration(&bytes).unwrap_or_else(|| panic!("{track}: not Ogg Vorbis"));
            assert!(duration > 60.0, "{track}: {duration}");
        }
    }

    #[test]
    fn hands_off_inside_the_closing_crossfade() {
        assert!(!hand_off_due(100.0, Some(184.9), false, FADE));
        assert!(!hand_off_due(181.8, Some(184.9), false, FADE));
        assert!(hand_off_due(181.9, Some(184.9), false, FADE));
    }

    #[test]
    fn hands_off_when_the_track_ends_without_a_known_length() {
        assert!(!hand_off_due(500.0, None, false, FADE));
        assert!(hand_off_due(500.0, None, true, FADE));
    }

    #[test]
    fn fades_in_from_silence() {
        assert_eq!(track_gain(0.0, None, FADE), 0.0);
        assert!(track_gain(1.0, None, FADE) >= 0.5 - 1e-6, "audible within a second");
        assert_eq!(track_gain(FADE, None, FADE), 1.0);
        assert_eq!(track_gain(60.0, Some(120.0), FADE), 1.0);
    }

    #[test]
    fn fades_out_to_silence_at_the_end() {
        assert!(track_gain(118.5, Some(120.0), FADE) < 1.0);
        assert!(track_gain(120.0, Some(120.0), FADE).abs() < 1e-6);
    }

    #[test]
    fn crossfade_keeps_power_constant() {
        let duration = 120.0;
        for step in 0..=30 {
            let into_fade = step as f32 * FADE / 30.0;
            let outgoing = track_gain(duration - FADE + into_fade, Some(duration), FADE);
            let incoming = track_gain(into_fade, Some(200.0), FADE);
            assert!((outgoing * outgoing + incoming * incoming - 1.0).abs() < 1e-5, "at {into_fade}s");
        }
    }

    #[test]
    fn a_track_shorter_than_two_fades_never_reaches_full_gain() {
        assert!(track_gain(2.0, Some(4.0), FADE) < 1.0);
    }

    #[test]
    fn music_volume_zero_is_silent_and_one_is_the_mix() {
        let mix = MusicSettings::default();
        let at = |music_volume: f32| music_gain(1.0, &mix, &GameSettings { music_volume, ..default() }, 1.0);
        assert_eq!(at(0.0), 0.0);
        assert_eq!(at(1.0), mix.volume);
        assert!((at(0.5) - mix.volume * 0.5).abs() < 1e-6);
    }

    #[test]
    fn volume_keys_step_by_tenths_within_range() {
        assert!((turned_volume(1.0, -1) - 0.9).abs() < 1e-6);
        assert_eq!(turned_volume(1.0, 1), 1.0);
        assert_eq!(turned_volume(0.0, -1), 0.0);
        assert!((turned_volume(0.33, 1) - 0.4).abs() < 1e-6);
        let mut volume = 1.0;
        for _ in 0..10 {
            volume = turned_volume(volume, -1);
        }
        assert_eq!(volume, 0.0);
    }

    #[test]
    fn a_volume_key_applies_within_the_frame() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>().init_resource::<Keymap>().init_resource::<GameSettings>().add_systems(Update, turn_music_volume);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Minus);
        app.update();
        assert!((app.world().resource::<GameSettings>().music_volume - 0.9).abs() < 1e-6);
    }

    #[test]
    fn reads_the_length_of_the_soundtrack() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/audio/the-wizards-tower.ogg");
        let duration = ogg_vorbis_duration(&std::fs::read(path).unwrap()).unwrap();
        assert!((duration - 184.93).abs() < 0.05, "{duration}");
    }

    #[test]
    fn non_vorbis_bytes_have_no_length() {
        assert_eq!(ogg_vorbis_duration(b""), None);
        assert_eq!(ogg_vorbis_duration(b"RIFF....WAVEfmt "), None);
        assert_eq!(ogg_vorbis_duration(b"OggS no header here"), None);
    }
}
