//! Torches: a wooden handle with a head that burns. A rag wrapped round the handle and
//! soaked in pitch or tallow, a bundle of resinous splints bound together, or an iron cresset
//! cage packed with pitch-soaked tow. Each can be fresh, burning, doused or burnt out.
//!
//! The torch is one of the bases the plate drew outside the Armory (`Torch` and `Spare torch`
//! in `data/bases.json` and `data/goods.json`, `weaponGroup()` in
//! `reference/forever-below-plate.html`): a lathed handle, a lathed wrap and a two-layer flame
//! with a light at its heart. The Armory prototype has no torch, so the sliders and ranges
//! here are new.
//!
//! Departures from the plate:
//! - Three heads instead of one, and the plate's `Pitch` and `Tallow` materials are the
//!   head's [`Fuel`], with resin for splint bundles.
//! - Burning is state ([`TorchBurn`]): how charred the head is, whether it is alight, and how
//!   tall the flame stands. A burning head shows a glowing ring of embers where the char
//!   meets the fuel; a spent torch is a charred stub.
//! - The flame is mesh with a glow ([`crate::build::light::add_flame`]), not an unlit
//!   material and a sprite; the halo is the game's. [`Torch::light`] says where the light is.
//!
//! Item space: the hand is at the origin, the handle runs up +Y with its butt below, and the
//! head and flame stand above it.

use std::f32::consts::{PI, TAU};

use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use crate::build::chain::{cord, thong};
use crate::build::hafted::smoothstep;
use crate::build::light::{CHAR, EMBER, FLAME_LIGHT, LightAnchor, add_flame_leaning, ember_ring, spine_offset, glowing};
use crate::build::parts::{Look, Parts};
use crate::build::strap::strap;
use crate::cond::{Condition, ConditionLimits};
use crate::families::Generator;
use crate::geom::{
    ItemMesh, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, frustum, lathe, lathe_warped, regular_polygon,
    spline_profile, sweep, torus, tube, value_noise3, vec3,
};
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::slider::{Sliders, sliders};

use MaterialKind::{Cloth, Cord, Iron, Leather, Wood};

/// The torch generator.
pub struct Torch;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TorchStyle {
    /// A rag wound round the handle's end and soaked in fuel.
    Rag,
    /// A bundle of split, resinous sticks bound round the handle.
    Bundle,
    /// An iron cage on a socket, packed with fuel-soaked tow.
    Cresset,
}

impl TorchStyle {
    pub const ALL: [TorchStyle; 3] = [TorchStyle::Rag, TorchStyle::Bundle, TorchStyle::Cresset];
}

/// What burns: the plate's torch materials, plus resin for splint bundles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fuel {
    Pitch,
    Tallow,
    /// Resin-rich pine (fatwood): the splints are their own fuel.
    Resin,
}

impl Fuel {
    pub const ALL: [Fuel; 3] = [Fuel::Pitch, Fuel::Tallow, Fuel::Resin];

    /// The colour of a head soaked in it.
    pub fn color(self) -> Rgb {
        match self {
            Fuel::Pitch => Rgb::hex(0x2e241c),
            Fuel::Tallow => Rgb::hex(0xd9c79a),
            Fuel::Resin => Rgb::hex(0xa8783f),
        }
    }
}

/// How the handle is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandleGrip {
    Bare,
    /// A leather thong wound round the grip.
    Leather,
    /// Twisted cord wound round the grip.
    Cord,
}

impl HandleGrip {
    pub const ALL: [HandleGrip; 3] = [HandleGrip::Bare, HandleGrip::Leather, HandleGrip::Cord];
}

/// Where a torch is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BurnState {
    /// Never lit.
    Fresh,
    Burning,
    /// Lit once and put out: charred, cold.
    Doused,
    /// Burnt down to a charred stub.
    Spent,
}

impl BurnState {
    pub const ALL: [BurnState; 4] = [BurnState::Fresh, BurnState::Burning, BurnState::Doused, BurnState::Spent];
}

/// The handle (a stick, a branch or a turned haft).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TorchHandle {
    /// Butt to head.
    pub length: f32,
    /// Radius under the head.
    pub radius: f32,
    /// How much thinner the butt is than the head end.
    pub taper: f32,
    /// How much the stick bows (a cut branch rather than a turned haft).
    pub crook: f32,
    /// Sides round the handle; eight or fewer reads as whittled.
    pub facets: u32,
    pub grip: HandleGrip,
    /// An iron collar under the head (always on a cresset, as its socket).
    pub ferrule: bool,
}

sliders! {
    TorchHandleLimits for TorchHandle {
        length: 2.4, 4.8, 0.01;
        radius: 0.07, 0.16, 0.002;
        taper: 0.0, 0.5, 0.01;
        crook: 0.0, 1.0, 0.01;
        facets: 5.0, 16.0, 1.0;
    }
}

impl Default for TorchHandle {
    /// The plate's torch: a handle about 3.8 units long, 0.1 to 0.15 thick.
    fn default() -> Self {
        TorchHandle { length: 3.8, radius: 0.13, taper: 0.25, crook: 0.0, facets: 12, grip: HandleGrip::Bare, ferrule: false }
    }
}

/// The head that burns.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TorchHead {
    pub style: TorchStyle,
    pub fuel: Fuel,
    pub length: f32,
    /// Radius at its widest.
    pub girth: f32,
    /// Turns of rag, splints in a bundle, or bars in a cresset.
    pub layers: u32,
    /// Bindings round a rag or a bundle.
    pub bindings: u32,
    /// What the bindings are (cord, leather or iron wire).
    pub binding: MaterialKind,
}

sliders! {
    TorchHeadLimits for TorchHead {
        length: 0.6, 1.6, 0.01;
        girth: 0.14, 0.36, 0.002;
        layers: 2.0, 12.0, 1.0;
        bindings: 0.0, 4.0, 1.0;
    }
}

impl Default for TorchHead {
    /// The plate's torch: a pitch-dark wrap about 0.9 long and 0.2 thick.
    fn default() -> Self {
        TorchHead { style: TorchStyle::Rag, fuel: Fuel::Pitch, length: 0.9, girth: 0.21, layers: 4, bindings: 0, binding: Cord }
    }
}

/// How far the torch has burnt.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TorchBurn {
    pub state: BurnState,
    /// How much of the head is charred, 0 (fresh) to 1 (burnt out).
    pub char: f32,
    /// Flame height, as a multiplier.
    pub flame: f32,
    /// Burning time left, in game seconds ([`crate::kindle::TORCH_SECONDS`] when full). Runtime
    /// state: it burns down while the torch is alight, and `char` follows it.
    #[serde(default = "full_torch")]
    pub fuel_left: f32,
}

fn full_torch() -> f32 {
    crate::kindle::TORCH_SECONDS
}

sliders! {
    TorchBurnLimits for TorchBurn {
        char: 0.0, 1.0, 0.01;
        flame: 0.5, 1.5, 0.01;
    }
}

impl Default for TorchBurn {
    /// The plate's torch: alight.
    fn default() -> Self {
        TorchBurn { state: BurnState::Burning, char: 0.3, flame: 1.0, fuel_left: crate::kindle::TORCH_SECONDS * 0.7 }
    }
}

/// Every parameter of one torch.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TorchState {
    pub seed: u32,
    pub handle: TorchHandle,
    pub head: TorchHead,
    pub burn: TorchBurn,
    pub cond: Condition,
    pub palette: Palette,
}


/// Slider limits for every torch section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TorchLimits {
    pub handle: TorchHandleLimits,
    pub head: TorchHeadLimits,
    pub burn: TorchBurnLimits,
    pub cond: ConditionLimits,
}

/// The torch's slider ranges (new: the prototypes have none).
pub const LIMITS: TorchLimits = TorchLimits {
    handle: TorchHandleLimits::SLIDERS,
    head: TorchHeadLimits::SLIDERS,
    burn: TorchBurnLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl TorchState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &TorchLimits) {
        self.handle.fit(&limits.handle);
        self.head.fit(&limits.head);
        self.burn.fit(&limits.burn);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &TorchLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("handle", self.handle.out_of_range(&limits.handle)),
            tag("head", self.head.out_of_range(&limits.head)),
            tag("burn", self.burn.out_of_range(&limits.burn)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

impl Generator for Torch {
    type State = TorchState;

    fn roll(seed: u32) -> TorchState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&[(TorchStyle::Rag, 5), (TorchStyle::Bundle, 3), (TorchStyle::Cresset, 2)]);
        compose(&mut rng, seed, style)
    }

    fn noun(s: &TorchState) -> String {
        if s.burn.state == BurnState::Spent {
            return "Torch Stub".to_string();
        }
        match (s.head.style, s.head.fuel) {
            (TorchStyle::Rag, Fuel::Pitch) => "Pitch Torch",
            (TorchStyle::Rag, Fuel::Tallow) => "Tallow Torch",
            (TorchStyle::Rag, Fuel::Resin) => "Torch",
            (TorchStyle::Bundle, _) => "Splint Torch",
            (TorchStyle::Cresset, _) => "Cresset Torch",
        }
        .to_string()
    }

    fn build(s: &TorchState) -> ItemMesh {
        build_torch(s)
    }
}

impl Torch {
    /// "Forge new" forced to one head style, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, style)` when `style` is what
    /// `s` rolls.
    pub fn roll_as(seed: u32, style: TorchStyle) -> TorchState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, style)
    }

    /// How far the flame reaches, as if the torch were burning: its tip and its bulb. Unlike
    /// [`Torch::light`] this answers for any burn state, so a holder can keep a torch's flame
    /// clear of the head before it is lit.
    pub fn flame_extent(s: &TorchState) -> FlameExtent {
        let g = Geometry::of(s);
        let h = g.flame_height;
        FlameExtent {
            top: vec3::add(g.flame_base, [0.0, h, h * spine_offset(1.0, FLAME_TILT)]),
            bulb: vec3::add(g.flame_base, [0.0, h * FLAME_BELLY, h * spine_offset(FLAME_BELLY, FLAME_TILT)]),
            radius: g.flame_radius,
        }
    }

    /// Where the torch gives its light, while it burns: the heart of the flame, in the
    /// prototype's torch colour, reaching about the SRD's 20 ft of bright light ([`TORCH_FEET`]):
    /// a cresset's open basket a little farther, a bundle of splints a little less, tallow
    /// less than pitch or resin, and a bigger flame a little farther. The shares are our own,
    /// kept narrow enough that no torch reaches as far as the weakest lantern
    /// ([`crate::families::lantern::Lantern::light`]). `None` unless the torch is
    /// [`BurnState::Burning`].
    pub fn light(s: &TorchState) -> Option<LightAnchor> {
        if s.burn.state != BurnState::Burning {
            return None;
        }
        let g = Geometry::of(s);
        let style = match s.head.style {
            TorchStyle::Rag => 1.0,
            TorchStyle::Bundle => 0.9,
            TorchStyle::Cresset => 1.08,
        };
        let fuel = if s.head.fuel == Fuel::Tallow { 0.92 } else { 1.0 };
        let flicker = match s.head.style {
            TorchStyle::Rag => 0.8,
            TorchStyle::Bundle => 1.0,
            TorchStyle::Cresset => 0.6,
        };
        let feet = TORCH_FEET * style * fuel * (0.9 + 0.1 * s.burn.flame);
        let at = vec3::add(g.flame_base, [0.0, g.flame_height * FLAME_BELLY, g.flame_height * spine_offset(FLAME_BELLY, FLAME_TILT)]);
        Some(LightAnchor::feet(at, FLAME_LIGHT, feet, flicker))
    }
}

/// How far a torch's flame reaches, in item space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlameExtent {
    /// The flame's tip.
    pub top: V3,
    /// The middle of the flame's widest ring, which wraps the burning head.
    pub bulb: V3,
    /// Radius of the widest ring.
    pub radius: f32,
}

/// Where the widest ring of [`add_flame`]'s flame stands, as a fraction of its height.
const FLAME_BELLY: f32 = 0.26;

/// How far a torch's flame leans forward (along item +Z) at its base, in radians: 55 degrees,
/// curling back up to vertical at the tip as a carried flame does. Our own.
pub const FLAME_TILT: f32 = 55.0 * std::f32::consts::PI / 180.0;

/// Where a torch's flame belly sits on the burning head, as a fraction of the way from the
/// head's base to its top: high, so the flame crowns the head rather than hanging off its side.
const FLAME_SEAT: f32 = 0.8;

/// How wide a torch's flame is at its belly, as a multiple of the head's girth: the bulb
/// wraps the burning head without swallowing it (#276, #280).
const FLAME_GIRTH: f32 = 1.3;

/// A bundle of splints fans wider than its girth, so its flame is that much wider again.
const BUNDLE_SPLAY: f32 = 1.35;

/// The torch's light in feet (`light` of `Torch` in `data/bases.json`): the SRD's torch, 20 ft
/// of bright light (https://www.dndbeyond.com/sources/dnd/free-rules/equipment#AdventuringGear).
pub const TORCH_FEET: f32 = 20.0;

fn compose(rng: &mut Mulberry32, seed: u32, style: TorchStyle) -> TorchState {
    use TorchStyle as T;
    let mut s = TorchState { seed, ..TorchState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);

    let h = &mut s.handle;
    h.length = rng.range_f32(2.8, 4.4);
    h.radius = rng.range_f32(0.09, 0.13);
    h.taper = rng.range_f32(0.1, 0.4);
    h.crook = match style {
        T::Rag => rng.range_f32(0.0, 0.6),
        T::Bundle => rng.range_f32(0.2, 1.0),
        T::Cresset => rng.range_f32(0.0, 0.15),
    };
    h.facets = match style {
        T::Rag => rng.int(6, 14),
        T::Bundle => rng.int(5, 9),
        T::Cresset => rng.int(10, 16),
    };
    h.grip = *rng.weighted(match style {
        T::Rag => &[(HandleGrip::Bare, 3), (HandleGrip::Leather, 2), (HandleGrip::Cord, 1)][..],
        T::Bundle => &[(HandleGrip::Bare, 3), (HandleGrip::Cord, 2)],
        T::Cresset => &[(HandleGrip::Leather, 2), (HandleGrip::Bare, 1), (HandleGrip::Cord, 1)],
    });
    h.ferrule = match style {
        T::Rag => rng.chance(0.25),
        T::Bundle => false,
        T::Cresset => true,
    };

    let d = &mut s.head;
    d.style = style;
    d.fuel = *rng.weighted(match style {
        T::Rag => &[(Fuel::Pitch, 3), (Fuel::Tallow, 2)][..],
        T::Bundle => &[(Fuel::Resin, 3), (Fuel::Pitch, 1)],
        T::Cresset => &[(Fuel::Pitch, 2), (Fuel::Resin, 1), (Fuel::Tallow, 1)],
    });
    d.length = match style {
        T::Rag => rng.range_f32(0.85, 1.3),
        T::Bundle => rng.range_f32(1.0, 1.5),
        T::Cresset => rng.range_f32(0.8, 1.2),
    };
    d.girth = match style {
        T::Rag => rng.range_f32(0.2, 0.3),
        T::Bundle => rng.range_f32(0.2, 0.28),
        T::Cresset => rng.range_f32(0.22, 0.32),
    };
    d.layers = match style {
        T::Rag => rng.int(3, 7),
        T::Bundle => rng.int(6, 12),
        T::Cresset => rng.int(4, 8),
    };
    d.bindings = match style {
        T::Rag => rng.int(0, 2),
        T::Bundle => rng.int(1, 3),
        T::Cresset => 0,
    };
    d.binding = match style {
        T::Rag => *rng.weighted(&[(Cord, 2), (Iron, 1)]),
        T::Bundle => *rng.weighted(&[(Cord, 3), (Leather, 1)]),
        T::Cresset => Iron,
    };

    let b = &mut s.burn;
    b.state = *rng.weighted(&[(BurnState::Fresh, 3), (BurnState::Burning, 5), (BurnState::Doused, 1), (BurnState::Spent, 1)]);
    b.char = match b.state {
        BurnState::Fresh => 0.0,
        BurnState::Burning => rng.range_f32(0.1, 0.6),
        BurnState::Doused => rng.range_f32(0.15, 0.7),
        BurnState::Spent => rng.range_f32(0.86, 1.0),
    };
    b.flame = if b.state == BurnState::Burning { rng.range_f32(0.7, 1.4) } else { 1.0 };
    // The fuel is what the char says is left.
    b.fuel_left = if b.state == BurnState::Spent { 0.0 } else { crate::kindle::TORCH_SECONDS * (1.0 - b.char) };
    s.fit(&LIMITS);
    s
}

// ───────────────────────── Geometry ─────────────────────────

/// The measurements every part of the build shares, and [`Torch::light`] reads.
struct Geometry {
    /// Butt and top of the handle (the top is the head's base).
    butt: f32,
    top: f32,
    /// Bottom and burnt-down top of the head.
    head_base: f32,
    head_top: f32,
    /// Where the char meets the fuel.
    char_line: f32,
    flame_base: V3,
    flame_height: f32,
    flame_radius: f32,
}

impl Geometry {
    fn of(s: &TorchState) -> Geometry {
        let (h, d, b) = (&s.handle, &s.head, &s.burn);
        let butt = -0.36 * h.length;
        let top = 0.64 * h.length;
        let length = match d.style {
            TorchStyle::Rag => d.length,
            TorchStyle::Bundle => d.length * 0.8,
            TorchStyle::Cresset => d.length,
        };
        let head_base = match d.style {
            TorchStyle::Rag => top - 0.1 * d.length,
            TorchStyle::Bundle => top - 0.2 * d.length,
            TorchStyle::Cresset => top,
        };
        // A burning head shrinks as it chars; a spent one is a stub.
        let head_top = head_base + length * (1.0 - 0.45 * b.char);
        let char_line = head_top - (head_top - head_base) * b.char.clamp(0.0, 1.0) * 0.92;
        let (height, radius) = match d.style {
            TorchStyle::Rag => (1.35 * d.length, d.girth * FLAME_GIRTH),
            TorchStyle::Bundle => (1.1 * d.length, d.girth * FLAME_GIRTH * BUNDLE_SPLAY),
            TorchStyle::Cresset => (1.5 * d.length, d.girth * FLAME_GIRTH),
        };
        // The flame's belly (widest ring) stands at [`FLAME_BELLY`] of its height; seat it
        // across the upper head, with the base at or below the char line, so the flame
        // wraps the burning part and the tip still rises above the head.
        let flame_height = height * b.flame;
        let belly = head_base + (head_top - head_base) * FLAME_SEAT;
        let flame_base = [0.0, char_line.min(belly - FLAME_BELLY * flame_height), 0.0];
        Geometry {
            butt,
            top,
            head_base,
            head_top,
            char_line,
            flame_base,
            flame_height,
            flame_radius: radius * (0.8 + 0.2 * b.flame),
        }
    }
}

/// The handle's centre line at `u` (0 at the butt, 1 at the head): a bowed branch meets the
/// head on the axis.
fn handle_centre(s: &TorchState, g: &Geometry, u: f32) -> V3 {
    let l = s.handle.length;
    let c = s.handle.crook;
    [c * 0.045 * l * (PI * u).sin(), g.butt + (g.top - g.butt) * u, c * 0.015 * l * (TAU * u).sin()]
}

fn handle_radius(s: &TorchState, u: f32) -> f32 {
    s.handle.radius * (1.0 - s.handle.taper * (1.0 - u))
}

/// Splits a lathe profile walked upward at height `y`: the part below and the part above,
/// sharing the cut point.
fn split_profile(profile: &[[f32; 2]], y: f32) -> (Vec<[f32; 2]>, Vec<[f32; 2]>) {
    let (mut lower, mut upper) = (Vec::new(), Vec::new());
    for (i, &p) in profile.iter().enumerate() {
        if p[1] <= y {
            lower.push(p);
        } else {
            if upper.is_empty() && i > 0 {
                let a = profile[i - 1];
                let t = (y - a[1]) / (p[1] - a[1]).max(1e-9);
                let cut = [a[0] + (p[0] - a[0]) * t, y];
                lower.push(cut);
                upper.push(cut);
            }
            upper.push(p);
        }
    }
    if lower.len() < 2 {
        lower.clear();
    }
    if upper.len() < 2 {
        upper.clear();
    }
    (lower, upper)
}

/// How much a char shrinks the head at height `y`.
fn char_shrink(g: &Geometry, char: f32, y: f32) -> f32 {
    1.0 - 0.25 * char * smoothstep(g.char_line - 0.05, g.head_top, y)
}

fn build_torch(s: &TorchState) -> ItemMesh {
    let g = Geometry::of(s);
    let mut parts = Parts::new();
    let mut rng = Mulberry32::new(s.seed.wrapping_mul(2_654_435_761).wrapping_add(0x70c4));
    handle(s, &g, &mut parts);
    let fuel = s.head.fuel.color();
    let burning = s.burn.state == BurnState::Burning;
    match s.head.style {
        TorchStyle::Rag => rag_head(s, &g, fuel, &mut parts),
        TorchStyle::Bundle => bundle_head(s, &g, fuel, &mut rng, &mut parts),
        TorchStyle::Cresset => cresset_head(s, &g, fuel, &mut parts),
    }
    if burning {
        add_flame_leaning(&mut parts, g.flame_base, g.flame_height, g.flame_radius, 0.0, FLAME_TILT, s.seed);
    }
    parts.finish()
}

/// The handle, its grip wrap and its ferrule.
fn handle(s: &TorchState, g: &Geometry, parts: &mut Parts) {
    let h = &s.handle;
    let samples = 32;
    // Into the head a little, so no seam shows under it.
    let reach = 1.0 + 0.05 * s.head.length / (g.top - g.butt);
    let path: Vec<V3> = (0..=samples).map(|i| handle_centre(s, g, reach * i as f32 / samples as f32)).collect();
    let whittled = h.facets <= 8;
    let options = SweepOptions { shading: if whittled { Shading::FACETED } else { Shading::SMOOTH }, ..Default::default() };
    let mesh = tube(&path, h.facets.max(5) as usize, &options, |t| handle_radius(s, (t * reach).min(1.0)));
    parts.add("handle", mesh, Wood);

    if h.grip != HandleGrip::Bare {
        let (y0, y1) = (-0.12 * h.length, 0.12 * h.length);
        let wire = h.radius * if h.grip == HandleGrip::Leather { 0.2 } else { 0.17 };
        let turns = ((y1 - y0) / (wire * 2.6)).round().max(3.0);
        let n = (turns * 20.0) as usize;
        let path: Vec<V3> = (0..=n)
            .map(|i| {
                let k = i as f32 / n as f32;
                let y = y0 + (y1 - y0) * k;
                let u = (y - g.butt) / (g.top - g.butt);
                let c = handle_centre(s, g, u);
                let r = handle_radius(s, u) + wire * 0.55;
                let (sn, cs) = (TAU * turns * k).sin_cos();
                [c[0] + r * cs, c[1], c[2] - r * sn]
            })
            .collect();
        match h.grip {
            HandleGrip::Leather => parts.add("grip", thong(&path, false, wire), Leather),
            _ => parts.add_unoutlined("grip", cord(&path, false, wire, 6), Cord),
        };
    }

    if h.ferrule {
        let r = h.radius * 1.08;
        let (y0, y1) = match s.head.style {
            TorchStyle::Cresset => (g.top - 0.32 * s.head.length, g.top + 0.02),
            _ => (g.head_base - 0.16 * s.head.length, g.head_base + 0.02),
        };
        let band = r * 0.18;
        let profile = [
            [r * 0.98, y0],
            [r + band, y0],
            [r + band, y0 + band * 1.2],
            [r + band * 0.6, y0 + band * 1.4],
            [r + band * 0.6, y1 - band * 1.4],
            [r + band * 1.2, y1 - band * 1.2],
            [r + band * 1.2, y1],
            [r * 0.98, y1],
        ];
        parts.add("ferrule", lathe(&profile, &LatheOptions { facets: 20, shading: Shading::BANDED, ..Default::default() }), Iron);
    }
}

/// Adds a head profile split at the char line: fuel below, char above, and a ring of
/// embers where they meet while the torch burns.
fn split_head(s: &TorchState, g: &Geometry, profile: &[[f32; 2]], fuel: Look, warp: impl Fn(V3) -> V3, parts: &mut Parts) {
    let (lower, upper) = split_profile(profile, g.char_line);
    let options = LatheOptions::new(36, Shading::SMOOTH);
    parts.add_look("head", lathe_warped(&lower, &options, &warp), fuel);
    parts.add_look("char", lathe_warped(&upper, &options, &warp), Look::of(Wood).tinted(CHAR));
    if s.burn.state == BurnState::Burning && !upper.is_empty() && !lower.is_empty() {
        let at = upper[0];
        let ring = warp([at[0], at[1], 0.0]);
        parts.add_look("embers", ember_ring(ring[0].abs() * 0.99, at[1], s.head.girth * 0.05), glowing(EMBER, 0.9));
    }
}

/// A rag wound round the handle's end: overlapping turns of cloth over a swelling core, a
/// loose tail, and bindings.
fn rag_head(s: &TorchState, g: &Geometry, fuel: Rgb, parts: &mut Parts) {
    let d = &s.head;
    let (y0, len, gi) = (g.head_base, g.head_top - g.head_base, d.girth);
    let r_top = s.handle.radius;
    let control = [
        [0.0, y0],
        [r_top * 1.05, y0],
        [gi * 0.74, y0 + 0.08 * len],
        [gi * 0.95, y0 + 0.32 * len],
        [gi, y0 + 0.62 * len],
        [gi * 0.86, y0 + 0.88 * len],
        [gi * 0.42, y0 + 0.99 * len],
        [0.0, y0 + len],
    ];
    let profile = spline_profile(&control, 72);
    let turns = d.layers.max(2) as f32;
    let char = s.burn.char;
    let seed = s.seed;
    let warp = |p: V3| {
        let t = ((p[1] - y0) / len).clamp(0.0, 1.0);
        let angle = (-p[2]).atan2(p[0]);
        // Turns of a strip wound upward: each overlaps the one below, a step at its edge.
        let shingle = (t * turns + angle / TAU).rem_euclid(1.0).powf(1.6);
        let lump = value_noise3([p[0] * 7.0, p[1] * 5.0, p[2] * 7.0], seed) - 0.5;
        let k = (1.0 + 0.09 * (shingle - 0.5) + 0.08 * lump) * char_shrink(g, char, p[1]);
        [p[0] * k, p[1], p[2] * k]
    };
    split_head(s, g, &profile, Look::of(Cloth).tinted(fuel), warp, parts);

    // The loose end of the rag, hanging off the bottom turn.
    if s.burn.state != BurnState::Spent {
        let a = (s.seed % 7) as f32 * 0.9;
        let (sa, ca) = a.sin_cos();
        let out = [ca, 0.0, -sa];
        let at = |r: f32, y: f32| [ca * r, y, -sa * r];
        let path = vec![
            at(gi * 0.9, y0 + 0.16 * len),
            at(gi * 0.86, y0 + 0.04 * len),
            at(gi * 0.8, y0 - 0.1 * len),
            at(gi * 0.95, y0 - 0.26 * len),
        ];
        let faces = vec![out; path.len()];
        parts.add_look("rag_tail", strap(&path, &faces, gi * 0.55, gi * 0.06, false), Look::of(Cloth).tinted(fuel));
    }

    // Bindings round the lower turns, below the char.
    let mut bindings = MeshBuilder::new();
    for k in 0..d.bindings {
        let y = y0 + len * (0.14 + 0.2 * k as f32);
        if y >= g.char_line - 0.04 {
            break;
        }
        let r = crate::geom::profile_radius_at(&profile, y, false).unwrap_or(gi) * 1.02;
        let wire = if d.binding == Iron { gi * 0.035 } else { gi * 0.05 };
        bindings.append(&ember_ring(r, y, wire));
    }
    parts.add("bindings", bindings, d.binding);
}

/// A bundle of split sticks round the handle's end, splaying a little toward their charred
/// tips, bound with cord or thong.
fn bundle_head(s: &TorchState, g: &Geometry, fuel: Rgb, rng: &mut Mulberry32, parts: &mut Parts) {
    let d = &s.head;
    let n = d.layers.clamp(5, 12) as usize;
    let gi = d.girth;
    let rs = (gi * (PI / n as f32).sin() * 0.95).max(0.03);
    let ring = (gi - rs).max(s.handle.radius + rs * 0.85);
    let (y0, y1) = (g.head_base, g.head_top);
    let char_line = g.char_line;
    let section = regular_polygon(5, 1.0, 0.0);
    let options = SweepOptions { shading: Shading::FACETED, ..Default::default() };
    let (mut wood, mut charred, mut embers) = (MeshBuilder::new(), MeshBuilder::new(), MeshBuilder::new());
    // A ring of sticks round the handle, and sticks in the middle standing on its end.
    let centre = 1 + n.saturating_sub(5) / 3;
    for i in 0..n + centre {
        let core = i >= n;
        let a = TAU * i as f32 / n as f32 + rng.range_f32(-0.12, 0.12);
        let (sa, ca) = a.sin_cos();
        // The first middle stick is thick enough to hide the handle's end.
        let rs = if i == n { (ring - rs * 0.7).max(rs) } else { rs };
        let ring = if core { (ring - s.handle.radius - rs * 0.5) * 0.5 * (i - n) as f32 } else { ring };
        let y0 = if core { g.top } else { y0 };
        let tip = y1 + (y1 - y0) * rng.range_f32(-0.08, 0.06);
        let at = |y: f32| {
            let t = ((y - y0) / (y1 - y0)).clamp(0.0, 1.0);
            let r = ring * (1.0 + 0.18 * t * t);
            [ca * r, y, -sa * r]
        };
        let radius = |y: f32| {
            let t = ((y - char_line) / (tip - char_line).max(1e-3)).clamp(0.0, 1.0);
            rs * (1.0 - 0.45 * t)
        };
        let cut = if s.burn.char <= 0.0 { tip } else { char_line.min(tip - 0.02) };
        let stick = |from: f32, to: f32| {
            if to - from < 1e-3 {
                return MeshBuilder::new();
            }
            let path: Vec<V3> = (0..=8).map(|k| at(from + (to - from) * k as f32 / 8.0)).collect();
            sweep(&path, &section, &SweepOptions { up: Some([ca, 0.0, -sa]), ..options.clone() }, |t| radius(from + (to - from) * t))
        };
        wood.append(&stick(y0, cut));
        charred.append(&stick(cut, tip));
        if s.burn.state == BurnState::Burning && tip - cut > 0.03 {
            let mut e = torus(radius(cut) * 0.9, rs * 0.25, 10, 5, Shading::SMOOTH);
            e.translate(at(cut));
            embers.append(&e);
        }
    }
    parts.add_look("head", wood, Look::of(Wood).tinted(fuel));
    parts.add_look("char", charred, Look::of(Wood).tinted(CHAR));
    parts.add_look("embers", embers, glowing(EMBER, 0.9));

    let mut bindings = MeshBuilder::new();
    for k in 0..d.bindings {
        let y = y0 + (y1 - y0) * (0.06 + 0.18 * k as f32);
        if y >= char_line - 0.04 {
            break;
        }
        let t = ((y - y0) / (y1 - y0)).clamp(0.0, 1.0);
        let r = ring * (1.0 + 0.18 * t * t) + rs * 0.95;
        bindings.append(&ember_ring(r, y, rs * 0.32));
    }
    parts.add("bindings", bindings, d.binding);
}

/// An iron cage of curved bars on the socket, with a lumpy mass of fuel-soaked tow in it.
fn cresset_head(s: &TorchState, g: &Geometry, fuel: Rgb, parts: &mut Parts) {
    let d = &s.head;
    let gi = d.girth;
    let y0 = g.top;
    let len = d.length;
    let bars = d.layers.clamp(4, 8) as usize;
    let wire = gi * 0.05;
    let r_socket = s.handle.radius * 1.25;
    let bar_r = |t: f32| r_socket + (gi * 1.05 - r_socket) * (PI * 0.5 * (t / 0.55).min(1.0)).sin() - gi * 0.12 * smoothstep(0.55, 1.0, t);
    let mut cage = MeshBuilder::new();
    for i in 0..bars {
        let a = TAU * i as f32 / bars as f32;
        let (sa, ca) = a.sin_cos();
        let mut path: Vec<V3> = (0..=16)
            .map(|k| {
                let t = k as f32 / 16.0;
                let r = bar_r(t);
                [ca * r, y0 + len * t, -sa * r]
            })
            .collect();
        // Each bar's end curls outward.
        let end = path[path.len() - 1];
        for k in 1..=5 {
            let q = k as f32 / 5.0 * PI * 0.8;
            let r = gi * 0.12;
            let radial = vec3::length([end[0], 0.0, end[2]]) + r * (1.0 - q.cos());
            path.push([ca * radial, end[1] + r * q.sin(), -sa * radial]);
        }
        cage.append(&tube(&path, 6, &SweepOptions::default(), |_| wire));
    }
    for (t, scale) in [(0.3f32, 1.0f32), (1.0, 1.0)] {
        cage.append(&ember_ring(bar_r(t) * scale, y0 + len * t, wire * 1.2));
    }
    let mut socket = frustum(r_socket * 0.9, r_socket * 1.1, 0.06 * len, 16, Shading::SMOOTH);
    socket.translate([0.0, y0 - 0.02, 0.0]);
    cage.append(&socket);
    parts.add("cage", cage, Iron);

    // The fuel mass sits inside the bars.
    let top = g.head_top;
    let control = [
        [0.0, y0 + 0.04 * len],
        [gi * 0.5, y0 + 0.1 * len],
        [gi * 0.82, y0 + 0.45 * len],
        [gi * 0.8, y0 + 0.4 * len + (top - y0) * 0.45],
        [gi * 0.4, top - 0.02],
        [0.0, top],
    ];
    let mut control = control.to_vec();
    control.sort_by(|a, b| a[1].total_cmp(&b[1]));
    let profile = spline_profile(&control, 48);
    let seed = s.seed;
    let char = s.burn.char;
    let warp = |p: V3| {
        let lump = value_noise3([p[0] * 9.0, p[1] * 9.0, p[2] * 9.0], seed) - 0.5;
        let k = (1.0 + 0.3 * lump) * char_shrink(g, char, p[1]);
        [p[0] * k, p[1], p[2] * k]
    };
    split_head(s, g, &profile, Look::of(Cloth).tinted(fuel), warp, parts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_profile_shares_the_cut() {
        let p = [[0.0, 0.0], [1.0, 0.0], [1.0, 2.0], [0.0, 2.0]];
        let (lo, hi) = split_profile(&p, 1.0);
        assert_eq!(lo.last(), hi.first());
        assert_eq!(lo.last().unwrap(), &[1.0, 1.0]);
        let (lo, hi) = split_profile(&p, 5.0);
        assert_eq!(lo.len(), 4);
        assert!(hi.is_empty());
    }

    #[test]
    fn light_follows_the_flame() {
        let mut s = Torch::roll_as(5, TorchStyle::Rag);
        s.burn.state = BurnState::Burning;
        let light = Torch::light(&s).unwrap();
        let mesh = Torch::build(&s);
        let flame = mesh.parts.iter().find(|p| p.name == "flame").unwrap();
        let (lo, hi) = flame.positions.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
        assert!(light.position[1] > lo && light.position[1] < hi);
        assert!(light.radius > crate::build::light::UNITS_PER_FOOT * 10.0);
        s.burn.state = BurnState::Doused;
        assert!(Torch::light(&s).is_none());
    }

    fn burning(seed: u32, style: TorchStyle, char: f32, flame: f32) -> TorchState {
        let mut s = Torch::roll_as(seed, style);
        s.burn = TorchBurn { state: BurnState::Burning, char, flame, ..s.burn };
        s
    }

    #[test]
    fn the_flame_engulfs_the_burning_head() {
        let mut ratios = Vec::new();
        for style in [TorchStyle::Rag, TorchStyle::Bundle, TorchStyle::Cresset] {
            for seed in 0..120u32 {
                for (char, flame) in [(0.0, 1.0), (0.3, 0.7), (0.5, 1.0), (0.8, 1.4), (1.0, 0.5)] {
                    let s = burning(seed, style, char, flame);
                    let g = Geometry::of(&s);
                    let mesh = Torch::build(&s);
                    let what = format!("{style:?} seed {seed} char {char} flame {flame}");
                    let fl = mesh.parts.iter().find(|p| p.name == "flame").unwrap();
                    let lo = fl.positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
                    let hi = fl.positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
                    // The widest ring, by its mean radius (the tongues make single points wander).
                    let reach = |p: &V3| (p[0] * p[0] + p[2] * p[2]).sqrt();
                    let mut rings: Vec<(i32, Vec<V3>)> = Vec::new();
                    for p in &fl.positions {
                        let key = (p[1] * 1000.0).round() as i32;
                        match rings.iter_mut().find(|r| r.0 == key) {
                            Some(r) => r.1.push(*p),
                            None => rings.push((key, vec![*p])),
                        }
                    }
                    // Mean distance from the ring's own centre (the tip leans sideways).
                    let ring_radius = |pts: &Vec<V3>| {
                        let n = pts.len() as f32;
                        let (cx, cz) = (pts.iter().map(|p| p[0]).sum::<f32>() / n, pts.iter().map(|p| p[2]).sum::<f32>() / n);
                        pts.iter().map(|p| ((p[0] - cx).powi(2) + (p[2] - cz).powi(2)).sqrt()).sum::<f32>() / n
                    };
                    let widest = rings.iter().max_by(|a, b| ring_radius(&a.1).total_cmp(&ring_radius(&b.1))).unwrap();
                    let (belly_y, belly_r) = (widest.1[0][1], ring_radius(&widest.1));
                    assert!(belly_y >= g.head_base && belly_y <= g.head_top, "belly off the head: {what} y {belly_y} base {} top {} flame_base {} lo {lo} hi {hi}", g.head_base, g.head_top, g.flame_base[1]);
                    // The head's own radius round that height.
                    let head_r = mesh
                        .parts
                        .iter()
                        .filter(|p| ["head", "char", "cage", "bindings"].contains(&p.name))
                        .flat_map(|p| p.positions.iter())
                        .filter(|p| (p[1] - belly_y).abs() < 0.06)
                        .map(reach)
                        .fold(0.0f32, f32::max);
                    // Wider than the head, but not by much (#280): the bulb is about 1.3 times
                    // the head's girth, and the tongues and the burn's size move it either way.
                    assert!(belly_r > head_r * 1.02, "flame {belly_r} not wider than head {head_r}: {what}");
                    assert!(belly_r < head_r * 2.0, "flame {belly_r} swallows head {head_r}: {what}");
                    ratios.push(belly_r / head_r);
                    assert!(lo <= g.char_line + 0.02, "flame starts above the char line: {what}");
                    assert!(hi > g.head_top, "tip below the head top: {what}");
                    let light = Torch::light(&s).unwrap().position;
                    let (min_x, max_x) = fl.positions.iter().map(|p| p[0]).fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x), b.max(x)));
                    assert!(light[1] > lo && light[1] < hi && light[0] > min_x && light[0] < max_x, "light outside the flame: {what}");
                    let (min_z, max_z) = fl.positions.iter().map(|p| p[2]).fold((f32::MAX, f32::MIN), |(a, b), z| (a.min(z), b.max(z)));
                    assert!(light[2] > min_z && light[2] < max_z, "light outside the flame in z: {what}");
                    // The flame leans forward: it reaches further ahead of the wick than behind.
                    assert!(max_z > -min_z, "flame does not lean forward: {what}");
                }
            }
        }
        let mean = ratios.iter().sum::<f32>() / ratios.len() as f32;
        assert!((1.1..1.5).contains(&mean), "the bulb is about 1.3 times the head on average: {mean}");
    }
}
