//! Helmets: a shell of plate with its face opening, slits and breaths, and the fittings riveted
//! on: brow band, spangen bands, nasal, cheek plates, comb, visor, brim, mail aventail and
//! crest.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (the `helm`,
//! `fittings`, `aventail` and `crest` sections, `BASE`, `PRESETS.helm`, the `fam==='helm'`
//! branch of `randomize()`, `buildHelm()` and `forgeNoun()`). There is no `RANGES.helm`.
//!
//! The shell is one curved plate from [`crate::build::plate`]: its outline lives in the bowl's
//! own coordinates (`u` round the bowl, `v` down the profile from the crown), so a face
//! opening is a notch in the outline's lower edge and eye slits and breaths are holes, cut
//! clean through a shell with real thickness. Every fitting is another plate laid on the same
//! surface, so bands, nasals and cheek plates follow the bowl and its facets exactly.
//!
//! Departures from the prototype:
//! - [`HelmStyle::Barbute`] is added, with its T-shaped face opening.
//! - [`Visor`] is new: eye slits cut into great helms and closed sallets, and a hinged snout
//!   visor (hounskull) for bascinets, with breaths. The prototype painted dark slits on.
//! - [`CheekPlates`] are new (spangenhelms, nasal helms, skullcaps).
//! - Spangen bands meet under a crown cap with a knob instead of crossing at the apex.
//! - Plumes take a [`Tincture`] for their colour.
//! - The aventail's mail and its cloth hem (the shared [`Hem`]) reach the shader as surface
//!   patterns.
//!
//! Item space: y = 0 at the brow, +Y up, the face toward +Z, the bowl round the Y axis.

use std::f32::consts::{PI, TAU};

use crate::build::hafted::smoothstep;
use crate::build::parts::{Look, Parts};
use crate::build::plate::{PlateOptions, Surface, on_surface, plate, rivet_head, scatter_on_surface, sweep_on_surface};
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::geom::vec3::{self, Mat3, V3};
use crate::geom::{
    CatmullRom, ItemMesh, LatheOptions, P2, MeshBuilder, Shading, SweepOptions, extrude, fillet, fillet_polyline, lathe, regular_polygon,
    spline_profile, sweep, tube,
};
use crate::fabric::{Fabric, FabricLimits, FabricPattern, PatternKind, SurfacePattern};
use crate::heraldry::Tincture;
use crate::material::Rgb;
use crate::sections::hem::{Hem, HemLimits};
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::slider::{Sliders, sliders};
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Bone, Brass, Cloth, Gold, Iron, Leather, Silver};

/// The helmet generator.
pub struct Helm;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelmStyle {
    /// A round bowl with a brim.
    Kettle,
    /// A conical bowl, often segmented, with a nasal bar.
    Nasal,
    /// A tall pointed bowl down over the ears and neck, open at the face.
    Bascinet,
    /// A flat-topped closed cylinder with eye slits.
    GreatHelm,
    /// A low bowl sweeping back into a long tail.
    Sallet,
    /// A shallow cap.
    Skullcap,
    /// A close round helm to the jaw with a T-shaped face opening.
    Barbute,
}

impl HelmStyle {
    pub const ALL: [HelmStyle; 7] = [
        HelmStyle::Kettle, HelmStyle::Nasal, HelmStyle::Bascinet, HelmStyle::GreatHelm, HelmStyle::Sallet, HelmStyle::Skullcap,
        HelmStyle::Barbute,
    ];

    /// Styles whose bowl stops at the brow, so a brow band, nasal, cheek plates and a brim
    /// can hang from its rim.
    pub fn is_cap(self) -> bool {
        matches!(self, HelmStyle::Kettle | HelmStyle::Nasal | HelmStyle::Skullcap)
    }
}

/// The bowl: the prototype's `helm` section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HelmShell {
    pub style: HelmStyle,
    /// Half the width.
    pub radius: f32,
    pub height: f32,
    /// How pointed the crown is, 0..=1.
    pub point: f32,
    /// Sides of a segmented bowl; 12 or more reads as round.
    pub facets: u32,
    /// Brim width (kettle hats).
    pub brim: f32,
    /// How far the brim droops, 0..=1.
    pub droop: f32,
    /// How far the neck tail sweeps back and down (sallets), 0..=1.
    pub tail: f32,
    pub material: MaterialKind,
}

sliders! {
    HelmShellLimits for HelmShell {
        radius: 0.8, 1.5, 0.01;
        height: 0.8, 2.6, 0.01;
        point: 0.0, 1.0, 0.01;
        facets: 4.0, 32.0, 1.0;
        brim: 0.0, 1.2, 0.01;
        droop: 0.0, 1.0, 0.01;
        tail: 0.0, 1.0, 0.01;
    }
}

impl Default for HelmShell {
    /// The prototype's `BASE.helm`.
    fn default() -> Self {
        HelmShell { style: HelmStyle::Kettle, radius: 1.05, height: 1.3, point: 0.3, facets: 24, brim: 0.7, droop: 0.45, tail: 0.0, material: Iron }
    }
}

impl HelmShell {
    /// Whether the bowl is built from flat segments.
    pub fn is_segmented(&self) -> bool {
        self.facets <= 10
    }
}

/// Bands and bars riveted to the bowl: the prototype's `fittings` section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fittings {
    pub browband: bool,
    pub rivets: u32,
    /// Spangen bands from the brow up to the crown.
    pub bands: u32,
    pub nasal: bool,
    /// Height of a raised comb along the crown.
    pub comb: f32,
    pub material: MaterialKind,
}

sliders! {
    FittingsLimits for Fittings {
        rivets: 0.0, 24.0, 1.0;
        bands: 0.0, 8.0, 1.0;
        comb: 0.0, 0.6, 0.01;
    }
}

impl Default for Fittings {
    fn default() -> Self {
        Fittings { browband: true, rivets: 0, bands: 0, nasal: false, comb: 0.0, material: Iron }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisorStyle {
    None,
    /// The shell closes over the face with eye slits cut in it (great helms, closed sallets).
    Slit,
    /// A hinged, pointed snout over the face opening (a hounskull bascinet).
    Snout,
}

/// What covers the face. New: the prototype has no visor section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Visor {
    pub style: VisorStyle,
    /// Breathing holes.
    pub breaths: u32,
}

sliders! {
    VisorLimits for Visor {
        breaths: 0.0, 16.0, 1.0;
    }
}

impl Default for Visor {
    fn default() -> Self {
        Visor { style: VisorStyle::None, breaths: 0 }
    }
}

/// Hinged plates over the cheeks. New: the prototype has none.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CheekPlates {
    pub on: bool,
    /// How far below the rim they hang.
    pub length: f32,
    /// Width at the hinge.
    pub width: f32,
    pub material: MaterialKind,
}

sliders! {
    CheekPlatesLimits for CheekPlates {
        length: 0.4, 1.6, 0.01;
        width: 0.3, 1.0, 0.01;
    }
}

impl Default for CheekPlates {
    fn default() -> Self {
        CheekPlates { on: false, length: 1.0, width: 0.6, material: Iron }
    }
}

/// A mail curtain hung from the rim: the prototype's `aventail` section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Aventail {
    pub on: bool,
    pub length: f32,
    /// How far it spreads over the shoulders, 0..=1.
    pub flare: f32,
    /// Rings per unit, for the shader's mail pattern.
    pub ring_size: f32,
    pub material: MaterialKind,
}

sliders! {
    AventailLimits for Aventail {
        length: 0.4, 3.0, 0.01;
        flare: 0.0, 1.0, 0.01;
        ring_size: 2.0, 14.0, 0.1;
    }
}

impl Default for Aventail {
    fn default() -> Self {
        Aventail { on: false, length: 1.5, flare: 0.4, ring_size: 7.0, material: Iron }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrestStyle {
    None,
    Plume,
    Horns,
}

/// The prototype's `crest` section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crest {
    pub style: CrestStyle,
    pub size: f32,
    /// Feathers in a plume.
    pub count: u32,
    /// Plume colour.
    pub color: Tincture,
    /// Horn material.
    pub material: MaterialKind,
}

sliders! {
    CrestLimits for Crest {
        size: 0.5, 3.0, 0.01;
        count: 1.0, 9.0, 1.0;
    }
}

impl Default for Crest {
    fn default() -> Self {
        Crest { style: CrestStyle::None, size: 1.6, count: 5, color: Tincture::Gules, material: Bone }
    }
}

/// Every parameter of one helmet. The default is the prototype's `BASE` helm.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HelmState {
    pub seed: u32,
    pub shell: HelmShell,
    pub fittings: Fittings,
    pub visor: Visor,
    pub cheeks: CheekPlates,
    pub aventail: Aventail,
    /// Cloth trim along the aventail's lower edge (only `hemline` applies).
    pub hem: Hem,
    pub crest: Crest,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every helmet section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HelmLimits {
    pub shell: HelmShellLimits,
    pub fittings: FittingsLimits,
    pub visor: VisorLimits,
    pub cheeks: CheekPlatesLimits,
    pub aventail: AventailLimits,
    pub hem: HemLimits,
    /// The hem's cloth.
    pub trim: FabricLimits,
    pub crest: CrestLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges (it narrows none for helmets).
pub const LIMITS: HelmLimits = HelmLimits {
    shell: HelmShellLimits::SLIDERS,
    fittings: FittingsLimits::SLIDERS,
    visor: VisorLimits::SLIDERS,
    cheeks: CheekPlatesLimits::SLIDERS,
    aventail: AventailLimits::SLIDERS,
    hem: HemLimits::SLIDERS,
    trim: FabricLimits::SLIDERS,
    crest: CrestLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl HelmState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &HelmLimits) {
        self.shell.fit(&limits.shell);
        self.fittings.fit(&limits.fittings);
        self.visor.fit(&limits.visor);
        self.cheeks.fit(&limits.cheeks);
        self.aventail.fit(&limits.aventail);
        self.hem.fit(&limits.hem);
        self.hem.fabric.fit(&limits.trim);
        self.crest.fit(&limits.crest);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    fn nudge(&mut self, rng: &mut Mulberry32, limits: &HelmLimits, odds: f64, spread: f32) {
        self.shell.nudge(rng, &limits.shell, odds, spread);
        self.fittings.nudge(rng, &limits.fittings, odds, spread);
        self.visor.nudge(rng, &limits.visor, odds, spread);
        self.cheeks.nudge(rng, &limits.cheeks, odds, spread);
        self.aventail.nudge(rng, &limits.aventail, odds, spread);
        self.hem.nudge(rng, &limits.hem, odds, spread);
        self.hem.fabric.nudge(rng, &limits.trim, odds, spread);
        self.crest.nudge(rng, &limits.crest, odds, spread);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &HelmLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("shell", self.shell.out_of_range(&limits.shell)),
            tag("fittings", self.fittings.out_of_range(&limits.fittings)),
            tag("visor", self.visor.out_of_range(&limits.visor)),
            tag("cheeks", self.cheeks.out_of_range(&limits.cheeks)),
            tag("aventail", self.aventail.out_of_range(&limits.aventail)),
            tag("hem", self.hem.out_of_range(&limits.hem)),
            tag("hem.fabric", self.hem.fabric.out_of_range(&limits.trim)),
            tag("crest", self.crest.out_of_range(&limits.crest)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

/// Roll weights per style, in percent. The style is the archetype: it fixes which fittings
/// belong and sizes everything else.
pub const STYLE_WEIGHTS: [(HelmStyle, u32); 7] = [
    (HelmStyle::Kettle, 15),
    (HelmStyle::Nasal, 18),
    (HelmStyle::Bascinet, 16),
    (HelmStyle::GreatHelm, 12),
    (HelmStyle::Sallet, 13),
    (HelmStyle::Skullcap, 14),
    (HelmStyle::Barbute, 12),
];

impl Generator for Helm {
    type State = HelmState;

    fn roll(seed: u32) -> HelmState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&STYLE_WEIGHTS);
        compose(&mut rng, seed, style)
    }

    /// `forgeNoun()` for helmets, with the barbute and the hounskull added.
    fn noun(s: &HelmState) -> String {
        let base = match s.shell.style {
            HelmStyle::Nasal if s.fittings.bands > 0 => "Spangenhelm",
            HelmStyle::Nasal => "Nasal Helm",
            HelmStyle::Skullcap if s.crest.style == CrestStyle::Horns => "Horned Helm",
            HelmStyle::Skullcap => "Skullcap",
            HelmStyle::Kettle => "Kettle Hat",
            HelmStyle::Bascinet if s.visor.style == VisorStyle::Snout => "Hounskull",
            HelmStyle::Bascinet => "Bascinet",
            HelmStyle::GreatHelm => "Great Helm",
            HelmStyle::Sallet => "Sallet",
            HelmStyle::Barbute => "Barbute",
        };
        if s.crest.style == CrestStyle::Plume { format!("Plumed {base}") } else { base.to_string() }
    }

    fn build(s: &HelmState) -> ItemMesh {
        build_helm(s)
    }
}

impl HelmState {
    /// The same helmet made to fit a head of bowl radius `radius`: every length of the bowl,
    /// its brim, cheek plates, aventail, comb and crest scaled with it (the plate's
    /// `saHelm`), plate and mail thickness unchanged.
    pub fn sized(&self, radius: f32) -> HelmState {
        let k = radius / self.shell.radius.max(1e-3);
        let mut s = self.clone();
        s.shell.radius = radius;
        s.shell.height *= k;
        s.shell.brim *= k;
        s.cheeks.length *= k;
        s.aventail.length *= k;
        s.fittings.comb *= k;
        s.crest.size *= k;
        s
    }
}

impl Helm {
    /// "Forge new" forced to one style, for turntables and tests. Draws the same stream as
    /// [`Generator::roll`], so `roll(s)` equals `roll_as(s, style)` when `style` is what `s`
    /// rolls.
    pub fn roll_as(seed: u32, style: HelmStyle) -> HelmState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, style)
    }
}

// ───────────────────────── The bowl's surface ─────────────────────────

/// The bowl as a surface: `u` runs round it (an arc length at the rim's scale, `u = 0` at the
/// front, increasing toward the wearer's right), `v` runs down the profile from the crown by
/// arc length. Past the rim the surface carries on straight down the last stretch of profile,
/// so nasals, cheek plates and sallet tails have something to lie on; at the back it sweeps
/// outward by `tail`.
#[derive(Clone, Debug)]
struct Bowl {
    /// `(v, r, y)` from the crown to the rim.
    profile: Vec<[f32; 3]>,
    /// Arc length from the crown to the rim.
    rim: f32,
    /// `u` per radian round the bowl.
    scale: f32,
    /// Front-to-back over side-to-side.
    depth: f32,
    facets: Option<u32>,
    tail: f32,
    tail_len: f32,
}

impl Bowl {
    fn new(control: &[[f32; 2]], shell: &HelmShell) -> Bowl {
        // Control points run from the rim up to the crown; sample, then walk down from the crown.
        let mut pts = spline_profile(control, 96);
        pts.reverse();
        pts[0][0] = 0.0;
        let mut profile = Vec::with_capacity(pts.len());
        let mut v = 0.0;
        for (i, p) in pts.iter().enumerate() {
            if i > 0 {
                v += (p[0] - pts[i - 1][0]).hypot(p[1] - pts[i - 1][1]);
            }
            profile.push([v, p[0], p[1]]);
        }
        Bowl {
            rim: v,
            profile,
            scale: shell.radius,
            depth: 1.12,
            facets: shell.is_segmented().then_some(shell.facets.max(3)),
            tail: if shell.style == HelmStyle::Sallet { shell.tail } else { 0.0 },
            tail_len: shell.height * 0.5,
        }
    }

    fn theta(&self, u: f32) -> f32 {
        -u / self.scale
    }

    fn u(&self, theta: f32) -> f32 {
        -theta * self.scale
    }

    /// Half the bowl's circumference in `u`: the seam at the back.
    fn seam(&self) -> f32 {
        PI * self.scale
    }

    /// `(r, y)` of the profile at `v`, round the bowl at `theta`.
    fn section(&self, v: f32, theta: f32) -> (f32, f32) {
        let p = &self.profile;
        if v <= self.rim {
            let k = p.partition_point(|q| q[0] < v).clamp(1, p.len() - 1);
            let (a, b) = (p[k - 1], p[k]);
            let t = ((v - a[0]) / (b[0] - a[0]).max(1e-9)).clamp(0.0, 1.0);
            return (a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t);
        }
        let (a, b) = (p[p.len() - 2], p[p.len() - 1]);
        let len = (b[1] - a[1]).hypot(b[2] - a[2]).max(1e-9);
        let (dr, dy) = ((b[1] - a[1]) / len, (b[2] - a[2]) / len);
        let e = v - self.rim;
        let back = (-theta.cos()).max(0.0).powf(1.5);
        let alpha = self.tail * back * 1.1 * (e / (e + self.tail_len));
        let (sa, ca) = alpha.sin_cos();
        let (dr, dy) = (dr * ca - dy * sa, dy * ca + dr * sa);
        (b[1] + dr * e, b[2] + dy * e)
    }

    /// The radius factor of a segmented bowl at depth `v`: flat facets between ridges, a facet
    /// centred on the front, rounding out toward the rim so the bowl sits on a round brow
    /// (as spangenhelm segments sit on their brow band).
    fn facet(&self, theta: f32, v: f32) -> f32 {
        match self.facets {
            Some(n) => {
                let step = TAU / n as f32;
                let phi = theta - (theta / step).round() * step;
                let flat = (PI / n as f32).cos() / phi.cos();
                let round = smoothstep(self.rim * 0.5, self.rim * 0.95, v);
                flat + (1.0 - flat) * round
            }
            None => 1.0,
        }
    }

    /// The first `v` from the crown down where the profile reaches height `y` (straight on
    /// past the rim if it never does).
    fn v_at(&self, y: f32) -> f32 {
        let p = &self.profile;
        for k in 1..p.len() {
            let (a, b) = (p[k - 1], p[k]);
            if b[2] <= y {
                let t = ((a[2] - y) / (a[2] - b[2]).max(1e-9)).clamp(0.0, 1.0);
                return a[0] + (b[0] - a[0]) * t;
            }
        }
        let (a, b) = (p[p.len() - 2], p[p.len() - 1]);
        let dy = (b[2] - a[2]) / (b[0] - a[0]).max(1e-9);
        self.rim + (y - b[2]) / dy.min(-1e-3)
    }

    fn r_at(&self, v: f32) -> f32 {
        self.section(v, 0.0).0
    }

    /// The `u` half-width that spans `width` across the bowl at depth `v`.
    fn u_span(&self, width: f32, v: f32) -> f32 {
        width * self.scale / self.r_at(v).max(0.05 * self.scale)
    }
}

impl Surface for Bowl {
    fn point(&self, p: P2) -> V3 {
        let theta = self.theta(p[0]);
        let (r, y) = self.section(p[1], theta);
        let r = r * self.facet(theta, p[1]);
        let (s, c) = theta.sin_cos();
        [r * s, y, r * c * self.depth]
    }
}

/// The bowl pushed out along the horizontal: an aventail, a brim or a visor hung off it.
struct Hung<'a, F: Fn(f32, f32) -> (f32, f32)> {
    bowl: &'a Bowl,
    /// `(outward, y)` at `(theta, v)`.
    shape: F,
}

impl<F: Fn(f32, f32) -> (f32, f32)> Surface for Hung<'_, F> {
    fn point(&self, p: P2) -> V3 {
        let theta = self.bowl.theta(p[0]);
        let (r, y) = (self.shape)(theta, p[1]);
        let (s, c) = theta.sin_cos();
        [r * s, y, r * c * self.bowl.depth]
    }
}

/// Head landmarks below the brow, as fractions of the helm's radius.
const EYE: f32 = -0.3;
const NOSE: f32 = -0.62;

// ───────────────────────── Build ─────────────────────────

fn profile_control(sh: &HelmShell) -> Vec<[f32; 2]> {
    let (r, h, pt) = (sh.radius, sh.height, sh.point);
    match sh.style {
        HelmStyle::Nasal => vec![[r, 0.0], [r * 0.97, h * 0.25], [r * 0.76, h * 0.6], [r * 0.4, h * 0.86], [0.0, h * (1.0 + pt * 0.25)]],
        HelmStyle::Bascinet => {
            let low = -h * 0.38;
            vec![[r * 1.02, low], [r, 0.0], [r * 0.96, h * 0.35], [r * 0.74, h * 0.7], [r * 0.33, h * 0.93], [0.0, h * (1.0 + pt * 0.35)]]
        }
        HelmStyle::GreatHelm => {
            let low = -h * 0.78;
            vec![[r * 1.04, low], [r * 1.02, -h * 0.2], [r, h * 0.35], [r * 0.97, h * 0.62], [r * 0.8, h * 0.8], [0.0, h * (0.84 + pt * 0.12)]]
        }
        HelmStyle::Sallet => {
            let low = -h * 0.15;
            vec![[r * 1.05, low], [r, h * 0.15], [r * 0.93, h * 0.5], [r * 0.68, h * 0.82], [0.0, h * (1.0 + pt * 0.12)]]
        }
        HelmStyle::Skullcap => vec![[r, 0.0], [r * 0.98, h * 0.22], [r * 0.86, h * 0.45], [r * 0.55, h * 0.63], [0.0, h * (0.7 + pt * 0.1)]],
        HelmStyle::Barbute => {
            let low = -h * 0.75;
            vec![[r * 0.9, low], [r * 0.99, -h * 0.4], [r * 1.02, h * 0.05], [r * 0.95, h * 0.45], [r * 0.7, h * 0.78], [0.0, h * (0.92 + pt * 0.1)]]
        }
        HelmStyle::Kettle => vec![[r, 0.0], [r * 0.98, h * 0.3], [r * 0.86, h * 0.62], [r * 0.55, h * 0.86], [0.0, h * (0.96 + pt * 0.2)]],
    }
}

/// How far below the profile's rim the shell reaches at `theta`: the sallet's tail and closed
/// front, or a notch cut up for an open face (negative).
fn rim_drop(s: &HelmState, bowl: &Bowl, theta: f32) -> f32 {
    let sh = &s.shell;
    let r = sh.radius;
    let front = |half: f32| smoothstep(half, half - 0.22, theta.abs());
    match sh.style {
        HelmStyle::Sallet => {
            let tail = sh.tail * sh.height * 0.55 * (-theta.cos()).max(0.0).powf(1.5);
            let face = if s.visor.style == VisorStyle::Slit {
                (bowl.v_at(NOSE * r) - bowl.rim) * front(1.05)
            } else {
                (bowl.v_at(0.02 * r) - bowl.rim) * front(0.95)
            };
            tail + face
        }
        HelmStyle::Bascinet => (bowl.v_at(-0.02 * r) - bowl.rim) * front(0.78),
        _ => 0.0,
    }
}

/// The shell's outline in bowl coordinates: the crown edge, the seam, and the rim with any face
/// opening, as `(outline, holes)`.
fn shell_outline(s: &HelmState, bowl: &Bowl, du: f32) -> (Vec<P2>, Vec<Vec<P2>>) {
    let sh = &s.shell;
    let r = sh.radius;
    let seam = bowl.seam();
    let steps = ((2.0 * seam) / (du * 0.5)).ceil() as usize;
    let rim_at = |u: f32| bowl.rim + rim_drop(s, bowl, bowl.theta(u));
    // The rim from the seam on the wearer's right (u = +seam), round the front, to the left.
    let mut rim: Vec<P2> = if sh.style == HelmStyle::Barbute {
        let (slot, eyes) = (0.2, 0.68);
        let (top, bar) = (bowl.v_at(-0.04 * r), bowl.v_at(-0.4 * r));
        let mut pts: Vec<P2> = Vec::new();
        let edge = |pts: &mut Vec<P2>, from: f32, to: f32| {
            let n = (((from - to).abs() / (du * 0.5)).ceil() as usize).max(1);
            for k in 0..=n {
                let u = from + (to - from) * k as f32 / n as f32;
                pts.push([u, bowl.rim]);
            }
        };
        edge(&mut pts, seam, bowl.u(-slot));
        let notch = [
            [bowl.u(-slot), bar], [bowl.u(-eyes), bar], [bowl.u(-eyes), top], [bowl.u(eyes), top], [bowl.u(eyes), bar], [bowl.u(slot), bar],
        ];
        pts.extend_from_slice(&notch);
        edge(&mut pts, bowl.u(slot), -seam);
        fillet_polyline(&pts, 0.07, 0.3, 5)
    } else {
        (0..=steps).map(|k| {
            let u = seam - 2.0 * seam * k as f32 / steps as f32;
            [u, rim_at(u)]
        }).collect()
    };
    rim.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-5);
    let mut outline = vec![[-seam, 0.0], [seam, 0.0]];
    outline.extend(rim);

    let mut holes: Vec<Vec<P2>> = Vec::new();
    let slit = |holes: &mut Vec<Vec<P2>>, t0: f32, t1: f32, y: f32, h: f32| {
        let (v0, v1) = (bowl.v_at(y + h * 0.5), bowl.v_at(y - h * 0.5));
        let rect = vec![[bowl.u(t0), v0], [bowl.u(t1), v0], [bowl.u(t1), v1], [bowl.u(t0), v1]];
        holes.push(fillet(&rect, h * 0.45, 0.3, 4));
    };
    match (sh.style, s.visor.style) {
        (HelmStyle::GreatHelm, _) => {
            let h = 0.09 * r;
            slit(&mut holes, -0.78, -0.09, EYE * r, h);
            slit(&mut holes, 0.09, 0.78, EYE * r, h);
            breaths(&mut holes, bowl, s.visor.breaths, (EYE - 0.4) * r, &[-0.95, -0.25], 0.032 * r);
        }
        (HelmStyle::Sallet, VisorStyle::Slit) => {
            slit(&mut holes, -0.82, 0.82, EYE * r, 0.08 * r);
            breaths(&mut holes, bowl, s.visor.breaths, (EYE - 0.22) * r, &[-0.75, -0.3], 0.026 * r);
        }
        _ => {}
    }
    (outline, holes)
}

/// Round breathing holes in rows below `y`, across `theta` from `span[0]` to `span[1]`.
fn breaths(holes: &mut Vec<Vec<P2>>, bowl: &Bowl, count: u32, y: f32, span: &[f32; 2], radius: f32) {
    if count == 0 {
        return;
    }
    let per_row = 4.min(count) as usize;
    let rows = (count as usize).div_ceil(per_row);
    let mut left = count as usize;
    for row in 0..rows {
        let yr = y - row as f32 * radius * 3.6;
        let v = bowl.v_at(yr);
        let n = per_row.min(left);
        left -= n;
        for k in 0..n {
            let t = span[0] + (span[1] - span[0]) * (k as f32 + 0.5 + if row % 2 == 1 { 0.25 } else { 0.0 }) / per_row as f32;
            let ru = bowl.u_span(radius, v);
            holes.push((0..10).map(|j| {
                let a = TAU * j as f32 / 10.0;
                [bowl.u(t) + a.cos() * ru, v + a.sin() * radius]
            }).collect());
        }
    }
}

/// A strip laid along a meridian of the bowl, `width(v)` wide, from `v0` to `v1`, centred on
/// `theta`.
fn meridian_strip(bowl: &Bowl, theta: f32, v0: f32, v1: f32, width: impl Fn(f32) -> f32) -> Vec<P2> {
    let n = 24;
    let c = bowl.u(theta);
    let side = |sign: f32| (0..=n).map(move |k| {
        let v = v0 + (v1 - v0) * k as f32 / n as f32;
        (sign, v)
    });
    let mut pts: Vec<P2> = side(1.0).map(|(_, v)| [c + bowl.u_span(width(v) * 0.5, v), v]).collect();
    let back: Vec<P2> = side(-1.0).map(|(_, v)| [c - bowl.u_span(width(v) * 0.5, v), v]).collect();
    pts.extend(back.into_iter().rev());
    pts
}

/// A strip round the bowl from `v0` to `v1` (`v` may vary with `u` through `drop`), the whole
/// way round.
fn ring_strip(bowl: &Bowl, v0: f32, v1: f32, du: f32) -> Vec<P2> {
    let seam = bowl.seam();
    let _ = du;
    vec![[-seam, v0], [seam, v0], [seam, v1], [-seam, v1]]
}

fn build_helm(s: &HelmState) -> ItemMesh {
    let sh = &s.shell;
    let f = &s.fittings;
    let r = sh.radius;
    let bowl = Bowl::new(&profile_control(sh), sh);
    let seam = bowl.seam();
    let thick = 0.04;
    // Grid columns land on every facet ridge and on the seam.
    let target = 0.065;
    let du = match bowl.facets {
        Some(n) => {
            let facet = 2.0 * seam / n as f32;
            let k = ((facet / target).ceil() as usize).max(2).next_multiple_of(2);
            facet / k as f32
        }
        None => {
            let cols = ((2.0 * seam / target).round() as usize).max(8).next_multiple_of(2);
            2.0 * seam / cols as f32
        }
    };
    let cell = [du, target];
    let h = 1e-4;
    let seam_edge = |a: P2, b: P2| {
        let on_seam = (a[0].abs() - seam).abs() < 1e-4 && (b[0].abs() - seam).abs() < 1e-4;
        let on_crown = a[1].abs() < 1e-5 && b[1].abs() < 1e-5;
        on_seam || on_crown
    };
    let mut parts = Parts::default();

    // The shell, its outer skin on the surface.
    let (outline, holes) = shell_outline(s, &bowl, du);
    let options = PlateOptions { thickness: thick, offset: -thick * 0.5, cell, origin: [0.0, 0.0], crease: 0.2, open_edge: Some(&seam_edge), uv_rect: None };
    parts.add("shell", plate(&outline, &holes, &bowl, &options).merged(), sh.material);

    // A rolled edge round the rim of plate helms.
    if !sh.style.is_cap() {
        let mut rim: Vec<P2> = outline[2..].to_vec();
        rim.pop();
        let bead = regular_polygon(10, thick * 0.75, 0.0);
        parts.add("rolled_edge", sweep_on_surface(&bowl, &rim, true, -thick * 0.5, &bead, h), sh.material);
    }

    let overlay = 0.03;
    let on_top = |t: f32| PlateOptions { thickness: t, offset: t * 0.5 - 0.004, cell: [du * 0.5, target * 0.5], crease: 0.2, open_edge: Some(&seam_edge), uv_rect: None, origin: [0.0, 0.0] };
    let mut fittings = MeshBuilder::new();
    let mut rivet_at: Vec<P2> = Vec::new();
    let rivet = rivet_head(0.03 * r + 0.01, 10);

    // Brow band round a cap's rim.
    let band_w = sh.height * 0.13;
    let brow = f.browband && sh.style.is_cap();
    if brow {
        fittings.append(&plate(&ring_strip(&bowl, bowl.rim - band_w, bowl.rim + 0.005, du), &[], &bowl, &on_top(overlay)).merged());
    }
    // Rivets round the brow band, or just above the rim.
    if f.rivets > 0 {
        let v = if brow { bowl.rim - band_w * 0.5 } else { bowl.rim - 0.07 };
        for k in 0..f.rivets {
            let theta = TAU * k as f32 / f.rivets as f32 + 0.13;
            let wrapped = (theta + PI).rem_euclid(TAU) - PI;
            let drop = rim_drop(s, &bowl, wrapped);
            // Keep clear of face openings and the barbute's T.
            let open = drop < -0.01 || (sh.style == HelmStyle::Barbute && wrapped.abs() < 0.8);
            if !open {
                rivet_at.push([bowl.u(wrapped), v + drop.max(0.0)]);
            }
        }
    }

    // Spangen bands up to a crown cap with a knob.
    if f.bands > 0 {
        let cap_v = (0..bowl.profile.len()).map(|k| bowl.profile[k]).find(|q| q[1] > r * 0.3).map_or(bowl.rim * 0.2, |q| q[0]);
        let bottom = if brow { bowl.rim - band_w + 0.02 } else { bowl.rim };
        for k in 0..f.bands {
            let theta = (k as f32 + 0.5) * TAU / f.bands as f32;
            let theta = (theta + PI).rem_euclid(TAU) - PI;
            let strip = meridian_strip(&bowl, theta, cap_v - 0.06, bottom, |v| 0.13 * r * (0.7 + 0.3 * v / bowl.rim));
            fittings.append(&plate(&strip, &[], &bowl, &on_top(overlay)).merged());
            for j in 1..=3 {
                let v = cap_v + (bottom - cap_v) * j as f32 / 3.6;
                rivet_at.push([bowl.u(theta), v]);
            }
        }
        let cap = ring_strip(&bowl, 0.0, cap_v, du);
        fittings.append(&plate(&cap, &[], &bowl, &on_top(overlay)).merged());
        let (top, up) = on_surface(&bowl, [0.0, 0.0], overlay, h);
        let mut knob = lathe(&spline_profile(&[[0.08 * r, 0.0], [0.06 * r, 0.06 * r], [0.03 * r, 0.12 * r], [0.045 * r, 0.16 * r], [0.0, 0.22 * r]], 12), &LatheOptions::new(12, Shading::SMOOTH));
        knob.place(vec3::sub(top, vec3::scale(up, 0.01)), up);
        fittings.append(&knob);
    }

    // The nasal, from the brow band down over the nose.
    if f.nasal && sh.style.is_cap() {
        let (v0, v1) = (bowl.rim - band_w * 0.9, bowl.v_at(NOSE * r * 1.05));
        let strip = meridian_strip(&bowl, 0.0, v0, v1, |v| {
            let t = ((v - v0) / (v1 - v0)).clamp(0.0, 1.0);
            r * (0.17 + 0.12 * smoothstep(0.4, 1.0, t))
        });
        let strip = fillet(&strip, 0.06, 0.3, 4);
        fittings.append(&plate(&strip, &[], &bowl, &PlateOptions { offset: overlay + 0.02 - 0.004, ..on_top(0.04) }).merged());
        rivet_at.push([0.0, bowl.rim - band_w * 0.5]);
    }

    // The great helm's cross: a strip down the front and a band above the slits.
    if sh.style == HelmStyle::GreatHelm {
        let v0 = bowl.v_at(sh.height * 0.35);
        let strip = meridian_strip(&bowl, 0.0, v0, bowl.rim + 0.01, |_| 0.13 * r);
        fittings.append(&plate(&strip, &[], &bowl, &on_top(overlay)).merged());
        let (b0, b1) = (bowl.v_at((EYE + 0.2) * r), bowl.v_at((EYE + 0.1) * r));
        fittings.append(&plate(&ring_strip(&bowl, b0, b1, du), &[], &bowl, &on_top(overlay)).merged());
        for k in 0..5 {
            let v = v0 + (bowl.rim - v0) * (k as f32 + 0.5) / 5.0;
            if (v - (b0 + b1) * 0.5).abs() > 0.08 {
                rivet_at.push([0.0, v]);
            }
        }
    }

    // A comb along the crown, front to back.
    if f.comb > 0.0 {
        fittings.append(&comb(&bowl, s));
    }
    parts.add("fittings", fittings, f.material);

    // Cheek plates hinged from the rim.
    if s.cheeks.on && sh.style.is_cap() {
        let c = &s.cheeks;
        let mut cheeks = MeshBuilder::new();
        for side in [-1.0f32, 1.0] {
            let centre = side * 1.25;
            let (v0, v1) = (bowl.rim - if brow { band_w * 0.6 } else { 0.05 }, bowl.rim + c.length);
            let half = |t: f32| c.width * 0.5 * (1.0 - 0.35 * t);
            // A rounded plate, its front edge swept back toward the chin.
            let n = 16;
            let mut pts: Vec<P2> = Vec::new();
            for k in 0..=n {
                let t = k as f32 / n as f32;
                let v = v0 + (v1 - v0) * t;
                pts.push([bowl.u(centre) - side * bowl.u_span(half(t), v), v]);
            }
            for k in (0..=n).rev() {
                let t = k as f32 / n as f32;
                let v = v0 + (v1 - v0) * t;
                pts.push([bowl.u(centre) + side * bowl.u_span(half(t) * (1.0 - 0.25 * t * t), v), v]);
            }
            let pts = fillet(&pts, 0.12, 0.25, 6);
            let off = overlay + if brow { overlay } else { 0.0 };
            cheeks.append(&plate(&pts, &[], &bowl, &PlateOptions { offset: off + 0.015, ..on_top(0.03) }).merged());
            for dt in [-0.25, 0.25] {
                rivet_at.push([bowl.u(centre) + bowl.u_span(dt * c.width, v0 + 0.05), v0 + 0.06]);
            }
        }
        parts.add("cheeks", cheeks, c.material);
    }

    // The hounskull visor: a pointed snout on pivots, eye slits and breaths cut through it.
    if s.visor.style == VisorStyle::Snout {
        let top = bowl.v_at(0.12 * r);
        let bottom = bowl.v_at(-1.15 * r);
        let half = 0.95;
        let v_nose = bowl.v_at(NOSE * r * 0.85);
        let snout = Hung {
            bowl: &bowl,
            shape: |theta: f32, v: f32| {
                let (base, y) = bowl.section(v, 0.0);
                // A blunt cone pointing forward from the nose, flattening out to the edges.
                let rho = ((theta / 0.8).powi(2) + ((v - v_nose) / (0.62 * r)).powi(2)).sqrt();
                let reach = 0.5 * r * (1.0 - rho).max(0.0).powf(1.6);
                let clear = 0.05 + 0.08 * smoothstep(bowl.rim - 0.1, bowl.rim + 0.1, v);
                (base + clear + reach, y)
            },
        };
        let rect = vec![[bowl.u(-half), top], [bowl.u(half), top], [bowl.u(half), bottom], [bowl.u(-half), bottom]];
        let outline = fillet(&rect, 0.25, 0.3, 6);
        let mut holes: Vec<Vec<P2>> = Vec::new();
        for (t0, t1) in [(-0.62, -0.1), (0.1, 0.62)] {
            let (v0, v1) = (bowl.v_at((EYE + 0.04) * r), bowl.v_at((EYE - 0.04) * r));
            let rect = vec![[bowl.u(t0), v0], [bowl.u(t1), v0], [bowl.u(t1), v1], [bowl.u(t0), v1]];
            holes.push(fillet(&rect, 0.035, 0.3, 4));
        }
        breaths(&mut holes, &bowl, s.visor.breaths, (NOSE - 0.15) * r, &[-0.45, 0.45], 0.028 * r);
        let visor = plate(&outline, &holes, &snout, &PlateOptions { thickness: thick, offset: 0.0, cell, crease: 0.2, ..Default::default() });
        parts.add("visor", visor.merged(), sh.material);
        for side in [-1.0f32, 1.0] {
            rivet_at.push([bowl.u(side * (half + 0.12)), bowl.v_at(0.0)]);
        }
    }

    // A kettle hat's brim, drooping, with a rolled edge.
    if sh.brim > 0.0 && sh.style.is_cap() {
        let (rim_r, rim_y) = bowl.section(bowl.rim, 0.0);
        let (b, droop) = (sh.brim, sh.droop);
        let brim = Hung {
            bowl: &bowl,
            shape: move |_theta: f32, v: f32| {
                let t = (v / b).max(0.0);
                (rim_r + v, rim_y - droop * b * 0.8 * t.powf(1.4) - 0.01)
            },
        };
        let ring = vec![[-seam, -0.06], [seam, -0.06], [seam, b], [-seam, b]];
        let open = |a: P2, c: P2| (a[0].abs() - seam).abs() < 1e-4 && (c[0].abs() - seam).abs() < 1e-4;
        let plate_opts = PlateOptions { thickness: 0.035, offset: 0.0, cell: [du, 0.06], crease: 0.2, open_edge: Some(&open), uv_rect: None, origin: [0.0, 0.0] };
        let mut mesh = plate(&ring, &[], &brim, &plate_opts).merged();
        let steps = (2.0 * seam / du).round() as usize;
        let edge: Vec<P2> = (0..steps).map(|k| [seam - 2.0 * seam * k as f32 / steps as f32, b]).collect();
        mesh.append(&sweep_on_surface(&brim, &edge, true, 0.0, &regular_polygon(10, 0.035, 0.0), h));
        parts.add("brim", mesh, sh.material);
    }

    // The mail aventail, open at the face unless a visor closes it.
    if s.aventail.on {
        let av = &s.aventail;
        // Closed round under a visor; open at the face; only behind cheek plates when they hang over it.
        let gap = if s.cheeks.on {
            1.55
        } else if s.visor.style == VisorStyle::Snout {
            0.0
        } else {
            0.62
        };
        let attach = bowl.rim - 0.08;
        let (len, flare) = (av.length, av.flare);
        let b = &bowl;
        let hang = Hung {
            bowl: b,
            shape: move |theta: f32, v: f32| {
                let drop = rim_drop(s, b, theta).max(0.0);
                let (rr, y) = b.section(attach + drop, theta);
                let t = (v / len).clamp(0.0, 1.0);
                // Mail falls in soft folds that deepen toward the hem.
                let folds = 0.035 * r * t * (theta * 14.0).sin();
                (rr + 0.035 + (0.04 + 0.35 * flare) * r * t.powf(1.3) + folds, y - v)
            },
        };
        let open = |a: P2, c: P2| (a[0].abs() - seam).abs() < 1e-4 && (c[0].abs() - seam).abs() < 1e-4;
        let (u0, u1) = if gap > 0.0 { (bowl.u(TAU - gap), bowl.u(gap)) } else { (-seam, seam) };
        let open_edge: Option<&dyn Fn(P2, P2) -> bool> = if gap > 0.0 { None } else { Some(&open) };
        let options = PlateOptions { thickness: 0.035, offset: 0.0, cell: [du, 0.07], origin: [0.0, 0.0], crease: 0.3, open_edge, uv_rect: None };
        let rect = |v0: f32, v1: f32| {
            let rect = vec![[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
            if gap > 0.0 { fillet(&rect, 0.08, 0.3, 4) } else { rect }
        };
        let mail = SurfacePattern { kind: PatternKind::Mail, color: Rgb::hex(0x232323), scale: av.ring_size };
        parts.add_look("aventail", plate(&rect(0.0, len), &[], &hang, &options).merged(), Look::patterned(av.material, mail));
        // A cloth hem along the lower edge, wrapped round it.
        if s.hem.on && s.hem.hemline {
            let w = (len * 0.3).min(s.hem.width * 0.5);
            let trim = PlateOptions { thickness: 0.06, ..options };
            parts.add_look("hem", plate(&rect(len - w, len + 0.01), &[], &hang, &trim).merged(), Look::cloth(&s.hem.fabric));
        }
    }

    // Crest.
    let crown = on_surface(&bowl, [0.0, 0.0], 0.0, h).0;
    match s.crest.style {
        CrestStyle::Plume => {
            let (plume, holder) = plume(&s.crest, crown, r);
            parts.add("crest_holder", holder, f.material);
            parts.add_tinted("crest", plume, Cloth, s.crest.color.rgb());
        }
        CrestStyle::Horns => {
            parts.add("crest", horns(&s.crest, &bowl, sh), s.crest.material);
        }
        CrestStyle::None => {}
    }

    parts.add("rivets", scatter_on_surface(&bowl, &rivet, &rivet_at, overlay - 0.006, h), f.material);
    parts.finish()
}

/// The comb: a fin standing on the crown from the brow to the nape.
fn comb(bowl: &Bowl, s: &HelmState) -> MeshBuilder {
    let height = s.fittings.comb;
    let n = 40;
    // Walk the sagittal line: front rim up over the crown and down the back.
    let front_v = bowl.rim * 0.92;
    let pts: Vec<(V3, V3)> = (0..=n)
        .map(|k| {
            let t = k as f32 / n as f32;
            let (u, v) = if t < 0.5 { (0.0, front_v * (1.0 - 2.0 * t)) } else { (bowl.seam(), front_v * (2.0 * t - 1.0)) };
            on_surface(bowl, [u, v], 0.0, 1e-4)
        })
        .collect();
    let mut lower: Vec<P2> = Vec::new();
    let mut upper: Vec<P2> = Vec::new();
    for (k, (p, nrm)) in pts.iter().enumerate() {
        let t = k as f32 / n as f32;
        let rise = height * (PI * t).sin().max(0.0).powf(0.5) + 0.02;
        lower.push([p[2] - nrm[2] * 0.03, p[1] - nrm[1] * 0.03]);
        upper.push([p[2] + nrm[2] * rise, p[1] + nrm[1] * rise]);
    }
    let mut outline = lower;
    outline.extend(upper.into_iter().rev());
    let outline = fillet(&outline, 0.03, 0.4, 3);
    let mut fin = extrude(&outline, &[], 0.035);
    // The outline is drawn in (z, y); turn its extrusion axis onto x.
    fin.transform(&Mat3::from_cols([0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]));
    fin
}

/// A plume of feathers springing from a socket at the crown, and the socket.
fn plume(c: &Crest, crown: V3, r: f32) -> (MeshBuilder, MeshBuilder) {
    let n = c.count.max(1);
    let l = c.size;
    let mut feathers = MeshBuilder::new();
    let section: Vec<P2> = (0..12).map(|k| {
        let a = TAU * k as f32 / 12.0;
        [a.cos() * 0.22, a.sin()]
    }).collect();
    let y = crown[1] + 0.18 * r;
    for k in 0..n {
        // Fanned out side to side, the middle feathers standing tallest.
        let sp = if n == 1 { 0.0 } else { (k as f32 / (n - 1) as f32 - 0.5) * 1.4 };
        let tall = 1.0 - 0.25 * (sp / 0.7).powi(2);
        let control = vec![
            [0.0, y - 0.05, 0.0],
            [sp.sin() * l * 0.22, y + l * 0.4 * tall, -l * 0.05],
            [sp.sin() * l * 0.45, y + l * 0.62 * tall, -l * 0.42],
            [sp.sin() * l * 0.62, y + l * 0.38 * tall, -l * 0.92],
        ];
        let path = CatmullRom::centripetal(control).sample(12);
        let width = |t: f32| l * 0.085 * (PI * (t * 1.1 + 0.05).min(1.0)).sin().max(0.0) + 0.012;
        feathers.append(&sweep(&path, &section, &SweepOptions { up: Some(vec3::X), ..Default::default() }, width));
    }
    let mut holder = lathe(&[[0.07 * r, 0.0], [0.08 * r, 0.04 * r], [0.12 * r, 0.22 * r], [0.1 * r, 0.25 * r]], &LatheOptions::new(12, Shading::SMOOTH));
    holder.translate([crown[0], crown[1] - 0.04, crown[2]]);
    (feathers, holder)
}

/// Two horns sweeping out, up and forward from the sides of the bowl.
fn horns(c: &Crest, bowl: &Bowl, sh: &HelmShell) -> MeshBuilder {
    let l = c.size;
    let y0 = sh.height * 0.45;
    let v0 = bowl.v_at(y0);
    let r0 = bowl.r_at(v0);
    let mut out = MeshBuilder::new();
    for side in [1.0f32, -1.0] {
        let control = vec![
            [side * r0 * 0.8, y0, 0.0],
            [side * (r0 + l * 0.35), y0 + l * 0.15, 0.0],
            [side * (r0 + l * 0.6), y0 + l * 0.55, l * 0.08],
            [side * (r0 + l * 0.45), y0 + l * 0.95, l * 0.15],
        ];
        let path = CatmullRom::centripetal(control).sample(16);
        let base = sh.radius * 0.2;
        out.append(&tube(&path, 14, &SweepOptions::default(), |t| base * (1.0 - t).max(0.0).powf(0.8) + 0.01));
    }
    out
}

// ───────────────────────── Forge new ─────────────────────────

fn compose(rng: &mut Mulberry32, seed: u32, style: HelmStyle) -> HelmState {
    use HelmStyle as S;
    let mut s = HelmState { seed, ..HelmState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    s.nudge(rng, &LIMITS, 0.75, 0.3);

    let sh = &mut s.shell;
    sh.style = style;
    sh.radius = rng.range_f32(0.95, 1.12);
    sh.height = match style {
        S::Kettle => rng.range_f32(1.15, 1.45),
        S::Nasal => rng.range_f32(1.35, 1.75),
        S::Bascinet => rng.range_f32(1.45, 1.75),
        S::GreatHelm => rng.range_f32(1.6, 1.85),
        S::Sallet => rng.range_f32(1.3, 1.5),
        S::Skullcap => rng.range_f32(1.4, 1.7),
        S::Barbute => rng.range_f32(1.5, 1.75),
    };
    sh.point = match style {
        S::Nasal => rng.range_f32(0.4, 0.95),
        S::Bascinet => rng.range_f32(0.45, 1.0),
        S::GreatHelm => rng.range_f32(0.0, 0.3),
        S::Barbute => rng.range_f32(0.1, 0.45),
        _ => rng.range_f32(0.1, 0.5),
    };
    sh.facets = match style {
        S::Nasal => *rng.weighted(&[(4, 3), (6, 2), (8, 2), (24, 2)]),
        S::Skullcap => *rng.weighted(&[(8, 1), (12, 2), (24, 2), (32, 1)]),
        S::Kettle => *rng.weighted(&[(8, 1), (24, 3), (28, 2)]),
        _ => *rng.pick(&[24, 28, 32]),
    };
    sh.brim = if style == S::Kettle { rng.range_f32(0.4, 1.0) } else { 0.0 };
    sh.droop = rng.range_f32(0.2, 0.7);
    sh.tail = if style == S::Sallet { rng.range_f32(0.5, 1.0) } else { 0.0 };
    sh.material = match style {
        S::Skullcap => *rng.weighted(&[(Iron, 6), (Brass, 1), (Leather, 2)]),
        S::Nasal | S::Kettle => *rng.weighted(&[(Iron, 8), (Brass, 1)]),
        _ => *rng.weighted(&[(Iron, 12), (Brass, 1), (Silver, 1)]),
    };
    let shell = *sh;

    let f = &mut s.fittings;
    f.browband = match style {
        S::Kettle => rng.chance(0.7),
        S::Nasal => true,
        S::Skullcap => rng.chance(0.75) || shell.material == Leather,
        _ => false,
    };
    f.bands = match style {
        S::Nasal if shell.facets <= 8 => shell.facets,
        S::Nasal => *rng.pick(&[0, 4]),
        S::Skullcap if shell.material == Leather => *rng.pick(&[2, 4]),
        S::Skullcap => *rng.pick(&[0, 0, 2, 4]),
        _ => 0,
    };
    f.nasal = style == S::Nasal || (style == S::Skullcap && rng.chance(0.5));
    f.comb = match style {
        S::Sallet if rng.chance(0.5) => rng.range_f32(0.05, 0.15),
        S::Kettle | S::Barbute if rng.chance(0.25) => rng.range_f32(0.06, 0.2),
        _ => 0.0,
    };
    f.rivets = match style {
        S::Kettle | S::Nasal | S::Skullcap => rng.int(8, 16),
        S::Bascinet => rng.int(12, 20),
        _ => rng.int(6, 14),
    };
    let accent = shell.material == Iron && rng.chance(0.4);
    f.material = if shell.material == Leather { Iron } else if accent { Brass } else { shell.material };

    s.visor = match style {
        S::GreatHelm => Visor { style: VisorStyle::Slit, breaths: rng.int(6, 12) },
        S::Sallet if rng.chance(0.6) => Visor { style: VisorStyle::Slit, breaths: if rng.chance(0.4) { rng.int(3, 6) } else { 0 } },
        S::Bascinet if rng.chance(0.5) => Visor { style: VisorStyle::Snout, breaths: rng.int(4, 12) },
        _ => Visor { style: VisorStyle::None, breaths: 0 },
    };

    s.cheeks.on = match style {
        S::Nasal => rng.chance(0.35),
        S::Skullcap => rng.chance(0.3),
        _ => false,
    };
    s.cheeks.length = shell.radius * rng.range_f32(0.8, 1.2);
    s.cheeks.width = rng.range_f32(0.55, 0.8);
    s.cheeks.material = if shell.material == Leather { Iron } else { shell.material };

    s.aventail.on = match style {
        S::Bascinet => rng.chance(0.85),
        S::Nasal => rng.chance(0.6),
        S::Skullcap => rng.chance(0.4),
        S::Kettle => rng.chance(0.2),
        _ => false,
    };
    s.aventail.length = rng.range_f32(0.9, 1.9);
    s.aventail.flare = rng.range_f32(0.2, 0.5);
    s.aventail.ring_size = rng.range_f32(5.5, 9.0);
    s.aventail.material = *rng.weighted(&[(Iron, 9), (Brass, 1)]);
    s.hem.on = s.aventail.on && rng.chance(if style == S::Bascinet { 0.45 } else { 0.25 });
    s.hem.hemline = true;
    s.hem.width = rng.range_f32(0.15, 0.35);
    s.hem.fabric = Fabric {
        color: *rng.pick(&Fabric::TRIM_DYES),
        pattern: *rng.weighted(&[(FabricPattern::Plain, 4), (FabricPattern::Chevrons, 1), (FabricPattern::Stripes, 1)]),
        color2: *rng.pick(&Fabric::PATTERN_DYES),
        scale: rng.range_f32(2.0, 6.0),
    };

    let c = &mut s.crest;
    c.style = match style {
        S::Skullcap => *rng.weighted(&[(CrestStyle::None, 5), (CrestStyle::Horns, 4), (CrestStyle::Plume, 1)]),
        S::GreatHelm => *rng.weighted(&[(CrestStyle::None, 5), (CrestStyle::Plume, 3), (CrestStyle::Horns, 1)]),
        S::Nasal => *rng.weighted(&[(CrestStyle::None, 8), (CrestStyle::Horns, 1), (CrestStyle::Plume, 1)]),
        _ => *rng.weighted(&[(CrestStyle::None, 8), (CrestStyle::Plume, 2)]),
    };
    c.size = match c.style {
        CrestStyle::Horns => rng.range_f32(1.0, 1.8),
        _ => rng.range_f32(1.2, 2.2),
    };
    c.count = rng.int(3, 7);
    c.color = *rng.pick(&[Tincture::Gules, Tincture::Argent, Tincture::Azure, Tincture::Or, Tincture::Sable]);
    c.material = *rng.weighted(&[(Bone, 6), (Iron, 1), (Gold, 1)]);

    let mut targets = vec![(EtchTarget::Metals, 3)];
    if shell.material == Leather {
        targets.push((EtchTarget::Leather, 2));
    }
    s.etch = Etching::roll(rng, 0.35, &targets, 1);

    s.fit(&LIMITS);
    s
}
