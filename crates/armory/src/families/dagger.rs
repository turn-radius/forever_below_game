//! Daggers and knives: a short hilt and blade, built like a sword at hand scale.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (`SECTIONS`,
//! `RANGES.dagger`, `PRESETS.dagger`, the `fam==='dagger'` branch of `randomize()`, the
//! dagger rules in `proportion()`, and `forgeNoun()`).
//!
//! The prototype picks a guard style, pommel style and blade at random, so a rondel's discs
//! rarely meet and a seax can carry ballock lobes. Here a weighted [`DaggerArchetype`]
//! composes the dagger as a whole (the rondel's two discs, the ballock's lobes and turned
//! grip, the knife's riveted scales and clipped back), while the prototype's per-slider
//! nudge still varies what the archetype leaves alone. The geometry is the sword's, from
//! [`crate::build::hilt`].

use crate::build::hilt::{Hilt, hilted_blade};
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{Etching, EtchingLimits};
use crate::families::Generator;
use crate::families::hilted::{close_wind, etch_targets, is_metal, roll_terminal, set_guard_stone, set_pommel_stones, size_pommel};
use crate::gem::Gemstone;
use crate::geom::ItemMesh;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::blade::{Blade, BladeLimits, BladeSection, Guard, GuardLimits, GuardStyle, GuardTerminal, HiltProportions};
use crate::sections::hafted::{Grip, GripLimits, Pommel, PommelLimits, PommelStyle, Shaft, ShaftLimits, Wrap, WrapLimits};
use crate::slider::Sliders;
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Bone, Brass, Cord, Gold, Iron, Leather, Silver, Wood};

/// The dagger generator.
pub struct Dagger;

/// Every parameter of one dagger or knife. The shaft is the handle's core under the grip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DaggerState {
    pub seed: u32,
    pub shaft: Shaft,
    pub grip: Grip,
    pub wrap: Wrap,
    pub pommel: Pommel,
    pub guard: Guard,
    pub blade: Blade,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for DaggerState {
    /// The prototype's `BASE` with its "Quillon dagger" preset.
    fn default() -> Self {
        DaggerState {
            seed: 0,
            shaft: Shaft { length: 0.95, radius: 0.12, taper: 1.0, gnarl: 0.0, ..Shaft::default() },
            grip: Grip { length: 0.95, thickness: 1.15, material: Wood, ..Grip::default() },
            wrap: Wrap { turns: 10, cord: 0.018, coverage: 1.0, ..Wrap::default() },
            pommel: Pommel { style: PommelStyle::ScentStopper, length: 0.35, radius: 0.15, facets: 8, ..Pommel::default() },
            guard: Guard {
                span: 0.5,
                radius: 0.03,
                curve: 0.25,
                taper: 0.3,
                terminal: GuardTerminal::Ball,
                terminal_size: 0.035,
                block: 0.1,
                block_height: 0.08,
                gem_size: 0.04,
                ..Guard::default()
            },
            blade: Blade {
                length: 2.8,
                width: 0.32,
                taper: 0.55,
                point_start: 0.7,
                point_fullness: 0.3,
                thickness: 0.05,
                fuller_length: 0.5,
                fuller_width: 0.2,
                ..Blade::default()
            },
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every dagger section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DaggerLimits {
    pub shaft: ShaftLimits,
    pub grip: GripLimits,
    pub wrap: WrapLimits,
    pub pommel: PommelLimits,
    pub guard: GuardLimits,
    pub blade: BladeLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges with `RANGES.dagger` applied.
pub const LIMITS: DaggerLimits = DaggerLimits {
    shaft: ShaftLimits {
        length: ShaftLimits::SLIDERS.length.narrowed(0.7, 1.4),
        radius: ShaftLimits::SLIDERS.radius.narrowed(0.08, 0.16),
        ..ShaftLimits::SLIDERS
    },
    grip: GripLimits {
        length: GripLimits::SLIDERS.length.narrowed(0.6, 1.4),
        thickness: GripLimits::SLIDERS.thickness.narrowed(1.0, 1.35),
        ..GripLimits::SLIDERS
    },
    wrap: WrapLimits {
        cord: WrapLimits::SLIDERS.cord.narrowed(0.008, 0.025),
        turns: WrapLimits::SLIDERS.turns.narrowed(4.0, 20.0),
        ..WrapLimits::SLIDERS
    },
    pommel: PommelLimits {
        length: PommelLimits::SLIDERS.length.narrowed(0.1, 0.5),
        radius: PommelLimits::SLIDERS.radius.narrowed(0.08, 0.32),
        gem_size: PommelLimits::SLIDERS.gem_size.narrowed(0.02, 0.08),
        ..PommelLimits::SLIDERS
    },
    guard: GuardLimits {
        span: GuardLimits::SLIDERS.span.narrowed(0.1, 0.6),
        radius: GuardLimits::SLIDERS.radius.narrowed(0.015, 0.06),
        terminal_size: GuardLimits::SLIDERS.terminal_size.narrowed(0.02, 0.06),
        block: GuardLimits::SLIDERS.block.narrowed(0.0, 0.15),
        block_height: GuardLimits::SLIDERS.block_height.narrowed(0.04, 0.12),
        langets: GuardLimits::SLIDERS.langets.narrowed(0.0, 0.3),
        gem_size: GuardLimits::SLIDERS.gem_size.narrowed(0.02, 0.06),
        ..GuardLimits::SLIDERS
    },
    blade: BladeLimits {
        length: BladeLimits::SLIDERS.length.narrowed(1.2, 4.5),
        width: BladeLimits::SLIDERS.width.narrowed(0.08, 0.9),
        thickness: BladeLimits::SLIDERS.thickness.narrowed(0.03, 0.1),
        ricasso: BladeLimits::SLIDERS.ricasso.narrowed(0.0, 0.15),
        wave_depth: BladeLimits::SLIDERS.wave_depth.narrowed(0.0, 0.12),
        ..BladeLimits::SLIDERS
    },
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl DaggerState {
    /// The prototype's dagger proportions, then every slider clamped and snapped into `limits`.
    pub fn fit(&mut self, limits: &DaggerLimits) {
        self.shaft.fit(&limits.shaft);
        HiltProportions::DAGGER.apply(&self.shaft, &mut self.grip, &mut self.pommel, &mut self.guard, &mut self.blade);
        self.grip.fit(&limits.grip);
        self.wrap.fit(&limits.wrap);
        self.pommel.fit(&limits.pommel);
        self.guard.fit(&limits.guard);
        self.blade.fit(&limits.blade);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    fn nudge(&mut self, rng: &mut Mulberry32, limits: &DaggerLimits, odds: f64, spread: f32) {
        self.shaft.nudge(rng, &limits.shaft, odds, spread);
        self.wrap.nudge(rng, &limits.wrap, odds, spread);
        self.pommel.nudge(rng, &limits.pommel, odds, spread);
        self.guard.nudge(rng, &limits.guard, odds, spread);
        self.blade.nudge(rng, &limits.blade, odds, spread);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &DaggerLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("shaft", self.shaft.out_of_range(&limits.shaft)),
            tag("grip", self.grip.out_of_range(&limits.grip)),
            tag("wrap", self.wrap.out_of_range(&limits.wrap)),
            tag("pommel", self.pommel.out_of_range(&limits.pommel)),
            tag("guard", self.guard.out_of_range(&limits.guard)),
            tag("blade", self.blade.out_of_range(&limits.blade)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

/// The kinds of dagger "Forge new" composes. Not stored: the noun reads the state itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaggerArchetype {
    /// A disc guard and a disc pommel round a stiff thrusting blade.
    Rondel,
    /// Two lobes at the base of a turned grip.
    Ballock,
    /// A small cross, like a sword's.
    Quillon,
    /// A needle of a blade on a small cross.
    Stiletto,
    /// A single edge with the back angled down to the point.
    Seax,
    /// A short single-edged knife with riveted scales.
    Knife,
    /// A long single-edged knife (a peasant's sidearm), riveted and barred.
    Bauernwehr,
    /// A broad triangular blade, widest at the hilt, with stacked fullers.
    Cinquedea,
    /// No guard: a straight blade on a plain or carved grip.
    Dirk,
}

impl DaggerArchetype {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(DaggerArchetype, u32); 9] = [
        (DaggerArchetype::Rondel, 11),
        (DaggerArchetype::Ballock, 11),
        (DaggerArchetype::Quillon, 18),
        (DaggerArchetype::Stiletto, 9),
        (DaggerArchetype::Seax, 10),
        (DaggerArchetype::Knife, 13),
        (DaggerArchetype::Bauernwehr, 8),
        (DaggerArchetype::Cinquedea, 7),
        (DaggerArchetype::Dirk, 13),
    ];
}

impl Generator for Dagger {
    type State = DaggerState;

    fn roll(seed: u32) -> DaggerState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let archetype = *rng.weighted(&DaggerArchetype::WEIGHTS);
        compose(&mut rng, seed, archetype)
    }

    /// `forgeNoun()` for daggers.
    fn noun(s: &DaggerState) -> String {
        let b = &s.blade;
        let g = &s.guard;
        let noun = if g.on && g.style == GuardStyle::Lobes {
            "Ballock Dagger"
        } else if g.on && g.style == GuardStyle::Disc && s.pommel.style == PommelStyle::Disc {
            "Rondel Dagger"
        } else if b.width < 0.16 {
            "Stiletto"
        } else if b.spine > 0.5 {
            if b.clip < 0.7 {
                "Seax"
            } else if b.length < 2.6 {
                "Knife"
            } else {
                "Bauernwehr"
            }
        } else if b.shoulder > 1.08 && b.point_start < 0.25 {
            "Cinquedea"
        } else if g.on {
            "Quillon Dagger"
        } else {
            "Dirk"
        };
        noun.to_string()
    }

    /// The hilt, guard and blade from [`crate::build::hilt`].
    fn build(s: &DaggerState) -> ItemMesh {
        let hilt = Hilt {
            shaft: &s.shaft,
            grip: &s.grip,
            wrap: &s.wrap,
            pommel: &s.pommel,
            guard: &s.guard,
            blade: &s.blade,
            cond: &s.cond,
            seed: s.seed,
        };
        hilted_blade(&hilt).parts.finish()
    }
}

impl Dagger {
    /// "Forge new" forced to one archetype, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, a)` when `a` is what `s` rolls.
    pub fn roll_as(seed: u32, archetype: DaggerArchetype) -> DaggerState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, archetype)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, a: DaggerArchetype) -> DaggerState {
    use DaggerArchetype as A;
    let mut s = DaggerState { seed, ..DaggerState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    // The prototype's generic pass: most sliders drift up to 15% of their range from base.
    s.nudge(rng, &LIMITS, 0.75, 0.3);

    roll_blade(rng, a, &mut s.blade);
    roll_handle(rng, a, &mut s.shaft, &mut s.grip, &mut s.wrap);
    let fittings = *rng.weighted(&[(Iron, 12), (Brass, 6), (Silver, 1), (Gold, 1)]);
    roll_guard(rng, a, fittings, s.shaft.radius, &s.grip, &mut s.guard);
    roll_pommel(rng, a, fittings, &s.shaft, &s.grip, &mut s.pommel);
    let stone = *rng.pick(&Gemstone::ALL);
    let precious = matches!(fittings, Gold | Silver);
    let jewelled = matches!(a, A::Quillon | A::Cinquedea | A::Rondel | A::Stiletto);
    set_guard_stone(rng, &mut s.guard, if precious { 0.6 } else if jewelled { 0.12 } else { 0.0 }, (0.025, 0.05), stone);
    s.guard.gem &= s.guard.on;
    set_pommel_stones(rng, &mut s.pommel, if precious { 0.5 } else { 0.12 }, stone);
    s.etch = Etching::roll(rng, if precious { 0.6 } else { 0.35 }, &etch_targets(&s.grip), if precious { 2 } else { 1 });
    s.fit(&LIMITS);
    s
}

fn roll_blade(rng: &mut Mulberry32, a: DaggerArchetype, b: &mut Blade) {
    use BladeSection::*;
    use DaggerArchetype as A;
    b.material = Iron;
    b.shoulder = 1.0;
    b.belly = 0.0;
    b.belly_width = 1.0;
    b.curve = 0.0;
    b.spine = 0.0;
    b.clip = rng.range_f32(0.75, 0.9);
    b.waves = 0;
    b.ricasso = 0.0;
    b.fuller = 0.0;
    b.fullers = 1;
    let fullered = |rng: &mut Mulberry32, b: &mut Blade, odds: f64, depth: (f32, f32), length: (f32, f32), width: (f32, f32)| {
        if rng.chance(odds) {
            b.fuller = rng.range_f32(depth.0, depth.1);
            b.fuller_length = rng.range_f32(length.0, length.1);
            b.fuller_width = rng.range_f32(width.0, width.1);
        }
    };
    match a {
        A::Rondel | A::Ballock => {
            b.length = rng.range_f32(2.6, 4.2);
            b.width = rng.range_f32(0.2, 0.3);
            // Straight-sided to a long point, the whole blade a narrow triangle.
            b.taper = rng.range_f32(0.0, 0.2);
            b.point_start = rng.range_f32(0.1, 0.3);
            b.point_fullness = rng.range_f32(0.0, 0.1);
            b.section = *rng.weighted(&[(Diamond, 3), (Hexagonal, 2)]);
            b.thickness = rng.range_f32(0.06, 0.075);
            b.ricasso = if a == A::Ballock && rng.chance(0.3) { rng.range_f32(0.04, 0.1) } else { 0.0 };
        }
        A::Quillon => {
            b.length = rng.range_f32(2.4, 3.8);
            b.width = rng.range_f32(0.26, 0.38);
            b.taper = rng.range_f32(0.45, 0.62);
            b.point_start = rng.range_f32(0.6, 0.76);
            b.point_fullness = rng.range_f32(0.15, 0.4);
            b.section = *rng.weighted(&[(Lenticular, 3), (Diamond, 2), (Hexagonal, 1)]);
            b.thickness = rng.range_f32(0.045, 0.06);
            fullered(rng, b, 0.5, (0.4, 0.6), (0.4, 0.6), (0.15, 0.25));
        }
        A::Stiletto => {
            b.length = rng.range_f32(2.2, 3.4);
            b.width = rng.range_f32(0.09, 0.15);
            b.taper = rng.range_f32(0.0, 0.15);
            b.point_start = rng.range_f32(0.1, 0.35);
            b.point_fullness = 0.0;
            b.section = *rng.weighted(&[(Diamond, 3), (Hexagonal, 1)]);
            b.thickness = rng.range_f32(0.05, 0.08);
        }
        A::Seax => {
            b.length = rng.range_f32(2.8, 4.4);
            b.width = rng.range_f32(0.3, 0.42);
            b.taper = rng.range_f32(0.0, 0.1);
            b.point_start = rng.range_f32(0.8, 0.88);
            b.point_fullness = rng.range_f32(0.1, 0.3);
            b.spine = 1.0;
            b.clip = rng.range_f32(0.55, 0.68);
            b.section = Flat;
            b.thickness = rng.range_f32(0.05, 0.07);
            fullered(rng, b, 0.5, (0.3, 0.5), (0.5, 0.7), (0.08, 0.12));
            b.fullers = if b.fuller > 0.0 { rng.int(1, 2) } else { 1 };
        }
        A::Knife => {
            b.length = rng.range_f32(1.6, 2.5);
            b.width = rng.range_f32(0.3, 0.42);
            b.shoulder = rng.range_f32(0.88, 0.95);
            b.belly = rng.range_f32(0.5, 0.62);
            b.belly_width = rng.range_f32(1.03, 1.08);
            b.taper = 0.0;
            b.point_start = rng.range_f32(0.65, 0.75);
            b.point_fullness = rng.range_f32(0.55, 0.75);
            b.spine = 1.0;
            b.clip = rng.range_f32(0.72, 0.8);
            b.section = Flat;
            b.thickness = rng.range_f32(0.04, 0.055);
        }
        A::Bauernwehr => {
            b.length = rng.range_f32(2.7, 4.2);
            b.width = rng.range_f32(0.35, 0.45);
            b.taper = rng.range_f32(0.1, 0.25);
            b.point_start = rng.range_f32(0.75, 0.88);
            b.point_fullness = rng.range_f32(0.3, 0.6);
            b.curve = rng.range_f32(0.0, 0.1);
            b.spine = rng.range_f32(0.85, 1.0);
            b.clip = rng.range_f32(0.72, 0.85);
            b.section = *rng.weighted(&[(Flat, 2), (Lenticular, 1)]);
            b.thickness = rng.range_f32(0.045, 0.06);
            fullered(rng, b, 0.3, (0.3, 0.5), (0.4, 0.6), (0.08, 0.14));
        }
        A::Cinquedea => {
            b.length = rng.range_f32(2.9, 3.9);
            b.width = rng.range_f32(0.72, 0.9);
            b.shoulder = rng.range_f32(1.1, 1.2);
            b.belly_width = b.shoulder;
            b.taper = rng.range_f32(0.0, 0.1);
            b.point_start = rng.range_f32(0.05, 0.2);
            b.point_fullness = rng.range_f32(0.05, 0.15);
            b.section = Lenticular;
            b.thickness = rng.range_f32(0.05, 0.07);
            b.fuller = rng.range_f32(0.4, 0.6);
            b.fuller_length = rng.range_f32(0.35, 0.5);
            b.fuller_width = rng.range_f32(0.1, 0.15);
            b.fullers = rng.int(2, 3);
        }
        A::Dirk => {
            b.length = rng.range_f32(2.6, 4.2);
            b.width = rng.range_f32(0.2, 0.32);
            b.taper = rng.range_f32(0.5, 0.7);
            b.point_start = rng.range_f32(0.4, 0.7);
            b.point_fullness = rng.range_f32(0.05, 0.25);
            b.section = *rng.weighted(&[(Lenticular, 2), (Diamond, 2), (Hexagonal, 1)]);
            b.thickness = rng.range_f32(0.05, 0.07);
            fullered(rng, b, 0.3, (0.3, 0.5), (0.4, 0.6), (0.12, 0.2));
        }
    }
}

fn roll_handle(rng: &mut Mulberry32, a: DaggerArchetype, sh: &mut Shaft, g: &mut Grip, w: &mut Wrap) {
    use DaggerArchetype as A;
    sh.length = rng.range_f32(0.85, 1.2);
    sh.radius = rng.range_f32(0.095, 0.13);
    // A ballock grip swells toward the blade.
    sh.taper = if a == A::Ballock { rng.range_f32(1.1, 1.2) } else { rng.range_f32(0.95, 1.05) };
    sh.gnarl = 0.0;
    sh.bands = 0;
    sh.material = Wood;
    g.on = true;
    g.position = 0.0;
    g.length = sh.length;
    g.thickness = rng.range_f32(1.08, 1.3);
    // Knives have scales riveted through the tang; the rest are turned, carved or wrapped.
    let riveted = matches!(a, A::Knife | A::Bauernwehr | A::Cinquedea) || (a == A::Seax && rng.chance(0.5));
    g.material = *rng.weighted(match a {
        A::Ballock | A::Rondel => &[(Wood, 3), (Bone, 2)][..],
        A::Cinquedea => &[(Bone, 3), (Wood, 1)],
        _ => &[(Wood, 4), (Bone, 2), (Leather, 1)],
    });
    w.on = !riveted && rng.chance(match a {
        A::Quillon | A::Stiletto => 0.55,
        A::Dirk => 0.4,
        _ => 0.15,
    });
    g.rivets = if riveted { rng.int(2, 3) } else { 0 };
    w.coverage = if rng.chance(0.7) { 1.0 } else { rng.range_f32(0.75, 0.95) };
    w.cord = rng.range_f32(0.012, 0.022);
    w.cross = rng.chance(0.25);
    w.bind = rng.chance(0.6);
    w.material = *rng.weighted(&[(Leather, 3), (Cord, 2), (Brass, 1), (Silver, 1)]);
    if w.material == Leather {
        w.turns = (sh.length * rng.range_f32(9.0, 14.0)).round() as u32;
    } else {
        close_wind(sh.length, w, 20, LIMITS.wrap.cord.max);
    }
}

fn roll_guard(rng: &mut Mulberry32, a: DaggerArchetype, fittings: MaterialKind, handle_radius: f32, grip: &Grip, g: &mut Guard) {
    use DaggerArchetype as A;
    use GuardStyle::{Bar, Cross, Disc, Lobes};
    use GuardTerminal::{Ball, Flare, None};
    g.material = fittings;
    g.s_curve = false;
    g.curl = 0.0;
    g.curve = 0.0;
    g.langets = 0.0;
    g.block = 0.0;
    g.taper = rng.range_f32(0.15, 0.4);
    g.on = match a {
        A::Dirk => false,
        A::Knife => rng.chance(0.6),
        A::Seax => rng.chance(0.5),
        _ => true,
    };
    g.style = match a {
        A::Rondel => Disc,
        A::Ballock => Lobes,
        A::Knife | A::Seax | A::Bauernwehr => *rng.weighted(&[(Bar, 3), (Cross, 1)]),
        _ => Cross,
    };
    match g.style {
        Disc => {
            g.span = rng.range_f32(0.22, 0.32);
            g.radius = rng.range_f32(0.025, 0.04);
        }
        Lobes => {
            // Each lobe about as broad as the grip.
            g.span = handle_radius * rng.range_f32(1.9, 2.2);
            g.radius = 0.04;
            // Lobes are of a piece with the grip, or brass.
            g.material = if rng.chance(0.7) { grip.material } else { Brass };
        }
        _ => {
            (g.span, g.radius) = match a {
                A::Stiletto => (rng.range_f32(0.22, 0.32), rng.range_f32(0.02, 0.03)),
                A::Quillon => (rng.range_f32(0.32, 0.5), rng.range_f32(0.025, 0.04)),
                A::Cinquedea => (rng.range_f32(0.45, 0.6), rng.range_f32(0.03, 0.045)),
                _ => (rng.range_f32(0.12, 0.25), rng.range_f32(0.025, 0.04)),
            };
            g.curve = match a {
                // A cinquedea's guard arches toward the blade.
                A::Cinquedea => rng.range_f32(0.4, 0.6),
                A::Quillon => rng.range_f32(-0.15, 0.4),
                _ if g.style == Cross => rng.range_f32(-0.1, 0.15),
                _ => 0.0,
            };
            if a == A::Quillon {
                g.s_curve = rng.chance(0.1);
                g.curl = if rng.chance(0.12) { rng.range_f32(0.3, 0.6) } else { 0.0 };
                roll_terminal(rng, g, &[(Ball, 3), (GuardTerminal::Disc, 2), (Flare, 2), (None, 2)], (0.03, 0.05));
                g.block = if rng.chance(0.5) { rng.range_f32(0.06, 0.12) } else { 0.0 };
                g.langets = if rng.chance(0.2) { rng.range_f32(0.1, 0.25) } else { 0.0 };
            } else if a == A::Stiletto {
                roll_terminal(rng, g, &[(Ball, 3), (GuardTerminal::Disc, 1)], (0.025, 0.04));
                g.block = if rng.chance(0.3) { rng.range_f32(0.05, 0.08) } else { 0.0 };
            } else if a == A::Cinquedea {
                roll_terminal(rng, g, &[(None, 3), (Ball, 1)], (0.03, 0.05));
            } else {
                roll_terminal(rng, g, &[(None, 3), (Ball, 1)], (0.025, 0.04));
            }
            g.block_height = rng.range_f32(0.05, 0.1);
        }
    }
}

fn roll_pommel(rng: &mut Mulberry32, a: DaggerArchetype, fittings: MaterialKind, sh: &Shaft, grip: &Grip, p: &mut Pommel) {
    use DaggerArchetype as A;
    use PommelStyle::*;
    p.on = true;
    p.style = *rng.weighted(match a {
        A::Rondel => &[(Disc, 1)][..],
        A::Ballock => &[(Ball, 2), (Flared, 2)],
        A::Quillon => &[(ScentStopper, 3), (Wheel, 2), (Ball, 2), (Flared, 1)],
        A::Stiletto => &[(Ball, 2), (ScentStopper, 2), (Flared, 1)],
        A::Seax | A::Knife => &[(Ferrule, 3), (Flared, 1)],
        A::Bauernwehr => &[(Flared, 2), (Ferrule, 2), (ScentStopper, 1)],
        A::Cinquedea => &[(Flared, 3), (Wheel, 1)],
        A::Dirk => &[(Disc, 2), (Ball, 2), (Flared, 1), (ScentStopper, 1), (Wheel, 1)],
    });
    size_pommel(rng, p, sh.radius, 0.6);
    if a == A::Rondel {
        // The second rondel: a broad, thin disc.
        p.radius = rng.range_f32(0.2, 0.28);
        p.length = rng.range_f32(0.1, 0.16);
    }
    p.material = match a {
        // A ballock's pommel is turned with the grip, or a brass knob.
        A::Ballock if rng.chance(0.6) => grip.material,
        _ if rng.chance(0.85) || !is_metal(fittings) => fittings,
        _ => Brass,
    };
}
