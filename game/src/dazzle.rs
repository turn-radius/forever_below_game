//! Stepping from long darkness into bright light dazzles the delver for a moment: the `dazzled`
//! effect in `data/effects.json`, which glares over the view and shortens sight while it fades
//! (`crate::sight`). Eyes used to the dark ([`Adaptation`]) are what make the light hurt, so a
//! lantern lit after five dark minutes dazzles and one lit after a lit minute does not. Our own,
//! after how eyes adapt; the SRD has no dazzle.

use bevy::{camera::Exposure, prelude::*};
use fb_character::effect::{Effects, Source};

use crate::character::DelverEffects;
use crate::game_clock::GameClock;
use crate::light::lit_level;
use crate::play::{DelveSystems, EYE_HEIGHT, Player};

/// The effect bright light after darkness brings on.
pub const DAZZLED: &str = "dazzled";
/// Light on the eye below this is darkness the eyes get used to.
pub const DARK: f32 = 0.12;
/// Light above this, met with eyes used to the dark, dazzles.
pub const BRIGHT: f32 = 0.45;
/// Game minutes of darkness before bright light dazzles.
pub const DARK_MINUTES: f64 = 3.0;
/// Game minutes of darkness that give the fullest dazzle.
pub const FULL_MINUTES: f64 = 20.0;

pub struct DazzlePlugin;

impl Plugin for DazzlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, dazzle_the_delver.in_set(DelveSystems));
    }
}

/// How used to the dark the delver's eyes are: game minutes spent in it lately.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Adaptation {
    pub dark_minutes: f64,
}

impl Adaptation {
    /// Takes `minutes` of game time with `lit` light on the eye, and returns how hard the light
    /// dazzles (0 to 1) if it has just turned bright on eyes used to the dark. Middling light
    /// wears the eyes' adaptation off twice as fast as darkness builds it.
    pub fn see(&mut self, lit: f32, minutes: f64) -> Option<f64> {
        if lit < DARK {
            self.dark_minutes += minutes;
            return None;
        }
        let used_to_the_dark = self.dark_minutes;
        if lit > BRIGHT {
            self.dark_minutes = 0.0;
            return (used_to_the_dark >= DARK_MINUTES).then(|| (0.5 + 0.5 * (used_to_the_dark - DARK_MINUTES) / (FULL_MINUTES - DARK_MINUTES)).min(1.0));
        }
        self.dark_minutes = (self.dark_minutes - 2.0 * minutes).max(0.0);
        None
    }
}

/// Watches the light on the delver's eyes and dazzles them when it turns bright after darkness.
fn dazzle_the_delver(
    mut commands: Commands,
    clock: Res<GameClock>,
    rules: Option<Res<crate::rules::GameRules>>,
    lights: Query<(&GlobalTransform, &PointLight)>,
    mut players: Query<(Entity, &Transform, Option<&mut DelverEffects>), With<Player>>,
    mut state: Local<(Adaptation, Option<f64>)>,
    mut jumps: Local<u32>,
) {
    let (eyes, last) = &mut *state;
    let now = clock.game_seconds();
    // A new delve restarts the clock below where this last stood, and a clock set at a stroke (a
    // delve resumed) lets no time pass.
    let jumped = std::mem::replace(&mut *jumps, clock.jumps()) != clock.jumps();
    let minutes = last.map_or(0.0, |was| if jumped || now < was { 0.0 } else { (now - was) / 60.0 });
    *last = Some(now);
    let Some((player, at, effects)) = players.iter_mut().next() else { return };
    let eye = at.translation + Vec3::Y * EYE_HEIGHT;
    let lit = lit_level(eye, lights.iter().map(|(t, l)| (t.translation(), l)), Exposure::INDOOR.exposure());
    let Some(intensity) = eyes.see(lit, minutes) else { return };
    let defs = &crate::rules::GameRules::of(&rules).effects;
    match effects {
        Some(mut effects) => effects.0.add(defs, DAZZLED, intensity, None, Source::Surroundings),
        None => {
            let mut fresh = Effects::default();
            fresh.add(defs, DAZZLED, intensity, None, Source::Surroundings);
            commands.entity(player).insert(DelverEffects(fresh));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `minutes` of game time at `lit`, a minute at a time, then whether a bright light dazzles.
    fn after(lit: f32, minutes: u32) -> Option<f64> {
        let mut eyes = Adaptation::default();
        for _ in 0..minutes {
            assert_eq!(eyes.see(lit, 1.0), None);
        }
        eyes.see(0.9, 0.0)
    }

    #[test]
    fn a_light_lit_after_five_dark_minutes_dazzles_and_after_a_lit_minute_does_not() {
        assert!(after(0.02, 5).is_some(), "five dark minutes");
        assert_eq!(after(0.6, 1), None, "a lit minute");
        assert_eq!(after(0.02, 2), None, "two dark minutes are not enough");
    }

    #[test]
    fn longer_darkness_dazzles_harder_up_to_full() {
        let (short, long) = (after(0.02, 4).unwrap(), after(0.02, 15).unwrap());
        assert!(long > short, "{short} then {long}");
        assert_eq!(after(0.02, 40), Some(1.0));
    }

    #[test]
    fn one_dazzle_uses_up_the_darkness_and_middling_light_wears_it_off() {
        let mut eyes = Adaptation::default();
        eyes.see(0.02, 10.0);
        assert!(eyes.see(0.9, 0.0).is_some());
        assert_eq!(eyes.see(0.9, 0.0), None, "the eyes have adjusted");
        eyes.see(0.02, 10.0);
        eyes.see(0.3, 5.0);
        assert_eq!(eyes.see(0.9, 0.0), None, "five middling minutes undid ten dark ones");
    }

    #[test]
    fn dazzled_fades_within_a_game_minute() {
        let defs = fb_character::effect::shipped();
        let mut effects = Effects::default();
        effects.add(&defs, DAZZLED, 1.0, None, Source::Surroundings);
        assert!(effects.glare(&defs) > 0.0 && effects.sight(&defs) < 1.0);
        effects.step(&defs, 1.0 / 60.0, &[]);
        assert!(effects.active.is_empty(), "gone after a game minute");
    }
}
