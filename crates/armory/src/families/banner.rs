//! Banners: a painted cloth field hung from a crossbar rod on a wall, or from a crossbar on a
//! pole as a standard, with a hem, an optional fringe and an end that is straight, forked,
//! swallow-tailed or pointed.
//!
//! The Armory prototype has no banner; every slider and range here is this port's own. The
//! field is the shield's [`Field`] (a division, two tinctures and a charge), laid over the cloth's
//! uv, which runs 0..=1 across the width and down the length with the fess point at 0.5, 0.5
//! (`docs/rendering.md`, "Field patterns"). A faction flies its colours by giving the banner its
//! division, tinctures and crest.
//!
//! The cloth is one sheet laid on a drape surface ([`crate::build::plate`]): it hangs from its
//! tabs with the top edge dipping between them, falls into soft folds that grow towards the
//! foot, and swings out from the wall by `hang`. The sheet is shown from both sides (the
//! painting reads mirrored on the back, as dyed cloth does). The charge is a thin plate through
//! the cloth, so it shows on both faces, and the hem a thicker band round the edge.
//!
//! Item space: the middle of the crossbar at the origin, the cloth hanging down -Y from its
//! top edge at y = 0, its face toward +Z. A pole stands behind the cloth, crossing the bar.

use std::f32::consts::{PI, TAU};

use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use crate::build::device::device_shapes;
use crate::build::parts::{Look, Parts};
use crate::build::plate::{PlateOptions, Surface, plate};
use crate::cond::{Condition, ConditionLimits};
use crate::fabric::Fabric;
use crate::families::Generator;
use crate::families::shield::roll_field;
use crate::geom::{ItemMesh, MeshBuilder, P2, Shading, SweepOptions, V3, cylinder, offset_ring, signed_area, spike, torus, tube, uv_sphere};
use crate::heraldry::{Division, Field, FieldLimits, Tincture};
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::sections::device::Device;
use crate::slider::{Sliders, sliders};

use MaterialKind::{Brass, Cloth, Cord, Gold, Iron, Wood};

/// The banner generator.
pub struct Banner;

/// The foot of the cloth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BannerEnd {
    /// Cut square.
    Straight,
    /// A V notch cut up from the foot, square tails.
    Forked,
    /// A deeper notch with tails drawn to points.
    Swallowtail,
    /// Drawn down to a single point.
    Pointed,
}

impl BannerEnd {
    pub const ALL: [BannerEnd; 4] = [BannerEnd::Straight, BannerEnd::Forked, BannerEnd::Swallowtail, BannerEnd::Pointed];
}

/// How the cloth is held up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BannerMount {
    /// A crossbar with finials at each end, hung on a wall.
    Rod,
    /// A crossbar on a pole that stands behind the cloth: a standard.
    Pole,
}

/// What the rod and pole are made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BannerStuff {
    Wood,
    Iron,
    Brass,
}

impl BannerStuff {
    fn material(self) -> MaterialKind {
        match self {
            BannerStuff::Wood => Wood,
            BannerStuff::Iron => Iron,
            BannerStuff::Brass => Brass,
        }
    }
}

/// What caps the rod's ends and the pole's top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BannerFinial {
    Ball,
    Spike,
    Disc,
}

/// The cloth: its size, foot, and how it hangs.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BannerCloth {
    pub width: f32,
    /// From the top edge to the lowest point of the foot.
    pub length: f32,
    pub end: BannerEnd,
    /// How far the notch or point cuts up, as a fraction of the length.
    pub end_depth: f32,
    /// How far the top edge dips between its tabs.
    pub sag: f32,
    /// Depth of the soft folds, growing towards the foot.
    pub folds: f32,
    /// How far the foot swings out from the wall.
    pub hang: f32,
}

sliders! {
    BannerClothLimits for BannerCloth {
        width: 3.0, 9.0, 0.1;
        length: 4.0, 14.0, 0.1;
        end_depth: 0.1, 0.8, 0.01;
        sag: 0.0, 1.0, 0.01;
        folds: 0.0, 1.0, 0.01;
        hang: 0.0, 1.0, 0.01;
    }
}

impl Default for BannerCloth {
    fn default() -> Self {
        BannerCloth { width: 5.0, length: 9.0, end: BannerEnd::Swallowtail, end_depth: 0.3, sag: 0.4, folds: 0.5, hang: 0.2 }
    }
}

/// The band of cloth sewn round the edge.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BannerHem {
    pub on: bool,
    pub width: f32,
    pub trim: Tincture,
}

sliders! {
    BannerHemLimits for BannerHem {
        width: 0.15, 0.7, 0.01;
    }
}

impl Default for BannerHem {
    fn default() -> Self {
        BannerHem { on: true, width: 0.35, trim: Tincture::Or }
    }
}

/// Cords hanging from the foot.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BannerFringe {
    pub on: bool,
    pub length: f32,
    pub strands: u32,
}

sliders! {
    BannerFringeLimits for BannerFringe {
        length: 0.3, 1.8, 0.05;
        strands: 6.0, 160.0, 1.0;
    }
}

impl Default for BannerFringe {
    fn default() -> Self {
        BannerFringe { on: false, length: 0.7, strands: 16 }
    }
}

/// The crossbar, and the pole of a standard.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BannerHanging {
    pub mount: BannerMount,
    pub stuff: BannerStuff,
    pub finial: BannerFinial,
    /// How far the crossbar reaches past each side of the cloth.
    pub overhang: f32,
    /// Loops that tie the cloth to the bar.
    pub tabs: u32,
    /// A pole's height above the bar.
    pub reach: f32,
    /// A pole's length below the foot of the cloth.
    pub foot: f32,
}

sliders! {
    BannerHangingLimits for BannerHanging {
        overhang: 0.2, 2.0, 0.05;
        tabs: 2.0, 8.0, 1.0;
        reach: 1.0, 6.0, 0.1;
        foot: 1.0, 12.0, 0.1;
    }
}

impl Default for BannerHanging {
    fn default() -> Self {
        BannerHanging {
            mount: BannerMount::Rod,
            stuff: BannerStuff::Wood,
            finial: BannerFinial::Ball,
            overhang: 0.8,
            tabs: 4,
            reach: 3.0,
            foot: 4.0,
        }
    }
}

/// Every parameter of one banner.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BannerState {
    pub seed: u32,
    pub cloth: BannerCloth,
    /// The painting: division, tinctures and charge, as on a shield.
    pub field: Field,
    pub hem: BannerHem,
    pub fringe: BannerFringe,
    pub hanging: BannerHanging,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for BannerState {
    fn default() -> Self {
        BannerState {
            seed: 0,
            cloth: BannerCloth::default(),
            field: Field::default(),
            hem: BannerHem::default(),
            fringe: BannerFringe::default(),
            hanging: BannerHanging::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every banner section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BannerLimits {
    pub cloth: BannerClothLimits,
    pub field: FieldLimits,
    pub hem: BannerHemLimits,
    pub fringe: BannerFringeLimits,
    pub hanging: BannerHangingLimits,
    pub cond: ConditionLimits,
}

/// This port's own ranges. A wall banner is 6 to 12 units (60 to 120 cm) tall.
pub const LIMITS: BannerLimits = BannerLimits {
    cloth: BannerClothLimits::SLIDERS,
    field: FieldLimits::SLIDERS,
    hem: BannerHemLimits::SLIDERS,
    fringe: BannerFringeLimits::SLIDERS,
    hanging: BannerHangingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl BannerState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &BannerLimits) {
        self.cloth.fit(&limits.cloth);
        self.field.fit(&limits.field);
        self.hem.fit(&limits.hem);
        self.fringe.fit(&limits.fringe);
        self.hanging.fit(&limits.hanging);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &BannerLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("cloth", self.cloth.out_of_range(&limits.cloth)),
            tag("field", self.field.out_of_range(&limits.field)),
            tag("hem", self.hem.out_of_range(&limits.hem)),
            tag("fringe", self.fringe.out_of_range(&limits.fringe)),
            tag("hanging", self.hanging.out_of_range(&limits.hanging)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

impl BannerEnd {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(BannerEnd, u32); 4] = [(BannerEnd::Straight, 25), (BannerEnd::Forked, 25), (BannerEnd::Swallowtail, 28), (BannerEnd::Pointed, 22)];
}

impl Generator for Banner {
    type State = BannerState;

    fn roll(seed: u32) -> BannerState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let mount = *rng.weighted(&[(BannerMount::Rod, 55), (BannerMount::Pole, 45)]);
        let end = *rng.weighted(&BannerEnd::WEIGHTS);
        compose(&mut rng, seed, mount, end)
    }

    /// Banners are named for how they hang and how they end: a pennant is a long point, a
    /// gonfalon hangs tailed from a pole's crossbar, a standard is a square one on a pole.
    fn noun(s: &BannerState) -> String {
        let c = &s.cloth;
        match (s.hanging.mount, c.end) {
            (_, BannerEnd::Pointed) if c.end_depth >= 0.55 => "Pennant",
            (BannerMount::Pole, BannerEnd::Straight) => "Standard",
            (BannerMount::Pole, _) => "Gonfalon",
            (BannerMount::Rod, BannerEnd::Straight) => "Banner",
            (BannerMount::Rod, BannerEnd::Forked) => "Forked Banner",
            (BannerMount::Rod, BannerEnd::Swallowtail) => "Swallowtail Banner",
            (BannerMount::Rod, BannerEnd::Pointed) => "Pointed Banner",
        }
        .to_string()
    }

    fn build(s: &BannerState) -> ItemMesh {
        build(s)
    }
}

impl Banner {
    /// "Forge new" forced to one hanging and end, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, m, e)` for the pair `s` rolls.
    pub fn roll_as(seed: u32, mount: BannerMount, end: BannerEnd) -> BannerState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        rng.next_f64();
        compose(&mut rng, seed, mount, end)
    }
}

const DIVISIONS: [(Division, u32); 11] = [
    (Division::Plain, 3),
    (Division::PerPale, 4),
    (Division::PerFess, 3),
    (Division::Quarterly, 2),
    (Division::PerBend, 2),
    (Division::PerChevron, 2),
    (Division::PerSaltire, 1),
    (Division::Paly, 1),
    (Division::Barry, 1),
    (Division::Bendy, 1),
    (Division::Lozengy, 1),
];

fn compose(rng: &mut Mulberry32, seed: u32, mount: BannerMount, end: BannerEnd) -> BannerState {
    let mut s = BannerState { seed, palette: *rng.pick(&Palette::PRESETS), ..BannerState::default() };
    let pole = mount == BannerMount::Pole;

    s.field = roll_field(rng, true, &DIVISIONS, 0.85);
    // The banner's sign reads from across a hall: big.
    s.field.charge_size = s.field.charge_size.max(0.35);

    let c = &mut s.cloth;
    c.end = end;
    (c.width, c.length) = if pole { (rng.range_f32(3.0, 5.5), rng.range_f32(5.0, 9.0)) } else { (rng.range_f32(3.5, 7.0), rng.range_f32(6.0, 12.0)) };
    c.end_depth = match end {
        BannerEnd::Straight => rng.range_f32(0.2, 0.4),
        BannerEnd::Forked => rng.range_f32(0.15, 0.35),
        BannerEnd::Swallowtail => rng.range_f32(0.25, 0.45),
        BannerEnd::Pointed if rng.chance(0.25) => rng.range_f32(0.6, 0.8),
        BannerEnd::Pointed => rng.range_f32(0.12, 0.3),
    };
    c.sag = rng.range_f32(0.2, 0.7);
    c.folds = rng.range_f32(0.3, 0.8);
    c.hang = if pole { rng.range_f32(0.0, 0.2) } else { rng.range_f32(0.05, 0.5) };

    s.hem.on = rng.chance(0.7);
    s.hem.width = rng.range_f32(0.2, 0.5);
    s.hem.trim = if rng.chance(0.6) {
        s.field.charge_tincture
    } else if s.field.tincture.is_metal() {
        *rng.pick(&Tincture::COLOURS)
    } else {
        *rng.pick(&Tincture::METALS)
    };

    s.fringe.on = rng.chance(0.45);
    s.fringe.length = rng.range_f32(0.5, 1.2);
    let foot_length: f32 = outline(&s.cloth).1.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum();
    s.fringe.strands = (foot_length / rng.range_f32(0.08, 0.1)).round() as u32;

    let h = &mut s.hanging;
    h.mount = mount;
    h.stuff = *rng.weighted(&[(BannerStuff::Wood, 5), (BannerStuff::Iron, 2), (BannerStuff::Brass, 1)]);
    h.finial = *rng.weighted(&[(BannerFinial::Ball, 3), (BannerFinial::Spike, 3), (BannerFinial::Disc, 2)]);
    h.overhang = rng.range_f32(0.4, 1.2);
    h.tabs = rng.int(3, 6);
    h.reach = rng.range_f32(1.5, 4.0);
    h.foot = rng.range_f32(3.0, 9.0);

    s.fit(&LIMITS);
    s
}

// ---------------------------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------------------------

/// The cloth's resting outline in its own plane, x across and y down from the top edge at 0,
/// and the polyline along its foot (left to right) where the fringe hangs.
fn outline(c: &BannerCloth) -> (Vec<P2>, Vec<P2>) {
    let (hw, len) = (c.width * 0.5, c.length);
    let d = c.end_depth * len;
    match c.end {
        BannerEnd::Straight => {
            let foot = vec![[-hw, -len], [hw, -len]];
            (vec![[-hw, 0.0], [-hw, -len], [hw, -len], [hw, 0.0]], foot)
        }
        BannerEnd::Forked => {
            let foot = vec![[-hw, -len], [0.0, -len + d], [hw, -len]];
            (vec![[-hw, 0.0], [-hw, -len], [0.0, -len + d], [hw, -len], [hw, 0.0]], foot)
        }
        BannerEnd::Swallowtail => {
            let (shoulder, tip, notch) = (-len + d * 0.7, [0.62 * hw, -len], -len + d * 0.45);
            let foot = vec![[-hw, shoulder], [-tip[0], tip[1]], [0.0, notch], tip, [hw, shoulder]];
            let mut ring = vec![[-hw, 0.0]];
            ring.extend(foot.iter().copied());
            ring.push([hw, 0.0]);
            (ring, foot)
        }
        BannerEnd::Pointed => {
            let foot = vec![[-hw, -len + d], [0.0, -len], [hw, -len + d]];
            let mut ring = vec![[-hw, 0.0]];
            ring.extend(foot.iter().copied());
            ring.push([hw, 0.0]);
            (ring, foot)
        }
    }
}

/// Where the cloth's points land: hung from tabs along the top edge, folded, swung out.
struct Drape {
    half_width: f32,
    length: f32,
    folds: f32,
    hang: f32,
    sag: f32,
    /// Distance between tabs along the top edge.
    pitch: f32,
    wave: f32,
    phase: f32,
}

impl Drape {
    fn new(s: &BannerState) -> Self {
        let (c, h) = (&s.cloth, &s.hanging);
        let half_width = c.width * 0.5;
        // Wavelength of the folds: about 0.7 to 1.0 of the half width, fixed by the seed.
        let wavelength = half_width * (0.7 + 0.1 * (s.seed % 4) as f32);
        Drape {
            half_width,
            length: c.length,
            folds: c.folds * 0.45,
            hang: c.hang * 0.07 * c.length,
            sag: c.sag * 0.3,
            pitch: c.width / (h.tabs.max(2) - 1) as f32,
            wave: TAU / wavelength,
            phase: (s.seed % 628) as f32 * 0.01,
        }
    }
}

impl Surface for Drape {
    fn point(&self, p: P2) -> V3 {
        let t = (-p[1] / self.length).clamp(0.0, 1.0);
        let grip = (t * 3.0).min(1.0) * (0.6 + 0.4 * t);
        let fold = self.folds * grip * (self.wave * p[0] + self.phase + 1.1 * t).sin();
        let swing = self.hang * t * t;
        let tab = (PI * (p[0] + self.half_width) / self.pitch).sin();
        let dip = self.sag * tab * tab * (1.0 - t * 5.0).max(0.0);
        [p[0], p[1] - dip, fold + swing]
    }
}

fn build(s: &BannerState) -> ItemMesh {
    let c = &s.cloth;
    let drape = Drape::new(s);
    let (ring, foot) = outline(c);
    let hw = c.width * 0.5;
    let cell = [0.1, 0.1];
    let uv_rect = Some(([-hw, -c.length], [c.width, c.length]));
    let sheet = PlateOptions { thickness: 0.0, cell, crease: 1.2, uv_rect, ..Default::default() };

    let trim = s.hem.trim.rgb();
    let mut parts = Parts::default();

    // The hem band is a ring between the outline and the same outline drawn in; fall back to
    // a bare edge if the inset folds over itself.
    let inset = offset_ring(&ring, s.hem.width);
    let (inner, outer) = (signed_area(&inset), signed_area(&ring));
    let hemmed = s.hem.on && inner.signum() == outer.signum() && inner.abs() > 0.05 * outer.abs();

    let field_look = if s.field.painted {
        Look { pattern: s.field.surface(), ..Look::of(Cloth).tinted(s.field.tincture.rgb()) }
    } else {
        Look::of(Cloth).tinted(Rgb::hex(0xcbbf9f))
    };
    let mut cloth = plate(if hemmed { &inset } else { &ring }, &[], &drape, &sheet).face;
    cloth.make_double_sided();
    parts.add_look("cloth", cloth, field_look);

    if hemmed {
        let band = PlateOptions { thickness: 0.03, cell, crease: 1.2, uv_rect, ..Default::default() };
        parts.add_look("hem", plate(&ring, &[inset], &drape, &band).merged(), Look::cloth(&Fabric::plain(trim)));
    }

    // The charge sits on the part of the field above the foot's cut.
    if s.field.painted && s.field.charge != Device::None {
        let cut = match c.end {
            BannerEnd::Straight => 0.0,
            BannerEnd::Forked | BannerEnd::Swallowtail => c.end_depth * c.length,
            BannerEnd::Pointed => c.end_depth * c.length * 0.5,
        };
        let usable = c.length - cut;
        // On the fess point (half the length down), moved up only to clear the foot's cut.
        let top = 0.3 + if s.hem.on { s.hem.width } else { 0.0 };
        let bottom = usable - 0.2;
        let size = (s.field.charge_size * c.width.min(usable * 0.9)).min(0.8 * c.width).min((bottom - top).max(0.5));
        let depth = (c.length * 0.5).min(bottom - size * 0.5).max(top + size * 0.5);
        let centre = [0.0, -depth];
        let place = |p: &P2| [p[0] * size + centre[0], p[1] * size + centre[1]];
        // A decal: the pieces triangulated on the cloth's own grid, lifted off it along its
        // normal, on the front and (turned over) on the back.
        let mut charge = MeshBuilder::new();
        for side in [1.0f32, -1.0] {
            let options = PlateOptions { thickness: 0.0, offset: 0.03 * side, cell, crease: 1.2, ..Default::default() };
            let mut face = MeshBuilder::new();
            for (piece, holes) in device_shapes(s.field.charge, s.seed) {
                let piece: Vec<P2> = piece.iter().map(place).collect();
                let holes: Vec<Vec<P2>> = holes.iter().map(|hole| hole.iter().map(place).collect()).collect();
                face.append(&plate(&piece, &holes, &drape, &options).face);
            }
            if side < 0.0 {
                face.flip();
            }
            charge.append(&face);
        }
        parts.add_look("charge", charge, Look::of(Cloth).tinted(s.field.charge_tincture.rgb()));
    }

    if s.fringe.on {
        let cord = if s.hem.on { trim } else { s.field.charge_tincture.rgb() };
        parts.add_look("fringe", fringe(s, &drape, &foot), Look::of(Cord).tinted(cord));
    }

    hanging(s, &mut parts);
    parts.finish()
}

/// Cords hung from the foot, spread by arc length along its edge.
fn fringe(s: &BannerState, drape: &Drape, foot: &[P2]) -> MeshBuilder {
    let f = &s.fringe;
    let lengths: Vec<f32> = foot.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).collect();
    let total: f32 = lengths.iter().sum();
    let mut out = MeshBuilder::new();
    let mut rng = Mulberry32::new(armory_stream_seed(s.seed ^ 0xF21A));
    let options = SweepOptions { shading: Shading::SMOOTH, ..Default::default() };
    for i in 0..f.strands {
        let mut at = total * (i as f32 + 0.5) / f.strands as f32;
        let mut p = foot[0];
        for (k, &len) in lengths.iter().enumerate() {
            if at <= len || k + 1 == lengths.len() {
                let u = (at / len.max(1e-6)).clamp(0.0, 1.0);
                p = [foot[k][0] + (foot[k + 1][0] - foot[k][0]) * u, foot[k][1] + (foot[k + 1][1] - foot[k][1]) * u];
                break;
            }
            at -= len;
        }
        let top = drape.point([p[0], p[1] + 0.04]);
        let sway = rng.range_f32(-0.08, 0.08) * f.length;
        let length = f.length * rng.range_f32(0.8, 1.15);
        let path: Vec<V3> = (0..4)
            .map(|k| {
                let t = k as f32 / 3.0;
                [top[0] + sway * t * t, top[1] - length * t, top[2] + 0.015 + 0.04 * t.sqrt()]
            })
            .collect();
        out.append(&tube(&path, 4, &options, |t| 0.019 * (1.0 - 0.4 * t)));
    }
    out
}

/// The finial, standing on +Y from the origin with a base of radius `r`.
fn finial(shape: BannerFinial, r: f32) -> MeshBuilder {
    match shape {
        BannerFinial::Ball => {
            let mut ball = uv_sphere(r * 1.7, 12, 8);
            ball.translate([0.0, r * 1.2, 0.0]);
            ball
        }
        BannerFinial::Spike => spike(r * 1.4, r * 7.0, 8, 0.9, Shading::SMOOTH),
        BannerFinial::Disc => cylinder(r * 2.1, r * 0.9, 12, Shading::SMOOTH),
    }
}

/// The crossbar, its tab loops and finials, and the pole of a standard.
fn hanging(s: &BannerState, parts: &mut Parts) {
    let h = &s.hanging;
    let hw = s.cloth.width * 0.5;
    let pole = h.mount == BannerMount::Pole;
    let r = if pole { 0.12 } else { 0.15 };
    let (bar_y, bar_z) = (0.06, -0.22);
    let stuff = h.stuff.material();
    let metal = if h.stuff == BannerStuff::Brass { Gold } else { Brass };

    let half = hw + h.overhang;
    let mut rod = cylinder(r, half * 2.0, 12, Shading::SMOOTH);
    rod.translate([0.0, -half, 0.0]).rotate_z(PI / 2.0).translate([0.0, bar_y, bar_z]);
    parts.add("rod", rod, stuff);

    // Loops of cord round the bar and the cloth's top edge.
    let mut loops = MeshBuilder::new();
    let tabs = h.tabs.max(2);
    for k in 0..tabs {
        let x = -hw + s.cloth.width * k as f32 / (tabs - 1) as f32;
        let mut ring = torus(0.26, 0.04, 12, 5, Shading::SMOOTH);
        ring.rotate_z(PI / 2.0).translate([x, 0.0, bar_z * 0.45]);
        loops.append(&ring);
    }
    parts.add("loops", loops, Cord);

    // Finials: the chosen shape at the rod's ends, or small balls there when a pole's top wears it.
    let end_shape = if pole { BannerFinial::Ball } else { h.finial };
    let end_r = if pole { r * 0.8 } else { r };
    let mut ends = MeshBuilder::new();
    for side in [-1.0f32, 1.0] {
        let mut cap = finial(end_shape, end_r);
        cap.place([side * half, bar_y, bar_z], [side, 0.0, 0.0]);
        ends.append(&cap);
    }
    parts.add("finials", ends, metal);

    if pole {
        let pole_r = 0.2;
        let bottom = -(s.cloth.length + h.foot);
        let mut shaft = cylinder(pole_r, h.reach - bottom + bar_y, 12, Shading::SMOOTH);
        shaft.translate([0.0, bottom, bar_z]);
        parts.add("pole", shaft, stuff);
        let mut top = finial(h.finial, pole_r * 1.2);
        top.place([0.0, h.reach + bar_y, bar_z], [0.0, 1.0, 0.0]);
        parts.add("pole_finial", top, metal);
    }
}
