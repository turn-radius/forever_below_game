//! Trolls in the game: regenerating giants that rise again unless burned.
//!
//! A troll carries its measured joints and its [`Regeneration`] clock (`fb_creatures::troll`,
//! the rule itself, tested without Bevy) as a [`Troll`] on its root.
//!
//! - **Regaining.** Every [`TURN_SECONDS`](fb_creatures::troll::TURN_SECONDS) a living troll
//!   regains [`REGAIN`](fb_creatures::troll::REGAIN) hit points, up to its maximum.
//! - **Burning.** A blow from a lit torch counts as fire ([`crate::combat::swing::flaming_blow`]
//!   sends a [`Burned`] with it): the troll's next regeneration is stopped.
//! - **Falling and rising.** At 0 hit points it falls like any creature ([`Corpse`], its mind
//!   gone). A turn after, unless it was burned since, it rises with the hit points it
//!   regenerated: its collapse is undone, its [`Corpse`] removed and a new mind given
//!   ([`brain_for`]) so it walks and fights again. Burned, it dies for good: its [`Troll`] is
//!   spent and the body stays where it lies.
//! - A body brought back from the record (a level left behind) is long past rising, and stays.
//! - **Arms.** About half of trolls hold a crude weapon ([`TrollArms`], `fb_items::troll`) in the
//!   right hand, fitted as a skeleton's is (`fb_creatures::SkeletonGear`). Dead for good, a troll
//!   sets it down on the floor once ([`drop_weapon`]), as a skeleton sheds its gear.

use bevy::prelude::*;
use fb_creatures::pose::ArmStance;
use fb_creatures::skeleton::SkeletonDims;
use fb_creatures::troll::{Regeneration, Turn};
use fb_creatures::{Creature, Dice, SkeletonGear};
use fb_items::{Item, Slot};

use super::behaviour::brain_for;
use super::skeletons::{Looted, WornGear};
use super::{Bones, Critter, Intent};
use crate::combat::Health;
use crate::combat::feedback::{Burned, Corpse, take_blows};
use crate::inventory::Inventory;
use crate::inventory::underfoot::{FloorChanges, set_down_around};
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Obstacles};
use crate::player::blocked_by;
use crate::tuning::Tuning;

/// What a troll's weapon adds to its damage: its Strength modifier, 4 (SRD 5.1 p. 351, "Claw:
/// 2d6 + 4"). The weapon hits at the stat block's attack bonus, +7, as the claw does.
pub const STRENGTH_BONUS: i32 = 4;

pub struct TrollsPlugin;

impl Plugin for TrollsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Burned>().add_systems(
            Update,
            (burn, fall, regenerate, drop_weapon.before(crate::inventory::underfoot::apply_floor_changes)).chain().after(take_blows).in_set(DelveSystems),
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
fn regenerate(
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

/// Sets down the weapon of a troll dead for good, once, where it fell: taken off its hand and
/// put on the floor as the item it is ([`set_down_around`]). The troll is marked [`Looted`], and
/// a body brought back from the record is spawned [`Looted`] with empty hands, so the weapon is
/// never set down twice.
#[allow(clippy::too_many_arguments)]
fn drop_weapon(
    mut commands: Commands,
    tuning: Res<Tuning>,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Option<Res<Obstacles>>,
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    bodies: Query<(Entity, &Critter, &Troll, &Transform), (With<Corpse>, Without<Looted>)>,
    gear: Query<(Entity, &ChildOf), With<WornGear>>,
    bones: Query<&Bones>,
) {
    let obstacles = obstacles.as_deref().cloned().unwrap_or_default();
    let blocked = blocked_by(&level.0, &blocked_cells);
    for (entity, critter, troll, at) in &bodies {
        if !troll.spent {
            continue;
        }
        commands.entity(entity).insert(Looted);
        let Some(arms) = critter.arms.as_ref() else { continue };
        // Out of its hand.
        let own: Vec<Entity> = bones.get(entity).map(|b| b.0.clone()).unwrap_or_default();
        for (piece, parent) in &gear {
            if own.contains(&parent.parent()) {
                commands.entity(piece).despawn();
            }
        }
        // Onto the floor beside it.
        set_down_around(at.translation.xz(), std::iter::once(&arms.weapon), critter.spot.seed, &mut inventory, &mut changes, &blocked, &obstacles.0, tuning.view.floor_item_scale);
        info!("The troll's {} falls to the floor", arms.weapon.title());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_creatures::{Species, TrollState};
    use fb_level::CreatureSpot;

    /// A seed whose troll holds a weapon.
    fn armed_seed() -> u32 {
        (0..40).find(|&s| !fb_items::troll::troll_outfit(s, fb_items::Odds::kit(1)).worn.is_empty()).expect("some troll is armed")
    }

    /// A troll of `seed`, armed as it rolled, lying as a body with `spent` and `looted` as given,
    /// in an app that runs only [`drop_weapon`]. The app and the troll.
    fn body(seed: u32, spent: bool, looted: bool) -> (App, Entity) {
        let state = TrollState::roll(seed);
        let spot = CreatureSpot { x: 20.5, y: 20.5, yaw: 0.0, species: Species::Troll, seed, pack: 0 };
        let gear = SkeletonGear::roll_troll(&state, fb_items::Odds::kit(1));
        let mut critter = Critter::new(spot, Creature::Troll(state.clone()));
        if let Some(arms) = TrollArms::of(&gear) {
            critter = critter.with_arms(arms);
        }
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Tuning>()
            .init_resource::<BlockedCells>()
            .init_resource::<Inventory>()
            .init_resource::<FloorChanges>()
            .insert_resource(CurrentLevel(fb_level::generate(1207, 1)))
            .add_systems(Update, drop_weapon);
        let at = Transform::from_xyz(30.0, 0.0, 30.0);
        let mut troll = Troll::new(&state);
        troll.spent = spent;
        let id = app.world_mut().spawn((critter, troll, Corpse { since: 30.0, fell: at }, at, Bones(Vec::new()))).id();
        if looted {
            app.world_mut().entity_mut(id).insert(Looted);
        }
        (app, id)
    }

    fn set_down(app: &App) -> usize {
        app.world().resource::<FloorChanges>().0.len()
    }

    #[test]
    fn a_troll_dead_for_good_sets_its_weapon_down_once() {
        let (mut app, troll) = body(armed_seed(), true, false);
        app.update();
        assert_eq!(set_down(&app), 1, "the weapon falls");
        assert!(app.world().get::<Looted>(troll).is_some());
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(set_down(&app), 1, "and never again");
    }

    #[test]
    fn a_troll_that_will_rise_keeps_its_weapon() {
        let (mut app, _) = body(armed_seed(), false, false);
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(set_down(&app), 0);
    }

    #[test]
    fn a_troll_brought_back_from_the_record_drops_nothing() {
        // A restored body is spawned looted, and without its weapon in its hand.
        let (mut app, _) = body(armed_seed(), true, true);
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(set_down(&app), 0);
    }

    #[test]
    fn an_unarmed_troll_has_nothing_to_drop() {
        let seed = (0..40).find(|&s| fb_items::troll::troll_outfit(s, fb_items::Odds::kit(1)).worn.is_empty()).unwrap();
        let (mut app, troll) = body(seed, true, false);
        app.update();
        assert_eq!(set_down(&app), 0);
        assert!(app.world().get::<Looted>(troll).is_some());
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
}
