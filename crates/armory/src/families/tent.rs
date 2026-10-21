//! Tents: a ridge tent (an A-frame of canvas on a ridge pole and two uprights) or a bell tent
//! (a cone of canvas on one centre pole), pitched with guy lines, pegs and door flaps, or
//! packed as a rolled bundle of canvas with its poles, strapped.
//!
//! Neither is in the prototypes; the sliders here are new. Item space: a pitched tent stands
//! on the ground (y = 0) centred over the origin, a ridge tent with its ridge along X and its
//! door at the +X gable, a bell tent with its door toward +X. A packed tent lies along X.
//!
//! The pitched ridge tent is about 10 to 14 units (1 to 1.4 m) at the ridge pole, wide enough
//! for two to sleep (SRD 5.1 p. 68, "Tent, two-person", 20 lb). The packed form is the same
//! tent's canvas rolled: its girth grows with the canvas area, so a bigger tent is a fatter
//! roll.

use std::f32::consts::{PI, TAU};

use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use crate::build::chain::{cord, thong};
use crate::build::parts::{Look, Parts};
use crate::build::roll::{roll_straps, rolled_sheet};
use crate::cond::{Condition, ConditionLimits};
use crate::fabric::{Fabric, FabricPattern};
use crate::families::Generator;
use crate::geom::{
    GridOptions, ItemMesh, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, cuboid, frustum, icosphere, lathe, spline_profile, thick_sheet, torus,
    tube, vec3,
};
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::slider::{Sliders, sliders};

use MaterialKind::{Brass, Cloth, Cord, Leather, Wood};

/// The tent generator.
pub struct Tent;

/// Pitched on the ground, or rolled up with its poles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TentForm {
    Pitched,
    Packed,
}

/// The tent's cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TentStyle {
    /// An A-frame: two sloped panels, two gables, a ridge pole on uprights.
    Ridge,
    /// A cone of canvas round one centre pole, with a short wall.
    Bell,
}

/// What the canvas is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Canvas {
    Linen,
    Hemp,
    /// Waxed canvas, dark and slick.
    Oilcloth,
    /// Sewn hides.
    Hide,
}

impl Canvas {
    pub const ALL: [Canvas; 4] = [Canvas::Linen, Canvas::Hemp, Canvas::Oilcloth, Canvas::Hide];

    /// What the shader makes it of.
    pub fn material(self) -> MaterialKind {
        if self == Canvas::Hide { Leather } else { Cloth }
    }

    /// The dyes this canvas is made in.
    pub fn colors(self) -> [Rgb; 3] {
        match self {
            Canvas::Linen => [Rgb::hex(0xe6dfcc), Rgb::hex(0xd8c9a0), Rgb::hex(0xcbbf9f)],
            Canvas::Hemp => [Rgb::hex(0xb59a68), Rgb::hex(0xa58a5a), Rgb::hex(0x9c8458)],
            Canvas::Oilcloth => [Rgb::hex(0x5d5b3a), Rgb::hex(0x7a5b34), Rgb::hex(0x3f4a3a)],
            Canvas::Hide => [Rgb::hex(0x7a5a3c), Rgb::hex(0x5e4330), Rgb::hex(0x8b6a47)],
        }
    }
}

/// The tent itself.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Canopy {
    pub form: TentForm,
    pub style: TentStyle,
    pub canvas: Canvas,
    pub color: Rgb,
    pub pattern: FabricPattern,
    pub color2: Rgb,
    /// Across the eaves of a ridge tent, or the diameter of a bell tent's base.
    pub width: f32,
    /// Along a ridge tent's ridge. A packed bell tent rolls to a length of its own.
    pub length: f32,
    /// To the top of the pole.
    pub height: f32,
    /// How far the door flaps are tied back, 0 shut to 1 wide.
    pub door_open: f32,
    /// Guy lines, up to eight.
    pub guys: u32,
    /// Pegs along each eave of a ridge tent, or in each quarter of a bell tent's ring.
    pub pegs: u32,
    /// Sewn patches on the canvas.
    pub patches: u32,
}

sliders! {
    CanopyLimits for Canopy {
        width: 10.0, 26.0, 0.5;
        length: 12.0, 22.0, 0.5;
        height: 10.0, 14.0, 0.25;
        door_open: 0.0, 1.0, 0.01;
        guys: 0.0, 8.0, 1.0;
        pegs: 0.0, 4.0, 1.0;
        patches: 0.0, 6.0, 1.0;
    }
}

impl Default for Canopy {
    fn default() -> Self {
        Canopy {
            form: TentForm::Pitched,
            style: TentStyle::Ridge,
            canvas: Canvas::Linen,
            color: Canvas::Linen.colors()[0],
            pattern: FabricPattern::Plain,
            color2: Fabric::PATTERN_DYES[0],
            width: 14.0,
            length: 18.0,
            height: 12.0,
            door_open: 0.5,
            guys: 4,
            pegs: 2,
            patches: 0,
        }
    }
}

/// Every parameter of one tent.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TentState {
    pub seed: u32,
    pub tent: Canopy,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every tent section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TentLimits {
    pub tent: CanopyLimits,
    pub cond: ConditionLimits,
}

/// The tents' slider ranges (new: the prototypes have none).
pub const LIMITS: TentLimits = TentLimits { tent: CanopyLimits::SLIDERS, cond: ConditionLimits::SLIDERS };

impl TentState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &TentLimits) {
        self.tent.fit(&limits.tent);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &TentLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [tag("tent", self.tent.out_of_range(&limits.tent)), tag("cond", self.cond.out_of_range(&limits.cond))].concat()
    }
}

/// SRD 5.1 p. 68: a two-person tent weighs 20 lb.
pub const TENT_POUNDS: f32 = 20.0;

impl Generator for Tent {
    type State = TentState;

    fn roll(seed: u32) -> TentState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&[(TentStyle::Ridge, 3), (TentStyle::Bell, 1)]);
        let form = *rng.weighted(&[(TentForm::Pitched, 1), (TentForm::Packed, 1)]);
        compose(&mut rng, seed, style, form)
    }

    fn noun(s: &TentState) -> String {
        let t = &s.tent;
        match (t.form, t.style) {
            (TentForm::Pitched, TentStyle::Ridge) => "Ridge Tent",
            (TentForm::Pitched, TentStyle::Bell) => "Bell Tent",
            (TentForm::Packed, TentStyle::Ridge) => "Rolled Ridge Tent",
            (TentForm::Packed, TentStyle::Bell) => "Rolled Bell Tent",
        }
        .to_string()
    }

    fn build(s: &TentState) -> ItemMesh {
        let mut parts = Parts::new();
        match s.tent.form {
            TentForm::Pitched => match s.tent.style {
                TentStyle::Ridge => ridge_tent(s, &mut parts),
                TentStyle::Bell => bell_tent(s, &mut parts),
            },
            TentForm::Packed => packed_tent(s, &mut parts),
        }
        parts.rest_on_ground();
        parts.finish()
    }
}

impl Tent {
    /// "Forge new" forced to a style and form, for turntables, bases and tests. Draws the
    /// same stream as [`Generator::roll`], so `roll(s)` equals `roll_as(s, style, form)` for
    /// the style and form `s` rolls.
    pub fn roll_as(seed: u32, style: TentStyle, form: TentForm) -> TentState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        rng.next_f64();
        compose(&mut rng, seed, style, form)
    }

    /// The same tent, rolled up with its poles or pitched.
    pub fn in_form(state: &TentState, form: TentForm) -> TentState {
        let mut s = state.clone();
        s.tent.form = form;
        s
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, style: TentStyle, form: TentForm) -> TentState {
    let mut s = TentState { seed, ..TentState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    let t = &mut s.tent;
    t.form = form;
    t.style = style;
    t.canvas = *rng.weighted(&[(Canvas::Linen, 3), (Canvas::Hemp, 3), (Canvas::Oilcloth, 2), (Canvas::Hide, 1)]);
    t.color = *rng.pick(&t.canvas.colors());
    t.pattern = if t.canvas == Canvas::Linen { *rng.weighted(&[(FabricPattern::Plain, 4), (FabricPattern::Stripes, 1)]) } else { FabricPattern::Plain };
    t.color2 = *rng.pick(&Fabric::PATTERN_DYES);
    match style {
        TentStyle::Ridge => {
            t.width = rng.range_f32(12.0, 17.0);
            t.length = rng.range_f32(15.0, 21.0);
        }
        TentStyle::Bell => {
            t.width = rng.range_f32(16.0, 24.0);
            t.length = rng.range_f32(15.0, 21.0);
        }
    }
    t.height = rng.range_f32(10.5, 13.5);
    t.door_open = rng.range_f32(0.0, 1.0);
    t.guys = rng.int(2, 6);
    t.pegs = rng.int(1, 3);
    t.patches = if rng.chance(0.4) { rng.int(1, 4) } else { 0 };
    s.fit(&LIMITS);
    s
}

// ───────────────────────── Geometry ─────────────────────────

/// A canvas sheet's thickness.
const CANVAS: f32 = 0.035;
/// Height of a ridge tent's vertical skirt.
const WALL: f32 = 0.9;
/// Where the skirt ends and the slope begins, as a share of a panel's rows.
const SKIRT: f32 = 0.12;
/// Rows across a ridge tent's panels and columns along them.
const ROWS: usize = 14;
const COLS: usize = 22;

fn canvas_look(t: &Canopy) -> Look {
    let fabric = Fabric { color: t.color, pattern: t.pattern, color2: t.color2, scale: 2.0 };
    Look::cloth(&fabric).made_of(t.canvas.material())
}

/// A rod from `a` to `b`, `radius` thick, capped.
pub(crate) fn rod(a: V3, b: V3, radius: f32) -> MeshBuilder {
    let mid = vec3::lerp(a, b, 0.5);
    tube(&[a, mid, b], 8, &SweepOptions::default(), |_| radius)
}

const PEG_HEIGHT: f32 = 0.95;
const PEG_LEAN: f32 = 0.35;

/// A tent peg driven in at `base` (on the ground), its top leaning away from the tent along
/// `away`. Returns the peg and its head, where the line is tied.
fn peg(base: V3, away: V3) -> (MeshBuilder, V3) {
    let mut p = frustum(0.12, 0.08, PEG_HEIGHT, 8, Shading::SMOOTH);
    let lean = vec3::normalize_or([away[0], 0.0, away[2]], vec3::X);
    p.rotate(vec3::cross(vec3::Y, lean), PEG_LEAN);
    p.translate(base);
    let head = vec3::add(base, [lean[0] * PEG_HEIGHT * PEG_LEAN.sin(), PEG_HEIGHT * PEG_LEAN.cos(), lean[2] * PEG_HEIGHT * PEG_LEAN.sin()]);
    (p, head)
}

/// One staked guy line: tied to the canvas at `from`, taut down to the head of the peg driven
/// in at `base`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stake {
    pub from: V3,
    pub base: V3,
    pub head: V3,
}

impl Stake {
    /// A line from `from` pegged `reach` of its height out along `away` (horizontal).
    fn new(from: V3, away: V3, reach: f32) -> Stake {
        let away = vec3::normalize_or([away[0], 0.0, away[2]], vec3::X);
        let base = vec3::add([from[0], 0.0, from[2]], vec3::scale(away, from[1] * reach));
        let head = peg(base, away).1;
        Stake { from, base, head }
    }

    fn away(&self) -> V3 {
        vec3::normalize_or([self.base[0] - self.from[0], 0.0, self.base[2] - self.from[2]], vec3::X)
    }

    /// How steeply the line comes down to its peg, in radians from the horizontal.
    pub fn descent(&self) -> f32 {
        let run = ((self.head[0] - self.from[0]).powi(2) + (self.head[2] - self.from[2]).powi(2)).sqrt();
        (self.from[1] - self.head[1]).atan2(run)
    }
}

/// How far out a guy is pegged, as a share of the height it is tied at (about 53 degrees).
const GUY_REACH: f32 = 0.75;

/// Row fraction up a ridge panel where eave and corner guys are tied.
const SLOPE_GUY: f32 = 0.6;

/// The guy lines of a pitched tent, in the order `guys` takes them.
pub fn stakes(t: &Canopy) -> Vec<Stake> {
    match t.style {
        TentStyle::Ridge => {
            let r = Ridge::of(t);
            let end = r.half_l + 0.35;
            let mut all = vec![
                Stake::new([end, r.height - 0.2, 0.0], vec3::X, GUY_REACH),
                Stake::new([-end, r.height - 0.2, 0.0], vec3::neg(vec3::X), GUY_REACH),
                Stake::new(r.panel(1.0, SLOPE_GUY, 0.5), vec3::Z, GUY_REACH),
                Stake::new(r.panel(-1.0, SLOPE_GUY, 0.5), vec3::neg(vec3::Z), GUY_REACH),
            ];
            for (u, sx) in [(1.0, 1.0), (1.0, -1.0), (0.0, 1.0), (0.0, -1.0)] {
                // u = 1 is the front (+X) end; sx picks the side.
                let from = r.panel(sx, SLOPE_GUY, u);
                let along = if u > 0.5 { 1.0 } else { -1.0 };
                all.push(Stake::new(from, [along * 0.6, 0.0, sx], GUY_REACH));
            }
            all.truncate(t.guys as usize);
            all
        }
        TentStyle::Bell => {
            let profile = bell_profile(t);
            let half = bell_door_half(t);
            let (clear, n) = (half + 0.1, t.guys.max(1) as f32);
            (0..t.guys)
                .map(|k| {
                    let a = clear + (TAU - 2.0 * clear) * (k as f32 + 0.5) / n;
                    Stake::new(bell_point(&profile, 0.55, a), [a.cos(), 0.0, -a.sin()], GUY_REACH)
                })
                .collect()
        }
    }
}

/// A guy line from `from` taut to `to`.
fn guy(from: V3, to: V3) -> MeshBuilder {
    let path: Vec<V3> = (0..=12)
        .map(|i| {
            let k = i as f32 / 12.0;
            let mut p = vec3::lerp(from, to, k);
            p[1] -= 0.04 * (PI * k).sin();
            p
        })
        .collect();
    cord(&path, false, 0.045, 5)
}

/// A loop of tape from the skirt at `from` to a peg's head.
fn tape(from: V3, head: V3) -> MeshBuilder {
    thong(&[from, vec3::lerp(from, head, 0.5), head], false, 0.05)
}

/// A pole's finial: a small ball.
fn knob(at: V3, radius: f32) -> MeshBuilder {
    let mut b = icosphere(radius, 1);
    b.translate(at);
    b
}

fn grid(rows: usize, cols: usize, f: impl Fn(f32, f32) -> V3) -> Vec<Vec<V3>> {
    (0..=rows).map(|r| (0..=cols).map(|c| f(r as f32 / rows as f32, c as f32 / cols as f32)).collect()).collect()
}

/// The ridge tent's measures.
struct Ridge {
    half_w: f32,
    half_l: f32,
    /// The canvas's ridge line (a little below the pole top).
    ridge: f32,
    height: f32,
    sag: f32,
}

impl Ridge {
    fn of(t: &Canopy) -> Ridge {
        Ridge { half_w: t.width * 0.5, half_l: t.length * 0.5, ridge: t.height - 0.12, height: t.height, sag: 0.03 * t.length }
    }

    /// The share of the way up a panel past its skirt, at row fraction `t`.
    fn slope(t: f32) -> f32 {
        ((t - SKIRT) / (1.0 - SKIRT)).max(0.0)
    }

    /// Half the width of the cross-section at row fraction `t` (the gable's edge).
    fn half_width_at(&self, t: f32) -> f32 {
        self.half_w * (1.0 - Self::slope(t))
    }

    fn height_at(&self, t: f32) -> f32 {
        if t < SKIRT { WALL * t / SKIRT } else { WALL + (self.ridge - WALL) * Self::slope(t) }
    }

    /// A point on the panel of `side` (+1 or -1) at row fraction `t` (eave to ridge) and
    /// column fraction `u` (back to front).
    fn panel(&self, side: f32, t: f32, u: f32) -> V3 {
        let tt = Self::slope(t);
        let hang = (PI * u).sin();
        let billow = 0.3 * hang * (PI * tt).sin();
        [
            -self.half_l + 2.0 * self.half_l * u,
            self.height_at(t) - self.sag * hang * tt,
            side * (self.half_width_at(t) + billow),
        ]
    }

    /// A point on the gable at x = `end` (+1 front, -1 back) at row `t`, `across` from -1 to 1.
    fn gable(&self, end: f32, t: f32, across: f32) -> V3 {
        [end * self.half_l, self.height_at(t), across * self.half_width_at(t)]
    }
}

fn ridge_tent(s: &TentState, parts: &mut Parts) {
    let t = &s.tent;
    let r = Ridge::of(t);
    let look = canvas_look(t);
    let opts = GridOptions::default();

    // The two sloped panels and the closed back gable.
    let mut canvas = MeshBuilder::new();
    for side in [1.0, -1.0] {
        let mut panel = thick_sheet(&grid(ROWS, COLS, |tt, u| r.panel(side, tt, u)), &opts, CANVAS);
        if side < 0.0 {
            panel.flip();
        }
        canvas.append(&panel);
    }
    canvas.append(&thick_sheet(&grid(ROWS, 10, |tt, a| r.gable(-1.0, tt, a * 2.0 - 1.0)), &opts, CANVAS));
    parts.add_look("canvas", canvas, look);

    // The door: two flaps at the front gable, their inner edges tied back as it opens.
    let open = t.door_open.clamp(0.0, 1.0);
    let mut flaps = MeshBuilder::new();
    for side in [1.0, -1.0] {
        let flap = grid(ROWS, 6, |tt, a| {
            // a = 0 at the outer edge (sewn to the panel), 1 at the free edge.
            let outer = r.half_width_at(tt);
            let inner = outer * 0.9 * open;
            let z = side * (outer + (inner - outer) * a);
            let bulge = 0.35 * open * (PI * a).sin() * (PI * Ridge::slope(tt)).sin().max(0.0);
            [r.half_l + bulge + 0.04, r.height_at(tt), z]
        });
        let mut m = thick_sheet(&flap, &opts, CANVAS);
        if side < 0.0 {
            m.flip();
        }
        flaps.append(&m);
    }
    parts.add_look("flaps", flaps, look);

    // The sewn-in groundsheet.
    let mut floor = cuboid([t.length - 0.2, 0.06, t.width - 0.2]);
    floor.translate([0.0, 0.03, 0.0]);
    parts.add_look("groundsheet", floor, Look::of(Leather).tinted(t.color.scaled(0.55)));

    if t.patches > 0 {
        let mut rng = Mulberry32::new(s.seed.wrapping_mul(0x9e37_79b9).wrapping_add(0x7e47));
        let (mut patches, mut stitches) = (MeshBuilder::new(), MeshBuilder::new());
        for _ in 0..t.patches {
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let (u0, v0) = (rng.range_f32(0.1, 0.7), rng.range_f32(0.25, 0.7));
            let (du, dv) = (rng.range_f32(0.14, 0.22), rng.range_f32(0.1, 0.16));
            let (cloth, thread) = patch(|v, u| r.panel(side, v, u), v0, u0, dv, du, |p| [p[0], r.ridge * 0.3, 0.0]);
            patches.append(&cloth);
            stitches.append(&thread);
        }
        parts.add_look("patches", patches, Look::of(t.canvas.material()).tinted(patch_color(t.color)));
        parts.add("stitches", stitches, Cord);
    }

    // Poles: a ridge pole on two uprights standing just outside the gables.
    let mut poles = MeshBuilder::new();
    let end = r.half_l + 0.35;
    poles.append(&rod([-end - 0.4, r.height, 0.0], [end + 0.4, r.height, 0.0], 0.14));
    for x in [end, -end] {
        poles.append(&rod([x, 0.0, 0.0], [x, r.height, 0.0], 0.15));
        poles.append(&knob([x, r.height + 0.12, 0.0], 0.2));
    }
    parts.add("poles", poles, Wood);

    // Guy lines to the ridge ends, the eave middles and the eave corners, and the pegs they
    // and the skirt's tape loops are staked with. Every peg has a line tied to it.
    let (mut guys, mut pegs) = (MeshBuilder::new(), MeshBuilder::new());
    for st in stakes(t) {
        guys.append(&guy(st.from, st.head));
        pegs.append(&peg(st.base, st.away()).0);
    }
    for side in [1.0, -1.0] {
        for k in 0..t.pegs {
            let x = -r.half_l + 2.0 * r.half_l * (k as f32 + 0.5) / t.pegs as f32;
            let (p, head) = peg([x, 0.0, side * (r.half_w + 0.3)], [0.0, 0.0, side]);
            pegs.append(&p);
            guys.append(&tape([x, 0.12, side * r.half_w], head));
        }
    }
    if !guys.is_empty() {
        parts.add("guys", guys, Cord);
    }
    if !pegs.is_empty() {
        parts.add("pegs", pegs, Wood);
    }
}

/// A sewn patch over the surface `f(v, u)`: the cell `dv` by `du` from (`v0`, `u0`) standing
/// proud of it, away from the axis point `centre_of(p)` gives for each point, with a dashed
/// row of stitches round its rim. Returns `(cloth, stitches)`.
fn patch(f: impl Fn(f32, f32) -> V3, v0: f32, u0: f32, dv: f32, du: f32, centre_of: impl Fn(V3) -> V3) -> (MeshBuilder, MeshBuilder) {
    const N: usize = 6;
    let lift = |v: f32, u: f32, by: f32| {
        let p = f(v, u);
        let n = vec3::normalize_or(vec3::cross(vec3::sub(f(v + 0.01, u), p), vec3::sub(f(v, u + 0.01), p)), vec3::Y);
        let n = if vec3::dot(n, vec3::sub(p, centre_of(p))) < 0.0 { vec3::neg(n) } else { n };
        vec3::add(p, vec3::scale(n, by))
    };
    let at = |r: usize, c: usize| (v0 + dv * r as f32 / N as f32, u0 + du * c as f32 / N as f32);
    let g: Vec<Vec<V3>> = (0..=N)
        .map(|r| {
            (0..=N)
                .map(|c| {
                    let (v, u) = at(r, c);
                    lift(v, u, PATCH_LIFT)
                })
                .collect()
        })
        .collect();
    let cloth = thick_sheet(&g, &GridOptions::default(), CANVAS * 1.4);
    // Stitches: short dashes along a line just inside each edge.
    let mut stitches = MeshBuilder::new();
    let inset = 0.12;
    let k = 4 * N;
    let edge: Vec<(f32, f32)> = (0..k)
        .map(|i| {
            let q = (i as f32 / k as f32) * 4.0;
            let (side, t) = (q.floor() as i32, q.fract());
            let (a, b) = match side {
                0 => (inset + (1.0 - 2.0 * inset) * t, inset),
                1 => (1.0 - inset, inset + (1.0 - 2.0 * inset) * t),
                2 => (1.0 - inset - (1.0 - 2.0 * inset) * t, 1.0 - inset),
                _ => (inset, 1.0 - inset - (1.0 - 2.0 * inset) * t),
            };
            (v0 + dv * b, u0 + du * a)
        })
        .collect();
    for i in (0..edge.len()).step_by(2) {
        let (p, q) = (edge[i], edge[(i + 1) % edge.len()]);
        stitches.append(&rod(lift(p.0, p.1, PATCH_LIFT + CANVAS * 2.2), lift(q.0, q.1, PATCH_LIFT + CANVAS * 2.2), 0.022));
    }
    (cloth, stitches)
}

/// How far a patch stands off the canvas.
const PATCH_LIFT: f32 = 0.09;

/// A patch's cloth: the canvas shifted toward walnut, so it reads as a different bolt.
fn patch_color(canvas: Rgb) -> Rgb {
    let w = Rgb::hex(0x8a5a2b);
    let m = |a: f32, b: f32| a * 0.55 + b * 0.45;
    Rgb(m(canvas.0, w.0), m(canvas.1, w.1), m(canvas.2, w.2))
}

/// A bell tent's profile, base to apex: radius and height.
fn bell_profile(t: &Canopy) -> Vec<[f32; 2]> {
    let (r, h) = (t.width * 0.5, t.height);
    let wall = WALL * 1.4;
    let rise = h - wall;
    spline_profile(
        &[[r, 0.0], [r, wall], [r * 0.9, wall + rise * 0.22], [r * 0.66, wall + rise * 0.52], [r * 0.36, wall + rise * 0.8], [r * 0.12, h - 0.4], [0.0, h - 0.15]],
        40,
    )
}

/// The point on a bell tent's canvas at fraction `v` up its profile and `a` radians round
/// from the door at +X.
fn bell_point(profile: &[[f32; 2]], v: f32, a: f32) -> V3 {
    let f = v.clamp(0.0, 1.0) * (profile.len() - 1) as f32;
    let i = (f.floor() as usize).min(profile.len() - 2);
    let k = f - i as f32;
    let (p, q) = (profile[i], profile[i + 1]);
    let (r, y) = (p[0] + (q[0] - p[0]) * k, p[1] + (q[1] - p[1]) * k);
    [r * a.cos(), y, -r * a.sin()]
}

/// Half the angle of a bell tent's doorway.
fn bell_door_half(t: &Canopy) -> f32 {
    (2.8 / (t.width * 0.5)).clamp(0.15, 0.45)
}

fn bell_tent(s: &TentState, parts: &mut Parts) {
    let t = &s.tent;
    let look = canvas_look(t);
    let r = t.width * 0.5;
    let profile = bell_profile(t);
    let opts = GridOptions::default();

    let mut shell = lathe(&profile, &LatheOptions { facets: 56, shading: Shading::SMOOTH, cap_start: false, cap_end: false, phase: 0.0 });
    shell.make_double_sided();
    parts.add_look("canvas", shell, look);

    // The door: a dark doorway cut in the cone's wall, two flaps tied back from it.
    let door_top = 0.5;
    let half = bell_door_half(t);
    let open = t.door_open.clamp(0.0, 1.0);
    let way = grid(6, 6, |v, a| bell_point(&profile, v * door_top, (a * 2.0 - 1.0) * half));
    let mut doorway = thick_sheet(&way, &opts, CANVAS);
    doorway.translate([0.04, 0.0, 0.0]);
    parts.add_look("doorway", doorway, Look::of(Cloth).tinted(Rgb::hex(0x1a1612)).without_outline());
    let mut flaps = MeshBuilder::new();
    for side in [1.0f32, -1.0] {
        let flap = grid(8, 6, |v, a| {
            // a = 0 at the sewn edge, 1 at the free edge, which swings aside as the door opens.
            let free = 0.9 * open;
            let ang = side * half * (1.0 + (free - 1.0) * a);
            let p = bell_point(&profile, v * door_top, ang);
            let out = vec3::normalize_or([p[0], 0.0, p[2]], vec3::X);
            let lift = 0.08 + 0.3 * open * (PI * a).sin() * (1.0 - v);
            vec3::add(p, vec3::scale(out, lift))
        });
        let mut m = thick_sheet(&flap, &opts, CANVAS);
        if side < 0.0 {
            m.flip();
        }
        flaps.append(&m);
    }
    parts.add_look("flaps", flaps, look);

    let floor = frustum(r - 0.05, r - 0.05, 0.06, 40, Shading::SMOOTH);
    parts.add_look("groundsheet", floor, Look::of(Leather).tinted(t.color.scaled(0.55)));

    if t.patches > 0 {
        let mut rng = Mulberry32::new(s.seed.wrapping_mul(0x9e37_79b9).wrapping_add(0x7e47));
        let (mut patches, mut stitches) = (MeshBuilder::new(), MeshBuilder::new());
        for _ in 0..t.patches {
            let (u0, v0) = (rng.range_f32(0.12, 0.78), rng.range_f32(0.2, 0.6));
            let (du, dv) = (rng.range_f32(0.04, 0.07), rng.range_f32(0.09, 0.14));
            let (cloth, thread) = patch(|v, u| bell_point(&profile, v, u * TAU), v0, u0, dv, du, |p| [0.0, p[1], 0.0]);
            patches.append(&cloth);
            stitches.append(&thread);
        }
        parts.add_look("patches", patches, Look::of(t.canvas.material()).tinted(patch_color(t.color)));
        parts.add("stitches", stitches, Cord);
    }

    // One centre pole, through the apex.
    let mut poles = rod([0.0, 0.0, 0.0], [0.0, t.height + 0.7, 0.0], 0.17);
    poles.append(&knob([0.0, t.height + 0.8, 0.0], 0.24));
    parts.add("poles", poles, Wood);

    // Guys fan over the arc beyond the door, and a ring of pegs holds the skirt.
    let (mut guys, mut pegs) = (MeshBuilder::new(), MeshBuilder::new());
    let stakes = stakes(t);
    for st in &stakes {
        guys.append(&guy(st.from, st.head));
        pegs.append(&peg(st.base, st.away()).0);
    }
    let ring = (t.pegs * 4) as usize;
    for k in 0..ring {
        let a = TAU * (k as f32 + 0.5) / ring as f32;
        let off_door = a > half + 0.05 && a < TAU - half - 0.05;
        let guy_angle = |st: &Stake| {
            let b = (-st.from[2]).atan2(st.from[0]).rem_euclid(TAU);
            (b - a).abs()
        };
        if !off_door || stakes.iter().any(|st| guy_angle(st) < 0.08) {
            continue;
        }
        let dir = [a.cos(), 0.0, -a.sin()];
        let (p, head) = peg([dir[0] * (r + 0.3), 0.0, dir[2] * (r + 0.3)], dir);
        pegs.append(&p);
        guys.append(&tape([dir[0] * r, 0.12, dir[2] * r], head));
    }
    if !guys.is_empty() {
        parts.add("guys", guys, Cord);
    }
    if !pegs.is_empty() {
        parts.add("pegs", pegs, Wood);
    }
}

/// The canvas area a tent rolls up, in square units.
fn canvas_area(t: &Canopy) -> f32 {
    match t.style {
        TentStyle::Ridge => {
            let slope = (t.height * t.height + t.width * t.width * 0.25).sqrt();
            2.0 * slope * t.length + t.width * t.height
        }
        TentStyle::Bell => {
            let r = t.width * 0.5;
            PI * r * (r * r + t.height * t.height).sqrt() * 0.9
        }
    }
}

/// A packed tent's roll: radius and length of the canvas.
pub fn packed_size(t: &Canopy) -> (f32, f32) {
    let radius = 0.05 * canvas_area(t).sqrt();
    let length = match t.style {
        TentStyle::Ridge => t.length * 0.5,
        TentStyle::Bell => (t.width * 0.38 + 3.0).clamp(6.5, 12.0),
    };
    (radius, length)
}

/// The canvas rolled round its poles and strapped, the pegs tied to it and the guy line coiled
/// on top.
fn packed_tent(s: &TentState, parts: &mut Parts) {
    let t = &s.tent;
    let (r, len) = packed_size(t);
    let centre = [0.0, r, 0.0];
    let fabric = Fabric { color: t.color, pattern: t.pattern, color2: t.color2, scale: 2.0 };
    let (roll, edge) = rolled_sheet(centre, r, len, 0.04, 4.5);
    parts.add_look("roll", roll, Look::cloth(&fabric).made_of(t.canvas.material()));
    parts.add_look("roll_edge", edge, Look::of(t.canvas.material()).tinted(t.color.scaled(0.75)).without_outline());

    // The poles run through the roll and stand out of both ends.
    let pole = len * 0.5 + 0.75;
    let mut poles = MeshBuilder::new();
    let spread = r * 0.34;
    let count = if t.style == TentStyle::Ridge { 3 } else { 2 };
    for k in 0..count {
        let a = TAU * k as f32 / count as f32 + 0.5;
        let (y, z) = (r + spread * a.cos(), spread * a.sin());
        poles.append(&rod([-pole, y, z], [pole, y, z], 0.13));
    }
    parts.add("poles", poles, Wood);

    let xs = [-0.3 * len, 0.3 * len];
    let (straps, buckles) = roll_straps(centre, r * 1.03 + 0.04, &xs, 0.55, 0.07, 0.04);
    parts.add_look("straps", straps, Look::of(Leather).tinted(Rgb::hex(0x4a3226)));
    parts.add("buckles", buckles, Brass);

    // A bundle of pegs tied under the near strap.
    let mut pegs = MeshBuilder::new();
    for k in 0..5 {
        let mut p = frustum(0.1, 0.07, 1.6, 8, Shading::SMOOTH);
        p.rotate_z(-PI * 0.5);
        p.translate([xs[0] - 0.7, r * 2.0 + 0.1, (k as f32 - 2.0) * 0.17]);
        pegs.append(&p);
    }
    parts.add("pegs", pegs, Wood);

    // The guy line: two turns of cord coiled on the roll between the straps.
    let mut coil = MeshBuilder::new();
    for k in 0..2 {
        let mut c = torus(0.5, 0.05, 28, 6, Shading::SMOOTH);
        c.rotate_x(PI * 0.5).translate([0.3 * len * 0.2 + (k as f32 - 0.5) * 0.14, r * 2.0 + 0.12, 0.0]);
        coil.append(&c);
    }
    parts.add("guy_coil", coil, Cord);
}
