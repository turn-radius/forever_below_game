//! The head of a thrusting polearm: the spearhead blade and the wings or lugs below it.
//! Spears use it now; javelins, pikes, partisans and other polearm heads can reuse it.
//!
//! The sliders are the prototype's `blade` section (`SECTIONS` in
//! `reference/spline_armory_engine.js`) less the sword-only ones (curve, single edge, clip,
//! ricasso), plus a midrib, with the cross-section shared with swords and daggers
//! ([`BladeSection`]); its `guard` section as spears use it ("Wings and lugs"); and its
//! `socket` section, in [`crate::sections::socket`].

use crate::material::MaterialKind;
use crate::sections::blade::{BladeSection, GuardMount, GuardTerminal};
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

/// The spearhead blade. Item space: its base at y = 0, its point up +Y, its edges along
/// ±X and its faces toward ±Z.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spearhead {
    pub length: f32,
    /// Full width at the widest.
    pub width: f32,
    /// Width at the base, as a fraction of `width`.
    pub shoulder: f32,
    /// Where the blade is widest, as a fraction of its length.
    pub belly: f32,
    /// Width at the belly, as a multiple of `width`.
    pub belly_width: f32,
    /// How much the edges close in from the belly to where the point begins.
    pub taper: f32,
    /// Where the point begins, as a fraction of the length.
    pub point_start: f32,
    /// 0 is a straight-sided point, 1 a round, full one.
    pub point_fullness: f32,
    /// Flame waves along the edges.
    pub waves: u32,
    /// Depth of the waves, as a fraction of `width`.
    pub wave_depth: f32,
    pub section: BladeSection,
    pub thickness: f32,
    /// How far up the blade the socket's round rib runs, as a fraction of its length.
    pub midrib: f32,
    /// Depth of the fullers, 0 for none.
    pub fuller: f32,
    pub fuller_length: f32,
    pub fuller_width: f32,
    pub fullers: u32,
    pub material: MaterialKind,
}

sliders! {
    SpearheadLimits for Spearhead {
        length: 1.0, 13.0, 0.05;
        width: 0.08, 1.2, 0.005;
        shoulder: 0.1, 1.6, 0.01;
        belly: 0.0, 0.9, 0.01;
        belly_width: 0.5, 1.8, 0.01;
        taper: -0.3, 0.9, 0.01;
        point_start: 0.1, 0.99, 0.01;
        point_fullness: 0.0, 1.0, 0.01;
        waves: 0.0, 16.0, 1.0;
        wave_depth: 0.0, 0.25, 0.005;
        thickness: 0.02, 0.16, 0.002;
        midrib: 0.0, 1.0, 0.01;
        fuller: 0.0, 1.0, 0.01;
        fuller_length: 0.05, 0.95, 0.01;
        fuller_width: 0.05, 0.6, 0.01;
        fullers: 1.0, 3.0, 1.0;
    }
}

impl Default for Spearhead {
    /// The prototype's "Leaf spear" blade.
    fn default() -> Self {
        Spearhead {
            length: 3.2,
            width: 0.68,
            shoulder: 0.3,
            belly: 0.38,
            belly_width: 1.15,
            taper: 0.25,
            point_start: 0.6,
            point_fullness: 0.55,
            waves: 0,
            wave_depth: 0.08,
            section: BladeSection::Diamond,
            thickness: 0.08,
            midrib: 0.0,
            fuller: 0.0,
            fuller_length: 0.62,
            fuller_width: 0.22,
            fullers: 1,
            material: MaterialKind::Iron,
        }
    }
}

/// Wings stand out flat in the blade's plane; a bar passes through the socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LugStyle {
    /// Flat lugs in the blade's plane (a winged spear's wings, a partisan's flukes).
    Wings,
    /// A crossbar through the socket (a boar spear's toggle).
    Bar,
}

/// Wings or a bar below the blade (the prototype's "Wings and lugs").
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lugs {
    pub on: bool,
    pub style: LugStyle,
    /// Where the lugs are fixed (the prototype's guard `mount`).
    pub mount: GuardMount,
    /// How far each lug reaches out from the socket or blade.
    pub span: f32,
    /// Thickness at the root.
    pub thickness: f32,
    /// Sweep toward the point (positive) or the butt (negative); past 0.5 they are flukes.
    pub curve: f32,
    /// How much they thin toward the ends.
    pub taper: f32,
    /// The terminals on the lugs' ends (the prototype's guard `ends`).
    pub ends: GuardTerminal,
    pub end_size: f32,
    pub material: MaterialKind,
}

sliders! {
    LugsLimits for Lugs {
        span: 0.15, 2.0, 0.01;
        thickness: 0.015, 0.15, 0.002;
        curve: -1.0, 1.0, 0.01;
        taper: 0.0, 1.0, 0.01;
        end_size: 0.02, 0.2, 0.005;
    }
}

impl Default for Lugs {
    fn default() -> Self {
        Lugs {
            on: false,
            style: LugStyle::Wings,
            mount: GuardMount::Socket,
            span: 0.42,
            thickness: 0.07,
            curve: 0.0,
            taper: 0.7,
            ends: GuardTerminal::None,
            end_size: 0.06,
            material: MaterialKind::Iron,
        }
    }
}
