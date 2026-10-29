//! Frighten and charm: the prayers a devotion unlocks. A favored foe unlocks bane and frighten, an
//! alignment unlocks charm ([`fb_character::casting::Spellbook::follow_devotion`]); switching
//! devotion locks again what the new one does not grant, though the prayers already written stay
//! (one with a locked term fails to cast and says so).
//!
//! A cast of either reaches the creatures its reach term names (around you: all within the
//! range; touch: the one in front; cone: those in front), and each gets a save: the bigger it is
//! the more often it resists, and a higher tier helps ([`fb_character::casting::InfluenceRules`],
//! rolled from [`RunDice`]). A creature taken is [`Frightened`] (it runs from the delver) or
//! [`Charmed`] (it ignores them until struck), for the cast's duration in game time. Folk and
//! denizens do not share the creature mind and are not touched yet.

use bevy::prelude::*;
use fb_character::combat::FEET_TO_UNITS;
use fb_creatures::Size;

use crate::casting::{Casted, DelverSpellbook};
use crate::character::Character;
use crate::combat::RunDice;
use crate::creatures::Critter;
use crate::creatures::behaviour::{Aroused, Charmed, Frightened};
use crate::inventory::Toast;
use crate::play::{DelveSystems, Player};
use crate::rules::GameRules;
use crate::tuning::Tuning;

/// The cosine of the half angle of the front a cone and a touch reach.
const CONE_COS: f32 = 0.8;
const TOUCH_COS: f32 = 0.6;

pub fn register(app: &mut App) {
    app.add_systems(Update, (follow_the_devotion, influence_creatures).chain().after(crate::casting::cast_requests).in_set(DelveSystems));
}

/// Keeps the book's devotion terms those the cleric's devotion grants.
fn follow_the_devotion(character: Option<Res<Character>>, rules: Res<GameRules>, mut books: Query<&mut DelverSpellbook, With<Player>>) {
    let Some(character) = character else { return };
    let devotion = character.devotion.filter(|_| character.class == crate::character::Class::Cleric);
    for mut book in &mut books {
        if !book.0.follows_devotion(devotion) {
            book.0.follow_devotion_with(&rules.0, devotion);
        }
    }
}

/// How many sizes above Tiny.
fn size_steps(size: Size) -> u8 {
    match size {
        Size::Tiny => 0,
        Size::Small => 1,
        Size::Medium => 2,
        Size::Large => 3,
    }
}

#[allow(clippy::too_many_arguments)]
fn influence_creatures(
    mut casts: MessageReader<Casted>,
    mut commands: Commands,
    rules: Res<GameRules>,
    tuning: Option<Res<Tuning>>,
    mut dice: ResMut<RunDice>,
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
    player: Query<&Transform, With<Player>>,
    creatures: Query<(Entity, &Critter, &Transform), (Without<Player>, Without<crate::combat::feedback::Corpse>)>,
    level: Option<Res<crate::play::CurrentLevel>>,
    blocked_cells: Option<Res<crate::play::BlockedCells>>,
    doors: Option<Res<crate::doors::Doors>>,
) {
    let now = time.map_or(0.0, |t| t.elapsed_secs());
    for Casted(cast) in casts.read() {
        let charm = match cast.written.effect.as_str() {
            "frighten" => false,
            "charm" => true,
            _ => continue,
        };
        let Some(me) = player.iter().next() else { continue };
        let (at, ahead) = (me.translation.xz(), me.forward().as_vec3().xz().normalize_or_zero());
        let range = cast.range as f32 * FEET_TO_UNITS;
        let front = match cast.written.reach.as_str() {
            "around" => -1.0,
            "touch" => TOUCH_COS,
            "cone" => CONE_COS,
            _ => {
                toast.say(format!("{} reaches no one", cast.written.name), now);
                continue;
            }
        };
        // Only what the cleric can see: a wall or a shut door stops a prayer.
        let cells = blocked_cells.as_deref().cloned().unwrap_or_default();
        let see = level.as_ref().map(|l| crate::doors::opaque_for(&l.0, &cells, doors.as_deref(), None));
        let seen = |p: Vec2| see.as_ref().is_none_or(|see| fb_level::path::line_of_sight((at.x / fb_level::CELL, at.y / fb_level::CELL), (p.x / fb_level::CELL, p.y / fb_level::CELL), see));
        let mut hit: Vec<(f32, Entity, &Critter)> = creatures
            .iter()
            .filter_map(|(e, c, t)| {
                let to = t.translation.xz() - at;
                let near = to.length();
                (near <= range + c.radius && seen(t.translation.xz()) && (front < 0.0 || to.normalize_or_zero().dot(ahead) >= front)).then_some((near, e, c))
            })
            .collect();
        hit.sort_by(|a, b| a.0.total_cmp(&b.0));
        if cast.written.reach == "touch" {
            hit.truncate(1);
        }
        // The duration is game seconds; the creatures count real ones.
        let seconds = cast.duration as f32 / tuning.as_ref().map_or(12.0, |t| t.clock.game_seconds_per_second).max(0.1);
        let (mut taken, mut resisted) = (Vec::new(), Vec::new());
        for (_, entity, critter) in hit {
            let block = critter.spot.species.stat_block();
            let name = block.name.to_lowercase();
            if rules.casting.influence.takes_hold(size_steps(block.size), cast.tier, dice.0.next_f64()) {
                if charm {
                    commands.entity(entity).remove::<Aroused>().insert(Charmed(seconds));
                } else {
                    commands.entity(entity).insert(Frightened(seconds));
                }
                taken.push(name);
            } else {
                resisted.push(name);
            }
        }
        let verb = if charm { "is charmed" } else { "flees" };
        let line = match (taken.as_slice(), resisted.as_slice()) {
            ([], []) => format!("{} finds no one", cast.written.name),
            ([one], []) => format!("The {one} {verb}"),
            ([], [one]) => format!("The {one} resists"),
            (taken, resisted) => format!("{} {}, {} resist", taken.len(), if charm { "charmed" } else { "flee" }, resisted.len()),
        };
        toast.say(line, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_character::casting::{Cast, Spellbook, compose};
    use fb_character::devotion::Devotion;
    use fb_character::kin::Kin;
    use fb_creatures::Species;
    use fb_items::Foe;

    fn app_with(devotion: Option<Devotion>) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<GameRules>().init_resource::<Toast>().insert_resource(RunDice(procgen_rng::Mulberry32::new(7))).add_message::<Casted>();
        let mut character = Character::roll(1);
        character.class = crate::character::Class::Cleric;
        character.devotion = devotion;
        app.insert_resource(character);
        let book = DelverSpellbook(Spellbook::starting(fb_character::Casting::Divine));
        let player = app.world_mut().spawn((Player, Transform::default(), book)).id();
        app.add_systems(Update, (follow_the_devotion, influence_creatures).chain());
        (app, player)
    }

    #[test]
    fn unlocking_follows_the_devotion_and_switching_relocks() {
        let (mut app, player) = app_with(Some(Devotion::FavoredFoe(Kin::Foe(Foe::Vermin))));
        app.update();
        let unlocked = |app: &App| ["bane", "frighten", "charm"].map(|t| app.world().get::<DelverSpellbook>(player).unwrap().0.unlocked.contains(t));
        assert_eq!(unlocked(&app), [true, true, false]);
        app.world_mut().resource_mut::<Character>().devotion = Some(Devotion::Aligned(Kin::Foe(Foe::Spider)));
        app.update();
        assert_eq!(unlocked(&app), [false, false, true]);
    }

    fn cast_at(app: &mut App, effect: &str, reach: &str, tier: u8) {
        let unlocked = ["bane", "frighten", "charm", "around", "touch", "short", "light", "self"].map(String::from).into();
        let written = compose(&unlocked, fb_character::Casting::Divine, "Test", effect, reach, "short").unwrap();
        let p = written.preview(tier);
        app.world_mut().write_message(Casted(Cast { written, tier, cost: p.cost, range: p.range, duration: p.duration }));
        app.update();
    }

    fn rat(app: &mut App, at: Vec3, seed: u32) -> Entity {
        let spot = fb_level::CreatureSpot { x: 0.0, y: 0.0, yaw: 0.0, species: Species::Rat, seed, pack: 0 };
        app.world_mut().spawn((Critter::new(spot, spot.creature()), Transform::from_translation(at))).id()
    }

    #[test]
    fn frighten_reaches_the_rats_in_range_and_charm_clears_a_blow() {
        let (mut app, _) = app_with(Some(Devotion::FavoredFoe(Kin::Foe(Foe::Vermin))));
        let near: Vec<Entity> = (0..20).map(|i| rat(&mut app, Vec3::new(5.0, 0.0, 0.0), i)).collect();
        let far = rat(&mut app, Vec3::new(500.0, 0.0, 0.0), 99);
        cast_at(&mut app, "frighten", "around", 3);
        let scared = near.iter().filter(|&&r| app.world().get::<Frightened>(r).is_some()).count();
        assert!(scared >= 15, "{scared} of 20 rats took fright at tier 3");
        assert!(app.world().get::<Frightened>(far).is_none());
        for &r in &near {
            app.world_mut().entity_mut(r).insert(Aroused);
        }
        cast_at(&mut app, "charm", "around", 3);
        let charmed = near.iter().filter(|&&r| app.world().get::<Charmed>(r).is_some()).count();
        assert!(charmed >= 15 && near.iter().all(|&r| app.world().get::<Charmed>(r).is_some() != app.world().entity(r).contains::<Aroused>()));
    }

    fn hit(app: &App, e: Entity) -> bool {
        app.world().get::<Frightened>(e).is_some()
    }

    #[test]
    fn a_cone_reaches_the_front_and_a_touch_only_the_nearest_ahead() {
        // The player faces -Z by default.
        let (mut app, _) = app_with(Some(Devotion::FavoredFoe(Kin::Foe(Foe::Vermin))));
        let ahead: Vec<Entity> = (0..12).map(|i| rat(&mut app, Vec3::new(0.0, 0.0, -10.0), i)).collect();
        let behind: Vec<Entity> = (0..12).map(|i| rat(&mut app, Vec3::new(0.0, 0.0, 10.0), 20 + i)).collect();
        app.world_mut().resource_mut::<GameRules>().casting.influence.base = 1.0;
        app.world_mut().resource_mut::<GameRules>().casting.influence.ceiling = 1.0;
        app.world_mut().resource_mut::<GameRules>().casting.influence.floor = 1.0;
        let unlocked = ["frighten", "cone", "short"].map(String::from).into();
        let written = compose(&unlocked, fb_character::Casting::Divine, "Test", "frighten", "cone", "short").unwrap();
        let p = written.preview(1);
        app.world_mut().write_message(Casted(Cast { written, tier: 1, cost: p.cost, range: p.range, duration: p.duration }));
        app.update();
        assert!(ahead.iter().all(|&r| hit(&app, r)), "the cone reaches ahead");
        assert!(behind.iter().all(|&r| !hit(&app, r)), "not behind");
        for &r in &ahead {
            app.world_mut().entity_mut(r).remove::<Frightened>();
        }
        let near = rat(&mut app, Vec3::new(0.0, 0.0, -4.0), 90);
        cast_at(&mut app, "frighten", "touch", 1);
        assert!(hit(&app, near));
        assert_eq!(ahead.iter().filter(|&&r| hit(&app, r)).count(), 0, "only the nearest");
    }

    #[test]
    fn a_corpse_is_spared() {
        let (mut app, _) = app_with(Some(Devotion::FavoredFoe(Kin::Foe(Foe::Vermin))));
        let dead = rat(&mut app, Vec3::new(5.0, 0.0, 0.0), 1);
        app.world_mut().entity_mut(dead).insert(crate::combat::feedback::Corpse { since: 0.0, fell: Transform::default() });
        app.world_mut().resource_mut::<GameRules>().casting.influence.floor = 1.0;
        app.world_mut().resource_mut::<GameRules>().casting.influence.ceiling = 1.0;
        cast_at(&mut app, "frighten", "around", 3);
        assert!(!hit(&app, dead));
    }

    #[test]
    fn a_rat_behind_a_wall_is_spared() {
        let (mut app, _) = app_with(Some(Devotion::FavoredFoe(Kin::Foe(Foe::Vermin))));
        let level = fb_level::generate(1207, 1);
        let wall = (0..level.width as i32 * level.height as i32)
            .map(|i| (i % level.width as i32, i / level.width as i32))
            .find(|&(x, y)| !level.is_open(x, y) && level.is_open(x - 1, y) && level.is_open(x + 1, y))
            .expect("a wall with floor on both sides");
        let cell = fb_level::CELL;
        let me = Vec3::new((wall.0 - 1) as f32 * cell + cell / 2.0, 0.0, wall.1 as f32 * cell + cell / 2.0);
        let there = Vec3::new((wall.0 + 1) as f32 * cell + cell / 2.0, 0.0, me.z);
        app.insert_resource(crate::play::CurrentLevel(level)).init_resource::<crate::play::BlockedCells>();
        app.world_mut().query::<&mut Transform>().iter_mut(app.world_mut()).for_each(|mut t| t.translation = me);
        let behind = rat(&mut app, there, 1);
        app.world_mut().resource_mut::<GameRules>().casting.influence.floor = 1.0;
        app.world_mut().resource_mut::<GameRules>().casting.influence.ceiling = 1.0;
        cast_at(&mut app, "frighten", "around", 3);
        assert!(!hit(&app, behind));
    }

    #[test]
    fn a_large_creature_resists_more_often_than_a_tiny_one() {
        let rules = &GameRules::default().casting.influence;
        let mut dice = procgen_rng::Mulberry32::new(11);
        let mut held = [0u32; 2];
        for _ in 0..2000 {
            for (i, size) in [Size::Tiny, Size::Large].into_iter().enumerate() {
                held[i] += u32::from(rules.takes_hold(size_steps(size), 1, dice.next_f64()));
            }
        }
        assert!(held[0] > held[1] + 800, "tiny {} large {}", held[0], held[1]);
    }
}
