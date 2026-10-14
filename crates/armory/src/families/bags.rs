//! Bags: pouches, purses, sacks, belt pouches, satchels and backpacks.
//!
//! Sliders, options and the noun come from `reference/spline_armory_engine.js` (the `bag`
//! section, `BASE.bag`, `PRESETS.bags`, the `fam==='bags'` branch of `randomize()`,
//! `buildBag()` and `forgeNoun()`).
//!
//! The roll departs from the prototype on purpose. The prototype rolls the front pocket,
//! bedroll and patch for every style although only backpacks draw the first two and only
//! sacks the patch; here they roll only where they show, and a sewn patch can go on any bag.
//! Cloth bags can take any woven pattern and leather ones can be studded.
//!
//! The geometry is rebuilt rather than ported: every panel has a real thickness, so the
//! mouth of a pouch and the edge of a flap read as leather or cloth, straps lie on the
//! surface they cross and end in proper buckles, and a satchel's strap hangs from D-rings.
//! [`Bags::container`] says where the mouth is and how much the bag holds.

use std::f32::consts::{PI, TAU};

use crate::build::container::{ContainerSpace, Opening, stacked_volume};
use crate::build::hafted::smoothstep;
use crate::build::hoard::coin_scatter;
use crate::build::parts::{Look, Parts};
use crate::build::roll::{roll_straps, rolled_sheet};
use crate::build::strap::{buckle, d_ring, strap, strap_along};
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::geom::{
    CatmullRom, GridOptions, ItemMesh, LatheOptions, MeshBuilder, Shading, SweepOptions, V3, grid_normals, lathe, spline_profile,
    thick_sheet, tube, uv_sphere,
};
use crate::geom::vec3;
use crate::material::{MaterialKind, Rgb};
use crate::palette::Palette;
use crate::fabric::{Fabric, FabricLimits, FabricPattern, Textile};
use crate::slider::{Sliders, sliders};
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

/// The bag generator.
pub struct Bags;

/// The prototype's `bag.style`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BagStyle {
    /// A round drawstring pouch.
    Pouch,
    /// A small drawstring coin purse.
    Purse,
    /// A small flapped pouch with loops for a belt.
    BeltPouch,
    /// A flapped bag on a shoulder strap.
    Satchel,
    /// A flapped pack on two shoulder straps.
    Backpack,
    /// A large drawstring sack.
    Sack,
}

impl BagStyle {
    pub const ALL: [BagStyle; 6] = [BagStyle::Pouch, BagStyle::Purse, BagStyle::BeltPouch, BagStyle::Satchel, BagStyle::Backpack, BagStyle::Sack];

    /// Gathered at the neck by a cord (pouch, purse, sack), as opposed to closed by a flap.
    pub fn is_drawstring(self) -> bool {
        matches!(self, BagStyle::Pouch | BagStyle::Purse | BagStyle::Sack)
    }
}

/// The prototype's `bag` section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bag {
    pub style: BagStyle,
    /// Overall scale.
    pub size: f32,
    /// Plumper (1) or slimmer (0): girth for drawstring bags and packs, depth for flapped bags.
    pub shape: f32,
    pub fabric: Textile,
    /// The body's colour and pattern. Its second colour is the pattern's and also the
    /// flap's, the bedroll's and the patch's, as in the prototype's `color2`.
    pub cloth: Fabric,
    /// Buckles, toggles and rings.
    pub fittings: MaterialKind,
    /// Coins spilled at its foot (pouches and purses).
    pub coins: bool,
    /// A front pocket (backpacks).
    pub pocket: bool,
    /// A rolled blanket strapped on top (backpacks).
    pub bedroll: bool,
    /// A sewn-on patch.
    pub patch: bool,
}

sliders! {
    BagLimits for Bag {
        size: 0.7, 1.4, 0.01;
        shape: 0.0, 1.0, 0.01;
    }
}

impl Default for Bag {
    /// The prototype's `BASE.bag`: an adventurer's pack.
    fn default() -> Self {
        Bag {
            style: BagStyle::Backpack,
            size: 1.0,
            shape: 0.5,
            fabric: Textile::Leather,
            cloth: Fabric { color: Rgb::hex(0x6b4a2e), pattern: FabricPattern::Plain, color2: Rgb::hex(0x7a8a5a), scale: 3.0 },
            fittings: MaterialKind::Iron,
            coins: false,
            pocket: true,
            bedroll: true,
            patch: false,
        }
    }
}

/// Every parameter of one bag.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BagState {
    pub seed: u32,
    pub bag: Bag,
    /// Tooling on leather, embroidery on cloth, engraving on the fittings.
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every bag section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BagsLimits {
    pub bag: BagLimits,
    pub cloth: FabricLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges (it has no `RANGES.bags`).
pub const LIMITS: BagsLimits = BagsLimits { bag: BagLimits::SLIDERS, cloth: FabricLimits::SLIDERS, etch: EtchingLimits::SLIDERS, cond: ConditionLimits::SLIDERS };

impl BagState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &BagsLimits) {
        self.bag.fit(&limits.bag);
        self.bag.cloth.fit(&limits.cloth);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &BagsLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("bag", self.bag.out_of_range(&limits.bag)),
            tag("bag.cloth", self.bag.cloth.out_of_range(&limits.cloth)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

impl Bags {
    /// Roll weights per style, in percent. Backpacks are the commonest find, as in the
    /// prototype's pick list.
    pub const STYLE_WEIGHTS: [(BagStyle, u32); 6] = [
        (BagStyle::Pouch, 16),
        (BagStyle::Purse, 12),
        (BagStyle::BeltPouch, 14),
        (BagStyle::Satchel, 16),
        (BagStyle::Backpack, 26),
        (BagStyle::Sack, 16),
    ];

    /// "Forge new" forced to one style, for turntables and tests. Draws the same stream as
    /// [`Generator::roll`], so `roll(s)` equals `roll_as(s, st)` when `st` is what `s` rolls.
    pub fn roll_as(seed: u32, style: BagStyle) -> BagState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, style)
    }

    /// The bag's mouth and inside, in the same item space as [`Generator::build`].
    pub fn container(s: &BagState) -> ContainerSpace {
        match Shape::of(&s.bag) {
            Shape::Drawstring(d) => {
                let rows = d.rows(48);
                let rn = d.radius_at(d.neck);
                let floor = s.bag.fabric.thickness();
                ContainerSpace {
                    opening: Opening::upward([0.0, d.neck, 0.0], [rn, rn * DEPTH_SQUASH], true),
                    interior_min: [-d.belly * 0.9, floor, -d.belly * DEPTH_SQUASH * 0.9],
                    interior_max: [d.belly * 0.9, d.neck, d.belly * DEPTH_SQUASH * 0.9],
                    litres: stacked_volume(&rows, d.neck),
                }
            }
            Shape::Flapped(f) => {
                let body = f.body();
                let rows = body.rows(16, 24);
                let t = s.bag.fabric.thickness();
                ContainerSpace {
                    opening: Opening::upward([0.0, f.height, 0.0], [f.width * 0.5 - t, f.depth * 0.5 - t], false),
                    interior_min: [-f.width * 0.5 + t, t, -f.depth * 0.5 + t],
                    interior_max: [f.width * 0.5 - t, f.height, f.depth * 0.5 - t],
                    litres: stacked_volume(&rows, f.height),
                }
            }
        }
    }
}

impl Generator for Bags {
    type State = BagState;

    fn roll(seed: u32) -> BagState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let style = *rng.weighted(&Bags::STYLE_WEIGHTS);
        compose(&mut rng, seed, style)
    }

    /// `forgeNoun()` for bags.
    fn noun(s: &BagState) -> String {
        let b = &s.bag;
        match b.style {
            BagStyle::Pouch => "Drawstring Pouch",
            BagStyle::Purse => "Coin Purse",
            BagStyle::BeltPouch => "Belt Pouch",
            BagStyle::Satchel => "Satchel",
            BagStyle::Backpack if b.bedroll => "Adventurer's Pack",
            BagStyle::Backpack => "Rucksack",
            BagStyle::Sack if b.fabric == Textile::Burlap => "Grain Sack",
            BagStyle::Sack => "Sack",
        }
        .to_string()
    }

    /// The body as a thick sheet (open at the mouth), then its closures and carrying gear.
    /// Item space: +Y up from the floor the bag stands on, the front faces +Z.
    fn build(s: &BagState) -> ItemMesh {
        let b = &s.bag;
        let mut parts = Parts::new();
        let looks = Looks::of(b, &s.palette);
        match Shape::of(b) {
            Shape::Drawstring(d) => build_drawstring(b, &d, &looks, s.seed, &mut parts),
            Shape::Flapped(f) => build_flapped(b, &f, &looks, &mut parts),
        }
        parts.finish()
    }
}

/// A drawstring bag's depth is a little less than its width.
const DEPTH_SQUASH: f32 = 0.92;

fn compose(rng: &mut Mulberry32, seed: u32, style: BagStyle) -> BagState {
    use BagStyle::*;
    use MaterialKind::{Brass, Gold, Iron, Silver};
    let mut s = BagState { seed, ..BagState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    // The prototype's generic pass: sliders drift up to 15% of their range from base.
    s.bag.nudge(rng, &LIMITS.bag, 0.75, 0.3);
    s.bag.cloth.nudge(rng, &LIMITS.cloth, 0.75, 0.3);
    let b = &mut s.bag;
    b.style = style;
    b.size = rng.range_f32(0.85, 1.15);
    b.fabric = *rng.weighted(match style {
        Sack => &[(Textile::Burlap, 3), (Textile::Cloth, 1)][..],
        Purse => &[(Textile::Cloth, 3), (Textile::Leather, 2)],
        Pouch => &[(Textile::Leather, 3), (Textile::Cloth, 1)],
        BeltPouch | Satchel => &[(Textile::Leather, 5), (Textile::Cloth, 1)],
        Backpack => &[(Textile::Leather, 3), (Textile::Cloth, 2)],
    });
    let hides = [0x6b4a2e, 0x7a5232, 0x5e3d24, 0x4a3a2e, 0x3a2a20];
    let dyes = [0x7a1f24, 0x2c3e6b, 0x4a5a3a, 0x6a5a7a, 0x8a6a3a, 0xcbbf9f];
    b.cloth.color = Rgb::hex(*rng.pick(match b.fabric {
        Textile::Leather => &hides[..],
        Textile::Burlap => &[0xb49a6a, 0xa68c5c],
        Textile::Cloth => &dyes,
    }));
    b.cloth.color2 = Rgb::hex(*rng.pick(&[0x7a8a5a, 0x5e3d24, 0x8a3a2a, 0xcbbf9f, 0x4a2f1c]));
    b.cloth.pattern = match b.fabric {
        Textile::Cloth => *rng.weighted(&[
            (FabricPattern::Plain, 5),
            (FabricPattern::Stripes, 2),
            (FabricPattern::Checks, 2),
            (FabricPattern::Powdered, 2),
            (FabricPattern::Hoops, 1),
            (FabricPattern::Tartan, 1),
            (FabricPattern::Lozenges, 1),
            (FabricPattern::Chevrons, 1),
            (FabricPattern::Quilted, 1),
        ]),
        Textile::Leather => *rng.weighted(&[(FabricPattern::Plain, 9), (FabricPattern::Rivets, 1)]),
        Textile::Burlap => FabricPattern::Plain,
    };
    b.cloth.scale = if b.cloth.pattern == FabricPattern::Powdered { rng.range_f32(4.5, 7.0) } else { rng.range_f32(2.0, 5.0) };
    b.fittings = *rng.weighted(if style == Purse { &[(Brass, 2), (Gold, 2), (Silver, 1)][..] } else { &[(Iron, 3), (Brass, 3), (Gold, 1)] });
    b.coins = matches!(style, Pouch | Purse) && rng.chance(if style == Purse { 0.6 } else { 0.4 });
    b.pocket = style == Backpack && rng.chance(0.6);
    b.bedroll = style == Backpack && rng.chance(0.6);
    b.patch = rng.chance(match (style, b.fabric) {
        (Sack, _) => 0.5,
        (_, Textile::Leather) => 0.1,
        _ => 0.2,
    });

    let targets: &[(EtchTarget, u32)] = match b.fabric {
        Textile::Leather => &[(EtchTarget::Leather, 3), (EtchTarget::Metals, 1)],
        _ => &[(EtchTarget::Cloth, 3), (EtchTarget::Metals, 1)],
    };
    let odds = if b.fabric == Textile::Burlap { 0.0 } else { 0.2 };
    s.etch = Etching::roll(rng, odds, targets, 1);
    s.fit(&LIMITS);
    s
}

/// The bag's materials: the body in its patterned cloth, the flap, patch and bedroll plain in
/// the second colour, straps a darker cut of the same hide (or plain leather on a cloth bag).
struct Looks {
    body: Look,
    flap: Look,
    bedroll: Look,
    /// The bedroll's spiralled ends, a shade darker.
    bedroll_edge: Look,
    strap: Look,
}

impl Looks {
    fn of(b: &Bag, palette: &Palette) -> Looks {
        let m = b.fabric.material();
        let second = Fabric::plain(b.cloth.color2);
        let strap = if b.fabric == Textile::Leather { b.cloth.color.scaled(0.7) } else { palette.leather };
        Looks {
            body: Look::cloth(&b.cloth).made_of(m),
            flap: Look::cloth(&second).made_of(m),
            bedroll: Look::cloth(&second),
            bedroll_edge: Look::cloth(&Fabric::plain(b.cloth.color2.scaled(0.7))),
            strap: Look::of(MaterialKind::Leather).tinted(strap),
        }
    }
}

/// The two body plans: gathered round a neck, or a box with a flap.
enum Shape {
    Drawstring(Drawstring),
    Flapped(Flapped),
}

impl Shape {
    fn of(b: &Bag) -> Shape {
        let (sz, sh) = (b.size, b.shape);
        let girth = 0.85 + sh * 0.3;
        match b.style {
            BagStyle::Pouch | BagStyle::Purse | BagStyle::Sack => {
                let (base, tall) = match b.style {
                    BagStyle::Sack => (2.1, 2.5),
                    BagStyle::Purse => (0.6, 1.35),
                    _ => (0.9, 1.65),
                };
                let belly = base * sz * girth;
                let height = belly * tall;
                let pleats = if b.style == BagStyle::Sack { 9 } else { 12 };
                Shape::Drawstring(Drawstring::new(belly, height, pleats))
            }
            BagStyle::BeltPouch | BagStyle::Satchel => {
                let satchel = b.style == BagStyle::Satchel;
                let (w, h, d) = if satchel { (3.2, 2.5, 1.0) } else { (1.7, 1.6, 0.75) };
                Shape::Flapped(Flapped {
                    width: w * sz,
                    height: h * sz,
                    depth: d * sz * (0.8 + sh * 0.4),
                    bulge: 0.08,
                    squareness: 3.2,
                    flap: h * sz * if satchel { 0.62 } else { 0.7 },
                    size: sz,
                })
            }
            BagStyle::Backpack => {
                let h = 4.4 * sz * girth;
                Shape::Flapped(Flapped { width: 3.1 * sz, height: h, depth: 1.7 * sz, bulge: 0.12, squareness: 3.0, flap: h * 0.42, size: sz })
            }
        }
    }
}

/// A pouch, purse or sack: a turned profile, pleated where the cord gathers it.
struct Drawstring {
    /// Widest radius.
    belly: f32,
    height: f32,
    /// Height of the cord.
    neck: f32,
    pleats: f32,
    /// `[radius, y]`, base (on the axis) to mouth.
    profile: Vec<[f32; 2]>,
}

impl Drawstring {
    fn new(belly: f32, height: f32, pleats: u32) -> Drawstring {
        let (r, h) = (belly, height);
        let neck = h * 0.76;
        let control = [[0.0, 0.0], [r * 0.7, r * 0.06], [r, h * 0.32], [r * 0.92, h * 0.58], [r * 0.42, neck], [r * 0.5, h * 0.88], [r * 0.68, h]];
        let profile = spline_profile(&control, 54);
        Drawstring { belly, height, neck, pleats: pleats as f32, profile }
    }

    /// Profile radius at height `y`.
    fn radius_at(&self, y: f32) -> f32 {
        let p = &self.profile;
        for w in p.windows(2) {
            if y >= w[0][1] && y <= w[1][1] {
                let t = (y - w[0][1]) / (w[1][1] - w[0][1]).max(1e-6);
                return w[0][0] + (w[1][0] - w[0][0]) * t;
            }
        }
        if y < p[0][1] { p[0][0] } else { p[p.len() - 1][0] }
    }

    /// How gathered the cloth is at `y`: nothing low on the belly, most at the cord, easing
    /// toward the frilled mouth.
    fn gather(&self, y: f32) -> f32 {
        let h = self.height;
        smoothstep(h * 0.45, self.neck, y) * (1.0 - 0.4 * smoothstep(self.neck, h, y))
    }

    /// Radial pleat factor at angle `a` and height `y`. The cord pulls the pleats tight, so
    /// they are shallow right under it.
    fn pleat(&self, a: f32, y: f32) -> f32 {
        let under_cord = 1.0 - 0.75 * (-((y - self.neck) / (self.height * 0.05)).powi(2)).exp();
        let g = self.gather(y) * under_cord;
        1.0 + 0.11 * g * (a * self.pleats).sin() + 0.05 * g * (a * 5.0 + 2.0).sin()
    }

    /// A point of the outer surface: `a` round from the front (+Z), at profile radius `r`.
    fn point(&self, a: f32, r: f32, y: f32) -> V3 {
        Self::turned(a, r * self.pleat(a, y), y)
    }

    /// A point of the turned profile at radius `r`, before pleating.
    fn turned(a: f32, r: f32, y: f32) -> V3 {
        [a.sin() * r, y, a.cos() * r * DEPTH_SQUASH]
    }

    /// The surface as rows (profile points) of `around` columns, front faces outward.
    fn rows(&self, around: usize) -> Vec<Vec<V3>> {
        self.profile.iter().map(|&[r, y]| (0..around).map(|c| self.point(TAU * c as f32 / around as f32, r, y)).collect()).collect()
    }

    /// [`Drawstring::rows`] without the pleats, point for point.
    fn unpleated_rows(&self, around: usize) -> Vec<Vec<V3>> {
        self.profile.iter().map(|&[r, y]| (0..around).map(|c| Self::turned(TAU * c as f32 / around as f32, r, y)).collect()).collect()
    }

    /// The outer surface at (`a`, `y`) and its outward normal.
    fn surface(&self, a: f32, y: f32) -> (V3, V3) {
        let at = |a: f32, y: f32| self.point(a, self.radius_at(y), y);
        let (da, dy) = (1e-3, 1e-3 * self.height);
        let p = at(a, y);
        let n = vec3::cross(vec3::sub(at(a + da, y), at(a - da, y)), vec3::sub(at(a, y + dy), at(a, y - dy)));
        (p, vec3::normalize_or(n, [a.sin(), 0.0, a.cos()]))
    }
}

fn build_drawstring(b: &Bag, d: &Drawstring, looks: &Looks, seed: u32, parts: &mut Parts) {
    let sz = b.size;
    let sack = b.style == BagStyle::Sack;
    let t = b.fabric.thickness() * sz.max(0.8);
    let options = GridOptions { wrap_around: true, wrap_along: false, shading: Shading::SMOOTH, row_v: None };
    // The pleats stay in the silhouette but shade with the turned profile's normals: under
    // banded toon light, folds this fine break the neck into ragged patches wherever a band
    // edge crosses it.
    let mut body = thick_sheet(&d.rows(72), &options, t);
    body.normals = thick_sheet(&d.unpleated_rows(72), &options, t).normals;
    parts.add_look("body", body, looks.body);

    // The cord round the neck, sitting on the pleats it gathers.
    let cord_r = 0.035 * sz * if sack { 1.8 } else { 1.0 };
    let ny = d.neck;
    let rn = d.radius_at(ny);
    let ring: Vec<V3> = (0..64)
        .map(|i| {
            let a = TAU * i as f32 / 64.0;
            let r = rn * d.pleat(a, ny) + cord_r * 0.9;
            [a.sin() * r, ny, a.cos() * (rn * d.pleat(a, ny) * DEPTH_SQUASH + cord_r * 0.9)]
        })
        .collect();
    let mut cord = tube(&ring, 8, &SweepOptions { up: Some(vec3::Y), ..SweepOptions::closed_loop() }, |_| cord_r);

    // Two tails from a knot at the front, draped down over the belly, each ending in a toggle.
    let mut toggles = MeshBuilder::new();
    let knot_at = [0.0, ny, rn * d.pleat(0.0, ny) * DEPTH_SQUASH + cord_r * 1.6];
    let mut knot = uv_sphere(cord_r * 1.7, 12, 8);
    knot.translate(knot_at);
    cord.append(&knot);
    let tail = d.height * if sack { 0.28 } else { 0.4 };
    for side in [-1.0f32, 1.0] {
        let path: Vec<V3> = (0..=10)
            .map(|i| {
                let k = i as f32 / 10.0;
                let y = ny - tail * k;
                let a = side * (0.06 + 0.22 * k);
                let (p, n) = d.surface(a, y);
                vec3::add(p, vec3::scale(n, cord_r * (1.0 + 0.8 * (1.0 - k))))
            })
            .collect();
        let path = CatmullRom::centripetal(path).sample(4);
        cord.append(&tube(&path, 8, &SweepOptions::default(), |_| cord_r * 0.75));
        let end = path[path.len() - 1];
        let (_, n) = d.surface(side * 0.28, ny - tail);
        let bead_r = 0.055 * sz * if sack { 1.6 } else { 1.0 };
        let mut bead = lathe(
            &spline_profile(&[[0.0, -bead_r], [bead_r * 0.8, -bead_r * 0.6], [bead_r, 0.0], [bead_r * 0.8, bead_r * 0.6], [0.0, bead_r]], 12),
            &LatheOptions::new(12, Shading::SMOOTH),
        );
        bead.translate(vec3::add(end, vec3::scale(n, bead_r * 0.6)));
        toggles.append(&bead);
    }
    parts.add("cord", cord, MaterialKind::Cord);
    parts.add("fittings", toggles, b.fittings);

    if b.patch {
        // A square patch on the front right of the belly, following its curve.
        let (y0, y1) = (d.height * 0.16, d.height * 0.42);
        let half = (y1 - y0) * 0.5 / d.belly;
        let rows: Vec<Vec<V3>> = (0..=8)
            .map(|i| {
                let y = y0 + (y1 - y0) * i as f32 / 8.0;
                (0..=8).map(|j| 0.45 + half * (j as f32 / 4.0 - 1.0)).map(|a| d.surface(a, y)).map(|(p, n)| vec3::add(p, vec3::scale(n, t * 0.8))).collect()
            })
            .collect();
        add_patch(parts, rows, t * 0.8, looks.flap);
    }

    if b.coins {
        let mut rng = Mulberry32::new(seed.wrapping_mul(191).wrapping_add(3));
        let n = if b.style == BagStyle::Purse { 9 } else { 6 };
        let r = d.belly;
        parts.add("coins", coin_scatter(&mut rng, [r * 0.55, 0.0, r * 1.05], [r * 1.4, r * 0.8], n, 0.09 * sz), MaterialKind::Gold);
    }
}

/// A sewn patch of `rows` (columns run across it, which way it faces), as a thin sheet.
fn add_patch(parts: &mut Parts, rows: Vec<Vec<V3>>, thickness: f32, look: Look) {
    let options = GridOptions { wrap_around: false, wrap_along: false, shading: Shading::SMOOTH, row_v: None };
    // Rows run up, columns round: du x dv faces outward when the columns run the right way.
    let rows: Vec<Vec<V3>> = rows.into_iter().map(|r| r.into_iter().rev().collect()).collect();
    let mut sheet = thick_sheet(&rows, &options, thickness);
    if !faces_out(&rows) {
        sheet.flip();
    }
    parts.add_look("patch", sheet, look);
}

/// Whether a grid's front (du x dv) faces away from the vertical axis at its centre.
fn faces_out(rows: &[Vec<V3>]) -> bool {
    let (r, c) = (rows.len() / 2, rows[0].len() / 2);
    let n = grid_normals(rows, false, false)[r][c];
    let p = rows[r][c];
    vec3::dot(n, [p[0], 0.0, p[2]]) > 0.0
}

/// A belt pouch, satchel or backpack: a rounded box open at the top with a flap over it.
struct Flapped {
    width: f32,
    height: f32,
    depth: f32,
    /// How far the sides swell at mid height.
    bulge: f32,
    /// Superellipse exponent of the plan: higher is boxier.
    squareness: f32,
    /// How far the flap hangs down the front.
    flap: f32,
    size: f32,
}

/// A rounded box body (the prototype's `bagSurface`): superellipse plan, sides swelling at mid
/// height, a rounded base closing to a point.
#[derive(Clone, Copy)]
struct BoxBody {
    width: f32,
    height: f32,
    depth: f32,
    bulge: f32,
    squareness: f32,
    origin: V3,
}

/// `sign(c) * |c|^e`.
fn signed_pow(c: f32, e: f32) -> f32 {
    c.signum() * c.abs().powf(e)
}

impl BoxBody {
    /// Swell at height fraction `v`: a quarter circle closing the base, the bulge above.
    fn swell(&self, v: f32) -> f32 {
        let base = (1.0 - (1.0 - (v / 0.14).min(1.0)).powi(2)).max(0.0).sqrt();
        (1.0 + self.bulge * (PI * v.clamp(0.0, 1.0)).sin().powf(0.8)) * base
    }

    /// The point at plan angle `a` (0 at +X, PI/2 at the front, +Z) and height fraction `v`.
    fn point(&self, a: f32, v: f32) -> V3 {
        let k = self.swell(v);
        let e = 2.0 / self.squareness;
        vec3::add(self.origin, [self.width * 0.5 * k * signed_pow(a.cos(), e), self.height * v, self.depth * 0.5 * k * signed_pow(a.sin(), e)])
    }

    /// Height fractions of the rows: dense round the rounded base, even above it.
    fn row_heights(base_rows: usize, wall_rows: usize) -> Vec<f32> {
        let base = (0..base_rows).map(|i| 0.14 * (1.0 - (PI * 0.5 * i as f32 / base_rows as f32).cos()));
        let wall = (0..=wall_rows).map(|i| 0.14 + 0.86 * i as f32 / wall_rows as f32);
        base.chain(wall).collect()
    }

    /// The surface as rows, front faces outward. Columns run clockwise seen from above, which
    /// with rows running up makes `du x dv` point out.
    fn rows(&self, base_rows: usize, wall_rows: usize) -> Vec<Vec<V3>> {
        let around = 72;
        BoxBody::row_heights(base_rows, wall_rows)
            .into_iter()
            .map(|v| (0..around).map(|c| self.point(-TAU * c as f32 / around as f32, v)).collect())
            .collect()
    }

    /// The plan angle whose point lies at `x` on the front (+Z) or back (-Z) face.
    fn angle_at(&self, x: f32, v: f32, front: bool) -> f32 {
        let k = self.swell(v).max(1e-4);
        let c = signed_pow((x / (self.width * 0.5 * k)).clamp(-1.0, 1.0), self.squareness / 2.0).clamp(-1.0, 1.0);
        let a = c.acos();
        if front { a } else { -a }
    }

    /// The surface at `x` on the front or back face at height fraction `v`, and its normal.
    fn face_point(&self, x: f32, v: f32, front: bool) -> (V3, V3) {
        let at = |x: f32, v: f32| self.point(self.angle_at(x - self.origin[0], v, front), v);
        let (dx, dv) = (1e-3 * self.width, 1e-3);
        let n = vec3::cross(vec3::sub(at(x + dx, v), at(x - dx, v)), vec3::sub(at(x, v + dv), at(x, v - dv)));
        let n = if front { n } else { vec3::neg(n) };
        (at(x, v), vec3::normalize_or(n, if front { vec3::Z } else { vec3::neg(vec3::Z) }))
    }

    /// The surface on a side (+X or -X) at height fraction `v`, and its normal.
    fn side_point(&self, right: bool, v: f32) -> (V3, V3) {
        let a = if right { 0.0 } else { PI };
        let p = self.point(a, v);
        (p, if right { vec3::X } else { vec3::neg(vec3::X) })
    }
}

impl Flapped {
    fn body(&self) -> BoxBody {
        BoxBody { width: self.width, height: self.height, depth: self.depth, bulge: self.bulge, squareness: self.squareness, origin: [0.0; 3] }
    }
}

/// A flap hung from the back of `body`, over its mouth and `length` down its front (the
/// prototype's `bagFlap`), with a rounded tab at the end.
#[derive(Clone)]
struct Flap {
    path: Vec<V3>,
    width: f32,
    bulge: f32,
    /// How far it stands over the mouth.
    lift: f32,
}

impl Flap {
    /// A flap `thickness` thick, its inside clear of the body and the body's rim.
    fn new(body: &BoxBody, length: f32, size: f32, thickness: f32) -> Flap {
        let (h, d, bl, o) = (body.height, body.depth, body.bulge, body.origin);
        let (lift, t) = (0.1 * size + thickness, thickness * 1.5);
        let control = vec![
            // Sewn to the outside of the back, then over the back edge of the mouth.
            [0.0, h * 0.86, -d * 0.5 * (1.0 + bl * 0.4) - t],
            [0.0, h - lift * 0.5, -d * 0.5 * (1.0 + bl * 0.3) - 0.02 * size - t],
            [0.0, h + lift * 0.6, -d * 0.5 * (1.0 + bl * 0.2) * 0.92 - t],
            [0.0, h + lift, -d * 0.2],
            [0.0, h + lift, d * 0.18],
            // Rolled over the front edge of the mouth, clear of it.
            [0.0, h + lift * 0.6, d * 0.5 * (1.0 + bl * 0.2) * 0.92 + t],
            [0.0, h - lift * 0.5, d * 0.5 * (1.0 + bl * 0.3) + 0.03 * size + t],
            [0.0, h - length * 0.5, d * 0.5 * (1.0 + bl * 0.55) + 0.04 * size + t],
            [0.0, h - length, d * 0.5 * (1.0 + bl * 0.7) + 0.05 * size + t],
        ];
        let path = CatmullRom::centripetal(control.into_iter().map(|p| vec3::add(p, o)).collect()).sample_evenly(41);
        Flap { path, width: body.width, bulge: bl, lift }
    }

    /// Half the flap's width at `v` along it, narrowing into the rounded tab.
    fn half_width(&self, v: f32) -> f32 {
        let tab = if v > 0.82 { (1.0 - ((v - 0.82) / 0.18).powi(2)).max(0.0).sqrt() * 0.55 + 0.45 } else { 1.0 };
        self.width * 0.5 * 1.05 * (1.0 + self.bulge * 0.4 * (PI * (0.55 + v * 0.45).min(1.0)).sin()) * tab
    }

    /// Where along the flap it turns down the front: the first point past the mouth's front
    /// edge that has dropped below the top of the bag.
    fn front_start(&self) -> f32 {
        let top = self.path.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        let (back, front) = self.path.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[2]), hi.max(p[2])));
        let n = self.path.len() - 1;
        (0..=n).find(|&i| self.path[i][2] > (back + front) * 0.5 && self.path[i][1] < top - self.lift).map_or(0.5, |i| i as f32 / n as f32)
    }

    fn centre(&self, v: f32) -> V3 {
        let f = v.clamp(0.0, 1.0) * (self.path.len() - 1) as f32;
        let i = (f.floor() as usize).min(self.path.len() - 2);
        vec3::lerp(self.path[i], self.path[i + 1], f - i as f32)
    }

    /// The point `x` across the flap (from its centre line) at `v` along it.
    fn point(&self, x: f32, v: f32) -> V3 {
        vec3::add(self.centre(v), [x, 0.0, 0.0])
    }

    /// The outside of the flap at `x`, `v`, and its normal.
    fn surface(&self, x: f32, v: f32) -> (V3, V3) {
        let dv = 1e-3;
        let along = vec3::sub(self.centre(v + dv), self.centre(v - dv));
        // Across is -X as the grid runs, so the outside is along (-X) x along.
        (self.point(x, v), vec3::normalize_or(vec3::cross(vec3::neg(vec3::X), along), vec3::Z))
    }

    fn rows(&self) -> Vec<Vec<V3>> {
        let (across, along) = (24, self.path.len() - 1);
        (0..=along)
            .map(|i| {
                let v = i as f32 / along as f32;
                let w = self.half_width(v);
                (0..=across).map(|j| self.point(w * (1.0 - 2.0 * j as f32 / across as f32), v)).collect()
            })
            .collect()
    }

    fn mesh(&self, thickness: f32) -> MeshBuilder {
        let options = GridOptions { wrap_around: false, wrap_along: false, shading: Shading::SMOOTH, row_v: None };
        thick_sheet(&self.rows(), &options, thickness)
    }
}

/// A strap down the flap ending in a buckle at its tip, and a strap from below the buckle
/// down the body: how flapped bags close. Returns (straps, buckles).
fn flap_closure(body: &BoxBody, flap: &Flap, x: f32, width: f32, thick: f32, flap_t: f32) -> (MeshBuilder, MeshBuilder) {
    let tip_v = if x.abs() + width * 0.5 > flap.half_width(0.93) { 0.88 } else { 0.95 };
    let v0 = flap.front_start();
    let mut straps = strap_along(16, width, thick, |t| flap.surface(x, v0 + (tip_v - v0) * t));
    let (tip, n) = flap.surface(x, tip_v);
    let along = vec3::sub(flap.centre(tip_v - 0.01), flap.centre(tip_v));
    let wire = width * 0.12;
    let mut b = buckle(width, wire);
    b.place_basis(vec3::add(tip, vec3::scale(n, thick)), along, n);
    // The body strap rises from low on the front into the buckle, tucked under the flap tip.
    let v_tip = (tip[1] - body.origin[1]) / body.height;
    let v_low = (v_tip - 0.3).max(0.2);
    let ox = body.origin[0];
    straps.append(&strap_along(16, width * 0.9, thick, |t| {
        let (p, n) = body.face_point(ox + x, v_low + (v_tip + 0.02 - v_low) * t, true);
        (vec3::add(p, vec3::scale(n, flap_t * 0.2)), n)
    }));
    (straps, b)
}

fn build_flapped(b: &Bag, f: &Flapped, looks: &Looks, parts: &mut Parts) {
    let sz = f.size;
    let t = b.fabric.thickness() * sz.max(0.8);
    let body = f.body();
    let options = GridOptions { wrap_around: true, wrap_along: false, shading: Shading::SMOOTH, row_v: None };
    parts.add_look("body", thick_sheet(&body.rows(10, 24), &options, t), looks.body);
    let flap = Flap::new(&body, f.flap, sz, t);
    let mut flaps = flap.mesh(t);

    let mut straps = MeshBuilder::new();
    let mut fittings = MeshBuilder::new();
    let strap_t = 0.03 * sz;
    let (closures, strap_w): (&[f32], f32) = match b.style {
        BagStyle::Satchel => (&[-0.24, 0.24], 0.16 * sz),
        BagStyle::Backpack => (&[-0.22, 0.22], 0.18 * sz),
        _ => (&[0.0], 0.16 * sz),
    };
    for &k in closures {
        let (s, bk) = flap_closure(&body, &flap, k * f.width, strap_w, strap_t, t);
        straps.append(&s);
        fittings.append(&bk);
    }

    match b.style {
        BagStyle::Satchel => {
            // D-rings on stitched tabs at each side, and the shoulder strap arched between them.
            let ring_w = 0.16 * sz;
            let wire = 0.022 * sz;
            let mut ends = Vec::new();
            for right in [false, true] {
                let s = if right { 1.0 } else { -1.0 };
                straps.append(&strap_along(8, ring_w * 0.8, strap_t, |u| body.side_point(right, 0.58 + 0.16 * u)));
                let (p, n) = body.side_point(right, 0.74);
                let origin = vec3::add(p, vec3::scale(n, strap_t + wire));
                let mut ring = d_ring(ring_w, wire);
                ring.place_basis(origin, vec3::Y, n);
                fittings.append(&ring);
                ends.push(vec3::add(origin, [s * wire * 0.5, ring_w * 0.65 + wire, 0.0]));
            }
            let h = f.height;
            let control = vec![
                ends[0],
                [-f.width * 0.58, h * 1.9, 0.0],
                [0.0, h * 2.5, 0.0],
                [f.width * 0.58, h * 1.9, 0.0],
                ends[1],
            ];
            let path = CatmullRom::centripetal(control).sample_evenly(64);
            let centre = [0.0, h * 1.2, 0.0];
            let faces: Vec<V3> = path.iter().map(|&p| vec3::sub(p, centre)).collect();
            straps.append(&strap(&path, &faces, 0.16 * sz, strap_t, false));
        }
        BagStyle::BeltPouch => {
            // Two loops on the back for the belt to pass through.
            for k in [-0.25f32, 0.25] {
                let x = k * f.width;
                straps.append(&strap_along(16, 0.2 * sz, strap_t, |u| {
                    let (p, n) = body.face_point(x, 0.32 + 0.6 * u, false);
                    (vec3::add(p, vec3::scale(n, 0.1 * sz * (PI * u).sin().max(0.0).sqrt())), n)
                }));
            }
        }
        BagStyle::Backpack => {
            let (w, h, d) = (f.width, f.height, f.depth);
            for side in [-1.0f32, 1.0] {
                let x = side * w * 0.22;
                let control = vec![
                    [x * 1.1, h * 0.92, -d * 0.45],
                    [x * 1.3, h * 0.75, -d * 0.5 - 0.7 * sz],
                    [x * 1.35, h * 0.35, -d * 0.5 - 0.75 * sz],
                    [x * 1.2, h * 0.08, -d * 0.4],
                ];
                let path = CatmullRom::centripetal(control).sample_evenly(40);
                let faces = vec![vec3::neg(vec3::Z); path.len()];
                straps.append(&strap(&path, &faces, 0.24 * sz, 0.05 * sz, false));
            }
            // A carrying loop stitched to the flap where it crests the back.
            let n = flap.path.len() - 1;
            let crest = (0..=n).find(|&i| flap.path[i][1] >= h + flap.lift * 0.5).map_or(0.2, |i| i as f32 / n as f32);
            let (loop_path, faces): (Vec<V3>, Vec<V3>) = (0..=12)
                .map(|i| {
                    let a = PI * i as f32 / 12.0;
                    let (p, nrm) = flap.surface(-a.cos() * w * 0.1, crest);
                    (vec3::add(p, vec3::scale(nrm, a.sin() * h * 0.06)), nrm)
                })
                .unzip();
            straps.append(&strap(&loop_path, &faces, 0.12 * sz, strap_t, false));
            if b.pocket {
                let pocket_body = BoxBody { width: w * 0.62, height: h * 0.3, depth: d * 0.35, bulge: 0.1, squareness: 3.0, origin: [0.0, h * 0.06, d * 0.5 * 0.95] };
                parts.add_look("pocket", thick_sheet(&pocket_body.rows(8, 12), &options, t), looks.body);
                let pocket_flap = Flap::new(&pocket_body, pocket_body.height * 0.5, sz * 0.7, t);
                flaps.append(&pocket_flap.mesh(t));
                let (s, bk) = flap_closure(&pocket_body, &pocket_flap, 0.0, 0.12 * sz, strap_t, t);
                straps.append(&s);
                fittings.append(&bk);
            }
            if b.bedroll {
                add_bedroll(f, &body, looks, strap_t, &mut straps, &mut fittings, parts);
            }
        }
        _ => {}
    }

    parts.add_look("flap", flaps, looks.flap);
    parts.add_look("straps", straps, looks.strap);
    parts.add("fittings", fittings, b.fittings);

    if b.patch {
        // A patch on the front below the flap (a backpack's beside its pocket).
        let (x, v0, v1) = match b.style {
            BagStyle::Backpack if b.pocket => (0.0, 0.38, 0.52),
            BagStyle::Backpack => (f.width * 0.08, 0.15, 0.4),
            _ => (f.width * 0.2, 0.12, 0.3),
        };
        let half = f.height * (v1 - v0) * 0.5;
        let rows: Vec<Vec<V3>> = (0..=6)
            .map(|i| {
                let v = v0 + (v1 - v0) * i as f32 / 6.0;
                (0..=6).map(|j| body.face_point(x + half * (j as f32 / 3.0 - 1.0), v, true)).map(|(p, n)| vec3::add(p, vec3::scale(n, t * 0.8))).collect()
            })
            .collect();
        add_patch(parts, rows, t * 0.8, looks.flap);
    }
}

/// A blanket rolled and strapped across the top of a pack, its spiral showing at each end.
fn add_bedroll(f: &Flapped, body: &BoxBody, looks: &Looks, strap_t: f32, straps: &mut MeshBuilder, fittings: &mut MeshBuilder, parts: &mut Parts) {
    let sz = f.size;
    let r = 0.5 * sz;
    let len = f.width * 1.06;
    let centre = [0.0, body.height + 0.1 * sz + r * 0.9, 0.0];
    let (roll, spirals) = rolled_sheet(centre, r, len, 0.018 * sz, 3.0);
    parts.add_look("bedroll", roll, looks.bedroll);
    parts.add_look("bedroll_edge", spirals, looks.bedroll_edge);

    // Two straps round the roll, buckled at the front.
    let (s, bk) = roll_straps(centre, r, &[-0.28 * f.width, 0.28 * f.width], 0.12 * sz, strap_t, 0.014 * sz);
    straps.append(&s);
    fittings.append(&bk);
}
