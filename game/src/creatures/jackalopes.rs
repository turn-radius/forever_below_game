//! Jackalopes in the game: what a slain one leaves behind.
//!
//! The things it sheds ([`fb_items::jackalope::jackalope_drop`], a pure function of its seed and
//! the level's depth) are set down where it fell, once ([`set_down_around`]). It is marked
//! [`Looted`] as it does, and a body brought back from the record is spawned [`Looted`]
//! (`super::spawn_creatures`), so nothing is set down twice. How it flees and when it gores is in
//! [`super::behaviour`].

use bevy::prelude::*;

use super::Critter;
use super::skeletons::Looted;
use crate::combat::feedback::Corpse;
use crate::inventory::Inventory;
use crate::inventory::underfoot::{FloorChanges, apply_floor_changes, set_down_around};
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Obstacles};
use crate::player::blocked_by;
use crate::tuning::Tuning;

pub struct JackalopesPlugin;

impl Plugin for JackalopesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tuning>().init_resource::<FloorChanges>().add_systems(
            Update,
            drop_things.after(crate::combat::feedback::collapse).before(apply_floor_changes).in_set(DelveSystems),
        );
    }
}

/// Sets down what each slain jackalope sheds, around where it fell, and marks it [`Looted`].
#[allow(clippy::too_many_arguments)]
pub fn drop_things(
    mut commands: Commands,
    tuning: Res<Tuning>,
    level: Res<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Option<Res<Obstacles>>,
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    bodies: Query<(Entity, &Critter, &Transform), (With<Corpse>, Without<Looted>)>,
) {
    let obstacles = obstacles.as_deref().cloned().unwrap_or_default();
    let blocked = blocked_by(&level.0, &blocked_cells);
    for (entity, critter, at) in &bodies {
        if !critter.spot.species.is_jackalope() {
            continue;
        }
        commands.entity(entity).insert(Looted);
        let items = fb_items::jackalope::jackalope_drop(critter.spot.seed, level.0.depth);
        set_down_around(at.translation.xz(), items.iter(), critter.spot.seed, &mut inventory, &mut changes, &blocked, &obstacles.0, tuning.view.floor_item_scale);
        info!("The jackalope drops {} things", items.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::underfoot::FloorChange;
    use fb_creatures::Species;
    use fb_level::CreatureSpot;

    /// A creature of `species` and `seed` lying as a body, in an app that runs only
    /// [`drop_things`].
    fn body(species: Species, seed: u32, looted: bool) -> (App, Entity) {
        let spot = CreatureSpot { x: 20.5, y: 20.5, yaw: 0.0, species, seed, pack: 0 };
        let critter = Critter::new(spot, spot.creature());
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Tuning>()
            .init_resource::<BlockedCells>()
            .init_resource::<Inventory>()
            .init_resource::<FloorChanges>()
            .insert_resource(CurrentLevel(fb_level::generate(1207, 3)))
            .add_systems(Update, drop_things);
        let at = Transform::from_xyz(30.0, 0.0, 30.0);
        let id = app.world_mut().spawn((critter, Corpse { since: 0.0, fell: at }, at)).id();
        if looted {
            app.world_mut().entity_mut(id).insert(Looted);
        }
        (app, id)
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

    #[test]
    fn a_slain_jackalope_sets_its_drop_down_once() {
        let (mut app, jackalope) = body(Species::Jackalope, 7, false);
        let wanted = fb_items::jackalope::jackalope_drop(7, 3);
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
            let (mut app, _) = body(Species::Jackalope, seed, false);
            app.update();
            things(&app)
        };
        assert_eq!(drop(11), drop(11));
        assert_ne!(drop(11), drop(12));
    }

    #[test]
    fn a_body_brought_back_from_the_record_and_other_creatures_drop_nothing() {
        let (mut restored, _) = body(Species::Jackalope, 7, true);
        let (mut rat, _) = body(Species::Rat, 7, false);
        for _ in 0..30 {
            restored.update();
            rat.update();
        }
        assert_eq!(set_down(&restored), 0);
        assert_eq!(set_down(&rat), 0);
    }
}
