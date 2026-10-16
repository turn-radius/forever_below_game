//! The delver's own breathing: soft, muted loops under everything else.
//!
//! After a sprint, or when stamina runs low, the delver breathes hard for a few seconds and
//! settles as stamina refills. With exhaustion of level 1 or more a slow heavy breath stays
//! under everything, louder and more ragged with each level, until the level returns to 0.
//!
//! Which loop plays and how loud is a pure function ([`breathing`]) of the stamina fraction,
//! whether the delver is winded and their exhaustion level. Three loops are synthesised
//! (`tools/synth_breath.py`): [`Breath::Steady`], [`Breath::Harder`] and [`Breath::Ragged`].
//! They are not spatial (it is the delver's own breath), play at a quiet fraction of the
//! effects volume ([`BREATH_GAIN`]), crossfade over [`BREATH_FADE`] seconds rather than cutting,
//! follow the sound volume and tuning live, and pause while any screen is open.

use bevy::{
    audio::{AudioSinkPlayback, Volume},
    prelude::*,
};

use crate::audio::effect_volume;
use crate::character::DelverStamina;
use crate::exhaustion::Exhaustion;
use crate::launch::AppState;
use crate::play::{DelveSystems, Player, ScreenOpen};
use crate::settings::GameSettings;
use crate::tuning::Tuning;

/// How the delver breathes, from calmest to most laboured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Breath {
    /// Slow and even.
    Steady,
    /// Faster and deeper.
    Harder,
    /// Fast and uneven.
    Ragged,
}

impl Breath {
    /// How loud this breath is against the others.
    fn weight(self) -> f32 {
        match self {
            Breath::Steady => 0.6,
            Breath::Harder => 0.8,
            Breath::Ragged => 1.0,
        }
    }
}

/// Gain of the breath against the effects volume before the weight and exhaustion: breathing is
/// quiet, felt more than heard.
pub const BREATH_GAIN: f32 = 0.25;

/// How much louder each exhaustion level makes the breath, as a fraction of its gain.
pub const EXHAUSTION_GAIN_STEP: f32 = 0.15;

/// Seconds a loop takes to fade in or out.
pub const BREATH_FADE: f32 = 1.5;

/// Stamina fractions under which the delver breathes steadily, harder and raggedly. Winded
/// delvers breathe raggedly whatever the fraction.
const STEADY_BELOW: f32 = 0.7;
const HARDER_BELOW: f32 = 0.45;
const RAGGED_BELOW: f32 = 0.2;

/// The breath for a stamina `fraction` (0 to 1), whether the delver is `winded`, and their
/// `exhaustion` level (0 to 6): which loop plays and its gain against the effects volume, or
/// `None` when they breathe unheard (rested, level 0).
///
/// Stamina sets one strength and exhaustion another (steady at levels 1 and 2, harder at 3 and 4,
/// ragged from 5); the more laboured wins. Each exhaustion level makes either a little louder.
pub fn breathing(fraction: f32, winded: bool, exhaustion: u32) -> Option<(Breath, f32)> {
    let from_stamina = if winded || fraction < RAGGED_BELOW {
        Some(Breath::Ragged)
    } else if fraction < HARDER_BELOW {
        Some(Breath::Harder)
    } else if fraction < STEADY_BELOW {
        Some(Breath::Steady)
    } else {
        None
    };
    let from_exhaustion = match exhaustion {
        0 => None,
        1 | 2 => Some(Breath::Steady),
        3 | 4 => Some(Breath::Harder),
        _ => Some(Breath::Ragged),
    };
    let breath = from_stamina.max(from_exhaustion)?;
    let louder = 1.0 + EXHAUSTION_GAIN_STEP * exhaustion.min(6) as f32;
    Some((breath, BREATH_GAIN * breath.weight() * louder))
}

/// The loops' sound assets.
#[derive(Resource, Clone, Debug, Default)]
struct BreathSounds {
    steady: Handle<AudioSource>,
    harder: Handle<AudioSource>,
    ragged: Handle<AudioSource>,
}

impl BreathSounds {
    fn of(&self, breath: Breath) -> &Handle<AudioSource> {
        match breath {
            Breath::Steady => &self.steady,
            Breath::Harder => &self.harder,
            Breath::Ragged => &self.ragged,
        }
    }
}

/// One playing breath loop. It fades in while it is the delver's breath and out when it is not,
/// and is despawned once silent.
#[derive(Component, Debug)]
pub struct BreathLoop {
    pub breath: Breath,
    /// From 0 (silent) to 1 (full), moved toward the loop's target each frame.
    pub fade: f32,
    /// Its gain against the effects volume while it is the delver's breath.
    pub gain: f32,
}

pub struct BreathPlugin;

impl Plugin for BreathPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BreathSounds>()
            .init_resource::<ScreenOpen>()
            .init_resource::<GameSettings>()
            .init_resource::<Tuning>()
            .add_systems(Startup, load_breath_sounds.run_if(resource_exists::<AssetServer>))
            .add_systems(Update, breathe.in_set(DelveSystems));
    }
}

fn load_breath_sounds(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(BreathSounds {
        steady: assets.load("audio/breath_steady.ogg"),
        harder: assets.load("audio/breath_harder.ogg"),
        ragged: assets.load("audio/breath_ragged.ogg"),
    });
}

/// Equal-power gain for a fade fraction.
fn fade_curve(fade: f32) -> f32 {
    (std::f32::consts::FRAC_PI_2 * fade.clamp(0.0, 1.0)).sin()
}

#[allow(clippy::too_many_arguments)]
fn breathe(
    mut commands: Commands,
    time: Res<Time>,
    screen: Res<ScreenOpen>,
    sounds: Res<BreathSounds>,
    game: Res<GameSettings>,
    tuning: Res<Tuning>,
    global: Option<Res<GlobalVolume>>,
    delver: Query<(&DelverStamina, &Exhaustion), With<Player>>,
    mut loops: Query<(Entity, &mut BreathLoop, Option<&mut AudioSink>)>,
) {
    // A screen freezes the breath where it is: no fading, no sound.
    if screen.0.is_some() {
        for (_, _, sink) in &mut loops {
            if let Some(sink) = sink {
                sink.pause();
            }
        }
        return;
    }
    let target = delver.single().ok().and_then(|(stamina, exhaustion)| breathing(stamina.0.value as f32, stamina.0.winded, exhaustion.0.level));
    let step = time.delta_secs() / BREATH_FADE;
    let volume = effect_volume(&game, &tuning) * global.map_or(1.0, |global| global.volume.to_linear());

    if let Some((breath, gain)) = target
        && !loops.iter().any(|(_, l, _)| l.breath == breath)
    {
        commands.spawn((
            Name::new("delver breath"),
            BreathLoop { breath, fade: 0.0, gain },
            AudioPlayer::new(sounds.of(breath).clone()),
            PlaybackSettings::LOOP.with_volume(Volume::SILENT),
            DespawnOnExit(AppState::Delving),
        ));
    }
    for (entity, mut breath_loop, sink) in &mut loops {
        match target.filter(|(b, _)| *b == breath_loop.breath) {
            Some((_, gain)) => {
                breath_loop.gain = gain;
                breath_loop.fade = (breath_loop.fade + step).min(1.0);
            }
            None => breath_loop.fade = (breath_loop.fade - step).max(0.0),
        }
        if breath_loop.fade <= 0.0 && target.is_none_or(|(b, _)| b != breath_loop.breath) {
            commands.entity(entity).despawn();
        } else if let Some(mut sink) = sink {
            if sink.is_paused() {
                sink.play();
            }
            sink.set_volume(Volume::Linear(volume * breath_loop.gain * fade_curve(breath_loop.fade)));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::{state::app::StatesPlugin, time::TimeUpdateStrategy};

    use super::*;

    fn gain(fraction: f32, winded: bool, level: u32) -> f32 {
        breathing(fraction, winded, level).map_or(0.0, |(_, gain)| gain)
    }

    #[test]
    fn a_rested_delver_at_level_zero_does_not_breathe_audibly() {
        assert_eq!(breathing(1.0, false, 0), None);
        assert_eq!(breathing(0.7, false, 0), None);
    }

    #[test]
    fn low_stamina_breathes_harder_as_it_falls() {
        assert_eq!(breathing(0.6, false, 0).map(|b| b.0), Some(Breath::Steady));
        assert_eq!(breathing(0.3, false, 0).map(|b| b.0), Some(Breath::Harder));
        assert_eq!(breathing(0.1, false, 0).map(|b| b.0), Some(Breath::Ragged));
        // Winded is ragged even if the fraction has climbed back past the lowest band.
        assert_eq!(breathing(0.3, true, 0).map(|b| b.0), Some(Breath::Ragged));
        assert!(gain(0.6, false, 0) < gain(0.3, false, 0) && gain(0.3, false, 0) < gain(0.1, false, 0));
    }

    #[test]
    fn exhaustion_breathes_louder_and_more_ragged_per_level() {
        let breaths: Vec<_> = (1..=6).map(|level| breathing(1.0, false, level).unwrap()).collect();
        assert!(breaths.windows(2).all(|w| w[0].0 <= w[1].0), "{breaths:?}");
        assert_eq!(breaths[0].0, Breath::Steady);
        assert_eq!(breaths[5].0, Breath::Ragged);
        assert!(breaths.windows(2).all(|w| w[0].1 < w[1].1), "{breaths:?}");
    }

    #[test]
    fn the_more_laboured_source_wins_and_the_breath_stays_quiet() {
        assert_eq!(breathing(0.1, false, 1).map(|b| b.0), Some(Breath::Ragged));
        assert_eq!(breathing(1.0, false, 4).map(|b| b.0), Some(Breath::Harder));
        assert!(gain(0.0, true, 0) <= BREATH_GAIN);
        assert!(gain(1.0, false, 1) < 0.5);
    }

    fn breath_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)))
            .init_resource::<GameSettings>()
            .init_resource::<Tuning>()
            .init_resource::<ScreenOpen>()
            .init_resource::<BreathSounds>()
            .add_systems(Update, breathe);
        app.world_mut().spawn((Player, DelverStamina::default(), Exhaustion::default()));
        app
    }

    fn run(app: &mut App, seconds: f32) {
        for _ in 0..(seconds * 10.0) as usize {
            app.update();
        }
    }

    fn set_level(app: &mut App, level: u32) {
        app.world_mut().query::<&mut Exhaustion>().single_mut(app.world_mut()).unwrap().0 = fb_character::Exhaustion::new(level);
    }

    fn set_stamina(app: &mut App, value: f64, winded: bool) {
        app.world_mut().query::<&mut DelverStamina>().single_mut(app.world_mut()).unwrap().0 = fb_character::Stamina { value, winded };
    }

    fn loops(app: &mut App) -> Vec<(Breath, f32, f32)> {
        app.world_mut().query::<&BreathLoop>().iter(app.world()).map(|l| (l.breath, l.fade, l.gain)).collect()
    }

    #[test]
    fn rested_at_level_zero_plays_no_loop() {
        let mut app = breath_app();
        run(&mut app, 5.0);
        assert!(loops(&mut app).is_empty());
    }

    #[test]
    fn level_two_plays_louder_than_level_one() {
        let mut app = breath_app();
        set_level(&mut app, 1);
        run(&mut app, 3.0);
        let one = loops(&mut app);
        assert_eq!(one.len(), 1);
        assert!((one[0].1 - 1.0).abs() < 1e-6, "faded fully in: {one:?}");
        set_level(&mut app, 2);
        run(&mut app, 1.0);
        let two = loops(&mut app);
        assert_eq!(two.len(), 1);
        assert!(two[0].2 > one[0].2, "{one:?} vs {two:?}");
    }

    #[test]
    fn dropping_to_level_zero_fades_the_loop_out_rather_than_cutting() {
        let mut app = breath_app();
        set_level(&mut app, 2);
        run(&mut app, 3.0);
        assert_eq!(loops(&mut app).len(), 1);
        set_level(&mut app, 0);
        run(&mut app, 0.5);
        let fading = loops(&mut app);
        assert_eq!(fading.len(), 1, "still there");
        assert!(fading[0].1 > 0.0 && fading[0].1 < 1.0, "{fading:?}");
        run(&mut app, 2.0);
        assert!(loops(&mut app).is_empty());
    }

    #[test]
    fn a_sprint_to_winded_breathes_hard_then_settles_as_stamina_recovers() {
        let mut app = breath_app();
        run(&mut app, 1.0);
        assert!(loops(&mut app).is_empty());
        set_stamina(&mut app, 0.0, true);
        run(&mut app, 3.0);
        let hard = loops(&mut app);
        assert_eq!(hard.len(), 1);
        assert_eq!(hard[0].0, Breath::Ragged);
        // Refilled past second wind and well rested: the breath fades away.
        set_stamina(&mut app, 1.0, false);
        run(&mut app, 3.0);
        assert!(loops(&mut app).is_empty());
    }

    #[test]
    fn an_open_screen_freezes_the_fade() {
        let mut app = breath_app();
        set_level(&mut app, 2);
        run(&mut app, 0.5);
        let before = loops(&mut app);
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(crate::play::Screen::Menu);
        run(&mut app, 3.0);
        assert_eq!(loops(&mut app), before);
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        run(&mut app, 3.0);
        assert!((loops(&mut app)[0].1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_loops_are_ogg_assets() {
        for name in ["steady", "harder", "ragged"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/audio").join(format!("breath_{name}.ogg"));
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(crate::audio::ogg_vorbis_duration(&bytes).unwrap() > 5.0, "{name}");
        }
    }
}
