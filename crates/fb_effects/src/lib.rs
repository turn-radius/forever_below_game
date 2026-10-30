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
}
