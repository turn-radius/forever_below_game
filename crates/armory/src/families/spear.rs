//! Spears: a long haft with a socketed blade, sometimes wings or a bar below it, and a butt
//! cap.
//!
//! Sliders, ranges and the noun come from `reference/spline_armory_engine.js` (`SECTIONS`,
//! `RANGES.spear`, `PRESETS.spear`, the `fam==='spear'` branch of `randomize()`,
//! `buildBladed()`, `proportion()` and `forgeNoun()`).
//!
//! Departures from the prototype:
//! - The blade is a [`Spearhead`], the prototype's `blade` sliders without the sword-only
//!   ones (curve, single edge, clip, ricasso) and with a midrib, and the guard is [`Lugs`]
//!   (wings or a bar, on the socket or the blade base): a spear's guard is never a disc or
//!   ballock lobes, and its wings are plates in the blade's plane, not swept rods.
//! - The socket sleeves the end of the haft instead of standing on it, so the head is seated
//!   the way a forged socket is, and langets lie on the haft and are riveted through.
//! - The roll composes a weighted [`SpearArchetype`] (leaf, plain, javelin, pike, awl pike,
//!   bodkin, winged, lugged, boar, partisan) instead of nudging a random preset, so every name
//!   in `forgeNoun()` comes up with a head that fits it.

use crate::build::hafted::{Haft, butt_radius, grip_mesh, grip_span, pommel_mesh, shaft_bands, wrap_mesh};
use crate::build::parts::Parts;
use crate::build::socket::{langets_mesh, socket_mesh};
use crate::build::spearhead::{Break, half_width, lugs_mesh, spearhead_mesh};
use crate::cond::{Condition, ConditionLimits};
use crate::etch::{EtchTarget, Etching, EtchingLimits};
use crate::families::Generator;
use crate::geom::ItemMesh;
use crate::material::MaterialKind;
use crate::palette::Palette;
use crate::sections::hafted::{Grip, GripLimits, Pommel, PommelGem, PommelLimits, PommelStyle, Shaft, ShaftLimits, Wrap, WrapLimits};
use crate::sections::socket::{Socket, SocketLimits};
use crate::sections::blade::{BladeSection, GuardMount, GuardTerminal};
use crate::sections::spearhead::{LugStyle, Lugs, LugsLimits, Spearhead, SpearheadLimits};
use crate::slider::Sliders;
use procgen_rng::{Mulberry32, armory_stream_seed};
use serde::{Deserialize, Serialize};

use MaterialKind::{Bone, Brass, Cord, Iron, Leather, Wood};

/// The spear generator.
pub struct Spear;

/// Every parameter of one spear.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpearState {
    pub seed: u32,
    /// The haft.
    pub shaft: Shaft,
    pub grip: Grip,
    pub wrap: Wrap,
    /// The butt cap.
    pub pommel: Pommel,
    pub socket: Socket,
    pub blade: Spearhead,
    /// Wings or a bar below the blade.
    pub lugs: Lugs,
    pub etch: Etching,
    pub cond: Condition,
    pub palette: Palette,
}

impl Default for SpearState {
    /// The prototype's "Leaf spear" preset.
    fn default() -> Self {
        SpearState {
            seed: 0,
            shaft: Shaft { length: 17.0, radius: 0.15, taper: 0.95, gnarl: 0.04, gnarl_freq: 2.0, bands: 0, ..Shaft::default() },
            grip: Grip { on: false, position: 0.45, length: 2.4, thickness: 1.1, material: Leather, ..Grip::default() },
            wrap: Wrap { on: false, ..Wrap::default() },
            pommel: Pommel { style: PommelStyle::Ferrule, length: 0.7, radius: 0.17, facets: 8, ..Pommel::default() },
            socket: Socket::default(),
            blade: Spearhead::default(),
            lugs: Lugs::default(),
            etch: Etching::default(),
            cond: Condition::default(),
            palette: Palette::default(),
        }
    }
}

/// Slider limits for every spear section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpearLimits {
    pub shaft: ShaftLimits,
    pub grip: GripLimits,
    pub wrap: WrapLimits,
    pub pommel: PommelLimits,
    pub socket: SocketLimits,
    pub blade: SpearheadLimits,
    pub lugs: LugsLimits,
    pub etch: EtchingLimits,
    pub cond: ConditionLimits,
}

/// The prototype's slider ranges with `RANGES.spear` applied.
pub const LIMITS: SpearLimits = SpearLimits {
    shaft: ShaftLimits {
        length: ShaftLimits::SLIDERS.length.narrowed(8.0, 22.0),
        radius: ShaftLimits::SLIDERS.radius.narrowed(0.11, 0.2),
        ..ShaftLimits::SLIDERS
    },
    grip: GripLimits::SLIDERS,
    wrap: WrapLimits::SLIDERS,
    pommel: PommelLimits {
        length: PommelLimits::SLIDERS.length.narrowed(0.2, 1.2),
        radius: PommelLimits::SLIDERS.radius.narrowed(0.1, 0.25),
        ..PommelLimits::SLIDERS
    },
    socket: SocketLimits { length: SocketLimits::SLIDERS.length.narrowed(0.5, 2.5), ..SocketLimits::SLIDERS },
    blade: SpearheadLimits {
        length: SpearheadLimits::SLIDERS.length.narrowed(1.2, 5.0),
        width: SpearheadLimits::SLIDERS.width.narrowed(0.1, 0.9),
        thickness: SpearheadLimits::SLIDERS.thickness.narrowed(0.04, 0.15),
        ..SpearheadLimits::SLIDERS
    },
    lugs: LugsLimits {
        span: LugsLimits::SLIDERS.span.narrowed(0.15, 0.8),
        thickness: LugsLimits::SLIDERS.thickness.narrowed(0.03, 0.09),
        ..LugsLimits::SLIDERS
    },
    etch: EtchingLimits::SLIDERS,
    cond: ConditionLimits::SLIDERS,
};

impl SpearState {
    /// Clamp and snap every slider into `limits`, then keep the butt cap in proportion to
    /// the haft (the prototype's `proportion()`).
    pub fn fit(&mut self, limits: &SpearLimits) {
        self.shaft.fit(&limits.shaft);
        self.grip.fit(&limits.grip);
        self.wrap.fit(&limits.wrap);
        self.pommel.radius = self.pommel.radius.clamp(self.shaft.radius * 0.95, self.shaft.radius * 1.4);
        self.pommel.fit(&limits.pommel);
        self.socket.fit(&limits.socket);
        self.blade.fit(&limits.blade);
        self.lugs.fit(&limits.lugs);
        self.etch.fit(&limits.etch);
        self.cond.fit(&limits.cond);
    }

    /// The prototype's "Forge new" nudge: each slider moves with probability `odds` by up to
    /// `spread / 2` of its range.
    fn nudge(&mut self, rng: &mut Mulberry32, limits: &SpearLimits, odds: f64, spread: f32) {
        self.shaft.nudge(rng, &limits.shaft, odds, spread);
        self.grip.nudge(rng, &limits.grip, odds, spread);
        self.wrap.nudge(rng, &limits.wrap, odds, spread);
        self.pommel.nudge(rng, &limits.pommel, odds, spread);
        self.socket.nudge(rng, &limits.socket, odds, spread);
        self.blade.nudge(rng, &limits.blade, odds, spread);
        self.lugs.nudge(rng, &limits.lugs, odds, spread);
    }

    /// Every slider outside `limits`, as `"section.field"`.
    pub fn out_of_range(&self, limits: &SpearLimits) -> Vec<String> {
        let tag = |sec: &str, fields: Vec<&'static str>| fields.into_iter().map(|f| format!("{sec}.{f}")).collect::<Vec<_>>();
        [
            tag("shaft", self.shaft.out_of_range(&limits.shaft)),
            tag("grip", self.grip.out_of_range(&limits.grip)),
            tag("wrap", self.wrap.out_of_range(&limits.wrap)),
            tag("pommel", self.pommel.out_of_range(&limits.pommel)),
            tag("socket", self.socket.out_of_range(&limits.socket)),
            tag("blade", self.blade.out_of_range(&limits.blade)),
            tag("lugs", self.lugs.out_of_range(&limits.lugs)),
            tag("etch", self.etch.out_of_range(&limits.etch)),
            tag("cond", self.cond.out_of_range(&limits.cond)),
        ]
        .concat()
    }
}

/// The kinds of spear "Forge new" composes. Not stored: the noun reads the state itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpearArchetype {
    /// A leaf-shaped blade, widest a third of the way up, sometimes with flame waves.
    Leaf,
    /// A long, angular blade widest near its base.
    Plain,
    /// A short haft and a small head, for throwing.
    Javelin,
    /// A very long haft with a small head.
    Pike,
    /// A very long haft with a long, square, needle head and long langets.
    AwlPike,
    /// A narrow, square-sectioned head on an ordinary haft.
    Bodkin,
    /// Wings on the socket below the blade.
    Winged,
    /// Lugs at the base of the blade.
    Lugged,
    /// A broad blade and a crossbar through the socket.
    Boar,
    /// A long triangular blade with upswept flukes at its base.
    Partisan,
}

impl SpearArchetype {
    /// Roll weights, in percent.
    pub const WEIGHTS: [(SpearArchetype, u32); 10] = [
        (SpearArchetype::Leaf, 18),
        (SpearArchetype::Plain, 12),
        (SpearArchetype::Javelin, 10),
        (SpearArchetype::Pike, 8),
        (SpearArchetype::AwlPike, 6),
        (SpearArchetype::Bodkin, 8),
        (SpearArchetype::Winged, 12),
        (SpearArchetype::Lugged, 8),
        (SpearArchetype::Boar, 10),
        (SpearArchetype::Partisan, 8),
    ];
}

impl Generator for Spear {
    type State = SpearState;

    fn roll(seed: u32) -> SpearState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        let archetype = *rng.weighted(&SpearArchetype::WEIGHTS);
        compose(&mut rng, seed, archetype)
    }

    /// `forgeNoun()` for spears.
    fn noun(s: &SpearState) -> String {
        let (b, hl, l) = (&s.blade, s.shaft.length, &s.lugs);
        let noun = if b.width < 0.22 {
            if hl > 18.0 { "Awl Pike" } else { "Bodkin Spear" }
        } else if l.on && l.curve > 0.5 {
            "Partisan"
        } else if l.on {
            match (l.style, l.mount) {
                (LugStyle::Bar, _) => "Boar Spear",
                (LugStyle::Wings, GuardMount::Socket) => "Winged Spear",
                (LugStyle::Wings, GuardMount::Blade) => "Lugged Spear",
            }
        } else if hl > 18.0 {
            "Pike"
        } else if hl < 11.0 {
            "Javelin"
        } else if b.belly > 0.25 {
            "Leaf-bladed Spear"
        } else {
            "Spear"
        };
        noun.to_string()
    }

    /// The haft from [`crate::build::hafted`], with the grip where the hands go and the
    /// pommel as a butt cap; then the socket sleeved over the top of the haft, the blade on
    /// the socket, and the wings or bar, all from [`crate::build::socket`] and
    /// [`crate::build::spearhead`]. Langets run down the haft from the socket's mouth.
    fn build(s: &SpearState) -> ItemMesh {
        let mut parts = Parts::new();

        let span = grip_span(&s.shaft, &s.grip);
        let mut haft = Haft::new(&s.shaft, s.seed);
        let held = s.grip.on || s.wrap.on;
        if held {
            haft.clear(span.0, span.1);
        }
        parts.add("shaft", haft.mesh(), s.shaft.material);
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

        // The head, upright in head space on the top of the haft.
        let mut head = Parts::new();
        let r_top = haft.outer_radius(1.0);
        let b = &s.blade;
        // The socket always closes in to a neck; the blade's shoulders slope down into it.
        let neck = (b.width * 0.5 * b.shoulder * 0.75).clamp(r_top * 0.45, r_top * 0.75);
        let (blade_y, socket) = if s.socket.on {
            let socket = socket_mesh(&s.socket, r_top, neck);
            head.add("socket", socket.mesh.clone(), s.socket.material);
            // The blade's root is sunk into the socket's rounded top.
            (socket.top - 0.02 - neck * 0.5, Some(socket))
        } else {
            (-0.02, None)
        };
        let broken = s.cond.broken.then(|| Break { at: 0.55 + 0.25 * Mulberry32::new(s.seed.wrapping_mul(89).wrapping_add(13)).next_f64() as f32, seed: s.seed });
        let mut blade = spearhead_mesh(b, neck, broken);
        blade.translate([0.0, blade_y, 0.0]);
        head.add("blade", blade, b.material);
        if s.lugs.on {
            let (y, root) = match (s.lugs.mount, &socket) {
                (GuardMount::Socket, Some(sk)) => {
                    let y = match s.lugs.style {
                        LugStyle::Wings => sk.top - (sk.top - sk.mouth) * 0.3,
                        LugStyle::Bar => sk.mouth + (sk.top - sk.mouth) * 0.4,
                    };
                    (y, sk.radius_at(y))
                }
                _ => (blade_y + s.lugs.thickness * 0.8, half_width(b, 0.03).max(neck) * 0.9),
            };
            let mut lugs = lugs_mesh(&s.lugs, root);
            lugs.translate([0.0, y, 0.0]);
            head.add("lugs", lugs, s.lugs.material);
        }
        head.place(haft.point(1.0), haft.tangent(1.0));
        parts.extend(head);

        if let Some(sk) = socket.filter(|_| s.socket.langets > 0) {
            // From just under the socket's lip, down the haft, first one on a flat of the blade.
            let t_start = 1.0 - (-sk.mouth - 0.04) / haft.length();
            let width = (r_top * 0.6).max(0.04);
            let langets = langets_mesh(&haft, t_start, s.socket.langets, s.socket.langet_length, width, std::f32::consts::FRAC_PI_2);
            parts.add("langets", langets, s.socket.material);
        }
        parts.finish()
    }
}

impl Spear {
    /// "Forge new" forced to one archetype, for turntables and tests. Draws the same stream
    /// as [`Generator::roll`], so `roll(s)` equals `roll_as(s, a)` when `a` is what `s` rolls.
    pub fn roll_as(seed: u32, archetype: SpearArchetype) -> SpearState {
        let mut rng = Mulberry32::new(armory_stream_seed(seed));
        rng.next_f64();
        compose(&mut rng, seed, archetype)
    }
}

fn compose(rng: &mut Mulberry32, seed: u32, a: SpearArchetype) -> SpearState {
    use SpearArchetype as A;
    let mut s = SpearState { seed, ..SpearState::default() };
    s.palette = *rng.pick(&Palette::PRESETS);
    // The prototype's generic pass: most sliders drift up to 15% of their range from base.
    s.nudge(rng, &LIMITS, 0.75, 0.3);

    roll_shaft(rng, a, &mut s.shaft);
    roll_blade(rng, a, &mut s.blade);
    roll_lugs(rng, a, &mut s.lugs);
    roll_socket(rng, a, &mut s.socket);
    roll_grip_and_wrap(rng, a, &s.shaft, &mut s.grip, &mut s.wrap);
    roll_pommel(rng, &s.shaft, &s.socket, &mut s.pommel);

    let mut targets = vec![(EtchTarget::Metals, 3)];
    if s.shaft.material == Wood {
        targets.push((EtchTarget::Wood, 1));
    }
    let fine = matches!(a, A::Partisan | A::Winged);
    s.etch = Etching::roll(rng, if fine { 0.35 } else { 0.2 }, &targets, if fine { 2 } else { 1 });

    s.fit(&LIMITS);
    s
}

fn roll_shaft(rng: &mut Mulberry32, a: SpearArchetype, sh: &mut Shaft) {
    use SpearArchetype as A;
    sh.material = *rng.weighted(match a {
        A::Javelin => &[(Wood, 9), (Bone, 1)][..],
        A::Pike | A::AwlPike => &[(Wood, 1)],
        _ => &[(Wood, 12), (Iron, 1), (Bone, 1)],
    });
    // Over 18 is a pike, under 11 a javelin (forgeNoun).
    sh.length = match a {
        A::Javelin => rng.range_f32(8.0, 10.5),
        A::Pike | A::AwlPike => rng.range_f32(18.5, 22.0),
        _ => rng.range_f32(11.5, 17.5),
    };
    sh.radius = match (a, sh.material) {
        (A::Javelin, _) => rng.range_f32(0.11, 0.14),
        (_, Iron | Bone) => rng.range_f32(0.11, 0.14),
        (A::Pike | A::AwlPike, _) => rng.range_f32(0.15, 0.2),
        _ => rng.range_f32(0.13, 0.18),
    };
    // A haft thins toward the head.
    sh.taper = rng.range_f32(0.85, 1.0);
    sh.gnarl = if sh.material == Wood { rng.range_f32(0.0, 0.05) } else { 0.0 };
    sh.gnarl_freq = rng.range_f32(1.5, 3.0);
    sh.bands = if sh.material == Wood && matches!(a, A::Pike | A::AwlPike | A::Boar) && rng.chance(0.4) { rng.int(1, 2) } else { 0 };
    sh.band_size = rng.range_f32(0.02, 0.035);
}

fn roll_blade(rng: &mut Mulberry32, a: SpearArchetype, b: &mut Spearhead) {
    use BladeSection::*;
    use SpearArchetype as A;
    b.material = Iron;
    b.waves = 0;
    b.fuller = 0.0;
    b.midrib = 0.0;
    b.taper = rng.range_f32(0.1, 0.35);
    b.belly_width = rng.range_f32(0.95, 1.1);
    match a {
        A::Leaf | A::Javelin | A::Winged | A::Lugged if a == A::Leaf || rng.chance(0.6) => {
            // Leaf-shaped: widest a third of the way up, the edges swelling into the point.
            b.length = match a {
                A::Javelin => rng.range_f32(1.2, 2.0),
                _ => rng.range_f32(2.2, 4.0),
            };
            b.width = rng.range_f32(0.4, 0.75).min(b.length * 0.32);
            b.shoulder = rng.range_f32(0.15, 0.4);
            b.belly = rng.range_f32(0.3, 0.45);
            b.belly_width = rng.range_f32(1.0, 1.15);
            b.point_start = rng.range_f32(0.55, 0.75);
            b.point_fullness = rng.range_f32(0.4, 0.8);
            b.section = *rng.weighted(&[(Lenticular, 3), (Diamond, 2), (Hexagonal, 1)]);
            if a == A::Leaf && rng.chance(0.12) {
                b.waves = rng.int(3, 6);
                b.wave_depth = rng.range_f32(0.04, 0.09);
            }
        }
        A::Leaf | A::Plain | A::Javelin | A::Pike | A::Winged | A::Lugged => {
            // Angular: widest near the base, straight edges to a long point.
            b.length = match a {
                A::Javelin => rng.range_f32(1.2, 2.0),
                A::Pike => rng.range_f32(1.4, 2.4),
                _ => rng.range_f32(2.0, 3.8),
            };
            b.width = rng.range_f32(0.3, 0.55).min(b.length * 0.28);
            b.shoulder = rng.range_f32(0.5, 0.9);
            b.belly = rng.range_f32(0.05, 0.22);
            b.point_start = rng.range_f32(0.4, 0.7);
            b.point_fullness = rng.range_f32(0.0, 0.3);
            b.section = *rng.weighted(&[(Diamond, 3), (Hexagonal, 1), (Flat, 1)]);
        }
        A::AwlPike | A::Bodkin => {
            // A square needle.
            b.length = if a == A::AwlPike { rng.range_f32(3.0, 5.0) } else { rng.range_f32(1.4, 2.6) };
            b.width = rng.range_f32(0.11, 0.2);
            b.shoulder = rng.range_f32(0.95, 1.1);
            b.belly = rng.range_f32(0.0, 0.1);
            b.belly_width = 1.0;
            b.taper = rng.range_f32(0.3, 0.6);
            b.point_start = rng.range_f32(0.6, 0.9);
            b.point_fullness = 0.0;
            b.section = Diamond;
        }
        A::Boar => {
            // Broad, to stop a charge.
            b.length = rng.range_f32(2.2, 3.4);
            b.width = rng.range_f32(0.65, 0.9);
            b.shoulder = rng.range_f32(0.2, 0.45);
            b.belly = rng.range_f32(0.2, 0.3);
            b.point_start = rng.range_f32(0.3, 0.5);
            b.point_fullness = rng.range_f32(0.0, 0.3);
            b.taper = rng.range_f32(0.0, 0.15);
            b.section = *rng.weighted(&[(Diamond, 2), (Lenticular, 1)]);
        }
        A::Partisan => {
            b.length = rng.range_f32(2.6, 4.5);
            b.width = rng.range_f32(0.45, 0.75);
            b.shoulder = rng.range_f32(0.6, 1.0);
            b.belly = rng.range_f32(0.05, 0.15);
            b.point_start = rng.range_f32(0.3, 0.5);
            b.point_fullness = rng.range_f32(0.1, 0.3);
            b.section = *rng.weighted(&[(Diamond, 2), (Hexagonal, 1)]);
        }
    }
    b.thickness = match a {
        // Square: as thick as it is wide.
        A::AwlPike | A::Bodkin => b.width * rng.range_f32(0.8, 0.95),
        A::Boar => rng.range_f32(0.08, 0.12),
        _ => rng.range_f32(0.05, 0.09),
    };
    let ribbed = match a {
        A::Leaf => 0.6,
        A::Plain | A::Winged | A::Lugged => 0.45,
        A::Boar | A::Javelin => 0.3,
        _ => 0.0,
    };
    if b.section != Flat && rng.chance(ribbed) {
        b.midrib = rng.range_f32(0.4, 0.9);
    }
    if matches!(a, A::Winged | A::Partisan | A::Plain) && b.midrib == 0.0 && rng.chance(0.3) {
        b.fullers = *rng.weighted(&[(1, 1), (2, 2)]);
        b.fuller = rng.range_f32(0.25, 0.45);
        b.fuller_length = rng.range_f32(0.3, 0.5);
        b.fuller_width = rng.range_f32(0.15, 0.3);
    }
}

fn roll_lugs(rng: &mut Mulberry32, a: SpearArchetype, l: &mut Lugs) {
    use SpearArchetype as A;
    l.on = matches!(a, A::Winged | A::Lugged | A::Boar | A::Partisan);
    l.material = Iron;
    l.taper = rng.range_f32(0.5, 0.9);
    match a {
        A::Winged => {
            l.style = LugStyle::Wings;
            l.mount = GuardMount::Socket;
            l.span = rng.range_f32(0.18, 0.4);
            l.thickness = rng.range_f32(0.04, 0.07);
            l.curve = rng.range_f32(-0.15, 0.3);
            l.ends = *rng.weighted(&[(GuardTerminal::None, 4), (GuardTerminal::Ball, 1)]);
        }
        A::Lugged => {
            l.style = LugStyle::Wings;
            l.mount = GuardMount::Blade;
            l.span = rng.range_f32(0.15, 0.32);
            l.thickness = rng.range_f32(0.035, 0.06);
            l.curve = rng.range_f32(-0.3, 0.4);
            l.ends = *rng.weighted(&[(GuardTerminal::None, 3), (GuardTerminal::Disc, 1)]);
        }
        A::Boar => {
            l.style = LugStyle::Bar;
            l.mount = GuardMount::Socket;
            l.span = rng.range_f32(0.3, 0.55);
            l.thickness = rng.range_f32(0.04, 0.06);
            l.curve = 0.0;
            l.taper = rng.range_f32(0.0, 0.5);
            l.ends = *rng.weighted(&[(GuardTerminal::Ball, 3), (GuardTerminal::Disc, 1), (GuardTerminal::None, 1)]);
        }
        A::Partisan => {
            l.style = LugStyle::Wings;
            l.mount = GuardMount::Blade;
            l.span = rng.range_f32(0.35, 0.7);
            l.thickness = rng.range_f32(0.04, 0.07);
            l.curve = rng.range_f32(0.6, 1.0);
            l.ends = *rng.weighted(&[(GuardTerminal::None, 3), (GuardTerminal::Flare, 1)]);
        }
        _ => {}
    }
    l.end_size = (l.thickness * rng.range_f32(1.2, 1.8)).max(0.04);
}

fn roll_socket(rng: &mut Mulberry32, a: SpearArchetype, so: &mut Socket) {
    use SpearArchetype as A;
    so.on = true;
    so.material = Iron;
    so.length = match a {
        A::AwlPike => rng.range_f32(1.2, 2.0),
        A::Javelin => rng.range_f32(0.8, 1.4),
        _ => rng.range_f32(0.9, 1.8),
    };
    so.flare = rng.range_f32(1.05, 1.35);
    so.rings = *rng.weighted(&[(0, 2), (1, 2), (2, 3), (3, 1)]);
    so.facets = *rng.weighted(&[(8, 3), (6, 1), (20, 2)]);
    so.langets = match a {
        A::AwlPike => rng.int(2, 4),
        A::Pike if rng.chance(0.4) => 2,
        A::Javelin => 0,
        _ if rng.chance(0.15) => 2,
        _ => 0,
    };
    so.langet_length = match a {
        A::AwlPike => rng.range_f32(1.5, 3.5),
        _ => rng.range_f32(0.8, 2.0),
    };
}

fn roll_grip_and_wrap(rng: &mut Mulberry32, a: SpearArchetype, sh: &Shaft, g: &mut Grip, w: &mut Wrap) {
    use SpearArchetype as A;
    g.on = rng.chance(match a {
        A::Boar => 0.6,
        A::Javelin => 0.4,
        _ => 0.3,
    });
    // A javelin is held at its balance point, a spear a little behind the middle.
    g.position = if a == A::Javelin { rng.range_f32(0.35, 0.5) } else { rng.range_f32(0.4, 0.6) };
    g.length = rng.range_f32(1.6, 2.8).min(sh.length * 0.25);
    g.thickness = rng.range_f32(1.05, 1.15);
    g.rivets = 0;
    g.material = *rng.weighted(&[(Leather, 4), (Wood, 1)]);
    w.on = rng.chance(if g.on { 0.6 } else { 0.35 });
    w.cord = rng.range_f32(0.02, 0.04);
    w.coverage = rng.range_f32(0.7, 1.0);
    let wound = g.length * w.coverage;
    w.cord = w.cord.max(wound / (48.0 * 2.1));
    w.turns = ((wound / (w.cord * 2.1)).round() as u32).clamp(4, 48);
    w.cross = rng.chance(0.3);
    if w.cross {
        w.turns = (w.turns / 2).max(3);
    }
    w.bind = rng.chance(0.6);
    w.material = *rng.weighted(&[(Leather, 4), (Cord, 3)]);
}

fn roll_pommel(rng: &mut Mulberry32, sh: &Shaft, so: &Socket, p: &mut Pommel) {
    p.on = rng.chance(0.85);
    p.style = *rng.weighted(&[(PommelStyle::Ferrule, 3), (PommelStyle::Spike, 2)]);
    p.length = match p.style {
        PommelStyle::Spike => rng.range_f32(0.5, 1.0),
        _ => rng.range_f32(0.4, 0.9),
    };
    p.radius = sh.radius * rng.range_f32(1.0, 1.3);
    p.facets = so.facets;
    p.material = *rng.weighted(&[(Iron, 4), (Brass, 1)]);
    p.gem = PommelGem::None;
}
