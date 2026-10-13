//! Food and drink: breads, cheeses, fruit, meats, fish, pies and mushrooms, and the vessels
//! drink comes in (tankards, bottles, potions, waterskins, flasks).
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (the `food`
//! entry of `SECTIONS`, `PRESETS.food`, the `fam==='food'` branch of `randomize()`,
//! `buildFood()` and `forgeNoun()`), and the sundries in `data/goods.json` (glowcaps,
//! waterskins, lamp-oil flasks).
//!
//! Departures from the prototype:
//! - Three more styles the plate needed outside the Armory: [`FoodStyle::Mushroom`] (the
//!   glowcap), [`FoodStyle::Waterskin`] and [`FoodStyle::Flask`].
//! - The prototype's `cut` toggle (a wedge out of a cheese) becomes a [`Portion`]: whole, cut
//!   open, a single piece (a slice of bread, a wedge of cheese, a slice of pie) or bitten.
//!   Every cut face shows a rind band (crust, rind, skin, pastry) round its core.
//! - The prototype's two colours (`color`, `color2`), whose meaning changed per style, are
//!   four named by role: `outer`, `inner`, `accent` and `liquid`.
//! - A potion's brew is a typed [`Brew`]; the noun still reads the liquid's colour, so a
//!   hand-tinted potion is named for the brew it looks like.
//! - There is no glass material: glass is [`MaterialKind::Gem`] with its own tint, as are
//!   liquids. Cork is tinted wood, wax is tinted bone, a bottle's paper label tinted cloth.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::cond::{Condition, ConditionLimits};
use crate::families::Generator;
use crate::geom::{
    CatmullRom, GridOptions, Hollow, ItemMesh, LatheOptions, OutlineMode, MeshBuilder, Shading, SweepOptions, V3, carve_hollows, clip_profile,
    extrude, frustum, icosphere, lathe, lathe_sector, lathe_warped, parametric_surface, profile_face, profile_height_at,
    profile_radius_at, regular_polygon, resample_by_length, rind_face, shell_inside, shell_profile, spike, spline_profile,
    stitch_grid, sweep, torus, tube, uv_sphere, vec3,
};
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::slider::{Sliders, sliders};
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Bone, Brass, Cloth, Cord, Gem, Iron, Leather, Silver, Wood};

/// The food and drink generator.
pub struct Food;

/// What the item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoodStyle {
    /// A long crusty loaf with slashed scores.
    Loaf,
    /// A round loaf scored across its dome.
    Boule,
    /// A wheel of cheese in its rind.
    Cheese,
    Apple,
    Pear,
    /// A roast leg on the bone.
    Haunch,
    /// A whole smoked fish.
    Fish,
    /// Linked sausages tied with string.
    Sausages,
    /// A covered pie in its dish.
    Pie,
    /// A cluster of cave mushrooms; glowing ones are glowcaps.
    Mushroom,
    Tankard,
    /// A corked wine bottle.
    Bottle,
    /// A stoppered glass flask or vial of brew.
    Potion,
    Waterskin,
    /// A flat pilgrim's flask on a strap.
    Flask,
}

impl FoodStyle {
    pub const ALL: [FoodStyle; 15] = [
        FoodStyle::Loaf, FoodStyle::Boule, FoodStyle::Cheese, FoodStyle::Apple, FoodStyle::Pear, FoodStyle::Haunch,
        FoodStyle::Fish, FoodStyle::Sausages, FoodStyle::Pie, FoodStyle::Mushroom, FoodStyle::Tankard, FoodStyle::Bottle,
        FoodStyle::Potion, FoodStyle::Waterskin, FoodStyle::Flask,
    ];

    /// Roll weights, in percent. Potions are doubled as in the prototype.
    pub const WEIGHTS: [(FoodStyle, u32); 15] = [
        (FoodStyle::Loaf, 8),
        (FoodStyle::Boule, 6),
        (FoodStyle::Cheese, 8),
        (FoodStyle::Apple, 7),
        (FoodStyle::Pear, 5),
        (FoodStyle::Haunch, 6),
        (FoodStyle::Fish, 6),
        (FoodStyle::Sausages, 6),
        (FoodStyle::Pie, 6),
        (FoodStyle::Mushroom, 6),
        (FoodStyle::Tankard, 7),
        (FoodStyle::Bottle, 6),
        (FoodStyle::Potion, 12),
        (FoodStyle::Waterskin, 5),
        (FoodStyle::Flask, 6),
    ];

    /// The portions this style can come in (always starting with [`Portion::Whole`]).
    pub fn portions(self) -> &'static [Portion] {
        use Portion::*;
        match self {
            FoodStyle::Loaf => &[Whole, Cut, Piece, Bitten],
            FoodStyle::Boule | FoodStyle::Cheese | FoodStyle::Pie => &[Whole, Cut, Piece],
            FoodStyle::Apple | FoodStyle::Pear => &[Whole, Cut, Bitten],
            FoodStyle::Haunch => &[Whole, Bitten],
            _ => &[Whole],
        }
    }

    /// A vessel for drink rather than something to eat.
    pub fn is_drink(self) -> bool {
        matches!(self, FoodStyle::Tankard | FoodStyle::Bottle | FoodStyle::Potion | FoodStyle::Waterskin | FoodStyle::Flask)
    }
}

/// How much of it is left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Portion {
    Whole,
    /// Cut open: a wedge out of a wheel, a pie or a round loaf, the end off a loaf, a slice
    /// out of an apple. The cut faces show rind and core.
    Cut,
    /// One piece on its own: a slice of bread, a wedge of cheese or bread, a slice of pie.
    Piece,
    /// A bite taken out of it.
    Bitten,
}

/// What a potion holds, which fixes its colour (the prototype's potion colours and names).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Brew {
    Healing,
    Mana,
    Antidote,
    Insight,
    Elixir,
    HolyWater,
}

impl Brew {
    pub const ALL: [Brew; 6] = [Brew::Healing, Brew::Mana, Brew::Antidote, Brew::Insight, Brew::Elixir, Brew::HolyWater];

    pub const fn rgb(self) -> Rgb {
        match self {
            Brew::Healing => Rgb::hex(0xc8323c),
            Brew::Mana => Rgb::hex(0x3c6fd4),
            Brew::Antidote => Rgb::hex(0x4c9a52),
            Brew::Insight => Rgb::hex(0x6d4bb3),
            Brew::Elixir => Rgb::hex(0xd4a838),
            Brew::HolyWater => Rgb::hex(0xd9dfe6),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Brew::Healing => "Healing Potion",
            Brew::Mana => "Mana Potion",
            Brew::Antidote => "Antidote",
            Brew::Insight => "Potion of Insight",
            Brew::Elixir => "Elixir",
            Brew::HolyWater => "Holy Water",
        }
    }

    /// The brew whose colour is nearest `c` (the prototype names potions this way).
    pub fn nearest(c: Rgb) -> Brew {
        let distance = |b: &Brew| {
            let d = b.rgb();
            (c.0 - d.0).powi(2) + (c.1 - d.1).powi(2) + (c.2 - d.2).powi(2)
        };
        Brew::ALL.into_iter().min_by(|a, b| distance(a).total_cmp(&distance(b))).unwrap_or(Brew::Healing)
    }
}

/// One piece of food or drink: the prototype's `food` section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Provision {
    pub style: FoodStyle,
    pub portion: Portion,
    /// Overall scale.
    pub size: f32,
    /// Proportions within the style (longer loaf, taller wheel, rounder flask...).
    pub shape: f32,
    /// Scores on bread, holes in a cheese's cut faces, char on a roast, links of sausage,
    /// vents in a pie, mushrooms in a cluster, bubbles in a potion.
    pub count: u32,
    /// How full a vessel is, 0..=1.
    pub fill: f32,
    /// Glow of a potion's brew or a glowcap.
    pub glow: f32,
    /// Crust, rind, skin, casing, glass, or a mushroom's cap.
    pub outer: Rgb,
    /// Crumb, paste, flesh, filling, or a mushroom's gills and stem.
    pub inner: Rgb,
    /// Scores, leaves, char, fins, foam, or a bottle's label.
    pub accent: Rgb,
    /// The drink in a vessel.
    pub liquid: Rgb,
    /// What a tankard, pie dish, waterskin or flask is made of ([`MaterialKind::Gem`] is glass).
    pub vessel: MaterialKind,
    /// Wax over the stopper of a bottle, potion or flask, in its colour.
    pub seal: Option<Rgb>,
}

sliders! {
    ProvisionLimits for Provision {
        size: 0.6, 1.5, 0.01;
        shape: 0.0, 1.0, 0.01;
        count: 1.0, 8.0, 1.0;
        fill: 0.0, 1.0, 0.01;
        glow: 0.0, 1.0, 0.01;
    }
}

impl Default for Provision {
    /// The prototype's `BASE.food`, whole.
    fn default() -> Self {
        Provision {
            style: FoodStyle::Loaf,
            portion: Portion::Whole,
            size: 1.0,
            shape: 0.5,
            count: 3,
            fill: 0.7,
            glow: 0.5,
            outer: Rgb::hex(0xb8752f),
            inner: Rgb::hex(0xecd29a),
            accent: Rgb::hex(0xecd29a),
            liquid: Rgb::hex(0x8a5a1a),
            vessel: Wood,
            seal: None,
        }
    }
}

impl Provision {
    /// The portion as built: one the style cannot come in counts as whole.
    pub fn portion_shown(&self) -> Portion {
        if self.style.portions().contains(&self.portion) { self.portion } else { Portion::Whole }
    }

    /// Whether a vessel holds anything worth naming.
    pub fn is_full(&self) -> bool {
        self.fill > 0.05
    }
}

/// Every parameter of one piece of food or drink.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FoodState {
    pub seed: u32,
    pub provision: Provision,
    pub cond: Condition,
    pub palette: Palette,
}


/// Slider limits for every food section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FoodLimits {
    pub provision: ProvisionLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges (it has no `RANGES.food`).
pub const LIMITS: FoodLimits = FoodLimits { provision: ProvisionLimits::SLIDERS, cond: ConditionLimits::SLIDERS };

impl FoodState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &FoodLimits) {
        self.provision.fit(&limits.provision);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &FoodLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [tag("provision", self.provision.out_of_range(&limits.provision)), tag("cond", self.cond.out_of_range(&limits.cond))].concat()
    }
}

/// The word the item card uses for food's overall wear (`condWord` for the food family).
pub fn freshness(cond: &Condition) -> &'static str {
    match cond.wear {
        w if w < 0.1 => "Fresh",
        w if w < 0.3 => "Day-old",
        w if w < 0.55 => "Stale",
        w if w < 0.8 => "Mouldy",
        _ => "Rotten",
    }
}

impl Generator for Food {
    type State = FoodState;

    fn roll(seed: u32) -> FoodState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&FoodStyle::WEIGHTS);
        compose(&mut rng, seed, style)
    }

    /// `forgeNoun()` for food, with names for the new styles and portions.
    fn noun(s: &FoodState) -> String {
        use FoodStyle as F;
        use Portion::*;
        let p = &s.provision;
        let portion = p.portion_shown();
        let noun = match p.style {
            F::Loaf => match portion {
                Whole => "Crusty Loaf",
                Cut => "Cut Loaf",
                Piece => "Slice of Bread",
                Bitten => "Bitten Loaf",
            },
            F::Boule => match portion {
                Cut => "Cut Round Loaf",
                Piece => "Wedge of Bread",
                _ => "Round Loaf",
            },
            F::Cheese => match portion {
                Cut => "Cut Cheese",
                Piece => "Wedge of Cheese",
                _ => "Cheese Wheel",
            },
            F::Apple => match portion {
                Cut => "Cut Apple",
                Bitten => "Bitten Apple",
                _ => "Apple",
            },
            F::Pear => match portion {
                Cut => "Cut Pear",
                Bitten => "Bitten Pear",
                _ => "Pear",
            },
            F::Haunch => if portion == Bitten { "Gnawed Haunch" } else { "Roast Haunch" },
            F::Fish => "Smoked Fish",
            F::Sausages => if p.count <= 1 { "Sausage" } else { "String of Sausages" },
            F::Pie => match portion {
                Cut => "Cut Pie",
                Piece => "Slice of Pie",
                _ => "Meat Pie",
            },
            F::Mushroom => match (p.glow >= GLOWCAP, p.count > 1) {
                (true, false) => "Glowcap",
                (true, true) => "Glowcaps",
                (false, false) => "Cave Mushroom",
                (false, true) => "Cave Mushrooms",
            },
            F::Tankard => if p.is_full() { "Tankard of Ale" } else { "Empty Tankard" },
            F::Bottle => if p.is_full() { "Bottle of Wine" } else { "Empty Bottle" },
            F::Potion => if p.is_full() { Brew::nearest(p.liquid).name() } else { "Empty Vial" },
            F::Waterskin => if p.is_full() { "Waterskin" } else { "Empty Waterskin" },
            F::Flask => match p.vessel {
                Gem => "Glass Flask",
                Leather => "Leather Flask",
                Brass => "Brass Flask",
                Silver => "Silver Flask",
                Iron => "Iron Flask",
                Wood => "Wooden Flask",
                _ => "Flask",
            },
        };
        noun.to_string()
    }

    /// One builder per style; the item is then set down on y = 0, centred on the origin.
    fn build(s: &FoodState) -> ItemMesh {
        let p = &s.provision;
        let mut a = Parts::default();
        let mut rng = Mulberry32::new(s.seed.wrapping_mul(173).wrapping_add(5));
        match p.style {
            FoodStyle::Loaf => loaf(p, &mut a),
            FoodStyle::Boule => boule(p, &mut a),
            FoodStyle::Cheese => cheese(p, &mut rng, &mut a),
            FoodStyle::Apple | FoodStyle::Pear => fruit(p, &mut rng, &mut a),
            FoodStyle::Haunch => haunch(p, &mut rng, &mut a),
            FoodStyle::Fish => fish(p, &mut a),
            FoodStyle::Sausages => sausages(p, &mut a),
            FoodStyle::Pie => pie(p, &mut a),
            FoodStyle::Mushroom => mushrooms(p, &mut rng, &mut a),
            FoodStyle::Tankard => tankard(p, &mut rng, &mut a),
            FoodStyle::Bottle => bottle(p, &mut a),
            FoodStyle::Potion => potion(p, &mut rng, &mut a),
            FoodStyle::Waterskin => waterskin(p, &mut rng, &mut a),
            FoodStyle::Flask => flask(p, &mut a),
        }
        a.set_down();
        a.finish()
    }
}

/// Glow at or above which a mushroom is a glowcap.
pub const GLOWCAP: f32 = 0.25;

impl Food {
    /// "Forge new" forced to one style, for turntables and tests. Draws the same stream as
    /// [`Generator::roll`], so `roll(s)` equals `roll_as(s, style)` when `s` rolls that style.
    pub fn roll_as(seed: u32, style: FoodStyle) -> FoodState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, style)
    }
}

// ── Roll ─────────────────────────────────────────────────────────────────────────────────

const CORK: Rgb = Rgb::hex(0xb08a5a);
const STEM: Rgb = Rgb::hex(0x5a3b22);
const DARK: Rgb = Rgb::hex(0x2a1d14);
const GLASS: Rgb = Rgb::hex(0xdfe9ee);

fn pick_rgb(rng: &mut Mulberry32, hexes: &[u32]) -> Rgb {
    Rgb::hex(*rng.pick(hexes))
}

fn compose(rng: &mut Mulberry32, seed: u32, style: FoodStyle) -> FoodState {
    use FoodStyle as F;
    use Portion::*;
    let mut s = FoodState { seed, ..FoodState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    let p = &mut s.provision;
    p.style = style;
    p.size = rng.range_f32(0.85, 1.15);
    p.shape = rng.range_f32(0.0, 1.0);
    p.glow = 0.0;
    p.fill = 0.0;
    p.seal = None;
    p.portion = *rng.weighted(match style {
        F::Loaf => &[(Whole, 4), (Cut, 2), (Piece, 2), (Bitten, 2)][..],
        F::Boule => &[(Whole, 5), (Cut, 2), (Piece, 2)],
        // The prototype cuts seven cheeses in ten.
        F::Cheese => &[(Whole, 3), (Cut, 5), (Piece, 3)],
        F::Apple | F::Pear => &[(Whole, 5), (Cut, 2), (Bitten, 2)],
        F::Haunch => &[(Whole, 4), (Bitten, 2)],
        F::Pie => &[(Whole, 4), (Cut, 2), (Piece, 2)],
        _ => &[(Whole, 1)],
    });
    match style {
        F::Loaf | F::Boule => {
            // Wheat, or dark rye with a greyer crumb.
            let rye = rng.chance(0.25);
            p.outer = if rye { pick_rgb(rng, &[0x6a4322, 0x7a4a22]) } else { pick_rgb(rng, &[0xb8752f, 0xa8692a, 0xc98a3e]) };
            p.inner = if rye { Rgb::hex(0xb89a76) } else { pick_rgb(rng, &[0xecd29a, 0xf0dcaa]) };
            p.accent = p.inner;
            p.count = if style == F::Loaf { rng.int(2, 6) } else { rng.int(2, 4) };
        }
        F::Cheese => {
            p.outer = pick_rgb(rng, &[0xc9a23c, 0xa8322a, 0xe0c070, 0x8a6a3a]);
            p.inner = pick_rgb(rng, &[0xf0d98a, 0xf4e6b8, 0xe8c860]);
            p.accent = p.inner;
            p.count = rng.int(3, 8);
        }
        F::Apple | F::Pear => {
            p.outer = if style == F::Apple {
                pick_rgb(rng, &[0xb8322a, 0x8fb43a, 0xc9a23c])
            } else {
                pick_rgb(rng, &[0xb9b44a, 0xa8a040, 0xb07a3a])
            };
            p.inner = Rgb::hex(0xf2e6c0);
            p.accent = pick_rgb(rng, &[0x5f8a34, 0x4f7a2c]);
            p.count = 1;
        }
        F::Haunch => {
            p.outer = pick_rgb(rng, &[0x9a5a2a, 0x8a4a22, 0xa8682e]);
            p.inner = Rgb::hex(0xc08070);
            p.accent = Rgb::hex(0x5a2e14);
            p.count = rng.int(3, 7);
        }
        F::Fish => {
            // Smoked, golden-smoked, or a pale blind cavefish.
            p.outer = pick_rgb(rng, &[0x9a8a5a, 0xb08a4a, 0xd8d2c4]);
            p.accent = Rgb::hex(0x6f5f3a);
            p.inner = Rgb::hex(0xe8d8c0);
            p.count = 1;
        }
        F::Sausages => {
            p.outer = pick_rgb(rng, &[0x8a3f2a, 0x9a5a3a, 0x6a2a1a]);
            p.inner = Rgb::hex(0xc08070);
            p.accent = p.outer;
            p.count = rng.int(2, 8);
        }
        F::Pie => {
            p.outer = pick_rgb(rng, &[0xc98a3e, 0xd8a050, 0xb0702e]);
            p.inner = pick_rgb(rng, &[0x7a3f2a, 0x6a2a1a, 0x8a4a2a]);
            p.accent = DARK;
            p.count = rng.int(3, 8);
            p.vessel = *rng.weighted(&[(Iron, 4), (Brass, 2), (Silver, 1)]);
        }
        F::Mushroom => {
            if rng.chance(0.6) {
                p.outer = pick_rgb(rng, &[0x6ad0c8, 0x9ad06a, 0xc8a0e0]);
                p.glow = rng.range_f32(0.4, 0.9);
            } else {
                p.outer = pick_rgb(rng, &[0x8a5a3a, 0xc9b08a, 0xa83a2a]);
            }
            p.inner = pick_rgb(rng, &[0xe8e0cc, 0xd8d0b8]);
            p.accent = p.inner;
            p.count = *rng.weighted(&[(1, 3), (2, 2), (3, 3), (4, 2), (5, 1)]);
        }
        F::Tankard => {
            p.vessel = *rng.weighted(&[(Wood, 4), (Iron, 2), (Silver, 1), (Brass, 1)]);
            p.fill = if rng.chance(0.8) { rng.range_f32(0.5, 1.0) } else { 0.0 };
            p.liquid = pick_rgb(rng, &[0x8a5a1a, 0x4a2a12, 0xb8862a]);
            p.accent = Rgb::hex(0xf1ead6);
        }
        F::Bottle => {
            p.outer = pick_rgb(rng, &[0x2c3f2a, 0x3a2a2a, 0x5a4a2a, 0x2a3a4a]);
            p.accent = pick_rgb(rng, &[0xe8dcc0, 0xd8c8a0]);
            p.liquid = pick_rgb(rng, &[0x5a1020, 0x7a2a1a, 0xc8a040]);
            p.fill = if rng.chance(0.85) { rng.range_f32(0.3, 1.0) } else { 0.0 };
            p.seal = rng.chance(0.4).then(|| pick_rgb(rng, &[0x8a1f1f, 0x2a2a2a, 0x2f5a2a]));
        }
        F::Potion => {
            let brew = *rng.pick(&Brew::ALL);
            // Clear glass reads the colour of what it holds.
            p.outer = mix(GLASS, brew.rgb(), 0.4);
            p.liquid = brew.rgb();
            p.fill = rng.range_f32(0.3, 0.95);
            p.glow = rng.range_f32(0.0, 0.9);
            p.count = rng.int(2, 7);
            p.seal = rng.chance(0.3).then(|| pick_rgb(rng, &[0x8a1f1f, 0x2a2a2a, 0xc9a13b]));
        }
        F::Waterskin => {
            p.vessel = Leather;
            p.fill = if rng.chance(0.85) { rng.range_f32(0.3, 1.0) } else { 0.0 };
            p.liquid = Rgb::hex(0x8fa8b0);
        }
        F::Flask => {
            p.vessel = *rng.weighted(&[(Leather, 3), (Brass, 2), (Silver, 2), (Iron, 1), (Gem, 2), (Wood, 1)]);
            p.outer = pick_rgb(rng, &[0x6a8a7a, 0x8aa0b0, 0x7a5a3a]);
            p.liquid = Rgb::hex(0xc8a040);
            p.fill = rng.range_f32(0.2, 1.0);
            p.seal = (p.vessel != Leather && rng.chance(0.25)).then(|| pick_rgb(rng, &[0x8a1f1f, 0x2a2a2a]));
        }
    }
    s.fit(&LIMITS);
    s
}

// ── Parts ───────────────────────────────────────────────────────────────────────────────

/// How a part is drawn: a palette material, or a material in its own colour with a glow.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Look {
    Material(MaterialKind),
    Tinted { material: MaterialKind, tint: Rgb, glow: f32, outline: bool },
}

/// Named meshes on their way to an [`ItemMesh`]; meshes with the same name and look merge.
/// Method names follow `build::parts::Parts`; this adds glowing tints and setting down.
#[derive(Default)]
struct Parts(Vec<(&'static str, MeshBuilder, Look)>);

impl Parts {
    fn add_look(&mut self, name: &'static str, mesh: MeshBuilder, look: Look) {
        if mesh.is_empty() {
            return;
        }
        match self.0.iter_mut().find(|(n, _, l)| *n == name && *l == look) {
            Some((_, existing, _)) => {
                existing.append(&mesh);
            }
            None => self.0.push((name, mesh, look)),
        }
    }

    fn add(&mut self, name: &'static str, mesh: MeshBuilder, material: MaterialKind) {
        self.add_look(name, mesh, Look::Material(material));
    }

    fn add_tinted(&mut self, name: &'static str, mesh: MeshBuilder, material: MaterialKind, tint: Rgb) {
        self.add_look(name, mesh, Look::Tinted { material, tint, glow: 0.0, outline: true });
    }

    fn for_each_mesh(&mut self, mut f: impl FnMut(&mut MeshBuilder)) {
        self.0.iter_mut().for_each(|(_, m, _)| f(m));
    }

    /// Moves everything to rest on y = 0, centred over the origin: set down on a table.
    fn set_down(&mut self) {
        let bounds = self.0.iter().filter_map(|(_, m, _)| m.bounds()).reduce(|(lo, hi), (a, b)| {
            ([lo[0].min(a[0]), lo[1].min(a[1]), lo[2].min(a[2])], [hi[0].max(b[0]), hi[1].max(b[1]), hi[2].max(b[2])])
        });
        if let Some((lo, hi)) = bounds {
            let offset = [-(lo[0] + hi[0]) * 0.5, -lo[1], -(lo[2] + hi[2]) * 0.5];
            self.for_each_mesh(|m| {
                m.translate(offset);
            });
        }
    }

    fn finish(self) -> ItemMesh {
        let parts = self
            .0
            .into_iter()
            .map(|(name, mesh, look)| match look {
                Look::Material(m) => mesh.into_part(name, OutlineMode::Welded).with_material(m),
                Look::Tinted { material, tint, glow, outline } => {
                    let part = mesh.into_part(name, OutlineMode::Welded).with_material(material).with_tint(tint).with_glow(glow);
                    if outline { part } else { part.without_outline() }
                }
            })
            .collect();
        ItemMesh { parts }
    }
}

// ── Build helpers ───────────────────────────────────────────────────────────────────────

/// Signed power: keeps the sign, shapes the magnitude (squarer or pointier sections).
fn sgp(v: f32, e: f32) -> f32 {
    v.signum() * v.abs().powf(e)
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    Rgb(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}

fn darken(c: Rgb, k: f32) -> Rgb {
    Rgb(c.0 * k, c.1 * k, c.2 * k)
}

fn food(a: &mut Parts, name: &'static str, mesh: MeshBuilder, color: Rgb) {
    a.add_tinted(name, mesh, MaterialKind::Food, color);
}

fn glowing(a: &mut Parts, name: &'static str, mesh: MeshBuilder, color: Rgb, glow: f32) {
    a.add_look(name, mesh, Look::Tinted { material: MaterialKind::Food, tint: color, glow, outline: true });
}

/// Glass in its own colour.
fn glass(a: &mut Parts, mesh: MeshBuilder, color: Rgb) {
    a.add_tinted("glass", mesh, Gem, color);
}

/// The drink inside a vessel: no outline, since it shows through glass or over a rim.
fn drink(a: &mut Parts, mesh: MeshBuilder, color: Rgb, glow: f32) {
    a.add_look("drink", mesh, Look::Tinted { material: Gem, tint: color, glow, outline: false });
}

/// Flips a closed surface whose faces point inward.
fn outward(mut mesh: MeshBuilder) -> MeshBuilder {
    let center = mesh.bounds().map(|(lo, hi)| vec3::lerp(lo, hi, 0.5)).unwrap_or(vec3::ZERO);
    let mut sum = 0.0;
    for t in mesh.indices.as_chunks::<3>().0 {
        let [p0, p1, p2] = [t[0], t[1], t[2]].map(|i| mesh.positions[i as usize]);
        let face = vec3::cross(vec3::sub(p1, p0), vec3::sub(p2, p0));
        sum += vec3::dot(face, vec3::sub(vec3::scale(vec3::add(vec3::add(p0, p1), p2), 1.0 / 3.0), center));
    }
    if sum < 0.0 {
        mesh.flip();
    }
    mesh
}

/// Which angles of a turned solid are left: all of it, or a sector `start..end` (radians, in
/// the lathe's convention).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kept {
    Whole,
    Sector(f32, f32),
}

impl Kept {
    /// What a portion leaves of a turned solid: a `wedge`-wide gap for a cut, or only that
    /// wedge for a piece.
    fn of(portion: Portion, wedge: f32) -> Kept {
        match portion {
            Portion::Cut => Kept::Sector(wedge * 0.5, TAU - wedge * 0.5),
            Portion::Piece => Kept::Sector(-wedge * 0.5, wedge * 0.5),
            _ => Kept::Whole,
        }
    }

    /// Whether angle `a` is kept, at least `margin` inside the cut.
    fn contains(self, a: f32, margin: f32) -> bool {
        match self {
            Kept::Whole => true,
            Kept::Sector(start, end) => {
                let a = start + (a - start).rem_euclid(TAU);
                a >= start + margin && a <= end - margin
            }
        }
    }

    /// The arc `(start, end)` a ring round the solid covers.
    fn arc(self) -> (f32, f32) {
        match self {
            Kept::Whole => (0.0, TAU),
            Kept::Sector(s, e) => (s, e),
        }
    }
}

/// A turned solid, cut to `kept`: the surface (and any rind band) goes to `outer_name` in
/// `outer`, the cut faces' core to `core_name` in `core`.
#[allow(clippy::too_many_arguments)]
fn turned_food(
    a: &mut Parts,
    profile: &[[f32; 2]],
    kept: Kept,
    facets: usize,
    rind: f32,
    (outer_name, outer): (&'static str, Rgb),
    (core_name, core): (&'static str, Rgb),
) {
    let options = LatheOptions::new(facets, Shading::SMOOTH);
    match kept {
        Kept::Whole => {
            food(a, outer_name, lathe(profile, &options), outer);
        }
        Kept::Sector(start, end) => {
            let steps = ((facets as f32) * (end - start) / TAU).ceil().max(4.0) as usize;
            let body = lathe_sector(profile, &LatheOptions { facets: steps, ..options }, start, end);
            food(a, outer_name, body, outer);
            let (rind_mesh, core_mesh) = cut_faces(profile, rind, start, end);
            food(a, outer_name, rind_mesh, outer);
            food(a, core_name, core_mesh, core);
        }
    }
}

/// The two faces of a sector cut from `start` to `end`, as `(rind, core)`.
fn cut_faces(profile: &[[f32; 2]], rind: f32, start: f32, end: f32) -> (MeshBuilder, MeshBuilder) {
    let (mut r0, mut c0) = profile_face(profile, rind);
    let (mut r1, mut c1) = (r0.clone(), c0.clone());
    r0.rotate_y(start);
    c0.rotate_y(start);
    r1.flip().rotate_y(end);
    c1.flip().rotate_y(end);
    r0.append(&r1);
    c0.append(&c1);
    (r0, c0)
}

/// Places a mesh built on the cut faces' plane (XY, facing +Z) onto both faces of a sector.
fn on_cut_faces(mesh: &MeshBuilder, start: f32, end: f32) -> MeshBuilder {
    let mut near = mesh.clone();
    near.rotate_y(start);
    // Mirrored, so what stood proud of the near face stands proud of the far one.
    let mut far = mesh.clone();
    far.scale([1.0, 1.0, -1.0]).rotate_y(end);
    near.append(&far);
    near
}

/// A flat disc of `radius` in the XY plane at z = `lift`, facing +Z.
fn disc(radius: f32, center: [f32; 2], lift: f32, sides: usize) -> MeshBuilder {
    let mut m = MeshBuilder::new();
    let ring: Vec<V3> = regular_polygon(sides, radius, 0.0).into_iter().map(|q| [center[0] + q[0], center[1] + q[1], lift]).collect();
    m.push_fan_cap(&ring, vec3::Z);
    m
}

/// A smooth flattened blob (scores, char, vents, spots): a sphere scaled to `size`.
fn blob(size: V3) -> MeshBuilder {
    let mut m = uv_sphere(1.0, 16, 10);
    m.scale(size);
    m
}

/// Seeded low-frequency lumps: a sum of a few sine waves in space, about -1..=1.
struct Lumps([(V3, f32); 4]);

impl Lumps {
    fn new(rng: &mut Mulberry32, frequency: f32) -> Lumps {
        let mut wave = || {
            let dir = vec3::normalize_or([rng.range_f32(-1.0, 1.0), rng.range_f32(-1.0, 1.0), rng.range_f32(-1.0, 1.0)], vec3::Y);
            (vec3::scale(dir, frequency * rng.range_f32(0.7, 1.6)), rng.range_f32(0.0, TAU))
        };
        Lumps([wave(), wave(), wave(), wave()])
    }

    fn at(&self, p: V3) -> f32 {
        self.0.iter().map(|&(f, phase)| (vec3::dot(f, p) + phase).sin()).sum::<f32>() * 0.4
    }
}

/// A leaf on a short petiole, built along +X from the origin, double sided.
fn leaf(length: f32, width: f32) -> MeshBuilder {
    let mut m = parametric_surface(14, 6, &GridOptions::default(), |u, v| {
        let across = 2.0 * v - 1.0;
        let w = width * (PI * u).sin().max(0.0).powf(0.7) * (1.0 - 0.25 * u);
        [u * length, 0.3 * w * across.abs() - 0.25 * length * u * u, across * w]
    });
    m.make_double_sided();
    m
}

// ── Breads ──────────────────────────────────────────────────────────────────────────────

fn loaf(p: &Provision, a: &mut Parts) {
    let s = p.size;
    let length = 3.0 * s * (0.8 + p.shape * 0.5);
    let r0 = 0.62 * s;
    let lift = 0.18 * r0;
    let radius = |x: f32| r0 * (1.0 - (2.0 * x / length).powi(2)).max(0.0).powf(0.42);
    // A domed top over a flat-ish bottom, in (z, y) for a station x.
    let section = |x: f32, th: f32| -> [f32; 2] {
        let (sn, c) = th.sin_cos();
        let r = radius(x);
        let y = if sn >= 0.0 { r * 0.8 * sgp(sn, 0.8) } else { r * 0.18 * sn };
        [r * sgp(c, 0.85), lift + y]
    };
    let bitten = p.portion_shown() == Portion::Bitten;
    let around = if bitten { 96 } else { 64 };
    let ring = |x: f32| -> Vec<[f32; 2]> { (0..around).map(|j| section(x, TAU * j as f32 / around as f32)).collect() };
    let crust_thickness = 0.05 * s;
    let portion = p.portion_shown();

    if portion == Portion::Piece {
        // One slice from the middle, laid flat.
        let thick = 0.22 * s;
        let outline = ring(0.0);
        let rows: Vec<Vec<V3>> = [-thick * 0.5, thick * 0.5].iter().map(|&x| outline.iter().map(|q| [x, q[1], q[0]]).collect()).collect();
        let wall = outward(stitch_grid(&rows, &GridOptions { wrap_around: true, shading: Shading::BANDED, ..Default::default() }));
        food(a, "crust", wall, p.outer);
        let flat: Vec<[f32; 2]> = outline.iter().map(|q| [-q[0], q[1]]).collect();
        let (rind, core) = rind_face(&flat, crust_thickness);
        for (side, x) in [(1.0f32, thick * 0.5), (-1.0, -thick * 0.5)] {
            for (name, mut mesh, color) in [("crust", rind.clone(), p.outer), ("crumb", core.clone(), p.inner)] {
                mesh.rotate_y(FRAC_PI_2).scale([side, 1.0, 1.0]).translate([x, 0.0, 0.0]);
                food(a, name, mesh, color);
            }
        }
        a.for_each_mesh(|m| {
            m.rotate_z(FRAC_PI_2).rotate_x(0.25);
        });
        return;
    }

    // The end comes off a cut loaf a fifth of the way from the middle.
    let cut_at = if portion == Portion::Cut { Some(length * 0.18) } else { None };
    let rows_n = if bitten { 96 } else { 56 };
    let stations: Vec<f32> = (0..=rows_n)
        .map(|i| {
            let v = i as f32 / rows_n as f32;
            match cut_at {
                Some(xc) => -length * 0.5 + (xc + length * 0.5) * (1.0 - (FRAC_PI_2 * v).cos()),
                None => -length * 0.5 * (PI * v).cos(),
            }
        })
        .collect();
    let rows: Vec<Vec<V3>> = stations.iter().map(|&x| ring(x).into_iter().map(|q| [x, q[1], q[0]]).collect()).collect();
    let mut crust = outward(stitch_grid(&rows, &GridOptions { wrap_around: true, ..Default::default() }));

    if let Some(xc) = cut_at {
        let flat: Vec<[f32; 2]> = ring(xc).iter().map(|q| [-q[0], q[1]]).collect();
        let (mut rind, mut core) = rind_face(&flat, crust_thickness);
        for m in [&mut rind, &mut core] {
            m.rotate_y(FRAC_PI_2).translate([xc, 0.0, 0.0]);
        }
        food(a, "crust", rind, p.outer);
        food(a, "crumb", core, p.inner);
    }
    // A bite from the front, a little toward one end.
    let bite_at = bitten.then_some(length * 0.3);
    if let Some(bx) = bite_at {
        let hollows = [
            Hollow::bite([bx, lift, 0.0], [0.15, 0.7, 1.0], radius(bx) * 0.85, 0.65 * r0),
            Hollow::bite([bx + 0.35 * r0, lift, 0.0], [0.0, 0.9, 1.0], radius(bx) * 0.85, 0.6 * r0),
        ];
        let (kept, bitten) = carve_hollows(&crust, &hollows);
        crust = kept;
        food(a, "crumb", bitten, p.inner);
    }
    food(a, "crust", crust, p.outer);

    // Slashed scores, opened into pale ears along the top.
    let n = p.count.max(1);
    for i in 0..n {
        let x = -length * 0.34 + length * 0.68 * if n == 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
        let in_bite = bite_at.is_some_and(|bx| (x - bx).abs() < r0);
        if x > cut_at.unwrap_or(f32::MAX) - 0.12 * length || in_bite {
            continue;
        }
        let mut score = blob([length * 0.1, 0.045 * s, 0.085 * s]);
        score.rotate_y(0.65).translate([x, lift + radius(x) * 0.8 - 0.012 * s, 0.0]);
        food(a, "scores", score, p.accent);
    }
}

fn boule(p: &Provision, a: &mut Parts) {
    let s = p.size;
    let r = 1.0 * s;
    let h = r * (0.7 + p.shape * 0.35);
    let profile = spline_profile(&[[0.0, 0.0], [r * 0.9, 0.0], [r, h * 0.15], [r * 0.93, h * 0.5], [r * 0.62, h * 0.85], [0.0, h]], 48);
    let kept = Kept::of(p.portion_shown(), 0.9);
    turned_food(a, &profile, kept, 56, 0.06 * s, ("crust", p.outer), ("crumb", p.inner));
    // Scores radiate from the crown: each line through the middle is two streaks.
    let streaks = 2 * p.count.max(2);
    for k in 0..streaks {
        let angle = k as f32 * PI / p.count.max(2) as f32;
        if !kept.contains(angle, 0.18) {
            continue;
        }
        let (sn, c) = angle.sin_cos();
        let path: Vec<V3> = (0..=12)
            .map(|i| {
                let rr = r * (0.1 + 0.52 * i as f32 / 12.0);
                let y = profile_height_at(&profile, rr).unwrap_or(h);
                [rr * c, y, -rr * sn]
            })
            .collect();
        let ear = regular_polygon(10, 1.0, 0.0).into_iter().map(|q| [q[0] * 0.025 * s, q[1] * 0.06 * s]).collect::<Vec<_>>();
        let mesh = sweep(&path, &ear, &SweepOptions { up: Some(vec3::Y), ..Default::default() }, |t| (PI * t).sin().powf(0.5));
        food(a, "scores", mesh, p.accent);
    }
}

// ── Cheese ──────────────────────────────────────────────────────────────────────────────

fn cheese(p: &Provision, rng: &mut Mulberry32, a: &mut Parts) {
    let s = p.size;
    let r = 1.4 * s;
    let h = r * (0.45 + p.shape * 0.4);
    let profile = spline_profile(
        &[
            [0.0, 0.0],
            [r * 0.92, 0.0],
            [r * 0.99, h * 0.06],
            [r, h * 0.2],
            [r, h * 0.8],
            [r * 0.99, h * 0.94],
            [r * 0.92, h],
            [0.0, h],
        ],
        56,
    );
    let kept = Kept::of(p.portion_shown(), 0.75);
    turned_food(a, &profile, kept, 64, 0.05 * s, ("rind", p.outer), ("paste", p.inner));
    if let Kept::Sector(start, end) = kept {
        // Eyes in the paste, on each cut face.
        let mut holes = MeshBuilder::new();
        let mut placed: Vec<([f32; 2], f32)> = Vec::new();
        for _ in 0..p.count {
            // A few tries for each eye to find room clear of the others.
            for _ in 0..6 {
                let at = [r * rng.range_f32(0.2, 0.85), h * rng.range_f32(0.22, 0.78)];
                let size = (s * rng.range_f32(0.05, 0.12)).min(h * 0.15);
                if placed.iter().all(|(q, qr)| (q[0] - at[0]).hypot(q[1] - at[1]) > qr + size + 0.02 * s) {
                    holes.append(&disc(size, at, 0.004, 16));
                    placed.push((at, size));
                    break;
                }
            }
        }
        a.add_look("holes", on_cut_faces(&holes, start, end), Look::Tinted { material: MaterialKind::Food, tint: darken(p.inner, 0.72), glow: 0.0, outline: false });
    }
}

// ── Fruit ───────────────────────────────────────────────────────────────────────────────

fn fruit(p: &Provision, rng: &mut Mulberry32, a: &mut Parts) {
    let s = p.size;
    let apple = p.style == FoodStyle::Apple;
    let r = if apple { 0.42 } else { 0.38 } * s;
    let h = if apple { 0.82 } else { 1.15 } * s * (0.85 + p.shape * 0.3);
    let control: Vec<[f32; 2]> = if apple {
        vec![[0.0, h * 0.06], [r * 0.55, 0.0], [r * 0.92, h * 0.22], [r, h * 0.52], [r * 0.88, h * 0.82], [r * 0.5, h * 0.98], [r * 0.16, h * 0.92], [0.0, h * 0.88]]
    } else {
        vec![[0.0, 0.0], [r * 0.62, h * 0.02], [r, h * 0.24], [r * 0.92, h * 0.44], [r * 0.56, h * 0.66], [r * 0.42, h * 0.84], [r * 0.22, h * 0.97], [0.0, h]]
    };
    let profile = spline_profile(&control, 56);
    let portion = p.portion_shown();
    let kept = if portion == Portion::Cut { Kept::Sector(0.45, TAU - 0.45) } else { Kept::Whole };
    if portion == Portion::Bitten {
        // Two overlapping bites out of one side.
        let hollows = [
            Hollow::bite([0.0, h * 0.5, 0.0], [0.15, 0.1, 1.0], r, r * 0.6),
            Hollow::bite([0.0, h * 0.45, 0.0], [-0.3, -0.05, 1.0], r, r * 0.55),
        ];
        // Finely divided, so the bite's edge is clean.
        let fine = spline_profile(&control, 96);
        let (skin, flesh) = carve_hollows(&lathe(&fine, &LatheOptions::new(96, Shading::SMOOTH)), &hollows);
        food(a, "skin", skin, p.outer);
        food(a, "flesh", flesh, p.inner);
    } else {
        turned_food(a, &profile, kept, 48, 0.012 * s, ("skin", p.outer), ("flesh", p.inner));
    }
    if let Kept::Sector(start, end) = kept {
        // Pips either side of the core.
        let mut pips = MeshBuilder::new();
        for y in [0.45, 0.58] {
            let mut pip = disc(1.0, [0.0, 0.0], 0.0, 10);
            pip.scale([0.035 * s, 0.06 * s, 1.0]).translate([r * 0.14, h * y, 0.003]);
            pips.append(&pip);
        }
        a.add_look("pips", on_cut_faces(&pips, start, end), Look::Tinted { material: MaterialKind::Food, tint: DARK, glow: 0.0, outline: false });
    }
    let top = if apple { h * 0.88 } else { h };
    let stem_path = CatmullRom::centripetal(vec![[0.0, top - 0.03 * s, 0.0], [0.02 * s, top + 0.1 * s, 0.0], [0.06 * s, top + 0.18 * s, 0.0]]).sample(8);
    food(a, "stem", tube(&stem_path, 8, &SweepOptions::default(), |t| 0.02 * s * (1.0 - 0.3 * t)), STEM);
    if rng.chance(0.75) {
        let mut l = leaf(0.32 * s, 0.09 * s);
        l.rotate_x(rng.range_f32(-0.4, 0.4)).rotate_z(0.35).rotate_y(rng.range_f32(0.0, TAU)).translate([0.03 * s, top + 0.12 * s, 0.0]);
        food(a, "leaf", l, p.accent);
    }
}

// ── Meat and fish ───────────────────────────────────────────────────────────────────────

fn haunch(p: &Provision, rng: &mut Mulberry32, a: &mut Parts) {
    let s = p.size;
    let length = 2.2 * s;
    let r = 0.62 * s * (0.85 + p.shape * 0.3);
    let bitten = p.portion_shown() == Portion::Bitten;
    let control = [[0.0, 0.0], [r * 0.62, length * 0.06], [r, length * 0.34], [r * 0.84, length * 0.66], [r * 0.42, length * 0.9], [r * 0.32, length]];
    let mut profile = spline_profile(&control, if bitten { 96 } else { 48 });
    profile.push([0.0, length]);
    let lumps = Lumps::new(rng, 2.5 / r);
    let warp = |q: V3| -> V3 {
        let k = 1.0 + 0.07 * lumps.at(q);
        [q[0] * k, q[1], q[2] * k]
    };
    let mut meat = lathe_warped(&profile, &LatheOptions::new(if bitten { 96 } else { 40 }, Shading::SMOOTH), warp);
    // Bites out of the meaty front, which faces +Z once the haunch is laid down.
    let bites = [
        Hollow::bite([0.0, length * 0.5, 0.0], [0.0, 0.0, 1.0], r * 0.9, r * 0.7),
        Hollow::bite([0.0, length * 0.38, 0.0], [0.45, 0.0, 1.0], r * 0.95, r * 0.65),
    ];
    if bitten {
        let (kept, flesh) = carve_hollows(&meat, &bites);
        meat = kept;
        food(a, "flesh", flesh, p.inner);
    }
    food(a, "meat", meat, p.outer);
    // Charred, crackled patches.
    for _ in 0..p.count {
        let t = 0.2 + 0.6 * rng.next_f64() as f32;
        let angle = rng.next_f64() as f32 * PI * 1.8 - PI * 0.9;
        let y = length * t;
        let along = profile_radius_at(&profile, y, false).unwrap_or(r);
        let (sn, c) = angle.sin_cos();
        let on = warp([along * c, y, -along * sn]);
        if bitten && bites.iter().any(|b| vec3::distance(on, b.center) < b.radius + 0.12 * s) {
            continue;
        }
        let mut patch = blob([0.22 * s, 0.02 * s, 0.15 * s]);
        patch.rotate_y(rng.range_f32(0.0, PI)).align_y_to([c, 0.15, -sn]).translate(vec3::scale(on, 0.995));
        food(a, "char", patch, p.accent);
    }
    // The knuckle of bone at the narrow end.
    let bone_len = 0.9 * s;
    let bone = lathe(
        &spline_profile(&[[0.11 * s, length - 0.2 * s], [0.085 * s, length + bone_len * 0.3], [0.08 * s, length + bone_len * 0.65], [0.1 * s, length + bone_len * 0.88], [0.0, length + bone_len]], 20),
        &LatheOptions::new(16, Shading::SMOOTH),
    );
    a.add("bone", bone, Bone);
    for side in [-1.0f32, 1.0] {
        let mut knob = uv_sphere(0.12 * s, 16, 10);
        knob.translate([side * 0.08 * s, length + bone_len - 0.06 * s, 0.0]);
        a.add("bone", knob, Bone);
    }
    // Laid down on its side, bone end raised.
    a.for_each_mesh(|m| {
        m.rotate_z(-1.2);
    });
}

fn fish(p: &Provision, a: &mut Parts) {
    let s = p.size;
    let length = 3.0 * s;
    let tall = 0.55 * s * (0.8 + p.shape * 0.5);
    let spine = CatmullRom::centripetal(vec![[-length / 2.0, 0.0, 0.0], [-length / 6.0, 0.06 * s, 0.0], [length / 6.0, 0.04 * s, 0.0], [length / 2.0 * 0.86, -0.06 * s, 0.0]]);
    let height = |t: f32| tall * if t < 0.22 { (t / 0.22).sqrt() } else { 1.0 - 0.86 * ((t - 0.22) / 0.78).powf(1.3) };
    let width = |t: f32| height(t) * 0.42;
    // Section: a deep lens, fuller toward the belly.
    let at = |t: f32, th: f32| -> V3 {
        let c = spine.point(t);
        let (sn, cs) = th.sin_cos();
        let belly = if sn < 0.0 { 1.08 } else { 1.0 };
        [c[0], c[1] + height(t) * sn * belly, width(t) * sgp(cs, 0.9)]
    };
    let (rows_n, around) = (72, 40);
    let rows: Vec<Vec<V3>> = (0..=rows_n)
        .map(|i| {
            let t = 1.0 - (FRAC_PI_2 * (1.0 - i as f32 / rows_n as f32)).sin();
            (0..around).map(|j| at(t, TAU * j as f32 / around as f32)).collect()
        })
        .collect();
    let mut body = stitch_grid(&rows, &GridOptions { wrap_around: true, ..Default::default() });
    let tail_ring = rows[rows_n].clone();
    body.push_fan_cap(&tail_ring, vec3::X);
    food(a, "body", outward(body), p.outer);

    let tail_end = spine.point(1.0);
    let fin = |outline: &[[f32; 2]], depth: f32| extrude(outline, &[], depth);
    let mut tail = fin(&[[0.0, 0.0], [0.5 * s, 0.45 * s], [0.4 * s, 0.05 * s], [0.5 * s, -0.4 * s]], 0.02 * s);
    tail.translate([tail_end[0] - 0.08 * s, tail_end[1], 0.0]);
    food(a, "fins", tail, p.accent);
    let dorsal_at = at(0.45, FRAC_PI_2);
    let mut dorsal = fin(&[[0.0, 0.0], [0.15 * s, 0.28 * s], [0.6 * s, 0.2 * s], [0.75 * s, 0.0]], 0.015 * s);
    dorsal.translate([dorsal_at[0] - 0.3 * s, dorsal_at[1] - 0.04 * s, 0.0]);
    food(a, "fins", dorsal, p.accent);
    let anal_at = at(0.7, -FRAC_PI_2);
    let mut anal = fin(&[[0.0, 0.0], [0.3 * s, 0.0], [0.08 * s, -0.16 * s]], 0.012 * s);
    anal.translate([anal_at[0] - 0.15 * s, anal_at[1] + 0.03 * s, 0.0]);
    food(a, "fins", anal, p.accent);
    for side in [-1.0f32, 1.0] {
        // Pectoral fins lie back against the flanks.
        let root = at(0.27, -0.5);
        let mut pectoral = fin(&[[0.0, 0.0], [0.28 * s, 0.06 * s], [0.24 * s, -0.08 * s]], 0.01 * s);
        pectoral.rotate_y(side * 0.35).translate([root[0], root[1], side * (root[2].abs() + 0.01 * s)]);
        food(a, "fins", pectoral, p.accent);

        let eye_at = at(0.1, 0.35);
        let mut eye = uv_sphere(0.07 * s, 14, 10);
        eye.translate([eye_at[0], eye_at[1], side * eye_at[2].abs() * 0.92]);
        food(a, "eye", eye, Rgb::hex(0xe8e4d8));
        let mut pupil = uv_sphere(0.04 * s, 12, 8);
        pupil.translate([eye_at[0], eye_at[1], side * (eye_at[2].abs() * 0.92 + 0.045 * s)]);
        food(a, "pupil", pupil, DARK);

        let gill: Vec<V3> = (0..=12)
            .map(|k| {
                let th = 0.9 - 1.7 * k as f32 / 12.0;
                let q = at(0.2 + 0.03 * (1.0 - (th * 1.2).cos()), th);
                [q[0], q[1], side * (q[2].abs() + 0.004 * s)]
            })
            .collect();
        food(a, "gills", tube(&gill, 6, &SweepOptions::default(), |t| 0.012 * s * (PI * t).sin().max(0.3)), DARK);
    }
}

fn sausages(p: &Provision, a: &mut Parts) {
    let s = p.size;
    let n = p.count.max(1) as usize;
    let link = 0.8 * s * (0.85 + 0.3 * p.shape);
    let r = 0.17 * s * (0.8 + p.shape * 0.5);
    let span = link * n as f32;
    // A loop of links hanging between two hands, or one sausage gently bent.
    let width = if n == 1 { span * 0.95 } else { span * 0.68 };
    let sag = if n == 1 { 0.08 * span } else { 0.26 * span };
    let rough: Vec<V3> = (0..=24)
        .map(|i| {
            let u = i as f32 / 24.0;
            [width * (u - 0.5), -sag * (1.0 - (2.0 * u - 1.0).powi(2)), 0.1 * s * (TAU * u).sin()]
        })
        .collect();
    let path = resample_by_length(&CatmullRom::centripetal(rough).sample(4), 30 * n + 1, false);
    let links = n as f32;
    let radius = |t: f32| r * (0.32 + 0.68 * (PI * links * t).sin().abs().powf(0.35));
    food(a, "casing", tube(&path, 16, &SweepOptions::default(), radius), p.outer);
    let tangent = |i: usize| vec3::normalize_or(vec3::sub(path[(i + 1).min(path.len() - 1)], path[i.saturating_sub(1)]), vec3::X);
    for k in 1..n {
        let i = k * (path.len() - 1) / n;
        let mut tie = torus(r * 0.36, r * 0.11, 16, 6, Shading::SMOOTH);
        tie.align_y_to(tangent(i)).translate(path[i]);
        a.add("ties", tie, Cord);
    }
    // Twisted-off ends.
    for (i, dir) in [(0, -1.0f32), (path.len() - 1, 1.0)] {
        let mut nub = spike(r * 0.3, r * 0.5, 8, 0.6, Shading::SMOOTH);
        nub.align_y_to(vec3::scale(tangent(i), dir)).translate(path[i]);
        food(a, "casing", nub, p.outer);
    }
}

// ── Pie ─────────────────────────────────────────────────────────────────────────────────

fn pie(p: &Provision, a: &mut Parts) {
    let s = p.size;
    let r = 1.15 * s;
    let h = 0.42 * s * (0.8 + p.shape * 0.5);
    let wall = 0.035 * s;
    let portion = p.portion_shown();
    let kept = Kept::of(portion, 0.8);

    let dish_outer = [[0.0, 0.0], [r * 0.82, 0.0], [r * 0.98, h * 0.68], [r * 1.1, h * 0.74], [r * 1.12, h * 0.78]];
    let dish = shell_profile(&dish_outer, wall);
    let inside = shell_inside(&dish, dish_outer.len());
    // The pie fills the dish and domes over its rim; its rind is the pastry all round.
    // Up the inside of the dish to just short of its flange, then over the dome.
    let mut body: Vec<[f32; 2]> = inside.iter().filter(|q| q[0] < r * 0.95).map(|&[x, y]| [x, y + 0.006 * s]).collect();
    let dome = spline_profile(&[[r * 1.0, h * 0.74], [r * 1.02, h * 0.82], [r * 0.92, h * 0.98], [r * 0.55, h * 1.25], [0.0, h * 1.32]], 32);
    body.extend(dome);
    turned_food(a, &body, kept, 72, 0.07 * s, ("crust", p.outer), ("filling", p.inner));

    if portion != Portion::Piece {
        let options = LatheOptions::new(64, Shading::BANDED);
        match kept {
            Kept::Whole => a.add("dish", lathe(&dish, &options), p.vessel),
            Kept::Sector(start, end) => {
                let steps = (64.0 * (end - start) / TAU).ceil() as usize;
                a.add("dish", lathe_sector(&dish, &LatheOptions { facets: steps, ..options }, start, end), p.vessel);
                let (_, faces) = cut_faces(&dish, 0.0, start, end);
                a.add("dish", faces, p.vessel)
            }
        };
    }
    // The crimped edge.
    let (start, end) = kept.arc();
    let closed = kept == Kept::Whole;
    let steps = (220.0 * (end - start) / TAU).ceil() as usize;
    let crimp: Vec<V3> = (0..steps + usize::from(!closed))
        .map(|i| {
            let t = start + (end - start) * i as f32 / steps as f32;
            [t.cos() * r * 1.04, h * 0.8 + 0.03 * s * (t * 28.0).sin(), -t.sin() * r * 1.04]
        })
        .collect();
    let options = if closed { SweepOptions::closed_loop() } else { SweepOptions::default() };
    food(a, "crust", tube(&crimp, 10, &options, |_| 0.065 * s), p.outer);
    // Steam vents.
    for i in 0..p.count {
        let angle = i as f32 / p.count as f32 * TAU + 0.3;
        if !kept.contains(angle, 0.2) {
            continue;
        }
        let rr = r * 0.32;
        let y = profile_height_at(&body, rr).unwrap_or(h * 1.3);
        let mut vent = blob([0.12 * s, 0.025 * s, 0.03 * s]);
        vent.rotate_y(angle).translate([angle.cos() * rr, y - 0.012 * s, -angle.sin() * rr]);
        a.add_look("vents", vent, Look::Tinted { material: MaterialKind::Food, tint: p.accent, glow: 0.0, outline: false });
    }
}

// ── Mushrooms ───────────────────────────────────────────────────────────────────────────

fn mushrooms(p: &Provision, rng: &mut Mulberry32, a: &mut Parts) {
    let s = p.size;
    let n = p.count.clamp(1, 5);
    for i in 0..n {
        let k = if i == 0 { 1.0 } else { rng.range_f32(0.55, 0.85) };
        let rc = 0.42 * s * k * (0.8 + 0.4 * p.shape);
        let hc = rc * rng.range_f32(0.45, 0.8);
        let hs = 0.9 * s * k * rng.range_f32(0.8, 1.2);
        let rs = 0.11 * s * k;
        let mut parts: Vec<(&'static str, MeshBuilder, Rgb)> = Vec::new();

        let stem = lathe(&spline_profile(&[[rs * 1.35, 0.0], [rs * 1.15, hs * 0.12], [rs, hs * 0.5], [rs * 0.9, hs]], 16), &LatheOptions { cap_end: false, ..LatheOptions::new(20, Shading::SMOOTH) });
        parts.push(("stem", stem, p.inner));
        let cap_profile = spline_profile(&[[rc, 0.0], [rc * 0.98, hc * 0.25], [rc * 0.75, hc * 0.75], [rc * 0.35, hc * 0.98], [0.0, hc]], 24);
        let mut cap = lathe(&cap_profile, &LatheOptions { cap_start: false, ..LatheOptions::new(40, Shading::SMOOTH) });
        cap.translate([0.0, hs * 0.97, 0.0]);
        parts.push(("cap", cap, p.outer));
        // Gills: radial ridges pressed into the underside.
        let gill_count = 30.0;
        let gill_depth = 0.07 * rc;
        let under = spline_profile(&[[rs * 0.85, -0.14 * hc], [rc * 0.5, -0.08 * hc], [rc, 0.0]], 12);
        let mut gills = lathe_warped(&under, &LatheOptions::new(150, Shading::SMOOTH), |q| {
            let radial = q[0].hypot(q[2]);
            let angle = (-q[2]).atan2(q[0]);
            let ridge = (gill_count * angle * 0.5).cos().abs().powi(8);
            let envelope = (PI * ((radial - rs) / (rc - rs)).clamp(0.0, 1.0)).sin();
            [q[0], q[1] - gill_depth * ridge * envelope, q[2]]
        });
        gills.translate([0.0, hs * 0.97, 0.0]);
        parts.push(("gills", gills, p.inner));

        let (lean, turn, place) = if i == 0 {
            (rng.range_f32(0.0, 0.12), 0.0, [0.0, 0.0, 0.0])
        } else {
            let angle = TAU * (i - 1) as f32 / (n - 1) as f32 + rng.range_f32(-0.4, 0.4);
            let d = 0.42 * s * rng.range_f32(0.9, 1.3);
            (rng.range_f32(0.25, 0.5), angle, [angle.cos() * d, 0.0, -angle.sin() * d])
        };
        for (name, mut mesh, color) in parts {
            // Lean away from the cluster's middle.
            mesh.rotate_z(-lean).rotate_y(turn).translate(place);
            let glow = if name == "stem" { p.glow * 0.3 } else { p.glow };
            glowing(a, name, mesh, color, glow);
        }
    }
}

// ── Drink ───────────────────────────────────────────────────────────────────────────────

fn stopper(radius_bottom: f32, radius_top: f32, height: f32, base: f32) -> MeshBuilder {
    let mut m = frustum(radius_bottom, radius_top, height, 16, Shading::SMOOTH);
    m.translate([0.0, base, 0.0]);
    m
}

/// Wax poured over a stopper from `base` up to `top`, `radius` wide at the neck.
fn wax_seal(a: &mut Parts, color: Rgb, radius: f32, base: f32, top: f32) {
    let profile = spline_profile(&[[radius * 1.12, base], [radius * 1.2, base + (top - base) * 0.4], [radius * 1.05, top - (top - base) * 0.1], [radius * 0.6, top + (top - base) * 0.05], [0.0, top + (top - base) * 0.08]], 20);
    let mut wax = lathe(&profile, &LatheOptions::new(24, Shading::SMOOTH));
    // Drips down the neck.
    for k in 0..3 {
        let angle = k as f32 * 2.2 + 0.4;
        let mut drip = blob([radius * 0.22, (top - base) * 0.5, radius * 0.18]);
        drip.translate([angle.cos() * radius * 1.12, base, -angle.sin() * radius * 1.12]);
        wax.append(&drip);
    }
    a.add_tinted("seal", wax, Bone, color);
}

fn tankard(p: &Provision, rng: &mut Mulberry32, a: &mut Parts) {
    let s = p.size;
    let r = 0.5 * s;
    let h = 1.3 * s * (0.8 + p.shape * 0.4);
    let wooden = p.vessel == Wood;
    let outer = spline_profile(&[[0.0, 0.0], [r * 0.96, 0.0], [r, h * 0.05], [r * 0.94, h * 0.4], [r * 0.94, h * 0.7], [r * 0.97, h * 0.94], [r, h]], 40);
    let wall = if wooden { 0.07 * s } else { 0.035 * s };
    let shell = shell_profile(&outer, wall);
    let staves = if wooden { rng.int(12, 16) as usize } else { 48 };
    let options = LatheOptions::new(staves, if wooden { Shading::FACETED } else { Shading::SMOOTH });
    a.add("vessel", lathe(&shell, &options), p.vessel);
    let fittings = if wooden { Iron } else { p.vessel };
    for y in if wooden { [h * 0.12, h * 0.85] } else { [h * 0.06, h * 0.92] } {
        let rr = profile_radius_at(&outer, y, false).unwrap_or(r) + 0.012 * s;
        let mut hoop = torus(rr, 0.03 * s, 64, 8, Shading::SMOOTH);
        hoop.translate([0.0, y, 0.0]);
        a.add("hoops", hoop, fittings);
    }
    let handle_path = CatmullRom::centripetal(vec![[r * 0.88, h * 0.82, 0.0], [r * 1.6, h * 0.78, 0.0], [r * 1.65, h * 0.35, 0.0], [r * 0.88, h * 0.22, 0.0]]).sample(12);
    let grip: Vec<[f32; 2]> = regular_polygon(12, 1.0, 0.0).into_iter().map(|q| [q[0] * 0.075 * s, q[1] * 0.045 * s]).collect();
    a.add("handle", sweep(&handle_path, &grip, &SweepOptions { up: Some(vec3::Z), ..Default::default() }, |_| 1.0), fittings);

    if p.is_full() {
        let inside = shell_inside(&shell, outer.len());
        let bottom = inside[0][1];
        let level = bottom + p.fill * (h - bottom) * 0.9;
        let liquid: Vec<[f32; 2]> = clip_profile(&inside, level).into_iter().map(|[x, y]| [x * 0.995, y]).collect();
        drink(a, lathe(&liquid, &LatheOptions::new(staves.max(24), Shading::SMOOTH)), p.liquid, 0.0);
        if p.fill >= 0.6 {
            // A head of foam, heaped over the rim when brim-full.
            let ri = profile_radius_at(&inside, level, true).unwrap_or(r - wall);
            let hf = r * 0.35 * p.fill;
            let lumps = Lumps::new(rng, 9.0 / r);
            let foam_profile = spline_profile(&[[ri * 1.01, -0.02 * s], [ri, hf * 0.4], [ri * 0.7, hf * 0.85], [0.0, hf]], 16);
            let mut foam = lathe_warped(&foam_profile, &LatheOptions::new(40, Shading::SMOOTH), |q| [q[0], q[1] + q[1].max(0.0) * 0.35 * lumps.at(q), q[2]]);
            foam.translate([0.0, level, 0.0]);
            food(a, "foam", foam, p.accent);
        }
    }
}

fn bottle(p: &Provision, a: &mut Parts) {
    let s = p.size;
    let r = 0.38 * s;
    let h = 2.9 * s * (0.85 + p.shape * 0.3);
    let profile = spline_profile(
        &[
            [0.0, 0.0],
            [r * 0.92, 0.0],
            [r, r * 0.1],
            [r, h * 0.58],
            [r * 0.9, h * 0.66],
            [r * 0.42, h * 0.76],
            [r * 0.3, h * 0.86],
            [r * 0.34, h * 0.9],
            [r * 0.3, h * 0.93],
            [r * 0.3, h * 0.97],
        ],
        72,
    );
    glass(a, lathe(&profile, &LatheOptions::new(40, Shading::SMOOTH)), p.outer);
    if p.is_full() {
        let inner: Vec<[f32; 2]> = profile.iter().map(|&[x, y]| [x * 0.9, y]).collect();
        let level = r * 0.1 + p.fill * (h * 0.86 - r * 0.1);
        drink(a, lathe(&clip_profile(&inner, level), &LatheOptions::new(32, Shading::SMOOTH)), p.liquid, 0.0);
    }
    a.add_tinted("stopper", stopper(r * 0.24, r * 0.27, h * 0.08, h * 0.95), Wood, CORK);
    if let Some(wax) = p.seal {
        wax_seal(a, wax, r * 0.32, h * 0.9, h * 1.03);
    }
    // The paper follows the glass, standing just proud of it.
    let label_profile: Vec<[f32; 2]> = (0..=8)
        .map(|i| {
            let y = h * (0.2 + 0.25 * i as f32 / 8.0);
            [profile_radius_at(&profile, y, false).unwrap_or(r) + 0.006 * s, y]
        })
        .collect();
    let label = lathe_sector(&label_profile,&LatheOptions { cap_start: false, cap_end: false, ..LatheOptions::new(20, Shading::SMOOTH) }, -1.0, 1.0);
    a.add_tinted("label", label, Cloth, p.accent);
}

fn potion(p: &Provision, rng: &mut Mulberry32, a: &mut Parts) {
    let s = p.size;
    let r = 0.62 * s * (0.8 + p.shape * 0.4);
    // Tall vials for the narrowest, round-bellied flasks otherwise.
    let vial = p.shape > 0.75;
    let (control, neck): (Vec<[f32; 2]>, f32) = if vial {
        (vec![[0.0, 0.0], [r * 0.48, 0.0], [r * 0.55, r * 0.12], [r * 0.55, r * 1.9], [r * 0.4, r * 2.1], [r * 0.3, r * 2.25], [r * 0.38, r * 2.4], [r * 0.33, r * 2.45]], r * 2.1)
    } else {
        (vec![[0.0, 0.0], [r * 0.5, r * 0.06], [r * 0.95, r * 0.6], [r, r], [r * 0.82, r * 1.55], [r * 0.36, r * 1.88], [r * 0.3, r * 2.3], [r * 0.38, r * 2.4], [r * 0.33, r * 2.45]], r * 1.85)
    };
    let profile = spline_profile(&control, 64);
    glass(a, lathe(&profile, &LatheOptions::new(36, Shading::SMOOTH)), p.outer);
    if p.is_full() {
        let inner: Vec<[f32; 2]> = profile.iter().map(|&[x, y]| [x * 0.93, y.max(r * 0.04)]).collect();
        let level = r * 0.08 + (neck - r * 0.08) * p.fill;
        drink(a, lathe(&clip_profile(&inner, level), &LatheOptions::new(32, Shading::SMOOTH)), p.liquid, p.glow);
        let mut bubbles = MeshBuilder::new();
        for _ in 0..p.count {
            let y = r * 0.25 + (level - r * 0.4).max(0.0) * rng.next_f64() as f32;
            let room = profile_radius_at(&inner, y, false).unwrap_or(r * 0.3) * 0.7;
            let (angle, rr) = (rng.range_f32(0.0, TAU), room * rng.next_f64() as f32);
            let mut b = icosphere(r * rng.range_f32(0.04, 0.09), 1);
            b.translate([angle.cos() * rr, y, -angle.sin() * rr]);
            bubbles.append(&b);
        }
        a.add_look("bubbles", bubbles, Look::Tinted { material: Gem, tint: Rgb::hex(0xffffff), glow: p.glow * 0.5, outline: false });
    }
    a.add_tinted("stopper", stopper(r * 0.25, r * 0.3, r * 0.42, r * 2.31), Wood, CORK);
    let mut cord = torus(r * 0.33, 0.025 * s, 32, 6, Shading::SMOOTH);
    cord.translate([0.0, r * 2.12, 0.0]);
    a.add("cord", cord, Cord);
    if let Some(wax) = p.seal {
        wax_seal(a, wax, r * 0.3, r * 2.32, r * 2.76);
    }
}

fn waterskin(p: &Provision, rng: &mut Mulberry32, a: &mut Parts) {
    let s = p.size;
    let full = p.fill;
    let r = 0.55 * s * (0.8 + 0.2 * full) * (0.9 + 0.2 * p.shape);
    let h = 1.5 * s;
    let profile = spline_profile(&[[0.0, 0.0], [r * 0.7, r * 0.06], [r, h * 0.3], [r * 0.92, h * 0.55], [r * 0.55, h * 0.8], [r * 0.2, h * 0.95], [r * 0.18, h]], 40);
    let flatten = 0.55 * (0.8 + 0.2 * full);
    let phase = rng.range_f32(0.0, TAU);
    // A slack skin creases; a full one is drum tight.
    let crease = (1.0 - full) * 0.06;
    let warp = |q: V3| -> V3 {
        let angle = (-q[2]).atan2(q[0]);
        let k = 1.0 + crease * (9.0 * q[1] / s + 3.0 * angle + phase).sin();
        [q[0] * k, q[1], q[2] * k * flatten]
    };
    a.add("body", lathe_warped(&profile, &LatheOptions::new(40, Shading::SMOOTH), warp), p.vessel);
    let neck = lathe(&[[r * 0.19, h * 0.93], [r * 0.16, h * 1.05], [r * 0.17, h * 1.12]], &LatheOptions { cap_start: false, ..LatheOptions::new(16, Shading::SMOOTH) });
    a.add("body", neck, p.vessel);
    for k in 0..3 {
        let mut tie = torus(r * 0.175, 0.016 * s, 20, 6, Shading::SMOOTH);
        tie.translate([0.0, h * 0.97 + k as f32 * 0.035 * s, 0.0]);
        a.add("ties", tie, Cord);
    }
    let mut plug = stopper(r * 0.13, r * 0.16, 0.16 * s, h * 1.08);
    let mut knob = uv_sphere(0.06 * s, 12, 8);
    knob.translate([0.0, h * 1.08 + 0.2 * s, 0.0]);
    plug.append(&knob);
    a.add("stopper", plug, Wood);
    // The stitched seam round the edge.
    let mut seam: Vec<V3> = profile.iter().skip(1).map(|&[x, y]| warp([x * 1.01, y, 0.0])).collect();
    let left: Vec<V3> = profile.iter().rev().skip(1).map(|&[x, y]| warp([-x * 1.01, y, 0.0])).collect();
    seam.reverse();
    seam.extend(left);
    a.add("seam", tube(&seam, 6, &SweepOptions::default(), |_| 0.015 * s), Cord);
    let strap_path = CatmullRom::centripetal(vec![[-r * 0.15, h * 0.98, 0.0], [-r * 0.9, h * 1.3, 0.0], [0.0, h * 1.55, 0.0], [r * 0.95, h * 1.25, 0.0], [r * 0.9, h * 0.6, 0.0]]).sample(10);
    let band: Vec<[f32; 2]> = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]].iter().map(|q| [q[0] * 0.05 * s, q[1] * 0.012 * s]).collect();
    a.add("strap", sweep(&strap_path, &band, &SweepOptions { up: Some(vec3::Z), shading: Shading::FACETED, ..Default::default() }, |_| 1.0), Leather);
}

fn flask(p: &Provision, a: &mut Parts) {
    let s = p.size;
    let r = 0.55 * s * (0.85 + 0.3 * p.shape);
    let t = r * 0.45;
    let lens = spline_profile(&[[0.0, -t], [r * 0.6, -t * 0.92], [r * 0.92, -t * 0.5], [r, 0.0], [r * 0.92, t * 0.5], [r * 0.6, t * 0.92], [0.0, t]], 40);
    let mut body = lathe(&lens, &LatheOptions::new(48, Shading::SMOOTH));
    // Stood on edge: the faces look forward and back.
    body.rotate_x(FRAC_PI_2).translate([0.0, r, 0.0]);
    let neck = lathe(&[[r * 0.2, r * 1.85], [r * 0.17, r * 2.25], [r * 0.23, r * 2.3], [r * 0.2, r * 2.38]], &LatheOptions { cap_start: false, ..LatheOptions::new(20, Shading::SMOOTH) });
    let is_glass = p.vessel == Gem;
    if is_glass {
        glass(a, body, p.outer);
        glass(a, neck, p.outer);
    } else {
        a.add("vessel", body, p.vessel);
        a.add("vessel", neck, p.vessel);
    }
    a.add_tinted("stopper", stopper(r * 0.15, r * 0.19, r * 0.32, r * 2.3), Wood, CORK);
    if let Some(wax) = p.seal {
        wax_seal(a, wax, r * 0.21, r * 2.26, r * 2.66);
    }
    let fittings = if matches!(p.vessel, Brass | Silver | Iron) { p.vessel } else { Brass };
    for side in [-1.0f32, 1.0] {
        let mut lug = torus(r * 0.1, r * 0.03, 16, 6, Shading::SMOOTH);
        lug.rotate_x(FRAC_PI_2).translate([side * r * 0.8, r * 1.62, 0.0]);
        a.add("lugs", lug, fittings);
        if matches!(p.vessel, Brass | Silver | Iron) {
            // A raised ring on each face.
            let mut ring = torus(r * 0.55, r * 0.03, 48, 8, Shading::SMOOTH);
            ring.rotate_x(FRAC_PI_2).translate([0.0, r, side * t * 0.88]);
            a.add("vessel", ring, p.vessel);
        }
    }
    if p.vessel == Leather {
        let seam: Vec<V3> = (0..64).map(|i| {
            let angle = TAU * i as f32 / 64.0;
            [angle.cos() * r * 1.01, r + angle.sin() * r * 1.01, 0.0]
        }).collect();
        a.add("seam", tube(&seam, 6, &SweepOptions::closed_loop(), |_| 0.02 * s), Cord);
    }
    let strap = CatmullRom::centripetal(vec![[-r * 0.8, r * 1.55, 0.0], [-r * 0.75, r * 2.5, 0.0], [0.0, r * 2.95, 0.0], [r * 0.75, r * 2.5, 0.0], [r * 0.8, r * 1.55, 0.0]]).sample(10);
    a.add("strap", tube(&strap, 8, &SweepOptions::default(), |_| 0.025 * s), Cord);
}
