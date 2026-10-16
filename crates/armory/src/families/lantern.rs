//! Lanterns: a hooded body hung by a bail on a short chain from a carrying ring, lit by a
//! candle or an oil font inside. Horn-paned frames, pierced tin drums, and wire cages round a
//! glass chimney.
//!
//! The lantern is one of the bases the plate drew outside the Armory (`Lantern` in
//! `data/bases.json`, `weaponGroup()` in `reference/forever-below-plate.html`): a ring, a
//! stub of rod, a lathed hood, a glowing cylinder with four bars and a lathed base, with its
//! light at the cylinder's middle. The plate's `Tin`, `Brass` and `Iron` materials are
//! [`LanternMetal`]. The Armory prototype has no lantern, so the sliders here are new.
//!
//! Departures from the plate:
//! - Three bodies (horn panes in a frame, a pierced tin drum, a wire cage round a glass
//!   chimney), each under a hood and over a base, instead of one glowing cylinder.
//! - The bail is a real wire loop pivoting on the hood, hung on a cable chain of 0 to 6 links
//!   from the carrying ring, and inside stands a candle ([`crate::build::light::candle`]) or
//!   an oil font with a burner, wick and filler cap.
//! - Lit or not is state. A lit lantern's panes, pierced holes or chimney glow, and
//!   [`Lantern::light`] says where its light is and how far horn or tin lets it reach.
//! - There is no tin material: tin is iron tinted pale, and horn is bone tinted amber. A glass
//!   chimney is [`MaterialKind::Glass`], as in the food family.
//!
//! Item space: the carrying ring is at the origin, where the hand holds it, and the lantern
//! hangs below it, facing +Z.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use crate::build::chain::{cable_chain, jump_ring, link_pitch};
use crate::build::light::{
    CANDLE_LIGHT, CHAR, FLAME_LIGHT, HORN_LIGHT, LightAnchor, add_flame, candle, candle_wick_top, glowing,
};
use crate::build::parts::{Look, Parts};
use crate::cond::{Condition, ConditionLimits};
use crate::families::Generator;
use crate::families::food::stopper;
use crate::geom::{
    ItemMesh, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, cuboid, cylinder, lathe, spline_profile, torus, tube,
    uv_sphere, vec3,
};
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::slider::{Sliders, sliders};

use MaterialKind::{Bone, Brass, Cord, Glass, Iron};

/// The lantern generator.
pub struct Lantern;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanternStyle {
    /// Panes of scraped horn in a metal frame, under a pyramid hood.
    Horn,
    /// A drum of tin punched with a pattern of holes, under a pierced cone.
    Pierced,
    /// Wire guards round a glass chimney, between a base and a domed top.
    Cage,
}

impl LanternStyle {
    pub const ALL: [LanternStyle; 3] = [LanternStyle::Horn, LanternStyle::Pierced, LanternStyle::Cage];
}

/// The plate's lantern materials.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanternMetal {
    /// Tinned iron sheet.
    Tin,
    Brass,
    Iron,
}

impl LanternMetal {
    pub const ALL: [LanternMetal; 3] = [LanternMetal::Tin, LanternMetal::Brass, LanternMetal::Iron];

    /// How the metal is drawn.
    pub fn look(self) -> Look {
        match self {
            LanternMetal::Tin => Look::of(Iron).tinted(TIN),
            LanternMetal::Brass => Look::of(Brass),
            LanternMetal::Iron => Look::of(Iron),
        }
    }
}

/// Tinplate: paler and greyer than wrought iron.
const TIN: Rgb = Rgb::hex(0xb9bfc3);
/// Scraped horn.
const HORN: Rgb = Rgb::hex(0xd8b26a);
/// Chimney glass.
const GLASS: Rgb = Rgb::hex(0xe6e2cc);
/// Tallow candles.
const TALLOW: Rgb = Rgb::hex(0xe8dfc2);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanternFuel {
    Candle,
    /// An oil font with a burner and a flat wick (`Lamp oil` in `data/goods.json`).
    Oil,
}

impl LanternFuel {
    pub const ALL: [LanternFuel; 2] = [LanternFuel::Candle, LanternFuel::Oil];
}

/// The body: its frame, hood and base.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LanternBody {
    pub style: LanternStyle,
    pub metal: LanternMetal,
    /// Panes of a horn lantern, columns of the pierced pattern, or guards of a cage.
    pub sides: u32,
    /// Height of the body between hood and base.
    pub height: f32,
    /// Radius of the body.
    pub width: f32,
    /// Height of the hood, as a share of the width.
    pub hood: f32,
    /// Small ball feet under the base.
    pub feet: bool,
}

sliders! {
    LanternBodyLimits for LanternBody {
        sides: 4.0, 16.0, 1.0;
        height: 1.0, 1.9, 0.01;
        width: 0.3, 0.55, 0.005;
        hood: 0.2, 1.0, 0.01;
    }
}

impl Default for LanternBody {
    /// The plate's lantern: a body 1.0 tall and 0.36 wide under a brass hood.
    fn default() -> Self {
        LanternBody { style: LanternStyle::Horn, metal: LanternMetal::Brass, sides: 4, height: 1.1, width: 0.4, hood: 0.5, feet: false }
    }
}

/// The bail, its chain and the carrying ring.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bail {
    /// Chain links between the ring and the bail; 0 hooks the bail straight on the ring.
    pub links: u32,
    /// Length of each link.
    pub link: f32,
    /// Radius of the carrying ring.
    pub ring: f32,
}

sliders! {
    BailLimits for Bail {
        links: 0.0, 6.0, 1.0;
        link: 0.1, 0.24, 0.002;
        ring: 0.12, 0.26, 0.002;
    }
}

impl Default for Bail {
    fn default() -> Self {
        Bail { links: 2, link: 0.16, ring: 0.18 }
    }
}

/// What burns inside, and whether it is alight.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LanternFlame {
    pub fuel: LanternFuel,
    pub lit: bool,
    /// How much candle is left, or how full the font is, 0..1.
    pub level: f32,
    /// Flame height, as a multiplier.
    pub flame: f32,
    /// Burning time left, in game seconds ([`crate::kindle::LANTERN_SECONDS`] when full).
    /// Runtime state: it burns down while the lantern is lit, and `level` follows it.
    #[serde(default = "full_lantern")]
    pub fuel_left: f32,
}

fn full_lantern() -> f32 {
    crate::kindle::LANTERN_SECONDS
}

sliders! {
    LanternFlameLimits for LanternFlame {
        level: 0.1, 1.0, 0.01;
        flame: 0.6, 1.3, 0.01;
    }
}

impl Default for LanternFlame {
    /// The plate's lantern: alight.
    fn default() -> Self {
        LanternFlame { fuel: LanternFuel::Candle, lit: true, level: 0.8, flame: 1.0, fuel_left: crate::kindle::LANTERN_SECONDS * 0.8 }
    }
}

/// Every parameter of one lantern.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LanternState {
    pub seed: u32,
    pub body: LanternBody,
    pub bail: Bail,
    pub flame: LanternFlame,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every lantern section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LanternLimits {
    pub body: LanternBodyLimits,
    pub bail: BailLimits,
    pub flame: LanternFlameLimits,
    pub cond: ConditionLimits,
}

/// The lantern's slider ranges (new: the prototypes have none).
pub const LIMITS: LanternLimits = LanternLimits {
    body: LanternBodyLimits::SLIDERS,
    bail: BailLimits::SLIDERS,
    flame: LanternFlameLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl LanternState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &LanternLimits) {
        self.body.fit(&limits.body);
        self.bail.fit(&limits.bail);
        self.flame.fit(&limits.flame);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &LanternLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("body", self.body.out_of_range(&limits.body)),
            tag("bail", self.bail.out_of_range(&limits.bail)),
            tag("flame", self.flame.out_of_range(&limits.flame)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

/// The lantern's light in feet (`light` of `Lantern` in `data/bases.json`): the SRD's hooded
/// lantern, 30 ft of bright light
/// (https://www.dndbeyond.com/sources/dnd/free-rules/equipment#AdventuringGear).
pub const LANTERN_FEET: f32 = 30.0;

impl Generator for Lantern {
    type State = LanternState;

    fn roll(seed: u32) -> LanternState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&[(LanternStyle::Horn, 4), (LanternStyle::Pierced, 3), (LanternStyle::Cage, 2)]);
        compose(&mut rng, seed, style)
    }

    fn noun(s: &LanternState) -> String {
        match (s.body.style, s.body.metal) {
            (LanternStyle::Horn, _) => "Horn Lantern",
            (LanternStyle::Pierced, LanternMetal::Tin) => "Pierced Tin Lantern",
            (LanternStyle::Pierced, _) => "Pierced Lantern",
            (LanternStyle::Cage, _) if s.flame.fuel == LanternFuel::Oil => "Oil Lantern",
            (LanternStyle::Cage, _) => "Cage Lantern",
        }
        .to_string()
    }

    fn build(s: &LanternState) -> ItemMesh {
        build_lantern(s)
    }
}

impl Lantern {
    /// "Forge new" forced to one body style, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, style)` when `style` is what
    /// `s` rolls.
    pub fn roll_as(seed: u32, style: LanternStyle) -> LanternState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, style)
    }

    /// Where the lantern gives its light, while lit: the heart of the flame, reaching about the
    /// SRD's hooded lantern's 30 ft of bright light ([`LANTERN_FEET`]). Horn panes warm it, a
    /// pierced drum lets out less, an open cage most; an oil flame reaches farther than a
    /// candle, and a bigger flame a little farther. The shares are our own, kept narrow enough
    /// that the weakest lantern, pierced with a candle, still reaches farther than any torch
    /// ([`crate::families::torch::Torch::light`]). `None` when the lantern is out.
    pub fn light(s: &LanternState) -> Option<LightAnchor> {
        if !s.flame.lit {
            return None;
        }
        let g = Layout::of(s);
        let (color, share) = match s.body.style {
            LanternStyle::Horn => (HORN_LIGHT, 1.0),
            LanternStyle::Pierced => (CANDLE_LIGHT, 0.92),
            LanternStyle::Cage => (if s.flame.fuel == LanternFuel::Oil { FLAME_LIGHT } else { CANDLE_LIGHT }, 1.1),
        };
        let fuel = if s.flame.fuel == LanternFuel::Oil { 1.0 } else { 0.92 };
        let feet = LANTERN_FEET * share * fuel * (0.9 + 0.1 * s.flame.flame);
        let at = vec3::add(g.flame_base, [0.0, g.flame_height * 0.4, 0.0]);
        Some(LightAnchor::feet(at, color, feet, 0.25))
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, style: LanternStyle) -> LanternState {
    use LanternStyle as S;
    let mut s = LanternState { seed, ..LanternState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    let b = &mut s.body;
    b.style = style;
    b.metal = *rng.weighted(match style {
        S::Horn => &[(LanternMetal::Tin, 2), (LanternMetal::Brass, 2), (LanternMetal::Iron, 2)][..],
        S::Pierced => &[(LanternMetal::Tin, 4), (LanternMetal::Brass, 1), (LanternMetal::Iron, 1)],
        S::Cage => &[(LanternMetal::Iron, 2), (LanternMetal::Brass, 2), (LanternMetal::Tin, 1)],
    });
    b.sides = match style {
        S::Horn => *rng.weighted(&[(4, 3), (6, 2), (8, 1)]),
        S::Pierced => rng.int(8, 16),
        S::Cage => rng.int(4, 8),
    };
    b.height = match style {
        S::Horn => rng.range_f32(1.1, 1.7),
        S::Pierced => rng.range_f32(1.2, 1.8),
        S::Cage => rng.range_f32(1.0, 1.5),
    };
    b.width = match style {
        S::Horn => rng.range_f32(0.36, 0.5),
        S::Pierced => rng.range_f32(0.32, 0.45),
        S::Cage => rng.range_f32(0.34, 0.48),
    };
    b.hood = match style {
        S::Horn => rng.range_f32(0.4, 0.9),
        S::Pierced => rng.range_f32(0.6, 1.0),
        S::Cage => rng.range_f32(0.2, 0.45),
    };
    b.feet = rng.chance(0.35);

    let bl = &mut s.bail;
    bl.links = match style {
        S::Cage => rng.int(0, 3),
        _ => rng.int(0, 5),
    };
    bl.link = rng.range_f32(0.12, 0.2);
    bl.ring = rng.range_f32(0.14, 0.22);

    let f = &mut s.flame;
    f.fuel = *rng.weighted(match style {
        S::Cage => &[(LanternFuel::Oil, 3), (LanternFuel::Candle, 1)][..],
        S::Horn => &[(LanternFuel::Candle, 3), (LanternFuel::Oil, 1)],
        S::Pierced => &[(LanternFuel::Candle, 4), (LanternFuel::Oil, 1)],
    });
    f.lit = rng.chance(0.55);
    f.level = rng.range_f32(0.3, 1.0);
    f.flame = rng.range_f32(0.75, 1.25);
    s.fit(&LIMITS);
    // The font is as full as the fuel it holds.
    s.flame.fuel_left = crate::kindle::LANTERN_SECONDS * s.flame.level;
    s
}

// ───────────────────────── Geometry ─────────────────────────

/// Heights down the lantern from the ring at the origin, shared by the build and
/// [`Lantern::light`].
struct Layout {
    wire: f32,
    /// Where the bottom of the chain (or the ring) catches the bail.
    bail_top: f32,
    /// The hood's apex, under its chimney cap.
    hood_apex: f32,
    /// Top and bottom of the body.
    top: f32,
    bottom: f32,
    /// What stands inside: the candle's or font's base.
    floor: f32,
    flame_base: V3,
    flame_height: f32,
}

impl Layout {
    fn of(s: &LanternState) -> Layout {
        let (b, bl, f) = (&s.body, &s.bail, &s.flame);
        let wire = (b.width * 0.07).max(0.02);
        let chain_wire = bl.link * 0.11;
        let ring_bottom = -bl.ring + chain_wire;
        let chain = if bl.links == 0 { 0.0 } else { link_pitch(bl.link, chain_wire) * (bl.links as f32 - 1.0) + bl.link - chain_wire * 2.0 };
        let bail_top = ring_bottom - chain;
        let w = b.width;
        let cap = w * 0.22;
        let hood_h = w * b.hood;
        let hood_apex = bail_top - w * 0.35 - cap;
        let top = hood_apex - hood_h;
        let bottom = top - b.height;
        let floor = bottom + w * 0.06;
        let (base_y, flame_h) = match f.fuel {
            LanternFuel::Candle => {
                let (r, h) = candle_size(s);
                (floor + w * 0.08 + candle_wick_top(r, h, f.lit || f.level < 0.95), b.height * 0.2)
            }
            LanternFuel::Oil => (floor + oil_height(s) + w * 0.12, b.height * 0.22),
        };
        Layout {
            wire,
            bail_top,
            hood_apex,
            top,
            bottom,
            floor,
            flame_base: [0.0, base_y - flame_h * 0.06, 0.0],
            flame_height: flame_h * f.flame,
        }
    }
}

/// Radius and height of the candle inside.
fn candle_size(s: &LanternState) -> (f32, f32) {
    (s.body.width * 0.24, s.body.height * (0.12 + 0.4 * s.flame.level))
}

/// Height of the oil font, burner and all.
fn oil_height(s: &LanternState) -> f32 {
    s.body.height * 0.24
}

fn build_lantern(s: &LanternState) -> ItemMesh {
    let g = Layout::of(s);
    let b = &s.body;
    let metal = b.metal.look();
    let lit = s.flame.lit;
    let mut parts = Parts::new();
    let mut frame = MeshBuilder::new();

    hanging(s, &g, &mut parts, metal);
    match b.style {
        LanternStyle::Horn => horn_body(s, &g, &mut frame, &mut parts),
        LanternStyle::Pierced => pierced_body(s, &g, &mut frame, &mut parts),
        LanternStyle::Cage => cage_body(s, &g, &mut frame, &mut parts),
    }
    base(s, &g, &mut frame);
    parts.add_look("frame", frame, metal);
    fuel(s, &g, &mut parts);
    if lit {
        add_flame(&mut parts, g.flame_base, g.flame_height, s.body.width * 0.13 * s.flame.flame.sqrt(), 0.05, s.seed);
    }
    parts.finish()
}

/// The carrying ring, the chain, the bail and its pivots.
fn hanging(s: &LanternState, g: &Layout, parts: &mut Parts, metal: Look) {
    let bl = &s.bail;
    let chain_wire = bl.link * 0.11;
    let mut ring = jump_ring(bl.ring, chain_wire * 1.3);
    ring.rotate_y(FRAC_PI_2);
    parts.add_look("ring", ring, metal);
    if bl.links > 0 {
        let start = -bl.ring + chain_wire - bl.link * 0.5 + chain_wire;
        let pitch = link_pitch(bl.link, chain_wire);
        let path: Vec<V3> = (0..bl.links).map(|k| [0.0, start - pitch * k as f32, 0.0]).collect();
        let path = if path.len() == 1 { vec![path[0], [0.0, path[0][1] - 1e-3, 0.0]] } else { path };
        parts.add_look("chain", cable_chain(&path, false, bl.link, chain_wire, bl.links as usize), metal.without_outline());
    }
    // The bail: a wire arch from one side of the hood, up through the chain, to the other.
    let w = s.body.width;
    let span = w * 1.04 + g.wire;
    let pivot = g.top + w * 0.06;
    let rise = g.bail_top - pivot;
    let path: Vec<V3> = (0..=32)
        .map(|i| {
            let a = PI * i as f32 / 32.0;
            // A rounded arch: steep sides, a round crown where the chain hooks on.
            let y = pivot + rise * a.sin().max(0.0).powf(0.45);
            [span * a.cos(), y, 0.0]
        })
        .collect();
    let mut bail = tube(&path, 8, &SweepOptions::default(), |_| g.wire * 0.55);
    for side in [-1.0f32, 1.0] {
        let mut boss = uv_sphere(g.wire * 1.3, 12, 8);
        boss.translate([side * span, pivot, 0.0]);
        bail.append(&boss);
    }
    parts.add_look("bail", bail, metal);
}

/// A hood from the body's top up to its apex, with a vented chimney cap: `facets` sides,
/// turned so flat sides face front when `facets` is small.
fn hood(s: &LanternState, g: &Layout, facets: usize, shading: Shading, frame: &mut MeshBuilder) {
    let w = s.body.width;
    let (top, apex) = (g.top, g.hood_apex);
    let h = apex - top;
    let profile = [
        [0.0, top - w * 0.02],
        [w * 1.08, top - w * 0.02],
        [w * 1.1, top + w * 0.03],
        [w * 0.98, top + h * 0.12],
        [w * 0.45, top + h * 0.85],
        [w * 0.24, apex],
        [0.0, apex],
    ];
    let phase = if facets <= 8 { PI / facets as f32 } else { 0.0 };
    frame.append(&lathe(&profile, &LatheOptions { facets, shading, phase, ..Default::default() }));
    // The chimney: a short vented collar and a cap over it.
    let cap = w * 0.22;
    let mut collar = cylinder(w * 0.17, cap * 0.7, 12, Shading::SMOOTH);
    collar.translate([0.0, apex - cap * 0.05, 0.0]);
    frame.append(&collar);
    let mut lid = lathe(
        &spline_profile(&[[0.0, 0.0], [w * 0.28, 0.0], [w * 0.26, cap * 0.18], [w * 0.12, cap * 0.38], [0.0, cap * 0.42]], 12),
        &LatheOptions::new(16, Shading::SMOOTH),
    );
    lid.translate([0.0, apex + cap * 0.6, 0.0]);
    frame.append(&lid);
}

/// The base: a dished plinth under the body, and its feet.
fn base(s: &LanternState, g: &Layout, frame: &mut MeshBuilder) {
    let w = s.body.width;
    let y = g.bottom;
    let facets = if s.body.style == LanternStyle::Horn { s.body.sides as usize } else { 28 };
    let shading = if s.body.style == LanternStyle::Horn { Shading::FLAT } else { Shading::BANDED };
    let phase = if facets <= 8 { PI / facets as f32 } else { 0.0 };
    let profile = [
        [0.0, y - w * 0.16],
        [w * 1.0, y - w * 0.16],
        [w * 1.1, y - w * 0.1],
        [w * 1.1, y - w * 0.04],
        [w * 1.02, y + w * 0.02],
        [w * 0.98, y + w * 0.06],
        [0.0, y + w * 0.06],
    ];
    frame.append(&lathe(&profile, &LatheOptions { facets, shading, phase, ..Default::default() }));
    if s.body.feet {
        for k in 0..3 {
            let a = TAU * k as f32 / 3.0 + PI / 2.0;
            let mut foot = uv_sphere(w * 0.12, 12, 8);
            foot.scale([1.0, 0.7, 1.0]).translate([a.cos() * w * 0.75, y - w * 0.2, -a.sin() * w * 0.75]);
            frame.append(&foot);
        }
    }
}

/// Corner `k` of a body of `sides` sides and circumradius `r`, flat side to the front.
fn corner(sides: u32, k: u32, r: f32) -> (f32, f32) {
    let a = TAU * k as f32 / sides as f32 + PI / sides as f32 - FRAC_PI_2;
    (a.cos() * r, -a.sin() * r)
}

/// Horn panes in a frame of corner posts and rails, under a pyramid hood.
fn horn_body(s: &LanternState, g: &Layout, frame: &mut MeshBuilder, parts: &mut Parts) {
    let b = &s.body;
    let n = b.sides.clamp(4, 8);
    let r = b.width / (PI / n as f32).cos();
    let post = g.wire * 0.9;
    for k in 0..n {
        let (x, z) = corner(n, k, r);
        let mut p = cuboid([post * 2.0, b.height, post * 2.0]);
        p.rotate_y(-(TAU * k as f32 / n as f32 + PI / n as f32 - FRAC_PI_2)).translate([x, g.bottom + b.height * 0.5, z]);
        frame.append(&p);
    }
    for y in [g.bottom + post, g.top - post, g.bottom + b.height * 0.5] {
        let rail: Vec<V3> = (0..n).map(|k| corner(n, k, r)).map(|(x, z)| [x, y, z]).collect();
        let mut rails = MeshBuilder::new();
        for k in 0..n as usize {
            let (a, c) = (rail[k], rail[(k + 1) % n as usize]);
            let len = vec3::distance(a, c);
            let mut bar = cuboid([len, post * 1.6, post * 1.4]);
            let mid = vec3::lerp(a, c, 0.5);
            bar.rotate_y(-(c[2] - a[2]).atan2(c[0] - a[0])).translate(mid);
            rails.append(&bar);
        }
        frame.append(&rails);
    }
    // The panes, a little inside the posts.
    let mut panes = MeshBuilder::new();
    let side = 2.0 * r * (PI / n as f32).sin();
    for k in 0..n {
        let (a, c) = (corner(n, k, r * 0.97), corner(n, k + 1, r * 0.97));
        let mut pane = cuboid([side * 0.96, b.height - post * 2.0, post * 0.5]);
        pane.rotate_y(-(c.1 - a.1).atan2(c.0 - a.0)).translate([(a.0 + c.0) * 0.5, g.bottom + b.height * 0.5, (a.1 + c.1) * 0.5]);
        panes.append(&pane);
    }
    let look = if s.flame.lit { glowing(HORN, 0.55) } else { Look::of(Bone).tinted(HORN) };
    parts.add_look("panes", panes, look);
    hood(s, g, n as usize, Shading::FLAT, frame);
}

/// A drum of sheet metal punched with rows of holes, under a pierced cone.
fn pierced_body(s: &LanternState, g: &Layout, frame: &mut MeshBuilder, parts: &mut Parts) {
    let b = &s.body;
    let w = b.width;
    // A sheet with a real thickness: up the outside, down the inside.
    let t = w * 0.025;
    let profile = [[w, g.bottom], [w, g.top], [w - t, g.top], [w - t, g.bottom], [w, g.bottom]];
    frame.append(&lathe(&profile, &LatheOptions { facets: 40, shading: Shading::BANDED, cap_start: false, cap_end: false, phase: 0.0 }));
    for y in [g.bottom + w * 0.04, g.top - w * 0.04] {
        let mut bead = torus(w * 1.01, w * 0.035, 40, 6, Shading::SMOOTH);
        bead.translate([0.0, y, 0.0]);
        frame.append(&bead);
    }
    // The pattern: columns of holes in staggered rows, thinned to a lozenge motif.
    let cols = b.sides.clamp(8, 16);
    let hole = w * 0.045;
    let rows = ((b.height - w * 0.3) / (hole * 3.2)).floor().max(3.0) as u32;
    let motif = s.seed % 3;
    let mut holes = MeshBuilder::new();
    let dot = {
        let mut d = cylinder(hole, w * 0.02, 6, Shading::SMOOTH);
        d.translate([0.0, -w * 0.01, 0.0]);
        d
    };
    for row in 0..rows {
        let y = g.bottom + w * 0.15 + (b.height - w * 0.3) * (row as f32 + 0.5) / rows as f32;
        for col in 0..cols * 2 {
            if (row + col) % 2 == 1 {
                continue;
            }
            let keep = match motif {
                0 => (row as i32 - rows as i32 / 2).unsigned_abs() + (col % 4).min(4 - col % 4) <= rows / 2,
                1 => col % 4 != 2,
                _ => true,
            };
            if !keep {
                continue;
            }
            let a = PI * col as f32 / cols as f32;
            let dir = [a.cos(), 0.0, -a.sin()];
            let mut d = dot.clone();
            d.place(vec3::add([0.0, y, 0.0], vec3::scale(dir, w * 1.0)), dir);
            holes.append(&d);
        }
    }
    let look = if s.flame.lit { glowing(Rgb::hex(0xffcf73), 0.9) } else { Look::of(Iron).tinted(CHAR).without_outline() };
    parts.add_look("holes", holes, look);
    hood(s, g, 32, Shading::SMOOTH, frame);
}

/// Wire guards round a glass chimney, under a low domed top.
fn cage_body(s: &LanternState, g: &Layout, frame: &mut MeshBuilder, parts: &mut Parts) {
    let b = &s.body;
    let w = b.width;
    let (y0, y1) = (g.bottom, g.top);
    // The chimney: a bulging glass globe from base to top.
    let h = y1 - y0;
    let profile = spline_profile(
        &[[w * 0.5, y0 + w * 0.04], [w * 0.62, y0 + h * 0.18], [w * 0.8, y0 + h * 0.5], [w * 0.62, y0 + h * 0.85], [w * 0.5, y1 - w * 0.02]],
        32,
    );
    // Glass with a thickness: up the outside, back down the inside.
    let mut shell = profile.clone();
    shell.extend(profile.iter().rev().map(|&[r, y]| [r - w * 0.03, y]));
    shell.push(profile[0]);
    let glass = lathe(&shell, &LatheOptions { facets: 32, shading: Shading::SMOOTH, cap_start: false, cap_end: false, phase: 0.0 });
    // Lit, the chimney glows with the flame behind it; glass either way, so it keeps its ink.
    let look = if s.flame.lit { Look { glow: 0.45, ..Look::of(Glass).tinted(Rgb::hex(0xffe6b0)) } } else { Look::of(Glass).tinted(GLASS) };
    parts.add_look("glass", glass, look);
    // Guards bowed out round the globe.
    let n = b.sides.clamp(4, 8);
    for k in 0..n {
        let a = TAU * k as f32 / n as f32 + PI / n as f32;
        let (sa, ca) = a.sin_cos();
        let path: Vec<V3> = (0..=16)
            .map(|i| {
                let t = i as f32 / 16.0;
                let r = w * (0.98 + 0.06 * (PI * t).sin());
                [ca * r, y0 + h * t, -sa * r]
            })
            .collect();
        frame.append(&tube(&path, 6, &SweepOptions::default(), |_| g.wire * 0.5));
    }
    let mut hoop = torus(w * 1.04, g.wire * 0.5, 40, 6, Shading::SMOOTH);
    hoop.translate([0.0, y0 + h * 0.5, 0.0]);
    frame.append(&hoop);
    hood(s, g, 32, Shading::SMOOTH, frame);
}

/// The candle or oil font standing inside.
fn fuel(s: &LanternState, g: &Layout, parts: &mut Parts) {
    let w = s.body.width;
    let f = &s.flame;
    match f.fuel {
        LanternFuel::Candle => {
            // A socket cup on the floor, and the candle in it.
            let (r, h) = candle_size(s);
            let burnt = f.lit || f.level < 0.95;
            let socket = lathe(
                &[[0.0, g.floor], [r * 1.5, g.floor], [r * 1.5, g.floor + w * 0.03], [r * 1.12, g.floor + w * 0.05], [r * 1.12, g.floor + w * 0.14], [r * 0.9, g.floor + w * 0.14]],
                &LatheOptions { facets: 20, shading: Shading::BANDED, cap_end: false, ..Default::default() },
            );
            parts.add_look("socket", socket, s.body.metal.look());
            let (mut wax, mut wick) = candle(r, h, (s.seed % 3) + 1, burnt, s.seed);
            for m in [&mut wax, &mut wick] {
                m.translate([0.0, g.floor + w * 0.08, 0.0]);
            }
            parts.add_look("candle", wax, Look::of(Bone).tinted(TALLOW));
            parts.add_look("wick", wick, Look::of(Cord).tinted(if burnt { CHAR } else { TALLOW }).without_outline());
        }
        LanternFuel::Oil => {
            // A squat font, a burner collar and a flat wick, and a filler cap off to one side.
            let fh = oil_height(s);
            let fr = w * 0.6;
            let y = g.floor;
            let font = lathe(
                &spline_profile(&[[0.0, y], [fr * 0.9, y], [fr, y + fh * 0.25], [fr * 0.9, y + fh * 0.65], [fr * 0.35, y + fh * 0.85], [fr * 0.3, y + fh]], 24),
                &LatheOptions::new(28, Shading::SMOOTH),
            );
            let mut burner = cylinder(fr * 0.3, w * 0.1, 14, Shading::SMOOTH);
            burner.translate([0.0, y + fh, 0.0]);
            let mut cap = stopper(fr * 0.12, fr * 0.14, fh * 0.2, y + fh * 0.72);
            cap.translate([fr * 0.55, 0.0, 0.0]);
            let font = MeshBuilder::merged([&font, &burner, &cap]);
            parts.add_look("font", font, s.body.metal.look());
            let mut wick = cuboid([fr * 0.28, w * 0.07, w * 0.03]);
            wick.translate([0.0, y + fh + w * 0.13, 0.0]);
            let burnt = f.lit || f.level < 0.95;
            parts.add_look("wick", wick, Look::of(Cord).tinted(if burnt { CHAR } else { TALLOW }).without_outline());
        }
    }
}
