//! The simulation of an [`Effect`]: stepping it with the places of the caster and the targets
//! throws particles and moves them, and reports what to draw. Plain data in and out, seeded
//! with [`Mulberry32`], so the same effect, seed, places and steps give the same particles.
//!
//! A particle's place is a closed form of its age, from where and how it was thrown, so it does
//! not drift with the step size.

use std::sync::Arc;

use procgen_rng::Mulberry32;

use crate::effect::{Anchor, Card, Effect, Emitter, Follows, MotionKind, Shape, TimingKind, V3};
use crate::envelope;

/// Units a target's left and right anchors stand off its middle.
const SIDE_OFF: f32 = 6.0;
/// Units over a target's head [`Anchor::AboveTarget`] is, and how high the sky is.
const ABOVE_OFF: f32 = 4.0;
const SKY_OFF: f32 = 80.0;
/// Where [`Anchor::BesideCaster`] is from the eye: right, down, ahead.
const HAND: V3 = [3.0, -3.0, 2.0];
/// A flash's rise and the time its light takes to die to a third, seconds.
const FLASH_RISE: f32 = 0.03;
const FLASH_FALL: f32 = 0.25;
/// The share of a life a particle fades in over, and out over.
const FADE_IN: f32 = 0.06;
const FADE_OUT: f32 = 0.3;
/// A step to find a particle's velocity by, seconds.
const EPS: f32 = 0.01;

/// A creature or point an effect plays on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    /// Where it stands on the floor.
    pub feet: V3,
    /// How tall it is, units (0 for a point on the floor).
    pub height: f32,
}

/// Where the caster and the targets are now. The game supplies them each step.
#[derive(Clone, Debug, PartialEq)]
pub struct Places {
    /// The caster's eye.
    pub caster: V3,
    /// The way the caster faces.
    pub forward: V3,
    pub targets: Vec<Target>,
}

/// A particle to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleOut {
    /// Counts up from 0 in the order thrown; stable for the particle's life.
    pub id: u32,
    pub emitter: usize,
    pub card: Card,
    pub pos: V3,
    /// Units a second.
    pub vel: V3,
    pub size: f32,
    /// sRGB.
    pub colour: V3,
    pub alpha: f32,
    pub age: f32,
    pub life: f32,
    pub tail: f32,
}

/// A light of the effect, to light with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightOut {
    pub pos: V3,
    /// Its share of the effect's light `peak`: 0 dark, 1 full, a little over while it blooms.
    pub brightness: f32,
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn mul(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn lerp(a: V3, b: V3, t: f32) -> V3 {
    add(a, mul(sub(b, a), t))
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: V3, or: V3) -> V3 {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if l > 1e-6 { mul(a, 1.0 / l) } else { or }
}

/// Where the anchors are for one play: the caster and, if the effect is on a target, that one.
struct Ctx {
    caster: V3,
    forward: V3,
    right: V3,
    target: Option<Target>,
}

impl Ctx {
    fn new(places: &Places, target: Option<Target>) -> Ctx {
        let forward = norm([places.forward[0], 0.0, places.forward[2]], [0.0, 0.0, -1.0]);
        let right = cross(forward, [0.0, 1.0, 0.0]);
        Ctx { caster: places.caster, forward, right, target }
    }

    /// Where `anchor` is, moved by `nudge` (right, up, ahead of the caster).
    fn at(&self, anchor: Anchor, nudge: V3) -> V3 {
        let up = [0.0, 1.0, 0.0];
        let t = self.target.unwrap_or(Target { feet: add(self.caster, mul(self.forward, 30.0)), height: 0.0 });
        let middle = add(t.feet, mul(up, t.height * 0.5));
        // The side the caster sees the target's right on.
        let side = norm(cross(norm([middle[0] - self.caster[0], 0.0, middle[2] - self.caster[2]], self.forward), up), self.right);
        let p = match anchor {
            Anchor::Caster => self.caster,
            Anchor::BesideCaster => add(self.caster, add(mul(self.right, HAND[0]), add(mul(up, HAND[1]), mul(self.forward, HAND[2])))),
            Anchor::Target => middle,
            Anchor::TargetsFeet => t.feet,
            Anchor::AboveTarget => add(t.feet, mul(up, t.height + ABOVE_OFF)),
            Anchor::LeftOfTarget => add(middle, mul(side, -SIDE_OFF)),
            Anchor::RightOfTarget => add(middle, mul(side, SIDE_OFF)),
            Anchor::SkyAboveTarget => add(t.feet, mul(up, SKY_OFF)),
            Anchor::Between => lerp(self.caster, middle, 0.5),
        };
        add(p, add(mul(self.right, nudge[0]), add(mul(up, nudge[1]), mul(self.forward, nudge[2]))))
    }
}

/// The point `s` of the way along the quadratic arc from `a` to `b`, bowed up by `arc` at its middle.
fn arc_at(a: V3, b: V3, arc: f32, s: f32) -> V3 {
    let control = add(lerp(a, b, 0.5), [0.0, 2.0 * arc, 0.0]);
    lerp(lerp(a, control, s), lerp(control, b, s), s)
}

/// The tilted ring's plane normal for ring `i`: three tilts, each turned its own way about up.
fn ring_normal(i: u32) -> V3 {
    let k = (i % 3) as f32;
    let (tilt, yaw) = (0.3 + 0.45 * k, 2.1 * k);
    [tilt.sin() * yaw.cos(), tilt.cos(), tilt.sin() * yaw.sin()]
}

/// One particle, as thrown.
#[derive(Clone, Copy, Debug)]
struct Live {
    id: u32,
    emitter: usize,
    born: f32,
    p0: V3,
    v0: V3,
    goal: V3,
    arc: f32,
    phase: f32,
    normal: V3,
}

/// What one emitter on one target has thrown so far.
#[derive(Clone, Copy, Debug, Default)]
struct Spawner {
    emitter: usize,
    /// The target it plays on; 0 when it plays on none in particular.
    instance: usize,
    /// The cycle a burst or a prefill was thrown in.
    fired: Option<u32>,
    carry: f32,
}

/// A running effect.
#[derive(Clone, Debug)]
pub struct Sim {
    effect: Arc<Effect>,
    rng: Mulberry32,
    t: f32,
    /// When the effect ends: its length, or never for a lasting one until it is stopped.
    ends: f32,
    live: Vec<Live>,
    spawners: Vec<Spawner>,
    next_id: u32,
}

impl Sim {
    /// `effect` playing on `targets` targets, thrown from `seed`.
    pub fn new(effect: impl Into<Arc<Effect>>, seed: u32, targets: usize) -> Sim {
        let effect: Arc<Effect> = effect.into();
        let mut spawners = Vec::new();
        for (emitter, e) in effect.emitters.iter().enumerate() {
            for instance in 0..if e.per_target() { targets } else { 1 } {
                spawners.push(Spawner { emitter, instance, ..Spawner::default() });
            }
        }
        let ends = if effect.lasts { f32::INFINITY } else { effect.length };
        Sim { effect, rng: Mulberry32::new(seed), t: 0.0, ends, live: Vec::new(), spawners, next_id: 0 }
    }

    pub fn effect(&self) -> &Effect {
        &self.effect
    }

    /// Seconds played.
    pub fn time(&self) -> f32 {
        self.t
    }

    /// Particles thrown so far.
    pub fn spawned(&self) -> u32 {
        self.next_id
    }

    /// Whether it has ended and the last particle has faded.
    pub fn finished(&self) -> bool {
        self.t >= self.ends && self.live.is_empty()
    }

    /// Ends the effect now: nothing more is thrown, and the light sputters out over
    /// `sputter_out`. A lasting effect only ends this way.
    pub fn stop(&mut self) {
        self.ends = self.ends.min(self.t + self.effect.sputter_out.max(0.0));
    }

    fn rnd(&mut self) -> f32 {
        self.rng.next_f64() as f32
    }

    fn signed(&mut self) -> f32 {
        self.rnd() * 2.0 - 1.0
    }

    fn unit(&mut self) -> V3 {
        let (z, a) = (self.signed(), std::f32::consts::TAU * self.rnd());
        let r = (1.0 - z * z).max(0.0).sqrt();
        [r * a.cos(), z, r * a.sin()]
    }

    /// Steps `dt` seconds with the caster and targets at `places`: throws what is due and
    /// retires what has lived its life.
    pub fn step(&mut self, dt: f32, places: &Places) {
        let (t0, t1) = (self.t, self.t + dt.max(0.0));
        for s in 0..self.spawners.len() {
            let Spawner { emitter, instance, .. } = self.spawners[s];
            let e = &self.effect.emitters[emitter];
            let target = if e.per_target() { places.targets.get(instance).copied() } else { None };
            if e.per_target() && target.is_none() {
                continue;
            }
            let ctx = Ctx::new(places, target);
            self.throw(s, &ctx, t0, t1);
        }
        self.t = t1;
        let emitters = &self.effect.emitters;
        let t = self.t;
        self.live.retain(|p| t - p.born < emitters[p.emitter].timing.life.max(1e-3));
    }

    /// Throws what spawner `s` owes between `t0` and `t1`.
    fn throw(&mut self, s: usize, ctx: &Ctx, t0: f32, t1: f32) {
        let effect = Arc::clone(&self.effect);
        let e = &effect.emitters[self.spawners[s].emitter];
        let timing = e.timing;
        let length = self.effect.length.max(1e-3);
        let cycle = if self.effect.lasts { (t1 / length).floor() as u32 } else { 0 };
        let local = t1 - cycle as f32 * length;
        let origin = cycle as f32 * length;
        if t0 >= self.ends {
            return;
        }
        match timing.kind {
            TimingKind::Burst => {
                if self.spawners[s].fired != Some(cycle) && local >= timing.start && origin + timing.start < self.ends {
                    self.spawners[s].fired = Some(cycle);
                    for _ in 0..timing.count {
                        self.spawn(s, e, ctx, origin + timing.start);
                    }
                }
            }
            TimingKind::Stream | TimingKind::Lasting => {
                let rate = timing.count as f32 / timing.life.max(1e-3);
                let stop = if timing.end > 0.0 { timing.end.min(if self.effect.lasts { length } else { self.ends }) } else if self.effect.lasts { length } else { self.ends };
                if timing.kind == TimingKind::Lasting && self.spawners[s].fired.is_none() && local >= timing.start {
                    self.spawners[s].fired = Some(cycle);
                    for i in 0..timing.count {
                        let ago = timing.life * (i as f32 + self.rnd()) / timing.count.max(1) as f32;
                        self.spawn(s, e, ctx, t1 - ago);
                    }
                }
                let span = if self.effect.lasts {
                    if local >= timing.start && local <= stop && t1 <= self.ends { t1 - t0 } else { 0.0 }
                } else {
                    (t1.min(stop) - t0.max(timing.start)).max(0.0)
                };
                let carry = self.spawners[s].carry + span * rate;
                let n = carry.floor();
                self.spawners[s].carry = carry - n;
                for k in 0..n as u32 {
                    let born = t1 - (self.spawners[s].carry + (n as u32 - 1 - k) as f32) / rate;
                    self.spawn(s, e, ctx, born);
                }
            }
        }
    }

    /// Throws one particle of spawner `s`, born at `born`.
    fn spawn(&mut self, s: usize, e: &Emitter, ctx: &Ctx, born: f32) {
        let sp = e.spawn;
        let a = ctx.at(sp.at, sp.nudge);
        let goal = ctx.at(sp.to.unwrap_or(sp.at), if sp.to.is_some() { [0.0; 3] } else { sp.nudge });
        let phase = self.rnd() * std::f32::consts::TAU;
        let p0 = match sp.shape {
            Shape::Point => a,
            Shape::Ring => add(a, [sp.spread * phase.cos(), 0.0, sp.spread * phase.sin()]),
            Shape::Disc | Shape::Column => {
                let r = sp.spread * self.rnd().sqrt();
                let up = if sp.shape == Shape::Column { sp.height * self.rnd() } else { 0.0 };
                add(a, [r * phase.cos(), up, r * phase.sin()])
            }
            Shape::Box => {
                let (x, z, y) = (self.signed() * sp.spread, self.signed() * sp.spread, self.rnd() * sp.height);
                add(a, [x, y, z])
            }
            Shape::Path => {
                let along = arc_at(a, goal, sp.arc, self.rnd());
                add(along, mul(self.unit(), sp.spread))
            }
        };
        let m = e.motion;
        let jitter = |me: &mut Sim| [me.signed() * m.scatter, me.signed() * m.scatter * 0.5, me.signed() * m.scatter];
        let v0 = match m.kind {
            MotionKind::Still | MotionKind::Orbit | MotionKind::Gather | MotionKind::Path | MotionKind::Curl => [0.0; 3],
            MotionKind::Rise => add([0.0, m.speed, 0.0], jitter(self)),
            MotionKind::Fall => add([0.0, -m.speed, 0.0], jitter(self)),
            MotionKind::Burst => {
                let mut d = self.unit();
                if d[1] < 0.0 {
                    d[1] *= -0.3;
                }
                let k = m.speed * (0.5 + 0.5 * self.rnd());
                add(mul(d, k), jitter(self))
            }
            MotionKind::Drift => add(mul(self.unit(), m.speed), jitter(self)),
        };
        let id = self.next_id;
        self.next_id += 1;
        let normal = ring_normal(id);
        self.live.push(Live { id, emitter: self.spawners[s].emitter, born, p0, v0, goal, arc: sp.arc, phase, normal });
    }

    /// Where `p` is `age` seconds after it was thrown.
    fn place(&self, p: &Live, age: f32) -> V3 {
        let e = &self.effect.emitters[p.emitter];
        let m = e.motion;
        let life = e.timing.life.max(1e-3);
        let share = (age / life).clamp(0.0, 1.0);
        let kinetic = |down: f32| {
            let g = [0.0, -0.5 * m.gravity * age * age * down, 0.0];
            add(add(p.p0, mul(p.v0, age)), g)
        };
        let base = match m.kind {
            MotionKind::Still => p.p0,
            MotionKind::Rise | MotionKind::Fall | MotionKind::Burst | MotionKind::Drift => kinetic(1.0),
            MotionKind::Curl => {
                let w = p.phase + 3.0 * age;
                add(add(p.p0, [0.0, m.speed * age, 0.0]), [m.scatter * w.cos(), 0.0, m.scatter * w.sin()])
            }
            MotionKind::Orbit => {
                let angle = p.phase + std::f32::consts::TAU * m.orbits * share;
                let u = norm(cross(p.normal, [0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]);
                let w = cross(p.normal, u);
                add(add(p.p0, [0.0, m.speed * age, 0.0]), add(mul(u, m.orbit_radius * angle.cos()), mul(w, m.orbit_radius * angle.sin())))
            }
            MotionKind::Gather => lerp(p.p0, p.goal, share * share * (3.0 - 2.0 * share)),
            MotionKind::Path => arc_at(p.p0, p.goal, p.arc, share),
        };
        add(base, mul(m.wind, age))
    }

    /// Every live particle as it is now, in the order thrown.
    pub fn particles(&self) -> Vec<ParticleOut> {
        let mut out = Vec::new();
        self.particles_into(&mut out);
        out
    }

    /// [`Sim::particles`] into `out`, which is cleared first and so keeps its room.
    pub fn particles_into(&self, out: &mut Vec<ParticleOut>) {
        out.clear();
        out.extend(self.live.iter().map(|p| {
                let e = &self.effect.emitters[p.emitter];
                let life = e.timing.life.max(1e-3);
                let age = (self.t - p.born).max(0.0);
                let share = (age / life).clamp(0.0, 1.0);
                let pos = self.place(p, age);
                let ahead = self.place(p, age + EPS);
                let vel = mul(sub(ahead, pos), 1.0 / EPS);
                let fade_in = (share / FADE_IN).min(1.0);
                let fade_out = ((1.0 - share) / FADE_OUT).min(1.0);
                let smooth = |x: f32| x * x * (3.0 - 2.0 * x);
                ParticleOut {
                    id: p.id,
                    emitter: p.emitter,
                    card: e.card,
                    pos,
                    vel,
                    size: e.size_at(share),
                    colour: e.colour_at(share),
                    alpha: smooth(fade_in) * smooth(fade_out),
                    age,
                    life,
                    tail: e.tail,
                }
        }));
    }

    /// The effect's lights now, one on each target when the light is placed by the target.
    pub fn lights(&self, places: &Places) -> Vec<LightOut> {
        let mut out = Vec::new();
        self.lights_into(places, &mut out);
        out
    }

    /// [`Sim::lights`] into `out`, which is cleared first and so keeps its room.
    pub fn lights_into(&self, places: &Places, out: &mut Vec<LightOut>) {
        out.clear();
        let Some(light) = self.effect.light else { return };
        let length = self.effect.length.max(1e-3);
        let local = if self.effect.lasts { self.t % length } else { self.t };
        let left = if self.t >= self.ends {
            0.0
        } else if self.effect.sputter_out > 0.0 {
            ((self.ends - self.t) / self.effect.sputter_out).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let held = envelope::brightness(self.t, self.effect.bloom_in, left, self.t);
        let brightness = match light.follows {
            Follows::Envelope | Follows::Travel => held,
            Follows::Flash { at_s } => {
                let since = local - at_s;
                if since < 0.0 { 0.0 } else if since < FLASH_RISE { since / FLASH_RISE } else { (-(since - FLASH_RISE) / FLASH_FALL).exp() }
            }
        };
        let count = if light.at.on_target() { places.targets.len() } else { 1 };
        for i in 0..count {
            let ctx = Ctx::new(places, if light.at.on_target() { Some(places.targets[i]) } else { None });
            let there = ctx.at(light.at, [0.0; 3]);
            let pos = match light.follows {
                Follows::Travel => lerp(ctx.caster, there, (self.t / length).clamp(0.0, 1.0)),
                _ => there,
            };
            out.push(LightOut { pos, brightness });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{ColourStop, Light, Motion, SizeStop, Spawn, Timing};

    fn places(targets: usize) -> Places {
        Places { caster: [0.0, 16.0, 0.0], forward: [0.0, 0.0, -1.0], targets: (0..targets).map(|i| Target { feet: [i as f32 * 10.0, 0.0, -40.0], height: 10.0 }).collect() }
    }

    fn burst(count: u32) -> Effect {
        Effect {
            name: "burst".into(),
            length: 1.0,
            emitters: vec![Emitter {
                card: Card::Spark,
                spawn: Spawn { at: Anchor::TargetsFeet, ..Spawn::default() },
                timing: Timing { kind: TimingKind::Burst, count, life: 0.8, start: 0.0, end: 0.0 },
                motion: Motion { kind: MotionKind::Burst, speed: 20.0, gravity: 30.0, ..Motion::default() },
                ..Emitter::default()
            }],
            ..Effect::default()
        }
    }

    fn run(effect: &Effect, seed: u32, steps: usize) -> Vec<ParticleOut> {
        let p = places(1);
        let mut sim = Sim::new(effect.clone(), seed, 1);
        for _ in 0..steps {
            sim.step(1.0 / 30.0, &p);
        }
        sim.particles()
    }

    #[test]
    fn the_same_seed_and_steps_give_the_same_particles() {
        let e = burst(40);
        assert_eq!(run(&e, 7, 10), run(&e, 7, 10));
        assert_ne!(run(&e, 7, 10), run(&e, 8, 10));
    }

    #[test]
    fn a_burst_throws_its_count_on_each_target() {
        let mut sim = Sim::new(burst(25), 1, 3);
        sim.step(1.0 / 30.0, &places(3));
        assert_eq!(sim.spawned(), 75);
        assert_eq!(sim.particles().len(), 75);
        sim.step(1.0 / 30.0, &places(3));
        assert_eq!(sim.spawned(), 75, "a burst goes off once");
    }

    #[test]
    fn a_stream_throws_count_over_life_a_second() {
        let mut e = burst(20);
        e.length = 10.0;
        e.emitters[0].timing = Timing { kind: TimingKind::Stream, count: 20, life: 2.0, start: 0.0, end: 0.0 };
        let mut sim = Sim::new(e, 1, 1);
        for _ in 0..30 {
            sim.step(1.0 / 30.0, &places(1));
        }
        assert_eq!(sim.spawned(), 10, "a second at 20 over 2 s");
    }

    #[test]
    fn the_effect_ends_and_its_particles_fade() {
        let mut sim = Sim::new(burst(5), 1, 1);
        assert!(!sim.finished());
        for _ in 0..60 {
            sim.step(1.0 / 30.0, &places(1));
        }
        assert!(sim.finished());
    }

    #[test]
    fn a_lasting_effect_runs_until_stopped() {
        let mut e = burst(5);
        e.lasts = true;
        e.sputter_out = 0.5;
        let mut sim = Sim::new(e, 1, 1);
        for _ in 0..90 {
            sim.step(1.0 / 30.0, &places(1));
        }
        assert!(!sim.finished());
        assert_eq!(sim.spawned(), 15, "its burst goes off again each length");
        sim.stop();
        for _ in 0..60 {
            sim.step(1.0 / 30.0, &places(1));
        }
        assert!(sim.finished());
    }

    #[test]
    fn curves_run_straight_between_any_number_of_points() {
        let e = Emitter {
            size: vec![SizeStop { at: 0.0, size: 1.0 }, SizeStop { at: 0.5, size: 3.0 }, SizeStop { at: 0.75, size: 2.0 }, SizeStop { at: 1.0, size: 6.0 }],
            colour: vec![ColourStop { at: 0.2, rgb: [1.0, 0.0, 0.0] }, ColourStop { at: 0.6, rgb: [0.0, 1.0, 0.0] }],
            ..Emitter::default()
        };
        assert_eq!(e.size_at(0.25), 2.0);
        assert_eq!(e.size_at(0.625), 2.5);
        assert_eq!(e.size_at(0.875), 4.0);
        assert_eq!(e.size_at(2.0), 6.0, "held flat past the end");
        assert_eq!(e.colour_at(0.0), [1.0, 0.0, 0.0], "and before the start");
        assert!(e.colour_at(0.4).iter().zip([0.5, 0.5, 0.0]).all(|(a, b)| (a - b).abs() < 1e-5));
        assert_eq!(Emitter::default().size_at(0.3), 1.0);
    }

    #[test]
    fn an_orbit_keeps_its_radius_about_the_anchor() {
        let mut e = burst(6);
        e.emitters[0].spawn.at = Anchor::Target;
        e.emitters[0].motion = Motion { kind: MotionKind::Orbit, orbit_radius: 5.0, orbits: 2.0, ..Motion::default() };
        let mut sim = Sim::new(e, 3, 1);
        let p = places(1);
        let centre = Ctx::new(&p, Some(p.targets[0])).at(Anchor::Target, [0.0; 3]);
        for _ in 0..12 {
            sim.step(1.0 / 30.0, &p);
            for q in sim.particles() {
                let d = sub(q.pos, centre);
                assert!(((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - 5.0).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn a_path_arcs_from_the_caster_to_the_target() {
        let mut e = burst(1);
        e.emitters[0].spawn = Spawn { at: Anchor::Caster, to: Some(Anchor::Target), arc: 8.0, ..Spawn::default() };
        e.emitters[0].motion = Motion { kind: MotionKind::Path, ..Motion::default() };
        e.emitters[0].timing.life = 1.0;
        e.length = 2.0;
        let mut sim = Sim::new(e, 3, 1);
        let p = places(1);
        sim.step(0.5, &p);
        let mid = sim.particles()[0].pos;
        assert!(mid[1] > 16.0 - 8.0 + 4.0 && mid[2] < -10.0 && mid[2] > -30.0, "{mid:?}");
        sim.step(0.4999, &p);
        let end = sim.particles()[0].pos;
        assert!((end[2] + 40.0).abs() < 1.0, "{end:?}");
    }

    #[test]
    fn a_gather_draws_in_to_its_goal() {
        let mut e = burst(1);
        e.emitters[0].spawn = Spawn { shape: Shape::Ring, at: Anchor::Target, to: Some(Anchor::Caster), spread: 6.0, ..Spawn::default() };
        e.emitters[0].motion = Motion { kind: MotionKind::Gather, ..Motion::default() };
        e.emitters[0].timing.life = 1.0;
        e.length = 2.0;
        let mut sim = Sim::new(e, 3, 1);
        let p = places(1);
        sim.step(0.99, &p);
        let near = sim.particles()[0].pos;
        assert!(near[2].abs() < 1.0, "{near:?}");
    }

    #[test]
    fn the_light_blooms_and_a_flash_dies_away() {
        let mut e = burst(1);
        e.bloom_in = 0.5;
        e.light = Some(Light { at: Anchor::AboveTarget, peak: 1.0, reach: 100.0, colour: [1.0; 3], follows: Follows::Envelope });
        let mut sim = Sim::new(e.clone(), 1, 2);
        let p = places(2);
        assert_eq!(sim.lights(&p).len(), 2, "one on each target");
        sim.step(0.25, &p);
        let early = sim.lights(&p)[0].brightness;
        sim.step(0.4, &p);
        let late = sim.lights(&p)[0].brightness;
        assert!(early > 0.0 && late > early * 0.9, "{early} {late}");
        e.light.as_mut().unwrap().follows = Follows::Flash { at_s: 0.2 };
        let mut sim = Sim::new(e, 1, 1);
        sim.step(0.1, &places(1));
        assert_eq!(sim.lights(&places(1))[0].brightness, 0.0);
        sim.step(0.13, &places(1));
        let at = sim.lights(&places(1))[0].brightness;
        sim.step(0.5, &places(1));
        assert!(at > 0.9 && sim.lights(&places(1))[0].brightness < at * 0.3);
    }
}
