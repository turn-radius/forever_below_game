//! Trousers, breeches and leggings on a [`LowerBodyForm`]: two legs of cloth seamed down the
//! front and back, a waistband with its closure, turned cuffs, knee bands, puttees and knee
//! patches. Built in the form's space (soles at y = 0, front +Z), so `fb_body` can pass a
//! folk body's own form.
//!
//! **Drape.** The cloth stands off the body by an ease that grows toward a straight hem or
//! closes in toward a tapered one. Breeches balloon over the thigh and gather into the knee
//! band. Folds are the garment drape's uneven vertical folds ([`Drape::fold_wave`]) plus
//! horizontal breaks where cloth stacks above the hem and creases behind the knee. Puttees
//! bind the leg in and the cloth blouses out above them.
//!
//! The prototype's plate draws legwear as a tube swept down each leg at a fixed offset; here
//! the legs join into a seat, and every sheet has thickness.

use std::f32::consts::{PI, TAU};

use procgen_rng::Mulberry32;

use crate::build::form::{FRONT, LowerBodyForm, LowerBodyProfile, ShellOptions, patch, rolled_edge, wound_band};
use crate::build::garment::Drape;
use crate::build::hafted::smoothstep;
use crate::build::parts::{Look, Parts};
use crate::build::strap::buckle;
use crate::fabric::Fabric;
use crate::geom::vec3::{self, V3};
use crate::geom::{CatmullRom, MeshBuilder, Shading, SweepOptions, cylinder, tube, uv_sphere};
use crate::material::MaterialKind;
use crate::sections::hem::Hem;
use crate::sections::legwear::{Legwear, LegwearStyle, WaistClosure};

/// Where the cloth of a pair of legs lies: its heights and how far it stands off the form.
#[derive(Clone, Debug)]
pub struct LegDrape {
    pub profile: LowerBodyProfile,
    pub wear: Legwear,
    /// Hem height at the front (the back hangs a little lower on trousers).
    pub hem: f32,
    /// Top of the waistband.
    pub top: f32,
    /// Top of the puttees, when wound.
    pub puttee_top: Option<f32>,
    drape: Drape,
}

impl LegDrape {
    pub fn new(form: &LowerBodyForm, wear: &Legwear, seed: u32) -> LegDrape {
        let profile = form.profile();
        let knee = form.knee_height();
        let hem = match wear.style {
            LegwearStyle::Trousers => 0.3,
            LegwearStyle::Leggings => 0.4,
            LegwearStyle::Breeches => knee - 0.55,
        };
        let top = form.waist_height + 0.15;
        let puttee_top = wear.puttees.then_some(match wear.style {
            LegwearStyle::Breeches => hem + 0.2,
            _ => knee - 0.35,
        });
        let mut rng = Mulberry32::new(seed ^ 0x5bd1_e995);
        let fold_count = rng.int(5, 8);
        let phases = [rng.range_f32(0.0, TAU), rng.range_f32(0.0, TAU), rng.range_f32(0.0, TAU)];
        let drape = Drape { fold_count, phases, folds: 1.0, ..Drape::fitted(top, hem, 0.0) };
        LegDrape { profile, wear: *wear, hem, top, puttee_top, drape }
    }

    /// The hem's height at `angle`: trousers break over the instep and hang lower behind.
    pub fn hem_at(&self, angle: f32) -> f32 {
        match self.wear.style {
            LegwearStyle::Trousers if self.puttee_top.is_none() => self.hem + 0.04 - 0.06 * angle.sin(),
            _ => self.hem,
        }
    }

    /// The ease off the form at height `y`, without folds.
    pub fn ease_at(&self, y: f32) -> f32 {
        let w = &self.wear;
        let f = &self.profile.form;
        let span = (f.crotch_height - self.hem).max(0.5);
        let t = ((f.crotch_height - y) / span).clamp(0.0, 1.0);
        let mut off = match w.style {
            LegwearStyle::Trousers => {
                let base = 0.05 + 0.2 * w.ease;
                let at_hem = base + 0.1 + 0.1 * w.ease;
                let at_hem = at_hem + (0.03 - at_hem) * w.taper;
                base + (at_hem - base) * t.powf(1.6)
            }
            LegwearStyle::Breeches => {
                let band_top = self.hem + 0.22;
                let base = 0.06 + 0.12 * w.ease;
                // Fullest over the thigh, easing back onto the hips.
                let bag = (0.08 + 0.2 * w.ease) * smoothstep(band_top, band_top + 1.2, y) * (1.0 - smoothstep(f.crotch_height - 1.0, self.top, y));
                0.035 + (base - 0.035) * smoothstep(band_top, band_top + 0.35, y) + bag
            }
            LegwearStyle::Leggings => 0.02 + 0.015 * w.ease,
        };
        if let Some(pt) = self.puttee_top {
            let tucked = smoothstep(pt + 0.45, pt, y);
            let blouse = (0.03 + 0.08 * w.ease) * (-((y - pt - 0.25) / 0.22).powi(2)).exp();
            off = off + (0.03 - off) * tucked + blouse;
        }
        off
    }

    /// The cloth's distance off the form at `angle` and height `y` on leg `side`, folds
    /// included. Folds fade out on the midline above the crotch, so both halves meet.
    pub fn offset(&self, side: f32, angle: f32, y: f32) -> f32 {
        let w = &self.wear;
        let f = &self.profile.form;
        let mut off = self.ease_at(y);
        let folds = w.folds * if w.style == LegwearStyle::Leggings { 0.35 } else { 1.0 };
        if folds <= 0.0 {
            return off;
        }
        let span = (f.crotch_height - self.hem).max(0.5);
        let t = ((f.crotch_height - y) / span).clamp(0.0, 1.0);
        let inner = 1.0 - smoothstep(-0.25, 0.3, side * angle.cos());
        let mask = 1.0 - self.profile.seat(y) * inner;
        // A leg's folds mirror the other's, phase-shifted so they do not match exactly.
        let a = if side > 0.0 { angle } else { PI - angle + 0.7 };
        let vertical = 0.035 * self.drape.fold_wave(a) * (0.25 + 0.75 * t);
        let mut f_sum = vertical;
        if w.style != LegwearStyle::Breeches {
            // Cloth stacking above the hem: diagonal breaks.
            let breaks = (y * 7.5 + 1.6 * (2.0 * a).sin() + self.drape.phases[0]).cos();
            let near_hem = smoothstep(self.hem + 1.8, self.hem + 0.15, y) * (1.0 - 0.6 * w.taper);
            let near_puttee = self.puttee_top.map_or(0.0, |pt| (-((y - pt - 0.3) / 0.35).powi(2)).exp());
            f_sum += 0.045 * (0.5 + 0.5 * breaks).powi(2) * (near_hem + near_puttee);
        }
        // Creases behind the knee.
        let knee = f.knee_height();
        let back = angle.sin().max(0.0).powi(2);
        f_sum += 0.03 * (y * 9.0 + self.drape.phases[1]).sin() * (-((y - knee) / 0.45).powi(2)).exp() * back;
        off += folds * f_sum * mask;
        off.max(0.01)
    }

    /// The cloth's outer surface on leg `side` at `angle` and height `y`, `extra` further out.
    pub fn point(&self, side: f32, angle: f32, y: f32, extra: f32) -> V3 {
        self.profile.point(side, angle, y, self.offset(side, angle, y) + extra)
    }

    /// One leg's cloth between heights given per angle by `span(angle) -> (y0, y1)`,
    /// `extra` out, as a thickened tube.
    pub fn band(&self, side: f32, rows: usize, extra: f32, thickness: f32, span: impl Fn(f32) -> (f32, f32)) -> MeshBuilder {
        patch(72, rows.max(1), &ShellOptions::tube(thickness), |u, v| {
            let a = TAU * u;
            let (y0, y1) = span(a);
            self.point(side, a, y0 + (y1 - y0) * v, extra)
        })
    }
}

/// A hole through one leg's cloth where something comes out through it (a tail through the
/// seat): an ellipse centred on `angle` round the leg and height `y`, `half_angle` radians
/// and `half_height` across. Our own; the prototype's legwear has no tails to clear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegHole {
    pub angle: f32,
    pub y: f32,
    pub half_angle: f32,
    pub half_height: f32,
}

impl LegHole {
    /// How far above and below its middle the hole reaches at `angle` (0 outside it).
    pub fn reach(&self, angle: f32) -> f32 {
        let off = ((angle - self.angle + PI).rem_euclid(TAU) - PI).abs() / self.half_angle;
        if off >= 1.0 { 0.0 } else { self.half_height * (1.0 - off * off).sqrt() }
    }
}

/// Cloth thickness of each style.
fn thickness(style: LegwearStyle) -> f32 {
    match style {
        LegwearStyle::Leggings => 0.025,
        _ => 0.035,
    }
}

/// Every part of a pair of legs on `form`: `"legs"`, `"waistband"`, `"ties"`, `"laces"`,
/// `"toggles"`, `"cuffs"`, `"trim"`, `"kneebands"`, `"buckles"`, `"puttees"` and
/// `"patches"`, as the state calls for them.
pub fn legwear_parts(form: &LowerBodyForm, wear: &Legwear, hem: &Hem, seed: u32) -> Parts {
    legwear_parts_holed(form, wear, hem, seed, [None, None])
}

/// [`legwear_parts`] with a hole through either leg's cloth (left, then right): each leg is
/// cut into the cloth below the hole and the cloth above it, meeting either side of it.
pub fn legwear_parts_holed(form: &LowerBodyForm, wear: &Legwear, hem: &Hem, seed: u32, holes: [Option<LegHole>; 2]) -> Parts {
    let mut parts = Parts::new();
    let d = LegDrape::new(form, wear, seed);
    let cloth = Look::cloth(&wear.cloth);
    let trim = (hem.on && hem.hemline).then(|| Look::cloth(&hem.fabric));
    let t = thickness(wear.style);
    let sides = [1.0f32, -1.0];

    // The legs, hem to waist.
    let rows = ((d.top - d.hem) * 9.0).round() as usize;
    let mut legs = MeshBuilder::new();
    for (side, hole) in sides.into_iter().zip(holes) {
        match hole {
            Some(h) => {
                let y = h.y.clamp(d.hem + 0.1, d.top - wear.band - 0.05);
                let split = |to: f32| (((to - d.hem) / (d.top - d.hem)) * rows as f32).round().max(2.0) as usize;
                legs.append(&d.band(side, split(y), 0.0, t, |a| (d.hem_at(a), (y - h.reach(a)).max(d.hem_at(a) + 0.02))));
                legs.append(&d.band(side, rows.saturating_sub(split(y)).max(2), 0.0, t, |a| ((y + h.reach(a)).min(d.top - 0.02), d.top)));
            }
            None => {
                legs.append(&d.band(side, rows, 0.0, t, |a| (d.hem_at(a), d.top)));
            }
        }
    }
    parts.add_look("legs", legs, cloth);

    // Waistband round both halves, and its closure at the front.
    let p = &d.profile;
    let waist_off = d.ease_at(d.top) + t + 0.025;
    let (y0, y1) = (d.top - wear.band, d.top + 0.02);
    let band = patch(96, 3, &ShellOptions::tube(0.05), |u, v| p.torso_point(TAU * u, y0 + (y1 - y0) * v, waist_off));
    parts.add_look("waistband", band, cloth);
    let front = |x: f32, y: f32, extra: f32| -> V3 {
        let c = p.torso_point(FRONT, y, waist_off + 0.05 + extra);
        [x, c[1], c[2] - 0.6 * x * x]
    };
    let ym = d.top - wear.band * 0.5;
    match wear.closure {
        WaistClosure::Drawstring => {
            let mut ties = MeshBuilder::new();
            for k in [-1.0f32, 1.0] {
                let start = front(0.09 * k, ym, 0.0);
                let path = CatmullRom::centripetal(vec![
                    start,
                    vec3::add(start, [0.07 * k, -0.15, 0.06]),
                    vec3::add(start, [0.05 * k, -0.5, 0.08]),
                    vec3::add(start, [0.1 * k + 0.03, -0.85 - 0.08 * k, 0.05]),
                ])
                .sample_evenly(24);
                let end = *path.last().unwrap();
                ties.append(&tube(&path, 6, &SweepOptions::default(), |_| 0.022));
                let mut knot = uv_sphere(0.045, 10, 6);
                knot.translate(end);
                ties.append(&knot);
            }
            let mut knot = uv_sphere(0.06, 12, 8);
            knot.scale([1.5, 1.0, 0.8]).translate(front(0.0, ym, 0.02));
            ties.append(&knot);
            parts.add("ties", ties, MaterialKind::Cord);
        }
        WaistClosure::Laces => {
            // A slit down the front from the band, laced across.
            let depth = 0.75;
            let mut zig = Vec::new();
            for i in 0..=6 {
                let y = ym - depth * i as f32 / 6.0;
                let x = if i % 2 == 0 { -0.07 } else { 0.07 };
                zig.push(front(x, y, -0.02));
            }
            let mut laces = tube(&CatmullRom::centripetal(zig.clone()).sample(6), 6, &SweepOptions::default(), |_| 0.018);
            let mirrored: Vec<V3> = zig.iter().map(|q| [-q[0], q[1], q[2]]).collect();
            laces.append(&tube(&CatmullRom::centripetal(mirrored).sample(6), 6, &SweepOptions::default(), |_| 0.018));
            let tag = |end: V3, k: f32| {
                let path = [end, vec3::add(end, [0.04 * k, -0.25, 0.04]), vec3::add(end, [0.06 * k, -0.45, 0.02])];
                tube(&CatmullRom::centripetal(path.to_vec()).sample(6), 6, &SweepOptions::default(), |_| 0.018)
            };
            laces.append(&tag(front(-0.07, ym, -0.02), -1.0));
            laces.append(&tag(front(0.07, ym, -0.02), 1.0));
            parts.add("laces", laces, MaterialKind::Leather);
        }
        WaistClosure::Toggles => {
            let mut toggles = MeshBuilder::new();
            let mut loops = MeshBuilder::new();
            for (i, y) in [ym + wear.band * 0.15, ym - wear.band * 0.15 - 0.12].into_iter().enumerate() {
                let at = front(0.06 + 0.02 * i as f32, y, 0.03);
                let mut toggle = cylinder(0.04, 0.22, 10, Shading::SMOOTH);
                toggle.rotate_z(PI / 2.0 - 0.15).translate(at);
                toggles.append(&toggle);
                let a = front(-0.12, y, 0.0);
                let path = [a, vec3::add(a, [0.1, 0.05, 0.05]), vec3::add(at, [0.06, 0.04, 0.02]), vec3::add(at, [0.06, -0.04, 0.02]), vec3::add(a, [0.1, -0.05, 0.05])];
                loops.append(&tube(&CatmullRom::centripetal(path.to_vec()).sample(6), 6, &SweepOptions::default(), |_| 0.016));
            }
            parts.add("toggles", toggles, wear.fittings);
            parts.add("ties", loops, MaterialKind::Cord);
        }
    }

    // Hems: a turned cuff, a trim band, or a breeches' knee band with its buckle.
    match wear.style {
        LegwearStyle::Breeches => {
            let mut bands = MeshBuilder::new();
            let mut buckles = MeshBuilder::new();
            for side in sides {
                bands.append(&d.band(side, 3, 0.03, 0.04, |_| (d.hem - 0.01, d.hem + 0.18)));
                let at = d.point(side, if side > 0.0 { 0.0 } else { PI }, d.hem + 0.09, 0.08);
                let mut b = buckle(0.16, 0.018);
                b.place_basis(at, vec3::Z, [side, 0.0, 0.0]);
                buckles.append(&b);
            }
            parts.add_look("kneebands", bands, trim.unwrap_or(Look::of(MaterialKind::Leather)));
            parts.add("buckles", buckles, wear.fittings);
        }
        LegwearStyle::Trousers if wear.cuff > 0.0 && !wear.puttees => {
            let mut cuffs = MeshBuilder::new();
            for side in sides {
                cuffs.append(&d.band(side, 4, 0.03, t + 0.01, |a| (d.hem_at(a) - 0.01, d.hem_at(a) + wear.cuff)));
                let edge: Vec<V3> = (0..72).map(|i| TAU * i as f32 / 72.0).map(|a| d.point(side, a, d.hem_at(a) + wear.cuff, 0.03 + t)).collect();
                cuffs.append(&rolled_edge(&edge, 0.025, true));
            }
            parts.add_look("cuffs", cuffs, trim.unwrap_or(cloth));
        }
        _ => {
            if let Some(trim) = trim.filter(|_| !wear.puttees) {
                let mut bands = MeshBuilder::new();
                for side in sides {
                    bands.append(&d.band(side, 3, 0.015, t + 0.015, |a| (d.hem_at(a) - 0.01, d.hem_at(a) + hem.width * 0.5)));
                }
                parts.add_look("trim", bands, trim);
            }
        }
    }

    // Puttees from the ankle up the shin, over the tucked cloth (or the bare shin below
    // breeches).
    if let Some(top) = d.puttee_top {
        let mut wraps = MeshBuilder::new();
        let mut edges = MeshBuilder::new();
        let bottom = 0.25;
        let width = 0.22;
        let turns = ((top - bottom) / (width * 0.9)).round().max(2.0);
        for side in sides {
            let (ribbon, edge) = wound_band(turns, bottom, top, width, 0.02, |a, y, inflate| {
                let off = if y > d.hem { d.offset(side, a, y) + t } else { 0.025 };
                p.point(side, a, y, off + inflate)
            });
            wraps.append(&ribbon);
            edges.append(&edge);
        }
        parts.add_look("puttees", wraps, Look::cloth(&Fabric::plain(wear.wrap_cloth)));
        parts.add_look("puttees", edges, Look::cloth(&Fabric::plain(wear.wrap_cloth.scaled(0.6))).without_outline());
    }

    // Leather patches over the knees.
    if wear.patches && wear.style == LegwearStyle::Trousers {
        let knee = form.knee_height();
        let mut patches = MeshBuilder::new();
        for side in sides {
            patches.append(&patch(16, 10, &ShellOptions::sheet(0.025), |u, v| {
                let y = knee - 0.38 + 0.82 * v;
                let k = (1.0 - (2.0 * v - 1.0).powi(2)).max(0.0).powf(0.35);
                let a = FRONT + (u - 0.5) * 1.3 * (0.35 + 0.65 * k);
                d.point(side, a, y, t + 0.01)
            }));
        }
        parts.add("patches", patches, MaterialKind::Leather);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cloth_stands_off_the_form() {
        let form = LowerBodyForm::mannequin();
        for style in LegwearStyle::ALL {
            let wear = Legwear { style, folds: 1.0, ..Legwear::default() };
            let d = LegDrape::new(&form, &wear, 7);
            for side in [1.0f32, -1.0] {
                for i in 0..24 {
                    let a = TAU * i as f32 / 24.0;
                    for y in [d.hem + 0.05, 3.0, 6.0] {
                        if y < d.hem {
                            continue;
                        }
                        let body = d.profile.point(side, a, y, 0.0);
                        let cloth = d.point(side, a, y, 0.0);
                        let axis = [side * d.profile.axis_at(y), y, 0.0];
                        assert!(vec3::distance(cloth, axis) > vec3::distance(body, axis), "{style:?} side {side} angle {a} y {y}");
                    }
                }
            }
        }
    }

    #[test]
    fn breeches_gather_at_the_knee_band() {
        let form = LowerBodyForm::mannequin();
        let d = LegDrape::new(&form, &Legwear { style: LegwearStyle::Breeches, ease: 0.8, ..Legwear::default() }, 1);
        assert!(d.ease_at(d.hem + 0.1) < 0.05);
        assert!(d.ease_at(6.0) > 0.2, "{}", d.ease_at(6.0));
    }
}
