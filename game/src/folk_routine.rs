//! What folk do with their time: stand a while, then stroll somewhere near and stand again.
//!
//! - **Home.** Each person keeps the spot they were placed at (their [`fb_level::FolkSpot`]).
//!   They set out from it to a place in or just beside their room, and every second walk is
//!   the way back, so they are never far from it for long.
//! - **Places.** The light (a brazier or glowcaps, stood beside, never in), a stretch of wall,
//!   a doorway and a little way through it into the next room, or a spot out on the floor
//!   ([`Place`]). All are rolled from the person's seed (`procgen_rng::Mulberry32`), so a level's
//!   folk start out the same each run; only the timing of the frame varies.
//! - **Posts.** A guard ([`keeps_post`]) does not wander: placed at their claim's hall door facing
//!   out, they stand there, watch the delver who comes near like anyone, and walk back to the
//!   post after a hold or a fight.
//! - **Pausing.** A few seconds between walks, each from the tuning (`folk.pause_min` to
//!   `pause_max`), rolled per person.
//! - **Walking.** The way there is found once when a place is chosen (`fb_level::path::find_path`)
//!   and followed with `steer_toward`, never searched again on the way, and every step slides round
//!   rock, props, creatures, other folk and the player (`player::slide_around`), so none passes
//!   through them. They turn on the spot to face the way they go before setting off, and ease up
//!   to pace and down to a stop. The pace is the tuned `folk.walk_speed` for a human, slower in
//!   proportion to height for shorter folk. One that makes no headway for [`GIVE_UP_AFTER`]
//!   seconds (blocked in a doorway) gives up that walk and pauses.
//! - **Blocking.** Each person's [`BlocksTheWay`] circle rides on their root, and
//!   `play::gather_obstacles` reads it from the transform every frame, so the player and the
//!   creatures keep out of them where they stand now.
//! - **Gait.** [`Stride`] says how fast they actually moved this frame, for the walk animation.
//! - **Holding.** [`Routine::hold`] stops a person where they stand and [`Routine::release`] lets
//!   them carry on, resuming the walk that was cut short.
//! - **Noticing.** A person who comes within `folk.notice_distance` of the player, with a clear
//!   line of sight (`fb_level::path::line_of_sight`, as creatures notice), stops, turns their
//!   whole body to face the player and keeps facing them while they stay near (the head, in
//!   `folk::turn_heads`, leads and the body follows). Once the player has been further than
//!   `folk.release_distance` (or out of sight) for `folk.release_delay` seconds they go on with
//!   what they were doing. They stop well before contact, and the player cannot walk into them
//!   (their [`BlocksTheWay`] circle).
//!
//! - **Fighting.** A hostile folk (`crate::hostility`) leaves their routine where it stands and
//!   fights (`crate::folk_combat`); the others still keep out of their circle. One who gives up
//!   the fight walks back to their spot ([`Routine::head_home`]) and carries on from there.
//!
//! Every pace, range and wait here is our own tuning (`FolkTuning`) unless noted.

use bevy::prelude::*;
use fb_level::path::{find_path, line_of_sight, steer_toward};
use fb_level::{CELL, Cell, Level, LightKind};
use procgen_rng::Mulberry32;

use crate::creatures::Critter;
use crate::creatures::behaviour::body_radius;
use crate::folk::{Denizen, Stride};
use crate::hostility::Hostility;
use crate::level::geometry::Relief;
use crate::play::{BlockedCells, BlocksTheWay, CurrentLevel, DelveSystems, Obstacle, Player};
use crate::player::slide_around;
use crate::tuning::Tuning;

pub struct FolkRoutinePlugin;

impl Plugin for FolkRoutinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockedCells>().init_resource::<Tuning>().add_systems(Update, (begin_routines, walk_routines).chain().in_set(DelveSystems));
    }
}

/// Salt for a person's routine stream. Our own.
const ROUTINE_SALT: u32 = 0x40F7_1A2E;
/// Seconds without headway before a walk is given up.
pub const GIVE_UP_AFTER: f32 = 1.5;
/// Radians per second a person turns at, and how far off their way they may face and still walk.
const TURN_RATE: f32 = 4.5;
const FACING_ENOUGH: f32 = 0.7;
/// How quickly pace eases up and down, world units per second per second.
const EASE: f32 = 30.0;
/// How far a light is stood from, and the circle kept clear of a brazier.
const LIGHT_STAND_OFF: f32 = 7.0;
const BRAZIER_RADIUS: f32 = 3.5;
/// A person's reach on arrival, world units.
const ARRIVED: f32 = 1.0;
/// How far past a doorway the next room's place is, cells.
const THROUGH_THE_DOOR: f32 = 2.0;
/// A human's height from `data/races.json`, which pace is scaled against.
const HUMAN_HEIGHT: f32 = 17.2;
/// How many of the first places chosen a [`Routine`] keeps.
const KEEP_CHOSEN: usize = 8;

/// What a person is doing now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// Standing for `left` more seconds.
    Pause { left: f32 },
    /// Walking to `goal` (world `(x, z)`).
    Walk { goal: Vec2 },
    /// Stopped where they stand until [`Routine::release`] (someone has their attention).
    Held,
}

/// A kind of place a person walks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Light,
    Wall,
    Door,
    Floor,
}

/// A person's mind: where home is, what they are doing and the stream of places they choose.
#[derive(Component, Clone, Debug)]
pub struct Routine {
    pub phase: Phase,
    /// Where they were placed (world `(x, z)`) and the way they face there (unit, world `(x, z)`).
    pub home: Vec2,
    pub facing: Vec2,
    /// The first places chosen, kept to compare runs.
    pub chosen: Vec<Vec2>,
    /// How many walks were set out on, and how many times a way was searched for.
    pub walks: u32,
    pub searches: u32,
    /// The walk goal a hold cut short, resumed on release.
    resume: Option<Vec2>,
    /// Seconds the player has been out of reach while held.
    away: f32,
    /// How many holders have called [`Routine::hold`] and not yet [`Routine::release`]: the
    /// routine carries on only when the last lets go, so a greeting ending does not free a folk
    /// who is also talking.
    holds: u32,
    path: Vec<(i32, i32)>,
    stuck: f32,
    speed: f32,
    rng: Mulberry32,
}

impl Routine {
    /// Whether they are walking a way that goes through grid cell `cell`.
    pub fn heads_through(&self, cell: (i32, i32)) -> bool {
        matches!(self.phase, Phase::Walk { .. }) && self.path.contains(&cell)
    }

    /// A routine of someone placed at `home` facing `facing`, pausing `first` seconds before
    /// their first walk, drawing from `rng`.
    pub fn placed(home: Vec2, facing: Vec2, first: f32, rng: Mulberry32) -> Self {
        Routine { phase: Phase::Pause { left: first }, home, facing, chosen: Vec::new(), walks: 0, searches: 0, resume: None, away: 0.0, holds: 0, path: Vec::new(), stuck: 0.0, speed: 0.0, rng }
    }

    /// Sets the way they are walking (for tests).
    #[cfg(test)]
    pub fn walking(mut self, goal: Vec2, path: Vec<(i32, i32)>) -> Self {
        self.phase = Phase::Walk { goal };
        self.path = path;
        self
    }

    /// Stops them where they are. Each hold wants its own [`Routine::release`].
    pub fn hold(&mut self) {
        self.holds += 1;
        if let Phase::Walk { goal } = self.phase {
            self.resume = Some(goal);
        }
        self.phase = Phase::Held;
        self.away = 0.0;
    }

    /// Drops whatever they were doing for a short pause, after which they walk back to their
    /// spot (a pause away from home always ends in the walk back): for one coming back to their
    /// routine from somewhere else, as a folk who gave up a fight does.
    pub fn head_home(&mut self) {
        self.holds = 0;
        self.resume = None;
        self.path.clear();
        self.speed = 0.0;
        self.stuck = 0.0;
        self.phase = Phase::Pause { left: 0.5 };
    }

    /// Lets a held person carry on: back to the walk they were on, or else a short pause and a
    /// new walk.
    pub fn release(&mut self) {
        self.holds = self.holds.saturating_sub(1);
        if self.holds == 0 && self.phase == Phase::Held {
            self.phase = match self.resume.take() {
                Some(goal) => {
                    self.stuck = 0.0;
                    Phase::Walk { goal }
                }
                None => Phase::Pause { left: 0.5 },
            };
        }
    }
}

/// A person's pace, world units per second: the tuned human pace, in proportion to height for
/// shorter folk.
pub fn pace(denizen: &Denizen, walk_speed: f32) -> f32 {
    walk_speed * (denizen.person.spec.folk.race().h / HUMAN_HEIGHT).min(1.0)
}

/// Whether this person is a guard, who stands at their post instead of wandering: they idle
/// there facing out, turn to watch the delver who comes near (the notice above), and walk back
/// to the post after anything that moved them.
pub fn keeps_post(denizen: &Denizen) -> bool {
    denizen.person.spot.trade.is_some_and(|t| t.profession == fb_faction::Profession::Guard)
}

fn grid(p: Vec2) -> (f32, f32) {
    (p.x / CELL, p.y / CELL)
}

fn cell(p: Vec2) -> (i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

/// Gives each new person a routine (a body brought back from the record has none).
#[allow(clippy::type_complexity)]
pub(crate) fn begin_routines(mut commands: Commands, tuning: Res<Tuning>, new: Query<(Entity, &Denizen), (Added<Denizen>, Without<crate::combat::feedback::Corpse>)>) {
    for (entity, denizen) in &new {
        let mut rng = Mulberry32::new(denizen.person.spot.seed ^ ROUTINE_SALT);
        let first = rng.range_f32(0.0, tuning.folk.pause_max.max(tuning.folk.pause_min));
        // Home is their spot, even for someone brought back from the record somewhere else.
        let placed = crate::folk::stand_at(&denizen.person.spot);
        let facing = (placed.rotation * Vec3::Z).xz().normalize_or_zero();
        commands.entity(entity).insert(Routine {
            phase: Phase::Pause { left: first },
            home: placed.translation.xz(),
            facing,
            chosen: Vec::new(),
            walks: 0,
            searches: 0,
            resume: None,
            away: 0.0,
            holds: 0,
            path: Vec::new(),
            stuck: 0.0,
            speed: 0.0,
            rng,
        });
    }
}

/// Whether a circle of `radius` at `p` stands on open floor clear of rock, and of `keep_out`.
fn standable(level: &Level, p: Vec2, radius: f32, keep_out: &[Obstacle]) -> bool {
    let (cx, cy) = cell(p);
    if !level.is_open(cx, cy) {
        return false;
    }
    for dy in -1..=1 {
        for dx in -1..=1 {
            if !level.is_open(cx + dx, cy + dy) {
                let (min, max) = (Vec2::new((cx + dx) as f32, (cy + dy) as f32) * CELL, Vec2::new((cx + dx + 1) as f32, (cy + dy + 1) as f32) * CELL);
                if p.distance(p.clamp(min, max)) < radius {
                    return false;
                }
            }
        }
    }
    !keep_out.iter().any(|o| o.overlaps(p, radius))
}

/// A place for someone from `home` to walk to, or `None` if the draws found none. Everything it
/// reads is the level's own (rock, rooms, lights, props), so it is a pure function of the
/// stream.
fn choose_place(level: &Level, home: Vec2, radius: f32, wander: f32, keep_out: &[Obstacle], rng: &mut Mulberry32) -> Option<Vec2> {
    let home_cell = cell(home);
    let room = level.room_at(home_cell.0, home_cell.1).map(|i| level.rooms[i]);
    let margin = radius + 2.0;
    for _ in 0..8 {
        let place = *rng.weighted(&[(Place::Light, 3), (Place::Wall, 2), (Place::Door, 3), (Place::Floor, 2)]);
        let at = match (place, room) {
            (Place::Light, _) => {
                let lights: Vec<Vec2> = level.lights.iter().map(|l| Vec2::new(l.x, l.y) * CELL).filter(|l| l.distance(home) <= wander).collect();
                if lights.is_empty() {
                    continue;
                }
                let light = *rng.pick(&lights);
                light + Vec2::from_angle(rng.range_f32(0.0, std::f32::consts::TAU)) * LIGHT_STAND_OFF
            }
            (_, None) => continue,
            (Place::Wall, Some(room)) => {
                let (lo, hi) = inset(&room, margin);
                let (along_x, along_y) = (rng.range_f32(lo.x, hi.x), rng.range_f32(lo.y, hi.y));
                match rng.int(0, 3) {
                    0 => Vec2::new(along_x, lo.y),
                    1 => Vec2::new(along_x, hi.y),
                    2 => Vec2::new(lo.x, along_y),
                    _ => Vec2::new(hi.x, along_y),
                }
            }
            (Place::Floor, Some(room)) => {
                let (lo, hi) = inset(&room, margin);
                Vec2::new(rng.range_f32(lo.x, hi.x), rng.range_f32(lo.y, hi.y))
            }
            (Place::Door, Some(room)) => {
                // Doors of the room: a door cell beside one of its cells; out through it.
                let mut doors = Vec::new();
                for y in room.y as i32 - 1..=(room.y + room.h) as i32 {
                    for x in room.x as i32 - 1..=(room.x + room.w) as i32 {
                        if level.cell(x, y) != Cell::Door || room.contains(x, y) {
                            continue;
                        }
                        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                            if room.contains(x + dx, y + dy) {
                                doors.push((Vec2::new(x as f32 + 0.5, y as f32 + 0.5) * CELL, Vec2::new(-dx as f32, -dy as f32)));
                            }
                        }
                    }
                }
                if doors.is_empty() {
                    continue;
                }
                let &(door, out) = rng.pick(&doors);
                // As far through as stands clear, else the doorway itself.
                [THROUGH_THE_DOOR, 1.0, 0.0].into_iter().map(|cells| door + out * cells * CELL).find(|p| standable(level, *p, radius, keep_out)).unwrap_or(door)
            }
        };
        if at.distance(home) <= wander && standable(level, at, radius, keep_out) {
            return Some(at);
        }
    }
    None
}

/// The corners of `room` in world units, drawn in by `margin` (to its middle if it is narrower).
fn inset(room: &fb_level::Room, margin: f32) -> (Vec2, Vec2) {
    let (lo, hi) = (Vec2::new(room.x as f32, room.y as f32) * CELL, Vec2::new((room.x + room.w) as f32, (room.y + room.h) as f32) * CELL);
    let m = Vec2::splat(margin).min((hi - lo) * 0.45);
    (lo + m, hi - m)
}

/// One frame of every person's routine: pause, choose a place and its way, then step along it
/// round whatever is in the way, turning to face where they go.
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn walk_routines(
    time: Res<Time>,
    level: Res<CurrentLevel>,
    tuning: Res<Tuning>,
    blocked_cells: Res<BlockedCells>,
    doors: Option<Res<crate::doors::Doors>>,
    player: Query<&Transform, (With<Player>, Without<Denizen>)>,
    creatures: Query<(&Critter, &Transform), Without<Denizen>>,
    props: Query<(&BlocksTheWay, &GlobalTransform), Without<Denizen>>,
    mut folk: Query<(Entity, &Denizen, &mut Routine, &mut Stride, &mut Transform, Option<&Hostility>, Has<crate::folk_regard::Attending>, Has<crate::folk_regard::Assisting>)>,
) {
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }
    let player_at = player.iter().next().map(|t| t.translation.xz());
    let level = &level.0;
    let tune = &tuning.folk;
    // What stops a person (rock, and shut doors but their own faction's) is made per person below.
    // What every person keeps out of that stays put: props and braziers. Where they choose to
    // go leaves out the creatures and the player, which move, so the choice stays the seed's.
    let mut fixed: Vec<Obstacle> = props.iter().map(|(blocks, at)| Obstacle::of(blocks.0, at)).collect();
    fixed.extend(level.lights.iter().filter(|l| l.kind == LightKind::Brazier).map(|l| Obstacle::Circle { centre: Vec2::new(l.x, l.y) * CELL, radius: BRAZIER_RADIUS }));
    let mut moving: Vec<Obstacle> = creatures.iter().map(|(c, t)| Obstacle::Circle { centre: t.translation.xz(), radius: body_radius(c) }).collect();
    moving.extend(player.iter().next().map(|t| Obstacle::Circle { centre: t.translation.xz(), radius: tuning.moving.body_radius }));
    // Everything a walker keeps out of, built once: the fixed and moving obstacles, then every
    // person's circle (each walker sets their own aside while they step).
    let mut shared = fixed.clone();
    shared.extend(moving.iter().copied());
    let mut body_of: Vec<(Entity, usize)> = Vec::new();
    for (e, d, _, _, t, _, _, _) in folk.iter() {
        body_of.push((e, shared.len()));
        shared.push(Obstacle::Circle { centre: t.translation.xz(), radius: d.radius });
    }
    let relief = Relief::new(level);

    for (entity, denizen, mut routine, mut stride, mut transform, hostility, attending, assisting) in &mut folk {
        let blocked = crate::doors::solid_for(level, &blocked_cells, doors.as_deref(), Some(denizen.person.faction.id));
        let see = crate::doors::opaque_for(level, &blocked_cells, doors.as_deref(), Some(denizen.person.faction.id));
        // A hostile folk has left their routine to fight (`crate::folk_combat::fight`), though
        // the others still keep out of where they stand.
        // Likewise an ally who has joined the delver's fight (`crate::folk_regard`).
        if hostility.is_some_and(|h| h.is_hostile()) || assisting {
            continue;
        }
        let routine = &mut *routine;
        let at = transform.translation.xz();
        let radius = denizen.radius;
        let mut face: Option<Vec2> = None;
        let mut moved = 0.0;
        let pause = |rng: &mut Mulberry32| Phase::Pause { left: rng.range_f32(tune.pause_min, tune.pause_max.max(tune.pause_min)) };

        // Notice the player, and let go of them once they are well away.
        let near = player_at.map(|p| (p, p.distance(at)));
        let sees = |p: Vec2| line_of_sight(grid(at), grid(p), &see);
        match (routine.phase, near) {
            // Watching or greeting the delver (`crate::folk_regard`) keeps them held.
            (Phase::Held, Some(_)) if attending => routine.away = 0.0,
            (Phase::Held, Some((p, d))) => {
                if d > tune.release_distance || !sees(p) {
                    routine.away += dt;
                    if routine.away >= tune.release_delay {
                        routine.release();
                    }
                } else {
                    routine.away = 0.0;
                }
            }
            (Phase::Held, None) => routine.release(),
            (_, Some((p, d))) if d <= tune.notice_distance && sees(p) => routine.hold(),
            _ => {}
        }

        match routine.phase {
            Phase::Held => {
                routine.speed = 0.0;
                face = player_at;
            }
            Phase::Pause { left } => {
                routine.speed = 0.0;
                let away = at.distance(routine.home) > radius + 2.0 * ARRIVED;
                if !away {
                    face = Some(at + routine.facing * CELL);
                }
                if left > dt {
                    routine.phase = Phase::Pause { left: left - dt };
                } else {
                    // Away from home, the next walk is the way back; at home, somewhere new.
                    // A guard keeps their post: they never choose a place to stroll to.
                    let goal = if away { Some(routine.home) } else if keeps_post(denizen) { None } else { choose_place(level, routine.home, radius, tune.wander_radius, &fixed, &mut routine.rng) };
                    let way = goal.and_then(|g| {
                        routine.searches += 1;
                        find_path(cell(at), cell(g), 4000, &blocked).map(|path| (g, path))
                    });
                    match way {
                        Some((goal, path)) => {
                            routine.path = path;
                            routine.stuck = 0.0;
                            routine.walks += 1;
                            if routine.chosen.len() < KEEP_CHOSEN {
                                routine.chosen.push(goal);
                            }
                            routine.phase = Phase::Walk { goal };
                        }
                        None => routine.phase = pause(&mut routine.rng),
                    }
                }
            }
            Phase::Walk { goal } => {
                let remaining = goal.distance(at);
                if remaining < ARRIVED || routine.stuck > GIVE_UP_AFTER {
                    routine.phase = pause(&mut routine.rng);
                    routine.path.clear();
                    routine.speed = 0.0;
                } else {
                    let (x, y) = steer_toward(grid(at), grid(goal), &routine.path, radius / CELL, &blocked);
                    let target = Vec2::new(x, y) * CELL;
                    let direction = (target - at).normalize_or_zero();
                    face = Some(target);
                    // Turn first, then go.
                    let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
                    let off = angle_between(yaw, direction.x.atan2(direction.y));
                    if direction != Vec2::ZERO && off.abs() < FACING_ENOUGH {
                        let wanted = pace(denizen, tune.walk_speed) * (remaining / (0.5 * CELL)).clamp(0.3, 1.0);
                        routine.speed += (wanted - routine.speed).clamp(-EASE * dt, EASE * dt);
                        let step = (direction * routine.speed * dt).clamp_length_max((target - at).length());
                        let own = body_of.iter().find(|(e, _)| *e == entity).map(|&(_, i)| i);
                        let kept = own.map(|i| std::mem::replace(&mut shared[i], Obstacle::Circle { centre: Vec2::splat(f32::MAX), radius: 0.0 }));
                        let to = slide_around(at, at + step, radius, &blocked, &shared);
                        if let (Some(i), Some(kept)) = (own, kept) {
                            shared[i] = kept;
                        }
                        let made = (to - at).dot(direction);
                        routine.stuck = if made < 0.25 * routine.speed * dt { routine.stuck + dt } else { 0.0 };
                        moved = (to - at).length();
                        transform.translation = Vec3::new(to.x, relief.floor_height(to.x, to.y), to.y);
                    } else {
                        routine.speed = 0.0;
                    }
                }
            }
        }
        stride.speed = moved / dt;

        // Turn toward where they go, or at home, the way they were placed.
        if let Some(look) = face {
            let to = look - transform.translation.xz();
            if to.length_squared() > 1e-4 {
                let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
                let turn = angle_between(yaw, to.x.atan2(to.y));
                transform.rotation = Quat::from_rotation_y(yaw + turn.clamp(-TURN_RATE * dt, TURN_RATE * dt));
            }
        }
    }
}

/// The signed turn from yaw `from` to yaw `to`, radians, the short way round.
fn angle_between(from: f32, to: f32) -> f32 {
    (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creatures::behaviour::BehaviourPlugin;
    use crate::creatures::spawn_skeleton;
    use crate::play::{Obstacles, gather_obstacles};
    use bevy::time::TimeUpdateStrategy;

    /// A dressed figure's footprint, near enough (`folk::footprint` is about 3 on 1207).
    const FOOTPRINT: f32 = 3.0;

    /// Seed 1207's folk as stand-ins (no drawing), props, creatures and a player, stepped 1/60 s
    /// a frame, with the routines and the creatures' minds running and the obstacles gathered as
    /// the game does.
    fn level_app(player_at: Vec2) -> (App, Vec<Entity>) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .add_plugins((BehaviourPlugin, FolkRoutinePlugin))
            .add_systems(PostUpdate, gather_obstacles.after(TransformSystems::Propagate));
        let level = fb_level::generate(1207, 1);
        app.insert_resource(CurrentLevel(level.clone()));
        app.world_mut().spawn((Player, Transform::from_xyz(player_at.x, 0.0, player_at.y)));
        for p in &level.props {
            let at = Transform::from_translation(Vec3::from_array(Level::world(p.x, p.y))).with_rotation(Quat::from_rotation_y(p.yaw));
            app.world_mut().spawn((BlocksTheWay(p.prop.floor_shape()), at));
        }
        let folk: Vec<Entity> = level
            .folk
            .iter()
            .map(|spot| {
                let denizen = Denizen { person: crate::folk::person_1207(spot), radius: FOOTPRINT, min: Vec3::ZERO, max: Vec3::ZERO };
                app.world_mut().spawn((denizen, BlocksTheWay(fb_level::FloorShape::Round(FOOTPRINT)), Stride::default(), crate::folk::stand_at(spot))).id()
            })
            .collect();
        {
            let mut commands = app.world_mut().commands();
            for spot in &level.creatures {
                spawn_skeleton(&mut commands, &level, spot, &spot.creature().rig());
            }
        }
        app.world_mut().flush();
        (app, folk)
    }

    fn position(app: &App, entity: Entity) -> Vec2 {
        app.world().get::<Transform>(entity).unwrap().translation.xz()
    }

    /// Steps a minute and more, calling `each` after every frame.
    fn run(app: &mut App, frames: usize, mut each: impl FnMut(&mut App, usize)) {
        for frame in 0..frames {
            app.update();
            each(app, frame);
        }
    }

    #[test]
    fn folk_walk_about_over_a_minute() {
        let (mut app, folk) = level_app(Vec2::new(1.5, 22.5) * CELL);
        let homes: Vec<Vec2> = folk.iter().map(|&f| position(&app, f)).collect();
        let mut away = vec![0.0f32; folk.len()];
        let mut fastest = 0.0f32;
        run(&mut app, 60 * 60, |app, _| {
            for (i, &f) in folk.iter().enumerate() {
                away[i] = away[i].max(position(app, f).distance(homes[i]));
                fastest = fastest.max(app.world().get::<Stride>(f).unwrap().speed);
            }
        });
        let left = away.iter().filter(|&&d| d > 2.0 * CELL * 0.25).count();
        assert!(left * 2 > folk.len(), "{left} of {} left their spots: {away:?}", folk.len());
        let walk = crate::tuning::FolkTuning::default().walk_speed;
        assert!(fastest > 4.0 && fastest <= walk + 0.5, "fastest pace {fastest}");
    }

    #[test]
    fn guards_keep_their_post_and_the_rest_wander() {
        let (mut app, folk) = level_app(Vec2::new(1.5, 22.5) * CELL);
        let homes: Vec<Vec2> = folk.iter().map(|&f| position(&app, f)).collect();
        let guard: Vec<bool> = folk.iter().map(|&f| keeps_post(app.world().get::<Denizen>(f).unwrap())).collect();
        assert!(guard.iter().any(|&g| g), "seed 1207 has no guard");
        let mut away = vec![0.0f32; folk.len()];
        run(&mut app, 60 * 120, |app, _| {
            for (i, &f) in folk.iter().enumerate() {
                away[i] = away[i].max(position(app, f).distance(homes[i]));
            }
        });
        for (i, &f) in folk.iter().enumerate() {
            if guard[i] {
                assert!(away[i] < 0.5, "guard {i} left their post by {}", away[i]);
                assert_eq!(app.world().get::<Routine>(f).unwrap().walks, 0);
            }
        }
        assert!(guard.iter().zip(&away).any(|(&g, &d)| !g && d > CELL * 0.5), "no one else wandered: {away:?}");
    }

    #[test]
    fn folk_never_pass_through_walls_props_creatures_each_other_or_the_player() {
        let level = fb_level::generate(1207, 1);
        let props: Vec<Obstacle> = level
            .props
            .iter()
            .map(|p| Obstacle::of(p.prop.floor_shape(), &GlobalTransform::from(Transform::from_translation(Vec3::from_array(Level::world(p.x, p.y))).with_rotation(Quat::from_rotation_y(p.yaw)))))
            .collect();
        // The player stands where someone is going: the first place a person chose, found by
        // running the level once without them (the places are the seed's, so they are the same).
        let player = {
            let (mut probe, folk) = level_app(Vec2::new(1.5, 22.5) * CELL);
            run(&mut probe, 60 * 30, |_, _| {});
            folk.iter().find_map(|&f| probe.world().get::<Routine>(f).unwrap().chosen.first().copied()).expect("someone chose a place")
        };
        let (mut app, folk) = level_app(player);
        let mut critters = app.world_mut().query::<(&Critter, &Transform)>();
        let mut nearest = f32::MAX;
        run(&mut app, 60 * 60, |app, frame| {
            let at: Vec<Vec2> = folk.iter().map(|&f| position(app, f)).collect();
            let creatures: Vec<(Vec2, f32)> = critters.iter(app.world()).map(|(c, t)| (t.translation.xz(), body_radius(c))).collect();
            for (i, &p) in at.iter().enumerate() {
                let (gx, gy) = (p.x / CELL, p.y / CELL);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (x, y) = (gx.floor() as i32 + dx, gy.floor() as i32 + dy);
                        if !level.is_open(x, y) {
                            let (min, max) = (Vec2::new(x as f32, y as f32) * CELL, Vec2::new(x as f32 + 1.0, y as f32 + 1.0) * CELL);
                            assert!(p.distance(p.clamp(min, max)) >= FOOTPRINT - 0.05, "folk {i} in the wall at {p} (frame {frame})");
                        }
                    }
                }
                for o in &props {
                    assert!(!o.overlaps(p, FOOTPRINT - 0.05), "folk {i} at {p} walks into {o:?} (frame {frame})");
                }
                for (j, &q) in at.iter().enumerate().skip(i + 1) {
                    assert!(p.distance(q) >= 2.0 * FOOTPRINT - 0.1, "folk {i} and {j} overlap at {p} (frame {frame})");
                }
                for &(c, r) in &creatures {
                    assert!(p.distance(c) >= FOOTPRINT + r - 0.1, "folk {i} at {p} overlaps a creature at {c} (frame {frame})");
                }
                assert!(p.distance(player) >= FOOTPRINT + 3.5 - 0.1, "folk {i} at {p} overlaps the player (frame {frame})");
                nearest = nearest.min(p.distance(player));
            }
        });
        let notice = crate::tuning::FolkTuning::default().notice_distance;
        assert!(nearest <= notice + 5.0, "no one came near the player (nearest {nearest})");
    }

    #[test]
    fn each_returns_near_home() {
        let (mut app, folk) = level_app(Vec2::new(1.5, 22.5) * CELL);
        let homes: Vec<Vec2> = folk.iter().map(|&f| position(&app, f)).collect();
        // Out and back at least once, in the time of a couple of walks each way.
        let mut back = vec![false; folk.len()];
        let mut out = vec![false; folk.len()];
        run(&mut app, 60 * 150, |app, _| {
            for (i, &f) in folk.iter().enumerate() {
                let d = position(app, f).distance(homes[i]);
                if d > CELL * 0.5 {
                    out[i] = true;
                } else if out[i] && d < 2.0 {
                    back[i] = true;
                }
            }
        });
        let went = out.iter().filter(|&&o| o).count();
        assert!(went > 0);
        for (i, &o) in out.iter().enumerate() {
            assert!(!o || back[i], "folk {i} never came back to {}", homes[i]);
        }
    }

    #[test]
    fn two_runs_choose_the_same_first_places() {
        let first = |ms: u64| {
            let (mut app, folk) = level_app(Vec2::new(1.5, 22.5) * CELL);
            // A different frame rate changes the timing, not the places.
            app.insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(ms)));
            let frames = (60_000 / ms) as usize;
            run(&mut app, frames, |_, _| {});
            folk.iter().map(|&f| app.world().get::<Routine>(f).unwrap().chosen.iter().take(1).copied().collect::<Vec<_>>()).collect::<Vec<_>>()
        };
        let a = first(16);
        let b = first(16);
        assert_eq!(a, b);
        assert!(a.iter().any(|c| !c.is_empty()));
        let c = first(25);
        for (x, y) in a.iter().zip(&c) {
            if let (Some(p), Some(q)) = (x.first(), y.first()) {
                assert_eq!(p, q, "a different frame rate gave a different first place");
            }
        }
    }

    #[test]
    fn paths_are_searched_only_when_a_place_is_chosen() {
        let (mut app, folk) = level_app(Vec2::new(1.5, 22.5) * CELL);
        run(&mut app, 60 * 60, |_, _| {});
        for &f in &folk {
            let r = app.world().get::<Routine>(f).unwrap();
            // One search per walk set out on, and the odd choice that found no way.
            assert!(r.searches <= r.walks + 3, "{} searches for {} walks", r.searches, r.walks);
            assert!(r.searches <= 60, "{} searches in a minute", r.searches);
        }
    }

    #[test]
    fn folk_obstacles_follow_them() {
        let (mut app, folk) = level_app(Vec2::new(1.5, 22.5) * CELL);
        run(&mut app, 60 * 30, |_, _| {});
        let obstacles = app.world().resource::<Obstacles>().0.clone();
        for &f in &folk {
            let p = position(&app, f);
            assert!(obstacles.iter().any(|o| matches!(o, Obstacle::Circle { centre, radius } if centre.distance(p) < 0.5 && *radius == FOOTPRINT)), "no obstacle at {p}");
        }
    }

    #[test]
    fn shorter_folk_walk_slower() {
        // Folk of several worlds, so every folk's height is there to compare.
        let tall = (1..=8)
            .flat_map(|world| {
                let level = fb_level::generate(world, 1);
                level.folk.iter().map(|s| crate::folk::Person::of(s, level.faction(s.faction), 1)).collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mk = |p: &crate::folk::Person| Denizen { person: p.clone(), radius: 3.0, min: Vec3::ZERO, max: Vec3::ZERO };
        for a in &tall {
            for b in &tall {
                if a.spec.folk.race().h < b.spec.folk.race().h {
                    // Strictly slower unless both are as tall as a human, whose pace is the cap.
                    let capped = a.spec.folk.race().h >= HUMAN_HEIGHT;
                    assert!(if capped { pace(&mk(a), 11.0) <= pace(&mk(b), 11.0) } else { pace(&mk(a), 11.0) < pace(&mk(b), 11.0) });
                }
            }
        }
        assert!(tall.iter().all(|p| pace(&mk(p), 11.0) <= 11.0));
    }

    fn set_player(app: &mut App, at: Vec2) {
        let mut q = app.world_mut().query_filtered::<&mut Transform, With<Player>>();
        for mut t in q.iter_mut(app.world_mut()) {
            t.translation = Vec3::new(at.x, 0.0, at.y);
        }
    }

    fn phase(app: &App, e: Entity) -> Phase {
        app.world().get::<Routine>(e).unwrap().phase
    }

    fn yaw_to(app: &App, e: Entity, target: Vec2) -> f32 {
        let (yaw, _, _) = app.world().get::<Transform>(e).unwrap().rotation.to_euler(EulerRot::YXZ);
        let to = target - position(app, e);
        angle_between(yaw, to.x.atan2(to.y)).abs()
    }

    /// Runs until some person is walking and returns them with a spot `distance` from them, in
    /// the open and in sight, to put the player at.
    fn walker_with_player_near(app: &mut App, folk: &[Entity], distance: f32) -> (Entity, Vec2) {
        let level = fb_level::generate(1207, 1);
        let blocked = |x: i32, y: i32| !level.is_open(x, y);
        for _ in 0..60 * 30 {
            app.update();
            for &f in folk {
                if let Phase::Walk { .. } = phase(app, f) {
                    let at = position(app, f);
                    for k in 0..16 {
                        let p = at + Vec2::from_angle(k as f32 * 0.39) * distance;
                        if level.is_open((p.x / CELL) as i32, (p.y / CELL) as i32) && line_of_sight((at.x / CELL, at.y / CELL), (p.x / CELL, p.y / CELL), blocked) {
                            return (f, p);
                        }
                    }
                }
            }
        }
        panic!("nobody walked with room for the player");
    }

    #[test]
    fn a_walking_person_stops_and_faces_a_near_player() {
        let (mut app, folk) = level_app(Vec2::new(1.5, 22.5) * CELL);
        let (f, p) = walker_with_player_near(&mut app, &folk, 20.0);
        set_player(&mut app, p);
        run(&mut app, 90, |_, _| {});
        assert_eq!(phase(&app, f), Phase::Held);
        assert!(yaw_to(&app, f, p) < 0.1, "off by {}", yaw_to(&app, f, p));
        assert_eq!(app.world().get::<Stride>(f).unwrap().speed, 0.0);
        // Still facing them as they move about nearby.
        let at = position(&app, f);
        let q = at + (p - at).perp().normalize() * 20.0;
        set_player(&mut app, q);
        run(&mut app, 90, |_, _| {});
        assert_eq!(phase(&app, f), Phase::Held);
        assert!(yaw_to(&app, f, q) < 0.1);
    }

    #[test]
    fn a_person_resumes_the_walk_after_the_player_leaves() {
        let (mut app, folk) = level_app(Vec2::new(1.5, 22.5) * CELL);
        let (f, p) = walker_with_player_near(&mut app, &folk, 20.0);
        let Phase::Walk { goal } = phase(&app, f) else { unreachable!() };
        set_player(&mut app, p);
        run(&mut app, 60, |_, _| {});
        assert_eq!(phase(&app, f), Phase::Held);
        // Between the notice and release distances they stay put.
        let at = position(&app, f);
        set_player(&mut app, at + (p - at).normalize() * 34.0);
        run(&mut app, 120, |_, _| {});
        assert_eq!(phase(&app, f), Phase::Held);
        // Far away, they carry on to the same place.
        set_player(&mut app, Vec2::new(1.5, 22.5) * CELL);
        run(&mut app, 90, |_, _| {});
        assert_eq!(phase(&app, f), Phase::Walk { goal });
        let held_at = position(&app, f);
        run(&mut app, 60, |_, _| {});
        assert!(position(&app, f).distance(held_at) > 2.0, "did not move on");
    }

    #[test]
    fn folk_never_overlap_a_player_walking_straight_at_them() {
        let level = fb_level::generate(1207, 1);
        let (mut app, folk) = level_app(Vec2::new(1.5, 22.5) * CELL);
        let (f, p) = walker_with_player_near(&mut app, &folk, 30.0);
        let mut player = p;
        let mut closest = f32::MAX;
        run(&mut app, 60 * 6, |app, _| {
            let at = position(app, f);
            // 14 units per second, straight at them, as far as the floor is open.
            let next = player + (at - player).normalize_or_zero() * 14.0 / 60.0;
            let others: Vec<Obstacle> = folk.iter().map(|&g| Obstacle::Circle { centre: position(app, g), radius: FOOTPRINT }).collect();
            player = slide_around(player, next, 3.5, |x: i32, y: i32| !level.is_open(x, y), &others);
            set_player(app, player);
            for &g in &folk {
                closest = closest.min(position(app, g).distance(player));
            }
        });
        assert!(closest >= FOOTPRINT + 3.5 - 0.1, "closest {closest}");
    }

    #[test]
    fn a_player_behind_a_wall_is_not_noticed() {
        let (mut app, folk) = level_app(Vec2::new(1.5, 22.5) * CELL);
        let (f, p) = walker_with_player_near(&mut app, &folk, 26.0);
        // The cell the player stands in is shut off, as by a door, so the way is not clear.
        let (from, to) = (cell(position(&app, f)), cell(p));
        assert_ne!(from, to);
        let wall: bevy::platform::collections::HashSet<(i32, i32)> = [to].into_iter().collect();
        app.insert_resource(BlockedCells(wall));
        set_player(&mut app, p);
        run(&mut app, 60, |app, _| assert_ne!(phase(app, f), Phase::Held));
        // The same spot with the way clear is noticed.
        app.insert_resource(BlockedCells::default());
        let at = position(&app, f);
        set_player(&mut app, at + (p - at).normalize() * 20.0);
        run(&mut app, 60, |_, _| {});
        assert_eq!(phase(&app, f), Phase::Held);
    }
}
