//! Body armour and clothing on a [`TorsoForm`]: cloth that drapes, sleeves, trims, belts,
//! collars, hoods, and plate fittings. Everything is built in the form's space (waist at
//! y = 0, front +Z), so `fb_body` can pass a folk body's own form.
//!
//! **Drape.** No physics, a deterministic model of how cloth hangs (see [`Drape`]):
//!
//! 1. *Hanging hull.* Below its support (the shoulders) cloth cannot follow the body into a
//!    hollow: walking down each meridian, the cloth's distance from the axis may shrink by
//!    at most `tuck` per unit of drop. A loose robe falls straight from the chest; a mail
//!    shirt tucks in at the waist; plate follows the form exactly.
//! 2. *Cinch.* A belt pulls the cloth back onto the waist, the cloth bloused just above it,
//!    and the hull restarts from the belt.
//! 3. *Flare.* Below the hips the skirt widens toward the hem.
//! 4. *Folds.* Vertical folds, seeded and unevenly spaced, deepen toward the hem with
//!    rounded crests and sharp valleys, and the hem line undulates with them.
//!
//! The prototype draws a single-sided surface with one sinusoid of folds; here every sheet
//! has thickness ([`shell`]), so hems, cuffs and openings show a real edge.

use std::f32::consts::{PI, TAU};

use procgen_rng::Mulberry32;

use crate::build::form::{BACK, FRONT, LameStack, LimbForm, ShellOptions, TorsoForm, TorsoProfile, limb_lames, patch, rolled_edge, shell};
use crate::build::gem::cut_stone;
use crate::build::hafted::smoothstep;
use crate::build::parts::{Look, Parts};
use crate::fabric::{PatternKind, SurfacePattern, Wrap};
use crate::gem::{GemCut, Gemstone};
use crate::geom::vec3::{self, V3};
use crate::geom::{CatmullRom, MeshBuilder, SweepOptions, rotation_minimizing_frames, tube};
use crate::material::MaterialKind;
use crate::sections::garment::{Belt, GarmentCut, GarmentStyle, Neckline, Plates, Sleeves};
use crate::sections::hem::Hem;

/// How a sheet hangs on a [`TorsoForm`]. Heights are in the form's space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drape {
    /// Top of the sheet at the back of the neck.
    pub top: f32,
    /// How far the neckline drops at the front.
    pub neck_dip: f32,
    /// Bottom edge.
    pub hem: f32,
    /// Below this height the cloth hangs free of the form.
    pub support: f32,
    /// Distance kept off the form everywhere.
    pub ease: f32,
    /// How fast hanging cloth may draw back toward the form, per unit of drop.
    pub tuck: f32,
    /// Where the skirt starts to widen.
    pub flare_from: f32,
    pub flare: f32,
    /// Fold depth, 0..=1.
    pub folds: f32,
    pub fold_count: u32,
    /// Seeded phases that make the folds uneven.
    pub phases: [f32; 3],
    /// A belt: its height and half width.
    pub cinch: Option<(f32, f32)>,
    /// An opening down the front: its half angle at the top and at the hem, in radians.
    pub opening: Option<(f32, f32)>,
    /// Quilting puffs between stitch lines, if any.
    pub quilt: Option<Quilting>,
    /// A medial ridge down the front (plate).
    pub ridge: f32,
    /// A slit cut up from the hem, if any.
    pub notch: Option<Notch>,
}

/// A slit cut up from a hem, its sides meeting at a point (a vent at the back of a cloak, where
/// a tail passes through): centred on `angle` (lathe convention), `half_width` radians either
/// side of it at the hem, narrowing to nothing at height `top`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Notch {
    pub angle: f32,
    pub top: f32,
    pub half_width: f32,
}

/// Quilted padding: puffs between stitch lines laid out as the shader lays out the
/// [`PatternKind::Quilted`] pattern ([`Wrap`]), so the stitching it draws runs along the
/// hollows between the puffs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quilting {
    /// How far a puff stands out at its middle.
    pub depth: f32,
    /// The plain coat's wrap.
    pub wrap: Wrap,
    /// Cells per unit, fitted to the wrap so a whole number of cells goes round.
    pub scale: f32,
}

impl Quilting {
    /// Puff size, in cells per unit: about half a unit (5 cm) square.
    pub const SCALE: f32 = 2.0;
    pub const DEPTH: f32 = 0.035;

    /// Quilting for a coat whose plain (unpuffed) vertices are `positions`.
    pub fn fitted(positions: &[V3]) -> Self {
        let wrap = Wrap::of(positions);
        Quilting { depth: Self::DEPTH, wrap, scale: wrap.fitted_scale(Self::SCALE) }
    }

    /// How far the cloth puffs out at a point of the plain coat: nothing on a stitch line,
    /// `depth` in the middle of a cell.
    pub fn puff(&self, p: V3) -> f32 {
        let [u, v] = self.wrap.coords(p, self.scale);
        self.depth * ((PI * u.rem_euclid(1.0)).sin() * (PI * v.rem_euclid(1.0)).sin()).abs().sqrt()
    }

    /// The quilted pattern the shader draws over these puffs.
    pub fn pattern(&self, color: crate::material::Rgb) -> SurfacePattern {
        SurfacePattern { kind: PatternKind::Quilted, color, scale: self.scale }
    }
}

impl Drape {
    /// A rigid sheet that follows the form (plate).
    pub fn fitted(top: f32, hem: f32, ease: f32) -> Drape {
        Drape {
            top,
            neck_dip: 0.0,
            hem,
            support: top,
            ease,
            tuck: 1e3,
            flare_from: hem,
            flare: 0.0,
            folds: 0.0,
            fold_count: 9,
            phases: [0.0; 3],
            cinch: None,
            opening: None,
            quilt: None,
            ridge: 0.0,
            notch: None,
        }
    }

    /// Height of the bottom edge at `angle`: the hem, or the side of a notch cut up from it.
    pub fn hem_at(&self, angle: f32) -> f32 {
        match self.notch {
            Some(n) => {
                let off = ((angle - n.angle + PI).rem_euclid(TAU) - PI).abs();
                if off >= n.half_width {
                    return self.hem;
                }
                self.hem + (n.top - self.hem).max(0.0) * (1.0 - off / n.half_width)
            }
            None => self.hem,
        }
    }

    /// Height of the neckline's top edge at `angle`.
    pub fn top_at(&self, angle: f32) -> f32 {
        let front = (-angle.sin()).max(0.0);
        self.top - self.neck_dip * front * front
    }

    /// Angle round the form of column `u` (0..=1) at height `y`: from the back for a closed
    /// sheet, from one side of the front opening to the other for an open one.
    pub fn angle(&self, u: f32, y: f32) -> f32 {
        match self.opening {
            None => BACK + u * TAU,
            Some((gap_top, gap_hem)) => {
                let t = ((self.top - y) / (self.top - self.hem).max(1e-3)).clamp(0.0, 1.0);
                let gap = gap_top + (gap_hem - gap_top) * t.powf(1.3);
                FRONT + gap + u * (TAU - 2.0 * gap)
            }
        }
    }

    /// The folds' shape round the form, -0.4..=1: broad crests, sharp valleys.
    pub fn fold_wave(&self, angle: f32) -> f32 {
        let [p0, p1, p2] = self.phases;
        let x = self.fold_count as f32 * angle + p0 + 0.9 * (3.0 * angle + p1).sin() + 0.5 * (2.0 * angle + p2).sin();
        let w = (0.5 + 0.5 * x.cos()).powf(0.6);
        w * 1.4 - 0.4
    }

    /// Depth of the folds at height `y`.
    pub fn fold_depth(&self, y: f32) -> f32 {
        let skirt = ((self.flare_from - y) / (self.flare_from - self.hem).max(0.3)).clamp(0.0, 1.0);
        let torso = smoothstep(self.support, self.flare_from, y) * 0.25;
        self.folds * (0.04 * torso + 0.3 * skirt.powf(0.9) * (0.6 + self.flare))
    }

    /// The hem's rise and fall with the folds, faded out above it.
    fn hem_wave(&self, angle: f32, y: f32) -> f32 {
        0.2 * self.folds * self.fold_wave(angle) * smoothstep(self.hem + 1.0, self.hem, y)
    }
}

/// A [`Drape`] laid over a form, ready to sample.
#[derive(Clone, Debug)]
pub struct DrapedSurface {
    pub profile: TorsoProfile,
    pub drape: Drape,
    /// Hull clearance over the form, `[height][column]`.
    hull: Vec<Vec<f32>>,
    y_max: f32,
    /// Hung over an underlay as well as the form ([`DrapedSurface::over`]): the skirt does not
    /// flare out from it.
    underlaid: bool,
}

const HULL_COLUMNS: usize = 128;
const HULL_STEP: f32 = 0.05;

/// How fast cloth hanging over an underlay (a cloak over the arms) draws back toward the form
/// below what holds it out, per unit of drop. Our own figure: about the slope a cloak falls
/// at from the hands toward the shins, so its hem comes back in toward the legs.
pub const UNDERLAY_TUCK: f32 = 0.15;

/// What a garment hangs over besides its form, on a real body: the least distance from the
/// form's axis its cloth's inner face must keep toward each angle (lathe convention) at each
/// height in the form's space, such as a body's arms and whatever is worn on them.
pub type Underlay<'a> = &'a dyn Fn(f32, f32) -> f32;

impl DrapedSurface {
    pub fn new(form: &TorsoForm, drape: Drape) -> DrapedSurface {
        Self::over(form, drape, None, 0.0)
    }

    /// The drape over `form` and, when given, over `under` too, its cloth `thickness` thick.
    ///
    /// Over an underlay, cloth spans across each row as it falls down each meridian: in every
    /// horizontal slice it takes the convex hull of what it rests on, so it bridges from the
    /// trunk to the arms instead of dipping between them, and its fold valleys keep clear of
    /// the underlay. Below what holds it out it draws back toward the form at no less than
    /// [`UNDERLAY_TUCK`]. Our own model, in the spirit of the hanging hull.
    pub fn over(form: &TorsoForm, drape: Drape, under: Option<Underlay>, thickness: f32) -> DrapedSurface {
        let profile = form.profile();
        let y_max = drape.top + 0.5;
        let rows = ((y_max - (drape.hem - 1.5)) / HULL_STEP).ceil() as usize + 1;
        let radial = |a: f32, y: f32| {
            let p = profile.point(a, y);
            p[0].hypot(p[2])
        };
        let angle = |j: usize| TAU * j as f32 / HULL_COLUMNS as f32;
        // How far out the cloth must stand in each row before it hangs: off the form by its
        // ease, and clear of the underlay, spanned convexly across the row.
        let need: Vec<Vec<f32>> = (0..rows)
            .map(|i| {
                let y = y_max - i as f32 * HULL_STEP;
                let mut row: Vec<f32> = (0..HULL_COLUMNS).map(|j| radial(angle(j), y) + drape.ease).collect();
                if let Some(under) = under {
                    // A column hangs out along its form point's direction from the axis, which
                    // a superelliptic section turns away from the column's own angle.
                    let dirs: Vec<[f32; 2]> = (0..HULL_COLUMNS)
                        .map(|j| {
                            let p = profile.point(angle(j), y);
                            let l = p[0].hypot(p[2]).max(1e-6);
                            [p[0] / l, p[2] / l]
                        })
                        .collect();
                    let valley = 0.4 * drape.fold_depth(y);
                    for (r, d) in row.iter_mut().zip(&dirs) {
                        *r = r.max(under((-d[1]).atan2(d[0]), y) + thickness + valley);
                    }
                    row = convex_row(&row, &dirs);
                }
                row
            })
            .collect();
        let tuck = if under.is_some() { drape.tuck.max(UNDERLAY_TUCK) } else { drape.tuck };
        let mut hull = vec![vec![drape.ease; HULL_COLUMNS]; rows];
        for j in 0..HULL_COLUMNS {
            let a = angle(j);
            let mut prev = need[0][j];
            for (i, row) in hull.iter_mut().enumerate() {
                let y = y_max - i as f32 * HULL_STEP;
                let body = radial(a, y);
                let mut total = need[i][j];
                if y < drape.support {
                    total = total.max(prev - tuck * HULL_STEP);
                }
                if let Some((cy, cw)) = drape.cinch {
                    if (y - cy).abs() <= cw {
                        total = body + drape.ease * 0.6;
                    } else if y > cy + cw && y < cy + cw + 0.5 {
                        // Cloth bloused over the belt.
                        total += 0.025 * (PI * (y - cy - cw) / 0.5).sin();
                    }
                }
                row[j] = total - body;
                prev = total;
            }
        }
        DrapedSurface { profile, drape, hull, y_max, underlaid: under.is_some() }
    }

    fn clearance(&self, angle: f32, y: f32) -> f32 {
        let fj = angle.rem_euclid(TAU) / TAU * HULL_COLUMNS as f32;
        let (j0, tj) = (fj.floor() as usize % HULL_COLUMNS, fj.fract());
        let j1 = (j0 + 1) % HULL_COLUMNS;
        let fi = ((self.y_max - y) / HULL_STEP).clamp(0.0, (self.hull.len() - 1) as f32);
        let (i0, ti) = (fi.floor() as usize, fi.fract());
        let i1 = (i0 + 1).min(self.hull.len() - 1);
        let row = |i: usize| self.hull[i][j0] * (1.0 - tj) + self.hull[i][j1] * tj;
        row(i0) * (1.0 - ti) + row(i1) * ti
    }

    /// The cloth's outer surface at `angle` and height `y`, `offset` further out.
    pub fn point_at(&self, angle: f32, y: f32, offset: f32) -> V3 {
        let d = &self.drape;
        let y = y + d.hem_wave(angle, y);
        let p = self.profile.point(angle, y);
        let body = p[0].hypot(p[2]).max(1e-3);
        let dir = [p[0] / body, p[2] / body];
        let mut r = body + self.clearance(angle, y);
        let skirt = ((d.flare_from - y) / (d.flare_from - d.hem).max(0.3)).clamp(0.0, 1.0);
        // Over an underlay the skirt's fullness falls into its folds round what it hangs over
        // rather than spreading out from it.
        if !self.underlaid {
            r *= 1.0 + d.flare * 0.9 * skirt.powf(1.3);
        }
        r += d.fold_depth(y) * d.fold_wave(angle);
        if let Some(quilt) = &d.quilt {
            r += quilt.puff([dir[0] * r, y, dir[1] * r]);
        }
        if d.ridge > 0.0 {
            let off = (angle - FRONT) / 0.28;
            let span = smoothstep(-0.3, 0.4, y) * (1.0 - smoothstep(2.4, 3.4, y));
            r += d.ridge * 0.1 * (-off * off).exp() * span;
        }
        r += offset;
        [dir[0] * r, y, dir[1] * r]
    }

    /// Column `u` (0..=1, see [`Drape::angle`]) at height `y`.
    pub fn point(&self, u: f32, y: f32, offset: f32) -> V3 {
        self.point_at(self.drape.angle(u, y), y, offset)
    }

    /// The cloth between heights given per column by `y0(angle)` and `y1(angle)` (or the
    /// columns `u0..=u1` of it), `offset` out, as a thickened band.
    pub fn band(&self, u0: f32, u1: f32, rows: usize, offset: f32, thickness: f32, span: impl Fn(f32) -> (f32, f32)) -> MeshBuilder {
        let closed = self.drape.opening.is_none() && u0 <= 0.0 && u1 >= 1.0;
        let cols = ((u1 - u0) * 96.0).round().max(4.0) as usize;
        let options = ShellOptions { wrap_around: closed, thickness, ..ShellOptions::sheet(thickness) };
        patch(cols, rows, &options, |u, v| {
            let uu = u0 + (u1 - u0) * u;
            // The span is read at the angle the column has at the band's middle.
            let (y0, y1) = span(self.drape.angle(uu, self.drape.top));
            self.point(uu, y0 + (y1 - y0) * v, offset)
        })
    }
}

/// The convex hull of a row of radii from an axis along unit directions `dirs` (x, z),
/// as radii along the same directions: what cloth stretched round the row rests on.
fn convex_row(radii: &[f32], dirs: &[[f32; 2]]) -> Vec<f32> {
    let n = radii.len();
    let dir = |j: usize| dirs[j];
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    // Andrew's monotone chain over the row's points.
    let mut pts: Vec<[f32; 2]> = (0..n).map(|j| dir(j).map(|c| c * radii[j])).collect();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    let chain = |points: &mut dyn Iterator<Item = [f32; 2]>| {
        let mut out: Vec<[f32; 2]> = Vec::with_capacity(n);
        for p in points {
            while out.len() >= 2 && cross(out[out.len() - 2], out[out.len() - 1], p) <= 0.0 {
                out.pop();
            }
            out.push(p);
        }
        out.pop();
        out
    };
    let mut hull = chain(&mut pts.iter().copied());
    hull.extend(chain(&mut pts.iter().rev().copied()));
    // Each column's ray from the axis leaves the hull through one edge.
    (0..n)
        .map(|j| {
            let d = dir(j);
            let mut best = radii[j];
            for k in 0..hull.len() {
                let (p, q) = (hull[k], hull[(k + 1) % hull.len()]);
                let e = [q[0] - p[0], q[1] - p[1]];
                let den = d[0] * e[1] - d[1] * e[0];
                if den.abs() < 1e-9 {
                    continue;
                }
                let t = (p[0] * e[1] - p[1] * e[0]) / den;
                let s = (p[0] * d[1] - p[1] * d[0]) / den;
                if t > 0.0 && (-1e-4..=1.0 + 1e-4).contains(&s) {
                    best = best.max(t);
                }
            }
            best
        })
        .collect()
}

/// A sleeve hanging from the shoulder, as rings round a path.
pub struct SleeveForm {
    rows: Vec<Vec<V3>>,
}

impl SleeveForm {
    /// `fit` scales its girth, `flare` bells it toward the cuff (hanging lower underneath),
    /// `folds` creases it. `side` is +1 for the left arm, -1 for the right.
    pub fn new(form: &TorsoForm, garment: &GarmentCut, side: f32, offset: f32, phase: f32) -> SleeveForm {
        let length = garment.sleeves.length();
        let (root, dir) = form.arm_root(side, garment.arm_angle.to_radians());
        let start = [side * form.shoulder.half_width * 0.7, form.shoulder.y - 0.35, 0.0];
        let path = CatmullRom::centripetal(vec![start, root, vec3::add(root, vec3::scale(dir, length * 0.5)), vec3::add(root, vec3::scale(dir, length))])
            .sample_evenly(40);
        let bell = garment.sleeve_flare * 1.5 * if length > 3.0 { 1.0 } else { 0.5 };
        Self::swept(&path, garment, offset, phase, |t| 0.68 * garment.fit * (1.0 - 0.3 * t) * (1.0 + bell * smoothstep(0.5, 1.0, t)))
    }

    /// A sleeve on a real arm (`fb_body`'s limbs): from the root inside the trunk down to the
    /// elbow for short sleeves or the wrist for long ones, standing `ease` off the arm's own
    /// radius and scaled by the garment's fit.
    pub fn on_arm(arm: &LimbForm, garment: &GarmentCut, ease: f32, offset: f32, phase: f32) -> SleeveForm {
        let [ts, te] = arm.t_joints;
        let end = match garment.sleeves {
            Sleeves::Long => 0.975,
            _ => ts + (te - ts) * 0.55,
        };
        // From halfway between the root and the shoulder, still inside the garment's body.
        let start = 0.5 * ts;
        let path = arm.span(start, end, 40);
        let long = garment.sleeves == Sleeves::Long;
        let bell = garment.sleeve_flare * if long { 1.0 } else { 0.5 };
        Self::swept(&path, garment, offset, phase, |t| {
            let r = arm.radius_at(start + t * (end - start));
            (r + ease) * garment.fit + r * bell * smoothstep(0.5, 1.0, t)
        })
    }

    /// Rings round `path` of radius `base(t)`, belled and creased by the garment.
    fn swept(path: &[V3], garment: &GarmentCut, offset: f32, phase: f32, base: impl Fn(f32) -> f32) -> SleeveForm {
        let frames = rotation_minimizing_frames(path, Some(vec3::Z), false);
        let rows = frames
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let t = i as f32 / (frames.len() - 1) as f32;
                let down = vec3::normalize_or(vec3::sub(vec3::neg(vec3::Y), vec3::scale(f.tangent, -f.tangent[1])), vec3::neg(vec3::Y));
                let base = base(t);
                (0..24)
                    .map(|k| {
                        let a = TAU * k as f32 / 24.0;
                        let d = vec3::add(vec3::scale(f.normal, a.cos()), vec3::scale(f.binormal, a.sin()));
                        let hang = vec3::dot(d, down).max(0.0);
                        let r = base * (1.0 + 0.6 * garment.sleeve_flare * hang * hang * smoothstep(0.4, 1.0, t))
                            + 0.04 * garment.folds * t * (5.0 * a + phase).cos()
                            + offset;
                        vec3::add(f.origin, vec3::scale(d, r))
                    })
                    .collect()
            })
            .collect();
        SleeveForm { rows }
    }

    pub fn mesh(&self, thickness: f32) -> MeshBuilder {
        shell(&self.rows, &ShellOptions::tube(thickness))
    }

    /// The last `fraction` of the sleeve (a cuff band).
    pub fn cuff(&self, fraction: f32, thickness: f32) -> MeshBuilder {
        let n = self.rows.len();
        let from = ((1.0 - fraction.clamp(0.05, 1.0)) * (n - 1) as f32).floor() as usize;
        shell(&self.rows[from.min(n - 2)..], &ShellOptions::tube(thickness))
    }
}

/// Cloth thickness, ease, tuck and how much the style folds.
fn style_drape(style: GarmentStyle) -> (f32, f32, f32, f32) {
    use GarmentStyle::*;
    match style {
        Tunic => (0.035, 0.06, 0.15, 1.0),
        Robe => (0.04, 0.08, 0.06, 1.0),
        Gambeson => (0.09, 0.12, 0.5, 0.4),
        Hauberk => (0.045, 0.05, 0.35, 0.6),
        Brigandine => (0.07, 0.1, 0.6, 0.15),
        Scale => (0.06, 0.07, 0.5, 0.35),
        Cuirass => (0.06, 0.08, 1e3, 0.0),
        Cloak => (0.035, 0.14, 0.04, 1.0),
    }
}

/// The drape a garment's state describes on `form` (already eased by the fit).
pub fn garment_drape(form: &TorsoForm, g: &GarmentCut, belt: &Belt, plates: &Plates, seed: u32) -> Drape {
    let mut rng = Mulberry32::new(seed ^ 0x9e37_79b9);
    let (_, ease, tuck, folding) = style_drape(g.style);
    let rigid = g.style == GarmentStyle::Cuirass;
    let cloak = g.style.is_open();
    let hem = if rigid { form.waist.y - 0.15 } else { form.waist.y - g.length };
    let neck_dip = match (g.style, g.neck) {
        (GarmentStyle::Cuirass, _) => 0.2,
        (_, Neckline::High) => 0.05,
        (_, Neckline::Cowl) => 0.25,
        (GarmentStyle::Cloak, _) => 0.1,
        _ => 0.35,
    };
    let fold_count = rng.int(6, 10);
    let phases = [rng.range_f32(0.0, TAU), rng.range_f32(0.0, TAU), rng.range_f32(0.0, TAU)];
    Drape {
        top: form.neck.y,
        neck_dip,
        hem,
        support: if cloak { form.shoulder.y } else { form.shoulder.y - 0.3 },
        ease,
        tuck,
        flare_from: if cloak { form.shoulder.y } else { form.hip.y },
        flare: if rigid { 0.0 } else { g.flare },
        folds: g.folds * folding,
        fold_count,
        phases,
        cinch: (belt.on && !cloak && !rigid).then_some((form.waist.y, belt.width * 0.5 + 0.05)),
        opening: cloak.then_some((0.07, 1.07)),
        // Set once the plain coat is built, so the puffs line up with the pattern.
        quilt: None,
        ridge: if rigid { plates.ridge } else { 0.0 },
        notch: None,
    }
}

/// The look of the garment's body: cloth with its pattern, mail, scales or plate.
pub fn body_look(g: &GarmentCut, plates: &Plates) -> Look {
    match g.style {
        GarmentStyle::Hauberk => {
            Look::patterned(g.material, SurfacePattern { kind: PatternKind::Mail, color: g.cloth.color2, scale: plates.ring_size })
        }
        GarmentStyle::Scale => {
            Look::patterned(g.material, SurfacePattern { kind: PatternKind::Scales, color: g.cloth.color2, scale: plates.ring_size * 0.5 })
        }
        GarmentStyle::Cuirass => Look::of(g.material),
        GarmentStyle::Gambeson if g.cloth.pattern.kind().is_none() => {
            // Fitted to the coat in `garment_parts`.
            Look { pattern: Some(SurfacePattern { kind: PatternKind::Quilted, color: g.cloth.color, scale: Quilting::SCALE }), ..Look::cloth(&g.cloth) }
        }
        _ => Look::cloth(&g.cloth),
    }
}

/// Every part of a garment on `form` (waist at y = 0): `"body"`, `"sleeves"`, `"trim"`,
/// `"collar"`, `"cowl"`, `"hood"`, `"belt"`, `"buckle"`, `"faulds"`, `"pauldrons"`,
/// `"edges"`, `"brooch"` and `"stone"`, as the state calls for them.
pub fn garment_parts(form: &TorsoForm, g: &GarmentCut, hem: &Hem, belt: &Belt, plates: &Plates, seed: u32) -> Parts {
    build_garment(form, &Fitting::default(), g, hem, belt, plates, seed)
}

/// [`garment_parts`] on a real body: sleeves and pauldrons follow `arms` (right then left,
/// in the form's space) instead of the mannequin's arm roots.
pub fn garment_parts_on_arms(form: &TorsoForm, arms: &[LimbForm; 2], g: &GarmentCut, hem: &Hem, belt: &Belt, plates: &Plates, seed: u32) -> Parts {
    build_garment(form, &Fitting { arms: Some(arms), ..Fitting::default() }, g, hem, belt, plates, seed)
}

/// How a garment sits on a real body beyond its form.
#[derive(Clone, Copy, Default)]
pub struct Fitting<'a> {
    /// Sleeves and pauldrons follow these arms (right then left, in the form's space) instead
    /// of the mannequin's arm roots.
    pub arms: Option<&'a [LimbForm; 2]>,
    /// What the cloth hangs over besides the form ([`DrapedSurface::over`]): a cloak over the
    /// arms and whatever is on them.
    pub over: Option<Underlay<'a>>,
    /// A slit cut up from the hem (where a tail passes through).
    pub notch: Option<Notch>,
}

/// [`garment_parts`] fitted to a real body as `fitting` describes.
pub fn garment_parts_fitted(form: &TorsoForm, fitting: &Fitting, g: &GarmentCut, hem: &Hem, belt: &Belt, plates: &Plates, seed: u32) -> Parts {
    build_garment(form, fitting, g, hem, belt, plates, seed)
}

fn build_garment(form: &TorsoForm, fitting: &Fitting, g: &GarmentCut, hem: &Hem, belt: &Belt, plates: &Plates, seed: u32) -> Parts {
    let mut parts = Parts::new();
    let arms = fitting.arms;
    let form = form.eased(g.fit);
    let drape = Drape { notch: fitting.notch, ..garment_drape(&form, g, belt, plates, seed) };
    let (thickness, _, _, _) = style_drape(g.style);
    let mut surface = DrapedSurface::over(&form, drape, fitting.over, thickness);
    let mut look = body_look(g, plates);
    let style = g.style;
    let rigid = style == GarmentStyle::Cuirass;
    let cloak = style.is_open();
    let trim = hem.on.then(|| Look::cloth(&hem.fabric));
    let tw = hem.width;
    let trim_thickness = thickness + 0.03;

    // Body: rows from the hem to the neckline, which dips at the front.
    let height = drape.top - drape.hem;
    let rows = (30.0 + height * 8.0).round() as usize;
    let columns = 96;
    let cols = if cloak { columns + 1 } else { columns };
    let points = |surface: &DrapedSurface| -> Vec<Vec<V3>> {
        (0..=rows)
            .map(|r| {
                let v = r as f32 / rows as f32;
                (0..cols)
                    .map(|c| {
                        let u = c as f32 / columns as f32;
                        let a = drape.angle(u, drape.top);
                        let hem = drape.hem_at(a);
                        surface.point(u, hem + (drape.top_at(a) - hem) * v, 0.0)
                    })
                    .collect()
            })
            .collect()
    };
    let body_options = ShellOptions { wrap_around: !cloak, thickness, lip_columns: cloak, ..ShellOptions::tube(thickness) };
    let mut body = shell(&points(&surface), &body_options);
    if style == GarmentStyle::Gambeson {
        // Quilting puffs stay in the silhouette but shade with the coat's form: under banded
        // toon light, relief this fine breaks the coat into islands wherever a band edge
        // crosses it. The `Quilted` pattern draws the stitching and the shading of the puffs
        // instead, laid out to match them.
        let quilting = Quilting::fitted(&body.positions);
        surface.drape.quilt = Some(quilting);
        let normals = std::mem::take(&mut body.normals);
        body = shell(&points(&surface), &body_options);
        body.normals = normals;
        if let Some(pattern) = look.pattern.as_mut().filter(|p| p.kind == PatternKind::Quilted) {
            *pattern = quilting.pattern(pattern.color);
        }
    }
    parts.add_look("body", body, look);

    // Trims.
    if let Some(trim) = trim {
        if hem.hemline {
            parts.add_look("trim", surface.band(0.0, 1.0, 4, 0.016, trim_thickness, |a| (drape.hem_at(a) - 0.012, drape.hem_at(a) - 0.012 + tw)), trim);
        }
        if hem.neck && g.neck != Neckline::High {
            parts.add_look("trim", surface.band(0.0, 1.0, 3, 0.02, trim_thickness, |a| (drape.top_at(a) - tw * 0.7, drape.top_at(a) + 0.01)), trim);
        }
        if hem.front {
            let du = tw / (4.0 * PI * 1.9 * g.fit);
            if cloak {
                for (u0, u1) in [(0.0, du * 1.4), (1.0 - du * 1.4, 1.0)] {
                    parts.add_look("trim", surface.band(u0, u1, 40, 0.014, trim_thickness, |a| (drape.hem - 0.01, drape.top_at(a))), trim);
                }
            } else {
                parts.add_look("trim", surface.band(0.5 - du, 0.5 + du, 40, 0.014, trim_thickness, |a| (drape.hem, drape.top_at(a))), trim);
            }
        }
    }

    // Collar, cowl and hood.
    let neck = form.neck;
    match g.neck {
        Neckline::High => {
            let options = ShellOptions::tube(0.05);
            let collar = patch(40, 6, &options, |u, v| {
                let a = BACK + u * TAU;
                let y = drape.top_at(a) - 0.12 + 0.6 * v;
                let k = 1.1 + 0.05 * v;
                let [x, z] = crate::build::form::section_point(a, neck.half_width * k, neck.half_depth * k, 0.9, 1.0);
                // Where the neckline dips below the neck the collar's foot stands on the
                // cloth, not inside it.
                let cloth = surface.point_at(a, y, 0.02);
                let (r, rc) = (x.hypot(z).max(1e-4), cloth[0].hypot(cloth[2]));
                let k = if y < neck.y && rc > r { rc / r } else { 1.0 };
                [x * k, y, z * k]
            });
            parts.add_look("collar", collar, trim.filter(|_| hem.neck).unwrap_or(look));
        }
        Neckline::Cowl => {
            let path: Vec<V3> = (0..48)
                .map(|i| {
                    let a = BACK + TAU * i as f32 / 48.0;
                    let front = (-a.sin()).max(0.0);
                    surface.point_at(a, form.shoulder_top.y - 0.05 - 0.35 * front * front, 0.12)
                })
                .collect();
            let fat = if cloak { 0.34 } else { 0.28 };
            let cowl = crate::geom::tube_varying(&path, 14, &SweepOptions { up: Some(vec3::Y), ..SweepOptions::closed_loop() }, |t, _| {
                let a = BACK + TAU * t;
                fat * (1.0 + 0.35 * (-a.sin()).max(0.0))
            });
            parts.add_look("cowl", cowl, look);
            parts.add_look("hood", hood_mesh(&surface, &form, thickness), look);
        }
        Neckline::Round => {}
    }

    // Sleeves.
    if g.sleeves.length() > 0.0 && !cloak && !rigid && style != GarmentStyle::Brigandine {
        let (_, ease, _, _) = style_drape(style);
        for (side, phase) in [(1.0, drape.phases[0]), (-1.0, drape.phases[1])] {
            let sleeve_at = |offset: f32| match arms {
                Some(arms) => SleeveForm::on_arm(&arms[if side > 0.0 { 1 } else { 0 }], g, ease, offset, phase),
                None => SleeveForm::new(&form, g, side, offset, phase),
            };
            let sleeve = sleeve_at(0.0);
            parts.add_look("sleeves", sleeve.mesh(thickness), look);
            if let Some(trim) = trim.filter(|_| hem.cuffs) {
                let cuff = sleeve_at(0.02);
                parts.add_look("trim", cuff.cuff(tw / g.sleeves.length(), trim_thickness), trim);
            }
        }
    }

    // Belt and buckle.
    if belt.on && !cloak && !rigid {
        let (y0, y1) = (form.waist.y - belt.width / 2.0, form.waist.y + belt.width / 2.0);
        parts.add_look("belt", surface.band(0.0, 1.0, 3, 0.03, 0.06, |_| (y0, y1)), Look::of(belt.material));
        let front = surface.point_at(FRONT, form.waist.y, 0.09);
        parts.add_look("buckle", buckle_mesh(belt.width, front), Look::of(belt.buckle));
        // The strap's free end, tucked down from the buckle.
        let tongue = surface.band(0.5 + 0.035, 0.5 + 0.035 + belt.width * 0.5 / 9.0, 6, 0.09, 0.04, |_| {
            (form.waist.y - belt.width * 1.6, form.waist.y + belt.width * 0.3)
        });
        parts.add_look("belt", tongue, Look::of(belt.material));
    }

    // Plate fittings.
    if rigid {
        let edge = |y: f32, off: f32, r: f32| -> MeshBuilder {
            let ring: Vec<V3> = (0..96).map(|i| surface.point_at(BACK + TAU * i as f32 / 96.0, y, off)).collect();
            rolled_edge(&ring, r, true)
        };
        parts.add_look("edges", edge(drape.hem + 0.02, 0.012, 0.045), Look::of(g.material));
        let neck_ring: Vec<V3> = (0..96)
            .map(|i| {
                let a = BACK + TAU * i as f32 / 96.0;
                surface.point_at(a, drape.top_at(a) - 0.02, 0.012)
            })
            .collect();
        parts.add_look("edges", rolled_edge(&neck_ring, 0.04, true), Look::of(g.material));
        let faulds = plates.faulds;
        let options = ShellOptions::tube(0.045);
        for i in 0..faulds {
            let y1 = drape.hem - i as f32 * 0.42 + 0.08;
            let y0 = y1 - 0.5;
            let off = 0.05 + 0.045 * i as f32;
            let lame = patch(96, 3, &options, |u, v| surface.point(u, y0 + (y1 - y0) * v, off + 0.04 * (1.0 - v)));
            parts.add_look("faulds", lame, Look::of(g.material));
            parts.add_look("faulds", edge(y0, off + 0.04, 0.04), Look::of(g.material));
        }
        if let Some(trim) = trim.filter(|_| hem.hemline && faulds > 0) {
            let y = drape.hem - faulds as f32 * 0.42;
            parts.add_look("trim", surface.band(0.0, 1.0, 2, 0.06 + 0.045 * faulds as f32, 0.04, |_| (y - 0.06, y + 0.06)), trim);
        }
    }
    if plates.pauldrons && matches!(style, GarmentStyle::Cuirass | GarmentStyle::Brigandine | GarmentStyle::Hauberk | GarmentStyle::Scale) {
        for side in [1.0f32, -1.0] {
            let a = 0.38f32;
            // On a body the cop sits over the shoulder joint, down the upper arm (the plate's
            // `saGarment` pauldrons), clear of the sleeves under it.
            let (origin, axis, radius) = match arms {
                Some(arms) => {
                    let arm = &arms[if side > 0.0 { 1 } else { 0 }];
                    let ts = arm.t_joints[0];
                    let shoulder = arm.point_at(ts);
                    let r = arm.radius_at(ts);
                    let axis = vec3::normalize_or(vec3::sub(arm.point_at(arm.t_joints[1]), shoulder), vec3::neg(vec3::Y));
                    (vec3::add(shoulder, [0.0, r * 0.45, 0.0]), axis, (r * 1.15 + 0.12) * g.fit)
                }
                None => ([side * form.shoulder.half_width * 0.9, form.shoulder.y + 0.02, 0.0], [side * a.sin(), -a.cos(), 0.0], 0.92 * g.fit),
            };
            let stack = LameStack {
                origin,
                axis,
                toward: [side, 0.2, 0.0],
                count: plates.lames,
                radius,
                step: 0.05,
                height: 0.42,
                overlap: 0.2,
                half_angle: 1.95,
                thickness: 0.05,
                cap: true,
            };
            parts.add_look("pauldrons", limb_lames(&stack), Look::of(g.material));
        }
    }

    // A cloak's brooch.
    if cloak {
        let y = drape.top - 0.35;
        let at = surface.point_at(FRONT, y, 0.06);
        let mut disc = crate::geom::cylinder(0.24, 0.07, 24, crate::geom::Shading::BANDED);
        disc.rotate_x(PI / 2.0).translate(vec3::add(at, [0.0, 0.0, -0.02]));
        parts.add_look("brooch", disc, Look::of(MaterialKind::Brass));
        let mut stone = cut_stone(GemCut::Cabochon, 0.12);
        stone.rotate_x(PI / 2.0).translate(vec3::add(at, [0.0, 0.0, 0.06]));
        parts.stone("stone", stone, Gemstone::Garnet, 0.0, false);
    }
    parts
}

/// A hood lying down the back from a cowl: a soft sheet whose sides narrow to a point.
fn hood_mesh(surface: &DrapedSurface, form: &TorsoForm, thickness: f32) -> MeshBuilder {
    let top = form.shoulder_top.y + 0.05;
    let bottom = form.shoulder.y - 2.1;
    let options = ShellOptions::sheet(thickness);
    // u across the back from the wearer's left to right; v from the point up to the neck.
    patch(32, 18, &options, |u, v| {
        let width = 0.85 * (v * PI * 0.5).sin().powf(0.6);
        let a = BACK + (0.5 - u) * 2.0 * width;
        let y = bottom + (top - bottom) * v;
        let puff = 0.06 + 0.32 * (PI * u).sin() * (PI * v).sin().max(0.0).powf(0.7) - 0.06 * (1.0 - (2.0 * u - 1.0).abs()).powf(6.0);
        surface.point_at(a, y, puff)
    })
}

/// A square buckle frame and prong of belt width `width`, centred on `at`, facing +Z.
pub fn buckle_mesh(width: f32, at: V3) -> MeshBuilder {
    let (hw, hh, r) = (width * 0.42, width * 0.55, width * 0.08);
    let corners = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
    let mut ring = Vec::new();
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        for k in 0..6 {
            let t = k as f32 / 6.0;
            ring.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, 0.0]);
        }
    }
    let ring = CatmullRom::centripetal(ring).with_closed(true).sample(2);
    let mut mesh = tube(&ring, 8, &SweepOptions::closed_loop(), |_| r);
    let prong = tube(&[[0.0, 0.0, r * 0.5], [hw * 0.9, 0.0, r * 0.8]], 6, &SweepOptions::default(), |_| r * 0.55);
    mesh.append(&prong);
    mesh.translate(at);
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hanging_cloth_never_cuts_into_the_form() {
        let form = TorsoForm::MANNEQUIN;
        let g = GarmentCut::default();
        let drape = garment_drape(&form, &g, &Belt { on: false, ..Belt::default() }, &Plates::default(), 3);
        let s = DrapedSurface::new(&form, Drape { folds: 0.0, ..drape });
        let profile = form.profile();
        for i in 0..40 {
            let a = TAU * i as f32 / 40.0;
            for y in [-2.0, -1.0, 0.0, 1.0, 2.4, 3.5] {
                let cloth = s.point_at(a, y, 0.0);
                let body = profile.point(a, y);
                assert!(cloth[0].hypot(cloth[2]) >= body[0].hypot(body[2]) + 0.03, "angle {a} y {y}");
            }
        }
    }

    #[test]
    fn a_belt_pulls_the_cloth_in() {
        let form = TorsoForm::MANNEQUIN;
        let g = GarmentCut { style: GarmentStyle::Robe, flare: 0.0, folds: 0.0, ..GarmentCut::default() };
        let belted = DrapedSurface::new(&form, garment_drape(&form, &g, &Belt::default(), &Plates::default(), 1));
        let loose = DrapedSurface::new(&form, garment_drape(&form, &g, &Belt { on: false, ..Belt::default() }, &Plates::default(), 1));
        let r = |s: &DrapedSurface| {
            let p = s.point_at(FRONT, 0.0, 0.0);
            p[0].hypot(p[2])
        };
        assert!(r(&belted) < r(&loose) - 0.05, "belted {} loose {}", r(&belted), r(&loose));
    }
}
