//! Ammunition holders: quivers for arrows and bolt cases for crossbow bolts.
//!
//! A quiver is a tube of leather or bark on a shoulder strap, ringed with bands near the mouth
//! and the foot. A bolt case is a wooden box with a hinged lid, propped open, on a carrying
//! strap. Both show what they hold: the nock and fletching of each arrow or bolt stands out of
//! the mouth, built from the real ammunition states ([`AmmoState`]) in
//! [`QuiverState::contents`]. An empty holder shows nothing but its hollow.
//!
//! "Forge new" rolls an empty holder, so a holder is a pure function of its seed and what it
//! holds is the game's business. [`Quivers::stocked`] and [`BoltCases::stocked`] roll a holder
//! and fill it to a fraction of its [`capacity`](Quivers::capacity) with matching ammunition,
//! for the turntable and for loot; or push your own [`AmmoState`]s onto `contents`. Contents
//! beyond the capacity are not drawn.
//!
//! One state, [`QuiverState`], serves both families ([`Quivers`] and [`BoltCases`]). The
//! prototypes have none of this, so the sliders are new.
//!
//! Item space: standing on the ground (y = 0), centred over the origin; the strap loops out
//! toward +X.

use std::f32::consts::TAU;

use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use crate::build::parts::Parts;
use crate::build::strap::{rivets, strap};
use crate::cond::{Condition, ConditionLimits};
use crate::families::Generator;
use crate::families::ammo::{AmmoState, Arrows, Bolts};
use crate::geom::{CatmullRom, ItemMesh, LatheOptions, MeshBuilder, Shading, V3, cuboid, lathe, spline_profile, vec3};
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::slider::{Sliders, sliders};

use MaterialKind::{Brass, Iron, Leather, Wood};

/// The quiver generator.
pub struct Quivers;

/// The bolt case generator.
pub struct BoltCases;

/// The holder itself.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Holder {
    /// Height of the tube or the box.
    pub length: f32,
    /// A quiver's mouth across; a case's width.
    pub width: f32,
    /// A case's depth front to back (a quiver is round).
    pub depth: f32,
    /// How much narrower a quiver is at its foot than at its mouth.
    pub taper: f32,
    /// Width of the carrying strap.
    pub strap_width: f32,
    /// How far a case's lid is propped open, in radians (0 shut, about 1.6 upright).
    pub lid: f32,
    /// Leather or bark for a quiver, wood for a case.
    pub material: MaterialKind,
    /// The metal of the bands, clasp and hinges.
    pub trim: MaterialKind,
}

sliders! {
    HolderLimits for Holder {
        length: 2.0, 9.0, 0.05;
        width: 0.8, 2.6, 0.01;
        depth: 0.6, 1.8, 0.01;
        taper: 0.0, 0.45, 0.01;
        strap_width: 0.1, 0.35, 0.005;
        lid: 0.0, 2.0, 0.01;
    }
}

impl Default for Holder {
    fn default() -> Self {
        Holder { length: 5.6, width: 1.1, depth: 1.0, taper: 0.2, strap_width: 0.2, lid: 1.7, material: Leather, trim: Iron }
    }
}

/// Every parameter of one quiver or bolt case, and what is in it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct QuiverState {
    pub seed: u32,
    pub holder: Holder,
    pub cond: Condition,
    pub palette: Palette,
    /// The arrows or bolts inside, empty from "Forge new".
    #[serde(default)]
    pub contents: Vec<AmmoState>,
}

/// Slider limits for every holder section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuiverLimits {
    pub holder: HolderLimits,
    pub cond: ConditionLimits,
}

pub const LIMITS: QuiverLimits = QuiverLimits { holder: HolderLimits::SLIDERS, cond: ConditionLimits::SLIDERS };

impl QuiverState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &QuiverLimits) {
        self.holder.fit(&limits.holder);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &QuiverLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [tag("holder", self.holder.out_of_range(&limits.holder)), tag("cond", self.cond.out_of_range(&limits.cond))].concat()
    }
}

impl Generator for Quivers {
    type State = QuiverState;

    fn roll(seed: u32) -> QuiverState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let mut s = QuiverState { seed, ..QuiverState::default() };
        s.palette = *rng.pick(&Palette::PRESETS);
        let h = &mut s.holder;
        h.material = *rng.weighted(&[(Leather, 7), (Wood, 3)]);
        h.length = rng.range_f32(4.6, 7.4);
        h.width = rng.range_f32(1.1, 1.7);
        h.taper = rng.range_f32(0.05, 0.2);
        h.strap_width = rng.range_f32(0.14, 0.26);
        h.trim = *rng.weighted(&[(Iron, 3), (Brass, 2)]);
        s.fit(&LIMITS);
        s
    }

    fn noun(s: &QuiverState) -> String {
        match s.holder.material {
            Wood => "Bark Quiver",
            _ => "Leather Quiver",
        }
        .to_string()
    }

    fn build(s: &QuiverState) -> ItemMesh {
        build_quiver(s)
    }
}

impl Generator for BoltCases {
    type State = QuiverState;

    fn roll(seed: u32) -> QuiverState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let mut s = QuiverState { seed, ..QuiverState::default() };
        s.palette = *rng.pick(&Palette::PRESETS);
        let h = &mut s.holder;
        h.material = Wood;
        h.length = rng.range_f32(2.8, 4.0);
        h.width = rng.range_f32(1.5, 2.3);
        h.depth = rng.range_f32(0.8, 1.2);
        h.taper = 0.0;
        h.strap_width = rng.range_f32(0.14, 0.24);
        h.lid = rng.range_f32(1.4, 2.0);
        h.trim = *rng.weighted(&[(Iron, 3), (Brass, 2)]);
        s.fit(&LIMITS);
        s
    }

    fn noun(_: &QuiverState) -> String {
        "Bolt Case".to_string()
    }

    fn build(s: &QuiverState) -> ItemMesh {
        build_case(s)
    }
}

impl Quivers {
    /// How many arrows the quiver can show.
    pub fn capacity(s: &QuiverState) -> usize {
        quiver_slots(s).len()
    }

    /// A rolled quiver filled to `fullness` (0..=1) of its capacity with matching arrows, its
    /// length sized to them. Pure in `(seed, fullness)`.
    pub fn stocked(seed: u32, fullness: f32) -> QuiverState {
        let mut s = Quivers::roll(seed);
        stock(&mut s, Arrows::roll(seed ^ 0x51ED_A770), fullness, 0.72, Quivers::capacity);
        s
    }
}

impl BoltCases {
    /// How many bolts the case can show.
    pub fn capacity(s: &QuiverState) -> usize {
        case_slots(s).len()
    }

    /// A rolled bolt case filled to `fullness` (0..=1) of its capacity with matching bolts.
    /// Pure in `(seed, fullness)`.
    pub fn stocked(seed: u32, fullness: f32) -> QuiverState {
        let mut s = BoltCases::roll(seed);
        stock(&mut s, Bolts::roll(seed ^ 0xB017_5EED), fullness, 0.78, BoltCases::capacity);
        s
    }
}

/// Sizes the holder to `ammo`, then fills it with copies that differ a little.
fn stock(s: &mut QuiverState, ammo: AmmoState, fullness: f32, height_ratio: f32, capacity: impl Fn(&QuiverState) -> usize) {
    s.holder.length = ammo.reach() * height_ratio;
    s.fit(&LIMITS);
    let count = (capacity(s) as f32 * fullness.clamp(0.0, 1.0)).round() as usize;
    let mut rng = Mulberry32::new(armory_stream_seed(s.seed ^ 0xF111));
    s.contents = (0..count)
        .map(|i| {
            let mut a = ammo.clone();
            a.seed = ammo.seed.wrapping_add(i as u32);
            a.arrow.length += rng.range_f32(-0.12, 0.12);
            if rng.chance(0.15) {
                a.palette.fletching = *rng.pick(&crate::families::ammo::FLETCHINGS);
            }
            a
        })
        .collect();
}

// ───────────────────────── Geometry ─────────────────────────

const WALL: f32 = 0.06;
/// Room each piece of ammunition takes up across.
const SPACING: f32 = 0.27;

/// A quiver's radius at height `y`.
fn quiver_radius(h: &Holder, y: f32) -> f32 {
    let mouth = h.width * 0.5;
    let foot = mouth * (1.0 - h.taper);
    foot + (mouth - foot) * (y / h.length).clamp(0.0, 1.0).powf(0.8)
}

/// Where arrows stand in a quiver, centre first and then ring by ring.
fn quiver_slots(s: &QuiverState) -> Vec<[f32; 2]> {
    let h = &s.holder;
    // The vanes need the mouth's room; the shafts only the narrower tube low down.
    let room = (h.width * 0.5 - WALL - 0.15).min(quiver_radius(h, 0.7) - WALL - 0.05);
    let mut slots = vec![];
    if room < 0.0 {
        return slots;
    }
    slots.push([0.0, 0.0]);
    for ring in 1..=4 {
        let radius = SPACING * ring as f32;
        if radius > room {
            break;
        }
        let count = 6 * ring;
        slots.extend((0..count).map(|k| {
            let a = TAU * (k as f32 + 0.5 * (ring % 2) as f32) / count as f32;
            [radius * a.cos(), radius * a.sin()]
        }));
    }
    slots
}

/// Where bolts stand in a case, middle out.
fn case_slots(s: &QuiverState) -> Vec<[f32; 2]> {
    let h = &s.holder;
    let (ix, iz) = (h.width * 0.5 - WALL - 0.14, h.depth * 0.5 - WALL - 0.14);
    if ix < 0.0 || iz < 0.0 {
        return vec![];
    }
    let (nx, nz) = ((2.0 * ix / SPACING).floor() as usize + 1, (2.0 * iz / SPACING).floor() as usize + 1);
    let mut slots: Vec<[f32; 2]> = (0..nx)
        .flat_map(|i| {
            (0..nz).map(move |j| {
                let x = (i as f32 - (nx - 1) as f32 * 0.5) * SPACING;
                let z = (j as f32 - (nz - 1) as f32 * 0.5) * SPACING;
                [x, z]
            })
        })
        .collect();
    slots.sort_by(|a, b| (a[0] * a[0] + a[1] * a[1]).total_cmp(&(b[0] * b[0] + b[1] * b[1])));
    slots
}

/// Stands each piece of ammunition head-down in its slot, nock and vanes out of the mouth, and
/// groups the pieces by material (and vane colour) into `parts`. Heads are inside the holder
/// and hidden, so they are left out.
fn add_contents(s: &QuiverState, slots: &[[f32; 2]], mouth: f32, nocked: bool, parts: &mut Parts) {
    let mut rng = Mulberry32::new(armory_stream_seed(s.seed ^ 0xC0DE));
    let mut shafts = MeshBuilder::new();
    let mut nocks = MeshBuilder::new();
    let mut vanes: Vec<(Rgb, MeshBuilder)> = vec![];
    for (ammo, slot) in s.contents.iter().zip(slots) {
        let a = &ammo.arrow;
        // Stands on the foot, or rides high enough that its vanes clear the mouth.
        let top = (0.15 + ammo.reach()).max(mouth + a.fletch_length + 0.3) + rng.range_f32(0.0, 0.12);
        let at = [slot[0] + rng.range_f32(-0.03, 0.03), top, slot[1] + rng.range_f32(-0.03, 0.03)];
        let lean = [rng.range_f32(-0.02, 0.02), -1.0, rng.range_f32(-0.02, 0.02)];
        let m = ammo.meshes(nocked).placed(at, vec3::normalize_or(lean, vec3::neg(vec3::Y)), rng.range_f32(0.0, TAU));
        shafts.append(&m.shaft);
        nocks.append(&m.nock);
        let colour = ammo.palette.fletching;
        match vanes.iter_mut().find(|(c, _)| *c == colour) {
            Some((_, mesh)) => {
                mesh.append(&m.fletching);
            }
            None => vanes.push((colour, m.fletching)),
        }
    }
    let wood = s.contents.first().map_or(Wood, |a| a.arrow.material);
    parts.add("ammo_shafts", shafts, wood);
    parts.add("ammo_nocks", nocks, MaterialKind::Bone);
    for (colour, mesh) in vanes {
        parts.add_tinted("ammo_fletching", mesh, MaterialKind::Cloth, colour);
    }
}

/// A flat band round the quiver at height `y`, a hair proud of its wall.
fn band(h: &Holder, y: f32, width: f32) -> MeshBuilder {
    let r = quiver_radius(h, y);
    let ring: Vec<V3> = (0..24).map(|k| [r * (TAU * k as f32 / 24.0).cos(), y, -r * (TAU * k as f32 / 24.0).sin()]).collect();
    let faces: Vec<V3> = ring.iter().map(|p| vec3::normalize_or([p[0], 0.0, p[2]], vec3::X)).collect();
    strap(&ring, &faces, width, 0.035, true)
}

/// A carrying strap looping out of two points on the right of the holder.
fn carry_strap(top: V3, bottom: V3, reach: f32, width: f32) -> MeshBuilder {
    let mid = vec3::lerp(top, bottom, 0.5);
    let out = |p: V3, d: f32| [p[0] + d, p[1], p[2]];
    let curve = CatmullRom::centripetal(vec![
        top,
        out(top, reach * 0.35),
        out(vec3::lerp(top, mid, 0.5), reach * 0.85),
        out(mid, reach),
        out(vec3::lerp(bottom, mid, 0.5), reach * 0.85),
        out(bottom, reach * 0.35),
        bottom,
    ]);
    let path = curve.sample(8);
    let centre = out(mid, reach * 0.45);
    let faces: Vec<V3> = path.iter().map(|p| vec3::normalize_or(vec3::sub(centre, *p), vec3::X)).collect();
    strap(&path, &faces, width, 0.04, false)
}

fn build_quiver(s: &QuiverState) -> ItemMesh {
    let h = &s.holder;
    let (len, mouth, foot) = (h.length, h.width * 0.5, quiver_radius(h, 0.0));
    let mut parts = Parts::new();

    // The tube: outside, rim, inside and floor are separate surfaces so the rim stays crisp.
    let outer: Vec<[f32; 2]> = (0..=10).map(|i| {
        let y = len * i as f32 / 10.0;
        [quiver_radius(h, y), y]
    }).collect();
    let opts = LatheOptions { facets: 20, shading: Shading::SMOOTH, cap_start: true, cap_end: false, phase: 0.0 };
    let mut body = lathe(&spline_profile(&outer, 20), &opts);
    body.append(&lathe(&[[mouth, len], [mouth - WALL, len]], &LatheOptions { facets: 20, shading: Shading::BANDED, ..opts }));
    let inner: Vec<[f32; 2]> = (0..=10).rev().map(|i| {
        let y = len * i as f32 / 10.0;
        [(quiver_radius(h, y) - WALL).max(0.05), y.max(0.12)]
    }).collect();
    body.append(&lathe(&spline_profile(&inner, 20), &opts));
    body.append(&lathe(&[[foot - WALL, 0.12], [0.0, 0.12]], &LatheOptions { shading: Shading::BANDED, ..opts }));
    parts.add("quiver", body, h.material);

    // Bands at the mouth, the middle and the foot, and a cap under the foot.
    let (top_y, mid_y, low_y) = (len * 0.9, len * 0.5, len * 0.12);
    let mut bands = band(h, top_y, 0.26);
    bands.append(&band(h, low_y, 0.22));
    parts.add("bands", bands, h.trim);
    parts.add("belt", band(h, mid_y, 0.16), Leather);

    // The shoulder strap runs from the upper band to the lower, out to the right.
    let top = [quiver_radius(h, top_y), top_y, 0.0];
    let bottom = [quiver_radius(h, low_y), low_y, 0.0];
    parts.add("strap", carry_strap(top, bottom, len * 0.22 + 0.5, h.strap_width), Leather);
    parts.add("rivets", rivets(&[(top, vec3::X), (bottom, vec3::X)], 0.05), h.trim);

    let slots = quiver_slots(s);
    add_contents(s, &slots, len, true, &mut parts);
    parts.finish()
}

fn build_case(s: &QuiverState) -> ItemMesh {
    let h = &s.holder;
    let (w, d, len) = (h.width, h.depth, h.length);
    let t = WALL * 1.4;
    let mut parts = Parts::new();

    // An open box: floor, two long sides, front and back.
    let mut wood = cuboid([w, t, d]);
    wood.translate([0.0, t * 0.5, 0.0]);
    let wall = |size: V3, at: V3| {
        let mut m = cuboid(size);
        m.translate(at);
        m
    };
    wood.append(&wall([t, len, d], [-(w - t) * 0.5, len * 0.5, 0.0]));
    wood.append(&wall([t, len, d], [(w - t) * 0.5, len * 0.5, 0.0]));
    wood.append(&wall([w - 2.0 * t, len, t], [0.0, len * 0.5, (d - t) * 0.5]));
    wood.append(&wall([w - 2.0 * t, len, t], [0.0, len * 0.5, -(d - t) * 0.5]));
    parts.add("case", wood, h.material);

    // The lid hinges on the back edge and is propped open.
    let lid_t = t * 1.3;
    let mut lid = cuboid([w + 0.06, lid_t, d + 0.06]);
    lid.translate([0.0, lid_t * 0.5, (d + 0.06) * 0.5]);
    lid.rotate_x(-h.lid);
    lid.translate([0.0, len, -d * 0.5]);
    parts.add("lid", lid, h.material);

    // Iron bands round the box, hinge plates and a clasp on the front.
    let mut metal = MeshBuilder::new();
    for y in [len * 0.18, len * 0.82] {
        metal.append(&wall([w + 0.03, 0.14, d + 0.03], [0.0, y, 0.0]));
    }
    for x in [-w * 0.3, w * 0.3] {
        let mut plate = cuboid([0.3, 0.05, 0.4]);
        plate.translate([0.0, 0.0, 0.2]);
        plate.rotate_x(-h.lid);
        plate.translate([x, len + lid_t, -d * 0.5]);
        metal.append(&plate);
    }
    metal.append(&wall([0.22, 0.3, 0.05], [0.0, len * 0.82, d * 0.5 + 0.05]));
    parts.add("fittings", metal, h.trim);

    // A carrying strap down both ends.
    let top = [w * 0.5 + 0.02, len * 0.8, 0.0];
    let bottom = [w * 0.5 + 0.02, len * 0.22, 0.0];
    parts.add("strap", carry_strap(top, bottom, 0.9, h.strap_width), Leather);

    let slots = case_slots(s);
    add_contents(s, &slots, len, false, &mut parts);
    parts.finish()
}
