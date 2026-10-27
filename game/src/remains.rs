//! The gear of the dead, kept on their bodies until the delver takes it.
//!
//! Slain folk, denizens and the risen dead stay dressed in what they wore and held; nothing is
//! set on the floor as they fall ([`crate::belongings::drop_belongings`]). A fallen body keeps
//! its things in the inventory under a [`BodyStock`]: a grid-holding "Body" item
//! ([`crate::inventory::items::BODY_BASE`]) in the delver's own [`Inventory`], so the body's
//! things go where the delver drags them like those of any other container. The body's things
//! are its worn and held gear, each in its slot ([`BodyStock::kit`]), and what it carried.
//!
//! - **Search** ([`search_aimed`], [`prompt_search`]): Primary near a fallen body that still has
//!   something on it opens the inventory screen with the body's grid as a window beside the
//!   pack. The window is a bag window ([`crate::inventory::bags`]), so a worn bag in it (a
//!   satchel, a pack, a quiver) opens as a window of its own with a click, and its contents
//!   can be taken without taking the bag. The window stays while the delver is within
//!   [`BODY_REACH`] of the body ([`NearBodies`]) and closes when they walk away or the screen
//!   closes.
//! - **What is drawn** ([`keep_bodies_dressed`]): what is taken leaves the body's drawn kit at
//!   once. A folk is drawn again in what is left; a skeleton's gear pieces among its bones
//!   are taken off. A body with nothing left is [`Looted`], bare.
//! - **Persistence** ([`record_body`], [`restore_body`]): the level record keeps what is left
//!   on each body ([`BodyRecord`]), where each lay and in what slot it was worn, so leaving and
//!   coming back finds the body still dressed in what was not taken. A body looted bare comes
//!   back bare. A body recorded before bodies kept their things has them on the floor, as it
//!   was.

use bevy::prelude::*;
use fb_items::{Item, Slot};

use crate::combat::feedback::{Corpse, Looted};
use crate::creatures::skeletons::{WornGear, WornItem};
use crate::creatures::{Bones, Critter};
use crate::folk::{Denizen, Stance};
use crate::inventory::bags::BagWindows;
use crate::inventory::items::BODY_BASE;
use crate::inventory::{Grid, Inventory, ItemId, Location, Placement};
use crate::interact::{CrosshairPrompt, Delver, Target, key_line, verbs_for};
use crate::keymap::{Action, ActionInput, Keymap};
use crate::play::{DelveSystems, Player, Screen, ScreenOpen, show_screen};
use crate::record::{BodyRecord, BodyStowed, Stowed, record_item, restore_item};
use crate::render::ItemSpawner;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::window::{CursorOptions, PrimaryWindow};

/// How wide a body's grid is, cells.
pub const BODY_COLS: u8 = 8;
/// The fewest rows a body's grid has.
pub const BODY_MIN_ROWS: u8 = 3;
/// How near the delver must stand to a fallen body to search it, and to keep its windows open,
/// world units (2.2 m): an arm's reach and a stooping lean from its feet.
pub const BODY_REACH: f32 = 22.0;

pub struct RemainsPlugin;

impl Plugin for RemainsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NearBodies>().init_resource::<BagWindows>().add_systems(
            Update,
            (
                track_near_bodies,
                keep_bodies_dressed,
                search_aimed.run_if(crate::play::level_has_focus),
                prompt_search.after(crate::props::prompt_aimed_container),
                close_body_windows_with_screen,
            )
                .chain()
                .in_set(DelveSystems),
        );
        if std::env::var("FB_SLAY_NEAR").is_ok() {
            app.add_systems(Update, slay_near_for_capture.run_if(resource_exists::<crate::play::CurrentLevel>).in_set(DelveSystems));
        }
    }
}

/// On a fallen body that keeps its things: the container that stands for them in the
/// [`Inventory`], and which of them are drawn on the body.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct BodyStock {
    pub container: ItemId,
    /// What the body wore or held, by slot, that is still in the container.
    pub kit: Vec<(Slot, ItemId)>,
}

/// The containers of the fallen bodies within [`BODY_REACH`] of the delver, nearest first.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct NearBodies(pub Vec<ItemId>);

/// A body's container item: a chest in make, the base [`BODY_BASE`], with a grid `rows` deep
/// called `label`.
fn body_item(seed: u32, label: &str, rows: u8) -> Item {
    let mut item = fb_items::container_item(fb_level::chest_state(armory::families::chests::ChestStyle::Chest, seed, false));
    item.base = BODY_BASE.to_string();
    item.name = label.to_string();
    item.profile.holds = Some(fb_items::footprint::Grid { cols: BODY_COLS, rows: rows.max(BODY_MIN_ROWS), label: label.to_string(), accepts: Vec::new() });
    item
}

/// Puts each of `laid` into the grid of `container` where the layout has it; one that cannot go
/// there is forgotten.
fn lay_in(inventory: &mut Inventory, container: ItemId, laid: &Grid<ItemId>) {
    for entry in laid.entries() {
        if inventory.put(entry.id, container, entry.at).is_err() {
            inventory.forget(entry.id);
        }
    }
}

/// Keeps the things of a fallen body in `inventory`, in a grid called `label`: its worn and
/// held `worn`, and what it carried, `goods` (ammunition goes into a quiver or case it has,
/// as a stack). `None` for a body with nothing on it.
pub fn stock_body(inventory: &mut Inventory, seed: u32, label: &str, worn: Vec<(Slot, Item)>, goods: Vec<Item>) -> Option<BodyStock> {
    if worn.is_empty() && goods.is_empty() {
        return None;
    }
    let worn: Vec<(Slot, ItemId)> = worn.into_iter().map(|(slot, item)| (slot, inventory.add(item))).collect();
    let goods: Vec<ItemId> = goods.into_iter().map(|item| inventory.add(item)).collect();
    // What goes in a holder it wears or carries (an archer's arrows in its quiver) stays in it.
    let holders: Vec<ItemId> = worn.iter().map(|&(_, id)| id).chain(goods.iter().copied()).filter(|&id| inventory.item(id).and_then(|i| i.profile.holds.as_ref()).is_some_and(|g| !g.accepts.is_empty())).collect();
    let loose: Vec<ItemId> = worn
        .iter()
        .map(|&(_, id)| id)
        .chain(goods.iter().copied())
        .filter(|&id| {
            for &holder in &holders {
                if inventory.item(id).is_none() || inventory.location(id) != Some(Location::Loose) {
                    break;
                }
                if holder != id && inventory.item(id).is_some_and(|item| inventory.accepts(holder, item)) {
                    let _ = inventory.stow_in(id, holder);
                }
            }
            inventory.item(id).is_some() && inventory.location(id) == Some(Location::Loose)
        })
        .collect();
    // The grid is as deep as laying them out takes.
    let laid = Grid::<ItemId>::pack(BODY_COLS, BODY_MIN_ROWS, loose.iter().filter_map(|&id| Some((id, inventory.footprint(id)?))));
    let container = inventory.add(body_item(seed, label, laid.rows));
    lay_in(inventory, container, &laid);
    let kit = worn.into_iter().filter(|&(_, id)| matches!(inventory.location(id), Some(Location::In { container: c, .. }) if c == container)).collect();
    Some(BodyStock { container, kit })
}

/// What is left on the body, as the record keeps it; `None` for a body with nothing on it.
pub fn record_body(inventory: &Inventory, stock: &BodyStock) -> Option<BodyRecord> {
    let grid = inventory.grid(stock.container)?;
    let contents: Vec<BodyStowed> = grid
        .entries()
        .iter()
        .filter_map(|e| {
            let slot = stock.kit.iter().find(|(_, id)| *id == e.id).map(|(slot, _)| *slot);
            Some(BodyStowed { slot, stowed: Stowed { x: e.at.x, y: e.at.y, turned: e.at.turned, item: record_item(inventory, e.id)? } })
        })
        .collect();
    (!contents.is_empty()).then_some(BodyRecord { rows: grid.rows, contents })
}

/// Brings a body back as its record has it: its things in a new grid, each where it lay.
pub fn restore_body(inventory: &mut Inventory, record: &BodyRecord, seed: u32, label: &str) -> BodyStock {
    let container = inventory.add(body_item(seed, label, record.rows));
    let mut kit = Vec::new();
    for body in &record.contents {
        let Some(id) = restore_item(inventory, &body.stowed.item) else { continue };
        let at = Placement::new(body.stowed.x, body.stowed.y, body.stowed.turned);
        let placed = inventory.put(id, container, at).is_ok() || inventory.footprint(id).and_then(|f| inventory.grid(container)?.first_free(f)).is_some_and(|free| inventory.put(id, container, free).is_ok());
        if !placed {
            inventory.forget(id);
        } else if let Some(slot) = body.slot {
            kit.push((slot, id));
        }
    }
    BodyStock { container, kit }
}

/// What of a body's `kit` is drawn on it: a fitting hangs from the pack or belt that carries it
/// (`Slot::host`) and is drawn only while that is on the body and was rolled with the fitting. A
/// pack taken from a body takes its strap and lashing with it from the picture, as it does from
/// the delver ([`Inventory::unfit`]); what hung on it stays among the body's things to take.
pub fn drawn_kit(inventory: &Inventory, kit: &[(Slot, ItemId)]) -> Vec<(Slot, ItemId)> {
    kit.iter()
        .copied()
        .filter(|&(slot, _)| {
            let (Some(host), Some(needs)) = (slot.host(), slot.needs()) else { return true };
            kit.iter().any(|&(s, id)| s == host && inventory.item(id).is_some_and(|i| i.has_accessory(needs)))
        })
        .collect()
}

/// Every `(family, seed)` of what lies in `container` at any depth: what is still among a body's
/// things.
/// A fitting that no longer hangs from anything (in `kit` but not `drawn`) is left out.
fn keys_in(inventory: &Inventory, container: ItemId, drawn: &[(Slot, ItemId)], kit: &[(Slot, ItemId)], keys: &mut Vec<(armory::Family, u32)>) {
    let Some(grid) = inventory.grid(container) else { return };
    for entry in grid.entries() {
        if kit.iter().any(|(_, k)| *k == entry.id) && !drawn.iter().any(|(_, d)| *d == entry.id) {
            continue;
        }
        if let Some(item) = inventory.item(entry.id) {
            keys.push((item.shape.family(), item.shape.seed()));
        }
        keys_in(inventory, entry.id, drawn, kit, keys);
    }
}

/// Which bodies are within reach of the delver, nearest first.
pub fn track_near_bodies(players: Query<&Transform, With<Player>>, bodies: Query<(&Corpse, &BodyStock)>, mut near: ResMut<NearBodies>) {
    let Some(player) = players.iter().next() else {
        if !near.0.is_empty() {
            near.0.clear();
        }
        return;
    };
    let at = player.translation.xz();
    let mut found: Vec<(f32, ItemId)> = bodies.iter().map(|(corpse, stock)| (corpse.fell.translation.xz().distance(at), stock.container)).filter(|(d, _)| *d <= BODY_REACH).collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    let ids: Vec<ItemId> = found.into_iter().map(|(_, id)| id).collect();
    if near.0 != ids {
        near.0 = ids;
    }
}

/// Draws each fallen body in what is left on it. A thing taken from a body's grid is no longer
/// its own: a folk is drawn again without it, and the gear pieces of a skeleton that are no
/// longer among its things are taken off its bones. A body with nothing left is [`Looted`].
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn keep_bodies_dressed(
    mut commands: Commands,
    inventory: Res<Inventory>,
    mut bodies: Query<(Entity, &mut BodyStock, Option<&mut Denizen>, Option<&Bones>, Option<&Children>), (With<Corpse>, Without<Looted>)>,
    pieces: Query<(Entity, &ChildOf, &WornItem), With<WornGear>>,
    stances: Query<(), With<Stance>>,
    bindposes: Option<ResMut<Assets<SkinnedMeshInverseBindposes>>>,
    mut spawner: ItemSpawner,
) {
    if bodies.is_empty() {
        return;
    }
    let mut bindposes = bindposes;
    for (entity, mut stock, denizen, bones, children) in &mut bodies {
        let here = |id: ItemId| matches!(inventory.location(id), Some(Location::In { container, .. }) if container == stock.container);
        if stock.kit.iter().any(|&(_, id)| !here(id)) {
            let kit: Vec<(Slot, ItemId)> = stock.kit.iter().copied().filter(|&(_, id)| here(id)).collect();
            stock.kit = kit;
        }
        let drawn = drawn_kit(&inventory, &stock.kit);
        if let Some(mut denizen) = denizen {
            if denizen.person.gear.len() != drawn.len() {
                denizen.person.gear = drawn.iter().filter_map(|&(slot, id)| Some((slot, inventory.item(id)?.clone()))).collect();
                let kept: Vec<ItemId> = inventory.grid(stock.container).map(|g| g.entries().iter().map(|e| e.id).filter(|id| !drawn.iter().any(|(_, k)| k == id)).collect()).unwrap_or_default();
                denizen.person.goods = kept.into_iter().filter_map(|id| inventory.item(id).cloned()).collect();
                let stance = children.into_iter().flat_map(|c| c.iter()).find(|&c| stances.contains(c));
                if let (Some(stance), Some(bindposes)) = (stance, bindposes.as_deref_mut()) {
                    let figure = crate::figure::Figure::dress(&denizen.person.spec, &denizen.person.gear, denizen.person.dirt);
                    crate::folk_combat::redraw(&mut spawner, bindposes, stance, &figure, denizen.person.spot.seed);
                }
            }
        } else if let Some(bones) = bones {
            let mut keys = Vec::new();
            keys_in(&inventory, stock.container, &drawn, &stock.kit, &mut keys);
            for (piece, parent, worn) in &pieces {
                if bones.0.contains(&parent.parent()) && !keys.iter().any(|(family, seed)| *family == worn.family && *seed == worn.seed) {
                    commands.entity(piece).despawn();
                }
            }
        }
        if inventory.grid(stock.container).is_none_or(Grid::is_empty) {
            commands.entity(entity).try_insert(Looted);
        }
    }
}

/// The nearest fallen body in reach that has something on it, and whether the screen is shut.
fn searchable(near: &NearBodies, inventory: &Inventory) -> Option<ItemId> {
    near.0.iter().copied().find(|&id| inventory.grid(id).is_some_and(|g| !g.is_empty()))
}

/// Primary near a fallen body that still has something on it opens its inventory as a window
/// beside the pack, unless an item or container is under the crosshair.
#[allow(clippy::too_many_arguments)]
pub fn search_aimed(
    input: ActionInput,
    aimed: Option<Res<crate::carry::Aimed>>,
    near: Res<NearBodies>,
    inventory: Res<Inventory>,
    mut open: ResMut<ScreenOpen>,
    mut windows: ResMut<BagWindows>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
) {
    if !input.just_pressed(Action::Primary) || open.0.is_some() || aimed.is_some_and(|a| a.0.is_some()) {
        return;
    }
    let Some(body) = searchable(&near, &inventory) else { return };
    open_body(&mut open, &mut windows, body, cursor.as_deref_mut().map(|c| &mut **c));
}

/// Opens the inventory screen with `body`'s window in front.
pub fn open_body(open: &mut ScreenOpen, windows: &mut BagWindows, body: ItemId, cursor: Option<&mut CursorOptions>) {
    show_screen(open, Some(Screen::Inventory), cursor);
    if !windows.is_open(body) {
        windows.order.push(body);
    } else {
        windows.raise(body);
    }
}

/// Says Search over the crosshair while [`search_aimed`] would. Clears only its own line.
pub fn prompt_search(
    aimed: Option<Res<crate::carry::Aimed>>,
    near: Res<NearBodies>,
    inventory: Res<Inventory>,
    open: Res<ScreenOpen>,
    keymap: Option<Res<Keymap>>,
    mut prompt: ResMut<CrosshairPrompt>,
    mut said: Local<Option<String>>,
) {
    let line = (|| {
        if open.0.is_some() || aimed.is_some_and(|a| a.0.is_some()) {
            return None;
        }
        searchable(&near, &inventory)?;
        let fallback = Keymap::default();
        Some(key_line(keymap.as_deref().unwrap_or(&fallback), &verbs_for(&Target::Body, &Delver::default()))).filter(|line| !line.is_empty())
    })();
    if *said != line {
        if line.is_some() || prompt.0 == *said {
            prompt.0.clone_from(&line);
        }
        *said = line;
    } else if line.is_some() && prompt.0 != line {
        prompt.0.clone_from(&line);
    }
}

/// A body's windows close with the inventory screen, so the next time it opens it is the pack
/// and what is underfoot that show.
fn close_body_windows_with_screen(open: Res<ScreenOpen>, bodies: Query<&BodyStock>, mut windows: ResMut<BagWindows>) {
    if open.is(Screen::Inventory) || windows.order.is_empty() {
        return;
    }
    let mine: Vec<ItemId> = bodies.iter().map(|b| b.container).filter(|&c| windows.is_open(c)).collect();
    for id in mine {
        windows.close(id);
    }
}

/// For captures (`FB_SLAY_NEAR=1`): a few seconds in, strikes down the first folk or skeleton in
/// the level and stands the delver beside the body, facing it; with `FB_SEARCH_BODY=1` the
/// inventory then opens on its window, and with `FB_SEARCH_BODY=bag` its first bag opens too
/// (a satchel with something in it is stocked for the purpose).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn slay_near_for_capture(
    mut commands: Commands,
    time: Res<Time>,
    mut players: Query<&mut Transform, (With<Player>, Without<Denizen>, Without<Corpse>)>,
    bodies: Query<(Entity, &Transform), (Or<(With<Denizen>, With<Critter>)>, Without<Corpse>, Without<Player>)>,
    stocks: Query<(&Corpse, &BodyStock)>,
    mut inventory: ResMut<Inventory>,
    mut open: ResMut<ScreenOpen>,
    mut windows: ResMut<BagWindows>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
    mut state: Local<(bool, bool)>,
) {
    let Ok(mut player) = players.single_mut() else { return };
    if !state.0 {
        if time.elapsed_secs() < 3.0 {
            return;
        }
        let at = player.translation;
        let Some((entity, transform)) = bodies.iter().min_by(|a, b| a.1.translation.distance(at).total_cmp(&b.1.translation.distance(at))) else { return };
        commands.entity(entity).insert(Corpse { since: 0.0, fell: *transform }).remove::<(crate::play::BlocksTheWay, crate::folk_routine::Routine)>();
        state.0 = true;
        return;
    }
    let Some((corpse, stock)) = stocks.iter().next() else { return };
    if !state.1 {
        // Beside the body, looking down at it a stride off.
        let body = corpse.fell.translation;
        // Where the delver already looks, a stride before the body.
        let ahead = player.forward().as_vec3().with_y(0.0).normalize_or_zero();
        // Close enough to search when the inventory is to open, else far enough to see the body whole.
        let stride = if std::env::var("FB_SEARCH_BODY").is_ok() { 13.0 } else { 34.0 };
        let from = body - ahead * stride;
        player.translation = Vec3::new(from.x, player.translation.y, from.z);
        state.1 = true;
        return;
    }
    let want = std::env::var("FB_SEARCH_BODY").unwrap_or_default();
    if want.is_empty() || open.0.is_some() {
        return;
    }
    if want == "bag" {
        // A satchel the body wears gets a few things to take out.
        let bag = inventory.grid(stock.container).and_then(|g| g.entries().iter().map(|e| e.id).find(|&id| id != stock.container && inventory.grid(id).is_some_and(|b| b.is_empty()) && inventory.item(id).is_some_and(|i| i.profile.holds.as_ref().is_some_and(|h| h.accepts.is_empty()))));
        if let Some(bag) = bag {
            for base in ["Bread", "Healing vial", "Candle"] {
                if let Some(item) = fb_items::roll_item(base, 41) {
                    let id = inventory.add(item);
                    let _ = inventory.stow_in(id, bag);
                }
            }
            windows.order.push(bag);
        }
    }
    open_body(&mut open, &mut windows, stock.container, cursor.as_deref_mut().map(|c| &mut **c));
    // The body's window goes behind its bag's, which was queued first.
    state.1 = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::bags::place_windows;

    fn rolled(base: &str, seed: u32) -> Item {
        fb_items::roll_item(base, seed).unwrap_or_else(|| panic!("no {base}"))
    }

    /// A spear too long to pack held, a pack with a weapon strap and the spear in it, a satchel
    /// with a loaf in it, and a jerkin, on a body.
    fn goblin(inventory: &mut Inventory) -> BodyStock {
        let spear = (1..400).map(|s| rolled("Spear", s)).find(|s| !s.profile.packable).expect("a long spear");
        let pack = (1..400).map(|s| rolled("Pack", s)).find(|p| p.has_accessory(fb_items::Accessory::WeaponStrap)).expect("a pack with a strap");
        let satchel = rolled("Satchel", 3);
        let jerkin = rolled("Leather jerkin", 2);
        let stock = stock_body(inventory, 9, "Grub", vec![(Slot::Mainhand, spear.clone()), (Slot::Back, pack), (Slot::Strap, spear), (Slot::Satchel, satchel), (Slot::Body, jerkin)], vec![]).expect("a goblin carries things");
        let satchel = stock.kit.iter().find(|(s, _)| *s == Slot::Satchel).unwrap().1;
        let loaf = inventory.add(rolled("Bread", 1));
        inventory.stow_in(loaf, satchel).expect("room in the satchel");
        stock
    }

    #[test]
    fn a_body_keeps_everything_it_wore_even_a_spear_too_long_to_pack() {
        let mut inventory = Inventory::default();
        let stock = goblin(&mut inventory);
        assert_eq!(stock.kit.len(), 5, "all five are on the body, in their slots");
        for &(_, id) in &stock.kit {
            assert!(matches!(inventory.location(id), Some(Location::In { container, .. }) if container == stock.container));
        }
        assert!(inventory.is_body(stock.container));
        assert_eq!(stock_body(&mut inventory, 1, "Nobody", vec![], vec![]), None);
    }

    #[test]
    fn what_hangs_from_a_pack_is_drawn_only_while_the_pack_is_on_the_body() {
        let mut inventory = Inventory::default();
        let stock = goblin(&mut inventory);
        assert_eq!(drawn_kit(&inventory, &stock.kit).len(), 5);
        let pack = stock.kit.iter().find(|(s, _)| *s == Slot::Back).unwrap().1;
        inventory.take_out(pack);
        let kit: Vec<_> = stock.kit.iter().copied().filter(|&(_, id)| id != pack).collect();
        let drawn = drawn_kit(&inventory, &kit);
        assert!(!drawn.iter().any(|(s, _)| *s == Slot::Strap), "the strapped spear is not drawn without its pack");
        assert_eq!(drawn.len(), 3);
        let strapped = kit.iter().find(|(s, _)| *s == Slot::Strap).unwrap().1;
        assert!(matches!(inventory.location(strapped), Some(Location::In { container, .. }) if container == stock.container), "it is still among the body's things to take");
    }

    #[test]
    fn a_bodys_satchel_opens_and_its_contents_come_out_without_the_satchel() {
        let mut inventory = Inventory::default();
        let stock = goblin(&mut inventory);
        let satchel = stock.kit.iter().find(|(s, _)| *s == Slot::Satchel).unwrap().1;
        let loaf = inventory.grid(satchel).unwrap().entries()[0].id;
        assert!(crate::inventory::bags::reachable(&inventory, &[stock.container], None, satchel), "in reach with the body");
        assert!(!crate::inventory::bags::reachable(&inventory, &[], None, satchel));
        let sack = inventory.add(rolled("Sack", 4));
        inventory.wear(sack, Slot::Back).unwrap();
        inventory.put(loaf, sack, Placement::new(0, 0, false)).expect("into the delver's sack");
        assert!(inventory.is_carried(loaf));
        assert!(inventory.grid(satchel).unwrap().is_empty());
        assert!(matches!(inventory.location(satchel), Some(Location::In { container, .. }) if container == stock.container), "the satchel is still on the body");
    }

    #[test]
    fn a_body_is_recorded_with_what_is_left_and_comes_back_the_same() {
        let mut inventory = Inventory::default();
        let stock = goblin(&mut inventory);
        let jerkin = stock.kit.iter().find(|(s, _)| *s == Slot::Body).unwrap().1;
        inventory.take_out(jerkin);
        let kit: Vec<_> = stock.kit.iter().copied().filter(|&(_, id)| id != jerkin).collect();
        let stock = BodyStock { container: stock.container, kit };
        let record = record_body(&inventory, &stock).expect("things are left");
        let record: BodyRecord = serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
        assert_eq!(record.contents.len(), 4);
        let mut other = Inventory::default();
        let back = restore_body(&mut other, &record, 9, "Grub");
        assert_eq!(back.kit.len(), 4);
        for slot in [Slot::Mainhand, Slot::Back, Slot::Strap, Slot::Satchel] {
            assert!(back.kit.iter().any(|(s, _)| *s == slot), "{slot:?} is back in its slot");
        }
        assert!(!back.kit.iter().any(|(s, _)| *s == Slot::Body), "the jerkin was taken");
        let satchel = back.kit.iter().find(|(s, _)| *s == Slot::Satchel).unwrap().1;
        assert_eq!(other.contents_count(satchel), 1, "the satchel's loaf is still in it");
        assert_eq!(record_body(&other, &back), Some(record), "and it records the same");
        // Nothing left: nothing to record.
        let mut bare = Inventory::default();
        let empty = stock_body(&mut bare, 1, "Grub", vec![(Slot::Mainhand, rolled("Sword", 1))], vec![]).unwrap();
        bare.take_out(empty.kit[0].1);
        assert_eq!(record_body(&bare, &empty), None);
    }

    #[test]
    fn a_record_from_before_bodies_kept_their_gear_loads_with_none() {
        let old: crate::record::FolkRecord = serde_json::from_str(r#"{"hp":0,"max":5,"at":[1.0,0.0,2.0],"turn":[0.0,0.0,0.0,1.0],"hostile":false,"dead":true}"#).unwrap();
        assert!(old.dead && old.body.is_none());
    }

    fn window_app(inventory: Inventory) -> App {
        let mut app = App::new();
        app.init_resource::<BagWindows>()
            .init_resource::<crate::tuning::Tuning>()
            .init_resource::<crate::inventory::Underfoot>()
            .init_resource::<crate::inventory::OpenContainer>()
            .init_resource::<crate::barter::Barter>()
            .init_resource::<NearBodies>()
            .insert_resource(inventory)
            .add_systems(Update, place_windows);
        app
    }

    #[test]
    fn a_bodys_windows_stay_while_the_delver_is_near_and_close_when_they_walk_away() {
        let mut inventory = Inventory::default();
        let stock = goblin(&mut inventory);
        let satchel = stock.kit.iter().find(|(s, _)| *s == Slot::Satchel).unwrap().1;
        let mut app = window_app(inventory);
        app.world_mut().resource_mut::<BagWindows>().order = vec![stock.container, satchel];
        app.world_mut().resource_mut::<NearBodies>().0 = vec![stock.container];
        app.update();
        assert_eq!(app.world().resource::<BagWindows>().order, vec![stock.container, satchel], "near, both stay");
        app.world_mut().resource_mut::<NearBodies>().0.clear();
        app.update();
        assert!(app.world().resource::<BagWindows>().order.is_empty(), "walked away, both close");
    }

    #[test]
    fn only_a_fallen_body_within_reach_is_near() {
        let mut app = App::new();
        app.init_resource::<NearBodies>().add_systems(Update, track_near_bodies);
        let mut inventory = Inventory::default();
        let stock = goblin(&mut inventory);
        app.insert_resource(inventory);
        app.world_mut().spawn((Player, Transform::from_xyz(0.0, 0.0, 0.0)));
        let at = Transform::from_xyz(BODY_REACH - 2.0, 0.0, 0.0);
        let body = app.world_mut().spawn((Corpse { since: 30.0, fell: at }, at, stock.clone())).id();
        app.update();
        assert_eq!(app.world().resource::<NearBodies>().0, vec![stock.container]);
        let far = Transform::from_xyz(BODY_REACH + 20.0, 0.0, 0.0);
        app.world_mut().entity_mut(body).insert(Corpse { since: 30.0, fell: far });
        app.update();
        assert!(app.world().resource::<NearBodies>().0.is_empty());
        // The living carry no Corpse, so they are never near.
        app.world_mut().entity_mut(body).insert(Corpse { since: 30.0, fell: at }).remove::<Corpse>();
        app.update();
        assert!(app.world().resource::<NearBodies>().0.is_empty());
    }

    #[test]
    fn opening_a_body_shows_the_inventory_with_its_window_in_front() {
        let mut open = ScreenOpen::default();
        let mut windows = BagWindows::default();
        windows.order = vec![ItemId(7), ItemId(3)];
        open_body(&mut open, &mut windows, ItemId(7), None);
        assert!(open.is(Screen::Inventory));
        assert_eq!(windows.order, vec![ItemId(3), ItemId(7)]);
        open_body(&mut open, &mut windows, ItemId(9), None);
        assert_eq!(windows.order.last(), Some(&ItemId(9)));
    }
}
