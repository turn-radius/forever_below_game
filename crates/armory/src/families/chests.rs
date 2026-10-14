//! Chests and containers: treasure chests, strongboxes, jewel coffers, barrels, crates and
//! amphorae.
//!
//! Sliders, options and the noun come from `reference/spline_armory_engine.js` (the `chest`
//! section, `BASE.chest`, `PRESETS.chests`, the `fam==='chests'` branch of `randomize()`,
//! `buildChest()` and `forgeNoun()`).
//!
//! The roll departs from the prototype on purpose. Each style rolls only the options it
//! shows: studs, a lock and handles on lidded boxes, iron hoops on barrels, shipping straps
//! and rope handles on crates (the prototype ignored `bands` and `handles` there), a lid on
//! an amphora (its `lock`). Coffers and strongboxes can stand open too, and a coffer's
//! finial stone is a [`Gemstone`] rather than the second colour. Stencils go on barrels as
//! well as crates.
//!
//! The geometry is rebuilt rather than ported: a lidded box is hollow, with walls, a rim
//! and a floor, so an open lid shows a real inside; wooden walls are planked; bands wrap
//! under the box and over the lid; the lid turns on hinges and carries its hasp; barrels
//! are staves with a sunk head; crates are boards behind a braced frame. [`Chests::container`]
//! says where the mouth is and how much it holds.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::build::container::{ContainerSpace, Opening, stacked_volume};
use crate::build::device::device_mesh;
use crate::build::fitting::{drop_handle, hasp, lock_plate, padlock};
use crate::build::gem::{bezel, cut_stone};
use crate::build::hoard::{coin, coin_heap};
use crate::build::parts::{Look, Parts};
use crate::build::plank::{plank, planked_disc, planked_panel};
use crate::build::strap::{rivets, strap};
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::gem::{GemCut, Gemstone};
use crate::geom::{
    CatmullRom, GridOptions, ItemMesh, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, cuboid, cylinder, extrude_smooth,
    frustum, lathe, spline_profile, stitch_grid, thick_sheet, tube, uv_sphere,
};
use crate::geom::vec3;
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::sections::device::Device;
use crate::fabric::{Fabric, FabricLimits, FabricPattern};
use crate::slider::{Sliders, sliders};
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Brass, Gold, Iron, Silver, Stone, Wood};

/// The chest generator.
pub struct Chests;

/// The prototype's `chest.style`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChestStyle {
    /// A long wooden chest with a rounded lid.
    Chest,
    /// A small iron box with a flat lid.
    Strongbox,
    /// A small precious box with a hipped lid, on feet.
    Coffer,
    Barrel,
    Crate,
    /// A two-handled clay jar (the prototype's `urn`).
    Amphora,
}

impl ChestStyle {
    pub const ALL: [ChestStyle; 6] = [ChestStyle::Chest, ChestStyle::Strongbox, ChestStyle::Coffer, ChestStyle::Barrel, ChestStyle::Crate, ChestStyle::Amphora];

    /// A box with a hinged lid (chest, strongbox, coffer).
    pub fn is_lidded_box(self) -> bool {
        matches!(self, ChestStyle::Chest | ChestStyle::Strongbox | ChestStyle::Coffer)
    }
}

/// The prototype's `chest` section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chest {
    pub style: ChestStyle,
    pub size: f32,
    /// Wider (1) or narrower (0).
    pub shape: f32,
    /// How far the lid stands open, 0 shut to 1 thrown back.
    pub open: f32,
    /// Heaped with coins and stones (shows when open).
    pub treasure: bool,
    /// Iron bands on a box, hoops on a barrel, shipping straps on a crate.
    pub bands: u32,
    /// Studs along the bands.
    pub studs: bool,
    /// A lock on a box, a lid on an amphora.
    pub lock: bool,
    /// Drop handles on a box, rope handles on a crate, the handles of an amphora.
    pub handles: bool,
    /// The body: wood, iron, a precious metal, or clay (stone) for an amphora.
    pub material: MaterialKind,
    /// Bands, hoops, locks, hinges and handles.
    pub fittings: MaterialKind,
    /// A stencil painted on a crate or barrel.
    pub device: Device,
    /// An amphora's glaze: its colour, and the pattern painted over it in a second colour.
    pub glaze: Fabric,
    /// The stone crowning a coffer's lid.
    pub stone: Gemstone,
}

sliders! {
    ChestLimits for Chest {
        size: 0.7, 1.4, 0.01;
        shape: 0.0, 1.0, 0.01;
        open: 0.0, 1.0, 0.01;
        bands: 0.0, 5.0, 1.0;
    }
}

impl Default for Chest {
    /// The prototype's `BASE.chest`: a shut, iron-bound wooden chest.
    fn default() -> Self {
        Chest {
            style: ChestStyle::Chest,
            size: 1.0,
            shape: 0.5,
            open: 0.0,
            treasure: false,
            bands: 3,
            studs: true,
            lock: true,
            handles: true,
            material: Wood,
            fittings: Iron,
            device: Device::None,
            glaze: Fabric { color: Rgb::hex(0xb8673a), pattern: FabricPattern::Hoops, color2: Rgb::hex(0x2a1d14), scale: 3.0 },
            stone: Gemstone::Garnet,
        }
    }
}

/// Every parameter of one chest.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChestState {
    pub seed: u32,
    pub chest: Chest,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every chest section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChestsLimits {
    pub chest: ChestLimits,
    pub glaze: FabricLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges (it has no `RANGES.chests`).
pub const LIMITS: ChestsLimits = ChestsLimits { chest: ChestLimits::SLIDERS, glaze: FabricLimits::SLIDERS, etch: EtchingLimits::SLIDERS, cond: ConditionLimits::SLIDERS };

impl ChestState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &ChestsLimits) {
        self.chest.fit(&limits.chest);
        self.chest.glaze.fit(&limits.glaze);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &ChestsLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("chest", self.chest.out_of_range(&limits.chest)),
            tag("chest.glaze", self.chest.glaze.out_of_range(&limits.glaze)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

/// A lid open less than this counts as shut.
const SHUT: f32 = 0.05;

impl Chests {
    /// Roll weights per style, in percent. Chests come up twice as often, as in the
    /// prototype's pick list.
    pub const STYLE_WEIGHTS: [(ChestStyle, u32); 6] = [
        (ChestStyle::Chest, 28),
        (ChestStyle::Strongbox, 14),
        (ChestStyle::Coffer, 14),
        (ChestStyle::Barrel, 15),
        (ChestStyle::Crate, 15),
        (ChestStyle::Amphora, 14),
    ];

    /// "Forge new" forced to one style, for turntables and tests. Draws the same stream as
    /// [`Generator::roll`], so `roll(s)` equals `roll_as(s, st)` when `st` is what `s` rolls.
    pub fn roll_as(seed: u32, style: ChestStyle) -> ChestState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, style)
    }

    /// Whether the lid stands open far enough to see inside.
    pub fn is_open(s: &ChestState) -> bool {
        s.chest.style.is_lidded_box() && s.chest.open > SHUT
    }

    /// The container's mouth and inside, in the same item space as [`Generator::build`].
    pub fn container(s: &ChestState) -> ContainerSpace {
        let c = &s.chest;
        match c.style {
            ChestStyle::Chest | ChestStyle::Strongbox | ChestStyle::Coffer => {
                let b = BoxDims::of(c);
                let (iw, id) = (b.plan.half_w - b.wall, b.plan.half_d - b.wall);
                let rows = [b.plan.ring(b.base + b.wall, b.wall), b.plan.ring(b.top(), b.wall)];
                ContainerSpace {
                    opening: Opening::upward([0.0, b.top(), 0.0], [iw, id], false),
                    interior_min: [-iw, b.base + b.wall, -id],
                    interior_max: [iw, b.top(), id],
                    litres: stacked_volume(&rows, f32::MAX),
                }
            }
            ChestStyle::Barrel => {
                let b = BarrelDims::of(c);
                let (low, high) = (b.head_inset, b.height - b.head_inset);
                let rows: Vec<Vec<V3>> = (0..=16).map(|i| low + (high - low) * i as f32 / 16.0).map(|y| circle(b.radius_at(y) - b.wall, y, 48)).collect();
                let r = b.radius - b.wall;
                ContainerSpace {
                    opening: Opening::upward([0.0, high, 0.0], [b.radius_at(high) - b.wall; 2], true),
                    interior_min: [-r, low, -r],
                    interior_max: [r, high, r],
                    litres: stacked_volume(&rows, f32::MAX),
                }
            }
            ChestStyle::Crate => {
                let b = CrateDims::of(c);
                let (iw, ih, id) = (b.width * 0.5 - b.board, b.height - b.board, b.depth * 0.5 - b.board);
                ContainerSpace {
                    opening: Opening::upward([0.0, b.height, 0.0], [iw, id], false),
                    interior_min: [-iw, b.board, -id],
                    interior_max: [iw, ih, id],
                    litres: 2.0 * iw * 2.0 * id * (ih - b.board),
                }
            }
            ChestStyle::Amphora => {
                let a = AmphoraDims::of(c);
                let inner: Vec<Vec<V3>> = a.outer.iter().filter(|p| p[1] > a.floor && p[1] < a.neck).map(|&[r, y]| circle((r - a.wall).max(0.0), y, 48)).collect();
                let r = a.belly - a.wall;
                ContainerSpace {
                    opening: Opening::upward([0.0, a.height, 0.0], [a.mouth; 2], true),
                    interior_min: [-r, a.floor, -r],
                    interior_max: [r, a.neck, r],
                    litres: stacked_volume(&inner, f32::MAX),
                }
            }
        }
    }
}

impl Generator for Chests {
    type State = ChestState;

    fn roll(seed: u32) -> ChestState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&Chests::STYLE_WEIGHTS);
        compose(&mut rng, seed, style)
    }

    /// `forgeNoun()` for chests.
    fn noun(s: &ChestState) -> String {
        let c = &s.chest;
        match c.style {
            ChestStyle::Chest if c.treasure && c.open > SHUT => "Treasure Chest",
            ChestStyle::Chest => "Iron-bound Chest",
            ChestStyle::Strongbox => "Strongbox",
            ChestStyle::Coffer => "Jewel Coffer",
            ChestStyle::Barrel => "Barrel",
            ChestStyle::Crate => "Supply Crate",
            ChestStyle::Amphora => "Amphora",
        }
        .to_string()
    }

    /// Item space: +Y up from the floor it stands on, the front (lock, bung, stencil side
    /// aside) faces +Z, a lid hinges at the back.
    fn build(s: &ChestState) -> ItemMesh {
        let mut parts = Parts::new();
        match s.chest.style {
            ChestStyle::Chest | ChestStyle::Strongbox | ChestStyle::Coffer => build_box(s, &mut parts),
            ChestStyle::Barrel => build_barrel(s, &mut parts),
            ChestStyle::Crate => build_crate(s, &mut parts),
            ChestStyle::Amphora => build_amphora(s, &mut parts),
        }
        let mut mesh = parts.finish();
        // Paint and keyholes lie flush on a surface: an ink hull round them would halo.
        for part in &mut mesh.parts {
            if matches!(part.name, "device" | "keyhole") {
                part.outline = false;
            }
        }
        mesh
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, style: ChestStyle) -> ChestState {
    use ChestStyle::*;
    let mut s = ChestState { seed, ..ChestState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    s.chest.nudge(rng, &LIMITS.chest, 0.75, 0.3);
    s.chest.glaze.nudge(rng, &LIMITS.glaze, 0.75, 0.3);
    let c = &mut s.chest;
    c.style = style;
    c.size = rng.range_f32(0.85, 1.15);
    c.open = match style {
        Chest if rng.chance(0.5) => rng.range_f32(0.35, 0.75),
        Coffer if rng.chance(0.3) => rng.range_f32(0.5, 0.9),
        Strongbox if rng.chance(0.2) => rng.range_f32(0.4, 0.7),
        _ => 0.0,
    };
    c.treasure = c.open > SHUT && rng.chance(0.8);
    c.bands = match style {
        Chest => *rng.weighted(&[(0, 1), (1, 1), (2, 3), (3, 4), (4, 2)]),
        Strongbox => rng.int(2, 3),
        Coffer => rng.int(0, 2),
        Barrel => rng.int(2, 5),
        Crate => *rng.weighted(&[(0, 3), (1, 1), (2, 2)]),
        Amphora => 0,
    };
    c.studs = c.bands > 0 && rng.chance(match style {
        Chest => 0.6,
        Strongbox => 0.8,
        Coffer => 0.2,
        _ => 0.0,
    });
    c.lock = rng.chance(match style {
        Chest => 0.85,
        Strongbox => 1.0,
        Coffer => 0.9,
        Amphora => 0.5,
        _ => 0.0,
    });
    c.handles = match style {
        Chest => rng.chance(0.85),
        Strongbox => rng.chance(0.9),
        Crate => rng.chance(0.5),
        Amphora => true,
        _ => false,
    };
    c.material = match style {
        Strongbox => Iron,
        Coffer => *rng.weighted(&[(Brass, 3), (Gold, 2), (Silver, 2), (Wood, 3)]),
        Amphora => Stone,
        _ => Wood,
    };
    c.fittings = *rng.weighted(match style {
        Coffer => &[(Gold, 3), (Brass, 2), (Silver, 2)][..],
        Strongbox => &[(Iron, 4), (Brass, 1)],
        _ => &[(Iron, 4), (Brass, 1), (Gold, 1)],
    });
    // Coffers of precious metal are trimmed in another one.
    if style == Coffer && c.fittings == c.material {
        c.fittings = if c.material == Gold { Silver } else { Gold };
    }
    let stencils = [(Device::Tower, 2), (Device::Cross, 2), (Device::Star, 2), (Device::Wheel, 2), (Device::Crescent, 1), (Device::Rune, 1)];
    c.device = match style {
        Crate if rng.chance(0.8) => *rng.weighted(&stencils),
        Barrel if rng.chance(0.3) => *rng.weighted(&stencils),
        _ => Device::None,
    };
    c.glaze.color = Rgb::hex(*rng.pick(&[0xb8673a, 0xc98a5a, 0x8a5a3a, 0xd8c8a8]));
    c.glaze.pattern = *rng.weighted(&[
        (FabricPattern::Hoops, 4),
        (FabricPattern::Chevrons, 2),
        (FabricPattern::Plain, 3),
        (FabricPattern::Lozenges, 2),
        (FabricPattern::Stripes, 1),
        (FabricPattern::Powdered, 1),
    ]);
    c.glaze.color2 = Rgb::hex(*rng.pick(&[0x2a1d14, 0xa8323a, 0x2c4e8c, 0xd4a838]));
    c.glaze.scale = rng.range_f32(2.0, 4.0);
    let stones: Vec<Gemstone> = Gemstone::ALL.into_iter().filter(|g| g.suits(GemCut::Cabochon)).collect();
    c.stone = *rng.pick(&stones);

    let mut targets = vec![(EtchTarget::Metals, 3)];
    if c.material == Wood {
        targets.push((EtchTarget::Wood, 1));
    }
    let odds = match style {
        Amphora => 0.0,
        Coffer => 0.5,
        _ => 0.2,
    };
    s.etch = Etching::roll(rng, odds, &targets, if style == Coffer { 2 } else { 1 });
    s.fit(&LIMITS);
    s
}

// ---------------------------------------------------------------------------------------
// Shared bits.

/// `sign(c) * |c|^e`.
fn signed_pow(c: f32, e: f32) -> f32 {
    c.signum() * c.abs().powf(e)
}

/// A horizontal circle of `n` points at height `y`, turning from +X toward -Z.
fn circle(r: f32, y: f32, n: usize) -> Vec<V3> {
    (0..n).map(|i| {
        let a = TAU * i as f32 / n as f32;
        [r * a.cos(), y, -r * a.sin()]
    }).collect()
}

/// A darker shade of a part's colour, for insides and shadowed boards.
fn shade(palette: &Palette, material: MaterialKind, factor: f32) -> Rgb {
    palette.get(material).unwrap_or(Rgb::hex(0x6b5038)).scaled(factor)
}

/// The colour of a keyhole's dark recess.
const KEYHOLE: Rgb = Rgb::hex(0x15110d);
/// The colour of a stencil's paint.
const STENCIL: Rgb = Rgb::hex(0x2a221c);

// ---------------------------------------------------------------------------------------
// Chests, strongboxes and coffers.

/// A rounded-rectangle plan: a superellipse of exponent 10, nearly square at the corners.
#[derive(Clone, Copy)]
struct Plan {
    half_w: f32,
    half_d: f32,
}

impl Plan {
    const EXPONENT: f32 = 10.0;
    const AROUND: usize = 96;

    /// The point at plan angle `a` (0 at +X, PI/2 at the front), `inset` in from the outside.
    fn point(&self, a: f32, y: f32, inset: f32) -> V3 {
        let e = 2.0 / Plan::EXPONENT;
        [(self.half_w - inset) * signed_pow(a.cos(), e), y, (self.half_d - inset) * signed_pow(a.sin(), e)]
    }

    /// A ring of points at `y`, clockwise from above, so rows of rings stitched upward face out.
    fn ring(&self, y: f32, inset: f32) -> Vec<V3> {
        (0..Plan::AROUND).map(|c| self.point(-TAU * c as f32 / Plan::AROUND as f32, y, inset)).collect()
    }

    /// The outward normal of the plan at a point on it.
    fn normal(&self, p: V3) -> V3 {
        let k = Plan::EXPONENT - 1.0;
        let gx = signed_pow(p[0] / self.half_w, k) / self.half_w;
        let gz = signed_pow(p[2] / self.half_d, k) / self.half_d;
        vec3::normalize_or([gx, 0.0, gz], vec3::Z)
    }

    /// How far forward the front face is at `x`.
    fn front_z(&self, x: f32) -> f32 {
        let t = (x / self.half_w).abs().min(1.0);
        self.half_d * (1.0 - t.powf(Plan::EXPONENT)).max(0.0).powf(1.0 / Plan::EXPONENT)
    }
}

/// A lidded box's measurements (the prototype's `W`, `D`, `H`, `Lh` and `yb`).
struct BoxDims {
    plan: Plan,
    height: f32,
    lid: f32,
    /// Bottom of the walls: up on feet for a coffer, up by the bands' thickness when bands
    /// run underneath.
    base: f32,
    wall: f32,
    band_t: f32,
    size: f32,
}

impl BoxDims {
    fn of(c: &Chest) -> BoxDims {
        let sz = c.size;
        let (w, d, h, lid) = match c.style {
            ChestStyle::Strongbox => (3.4, 2.4, 2.1, 0.35),
            ChestStyle::Coffer => (2.4, 1.6, 1.3, 0.75),
            _ => (6.0, 3.6, 3.0, 1.3),
        };
        let band_t = 0.05 * sz;
        let feet = if c.style == ChestStyle::Coffer { 0.3 * sz } else { 0.0 };
        let base = feet + if c.bands > 0 && c.style != ChestStyle::Coffer { band_t } else { 0.0 };
        let wall = if c.style == ChestStyle::Strongbox { 0.07 * sz } else { 0.1 * sz };
        BoxDims {
            plan: Plan { half_w: w * sz * (0.85 + c.shape * 0.3) * 0.5, half_d: d * sz * 0.5 },
            height: h * sz,
            lid: lid * sz,
            base,
            wall,
            band_t,
            size: sz,
        }
    }

    fn top(&self) -> f32 {
        self.base + self.height
    }

    /// Takes a mesh built in lid space (base on y = 0, centred on the plan) to where the
    /// lid stands, turned back `open` on its hinges along the top of the back wall.
    fn place_lid(&self, mesh: &mut MeshBuilder, open: f32) {
        mesh.translate([0.0, 0.0, self.plan.half_d]).rotate_x(-open * 1.85).translate([0.0, self.top(), -self.plan.half_d]);
    }
}

/// The lid's cross-section `[z, y]` from its front foot over to its back foot, in lid space.
fn lid_profile(style: ChestStyle, b: &BoxDims) -> Vec<[f32; 2]> {
    let (d, h) = (b.plan.half_d, b.lid);
    match style {
        ChestStyle::Chest => (0..=32).map(|i| {
            let a = PI * i as f32 / 32.0;
            [a.cos() * d, a.sin() * h]
        }).collect(),
        ChestStyle::Strongbox => vec![[d, 0.0], [d, h * 0.9], [d * 0.9, h], [-d * 0.9, h], [-d, h * 0.9], [-d, 0.0]],
        _ => {
            // A hipped lid: steep eaves rising to a small plateau.
            vec![[d, 0.0], [d, h * 0.12], [d * 0.3, h * 0.85], [d * 0.25, h], [-d * 0.25, h], [-d * 0.3, h * 0.85], [-d, h * 0.12], [-d, 0.0]]
        }
    }
}

/// Outward normals of a lid profile, one per point (averaged at corners).
fn profile_normals(profile: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let n = profile.len();
    let edge = |i: usize| {
        let (a, b) = (profile[i], profile[i + 1]);
        // Walking front to back over the top, the outside is on the left.
        let (dz, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dz * dz + dy * dy).sqrt().max(1e-6);
        [dy / len, -dz / len]
    };
    (0..n)
        .map(|i| {
            let e = match (i.checked_sub(1).map(edge), (i + 1 < n).then(|| edge(i))) {
                (Some(a), Some(b)) => [a[0] + b[0], a[1] + b[1]],
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => [0.0, 1.0],
            };
            let len = (e[0] * e[0] + e[1] * e[1]).sqrt().max(1e-6);
            [e[0] / len, e[1] / len]
        })
        .collect()
}

/// A band over the lid at `x`, in lid space: along the profile from front foot to back foot.
fn lid_band(profile: &[[f32; 2]], x: f32, width: f32, thickness: f32) -> MeshBuilder {
    let normals = profile_normals(profile);
    let path: Vec<V3> = profile.iter().map(|&[z, y]| [x, y, z]).collect();
    let faces: Vec<V3> = normals.iter().map(|&[nz, ny]| [0.0, ny, nz]).collect();
    // Corners of a faceted lid would pinch a band; resample so it bends over them smoothly.
    let path = crate::geom::resample_by_length(&CatmullRom::centripetal(path).sample(6), 64, false);
    let faces: Vec<V3> = path
        .iter()
        .map(|p| {
            let i = profile.iter().enumerate().min_by(|a, b| {
                let da = (a.1[0] - p[2]).powi(2) + (a.1[1] - p[1]).powi(2);
                let db = (b.1[0] - p[2]).powi(2) + (b.1[1] - p[1]).powi(2);
                da.total_cmp(&db)
            }).map_or(0, |(i, _)| i);
            faces[i]
        })
        .collect();
    strap(&path, &faces, width, thickness, false)
}

/// The lid's solid, in lid space.
fn lid_mesh(style: ChestStyle, b: &BoxDims) -> MeshBuilder {
    let profile = lid_profile(style, b);
    let width = b.plan.half_w * 2.0;
    match style {
        ChestStyle::Coffer => {
            // A hipped roof: four sloping faces to a plateau, square in plan, then stretched
            // to the box's proportions.
            let r = b.plan.half_w * std::f32::consts::SQRT_2;
            let h = b.lid;
            let profile = [[r, 0.0], [r, h * 0.12], [r * 0.3, h * 0.85], [r * 0.25, h]];
            let mut roof = lathe(&profile, &LatheOptions { facets: 4, shading: Shading::FLAT, cap_start: true, cap_end: true, phase: PI * 0.25 });
            roof.scale([1.0, 1.0, b.plan.half_d / b.plan.half_w]);
            roof
        }
        _ => {
            let outline: Vec<[f32; 2]> = profile.to_vec();
            let n = outline.len();
            let sharp: Vec<bool> = match style {
                ChestStyle::Chest => (0..n).map(|i| i == 0 || i == n - 1).collect(),
                _ => vec![true; n],
            };
            let mut lid = extrude_smooth(&outline, &sharp, width);
            // Outline x is the lid's z; the extrusion's z runs across the box.
            lid.rotate_y(-FRAC_PI_2);
            lid
        }
    }
}

fn build_box(s: &ChestState, parts: &mut Parts) {
    let c = &s.chest;
    let b = BoxDims::of(c);
    let sz = b.size;
    let plan = b.plan;
    let open = if c.open > SHUT { c.open } else { 0.0 };

    // Walls: planked outside on a wooden box, a rim, then the inside and its floor.
    let wooden = c.material == Wood;
    let mut ys = vec![b.base];
    if wooden {
        let planks = ((b.height / (0.6 * sz)).round() as u32).max(2);
        let groove = 0.03 * sz;
        for k in 1..planks {
            let y = b.base + b.height * k as f32 / planks as f32;
            ys.extend([y - groove, y, y + groove]);
        }
    }
    ys.push(b.top());
    let groove_depth = 0.02 * sz;
    let rows: Vec<Vec<V3>> = ys.iter().enumerate().map(|(i, &y)| plan.ring(y, if wooden && i % 3 == 2 && i + 1 < ys.len() { groove_depth } else { 0.0 })).collect();
    let mut body = stitch_grid(&rows, &GridOptions { wrap_around: true, wrap_along: false, shading: Shading::BANDED, row_v: None });
    let top_in = plan.ring(b.top(), b.wall);
    body.append(&stitch_grid(&[rows[rows.len() - 1].clone(), top_in.clone()], &GridOptions { wrap_around: true, ..GridOptions::default() }));
    body.push_fan_cap(&rows[0], vec3::neg(vec3::Y));
    parts.add("body", body, c.material);

    let floor = plan.ring(b.base + b.wall, b.wall);
    let mut inside = stitch_grid(&[floor.clone(), top_in], &GridOptions { wrap_around: true, shading: Shading::SMOOTH, ..GridOptions::default() });
    inside.flip();
    inside.push_fan_cap(&floor, vec3::Y);
    parts.add_tinted("interior", inside, c.material, shade(&s.palette, c.material, 0.45));

    // The lid and everything that turns with it.
    let profile = lid_profile(c.style, &b);
    let mut lid = lid_mesh(c.style, &b);
    let mut lid_bands = MeshBuilder::new();
    let mut lid_studs = MeshBuilder::new();
    let mut lid_fittings = MeshBuilder::new();

    let mut bands = MeshBuilder::new();
    let mut studs = MeshBuilder::new();
    let band_w = 0.24 * sz;
    let n = c.bands;
    let band_xs: Vec<f32> = (0..n).map(|i| if n == 1 { 0.0 } else { (-1.0 + 2.0 * i as f32 / (n - 1) as f32) * plan.half_w * 0.76 }).collect();
    for &x in &band_xs {
        bands.append(&body_band(&plan, &b, x, band_w));
        if c.style != ChestStyle::Coffer {
            lid_bands.append(&lid_band(&profile, x, band_w, b.band_t));
        }
        if c.studs {
            let zf = plan.front_z(x) + b.band_t;
            let spots: Vec<(V3, V3)> = (0..4)
                .flat_map(|k| {
                    let y = b.base + b.height * (0.15 + 0.23 * k as f32);
                    [([x, y, zf], vec3::Z), ([x, y, -zf], vec3::neg(vec3::Z))]
                })
                .collect();
            studs.append(&rivets(&spots, 0.05 * sz));
            if c.style == ChestStyle::Chest {
                let normals = profile_normals(&profile);
                let spots: Vec<(V3, V3)> = [8usize, 16, 24]
                    .iter()
                    .map(|&i| {
                        let ([z, y], [nz, ny]) = (profile[i], normals[i]);
                        ([x, y + ny * b.band_t, z + nz * b.band_t], [0.0, ny, nz])
                    })
                    .collect();
                lid_studs.append(&rivets(&spots, 0.05 * sz));
            }
        }
    }
    // Rims round the foot and the mouth.
    if c.bands > 0 || c.style == ChestStyle::Strongbox {
        for y in [b.base + 0.08 * sz, b.top() - 0.08 * sz] {
            let ring = plan.ring(y, 0.0);
            let faces: Vec<V3> = ring.iter().map(|&p| plan.normal(p)).collect();
            bands.append(&strap(&ring, &faces, 0.12 * sz, 0.035 * sz, true));
        }
    }

    // Hinges: a knuckle on the pivot, a leaf down the back wall and one up the lid.
    let mut hinges = MeshBuilder::new();
    for x in [-plan.half_w * 0.45, plan.half_w * 0.45] {
        let r = 0.05 * sz;
        let mut knuckle = cylinder(r, 0.3 * sz, 12, Shading::BANDED);
        knuckle.translate([0.0, -0.15 * sz, 0.0]).rotate_z(FRAC_PI_2).translate([x, b.top(), -plan.half_d - r * 0.6]);
        hinges.append(&knuckle);
        let leaf_len = (0.45 * sz).min(b.height * 0.4);
        let leaf: Vec<V3> = (0..=4).map(|i| [x, b.top() - leaf_len * i as f32 / 4.0, -plan.front_z(x)]).collect();
        hinges.append(&strap(&leaf, &vec![vec3::neg(vec3::Z); leaf.len()], 0.16 * sz, 0.03 * sz, false));
        let back: Vec<[f32; 2]> = profile.iter().rev().copied().collect();
        let normals: Vec<[f32; 2]> = profile_normals(&profile).into_iter().rev().collect();
        // A coffer's hipped roof also slopes sideways, so its leaf stops at the eave.
        let reach = if c.style == ChestStyle::Coffer { b.lid * 0.12 } else { (0.45 * sz).min(b.lid * 1.4) };
        let mut path = Vec::new();
        let mut faces = Vec::new();
        let mut walked = 0.0;
        for i in 0..back.len() {
            if i > 0 {
                walked += ((back[i][0] - back[i - 1][0]).powi(2) + (back[i][1] - back[i - 1][1]).powi(2)).sqrt();
            }
            path.push([x, back[i][1], back[i][0]]);
            faces.push([0.0, normals[i][1], normals[i][0]]);
            if walked >= reach - 1e-4 {
                break;
            }
        }
        lid_fittings.append(&strap(&path, &faces, 0.16 * sz, 0.03 * sz, false));
    }

    // Handles on the ends.
    let mut handles = MeshBuilder::new();
    if c.handles && c.style != ChestStyle::Coffer {
        for side in [-1.0f32, 1.0] {
            let mut h = drop_handle(0.5 * sz, 0.045 * sz);
            h.place_basis([side * plan.half_w, b.base + b.height * 0.72, 0.0], vec3::Y, [side, 0.0, 0.0]);
            handles.append(&h);
        }
    }

    // Lock plate on the front, hasp on the lid, padlock on a shut strongbox.
    let mut lock = MeshBuilder::new();
    let mut keyhole = MeshBuilder::new();
    if c.lock {
        let on_band = band_xs.iter().any(|x| x.abs() < 0.2 * sz);
        let proud = plan.front_z(0.0) + if on_band { b.band_t } else { 0.0 };
        let (pw, ph) = if c.style == ChestStyle::Coffer { (0.3 * sz, 0.36 * sz) } else { (0.5 * sz, 0.6 * sz) };
        let (mut plate, mut recess) = lock_plate(pw, ph, 0.04 * sz);
        let at = [0.0, b.top() - ph * 0.5 - 0.12 * sz, proud];
        plate.place_basis(at, vec3::Y, vec3::Z);
        recess.place_basis(at, vec3::Y, vec3::Z);
        lock.append(&plate);
        keyhole.append(&recess);
        let hasp_len = ph * 0.5 + 0.2 * sz;
        let mut h = hasp(pw * 0.36, hasp_len, 0.03 * sz);
        let lid_front = profile[0][0] + if on_band && c.style != ChestStyle::Coffer { b.band_t } else { 0.0 };
        h.place_basis([0.0, 0.08 * sz, lid_front + 0.01 * sz], vec3::Y, vec3::Z);
        lid_fittings.append(&h);
        if c.style == ChestStyle::Strongbox && open == 0.0 {
            let mut p = padlock(0.32 * sz);
            p.translate([0.0, b.top() + 0.08 * sz - hasp_len + pw * 0.36 * 0.6, proud + 0.12 * sz]);
            lock.append(&p);
        }
    }

    // A coffer's feet and its crowning stone.
    if c.style == ChestStyle::Coffer {
        let mut feet = MeshBuilder::new();
        let r = 0.17 * sz;
        for (sx, sz_) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let mut ball = uv_sphere(r, 16, 10);
            ball.translate([sx * plan.half_w * 0.8, r, sz_ * plan.half_d * 0.75]);
            feet.append(&ball);
        }
        parts.add("feet", feet, c.fittings);
        let size = 0.16 * sz;
        let mut stone = cut_stone(GemCut::Cabochon, size);
        stone.translate([0.0, b.lid, 0.0]);
        let mut setting = bezel(size, 24, Shading::SMOOTH);
        setting.translate([0.0, b.lid, 0.0]);
        b.place_lid(&mut stone, open);
        // The setting turns with the lid's other fittings below.
        lid_fittings.append(&setting);
        parts.stone("gem", stone, c.stone, 0.15, true);
    }

    for mesh in [&mut lid, &mut lid_bands, &mut lid_studs, &mut lid_fittings] {
        b.place_lid(mesh, open);
    }
    parts.add("lid", lid, c.material);
    bands.append(&lid_bands);
    parts.add("bands", bands, c.fittings);
    studs.append(&lid_studs);
    parts.add("studs", studs, c.fittings);
    hinges.append(&lid_fittings);
    parts.add("hinges", hinges, c.fittings);
    parts.add("handles", handles, c.fittings);
    parts.add("lock", lock, c.fittings);
    parts.add_tinted("keyhole", keyhole, Iron, KEYHOLE);

    if open > 0.0 && c.treasure {
        add_treasure(s, &b, parts);
    }
}

/// A band at `x` down the front, under the box and up the back, turning the bottom edges.
fn body_band(plan: &Plan, b: &BoxDims, x: f32, width: f32) -> MeshBuilder {
    let zf = plan.front_z(x);
    let t = b.band_t;
    let (top, base) = (b.top() - 0.02 * b.size, b.base);
    let r = t * 1.5;
    let mut path = Vec::new();
    let mut faces = Vec::new();
    let mut push = |p: V3, n: V3| {
        path.push(p);
        faces.push(n);
    };
    for i in 0..=6 {
        push([x, top - (top - base - r) * i as f32 / 6.0, zf], vec3::Z);
    }
    for i in 1..=4 {
        let a = FRAC_PI_2 * i as f32 / 4.0;
        let n = [0.0, -a.sin(), a.cos()];
        push(vec3::add([x, base + r, zf - r], vec3::scale(n, r)), n);
    }
    for i in 1..8 {
        push([x, base, (zf - r) * (1.0 - 2.0 * i as f32 / 8.0)], vec3::neg(vec3::Y));
    }
    for i in 0..=4 {
        let a = FRAC_PI_2 * i as f32 / 4.0;
        let n = [0.0, -a.cos(), -a.sin()];
        push(vec3::add([x, base + r, -(zf - r)], vec3::scale(n, r)), n);
    }
    for i in 1..=6 {
        push([x, base + r + (top - base - r) * i as f32 / 6.0, -zf], vec3::neg(vec3::Z));
    }
    // The band is lifted off the walls by its own thickness only at the foot; up the walls it
    // lies on them.
    strap(&path, &faces, width, t, false)
}

/// A heap of coins in an open box, coins strewn over it and a few loose stones.
fn add_treasure(s: &ChestState, b: &BoxDims, parts: &mut Parts) {
    let sz = b.size;
    let mut rng = Mulberry32::new(s.seed.wrapping_mul(197).wrapping_add(11));
    let (hx, hz) = ((b.plan.half_w - b.wall) * 0.94, (b.plan.half_d - b.wall) * 0.9);
    // Heaped to just crest the rim, as a hoard should.
    let hy = (b.height - b.wall) * 1.04;
    let floor = b.base + b.wall;
    let coin_r = 0.1 * sz;
    let mut hoard = coin_heap([hx, hy, hz], coin_r, s.seed);
    hoard.translate([0.0, floor, 0.0]);
    let surface = |x: f32, z: f32| -> (V3, V3) {
        let q = (1.0 - (x / hx).powi(2) - (z / hz).powi(2)).max(0.0);
        let y = hy * q.sqrt();
        let n = vec3::normalize_or([x / (hx * hx), y / (hy * hy), z / (hz * hz)], vec3::Y);
        ([x, floor + y, z], n)
    };
    let one = coin(coin_r);
    let count = if s.chest.style == ChestStyle::Coffer { 8 } else { 16 };
    for _ in 0..count {
        let (a, r) = (rng.range_f32(0.0, TAU), rng.range_f32(0.0, 0.8).sqrt());
        let (p, n) = surface(a.cos() * r * hx, a.sin() * r * hz);
        let mut c = one.clone();
        c.rotate_y(rng.range_f32(0.0, TAU)).rotate_x(rng.range_f32(-0.3, 0.3)).place(vec3::add(p, vec3::scale(n, coin_r * 0.1)), n);
        hoard.append(&c);
    }
    parts.add("treasure", hoard, Gold);
    let cuts = [GemCut::Brilliant, GemCut::Rose, GemCut::Octahedron];
    let stones = if s.chest.style == ChestStyle::Coffer { 6 } else { 4 };
    for _ in 0..stones {
        let cut = *rng.pick(&cuts);
        let choices: Vec<Gemstone> = Gemstone::ALL.into_iter().filter(|g| g.suits(cut)).collect();
        let stone = *rng.pick(&choices);
        let size = 0.12 * sz;
        let (a, r) = (rng.range_f32(0.0, TAU), rng.range_f32(0.0, 0.6).sqrt());
        let (p, n) = surface(a.cos() * r * hx, a.sin() * r * hz);
        let mut g = cut_stone(cut, size);
        g.rotate_x(rng.range_f32(-0.8, 0.8)).rotate_y(rng.range_f32(0.0, TAU)).place(vec3::add(p, vec3::scale(n, size * 0.3)), n);
        parts.stone("treasure_gem", g, stone, 0.1, true);
    }
}

// ---------------------------------------------------------------------------------------
// Barrels.

struct BarrelDims {
    radius: f32,
    height: f32,
    wall: f32,
    /// How far the heads sit in from the ends of the staves.
    head_inset: f32,
    size: f32,
}

impl BarrelDims {
    fn of(c: &Chest) -> BarrelDims {
        let sz = c.size;
        BarrelDims { radius: 2.4 * sz * (0.85 + c.shape * 0.3), height: 6.8 * sz, wall: 0.12 * sz, head_inset: 0.25 * sz, size: sz }
    }

    /// The staves' outside radius at `y`: narrow at the heads, full at the bilge.
    fn radius_at(&self, y: f32) -> f32 {
        self.radius * (0.86 + 0.14 * (PI * (y / self.height).clamp(0.0, 1.0)).sin())
    }

    fn slope_at(&self, y: f32) -> f32 {
        self.radius * 0.14 * PI / self.height * (PI * y / self.height).cos()
    }

    /// The outward normal of the staves at angle `a` (from +X toward -Z) and height `y`.
    fn normal(&self, a: f32, y: f32) -> V3 {
        vec3::normalize_or([a.cos(), -self.slope_at(y), -a.sin()], [a.cos(), 0.0, -a.sin()])
    }
}

fn build_barrel(s: &ChestState, parts: &mut Parts) {
    let c = &s.chest;
    let b = BarrelDims::of(c);
    let sz = b.size;
    let mut rng = Mulberry32::new(s.seed.wrapping_mul(197).wrapping_add(11));

    // Staves: each a slightly crowned board with a groove where it meets the next.
    let staves = ((TAU * b.radius / (0.95 * sz)).round() as usize).clamp(12, 30);
    let swell: Vec<f32> = (0..staves).map(|_| rng.range_f32(-0.006, 0.006)).collect();
    let step = TAU / staves as f32;
    let columns: Vec<(f32, f32)> = (0..staves)
        .flat_map(|k| {
            let a = k as f32 * step;
            let j = 1.0 + swell[k];
            [(a, 0.985), (a + step * 0.1, j), (a + step * 0.5, j * 1.004), (a + step * 0.9, j)]
        })
        .collect();
    // The stave ends are bevelled where they meet the chime, as coopers cut them. Square, the
    // ends' thin rim gave the ink outline a normal that barely rose off the wall, so the far
    // rim drew almost no line.
    let bevel = b.wall * 0.6;
    let mut levels: Vec<(f32, f32)> = vec![(0.0, bevel)];
    levels.extend((0..=28).map(|i| (bevel + (b.height - 2.0 * bevel) * i as f32 / 28.0, 0.0)));
    levels.push((b.height, bevel));
    let rows: Vec<Vec<V3>> = levels
        .iter()
        .map(|&(y, inset)| {
            let r = b.radius_at(y) - inset;
            columns.iter().map(|&(a, k)| [r * k * a.cos(), y, -r * k * a.sin()]).collect()
        })
        .collect();
    let options = GridOptions { wrap_around: true, wrap_along: false, shading: Shading::SMOOTH, row_v: None };
    parts.add("body", thick_sheet(&rows, &options, b.wall), c.material);

    // Heads sunk in the chimes. They meet the staves all round, never at a silhouette, so they
    // draw no ink: their outline hull poked through the thin staves above them in dashes.
    let mut heads = MeshBuilder::new();
    for y in [b.head_inset, b.height - b.head_inset] {
        let mut head = planked_disc(b.radius_at(y) - b.wall * 1.02, 5, 0.015 * sz, 0.1 * sz);
        head.translate([0.0, y, 0.0]);
        heads.append(&head);
    }
    parts.add_look("head", heads, Look::of(c.material).tinted(shade(&s.palette, c.material, 0.9)).without_outline());

    // Hoops: chime hoops at the ends, bilge hoops toward the middle.
    let hoops: &[f32] = match c.bands.max(2) {
        2 => &[0.1, 0.9],
        3 => &[0.08, 0.2, 0.92],
        4 => &[0.06, 0.2, 0.8, 0.94],
        _ => &[0.06, 0.2, 0.44, 0.8, 0.94],
    };
    let mut bands = MeshBuilder::new();
    for &t in hoops {
        let y = b.height * t;
        let r = b.radius_at(y) * 1.008;
        let ring: Vec<V3> = circle(r, y, 72);
        let faces: Vec<V3> = (0..72).map(|i| b.normal(TAU * i as f32 / 72.0, y)).collect();
        bands.append(&strap(&ring, &faces, 0.22 * sz, 0.03 * sz, true));
    }
    parts.add("bands", bands, c.fittings);

    // The bung on the front of the bilge.
    let y = b.height * 0.55;
    let mut bung = frustum(0.14 * sz, 0.12 * sz, 0.07 * sz, 16, Shading::BANDED);
    bung.place([0.0, y, b.radius_at(y) - 0.01 * sz], vec3::Z);
    parts.add("bung", bung, Wood);

    // A stencil wrapped onto the staves below the bung.
    if c.device != Device::None {
        let yc = b.height * 0.33;
        let mut decal = device_mesh(c.device, b.height * 0.14, 0.008 * sz, s.seed);
        let rc = b.radius_at(yc);
        for (p, n) in decal.positions.iter_mut().zip(decal.normals.iter_mut()) {
            let y = p[1] + yc;
            let theta = p[0] / rc;
            let r = b.radius_at(y) * 1.004 + p[2];
            let (st, ct) = theta.sin_cos();
            *p = [r * st, y, r * ct];
            *n = [n[0] * ct + n[2] * st, n[1], -n[0] * st + n[2] * ct];
        }
        parts.add_tinted("device", decal, MaterialKind::Paint, STENCIL);
    }
}

// ---------------------------------------------------------------------------------------
// Crates.

struct CrateDims {
    width: f32,
    height: f32,
    depth: f32,
    /// Board thickness.
    board: f32,
    /// Batten width and thickness.
    slat: f32,
    batten: f32,
    size: f32,
}

impl CrateDims {
    fn of(c: &Chest) -> CrateDims {
        let sz = c.size;
        CrateDims {
            width: 4.6 * sz * (0.85 + c.shape * 0.3),
            height: 4.2 * sz,
            depth: 4.2 * sz,
            board: 0.08 * sz,
            slat: 0.32 * sz,
            batten: 0.1 * sz,
            size: sz,
        }
    }
}

fn build_crate(s: &ChestState, parts: &mut Parts) {
    let c = &s.chest;
    let k = CrateDims::of(c);
    let (w, h, d, bt, sl, t, sz) = (k.width, k.height, k.depth, k.board, k.slat, k.batten, k.size);
    let gap = 0.025 * sz;
    let bevel = 0.012 * sz;
    let rows = ((h / (0.55 * sz)).round() as u32).max(4);

    let mut core = cuboid([w - 2.0 * bt, h - 2.0 * bt, d - 2.0 * bt]);
    core.translate([0.0, h * 0.5, 0.0]);
    parts.add_tinted("interior", core, c.material, shade(&s.palette, c.material, 0.35));

    // Boards on the four walls and the top.
    let mut boards = MeshBuilder::new();
    for side in [-1.0f32, 1.0] {
        let mut wall = planked_panel(w, h, rows, gap, bt, bevel);
        wall.translate([0.0, h * 0.5, side * (d * 0.5 - bt * 0.5)]);
        boards.append(&wall);
        let mut end = planked_panel(d - 2.0 * bt, h, rows, gap, bt, bevel);
        end.rotate_y(FRAC_PI_2).translate([side * (w * 0.5 - bt * 0.5), h * 0.5, 0.0]);
        boards.append(&end);
    }
    let lid_rows = ((d / (0.55 * sz)).round() as u32).max(4);
    let mut lid = planked_panel(w - 2.0 * bt, d - 2.0 * bt, lid_rows, gap, bt, bevel);
    lid.rotate_x(-FRAC_PI_2).translate([0.0, h - bt * 0.5, 0.0]);
    boards.append(&lid);
    parts.add("boards", boards, c.material);

    // The frame: battens round each face, a brace across the front and back, two over the top.
    let mut slats = MeshBuilder::new();
    let mut batten = |len: f32, width: f32, thick: f32, origin: V3, along: V3, face: V3| {
        let mut p = plank(len, width, thick, bevel);
        // Plank runs along X with its face along Z: map X onto `along`, Z onto `face`.
        let up = vec3::cross(face, along);
        p.place_basis(origin, up, face);
        slats.append(&p);
    };
    for side in [-1.0f32, 1.0] {
        let z = side * (d * 0.5 + t * 0.5);
        let face = [0.0, 0.0, side];
        batten(w, sl, t, [0.0, sl * 0.5, z], vec3::X, face);
        batten(w, sl, t, [0.0, h - sl * 0.5, z], vec3::X, face);
        for sx in [-1.0f32, 1.0] {
            batten(h - 2.0 * sl, sl, t, [sx * (w * 0.5 - sl * 0.5), h * 0.5, z], vec3::Y, face);
        }
        let (dw, dh) = (w - 2.0 * sl, h - 2.0 * sl);
        let along = vec3::normalize_or([dw, side * dh, 0.0], vec3::X);
        batten((dw * dw + dh * dh).sqrt() - sl * 0.6, sl * 0.85, t * 0.9, [0.0, h * 0.5, z - side * t * 0.05], along, face);

        let x = side * (w * 0.5 + t * 0.5);
        let face = [side, 0.0, 0.0];
        batten(d + 2.0 * t, sl, t, [x, sl * 0.5, 0.0], vec3::Z, face);
        batten(d + 2.0 * t, sl, t, [x, h - sl * 0.5, 0.0], vec3::Z, face);
        for sz_ in [-1.0f32, 1.0] {
            batten(h - 2.0 * sl, sl, t, [x, h * 0.5, sz_ * (d * 0.5 + t - sl * 0.5)], vec3::Y, face);
        }
        batten(w, sl, t, [0.0, h + t * 0.5, side * (d * 0.5 - sl * 0.5)], vec3::X, vec3::Y);
    }
    parts.add_tinted("slats", slats, c.material, shade(&s.palette, c.material, 0.8));

    // Iron shipping straps round the crate, over the top and under it.
    if c.bands > 0 {
        let mut bands = MeshBuilder::new();
        let xs: &[f32] = if c.bands == 1 { &[0.0] } else { &[-0.3, 0.3] };
        let strap_t = 0.025 * sz;
        let (hz, top, bottom, r) = (d * 0.5 + t, h + t, strap_t, 0.12 * sz);
        for &f in xs {
            let x = f * w;
            // A rounded rectangle in the YZ plane round the outside of the frame.
            let corners = [(hz - r, top - r, 0.0), (-(hz - r), top - r, FRAC_PI_2), (-(hz - r), bottom + r, PI), (hz - r, bottom + r, PI * 1.5)];
            let mut path = Vec::new();
            let mut faces = Vec::new();
            for (cz, cy, a0) in corners {
                for i in 0..=6 {
                    let a = a0 + FRAC_PI_2 * i as f32 / 6.0;
                    let n = [0.0, a.sin(), a.cos()];
                    path.push([x, cy + r * a.sin(), cz + r * a.cos()]);
                    faces.push(n);
                }
            }
            // Clockwise round +X, so reverse to keep the strap's face out.
            bands.append(&strap(&path, &faces, 0.14 * sz, strap_t, true));
        }
        parts.add("bands", bands, c.fittings);
    }

    // Rope handles through the ends.
    if c.handles {
        let mut ropes = MeshBuilder::new();
        let (y, span) = (h * 0.75, 0.7 * sz);
        for side in [-1.0f32, 1.0] {
            let out = w * 0.5 + t;
            let control = vec![
                [side * (w * 0.5 - 0.1 * sz), y, -span * 0.5],
                [side * (out + 0.06 * sz), y, -span * 0.5],
                [side * (out + 0.14 * sz), y - 0.3 * sz, 0.0],
                [side * (out + 0.06 * sz), y, span * 0.5],
                [side * (w * 0.5 - 0.1 * sz), y, span * 0.5],
            ];
            let path = CatmullRom::centripetal(control).sample(8);
            ropes.append(&tube(&path, 8, &SweepOptions::default(), |_| 0.05 * sz));
        }
        parts.add("handles", ropes, MaterialKind::Cord);
    }

    // The stencil on the left end, between the battens.
    if c.device != Device::None {
        let mut decal = device_mesh(c.device, (h - 2.0 * sl) * 0.7, 0.008 * sz, s.seed);
        decal.place_basis([-(w * 0.5) - 0.001, h * 0.5, 0.0], vec3::Y, vec3::neg(vec3::X));
        parts.add_tinted("device", decal, MaterialKind::Paint, STENCIL);
    }
}

// ---------------------------------------------------------------------------------------
// Amphorae.

struct AmphoraDims {
    belly: f32,
    height: f32,
    wall: f32,
    /// Inner floor and the top of the neck's straight part.
    floor: f32,
    neck: f32,
    mouth: f32,
    /// The outside profile `[radius, y]`, foot to lip.
    outer: Vec<[f32; 2]>,
    size: f32,
}

impl AmphoraDims {
    fn of(c: &Chest) -> AmphoraDims {
        let sz = c.size;
        let r = 1.7 * sz * (0.85 + c.shape * 0.3);
        let h = 5.2 * sz;
        let control = [
            [r * 0.38, 0.0], [r * 0.4, h * 0.03], [r * 0.34, h * 0.07], [r * 0.6, h * 0.18], [r * 0.95, h * 0.38], [r, h * 0.5],
            [r * 0.8, h * 0.68], [r * 0.36, h * 0.82], [r * 0.3, h * 0.9], [r * 0.4, h * 0.97], [r * 0.42, h],
        ];
        let outer = spline_profile(&control, 64);
        let wall = 0.08 * sz;
        AmphoraDims { belly: r, height: h, wall, floor: h * 0.05, neck: h * 0.9, mouth: r * 0.42 - wall * 1.5, outer, size: sz }
    }
}

fn build_amphora(s: &ChestState, parts: &mut Parts) {
    let c = &s.chest;
    let a = AmphoraDims::of(c);
    let (r, h, sz) = (a.belly, a.height, a.size);
    let glaze = Look::cloth(&c.glaze).made_of(Stone);
    // Outside up to the lip, over it, and down the inside of the neck to a floor in its throat.
    let mut profile: Vec<[f32; 2]> = vec![[0.0, 0.0]];
    profile.extend(a.outer.iter().copied());
    let lip_in = r * 0.42 - a.wall * 1.5;
    let throat = [
        [lip_in + a.wall * 0.6, h + a.wall * 0.15],
        [lip_in, h - a.wall * 0.4],
        [lip_in * 0.95, h * 0.94],
        [r * 0.3 - a.wall, h * 0.9],
        [r * 0.32 - a.wall, h * 0.86],
        [0.0, h * 0.85],
    ];
    profile.extend(throat);
    let body = lathe(&profile, &LatheOptions { facets: 48, shading: Shading::SMOOTH, cap_start: false, cap_end: false, phase: 0.0 });
    parts.add_look("body", body, glaze);

    if c.handles {
        let mut handles = MeshBuilder::new();
        for side in [-1.0f32, 1.0] {
            let control = vec![[side * r * 0.26, h * 0.88, 0.0], [side * r * 0.75, h * 0.95, 0.0], [side * r * 0.86, h * 0.78, 0.0], [side * r * 0.62, h * 0.65, 0.0]];
            let path = CatmullRom::centripetal(control).sample(12);
            handles.append(&tube(&path, 12, &SweepOptions::default(), |_| 0.09 * sz));
        }
        parts.add_look("handles", handles, glaze);
    }

    if c.lock {
        let lid = [[r * 0.45, 0.0], [r * 0.44, r * 0.05], [r * 0.3, r * 0.12], [r * 0.12, r * 0.15], [r * 0.1, r * 0.24], [r * 0.06, r * 0.28], [0.0, r * 0.28]];
        let mut stopper = lathe(&spline_profile(&lid, 24), &LatheOptions { facets: 40, shading: Shading::SMOOTH, cap_start: true, cap_end: false, phase: 0.0 });
        stopper.translate([0.0, h + a.wall * 0.15, 0.0]);
        parts.add_look("lid", stopper, glaze);
    }
}
