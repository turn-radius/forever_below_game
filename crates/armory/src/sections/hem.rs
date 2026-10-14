//! Hems and trims: a band of contrasting cloth along an edge. Shared by garments, helmets
//! (the aventail's edge), gloves and footwear (the prototype's `hem` section, whose `fam`
//! lists garment, helm, hands and feet). The neckline, cuff and front-edge toggles apply to
//! garments only.

use crate::fabric::{Fabric, FabricPattern};
use crate::material::Rgb;
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hem {
    pub on: bool,
    /// Band width.
    pub width: f32,
    pub fabric: Fabric,
    /// Along the bottom edge (or a glove's or boot's opening).
    pub hemline: bool,
    pub neck: bool,
    pub cuffs: bool,
    /// Down the front opening.
    pub front: bool,
}

sliders! {
    HemLimits for Hem {
        width: 0.1, 1.0, 0.01;
    }
}

impl Default for Hem {
    /// The prototype's `BASE.hem`.
    fn default() -> Self {
        Hem {
            on: true,
            width: 0.35,
            fabric: Fabric { color: Rgb::hex(0xc09a3a), pattern: FabricPattern::Chevrons, color2: Rgb::hex(0x7a2f22), scale: 4.0 },
            hemline: true,
            neck: true,
            cuffs: true,
            front: false,
        }
    }
}
