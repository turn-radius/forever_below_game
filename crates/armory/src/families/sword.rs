//! Swords: a hilt (grip, wrap, pommel), a guard and a long blade.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (`SECTIONS`,
//! `RANGES.sword`, `PRESETS.sword`, the `fam==='sword'` branch of `randomize()`, the sword
//! rules in `proportion()`, and `forgeNoun()`).
//!
//! As with maces, the roll departs from the prototype on purpose. The prototype starts from
//! one of six presets and nudges every slider, so a sabre's guard can land on a gladius and
//! single edges come and go at random. Here a weighted [`SwordArchetype`] lays out the blade
//! first (two-edged or single, straight or curved, flamed or not), then a handle and guard
//! that belong with it, while the prototype's per-slider nudge still varies what the
//! archetype leaves alone.

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

/// The sword generator.
pub struct Sword;

/// Every parameter of one sword. The shaft is the handle's core under the grip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SwordState {
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

impl Default for SwordState {
    /// The prototype's `BASE` with its "Arming sword" preset.
    fn default() -> Self {
        SwordState {
            seed: 0,
            shaft: Shaft { length: 1.3, radius: 0.13, taper: 1.0, gnarl: 0.0, ..Shaft::default() },
            grip: Grip { length: 1.3, thickness: 1.15, material: Leather, ..Grip::default() },
            wrap: Wrap { turns: 16, cord: 0.02, coverage: 1.0, ..Wrap::default() },
            pommel: Pommel { style: PommelStyle::Wheel, length: 0.5, radius: 0.28, facets: 20, ..Pommel::default() },
            guard: Guard { curve: 0.12, ..Guard::default() },
            blade: Blade { length: 7.6, fuller: 0.55, ..Blade::default() },
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every sword section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SwordLimits {
    pub shaft: ShaftLimits,
    pub grip: GripLimits,
    pub wrap: WrapLimits,
    pub pommel: PommelLimits,
    pub guard: GuardLimits,
    pub blade: BladeLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges with `RANGES.sword` applied.
pub const LIMITS: SwordLimits = SwordLimits {
    shaft: ShaftLimits {
        length: ShaftLimits::SLIDERS.length.narrowed(0.9, 3.4),
        radius: ShaftLimits::SLIDERS.radius.narrowed(0.1, 0.18),
        ..ShaftLimits::SLIDERS
    },
    grip: GripLimits {
        length: GripLimits::SLIDERS.length.narrowed(0.8, 3.4),
        thickness: GripLimits::SLIDERS.thickness.narrowed(1.0, 1.4),
        ..GripLimits::SLIDERS
    },
    wrap: WrapLimits { cord: WrapLimits::SLIDERS.cord.narrowed(0.012, 0.035), ..WrapLimits::SLIDERS },
    pommel: PommelLimits {
        length: PommelLimits::SLIDERS.length.narrowed(0.2, 0.8),
        radius: PommelLimits::SLIDERS.radius.narrowed(0.12, 0.4),
        ..PommelLimits::SLIDERS
    },
    guard: GuardLimits {
        span: GuardLimits::SLIDERS.span.narrowed(0.3, 2.0),
        radius: GuardLimits::SLIDERS.radius.narrowed(0.03, 0.1),
        terminal_size: GuardLimits::SLIDERS.terminal_size.narrowed(0.03, 0.12),
        block: GuardLimits::SLIDERS.block.narrowed(0.0, 0.35),
        block_height: GuardLimits::SLIDERS.block_height.narrowed(0.06, 0.25),
        langets: GuardLimits::SLIDERS.langets.narrowed(0.0, 0.6),
        ..GuardLimits::SLIDERS
    },
    blade: BladeLimits {
        length: BladeLimits::SLIDERS.length.narrowed(4.0, 13.0),
        width: BladeLimits::SLIDERS.width.narrowed(0.2, 0.75),
        thickness: BladeLimits::SLIDERS.thickness.narrowed(0.04, 0.12),
        ..BladeLimits::SLIDERS
    },
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl SwordState {
    /// The prototype's sword proportions, then every slider clamped and snapped into `limits`.
    pub fn fit(&mut self, limits: &SwordLimits) {
        self.shaft.fit(&limits.shaft);
        HiltProportions::SWORD.apply(&self.shaft, &mut self.grip, &mut self.pommel, &mut self.guard, &mut self.blade);
        self.grip.fit(&limits.grip);
        self.wrap.fit(&limits.wrap);
        self.pommel.fit(&limits.pommel);
        self.guard.fit(&limits.guard);
        self.blade.fit(&limits.blade);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    fn nudge(&mut self, rng: &mut Mulberry32, limits: &SwordLimits, odds: f64, spread: f32) {
        self.shaft.nudge(rng, &limits.shaft, odds, spread);
        self.wrap.nudge(rng, &limits.wrap, odds, spread);
        self.pommel.nudge(rng, &limits.pommel, odds, spread);
        self.guard.nudge(rng, &limits.guard, odds, spread);
        self.blade.nudge(rng, &limits.blade, odds, spread);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &SwordLimits) -> Vec<String> {
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

/// The kinds of sword "Forge new" composes. Not stored: the noun reads the state itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwordArchetype {
    /// One-handed, two-edged, cross-hilted, fullered.
    ArmingSword,
    /// A hand-and-a-half grip, a long cross, often langets.
    Longsword,
    /// Two-handed, with a ricasso to choke up on.
    Greatsword,
    /// A narrow, stiff thrusting blade.
    Estoc,
    /// Short, straight and two-edged.
    ShortSword,
    /// Short and waisted, swelling before a short point: the gladius, or a bronze leaf blade.
    Gladius,
    /// A broad single edge that swells toward the point.
    Falchion,
    /// A straight or gently curved single edge on a knife-built hilt.
    Messer,
    /// A curved single edge.
    Sabre,
    /// A blade snaking in flame waves.
    Flamberge,
}

impl SwordArchetype {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(SwordArchetype, u32); 10] = [
        (SwordArchetype::ArmingSword, 21),
        (SwordArchetype::Longsword, 17),
        (SwordArchetype::Greatsword, 8),
        (SwordArchetype::Estoc, 7),
        (SwordArchetype::ShortSword, 6),
        (SwordArchetype::Gladius, 6),
        (SwordArchetype::Falchion, 8),
        (SwordArchetype::Messer, 9),
        (SwordArchetype::Sabre, 11),
        (SwordArchetype::Flamberge, 7),
    ];
}

impl Generator for Sword {
    type State = SwordState;

    fn roll(seed: u32) -> SwordState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let archetype = *rng.weighted(&SwordArchetype::WEIGHTS);
        compose(&mut rng, seed, archetype)
    }

    /// `forgeNoun()` for swords.
    fn noun(s: &SwordState) -> String {
        let b = &s.blade;
        let handle = s.shaft.length;
        let noun = if b.waves >= 3 && b.wave_depth > 0.03 {
            if b.length > 9.0 { "Flamberge" } else { "Flame-bladed Sword" }
        } else if b.spine > 0.5 && b.curve > 0.3 {
            "Sabre"
        } else if b.spine > 0.5 && b.belly_width > 1.2 {
            "Falchion"
        } else if b.spine > 0.5 {
            "Messer"
        } else if b.length < 5.8 {
            if b.belly_width > 1.03 && b.belly > 0.25 { "Gladius" } else { "Short Sword" }
        } else if b.width < 0.3 && b.taper > 0.5 {
            "Estoc"
        } else if b.length > 10.0 || handle > 3.0 {
            "Greatsword"
        } else if handle > 1.8 {
            "Longsword"
        } else {
            "Arming Sword"
        };
        noun.to_string()
    }

    /// The hilt, guard and blade from [`crate::build::hilt`].
    fn build(s: &SwordState) -> ItemMesh {
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

impl Sword {
    /// "Forge new" forced to one archetype, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, a)` when `a` is what `s` rolls.
    pub fn roll_as(seed: u32, archetype: SwordArchetype) -> SwordState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, archetype)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, a: SwordArchetype) -> SwordState {
    let mut s = SwordState { seed, ..SwordState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    // The prototype's generic pass: most sliders drift up to 15% of their range from base.
    s.nudge(rng, &LIMITS, 0.75, 0.3);

    roll_blade(rng, a, &mut s.blade);
    roll_handle(rng, a, s.blade.length, &mut s.shaft, &mut s.grip, &mut s.wrap);
    let fittings = *rng.weighted(&[(Iron, 14), (Brass, 5), (Silver, 1), (Gold, 1)]);
    roll_guard(rng, a, s.blade.length, fittings, &mut s.guard);
    roll_pommel(rng, a, fittings, &s.shaft, &mut s.pommel);
    if a == SwordArchetype::Gladius {
        // A big turned knob of the grip's wood or bone.
        s.pommel.material = s.grip.material;
        s.pommel.radius = rng.range_f32(0.28, 0.36);
        s.pommel.length = rng.range_f32(0.45, 0.6);
    }
    let stone = *rng.pick(&Gemstone::ALL);
    let precious = matches!(fittings, Gold | Silver);
    set_guard_stone(rng, &mut s.guard, if precious { 0.5 } else { 0.08 }, (0.035, 0.07), stone);
    s.guard.gem &= s.guard.on;
    set_pommel_stones(rng, &mut s.pommel, if precious { 0.5 } else { 0.1 }, stone);
    s.etch = Etching::roll(rng, if precious { 0.6 } else { 0.35 }, &etch_targets(&s.grip), if precious { 2 } else { 1 });
    s.fit(&LIMITS);
    s
}

fn roll_blade(rng: &mut Mulberry32, a: SwordArchetype, b: &mut Blade) {
    use BladeSection::*;
    use SwordArchetype as A;
    // Two-edged and straight unless the archetype says otherwise.
    b.material = Iron;
    b.shoulder = 1.0;
    b.belly = 0.0;
    b.belly_width = 1.0;
    b.curve = 0.0;
    b.spine = 0.0;
    b.clip = rng.range_f32(0.75, 0.9);
    b.waves = 0;
    b.ricasso = 0.0;
    b.fullers = 1;
    let fullered = |rng: &mut Mulberry32, b: &mut Blade, odds: f64, depth: (f32, f32), length: (f32, f32), width: (f32, f32)| {
        if rng.chance(odds) {
            b.fuller = rng.range_f32(depth.0, depth.1);
            b.fuller_length = rng.range_f32(length.0, length.1);
            b.fuller_width = rng.range_f32(width.0, width.1);
        } else {
            b.fuller = 0.0;
        }
    };
    match a {
        A::ArmingSword => {
            b.length = rng.range_f32(6.4, 8.6);
            b.width = rng.range_f32(0.4, 0.58);
            b.taper = rng.range_f32(0.3, 0.55);
            b.point_start = rng.range_f32(0.7, 0.86);
            b.point_fullness = rng.range_f32(0.2, 0.5);
            b.section = *rng.weighted(&[(Lenticular, 4), (Diamond, 2), (Hexagonal, 1)]);
            b.thickness = rng.range_f32(0.06, 0.08);
            b.ricasso = if rng.chance(0.2) { rng.range_f32(0.01, 0.03) } else { 0.0 };
            fullered(rng, b, 0.8, (0.4, 0.7), (0.5, 0.75), (0.15, 0.28));
            if rng.chance(0.12) {
                b.fullers = 2;
            }
        }
        A::Longsword => {
            b.length = rng.range_f32(8.2, 9.9);
            b.width = rng.range_f32(0.36, 0.5);
            b.taper = rng.range_f32(0.5, 0.68);
            b.point_start = rng.range_f32(0.6, 0.76);
            b.point_fullness = rng.range_f32(0.08, 0.25);
            b.section = *rng.weighted(&[(Diamond, 3), (Hexagonal, 2), (Lenticular, 1)]);
            b.thickness = rng.range_f32(0.07, 0.09);
            b.ricasso = if rng.chance(0.4) { rng.range_f32(0.02, 0.06) } else { 0.0 };
            fullered(rng, b, 0.5, (0.3, 0.5), (0.25, 0.45), (0.12, 0.2));
        }
        A::Greatsword => {
            b.length = rng.range_f32(10.2, 12.6);
            b.width = rng.range_f32(0.45, 0.65);
            b.taper = rng.range_f32(0.35, 0.6);
            b.point_start = rng.range_f32(0.72, 0.85);
            b.point_fullness = rng.range_f32(0.15, 0.4);
            b.section = *rng.weighted(&[(Diamond, 3), (Hexagonal, 2), (Lenticular, 1)]);
            b.thickness = rng.range_f32(0.08, 0.1);
            b.ricasso = rng.range_f32(0.08, 0.16);
            fullered(rng, b, 0.4, (0.3, 0.5), (0.3, 0.5), (0.12, 0.2));
        }
        A::Estoc => {
            b.length = rng.range_f32(8.0, 10.5);
            b.width = rng.range_f32(0.2, 0.28);
            b.taper = rng.range_f32(0.6, 0.8);
            b.point_start = rng.range_f32(0.62, 0.8);
            b.point_fullness = rng.range_f32(0.0, 0.1);
            b.section = *rng.weighted(&[(Diamond, 3), (Hexagonal, 2)]);
            b.thickness = rng.range_f32(0.09, 0.12);
            b.ricasso = if rng.chance(0.4) { rng.range_f32(0.03, 0.08) } else { 0.0 };
            b.fuller = 0.0;
        }
        A::Gladius => {
            let leaf = rng.chance(0.35);
            b.length = rng.range_f32(4.4, 5.7);
            b.material = if leaf { Brass } else { Iron };
            b.width = rng.range_f32(0.45, 0.56);
            b.shoulder = if leaf { rng.range_f32(0.7, 0.82) } else { rng.range_f32(0.9, 0.97) };
            b.belly = rng.range_f32(0.4, 0.62);
            b.belly_width = if leaf { rng.range_f32(1.15, 1.3) } else { rng.range_f32(1.06, 1.15) };
            b.taper = rng.range_f32(0.0, 0.12);
            b.point_start = rng.range_f32(0.8, 0.88);
            b.point_fullness = rng.range_f32(0.1, 0.3);
            b.section = Diamond;
            b.thickness = rng.range_f32(0.07, 0.085);
            b.fuller = 0.0;
        }
        A::ShortSword => {
            b.length = rng.range_f32(4.4, 5.7);
            b.width = rng.range_f32(0.38, 0.52);
            b.taper = rng.range_f32(0.3, 0.5);
            b.point_start = rng.range_f32(0.7, 0.82);
            b.point_fullness = rng.range_f32(0.2, 0.45);
            b.section = *rng.weighted(&[(Lenticular, 2), (Diamond, 2), (Hexagonal, 1)]);
            b.thickness = rng.range_f32(0.06, 0.08);
            fullered(rng, b, 0.5, (0.35, 0.6), (0.45, 0.65), (0.15, 0.25));
        }
        A::Falchion => {
            b.length = rng.range_f32(5.9, 7.6);
            b.width = rng.range_f32(0.5, 0.62);
            b.shoulder = rng.range_f32(0.78, 0.9);
            b.belly = rng.range_f32(0.68, 0.84);
            b.belly_width = rng.range_f32(1.3, 1.5);
            b.taper = rng.range_f32(0.0, 0.1);
            b.point_start = rng.range_f32(0.88, 0.93);
            b.point_fullness = rng.range_f32(0.6, 0.8);
            b.curve = rng.range_f32(0.0, 0.15);
            b.spine = rng.range_f32(0.85, 1.0);
            b.clip = rng.range_f32(0.74, 0.82);
            b.section = Flat;
            b.thickness = rng.range_f32(0.05, 0.07);
            fullered(rng, b, 0.3, (0.3, 0.5), (0.4, 0.6), (0.08, 0.14));
        }
        A::Messer => {
            b.length = rng.range_f32(6.0, 8.0);
            b.width = rng.range_f32(0.4, 0.52);
            b.belly = rng.range_f32(0.0, 0.3);
            b.belly_width = rng.range_f32(1.0, 1.12);
            b.taper = rng.range_f32(0.1, 0.3);
            b.point_start = rng.range_f32(0.8, 0.9);
            b.point_fullness = rng.range_f32(0.4, 0.7);
            b.curve = rng.range_f32(0.0, 0.25);
            b.spine = rng.range_f32(0.9, 1.0);
            b.clip = rng.range_f32(0.7, 0.85);
            b.section = *rng.weighted(&[(Flat, 2), (Lenticular, 1)]);
            b.thickness = rng.range_f32(0.05, 0.07);
            fullered(rng, b, 0.5, (0.3, 0.5), (0.4, 0.7), (0.08, 0.14));
        }
        A::Sabre => {
            b.length = rng.range_f32(6.8, 8.6);
            b.width = rng.range_f32(0.3, 0.42);
            b.taper = rng.range_f32(0.15, 0.35);
            b.point_start = rng.range_f32(0.82, 0.9);
            b.point_fullness = rng.range_f32(0.4, 0.65);
            b.curve = rng.range_f32(0.4, 0.8);
            b.spine = rng.range_f32(0.8, 0.95);
            b.clip = rng.range_f32(0.82, 0.9);
            b.section = *rng.weighted(&[(Lenticular, 2), (Flat, 1)]);
            b.thickness = rng.range_f32(0.05, 0.065);
            fullered(rng, b, 0.6, (0.35, 0.5), (0.6, 0.8), (0.15, 0.24));
        }
        A::Flamberge => {
            let great = rng.chance(0.6);
            b.length = if great { rng.range_f32(9.6, 12.0) } else { rng.range_f32(6.5, 8.8) };
            b.width = rng.range_f32(0.42, 0.56);
            b.taper = rng.range_f32(0.35, 0.55);
            b.point_start = rng.range_f32(0.78, 0.86);
            b.point_fullness = rng.range_f32(0.2, 0.4);
            b.waves = rng.int(6, 13);
            b.wave_depth = rng.range_f32(0.1, 0.18);
            b.ricasso = if great { rng.range_f32(0.1, 0.18) } else { rng.range_f32(0.0, 0.05) };
            b.section = *rng.weighted(&[(Diamond, 3), (Lenticular, 1)]);
            b.thickness = rng.range_f32(0.07, 0.09);
            b.fuller = 0.0;
        }
    }
}

fn roll_handle(rng: &mut Mulberry32, a: SwordArchetype, blade_length: f32, sh: &mut Shaft, g: &mut Grip, w: &mut Wrap) {
    use SwordArchetype as A;
    sh.length = match a {
        A::ArmingSword | A::Falchion | A::ShortSword | A::Gladius => rng.range_f32(1.0, 1.5),
        A::Messer => rng.range_f32(1.1, 1.7),
        A::Sabre => rng.range_f32(1.2, 1.5),
        A::Longsword | A::Estoc => rng.range_f32(2.0, 2.8),
        A::Greatsword => rng.range_f32(2.8, 3.4),
        A::Flamberge if blade_length > 9.0 => rng.range_f32(2.4, 3.4),
        A::Flamberge => rng.range_f32(1.2, 1.6),
    };
    sh.radius = rng.range_f32(0.115, 0.145);
    sh.taper = rng.range_f32(0.95, 1.05);
    sh.gnarl = 0.0;
    sh.bands = 0;
    sh.material = Wood;
    g.on = true;
    g.position = 0.0;
    g.length = sh.length;
    g.rivets = 0;
    // A gladius has a carved grip; a messer is built like a knife, its scales riveted on.
    let gladius = a == A::Gladius;
    if gladius || a == A::Messer {
        g.thickness = if gladius { rng.range_f32(1.25, 1.38) } else { rng.range_f32(1.1, 1.25) };
        g.material = *rng.weighted(&[(Wood, 3), (Bone, 2)]);
        g.rivets = if a == A::Messer { rng.int(2, 3) } else { 0 };
        w.on = false;
    } else {
        g.thickness = rng.range_f32(1.08, 1.25);
        g.material = *rng.weighted(&[(Leather, 3), (Wood, 2)]);
        w.on = rng.chance(0.85);
    }
    w.turns = (sh.length * rng.range_f32(9.0, 13.0)).round() as u32;
    w.cord = rng.range_f32(0.014, 0.03);
    w.coverage = if rng.chance(0.7) { 1.0 } else { rng.range_f32(0.8, 0.95) };
    w.cross = rng.chance(if matches!(a, A::Longsword | A::Greatsword | A::Flamberge) { 0.6 } else { 0.25 });
    w.bind = rng.chance(0.7);
    w.material = *rng.weighted(&[(Leather, 5), (Cord, 3), (Brass, 1), (Silver, 1)]);
    if w.material != Leather {
        close_wind(sh.length, w, 48, LIMITS.wrap.cord.max);
    }
}

fn roll_guard(rng: &mut Mulberry32, a: SwordArchetype, blade_length: f32, fittings: MaterialKind, g: &mut Guard) {
    use GuardStyle::{Bar, Cross, Disc};
    use GuardTerminal::{Ball, Flare, None};
    use SwordArchetype as A;
    g.on = rng.chance(if a == A::ShortSword { 0.9 } else { 0.97 }) || a == A::Gladius;
    g.material = fittings;
    g.s_curve = false;
    g.curl = 0.0;
    g.langets = 0.0;
    g.style = match a {
        A::ArmingSword => *rng.weighted(&[(Cross, 6), (Bar, 1)]),
        A::ShortSword => *rng.weighted(&[(Cross, 4), (Bar, 1)]),
        A::Gladius => Disc,
        A::Messer => *rng.weighted(&[(Bar, 2), (Cross, 1)]),
        A::Falchion => *rng.weighted(&[(Cross, 3), (Bar, 1)]),
        _ => Cross,
    };
    if g.style == Disc {
        // A plate guard, often wood or bone like the grip.
        g.span = rng.range_f32(0.42, 0.55);
        g.radius = rng.range_f32(0.06, 0.09);
        g.material = *rng.weighted(&[(Wood, 2), (Bone, 1), (fittings, 1)]);
        g.block = 0.0;
        return;
    }
    (g.span, g.radius) = match a {
        A::Longsword | A::Estoc => (rng.range_f32(1.0, 1.5), rng.range_f32(0.04, 0.055)),
        A::Greatsword => (rng.range_f32(1.4, 1.9), rng.range_f32(0.05, 0.075)),
        A::Flamberge => (blade_length * rng.range_f32(0.13, 0.16), rng.range_f32(0.045, 0.07)),
        A::Sabre => (rng.range_f32(0.45, 0.65), rng.range_f32(0.035, 0.05)),
        A::Messer => (rng.range_f32(0.45, 0.75), rng.range_f32(0.035, 0.05)),
        A::ShortSword => (rng.range_f32(0.55, 0.85), rng.range_f32(0.04, 0.06)),
        _ => (rng.range_f32(0.8, 1.2), rng.range_f32(0.04, 0.06)),
    };
    g.curve = match a {
        A::Falchion => rng.range_f32(-0.45, -0.1),
        A::Greatsword | A::Flamberge => rng.range_f32(0.0, 0.35),
        A::Sabre => rng.range_f32(-0.1, 0.25),
        _ if g.style == Bar => 0.0,
        _ => rng.range_f32(-0.1, 0.25),
    };
    g.taper = rng.range_f32(0.15, 0.45);
    if g.style == Cross {
        g.curl = match a {
            A::Sabre => rng.range_f32(0.4, 0.8),
            A::Greatsword | A::Flamberge if rng.chance(0.5) => rng.range_f32(0.3, 0.6),
            _ if rng.chance(0.08) => rng.range_f32(0.3, 0.6),
            _ => 0.0,
        };
        g.s_curve = (a == A::Sabre && rng.chance(0.5)) || rng.chance(0.04);
    }
    let size = (g.radius * 1.3, g.radius * 2.2);
    match a {
        A::Sabre | A::Falchion => roll_terminal(rng, g, &[(Ball, 4), (GuardTerminal::Disc, 1), (None, 1)], size),
        A::Longsword | A::Estoc | A::Greatsword | A::Flamberge => {
            roll_terminal(rng, g, &[(None, 3), (Ball, 1), (Flare, 1), (GuardTerminal::Disc, 1)], size)
        }
        _ => roll_terminal(rng, g, &[(Flare, 3), (Ball, 3), (GuardTerminal::Disc, 2), (None, 2)], size),
    }
    g.block = if rng.chance(0.7) { rng.range_f32(0.1, 0.26) } else { 0.0 };
    g.block_height = rng.range_f32(0.1, 0.2);
    if matches!(a, A::Longsword | A::Greatsword | A::Flamberge | A::Estoc) && rng.chance(0.5) {
        g.langets = rng.range_f32(0.2, 0.55);
    }
}

fn roll_pommel(rng: &mut Mulberry32, a: SwordArchetype, fittings: MaterialKind, sh: &Shaft, p: &mut Pommel) {
    use PommelStyle::*;
    use SwordArchetype as A;
    p.on = true;
    p.style = *rng.weighted(match a {
        A::ArmingSword | A::Falchion => &[(Wheel, 5), (Disc, 2), (Ball, 1), (ScentStopper, 1), (Flared, 1)][..],
        A::Longsword | A::Greatsword | A::Flamberge | A::Estoc => &[(ScentStopper, 4), (Wheel, 3), (Ball, 1), (Flared, 1)],
        A::ShortSword => &[(Wheel, 3), (Ball, 2), (Disc, 1)],
        A::Gladius => &[(Ball, 1)],
        A::Messer => &[(Flared, 3), (Ferrule, 2), (ScentStopper, 1)],
        A::Sabre => &[(Ferrule, 3), (Ball, 1), (Flared, 1)],
    });
    size_pommel(rng, p, sh.radius, 1.0);
    p.material = if rng.chance(0.85) || !is_metal(fittings) { fittings } else { Brass };
}
