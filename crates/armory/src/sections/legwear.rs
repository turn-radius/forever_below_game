//! Trousers, breeches and leggings: the cut of the legs, the waistband and its closure,
//! turned cuffs, puttees and knee patches. The prototype has no Armory section for legwear
//! (its plate draws trousers as an offset of the body); ranges here are this port's own.

use crate::fabric::Fabric;
use crate::material::{MaterialKind, Rgb};
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

/// The cut of a pair of legs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegwearStyle {
    /// Full length to the ankle, straight or tapered.
    Trousers,
    /// Full in the thigh, gathered into a band just below the knee.
    Breeches,
    /// Close-fitting knit or cloth, ankle length.
    Leggings,
}

impl LegwearStyle {
    pub const ALL: [LegwearStyle; 3] = [LegwearStyle::Trousers, LegwearStyle::Breeches, LegwearStyle::Leggings];
}

/// How the waistband is fastened at the front.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaistClosure {
    /// A cord threaded through the band, tied at the front.
    Drawstring,
    /// A short laced opening down the front.
    Laces,
    /// Wooden or bone toggles through cord loops.
    Toggles,
}

impl WaistClosure {
    pub const ALL: [WaistClosure; 3] = [WaistClosure::Drawstring, WaistClosure::Laces, WaistClosure::Toggles];
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Legwear {
    pub style: LegwearStyle,
    /// How loose the legs hang off the body, 0 (close) to 1 (baggy).
    pub ease: f32,
    /// How much the leg narrows toward the hem, 0 (wide, straight) to 1 (tapered close).
    pub taper: f32,
    /// How deep the folds and creases are.
    pub folds: f32,
    /// Depth of the waistband.
    pub band: f32,
    pub closure: WaistClosure,
    /// Depth of a turned-up cuff at the hem (0 for none).
    pub cuff: f32,
    /// Strips of cloth wound from the ankle up the shin.
    pub puttees: bool,
    /// Leather patches sewn over the knees.
    pub patches: bool,
    pub cloth: Fabric,
    /// The puttees' cloth.
    pub wrap_cloth: Rgb,
    /// Toggles, knee-band buckles and lace tags.
    pub fittings: MaterialKind,
}

sliders! {
    LegwearLimits for Legwear {
        ease: 0.0, 1.0, 0.01;
        taper: 0.0, 1.0, 0.01;
        folds: 0.0, 1.0, 0.01;
        band: 0.15, 0.6, 0.01;
        cuff: 0.0, 0.6, 0.01;
    }
}

impl Default for Legwear {
    /// Plain wool trousers with a drawstring.
    fn default() -> Self {
        Legwear {
            style: LegwearStyle::Trousers,
            ease: 0.4,
            taper: 0.4,
            folds: 0.5,
            band: 0.3,
            closure: WaistClosure::Drawstring,
            cuff: 0.0,
            puttees: false,
            patches: false,
            cloth: Fabric::plain(Rgb::hex(0x5a4a3a)),
            wrap_cloth: Rgb::hex(0xcbbf9f),
            fittings: MaterialKind::Wood,
        }
    }
}

impl Legwear {
    /// Undyed and earth-dyed wool, hemp and canvas.
    pub const DYES: [Rgb; 8] = [
        Rgb::hex(0x5a4a3a), Rgb::hex(0x6b5a44), Rgb::hex(0x4a3a2e), Rgb::hex(0x7a6a50),
        Rgb::hex(0x3f4a3a), Rgb::hex(0x2c3e5b), Rgb::hex(0x6a2a24), Rgb::hex(0xa89878),
    ];
    /// Puttee colours.
    pub const WRAP_CLOTHS: [Rgb; 4] = [Rgb::hex(0xcbbf9f), Rgb::hex(0xa89878), Rgb::hex(0x8a7a60), Rgb::hex(0x5a4a3a)];
}
