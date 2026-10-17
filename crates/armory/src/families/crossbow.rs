//! Crossbows: a stock with a prod across its front, a string, a nut and trigger to hold it
//! spanned, and sometimes a bolt on the deck.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (`SECTIONS`
//! `stock`, `limbs` (the prod), `string` and `arrow` (the bolt); `FAM_RANGE.crossbow`;
//! `PRESETS.crossbow`; the `fam==='crossbow'` branch of `randomize()`; `buildCrossbow`; and
//! `forgeNoun()`). The prod, nocks and string come from [`crate::build::limb`], shared with
//! bows, and the bolt from [`crate::build::arrow`].
//!
//! Departures from the prototype:
//! - The roll picks a weighted [`CrossbowArchetype`] (one per noun) and sizes the stock,
//!   prod and bolt to suit it, instead of nudging a random preset.
//! - `string.spanned` is now [`CrossbowState::spanned`], and it is drawn: a spanned string
//!   runs back to the nut and bends the prod further. The prototype stored it but always
//!   drew the string at rest, straight through the stock.
//! - The prod sits just below the deck and the string rides over the deck, where the
//!   prototype centred both in the stock.
//! - The bolt (the prototype's `arrow` section, defined but never drawn) lies on the deck
//!   with its butt at the nut.
//! - The trigger is a lever under the stock on a pin, not a bar along its side, and lashings
//!   are several tight turns of cord rather than one ring.
//!
//! Item space: the butt at the origin, the stock along +Y to the prod and stirrup, the deck
//! (where the bolt lies) facing +Z, the prod spanning X.

use crate::build::arrow::arrow_meshes;
use crate::build::hafted::smoothstep;
use crate::build::limb::{LimbGrip, LimbPlane, Stave, bowstring_mesh, point_along, serving_mesh};
use crate::build::parts::Parts;
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::geom::{
    CatmullRom, Frame, ItemMesh, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, cuboid, lathe, sweep_scaled, tube, vec3,
};
use crate::geom::polygon::P2;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::ranged::{
    Arrow, ArrowHead, ArrowLimits, Bowstring, BowstringLimits, FletchCut, LimbShape, Limbs, LimbsLimits, NockStyle,
};
use crate::slider::{Sliders, sliders};
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Bone, Brass, Cloth, Cord, Iron, Wood};

/// The crossbow generator.
pub struct Crossbow;

/// The stock and its fittings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stock {
    /// Butt to front.
    pub length: f32,
    /// Width of the butt, against the shoulder.
    pub butt_width: f32,
    /// Width along the middle, where the hand holds it (the prototype's "tiller width").
    pub mid_width: f32,
    /// Width at the front, where the prod passes through.
    pub front_width: f32,
    /// Depth of the stock (the butt is deeper).
    pub depth: f32,
    /// How far behind the front the prod sits.
    pub prod_set: f32,
    /// Where the nut sits, as a fraction of the length from the butt.
    pub nut: f32,
    pub material: MaterialKind,
    /// An iron loop at the front to put a foot in while spanning.
    pub stirrup: bool,
    pub stirrup_size: f32,
    /// A bone strip along the deck where the bolt slides.
    pub inlay: bool,
    /// A trigger lever under the stock.
    pub trigger: bool,
    /// Cord bindings holding the prod to the stock.
    pub lashings: u32,
}

sliders! {
    StockLimits for Stock {
        length: 3.0, 13.0, 0.05;
        butt_width: 0.2, 1.2, 0.01;
        mid_width: 0.12, 0.6, 0.01;
        front_width: 0.12, 0.8, 0.01;
        depth: 0.15, 0.7, 0.01;
        prod_set: 0.1, 1.5, 0.01;
        nut: 0.3, 0.75, 0.01;
        stirrup_size: 0.3, 1.4, 0.01;
        lashings: 0.0, 4.0, 1.0;
    }
}

impl Default for Stock {
    fn default() -> Self {
        Stock {
            length: 7.0,
            butt_width: 0.55,
            mid_width: 0.26,
            front_width: 0.34,
            depth: 0.32,
            prod_set: 0.5,
            nut: 0.55,
            material: Wood,
            stirrup: true,
            stirrup_size: 0.8,
            inlay: true,
            trigger: true,
            lashings: 2,
        }
    }
}

impl Stock {
    /// Width at `t` (0 butt, 1 front), as the prototype shapes it.
    pub fn width_at(&self, t: f32) -> f32 {
        let lerp = |a: f32, b: f32, x: f32| a + (b - a) * x;
        if t < 0.22 {
            lerp(self.butt_width, self.butt_width * 0.9, t / 0.22)
        } else if t < 0.42 {
            lerp(self.butt_width * 0.9, self.mid_width, smoothstep(0.22, 0.42, t))
        } else if t < 0.82 {
            self.mid_width
        } else {
            lerp(self.mid_width, self.front_width, smoothstep(0.82, 0.9, t))
        }
    }

    /// Depth at `t`: deeper toward the butt.
    pub fn depth_at(&self, t: f32) -> f32 {
        self.depth * (1.0 + 0.35 * (1.0 - smoothstep(0.0, 0.3, t)))
    }

    /// Height of the deck: the stock's flat top, where the string rides and the bolt lies.
    pub fn deck(&self) -> f32 {
        self.depth * 0.5
    }
}

/// Every parameter of one crossbow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrossbowState {
    pub seed: u32,
    pub stock: Stock,
    /// The prod.
    pub limbs: Limbs,
    pub string: Bowstring,
    /// Drawn back and held on the nut, ready to loose.
    pub spanned: bool,
    /// While being spanned, how far the string has come back toward the nut, 0 to 1. Never
    /// rolled: the game sets it during a reload. Ignored once `spanned`.
    #[serde(default)]
    pub span: f32,
    /// The bolt.
    pub arrow: Arrow,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for CrossbowState {
    /// The prototype's `BASE` state for the crossbow sections.
    fn default() -> Self {
        CrossbowState {
            seed: 0,
            stock: Stock::default(),
            limbs: Limbs { shape: LimbShape::Recurve, length: 7.0, brace: 0.8, width: 0.16, thickness: 0.07, taper: 0.4, recurve: 0.2, nocks: NockStyle::Cap, nock_material: Iron, material: Iron, ..Limbs::default() },
            string: Bowstring { thickness: 0.022, serving: true },
            spanned: true,
            span: 0.0,
            arrow: Arrow { length: 3.0, radius: 0.04, fletches: 2, fletch_cut: FletchCut::Triangle, fletch_length: 0.6, fletch_height: 0.11, ..Arrow::default() },
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every crossbow section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrossbowLimits {
    pub stock: StockLimits,
    pub limbs: LimbsLimits,
    pub string: BowstringLimits,
    pub arrow: ArrowLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges with `FAM_RANGE.crossbow` applied.
pub const LIMITS: CrossbowLimits = CrossbowLimits {
    stock: StockLimits::SLIDERS,
    limbs: LimbsLimits { length: LimbsLimits::SLIDERS.length.narrowed(3.0, 10.0), ..LimbsLimits::SLIDERS },
    string: BowstringLimits::SLIDERS,
    arrow: ArrowLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl CrossbowState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &CrossbowLimits) {
        self.stock.fit(&limits.stock);
        self.limbs.fit(&limits.limbs);
        self.string.fit(&limits.string);
        self.arrow.fit(&limits.arrow);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &CrossbowLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("stock", self.stock.out_of_range(&limits.stock)),
            tag("limbs", self.limbs.out_of_range(&limits.limbs)),
            tag("string", self.string.out_of_range(&limits.string)),
            tag("arrow", self.arrow.out_of_range(&limits.arrow)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

/// The kinds of crossbow "Forge new" composes, one per noun. Not stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossbowArchetype {
    /// A wooden or horn prod on a hunting stock.
    Crossbow,
    /// A steel prod on a middling stock.
    SteelCrossbow,
    /// A heavy steel prod on a long stock, with a stirrup.
    Arbalest,
    /// A small one-handed crossbow.
    Latchbow,
}

impl CrossbowArchetype {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(CrossbowArchetype, u32); 4] = [
        (CrossbowArchetype::Crossbow, 30),
        (CrossbowArchetype::SteelCrossbow, 25),
        (CrossbowArchetype::Arbalest, 25),
        (CrossbowArchetype::Latchbow, 20),
    ];
}

/// Where the prod sits and how far it bends: its plane, the stave, and the nut's catch.
struct Layout {
    plane: LimbPlane,
    stave: Stave,
    nut_y: f32,
    nut_radius: f32,
    deck: f32,
}

fn layout(s: &CrossbowState) -> Layout {
    let st = &s.stock;
    let l = &s.limbs;
    let deck = st.deck();
    let prod_y = st.length - st.prod_set;
    let nut_y = st.length * st.nut;
    // Thickened where it passes through the stock, with its upper edge just under the deck
    // so the string rides over it.
    let grip = LimbGrip { half_span: 0.07, width: l.width * 1.15, thickness: l.thickness * 1.5 };
    let plane = LimbPlane { origin: [0.0, prod_y, deck - grip.width * 0.5 - 0.01], along: vec3::X, toward_string: vec3::neg(vec3::Y) };
    // Spanning pulls the tips back a third of the way to the nut.
    let span = s.span_fraction();
    let draw = (prod_y - nut_y - l.brace).max(0.0);
    let (half, bend) = (l.length * 0.5 * (1.0 - 0.04 * span), l.brace + draw * 0.3 * span);
    Layout { plane, stave: Stave::new(l, plane, grip, half, bend), nut_y, nut_radius: st.depth * 0.3, deck }
}

impl Generator for Crossbow {
    type State = CrossbowState;

    fn roll(seed: u32) -> CrossbowState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let archetype = *rng.weighted(&CrossbowArchetype::WEIGHTS);
        compose(&mut rng, seed, archetype)
    }

    /// `forgeNoun()` for crossbows.
    fn noun(s: &CrossbowState) -> String {
        let noun = if s.stock.length < 4.5 {
            "Latchbow"
        } else if s.limbs.material == Iron {
            if s.stock.length > 7.5 { "Arbalest" } else { "Steel Crossbow" }
        } else {
            "Crossbow"
        };
        noun.to_string()
    }

    /// The stock, the prod and string from [`crate::build::limb`], the nut, inlay, lashings,
    /// stirrup and trigger, and the bolt from [`crate::build::arrow`].
    fn build(s: &CrossbowState) -> ItemMesh {
        let mut parts = Parts::new();
        let st = &s.stock;
        let lay = layout(s);
        let deck = lay.deck;
        let cord = s.string.thickness;

        parts.add("stock", stock_mesh(st), st.material);
        parts.add("prod", lay.stave.mesh(0.0, 1.0, 1.0), s.limbs.material);
        parts.add("nocks", lay.stave.nocks_mesh(), s.limbs.nock_material);

        // The string: to the nut when spanned, otherwise across the deck at rest. Either way
        // it rides up over the edges of the stock.
        let catch = [0.0, lay.nut_y + lay.nut_radius * 0.35, deck + cord];
        // Part way through spanning, the string is pulled from brace toward the nut.
        let span = s.span_fraction();
        let catch = if s.spanned { catch } else { [0.0, catch[1] + (1.0 - span) * (lay.plane.origin[1] - s.limbs.brace - catch[1]), catch[2]] };
        let nut_point: Vec<V3> = if span > 0.0 { vec![catch] } else { vec![] };
        let rough = lay.stave.string_path(cord, &nut_point);
        let mut extra = nut_point.clone();
        for side in [-1.0f32, 1.0] {
            let probe = point_along(&rough, &lay.plane, side * st.front_width);
            let y = probe[1];
            let edge = st.width_at((y / st.length).clamp(0.0, 1.0)) * 0.5 + cord * 2.0;
            let p = point_along(&rough, &lay.plane, side * edge);
            extra.push([p[0], p[1] - cord * 0.25, p[2].max(deck + cord)]);
        }
        let path = lay.stave.string_path(cord, &extra);
        let anchors = lay.stave.string_anchors();
        parts.add_unoutlined("string", bowstring_mesh(&anchors, &path, &s.string), Cord);
        if s.string.serving {
            let centre = if span > 0.0 { catch } else { point_along(&path, &lay.plane, 0.0) };
            parts.add_unoutlined("serving", serving_mesh(&path, centre, 0.35, &s.string), Cord);
        }

        parts.add("nut", nut_mesh(lay.nut_y, lay.nut_radius, deck), Brass);
        let inlay_top = if st.inlay { 0.014 } else { 0.0 };
        if st.inlay {
            let (y0, y1) = (lay.nut_y + lay.nut_radius, lay.plane.origin[1] - s.limbs.thickness);
            let mut strip = cuboid([st.mid_width * 0.42, (y1 - y0).max(0.1), inlay_top]);
            strip.translate([0.0, (y0 + y1) * 0.5, deck + inlay_top * 0.5 - 0.001]);
            parts.add("inlay", strip, Bone);
        }
        let lashings = lashings_mesh(s, &lay);
        parts.add("lashings", lashings, Cord);
        if st.stirrup {
            parts.add("stirrup", stirrup_mesh(st), Iron);
        }
        if st.trigger {
            parts.add("trigger", trigger_mesh(st, lay.nut_y), Iron);
        }

        if s.arrow.on {
            let a = &s.arrow;
            let nock = [0.0, lay.nut_y + lay.nut_radius * 0.5, deck + inlay_top + a.radius];
            // Two vanes lie flat; with more, the odd vane stands up off the deck.
            let roll = if a.fletches >= 3 { std::f32::consts::PI } else { 0.0 };
            let bolt = arrow_meshes(a, false).placed(nock, vec3::Y, roll);
            parts.add("arrow_shaft", bolt.shaft, a.material);
            parts.add("arrow_head", bolt.head, Iron);
            parts.add_tinted("fletching", bolt.fletching, Cloth, s.palette.fletching);
        }
        parts.finish()
    }
}

impl CrossbowState {
    /// Lays `ammo` as this crossbow's bolt on the deck, in its own vane colour. The rolled
    /// crossbow is unchanged until this is called.
    pub fn load(&mut self, ammo: &crate::families::ammo::AmmoState) {
        self.arrow = ammo.arrow;
        self.arrow.on = true;
        self.palette.fletching = ammo.palette.fletching;
    }

    /// Takes the bolt on the deck off.
    pub fn unload(&mut self) {
        self.arrow.on = false;
    }

    /// How far the string is back toward the nut: 1 when spanned, else [`CrossbowState::span`].
    pub fn span_fraction(&self) -> f32 {
        if self.spanned { 1.0 } else { self.span.clamp(0.0, 1.0) }
    }
}


impl Crossbow {
    /// "Forge new" forced to one archetype, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, a)` when `a` is what `s` rolls.
    pub fn roll_as(seed: u32, archetype: CrossbowArchetype) -> CrossbowState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, archetype)
    }
}

/// A squared-off section with rounded corners, spanning -1..=1 both ways.
fn stock_section() -> Vec<P2> {
    (0..28)
        .map(|k| {
            let (s, c) = (std::f32::consts::TAU * k as f32 / 28.0).sin_cos();
            [c.signum() * c.abs().powf(0.3), s.signum() * s.abs().powf(0.3)]
        })
        .collect()
}

/// The stock: a flat deck on top, widening to the butt and deepening below it.
fn stock_mesh(st: &Stock) -> MeshBuilder {
    let rows = 96;
    let deck = st.deck();
    let frames: Vec<Frame> = (0..=rows)
        .map(|i| {
            let t = i as f32 / rows as f32;
            let d = st.depth_at(t);
            // Normal across the width, binormal down: the section's y runs top to bottom.
            Frame { origin: [0.0, st.length * t, deck - d * 0.5], tangent: vec3::Y, normal: vec3::X, binormal: vec3::neg(vec3::Z) }
        })
        .collect();
    sweep_scaled(&frames, &stock_section(), &SweepOptions::default(), |t| [st.width_at(t) * 0.5, st.depth_at(t) * 0.5])
}

/// The nut: a turned drum across the stock whose top stands proud of the deck to catch the
/// string.
fn nut_mesh(y: f32, r: f32, deck: f32) -> MeshBuilder {
    let w = 0.06;
    let profile = [[0.0, -w], [r * 0.9, -w], [r, -w * 0.7], [r, w * 0.7], [r * 0.9, w], [0.0, w]];
    let mut nut = lathe(&profile, &LatheOptions { facets: 20, shading: Shading { smooth_around: true, smooth_along: false }, ..Default::default() });
    nut.rotate_z(std::f32::consts::FRAC_PI_2).translate([0.0, y, deck - r * 0.55]);
    nut
}

/// Cord lashings either side of the prod, each a few tight turns round the stock.
fn lashings_mesh(s: &CrossbowState, lay: &Layout) -> MeshBuilder {
    let st = &s.stock;
    let mut out = MeshBuilder::new();
    let cord = 0.018;
    let prod_y = lay.plane.origin[1];
    for k in 0..st.lashings {
        let side = if k % 2 == 1 { 1.0 } else { -1.0 };
        let y = prod_y + side * (s.limbs.thickness * 0.75 + 0.06 + (k / 2) as f32 * 0.09);
        if y > st.length - cord * 3.0 {
            continue;
        }
        let t = (y / st.length).clamp(0.0, 1.0);
        let (hw, hd) = (st.width_at(t) * 0.5 + cord, st.depth_at(t) * 0.5 + cord);
        let centre_z = lay.deck - st.depth_at(t) * 0.5;
        for turn in 0..3 {
            let yy = y + (turn as f32 - 1.0) * cord * 2.05;
            let path: Vec<V3> = (0..32)
                .map(|j| {
                    let (sn, c) = (std::f32::consts::TAU * j as f32 / 32.0).sin_cos();
                    [c.signum() * c.abs().powf(0.4) * hw, yy, centre_z + sn.signum() * sn.abs().powf(0.4) * hd]
                })
                .collect();
            out.append(&tube(&path, 6, &SweepOptions { up: Some(vec3::Y), ..SweepOptions::closed_loop() }, |_| cord));
        }
    }
    out
}

/// The stirrup: a D of iron bar standing out from the front of the stock.
fn stirrup_mesh(st: &Stock) -> MeshBuilder {
    let sz = st.stirrup_size;
    let z = st.deck() - st.depth * 0.5;
    let control: Vec<V3> = [[-0.3, -0.15], [-0.5, 0.45], [-0.32, 0.85], [0.0, 0.96], [0.32, 0.85], [0.5, 0.45], [0.3, -0.15]]
        .iter()
        .map(|&[x, y]| [x * sz, st.length + y * sz, z])
        .collect();
    let path = CatmullRom::centripetal(control).sample_evenly(72);
    let section: Vec<P2> = (0..10)
        .map(|k| {
            let (s, c) = (std::f32::consts::TAU * k as f32 / 10.0).sin_cos();
            [c * 0.035, s * 0.05]
        })
        .collect();
    crate::geom::sweep(&path, &section, &SweepOptions { up: Some(vec3::Z), ..Default::default() }, |_| 1.0)
}

/// The trigger: a long lever under the stock, hung on a pin through it just behind the nut.
fn trigger_mesh(st: &Stock, nut_y: f32) -> MeshBuilder {
    let t = nut_y / st.length;
    let bottom = st.deck() - st.depth_at(t);
    let pivot = [0.0, nut_y - 0.08, bottom + 0.06];
    let control = vec![
        pivot,
        [0.0, nut_y - 0.2, bottom - 0.07],
        [0.0, nut_y - st.length * 0.18, bottom - 0.1],
        [0.0, nut_y - st.length * 0.3, bottom - 0.12],
    ];
    let path = CatmullRom::centripetal(control).sample_evenly(40);
    let mut out = tube(&path, 8, &SweepOptions::default(), |u| 0.028 * (1.0 - 0.3 * u));
    let half = st.width_at(t) * 0.5 + 0.02;
    let mut pin = crate::geom::cylinder(0.02, half * 2.0, 10, Shading::SMOOTH);
    pin.translate([0.0, -half, 0.0]).rotate_z(-std::f32::consts::FRAC_PI_2).translate(pivot);
    out.append(&pin);
    out
}

fn compose(rng: &mut Mulberry32, seed: u32, a: CrossbowArchetype) -> CrossbowState {
    use CrossbowArchetype as A;
    let mut s = CrossbowState { seed, ..CrossbowState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    let small = a == A::Latchbow;

    let st = &mut s.stock;
    st.length = match a {
        A::Crossbow => rng.range_f32(5.0, 9.0),
        A::SteelCrossbow => rng.range_f32(5.0, 7.4),
        A::Arbalest => rng.range_f32(7.7, 10.5),
        A::Latchbow => rng.range_f32(3.2, 4.4),
    };
    st.butt_width = if small { rng.range_f32(0.25, 0.35) } else { rng.range_f32(0.45, 0.62) };
    st.mid_width = if small { rng.range_f32(0.14, 0.18) } else { rng.range_f32(0.18, 0.28) };
    st.front_width = if small { rng.range_f32(0.2, 0.26) } else { rng.range_f32(0.28, 0.38) };
    st.depth = if small { rng.range_f32(0.2, 0.25) } else { rng.range_f32(0.28, 0.36) };
    st.prod_set = if small { rng.range_f32(0.2, 0.3) } else { st.length * rng.range_f32(0.045, 0.07) };
    st.nut = rng.range_f32(0.5, 0.58);
    st.material = Wood;
    st.stirrup = st.length > 5.5 && rng.chance(if a == A::Arbalest { 0.9 } else { 0.6 });
    st.stirrup_size = rng.range_f32(0.75, 1.0) * if a == A::Arbalest { 1.1 } else { 1.0 };
    st.inlay = rng.chance(if small { 0.2 } else { 0.6 });
    st.trigger = rng.chance(0.92);
    st.lashings = match a {
        A::Crossbow => rng.int(2, 3),
        A::SteelCrossbow => rng.int(1, 3),
        A::Arbalest => rng.int(2, 4),
        A::Latchbow => rng.int(0, 1),
    };
    let stock = *st;

    let l = &mut s.limbs;
    l.material = match a {
        A::Crossbow => Wood,
        A::SteelCrossbow | A::Arbalest => Iron,
        A::Latchbow => *rng.weighted(&[(Iron, 2), (Wood, 1)]),
    };
    let steel = l.material == Iron;
    l.shape = *rng.weighted(if steel {
        &[(LimbShape::Recurve, 3), (LimbShape::Longbow, 2)][..]
    } else {
        &[(LimbShape::Composite, 3), (LimbShape::Recurve, 2), (LimbShape::Longbow, 1)]
    });
    l.length = (stock.length * rng.range_f32(0.8, 1.15)).clamp(3.0, 10.0);
    l.brace = l.length * rng.range_f32(0.1, 0.14);
    let scale = if small { 0.75 } else { 1.0 };
    (l.width, l.thickness) = if steel {
        (rng.range_f32(0.12, 0.17) * scale, rng.range_f32(0.05, 0.08) * scale)
    } else {
        (rng.range_f32(0.17, 0.22) * scale, rng.range_f32(0.1, 0.14) * scale)
    };
    l.taper = rng.range_f32(0.35, 0.5);
    l.recurve = match l.shape {
        LimbShape::Recurve => rng.range_f32(0.15, 0.4),
        LimbShape::Composite => rng.range_f32(0.4, 0.6),
        _ => rng.range_f32(0.0, 0.05),
    };
    l.siyah = if l.shape == LimbShape::Composite { rng.range_f32(0.1, 0.2) } else { 0.0 };
    l.nocks = if steel { NockStyle::Cap } else { *rng.weighted(&[(NockStyle::Horn, 2), (NockStyle::None, 1)]) };
    l.nock_material = if steel { Iron } else { *rng.weighted(&[(Bone, 3), (Brass, 1)]) };

    s.string = Bowstring { thickness: if small { rng.range_f32(0.012, 0.016) } else { rng.range_f32(0.018, 0.028) }, serving: rng.chance(0.8) };

    let ar = &mut s.arrow;
    ar.on = rng.chance(0.8);
    ar.length = (stock.length * rng.range_f32(0.38, 0.44)).max(1.4);
    ar.radius = if small { rng.range_f32(0.025, 0.03) } else { rng.range_f32(0.035, 0.045) };
    ar.head = *rng.weighted(&[(ArrowHead::Bodkin, 5), (ArrowHead::Broadhead, 2), (ArrowHead::Blunt, 1), (ArrowHead::Leaf, 1)]);
    (ar.head_length, ar.head_width) = match ar.head {
        ArrowHead::Bodkin => (rng.range_f32(0.3, 0.4) * scale, rng.range_f32(0.1, 0.14) * scale),
        ArrowHead::Broadhead => (rng.range_f32(0.35, 0.45) * scale, rng.range_f32(0.22, 0.28) * scale),
        ArrowHead::Leaf => (rng.range_f32(0.3, 0.4) * scale, rng.range_f32(0.14, 0.18) * scale),
        ArrowHead::Blunt => (rng.range_f32(0.18, 0.25) * scale, rng.range_f32(0.12, 0.16) * scale),
    };
    ar.material = Wood;
    ar.fletches = *rng.weighted(&[(2, 6), (3, 1)]);
    ar.fletch_cut = *rng.weighted(&[(FletchCut::Triangle, 5), (FletchCut::Shield, 1), (FletchCut::Parabolic, 1)]);
    ar.fletch_length = rng.range_f32(0.5, 0.75) * scale;
    ar.fletch_height = rng.range_f32(0.08, 0.12) * scale;
    s.spanned = rng.chance(if s.arrow.on { 0.85 } else { 0.5 });

    let mut targets = vec![(EtchTarget::Wood, 3), (EtchTarget::Metals, 1)];
    if stock.inlay {
        targets.push((EtchTarget::Bone, 1));
    }
    s.etch = Etching::roll(rng, 0.25, &targets, 1);
    s.fit(&LIMITS);
    s
}
