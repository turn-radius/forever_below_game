//! [`BodySpec`]: everything that makes one folk's body its own, rolled from a seed (the body
//! half of the plate prototype's `rollSpec`).

use armory::Rgb;
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::folk::{Beard, Folk, GREY_HAIR, HairStyle, eye_colours, hair_colours};

/// The heights the creator offers, as shares of the folk's typical height.
pub const STATURE: (f32, f32) = (0.9, 1.1);

/// One character's body: its folk, build, face and hair.
#[derive(Clone, Debug, PartialEq)]
pub struct BodySpec {
    pub folk: Folk,
    pub seed: u32,
    /// 0 broad shoulders and narrow hips, 1 narrow shoulders and wide hips (the plate's
    /// "Shoulders · hips" slider, `C.form`).
    pub figure: f32,
    /// 0 lean, 1 heavy (the plate's "Frame"): girth, belly and softness.
    pub frame: f32,
    /// Height as a share of the folk's typical height (`C.hs`): rolled within 0.95..=1.05,
    /// and [`STATURE`] lets the creator go further.
    pub stature: f32,
    /// 0 soft, 1 muscular: shoulders, chest, deltoids, arms, thighs and calves (our own).
    pub muscle: f32,
    /// 0 stocky, 1 slender: the girth of trunk and limbs at the same height (our own).
    pub slender: f32,
    /// 0 a flat chest, 1 a full bust (the plate swelled one only past figure 0.55).
    pub bust: f32,
    /// 0 high and narrow, 1 round, full and lower.
    pub bust_round: f32,
    /// 0..=1 nudges to the jaw, the brow's tilt (and the mouth's set), the nose's size.
    pub jaw: f32,
    pub brow: f32,
    pub nose: f32,
    pub skin: Rgb,
    pub eyes: Rgb,
    pub hair: HairStyle,
    pub hair_colour: Rgb,
    pub beard: Beard,
    /// How far the beard hangs, 0 none to 1 down to mid-chest (further on a dwarf): what the
    /// creator's Beard slider sets. A rolled beard takes its style's length.
    pub beard_length: f32,
}

/// The fields of a [`BodySpec`] that shape the body rather than the face and hair: what the
/// creator's sliders set ([`BodyShape::SLIDERS`]), saved with a delver.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyShape {
    pub stature: f32,
    pub frame: f32,
    pub slender: f32,
    pub muscle: f32,
    pub figure: f32,
    pub bust: f32,
    pub bust_round: f32,
    /// The beard's length ([`BodySpec::beard_length`]); delvers saved without one have none.
    #[serde(default)]
    pub beard: f32,
}

/// One of the creator's body sliders: its key (as [`BodyShape::parse`] takes it), its label and
/// its range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeSlider {
    pub key: &'static str,
    pub label: &'static str,
    pub low: f32,
    pub high: f32,
}

impl BodyShape {
    pub const SLIDERS: [ShapeSlider; 8] = [
        ShapeSlider { key: "height", label: "Height", low: STATURE.0, high: STATURE.1 },
        ShapeSlider { key: "build", label: "Build", low: 0.0, high: 1.0 },
        ShapeSlider { key: "slender", label: "Slenderness", low: 0.0, high: 1.0 },
        ShapeSlider { key: "muscle", label: "Muscle", low: 0.0, high: 1.0 },
        ShapeSlider { key: "figure", label: "Shoulders to hips", low: 0.0, high: 1.0 },
        ShapeSlider { key: "bust", label: "Bust", low: 0.0, high: 1.0 },
        ShapeSlider { key: "round", label: "Bust roundness", low: 0.0, high: 1.0 },
        ShapeSlider { key: "beard", label: "Beard", low: 0.0, high: 1.0 },
    ];

    fn field(&mut self, key: &str) -> Option<&mut f32> {
        Some(match key {
            "height" => &mut self.stature,
            "build" => &mut self.frame,
            "slender" => &mut self.slender,
            "muscle" => &mut self.muscle,
            "figure" => &mut self.figure,
            "bust" => &mut self.bust,
            "round" => &mut self.bust_round,
            "beard" => &mut self.beard,
            _ => return None,
        })
    }

    /// The value of the slider `key`.
    pub fn get(&self, key: &str) -> Option<f32> {
        let mut shape = *self;
        shape.field(key).map(|v| *v)
    }

    /// Sets the slider `key` to `value`, held within its range.
    pub fn set(&mut self, key: &str, value: f32) {
        if let (Some(slider), Some(field)) = (Self::SLIDERS.iter().find(|s| s.key == key), self.field(key)) {
            *field = value.clamp(slider.low, slider.high);
        }
    }

    /// This shape with `key=value` pairs, comma separated, set on it ("muscle=1,bust=0.4").
    pub fn parse(mut self, text: &str) -> Result<BodyShape, String> {
        for pair in text.split(',').filter(|p| !p.trim().is_empty()) {
            let (key, value) = pair.split_once('=').ok_or(format!("{pair:?} is not key=value"))?;
            let value: f32 = value.trim().parse().map_err(|_| format!("{value:?} is not a number"))?;
            let key = key.trim();
            if self.get(key).is_none() {
                let keys: Vec<_> = Self::SLIDERS.iter().map(|s| s.key).collect();
                return Err(format!("unknown body field {key:?}; one of {}", keys.join(", ")));
            }
            self.set(key, value);
        }
        Ok(self)
    }
}

impl BodySpec {
    /// The same body with a beard of `style`, at that style's length.
    pub fn with_beard(self, beard: Beard) -> BodySpec {
        BodySpec { beard, beard_length: beard_reach(beard), ..self }
    }

    /// The fields that shape the body.
    pub fn shape(&self) -> BodyShape {
        BodyShape {
            stature: self.stature,
            frame: self.frame,
            slender: self.slender,
            muscle: self.muscle,
            figure: self.figure,
            bust: self.bust,
            bust_round: self.bust_round,
            beard: self.beard_length,
        }
    }

    /// The same face and hair on a body of `shape`. The beard is cut to the shape's length: none
    /// at 0, then stubble, short, full, and long (or forked, if it was) as it grows.
    pub fn with_shape(self, shape: BodyShape) -> BodySpec {
        let BodyShape { stature, frame, slender, muscle, figure, bust, bust_round, beard } = shape;
        let style = beard_cut(self.beard, beard);
        BodySpec { stature, frame, slender, muscle, figure, bust, bust_round, beard: style, beard_length: beard, ..self }
    }

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
        // Our own fields, rolled after the plate's so those stay as they were. A bust is
        // likelier with a wider-hipped figure, as the plate's was.
        let muscle = rng.range_f32(0.1, 0.9);
        let slender = rng.range_f32(0.15, 0.85);
        let bust = (1.7 * (figure - 0.42) + rng.range_f32(-0.2, 0.2)).clamp(0.0, 1.0);
        let bust_round = rng.next_f64() as f32;
        let beard_length = beard_reach(beard);
        BodySpec { folk, seed, figure, frame, stature, muscle, slender, bust, bust_round, jaw, brow, nose, skin, eyes, hair, hair_colour, beard, beard_length }
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
            muscle: 0.5,
            slender: 0.5,
            bust: 0.0,
            bust_round: 0.5,
            jaw: 0.5,
            brow: 0.5,
            nose: 0.5,
            skin: race.skins[0].0,
            eyes: eye_colours()[0],
            hair: race.hairs[0],
            hair_colour: hair_colours(folk)[0],
            beard: Beard::None,
            beard_length: 0.0,
        }
    }
}

/// The length a rolled beard of `style` hangs to ([`BodySpec::beard_length`]).
pub fn beard_reach(style: Beard) -> f32 {
    match style {
        Beard::None => 0.0,
        Beard::Stubble => 0.05,
        Beard::Short => 0.2,
        Beard::Full => 0.45,
        Beard::Long => 0.9,
        Beard::Forked => 0.85,
    }
}

/// The style a beard of `length` is cut to, from the style it had: a long beard stays forked if
/// it was forked.
pub fn beard_cut(was: Beard, length: f32) -> Beard {
    match length {
        l if l <= 0.0 => Beard::None,
        l if l < 0.1 => Beard::Stubble,
        l if l < 0.3 => Beard::Short,
        l if l < 0.6 => Beard::Full,
        _ if was == Beard::Forked => Beard::Forked,
        _ => Beard::Long,
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
                for v in [a.muscle, a.slender, a.bust, a.bust_round] {
                    assert!((0.0..=1.0).contains(&v));
                }
                if matches!(folk, Folk::Goblin | Folk::Lizardfolk) {
                    assert_eq!(a.beard, Beard::None);
                }
            }
        }
        assert!((1..40).all(|s| BodySpec::roll(Folk::Dwarf, s).beard != Beard::None), "dwarves are always bearded");
    }

    #[test]
    fn a_shape_sets_only_the_body() {
        let spec = BodySpec::roll(Folk::Gnome, 9);
        let shape = spec.shape().parse("muscle=1, bust=0.25,height=2").unwrap();
        assert_eq!((shape.muscle, shape.bust, shape.stature), (1.0, 0.25, STATURE.1), "held within range");
        let shaped = spec.clone().with_shape(shape);
        assert_eq!(shaped.shape(), shape);
        assert_eq!((shaped.jaw, shaped.hair, shaped.skin), (spec.jaw, spec.hair, spec.skin));
        assert_eq!(spec.clone().with_shape(spec.shape()), spec);
        for beard in Beard::ALL {
            let spec = BodySpec { beard, beard_length: beard_reach(beard), ..BodySpec::typical(Folk::Human) };
            assert_eq!(spec.clone().with_shape(spec.shape()).beard, beard, "a rolled beard keeps its cut");
        }
        let shaved = spec.clone().with_shape(spec.shape().parse("beard=0").unwrap());
        assert_eq!(shaved.beard, Beard::None);
        assert_eq!(spec.clone().with_shape(spec.shape().parse("beard=1").unwrap()).beard, Beard::Long);
        assert!(spec.shape().parse("wings=1").is_err() && spec.shape().parse("muscle").is_err());
        for slider in BodyShape::SLIDERS {
            assert!(spec.shape().get(slider.key).is_some(), "{}", slider.key);
        }
    }
}
