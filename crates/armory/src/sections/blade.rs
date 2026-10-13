//! The business end of a bladed item: the blade and the guard at its base. Shared by
//! swords, daggers and spears (and later axes and glaives). Ranges are the prototype's
//! `SECTIONS` (`blade` and `guard` in `reference/spline_armory_engine.js`); families narrow
//! them.
//!
//! Blade space: the blade runs +Y from its base, the cutting edge faces +X (the back, or
//! spine, faces -X) and the flats face +Z and -Z. A guard's span runs along X, in the plane
//! of the blade's width.

use crate::gem::Gemstone;
use crate::material::MaterialKind;
use crate::sections::hafted::{Grip, Pommel, Shaft};
use crate::slider::sliders;
use serde::{Deserialize, Serialize};

/// The shape of the blade across its width (the prototype's `section` select).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BladeSection {
    /// Lens-shaped: both flats curve smoothly to the edges.
    Lenticular,
    /// A central ridge with flat bevels to each edge.
    Diamond,
    /// A flat centre with bevels to each edge.
    Hexagonal,
    /// Flat with a narrow edge bevel, as on most single-edged blades.
    Flat,
}

impl BladeSection {
    pub const ALL: [BladeSection; 4] = [BladeSection::Lenticular, BladeSection::Diamond, BladeSection::Hexagonal, BladeSection::Flat];

    /// Thickness across the blade as a fraction of the full thickness, at `v` from -1 (one
    /// edge) through 0 (the middle) to 1 (the other edge).
    pub fn thickness_at(self, v: f32) -> f32 {
        let a = v.abs().min(1.0);
        match self {
            BladeSection::Lenticular => (1.0 - a * a).max(0.0).sqrt(),
            BladeSection::Diamond => 1.0 - a,
            BladeSection::Hexagonal => ((1.0 - a) / 0.38).min(1.0),
            BladeSection::Flat => ((1.0 - a) / 0.12).min(1.0),
        }
    }

    /// Where the faces fold (hard edges along the blade), as `v` in -1..=1, edges included.
    pub fn creases(self) -> &'static [f32] {
        match self {
            BladeSection::Lenticular => &[-1.0, 1.0],
            BladeSection::Diamond => &[-1.0, 0.0, 1.0],
            BladeSection::Hexagonal => &[-1.0, -0.62, 0.62, 1.0],
            BladeSection::Flat => &[-1.0, -0.88, 0.88, 1.0],
        }
    }
}

/// The blade: its outline (profile), edge and curve, and cross-section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Blade {
    pub length: f32,
    /// Full width at the widest part of the body (`width` in the prototype).
    pub width: f32,
    /// Width at the base, as a multiple of `width` (`baseNarrow`, "Shoulder width").
    pub shoulder: f32,
    /// Where the belly (the widest point) sits, as a fraction of the length.
    pub belly: f32,
    /// Width at the belly, as a multiple of `width`.
    pub belly_width: f32,
    /// How much the body narrows from the belly to where the point begins, 0..1.
    pub taper: f32,
    /// Where the point begins, as a fraction of the length.
    pub point_start: f32,
    /// 0 is a straight-sided point, 1 a rounded one (`pointShape`, "Point fullness").
    pub point_fullness: f32,
    /// How far the blade bends back, edge on the outside (a sabre's curve). Negative bends it
    /// forward.
    pub curve: f32,
    /// How far the back is a blunt spine rather than a second edge, 0..1 ("Single edge").
    pub spine: f32,
    /// Where the back drops toward the point (the clip), as a fraction of the length.
    pub clip: f32,
    /// Flame waves along the length (a flamberge or kris).
    pub waves: u32,
    /// Wave depth, as a multiple of the half width (`waveAmp`).
    pub wave_depth: f32,
    /// The unsharpened stretch above the guard, as a fraction of the length.
    pub ricasso: f32,
    pub section: BladeSection,
    pub thickness: f32,
    /// Fuller depth, 0..1 of the thickness it may take.
    pub fuller: f32,
    /// How far up the blade the fullers run, as a fraction of the length.
    pub fuller_length: f32,
    /// Fuller width, as a fraction of the half width.
    pub fuller_width: f32,
    /// Parallel fullers side by side.
    pub fullers: u32,
    pub material: MaterialKind,
}

sliders! {
    BladeLimits for Blade {
        length: 1.0, 13.0, 0.05;
        width: 0.08, 1.2, 0.005;
        shoulder: 0.1, 1.6, 0.01;
        belly: 0.0, 0.9, 0.01;
        belly_width: 0.5, 1.8, 0.01;
        taper: -0.3, 0.9, 0.01;
        point_start: 0.1, 0.99, 0.01;
        point_fullness: 0.0, 1.0, 0.01;
        curve: -0.3, 1.0, 0.01;
        spine: 0.0, 1.0, 0.01;
        clip: 0.5, 1.0, 0.01;
        waves: 0.0, 16.0, 1.0;
        wave_depth: 0.0, 0.25, 0.005;
        ricasso: 0.0, 0.3, 0.005;
        thickness: 0.02, 0.16, 0.002;
        fuller: 0.0, 1.0, 0.01;
        fuller_length: 0.05, 0.95, 0.01;
        fuller_width: 0.05, 0.6, 0.01;
        fullers: 1.0, 3.0, 1.0;
    }
}

impl Default for Blade {
    /// The prototype's `BASE.blade`.
    fn default() -> Self {
        Blade {
            length: 7.5,
            width: 0.5,
            shoulder: 1.0,
            belly: 0.0,
            belly_width: 1.0,
            taper: 0.45,
            point_start: 0.78,
            point_fullness: 0.35,
            curve: 0.0,
            spine: 0.0,
            clip: 0.8,
            waves: 0,
            wave_depth: 0.08,
            ricasso: 0.0,
            section: BladeSection::Lenticular,
            thickness: 0.07,
            fuller: 0.5,
            fuller_length: 0.62,
            fuller_width: 0.22,
            fullers: 1,
            material: MaterialKind::Iron,
        }
    }
}

impl Blade {
    /// Single-edged: the back is mostly spine.
    pub fn is_single_edged(&self) -> bool {
        self.spine > 0.5
    }
}

/// The guard's form (the prototype's `style` select).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardStyle {
    /// Two quillons, straight, curved or curled.
    Cross,
    /// A straight bar.
    Bar,
    /// A round plate (rondel daggers, the gladius).
    Disc,
    /// Two rounded lobes (ballock daggers).
    Lobes,
}

impl GuardStyle {
    pub const ALL: [GuardStyle; 4] = [GuardStyle::Cross, GuardStyle::Bar, GuardStyle::Disc, GuardStyle::Lobes];

    /// Styles with arms that end in terminals.
    pub fn has_arms(self) -> bool {
        matches!(self, GuardStyle::Cross | GuardStyle::Bar)
    }
}

/// Where a spear's wings or lugs sit (spears only; swords and daggers ignore it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardMount {
    /// On the socket, below the blade.
    Socket,
    /// At the base of the blade.
    Blade,
}

/// What finishes each end of a cross or bar (the prototype's `ends`, "Terminals").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardTerminal {
    None,
    Ball,
    Disc,
    /// A trumpet flare.
    Flare,
}

impl GuardTerminal {
    pub const ALL: [GuardTerminal; 4] = [GuardTerminal::None, GuardTerminal::Ball, GuardTerminal::Disc, GuardTerminal::Flare];
}

/// The guard (crossguard, quillons, rondel, lobes, or a spear's wings and lugs).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guard {
    pub on: bool,
    pub style: GuardStyle,
    pub mount: GuardMount,
    /// Reach of each arm from the centre (a disc's radius, the lobes' combined width).
    pub span: f32,
    /// Radius of the arms (half a disc's thickness; the prototype's "Thickness").
    pub radius: f32,
    /// How far the arms curve toward the blade (negative: toward the grip).
    pub curve: f32,
    /// The second arm curves the other way.
    pub s_curve: bool,
    /// How much the arms narrow toward their ends, 0..1.
    pub taper: f32,
    /// How far the arm ends curl back on themselves, 0..1.
    pub curl: f32,
    pub terminal: GuardTerminal,
    pub terminal_size: f32,
    /// Width of the centre block (the escutcheon); 0 for none.
    pub block: f32,
    pub block_height: f32,
    /// Length of the langets, the tongues that run up each flat of the blade; 0 for none.
    pub langets: f32,
    pub material: MaterialKind,
    /// A stone set in the centre, one on each face.
    pub gem: bool,
    pub gem_size: f32,
    pub gem_stone: Gemstone,
}

sliders! {
    GuardLimits for Guard {
        span: 0.15, 2.0, 0.01;
        radius: 0.015, 0.15, 0.002;
        curve: -1.0, 1.0, 0.01;
        taper: 0.0, 1.0, 0.01;
        curl: 0.0, 1.0, 0.01;
        terminal_size: 0.02, 0.2, 0.005;
        block: 0.0, 0.5, 0.005;
        block_height: 0.04, 0.4, 0.005;
        langets: 0.0, 1.2, 0.01;
        gem_size: 0.02, 0.2, 0.005;
    }
}

impl Default for Guard {
    /// The prototype's `BASE.guard`.
    fn default() -> Self {
        Guard {
            on: true,
            style: GuardStyle::Cross,
            mount: GuardMount::Socket,
            span: 1.0,
            radius: 0.05,
            curve: 0.1,
            s_curve: false,
            taper: 0.35,
            curl: 0.0,
            terminal: GuardTerminal::Flare,
            terminal_size: 0.06,
            block: 0.2,
            block_height: 0.14,
            langets: 0.0,
            material: MaterialKind::Iron,
            gem: false,
            gem_size: 0.05,
            gem_stone: Gemstone::Garnet,
        }
    }
}

impl Guard {
    /// How tall the guard stands along the blade's axis: what a hilt stacks the blade on
    /// (`gh` in the prototype's `buildBladed`). Zero when the guard is off.
    pub fn height(&self) -> f32 {
        if !self.on {
            return 0.0;
        }
        match self.style {
            GuardStyle::Lobes => self.span * 0.9,
            GuardStyle::Disc => self.radius * 2.0,
            GuardStyle::Cross | GuardStyle::Bar => (self.radius * 2.0).max(if self.block > 0.0 { self.block_height } else { 0.0 }),
        }
    }
}

/// The prototype's `proportion()` rules for a hilted blade (swords and daggers): the grip
/// runs the whole handle, the pommel suits the handle, the blade is no wider than its length
/// allows nor thicker or thinner than its width, and the guard clears the blade and fits
/// the grip. Apply before clamping to a family's limits.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HiltProportions {
    /// Widest pommel, as a multiple of the handle radius.
    pub pommel_max: f32,
    /// Widest blade, as a fraction of its length.
    pub width_per_length: f32,
}

impl HiltProportions {
    pub const SWORD: HiltProportions = HiltProportions { pommel_max: 3.0, width_per_length: 0.12 };
    pub const DAGGER: HiltProportions = HiltProportions { pommel_max: 2.6, width_per_length: 0.32 };

    pub fn apply(&self, shaft: &Shaft, grip: &mut Grip, pommel: &mut Pommel, guard: &mut Guard, blade: &mut Blade) {
        grip.length = shaft.length;
        grip.position = 0.0;
        pommel.radius = pommel.radius.max(shaft.radius * 1.15).min(shaft.radius * self.pommel_max);
        blade.width = blade.width.min(blade.length * self.width_per_length);
        blade.thickness = blade.thickness.max((blade.width * 0.12).min(0.03)).min((blade.width * 0.55).max(0.04));
        match guard.style {
            GuardStyle::Cross | GuardStyle::Bar => guard.span = guard.span.max(blade.width * 0.6 + guard.radius),
            GuardStyle::Disc => guard.span = guard.span.max(blade.width * 0.6).min(shaft.radius * 3.2),
            GuardStyle::Lobes => guard.span = guard.span.max(shaft.radius * 1.4).min(shaft.radius * 2.2),
        }
        guard.terminal_size = guard.terminal_size.min(guard.radius * 2.2 + 0.01);
    }
}
