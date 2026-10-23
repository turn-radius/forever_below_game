//! Sundries: the small goods a delver carries. Candles, a coil of rope, a roll of lockpicks,
//! a ring of keys, a tinderbox, a bedroll and a roll of bandages.
//!
//! These are the goods the plate drew outside the Armory (`Rope`, `Lockpicks`, `Candle` and
//! `Bandages` in `data/goods.json`, `goodGroup()` in `reference/forever-below-plate.html`), plus
//! keys, a tinderbox and a bedroll the plate's text names (lockpicking, "fever took them in
//! their bedroll") but never drew. The waterskin, the lamp-oil flask and the glowcap are food
//! (`crate::families::food`), the purse a bag. The Armory prototype has no sundries, so the
//! sliders here are new.
//!
//! Departures from the plate:
//! - The plate's candle was a lathed stick and a wick. Here it can stand in a chamberstick,
//!   carry runs of wax, be burnt down and be lit, and [`Sundry::light`] says where its light
//!   is ("Light 5 ft").
//! - The plate's rope was four stacked tori; here it is one twisted rope laid in a coil, tied
//!   round twice, with its end trailing. Its lockpicks were four rods in a block; here they
//!   are hooks, rakes, diamonds and a tension wrench tucked in an open leather roll.
//! - Bandages are a rolled strip of linen with its end unrolled, not a cylinder.
//!
//! Item space: each good rests on the ground (y = 0), centred over the origin, its long side
//! along X. A candle stands up.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use crate::build::chain::{thong, twisted_cord};
use crate::build::fitting::rounded_rect;
use crate::build::key::{KeyBow, key};
use crate::build::light::{CANDLE_LIGHT, CHAR, LightAnchor, add_flame, candle, candle_wick_top};
use crate::build::parts::{Look, Parts};
use crate::build::roll::{roll_straps, rolled_sheet};
use crate::build::strap::strap;
use crate::cond::{Condition, ConditionLimits};
use crate::fabric::{Fabric, FabricPattern};
use crate::families::Generator;
use crate::geom::polygon::P2;
use crate::geom::{
    GridOptions, ItemMesh, LatheOptions, MeshBuilder, Shading, SlabOptions, SweepOptions, V3, bevelled_slab, cuboid, cylinder, ellipsoid,
    icosphere, lathe, spline_profile, thick_sheet, torus, tube, vec3,
};
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::slider::{Sliders, sliders};

use MaterialKind::{Bone, Brass, Cloth, Cord, Iron, Leather, Silver, Stone};

/// The sundry generator.
pub struct Sundry;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SundryKind {
    Candle,
    /// Hemp rope, 50 ft, coiled.
    Rope,
    /// Thieves' picks in a leather roll.
    Lockpicks,
    /// Keys on an iron ring.
    Keys,
    /// A tin of tinder with a fire steel and a flint.
    Tinderbox,
    /// A blanket rolled and strapped.
    Bedroll,
    /// A roll of linen bandage.
    Bandages,
}

impl SundryKind {
    pub const ALL: [SundryKind; 7] = [
        SundryKind::Candle, SundryKind::Rope, SundryKind::Lockpicks, SundryKind::Keys, SundryKind::Tinderbox,
        SundryKind::Bedroll, SundryKind::Bandages,
    ];

    /// Kinds made of, or held in, metal (`Article::material`).
    pub fn has_metal(self) -> bool {
        matches!(self, SundryKind::Candle | SundryKind::Lockpicks | SundryKind::Keys | SundryKind::Tinderbox | SundryKind::Bedroll)
    }
}

/// The good itself.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Article {
    pub kind: SundryKind,
    /// Overall scale.
    pub size: f32,
    /// Runs of wax down a candle (one fewer), turns of rope (two fewer), picks, keys, or a
    /// tinderbox's shape (1 oblong, 2 round).
    pub count: u32,
    /// The metal: a chamberstick, picks, keys, a tinderbox, a bedroll's buckles.
    pub material: MaterialKind,
    /// Wax, rope, the picks' leather roll, linen or blanket.
    pub color: Rgb,
    /// A bedroll's weave.
    pub pattern: FabricPattern,
    pub color2: Rgb,
    /// A candle stands in a chamberstick.
    pub holder: bool,
    /// The keys' bows.
    pub bow: KeyBow,
    /// A candle is alight.
    pub lit: bool,
    /// How far a candle has burnt down, 0..0.8.
    pub burnt: f32,
    /// A bedroll is unrolled and laid out on the floor to sleep on, not rolled and strapped for
    /// carrying. The same bedroll either way: its colours, weave and wear carry over.
    #[serde(default)]
    pub laid: bool,
}

sliders! {
    ArticleLimits for Article {
        size: 0.6, 1.4, 0.01;
        count: 1.0, 8.0, 1.0;
        burnt: 0.0, 0.8, 0.01;
    }
}

/// Tallow, beeswax and bleached wax.
pub const WAXES: [Rgb; 3] = [Rgb::hex(0xe8dfc2), Rgb::hex(0xd9a441), Rgb::hex(0xf2ede0)];
/// Hemp and tarred rope.
pub const ROPES: [Rgb; 3] = [Rgb::hex(0xb89a62), Rgb::hex(0xa58a5a), Rgb::hex(0x6e5a3e)];
/// Linen for bandages.
pub const LINEN: Rgb = Rgb::hex(0xe6dfcc);

impl Default for Article {
    /// The plate's candle.
    fn default() -> Self {
        Article {
            kind: SundryKind::Candle,
            size: 1.0,
            count: 2,
            material: Brass,
            color: WAXES[0],
            pattern: FabricPattern::Plain,
            color2: Fabric::PATTERN_DYES[0],
            holder: false,
            bow: KeyBow::Ring,
            lit: false,
            burnt: 0.0,
            laid: false,
        }
    }
}

/// Every parameter of one sundry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SundryState {
    pub seed: u32,
    pub article: Article,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every sundry section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SundryLimits {
    pub article: ArticleLimits,
    pub cond: ConditionLimits,
}

/// The sundries' slider ranges (new: the prototypes have none).
pub const LIMITS: SundryLimits = SundryLimits { article: ArticleLimits::SLIDERS, cond: ConditionLimits::SLIDERS };

impl SundryState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &SundryLimits) {
        self.article.fit(&limits.article);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &SundryLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [tag("article", self.article.out_of_range(&limits.article)), tag("cond", self.cond.out_of_range(&limits.cond))].concat()
    }
}

/// A candle's light in feet ("Light 5 ft" in `data/goods.json`).
pub const CANDLE_FEET: f32 = 5.0;

impl Generator for Sundry {
    type State = SundryState;

    fn roll(seed: u32) -> SundryState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        use SundryKind as K;
        let kind = *rng.weighted(&[(K::Candle, 3), (K::Rope, 2), (K::Lockpicks, 2), (K::Keys, 2), (K::Tinderbox, 2), (K::Bedroll, 1), (K::Bandages, 2)]);
        compose(&mut rng, seed, kind)
    }

    fn noun(s: &SundryState) -> String {
        let a = &s.article;
        match a.kind {
            SundryKind::Candle if a.holder => "Chamberstick",
            SundryKind::Candle if a.color == WAXES[1] => "Beeswax Candle",
            SundryKind::Candle if a.color == WAXES[0] => "Tallow Candle",
            SundryKind::Candle => "Candle",
            SundryKind::Rope => "Coil of Rope",
            SundryKind::Lockpicks => "Lockpicks",
            SundryKind::Keys if a.count > 1 => "Ring of Keys",
            SundryKind::Keys => "Key on a Ring",
            SundryKind::Tinderbox => "Tinderbox",
            SundryKind::Bedroll => "Bedroll",
            SundryKind::Bandages => "Bandages",
        }
        .to_string()
    }

    fn build(s: &SundryState) -> ItemMesh {
        build_sundry(s)
    }
}

impl Sundry {
    /// "Forge new" forced to one kind, for turntables and tests. Draws the same stream as
    /// [`Generator::roll`], so `roll(s)` equals `roll_as(s, kind)` when `kind` is what `s`
    /// rolls.
    pub fn roll_as(seed: u32, kind: SundryKind) -> SundryState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, kind)
    }

    /// Where a lit candle gives its light: the heart of its flame, 5 ft. `None` for anything
    /// else, or a candle that is out.
    pub fn light(s: &SundryState) -> Option<LightAnchor> {
        let a = &s.article;
        if a.kind != SundryKind::Candle || !a.lit {
            return None;
        }
        let c = CandleLayout::of(a);
        let at = [0.0, c.wick_top + c.flame_height * 0.35, 0.0];
        Some(LightAnchor::feet(at, CANDLE_LIGHT, CANDLE_FEET, 0.5))
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, kind: SundryKind) -> SundryState {
    use SundryKind as K;
    let mut s = SundryState { seed, ..SundryState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    let a = &mut s.article;
    a.kind = kind;
    a.size = rng.range_f32(0.8, 1.2);
    a.count = match kind {
        K::Candle => rng.int(1, 5),
        K::Rope => rng.int(4, 8),
        K::Lockpicks => rng.int(3, 7),
        K::Keys => rng.int(1, 5),
        K::Tinderbox => rng.int(1, 2),
        _ => 1,
    };
    a.material = match kind {
        K::Candle | K::Bedroll => *rng.weighted(&[(Brass, 2), (Iron, 2)]),
        K::Lockpicks => *rng.weighted(&[(Iron, 3), (Silver, 1)]),
        K::Keys => *rng.weighted(&[(Iron, 4), (Brass, 2)]),
        K::Tinderbox => *rng.weighted(&[(Iron, 3), (Brass, 1)]),
        _ => Cord,
    };
    a.color = match kind {
        K::Candle => *rng.weighted(&[(WAXES[0], 3), (WAXES[1], 2), (WAXES[2], 1)]),
        K::Rope => *rng.pick(&ROPES),
        K::Lockpicks => *rng.pick(&[Rgb::hex(0x4a3226), Rgb::hex(0x5e3b26), Rgb::hex(0x2f2a24)]),
        K::Bedroll => *rng.pick(&Fabric::GARMENT_DYES),
        K::Bandages => LINEN,
        K::Keys | K::Tinderbox => a.color,
    };
    if kind == K::Bedroll {
        a.pattern = *rng.weighted(&[(FabricPattern::Plain, 3), (FabricPattern::Stripes, 2), (FabricPattern::Checks, 1), (FabricPattern::Tartan, 1)]);
        a.color2 = *rng.pick(&Fabric::PATTERN_DYES);
    }
    a.holder = kind == K::Candle && rng.chance(0.4);
    a.bow = if kind == K::Keys { *rng.pick(&KeyBow::ALL) } else { KeyBow::Ring };
    if kind == K::Candle {
        a.lit = rng.chance(0.5);
        a.burnt = if a.lit || rng.chance(0.5) { rng.range_f32(0.1, 0.7) } else { 0.0 };
    }
    s.fit(&LIMITS);
    s
}

// ───────────────────────── Geometry ─────────────────────────

fn build_sundry(s: &SundryState) -> ItemMesh {
    let mut parts = Parts::new();
    match s.article.kind {
        SundryKind::Candle => candle_good(s, &mut parts),
        SundryKind::Rope => rope(s, &mut parts),
        SundryKind::Lockpicks => lockpicks(s, &mut parts),
        SundryKind::Keys => keys(s, &mut parts),
        SundryKind::Tinderbox => tinderbox(s, &mut parts),
        SundryKind::Bedroll if s.article.laid => laid_bedroll(s, &mut parts),
        SundryKind::Bedroll => bedroll(s, &mut parts),
        SundryKind::Bandages => bandages(s, &mut parts),
    }
    // Straps, buckles and ties stand a little proud of what they bind. A candle already
    // stands on y = 0, so its light needs no shift.
    if s.article.kind != SundryKind::Candle {
        parts.rest_on_ground();
    }
    parts.finish()
}

/// A candle's heights, shared by the build and [`Sundry::light`].
struct CandleLayout {
    radius: f32,
    height: f32,
    /// Where the candle stands (on the floor, or in a chamberstick's socket).
    base: f32,
    burnt: bool,
    wick_top: f32,
    flame_height: f32,
}

impl CandleLayout {
    fn of(a: &Article) -> CandleLayout {
        let s = a.size;
        let radius = 0.2 * s;
        let height = 1.1 * s * (1.0 - a.burnt * 0.85);
        let base = if a.holder { 0.12 * s } else { 0.0 };
        let burnt = a.lit || a.burnt > 0.0;
        CandleLayout { radius, height, base, burnt, wick_top: base + candle_wick_top(radius, height, burnt), flame_height: 0.42 * s }
    }
}

fn candle_good(s: &SundryState, parts: &mut Parts) {
    let a = &s.article;
    let c = CandleLayout::of(a);
    let (mut wax, mut wick) = candle(c.radius, c.height, a.count.saturating_sub(1), c.burnt, s.seed);
    for m in [&mut wax, &mut wick] {
        m.translate([0.0, c.base, 0.0]);
    }
    if !a.holder && a.burnt > 0.3 {
        // Wax pooled round the foot of a candle that has burnt a while.
        let mut pool = ellipsoid([c.radius * 1.7, c.radius * 0.18, c.radius * 1.5]);
        pool.translate([c.radius * 0.2, c.radius * 0.16, 0.0]);
        wax.append(&pool);
    }
    parts.add_look("candle", wax, Look::of(Bone).tinted(a.color));
    parts.add_look("wick", wick, Look::of(Cord).tinted(if c.burnt { CHAR } else { a.color }).without_outline());
    if a.holder {
        let r = c.radius;
        let sz = a.size;
        // A dish with a raised rim, a socket cup for the candle, and a finger ring.
        let dish = lathe(
            &spline_profile(&[[0.0, 0.0], [0.5 * sz, 0.0], [0.56 * sz, 0.04 * sz], [0.56 * sz, 0.1 * sz], [0.5 * sz, 0.08 * sz], [0.2 * sz, 0.06 * sz], [0.0, 0.06 * sz]], 20),
            &LatheOptions::new(32, Shading::SMOOTH),
        );
        let socket = lathe(
            &[[r * 1.35, 0.05 * sz], [r * 1.35, c.base + 0.12 * sz], [r * 1.5, c.base + 0.14 * sz], [r * 1.02, c.base + 0.14 * sz], [r * 1.02, c.base]],
            &LatheOptions { facets: 24, shading: Shading::BANDED, cap_start: false, cap_end: false, phase: 0.0 },
        );
        let mut ring = torus(0.13 * sz, 0.025 * sz, 24, 8, Shading::SMOOTH);
        ring.rotate_x(FRAC_PI_2).translate([0.62 * sz, 0.165 * sz, 0.0]);
        parts.add("holder", MeshBuilder::merged([&dish, &socket, &ring]), a.material);
    }
    if a.lit {
        add_flame(parts, [0.0, c.wick_top - c.radius * 0.15, 0.0], c.flame_height, c.radius * 0.45, 0.04, s.seed);
    }
}

/// Hemp rope laid in a coil: each turn crosses in and out over the last as the coil rises,
/// tied round in two places, with its end trailing off.
fn rope(s: &SundryState, parts: &mut Parts) {
    let a = &s.article;
    let sz = a.size;
    let (big, rr) = (0.62 * sz, 0.07 * sz);
    let turns = (a.count + 2) as f32;
    let n = (turns * 40.0) as usize;
    let at = |u: f32| {
        let th = TAU * u;
        let radial = big + rr * 1.1 * (PI * u).cos();
        [radial * th.cos(), rr * (1.0 + 1.6 * u / turns), -radial * th.sin()]
    };
    let mut path: Vec<V3> = (0..=n).map(|i| at(turns * i as f32 / n as f32)).collect();
    // The end leaves the coil along its last tangent and drops to the ground.
    let end = path[path.len() - 1];
    let dir = vec3::normalize_or(vec3::sub(end, path[path.len() - 2]), vec3::X);
    for k in 1..=8 {
        let t = k as f32 / 8.0;
        let p = vec3::add(end, vec3::scale(dir, big * 0.9 * t));
        path.push([p[0] + rr * 0.6 * (t * 5.0).sin(), rr + (end[1] - rr) * (1.0 - t).powi(2), p[2]]);
    }
    parts.add("rope", twisted_cord(&path, false, rr, 3), Cord);
    let mut ties = MeshBuilder::new();
    for angle in [0.9f32, 0.9 + PI] {
        let mut tie = torus(rr * 1.9, rr * 0.4, 24, 6, Shading::SMOOTH);
        tie.rotate_x(FRAC_PI_2).rotate_y(angle).translate([big * angle.cos(), rr * 1.8, -big * angle.sin()]);
        ties.append(&tie);
    }
    parts.add_look("ties", ties, Look::of(Cord).tinted(a.color.scaled(0.75)));
}

/// One pick, lying in the XY plane with its handle's foot at the origin: a flat handle, a
/// wire shank and a tip filed to `shape` (hook, rake, half diamond or tension wrench).
fn pick(shape: u32, length: f32, wire: f32) -> (MeshBuilder, MeshBuilder) {
    let hl = length * 0.38;
    let handle = bevelled_slab(&rounded_rect(0.0, hl * 0.5, wire * 5.0, hl, wire * 2.0), &[], &SlabOptions::rounded(wire * 1.6, wire * 0.5));
    let top = length;
    let mut path: Vec<V3> = vec![[0.0, hl * 0.9, 0.0], [0.0, top * 0.8, 0.0]];
    match shape % 4 {
        // A short hook.
        0 => path.extend([[0.0, top * 0.94, 0.0], [wire * 2.0, top, 0.0], [wire * 3.5, top * 0.97, 0.0]]),
        // A rake: three bumps.
        1 => {
            for k in 0..6 {
                let y = top * (0.82 + 0.03 * k as f32);
                path.push([if k % 2 == 0 { wire * 2.5 } else { 0.0 }, y, 0.0]);
            }
        }
        // A half diamond.
        2 => path.extend([[wire * 3.0, top * 0.9, 0.0], [0.0, top * 0.97, 0.0], [0.0, top, 0.0]]),
        // A tension wrench: an L bent at the top.
        _ => path.extend([[0.0, top * 0.9, 0.0], [wire * 1.5, top * 0.92, 0.0], [wire * 6.0, top * 0.92, 0.0]]),
    }
    let shank = tube(&path, 6, &SweepOptions::default(), |_| wire);
    (handle, shank)
}

/// Picks tucked in the pocket of an open leather roll, its tie trailing.
fn lockpicks(s: &SundryState, parts: &mut Parts) {
    let a = &s.article;
    let sz = a.size;
    let (w, h, t) = (1.3 * sz, 1.0 * sz, 0.03 * sz);
    let mut roll = bevelled_slab(&rounded_rect(0.0, 0.0, w, h, 0.08 * sz), &[], &SlabOptions::rounded(t, t * 0.4));
    let mut pocket = bevelled_slab(&rounded_rect(0.0, -h * 0.3, w * 0.96, h * 0.36, 0.05 * sz), &[], &SlabOptions::rounded(t, t * 0.4));
    pocket.translate([0.0, 0.0, t * 1.4]);
    let n = a.count.clamp(1, 8);
    let wire = 0.018 * sz;
    let (mut handles, mut shanks) = (MeshBuilder::new(), MeshBuilder::new());
    for k in 0..n {
        let x = w * 0.8 * ((k as f32 + 0.5) / n as f32 - 0.5);
        let len = h * (0.7 + 0.08 * ((s.seed + k) % 3) as f32);
        let (mut hd, mut sh) = pick(k + s.seed % 4, len, wire);
        for m in [&mut hd, &mut sh] {
            m.translate([x, -h * 0.42, t * 0.9]);
        }
        handles.append(&hd);
        shanks.append(&sh);
    }
    // The tie: a thong from the roll's edge, trailing loose.
    let path: Vec<V3> = (0..=10)
        .map(|i| {
            let k = i as f32 / 10.0;
            [w * 0.5 + w * 0.45 * k, h * 0.1 * (k * 7.0).sin(), t * 0.5]
        })
        .collect();
    let mut tie = thong(&path, false, 0.02 * sz);
    for m in [&mut roll, &mut pocket, &mut handles, &mut shanks, &mut tie] {
        m.rotate_x(-FRAC_PI_2).translate([0.0, t * 0.5, 0.0]);
    }
    let leather = Look::of(Leather).tinted(a.color);
    parts.add_look("roll", MeshBuilder::merged([&roll, &pocket]), leather);
    parts.add_look("tie", tie, leather);
    parts.add("handles", handles, if a.material == Silver { Bone } else { MaterialKind::Wood });
    parts.add_unoutlined("picks", shanks, a.material);
}

/// Keys fanned out from an iron ring, lying flat.
fn keys(s: &SundryState, parts: &mut Parts) {
    let a = &s.article;
    let sz = a.size;
    let ring_r = 0.3 * sz;
    let wire = 0.03 * sz;
    let mut ring = torus(ring_r, wire, 40, 8, Shading::SMOOTH);
    ring.translate([0.0, wire, 0.0]);
    parts.add("ring", ring, Iron);
    let n = a.count.clamp(1, 6);
    let mut rng = Mulberry32::new(s.seed.wrapping_mul(0x9e37_79b9).wrapping_add(17));
    let mut all = MeshBuilder::new();
    for k in 0..n {
        let len = sz * rng.range_f32(0.7, 1.1);
        let wards: Vec<f32> = (0..rng.int(1, 4)).map(|_| rng.range_f32(0.0, 1.0)).collect();
        let mut m = key(len, 1.0, a.bow, &wards);
        // Lay it flat, its length out along +Z, its bow hooked on the ring.
        let hook = len * 0.14 + len * 0.035;
        m.rotate_x(-FRAC_PI_2).translate([0.0, len * 0.035 * 0.8 + wire * 0.2, ring_r - hook]);
        let spread = if n == 1 { 0.0 } else { -0.9 + 1.8 * k as f32 / (n - 1) as f32 };
        m.rotate_y(spread + rng.range_f32(-0.1, 0.1));
        all.append(&m);
    }
    parts.add("keys", all, a.material);
}

/// A tin with a hinged lid, a C-shaped fire steel on top and a flint beside it.
fn tinderbox(s: &SundryState, parts: &mut Parts) {
    let a = &s.article;
    let sz = a.size;
    // An oblong tin with a hinged lid, or a round one with a push-on lid.
    let round = a.count >= 2;
    let (w, d, h) = if round { (0.75 * sz, 0.75 * sz, 0.28 * sz) } else { (0.9 * sz, 0.6 * sz, 0.3 * sz) };
    let corner = if round { w * 0.5 } else { 0.08 * sz };
    let lay = |m: &mut MeshBuilder, y: f32| {
        m.rotate_x(-FRAC_PI_2).translate([0.0, y, 0.0]);
    };
    let mut body = bevelled_slab(&rounded_rect(0.0, 0.0, w, d, corner), &[], &SlabOptions::rounded(h * 0.72, h * 0.06));
    lay(&mut body, h * 0.36);
    let mut lid = bevelled_slab(&rounded_rect(0.0, 0.0, w * 1.02, d * 1.02, corner * 1.02), &[], &SlabOptions::rounded(h * 0.3, h * 0.08));
    lay(&mut lid, h * 0.86);
    let mut hinge = cylinder(h * 0.07, w * 0.7, 10, Shading::SMOOTH);
    hinge.rotate_z(FRAC_PI_2).translate([w * 0.35, h * 0.72, d * 0.51]);
    let tin = if a.material == Iron { Look::of(Iron).tinted(Rgb::hex(0xb9bfc3)) } else { Look::of(a.material) };
    let hinge = if round { MeshBuilder::new() } else { hinge };
    parts.add_look("box", MeshBuilder::merged([&body, &lid, &hinge]), tin);

    // The fire steel: a flat C with curled ends, lying on the lid.
    let top = h * 1.01 + 0.025 * sz;
    let path: Vec<V3> = (0..=24)
        .map(|i| {
            let t = i as f32 / 24.0;
            let ang = PI * (0.15 + 1.7 * t);
            let (r, curl) = (0.22 * sz, if !(0.08..=0.92).contains(&t) { 0.6 } else { 1.0 });
            [w * 0.05 + ang.cos() * r * curl * 1.5, top, -ang.sin() * r * curl * 0.7]
        })
        .collect();
    let section: Vec<P2> = rounded_rect(0.0, 0.0, 0.05 * sz, 0.07 * sz, 0.012 * sz);
    let steel = crate::geom::sweep(&path, &section, &SweepOptions { up: Some(vec3::Y), ..Default::default() }, |_| 1.0);
    parts.add("steel", steel, Iron);
    let mut flint = icosphere(0.09 * sz, 1).flat_shaded();
    flint.scale([1.3, 0.6, 1.0]).translate([w * 0.62, 0.05 * sz, d * 0.15]);
    parts.add_look("flint", flint, Look::of(Stone).tinted(Rgb::hex(0x55524c)));
}

/// A blanket rolled and buckled round with two straps, a carrying thong between them.
fn bedroll(s: &SundryState, parts: &mut Parts) {
    let a = &s.article;
    let sz = a.size;
    let (r, len) = (0.9 * sz, 4.5 * sz);
    let centre = [0.0, r, 0.0];
    let cloth = Fabric { color: a.color, pattern: a.pattern, color2: a.color2, scale: 2.0 };
    let (roll, edge) = rolled_sheet(centre, r, len, 0.03 * sz, 3.5);
    parts.add_look("bedroll", roll, Look::cloth(&cloth));
    parts.add_tinted("bedroll_edge", edge, Cloth, a.color.scaled(0.7));
    let xs = [-0.3 * len, 0.3 * len];
    let (straps, buckles) = roll_straps(centre, r, &xs, 0.24 * sz, 0.035 * sz, 0.028 * sz);
    // The carrying thong arches over the top from strap to strap.
    let path: Vec<V3> = (0..=16)
        .map(|i| {
            let t = i as f32 / 16.0;
            [xs[0] + (xs[1] - xs[0]) * t, r * 2.0 + 0.02 * sz + r * 0.35 * (PI * t).sin(), -r * 0.2]
        })
        .collect();
    parts.add("straps", MeshBuilder::merged([&straps, &thong(&path, false, 0.035 * sz)]), Leather);
    parts.add("buckles", buckles, a.material);
}

/// How far a laid bedroll reaches from its centre along X and Z at its largest, whatever its
/// size or seed, in world units: the floor the game keeps clear to lay one.
pub const LAID_BEDROLL_HALF: (f32, f32) = (9.0, 3.6);

/// A bedroll unrolled on the floor: a canvas mat about 16 units long and 6 wide under a blanket
/// of the bedroll's own dye over its foot two thirds, with a rolled pillow across the head and
/// the strap lying loose at the foot. Lies along X, head toward -X, as the room's
/// `Scenery::Bedroll` does, under 1.3 units high.
fn laid_bedroll(s: &SundryState, parts: &mut Parts) {
    let a = &s.article;
    // A short folk's length to a tall one's; never past [`LAID_BEDROLL_HALF`].
    let k = (a.size / 1.2).clamp(0.85, 1.0);
    let (len, wide) = (16.0 * k, 6.0 * k);
    let mut mat = cuboid([len, 0.3, wide]);
    mat.translate([0.0, 0.15, 0.0]);
    let cover = len * 0.66;
    let foot = len * 0.5;
    // The blanket is a sheet draped over the foot two thirds of the mat: thick, rucked along its
    // length and drooping at the edges, so its weave runs along it as the folded one's does.
    let (rows, cols) = (6, 14);
    let covered = wide * 0.94;
    let head = foot - cover - 0.1 * k;
    let drape: Vec<Vec<V3>> = (0..=rows)
        .map(|r| {
            let across = r as f32 / rows as f32 * 2.0 - 1.0;
            (0..=cols)
                .map(|c| {
                    let along = c as f32 / cols as f32;
                    let ruck = 0.16 * (along * 7.0 + across * 1.6).sin() * (1.0 - 0.5 * across.abs());
                    [head + cover * along, 0.62 + ruck - 0.22 * across * across, across * covered * 0.5]
                })
                .collect()
        })
        .collect();
    let blanket = thick_sheet(&drape, &GridOptions::default(), 0.22);
    let mut fold = cuboid([1.3 * k, 0.55, covered]);
    fold.translate([head + 0.65 * k, 0.5, 0.0]);
    let r = 0.45 * k;
    let (mut pillow, mut ends) = rolled_sheet([0.0, 0.0, 0.0], r, 3.6 * k, 0.015, 3.0);
    for m in [&mut pillow, &mut ends] {
        m.rotate_y(FRAC_PI_2).translate([-foot + 1.6 * k, 0.3 + r, 0.0]);
    }
    parts.add_tinted("mat", mat, Cloth, Rgb::hex(0x6e6248));
    // Plain dye: a woven pattern is wrapped round a part (`fabric::Wrap`), which a flat sheet that
    // spans the middle of the item would show as a sunburst; the rolled bedroll keeps the weave.
    parts.add_look("blanket", blanket, Look::of(Cloth).tinted(a.color));
    parts.add_tinted("blanket_fold", fold, Cloth, a.color.scaled(0.8));
    parts.add_tinted("pillow", pillow, Cloth, Rgb::hex(0xb8ab88));
    parts.add_tinted("pillow_end", ends, Cloth, Rgb::hex(0x8f8466));
    // The strap, laid flat across the mat's foot.
    let mut strap = cuboid([0.5 * k, 0.1, wide * 0.9]);
    strap.translate([foot - 0.35 * k, 0.35, 0.0]);
    parts.add("straps", strap, Leather);
}

/// A roll of linen with its end unrolled along the ground.
fn bandages(s: &SundryState, parts: &mut Parts) {
    let a = &s.article;
    let sz = a.size;
    let (r, len) = (0.32 * sz, 0.55 * sz);
    let centre = [0.0, r, 0.0];
    let (roll, edge) = rolled_sheet(centre, r, len, 0.008 * sz, 5.0);
    let linen = Look::of(Cloth).tinted(a.color);
    parts.add_look("roll", roll, linen);
    parts.add_look("roll_edge", edge, Look::of(Cloth).tinted(a.color.scaled(0.82)).without_outline());
    // The loose end: from under the roll, forward along the ground.
    let t = 0.012 * sz;
    let path: Vec<V3> = (0..=14)
        .map(|i| {
            let k = i as f32 / 14.0;
            [0.0, t * 0.5 + 0.012 * sz * (k * 9.0).sin().abs() * k, -r * 0.2 + (r * 1.2 + sz * 0.9) * k]
        })
        .collect();
    let faces = vec![vec3::Y; path.len()];
    parts.add_look("strip", strap(&path, &faces, len * 0.96, t, false), linen);
}
