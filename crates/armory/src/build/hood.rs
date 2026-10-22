//! Hoods, cowls and coifs on a [`HeadForm`] set on a [`TorsoForm`]: one sheet from the
//! crown over the head, round the face opening, down the neck and out over the shoulders as
//! a cape. Built in the forms' space (the torso's waist at y = 0, front +Z), so `fb_body`
//! can pass a folk body's own head and torso.
//!
//! The sheet is laid out in columns round the head (`u`, from one side of the face opening
//! round the back to the other) and rows from the cape's hem up to the crown (`s`):
//!
//! 1. *Cape*: the garment drape ([`DrapedSurface`]) hanging from the shoulders, with its
//!    folds, flare and a dagged edge.
//! 2. *Neck*: a curve from the top of the cape to where the hood leaves the head, matching
//!    both surfaces' slopes, so the cloth falls from the back of the head and the chin
//!    rather than following the neck in.
//! 3. *Head*: the head form, eased out, its crown drawn to a point toward the back.
//!
//! A coif is the head part alone, cut round the jaw and the nape. The plate prototype draws
//! a hood as a single head offset with a torus cowl, and a mail coif as a lathed dome with a
//! skirt; here the hood is one continuous sheet with thickness.

use std::f32::consts::{PI, TAU};

use procgen_rng::Mulberry32;

use crate::build::form::{HeadForm, ShellOptions, TorsoForm, rolled_edge, shell};
use crate::build::garment::{Drape, DrapedSurface, Underlay};
use crate::build::hafted::smoothstep;
use crate::build::parts::{Look, Parts};
use crate::build::strap::{buckle, strap_along};
use crate::fabric::{PatternKind, SurfacePattern};
use crate::geom::vec3::{self, V3};
use crate::geom::{CatmullRom, MeshBuilder, SweepOptions, tube, uv_sphere};
use crate::material::{MaterialKind, Rgb};
use crate::sections::hem::Hem;
use crate::sections::hood::{Hood, HoodStyle};

/// The hood's sheet, ready to sample by column `u` and row `s`.
#[derive(Clone, Debug)]
pub struct HoodSurface {
    pub head: HeadForm,
    pub hood: Hood,
    /// The cape's drape, for styles with a cape.
    pub cape: Option<DrapedSurface>,
    /// Distance kept off the head.
    pub ease: f32,
    /// Rows `0..s_cape` are the cape, `s_cape..s_head` the neck, `s_head..=1` the head.
    pub s_cape: f32,
    pub s_head: f32,
    /// Top of the cape at the back, and its hem.
    pub cape_top: f32,
    pub cape_hem: f32,
    /// Where the point of the crown is drawn to, from the head's surface.
    peak_pull: V3,
    /// Polar angle (from the crown) where the hood leaves the head for the neck along its sides
    /// and back: [`FRONT_RIM`], or higher up the head where the cape's top stands as high as
    /// that (a cape over high shoulders, on a small head), so the neck always rises from the
    /// cape to the head instead of folding back on itself. Under the chin and along the jaw the
    /// rim stays at [`FRONT_RIM`] whatever the cape does ([`HoodSurface::rim`]): raising it
    /// there would draw the cloth across the mouth and nose.
    polar_low: f32,
    /// The least the face opening may be, a half angle round the face: out to the wearer's eyes
    /// and a little past them (a lizard's sit far round the sides of its head).
    min_opening: f32,
}

/// Polar angle (from the crown) where a hood leaves the head for the neck under the chin: the
/// face opening stays open all the way down to here (the chin is at 0.8 of a half turn), then
/// closes to the slit down the front of the neck.
const FRONT_RIM: f32 = 0.88 * PI;
/// Where the face opening begins to close: the chin, below the mouth's muzzle.
const CLOSE_FROM: f32 = 0.78 * PI;
/// The highest a hood may leave the head at the sides and back for a high cape: a little under
/// the equator, so the slit's smoothstep from the equator to the join never has equal edges.
const HOOD_FLOOR: f32 = 0.55 * PI;
/// The radius, in units, that the cape's resting surface is rounded to where it turns a corner
/// over a pauldron or the shoulder's edge: a cape drapes round a corner, it does not fold to a
/// point there.
const CAPE_ROUND: f32 = 0.5;
/// How far below the cape's top its own rows stop and the neck begins, so the cloth turns from
/// the shoulder to the neck over a stretch (a bend over a few rows folded the sheet's thickness).
const CAPE_BLEND: f32 = 0.1;
/// Half the width of the slit left down the front of the neck.
const SLIT: f32 = 0.02;
/// Polar angle of the crown's point, toward the back of the head.
const PEAK_POLAR: f32 = 0.24 * PI;

impl HoodSurface {
    pub fn new(head: &HeadForm, torso: &TorsoForm, hood: &Hood, seed: u32) -> HoodSurface {
        Self::over(head, torso, None, 0.0, hood, seed)
    }

    /// The sheet with its cape hung over `under` as well as the torso (the arms and whatever is
    /// worn on them), as a cloak is, so it lies on the shoulders and falls down them instead
    /// of standing out from a form widened all round to clear them.
    pub fn over(head: &HeadForm, torso: &TorsoForm, under: Option<Underlay>, min_opening: f32, hood: &Hood, seed: u32) -> HoodSurface {
        let mut rng = Mulberry32::new(seed ^ 0x2545_f491);
        let style = hood.style;
        let ease = match style {
            HoodStyle::Hood => 0.1 + 0.2 * hood.ease,
            HoodStyle::Cowl => 0.22 + 0.25 * hood.ease,
            HoodStyle::Coif => 0.03 + 0.03 * hood.ease,
            HoodStyle::MailCoif => 0.06 + 0.06 * hood.ease,
        };
        let cape_top = torso.shoulder_top.y + 0.5 * (torso.neck.y - torso.shoulder_top.y).max(0.1);
        let cape_hem = torso.shoulder.y - hood.cape;
        let cape = style.has_cape().then(|| {
            let fold_count = rng.int(7, 11);
            let phases = [rng.range_f32(0.0, TAU), rng.range_f32(0.0, TAU), rng.range_f32(0.0, TAU)];
            let stiff = if style.is_mail() { 0.5 } else { 1.0 };
            let drape = Drape {
                top: cape_top,
                neck_dip: 0.0,
                hem: cape_hem,
                support: torso.shoulder.y - 0.1,
                ease: 0.12 + 0.08 * hood.ease,
                tuck: 0.15,
                flare_from: cape_top,
                flare: (0.04 + 0.1 * hood.ease) * stiff,
                folds: hood.folds * stiff,
                fold_count,
                phases,
                cinch: None,
                opening: None,
                quilt: None,
                ridge: 0.0,
                notch: None,
            };
            DrapedSurface::over_rounded(torso, drape, under, thickness(style), CAPE_ROUND)
        });
        let (s_cape, s_head) = if style.has_cape() { (0.34, 0.5) } else { (0.0, 0.0) };
        let lift = 0.35 + 0.35 * hood.peak;
        let peak_pull = vec3::scale([0.0, lift * 0.7, -lift], hood.peak);
        let mut surface = HoodSurface { head: *head, hood: *hood, cape, ease, s_cape, s_head, cape_top, cape_hem, peak_pull, polar_low: FRONT_RIM, min_opening };
        if surface.cape.is_some() {
            surface.neck_join();
        }
        surface
    }

    /// Where the hood leaves the head along the sides and back: the lowest point on the head
    /// (from [`FRONT_RIM`] up to the head's equator) at which the head's surface, all the way
    /// round but along the jaw (where the cloth stays low and the cape's top is brought down
    /// instead, [`HoodSurface::cape_top_at`]), lies a little above the top of the cape beneath it.
    fn neck_join(&mut self) {
        let margin = 0.08 * self.head.half_height;
        let steps = ((FRONT_RIM - HOOD_FLOOR) / (0.01 * PI)).round() as usize;
        let mut found = HOOD_FLOOR;
        for i in 0..=steps {
            let polar = FRONT_RIM - 0.01 * PI * i as f32;
            self.polar_low = polar;
            let clear = (0..96).all(|k| {
                let u = k as f32 / 96.0;
                let az = Self::neck_azimuth(u);
                // Only where the side rim has any say: the jaw keeps its own.
                self.rim_weight(u) < 0.5 || self.head_point(az, self.rim(u), 0.0)[1] >= self.cape_top_raw(az) + margin
            });
            if clear {
                found = polar;
                break;
            }
        }
        self.polar_low = found;
    }

    /// How far column `u` has left the face's edge for the head's side and back, `0..=1`.
    fn rim_weight(&self, u: f32) -> f32 {
        smoothstep(0.02, 0.16, u.min(1.0 - u))
    }

    /// Polar angle where the hood leaves the head at column `u`: under the chin and along the
    /// jaw at [`FRONT_RIM`], rising to the side rim round the ear and back.
    fn rim(&self, u: f32) -> f32 {
        let w = self.rim_weight(u);
        FRONT_RIM + (self.polar_low - FRONT_RIM) * w
    }

    /// The azimuth of column `u` down the neck, where the face opening is only the slit.
    fn neck_azimuth(u: f32) -> f32 {
        SLIT + u * (TAU - 2.0 * SLIT)
    }

    /// Half the angle of the face opening at polar angle `polar` (rows on the head).
    fn gap(&self, polar: f32) -> f32 {
        let o = self.hood.opening;
        match self.hood.style {
            HoodStyle::Coif => (1.05 + 0.4 * o) * smoothstep(0.1 * PI, 0.3 * PI, polar),
            // Closed over the crown (a seam), open round the face, a slit under the chin.
            _ => {
                let max = (0.5 + 0.6 * o).max(self.min_opening);
                max * smoothstep(0.16 * PI, 0.36 * PI, polar) * (1.0 - smoothstep(CLOSE_FROM, FRONT_RIM, polar))
                    + SLIT * smoothstep(0.5 * PI, FRONT_RIM, polar)
            }
        }
    }

    /// Polar angle of the coif's lower edge at `azimuth`: the jaw at the sides, the nape behind.
    fn coif_edge(azimuth: f32) -> f32 {
        let back = (-azimuth.cos()).max(0.0);
        0.74 * PI - 0.12 * PI * back
    }

    /// The point of the crown: where the sheet is drawn out toward the back.
    fn peak_weight(&self, azimuth: f32, polar: f32) -> f32 {
        if self.hood.peak <= 0.0 {
            return 0.0;
        }
        let d = [polar.sin() * azimuth.sin(), polar.cos(), polar.sin() * azimuth.cos()];
        let tip = [0.0, PEAK_POLAR.cos(), -PEAK_POLAR.sin()];
        let angle = vec3::dot(d, tip).clamp(-1.0, 1.0).acos();
        // A cusp at the tip, so the crown draws to a point rather than a bump.
        (1.0 - angle / 0.9).max(0.0).powf(2.5)
    }

    /// The tip of the crown's point.
    pub fn peak_tip(&self) -> V3 {
        vec3::add(self.head.point(PI, PEAK_POLAR, self.ease), self.peak_pull)
    }

    fn head_point(&self, azimuth: f32, polar: f32, extra: f32) -> V3 {
        let p = self.head.point(azimuth, polar, self.ease + extra);
        vec3::add(p, vec3::scale(self.peak_pull, self.peak_weight(azimuth, polar)))
    }

    pub fn cape_point(&self, azimuth: f32, y: f32, extra: f32) -> V3 {
        let cape = self.cape.as_ref().expect("a cape");
        cape.point_at(HeadForm::torso_angle(azimuth), y, extra)
    }

    /// Top of the cape at `azimuth`, never above where the hood leaves the head there: a cape
    /// over high shoulders is brought down under the chin rather than the hood being lifted
    /// over the face.
    fn cape_top_at(&self, azimuth: f32) -> f32 {
        let u = ((azimuth - SLIT) / (TAU - 2.0 * SLIT)).clamp(0.0, 1.0);
        let rim = self.head_point(azimuth, self.rim(u), 0.0)[1];
        // Smoothly, so the top has no kink where the head takes over from the shoulders.
        let (a, b, k) = (self.cape_top_raw(azimuth), rim - 0.08 * self.head.half_height, 0.15 * self.head.half_height);
        let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
        b + (a - b) * h - k * h * (1.0 - h)
    }

    /// Top of the cape at `azimuth` before the head is taken into account: a cowl's falls low
    /// over the chest.
    fn cape_top_raw(&self, azimuth: f32) -> f32 {
        // A cowl falls low over the chest only: narrow enough that it stays up over the
        // shoulders it rests on (at 50 degrees from straight ahead it dips by under a fifth of
        // its fall, where the square of the cosine gave two fifths and left what is worn on
        // the shoulders standing out above its edge).
        let cowl = self.hood.style == HoodStyle::Cowl;
        let front = azimuth.cos().max(0.0).powi(if cowl { 4 } else { 2 });
        let dip = if cowl { 0.9 + 0.6 * self.hood.ease } else { 0.25 };
        self.cape_top - dip * front
    }

    /// The cape's hem at `azimuth`, cut into rounded tongues when dagged.
    pub fn hem_at(&self, azimuth: f32) -> f32 {
        let n = self.hood.dags;
        if n == 0 {
            return self.cape_hem;
        }
        let v = 0.5 + 0.5 * (n as f32 * (azimuth - PI)).cos();
        self.cape_hem + 0.35 * (1.0 - v.powf(0.35))
    }

    /// The azimuth of column `u` at row `s`.
    pub fn azimuth(&self, u: f32, s: f32) -> f32 {
        let gap = if s < self.s_head { SLIT } else { self.gap(self.polar_front(s)) };
        let gap = if self.hood.style == HoodStyle::Coif { self.gap(self.polar_front(s)) } else { gap };
        gap + u * (TAU - 2.0 * gap)
    }

    /// Polar angle on the head of row `s` at column `u` (from `s_head` at the hood's lower edge
    /// to the crown).
    fn polar(&self, s: f32, u: f32) -> f32 {
        let t = ((s - self.s_head) / (1.0 - self.s_head)).clamp(0.0, 1.0);
        self.rim(u) * (1.0 - t)
    }

    /// Polar angle of row `s` at the face's edge, where the opening is measured.
    fn polar_front(&self, s: f32) -> f32 {
        if self.hood.style == HoodStyle::Coif {
            // A coif is cut at the jaw: its rows run from there to the crown.
            return 0.74 * PI * (1.0 - s.clamp(0.0, 1.0));
        }
        self.polar(s, 0.0)
    }

    /// The sheet's outer surface at column `u` and row `s`, `extra` further out.
    pub fn point(&self, u: f32, s: f32, extra: f32) -> V3 {
        let az = self.azimuth(u, s);
        if self.hood.style == HoodStyle::Coif {
            let t = s.clamp(0.0, 1.0);
            return self.head_point(az, Self::coif_edge(az) * (1.0 - t), extra);
        }
        if s >= self.s_head {
            return self.head_point(az, self.polar(s, u), extra);
        }
        let top = self.cape_top_at(az) - CAPE_BLEND;
        if s <= self.s_cape {
            let hem = self.hem_at(az);
            let y = hem + (top - hem) * (s / self.s_cape);
            return self.cape_point(az, y, extra);
        }
        // The neck: a cubic from the top of the cape to the head, matching both slopes.
        let t = (s - self.s_cape) / (self.s_head - self.s_cape);
        let p0 = self.cape_point(az, top, extra);
        let below = self.cape_point(az, top - 0.2, extra);
        let rim = self.rim(u);
        let p3 = self.head_point(az, rim, extra);
        let above = self.head_point(az, rim - 0.08, extra);
        let l = vec3::distance(p0, p3) / 3.0;
        let t0 = vec3::normalize_or(vec3::sub(p0, below), vec3::Y);
        let t3 = vec3::normalize_or(vec3::sub(above, p3), vec3::Y);
        let p1 = vec3::add(p0, vec3::scale(t0, l));
        let p2 = vec3::sub(p3, vec3::scale(t3, l));
        let k = 1.0 - t;
        let mut p = vec3::add(
            vec3::add(vec3::scale(p0, k * k * k), vec3::scale(p1, 3.0 * k * k * t)),
            vec3::add(vec3::scale(p2, 3.0 * k * t * t), vec3::scale(p3, t * t * t)),
        );
        // Soft folds where the cloth gathers round the neck; a cowl's neck falls forward in
        // a deep, rolled loop over the chest.
        if let Some(cape) = &self.cape {
            let cowl = self.hood.style == HoodStyle::Cowl;
            let depth = if cowl { 0.12 + 0.08 * self.hood.ease } else { 0.05 };
            let bulge = (PI * t).sin();
            let fold = cape.drape.fold_wave(HeadForm::torso_angle(az)) * depth * self.hood.folds * bulge;
            let out = vec3::normalize_or([p[0] - self.head.centre[0], 0.0, p[2] - self.head.centre[2]], vec3::Z);
            p = vec3::add(p, vec3::scale(out, fold));
            if cowl {
                let front = az.cos().max(0.0).powf(1.5);
                let k = front * bulge * (0.6 + 0.4 * self.hood.ease);
                // The loop falls forward and down, but never under the top of the cape it hangs
                // from: where the neck climbs little (high shoulders) it would fold under it.
                let drop = (0.35 * k).min(0.8 * (p[1] - p0[1]).max(0.0));
                p = vec3::add(p, [0.0, -drop, 0.3 * k]);
            }
        }
        p
    }

    /// The sheet between rows `s0` and `s1`, `extra` out, `cols` by `rows` cells.
    pub fn sheet(&self, s0: f32, s1: f32, cols: usize, rows: usize, extra: f32, thickness: f32) -> MeshBuilder {
        let points: Vec<Vec<V3>> = (0..=rows)
            .map(|r| {
                let s = s0 + (s1 - s0) * r as f32 / rows as f32;
                (0..=cols).map(|c| self.point(c as f32 / cols as f32, s, extra)).collect()
            })
            .collect();
        shell(&points, &ShellOptions::sheet(thickness))
    }

    /// The face opening's edge, up one side and down the other (head rows only).
    pub fn opening_edge(&self, extra: f32) -> Vec<V3> {
        let s0 = self.s_head + if self.hood.style == HoodStyle::Coif { 0.0 } else { 0.02 };
        // Up to the brow, where the opening closes into the crown's seam.
        let rows: Vec<f32> = (0..=60).map(|i| s0 + (1.0 - s0) * i as f32 / 60.0).take_while(|&s| self.gap(self.polar_front(s)) > 0.02).collect();
        let mut edge: Vec<V3> = rows.iter().map(|&s| self.point(0.0, s, extra)).collect();
        edge.extend(rows.iter().rev().map(|&s| self.point(1.0, s, extra)));
        edge
    }
}

/// Cloth or mail thickness of each style.
fn thickness(style: HoodStyle) -> f32 {
    match style {
        HoodStyle::Hood => 0.03,
        HoodStyle::Cowl => 0.035,
        HoodStyle::Coif => 0.025,
        HoodStyle::MailCoif => 0.04,
    }
}

/// The look of the hood's body: cloth with its pattern, or mail.
pub fn hood_look(hood: &Hood) -> Look {
    if hood.style.is_mail() {
        Look::patterned(hood.material, SurfacePattern { kind: PatternKind::Mail, color: Rgb::hex(0x232323), scale: hood.ring_size })
    } else {
        Look::cloth(&hood.cloth)
    }
}

/// Every part of a hood on `head` and `torso`: `"hood"`, `"liripipe"`, `"trim"`,
/// `"binding"`, `"ties"`, `"ventail"`, `"strap"` and `"buckle"`, as the state calls for them.
/// The hem's `hemline` trims the cape's edge, its `neck` the face opening.
pub fn hood_parts(head: &HeadForm, torso: &TorsoForm, hood: &Hood, hem: &Hem, seed: u32) -> Parts {
    hood_parts_over(head, torso, None, 0.0, hood, hem, seed)
}

/// [`hood_parts`] with the cape hung over `under` too, and the face opening at least
/// `min_opening` round ([`HoodSurface::over`]).
pub fn hood_parts_over(head: &HeadForm, torso: &TorsoForm, under: Option<Underlay>, min_opening: f32, hood: &Hood, hem: &Hem, seed: u32) -> Parts {
    let mut parts = Parts::new();
    let surface = HoodSurface::over(head, torso, under, min_opening, hood, seed);
    let look = hood_look(hood);
    let t = thickness(hood.style);
    let style = hood.style;
    let cols = if hood.dags > 0 { 160 } else { 112 };
    parts.add_look("hood", surface.sheet(0.0, 1.0, cols, 90, 0.0, t), look);

    // A tail hanging from the point down the back.
    if style == HoodStyle::Hood && hood.liripipe > 0.0 && hood.peak > 0.0 {
        let tip = surface.peak_tip();
        let len = hood.liripipe;
        let r0 = 0.07 + 0.05 * hood.peak;
        let radius = |u: f32| r0 * (1.0 - 0.55 * u) + 0.015;
        // Down the back, lying on the hood and cape rather than through them.
        let back: Vec<V3> = (0..=120).map(|i| surface.point(0.5, i as f32 / 120.0, 0.0)).collect();
        let back_z = |y: f32| -> f32 {
            back.windows(2)
                .rev()
                .find(|w| (w[0][1] - y) * (w[1][1] - y) <= 0.0)
                .map_or(back[0][2], |w| {
                    let t = (y - w[0][1]) / (w[1][1] - w[0][1]).abs().max(1e-5).copysign(w[1][1] - w[0][1]);
                    w[0][2] + (w[1][2] - w[0][2]) * t.clamp(0.0, 1.0)
                })
        };
        let mut control = vec![vec3::add(tip, [0.0, 0.0, 0.12]), tip];
        for k in 1..=8 {
            let u = k as f32 / 8.0;
            let y = tip[1] - 0.15 - len * u;
            control.push([0.08 * u, y, back_z(y).min(tip[2]) - radius(u) - 0.03]);
        }
        let path = CatmullRom::centripetal(control).sample_evenly(64);
        let tail = tube(&path, 12, &SweepOptions::default(), radius);
        parts.add_look("liripipe", tail, look);
    }

    // Trims: the cape's edge and the face opening.
    let trim = hem.on.then(|| Look::cloth(&hem.fabric));
    if let Some(trim) = trim.filter(|_| hem.hemline && style.has_cape()) {
        let band = (hem.width * 0.4 / (surface.cape_top - surface.cape_hem + 1.0)).clamp(0.02, 0.1);
        parts.add_look("trim", surface.sheet(0.0, band, cols, 3, 0.012, t + 0.02), trim);
    }
    let edge = surface.opening_edge(t * 0.5);
    let binding = if style.is_mail() { Some(trim.filter(|_| hem.neck).unwrap_or(Look::of(MaterialKind::Leather))) } else { trim.filter(|_| hem.neck) };
    if let Some(binding) = binding {
        let r = if style.is_mail() { 0.05 } else { (hem.width * 0.12).clamp(0.03, 0.08) };
        parts.add_look("binding", rolled_edge(&edge, r, false), binding);
    }

    // A coif's ties, knotted under the chin.
    if style == HoodStyle::Coif && hood.ties {
        let knot = head.point(0.0, 0.93 * PI, surface.ease + 0.06);
        let mut ties = MeshBuilder::new();
        for u in [0.0f32, 1.0] {
            let corner = surface.point(u, 0.03, t * 0.5);
            let k = if u == 0.0 { 1.0 } else { -1.0 };
            let mid = vec3::lerp(corner, knot, 0.5);
            let control = vec![corner, vec3::add(mid, [0.0, -0.15, 0.12]), vec3::add(knot, [0.06 * k, 0.02, 0.0])];
            ties.append(&tube(&CatmullRom::centripetal(control).sample_evenly(20), 6, &SweepOptions::default(), |_| 0.03));
            let dangle = vec![knot, vec3::add(knot, [0.05 * k, -0.25, 0.08]), vec3::add(knot, [0.02 * k, -0.55, 0.05])];
            ties.append(&tube(&CatmullRom::centripetal(dangle).sample_evenly(16), 6, &SweepOptions::default(), |_| 0.028));
        }
        let mut bow = uv_sphere(0.06, 10, 6);
        bow.scale([1.4, 1.0, 1.0]).translate(knot);
        ties.append(&bow);
        parts.add_look("ties", ties, Look::cloth(&hood.cloth));
    }

    // A mail coif's ventail: a flap under the chin and along the jaw, strapped up at the
    // temple. It stops at the chin: raised over the mouth and nose it hid the face (the face
    // opening stays open from the brow to the chin).
    if style.is_mail() && hood.ventail {
        let g = surface.gap(0.5 * PI);
        let flap = crate::build::form::patch(24, 10, &ShellOptions::sheet(t), |u, v| {
            let az = -g * 1.05 + (2.0 * g * 0.95) * u;
            let polar = 0.9 * PI - 0.1 * PI * v;
            head.point(az, polar, surface.ease + t + 0.02)
        });
        parts.add_look("ventail", flap, look);
        let az = g * 0.9 + 0.12;
        let at = head.point(az, 0.5 * PI, surface.ease + t + 0.02);
        let n = head.normal(az, 0.5 * PI);
        parts.add_look(
            "strap",
            strap_along(10, 0.12, 0.02, |k| {
                let a = g * 0.75 + 0.35 * k;
                (head.point(a, (0.8 - 0.3 * k) * PI, surface.ease + t + 0.03), head.normal(a, (0.8 - 0.3 * k) * PI))
            }),
            Look::of(MaterialKind::Leather),
        );
        let mut b = buckle(0.12, 0.015);
        b.place_basis(vec3::add(at, vec3::scale(n, 0.03)), [0.0, 1.0, 0.0], n);
        parts.add_look("buckle", b, Look::of(MaterialKind::Brass));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hood_stands_off_the_head() {
        let head = HeadForm::mannequin(1.0);
        let torso = TorsoForm::MANNEQUIN;
        for style in HoodStyle::ALL {
            let s = HoodSurface::new(&head, &torso, &Hood { style, peak: 0.0, ..Hood::default() }, 3);
            for i in 0..=8 {
                let u = i as f32 / 8.0;
                let p = s.point(u, 0.9, 0.0);
                let r = vec3::distance(p, head.centre);
                assert!(r > head.half_width * 0.95, "{style:?} at u {u}: {r}");
            }
        }
    }

    #[test]
    fn the_neck_rises_from_a_high_cape_to_the_head() {
        // Shoulders that stand up past the hood's usual rim (a cape over pauldrons on a small
        // head): the hood leaves the head higher up, so the neck still climbs from the cape's
        // top to the rim rather than folding back down onto itself.
        let head = HeadForm::mannequin(0.8);
        let mut torso = TorsoForm::MANNEQUIN;
        torso.shoulder_top.y += 0.6;
        torso.neck.y += 0.6;
        for style in [HoodStyle::Hood, HoodStyle::Cowl] {
            let s = HoodSurface::new(&head, &torso, &Hood { style, peak: 0.0, ..Hood::default() }, 3);
            assert!(s.polar_low < FRONT_RIM, "{style:?} still leaves the head at {}", s.polar_low);
            for i in 0..48 {
                let u = i as f32 / 48.0;
                let az = HoodSurface::neck_azimuth(u);
                let (top, rim) = (s.cape_top_at(az), s.head_point(az, s.rim(u), 0.0)[1]);
                assert!(rim >= top, "{style:?} at {az}: the rim {rim} is under the cape's top {top}");
            }
            // Under the chin the rim stays put whatever the cape does: the face stays open.
            assert_eq!(s.rim(0.0), FRONT_RIM);
        }
        // And over ordinary shoulders the sides leave the head along the jaw, well below the eyes.
        let s = HoodSurface::new(&HeadForm::mannequin(1.0), &TorsoForm::MANNEQUIN, &Hood::default(), 3);
        assert!(s.polar_low > 0.65 * PI, "{}", s.polar_low);
    }

    #[test]
    fn a_tiny_head_under_very_high_shoulders_still_builds_a_clean_sheet() {
        // Past anywhere the head can clear them the hood leaves at its floor, short of the
        // equator, and the sheet has no NaN where the slit's smoothstep would have had equal edges.
        let head = HeadForm::mannequin(0.4);
        let mut torso = TorsoForm::MANNEQUIN;
        torso.shoulder_top.y += 3.0;
        torso.neck.y += 3.0;
        for style in [HoodStyle::Hood, HoodStyle::Cowl] {
            let s = HoodSurface::new(&head, &torso, &Hood { style, ..Hood::default() }, 3);
            assert_eq!(s.polar_low, HOOD_FLOOR, "{style:?}");
            for r in 0..=20 {
                for c in 0..=20 {
                    let p = s.point(c as f32 / 20.0, r as f32 / 20.0, 0.0);
                    assert!(p.iter().all(|v| v.is_finite()), "{style:?} at {c},{r}: {p:?}");
                }
            }
        }
    }

    #[test]
    fn a_cowl_dips_over_the_chest_only() {
        let (head, torso) = (HeadForm::mannequin(1.0), TorsoForm::MANNEQUIN);
        let s = HoodSurface::new(&head, &torso, &Hood { style: HoodStyle::Cowl, ..Hood::default() }, 3);
        let fall = |az: f32| s.cape_top - s.cape_top_raw(az);
        assert!(fall(0.0) > 0.9, "a cowl falls low over the chest: {}", fall(0.0));
        // At 50 degrees from straight ahead, over the shoulder's front, under a fifth of that.
        assert!(fall(50f32.to_radians()) < 0.2 * fall(0.0), "{} at 50 degrees", fall(50f32.to_radians()));
        assert!(fall(PI / 2.0).abs() < 1e-4 && fall(PI).abs() < 1e-4, "level over the shoulders and the back");
    }

    #[test]
    fn the_cape_reaches_its_hem() {
        let head = HeadForm::mannequin(1.0);
        let torso = TorsoForm::MANNEQUIN;
        let s = HoodSurface::new(&head, &torso, &Hood { cape: 1.5, ..Hood::default() }, 3);
        let p = s.point(0.5, 0.0, 0.0);
        assert!((p[1] - (torso.shoulder.y - 1.5)).abs() < 0.3, "{p:?}");
        assert!(p[2] < 0.0, "u = 0.5 is the back: {p:?}");
    }
}
