//! Base colours per material, the colour half of the prototype's `render` section
//! (`BASE.render` and `PALETTES` in `reference/spline_armory_engine.js`). "Forge new" picks
//! one palette per item; wear and decay are applied on top by the shader.

use crate::material::{MaterialKind, Rgb};
use serde::{Deserialize, Serialize};

/// One colour per paletted material. Stones carry their own colour ([`crate::gem::Gemstone`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    pub iron: Rgb,
    pub brass: Rgb,
    pub gold: Rgb,
    pub silver: Rgb,
    pub wood: Rgb,
    pub bone: Rgb,
    pub stone: Rgb,
    pub leather: Rgb,
    pub cord: Rgb,
    /// Arrow fletching.
    pub fletching: Rgb,
}

impl Palette {
    /// The prototype's `BASE.render` colours.
    pub const BASE: Palette = Palette {
        iron: Rgb::hex(0x8f9ca3),
        brass: Rgb::hex(0xb58a3c),
        gold: Rgb::hex(0xc9a13b),
        silver: Rgb::hex(0xc3c8cc),
        wood: Rgb::hex(0x7b5434),
        bone: Rgb::hex(0xe2d6bb),
        stone: Rgb::hex(0x8f928b),
        leather: Rgb::hex(0x4a3227),
        cord: Rgb::hex(0xd8ccb0),
        fletching: Rgb::hex(0xe3ddcc),
    };

    /// The prototype's `PALETTES`: iron, brass, wood and leather vary, the rest stay at base.
    pub const PRESETS: [Palette; 5] = [
        Palette::BASE.with(0x8f9ca3, 0xb58a3c, 0x7b5434, 0x4a3227),
        Palette::BASE.with(0x7d8a8f, 0xa5733a, 0x5e4a3a, 0x6b2f2a),
        Palette::BASE.with(0xa3aab0, 0xc29b48, 0x8a6a45, 0x2f3a33),
        Palette::BASE.with(0x6f7a80, 0x8d5b3a, 0x4e3b2c, 0x3d3049),
        Palette::BASE.with(0x9aa39e, 0xb79b5c, 0xa07b52, 0x5a3a22),
    ];

    const fn with(self, iron: u32, brass: u32, wood: u32, leather: u32) -> Palette {
        Palette { iron: Rgb::hex(iron), brass: Rgb::hex(brass), wood: Rgb::hex(wood), leather: Rgb::hex(leather), ..self }
    }

    /// The base colour of a material, or `None` for kinds coloured elsewhere (gems, cloth,
    /// paint, food).
    pub fn get(&self, kind: MaterialKind) -> Option<Rgb> {
        Some(match kind {
            MaterialKind::Iron => self.iron,
            MaterialKind::Brass => self.brass,
            MaterialKind::Gold => self.gold,
            MaterialKind::Silver => self.silver,
            MaterialKind::Wood => self.wood,
            MaterialKind::Bone => self.bone,
            MaterialKind::Stone => self.stone,
            MaterialKind::Leather => self.leather,
            MaterialKind::Cord => self.cord,
            MaterialKind::Gem | MaterialKind::Glass | MaterialKind::Cloth | MaterialKind::Paint | MaterialKind::Food => return None,
        })
    }
}

impl Default for Palette {
    fn default() -> Self {
        Palette::BASE
    }
}
