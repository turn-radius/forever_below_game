//! Heraldry: tinctures, divisions of the field, and devices (charges). Shared by anything
//! painted, embroidered or engraved with arms: shield faces now, and later surcoats, banners,
//! chest lids, signet rings and medallions.
//!
//! From the prototype's `field` section, its tincture list in the shield branch of
//! `randomize()`, and `DEVICES` / `devicePts` (`reference/spline_armory_engine.js`). The
//! names are the heraldic ones; the colours are the prototype's.
//!
//! Divisions are drawn by the shader: [`Field::surface`] turns one into a
//! [`SurfacePattern`] in [`Field::tincture2`] over a painted part tinted [`Field::tincture`],
//! whose uv runs 0..=1 across the field. Charges are the shared [`Device`]s, whose outlines
//! are in [`crate::build::device`].

use crate::fabric::{PatternKind, SurfacePattern};
use crate::material::Rgb;
use crate::sections::device::Device;
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

/// A heraldic colour. Or and Argent are the metals; the rest are colours. The rule of
/// tincture puts metal on colour or colour on metal, never like on like.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tincture {
    /// Gold or yellow.
    Or,
    /// Silver or white.
    Argent,
    /// Red.
    Gules,
    /// Blue.
    Azure,
    /// Green.
    Vert,
    /// Purple.
    Purpure,
    /// Black.
    Sable,
}

impl Tincture {
    pub const ALL: [Tincture; 7] =
        [Tincture::Or, Tincture::Argent, Tincture::Gules, Tincture::Azure, Tincture::Vert, Tincture::Purpure, Tincture::Sable];
    pub const METALS: [Tincture; 2] = [Tincture::Or, Tincture::Argent];
    pub const COLOURS: [Tincture; 5] = [Tincture::Gules, Tincture::Azure, Tincture::Vert, Tincture::Purpure, Tincture::Sable];

    pub fn is_metal(self) -> bool {
        matches!(self, Tincture::Or | Tincture::Argent)
    }

    /// The prototype's paint colours.
    pub fn rgb(self) -> Rgb {
        Rgb::hex(match self {
            Tincture::Or => 0xd4a838,
            Tincture::Argent => 0xe8e6df,
            Tincture::Gules => 0xa3262a,
            Tincture::Azure => 0x2c4e8c,
            Tincture::Vert => 0x2f6b3a,
            Tincture::Purpure => 0x6a3d7a,
            Tincture::Sable => 0x232323,
        })
    }
}

/// How the field is split between its two tinctures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Division {
    Plain,
    /// Split down the middle.
    PerPale,
    /// Split across the middle.
    PerFess,
    Quarterly,
    /// Split corner to corner, dexter chief to sinister base.
    PerBend,
    /// Split by an inverted V.
    PerChevron,
    /// Split by both diagonals.
    PerSaltire,
    /// Vertical stripes.
    Paly,
    /// Horizontal stripes.
    Barry,
    /// Diagonal stripes.
    Bendy,
    /// A chequerboard.
    Checky,
    /// Diamonds.
    Lozengy,
}

impl Division {
    /// What the shader draws for this division, or `None` for a plain field.
    pub fn kind(self) -> Option<PatternKind> {
        Some(match self {
            Division::Plain => return None,
            Division::PerPale => PatternKind::PerPale,
            Division::PerFess => PatternKind::PerFess,
            Division::Quarterly => PatternKind::Quarterly,
            Division::PerBend => PatternKind::PerBend,
            Division::PerChevron => PatternKind::PerChevron,
            Division::PerSaltire => PatternKind::PerSaltire,
            Division::Paly => PatternKind::Paly,
            Division::Barry => PatternKind::Barry,
            Division::Bendy => PatternKind::Bendy,
            Division::Checky => PatternKind::Checky,
            Division::Lozengy => PatternKind::Lozenges,
        })
    }

    /// Repeats across the field: one for a party field, several for stripes and chequers.
    pub fn repeats(self) -> f32 {
        match self {
            Division::Paly | Division::Barry | Division::Bendy => 6.0,
            Division::Checky | Division::Lozengy => 5.0,
            _ => 1.0,
        }
    }

    pub const ALL: [Division; 12] = [
        Division::Plain, Division::PerPale, Division::PerFess, Division::Quarterly, Division::PerBend, Division::PerChevron,
        Division::PerSaltire, Division::Paly, Division::Barry, Division::Bendy, Division::Checky, Division::Lozengy,
    ];
}

/// A painted (or bare) field with an optional charge: the prototype's "Field and charge".
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    /// Painted in tinctures; otherwise the bare material shows and `division` is ignored.
    pub painted: bool,
    pub division: Division,
    pub tincture: Tincture,
    pub tincture2: Tincture,
    /// [`Device::None`] for an empty field.
    pub charge: Device,
    pub charge_tincture: Tincture,
    /// The charge's size as a fraction of the field's smaller side.
    pub charge_size: f32,
}

sliders! {
    FieldLimits for Field {
        charge_size: 0.2, 0.8, 0.01;
    }
}

impl Default for Field {
    /// The prototype's `BASE.field`: azure, a tower or.
    fn default() -> Self {
        Field {
            painted: true,
            division: Division::Plain,
            tincture: Tincture::Azure,
            tincture2: Tincture::Argent,
            charge: Device::Tower,
            charge_tincture: Tincture::Or,
            charge_size: 0.5,
        }
    }
}

impl Field {
    /// The division as the shader draws it on a painted face, `None` when plain or bare.
    pub fn surface(&self) -> Option<SurfacePattern> {
        let kind = self.division.kind().filter(|_| self.painted)?;
        Some(SurfacePattern { kind, color: self.tincture2.rgb(), scale: self.division.repeats() })
    }

    /// Whether the charge follows the rule of tincture against the field it sits on.
    pub fn charge_contrasts(&self) -> bool {
        let field_metal = self.tincture.is_metal();
        let split_metal = self.division != Division::Plain && self.tincture2.is_metal() != field_metal;
        split_metal || self.charge_tincture.is_metal() != field_metal
    }
}
