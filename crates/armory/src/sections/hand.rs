//! Gloves, gauntlets, mittens and bracers (the prototype's `hand` section). Ranges are the
//! prototype's `SECTIONS`.

use crate::material::MaterialKind;
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandStyle {
    /// A leather glove.
    Glove,
    /// A fingered gauntlet: a glove with plates over each finger and the back of the hand.
    Gauntlet,
    /// A mitten gauntlet with lames over the fingers and a cuff waisted like an hourglass.
    Hourglass,
    /// A mail mitten.
    Mitten,
    /// An archer's bracer: the forearm only.
    Bracer,
}

impl HandStyle {
    pub const ALL: [HandStyle; 5] = [HandStyle::Glove, HandStyle::Gauntlet, HandStyle::Hourglass, HandStyle::Mitten, HandStyle::Bracer];

    /// The fingers are covered together rather than one by one.
    pub fn is_mitten(self) -> bool {
        matches!(self, HandStyle::Hourglass | HandStyle::Mitten)
    }

    /// Has plates over the hand.
    pub fn is_plated(self) -> bool {
        matches!(self, HandStyle::Gauntlet | HandStyle::Hourglass)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hand {
    pub style: HandStyle,
    /// Scale of the hand inside.
    pub size: f32,
    /// How far the fingers curl toward the palm.
    pub curl: f32,
    /// How far the fingers splay apart.
    pub spread: f32,
    /// Length of the cuff up the forearm.
    pub cuff: f32,
    /// How much the cuff bells open toward the elbow.
    pub flare: f32,
    /// Plates over the back of the hand.
    pub lames: u32,
    /// Spikes on the knuckles.
    pub gadlings: bool,
    /// The glove itself.
    pub material: MaterialKind,
    /// Plates or mail.
    pub plate: MaterialKind,
}

sliders! {
    HandLimits for Hand {
        size: 0.85, 1.2, 0.01;
        curl: 0.0, 1.0, 0.01;
        spread: 0.0, 1.0, 0.01;
        cuff: 0.3, 3.0, 0.01;
        flare: 0.0, 1.0, 0.01;
        lames: 2.0, 6.0, 1.0;
    }
}

impl Default for Hand {
    /// The prototype's `BASE.hand`.
    fn default() -> Self {
        Hand {
            style: HandStyle::Gauntlet,
            size: 1.0,
            curl: 0.25,
            spread: 0.3,
            cuff: 1.2,
            flare: 0.5,
            lames: 4,
            gadlings: false,
            material: MaterialKind::Leather,
            plate: MaterialKind::Iron,
        }
    }
}
