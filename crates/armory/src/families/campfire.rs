//! Campfire kits: a ring of stones round a bed of ash, logs stacked crosswise with kindling
//! leaned over them, and an optional pot hook or tripod with an iron pot hanging over the fire;
//! or the kit packed, a bundle of sticks tied up with the rig laid along it.
//!
//! Neither form is in the prototypes; the sliders here are new. Item space: the fire rests on
//! the ground (y = 0) centred over the origin, a packed bundle lies along X. A tripod stands
//! about 7 units (70 cm) at the apex at the default size, and the pot hangs 3 units over the
//! ash.
//!
//! The rig and pot only draw together: a pot with nothing to hang it from is left out.

use std::f32::consts::{PI, TAU};

use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use crate::build::chain::cable_chain;
use crate::build::light::{CHAR, FLAME_LIGHT, LightAnchor, add_flame};
use crate::build::parts::{Look, Parts};
use crate::cond::{Condition, ConditionLimits};
use crate::families::Generator;
use crate::families::tent::rod;
use crate::geom::{ItemMesh, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, frustum, icosphere, lathe, spline_profile, torus, tube, vec3};
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::slider::{Sliders, sliders};

use MaterialKind::{Cord, Iron, Stone, Wood};

/// The campfire generator.
pub struct Campfire;

/// Set down and built, or packed up to carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FireForm {
    Set,
    Packed,
}

/// What hangs the pot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rig {
    /// Nothing: just a fire.
    Bare,
    /// An iron post with a swinging arm over the fire.
    Hook,
    /// Three poles lashed at the top.
    Tripod,
}

/// The fire itself.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hearth {
    pub form: FireForm,
    pub rig: Rig,
    /// An iron pot on the rig.
    pub pot: bool,
    /// The fire is burning.
    pub lit: bool,
    /// Overall scale.
    pub size: f32,
    /// Stones in the ring.
    pub stones: u32,
    /// Logs stacked crosswise, two to a layer.
    pub logs: u32,
    /// Kindling sticks leaned over the logs.
    pub kindling: u32,
    /// How far the wood has burnt: charred logs on a grey bed.
    pub ash: f32,
}

sliders! {
    HearthLimits for Hearth {
        size: 0.8, 2.2, 0.01;
        stones: 6.0, 14.0, 1.0;
        logs: 0.0, 8.0, 1.0;
        kindling: 0.0, 10.0, 1.0;
        ash: 0.0, 1.0, 0.01;
    }
}

impl Default for Hearth {
    fn default() -> Self {
        Hearth { form: FireForm::Set, rig: Rig::Tripod, pot: true, lit: false, size: 1.0, stones: 9, logs: 4, kindling: 6, ash: 0.0 }
    }
}

/// Every parameter of one campfire kit.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CampfireState {
    pub seed: u32,
    pub fire: Hearth,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every campfire section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CampfireLimits {
    pub fire: HearthLimits,
    pub cond: ConditionLimits,
}

/// The campfires' slider ranges (new: the prototypes have none).
pub const LIMITS: CampfireLimits = CampfireLimits { fire: HearthLimits::SLIDERS, cond: ConditionLimits::SLIDERS };

impl CampfireState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &CampfireLimits) {
        self.fire.fit(&limits.fire);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &CampfireLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [tag("fire", self.fire.out_of_range(&limits.fire)), tag("cond", self.cond.out_of_range(&limits.cond))].concat()
    }
}

/// How far a lit campfire lights, in feet (our own: brighter than a torch's 20).
pub const FIRE_FEET: f32 = 40.0;

impl Generator for Campfire {
    type State = CampfireState;

    fn roll(seed: u32) -> CampfireState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let form = *rng.weighted(&[(FireForm::Set, 1), (FireForm::Packed, 1)]);
        compose(&mut rng, seed, form)
    }

    fn noun(s: &CampfireState) -> String {
        let f = &s.fire;
        match (f.form, f.rig, f.pot) {
            (FireForm::Packed, ..) => "Campfire Kit",
            (FireForm::Set, Rig::Bare, _) | (FireForm::Set, _, false) => "Campfire",
            (FireForm::Set, Rig::Hook, true) => "Cookfire on a Hook",
            (FireForm::Set, Rig::Tripod, true) => "Cookfire on a Tripod",
        }
        .to_string()
    }

    fn build(s: &CampfireState) -> ItemMesh {
        let mut parts = Parts::new();
        match s.fire.form {
            FireForm::Set => set_fire(s, &mut parts),
            FireForm::Packed => packed_kit(s, &mut parts),
        }
        parts.rest_on_ground();
        parts.finish()
    }
}

impl Campfire {
    /// "Forge new" forced to a form, for turntables, bases and tests. Draws the same stream
    /// as [`Generator::roll`].
    pub fn roll_as(seed: u32, form: FireForm) -> CampfireState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, form)
    }

    /// Where a lit, set fire gives its light: the heart of the flame, or above the rig over one. `None` otherwise.
    pub fn light(s: &CampfireState) -> Option<LightAnchor> {
        let f = &s.fire;
        // Over a tripod or hook the light hangs above the rig, so the pot does not throw its
        // shadow over the ceiling; a bare fire's sits at the heart of the flame.
        let height = if f.rig == Rig::Bare { 1.6 } else { 8.0 };
        (f.form == FireForm::Set && f.lit).then(|| LightAnchor::feet([0.0, height * f.size, 0.0], FLAME_LIGHT, FIRE_FEET, 0.7))
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, form: FireForm) -> CampfireState {
    let mut s = CampfireState { seed, ..CampfireState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    let f = &mut s.fire;
    f.form = form;
    f.rig = *rng.weighted(&[(Rig::Bare, 1), (Rig::Hook, 2), (Rig::Tripod, 3)]);
    f.pot = f.rig != Rig::Bare && rng.chance(0.75);
    f.size = rng.range_f32(0.9, 1.15);
    f.stones = rng.int(7, 12);
    f.logs = rng.int(3, 6);
    f.kindling = rng.int(3, 8);
    f.lit = form == FireForm::Set && rng.chance(0.4);
    f.ash = if form == FireForm::Set { rng.range_f32(0.0, 0.6) } else { 0.0 };
    s.fit(&LIMITS);
    s
}

// ───────────────────────── Geometry ─────────────────────────

const ASH: Rgb = Rgb::hex(0x4a4640);
const COLD_ASH: Rgb = Rgb::hex(0x8a857b);

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    Rgb(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}

/// An iron cooking pot standing on y = 0: a round belly, a rolled lip, a bail arching over it.
fn pot(size: f32) -> (MeshBuilder, MeshBuilder) {
    let s = size;
    let outer = spline_profile(&[[0.0, 0.0], [0.7 * s, 0.02 * s], [1.05 * s, 0.35 * s], [1.15 * s, 0.8 * s], [1.0 * s, 1.15 * s], [1.05 * s, 1.2 * s]], 24);
    let mut profile = outer.clone();
    // The inside, down to a floor a little above the base.
    profile.extend(outer.iter().rev().skip(1).map(|&[r, y]| [(r - 0.08 * s).max(0.0), (y + 0.06 * s).min(1.18 * s)]));
    let body = lathe(&profile, &LatheOptions::new(32, Shading::SMOOTH));
    let bail: Vec<V3> = (0..=16)
        .map(|i| {
            let a = PI * i as f32 / 16.0;
            [a.cos() * 1.05 * s, 1.15 * s + a.sin() * 0.95 * s, 0.0]
        })
        .collect();
    let bail = tube(&bail, 6, &SweepOptions::default(), |_| 0.04 * s);
    (body, bail)
}

/// The hook: an iron post by the fire with an arm swung out over it. Returns the iron and
/// the point the chain hangs from.
fn hook_rig(s: f32, ring: f32) -> (MeshBuilder, V3) {
    let (top, x) = (6.6 * s, -(ring + 0.6 * s));
    let mut iron = rod([x, 0.0, 0.0], [x, top + 0.3 * s, 0.0], 0.1 * s);
    iron.append(&rod([x, top, 0.0], [0.0, top, 0.0], 0.08 * s));
    iron.append(&rod([x, top - 2.2 * s, 0.0], [x * 0.45, top, 0.0], 0.06 * s));
    iron.append(&knob_at([x, top + 0.38 * s, 0.0], 0.14 * s));
    // The foot spreads on the ground.
    let mut foot = frustum(0.4 * s, 0.14 * s, 0.35 * s, 10, Shading::SMOOTH);
    foot.translate([x, 0.0, 0.0]);
    iron.append(&foot);
    (iron, [0.0, top, 0.0])
}

fn knob_at(at: V3, r: f32) -> MeshBuilder {
    let mut b = icosphere(r, 1);
    b.translate(at);
    b
}

/// The tripod: three poles leaning together, lashed at the top. Returns the poles, the
/// lashing and the point the chain hangs from.
fn tripod_rig(s: f32, ring: f32, turn: f32) -> (MeshBuilder, MeshBuilder, V3) {
    let apex = [0.0, 7.0 * s, 0.0];
    let foot = ring + 0.9 * s;
    let mut poles = MeshBuilder::new();
    for k in 0..3 {
        let a = turn + TAU * k as f32 / 3.0;
        poles.append(&rod([foot * a.cos(), 0.0, -foot * a.sin()], vec3::add(apex, [0.1 * s * a.cos(), 0.6 * s, -0.1 * s * a.sin()]), 0.11 * s));
    }
    let mut lashing = torus(0.28 * s, 0.07 * s, 16, 6, Shading::SMOOTH);
    lashing.translate(vec3::add(apex, [0.0, 0.15 * s, 0.0]));
    (poles, lashing, apex)
}

fn set_fire(s: &CampfireState, parts: &mut Parts) {
    let f = &s.fire;
    let size = f.size;
    let mut rng = Mulberry32::new(s.seed.wrapping_mul(0x9e37_79b9).wrapping_add(0xf17e));
    let ring = 2.3 * size;

    // The bed of ash inside the ring.
    let bed = frustum(ring - 0.2 * size, ring - 0.4 * size, 0.14 * size, 28, Shading::SMOOTH);
    parts.add_look("ash", bed, Look::of(Stone).tinted(mix(ASH, COLD_ASH, f.ash)));

    // The ring of stones, each a rough lump.
    let mut stones = MeshBuilder::new();
    for k in 0..f.stones {
        let a = TAU * k as f32 / f.stones as f32 + rng.range_f32(-0.08, 0.08);
        let r = 0.5 * size * rng.range_f32(0.85, 1.2);
        let mut st = icosphere(r, 1).flat_shaded();
        st.scale([1.25, 0.8, 1.0]).rotate_y(rng.range_f32(0.0, TAU)).translate([ring * a.cos(), r * 0.62, -ring * a.sin()]);
        stones.append(&st);
    }
    parts.add("stones", stones, Stone);

    // Logs stacked crosswise, two to a layer, and kindling leaned over them.
    let log_r = 0.24 * size;
    let mut logs = MeshBuilder::new();
    for k in 0..f.logs {
        let layer = (k / 2) as f32;
        let side = if k % 2 == 0 { -1.0 } else { 1.0 };
        let len = 3.6 * size * rng.range_f32(0.85, 1.1);
        let mut log = frustum(log_r, log_r * 0.9, len, 10, Shading::SMOOTH);
        // Lie along X (even layers) or Z (odd), centred.
        log.translate([0.0, -len * 0.5, 0.0]).rotate_z(PI * 0.5);
        if (k / 2) % 2 == 1 {
            log.rotate_y(PI * 0.5);
        }
        let lateral = side * 0.55 * size;
        let (dx, dz) = if (k / 2) % 2 == 1 { (lateral, 0.0) } else { (0.0, lateral) };
        log.translate([dx, 0.14 * size + log_r + layer * log_r * 1.9, dz]);
        logs.append(&log);
    }
    let stack_top = 0.14 * size + 2.0 * log_r * (f.logs.div_ceil(2) as f32).max(0.5);
    let wood = |t: f32| Look::of(Wood).tinted(mix(s.palette.wood, CHAR, t));
    if !logs.is_empty() {
        parts.add_look("logs", logs, wood(f.ash * 0.85));
    }
    let mut kindling = MeshBuilder::new();
    let tip = stack_top + 1.5 * size;
    for k in 0..f.kindling {
        let a = TAU * k as f32 / f.kindling.max(1) as f32 + rng.range_f32(-0.15, 0.15);
        let foot = 1.45 * size * rng.range_f32(0.9, 1.1);
        kindling.append(&rod([foot * a.cos(), 0.14 * size, -foot * a.sin()], [0.12 * size * a.cos(), tip, -0.12 * size * a.sin()], 0.07 * size));
    }
    if !kindling.is_empty() {
        parts.add_look("kindling", kindling, wood(f.ash * 0.6));
    }

    // The rig, its chain and the pot.
    if f.rig != Rig::Bare {
        let point = match f.rig {
            Rig::Hook => {
                let (iron, point) = hook_rig(size, ring);
                parts.add("rig", iron, Iron);
                point
            }
            _ => {
                let (poles, lashing, point) = tripod_rig(size, ring, rng.range_f32(0.0, TAU / 3.0));
                parts.add("rig", poles, Wood);
                parts.add("lashing", lashing, Cord);
                point
            }
        };
        if f.pot {
            let bottom = 3.0 * size;
            let (mut body, mut bail) = pot(size);
            let top = bottom + 2.1 * size;
            for m in [&mut body, &mut bail] {
                m.translate([0.0, bottom, 0.0]);
            }
            let chain = cable_chain(&[point, [0.0, top, 0.0]], false, 0.34 * size, 0.045 * size, 40);
            parts.add("chain", chain, Iron);
            parts.add("pot", body, Iron);
            parts.add("bail", bail, Iron);
        }
    }

    if f.lit {
        add_flame(parts, [0.0, stack_top * 0.6, 0.0], 2.7 * size, 0.85 * size, 0.04, s.seed);
    }
}

/// A bundle of sticks tied up in two places, the rig laid along its top and the pot set at one
/// end.
fn packed_kit(s: &CampfireState, parts: &mut Parts) {
    let f = &s.fire;
    let size = f.size;
    let mut rng = Mulberry32::new(s.seed.wrapping_mul(0x9e37_79b9).wrapping_add(0xb0d1));
    let n = (f.logs + f.kindling).clamp(6, 16);
    let stick = 0.1 * size;
    let girth = 0.19 * size * (n as f32).sqrt();
    let len = 6.0 * size;
    let mut sticks = MeshBuilder::new();
    for i in 0..n {
        let rr = 0.19 * size * (i as f32 + 0.5).sqrt();
        let a = i as f32 * 2.4;
        let l = len * rng.range_f32(0.88, 1.06);
        let y = girth + stick + rr * a.cos();
        sticks.append(&rod([-l * 0.5, y, rr * a.sin()], [l * 0.5, y, rr * a.sin()], stick * rng.range_f32(0.8, 1.15)));
    }
    parts.add_look("sticks", sticks, Look::of(Wood));
    let top = 2.0 * (girth + stick) + 0.1 * size;

    // The rig, laid on top of the bundle.
    match f.rig {
        Rig::Bare => {}
        Rig::Tripod => {
            let mut poles = MeshBuilder::new();
            for k in 0..3 {
                let z = (k as f32 - 1.0) * 0.26 * size;
                poles.append(&rod([-len * 0.6, top + 0.08 * size + 0.02 * k as f32, z], [len * 0.6, top + 0.08 * size, z], 0.11 * size));
            }
            parts.add("rig", poles, Wood);
        }
        Rig::Hook => {
            let mut iron = rod([-len * 0.5, top + 0.1 * size, -0.12 * size], [len * 0.55, top + 0.1 * size, -0.12 * size], 0.1 * size);
            iron.append(&rod([-len * 0.35, top + 0.1 * size, 0.18 * size], [len * 0.4, top + 0.1 * size, 0.18 * size], 0.08 * size));
            parts.add("rig", iron, Iron);
        }
    }

    // Two ties of cord.
    let mut ties = MeshBuilder::new();
    for x in [-0.28 * len, 0.28 * len] {
        let mut t = torus(girth + stick * 1.4 + if f.rig == Rig::Bare { 0.0 } else { 0.1 * size }, 0.06 * size, 28, 6, Shading::SMOOTH);
        t.rotate_z(PI * 0.5).translate([x, girth + stick, 0.0]);
        ties.append(&t);
    }
    parts.add("ties", ties, Cord);

    // The pot, set at one end.
    if f.pot && f.rig != Rig::Bare {
        let (mut body, mut bail) = pot(size * 0.8);
        for m in [&mut body, &mut bail] {
            m.translate([len * 0.5 + 1.25 * size, 0.0, 0.0]);
        }
        parts.add("pot", body, Iron);
        parts.add("bail", bail, Iron);
    }
}
