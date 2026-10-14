//! Soft goods and the surface patterns the shader draws over a part: the textiles bags and
//! sacks are cut from, dyed cloth, woven and printed fabric patterns, quilting, rivets, mail
//! rings, scales and heraldic divisions. Shared by garments, hems and trims, leg cloths,
//! hoods, bags, a chest's glaze, shields and anything else that carries a pattern.
//!
//! From the prototype's `FABRIC` list, the bag's `fabric` select, `PAT_ID` and `clothMat` in
//! `reference/spline_armory_engine.js`. The pattern is state, not geometry: a part carries a
//! [`SurfacePattern`] and the toon shader draws it (`docs/rendering.md`).

use crate::material::{MaterialKind, Rgb};
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

/// The stuff a bag, sack or pouch is cut from (the prototype's bag `fabric` select).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Textile {
    Leather,
    /// Woven wool or linen.
    Cloth,
    /// Coarse jute or hemp sacking.
    Burlap,
}

impl Textile {
    pub const ALL: [Textile; 3] = [Textile::Leather, Textile::Cloth, Textile::Burlap];

    /// The material a part cut from it renders and decays as.
    pub fn material(self) -> MaterialKind {
        match self {
            Textile::Leather => MaterialKind::Leather,
            Textile::Cloth | Textile::Burlap => MaterialKind::Cloth,
        }
    }

    /// How thick a cut panel is, in units (1 unit = 10 cm).
    pub fn thickness(self) -> f32 {
        match self {
            Textile::Leather => 0.035,
            Textile::Cloth => 0.02,
            Textile::Burlap => 0.03,
        }
    }
}

/// A woven, printed, painted or glazed pattern (the prototype's `FABRIC` select).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FabricPattern {
    Plain,
    Stripes,
    Hoops,
    Checks,
    Tartan,
    Lozenges,
    /// Small dots scattered over the cloth ("powdered").
    Powdered,
    Chevrons,
    Quilted,
    Rivets,
}

impl FabricPattern {
    pub const ALL: [FabricPattern; 10] = [
        FabricPattern::Plain, FabricPattern::Stripes, FabricPattern::Hoops, FabricPattern::Checks, FabricPattern::Tartan,
        FabricPattern::Lozenges, FabricPattern::Powdered, FabricPattern::Chevrons, FabricPattern::Quilted, FabricPattern::Rivets,
    ];

    /// What the shader draws for this pattern, or `None` for plain cloth.
    pub fn kind(self) -> Option<PatternKind> {
        Some(match self {
            FabricPattern::Plain => return None,
            FabricPattern::Stripes => PatternKind::Stripes,
            FabricPattern::Hoops => PatternKind::Hoops,
            FabricPattern::Checks => PatternKind::Checks,
            FabricPattern::Tartan => PatternKind::Tartan,
            FabricPattern::Lozenges => PatternKind::Lozenges,
            FabricPattern::Powdered => PatternKind::Powdered,
            FabricPattern::Chevrons => PatternKind::Chevrons,
            FabricPattern::Quilted => PatternKind::Quilted,
            FabricPattern::Rivets => PatternKind::Rivets,
        })
    }
}

/// Every pattern the surface shader draws. Ids follow the prototype's `PAT_ID`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternKind {
    Stripes,
    Hoops,
    Checks,
    Tartan,
    Lozenges,
    Powdered,
    Chevrons,
    Quilted,
    /// Butted or riveted mail rings.
    Mail,
    /// Overlapping scales.
    Scales,
    Rivets,
    /// Heraldic divisions of a painted field, between the part's tint and the pattern colour,
    /// over uv 0..=1 across the field ([`crate::heraldry::Division`]).
    PerPale,
    PerFess,
    Quarterly,
    PerBend,
    PerChevron,
    Paly,
    Bendy,
    Checky,
    PerSaltire,
    Barry,
    /// Boards running up the part, `scale` of them across uv 0..=1 (bare shield faces).
    Planks,
}

impl PatternKind {
    /// The shader's pattern id (`PAT_ID` in the prototype).
    pub fn shader_id(self) -> u32 {
        match self {
            PatternKind::Stripes => 1,
            PatternKind::Hoops => 2,
            PatternKind::Checks => 3,
            PatternKind::Tartan => 4,
            PatternKind::Lozenges => 5,
            PatternKind::Powdered => 6,
            PatternKind::Chevrons => 7,
            PatternKind::Quilted => 8,
            PatternKind::Mail => 9,
            PatternKind::Scales => 10,
            PatternKind::Rivets => 11,
            PatternKind::PerPale => 12,
            PatternKind::PerFess => 13,
            PatternKind::Quarterly => 14,
            PatternKind::PerBend => 15,
            PatternKind::PerChevron => 16,
            PatternKind::Paly => 17,
            PatternKind::Bendy => 18,
            PatternKind::Checky => 19,
            PatternKind::Planks => 21,
            // Not in the prototype's table.
            PatternKind::PerSaltire => 22,
            PatternKind::Barry => 23,
        }
    }
}

/// A pattern as a part carries it: what to draw, in which second colour, at what scale.
/// `color` is ignored by metal patterns (mail, scales), which shade the part's own colour.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfacePattern {
    pub kind: PatternKind,
    pub color: Rgb,
    /// Repeats per unit of surface (the prototype's `patScale` or `ringSize`).
    pub scale: f32,
}

/// One dyed cloth: the prototype's colour, pattern, pattern colour and pattern scale.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fabric {
    pub color: Rgb,
    pub pattern: FabricPattern,
    pub color2: Rgb,
    /// Pattern repeats per unit.
    pub scale: f32,
}

sliders! {
    FabricLimits for Fabric {
        scale: 0.5, 8.0, 0.05;
    }
}

impl Fabric {
    /// Plain cloth of one colour.
    pub const fn plain(color: Rgb) -> Self {
        Fabric { color, pattern: FabricPattern::Plain, color2: color, scale: 2.0 }
    }

    /// The shader pattern for this cloth, if it has one.
    pub fn surface(&self) -> Option<SurfacePattern> {
        self.pattern.kind().map(|kind| SurfacePattern { kind, color: self.color2, scale: self.scale })
    }

    /// The prototype's garment colours (madder, woad, weld and walnut dyes, undyed wool).
    pub const GARMENT_DYES: [Rgb; 8] = [
        Rgb::hex(0x6a7f3a), Rgb::hex(0x2c3e6b), Rgb::hex(0x7a1f24), Rgb::hex(0x4a3a2e),
        Rgb::hex(0xcbbf9f), Rgb::hex(0x5a3d6b), Rgb::hex(0x3f5a5a), Rgb::hex(0x8a5a2b),
    ];
    /// The prototype's pattern colours.
    pub const PATTERN_DYES: [Rgb; 5] = [Rgb::hex(0xe0d4b0), Rgb::hex(0xd4b850), Rgb::hex(0x232323), Rgb::hex(0xa3262a), Rgb::hex(0x2c4e8c)];
    /// The prototype's trim colours.
    pub const TRIM_DYES: [Rgb; 5] = [Rgb::hex(0xc09a3a), Rgb::hex(0xd4b850), Rgb::hex(0x7a2f22), Rgb::hex(0x232323), Rgb::hex(0xe0d4b0)];
}

impl Default for Fabric {
    /// The prototype's `BASE.garment` cloth.
    fn default() -> Self {
        Fabric { color: Rgb::hex(0x6a7f3a), pattern: FabricPattern::Plain, color2: Rgb::hex(0xe0d4b0), scale: 2.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_cloth_has_no_surface_pattern() {
        assert_eq!(Fabric::plain(Rgb::hex(0x123456)).surface(), None);
        let striped = Fabric { pattern: FabricPattern::Stripes, ..Fabric::default() };
        assert_eq!(striped.surface().unwrap().kind.shader_id(), 1);
    }
}
