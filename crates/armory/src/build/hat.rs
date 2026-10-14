//! Hats on a [`HeadForm`]: a crown raised from the brow line, a brim out from its base, a band
//! round the join and sometimes a feather in it. Built in the head's space (front +Z), so
//! `fb_body` can pass a folk body's own head.
//!
//! The crown is a surface of revolution about the head's vertical axis, drawn through the
//! brow line ([`HeadForm::brow_point`]) rather than a circle, so it follows the head's oval
//! and its tip back: every column runs from the brow up a profile (`radius`, `height`) to the
//! top, kept off the head ([`HeadForm::point_at_height`]) where the profile would cut into it,
//! then slumped over by `bend`. The brim is a sheet out from the brow line bent along a
//! circular arc, flat, drooping or turned up against the crown. Both are thickened with
//! [`shell`], so the felt shows an edge.
//!
//! The plate prototype draws its `Pointed hat` as a torus brim under a cone swept along a
//! bent spline; here the cone, the brim and the band are one fitted set on the head form.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use procgen_rng::Mulberry32;

use crate::build::form::{HeadForm, ShellOptions, shell};
use crate::build::hafted::smoothstep;
use crate::build::parts::{Look, Parts};
use crate::fabric::Fabric;
use crate::geom::vec3::{self, V3};
use crate::geom::{CatmullRom, GridOptions, Shading, SweepOptions, stitch_grid, tube};
use crate::sections::hat::{Hat, HatStyle};

/// Felt thickness of the crown and the brim.
const CROWN_THICKNESS: f32 = 0.04;
const BRIM_THICKNESS: f32 = 0.05;
/// Columns round the hat.
const COLUMNS: usize = 96;

/// The hat's crown and brim, ready to sample.
#[derive(Clone, Debug)]
pub struct HatSurface {
    pub head: HeadForm,
    pub hat: Hat,
    /// Distance the band keeps off the head.
    pub ease: f32,
    /// Height of the top of the crown, and of the highest point of the brow line.
    pub top: f32,
    pub brow: f32,
    /// Phases of the crumple's dents and waves.
    phases: [f32; 3],
}

impl HatSurface {
    pub fn new(head: &HeadForm, hat: &Hat, seed: u32) -> HatSurface {
        let mut rng = Mulberry32::new(seed ^ 0x4a7c_15e3);
        let phases = [rng.range_f32(0.0, TAU), rng.range_f32(0.0, TAU), rng.range_f32(0.0, TAU)];
        let ease = 0.04 + 0.12 * hat.ease;
        let brow = (0..8).map(|i| head.brow_point(i as f32 / 8.0 * TAU, ease)[1]).fold(f32::MIN, f32::max);
        // However low the crown is set, it clears the head.
        let top = (brow + hat.crown).max(head.crown_top() + ease + 0.15);
        HatSurface { head: *head, hat: *hat, ease, top, brow, phases }
    }

    /// The crown's profile at `t` (0 the brow, 1 the top): the share of the brow's radius
    /// kept, and the share of the height risen.
    fn profile(&self, t: f32) -> (f32, f32) {
        match self.hat.style {
            HatStyle::Pointed => ((1.0 - t).powf(1.15), t),
            HatStyle::WideBrim => {
                // Straight, slightly tapered sides, then a squared-off dome.
                const KNEE: f32 = 0.5;
                const DOME: f32 = 0.3;
                const TAPER: f32 = 0.1;
                if t < KNEE {
                    let s = t / KNEE;
                    (1.0 - TAPER * s, (1.0 - DOME) * s)
                } else {
                    let a = (t - KNEE) / (1.0 - KNEE) * FRAC_PI_2;
                    ((1.0 - TAPER) * a.cos().max(0.0).powf(0.55), 1.0 - DOME + DOME * a.sin().max(0.0).powf(0.6))
                }
            }
            HatStyle::Cap => {
                let a = t * FRAC_PI_2;
                let dome = (a.cos().max(0.0).powf(0.8), a.sin().max(0.0).powf(0.9));
                // A cap that slumps forward is drawn up into a soft cone first: the Phrygian
                // cap's tall, rounded peak.
                let peak = ((1.0 - t).powf(0.6) * (1.0 - 0.1 * t), t);
                let w = smoothstep(0.25, 0.6, self.hat.bend);
                (dome.0 + (peak.0 - dome.0) * w, dome.1 + (peak.1 - dome.1) * w)
            }
        }
    }

    /// Which way the top slumps.
    fn bend_direction(&self) -> V3 {
        match self.hat.style {
            HatStyle::Cap => [0.0, 0.0, 1.0],
            _ => vec3::normalize_or([0.3, 0.0, -1.0], vec3::Z),
        }
    }

    /// How far the crown's axis is carried at height `y`, and its direction there. A
    /// stiffened pointed hat bends along its whole height, most at the tip; a soft cap stays
    /// on the head and only the peak standing above it falls over.
    fn bend_at(&self, y: f32) -> (V3, V3) {
        let (pivot, reach, fall) = match self.hat.style {
            HatStyle::Cap => (self.head.crown_top() - 0.3, 0.8, 0.5),
            _ => (self.brow, 0.55, 0.3),
        };
        let span = (self.top - pivot).max(1e-3);
        let bend = self.hat.bend * span;
        let power = if self.hat.style == HatStyle::Cap { 2.0 } else { 2.2 };
        let offset = |y: f32| {
            let k = ((y - pivot) / span).clamp(0.0, 1.0).powf(power);
            vec3::add(vec3::scale(self.bend_direction(), bend * reach * k), [0.0, -bend * fall * k * k, 0.0])
        };
        let h = 0.01 * span;
        let tangent = vec3::add([0.0, 2.0 * h, 0.0], vec3::sub(offset(y + h), offset(y - h)));
        (offset(y), vec3::normalize_or(tangent, vec3::Y))
    }

    /// A point on the crown at column `u` (0..1 round from the face) and row `t` (0 the brow,
    /// 1 the top), `inflate` further out across the crown.
    pub fn crown_point(&self, u: f32, t: f32, inflate: f32) -> V3 {
        let az = u * TAU;
        let head = &self.head;
        let c = head.centre;
        let brow = head.brow_point(az, self.ease);
        let (radius, rise) = self.profile(t);
        // Felt blocked by hand: a few soft dents round the crown, strongest halfway up.
        let dent = 1.0 + self.hat.crumple * 0.06 * (3.0 * az + self.phases[0]).sin() * (PI * t).sin();
        let y = brow[1] - 0.04 + (self.top - brow[1] + 0.04) * rise;
        let out = [brow[0] - c[0], 0.0, brow[2] - c[2]];
        let r_brow = vec3::length(out).max(1e-4);
        let dir = vec3::scale(out, 1.0 / r_brow);
        // Kept off the head where the profile would cut into it, joined smoothly.
        let floor = {
            let p = head.point_at_height(az, y, self.ease);
            vec3::length([p[0] - c[0], 0.0, p[2] - c[2]])
        };
        // The rounding fades with the head under it, so the top closes to a point.
        let r = smooth_max(r_brow * radius * dent, floor, 0.08 * (floor / 0.2).min(1.0)) + inflate;
        // Bent about a spine up the crown's axis, each row turned square to the spine, so the
        // crown keeps its girth where the top falls over sideways.
        let axis = [c[0], y, c[2]];
        let (offset, turn) = self.bend_at(y);
        // Wide rows near the head turn only a little, or they would fold over the rows below.
        let turn = vec3::lerp(vec3::Y, turn, (1.0 - radius).clamp(0.0, 1.0));
        let row = vec3::Mat3::from_rotation_arc(vec3::Y, turn);
        vec3::add(vec3::add(axis, offset), row.transform([dir[0] * r, 0.0, dir[2] * r]))
    }

    /// The crown row at height `height` above the brow in column `u`.
    fn row_at_height(&self, u: f32, height: f32) -> f32 {
        let base = self.crown_point(u, 0.0, 0.0)[1];
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = 0.5 * (lo + hi);
            if self.crown_point(u, mid, 0.0)[1] - base < height {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// A point on the brim's top face at column `u` and `v` (0 the crown, 1 the edge).
    pub fn brim_point(&self, u: f32, v: f32) -> V3 {
        let az = u * TAU;
        let head = &self.head;
        let c = head.centre;
        let brow = head.brow_point(az, self.ease);
        let out = [brow[0] - c[0], 0.0, brow[2] - c[2]];
        let dir = vec3::normalize_or(out, vec3::Z);
        // A broad brim reaches a little further fore and aft than at the sides.
        let oval = if self.hat.droop < -0.3 { 1.0 } else { 1.0 + 0.12 * az.cos().powi(2) };
        let width = self.hat.brim * oval;
        // Bent along a circular arc: `droop` sets the angle reached at the edge.
        let angle = self.hat.droop * 1.4;
        let (across, down) = if angle.abs() < 1e-3 {
            (width * v, 0.0)
        } else {
            let radius = width / angle;
            (radius * (angle * v).sin(), radius * (1.0 - (angle * v).cos()))
        };
        // The felt waves where it was blocked unevenly: two lobes dip fore and aft.
        let wave = self.hat.crumple * 0.12 * width * v * v * (2.0 * az + self.phases[1]).cos();
        [brow[0] + dir[0] * across, brow[1] - down - wave, brow[2] + dir[2] * across]
    }
}

/// A max of `a` and `b` rounded over `k`, so a surface pushed off another meets it without
/// a crease (the polynomial smooth maximum; this port's own use of the standard form).
fn smooth_max(a: f32, b: f32, k: f32) -> f32 {
    0.5 * (a + b + ((a - b).powi(2) + k * k).sqrt())
}

/// Every part of a hat on `head`: `"crown"`, `"brim"`, `"band"`, `"feather"` and `"quill"`, as
/// the state calls for them.
pub fn hat_parts(head: &HeadForm, hat: &Hat, seed: u32) -> Parts {
    let mut parts = Parts::new();
    let surface = HatSurface::new(head, hat, seed);
    let felt = Look::cloth(&hat.cloth);
    let rows = if hat.style == HatStyle::Pointed { 72 } else { 40 };

    let crown: Vec<Vec<V3>> = (0..=rows)
        .map(|r| (0..COLUMNS).map(|c| surface.crown_point(c as f32 / COLUMNS as f32, r as f32 / rows as f32, 0.0)).collect())
        .collect();
    // Felt has thickness only where the crown is wide enough to hold it: near the tip the
    // inner face would turn inside out, so the tip is a closed cone on the shell's top lip.
    let spread = |row: &[V3]| {
        let c = vec3::centroid(row);
        row.iter().map(|&p| vec3::distance(p, c)).fold(0.0, f32::max)
    };
    let cut = crown.iter().position(|row| spread(row) < 3.0 * CROWN_THICKNESS).unwrap_or(rows).max(1);
    let mut crown_mesh = shell(&crown[..=cut], &ShellOptions::tube(CROWN_THICKNESS));
    if cut < rows {
        crown_mesh.append(&stitch_grid(&crown[cut..], &GridOptions { wrap_around: true, wrap_along: false, shading: Shading::SMOOTH, row_v: None }));
    }
    parts.add_look("crown", crown_mesh, felt);

    if hat.brim > 0.05 {
        // Rows from the edge in, so the outer face is the top.
        let n = 16;
        let brim: Vec<Vec<V3>> = (0..=n)
            .map(|r| (0..COLUMNS).map(|c| surface.brim_point(c as f32 / COLUMNS as f32, 1.0 - r as f32 / n as f32)).collect())
            .collect();
        parts.add_look("brim", shell(&brim, &ShellOptions::tube(BRIM_THICKNESS)), felt);
    }

    let band_rows = |height: f32, inflate: f32| -> Vec<Vec<V3>> {
        (0..=2)
            .map(|r| {
                (0..COLUMNS)
                    .map(|c| {
                        let u = c as f32 / COLUMNS as f32;
                        let t = surface.row_at_height(u, 0.04 + height * r as f32 / 2.0);
                        surface.crown_point(u, t, inflate)
                    })
                    .collect()
            })
            .collect()
    };
    if hat.band > 0.0 {
        parts.add_look("band", shell(&band_rows(hat.band, 0.03), &ShellOptions::tube(0.03)), Look::cloth(&hat.band_cloth));
    }

    if hat.feather {
        let (feather, quill) = feather(&surface, seed);
        let look = Look::cloth(&Fabric::plain(hat.feather_color));
        parts.add_look("feather", feather, look);
        parts.add_look("quill", quill, look);
    }
    parts
}

/// A feather tucked into the band on the wearer's left, sweeping back and up: a vane either
/// side of a tapering quill, cupped a little along its length.
fn feather(surface: &HatSurface, seed: u32) -> (crate::geom::MeshBuilder, crate::geom::MeshBuilder) {
    let mut rng = Mulberry32::new(seed ^ 0x0fea_7e55);
    let u = 0.25 + 0.06;
    let height = surface.hat.band.max(0.12) * 0.5;
    let anchor = surface.crown_point(u, surface.row_at_height(u, 0.04 + height), 0.05);
    let length = rng.range_f32(1.6, 2.4);
    let lift = rng.range_f32(0.5, 0.9);
    let control: Vec<V3> = [[0.0, -0.15, 0.1], [0.0, 0.0, 0.0], [0.12, 0.35 * lift, -0.35], [0.22, 0.75 * lift, -0.75], [0.2, lift, -1.0]]
        .iter()
        .map(|&k| vec3::add(anchor, vec3::scale(k, if k[2] > 0.0 { 1.0 } else { length })))
        .collect();
    let spine = CatmullRom::centripetal(control).sample_evenly(40);
    let n = spine.len();
    let outward = [1.0, 0.0, 0.0];
    let width = 0.22 * length / 2.0;
    let vane: Vec<Vec<V3>> = (0..=6)
        .map(|j| {
            let v = j as f32 / 6.0 * 2.0 - 1.0;
            (0..n)
                .map(|i| {
                    let t = i as f32 / (n - 1) as f32;
                    let tangent = vec3::normalize_or(vec3::sub(spine[(i + 1).min(n - 1)], spine[i.saturating_sub(1)]), vec3::Y);
                    let side = vec3::normalize_or(vec3::cross(tangent, outward), vec3::Y);
                    let normal = vec3::cross(side, tangent);
                    // Bare quill where it enters the band, widest two thirds along, to a point.
                    let w = width * smoothstep(0.12, 0.3, t) * (1.0 - t.powf(3.0)).max(0.0).sqrt();
                    vec3::add(spine[i], vec3::add(vec3::scale(side, v * w), vec3::scale(normal, -0.35 * w * v * v)))
                })
                .collect()
        })
        .collect();
    let quill = tube(&spine, 6, &SweepOptions::default(), |t| 0.025 * (1.0 - 0.8 * t));
    (shell(&vane, &ShellOptions::sheet(0.012)), quill)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crown_clears_the_head() {
        let head = HeadForm::mannequin(1.0);
        for style in HatStyle::ALL {
            let hat = Hat { style, crown: 0.6, bend: 0.0, ..Hat::default() };
            let s = HatSurface::new(&head, &hat, 3);
            assert!(s.top > head.crown_top(), "{style:?}");
            for i in 0..16 {
                for j in 0..=8 {
                    let (u, t) = (i as f32 / 16.0, j as f32 / 8.0);
                    let p = s.crown_point(u, t, 0.0);
                    let q = head.point_at_height(u * TAU, p[1], 0.0);
                    let r = |p: V3| vec3::length([p[0] - head.centre[0], 0.0, p[2] - head.centre[2]]);
                    assert!(r(p) >= r(q) - 1e-3 || p[1] > head.crown_top(), "{style:?} at {u}, {t}");
                }
            }
        }
    }

    #[test]
    fn a_turned_up_brim_rises_and_a_droop_falls() {
        let head = HeadForm::mannequin(1.0);
        let level = |droop: f32| {
            let s = HatSurface::new(&head, &Hat { droop, brim: 1.0, crumple: 0.0, ..Hat::default() }, 1);
            s.brim_point(0.0, 1.0)[1] - s.brim_point(0.0, 0.0)[1]
        };
        assert!(level(-1.0) > 0.5);
        assert!(level(0.0).abs() < 1e-4);
        assert!(level(0.8) < -0.3);
    }
}
