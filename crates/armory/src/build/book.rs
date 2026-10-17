//! Bound books: boards and a rounded spine round a block of leaves, with spine bands,
//! corners, clasps or a strap, a boss or a gem, a chain, a ribbon, and lying shut or open.
//! The writing family routes its Book kind here ([`crate::families::writing`]).
//!
//! Built in book space: the spine runs along X, the pages spread across Z from the spine at
//! z = 0 and the book stacks up Y with its back board on y = 0. A closed book has its fore-edge
//! at z = [`Dims::w`]; an open one has the spine in the middle with a half-book each side.
//! The result is centred over the origin with its longer side along X. Condition drives the
//! wear: scuffs and rubbed corners on the covers, frayed cloth, uneven leaves and browned paper.
//! Rust and patina on metal come from the item's condition at draw time, as in every family.

use std::f32::consts::{FRAC_PI_2, PI};

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::build::chain::{cable_chain, cord, jump_ring};
use crate::build::fitting::rounded_rect;
use crate::build::gem::{collet, collet_seat, cut_stone};
use crate::build::pages::{LeafStack, leaf_stack};
use crate::build::parts::{Look, Parts};
use crate::build::strap::{buckle, rivets, strap};
use crate::cond::Condition;
use crate::families::writing::{Piece, TIES};
use crate::gem::{GemCut, Gemstone};
use crate::geom::{
    CatmullRom, GridOptions, LatheOptions, MeshBuilder, Shading, SlabOptions, V3, bevelled_slab, ellipsoid, extrude, lathe, spline_profile, stitch_grid,
    thick_sheet,
};
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::slider::sliders;

use MaterialKind::{Cloth, Cord, Gold, Leather, Paint, Wood};

/// What the boards are covered in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cover {
    Leather,
    Cloth,
    /// Bare wooden boards with a leather spine.
    Wood,
}

impl Cover {
    pub const ALL: [Cover; 3] = [Cover::Leather, Cover::Cloth, Cover::Wood];
}

/// How the edges of the leaves are finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    Plain,
    Gilt,
    Red,
}

impl Edge {
    pub const ALL: [Edge; 3] = [Edge::Plain, Edge::Gilt, Edge::Red];
}

/// What holds the book shut.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Closure {
    None,
    Clasp,
    TwinClasps,
    /// A leather strap round the book, buckled on the cover.
    Strap,
}

impl Closure {
    pub const ALL: [Closure; 4] = [Closure::None, Closure::Clasp, Closure::TwinClasps, Closure::Strap];
}

/// What stands out of the middle of the front cover.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Centre {
    None,
    Boss,
    Gem,
}

impl Centre {
    pub const ALL: [Centre; 3] = [Centre::None, Centre::Boss, Centre::Gem];
}

/// The metal of a book's fittings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metal {
    Iron,
    Brass,
    Silver,
    Gold,
}

impl Metal {
    pub const ALL: [Metal; 4] = [Metal::Iron, Metal::Brass, Metal::Silver, Metal::Gold];

    pub fn material(self) -> MaterialKind {
        match self {
            Metal::Iron => MaterialKind::Iron,
            Metal::Brass => MaterialKind::Brass,
            Metal::Silver => MaterialKind::Silver,
            Metal::Gold => MaterialKind::Gold,
        }
    }
}

/// A book's own options.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookOpts {
    /// Along the spine: 1.4 for a pocket book up to 4 for a great grimoire.
    pub length: f32,
    /// Width of a cover over its length.
    pub aspect: f32,
    /// The block of leaves, between the boards.
    pub thick: f32,
    /// How thick each board is.
    pub board: f32,
    /// How far the boards stand out past the leaves.
    pub square: f32,
    /// 0 flat spine, 1 fully round.
    pub round: f32,
    /// Raised cords across the spine.
    pub bands: u32,
    pub band_raise: f32,
    pub cover: Cover,
    /// Dye of a cloth cover.
    pub cover_color: Rgb,
    /// A tooled frame round the front cover.
    pub border: bool,
    pub edge: Edge,
    /// How unevenly the leaves sit, 0..=1.
    pub uneven: f32,
    /// How far the boards warp (curl away from the leaves), 0..=1.
    pub warp: f32,
    pub metal: Metal,
    pub corners: bool,
    pub closure: Closure,
    pub centre: Centre,
    pub stone: Gemstone,
    pub cut: GemCut,
    /// A chained library book.
    pub chain: bool,
    pub chain_len: f32,
    pub ribbon: bool,
    pub ribbon_color: Rgb,
    pub ribbon_len: f32,
    /// Lying open, not shut.
    pub open: bool,
    /// Leaves lifted and fanned on an open book.
    pub fan: u32,
    pub fan_lift: f32,
    /// Share of the leaves on the right of an open book.
    pub split: f32,
}

sliders! {
    BookLimits for BookOpts {
        length: 1.4, 4.0, 0.05;
        aspect: 0.6, 0.85, 0.01;
        thick: 0.12, 0.9, 0.01;
        board: 0.02, 0.09, 0.005;
        square: 0.01, 0.06, 0.005;
        round: 0.0, 1.0, 0.05;
        bands: 0.0, 6.0, 1.0;
        band_raise: 0.008, 0.03, 0.001;
        uneven: 0.0, 1.0, 0.05;
        warp: 0.0, 1.0, 0.05;
        chain_len: 0.8, 2.2, 0.05;
        ribbon_len: 0.2, 0.8, 0.05;
        fan: 0.0, 8.0, 1.0;
        fan_lift: 0.2, 1.0, 0.05;
        split: 0.3, 0.7, 0.05;
    }
}

/// Cloth dyes: oxblood, navy, forest, plum, ochre, slate.
pub const CLOTHS: [Rgb; 6] = [Rgb::hex(0x7a2a2a), Rgb::hex(0x2c3f6e), Rgb::hex(0x34553a), Rgb::hex(0x5a3a6e), Rgb::hex(0x9a7a32), Rgb::hex(0x4a5560)];

impl Default for BookOpts {
    fn default() -> Self {
        BookOpts {
            length: 2.4,
            aspect: 0.72,
            thick: 0.4,
            board: 0.035,
            square: 0.03,
            round: 0.6,
            bands: 3,
            band_raise: 0.016,
            cover: Cover::Leather,
            cover_color: CLOTHS[0],
            border: false,
            edge: Edge::Plain,
            uneven: 0.5,
            warp: 0.0,
            metal: Metal::Brass,
            corners: false,
            closure: Closure::None,
            centre: Centre::None,
            stone: Gemstone::Ruby,
            cut: GemCut::Cabochon,
            chain: false,
            chain_len: 1.4,
            ribbon: false,
            ribbon_color: TIES[0],
            ribbon_len: 0.5,
            open: false,
            fan: 0,
            fan_lift: 0.6,
            split: 0.5,
        }
    }
}

/// "Forge new" for a book, drawing from its own stream so the other kinds' rolls are untouched.
pub fn roll_book(rng: &mut Mulberry32) -> BookOpts {
    let mut b = BookOpts::default();
    // Size class: pocket book, book, tome, grimoire.
    let (lo, hi, tlo, thi) = *rng.weighted(&[((1.4, 1.9, 0.12, 0.3), 2), ((2.0, 2.8, 0.2, 0.6), 5), ((2.8, 3.4, 0.35, 0.8), 3), ((3.4, 4.0, 0.5, 0.9), 2)]);
    b.length = rng.range_f32(lo, hi);
    b.aspect = rng.range_f32(0.62, 0.8);
    b.thick = rng.range_f32(tlo, thi);
    b.cover = *rng.weighted(&[(Cover::Leather, 5), (Cover::Cloth, 3), (Cover::Wood, 3)]);
    b.board = if b.cover == Cover::Wood { rng.range_f32(0.05, 0.085) } else { rng.range_f32(0.025, 0.045) };
    b.square = rng.range_f32(0.015, 0.045);
    b.round = rng.range_f32(0.2, 1.0);
    b.bands = *rng.weighted(&[(0, 3), (1, 1), (2, 2), (3, 2), (4, 2), (5, 1), (6, 2)]);
    b.band_raise = rng.range_f32(0.01, 0.026);
    b.cover_color = *rng.pick(&CLOTHS);
    b.border = rng.chance(0.3);
    b.edge = *rng.weighted(&[(Edge::Plain, 5), (Edge::Gilt, 2), (Edge::Red, 2)]);
    b.uneven = rng.range_f32(0.2, 0.9);
    b.warp = if b.cover == Cover::Wood && rng.chance(0.5) { rng.range_f32(0.2, 0.8) } else { rng.range_f32(0.0, 0.2) };
    b.metal = *rng.weighted(&[(Metal::Iron, 3), (Metal::Brass, 4), (Metal::Silver, 1), (Metal::Gold, 1)]);
    b.corners = rng.chance(if b.cover == Cover::Wood { 0.6 } else { 0.25 });
    b.closure = *rng.weighted(&[(Closure::None, 5), (Closure::Clasp, 2), (Closure::TwinClasps, 2), (Closure::Strap, 2)]);
    b.centre = *rng.weighted(&[(Centre::None, 5), (Centre::Boss, 2), (Centre::Gem, 2)]);
    let cut = *rng.pick(&[GemCut::Cabochon, GemCut::Brilliant, GemCut::Rose, GemCut::Octahedron]);
    let stones: Vec<Gemstone> = Gemstone::ALL.into_iter().filter(|g| g.suits(cut)).collect();
    b.stone = *rng.pick(&stones);
    b.cut = cut;
    b.chain = rng.chance(0.12);
    b.chain_len = rng.range_f32(1.0, 2.0);
    b.ribbon = rng.chance(0.3);
    b.ribbon_color = *rng.pick(&TIES);
    b.ribbon_len = rng.range_f32(0.25, 0.7);
    b.open = rng.chance(0.3);
    b.fan = if rng.chance(0.6) { rng.int(2, 7) } else { 0 };
    b.fan_lift = rng.range_f32(0.3, 0.9);
    b.split = rng.range_f32(0.4, 0.6);
    b
}

/// What a book is called.
pub fn book_noun(b: &BookOpts) -> &'static str {
    if b.chain {
        "Chained tome"
    } else if b.length < 1.8 {
        "Pocket book"
    } else if b.length >= 3.3 && (b.centre != Centre::None || b.corners) {
        "Grimoire"
    } else if b.cover == Cover::Cloth && b.closure == Closure::None && b.thick >= 0.35 {
        "Ledger"
    } else if b.length >= 2.9 || b.cover == Cover::Wood {
        "Tome"
    } else {
        "Book"
    }
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

/// A book's measurements.
struct Dims {
    /// Length along the spine and width of a cover.
    l: f32,
    w: f32,
    /// Board thickness, block thickness and the whole.
    tb: f32,
    t: f32,
    h: f32,
    /// How far the boards stand out past the leaves.
    sq: f32,
    /// How far the spine bulges out from the boards' edge.
    ru: f32,
    /// Leaves, their pitch and thickness.
    n: usize,
    pitch: f32,
    leaf: f32,
}

impl Dims {
    fn of(b: &BookOpts) -> Dims {
        let tb = b.board;
        let h = 2.0 * tb + b.thick;
        let ru = (h * 0.5 * (0.2 + 0.8 * b.round)).max(tb + 0.03);
        let n = ((b.thick / 0.02).round() as usize).clamp(3, 56);
        let pitch = b.thick / n as f32;
        Dims { l: b.length, w: b.length * b.aspect, tb, t: b.thick, h, sq: b.square, ru, n, pitch, leaf: pitch * 0.8 }
    }

    /// Half the leaves' length.
    fn half_block(&self) -> f32 {
        (self.l - 2.0 * self.sq) * 0.5
    }

    /// The spine's outer curve at angle `a` (0 at the bottom board, pi at the top): point in
    /// (z, y) and its outward normal.
    fn spine(&self, a: f32, inset: f32) -> ([f32; 2], [f32; 2]) {
        let (au, ay) = (self.ru - inset, self.h * 0.5 - inset);
        let p = [-au * a.sin(), self.h * 0.5 - ay * a.cos()];
        let n = [-ay * a.sin(), -au * a.cos()];
        let len = n[0].hypot(n[1]).max(1e-6);
        (p, [n[0] / len, n[1] / len])
    }
}

/// Meshes in build order with their looks.
struct Out(Vec<(&'static str, MeshBuilder, Look)>);

impl Out {
    fn add(&mut self, name: &'static str, mesh: MeshBuilder, look: Look) {
        if !mesh.is_empty() {
            self.0.push((name, mesh, look));
        }
    }
}

/// A flat rounded slab `sx` along X and `sz` along Z, centred at (`cx`, `cz`), its middle at `y`.
fn flat_slab(cx: f32, cz: f32, sx: f32, sz: f32, y: f32, depth: f32, r: f32) -> MeshBuilder {
    let mut m = bevelled_slab(&rounded_rect(cx, -cz, sx, sz, r), &[], &SlabOptions::rounded(depth, depth * 0.3));
    m.rotate_x(-FRAC_PI_2).translate([0.0, y, 0.0]);
    m
}

fn strip(a: Vec<V3>, b: Vec<V3>) -> MeshBuilder {
    stitch_grid(&[a, b], &GridOptions { shading: Shading::SMOOTH, ..Default::default() })
}

/// A path round a rounded rectangle in the (z, y) plane at `x`, with the outward face at each
/// point. `closed` goes all the way round; otherwise it is the fore-edge wrap that runs over the
/// top from `z_top`, down the edge at `z1` and under to `z_bot`.
fn rect_path(x: f32, (z0, z1): (f32, f32), (y0, y1): (f32, f32), r: f32, closed: bool, (z_top, z_bot): (f32, f32)) -> (Vec<V3>, Vec<V3>) {
    let (mut p, mut f): (Vec<V3>, Vec<V3>) = (Vec::new(), Vec::new());
    let line = |p: &mut Vec<V3>, f: &mut Vec<V3>, from: [f32; 2], to: [f32; 2], face: [f32; 2], steps: usize| {
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            p.push([x, lerp(from[1], to[1], t), lerp(from[0], to[0], t)]);
            f.push([0.0, face[1], face[0]]);
        }
    };
    let arc = |p: &mut Vec<V3>, f: &mut Vec<V3>, c: [f32; 2], a0: f32, a1: f32| {
        for k in 1..5 {
            let a = lerp(a0, a1, k as f32 / 5.0);
            p.push([x, c[1] + r * a.sin(), c[0] + r * a.cos()]);
            f.push([0.0, a.sin(), a.cos()]);
        }
    };
    let start = if closed { z0 + r } else { z_top };
    line(&mut p, &mut f, [start, y1], [z1 - r, y1], [0.0, 1.0], 3);
    arc(&mut p, &mut f, [z1 - r, y1 - r], FRAC_PI_2, 0.0);
    line(&mut p, &mut f, [z1, y1 - r], [z1, y0 + r], [1.0, 0.0], 4);
    arc(&mut p, &mut f, [z1 - r, y0 + r], 0.0, -FRAC_PI_2);
    if closed {
        line(&mut p, &mut f, [z1 - r, y0], [z0 + r, y0], [0.0, -1.0], 5);
        arc(&mut p, &mut f, [z0 + r, y0 + r], -FRAC_PI_2, -PI);
        line(&mut p, &mut f, [z0, y0 + r], [z0, y1 - r], [-1.0, 0.0], 4);
        arc(&mut p, &mut f, [z0 + r, y1 - r], PI, FRAC_PI_2);
    } else {
        line(&mut p, &mut f, [z1 - r, y0], [z_bot, y0], [0.0, -1.0], 3);
    }
    (p, f)
}

/// A corner plate's outline, the board's corner at the origin and the plate reaching `size`
/// along both edges.
fn corner_outline(size: f32, rc: f32) -> Vec<[f32; 2]> {
    let mut o: Vec<[f32; 2]> = Vec::new();
    for k in 0..=5 {
        let th = lerp(-FRAC_PI_2, -PI, k as f32 / 5.0);
        o.push([rc + rc * th.cos(), rc + rc * th.sin()]);
    }
    for k in 0..=8 {
        let ph = lerp(FRAC_PI_2, 0.0, k as f32 / 8.0);
        o.push([size * ph.cos(), size * ph.sin()]);
    }
    o
}

/// The whole book.
pub fn build_book(b: &BookOpts, p: &Piece, cond: &Condition, pal: &Palette, seed: u32, parts: &mut Parts) {
    let d = Dims::of(b);
    let mut rng = Mulberry32::new(seed.wrapping_mul(0x9e37_79b9).wrapping_add(77));
    let fox = (cond.wear * cond.rot).clamp(0.0, 1.0);
    let paper = mix(p.color, Rgb::hex(0x7a5a30), 0.35 * fox + 0.1);
    let paper_look = Look::of(Cloth).tinted(paper);
    let metal = Look::of(b.metal.material());
    let (cover_look, spine_look) = match b.cover {
        Cover::Leather => (Look::of(Leather), Look::of(Leather)),
        Cover::Cloth => (Look::of(Cloth).tinted(b.cover_color), Look::of(Cloth).tinted(b.cover_color)),
        Cover::Wood => (Look::of(Wood), Look::of(Leather)),
    };
    let edge_look = match b.edge {
        Edge::Plain => paper_look,
        Edge::Gilt => Look::of(Gold),
        Edge::Red => Look::of(Paint).tinted(Rgb::hex(0x7a1c18)),
    };
    let scuff_look = match b.cover {
        Cover::Leather => Look::of(Leather).tinted(pal.leather.scaled(0.62)),
        Cover::Cloth => Look::of(Cloth).tinted(b.cover_color.scaled(0.72)),
        Cover::Wood => Look::of(Wood).tinted(pal.wood.scaled(0.7)),
    }
    .without_outline();
    let (l, w, tb, h) = (d.l, d.w, d.tb, d.h);
    let hb = d.half_block();
    let open = b.open;

    // Warped boards curl up from the leaves toward their fore-edge and ends.
    let lift = |x: f32, z: f32| -> f32 {
        let (u, e) = ((z.abs() / w).min(1.2), (x / (l * 0.5)).abs().min(1.2));
        b.warp * (0.05 * u * u + 0.025 * e * e * u)
    };
    let warp = |m: &mut MeshBuilder, sign: f32| {
        if b.warp > 0.0 {
            for q in &mut m.positions {
                q[1] += sign * lift(q[0], q[2]);
            }
        }
    };

    let mut out = Out(Vec::new());
    let top_y = if open { tb } else { h };
    let top_lift = |x: f32, z: f32| lift(x, z);

    // Boards.
    let mut boards = MeshBuilder::new();
    if open {
        for s in [1.0f32, -1.0] {
            let mut m = flat_slab(0.0, s * (w + 0.02) * 0.5, l, w - 0.02, tb * 0.5, tb, 0.04);
            warp(&mut m, 1.0);
            boards.append(&m);
        }
        // The hinge joining them under the gutter.
        boards.append(&flat_slab(0.0, 0.0, l, 0.06, tb * 0.35, tb * 0.7, 0.02));
    } else {
        let mut back = flat_slab(0.0, w * 0.5, l, w, tb * 0.5, tb, 0.04);
        warp(&mut back, -1.0);
        let mut front = flat_slab(0.0, w * 0.5, l, w, h - tb * 0.5, tb, 0.04);
        warp(&mut front, 1.0);
        boards.append(&back);
        boards.append(&front);
    }
    out.add("cover", boards, cover_look);

    // Spine and its bands (a shut book only; an open one lies on it).
    if !open {
        let rows: Vec<f32> = vec![-l * 0.5, 0.0, l * 0.5];
        let grid: Vec<Vec<V3>> = rows
            .iter()
            .map(|&x| {
                (0..=28)
                    .map(|k| {
                        let (pt, _) = d.spine(PI * k as f32 / 28.0, 0.0);
                        [x, pt[1], pt[0]]
                    })
                    .collect()
            })
            .collect();
        let mut spine = thick_sheet(&grid, &GridOptions { shading: Shading::SMOOTH, ..Default::default() }, tb);
        let n = b.bands as usize;
        for k in 0..n {
            let x0 = if n == 1 { 0.0 } else { lerp(-l * 0.5 + 0.2, l * 0.5 - 0.2, k as f32 / (n - 1) as f32) };
            let bw = 0.04 + b.band_raise * 2.0;
            let grid: Vec<Vec<V3>> = (0..=6)
                .map(|r| {
                    let s = r as f32 / 6.0;
                    let hh = b.band_raise * (PI * s).sin().max(0.0).powf(0.6);
                    (0..=28)
                        .map(|c| {
                            let (pt, nrm) = d.spine(PI * c as f32 / 28.0, 0.0);
                            [x0 - bw * 0.5 + bw * s, pt[1] + nrm[1] * hh, pt[0] + nrm[0] * hh]
                        })
                        .collect()
                })
                .collect();
            spine.append(&stitch_grid(&grid, &GridOptions { shading: Shading::SMOOTH, ..Default::default() }));
        }
        out.add("spine", spine, spine_look);
    }

    // The block of leaves.
    let mut seed_pages = seed ^ 0x0b00;
    if open {
        let (n_right, g) = (((d.n as f32 * b.split).round() as usize).clamp(1, d.n - 1), 0.28 * w);
        let n_left = d.n - n_right;
        let fan_of = |side_n: usize, side: f32| -> usize {
            let f = if side > 0.0 { b.fan as usize } else { b.fan as usize / 3 };
            f.min(side_n.saturating_sub(1))
        };
        let reach_max = w - d.sq;
        let (mut all_pages, mut all_edge, mut all_ink) = (MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new());
        for (side, count) in [(1.0f32, n_right), (-1.0, n_left)] {
            let fan = fan_of(count, side);
            let first_fan = count - fan;
            let rank = move |i: usize| if i >= first_fan { i + 1 - first_fan } else { 0 };
            let reach = move |i: usize| {
                let r = rank(i);
                reach_max - if r > 0 { 0.1 * b.fan_lift * w * r as f32 / fan as f32 } else { 0.0 }
            };
            let shape = |i: usize, u: f32, x: f32| -> f32 {
                let base = tb + i as f32 * d.pitch * lerp(0.35, 1.0, smooth(0.0, g, u)) + d.leaf;
                let r = rank(i);
                if r == 0 {
                    return base;
                }
                let t = (u / reach(i)).clamp(0.0, 1.0);
                let amp = b.fan_lift * 0.32 * w * r as f32 / fan as f32;
                base + amp * (PI * t.powf(0.75)).sin().max(0.0) * (1.0 + 0.12 * (x * 1.3).sin())
            };
            let flat = LeafStack { count: first_fan, thickness: d.leaf, half_len: hb, u0: 0.012, cols: 12, rows: 2, uneven: b.uneven, seed: seed_pages };
            let mut pages = leaf_stack(&flat, side, &reach, &shape);
            if fan > 0 {
                seed_pages = seed_pages.wrapping_add(31);
                let lifted = LeafStack { count: fan, cols: 18, rows: 5, seed: seed_pages, ..flat };
                pages.append(&leaf_stack(&lifted, side, &|j| reach(j + first_fan), &|j, u, x| shape(j + first_fan, u, x)));
            }
            all_pages.append(&pages);

            let top = count - 1;
            let (ua, ub) = (g * 0.9 + 0.08, reach(top) - 0.1);
            if b.edge != Edge::Plain {
                // A band of gilt or stain on the fore-edge of the top leaf.
                let rim = |u: f32| -> Vec<V3> { (0..=8).map(|k| lerp(-hb, hb, k as f32 / 8.0)).map(|x| [x, shape(top, u, x) + 0.002, side * u]).collect() };
                let (near, far) = (reach(top) - 0.035, reach(top) - 0.005);
                let (a, c) = if side > 0.0 { (rim(far), rim(near)) } else { (rim(near), rim(far)) };
                all_edge.append(&strip(a, c));
            }
            if p.lines > 0 && ub > ua {
                all_ink.append(&ink(&mut rng, (ua, ub), hb, p.lines, side, &|u, x| shape(top, u, x) + 0.003));
            }
        }
        out.add("pages", all_pages, paper_look);
        out.add("edge", all_edge, edge_look.without_outline());
        out.add("ink", all_ink, Look::of(Paint).tinted(p.ink).without_outline());
    } else {
        // The back of the block, filling the spine's hollow.
        let inner: Vec<[f32; 2]> = (0..=20)
            .map(|k| {
                let (pt, _) = d.spine(PI * k as f32 / 20.0, tb);
                [pt[0], pt[1]]
            })
            .collect();
        let mut back = extrude(&inner, &[], hb * 2.0);
        back.rotate_y(-FRAC_PI_2);
        let stack = LeafStack { count: d.n, thickness: d.leaf, half_len: hb, u0: 0.0, cols: 3, rows: 2, uneven: b.uneven, seed: seed_pages };
        let shape = |i: usize, _u: f32, _x: f32| tb + i as f32 * d.pitch + d.leaf;
        let mut block = leaf_stack(&stack, 1.0, &|_| w - d.sq, &shape);
        if b.edge == Edge::Plain {
            block.append(&back);
            out.add("pages", block, paper_look);
        } else {
            out.add("pages", block, edge_look);
            out.add("back", back, paper_look);
        }
    }

    // Corners, the border and the centre of the front cover.
    let mut fittings = MeshBuilder::new();
    let mut tooling = MeshBuilder::new();
    let size = (0.07 * w).clamp(0.12, 0.28);
    if b.corners {
        let plate = |cx: f32, cz: f32, ix: f32, iz: f32, y: f32| {
            let o: Vec<[f32; 2]> = corner_outline(size, 0.04).iter().map(|&[a, c]| [cx + ix * a, -(cz + iz * c)]).collect();
            let mut m = bevelled_slab(&o, &[], &SlabOptions::rounded(0.016, 0.005));
            m.rotate_x(-FRAC_PI_2).translate([0.0, y, 0.0]);
            m
        };
        let sides: Vec<(f32, f32)> = if open { vec![(1.0, 1.0), (-1.0, 1.0)] } else { vec![(1.0, 1.0)] };
        for (s, _) in sides {
            for ix in [1.0f32, -1.0] {
                // Fore-edge corner and spine-side corner of this board.
                for (cz, iz) in [(s * w, -s), (s * 0.012, s)] {
                    let cx = -ix * l * 0.5;
                    let y = top_y + 0.008 + top_lift(cx, cz);
                    fittings.append(&plate(cx, cz, ix, iz, y));
                    if !open {
                        fittings.append(&plate(cx, cz, ix, iz, -0.008));
                    }
                }
            }
        }
    }
    if !open {
        if b.border {
            let (bw, bl) = (w * 0.78, l * 0.82);
            let outer = rounded_rect(0.0, 0.0, bl, bw, 0.05);
            let hole = rounded_rect(0.0, 0.0, bl - 0.07, bw - 0.07, 0.03);
            let mut frame = bevelled_slab(&outer, &[hole], &SlabOptions::rounded(0.012, 0.004));
            frame.rotate_x(-FRAC_PI_2).translate([0.0, h + 0.004, w * 0.5]);
            warp(&mut frame, 1.0);
            let look = match b.cover {
                Cover::Leather => Look::of(Leather).tinted(pal.leather.scaled(0.7)),
                Cover::Cloth => Look::of(Cloth).tinted(b.cover_color.scaled(0.65)),
                Cover::Wood => metal,
            };
            tooling.append(&frame);
            out.add("tooling", tooling, look);
        }
        let mid_z = w * 0.5;
        let c_lift = top_lift(0.0, mid_z);
        let r = (0.07 * w).clamp(0.07, 0.2);
        match b.centre {
            Centre::Boss => {
                let hh = r * 0.5;
                let prof = spline_profile(&[[r * 1.25, 0.0], [r * 1.1, hh * 0.35], [r * 0.85, hh * 0.8], [r * 0.4, hh * 1.05], [0.0, hh * 1.1]], 12);
                let mut boss = lathe(&prof, &LatheOptions::new(28, Shading::SMOOTH));
                boss.translate([0.0, h + c_lift, mid_z]);
                fittings.append(&boss);
            }
            Centre::Gem => {
                let prof = spline_profile(&[[r * 1.6, 0.0], [r * 1.6, 0.018], [r * 1.2, 0.04], [0.0, 0.048]], 8);
                let mut base = lathe(&prof, &LatheOptions::new(28, Shading::SMOOTH));
                base.translate([0.0, h + c_lift, mid_z]);
                fittings.append(&base);
                let rg = r * 0.7;
                let mut cup = collet(rg, rg * 0.35, 16);
                cup.translate([0.0, h + c_lift + 0.045, mid_z]);
                fittings.append(&cup);
                let mut stone = cut_stone(b.cut, rg * 0.9);
                stone.translate([0.0, h + c_lift + 0.045 + collet_seat(rg, rg * 0.35), mid_z]);
                out.add("gem", stone, Look::stone(b.stone, 0.18));
            }
            Centre::None => {}
        }
    }

    // Closure: clasps over the fore-edge, or a strap round the whole book.
    let mut straps = MeshBuilder::new();
    if !open {
        let cl = 0.004;
        let xs: Vec<f32> = match b.closure {
            Closure::Clasp => vec![0.0],
            Closure::TwinClasps => vec![-l * 0.25, l * 0.25],
            _ => vec![],
        };
        for x in xs {
            let (path, faces) = rect_path(x, (0.0, w + cl), (-cl, h + cl), 0.03, false, (w - 0.26, w - 0.1));
            fittings.append(&strap(&path, &faces, 0.11, 0.012, false));
            let mut pins = vec![([x, h + cl + 0.012, w - 0.22], [0.0, 1.0, 0.0]), ([x, h + cl + 0.012, w - 0.06], [0.0, 1.0, 0.0])];
            pins.push(([x, -cl - 0.012, w - 0.12], [0.0, -1.0, 0.0]));
            fittings.append(&rivets(&pins, 0.02));
        }
        if b.closure == Closure::Strap {
            let x = if l > 2.0 { l * 0.18 } else { 0.0 };
            let (path, faces) = rect_path(x, (-d.ru - cl, w + cl), (-cl, h + cl), 0.05, true, (0.0, 0.0));
            straps.append(&strap(&path, &faces, 0.13, 0.012, true));
            let zb = w * 0.62;
            let mut bk = buckle(0.13, 0.012);
            bk.place_basis([x, h + cl + 0.012, zb], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
            fittings.append(&bk);
            let tail: Vec<V3> = (0..=6).map(|k| [x, h + cl + 0.026, zb + 0.03 + 0.36 * k as f32 / 6.0]).collect();
            straps.append(&strap(&tail, &vec![[0.0, 1.0, 0.0]; tail.len()], 0.12, 0.01, false));
            let spots: Vec<(V3, V3)> = [0.2, 0.3].iter().map(|dz| ([x, h + cl + 0.036, zb + dz], [0.0, 1.0, 0.0])).collect();
            fittings.append(&rivets(&spots, 0.018));
        }
    }
    out.add("fittings", fittings, metal);
    out.add("strap", straps, Look::of(Leather).tinted(pal.leather.scaled(0.8)));

    // Wear on the covers.
    if !open {
        let scuffs = (cond.wear * 18.0).round() as u32;
        let mut sc = MeshBuilder::new();
        for _ in 0..scuffs {
            let (x, z) = (rng.range_f32(-l * 0.45, l * 0.45), rng.range_f32(0.1, w - 0.05));
            let mut m = ellipsoid([rng.range_f32(0.04, 0.2), 0.004, rng.range_f32(0.03, 0.14)]);
            m.translate([x, h - 0.0005 + top_lift(x, z), z]);
            sc.append(&m);
        }
        out.add("scuff", sc, scuff_look);
        if b.cover == Cover::Leather {
            let mut rubbed = MeshBuilder::new();
            for _ in 0..(cond.damage * 5.0).round() as u32 {
                let (x, z) = ([-1.0f32, 1.0][rng.int(0, 1) as usize] * (l * 0.5 - 0.05), [0.04, w - 0.04][rng.int(0, 1) as usize]);
                let mut m = ellipsoid([0.05, 0.004, 0.05]);
                m.translate([x, h - 0.0005 + top_lift(x, z), z]);
                rubbed.append(&m);
            }
            out.add("rubbed", rubbed, Look::of(Wood).tinted(pal.wood.scaled(0.8)).without_outline());
        }
        if b.cover == Cover::Cloth {
            let mut threads = MeshBuilder::new();
            for _ in 0..(cond.fray * cond.wear * 10.0).round() as u32 {
                let (x, z) = ([-1.0f32, 1.0][rng.int(0, 1) as usize] * l * 0.5, [0.0, w][rng.int(0, 1) as usize]);
                let out_x = x.signum();
                let y = h - tb * 0.4;
                let pts = vec![
                    [x, y, z],
                    [x + out_x * rng.range_f32(0.03, 0.06), y - 0.01, z + rng.range_f32(-0.03, 0.03)],
                    [x + out_x * rng.range_f32(0.06, 0.12), y - rng.range_f32(0.03, 0.08), z + rng.range_f32(-0.05, 0.05)],
                    [x + out_x * rng.range_f32(0.07, 0.15), y - rng.range_f32(0.08, 0.14), z + rng.range_f32(-0.06, 0.06)],
                ];
                threads.append(&cord(&CatmullRom::centripetal(pts).sample(5), false, 0.004, 4));
            }
            out.add("fray", threads, Look::of(Cord).tinted(b.cover_color.scaled(0.85)).without_outline());
        }
    }

    // Ribbon bookmark, coming out of the tail of the block and hanging to the floor.
    if b.ribbon {
        let (z0, ym) = if open { (0.05, tb + 0.05) } else { (w * 0.35, tb + d.t * 0.5) };
        let xt = -l * 0.5 - 0.03;
        let up = [0.0, 1.0, 0.0];
        let down = [-0.7, 0.7, 0.0];
        let pts: Vec<(V3, V3)> = vec![
            ([-hb + 0.25, ym, z0], up),
            ([xt + 0.02, ym, z0], up),
            ([xt - 0.04, ym - 0.05, z0], down),
            ([xt - 0.08, (ym * 0.5).max(0.05), z0 + 0.01], [-1.0, 0.2, 0.0]),
            ([xt - 0.12, 0.012, z0 + 0.02], up),
            ([xt - 0.12 - b.ribbon_len * 0.5, 0.008, z0 + 0.05], up),
            ([xt - 0.12 - b.ribbon_len, 0.008, z0 + 0.1], up),
        ];
        let (path, faces): (Vec<V3>, Vec<V3>) = pts.into_iter().unzip();
        let path = CatmullRom::centripetal(path).sample(5);
        let faces: Vec<V3> = (0..path.len()).map(|i| faces[(i * faces.len() / path.len()).min(faces.len() - 1)]).collect();
        out.add("ribbon", strap(&path, &faces, 0.1, 0.006, false), Look::of(Cloth).tinted(b.ribbon_color));
    }

    // Chain from a staple on the fore-edge of the back board, down and away along the floor.
    let mut chain = MeshBuilder::new();
    if b.chain {
        let (x, y, z) = (l * 0.3, tb * 0.5, w);
        let mut staple = bevelled_slab(&rounded_rect(x, y, 0.16, tb * 1.4, 0.015), &[], &SlabOptions::rounded(0.02, 0.006));
        staple.translate([0.0, 0.0, z + 0.01]);
        chain.append(&staple);
        let mut ring = jump_ring(0.045, 0.009);
        ring.rotate_y(FRAC_PI_2).translate([x, y, z + 0.05]);
        chain.append(&ring);
        let g = 0.03;
        let run = b.chain_len;
        let mut pts: Vec<V3> = vec![[x, y, z + 0.08], [x, y - 0.03, z + 0.2], [x + 0.03, g + 0.05, z + 0.36], [x + 0.1, g, z + 0.55]];
        for k in 1..=5 {
            let t = k as f32 / 5.0;
            pts.push([x + 0.1 + 0.28 * (t * 5.0).sin() * 0.5, g, z + 0.55 + run * t]);
        }
        let end = *pts.last().unwrap_or(&[x, g, z]);
        let path = CatmullRom::centripetal(pts).sample(6);
        chain.append(&cable_chain(&path, false, 0.07, 0.009, 90));
        let mut big = jump_ring(0.1, 0.014);
        big.rotate_x(FRAC_PI_2).translate([end[0], g, end[2] + 0.1]);
        chain.append(&big);
    }
    out.add("chain", chain, metal);

    // Centre over the origin, longer side along X, then set down.
    let body = |name: &str| !matches!(name, "chain" | "ribbon");
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for (n, m, _) in out.0.iter().filter(|(n, _, _)| body(n)) {
        let _ = n;
        if let Some((a, c)) = m.bounds() {
            for k in 0..3 {
                lo[k] = lo[k].min(a[k]);
                hi[k] = hi[k].max(c[k]);
            }
        }
    }
    let centre = [(lo[0] + hi[0]) * 0.5, (lo[2] + hi[2]) * 0.5];
    let turn = hi[2] - lo[2] > hi[0] - lo[0];
    for (name, mut m, look) in out.0 {
        m.translate([-centre[0], 0.0, -centre[1]]);
        if turn {
            m.rotate_y(FRAC_PI_2);
        }
        parts.add_look(name, m, look);
    }
}

/// Rows of ink across a page: words as thin strips lying on the leaf. `side` picks the page
/// (+1 right of the spine), `range` is the span from the spine it may cover and `top(u, x)` the
/// height of the page there.
fn ink(rng: &mut Mulberry32, range: (f32, f32), half: f32, lines: u32, side: f32, top: &dyn Fn(f32, f32) -> f32) -> MeshBuilder {
    let (ua, ub) = range;
    let pitch = 0.07;
    let avail = ((2.0 * half - 0.3) / pitch).floor().max(1.0) as u32;
    let rows = avail.min(lines * 4);
    let x0 = -(rows as f32 - 1.0) * pitch * 0.5;
    let mut out = MeshBuilder::new();
    for row in 0..rows {
        let x = x0 + pitch * row as f32;
        let end = ub - if rng.chance(0.2) { rng.range_f32(0.05, 0.4) } else { 0.0 };
        let width = rng.range_f32(0.012, 0.016);
        let mut u = ua;
        while u < end - 0.08 {
            let len = rng.range_f32(0.08, 0.3).min(end - u);
            let segs = (len / 0.06).ceil().max(2.0) as usize;
            let line = |dx: f32| -> Vec<V3> {
                let us: Vec<f32> = (0..=segs).map(|i| u + len * i as f32 / segs as f32).collect();
                let mut pts: Vec<V3> = us.iter().map(|&uu| [x + dx, top(uu, x + dx), side * uu]).collect();
                // Columns run toward +Z so the strip faces up.
                if side < 0.0 {
                    pts.reverse();
                }
                pts
            };
            out.append(&strip(line(0.0), line(width)));
            u += len + rng.range_f32(0.03, 0.07);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolls_stay_in_range_and_builds_are_finite() {
        use crate::slider::Sliders;
        for seed in 0..40u32 {
            let mut rng = Mulberry32::new(seed);
            let b = roll_book(&mut rng);
            assert!(b.out_of_range(&BookLimits::SLIDERS).is_empty(), "{seed}: {:?}", b.out_of_range(&BookLimits::SLIDERS));
        }
    }
}
