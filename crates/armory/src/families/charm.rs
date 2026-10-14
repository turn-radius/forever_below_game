//! Charms: small carved or cast tokens on a jump ring, a chain stub or a cord loop. A fang,
//! a hammer, a sun wheel, a knot, an idol, a rune stone, an acorn, a bell or a key.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (the `charm`,
//! `loop` and `stone` entries of `SECTIONS`, `PRESETS.charm`, the `fam==='charm'` branch of
//! `randomize()`, the charm rule in `proportion()`, `buildCharm()` and `forgeNoun()`).
//!
//! Departures from the prototype:
//! - The shape is the archetype: each picks its own size, materials, carving and hanger
//!   (a cast bell hangs from a jump ring, a carved idol from a beaded cord), instead of the
//!   prototype's independent picks.
//! - The prototype's `loop` section is [`Hanger`] here (`loop` is a Rust keyword), and the
//!   charm's `stone` toggle is [`Charm::inset`], so it does not shadow the stone section.
//! - The bell is a hollow shell with a lip and a clapper on a rod, and the hammer head, the
//!   key bits and the carvings are bevelled slabs instead of three.js boxes and bevels.
//!
//! Item space: the hanger is at the top, about y = 0, and the charm hangs below it to
//! about y = -size, facing +Z.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use crate::build::chain::{cable_chain, jump_ring, link_pitch, twisted_cord};
use crate::build::device::device_mesh;
use crate::build::gem::{cut_for, cut_stone};
use crate::build::key::{KeyBow, collar_height, key};
use crate::build::parts::{Look, Parts};
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::gem::{GemCut, Gemstone};
use crate::geom::polygon::P2;
use crate::geom::{
    CatmullRom, ItemMesh, LatheOptions, MeshBuilder, Shading, SlabOptions, SweepOptions, V3, bevelled_slab,
    cylinder, frustum, icosphere, lathe, resample_by_length, spline_profile, sweep, torus, tube,
    uv_sphere, value_noise3, vec3,
};
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::sections::device::Device;
use crate::sections::jewel::{SetStone, SetStoneLimits};
use crate::slider::{Sliders, sliders};

use MaterialKind::{Bone, Brass, Cord, Gold, Iron, Leather, Silver, Stone, Wood};

/// The charm generator.
pub struct Charm;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CharmShape {
    /// A fang or claw.
    Fang,
    /// A Thor's hammer.
    Hammer,
    /// A spoked sun wheel.
    Wheel,
    /// A torus knot.
    Knot,
    Idol,
    RuneStone,
    Acorn,
    Bell,
    Key,
}

impl CharmShape {
    pub const ALL: [CharmShape; 9] = [
        CharmShape::Fang, CharmShape::Hammer, CharmShape::Wheel, CharmShape::Knot, CharmShape::Idol, CharmShape::RuneStone,
        CharmShape::Acorn, CharmShape::Bell, CharmShape::Key,
    ];

    /// Shapes that can take an inset stone.
    pub fn takes_stone(self) -> bool {
        matches!(self, CharmShape::Wheel | CharmShape::Idol | CharmShape::Key | CharmShape::Hammer)
    }

    /// Shapes with a face to carve a device on.
    pub fn takes_device(self) -> bool {
        matches!(self, CharmShape::Hammer | CharmShape::Wheel | CharmShape::Idol | CharmShape::RuneStone)
    }

    /// Shapes whose `bindings` slider binds them with cord.
    pub fn takes_bindings(self) -> bool {
        matches!(self, CharmShape::Fang | CharmShape::Hammer | CharmShape::Idol)
    }
}

/// The charm itself (the prototype's `charm` section).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharmBody {
    pub shape: CharmShape,
    /// Height.
    pub size: f32,
    /// Thickness, as a multiplier.
    pub girth: f32,
    /// How far a fang hooks.
    pub curve: f32,
    /// Spokes of a wheel (detail + 2), which knot, how many key bits.
    pub detail: u32,
    /// Sides round turned and swept shapes; ten or fewer reads as whittled.
    pub facets: u32,
    /// Cord bindings round a fang, a hammer's handle or an idol.
    pub bindings: u32,
    /// An inset stone (wheel, idol, key, hammer).
    pub inset: bool,
    /// A carving (hammer, wheel, idol, rune stone), when no stone is inset.
    pub device: Device,
    pub material: MaterialKind,
}

sliders! {
    CharmBodyLimits for CharmBody {
        size: 0.15, 0.7, 0.005;
        girth: 0.5, 1.6, 0.01;
        curve: 0.0, 1.0, 0.01;
        detail: 1.0, 8.0, 1.0;
        facets: 4.0, 24.0, 1.0;
        bindings: 0.0, 4.0, 1.0;
    }
}

impl Default for CharmBody {
    /// The prototype's `BASE.charm`.
    fn default() -> Self {
        CharmBody {
            shape: CharmShape::Fang,
            size: 0.45,
            girth: 1.0,
            curve: 0.6,
            detail: 3,
            facets: 12,
            bindings: 2,
            inset: false,
            device: Device::None,
            material: Bone,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HangerStyle {
    JumpRing,
    CordLoop,
    ChainStub,
}

impl HangerStyle {
    pub const ALL: [HangerStyle; 3] = [HangerStyle::JumpRing, HangerStyle::CordLoop, HangerStyle::ChainStub];
}

/// What the charm hangs from (the prototype's `loop` section).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hanger {
    pub style: HangerStyle,
    /// Ring diameter, link length, or a twentieth of the cord loop's height.
    pub size: f32,
    /// Glass beads threaded on a cord loop.
    pub beads: u32,
    pub material: MaterialKind,
}

sliders! {
    HangerLimits for Hanger {
        size: 0.02, 0.2, 0.001;
        beads: 0.0, 6.0, 1.0;
    }
}

impl Default for Hanger {
    /// The prototype's `BASE.loop`.
    fn default() -> Self {
        Hanger { style: HangerStyle::CordLoop, size: 0.06, beads: 2, material: Cord }
    }
}

/// Every parameter of one charm.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharmState {
    pub seed: u32,
    pub charm: CharmBody,
    pub hanger: Hanger,
    pub stone: SetStone,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for CharmState {
    fn default() -> Self {
        CharmState {
            seed: 0,
            charm: CharmBody::default(),
            hanger: Hanger::default(),
            // The prototype's charms set small cabochons.
            stone: SetStone { cut: GemCut::Cabochon, size: 0.025, stone: Gemstone::Turquoise },
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every charm section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharmLimits {
    pub charm: CharmBodyLimits,
    pub hanger: HangerLimits,
    pub stone: SetStoneLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges (it has no `RANGES.charm`).
pub const LIMITS: CharmLimits = CharmLimits {
    charm: CharmBodyLimits::SLIDERS,
    hanger: HangerLimits::SLIDERS,
    stone: SetStoneLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl CharmState {
    /// Clamp and snap every slider into `limits`, then keep the stone at most a quarter of
    /// the charm (the prototype's `proportion()`).
    pub fn fit(&mut self, limits: &CharmLimits) {
        self.charm.fit(&limits.charm);
        self.hanger.fit(&limits.hanger);
        self.stone.fit(&limits.stone);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
        self.stone.size = self.stone.size.min(self.charm.size * 0.25).max(limits.stone.size.min);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &CharmLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("charm", self.charm.out_of_range(&limits.charm)),
            tag("hanger", self.hanger.out_of_range(&limits.hanger)),
            tag("stone", self.stone.out_of_range(&limits.stone)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

impl Generator for Charm {
    type State = CharmState;

    fn roll(seed: u32) -> CharmState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let shape = *rng.pick(&CharmShape::ALL);
        compose(&mut rng, seed, shape)
    }

    /// `forgeNoun()` for charms.
    fn noun(s: &CharmState) -> String {
        match s.charm.shape {
            CharmShape::Fang => "Fang Charm",
            CharmShape::Hammer => "Hammer Amulet",
            CharmShape::Wheel => "Sun Wheel",
            CharmShape::Knot => "Knot Charm",
            CharmShape::Idol => "Carved Idol",
            CharmShape::RuneStone => "Rune Stone",
            CharmShape::Acorn => "Acorn Charm",
            CharmShape::Bell => "Bell Charm",
            CharmShape::Key => "Key Charm",
        }
        .to_string()
    }

    fn build(s: &CharmState) -> ItemMesh {
        build_charm(s)
    }
}

impl Charm {
    /// "Forge new" forced to one shape, for turntables and tests. Draws the same stream as
    /// [`Generator::roll`], so `roll(s)` equals `roll_as(s, shape)` when `shape` is what
    /// `s` rolls.
    pub fn roll_as(seed: u32, shape: CharmShape) -> CharmState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, shape)
    }
}

fn is_metal(m: MaterialKind) -> bool {
    matches!(m, Iron | Brass | Gold | Silver)
}

fn compose(rng: &mut Mulberry32, seed: u32, shape: CharmShape) -> CharmState {
    use CharmShape as C;
    let mut s = CharmState { seed, ..CharmState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    let c = &mut s.charm;
    c.shape = shape;
    c.material = *rng.weighted(match shape {
        C::Fang => &[(Bone, 1)][..],
        C::Idol => &[(Wood, 3), (Bone, 2), (Stone, 1)],
        C::RuneStone => &[(Stone, 3), (Bone, 1)],
        C::Hammer => &[(Silver, 3), (Iron, 2), (Brass, 2), (Gold, 1)],
        C::Wheel => &[(Brass, 3), (Gold, 2), (Silver, 2)],
        C::Knot => &[(Brass, 2), (Silver, 2), (Gold, 1), (Iron, 1)],
        C::Acorn => &[(Gold, 2), (Brass, 2), (Silver, 1), (Wood, 2)],
        C::Bell => &[(Brass, 4), (Silver, 2), (Iron, 1)],
        C::Key => &[(Iron, 4), (Brass, 2), (Silver, 1)],
    });
    c.size = match shape {
        C::Fang => rng.range_f32(0.35, 0.55),
        C::Hammer => rng.range_f32(0.32, 0.45),
        C::Wheel => rng.range_f32(0.3, 0.45),
        C::Knot => rng.range_f32(0.3, 0.42),
        C::Idol => rng.range_f32(0.4, 0.6),
        C::RuneStone => rng.range_f32(0.32, 0.48),
        C::Acorn => rng.range_f32(0.25, 0.38),
        C::Bell => rng.range_f32(0.28, 0.42),
        C::Key => rng.range_f32(0.4, 0.6),
    };
    c.girth = rng.range_f32(0.8, 1.25);
    c.curve = if shape == C::Fang { rng.range_f32(0.3, 0.9) } else { 0.6 };
    c.detail = match shape {
        C::Wheel => rng.int(2, 6),
        C::Knot => rng.int(1, 8),
        C::Key => rng.int(1, 4),
        _ => 3,
    };
    c.facets = match shape {
        C::Fang => rng.int(8, 16),
        C::Idol => rng.int(6, 12),
        C::Hammer => rng.int(6, 10),
        C::Acorn => rng.int(10, 18),
        C::Bell => rng.int(14, 24),
        _ => 12,
    };
    c.bindings = match shape {
        C::Fang => rng.int(0, 3),
        C::Idol => rng.int(0, 2),
        C::Hammer if rng.chance(0.3) => 1,
        _ => 0,
    };
    c.inset = shape.takes_stone() && rng.chance(0.45);
    c.device = match shape {
        C::RuneStone => *rng.weighted(&[(Device::Rune, 2), (Device::Cross, 1), (Device::Wheel, 1)]),
        _ if shape.takes_device() && !c.inset => *rng.weighted(&[(Device::None, 2), (Device::Rune, 1), (Device::Star, 1)]),
        _ => Device::None,
    };
    let body = *c;

    let h = &mut s.hanger;
    h.style = *rng.weighted(if is_metal(body.material) {
        &[(HangerStyle::JumpRing, 3), (HangerStyle::ChainStub, 2), (HangerStyle::CordLoop, 1)][..]
    } else {
        &[(HangerStyle::CordLoop, 3), (HangerStyle::JumpRing, 1), (HangerStyle::ChainStub, 1)]
    });
    h.material = match h.style {
        HangerStyle::CordLoop => *rng.pick(&[Cord, Leather]),
        _ if is_metal(body.material) => body.material,
        _ => *rng.pick(&[Iron, Brass]),
    };
    h.size = if shape == C::Key { rng.range_f32(0.06, 0.09) } else { rng.range_f32(0.04, 0.08) };
    h.beads = if h.style == HangerStyle::CordLoop && rng.chance(0.5) { rng.int(1, 4) } else { 0 };

    s.stone.cut = GemCut::Cabochon;
    let stones: Vec<Gemstone> = Gemstone::ALL.into_iter().filter(|g| g.suits(GemCut::Cabochon)).collect();
    s.stone.stone = *rng.pick(&stones);
    s.stone.size = body.size * rng.range_f32(0.08, 0.2);

    let target = match body.material {
        Wood => EtchTarget::Wood,
        Bone => EtchTarget::Bone,
        _ => EtchTarget::Metals,
    };
    s.etch = Etching::roll(rng, 0.25, &[(target, 1)], 1);
    s.fit(&LIMITS);
    s
}

// ───────────────────────── Geometry ─────────────────────────

/// Collects the charm's meshes; `top` is where the hanger attaches.
struct Body {
    parts: Parts,
    top: f32,
}

fn turned(facets: u32) -> (usize, Shading) {
    if facets <= 10 { (facets.max(4) as usize, Shading::FACETED) } else { (facets as usize, Shading::SMOOTH) }
}

/// A cord binding round a shape at height `y`, of inner radius `r`.
fn binding(y: f32, r: f32, h: f32) -> MeshBuilder {
    let mut m = torus(r * 0.97, (h * 0.008).max(r * 0.08), 24, 8, Shading::SMOOTH);
    m.translate([0.0, y, 0.0]);
    m
}

/// A stone sunk into a surface at `at` facing `dir`, its girdle a little below the surface.
fn inset_stone(cut: GemCut, diameter: f32, at: V3, dir: V3) -> MeshBuilder {
    let r = diameter * 0.5;
    let mut gem = cut_stone(cut, r);
    gem.translate([0.0, -r * 0.1, 0.0]).place(at, dir);
    gem
}

/// A carving in relief at `at`, facing `dir`.
fn carving(device: Device, size: f32, depth: f32, seed: u32, at: V3, dir: V3) -> MeshBuilder {
    let mut m = device_mesh(device, size, depth, seed);
    // Carvings are built facing +Z; turn them to face `dir`, then sink them a little.
    m.rotate_x(-FRAC_PI_2).translate([0.0, -depth * 0.4, 0.0]).place(at, dir);
    m
}

fn build_charm(s: &CharmState) -> ItemMesh {
    let c = &s.charm;
    let mut rng = Mulberry32::new(s.seed.wrapping_mul(151).wrapping_add(11));
    let damage = s.cond.damage;
    let h = c.size;
    let gi = c.girth;
    let dark: Rgb = s.palette.get(c.material).unwrap_or(s.palette.bone).scaled(0.72);
    let cut = cut_for(s.stone.stone, s.stone.cut);
    let mut keep_stone = || !rng.chance((damage * 0.4) as f64);
    let mut body = Body { parts: Parts::new(), top: 0.0 };
    let mut bindings = MeshBuilder::new();
    let mut stones = MeshBuilder::new();
    let mut device = MeshBuilder::new();
    let carve_depth = h * 0.014;

    match c.shape {
        CharmShape::Fang => {
            let cu = c.curve;
            let path = CatmullRom::centripetal(vec![
                [0.0, 0.0, 0.0],
                [h * 0.08 * cu, -h * 0.33, 0.0],
                [h * 0.3 * cu, -h * 0.64, 0.0],
                [h * 0.6 * cu, -h * (1.0 - 0.28 * cu), 0.0],
            ])
            .sample(20);
            let (sides, shading) = turned(c.facets);
            let section: Vec<P2> = (0..sides)
                .map(|k| {
                    let a = TAU * k as f32 / sides as f32;
                    [a.cos(), a.sin() * 0.82]
                })
                .collect();
            let r0 = h * 0.13 * gi;
            let radius = |t: f32| r0 * (1.0 - t).max(0.0).powf(0.75) + h * 0.006;
            let options = SweepOptions { shading, up: Some(vec3::Z), ..Default::default() };
            body.parts.add("charm", sweep(&path, &section, &options, radius), c.material);
            for k in 0..c.bindings {
                let t = 0.07 + k as f32 * 0.055;
                bindings.append(&binding(-h * t * 1.02, radius(t), h));
            }
        }
        CharmShape::Hammer => {
            let hw = h * 0.06 * gi;
            let (sides, shading) = turned(c.facets);
            let mut handle = frustum(hw * 1.1, hw, h * 0.55, sides, shading);
            handle.translate([0.0, -h * 0.55, 0.0]);
            let mut knob = uv_sphere(hw * 1.35, 14, 10);
            knob.translate([0.0, -hw * 0.6, 0.0]);
            let d = h * 0.13 * gi;
            let outline = [[-h * 0.14, -h * 0.5], [h * 0.14, -h * 0.5], [h * 0.3, -h * 0.93], [h * 0.24, -h], [-h * 0.24, -h], [-h * 0.3, -h * 0.93]];
            let head = bevelled_slab(&outline, &[], &SlabOptions::chamfered(d, (d * 0.2).min(h * 0.015)));
            body.parts.add("charm", MeshBuilder::merged([&handle, &knob, &head]), c.material);
            let face = [0.0, -h * 0.77, d * 0.5];
            if c.inset {
                if keep_stone() {
                    stones.append(&inset_stone(cut, s.stone.size.min(h * 0.18), face, vec3::Z));
                }
            } else {
                device.append(&carving(c.device, h * 0.3, carve_depth, s.seed, face, vec3::Z));
            }
            for k in 0..c.bindings {
                bindings.append(&binding(-h * (0.15 + k as f32 * 0.08), hw * 1.05, h));
            }
            body.top = hw * 0.75;
        }
        CharmShape::Wheel => {
            let (rr, tb, cy) = (h * 0.42, h * 0.045 * gi, -h * 0.5);
            let mut rim = torus(rr, tb, 56, 12, Shading::SMOOTH);
            rim.rotate_x(FRAC_PI_2).translate([0.0, cy, 0.0]);
            let n = (c.detail + 2).max(3);
            let mut spokes = MeshBuilder::new();
            for k in 0..n {
                let mut sp = cylinder(tb * 0.6, rr, 10, Shading::SMOOTH);
                sp.rotate_z(TAU * k as f32 / n as f32 + PI / n as f32).translate([0.0, cy, 0.0]);
                spokes.append(&sp);
            }
            let mut hub = lathe(
                &spline_profile(&[[tb * 2.2, -tb * 1.2], [tb * 2.3, 0.0], [tb * 2.2, tb * 1.2]], 8),
                &LatheOptions { facets: 20, ..Default::default() },
            );
            hub.rotate_x(FRAC_PI_2).translate([0.0, cy, 0.0]);
            body.parts.add("charm", MeshBuilder::merged([&rim, &spokes, &hub]), c.material);
            let face = [0.0, cy, tb * 1.2];
            if c.inset {
                if keep_stone() {
                    stones.append(&inset_stone(cut, s.stone.size.min(tb * 3.6), face, vec3::Z));
                }
            } else {
                device.append(&carving(c.device, tb * 3.4, carve_depth.min(tb * 0.3), s.seed, face, vec3::Z));
            }
            body.top = cy + rr + tb * 0.4;
        }
        CharmShape::Knot => {
            const KNOTS: [(f32, f32); 8] = [(2.0, 3.0), (2.0, 5.0), (3.0, 4.0), (3.0, 5.0), (2.0, 7.0), (3.0, 7.0), (4.0, 5.0), (5.0, 6.0)];
            let (p, q) = KNOTS[(c.detail.max(1) as usize - 1) % 8];
            let (radius, wire, cy) = (h * 0.28, h * 0.055 * gi, -h * 0.5);
            // A (p, q) torus knot (as three.js's TorusKnotGeometry draws it).
            let samples = 260;
            let path: Vec<V3> = (0..samples)
                .map(|i| {
                    let u = TAU * p * i as f32 / samples as f32;
                    let qu = q / p * u;
                    let r = radius * (2.0 + qu.cos()) * 0.5;
                    [r * u.cos(), cy + r * u.sin(), radius * qu.sin() * 0.5]
                })
                .collect();
            body.parts.add("charm", tube(&path, 12, &SweepOptions::closed_loop(), |_| wire), c.material);
            body.top = cy + radius * 1.5 + wire * 0.5;
        }
        CharmShape::Idol => {
            let w = h * gi;
            let control: Vec<P2> = [
                [0.0, -1.0], [0.24, -0.97], [0.2, -0.8], [0.16, -0.62], [0.22, -0.45], [0.24, -0.32], [0.09, -0.25],
                [0.15, -0.2], [0.2, -0.12], [0.16, -0.04], [0.0, 0.0],
            ]
            .iter()
            .map(|&[r, y]| [r * w, y * h])
            .collect();
            let (sides, shading) = turned(c.facets);
            let figure = lathe(&spline_profile(&control, 64), &LatheOptions { facets: sides, shading, ..Default::default() });
            body.parts.add("charm", figure, c.material);
            for k in 0..c.bindings {
                bindings.append(&binding(-h * (0.5 + k as f32 * 0.07), w * 0.2, h));
            }
            if c.inset {
                // Stone eyes.
                let es = s.stone.size.min(h * 0.06);
                for side in [-1.0f32, 1.0] {
                    if keep_stone() {
                        let dir = vec3::normalize_or([side * 0.25, 0.0, 1.0], vec3::Z);
                        stones.append(&inset_stone(cut, es, [side * w * 0.065, -h * 0.13, w * 0.19], dir));
                    }
                }
            } else {
                device.append(&carving(c.device, h * 0.2, carve_depth, s.seed, [0.0, -h * 0.6, w * 0.17], vec3::Z));
            }
        }
        CharmShape::RuneStone => {
            let mut stone = icosphere(1.0, 3);
            let so = (s.seed % 17) as f32;
            let (sx, sy, sz) = (h * 0.38 * gi, h * 0.5, h * 0.17);
            for p in &mut stone.positions {
                let k = 1.0 + 0.14 * (value_noise3([p[0] * 2.2 + so, p[1] * 2.2, p[2] * 2.2], s.seed) - 0.5);
                *p = [p[0] * k * sx, p[1] * k * sy - h * 0.52, p[2] * k * sz];
            }
            stone.recompute_smooth_normals();
            // Seat the carving on the stone's actual face.
            let face_z = stone.positions.iter().filter(|p| p[0].abs() < h * 0.08 && (p[1] + h * 0.52).abs() < h * 0.08).fold(sz * 0.8, |m, p| m.max(p[2]));
            body.parts.add("charm", stone, c.material);
            device.append(&carving(c.device, h * 0.5, carve_depth, s.seed, [0.0, -h * 0.52, face_z - carve_depth * 0.3], vec3::Z));
            body.top = -h * 0.04;
        }
        CharmShape::Acorn => {
            let w = h * gi;
            let (sides, shading) = turned(c.facets);
            let scale = |pts: &[[f32; 2]]| pts.iter().map(|&[r, y]| [r * w, y * h]).collect::<Vec<P2>>();
            let nut = lathe(
                &spline_profile(&scale(&[[0.0, -1.0], [0.13, -0.96], [0.26, -0.78], [0.31, -0.55], [0.29, -0.4]]), 24),
                &LatheOptions { facets: sides, shading, ..Default::default() },
            );
            let mut cap = lathe(
                &spline_profile(&scale(&[[0.26, -0.43], [0.33, -0.38], [0.34, -0.3], [0.29, -0.2], [0.13, -0.11], [0.045, -0.07], [0.04, 0.0]]), 24),
                &LatheOptions { facets: sides, shading, ..Default::default() },
            );
            // A cupule is scaly: rough its surface with a little noise.
            for p in &mut cap.positions {
                let k = 1.0 + 0.04 * (value_noise3([p[0] * 90.0, p[1] * 90.0, p[2] * 90.0], s.seed) - 0.5);
                *p = [p[0] * k, p[1], p[2] * k];
            }
            body.parts.add("charm", nut, c.material);
            body.parts.add_tinted("cap", cap, c.material, s.palette.get(c.material).unwrap_or(s.palette.wood).scaled(0.8));
        }
        CharmShape::Bell => {
            let w = h * gi * 0.9;
            let scale = |pts: &[[f32; 2]]| pts.iter().map(|&[r, y]| [r * w, y * h]).collect::<Vec<P2>>();
            // A shell: down the inside from the crown, round the lip, up the outside.
            let mut profile = spline_profile(&scale(&[[0.0, -0.15], [0.09, -0.17], [0.16, -0.28], [0.18, -0.45], [0.23, -0.72], [0.34, -0.92], [0.36, -0.98]]), 24);
            profile.extend(spline_profile(&scale(&[[0.4, -1.0], [0.38, -0.93], [0.26, -0.72], [0.21, -0.45], [0.19, -0.25], [0.11, -0.13], [0.0, -0.1]]), 24));
            let (sides, shading) = turned(c.facets);
            let shell = lathe(&profile, &LatheOptions { facets: sides, shading, ..Default::default() });
            let mut crown = torus(h * 0.04, h * 0.012, 16, 8, Shading::SMOOTH);
            crown.rotate_x(FRAC_PI_2).translate([0.0, -h * 0.1 + h * 0.035, 0.0]);
            body.parts.add("charm", MeshBuilder::merged([&shell, &crown]), c.material);
            let mut clapper = uv_sphere(h * 0.07, 12, 8);
            clapper.translate([0.0, -h * 0.9, 0.0]);
            let mut rod = cylinder(h * 0.012, h * 0.72, 6, Shading::SMOOTH);
            rod.translate([0.0, -h * 0.88, 0.0]);
            body.parts.add_look("clapper", MeshBuilder::merged([&clapper, &rod]), Look::of(c.material).tinted(dark).without_outline());
            body.top = -h * 0.1 + h * 0.08;
        }
        CharmShape::Key => {
            // A ring-bowed key from the shared builder, its bow top at the origin, one ward per
            // bit (`detail`).
            let wards: Vec<f32> = (0..c.detail.clamp(1, 4)).map(|_| rng.range_f32(0.0, 1.0)).collect();
            let key = key(h, gi, KeyBow::Ring, &wards);
            let bar = h * 0.035 * gi;
            body.parts.add("charm", key, c.material);
            if c.inset && rng.chance(1.0 - (damage * 0.4) as f64) {
                let at = [0.0, collar_height(h, gi, KeyBow::Ring), bar * 1.2];
                stones.append(&inset_stone(cut, s.stone.size.min(h * 0.12), at, vec3::Z));
            }
            // The hanger threads the top of the bow.
            body.top = -bar * 0.6;
        }
    }

    let mut parts = body.parts;
    parts.add("bindings", bindings, Cord);
    parts.stone("stones", stones, s.stone.stone, 0.0, false);
    parts.add_look("device", device, Look::of(c.material).tinted(dark).without_outline());
    hanger(&mut parts, s, body.top);
    parts.finish()
}

/// The hanger above `top`: a jump ring, a stub of chain under a larger ring, or a cord loop
/// knotted at the charm with glass beads.
fn hanger(parts: &mut Parts, s: &CharmState, top: f32) {
    let hg = &s.hanger;
    let h = s.charm.size;
    let ls = hg.size;
    match hg.style {
        HangerStyle::JumpRing => {
            let t = (ls * 0.13).max(h * 0.012);
            let mut ring = jump_ring(ls * 0.5, t);
            ring.rotate_y(1.2).translate([0.0, top + ls * 0.5 - t * 0.6, 0.0]);
            parts.add("hanger", ring, hg.material);
        }
        HangerStyle::ChainStub => {
            let wire = (ls * 0.1).max(h * 0.008);
            let pitch = link_pitch(ls, wire);
            let start = top + ls * 0.5 - wire * 1.5;
            let path: Vec<V3> = (0..=3).map(|k| [0.0, start + pitch * k as f32, 0.0]).collect();
            let chain = cable_chain(&path, false, ls, wire, 4);
            let last_top = start + pitch * 3.0 + ls * 0.5;
            let r = ls * 0.9;
            let mut ring = jump_ring(r, wire * 1.2);
            ring.rotate_y(0.3).translate([0.0, last_top + r - wire * 2.0, 0.0]);
            parts.add_unoutlined("hanger", chain, hg.material);
            parts.add("hanger_ring", ring, hg.material);
        }
        HangerStyle::CordLoop => {
            let lh = ls * 4.5;
            let w = lh * 0.32;
            let r = (h * 0.012).max(ls * 0.09);
            let control: Vec<V3> = [[0.0, 0.0], [w, lh * 0.3], [w * 0.85, lh * 0.8], [0.0, lh], [-w * 0.85, lh * 0.8], [-w, lh * 0.3]]
                .iter()
                .map(|&[x, y]| [x, top + y, 0.0])
                .collect();
            let path = CatmullRom::centripetal(control).with_closed(true).sample(16);
            parts.add_unoutlined("hanger", twisted_cord(&path, true, r, 2), hg.material);
            let mut knot = uv_sphere(r * 1.9, 12, 8);
            knot.translate([0.0, top + r * 0.6, 0.0]);
            parts.add("knot", knot, hg.material);
            let even = resample_by_length(&path, 400, true);
            let mut beads = MeshBuilder::new();
            for k in 0..hg.beads {
                let t = 0.12 + (k / 2) as f32 * 0.07;
                let u = if k % 2 == 1 { 1.0 - t } else { t };
                let mut b = uv_sphere(r * 2.4, 12, 8);
                b.translate(even[((u * 400.0) as usize).min(399)]);
                beads.append(&b);
            }
            parts.stone("beads", beads, s.stone.stone, 0.0, true);
        }
    }
}
