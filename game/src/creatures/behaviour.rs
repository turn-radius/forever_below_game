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
//!   snakes come on at their speed and keep near their spot; a spider rushes at twice its speed
//!   too. A giant centipede scuttles in
//!   quick bursts with pauses between while it wanders ([`scuttling`]), and comes on at twice
//!   its speed, without pause, once it has noticed the delver.
//! - **Attacking.** In reach, it turns to the player and attacks on a cooldown: a rat lunges
//!   and bites; a snake draws its coils in and rears, then strikes with its jaw open; a spider
//!   rears up on its back legs with the front pair raised, then lunges with its fangs. At the
//!   moment of the bite it rolls to hit, the stat block's attack bonus against the delver's
//!   Armor Class ([`BlowsAtDelver`]): a hit sends an [`AttackOnDelver`] with the stat block's
//!   bite rolled, which combat applies, and a miss a [`MissedDelver`].
//!   A giant spider lies in wait: it never wanders, and notices the delver only within
//!   [`SPIDER_AMBUSH_FEET`], then rushes at twice its speed.
//! - **Losing the player.** Out of sight for [`LOSE_AFTER`] seconds once it has reached where
//!   it last saw them, or led too far from home, it goes back to wandering its room.
//!
//! A troll attacks as the SRD's Multiattack: in one swing of its arms it bites and claws twice,
//! each rolled to hit on its own ([`TROLL_COOLDOWN`] between swings); an armed troll swings its
//! weapon in place of the right claw; its regeneration is in [`super::trolls`].
//!
//! Every range, rate and timing here is our own tuning unless an SRD page is named. The SRD's
//! round is 6 seconds (p. 90); a bite every one and a half to two and a half seconds keeps a
//! real-time fight moving.

use bevy::prelude::*;
use fb_creatures::{Creature, Species, UNITS_PER_FOOT};
use fb_level::path::{find_path, line_of_sight, steer_toward};
use fb_level::{CELL, Level};
use procgen_rng::Mulberry32;

use super::{Critter, Driven, Flight, Intent, Wing, tongue_flick};
use crate::combat::ArmourClass;
use crate::combat::feedback::Corpse;
use fb_character::combat::{Odds, ToHit};
use crate::level::geometry::Relief;
use crate::level::geometry::stone::Vaults;
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, EYE_HEIGHT, Obstacle, Obstacles, Player};
use crate::player::slide_around;
use crate::tuning::Tuning;

pub struct BehaviourPlugin;

impl Plugin for BehaviourPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AttackOnDelver>()
            .add_message::<MissedDelver>()
            .init_resource::<BlockedCells>()
            .init_resource::<Obstacles>()
            .init_resource::<Tuning>()
            .add_systems(Update, (wake, tick_influences, behave).chain().before(super::idle).in_set(DelveSystems));
    }
}

/// A blow landing on the delver, for combat to apply: a creature's bite or blow, or a hostile
/// folk's (`crate::folk_combat`).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct AttackOnDelver {
    pub attacker: Entity,
    /// The attacking creature's species, `None` for a folk.
    pub species: Option<Species>,
    /// The bite's damage, rolled from the stat block ([`fb_creatures::StatBlock::bite`]).
    pub damage: u32,
    /// Poison rolled from the stat block's venom (the poisonous snake's 2d4), before the DC 10
    /// Constitution save that halves it, which combat resolves; 0 for a creature with none.
    pub poison: u32,
    /// The attacker's position on the floor (world `(x, z)`), to place the marker relative to the
    /// delver's facing.
    pub attacker_pos: Vec2,
}

/// A blow aimed at the delver that missed: its attack roll fell short of their Armor Class, or
/// it hit and their raised shield turned it aside. Combat whooshes a miss past them
/// (`crate::combat::vitals`) and clangs a blocked one on the shield, which costs them stamina
/// (`crate::combat::guard`); nothing else happens.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct MissedDelver {
    pub attacker: Entity,
    /// The attacker's position on the floor (world `(x, z)`).
    pub attacker_pos: Vec2,
    /// Whether it hit and the shield turned it aside.
    pub blocked: bool,
}

/// Where every blow at the delver, a creature's or a folk's, is rolled: the attacker's to-hit
/// against the delver's Armor Class, the same rule as the delver's swings
/// ([`fb_character::combat::ToHit`]). A hit is sent as an [`AttackOnDelver`], a miss as a
/// [`MissedDelver`]. Once the delver is down nothing is rolled or sent, and nobody starts a
/// blow at them ([`BlowsAtDelver::delver_is_down`]).
#[derive(bevy::ecs::system::SystemParam)]
pub struct BlowsAtDelver<'w, 's> {
    hits: MessageWriter<'w, AttackOnDelver>,
    misses: MessageWriter<'w, MissedDelver>,
    fallen: Option<Res<'w, crate::combat::vitals::Fallen>>,
    #[allow(clippy::type_complexity)]
    delver: Query<'w, 's, (&'static crate::combat::Health, Option<&'static crate::combat::vitals::BlockChance>, Option<&'static crate::combat::guard::Guard>), With<Player>>,
    rules: Option<Res<'w, crate::rules::GameRules>>,
}

impl BlowsAtDelver<'_, '_> {
    /// Whether the delver has fallen (at 0 hit points, or the run ending): there is nothing left
    /// to fight.
    pub fn delver_is_down(&self) -> bool {
        self.fallen.is_some() || self.delver.iter().any(|(health, ..)| health.is_down())
    }

    /// Rolls `blow`, by `who` with `to_hit`, against `armour_class` (the delver's, or
    /// [`ArmourClass::UNARMOURED`] when they have none), from `rng`; returns whether it landed.
    /// Against a fallen delver nothing is rolled and it does not land.
    pub fn strike(&mut self, who: &str, blow: AttackOnDelver, to_hit: ToHit, armour_class: Option<&ArmourClass>, rng: &mut Mulberry32) -> bool {
        if self.delver_is_down() {
            return false;
        }
        let ac = armour_class.copied().unwrap_or(ArmourClass::UNARMOURED).0;
        let attack = to_hit.roll_with(crate::rules::GameRules::of(&self.rules), ac, Odds::Even, rng);
        // A hit the delver's raised shield turns aside is blocked: a miss in all but name. It
        // covers only blows from ahead; nothing is drawn with the guard down or the blow
        // outside it, so the dice run as they always did.
        let covered = self.delver.iter().next().and_then(|(_, _, guard)| guard).is_some_and(|guard| guard.covers(blow.attacker_pos));
        let block = self.delver.iter().next().and_then(|(_, b, ..)| b.copied()).filter(|_| covered).unwrap_or_default();
        let blocked = attack.hit && block.turns_aside(rng);
        let verdict = if blocked { ", blocked" } else { "" };
        info!("{who} attacks the delver: {}{verdict}", attack.line());
        let lands = attack.hit && !blocked;
        if lands {
            self.hits.write(blow);
        } else {
            self.misses.write(MissedDelver { attacker: blow.attacker, attacker_pos: blow.attacker_pos, blocked });
        }
        lands
    }
}

/// The delver as a blow at them needs them: where they stand and their Armor Class.
pub type DelverToStrike = (&'static Transform, Option<&'static ArmourClass>);

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
    /// A fleeing jackalope's goal cell, and the seconds until it picks again ([`flee_goal`]).
    flee: Option<(i32, i32)>,
    flee_in: f32,
    /// Whether it found nowhere farther to run to.
    cornered: bool,
    rng: Mulberry32,
    /// This creature's one-off roll, 0 to 1, fixed by its own seed: it never notices a delver
    /// whose shun chance for its kind is above it ([`Shunned::passes_over`]), until it is struck
    /// ([`Aroused`]).
    pub shun_roll: f32,
}

/// Marks a creature the delver has struck: it no longer passes over a delver who shuns its kind
/// and chases like any other.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Aroused;

/// Frightened: the creature runs from the delver for this many more seconds, and does not
/// attack. Put on by the cleric's Frighten prayer ([`crate::casting`]).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Frightened(pub f32);

/// Charmed: the creature takes no notice of the delver and does not attack for this many more
/// seconds, until the delver strikes it ([`Aroused`]). Not the cleric's alone: the Charm spell
/// of the mage uses it too.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Charmed(pub f32);

/// How far a frightened creature still sees the delver to run from, world units, and how fast it
/// runs as a multiple of its walking speed. Our own.
const FRIGHT_SIGHT: f32 = 60.0;
const FRIGHT_BOLT: f32 = 1.5;

/// Who is frightened or charmed, and what the delver shuns, for [`behave`].
#[derive(bevy::ecs::system::SystemParam)]
struct Influences<'w, 's> {
    states: Query<'w, 's, (Has<Frightened>, Has<Charmed>)>,
    shunned: Query<'w, 's, &'static crate::combat::vitals::Shunned, With<Player>>,
}

impl Influences<'_, '_> {
    /// Whether `entity` is frightened, and whether it is charmed.
    fn of(&self, entity: Entity) -> (bool, bool) {
        self.states.get(entity).unwrap_or_default()
    }
}

/// Counts down every frightened and charmed creature, and frees the charmed one that was struck.
fn tick_influences(
    time: Res<Time>,
    mut commands: Commands,
    mut frightened: Query<(Entity, &mut Frightened)>,
    mut charmed: Query<(Entity, &mut Charmed, Has<Aroused>)>,
) {
    let dt = time.delta_secs().min(0.1);
    for (entity, mut f) in &mut frightened {
        f.0 -= dt;
        if f.0 <= 0.0 {
            commands.entity(entity).remove::<Frightened>();
        }
    }
    for (entity, mut c, struck) in &mut charmed {
        c.0 -= dt;
        if c.0 <= 0.0 || struck {
            commands.entity(entity).remove::<Charmed>();
        }
    }
}

/// Mixed into a creature's seed for its [`Brain::shun_roll`]. Our own.
const SHUN_SALT: u32 = 0x5B0A_C3D1;

/// The one-off roll, 0 to 1, of the creature with `seed`: the same every run.
pub fn shun_roll(seed: u32) -> f32 {
    Mulberry32::new(seed ^ SHUN_SALT).next_f64() as f32
}

/// Seconds out of sight, at the place it last saw the player, before a creature gives up.
pub const LOSE_AFTER: f32 = 3.0;
/// Seconds a creature presses on without getting anywhere before it counts as stuck at a grate it
/// sees the delver through, and how long it rests after giving them up.
pub const STUCK_AT_GRATE: f32 = 1.5;
pub const GRATE_REST: f32 = 8.0;

/// Whether a creature that sees the delver only through a grate (no way to them) gives them up:
/// once it has pressed on without progress for [`STUCK_AT_GRATE`], and for as long as it rests after.
pub fn gives_up_at_grate(stuck: f32, resting: bool) -> bool {
    stuck > STUCK_AT_GRATE || resting
}
/// How far from its spot a snake follows, in world units (3 cells).
const SNAKE_LEASH: f32 = 3.0 * CELL;
/// How often a creature finds its way again while following, seconds.
const REPATH: f32 = 0.5;
/// Turning rates, radians per second.
const RAT_TURN: f32 = 10.0;
const SNAKE_TURN: f32 = 3.0;
const SKELETON_TURN: f32 = 5.0;
const SPIDER_TURN: f32 = 6.0;
const TROLL_TURN: f32 = 3.5;
/// The wait between a troll's attacks, seconds, and how long one takes: the rake of both claws
/// ([`fb_creatures::pose::troll_with`]), the blows landing as the arms come down. Our own.
const TROLL_COOLDOWN: f32 = 1.2;
const TROLL_SWING: f32 = 1.6;
const TROLL_LANDS: f32 = 0.62;
/// How much farther than its reach a troll's blows still land, as for a skeleton's.
const TROLL_SLACK: f32 = 1.15;
const CENTIPEDE_TURN: f32 = 7.0;
/// A centipede's chase speed as a multiple of its walking speed (the Dash action, as a rat's),
/// and the wait between its bites, seconds. Our own cooldown, like the rat's.
const CENTIPEDE_CHASE: f32 = 2.0;
const CENTIPEDE_COOLDOWN: f32 = 1.6;
/// How fast a centipede runs in a wandering burst, as a share of its walking speed. Our own.
const CENTIPEDE_BURST: f32 = 0.9;
/// A wandering centipede runs for this share of each period and is still for the rest. Our own.
const SCUTTLE_PERIOD: f32 = 1.7;
const SCUTTLE_SHARE: f32 = 0.4;

/// Whether a wandering centipede with `seed` is in a burst of running at `t` seconds, rather
/// than pausing: each has its own timing, so a room's centipedes do not scuttle in step.
pub fn scuttling(t: f32, seed: u32) -> bool {
    ((t + (seed % 97) as f32 * 0.07) / SCUTTLE_PERIOD).fract() < SCUTTLE_SHARE
}
/// A jackalope bolts at this multiple of its walking speed once it has noticed the delver, and
/// gores only when cornered: pinned against rock or a prop with the delver in reach. Our own.
const JACKALOPE_BOLT: f32 = 1.5;
/// How long it keeps a place to flee to before choosing again, seconds, and how long it must
/// have made no headway before it counts as cornered.
const FLEE_REPLAN: f32 = 0.75;
const CORNERED_AFTER: f32 = 0.25;
/// How far a fleeing jackalope looks for somewhere to run to, cells either way.
const FLEE_RADIUS: i32 = 28;
/// A jackalope's gore: it lands this far into the lunge, which takes this long, and the wait
/// between gores, seconds. Our own.
const GORE_LANDS: f32 = 0.18;
const GORE_TIME: f32 = 0.5;
const JACKALOPE_COOLDOWN: f32 = 1.4;
/// A standing jackalope sits up for this share of each period, seconds. Our own.
const SIT_PERIOD: f32 = 13.0;
const SIT_SHARE: f32 = 0.35;

/// Whether a standing jackalope with `seed` is sat up on its haunches at `t` seconds (its ears
/// turning) rather than on all fours: each has its own timing.
pub fn sitting_up(t: f32, seed: u32) -> bool {
    ((t + (seed % 89) as f32 * 0.37) / SIT_PERIOD).fract() < SIT_SHARE
}

/// Steps from `start` to every cell within [`FLEE_RADIUS`] of it that can be walked (four ways
/// at a time).
fn steps_from(start: (i32, i32), blocked: &impl Fn(i32, i32) -> bool) -> std::collections::HashMap<(i32, i32), u32> {
    let mut steps = std::collections::HashMap::from([(start, 0)]);
    let mut frontier = std::collections::VecDeque::from([start]);
    while let Some(at) = frontier.pop_front() {
        let next_steps = steps[&at] + 1;
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = (at.0 + dx, at.1 + dy);
            if (next.0 - start.0).abs() > FLEE_RADIUS || (next.1 - start.1).abs() > FLEE_RADIUS || blocked(next.0, next.1) || steps.contains_key(&next) {
                continue;
            }
            steps.insert(next, next_steps);
            frontier.push_back(next);
        }
    }
    steps
}

/// Where a creature at cell `from` runs to, away from the delver at cell `threat`: the cell
/// farthest from the delver by the way they would walk, of those it can reach no later than they can
/// (so its way never passes them). With a way on past a door or a prop that is somewhere
/// through it, not the corner of the room it is in. `None` when nowhere it can reach is farther
/// from them than it is: it is cornered.
pub fn flee_goal(from: (i32, i32), threat: (i32, i32), blocked: impl Fn(i32, i32) -> bool) -> Option<(i32, i32)> {
    let (mine, theirs) = (steps_from(from, &blocked), steps_from(threat, &blocked));
    // Cells the delver's search never got to are far indeed.
    let far = 4 * FLEE_RADIUS as u32;
    let by_them = |cell: &(i32, i32)| theirs.get(cell).copied().unwrap_or(far);
    let here = by_them(&from);
    mine.iter()
        .filter(|(cell, steps)| **steps <= by_them(cell))
        .map(|(&cell, &steps)| (by_them(&cell), std::cmp::Reverse(steps), cell))
        .max()
        .filter(|(farthest, ..)| *farthest > here)
        .map(|(.., cell)| cell)
}

/// How far past its own body a bat darts to bite, world units. Our own.
const BAT_DART: f32 = 2.0;
/// How fast a bat and a giant bat turn, radians a second: the giant is slower. Our own.
const BAT_TURN: f32 = 7.0;
const GIANT_BAT_TURN: f32 = 3.0;
/// How far from the delver's eye a bat's body may be and still bite, world units. Our own.
const BITE_BAND: f32 = 7.0;
/// How long a bat looks for a way back to its spot before it hangs wherever it is, seconds.
const RETURN_GIVE_UP: f32 = 15.0;
/// The wait between a skeleton's swings, seconds. Our own.
const SKELETON_COOLDOWN: f32 = 0.8;
/// How much farther than its reach a skeleton's blow still lands, in case the delver steps
/// back as it falls. Our own.
const SKELETON_SLACK: f32 = 1.15;
/// A rat's bite: the lunge takes this long, landing partway through. The wait between bites is
/// tuned (`rat_cooldown`, [`crate::tuning::CreatureTuning`]).
const BITE_TIME: f32 = 0.35;
const BITE_LANDS: f32 = 0.12;
/// A snake's strike: drawn in and reared, thrown, held and drawn back. The wait between is
/// tuned (`snake_cooldown`).
const STRIKE_WINDUP: f32 = 0.7;
const STRIKE_THROW: f32 = 0.12;
const STRIKE_TIME: f32 = 1.4;

/// A giant spider's attack: it rears for [`SPIDER_WINDUP`], stabs for a moment as the bite lands
/// and settles by [`SPIDER_TIME`]. The wait between bites is [`SPIDER_COOLDOWN`]. Our own.
const SPIDER_WINDUP: f32 = 0.6;
const SPIDER_STAB: f32 = 0.12;
const SPIDER_TIME: f32 = 1.2;
const SPIDER_COOLDOWN: f32 = 2.0;
/// A spider rushes at twice its walking speed, as the SRD's Dash action gives (p. 93).
const SPIDER_CHASE: f32 = 2.0;
/// How near the delver must come for a spider waiting still to spring, feet: well inside its
/// darkvision of 60 ft, so it lies in wait until they are close. Our own.
pub const SPIDER_AMBUSH_FEET: f32 = 25.0;

/// How far it notices the player, world units: the stat block's darkvision or blindsight, but
/// at least 20 ft (a snake's blindsight of 10 ft would only notice someone standing on it, and
/// the player carries a light), times `scale` (the tuning's `sense_scale`). A spider in wait
/// springs only within [`SPIDER_AMBUSH_FEET`].
pub fn notice_range(species: Species, scale: f32) -> f32 {
    let feet = if species.is_spider() { SPIDER_AMBUSH_FEET } else { species.stat_block().senses_feet.max(20.0) };
    feet * UNITS_PER_FOOT * scale
}

/// How near its centre must come to the centre of a player of `player_radius` to bite, world
/// units: the player's body, its own reach (a rat's snout, a snake's coil and a third of its
/// neck, which it strikes across), at the size it is drawn.
pub fn reach(critter: &Critter, player_radius: f32) -> f32 {
    match &critter.creature {
        Creature::Rat(rat) => player_radius + 0.55 * rat.length * critter.size,
        // A bat darts in past its own body to bite. The dart is our own.
        Creature::Bat(bat) => player_radius + (0.5 * bat.length + BAT_DART) * critter.size,
        Creature::Snake(snake) => player_radius + critter.radius + 0.35 * snake.dims().neck * critter.size,
        // Arm and what it holds, as worked out once at spawn: a spear outreaches a dagger.
        Creature::Skeleton(skeleton) => player_radius + critter.kit.as_ref().map_or_else(|| 0.45 * skeleton.height(), |k| k.reach) * critter.size,
        // Its fangs, with the front legs raised over the delver.
        Creature::Spider(spider) => player_radius + 0.3 * spider.span * critter.size,
        // The floor it covers and the arms' reach past it.
        Creature::Troll(_) => player_radius + 1.1 * critter.radius,
        // Its head lowered, the antlers a little past its nose.
        Creature::Jackalope(jackalope) => player_radius + 0.55 * jackalope.length * critter.size,
        // Its head leads the body from its middle: it bites when that reaches the delver.
        Creature::Centipede(centipede) => player_radius + 0.5 * centipede.length * critter.size,
    }
}

/// The circle a creature keeps clear of others and walls, world units. A rat's is smaller than
/// all of it, since its tail may brush past things. A snake's is its coil, so it coils clear
/// of the walls, but no wider than [`SNAKE_SQUEEZE`] so it still fits a doorway; laid out, its
/// head leads from the root and the body follows the way the head went (`super::Trail`).
pub fn body_radius(critter: &Critter) -> f32 {
    match &critter.creature {
        Creature::Rat(rat) => 0.3 * rat.length * critter.size,
        // Its body, not its wings, which fold and flex past what they meet.
        Creature::Bat(bat) => 0.5 * bat.length * critter.size,
        Creature::Snake(_) => critter.radius.min(SNAKE_SQUEEZE),
        Creature::Jackalope(jackalope) => 0.3 * jackalope.length * critter.size,
        Creature::Skeleton(skeleton) => 0.12 * critter.kit.as_ref().map_or_else(|| skeleton.height(), |k| k.height) * critter.size,
        // Its body, not its legs, which fold round whatever it passes.
        Creature::Spider(spider) => 0.2 * spider.span * critter.size,
        // Its body, not its reach, and no wider than a doorway lets through.
        Creature::Troll(_) => (0.55 * critter.radius).min(TROLL_SQUEEZE),
        // A third of its length from its middle, no wider than a snake's, so it passes a doorway.
        Creature::Centipede(centipede) => (0.35 * centipede.length * critter.size).min(SNAKE_SQUEEZE),
    }
}

/// The widest a troll's [`body_radius`] gets, world units: under a third of a cell. Our own.
const TROLL_SQUEEZE: f32 = 0.3 * CELL;

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
#[allow(clippy::type_complexity)]
fn wake(mut commands: Commands, level: Res<CurrentLevel>, new: Query<(Entity, &Critter, &Transform), (Added<Critter>, Without<Corpse>)>) {
    for (entity, critter, at) in &new {
        commands.entity(entity).insert((brain_for(&level.0, critter, at.translation.xz()), Driven));
    }
}

/// The mind of `critter` standing at `at` (world `(x, z)`) in `level`: wandering its home room,
/// with its attack ready. A creature wakes with one, and a troll that rises gets a new one.
pub fn brain_for(level: &Level, critter: &Critter, at: Vec2) -> Brain {
    // Home is where the level placed it, even when it comes back somewhere else after the
    // delver left the level ([`crate::record`]): it wanders back to its own room.
    let [hx, _, hz] = fb_level::Level::world(critter.spot.x, critter.spot.y);
    let home = Vec2::new(hx, hz);
    let mut rng = Mulberry32::new(critter.spot.seed ^ 0xB4A1);
    let wait = rng.range_f32(0.5, 4.0);
    let (x, y) = cell(home);
    Brain {
        mode: Mode::Wander { target: None, wait },
        home,
        room: level.room_at(x, y),
        cooldown: 0.0,
        attack: None,
        bitten: false,
        path: Vec::new(),
        goal: cell(at),
        repath: 0.0,
        stuck: 0.0,
        flee: None,
        flee_in: 0.0,
        cornered: false,
        rng,
        shun_roll: shun_roll(critter.spot.seed),
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
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn behave(
    time: Res<Time>,
    level: Res<CurrentLevel>,
    tuning: Res<Tuning>,
    rules: Option<Res<crate::rules::GameRules>>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Res<Obstacles>,
    doors: Option<Res<crate::doors::Doors>>,
    player: Query<DelverToStrike, (With<Player>, Without<Critter>)>,
    mut creatures: Query<(Entity, &Critter, &mut Brain, &mut Intent, &mut Transform, Option<&mut Flight>, Has<Aroused>)>,
    mut vaults: Local<Option<((u32, u32), Vaults)>>,
    mut attacks: BlowsAtDelver,
    mut commands: Commands,
    influences: Influences,
    sounds: Option<Res<crate::combat::feedback::CreatureSounds>>,
    bone_sounds: Option<Res<super::skeletons::BoneSounds>>,
    settings: Option<Res<crate::settings::GameSettings>>,
) {
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }
    let level = &level.0;
    let relief = Relief::new(level);
    // A shut claim door is a wall to a creature, and opaque.
    let blocked = crate::doors::solid_for(level, &blocked_cells, doors.as_deref(), None);
    let see = crate::doors::opaque_for(level, &blocked_cells, doors.as_deref(), None);
    let delver_ac = player.iter().next().and_then(|(_, ac)| ac.copied());
    let player = player.iter().next().map(|(t, _)| t.translation.xz());
    let shunned = influences.shunned.iter().next().copied().unwrap_or_default();
    // Every creature's circle, to keep the others out of.
    let bodies: Vec<(Entity, Vec2, f32)> = creatures.iter().map(|(e, c, _, _, t, ..)| (e, t.translation.xz(), body_radius(c))).collect();
    // A stone hall's vaults, worked out once for the level, for the bats that fly under them.
    if level.style == fb_level::LevelStyle::StoneHalls && vaults.as_ref().is_none_or(|(key, _)| *key != (level.seed, level.depth)) && creatures.iter().any(|(_, c, ..)| c.spot.species.is_bat()) {
        *vaults = Some(((level.seed, level.depth), Vaults::new(level)));
    }
    let vaults = vaults.as_ref().filter(|_| level.style == fb_level::LevelStyle::StoneHalls).map(|(_, v)| v);
    let t = time.elapsed_secs();
    let (tune, player_radius) = (&tuning.creatures, tuning.moving.body_radius);
    for (entity, critter, mut brain, mut intent, mut transform, mut flight, aroused) in &mut creatures {
        let brain = &mut *brain;
        let species = critter.spot.species;
        let snake = species.is_snake();
        let bat = species.is_bat();
        let at = transform.translation.xz();
        let radius = body_radius(critter);
        // A creature whose own roll falls under the delver's shun chance for its kind does not
        // notice them, unless the delver has struck it.
        let (frightened, charmed) = influences.of(entity);
        let heeds = !charmed && (aroused || !shunned.passes_over(species.foe(), brain.shun_roll));
        let sees = player.filter(|p| heeds && p.distance(at) <= notice_range(species, tune.sense_scale) && line_of_sight(grid(at), grid(*p), &see));
        // One seen only through a grate, with no way to them, is soon given up (it stands at the
        // grate pressing on otherwise): after pressing against it, it loses them, and rests a while
        // before it takes notice again.
        let sees = sees.filter(|p| {
            line_of_sight(grid(at), grid(*p), &blocked) || !gives_up_at_grate(brain.stuck, matches!(brain.mode, Mode::Wander { wait, .. } if wait > 0.0))
        });
        brain.cooldown = (brain.cooldown - dt).max(0.0);

        // Notice, follow, give up.
        match (&mut brain.mode, sees) {
            (Mode::Chase { last_seen, lost }, Some(p)) => {
                *last_seen = p;
                *lost = 0.0;
            }
            (Mode::Chase { last_seen, lost }, None) => {
                *lost += dt;
                // A jackalope has nothing to go back to: it stops running once it has lost them.
                let there = species.is_jackalope() || last_seen.distance(at) < reach(critter, player_radius) + CELL * 0.25;
                if (*lost > LOSE_AFTER && there) || *lost > 3.0 * LOSE_AFTER {
                    let wait = if brain.stuck > STUCK_AT_GRATE { GRATE_REST } else { brain.rng.range_f32(0.5, 2.0) };
                    brain.mode = Mode::Wander { target: None, wait };
                }
            }
            (Mode::Wander { .. }, Some(p)) => {
                brain.mode = Mode::Chase { last_seen: p, lost: 0.0 };
                brain.stuck = 0.0;
                brain.flee_in = 0.0;
            }
            (Mode::Wander { .. }, None) => {}
        }
        // A frightened or charmed creature does not give chase or attack.
        if frightened || charmed {
            if matches!(brain.mode, Mode::Chase { .. }) {
                brain.mode = Mode::Wander { target: None, wait: 0.5 };
            }
            brain.attack = None;
        }

        // A bat gets going the moment it notices the delver, and goes back to its spot when it
        // loses them.
        if let Some(flight) = flight.as_deref_mut() {
            let chasing = matches!(brain.mode, Mode::Chase { .. });
            match (chasing, flight.phase) {
                (true, Wing::Roosting | Wing::Returning) => {
                    flight.phase = Wing::Wheeling;
                    flight.timer = brain.rng.range_f32(0.5, 1.5);
                }
                (false, Wing::Wheeling | Wing::Swooping | Wing::Away) => {
                    flight.phase = Wing::Returning;
                    flight.timer = RETURN_GIVE_UP;
                }
                _ => {}
            }
        }
        if snake && let Mode::Chase { last_seen, .. } = brain.mode
            && last_seen.distance(brain.home) > SNAKE_LEASH + reach(critter, player_radius)
        {
            brain.mode = Mode::Wander { target: Some(brain.home), wait: 0.0 };
        }

        // A spider that has lost the delver goes back to where it lay in wait.
        if species.is_spider()
            && brain.stuck <= 1.5
            && at.distance(brain.home) > radius + 1.0
            && let Mode::Wander { target: target @ None, .. } = &mut brain.mode
        {
            *target = Some(brain.home);
        }

        // Where to go, how fast.
        let mut heading: Option<Vec2> = None;
        let mut speed = 0.0;
        let mut face: Option<Vec2> = None;
        match &mut brain.mode {
            Mode::Wander { target, wait } => {
                if target.is_none() {
                    *wait -= dt;
                    // A spider lies in wait where it is.
                    if *wait <= 0.0 && !species.is_spider() {
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
                        // A centipede runs in bursts with pauses between them.
                        let pace = if snake {
                            0.35
                        } else if species.is_centipede() {
                            if scuttling(t, critter.spot.seed) { CENTIPEDE_BURST } else { 0.0 }
                        } else {
                            0.45
                        };
                        heading = Some(goal);
                        speed = species.speed() * pace;
                    }
                }
            }
            Mode::Chase { last_seen, .. } => {
                let goal = sees.unwrap_or(*last_seen);
                face = Some(goal);
                let in_reach = player.is_some_and(|p| p.distance(at) <= reach(critter, player_radius)) && sees.is_some();
                // A jackalope runs from the delver to the farthest place it can reach first, and
                // turns on them only when it can run no farther.
                let mut strikes = in_reach;
                if species.is_jackalope() {
                    brain.flee_in -= dt;
                    if brain.flee_in <= 0.0 || brain.flee.is_some_and(|goal| goal == cell(at)) {
                        brain.flee = flee_goal(cell(at), cell(goal), &blocked);
                        brain.flee_in = FLEE_REPLAN;
                        brain.cornered = brain.flee.is_none();
                    }
                    strikes = in_reach && (brain.cornered || brain.stuck > CORNERED_AFTER);
                    if !strikes
                        && brain.attack.is_none()
                        && let Some((x, y)) = brain.flee
                    {
                        heading = Some((Vec2::new(x as f32, y as f32) + Vec2::splat(0.5)) * CELL);
                        speed = species.speed() * JACKALOPE_BOLT;
                    }
                    // Running, it looks where it goes; cornered, it faces the delver.
                    if !strikes {
                        face = None;
                    }
                } else if !in_reach && brain.attack.is_none() {
                    heading = Some(goal);
                    speed = species.speed() * if snake { tune.snake_chase_speed } else if species.is_skeleton() || species.is_troll() { 1.0 } else if species.is_spider() { SPIDER_CHASE } else if species.is_centipede() { CENTIPEDE_CHASE } else { tune.rat_chase_speed };
                }
                // A bat bites only swooping at the delver's head.
                let at_height = flight.as_deref().is_none_or(|f| f.phase == Wing::Swooping && (f.altitude - EYE_HEIGHT).abs() <= BITE_BAND);
                if strikes && at_height && brain.attack.is_none() && brain.cooldown <= 0.0 && !attacks.delver_is_down() {
                    brain.attack = Some(0.0);
                    brain.bitten = false;
                    // The cue sounds from the creature as the lunge or rear begins, before the bite.
                    // A skeleton's is its own: a rattle of bone as the arm goes back.
                    let game = settings.as_deref().copied().unwrap_or_default();
                    if species.is_skeleton() {
                        if let Some(sounds) = &bone_sounds {
                            super::skeletons::play_at(&mut commands, "creature attack cue", sounds.rattle.clone(), transform.translation, &game, &tuning);
                        }
                    } else if species.is_troll() || species.is_centipede() {
                        // The arms come up in silence: the thud of the blow is the cue. A centipede
                        // makes no sound at all, which is worse.
                    } else if let Some(sounds) = &sounds {
                        // A spider's is a hiss, as it rears.
                        let cue = if snake || species.is_spider() { sounds.snake_rise.clone() } else { sounds.rat_squeal.clone() };
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

        // A frightened creature runs from the delver to the farthest place it reaches first.
        if frightened
            && flight.is_none()
            && let Some(p) = player.filter(|p| p.distance(at) <= FRIGHT_SIGHT)
        {
            brain.flee_in -= dt;
            if brain.flee_in <= 0.0 || brain.flee.is_some_and(|goal| goal == cell(at)) {
                brain.flee = flee_goal(cell(at), cell(p), &blocked);
                brain.flee_in = FLEE_REPLAN;
                brain.cornered = brain.flee.is_none();
            }
            match brain.flee {
                Some((x, y)) => {
                    heading = Some((Vec2::new(x as f32, y as f32) + Vec2::splat(0.5)) * CELL);
                    speed = species.speed() * FRIGHT_BOLT;
                    face = None;
                }
                None => {
                    heading = None;
                    speed = 0.0;
                    face = Some(p);
                }
            }
        }

        // A bat flies where it likes: wheeling, swooping, pulling away or going home, at its
        // own height under the ceiling.
        let mut flying = None;
        if let Some(flight) = flight.as_deref_mut() {
            let goal = match brain.mode {
                Mode::Chase { last_seen, .. } => Some(sees.unwrap_or(last_seen)),
                Mode::Wander { .. } => None,
            };
            let ceiling = ceiling_over(level, vaults, &relief, at);
            let air = Air { ceiling, giant: species == Species::GiantBat, fly: species.fly_speed() };
            let bitten = brain.attack.is_some() && brain.bitten;
            let cues = FlightCues { at, home: brain.home, goal, sees: sees.is_some(), bitten };
            let plan = fly(flight, &air, &cues, &blocked, &mut brain.rng, dt);
            heading = plan.heading;
            speed = plan.speed;
            face = None;
            flying = Some(plan.power);
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
            let mut direction = (target - at).normalize_or_zero();
            // A bat carries on the way it faces, coming round as fast as it can turn.
            if flying.is_some() && direction != Vec2::ZERO {
                let ahead = (transform.rotation * Vec3::Z).xz();
                speed *= 0.35 + 0.65 * ahead.dot(direction).max(0.0);
                direction = (ahead + direction).normalize_or(direction);
            }
            if direction != Vec2::ZERO {
                let others: Vec<Obstacle> = bodies
                    .iter()
                    .filter(|(e, _, _)| *e != entity)
                    .map(|&(_, centre, r)| Obstacle::Circle { centre, radius: r })
                    .chain(player.map(|centre| Obstacle::Circle { centre, radius: player_radius }))
                    .chain(obstacles.0.iter().copied())
                    .collect();
                // Webs on the floor hold back all but the spiders that spun them.
                let speed = speed * fb_level::webs::web_pace(&level.webs, at.x, at.y, radius, Some(species), tuning.moving.web_pace);
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
                let rate = if bat { if species == Species::GiantBat { GIANT_BAT_TURN } else { BAT_TURN } } else if snake { SNAKE_TURN } else if species.is_skeleton() { SKELETON_TURN } else if species.is_spider() { SPIDER_TURN } else if species.is_troll() { TROLL_TURN } else if species.is_centipede() { CENTIPEDE_TURN } else { RAT_TURN };
                let turned = turn.clamp(-rate * dt, rate * dt);
                transform.rotation = Quat::from_rotation_y(yaw + turned);
                if let Some(flight) = flight.as_deref_mut() {
                    flight.turning = turned / dt;
                }
            }
        }

        // The attack under way, and what it looks like.
        *intent = Intent::default();
        if let (Some(flight), Some(power)) = (flight.as_deref(), flying) {
            intent.wings = flight.wings(power);
        }
        let chasing = matches!(brain.mode, Mode::Chase { .. });
        if snake {
            let moving = speed > 0.0;
            intent.coil = if moving { 0.0 } else { 1.0 };
            intent.rear = if chasing { 1.0 } else { 0.3 + 0.35 * (0.5 + 0.5 * (t * 0.35 + critter.spot.seed as f32).sin()) };
            intent.tongue = tongue_flick(t + (critter.spot.seed % 97) as f32 * 0.1);
        }
        if species.is_jackalope() && speed == 0.0 && !chasing {
            // Standing, it sometimes sits up with its ears turning.
            intent.rear = if sitting_up(t, critter.spot.seed) { 1.0 } else { 0.0 };
        }
        if species.is_centipede() {
            // Its front lifts a little as it comes on.
            intent.rear = if chasing { 0.3 } else { 0.0 };
        }
        if let Some(since) = brain.attack.as_mut() {
            *since += dt;
            let since = *since;
            // A skeleton swings its weapon: its own time, the blow landing as the arm comes down.
            let swing_time = critter.kit.as_ref().filter(|_| species.is_skeleton()).map(|k| k.swing_time);
            let (lands, done) = match swing_time {
                Some(time) => (super::skeletons::LANDS * time, time),
                None if species.is_troll() => (TROLL_LANDS * TROLL_SWING, TROLL_SWING),
                None if snake => (STRIKE_WINDUP + STRIKE_THROW, STRIKE_TIME),
                None if species.is_spider() => (SPIDER_WINDUP + SPIDER_STAB, SPIDER_TIME),
                None if species.is_jackalope() => (GORE_LANDS, GORE_TIME),
                None => (BITE_LANDS, BITE_TIME),
            };
            if let Some(time) = swing_time {
                intent.swing = Some((since / time).min(1.0));
            } else if species.is_troll() {
                intent.swing = Some((since / TROLL_SWING).min(1.0));
            } else if snake {
                intent.coil = 1.0;
                intent.rear = 1.0;
                let thrown = (STRIKE_WINDUP..STRIKE_WINDUP + 0.35).contains(&since);
                intent.strike = if thrown { 1.0 } else { 0.0 };
                intent.jaw = intent.strike;
                intent.tongue = 0.0;
            } else if species.is_spider() {
                // Reared on its back legs, then the stab.
                intent.rear = 1.0;
                intent.strike = if (SPIDER_WINDUP..SPIDER_WINDUP + 0.35).contains(&since) { 1.0 } else { 0.0 };
            } else if species.is_jackalope() {
                // Head down, antlers ahead, thrown forward.
                intent.strike = if since < GORE_TIME * 0.7 { 1.0 } else { 0.0 };
            } else {
                intent.strike = if since < BITE_TIME * 0.6 { 1.0 } else { 0.0 };
            }
            if !brain.bitten && since >= lands && species.is_skeleton() {
                brain.bitten = true;
                // The weapon's blow: only if the delver has not stepped out of its reach.
                // Nor around a wall: it needs sight of the delver at the moment it lands.
                let within = player.is_some_and(|p| p.distance(at) <= reach(critter, player_radius) * SKELETON_SLACK && line_of_sight(grid(at), grid(p), &blocked));
                if let (true, Some(kit)) = (within, critter.kit.as_ref()) {
                    let damage = kit.roll_damage(&mut brain.rng);
                    let blow = AttackOnDelver { attacker: entity, species: Some(species), damage, poison: 0, attacker_pos: at };
                    attacks.strike("A skeleton", blow, critter.to_hit_with(crate::rules::GameRules::of(&rules)), delver_ac.as_ref(), &mut brain.rng);
                }
            }
            if !brain.bitten && since >= lands && species.is_troll() {
                brain.bitten = true;
                // Multiattack: a bite and two claws, each rolled on its own. Nor around a wall,
                // nor if the delver has stepped out of its reach.
                let within = player.is_some_and(|p| p.distance(at) <= reach(critter, player_radius) * TROLL_SLACK && line_of_sight(grid(at), grid(p), &blocked));
                if within {
                    let block = crate::rules::GameRules::of(&rules).creature(species);
                    let to_hit = critter.to_hit_with(crate::rules::GameRules::of(&rules));
                    // Armed, the weapon takes the place of the right claw.
                    let blows: Vec<fb_creatures::Dice> = match critter.arms.as_ref() {
                        Some(arms) => arms.attack(block.bite).to_vec(),
                        None => std::iter::once(block.bite).chain(std::iter::repeat_n(fb_creatures::troll::CLAW, fb_creatures::troll::CLAWS as usize)).collect(),
                    };
                    for dice in blows {
                        let blow = AttackOnDelver { attacker: entity, species: Some(species), damage: dice.roll(&mut brain.rng), poison: 0, attacker_pos: at };
                        attacks.strike(block.name, blow, to_hit, delver_ac.as_ref(), &mut brain.rng);
                    }
                }
            }
            if !brain.bitten && since >= lands {
                brain.bitten = true;
                // A bite lands only with the delver in sight at that moment, not round a wall.
                if player.is_some_and(|p| line_of_sight(grid(at), grid(p), &blocked)) {
                    let block = crate::rules::GameRules::of(&rules).creature(species);
                    let damage = block.bite.roll(&mut brain.rng);
                    let poison = block.venom.map_or(0, |v| v.roll(&mut brain.rng));
                    let blow = AttackOnDelver { attacker: entity, species: Some(species), damage, poison, attacker_pos: at };
                    attacks.strike(block.name, blow, critter.to_hit_with(crate::rules::GameRules::of(&rules)), delver_ac.as_ref(), &mut brain.rng);
                }
            }
            if since >= done {
                brain.attack = None;
                brain.cooldown = if snake {
                    tune.snake_cooldown
                } else if species.is_skeleton() {
                    SKELETON_COOLDOWN
                } else if species.is_spider() {
                    SPIDER_COOLDOWN
                } else if species.is_troll() {
                    TROLL_COOLDOWN
                } else if species.is_centipede() {
                    CENTIPEDE_COOLDOWN
                } else if species.is_jackalope() {
                    JACKALOPE_COOLDOWN
                } else {
                    tune.rat_cooldown
                };
            }
        }
    }
}

/// The ceiling over world `(x, z)` as a height over the floor there: a stone hall's vault
/// (`vaults`), or the rough rock's flat ceiling less the most its relief comes down.
pub fn ceiling_over(level: &Level, vaults: Option<&Vaults>, relief: &Relief, at: Vec2) -> f32 {
    let top = vaults.map_or(level.ceiling - ROCK_CEILING_RELIEF, |v| v.height(at.x, at.y));
    top - relief.floor_height(at.x, at.y)
}

/// How far a rough rock ceiling wanders down from its nominal height, at most, world units.
const ROCK_CEILING_RELIEF: f32 = 1.2;
/// How near a flying bat's body comes to the floor and the ceiling beyond what its pose reaches,
/// world units. Our own.
const AIR_MARGIN: f32 = 0.6;
/// How high above the floor a bat wheels at its lowest and swoops to, world units: head height
/// for a bat, lower for a giant bat. Our own.
const BAT_WHEEL_LOW: f32 = 14.0;
const GIANT_WHEEL_LOW: f32 = 10.0;
const BAT_SWOOP: f32 = 14.0;
const GIANT_SWOOP: f32 = 11.0;
/// How wide a bat circles the delver, in cells; a giant bat wheels wider. Our own.
const BAT_CIRCLE: f32 = 0.9;
const GIANT_CIRCLE: f32 = 1.3;
/// A bat's wheeling speed as a share of its flying speed; swooping and pulling away are flat out.
const WHEEL_PACE: f32 = 0.55;

/// The room a flying bat has: how high the ceiling is over the floor under it, whether it is
/// a giant, and its flying speed.
#[derive(Clone, Copy, Debug)]
pub struct Air {
    pub ceiling: f32,
    pub giant: bool,
    pub fly: f32,
}

/// What a flying bat senses this frame.
#[derive(Clone, Copy, Debug)]
pub struct FlightCues {
    /// Where it is on the floor plan and where it roosts (world `(x, z)`).
    pub at: Vec2,
    pub home: Vec2,
    /// The delver, or where it last saw them, while it is after them.
    pub goal: Option<Vec2>,
    /// Whether it sees them this moment.
    pub sees: bool,
    /// Whether its bite has landed this attack.
    pub bitten: bool,
}

/// Where a bat means to go this frame, and how hard it beats its wings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Course {
    pub heading: Option<Vec2>,
    pub speed: f32,
    pub power: f32,
}

/// The heights a bat flies between in `air` as `flight` reaches: the lowest it wheels, the
/// highest, and what it swoops to bite at, all for the middle of its body over the floor.
pub fn flying_heights(flight: &Flight, air: &Air) -> (f32, f32, f32) {
    let floor = flight.below + AIR_MARGIN;
    let high = (air.ceiling - flight.above - AIR_MARGIN).max(floor);
    let low = (if air.giant { GIANT_WHEEL_LOW } else { BAT_WHEEL_LOW }).clamp(floor, high);
    let swoop = (if air.giant { GIANT_SWOOP } else { BAT_SWOOP }).clamp(floor, high);
    (low, high, swoop)
}

/// One frame of a bat's flight: which phase it is in and what comes next, where it heads, and
/// its height over the floor, moved toward the height it wants and held between the floor and
/// the ceiling whatever else happens. It roosts hanging from the ceiling until the delver is
/// noticed ([`Wing::Wheeling`] is set by the caller), wheels round them at head height and
/// above, swoops in to bite, pulls up and away, and comes round again; when it loses them it
/// returns to its spot and roosts. Our own tuning throughout.
pub fn fly(flight: &mut Flight, air: &Air, cues: &FlightCues, blocked: &impl Fn(i32, i32) -> bool, rng: &mut Mulberry32, dt: f32) -> Course {
    let (low, high, swoop) = flying_heights(flight, air);
    let roost = air.ceiling - flight.roost_above;
    let at = cues.at;
    let mut course = Course { heading: None, speed: 0.0, power: 0.0 };
    let mut want = roost;
    flight.timer -= dt;
    match flight.phase {
        Wing::Roosting => {}
        Wing::Returning => {
            let there = cues.home.distance(at);
            // Out of time to find the way: hang wherever it is.
            let home = if flight.timer <= 0.0 { at } else { cues.home };
            if there > 1.5 && flight.timer > 0.0 {
                course = Course { heading: Some(home), speed: (WHEEL_PACE * air.fly).min(there * 4.0).max(1.0), power: 0.8 };
                want = 0.5 * (low + high);
            } else if flight.altitude >= roost - 0.3 {
                flight.phase = Wing::Roosting;
                flight.climb = 0.0;
            } else {
                course.power = 1.0;
            }
        }
        Wing::Wheeling => {
            let Some(goal) = cues.goal else { return course };
            let circle = (if air.giant { GIANT_CIRCLE } else { BAT_CIRCLE }) * CELL;
            let pace = WHEEL_PACE * air.fly;
            flight.angle = (flight.angle + flight.spin * dt * pace / circle) % std::f32::consts::TAU;
            let mut aim = goal + Vec2::from_angle(flight.angle) * circle;
            if !line_of_sight(grid(goal), grid(aim), blocked) {
                // Rock in the way of the circle: round the other way, and straight at them.
                flight.spin = -flight.spin;
                aim = goal;
            }
            let climb = 0.5 * (high - low);
            want = 0.5 * (low + high) + climb * (2.0 * flight.angle).sin();
            course = Course { heading: Some(aim), speed: pace, power: if want > flight.altitude + 0.5 { 1.0 } else { 0.4 } };
            if flight.timer <= 0.0 && cues.sees {
                flight.phase = Wing::Swooping;
                flight.timer = 3.0;
            }
        }
        Wing::Swooping => {
            let Some(goal) = cues.goal else { return course };
            want = swoop;
            course = Course { heading: Some(goal), speed: air.fly, power: 0.0 };
            if cues.bitten || flight.timer <= 0.0 {
                flight.phase = Wing::Away;
                flight.timer = rng.range_f32(1.0, 1.8);
            }
        }
        Wing::Away => {
            let Some(goal) = cues.goal else { return course };
            want = high;
            // Straight away from them, or the nearest way that is open, or home.
            let from = (at - goal).normalize_or(Vec2::X);
            let run = 3.0 * CELL;
            let way = [0.0f32, 0.9, -0.9, 1.9, -1.9].into_iter().map(|turn| at + Vec2::from_angle(turn).rotate(from) * run).find(|&to| line_of_sight(grid(at), grid(to), blocked));
            course = Course { heading: Some(way.unwrap_or(cues.home)), speed: air.fly, power: 1.0 };
            if flight.timer <= 0.0 {
                flight.phase = Wing::Wheeling;
                flight.timer = rng.range_f32(1.0, 2.4);
                if rng.chance(0.5) {
                    flight.spin = -flight.spin;
                }
            }
        }
    }
    // Up and down: eased toward the height wanted, never past the floor or the ceiling.
    let vertical = air.fly.max(6.0);
    let target = ((want - flight.altitude) * 3.0).clamp(-1.5 * vertical, vertical);
    flight.climb += (target - flight.climb) * (1.0 - (-8.0 * dt).exp());
    let before = flight.altitude;
    flight.altitude += flight.climb * dt;
    // Hanging it may reach right up; in the air its wings need their room, but a bat just
    // let go of the roof comes down from where it was rather than being snapped there.
    let hung = matches!(flight.phase, Wing::Roosting | Wing::Returning);
    let top_roost = air.ceiling - flight.roost_above.min(flight.above);
    let top = if hung { top_roost } else { (air.ceiling - flight.above).max(before.min(top_roost)) };
    let bottom = flight.below;
    if flight.altitude < bottom || flight.altitude > top.max(bottom) {
        flight.altitude = flight.altitude.clamp(bottom, top.max(bottom));
        flight.climb = 0.0;
    }
    course
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
        level_app_on(1207, player_at)
    }

    /// [`level_app`] on the first level of `seed`.
    fn level_app_on(seed: u32, player_at: Vec2) -> (App, Vec<Entity>, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .add_plugins(BehaviourPlugin)
            .add_systems(Update, (super::super::idle, animate).chain().after(behave));
        let level = fb_level::generate(seed, 1);
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

    fn attacks(app: &mut App) -> Vec<AttackOnDelver> {
        let messages = app.world().resource::<Messages<AttackOnDelver>>();
        messages.iter_current_update_messages().copied().collect()
    }

    #[test]
    fn half_the_rats_shun_a_delver_with_a_fifty_percent_vermin_shun_and_the_same_ones_every_run() {
        use crate::combat::vitals::Shunned;
        let mut shun = [0.0; fb_items::Foe::COUNT];
        shun[fb_items::Foe::Vermin.index()] = 0.5;
        let delver = Shunned(shun);
        let passes = |seed: u32| delver.passes_over(Species::Rat.foe(), shun_roll(seed));
        let first: Vec<bool> = (0..200).map(passes).collect();
        let second: Vec<bool> = (0..200).map(passes).collect();
        assert_eq!(first, second, "the same rats every run");
        let unaware = first.iter().filter(|&&p| p).count();
        assert!((85..=115).contains(&unaware), "{unaware} of 200 rats never notice");
        assert!((0..200).all(|s| !Shunned::default().passes_over(Species::Rat.foe(), shun_roll(s))), "no shun, every rat notices");
    }

    #[test]
    fn a_shunning_rat_ignores_the_delver_until_it_is_struck_and_then_chases() {
        use crate::combat::vitals::Shunned;
        let level = fb_level::generate(1207, 1);
        let first = level.creatures.iter().find(|c| c.species.is_rat()).unwrap();
        let room = level.rooms[level.room_at(first.x as i32, first.y as i32).unwrap()];
        let (cx, cy) = room.centre();
        let player_at = Vec2::new(cx, cy) * CELL + Vec2::new(-1.0, 1.3) * CELL;
        let (mut app, roots, player) = level_app(player_at);
        // Every vermin shuns them (a chance of 1 is above every roll).
        let mut shun = [0.0; fb_items::Foe::COUNT];
        shun[fb_items::Foe::Vermin.index()] = 1.0;
        app.world_mut().entity_mut(player).insert(Shunned(shun));
        let pack = rat_pack(&app, &roots);
        for _ in 0..600 {
            app.update();
            assert!(attacks(&mut app).iter().all(|a| !pack.contains(&a.attacker)), "a rat that shuns them attacked");
        }
        for &rat in &pack {
            app.world_mut().entity_mut(rat).insert(Aroused);
        }
        let mut bitten = false;
        for _ in 0..600 {
            app.update();
            bitten |= attacks(&mut app).iter().any(|a| pack.contains(&a.attacker));
        }
        assert!(bitten, "struck rats chase and bite");
    }

    /// A lone rat in the big room with the delver `gap` away.
    fn rat_and_delver(gap: f32) -> (App, Entity, Entity, Vec2) {
        let (level, middle, _) = big_room();
        let player_at = middle + Vec2::X * gap;
        let (app, rat, player) = solo_app(Species::Rat, 5, &level, middle, player_at);
        (app, rat, player, middle)
    }

    #[test]
    fn a_frightened_rat_flees_for_its_duration_then_chases_again() {
        let (mut app, rat, player, _) = rat_and_delver(8.0);
        app.world_mut().entity_mut(rat).insert(Frightened(3.0));
        let start = position(&app, rat).distance(position(&app, player));
        let mut bitten = false;
        for _ in 0..120 {
            app.update();
            bitten |= attacks(&mut app).iter().any(|a| a.attacker == rat);
        }
        let gap = position(&app, rat).distance(position(&app, player));
        assert!(gap > start + 6.0, "it ran from {start} to only {gap}");
        assert!(!bitten, "a frightened rat bit");
        assert!(app.world().get::<Frightened>(rat).is_some());
        for _ in 0..120 {
            app.update();
        }
        assert!(app.world().get::<Frightened>(rat).is_none(), "the fright wore off");
        let mut closest = f32::MAX;
        for _ in 0..360 {
            app.update();
            closest = closest.min(position(&app, rat).distance(position(&app, player)));
        }
        assert!(closest < gap - 5.0, "it never came back: {closest} against {gap}");
    }

    #[test]
    fn a_charmed_rat_does_not_attack_until_struck() {
        let (mut app, rat, _player, _) = rat_and_delver(6.0);
        app.world_mut().entity_mut(rat).insert(Charmed(100.0));
        for _ in 0..300 {
            app.update();
            assert!(attacks(&mut app).iter().all(|a| a.attacker != rat), "a charmed rat bit");
        }
        assert!(app.world().get::<Charmed>(rat).is_some());
        app.world_mut().entity_mut(rat).insert(Aroused);
        app.update();
        assert!(app.world().get::<Charmed>(rat).is_none(), "a blow ended the charm");
        let mut bitten = false;
        for _ in 0..600 {
            app.update();
            bitten |= attacks(&mut app).iter().any(|a| a.attacker == rat);
        }
        assert!(bitten, "the struck rat bites again");
    }

    #[test]
    fn nothing_attacks_a_fallen_delver() {
        let level = fb_level::generate(1207, 1);
        let first = level.creatures.iter().find(|c| c.species.is_rat()).unwrap();
        let room = level.rooms[level.room_at(first.x as i32, first.y as i32).unwrap()];
        let (cx, cy) = room.centre();
        let player_at = Vec2::new(cx, cy) * CELL + Vec2::new(-1.0, 1.3) * CELL;
        let (mut app, _, player) = level_app(player_at);
        // Bitten a while first, then down.
        let mut before = 0;
        for _ in 0..600 {
            app.update();
            before += attacks(&mut app).len() + app.world().resource::<Messages<MissedDelver>>().iter_current_update_messages().count();
        }
        assert!(before > 0, "the rats never went for the delver");
        app.world_mut().entity_mut(player).insert(crate::combat::Health { hp: 0, max: 10 });
        for frame in 0..600 {
            app.update();
            assert!(attacks(&mut app).is_empty(), "bitten after the fall, at frame {frame}");
            assert_eq!(app.world().resource::<Messages<MissedDelver>>().iter_current_update_messages().count(), 0, "a blow at the fallen, at frame {frame}");
        }
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

    /// A skeleton with a spear mid-swing at a delver one cell of stone away, in reach (the
    /// spear outreaches the wall's thickness): the blow lands with nothing between them and not
    /// with a wall between them.
    #[test]
    fn a_blow_does_not_land_round_a_wall() {
        use crate::creatures::skeletons::SkeletonKit;
        use fb_level::Cell;
        let blow = |wall: bool| {
            let mut level = fb_level::generate(1207, 1);
            // A run of four open cells along a row, the second of which may be stone.
            let (x, y) = (0..level.height as i32)
                .flat_map(|y| (0..level.width as i32).map(move |x| (x, y)))
                .find(|&(x, y)| (0..4).all(|k| level.cell(x + k, y) == Cell::Floor))
                .expect("a straight run of floor");
            if wall {
                level.cells[(y as u32 * level.width + (x + 1) as u32) as usize] = Cell::Rock;
            }
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
                .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
                .add_plugins(BehaviourPlugin);
            app.insert_resource(Obstacles(Vec::new()));
            app.insert_resource(CurrentLevel(level.clone()));
            let state = fb_creatures::SkeletonState::roll(5);
            let outfit = fb_items::outfit::OutfitSet { worn: vec![(fb_items::Slot::Mainhand, fb_items::roll_item("Spear", 5).unwrap())], goods: Vec::new(), ground: Vec::new() };
            let kit = SkeletonKit::new(&state, outfit, [None, None]);
            let spot = fb_level::CreatureSpot { x: x as f32 + 0.8, y: y as f32 + 0.5, yaw: 0.0, species: Species::Skeleton, seed: 5, pack: 0 };
            let skeleton = {
                let mut commands = app.world_mut().commands();
                crate::creatures::spawn_skeleton_with(&mut commands, &level, &spot, &state.build(), Some(kit)).0
            };
            // One cell of stone between, the delver's centre 1.4 cells from the skeleton's.
            let player_at = Vec2::new(x as f32 + 2.2, y as f32 + 0.5) * CELL;
            app.world_mut().spawn((Player, Transform::from_xyz(player_at.x, 0.0, player_at.y)));
            app.world_mut().flush();
            app.update();
            let reaches = {
                let critter = app.world().get::<Critter>(skeleton).unwrap();
                reach(critter, BODY_RADIUS) * SKELETON_SLACK
            };
            let apart = app.world().get::<Transform>(skeleton).unwrap().translation.xz().distance(player_at);
            assert!(apart <= reaches, "the delver is within reach: {apart} of {reaches}");
            {
                let mut brain = app.world_mut().get_mut::<Brain>(skeleton).unwrap();
                brain.mode = Mode::Chase { last_seen: player_at, lost: 0.0 };
                brain.attack = Some(0.0);
            }
            let mut landed = false;
            for _ in 0..180 {
                app.update();
                landed |= !attacks(&mut app).is_empty();
            }
            landed
        };
        assert!(blow(false), "a blow lands on a delver in reach and in sight");
        assert!(!blow(true), "a blow does not land through a wall");
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
        let seed = crate::creatures::seed_holding(&[Species::ConstrictorSnake, Species::PoisonousSnake]);
        let level = fb_level::generate(seed, 1);
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
        // The player walks round each snake's room, corner to corner, so each snake rears,
        // turns and follows.
        let spots: Vec<Vec2> = level
            .creatures
            .iter()
            .filter(|c| c.species.is_snake())
            .flat_map(|c| {
                let room = level.rooms[level.room_at(c.x as i32, c.y as i32).unwrap()];
                let (x0, y0, x1, y1) = (room.x as f32 + 1.0, room.y as f32 + 1.0, (room.x + room.w) as f32 - 1.0, (room.y + room.h) as f32 - 1.0);
                [Vec2::new(x0, y0), Vec2::new(x1, y1), Vec2::new(x0, y1), Vec2::new(x1, y0)]
            })
            .collect();
        let (mut app, roots, player) = level_app_on(seed, spots[0] * CELL);
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
        assert!(moved > CELL / 4.0, "the snakes stayed put: seed {seed} moved {moved} of {}", snakes.len());
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

    /// The roomiest room of seed 1207's level: its level, centre (world `(x, z)`) and half the
    /// width of floor it has clear on each side, world units.
    fn big_room() -> (Level, Vec2, f32) {
        let level = fb_level::generate(1207, 1);
        let room = *level.rooms.iter().max_by_key(|r| r.w.min(r.h)).unwrap();
        let (cx, cy) = room.centre();
        let half = (room.w.min(room.h) as f32 * 0.5 - 0.8) * CELL;
        (level, Vec2::new(cx, cy) * CELL, half)
    }

    /// One creature of `species` and `seed` alone in the big room's middle, thinking and moving
    /// with a player at `player_at` (world `(x, z)`), stepped 1/60 s a frame.
    fn solo_app(species: Species, seed: u32, level: &Level, at: Vec2, player_at: Vec2) -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .add_plugins(BehaviourPlugin)
            .add_systems(Update, (super::super::idle, animate).chain().after(behave));
        app.insert_resource(Obstacles(Vec::new()));
        app.insert_resource(CurrentLevel(level.clone()));
        let player = app.world_mut().spawn((Player, Transform::from_xyz(player_at.x, 0.0, player_at.y))).id();
        let spot = fb_level::CreatureSpot { x: at.x / CELL, y: at.y / CELL, yaw: 0.0, species, seed, pack: 0 };
        let creature = {
            let mut commands = app.world_mut().commands();
            spawn_skeleton(&mut commands, level, &spot, &spot.creature().rig()).0
        };
        app.world_mut().flush();
        (app, creature, player)
    }

    #[test]
    fn a_centipede_wanders_in_bursts_and_chases_at_twice_its_speed_without_pause() {
        let (level, middle, half) = big_room();
        let speed = Species::GiantCentipede.speed();
        // Far from the delver: in bursts with pauses between.
        let (mut app, centipede, player) = solo_app(Species::GiantCentipede, 3, &level, middle, Vec2::new(5.0, 5.0));
        let (mut last, mut bursts, mut pauses, mut fastest) = (position(&app, centipede), 0, 0, 0.0f32);
        for _ in 0..(60 * 40) {
            app.update();
            let now = position(&app, centipede);
            let v = now.distance(last) * 60.0;
            last = now;
            fastest = fastest.max(v);
            if app.world().get::<Brain>(centipede).is_some_and(|b| matches!(b.mode, Mode::Wander { target: Some(_), .. })) {
                if v > 0.5 * speed { bursts += 1 } else if v < 0.01 { pauses += 1 }
            }
        }
        assert!(bursts > 60 && pauses > 60, "{bursts} frames of running and {pauses} of pausing on the way");
        assert!(fastest < 1.2 * speed, "a wandering burst is no faster than its walk: {fastest}");
        // Noticing the delver: twice its speed, and never still on the way.
        let range = notice_range(Species::GiantCentipede, 1.0);
        let _ = player;
        let (mut app, centipede, _) = solo_app(Species::GiantCentipede, 3, &level, middle, middle + Vec2::X * (0.8 * range).min(half));
        let (mut still, mut quick) = (0, 0);
        let mut last = position(&app, centipede);
        for _ in 0..40 {
            app.update();
            let now = position(&app, centipede);
            let v = now.distance(last) * 60.0;
            last = now;
            if app.world().get::<Brain>(centipede).is_some_and(|b| matches!(b.mode, Mode::Chase { .. })) && !app.world().get::<Brain>(centipede).unwrap().attack.is_some() {
                if v < 0.1 * speed { still += 1 }
                if v > 1.7 * speed { quick += 1 }
            }
        }
        assert!(quick > 10, "it chases at twice its speed ({quick} quick frames)");
        assert_eq!(still, 0, "and never pauses while it comes on");
    }

    #[test]
    fn a_centipede_bite_carries_its_venom() {
        let (level, middle, half) = big_room();
        let (mut app, _, _) = solo_app(Species::GiantCentipede, 3, &level, middle, middle + Vec2::X * half);
        let mut bites = Vec::new();
        for _ in 0..(60 * 20) {
            app.update();
            bites.extend(attacks(&mut app));
        }
        assert!(!bites.is_empty(), "the centipede never bit");
        assert!(bites.iter().all(|b| b.species == Some(Species::GiantCentipede) && (3..=18).contains(&b.poison) && (3..=6).contains(&b.damage)), "{bites:?}");
    }

    /// Walks `player` toward `to` at `pace` world units a second for one frame.
    fn walk_toward(app: &mut App, player: Entity, to: Vec2, pace: f32) {
        let mut t = app.world_mut().get_mut::<Transform>(player).unwrap();
        let at = t.translation.xz();
        let step = (to - at).normalize_or_zero() * pace / 60.0;
        t.translation += Vec3::new(step.x, 0.0, step.y);
    }

    #[test]
    fn a_jackalope_moves_away_from_a_delver_walking_toward_it_and_does_not_gore() {
        let (level, middle, half) = big_room();
        let start = middle + Vec2::X * half;
        let (mut app, jackalope, player) = solo_app(Species::Jackalope, 5, &level, middle, start);
        let (mut gored, mut closest) = (false, f32::MAX);
        let mut gaps = Vec::new();
        for frame in 0..(60 * 6) {
            let here = position(&app, jackalope);
            walk_toward(&mut app, player, here, 12.0);
            app.update();
            gored |= !attacks(&mut app).is_empty() || app.world().resource::<Messages<MissedDelver>>().iter_current_update_messages().count() > 0;
            let gap = position(&app, jackalope).distance(position(&app, player));
            closest = closest.min(gap);
            if frame % 60 == 59 {
                gaps.push(gap);
            }
        }
        assert!(!gored, "it gored a delver it could run from");
        // The delver walks at 12 a second and the jackalope bolts at 1.5 times its 25: the gap
        // opens in the first seconds, while there is floor to run over.
        assert!(gaps[1] > gaps[0] || gaps[2] > gaps[0], "the gap never opened: {gaps:?}");
        assert!(closest > 0.5 * half, "it let the delver within {closest} of it");
    }

    /// A jackalope in a pocket of one cell, its neighbours blocked, with the delver in reach of it
    /// (`pocket`), or the same with the room open round it.
    fn gores(pocket: bool) -> bool {
        let (level, middle, _) = big_room();
        let cell_at = (middle / CELL).floor();
        let at = (cell_at + Vec2::splat(0.5)) * CELL;
        let reaches = {
            let spot = fb_level::CreatureSpot { x: 0.0, y: 0.0, yaw: 0.0, species: Species::Jackalope, seed: 5, pack: 0 };
            reach(&Critter::new(spot, spot.creature()), BODY_RADIUS)
        };
        let player_at = at + Vec2::X * 0.8 * reaches;
        let (mut app, _, _) = solo_app(Species::Jackalope, 5, &level, at, player_at);
        if pocket {
            let mut blocked = app.world_mut().resource_mut::<BlockedCells>();
            for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                blocked.0.insert((cell_at.x as i32 + dx, cell_at.y as i32 + dy));
            }
        }
        let mut gored = false;
        for _ in 0..(60 * 4) {
            app.update();
            gored |= attacks(&mut app).iter().any(|a| a.species == Some(Species::Jackalope)) || app.world().resource::<Messages<MissedDelver>>().iter_current_update_messages().count() > 0;
        }
        gored
    }

    #[test]
    fn a_jackalope_gores_only_when_it_is_cornered_within_reach() {
        assert!(gores(true), "cornered with the delver in reach, it gores");
        assert!(!gores(false), "in the open, it runs instead");
    }

    #[test]
    fn a_fleeing_jackalopes_hops_show_in_its_pose_and_a_standing_ones_sit_up() {
        let (level, middle, half) = big_room();
        let body = |app: &App, jackalope: Entity| {
            let bones = app.world().get::<crate::creatures::Bones>(jackalope).unwrap();
            app.world().get::<Transform>(bones.0[fb_creatures::jackalope::bone::BODY]).unwrap().translation.y
        };
        let (mut app, jackalope, player) = solo_app(Species::Jackalope, 5, &level, middle, middle + Vec2::X * half);
        let Creature::Jackalope(state) = app.world().get::<Critter>(jackalope).unwrap().creature.clone() else { unreachable!() };
        let (mut low, mut high) = (f32::MAX, f32::MIN);
        for frame in 0..(60 * 3) {
            let here = position(&app, jackalope);
            walk_toward(&mut app, player, here, 5.0);
            app.update();
            if frame > 30 {
                let y = body(&app, jackalope);
                (low, high) = (low.min(y), high.max(y));
            }
        }
        assert!(high - low > 0.1 * state.length, "its body rises and falls with each hop: {low} to {high}");

        // Standing about far from the delver, it sits up now and then, and is on all fours between.
        let (mut app, jackalope, _) = solo_app(Species::Jackalope, 5, &level, middle, Vec2::new(5.0, 5.0));
        let (mut sat, mut stood) = (0, 0);
        for _ in 0..(60 * 40) {
            app.update();
            let rear = app.world().get::<Intent>(jackalope).unwrap().rear;
            if rear > 0.95 {
                sat += 1;
            } else if rear < 0.05 {
                stood += 1;
            }
        }
        assert!(sat > 60 && stood > 60, "{sat} frames sat up and {stood} on all fours");
    }

    #[test]
    fn it_flees_to_the_farthest_cell_it_reaches_first() {
        // A corridor along the top with a side branch at its sixth cell.
        let open = |x: i32, y: i32| ((0..=12).contains(&x) && y == 0) || (x == 6 && (1..=5).contains(&y));
        let blocked = |x: i32, y: i32| !open(x, y);
        // The delver at cell 4 and the jackalope past them at 8: it runs on to the far end.
        assert_eq!(flee_goal((8, 0), (4, 0), blocked), Some((12, 0)));
        // The delver beyond the far end: the other end, the nearer one only by getting past
        // the branch where it is closer than they are.
        assert_eq!(flee_goal((8, 0), (12, 0), blocked), Some((0, 0)));
        // Pinned against the end with the delver at its heels: nowhere farther to go.
        assert_eq!(flee_goal((12, 0), (11, 0), blocked), None);
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

    // Bats

    use fb_creatures::BatState;
    use fb_level::CreatureSpot;

    /// A bat of `species` rolled from `seed`, hanging in a room `ceiling` high.
    fn roosting_bat(species: Species, seed: u32, ceiling: f32) -> (Flight, Air) {
        let bat = BatState::roll_as(seed, species);
        let rig = bat.build();
        let flight = Flight::roosting(&bat, &rig, ceiling, seed);
        (flight, Air { ceiling, giant: species == Species::GiantBat, fly: species.fly_speed() })
    }

    #[test]
    fn a_bat_stays_between_the_floor_and_the_ceiling_over_a_long_flight() {
        for ceiling in [30.0, 40.0, 60.0] {
            for species in [Species::Bat, Species::GiantBat] {
                for seed in 0..4 {
                    let (mut flight, air) = roosting_bat(species, seed, ceiling);
                    let mut rng = Mulberry32::new(seed);
                    let (mut at, home) = (Vec2::new(100.0, 100.0), Vec2::new(100.0, 100.0));
                    let mut delver = Vec2::new(140.0, 100.0);
                    let open = |_: i32, _: i32| false;
                    let (mut flew, mut reached_head, mut reached_roof) = (0, false, false);
                    let (low, high, _) = flying_heights(&flight, &air);
                    flight.phase = Wing::Wheeling;
                    flight.timer = 1.0;
                    // Two minutes: wheeling round a delver who wanders, swooping and biting.
                    for frame in 0..7200 {
                        let t = frame as f32 / 60.0;
                        delver = Vec2::new(140.0, 100.0) + Vec2::from_angle(t * 0.3) * 15.0;
                        let bitten = flight.phase == Wing::Swooping && at.distance(delver) < 4.0;
                        let cues = FlightCues { at, home, goal: Some(delver), sees: true, bitten };
                        let course = fly(&mut flight, &air, &cues, &open, &mut rng, 1.0 / 60.0);
                        if let Some(to) = course.heading {
                            at += (to - at).normalize_or_zero() * course.speed / 60.0;
                        }
                        let (bottom, top) = (flight.below, ceiling - flight.roost_above.min(flight.above));
                        assert!(flight.altitude >= bottom - 1e-4 && flight.altitude <= top + 1e-4, "{species:?} {seed} under a {ceiling} ceiling: altitude {} at frame {frame}", flight.altitude);
                        if frame > 120 {
                            // In the air it keeps its wings off the stone: within the heights it flies at.
                            assert!(flight.altitude >= flight.below, "wings in the floor");
                            assert!(flight.altitude + flight.above <= ceiling + 0.5, "{species:?} {seed} under a {ceiling} ceiling: wings through it at {}", flight.altitude);
                        }
                        flew += (flight.phase == Wing::Swooping) as u32;
                        reached_head |= flight.altitude <= low + 1.5;
                        reached_roof |= flight.altitude >= high - 1.5;
                    }
                    let _ = delver;
                    assert!(flew > 0, "{species:?} {seed} never swooped");
                    assert!(reached_head && reached_roof, "{species:?} {seed} under a {ceiling} ceiling kept to one height ({low}..{high})");
                }
            }
        }
    }

    #[test]
    fn a_roosting_bat_does_not_stir_and_a_returning_one_hangs_again() {
        let (mut flight, air) = roosting_bat(Species::Bat, 3, 30.0);
        let hung = flight.altitude;
        let mut rng = Mulberry32::new(1);
        let (mut at, home) = (Vec2::new(40.0, 40.0), Vec2::new(100.0, 40.0));
        let open = |_: i32, _: i32| false;
        for _ in 0..300 {
            let cues = FlightCues { at: home, home, goal: None, sees: false, bitten: false };
            let course = fly(&mut flight, &air, &cues, &open, &mut rng, 1.0 / 60.0);
            assert_eq!(course.heading, None);
        }
        assert_eq!(flight.altitude, hung);
        assert_eq!(flight.phase, Wing::Roosting);
        // Lost the delver away from its spot: it flies home and hangs again.
        flight.phase = Wing::Returning;
        flight.timer = RETURN_GIVE_UP;
        flight.altitude = 15.0;
        for _ in 0..1800 {
            let cues = FlightCues { at, home, goal: None, sees: false, bitten: false };
            let course = fly(&mut flight, &air, &cues, &open, &mut rng, 1.0 / 60.0);
            if let Some(to) = course.heading {
                at += (to - at).normalize_or_zero() * course.speed / 60.0;
            }
        }
        assert_eq!(flight.phase, Wing::Roosting);
        assert!(at.distance(home) < 2.0 && (flight.altitude - hung).abs() < 0.5, "{at} {}", flight.altitude);
    }

    #[test]
    fn a_giant_bat_flies_lower_and_turns_slower() {
        let (small, small_air) = roosting_bat(Species::Bat, 1, 30.0);
        let (giant, giant_air) = roosting_bat(Species::GiantBat, 1, 30.0);
        let (small_low, _, small_swoop) = flying_heights(&small, &small_air);
        let (giant_low, _, giant_swoop) = flying_heights(&giant, &giant_air);
        assert!(giant_low < small_low && giant_swoop < small_swoop, "{giant_low} {small_low}");
        const { assert!(GIANT_BAT_TURN < BAT_TURN) };
    }

    /// An app of bats and a player only, thinking and moving, in the largest room of seed 1207.
    fn bat_app(species: Species, count: u32) -> (App, Vec<Entity>, Entity, Vec2) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .add_plugins(BehaviourPlugin)
            .add_systems(Update, (super::super::idle, animate).chain().after(behave));
        let level = fb_level::generate(1207, 1);
        let room = *level.rooms.iter().max_by_key(|r| r.w * r.h).unwrap();
        let (cx, cy) = room.centre();
        app.insert_resource(Obstacles(Vec::new()));
        app.insert_resource(CurrentLevel(level.clone()));
        let player_at = Vec2::new(cx, cy) * CELL + Vec2::new(-1.5, 1.0) * CELL;
        let player = app.world_mut().spawn((Player, Transform::from_xyz(player_at.x, 0.0, player_at.y))).id();
        let roots = {
            let mut commands = app.world_mut().commands();
            (0..count)
                .map(|i| {
                    let spot = CreatureSpot { x: cx + 0.3 * i as f32, y: cy, yaw: 0.0, species, seed: 11 + i, pack: 0 };
                    spawn_skeleton(&mut commands, &level, &spot, &spot.creature().rig()).0
                })
                .collect()
        };
        app.world_mut().flush();
        (app, roots, player, player_at)
    }

    fn flight(app: &App, bat: Entity) -> Flight {
        app.world().get::<Flight>(bat).unwrap().clone()
    }

    #[test]
    fn bats_hang_until_the_delver_comes_then_wheel_swoop_and_bite_then_roost_again() {
        let (mut app, bats, player, near) = bat_app(Species::Bat, 3);
        let level = fb_level::generate(1207, 1);
        let relief = Relief::new(&level);
        let hung: Vec<f32> = bats.iter().map(|&b| flight(&app, b).altitude).collect();
        // The delver far away across the level, out of sight: they hang.
        let far = {
            let (x, y) = (0..level.height as i32).flat_map(|y| (0..level.width as i32).map(move |x| (x, y))).filter(|&(x, y)| level.is_open(x, y)).max_by_key(|&(x, y)| (x * 20 - near.x as i32).abs() + (y * 20 - near.y as i32).abs()).unwrap();
            Vec2::new(x as f32 + 0.5, y as f32 + 0.5) * CELL
        };
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = Vec3::new(far.x, 0.0, far.y);
        for _ in 0..300 {
            app.update();
        }
        for (&b, &h) in bats.iter().zip(&hung) {
            let f = flight(&app, b);
            assert_eq!(f.phase, Wing::Roosting);
            assert_eq!(f.altitude, h);
        }
        // The delver walks in: they leave the ceiling, stay in the air and out of the rock, and bite.
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = Vec3::new(near.x, 0.0, near.y);
        let (mut blows, mut swooped, mut came_down) = (0, false, false);
        for frame in 0..1500 {
            app.update();
            blows += attacks(&mut app).len() + app.world().resource::<Messages<MissedDelver>>().iter_current_update_messages().count();
            for &b in &bats {
                let f = flight(&app, b);
                let at = position(&app, b);
                assert!(level.is_open((at.x / CELL).floor() as i32, (at.y / CELL).floor() as i32), "a bat in the rock at frame {frame}");
                let ceiling = crate::level::geometry::stone::ceiling_at(&level, at.x, at.y) - relief.floor_height(at.x, at.y);
                assert!(f.altitude >= f.below - 1e-3 && f.altitude + f.roost_above.min(f.above) <= ceiling + 1e-3, "bat out of bounds: {} in {ceiling}", f.altitude);
                swooped |= f.phase == Wing::Swooping;
                came_down |= f.altitude < 20.0;
            }
        }
        assert!(swooped && came_down, "they never swooped");
        assert!(blows > 0, "they never bit");
        // The delver leaves again: out of sight, they fly back and hang.
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = Vec3::new(far.x, 0.0, far.y);
        for _ in 0..3600 {
            app.update();
        }
        for &b in &bats {
            assert_eq!(flight(&app, b).phase, Wing::Roosting, "a bat has not gone back to roost");
        }
    }


    #[test]
    fn a_creature_that_sees_the_delver_through_a_grate_gives_up_and_rests() {
        use crate::doors::Doors;
        use fb_faction::Faction;
        use fb_level::{Cell, DoorKind, SwitchKind};
        assert!(gives_up_at_grate(STUCK_AT_GRATE + 0.1, false) && gives_up_at_grate(0.0, true) && !gives_up_at_grate(0.5, false));
        let mut level = fb_level::generate(1207, 1);
        // A corridor of seven cells along a row, shut in by rock all round, with a shut grate across it.
        let (x, y) = (0..level.height as i32)
            .flat_map(|y| (1..level.width as i32 - 8).map(move |x| (x, y)))
            .find(|&(x, y)| y > 0 && (0..7).all(|k| level.cell(x + k, y) == Cell::Floor))
            .expect("a run of floor");
        let width = level.width;
        for k in -1..=7 {
            for dy in [-1, 1] {
                level.cells[((y + dy) as u32 * width + (x + k) as u32) as usize] = Cell::Rock;
            }
        }
        for gx in [x - 1, x + 7] {
            level.cells[(y as u32 * width + gx as u32) as usize] = Cell::Rock;
        }
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .add_plugins(BehaviourPlugin)
            .add_systems(Update, (super::super::idle, animate).chain().after(behave));
        app.insert_resource(Obstacles(Vec::new()));
        app.insert_resource(CurrentLevel(level.clone()));
        let mut doors = Doors::default();
        let door = fb_level::ClaimDoor { x: x + 3, y, faction: Faction(1), room: 0, main: false, inward: (1, 0), shared: false, kind: DoorKind::Portcullis, switch: SwitchKind::Chain };
        doors.add(&door, &crate::level::geometry::Doorway { x: x + 3, y, through: [1.0, 0.0, 0.0] }, None);
        assert!(doors.shuts(x + 3, y));
        app.insert_resource(doors);
        let spot = fb_level::CreatureSpot { x: x as f32 + 1.5, y: y as f32 + 0.5, yaw: 0.0, species: Species::Rat, seed: 3, pack: 0 };
        let rat = {
            let mut commands = app.world_mut().commands();
            spawn_skeleton(&mut commands, &level, &spot, &spot.creature().rig()).0
        };
        let delver = Vec2::new(x as f32 + 5.0, y as f32 + 0.5) * CELL;
        app.world_mut().spawn((Player, Transform::from_xyz(delver.x, 0.0, delver.y)));
        app.world_mut().flush();
        let (mut chasing, mut resting, frames) = (0, 0, 60 * 40);
        for _ in 0..frames {
            app.update();
            match app.world().get::<Brain>(rat).unwrap().mode {
                Mode::Chase { .. } => chasing += 1,
                Mode::Wander { .. } => resting += 1,
            }
            assert!(position(&app, rat).x < (x + 3) as f32 * CELL, "the rat got through a shut grate");
        }
        // It noticed them, and did not press on at the grate for the whole forty seconds.
        assert!(chasing > 0, "it never noticed the delver through the grate");
        assert!(resting * 10 > frames * 4, "chasing {chasing} of {frames} frames: it stands at the grate for good");
    }
}
