//! Axes: a haft with a bit mounted crosswise through an eye near its top, backed by a plain
//! eye, a hammer poll, a spike or a second bit.
//!
//! Not an Armory family in the prototype. Its plate (`reference/forever-below-plate.html`)
//! faked the "Hand axe" base (`data/bases.json`, materials Iron and Steel) as a flat extruded
//! outline on a swept stick, turned so the bit faces forward in the hand. Here it is built
//! from shared code:
//! - The haft, grip, wrap and butt knob come from [`crate::build::hafted`].
//! - The head is an [`AxeHead`] built by [`axe_head_mesh`]: a wedge-sectioned bit with a
//!   hard bevel, the blade cross-sections and edge damage swords use, through an oval eye
//!   (a [`Socket`] section: its length is the eye's height, its flare the eye's girth),
//!   with langets from [`langets_mesh`] running down the haft.
//! - The roll composes a weighted [`AxeArchetype`] (hatchet, hand, bearded, broad, long,
//!   battle, horseman's, throwing) so every noun comes up with a head that fits it.

use crate::build::axe_head::axe_head_mesh;
use crate::build::blade::BladeDamage;
use crate::build::hafted::{Haft, butt_radius, grip_mesh, grip_span, pommel_mesh, wrap_mesh};
use crate::build::parts::Parts;
use crate::build::socket::langets_mesh;
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::geom::ItemMesh;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::axe_head::{AxeHead, AxeHeadLimits, AxePoll};
use crate::sections::blade::BladeSection;
use crate::sections::hafted::{Grip, GripLimits, Pommel, PommelGem, PommelLimits, PommelStyle, Shaft, ShaftLimits, Wrap, WrapLimits};
use crate::sections::socket::{Socket, SocketLimits};
use crate::slider::Sliders;
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Cord, Iron, Leather, Wood};

/// The axe generator.
pub struct Axe;

/// Every parameter of one axe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxeState {
    pub seed: u32,
    /// The haft.
    pub shaft: Shaft,
    pub grip: Grip,
    pub wrap: Wrap,
    /// The knob or cap at the butt.
    pub pommel: Pommel,
    /// The eye the haft passes through: its length is the eye's height, its flare its girth.
    pub eye: Socket,
    pub head: AxeHead,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for AxeState {
    /// A plain hand axe.
    fn default() -> Self {
        AxeState {
            seed: 0,
            shaft: Shaft { length: 4.2, radius: 0.13, taper: 1.0, ..Shaft::default() },
            grip: Grip { on: false, length: 1.4, thickness: 1.1, material: Leather, ..Grip::default() },
            wrap: Wrap { on: false, ..Wrap::default() },
            pommel: Pommel { on: false, style: PommelStyle::Flared, length: 0.3, radius: 0.18, material: Wood, ..Pommel::default() },
            eye: Socket { length: 0.5, flare: 1.4, rings: 0, facets: 8, langets: 0, langet_length: 1.0, ..Socket::default() },
            head: AxeHead::default(),
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every axe section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxeLimits {
    pub shaft: ShaftLimits,
    pub grip: GripLimits,
    pub wrap: WrapLimits,
    pub pommel: PommelLimits,
    pub eye: SocketLimits,
    pub head: AxeHeadLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The shared slider ranges, narrowed to axes from hatchets to long axes.
pub const LIMITS: AxeLimits = AxeLimits {
    shaft: ShaftLimits {
        length: ShaftLimits::SLIDERS.length.narrowed(2.2, 14.0),
        radius: ShaftLimits::SLIDERS.radius.narrowed(0.09, 0.2),
        ..ShaftLimits::SLIDERS
    },
    grip: GripLimits { length: GripLimits::SLIDERS.length.narrowed(0.8, 2.4), ..GripLimits::SLIDERS },
    wrap: WrapLimits::SLIDERS,
    pommel: PommelLimits {
        length: PommelLimits::SLIDERS.length.narrowed(0.15, 0.7),
        radius: PommelLimits::SLIDERS.radius.narrowed(0.1, 0.35),
        ..PommelLimits::SLIDERS
    },
    eye: SocketLimits {
        length: SocketLimits::SLIDERS.length.narrowed(0.3, 1.4),
        flare: SocketLimits::SLIDERS.flare.narrowed(1.15, 2.0),
        langet_length: SocketLimits::SLIDERS.langet_length.narrowed(0.3, 2.5),
        ..SocketLimits::SLIDERS
    },
    head: AxeHeadLimits::SLIDERS,
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl AxeState {
    /// Clamp and snap every slider into `limits`, keeping the eye at least as tall as the
    /// bit's root.
    pub fn fit(&mut self, limits: &AxeLimits) {
        self.shaft.fit(&limits.shaft);
        self.grip.fit(&limits.grip);
        self.wrap.fit(&limits.wrap);
        self.pommel.fit(&limits.pommel);
        self.head.fit(&limits.head);
        self.eye.length = self.eye.length.max(self.head.root);
        self.eye.fit(&limits.eye);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    /// The prototype's "Forge new" nudge: each slider moves with probability `odds` by up to
    /// `spread / 2` of its range.
    fn nudge(&mut self, rng: &mut Mulberry32, limits: &AxeLimits, odds: f64, spread: f32) {
        self.shaft.nudge(rng, &limits.shaft, odds, spread);
        self.grip.nudge(rng, &limits.grip, odds, spread);
        self.wrap.nudge(rng, &limits.wrap, odds, spread);
        self.pommel.nudge(rng, &limits.pommel, odds, spread);
        self.eye.nudge(rng, &limits.eye, odds, spread);
        self.head.nudge(rng, &limits.head, odds, spread);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &AxeLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("shaft", self.shaft.out_of_range(&limits.shaft)),
            tag("grip", self.grip.out_of_range(&limits.grip)),
            tag("wrap", self.wrap.out_of_range(&limits.wrap)),
            tag("pommel", self.pommel.out_of_range(&limits.pommel)),
            tag("eye", self.eye.out_of_range(&limits.eye)),
            tag("head", self.head.out_of_range(&limits.head)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }

    /// Where the middle of the eye sits, as a fraction of the haft from the butt: high enough
    /// that the haft's end just shows above it.
    pub fn eye_position(&self) -> f32 {
        let length = self.shaft.length.max(0.1);
        ((length - OVERHANG - self.eye.length * 0.5) / length).clamp(0.0, 1.0)
    }
}

/// How far the haft's end stands above the eye.
const OVERHANG: f32 = 0.06;

/// The kinds of axe "Forge new" composes. Not stored: the noun reads the state itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxeArchetype {
    /// A short haft and a small head, often with a hammer poll.
    Hatchet,
    /// The plate's base: a plain one-handed axe.
    HandAxe,
    /// A narrow neck and a long beard hanging below the eye.
    Bearded,
    /// A tall, broad edge rising and dropping well past the eye.
    Broad,
    /// A long two-handed haft and a thin, wide crescent edge.
    Long,
    /// A second bit in place of the poll.
    Battle,
    /// A back spike in place of the poll, on a sometimes iron haft.
    Horseman,
    /// A short haft and an upswept head, for throwing.
    Throwing,
}

impl AxeArchetype {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(AxeArchetype, u32); 8] = [
        (AxeArchetype::Hatchet, 14),
        (AxeArchetype::HandAxe, 18),
        (AxeArchetype::Bearded, 16),
        (AxeArchetype::Broad, 12),
        (AxeArchetype::Long, 10),
        (AxeArchetype::Battle, 10),
        (AxeArchetype::Horseman, 10),
        (AxeArchetype::Throwing, 10),
    ];
}

/// Hafts longer than this are two-handed long axes.
const LONG_HAFT: f32 = 8.0;
/// Hafts shorter than this are hatchets.
const HATCHET_HAFT: f32 = 3.3;
/// An edge raised this far above the eye is a throwing axe's.
const UPSWEPT: f32 = 0.2;
/// A beard deeper than this (with little toe) makes a bearded axe.
const BEARDED: f32 = 0.35;
/// An edge taller than this is a broad axe's.
const BROAD_EDGE: f32 = 1.1;

impl Generator for Axe {
    type State = AxeState;

    fn roll(seed: u32) -> AxeState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let archetype = *rng.weighted(&AxeArchetype::WEIGHTS);
        compose(&mut rng, seed, archetype)
    }

    fn noun(s: &AxeState) -> String {
        let h = &s.head;
        let noun = match h.poll {
            AxePoll::Bit => "Battle Axe",
            AxePoll::Spike => "Horseman's Axe",
            _ if s.shaft.length > LONG_HAFT => "Long Axe",
            _ if h.droop > UPSWEPT => "Throwing Axe",
            _ if h.beard > BEARDED && h.toe < 0.2 => "Bearded Axe",
            _ if h.edge_height() > BROAD_EDGE => "Broad Axe",
            _ if s.shaft.length < HATCHET_HAFT => "Hatchet",
            _ => "Hand Axe",
        };
        noun.to_string()
    }

    /// The haft from [`crate::build::hafted`], then the head from [`axe_head_mesh`] set
    /// round the haft just below its end, the bit toward the haft frame's normal (+X on a
    /// straight haft), and langets running down the haft under the bit and the poll.
    fn build(s: &AxeState) -> ItemMesh {
        let mut parts = Parts::new();

        let span = grip_span(&s.shaft, &s.grip);
        let mut haft = Haft::new(&s.shaft, s.seed);
        let held = s.grip.on || s.wrap.on;
        if held {
            haft.clear(span.0, span.1);
        }
        let t_eye = s.eye_position();
        let r_eye = haft.outer_radius(t_eye);
        parts.add("shaft", haft.mesh(), s.shaft.material);
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

        let built = axe_head_mesh(&s.head, &s.eye, r_eye, &BladeDamage::from_condition(&s.cond, s.seed));
        let mut head = Parts::new();
        head.add("eye", built.eye, s.eye.material);
        head.add("bit", built.bit, s.head.material);
        head.add("poll", built.poll, s.head.material);
        head.place(haft.point(t_eye), haft.tangent(t_eye));
        parts.extend(head);

        if s.eye.langets > 0 {
            let t_start = t_eye - (built.eye_height * 0.5 - 0.02) / haft.length();
            let width = (r_eye * 0.6).max(0.04);
            parts.add("langets", langets_mesh(&haft, t_start, s.eye.langets, s.eye.langet_length, width, 0.0), s.eye.material);
        }
        parts.finish()
    }
}

impl Axe {
    /// "Forge new" forced to one archetype, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, a)` when `a` is what `s` rolls.
    pub fn roll_as(seed: u32, archetype: AxeArchetype) -> AxeState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, archetype)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, a: AxeArchetype) -> AxeState {
    let mut s = AxeState { seed, ..AxeState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    // The prototype's generic pass: most sliders drift up to 15% of their range from base.
    s.nudge(rng, &LIMITS, 0.75, 0.3);

    roll_shaft(rng, a, &mut s.shaft);
    roll_head(rng, a, &mut s.head);
    roll_eye(rng, a, &s.head, &mut s.eye);
    roll_grip_and_wrap(rng, a, &s.shaft, &mut s.grip, &mut s.wrap);
    roll_pommel(rng, &s.shaft, &mut s.pommel);

    let mut targets = vec![(EtchTarget::Metals, 3)];
    if s.shaft.material == Wood {
        targets.push((EtchTarget::Wood, 1));
    }
    let fine = matches!(a, AxeArchetype::Battle | AxeArchetype::Horseman | AxeArchetype::Broad);
    s.etch = Etching::roll(rng, if fine { 0.3 } else { 0.12 }, &targets, if fine { 2 } else { 1 });

    s.fit(&LIMITS);
    s
}

fn roll_shaft(rng: &mut Mulberry32, a: AxeArchetype, sh: &mut Shaft) {
    use AxeArchetype as A;
    sh.material = if a == A::Horseman && rng.chance(0.4) { Iron } else { Wood };
    sh.length = match a {
        A::Hatchet => rng.range_f32(2.4, 3.2),
        A::Throwing => rng.range_f32(2.8, 3.8),
        A::HandAxe => rng.range_f32(3.5, 5.0),
        A::Bearded => rng.range_f32(3.8, 5.5),
        A::Horseman => rng.range_f32(4.5, 6.5),
        A::Broad | A::Battle => rng.range_f32(4.5, 7.0),
        A::Long => rng.range_f32(9.0, 13.0),
    };
    sh.radius = match (a, sh.material) {
        (_, Iron) => rng.range_f32(0.09, 0.11),
        (A::Long, _) => rng.range_f32(0.14, 0.17),
        (A::Hatchet | A::Throwing, _) => rng.range_f32(0.1, 0.13),
        _ => rng.range_f32(0.11, 0.15),
    };
    sh.taper = rng.range_f32(0.95, 1.1);
    sh.gnarl = if sh.material == Wood { rng.range_f32(0.0, 0.03) } else { 0.0 };
    sh.gnarl_freq = rng.range_f32(1.5, 3.0);
    sh.bands = 0;
}

fn roll_head(rng: &mut Mulberry32, a: AxeArchetype, h: &mut AxeHead) {
    use AxeArchetype as A;
    h.material = Iron;
    h.section = *rng.weighted(&[(BladeSection::Flat, 4), (BladeSection::Hexagonal, 2), (BladeSection::Lenticular, 2), (BladeSection::Diamond, 1)]);
    h.thickness = if a == A::Long { rng.range_f32(0.07, 0.1) } else { rng.range_f32(0.1, 0.18) };
    h.bevel = rng.range_f32(0.1, 0.25);
    h.droop = rng.range_f32(-0.05, 0.05);
    h.poll = AxePoll::Plain;
    h.poll_length = rng.range_f32(0.1, 0.25);
    // Each archetype's (reach, root, toe, beard, waist, curve) ranges.
    let r = |rng: &mut Mulberry32, lo: f32, hi: f32| rng.range_f32(lo, hi);
    match a {
        A::Hatchet => {
            (h.reach, h.root, h.toe, h.beard) = (r(rng, 0.6, 0.85), r(rng, 0.25, 0.35), r(rng, 0.05, 0.2), r(rng, 0.05, 0.25));
            (h.waist, h.curve) = (r(rng, 0.1, 0.4), r(rng, 0.05, 0.2));
            h.poll = *rng.weighted(&[(AxePoll::Hammer, 3), (AxePoll::Plain, 2)]);
            h.poll_length = r(rng, 0.1, 0.2);
        }
        A::HandAxe => {
            (h.reach, h.root, h.toe, h.beard) = (r(rng, 0.8, 1.15), r(rng, 0.28, 0.4), r(rng, 0.05, 0.25), r(rng, 0.05, 0.3));
            (h.waist, h.curve) = (r(rng, 0.1, 0.5), r(rng, 0.05, 0.2));
            h.poll = *rng.weighted(&[(AxePoll::Plain, 3), (AxePoll::Hammer, 1)]);
        }
        A::Bearded => {
            (h.reach, h.root, h.toe, h.beard) = (r(rng, 0.9, 1.3), r(rng, 0.2, 0.3), r(rng, 0.0, 0.15), r(rng, 0.45, 0.9));
            (h.waist, h.curve) = (r(rng, 0.5, 0.9), r(rng, 0.1, 0.25));
        }
        A::Broad => {
            (h.reach, h.root, h.toe, h.beard) = (r(rng, 1.0, 1.4), r(rng, 0.35, 0.5), r(rng, 0.4, 0.6), r(rng, 0.4, 0.6));
            (h.waist, h.curve) = (r(rng, 0.2, 0.6), r(rng, 0.15, 0.3));
            h.poll = *rng.weighted(&[(AxePoll::Plain, 3), (AxePoll::Hammer, 1)]);
        }
        A::Long => {
            (h.reach, h.root, h.toe, h.beard) = (r(rng, 1.4, 2.0), r(rng, 0.25, 0.35), r(rng, 0.5, 0.9), r(rng, 0.6, 1.0));
            (h.waist, h.curve) = (r(rng, 0.5, 0.9), r(rng, 0.25, 0.45));
        }
        A::Battle => {
            (h.reach, h.root, h.toe, h.beard) = (r(rng, 0.9, 1.3), r(rng, 0.25, 0.4), r(rng, 0.25, 0.5), r(rng, 0.25, 0.5));
            (h.waist, h.curve) = (r(rng, 0.4, 0.8), r(rng, 0.25, 0.45));
            h.poll = AxePoll::Bit;
        }
        A::Horseman => {
            (h.reach, h.root, h.toe, h.beard) = (r(rng, 0.7, 1.0), r(rng, 0.2, 0.3), r(rng, 0.05, 0.2), r(rng, 0.1, 0.35));
            (h.waist, h.curve) = (r(rng, 0.3, 0.7), r(rng, 0.05, 0.2));
            h.poll = AxePoll::Spike;
            h.poll_length = r(rng, 0.4, 0.8);
        }
        A::Throwing => {
            (h.reach, h.root, h.toe, h.beard) = (r(rng, 0.7, 1.0), r(rng, 0.2, 0.3), r(rng, 0.0, 0.1), r(rng, 0.15, 0.35));
            (h.waist, h.curve) = (r(rng, 0.4, 0.8), r(rng, 0.1, 0.3));
            h.droop = r(rng, 0.25, 0.45);
        }
    }
}

fn roll_eye(rng: &mut Mulberry32, a: AxeArchetype, h: &AxeHead, e: &mut Socket) {
    use AxeArchetype as A;
    e.on = true;
    e.material = h.material;
    e.length = (h.root + rng.range_f32(0.05, 0.25)).max(0.35);
    e.flare = rng.range_f32(1.25, 1.5);
    e.facets = *rng.weighted(&[(8, 3), (6, 1), (24, 2)]);
    e.rings = *rng.weighted(&[(0, 3), (1, 1), (2, 2)]);
    let long = matches!(a, A::Long | A::Battle | A::Horseman);
    e.langets = if rng.chance(if long { 0.45 } else { 0.1 }) { 2 } else { 0 };
    e.langet_length = rng.range_f32(0.6, if a == A::Long { 2.2 } else { 1.4 });
}

fn roll_grip_and_wrap(rng: &mut Mulberry32, a: AxeArchetype, sh: &Shaft, g: &mut Grip, w: &mut Wrap) {
    use AxeArchetype as A;
    let fine = matches!(a, A::Battle | A::Horseman | A::Long);
    g.on = rng.chance(if fine { 0.7 } else { 0.4 });
    g.position = 0.0;
    g.length = rng.range_f32(1.0, 1.8).min(sh.length * 0.4);
    g.thickness = rng.range_f32(1.05, 1.18);
    g.rivets = 0;
    g.material = *rng.weighted(&[(Leather, 4), (Wood, 1)]);
    w.on = rng.chance(if g.on { 0.5 } else { 0.3 });
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

fn roll_pommel(rng: &mut Mulberry32, sh: &Shaft, p: &mut Pommel) {
    p.on = rng.chance(0.6);
    // A swelling of the haft's own wood (a fawn's foot), a knob, or an iron cap.
    p.style = *rng.weighted(&[(PommelStyle::Flared, 3), (PommelStyle::Ball, 1), (PommelStyle::Ferrule, 2)]);
    (p.length, p.radius) = match p.style {
        PommelStyle::Ferrule => (rng.range_f32(0.25, 0.45), sh.radius * rng.range_f32(1.05, 1.2)),
        _ => (rng.range_f32(0.2, 0.4), sh.radius * rng.range_f32(1.3, 1.7)),
    };
    p.facets = rng.int(16, 24);
    p.material = if p.style == PommelStyle::Ferrule || sh.material == Iron { Iron } else { sh.material };
    p.gem = PommelGem::None;
}
