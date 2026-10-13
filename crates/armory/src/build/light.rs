//! Carried light: where a torch, lantern, candle or glowcap gives its light, in what colour
//! and how far, and the flames and embers that show it.
//!
//! In the abyss carried light is the main light (`docs/rendering.md`, "Light"), and its
//! radius is a game stat (`light` in `data/bases.json`, "Light 5 ft" in `data/goods.json`).
//! Each light-giving family returns a [`LightAnchor`] from its own `light(&state)`, in the same
//! item space as its mesh, so the game can hang a point light on the held item.

use serde::{Deserialize, Serialize};

use crate::build::parts::{Look, Parts};
use crate::geom::{LatheOptions, MeshBuilder, Shading, V3, lathe_warped, spline_profile, torus, value_noise3};
use crate::material::{MaterialKind, Rgb};

/// Item units (10 cm) in a foot: the game's light stats are in feet.
pub const UNITS_PER_FOOT: f32 = 3.048;

/// An open flame's light, the prototype's torch light (`#ffb36a`, `docs/rendering.md`).
pub const FLAME_LIGHT: Rgb = Rgb::hex(0xffb36a);
/// A tallow or wax candle flame: a little yellower than a torch.
pub const CANDLE_LIGHT: Rgb = Rgb::hex(0xffc98a);
/// Light through horn panes: warmer and duskier.
pub const HORN_LIGHT: Rgb = Rgb::hex(0xffa457);
/// A glowcap's cold light (the plate's glowcap cap colour).
pub const GLOWCAP_LIGHT: Rgb = Rgb::hex(0x7fd8c8);

/// The outer veil of a flame.
pub const FLAME_OUTER: Rgb = Rgb::hex(0xff8a2a);
/// The bright core of a flame.
pub const FLAME_CORE: Rgb = Rgb::hex(0xffe08a);
/// Live embers at a burning edge.
pub const EMBER: Rgb = Rgb::hex(0xff5a1a);
/// Charred wood, cloth and wick.
pub const CHAR: Rgb = Rgb::hex(0x1d1713);

/// Where an item gives its light, in item space.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LightAnchor {
    /// Centre of the light: the heart of the flame, or of a glowing cap.
    pub position: V3,
    /// Suggested light colour (sRGB).
    pub color: Rgb,
    /// Suggested reach, in item units (10 cm), from the game's light stat in feet.
    pub radius: f32,
    /// How much the light wavers, 0 (steady, a glowcap) to 1 (a torch in a draught). The
    /// game scales the prototype's flicker curve by it.
    pub flicker: f32,
}

impl LightAnchor {
    /// A light of `feet` reach (the game's stat) at `position`.
    pub fn feet(position: V3, color: Rgb, feet: f32, flicker: f32) -> Self {
        LightAnchor { position, color, radius: feet * UNITS_PER_FOOT, flicker: flicker.clamp(0.0, 1.0) }
    }
}

/// A flame standing on y = 0: an outer veil `height` tall and `radius` wide at its belly,
/// and a bright core inside it. `lean` bends the tip sideways along +X (a fraction of the
/// height), and `seed` sets the shape of its tongues, so no two flames are alike.
/// Returns `(outer, core)`.
pub fn flame(height: f32, radius: f32, lean: f32, seed: u32) -> (MeshBuilder, MeshBuilder) {
    let veil = |h: f32, r: f32, tongues: f32| {
        let profile = spline_profile(&[[0.0, 0.0], [r * 0.75, h * 0.08], [r, h * 0.26], [r * 0.82, h * 0.48], [r * 0.4, h * 0.76], [0.0, h]], 28);
        let so = (seed % 97) as f32 * 0.37;
        lathe_warped(&profile, &LatheOptions::new(20, Shading::SMOOTH), move |p| {
            let t = (p[1] / h).clamp(0.0, 1.0);
            let angle = (-p[2]).atan2(p[0]);
            // Tongues: the upper flame splits into licks that wander with height.
            let lick = 1.0 + 0.32 * t * (tongues * angle + so + t * 5.0).sin() + 0.12 * (value_noise3([p[0] * 6.0 + so, p[1] * 3.0, p[2] * 6.0], seed) - 0.5);
            let sway = lean * h * t * t;
            [p[0] * lick + sway, p[1], p[2] * lick]
        })
    };
    (veil(height, radius, 3.0), veil(height * 0.62, radius * 0.55, 2.0))
}

/// Adds a [`flame`] to `parts` as `flame` and `flame_core`, glowing and unoutlined, after
/// moving it to stand on `base`.
pub fn add_flame(parts: &mut Parts, base: V3, height: f32, radius: f32, lean: f32, seed: u32) {
    let (mut outer, mut core) = flame(height, radius, lean, seed);
    outer.translate(base);
    // The core sits a little up inside the veil.
    core.translate([base[0], base[1] + height * 0.04, base[2]]);
    parts.add_look("flame", outer, glowing(FLAME_OUTER, 1.0));
    parts.add_look("flame_core", core, glowing(FLAME_CORE, 1.0));
}

/// A glowing, unoutlined look in its own colour: flames, embers, lit horn and glowing caps.
pub fn glowing(color: Rgb, glow: f32) -> Look {
    Look { glow: glow.clamp(0.0, 1.0), ..Look::of(MaterialKind::Gem).tinted(color).without_outline() }
}

/// A ring of embers round a burning edge of `radius` at height `y`, `thickness` thick.
pub fn ember_ring(radius: f32, y: f32, thickness: f32) -> MeshBuilder {
    let mut ring = torus(radius, thickness, 40, 6, Shading::SMOOTH);
    ring.translate([0.0, y, 0.0]);
    ring
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    #[test]
    fn flames_are_drawable_and_stand_on_their_base() {
        for seed in [0, 1, 77, 9999] {
            let (outer, core) = flame(1.5, 0.3, 0.2, seed);
            let (lo, hi) = outer.bounds().unwrap();
            assert!(lo[1].abs() < 1e-4 && (hi[1] - 1.5).abs() < 1e-3, "{lo:?} {hi:?}");
            let (clo, chi) = core.bounds().unwrap();
            assert!(clo[1] >= -1e-4 && chi[1] < hi[1]);
            outer.into_part("flame", OutlineMode::Welded).validate().unwrap();
            core.into_part("flame_core", OutlineMode::Welded).validate().unwrap();
        }
    }

    #[test]
    fn light_reach_is_in_feet() {
        let light = LightAnchor::feet([0.0, 1.0, 0.0], FLAME_LIGHT, 20.0, 2.0);
        assert!((light.radius - 60.96).abs() < 1e-3);
        assert_eq!(light.flicker, 1.0);
    }
}
