//! Ambient sound: water drips placed at fixed spots in open cells of the level from the seed,
//! and a deep rumble at random intervals. All scale with [`GameSettings::sound_volume`] and
//! respect [`SoundTuning`] levels and rumble intervals.
//!
//! Drip placement is a pure function of the level's seed: the same level always places drips at
//! the same spots in open floor, a little below the ceiling, and they are heard spatially.
//! Rumbles are not part of the world: like the music's order they are drawn from a generator
//! seeded from the clock when the delve begins (`launch::clock_seed`), so they differ run to run,
//! and their intervals are read from the live tuning, the first included.
//!
//! Drips play one-shots from emitters placed at level generation, each with a jittered interval
//! timer. All ambient sounds are despawned when the delve ends, and sound volume 0 silences them all.

use bevy::{audio::{SpatialAudioSink, Volume}, prelude::*};
use procgen_rng::Mulberry32;

use crate::audio::ambience_volume;
use crate::launch::AppState;
use crate::play::{CurrentLevel, DelveSystems};
use crate::settings::GameSettings;
use crate::tuning::{SoundTuning, Tuning};

/// A location in the level where a drip plays periodically, at the entity's [`Transform`]. The
/// same level seed always chooses the same spots, and they are only in open floor cells.
#[derive(Component, Clone, Debug)]
struct DripEmitter {
    /// Timer for the next drip.
    timer: Timer,
}

/// An ambient sound currently playing: drip or rumble.
#[derive(Component, Debug)]
struct Ambience;

/// State of rumble playback: timer for the next rumble and the RNG that drove this run.
#[derive(Resource, Debug)]
struct RumbleSchedule {
    rng: Mulberry32,
    /// Timer counting down to the next rumble.
    timer: Timer,
}

/// Sound assets for ambient sounds.
#[derive(Resource, Clone, Debug, Default)]
struct AmbientSounds {
    drip: Handle<AudioSource>,
    rumble: Handle<AudioSource>,
}

pub struct AmbientPlugin;

impl Plugin for AmbientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AmbientSounds>()
            .add_systems(Startup, load_ambient_sounds.run_if(resource_exists::<AssetServer>))
            .add_systems(OnEnter(AppState::Delving), spawn_rumble_schedule)
            .add_systems(OnExit(AppState::Delving), end_rumble_schedule)
            // Each level the delver comes to drips from its own spots.
            .add_systems(Update, (clear_drips, spawn_ambient_emitters).chain().run_if(crate::loading::laid_out_at(6)).in_set(DelveSystems))
            .add_systems(Update, (play_drips, play_rumbles, turn_ambience_volume).in_set(DelveSystems));
    }
}

fn load_ambient_sounds(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(AmbientSounds {
        drip: assets.load("audio/drip.ogg"),
        rumble: assets.load("audio/rumble.ogg"),
    });
}

/// Drip interval base in seconds, before jittering. Our own.
const DRIP_INTERVAL: f32 = 4.0;
/// Drip interval jitter range as a fraction of the base interval. Our own.
const DRIP_JITTER: f32 = 0.5;
/// How many drip emitters to place in the level, spread across the level's area. Our own.
const DRIP_EMITTERS: usize = 4;
/// How far below the ceiling a drip forms, in world units (10 cm each). Our own.
const DRIP_DROP: f32 = 1.5;

/// Choose drip emitter positions from the level seed: spread them across the level area in
/// open floor cells only, at the cell centre and just under the level's ceiling there. The same level
/// seed always gives the same positions.
fn drip_positions(level: &fb_level::Level) -> Vec<Vec3> {
    let mut rng = Mulberry32::new(level.seed);
    let mut out = Vec::new();
    let mut attempts = 0;
    let max_attempts = DRIP_EMITTERS * 100;

    while out.len() < DRIP_EMITTERS && attempts < max_attempts {
        let x = (rng.next_f64() * (level.width - 1) as f64) as i32;
        let y = (rng.next_f64() * (level.height - 1) as f64) as i32;

        if level.is_open(x, y) {
            let world_x = (x as f32 + 0.5) * fb_level::CELL;
            let world_z = (y as f32 + 0.5) * fb_level::CELL;
            out.push(Vec3::new(world_x, crate::level::geometry::stone::ceiling_at(level, world_x, world_z) - DRIP_DROP, world_z));
        }

        attempts += 1;
    }

    out
}

/// A drip interval in seconds, drawn with jitter from the base interval.
fn drip_interval(rng: &mut Mulberry32) -> f32 {
    let jitter = rng.range(1.0 - DRIP_JITTER as f64, 1.0 + DRIP_JITTER as f64) as f32;
    DRIP_INTERVAL * jitter
}

/// A time in seconds drawn evenly from the tuning's rumble interval range.
fn rumble_interval(rng: &mut Mulberry32, tuning: &SoundTuning) -> f32 {
    rng.range(tuning.rumble_interval_min as f64, tuning.rumble_interval_max.max(tuning.rumble_interval_min) as f64) as f32
}

/// The rumble generator seeded with `seed`, and its first wait drawn from `sound`.
fn new_rumble_schedule(seed: u32, sound: &SoundTuning) -> RumbleSchedule {
    let mut rng = Mulberry32::new(seed);
    let interval = rumble_interval(&mut rng, sound);
    RumbleSchedule { rng, timer: Timer::from_seconds(interval, TimerMode::Once) }
}

fn clear_drips(mut commands: Commands, old: Query<Entity, With<DripEmitter>>) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
}

fn spawn_ambient_emitters(mut commands: Commands, level: Res<CurrentLevel>) {
    let mut rng = Mulberry32::new(level.0.seed);

    for position in drip_positions(&level.0) {
        let interval = drip_interval(&mut rng);
        commands.spawn((
            Name::new("drip emitter"),
            DripEmitter { timer: Timer::from_seconds(interval, TimerMode::Repeating) },
            Transform::from_translation(position),
            DespawnOnExit(AppState::Delving),
        ));
    }
}

fn spawn_rumble_schedule(mut commands: Commands, tuning: Res<Tuning>) {
    commands.insert_resource(new_rumble_schedule(crate::launch::clock_seed(), &tuning.sound));
}

/// The rumble's schedule goes with the delve; the next delve draws its own.
fn end_rumble_schedule(mut commands: Commands) {
    commands.remove_resource::<RumbleSchedule>();
}

fn play_drips(
    mut commands: Commands,
    time: Res<Time>,
    sounds: Res<AmbientSounds>,
    game: Res<GameSettings>,
    tuning: Res<Tuning>,
    mut emitters: Query<(&mut DripEmitter, &Transform)>,
) {
    let volume = ambience_volume(&game, &tuning);

    for (mut emitter, at) in &mut emitters {
        if emitter.timer.tick(time.delta()).just_finished() {
            commands.spawn((
                Name::new("drip"),
                Ambience,
                AudioPlayer::new(sounds.drip.clone()),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)).with_spatial(true),
                Transform::from_translation(at.translation),
                DespawnOnExit(AppState::Delving),
            ));
        }
    }
}

fn play_rumbles(
    mut commands: Commands,
    time: Res<Time>,
    sounds: Res<AmbientSounds>,
    game: Res<GameSettings>,
    tuning: Res<Tuning>,
    mut schedule: ResMut<RumbleSchedule>,
) {
    if schedule.timer.tick(time.delta()).just_finished() {
        let volume = ambience_volume(&game, &tuning);
        commands.spawn((
            Name::new("rumble"),
            Ambience,
            AudioPlayer::new(sounds.rumble.clone()),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
            DespawnOnExit(AppState::Delving),
        ));

        let interval = rumble_interval(&mut schedule.rng, &tuning.sound);
        schedule.timer = Timer::from_seconds(interval, TimerMode::Once);
    }
}

/// Ambient sounds already playing follow a change of sound volume or of the tuning, the same frame.
fn turn_ambience_volume(
    game: Res<GameSettings>,
    tuning: Res<Tuning>,
    global: Res<GlobalVolume>,
    mut sinks: Query<&mut AudioSink, With<Ambience>>,
    mut spatial_sinks: Query<&mut SpatialAudioSink, With<Ambience>>,
) {
    if game.is_changed() || tuning.is_changed() {
        let volume = Volume::Linear(global.volume.to_linear() * ambience_volume(&game, &tuning));
        for mut sink in &mut sinks {
            // Bevy applies `GlobalVolume` only when it creates a sink, so it's folded in here.
            sink.set_volume(volume);
        }
        for mut sink in &mut spatial_sinks {
            sink.set_volume(volume);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drip_positions_are_deterministic() {
        let pos1 = drip_positions(&fb_level::generate(1207, 1));
        let pos2 = drip_positions(&fb_level::generate(1207, 1));
        let pos_other = drip_positions(&fb_level::generate(1208, 1));

        assert_eq!(pos1.len(), DRIP_EMITTERS);
        assert_eq!(pos1, pos2, "same seed gives same positions");
        assert_eq!(pos_other.len(), DRIP_EMITTERS);
        assert_ne!(pos1, pos_other, "different seeds give different positions");
    }

    #[test]
    fn drip_positions_are_open_cells() {
        for seed in [1207, 1208, 42, 7] {
            let level = fb_level::generate(seed, 1);
            for pos in drip_positions(&level) {
                let x = (pos.x / fb_level::CELL).floor() as i32;
                let y = (pos.z / fb_level::CELL).floor() as i32;
                assert!(level.is_open(x, y), "seed {seed}: drip at ({x}, {y}) is not in an open cell");
            }
        }
    }

    #[test]
    fn drips_hang_a_little_below_the_ceiling() {
        let level = fb_level::generate(1207, 1);
        for pos in drip_positions(&level) {
            assert!(pos.y < level.ceiling && pos.y > level.ceiling - 5.0, "drip at y={} under ceiling {}", pos.y, level.ceiling);
        }
    }

    #[test]
    fn drips_fall_inside_the_drawn_hall_at_every_depth() {
        use crate::level::geometry::stone::ceiling_at;
        for depth in [1, 2] {
            for seed in [1207, 1208, 42, 7, 99, 3] {
                let level = fb_level::generate(seed, depth);
                for pos in drip_positions(&level) {
                    let ceiling = ceiling_at(&level, pos.x, pos.z);
                    assert!(pos.y > 0.0 && pos.y < ceiling, "depth {depth} seed {seed}: drip at y={} between the floor and ceiling {ceiling}", pos.y);
                    assert!(pos.y > ceiling - 5.0, "depth {depth} seed {seed}: drip at y={} hangs far from ceiling {ceiling}", pos.y);
                }
            }
        }
    }

    #[test]
    fn drip_interval_is_jittered() {
        let mut rng = Mulberry32::new(42);
        let intervals: Vec<f32> = (0..100).map(|_| drip_interval(&mut rng)).collect();
        let min = intervals.iter().copied().fold(f32::INFINITY, f32::min);
        let max = intervals.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(min < DRIP_INTERVAL, "intervals vary below base");
        assert!(max > DRIP_INTERVAL, "intervals vary above base");
        assert!(min >= DRIP_INTERVAL * (1.0 - DRIP_JITTER), "intervals respect jitter bound");
        assert!(max <= DRIP_INTERVAL * (1.0 + DRIP_JITTER), "intervals respect jitter bound");
    }

    #[test]
    fn rumble_interval_chooses_from_tuning_range() {
        let tuning = SoundTuning::default();
        let mut rng = Mulberry32::new(42);
        for _ in 0..1000 {
            let interval = rumble_interval(&mut rng, &tuning);
            assert!(interval >= tuning.rumble_interval_min && interval <= tuning.rumble_interval_max, "interval {interval} outside range");
        }
    }

    #[test]
    fn the_first_rumble_waits_by_the_live_tuning() {
        let sound = SoundTuning { rumble_interval_min: 7.0, rumble_interval_max: 7.0, ..default() };
        assert_eq!(new_rumble_schedule(5, &sound).timer.duration().as_secs_f32(), 7.0);
        // The system reads the resource, not the defaults.
        let mut app = App::new();
        let tuning = Tuning { sound, ..Default::default() };
        app.insert_resource(tuning).add_systems(Update, spawn_rumble_schedule);
        app.update();
        assert_eq!(app.world().resource::<RumbleSchedule>().timer.duration().as_secs_f32(), 7.0);
    }
}
