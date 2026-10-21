//! Shields: a curved board with a painted field, a rim, a boss, studs and the straps or grip
//! behind.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (the `shield`,
//! `field` and `rim` sections, `BASE.shield`, `PRESETS.shield`, the `fam==='shield'` branch of
//! `randomize()`, `buildShield()` and `forgeNoun()`). There is no `RANGES.shield`.
//!
//! Departures from the prototype:
//! - Shapes: the prototype's `tower` (labelled Pavise) is split into [`ShieldShape::Pavise`]
//!   (flat foot, shoulders, a ridged face) and [`ShieldShape::Tower`] (a deeply curved
//!   scutum), and [`ShieldShape::Oval`] is added.
//! - How the board curves is its own select ([`ShieldBend`]): a dome, a cylinder or a ridge.
//!   The face is a true bent plate from [`crate::build::plate`], not a bulge pushed along z,
//!   so the outline keeps its size on the curve.
//! - Planks are a count on the body, not a field division: bare boards show them and paint
//!   covers them, so a painted round shield can still be divided.
//! - The roll composes a [`ShieldArchetype`] and follows the rule of tincture (metal on
//!   colour, colour on metal); the prototype picks tinctures at random.
//! - The back has arm straps (enarmes) or a hand grip behind the boss; the prototype's back is
//!   bare.
//!
//! Item space: the face's centre at the origin, mid-thickness, the face toward +Z, +Y up.
//! The painted face's uv runs 0..=1 across the shield's width and height, for the shader's
//! division (`Field::division`, `docs/rendering.md`).

use std::f32::consts::PI;

use crate::build::device::device_shapes;
use crate::build::parts::{Look, Parts};
use crate::build::plate::{PlateOptions, Surface, plate, rivet_head, rounded_rect, scatter_on_surface, sweep_on_surface};
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::geom::vec3::{self, V3};
use crate::geom::{
    ItemMesh, P2, LatheOptions, MeshBuilder, Shading, SweepOptions, fillet, lathe, offset_ring, resample_closed, signed_area, spline_profile, sweep,
    tube,
};
use crate::fabric::{PatternKind, SurfacePattern};
use crate::heraldry::{Division, Field, FieldLimits, Tincture};
use crate::sections::device::Device;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::slider::{Sliders, sliders};
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Brass, Gold, Iron, Leather, Paint, Silver, Wood};

/// The shield generator.
pub struct Shield;

/// The outline of the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShieldShape {
    Round,
    /// A small round fist shield.
    Buckler,
    /// Flat top, straight sides curving to a point.
    Heater,
    /// Round top tapering to a long point.
    Kite,
    /// A tall round-cornered oblong (scutum).
    Tower,
    /// A standing shield: flat foot, sloped shoulders, a ridge down the face.
    Pavise,
    Oval,
}

impl ShieldShape {
    pub const ALL: [ShieldShape; 7] = [
        ShieldShape::Round, ShieldShape::Buckler, ShieldShape::Heater, ShieldShape::Kite, ShieldShape::Tower, ShieldShape::Pavise,
        ShieldShape::Oval,
    ];

    /// Round outlines centre their boss and charge; the others sit them higher.
    pub fn is_round(self) -> bool {
        matches!(self, ShieldShape::Round | ShieldShape::Buckler)
    }
}

/// How the board curves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShieldBend {
    /// Dished both ways, a spherical cap (round shields, bucklers).
    Dome,
    /// Bent round a vertical axis (heaters, kites, scuta).
    Cylinder,
    /// Two flats meeting at a vertical ridge (pavises).
    Ridge,
}

/// The board: the prototype's `shield` section, plus [`ShieldBend`] and planks.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShieldBody {
    pub shape: ShieldShape,
    /// Height.
    pub size: f32,
    /// Width over height.
    pub width: f32,
    /// How deeply the board curves, 0 flat to 1 deep.
    pub curve: f32,
    pub bend: ShieldBend,
    pub thickness: f32,
    /// Boards the body is glued from, running up the shield. Shows where the face is bare.
    pub planks: u32,
    pub material: MaterialKind,
}

sliders! {
    ShieldBodyLimits for ShieldBody {
        size: 3.0, 12.0, 0.05;
        width: 0.4, 1.1, 0.01;
        curve: 0.0, 1.0, 0.01;
        thickness: 0.05, 0.4, 0.005;
        planks: 0.0, 12.0, 1.0;
    }
}

impl Default for ShieldBody {
    /// The prototype's `BASE.shield`.
    fn default() -> Self {
        ShieldBody {
            shape: ShieldShape::Heater,
            size: 7.0,
            width: 0.85,
            curve: 0.45,
            bend: ShieldBend::Cylinder,
            thickness: 0.14,
            planks: 5,
            material: Wood,
        }
    }
}

impl ShieldBody {
    pub fn height(&self) -> f32 {
        self.size
    }

    pub fn breadth(&self) -> f32 {
        self.size * self.width
    }
}

/// An edging round the board.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rim {
    pub on: bool,
    /// How far the edging reaches over the face (the prototype's "Rim thickness").
    pub width: f32,
    pub material: MaterialKind,
}

sliders! {
    RimLimits for Rim {
        width: 0.04, 0.3, 0.005;
    }
}

impl Default for Rim {
    fn default() -> Self {
        Rim { on: true, width: 0.1, material: Iron }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BossStyle {
    /// A rounded dome on a flange.
    Dome,
    /// A cone rising to a blunt point.
    Cone,
    /// A dome with a spike.
    Spike,
}

impl BossStyle {
    pub const ALL: [BossStyle; 3] = [BossStyle::Dome, BossStyle::Cone, BossStyle::Spike];
}

/// The metal boss over the hand.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Boss {
    pub on: bool,
    pub style: BossStyle,
    /// Diameter of the flange.
    pub size: f32,
    pub material: MaterialKind,
}

sliders! {
    BossLimits for Boss {
        size: 0.3, 2.0, 0.01;
    }
}

impl Default for Boss {
    fn default() -> Self {
        Boss { on: false, style: BossStyle::Dome, size: 1.0, material: Iron }
    }
}

/// Nail heads in a ring just inside the rim.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Studs {
    pub count: u32,
    pub material: MaterialKind,
}

sliders! {
    StudsLimits for Studs {
        count: 0.0, 32.0, 1.0;
    }
}

impl Default for Studs {
    fn default() -> Self {
        Studs { count: 0, material: Iron }
    }
}

/// Every parameter of one shield.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShieldState {
    pub seed: u32,
    pub body: ShieldBody,
    pub field: Field,
    pub rim: Rim,
    pub boss: Boss,
    pub studs: Studs,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for ShieldState {
    /// The prototype's `BASE` state for the shield sections.
    fn default() -> Self {
        ShieldState {
            seed: 0,
            body: ShieldBody::default(),
            field: Field::default(),
            rim: Rim::default(),
            boss: Boss::default(),
            studs: Studs::default(),
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every shield section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShieldLimits {
    pub body: ShieldBodyLimits,
    pub field: FieldLimits,
    pub rim: RimLimits,
    pub boss: BossLimits,
    pub studs: StudsLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges (it narrows none for shields).
pub const LIMITS: ShieldLimits = ShieldLimits {
    body: ShieldBodyLimits::SLIDERS,
    field: FieldLimits::SLIDERS,
    rim: RimLimits::SLIDERS,
    boss: BossLimits::SLIDERS,
    studs: StudsLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl ShieldState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &ShieldLimits) {
        self.body.fit(&limits.body);
        self.field.fit(&limits.field);
        self.rim.fit(&limits.rim);
        self.boss.fit(&limits.boss);
        self.studs.fit(&limits.studs);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    fn nudge(&mut self, rng: &mut Mulberry32, limits: &ShieldLimits, odds: f64, spread: f32) {
        self.body.nudge(rng, &limits.body, odds, spread);
        self.field.nudge(rng, &limits.field, odds, spread);
        self.rim.nudge(rng, &limits.rim, odds, spread);
        self.boss.nudge(rng, &limits.boss, odds, spread);
        self.studs.nudge(rng, &limits.studs, odds, spread);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &ShieldLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("body", self.body.out_of_range(&limits.body)),
            tag("field", self.field.out_of_range(&limits.field)),
            tag("rim", self.rim.out_of_range(&limits.rim)),
            tag("boss", self.boss.out_of_range(&limits.boss)),
            tag("studs", self.studs.out_of_range(&limits.studs)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

/// The kinds of shield "Forge new" composes. Not stored: the noun reads the state itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShieldArchetype {
    /// Planked round shield with an iron boss.
    Round,
    /// Small round leather shield, studded, sometimes with a spike.
    Targe,
    /// Painted heater with arms.
    Heater,
    /// Long kite, painted, sometimes bossed.
    Kite,
    /// Curved scutum, oblong or oval, with a boss.
    Scutum,
    /// Tall standing shield with a ridge and bold arms.
    Pavise,
    /// Small steel fist shield with a big boss.
    Buckler,
}

impl ShieldArchetype {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(ShieldArchetype, u32); 7] = [
        (ShieldArchetype::Round, 22),
        (ShieldArchetype::Targe, 8),
        (ShieldArchetype::Heater, 22),
        (ShieldArchetype::Kite, 14),
        (ShieldArchetype::Scutum, 10),
        (ShieldArchetype::Pavise, 10),
        (ShieldArchetype::Buckler, 14),
    ];
}

impl Generator for Shield {
    type State = ShieldState;

    fn roll(seed: u32) -> ShieldState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let archetype = *rng.weighted(&ShieldArchetype::WEIGHTS);
        compose(&mut rng, seed, archetype)
    }

    /// `forgeNoun()` for shields, with names for the added shapes and the targe.
    fn noun(s: &ShieldState) -> String {
        match s.body.shape {
            ShieldShape::Round if s.body.material == Leather => "Targe",
            ShieldShape::Round => "Round Shield",
            ShieldShape::Buckler => "Buckler",
            ShieldShape::Heater => "Heater Shield",
            ShieldShape::Kite => "Kite Shield",
            ShieldShape::Tower => "Tower Shield",
            ShieldShape::Pavise => "Pavise",
            ShieldShape::Oval => "Oval Shield",
        }
        .to_string()
    }

    /// The board from [`plate`], the charge laid on its face, the rim swept round its edge,
    /// then the boss, studs, and the straps or grip behind.
    fn build(s: &ShieldState) -> ItemMesh {
        let b = &s.body;
        let (w, hgt, th) = (b.breadth(), b.height(), b.thickness);
        let outline = shield_outline(b);
        let board = Board::new(b);
        let cell = (w.max(hgt) / 64.0).clamp(0.05, 0.16);
        let h = cell * 1e-3;
        // The field's uv, centred on the fess point where the charge sits (`Field::surface`).
        let uv_rect = Some(([-w * 0.5, charge_height(b) - hgt * 0.5], [w, hgt]));
        let shell = plate(&outline, &[], &board, &PlateOptions { thickness: th, cell: [cell, cell], crease: 0.25, uv_rect, ..Default::default() });

        let mut parts = Parts::default();
        // Bare boards show their planks; paint shows the field's division.
        let planks = (b.planks > 0 && b.material == Wood)
            .then_some(SurfacePattern { kind: PatternKind::Planks, color: s.palette.wood, scale: b.planks as f32 });
        if s.field.painted {
            parts.add("body", MeshBuilder::merged([&shell.back, &shell.edge]), b.material);
            let paint = Look { pattern: s.field.surface(), ..Look::of(Paint).tinted(s.field.tincture.rgb()) };
            parts.add_look("face", shell.face, paint);
        } else {
            parts.add_look("body", shell.merged(), Look { pattern: planks, ..Look::of(b.material) });
        }

        // The charge, a thin plate on the face.
        let centre = [0.0, charge_height(b)];
        if s.field.charge != Device::None {
            let device = s.field.charge;
            let size = s.field.charge_size * w.min(hgt * 0.8);
            let depth = (th * 0.1).max(0.015);
            let options = PlateOptions { thickness: depth, offset: th * 0.5 + depth * 0.5 - 0.004, cell: [cell * 0.5, cell * 0.5], ..Default::default() };
            let mut charge = MeshBuilder::new();
            let place = |p: &P2| [p[0] * size + centre[0], p[1] * size + centre[1]];
            for (piece, holes) in device_shapes(device, s.seed) {
                let piece: Vec<P2> = piece.iter().map(place).collect();
                let holes: Vec<Vec<P2>> = holes.iter().map(|h| h.iter().map(place).collect()).collect();
                charge.append(&plate(&piece, &holes, &board, &options).merged());
            }
            if s.field.painted {
                parts.add_tinted("charge", charge, Paint, s.field.charge_tincture.rgb());
            } else {
                parts.add("charge", charge, applied_metal(s.field.charge_tincture));
            }
        }

        if s.rim.on {
            let wall = (s.rim.width * 0.22).clamp(0.015, 0.045);
            let reach = s.rim.width * 0.6 + 0.02;
            let section = rounded_rect([-(th * 0.5 + wall), -reach], [th * 0.5 + wall, wall], 0.55, 20);
            let path = resample_closed(&outline, ((perimeter(&outline) / (cell * 0.6)).ceil() as usize).max(48));
            parts.add("rim", sweep_on_surface(&board, &path, true, 0.0, &section, h), s.rim.material);
        }

        let boss_at = [0.0, boss_height(b)];
        if s.boss.on {
            let r = s.boss.size * 0.5;
            let (origin, normal) = on(&board, boss_at, th * 0.5 - 0.01, h);
            let mut boss = boss_mesh(s.boss.style, r);
            boss.place(origin, normal);
            parts.add("boss", boss, s.boss.material);
            let n = if r > 0.5 { 6 } else { 4 };
            let ring: Vec<P2> = (0..n).map(|k| {
                let a = PI * 2.0 * k as f32 / n as f32 + PI / n as f32;
                [boss_at[0] + a.cos() * r * 0.84, boss_at[1] + a.sin() * r * 0.84]
            }).collect();
            parts.add("boss_rivets", scatter_on_surface(&board, &rivet_head(r * 0.07 + 0.012, 10), &ring, th * 0.5 + r * 0.05, h), s.boss.material);
        }

        if s.studs.count > 0 {
            let radius = (th * 0.28).clamp(0.035, 0.07);
            let inset = if s.rim.on { s.rim.width * 0.6 + 0.02 } else { 0.0 } + radius * 1.8 + 0.03;
            let inner = offset_ring(&resample_closed(&outline, 256), inset);
            let at = resample_closed(&inner, s.studs.count as usize);
            parts.add("studs", scatter_on_surface(&board, &rivet_head(radius, 12), &at, th * 0.5 - 0.004, h), s.studs.material);
        }

        // Behind: a grip across the back of the boss, or arm straps.
        if s.boss.on && !matches!(b.shape, ShieldShape::Heater | ShieldShape::Kite | ShieldShape::Pavise) {
            let span = (s.boss.size * 0.55).max(0.5).min(w * 0.4);
            let path: Vec<V3> = (0..=12)
                .map(|k| {
                    let t = k as f32 / 12.0;
                    let x = boss_at[0] - span + 2.0 * span * t;
                    let lift = 0.02 + 0.12 * (PI * t).sin().max(0.0).powf(0.4);
                    on(&board, [x, boss_at[1]], -th * 0.5 - lift, h).0
                })
                .collect();
            let bar_material = if b.material == Wood { Wood } else { Iron };
            parts.add("grip", tube(&path, 10, &SweepOptions::default(), |_| 0.05), bar_material);
        } else {
            parts.add("straps", straps(&board, b, &outline, h), Leather);
        }
        parts.finish()
    }
}


impl Shield {
    /// "Forge new" forced to one archetype, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, a)` when `a` is what `s` rolls.
    pub fn roll_as(seed: u32, archetype: ShieldArchetype) -> ShieldState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, archetype)
    }

    /// The board's outline in its own plane (x across, y up, centred), counter-clockwise.
    pub fn outline(body: &ShieldBody) -> Vec<P2> {
        shield_outline(body)
    }
}

/// The surface the board is bent onto: x and y in the outline's plane map to item space,
/// the centre to the origin, the face toward +Z. Lengths along the face are kept, so the
/// outline keeps its size.
#[derive(Clone, Copy, Debug)]
struct Board {
    bend: ShieldBend,
    /// Bend radius (cylinder, dome) or ridge slope.
    k: f32,
    /// Gentle bow along the height, as on a real heater or kite.
    bow: f32,
}

impl Board {
    fn new(b: &ShieldBody) -> Board {
        let (w, hgt) = (b.breadth(), b.height());
        let curve = b.curve.clamp(0.0, 1.0);
        match b.bend {
            // Half the arc subtends `curve` radians at most (about 57 degrees).
            ShieldBend::Cylinder => Board { bend: b.bend, k: (w * 0.5) / (curve * 1.0).max(1e-3), bow: curve * 0.03 / hgt },
            ShieldBend::Dome => Board { bend: b.bend, k: (w.max(hgt) * 0.5) / (curve * 1.1).max(1e-3), bow: 0.0 },
            ShieldBend::Ridge => Board { bend: b.bend, k: curve * 0.32, bow: curve * 0.02 / hgt },
        }
    }
}

impl Surface for Board {
    fn point(&self, p: P2) -> V3 {
        let [x, y] = p;
        let bow = -self.bow * y * y;
        match self.bend {
            ShieldBend::Cylinder => {
                let a = x / self.k;
                [self.k * a.sin(), y, self.k * (a.cos() - 1.0) + bow]
            }
            ShieldBend::Dome => {
                let r = x.hypot(y);
                if r < 1e-6 {
                    return [x, y, -r * r / (2.0 * self.k)];
                }
                let a = r / self.k;
                let s = self.k * a.sin() / r;
                [x * s, y * s, self.k * (a.cos() - 1.0)]
            }
            ShieldBend::Ridge => [x, y, -self.k * x.abs() - 0.02 * x * x + bow],
        }
    }
}

fn on(surface: &impl Surface, p: P2, offset: f32, h: f32) -> (V3, V3) {
    crate::build::plate::on_surface(surface, p, offset, h)
}

/// Where the charge sits: centred on round shields, high on pointed ones.
fn charge_height(b: &ShieldBody) -> f32 {
    b.height()
        * match b.shape {
            ShieldShape::Heater => 0.08,
            ShieldShape::Kite => 0.18,
            ShieldShape::Pavise => 0.05,
            _ => 0.0,
        }
}

/// Where the boss sits: over the hand, at the widest part.
fn boss_height(b: &ShieldBody) -> f32 {
    b.height()
        * match b.shape {
            ShieldShape::Heater => 0.1,
            ShieldShape::Kite => 0.2,
            _ => 0.0,
        }
}

fn perimeter(ring: &[P2]) -> f32 {
    (0..ring.len()).map(|k| {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        (b[0] - a[0]).hypot(b[1] - a[1])
    }).sum()
}

/// The outline, counter-clockwise, `width * size` across and `size` tall, centred.
fn shield_outline(b: &ShieldBody) -> Vec<P2> {
    let (w, hgt) = (b.breadth(), b.height());
    let half = |hw: &dyn Fn(f32) -> f32, n: usize| -> Vec<P2> {
        // Right side top to bottom, then the left side bottom to top, from a half-width
        // function of y in -0.5..=0.5 (scaled by the shield's size).
        let mut pts: Vec<P2> = (0..=n).map(|i| {
            let y = 0.5 - i as f32 / n as f32;
            [hw(y) * w, y * hgt]
        }).collect();
        let left: Vec<P2> = pts.iter().rev().map(|&[x, y]| [-x, y]).collect();
        pts.extend(left);
        pts
    };
    let raw: Vec<P2> = match b.shape {
        ShieldShape::Round | ShieldShape::Buckler | ShieldShape::Oval => (0..128).map(|k| {
            let a = PI * 2.0 * k as f32 / 128.0;
            [a.cos() * w * 0.5, a.sin() * hgt * 0.5]
        }).collect(),
        ShieldShape::Heater => half(&|y: f32| {
            if y > 0.08 { 0.5 } else { 0.5 * (1.0 - (1.0 - ((y + 0.5) / 0.58).max(0.0)).powf(1.8)).max(0.0).powf(0.6) }
        }, 96),
        ShieldShape::Kite => half(&|y: f32| {
            // Widest at y = 0.2: an elliptical crown above, a sine taper below that leaves the
            // widest point level and closes to a point.
            if y > 0.2 {
                0.5 * (1.0 - ((y - 0.2) / 0.3).powi(2)).max(0.0).sqrt()
            } else {
                0.5 * (std::f32::consts::FRAC_PI_2 * ((y + 0.5) / 0.7).clamp(0.0, 1.0)).sin()
            }
        }, 128),
        ShieldShape::Tower => half(&|y: f32| {
            let e = (y.abs() - 0.4).max(0.0) / 0.1;
            0.5 * (1.0 - e.powi(2)).max(0.0).powf(0.35)
        }, 96),
        ShieldShape::Pavise => {
            // Flat foot, sides widening a little upward, shoulders sloping in to a flat top.
            vec![[-0.44, -0.5], [0.44, -0.5], [0.5, 0.36], [0.36, 0.5], [-0.36, 0.5], [-0.5, 0.36]]
                .into_iter()
                .map(|[x, y]| [x * w, y * hgt])
                .collect()
        }
    };
    let mut ring = dedup_ring(raw);
    if signed_area(&ring) < 0.0 {
        ring.reverse();
    }
    let corner = (w.min(hgt) * 0.035).clamp(0.05, 0.25);
    dedup_ring(fillet(&ring, corner, 0.35, 6))
}

fn dedup_ring(mut ring: Vec<P2>) -> Vec<P2> {
    ring.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-5);
    while ring.len() > 2 && {
        let (a, b) = (ring[0], ring[ring.len() - 1]);
        (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-5
    } {
        ring.pop();
    }
    ring
}

/// A boss of flange radius `r`, base on y = 0: a crisp flange ring, then the dome, cone or
/// spiked dome.
fn boss_mesh(style: BossStyle, r: f32) -> MeshBuilder {
    let flange = lathe(&[[r, 0.0], [r, r * 0.06], [r * 0.64, r * 0.1]], &LatheOptions { facets: 40, shading: Shading::BANDED, cap_start: true, cap_end: false, phase: 0.0 });
    let control: Vec<[f32; 2]> = match style {
        BossStyle::Dome => vec![[r * 0.64, r * 0.1], [r * 0.6, r * 0.3], [r * 0.48, r * 0.5], [r * 0.26, r * 0.64], [0.0, r * 0.68]],
        BossStyle::Cone => vec![[r * 0.64, r * 0.1], [r * 0.58, r * 0.22], [r * 0.3, r * 0.6], [r * 0.08, r * 0.86], [0.0, r * 0.9]],
        BossStyle::Spike => vec![[r * 0.64, r * 0.1], [r * 0.58, r * 0.3], [r * 0.4, r * 0.5], [r * 0.14, r * 0.62], [r * 0.08, r * 0.8], [0.0, r * 1.25]],
    };
    let dome = lathe(&spline_profile(&control, 24), &LatheOptions { facets: 40, shading: Shading::SMOOTH, cap_start: false, cap_end: false, phase: 0.0 });
    MeshBuilder::merged([&flange, &dome])
}

/// Two leather arm straps across the back, bowed off it for the forearm and the fist.
fn straps(board: &Board, b: &ShieldBody, outline: &[P2], h: f32) -> MeshBuilder {
    let (w, hgt, th) = (b.breadth(), b.height(), b.thickness);
    let mut out = MeshBuilder::new();
    let band = rounded_rect([-0.012, -0.07], [0.012, 0.07], 0.3, 12);
    for (y, reach, lift) in [(hgt * 0.14, 0.32, 0.1), (-hgt * 0.1, 0.42, 0.16)] {
        let y = y + boss_height(b) * 0.5;
        // Keep the straps inside the board at this height.
        let inside = outline.iter().filter(|p| (p[1] - y).abs() < hgt * 0.05).map(|p| p[0].abs()).fold(w * 0.5, f32::min);
        let half = (w * reach).min(inside * 0.8);
        let path: Vec<V3> = (0..=16)
            .map(|k| {
                let t = k as f32 / 16.0;
                let x = -half + 2.0 * half * t;
                let off = 0.015 + lift * (PI * t).sin().max(0.0).powf(0.6);
                crate::build::plate::on_surface(board, [x, y], -th * 0.5 - off, h).0
            })
            .collect();
        let up = vec3::neg(board.normal([0.0, y], h));
        out.append(&sweep(&path, &band, &SweepOptions { up: Some(up), ..Default::default() }, |_| 1.0));
    }
    out
}

/// Metal for a charge on a bare face: gold for Or, silver for Argent, brass otherwise.
fn applied_metal(t: Tincture) -> MaterialKind {
    match t {
        Tincture::Or => Gold,
        Tincture::Argent => Silver,
        _ => Brass,
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, a: ShieldArchetype) -> ShieldState {
    use ShieldArchetype as A;
    let mut s = ShieldState { seed, ..ShieldState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    // The prototype's generic pass: most sliders drift up to 15% of their range from base.
    s.nudge(rng, &LIMITS, 0.75, 0.3);

    let body = &mut s.body;
    match a {
        A::Round => {
            body.shape = ShieldShape::Round;
            body.size = rng.range_f32(7.0, 9.0);
            body.width = 1.0;
            body.bend = ShieldBend::Dome;
            body.curve = rng.range_f32(0.12, 0.35);
            body.thickness = rng.range_f32(0.1, 0.18);
            body.material = Wood;
            body.planks = rng.int(5, 9);
        }
        A::Targe => {
            body.shape = ShieldShape::Round;
            body.size = rng.range_f32(5.0, 6.6);
            body.width = 1.0;
            body.bend = ShieldBend::Dome;
            body.curve = rng.range_f32(0.05, 0.2);
            body.thickness = rng.range_f32(0.12, 0.2);
            body.material = Leather;
            body.planks = rng.int(2, 4);
        }
        A::Heater => {
            body.shape = ShieldShape::Heater;
            body.size = rng.range_f32(6.2, 7.8);
            body.width = rng.range_f32(0.8, 0.9);
            body.bend = ShieldBend::Cylinder;
            body.curve = rng.range_f32(0.25, 0.6);
            body.thickness = rng.range_f32(0.1, 0.16);
            body.material = Wood;
            body.planks = rng.int(3, 6);
        }
        A::Kite => {
            body.shape = ShieldShape::Kite;
            body.size = rng.range_f32(9.0, 11.0);
            body.width = rng.range_f32(0.58, 0.68);
            body.bend = ShieldBend::Cylinder;
            body.curve = rng.range_f32(0.4, 0.75);
            body.thickness = rng.range_f32(0.1, 0.16);
            body.material = Wood;
            body.planks = rng.int(3, 6);
        }
        A::Scutum => {
            body.shape = if rng.chance(0.6) { ShieldShape::Tower } else { ShieldShape::Oval };
            body.size = rng.range_f32(9.0, 11.0);
            body.width = if body.shape == ShieldShape::Tower { rng.range_f32(0.55, 0.68) } else { rng.range_f32(0.62, 0.76) };
            body.bend = ShieldBend::Cylinder;
            body.curve = rng.range_f32(0.6, 0.95);
            body.thickness = rng.range_f32(0.1, 0.15);
            body.material = Wood;
            body.planks = rng.int(4, 8);
        }
        A::Pavise => {
            body.shape = ShieldShape::Pavise;
            body.size = rng.range_f32(10.0, 11.6);
            body.width = rng.range_f32(0.55, 0.67);
            body.bend = ShieldBend::Ridge;
            body.curve = rng.range_f32(0.3, 0.7);
            body.thickness = rng.range_f32(0.14, 0.22);
            body.material = Wood;
            body.planks = rng.int(3, 5);
        }
        A::Buckler => {
            body.shape = ShieldShape::Buckler;
            body.size = rng.range_f32(3.0, 3.8);
            body.width = 1.0;
            body.bend = ShieldBend::Dome;
            body.curve = rng.range_f32(0.45, 0.9);
            body.thickness = rng.range_f32(0.05, 0.08);
            body.material = *rng.weighted(&[(Iron, 6), (Brass, 2), (Wood, 1)]);
            body.planks = if body.material == Wood { rng.int(2, 4) } else { 0 };
        }
    }
    let body = s.body;

    // Field: painted with arms, or bare.
    let painted = match a {
        A::Targe => false,
        A::Buckler => body.material == Wood && rng.chance(0.5) || rng.chance(0.15),
        A::Round => rng.chance(0.8),
        _ => true,
    };
    let divisions: &[(Division, u32)] = match a {
        A::Round | A::Buckler => &[(Division::Plain, 4), (Division::Quarterly, 2), (Division::PerPale, 2), (Division::PerSaltire, 1), (Division::Barry, 1)],
        A::Pavise => &[(Division::Bendy, 3), (Division::Paly, 3), (Division::PerPale, 2), (Division::Quarterly, 1), (Division::Checky, 1), (Division::Plain, 1)],
        A::Scutum => &[(Division::Plain, 5), (Division::PerFess, 1), (Division::Barry, 1)],
        _ => &[
            (Division::Plain, 5), (Division::PerPale, 2), (Division::PerFess, 2), (Division::Quarterly, 2), (Division::PerBend, 2),
            (Division::PerChevron, 2), (Division::PerSaltire, 1), (Division::Paly, 1), (Division::Barry, 1), (Division::Bendy, 1),
            (Division::Checky, 1), (Division::Lozengy, 1),
        ],
    };
    let charge_odds = match a {
        A::Heater => 0.65,
        A::Pavise => 0.7,
        A::Kite | A::Scutum => 0.4,
        A::Round => 0.2,
        A::Buckler => 0.2,
        A::Targe => 0.0,
    };
    s.field = roll_field(rng, painted, divisions, charge_odds);

    // Rim.
    s.rim.on = rng.chance(match a {
        A::Targe => 0.5,
        A::Buckler => 0.75,
        _ => 0.85,
    });
    s.rim.material = *rng.weighted(match a {
        A::Round => &[(Leather, 3), (Iron, 2)][..],
        A::Targe => &[(Brass, 2), (Leather, 1)],
        A::Buckler => &[(Iron, 2), (Brass, 2)],
        A::Scutum => &[(Brass, 3), (Iron, 1)],
        _ => &[(Iron, 4), (Leather, 2), (Brass, 2)],
    });
    s.rim.width = match a {
        A::Buckler => rng.range_f32(0.05, 0.09),
        _ => rng.range_f32(0.07, 0.14),
    };

    // Boss.
    s.boss.on = match a {
        A::Round | A::Buckler => true,
        A::Targe => rng.chance(0.6),
        A::Kite => rng.chance(0.45),
        A::Scutum => rng.chance(0.85),
        A::Heater | A::Pavise => false,
    };
    s.boss.style = *rng.weighted(match a {
        A::Targe => &[(BossStyle::Spike, 3), (BossStyle::Dome, 2)][..],
        A::Buckler => &[(BossStyle::Dome, 3), (BossStyle::Cone, 2), (BossStyle::Spike, 2)],
        A::Round => &[(BossStyle::Dome, 4), (BossStyle::Cone, 2)],
        _ => &[(BossStyle::Dome, 4), (BossStyle::Cone, 1)],
    });
    s.boss.size = match a {
        A::Round => rng.range_f32(1.2, 1.8),
        A::Targe => rng.range_f32(0.8, 1.2),
        A::Buckler => body.breadth() * rng.range_f32(0.3, 0.42),
        A::Kite => rng.range_f32(0.6, 0.9),
        _ => rng.range_f32(1.1, 1.6),
    };
    if s.boss.on {
        // The boss sits where the charge would: one or the other.
        s.field.charge = Device::None;
    }
    s.boss.material = *rng.weighted(match a {
        A::Targe => &[(Brass, 3), (Iron, 1)][..],
        A::Scutum => &[(Brass, 2), (Iron, 2)],
        A::Buckler if body.material == Brass => &[(Brass, 1)],
        _ => &[(Iron, 5), (Brass, 1)],
    });

    // Studs.
    s.studs.count = match a {
        A::Round => *rng.pick(&[0, 8, 12, 16]),
        A::Targe => rng.int(16, 32),
        A::Buckler => *rng.pick(&[0, 0, 8, 12]),
        A::Heater | A::Pavise => *rng.pick(&[0, 0, 0, 8]),
        _ => *rng.pick(&[0, 0, 8, 10]),
    };
    s.studs.material = if s.rim.on && s.rim.material != Leather { s.rim.material } else { *rng.pick(&[Iron, Brass]) };

    let mut targets = vec![(EtchTarget::Metals, 3)];
    if !s.field.painted && body.material == Wood {
        targets.push((EtchTarget::Wood, 1));
    }
    if body.material == Leather {
        targets.push((EtchTarget::Leather, 3));
    }
    let odds = match a {
        A::Buckler => 0.5,
        A::Targe => 0.4,
        _ => 0.25,
    };
    s.etch = Etching::roll(rng, odds, &targets, 1);

    s.fit(&LIMITS);
    s
}

/// A field that keeps the rule of tincture: two tinctures of opposite kinds for a divided
/// field, and a charge that stands out from the field.
pub(crate) fn roll_field(rng: &mut Mulberry32, painted: bool, divisions: &[(Division, u32)], charge_odds: f64) -> Field {
    let metal_field = rng.chance(0.3);
    let (first, second): (&[Tincture], &[Tincture]) =
        if metal_field { (&Tincture::METALS, &Tincture::COLOURS) } else { (&Tincture::COLOURS, &Tincture::METALS) };
    let tincture = *rng.pick(first);
    let tincture2 = *rng.pick(second);
    let division = *rng.weighted(divisions);
    let charge = if rng.chance(charge_odds) {
        *rng.weighted(&[(Device::Cross, 3), (Device::Star, 2), (Device::Crescent, 2), (Device::Tower, 2), (Device::Wheel, 1), (Device::Rune, 1)])
    } else {
        Device::None
    };
    let charge_tincture = if !painted {
        *rng.pick(&Tincture::METALS)
    } else if division == Division::Plain {
        *rng.pick(second)
    } else {
        // On a divided field, a third tincture of the field's opposite kind where one is left.
        let others: Vec<Tincture> = second.iter().copied().filter(|&t| t != tincture2).collect();
        if others.is_empty() { *rng.pick(first) } else { *rng.pick(&others) }
    };
    let charge_size = if division == Division::Plain { rng.range_f32(0.4, 0.62) } else { rng.range_f32(0.3, 0.45) };
    Field {
        painted,
        division: if painted { division } else { Division::Plain },
        tincture,
        tincture2,
        charge,
        charge_tincture,
        charge_size,
    }
}
