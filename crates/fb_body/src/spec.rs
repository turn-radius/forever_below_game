//! [`BodySpec`]: everything that makes one folk's body its own, rolled from a seed (the body
//! half of the plate prototype's `rollSpec`).

use armory::Rgb;
use procgen_rng::Mulberry32;

use crate::folk::{Beard, Folk, GREY_HAIR, HairStyle, eye_colours, hair_colours};

/// One character's body: its folk, build, face and hair.
#[derive(Clone, Debug, PartialEq)]
pub struct BodySpec {
    pub folk: Folk,
    pub seed: u32,
    /// 0 broad shoulders and narrow hips, 1 narrow shoulders, wide hips and a bust (the
    /// plate's "Shoulders · hips" slider, `C.form`).
    pub figure: f32,
    /// 0 lean, 1 heavy (the plate's "Frame").
    pub frame: f32,
    /// Height as a share of the folk's typical height, 0.95..=1.05 (`C.hs`).
    pub stature: f32,
    /// 0..=1 nudges to the jaw, the brow's tilt (and the mouth's set), the nose's size.
    pub jaw: f32,
    pub brow: f32,
    pub nose: f32,
    pub skin: Rgb,
    pub eyes: Rgb,
    pub hair: HairStyle,
    pub hair_colour: Rgb,
    pub beard: Beard,
}

impl BodySpec {
    /// A body of `folk` rolled from `seed`: the same seed always gives the same body.
    pub fn roll(folk: Folk, seed: u32) -> BodySpec {
        let race = folk.race();
        let mut rng = Mulberry32::new(seed);
        let figure = rng.next_f64() as f32;
        let frame = rng.range_f32(0.08, 0.93);
        let stature = rng.range_f32(0.95, 1.05);
        let jaw = rng.next_f64() as f32;
        let skin = rng.pick(&race.skins).0;
        let brow = rng.next_f64() as f32;
        let nose = rng.next_f64() as f32;
        let eyes = *rng.pick(eye_colours());
        let hair = *rng.pick(&race.hairs);
        let mut hair_colour = *rng.pick(hair_colours(folk));
        if !matches!(folk, Folk::Lizardfolk | Folk::Goblin) && rng.chance(0.16) {
            hair_colour = *rng.pick(&GREY_HAIR);
        }
        let beard = if rng.chance(race.beard_chance as f64) {
            *rng.pick(if folk == Folk::Dwarf { &[Beard::Long, Beard::Forked, Beard::Full][..] } else { &[Beard::Short, Beard::Full, Beard::Stubble, Beard::Long][..] })
        } else {
            Beard::None
        };
        BodySpec { folk, seed, figure, frame, stature, jaw, brow, nose, skin, eyes, hair, hair_colour, beard }
    }

    /// The folk's midpoint body: average build, typical height, the first skin and hair.
    pub fn typical(folk: Folk) -> BodySpec {
        let race = folk.race();
        BodySpec {
            folk,
            seed: 0,
            figure: 0.3,
            frame: 0.5,
            stature: 1.0,
            jaw: 0.5,
            brow: 0.5,
            nose: 0.5,
            skin: race.skins[0].0,
            eyes: eye_colours()[0],
            hair: race.hairs[0],
            hair_colour: hair_colours(folk)[0],
            beard: Beard::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolls_are_deterministic_and_follow_the_folk() {
        for folk in Folk::ALL {
            for seed in [1, 7, 1207, 99_999] {
                let a = BodySpec::roll(folk, seed);
                assert_eq!(a, BodySpec::roll(folk, seed));
                assert!(folk.race().hairs.contains(&a.hair));
                assert!((0.95..=1.05).contains(&a.stature));
                if matches!(folk, Folk::Goblin | Folk::Lizardfolk) {
                    assert_eq!(a.beard, Beard::None);
                }
            }
        }
        assert!((1..40).all(|s| BodySpec::roll(Folk::Dwarf, s).beard != Beard::None), "dwarves are always bearded");
    }
}
