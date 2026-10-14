//! Sliders: the prototype's `R(key, label, min, max, step)` ranges, typed.
//!
//! Each section struct (a shaft, a head, a condition...) gets a companion `...Limits` struct
//! from the [`sliders!`] macro: one [`Slider`] per ranged field, `SLIDERS` holding the full
//! ranges from `SECTIONS` in `reference/spline_armory_engine.js`, and an implementation of
//! [`Sliders`] to nudge, fit and check every ranged field. Families narrow the ranges the
//! way the prototype's per-family `RANGES` table does (see `families::mace::LIMITS`).

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

/// One ranged parameter: inclusive bounds and the slider step.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slider {
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

impl Slider {
    pub const fn new(min: f32, max: f32, step: f32) -> Self {
        Self { min, max, step }
    }

    /// Same step, a narrower range (the prototype's per-family `RANGES`).
    pub const fn narrowed(self, min: f32, max: f32) -> Self {
        Self { min, max, step: self.step }
    }

    pub fn clamp(&self, v: f32) -> f32 {
        v.clamp(self.min, self.max)
    }

    /// Clamp, then round to the nearest step counted from `min`.
    pub fn snap(&self, v: f32) -> f32 {
        let v = self.clamp(v);
        self.clamp(self.min + ((v - self.min) / self.step).round() * self.step)
    }

    pub fn contains(&self, v: f32) -> bool {
        v >= self.min && v <= self.max
    }

    /// Uniform over the whole range.
    pub fn uniform(&self, rng: &mut Mulberry32) -> f32 {
        rng.range_f32(self.min, self.max)
    }

    /// The prototype's "Forge new" nudge: move `v` by up to `spread / 2` of the range either
    /// way, clamped. One draw.
    pub fn nudge(&self, rng: &mut Mulberry32, v: f32, spread: f32) -> f32 {
        self.clamp(v + (rng.next_f64() as f32 - 0.5) * (self.max - self.min) * spread)
    }
}

/// A field a [`Slider`] can drive: `f32` amounts and `u32` counts.
pub trait SliderValue: Copy {
    fn to_f32(self) -> f32;
    fn from_f32(v: f32) -> Self;
}

impl SliderValue for f32 {
    fn to_f32(self) -> f32 {
        self
    }
    fn from_f32(v: f32) -> Self {
        v
    }
}

impl SliderValue for u32 {
    fn to_f32(self) -> f32 {
        self as f32
    }
    fn from_f32(v: f32) -> Self {
        v.round().max(0.0) as u32
    }
}

/// Every ranged field of a section, driven together.
pub trait Sliders {
    type Limits;
    /// Each ranged field, with probability `odds`, gets [`Slider::nudge`]d by `spread`.
    fn nudge(&mut self, rng: &mut Mulberry32, limits: &Self::Limits, odds: f64, spread: f32);
    /// Clamp and snap every ranged field into `limits` (the prototype's `proportion()`).
    fn fit(&mut self, limits: &Self::Limits);
    /// The names of ranged fields outside `limits`.
    fn out_of_range(&self, limits: &Self::Limits) -> Vec<&'static str>;
}

/// Declares `$limits` (one [`Slider`] per listed field, `SLIDERS` = the prototype's ranges)
/// and implements [`Sliders`] for `$state` over those fields.
macro_rules! sliders {
    ($(#[$meta:meta])* $limits:ident for $state:ty { $($field:ident: $min:expr, $max:expr, $step:expr;)* }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
        pub struct $limits {
            $(pub $field: $crate::slider::Slider,)*
        }

        impl $limits {
            /// The full slider ranges from the prototype's `SECTIONS`.
            pub const SLIDERS: Self = Self { $($field: $crate::slider::Slider::new($min, $max, $step),)* };
        }

        impl Default for $limits {
            fn default() -> Self {
                Self::SLIDERS
            }
        }

        impl $crate::slider::Sliders for $state {
            type Limits = $limits;

            fn nudge(&mut self, rng: &mut procgen_rng::Mulberry32, limits: &$limits, odds: f64, spread: f32) {
                use $crate::slider::SliderValue;
                $(
                    if rng.chance(odds) {
                        self.$field = SliderValue::from_f32(limits.$field.nudge(rng, self.$field.to_f32(), spread));
                    }
                )*
            }

            fn fit(&mut self, limits: &$limits) {
                use $crate::slider::SliderValue;
                $( self.$field = SliderValue::from_f32(limits.$field.snap(self.$field.to_f32())); )*
            }

            fn out_of_range(&self, limits: &$limits) -> Vec<&'static str> {
                use $crate::slider::SliderValue;
                let mut out = Vec::new();
                $( if !limits.$field.contains(self.$field.to_f32()) { out.push(stringify!($field)); } )*
                out
            }
        }
    };
}

pub(crate) use sliders;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_stays_inside_and_on_step() {
        let s = Slider::new(0.015, 0.25, 0.005);
        assert_eq!(s.snap(1.0), 0.25);
        assert_eq!(s.snap(-1.0), 0.015);
        assert!((s.snap(0.0712) - 0.07).abs() < 1e-6);
    }
}
