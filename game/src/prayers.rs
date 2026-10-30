//! The cleric's fighting prayers: Smite and Shield. Both are written from terms like any
//! prayer ([`fb_character::casting`]) and reacted to here from the [`Casted`] message.
//!
//! **Smite** (smite, at one you see, instant) falls on the creature nearest the crosshair within
//! the cast's range that the cleric can see: a wall or a shut door stops it, as it stops frighten
//! and charm. It deals a d8 radiant a tier ([`smite_damage`]), doubled against the undead, rolled
//! from [`RunDice`] and sent as a [`Struck`], the one path every blow of the delver's takes, so
//! hit points, callouts, kills, deeds and corpses work as for a sword. The pillar falls from
//! above, so it comes from the creature's own spot: no shield turns it and it pushes nothing.
//!
//! Smite can only be written with the sight reach ([`fb_character::casting::ComposeError::NeedsReach`]),
//! and only a prayer of the smite effect needs a target, so light + sight casts with no one there.
//!
//! Finding the target is done twice: [`crate::casting::cast_requests`] asks [`Sight`] before the
//! mana is spent, so a cast with no one in sight costs nothing, and [`smite_the_seen`] asks again
//! when the cast has gone through. Both see the same floor in the same frame.
//!
//! **Shield** (shield, self, short) raises the caster's armour class by 2 a tier
//! ([`fb_character::casting::shield_bonus`]) while a [`ShieldPrayer`] lasts. It is an entity like
//! Divine Light's bauble, so it is not saved with the delve, as the light is not: it ends when its
//! game time is up ([`end_the_shield`]), and recasting replaces it, never stacks. The delver's
//! [`crate::combat::ArmourClass`] and the sheet add it as its own "Shield" term
//! ([`crate::character::Character::armour_class_shielded`]).
//!
//! **The visuals hooks**: [`PrayerLanded`] is sent when a prayer's effect lands, with the
//! effect's name (`"smite"` on the creature it fell on, `"shield"` on the caster, with no
//! targets); the effect system plays the pillar at the targets' feet, and the shimmer for as
//! long as a [`ShieldPrayer`] exists. [`ShieldStruck`] is sent when a blow lands on a shielded
//! caster, for the ripple.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use fb_character::casting::smite_damage;
use fb_character::combat::FEET_TO_UNITS;
use fb_items::Foe;

use crate::casting::{Casted, cast_requests};
use crate::combat::RunDice;
use crate::combat::feedback::{Corpse, Struck};
use crate::creatures::Critter;
use crate::camp::Asleep;
use crate::effects::{EffectLibrary, PlayEffect, Playing};
use crate::rules::GameRules;
use crate::tuning::Tuning;
use crate::creatures::behaviour::AttackOnDelver;
use crate::game_clock::GameClock;
use crate::launch::AppState;
use crate::play::{DelveSystems, Player};

/// A prayer's effect landed on creatures: what the effect system plays it from. `effect` is the
/// effect term's id (`"smite"`), `targets` the creatures it fell on (play it at their feet).
#[derive(Message, Clone, Debug, PartialEq)]
pub struct PrayerLanded {
    pub effect: String,
    pub tier: u8,
    pub caster: Entity,
    pub targets: Vec<Entity>,
}

/// The Shield prayer on the delver: `bonus` armour class until game time `expires`. At most one
/// exists; a new cast replaces it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[require(DespawnOnExit<AppState> = DespawnOnExit(AppState::Delving))]
pub struct ShieldPrayer {
    /// The tier it was cast at.
    pub tier: u8,
    /// The armour class it adds.
    pub bonus: i32,
    /// The game time it ends, seconds.
    pub expires: f64,
    /// Game seconds it lasts in all, from its cast to `expires`.
    pub lasts: f64,
}

impl ShieldPrayer {
    /// The share of its time it has left at game time `now`, 1 when cast to 0 when over.
    pub fn time_left(&self, now: f64) -> f32 {
        if self.lasts <= 0.0 {
            return 0.0;
        }
        ((self.expires - now) / self.lasts).clamp(0.0, 1.0) as f32
    }
}

/// A blow landed on a delver under a [`ShieldPrayer`]: what the ripple and flash are played
/// from. `from` is where the blow came from on the floor (world `(x, z)`).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct ShieldStruck {
    pub caster: Entity,
    pub from: Vec2,
    pub damage: u32,
}

/// The armour class the shields on the delver add (the highest; a recast replaces, never stacks).
pub fn shield_bonus<'a>(shields: impl IntoIterator<Item = &'a ShieldPrayer>) -> i32 {
    shields.into_iter().map(|s| s.bonus).max().unwrap_or(0)
}

pub fn register(app: &mut App) {
    app.add_message::<PrayerLanded>()
        .add_message::<ShieldStruck>()
        .add_message::<AttackOnDelver>()
        .add_message::<Struck>()
        .add_message::<PlayEffect>()
        .add_systems(
            Update,
            (smite_the_seen, raise_the_shield, end_the_shield, ripple_the_shield, show_the_prayers, start_the_shimmer, hold_the_shimmer, debug_blow).chain().after(cast_requests).in_set(DelveSystems),
        );
}

/// The effect the shield plays for as long as it stands, and the one a blow on it plays.
const SHIELD_EFFECT: &str = "shield";
const SHIELD_STRUCK_EFFECT: &str = "shield_struck";
/// How far from the delver's feet a blow's ripple plays, toward where it came from, units.
const RIPPLE_REACH: f32 = 9.0;
/// And this far ahead of them, units.
const RIPPLE_AHEAD: f32 = 14.0;

/// How tall a creature stands, world units, for the effects that anchor to its head and middle.
fn height_of(critter: &Critter) -> f32 {
    use fb_creatures::Creature;
    let rolled = match &critter.creature {
        Creature::Skeleton(s) => s.height(),
        Creature::Troll(t) => t.height(),
        Creature::Rat(r) => r.height(),
        Creature::Jackalope(j) => j.height(),
        other => other.radius() * 2.0,
    };
    (rolled * critter.size).max(critter.radius)
}

/// A creature that is still on its feet, and is not the delver.
type Alive = (Without<Player>, Without<Corpse>);

/// What the cleric can see of the creatures round them: the way [`crate::casting_influence`]
/// finds those in reach, a wall or a shut door blocking the line.
#[derive(SystemParam)]
pub struct Sight<'w, 's> {
    player: Query<'w, 's, (Entity, &'static Transform), With<Player>>,
    creatures: Query<'w, 's, (Entity, &'static Critter, &'static Transform), Alive>,
    level: Option<Res<'w, crate::play::CurrentLevel>>,
    blocked_cells: Option<Res<'w, crate::play::BlockedCells>>,
    doors: Option<Res<'w, crate::doors::Doors>>,
}

impl Sight<'_, '_> {
    /// The creature nearest the crosshair (the way the delver faces) within `feet` that they can
    /// see and that is ahead of them; `None` with no one. Nearest the crosshair is the smallest
    /// angle off straight ahead, the closer winning a tie.
    pub fn nearest_the_crosshair(&self, feet: f64) -> Option<Entity> {
        let (_, me) = self.player.iter().next()?;
        let (at, ahead) = (me.translation.xz(), me.forward().as_vec3().xz().normalize_or_zero());
        let range = feet as f32 * FEET_TO_UNITS;
        let cells = self.blocked_cells.as_deref().cloned().unwrap_or_default();
        let see = self.level.as_ref().map(|l| crate::doors::opaque_for(&l.0, &cells, self.doors.as_deref(), None));
        let seen = |p: Vec2| see.as_ref().is_none_or(|see| fb_level::path::line_of_sight((at.x / fb_level::CELL, at.y / fb_level::CELL), (p.x / fb_level::CELL, p.y / fb_level::CELL), see));
        self.creatures
            .iter()
            .filter_map(|(entity, critter, t)| {
                let to = t.translation.xz() - at;
                let near = to.length();
                let off = ahead.angle_to(to.normalize_or_zero()).abs();
                (near <= range + critter.radius && to.dot(ahead) > 0.0 && seen(t.translation.xz())).then_some((off, near, entity))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)))
            .map(|(_, _, e)| e)
    }

    /// The caster, if there is one.
    pub fn caster(&self) -> Option<Entity> {
        self.player.iter().next().map(|(e, _)| e)
    }
}

/// Smite: hurts the creature each cast of it fell on, as any blow of the delver's does.
fn smite_the_seen(mut casts: MessageReader<Casted>, sight: Sight, mut dice: ResMut<RunDice>, mut struck: MessageWriter<Struck>, mut landed: MessageWriter<PrayerLanded>, creatures: Query<(&Critter, &Transform)>) {
    for Casted(cast) in casts.read() {
        if cast.written.effect != "smite" {
            continue;
        }
        let (Some(caster), Some(target)) = (sight.caster(), sight.nearest_the_crosshair(cast.range)) else { continue };
        let Ok((critter, at)) = creatures.get(target) else { continue };
        let damage = smite_damage(cast.tier, critter.spot.species.foe() == Foe::Undead, &mut dice.0);
        // Radiant is taken as it is, like the fire of a torch: carried as `extra` damage so a
        // skeleton's weakness to bludgeoning never doubles it.
        struck.write(Struck { target, damage: 0, from: at.translation.xz(), bludgeoning: false, critical: false, by_ally: false, graze: false, extra: damage });
        landed.write(PrayerLanded { effect: "smite".into(), tier: cast.tier, caster, targets: vec![target] });
    }
}

/// Shield: raises it on the caster, replacing any that stood.
fn raise_the_shield(mut casts: MessageReader<Casted>, mut commands: Commands, clock: Option<Res<GameClock>>, players: Query<Entity, With<Player>>, old: Query<Entity, With<ShieldPrayer>>, mut landed: MessageWriter<PrayerLanded>) {
    for Casted(cast) in casts.read() {
        if cast.written.effect != "shield" {
            continue;
        }
        let Some(caster) = players.iter().next() else { continue };
        let now = clock.as_deref().map_or(0.0, GameClock::game_seconds);
        for shield in &old {
            commands.entity(shield).despawn();
        }
        let bonus = fb_character::casting::shield_bonus(cast.tier);
        commands.spawn((Name::new(format!("Shield, tier {}", cast.tier)), ShieldPrayer { tier: cast.tier, bonus, expires: now + cast.duration, lasts: cast.duration }));
        landed.write(PrayerLanded { effect: "shield".into(), tier: cast.tier, caster, targets: vec![] });
    }
}

/// Lets the shield go when its time is up.
fn end_the_shield(mut commands: Commands, clock: Option<Res<GameClock>>, shields: Query<(Entity, &ShieldPrayer)>) {
    let now = clock.as_deref().map_or(0.0, GameClock::game_seconds);
    for (entity, shield) in &shields {
        if now >= shield.expires {
            commands.entity(entity).despawn();
        }
    }
}

/// Says each blow that lands on the shielded delver, for the ripple.
fn ripple_the_shield(mut blows: MessageReader<AttackOnDelver>, shields: Query<&ShieldPrayer>, players: Query<Entity, With<Player>>, mut struck: MessageWriter<ShieldStruck>) {
    for blow in blows.read() {
        if let (Some(caster), false) = (players.iter().next(), shields.is_empty()) {
            struck.write(ShieldStruck { caster, from: blow.attacker_pos, damage: blow.damage });
        }
    }
}

/// Plays what a prayer shows: Smite's pillar on each creature it fell on, following it, and the
/// shield's shimmer for as long as its [`ShieldPrayer`] stands; a blow on the shield plays a ripple
/// toward where it came from.
fn show_the_prayers(
    mut landed: MessageReader<PrayerLanded>,
    mut struck: MessageReader<ShieldStruck>,
    mut plays: MessageWriter<PlayEffect>,
    creatures: Query<&Critter>,
    players: Query<&Transform, With<Player>>,
    mut seed: Local<u32>,
) {
    for PrayerLanded { effect, targets, .. } in landed.read() {
        if effect != "smite" {
            continue;
        }
        *seed = seed.wrapping_add(1);
        let mut play = PlayEffect::new("smite", *seed);
        for &target in targets {
            let height = creatures.get(target).map_or(17.0, height_of);
            play = play.on(target, height);
        }
        plays.write(play);
    }
    for blow in struck.read() {
        let Ok(me) = players.get(blow.caster) else { continue };
        let away = (blow.from - me.translation.xz()).normalize_or_zero();
        *seed = seed.wrapping_add(1);
        // Toward the blow and a little ahead, so it shows at that edge of the view and not behind it.
        let ahead = me.forward().as_vec3().with_y(0.0).normalize_or_zero();
        plays.write(PlayEffect::new(SHIELD_STRUCK_EFFECT, *seed).at(me.translation + Vec3::new(away.x, 0.0, away.y) * RIPPLE_REACH + ahead * RIPPLE_AHEAD));
    }
}

/// How many game seconds pass in a real one now: the sleeping pace while the delver sleeps, else
/// the clock's.
fn clock_ratio(tuning: Option<&Tuning>, asleep: Option<&Asleep>, rules: Option<&GameRules>) -> f64 {
    match (asleep.is_some_and(|a| a.0.is_some()), rules) {
        (true, Some(rules)) => rules.camp.sleep_clock_ratio,
        _ => tuning.map_or(1.0, |t| f64::from(t.clock.game_seconds_per_second)),
    }
}

/// How many game seconds before the shield ends its shimmer starts to sputter out: the effect's
/// sputter, which runs in real time, at the pace the game clock runs now. The clock holds still
/// with a screen open, so the shimmer holds with it and never ends while the shield stands.
fn sputter_lead(sputter_out: f32, ratio: f64) -> f64 {
    f64::from(sputter_out) * ratio
}

/// Lets the shimmer go with its shield: it sputters out as the shield nears its end, and at once
/// when the shield is gone or replaced.
#[allow(clippy::type_complexity)]
pub(crate) fn hold_the_shimmer(
    clock: Option<Res<GameClock>>,
    tuning: Option<Res<Tuning>>,
    asleep: Option<Res<Asleep>>,
    rules: Option<Res<GameRules>>,
    shields: Query<&ShieldPrayer>,
    mut shimmers: Query<&mut Playing>,
) {
    let now = clock.as_deref().map_or(0.0, GameClock::game_seconds);
    let ratio = clock_ratio(tuning.as_deref(), asleep.as_deref(), rules.as_deref());
    for mut shimmer in &mut shimmers {
        let Some(owner) = shimmer.owner() else { continue };
        if shimmer.name() != SHIELD_EFFECT {
            continue;
        }
        let ending = match shields.get(owner) {
            Ok(shield) => shield.expires - now <= sputter_lead(shimmer.sputter_out(), ratio),
            Err(_) => true,
        };
        if ending {
            shimmer.stop();
        }
    }
}

/// Starts the shimmer for each shield that has none, and starts it again if one goes missing
/// (pushed out by [`crate::effects::MAX_PLAYING`], or never started). A moment is left for a
/// requested one to appear, and none is started once the shield is on its last seconds.
pub(crate) fn start_the_shimmer(
    clock: Option<Res<GameClock>>,
    tuning: Option<Res<Tuning>>,
    asleep: Option<Res<Asleep>>,
    rules: Option<Res<GameRules>>,
    library: Option<Res<EffectLibrary>>,
    shields: Query<(Entity, &ShieldPrayer)>,
    shimmers: Query<&Playing>,
    mut plays: MessageWriter<PlayEffect>,
    mut waiting: Local<std::collections::HashMap<Entity, u8>>,
    mut seed: Local<u32>,
) {
    /// Frames a requested shimmer is given to appear before it is asked for again.
    const GRACE: u8 = 3;
    let now = clock.as_deref().map_or(0.0, GameClock::game_seconds);
    let ratio = clock_ratio(tuning.as_deref(), asleep.as_deref(), rules.as_deref());
    let sputter = library.as_deref().and_then(|l| l.get(SHIELD_EFFECT)).map_or(0.0, |e| e.sputter_out);
    waiting.retain(|shield, _| shields.contains(*shield));
    for (entity, shield) in &shields {
        if shimmers.iter().any(|p| p.owner() == Some(entity) && p.name() == SHIELD_EFFECT) {
            waiting.remove(&entity);
            continue;
        }
        if shield.expires - now <= sputter_lead(sputter, ratio) {
            continue;
        }
        let waited = waiting.entry(entity).or_insert(GRACE);
        if *waited >= GRACE {
            *waited = 0;
            *seed = seed.wrapping_add(1);
            plays.write(PlayEffect::new(SHIELD_EFFECT, *seed).owned_by(entity));
        } else {
            *waited += 1;
        }
    }
}

/// For captures (`FB_PRAY_BLOW=60,120`): that many frames after the cast, a blow lands on the
/// shielded delver from their left.
fn debug_blow(mut frames: Local<u32>, mut list: Local<Option<Vec<u32>>>, shields: Query<&ShieldPrayer>, players: Query<(Entity, &Transform), With<Player>>, mut struck: MessageWriter<ShieldStruck>) {
    *frames += 1;
    let list = list.get_or_insert_with(|| std::env::var("FB_PRAY_BLOW").map(|v| v.split(',').filter_map(|n| n.trim().parse().ok()).collect()).unwrap_or_default());
    if list.is_empty() {
        return;
    }
    let Some((caster, me)) = players.iter().next() else { return };
    if shields.is_empty() {
        *frames = 0;
        return;
    }
    if list.contains(&*frames) {
        let left = me.left().as_vec3().xz();
        struck.write(ShieldStruck { caster, from: me.translation.xz() + left * 30.0, damage: 3 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casting::{CastRequest, DelverSpellbook};
    use crate::character::{Character, Class, DelverMana};
    use crate::combat::feedback::damage_to;
    use crate::inventory::Toast;
    use fb_character::Mana;
    use fb_character::casting::Spellbook;
    use fb_creatures::Species;

    fn delve(mana: f64) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<GameRules>().init_resource::<Toast>().insert_resource(RunDice(procgen_rng::Mulberry32::new(7))).add_message::<CastRequest>().add_message::<Casted>().add_message::<Struck>();
        crate::prayers::register(&mut app);
        app.add_systems(Update, cast_requests.before(smite_the_seen));
        let mut character = Character::roll(1);
        character.class = Class::Cleric;
        app.insert_resource(character);
        let book = DelverSpellbook(Spellbook::starting(fb_character::Casting::Divine));
        let player = app.world_mut().spawn((Player, Transform::default(), book, DelverMana(Mana { points: mana, max: 20.0 }))).id();
        (app, player)
    }

    fn creature(app: &mut App, species: Species, at: Vec3, seed: u32) -> Entity {
        let spot = fb_level::CreatureSpot { x: 0.0, y: 0.0, yaw: 0.0, species, seed, pack: 0 };
        app.world_mut().spawn((Critter::new(spot, spot.creature()), Transform::from_translation(at))).id()
    }

    fn smite(app: &mut App, tier: u8) {
        app.world_mut().write_message(CastRequest { name: "Smite".into(), tier });
        app.update();
    }

    fn mana(app: &App, player: Entity) -> f64 {
        app.world().get::<DelverMana>(player).unwrap().0.points
    }

    fn blows(app: &App) -> Vec<Struck> {
        app.world().resource::<Messages<Struck>>().iter_current_update_messages().copied().collect()
    }

    /// The player faces -Z; 30 ft is about 91.5 world units.
    const AHEAD: Vec3 = Vec3::new(0.0, 0.0, -91.5);

    #[test]
    fn smite_at_tier_one_on_a_skeleton_spends_three_and_deals_the_dice_doubled() {
        let (mut app, player) = delve(10.0);
        let skeleton = creature(&mut app, Species::Skeleton, AHEAD, 1);
        smite(&mut app, 1);
        assert_eq!(mana(&app, player), 7.0, "smite 2 + at one you see 1 + instant 0");
        let blows = blows(&app);
        assert_eq!(blows.len(), 1);
        let roll = procgen_rng::Mulberry32::new(7).int(1, 8);
        let dealt = damage_to(app.world().get::<Critter>(skeleton).unwrap(), &blows[0]);
        assert_eq!((blows[0].target, dealt), (skeleton, roll * 2), "one d8 from the run's dice, doubled for the undead");
        let landed: Vec<_> = app.world().resource::<Messages<PrayerLanded>>().iter_current_update_messages().cloned().collect();
        assert_eq!(landed, vec![PrayerLanded { effect: "smite".into(), tier: 1, caster: player, targets: vec![skeleton] }]);
    }

    #[test]
    fn smite_on_a_rat_deals_single_dice_a_tier() {
        let (mut app, player) = delve(10.0);
        let rat = creature(&mut app, Species::Rat, AHEAD, 1);
        smite(&mut app, 2);
        assert_eq!(mana(&app, player), 4.0, "tier two costs twice three");
        let mut twin = procgen_rng::Mulberry32::new(7);
        let expected = twin.int(1, 8) + twin.int(1, 8);
        let blows = blows(&app);
        assert_eq!((blows[0].target, damage_to(app.world().get::<Critter>(rat).unwrap(), &blows[0])), (rat, expected));
    }

    #[test]
    fn nothing_in_sight_says_so_and_spends_nothing() {
        let (mut app, player) = delve(10.0);
        smite(&mut app, 1);
        assert_eq!(mana(&app, player), 10.0);
        assert!(blows(&app).is_empty());
        assert_eq!(app.world().resource::<Toast>().text, "No one is in sight");
        // Behind the cleric, out of range and a corpse are no one either.
        creature(&mut app, Species::Rat, Vec3::new(0.0, 0.0, 30.0), 1);
        creature(&mut app, Species::Rat, Vec3::new(0.0, 0.0, -300.0), 2);
        let dead = creature(&mut app, Species::Rat, AHEAD, 3);
        app.world_mut().entity_mut(dead).insert(Corpse { since: 0.0, fell: Transform::default() });
        smite(&mut app, 1);
        assert_eq!(mana(&app, player), 10.0);
        assert!(blows(&app).is_empty());
    }

    #[test]
    fn a_creature_behind_a_wall_is_not_a_target() {
        let (mut app, player) = delve(10.0);
        let level = fb_level::generate(1207, 1);
        let wall = (0..level.width as i32 * level.height as i32)
            .map(|i| (i % level.width as i32, i / level.width as i32))
            .find(|&(x, y)| !level.is_open(x, y) && level.is_open(x - 1, y) && level.is_open(x + 1, y))
            .expect("a wall with floor on both sides");
        let cell = fb_level::CELL;
        let me = Vec3::new((wall.0 - 1) as f32 * cell + cell / 2.0, 0.0, wall.1 as f32 * cell + cell / 2.0);
        let there = Vec3::new((wall.0 + 1) as f32 * cell + cell / 2.0, 0.0, me.z);
        app.insert_resource(crate::play::CurrentLevel(level)).init_resource::<crate::play::BlockedCells>();
        // Face +X, towards the rat on the far side of the wall.
        app.world_mut().entity_mut(player).insert(Transform::from_translation(me).looking_to(Vec3::X, Vec3::Y));
        creature(&mut app, Species::Rat, there, 1);
        smite(&mut app, 3);
        assert_eq!(mana(&app, player), 10.0, "nothing spent");
        assert!(blows(&app).is_empty());
        assert_eq!(app.world().resource::<Toast>().text, "No one is in sight");
    }

    #[test]
    fn the_creature_nearest_the_crosshair_is_the_one_struck() {
        let (mut app, _) = delve(10.0);
        // The closer one is well off to the side; the farther one is straight ahead.
        let near_aside = creature(&mut app, Species::Rat, Vec3::new(30.0, 0.0, -50.0), 1);
        let far_ahead = creature(&mut app, Species::Rat, Vec3::new(0.0, 0.0, -150.0), 2);
        smite(&mut app, 1);
        let blows = blows(&app);
        assert_eq!(blows.len(), 1, "one target");
        assert_eq!(blows[0].target, far_ahead);
        assert_ne!(blows[0].target, near_aside);
    }

    fn shielded() -> (App, Entity, i32) {
        let (mut app, player) = delve(20.0);
        app.init_resource::<GameClock>().add_message::<AttackOnDelver>().add_systems(Update, crate::combat::vitals::armour_the_delver.after(end_the_shield));
        app.update();
        let base = armour(&app, player);
        (app, player, base)
    }

    fn armour(app: &App, player: Entity) -> i32 {
        app.world().get::<crate::combat::ArmourClass>(player).expect("the delver carries their Armor Class").0
    }

    fn shield(app: &mut App, tier: u8) {
        app.world_mut().write_message(CastRequest { name: "Shield".into(), tier });
        app.update();
        app.update();
    }

    #[test]
    fn shield_at_tier_two_raises_armour_class_by_four_until_its_time_is_up() {
        let (mut app, player, base) = shielded();
        shield(&mut app, 2);
        assert_eq!(mana(&app, player), 14.0, "shield 2 + self 0 + short 1, twice");
        assert_eq!(armour(&app, player), base + 4);
        let shields: Vec<ShieldPrayer> = app.world_mut().query::<&ShieldPrayer>().iter(app.world()).copied().collect();
        let lasts = shields[0].lasts;
        let character = app.world().resource::<Character>();
        let inventory = crate::inventory::Inventory::default();
        assert_eq!((shields.len(), shields[0].bonus, lasts), (1, 4, 1200.0));
        let rules = fb_character::rules::defaults();
        assert_eq!(character.derive_prayed(rules, &inventory, None, shield_bonus(&shields)).armour_class, base + 4, "the sheet shows it");
        let line = character.armour_class_shielded(rules, &inventory, shield_bonus(&shields)).line();
        assert!(line.contains("Shield (+4)"), "and explains it: {line}");
        app.world_mut().resource_mut::<GameClock>().advance(lasts * 0.5);
        app.update();
        assert_eq!(armour(&app, player), base + 4, "still holding at half time");
        app.world_mut().resource_mut::<GameClock>().advance(lasts);
        app.update();
        app.update();
        assert_eq!(armour(&app, player), base, "back when the time is up");
        assert_eq!(app.world_mut().query::<&ShieldPrayer>().iter(app.world()).count(), 0);
    }

    #[test]
    fn recasting_shield_replaces_it_and_never_stacks() {
        let (mut app, player, base) = shielded();
        shield(&mut app, 1);
        assert_eq!(armour(&app, player), base + 2);
        shield(&mut app, 1);
        assert_eq!(armour(&app, player), base + 2, "the same tier again is the same shield");
        shield(&mut app, 3);
        assert_eq!(armour(&app, player), base + 6);
        shield(&mut app, 1);
        assert_eq!(armour(&app, player), base + 2, "the new cast replaces, even a lesser one");
        assert_eq!(app.world_mut().query::<&ShieldPrayer>().iter(app.world()).count(), 1);
    }

    #[test]
    fn a_blow_on_a_shielded_caster_is_for_the_ripple() {
        let (mut app, player, _) = shielded();
        let blow = AttackOnDelver { attacker: Entity::PLACEHOLDER, species: None, damage: 3, poison: 0, attacker_pos: Vec2::new(5.0, 0.0) };
        let ripples = |app: &App| app.world().resource::<Messages<ShieldStruck>>().iter_current_update_messages().copied().collect::<Vec<_>>();
        app.world_mut().write_message(blow);
        app.update();
        assert!(ripples(&app).is_empty(), "no shield, no ripple");
        shield(&mut app, 1);
        app.world_mut().write_message(blow);
        app.update();
        assert_eq!(ripples(&app), vec![ShieldStruck { caster: player, from: Vec2::new(5.0, 0.0), damage: 3 }]);
    }

    #[test]
    fn the_shield_reports_its_time_left() {
        let shield = ShieldPrayer { tier: 1, bonus: 2, expires: 700.0, lasts: 600.0 };
        assert_eq!((shield.time_left(100.0), shield.time_left(400.0), shield.time_left(900.0)), (1.0, 0.5, 0.0));
    }

    #[test]
    fn light_at_one_you_see_casts_with_no_one_there() {
        let (mut app, player) = delve(10.0);
        let mut glimmer = fb_character::casting::Written::divine_light();
        glimmer.name = "Glimmer".into();
        glimmer.reach = "sight".into();
        app.world_mut().get_mut::<DelverSpellbook>(player).unwrap().0.save(glimmer).unwrap();
        app.world_mut().write_message(CastRequest { name: "Glimmer".into(), tier: 1 });
        app.update();
        assert_eq!(mana(&app, player), 7.0, "light 1 + sight 1 + short 1 spent with no target");
    }

    #[test]
    fn smite_reaches_sixty_feet_a_tier_and_no_further() {
        let (mut app, player) = delve(20.0);
        // 100 ft is out of reach at tier one (60 ft) and in reach at tier two (120 ft).
        creature(&mut app, Species::Rat, Vec3::new(0.0, 0.0, -305.0), 1);
        smite(&mut app, 1);
        assert_eq!((mana(&app, player), blows(&app).len()), (20.0, 0));
        smite(&mut app, 2);
        assert_eq!((mana(&app, player), blows(&app).len()), (14.0, 1));
    }

    fn plays(app: &App) -> Vec<PlayEffect> {
        app.world().resource::<Messages<PlayEffect>>().iter_current_update_messages().cloned().collect()
    }

    #[test]
    fn smite_plays_its_pillar_on_the_creature_it_fell_on() {
        let (mut app, _) = delve(10.0);
        let target = creature(&mut app, Species::Skeleton, AHEAD, 1);
        smite(&mut app, 1);
        let sent = plays(&app);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].name, "smite");
        assert!(matches!(sent[0].targets.as_slice(), [crate::effects::EffectTarget::Entity { entity, height }] if *entity == target && *height > 0.0), "{:?}", sent[0].targets);
    }

    #[test]
    fn a_new_shield_starts_the_lasting_shimmer_it_owns() {
        let (mut app, _, _) = shielded();
        #[derive(Resource, Default)]
        struct Log(Vec<PlayEffect>);
        app.init_resource::<Log>().add_systems(Update, |mut asked: MessageReader<PlayEffect>, mut log: ResMut<Log>| log.0.extend(asked.read().cloned()));
        app.world_mut().write_message(CastRequest { name: "Shield".into(), tier: 1 });
        // Nothing here plays effects, so the shimmer is asked for again once its moment has passed.
        for _ in 0..5 {
            app.update();
        }
        let started: Vec<PlayEffect> = app.world().resource::<Log>().0.iter().filter(|p| p.name == "shield").cloned().collect();
        let owner = app.world_mut().query_filtered::<Entity, With<ShieldPrayer>>().single(app.world()).unwrap();
        assert_eq!(started.len(), 1, "once, and not again while it is given its moment to appear");
        assert_eq!(started[0].owner, Some(owner));
    }

    #[test]
    fn a_blow_on_the_shield_plays_the_ripple_toward_it() {
        let (mut app, player, _) = shielded();
        shield(&mut app, 1);
        app.world_mut().write_message(ShieldStruck { caster: player, from: Vec2::new(30.0, 0.0), damage: 2 });
        app.update();
        let ripple = plays(&app).into_iter().find(|p| p.name == "shield_struck").expect("a ripple");
        match ripple.targets.as_slice() {
            [crate::effects::EffectTarget::Point(at)] => assert!(at.x > 0.0, "toward the blow: {at}"),
            other => panic!("{other:?}"),
        }
    }
}
