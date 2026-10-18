//! Footsteps as the delver and folk walk: varied stone steps with cadence matching speed.
//!
//! The delver plays a non-spatial footstep at each stride, paced by their walking speed.
//! Standing still plays no steps; doubling speed doubles the step rate. The same sample never
//! plays twice in a row. Burdened and Encumbered steps are heavier (lower, a little longer) than
//! light steps. All steps are soft and quiet, with a little random gain and speed each.
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

/// Gain on the delver's step against the effects volume: footsteps are background detail, soft
/// and dull under the other effects.
const STEP_GAIN: f32 = 0.35;

/// A creature's step as a share of the delver's. Spatial playback already fades it with distance
/// (scale 0.1, so a rat 20 units away is 2 m off and plays at a quarter of this), which keeps a
/// rat's step quieter than the delver's own but present at a few metres.
const CREATURE_STEP_SHARE: f32 = 0.7;

/// Per-step random spread of gain (a fraction either way) and playback speed, so repeats of the
/// same sample do not sound identical.
const GAIN_SPREAD: f32 = 0.15;
const SPEED_SPREAD: f32 = 0.06;

/// Playback for one step: `gain` against the effects volume, varied by up to `GAIN_SPREAD` and
/// `SPEED_SPREAD` from the entity's seeded RNG.
fn step_settings(rng: &mut Mulberry32, gain: f32, effects: f32) -> PlaybackSettings {
    let jitter = |rng: &mut Mulberry32, spread: f32| 1.0 + spread * (rng.next_f64() as f32 * 2.0 - 1.0);
    let volume = effects * gain * jitter(rng, GAIN_SPREAD);
    let speed = jitter(rng, SPEED_SPREAD);
    PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)).with_speed(speed)
}

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
    rules: Option<Res<crate::rules::GameRules>>,
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
            let derived = character.derive_with(crate::rules::GameRules::of(&rules), &inventory);
            burden_threshold(derived.load, derived.capacity)
        }
        _ => false,
    };
    let samples = if is_burdened { &sounds.heavy } else { &sounds.light };
    let settings = step_settings(&mut state.rng, STEP_GAIN, effect_volume(&game, &tuning));

    commands.spawn((
        Name::new("delver step"),
        AudioPlayer::new(samples[sample].clone()),
        settings,
        DespawnOnExit(AppState::Delving),
    ));
}

/// Add step state to creatures when they become Driven (active in behaviour).
fn init_creature_steps(mut commands: Commands, new: Query<(Entity, &Critter, &Transform), Added<Driven>>) {
    for (entity, critter, transform) in &new {
        // What flies makes no footsteps.
        if critter.spot.species.is_flying() {
            continue;
        }
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
    bones: Option<Res<crate::creatures::skeletons::BoneSounds>>,
    mut creatures: Query<(&Critter, &Transform, &mut StepState, Option<&crate::creatures::trolls::Troll>), (With<Driven>, With<WalksWithSteps>, Without<crate::combat::feedback::Corpse>)>,
) {
    const RAT_STRIDE: f32 = 0.5;  // World units per step for rats.

    for (critter, transform, mut state, troll) in &mut creatures {
        // Snakes don't make footsteps; they slither.
        if critter.spot.species.is_snake() {
            state.last_pos = Some(transform.translation.xz());
            continue;
        }

        // A skeleton clicks along on bone, a step to each of its own strides (a step is 0.7 of
        // its leg); rats make light, quick footsteps. Accumulate distance and step at stride
        // intervals.
        let skeleton = critter.spot.species.is_skeleton();
        let stride = match (&critter.kit, &troll, skeleton) {
            (Some(kit), _, true) => 0.7 * (kit.dims.thigh + kit.dims.shin) * critter.size,
            // A troll's heavy tread keeps to its own long stride.
            (_, Some(troll), _) => 0.7 * (troll.dims.thigh + troll.dims.shin) * critter.size,
            // A spider's eight feet patter: a tick every quarter of its radius.
            _ if critter.spot.species.is_spider() => 0.25 * critter.creature.radius() * critter.size,
            _ => RAT_STRIDE,
        };
        let current_pos = transform.translation.xz();
        let moved = state.last_pos.map_or(0.0, |last| current_pos.distance(last));

        // Accumulate movement distance in prev_phase.
        state.prev_phase += moved;
        state.last_pos = Some(current_pos);

        // Step when accumulated distance exceeds stride threshold.
        if state.prev_phase >= stride {
            state.prev_phase -= stride;

            let old_last = state.last_sample;
            let sample = next_sample(&mut state.rng, old_last);
            state.last_sample = sample;

            // Rats use light steps always, quieter than the delver's; distance fades them. A troll's
            // are the heavy ones, and a little louder.
            let gain = if troll.is_some() { 1.4 * STEP_GAIN * CREATURE_STEP_SHARE } else { STEP_GAIN * CREATURE_STEP_SHARE };
            let settings = step_settings(&mut state.rng, gain, effect_volume(&game, &tuning));

            let clip = match (&bones, skeleton) {
                (Some(bones), true) => bones.steps[sample].clone(),
                (None, true) => continue,
                _ if troll.is_some() => sounds.heavy[sample].clone(),
                _ => sounds.light[sample].clone(),
            };
            commands.spawn((
                Name::new("creature step"),
                AudioPlayer::new(clip),
                settings.with_spatial(true),
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
    fn what_flies_makes_no_footsteps_and_what_walks_does() {
        let mut app = App::new();
        app.add_systems(Update, init_creature_steps);
        let spot = |species| fb_level::CreatureSpot { x: 1.0, y: 1.0, yaw: 0.0, species, seed: 3, pack: 0 };
        let mut spawn = |species| {
            let spot = spot(species);
            app.world_mut().spawn((Critter::new(spot, spot.creature()), Transform::default(), Driven)).id()
        };
        let (bat, giant, rat) = (spawn(fb_creatures::Species::Bat), spawn(fb_creatures::Species::GiantBat), spawn(fb_creatures::Species::Rat));
        app.update();
        assert!(!app.world().entity(bat).contains::<WalksWithSteps>() && !app.world().entity(giant).contains::<WalksWithSteps>());
        assert!(app.world().entity(rat).contains::<WalksWithSteps>());
    }

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
    fn step_settings_vary_within_their_spread() {
        let mut rng = Mulberry32::new(7);
        let (mut volumes, mut speeds) = (Vec::new(), Vec::new());
        for _ in 0..200 {
            let settings = step_settings(&mut rng, STEP_GAIN, 1.0);
            let Volume::Linear(volume) = settings.volume else { panic!("linear volume") };
            assert!((STEP_GAIN * (1.0 - GAIN_SPREAD)..=STEP_GAIN * (1.0 + GAIN_SPREAD)).contains(&volume), "{volume}");
            assert!((1.0 - SPEED_SPREAD..=1.0 + SPEED_SPREAD).contains(&settings.speed), "{}", settings.speed);
            volumes.push(volume);
            speeds.push(settings.speed);
        }
        assert!(volumes.iter().any(|v| *v > STEP_GAIN) && volumes.iter().any(|v| *v < STEP_GAIN));
        assert!(speeds.iter().any(|s| *s > 1.0) && speeds.iter().any(|s| *s < 1.0));
    }

    #[test]
    fn delver_steps_play_at_a_fraction_of_the_effects_volume() {
        let mut app = delver_app(1.0);
        for _ in 0..20 {
            app.update();
        }
        let effects = effect_volume(app.world().resource::<GameSettings>(), app.world().resource::<Tuning>());
        let settings: Vec<_> = app.world_mut().query::<&PlaybackSettings>().iter(app.world()).cloned().collect();
        assert_eq!(settings.len(), 1);
        let Volume::Linear(volume) = settings[0].volume else { panic!("linear volume") };
        assert!(volume <= effects * STEP_GAIN * (1.0 + GAIN_SPREAD) + 1e-6, "{volume} vs {effects}");
        assert!(volume >= effects * STEP_GAIN * (1.0 - GAIN_SPREAD) - 1e-6, "{volume} vs {effects}");
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

    #[test]
    fn skeletons_click_along_on_bone_never_on_a_rats_steps() {
        use crate::creatures::skeletons::BoneSounds;
        use bevy::state::app::StatesPlugin;
        use fb_creatures::Species;

        let bone: Handle<AudioSource> = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(0xB0), std::marker::PhantomData);
        let bone_steps = [bone.clone(), Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(0xB1), std::marker::PhantomData), Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(0xB2), std::marker::PhantomData)];
        let light = [0x10, 0x11, 0x12].map(|n| Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(n), std::marker::PhantomData));
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .init_resource::<GameSettings>()
            .init_resource::<Tuning>()
            .insert_resource(FootstepSounds { light: light.clone(), heavy: light.clone() })
            .insert_resource(BoneSounds { steps: bone_steps.clone(), ..default() });
        let spot = fb_level::CreatureSpot { x: 0.0, y: 0.0, yaw: 0.0, species: Species::Skeleton, seed: 42, pack: 0 };
        let critter = Critter::new(spot, spot.creature());
        let stride = {
            let kit = critter.kit.as_ref().unwrap();
            0.7 * (kit.dims.thigh + kit.dims.shin)
        };
        let skeleton = app
            .world_mut()
            .spawn((critter, Driven, Transform::default(), Visibility::default(), StepState { rng: Mulberry32::new(42), last_sample: 0, prev_phase: 0.0, last_pos: Some(Vec2::ZERO) }, WalksWithSteps))
            .id();
        app.add_systems(Update, play_creature_steps);
        // A rat's stride is half a unit: a skeleton walking that far is not yet at its first step.
        app.world_mut().get_mut::<Transform>(skeleton).unwrap().translation = Vec3::new(0.6, 0.0, 0.0);
        app.update();
        assert_eq!(app.world_mut().query::<&AudioPlayer>().iter(app.world()).count(), 0, "a skeleton's step is its own stride, not a rat's");
        app.world_mut().get_mut::<Transform>(skeleton).unwrap().translation = Vec3::new(stride + 1.0, 0.0, 0.0);
        app.update();
        let heard: Vec<Handle<AudioSource>> = app.world_mut().query::<&AudioPlayer<AudioSource>>().iter(app.world()).map(|p| p.0.clone()).collect();
        assert_eq!(heard.len(), 1, "one step for a stride walked");
        assert!(bone_steps.contains(&heard[0]), "bone, not the delver's or a rat's light step");
    }
}
