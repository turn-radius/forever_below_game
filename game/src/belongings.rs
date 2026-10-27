//! What a slain character leaves behind, through one system.
//!
//! A character carries its [`Belongings`] from the moment it appears: a skeleton's outfit, a
//! troll's weapon, a jackalope's rare drop ([`fb_items::jackalope::jackalope_drop`]), a folk's
//! worn and held things. When it is dead for good ([`dead_for_good`]) and not yet [`Looted`],
//! [`drop_belongings`] takes the gear drawn on its bones off, sets each item down round where it
//! fell ([`set_down_around`]), marks it [`Looted`] and plays its species' fall sound where it has
//! one (the skeleton's bones in a heap).
//!
//! A body brought back from the world record is spawned [`Looted`], so it never drops: what it
//! carried is on the floor as the record has it. Folk-specific work that is not dropping (the
//! body drawn again bare) is in [`crate::folk_combat`].

use bevy::prelude::*;
use fb_items::Item;

use crate::combat::feedback::{Corpse, Looted, collapse, take_blows};
use crate::creatures::skeletons::{BoneSounds, WornGear, play_at};
use crate::creatures::trolls::Troll;
use crate::creatures::{Bones, Critter};
use crate::folk_combat::{collapse_folk, take_folk_blows};
use crate::inventory::{Inventory, ItemId};
use crate::inventory::underfoot::{FloorChanges, apply_floor_changes, lay_round, set_down_around};
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Obstacle, Obstacles};
use crate::player::blocked_by;
use crate::settings::GameSettings;
use crate::tuning::Tuning;

pub struct BelongingsPlugin;

impl Plugin for BelongingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tuning>().init_resource::<FloorChanges>().add_systems(Update, set_down_spill.before(apply_floor_changes).in_set(DelveSystems)).add_systems(
            Update,
            // After every system that lays a body down (a critter's fall, a folk's blow, a troll's
            // last regeneration) and before a folk is laid back, so the items fall where it fell.
            drop_belongings
                .after(collapse)
                .after(take_blows)
                .after(take_folk_blows)
                .after(crate::creatures::trolls::regenerate)
                .before(collapse_folk)
                .before(apply_floor_changes)
                .in_set(DelveSystems),
        );
    }
}

/// Sets down at the delver's feet what a load found no room for in its container
/// ([`Inventory::spill`]): an old quiver's twenty kinds of arrow in a few stack places. Each lies
/// round them, slid clear of walls and props as a body's things are ([`lay_round`]), through
/// [`FloorChange::SetDownAt`], so nothing a save held is lost.
#[allow(clippy::too_many_arguments)]
pub fn set_down_spill(
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    tuning: Res<Tuning>,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Option<Res<Obstacles>>,
    eye: Query<&GlobalTransform, With<crate::play::PlayerCamera>>,
) {
    if inventory.spill.is_empty() {
        return;
    }
    // The camera is a child of the delver, so only its global place is where they stand.
    let Ok(eye) = eye.single() else { return };
    let spilled: Vec<ItemId> = std::mem::take(&mut inventory.spill).into_iter().filter(|&id| inventory.item(id).is_some()).collect();
    let obstacles = obstacles.as_deref().cloned().unwrap_or_default();
    let blocked = blocked_by(&level.0, &blocked_cells);
    let feet = eye.translation().xz();
    lay_round(feet, spilled, feet.x.to_bits() ^ feet.y.to_bits(), &inventory, &mut changes, &blocked, &obstacles.0, tuning.view.floor_item_scale);
}

/// What a character leaves on the floor when it is dead for good, filled when it appears.
#[derive(Component, Clone, Debug, Default)]
pub struct Belongings {
    pub items: Vec<Item>,
    /// The seed that scatters them ([`set_down_around`]): the character's own.
    pub seed: u32,
    /// The radius of the circle the character blocked, if it blocks one: it no longer blocks it
    /// once down, so the things may fall in it.
    pub stood_in: Option<f32>,
}

impl Belongings {
    pub fn new(seed: u32, items: impl IntoIterator<Item = Item>) -> Belongings {
        // Pieces of one kind of arrow lie as stacks of ten, not as twenty things.
        Belongings { items: fb_items::stack_up(items), seed, stood_in: None }
    }

    /// These belongings of a character that blocked a circle of `radius`.
    pub fn standing_in(mut self, radius: f32) -> Belongings {
        self.stood_in = Some(radius);
        self
    }

    /// What `critter` carries on a level `depth` deep, or `None` for a creature that carries
    /// nothing: a skeleton's outfit and what it carries, a troll's weapon, a jackalope's drop.
    pub fn of(critter: &Critter, depth: u32) -> Option<Belongings> {
        let seed = critter.spot.seed;
        let species = critter.spot.species;
        if species.is_skeleton() {
            let kit = critter.kit.as_ref()?;
            Some(Belongings::new(seed, kit.outfit.worn.iter().map(|(_, item)| item).chain(&kit.outfit.goods).cloned()))
        } else if species.is_jackalope() {
            Some(Belongings::new(seed, fb_items::jackalope::jackalope_drop(seed, depth)))
        } else {
            critter.arms.as_ref().map(|arms| Belongings::new(seed, [arms.weapon.clone()]))
        }
    }
}

/// Whether a body is down for good: any body but a troll that will rise, which is not until it
/// is burned ([`Troll::spent`]).
pub fn dead_for_good(troll: Option<&Troll>) -> bool {
    troll.is_none_or(|troll| troll.spent)
}

/// Sets down the belongings of each body that is dead for good, round where it fell, once: the
/// gear drawn on its bones is taken off, its items are put on the floor as the items they are
/// (the floor machinery spawns and records them like any dropped item), and it is marked
/// [`Looted`] and plays its fall sound where its species has one.
///
/// It fires the frame the body falls, so one struck down just before the delver leaves has
/// already dropped into the floor the record captures (the floor changes it makes are applied
/// before [`crate::record::record_level_left`] reads the floor).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn drop_belongings(
    mut commands: Commands,
    tuning: Res<Tuning>,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Option<Res<Obstacles>>,
    settings: Option<Res<GameSettings>>,
    sounds: Option<Res<BoneSounds>>,
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    bodies: Query<(Entity, &Belongings, &Transform, Option<&Critter>, Option<&Troll>), (With<Corpse>, Without<Looted>)>,
    gear: Query<(Entity, &ChildOf), With<WornGear>>,
    bones: Query<&Bones>,
) {
    let obstacles = obstacles.as_deref().cloned().unwrap_or_default();
    let blocked = blocked_by(&level.0, &blocked_cells);
    let game = settings.as_deref().copied().unwrap_or_default();
    for (entity, belongings, at, critter, troll) in &bodies {
        if !dead_for_good(troll) {
            continue;
        }
        commands.entity(entity).insert(Looted);
        if let (Some(sounds), Some(critter)) = (&sounds, critter)
            && critter.spot.species.is_skeleton()
        {
            play_at(&mut commands, "bones fall", sounds.collapse.clone(), at.translation, &game, &tuning);
        }
        // Off the bones.
        let own: Vec<Entity> = bones.get(entity).map(|b| b.0.clone()).unwrap_or_default();
        for (piece, parent) in &gear {
            if own.contains(&parent.parent()) {
                commands.entity(piece).despawn();
            }
        }
        // Onto the floor, round the place it fell, and not round the circle it stood in.
        let others: Vec<Obstacle> = match belongings.stood_in {
            Some(radius) => obstacles
                .0
                .iter()
                .filter(|o| !matches!(o, Obstacle::Circle { centre, radius: r } if centre.distance(at.translation.xz()) < 0.5 * radius && *r == radius))
                .copied()
                .collect(),
            None => obstacles.0.clone(),
        };
        set_down_around(at.translation.xz(), belongings.items.iter(), belongings.seed, &mut inventory, &mut changes, &blocked, &others, tuning.view.floor_item_scale);
        info!("A body drops {} things", belongings.items.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creatures::skeletons::SkeletonKit;
    use crate::creatures::trolls::TrollArms;
    use crate::inventory::underfoot::FloorChange;
    use fb_creatures::{Creature, Species, SkeletonGear, TrollState};
    use fb_level::CreatureSpot;

    const DEPTH: u32 = 3;

    /// An app that runs only [`drop_belongings`].
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Tuning>()
            .init_resource::<BlockedCells>()
            .init_resource::<Inventory>()
            .init_resource::<FloorChanges>()
            .insert_resource(CurrentLevel(fb_level::generate(1207, DEPTH)))
            .add_systems(Update, drop_belongings);
        app
    }

    #[test]
    fn a_spill_lands_round_the_delver_not_round_their_eye_in_its_parent() {
        let mut app = app();
        app.add_systems(Update, set_down_spill);
        // The delver stands at (60, 40); the camera rides them at eye height, so its own transform
        // is near the origin and only its global one is where they are.
        let delver = app.world_mut().spawn((Transform::from_xyz(60.0, 0.0, 40.0), GlobalTransform::from_xyz(60.0, 0.0, 40.0))).id();
        let eye = app.world_mut().spawn((crate::play::PlayerCamera, Transform::from_xyz(0.0, 16.0, 0.0), GlobalTransform::from_xyz(60.0, 16.0, 40.0))).id();
        app.world_mut().entity_mut(delver).add_child(eye);
        let ids: Vec<ItemId> = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let ids: Vec<ItemId> = (1..4).map(|s| inventory.add(fb_items::roll_item("Arrow", s).unwrap())).collect();
            inventory.spill = ids.clone();
            ids
        };
        app.update();
        assert!(app.world().resource::<Inventory>().spill.is_empty());
        let spots: Vec<Vec2> = app
            .world()
            .resource::<FloorChanges>()
            .0
            .iter()
            .filter_map(|change| match change {
                FloorChange::SetDownAt(id, at, _) if ids.contains(id) => Some(*at * fb_level::CELL),
                _ => None,
            })
            .collect();
        assert_eq!(spots.len(), ids.len());
        for at in spots {
            assert!(at.distance(Vec2::new(60.0, 40.0)) < fb_level::CELL, "{at} is not at the delver's feet");
        }
    }

    fn spot(species: Species, seed: u32) -> CreatureSpot {
        CreatureSpot { x: 20.5, y: 20.5, yaw: 0.0, species, seed, pack: 0 }
    }

    /// `critter` lying as a body, with the belongings it spawns with, `looted` as given.
    fn body_of(critter: Critter, looted: bool) -> (App, Entity) {
        let mut app = app();
        let at = Transform::from_xyz(30.0, 0.0, 30.0);
        let id = app.world_mut().spawn((critter.clone(), Corpse { since: 30.0, fell: at }, at, Bones(Vec::new()))).id();
        if let Some(belongings) = Belongings::of(&critter, DEPTH) {
            app.world_mut().entity_mut(id).insert(belongings);
        }
        if looted {
            app.world_mut().entity_mut(id).insert(Looted);
        }
        (app, id)
    }

    fn creature_body(species: Species, seed: u32, looted: bool) -> (App, Entity) {
        let spot = spot(species, seed);
        body_of(Critter::new(spot, spot.creature()), looted)
    }

    fn set_down(app: &App) -> usize {
        app.world().resource::<FloorChanges>().0.len()
    }

    /// What lies on the floor: each thing's base, seed and rarity.
    fn things(app: &App) -> Vec<(String, u32, fb_items::Rarity)> {
        let inventory = app.world().resource::<Inventory>();
        let mut found: Vec<_> = app
            .world()
            .resource::<FloorChanges>()
            .0
            .iter()
            .filter_map(|change| match change {
                FloorChange::SetDownAt(id, ..) => inventory.item(*id).map(|i| (i.base.clone(), i.seed, i.rarity)),
                _ => None,
            })
            .collect();
        found.sort();
        found
    }

    // Skeletons.

    fn armed_skeleton(looted: bool) -> (App, Entity, usize) {
        let spot = spot(Species::Skeleton, 5);
        let Creature::Skeleton(state) = spot.creature() else { panic!("a skeleton") };
        let gear = SkeletonGear::roll(&state, fb_items::Odds::kit(1));
        let kit = SkeletonKit::armed(&state, &gear);
        // What it wears, and the quiver an archer carries (its arrows fall in it).
        let shed = kit.outfit.worn.len() + kit.outfit.goods.iter().filter(|i| i.base == "Quiver").count();
        let (app, id) = body_of(Critter::new(spot, spot.creature()).with_kit(kit), looted);
        (app, id, shed)
    }

    #[test]
    fn a_slain_skeleton_sets_everything_it_wore_down_once() {
        let (mut app, skeleton, shed) = armed_skeleton(false);
        assert!(shed > 0, "this skeleton wears something");
        app.update();
        assert_eq!(set_down(&app), shed);
        assert!(app.world().get::<Looted>(skeleton).is_some());
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(set_down(&app), shed, "and never again");
    }

    #[test]
    fn a_slain_skeleton_archer_leaves_its_arrows_in_its_quiver() {
        let odds = fb_items::Odds::kit(1);
        let seed = (0..400).find(|&s| fb_items::undead::skeleton_outfit(s, odds).item(fb_items::Slot::Mainhand).is_some_and(|w| w.base == "Bow")).expect("some skeleton holds a bow");
        let spot = spot(Species::Skeleton, seed);
        let Creature::Skeleton(state) = spot.creature() else { panic!("a skeleton") };
        let kit = SkeletonKit::armed(&state, &SkeletonGear::roll(&state, odds));
        let arrows: usize = kit.outfit.goods.iter().filter(|i| i.base == "Arrow").map(|i| i.quantity as usize).sum();
        assert!(arrows > 0, "an archer carries arrows");
        let (mut app, _) = body_of(Critter::new(spot, spot.creature()).with_kit(kit), false);
        app.update();
        let lying = things(&app);
        assert!(lying.iter().all(|(base, ..)| base != "Arrow"), "no arrow lies loose: {lying:?}");
        let inventory = app.world().resource::<Inventory>();
        let quiver = app.world().resource::<FloorChanges>().0.iter().find_map(|c| match c {
            FloorChange::SetDownAt(id, ..) if inventory.item(*id).is_some_and(|i| i.base == "Quiver") => Some(*id),
            _ => None,
        });
        assert_eq!(quiver.map(|q| inventory.pieces_in(q)), Some(arrows), "the quiver falls with every arrow in it");
    }

    #[test]
    fn a_skeleton_brought_back_from_the_record_drops_nothing() {
        let (mut app, ..) = armed_skeleton(true);
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(set_down(&app), 0);
    }

    // Trolls.

    /// A seed whose troll holds a weapon.
    fn armed_seed() -> u32 {
        (0..40).find(|&s| !fb_items::troll::troll_outfit(s, fb_items::Odds::kit(1)).worn.is_empty()).expect("some troll is armed")
    }

    /// A troll of `seed`, armed as it rolled, lying as a body with `spent` and `looted` as given.
    fn troll(seed: u32, spent: bool, looted: bool) -> (App, Entity) {
        let state = TrollState::roll(seed);
        let gear = SkeletonGear::roll_troll(&state, fb_items::Odds::kit(1));
        let mut critter = Critter::new(spot(Species::Troll, seed), Creature::Troll(state.clone()));
        if let Some(arms) = TrollArms::of(&gear) {
            critter = critter.with_arms(arms);
        }
        let (mut app, id) = body_of(critter, looted);
        let mut troll = Troll::new(&state);
        troll.spent = spent;
        app.world_mut().entity_mut(id).insert(troll);
        (app, id)
    }

    #[test]
    fn a_troll_dead_for_good_sets_its_weapon_down_once() {
        let (mut app, troll) = troll(armed_seed(), true, false);
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
        let (mut app, troll) = troll(armed_seed(), false, false);
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(set_down(&app), 0);
        assert!(app.world().get::<Looted>(troll).is_none());
        // Burned, it is dead for good, and the weapon falls.
        app.world_mut().get_mut::<Troll>(troll).unwrap().spent = true;
        app.update();
        assert_eq!(set_down(&app), 1);
    }

    #[test]
    fn a_troll_brought_back_from_the_record_drops_nothing() {
        // A restored body is spawned looted.
        let (mut app, _) = troll(armed_seed(), true, true);
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(set_down(&app), 0);
    }

    #[test]
    fn an_unarmed_troll_has_nothing_to_drop() {
        let seed = (0..40).find(|&s| fb_items::troll::troll_outfit(s, fb_items::Odds::kit(1)).worn.is_empty()).unwrap();
        let (mut app, troll) = troll(seed, true, false);
        app.update();
        assert_eq!(set_down(&app), 0);
        assert!(app.world().get::<Looted>(troll).is_none(), "nothing carried, so nothing to mark");
    }

    #[test]
    fn only_a_troll_not_yet_burned_is_not_dead_for_good() {
        let state = TrollState::roll(1);
        let mut troll = Troll::new(&state);
        assert!(!dead_for_good(Some(&troll)));
        troll.spent = true;
        assert!(dead_for_good(Some(&troll)));
        assert!(dead_for_good(None));
    }

    // Jackalopes.

    #[test]
    fn a_slain_jackalope_sets_its_drop_down_once() {
        let (mut app, jackalope) = creature_body(Species::Jackalope, 7, false);
        let wanted = fb_items::jackalope::jackalope_drop(7, DEPTH);
        assert!(!wanted.is_empty());
        app.update();
        assert_eq!(set_down(&app), wanted.len(), "what the roll gives falls where it fell");
        assert!(app.world().get::<Looted>(jackalope).is_some());
        assert!(things(&app).iter().all(|(.., rarity)| *rarity > fb_items::Rarity::Common), "never a common thing");
        for _ in 0..30 {
            app.update();
        }
        assert_eq!(set_down(&app), wanted.len(), "and never again");
    }

    #[test]
    fn the_same_jackalope_drops_the_same_things_every_time() {
        let drop = |seed| {
            let (mut app, _) = creature_body(Species::Jackalope, seed, false);
            app.update();
            things(&app)
        };
        assert_eq!(drop(11), drop(11));
        assert_ne!(drop(11), drop(12));
    }

    #[test]
    fn a_body_brought_back_from_the_record_and_other_creatures_drop_nothing() {
        let (mut restored, _) = creature_body(Species::Jackalope, 7, true);
        let (mut rat, _) = creature_body(Species::Rat, 7, false);
        for _ in 0..30 {
            restored.update();
            rat.update();
        }
        assert_eq!(set_down(&restored), 0);
        assert_eq!(set_down(&rat), 0);
    }

    // Folk, and anything else that carries belongings of its own.

    #[test]
    fn a_fallen_body_with_belongings_sets_them_down_once_wherever_it_came_from() {
        let mut app = app();
        let item = fb_items::jackalope::jackalope_drop(7, DEPTH).remove(0);
        let at = Transform::from_xyz(30.0, 0.0, 30.0);
        let body = app.world_mut().spawn((Belongings::new(9, [item]).standing_in(2.0), Corpse { since: 30.0, fell: at }, at)).id();
        app.update();
        assert_eq!(set_down(&app), 1);
        assert!(app.world().get::<Looted>(body).is_some());
        app.update();
        assert_eq!(set_down(&app), 1);
    }
}
