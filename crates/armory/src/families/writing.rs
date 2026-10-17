//! Writing: the written goods the abyss feeds a delver. Scrolls, paper notes and books.
//!
//! Nothing in the prototypes draws them, so the whole family and its sliders are new. One state
//! holds all three kinds: a [`Piece`] section every kind shares (its kind, size, paper and
//! ink) and one section of options per kind ([`ScrollOpts`] now; notes and books add theirs
//! as they are built). Only the scroll has its real shape so far; a note and a book are plain
//! stand-ins until their builders land.
//!
//! The scroll:
//! - The sheet is one thick sheet laid along a single path: a spiral wound on a roll, a flat
//!   span lying on the ground (rippled, its free end curling up) and, with two rollers, a
//!   second spiral wound the other way. Rolled shut it is just the spirals; partly unrolled
//!   the span shows rows of ink, drawn as thin strips lying on the sheet.
//! - Rollers are turned dowels with knobs of wood, bone or brass, none, one or two. A rolled
//!   shut scroll can be tied with ribbon or cord and sealed with wax pressed with a sigil.
//! - [`Condition`] drives the wear: `damage` ragged and tattered edges (bites out of the
//!   sides, a torn end), `wear` and `rot` foxing and a browner sheet, and wear with damage
//!   scorched edges.
//!
//! Papyrus shows its fibres. Parchment and vellum are skins; vellum is paler.
//!
//! Item space: the item rests on the ground (y = 0), centred over the origin, its long side
//! along X. A scroll is 2 to 4 units (20 to 40 cm) long.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use crate::build::chain::{bead, twisted_cord};
use crate::build::fitting::rounded_rect;
use crate::build::parts::{Look, Parts};
use crate::build::strap::strap;
use crate::cond::{Condition, ConditionLimits};
use crate::families::Generator;
use crate::geom::polygon::P2;
use crate::geom::{
    GridOptions, ItemMesh, LatheOptions, MeshBuilder, Shading, SlabOptions, V3, bevelled_slab, cylinder, ellipsoid, extrude, lathe,
    lathe_warped, spline_profile, stitch_grid, thick_sheet, value_noise3,
};
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::slider::{Sliders, sliders};

use MaterialKind::{Bone, Brass, Cloth, Cord, Leather, Paint, Wood};

/// The writing generator.
pub struct Writing;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritingKind {
    /// A sheet wound on rollers or on itself.
    Scroll,
    /// A paper note, from a scrap to a letter.
    Note,
    /// A bound book.
    Book,
}

impl WritingKind {
    pub const ALL: [WritingKind; 3] = [WritingKind::Scroll, WritingKind::Note, WritingKind::Book];
}

/// What the sheet is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Paper {
    Parchment,
    /// Paler and smoother than parchment, from calfskin.
    Vellum,
    /// Pressed reed, its fibres showing.
    Papyrus,
}

impl Paper {
    pub const ALL: [Paper; 3] = [Paper::Parchment, Paper::Vellum, Paper::Papyrus];

    /// The colours "Forge new" picks from.
    pub fn colors(self) -> [Rgb; 3] {
        match self {
            Paper::Parchment => [Rgb::hex(0xd6c496), Rgb::hex(0xcdb98a), Rgb::hex(0xdccba0)],
            Paper::Vellum => [Rgb::hex(0xece2c6), Rgb::hex(0xe4d8b8), Rgb::hex(0xf0e8d2)],
            Paper::Papyrus => [Rgb::hex(0xc8ad72), Rgb::hex(0xbf9f64), Rgb::hex(0xd0b57c)],
        }
    }
}

/// The turned knobs on a roller's ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Knob {
    Wood,
    Bone,
    Brass,
}

impl Knob {
    pub const ALL: [Knob; 3] = [Knob::Wood, Knob::Bone, Knob::Brass];

    pub fn material(self) -> MaterialKind {
        match self {
            Knob::Wood => Wood,
            Knob::Bone => Bone,
            Knob::Brass => Brass,
        }
    }
}

/// What ties a rolled-shut scroll.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tie {
    None,
    Ribbon,
    Cord,
}

impl Tie {
    pub const ALL: [Tie; 3] = [Tie::None, Tie::Ribbon, Tie::Cord];
}

/// What every kind of writing shares.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Piece {
    pub kind: WritingKind,
    /// Overall scale.
    pub size: f32,
    pub paper: Paper,
    pub color: Rgb,
    pub ink: Rgb,
    /// Rows of ink on a sheet that shows its face.
    pub lines: u32,
}

/// A scroll's own options.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScrollOpts {
    /// Rollers: none, one, or two (the sheet wound on both).
    pub rollers: u32,
    pub knob: Knob,
    /// Scales the turned knobs.
    pub knob_size: f32,
    /// Turns of sheet wound on a roll (halved on two rollers).
    pub turns: f32,
    /// How thick the sheet is.
    pub thick: f32,
    /// How much is unrolled; under 0.12 the scroll is rolled shut.
    pub unroll: f32,
    /// How far the free end curls up.
    pub curl: f32,
    pub tie: Tie,
    pub tie_color: Rgb,
    /// A wax seal, on a rolled-shut scroll.
    pub seal: bool,
    pub seal_color: Rgb,
    /// Points on the star pressed into the wax.
    pub sigil: u32,
}

sliders! {
    PieceLimits for Piece {
        size: 0.9, 1.25, 0.01;
        lines: 0.0, 12.0, 1.0;
    }
}

sliders! {
    ScrollLimits for ScrollOpts {
        rollers: 0.0, 2.0, 1.0;
        knob_size: 0.7, 1.3, 0.01;
        turns: 3.0, 7.0, 0.1;
        thick: 0.015, 0.035, 0.001;
        unroll: 0.0, 0.8, 0.01;
        curl: 0.0, 1.0, 0.01;
        sigil: 3.0, 9.0, 1.0;
    }
}

/// Inks: iron gall, soot and a brown fade.
pub const INKS: [Rgb; 3] = [Rgb::hex(0x2a2018), Rgb::hex(0x1c1a1a), Rgb::hex(0x4a3320)];
/// Ribbon dyes and cord colours.
pub const TIES: [Rgb; 5] = [Rgb::hex(0x8e2a2a), Rgb::hex(0x2c4e8c), Rgb::hex(0x3d6b3a), Rgb::hex(0x6b4a8a), Rgb::hex(0xb89a62)];
/// Wax: red, black, green, blue and beeswax.
pub const SEALS: [Rgb; 5] = [Rgb::hex(0x8e1f1a), Rgb::hex(0x25221f), Rgb::hex(0x2f5a35), Rgb::hex(0x2a3f73), Rgb::hex(0xc9a24a)];

impl Default for Piece {
    fn default() -> Self {
        Piece { kind: WritingKind::Scroll, size: 1.0, paper: Paper::Parchment, color: Paper::Parchment.colors()[0], ink: INKS[0], lines: 6 }
    }
}

impl Default for ScrollOpts {
    fn default() -> Self {
        ScrollOpts {
            rollers: 1,
            knob: Knob::Wood,
            knob_size: 1.0,
            turns: 5.0,
            thick: 0.025,
            unroll: 0.0,
            curl: 0.5,
            tie: Tie::None,
            tie_color: TIES[0],
            seal: false,
            seal_color: SEALS[0],
            sigil: 5,
        }
    }
}

impl ScrollOpts {
    /// Rolled shut, not partly unrolled.
    pub fn is_shut(&self) -> bool {
        self.unroll < SHUT_BELOW
    }
}

const SHUT_BELOW: f32 = 0.12;

/// Every parameter of one piece of writing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WritingState {
    pub seed: u32,
    pub piece: Piece,
    pub scroll: ScrollOpts,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every writing section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WritingLimits {
    pub piece: PieceLimits,
    pub scroll: ScrollLimits,
    pub cond: ConditionLimits,
}

/// The writing sliders' ranges (new: the prototypes have none).
pub const LIMITS: WritingLimits = WritingLimits { piece: PieceLimits::SLIDERS, scroll: ScrollLimits::SLIDERS, cond: ConditionLimits::SLIDERS };

impl WritingState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &WritingLimits) {
        self.piece.fit(&limits.piece);
        self.scroll.fit(&limits.scroll);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &WritingLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("piece", self.piece.out_of_range(&limits.piece)),
            tag("scroll", self.scroll.out_of_range(&limits.scroll)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

impl Generator for Writing {
    type State = WritingState;

    fn roll(seed: u32) -> WritingState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        use WritingKind as K;
        let kind = *rng.weighted(&[(K::Scroll, 4), (K::Note, 3), (K::Book, 2)]);
        compose(&mut rng, seed, kind)
    }

    fn noun(s: &WritingState) -> String {
        let (p, sc) = (&s.piece, &s.scroll);
        match p.kind {
            WritingKind::Scroll if sc.seal && sc.is_shut() => "Sealed Scroll",
            WritingKind::Scroll if p.paper == Paper::Papyrus => "Papyrus Scroll",
            WritingKind::Scroll if p.paper == Paper::Vellum => "Vellum Scroll",
            WritingKind::Scroll => "Scroll",
            WritingKind::Note => "Note",
            WritingKind::Book => "Book",
        }
        .to_string()
    }

    fn build(s: &WritingState) -> ItemMesh {
        build_writing(s)
    }
}

impl Writing {
    /// "Forge new" forced to one kind, for turntables and tests. Draws the same stream as
    /// [`Generator::roll`], so `roll(s)` equals `roll_as(s, kind)` when `kind` is what `s`
    /// rolls.
    pub fn roll_as(seed: u32, kind: WritingKind) -> WritingState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, kind)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, kind: WritingKind) -> WritingState {
    let mut s = WritingState { seed, ..WritingState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    let p = &mut s.piece;
    p.kind = kind;
    p.size = rng.range_f32(0.95, 1.15);
    p.paper = *rng.weighted(&[(Paper::Parchment, 4), (Paper::Vellum, 2), (Paper::Papyrus, 2)]);
    p.color = *rng.pick(&p.paper.colors());
    p.ink = *rng.pick(&INKS);
    p.lines = rng.int(4, 9);
    // The scroll's draws are made for every kind, so a kind's options never shift another's.
    let sc = &mut s.scroll;
    sc.rollers = *rng.weighted(&[(0, 2), (1, 3), (2, 3)]);
    sc.knob = *rng.weighted(&[(Knob::Wood, 3), (Knob::Bone, 2), (Knob::Brass, 2)]);
    sc.knob_size = rng.range_f32(0.8, 1.2);
    sc.turns = rng.range_f32(3.5, 6.5);
    sc.thick = rng.range_f32(0.02, 0.03);
    sc.unroll = if rng.chance(0.55) { rng.range_f32(0.2, 0.8) } else { 0.0 };
    sc.curl = rng.range_f32(0.2, 1.0);
    let tie = *rng.weighted(&[(Tie::None, 2), (Tie::Ribbon, 3), (Tie::Cord, 3)]);
    let seal = rng.chance(0.4);
    sc.tie_color = *rng.pick(&TIES);
    sc.seal_color = *rng.weighted(&[(SEALS[0], 4), (SEALS[1], 2), (SEALS[2], 1), (SEALS[3], 1), (SEALS[4], 2)]);
    sc.sigil = rng.int(4, 8);
    if sc.is_shut() {
        sc.tie = tie;
        sc.seal = seal;
    }
    s.fit(&LIMITS);
    s
}

// ───────────────────────── Geometry ─────────────────────────

fn build_writing(s: &WritingState) -> ItemMesh {
    let mut parts = Parts::new();
    match s.piece.kind {
        WritingKind::Scroll => scroll(s, &mut parts),
        WritingKind::Note => note(s, &mut parts),
        WritingKind::Book => book(s, &mut parts),
    }
    parts.rest_on_ground();
    parts.finish()
}

/// A flat stand-in for a note until its builder lands: one sheet of paper.
fn note(s: &WritingState, parts: &mut Parts) {
    let p = &s.piece;
    let (w, d, t) = (1.2 * p.size, 1.6 * p.size, 0.012 * p.size);
    let mut sheet = bevelled_slab(&rounded_rect(0.0, 0.0, w, d, 0.02), &[], &SlabOptions::rounded(t, t * 0.3));
    sheet.rotate_x(-FRAC_PI_2).translate([0.0, t * 0.5, 0.0]);
    parts.add_look("sheet", sheet, Look::of(Cloth).tinted(p.color));
}

/// A slab for a book until its builder lands: boards with a page block between them.
fn book(s: &WritingState, parts: &mut Parts) {
    let p = &s.piece;
    let (w, d, h) = (2.2 * p.size, 1.6 * p.size, 0.4 * p.size);
    let slab = |w: f32, d: f32, h: f32, y: f32| {
        let mut m = bevelled_slab(&rounded_rect(0.0, 0.0, w, d, 0.04), &[], &SlabOptions::rounded(h, h * 0.15));
        m.rotate_x(-FRAC_PI_2).translate([0.0, y + h * 0.5, 0.0]);
        m
    };
    let boards = MeshBuilder::merged([&slab(w, d, h * 0.14, 0.0), &slab(w, d, h * 0.14, h * 0.86)]);
    parts.add("cover", boards, Leather);
    parts.add_look("pages", slab(w * 0.95, d * 0.94, h * 0.72, h * 0.14), Look::of(Cloth).tinted(p.color));
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smooth(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    Rgb(lerp(a.0, b.0, t), lerp(a.1, b.1, t), lerp(a.2, b.2, t))
}

/// How far a scroll's condition has gone, 0..1 each: ragged edges, foxing, scorch.
struct Decay {
    ragged: f32,
    fox: f32,
    scorch: f32,
}

impl Decay {
    fn of(c: &Condition) -> Decay {
        Decay {
            ragged: c.damage.clamp(0.0, 1.0),
            fox: (c.wear * c.rot).clamp(0.0, 1.0),
            scorch: (c.damage * c.wear * 1.5 - 0.2).clamp(0.0, 1.0),
        }
    }
}

/// The scroll's measurements, shared by every piece of it. The roll's axis runs along X; the
/// sheet is laid out along Z, which `scroll` turns to X once it is built if that is longer.
struct Layout {
    /// Sheet width, along the axis.
    w: f32,
    /// Sheet thickness and the pitch of the spiral (a hair more).
    t: f32,
    pitch: f32,
    /// Dowel radius, and where the sheet's wound part starts.
    rd: f32,
    r0: f32,
    dowels: u32,
    /// Sheet wound on two rolls.
    two: bool,
    unrolled: bool,
    /// Turns on each roll and the roll's radius (to the sheet's inner face at its edge).
    n: f32,
    r: f32,
    /// Height of the roll's axis.
    cy: f32,
    /// Z of the second roll's axis (the first is at z = 0), or the end of the flat sheet.
    d: f32,
    /// The length of sheet showing between rolls, or beyond the one roll.
    span: f32,
    curl: f32,
    size: f32,
}

impl Layout {
    fn of(s: &WritingState) -> Layout {
        let (p, sc) = (&s.piece, &s.scroll);
        let size = p.size;
        let dowels = sc.rollers.min(2);
        let two = dowels == 2;
        let t = sc.thick;
        let pitch = t * 1.25;
        let rd = 0.05 * size;
        let r0 = if dowels > 0 { rd + 0.004 } else { 0.025 };
        let unrolled = !sc.is_shut();
        let n = (sc.turns * if two { 0.6 } else { 1.0 } * (1.0 - 0.35 * sc.unroll)).max(1.2);
        let r = r0 + pitch * n;
        let span = if unrolled { sc.unroll * 2.6 * size } else { 0.0 };
        let d = if two {
            if unrolled { span + 2.0 * r + 0.02 } else { 2.0 * r + 0.03 }
        } else if unrolled {
            r + span
        } else {
            0.0
        };
        Layout { w: 2.3 * size, t, pitch, rd, r0, dowels, two, unrolled, n, r, cy: r + t, d, span, curl: sc.curl, size }
    }

    /// Z of the first and last roll axes.
    fn roll_z(&self) -> (f32, f32) {
        (0.0, if self.two { self.d } else { 0.0 })
    }

    /// The flat sheet's range of z where its face is on view.
    fn visible(&self, ragged: f32) -> Option<(f32, f32)> {
        if !self.unrolled {
            return None;
        }
        let lo = self.r + 0.12;
        let hi = if self.two { self.d - self.r - 0.12 } else { self.d - 0.12 - ragged * 0.4 };
        (hi - lo > 0.12).then_some((lo, hi))
    }
}

/// The flat sheet's top: its height at (x, z), the ripples that lift it, and the curl at a
/// free end. The ink and stains lie on this.
struct Surface<'a> {
    l: &'a Layout,
    seed: u32,
}

impl Surface<'_> {
    fn lift(&self, z: f32) -> f32 {
        let l = self.l;
        if l.two || !l.unrolled {
            return 0.0;
        }
        let z0 = l.d - (0.7 * l.span).min(0.9);
        let u = ((z - z0) / (l.d - z0).max(1e-3)).clamp(0.0, 1.0);
        l.curl * 0.3 * u * u
    }

    fn weight(&self, z: f32) -> f32 {
        let l = self.l;
        smooth(0.0, 0.35, z) * if l.two { smooth(0.0, 0.35, l.d - z) } else { 1.0 }
    }

    fn ripple(&self, x: f32, z: f32) -> f32 {
        0.012 * self.l.size * self.weight(z) * value_noise3([x * 0.8, z * 0.8, 0.3], self.seed ^ 0x3d)
    }

    fn height(&self, x: f32, z: f32) -> f32 {
        self.l.t + self.lift(z) + self.ripple(x, z)
    }
}

/// The sheet's centre line in the (z, y) plane: a spiral wound on the first roll from its
/// core out to the edge, then (if unrolled) the flat sheet, then (on two rollers) the second
/// spiral wound back in. The sheet's face is on the left of the way it runs, so the face is
/// up on the flat. Returns the points and the point indices where the flat starts and ends.
fn sheet_path(l: &Layout) -> (Vec<[f32; 2]>, usize, usize) {
    let steps = (l.n * 40.0).ceil() as usize;
    let end = -FRAC_PI_2;
    let start = end - TAU * l.n;
    let radius = |phi: f32| l.r0 + l.pitch * (phi - start) / TAU;
    let at = |phi: f32| (radius(phi), phi.sin_cos());
    let mut path: Vec<[f32; 2]> = (0..=steps)
        .map(|i| {
            let (r, (s, c)) = at(start + (end - start) * i as f32 / steps as f32);
            [r * c, l.cy + r * s]
        })
        .collect();
    let flat_start = path.len() - 1;
    let surface = Surface { l, seed: 0 };
    if l.d > 0.0 {
        let n = (l.d / 0.07).ceil() as usize;
        for i in 1..=n {
            let z = l.d * i as f32 / n as f32;
            path.push([z, l.t + surface.lift(z)]);
        }
    }
    let flat_end = path.len() - 1;
    if l.two {
        for i in 1..=steps {
            let (r, (s, c)) = at(end - (end - start) * i as f32 / steps as f32);
            path.push([l.d - r * c, l.cy + r * s]);
        }
    }
    (path, flat_start, flat_end)
}

/// The scroll turned out in the plane: sheet, ink and stains, rollers, tie and seal.
fn scroll(s: &WritingState, parts: &mut Parts) {
    let (p, sc) = (&s.piece, &s.scroll);
    let l = Layout::of(s);
    let decay = Decay::of(&s.cond);
    let surface = Surface { l: &l, seed: s.seed };

    // Fox and age the paper's colour, darker where it has been lived in.
    let tint = mix(p.color, Rgb::hex(0x7a5a30), 0.4 * decay.fox);
    let (path, flat_start, flat_end) = sheet_path(&l);
    let cum: Vec<f32> = std::iter::once(0.0)
        .chain(path.windows(2).scan(0.0, |acc, w| {
            *acc += ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
            Some(*acc)
        }))
        .collect();
    let sheet_mesh = sheet(&l, &surface, &path, &cum, (flat_start, flat_end), &decay, s.seed);
    parts.add_look("sheet", sheet_mesh, Look::of(Cloth).tinted(tint));

    let s_of_z = |z: f32| {
        let flat = &path[flat_start..=flat_end];
        let i = flat.partition_point(|q| q[0] < z).clamp(1, flat.len().saturating_sub(1).max(1));
        cum[flat_start + i.min(flat.len() - 1)]
    };
    let edge = |z: f32, side: f32| edge_inset(s_of_z(z), side, decay.ragged, s.seed);

    if let Some((z0, z1)) = l.visible(decay.ragged) {
        let mut rng = Mulberry32::new(s.seed.wrapping_mul(0x9e37_79b9).wrapping_add(41));
        parts.add_look("ink", ink(p, &l, &surface, &edge, (z0, z1), decay.ragged, &mut rng), Look::of(Paint).tinted(p.ink).without_outline());
        if p.paper == Paper::Papyrus {
            let fibre = Look::of(Cloth).tinted(tint.scaled(0.72)).without_outline();
            parts.add_look("fibre", fibres(&l, &surface, &edge, (z0, z1), &mut rng), fibre);
        }
        let fox = (decay.fox * 14.0).round() as u32;
        if fox > 0 {
            parts.add_look("foxing", foxing(&l, &surface, (z0, z1), fox, &mut rng), Look::of(Cloth).tinted(Rgb::hex(0x7a4a22)).without_outline());
        }
        if decay.scorch > 0.0 {
            parts.add_look("scorch", scorch(&l, &surface, &edge, (z0, z1), decay.scorch, s.seed), Look::of(Cloth).tinted(Rgb::hex(0x2a1d12)).without_outline());
        }
    }

    // Rollers and their knobs.
    if l.dowels > 0 {
        let (rollers, knobs) = rollers(&l, sc.knob_size);
        parts.add("rollers", rollers, Wood);
        parts.add("knobs", knobs, sc.knob.material());
    }

    // Tie and seal, on a rolled-shut scroll.
    if !l.unrolled {
        let top = l.cy + l.r + l.t;
        let mid_z = l.d * 0.5;
        match sc.tie {
            Tie::Ribbon => parts.add_look("ribbon", ribbon(&l, top, mid_z), Look::of(Cloth).tinted(sc.tie_color)),
            Tie::Cord => parts.add_look("cord", cord_tie(&l, top, mid_z), Look::of(Cord).tinted(sc.tie_color)),
            Tie::None => &mut *parts,
        };
        if sc.seal {
            let base = if sc.tie == Tie::None && l.two { valley(&l) } else { top };
            parts.add_look("seal", seal(sc, &l, [0.0, base - 0.02, mid_z], s.seed), Look::of(Bone).tinted(sc.seal_color));
        }
    }

    // Centre over the origin and put the longer side along X.
    let b = |parts: &mut Parts| {
        parts.meshes_mut().filter_map(|m| m.bounds()).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), (a, b)| {
            ([lo[0].min(a[0]), lo[1].min(a[1]), lo[2].min(a[2])], [hi[0].max(b[0]), hi[1].max(b[1]), hi[2].max(b[2])])
        })
    };
    let (lo, hi) = b(parts);
    let turn = hi[2] - lo[2] > hi[0] - lo[0];
    let centre = [(lo[0] + hi[0]) * 0.5, 0.0, (lo[2] + hi[2]) * 0.5];
    for m in parts.meshes_mut() {
        m.translate([-centre[0], 0.0, -centre[2]]);
        if turn {
            m.rotate_y(FRAC_PI_2);
        }
    }
}

/// How far the sheet's side edge is bitten in at path length `s`: nothing when whole, then an
/// uneven margin with the odd deep bite.
fn edge_inset(s: f32, side: f32, ragged: f32, seed: u32) -> f32 {
    if ragged <= 0.0 {
        return 0.0;
    }
    let n1 = value_noise3([s * 2.1, side * 3.7, 0.5], seed);
    let n2 = value_noise3([s * 5.3, side * 3.7, 9.1], seed ^ 0x55);
    ragged * (0.16 * n1 + 0.4 * ((n2 - 0.7) / 0.3).max(0.0))
}

/// The thick sheet along its path: a grid of columns across the width and rows along the
/// path, each column free to end a little short (a torn end) and each row free to pull its
/// edges in (ragged sides).
fn sheet(l: &Layout, surface: &Surface, path: &[[f32; 2]], cum: &[f32], flat: (usize, usize), decay: &Decay, seed: u32) -> MeshBuilder {
    const COLS: usize = 15;
    let total = *cum.last().unwrap_or(&0.0);
    let rows = path.len().clamp(60, 700);
    let at = |s: f32| -> [f32; 2] {
        let s = s.clamp(0.0, total);
        let i = cum.partition_point(|&c| c < s).clamp(1, path.len() - 1);
        let (a, b) = (path[i - 1], path[i]);
        let u = ((s - cum[i - 1]) / (cum[i] - cum[i - 1]).max(1e-9)).clamp(0.0, 1.0);
        [lerp(a[0], b[0], u), lerp(a[1], b[1], u)]
    };
    let cap = 0.15 + 0.5 * (cum[flat.1] - cum[flat.0]);
    let (sf0, sf1) = (cum[flat.0], cum[flat.1]);
    let ends: Vec<f32> = (0..COLS)
        .map(|c| {
            if l.two {
                return total;
            }
            let cf = c as f32 / (COLS - 1) as f32;
            let tear = decay.ragged * (0.2 * value_noise3([c as f32 * 1.3, 0.3, 0.7], seed) + 0.5 * cf.powi(4) * value_noise3([c as f32 * 0.7, 4.1, 2.3], seed ^ 9));
            total - tear.min(cap)
        })
        .collect();
    let grid: Vec<Vec<V3>> = (0..rows)
        .map(|k| {
            let f = k as f32 / (rows - 1) as f32;
            (0..COLS)
                .map(|c| {
                    let cf = c as f32 / (COLS - 1) as f32;
                    let s = ends[c] * f;
                    let [z, y] = at(s);
                    let lo = -l.w * 0.5 + edge_inset(s, 0.0, decay.ragged, seed);
                    let hi = l.w * 0.5 - edge_inset(s, 1.0, decay.ragged, seed);
                    let x = hi - (hi - lo) * cf;
                    let ripple = if s >= sf0 && s <= sf1 { surface.ripple(x, z) } else { 0.0 };
                    [x, y + ripple, z]
                })
                .collect()
        })
        .collect();
    thick_sheet(&grid, &GridOptions { shading: Shading::SMOOTH, ..Default::default() }, l.t)
}

/// A strip between two lines of points: faces the way `a` runs crossed with `a` to `b`.
fn strip(a: Vec<V3>, b: Vec<V3>) -> MeshBuilder {
    stitch_grid(&[a, b], &GridOptions { shading: Shading::SMOOTH, ..Default::default() })
}

/// Rows of ink: words as thin strips lying on the sheet, a margin in from each ragged edge.
fn ink(p: &Piece, l: &Layout, surface: &Surface, edge: &dyn Fn(f32, f32) -> f32, range: (f32, f32), ragged: f32, rng: &mut Mulberry32) -> MeshBuilder {
    let (z0, z1) = range;
    let pitch = ((z1 - z0) / p.lines.max(1) as f32).clamp(0.1 * l.size, 0.15 * l.size);
    let rows = (((z1 - z0) / pitch).floor() as u32 + 1).min(p.lines);
    let margin = 0.16 + 0.3 * ragged;
    let mut out = MeshBuilder::new();
    for row in 0..rows {
        let z = z0 + pitch * row as f32 + rng.range_f32(-0.004, 0.004);
        let left = -l.w * 0.5 + margin + edge(z, 0.0);
        let right = l.w * 0.5 - margin - edge(z, 1.0) - if rng.chance(0.3) { rng.range_f32(0.1, 0.8) } else { 0.0 };
        let width = rng.range_f32(0.016, 0.022) * l.size;
        let mut x = left;
        while x < right - 0.08 {
            let len = rng.range_f32(0.1, 0.34).min(right - x);
            let phase = rng.range_f32(0.0, TAU);
            let segs = (len / 0.07).ceil().max(2.0) as usize;
            let line = |dz: f32| -> Vec<V3> {
                (0..=segs)
                    .map(|i| {
                        let px = x + len * i as f32 / segs as f32;
                        let pz = z + dz + 0.004 * (px * 9.0 + phase).sin();
                        [px, surface.height(px, pz) + 0.003, pz]
                    })
                    .collect()
            };
            out.append(&strip(line(0.0), line(-width)));
            x += len + rng.range_f32(0.04, 0.08);
        }
    }
    out
}

/// A papyrus sheet's fibres: fine lines running the length of the sheet.
fn fibres(l: &Layout, surface: &Surface, edge: &dyn Fn(f32, f32) -> f32, range: (f32, f32), rng: &mut Mulberry32) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    let (z0, z1) = (range.0 - 0.05, range.1 + 0.05);
    for _ in 0..9 {
        let x = rng.range_f32(-l.w * 0.5 + 0.12, l.w * 0.5 - 0.12);
        let width = rng.range_f32(0.008, 0.016);
        let segs = ((z1 - z0) / 0.08).ceil() as usize;
        let line = |dx: f32| -> Vec<V3> {
            (0..=segs)
                .map(|i| {
                    let z = z0 + (z1 - z0) * i as f32 / segs as f32;
                    let px = (x + dx + 0.015 * (z * 3.0 + x * 5.0).sin()).clamp(-l.w * 0.5 + edge(z, 0.0) + 0.01, l.w * 0.5 - edge(z, 1.0) - 0.01);
                    [px, surface.height(px, z) + 0.0015, z]
                })
                .collect()
        };
        out.append(&strip(line(0.0), line(width)));
    }
    out
}

/// Foxing: brown flecks that bloom on old paper.
fn foxing(l: &Layout, surface: &Surface, range: (f32, f32), count: u32, rng: &mut Mulberry32) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    for _ in 0..count {
        let x = rng.range_f32(-l.w * 0.5 + 0.2, l.w * 0.5 - 0.2);
        let z = rng.range_f32(range.0, range.1);
        let (rx, rz) = (rng.range_f32(0.025, 0.08), rng.range_f32(0.025, 0.08));
        let mut fleck = ellipsoid([rx, 0.004, rz]);
        fleck.translate([x, surface.height(x, z) + 0.001, z]);
        out.append(&fleck);
    }
    out
}

/// Scorching: a charred band along each side that deepens toward the edge.
fn scorch(l: &Layout, surface: &Surface, edge: &dyn Fn(f32, f32) -> f32, range: (f32, f32), amount: f32, seed: u32) -> MeshBuilder {
    let (z0, z1) = (range.0 - 0.1, range.1 + 0.1);
    let segs = (((z1 - z0) / 0.07).ceil() as usize).max(2);
    let zs: Vec<f32> = (0..=segs).map(|i| z0 + (z1 - z0) * i as f32 / segs as f32).collect();
    let mut out = MeshBuilder::new();
    for side in [0.0f32, 1.0] {
        let dir = if side == 0.0 { 1.0 } else { -1.0 };
        let edge_x = |z: f32| if side == 0.0 { -l.w * 0.5 + edge(z, 0.0) } else { l.w * 0.5 - edge(z, 1.0) };
        let width = |z: f32| amount * (0.04 + 0.12 * value_noise3([z * 3.1, side * 5.0, 1.7], seed ^ 0xb7));
        let order: Vec<f32> = if side == 0.0 { zs.clone() } else { zs.iter().rev().copied().collect() };
        let outer: Vec<V3> = order.iter().map(|&z| [edge_x(z), surface.height(edge_x(z), z) + 0.0025, z]).collect();
        let inner: Vec<V3> = order
            .iter()
            .map(|&z| {
                let x = edge_x(z) + dir * width(z);
                [x, surface.height(x, z) + 0.0025, z]
            })
            .collect();
        out.append(&strip(outer, inner));
    }
    out
}

/// Turned dowels through each roll, and the knobs on their ends. Returns `(shafts, knobs)`.
fn rollers(l: &Layout, knob_size: f32) -> (MeshBuilder, MeshBuilder) {
    let rd = l.rd;
    let k = knob_size;
    let reach = l.w * 0.5 + 0.07;
    let control = [
        [0.0, 0.0], [rd * 1.7, 0.0], [rd * 1.7, 0.03 * k], [rd * 0.9, 0.06 * k], [rd * 0.9, 0.09 * k], [rd * 1.4, 0.12 * k], [rd * 2.1, 0.19 * k],
        [rd * 1.9, 0.25 * k], [rd * 1.1, 0.3 * k], [rd * 0.45, 0.325 * k], [0.0, 0.34 * k],
    ];
    let knob = lathe(&spline_profile(&control, 30), &LatheOptions::new(20, Shading::SMOOTH));
    let (mut shafts, mut knobs) = (MeshBuilder::new(), MeshBuilder::new());
    let (za, zb) = l.roll_z();
    for z in [za, zb].into_iter().take(l.dowels as usize) {
        let mut shaft = cylinder(rd, reach * 2.0, 16, Shading::SMOOTH);
        shaft.rotate_z(-FRAC_PI_2).translate([-reach, l.cy, z]);
        shafts.append(&shaft);
        for side in [1.0f32, -1.0] {
            let mut cap = knob.clone();
            cap.rotate_z(-FRAC_PI_2 * side).translate([side * reach, l.cy, z]);
            knobs.append(&cap);
        }
    }
    (shafts, knobs)
}

/// A closed loop round the roll or rolls at radius `radius`: round the circle, or a stadium
/// round two rolls. Returns the points and the way the surface faces at each.
fn loop_round(l: &Layout, radius: f32, x: f32) -> (Vec<V3>, Vec<V3>) {
    let at = |zc: f32, a: f32| -> (V3, V3) {
        let (s, c) = a.sin_cos();
        ([x, l.cy + radius * s, zc + radius * c], [0.0, s, c])
    };
    let mut out: Vec<(V3, V3)> = Vec::new();
    if l.d < 1e-3 {
        out.extend((0..40).map(|i| at(0.0, TAU * i as f32 / 40.0)));
    } else {
        // Over the top from one roll to the other, round the far roll, back underneath, and
        // round the near roll.
        let line = |from: V3, to: V3, face: V3| (1..6).map(move |i| (vec3_lerp(from, to, i as f32 / 6.0), face));
        out.extend((0..=20).map(|i| at(l.d, FRAC_PI_2 - PI * i as f32 / 20.0)));
        out.extend(line(at(l.d, -FRAC_PI_2).0, at(0.0, -FRAC_PI_2).0, [0.0, -1.0, 0.0]));
        out.extend((0..=20).map(|i| at(0.0, -FRAC_PI_2 - PI * i as f32 / 20.0)));
        out.extend(line(at(0.0, FRAC_PI_2).0, at(l.d, FRAC_PI_2).0, [0.0, 1.0, 0.0]));
    }
    out.into_iter().unzip()
}

fn vec3_lerp(a: V3, b: V3, t: f32) -> V3 {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

/// An end of tie hanging from the knot over the far roll and down to the ground: from angle
/// `from` to `to` round the roll, then down and out along the ground. Returns points and faces.
fn hang(l: &Layout, x: f32, radius: f32, from: f32, to: f32, run: f32) -> (Vec<V3>, Vec<V3>) {
    let zc = l.roll_z().1;
    let (mut path, mut faces) = (Vec::new(), Vec::new());
    for i in 0..=10 {
        let (s, c) = (from + (to - from) * i as f32 / 10.0).sin_cos();
        path.push([x, l.cy + radius * s, zc + radius * c]);
        faces.push([0.0, s, c]);
    }
    let (y0, z0) = (path[10][1], path[10][2]);
    for k in 1..=6 {
        let f = k as f32 / 6.0;
        path.push([x, lerp(y0, 0.01, f.powf(0.7)), z0 + run * f * f]);
        faces.push(vec3_lerp(faces[10], [0.0, 1.0, 0.0], f));
    }
    (path, faces)
}

/// A ribbon round the roll or rolls with a knot on top and two ends hanging down the front.
fn ribbon(l: &Layout, top: f32, mid_z: f32) -> MeshBuilder {
    let rt = l.r + l.t + 0.012;
    let width = 0.11 * l.size;
    let (path, faces) = loop_round(l, rt, 0.0);
    let mut out = strap(&path, &faces, width, 0.012, true);
    let mut knot = ellipsoid([width * 0.7, 0.03, width * 0.6]);
    knot.translate([0.0, top + 0.015, mid_z]);
    out.append(&knot);
    for (dx, from, to, run) in [(-0.045, 1.35, 0.1, 0.26), (0.045, 1.25, 0.25, 0.4)] {
        let (p, f) = hang(l, dx * l.size, rt + 0.01, from, to, run);
        out.append(&strap(&p, &f, width * 0.7, 0.01, false));
    }
    out
}

/// A cord wound twice round with a knot and two ends, each with a bead.
fn cord_tie(l: &Layout, top: f32, mid_z: f32) -> MeshBuilder {
    let rt = l.r + l.t + 0.02;
    let wire = 0.02 * l.size;
    let mut out = MeshBuilder::new();
    for x in [-0.05, 0.05] {
        let (path, _) = loop_round(l, rt, x * l.size);
        out.append(&twisted_cord(&path, true, wire, 3));
    }
    let mut knot = ellipsoid([0.07, 0.04, 0.05]);
    knot.translate([0.0, top + 0.03, mid_z]);
    out.append(&knot);
    for (dx, from, to, run) in [(-0.04, 1.35, 0.1, 0.26), (0.04, 1.25, 0.25, 0.4)] {
        let (p, _) = hang(l, dx * l.size, rt + 0.01, from, to, run);
        out.append(&twisted_cord(&p, false, wire, 3));
        if let Some(&end) = p.last() {
            let mut b = bead(wire * 1.9, 10);
            b.rotate_z(FRAC_PI_2).translate(end);
            out.append(&b);
        }
    }
    out
}

/// Where the valley between two touching rolls lies, above the axis.
fn valley(l: &Layout) -> f32 {
    let rt = l.r + l.t;
    let half = l.d * 0.5;
    l.cy + (rt * rt - half * half).max(0.0).sqrt()
}

/// A puck of wax with an uneven edge, a raised rim and a star pressed in the middle, set with
/// its base at `at`.
fn seal(sc: &ScrollOpts, l: &Layout, at: V3, seed: u32) -> MeshBuilder {
    let s = l.size;
    let profile: Vec<[f32; 2]> = [
        [0.0, 0.0], [0.19, 0.0], [0.21, 0.012], [0.185, 0.035], [0.178, 0.07], [0.16, 0.082], [0.12, 0.076], [0.1, 0.06], [0.0, 0.058],
    ]
    .iter()
    .map(|&[r, y]| [r * s, y * s])
    .collect();
    let phase = (seed % 97) as f32;
    let mut puck = lathe_warped(&profile, &LatheOptions::new(36, Shading::SMOOTH), |p| {
        let a = p[2].atan2(p[0]);
        let wobble = 1.0 + 0.07 * (3.0 * a + phase).sin() + 0.04 * (7.0 * a + phase * 1.7).sin();
        [p[0] * wobble, p[1], p[2] * wobble]
    });
    let points = sc.sigil.clamp(3, 9) as usize;
    let star: Vec<P2> = (0..points * 2)
        .map(|i| {
            let a = PI * i as f32 / points as f32;
            let r = if i % 2 == 0 { 0.075 * s } else { 0.032 * s };
            [r * a.sin(), r * a.cos()]
        })
        .collect();
    let depth = 0.014 * s;
    let mut sigil = extrude(&star, &[], depth);
    sigil.rotate_x(-FRAC_PI_2).translate([0.0, 0.058 * s + depth * 0.5, 0.0]);
    puck.append(&sigil);
    puck.translate(at);
    puck
}
