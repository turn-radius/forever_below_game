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
//! Two themes loop in place of the playlist ([`loop_theme`]), the theme fading in as what
//! played fades out, and fade out in their turn ([`end_theme`]):
//!
//! - **Measured Pulse** ([`Playlist::creator_theme`]) from the moment the character creator
//!   opens until the delve begins. Going back to the title's buttons fades it out and the title
//!   track in again ([`back_to_the_title_track`]).
//! - **Trolls** ([`Playlist::troll_theme`]) while a troll not yet burned to death is within
//!   [`SoundTuning::troll_rooms`] rooms of the delver ([`fb_level::RoomGraph::rooms_apart`]),
//!   and for [`SoundTuning::troll_linger`] seconds after, so it doesn't stop and start on a
//!   doorstep. It ends with the delve.
//!
//! The silence after a track starts only once no other track plays, so nothing is drawn over a
//! theme, and the playlist picks up again after the silence that follows one.
//!
//! The player's music volume ([`GameSettings::music_volume`], 0 to 1) scales the mix; minus
//! and equals turn it down and up a tenth at a time ([`Action::MusicQuieter`], [`Action::MusicLouder`]),
//! and the change is heard the same frame.
//!
//! Sound effects play at [`GameSettings::sound_volume`] ([`sound_effect`]), and turning it
//! down turns down the ones already playing.
//!
//! The mix level, the silences and the effects' level are tuned ([`SoundTuning`]), and follow
//! the tuning file as it changes.
//!
//! Offscreen captures (`FB_SCREENSHOT`) stay silent. With no audio device Bevy logs a warning
//! and never creates sinks, so the music systems simply idle.

use std::f32::consts::FRAC_PI_2;

use bevy::{
    audio::{AudioSinkPlayback, SpatialScale, Volume},
    prelude::*,
};

use procgen_rng::Mulberry32;

use crate::capture::FrameCapturePlugin;
use crate::keymap::{Action, ActionInput, Keymap};
use crate::launch::AppState;
use crate::settings::GameSettings;
use crate::tuning::{SoundTuning, Tuning};

/// How far one press moves the music volume.
pub const MUSIC_VOLUME_STEP: f32 = 0.1;

/// The tracks music is drawn from, as asset paths. The first opens the game.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct Playlist {
    pub tracks: Vec<String>,
    /// The track that loops while a delver is made, never drawn at random.
    pub creator_theme: String,
    /// The track that loops while a troll is near, never drawn at random.
    pub troll_theme: String,
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
                "audio/let-me-think.ogg",
                "audio/meant-to-be-here.ogg",
                "audio/what-am-i-doing.ogg",
                "audio/am-i-seeing-the-light.ogg",
                "audio/bitter-pulse.ogg",
                "audio/descenso-al-infierno.ogg",
                "audio/falling-apart.ogg",
                "audio/i-should-have-stayed.ogg",
                "audio/i-went-too-far.ogg",
                "audio/im-right-where-i-started.ogg",
                "audio/missing-friends.ogg",
                "audio/sinking-bass.ogg",
            ]
            .map(String::from)
            .into(),
            creator_theme: "audio/measured-pulse.ogg".into(),
            troll_theme: "audio/trolls.ogg".into(),
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
/// The mix and the silences follow the tuning ([`MusicSettings::tuned`]).
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
        MusicSettings::tuned(&SoundTuning::default(), 3.0)
    }
}

impl MusicSettings {
    /// The mix and silences `sound` gives, fading over `fade` seconds.
    pub fn tuned(sound: &SoundTuning, fade: f32) -> MusicSettings {
        MusicSettings {
            volume: sound.music_mix,
            fade,
            gap: (sound.silence_min, sound.silence_max),
        }
    }
}

/// Keeps the music's mix and silences to the tuning.
fn mix_music(tuning: Res<Tuning>, mut settings: ResMut<MusicSettings>) {
    let tuned = MusicSettings::tuned(&tuning.sound, settings.fade);
    settings.set_if_neq(tuned);
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
            .init_resource::<Tuning>()
            .add_systems(
                PreUpdate,
                mix_music
                    .run_if(resource_changed::<Tuning>)
                    .after(crate::tuning::TuningSet),
            )
            .add_systems(
                Startup,
                start_music.run_if(not(resource_exists::<FrameCapturePlugin>)),
            )
            .add_systems(OnExit(AppState::Title), fade_out_for_the_delve)
            .add_systems(
                OnExit(AppState::Delving),
                end_troll_theme.run_if(resource_exists::<PlaylistAudio>),
            )
            .add_systems(
                Update,
                (
                    (
                        turn_music_volume,
                        play_creator_theme
                            .run_if(resource_exists::<PlaylistAudio>)
                            .run_if(resource_added::<crate::title::creator::Draft>),
                        // Beginning the delve closes the creator too, but the title has gone by
                        // the frame this sees it.
                        back_to_the_title_track
                            .run_if(resource_removed::<crate::title::creator::Draft>)
                            .run_if(resource_exists::<PlaylistAudio>)
                            .run_if(in_state(AppState::Title)),
                        play_troll_theme
                            .run_if(resource_exists::<PlaylistAudio>)
                            .run_if(in_state(AppState::Delving)),
                        play_music.run_if(resource_exists::<PlaylistAudio>),
                    )
                        .chain(),
                    turn_sound_volume,
                ),
            );
    }
}

/// One playing track of the [`Playlist`].
#[derive(Component, Debug)]
pub struct MusicTrack {
    /// Whether it loops, ending only where it is cut.
    pub looping: bool,
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
        if self.looping {
            return self.cut;
        }
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

/// Cuts short every track playing, each fading out over `fade` from where it is.
fn fade_out_all<'a>(
    tracks: impl Iterator<Item = (Mut<'a, MusicTrack>, Option<&'a AudioSink>)>,
    fade: f32,
    why: &str,
) {
    for (mut track, sink) in tracks {
        let position = sink.map_or(0.0, |sink| sink.position().as_secs_f32());
        track.cut_short(position, fade);
        info!("music: fading out at {position:.1} s {why}");
    }
}

/// The track playing on the title fades out as the delve begins.
pub fn fade_out_for_the_delve(
    settings: Res<MusicSettings>,
    mut tracks: Query<(&mut MusicTrack, Option<&AudioSink>)>,
) {
    fade_out_all(tracks.iter_mut(), settings.fade, "for the delve");
}

/// Every music track, with its player and, once it has one, its sink.
type Tracks<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut MusicTrack,
        &'static AudioPlayer,
        Option<&'static AudioSink>,
    ),
>;

/// Whether `theme` loops and hasn't been cut short.
fn is_looping(theme: &Handle<AudioSource>, tracks: &Tracks) -> bool {
    tracks
        .iter()
        .any(|(track, player, _)| player.0 == *theme && track.cut.is_none())
}

/// Loops `theme` in place of what plays, the theme fading in as the rest fade out, unless it
/// already loops.
fn loop_theme(
    commands: &mut Commands,
    theme: &Handle<AudioSource>,
    fade: f32,
    schedule: &mut MusicSchedule,
    tracks: &mut Tracks,
) {
    if is_looping(theme, tracks) {
        return;
    }
    fade_out_all(
        tracks.iter_mut().map(|(track, _, sink)| (track, sink)),
        fade,
        "for a theme",
    );
    spawn_track(commands, theme.clone(), true);
    schedule.resume_at = None;
}

/// Fades out `theme` where it loops; the playlist picks up after the silence that follows.
fn end_theme(theme: &Handle<AudioSource>, fade: f32, tracks: &mut Tracks) {
    let looping = tracks
        .iter_mut()
        .filter(|(track, player, _)| player.0 == *theme && track.cut.is_none());
    fade_out_all(
        looping.map(|(track, _, sink)| (track, sink)),
        fade,
        "as its theme ends",
    );
}

/// Opening the creator loops its theme, unless it already loops.
fn play_creator_theme(
    mut commands: Commands,
    audio: Res<PlaylistAudio>,
    settings: Res<MusicSettings>,
    mut schedule: ResMut<MusicSchedule>,
    mut tracks: Tracks,
) {
    loop_theme(
        &mut commands,
        &audio.creator_theme,
        settings.fade,
        &mut schedule,
        &mut tracks,
    );
}

/// Going back from the creator to the title's buttons fades out its theme and brings the title
/// track back in from the start.
fn back_to_the_title_track(
    mut commands: Commands,
    audio: Res<PlaylistAudio>,
    settings: Res<MusicSettings>,
    mut schedule: ResMut<MusicSchedule>,
    mut tracks: Tracks,
) {
    end_theme(&audio.creator_theme, settings.fade, &mut tracks);
    if let Some(title) = audio.tracks.first() {
        spawn_track(&mut commands, title.clone(), false);
        schedule.last = Some(0);
    }
    schedule.resume_at = None;
}

/// Whether a troll at `troll` is within `rooms` rooms of the delver at `delver`, both world
/// positions on the level whose rooms are `graph`.
pub fn troll_is_near(graph: &fb_level::RoomGraph, delver: Vec3, troll: Vec3, rooms: u32) -> bool {
    let cell = |at: Vec3| {
        let (x, y) = crate::map::explore::to_grid(at);
        (x.floor() as i32, y.floor() as i32)
    };
    graph
        .rooms_apart(cell(delver), cell(troll))
        .is_some_and(|apart| apart <= rooms)
}

/// Loops the troll theme while a troll not yet dead for good is near the delver, and ends it
/// once none has been for [`SoundTuning::troll_linger`] seconds.
#[allow(clippy::too_many_arguments)]
fn play_troll_theme(
    mut commands: Commands,
    audio: Res<PlaylistAudio>,
    settings: Res<MusicSettings>,
    tuning: Res<Tuning>,
    time: Res<Time<Real>>,
    rooms: Option<Res<crate::play::LevelRooms>>,
    delver: Query<&Transform, With<crate::play::Player>>,
    trolls: Query<(&Transform, &crate::creatures::trolls::Troll)>,
    mut schedule: ResMut<MusicSchedule>,
    mut tracks: Tracks,
    mut last_near: Local<Option<f32>>,
) {
    let (Some(rooms), Ok(delver)) = (rooms, delver.single()) else {
        return;
    };
    let now = time.elapsed_secs();
    let near = trolls.iter().any(|(at, troll)| {
        !troll.spent
            && troll_is_near(
                &rooms.0,
                delver.translation,
                at.translation,
                tuning.sound.troll_rooms,
            )
    });
    if near {
        *last_near = Some(now);
        loop_theme(
            &mut commands,
            &audio.troll_theme,
            settings.fade,
            &mut schedule,
            &mut tracks,
        );
    } else if last_near.is_some_and(|at| now - at >= tuning.sound.troll_linger) {
        *last_near = None;
        end_theme(&audio.troll_theme, settings.fade, &mut tracks);
    }
}

/// The troll theme ends with the delve.
fn end_troll_theme(audio: Res<PlaylistAudio>, settings: Res<MusicSettings>, mut tracks: Tracks) {
    end_theme(&audio.troll_theme, settings.fade, &mut tracks);
}

/// Handles to every playlist track and theme, loaded up front so a track never waits on the
/// disk.
#[derive(Resource, Debug)]
struct PlaylistAudio {
    tracks: Vec<Handle<AudioSource>>,
    creator_theme: Handle<AudioSource>,
    troll_theme: Handle<AudioSource>,
}

fn start_music(mut commands: Commands, assets: Res<AssetServer>, playlist: Res<Playlist>) {
    let audio = PlaylistAudio {
        tracks: playlist
            .tracks
            .iter()
            .map(|path| assets.load(path))
            .collect(),
        creator_theme: assets.load(&playlist.creator_theme),
        troll_theme: assets.load(&playlist.troll_theme),
    };
    if let Some(first) = audio.tracks.first() {
        spawn_track(&mut commands, first.clone(), false);
    }
    // Music isn't part of the seeded world, so each run draws its own order.
    commands.insert_resource(MusicSchedule {
        rng: Mulberry32::new(crate::launch::clock_seed()),
        last: (!audio.tracks.is_empty()).then_some(0),
        resume_at: None,
    });
    commands.insert_resource(audio);
}

fn spawn_track(commands: &mut Commands, source: Handle<AudioSource>, looping: bool) {
    commands.spawn((
        Name::new("Music"),
        MusicTrack {
            looping,
            duration: None,
            measured: false,
            cut: None,
        },
        AudioPlayer::new(source),
        // Silent until `play_music` sets the fade-in gain on the first frame its sink exists.
        if looping {
            PlaybackSettings::LOOP
        } else {
            PlaybackSettings::ONCE
        }
        .with_volume(Volume::SILENT),
    ));
}

/// The music volume `volume` turned by `steps` presses, kept to 0..=1 and to whole tenths so
/// repeated presses land on round values.
pub fn turned_volume(volume: f32, steps: i32) -> f32 {
    let tenths = 1.0 / MUSIC_VOLUME_STEP;
    ((volume * tenths).round() + steps as f32).clamp(0.0, tenths) / tenths
}

pub fn turn_music_volume(
    input: ActionInput,
    mut settings: ResMut<GameSettings>,
    naming: Option<Res<crate::title::creator::Draft>>,
) {
    // Minus and Equal type into the creator's name field while it is open.
    if naming.is_some() {
        return;
    }
    let steps = input.just_pressed(Action::MusicLouder) as i32
        - input.just_pressed(Action::MusicQuieter) as i32;
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

/// How loud a sound effect plays: the tuning's `effects_volume` times the player's
/// [`GameSettings::sound_volume`].
pub fn effect_volume(game: &GameSettings, tuning: &Tuning) -> f32 {
    tuning.sound.effects_volume.clamp(0.0, 1.0) * game.sound_volume.clamp(0.0, 1.0)
}

/// How loud ambient sound plays: the tuning's `ambience_volume` times the player's
/// [`GameSettings::sound_volume`].
pub fn ambience_volume(game: &GameSettings, tuning: &Tuning) -> f32 {
    tuning.sound.ambience_volume.clamp(0.0, 1.0) * game.sound_volume.clamp(0.0, 1.0)
}

/// What a sound effect's [`AudioPlayer`] is spawned with: played once and despawned, at
/// [`effect_volume`].
pub fn sound_effect(game: &GameSettings, tuning: &Tuning) -> (SoundEffect, PlaybackSettings) {
    (
        SoundEffect,
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(effect_volume(game, tuning))),
    )
}

/// How far one world unit sounds: 0.1, as a unit is 10 cm.
pub const SPATIAL_SCALE: f32 = 0.1;

/// Plays `sound` from `at`, heard as a [`spatial_sound_effect`], for the length of the delve.
pub fn play_at(
    commands: &mut Commands,
    name: &'static str,
    sound: Handle<AudioSource>,
    at: Vec3,
    game: &GameSettings,
    tuning: &Tuning,
) {
    commands.spawn((
        Name::new(name),
        AudioPlayer::new(sound),
        spatial_sound_effect(game, tuning),
        Transform::from_translation(at),
        DespawnOnExit(crate::launch::AppState::Delving),
    ));
}

/// [`sound_effect`], heard from where the [`AudioPlayer`]'s entity stands, by the
/// [`SpatialListener`] on the player's eye.
pub fn spatial_sound_effect(
    game: &GameSettings,
    tuning: &Tuning,
) -> (SoundEffect, PlaybackSettings) {
    let (effect, playback) = sound_effect(game, tuning);
    (
        effect,
        playback
            .with_spatial(true)
            .with_spatial_scale(SpatialScale::new(SPATIAL_SCALE)),
    )
}

/// Sound effects already playing follow a change of sound volume, or of the tuning, the same
/// frame.
fn turn_sound_volume(
    game: Res<GameSettings>,
    tuning: Res<Tuning>,
    global: Res<GlobalVolume>,
    mut sinks: Query<&mut AudioSink, With<SoundEffect>>,
) {
    if game.is_changed() || tuning.is_changed() {
        for mut sink in &mut sinks {
            // Bevy applies `GlobalVolume` only when it creates a sink, so it's folded in here.
            sink.set_volume(Volume::Linear(
                global.volume.to_linear() * effect_volume(&game, &tuning),
            ));
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
    // Every track, with a sink yet or not.
    all: Query<(), With<MusicTrack>>,
) {
    let now = time.elapsed_secs();
    if let Some(at) = schedule.resume_at
        && now >= at
    {
        let last = schedule.last;
        if let Some(next) = pick_track(&mut schedule.rng, audio.tracks.len(), last) {
            spawn_track(&mut commands, audio.tracks[next].clone(), false);
            schedule.last = Some(next);
        }
        schedule.resume_at = None;
    }
    let mut ended = 0;
    for (entity, mut track, player, mut sink) in &mut tracks {
        if !track.measured {
            track.measured = true;
            track.duration = sources
                .get(&player.0)
                .and_then(|source| ogg_vorbis_duration(&source.bytes));
            let path = player.0.path().map(ToString::to_string).unwrap_or_default();
            info!("music: playing {path}, {:?} s", track.duration);
        }
        let position = sink.position().as_secs_f32();
        if sink.empty() || track.is_cut_off(position) {
            sink.stop();
            commands.entity(entity).despawn();
            ended += 1;
        } else {
            let gain = track_gain(position, track.end(), settings.fade);
            // Bevy applies `GlobalVolume` only when it creates a sink, so it's folded in here.
            sink.set_volume(Volume::Linear(music_gain(
                global.volume.to_linear(),
                &settings,
                &game,
                gain,
            )));
        }
    }
    // The silence starts once the last track has ended, so none is drawn over a theme.
    if ended > 0 && all.iter().count() == ended {
        let silence = silence_seconds(&mut schedule.rng, settings.gap);
        schedule.resume_at = Some(now + silence);
        info!("music: silence for {silence:.0} s");
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
    let ident = bytes
        .windows(IDENTIFICATION.len())
        .position(|w| w == IDENTIFICATION)?;
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
    fn playlist_opens_on_forever_below_and_holds_all_nineteen_tracks() {
        let tracks = Playlist::default().tracks;
        assert_eq!(tracks.len(), 19);
        assert_eq!(tracks[0], "audio/forever-below.ogg");
    }

    #[test]
    fn tracks_are_drawn_at_random_and_never_twice_running() {
        let mut rng = Mulberry32::new(7);
        let len = Playlist::default().tracks.len();
        let mut seen = vec![0usize; len];
        let mut last = Some(0);
        for _ in 0..500 * len {
            let next = pick_track(&mut rng, len, last).unwrap();
            assert_ne!(Some(next), last);
            seen[next] += 1;
            last = Some(next);
        }
        // Every track turns up, each about 500 times.
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
        let playlist = Playlist::default();
        for theme in [&playlist.creator_theme, &playlist.troll_theme] {
            assert!(
                !playlist.tracks.contains(theme),
                "{theme} is never drawn at random"
            );
        }
        for track in playlist
            .tracks
            .into_iter()
            .chain([playlist.creator_theme, playlist.troll_theme])
        {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("assets")
                .join(&track);
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{track}: {e}"));
            let duration =
                ogg_vorbis_duration(&bytes).unwrap_or_else(|| panic!("{track}: not Ogg Vorbis"));
            assert!(duration > 60.0, "{track}: {duration}");
        }
    }

    #[test]
    fn fades_in_from_silence() {
        assert_eq!(track_gain(0.0, None, FADE), 0.0);
        assert!(
            track_gain(1.0, None, FADE) >= 0.5 - 1e-6,
            "audible within a second"
        );
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
        let mut track = MusicTrack {
            looping: false,
            duration: Some(200.0),
            measured: true,
            cut: None,
        };
        assert_eq!(track.end(), Some(200.0));
        track.cut_short(40.0, FADE);
        assert_eq!(track.end(), Some(43.0));
        assert_eq!(
            track_gain(40.0, track.end(), FADE),
            1.0,
            "no jump when the delve begins"
        );
        let falling: Vec<f32> = [40.5, 41.0, 42.0, 42.9]
            .iter()
            .map(|&t| track_gain(t, track.end(), FADE))
            .collect();
        assert!(falling.windows(2).all(|w| w[1] < w[0]), "{falling:?}");
        assert!(!track.is_cut_off(42.9));
        assert!(track.is_cut_off(43.0));
        assert!(track_gain(43.0, track.end(), FADE).abs() < 1e-6);
        // A second cut never lengthens the track.
        track.cut_short(42.0, FADE);
        assert_eq!(track.end(), Some(43.0));
        // After the silence the level's music is drawn from the others, never Forever Below.
        let mut rng = Mulberry32::new(11);
        assert!(
            (0..500).all(
                |_| pick_track(&mut rng, Playlist::default().tracks.len(), Some(0)) != Some(0)
            )
        );
    }

    #[test]
    fn a_cut_short_track_with_no_known_length_still_ends() {
        let mut track = MusicTrack {
            looping: false,
            duration: None,
            measured: true,
            cut: None,
        };
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
        let track = app
            .world_mut()
            .spawn(MusicTrack {
                looping: false,
                duration: Some(120.0),
                measured: false,
                cut: None,
            })
            .id();
        app.update();
        assert_eq!(
            app.world().get::<MusicTrack>(track).unwrap().cut,
            None,
            "it plays on over the title"
        );
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Delving);
        app.update();
        // No sink without an audio device, so it fades from the start.
        assert_eq!(
            app.world().get::<MusicTrack>(track).unwrap().cut,
            Some(MusicSettings::default().fade)
        );
    }

    #[test]
    fn a_looping_track_plays_on_past_its_length_until_cut() {
        let mut track = MusicTrack {
            looping: true,
            duration: Some(120.0),
            measured: true,
            cut: None,
        };
        assert_eq!(track.end(), None);
        assert_eq!(
            track_gain(500.0, track.end(), FADE),
            1.0,
            "no fade at the end of each loop"
        );
        // Positions keep counting across loops, so a cut on the third time round still fades.
        track.cut_short(300.0, FADE);
        assert_eq!(track.end(), Some(303.0));
        assert!(track.is_cut_off(303.0));
    }

    fn handle(n: u128) -> Handle<AudioSource> {
        Handle::Uuid(
            bevy::asset::uuid::Uuid::from_u128(n),
            std::marker::PhantomData,
        )
    }

    fn creator_theme_app() -> App {
        use bevy::state::app::StatesPlugin;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Title)
            .init_resource::<MusicSettings>()
            .insert_resource(PlaylistAudio {
                tracks: vec![handle(1), handle(2)],
                creator_theme: handle(9),
                troll_theme: handle(10),
            })
            .insert_resource(MusicSchedule {
                rng: Mulberry32::new(1),
                last: Some(0),
                resume_at: None,
            })
            .add_systems(OnExit(AppState::Title), fade_out_for_the_delve)
            .add_systems(
                Update,
                (
                    play_creator_theme.run_if(resource_added::<crate::title::creator::Draft>),
                    back_to_the_title_track
                        .run_if(resource_removed::<crate::title::creator::Draft>)
                        .run_if(in_state(AppState::Title)),
                )
                    .chain(),
            );
        app
    }

    fn tracks(app: &mut App) -> Vec<(bool, Option<f32>, Handle<AudioSource>)> {
        let mut query = app.world_mut().query::<(&MusicTrack, &AudioPlayer)>();
        query
            .iter(app.world())
            .map(|(track, player)| (track.looping, track.cut, player.0.clone()))
            .collect()
    }

    #[test]
    fn the_creator_loops_its_theme_until_the_delve_begins() {
        use crate::title::creator::Draft;
        let mut app = creator_theme_app();
        app.world_mut().spawn((
            MusicTrack {
                looping: false,
                duration: Some(180.0),
                measured: true,
                cut: None,
            },
            AudioPlayer::new(handle(1)),
        ));
        app.update();
        assert_eq!(
            tracks(&mut app).len(),
            1,
            "the title track plays on until the creator opens"
        );

        app.insert_resource(Draft::roll(1));
        app.update();
        let playing = tracks(&mut app);
        assert!(
            playing.contains(&(false, Some(FADE), handle(1))),
            "the title track fades out: {playing:?}"
        );
        assert!(
            playing.contains(&(true, None, handle(9))),
            "the theme loops in its place: {playing:?}"
        );

        // Back to the buttons: the theme fades out and the title track comes in again.
        app.world_mut().remove_resource::<Draft>();
        app.update();
        let playing = tracks(&mut app);
        assert!(
            playing.contains(&(true, Some(FADE), handle(9))),
            "the theme fades out: {playing:?}"
        );
        assert!(
            playing.contains(&(false, None, handle(1))),
            "the title track fades in: {playing:?}"
        );
        assert_eq!(app.world().resource::<MusicSchedule>().last, Some(0));

        // Into the creator again: a fresh theme over the title track, fading out.
        app.insert_resource(Draft::roll(2));
        app.update();
        let playing = tracks(&mut app);
        assert!(
            playing.contains(&(true, None, handle(9))),
            "the theme loops again: {playing:?}"
        );
        assert!(
            playing
                .iter()
                .all(|t| t == &(true, None, handle(9)) || t.1.is_some()),
            "nothing else plays on: {playing:?}"
        );

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Delving);
        app.update();
        assert!(
            tracks(&mut app).iter().all(|t| t.1.is_some()),
            "everything fades as the delve begins"
        );
    }

    #[test]
    fn beginning_the_delve_from_the_creator_does_not_bring_the_title_track_back() {
        use crate::title::creator::Draft;
        let mut app = creator_theme_app();
        app.insert_resource(Draft::roll(1));
        app.update();
        // `begin_delve` removes the draft and asks for the delve in the same frame.
        app.world_mut().remove_resource::<Draft>();
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Delving);
        app.update();
        let playing = tracks(&mut app);
        assert!(
            !playing.iter().any(|t| t.2 == handle(1)),
            "no title track under the delve: {playing:?}"
        );
        assert!(
            playing.iter().all(|t| t.1.is_some()),
            "the theme fades out: {playing:?}"
        );
    }

    /// A rough rock level's room graph, the centre of a room for the delver, and the centres of
    /// rooms 0, 1 and 3 or more rooms from it.
    fn rooms_to_stand_in() -> (fb_level::RoomGraph, Vec3, [Vec3; 3]) {
        let level = fb_level::generate(4, 1);
        let graph = fb_level::RoomGraph::new(&level);
        let corner = |room: &fb_level::Room| (room.x as i32, room.y as i32);
        let centre = |room: &fb_level::Room| {
            let (x, y) = room.centre();
            Vec3::new(x * fb_level::CELL, 0.0, y * fb_level::CELL)
        };
        let find = |from: &fb_level::Room, far: fn(u32) -> bool| {
            level
                .rooms
                .iter()
                .find(|r| graph.rooms_apart(corner(from), corner(r)).is_some_and(far))
        };
        let delver = level
            .rooms
            .iter()
            .find(|r| find(r, |a| a > 2).is_some())
            .expect("a room with one far off");
        let at = |far: fn(u32) -> bool| centre(find(delver, far).expect("a room that far"));
        let rooms = [at(|a| a == 0), at(|a| a == 1), at(|a| a > 2)];
        (graph.clone(), centre(delver), rooms)
    }

    #[test]
    fn a_troll_is_near_within_a_couple_of_rooms() {
        let (graph, delver, [same, next, far]) = rooms_to_stand_in();
        let rooms = SoundTuning::default().troll_rooms;
        assert_eq!(rooms, 2);
        assert!(troll_is_near(&graph, delver, same, rooms));
        assert!(troll_is_near(&graph, delver, next, rooms));
        assert!(
            !troll_is_near(&graph, delver, far, rooms),
            "three rooms or more off is too far"
        );
    }

    #[test]
    fn a_near_troll_loops_its_theme_until_it_is_burned() {
        use crate::creatures::trolls::Troll;
        use bevy::state::app::StatesPlugin;
        let (graph, delver, [_, next, _]) = rooms_to_stand_in();
        let mut app = App::new();
        let mut tuning = Tuning::default();
        tuning.sound.troll_linger = 0.0;
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .init_resource::<MusicSettings>()
            .insert_resource(tuning)
            .insert_resource(crate::play::LevelRooms(graph))
            .insert_resource(PlaylistAudio {
                tracks: vec![handle(1), handle(2)],
                creator_theme: handle(9),
                troll_theme: handle(10),
            })
            .insert_resource(MusicSchedule {
                rng: Mulberry32::new(1),
                last: Some(0),
                resume_at: Some(5.0),
            })
            .add_systems(Update, play_troll_theme.run_if(in_state(AppState::Delving)));
        app.world_mut()
            .spawn((crate::play::Player, Transform::from_translation(delver)));
        app.world_mut().spawn((
            MusicTrack {
                looping: false,
                duration: Some(180.0),
                measured: true,
                cut: None,
            },
            AudioPlayer::new(handle(2)),
        ));
        let troll = app
            .world_mut()
            .spawn((
                Troll::new(&fb_creatures::TrollState::roll(1)),
                Transform::from_translation(next),
            ))
            .id();
        app.update();
        let playing = tracks(&mut app);
        assert!(
            playing.contains(&(true, None, handle(10))),
            "the troll theme loops: {playing:?}"
        );
        assert!(
            playing.contains(&(false, Some(FADE), handle(2))),
            "the level's track fades out: {playing:?}"
        );
        assert_eq!(
            app.world().resource::<MusicSchedule>().resume_at,
            None,
            "nothing is drawn over it"
        );

        app.update();
        assert_eq!(
            tracks(&mut app)
                .iter()
                .filter(|t| t.2 == handle(10))
                .count(),
            1,
            "one theme, however long the troll stays"
        );

        app.world_mut().get_mut::<Troll>(troll).unwrap().spent = true;
        app.update();
        assert!(
            tracks(&mut app).iter().all(|t| t.1.is_some()),
            "burned to death, it is no threat: the theme fades out"
        );
    }

    #[test]
    fn the_troll_theme_ends_with_the_delve() {
        use bevy::state::app::StatesPlugin;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .init_resource::<MusicSettings>()
            .insert_resource(PlaylistAudio {
                tracks: vec![handle(1)],
                creator_theme: handle(9),
                troll_theme: handle(10),
            })
            .add_systems(OnExit(AppState::Delving), end_troll_theme);
        app.world_mut().spawn((
            MusicTrack {
                looping: true,
                duration: None,
                measured: true,
                cut: None,
            },
            AudioPlayer::new(handle(10)),
        ));
        app.update();
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Title);
        app.update();
        assert_eq!(tracks(&mut app), vec![(true, Some(FADE), handle(10))]);
    }

    #[test]
    fn music_volume_zero_is_silent_and_one_is_the_mix() {
        let mix = MusicSettings::default();
        let at = |music_volume: f32| {
            music_gain(
                1.0,
                &mix,
                &GameSettings {
                    music_volume,
                    ..default()
                },
                1.0,
            )
        };
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
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Keymap>()
            .init_resource::<GameSettings>()
            .add_systems(Update, turn_music_volume);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Minus);
        app.update();
        assert!((app.world().resource::<GameSettings>().music_volume - 0.9).abs() < 1e-6);
    }

    #[test]
    fn volume_keys_type_into_the_creator_instead() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Keymap>()
            .init_resource::<GameSettings>()
            .add_systems(Update, turn_music_volume);
        app.insert_resource(crate::title::creator::Draft::roll(1));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Minus);
        app.update();
        assert_eq!(
            app.world().resource::<GameSettings>().music_volume,
            GameSettings::default().music_volume
        );
    }

    #[test]
    fn reads_the_length_of_the_soundtrack() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/audio/the-wizards-tower.ogg"
        );
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
