//! Trolls in the game: regenerating giants that rise again unless burned.
//!
//! A troll carries its measured joints and its [`Regeneration`] clock (`fb_creatures::troll`,
//! the rule itself, tested without Bevy) as a [`Troll`] on its root.
//!
//! - **Regaining.** Every [`TURN_SECONDS`](fb_creatures::troll::TURN_SECONDS) a living troll
//!   regains [`REGAIN`](fb_creatures::troll::REGAIN) hit points, up to its maximum.
//! - **Burning.** A blow from a lit torch counts as fire ([`crate::combat::swing::flaming_blow`]
//!   sends a [`Burned`] with it): the troll's next regeneration is stopped. A torch is held in
//!   either hand to be swung, and its flame reaches a troll lying down as well as one on its
//!   feet ([`crate::combat::swing::Strikeable::fallen_trolls`]).
//! - **Falling and rising.** At 0 hit points it falls like any creature ([`Corpse`], its mind
//!   gone). A turn after, unless it was burned since, it rises with the hit points it
//!   regenerated: its collapse is undone, its [`Corpse`] removed and a new mind given
//!   ([`brain_for`]) so it walks and fights again. Burned, it dies for good: its [`Troll`] is
//!   spent and the body stays where it lies.
//! - A body brought back from the record (a level left behind) is long past rising, and stays.
//! - **Arms.** About half of trolls hold a crude weapon ([`TrollArms`], `fb_items::troll`) in the
//!   right hand, fitted as a skeleton's is (`fb_creatures::SkeletonGear`). Dead for good, a troll
//!   sets it down on the floor once ([`crate::belongings::drop_belongings`]), as a skeleton sheds its gear.

use bevy::prelude::*;
use fb_creatures::pose::ArmStance;
use fb_creatures::skeleton::SkeletonDims;
use fb_creatures::troll::{Regeneration, Turn};
use fb_creatures::{Creature, Dice, SkeletonGear};
use fb_items::{Item, Slot};

use super::behaviour::brain_for;
use super::{Critter, Intent};
use crate::combat::Health;
use crate::combat::feedback::{Burned, Corpse, take_blows};
use crate::play::{CurrentLevel, DelveSystems};

/// What a troll's weapon adds to its damage: its Strength modifier, 4 (SRD 5.1 p. 351, "Claw:
/// 2d6 + 4"). The weapon hits at the stat block's attack bonus, +7, as the claw does.
pub const STRENGTH_BONUS: i32 = 4;

pub struct TrollsPlugin;

impl Plugin for TrollsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Burned>().add_systems(
            Update,
            (burn, fall, regenerate).chain().after(take_blows).in_set(DelveSystems),
        );
    }
}

/// On a troll: its measured joints (so nothing rebuilds its body each frame), its height, and
/// its regeneration.
#[derive(Component, Clone, Debug)]
pub struct Troll {
    pub dims: SkeletonDims,
    /// Crown to soles, stooped, world units.
    pub height: f32,
    pub regeneration: Regeneration,
    /// It died for good: burned while down, or long fallen. Nothing brings it back.
    pub spent: bool,
}

impl Troll {
    pub fn new(state: &fb_creatures::TrollState) -> Troll {
        Troll { dims: state.dims(), height: state.height(), regeneration: Regeneration::new(), spent: false }
    }
}

/// Fire on a troll stops its next regeneration.
fn burn(mut burned: MessageReader<Burned>, mut trolls: Query<&mut Troll>) {
    for blow in burned.read() {
        if let Ok(mut troll) = trolls.get_mut(blow.target) {
            troll.regeneration.burn();
            info!("The troll burns: it will not regenerate");
        }
    }
}

/// A troll just felled has a whole turn to wait before it may rise; a body that had already
/// lain a while when it appeared (from the record) never will.
fn fall(mut trolls: Query<(&mut Troll, &Corpse), Added<Corpse>>) {
    for (mut troll, corpse) in &mut trolls {
        if corpse.since > 1.0 {
            troll.spent = true;
        } else {
            troll.regeneration.fall();
        }
    }
}

/// What a troll's turn does to it, apart from rising: `health` regains what it regained. The
/// turn's outcome, if one ended.
pub fn take_turn(troll: &mut Troll, health: &mut Health, dt: f32) -> Option<Turn> {
    let turn = troll.regeneration.tick(dt, health.hp.max(0) as u32, health.max.max(0) as u32)?;
    match turn {
        Turn::Regained(n) => {
            health.heal(n);
        }
        Turn::Rose(n) => health.hp = n as i32,
        Turn::Died => troll.spent = true,
        Turn::Stopped => {}
    }
    Some(turn)
}

/// Each troll's turns: living ones regain, a fallen one rises or stays down.
#[allow(clippy::type_complexity)]
pub(crate) fn regenerate(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<CurrentLevel>,
    mut trolls: Query<(Entity, &Critter, &mut Troll, &mut Health, &mut Transform, &mut Intent, Option<&Corpse>)>,
) {
    for (entity, critter, mut troll, mut health, mut transform, mut intent, corpse) in &mut trolls {
        if troll.spent || !matches!(critter.creature, Creature::Troll(_)) {
            continue;
        }
        match take_turn(&mut troll, &mut health, time.delta_secs()) {
            Some(Turn::Rose(hp)) => {
                let Some(corpse) = corpse else { continue };
                // On its feet where it lay, collapse undone, with a mind again.
                *transform = corpse.fell;
                *intent = Intent::default();
                commands.entity(entity).remove::<Corpse>().insert(brain_for(&level.0, critter, corpse.fell.translation.xz()));
                info!("The troll rises with {hp} hit points");
            }
            Some(Turn::Died) => info!("The troll, burned, does not rise"),
            Some(Turn::Regained(n)) if n > 0 => debug!("The troll regains {n} hit points"),
            _ => {}
        }
    }
}

/// What a troll holds, worked out once when it appears: the weapon it rolled, the stance of the
/// arm that holds it (right then left) and the blow it deals in place of a claw.
#[derive(Clone, Debug)]
pub struct TrollArms {
    pub weapon: Item,
    pub stance: [Option<ArmStance>; 2],
    /// The weapon's dice and the troll's Strength modifier.
    pub blow: Dice,
}

impl TrollArms {
    /// The arms of a troll fitted with `gear`, or `None` for one with nothing to hold.
    pub fn of(gear: &SkeletonGear) -> Option<TrollArms> {
        let weapon = gear.outfit.item(Slot::Mainhand)?.clone();
        let dice = weapon.profile.stats.damage?;
        Some(TrollArms { blow: Dice::new(dice.count as u32, dice.sides as u32, STRENGTH_BONUS), stance: gear.stance, weapon })
    }

    /// The blows of one attack, in order: the bite, the weapon in place of the right claw, and
    /// the left claw.
    pub fn attack(&self, bite: Dice) -> [Dice; 3] {
        [bite, self.blow, fb_creatures::troll::CLAW]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_creatures::{Species, TrollState};
    use crate::creatures::skeletons::Looted;
    use crate::inventory::{FloorChanges, Inventory};

    /// A seed whose troll holds a weapon.
    fn armed_seed() -> u32 {
        (0..40).find(|&s| !fb_items::troll::troll_outfit(s, fb_items::Odds::kit(1)).worn.is_empty()).expect("some troll is armed")
    }

    #[test]
    fn an_armed_trolls_attack_is_a_bite_the_weapon_and_a_claw() {
        let seed = armed_seed();
        let gear = SkeletonGear::roll_troll(&TrollState::roll(seed), fb_items::Odds::kit(1));
        let arms = TrollArms::of(&gear).expect("armed");
        let bite = Species::Troll.stat_block().bite;
        let [first, second, third] = arms.attack(bite);
        assert_eq!(first, bite);
        let dice = gear.outfit.item(Slot::Mainhand).unwrap().profile.stats.damage.unwrap();
        assert_eq!(second, Dice::new(dice.count as u32, dice.sides as u32, 4), "the weapon's dice and the Strength modifier");
        assert_eq!(third, fb_creatures::troll::CLAW);
        assert_eq!(Species::Troll.stat_block().attack_bonus, 7);
    }

    // The troll in play: spawned with the whole of combat running, struck, burned and left.

    use crate::combat::feedback::Struck;
    use crate::combat::swing::Swing;
    use crate::combat::{CombatPlugin, RunDice, Weapon};
    use crate::creatures::behaviour::{AttackOnDelver, BehaviourPlugin, Brain, MissedDelver};
    use crate::creatures::{animate, idle, spawn_creature_with};
    use crate::inventory::underfoot::apply_floor_changes;
    use crate::play::Player;
    use bevy::time::TimeUpdateStrategy;
    use crate::character::Character;

    /// How far from the troll the delver stands, world units: a club's swing from its hide.
    const AWAY: f32 = 12.0;
    /// Out of its senses, and of its reach.
    const FAR: f32 = 600.0;

    /// The troll of `seed` in the level's start, in an app that runs the whole of combat and the
    /// trolls, stepped 1/60 s a frame, with a delver holding a torch (lit or not) `AWAY` from it
    /// and facing it. The app, the delver and the troll.
    fn troll_fight(seed: u32, torch_lit: bool, away: f32) -> (App, Entity, Entity) {
        let weak = |n: u128| Handle::<AudioSource>::Uuid(bevy::asset::uuid::Uuid::from_u128(n), std::marker::PhantomData);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin, bevy::state::app::StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_state(crate::launch::AppState::Delving)
            .init_resource::<crate::keymap::Keymap>()
            .init_resource::<Character>()
            .init_resource::<crate::play::ScreenOpen>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
            .init_resource::<crate::render::OutlineMaterials>()
            .init_resource::<FloorChanges>()
            .add_plugins((BehaviourPlugin, CombatPlugin, TrollsPlugin, crate::belongings::BelongingsPlugin))
            .insert_resource(RunDice(procgen_rng::Mulberry32::new(9)))
            .insert_resource(crate::combat::feedback::CreatureSounds { squeak: weak(1), hiss: weak(2), rat_squeal: weak(3), snake_rise: weak(4), thud: weak(5) })
            .add_systems(Update, (idle, animate, apply_floor_changes).chain());
        let mut level = fb_level::generate(1207, 1);
        level.items.clear();
        level.creatures.clear();
        let spot = crate::creatures::dev_spots_for(&level, &format!("troll:{seed}")).remove(0);
        app.insert_resource(CurrentLevel(level.clone()));
        let built = crate::creatures::build_creature(&spot, level.depth);
        let rig = built.gear.as_ref().map_or_else(|| built.creature.rig(), |g| g.rig.clone());
        let troll = {
            let mut commands = app.world_mut().commands();
            spawn_creature_with(&mut commands, &level, &spot, &rig, None, built.arms).0
        };
        app.world_mut().flush();
        let mut inventory = Inventory::default();
        let torch = inventory.add(fb_items::roll_item("Torch", 7).unwrap());
        inventory.wear(torch, Slot::Mainhand).unwrap();
        if torch_lit {
            inventory.toggle_light(torch).unwrap();
        }
        app.insert_resource(inventory);
        app.update();
        let at = app.world().get::<Transform>(troll).unwrap().translation;
        let from = at + Vec3::X * away;
        let player = app.world_mut().spawn((Player, Transform::from_translation(from).looking_at(at, Vec3::Y))).id();
        (app, player, troll)
    }

    fn strike_down(app: &mut App, troll: Entity) {
        for _ in 0..400 {
            if app.world().get::<Corpse>(troll).is_some() {
                return;
            }
            app.world_mut().write_message(Struck { target: troll, damage: 500, from: Vec2::new(0.0, 0.0), bludgeoning: true, critical: false, by_ally: false });
            app.update();
        }
        panic!("the troll never fell");
    }

    fn run(app: &mut App, seconds: f32) {
        for _ in 0..(seconds * 60.0) as usize {
            app.update();
        }
    }

    /// The delver swings what they hold at the troll in front of them and the blow lands.
    fn swing(app: &mut App, player: Entity, troll: Entity) {
        app.world_mut().entity_mut(troll).insert(crate::combat::ArmourClass(-100));
        let weapon = Weapon::in_hand(app.world().resource::<Inventory>(), app.world().resource::<Character>());
        app.world_mut().entity_mut(player).insert(Swing::new(weapon));
        run(app, 0.6);
    }

    fn health(app: &App, troll: Entity) -> i32 {
        app.world().get::<Health>(troll).unwrap().hp
    }

    #[test]
    fn a_troll_struck_down_rises_after_its_regeneration_with_its_mind_and_body_back() {
        let (mut app, player, troll) = troll_fight(unarmed_seed(), false, FAR);
        assert_eq!(health(&app, troll), 84, "the SRD's 84 hit points");
        strike_down(&mut app, troll);
        run(&mut app, 1.0);
        assert_eq!(health(&app, troll), 0);
        assert!(app.world().get::<Brain>(troll).is_none(), "a body has no mind");
        let lying = app.world().get::<Transform>(troll).unwrap().rotation * Vec3::Y;
        assert!(lying.y.abs() < 0.3, "laid on its back: {lying}");
        assert!(app.world().get::<Intent>(troll).unwrap().fallen > 0.9);
        // A whole turn after it fell, it rises.
        run(&mut app, 6.0);
        assert!(app.world().get::<Corpse>(troll).is_none(), "it got up");
        assert_eq!(health(&app, troll), 10, "with the hit points it regenerated");
        assert!(app.world().get::<Brain>(troll).is_some(), "with its mind back");
        let up = app.world().get::<Transform>(troll).unwrap().rotation * Vec3::Y;
        assert!(up.y > 0.95, "on its feet: {up}");
        assert_eq!(app.world().get::<Intent>(troll).unwrap().fallen, 0.0);
        assert!(!app.world().get::<Troll>(troll).unwrap().spent);
        // And it fights: with the delver brought to its side it goes for them.
        let at = app.world().get::<Transform>(troll).unwrap().translation;
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = at + Vec3::X * AWAY;
        // (It stops at its first blow: a delver it fells ends the run.)
        let mut blows = 0;
        for _ in 0..600 {
            app.update();
            blows = app.world().resource::<Messages<AttackOnDelver>>().iter_current_update_messages().count() + app.world().resource::<Messages<MissedDelver>>().iter_current_update_messages().count();
            if blows > 0 {
                break;
            }
        }
        assert_eq!(blows, 3, "a risen troll goes for the delver, a bite and two claws (or its weapon) in one swing");
        // And it can be struck down again.
        app.world_mut().get_mut::<Transform>(player).unwrap().translation = at + Vec3::X * FAR;
        strike_down(&mut app, troll);
        assert_eq!(health(&app, troll), 0);
    }

    /// A seed whose troll holds nothing.
    fn unarmed_seed() -> u32 {
        (0..40).find(|&s| fb_items::troll::troll_outfit(s, fb_items::Odds::kit(1)).worn.is_empty()).expect("some troll is unarmed")
    }

    #[test]
    fn a_lit_torch_blow_stops_the_next_regeneration() {
        let burned = |lit: bool| {
            let (mut app, player, troll) = troll_fight(unarmed_seed(), lit, AWAY);
            // Wounded first, so there is something to regain; the torch's blow does the little it does.
            app.world_mut().get_mut::<Health>(troll).unwrap().hp = 50;
            swing(&mut app, player, troll);
            let after_blow = health(&app, troll);
            assert!(after_blow <= 50, "the blow landed: {after_blow}");
            assert_eq!(app.world().get::<Troll>(troll).unwrap().regeneration.is_stopped(), lit, "lit: {lit}");
            // Out of its reach, so only its turn changes its health.
            app.world_mut().get_mut::<Transform>(player).unwrap().translation += Vec3::X * 400.0;
            run(&mut app, 6.5);
            health(&app, troll) - after_blow
        };
        assert_eq!(burned(false), 10, "a troll regains 10 each turn");
        assert_eq!(burned(true), 0, "burned, it regains nothing");
    }

    #[test]
    fn a_troll_burned_while_down_stays_dead_and_drops_its_weapon_once() {
        let seed = armed_seed();
        let (mut app, player, troll) = troll_fight(seed, true, AWAY);
        strike_down(&mut app, troll);
        run(&mut app, 1.5);
        // The delver puts the lit torch to the fallen troll.
        swing(&mut app, player, troll);
        assert!(app.world().get::<Troll>(troll).unwrap().regeneration.is_stopped(), "the fire caught it where it lay");
        run(&mut app, 6.5);
        assert!(app.world().get::<Corpse>(troll).is_some(), "it did not rise");
        assert!(app.world().get::<Troll>(troll).unwrap().spent, "dead for good");
        assert_eq!(health(&app, troll), 0);
        let on_the_floor = |app: &mut App| app.world_mut().query::<&crate::carry::FloorItem>().iter(app.world()).count();
        assert_eq!(on_the_floor(&mut app), 1, "its weapon lies where it fell");
        assert!(app.world().get::<Looted>(troll).is_some());
        run(&mut app, 20.0);
        assert!(app.world().get::<Corpse>(troll).is_some() && health(&app, troll) == 0);
        assert_eq!(on_the_floor(&mut app), 1, "set down once and never again");
    }

    #[test]
    fn a_troll_felled_and_left_to_burn_by_other_means_is_just_as_dead() {
        // The message itself, as any fire would send it.
        let (mut app, _, troll) = troll_fight(armed_seed(), false, FAR);
        strike_down(&mut app, troll);
        app.world_mut().write_message(Burned { target: troll });
        run(&mut app, 7.5);
        assert!(app.world().get::<Corpse>(troll).is_some() && app.world().get::<Troll>(troll).unwrap().spent);
    }
}
