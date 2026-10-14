//! Rings: a band round the finger, and a setting on top (claws, a collet, a cluster or a
//! signet face), with accent stones on the shoulders.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (the `band`,
//! `setting` and `stone` entries of `SECTIONS`, `PRESETS.ring`, the `fam==='ring'` branch
//! of `randomize()`, the ring rules in `proportion()`, `buildRing()` and `forgeNoun()`).
//!
//! The roll departs from the prototype on purpose. The prototype picks the setting, the
//! band profile and the cut independently, so a signet can come on a knife-edge band and a
//! cluster on a dainty claw shank with stones bigger than the finger. Here a weighted
//! [`RingArchetype`] picks the setting, then the band, the stone and the metal to suit it.
//!
//! Item space: the finger runs along Z through the origin, the band lies in the XY plane,
//! and the setting sits on top (+Y). 1 unit = 10 cm, so a band's inner diameter is about
//! 0.18 (18 mm).

use std::f32::consts::{PI, TAU};

use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use crate::build::chain::{StrandLay, twisted_strands};
use crate::build::device::device_mesh;
use crate::build::emblem::plaque_shape;
use crate::build::gem::{claw_seat, claw_setting, collet, collet_seat, cut_for, cut_stone};
use crate::build::parts::{Look, Parts};
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::gem::{GemCut, Gemstone};
use crate::geom::polygon::P2;
use crate::geom::{GridOptions, ItemMesh, LatheOptions, MeshBuilder, Shading, SlabOptions, V3, bevelled_slab, lathe, stitch_grid, vec3};
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::device::Device;
use crate::sections::emblem::PlaqueShape;
use crate::sections::jewel::{SetStone, SetStoneLimits};
use crate::slider::{Sliders, sliders};

use MaterialKind::{Brass, Gold, Iron, Silver};

/// The ring generator.
pub struct Ring;

/// The band's cross-section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandProfile {
    Round,
    /// Domed outside, flat against the finger.
    DShape,
    /// A flat strip with softened edges.
    Flat,
    /// Flat inside, rising to a sharp ridge.
    KnifeEdge,
    /// Eight flat sides.
    Faceted,
}

impl BandProfile {
    pub const ALL: [BandProfile; 5] = [BandProfile::Round, BandProfile::DShape, BandProfile::Flat, BandProfile::KnifeEdge, BandProfile::Faceted];
}

/// The hoop.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Band {
    pub profile: BandProfile,
    /// Inner diameter (the finger).
    pub inner: f32,
    /// Across the finger.
    pub width: f32,
    /// Radially, at the back of the hoop.
    pub thickness: f32,
    /// How much the hoop thickens and widens toward the setting.
    pub shoulder: f32,
    /// One solid hoop, or 2 to 3 twisted wires.
    pub strands: u32,
    /// Full twists of a twisted band round the hoop.
    pub twists: u32,
    /// Accent stones on each shoulder.
    pub side_stones: u32,
    pub material: MaterialKind,
}

sliders! {
    BandLimits for Band {
        inner: 0.14, 0.24, 0.001;
        width: 0.015, 0.12, 0.001;
        thickness: 0.01, 0.05, 0.001;
        shoulder: 0.0, 1.5, 0.01;
        strands: 1.0, 3.0, 1.0;
        twists: 2.0, 14.0, 1.0;
        side_stones: 0.0, 3.0, 1.0;
    }
}

impl Default for Band {
    /// The prototype's `BASE.band`.
    fn default() -> Self {
        Band { profile: BandProfile::Round, inner: 0.18, width: 0.03, thickness: 0.02, shoulder: 0.4, strands: 1, twists: 7, side_stones: 0, material: Gold }
    }
}

/// What sits on top of the band.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingStyle {
    /// A plain band.
    None,
    /// A collet whose lip is turned over the stone.
    Bezel,
    /// Claws over a basket.
    Claws,
    /// A centre stone in claws ringed by a halo of small stones.
    Cluster,
    /// A flat face engraved with a device.
    Signet,
}

impl SettingStyle {
    pub const ALL: [SettingStyle; 5] = [SettingStyle::None, SettingStyle::Bezel, SettingStyle::Claws, SettingStyle::Cluster, SettingStyle::Signet];
}

/// A signet's face (the prototype's `setting.table`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignetFace {
    Oval,
    Round,
    Octagon,
    Shield,
}

impl SignetFace {
    pub const ALL: [SignetFace; 4] = [SignetFace::Oval, SignetFace::Round, SignetFace::Octagon, SignetFace::Shield];

    pub fn plaque(self) -> PlaqueShape {
        match self {
            SignetFace::Oval => PlaqueShape::Oval,
            SignetFace::Round => PlaqueShape::Disc,
            SignetFace::Octagon => PlaqueShape::Octagon,
            SignetFace::Shield => PlaqueShape::Shield,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Setting {
    pub style: SettingStyle,
    /// Height of the collet or claws above the band; a signet face's thickness.
    pub height: f32,
    /// Claws round the centre stone.
    pub claws: u32,
    /// Stones in a cluster's halo.
    pub halo: u32,
    pub face: SignetFace,
    /// Height of a signet's face.
    pub face_size: f32,
    /// What a signet is engraved with.
    pub device: Device,
    pub material: MaterialKind,
}

sliders! {
    SettingLimits for Setting {
        height: 0.005, 0.06, 0.001;
        claws: 3.0, 8.0, 1.0;
        halo: 5.0, 14.0, 1.0;
        face_size: 0.06, 0.22, 0.001;
    }
}

impl Default for Setting {
    /// The prototype's `BASE.setting`.
    fn default() -> Self {
        Setting {
            style: SettingStyle::Claws,
            height: 0.03,
            claws: 4,
            halo: 10,
            face: SignetFace::Oval,
            face_size: 0.13,
            device: Device::Tower,
            material: Gold,
        }
    }
}

/// Every parameter of one ring.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RingState {
    pub seed: u32,
    pub band: Band,
    pub setting: Setting,
    pub stone: SetStone,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

/// Slider limits for every ring section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RingLimits {
    pub band: BandLimits,
    pub setting: SettingLimits,
    pub stone: SetStoneLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges (it has no `RANGES.ring`).
pub const LIMITS: RingLimits = RingLimits {
    band: BandLimits::SLIDERS,
    setting: SettingLimits::SLIDERS,
    stone: SetStoneLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl RingState {
    /// Clamp and snap every slider into `limits`, then keep the parts in proportion (the
    /// prototype's `proportion()`): the stone no wider than 70% of the finger, a signet face
    /// at least a little wider than the band, the band no thicker than a quarter of the finger.
    pub fn fit(&mut self, limits: &RingLimits) {
        self.band.fit(&limits.band);
        self.setting.fit(&limits.setting);
        self.stone.fit(&limits.stone);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
        self.stone.size = self.stone.size.min(self.band.inner * 0.7);
        self.setting.face_size = self.setting.face_size.max(self.band.width * 1.1).min(limits.setting.face_size.max);
        self.band.thickness = self.band.thickness.min(self.band.inner * 0.25);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &RingLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("band", self.band.out_of_range(&limits.band)),
            tag("setting", self.setting.out_of_range(&limits.setting)),
            tag("stone", self.stone.out_of_range(&limits.stone)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

/// The kinds of ring "Forge new" composes. Not stored: the noun reads the state itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RingArchetype {
    /// One faceted stone held high in claws.
    Solitaire,
    /// A cabochon or rose cut in a collet.
    Bezel,
    /// A centre stone ringed by small ones.
    Cluster,
    /// A thick band with an engraved face.
    Signet,
    /// A plain hoop, sometimes channel-set with small stones.
    Band,
    /// Twisted wires.
    Twisted,
}

impl RingArchetype {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(RingArchetype, u32); 6] = [
        (RingArchetype::Solitaire, 22),
        (RingArchetype::Bezel, 18),
        (RingArchetype::Cluster, 12),
        (RingArchetype::Signet, 18),
        (RingArchetype::Band, 18),
        (RingArchetype::Twisted, 12),
    ];
}

impl Generator for Ring {
    type State = RingState;

    fn roll(seed: u32) -> RingState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let archetype = *rng.weighted(&RingArchetype::WEIGHTS);
        compose(&mut rng, seed, archetype)
    }

    /// `forgeNoun()` for rings.
    fn noun(s: &RingState) -> String {
        let gem = s.stone.stone.name();
        match s.setting.style {
            SettingStyle::Signet => "Signet Ring".to_string(),
            SettingStyle::Cluster => format!("{gem} Cluster Ring"),
            SettingStyle::None if s.band.strands > 1 => format!("Twisted {} Band", s.band.material.adjective()),
            SettingStyle::None => format!("{} Band", s.band.material.adjective()),
            SettingStyle::Bezel | SettingStyle::Claws => format!("{gem} Ring"),
        }
    }

    /// The hoop swept round the finger (or twisted from wires), thickening into the
    /// shoulders, with the setting built upright and seated on its crown.
    fn build(s: &RingState) -> ItemMesh {
        build_ring(s)
    }
}

impl Ring {
    /// "Forge new" forced to one archetype, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, a)` when `a` is what `s` rolls.
    pub fn roll_as(seed: u32, archetype: RingArchetype) -> RingState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, archetype)
    }
}

fn pick_stone(rng: &mut Mulberry32, cut: GemCut) -> Gemstone {
    let stones: Vec<Gemstone> = Gemstone::ALL.into_iter().filter(|g| g.suits(cut)).collect();
    *rng.pick(&stones)
}

fn compose(rng: &mut Mulberry32, seed: u32, a: RingArchetype) -> RingState {
    use BandProfile as P;
    use RingArchetype as A;
    let mut s = RingState { seed, ..RingState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);

    let metal = *rng.weighted(match a {
        A::Solitaire | A::Cluster => &[(Gold, 5), (Silver, 3), (Brass, 1)][..],
        A::Bezel => &[(Silver, 4), (Gold, 3), (Brass, 2), (Iron, 1)],
        A::Signet => &[(Gold, 4), (Silver, 3), (Brass, 2), (Iron, 2)],
        A::Band => &[(Gold, 3), (Silver, 3), (Brass, 2), (Iron, 3)],
        A::Twisted => &[(Silver, 4), (Gold, 3), (Brass, 2), (Iron, 2)],
    });
    let b = &mut s.band;
    b.material = metal;
    b.inner = rng.range_f32(0.15, 0.22);
    b.profile = *rng.weighted(match a {
        A::Solitaire => &[(P::Round, 4), (P::KnifeEdge, 2), (P::DShape, 2)][..],
        A::Bezel => &[(P::Flat, 3), (P::DShape, 3), (P::Round, 2), (P::Faceted, 1)],
        A::Cluster => &[(P::Round, 3), (P::DShape, 2), (P::KnifeEdge, 1)],
        A::Signet => &[(P::DShape, 4), (P::Flat, 2), (P::Faceted, 2)],
        A::Band => &[(P::Round, 2), (P::DShape, 3), (P::Flat, 3), (P::KnifeEdge, 2), (P::Faceted, 2)],
        A::Twisted => &[(P::Round, 1)],
    });
    (b.width, b.thickness, b.shoulder) = match a {
        A::Solitaire => (rng.range_f32(0.018, 0.03), rng.range_f32(0.016, 0.022), rng.range_f32(0.2, 0.6)),
        A::Bezel => (rng.range_f32(0.03, 0.06), rng.range_f32(0.018, 0.026), rng.range_f32(0.3, 0.8)),
        A::Cluster => (rng.range_f32(0.02, 0.035), rng.range_f32(0.017, 0.024), rng.range_f32(0.3, 0.7)),
        A::Signet => (rng.range_f32(0.035, 0.06), rng.range_f32(0.02, 0.028), rng.range_f32(0.8, 1.4)),
        A::Band => (rng.range_f32(0.03, 0.08), rng.range_f32(0.018, 0.03), rng.range_f32(0.0, 0.25)),
        A::Twisted => (rng.range_f32(0.03, 0.045), rng.range_f32(0.026, 0.034), 0.0),
    };
    b.strands = if a == A::Twisted { *rng.weighted(&[(2, 1), (3, 2)]) } else { 1 };
    b.twists = rng.int(5, 12);

    let st = &mut s.setting;
    st.style = match a {
        A::Solitaire => SettingStyle::Claws,
        A::Bezel => SettingStyle::Bezel,
        A::Cluster => SettingStyle::Cluster,
        A::Signet => SettingStyle::Signet,
        A::Band | A::Twisted => SettingStyle::None,
    };
    st.material = metal;
    if a == A::Solitaire && rng.chance(0.15) {
        // A white head on a yellow shank, or the other way round.
        st.material = if metal == Gold { Silver } else { Gold };
    }

    // The stone suits the setting.
    let cut = *rng.weighted(match a {
        A::Solitaire => &[(GemCut::Brilliant, 5), (GemCut::Rose, 2), (GemCut::Octahedron, 1)][..],
        A::Bezel => &[(GemCut::Cabochon, 4), (GemCut::Rose, 2), (GemCut::Orb, 1), (GemCut::Brilliant, 1)],
        A::Cluster => &[(GemCut::Brilliant, 3), (GemCut::Rose, 1)],
        _ => &[(GemCut::Brilliant, 2), (GemCut::Cabochon, 1)],
    });
    s.stone.stone = pick_stone(rng, cut);
    s.stone.cut = cut_for(s.stone.stone, cut);
    s.stone.size = match a {
        A::Solitaire => rng.range_f32(0.04, 0.08),
        A::Bezel => rng.range_f32(0.05, 0.11),
        A::Cluster => rng.range_f32(0.035, 0.06),
        _ => rng.range_f32(0.03, 0.05),
    };
    let r = s.stone.radius();
    st.height = match st.style {
        SettingStyle::Claws => r * rng.range_f32(0.8, 1.3),
        SettingStyle::Bezel => r * rng.range_f32(0.35, 0.7),
        SettingStyle::Cluster => r * rng.range_f32(0.8, 1.2),
        SettingStyle::Signet => rng.range_f32(0.008, 0.018),
        SettingStyle::None => rng.range_f32(0.02, 0.04),
    };
    st.claws = if a == A::Cluster { *rng.pick(&[4, 6]) } else { *rng.weighted(&[(4, 5), (6, 3), (3, 1), (5, 1), (8, 1)]) };
    st.halo = rng.int(6, 12);
    st.face = *rng.weighted(&[(SignetFace::Oval, 3), (SignetFace::Round, 2), (SignetFace::Octagon, 2), (SignetFace::Shield, 2)]);
    st.face_size = rng.range_f32(0.1, 0.16).max(s.band.width * 1.25);
    st.device = if rng.chance(0.9) { *rng.pick(&Device::SIGNS) } else { Device::None };

    s.band.side_stones = match a {
        A::Solitaire if rng.chance(0.35) => rng.int(1, 3),
        A::Bezel if rng.chance(0.3) => rng.int(1, 2),
        A::Cluster if rng.chance(0.25) => rng.int(1, 2),
        A::Band if rng.chance(0.2) => rng.int(1, 3),
        _ => 0,
    };

    let inlaid = if a == A::Signet { 2 } else { 1 };
    s.etch = Etching::roll(rng, 0.3, &[(EtchTarget::Metals, 1)], inlaid);
    s.fit(&LIMITS);
    s
}

// ───────────────────────── Geometry ─────────────────────────

/// The hoop as functions of the angle round the finger (0 at the top, under the setting).
struct Hoop {
    inner: f32,
    thickness: f32,
    width: f32,
    shoulder: f32,
    wear: f32,
}

impl Hoop {
    fn new(b: &Band, wear: f32) -> Self {
        Hoop { inner: b.inner * 0.5, thickness: b.thickness, width: b.width, shoulder: b.shoulder, wear }
    }

    /// How much of the shoulder swell applies at `th`: all at the top, none past a quarter turn.
    fn swell(th: f32) -> f32 {
        th.cos().max(0.0).powf(2.2)
    }

    /// Radial thickness at `th`, worn thin on the palm side.
    fn thickness(&self, th: f32) -> f32 {
        let palm = (-th.cos()).max(0.0).powi(2);
        self.thickness * (1.0 + self.shoulder * 1.4 * Self::swell(th)) * (1.0 - self.wear * 0.3 * palm)
    }

    fn width(&self, th: f32) -> f32 {
        self.width * (1.0 + self.shoulder * 0.6 * Self::swell(th))
    }

    /// The outward direction at `th`, in the XY plane.
    fn radial(th: f32) -> V3 {
        [th.sin(), th.cos(), 0.0]
    }

    /// A point of the section at `th`: `x` across the finger and `y` outward, both in -1..=1
    /// with y = -1 on the finger.
    fn point(&self, th: f32, [x, y]: P2) -> V3 {
        let t = self.thickness(th);
        vec3::add(vec3::scale(Self::radial(th), self.inner + t * 0.5 * (1.0 + y)), [0.0, 0.0, x * self.width(th) * 0.5])
    }

    /// Radius of the outer surface at `th`.
    fn outer(&self, th: f32) -> f32 {
        self.inner + self.thickness(th)
    }
}

/// The section as runs of points that shade smooth within themselves; runs meet at hard
/// edges. Counter-clockwise in (across, outward).
struct BandSection {
    runs: Vec<Vec<P2>>,
    /// A single closed run.
    closed: bool,
    /// Runs laid out point for point as `runs` whose normals the band shades with, when its
    /// own would break the light up (see `docs/rendering.md`).
    shading: Option<Vec<Vec<P2>>>,
}

impl BandSection {
    /// Half the section's width across the finger at height `y` (-1 on the finger, 1 at the
    /// top), as a share of the band's half width.
    fn half_width(&self, y: f32) -> f32 {
        let mut widest = 0.0f32;
        for run in &self.runs {
            let n = run.len();
            let edges = if self.closed { n } else { n - 1 };
            for i in 0..edges {
                let ([x0, y0], [x1, y1]) = (run[i], run[(i + 1) % n]);
                if (y0 - y) * (y1 - y) <= 0.0 && y0 != y1 {
                    widest = widest.max((x0 + (x1 - x0) * (y - y0) / (y1 - y0)).abs());
                }
            }
        }
        widest
    }
}

fn band_section(profile: BandProfile) -> BandSection {
    let ellipse = |n: usize, e: f32| -> Vec<P2> {
        (0..n)
            .map(|i| {
                let (s, c) = (TAU * i as f32 / n as f32).sin_cos();
                [c.signum() * c.abs().powf(e), s.signum() * s.abs().powf(e)]
            })
            .collect()
    };
    let plain = |runs: Vec<Vec<P2>>, closed: bool| BandSection { runs, closed, shading: None };
    match profile {
        BandProfile::Round => plain(vec![ellipse(24, 1.0)], true),
        BandProfile::Flat => plain(vec![ellipse(32, 0.4)], true),
        BandProfile::DShape => {
            let dome: Vec<P2> = (0..=16).map(|i| {
                let t = PI * i as f32 / 16.0;
                [t.cos(), -0.88 + 1.88 * t.sin().max(0.0).powf(0.85)]
            }).collect();
            // The inside is very slightly domed for comfort.
            let inside: Vec<P2> = (0..=6).map(|i| {
                let x = -1.0 + 2.0 * i as f32 / 6.0;
                [x, -0.88 - 0.12 * (1.0 - x * x)]
            }).collect();
            plain(vec![inside, dome], false)
        }
        BandProfile::KnifeEdge => plain(
            vec![
                vec![[-1.0, -1.0], [1.0, -1.0]],
                vec![[1.0, -1.0], [1.0, -0.5]],
                vec![[1.0, -0.5], [0.0, 1.0]],
                vec![[0.0, 1.0], [-1.0, -0.5]],
                vec![[-1.0, -0.5], [-1.0, -1.0]],
            ],
            false,
        ),
        BandProfile::Faceted => {
            // An octagon with flats against the finger, on top and at the sides. The facets
            // stay in the silhouette but shade as a round band: each flat facet crosses a toon
            // band edge at its own point round the hoop, so the light stepped round the band
            // in a staircase.
            const SPLITS: usize = 4;
            let k = 1.0 / (PI / 8.0).cos();
            let corner = |i: usize| -> P2 {
                let a = PI / 8.0 + PI / 4.0 * i as f32;
                [a.cos() * k, a.sin() * k]
            };
            let octagon: Vec<P2> = (0..8 * SPLITS)
                .map(|j| {
                    let (a, b, t) = (corner(j / SPLITS), corner(j / SPLITS + 1), (j % SPLITS) as f32 / SPLITS as f32);
                    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
                })
                .collect();
            let round: Vec<P2> = octagon
                .iter()
                .map(|&[x, y]| {
                    let (s, c) = y.atan2(x).sin_cos();
                    [c, s]
                })
                .collect();
            BandSection { runs: vec![octagon], closed: true, shading: Some(vec![round]) }
        }
    }
}

/// Points per full turn of the hoop.
const HOOP_ROWS: usize = 160;

fn hoop_mesh(hoop: &Hoop, profile: BandProfile) -> MeshBuilder {
    let section = band_section(profile);
    let stitch = |run: &[P2]| -> MeshBuilder {
        let rows: Vec<Vec<V3>> = (0..HOOP_ROWS)
            .map(|i| {
                let th = TAU * i as f32 / HOOP_ROWS as f32;
                run.iter().map(|&q| hoop.point(th, q)).collect()
            })
            .collect();
        stitch_grid(&rows, &GridOptions { wrap_around: section.closed, wrap_along: true, shading: Shading::SMOOTH, row_v: None })
    };
    let mut out = MeshBuilder::new();
    for (i, run) in section.runs.iter().enumerate() {
        let mut grid = stitch(run);
        if let Some(shading) = &section.shading {
            grid.normals = stitch(&shading[i]).normals;
        }
        // Face away from the section's centre line.
        let outward: f32 = grid
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| grid.positions[i as usize]);
                let face = vec3::cross(vec3::sub(b, a), vec3::sub(c, a));
                let mid = vec3::centroid(&[a, b, c]);
                let th = mid[0].atan2(mid[1]);
                let centre = vec3::scale(Hoop::radial(th), hoop.inner + hoop.thickness(th) * 0.5);
                vec3::dot(face, vec3::sub(mid, centre))
            })
            .sum();
        if outward < 0.0 {
            grid.flip();
        }
        out.append(&grid);
    }
    out
}

fn twisted_hoop(hoop: &Hoop, strands: u32, twists: u32) -> MeshBuilder {
    let t = hoop.thickness;
    let rs = t * 0.5 * if strands == 2 { 0.62 } else { 0.52 };
    let lay = t * 0.5 - rs * 0.75;
    // The innermost wire just touches the finger.
    let path: Vec<V3> = (0..HOOP_ROWS).map(|i| vec3::scale(Hoop::radial(TAU * i as f32 / HOOP_ROWS as f32), hoop.inner + lay + rs)).collect();
    twisted_strands(
        &path,
        true,
        &StrandLay { strands: strands as usize, lay, wire: rs, turns: twists as f32, aspect: hoop.width / t, sides: 8, up: Some(vec3::Y) },
    )
}

fn build_ring(s: &RingState) -> ItemMesh {
    let b = &s.band;
    let st = &s.setting;
    let mut parts = Parts::new();
    let mut rng = Mulberry32::new(s.seed.wrapping_mul(113).wrapping_add(7));
    let damage = s.cond.damage;
    let hoop = Hoop::new(b, s.cond.wear);
    let twisted = b.strands > 1;

    if twisted {
        parts.add("band", twisted_hoop(&hoop, b.strands, b.twists), b.material);
    } else {
        parts.add("band", hoop_mesh(&hoop, b.profile), b.material);
    }

    let crown = if twisted { hoop.inner + b.thickness * 1.1 } else { hoop.inner + hoop.thickness(0.0) * 0.96 };
    let r = s.stone.radius();
    let cut = cut_for(s.stone.stone, s.stone.cut);
    let lost = rng.chance((damage * 0.45) as f64);
    let mut setting = Parts::new();
    let mut side_start = 0.45;
    match st.style {
        SettingStyle::None => {}
        SettingStyle::Bezel => {
            setting.add("setting", collet(r, st.height, 28), st.material);
            if !lost {
                let mut gem = cut_stone(cut, r);
                gem.translate([0.0, collet_seat(r, st.height), 0.0]);
                setting.stone("stone", gem, s.stone.stone, 0.0, true);
            }
        }
        SettingStyle::Claws | SettingStyle::Cluster => {
            let bends: Vec<f32> = (0..st.claws).map(|_| damage * 0.6 * (rng.next_f64() as f32 - 0.5)).collect();
            let (basket, claws) = claw_setting(r, st.height, st.claws, |k| bends[k as usize]);
            if st.style == SettingStyle::Cluster {
                let h = st.height;
                let platform = lathe(
                    &[[r * 0.4, 0.0], [r * 2.0, h * 0.7], [r * 2.05, h * 0.85], [r * 0.3, h * 0.9]],
                    &LatheOptions { facets: 32, shading: Shading::SMOOTH, ..Default::default() },
                );
                setting.add("setting", MeshBuilder::merged([&basket, &platform]), st.material);
                // The halo: small collets round the centre, shrunk to fit their ring.
                let n = st.halo.max(3);
                let hr = (r * 0.36).min(PI * 1.55 * r / n as f32 * 0.9);
                let mut cups = MeshBuilder::new();
                let mut stones = MeshBuilder::new();
                for k in 0..n {
                    let a = TAU * k as f32 / n as f32;
                    let at = [a.cos() * r * 1.55, h * 0.85, -a.sin() * r * 1.55];
                    let mut cup = collet(hr, hr * 0.5, 12);
                    cup.translate(at);
                    cups.append(&cup);
                    if !rng.chance((damage * 0.35) as f64) {
                        let mut g = cut_stone(GemCut::Brilliant, hr);
                        g.translate(vec3::add(at, [0.0, collet_seat(hr, hr * 0.5), 0.0]));
                        stones.append(&g);
                    }
                }
                setting.add_unoutlined("halo_settings", cups, st.material);
                setting.stone("accent_stones", stones, s.stone.stone, 0.0, false);
                side_start = 0.7;
            } else {
                setting.add("setting", basket, st.material);
            }
            setting.add_unoutlined("claws", claws, st.material);
            if !lost {
                let mut gem = cut_stone(cut, r);
                gem.translate([0.0, claw_seat(r, st.height), 0.0]);
                setting.stone("stone", gem, s.stone.stone, 0.0, true);
            }
        }
        SettingStyle::Signet => {
            let size = st.face_size;
            let oval = st.face == SignetFace::Oval;
            let shape = plaque_shape(st.face.plaque()).scaled(size * if oval { 1.15 } else { 1.0 }, size);
            let tt = st.height.max(0.008);
            let bevel = (tt * 0.3).min(size * 0.04);
            // The rim is rounded over, and an octagon's corners crease: each flat side shades
            // evenly along its length, light to dark only across the rounded rim, and the
            // light changes at the corners as on any faceted stone. Shading the octagon as a
            // round face spread the corner normals across each flat, so a toon band edge
            // crossed the rim partway along a side.
            let options = SlabOptions::rounded(tt, bevel);
            let mut face = bevelled_slab(&shape.outline, &shape.holes, &options);
            face.rotate_x(-PI / 2.0).translate([0.0, tt * 0.5, 0.0]);
            // The bolster: the shoulders gather into a flared skirt under the face, from inside
            // the band at its foot to just inside the face's outline. The foot keeps inside the
            // band's sides where they narrow away from the crown, so the skirt rises out of
            // them along one clean seam instead of poking through in lumps.
            let half_x = shape.outline.iter().fold(0.0f32, |m, p| m.max(p[0].abs()));
            let half_z = size * 0.5;
            let drop = hoop.thickness(0.0) * 0.9;
            let section = band_section(b.profile);
            let rows: Vec<Vec<V3>> = (0..=12)
                .map(|i| {
                    let v = i as f32 / 12.0;
                    let flare = v * v;
                    let y = -drop * (1.0 - v) + tt * 0.15 * v;
                    (0..48)
                        .map(|k| {
                            let (sin, cos) = (TAU * k as f32 / 48.0).sin_cos();
                            let foot_x = half_x * 0.55 * cos;
                            let th = foot_x.atan2(crown - drop);
                            let level = 2.0 * (foot_x.hypot(crown - drop) - hoop.inner) / hoop.thickness(th) - 1.0;
                            let inside = hoop.width(th) * 0.5 * section.half_width(level.clamp(-1.0, 1.0)) * 0.85;
                            let foot_z = inside.min(half_z) * sin;
                            let top = [half_x * 0.95 * cos, half_z * 0.95 * sin];
                            [foot_x + (top[0] - foot_x) * flare, y, -(foot_z + (top[1] - foot_z) * flare)]
                        })
                        .collect()
                })
                .collect();
            let bolster = stitch_grid(&rows, &GridOptions { wrap_around: true, wrap_along: false, shading: Shading::SMOOTH, row_v: None });
            setting.add("setting", MeshBuilder::merged([&bolster, &face]), st.material);
            let depth = tt * 0.22;
            let mut device = device_mesh(st.device, size * 0.55 * if oval { 0.85 } else { 1.0 }, depth, s.seed);
            device.rotate_x(-PI / 2.0).translate([0.0, tt - depth * 0.4, 0.0]);
            let tint = s.palette.get(st.material).unwrap_or(s.palette.gold).scaled(0.72);
            setting.add_look("device", device, Look::of(st.material).tinted(tint).without_outline());
            side_start = f32::INFINITY;
        }
    }
    setting.place([0.0, crown - 0.002, 0.0], vec3::Y);
    parts.extend(setting);

    // Accent stones set into the shoulders.
    if b.side_stones > 0 && !twisted && side_start.is_finite() {
        let ssz = (hoop.width(0.6) * 0.42).min((s.stone.size * 0.4).max(0.012)).min(b.thickness * 1.5);
        let sr = ssz * 0.5;
        let side_cut = if cut == GemCut::Cabochon { GemCut::Cabochon } else { GemCut::Brilliant };
        let mut cups = MeshBuilder::new();
        let mut stones = MeshBuilder::new();
        for k in 0..b.side_stones {
            for sign in [1.0f32, -1.0] {
                let th = sign * (side_start + k as f32 * 0.24);
                let up = Hoop::radial(th);
                let at = vec3::scale(up, hoop.outer(th) * 0.985);
                let mut cup = collet(sr, ssz * 0.22, 12);
                cup.place(at, up);
                cups.append(&cup);
                if !rng.chance((damage * 0.35) as f64) {
                    let mut g = cut_stone(side_cut, sr);
                    g.translate([0.0, collet_seat(sr, ssz * 0.22), 0.0]).place(at, up);
                    stones.append(&g);
                }
            }
        }
        parts.add_unoutlined("side_settings", cups, st.material);
        parts.stone("side_stones", stones, s.stone.stone, 0.0, false);
    }
    parts.finish()
}
