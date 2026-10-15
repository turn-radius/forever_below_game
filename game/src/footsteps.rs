//! Footsteps as the delver and folk walk: varied stone steps with cadence matching speed.
//!
//! The delver plays a non-spatial footstep at each stride, paced by their walking speed.
//! Standing still plays no steps; doubling speed doubles the step rate. The same sample never
//! plays twice in a row. Burdened and Encumbered steps are heavier (lower modes, longer sustain)
//! than light steps.
//!
//! Folk and creatures that walk play spatial footsteps at their positions, quieter with distance.
//! Each entity tracks its own step state with RNG to vary samples independently.

use bevy::{audio::Volume, prelude::*};
use procgen_rng::Mulberry32;

use crate::audio::effect_volume;
use crate::character::Character;
use crate::creatures::{Critter, Driven};
use crate::inventory::Inventory;
use crate::launch::AppState;
use crate::play::{DelveSystems, Player};
use crate::player::Gait;
use crate::settings::GameSettings;
use crate::tuning::Tuning;

/// Sound assets for footsteps: 3 light and 3 heavy variants.
#[derive(Resource, Clone, Debug, Default)]
struct FootstepSounds {
    light: [Handle<AudioSource>; 3],
    heavy: [Handle<AudioSource>; 3],
}

/// Per-entity footstep state: RNG for varied samples and the last sample played.
#[derive(Component, Debug)]
struct StepState {
    /// RNG for sample variation.
    rng: Mulberry32,
    /// The last sample played (0–2), to avoid playing the same one twice.
    last_sample: usize,
    /// Previous gait phase, to detect step threshold crossing (delver) or accumulated distance (creatures).
    prev_phase: f32,
    /// Last position for creature footsteps (tracking movement).
    last_pos: Option<Vec2>,
}

/// Gain on a creature's step against the effects volume. Spatial playback already fades it with
/// distance (scale 0.1, so a rat 20 units away is 2 m off and plays at a quarter of this), which
/// keeps a rat's step quieter than the delver's own but present at a few metres.
const CREATURE_STEP_GAIN: f32 = 0.7;

/// Marker for entities (folk/creatures) that walk and need spatial footsteps.
#[derive(Component, Debug)]
pub struct WalksWithSteps;

pub struct FootstepsPlugin;

impl Plugin for FootstepsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FootstepSounds>()
            .add_systems(Startup, load_footstep_sounds.run_if(resource_exists::<AssetServer>))
            .add_systems(Update, (init_delver_steps, play_delver_steps, init_creature_steps, play_creature_steps).in_set(DelveSystems));
    }
}

fn load_footstep_sounds(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(FootstepSounds {
        light: [
            assets.load("audio/step_light_1.ogg"),
            assets.load("audio/step_light_2.ogg"),
            assets.load("audio/step_light_3.ogg"),
        ],
        heavy: [
            assets.load("audio/step_heavy_1.ogg"),
            assets.load("audio/step_heavy_2.ogg"),
            assets.load("audio/step_heavy_3.ogg"),
        ],
    });
}

/// Burden threshold as a fraction of capacity: Burdened at 34% or more.
pub fn burden_threshold(load: f32, capacity: f32) -> bool {
    capacity > 0.0 && load >= 0.34 * capacity
}

/// Next step sample: one of 0–2, never the same as last.
fn next_sample(rng: &mut Mulberry32, last: usize) -> usize {
    let sample = (rng.next_f64() * 3.0) as usize % 3;
    if sample == last {
        (sample + 1) % 3
    } else {
        sample
    }
}

/// Gives the delver their step state as they spawn.
fn init_delver_steps(mut commands: Commands, new: Query<Entity, Added<Player>>) {
    for entity in &new {
        commands.entity(entity).insert(StepState { rng: Mulberry32::new(0x5733_d317), last_sample: 0, prev_phase: 0.0, last_pos: None });
    }
}

/// Play footsteps for the delver: one per footfall of the gait, at the pace they walk. The
/// delver stays on the floor (there is no jump, and a fall only slows them), so there is no
/// airborne case.
fn play_delver_steps(
    mut commands: Commands,
    sounds: Res<FootstepSounds>,
    game: Res<GameSettings>,
    tuning: Res<Tuning>,
    character: Option<Res<Character>>,
    inventory: Option<Res<Inventory>>,
    mut delver: Query<(&Gait, &mut StepState), With<Player>>,
) {
    let Ok((gait, mut state)) = delver.single_mut() else {
        return;
    };
    let prev = std::mem::replace(&mut state.prev_phase, gait.phase);
    // No steps while standing still.
    if gait.pace <= 0.0 {
        return;
    }

    // A footfall is the phase crossing 0 (the wrap) or π.
    let (pi, tau) = (std::f32::consts::PI, std::f32::consts::TAU);
    let (phase, prev) = (gait.phase % tau, prev % tau);
    let crosses_zero = prev > pi && phase < pi && phase < prev;
    let crosses_pi = prev < pi && phase >= pi;
    if !crosses_zero && !crosses_pi {
        return;
    }

    let last = state.last_sample;
    let sample = next_sample(&mut state.rng, last);
    state.last_sample = sample;

    let is_burdened = match (character, inventory) {
        (Some(character), Some(inventory)) => {
            let derived = character.derive(&inventory);
            burden_threshold(derived.load, derived.capacity)
        }
        _ => false,
    };
    let samples = if is_burdened { &sounds.heavy } else { &sounds.light };

    commands.spawn((
        Name::new("delver step"),
        AudioPlayer::new(samples[sample].clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(effect_volume(&game, &tuning))),
        DespawnOnExit(AppState::Delving),
    ));
}

/// Add step state to creatures when they become Driven (active in behaviour).
fn init_creature_steps(mut commands: Commands, new: Query<(Entity, &Critter, &Transform), Added<Driven>>) {
    for (entity, critter, transform) in &new {
        let rng = Mulberry32::new(critter.spot.seed ^ 0x5733_7570);
        let pos = transform.translation.xz();
        commands.entity(entity).insert((
            StepState { rng, last_sample: 0, prev_phase: 0.0, last_pos: Some(pos) },
            WalksWithSteps,
        ));
    }
}

/// Play spatial footsteps for creatures: rats step, snakes slither (no steps).
fn play_creature_steps(
    mut commands: Commands,
    sounds: Res<FootstepSounds>,
    game: Res<GameSettings>,
    tuning: Res<Tuning>,
    mut creatures: Query<(&Critter, &Transform, &mut StepState), (With<Driven>, With<WalksWithSteps>)>,
) {
    const RAT_STRIDE: f32 = 0.5;  // World units per step for rats.

    for (critter, transform, mut state) in &mut creatures {
        // Snakes don't make footsteps; they slither.
        if critter.spot.species.is_snake() {
            state.last_pos = Some(transform.translation.xz());
            continue;
        }

        // Rats make light, quick footsteps. Accumulate distance and step at stride intervals.
        let current_pos = transform.translation.xz();
        let moved = state.last_pos.map_or(0.0, |last| current_pos.distance(last));

        // Accumulate movement distance in prev_phase.
        state.prev_phase += moved;
        state.last_pos = Some(current_pos);

        // Step when accumulated distance exceeds stride threshold.
        if state.prev_phase >= RAT_STRIDE {
            state.prev_phase -= RAT_STRIDE;

            let old_last = state.last_sample;
            let sample = next_sample(&mut state.rng, old_last);
            state.last_sample = sample;

            // Rats use light steps always, at the effects volume; distance fades them.
            let volume = effect_volume(&game, &tuning) * CREATURE_STEP_GAIN;

            commands.spawn((
                Name::new("creature step"),
                AudioPlayer::new(sounds.light[sample].clone()),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)).with_spatial(true),
                transform.clone(),
                DespawnOnExit(AppState::Delving),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burden_threshold_is_34_percent() {
        assert!(!burden_threshold(0.0, 100.0), "empty");
        assert!(!burden_threshold(33.9, 100.0), "just under");
        assert!(burden_threshold(34.0, 100.0), "at threshold");
        assert!(burden_threshold(50.0, 100.0), "well over");
        assert!(!burden_threshold(1.0, 0.0), "zero capacity");
    }

    #[test]
    fn next_sample_never_repeats() {
        let mut rng = Mulberry32::new(42);
        let mut last = 0;
        for _ in 0..100 {
            let sample = next_sample(&mut rng, last);
            assert_ne!(sample, last, "sample repeated");
            last = sample;
        }
    }

    #[test]
    fn footstep_sounds_are_loaded() {
        let sounds = FootstepSounds::default();
        assert_eq!(sounds.light.len(), 3);
        assert_eq!(sounds.heavy.len(), 3);
    }

    /// An app with the delver's step systems and a system that turns the gait by `pace` times
    /// `TURN` radians a frame, as `walk` does (the real gait needs input and a level).
    fn delver_app(pace: f32) -> App {
        use bevy::state::app::StatesPlugin;
        const TURN: f32 = 0.2;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .init_resource::<GameSettings>()
            .init_resource::<Tuning>()
            .init_resource::<FootstepSounds>();
        app.world_mut().spawn((Player, Gait { phase: 0.0, pace }));
        app.add_systems(
            Update,
            (
                |mut gait: Query<&mut Gait, With<Player>>| {
                    for mut gait in &mut gait {
                        gait.phase = (gait.phase + gait.pace * TURN) % std::f32::consts::TAU;
                    }
                },
                init_delver_steps,
                play_delver_steps,
            )
                .chain(),
        );
        app
    }

    fn steps(app: &mut App) -> usize {
        app.world_mut().query::<&AudioPlayer>().iter(app.world()).count()
    }

    #[test]
    fn the_delver_gets_step_state_and_steps_as_the_gait_turns() {
        let mut app = delver_app(1.0);
        // 20 frames of 0.2 rad is 4 rad: past pi once.
        for _ in 0..20 {
            app.update();
        }
        let state = app.world_mut().query_filtered::<&StepState, With<Player>>().single(app.world()).is_ok();
        assert!(state, "the player gets a StepState");
        assert_eq!(steps(&mut app), 1);
    }

    #[test]
    fn doubling_pace_doubles_the_steps() {
        let count = |pace: f32| {
            let mut app = delver_app(pace);
            for _ in 0..100 {
                app.update();
            }
            steps(&mut app)
        };
        let (walk, hurry) = (count(0.5), count(1.0));
        assert!(walk >= 3, "{walk}");
        assert_eq!(hurry, walk * 2);
    }

    #[test]
    fn a_still_delver_makes_no_steps() {
        let mut app = delver_app(0.0);
        for _ in 0..100 {
            app.update();
        }
        assert_eq!(steps(&mut app), 0);
    }

    #[test]
    fn no_footsteps_when_still() {
        use bevy::state::app::StatesPlugin;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .init_resource::<GameSettings>()
            .init_resource::<Tuning>()
            .init_resource::<FootstepSounds>();

        app.world_mut().spawn((
            Player,
            Gait { phase: 0.0, pace: 0.0 },
            StepState { rng: Mulberry32::new(42), last_sample: 0, prev_phase: 0.0, last_pos: None },
        ));

        app.add_systems(Update, play_delver_steps);
        app.update();

        // Should not spawn any footstep when pace is 0.
        let count = app
            .world_mut()
            .query::<&AudioPlayer>()
            .iter(app.world())
            .count();
        assert_eq!(count, 0, "no footsteps when still");
    }

    #[test]
    fn creature_footsteps_on_movement() {
        use bevy::state::app::StatesPlugin;
        use fb_creatures::Species;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .init_resource::<GameSettings>()
            .init_resource::<Tuning>()
            .init_resource::<FootstepSounds>();

        // Create a rat creature spot
        let spot = fb_level::CreatureSpot { x: 0.0, y: 0.0, yaw: 0.0, species: Species::Rat, seed: 42, pack: 0 };
        let creature = spot.creature();
        let critter = Critter::new(spot, creature);

        // Spawn rat with Driven component and movement
        let rat_entity = app.world_mut().spawn((
            critter,
            Driven,
            Transform::from_translation(Vec3::ZERO),
            Visibility::default(),
        )).id();

        // Initialize creature steps (adds StepState and WalksWithSteps)
        app.add_systems(Update, init_creature_steps);
        app.update();

        // Move the rat and play steps
        app.world_mut().get_mut::<Transform>(rat_entity).unwrap().translation = Vec3::new(1.0, 0.0, 0.0);

        app.add_systems(Update, play_creature_steps);
        app.update();

        // Should spawn footsteps when rat moves
        let count = app
            .world_mut()
            .query::<&AudioPlayer>()
            .iter(app.world())
            .count();
        assert!(count > 0, "rat moving should produce footsteps");
    }

    #[test]
    fn no_creature_footsteps_when_still() {
        use bevy::state::app::StatesPlugin;
        use fb_creatures::Species;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .init_resource::<GameSettings>()
            .init_resource::<Tuning>()
            .init_resource::<FootstepSounds>();

        // Create a rat creature spot
        let spot = fb_level::CreatureSpot { x: 0.0, y: 0.0, yaw: 0.0, species: Species::Rat, seed: 42, pack: 0 };
        let creature = spot.creature();
        let critter = Critter::new(spot, creature);

        // Spawn rat with Driven component but no movement
        app.world_mut().spawn((
            critter,
            Driven,
            Transform::from_translation(Vec3::ZERO),
            Visibility::default(),
            StepState { rng: Mulberry32::new(42), last_sample: 0, prev_phase: 0.0, last_pos: Some(Vec2::ZERO) },
            WalksWithSteps,
        ));

        app.add_systems(Update, play_creature_steps);
        app.update();

        // Should not spawn footsteps when rat is still
        let count = app
            .world_mut()
            .query::<&AudioPlayer>()
            .iter(app.world())
            .count();
        assert_eq!(count, 0, "still rat should not produce footsteps");
    }

    #[test]
    fn snake_footsteps_disabled() {
        use bevy::state::app::StatesPlugin;
        use fb_creatures::Species;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .init_resource::<GameSettings>()
            .init_resource::<Tuning>()
            .init_resource::<FootstepSounds>();

        // Create a snake creature spot
        let spot = fb_level::CreatureSpot { x: 0.0, y: 0.0, yaw: 0.0, species: Species::PoisonousSnake, seed: 42, pack: 0 };
        let creature = spot.creature();
        let critter = Critter::new(spot, creature);

        // Spawn snake with Driven component and movement
        let snake_entity = app.world_mut().spawn((
            critter,
            Driven,
            Transform::from_translation(Vec3::ZERO),
            Visibility::default(),
            StepState { rng: Mulberry32::new(42), last_sample: 0, prev_phase: 0.0, last_pos: Some(Vec2::ZERO) },
            WalksWithSteps,
        )).id();

        // Move the snake
        app.world_mut().get_mut::<Transform>(snake_entity).unwrap().translation = Vec3::new(1.0, 0.0, 0.0);

        app.add_systems(Update, play_creature_steps);
        app.update();

        // Should not spawn any footsteps for snakes (they slither)
        let count = app
            .world_mut()
            .query::<&AudioPlayer>()
            .iter(app.world())
            .count();
        assert_eq!(count, 0, "snake should not produce footsteps");
    }
}
