//! Hoods, cowls and coifs: soft head coverings of cloth or mail. The Armory prototype has no
//! section for them (its plate draws a `Hood` and a `Mail coif` as offsets of the head);
//! ranges here are this port's own.

use crate::fabric::Fabric;
use crate::material::MaterialKind;
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoodStyle {
    /// A cloth hood with a shoulder cape, its crown drawn to a point or a long tail.
    Hood,
    /// A loose, deep hood whose neck falls in folds over the chest.
    Cowl,
    /// A close cap over the crown and ears, tied under the chin.
    Coif,
    /// A hood of mail with a short cape (camail), sometimes with a flap across the chin.
    MailCoif,
}

impl HoodStyle {
    pub const ALL: [HoodStyle; 4] = [HoodStyle::Hood, HoodStyle::Cowl, HoodStyle::Coif, HoodStyle::MailCoif];

    /// Made of mail rather than cloth.
    pub fn is_mail(self) -> bool {
        self == HoodStyle::MailCoif
    }

    /// Covers the neck and falls onto the shoulders.
    pub fn has_cape(self) -> bool {
        self != HoodStyle::Coif
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hood {
    pub style: HoodStyle,
    /// How loose it sits off the head, 0 (close) to 1 (loose).
    pub ease: f32,
    /// How far the back of the crown is drawn out into a point.
    pub peak: f32,
    /// Length of the tail (liripipe) hanging from the point.
    pub liripipe: f32,
    /// How far the cape reaches below the shoulders.
    pub cape: f32,
    /// How wide the face opening is, 0 (a slit round the eyes) to 1 (the whole face).
    pub opening: f32,
    /// How deep the folds are.
    pub folds: f32,
    /// Scallops cut along the cape's edge (0 for a plain edge).
    pub dags: u32,
    /// Cords tied under the chin.
    pub ties: bool,
    /// A flap of mail across the chin and mouth, strapped at the temple.
    pub ventail: bool,
    pub cloth: Fabric,
    /// Mail for a mail coif.
    pub material: MaterialKind,
    /// Rings per unit, for the shader's mail pattern.
    pub ring_size: f32,
}

sliders! {
    HoodLimits for Hood {
        ease: 0.0, 1.0, 0.01;
        peak: 0.0, 1.0, 0.01;
        liripipe: 0.0, 4.0, 0.05;
        cape: 0.0, 3.0, 0.05;
        opening: 0.0, 1.0, 0.01;
        folds: 0.0, 1.0, 0.01;
        dags: 0.0, 16.0, 1.0;
        ring_size: 2.0, 14.0, 0.1;
    }
}

impl Default for Hood {
    /// A plain wool hood with a short cape.
    fn default() -> Self {
        Hood {
            style: HoodStyle::Hood,
            ease: 0.4,
            peak: 0.4,
            liripipe: 0.0,
            cape: 0.8,
            opening: 0.6,
            folds: 0.5,
            dags: 0,
            ties: false,
            ventail: false,
            cloth: Fabric::default(),
            material: MaterialKind::Iron,
            ring_size: 7.0,
        }
    }
}
