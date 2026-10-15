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
//! - [`Helm::build_with_openings`] cuts every part open where something on the head comes out
//!   through it (a goblin's ears, a lizard's snout): holes and notches in the shell, holes,
//!   slots and gaps in the brim, aventail and visor, cheek plates moved forward of it.
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
fn shell_outline(s: &HelmState, bowl: &Bowl, du: f32, cuts: &Cuts, margin: f32) -> (Vec<P2>, Vec<Vec<P2>>) {
    let sh = &s.shell;
    let r = sh.radius;
    let seam = bowl.seam();
    let steps = ((2.0 * seam) / (du * 0.5)).ceil() as usize;
    let rim_at = |u: f32| cuts.rim(u, bowl.rim + rim_drop(s, bowl, bowl.theta(u)), margin);
    // The rim from the seam on the wearer's right (u = +seam), round the front, to the left.
    // A barbute's rim lifted over something (a snout) opens right over its T: the notch is
    // widened to take the T in, and the rim follows the notch alone.
    let (slot, eyes) = (0.2, 0.68);
    let lifted;
    let (cuts, rim_at): (&Cuts, Box<dyn Fn(f32) -> f32>) = if sh.style == HelmStyle::Barbute && !cuts.notches.is_empty() {
        let (ue, top) = (bowl.u(-eyes).abs(), bowl.v_at(-0.04 * r));
        let mut wide = cuts.clone();
        for n in wide.notches.iter_mut().filter(|n| n.0 < ue && n.1 > -ue) {
            *n = (n.0.min(-ue - margin), n.1.max(ue + margin), n.2.min(top));
        }
        lifted = wide;
        (&lifted, Box::new(|u: f32| lifted.rim(u, bowl.rim, margin)))
    } else {
        (cuts, Box::new(rim_at))
    };
    let mut rim: Vec<P2> = if sh.style == HelmStyle::Barbute && cuts.notches.is_empty() {
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
    // Slits and breaths give way to openings cut for something coming through.
    let bounds = |h: &[P2]| h.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(l, u), p| ([l[0].min(p[0]), l[1].min(p[1])], [u[0].max(p[0]), u[1].max(p[1])]));
    holes.retain(|h| {
        let (l, u) = bounds(h);
        let meets_hole = cuts.holes.iter().map(|c| bounds(c)).any(|(cl, cu)| cl[0] < u[0] + margin && cu[0] > l[0] - margin && cl[1] < u[1] + margin && cu[1] > l[1] - margin);
        let meets_notch = cuts.notches.iter().any(|n| n.0 - margin < u[0] && n.1 + margin > l[0] && n.2 - margin < u[1]);
        !meets_hole && !meets_notch
    });
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

/// Openings cut through a helm where something comes out through it, such as a goblin's
/// ears: each a set of points in item space. Points near a surface mark where it crosses,
/// and the opening is their convex hull on it, `margin` wider all round. Through the shell,
/// one in `through` is a hole, or a notch up from the rim when it comes near the rim; one in
/// `lift` (a lizard's snout and brow) always lifts the rim over it, however far above the rim
/// it crosses. Brims, aventails, visors and cheek plates give way to both. Our own: the plate
/// prototype draws features through its helms.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Openings {
    pub through: Vec<Vec<V3>>,
    pub lift: Vec<Vec<V3>>,
    pub margin: f32,
}

impl Openings {
    /// Every feature, whether through or lifted over.
    fn all(&self) -> impl Iterator<Item = &Vec<V3>> {
        self.through.iter().chain(&self.lift)
    }
}

impl Helm {
    /// [`Generator::build`] with `openings` cut through the shell.
    pub fn build_with_openings(s: &HelmState, openings: &Openings) -> ItemMesh {
        build_helm_cut(s, openings)
    }
}

/// What [`Openings`] cut in bowl coordinates: holes through the shell, and notches cut up from
/// the rim (`u` from, `u` to, and the height `v` they reach up to) where an opening comes
/// down to it.
#[derive(Clone, Debug, Default)]
struct Cuts {
    holes: Vec<Vec<P2>>,
    notches: Vec<(f32, f32, f32)>,
}

impl Cuts {
    /// How far up from the crown the rim reaches at `u`, notches cut into `rim`.
    fn rim(&self, u: f32, rim: f32, margin: f32) -> f32 {
        self.notches.iter().fold(rim, |v, &(u0, u1, top)| {
            // Straight sides eased over the margin.
            let inside = smoothstep(u0 - margin, u0, u) * (1.0 - smoothstep(u1, u1 + margin, u));
            v.min(rim + (top - rim) * inside)
        })
    }

    /// The spans of `u` cut away from a band round the bowl between `v0` and `v1`.
    fn gaps(&self, v0: f32, v1: f32, margin: f32) -> Vec<(f32, f32)> {
        let mut out: Vec<(f32, f32)> = self
            .holes
            .iter()
            .filter(|h| h.iter().any(|p| p[1] >= v0 - margin) && h.iter().any(|p| p[1] <= v1 + margin))
            .map(|h| h.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0]))))
            .collect();
        out.extend(self.notches.iter().filter(|n| n.2 <= v1 + margin).map(|n| (n.0 - margin, n.1 + margin)));
        out
    }

    /// Whether a fitting at `p` would sit in or at the edge of a cut.
    fn hits(&self, p: P2, margin: f32) -> bool {
        self.gaps(p[1], p[1], margin).iter().any(|&(u0, u1)| p[0] > u0 - margin && p[0] < u1 + margin)
    }
}

/// A band round the bowl from `v0` to `v1` with `gaps` (spans of `u`) left out, as strips.
fn ring_strips(bowl: &Bowl, v0: f32, v1: f32, gaps: &[(f32, f32)]) -> Vec<Vec<P2>> {
    let seam = bowl.seam();
    let mut cuts: Vec<(f32, f32)> = gaps.iter().map(|&(a, b)| (a.max(-seam), b.min(seam))).filter(|g| g.1 > g.0).collect();
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut strips = Vec::new();
    let mut from = -seam;
    for (a, b) in cuts {
        if a > from + 0.02 {
            strips.push(vec![[from, v0], [a, v0], [a, v1], [from, v1]]);
        }
        from = from.max(b);
    }
    if seam > from + 0.02 {
        strips.push(vec![[from, v0], [seam, v0], [seam, v1], [from, v1]]);
    }
    strips
}

/// The holes and notches `openings` cut, in bowl coordinates.
fn opening_cuts(s: &HelmState, bowl: &Bowl, openings: &Openings) -> Cuts {
    let seam = bowl.seam();
    let m = openings.margin;
    let mut cuts = Cuts::default();
    for (k, points) in openings.all().enumerate() {
        let lift = k >= openings.through.len();
        // Where the points pass through the bowl's surface, in bowl coordinates.
        let mut uv: Vec<P2> = points
            .iter()
            .filter_map(|p| {
                let theta = p[0].atan2(p[2] / bowl.depth);
                let v = bowl.v_at(p[1]);
                let (r, _) = bowl.section(v, theta);
                let r = r * bowl.facet(theta, v);
                let rp = p[0].hypot(p[2] / bowl.depth);
                ((rp - r).abs() < 0.12 + m).then(|| [bowl.u(theta), v])
            })
            .collect();
        if uv.len() < 3 {
            continue;
        }
        let hull = hull_2d(&mut uv);
        if hull.len() < 3 {
            continue;
        }
        let grown: Vec<P2> = grow(&hull, m).into_iter().map(|p| [p[0].clamp(-seam + 0.03, seam - 0.03), p[1].max(0.03)]).collect();
        // Clear of the rim it is a hole; reaching down near the rim it is a notch cut up
        // from it to the opening's top.
        let (u0, u1) = grown.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
        let (v0, v1) = grown.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
        let rim = (0..=16).map(|k| bowl.rim + rim_drop(s, bowl, bowl.theta(u0 + (u1 - u0) * k as f32 / 16.0))).fold(f32::MAX, f32::min);
        if !lift && (v1 < rim - 0.12 || s.shell.style == HelmStyle::Barbute) {
            let clipped = clip_below(&grown, rim - 0.04);
            if clipped.len() >= 3 {
                cuts.holes.push(clipped);
            }
        } else {
            cuts.notches.push((u0, u1, v0));
        }
    }
    cuts
}

/// The convex hull `hull` grown by `m` all round: the hull of it moved `m` every way (an
/// octagon about each corner).
fn grow(hull: &[P2], m: f32) -> Vec<P2> {
    let mut points: Vec<P2> = hull
        .iter()
        .flat_map(|p| (0..8).map(move |k| {
            let a = TAU * k as f32 / 8.0;
            [p[0] + m * a.cos(), p[1] + m * a.sin()]
        }))
        .collect();
    hull_2d(&mut points)
}

/// The convex hull of `points` (Andrew's monotone chain), counterclockwise.
fn hull_2d(points: &mut [P2]) -> Vec<P2> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    let cross = |o: P2, a: P2, b: P2| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let chain = |pts: &mut dyn Iterator<Item = P2>| {
        let mut out: Vec<P2> = Vec::new();
        for p in pts {
            while out.len() >= 2 && cross(out[out.len() - 2], out[out.len() - 1], p) <= 0.0 {
                out.pop();
            }
            out.push(p);
        }
        out.pop();
        out
    };
    let mut hull = chain(&mut points.iter().copied());
    hull.extend(chain(&mut points.iter().rev().copied()));
    hull
}

/// `polygon` cut off at `v = limit`, keeping the part with smaller `v` (Sutherland-Hodgman).
fn clip_below(polygon: &[P2], limit: f32) -> Vec<P2> {
    let mut out = Vec::new();
    for (i, &a) in polygon.iter().enumerate() {
        let b = polygon[(i + 1) % polygon.len()];
        let (ia, ib) = (a[1] <= limit, b[1] <= limit);
        if ia {
            out.push(a);
        }
        if ia != ib {
            let t = (limit - a[1]) / (b[1] - a[1]);
            out.push([a[0] + (b[0] - a[0]) * t, limit]);
        }
    }
    out
}

/// `polygon` cut off at `u = limit`, keeping the side toward `keep` (-1 smaller `u`, +1
/// larger; Sutherland-Hodgman).
fn clip_u(polygon: &[P2], limit: f32, keep: f32) -> Vec<P2> {
    let mut out = Vec::new();
    let inside = |p: P2| (p[0] - limit) * keep >= 0.0;
    for (i, &a) in polygon.iter().enumerate() {
        let b = polygon[(i + 1) % polygon.len()];
        if inside(a) {
            out.push(a);
        }
        if inside(a) != inside(b) {
            let t = (limit - a[0]) / (b[0] - a[0]);
            out.push([limit, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

/// Where `points` come within `reach` of `surface` over its coordinates `lo..hi`, as the
/// coordinates of the nearest of its samples (`step` apart).
fn near_on(surface: &impl Surface, lo: P2, hi: P2, step: P2, points: &[V3], reach: f32) -> Vec<P2> {
    use std::collections::HashMap;
    let cell = |p: V3| [(p[0] / reach).floor() as i32, (p[1] / reach).floor() as i32, (p[2] / reach).floor() as i32];
    let (nu, nv) = (((hi[0] - lo[0]) / step[0]).ceil().max(1.0) as usize, ((hi[1] - lo[1]) / step[1]).ceil().max(1.0) as usize);
    let mut grid: HashMap<[i32; 3], Vec<(V3, P2)>> = HashMap::new();
    for i in 0..=nu {
        for j in 0..=nv {
            let uv = [lo[0] + (hi[0] - lo[0]) * i as f32 / nu as f32, lo[1] + (hi[1] - lo[1]) * j as f32 / nv as f32];
            let p = surface.point(uv);
            grid.entry(cell(p)).or_default().push((p, uv));
        }
    }
    points
        .iter()
        .filter_map(|&p| {
            let c = cell(p);
            let mut best: Option<(f32, P2)> = None;
            for d in 0..27 {
                let k = [c[0] + d % 3 - 1, c[1] + (d / 3) % 3 - 1, c[2] + d / 9 - 1];
                for &(q, uv) in grid.get(&k).into_iter().flatten() {
                    let dist = vec3::distance(p, q);
                    if dist < reach && best.is_none_or(|b| dist < b.0) {
                        best = Some((dist, uv));
                    }
                }
            }
            best.map(|b| b.1)
        })
        .collect()
}

/// What [`Openings`] cut in a sheet hung off the bowl (a brim, an aventail, a visor), in its
/// own coordinates over `lo..hi`: holes through it, notches cut in from its low-`v` and
/// high-`v` edges (`u` from, `u` to, and the `v` they reach), and spans of `u` cut away
/// right across it. Our own, as [`Cuts`] is for the shell.
#[derive(Clone, Debug, Default)]
struct SheetCuts {
    lo: P2,
    hi: P2,
    holes: Vec<Vec<P2>>,
    from_low: Vec<(f32, f32, f32)>,
    from_high: Vec<(f32, f32, f32)>,
    across: Vec<(f32, f32)>,
    margin: f32,
}

/// One piece of a sheet after [`SheetCuts`]: its outline, holes, and its two long edges
/// (low `v`, then high `v`, each running toward larger `u`).
struct SheetPiece {
    outline: Vec<P2>,
    holes: Vec<Vec<P2>>,
    high_edge: Vec<P2>,
    /// The whole sheet, uncut across (a ring stays a ring).
    whole: bool,
}

impl SheetCuts {
    /// The cuts `openings` make in `surface` over `lo..hi`. `wrap` when `u` runs the whole
    /// way round (a brim), so nothing is trimmed off its ends.
    fn new(surface: &impl Surface, lo: P2, hi: P2, openings: &Openings, wrap: bool) -> SheetCuts {
        let m = openings.margin;
        let mut cuts = SheetCuts { lo, hi, margin: m.max(0.04), ..SheetCuts::default() };
        if openings.all().next().is_none() {
            return cuts;
        }
        let step = [0.03, 0.03];
        for points in openings.all() {
            let mut uv = near_on(surface, lo, hi, step, points, 0.06 + m);
            if uv.len() < 3 {
                continue;
            }
            let hull = hull_2d(&mut uv);
            if hull.len() < 3 {
                continue;
            }
            let grown = grow(&hull, m);
            let (u0, u1) = grown.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
            let (v0, v1) = grown.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
            let edge = 0.5 * cuts.margin;
            let (low, high) = (v0 < lo[1] + edge, v1 > hi[1] - edge);
            let ends = !wrap && (u0 < lo[0] + edge || u1 > hi[0] - edge);
            match (low, high) {
                _ if ends => cuts.across.push((u0, u1)),
                (true, true) => cuts.across.push((u0, u1)),
                (true, false) => cuts.from_low.push((u0, u1, v1)),
                (false, true) => cuts.from_high.push((u0, u1, v0)),
                (false, false) => cuts.holes.push(grown),
            }
        }
        cuts
    }

    /// The low-`v` edge at `u`, notches cut in.
    fn low(&self, u: f32) -> f32 {
        let m = self.margin;
        self.from_low.iter().fold(self.lo[1], |v, &(u0, u1, to)| {
            let inside = smoothstep(u0 - m, u0, u) * (1.0 - smoothstep(u1, u1 + m, u));
            v.max(self.lo[1] + (to - self.lo[1]) * inside)
        })
    }

    /// The high-`v` edge at `u`, notches cut in.
    fn high(&self, u: f32) -> f32 {
        let m = self.margin;
        self.from_high.iter().fold(self.hi[1], |v, &(u0, u1, to)| {
            let inside = smoothstep(u0 - m, u0, u) * (1.0 - smoothstep(u1, u1 + m, u));
            v.min(self.hi[1] + (to - self.hi[1]) * inside)
        })
    }

    /// Whether anything is cut.
    fn is_empty(&self) -> bool {
        self.holes.is_empty() && self.from_low.is_empty() && self.from_high.is_empty() && self.across.is_empty()
    }

    /// Whether a hole or notch comes within `pad` of the rectangle `a..b`.
    fn meets(&self, a: P2, b: P2, pad: f32) -> bool {
        let boxes = self
            .holes
            .iter()
            .map(|h| h.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(l, u), p| ([l[0].min(p[0]), l[1].min(p[1])], [u[0].max(p[0]), u[1].max(p[1])])))
            .chain(self.from_low.iter().map(|n| ([n.0, self.lo[1]], [n.1, n.2])))
            .chain(self.from_high.iter().map(|n| ([n.0, n.2], [n.1, self.hi[1]])))
            .chain(self.across.iter().map(|n| ([n.0, self.lo[1]], [n.1, self.hi[1]])));
        boxes.into_iter().any(|(l, u)| l[0] < b[0] + pad && u[0] > a[0] - pad && l[1] < b[1] + pad && u[1] > a[1] - pad)
    }

    /// The sheet's pieces between `v0(u)` and `v1(u)` (its own edges, or a band of it such
    /// as an aventail's hem), its edges sampled every `du`. `wrap` as for [`SheetCuts::new`].
    fn pieces(&self, du: f32, wrap: bool, band: impl Fn(f32) -> (f32, f32)) -> Vec<SheetPiece> {
        let (a, b) = (self.lo[0], self.hi[0]);
        let span = b - a;
        let mut cut: Vec<(f32, f32)> = self.across.iter().map(|&(u0, u1)| (u0 - self.margin, u1 + self.margin)).collect();
        cut.sort_by(|x, y| x.0.total_cmp(&y.0));
        let ranges: Vec<(f32, f32)> = if cut.is_empty() {
            vec![(a, b)]
        } else if wrap {
            (0..cut.len()).map(|i| (cut[i].1, if i + 1 < cut.len() { cut[i + 1].0 } else { cut[0].0 + span })).filter(|r| r.1 > r.0 + 0.05).collect()
        } else {
            let mut out = vec![];
            let mut from = a;
            for &(u0, u1) in &cut {
                if u0 > from + 0.05 {
                    out.push((from, u0.min(b)));
                }
                from = from.max(u1);
            }
            if b > from + 0.05 {
                out.push((from, b));
            }
            out
        };
        let whole = cut.is_empty();
        let unwrap = |u: f32| if u > b { u - span } else { u };
        ranges
            .into_iter()
            .map(|(u0, u1)| {
                let n = (((u1 - u0) / du).ceil() as usize).max(2);
                let edge = |k: usize| {
                    let u = u0 + (u1 - u0) * k as f32 / n as f32;
                    let (lo, hi) = band(unwrap(u));
                    let lo = lo.max(self.low(unwrap(u)));
                    let hi = hi.min(self.high(unwrap(u)) + (hi - self.hi[1]).max(0.0)).max(lo + 0.02);
                    (u, lo, hi)
                };
                let samples: Vec<(f32, f32, f32)> = (0..=n).map(edge).collect();
                let mut outline: Vec<P2> = samples.iter().map(|&(u, lo, _)| [u, lo]).collect();
                let high_edge: Vec<P2> = samples.iter().map(|&(u, _, hi)| [u, hi]).collect();
                outline.extend(high_edge.iter().rev().copied());
                let holes = self
                    .holes
                    .iter()
                    .filter(|h| h.iter().all(|p| p[0] > u0 + 0.02 && p[0] < u1 - 0.02))
                    .filter(|h| h.iter().all(|p| {
                        let (lo, hi) = band(p[0]);
                        p[1] > lo + 0.02 && p[1] < hi - 0.02
                    }))
                    .cloned()
                    .collect();
                SheetPiece { outline, holes, high_edge, whole }
            })
            .collect()
    }
}

fn build_helm(s: &HelmState) -> ItemMesh {
    build_helm_cut(s, &Openings::default())
}

fn build_helm_cut(s: &HelmState, openings: &Openings) -> ItemMesh {
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
    let cuts = opening_cuts(s, &bowl, openings);
    let cut_margin = openings.margin.max(0.05);
    let (outline, mut holes) = shell_outline(s, &bowl, du, &cuts, cut_margin);
    holes.extend(cuts.holes.iter().cloned());
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
        for strip in ring_strips(&bowl, bowl.rim - band_w, bowl.rim + 0.005, &cuts.gaps(bowl.rim - band_w, bowl.rim, cut_margin)) {
            fittings.append(&plate(&strip, &[], &bowl, &on_top(overlay)).merged());
        }
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

    // How far down a strip `width` across at `theta` may run toward `to` before a notch cut
    // up from the rim (over a snout, say) stops it.
    let stop = |theta: f32, width: f32, to: f32| -> f32 {
        let c = bowl.u(theta);
        let w = bowl.u_span(width * 0.5, to) + cut_margin;
        let rim = [c - w, c, c + w].into_iter().map(|u| cuts.rim(u, bowl.rim, cut_margin)).fold(bowl.rim, f32::min);
        if rim < bowl.rim - 1e-3 { to.min(rim - band_w - 0.02) } else { to }
    };

    // Spangen bands up to a crown cap with a knob.
    if f.bands > 0 {
        let cap_v = (0..bowl.profile.len()).map(|k| bowl.profile[k]).find(|q| q[1] > r * 0.3).map_or(bowl.rim * 0.2, |q| q[0]);
        let bottom = if brow { bowl.rim - band_w + 0.02 } else { bowl.rim };
        for k in 0..f.bands {
            let theta = (k as f32 + 0.5) * TAU / f.bands as f32;
            let theta = (theta + PI).rem_euclid(TAU) - PI;
            let bottom = stop(theta, 0.13 * r, bottom);
            if bottom < cap_v + 0.1 {
                continue;
            }
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
    // (None over a snout come out under the rim.)
    if f.nasal && sh.style.is_cap() && stop(0.0, 0.29 * r, bowl.rim) >= bowl.rim {
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
        let bottom = stop(0.0, 0.13 * r, bowl.rim + 0.01);
        if bottom > v0 + 0.1 {
            let strip = meridian_strip(&bowl, 0.0, v0, bottom, |_| 0.13 * r);
            fittings.append(&plate(&strip, &[], &bowl, &on_top(overlay)).merged());
        }
        let (b0, b1) = (bowl.v_at((EYE + 0.2) * r), bowl.v_at((EYE + 0.1) * r));
        for strip in ring_strips(&bowl, b0, b1, &cuts.gaps(b0, b1, cut_margin)) {
            fittings.append(&plate(&strip, &[], &bowl, &on_top(overlay)).merged());
        }
        for k in 0..5 {
            let v = v0 + (bowl.rim - v0) * (k as f32 + 0.5) / 5.0;
            if (v - (b0 + b1) * 0.5).abs() > 0.08 {
                rivet_at.push([0.0, v]);
            }
        }
    }

    // A comb along the crown, front to back.
    if f.comb > 0.0 {
        let notched = cuts.rim(0.0, bowl.rim, cut_margin);
        let front_stop = if notched < bowl.rim - 1e-3 { notched - 2.0 * cut_margin } else { bowl.rim };
        fittings.append(&comb(&bowl, s, front_stop));
    }
    parts.add("fittings", fittings, f.material);

    // Cheek plates hinged from the rim.
    if s.cheeks.on && sh.style.is_cap() {
        let c = &s.cheeks;
        let mut cheeks = MeshBuilder::new();
        let (v0, v1) = (bowl.rim - if brow { band_w * 0.6 } else { 0.05 }, bowl.rim + c.length);
        // Where something comes out past them (an ear), as spans of `u`: the plates hang in
        // front of it, moved forward and trimmed if need be. Our own.
        let crossed = SheetCuts::new(&bowl, [-seam, v0], [seam, v1], openings, true);
        let spans: Vec<(f32, f32)> = crossed
            .holes
            .iter()
            .map(|h| h.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0]))))
            .chain(crossed.from_low.iter().chain(&crossed.from_high).map(|n| (n.0, n.1)))
            .chain(crossed.across.iter().copied())
            .collect();
        let pad = crossed.margin;
        for side in [-1.0f32, 1.0] {
            let centre = side * 1.25;
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
            let mut pts = fillet(&pts, 0.12, 0.25, 6);
            // `u` grows away from the front on this side by `out`.
            let out = -side;
            let reach = |pts: &[P2]| pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[0] * out), b.max(p[0] * out)));
            let mut shift = 0.0;
            for &(a, b) in &spans {
                let (a, b) = if out > 0.0 { (a, b) } else { (-b, -a) };
                let (front, back) = reach(&pts);
                if a > back + pad || b < front - pad {
                    continue;
                }
                if (a + b) * 0.5 > (front + back) * 0.5 {
                    // Behind the plate: forward of it, then trimmed.
                    let s = (back - (a - pad)).min((front - 0.4 * bowl.scale).max(0.0));
                    pts.iter_mut().for_each(|p| p[0] -= out * s);
                    shift += s;
                    let (_, back) = reach(&pts);
                    if back > a - pad {
                        pts = clip_u(&pts, (a - pad) * out, -out);
                    }
                } else {
                    pts = clip_u(&pts, (b + pad) * out, out);
                }
            }
            let (front, back) = reach(&pts);
            if pts.len() < 3 || back - front < 0.25 * c.width {
                continue;
            }
            let off = overlay + if brow { overlay } else { 0.0 };
            cheeks.append(&plate(&pts, &[], &bowl, &PlateOptions { offset: off + 0.015, ..on_top(0.03) }).merged());
            for dt in [-0.25, 0.25] {
                let at = [bowl.u(centre) - out * shift + bowl.u_span(dt * c.width, v0 + 0.05), v0 + 0.06];
                if at[0] * out > front + 0.03 && at[0] * out < back - 0.03 {
                    rivet_at.push(at);
                }
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
        let options = PlateOptions { thickness: thick, offset: 0.0, cell, crease: 0.2, ..Default::default() };
        // Whatever comes out through the visor (a snout) gets an opening of its own; slits
        // and breaths it would cut into are left out.
        let cut = SheetCuts::new(&snout, [bowl.u(half), top], [bowl.u(-half), bottom], openings, false);
        if cut.is_empty() {
            parts.add("visor", plate(&outline, &holes, &snout, &options).merged(), sh.material);
        } else {
            holes.retain(|h| {
                let (l, u) = h.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(l, u), p| ([l[0].min(p[0]), l[1].min(p[1])], [u[0].max(p[0]), u[1].max(p[1])]));
                !cut.meets(l, u, 0.04)
            });
            let mut visor = MeshBuilder::new();
            for mut piece in cut.pieces(du * 0.5, false, |_| (top, bottom)) {
                let (a, b) = piece.outline.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
                piece.holes.extend(holes.iter().filter(|h| h.iter().all(|p| p[0] > a + 0.03 && p[0] < b - 0.03)).cloned());
                visor.append(&plate(&fillet(&piece.outline, 0.25, 0.3, 6), &piece.holes, &snout, &options).merged());
            }
            parts.add("visor", visor, sh.material);
        }
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
        let open = |a: P2, c: P2| (a[0].abs() - seam).abs() < 1e-4 && (c[0].abs() - seam).abs() < 1e-4;
        let plate_opts = PlateOptions { thickness: 0.035, offset: 0.0, cell: [du, 0.06], crease: 0.2, open_edge: Some(&open), uv_rect: None, origin: [0.0, 0.0] };
        let bead = regular_polygon(10, 0.035, 0.0);
        // Whatever passes through it (an ear, a snout) gets a hole, a slot cut in from an
        // edge, or a gap right across it.
        let cut = SheetCuts::new(&brim, [-seam, -0.06], [seam, b], openings, true);
        let mut mesh = MeshBuilder::new();
        if cut.is_empty() {
            let ring = vec![[-seam, -0.06], [seam, -0.06], [seam, b], [-seam, b]];
            mesh.append(&plate(&ring, &[], &brim, &plate_opts).merged());
            let steps = (2.0 * seam / du).round() as usize;
            let edge: Vec<P2> = (0..steps).map(|k| [seam - 2.0 * seam * k as f32 / steps as f32, b]).collect();
            mesh.append(&sweep_on_surface(&brim, &edge, true, 0.0, &bead, h));
        } else {
            for piece in cut.pieces(du * 0.5, true, |_| (-0.06, b)) {
                mesh.append(&plate(&piece.outline, &piece.holes, &brim, &plate_opts).merged());
                // The rolled edge round the outside, following any slot cut in from it.
                let mut edge = piece.high_edge;
                edge.reverse();
                if piece.whole {
                    edge.pop();
                }
                mesh.append(&sweep_on_surface(&brim, &edge, piece.whole, 0.0, &bead, h));
            }
        }
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
        let w = (len * 0.3).min(s.hem.width * 0.5);
        let hemmed = s.hem.on && s.hem.hemline;
        let trim = PlateOptions { thickness: 0.06, ..options };
        // Whatever comes out through the mail (an ear, a snout) gets a slit cut down from its
        // top, a hole, or the mail opens back past it.
        let cut = SheetCuts::new(&hang, [u0, 0.0], [u1, len], openings, gap <= 0.0);
        if cut.is_empty() {
            parts.add_look("aventail", plate(&rect(0.0, len), &[], &hang, &options).merged(), Look::patterned(av.material, mail));
            // A cloth hem along the lower edge, wrapped round it.
            if hemmed {
                parts.add_look("hem", plate(&rect(len - w, len + 0.01), &[], &hang, &trim).merged(), Look::cloth(&s.hem.fabric));
            }
        } else {
            let options = PlateOptions { open_edge: None, ..options };
            let trim = PlateOptions { open_edge: None, ..trim };
            let round = |outline: &[P2]| fillet(outline, 0.08, 0.3, 4);
            let mut mesh = MeshBuilder::new();
            for piece in cut.pieces(du * 0.5, gap <= 0.0, |_| (0.0, len)) {
                mesh.append(&plate(&round(&piece.outline), &piece.holes, &hang, &options).merged());
            }
            parts.add_look("aventail", mesh, Look::patterned(av.material, mail));
            if hemmed {
                let mut hem = MeshBuilder::new();
                for piece in cut.pieces(du * 0.5, gap <= 0.0, |_| (len - w, len + 0.01)) {
                    hem.append(&plate(&round(&piece.outline), &piece.holes, &hang, &trim).merged());
                }
                parts.add_look("hem", hem, Look::cloth(&s.hem.fabric));
            }
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

    rivet_at.retain(|p| !cuts.hits(*p, cut_margin));
    parts.add("rivets", scatter_on_surface(&bowl, &rivet, &rivet_at, overlay - 0.006, h), f.material);
    parts.finish()
}

/// The comb: a fin standing on the crown from the brow to the nape.
fn comb(bowl: &Bowl, s: &HelmState, front_stop: f32) -> MeshBuilder {
    let height = s.fittings.comb;
    let n = 40;
    // Walk the sagittal line: front rim (or above a notch cut up from it, `front_stop`) up
    // over the crown and down the back.
    let front_v = (bowl.rim * 0.92).min(front_stop);
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
