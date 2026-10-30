//! The cleric's fighting prayers: Smite now, Shield next. Both are written from terms like any
//! prayer ([`fb_character::casting`]) and reacted to here from the [`Casted`] message.
//!
//! **Smite** (smite, at one you see, instant) falls on the creature nearest the crosshair within
//! the cast's range that the cleric can see: a wall or a shut door stops it, as it stops frighten
//! and charm. It deals a d8 radiant a tier ([`smite_damage`]), doubled against the undead, rolled
//! from [`RunDice`] and sent as a [`Struck`], the one path every blow of the delver's takes, so
//! hit points, callouts, kills, deeds and corpses work as for a sword. The pillar falls from
//! above, so it comes from the creature's own spot: no shield turns it and it pushes nothing.
//!
//! Finding the target is done twice: [`crate::casting::cast_requests`] asks [`Sight`] before the
//! mana is spent, so a cast with no one in sight costs nothing, and [`smite_the_seen`] asks again
//! when the cast has gone through. Both see the same floor in the same frame.
//!
//! **The visuals hook**: when a prayer's effect lands on creatures, [`PrayerLanded`] is sent with
//! the effect's name (`"smite"`), the caster and the creatures it fell on. The effect system plays
//! the pillar at their feet from it.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use fb_character::casting::smite_damage;
use fb_character::combat::FEET_TO_UNITS;
use fb_items::Foe;

use crate::casting::{Casted, cast_requests};
use crate::combat::RunDice;
use crate::combat::feedback::{Corpse, Struck};
use crate::creatures::Critter;
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

pub fn register(app: &mut App) {
    app.add_message::<PrayerLanded>().add_message::<Struck>().add_systems(Update, smite_the_seen.after(cast_requests).in_set(DelveSystems));
}

/// What the cleric can see of the creatures round them: the way [`crate::casting_influence`]
/// finds those in reach, a wall or a shut door blocking the line.
#[derive(SystemParam)]
pub struct Sight<'w, 's> {
    player: Query<'w, 's, (Entity, &'static Transform), With<Player>>,
    creatures: Query<'w, 's, (Entity, &'static Critter, &'static Transform), (Without<Player>, Without<Corpse>)>,
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
        // Radiant is taken as it is, like the fire of a torch: carried in `fire` so a skeleton's
        // weakness to bludgeoning never doubles it.
        struck.write(Struck { target, damage: 0, from: at.translation.xz(), bludgeoning: false, critical: false, by_ally: false, graze: false, fire: damage });
        landed.write(PrayerLanded { effect: "smite".into(), tier: cast.tier, caster, targets: vec![target] });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casting::{CastRequest, DelverSpellbook};
    use crate::character::{Character, Class, DelverMana};
    use crate::combat::feedback::damage_to;
    use crate::inventory::Toast;
    use crate::rules::GameRules;
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
}
