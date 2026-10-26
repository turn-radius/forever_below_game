//! What lies underfoot: the items on the floor within reach, laid out as a grid of their own,
//! each bag among them opening as its own grid too (`docs/items.md`, "Containers"). Dragging
//! from it into a bag picks the item up from the level; dragging out of a bag onto it, or off
//! the page onto the level, sets the item down in the world within reach.

use bevy::prelude::*;
use fb_items::footprint::Footprint;

use crate::carry::{FloorItem, REACH, lift, set_down, set_down_at};
use crate::play::{BlockedCells, CurrentLevel, Obstacles, PlayerCamera};
use crate::render::ItemSpawner;

use super::grid::Grid;
use super::items::{Inventory, ItemId};
use super::{Drag, GridView, Hovered, Target, drop_at};
use crate::keymap::{Action, ActionInput};

/// How far a shed item lands from where its wearer fell, world units, and the extra every
/// second one takes. Our own.
const SHED_RING: f32 = 2.6;
/// Salt for where shed items land. Our own.
const SHED_SALT: u32 = 0x5ED0_6EA8;

/// Sets each of `items` down round `centre` (world `(x, z)`) as a new loose item in `inventory`,
/// as the item it is: a ring of them, from an angle rolled from `seed`, each slid clear of
/// `blocked` cells and `obstacles` and turned at random, through [`FloorChange::SetDownAt`], which
/// the floor machinery spawns and records like any dropped item. What a fallen skeleton or folk
/// wore and held. `floor_scale` is the tuned size of things on the floor
/// (`view.floor_item_scale`).
#[allow(clippy::too_many_arguments)]
pub fn set_down_around<'a>(
    centre: Vec2,
    items: impl ExactSizeIterator<Item = &'a fb_items::Item>,
    seed: u32,
    inventory: &mut Inventory,
    changes: &mut FloorChanges,
    blocked: impl Fn(i32, i32) -> bool,
    obstacles: &[crate::play::Obstacle],
    floor_scale: f32,
) {
    use fb_level::CELL;
    let mut rng = procgen_rng::Mulberry32::new(seed ^ SHED_SALT);
    let ids: Vec<ItemId> = items.map(|item| inventory.add(item.clone())).collect();
    // What goes in a holder set down with it (an archer's arrows in its quiver) stays in it.
    let holders: Vec<ItemId> = ids
        .iter()
        .copied()
        .filter(|&id| {
            inventory
                .item(id)
                .and_then(|i| i.profile.holds.as_ref())
                .is_some_and(|g| !g.accepts.is_empty())
        })
        .collect();
    let loose: Vec<ItemId> = ids
        .into_iter()
        .filter(|&id| {
            let footprint = inventory
                .footprint(id)
                .unwrap_or(Footprint { cols: 1, rows: 1 });
            !holders.iter().any(|&holder| {
                holder != id
                    && inventory
                        .item(id)
                        .is_some_and(|item| inventory.accepts(holder, item))
                    && inventory
                        .grid(holder)
                        .and_then(|g| g.first_free(footprint))
                        .is_some_and(|at| inventory.put(id, holder, at).is_ok())
            })
        })
        .collect();
    let count = loose.len().max(1);
    let first = rng.range_f32(0.0, std::f32::consts::TAU);
    for (i, id) in loose.into_iter().enumerate() {
        let Some(item) = inventory.item(id) else {
            continue;
        };
        let radius = crate::carry::floor_radius(
            &crate::carry::item_points(&item.shape.build()),
            fb_items::profile::stands_on_base(&item.shape),
            floor_scale,
        )
        .min(CELL * 0.5 - 0.5);
        let angle = first + i as f32 / count as f32 * std::f32::consts::TAU;
        let want = centre + Vec2::from_angle(angle) * (SHED_RING + radius + 0.8 * (i % 2) as f32);
        let spot = crate::player::slide_around(centre, want, radius, &blocked, obstacles);
        changes.0.push(FloorChange::SetDownAt(
            id,
            spot / CELL,
            rng.range_f32(0.0, std::f32::consts::TAU),
        ));
    }
}

/// Columns of the underfoot grid, and the fewest rows it shows.
pub const UNDERFOOT_COLS: u8 = 6;
pub const UNDERFOOT_ROWS: u8 = 3;

/// The items on the floor within reach, nearest first, and the bags among them.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Underfoot {
    pub grid: Grid<ItemId>,
    pub bags: Vec<ItemId>,
}

impl Default for Underfoot {
    fn default() -> Self {
        Underfoot {
            grid: Grid::new(UNDERFOOT_COLS, UNDERFOOT_ROWS),
            bags: Vec::new(),
        }
    }
}

/// One floor item near the delver: its id, footprint, distance from the eye, and whether it
/// is a bag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nearby {
    pub id: ItemId,
    pub footprint: Footprint,
    pub distance: f32,
    pub bag: bool,
}

/// The underfoot layout of `nearby`: those within [`REACH`], nearest first, packed into the
/// grid at their first free spots ([`Grid::pack`]).
pub fn lay_out_underfoot(mut nearby: Vec<Nearby>) -> Underfoot {
    nearby.retain(|n| n.distance <= REACH);
    nearby.sort_by(|a, b| a.distance.total_cmp(&b.distance).then(a.id.cmp(&b.id)));
    Underfoot {
        grid: Grid::pack(
            UNDERFOOT_COLS,
            UNDERFOOT_ROWS,
            nearby.iter().map(|n| (n.id, n.footprint)),
        ),
        bags: nearby.iter().filter(|n| n.bag).map(|n| n.id).collect(),
    }
}

/// How far to the side of straight ahead something dropped on underfoot column `x` lands:
/// the grid's width spread across [`crate::carry::DROP_SPREAD`] either way, left to right.
pub fn aside_for_column(x: i32, cols: u8) -> f32 {
    let across = (x as f32 + 0.5) / cols.max(1) as f32;
    (across * 2.0 - 1.0).clamp(-1.0, 1.0) * crate::carry::DROP_SPREAD
}

/// Keeps [`Underfoot`] up to date while the screen is open.
pub fn track_underfoot(
    open: Res<crate::play::ScreenOpen>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    floor: Query<(&FloorItem, &GlobalTransform)>,
    inventory: Res<Inventory>,
    mut underfoot: ResMut<Underfoot>,
) {
    if !open.is(crate::play::Screen::Inventory) {
        return;
    }
    let Some(eye) = eyes.iter().next().map(GlobalTransform::translation) else {
        return;
    };
    let nearby = floor
        .iter()
        .filter_map(|(item, transform)| {
            let footprint = inventory.footprint(item.id)?;
            let centre = transform.transform_point((item.min + item.max) * 0.5);
            Some(Nearby {
                id: item.id,
                footprint,
                distance: centre.distance(eye),
                bag: inventory.grid(item.id).is_some(),
            })
        })
        .collect();
    underfoot.set_if_neq(lay_out_underfoot(nearby));
}

/// A change a drop makes to the level, beyond the inventory.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FloorChange {
    /// The item was lying in the level and has been taken up.
    Lifted(ItemId),
    /// The item has been let go, to land `aside` of straight ahead.
    SetDown(ItemId, f32),
    /// The item has been let go, to land at a spot on the floor (cells) turned `yaw`: where
    /// what took its place lay.
    SetDownAt(ItemId, Vec2, f32),
    /// A camp good already in its set-down form, put down in front of the eye without the checks
    /// of `crate::camp::lay_out`. For captures only.
    SetDownCamp(ItemId),
}

/// Changes to make to the level this frame.
#[derive(Resource, Debug, Default)]
pub struct FloorChanges(pub Vec<FloorChange>);

/// Takes lifted items out of the level and sets dropped ones down in it.
#[allow(clippy::too_many_arguments)]
pub fn apply_floor_changes(
    mut commands: Commands,
    mut changes: ResMut<FloorChanges>,
    floor: Query<(Entity, &FloorItem)>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    inventory: Res<Inventory>,
    mut level: ResMut<CurrentLevel>,
    blocked_cells: Res<BlockedCells>,
    obstacles: Option<Res<Obstacles>>,
    tuning: Option<Res<crate::tuning::Tuning>>,
    mut spawner: ItemSpawner,
) {
    if changes.0.is_empty() {
        return;
    }
    let tuning = tuning.as_deref().cloned().unwrap_or_default();
    // Into and out of the level data without respawning everything still there.
    let level = &mut level.bypass_change_detection().0;
    let obstacles = obstacles.as_deref().cloned().unwrap_or_default();
    for change in changes.0.drain(..) {
        match change {
            FloorChange::Lifted(id) => {
                if let Some((entity, item)) = floor.iter().find(|(_, f)| f.id == id) {
                    lift(&mut commands, level, entity, item);
                }
            }
            FloorChange::SetDown(id, aside) => {
                if let Some(eye) = eyes.iter().next() {
                    set_down(
                        &mut spawner,
                        &inventory,
                        level,
                        &blocked_cells,
                        &obstacles,
                        eye,
                        id,
                        aside,
                        &tuning,
                    );
                }
            }
            FloorChange::SetDownAt(id, at, yaw) => {
                set_down_at(&mut spawner, &inventory, level, id, at, yaw, &tuning);
            }
            FloorChange::SetDownCamp(id) => {
                if let Some(eye) = eyes.iter().next() {
                    crate::carry::set_camp_down_unchecked(
                        &mut spawner,
                        &inventory,
                        level,
                        eye,
                        id,
                        &tuning,
                    );
                }
            }
        }
    }
}

/// The drop key in the inventory: the carried or worn item under the mouse is set down underfoot
/// as dragging it onto the underfoot grid's first free spot does ([`drop_at`]), a container with
/// what it holds. With no free spot it is set down at the delver's feet, as dropping it on the
/// level does. Something already lying about, or nothing under the mouse, does nothing.
pub fn drop_hovered(
    input: ActionInput,
    hovered: Res<Hovered>,
    mut inventory: ResMut<Inventory>,
    underfoot: Res<Underfoot>,
    mut changes: ResMut<FloorChanges>,
) {
    let Some((id, _)) = hovered.0 else { return };
    if !input.just_pressed(Action::Drop) || !inventory.is_carried(id) {
        return;
    }
    let Some(size) = inventory.footprint(id) else {
        return;
    };
    let target = match underfoot.grid.first_free(size) {
        Some(at) => Target::Cell {
            view: GridView::Underfoot,
            cell: IVec2::new(at.x as i32, at.y as i32),
        },
        None => Target::Level,
    };
    let drag = Drag {
        id,
        grab: IVec2::ZERO,
        size,
        turned: false,
        press: Vec2::ZERO,
        active: true,
        target: None,
    };
    if let Ok(set_down) = drop_at(&mut inventory, &drag, target, underfoot.grid.cols) {
        changes.0.extend(set_down);
    }
}

/// For captures (`FB_DEBUG_KIT`): a few frames in, once the eye has settled, a sack holding a
/// couple of things and a loose thing beside it are set down in front of the delver.
pub fn set_down_debug_kit(
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    mut frames: Local<u32>,
) {
    *frames += 1;
    if *frames != 4 {
        return;
    }
    let Some(sack) = fb_items::roll_item("Sack", 41) else {
        return;
    };
    let sack = inventory.add(sack);
    for (base, seed) in [("Waterskin", 5), ("Rope", 9), ("Glowcap", 3)] {
        if let Some(item) = fb_items::roll_item(base, seed) {
            let id = inventory.add(item);
            let at = inventory.grid(sack).and_then(|g| {
                g.first_free(
                    inventory
                        .footprint(id)
                        .unwrap_or(Footprint { cols: 1, rows: 1 }),
                )
            });
            if at.is_none_or(|at| inventory.put(id, sack, at).is_err()) {
                inventory.forget(id);
            }
        }
    }
    changes.0.push(FloorChange::SetDown(sack, -2.5));
    if let Some(shield) = fb_items::roll_item("Shield", 12) {
        let shield = inventory.add(shield);
        changes.0.push(FloorChange::SetDown(shield, 3.0));
    }
}

/// The items `FB_DROP` asks for, as `base[:rarity]` separated by commas (`Sword:legendary`,
/// `Helm:rare,Ring`): for each, the first seed from 1 whose roll of that base, eight levels
/// down (where every rarity rolls), has that rarity (any rarity if none is named). Names that
/// roll nothing, or a rarity no seed gives, are skipped. An entry `@armorer/master/mail/8`
/// (`@profession/tier[/style[/depth]]`, style `-` for none) sets down the first three pieces of
/// that trader's stock, maker's line and all ([`stock_for_capture`]). An entry `Tent+set` or
/// `Bedroll+set` is that camp good already set down.
pub fn drops_for_capture(spec: &str) -> Vec<fb_items::Item> {
    spec.split(',')
        .flat_map(|entry| match entry.strip_prefix('@') {
            Some(stock) => stock_for_capture(stock),
            None => item_for_capture(entry).into_iter().collect(),
        })
        .collect()
}

/// The first three pieces of the stock `spec` names (`armorer/master/mail/8`), rolled for a
/// trader called Bera of the Ash Choir, seed 7.
fn stock_for_capture(spec: &str) -> Vec<fb_items::Item> {
    use fb_faction::{Profession, Style, Tier, Trade};
    let mut parts = spec.split('/');
    let key = |label: &str| label.replace(' ', "_");
    let (Some(profession), Some(tier)) = (parts.next(), parts.next()) else {
        return vec![];
    };
    let Some(profession) = Profession::TRADES
        .into_iter()
        .find(|p| key(p.label()).starts_with(profession))
    else {
        return vec![];
    };
    let Some(tier) = Tier::ALL.into_iter().find(|t| t.label() == tier) else {
        return vec![];
    };
    let style = parts.next().and_then(|name| {
        Style::ARMOUR
            .into_iter()
            .chain(Style::WEAPONS)
            .find(|s| key(s.label()) == name)
    });
    let depth = parts.next().and_then(|d| d.parse().ok()).unwrap_or(1);
    let Some(shop) = (Trade {
        profession,
        tier,
        style,
    })
    .shop("Bera", "the Ash Choir") else {
        return vec![];
    };
    fb_items::stock::stock(&shop, depth, 7)
        .into_iter()
        .take(3)
        .collect()
}

/// The item one `FB_DROP` entry names (`base[:rarity]`).
fn item_for_capture(entry: &str) -> Option<fb_items::Item> {
    use fb_items::Rarity;
    // `Tent+set` is the tent already pitched, a bedroll laid out (`crate::camp`).
    if let Some(base) = entry.strip_suffix("+set") {
        return item_for_capture(base).and_then(|item| fb_items::camp::set_up(&item));
    }
    let (base, rarity) = entry
        .split_once(':')
        .map_or((entry, None), |(b, r)| (b, Some(r.trim().to_lowercase())));
    let wanted = match rarity.as_deref() {
        None | Some("") => None,
        Some("common") => Some(Rarity::Common),
        Some("magic") => Some(Rarity::Magic),
        Some("rare") => Some(Rarity::Rare),
        Some("legendary") => Some(Rarity::Legendary),
        Some(other) => {
            warn!("FB_DROP: no rarity called {other:?}");
            return None;
        }
    };
    (1..20_000)
        .filter_map(|seed| fb_items::roll_item_with(base.trim(), seed, fb_items::Odds::loot(8)))
        .find(|item| wanted.is_none_or(|r| item.rarity == r))
}

/// For captures (`FB_DROP`): a few frames in, sets each item [`drops_for_capture`] names down
/// in front of the delver, spread from left to right.
pub fn set_down_for_capture(
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    mut frames: Local<u32>,
) {
    *frames += 1;
    if *frames != 4 {
        return;
    }
    let drops = drops_for_capture(&std::env::var("FB_DROP").unwrap_or_default());
    let count = drops.len();
    for (i, item) in drops.into_iter().enumerate() {
        let set_camp = item.is_set_camp();
        let id = inventory.add(item);
        let aside = if count > 1 {
            (i as f32 / (count - 1) as f32 * 2.0 - 1.0) * crate::carry::DROP_SPREAD
        } else {
            0.0
        };
        changes.0.push(if set_camp {
            FloorChange::SetDownCamp(id)
        } else {
            FloorChange::SetDown(id, aside)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(id: u32, cols: u8, rows: u8, distance: f32, bag: bool) -> Nearby {
        Nearby {
            id: ItemId(id),
            footprint: Footprint { cols, rows },
            distance,
            bag,
        }
    }

    #[test]
    fn only_what_is_in_reach_lies_underfoot_nearest_first() {
        let underfoot = lay_out_underfoot(vec![
            near(1, 1, 1, 15.0, false),
            near(2, 2, 2, 8.0, true),
            near(3, 1, 1, REACH + 1.0, false),
        ]);
        let ids: Vec<ItemId> = underfoot.grid.entries().iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![ItemId(2), ItemId(1)]);
        assert_eq!(underfoot.bags, vec![ItemId(2)]);
        assert_eq!(
            (underfoot.grid.cols, underfoot.grid.rows),
            (UNDERFOOT_COLS, UNDERFOOT_ROWS)
        );
        assert_eq!(lay_out_underfoot(vec![]), Underfoot::default());
    }

    use super::super::{Drag, GridView, Location, Placement, Target, drop_at};
    use crate::carry::DebugPose;
    use bevy::ecs::system::RunSystemOnce;
    use fb_items::Slot;

    /// An app with the level emptied of loot, a pack on the delver's back and their eye at the
    /// level's start, applying floor changes each update.
    fn floor_app() -> (App, ItemId, GlobalTransform) {
        let mut level = fb_level::generate(1207, 1);
        level.items.clear();
        let eye = GlobalTransform::from(DebugPose::parse(&level, "1").eye());
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.wear(pack, Slot::Back).unwrap();
        let mut app = App::new();
        app.insert_resource(CurrentLevel(level))
            .insert_resource(inventory)
            .init_resource::<FloorChanges>()
            .init_resource::<BlockedCells>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::render::ToonMaterial>>()
            .init_resource::<Assets<crate::render::OutlineMaterial>>()
            .init_resource::<crate::render::OutlineMaterials>()
            .add_systems(Update, apply_floor_changes);
        app.world_mut().spawn((PlayerCamera, eye));
        (app, pack, eye)
    }

    fn drag_of(inventory: &Inventory, id: ItemId) -> Drag {
        let size = inventory.footprint(id).unwrap();
        Drag {
            id,
            grab: IVec2::ZERO,
            size,
            turned: false,
            press: Vec2::ZERO,
            active: true,
            target: None,
        }
    }

    /// Lets `id` go at `target` as the screen does, then applies what that changes.
    fn drop(app: &mut App, id: ItemId, target: Target) {
        let change = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let drag = drag_of(&inventory, id);
            drop_at(&mut inventory, &drag, target, UNDERFOOT_COLS).unwrap()
        };
        app.world_mut()
            .resource_mut::<FloorChanges>()
            .0
            .extend(change);
        app.update();
    }

    fn floor_items(app: &mut App) -> Vec<(Entity, ItemId, Transform)> {
        let mut floor = app.world_mut().query::<(Entity, &FloorItem, &Transform)>();
        floor
            .iter(app.world())
            .map(|(e, f, t)| (e, f.id, *t))
            .collect()
    }

    fn dagger() -> fb_items::Item {
        (1..500)
            .filter_map(|s| fb_items::roll_item("Dagger", s))
            .find(|d| d.profile.footprint == Footprint { cols: 1, rows: 2 })
            .unwrap()
    }

    #[test]
    fn dropping_places_it_within_reach() {
        let (mut app, pack, eye) = floor_app();
        let knife = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let knife = inventory.add(dagger());
            inventory
                .put(knife, pack, Placement::new(0, 0, false))
                .unwrap();
            knife
        };
        drop(
            &mut app,
            knife,
            Target::Cell {
                view: GridView::Underfoot,
                cell: IVec2::new(5, 1),
            },
        );
        assert_eq!(
            app.world().resource::<Inventory>().location(knife),
            Some(Location::Loose)
        );
        let floor = floor_items(&mut app);
        assert_eq!(floor.len(), 1);
        let (_, id, at) = floor[0];
        assert_eq!(id, knife);
        assert!(
            at.translation.distance(eye.translation()) < REACH,
            "{}",
            at.translation
        );
        // Dropped on the right of the grid, it lies to the right of straight ahead.
        let right = eye.right().as_vec3();
        assert!((at.translation - eye.translation()).dot(right) > 1.0);
        assert_eq!(app.world().resource::<CurrentLevel>().0.items.len(), 1);
        // Something already lying there stays put when let go underfoot again.
        drop(&mut app, knife, Target::Level);
        assert_eq!(floor_items(&mut app).len(), 1);
    }

    #[test]
    fn picking_up_through_the_grid_removes_it_from_the_floor() {
        let (mut app, pack, eye) = floor_app();
        let knife = app.world_mut().resource_mut::<Inventory>().add(dagger());
        app.world_mut()
            .run_system_once(
                move |mut spawner: ItemSpawner,
                      inventory: Res<Inventory>,
                      mut level: ResMut<CurrentLevel>,
                      blocked: Res<BlockedCells>| {
                    set_down(
                        &mut spawner,
                        &inventory,
                        &mut level.0,
                        &blocked,
                        &Obstacles::default(),
                        &eye,
                        knife,
                        0.0,
                        &crate::tuning::Tuning::default(),
                    );
                },
            )
            .unwrap();
        let (entity, ..) = floor_items(&mut app)[0];
        drop(
            &mut app,
            knife,
            Target::Cell {
                view: GridView::Container(pack),
                cell: IVec2::new(3, 2),
            },
        );
        assert!(app.world().get_entity(entity).is_err());
        assert!(floor_items(&mut app).is_empty());
        assert!(app.world().resource::<CurrentLevel>().0.items.is_empty());
        assert_eq!(
            app.world().resource::<Inventory>().location(knife),
            Some(Location::In {
                container: pack,
                at: Placement::new(3, 2, false)
            })
        );
    }

    #[test]
    fn a_set_down_bag_keeps_its_contents() {
        let (mut app, pack, eye) = floor_app();
        let (sack, knife) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let sack = inventory.add(fb_items::roll_item("Sack", 3).unwrap());
            inventory
                .put(sack, pack, Placement::new(0, 0, false))
                .unwrap();
            let knife = inventory.add(dagger());
            inventory
                .put(knife, sack, Placement::new(1, 1, false))
                .unwrap();
            (sack, knife)
        };
        drop(&mut app, sack, Target::Level);
        let inventory = app.world().resource::<Inventory>();
        assert_eq!(inventory.location(sack), Some(Location::Loose));
        assert_eq!(
            inventory.location(knife),
            Some(Location::In {
                container: sack,
                at: Placement::new(1, 1, false)
            })
        );
        assert!(!inventory.is_carried(knife));
        // It lies underfoot, and opens there as its own grid.
        let (_, id, at) = floor_items(&mut app)[0];
        let inventory = app.world().resource::<Inventory>();
        assert_eq!(id, sack);
        let nearby = Nearby {
            id,
            footprint: inventory.footprint(id).unwrap(),
            distance: at.translation.distance(eye.translation()),
            bag: true,
        };
        let underfoot = lay_out_underfoot(vec![nearby]);
        assert_eq!(underfoot.bags, vec![sack]);
        assert!(underfoot.grid.get(sack).is_some());
        // Picked up again, it still holds the knife.
        drop(
            &mut app,
            sack,
            Target::Cell {
                view: GridView::Container(pack),
                cell: IVec2::new(4, 0),
            },
        );
        let inventory = app.world().resource::<Inventory>();
        assert!(inventory.is_carried(knife));
        assert_eq!(inventory.contents_count(sack), 1);
    }

    #[test]
    fn a_torch_on_the_third_column_of_bare_stone_lands_straight_ahead() {
        let (mut app, pack, eye) = floor_app();
        assert!(floor_items(&mut app).is_empty(), "an empty corridor");
        let torch = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let torch = inventory.add(fb_items::roll_item("Torch", 1).unwrap());
            inventory
                .put(torch, pack, Placement::new(0, 0, false))
                .unwrap();
            torch
        };
        drop(
            &mut app,
            torch,
            Target::Cell {
                view: GridView::Underfoot,
                cell: IVec2::new(2, 0),
            },
        );
        let floor = floor_items(&mut app);
        assert_eq!(floor.len(), 1);
        let to = floor[0].2.translation - eye.translation();
        let (ahead, aside) = (
            to.dot(eye.forward().as_vec3()),
            to.dot(eye.right().as_vec3()),
        );
        // In front, within a tenth of a metre of the line straight ahead.
        assert!(
            ahead > 1.0 && aside.abs() <= 1.0,
            "ahead {ahead}, aside {aside}"
        );
    }

    #[test]
    fn columns_spread_drops_left_to_right() {
        assert!(aside_for_column(0, 6) < 0.0 && aside_for_column(5, 6) > 0.0);
        assert!(aside_for_column(0, 6).abs() <= crate::carry::DROP_SPREAD);
        assert!(
            (aside_for_column(2, 6) + aside_for_column(3, 6)).abs() < 1e-5,
            "symmetric about the middle"
        );
    }

    /// The floor app with the drop key wired in and the inventory open; `hover` sets the item
    /// under the mouse, and `press_x` presses the key.
    fn drop_key_app() -> (App, ItemId, GlobalTransform) {
        let (mut app, pack, eye) = floor_app();
        app.init_resource::<crate::keymap::Keymap>()
            .init_resource::<Hovered>()
            .init_resource::<Underfoot>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(crate::keymap::ActiveContext(
                crate::keymap::Context::Inventory,
            ))
            .insert_resource(crate::play::ScreenOpen(Some(
                crate::play::Screen::Inventory,
            )))
            .add_systems(Update, drop_hovered.before(apply_floor_changes));
        (app, pack, eye)
    }

    fn hover(app: &mut App, id: ItemId) {
        app.world_mut().resource_mut::<Hovered>().0 = Some((id, Rect::default()));
    }

    fn press_x(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyX);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyX);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
    }

    #[test]
    fn the_drop_key_sets_the_hovered_torch_down_underfoot() {
        let (mut app, pack, eye) = drop_key_app();
        let torch = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let torch = inventory.add(fb_items::roll_item("Torch", 1).unwrap());
            inventory
                .put(torch, pack, Placement::new(0, 0, false))
                .unwrap();
            torch
        };
        press_x(&mut app);
        assert!(
            floor_items(&mut app).is_empty(),
            "nothing hovered, nothing dropped"
        );
        assert_eq!(
            app.world().resource::<Inventory>().location(torch),
            Some(Location::In {
                container: pack,
                at: Placement::new(0, 0, false)
            })
        );
        hover(&mut app, torch);
        press_x(&mut app);
        assert_eq!(
            app.world().resource::<Inventory>().location(torch),
            Some(Location::Loose)
        );
        let floor = floor_items(&mut app);
        assert_eq!(floor.len(), 1);
        assert_eq!(floor[0].1, torch);
        assert!(floor[0].2.translation.distance(eye.translation()) < REACH);
    }

    #[test]
    fn the_drop_key_keeps_a_sacks_contents() {
        let (mut app, pack, _) = drop_key_app();
        let (sack, knife) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let sack = inventory.add(fb_items::roll_item("Sack", 3).unwrap());
            inventory
                .put(sack, pack, Placement::new(0, 0, false))
                .unwrap();
            let knife = inventory.add(dagger());
            inventory
                .put(knife, sack, Placement::new(1, 1, false))
                .unwrap();
            (sack, knife)
        };
        hover(&mut app, sack);
        press_x(&mut app);
        let inventory = app.world().resource::<Inventory>();
        assert_eq!(inventory.location(sack), Some(Location::Loose));
        assert_eq!(
            inventory.location(knife),
            Some(Location::In {
                container: sack,
                at: Placement::new(1, 1, false)
            })
        );
        assert_eq!(floor_items(&mut app).len(), 1);
    }

    #[test]
    fn the_drop_key_takes_off_a_worn_item_and_sets_it_down() {
        let (mut app, _, _) = drop_key_app();
        let ring = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let ring = inventory.add(fb_items::roll_item("Ring", 3).unwrap());
            inventory.wear(ring, Slot::Ring1).unwrap();
            ring
        };
        hover(&mut app, ring);
        press_x(&mut app);
        assert_eq!(
            app.world().resource::<Inventory>().location(ring),
            Some(Location::Loose)
        );
        assert_eq!(floor_items(&mut app).len(), 1);
    }

    #[test]
    fn the_drop_key_sets_down_at_the_feet_when_underfoot_is_full() {
        let (mut app, pack, _) = drop_key_app();
        let torch = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let torch = inventory.add(fb_items::roll_item("Torch", 1).unwrap());
            inventory
                .put(torch, pack, Placement::new(0, 0, false))
                .unwrap();
            torch
        };
        let full = Grid::pack(
            UNDERFOOT_COLS,
            UNDERFOOT_ROWS,
            (100..100 + UNDERFOOT_COLS as u32 * UNDERFOOT_ROWS as u32)
                .map(|n| (ItemId(n), Footprint { cols: 1, rows: 1 })),
        );
        assert!(full.first_free(Footprint { cols: 1, rows: 1 }).is_none());
        app.world_mut().resource_mut::<Underfoot>().grid = full;
        hover(&mut app, torch);
        press_x(&mut app);
        assert_eq!(
            app.world().resource::<Inventory>().location(torch),
            Some(Location::Loose)
        );
        assert_eq!(floor_items(&mut app).len(), 1);
    }
}
