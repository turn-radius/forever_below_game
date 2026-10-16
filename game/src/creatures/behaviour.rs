//! What creatures do: wander their room, notice the player, close in round walls and props,
//! and bite or strike when in reach.
//!
//! - **Wandering.** A creature idles a while, then walks to a point in its home room (the room
//!   its spot is in) and idles again. Rats potter; a snake lies coiled most of the time and
//!   now and then slithers a little way, never far from its spot.
//! - **Noticing.** It notices the player within its sense range ([`notice_range`]) when the
//!   straight line between them crosses no rock (`fb_level::path::line_of_sight`).
//! - **Closing in.** In sight, it heads straight for the player; out of sight it follows the
//!   shortest way through the grid to where it last saw them (`fb_level::path::find_path`,
//!   cut short by `steer_toward`). Every step slides round rock, folk, props, the player and
//!   other creatures (`player::slide_around` with `play::Obstacles` and the others), so it
//!   never passes through them. Rats run at twice their speed, as the SRD's Dash action gives
//!   ([SRD 5.1 p. 93](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=93));
//!   snakes come on at their speed and keep near their spot.
//! - **Attacking.** In reach, it turns to the player and attacks on a cooldown: a rat lunges
//!   and bites; a snake draws its coils in and rears, then strikes with its jaw open. At the
//!   moment of the bite it sends a [`CreatureAttack`] with the stat block's bite rolled, which
//!   combat (#160) will apply; nothing here hurts anyone.
//! - **Losing the player.** Out of sight for [`LOSE_AFTER`] seconds once it has reached where
//!   it last saw them, or led too far from home, it goes back to wandering its room.
//!
//! Every range, rate and timing here is our own tuning unless an SRD page is named. The SRD's
//! round is 6 seconds (p. 90); a bite every one and a half to two and a half seconds keeps a
//! real-time fight moving.

use bevy::prelude::*;
use fb_creatures::{Creature, Species, UNITS_PER_FOOT};
use fb_level::path::{find_path, line_of_sight, steer_toward};
use fb_level::{CELL, Level};
use procgen_rng::Mulberry32;

use super::{Critter, Driven, Intent, tongue_flick};
use crate::combat::feedback::Corpse;
use crate::level::geometry::Relief;
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Obstacle, Obstacles, Player};
use crate::player::{blocked_by, slide_around};
use crate::tuning::Tuning;

pub struct BehaviourPlugin;

impl Plugin for BehaviourPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CreatureAttack>()
            .init_resource::<BlockedCells>()
            .init_resource::<Obstacles>()
            .init_resource::<Tuning>()
            .add_systems(Update, (wake, behave).chain().before(super::idle).in_set(DelveSystems));
    }
}

/// A creature's bite landing, for combat to apply.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct CreatureAttack {
    pub attacker: Entity,
    pub species: Species,
    /// The bite's damage, rolled from the stat block ([`fb_creatures::StatBlock::bite`]).
    pub damage: u32,
    /// Poison rolled from the stat block's venom (the poisonous snake's 2d4), before the DC 10
    /// Constitution save that halves it, which combat resolves; 0 for a creature with none.
    pub poison: u32,
    /// The attacker's position on the floor (world `(x, z)`), to place the marker relative to the
    /// delver's facing.
    pub attacker_pos: Vec2,
}

/// What a creature is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// Idling for `wait` seconds, then walking to `target` in its home room.
    Wander { target: Option<Vec2>, wait: f32 },
    /// After the player: where it last saw them (world `(x, z)`), and how long ago.
    Chase { last_seen: Vec2, lost: f32 },
}

/// A creature's mind.
#[derive(Component, Clone, Debug)]
pub struct Brain {
    pub mode: Mode,
    /// Where it started (world `(x, z)`) and the room that is its home.
    pub home: Vec2,
    pub room: Option<usize>,
    /// Seconds until it may attack again.
    pub cooldown: f32,
    /// Seconds into an attack, while one is under way.
    pub attack: Option<f32>,
    /// Whether this attack's bite has landed.
    bitten: bool,
    /// The way it is following, in cells, and the cell it leads to.
    path: Vec<(i32, i32)>,
    goal: (i32, i32),
    repath: f32,
    /// How long it has made no headway toward a wander target.
    stuck: f32,
    rng: Mulberry32,
}

/// Seconds out of sight, at the place it last saw the player, before a creature gives up.
pub const LOSE_AFTER: f32 = 3.0;
/// How far from its spot a snake follows, in world units (3 cells).
const SNAKE_LEASH: f32 = 3.0 * CELL;
/// How often a creature finds its way again while following, seconds.
const REPATH: f32 = 0.5;
/// Turning rates, radians per second.
const RAT_TURN: f32 = 10.0;
const SNAKE_TURN: f32 = 3.0;
/// A rat's bite: the lunge takes this long, landing partway through. The wait between bites is
/// tuned (`rat_cooldown`, [`crate::tuning::CreatureTuning`]).
const BITE_TIME: f32 = 0.35;
const BITE_LANDS: f32 = 0.12;
/// A snake's strike: drawn in and reared, thrown, held and drawn back. The wait between is
/// tuned (`snake_cooldown`).
const STRIKE_WINDUP: f32 = 0.7;
const STRIKE_THROW: f32 = 0.12;
const STRIKE_TIME: f32 = 1.4;

/// How far it notices the player, world units: the stat block's darkvision or blindsight, but
/// at least 20 ft (a snake's blindsight of 10 ft would only notice someone standing on it, and
/// the player carries a light), times `scale` (the tuning's `sense_scale`).
pub fn notice_range(species: Species, scale: f32) -> f32 {
    species.stat_block().senses_feet.max(20.0) * UNITS_PER_FOOT * scale
}

/// How near its centre must come to the centre of a player of `player_radius` to bite, world
/// units: the player's body, its own reach (a rat's snout, a snake's coil and a third of its
/// neck, which it strikes across), at the size it is drawn.
pub fn reach(critter: &Critter, player_radius: f32) -> f32 {
    match &critter.creature {
        Creature::Rat(rat) => player_radius + 0.55 * rat.length * critter.size,
        Creature::Snake(snake) => player_radius + critter.radius + 0.35 * snake.dims().neck * critter.size,
    }
}

/// The circle a creature keeps clear of others and walls, world units. A rat's is smaller than
/// all of it, since its tail may brush past things. A snake's is its coil, so it coils clear
/// of the walls, but no wider than [`SNAKE_SQUEEZE`] so it still fits a doorway; laid out, its
/// head leads from the root and the body follows the way the head went (`super::Trail`).
pub fn body_radius(critter: &Critter) -> f32 {
    match &critter.creature {
        Creature::Rat(rat) => 0.3 * rat.length * critter.size,
        Creature::Snake(_) => critter.radius.min(SNAKE_SQUEEZE),
    }
}

/// The widest a snake's [`body_radius`] gets, world units: under half a cell, so it passes a
/// one-cell doorway or corridor. Our own.
const SNAKE_SQUEEZE: f32 = 0.4 * CELL;

/// Grid coordinates of a world `(x, z)`.
fn grid(p: Vec2) -> (f32, f32) {
    (p.x / CELL, p.y / CELL)
}

fn cell(p: Vec2) -> (i32, i32) {
    let (x, y) = grid(p);
    (x.floor() as i32, y.floor() as i32)
}

/// Gives each new creature a brain: it now acts for itself, rather than idling.
fn wake(mut commands: Commands, level: Res<CurrentLevel>, new: Query<(Entity, &Critter, &Transform), (Added<Critter>, Without<Corpse>)>) {
    for (entity, critter, at) in &new {
        // Home is where the level placed it, even when it comes back somewhere else after the
        // delver left the level ([`crate::record`]): it wanders back to its own room.
        let [hx, _, hz] = fb_level::Level::world(critter.spot.x, critter.spot.y);
        let home = Vec2::new(hx, hz);
        let mut rng = Mulberry32::new(critter.spot.seed ^ 0xB4A1);
        let wait = rng.range_f32(0.5, 4.0);
        let (x, y) = cell(home);
        let brain = Brain {
            mode: Mode::Wander { target: None, wait },
            home,
            room: level.0.room_at(x, y),
            cooldown: 0.0,
            attack: None,
            bitten: false,
            path: Vec::new(),
            goal: cell(at.translation.xz()),
            repath: 0.0,
            stuck: 0.0,
            rng,
        };
        commands.entity(entity).insert((brain, Driven));
    }
}

/// A point in `room` at least `margin` world units from its walls, at random.
fn point_in_room(level: &Level, room: Option<usize>, near: Vec2, margin: f32, rng: &mut Mulberry32) -> Vec2 {
    let Some(room) = room.and_then(|i| level.rooms.get(i)) else { return near };
    let m = (margin / CELL).min(room.w as f32 * 0.45).min(room.h as f32 * 0.45);
    let x = rng.range_f32(room.x as f32 + m, (room.x + room.w) as f32 - m);
    let y = rng.range_f32(room.y as f32 + m, (room.y + room.h) as f32 - m);
    Vec2::new(x, y) * CELL
}

/// One frame of every creature's mind: notice, choose where to go, step there round what is in
/// the way, and attack in reach.
#[allow(clippy::too_many_arguments)]
fn behave(
    time: Res<Time>,
    level: Res<CurrentLevel>,
    tuning: Res<Tuning>,
    rules: Option<Res<crate::rules::GameRules>>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Res<Obstacles>,
    player: Query<&Transform, (With<Player>, Without<Critter>)>,
    mut creatures: Query<(Entity, &Critter, &mut Brain, &mut Intent, &mut Transform)>,
    mut attacks: MessageWriter<CreatureAttack>,
    mut commands: Commands,
    sounds: Option<Res<crate::combat::feedback::CreatureSounds>>,
    settings: Option<Res<crate::settings::GameSettings>>,
) {
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }
    let level = &level.0;
    let relief = Relief::new(level);
    let blocked = blocked_by(level, &blocked_cells);
    let player = player.iter().next().map(|t| t.translation.xz());
    // Every creature's circle, to keep the others out of.
    let bodies: Vec<(Entity, Vec2, f32)> = creatures.iter().map(|(e, c, _, _, t)| (e, t.translation.xz(), body_radius(c))).collect();
    let t = time.elapsed_secs();
    let (tune, player_radius) = (&tuning.creatures, tuning.moving.body_radius);
    for (entity, critter, mut brain, mut intent, mut transform) in &mut creatures {
        let brain = &mut *brain;
        let species = critter.spot.species;
        let snake = species.is_snake();
        let at = transform.translation.xz();
        let radius = body_radius(critter);
        let sees = player.filter(|p| p.distance(at) <= notice_range(species, tune.sense_scale) && line_of_sight(grid(at), grid(*p), &blocked));
        brain.cooldown = (brain.cooldown - dt).max(0.0);

        // Notice, follow, give up.
        match (&mut brain.mode, sees) {
            (Mode::Chase { last_seen, lost }, Some(p)) => {
                *last_seen = p;
                *lost = 0.0;
            }
            (Mode::Chase { last_seen, lost }, None) => {
                *lost += dt;
                let there = last_seen.distance(at) < reach(critter, player_radius) + CELL * 0.25;
                if (*lost > LOSE_AFTER && there) || *lost > 3.0 * LOSE_AFTER {
                    brain.mode = Mode::Wander { target: None, wait: brain.rng.range_f32(0.5, 2.0) };
                }
            }
            (Mode::Wander { .. }, Some(p)) => brain.mode = Mode::Chase { last_seen: p, lost: 0.0 },
            (Mode::Wander { .. }, None) => {}
        }
        if snake && let Mode::Chase { last_seen, .. } = brain.mode
            && last_seen.distance(brain.home) > SNAKE_LEASH + reach(critter, player_radius)
        {
            brain.mode = Mode::Wander { target: Some(brain.home), wait: 0.0 };
        }

        // Where to go, how fast.
        let mut heading: Option<Vec2> = None;
        let mut speed = 0.0;
        let mut face: Option<Vec2> = None;
        match &mut brain.mode {
            Mode::Wander { target, wait } => {
                if target.is_none() {
                    *wait -= dt;
                    if *wait <= 0.0 {
                        let goal = if snake {
                            brain.home + Vec2::from_angle(brain.rng.range_f32(0.0, std::f32::consts::TAU)) * brain.rng.range_f32(0.0, 0.8 * CELL)
                        } else {
                            point_in_room(level, brain.room, at, radius + 2.0, &mut brain.rng)
                        };
                        *target = Some(goal);
                        brain.stuck = 0.0;
                    }
                }
                if let Some(goal) = *target {
                    if goal.distance(at) < radius + 0.5 || brain.stuck > 1.5 {
                        *target = None;
                        *wait = if snake { brain.rng.range_f32(5.0, 12.0) } else { brain.rng.range_f32(1.0, 4.0) };
                    } else {
                        heading = Some(goal);
                        speed = species.speed() * if snake { 0.35 } else { 0.45 };
                    }
                }
            }
            Mode::Chase { last_seen, .. } => {
                let goal = sees.unwrap_or(*last_seen);
                face = Some(goal);
                let in_reach = player.is_some_and(|p| p.distance(at) <= reach(critter, player_radius)) && sees.is_some();
                if !in_reach && brain.attack.is_none() {
                    heading = Some(goal);
                    speed = species.speed() * if snake { tune.snake_chase_speed } else { tune.rat_chase_speed };
                }
                if in_reach && brain.attack.is_none() && brain.cooldown <= 0.0 {
                    brain.attack = Some(0.0);
                    brain.bitten = false;
                    // The cue sounds from the creature as the lunge or rear begins, before the bite.
                    if let Some(sounds) = &sounds {
                        let cue = if snake { sounds.snake_rise.clone() } else { sounds.rat_squeal.clone() };
                        let game = settings.as_deref().copied().unwrap_or_default();
                        commands.spawn((
                            Name::new("creature attack cue"),
                            AudioPlayer::new(cue),
                            crate::audio::spatial_sound_effect(&game, &tuning),
                            Transform::from_translation(transform.translation),
                            DespawnOnExit(crate::launch::AppState::Delving),
                        ));
                    }
                }
            }
        }

        // The way there: straight if it can be walked, else along a path round the rock.
        if let Some(goal) = heading {
            let (from_cell, goal_cell) = (cell(at), cell(goal));
            let clearance = radius / CELL;
            let target = if line_of_sight(grid(at), grid(goal), &blocked) {
                brain.path.clear();
                goal
            } else {
                brain.repath -= dt;
                if brain.path.is_empty() || brain.goal != goal_cell || brain.repath <= 0.0 {
                    brain.path = find_path(from_cell, goal_cell, 4000, &blocked).unwrap_or_default();
                    brain.goal = goal_cell;
                    brain.repath = REPATH;
                }
                let (x, y) = steer_toward(grid(at), grid(goal), &brain.path, clearance, &blocked);
                Vec2::new(x, y) * CELL
            };
            let direction = (target - at).normalize_or_zero();
            if direction != Vec2::ZERO {
                let others: Vec<Obstacle> = bodies
                    .iter()
                    .filter(|(e, _, _)| *e != entity)
                    .map(|&(_, centre, r)| Obstacle::Circle { centre, radius: r })
                    .chain(player.map(|centre| Obstacle::Circle { centre, radius: player_radius }))
                    .chain(obstacles.0.iter().copied())
                    .collect();
                let step = (direction * speed * dt).clamp_length_max((target - at).length());
                let to = slide_around(at, at + step, radius, &blocked, &others);
                let made = (to - at).dot(direction);
                brain.stuck = if made < 0.25 * speed * dt { brain.stuck + dt } else { 0.0 };
                transform.translation.x = to.x;
                transform.translation.z = to.y;
                transform.translation.y = relief.floor_height(to.x, to.y);
                face = face.or(Some(target));
            }
        }

        // Turn toward where it is going or what it is after. Creatures face +Z.
        if let Some(look) = face {
            let to = look - transform.translation.xz();
            if to.length_squared() > 1e-4 {
                let wanted = to.x.atan2(to.y);
                let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
                let turn = (wanted - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                let rate = if snake { SNAKE_TURN } else { RAT_TURN };
                transform.rotation = Quat::from_rotation_y(yaw + turn.clamp(-rate * dt, rate * dt));
            }
        }

        // The attack under way, and what it looks like.
        *intent = Intent::default();
        let chasing = matches!(brain.mode, Mode::Chase { .. });
        if snake {
            let moving = speed > 0.0;
            intent.coil = if moving { 0.0 } else { 1.0 };
            intent.rear = if chasing { 1.0 } else { 0.3 + 0.35 * (0.5 + 0.5 * (t * 0.35 + critter.spot.seed as f32).sin()) };
            intent.tongue = tongue_flick(t + (critter.spot.seed % 97) as f32 * 0.1);
        }
        if let Some(since) = brain.attack.as_mut() {
            *since += dt;
            let since = *since;
            let (lands, done) = if snake { (STRIKE_WINDUP + STRIKE_THROW, STRIKE_TIME) } else { (BITE_LANDS, BITE_TIME) };
            if snake {
                intent.coil = 1.0;
                intent.rear = 1.0;
                let thrown = (STRIKE_WINDUP..STRIKE_WINDUP + 0.35).contains(&since);
                intent.strike = if thrown { 1.0 } else { 0.0 };
                intent.jaw = intent.strike;
                intent.tongue = 0.0;
            } else {
                intent.strike = if since < BITE_TIME * 0.6 { 1.0 } else { 0.0 };
            }
            if !brain.bitten && since >= lands {
                brain.bitten = true;
                let block = crate::rules::GameRules::of(&rules).creature(species);
                let damage = block.bite.roll(&mut brain.rng);
                let poison = block.venom.map_or(0, |v| v.roll(&mut brain.rng));
                attacks.write(CreatureAttack { attacker: entity, species, damage, poison, attacker_pos: at });
            }
            if since >= done {
                brain.attack = None;
                brain.cooldown = if snake { tune.snake_cooldown } else { tune.rat_cooldown };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The player's body radius as shipped.
    const BODY_RADIUS: f32 = 3.5;

    #[test]
    fn the_body_radius_here_is_the_shipped_one() {
        assert_eq!(Tuning::default().moving.body_radius, BODY_RADIUS);
    }
    use crate::creatures::{animate, spawn_skeleton};
    use bevy::time::TimeUpdateStrategy;
    use fb_level::FloorShape;

    /// Seed 1207's creatures as bones (no drawing), thinking and moving, with folk and props in
    /// the way and a player, stepped 1/60 s a frame.
    fn level_app(player_at: Vec2) -> (App, Vec<Entity>, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .add_plugins(BehaviourPlugin)
            .add_systems(Update, (super::super::idle, animate).chain().after(behave));
        let level = fb_level::generate(1207, 1);
        app.insert_resource(Obstacles(obstacles(&level)));
        app.insert_resource(CurrentLevel(level.clone()));
        let player = app.world_mut().spawn((Player, Transform::from_xyz(player_at.x, 0.0, player_at.y))).id();
        let roots = {
            let mut commands = app.world_mut().commands();
            level.creatures.iter().map(|spot| spawn_skeleton(&mut commands, &level, spot, &spot.creature().rig()).0).collect()
        };
        app.world_mut().flush();
        (app, roots, player)
    }

    /// The folk and props of `level` as obstacles, as the game gathers them (folk a little
    /// narrower than a dressed figure's footprint).
    fn obstacles(level: &Level) -> Vec<Obstacle> {
        let at = |x: f32, y: f32, yaw: f32| GlobalTransform::from(Transform::from_translation(Vec3::from_array(Level::world(x, y))).with_rotation(Quat::from_rotation_y(yaw)));
        let folk = level.folk.iter().map(|f| Obstacle::of(FloorShape::Round(3.0), &at(f.x, f.y, f.yaw)));
        let props = level.props.iter().map(|p| Obstacle::of(p.prop.floor_shape(), &at(p.x, p.y, p.yaw)));
        folk.chain(props).collect()
    }

    fn rat_pack(app: &App, roots: &[Entity]) -> Vec<Entity> {
        roots.iter().copied().filter(|&r| app.world().get::<Critter>(r).unwrap().spot.species.is_rat()).collect()
    }

    fn position(app: &App, entity: Entity) -> Vec2 {
        app.world().get::<Transform>(entity).unwrap().translation.xz()
    }

    fn attacks(app: &mut App) -> Vec<CreatureAttack> {
        let messages = app.world().resource::<Messages<CreatureAttack>>();
        messages.iter_current_update_messages().copied().collect()
    }

    #[test]
    fn a_rat_that_sees_you_reaches_you_and_attacks() {
        let level = fb_level::generate(1207, 1);
        // The first pack's room, the player standing two cells from the pack in plain sight.
        let first = level.creatures.iter().find(|c| c.species.is_rat()).unwrap();
        let room = level.rooms[level.room_at(first.x as i32, first.y as i32).unwrap()];
        let (cx, cy) = room.centre();
        let player_at = Vec2::new(cx, cy) * CELL + Vec2::new(-1.0, 1.3) * CELL;
        let (mut app, roots, _) = level_app(player_at);
        let pack = rat_pack(&app, &roots);
        let mut bitten_by = Vec::new();
        for _ in 0..600 {
            app.update();
            bitten_by.extend(attacks(&mut app).into_iter().map(|a| (a.attacker, a.damage)));
        }
        let attacker = bitten_by.iter().find(|(e, _)| pack.contains(e)).map(|&(e, _)| e).expect("no rat bit");
        let critter = app.world().get::<Critter>(attacker).unwrap().clone();
        assert!(position(&app, attacker).distance(player_at) <= reach(&critter, BODY_RADIUS) + 1.0, "the biter is in reach");
        assert!(bitten_by.iter().all(|&(_, d)| d >= 1), "a rat's bite does 1 (SRD)");
        // Bites come on a cooldown, not every frame.
        let from_one = bitten_by.iter().filter(|(e, _)| *e == attacker).count();
        assert!((1..=7).contains(&from_one), "{from_one} bites in 10 s");
    }

    #[test]
    fn creatures_never_pass_through_walls_props_or_folk() {
        let level = fb_level::generate(1207, 1);
        let walls = |x: i32, y: i32| !level.is_open(x, y);
        let obstacles = obstacles(&level);
        // The player stands where the rats must come round the dwarves and the brazier, then
        // in the corridor outside, then far away.
        let spots = [Vec2::new(26.5, 9.0), Vec2::new(18.0, 11.5), Vec2::new(1.5, 22.5)];
        let (mut app, roots, player) = level_app(spots[0] * CELL);
        for frame in 0..900 {
            if frame % 300 == 0 {
                let at = spots[frame / 300] * CELL;
                app.world_mut().get_mut::<Transform>(player).unwrap().translation = Vec3::new(at.x, 0.0, at.y);
            }
            app.update();
            for &root in &roots {
                let critter = app.world().get::<Critter>(root).unwrap().clone();
                let at = position(&app, root);
                let r = body_radius(&critter);
                let (gx, gy) = grid(at);
                assert!(!walls(gx.floor() as i32, gy.floor() as i32), "{:?} in rock at {at} (frame {frame})", critter.spot.species);
                // Clear of the rock round its cell by its radius, less a hair.
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (x, y) = (gx.floor() as i32 + dx, gy.floor() as i32 + dy);
                        if walls(x, y) {
                            let (min, max) = (Vec2::new(x as f32, y as f32) * CELL, Vec2::new(x as f32 + 1.0, y as f32 + 1.0) * CELL);
                            assert!(at.distance(at.clamp(min, max)) >= r - 0.01, "{:?} in the wall at {at} (frame {frame})", critter.spot.species);
                        }
                    }
                }
                for o in &obstacles {
                    assert!(!o.overlaps(at, r), "{:?} at {at} walks into {o:?} (frame {frame})", critter.spot.species);
                }
            }
        }
    }

    #[test]
    fn out_of_sight_they_go_back_to_wandering() {
        let level = fb_level::generate(1207, 1);
        let first = level.creatures.iter().find(|c| c.species.is_rat()).unwrap();
        let near = Vec2::new(first.x + 1.0, first.y + 0.5) * CELL;
        let (mut app, roots, player) = level_app(near);
        let pack = rat_pack(&app, &roots);
        for _ in 0..60 {
            app.update();
        }
        let chasing = |app: &App| pack.iter().filter(|&&r| matches!(app.world().get::<Brain>(r).unwrap().mode, Mode::Chase { .. })).count();
        assert!(chasing(&app) > 0, "the pack noticed the player");
        // The player slips away to the start, out of sight behind rock.
        let (sx, sy, _) = level.start();
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = Vec3::new((sx as f32 + 0.5) * CELL, 0.0, (sy as f32 + 0.5) * CELL);
        for _ in 0..(60 * 15) {
            app.update();
        }
        assert_eq!(chasing(&app), 0, "they gave up");
        // And they are back in their room.
        for &r in &pack {
            let (gx, gy) = grid(position(&app, r));
            let room = app.world().get::<Brain>(r).unwrap().room;
            assert_eq!(level.room_at(gx as i32, gy as i32), room, "a rat wandered off");
        }
    }

    #[test]
    fn notice_ranges_follow_the_senses() {
        assert!((notice_range(Species::Rat, 1.0) - 30.0 * UNITS_PER_FOOT).abs() < 1e-3);
        assert!((notice_range(Species::GiantRat, 1.0) - 60.0 * UNITS_PER_FOOT).abs() < 1e-3);
        assert!((notice_range(Species::PoisonousSnake, 1.0) - 20.0 * UNITS_PER_FOOT).abs() < 1e-3);
    }

    #[test]
    fn long_snakes_keep_their_bodies_out_of_the_rock() {
        let level = fb_level::generate(1207, 1);
        // How deep a point lies in rock: 0 on open floor, else how far it is from the nearest
        // open cell beside the rock cell it is in.
        let depth = |x: f32, z: f32| {
            let (cx, cz) = ((x / CELL).floor() as i32, (z / CELL).floor() as i32);
            if level.is_open(cx, cz) {
                return 0.0;
            }
            let mut nearest = f32::MAX;
            for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                if level.is_open(cx + dx, cz + dz) {
                    let (min, max) = (Vec2::new((cx + dx) as f32, (cz + dz) as f32) * CELL, Vec2::new((cx + dx + 1) as f32, (cz + dz + 1) as f32) * CELL);
                    nearest = nearest.min(Vec2::new(x, z).distance(Vec2::new(x, z).clamp(min, max)));
                }
            }
            nearest
        };
        // The player walks round the constrictor's room and out of its door, then round the
        // viper's, so each snake rears, turns and follows.
        let spots = [Vec2::new(11.0, 10.0), Vec2::new(13.5, 14.5), Vec2::new(10.5, 14.0), Vec2::new(16.0, 11.5), Vec2::new(23.0, 29.0), Vec2::new(27.5, 32.5), Vec2::new(23.0, 32.0)];
        let (mut app, roots, player) = level_app(spots[0] * CELL);
        let snakes: Vec<Entity> = roots.iter().copied().filter(|&r| app.world().get::<Critter>(r).unwrap().spot.species.is_snake()).collect();
        assert!(snakes.len() >= 2);
        let mut moved = 0.0;
        let start: Vec<Vec2> = snakes.iter().map(|&s| position(&app, s)).collect();
        for frame in 0..(spots.len() * 240) {
            if frame % 240 == 0 {
                let at = spots[frame / 240] * CELL;
                app.world_mut().get_mut::<Transform>(player).unwrap().translation = Vec3::new(at.x, 0.0, at.y);
            }
            app.update();
            if frame % 10 != 9 {
                continue;
            }
            for &snake in &snakes {
                let Creature::Snake(state) = app.world().get::<Critter>(snake).unwrap().creature.clone() else { unreachable!() };
                let bones = app.world().get::<crate::creatures::Bones>(snake).unwrap().0.clone();
                for (k, &bone) in bones.iter().enumerate().skip(fb_creatures::snake::bone::SPINE) {
                    let at = app.world().get::<GlobalTransform>(bone).unwrap().translation();
                    // The body may brush the wall; its centre line never enters the rock.
                    let deep = depth(at.x, at.z);
                    
                    assert!(deep == 0.0, "{:?}: segment {k} {deep} into the rock at {at} (frame {frame})", state.species);
                }
            }
        }
        for (i, &snake) in snakes.iter().enumerate() {
            moved += position(&app, snake).distance(start[i]);
        }
        assert!(moved > CELL, "the snakes stayed put");
    }

    #[test]
    fn every_creature_fits_a_corridor_and_can_reach_past_its_own_body() {
        for species in Species::ALL {
            for seed in 0..32 {
                let spot = fb_level::CreatureSpot { x: 0.0, y: 0.0, yaw: 0.0, species, seed, pack: 0 };
                let creature = spot.creature();
                let critter = Critter::new(spot, creature);
                // Through a one-cell corridor or doorway, with room to spare.
                assert!(2.0 * body_radius(&critter) < 0.9 * CELL, "{species:?} {seed}: {}", body_radius(&critter));
                // Its bite reaches past the circles that keep it off the player.
                assert!(reach(&critter, BODY_RADIUS) > body_radius(&critter) + BODY_RADIUS + 1.0, "{species:?} {seed}");
            }
        }
    }

    /// The attack cues playing: where each stands, how loud it plays and whether it is spatial.
    fn cues(app: &mut App) -> Vec<(Vec3, f32, bool)> {
        let mut query = app.world_mut().query_filtered::<(&Transform, &PlaybackSettings), With<crate::audio::SoundEffect>>();
        query.iter(app.world()).map(|(t, p)| (t.translation, p.volume.to_linear(), p.spatial)).collect()
    }

    /// The first cue, where every creature stood the frame it began, and the frames the cue and
    /// the first bite came.
    fn first_cue(app: &mut App, roots: &[Entity]) -> ((Vec3, f32, bool), Vec<Vec3>, usize, usize) {
        let mut cue = None;
        for frame in 0..900 {
            app.update();
            let found = cues(app);
            if cue.is_none() && !found.is_empty() {
                let at = roots.iter().map(|&r| app.world().get::<Transform>(r).unwrap().translation).collect();
                cue = Some((found[0], at, frame));
            }
            if !attacks(app).is_empty() {
                let (found, at, started) = cue.expect("a bite came with no cue before it");
                return (found, at, started, frame);
            }
        }
        panic!("no bite");
    }

    fn rat_room_player() -> Vec2 {
        let level = fb_level::generate(1207, 1);
        let first = level.creatures.iter().find(|c| c.species.is_rat()).unwrap();
        let room = level.rooms[level.room_at(first.x as i32, first.y as i32).unwrap()];
        let (cx, cy) = room.centre();
        Vec2::new(cx, cy) * CELL + Vec2::new(-1.0, 1.3) * CELL
    }

    #[test]
    fn the_attack_cue_sounds_from_the_creature_before_its_bite() {
        let (mut app, roots, _) = level_app(rat_room_player());
        app.insert_resource(crate::combat::feedback::CreatureSounds::default());
        let ((at, _, spatial), creatures, started, bit) = first_cue(&mut app, &roots);
        assert!(spatial);
        assert!(started < bit, "the cue at frame {started}, the bite at {bit}");
        assert!(creatures.iter().any(|c| c.distance(at) < 0.01), "the cue stands at a creature: {at}");
    }

    #[test]
    fn the_attack_cue_follows_the_effects_volume_and_a_tenth_scale() {
        let (mut app, roots, _) = level_app(rat_room_player());
        app.insert_resource(crate::combat::feedback::CreatureSounds::default()).insert_resource(crate::settings::GameSettings { sound_volume: 0.5, ..default() });
        let wanted = crate::audio::effect_volume(app.world().resource(), app.world().resource());
        let ((_, volume, _), ..) = first_cue(&mut app, &roots);
        assert!(wanted > 0.0);
        assert_eq!(volume, wanted);
        let mut query = app.world_mut().query_filtered::<&PlaybackSettings, With<crate::audio::SoundEffect>>();
        let scale = query.iter(app.world()).next().unwrap().spatial_scale.unwrap();
        assert_eq!(scale.0, Vec3::splat(0.1));
    }
}
