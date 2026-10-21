//! What a folk does when they see the delver, by how their faction stands with them (#552).
//!
//! The band is the delver's standing with the folk's faction (`WorldRecord::standings`, #542):
//!
//! - **Hostile**: a folk who sees the delver attacks ([`hostile_on_sight`] turns them
//!   [`Hostility::Hostile`], and `crate::folk_combat::fight` does the rest), though they were never
//!   provoked themselves. A folk of a Hostile faction does not calm down.
//! - **Wary**: while the delver stands in a room the folk's faction holds ([`Level::claim_of`]) and
//!   is seen, the folk stops their routine and turns to watch ([`Attending`]), carrying on when
//!   the delver leaves sight or the claim ([`regard`]).
//! - **Neutral**: they ignore the delver, and keep to their routine.
//! - **Friendly**: when the delver comes within [`GREET_RANGE`] in sight, the folk turns to greet
//!   them, once, with a line shown low in the view ("Bera nods to you."); not again for
//!   [`GREET_COOLDOWN`] seconds ([`Greeting`]).
//! - **Allied**: as Friendly, and they join a fight: a creature that strikes the delver
//!   ([`LastStriker`]) is gone for by each allied folk within [`ALLY_RANGE`] of it with a line to
//!   it and to the delver ([`assist`]). They close in at a dash and swing what their main hand
//!   holds, a blow that lands as it does for the delver ([`Struck`]); a blow rolled to hit against
//!   the creature's Armor Class as the delver's are, and a kill it makes is not the delver's
//!   (`Struck::by_ally`).
//!
//! Sight is a clear line across the level ([`fb_level::path::line_of_sight`]) within [`SEE`].
//! Our own.

use bevy::prelude::*;
use fb_character::combat::Odds;
use fb_character::modifier;
use crate::combat::{ArmourClass, attack_roll};
use fb_faction::Band;
use fb_level::path::line_of_sight;
use fb_level::{CELL, Level};

use crate::combat::feedback::{Corpse, Struck};
use crate::creatures::Critter;
use crate::creatures::behaviour::{AttackOnDelver, body_radius};
use crate::folk::{Denizen, Stride};
use crate::folk_combat::{COOLDOWN, Combatant, DASH, Fight, LANDS, TURN_RATE};
use crate::folk_pose::ArmSwing;
use crate::folk_routine::{Routine, pace};
use crate::hostility::Hostility;
use crate::inventory::Toasts;
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Obstacle, Obstacles, Player};
use crate::player::slide_around;
use crate::record::WorldRecord;
use crate::tuning::Tuning;

/// How far a folk sees the delver, world units (12 cells).
pub const SEE: f32 = 12.0 * CELL;
/// How near the delver comes before a Friendly folk greets them (4 cells).
pub const GREET_RANGE: f32 = 4.0 * CELL;
/// How long a greeting holds the folk, seconds.
pub const GREET_TIME: f32 = 1.6;
/// How long before the same folk greets the delver again, seconds.
pub const GREET_COOLDOWN: f32 = 90.0;
/// How far from a creature an allied folk still goes to the delver's help (10 cells).
pub const ALLY_RANGE: f32 = 10.0 * CELL;
/// How soon a faction's folk speak a greeting again, seconds, so a group is one line.
pub const TOAST_GAP: f32 = 5.0;
/// How long a creature stays the one that struck the delver last, seconds.
pub const STRIKER_FOR: f32 = 12.0;

pub struct FolkRegardPlugin;

impl Plugin for FolkRegardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LastStriker>()
            .init_resource::<Obstacles>()
            .init_resource::<BlockedCells>()
            .init_resource::<Tuning>()
            .add_systems(Update, (note_striker, hostile_on_sight, regard, assist).chain().after(crate::hostility::turn_on_the_delver).before(crate::folk_routine::walk_routines).in_set(DelveSystems));
        // `FB_STANDING=<faction index>:<band>`, for captures (README, "Debug switches").
        if let Some(set) = std::env::var("FB_STANDING").ok().and_then(|v| parse_standing(&v)) {
            app.insert_resource(DebugStanding(set)).add_systems(Update, force_standing.in_set(DelveSystems));
        }
    }
}

/// A folk held to watch or greet the delver, until their system lets them go. The routine leaves
/// them held while this is on them.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Attending;

/// An allied folk who has joined a fight: the routine leaves them to [`assist`].
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Assisting;

/// A folk who has greeted the delver: seconds of the greeting left, and seconds until they would
/// greet again.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Greeting {
    pub left: f32,
    pub cooldown: f32,
}

/// The creature that last struck the delver, and when (seconds into the run).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct LastStriker(pub Option<(Entity, f32)>);

/// A faction's band forced for captures.
#[derive(Resource, Clone, Copy, Debug)]
struct DebugStanding((fb_faction::Faction, Band));

fn parse_standing(v: &str) -> Option<(fb_faction::Faction, Band)> {
    let (id, band) = v.split_once(':')?;
    let band = Band::ALL.into_iter().find(|b| b.label().eq_ignore_ascii_case(band.trim()))?;
    Some((fb_faction::Faction(id.trim().parse().ok()?), band))
}

/// Holds the faction `FB_STANDING` names at the middle of its band.
fn force_standing(set: Res<DebugStanding>, mut record: ResMut<WorldRecord>) {
    let (faction, band) = set.0;
    if record.standings.band(faction) != band {
        let middle = match band {
            Band::Hostile => -75,
            Band::Wary => -30,
            Band::Neutral => 0,
            Band::Friendly => 30,
            Band::Allied => 75,
        };
        let now = record.standings.get(faction).value();
        record.standings.shift(faction, middle - now);
    }
}

fn grid(p: Vec2) -> (f32, f32) {
    (p.x / CELL, p.y / CELL)
}

fn cell(p: Vec2) -> (i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

/// Whether `p` (world `(x, z)`) stands in a room `faction` holds on `level`.
pub fn in_claim(level: &Level, p: Vec2, faction: fb_faction::Faction) -> bool {
    let (x, y) = cell(p);
    level.room_at(x, y).and_then(|room| level.claim_of(room)).is_some_and(|claim| claim.faction == faction)
}

fn angle_between(from: f32, to: f32) -> f32 {
    (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Remembers the creature whose blow last landed on the delver.
fn note_striker(time: Res<Time>, mut attacks: MessageReader<AttackOnDelver>, mut last: ResMut<LastStriker>) {
    let now = time.elapsed_secs();
    for attack in attacks.read() {
        if attack.species.is_some() {
            last.0 = Some((attack.attacker, now));
        }
    }
}

/// Turns each folk of a Hostile faction who sees the delver hostile, provoked or not.
fn hostile_on_sight(
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    doors: Option<Res<crate::doors::Doors>>,
    record: Res<WorldRecord>,
    player: Query<&Transform, (With<Player>, Without<Denizen>)>,
    mut folk: Query<(&Denizen, &Transform, &mut Hostility), Without<Corpse>>,
) {
    let Some(player) = player.iter().next().map(|t| t.translation.xz()) else { return };
    for (denizen, at, mut hostility) in &mut folk {
        let blocked = crate::doors::solid_for(&level.0, &blocked_cells, doors.as_deref(), Some(denizen.person.faction.id));
        if hostility.is_hostile() || record.standings.band(denizen.person.faction.id) != Band::Hostile {
            continue;
        }
        let at = at.translation.xz();
        if at.distance(player) <= SEE && line_of_sight(grid(at), grid(player), &blocked) {
            *hostility = Hostility::Hostile;
            info!("{} of {} attacks the delver on sight", denizen.person.name, denizen.person.faction.name);
        }
    }
}

/// The first word of a name, for a greeting.
fn first_name(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or(name)
}

/// Watching by the Wary and greeting by the Friendly and Allied.
#[allow(clippy::too_many_arguments)]
fn regard(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    doors: Option<Res<crate::doors::Doors>>,
    record: Res<WorldRecord>,
    mut toasts: Toasts,
    mut last_spoke: Local<std::collections::HashMap<fb_faction::Faction, f32>>,
    player: Query<&Transform, (With<Player>, Without<Denizen>)>,
    mut folk: Query<(Entity, &Denizen, &Transform, &Hostility, &mut Routine, Option<&mut Greeting>, Has<Attending>, Has<Assisting>), Without<Corpse>>,
) {
    let dt = time.delta_secs().min(0.1);
    let Some(player) = player.iter().next().map(|t| t.translation.xz()) else { return };
    let now = time.elapsed_secs();
    for (entity, denizen, at, hostility, mut routine, greeting, attending, assisting) in &mut folk {
        let faction = denizen.person.faction.id;
        let blocked = crate::doors::solid_for(&level.0, &blocked_cells, doors.as_deref(), Some(faction));
        let band = record.standings.band(faction);
        let at = at.translation.xz();
        let distance = at.distance(player);
        let cares = !hostility.is_hostile() && matches!(band, Band::Wary | Band::Friendly | Band::Allied);
        let sees = cares && distance <= SEE && line_of_sight(grid(at), grid(player), &blocked);

        // Greeting: a hold of a moment, then the cooldown runs out on its own.
        let mut greeting_now = false;
        let mut greeted = greeting.is_some();
        if let Some(mut greeting) = greeting {
            greeting.cooldown -= dt;
            if greeting.left > 0.0 {
                greeting.left -= dt;
                greeting_now = greeting.left > 0.0 && !hostility.is_hostile();
                if !greeting_now {
                    greeting.left = 0.0;
                }
            }
            if greeting.cooldown <= 0.0 {
                commands.entity(entity).remove::<Greeting>();
                greeted = false;
            }
        }
        if !greeted && sees && !assisting && matches!(band, Band::Friendly | Band::Allied) && distance <= GREET_RANGE {
            // Kin who greet together turn together, but one speaks for them.
            if last_spoke.get(&faction).is_none_or(|&then| now - then > TOAST_GAP) {
                last_spoke.insert(faction, now);
                toasts.add(&format!("{} nods to you.", first_name(&denizen.person.name)));
            }
            commands.entity(entity).insert(Greeting { left: GREET_TIME, cooldown: GREET_COOLDOWN });
            greeting_now = true;
        }

        let watching = sees && !assisting && band == Band::Wary && in_claim(&level.0, player, faction);
        let hold = watching || greeting_now;
        if hold {
            // Their own hold, taken once and given back once: others who hold them (a talk) keep
            // them while theirs lasts.
            if !attending {
                commands.entity(entity).insert(Attending);
                routine.hold();
            }
        } else if attending {
            commands.entity(entity).remove::<Attending>();
            routine.release();
        }
    }
}

/// Allied folk who see the delver go for the creature that struck them last.
#[allow(clippy::too_many_arguments)]
fn assist(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<CurrentLevel>,
    tuning: Res<Tuning>,
    blocked_cells: Res<BlockedCells>,
    doors: Option<Res<crate::doors::Doors>>,
    obstacles: Res<Obstacles>,
    record: Res<WorldRecord>,
    last: Res<LastStriker>,
    mut struck: MessageWriter<Struck>,
    rules: Option<Res<crate::rules::GameRules>>,
    player: Query<&Transform, (With<Player>, Without<Denizen>)>,
    creatures: Query<(&Critter, &Transform, Option<&ArmourClass>), (Without<Denizen>, Without<Corpse>, Without<Player>)>,
    mut folk: Query<(Entity, &Denizen, &Hostility, &Combatant, &mut Fight, &mut Stride, &mut ArmSwing, &mut Transform, &mut Routine, Has<Assisting>), (With<Denizen>, Without<Corpse>, Without<Critter>, Without<Player>)>,
) {
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }
    let Some(player) = player.iter().next().map(|t| t.translation.xz()) else { return };
    let now = time.elapsed_secs();
    let target = last.0.filter(|(_, when)| now - when <= STRIKER_FOR).and_then(|(creature, _)| creatures.get(creature).ok().map(|(c, t, ac)| (creature, c, t.translation.xz(), ac.copied().unwrap_or(ArmourClass::UNARMOURED).0)));
    let relief = crate::level::geometry::Relief::new(&level.0);
    for (entity, denizen, hostility, combatant, mut fight, mut stride, mut arm, mut transform, mut routine, assisting) in &mut folk {
        let blocked = crate::doors::solid_for(&level.0, &blocked_cells, doors.as_deref(), Some(denizen.person.faction.id));
        let at = transform.translation.xz();
        let ally = !hostility.is_hostile() && record.standings.band(denizen.person.faction.id) == Band::Allied;
        let engaged = target.filter(|&(_, _, goes, _)| {
            ally && at.distance(goes) <= ALLY_RANGE && (assisting || (at.distance(player) <= SEE && line_of_sight(grid(at), grid(goes), &blocked) && line_of_sight(grid(at), grid(player), &blocked)))
        });
        let Some((creature, critter, goes, armour)) = engaged else {
            if assisting {
                commands.entity(entity).remove::<Assisting>();
                fight.calm();
                stride.speed = 0.0;
                arm.0 = None;
                routine.head_home();
            }
            continue;
        };
        if !assisting {
            commands.entity(entity).insert(Assisting);
            routine.hold();
            info!("{} joins the delver's fight", denizen.person.name);
        }
        let reach = body_radius(critter) + combatant.weapon.reach;
        let in_reach = at.distance(goes) <= reach;
        fight.cooldown = (fight.cooldown - dt).max(0.0);
        let mut moved = 0.0;
        if let Some(since) = fight.swing.as_mut() {
            *since += dt;
            let since = *since;
            if !fight.landed && since >= LANDS * combatant.swing_time {
                fight.landed = true;
                if at.distance(goes) <= reach * 1.15 {
                    // Rolled to hit against its Armor Class as the delver's swings are.
                    let attack = attack_roll(crate::rules::GameRules::of(&rules), &combatant.weapon, combatant.strength, armour, Odds::Even, &mut fight.rng);
                    if attack.hit {
                        let damage = combatant.weapon.roll(modifier(combatant.strength), &mut fight.rng);
                        struck.write(Struck { target: creature, damage, from: at, bludgeoning: combatant.weapon.bludgeoning, critical: attack.natural == 20, by_ally: true });
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
            let direction = (goes - at).normalize_or_zero();
            let own = |o: &&Obstacle| matches!(o, Obstacle::Circle { centre, radius } if centre.distance(at) < 0.5 * denizen.radius && *radius == denizen.radius);
            let others: Vec<Obstacle> = obstacles.0.iter().filter(|o| !own(o)).copied().collect();
            let step = (direction * pace(denizen, tuning.folk.walk_speed) * DASH * dt).clamp_length_max((goes - at).length());
            let to = slide_around(at, at + step, denizen.radius, &blocked, &others);
            moved = (to - at).length();
            transform.translation = Vec3::new(to.x, relief.floor_height(to.x, to.y), to.y);
        }
        stride.speed = moved / dt;
        arm.0 = fight.swing.map(|since| (since / combatant.swing_time).min(1.0));
        let to = goes - transform.translation.xz();
        if to.length_squared() > 1e-4 {
            let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
            let turn = angle_between(yaw, to.x.atan2(to.y));
            transform.rotation = Quat::from_rotation_y(yaw + turn.clamp(-TURN_RATE * dt, TURN_RATE * dt));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Health;
    use crate::folk_combat::tests::{folk_fight, out_of_sight_of};
    use crate::folk_routine::Phase;
    use crate::inventory::Toast;

    /// Holds `folk`'s faction at `band` for a delver beginning lawful good.
    fn set_band(app: &mut App, folk: Entity, band: Band) {
        let faction = app.world().get::<Denizen>(folk).unwrap().person.faction.id;
        let seed = app.world().resource::<CurrentLevel>().0.seed;
        let mut record = WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, seed);
        let middle = match band {
            Band::Hostile => -75,
            Band::Wary => -30,
            Band::Neutral => 0,
            Band::Friendly => 30,
            Band::Allied => 75,
        };
        let now = record.standings.get(faction).value();
        record.standings.shift(faction, middle - now);
        assert_eq!(record.standings.band(faction), band);
        app.insert_resource(record);
    }

    fn place(app: &mut App, entity: Entity, at: Vec2) {
        let y = app.world().get::<Transform>(entity).unwrap().translation.y;
        app.world_mut().get_mut::<Transform>(entity).unwrap().translation = Vec3::new(at.x, y, at.y);
    }

    fn at(app: &App, entity: Entity) -> Vec2 {
        app.world().get::<Transform>(entity).unwrap().translation.xz()
    }

    fn step(app: &mut App, frames: usize) {
        for _ in 0..frames {
            app.update();
        }
    }

    #[test]
    fn a_folk_of_a_hostile_faction_attacks_on_sight_without_being_provoked() {
        let (mut app, player, folk) = folk_fight(30.0);
        app.world_mut().entity_mut(player).insert(Health::full(500));
        set_band(&mut app, folk, Band::Hostile);
        assert_eq!(*app.world().get::<Hostility>(folk).unwrap(), Hostility::Friendly, "never provoked");
        let mut blows = 0;
        for _ in 0..600 {
            app.update();
            blows += app.world().resource::<Messages<AttackOnDelver>>().iter_current_update_messages().filter(|a| a.attacker == folk).count();
            blows += app.world().resource::<Messages<crate::creatures::behaviour::MissedDelver>>().len();
        }
        assert!(app.world().get::<Hostility>(folk).unwrap().is_hostile());
        assert!(blows > 0, "no blows");
    }

    #[test]
    fn a_hostile_factions_folk_does_not_calm_down_out_of_sight() {
        let (mut app, player, folk) = folk_fight(30.0);
        app.world_mut().entity_mut(player).insert(Health::full(500));
        app.world_mut().resource_mut::<Tuning>().folk.lose_interest_after = 1.0;
        set_band(&mut app, folk, Band::Hostile);
        step(&mut app, 10);
        assert!(app.world().get::<Hostility>(folk).unwrap().is_hostile());
        let far = out_of_sight_of(&app, at(&app, folk));
        place(&mut app, player, Vec2::new(far.x, far.z));
        step(&mut app, 240);
        assert!(app.world().get::<Hostility>(folk).unwrap().is_hostile(), "calmed down");
    }

    /// A cell of the folk's own room, between `min` and `max` world units from them, in sight.
    fn cell_in_their_room(app: &App, folk: Entity, min: f32, max: f32) -> Vec2 {
        let level = &app.world().resource::<CurrentLevel>().0;
        let here = at(app, folk);
        let room = level.room_at(cell(here).0, cell(here).1).expect("the folk stand in a room");
        let blocked = |x: i32, y: i32| !level.is_open(x, y);
        (0..level.height as i32)
            .flat_map(|y| (0..level.width as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| level.room_at(x, y) == Some(room))
            .map(|(x, y)| Vec2::new(x as f32 + 0.5, y as f32 + 0.5) * CELL)
            .find(|p| (min..=max).contains(&p.distance(here)) && line_of_sight(grid(*p), grid(here), blocked))
            .expect("a cell in the room")
    }

    #[test]
    fn a_wary_folk_in_its_claim_holds_its_routine_faces_the_delver_and_resumes_when_they_leave() {
        let (mut app, player, folk) = folk_fight(30.0);
        set_band(&mut app, folk, Band::Wary);
        let faction = app.world().get::<Denizen>(folk).unwrap().person.faction.id;
        let stand = cell_in_their_room(&app, folk, 45.0, 90.0);
        assert!(in_claim(&app.world().resource::<CurrentLevel>().0, stand, faction), "the folk's room is their faction's claim");
        place(&mut app, player, stand);
        step(&mut app, 90);
        assert_eq!(app.world().get::<Routine>(folk).unwrap().phase, Phase::Held);
        assert!(app.world().get::<Attending>(folk).is_some());
        let to = stand - at(&app, folk);
        let forward = (app.world().get::<Transform>(folk).unwrap().rotation * Vec3::Z).xz();
        assert!(forward.angle_to(to).abs() < 0.2, "faces the delver: {forward:?} toward {to:?}");
        // They stay held however long the delver stands there.
        step(&mut app, 300);
        assert_eq!(app.world().get::<Routine>(folk).unwrap().phase, Phase::Held);
        // Out of sight, they carry on.
        let far = out_of_sight_of(&app, at(&app, folk));
        place(&mut app, player, Vec2::new(far.x, far.z));
        step(&mut app, 10);
        assert!(app.world().get::<Attending>(folk).is_none());
        assert_ne!(app.world().get::<Routine>(folk).unwrap().phase, Phase::Held);
    }

    #[test]
    fn a_neutral_folk_in_its_claim_keeps_its_routine() {
        let (mut app, player, folk) = folk_fight(30.0);
        set_band(&mut app, folk, Band::Neutral);
        let stand = cell_in_their_room(&app, folk, 45.0, 90.0);
        place(&mut app, player, stand);
        step(&mut app, 60);
        assert!(app.world().get::<Attending>(folk).is_none());
        assert_ne!(app.world().get::<Routine>(folk).unwrap().phase, Phase::Held);
    }

    #[test]
    fn a_friendly_folk_greets_once_within_range_and_not_again_during_the_cooldown() {
        let (mut app, _, folk) = folk_fight(30.0);
        set_band(&mut app, folk, Band::Friendly);
        step(&mut app, 5);
        let greeting = *app.world().get::<Greeting>(folk).expect("greeted");
        assert!(greeting.left > 0.0 && app.world().get::<Attending>(folk).is_some());
        let name = first_name(&app.world().get::<Denizen>(folk).unwrap().person.name).to_string();
        assert_eq!(app.world().resource::<Toast>().text, format!("{name} nods to you."));
        // The greeting ends, and the delver standing there is not greeted again.
        step(&mut app, 200);
        assert_eq!(app.world().get::<Greeting>(folk).unwrap().left, 0.0);
        assert!(app.world().get::<Attending>(folk).is_none());
        app.world_mut().resource_mut::<Toast>().text.clear();
        step(&mut app, 3000);
        assert!(app.world().resource::<Toast>().text.is_empty(), "greeted again: {:?}", app.world().resource::<Toast>().text);
        // After the cooldown, a new greeting.
        step(&mut app, (GREET_COOLDOWN * 60.0) as usize);
        assert!(app.world().resource::<Toast>().text.contains("nods to you"), "never greeted again");
    }

    #[test]
    fn a_friendly_folk_out_of_range_does_not_greet() {
        let (mut app, player, folk) = folk_fight(30.0);
        set_band(&mut app, folk, Band::Friendly);
        let far = cell_in_their_room(&app, folk, GREET_RANGE + 5.0, SEE);
        place(&mut app, player, far);
        step(&mut app, 30);
        assert!(app.world().get::<Greeting>(folk).is_none());
    }

    /// A skeleton stood at `at`, with the folk's level around it.
    fn creature_at(app: &mut App, at: Vec2) -> (Entity, fb_level::CreatureSpot) {
        let level = app.world().resource::<CurrentLevel>().0.clone();
        let spot = level.creatures[0];
        let root = {
            let mut commands = app.world_mut().commands();
            crate::creatures::spawn_skeleton(&mut commands, &level, &spot, &spot.creature().rig()).0
        };
        app.world_mut().flush();
        place(app, root, at);
        (root, spot)
    }

    /// Blows struck on `creature` over `frames`, and whether any folk joined in.
    fn blows_on(app: &mut App, creature: Entity, frames: usize) -> (usize, bool) {
        let (mut blows, mut joined) = (0, false);
        for _ in 0..frames {
            app.update();
            joined |= app.world_mut().query_filtered::<(), With<Assisting>>().iter(app.world()).next().is_some();
            blows += app.world().resource::<Messages<Struck>>().iter_current_update_messages().filter(|s| s.target == creature).count();
        }
        (blows, joined)
    }

    #[test]
    fn an_allied_folk_goes_for_the_creature_that_struck_the_delver() {
        let (mut app, player, folk) = folk_fight(30.0);
        set_band(&mut app, folk, Band::Allied);
        let here = at(&app, folk);
        let beside = at(&app, player) + Vec2::new(0.0, 8.0);
        let (creature, spot) = creature_at(&mut app, beside);
        app.world_mut().write_message(AttackOnDelver { attacker: creature, species: Some(spot.species), damage: 1, poison: 0, attacker_pos: beside });
        let (blows, joined) = blows_on(&mut app, creature, 600);
        assert!(joined, "never joined");
        assert!(blows > 0, "no blows on the creature");
        assert!(at(&app, folk).distance(at(&app, creature)) < here.distance(at(&app, creature)), "closed in");
    }

    #[test]
    fn a_high_armour_class_turns_the_allys_blows_and_its_kills_are_not_the_delvers() {
        let landed = |armour: Option<i32>| {
            let (mut app, player, folk) = folk_fight(30.0);
            set_band(&mut app, folk, Band::Allied);
            let beside = at(&app, player) + Vec2::new(0.0, 8.0);
            let (creature, spot) = creature_at(&mut app, beside);
            if let Some(ac) = armour {
                app.world_mut().entity_mut(creature).insert(ArmourClass(ac));
            }
            // Out of reach of killing it, so the count is of blows and not of hit points.
            app.world_mut().entity_mut(creature).insert(crate::combat::Health::full(100_000));
            app.world_mut().write_message(AttackOnDelver { attacker: creature, species: Some(spot.species), damage: 1, poison: 0, attacker_pos: beside });
            let mut allied = true;
            let mut count = 0;
            for _ in 0..1200 {
                app.update();
                for s in app.world().resource::<Messages<Struck>>().iter_current_update_messages().filter(|s| s.target == creature) {
                    allied &= s.by_ally;
                    count += 1;
                }
            }
            assert!(allied, "a blow was not marked as an ally's");
            count
        };
        let (soft, hard) = (landed(None), landed(Some(40)));
        assert!(soft > 0 && hard < soft, "{hard} landed on AC 40, {soft} on AC 10");
    }

    #[test]
    fn a_group_greets_with_one_line_however_their_arrivals_stagger() {
        let (mut app, player, folk) = folk_fight(30.0);
        set_band(&mut app, folk, Band::Friendly);
        let kin = app.world_mut().entity_mut(folk).clone_and_spawn();
        let home = at(&app, folk);
        place(&mut app, kin, home + Vec2::new(0.0, 200.0));
        step(&mut app, 3);
        assert!(app.world().get::<Greeting>(folk).is_some() && app.world().get::<Greeting>(kin).is_none());
        let first = app.world().resource::<Toast>().text.clone();
        // The kinsman walks up a second later, and greets without a line of his own.
        step(&mut app, 60);
        let near = at(&app, player) + Vec2::new(0.0, 20.0);
        place(&mut app, kin, near);
        step(&mut app, 3);
        assert!(app.world().get::<Greeting>(kin).is_some(), "the kinsman did not greet");
        assert_eq!(app.world().resource::<Toast>().text.matches("nods to you").count(), 1, "{first:?}");
    }

    #[test]
    fn a_greeting_ending_does_not_free_a_folk_someone_else_holds() {
        let (mut app, _, folk) = folk_fight(30.0);
        set_band(&mut app, folk, Band::Friendly);
        step(&mut app, 3);
        app.world_mut().get_mut::<Routine>(folk).unwrap().hold(); // a talk
        step(&mut app, 200);
        assert!(app.world().get::<Attending>(folk).is_none(), "the greeting is over");
        assert_eq!(app.world().get::<Routine>(folk).unwrap().phase, Phase::Held);
    }

    #[test]
    fn a_friendly_folk_leaves_the_fight_to_the_delver() {
        let (mut app, player, folk) = folk_fight(30.0);
        set_band(&mut app, folk, Band::Friendly);
        let beside = at(&app, player) + Vec2::new(0.0, 8.0);
        let (creature, spot) = creature_at(&mut app, beside);
        app.world_mut().write_message(AttackOnDelver { attacker: creature, species: Some(spot.species), damage: 1, poison: 0, attacker_pos: beside });
        assert_eq!(blows_on(&mut app, creature, 300), (0, false));
    }

    #[test]
    fn the_standing_switch_reads_a_faction_and_a_band() {
        assert_eq!(parse_standing("2:wary"), Some((fb_faction::Faction(2), Band::Wary)));
        assert_eq!(parse_standing("0:Allied"), Some((fb_faction::Faction(0), Band::Allied)));
        assert_eq!(parse_standing("x:wary"), None);
        assert_eq!(parse_standing("1:grumpy"), None);
    }
}
