//! The quickslot bar: ten slots along the bottom of the view, running right from the orbs
//! ([`crate::combat::orbs`], [`crate::combat::eye`]).
//!
//! An item is bound to a slot by dragging it from the inventory onto the strip of slots the
//! inventory screen shows (the HUD bar itself hides under screens, like the rest of the HUD), or
//! by pressing the slot's key, 1 to 0, with the mouse over it in the inventory
//! ([`bind_hovered`]); both go through [`bind_dropped`], with the same refusals. A key over
//! something lying underfoot (or in a bag set down) picks it up first, into the first carried
//! place with room as a pickup would, and binds it ([`pick_up_and_bind`]).
//! The keys 1 to 0 then use the item in the slot ([`use_quickslot`]): food and drink are eaten
//! and drunk (`crate::eat`, a healing draught heals), a torch or lantern is drawn into the hand
//! and then lit or put out (`crate::kindle`), a flask of lamp oil fills the lanterns held or at
//! the hip (`crate::kindle::refill_and_say`), and a weapon or shield is drawn into the hand.
//!
//! A slot keeps pointing at its item until the item is used up or leaves the inventory
//! ([`empty_lost_slots`]). The inventory keeps no counts, so a stack of bread is several items:
//! when one is used up the slot moves to another of the same base that is carried, and empties
//! when the last is gone. It moves only to one bound to no other slot, so using one slot never
//! moves or empties another. Binding an item to a slot takes it from any other slot it was in.

use bevy::{prelude::*, text::FontSize};
use fb_items::{Item, Slot};

use crate::combat::orbs::{OrbCluster, cluster_layout};
use crate::eat::Eating;
use crate::inventory::{Dragging, FloorChange, FloorChanges, Hovered, Icons, Inventory, ItemId, Location, Refusal, Target, Toast, take_up};
use crate::keymap::{Action, ActionInput};
use crate::launch::AppState;
use crate::play::{DelveSystems, inventory_open, level_has_focus};
use crate::ui::{BAD, BRIGHT, CELL, LINE, OK, UiFonts, hud, hud_font};

/// How many slots there are, as keys 1 to 0.
pub const SLOTS: usize = 10;

pub struct QuickslotPlugin;

impl Plugin for QuickslotPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Quickslots>()
            .add_systems(OnExit(AppState::Delving), forget_the_bindings)
            .add_systems(
                Update,
                (
                    spawn_bar,
                    empty_lost_slots,
                    use_quickslot.run_if(level_has_focus),
                    bind_hovered.run_if(inventory_open),
                    fill_slots.after(crate::inventory::build_screen),
                    hint_slots,
                )
                    .chain()
                    .in_set(DelveSystems),
            );
    }
}

/// The item bound to each slot, if any. One resource for the delve; it is cleared when the
/// delve ends.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct Quickslots(pub [Option<ItemId>; SLOTS]);

impl Quickslots {
    /// The item bound to `slot`.
    pub fn get(&self, slot: usize) -> Option<ItemId> {
        self.0.get(slot).copied().flatten()
    }

    /// The slot `id` is bound to.
    pub fn slot_of(&self, id: ItemId) -> Option<usize> {
        self.0.iter().position(|&bound| bound == Some(id))
    }

    /// Binds `id` to `slot`, taking it from any other slot, and emptying whatever `slot` held.
    pub fn bind(&mut self, slot: usize, id: ItemId) {
        if slot >= SLOTS {
            return;
        }
        for bound in &mut self.0 {
            if *bound == Some(id) {
                *bound = None;
            }
        }
        self.0[slot] = Some(id);
    }

    /// Empties `slot`.
    pub fn clear(&mut self, slot: usize) {
        if let Some(bound) = self.0.get_mut(slot) {
            *bound = None;
        }
    }
}

/// What using an item from a slot does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickUse {
    /// Eat or drink it.
    Consume,
    /// Draw it into the hand, and then light it or put it out.
    Light,
    /// Fill the lanterns held or at the hip from it.
    Refill,
    /// Read it: a book.
    Read,
    /// Draw it into the hand.
    Draw,
}

/// What a slot does with `item`, or `None` if it can do nothing with it.
pub fn quick_use(item: &Item) -> Option<QuickUse> {
    if item.consume().is_some() && item.provision().is_some() {
        Some(QuickUse::Consume)
    } else if item.shape.lit().is_some() {
        Some(QuickUse::Light)
    } else if item.has_oil() {
        Some(QuickUse::Refill)
    } else if item.lesson().is_some() {
        Some(QuickUse::Read)
    } else if item.slots().iter().any(|s| matches!(s, Slot::Mainhand | Slot::Offhand)) {
        Some(QuickUse::Draw)
    } else {
        None
    }
}

/// Whether `id` is held in a hand or hung at the hip.
fn in_hand(inventory: &Inventory, id: ItemId) -> bool {
    matches!(inventory.location(id), Some(Location::Worn(Slot::Mainhand | Slot::Offhand | Slot::Hip)))
}

/// Empties each slot whose item is no longer in the inventory (used up, or set down or lost).
/// Only looks when the inventory has changed.
fn empty_lost_slots(inventory: Res<Inventory>, mut slots: ResMut<Quickslots>) {
    if !inventory.is_changed() {
        return;
    }
    let gone = |slot: &Option<ItemId>| slot.is_some_and(|id| inventory.item(id).is_none() || !inventory.is_carried(id));
    if slots.0.iter().any(gone) {
        for bound in &mut slots.0 {
            if gone(bound) {
                *bound = None;
            }
        }
    }
}

/// Another carried item of the same base and use as `id`, which has just been used up, and bound
/// to no slot: a slot never refills by taking another slot's item.
fn next_of_a_kind(inventory: &Inventory, slots: &Quickslots, base: &str, use_: QuickUse) -> Option<ItemId> {
    let mut ids: Vec<ItemId> = inventory.ids().filter(|&other| inventory.is_carried(other) && slots.slot_of(other).is_none()).collect();
    ids.sort();
    ids.into_iter().find(|&other| inventory.item(other).is_some_and(|item| item.base == base && quick_use(item) == Some(use_)))
}

/// What pressing a slot's key does to the item in it, and what the toast says. `None` if the
/// slot is empty.
pub fn use_slot(slot: usize, slots: &mut Quickslots, inventory: &mut Inventory, eating: &mut Eating, floor_changes: &mut FloorChanges) -> String {
    let Some(id) = slots.get(slot) else { return format!("Nothing is bound to slot {}", number(slot)) };
    let Some(item) = inventory.item(id) else {
        slots.clear(slot);
        return format!("Nothing is bound to slot {}", number(slot));
    };
    let (use_, base, name) = (quick_use(item), item.base.clone(), item.title().to_lowercase());
    match use_ {
        Some(QuickUse::Consume) => {
            let said = eating.eat(inventory, id).unwrap_or_else(|| "You cannot eat that now".to_string());
            if inventory.item(id).is_none() {
                match next_of_a_kind(inventory, slots, &base, QuickUse::Consume) {
                    Some(next) => slots.bind(slot, next),
                    None => slots.clear(slot),
                }
            }
            said
        }
        Some(QuickUse::Refill) => {
            let said = crate::kindle::refill_and_say(inventory, id);
            // An emptied flask is kept, as a drained waterskin is, but the slot moves to a full one,
            // or empties after the last.
            if !inventory.item(id).is_some_and(Item::has_oil) {
                match next_of_a_kind(inventory, slots, &base, QuickUse::Refill) {
                    Some(next) => slots.bind(slot, next),
                    None => slots.clear(slot),
                }
            }
            said
        }
        Some(QuickUse::Read) => eating.read(inventory, id).unwrap_or_else(|| format!("You cannot read the {name} now")),
        Some(QuickUse::Light) if in_hand(inventory, id) => crate::kindle::toggle_and_say(inventory, id),
        Some(QuickUse::Light | QuickUse::Draw) => {
            if matches!(use_, Some(QuickUse::Draw)) && matches!(inventory.location(id), Some(Location::Worn(Slot::Mainhand | Slot::Offhand))) {
                return format!("You already hold the {name}");
            }
            let Some(into) = inventory.target_slot(id) else { return format!("You cannot hold the {name}") };
            match crate::inventory::equip(inventory, id, into) {
                Ok(changes) => {
                    floor_changes.0.extend(changes);
                    format!("You draw the {name}")
                }
                Err(refusal) => refusal.reason().to_string(),
            }
        }
        None => {
            slots.clear(slot);
            format!("The {name} cannot be used from a slot")
        }
    }
}

/// The key a slot is used with: 1 to 9, then 0.
pub fn number(slot: usize) -> String {
    ((slot + 1) % 10).to_string()
}

/// Keys 1 to 0: uses the item in that slot.
fn use_quickslot(
    input: ActionInput,
    mut slots: ResMut<Quickslots>,
    mut inventory: ResMut<Inventory>,
    mut eating: Eating,
    mut floor_changes: ResMut<FloorChanges>,
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
) {
    let Some(slot) = input.just_pressed_index(Action::Quickslot) else { return };
    let said = use_slot(slot, &mut slots, &mut inventory, &mut eating, &mut floor_changes);
    toast.say(said, time.map_or(0.0, |t| t.elapsed_secs()));
}

/// Keys 1 to 0 in the inventory: binds the item under the mouse to that slot, as dropping it
/// on the strip's slot would, picking it up first if it lies underfoot ([`pick_up_and_bind`]),
/// and says so (or why not). Nothing under the mouse, nothing happens.
fn bind_hovered(
    input: ActionInput,
    hovered: Res<Hovered>,
    mut slots: ResMut<Quickslots>,
    mut inventory: ResMut<Inventory>,
    mut floor_changes: ResMut<FloorChanges>,
    mut toast: ResMut<Toast>,
    time: Option<Res<Time<Real>>>,
) {
    let Some(slot) = input.just_pressed_index(Action::Quickslot) else { return };
    let Some((id, _)) = hovered.0 else { return };
    let said = match pick_up_and_bind(&mut slots, &mut inventory, id, slot) {
        Ok((said, changes)) => {
            floor_changes.0.extend(changes);
            said
        }
        Err(refusal) => refusal.reason().to_string(),
    };
    toast.say(said, time.map_or(0.0, |t| t.elapsed_secs()));
}

/// Binds `id` to `slot` as [`bind_dropped`] does, first taking it up into the carried grids if
/// it is not carried ([`take_up`]: the first carried place with room, as a pickup puts it), and
/// says so, with what that changes in the level. Something a slot cannot use is refused before
/// anything is picked up, and with no room for it nothing is picked up or bound.
pub fn pick_up_and_bind(slots: &mut Quickslots, inventory: &mut Inventory, id: ItemId, slot: usize) -> Result<(String, Vec<FloorChange>), Refusal> {
    let item = inventory.item(id).ok_or(Refusal::Unknown)?;
    if quick_use(item).is_none() {
        return Err(Refusal::NotUsable);
    }
    let carried = inventory.is_carried(id);
    let changes = take_up(inventory, id)?;
    let said = bind_dropped(slots, inventory, id, slot)?;
    let said = if carried { said } else { said.replacen(" is bound", " is picked up and bound", 1) };
    Ok((said, changes))
}

/// The delve's end forgets the bindings: its items are gone.
fn forget_the_bindings(mut slots: ResMut<Quickslots>) {
    *slots = Quickslots::default();
}

/// A slot, on the HUD bar or the inventory's strip.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuickSlot(pub usize);

/// A slot on the inventory's strip, which an item can be dropped on.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuickslotDrop(pub usize);

/// What a slot shows now: the item whose icon is its child, and that icon.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Shown {
    item: Option<ItemId>,
    icon: Option<Entity>,
}

/// How one slot is drawn: the square's side, and the number's size.
#[derive(Clone, Copy)]
pub struct SlotStyle {
    pub size: Val,
    pub gap: Val,
    pub number: FontSize,
}

/// Spawns the ten slots as a row under `parent`. Each carries its [`QuickSlot`], and `drop`
/// slots a [`QuickslotDrop`] as well. The row is positioned by the caller's node.
pub fn spawn_slots(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, style: SlotStyle, drop: bool, row: Node) {
    parent.spawn((Name::new("Quickslots"), row, Pickable::IGNORE)).with_children(|row| {
        for slot in 0..SLOTS {
            let mut square = row.spawn((
                Name::new(format!("Quickslot {}", number(slot))),
                QuickSlot(slot),
                Shown::default(),
                Node {
                    width: style.size,
                    height: style.size,
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(3)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(CELL),
                BorderColor::all(LINE),
                Pickable::IGNORE,
            ));
            if drop {
                square.insert(QuickslotDrop(slot));
            }
            square.with_child((
                Text::new(number(slot)),
                TextFont::from_font_size(style.number).with_font(fonts.mono.clone()),
                TextColor(BRIGHT.with_alpha(0.55)),
                Node { position_type: PositionType::Absolute, left: px(2), top: px(0), ..default() },
                Pickable::IGNORE,
            ));
        }
    });
}

/// Whether `id` may be bound to a quickslot: carried, and something a slot can use.
pub fn check_bind(inventory: &Inventory, id: ItemId) -> Result<(), Refusal> {
    let item = inventory.item(id).ok_or(Refusal::Unknown)?;
    if quick_use(item).is_none() {
        return Err(Refusal::NotUsable);
    }
    if !inventory.is_carried(id) {
        return Err(Refusal::NotCarried);
    }
    Ok(())
}

/// Binds `id` to `slot`, dropped on the strip's slot or by the slot's key over it, and says so.
pub fn bind_dropped(slots: &mut Quickslots, inventory: &Inventory, id: ItemId, slot: usize) -> Result<String, Refusal> {
    check_bind(inventory, id)?;
    slots.bind(slot, id);
    let name = inventory.item(id).map_or_else(String::new, |item| item.title().to_lowercase());
    Ok(format!("The {name} is bound to slot {}", number(slot)))
}

/// The strip of slots the inventory screen shows, which items are dropped on: [`STRIP_SIZE`]
/// logical pixels square.
pub fn strip(parent: &mut ChildSpawnerCommands, fonts: &UiFonts) {
    let style = SlotStyle { size: px(STRIP_SIZE), gap: px(STRIP_GAP), number: FontSize::Px(12.0) };
    let row = Node { flex_direction: FlexDirection::Row, column_gap: style.gap, ..default() };
    spawn_slots(parent, fonts, style, true, row);
}

/// A slot on the strip, and the gap between them, in logical pixels.
pub const STRIP_SIZE: f32 = 50.0;
pub const STRIP_GAP: f32 = 6.0;

/// Draws the bar into each new cluster: the ten slots starting just right of the eye and level
/// with it, positioned from the layout ([`cluster_layout`]).
fn spawn_bar(mut commands: Commands, clusters: Query<Entity, Added<OrbCluster>>, fonts: Option<Res<UiFonts>>) {
    let Some(fonts) = fonts else { return };
    let layout = cluster_layout();
    for cluster in &clusters {
        commands.entity(cluster).with_children(|cluster| {
            let style = SlotStyle { size: hud(layout.slot), gap: hud(layout.gap), number: hud_font(layout.slot * 0.3) };
            let row = Node { position_type: PositionType::Absolute, left: hud(layout.bar.min.x), top: hud(layout.bar.min.y), flex_direction: FlexDirection::Row, column_gap: style.gap, ..default() };
            spawn_slots(cluster, &fonts, style, false, row);
            // The arrows or bolts left, over the bar's left end.
            cluster.spawn((
                crate::combat::shoot::AmmunitionCount,
                Text::new(""),
                UiFonts::style(&fonts.body, 16.0, BRIGHT),
                Node { position_type: PositionType::Absolute, left: hud(layout.bar.min.x), top: hud(layout.bar.min.y - layout.slot * 0.45), ..default() },
                Pickable::IGNORE,
            ));
        });
    }
}

/// Shows each slot's item as its icon, redrawn only when the item bound to it changes (an icon
/// drawn for the slot's square, as large as fits and keeping its shape), and the border bright
/// when filled.
fn fill_slots(
    mut commands: Commands,
    slots: Res<Quickslots>,
    inventory: Res<Inventory>,
    mut squares: Query<(Entity, &QuickSlot, &mut Shown, &mut BorderColor), Without<QuickslotDrop>>,
    mut drops: Query<(Entity, &QuickSlot, &mut Shown), With<QuickslotDrop>>,
    mut icons: Icons,
) {
    let mut fill = |commands: &mut Commands, entity: Entity, slot: usize, shown: &mut Shown| {
        let want = slots.get(slot).filter(|&id| inventory.item(id).is_some());
        if shown.item == want && (want.is_none() || shown.icon.is_some()) {
            return;
        }
        if let Some(icon) = shown.icon.take() {
            commands.entity(icon).try_despawn();
        }
        shown.item = want;
        if let (Some(id), Some(item)) = (want, want.and_then(|id| inventory.item(id))) {
            let footprint = inventory.footprint(id);
            let wide = footprint.is_none_or(|f| f.cols >= f.rows);
            let aspect = footprint.map_or(1.0, |f| f.cols as f32 / f.rows.max(1) as f32);
            let node = Node {
                position_type: PositionType::Absolute,
                width: if wide { percent(86) } else { Val::Auto },
                height: if wide { Val::Auto } else { percent(86) },
                aspect_ratio: Some(aspect),
                ..default()
            };
            commands.entity(entity).with_children(|slot| {
                shown.icon = Some(icons.spawn(slot, item, false, node, 14.0).insert(Pickable::IGNORE).id());
            });
        }
    };
    for (entity, slot, mut shown, mut border) in &mut squares {
        fill(&mut commands, entity, slot.0, &mut shown);
        let colour = if shown.item.is_some() { BRIGHT.with_alpha(0.7) } else { LINE };
        if border.top != colour {
            *border = BorderColor::all(colour);
        }
    }
    for (entity, slot, mut shown) in &mut drops {
        fill(&mut commands, entity, slot.0, &mut shown);
    }
}

/// A strip slot under a dragged item shows whether it may be bound there, green or red;
/// otherwise a filled one is cream and an empty one tan.
fn hint_slots(dragging: Res<Dragging>, mut drops: Query<(&QuickSlot, &Shown, &mut BorderColor), With<QuickslotDrop>>) {
    let drag = dragging.0.filter(|d| d.active);
    for (slot, shown, mut border) in &mut drops {
        let hint = drag.and_then(|d| d.target).and_then(|(target, fits)| match target {
            Target::Quickslot(s) if s == slot.0 => Some(if fits.is_ok() { OK } else { BAD }),
            _ => None,
        });
        let colour = hint.unwrap_or(if shown.item.is_some() { BRIGHT } else { LINE });
        if border.top != colour {
            *border = BorderColor::all(colour);
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::state::app::StatesPlugin;

    use super::*;
    use crate::character::DelverNeeds;
    use crate::combat::vitals::VitalsRoot;
    use crate::combat::{Health, RunDice};
    use crate::eat::{EatPlugin, HealSound};
    use crate::inventory::{Hovered, IconCache};
    use crate::keymap::Keymap;
    use crate::play::{Player, Screen, ScreenOpen, run_delve_systems_while_delving};
    use crate::tuning::Tuning;

    fn item(base: &str) -> fb_items::Item {
        fb_items::roll_item(base, 5).unwrap()
    }

    /// A delve of a hungry delver wearing a pack, with the eating and quickslot systems, stepped
    /// a frame at a time with no window, renderer or assets.
    fn delve() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .init_resource::<Tuning>()
            .init_resource::<Keymap>()
            .init_resource::<Inventory>()
            .init_resource::<Hovered>()
            .init_resource::<Toast>()
            .init_resource::<FloorChanges>()
            .init_resource::<Dragging>()
            .init_resource::<IconCache>()
            .init_resource::<UiFonts>()
            .init_resource::<Assets<Image>>()
            .init_resource::<ScreenOpen>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(RunDice(procgen_rng::Mulberry32::new(11)))
            .insert_resource(HealSound(Handle::default()))
            .add_plugins((EatPlugin, QuickslotPlugin, crate::combat::orbs::OrbPlugin));
        run_delve_systems_while_delving(&mut app);
        let mut needs = fb_character::Needs::default();
        needs.hunger.value = 0.55;
        let player = app.world_mut().spawn((Player, DelverNeeds(needs), Health { hp: 3, max: 12 })).id();
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
            inventory.wear(pack, Slot::Back).unwrap();
        }
        app.update();
        app.update();
        (app, player)
    }

    /// An item carried in the pack.
    fn give(app: &mut App, item: fb_items::Item) -> ItemId {
        let mut inventory = app.world_mut().resource_mut::<Inventory>();
        let id = inventory.add(item);
        inventory.stow(id).expect("room in the pack");
        id
    }

    fn bind(app: &mut App, slot: usize, id: ItemId) {
        app.world_mut().resource_mut::<Quickslots>().bind(slot, id);
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
        app.update();
    }

    fn toast(app: &App) -> String {
        app.world().resource::<Toast>().text.clone()
    }

    fn bound(app: &App, slot: usize) -> Option<ItemId> {
        app.world().resource::<Quickslots>().get(slot)
    }

    fn has(app: &App, id: ItemId) -> bool {
        app.world().resource::<Inventory>().item(id).is_some()
    }

    #[test]
    fn a_binding_moves_between_slots_and_clears() {
        let mut slots = Quickslots::default();
        let (a, b) = (ItemId(1), ItemId(2));
        slots.bind(0, a);
        slots.bind(1, b);
        assert_eq!((slots.get(0), slots.get(1), slots.slot_of(b)), (Some(a), Some(b), Some(1)));
        slots.bind(1, a);
        assert_eq!((slots.get(0), slots.get(1)), (None, Some(a)), "an item is in one slot only, and the new one replaces the old");
        slots.bind(SLOTS, b);
        assert_eq!(slots.slot_of(b), None, "there is no eleventh slot");
        slots.clear(1);
        assert_eq!(slots, Quickslots::default());
    }

    #[test]
    fn food_lights_and_weapons_each_have_their_use() {
        let uses: Vec<_> = ["Bread", "Tankard", "Healing draught", "Torch", "Lantern", "Sword", "Dagger", "Ring", "Mail"].into_iter().map(|b| quick_use(&item(b))).collect();
        use QuickUse::*;
        assert_eq!(uses, vec![Some(Consume), Some(Consume), Some(Consume), Some(Light), Some(Light), Some(Draw), Some(Draw), None, None]);
    }

    #[test]
    fn the_keys_are_one_to_nine_then_zero() {
        assert_eq!((0..SLOTS).map(number).collect::<String>(), "1234567890");
        let keymap = Keymap::default();
        let keys: Vec<_> = keymap.inputs(Action::Quickslot).to_vec();
        assert_eq!(keys.len(), SLOTS);
        assert_eq!(keys[2], crate::keymap::Input::Key(KeyCode::Digit3));
        assert_eq!(keys[9], crate::keymap::Input::Key(KeyCode::Digit0));
    }

    #[test]
    fn pressing_3_eats_the_loaf_and_the_slot_empties_after_the_last_one() {
        let (mut app, player) = delve();
        let (first, second) = (give(&mut app, item("Bread")), give(&mut app, item("Bread")));
        bind(&mut app, 2, first);
        press(&mut app, KeyCode::Digit3);
        assert!(!has(&app, first) && has(&app, second));
        assert!(toast(&app).starts_with("You eat the "), "{}", toast(&app));
        assert!((app.world().get::<DelverNeeds>(player).unwrap().0.hunger.value - 0.15).abs() < 1e-9);
        assert_eq!(bound(&app, 2), Some(second), "the slot moves to the next loaf");
        press(&mut app, KeyCode::Digit3);
        assert!(!has(&app, second));
        assert_eq!(bound(&app, 2), None, "and empties after the last");
        press(&mut app, KeyCode::Digit3);
        assert_eq!(toast(&app), "Nothing is bound to slot 3");
    }

    #[test]
    fn eating_from_one_slot_leaves_the_other_slots_alone() {
        let (mut app, _) = delve();
        let (a, b, c) = (give(&mut app, item("Bread")), give(&mut app, item("Bread")), give(&mut app, item("Bread")));
        bind(&mut app, 0, a);
        bind(&mut app, 1, b);
        press(&mut app, KeyCode::Digit1);
        assert!(!has(&app, a));
        assert_eq!((bound(&app, 0), bound(&app, 1)), (Some(c), Some(b)), "slot 1 moves to the loaf no slot holds, and slot 2 keeps its own");
        press(&mut app, KeyCode::Digit1);
        assert!(!has(&app, c));
        assert_eq!((bound(&app, 0), bound(&app, 1)), (None, Some(b)), "with only slot 2's loaf left, slot 1 empties rather than take it");
    }

    #[test]
    fn a_torch_on_slot_1_is_drawn_into_the_hand_and_then_lit_and_put_out() {
        let (mut app, _) = delve();
        let torch = give(&mut app, crate::carry::lit_torch(2));
        bind(&mut app, 0, torch);
        press(&mut app, KeyCode::Digit1);
        let held = app.world().resource::<Inventory>().location(torch);
        assert!(matches!(held, Some(Location::Worn(Slot::Mainhand | Slot::Offhand))), "{held:?}");
        assert!(toast(&app).starts_with("You draw the "), "{}", toast(&app));
        assert_eq!(bound(&app, 0), Some(torch), "a held item stays bound");
        let lit = |app: &App| app.world().resource::<Inventory>().item(torch).and_then(|i| i.shape.lit());
        assert_eq!(lit(&app), Some(true));
        press(&mut app, KeyCode::Digit1);
        assert_eq!(lit(&app), Some(false), "pressed again it puts the light out");
        assert_eq!(toast(&app), "You put out the torch");
        press(&mut app, KeyCode::Digit1);
        assert_eq!(lit(&app), Some(true));
    }

    #[test]
    fn lamp_oil_on_a_slot_fills_the_held_lantern_and_moves_to_the_next_flask() {
        let (mut app, _) = delve();
        let full_flask = || {
            let mut flask = item(fb_items::LAMP_OIL);
            if let armory::Item::Food(s) = &mut flask.shape {
                s.provision.fill = 1.0;
            }
            flask
        };
        let (first, second) = (give(&mut app, full_flask()), give(&mut app, full_flask()));
        let mut lantern = item("Lantern");
        lantern.shape.set_lit(true).unwrap();
        lantern.burn(1.0e9);
        let lantern = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let id = inventory.add(lantern);
            inventory.equip(id, Slot::Offhand).unwrap();
            id
        };
        assert_eq!(quick_use(&item(fb_items::LAMP_OIL)), Some(QuickUse::Refill));
        assert_eq!(quick_use(&item("Book")), Some(QuickUse::Read));
        assert_eq!(quick_use(&item("Scroll")), None, "a scroll does nothing yet");
        bind(&mut app, 4, first);
        press(&mut app, KeyCode::Digit5);
        assert_eq!(toast(&app), "You fill the lantern");
        let inventory = app.world().resource::<Inventory>();
        assert_eq!(inventory.item(lantern).unwrap().fuel_left(), inventory.item(lantern).unwrap().fuel_capacity());
        assert!(has(&app, first), "the empty flask is kept");
        assert_eq!(bound(&app, 4), Some(second), "the slot moves to the full flask");
        press(&mut app, KeyCode::Digit5);
        assert_eq!(toast(&app), "Your lantern is already full");
    }

    #[test]
    fn a_weapon_is_drawn_into_the_main_hand() {
        let (mut app, _) = delve();
        let sword = give(&mut app, item("Sword"));
        bind(&mut app, 1, sword);
        press(&mut app, KeyCode::Digit2);
        assert_eq!(app.world().resource::<Inventory>().location(sword), Some(Location::Worn(Slot::Mainhand)));
        press(&mut app, KeyCode::Digit2);
        assert!(toast(&app).starts_with("You already hold the "), "{}", toast(&app));
    }

    #[test]
    fn a_healing_draught_in_a_slot_heals_and_is_used_up() {
        let (mut app, player) = delve();
        let draught = give(&mut app, item("Healing draught"));
        bind(&mut app, 3, draught);
        press(&mut app, KeyCode::Digit4);
        let hp = app.world().get::<Health>(player).unwrap().hp;
        assert!((7..=12).contains(&hp), "{hp}");
        assert!(!has(&app, draught) && bound(&app, 3).is_none());
        assert!(toast(&app).starts_with("You drink the healing draught (+"), "{}", toast(&app));
    }

    #[test]
    fn a_slot_empties_when_its_item_leaves_the_inventory() {
        let (mut app, _) = delve();
        let loaf = give(&mut app, item("Bread"));
        let cheese = give(&mut app, item("Cheese"));
        bind(&mut app, 0, loaf);
        bind(&mut app, 1, cheese);
        app.update();
        assert_eq!((bound(&app, 0), bound(&app, 1)), (Some(loaf), Some(cheese)));
        // Set down on the floor, and thrown away.
        app.world_mut().resource_mut::<Inventory>().take_out(loaf);
        app.world_mut().resource_mut::<Inventory>().forget(cheese);
        app.update();
        assert_eq!((bound(&app, 0), bound(&app, 1)), (None, None));
    }

    #[test]
    fn the_keys_do_nothing_under_a_screen() {
        let (mut app, player) = delve();
        let loaf = give(&mut app, item("Bread"));
        bind(&mut app, 2, loaf);
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        press(&mut app, KeyCode::Digit3);
        assert!(has(&app, loaf));
        assert_eq!(app.world().get::<DelverNeeds>(player).unwrap().0.hunger.value, 0.55);
    }

    #[test]
    fn only_what_a_slot_can_use_and_what_is_carried_can_be_bound() {
        let (mut app, _) = delve();
        let ring = give(&mut app, item("Ring"));
        let loaf = give(&mut app, item("Bread"));
        let loose = app.world_mut().resource_mut::<Inventory>().add(item("Bread"));
        let mut slots = Quickslots::default();
        let inventory = app.world().resource::<Inventory>();
        assert_eq!(bind_dropped(&mut slots, inventory, ring, 0), Err(Refusal::NotUsable));
        assert_eq!(bind_dropped(&mut slots, inventory, loose, 0), Err(Refusal::NotCarried));
        assert_eq!(slots, Quickslots::default());
        let said = bind_dropped(&mut slots, inventory, loaf, 4).unwrap();
        assert!(said.ends_with("is bound to slot 5"), "{said}");
        assert_eq!(slots.get(4), Some(loaf));
    }

    fn hover(app: &mut App, id: Option<ItemId>) {
        app.world_mut().resource_mut::<Hovered>().0 = id.map(|id| (id, Rect::default()));
    }

    #[test]
    fn a_number_over_an_item_in_the_inventory_binds_it_to_that_slot() {
        let (mut app, player) = delve();
        let loaf = give(&mut app, item("Bread"));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        hover(&mut app, Some(loaf));
        press(&mut app, KeyCode::Digit3);
        assert_eq!(bound(&app, 2), Some(loaf));
        assert!(toast(&app).ends_with("is bound to slot 3"), "{}", toast(&app));
        assert!(has(&app, loaf), "binding does not eat it");
        assert_eq!(app.world().get::<DelverNeeds>(player).unwrap().0.hunger.value, 0.55);

        press(&mut app, KeyCode::Digit0);
        assert_eq!((bound(&app, 2), bound(&app, 9)), (None, Some(loaf)), "another number moves it");
        press(&mut app, KeyCode::Digit0);
        assert_eq!(bound(&app, 9), Some(loaf), "its own number leaves it");

        hover(&mut app, None);
        press(&mut app, KeyCode::Digit1);
        assert_eq!(bound(&app, 0), None, "nothing under the mouse, nothing happens");
        assert_eq!(bound(&app, 9), Some(loaf));
    }

    #[test]
    fn a_number_over_a_worn_or_unusable_item_acts_as_dragging_it_would() {
        let (mut app, _) = delve();
        let sword = app.world_mut().resource_mut::<Inventory>().add(item("Sword"));
        app.world_mut().resource_mut::<Inventory>().wear(sword, Slot::Mainhand).unwrap();
        let ring = give(&mut app, item("Ring"));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        hover(&mut app, Some(sword));
        press(&mut app, KeyCode::Digit2);
        assert_eq!(bound(&app, 1), Some(sword), "a held weapon binds");
        hover(&mut app, Some(ring));
        press(&mut app, KeyCode::Digit5);
        assert_eq!(bound(&app, 4), None);
        assert_eq!(toast(&app), Refusal::NotUsable.reason(), "the toast a drag onto the strip gives");
    }

    fn location(app: &App, id: ItemId) -> Option<Location> {
        app.world().resource::<Inventory>().location(id)
    }

    #[test]
    fn a_number_over_an_underfoot_item_picks_it_up_where_a_pickup_would_and_binds_it() {
        let (mut app, _) = delve();
        let loaf = app.world_mut().resource_mut::<Inventory>().add(item("Bread"));
        assert_eq!(location(&app, loaf), Some(Location::Loose), "lying underfoot");
        // Where picking it up would put it.
        let mut picked = app.world().resource::<Inventory>().clone();
        picked.pick_up(loaf).unwrap();
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        hover(&mut app, Some(loaf));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Digit4);
        app.update();
        assert_eq!(app.world().resource::<FloorChanges>().0, vec![FloorChange::Lifted(loaf)], "lifted from the level");
        assert_eq!(location(&app, loaf), picked.location(loaf));
        assert!(app.world().resource::<Inventory>().is_carried(loaf));
        assert_eq!(bound(&app, 3), Some(loaf));
        assert!(toast(&app).ends_with("is picked up and bound to slot 4"), "{}", toast(&app));
    }

    #[test]
    fn with_no_room_or_no_use_an_underfoot_item_stays_where_it_lies_and_nothing_binds() {
        let (mut app, _) = delve();
        // No pack: nowhere to put it.
        let pack = app.world().resource::<Inventory>().worn(Slot::Back).unwrap();
        app.world_mut().resource_mut::<Inventory>().forget(pack);
        let loaf = app.world_mut().resource_mut::<Inventory>().add(item("Bread"));
        let ring = app.world_mut().resource_mut::<Inventory>().add(item("Ring"));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        for (id, refusal) in [(loaf, Refusal::NoRoom), (ring, Refusal::NotUsable)] {
            hover(&mut app, Some(id));
            press(&mut app, KeyCode::Digit6);
            assert_eq!(bound(&app, 5), None);
            assert_eq!(location(&app, id), Some(Location::Loose), "{refusal:?}: still underfoot");
            assert_eq!(toast(&app), refusal.reason());
        }
        // A ring a slot cannot use is refused before anything is picked up, even with room.
        let mut inventory = app.world().resource::<Inventory>().clone();
        let sack = inventory.add(fb_items::roll_item("Sack", 7).unwrap());
        inventory.wear(sack, Slot::Back).unwrap();
        let mut slots = Quickslots::default();
        assert_eq!(pick_up_and_bind(&mut slots, &mut inventory, ring, 0), Err(Refusal::NotUsable));
        assert_eq!(inventory.location(ring), Some(Location::Loose));
    }

    /// The vitals HUD a view model makes, for the bar to join.
    fn hud(app: &mut App) -> Entity {
        let hud = app.world_mut().spawn((crate::carry::Hud, VitalsRoot, Visibility::default())).id();
        // The cluster, then what is drawn in it.
        for _ in 0..3 {
            app.update();
        }
        hud
    }

    fn slot_nodes(app: &mut App) -> Vec<(usize, usize)> {
        let mut query = app.world_mut().query_filtered::<(&QuickSlot, &Children), Without<QuickslotDrop>>();
        let mut slots: Vec<_> = query.iter(app.world()).map(|(s, c)| (s.0, c.len())).collect();
        slots.sort();
        slots
    }

    #[test]
    fn the_bar_has_ten_slots_that_show_their_items_and_hide_under_screens() {
        use crate::carry::hide_hud_under_screens;
        let (mut app, _) = delve();
        app.add_systems(Update, hide_hud_under_screens);
        let hud = hud(&mut app);
        assert_eq!(slot_nodes(&mut app), (0..SLOTS).map(|i| (i, 1)).collect::<Vec<_>>(), "each has its number");
        let loaf = give(&mut app, item("Bread"));
        bind(&mut app, 2, loaf);
        app.update();
        let slots = slot_nodes(&mut app);
        assert_eq!(slots[2], (2, 2), "the loaf's icon beside its number");
        assert_eq!(slots[3], (3, 1));
        // Used up, the icon goes.
        app.world_mut().resource_mut::<Inventory>().forget(loaf);
        app.update();
        app.update();
        assert_eq!(slot_nodes(&mut app)[2], (2, 1));
        // The bar is part of the HUD and the delve.
        assert!(app.world().get::<crate::carry::Hud>(hud).is_some());
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        app.update();
        assert_eq!(*app.world().get::<Visibility>(hud).unwrap(), Visibility::Hidden);
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        app.update();
        assert_eq!(*app.world().get::<Visibility>(hud).unwrap(), Visibility::Inherited);
    }

    #[test]
    fn leaving_the_delve_forgets_the_bindings() {
        let (mut app, _) = delve();
        let loaf = give(&mut app, item("Bread"));
        bind(&mut app, 0, loaf);
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Title);
        app.update();
        assert_eq!(*app.world().resource::<Quickslots>(), Quickslots::default());
    }

    #[test]
    fn the_bar_is_laid_out_by_the_cluster_and_fits_the_smallest_window() {
        use crate::combat::orbs::{MARGIN, orb_cluster_layout};
        let layout = cluster_layout();
        assert!((layout.bar.height() - layout.slot).abs() < 1e-4, "a row of slots");
        let row = layout.bar.width();
        let slots = SLOTS as f32;
        assert!((row - (slots * layout.slot + (slots - 1.0) * layout.gap)).abs() < 1e-4);
        // Everything inside 100 units, the height of any window 540 lines or more, and the slots
        // still big enough to read at the half-size floor (5.4 px a unit).
        assert!(MARGIN + layout.size.x <= 100.0, "{}", MARGIN + layout.size.x);
        assert!(layout.slot * 5.4 >= 20.0);
        let double = orb_cluster_layout(28.0);
        assert!((double.slot - layout.slot * 2.0).abs() < 1e-4, "the bar scales with the cluster");
    }
}
