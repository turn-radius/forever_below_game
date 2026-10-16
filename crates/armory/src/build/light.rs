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

/// How far up the bend a leaning flame is at `t` of the way up its height, 0 at `bend` (and
/// below) and 1 at the tip.
fn bend_share(t: f32, bend: f32) -> f32 {
    ((t - bend) / (1.0 - bend).max(1e-3)).clamp(0.0, 1.0)
}

/// How far a flame's spine stands off its base line, in flame heights, at `t` of the way up
/// (0 at the base, 1 at the tip). Below `bend` (a share of the height) the spine stands
/// straight up, so the flame's bulb sits centred round what burns; above it the spine leans,
/// its slope `tan(tilt) * 27/4 * s (1 - s)^2` over `s` of the way from the bend to the tip, so
/// it eases out of vertical, leans its steepest (`tilt`) a third of the way up the bend and
/// curls back to vertical at the tip, as a carried flame trails. No kinks. Our own curve.
/// The lean is along +Z, the flame frame's "forward".
pub fn spine_offset(t: f32, tilt: f32, bend: f32) -> f32 {
    let s = bend_share(t, bend);
    (1.0 - bend) * tilt.tan() * 6.75 * (0.5 * s * s - 2.0 / 3.0 * s * s * s + 0.25 * s * s * s * s)
}

/// The spine's lean from vertical at `t` of the way up, in radians: 0 up to `bend`, `tilt` a
/// third of the way from there to the tip, and 0 again at the tip.
pub fn spine_angle(t: f32, tilt: f32, bend: f32) -> f32 {
    let s = bend_share(t, bend);
    (tilt.tan() * 6.75 * s * (1.0 - s) * (1.0 - s)).atan()
}

/// A flame standing on y = 0: an outer veil `height` tall and `radius` wide at its belly,
/// and a bright core inside it. `lean` bends the tip sideways along +X (a fraction of the
/// height), and `seed` sets the shape of its tongues, so no two flames are alike.
/// Returns `(outer, core)`.
pub fn flame(height: f32, radius: f32, lean: f32, seed: u32) -> (MeshBuilder, MeshBuilder) {
    flame_leaning(height, radius, lean, 0.0, 0.0, seed)
}

/// [`flame`] that stands straight up to `bend` of its height and then leans forward (along
/// +Z) at up to `tilt` radians, curling back up to vertical at the tip ([`spine_offset`]).
/// The curve is part of the mesh, not a runtime bend: the mesh stays a pure function of the
/// item and needs no RNG, and the game only has to turn the flame upright and face it.
pub fn flame_leaning(height: f32, radius: f32, lean: f32, tilt: f32, bend: f32, seed: u32) -> (MeshBuilder, MeshBuilder) {
    // `rise` is how far up the whole flame the veil starts, so the core follows the same spine.
    let veil = move |h: f32, r: f32, tongues: f32, rise: f32| {
        let profile = spline_profile(&[[0.0, 0.0], [r * 0.75, h * 0.08], [r, h * 0.26], [r * 0.82, h * 0.48], [r * 0.4, h * 0.76], [0.0, h]], 28);
        let so = (seed % 97) as f32 * 0.37;
        lathe_warped(&profile, &LatheOptions::new(20, Shading::SMOOTH), move |p| {
            let t = (p[1] / h).clamp(0.0, 1.0);
            let angle = (-p[2]).atan2(p[0]);
            // Tongues: the upper flame splits into licks that wander with height.
            let lick = 1.0 + 0.32 * t * (tongues * angle + so + t * 5.0).sin() + 0.12 * (value_noise3([p[0] * 6.0 + so, p[1] * 3.0, p[2] * 6.0], seed) - 0.5);
            let sway = lean * h * t * t;
            let forward = height * spine_offset(((p[1] + rise) / height).clamp(0.0, 1.0), tilt, bend);
            [p[0] * lick + sway, p[1], p[2] * lick + forward]
        })
    };
    (veil(height, radius, 3.0, 0.0), veil(height * 0.62, radius * 0.55, 2.0, height * CORE_RISE))
}

/// How far up the veil its core starts, as a share of the flame's height.
const CORE_RISE: f32 = 0.04;

/// Adds a [`flame`] to `parts` as `flame` and `flame_core`, glowing and unoutlined, after
/// moving it to stand on `base`.
pub fn add_flame(parts: &mut Parts, base: V3, height: f32, radius: f32, lean: f32, seed: u32) {
    add_flame_leaning(parts, base, height, radius, lean, 0.0, 0.0, seed);
}

/// [`add_flame`] for a flame that stands straight to `bend` of its height, then leans up to
/// `tilt` radians forward ([`flame_leaning`]).
#[allow(clippy::too_many_arguments)]
pub fn add_flame_leaning(parts: &mut Parts, base: V3, height: f32, radius: f32, lean: f32, tilt: f32, bend: f32, seed: u32) {
    let (mut outer, mut core) = flame_leaning(height, radius, lean, tilt, bend, seed);
    outer.translate(base);
    // The core sits a little up inside the veil.
    core.translate([base[0], base[1] + height * CORE_RISE, base[2]]);
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

/// A candle standing on y = 0, `radius` wide and `height` tall to its rim, with `drips` runs
/// of wax down its side. A `burnt` candle's top is cupped round the wick and its wick is
/// charred. Returns `(wax, wick)`; the wick's tip is [`candle_wick_top`] above the base,
/// where a flame stands.
pub fn candle(radius: f32, height: f32, drips: u32, burnt: bool, seed: u32) -> (MeshBuilder, MeshBuilder) {
    let r = radius;
    let cup = if burnt { r * 0.28 } else { r * 0.06 };
    let profile = [
        [0.0, 0.0],
        [r * 0.98, 0.0],
        [r, r * 0.04],
        [r, height - r * 0.12],
        [r * 0.96, height],
        [r * 0.82, height - cup * 0.2],
        [r * 0.45, height - cup],
        [0.0, height - cup * 1.05],
    ];
    let mut wax = crate::geom::lathe(&profile, &LatheOptions { facets: 28, shading: Shading::SMOOTH, ..Default::default() });
    // Runs of wax from the rim down the side, each a stretched blob lying on the surface.
    for k in 0..drips {
        let a = (seed % 360) as f32 * 0.0174 + k as f32 * 2.4;
        let run = height * (0.12 + 0.3 * value_noise3([k as f32 * 3.1, seed as f32 * 0.013, 0.5], seed));
        let mut drip = crate::geom::ellipsoid([r * 0.16, run * 0.5, r * 0.12]);
        drip.translate([0.0, height - run * 0.45, r * 0.97]).rotate_y(a);
        wax.append(&drip);
    }
    let wick_len = r * 0.55;
    let base = height - cup * 1.05 - r * 0.05;
    let path: Vec<V3> = (0..=6)
        .map(|i| {
            let t = i as f32 / 6.0;
            // A burnt wick curls over at the tip.
            let bend = if burnt { r * 0.18 * t * t } else { 0.0 };
            [bend, base + wick_len * t, 0.0]
        })
        .collect();
    let wick = crate::geom::tube(&path, 6, &crate::geom::SweepOptions::default(), |t| r * 0.07 * (1.0 - 0.3 * t));
    (wax, wick)
}

/// Height above a [`candle`]'s base of its wick's tip.
pub fn candle_wick_top(radius: f32, height: f32, burnt: bool) -> f32 {
    let cup = if burnt { radius * 0.28 } else { radius * 0.06 };
    height - cup * 1.05 - radius * 0.05 + radius * 0.55
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

    /// Mean position of the ring of vertices nearest height `y`.
    fn ring_centre(points: &[V3], y: f32) -> V3 {
        let near = points.iter().map(|p| p[1]).min_by(|a, b| (a - y).abs().total_cmp(&(b - y).abs())).unwrap();
        let ring: Vec<&V3> = points.iter().filter(|p| (p[1] - near).abs() < 1e-4).collect();
        let n = ring.len() as f32;
        [ring.iter().map(|p| p[0]).sum::<f32>() / n, near, ring.iter().map(|p| p[2]).sum::<f32>() / n]
    }

    #[test]
    fn the_spine_stands_straight_to_the_bend_leans_its_steepest_above_and_stands_up_at_the_tip() {
        let (tilt, bend) = (55f32.to_radians(), 0.3);
        for t in [0.0, 0.15, 0.3] {
            assert_eq!(spine_offset(t, tilt, bend), 0.0);
            assert_eq!(spine_angle(t, tilt, bend), 0.0);
        }
        assert!((spine_angle(bend + (1.0 - bend) / 3.0, tilt, bend) - tilt).abs() < 1e-5);
        assert!(spine_angle(1.0, tilt, bend).abs() < 1e-5);
        // The angle rises then falls, never past `tilt`, and the spine only moves forward.
        let angles: Vec<f32> = (0..=40).map(|i| spine_angle(i as f32 / 40.0, tilt, bend)).collect();
        assert!(angles.iter().all(|&a| a <= tilt + 1e-5));
        let offsets: Vec<f32> = (0..=40).map(|i| spine_offset(i as f32 / 40.0, tilt, bend)).collect();
        assert!(offsets.windows(2).all(|w| w[1] >= w[0]));
        // Smooth: the angle never jumps between close samples, at the bend or anywhere else.
        let fine: Vec<f32> = (0..=1000).map(|i| spine_angle(i as f32 / 1000.0, tilt, bend)).collect();
        assert!(fine.windows(2).all(|w| (w[1] - w[0]).abs() < 0.02));
    }

    #[test]
    fn a_leaning_flames_mesh_follows_its_spine() {
        let (tilt, bend) = (55f32.to_radians(), 0.3);
        let height = 2.0;
        for seed in [0, 1, 77, 9999] {
            let (outer, _) = flame_leaning(height, 0.4, 0.0, tilt, bend, seed);
            let points: Vec<V3> = outer.clone().into_part("flame", OutlineMode::Welded).positions;
            let lean_between = |y0: f32, y1: f32| {
                let (a, b) = (ring_centre(&points, y0), ring_centre(&points, y1));
                ((b[2] - a[2]).atan2(b[1] - a[1])).to_degrees()
            };
            // The bulb below the bend stands centred on the base.
            for y in [0.05, 0.15, 0.25] {
                let c = ring_centre(&points, y * height);
                assert!(c[2].abs() < 0.04 * height, "seed {seed}: ring at {y} off centre {c:?}");
            }
            let steepest = lean_between(0.48 * height, 0.58 * height);
            let tip = lean_between(0.9 * height, 0.98 * height);
            assert!((40.0..=56.0).contains(&steepest), "seed {seed}: leans {steepest} degrees at its steepest");
            assert!(tip.abs() < 10.0, "seed {seed}: tip leans {tip} degrees");
            // Forward, not back: the tip stands ahead of the base.
            let (lo, hi) = outer.bounds().unwrap();
            assert!(hi[2] > 0.0 && hi[2] > -lo[2], "{lo:?} {hi:?}");
            assert!(lo[1].abs() < 1e-4 && (hi[1] - height).abs() < 1e-3);
        }
    }

    #[test]
    fn light_reach_is_in_feet() {
        let light = LightAnchor::feet([0.0, 1.0, 0.0], FLAME_LIGHT, 20.0, 2.0);
        assert!((light.radius - 60.96).abs() < 1e-3);
        assert_eq!(light.flicker, 1.0);
    }
}
