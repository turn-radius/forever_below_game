//! Forever Below effects: what a spell or an item shows when it goes off, described as data
//! (`data/effects/*.toml`) and played by a deterministic particle simulation. Plain Rust, no
//! Bevy: the game draws what [`Sim`] reports. See `docs/effects.md`.

pub mod effect;
pub mod envelope;
pub mod sim;

pub use effect::{Anchor, Card, ColourStop, Effect, Emitter, Follows, Light, Motion, MotionKind, Shape, SizeStop, Spawn, Timing, TimingKind, V3};
pub use sim::{LightOut, ParticleOut, Places, Sim, Target};

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../data/effects/test_burst.toml");

    #[test]
    fn the_sample_file_round_trips_unchanged() {
        let effect = Effect::from_toml(SAMPLE).unwrap();
        assert_eq!(effect.name, "test_burst");
        assert!(!effect.emitters.is_empty() && effect.light.is_some());
        let again = Effect::from_toml(&effect.to_toml().unwrap()).unwrap();
        assert_eq!(effect, again);
    }

    #[test]
    fn a_key_left_out_keeps_its_default() {
        let effect = Effect::from_toml("name = \"x\"\n[[emitters]]\ncard = \"mote\"\n").unwrap();
        assert_eq!(effect.emitters[0].card, Card::Mote);
        assert_eq!(effect.emitters[0].timing, Timing::default());
        assert!(Effect::from_toml("length = \"long\"").is_err());
    }

    #[test]
    fn a_file_the_simulation_cannot_play_is_refused() {
        let emitter = |body: &str| format!("[[emitters]]\n{body}\n");
        let bad = [
            ("an unknown key", "colour_of_sky = 3".to_string()),
            ("an unknown key in an emitter", emitter("sparkle = 3")),
            ("length 0", "length = 0.0".to_string()),
            ("a negative length", "length = -2.0".to_string()),
            ("a length that is not a number", "length = nan".to_string()),
            ("an infinite bloom", "bloom_in = inf".to_string()),
            ("a life of 0", emitter("[emitters.timing]\nlife = 0.0")),
            ("a count over the cap", emitter("[emitters.timing]\ncount = 2001")),
            ("a speed that is not a number", emitter("[emitters.motion]\nspeed = nan")),
            ("size stops out of order", emitter("size = [{ at = 0.8, size = 1.0 }, { at = 0.2, size = 2.0 }]")),
            ("colour stops out of order", emitter("colour = [{ at = 0.8, rgb = [1, 1, 1] }, { at = 0.2, rgb = [1, 1, 1] }]")),
            ("an unknown key in a stop", emitter("size = [{ at = 0.0, size = 1.0, weight = 2 }]")),
        ];
        for (what, text) in bad {
            assert!(Effect::from_toml(&text).is_err(), "{what} was taken");
        }
        assert!(Effect::from_toml(&emitter("[emitters.timing]\ncount = 2000")).is_ok());
    }
}
