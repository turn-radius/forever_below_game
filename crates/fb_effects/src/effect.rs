//! The description of an effect, as `data/effects/*.toml` holds it: what a spell or an item
//! shows when it goes off. An [`Effect`] is some emitters and an optional light. Each emitter
//! says what card it throws (a wisp, a spark, a ring), where it starts, when, how it moves, and
//! how its size and colour run over a particle's life. See `docs/effects.md`.

use serde::{Deserialize, Serialize};

/// A point or direction in the world, `[x, y, z]`, y up (1 unit = 10 cm).
pub type V3 = [f32; 3];

/// Where something starts, resolved against the caster and the target an effect plays on.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    /// The caster's eye.
    #[default]
    Caster,
    /// A little to the caster's right, below the eye, where a hand would be.
    BesideCaster,
    /// The middle of the target.
    Target,
    /// The floor under the target.
    TargetsFeet,
    /// A little over the target's head.
    AboveTarget,
    /// To the left of the target, looking from the caster.
    LeftOfTarget,
    /// To the right of the target, looking from the caster.
    RightOfTarget,
    /// High over the target, out of sight.
    SkyAboveTarget,
    /// Halfway from the caster to the target.
    Between,
}

impl Anchor {
    /// Whether the anchor is a place on the target, so an effect with it plays once on each
    /// target it is given.
    pub fn on_target(self) -> bool {
        !matches!(self, Anchor::Caster | Anchor::BesideCaster)
    }
}

/// The picture a particle is drawn as: one card turned to the eye, or lying on the floor.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Card {
    /// A glowing heart whose edge wavers.
    #[default]
    Wisp,
    /// A small soft dot.
    Mote,
    /// A bright streak stretched along its travel.
    Spark,
    /// A tall shaft of light standing on its foot.
    Beam,
    /// A ring lying flat on the floor.
    Ring,
}

/// The shape of the place particles start in, about the spawn's anchor.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// At the anchor.
    #[default]
    Point,
    /// On a flat circle of radius `spread`.
    Ring,
    /// Over a flat disc of radius `spread`.
    Disc,
    /// In a box `spread` either way across and `height` tall.
    Box,
    /// In an upright column of radius `spread` and `height`.
    Column,
    /// Along the arc from the anchor to `to`, `spread` loose of it.
    Path,
}

/// Where and in what shape particles appear.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Spawn {
    pub shape: Shape,
    pub at: Anchor,
    /// The far end of a path, or the place a gathering motion heads for.
    pub to: Option<Anchor>,
    /// Moved off the anchor: right of, above and ahead of where the caster faces.
    pub nudge: V3,
    pub spread: f32,
    pub height: f32,
    /// How high a path or a path motion bows over its middle.
    pub arc: f32,
}

impl Default for Spawn {
    fn default() -> Spawn {
        Spawn { shape: Shape::Point, at: Anchor::Caster, to: None, nudge: [0.0; 3], spread: 0.0, height: 0.0, arc: 0.0 }
    }
}

/// How an emitter throws its particles over time.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimingKind {
    /// `count` all at once at `start`.
    #[default]
    Burst,
    /// Steadily from `start` to `end`, `count` alive over a `life`.
    Stream,
    /// As a stream, but already `count` alive at `start`.
    Lasting,
}

/// When an emitter throws, how many, and how long each lives.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Timing {
    pub kind: TimingKind,
    /// Particles per target: all of a burst, or alive at once in a stream.
    pub count: u32,
    /// Seconds each particle lives.
    pub life: f32,
    /// Seconds into the effect it begins.
    pub start: f32,
    /// Seconds into the effect a stream stops; 0 runs to the effect's end.
    pub end: f32,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing { kind: TimingKind::Burst, count: 1, life: 1.0, start: 0.0, end: 0.0 }
    }
}

/// How a particle moves from where it starts.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MotionKind {
    /// Stays where it started.
    #[default]
    Still,
    /// Up at `speed`.
    Rise,
    /// Down at `speed`.
    Fall,
    /// Flung outward in every direction at about `speed`.
    Burst,
    /// Wanders off in a random direction at `speed`.
    Drift,
    /// Rises at `speed` while winding round a thin spiral `scatter` wide.
    Curl,
    /// Circles the spawn point on a tilted ring of `orbit_radius`, `orbits` times in its life.
    Orbit,
    /// Draws in from the spawn to the `to` anchor.
    Gather,
    /// Arcs from the spawn to the `to` anchor, bowed by `arc`.
    Path,
}

/// How particles move.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Motion {
    pub kind: MotionKind,
    /// Units a second.
    pub speed: f32,
    /// How much each particle's own velocity strays from the kind's, units a second.
    pub scatter: f32,
    /// Units a second a second, downward.
    pub gravity: f32,
    pub orbit_radius: f32,
    /// Turns round the ring in a particle's life; negative goes the other way.
    pub orbits: f32,
    /// A steady push, units a second.
    pub wind: V3,
}

impl Default for Motion {
    fn default() -> Motion {
        Motion { kind: MotionKind::Still, speed: 0.0, scatter: 0.0, gravity: 0.0, orbit_radius: 0.0, orbits: 0.0, wind: [0.0; 3] }
    }
}

/// A size at a share of a particle's life.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SizeStop {
    pub at: f32,
    pub size: f32,
}

/// A colour (sRGB, 0 to 1) at a share of a particle's life.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ColourStop {
    pub at: f32,
    pub rgb: V3,
}

/// One kind of particle an effect throws.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Emitter {
    pub name: String,
    pub card: Card,
    pub spawn: Spawn,
    pub timing: Timing,
    pub motion: Motion,
    /// Card size over a life (a wisp's width; a spark's width; a beam's width; a ring's
    /// diameter), straight between the stops and held flat past the ends. None is size 1.
    pub size: Vec<SizeStop>,
    /// Colour over a life, the same way. None is white.
    pub colour: Vec<ColourStop>,
    /// How long a particle trails: a spark's streak in seconds of its travel, a beam's height
    /// in world units.
    pub tail: f32,
}

impl Default for Emitter {
    fn default() -> Emitter {
        Emitter { name: String::new(), card: Card::Wisp, spawn: Spawn::default(), timing: Timing::default(), motion: Motion::default(), size: Vec::new(), colour: Vec::new(), tail: 0.0 }
    }
}

/// What the light follows over the effect.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Follows {
    /// Blooms in, holds, and sputters out with the effect ([`crate::envelope`]).
    Envelope,
    /// A flash at `at_s` seconds that dies away fast.
    Flash { at_s: f32 },
    /// Travels from the caster to its anchor over the effect, lit all the way.
    Travel,
}

/// The light an effect lights the level and what is held with.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Light {
    pub at: Anchor,
    /// How bright at its fullest, as a torch's peak (`crate::light` in the game).
    pub peak: f32,
    /// Units it reaches.
    pub reach: f32,
    pub colour: V3,
    pub follows: Follows,
}

/// A spell's or item's effect.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Effect {
    pub name: String,
    /// Seconds it plays. A once effect ends there; a lasting one loops its timeline over this
    /// long until it is stopped.
    pub length: f32,
    pub lasts: bool,
    /// Seconds its light and glow take to bloom in.
    pub bloom_in: f32,
    /// Seconds it sputters out over at the end.
    pub sputter_out: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<Light>,
    pub emitters: Vec<Emitter>,
}

impl Default for Effect {
    fn default() -> Effect {
        Effect { name: String::new(), length: 1.0, lasts: false, bloom_in: 0.0, sputter_out: 0.0, light: None, emitters: Vec::new() }
    }
}

/// The most particles one emitter may throw at once on one target.
pub const MAX_COUNT: u32 = 2000;

/// Why an effect file was not taken.
#[derive(Debug)]
pub enum EffectError {
    /// Not valid TOML for an effect: a bad value, a missing table or a key nothing reads.
    Toml(toml::de::Error),
    /// Valid TOML, but numbers the simulation cannot play.
    Invalid(String),
}

impl std::fmt::Display for EffectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EffectError::Toml(e) => write!(f, "{e}"),
            EffectError::Invalid(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for EffectError {}

impl Effect {
    /// The effect a `data/effects/*.toml` file describes, checked ([`Effect::validate`]). A key
    /// left out keeps its default; a key nothing reads is an error.
    pub fn from_toml(text: &str) -> Result<Effect, EffectError> {
        let effect: Effect = toml::from_str(text).map_err(EffectError::Toml)?;
        effect.validate().map_err(EffectError::Invalid)?;
        Ok(effect)
    }

    /// The effect as TOML text that [`Effect::from_toml`] reads back as the same effect.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Whether the simulation can play it: every number finite, a length and lives above 0,
    /// no more than [`MAX_COUNT`] to a burst, and size and colour stops in order.
    pub fn validate(&self) -> Result<(), String> {
        let finite = |what: &str, values: &[f32]| if values.iter().all(|v| v.is_finite()) { Ok(()) } else { Err(format!("{what} is not a finite number")) };
        finite("length", &[self.length, self.bloom_in, self.sputter_out])?;
        if self.length <= 0.0 {
            return Err("length must be above 0".into());
        }
        if self.bloom_in < 0.0 || self.sputter_out < 0.0 {
            return Err("bloom_in and sputter_out cannot be below 0".into());
        }
        if let Some(l) = &self.light {
            finite("light", &[l.peak, l.reach, l.colour[0], l.colour[1], l.colour[2]])?;
            if let Follows::Flash { at_s } = l.follows {
                finite("light flash", &[at_s])?;
            }
        }
        for (i, e) in self.emitters.iter().enumerate() {
            let at = |what: &str| format!("emitter {i} ({}): {what}", e.name);
            let (s, t, m) = (&e.spawn, &e.timing, &e.motion);
            finite(&at("spawn"), &[s.nudge[0], s.nudge[1], s.nudge[2], s.spread, s.height, s.arc])?;
            finite(&at("timing"), &[t.life, t.start, t.end])?;
            finite(&at("motion"), &[m.speed, m.scatter, m.gravity, m.orbit_radius, m.orbits, m.wind[0], m.wind[1], m.wind[2], e.tail])?;
            if t.life <= 0.0 {
                return Err(at("life must be above 0"));
            }
            if t.start < 0.0 || t.end < 0.0 {
                return Err(at("start and end cannot be below 0"));
            }
            if t.count > MAX_COUNT {
                return Err(at(&format!("count {} is over {MAX_COUNT}", t.count)));
            }
            for stop in &e.size {
                finite(&at("size"), &[stop.at, stop.size])?;
            }
            for stop in &e.colour {
                finite(&at("colour"), &[stop.at, stop.rgb[0], stop.rgb[1], stop.rgb[2]])?;
            }
            if e.size.windows(2).any(|w| w[1].at < w[0].at) {
                return Err(at("size stops are not in order of `at`"));
            }
            if e.colour.windows(2).any(|w| w[1].at < w[0].at) {
                return Err(at("colour stops are not in order of `at`"));
            }
        }
        Ok(())
    }
}

/// A point on a curve over a particle's life.
pub trait Stop {
    type Value: Copy;
    fn at(&self) -> f32;
    fn value(&self) -> Self::Value;
}

impl Stop for SizeStop {
    type Value = f32;
    fn at(&self) -> f32 {
        self.at
    }
    fn value(&self) -> f32 {
        self.size
    }
}

impl Stop for ColourStop {
    type Value = V3;
    fn at(&self) -> f32 {
        self.at
    }
    fn value(&self) -> V3 {
        self.rgb
    }
}

/// The value of a curve of `stops`, in order of `at`, at `share`: straight between neighbouring
/// stops, held flat before the first and after the last. `None` for no stops.
pub fn sample<S: Stop>(stops: &[S], share: f32, lerp: impl Fn(S::Value, S::Value, f32) -> S::Value) -> Option<S::Value> {
    let first = stops.first()?;
    let last = stops.last()?;
    if share <= first.at() {
        return Some(first.value());
    }
    if share >= last.at() {
        return Some(last.value());
    }
    let i = stops.windows(2).position(|w| share < w[1].at()).unwrap_or(0);
    let (a, b) = (&stops[i], &stops[i + 1]);
    let span = b.at() - a.at();
    Some(lerp(a.value(), b.value(), if span > 0.0 { (share - a.at()) / span } else { 1.0 }))
}

impl Emitter {
    /// The card size at `share` of a life.
    pub fn size_at(&self, share: f32) -> f32 {
        sample(&self.size, share, |a, b, t| a + (b - a) * t).unwrap_or(1.0)
    }

    /// The colour at `share` of a life.
    pub fn colour_at(&self, share: f32) -> V3 {
        sample(&self.colour, share, |a, b, t| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]).unwrap_or([1.0; 3])
    }

    /// Whether the emitter is placed by the target, so plays once on each.
    pub fn per_target(&self) -> bool {
        self.spawn.at.on_target() || self.spawn.to.is_some_and(Anchor::on_target)
    }
}
