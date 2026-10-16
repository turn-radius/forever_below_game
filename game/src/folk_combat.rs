//! Folk in a fight: their hit points and Armor Class, what a blow does to them, and their fall.
//!
//! - **Who they are in a fight** ([`FolkStats`]): a folk's abilities are their folk's scores in
//!   `data/races.json` (the scores the delver's are rolled about, `crate::character`), and their
//!   class is their outfit's (`data/outfits.json`: a fighter, ranger, cleric or mage). Hit points
//!   are a 1st-level character's, as the delver's are: the class's hit die at its highest plus
//!   the Constitution modifier (SRD 5.1 "Hit Points at 1st Level", [`crate::combat::first_level_hit_points`]).
//!   A scavenger or someone freshly fallen has no class: they have a commoner's hit die, a d8, at
//!   its average, 4 (SRD 5.1 "Commoner": 4 (1d8)), plus the modifier, at least 1. Armor Class is
//!   the delver's sum over what they wear and hold (10, the Dexterity modifier and each item's
//!   armour, [`fb_character::explain::armour_class`]), as a skeleton's is.
//! - **Struck** ([`take_folk_blows`]): a swing finds a folk at the circle they stand in
//!   (`combat::swing::Strikeable`) and rolls to hit against their [`ArmourClass`]; a blow that
//!   lands takes its damage from their [`Health`], flashes them and knocks them back a step as it
//!   does a creature ([`Flinches`], a Medium creature's push), with a thud from where they stand.
//! - **Felled**: at 0 they hold, flashing, for the flash's hold, then fall onto their back about
//!   their feet ([`collapse_folk`], `feedback::laid_back`, as a skeleton falls). The body is a
//!   [`Corpse`]: it stops its routine, no longer turns its head, blocks nobody and shows no card.
//!   "Name is slain" shows low in the view.
//!
//! - **What they leave** ([`shed_folk_gear`]): as they fall, every item they wore and held is set
//!   down round where they fell as the item it is (its rarity, affixes and wear), through the
//!   floor machinery, as a skeleton's is (`inventory::underfoot::set_down_around`), and their
//!   body is drawn again bare. The body is marked [`Looted`] at once, and one brought back from
//!   the record comes back bare and [`Looted`] (`crate::folk::spawn_folk`), so nothing is set down
//!   twice.
//! - **Fighting back** ([`fight`]): a folk given cause turns hostile (`crate::hostility`, with the
//!   kin who see it). A hostile folk leaves their routine and comes at the delver: straight at
//!   them in sight, else along the shortest way through the grid (`fb_level::path::find_path`,
//!   as creatures follow), sliding round rock, props, creatures, other folk and the delver, at
//!   twice their walking pace (the SRD's Dash, p. 93, as rats run). In reach they swing what
//!   their main hand holds ([`Combatant`]: the delver's own [`Weapon`] rules, its reach an arm to
//!   the wrist of their height and the weapon's length beyond, a fist an arm's length, a torch an
//!   improvised 1d4), the blow taking a creature's swing time for the weapon
//!   ([`crate::combat::swing_time`]) and landing as the arm comes down, if the delver is still in
//!   reach and in sight. It rolls to hit as the delver's swings do, the Strength modifier and the
//!   weapon's `ToHit` against the delver's Armor Class ([`BlowsAtDelver`]); a hit rolls the
//!   weapon's dice plus the Strength modifier and is sent as an [`AttackOnDelver`], which takes
//!   the delver's hit points as a creature's blow does, and a miss whooshes past. A friendly
//!   folk never attacks, however near the delver comes. One who has not seen the delver for
//!   `folk.lose_interest_after` seconds gives up, friendly again, and heads home.
//!
//! A folk's stance while lying is the one they had standing: the skinned figure is turned as a
//! whole, not ragdolled. Our own simplification.

use bevy::prelude::*;
use fb_character::explain::{Explained, armour_class};
use fb_character::modifier;
use fb_items::Slot;
use fb_level::CELL;
use fb_level::path::{find_path, line_of_sight, steer_toward};
use procgen_rng::Mulberry32;

use crate::character::Class;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;

use crate::combat::feedback::{Corpse, Flinches, Looted, Struck, collapse_share, knockback_for, laid_back};
use crate::combat::swing::SwungAt;
use crate::combat::{ArmourClass, Health, Weapon, first_level_hit_points_explained_for, swing_time};
use crate::creatures::Critter;
use crate::creatures::behaviour::{AttackOnDelver, BlowsAtDelver, DelverToStrike, MissedDelver, body_radius};
use crate::figure::Figure;
use crate::folk::{Denizen, Person, Stance, Stride};
use crate::folk_pose::{ArmSwing, spawn_rigged};
use crate::folk_routine::{Routine, pace};
use crate::hostility::{Hostility, HostilityPlugin, Provoked};
use crate::inventory::underfoot::{FloorChanges, apply_floor_changes, set_down_around};
use crate::inventory::{Inventory, Toasts};
use crate::level::geometry::Relief;
use crate::play::{BlockedCells, BlocksTheWay, CurrentLevel, DelveSystems, Obstacle, Obstacles, Player};
use crate::player::{blocked_by, slide_around};
use crate::render::ItemSpawner;
use crate::settings::GameSettings;
use crate::tuning::Tuning;

pub struct FolkCombatPlugin;

impl Plugin for FolkCombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(HostilityPlugin)
            .add_message::<Struck>()
            .add_message::<SwungAt>()
            .add_message::<AttackOnDelver>()
            .add_message::<MissedDelver>()
            .init_resource::<Tuning>()
            .init_resource::<Obstacles>()
            .init_resource::<BlockedCells>()
            .init_resource::<FloorChanges>()
            .add_systems(
                Update,
                (arm_folk, provoke_when_attacked, take_folk_blows, shed_folk_gear.before(apply_floor_changes), collapse_folk, fight)
                    .chain()
                    .before(crate::hostility::turn_on_the_delver)
                    .in_set(DelveSystems),
            );
        // `FB_PROVOKE=<n>` and `FB_SLAY=<n>`, for captures (README, "Debug switches").
        let index = |name: &str| std::env::var(name).ok().and_then(|v| v.trim().parse::<usize>().ok());
        if let Some(folk) = index("FB_PROVOKE") {
            app.add_systems(Update, debug_provoke.run_if(resource_exists::<CurrentLevel>).in_set(DelveSystems));
            app.insert_resource(DebugFolk { provoke: Some(folk), slay: index("FB_SLAY"), done: [false; 2] });
        } else if let Some(folk) = index("FB_SLAY") {
            app.add_systems(Update, debug_provoke.run_if(resource_exists::<CurrentLevel>).in_set(DelveSystems));
            app.insert_resource(DebugFolk { provoke: None, slay: Some(folk), done: [false; 2] });
        }
    }
}

/// The folk `FB_PROVOKE` provokes and `FB_SLAY` strikes down, by their index in the level's
/// folk (as `print_level` lists them), and whether each has been done.
#[derive(Resource, Clone, Copy, Debug)]
struct DebugFolk {
    provoke: Option<usize>,
    slay: Option<usize>,
    done: [bool; 2],
}

/// When `FB_PROVOKE` provokes its folk and `FB_SLAY` strikes theirs down, seconds into the delve.
const PROVOKE_AT: f32 = 1.0;
const SLAY_AT: f32 = 2.0;

/// Provokes or strikes down the folk the debug switches name, once each, a moment in.
fn debug_provoke(
    time: Res<Time>,
    level: Res<CurrentLevel>,
    mut debug: ResMut<DebugFolk>,
    mut since: Local<f32>,
    folk: Query<(Entity, &Denizen, &Transform), Without<Corpse>>,
    mut provoked: MessageWriter<Provoked>,
    mut struck: MessageWriter<Struck>,
) {
    *since += time.delta_secs();
    let find = |index: usize| level.0.folk.get(index).and_then(|spot| folk.iter().find(|(_, d, _)| d.person.spot == *spot));
    if let (Some(index), false) = (debug.provoke, debug.done[0])
        && *since >= PROVOKE_AT
        && let Some((entity, _, _)) = find(index)
    {
        provoked.write(Provoked { folk: entity });
        debug.done[0] = true;
    }
    if let (Some(index), false) = (debug.slay, debug.done[1])
        && *since >= SLAY_AT
        && let Some((entity, _, at)) = find(index)
    {
        struck.write(Struck { target: entity, damage: 1000, from: at.translation.xz() + (at.rotation * Vec3::Z).xz() * 10.0, bludgeoning: false });
        debug.done[1] = true;
    }
}

/// What a folk fights with: their main hand's [`Weapon`], their Strength, and how long a swing
/// takes, worked out once when they appear.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Combatant {
    pub weapon: Weapon,
    pub strength: i32,
    /// Seconds from wind-up to follow-through.
    pub swing_time: f32,
}

impl Combatant {
    /// `person` armed with what their main hand holds, at their folk's height.
    pub fn of(person: &Person) -> Combatant {
        let held = person.item(Slot::Mainhand);
        let weapon = Weapon::wielded(held, person.spot.folk.race().h);
        Combatant { weapon, strength: FolkStats::of(person).strength, swing_time: swing_time(held.map(|i| i.shape.family())) }
    }
}

/// A folk's part in a fight: the swing under way, the wait before the next, and the way to the
/// delver.
#[derive(Component, Clone, Debug)]
pub struct Fight {
    /// Seconds into the swing under way.
    pub swing: Option<f32>,
    /// Seconds until they may swing again.
    pub cooldown: f32,
    /// Whether this swing's blow has come down.
    landed: bool,
    /// Seconds since they last saw the delver.
    pub unseen: f32,
    path: Vec<(i32, i32)>,
    goal: (i32, i32),
    repath: f32,
    rng: Mulberry32,
}

impl Fight {
    fn new(seed: u32) -> Fight {
        Fight { swing: None, cooldown: 0.0, landed: false, unseen: 0.0, path: Vec::new(), goal: (i32::MIN, i32::MIN), repath: 0.0, rng: Mulberry32::new(seed ^ FIGHT_SALT) }
    }
}

impl Fight {
    /// Drops the fight under way: no swing, no way to the delver, no time unseen.
    fn calm(&mut self) {
        *self = Fight { rng: self.rng.clone(), ..Fight::new(0) };
    }
}

/// Salt for a folk's fighting stream. Our own.
const FIGHT_SALT: u32 = 0x0F16_4700;
/// How far through a swing the blow lands, as a skeleton's does (`creatures::skeletons::LANDS`).
pub const LANDS: f32 = crate::creatures::skeletons::LANDS;
/// The wait between a folk's swings, seconds, as a skeleton's. Our own.
const COOLDOWN: f32 = 0.8;
/// How much farther than their reach a blow still lands, as a skeleton's. Our own.
const SLACK: f32 = 1.15;
/// A hostile folk runs at this many times their walking pace: the SRD's Dash (p. 93).
const DASH: f32 = 2.0;
/// How quickly a hostile folk turns to face the delver, radians a second. Our own.
const TURN_RATE: f32 = 6.0;
/// How often a folk finds its way again while following, seconds, as a creature does.
const REPATH: f32 = 0.5;

/// A commoner's hit die and its average (SRD 5.1 "Commoner": Hit Points 4 (1d8)).
const COMMONER_DIE: u32 = 8;
const COMMONER_HIT_POINTS: f64 = 4.0;

/// What a folk is in a fight, from who they are: a pure function of their spot.
#[derive(Clone, Debug, PartialEq)]
pub struct FolkStats {
    /// Their outfit's class, or `None` for a look that is none (scavenger, freshly fallen).
    pub class: Option<Class>,
    pub strength: i32,
    pub dexterity: i32,
    pub constitution: i32,
    pub hit_points: Explained,
    pub armour_class: Explained,
}

impl FolkStats {
    /// `person`'s stats, as the module says.
    pub fn of(person: &Person) -> FolkStats {
        let [strength, dexterity, constitution, _, _] = person.spot.folk.race().stats;
        let class = Class::of_outfit(person.outfit);
        let hit_points = match class {
            Some(class) => first_level_hit_points_explained_for(class, constitution),
            None => {
                let e = Explained::new("Hit points", "").base(format!("d{COMMONER_DIE} average"), COMMONER_HIT_POINTS).add("CON", modifier(constitution) as f64);
                if e.value() < 1.0 { e.clamp("at least 1", 1.0, f64::INFINITY) } else { e }
            }
        };
        let armour = person.gear.iter().map(|(_, item)| (item.name.clone(), item.profile.stats.armour)).filter(|(_, a)| *a != 0.0);
        FolkStats { class, strength, dexterity, constitution, hit_points, armour_class: armour_class(dexterity, armour) }
    }

    pub fn max_hit_points(&self) -> i32 {
        self.hit_points.value().round() as i32
    }

    pub fn armour_class_value(&self) -> i32 {
        self.armour_class.value().round() as i32
    }
}

/// Gives each folk their hit points, Armor Class ([`FolkStats`]) and arms ([`Combatant`]) once
/// they stand in the level; one brought back from the record keeps the hit points it was left
/// with.
pub fn arm_folk(mut commands: Commands, new: Query<(Entity, &Denizen, Has<Health>), Without<ArmourClass>>) {
    for (entity, denizen, has_health) in &new {
        let stats = FolkStats::of(&denizen.person);
        let mut entity = commands.entity(entity);
        entity.insert((ArmourClass(stats.armour_class_value()), Combatant::of(&denizen.person), Fight::new(denizen.person.spot.seed)));
        if !has_health {
            entity.insert(Health::full(stats.max_hit_points()));
        }
    }
}

/// A swing aimed at a living folk, whether or not it lands, or a blow that lands on one, gives
/// them cause ([`Provoked`]).
pub fn provoke_when_attacked(mut swung: MessageReader<SwungAt>, mut struck: MessageReader<Struck>, folk: Query<(), (With<Denizen>, Without<Corpse>)>, mut provoked: MessageWriter<Provoked>) {
    let targets: Vec<Entity> = swung.read().map(|s| s.target).chain(struck.read().map(|s| s.target)).collect();
    for target in targets {
        if folk.contains(target) {
            provoked.write(Provoked { folk: target });
        }
    }
}

/// Applies each blow that lands on a folk: hit points down, a flash and a push, a thud; at 0 a
/// body, which stops its routine and blocks nobody.
#[allow(clippy::too_many_arguments)]
pub fn take_folk_blows(
    mut commands: Commands,
    mut struck: MessageReader<Struck>,
    tuning: Res<Tuning>,
    settings: Option<Res<GameSettings>>,
    sounds: Option<Res<crate::combat::feedback::CreatureSounds>>,
    mut toasts: Toasts,
    flinches: Flinches,
    mut folk: Query<(&Denizen, &Transform, &mut Health), Without<Corpse>>,
) {
    let game = settings.as_deref().copied().unwrap_or_default();
    for blow in struck.read() {
        let Ok((denizen, at, mut health)) = folk.get_mut(blow.target) else { continue };
        let fell = health.hurt(blow.damage);
        let away = (at.translation.xz() - blow.from).normalize_or_zero();
        let push = away * knockback_for(fb_creatures::Size::Medium) * tuning.combat.knockback_scale;
        commands.entity(blow.target).insert(flinches.of(blow.target, push, &tuning.combat));
        if let Some(sounds) = &sounds {
            crate::audio::play_at(&mut commands, "folk struck", sounds.thud.clone(), at.translation, &game, &tuning);
        }
        info!("{} takes {}: {} of {} hit points", denizen.person.name, blow.damage, health.hp, health.max);
        if fell {
            commands.entity(blow.target).insert(Corpse { since: 0.0, fell: *at }).remove::<(BlocksTheWay, Routine)>();
            toasts.add(&format!("{} is slain", denizen.person.name));
            info!("{} falls", denizen.person.name);
        }
    }
}

/// `person` with nothing worn or held: a body whose gear has been set down.
pub fn bare(person: &Person) -> Person {
    Person { gear: Vec::new(), ..person.clone() }
}

/// Draws the figure under `stance` again as `figure`: its skeleton and meshes, and any light it
/// held, replaced. On the spawner's own commands, so they apply in order.
pub fn redraw(spawner: &mut ItemSpawner, bindposes: &mut Assets<SkinnedMeshInverseBindposes>, stance: Entity, figure: &Figure, seed: u32) {
    spawner.commands().entity(stance).despawn_children();
    let anim = spawn_rigged(spawner, bindposes, figure, stance, false, seed);
    spawner.commands().entity(stance).insert(anim);
}

/// Sets everything a fallen folk wore and held down round where they fell, once, as they start
/// to fall ([`set_down_around`]), and draws their body again bare. They are marked [`Looted`]
/// and their gear taken off them, so it is never set down twice; the floor changes are applied
/// this frame, before the record could read the floor (as a skeleton's are).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn shed_folk_gear(
    tuning: Res<Tuning>,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Res<Obstacles>,
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    mut bodies: Query<(Entity, &mut Denizen, &Transform, Option<&Children>), (With<Corpse>, Without<Looted>)>,
    stances: Query<(), With<Stance>>,
    bindposes: Option<ResMut<Assets<SkinnedMeshInverseBindposes>>>,
    mut spawner: ItemSpawner,
) {
    let blocked = blocked_by(&level.0, &blocked_cells);
    let mut bindposes = bindposes;
    for (entity, mut denizen, at, children) in &mut bodies {
        spawner.commands().entity(entity).insert(Looted);
        let items = denizen.person.gear.iter().map(|(_, item)| item);
        // Not round the circle they stood in: they no longer block it.
        let own = |o: &&Obstacle| matches!(o, Obstacle::Circle { centre, radius } if centre.distance(at.translation.xz()) < 0.5 * denizen.radius && *radius == denizen.radius);
        let others: Vec<Obstacle> = obstacles.0.iter().filter(|o| !own(o)).copied().collect();
        set_down_around(at.translation.xz(), items, denizen.person.spot.seed, &mut inventory, &mut changes, &blocked, &others, tuning.view.floor_item_scale);
        info!("{} drops {} things", denizen.person.name, denizen.person.gear.len());
        denizen.person = bare(&denizen.person);
        // Drawn again with nothing on.
        let stance = children.into_iter().flat_map(|c| c.iter()).find(|&c| stances.contains(c));
        if let (Some(stance), Some(bindposes)) = (stance, bindposes.as_deref_mut()) {
            let figure = Figure::dress(&denizen.person.spec, &denizen.person.gear, denizen.person.dirt);
            redraw(&mut spawner, bindposes, stance, &figure, denizen.person.spot.seed);
        }
    }
}

/// Lays each fallen folk on their back once the flash's hold is over ([`collapse_share`]).
pub fn collapse_folk(time: Res<Time>, tuning: Res<Tuning>, mut bodies: Query<(&Denizen, &mut Corpse, &mut Transform)>) {
    let hold = tuning.combat.flash_hold;
    for (denizen, mut corpse, mut transform) in &mut bodies {
        corpse.since += time.delta_secs();
        if corpse.since <= hold {
            continue;
        }
        let laid = laid_back(denizen.max.y, &corpse.fell, collapse_share(corpse.since, hold));
        if *transform != laid {
            *transform = laid;
        }
    }
}

fn grid(p: Vec2) -> (f32, f32) {
    (p.x / CELL, p.y / CELL)
}

fn cell(p: Vec2) -> (i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

/// The signed turn from yaw `from` to yaw `to`, radians, the short way round.
fn angle_between(from: f32, to: f32) -> f32 {
    (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// What [`fight`] moves: each living folk who might fight.
type Fighters = (
    Entity,
    &'static Denizen,
    &'static mut Hostility,
    &'static Combatant,
    &'static mut Fight,
    &'static mut Stride,
    &'static mut ArmSwing,
    &'static mut Transform,
    Option<&'static mut Routine>,
);

/// A living creature, not a folk.
type Standing = (Without<Denizen>, Without<Corpse>);

/// One frame of every hostile folk's fight: close on the delver round what is in the way, turn
/// to them, and swing in reach, the blow landing as the arm comes down ([`LANDS`]).
#[allow(clippy::too_many_arguments)]
pub fn fight(
    time: Res<Time>,
    level: Res<CurrentLevel>,
    tuning: Res<Tuning>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Res<Obstacles>,
    player: Query<DelverToStrike, (With<Player>, Without<Denizen>)>,
    creatures: Query<(&Critter, &Transform), Standing>,
    mut folk: Query<Fighters, Without<Corpse>>,
    mut attacks: BlowsAtDelver,
) {
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }
    let Some((player, delver_ac)) = player.iter().next().map(|(t, ac)| (t.translation.xz(), ac.copied())) else { return };
    let level = &level.0;
    let relief = Relief::new(level);
    let blocked = blocked_by(level, &blocked_cells);
    let player_radius = tuning.moving.body_radius;
    // Everything that moves or stands but the folk themselves, whose circles are in the
    // obstacles already (each fighter sets its own aside).
    let mut around: Vec<Obstacle> = obstacles.0.clone();
    around.extend(creatures.iter().map(|(c, t)| Obstacle::Circle { centre: t.translation.xz(), radius: body_radius(c) }));
    around.push(Obstacle::Circle { centre: player, radius: player_radius });
    for (entity, denizen, mut hostility, combatant, mut fight, mut stride, mut arm, mut transform, routine) in &mut folk {
        if !hostility.is_hostile() {
            continue;
        }
        let fight = &mut *fight;
        let at = transform.translation.xz();
        let radius = denizen.radius;
        let reach = player_radius + combatant.weapon.reach;
        let sees = line_of_sight(grid(at), grid(player), &blocked);
        // Out of sight long enough, they give up and go home, friendly again.
        fight.unseen = if sees { 0.0 } else { fight.unseen + dt };
        if fight.unseen > tuning.folk.lose_interest_after {
            *hostility = Hostility::Friendly;
            fight.calm();
            stride.speed = 0.0;
            arm.0 = None;
            if let Some(mut routine) = routine {
                routine.head_home();
            }
            info!("{} loses interest in the delver", denizen.person.name);
            continue;
        }
        let in_reach = sees && player.distance(at) <= reach;
        fight.cooldown = (fight.cooldown - dt).max(0.0);
        let mut moved = 0.0;

        // The swing under way: the blow comes down at `LANDS`, if the delver is still there.
        if let Some(since) = fight.swing.as_mut() {
            *since += dt;
            let since = *since;
            if !fight.landed && since >= LANDS * combatant.swing_time {
                fight.landed = true;
                if sees && player.distance(at) <= reach * SLACK {
                    let damage = combatant.weapon.roll(modifier(combatant.strength), &mut fight.rng);
                    let blow = AttackOnDelver { attacker: entity, species: None, damage, poison: 0, attacker_pos: at };
                    let to_hit = combatant.weapon.to_hit(combatant.strength);
                    if attacks.strike(&denizen.person.name, blow, to_hit, delver_ac.as_ref(), &mut fight.rng) {
                        info!("{} strikes the delver for {damage}", denizen.person.name);
                    }
                }
            }
            if since >= combatant.swing_time {
                fight.swing = None;
                fight.cooldown = COOLDOWN;
            }
        } else if in_reach && fight.cooldown <= 0.0 {
            fight.swing = Some(0.0);
            fight.landed = false;
        } else if !in_reach {
            // Closing in: straight in sight, else along the way round the rock.
            let target = if sees {
                fight.path.clear();
                player
            } else {
                fight.repath -= dt;
                if fight.path.is_empty() || fight.goal != cell(player) || fight.repath <= 0.0 {
                    fight.path = find_path(cell(at), cell(player), 4000, &blocked).unwrap_or_default();
                    fight.goal = cell(player);
                    fight.repath = REPATH;
                }
                let (x, y) = steer_toward(grid(at), grid(player), &fight.path, radius / CELL, &blocked);
                Vec2::new(x, y) * CELL
            };
            let direction = (target - at).normalize_or_zero();
            if direction != Vec2::ZERO {
                let own = |o: &&Obstacle| matches!(o, Obstacle::Circle { centre, radius: r } if centre.distance(at) < 0.5 * radius && *r == radius);
                let others: Vec<Obstacle> = around.iter().filter(|o| !own(o)).copied().collect();
                let speed = pace(denizen, tuning.folk.walk_speed) * DASH;
                let step = (direction * speed * dt).clamp_length_max((target - at).length());
                let to = slide_around(at, at + step, radius, &blocked, &others);
                moved = (to - at).length();
                transform.translation = Vec3::new(to.x, relief.floor_height(to.x, to.y), to.y);
            }
        }
        stride.speed = moved / dt;
        arm.0 = fight.swing.map(|since| (since / combatant.swing_time).min(1.0));

        // Facing the delver throughout. Folk face +Z.
        let to = player - transform.translation.xz();
        if to.length_squared() > 1e-4 {
            let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
            let turn = angle_between(yaw, to.x.atan2(to.y));
            transform.rotation = Quat::from_rotation_y(yaw + turn.clamp(-TURN_RATE * dt, TURN_RATE * dt));
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::combat::{CombatPlugin, RunDice, Weapon, swing::Swing};
    use crate::inventory::Inventory;
    use crate::play::{CurrentLevel, Player};
    use bevy::time::TimeUpdateStrategy;
    use procgen_rng::Mulberry32;

    /// An app with the whole of combat and the folk's fighting but no drawing, on seed 1207's
    /// level, the delver holding a sword. Stepped 1/60 s a frame.
    fn combat_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin, bevy::state::app::StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_state(crate::launch::AppState::Delving)
            .init_resource::<crate::keymap::Keymap>()
            .init_resource::<crate::character::Character>()
            .init_resource::<crate::play::ScreenOpen>()
            .init_resource::<crate::play::Obstacles>()
            .init_resource::<crate::play::BlockedCells>()
            .init_resource::<crate::inventory::Toast>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
            .init_resource::<crate::render::OutlineMaterials>()
            .init_resource::<Assets<SkinnedMeshInverseBindposes>>()
            .add_plugins((CombatPlugin, FolkCombatPlugin, crate::folk_routine::FolkRoutinePlugin))
            .add_systems(Update, apply_floor_changes)
            .add_systems(PostUpdate, crate::play::gather_obstacles.after(TransformSystems::Propagate))
            .insert_resource(RunDice(Mulberry32::new(9)));
        let mut inventory = Inventory::default();
        let sword = inventory.add(fb_items::roll_item("Sword", 7).unwrap());
        inventory.wear(sword, Slot::Mainhand).unwrap();
        app.insert_resource(inventory);
        app.insert_resource(CurrentLevel(fb_level::generate(1207, 1)));
        app
    }

    /// A stand-in (no figure) for the folk at `spot`, standing there.
    fn stand_in(app: &mut App, spot: &fb_level::FolkSpot) -> Entity {
        let denizen = Denizen { person: Person::of(spot, 1), radius: 3.0, min: Vec3::new(-2.0, 0.0, -2.0), max: Vec3::new(2.0, 17.0, 2.0) };
        app.world_mut().spawn((denizen, BlocksTheWay(fb_level::FloorShape::Round(3.0)), crate::folk::stand_at(spot))).id()
    }

    /// The delver `away` world units west of `at`, facing it.
    fn delver_near(app: &mut App, at: Vec3, away: f32) -> Entity {
        let from = at - Vec3::X * away;
        app.world_mut().spawn((Player, Transform::from_translation(from).looking_at(at, Vec3::Y))).id()
    }

    /// Seed 1207's level with its first folk alone, standing at their spot as a stand-in, and
    /// the delver holding a sword `away` world units west of them, facing them.
    pub(crate) fn folk_fight(away: f32) -> (App, Entity, Entity) {
        let mut app = combat_app();
        let spot = app.world().resource::<CurrentLevel>().0.folk[0];
        let folk = stand_in(&mut app, &spot);
        let at = app.world().get::<Transform>(folk).unwrap().translation;
        let player = delver_near(&mut app, at, away);
        app.update();
        (app, player, folk)
    }

    /// Every folk of seed 1207 as stand-ins going about their routines, and the delver `away`
    /// world units west of the first, facing them.
    fn folk_level(away: f32) -> (App, Entity, Vec<Entity>) {
        let mut app = combat_app();
        let spots = app.world().resource::<CurrentLevel>().0.folk.clone();
        let folk: Vec<Entity> = spots.iter().map(|spot| stand_in(&mut app, spot)).collect();
        let at = app.world().get::<Transform>(folk[0]).unwrap().translation;
        let player = delver_near(&mut app, at, away);
        app.update();
        (app, player, folk)
    }

    fn attacks(app: &App) -> Vec<AttackOnDelver> {
        app.world().resource::<Messages<AttackOnDelver>>().iter_current_update_messages().copied().collect()
    }

    fn hostility(app: &App, folk: Entity) -> Hostility {
        *app.world().get::<Hostility>(folk).unwrap()
    }

    /// Swings the delver's weapon once, for a swing's time.
    pub(crate) fn swing_once(app: &mut App, player: Entity) {
        let weapon = Weapon::in_hand(app.world().resource::<Inventory>(), app.world().resource::<crate::character::Character>());
        app.world_mut().entity_mut(player).insert(Swing::new(weapon));
        for _ in 0..40 {
            app.update();
        }
    }

    #[test]
    fn a_swing_in_reach_hits_a_folk_and_lowers_its_hit_points() {
        let (mut app, player, folk) = folk_fight(9.0);
        let full = app.world().get::<Health>(folk).copied().expect("folk have hit points");
        assert_eq!(full.hp, FolkStats::of(&app.world().get::<Denizen>(folk).unwrap().person).max_hit_points());
        assert!(app.world().get::<ArmourClass>(folk).is_some());
        // Against an Armor Class anything but a natural 1 beats, a few swings must land.
        app.world_mut().entity_mut(folk).insert((ArmourClass(-10), Health::full(500)));
        for _ in 0..3 {
            swing_once(&mut app, player);
        }
        let now = app.world().get::<Health>(folk).unwrap().hp;
        assert!(now < 500, "no blow landed");
        // Out of reach, nothing lands.
        let (mut app, player, folk) = folk_fight(40.0);
        app.world_mut().entity_mut(folk).insert((ArmourClass(-10), Health::full(500)));
        swing_once(&mut app, player);
        assert_eq!(app.world().get::<Health>(folk).unwrap().hp, 500);
    }

    /// How many of `swings` swings land on a folk of Armor Class `ac`.
    fn landed(ac: i32, swings: usize) -> usize {
        landed_by(ac, swings, false)
    }

    /// [`landed`], the delver poisoned or not.
    fn landed_by(ac: i32, swings: usize, poisoned: bool) -> usize {
        let (mut app, player, folk) = folk_fight(9.0);
        if poisoned {
            app.world_mut().entity_mut(player).insert(crate::character::DelverPoison(fb_character::poison::Poisoned::for_hours(10.0)));
        }
        app.world_mut().entity_mut(folk).insert((ArmourClass(ac), Health::full(100_000)));
        // Struck, it fights back: the delver outlasts it.
        app.world_mut().entity_mut(player).insert(Health::full(100_000));
        let stood = *app.world().get::<Transform>(folk).unwrap();
        let mut hits = 0;
        for _ in 0..swings {
            // Back where it stood, however far the last blow knocked it.
            app.world_mut().entity_mut(folk).insert(stood);
            let before = app.world().get::<Health>(folk).unwrap().hp;
            swing_once(&mut app, player);
            if app.world().get::<Health>(folk).unwrap().hp < before {
                hits += 1;
            }
        }
        hits
    }

    #[test]
    fn armour_lowers_the_chance_to_hit() {
        let (light, heavy) = (landed(8, 60), landed(19, 60));
        assert!(light > heavy + 15, "AC 8: {light} of 60, AC 19: {heavy} of 60");
        assert!(heavy > 0, "a natural 20 still hits");
    }

    #[test]
    fn a_poisoned_delver_hits_less_often() {
        let (even, poisoned) = (landed_by(12, 60, false), landed_by(12, 60, true));
        assert!(poisoned + 8 < even, "AC 12: {even} of 60 well, {poisoned} of 60 poisoned");
    }

    #[test]
    fn a_folks_blows_roll_against_the_delvers_armour_class_and_a_miss_does_nothing() {
        let (mut app, player, folk) = folk_fight(30.0);
        app.world_mut().entity_mut(player).insert(Health::full(100_000));
        app.world_mut().write_message(Provoked { folk });
        let (mut hits, mut misses) = (Vec::new(), 0);
        for _ in 0..1800 {
            app.update();
            hits.extend(attacks(&app));
            misses += app.world().resource::<Messages<MissedDelver>>().iter_current_update_messages().count();
        }
        let sheet = app.world().resource::<crate::character::Character>().armour_class(app.world().resource::<Inventory>()).value().round() as i32;
        assert_eq!(app.world().get::<ArmourClass>(player), Some(&ArmourClass(sheet)), "the delver carries the sheet's Armor Class");
        assert!(!hits.is_empty() && misses > 0, "{} hits, {misses} misses", hits.len());
        let taken: u32 = hits.iter().map(|a| a.damage).sum();
        let delver = app.world().get::<Health>(player).unwrap();
        assert_eq!((delver.max - delver.hp) as u32, taken, "only the hits hurt");
    }

    #[test]
    fn at_0_hit_points_a_folk_dies_and_falls_where_it_stood() {
        let (mut app, _, folk) = folk_fight(9.0);
        let stood = *app.world().get::<Transform>(folk).unwrap();
        app.world_mut().write_message(Struck { target: folk, damage: 1000, from: stood.translation.xz() - Vec2::X * 9.0, bludgeoning: false });
        for _ in 0..90 {
            app.update();
        }
        assert!(app.world().get::<Health>(folk).unwrap().is_down());
        assert!(app.world().get::<Corpse>(folk).is_some(), "a body");
        assert!(app.world().get::<BlocksTheWay>(folk).is_none(), "a body blocks nobody");
        let down = app.world().get::<Transform>(folk).unwrap();
        assert!((down.rotation * Vec3::Y).y.abs() < 0.2, "lying down");
        assert!(down.translation.xz().distance(stood.translation.xz()) < 4.0, "where it stood");
        assert!(app.world().resource::<crate::inventory::Toast>().text.ends_with("is slain"));
    }

    fn people() -> Vec<Person> {
        fb_level::generate(1207, 1).folk.iter().map(|spot| Person::of(spot, 1)).collect()
    }

    #[test]
    fn folk_hit_points_follow_their_class_and_constitution() {
        for person in people() {
            let stats = FolkStats::of(&person);
            let con = modifier(stats.constitution);
            let expected = match stats.class {
                Some(class) => (class.hit_die() as i32 + con).max(1),
                None => (4 + con).max(1),
            };
            assert_eq!(stats.max_hit_points(), expected, "{} ({})", person.name, person.outfit);
            assert_eq!(FolkStats::of(&person), stats, "a pure function of the spot");
        }
    }

    #[test]
    fn worn_armour_raises_a_folks_armour_class() {
        let person = people().remove(0);
        let bare = FolkStats::of(&Person { gear: Vec::new(), ..person.clone() });
        assert_eq!(bare.armour_class_value(), 10 + modifier(bare.dexterity));
        let mail = fb_items::roll_item("Mail", 7).unwrap();
        let shield = fb_items::roll_item("Shield", 7).unwrap();
        let armour = (mail.profile.stats.armour + shield.profile.stats.armour).round() as i32;
        let kitted = FolkStats::of(&Person { gear: vec![(Slot::Body, mail), (Slot::Offhand, shield)], ..person });
        assert!(armour > 0);
        assert_eq!(kitted.armour_class_value(), bare.armour_class_value() + armour);
    }

    #[test]
    fn folk_that_are_not_attacked_never_attack() {
        let (mut app, player, folk) = folk_level(12.0);
        // The delver walks right up to the first, and round them among their kin, for half a
        // minute: nobody is angered and nobody strikes.
        let start = app.world().get::<Transform>(player).unwrap().translation;
        for frame in 0..1800 {
            let angle = frame as f32 / 300.0 * std::f32::consts::TAU;
            app.world_mut().get_mut::<Transform>(player).unwrap().translation = start + Vec3::new(angle.cos() * 4.0, 0.0, angle.sin() * 4.0);
            app.update();
            assert!(attacks(&app).is_empty(), "a friendly folk struck at frame {frame}");
        }
        assert!(folk.iter().all(|&f| hostility(&app, f) == Hostility::Friendly));
        let hp = app.world().get::<Health>(player).unwrap();
        assert_eq!(hp.hp, hp.max);
    }

    #[test]
    fn striking_a_folk_turns_it_and_the_kin_who_see_it_but_no_other_faction() {
        let (mut app, _, folk) = folk_level(12.0);
        let first = folk[0];
        let faction = app.world().get::<Denizen>(first).unwrap().person.spot.faction;
        // Who should turn, as they stand now.
        let kin: Vec<crate::hostility::Kin> = folk
            .iter()
            .map(|&f| crate::hostility::Kin { entity: f, faction: app.world().get::<Denizen>(f).unwrap().person.spot.faction, at: app.world().get::<Transform>(f).unwrap().translation.xz() })
            .collect();
        let level = app.world().resource::<CurrentLevel>().0.clone();
        let expected = crate::hostility::who_turns(first, &kin, &level, |x, y| !level.is_open(x, y));
        assert!(expected.len() > 1, "someone of the faction sees it");
        let at = app.world().get::<Transform>(first).unwrap().translation.xz();
        app.world_mut().write_message(Struck { target: first, damage: 1, from: at - Vec2::X * 9.0, bludgeoning: false });
        for _ in 0..3 {
            app.update();
        }
        for &f in &folk {
            let denizen = app.world().get::<Denizen>(f).unwrap();
            let hostile = hostility(&app, f).is_hostile();
            assert_eq!(hostile, expected.contains(&f), "{} of {}", denizen.person.name, denizen.person.spot.faction.name());
            if denizen.person.spot.faction != faction {
                assert!(!hostile, "{} of another faction turned", denizen.person.name);
            }
        }
        assert!(folk.iter().any(|&f| app.world().get::<Denizen>(f).unwrap().person.spot.faction != faction), "a level of one faction tests nothing");
        let toast = &app.world().resource::<crate::inventory::Toast>().text;
        assert_eq!(*toast, crate::hostility::turn_line(faction));
    }

    #[test]
    fn a_missed_swing_turns_a_folk_too() {
        let (mut app, player, folk) = folk_fight(9.0);
        app.world_mut().entity_mut(folk).insert(ArmourClass(1000));
        let before = app.world().get::<Health>(folk).unwrap().hp;
        swing_once(&mut app, player);
        assert_eq!(app.world().get::<Health>(folk).unwrap().hp, before, "it missed");
        assert!(hostility(&app, folk).is_hostile());
    }

    #[test]
    fn a_hostile_folk_closes_in_and_deals_damage_and_stays_hostile() {
        let (mut app, player, folk) = folk_fight(30.0);
        app.world_mut().write_message(Provoked { folk });
        let mut struck = Vec::new();
        for _ in 0..600 {
            app.update();
            struck.extend(attacks(&app));
        }
        assert!(!struck.is_empty(), "the hostile folk never struck");
        assert!(struck.iter().all(|a| a.attacker == folk && a.species.is_none() && a.damage >= 1));
        let delver = app.world().get::<Health>(player).unwrap();
        assert!(delver.hp < delver.max, "the delver took no damage");
        let me = app.world().get::<Transform>(player).unwrap().translation.xz();
        let combatant = *app.world().get::<Combatant>(folk).unwrap();
        assert!(struck[0].attacker_pos.distance(me) <= (combatant.weapon.reach + 3.5) * SLACK + 0.5, "struck from in reach");
        // Blows come on a swing's time and a cooldown, not every frame.
        assert!(struck.len() as f32 <= 10.0 / (combatant.swing_time + COOLDOWN) + 1.0, "{} blows in 10 s", struck.len());
        assert!(hostility(&app, folk).is_hostile(), "still hostile");
    }

    /// A cell of the level far from `from` (world `(x, z)`) with no line of sight to it.
    fn out_of_sight_of(app: &App, from: Vec2) -> Vec3 {
        let level = &app.world().resource::<CurrentLevel>().0;
        let blocked = |x: i32, y: i32| !level.is_open(x, y);
        let (w, h) = (level.width as i32, level.height as i32);
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| level.is_open(x, y))
            .map(|(x, y)| Vec2::new(x as f32 + 0.5, y as f32 + 0.5) * CELL)
            .filter(|p| p.distance(from) > 20.0 * CELL && !line_of_sight(grid(*p), grid(from), blocked))
            .map(|p| Vec3::new(p.x, 0.0, p.y))
            .next()
            .expect("somewhere out of sight")
    }

    #[test]
    fn a_hostile_folk_who_loses_sight_of_the_delver_long_enough_goes_home_friendly() {
        let (mut app, player, folk) = folk_fight(12.0);
        app.world_mut().resource_mut::<Tuning>().folk.lose_interest_after = 2.0;
        app.world_mut().entity_mut(player).insert(Health::full(100_000));
        app.world_mut().write_message(Provoked { folk });
        for _ in 0..30 {
            app.update();
        }
        assert!(hostility(&app, folk).is_hostile());
        let home = app.world().get::<Routine>(folk).expect("a routine").home;
        // The delver slips away out of sight.
        let at = app.world().get::<Transform>(folk).unwrap().translation.xz();
        let away = out_of_sight_of(&app, at);
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = away;
        let mut calmed = None;
        for frame in 0..240 {
            app.update();
            if calmed.is_none() && !hostility(&app, folk).is_hostile() {
                calmed = Some(frame);
            }
        }
        let calmed = calmed.expect("the folk never gave up");
        assert!(calmed as f32 / 60.0 >= 2.0, "gave up after {} s, sooner than the tuning", calmed as f32 / 60.0);
        assert_eq!(hostility(&app, folk), Hostility::Friendly);
        // Back to their routine: on their way home, or there.
        let routine = app.world().get::<Routine>(folk).unwrap();
        assert!(matches!(routine.phase, crate::folk_routine::Phase::Walk { goal } if goal == home) || app.world().get::<Transform>(folk).unwrap().translation.xz().distance(home) < 2.0 * CELL, "{:?}", routine.phase);
        // Struck again, they turn again.
        let at = app.world().get::<Transform>(folk).unwrap().translation.xz();
        app.world_mut().write_message(Struck { target: folk, damage: 1, from: at - Vec2::X * 9.0, bludgeoning: false });
        for _ in 0..3 {
            app.update();
        }
        assert!(hostility(&app, folk).is_hostile(), "provoked again");
    }

    #[test]
    fn a_hostile_folk_who_keeps_the_delver_in_sight_keeps_fighting() {
        let (mut app, player, folk) = folk_fight(12.0);
        app.world_mut().resource_mut::<Tuning>().folk.lose_interest_after = 2.0;
        app.world_mut().entity_mut(player).insert(Health::full(100_000));
        app.world_mut().write_message(Provoked { folk });
        for frame in 0..600 {
            app.update();
            assert!(hostility(&app, folk).is_hostile(), "gave up at frame {frame} with the delver in sight");
        }
    }

    #[test]
    fn the_record_keeps_a_hostile_folk_hostile() {
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, _, folk) = folk_fight(30.0);
        app.world_mut().write_message(Provoked { folk });
        app.update();
        app.init_resource::<crate::record::WorldRecord>();
        app.world_mut().run_system_once(crate::record::record_level_left).unwrap();
        let record = app.world().resource::<crate::record::WorldRecord>();
        let left = &record.level(1).unwrap().folk[&0];
        assert!(left.hostile);
        assert_eq!(left.hp, app.world().get::<Health>(folk).unwrap().hp);
    }

    fn on_the_floor(app: &mut App) -> Vec<crate::carry::FloorItem> {
        app.world_mut().query::<&crate::carry::FloorItem>().iter(app.world()).map(|f| crate::carry::FloorItem { id: f.id, spot: f.spot, min: f.min, max: f.max }).collect()
    }

    fn strike_down(app: &mut App, folk: Entity) {
        let at = app.world().get::<Transform>(folk).unwrap().translation.xz();
        app.world_mut().write_message(Struck { target: folk, damage: 1000, from: at - Vec2::X * 9.0, bludgeoning: false });
        for _ in 0..120 {
            app.update();
        }
        assert!(app.world().get::<Corpse>(folk).is_some(), "it fell");
    }

    #[test]
    fn a_slain_folk_drops_everything_it_wore_and_held_once_where_it_fell() {
        use crate::inventory::underfoot::{FloorChange, FloorChanges};
        let (mut app, _, folk) = folk_fight(30.0);
        let gear = app.world().get::<Denizen>(folk).unwrap().person.gear.clone();
        assert!(gear.len() >= 2, "a folk wears and holds something");
        assert!(on_the_floor(&mut app).is_empty(), "nothing lies there while they stand");
        let lying_before = app.world().resource::<CurrentLevel>().0.items.len();
        strike_down(&mut app, folk);
        // Every item they wore and held, as the same item, once.
        let floor = on_the_floor(&mut app);
        assert_eq!(floor.len(), gear.len(), "one for each piece of gear");
        let inventory = app.world().resource::<Inventory>();
        let mut lying: Vec<(String, u32)> = floor.iter().map(|f| inventory.item(f.id).map(|i| (i.base.to_string(), i.seed)).unwrap()).collect();
        let mut wore: Vec<(String, u32)> = gear.iter().map(|(_, i)| (i.base.to_string(), i.seed)).collect();
        lying.sort();
        wore.sort();
        assert_eq!(lying, wore);
        for f in &floor {
            let item = inventory.item(f.id).unwrap();
            let original = gear.iter().map(|(_, i)| i).find(|i| i.seed == item.seed && i.base == item.base).unwrap();
            assert_eq!(item, original, "the same rarity, affixes and wear");
        }
        assert_eq!(app.world().resource::<CurrentLevel>().0.items.len(), lying_before + gear.len(), "the level records them");
        // Where they fell.
        let at = app.world().get::<Corpse>(folk).unwrap().fell.translation.xz();
        for f in &floor {
            let world = fb_level::Level::world(f.spot.x, f.spot.y);
            assert!(Vec2::new(world[0], world[2]).distance(at) < 12.0, "{} from where they fell", Vec2::new(world[0], world[2]).distance(at));
        }
        // The body is bare, and sheds nothing more however long it lies.
        assert!(app.world().get::<Denizen>(folk).unwrap().person.gear.is_empty());
        for _ in 0..300 {
            app.update();
        }
        assert_eq!(on_the_floor(&mut app).len(), gear.len(), "dropped once");
        // Picked up, each is the delver's to wear or hold where the folk had it.
        let ids: Vec<(crate::inventory::ItemId, Slot)> = floor
            .iter()
            .map(|f| {
                let item = app.world().resource::<Inventory>().item(f.id).unwrap().clone();
                (f.id, gear.iter().find(|(_, i)| *i == item).unwrap().0)
            })
            .collect();
        for &(id, _) in &ids {
            app.world_mut().resource_mut::<FloorChanges>().0.push(FloorChange::Lifted(id));
        }
        app.update();
        assert!(on_the_floor(&mut app).is_empty(), "all taken up");
        let mut inventory = app.world_mut().resource_mut::<Inventory>();
        for &(id, slot) in &ids {
            inventory.equip(id, slot).unwrap_or_else(|e| panic!("{slot:?}: {e:?}"));
            assert_eq!(inventory.worn(slot), Some(id), "{slot:?} is worn");
        }
    }

    #[test]
    fn a_body_brought_back_from_the_record_lies_bare_and_drops_nothing() {
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, _, folk) = folk_fight(30.0);
        strike_down(&mut app, folk);
        app.init_resource::<crate::record::WorldRecord>();
        app.world_mut().run_system_once(crate::record::record_level_left).unwrap();
        let record = app.world().resource::<crate::record::WorldRecord>().clone();
        let left = &record.level(1).unwrap().folk[&0];
        assert!(left.dead && left.hp == 0);
        // The level laid out again from the record: the folk come back as it has them.
        let mut again = combat_app();
        again.insert_resource(record).init_resource::<crate::folk::FolkFigures>().add_systems(Update, crate::folk::spawn_folk);
        for _ in 0..120 {
            again.update();
        }
        let mut denizens = again.world_mut().query::<(Entity, &Denizen)>();
        let spot = again.world().resource::<CurrentLevel>().0.folk[0];
        let (body, denizen) = denizens.iter(again.world()).find(|(_, d)| d.person.spot == spot).map(|(e, d)| (e, d.clone())).unwrap();
        assert!(denizen.person.gear.is_empty(), "bare");
        assert!(again.world().get::<Corpse>(body).is_some() && again.world().get::<crate::combat::feedback::Looted>(body).is_some());
        assert!(again.world().get::<Routine>(body).is_none(), "a body keeps no routine");
        assert!(on_the_floor(&mut again).is_empty(), "nothing set down again");
        let down = again.world().get::<Transform>(body).unwrap();
        assert!((down.rotation * Vec3::Y).y.abs() < 0.2, "lying down");
        // Another, drawn in what they wear, struck down here: their figure is drawn again with
        // none of it on.
        let (other, person) = denizens.iter(again.world()).find(|(e, _)| *e != body).map(|(e, d)| (e, d.person.clone())).unwrap();
        let titles: Vec<String> = person.gear.iter().map(|(_, i)| i.title().to_string()).collect();
        let drawn = |app: &mut App| -> Vec<String> {
            let mut names = app.world_mut().query::<&Name>();
            let children = app.world().get::<Children>(other).map(|c| c.to_vec()).unwrap_or_default();
            let mut found = Vec::new();
            for child in children {
                let mut stack = vec![child];
                while let Some(e) = stack.pop() {
                    if let Ok(name) = names.get(app.world(), e) {
                        found.push(name.to_string());
                    }
                    stack.extend(app.world().get::<Children>(e).map(|c| c.to_vec()).unwrap_or_default());
                }
            }
            found
        };
        assert!(drawn(&mut again).iter().any(|n| titles.contains(n)), "drawn dressed");
        strike_down(&mut again, other);
        let now = drawn(&mut again);
        assert!(now.iter().any(|n| n == "Body"), "the body is drawn");
        assert!(!now.iter().any(|n| titles.contains(n)), "still drawn in its gear: {now:?}");
        assert_eq!(on_the_floor(&mut again).len(), person.gear.len());
    }
}
