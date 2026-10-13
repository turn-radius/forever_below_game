//! What an item part is made of. The game maps each kind to a toon material and a decay
//! type (`docs/rendering.md`, "Wear, rust and grime").

use serde::{Deserialize, Serialize};

/// The prototype's `MATS` list, plus the kinds other families need: cut stones (which carry
/// their own colour), cloth, paint over wood, and food.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaterialKind {
    #[serde(rename = "metal")]
    Iron,
    #[serde(rename = "accent")]
    Brass,
    Gold,
    Silver,
    Wood,
    Bone,
    Stone,
    Leather,
    #[serde(rename = "string")]
    Cord,
    Gem,
    Cloth,
    /// Paint over wood (shield faces, chests).
    Paint,
    Food,
}

impl MaterialKind {
    /// The selectable materials for a part, in the prototype's order.
    pub const SELECTABLE: [MaterialKind; 9] = [
        MaterialKind::Iron, MaterialKind::Brass, MaterialKind::Gold, MaterialKind::Silver, MaterialKind::Wood,
        MaterialKind::Bone, MaterialKind::Stone, MaterialKind::Leather, MaterialKind::Cord,
    ];

    /// The word an item name uses for this material ("Gold Band", "Wooden Idol"; the
    /// prototype's `MATN`, which calls brass bronze).
    pub fn adjective(self) -> &'static str {
        match self {
            MaterialKind::Iron => "Iron",
            MaterialKind::Brass => "Bronze",
            MaterialKind::Gold => "Gold",
            MaterialKind::Silver => "Silver",
            MaterialKind::Wood => "Wooden",
            MaterialKind::Bone => "Bone",
            MaterialKind::Stone => "Stone",
            MaterialKind::Leather => "Leather",
            MaterialKind::Cord => "Cord",
            MaterialKind::Gem => "Jewelled",
            MaterialKind::Cloth => "Cloth",
            MaterialKind::Paint => "Painted",
            MaterialKind::Food => "",
        }
    }

    /// Decay type id from the table in `docs/rendering.md`.
    pub fn decay_type(self) -> u32 {
        match self {
            MaterialKind::Iron => 1,
            MaterialKind::Brass => 2,
            MaterialKind::Wood => 3,
            MaterialKind::Leather | MaterialKind::Cord => 4,
            MaterialKind::Gem => 5,
            MaterialKind::Gold => 6,
            MaterialKind::Silver => 7,
            MaterialKind::Bone => 8,
            MaterialKind::Stone => 9,
            MaterialKind::Cloth => 10,
            MaterialKind::Paint => 11,
            MaterialKind::Food => 12,
        }
    }
}

/// An sRGB colour, 0..=1 per channel.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rgb(pub f32, pub f32, pub f32);

impl Rgb {
    pub const fn hex(v: u32) -> Self {
        Rgb(((v >> 16) & 0xff) as f32 / 255.0, ((v >> 8) & 0xff) as f32 / 255.0, (v & 0xff) as f32 / 255.0)
    }

    /// Every channel times `factor`, clamped to 0..=1 (a darker strap cut from the same hide).
    pub fn scaled(self, factor: f32) -> Self {
        let f = |c: f32| (c * factor).clamp(0.0, 1.0);
        Rgb(f(self.0), f(self.1), f(self.2))
    }
}
