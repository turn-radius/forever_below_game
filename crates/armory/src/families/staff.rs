//! Staves: a long haft with a grip near the middle, a butt cap, and a crowning head that
//! may cradle a stone in prongs or carry a floating orb.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (`SECTIONS`,
//! `RANGES.staff`, `PRESETS.staff`, the staff branch of `randomize()`, the head gem placement
//! in `build()`, and `forgeNoun()`). Everything but the stone is built by the shared hafted
//! and head builders, exactly as for maces.
//!
//! Departures from the prototype:
//! - The roll composes a weighted [`StaffArchetype`] (seer's, orb, wanderer's, quarterstaff,
//!   iron-shod) instead of nudging a random preset, so gnarled roots cradle shards, iron caps
//!   sit on straight quarterstaves, and every head style appears.
//! - The noun reads the state rather than a seeded pick: a stone floating in its prongs makes
//!   a Seer's Staff, a bright stone gripped by them a Focus Staff, any other a Gemmed Staff.
//! - A stone that does not float is gripped where the claws close on its girdle (or set in a
//!   bezel), never left hanging in mid-air; a floating one stays inside the cradle.

use crate::build::gem::{bezel, cut_for, cut_stone, stone_reach};
use crate::build::hafted::{Haft, butt_radius, grip_mesh, grip_span, pommel_mesh, shaft_bands, turned, wrap_mesh};
use crate::build::head::{HeadAssembly, HeadSections, head_assembly};
use crate::build::parts::Parts;
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::gem::{GemCut, Gemstone};
use crate::geom::{ItemMesh, LatheOptions, MeshBuilder, Shading, lathe, spline_profile};
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::hafted::{Grip, GripLimits, Pommel, PommelGem, PommelLimits, PommelStyle, Shaft, ShaftLimits, Wrap, WrapLimits};
use crate::sections::head::{
    Collar, CollarLimits, Finial, FinialLimits, FinialStyle, FlangeShape, Flanges, FlangesLimits, Head, HeadGem, HeadGemLimits,
    HeadLimits, HeadStyle, Hemisphere, Prongs, ProngsLimits, SpikePattern, Spikes, SpikesLimits,
};
use crate::slider::Sliders;
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Bone, Brass, Cord, Gold, Iron, Leather, Silver, Wood};

/// The staff generator.
pub struct Staff;

/// Every parameter of one staff.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StaffState {
    pub seed: u32,
    pub shaft: Shaft,
    pub grip: Grip,
    pub wrap: Wrap,
    /// The butt cap.
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

impl Default for StaffState {
    /// The prototype's `BASE` state for the staff sections.
    fn default() -> Self {
        StaffState {
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

/// Slider limits for every staff section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StaffLimits {
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

/// The prototype's slider ranges with `RANGES.staff` applied.
pub const LIMITS: StaffLimits = StaffLimits {
    shaft: ShaftLimits {
        length: ShaftLimits::SLIDERS.length.narrowed(10.0, 20.0),
        radius: ShaftLimits::SLIDERS.radius.narrowed(0.1, 0.25),
        ..ShaftLimits::SLIDERS
    },
    grip: GripLimits { length: GripLimits::SLIDERS.length.narrowed(1.0, 3.5), ..GripLimits::SLIDERS },
    wrap: WrapLimits::SLIDERS,
    pommel: PommelLimits::SLIDERS,
    collar: CollarLimits::SLIDERS,
    head: HeadLimits::SLIDERS,
    flanges: FlangesLimits::SLIDERS,
    spikes: SpikesLimits::SLIDERS,
    prongs: ProngsLimits::SLIDERS,
    finial: FinialLimits::SLIDERS,
    gem: HeadGemLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl StaffState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &StaffLimits) {
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

    /// The prototype's "Forge new" nudge: each slider moves with probability `odds` by up to
    /// `spread / 2` of its range.
    fn nudge(&mut self, rng: &mut Mulberry32, limits: &StaffLimits, odds: f64, spread: f32) {
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
    pub fn out_of_range(&self, limits: &StaffLimits) -> Vec<String> {
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

/// The kinds of staff "Forge new" composes. Not stored: the noun reads the state itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaffArchetype {
    /// Prongs (or roots of the shaft itself) cradling a stone.
    Seer,
    /// A stone with no cradle: in a cup, set on the crown, or floating over a finial.
    Orb,
    /// A gnarled, knotted wooden staff with a bare or knobbed top.
    Wanderer,
    /// A straight, even haft, often capped and shod at both ends.
    Quarterstaff,
    /// A quarterstaff ringed with iron bands, with an iron cap that may carry spikes.
    IronShod,
}

impl StaffArchetype {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(StaffArchetype, u32); 5] = [
        (StaffArchetype::Seer, 32),
        (StaffArchetype::Orb, 18),
        (StaffArchetype::Wanderer, 18),
        (StaffArchetype::Quarterstaff, 17),
        (StaffArchetype::IronShod, 15),
    ];
}

impl Generator for Staff {
    type State = StaffState;

    fn roll(seed: u32) -> StaffState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let archetype = *rng.weighted(&StaffArchetype::WEIGHTS);
        compose(&mut rng, seed, archetype)
    }

    /// `forgeNoun()` for staves, with the seeded pick between the cradled names replaced by
    /// what the cradle holds.
    fn noun(s: &StaffState) -> String {
        let noun = if s.prongs.on && s.gem.on {
            if s.gem.bob {
                "Seer's Staff"
            } else if s.gem.glow >= 0.6 {
                "Focus Staff"
            } else {
                "Gemmed Staff"
            }
        } else if s.gem.on {
            "Orb Staff"
        } else if s.shaft.gnarl > 0.4 {
            "Wanderer's Staff"
        } else if s.shaft.bands > 2 {
            "Iron-shod Quarterstaff"
        } else {
            "Quarterstaff"
        };
        noun.to_string()
    }

    /// The handle from [`crate::build::hafted`] with the grip mid-shaft and the pommel as a
    /// butt cap, then [`head_assembly`] on top with the stone seated in it.
    fn build(s: &StaffState) -> ItemMesh {
        let mut parts = Parts::new();

        let span = grip_span(&s.shaft, &s.grip);
        let mut haft = Haft::new(&s.shaft, s.seed);
        let held = s.grip.on || s.wrap.on;
        if held {
            haft.clear(span.0, span.1);
        }
        let neck = haft.outer_radius(1.0);
        let mut shaft = haft.mesh();
        if s.head.style == HeadStyle::None && !s.collar.on {
            // A bare top is rounded off, not sawn flat.
            let mut end = end_dome(neck);
            end.place(haft.point(1.0), haft.tangent(1.0));
            shaft.append(&end);
        }
        parts.add("shaft", shaft, s.shaft.material);
        let avoid: &[(f32, f32)] = if held { &[span] } else { &[] };
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

        let sections = HeadSections {
            collar: &s.collar,
            head: &s.head,
            flanges: &s.flanges,
            spikes: &s.spikes,
            prongs: &s.prongs,
            finial: &s.finial,
        };
        let mut head = head_assembly(sections, neck, s.seed);
        if s.gem.on {
            seat_stone(&mut head, s);
        }
        head.parts.place(haft.point(1.0), haft.tangent(1.0));
        parts.extend(head.parts);
        parts.finish()
    }
}

/// A rounded end for a shaft of radius `r`, built upright on y = 0.
fn end_dome(r: f32) -> MeshBuilder {
    let control = [[r * 1.0, -0.03], [r * 0.97, r * 0.25], [r * 0.7, r * 0.55], [0.0, r * 0.7]];
    lathe(&spline_profile(&control, 12), &LatheOptions { facets: 28, shading: Shading::SMOOTH, cap_start: false, cap_end: false, phase: 0.0 })
}

/// Seats the head stone (the prototype's "Head gem" placement for staves): held in or
/// floating inside the prongs' cradle, resting in a cup, floating over the crown, or set on
/// it in a bezel.
fn seat_stone(head: &mut HeadAssembly, s: &StaffState) {
    let size = s.gem.size;
    let cut = cut_for(s.gem.stone, s.gem.cut);
    let (down, up) = stone_reach(cut, size);
    let surface = &head.surface;
    let lh = surface.length();
    let mut setting = false;
    let girdle = match &head.cradle {
        Some(c) => {
            // The claws close on the girdle; a stone too big for them rests on their tips.
            let held = c.seat(size).unwrap_or(c.tip() - up * 0.3);
            if s.gem.bob {
                // Floats between the root of the prongs and where they would close on it.
                let low = c.profile[0][1] + down + size * 0.15;
                (low + (held - low) * s.gem.lift).min(held)
            } else {
                held
            }
        }
        None if s.head.style == HeadStyle::Cup && s.finial.style == FinialStyle::None => {
            // Rests in the cup, its foot on the cup's floor.
            (surface.base + lh * 0.8 + down * 0.85).max(surface.top - lh * 0.1)
        }
        None if s.gem.bob => head.crown + down + size * (0.35 + 0.9 * s.gem.lift),
        None => {
            setting = true;
            head.crown + size * 0.25
        }
    };
    let mut stone = cut_stone(cut, size);
    stone.translate([0.0, girdle, 0.0]);
    head.parts.stone("gem", stone, s.gem.stone, s.gem.glow, true);
    if setting {
        let (sides, shading) = turned(s.head.facets.max(8));
        let mut mount = bezel(size, sides, shading);
        mount.translate([0.0, girdle, 0.0]);
        let metal = if is_metal(s.head.material) && s.head.style != HeadStyle::None { s.head.material } else { Brass };
        head.parts.add("bezel", mount, metal);
    }
}

impl Staff {
    /// "Forge new" forced to one archetype, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, a)` when `a` is what `s` rolls.
    pub fn roll_as(seed: u32, archetype: StaffArchetype) -> StaffState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, archetype)
    }
}

fn is_metal(m: MaterialKind) -> bool {
    matches!(m, Iron | Brass | Gold | Silver)
}

/// A trim metal for fittings on `m`: brass on wood, bone and iron, gold on the precious metals.
fn trim_for(m: MaterialKind) -> MaterialKind {
    match m {
        Brass | Gold | Silver => Gold,
        _ => Brass,
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, a: StaffArchetype) -> StaffState {
    use StaffArchetype as A;
    let mut s = StaffState { seed, ..StaffState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    // The prototype's generic pass: most sliders drift up to 15% of their range from base.
    s.nudge(rng, &LIMITS, 0.75, 0.3);
    s.flanges.on = false;
    s.spikes.on = false;
    s.prongs.on = false;
    s.gem.on = false;

    roll_shaft(rng, a, &mut s.shaft);
    roll_head(rng, a, &s.shaft, &mut s.head);
    let head = s.head;

    match a {
        A::Seer => {
            roll_gem(rng, &mut s.gem, &[
                (GemCut::Brilliant, 3),
                (GemCut::Orb, 3),
                (GemCut::Shard, 3),
                (GemCut::Rose, 1),
                (GemCut::Octahedron, 1),
                (GemCut::Cabochon, 1),
            ]);
            s.gem.size = rng.range_f32(0.24, 0.48);
            roll_prongs(rng, &s.shaft, &head, &s.gem, &mut s.prongs);
            roll_fan_flanges(rng, &head, &mut s.flanges);
        }
        A::Orb => {
            roll_gem(rng, &mut s.gem, &[(GemCut::Orb, 5), (GemCut::Brilliant, 2), (GemCut::Cabochon, 1), (GemCut::Shard, 1)]);
            s.gem.size = if head.style == HeadStyle::Cup { head.radius * rng.range_f32(0.7, 0.9) } else { rng.range_f32(0.24, 0.5) };
            roll_fan_flanges(rng, &head, &mut s.flanges);
        }
        A::IronShod if rng.chance(0.3) => {
            // Short spikes round the iron cap.
            let sp = &mut s.spikes;
            sp.on = true;
            sp.count = rng.int(4, 8);
            sp.rings = rng.int(1, 2);
            sp.pattern = *rng.weighted(&[(SpikePattern::Aligned, 2), (SpikePattern::Staggered, 2)]);
            sp.spread = rng.range_f32(0.3, 0.6);
            sp.bias = 0.0;
            sp.hemisphere = Hemisphere::Both;
            sp.twist = 0.0;
            sp.phase = rng.range_f32(0.0, 360.0);
            sp.jitter = 0.0;
            sp.tilt = 0.0;
            sp.falloff = rng.range_f32(0.1, 0.3);
            sp.length = head.radius * rng.range_f32(0.6, 1.0);
            sp.base = sp.length * rng.range_f32(0.22, 0.32);
            sp.sides = *rng.pick(&[4, 4, 6]);
            sp.concave = rng.range_f32(0.0, 0.4);
            sp.material = Iron;
        }
        _ => {}
    }

    roll_finial(rng, a, &head, &s.gem, &mut s.finial);
    if s.gem.on && s.finial.style == FinialStyle::None && head.style == HeadStyle::Cup && !s.prongs.on {
        // A stone in a cup sits in it; the float would lift it out.
        s.gem.bob = false;
    }
    roll_collar(rng, a, &head, &mut s.collar);
    roll_grip_and_wrap(rng, a, &s.shaft, &mut s.grip, &mut s.wrap);
    roll_pommel(rng, a, &s.shaft, &s.collar, &head, &s.gem, &mut s.pommel);

    let mut targets = vec![(EtchTarget::Wood, 2)];
    if is_metal(head.material) || s.collar.on || s.pommel.on {
        targets.push((EtchTarget::Metals, 3));
    }
    if s.shaft.material == Bone {
        targets.push((EtchTarget::Bone, 2));
    }
    let gemmed = s.gem.on;
    s.etch = Etching::roll(rng, if gemmed { 0.25 } else { 0.15 }, &targets, if gemmed { 2 } else { 1 });
    if s.etch.target == EtchTarget::Wood && s.shaft.material != Wood {
        s.etch.target = EtchTarget::Metals;
    }

    s.fit(&LIMITS);
    s
}

fn roll_shaft(rng: &mut Mulberry32, a: StaffArchetype, sh: &mut Shaft) {
    use StaffArchetype as A;
    sh.material = *rng.weighted(match a {
        A::Seer => &[(Wood, 9), (Bone, 1)][..],
        A::Orb => &[(Wood, 7), (Bone, 1), (Silver, 1)],
        _ => &[(Wood, 1)],
    });
    sh.length = match a {
        A::Seer | A::Orb => rng.range_f32(13.0, 18.0),
        A::Wanderer => rng.range_f32(13.0, 17.5),
        A::Quarterstaff | A::IronShod => rng.range_f32(14.0, 19.0),
    };
    sh.radius = match (a, sh.material) {
        (_, Bone) => rng.range_f32(0.12, 0.15),
        (_, Silver) => rng.range_f32(0.1, 0.13),
        (A::Quarterstaff | A::IronShod, _) => rng.range_f32(0.14, 0.18),
        _ => rng.range_f32(0.13, 0.2),
    };
    sh.taper = match a {
        A::Wanderer => rng.range_f32(1.05, 1.35),
        A::Seer | A::Orb => rng.range_f32(0.9, 1.25),
        _ => rng.range_f32(0.95, 1.05),
    };
    sh.gnarl = match (a, sh.material) {
        (_, Bone | Silver) => 0.0,
        (A::Wanderer, _) => rng.range_f32(0.45, 1.0),
        (A::Seer, _) if rng.chance(0.5) => rng.range_f32(0.2, 0.95),
        (A::Seer | A::Orb, _) => rng.range_f32(0.0, 0.15),
        _ => rng.range_f32(0.0, 0.06),
    };
    sh.gnarl_freq = rng.range_f32(1.5, 4.5);
    sh.bands = match (a, sh.material) {
        (A::IronShod, _) => rng.int(3, 8),
        (A::Quarterstaff | A::Orb, Wood) => rng.int(0, 2),
        _ => 0,
    };
    sh.band_size = rng.range_f32(0.02, 0.04);
}

fn roll_head(rng: &mut Mulberry32, a: StaffArchetype, sh: &Shaft, head: &mut Head) {
    use HeadStyle::*;
    use StaffArchetype as A;
    head.style = *rng.weighted(match a {
        A::Seer => &[(Cup, 4), (Onion, 3), (Orb, 2), (None, 3)][..],
        A::Orb => &[(Cup, 3), (Pear, 2), (Orb, 2), (Onion, 1), (None, 1)],
        A::Wanderer => &[(None, 5), (Onion, 2), (Orb, 2)],
        A::Quarterstaff => &[(Drum, 4), (None, 3)],
        A::IronShod => &[(Drum, 4), (Orb, 1), (Pear, 1)],
    });
    head.material = match a {
        A::Seer | A::Orb => *rng.weighted(&[(Wood, 5), (Brass, 2), (Silver, 1), (Gold, 1), (Bone, 1)]),
        A::Wanderer => Wood,
        A::Quarterstaff => *rng.weighted(&[(Iron, 3), (Brass, 1)]),
        A::IronShod => Iron,
    };
    if head.material == Wood && sh.material != Wood {
        head.material = trim_for(sh.material);
    }
    let top = sh.radius * sh.taper;
    head.radius = match (a, head.style) {
        // A cap fits over the end of the haft.
        (A::Quarterstaff | A::IronShod, Drum) => top * rng.range_f32(1.12, 1.3),
        (A::IronShod, _) => rng.range_f32(0.24, 0.34),
        (_, Cup) => rng.range_f32(0.28, 0.45),
        (A::Wanderer, _) => (top * rng.range_f32(1.4, 1.9)).max(0.2),
        _ => rng.range_f32(0.22, 0.38),
    };
    let aspect = match head.style {
        Orb => rng.range_f32(0.95, 1.15),
        Pear => rng.range_f32(1.3, 1.7),
        Drum => rng.range_f32(1.4, 2.4),
        Onion => rng.range_f32(1.3, 1.8),
        Cup => rng.range_f32(0.75, 1.0),
        None => 1.0,
    };
    head.length = (head.radius * 2.0 * aspect).max(0.3);
    head.belly = match head.style {
        Pear => rng.range_f32(0.55, 0.75),
        Onion => rng.range_f32(0.3, 0.42),
        Cup => rng.range_f32(0.5, 0.7),
        _ => rng.range_f32(0.45, 0.55),
    };
    head.facets = match (head.style, head.material) {
        (Drum, _) => *rng.pick(&[6, 8, 8, 24]),
        (_, Wood | Bone) => rng.int(7, 12),
        _ => rng.int(16, 32),
    };
}

/// A stone whose cut suits it.
fn roll_gem(rng: &mut Mulberry32, gem: &mut HeadGem, cuts: &[(GemCut, u32)]) {
    gem.on = true;
    gem.cut = *rng.weighted(cuts);
    let stones: Vec<Gemstone> = Gemstone::ALL.into_iter().filter(|g| g.suits(gem.cut)).collect();
    gem.stone = *rng.pick(&stones);
    gem.glow = rng.range_f32(0.3, 0.9);
    gem.lift = rng.range_f32(0.3, 0.8);
    gem.bob = rng.chance(0.55);
}

/// Prongs sized to the stone: tall enough to close over it, spread round it. On a bare top
/// they are the shaft's own roots.
fn roll_prongs(rng: &mut Mulberry32, sh: &Shaft, head: &Head, gem: &HeadGem, p: &mut Prongs) {
    let roots = head.style == HeadStyle::None;
    let wooden = roots || matches!(head.material, Wood | Bone);
    p.on = true;
    p.count = *rng.weighted(if roots { &[(3, 2), (4, 3), (5, 3), (6, 2)][..] } else { &[(3, 3), (4, 4), (5, 2), (6, 1)] });
    p.height = (gem.size * rng.range_f32(2.4, 3.6)).clamp(0.7, 2.6);
    p.base = rng.range_f32(0.7, 1.0);
    // The cage's widest point stands a little off the stone, whatever the prongs root on:
    // their root circle is the head's radius where they leave it (see `prongs_mesh`).
    let at_root = match head.style {
        HeadStyle::None => sh.radius * sh.taper * 0.8,
        HeadStyle::Cup => head.radius,
        HeadStyle::Onion => head.radius * 0.45,
        _ => head.radius * 0.7,
    };
    let root = at_root * p.base;
    p.spread = (gem.size * rng.range_f32(1.25, 1.6) - root).max(0.06);
    p.curl = rng.range_f32(0.55, 0.95);
    p.thickness = if wooden { rng.range_f32(0.045, 0.075) } else { rng.range_f32(0.03, 0.055) };
    p.twist = if roots || rng.chance(0.4) { rng.range_f32(0.1, 0.45) * if rng.chance(0.5) { 1.0 } else { -1.0 } } else { 0.0 };
    p.phase = rng.range_f32(0.0, 360.0);
    p.material = if roots { sh.material } else { head.material };
}

/// Small fan flanges under a metal head, now and then.
fn roll_fan_flanges(rng: &mut Mulberry32, head: &Head, f: &mut Flanges) {
    if !is_metal(head.material) || head.style == HeadStyle::None || !rng.chance(0.2) {
        return;
    }
    *f = Flanges {
        on: true,
        count: rng.int(5, 8),
        phase: 0.0,
        height: rng.range_f32(0.4, 0.6),
        bias: rng.range_f32(-0.15, 0.0),
        shape: *rng.weighted(&[(FlangeShape::Fan, 3), (FlangeShape::Leaf, 1)]),
        depth: rng.range_f32(0.1, 0.2),
        thickness: rng.range_f32(0.025, 0.04),
        notch: 0.0,
        pierce: false,
        lean: 0.0,
        material: head.material,
    };
}

fn roll_finial(rng: &mut Mulberry32, a: StaffArchetype, head: &Head, gem: &HeadGem, f: &mut Finial) {
    use FinialStyle::*;
    use StaffArchetype as A;
    f.style = *rng.weighted(match a {
        // The stone crowns the cradle.
        A::Seer => &[(None, 1)][..],
        A::Orb if head.style == HeadStyle::Cup => &[(None, 1)],
        A::Orb => &[(Knob, 2), (Spire, 2), (None, 2)],
        A::Wanderer => &[(None, 3), (Knob, 1)],
        A::Quarterstaff => &[(None, 4), (Knob, 1)],
        A::IronShod => &[(None, 2), (Spike, 2), (Knob, 1)],
    });
    match f.style {
        Spike => {
            f.length = rng.range_f32(0.3, 0.55);
            f.radius = rng.range_f32(0.06, 0.1);
            f.sides = 4;
            f.material = Iron;
        }
        Knob => {
            f.length = rng.range_f32(0.18, 0.32);
            f.radius = rng.range_f32(0.07, 0.12);
            f.sides = rng.int(8, 16);
            f.material = if is_metal(head.material) { head.material } else { trim_for(head.material) };
        }
        Spire => {
            f.length = rng.range_f32(0.35, 0.6);
            f.radius = rng.range_f32(0.05, 0.09);
            f.sides = *rng.pick(&[4, 6, 8]);
            f.material = if is_metal(head.material) { head.material } else { trim_for(head.material) };
        }
        None => {}
    }
    // A finial carrying a stone is wide enough to hold it.
    if gem.on && f.style != None {
        f.radius = f.radius.max(gem.size * 0.35);
    }
}

fn roll_collar(rng: &mut Mulberry32, a: StaffArchetype, head: &Head, c: &mut Collar) {
    use StaffArchetype as A;
    c.on = rng.chance(match a {
        A::Seer => 0.7,
        A::Orb => 0.85,
        A::Wanderer => 0.25,
        A::Quarterstaff => 0.5,
        A::IronShod => 0.8,
    });
    c.bands = rng.int(1, 3);
    c.height = if c.bands == 1 { rng.range_f32(0.12, 0.25) } else { rng.range_f32(0.05, 0.12) };
    c.radius = rng.range_f32(1.15, 1.5);
    c.facets = if head.style == HeadStyle::None { rng.int(8, 24) } else { head.facets };
    c.material = if is_metal(head.material) && head.style != HeadStyle::None {
        if rng.chance(0.6) { head.material } else { trim_for(head.material) }
    } else {
        *rng.weighted(&[(Brass, 3), (Iron, 2), (Silver, 1), (Gold, 1)])
    };
}

fn roll_grip_and_wrap(rng: &mut Mulberry32, a: StaffArchetype, sh: &Shaft, g: &mut Grip, w: &mut Wrap) {
    use StaffArchetype as A;
    let fine = matches!(a, A::Seer | A::Orb);
    g.on = rng.chance(match a {
        A::Seer => 0.6,
        A::Orb => 0.7,
        A::Wanderer => 0.3,
        A::Quarterstaff | A::IronShod => 0.4,
    });
    g.position = rng.range_f32(0.35, 0.6);
    g.length = rng.range_f32(1.5, 3.0);
    g.thickness = rng.range_f32(1.05, 1.2);
    g.rivets = 0;
    g.material = *rng.weighted(if sh.material == Wood { &[(Leather, 4), (Wood, 2), (Bone, 1)][..] } else { &[(Leather, 4), (Wood, 1)] });
    w.on = rng.chance(if g.on { 0.75 } else { 0.6 });
    w.cord = rng.range_f32(0.02, 0.045);
    w.coverage = rng.range_f32(0.6, 1.0);
    // Wound tight, turn against turn (a leather strap overlaps a little more).
    let wound = g.length * w.coverage;
    w.cord = w.cord.max(wound / (48.0 * 2.1));
    w.turns = ((wound / (w.cord * 2.1)).round() as u32).clamp(4, 48);
    w.cross = rng.chance(0.35);
    if w.cross {
        // Two windings cross each other, each half as close.
        w.turns = (w.turns / 2).max(3);
    }
    w.bind = rng.chance(0.6);
    w.material = *rng.weighted(if fine { &[(Leather, 4), (Cord, 2), (Gold, 1), (Brass, 1)][..] } else { &[(Leather, 4), (Cord, 3)] });
}

fn roll_pommel(rng: &mut Mulberry32, a: StaffArchetype, sh: &Shaft, collar: &Collar, head: &Head, gem: &HeadGem, p: &mut Pommel) {
    use PommelStyle::*;
    use StaffArchetype as A;
    p.on = rng.chance(match a {
        A::Seer => 0.75,
        A::Orb | A::Quarterstaff => 0.85,
        A::Wanderer => 0.4,
        A::IronShod => 1.0,
    });
    p.style = *rng.weighted(&[(Ferrule, 6), (Spike, 2), (Ball, 1), (Flared, 1)]);
    (p.length, p.radius) = match p.style {
        Ferrule => (rng.range_f32(0.5, 0.9), sh.radius * rng.range_f32(1.05, 1.2)),
        Spike => (rng.range_f32(0.5, 0.9), sh.radius * rng.range_f32(1.1, 1.3)),
        _ => (rng.range_f32(0.25, 0.45), sh.radius * rng.range_f32(1.3, 1.6)),
    };
    p.facets = if collar.on { collar.facets } else { rng.int(8, 24) };
    p.material = if collar.on {
        collar.material
    } else if is_metal(head.material) {
        head.material
    } else {
        *rng.weighted(&[(Iron, 2), (Brass, 1)])
    };
    let set = gem.on && matches!(p.style, Ball | Flared) && rng.chance(0.3);
    p.gem = if set { PommelGem::EndStone } else { PommelGem::None };
    p.gem_count = rng.int(4, 8);
    p.gem_size = rng.range_f32(0.04, 0.08).min(p.radius * 0.5);
    p.gem_stone = gem.stone;
}
