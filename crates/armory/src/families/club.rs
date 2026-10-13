//! Clubs: a carved wooden shaft swelling into a heavy striking end, or a thinner shaft with a
//! knob, sometimes studded, spiked, set with teeth or bound in iron.
//!
//! Not an Armory family in the prototype. Its plate (`reference/forever-below-plate.html`)
//! faked the "Club" base (`data/bases.json`, materials Oak, Ash and Bone-studded) as a wobbly
//! swept tube swelling toward the top, with seven bone cones for the bone-studded kind. Here
//! it is built from the shared hafted and head code:
//! - The shaft is a [`Haft`] with a [`Swell`]: one piece of wood swelling into the striking
//!   end and closing in a dome, so knots, iron bands and studs all follow its surface.
//! - Knobbed clubs and shillelaghs carry a turned [`Head`] knob instead of a swell.
//! - Studs, nails and teeth are [`Spikes`] driven into the swelling with
//!   [`spikes_on_haft`] (or into the knob with [`spikes_mesh`]).
//! - The roll composes a weighted [`ClubArchetype`] so every noun comes up with a club that
//!   fits it; the generic nudge varies whatever the archetype leaves alone.

use crate::build::hafted::{Haft, bands_over, butt_radius, grip_mesh, grip_span, pommel_mesh, wrap_mesh};
use crate::build::head::{head_mesh, spikes_mesh, spikes_on_haft};
use crate::build::parts::Parts;
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::geom::ItemMesh;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::hafted::{
    Grip, GripLimits, Pommel, PommelGem, PommelLimits, PommelStyle, Shaft, ShaftLimits, Swell, SwellLimits, Wrap, WrapLimits,
};
use crate::sections::head::{Head, HeadLimits, HeadStyle, Hemisphere, SpikePattern, Spikes, SpikesLimits};
use crate::slider::Sliders;
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Bone, Brass, Cord, Iron, Leather, Wood};

/// The club generator.
pub struct Club;

/// Every parameter of one club.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClubState {
    pub seed: u32,
    /// The shaft. Its bands are iron bands round the striking end.
    pub shaft: Shaft,
    /// The shaft's swelling into the striking end.
    pub swell: Swell,
    pub grip: Grip,
    pub wrap: Wrap,
    /// A knob or flare at the butt.
    pub pommel: Pommel,
    /// A turned knob on the top of the shaft ([`HeadStyle::None`] for none).
    pub head: Head,
    /// Studs, nails or teeth driven into the striking end.
    pub spikes: Spikes,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for ClubState {
    /// A plain wooden club, swelling over its upper half.
    fn default() -> Self {
        ClubState {
            seed: 0,
            shaft: Shaft { length: 4.5, radius: 0.13, taper: 1.1, ..Shaft::default() },
            swell: Swell { on: true, ..Swell::default() },
            grip: Grip { on: false, length: 1.3, thickness: 1.1, material: Leather, ..Grip::default() },
            wrap: Wrap { on: false, ..Wrap::default() },
            pommel: Pommel { on: false, style: PommelStyle::Flared, length: 0.25, radius: 0.18, material: Wood, ..Pommel::default() },
            head: Head { style: HeadStyle::None, length: 0.7, radius: 0.3, facets: 24, material: Wood, ..Head::default() },
            spikes: Spikes::default(),
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every club section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClubLimits {
    pub shaft: ShaftLimits,
    pub swell: SwellLimits,
    pub grip: GripLimits,
    pub wrap: WrapLimits,
    pub pommel: PommelLimits,
    pub head: HeadLimits,
    pub spikes: SpikesLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The shared slider ranges, narrowed to one-handed clubs.
pub const LIMITS: ClubLimits = ClubLimits {
    shaft: ShaftLimits {
        length: ShaftLimits::SLIDERS.length.narrowed(2.4, 6.5),
        radius: ShaftLimits::SLIDERS.radius.narrowed(0.08, 0.2),
        ..ShaftLimits::SLIDERS
    },
    swell: SwellLimits::SLIDERS,
    grip: GripLimits { length: GripLimits::SLIDERS.length.narrowed(0.8, 2.0), ..GripLimits::SLIDERS },
    wrap: WrapLimits::SLIDERS,
    pommel: PommelLimits {
        length: PommelLimits::SLIDERS.length.narrowed(0.15, 0.6),
        radius: PommelLimits::SLIDERS.radius.narrowed(0.1, 0.35),
        ..PommelLimits::SLIDERS
    },
    head: HeadLimits {
        length: HeadLimits::SLIDERS.length.narrowed(0.3, 1.4),
        radius: HeadLimits::SLIDERS.radius.narrowed(0.15, 0.6),
        ..HeadLimits::SLIDERS
    },
    spikes: SpikesLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl ClubState {
    /// Clamp and snap every slider into `limits`.
    pub fn fit(&mut self, limits: &ClubLimits) {
        self.shaft.fit(&limits.shaft);
        self.swell.fit(&limits.swell);
        self.grip.fit(&limits.grip);
        self.wrap.fit(&limits.wrap);
        self.pommel.fit(&limits.pommel);
        self.head.fit(&limits.head);
        self.spikes.fit(&limits.spikes);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    /// The prototype's "Forge new" nudge: each slider moves with probability `odds` by up to
    /// `spread / 2` of its range.
    fn nudge(&mut self, rng: &mut Mulberry32, limits: &ClubLimits, odds: f64, spread: f32) {
        self.shaft.nudge(rng, &limits.shaft, odds, spread);
        self.swell.nudge(rng, &limits.swell, odds, spread);
        self.grip.nudge(rng, &limits.grip, odds, spread);
        self.wrap.nudge(rng, &limits.wrap, odds, spread);
        self.pommel.nudge(rng, &limits.pommel, odds, spread);
        self.head.nudge(rng, &limits.head, odds, spread);
        self.spikes.nudge(rng, &limits.spikes, odds, spread);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &ClubLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("shaft", self.shaft.out_of_range(&limits.shaft)),
            tag("swell", self.swell.out_of_range(&limits.swell)),
            tag("grip", self.grip.out_of_range(&limits.grip)),
            tag("wrap", self.wrap.out_of_range(&limits.wrap)),
            tag("pommel", self.pommel.out_of_range(&limits.pommel)),
            tag("head", self.head.out_of_range(&limits.head)),
            tag("spikes", self.spikes.out_of_range(&limits.spikes)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }

    /// Whether the club has a knob on top.
    pub fn has_knob(&self) -> bool {
        self.head.style != HeadStyle::None
    }
}

/// The kinds of club "Forge new" composes. Not stored: the noun reads the state itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClubArchetype {
    /// Short and plain, swelling to a rounded end.
    Cudgel,
    /// A full-length plain club.
    Plain,
    /// A knotted, bent root or branch.
    Gnarled,
    /// A thin shaft with a turned knob (a knobbed club, or a shillelagh when knotted).
    Knobbed,
    /// Short iron studs over the swelling.
    Studded,
    /// Long nails driven through the swelling.
    Spiked,
    /// Teeth or bone points set in the swelling (the plate's "Bone-studded").
    BoneStudded,
    /// Iron bands round the striking end.
    IronBound,
}

impl ClubArchetype {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(ClubArchetype, u32); 8] = [
        (ClubArchetype::Cudgel, 15),
        (ClubArchetype::Plain, 15),
        (ClubArchetype::Gnarled, 12),
        (ClubArchetype::Knobbed, 14),
        (ClubArchetype::Studded, 14),
        (ClubArchetype::Spiked, 10),
        (ClubArchetype::BoneStudded, 10),
        (ClubArchetype::IronBound, 10),
    ];
}

/// Gnarl above this reads as knotted wood.
const GNARLED: f32 = 0.15;
/// Shafts shorter than this are cudgels.
const CUDGEL_LENGTH: f32 = 3.8;
/// Spikes longer than this many times their root are nails rather than studs.
const NAIL: f32 = 2.2;

impl Generator for Club {
    type State = ClubState;

    fn roll(seed: u32) -> ClubState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let archetype = *rng.weighted(&ClubArchetype::WEIGHTS);
        compose(&mut rng, seed, archetype)
    }

    fn noun(s: &ClubState) -> String {
        let noun = if s.spikes.on {
            if s.spikes.material == Bone {
                "Bone-studded Club"
            } else if s.spikes.length > s.spikes.base * NAIL {
                "Spiked Club"
            } else {
                "Studded Club"
            }
        } else if s.has_knob() {
            if s.shaft.gnarl > GNARLED { "Shillelagh" } else { "Knobbed Club" }
        } else if s.shaft.bands > 0 {
            "Iron-bound Club"
        } else if s.shaft.gnarl > GNARLED {
            "Gnarled Club"
        } else if s.shaft.length < CUDGEL_LENGTH {
            "Cudgel"
        } else {
            "Club"
        };
        noun.to_string()
    }

    /// The shaft from [`crate::build::hafted`], swollen into the striking end, with its grip,
    /// wrap and butt; iron bands spaced over the swelling; then a knob from
    /// [`crate::build::head`] on top, and studs driven into the swelling or the knob.
    fn build(s: &ClubState) -> ItemMesh {
        let mut parts = Parts::new();

        let span = grip_span(&s.shaft, &s.grip);
        let mut haft = Haft::new(&s.shaft, s.seed).with_swell(&s.swell);
        let held = s.grip.on || s.wrap.on;
        if held {
            haft.clear(span.0, span.1);
        }
        parts.add("shaft", haft.mesh(), s.shaft.material);
        // The striking end: the swelling, or the top quarter of a plain shaft under a knob.
        let end = haft.swell_span().unwrap_or((0.75, 0.97));
        if s.shaft.bands > 0 {
            let (t0, t1) = (end.0 + (end.1 - end.0) * 0.35, end.1);
            parts.add("bands", bands_over(&haft, s.shaft.bands, s.shaft.band_size, (t0, t1)), Iron);
        }
        if s.grip.on {
            parts.add("grip", grip_mesh(&haft, &s.grip, span), s.grip.material);
        }
        if s.wrap.on {
            parts.add("wrap", wrap_mesh(&haft, &s.grip, span, &s.wrap), s.wrap.material);
        }
        if s.pommel.on {
            let pommel = pommel_mesh(&s.pommel, butt_radius(&haft, &s.grip, span)).placed_on(&haft);
            parts.add("pommel", pommel.body, s.pommel.material);
        }

        if s.has_knob() {
            let mut knob = Parts::new();
            let neck = haft.outer_radius(1.0);
            let (body, surface) = head_mesh(&s.head, -0.03, neck);
            knob.add("head", body, s.head.material);
            if s.spikes.on {
                knob.add("spikes", spikes_mesh(&s.spikes, &surface, s.seed), s.spikes.material);
            }
            knob.place(haft.point(1.0), haft.tangent(1.0));
            parts.extend(knob);
        } else if s.spikes.on {
            // Over the thick of the swelling, clear of where it starts and of the dome.
            let span = (end.0 + (end.1 - end.0) * 0.3, end.1 + (1.0 - end.1) * 0.35);
            parts.add("spikes", spikes_on_haft(&s.spikes, &haft, span, s.seed), s.spikes.material);
        }
        parts.finish()
    }
}

impl Club {
    /// "Forge new" forced to one archetype, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, a)` when `a` is what `s` rolls.
    pub fn roll_as(seed: u32, archetype: ClubArchetype) -> ClubState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, archetype)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, a: ClubArchetype) -> ClubState {
    let mut s = ClubState { seed, ..ClubState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    // The prototype's generic pass: most sliders drift up to 15% of their range from base.
    s.nudge(rng, &LIMITS, 0.75, 0.3);

    roll_shaft(rng, a, &mut s.shaft);
    roll_swell(rng, a, &mut s.swell);
    roll_knob(rng, a, &s.shaft, &mut s.head);
    roll_spikes(rng, a, &mut s.spikes);
    roll_grip_and_wrap(rng, a, &s.shaft, &mut s.grip, &mut s.wrap);
    roll_pommel(rng, a, &s.shaft, &mut s.pommel);

    let mut targets = vec![(EtchTarget::Wood, 3)];
    if s.shaft.bands > 0 || (s.spikes.on && s.spikes.material != Bone) {
        targets.push((EtchTarget::Metals, 1));
    }
    s.etch = Etching::roll(rng, 0.15, &targets, 1);

    s.fit(&LIMITS);
    s
}

fn roll_shaft(rng: &mut Mulberry32, a: ClubArchetype, sh: &mut Shaft) {
    use ClubArchetype as A;
    sh.material = Wood;
    sh.length = match a {
        A::Cudgel => rng.range_f32(2.6, 3.6),
        _ => rng.range_f32(4.0, 5.6),
    };
    sh.radius = match a {
        A::Knobbed => rng.range_f32(0.09, 0.12),
        A::Cudgel => rng.range_f32(0.11, 0.14),
        _ => rng.range_f32(0.12, 0.15),
    };
    sh.taper = match a {
        A::Knobbed => rng.range_f32(0.9, 1.05),
        _ => rng.range_f32(1.05, 1.25),
    };
    let knotted = a == A::Gnarled || (a == A::Knobbed && rng.chance(0.4));
    sh.gnarl = if knotted { rng.range_f32(0.25, 0.6) } else { rng.range_f32(0.0, 0.08) };
    sh.gnarl_freq = rng.range_f32(1.2, 3.0);
    sh.bands = if a == A::IronBound { rng.int(2, 4) } else { 0 };
    sh.band_size = rng.range_f32(0.03, 0.05);
}

fn roll_swell(rng: &mut Mulberry32, a: ClubArchetype, sw: &mut Swell) {
    use ClubArchetype as A;
    sw.on = a != A::Knobbed;
    sw.start = rng.range_f32(0.3, 0.5);
    sw.girth = match a {
        A::Cudgel => rng.range_f32(1.5, 2.1),
        A::Plain => rng.range_f32(2.0, 2.8),
        A::Gnarled => rng.range_f32(1.6, 2.4),
        _ => rng.range_f32(1.9, 2.6),
    };
    sw.peak = rng.range_f32(0.5, 1.0);
    sw.dome = rng.range_f32(0.3, 1.0);
}

fn roll_knob(rng: &mut Mulberry32, a: ClubArchetype, sh: &Shaft, h: &mut Head) {
    use HeadStyle::*;
    if a != ClubArchetype::Knobbed {
        h.style = None;
        return;
    }
    h.style = *rng.weighted(&[(Orb, 5), (Pear, 2), (Drum, 1)]);
    h.material = sh.material;
    h.radius = rng.range_f32(0.22, 0.4);
    let aspect = match h.style {
        Pear => rng.range_f32(1.2, 1.5),
        Drum => rng.range_f32(0.9, 1.2),
        _ => rng.range_f32(0.9, 1.1),
    };
    h.length = h.radius * 2.0 * aspect;
    h.belly = match h.style {
        Pear => rng.range_f32(0.55, 0.7),
        _ => rng.range_f32(0.45, 0.55),
    };
    h.facets = rng.int(20, 28);
}

fn roll_spikes(rng: &mut Mulberry32, a: ClubArchetype, sp: &mut Spikes) {
    use ClubArchetype as A;
    sp.on = matches!(a, A::Studded | A::Spiked | A::BoneStudded);
    sp.count = rng.int(5, 8);
    sp.rings = rng.int(3, 5);
    sp.pattern = *rng.weighted(match a {
        A::BoneStudded => &[(SpikePattern::GoldenAngle, 3), (SpikePattern::Staggered, 2), (SpikePattern::Spiral, 1)][..],
        _ => &[(SpikePattern::Staggered, 4), (SpikePattern::Aligned, 2), (SpikePattern::Spiral, 1), (SpikePattern::GoldenAngle, 1)],
    });
    sp.spread = rng.range_f32(0.65, 0.95);
    sp.bias = rng.range_f32(0.0, 0.1);
    sp.hemisphere = *rng.weighted(&[(Hemisphere::Both, 5), (Hemisphere::Upper, 1)]);
    sp.twist = if sp.pattern == SpikePattern::Spiral { rng.range_f32(15.0, 40.0) } else { 0.0 };
    sp.phase = rng.range_f32(0.0, 360.0);
    sp.falloff = rng.range_f32(0.0, 0.2);
    match a {
        A::Spiked => {
            // Nails driven through, hammered in a little crooked.
            sp.base = rng.range_f32(0.025, 0.04);
            sp.length = rng.range_f32(0.15, 0.3);
            sp.sides = *rng.pick(&[4, 6]);
            sp.concave = rng.range_f32(0.2, 0.5);
            sp.jitter = rng.range_f32(0.05, 0.25);
            sp.tilt = rng.range_f32(-10.0, 10.0);
            sp.material = Iron;
        }
        A::BoneStudded => {
            // Teeth and bone points, curved and irregular.
            sp.count = rng.int(4, 7);
            sp.base = rng.range_f32(0.035, 0.06);
            sp.length = rng.range_f32(0.12, 0.25);
            sp.sides = rng.int(6, 10);
            sp.concave = rng.range_f32(0.4, 0.8);
            sp.jitter = rng.range_f32(0.1, 0.3);
            sp.tilt = rng.range_f32(-15.0, 15.0);
            sp.material = Bone;
        }
        _ => {
            // Short domed studs.
            sp.base = rng.range_f32(0.045, 0.07);
            sp.length = sp.base * rng.range_f32(0.9, 1.4);
            sp.sides = *rng.pick(&[8, 12, 16]);
            sp.concave = rng.range_f32(0.0, 0.2);
            sp.jitter = rng.range_f32(0.0, 0.1);
            sp.tilt = 0.0;
            sp.material = *rng.weighted(&[(Iron, 4), (Brass, 1)]);
        }
    }
}

fn roll_grip_and_wrap(rng: &mut Mulberry32, a: ClubArchetype, sh: &Shaft, g: &mut Grip, w: &mut Wrap) {
    use ClubArchetype as A;
    let armed = matches!(a, A::Studded | A::Spiked | A::IronBound);
    g.on = rng.chance(if armed { 0.6 } else { 0.35 });
    g.position = 0.0;
    g.length = rng.range_f32(1.0, 1.6).min(sh.length * 0.4);
    g.thickness = rng.range_f32(1.05, 1.2);
    g.rivets = 0;
    g.material = *rng.weighted(&[(Leather, 3), (Wood, 1)]);
    w.on = rng.chance(if g.on { 0.6 } else { 0.4 });
    w.coverage = rng.range_f32(0.7, 1.0);
    w.material = *rng.weighted(&[(Leather, 3), (Cord, 2)]);
    // Cord is thin on a one-handed haft; leather goes on as a wider strap.
    w.cord = if w.material == Cord { rng.range_f32(0.012, 0.02) } else { rng.range_f32(0.02, 0.035) };
    let wound = g.length * w.coverage;
    w.cord = w.cord.max(wound / (48.0 * 2.1));
    w.turns = ((wound / (w.cord * 2.1)).round() as u32).clamp(4, 48);
    w.cross = rng.chance(0.3);
    if w.cross {
        w.turns = (w.turns / 2).max(3);
    }
    w.bind = rng.chance(0.5);
}

fn roll_pommel(rng: &mut Mulberry32, a: ClubArchetype, sh: &Shaft, p: &mut Pommel) {
    p.on = rng.chance(0.45);
    p.style = *rng.weighted(&[(PommelStyle::Flared, 3), (PommelStyle::Ball, 2), (PommelStyle::Disc, 1), (PommelStyle::Ferrule, 1)]);
    p.length = rng.range_f32(0.2, 0.35);
    p.radius = sh.radius * rng.range_f32(1.2, 1.6);
    p.facets = rng.int(16, 24);
    p.material = match p.style {
        PommelStyle::Ferrule => Iron,
        _ if a == ClubArchetype::IronBound && rng.chance(0.5) => Iron,
        _ => sh.material,
    };
    p.gem = PommelGem::None;
}
