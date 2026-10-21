//! The delver's voice: a grunt of pain when a blow lands on them, over the blow's thud.
//!
//! - Every blow that hits ([`DelverHurt`], sent by [`super::vitals`]) draws a grunt: one of
//!   three takes, never the same twice running, louder the harder the blow; a blow that takes
//!   [`HARD_SHARE`] of their hit points or more, or brings them to 0, draws the hard cry.
//! - Several blows in one frame draw one grunt, for the hardest, and none starts within
//!   [`GRUNT_GAP`] seconds of the last, so a flurry does not stutter.
//! - The voice follows the delver ([`voice_for`]): the low or the high recording by their sex,
//!   played a little faster or slower by their folk, so a gnome is higher and a dwarf deeper.
//!   The grunts are synthesised by `tools/synth_grunt.py`. It is their own voice, so it is not
//!   spatial, and it follows the effects volume. Our own.

use bevy::{audio::Volume, prelude::*};
use fb_body::Folk;
use procgen_rng::Mulberry32;

use crate::character::{Character, Sex};
use crate::launch::AppState;
use crate::play::DelveSystems;

pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DelverHurt>().add_systems(Startup, load_grunts).add_systems(Update, cry_out.in_set(DelveSystems));
    }
}

/// A blow landed on the delver for `damage` of their `max` hit points; `down` if it left them
/// at 0.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DelverHurt {
    pub damage: i32,
    pub max: i32,
    pub down: bool,
}

/// From this share of the delver's maximum hit points in one blow, they cry out hard. Our own.
pub const HARD_SHARE: f32 = 0.3;
/// The least time between two grunts, seconds. Our own.
pub const GRUNT_GAP: f32 = 0.2;

/// Which recording: the low voice or the high.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    Low,
    High,
}

/// Which grunt: one of the three everyday takes (0 to 2), or the hard cry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grunt {
    Take(usize),
    Hard,
}

/// The grunts, in both voices.
#[derive(Resource, Clone, Debug, Default)]
pub struct Grunts {
    pub low: [Handle<AudioSource>; 3],
    pub low_hard: Handle<AudioSource>,
    pub high: [Handle<AudioSource>; 3],
    pub high_hard: Handle<AudioSource>,
}

impl Grunts {
    pub fn get(&self, voice: Voice, grunt: Grunt) -> Handle<AudioSource> {
        match (voice, grunt) {
            (Voice::Low, Grunt::Take(i)) => self.low[i % 3].clone(),
            (Voice::Low, Grunt::Hard) => self.low_hard.clone(),
            (Voice::High, Grunt::Take(i)) => self.high[i % 3].clone(),
            (Voice::High, Grunt::Hard) => self.high_hard.clone(),
        }
    }
}

fn load_grunts(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    let Some(assets) = assets else { return };
    let takes = |voice: &str| [1, 2, 3].map(|n| assets.load(format!("audio/grunt_{voice}_{n}.ogg")));
    commands.insert_resource(Grunts { low: takes("low"), low_hard: assets.load("audio/grunt_low_hard.ogg"), high: takes("high"), high_hard: assets.load("audio/grunt_high_hard.ogg") });
}

/// The delver's voice and how fast to play it: the high recording for a woman, the low for a
/// man (or a delver with no sex chosen); smaller folk a little higher, bigger ones lower.
pub fn voice_for(folk: Folk, sex: Option<Sex>) -> (Voice, f32) {
    let voice = if sex == Some(Sex::Female) { Voice::High } else { Voice::Low };
    let speed = match folk {
        Folk::Human => 1.0,
        Folk::Dwarf => 0.9,
        Folk::Gnome => 1.12,
        Folk::Goblin => 1.15,
        Folk::Lizardfolk => 0.88,
        Folk::Pigfolk => 0.94,
    };
    (voice, speed)
}

/// The grunt for a blow, and how loud (0 to 1): the hard cry for a big share or a fall to 0,
/// otherwise the take `roll` (0 to 1) picks among those that are not `last`.
pub fn grunt_for(hurt: DelverHurt, last: Option<usize>, roll: f64) -> (Grunt, f32) {
    let share = hurt.damage.max(0) as f32 / hurt.max.max(1) as f32;
    if hurt.down || share >= HARD_SHARE {
        return (Grunt::Hard, 1.0);
    }
    let others: Vec<usize> = (0..3).filter(|t| Some(*t) != last).collect();
    let take = others[((roll * others.len() as f64) as usize).min(others.len() - 1)];
    (Grunt::Take(take), 0.6 + 0.4 * (share / HARD_SHARE).clamp(0.0, 1.0))
}

/// Cries out for the hardest blow that landed this frame.
#[allow(clippy::too_many_arguments)]
fn cry_out(
    mut commands: Commands,
    mut hurts: MessageReader<DelverHurt>,
    time: Res<Time>,
    grunts: Option<Res<Grunts>>,
    character: Option<Res<Character>>,
    settings: Option<Res<crate::settings::GameSettings>>,
    tuning: Option<Res<crate::tuning::Tuning>>,
    mut state: Local<(Option<usize>, Option<f32>, Option<Mulberry32>)>,
) {
    let hardest = hurts.read().copied().filter(|h| h.damage > 0 || h.down).max_by_key(|h| (h.down, h.damage));
    let (Some(hurt), Some(grunts)) = (hardest, grunts) else { return };
    let now = time.elapsed_secs();
    let (last_take, last_at, rng) = &mut *state;
    if last_at.is_some_and(|at| now - at < GRUNT_GAP) {
        return;
    }
    let rng = rng.get_or_insert_with(|| Mulberry32::new(0x6e75_6e74));
    let (grunt, loudness) = grunt_for(hurt, *last_take, rng.next_f64());
    if let Grunt::Take(take) = grunt {
        *last_take = Some(take);
    }
    *last_at = Some(now);
    let (voice, speed) = character.as_deref().map_or((Voice::Low, 1.0), |c| voice_for(c.folk, c.sex));
    let game = settings.as_deref().copied().unwrap_or_default();
    let tuning = tuning.as_deref().cloned().unwrap_or_default();
    let (effect, playback) = crate::audio::sound_effect(&game, &tuning);
    let volume = match playback.volume {
        Volume::Linear(v) => v * loudness,
        other => other.to_linear() * loudness,
    };
    commands.spawn((Name::new("delver grunts"), AudioPlayer::new(grunts.get(voice, grunt)), effect, playback.with_volume(Volume::Linear(volume)).with_speed(speed), DespawnOnExit(AppState::Delving)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hurt(damage: i32, max: i32) -> DelverHurt {
        DelverHurt { damage, max, down: false }
    }

    #[test]
    fn a_big_share_or_a_fall_to_nothing_draws_the_hard_cry() {
        assert_eq!(grunt_for(hurt(3, 10), None, 0.5).0, Grunt::Hard);
        assert_eq!(grunt_for(DelverHurt { damage: 1, max: 10, down: true }, None, 0.5).0, Grunt::Hard);
        assert!(matches!(grunt_for(hurt(2, 10), None, 0.5).0, Grunt::Take(_)));
    }

    #[test]
    fn a_take_never_comes_twice_running_and_a_harder_blow_is_louder() {
        for last in 0..3 {
            for roll in [0.0, 0.3, 0.6, 0.99] {
                assert_ne!(grunt_for(hurt(1, 10), Some(last), roll).0, Grunt::Take(last));
            }
        }
        assert!(grunt_for(hurt(2, 10), None, 0.5).1 > grunt_for(hurt(1, 10), None, 0.5).1);
    }

    #[test]
    fn the_voice_follows_sex_and_folk() {
        assert_eq!(voice_for(Folk::Human, Some(Sex::Female)), (Voice::High, 1.0));
        assert_eq!(voice_for(Folk::Human, None).0, Voice::Low);
        assert!(voice_for(Folk::Gnome, Some(Sex::Male)).1 > 1.0);
        assert!(voice_for(Folk::Dwarf, Some(Sex::Male)).1 < 1.0);
    }

    #[test]
    fn one_grunt_for_a_flurry_of_blows() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin)).insert_state(AppState::Delving).init_resource::<Grunts>().add_plugins(VoicePlugin);
        app.update();
        for damage in [1, 2, 1] {
            app.world_mut().write_message(hurt(damage, 10));
        }
        app.update();
        let count = |app: &mut App| app.world_mut().query::<&AudioPlayer>().iter(app.world()).count();
        assert_eq!(count(&mut app), 1, "one grunt for the frame");
        app.world_mut().write_message(hurt(1, 10));
        app.update();
        assert_eq!(count(&mut app), 1, "none so soon after");
    }
}
