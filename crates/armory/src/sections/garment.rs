//! Body armour and clothing: the garment's cut, its belt, and its plates and mail (the
//! prototype's `garment`, `belt` and `plates` sections). Ranges are the prototype's
//! `SECTIONS`.

use crate::fabric::Fabric;
use crate::material::MaterialKind;
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

/// The cut of a garment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GarmentStyle {
    Tunic,
    Robe,
    /// A padded, quilted coat.
    Gambeson,
    /// A shirt or coat of mail.
    Hauberk,
    /// Cloth over riveted plates.
    Brigandine,
    /// A coat of overlapping scales.
    Scale,
    /// A rigid breastplate and backplate.
    Cuirass,
    Cloak,
}

impl GarmentStyle {
    pub const ALL: [GarmentStyle; 8] = [
        GarmentStyle::Tunic, GarmentStyle::Robe, GarmentStyle::Gambeson, GarmentStyle::Hauberk, GarmentStyle::Brigandine,
        GarmentStyle::Scale, GarmentStyle::Cuirass, GarmentStyle::Cloak,
    ];

    /// The body is made of the garment's metal (or leather scales) rather than cloth.
    pub fn is_armour(self) -> bool {
        matches!(self, GarmentStyle::Hauberk | GarmentStyle::Scale | GarmentStyle::Cuirass)
    }

    /// Open down the front and hung from the shoulders.
    pub fn is_open(self) -> bool {
        self == GarmentStyle::Cloak
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sleeves {
    None,
    Short,
    Long,
}

impl Sleeves {
    /// Sleeve length from the shoulder, in units.
    pub fn length(self) -> f32 {
        match self {
            Sleeves::None => 0.0,
            Sleeves::Short => 2.2,
            Sleeves::Long => 5.2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Neckline {
    Round,
    /// A standing collar.
    High,
    /// A cowl round the neck with a hood hanging down the back.
    Cowl,
}

/// The garment's cut and cloth.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GarmentCut {
    pub style: GarmentStyle,
    /// How far it reaches below the waist.
    pub length: f32,
    /// Ease over the body: 1 is close fitting.
    pub fit: f32,
    /// How much the skirt widens toward the hem.
    pub flare: f32,
    /// How deep the hanging folds are.
    pub folds: f32,
    pub sleeves: Sleeves,
    /// How much the sleeves bell out toward the cuff.
    pub sleeve_flare: f32,
    /// Angle of the (implied) arms from the body, in degrees.
    pub arm_angle: f32,
    pub neck: Neckline,
    /// The cloth: main colour, pattern, pattern colour and scale.
    pub cloth: Fabric,
    /// Mail, scales or plate, for the armoured styles.
    pub material: MaterialKind,
}

sliders! {
    GarmentCutLimits for GarmentCut {
        length: 0.0, 9.0, 0.05;
        fit: 0.85, 1.3, 0.01;
        flare: 0.0, 1.0, 0.01;
        folds: 0.0, 1.0, 0.01;
        sleeve_flare: 0.0, 1.0, 0.01;
        arm_angle: 10.0, 60.0, 1.0;
    }
}

impl Default for GarmentCut {
    /// The prototype's `BASE.garment`.
    fn default() -> Self {
        GarmentCut {
            style: GarmentStyle::Tunic,
            length: 2.6,
            fit: 1.05,
            flare: 0.35,
            folds: 0.5,
            sleeves: Sleeves::Short,
            sleeve_flare: 0.15,
            arm_angle: 28.0,
            neck: Neckline::Round,
            cloth: Fabric::default(),
            material: MaterialKind::Iron,
        }
    }
}

/// A belt round the waist with a buckle at the front. Belts are their own base item too
/// (`docs/items.md`), so this section stands alone.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Belt {
    pub on: bool,
    pub width: f32,
    pub material: MaterialKind,
    pub buckle: MaterialKind,
}

sliders! {
    BeltLimits for Belt {
        width: 0.15, 0.8, 0.01;
    }
}

impl Default for Belt {
    fn default() -> Self {
        Belt { on: true, width: 0.35, material: MaterialKind::Leather, buckle: MaterialKind::Brass }
    }
}

/// Plate fittings and mail density.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plates {
    /// Hooped lames hanging below a cuirass.
    pub faulds: u32,
    /// A raised ridge down the front of a cuirass.
    pub ridge: f32,
    /// Lamed shoulder defences.
    pub pauldrons: bool,
    pub lames: u32,
    /// Rings or scales per unit, for the shader's mail and scale patterns.
    pub ring_size: f32,
}

sliders! {
    PlatesLimits for Plates {
        faulds: 0.0, 6.0, 1.0;
        ridge: 0.0, 1.0, 0.01;
        lames: 1.0, 5.0, 1.0;
        ring_size: 2.0, 14.0, 0.1;
    }
}

impl Default for Plates {
    fn default() -> Self {
        Plates { faulds: 3, ridge: 0.6, pauldrons: false, lames: 3, ring_size: 7.0 }
    }
}
