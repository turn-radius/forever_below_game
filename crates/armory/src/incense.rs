//! Incense: what a censer burns, and what a heap of it looks like. Three kinds. Each is only a
//! name, a colour and a smoke here; what each one protects against belongs to the game
//! (`fb_items`), which reads the kind off the censer ([`crate::families::lantern::LanternFuel`]).

use serde::{Deserialize, Serialize};

use crate::material::Rgb;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IncenseKind {
    /// Shaved cedar wood, a dry green smoke.
    #[default]
    Cedar,
    /// Dark resin tears, a heavy bitter smoke.
    Myrrh,
    /// Pale gold tears of resin, a thin white smoke.
    Frankincense,
}

impl IncenseKind {
    pub const ALL: [IncenseKind; 3] = [IncenseKind::Cedar, IncenseKind::Myrrh, IncenseKind::Frankincense];

    pub fn name(self) -> &'static str {
        match self {
            IncenseKind::Cedar => "Cedar",
            IncenseKind::Myrrh => "Myrrh",
            IncenseKind::Frankincense => "Frankincense",
        }
    }

    /// The colour of the incense unburnt: shavings or tears of resin.
    pub fn color(self) -> Rgb {
        match self {
            IncenseKind::Cedar => Rgb::hex(0xa4693a),
            IncenseKind::Myrrh => Rgb::hex(0x6a3622),
            IncenseKind::Frankincense => Rgb::hex(0xe3cf86),
        }
    }

    /// The colour of its smoke (sRGB).
    pub fn smoke(self) -> Rgb {
        match self {
            IncenseKind::Cedar => Rgb::hex(0xb7c0b4),
            IncenseKind::Myrrh => Rgb::hex(0xa59a92),
            IncenseKind::Frankincense => Rgb::hex(0xeeece6),
        }
    }
}
