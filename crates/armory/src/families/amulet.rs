//! Amulets: a pendant on a bail, hung from a cable chain, a twisted cord, a leather thong
//! or a string of beads.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (the
//! `pendant`, `chain` and `stone` entries of `SECTIONS`, `PRESETS.amulet`, the
//! `fam==='amulet'` branch of `randomize()`, the amulet rules in `proportion()`,
//! `buildAmulet()` and `forgeNoun()`).
//!
//! Departures from the prototype:
//! - The roll composes a weighted [`AmuletArchetype`] (medallion, jewel, devotional,
//!   lunula, heraldic, talisman) instead of picking shape, centre and chain independently,
//!   so a lunula hangs drops from a thong and a heraldic pendant bears a device. Talismans
//!   add carved bone, wood and stone pendants the prototype never rolled.
//! - The prototype's `chain` section is [`Necklace`] here, since it is as often a cord, a
//!   thong or beads as a chain. Its `frame` select only moved the camera and is dropped.
//! - Cable links interlock at their true pitch, and the pendant is a [`bevelled_slab`] with
//!   a rounded, cast-looking rim and smooth curved walls.
//!
//! Item space: the bail sits at the origin, the pendant hangs below it facing +Z, and the
//! necklace rises above it (+Y) and loops back (-Z) behind the neck.

use std::f32::consts::{FRAC_PI_2, PI};

use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use crate::build::chain::{bead, cable_chain, cord, jump_ring, necklace_path, stations, thong, twisted_cord};
use crate::build::device::device_mesh;
use crate::build::emblem::{plaque_centre, plaque_shape};
use crate::build::gem::{collet, collet_seat, cut_for, cut_stone};
use crate::build::parts::{Look, Parts};
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::gem::{GemCut, Gemstone};
use crate::geom::polygon::P2;
use crate::geom::{ItemMesh, MeshBuilder, SlabOptions, V3, bevelled_slab, torus, value_noise3, vec3};
use crate::geom::grid::Shading;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::device::Device;
use crate::sections::emblem::PlaqueShape;
use crate::sections::jewel::{SetStone, SetStoneLimits};
use crate::slider::{Sliders, sliders};

use MaterialKind::{Bone, Brass, Cord, Gold, Iron, Leather, Silver, Stone, Wood};

/// The amulet generator.
pub struct Amulet;

/// What fills the middle of the pendant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendantCentre {
    /// A stone in a collet.
    Stone,
    /// A device in relief.
    Device,
    Plain,
}

impl PendantCentre {
    pub const ALL: [PendantCentre; 3] = [PendantCentre::Stone, PendantCentre::Device, PendantCentre::Plain];
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pendant {
    pub shape: PlaqueShape,
    /// Height.
    pub size: f32,
    pub thickness: f32,
    /// Width of the raised rim, as a fraction of the size.
    pub rim: f32,
    pub centre: PendantCentre,
    pub device: Device,
    /// Stones hanging from the lower edge.
    pub drops: u32,
    pub material: MaterialKind,
}

sliders! {
    PendantLimits for Pendant {
        size: 0.15, 0.8, 0.005;
        thickness: 0.01, 0.08, 0.001;
        rim: 0.0, 0.2, 0.005;
        drops: 0.0, 5.0, 1.0;
    }
}

impl Default for Pendant {
    /// The prototype's `BASE.pendant`.
    fn default() -> Self {
        Pendant { shape: PlaqueShape::Disc, size: 0.42, thickness: 0.035, rim: 0.1, centre: PendantCentre::Device, device: Device::Star, drops: 0, material: Gold }
    }
}

/// What the pendant hangs from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NecklaceStyle {
    /// Interlocking oval links.
    Cable,
    /// Twisted cord.
    Cord,
    /// A leather thong.
    Thong,
    /// Beads on a string.
    Beads,
}

impl NecklaceStyle {
    pub const ALL: [NecklaceStyle; 4] = [NecklaceStyle::Cable, NecklaceStyle::Cord, NecklaceStyle::Thong, NecklaceStyle::Beads];
}

/// The prototype's `chain` section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Necklace {
    pub style: NecklaceStyle,
    /// How far the loop rises above the bail.
    pub drop: f32,
    /// Length of a link, or a bead's diameter.
    pub link: f32,
    /// Wire, cord or thong radius.
    pub thickness: f32,
    pub material: MaterialKind,
}

sliders! {
    NecklaceLimits for Necklace {
        drop: 1.0, 4.5, 0.05;
        link: 0.02, 0.12, 0.001;
        thickness: 0.004, 0.025, 0.0005;
    }
}

impl Default for Necklace {
    /// The prototype's `BASE.chain`.
    fn default() -> Self {
        Necklace { style: NecklaceStyle::Cable, drop: 1.5, link: 0.05, thickness: 0.008, material: Gold }
    }
}

/// Every parameter of one amulet.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AmuletState {
    pub seed: u32,
    pub pendant: Pendant,
    pub necklace: Necklace,
    pub stone: SetStone,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every amulet section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AmuletLimits {
    pub pendant: PendantLimits,
    pub necklace: NecklaceLimits,
    pub stone: SetStoneLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges (it has no `RANGES.amulet`).
pub const LIMITS: AmuletLimits = AmuletLimits {
    pendant: PendantLimits::SLIDERS,
    necklace: NecklaceLimits::SLIDERS,
    stone: SetStoneLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl AmuletState {
    /// Clamp and snap every slider into `limits`, then keep the parts in proportion (the
    /// prototype's `proportion()`): the stone at most half the pendant, links at least
    /// four wire radii long.
    pub fn fit(&mut self, limits: &AmuletLimits) {
        self.pendant.fit(&limits.pendant);
        self.necklace.fit(&limits.necklace);
        self.stone.fit(&limits.stone);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
        self.stone.size = self.stone.size.min(self.pendant.size * 0.5);
        self.necklace.link = self.necklace.link.max(self.necklace.thickness * 4.0).min(limits.necklace.link.max);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &AmuletLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("pendant", self.pendant.out_of_range(&limits.pendant)),
            tag("necklace", self.necklace.out_of_range(&limits.necklace)),
            tag("stone", self.stone.out_of_range(&limits.stone)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

/// The kinds of amulet "Forge new" composes. Not stored: the noun reads the state itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmuletArchetype {
    /// A round, oval or octagonal medal bearing a device.
    Medallion,
    /// A shaped setting for one large stone.
    Jewel,
    /// A cross, an ankh or a star.
    Devotional,
    /// A crescent, often hung with drops.
    Lunula,
    /// A shield bearing a device.
    Heraldic,
    /// A carved bone, wood or stone pendant on a thong or cord.
    Talisman,
}

impl AmuletArchetype {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(AmuletArchetype, u32); 6] = [
        (AmuletArchetype::Medallion, 22),
        (AmuletArchetype::Jewel, 22),
        (AmuletArchetype::Devotional, 16),
        (AmuletArchetype::Lunula, 12),
        (AmuletArchetype::Heraldic, 14),
        (AmuletArchetype::Talisman, 14),
    ];
}

impl Generator for Amulet {
    type State = AmuletState;

    fn roll(seed: u32) -> AmuletState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let archetype = *rng.weighted(&AmuletArchetype::WEIGHTS);
        compose(&mut rng, seed, archetype)
    }

    /// `forgeNoun()` for amulets.
    fn noun(s: &AmuletState) -> String {
        let p = &s.pendant;
        let gem = s.stone.stone.name();
        let stone = p.centre == PendantCentre::Stone;
        match p.shape {
            PlaqueShape::Disc | PlaqueShape::Octagon => "Medallion".to_string(),
            PlaqueShape::Oval if stone => format!("{gem} Amulet"),
            PlaqueShape::Oval => "Oval Medallion".to_string(),
            PlaqueShape::Shield => "Shield Pendant".to_string(),
            PlaqueShape::Teardrop if stone => format!("{gem} Drop"),
            PlaqueShape::Teardrop => "Teardrop Pendant".to_string(),
            PlaqueShape::Quatrefoil => "Quatrefoil Pendant".to_string(),
            PlaqueShape::Star => "Star Amulet".to_string(),
            PlaqueShape::Cross => "Pendant Cross".to_string(),
            PlaqueShape::Crescent => "Lunula".to_string(),
            PlaqueShape::Lozenge => "Lozenge Amulet".to_string(),
            PlaqueShape::Ankh if stone => "Jewelled Ankh".to_string(),
            PlaqueShape::Ankh => "Ankh".to_string(),
        }
    }

    fn build(s: &AmuletState) -> ItemMesh {
        build_amulet(s)
    }
}

impl Amulet {
    /// "Forge new" forced to one archetype, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, a)` when `a` is what `s` rolls.
    pub fn roll_as(seed: u32, archetype: AmuletArchetype) -> AmuletState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, archetype)
    }
}

fn is_metal(m: MaterialKind) -> bool {
    matches!(m, Iron | Brass | Gold | Silver)
}

fn compose(rng: &mut Mulberry32, seed: u32, a: AmuletArchetype) -> AmuletState {
    use AmuletArchetype as A;
    use NecklaceStyle as N;
    use PendantCentre as C;
    use PlaqueShape as S;
    let mut s = AmuletState { seed, ..AmuletState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);

    let p = &mut s.pendant;
    p.material = *rng.weighted(match a {
        A::Talisman => &[(Bone, 3), (Wood, 2), (Stone, 2)][..],
        A::Heraldic => &[(Brass, 3), (Silver, 2), (Gold, 2)],
        _ => &[(Gold, 3), (Silver, 3), (Brass, 2)],
    });
    p.shape = *rng.weighted(match a {
        A::Medallion => &[(S::Disc, 4), (S::Octagon, 2), (S::Oval, 2)][..],
        A::Jewel => &[(S::Teardrop, 3), (S::Oval, 3), (S::Quatrefoil, 2), (S::Lozenge, 2)],
        A::Devotional => &[(S::Cross, 3), (S::Ankh, 2), (S::Star, 2)],
        A::Lunula => &[(S::Crescent, 1)],
        A::Heraldic => &[(S::Shield, 1)],
        A::Talisman => &[(S::Disc, 2), (S::Lozenge, 2), (S::Teardrop, 1), (S::Octagon, 1)],
    });
    p.centre = match a {
        A::Medallion => *rng.weighted(&[(C::Device, 3), (C::Stone, 1)]),
        A::Jewel => C::Stone,
        A::Devotional => match p.shape {
            S::Star => *rng.weighted(&[(C::Device, 1), (C::Stone, 1), (C::Plain, 1)]),
            _ => *rng.weighted(&[(C::Stone, 2), (C::Plain, 1)]),
        },
        A::Lunula => *rng.weighted(&[(C::Plain, 7), (C::Stone, 3)]),
        A::Heraldic | A::Talisman => C::Device,
    };
    p.device = *rng.pick(&Device::SIGNS);
    p.size = match a {
        A::Talisman => rng.range_f32(0.3, 0.45),
        A::Devotional => rng.range_f32(0.38, 0.6),
        _ => rng.range_f32(0.32, 0.55),
    };
    p.thickness = if is_metal(p.material) { rng.range_f32(0.025, 0.05) } else { rng.range_f32(0.04, 0.065) };
    p.rim = match (a, p.shape) {
        (_, S::Crescent | S::Ankh | S::Cross) => 0.0,
        (A::Jewel, _) => rng.range_f32(0.08, 0.16),
        (A::Medallion | A::Heraldic, _) if rng.chance(0.8) => rng.range_f32(0.06, 0.14),
        (A::Talisman, _) if rng.chance(0.3) => rng.range_f32(0.05, 0.1),
        (A::Devotional, _) if rng.chance(0.4) => rng.range_f32(0.06, 0.1),
        _ => 0.0,
    };
    p.drops = match a {
        A::Lunula if rng.chance(0.7) => rng.int(1, 3),
        A::Jewel if rng.chance(0.25) => 1,
        A::Medallion if rng.chance(0.1) => 3,
        _ => 0,
    };
    let pendant = *p;

    let cut = *rng.weighted(&[(GemCut::Cabochon, 3), (GemCut::Brilliant, 1), (GemCut::Rose, 1)]);
    let stones: Vec<Gemstone> = Gemstone::ALL.into_iter().filter(|g| g.suits(cut)).collect();
    s.stone.stone = *rng.pick(&stones);
    s.stone.cut = cut_for(s.stone.stone, cut);
    let (_, room) = plaque_centre(pendant.shape);
    s.stone.size = pendant.size * room * rng.range_f32(0.5, 0.8);

    let n = &mut s.necklace;
    n.style = *rng.weighted(match a {
        A::Medallion => &[(N::Cable, 3), (N::Cord, 1), (N::Beads, 1)][..],
        A::Jewel => &[(N::Cable, 4), (N::Beads, 1)],
        A::Devotional => &[(N::Cord, 2), (N::Thong, 2), (N::Cable, 2), (N::Beads, 1)],
        A::Lunula => &[(N::Thong, 3), (N::Cord, 2), (N::Cable, 1)],
        A::Heraldic => &[(N::Cable, 2), (N::Beads, 2), (N::Cord, 1)],
        A::Talisman => &[(N::Thong, 3), (N::Cord, 3), (N::Beads, 2)],
    });
    n.material = match n.style {
        N::Cable if is_metal(pendant.material) => pendant.material,
        N::Cable => *rng.pick(&[Iron, Brass]),
        N::Cord => Cord,
        N::Thong => Leather,
        N::Beads => *rng.weighted(&[(Wood, 3), (Bone, 2), (Brass, 1), (Stone, 1)]),
    };
    n.drop = if rng.chance(0.15) { rng.range_f32(2.5, 3.8) } else { rng.range_f32(1.3, 2.0) };
    (n.link, n.thickness) = match n.style {
        N::Cable => (rng.range_f32(0.035, 0.065), rng.range_f32(0.005, 0.009)),
        N::Cord => (rng.range_f32(0.04, 0.06), rng.range_f32(0.01, 0.016)),
        N::Thong => (rng.range_f32(0.04, 0.06), rng.range_f32(0.012, 0.018)),
        N::Beads => (rng.range_f32(0.04, 0.07), rng.range_f32(0.006, 0.01)),
    };

    let carved = !is_metal(pendant.material);
    let target = match pendant.material {
        Wood => EtchTarget::Wood,
        Bone => EtchTarget::Bone,
        _ => EtchTarget::Metals,
    };
    s.etch = Etching::roll(rng, if carved { 0.2 } else { 0.3 }, &[(target, 1)], 1);
    s.fit(&LIMITS);
    s
}

// ───────────────────────── Geometry ─────────────────────────

/// Builds the amulet. Parts: `pendant`, `rim`, `bail`, `setting` and `stone` (a stone
/// centre), `device`, `drop_rings` and `drops`, then the necklace: `chain`, `cord`,
/// `thong`, or `string`, `beads` and `spacers`.
fn build_amulet(s: &AmuletState) -> ItemMesh {
    let p = &s.pendant;
    let n = &s.necklace;
    let mut rng = Mulberry32::new(s.seed.wrapping_mul(131).wrapping_add(3));
    let damage = s.cond.damage;
    let mut parts = Parts::new();

    let h = p.size;
    let th = p.thickness;
    let bevel = (th * 0.32).min(h * 0.04);
    let shape = plaque_shape(p.shape).scaled(h, h);
    // A battered pendant loses bits of its edge.
    let outline: Vec<P2> = if damage > 0.0 {
        shape
            .outline
            .iter()
            .map(|&[x, y]| {
                let k = 1.0 - damage * 0.08 * (value_noise3([x * 60.0 + 3.0, y * 60.0, 2.0], s.seed) - 0.4).max(0.0) * 2.2;
                [x * k, y * k]
            })
            .collect()
    } else {
        shape.outline.clone()
    };
    let top = outline.iter().fold(f32::MIN, |m, q| m.max(q[1]));
    let bottom = outline.iter().fold(f32::MAX, |m, q| m.min(q[1]));
    // The pendant hangs with its top just under the bail.
    let drop_y = -top;
    let front = th * 0.5;

    let mut body = bevelled_slab(&outline, &shape.holes, &SlabOptions::rounded(th, bevel));
    body.translate([0.0, drop_y, 0.0]);
    parts.add("pendant", body, p.material);

    if p.rim > 0.0 && !matches!(p.shape, PlaqueShape::Crescent | PlaqueShape::Ankh) {
        let k = 1.0 - p.rim;
        let inner: Vec<P2> = outline.iter().map(|&[x, y]| [x * k, y * k]).collect();
        let depth = th * 0.24;
        let mut rim = bevelled_slab(&outline, &[inner], &SlabOptions::rounded(depth, (depth * 0.45).min(h * p.rim * 0.3)));
        rim.translate([0.0, drop_y, front - th * 0.06]);
        parts.add("rim", rim, p.material);
    }

    let (centre, room) = plaque_centre(p.shape);
    let cy = drop_y + centre * h;
    match p.centre {
        PendantCentre::Stone if !rng.chance((damage * 0.45) as f64) => {
            let r = s.stone.size.min(h * room * 0.8) * 0.5;
            let mut cup = collet(r, r * 0.3, 28);
            let mut gem = cut_stone(cut_for(s.stone.stone, s.stone.cut), r);
            gem.translate([0.0, collet_seat(r, r * 0.3), 0.0]);
            for m in [&mut cup, &mut gem] {
                m.place([0.0, cy, front - th * 0.05], vec3::Z);
            }
            parts.add("setting", cup, p.material);
            parts.stone("stone", gem, s.stone.stone, 0.0, true);
        }
        PendantCentre::Device => {
            let depth = th * 0.22;
            let mut device = device_mesh(p.device, h * room, depth, s.seed);
            device.translate([0.0, cy, front - th * 0.04]);
            let tint = s.palette.get(p.material).unwrap_or(s.palette.gold).scaled(0.72);
            parts.add_look("device", device, Look::of(p.material).tinted(tint).without_outline());
        }
        _ => {}
    }

    if p.drops > 0 {
        let xs = outline.iter().fold((f32::MAX, f32::MIN), |(lo, hi), q| (lo.min(q[0]), hi.max(q[0])));
        let width = xs.1 - xs.0;
        let mut rings = MeshBuilder::new();
        let mut drops = MeshBuilder::new();
        let ring_r = h * 0.03;
        let mut tear = cut_stone(GemCut::Rose, h * 0.055);
        tear.rotate_x(PI).scale([0.8, 1.7, 0.8]);
        for k in 0..p.drops {
            let x = if p.drops == 1 { 0.0 } else { (k as f32 / (p.drops - 1) as f32 - 0.5) * width * 0.8 };
            // The lowest point of the outline near x.
            let edge = outline.iter().filter(|q| (q[0] - x).abs() < h * 0.05).fold(f32::MAX, |m, q| m.min(q[1]));
            let y = drop_y + if edge < f32::MAX { edge } else { bottom };
            let mut ring = jump_ring(ring_r, h * 0.007);
            ring.rotate_y(FRAC_PI_2).translate([x, y - h * 0.015, 0.0]);
            rings.append(&ring);
            if !rng.chance((damage * 0.35) as f64) {
                let mut d = tear.clone();
                d.translate([x, y - h * 0.02 - ring_r, 0.0]);
                drops.append(&d);
            }
        }
        parts.add_unoutlined("drop_rings", rings, p.material);
        parts.stone("drops", drops, s.stone.stone, 0.0, false);
    }

    // The bail: a loop standing in the pendant's plane, the necklace through it along X.
    let bail_r = (n.thickness * 2.4).max(h * 0.07);
    let bail_t = (h * 0.016).max(n.thickness * 0.7);
    let mut bail = torus(bail_r, bail_t, 28, 10, Shading::SMOOTH);
    bail.rotate_z(FRAC_PI_2).translate([0.0, bail_r * 0.8, 0.0]);
    parts.add("bail", bail, p.material);

    let rest = match n.style {
        NecklaceStyle::Cable => n.thickness.max(n.link * 0.09),
        NecklaceStyle::Beads => n.thickness * 0.45,
        _ => n.thickness,
    };
    let path = necklace_path([0.0, bail_r * 0.8 + bail_r - bail_t - rest, 0.0], n.drop);
    match n.style {
        NecklaceStyle::Cable => {
            let wire = n.thickness.max(n.link * 0.09);
            parts.add_unoutlined("chain", cable_chain(&path, true, n.link, wire, 1200), n.material);
        }
        NecklaceStyle::Cord => {
            parts.add_unoutlined("cord", twisted_cord(&path, true, n.thickness, 2), n.material);
        }
        NecklaceStyle::Thong => {
            parts.add_unoutlined("thong", thong(&path, true, n.thickness), n.material);
        }
        NecklaceStyle::Beads => {
            parts.add_unoutlined("string", cord(&path, true, (n.thickness * 0.45).max(0.002), 6), Cord);
            let br = n.link * 0.5;
            let one = bead(br, 12);
            let spacer_material = if n.material == Brass { Gold } else { Brass };
            let mut spacer_mesh = bead(br * 0.7, 10);
            spacer_mesh.scale([1.0, 0.6, 1.0]);
            let mut beads = MeshBuilder::new();
            let mut spacers = MeshBuilder::new();
            let bail_at: V3 = path[0];
            let clear = bail_r * 1.6 + br;
            for (k, (at, dir)) in stations(&path, true, br * 2.15, 0.0).into_iter().enumerate() {
                if vec3::distance(at, bail_at) < clear {
                    continue;
                }
                let (mut m, target) = if k % 6 == 0 { (spacer_mesh.clone(), &mut spacers) } else { (one.clone(), &mut beads) };
                m.place(at, dir);
                target.append(&m);
            }
            parts.add("beads", beads, n.material);
            parts.add("spacers", spacers, spacer_material);
        }
    }
    parts.finish()
}
