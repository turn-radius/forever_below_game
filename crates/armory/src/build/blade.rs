//! Blade geometry, from the [`crate::sections::blade::Blade`] section. Swords, daggers and
//! spears build their blades here; axes and glaives can too.
//!
//! A blade is an outline (the half widths of its edge and its back at each point along it)
//! times a cross-section (how thick it is at each point across it), laid along a centreline
//! that may curve and snake. Blade space: the base is at y = 0 and the blade runs +Y, the
//! edge faces +X, the flats face ±Z.
//!
//! From the prototype's `bladeGeo` in `reference/spline_armory_engine.js`, rebuilt at higher
//! fidelity:
//! - Ridges and bevels are hard creases (a diamond's midrib, a hexagonal blade's bevels, a
//!   flat blade's edge bevel) with smooth faces between them; fullers are smooth grooves.
//! - The width is laid out across the curved centreline, so a sabre keeps its width round the
//!   curve instead of being sheared sideways.
//! - Flame waves snake the whole blade (both edges in phase, as on a flamberge or kris)
//!   instead of swelling and pinching its width.
//! - Fullers on a single-edged blade sit toward the spine.
//! - Thickness thins with the width at the point, so the point is a point rather than a
//!   chisel; the blade is denser at the point, the ricasso and where the point begins.

use procgen_rng::Mulberry32;

use super::hafted::smoothstep;
use crate::cond::Condition;
use crate::geom::{GridOptions, MeshBuilder, Shading, V3, stitch_grid};
use crate::sections::blade::{Blade, BladeSection};

/// Nicks along the edges and a snapped point, from the item's [`Condition`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BladeDamage {
    /// 0..1: how many nicks, and how deep.
    pub nicks: f32,
    /// The point has snapped off.
    pub broken: bool,
    /// Places the nicks and the break.
    pub seed: u32,
}

impl BladeDamage {
    pub const NONE: BladeDamage = BladeDamage { nicks: 0.0, broken: false, seed: 0 };

    pub fn from_condition(cond: &Condition, seed: u32) -> Self {
        BladeDamage { nicks: cond.damage, broken: cond.broken, seed }
    }
}

/// A finished blade and the measurements a hilt, socket or haft fits it by.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BuiltBlade {
    pub mesh: MeshBuilder,
    /// How far the blade reaches from its base (less than its length if the point broke).
    pub reach: f32,
    /// Its greatest full width, edge to back.
    pub max_width: f32,
    /// Full width at the base.
    pub base_width: f32,
    /// Full thickness at the base.
    pub base_thickness: f32,
}

#[derive(Clone, Copy, Debug)]
struct Notch {
    s: f32,
    depth: f32,
    width: f32,
    front: bool,
}

/// One row across the blade at `s` (0 base, 1 tip).
#[derive(Clone, Copy, Debug)]
struct Row {
    s: f32,
    /// Centreline point in the XY plane.
    center: [f32; 2],
    /// Unit direction from the centreline toward the edge, in the XY plane.
    across: [f32; 2],
    /// Half widths toward the edge and toward the back.
    front: f32,
    back: f32,
    /// Half thickness at the thickest point of the section.
    half_thick: f32,
}

/// The outline and section functions of one blade (the prototype's `prof` and `g`).
struct Shape<'a> {
    b: &'a Blade,
    length: f32,
    half_width: f32,
    ricasso: f32,
    /// Length (as a fraction) over which the ricasso blends into the sharpened blade.
    ricasso_blend: f32,
    point_start: f32,
    belly: f32,
    notches: Vec<Notch>,
    reach: f32,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl<'a> Shape<'a> {
    fn new(b: &'a Blade, damage: &BladeDamage) -> Self {
        let length = b.length.max(0.05);
        let ricasso = b.ricasso.clamp(0.0, 0.6);
        let point_start = b.point_start.clamp(0.05, 0.995);
        let belly = b.belly.clamp(0.001, point_start - 0.001);
        let mut rng = Mulberry32::new(damage.seed.wrapping_mul(89).wrapping_add(13));
        let nicks = damage.nicks.clamp(0.0, 1.0);
        let notches = (0..(nicks * 16.0).round() as usize)
            .map(|_| {
                let s = 0.08 + rng.next_f64() as f32 * 0.86;
                let depth = (0.2 + rng.next_f64() as f32 * 0.8) * nicks * 0.32;
                let width = 0.008 + rng.next_f64() as f32 * 0.025;
                let front = b.is_single_edged() || rng.chance(0.55);
                Notch { s, depth, width, front }
            })
            .collect();
        let reach = if damage.broken { 0.55 + rng.next_f64() as f32 * 0.25 } else { 1.0 };
        Shape {
            b,
            length,
            half_width: b.width.max(0.01) * 0.5,
            ricasso,
            ricasso_blend: (0.04 / length).min(ricasso * 0.5),
            point_start,
            belly,
            notches,
            reach,
        }
    }

    /// The blade's width (as a multiple of `width`) along the body, before the point.
    fn body(&self, x: f32) -> f32 {
        let b = self.b;
        if x <= self.belly {
            lerp(b.shoulder, b.belly_width, smoothstep(0.0, self.belly, x))
        } else {
            lerp(b.belly_width, 1.0 - b.taper, smoothstep(self.belly, self.point_start, x))
        }
    }

    fn nick(&self, s: f32, front: bool) -> f32 {
        self.notches.iter().filter(|n| n.front == front).map(|n| n.depth * (1.0 - (s - n.s).abs() / n.width).max(0.0)).sum()
    }

    /// Distance along the sharpened blade, past the ricasso.
    fn sharpened(&self, s: f32) -> f32 {
        if s < self.ricasso { 0.0 } else { (s - self.ricasso) / (1.0 - self.ricasso) }
    }

    fn row(&self, s: f32) -> Row {
        use std::f32::consts::TAU;
        let b = self.b;
        let se = self.sharpened(s);
        let ps = self.point_start;
        let (mut w, u) = if se > ps {
            let u = (se - ps) / (1.0 - ps);
            let pf = b.point_fullness.clamp(0.0, 1.0);
            (self.body(ps) * ((1.0 - u) * (1.0 - pf) + (1.0 - u * u).max(0.0).sqrt() * pf), u)
        } else {
            (self.body(se), 0.0)
        };
        let tip = s >= 1.0 - 1e-6;
        w = if tip { 0.0 } else { w.max(0.01) };
        let ct = if se < b.clip { 0.0 } else { ((se - b.clip) / (1.0 - b.clip).max(0.02)).powf(0.85) };
        let spine = b.spine.clamp(0.0, 1.0);
        let back = if tip { 0.0 } else { lerp(w, b.shoulder * (1.0 - ct).max(0.0), spine).max(0.0) };
        // The undamaged widths set the thickness, so a nick does not thin the blade.
        let (clean_front, clean_back) = (self.half_width * w, self.half_width * back);
        let mut front = clean_front * (1.0 - self.nick(s, true)).max(0.2);
        let mut back = clean_back * (1.0 - self.nick(s, false)).max(0.2);
        if self.reach < 1.0 {
            // A ragged break: the edge falls away over the last stretch, jagged only there.
            let k = ((self.reach - s) / 0.06).clamp(0.0, 1.0);
            front *= k * if k < 1.0 { 0.8 + 0.2 * (s * 820.0).sin() } else { 1.0 };
            back *= ((self.reach - 0.02 - s) / 0.03).clamp(0.0, 1.0);
        }

        // The centreline bends back (edge outside) and snakes with the flame waves.
        let l = self.length;
        let cx = -b.curve * l * 0.16 * s * s;
        let tangent = normalize2([-b.curve * l * 0.32 * s, l]);
        let across = [tangent[1], -tangent[0]];
        let wave = if b.waves > 0 && b.wave_depth > 0.0 {
            let envelope = if se > ps { 1.0 - u } else { smoothstep(0.0, 0.06, se) };
            b.wave_depth * (se * b.waves as f32 * TAU).sin() * envelope * self.half_width
        } else {
            0.0
        };
        let center = [cx + across[0] * wave, s * l + across[1] * wave];
        let half_thick = (b.thickness * 0.5 * (1.0 - 0.6 * s)).min(0.375 * (clean_front + clean_back));
        Row { s, center, across, front, back, half_thick }
    }

    /// Thickness across the blade at `v` (-1 back, 0 centre, 1 edge), as a fraction of the
    /// row's half thickness.
    fn section(&self, v: f32, s: f32) -> f32 {
        let b = self.b;
        let a = v.abs().min(1.0);
        let mut z = b.section.thickness_at(v);
        if v < 0.0 && b.spine > 0.0 {
            z += b.spine * 0.55 * a;
        }
        if self.ricasso > 0.0 {
            z += 0.35 * a * (1.0 - smoothstep(self.ricasso - self.ricasso_blend, self.ricasso, s));
        }
        if b.fuller > 0.0 {
            let fade = smoothstep(0.0, 0.04, s) * (1.0 - smoothstep(b.fuller_length - 0.06, b.fuller_length, s));
            let n = b.fullers.max(1);
            let half = (b.fuller_width * 0.5).max(0.01);
            let groove: f32 = (0..n)
                .map(|k| {
                    let c = if n == 1 { 0.0 } else { (k as f32 / (n - 1) as f32 - 0.5) * 0.75 } - 0.35 * b.spine.clamp(0.0, 1.0);
                    (-((v - c) / half).powi(2)).exp()
                })
                .sum();
            z = (z * 0.35).max(z - b.fuller * 0.6 * fade * groove.min(1.0));
        }
        z
    }

    fn point(&self, row: &Row, v: f32, side: f32) -> V3 {
        let off = if v < 0.0 { v * row.back } else { v * row.front };
        [row.center[0] + row.across[0] * off, row.center[1] + row.across[1] * off, side * row.half_thick * self.section(v, row.s)]
    }
}

fn normalize2(v: [f32; 2]) -> [f32; 2] {
    let l = v[0].hypot(v[1]);
    if l < 1e-9 { [0.0, 1.0] } else { [v[0] / l, v[1] / l] }
}

/// The columns across the blade, back edge (-1) to front edge (+1), each flagged when it is a
/// hard crease (a ridge or bevel line).
fn columns(b: &Blade) -> Vec<(f32, bool)> {
    // Interior folds on one half (the edges are always strip boundaries).
    let creases: Vec<f32> = b.section.creases().iter().copied().filter(|&c| (0.0..1.0).contains(&c)).collect();
    let segments = if b.fuller > 0.0 {
        30
    } else if b.section == BladeSection::Lenticular {
        16
    } else {
        6
    };
    // Distances from the centre, 0..=1, for one half; a lens is denser toward its edges.
    let mut half: Vec<f32> = (0..=segments)
        .map(|i| {
            let u = i as f32 / segments as f32;
            if b.section == BladeSection::Lenticular { 0.6 * u + 0.4 * (u * std::f32::consts::FRAC_PI_2).sin() } else { u }
        })
        .collect();
    half.extend(creases.iter().copied());
    half.sort_by(f32::total_cmp);
    half.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    let hard = |a: f32| creases.iter().any(|c| (c - a).abs() < 1e-4);
    let mut out: Vec<(f32, bool)> = half.iter().rev().filter(|&&a| a > 0.0).map(|&a| (-a, hard(a))).collect();
    out.extend(half.iter().map(|&a| (a, hard(a))));
    out
}

/// The `s` value of every row: evenly spaced, with extra rows at the point, where the point
/// begins and round the end of the ricasso.
fn rows(shape: &Shape) -> Vec<f32> {
    let b = shape.b;
    let n = (50.0 + 14.0 * shape.length + 12.0 * b.waves as f32).round().clamp(60.0, 300.0) as usize;
    let reach = shape.reach;
    let mut s: Vec<f32> = (0..=n).map(|i| reach * i as f32 / n as f32).collect();
    let step = reach / n as f32;
    for k in [0.5, 0.25, 0.12, 0.05] {
        s.push(reach - step * k);
    }
    let r = shape.ricasso;
    if r > 0.0 {
        s.push(r - shape.ricasso_blend);
        s.push(r);
    }
    s.push(r + shape.point_start * (1.0 - r));
    s.retain(|&x| (0.0..=reach).contains(&x));
    s.sort_by(f32::total_cmp);
    s.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
    s
}

/// Builds `blade` with any `damage`, base at the origin, running +Y, edge toward +X.
pub fn blade_mesh(blade: &Blade, damage: &BladeDamage) -> BuiltBlade {
    let shape = Shape::new(blade, damage);
    let rows: Vec<Row> = rows(&shape).into_iter().map(|s| shape.row(s)).collect();
    let cols = columns(blade);
    let grid = |side: f32, from: usize, to: usize| -> Vec<Vec<V3>> {
        rows.iter()
            .map(|row| {
                let mut line: Vec<V3> = cols[from..=to].iter().map(|&(v, _)| shape.point(row, v, side)).collect();
                // The underside runs the other way so it faces -Z.
                if side < 0.0 {
                    line.reverse();
                }
                line
            })
            .collect()
    };
    let smooth = GridOptions { shading: Shading::SMOOTH, ..Default::default() };
    let mut mesh = MeshBuilder::new();
    // Each flat, split into strips at the hard creases.
    let mut start = 0;
    for end in 1..cols.len() {
        if cols[end].1 || end == cols.len() - 1 {
            for side in [1.0, -1.0] {
                mesh.append(&stitch_grid(&grid(side, start, end), &smooth));
            }
            start = end;
        }
    }
    // Walls where an edge or the spine has thickness (the spine, the ricasso, a broken end).
    let last = cols.len() - 1;
    let wall = |a: (f32, usize), b: (f32, usize)| -> Vec<Vec<V3>> {
        rows.iter().map(|row| vec![shape.point(row, cols[a.1].0, a.0), shape.point(row, cols[b.1].0, b.0)]).collect()
    };
    let faceted = GridOptions { shading: Shading::FACETED, ..Default::default() };
    mesh.append(&stitch_grid(&wall((1.0, last), (-1.0, last)), &faceted));
    mesh.append(&stitch_grid(&wall((-1.0, 0), (1.0, 0)), &faceted));
    // Base: the section at s = 0, closed flat.
    let base = &rows[0];
    let mut ring: Vec<V3> = cols.iter().map(|&(v, _)| shape.point(base, v, 1.0)).collect();
    ring.extend(cols.iter().rev().map(|&(v, _)| shape.point(base, v, -1.0)));
    ring.dedup_by(|a, b| crate::geom::vec3::distance(*a, *b) < 1e-6);
    mesh.push_fan_cap(&ring, [0.0, -1.0, 0.0]);

    let max_width = rows.iter().map(|r| r.front + r.back).fold(0.0, f32::max);
    let base_thickness = 2.0 * base.half_thick * cols.iter().map(|&(v, _)| shape.section(v, 0.0)).fold(0.0, f32::max);
    BuiltBlade { mesh, reach: shape.reach * shape.length, max_width, base_width: base.front + base.back, base_thickness }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    #[test]
    fn every_section_builds_to_its_length() {
        for section in BladeSection::ALL {
            for (spine, curve, waves, ricasso, fuller) in [(0.0, 0.0, 0, 0.0, 0.0), (1.0, 0.6, 0, 0.1, 0.5), (0.0, 0.0, 9, 0.12, 0.6)] {
                let blade = Blade { section, spine, curve, waves, wave_depth: 0.15, ricasso, fuller, fullers: 2, ..Blade::default() };
                let built = blade_mesh(&blade, &BladeDamage::NONE);
                assert!((built.reach - blade.length).abs() < 1e-4);
                assert!(built.max_width > blade.width * 0.9 && built.base_width > 0.0 && built.base_thickness > 0.0);
                let (lo, hi) = built.mesh.bounds().unwrap();
                assert!(lo[1].abs() < 1e-4 && hi[1] > blade.length * 0.97 && hi[1] < blade.length + 1e-3, "{section:?}: spans {lo:?}..{hi:?}");
                built.mesh.into_part("blade", OutlineMode::Welded).validate().unwrap();
            }
        }
    }

    #[test]
    fn a_broken_blade_is_shorter() {
        let blade = Blade::default();
        let built = blade_mesh(&blade, &BladeDamage { nicks: 0.8, broken: true, seed: 3 });
        assert!(built.reach < blade.length * 0.81);
        built.mesh.into_part("blade", OutlineMode::Welded).validate().unwrap();
    }
}
