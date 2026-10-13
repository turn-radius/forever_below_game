//! Footwear and greaves (the prototype's `boot` section). Ranges are the prototype's
//! `SECTIONS`.

use crate::material::{MaterialKind, Rgb};
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootStyle {
    /// A low, soft turnshoe.
    Shoe,
    Boot,
    /// A low shoe with a long pointed toe.
    Poulaine,
    /// Cloth leggings bound with a spiral wrap.
    Wrapped,
    /// Lamed plate over the foot, with a greave up the shin.
    Sabaton,
}

impl BootStyle {
    pub const ALL: [BootStyle; 5] = [BootStyle::Shoe, BootStyle::Boot, BootStyle::Poulaine, BootStyle::Wrapped, BootStyle::Sabaton];
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Boot {
    pub style: BootStyle,
    /// Scale of the foot inside.
    pub size: f32,
    pub width: f32,
    /// Height of the shaft above the foot.
    pub height: f32,
    /// How long and pointed the toe is.
    pub toe: f32,
    /// Depth of a turned-down cuff at the top of the shaft.
    pub cuff: f32,
    pub straps: u32,
    /// Plates over the foot of a sabaton.
    pub lames: u32,
    /// A knee cop over the top of a greave.
    pub knee: bool,
    pub material: MaterialKind,
    pub sole: MaterialKind,
    pub plate: MaterialKind,
    /// The leg cloth of wrapped legs.
    pub leg_cloth: Rgb,
    /// The binding wound round wrapped legs.
    pub wrap_cloth: Rgb,
}

sliders! {
    BootLimits for Boot {
        size: 0.85, 1.2, 0.01;
        width: 0.8, 1.3, 0.01;
        height: 0.0, 4.8, 0.05;
        toe: 0.0, 1.0, 0.01;
        cuff: 0.0, 0.8, 0.01;
        straps: 0.0, 4.0, 1.0;
        lames: 2.0, 8.0, 1.0;
    }
}

impl Default for Boot {
    /// The prototype's `BASE.boot`.
    fn default() -> Self {
        Boot {
            style: BootStyle::Boot,
            size: 1.0,
            width: 1.0,
            height: 3.6,
            toe: 0.0,
            cuff: 0.35,
            straps: 1,
            lames: 5,
            knee: true,
            material: MaterialKind::Leather,
            sole: MaterialKind::Leather,
            plate: MaterialKind::Iron,
            leg_cloth: Rgb::hex(0x6b5a44),
            wrap_cloth: Rgb::hex(0xcbbf9f),
        }
    }
}

impl Boot {
    /// The prototype's leg cloth colours.
    pub const LEG_CLOTHS: [Rgb; 4] = [Rgb::hex(0x7a6a50), Rgb::hex(0x5a4a3a), Rgb::hex(0x6b5a44), Rgb::hex(0x4a5a3a)];
    /// The prototype's binding colours.
    pub const WRAP_CLOTHS: [Rgb; 3] = [Rgb::hex(0xcbbf9f), Rgb::hex(0xa89878), Rgb::hex(0x8a7a60)];
}
