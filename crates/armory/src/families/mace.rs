//! Maces: a hafted shaft with a weighted head.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (`SECTIONS`,
//! `RANGES.mace`, `PRESETS.mace`, the `fam==='mace'` branch of `randomize()`, and
//! `forgeNoun()`).
//!
//! The roll departs from the prototype on purpose. The prototype starts from a random preset,
//! nudges every slider and re-picks a few selects at random, so ornaments rarely agree (gems
//! with no cradle, cup heads that never appear, prongs that never roll). Here a weighted
//! [`MaceArchetype`] composes the head first, then sizes the ornaments and picks materials
//! from it, while the prototype's per-slider nudge still varies everything the archetype
//! leaves alone.

use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::gem::{GemCut, Gemstone};
use crate::build::gem::{bezel, cut_for, cut_stone, stone_reach};
use crate::build::hafted::{Haft, butt_radius, grip_mesh, grip_span, pommel_mesh, shaft_bands, turned, wrap_mesh};
use crate::build::head::{HeadAssembly, HeadSections, head_assembly};
use crate::build::parts::Parts;
use crate::geom::ItemMesh;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::hafted::{Grip, GripLimits, Pommel, PommelGem, PommelLimits, PommelStyle, Shaft, ShaftLimits, Wrap, WrapLimits};
use crate::sections::head::{
    Collar, CollarLimits, FinialLimits, Finial, FinialStyle, FlangeShape, Flanges, FlangesLimits, Head, HeadGem,
    HeadGemLimits, HeadLimits, HeadStyle, Hemisphere, Prongs, ProngsLimits, SpikePattern, Spikes, SpikesLimits,
};
use crate::slider::Sliders;
use procgen_rng::{armory_stream_seed, Mulberry32};
use serde::{Deserialize, Serialize};

use MaterialKind::{Bone, Brass, Cord, Gold, Iron, Leather, Silver, Stone, Wood};

/// The mace generator.
pub struct Mace;

/// Every parameter of one mace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaceState {
    pub seed: u32,
    pub shaft: Shaft,
    pub grip: Grip,
    pub wrap: Wrap,
    pub pommel: Pommel,
    pub collar: Collar,
    pub head: Head,
    pub flanges: Flanges,
    pub spikes: Spikes,
    pub prongs: Prongs,
    pub finial: Finial,
    pub gem: HeadGem,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for MaceState {
    /// The prototype's `BASE` state for the mace sections.
    fn default() -> Self {
        MaceState {
            seed: 0,
            shaft: Shaft::default(),
            grip: Grip::default(),
            wrap: Wrap::default(),
            pommel: Pommel::default(),
            collar: Collar::default(),
            head: Head::default(),
            flanges: Flanges::default(),
            spikes: Spikes::default(),
            prongs: Prongs::default(),
            finial: Finial::default(),
            gem: HeadGem::default(),
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every mace section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaceLimits {
    pub shaft: ShaftLimits,
    pub grip: GripLimits,
    pub wrap: WrapLimits,
    pub pommel: PommelLimits,
    pub collar: CollarLimits,
    pub head: HeadLimits,
    pub flanges: FlangesLimits,
    pub spikes: SpikesLimits,
    pub prongs: ProngsLimits,
    pub finial: FinialLimits,
    pub gem: HeadGemLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges with `RANGES.mace` applied.
pub const LIMITS: MaceLimits = MaceLimits {
    shaft: ShaftLimits {
        length: ShaftLimits::SLIDERS.length.narrowed(3.0, 7.0),
        radius: ShaftLimits::SLIDERS.radius.narrowed(0.1, 0.22),
        ..ShaftLimits::SLIDERS
    },
    grip: GripLimits { length: GripLimits::SLIDERS.length.narrowed(0.8, 2.4), ..GripLimits::SLIDERS },
    wrap: WrapLimits::SLIDERS,
    pommel: PommelLimits {
        length: PommelLimits::SLIDERS.length.narrowed(0.2, 0.8),
        radius: PommelLimits::SLIDERS.radius.narrowed(0.12, 0.4),
        ..PommelLimits::SLIDERS
    },
    collar: CollarLimits::SLIDERS,
    head: HeadLimits {
        length: HeadLimits::SLIDERS.length.narrowed(0.6, 2.4),
        radius: HeadLimits::SLIDERS.radius.narrowed(0.15, 0.8),
        ..HeadLimits::SLIDERS
    },
    flanges: FlangesLimits::SLIDERS,
    spikes: SpikesLimits::SLIDERS,
    prongs: ProngsLimits::SLIDERS,
    finial: FinialLimits::SLIDERS,
    gem: HeadGemLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl MaceState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &MaceLimits) {
        self.shaft.fit(&limits.shaft);
        self.grip.fit(&limits.grip);
        self.wrap.fit(&limits.wrap);
        self.pommel.fit(&limits.pommel);
        self.collar.fit(&limits.collar);
        self.head.fit(&limits.head);
        self.flanges.fit(&limits.flanges);
        self.spikes.fit(&limits.spikes);
        self.prongs.fit(&limits.prongs);
        self.finial.fit(&limits.finial);
        self.gem.fit(&limits.gem);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    /// Nudge every slider the way the prototype's "Forge new" does: each moves with
    /// probability `odds` by up to `spread / 2` of its range.
    fn nudge(&mut self, rng: &mut Mulberry32, limits: &MaceLimits, odds: f64, spread: f32) {
        self.shaft.nudge(rng, &limits.shaft, odds, spread);
        self.grip.nudge(rng, &limits.grip, odds, spread);
        self.wrap.nudge(rng, &limits.wrap, odds, spread);
        self.pommel.nudge(rng, &limits.pommel, odds, spread);
        self.collar.nudge(rng, &limits.collar, odds, spread);
        self.head.nudge(rng, &limits.head, odds, spread);
        self.flanges.nudge(rng, &limits.flanges, odds, spread);
        self.spikes.nudge(rng, &limits.spikes, odds, spread);
        self.prongs.nudge(rng, &limits.prongs, odds, spread);
        self.finial.nudge(rng, &limits.finial, odds, spread);
        self.gem.nudge(rng, &limits.gem, odds, spread);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &MaceLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("shaft", self.shaft.out_of_range(&limits.shaft)),
            tag("grip", self.grip.out_of_range(&limits.grip)),
            tag("wrap", self.wrap.out_of_range(&limits.wrap)),
            tag("pommel", self.pommel.out_of_range(&limits.pommel)),
            tag("collar", self.collar.out_of_range(&limits.collar)),
            tag("head", self.head.out_of_range(&limits.head)),
            tag("flanges", self.flanges.out_of_range(&limits.flanges)),
            tag("spikes", self.spikes.out_of_range(&limits.spikes)),
            tag("prongs", self.prongs.out_of_range(&limits.prongs)),
            tag("finial", self.finial.out_of_range(&limits.finial)),
            tag("gem", self.gem.out_of_range(&limits.gem)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

/// The kinds of mace "Forge new" composes. Not stored: the noun reads the state itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaceArchetype {
    /// Radial flanges on a slim core (flanged mace, shestopyor, pernach).
    Flanged,
    /// Rings of spikes on a round head.
    Morningstar,
    /// Short flanges with a spike between each pair.
    SpikedFlanged,
    /// Precious metal, prongs cradling a stone, sometimes small fan flanges.
    Ceremonial,
    /// A bare knobbed head of iron, bronze, stone or wood.
    Plain,
}

impl MaceArchetype {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(MaceArchetype, u32); 5] = [
        (MaceArchetype::Flanged, 30),
        (MaceArchetype::Morningstar, 25),
        (MaceArchetype::SpikedFlanged, 15),
        (MaceArchetype::Ceremonial, 12),
        (MaceArchetype::Plain, 18),
    ];
}

impl Generator for Mace {
    type State = MaceState;

    fn roll(seed: u32) -> MaceState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let archetype = *rng.weighted(&MaceArchetype::WEIGHTS);
        compose(&mut rng, seed, archetype)
    }

    /// `forgeNoun()` for maces.
    fn noun(s: &MaceState) -> String {
        let noun = if s.gem.on && s.prongs.on {
            "Ceremonial Mace"
        } else if s.flanges.on && s.spikes.on {
            "Spiked Flanged Mace"
        } else if s.flanges.on {
            match s.flanges.count {
                6 => "Shestopyor",
                n if n >= 7 => "Pernach",
                _ => "Flanged Mace",
            }
        } else if s.spikes.on {
            "Morningstar"
        } else if s.gem.on {
            "Ceremonial Mace"
        } else {
            "Mace"
        };
        noun.to_string()
    }

    /// The handle from [`crate::build::hafted`], then [`head_assembly`] from
    /// [`crate::build::head`] set on the top of the haft, with the stone seated in the
    /// prongs' cradle or a bezel. One part per section and material.
    fn build(s: &MaceState) -> ItemMesh {
        let mut parts = Parts::new();

        // Handle.
        let span = grip_span(&s.shaft, &s.grip);
        let mut haft = Haft::new(&s.shaft, s.seed);
        if s.grip.on || s.wrap.on {
            haft.clear(span.0, span.1);
        }
        parts.add("shaft", haft.mesh(), s.shaft.material);
        let avoid: &[(f32, f32)] = if s.grip.on || s.wrap.on { &[span] } else { &[] };
        parts.add("bands", shaft_bands(&haft, &s.shaft, avoid), Iron);
        if s.grip.on {
            parts.add("grip", grip_mesh(&haft, &s.grip, span), s.grip.material);
        }
        if s.wrap.on {
            parts.add("wrap", wrap_mesh(&haft, &s.grip, span, &s.wrap), s.wrap.material);
        }
        if s.pommel.on {
            let pommel = pommel_mesh(&s.pommel, butt_radius(&haft, &s.grip, span)).placed_on(&haft);
            parts.add("pommel", pommel.body, s.pommel.material);
            parts.stone("pommel_stones", pommel.stones, s.pommel.gem_stone, 0.0, false);
        }

        // Head assembly, built upright on the top of the shaft, then set on it.
        let sections = HeadSections {
            collar: &s.collar,
            head: &s.head,
            flanges: &s.flanges,
            spikes: &s.spikes,
            prongs: &s.prongs,
            finial: &s.finial,
        };
        let mut head = head_assembly(sections, haft.outer_radius(1.0), s.seed);
        if s.gem.on {
            seat_stone(&mut head, s);
        }
        head.parts.place(haft.point(1.0), haft.tangent(1.0));
        parts.extend(head.parts);
        parts.finish()
    }
}

/// Seats the head stone: closed on by the prongs' claws, or set on the crown in a bezel.
fn seat_stone(head: &mut HeadAssembly, s: &MaceState) {
    let size = s.gem.size;
    let cut = cut_for(s.gem.stone, s.gem.cut);
    let (_, up) = stone_reach(cut, size);
    let rest = head.crown + size * 0.25;
    let girdle = match &head.cradle {
        // Claws close on the girdle; a stone too big for them rests on their tips.
        Some(c) => c.seat(size).unwrap_or(c.tip() - up * 0.3).max(rest),
        None => rest,
    };
    let mut stone = cut_stone(cut, size);
    stone.translate([0.0, girdle, 0.0]);
    head.parts.stone("gem", stone, s.gem.stone, s.gem.glow, true);
    if head.cradle.is_none() {
        let (sides, shading) = turned(s.head.facets.max(8));
        let mut setting = bezel(size, sides, shading);
        setting.translate([0.0, girdle, 0.0]);
        let metal = if matches!(s.head.material, Gold | Silver | Brass) { s.head.material } else { Brass };
        head.parts.add("bezel", setting, metal);
    }
}

impl Mace {
    /// "Forge new" forced to one archetype, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, a)` when `a` is what `s` rolls.
    pub fn roll_as(seed: u32, archetype: MaceArchetype) -> MaceState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, archetype)
    }
}

/// A trim metal that sets off `m`: brass on iron, gold on brass and silver, iron bindings
/// on stone, wood and bone.
fn trim_for(m: MaterialKind) -> MaterialKind {
    match m {
        Iron => Brass,
        Brass | Gold | Silver => Gold,
        _ => Iron,
    }
}

fn is_metal(m: MaterialKind) -> bool {
    matches!(m, Iron | Brass | Gold | Silver)
}

fn compose(rng: &mut Mulberry32, seed: u32, a: MaceArchetype) -> MaceState {
    use MaceArchetype as A;
    let mut s = MaceState { seed, ..MaceState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    // The prototype's generic pass: most sliders drift up to 15% of their range from base.
    s.nudge(rng, &LIMITS, 0.75, 0.3);

    roll_head(rng, a, &mut s.head);
    let head = s.head;

    match a {
        A::Flanged => {
            s.flanges = Flanges {
                on: true,
                count: *rng.weighted(&[(4, 1), (5, 1), (6, 5), (7, 3), (8, 3), (9, 1), (10, 1), (12, 1)]),
                shape: *rng.weighted(&[(FlangeShape::Blade, 4), (FlangeShape::Stepped, 3), (FlangeShape::Crescent, 2), (FlangeShape::Leaf, 2)]),
                depth: rng.range_f32(0.35, 0.7),
                thickness: rng.range_f32(0.04, 0.09),
                height: rng.range_f32(0.75, 1.0),
                bias: rng.range_f32(-0.08, 0.08),
                material: head.material,
                ..s.flanges
            };
            roll_flange_details(rng, &mut s.flanges);
            if head.style == HeadStyle::Drum && s.flanges.count >= 5 {
                s.head.facets = s.flanges.count;
            }
        }
        A::Morningstar => {
            s.spikes.on = true;
            s.spikes.count = rng.int(5, 10);
            s.spikes.rings = rng.int(2, 5);
            s.spikes.pattern = *rng.weighted(&[
                (SpikePattern::Staggered, 4),
                (SpikePattern::Aligned, 2),
                (SpikePattern::Spiral, 1),
                (SpikePattern::GoldenAngle, 1),
            ]);
            s.spikes.spread = rng.range_f32(0.55, 0.85);
            s.spikes.bias = rng.range_f32(-0.05, 0.05);
            s.spikes.hemisphere = *rng.weighted(&[(Hemisphere::Both, 8), (Hemisphere::Upper, 2), (Hemisphere::Lower, 1)]);
            s.spikes.twist = if s.spikes.pattern == SpikePattern::Spiral { rng.range_f32(15.0, 45.0) } else { 0.0 };
            s.spikes.phase = rng.range_f32(0.0, 360.0);
            s.spikes.jitter = if rng.chance(0.3) { rng.range_f32(0.02, 0.15) } else { 0.0 };
            s.spikes.tilt = if rng.chance(0.15) { rng.range_f32(-20.0, 20.0) } else { 0.0 };
            s.spikes.falloff = rng.range_f32(0.1, 0.4);
            roll_spike_shape(rng, &head, &mut s.spikes, 0.6, 1.2);
        }
        A::SpikedFlanged => {
            s.flanges = Flanges {
                on: true,
                count: *rng.weighted(&[(4, 2), (5, 1), (6, 3), (8, 2)]),
                shape: *rng.weighted(&[(FlangeShape::Blade, 2), (FlangeShape::Stepped, 1), (FlangeShape::Leaf, 1)]),
                depth: rng.range_f32(0.15, 0.3),
                thickness: rng.range_f32(0.04, 0.08),
                height: rng.range_f32(0.6, 0.9),
                bias: 0.0,
                material: head.material,
                ..s.flanges
            };
            roll_flange_details(rng, &mut s.flanges);
            // One spike between each pair of flanges.
            s.spikes.on = true;
            s.spikes.count = s.flanges.count;
            s.spikes.rings = rng.int(1, 2);
            s.spikes.pattern = SpikePattern::Aligned;
            s.spikes.hemisphere = Hemisphere::Both;
            s.spikes.phase = s.flanges.phase + 180.0 / s.flanges.count as f32;
            s.spikes.spread = rng.range_f32(0.3, 0.6);
            s.spikes.bias = 0.0;
            s.spikes.twist = 0.0;
            s.spikes.jitter = 0.0;
            s.spikes.tilt = 0.0;
            roll_spike_shape(rng, &head, &mut s.spikes, 0.6, 1.0);
        }
        A::Ceremonial => {
            roll_head_gem(rng, &head, &mut s.gem, &[
                (GemCut::Brilliant, 4),
                (GemCut::Rose, 2),
                (GemCut::Orb, 2),
                (GemCut::Octahedron, 1),
                (GemCut::Cabochon, 1),
                (GemCut::Shard, 1),
            ]);
            s.gem.size = if head.style == HeadStyle::Cup {
                head.radius * rng.range_f32(0.6, 0.85)
            } else {
                rng.range_f32(0.2, 0.35).min(head.radius * 0.9)
            };
            let count = *rng.weighted(&[(3, 2), (4, 4), (5, 2), (6, 2)]);
            s.prongs = Prongs {
                on: true,
                count,
                height: s.gem.size * rng.range_f32(2.2, 3.2),
                spread: head.radius * rng.range_f32(0.3, 0.6),
                curl: rng.range_f32(0.6, 0.9),
                thickness: rng.range_f32(0.03, 0.06),
                twist: if rng.chance(0.3) { rng.range_f32(-0.2, 0.2) } else { 0.0 },
                base: rng.range_f32(0.5, 0.9),
                phase: 180.0 / count as f32,
                material: head.material,
            };
            if rng.chance(0.4) {
                // Small fan flanges, as on a bishop's mace.
                s.flanges = Flanges {
                    on: true,
                    count: rng.int(6, 10),
                    phase: 0.0,
                    height: rng.range_f32(0.4, 0.6),
                    bias: rng.range_f32(-0.1, 0.0),
                    shape: FlangeShape::Fan,
                    depth: rng.range_f32(0.12, 0.25),
                    thickness: rng.range_f32(0.03, 0.05),
                    notch: 0.0,
                    pierce: false,
                    lean: 0.0,
                    material: head.material,
                };
            }
        }
        A::Plain => {
            if rng.chance(0.15) {
                // A stone set in the crown of the head, no cradle.
                roll_head_gem(rng, &head, &mut s.gem, &[(GemCut::Cabochon, 2), (GemCut::Orb, 2), (GemCut::Brilliant, 1)]);
                s.gem.size = head.radius * rng.range_f32(0.35, 0.55);
            }
        }
    }

    roll_finial(rng, a, &s.head, &s.spikes, s.gem.on, &mut s.finial);
    roll_collar(rng, a, &s.head, &mut s.collar);
    roll_shaft(rng, a, &s.head, &mut s.shaft);
    roll_grip_and_wrap(rng, a, &s.shaft, &mut s.grip, &mut s.wrap);
    roll_pommel(rng, a, &s.shaft, &s.collar, &s.head, &s.gem, &mut s.pommel);

    let mut targets = vec![(EtchTarget::Metals, 3)];
    if s.shaft.material == Wood {
        targets.push((EtchTarget::Wood, 1));
    }
    let ceremonial = a == A::Ceremonial;
    s.etch = Etching::roll(rng, if ceremonial { 0.6 } else { 0.3 }, &targets, if ceremonial { 2 } else { 1 });

    s.fit(&LIMITS);
    s
}

fn roll_head(rng: &mut Mulberry32, a: MaceArchetype, head: &mut Head) {
    use HeadStyle::*;
    use MaceArchetype as A;
    head.style = *rng.weighted(match a {
        A::Flanged => &[(Pear, 3), (Drum, 3), (Orb, 2), (Onion, 1)][..],
        A::Morningstar => &[(Orb, 5), (Pear, 2), (Drum, 1), (Onion, 1)],
        A::SpikedFlanged => &[(Orb, 2), (Drum, 2), (Pear, 1)],
        A::Ceremonial => &[(Onion, 3), (Cup, 3), (Orb, 2), (Pear, 1)],
        A::Plain => &[(Orb, 3), (Pear, 2), (Drum, 2), (Onion, 2)],
    });
    head.material = *rng.weighted(match a {
        A::Flanged | A::Morningstar | A::SpikedFlanged => &[(Iron, 8), (Brass, 2)][..],
        A::Ceremonial => &[(Gold, 3), (Silver, 3), (Brass, 3)],
        A::Plain => &[(Iron, 5), (Brass, 2), (Stone, 2), (Wood, 1)],
    });
    head.radius = match a {
        A::Flanged => rng.range_f32(0.15, 0.28),
        A::Morningstar => rng.range_f32(0.35, 0.7),
        A::SpikedFlanged => rng.range_f32(0.3, 0.45),
        A::Ceremonial => rng.range_f32(0.3, 0.55),
        A::Plain => rng.range_f32(0.3, 0.6),
    };
    // Flanges define a flanged head's silhouette; otherwise the profile sets the proportions.
    let aspect = match head.style {
        Orb => rng.range_f32(0.95, 1.2),
        Pear => rng.range_f32(1.3, 1.7),
        Drum => rng.range_f32(1.0, 1.5),
        Onion => rng.range_f32(1.3, 1.8),
        Cup => rng.range_f32(0.7, 1.0),
        None => 1.0,
    };
    head.length = if matches!(a, A::Flanged | A::SpikedFlanged) { rng.range_f32(1.0, 1.8) } else { head.radius * 2.0 * aspect };
    head.belly = match head.style {
        Pear => rng.range_f32(0.55, 0.75),
        Onion => rng.range_f32(0.3, 0.42),
        Cup => rng.range_f32(0.6, 0.8),
        _ => rng.range_f32(0.45, 0.55),
    };
    head.facets = match (head.style, head.material) {
        (Drum, _) => *rng.pick(&[6, 8]),
        (_, Stone) => rng.int(7, 12),
        _ if a == A::Ceremonial => rng.int(20, 32),
        _ => rng.int(12, 24),
    };
}

/// Notch, piercing and lean, shared by the flanged archetypes.
fn roll_flange_details(rng: &mut Mulberry32, f: &mut Flanges) {
    f.phase = 0.0;
    f.notch = if rng.chance(0.5) { 0.0 } else { rng.range_f32(0.1, 0.5) };
    f.pierce = matches!(f.shape, FlangeShape::Blade | FlangeShape::Leaf | FlangeShape::Crescent) && rng.chance(0.3);
    f.lean = if rng.chance(0.2) { rng.range_f32(-20.0, 20.0) } else { 0.0 };
}

/// Spike length scales with the head (`lo..hi` times its radius); root and profile follow.
fn roll_spike_shape(rng: &mut Mulberry32, head: &Head, sp: &mut Spikes, lo: f32, hi: f32) {
    sp.length = head.radius * rng.range_f32(lo, hi);
    sp.base = sp.length * rng.range_f32(0.2, 0.35);
    sp.sides = *rng.weighted(&[(4, 4), (3, 1), (6, 2), (8, 1), (12, 1)]);
    sp.concave = rng.range_f32(0.0, 0.5);
    sp.material = if is_metal(head.material) && rng.chance(0.2) { head.material } else { Iron };
}

/// A stone whose cut suits it.
fn roll_head_gem(rng: &mut Mulberry32, head: &Head, gem: &mut HeadGem, cuts: &[(GemCut, u32)]) {
    gem.on = true;
    gem.cut = *rng.weighted(cuts);
    let stones: Vec<Gemstone> = Gemstone::ALL.into_iter().filter(|g| g.suits(gem.cut)).collect();
    gem.stone = *rng.pick(&stones);
    gem.glow = rng.range_f32(0.1, 0.5);
    gem.lift = 0.55;
    gem.bob = false;
    gem.size = gem.size.min(head.radius);
}

fn roll_finial(rng: &mut Mulberry32, a: MaceArchetype, head: &Head, spikes: &Spikes, gem_on: bool, f: &mut Finial) {
    use FinialStyle::*;
    use MaceArchetype as A;
    // A set stone crowns the head; nothing goes over it.
    f.style = if gem_on {
        None
    } else {
        *rng.weighted(match a {
            A::Flanged => &[(None, 2), (Knob, 2), (Spire, 2), (Spike, 1)][..],
            A::Morningstar => &[(Spike, 4), (None, 2), (Knob, 1)],
            A::SpikedFlanged => &[(Spike, 2), (Spire, 1), (None, 1)],
            A::Ceremonial => &[(Knob, 1), (Spire, 1)],
            A::Plain => &[(None, 2), (Knob, 3), (Spire, 1), (Spike, 1)],
        })
    };
    match f.style {
        Spike => {
            f.length = rng.range_f32(0.3, 0.6);
            f.radius = rng.range_f32(0.06, 0.12);
            f.sides = if spikes.on { spikes.sides } else { 4 };
            f.material = if spikes.on { spikes.material } else { Iron };
        }
        Knob => {
            f.length = rng.range_f32(0.2, 0.35);
            f.radius = rng.range_f32(0.08, 0.14);
            f.sides = rng.int(8, 16);
            f.material = if rng.chance(0.5) && is_metal(head.material) { head.material } else { trim_for(head.material) };
        }
        Spire => {
            f.length = rng.range_f32(0.4, 0.7);
            f.radius = rng.range_f32(0.06, 0.1);
            f.sides = *rng.pick(&[4, 6, 8]);
            f.material = if rng.chance(0.5) && is_metal(head.material) { head.material } else { trim_for(head.material) };
        }
        None => {}
    }
}

fn roll_collar(rng: &mut Mulberry32, a: MaceArchetype, head: &Head, c: &mut Collar) {
    use MaceArchetype as A;
    c.on = rng.chance(match a {
        A::Flanged => 0.85,
        A::Morningstar => 0.7,
        A::SpikedFlanged => 0.75,
        A::Ceremonial => 0.95,
        A::Plain => 0.6,
    });
    c.bands = rng.int(1, 3);
    c.height = if c.bands == 1 { rng.range_f32(0.15, 0.3) } else { rng.range_f32(0.06, 0.15) };
    c.radius = rng.range_f32(1.3, 1.7);
    c.facets = head.facets;
    c.material = if rng.chance(0.5) && is_metal(head.material) { head.material } else { trim_for(head.material) };
}

fn roll_shaft(rng: &mut Mulberry32, a: MaceArchetype, head: &Head, sh: &mut Shaft) {
    use MaceArchetype as A;
    let ceremonial = [(head.material, 3), (Wood, 1)];
    sh.material = *rng.weighted(match a {
        A::Flanged => &[(Iron, 5), (Wood, 4)][..],
        A::Morningstar => &[(Wood, 7), (Iron, 3)],
        A::SpikedFlanged => &[(Wood, 5), (Iron, 4)],
        A::Ceremonial => &ceremonial,
        A::Plain => &[(Wood, 7), (Iron, 2), (Bone, 1)],
    });
    sh.length = rng.range_f32(3.8, 6.0);
    sh.radius = match sh.material {
        Wood => rng.range_f32(0.13, 0.2),
        Bone => rng.range_f32(0.12, 0.16),
        _ => rng.range_f32(0.11, 0.16),
    };
    sh.taper = rng.range_f32(1.0, 1.2);
    sh.gnarl = if sh.material == Wood && rng.chance(0.25) { rng.range_f32(0.05, 0.25) } else { 0.0 };
    sh.bands = match sh.material {
        Wood if rng.chance(0.4) => rng.int(1, 3),
        Wood | Bone => 0,
        _ => rng.int(0, 4),
    };
    sh.band_size = rng.range_f32(0.015, 0.04);
}

fn roll_grip_and_wrap(rng: &mut Mulberry32, a: MaceArchetype, sh: &Shaft, g: &mut Grip, w: &mut Wrap) {
    let ceremonial = a == MaceArchetype::Ceremonial;
    g.on = ceremonial || rng.chance(0.9);
    g.position = 0.0;
    g.length = rng.range_f32(1.2, 2.0);
    g.thickness = rng.range_f32(1.08, 1.3);
    g.rivets = 0;
    g.material = *rng.weighted(if ceremonial {
        &[(Leather, 2), (Wood, 1), (Bone, 1)][..]
    } else if sh.material == Wood {
        &[(Wood, 3), (Leather, 3), (Bone, 1)]
    } else {
        &[(Leather, 4), (Wood, 2), (Bone, 1)]
    });
    w.on = g.on && rng.chance(0.8);
    w.turns = rng.int(8, 24);
    w.cord = rng.range_f32(0.02, 0.04);
    w.coverage = rng.range_f32(0.7, 1.0);
    w.cross = rng.chance(0.35);
    w.bind = rng.chance(0.7);
    // Ceremonial grips are wound with wire.
    w.material = *rng.weighted(if ceremonial { &[(Gold, 2), (Brass, 1), (Silver, 1)][..] } else { &[(Leather, 4), (Cord, 2)] });
}

fn roll_pommel(rng: &mut Mulberry32, a: MaceArchetype, sh: &Shaft, collar: &Collar, head: &Head, gem: &HeadGem, p: &mut Pommel) {
    use MaceArchetype as A;
    use PommelStyle::*;
    let ceremonial = a == A::Ceremonial;
    p.on = ceremonial || rng.chance(0.85);
    p.style = *rng.weighted(match a {
        A::Ceremonial => &[(Ball, 3), (Disc, 2), (Flared, 2), (Wheel, 2)][..],
        A::Morningstar => &[(Ball, 4), (Flared, 3), (Disc, 2), (Ferrule, 2), (Wheel, 1), (Spike, 1)],
        _ => &[(Ball, 4), (Flared, 3), (Disc, 2), (Ferrule, 2), (Wheel, 1)],
    });
    (p.length, p.radius) = match p.style {
        Ferrule => (rng.range_f32(0.5, 0.8), sh.radius * rng.range_f32(1.05, 1.25)),
        Disc => (rng.range_f32(0.2, 0.35), sh.radius * rng.range_f32(1.5, 2.0)),
        // The wheel stands on edge, so its length spans the whole disc.
        Wheel => (rng.range_f32(0.45, 0.75), sh.radius * rng.range_f32(1.5, 2.0)),
        Spike => (rng.range_f32(0.4, 0.7), sh.radius * rng.range_f32(1.1, 1.4)),
        _ => (rng.range_f32(0.3, 0.55), sh.radius * rng.range_f32(1.3, 1.9)),
    };
    p.facets = if collar.on { collar.facets } else { rng.int(8, 24) };
    p.material = if collar.on {
        collar.material
    } else if is_metal(head.material) {
        head.material
    } else {
        Iron
    };
    let set = matches!(p.style, Ball | Flared | Disc | Wheel) && rng.chance(if ceremonial { 0.6 } else { 0.12 });
    p.gem = if !set {
        PommelGem::None
    } else if p.style.has_face() && rng.chance(0.5) {
        PommelGem::Ring
    } else {
        PommelGem::EndStone
    };
    p.gem_count = rng.int(4, 8);
    p.gem_size = match p.gem {
        PommelGem::Ring => rng.range_f32(0.03, 0.05),
        _ => rng.range_f32(0.05, 0.1).min(p.radius * 0.5),
    };
    let random_stone = *rng.pick(&Gemstone::ALL);
    p.gem_stone = if gem.on { gem.stone } else { random_stone };
}
