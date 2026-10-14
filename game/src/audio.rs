//! The game's music: Forever Below to open, then tracks drawn at random, with silence between
//! them.
//!
//! The game opens on the first track of the [`Playlist`], Forever Below: on the title screen,
//! or in the level when the game opens straight into it. When a delve begins from the title
//! the track is cut short, fading out over [`MusicSettings::fade`] ([`MusicTrack::cut_short`]),
//! and the level's music takes over after the silence that follows. After that, each time a track ends
//! the music falls silent for a while ([`MusicSettings::gap`], about a minute) and then a track
//! drawn at random from the playlist starts, never the one just heard. Each track fades in and
//! out over [`MusicSettings::fade`] seconds with an equal-power curve, so nothing overlaps.
//! Bevy's Vorbis decoder doesn't report a duration, so it's read from the Ogg stream
//! ([`ogg_vorbis_duration`]) to start the fade-out in time; if that fails the track simply
//! plays to its end.
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

use procgen_rng::Mulberry32;

use crate::capture::FrameCapturePlugin;
use crate::keymap::{Action, ActionInput, Keymap};
use crate::launch::AppState;
use crate::settings::GameSettings;

/// How far one press moves the music volume.
pub const MUSIC_VOLUME_STEP: f32 = 0.1;

/// The tracks music is drawn from, as asset paths. The first opens the game.
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

/// A track drawn at random from a playlist of `len`, never `last` unless it is the only one.
/// `None` when the playlist is empty.
pub fn pick_track(rng: &mut Mulberry32, len: usize, last: Option<usize>) -> Option<usize> {
    match (len, last) {
        (0, _) => None,
        (1, _) => Some(0),
        // Draw from the others: skip over `last` so every other track is equally likely.
        (_, Some(last)) if last < len => {
            let i = (rng.next_f64() * (len - 1) as f64) as usize % (len - 1);
            Some(if i >= last { i + 1 } else { i })
        }
        _ => Some((rng.next_f64() * len as f64) as usize % len),
    }
}

/// A silence in seconds drawn evenly from `gap`.
pub fn silence_seconds(rng: &mut Mulberry32, gap: (f32, f32)) -> f32 {
    rng.range(gap.0 as f64, gap.1.max(gap.0) as f64) as f32
}

/// How loud the music is mixed, how tracks fade, and how long the silences between them last.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct MusicSettings {
    /// Linear volume at full gain, before the player's [`GameSettings::music_volume`].
    pub volume: f32,
    /// Fade length in seconds, into and out of each track.
    pub fade: f32,
    /// Silence between tracks in seconds, drawn evenly from this range.
    pub gap: (f32, f32),
}

impl Default for MusicSettings {
    fn default() -> Self {
        MusicSettings { volume: 0.35, fade: 3.0, gap: (45.0, 75.0) }
    }
}

/// What the music does next: the track last heard, and when the silence after it ends.
#[derive(Resource, Debug)]
struct MusicSchedule {
    rng: Mulberry32,
    last: Option<usize>,
    /// Real seconds since startup at which the next track starts; `None` while one plays.
    resume_at: Option<f32>,
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
            .add_systems(OnExit(AppState::Title), fade_out_for_the_delve)
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
    /// Where in the track, in seconds, it was cut short, if it was: it has faded out by then.
    pub cut: Option<f32>,
}

impl MusicTrack {
    /// Where the track ends, in seconds: where it was cut, or its own end. `None` when neither
    /// is known.
    pub fn end(&self) -> Option<f32> {
        match (self.duration, self.cut) {
            (Some(duration), Some(cut)) => Some(duration.min(cut)),
            (duration, cut) => cut.or(duration),
        }
    }

    /// Cuts the track short from `position` seconds in: it fades out over `fade` seconds from
    /// there, then stops as if it had ended.
    pub fn cut_short(&mut self, position: f32, fade: f32) {
        let at = position + fade.max(0.0);
        self.cut = Some(self.cut.map_or(at, |cut| cut.min(at)));
    }

    /// Whether the track has played to its cut.
    pub fn is_cut_off(&self, position: f32) -> bool {
        self.cut.is_some_and(|cut| position >= cut)
    }
}

/// The track playing on the title fades out as the delve begins.
pub fn fade_out_for_the_delve(settings: Res<MusicSettings>, mut tracks: Query<(&mut MusicTrack, Option<&AudioSink>)>) {
    for (mut track, sink) in &mut tracks {
        let position = sink.map_or(0.0, |sink| sink.position().as_secs_f32());
        track.cut_short(position, settings.fade);
        info!("music: fading out at {position:.1} s for the delve");
    }
}

/// Handles to every playlist track, loaded up front so a track never waits on the disk.
#[derive(Resource, Debug)]
struct PlaylistAudio(Vec<Handle<AudioSource>>);

fn start_music(mut commands: Commands, assets: Res<AssetServer>, playlist: Res<Playlist>) {
    let audio = PlaylistAudio(playlist.tracks.iter().map(|path| assets.load(path)).collect());
    if !audio.0.is_empty() {
        spawn_track(&mut commands, &audio, 0);
    }
    // Music isn't part of the seeded world, so each run draws its own order.
    commands.insert_resource(MusicSchedule { rng: Mulberry32::new(crate::launch::clock_seed()), last: (!audio.0.is_empty()).then_some(0), resume_at: None });
    commands.insert_resource(audio);
}

fn spawn_track(commands: &mut Commands, audio: &PlaylistAudio, index: usize) {
    commands.spawn((
        Name::new("Music"),
        MusicTrack { index, duration: None, measured: false, cut: None },
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

#[allow(clippy::too_many_arguments)]
fn play_music(
    mut commands: Commands,
    sources: Res<Assets<AudioSource>>,
    audio: Res<PlaylistAudio>,
    settings: Res<MusicSettings>,
    game: Res<GameSettings>,
    global: Res<GlobalVolume>,
    time: Res<Time<Real>>,
    mut schedule: ResMut<MusicSchedule>,
    mut tracks: Query<(Entity, &mut MusicTrack, &AudioPlayer, &mut AudioSink)>,
) {
    let now = time.elapsed_secs();
    if let Some(at) = schedule.resume_at
        && now >= at
    {
        let last = schedule.last;
        if let Some(next) = pick_track(&mut schedule.rng, audio.0.len(), last) {
            spawn_track(&mut commands, &audio, next);
            schedule.last = Some(next);
        }
        schedule.resume_at = None;
    }
    for (entity, mut track, player, mut sink) in &mut tracks {
        if !track.measured {
            track.measured = true;
            track.duration = sources.get(&player.0).and_then(|source| ogg_vorbis_duration(&source.bytes));
            let path = player.0.path().map(ToString::to_string).unwrap_or_default();
            info!("music: playing {path}, {:?} s", track.duration);
        }
        let position = sink.position().as_secs_f32();
        if sink.empty() || track.is_cut_off(position) {
            sink.stop();
            commands.entity(entity).despawn();
            let silence = silence_seconds(&mut schedule.rng, settings.gap);
            schedule.resume_at = Some(now + silence);
            info!("music: silence for {silence:.0} s");
        } else {
            let gain = track_gain(position, track.end(), settings.fade);
            // Bevy applies `GlobalVolume` only when it creates a sink, so it's folded in here.
            sink.set_volume(Volume::Linear(music_gain(global.volume.to_linear(), &settings, &game, gain)));
        }
    }
}

/// A track's gain from 0 to 1 at `position` seconds: an equal-power fade in over the first
/// `fade` seconds and, when the length is known, out over the last.
pub fn track_gain(position: f32, duration: Option<f32>, fade: f32) -> f32 {
    let fade_in = fade_weight(position, fade);
    let fade_out = duration.map_or(1.0, |duration| fade_weight(duration - position, fade));
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
    fn playlist_opens_on_forever_below_and_holds_all_eight_tracks() {
        let tracks = Playlist::default().tracks;
        assert_eq!(tracks.len(), 8);
        assert_eq!(tracks[0], "audio/forever-below.ogg");
    }

    #[test]
    fn tracks_are_drawn_at_random_and_never_twice_running() {
        let mut rng = Mulberry32::new(7);
        let mut seen = [0usize; 8];
        let mut last = Some(0);
        for _ in 0..4000 {
            let next = pick_track(&mut rng, 8, last).unwrap();
            assert_ne!(Some(next), last);
            seen[next] += 1;
            last = Some(next);
        }
        // Every track turns up, each roughly an eighth of the time.
        assert!(seen.iter().all(|&n| (350..650).contains(&n)), "{seen:?}");
        assert_eq!(pick_track(&mut rng, 1, Some(0)), Some(0));
        assert_eq!(pick_track(&mut rng, 0, None), None);
    }

    #[test]
    fn silences_last_about_a_minute() {
        let mut rng = Mulberry32::new(3);
        let gap = MusicSettings::default().gap;
        for _ in 0..1000 {
            let s = silence_seconds(&mut rng, gap);
            assert!((45.0..=75.0).contains(&s), "{s}");
        }
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
    fn a_track_shorter_than_two_fades_never_reaches_full_gain() {
        assert!(track_gain(2.0, Some(4.0), FADE) < 1.0);
    }

    #[test]
    fn the_title_track_fades_out_as_the_delve_begins_and_does_not_come_straight_back() {
        let mut track = MusicTrack { index: 0, duration: Some(200.0), measured: true, cut: None };
        assert_eq!(track.end(), Some(200.0));
        track.cut_short(40.0, FADE);
        assert_eq!(track.end(), Some(43.0));
        assert_eq!(track_gain(40.0, track.end(), FADE), 1.0, "no jump when the delve begins");
        let falling: Vec<f32> = [40.5, 41.0, 42.0, 42.9].iter().map(|&t| track_gain(t, track.end(), FADE)).collect();
        assert!(falling.windows(2).all(|w| w[1] < w[0]), "{falling:?}");
        assert!(!track.is_cut_off(42.9));
        assert!(track.is_cut_off(43.0));
        assert!(track_gain(43.0, track.end(), FADE).abs() < 1e-6);
        // A second cut never lengthens the track.
        track.cut_short(42.0, FADE);
        assert_eq!(track.end(), Some(43.0));
        // After the silence the level's music is drawn from the others, never Forever Below.
        let mut rng = Mulberry32::new(11);
        assert!((0..500).all(|_| pick_track(&mut rng, Playlist::default().tracks.len(), Some(0)) != Some(0)));
    }

    #[test]
    fn a_cut_short_track_with_no_known_length_still_ends() {
        let mut track = MusicTrack { index: 0, duration: None, measured: true, cut: None };
        assert_eq!(track.end(), None);
        track.cut_short(1.0, FADE);
        assert_eq!(track.end(), Some(4.0));
    }

    #[test]
    fn leaving_the_title_fades_out_what_is_playing() {
        use bevy::state::app::StatesPlugin;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Title)
            .init_resource::<MusicSettings>()
            .add_systems(OnExit(AppState::Title), fade_out_for_the_delve);
        let track = app.world_mut().spawn(MusicTrack { index: 0, duration: Some(120.0), measured: false, cut: None }).id();
        app.update();
        assert_eq!(app.world().get::<MusicTrack>(track).unwrap().cut, None, "it plays on over the title");
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Delving);
        app.update();
        // No sink without an audio device, so it fades from the start.
        assert_eq!(app.world().get::<MusicTrack>(track).unwrap().cut, Some(MusicSettings::default().fade));
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
