//! Crystals: one generator for every crystal in the game, from a cavern's towering columns to
//! the rough gems a delver pockets. A crystal is grown in one of eight habits (the outward shape
//! real crystals take, see Crystal habit on Wikipedia), alone, in a cluster about a shared
//! base of rock, or lining a split geode.
//!
//! Not in the prototypes; the sliders here are new. Every crystal is a convex solid cut from
//! planes, so each facet is exactly flat with its own true normal, and every face meets its
//! neighbours at a real edge, which is what lets the light find them:
//!
//! - **prismatic**, **columnar**, **bladed**, **acicular**, **tabular**: a prism of 3 to 8
//!   sides, pointed or chisel-ended, set on its base, differing in length against width
//!   (`length`), flattening (`flat`), taper and the height of the termination (`point`);
//! - **cubic**, **octahedral**, **dodecahedral** (rhombic): the isometric forms, which
//!   `truncation` shaves at the corners into the combinations real fluorite and garnet show.
//!
//! On top of the habit: a point at one end or both (`double_ended`), a twin (`twinned`: a
//! second crystal grown through the first), a scepter head, cracks, inclusions and clouding.
//! The stone is a [`Gemstone`], which fixes the colour, and `glow` lights the surface in it.
//!
//! Item space: a single crystal stands on y = 0 pointing +Y; a cluster rises from its lump of
//! rock, a geode opens toward +Y. One world unit is 10 cm: the default roll stands a hand to a
//! man high, a cavern grows them taller and a rough gem is a unit or so.

use std::f32::consts::{PI, TAU};

use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use crate::build::parts::{Look, Parts};
use crate::cond::{Condition, ConditionLimits};
use crate::families::Generator;
use crate::gem::Gemstone;
use crate::geom::{ItemMesh, LatheOptions, Mat3, MeshBuilder, Shading, V3, icosphere, lathe_warped, value_noise3, vec3};
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::slider::{Sliders, sliders};

use MaterialKind::{Gem, Stone};

/// The crystal generator.
pub struct Crystals;

/// The outward shape a crystal grows in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Habit {
    /// A fat prism with a pointed head, like quartz.
    Prismatic,
    /// A long slender prism.
    Columnar,
    /// A flattened blade with a chisel end.
    Bladed,
    /// A needle, thin and sharp.
    Acicular,
    /// A short wide plate.
    Tabular,
    Cubic,
    Octahedral,
    /// The rhombic dodecahedron: twelve diamond faces.
    Dodecahedral,
}

impl Habit {
    pub const ALL: [Habit; 8] = [Habit::Prismatic, Habit::Columnar, Habit::Bladed, Habit::Acicular, Habit::Tabular, Habit::Cubic, Habit::Octahedral, Habit::Dodecahedral];

    /// A prism of some sides, as opposed to an isometric form.
    pub fn is_prism(self) -> bool {
        matches!(self, Habit::Prismatic | Habit::Columnar | Habit::Bladed | Habit::Acicular | Habit::Tabular)
    }
}

/// How the crystal grows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Growth {
    /// One crystal alone.
    Single,
    /// Several about a shared base of rock.
    Cluster,
    /// A split nodule of rock lined with small crystals.
    Geode,
}

/// The crystal itself.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crystal {
    pub habit: Habit,
    pub growth: Growth,
    /// The stone: fixes the colour, and what a rough gem is worth.
    pub stone: Gemstone,
    /// Height in world units (10 cm), or the width of a plate: a cluster's tallest crystal, a geode's width.
    pub size: f32,
    /// Faces round a prism (3 to 8). Isometric habits ignore it.
    pub sides: u32,
    /// Length against width of a prism; the stretch of an isometric form.
    pub length: f32,
    /// A prism flattened toward a blade, 0 round to 0.85 thin.
    pub flat: f32,
    /// How far a prism narrows toward its head.
    pub taper: f32,
    /// The height of the termination against the prism's width.
    pub point: f32,
    /// How far corners and edges of an isometric form are shaved off.
    pub truncation: f32,
    /// Pointed at both ends, not rooted at one.
    pub double_ended: bool,
    /// A second crystal grown through the first.
    pub twinned: bool,
    /// A wider head grown on a slender shaft (prisms).
    pub scepter: bool,
    /// Crystals in a cluster; in a geode, those lining it.
    pub count: u32,
    /// How far a cluster fans out from upright.
    pub spread: f32,
    /// Fractures showing as pale lines on the faces.
    pub cracks: f32,
    /// Dark flecks of other minerals caught in the faces.
    pub inclusions: f32,
    /// Milky cloud in the stone, washing its colour pale.
    pub cloud: f32,
    /// How strongly the surface lights in the stone's colour.
    pub glow: f32,
}

sliders! {
    CrystalLimits for Crystal {
        size: 0.3, 80.0, 0.1;
        sides: 3.0, 8.0, 1.0;
        length: 0.25, 16.0, 0.05;
        flat: 0.0, 0.85, 0.01;
        taper: 0.0, 0.6, 0.01;
        point: 0.1, 2.5, 0.01;
        truncation: 0.0, 0.6, 0.01;
        count: 1.0, 80.0, 1.0;
        spread: 0.0, 1.0, 0.01;
        cracks: 0.0, 1.0, 0.01;
        inclusions: 0.0, 1.0, 0.01;
        cloud: 0.0, 1.0, 0.01;
        glow: 0.0, 1.0, 0.01;
    }
}

impl Default for Crystal {
    fn default() -> Self {
        Crystal {
            habit: Habit::Prismatic,
            growth: Growth::Single,
            stone: Gemstone::Amethyst,
            size: 8.0,
            sides: 6,
            length: 2.6,
            flat: 0.0,
            taper: 0.05,
            point: 0.9,
            truncation: 0.0,
            double_ended: false,
            twinned: false,
            scepter: false,
            count: 7,
            spread: 0.5,
            cracks: 0.0,
            inclusions: 0.0,
            cloud: 0.0,
            glow: 0.0,
        }
    }
}

/// Every parameter of one crystal.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CrystalState {
    pub seed: u32,
    pub crystal: Crystal,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every crystal section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrystalSliders {
    pub crystal: CrystalLimits,
    pub cond: ConditionLimits,
}

/// The crystals' slider ranges (new: the prototypes have none).
pub const LIMITS: CrystalSliders = CrystalSliders { crystal: CrystalLimits::SLIDERS, cond: ConditionLimits::SLIDERS };

impl CrystalState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &CrystalSliders) {
        self.crystal.fit(&limits.crystal);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &CrystalSliders) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [tag("crystal", self.crystal.out_of_range(&limits.crystal)), tag("cond", self.cond.out_of_range(&limits.cond))].concat()
    }
}

/// The stones a crystal rolls: clear and coloured ones, never the opaque.
const CLEAR_STONES: [Gemstone; 7] = [Gemstone::Garnet, Gemstone::Amethyst, Gemstone::Sapphire, Gemstone::Emerald, Gemstone::Citrine, Gemstone::Ruby, Gemstone::Diamond];

impl Generator for Crystals {
    type State = CrystalState;

    fn roll(seed: u32) -> CrystalState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let habit = *rng.pick(&Habit::ALL);
        let growth = *rng.weighted(&[(Growth::Single, 3), (Growth::Cluster, 3), (Growth::Geode, 1)]);
        compose(&mut rng, seed, habit, growth)
    }

    fn noun(s: &CrystalState) -> String {
        let c = &s.crystal;
        let stone = c.stone.name();
        match c.growth {
            Growth::Cluster => format!("{stone} Cluster"),
            Growth::Geode => format!("{stone} Geode"),
            Growth::Single => match c.habit {
                Habit::Prismatic => format!("{stone} Crystal"),
                Habit::Columnar => format!("{stone} Column"),
                Habit::Bladed => format!("Bladed {stone}"),
                Habit::Acicular => format!("{stone} Needle"),
                Habit::Tabular => format!("{stone} Plate"),
                Habit::Cubic => format!("Cubic {stone}"),
                Habit::Octahedral => format!("{stone} Octahedron"),
                Habit::Dodecahedral => format!("{stone} Dodecahedron"),
            },
        }
    }

    fn build(s: &CrystalState) -> ItemMesh {
        let mut parts = Parts::new();
        let c = &s.crystal;
        let mut rng = Mulberry32::new(s.seed.wrapping_mul(0x9e37_79b9).wrapping_add(0xc275_7a1));
        match c.growth {
            Growth::Single => {
                let solids = single(c, &mut rng);
                add_crystals(&mut parts, c, &solids, &mut rng);
            }
            Growth::Cluster => {
                let (solids, matrix) = cluster(c, &mut rng);
                add_rock(&mut parts, "matrix", matrix, s);
                add_crystals(&mut parts, c, &solids, &mut rng);
            }
            Growth::Geode => {
                let (rind, lining, solids) = geode(c, s.seed, &mut rng);
                add_rock(&mut parts, "rind", rind, s);
                parts.add_look("lining", lining, Look::of(Gem).tinted(mix(c.stone.rgb(), MILK, 0.45 + 0.3 * c.cloud)));
                add_crystals(&mut parts, c, &solids, &mut rng);
            }
        }
        parts.rest_on_ground();
        parts.finish()
    }
}

impl Crystals {
    /// "Forge new" forced to a habit and growth, for turntables, caverns and tests. Draws the
    /// same stream as [`Generator::roll`].
    pub fn roll_as(seed: u32, habit: Habit, growth: Growth) -> CrystalState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        rng.next_f64();
        compose(&mut rng, seed, habit, growth)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, habit: Habit, growth: Growth) -> CrystalState {
    let mut s = CrystalState { seed, ..CrystalState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    let c = &mut s.crystal;
    c.habit = habit;
    c.growth = growth;
    c.stone = *rng.pick(&CLEAR_STONES);
    // Proportions come from the habit; the rest of the roll is shared.
    match habit {
        Habit::Prismatic => {
            c.sides = *rng.weighted(&[(5, 1), (6, 5), (8, 1)]);
            c.length = rng.range_f32(2.0, 3.6);
            c.taper = rng.range_f32(0.0, 0.14);
            c.point = rng.range_f32(0.6, 1.3);
        }
        Habit::Columnar => {
            c.sides = *rng.weighted(&[(4, 1), (6, 5), (8, 1)]);
            c.length = rng.range_f32(4.5, 8.0);
            c.taper = rng.range_f32(0.0, 0.1);
            c.point = rng.range_f32(0.4, 1.0);
        }
        Habit::Bladed => {
            c.sides = 4;
            c.length = rng.range_f32(3.0, 6.0);
            c.flat = rng.range_f32(0.55, 0.8);
            c.taper = rng.range_f32(0.0, 0.2);
            c.point = rng.range_f32(0.3, 0.9);
        }
        Habit::Acicular => {
            c.sides = rng.int(4, 6);
            c.length = rng.range_f32(10.0, 15.0);
            c.taper = rng.range_f32(0.05, 0.3);
            c.point = rng.range_f32(1.2, 2.2);
        }
        Habit::Tabular => {
            c.sides = *rng.weighted(&[(4, 2), (6, 4), (8, 1)]);
            c.length = rng.range_f32(0.3, 0.6);
            c.point = rng.range_f32(0.15, 0.4);
        }
        Habit::Cubic | Habit::Octahedral | Habit::Dodecahedral => {
            c.length = rng.range_f32(0.85, 1.3);
            c.truncation = if rng.chance(0.5) { rng.range_f32(0.1, 0.45) } else { 0.0 };
        }
    }
    let (lo, hi) = match growth {
        Growth::Single => (5.0, 13.0),
        Growth::Cluster => (6.0, 15.0),
        Growth::Geode => (6.0, 11.0),
    };
    c.size = rng.range_f32(lo, hi);
    c.count = match growth {
        Growth::Single => 1,
        Growth::Cluster if habit == Habit::Acicular => rng.int(10, 24),
        Growth::Cluster => rng.int(4, 12),
        Growth::Geode => rng.int(24, 60),
    };
    c.spread = rng.range_f32(0.35, 0.85);
    c.double_ended = growth == Growth::Single && habit.is_prism() && rng.chance(0.2);
    c.twinned = growth == Growth::Single && rng.chance(0.15);
    c.scepter = growth == Growth::Single && matches!(habit, Habit::Prismatic | Habit::Columnar) && rng.chance(0.2);
    c.cracks = if rng.chance(0.4) { rng.range_f32(0.15, 0.6) } else { 0.0 };
    c.inclusions = if rng.chance(0.4) { rng.range_f32(0.15, 0.6) } else { 0.0 };
    c.cloud = if rng.chance(0.5) { rng.range_f32(0.1, 0.55) } else { 0.0 };
    s.fit(&LIMITS);
    s
}

// ───────────────────────── Solids ─────────────────────────

const MILK: Rgb = Rgb(0.88, 0.9, 0.93);

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    Rgb(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}

type Poly = Vec<V3>;

/// The area-weighted normal of a polygon (Newell's method): its facing, and twice its area as
/// the length.
fn newell(p: &[V3]) -> V3 {
    let mut n = vec3::ZERO;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        n = vec3::add(n, [(a[1] - b[1]) * (a[2] + b[2]), (a[2] - b[2]) * (a[0] + b[0]), (a[0] - b[0]) * (a[1] + b[1])]);
    }
    n
}

/// Keeps the part of a convex polygon on the near side of the plane `n . p = d`.
fn clip(poly: &[V3], n: V3, d: f32) -> Poly {
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (da, db) = (vec3::dot(n, a) - d, vec3::dot(n, b) - d);
        if da <= 0.0 {
            out.push(a);
        }
        if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
            out.push(vec3::lerp(a, b, da / (da - db)));
        }
    }
    out
}

/// A convex solid: its faces, each a flat polygon wound counter-clockwise from outside.
#[derive(Clone, Debug, Default)]
struct Solid {
    faces: Vec<Poly>,
}

impl Solid {
    /// The solid inside every plane `n . p <= d`, one face per plane that survives. Each face
    /// starts as a huge square on its plane and is cut down by every other plane.
    fn from_planes(planes: &[(V3, f32)]) -> Solid {
        let planes: Vec<(V3, f32)> = planes.iter().map(|&(n, d)| (vec3::normalize_or(n, vec3::Y), d)).collect();
        let reach = planes.iter().fold(1.0f32, |m, p| m.max(p.1.abs())) * 60.0;
        let mut faces = Vec::new();
        for (i, &(n, d)) in planes.iter().enumerate() {
            let u = vec3::any_perpendicular(n);
            let v = vec3::cross(n, u);
            let c = vec3::scale(n, d);
            let corner = |a: f32, b: f32| vec3::add(c, vec3::add(vec3::scale(u, a * reach), vec3::scale(v, b * reach)));
            let mut poly = vec![corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)];
            for (j, &(nj, dj)) in planes.iter().enumerate() {
                if j != i {
                    poly = clip(&poly, nj, dj);
                    if poly.len() < 3 {
                        break;
                    }
                }
            }
            // Drop doubled corners and slivers.
            poly.dedup_by(|a, b| vec3::distance(*a, *b) < 1e-4 * d.abs().max(1e-3));
            while poly.len() > 1 && vec3::distance(poly[0], poly[poly.len() - 1]) < 1e-4 * d.abs().max(1e-3) {
                poly.pop();
            }
            if poly.len() >= 3 && vec3::length(newell(&poly)) > 1e-6 * d.abs().max(1e-3).powi(2) {
                faces.push(poly);
            }
        }
        Solid { faces }
    }

    /// Moves every corner (for positive scales and rotations the winding is kept).
    fn map(&mut self, f: impl Fn(V3) -> V3) -> &mut Self {
        for p in self.faces.iter_mut().flatten() {
            *p = f(*p);
        }
        self
    }

    fn scale(&mut self, s: V3) -> &mut Self {
        self.map(|p| vec3::mul(p, s))
    }

    fn rotate(&mut self, m: &Mat3) -> &mut Self {
        self.map(|p| m.transform(p))
    }

    fn translate(&mut self, o: V3) -> &mut Self {
        self.map(|p| vec3::add(p, o))
    }

    /// The overall size: the diagonal of the bounding box.
    fn extent(&self) -> f32 {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for p in self.faces.iter().flatten() {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        if lo[0].is_finite() { vec3::distance(lo, hi) } else { 0.0 }
    }

    fn mesh(&self) -> MeshBuilder {
        let mut m = MeshBuilder::new();
        for f in &self.faces {
            m.push_fan_cap(f, newell(f));
        }
        m
    }
}

/// What one crystal is grown as.
#[derive(Clone, Copy, Debug)]
struct Spec {
    habit: Habit,
    sides: u32,
    height: f32,
    /// Length against width.
    aspect: f32,
    flat: f32,
    taper: f32,
    point: f32,
    truncation: f32,
    double_ended: bool,
    twinned: bool,
    scepter: bool,
}

impl Spec {
    fn of(c: &Crystal, height: f32) -> Spec {
        Spec { habit: c.habit, sides: c.sides, height, aspect: c.length, flat: c.flat, taper: c.taper, point: c.point, truncation: c.truncation, double_ended: c.double_ended, twinned: c.twinned, scepter: c.scepter }
    }
}

/// A prism standing on y = 0: `sides` faces tapering a little, a pointed head and a flat base
/// (or a pointed foot too), faces a touch uneven as real ones grow.
fn prism(sp: &Spec, rng: &mut Mulberry32) -> Solid {
    let n = sp.sides.clamp(3, 8) as usize;
    // Longer than wide stands `height` tall; a plate, wider than long, is `height` wide.
    let (h, radius) = if sp.aspect < 1.0 { (sp.height * sp.aspect, sp.height / 2.0) } else { (sp.height, sp.height / (2.0 * sp.aspect)) };
    let a = radius * (PI / n as f32).cos();
    let pyr = (sp.point * a).min(0.45 * h);
    let body = (h - pyr * if sp.double_ended { 2.0 } else { 1.0 }).max(0.2 * h);
    let a_top = a * (1.0 - sp.taper.clamp(0.0, 0.7));
    let slope = (a - a_top) / body;
    // Termination faces alternate large and small, as quartz's do.
    let alt = rng.range_f32(0.0, 0.3);
    let phase = rng.range_f32(0.0, TAU);
    let mut planes: Vec<(V3, f32)> = Vec::new();
    let jitter = |rng: &mut Mulberry32| 1.0 + rng.range_f32(-0.05, 0.05);
    for k in 0..n {
        let az = phase + TAU * k as f32 / n as f32;
        let (s, c) = az.sin_cos();
        let side = vec3::normalize_or([c, slope, -s], vec3::X);
        planes.push((side, vec3::dot(side, [a * c, 0.0, -a * s]) * jitter(rng)));
        let pk = pyr * (1.0 + if k % 2 == 0 { alt } else { -alt });
        let top = vec3::normalize_or([pk * c, a_top, -pk * s], vec3::Y);
        planes.push((top, vec3::dot(top, [a_top * c, body, -a_top * s]) * jitter(rng)));
        if sp.double_ended {
            let foot = vec3::normalize_or([pk * c, -a, -pk * s], vec3::neg(vec3::Y));
            planes.push((foot, vec3::dot(foot, [a * c, 0.0, -a * s]) * jitter(rng)));
        }
    }
    if !sp.double_ended {
        planes.push((vec3::neg(vec3::Y), 0.0));
    }
    let mut solid = Solid::from_planes(&planes);
    solid.scale([1.0, 1.0, 1.0 - sp.flat.clamp(0.0, 0.85)]);
    solid
}

/// The isometric forms, axis-aligned about the origin, `height` tall along Y before the
/// stretch. Returns the solid centred on the origin.
fn isometric(sp: &Spec, rng: &mut Mulberry32) -> Solid {
    let h = sp.height;
    let stretch = sp.aspect.clamp(0.6, 1.8);
    let t = sp.truncation.clamp(0.0, 0.6);
    let j = |rng: &mut Mulberry32| 1.0 + rng.range_f32(-0.06, 0.06);
    let axes = [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]];
    let mut planes: Vec<(V3, f32)> = Vec::new();
    match sp.habit {
        Habit::Cubic => {
            let r = h / (2.0 * stretch);
            for ax in axes {
                planes.push((ax, r * j(rng)));
            }
            if t > 0.02 {
                let d = r * 3f32.sqrt() * (1.0 - t * 0.5);
                for sx in [-1.0, 1.0] {
                    for sy in [-1.0, 1.0] {
                        for sz in [-1.0, 1.0] {
                            planes.push(([sx, sy, sz], d * j(rng)));
                        }
                    }
                }
            }
        }
        Habit::Octahedral => {
            let r = h / (2.0 * stretch);
            for sx in [-1.0, 1.0] {
                for sy in [-1.0, 1.0] {
                    for sz in [-1.0, 1.0] {
                        planes.push(([sx, sy, sz], r / 3f32.sqrt() * j(rng)));
                    }
                }
            }
            if t > 0.02 {
                for ax in axes {
                    planes.push((ax, r * (1.0 - t * 0.8)));
                }
            }
        }
        _ => {
            let d = h / (2.0 * 2f32.sqrt() * stretch);
            for (a, b) in [(0, 1), (0, 2), (1, 2)] {
                for sa in [-1.0, 1.0] {
                    for sb in [-1.0, 1.0] {
                        let mut n = [0.0; 3];
                        n[a] = sa;
                        n[b] = sb;
                        planes.push((n, d * j(rng)));
                    }
                }
            }
            if t > 0.02 {
                for ax in axes {
                    planes.push((ax, d * 2f32.sqrt() * (1.0 - t * 0.5)));
                }
            }
        }
    }
    let mut solid = Solid::from_planes(&planes);
    solid.scale([1.0, stretch, 1.0]);
    solid
}

/// The turn that sets an isometric form on a face: a triangle's for the octahedron, a
/// diamond's for the dodecahedron; the cube already stands on one.
fn on_a_face(habit: Habit) -> Mat3 {
    match habit {
        Habit::Octahedral => Mat3::from_rotation_arc([1.0, 1.0, 1.0], vec3::Y),
        Habit::Dodecahedral => Mat3::from_rotation_arc([1.0, 1.0, 0.0], vec3::Y),
        _ => Mat3::IDENTITY,
    }
}

/// One crystal as a spec: the solid or solids (a twin or a scepter is more than one), standing
/// on y = 0.
fn crystal_solids(sp: &Spec, rng: &mut Mulberry32) -> Vec<Solid> {
    if sp.habit.is_prism() {
        let mut out = Vec::new();
        if sp.scepter {
            // A slender shaft, and a wide head grown over its tip.
            let shaft = Spec { aspect: sp.aspect * 1.7, height: sp.height * 0.82, scepter: false, twinned: false, ..*sp };
            out.push(prism(&shaft, rng));
            let head = Spec { height: sp.height * 0.4, aspect: sp.aspect * 0.8, double_ended: false, scepter: false, twinned: false, ..*sp };
            let mut head = prism(&head, rng);
            head.translate([0.0, sp.height * 0.6, 0.0]);
            out.push(head);
        } else {
            out.push(prism(sp, rng));
        }
        if sp.twinned {
            // A second crystal off the same root, leaning across the first.
            let angle = rng.range_f32(0.5, 0.85) * if rng.chance(0.5) { 1.0 } else { -1.0 };
            let pivot = if sp.double_ended { sp.height * 0.5 } else { 0.0 };
            let twin = Spec { height: sp.height * rng.range_f32(0.7, 0.95), twinned: false, scepter: false, ..*sp };
            let mut twin = prism(&twin, rng);
            let m = Mat3::from_axis_angle(vec3::Z, angle);
            twin.translate([0.0, -pivot, 0.0]).rotate(&m).translate([0.0, pivot, 0.0]);
            out.push(twin);
        }
        out
    } else {
        let mut main = isometric(sp, rng);
        let mut out = Vec::new();
        if sp.twinned {
            // A penetration twin: the same form turned to cross itself.
            let (axis, angle) = match sp.habit {
                Habit::Octahedral => ([1.0, 1.0, 1.0], PI / 3.0),
                _ => (vec3::Y, PI / 4.0),
            };
            let mut twin = main.clone();
            twin.rotate(&Mat3::from_axis_angle(axis, angle));
            out.push(twin);
        }
        out.push(std::mem::take(&mut main));
        let m = on_a_face(sp.habit);
        for s in &mut out {
            s.rotate(&m);
        }
        // Rest the form on its lowest point.
        let low = out.iter().flat_map(|s| s.faces.iter().flatten()).map(|p| p[1]).fold(f32::INFINITY, f32::min);
        for s in &mut out {
            s.translate([0.0, -low, 0.0]);
        }
        out
    }
}

fn single(c: &Crystal, rng: &mut Mulberry32) -> Vec<Solid> {
    crystal_solids(&Spec::of(c, c.size), rng)
}

/// A lump of rock with crystals rising from it: the tallest upright in the middle, the rest
/// leaning outward the more `spread` they have. Returns the crystals and the rock.
fn cluster(c: &Crystal, rng: &mut Mulberry32) -> (Vec<Solid>, MeshBuilder) {
    let h = c.size;
    // Stubby habits (the isometric forms and plates) are as wide as they are tall: they sit
    // scattered over the rock, smaller, where a prism's points can crowd together and fan.
    let stubby = !c.habit.is_prism() || c.length < 1.0;
    let mr = h * if stubby { 0.5 } else if c.habit == Habit::Acicular { 0.3 } else { 0.34 };
    let n = c.count.max(1);
    let mut solids = Vec::new();
    for i in 0..n {
        let central = i == 0;
        let frac = if central { 0.0 } else { rng.range_f32(0.2, 1.0) };
        let az = i as f32 * 2.399_963 + rng.range_f32(-0.3, 0.3);
        let (lean, len, root) = if stubby {
            let len = h * if central { 0.55 } else { rng.range_f32(0.22, 0.45) };
            let reach = mr * 0.8 * ((i as f32 + 0.5) / n as f32).sqrt();
            (c.spread * frac * 0.6, len, [reach * az.cos(), mr * 0.1, -reach * az.sin()])
        } else {
            let lean = (c.spread * frac * 1.25).min(1.3);
            let len = if central { h } else { h * rng.range_f32(0.4, 0.85) * (1.0 - 0.3 * frac) };
            (lean, len, [lean.sin() * az.cos() * mr * 0.5, mr * 0.1, -lean.sin() * az.sin() * mr * 0.5])
        };
        let sp = Spec { height: len, aspect: c.length * rng.range_f32(0.85, 1.2), double_ended: false, twinned: central && c.twinned, scepter: central && c.scepter, ..Spec::of(c, len) };
        let mut group = crystal_solids(&sp, rng);
        let dir = [lean.sin() * az.cos(), lean.cos(), -lean.sin() * az.sin()];
        let tilt = Mat3::from_rotation_arc(vec3::Y, dir);
        let twist = Mat3::from_axis_angle(vec3::Y, rng.range_f32(0.0, TAU));
        for s in &mut group {
            s.rotate(&twist).rotate(&tilt).translate(root);
        }
        solids.extend(group);
    }
    // The rock they grow from: a rough lump, flat shaded.
    let mut lump = icosphere(1.0, 1);
    let noise_seed = rng.int(0, 1 << 20);
    for p in &mut lump.positions {
        let k = 0.75 + 0.5 * value_noise3(vec3::scale(*p, 1.6), noise_seed);
        *p = vec3::mul(vec3::scale(*p, mr * k), [1.0, 0.55, 1.0]);
    }
    (solids, lump.flat_shaded())
}

/// A nodule split open, its cavity facing +Y: a rind of rock, a pale lining, and crystals
/// pointing in from the walls toward the centre. Returns the rind, the lining and the crystals.
fn geode(c: &Crystal, seed: u32, rng: &mut Mulberry32) -> (MeshBuilder, MeshBuilder, Vec<Solid>) {
    let r = c.size * 0.5;
    let ri = r * 0.76;
    let cy = r * 0.92;
    let steps = 10;
    let quarter = |radius: f32, from: f32, to: f32| -> Vec<[f32; 2]> {
        (0..=steps).map(|i| {
            let th = from + (to - from) * i as f32 / steps as f32;
            [radius * th.cos(), cy + radius * th.sin()]
        }).collect()
    };
    // Out up the rind to the rim, across it, and back down the inside.
    let mut profile = quarter(r, -PI / 2.0, 0.0);
    profile.extend(quarter(ri, 0.0, -PI / 2.0));
    let rough = r * 0.1;
    let warp = move |p: V3| {
        let off = [p[0], p[1] - cy, p[2]];
        let dist = vec3::length(off);
        if dist < ri * 1.02 && p[1] < cy - 1e-3 {
            return p;
        }
        let k = value_noise3(vec3::scale(p, 1.3 / r.max(0.1)), seed) - 0.5;
        let out = vec3::normalize_or(off, vec3::Y);
        let mut q = vec3::add(p, vec3::scale(out, k * rough * if p[1] >= cy - 1e-3 { 1.0 } else { 0.6 }));
        if p[1] >= cy - 1e-3 {
            q[1] += k * rough * 0.8;
        }
        q
    };
    let opts = LatheOptions { facets: 28, shading: Shading::SMOOTH, cap_start: false, cap_end: false, phase: 0.0 };
    let rind = lathe_warped(&profile, &opts, warp);
    // The lining sits just inside the rind, from a little below the rim down to the floor.
    let rl = ri - 0.012 * r;
    let lining_profile: Vec<[f32; 2]> = (0..=steps).map(|i| {
        let th = -0.3 - (PI / 2.0 - 0.3) * i as f32 / steps as f32;
        [rl * th.cos(), cy + rl * th.sin()]
    }).collect();
    let lining = lathe_warped(&lining_profile, &opts, |p| p);
    // The crystals: golden-angle spiral over the lower cap of the cavity.
    let n = c.count.max(1);
    let max_polar = 1.25f32;
    let mut solids = Vec::new();
    for i in 0..n {
        let u = (i as f32 + 0.5) / n as f32;
        let polar = (1.0 - u * (1.0 - max_polar.cos())).acos();
        let az = i as f32 * 2.399_963 + rng.range_f32(-0.15, 0.15);
        let dir = [polar.sin() * az.cos(), -polar.cos(), -polar.sin() * az.sin()];
        let len = ri * rng.range_f32(0.24, 0.5) * (1.0 - 0.35 * polar / max_polar);
        let sp = Spec { aspect: c.length.clamp(1.4, 3.0) * rng.range_f32(0.85, 1.2), double_ended: false, twinned: false, scepter: false, ..Spec::of(c, len) };
        let mut group = crystal_solids(&sp, rng);
        let tilt = Mat3::from_rotation_arc(vec3::Y, vec3::neg(dir));
        let twist = Mat3::from_axis_angle(vec3::Y, rng.range_f32(0.0, TAU));
        let root = vec3::add([0.0, cy, 0.0], vec3::scale(dir, rl + 0.03 * len));
        for s in &mut group {
            s.rotate(&twist).rotate(&tilt).translate(root);
        }
        solids.extend(group);
    }
    (rind, lining, solids)
}

// ───────────────────────── Surface ─────────────────────────

fn add_rock(parts: &mut Parts, name: &'static str, mesh: MeshBuilder, s: &CrystalState) {
    parts.add_look(name, mesh, Look::of(Stone).tinted(mix(s.palette.stone, Rgb(0.1, 0.1, 0.12), 0.35)));
}

/// The crystal solids as parts: the body in the stone's colour (washed pale by cloud), its
/// cracks and its inclusions.
fn add_crystals(parts: &mut Parts, c: &Crystal, solids: &[Solid], rng: &mut Mulberry32) {
    let mut body = MeshBuilder::new();
    let mut cracks = MeshBuilder::new();
    let mut flecks = MeshBuilder::new();
    for solid in solids {
        body.append(&solid.mesh());
        let extent = solid.extent();
        if c.cracks > 0.0 {
            add_cracks(&mut cracks, solid, c.cracks, extent, rng);
        }
        if c.inclusions > 0.0 {
            add_flecks(&mut flecks, solid, c.inclusions, extent, rng);
        }
    }
    let colour = mix(c.stone.rgb(), MILK, c.cloud * 0.6);
    parts.add_look("crystal", body, Look::stone(c.stone, c.glow).tinted(colour));
    parts.add_look("cracks", cracks, Look::of(Gem).tinted(mix(colour, MILK, 0.3)).without_outline());
    parts.add_look("inclusions", flecks, Look::of(Gem).tinted(mix(colour, Rgb(0.05, 0.04, 0.05), 0.35)).without_outline());
}

/// Fractures: a jagged pale line across some faces, laid a hair proud of them.
fn add_cracks(out: &mut MeshBuilder, solid: &Solid, amount: f32, extent: f32, rng: &mut Mulberry32) {
    let (lift, width) = (0.003 * extent, 0.007 * extent);
    for face in &solid.faces {
        let facing = vec3::normalize_or(newell(face), vec3::Y);
        // Short, tiny faces are left clean.
        if vec3::length(newell(face)) < 0.02 * extent * extent || !rng.chance(f64::from(amount) * 0.7) {
            continue;
        }
        let edge = |i: usize, t: f32| vec3::lerp(face[i % face.len()], face[(i + 1) % face.len()], t);
        let i = rng.int(0, face.len() as u32 - 1) as usize;
        let j = (i + 1 + rng.int(0, face.len() as u32 - 2) as usize) % face.len();
        let (a, b) = (edge(i, rng.range_f32(0.2, 0.8)), edge(j, rng.range_f32(0.2, 0.8)));
        let across = vec3::normalize_or(vec3::cross(facing, vec3::sub(b, a)), vec3::X);
        let length = vec3::distance(a, b);
        let line: Vec<V3> = (0..=5)
            .map(|k| {
                let t = k as f32 / 5.0;
                let wander = if k == 0 || k == 5 { 0.0 } else { rng.range_f32(-0.08, 0.08) * length };
                vec3::add(vec3::add(vec3::lerp(a, b, t), vec3::scale(across, wander)), vec3::scale(facing, lift))
            })
            .collect();
        for k in 0..5 {
            let dir = vec3::normalize_or(vec3::sub(line[k + 1], line[k]), vec3::X);
            let side = vec3::scale(vec3::normalize_or(vec3::cross(facing, dir), vec3::X), width * 0.5);
            let (p, q) = (line[k], line[k + 1]);
            out.push_flat_quad([vec3::add(p, side), vec3::add(q, side), vec3::sub(q, side), vec3::sub(p, side)], [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], facing);
        }
    }
}

/// Inclusions: small dark flecks sunk half into some faces.
fn add_flecks(out: &mut MeshBuilder, solid: &Solid, amount: f32, extent: f32, rng: &mut Mulberry32) {
    let r = 0.035 * extent;
    for face in &solid.faces {
        if vec3::length(newell(face)) < 0.02 * extent * extent || !rng.chance(f64::from(amount) * 0.6) {
            continue;
        }
        let facing = vec3::normalize_or(newell(face), vec3::Y);
        let centre = vec3::centroid(face);
        let corner = face[rng.int(0, face.len() as u32 - 1) as usize];
        let at = vec3::lerp(centre, corner, rng.range_f32(0.0, 0.55));
        let mut fleck = icosphere(r * rng.range_f32(0.6, 1.3), 0).flat_shaded();
        fleck.scale([1.0, 0.45, 1.6]).rotate_y(rng.range_f32(0.0, TAU)).align_y_to(facing).translate(vec3::sub(at, vec3::scale(facing, r * 0.15)));
        out.append(&fleck);
    }
}
