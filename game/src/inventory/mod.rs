//! The inventory screen: I opens a dark ledger over the level, which keeps drawing
//! behind it, frees the mouse and stops the body. Each carried container shows as its grid,
//! with every item in it drawn as an icon at its footprint; items are dragged between cells
//! and grids, and R or the right mouse button turns the one being dragged (the plate's
//! `rotateDrag`). The layout is the plate's inventory leaf (`renderGrids`,
//! `reference/forever-below-plate.html`).
//!
//! - [`grid`]: cells and the rules for placing things in them, free of Bevy.
//! - [`items`]: the [`Inventory`], where every item is: worn, in a grid, or loose.
//! - [`icon`]: icons drawn from the items' Armory meshes.
//! - [`doll`] and [`seal`]: the doll's leaf, the runic seal with the equipment slots round
//!   it; dragging onto a slot, or onto the doll, equips the item if it is made for that slot,
//!   swapping out what was there ([`Inventory::equip`]); double-clicking wears or takes off.
//! - [`header`]: the delver's portrait, name, folk, height, class and stats over the left
//!   leaf, and its tabs.
//! - [`card`]: the hover card, for an item under the mouse here or under the crosshair in the
//!   level.
//! - [`underfoot`]: the items on the floor within reach, as a grid, and bags set down as
//!   their own grids; dragging in and out of them picks things up and sets them down.
//! - An opened container standing in the level (`crate::props`) shows as its own grid ahead
//!   of what lies underfoot while the screen is open ([`OpenContainer`]).
//!
//! The delver ([`Character`]) is rolled from the level's seed and starts in the "Freshly
//! fallen" outfit of `data/outfits.json` (a tunic and trousers), with a belt whose pouch is
//! their first grid (`docs/items.md`, "Slots"; the plate's belt is a container), and a burning
//! torch in the off hand, which is their carried light (`crate::carry::carried_light`). `FB_OPEN_INVENTORY=1` opens the screen at the start, for
//! captures (`=character` on the character tab, `=factions` on the factions tab; `FB_MEET=0,2` meets those factions, `FB_INVENTORY_SCROLL=<px>` scrolls the page); `FB_DEBUG_KIT=1` dresses the delver in their class's outfit with a lit light in the
//! off hand, starts them with a pack full of things, and sets a sack holding a few more, and a
//! shield, down in front of them. `FB_HOLD=<main>[,<off>]` puts items of those bases in the hands
//! (a torch or lantern lit; `none` empties a hand), after either kit. `FB_DEBUG_HOVER=<n>` holds the
//! card over the `n`th thing in the first carried grid (`u<n>` underfoot, `o<n>` in the open
//! container or trader's shelf).

pub mod card;
pub mod doll;
pub mod factions;
pub mod grid;
pub mod header;
pub mod icon;
pub mod items;
pub mod seal;
pub mod sheet;
pub mod underfoot;

use bevy::{
    app::HierarchyPropagatePlugin,
    camera::visibility::RenderLayers,
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    light::NotShadowCaster,
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_items::{Kind, Rarity, Slot, footprint::Footprint};

pub use grid::{Grid, Placement};
pub use icon::{IconCache, Icons};
pub use doll::{DollAssets, DollStage, DollView, SlotView};
pub use items::{Displaced, Inventory, ItemId, Location, Refusal, char_bucket};
pub use underfoot::{FloorChange, FloorChanges, Underfoot};

use crate::carry::ViewModelCamera;
use crate::character::{Character, ChosenDelver, character_for};
use crate::keymap::{Action, ActionInput, Keymap};
use crate::launch::AppState;
use crate::play::{CurrentLevel, DelveSystems, Screen, ScreenOpen, SheetTab, show_screen, switch_screens};
use crate::quickslots::{QuickslotDrop, Quickslots};
pub use crate::ui::node_rect;
use crate::ui::{BAD, BG, BRIGHT, CELL, INK, LINE, MUTED, OK, UiFonts, rarity_color, ui_cursor};
use header::ToastText;
use crate::tuning::Tuning;

/// Width of a grid line, in logical pixels.
const LINE_PX: f32 = 2.0;
/// The widest the left leaf grows, in logical pixels: a pack and a belt pouch side by side at
/// the shipped 72-pixel cells (8 + 4 cells and the gap between).
const LEAF_MAX_PX: f32 = 900.0;
/// The left page's width on every tab, so the header and tabs never move when switching. The
/// interface scale grows it with the window.
pub const LEAF_PX: f32 = 620.0;
/// The same while bartering, wide enough for the trader's shelf beside a pack.
const BARTER_LEAF_MAX_PX: f32 = 1000.0;
/// Radius of a grid's rounded corners, in logical pixels (the plate's `.grid`).
const GRID_RADIUS_PX: f32 = 12.0;
/// How far the mouse moves with the button down before a press becomes a drag, in logical
/// pixels (the plate's 6).
const DRAG_START_PX: f32 = 6.0;
/// How far one wheel line scrolls the left column, in logical pixels.
const SCROLL_LINE_PX: f32 = 36.0;
/// How near the top or bottom of the left column a held item starts to scroll it, in logical
/// pixels.
const EDGE_SCROLL_PX: f32 = 64.0;
/// How fast a held item at the very edge scrolls the column, in logical pixels a second.
const EDGE_SCROLL_SPEED: f32 = 700.0;
/// How long a refusal stays on the screen, in seconds (the plate's `toast`, 3.6 s).
const TOAST_SECONDS: f32 = 3.6;
/// What the help line under the grids says (the plate's `.hint`).
pub fn help_text(keymap: &Keymap) -> String {
    format!(
        "Drag things between the pack, the doll and the floor. A bag keeps what is in it; set one down and it opens underfoot. {turn} while dragging to turn an item. Double-click to wear or take off, or to empty a purse into your own. Drop food, a light or a weapon on a quickslot to bind it, and the keys {slots} use it.",
        turn = keymap.label(Action::TurnItem),
        slots = keymap.label(Action::Quickslot),
    )
}
/// Longest gap between the two clicks of a double-click, in seconds: Windows' default of
/// 500 ms (`SetDoubleClickTime`,
/// <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setdoubleclicktime>).
const DOUBLE_CLICK_SECONDS: f32 = 0.5;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<HierarchyPropagatePlugin<RenderLayers>>() {
            app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(Update));
        }
        if !app.is_plugin_added::<HierarchyPropagatePlugin<NotShadowCaster>>() {
            app.add_plugins(HierarchyPropagatePlugin::<NotShadowCaster>::new(Update));
        }
        app.init_resource::<Inventory>()
            .init_resource::<IconCache>()
            .init_resource::<UiFonts>()
            .init_resource::<Keymap>()
            .init_resource::<ScreenOpen>()
            .init_resource::<SheetTab>()
            .init_resource::<crate::rules::GameRules>()
            .init_resource::<Dragging>()
            .init_resource::<Hovered>()
            .init_resource::<Quickslots>()
            .init_resource::<sheet::HoveredNumber>()
            .init_resource::<card::ShownCard>()
            .init_resource::<Toast>()
            .init_resource::<Underfoot>()
            .init_resource::<FloorChanges>()
            .init_resource::<OpenContainer>()
            .init_resource::<crate::barter::Barter>()
            .init_resource::<DollAssets>()
            .init_resource::<DollStage>()
            .init_resource::<crate::fitting::FitCache>()
            .init_resource::<Character>()
            .init_resource::<Tuning>()
            .init_resource::<ColumnScroll>()
            .init_resource::<ShelfScrolled>()
            .add_systems(OnExit(AppState::Delving), leave_the_delve)
            .add_systems(OnEnter(AppState::Delving), (roll_character, give_starting_kit, give_debug_kit.run_if(debug_kit_wanted), hold_for_capture.run_if(|| env_flag("FB_HOLD")), give_ammunition.run_if(|| debug_kit_wanted() || env_flag("FB_HOLD")), give_for_capture).chain())
            .add_systems(PostStartup, open_for_capture.run_if(in_state(AppState::Delving)).run_if(|| env_flag("FB_OPEN_INVENTORY")))
            .add_systems(
                Update,
                (
                    (
                        underfoot::set_down_debug_kit.run_if(debug_kit_wanted),
                        underfoot::set_down_for_capture.run_if(|| env_flag("FB_DROP")),
                        // Screens open and close in `switch_screens`.
                        underfoot::track_underfoot,
                        close_container_with_screen,
                        (factions::note_levels_met, meet_for_capture.run_if(|| std::env::var("FB_MEET").is_ok()), header::click_tabs, reset_scroll_on_tab).chain(),
                        build_screen,
                        scroll_column,
                        scroll_shelf,
                        drag_items,
                        unhang_without_belt,
                        crate::enchanting::scrap_for_capture.run_if(|| env_flag("FB_SCRAP")),
                        crate::character::wear_abilities,
                        (sheet::update_status, sheet::update_vitals, sheet::update_gear, sheet::update_conditions, sheet::update_enchanting, sheet::update_tongues).run_if(|open: Res<ScreenOpen>| open.is(Screen::Inventory)),
                        (sheet::hover_numbers, sheet::spawn_tip, sheet::show_breakdown).chain(),
                        debug_hover.run_if(|| std::env::var("FB_DEBUG_HOVER").is_ok()),
                        underfoot::apply_floor_changes,
                        show_drag,
                        show_toast,
                        (card::spawn_card, card::show_card, card::show_compare).chain(),
                    )
                        .chain()
                        .after(switch_screens),
                    (icon::draw_icons, icon::reveal_icons).chain().after(build_screen),
                    underfoot::drop_hovered.run_if(crate::play::inventory_open).run_if(crate::barter::not_bartering).after(drag_items).before(underfoot::apply_floor_changes),
                    doll::turn_seal,
                    (doll::start_doll_fitting, doll::poll_doll_fitting, doll::show_doll, doll::turn_doll.after(drag_items)).chain(),
                )
                    .in_set(DelveSystems),
            );
    }
}

/// Leaving the delve clears what the screens kept of it: the doll stage's record of its figure
/// and cameras (despawned with the delve), the icons still being drawn, what lay underfoot, a
/// container left open, a hovered item, a toast and the fits kept ([`crate::fitting::FitCache`]), none of which belong to the next delve's
/// delver and items.
pub fn leave_the_delve(mut commands: Commands, mut stage: ResMut<DollStage>, mut icons: ResMut<icon::IconCache>) {
    *stage = DollStage::default();
    commands.insert_resource(crate::fitting::FitCache::default());
    icons.abandon_drawing();
    icons.forget_arms();
    commands.insert_resource(Underfoot::default());
    commands.insert_resource(FloorChanges::default());
    commands.insert_resource(OpenContainer::default());
    commands.insert_resource(Hovered::default());
    commands.insert_resource(card::ShownCard::default());
    commands.insert_resource(Toast::default());
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty() && v != "0")
}

fn debug_kit_wanted() -> bool {
    env_flag("FB_DEBUG_KIT")
}

/// A seed for the starting kit, from the level's seed so a delve always starts the same.
fn kit_seed(level: Option<&CurrentLevel>, salt: u32) -> u32 {
    level.map_or(1, |l| l.0.seed).wrapping_mul(0x2545_f491).wrapping_add(salt)
}

/// The outfit the delver starts in (`data/outfits.json`).
pub const STARTING_OUTFIT: &str = "fallen";

/// The delver chosen on the title, taken up, or one rolled from the level's seed.
fn roll_character(mut commands: Commands, mut character: ResMut<Character>, chosen: Option<Res<ChosenDelver>>, level: Option<Res<CurrentLevel>>) {
    *character = match chosen {
        Some(chosen) => {
            commands.remove_resource::<ChosenDelver>();
            chosen.0.clone()
        }
        None => character_for(kit_seed(level.as_deref(), 0xc4a7)),
    };
}

/// The delver starts with almost nothing: the [`STARTING_OUTFIT`]'s clothes, a belt whose pouch
/// is their first grid, and a burning torch in the off hand so they start with a carried light.
/// All of it is common. The outfit's goods and the things it leaves at its wearer's feet (a
/// spare torch, bread) are not given; folk in the same outfit still carry them.
fn give_starting_kit(mut inventory: ResMut<Inventory>, character: Res<Character>, level: Option<Res<CurrentLevel>>) {
    if inventory.worn(Slot::Belt).is_some() {
        return;
    }
    for (slot, item) in starting_kit(|salt| kit_seed(level.as_deref(), salt), character.folk) {
        let id = inventory.add(item);
        if inventory.wear(id, slot).is_err() {
            inventory.forget(id);
        }
    }
}

/// The starting kit, slot by slot, each piece rolled from `seed(salt)`: the belt first, then
/// the [`STARTING_OUTFIT`] for `folk` (a piece for a slot already filled is not worn), then the
/// torch. The creator's figure wears it too ([`crate::title::preview`]).
pub fn starting_kit(seed: impl Fn(u32) -> u32, folk: fb_body::Folk) -> Vec<(Slot, fb_items::Item)> {
    let odds = starting_odds();
    let mut kit = Vec::new();
    if let Some(belt) = fb_items::roll_item_with("Belt", seed(0xbe17), odds) {
        kit.push((Slot::Belt, belt));
    }
    if let Some(outfit) = fb_items::outfit::outfit(STARTING_OUTFIT) {
        kit.extend(outfit.roll_with(seed(0x0f17), folk.key(), odds).worn);
    }
    kit.push((Slot::Offhand, starting_torch(seed(0x70c4))));
    kit
}

/// The odds the starting kit rolls at: the first level's, but common only.
fn starting_odds() -> fb_items::Odds {
    fb_items::Odds::loot(1).capped(Rarity::Common)
}

/// The first burning common torch rolled at or after `seed`.
fn starting_torch(seed: u32) -> fb_items::Item {
    (0..u32::MAX)
        .map(|k| seed.wrapping_add(k))
        .find_map(|s| fb_items::roll_item_with("Torch", s, starting_odds()).filter(|item| crate::carry::item_light(&item.shape).is_some()))
        .expect("some seed rolls a burning torch")
}

/// Equips `item` in `slot`; whatever it displaces is stowed, or forgotten if there is no room.
fn equip_kit(inventory: &mut Inventory, item: fb_items::Item, slot: Slot) {
    let id = inventory.add(item);
    match inventory.equip(id, slot) {
        Ok(Some((old, Displaced::Underfoot))) => {
            if inventory.stow(old).is_err() {
                inventory.forget(old);
            }
        }
        Ok(_) => {}
        Err(_) => inventory.forget(id),
    }
}

/// For captures and trying the screen: the delver's class outfit, with a lit light in the off
/// hand (a lantern if the outfit gave none), and a pack on the back filled with one of most
/// things.
fn give_debug_kit(mut inventory: ResMut<Inventory>, character: Res<Character>, level: Option<Res<CurrentLevel>>) {
    let seed = |salt| kit_seed(level.as_deref(), salt);
    if let Some(outfit) = fb_items::outfit::outfit(character.class.outfit_key()) {
        for (slot, item) in outfit.roll(seed(2), character.folk.key()).worn {
            equip_kit(&mut inventory, item, slot);
        }
    }
    if doll::lit_hand(&inventory) != Some(Slot::Offhand)
        && let Some(lantern) = (seed(3)..seed(3).wrapping_add(500)).filter_map(|s| fb_items::roll_item("Lantern", s)).find(|i| crate::carry::item_light(&i.shape).is_some())
    {
        equip_kit(&mut inventory, lantern, Slot::Offhand);
    }
    if inventory.worn(Slot::Back).is_none()
        && let Some(pack) = fb_items::roll_item("Pack", seed(1))
    {
        let pack = inventory.add(pack);
        let _ = inventory.wear(pack, Slot::Back);
    }
    let legendary = (1..4_000).filter_map(|s| fb_items::roll_item("Dagger", s)).find(|i| i.rarity == Rarity::Legendary);
    let things = ["Sword", "Healing draught", "Bread", "Ring", "Lantern", "Apple", "Lockpicks", "Gloves", "Candle", "Amulet", "Cheese"];
    for item in legendary.into_iter().chain(things.iter().enumerate().filter_map(|(i, base)| fb_items::roll_item(base, seed(10 + i as u32)))) {
        let id = inventory.add(item);
        if inventory.stow(id).is_err() {
            inventory.forget(id);
        }
    }
}

/// For captures and trying the game: a bow or crossbow in the main hand comes with a full quiver
/// or bolt case ([`fb_items::outfit::AMMUNITION_CARRIED`]), slung in the quiver slot, else carried.
/// One the kit already gave is filled up.
fn give_ammunition(mut inventory: ResMut<Inventory>, level: Option<Res<CurrentLevel>>) {
    let Some((_, ranged)) = crate::combat::shoot::ranged_in_hand(&inventory) else { return };
    let (holder_base, ammo_base) = if ranged.ammunition == armory::Family::Bolt { ("Bolt case", "Bolt") } else { ("Quiver", "Arrow") };
    let holder = match crate::combat::shoot::holder_of(&inventory, ranged.ammunition) {
        Some(holder) => holder,
        None => {
            let Some(item) = fb_items::roll_item_with(holder_base, kit_seed(level.as_deref(), 0xa770), starting_odds()) else { return };
            let holder = inventory.add(item);
            if inventory.wear(holder, Slot::Quiver).is_err() && inventory.stow(holder).is_err() {
                inventory.forget(holder);
                return;
            }
            holder
        }
    };
    let have = inventory.contents_count(holder);
    for k in have..fb_items::outfit::AMMUNITION_CARRIED {
        let Some(item) = fb_items::roll_item_with(ammo_base, kit_seed(level.as_deref(), 0xa770 + 1 + k as u32), starting_odds()) else { return };
        let id = inventory.add(item);
        let at = inventory.grid(holder).and_then(|g| g.first_free(fb_items::footprint::Footprint { cols: 1, rows: 1 }));
        if at.is_none_or(|at| inventory.put(id, holder, at).is_err()) {
            inventory.forget(id);
            break;
        }
    }
}

/// What `FB_HOLD`'s value `spec` puts in each hand: `(main, off)`, each `None` to leave that hand
/// as the kit left it, `Some(None)` to empty it, or the base to hold.
pub fn hold_spec(spec: &str) -> (Option<Option<String>>, Option<Option<String>>) {
    let mut hands = spec.split(',').map(|b| {
        let base = b.trim();
        match base {
            "" => None,
            "none" => Some(None),
            _ => Some(Some(base.to_string())),
        }
    });
    (hands.next().flatten(), hands.next().flatten())
}

/// The first item of `base` rolled at or after `seed`, lit if it is a light that can be.
/// `Chamberstick` is a candle in its holder.
fn roll_held(base: &str, seed: u32) -> Option<fb_items::Item> {
    let (base, holder) = if base == "Chamberstick" { ("Candle", true) } else { (base, false) };
    let rolled = |s: u32| fb_items::roll_item(base, s);
    let wanted = |i: &fb_items::Item| crate::carry::item_light(&i.shape).is_some() && (!holder || matches!(&i.shape, armory::Item::Sundry(s) if s.article.holder));
    let lit = (0..500).filter_map(|k| rolled(seed.wrapping_add(k))).find(wanted);
    lit.filter(|_| matches!(base, "Torch" | "Lantern" | "Candle")).or_else(|| rolled(seed))
}

/// For captures: the hands hold what `FB_HOLD` names ([`hold_spec`]), whatever the kit gave.
fn hold_for_capture(mut inventory: ResMut<Inventory>, level: Option<Res<CurrentLevel>>) {
    let Ok(spec) = std::env::var("FB_HOLD") else { return };
    let (main, off) = hold_spec(&spec);
    for (slot, want) in [(Slot::Mainhand, main), (Slot::Offhand, off)] {
        let Some(want) = want else { continue };
        let Some(base) = want else {
            empty_hand(&mut inventory, slot);
            continue;
        };
        if hold_rolled(&mut inventory, slot, &base, kit_seed(level.as_deref(), 0x401d + slot as u32)).is_none() {
            warn!("FB_HOLD: no base called {base:?}");
        }
    }
}

/// Empties `slot`, stowing what it held (or dropping it from the world with no room).
fn empty_hand(inventory: &mut Inventory, slot: Slot) {
    if let Some(old) = inventory.worn(slot) {
        inventory.take_out(old);
        if inventory.stow(old).is_err() {
            inventory.forget(old);
        }
    }
}

/// Puts the first item of `base` rolled at or after `seed` in `slot`, stowing what it held, and
/// returns the item's name; `None`, leaving the hand as it was, if there is no such base.
pub fn hold_rolled(inventory: &mut Inventory, slot: Slot, base: &str, seed: u32) -> Option<String> {
    let item = roll_held(base, seed)?;
    let name = item.name.clone();
    empty_hand(inventory, slot);
    equip_kit(inventory, item, slot);
    Some(name)
}

/// An `FB_EQUIP` entry: a base, or `base@slot` to equip it in another slot it fits
/// (`Lantern@hip`, `Sword@offhand`).
pub fn equip_entry(entry: &str) -> (&str, Option<Slot>) {
    match entry.split_once('@') {
        Some((base, slot)) => (base.trim(), serde_json::from_str(&format!("\"{}\"", slot.trim().to_lowercase())).ok()),
        None => (entry, None),
    }
}

/// The bases a comma-separated list names.
fn bases_in(list: &str) -> impl Iterator<Item = &str> {
    list.split(',').map(str::trim).filter(|b| !b.is_empty())
}

/// For captures: `FB_EQUIP` equips one of each base it names in its slot, then `FB_STOW` puts
/// one of each base it names in the carried grids, after the kit and `FB_HOLD`, so the screen
/// can be seen with given things worn and in the pack (`FB_EQUIP=Pack` first makes room).
fn give_for_capture(mut inventory: ResMut<Inventory>, level: Option<Res<CurrentLevel>>) {
    let roll = |base: &str, salt: u32| roll_held(base, kit_seed(level.as_deref(), 0x5700 + salt));
    let equip = std::env::var("FB_EQUIP").unwrap_or_default();
    for (i, entry) in bases_in(&equip).enumerate() {
        let (base, slot) = equip_entry(entry);
        let Some(item) = roll(base, 0x80 + i as u32) else {
            warn!("FB_EQUIP: no base called {base:?}");
            continue;
        };
        match slot.or(item.profile.slot) {
            Some(slot) => equip_kit(&mut inventory, item, slot),
            None => warn!("FB_EQUIP: {base:?} is not worn"),
        }
    }
    let taught = std::env::var("FB_TEACH").unwrap_or_default();
    for (i, spec) in bases_in(&taught).enumerate() {
        let Some(item) = crate::reading::teaching_for_capture(spec, i as u32) else { continue };
        let id = inventory.add(item);
        if inventory.stow(id).is_err() {
            warn!("FB_TEACH: no room for {spec:?}");
            inventory.forget(id);
        }
    }
    let stow = std::env::var("FB_STOW").unwrap_or_default();
    for (i, base) in bases_in(&stow).enumerate() {
        let Some(item) = roll(base, i as u32) else {
            warn!("FB_STOW: no base called {base:?}");
            continue;
        };
        let id = inventory.add(item);
        if inventory.stow(id).is_err() {
            warn!("FB_STOW: no room for {base:?}");
            inventory.forget(id);
        }
    }
}

fn open_for_capture(mut open: ResMut<ScreenOpen>, mut tab: ResMut<SheetTab>, mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>) {
    show_screen(&mut open, Some(Screen::Inventory), cursor.as_deref_mut().map(|c| &mut **c));
    if std::env::var("FB_OPEN_INVENTORY").is_ok_and(|v| v.eq_ignore_ascii_case("character")) {
        *tab = SheetTab::Character;
    } else if std::env::var("FB_OPEN_INVENTORY").is_ok_and(|v| v.eq_ignore_ascii_case("factions")) {
        *tab = SheetTab::Factions;
    }
}

/// `FB_MEET=<n>[,<n>...]` has the delver meet the factions at those places in the world's pool
/// (on the level they stand on), for captures of the factions tab.
fn meet_for_capture(record: Option<ResMut<crate::record::WorldRecord>>, abyss: Option<Res<crate::travel::Abyss>>, level: Option<Res<CurrentLevel>>, mut done: Local<bool>) {
    let (Some(mut record), Some(abyss), Some(level)) = (record, abyss, level) else { return };
    let Some(spec) = std::env::var("FB_MEET").ok().filter(|_| !*done) else { return };
    *done = true;
    let world = fb_faction::World::new(abyss.run_seed);
    for place in spec.split(',').filter_map(|p| p.trim().parse::<usize>().ok()) {
        let Some(faction) = world.factions().get(place) else { continue };
        record.standings.meet(faction.id);
        record.note_met_on(faction.id, level.0.depth);
        if place == 0 {
            record.standings.shift(faction.id, 70);
        }
    }
}

/// The whole screen, over the level.
#[derive(Component, Debug)]
#[require(DespawnOnExit<crate::launch::AppState> = DespawnOnExit(crate::launch::AppState::Delving))]
pub struct InventoryScreen;

/// The ledger page. Beyond it lies the level.
#[derive(Component, Debug)]
pub struct Ledger;

/// The left page: header, tabs and the tab's body, one width on every tab.
#[derive(Component, Debug)]
pub struct LeafPage;

/// The left column of grids under the header: it scrolls when it holds more than fits.
#[derive(Component, Debug)]
pub struct ScrollColumn;

/// How far the left column is scrolled, in logical pixels. The screen is rebuilt whenever the
/// inventory changes, so this is what keeps the column where it was.
#[derive(Resource, Default, Debug, PartialEq)]
pub struct ColumnScroll(pub f32);

/// The scroll area round the trader's shelf while bartering: the shelf can hold more than fits
/// beside the balance, so the grid scrolls inside it and the balance and its buttons stay put.
#[derive(Component, Debug)]
pub struct ShelfScroll;

/// How far the shelf is scrolled, in logical pixels, kept across the screen's rebuilds.
#[derive(Resource, Default, Debug, PartialEq)]
pub struct ShelfScrolled(pub f32);

/// The tallest the shelf's area is, at the reference size, logical pixels (the interface is
/// scaled with the window, so the same at every size): what the ledger leaves under the header
/// and the balance panel.
pub const SHELF_MAX_PX: f32 = 500.0;

/// How tall the shelf's area is for a grid `grid` pixels tall: all of it if it fits, else the most.
pub fn shelf_view_height(grid: f32) -> f32 {
    grid.min(SHELF_MAX_PX)
}

/// A grid on the screen, and whose it is.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridView {
    /// A container's own grid, carried or set down.
    Container(ItemId),
    /// What lies on the floor within reach.
    Underfoot,
}

/// The container standing in the level whose grid the screen shows, once it has been opened
/// (`crate::props`); none once the screen closes.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenContainer(pub Option<ItemId>);

/// Closing the screen closes the container it was opened for.
fn close_container_with_screen(open: Res<ScreenOpen>, mut showing: ResMut<OpenContainer>) {
    if !open.is(Screen::Inventory) && showing.0.is_some() {
        showing.0 = None;
    }
}

/// An item drawn in a grid.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemView {
    pub id: ItemId,
}

/// The icon inside an [`ItemView`].
#[derive(Component, Debug)]
struct ItemIcon;

/// The item following the mouse while it is dragged.
#[derive(Component, Debug)]
struct DragGhost;

/// The cells a dragged item would cover where it is now, green if it fits and red if not.
#[derive(Component, Debug)]
struct DropHint;

/// Where a dragged item would go if let go now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A cell of a grid; the item's top-left cell goes where the grab puts it.
    Cell { view: GridView, cell: IVec2 },
    /// The page, off every grid: nothing happens.
    Page,
    /// Off the page, over the level.
    Level,
    /// An equipment slot round the doll.
    Slot(Slot),
    /// The doll itself: the item goes in the slot it is made for.
    Doll,
    /// A quickslot on the strip: the item is bound to it, not moved ([`crate::quickslots`]).
    Quickslot(usize),
}

/// An item being dragged, or pressed and about to be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub id: ItemId,
    /// The cell of the item under the mouse when it was taken, from its top-left cell.
    pub grab: IVec2,
    /// Its footprint as it lies now.
    pub size: Footprint,
    pub turned: bool,
    /// Where the button went down, in logical pixels.
    pub press: Vec2,
    /// Whether the mouse has moved far enough for this to be a drag.
    pub active: bool,
    /// Where it would go now, and whether it may.
    pub target: Option<(Target, Result<(), Refusal>)>,
}

impl Drag {
    /// Turns it a quarter turn about the cell it was grabbed by.
    pub fn turn(&mut self) {
        self.size = Footprint { cols: self.size.rows, rows: self.size.cols };
        self.turned = !self.turned;
        self.grab = IVec2::new(self.grab.y.min(self.size.cols as i32 - 1), self.grab.x.min(self.size.rows as i32 - 1));
    }

    /// Where its top-left cell lands with the grabbed cell over `cell`.
    pub fn placement_at(&self, cell: IVec2) -> Option<Placement> {
        let at = cell - self.grab;
        (at.x >= 0 && at.y >= 0 && at.x < 256 && at.y < 256).then(|| Placement::new(at.x as u8, at.y as u8, self.turned))
    }
}

#[derive(Resource, Debug, Default)]
pub struct Dragging(pub Option<Drag>);

/// The item under the mouse, and its rectangle on screen in logical pixels.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Hovered(pub Option<(ItemId, Rect)>);

/// A line saying why a drop was refused, or what equipping from the floor did, for a few
/// seconds: beside the inventory's tabs, and low in the level's HUD (`crate::carry`).
#[derive(Resource, Debug, Default)]
pub struct Toast {
    pub text: String,
    pub until: f32,
}

impl Toast {
    /// Shows `text` from `now` (seconds of real time) for [`TOAST_SECONDS`].
    pub fn say(&mut self, text: impl Into<String>, now: f32) {
        self.text = text.into();
        self.until = now + TOAST_SECONDS;
    }

    /// Whether a line still shows at `now`.
    pub fn showing(&self, now: f32) -> bool {
        self.until > now && !self.text.is_empty()
    }
}

/// The [`Toast`] and the real clock it runs on, for any system to say a line through. Both are
/// optional, so a system that speaks still runs in an app without the HUD.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Toasts<'w> {
    toast: Option<ResMut<'w, Toast>>,
    real: Option<Res<'w, Time<Real>>>,
}

impl Toasts<'_> {
    fn now(&self) -> f32 {
        self.real.as_deref().map_or(0.0, |t| t.elapsed_secs())
    }

    /// Shows `text` afresh.
    pub fn say(&mut self, text: impl Into<String>) {
        let now = self.now();
        if let Some(toast) = self.toast.as_deref_mut() {
            toast.say(text, now);
        }
    }

    /// Shows `text`, unless it is the line already showing.
    pub fn say_once(&mut self, text: &str) {
        let now = self.now();
        if let Some(toast) = self.toast.as_deref_mut()
            && (toast.text != text || !toast.showing(now))
        {
            toast.say(text, now);
        }
    }

    /// Shows `text` after whatever line still shows (`"Hilda is slain · the Ash Choir turn on
    /// you"`), or alone when nothing does.
    pub fn add(&mut self, text: &str) {
        let now = self.now();
        if let Some(toast) = self.toast.as_deref_mut() {
            let line = if toast.showing(now) && !toast.text.contains(text) { format!("{} \u{b7} {text}", toast.text) } else { text.to_string() };
            toast.say(line, now);
        }
    }
}

/// Where `drag` lands in `container` with its grabbed cell over `cell`: as it lies now; or,
/// for something being taken off (worn things are drawn upright) and not turned with R since,
/// turned a quarter turn if it fits only that way.
pub fn landing(inventory: &Inventory, drag: &Drag, container: ItemId, cell: IVec2) -> Result<Placement, Refusal> {
    let at = drag.placement_at(cell).ok_or(Refusal::NoRoom)?;
    match inventory.check_put(drag.id, container, at) {
        Err(refusal) if !drag.turned && matches!(inventory.location(drag.id), Some(Location::Worn(_))) => {
            let mut turned = *drag;
            turned.turn();
            turned.placement_at(cell).filter(|&at| inventory.check_put(drag.id, container, at).is_ok()).ok_or(refusal)
        }
        fits => fits.map(|()| at),
    }
}

/// Whether `drag` may be let go at `target`.
pub fn check_drop(inventory: &Inventory, drag: &Drag, target: Target) -> Result<(), Refusal> {
    match target {
        Target::Cell { view: GridView::Container(container), cell } => landing(inventory, drag, container, cell).map(|_| ()),
        Target::Slot(slot) => inventory.check_wear(drag.id, slot),
        Target::Doll => inventory.target_slot(drag.id).map_or(Err(Refusal::NotWearable), |slot| inventory.check_wear(drag.id, slot)),
        Target::Quickslot(_) => crate::quickslots::check_bind(inventory, drag.id),
        Target::Cell { view: GridView::Underfoot, .. } | Target::Page | Target::Level => Ok(()),
    }
}

/// Equips `id` in `slot` ([`Inventory::equip`]) and says what that changes in the level: the
/// item lifted if it lay there, and whatever it displaced set down if it found no room where
/// the item came from.
pub fn equip(inventory: &mut Inventory, id: ItemId, slot: Slot) -> Result<Vec<FloorChange>, Refusal> {
    let lying = inventory.location(id) == Some(Location::Loose);
    let displaced = inventory.equip(id, slot)?;
    let mut changes: Vec<FloorChange> = lying.then_some(FloorChange::Lifted(id)).into_iter().collect();
    if let Some((old, Displaced::Underfoot)) = displaced {
        changes.push(FloorChange::SetDown(old, 0.0));
    }
    Ok(changes)
}

/// What equipping an item straight from the floor did ([`equip_from_floor`]): the slot it went
/// in, what it displaced and the bag that went into (`None` if it was set down where the item
/// lay), and the changes to make to the level.
#[derive(Clone, Debug, PartialEq)]
pub struct FloorEquip {
    pub slot: Slot,
    pub displaced: Option<(ItemId, Option<ItemId>)>,
    pub changes: Vec<FloorChange>,
}

impl FloorEquip {
    /// The line the toast shows for it: what is now worn or held, and where what it displaced
    /// went.
    pub fn message(&self, inventory: &Inventory, id: ItemId) -> String {
        let title = |id| inventory.item(id).map_or_else(String::new, |item| item.title().to_string());
        let mut line = match self.slot {
            Slot::Mainhand | Slot::Offhand => format!("Holding {}.", title(id)),
            Slot::Hip => format!("{} hangs from the belt.", title(id)),
            _ => format!("Wearing {}.", title(id)),
        };
        match self.displaced {
            Some((old, Some(bag))) => line += &format!(" {} went into the {}.", title(old), title(bag)),
            Some((old, None)) => line += &format!(" {} was set down.", title(old)),
            None => {}
        }
        line
    }
}

/// Equips `id`, lying on the floor at `at` (cells) turned `yaw`, in the slot it is made for
/// ([`Inventory::target_slot`]: worn gear in its slot, a weapon in the main hand or else a
/// free off hand, a torch,
/// lantern or shield in the off hand, a ring in a free ring slot) by [`equip`]. Whatever it
/// displaces is stowed in a carried bag if one has room, or else set down where `id` lay.
/// Equipping from the floor without the screen is the user's call; where the displaced item
/// goes is this build's own rule.
pub fn equip_from_floor(inventory: &mut Inventory, id: ItemId, at: Vec2, yaw: f32) -> Result<FloorEquip, Refusal> {
    let slot = inventory.target_slot(id).ok_or(Refusal::NotWearable)?;
    equip_from_floor_in(inventory, id, slot, at, yaw)
}

/// [`equip_from_floor`] into `slot` rather than the one it is made for: Hang at hip sends a
/// lantern to the hip ([`crate::interact::Verb::HangAtHip`]).
pub fn equip_from_floor_in(inventory: &mut Inventory, id: ItemId, slot: Slot, at: Vec2, yaw: f32) -> Result<FloorEquip, Refusal> {
    let mut changes = equip(inventory, id, slot)?;
    let slot = match inventory.location(id) {
        Some(Location::Worn(worn)) => worn,
        _ => slot,
    };
    let mut displaced = None;
    for change in &mut changes {
        if let FloorChange::SetDown(old, _) = *change {
            match inventory.stow(old) {
                Ok(bag) => displaced = Some((old, Some(bag))),
                Err(_) => {
                    displaced = Some((old, None));
                    *change = FloorChange::SetDownAt(old, at, yaw);
                }
            }
        }
    }
    changes.retain(|change| !matches!(change, FloorChange::SetDown(..)));
    Ok(FloorEquip { slot, displaced, changes })
}

/// A double-click on `id` (the plate's `quick`): something worn is taken off and stowed, or set
/// down underfoot if there is no room for it; a purse is opened ([`open_purse`]); something
/// wearable is equipped in its slot ([`equip`]); anything else lying underfoot, or in a
/// container not carried (a chest standing in the level, a bag set down), is taken into the
/// carried grids. Taking from a container on a double-click is this build's own rule.
pub fn quick(inventory: &mut Inventory, id: ItemId) -> Result<Vec<FloorChange>, Refusal> {
    let item = inventory.item(id).ok_or(Refusal::Unknown)?;
    let wearable = item.profile.slot.is_some();
    let purse = item.coins.is_some();
    match inventory.location(id) {
        Some(Location::Worn(_)) => {
            inventory.take_out(id);
            Ok(match inventory.stow(id) {
                Ok(_) => vec![],
                Err(_) => vec![FloorChange::SetDown(id, 0.0)],
            })
        }
        _ if purse => open_purse(inventory, id),
        _ if wearable => {
            let slot = inventory.target_slot(id).ok_or(Refusal::NotWearable)?;
            equip(inventory, id, slot)
        }
        _ => take_up(inventory, id),
    }
}

/// Empties purse `id` into the delver's coin purse ([`Inventory::coins`]) and forgets it,
/// wherever it is: in a pack, in a chest, or lying in the level, which it is lifted from.
pub fn open_purse(inventory: &mut Inventory, id: ItemId) -> Result<Vec<FloorChange>, Refusal> {
    let coins = inventory.item(id).and_then(|i| i.coins).ok_or(Refusal::Unknown)?;
    let lying = inventory.location(id) == Some(Location::Loose);
    inventory.add_coins(coins);
    inventory.forget(id);
    Ok(lying.then_some(FloorChange::Lifted(id)).into_iter().collect())
}

/// What the toast says when a purse holding `coins` is opened.
pub fn opened_line(coins: fb_items::Coins) -> String {
    if coins.is_empty() { "The purse is empty.".to_string() } else { format!("You empty the purse: {}.", coins.short()) }
}

/// Takes `id` into the carried grids if it is not carried: lying underfoot it is picked up
/// ([`Inventory::pick_up`], lifted from the level), and in a container not carried (a chest
/// standing in the level, a bag set down) it is stowed ([`Inventory::stow`]). Something
/// carried or worn stays where it is. Refused (no room), it stays where it was.
pub fn take_up(inventory: &mut Inventory, id: ItemId) -> Result<Vec<FloorChange>, Refusal> {
    match inventory.location(id) {
        Some(Location::Loose) => inventory.pick_up(id).map(|()| vec![FloorChange::Lifted(id)]),
        Some(Location::In { .. }) if !inventory.is_carried(id) => inventory.stow(id).map(|_| vec![]),
        Some(_) => Ok(vec![]),
        None => Err(Refusal::Unknown),
    }
}

/// Lets `drag` go at `target`, and says what that changes in the level: an item lying there
/// put into a bag is lifted from it; anything let go on the underfoot grid or the level is
/// set down, to the side the underfoot column says (`underfoot_cols` wide).
pub fn drop_at(inventory: &mut Inventory, drag: &Drag, target: Target, underfoot_cols: u8) -> Result<Vec<FloorChange>, Refusal> {
    let lying = inventory.location(drag.id) == Some(Location::Loose);
    let set_down = |inventory: &mut Inventory, aside: f32| {
        if lying {
            return vec![];
        }
        inventory.take_out(drag.id);
        vec![FloorChange::SetDown(drag.id, aside)]
    };
    match target {
        Target::Cell { view: GridView::Container(container), cell } => {
            let at = landing(inventory, drag, container, cell)?;
            inventory.put(drag.id, container, at)?;
            Ok(lying.then_some(FloorChange::Lifted(drag.id)).into_iter().collect())
        }
        Target::Slot(slot) => equip(inventory, drag.id, slot),
        Target::Doll => equip(inventory, drag.id, inventory.target_slot(drag.id).ok_or(Refusal::NotWearable)?),
        Target::Cell { view: GridView::Underfoot, cell } => Ok(set_down(inventory, underfoot::aside_for_column(cell.x, underfoot_cols))),
        Target::Level => Ok(set_down(inventory, 0.0)),
        Target::Page | Target::Quickslot(_) => Ok(vec![]),
    }
}

/// The grid cell under `point`, for a grid of `cell`-pixel cells (`cell_size`,
/// [`crate::tuning::InterfaceTuning`]) drawn at `rect`.
pub fn cell_under(rect: Rect, point: Vec2, cell: f32) -> IVec2 {
    let (x, y) = grid::cell_at((point.x - rect.min.x - LINE_PX * 0.5, point.y - rect.min.y - LINE_PX * 0.5), cell);
    IVec2::new(x, y)
}

/// Builds the screen when it opens and rebuilds it whenever what is in it changes, or the
/// tuning does; takes it down when it closes.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn build_screen(
    mut commands: Commands,
    open: Res<ScreenOpen>,
    tab: Res<SheetTab>,
    inventory: Res<Inventory>,
    underfoot: Res<Underfoot>,
    showing: Res<OpenContainer>,
    screens: Query<Entity, With<InventoryScreen>>,
    cameras: Query<Entity, With<ViewModelCamera>>,
    fonts: Res<UiFonts>,
    doll_assets: Res<DollAssets>,
    character: Res<Character>,
    (tuning, scrolled, barter, shelf_scrolled, record, abyss): (Res<Tuning>, Res<ColumnScroll>, Res<crate::barter::Barter>, Res<ShelfScrolled>, Option<Res<crate::record::WorldRecord>>, Option<Res<crate::travel::Abyss>>),
    rules: Option<Res<crate::rules::GameRules>>,
    keymap: Option<Res<Keymap>>,
    players: Query<(&crate::character::DelverNeeds, &crate::exhaustion::Exhaustion, Option<&crate::character::DelverStamina>, Option<&crate::character::DelverMana>), With<crate::play::Player>>,
    mut icons: Icons,
) {
    let existing = screens.iter().next();
    if !open.is(Screen::Inventory) {
        if let Some(screen) = existing {
            commands.entity(screen).despawn();
        }
        return;
    }
    if existing.is_some() && !inventory.is_changed() && !underfoot.is_changed() && !showing.is_changed() && !tab.is_changed() && !character.is_changed() && !tuning.is_changed() && !barter.is_changed() && !(*tab == SheetTab::Factions && record.as_ref().is_some_and(|r| r.is_changed())) && !rules.as_ref().is_some_and(|r| r.is_changed()) && !keymap.as_ref().is_some_and(|k| k.is_changed()) {
        return;
    }
    // Drawn by the camera that draws over the held torch, as the HUD is.
    let Some(camera) = cameras.iter().next() else { return };
    if let Some(screen) = existing {
        commands.entity(screen).despawn();
    }
    let fallback = Keymap::default();
    let keymap = keymap.as_deref().unwrap_or(&fallback);
    let screen = ScreenLayout { inventory: &inventory, barter: &barter, fonts: &fonts, cell: tuning.interface.cell_size };
    commands
        .spawn((
            Name::new("Inventory"),
            InventoryScreen,
            UiTargetCamera(camera),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            // The screen is the page and covers the level. The ledger has no edge of its own;
            // letting go of an item on the page beyond it still sets it down in the level.
            BackgroundColor(BG),
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((
                Ledger,
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: px(28),
                    padding: UiRect::all(px(22)),
                    max_width: percent(94),
                    // One height on every tab, so the header and tabs sit at the same place
                    // whatever the page holds; the pages scroll inside it.
                    height: if barter.active() { Val::Auto } else { percent(92) },
                    max_height: percent(92),
                    ..default()
                },
            ))
            .with_children(|ledger| {
                ledger
                    // Wide enough for a pack and a belt pouch side by side, and for underfoot
                    // and a bag set down beside it.
                    .spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(12), min_width: px(420), max_width: px(if barter.active() { BARTER_LEAF_MAX_PX } else { LEAF_MAX_PX }), width: if barter.active() { Val::Auto } else { px(LEAF_PX) }, ..default() }, LeafPage))
                    .with_children(|leaf| {
                        header::header(leaf, &fonts, crate::rules::GameRules::of(&rules), &character, &inventory, doll_assets.portrait.clone(), *tab);
                        if *tab != SheetTab::Inventory {
                            // The page scrolls under the header, whatever the window.
                            leaf.spawn((
                                ScrollColumn,
                                ScrollPosition(Vec2::new(0.0, scrolled.0)),
                                Node { flex_direction: FlexDirection::Column, min_height: px(0), padding: UiRect::right(px(8)), overflow: Overflow::scroll_y(), ..default() },
                            ))
                            .with_children(|page| {
                                if *tab == SheetTab::Factions {
                                    let listed = record.as_deref().zip(abyss.as_deref()).map(|(r, a)| factions::entries(r, a.run_seed)).unwrap_or_default();
                                    factions::page(page, &fonts, &mut icons, &listed);
                                    return;
                                }
                                let (needs, exhaustion, vitals) = players.single().map_or_else(
                                    |_| Default::default(),
                                    |(needs, exhaustion, stamina, mana)| (needs.0, exhaustion.0, sheet::Vitals { stamina: stamina.map_or_else(Default::default, |s| s.0), mana: mana.map_or_else(Default::default, |m| m.0) }),
                                );
                                sheet::sheet(page, &fonts, crate::rules::GameRules::of(&rules), &character, &inventory, &needs, exhaustion, vitals);
                            });
                            return;
                        }
                        // Scrolls when the grids are taller than the screen; the header stays.
                        leaf.spawn((
                            ScrollColumn,
                            ScrollPosition(Vec2::new(0.0, scrolled.0)),
                            Node { flex_direction: FlexDirection::Column, row_gap: px(16), min_height: px(0), overflow: Overflow::scroll_y(), ..default() },
                        ))
                        .with_children(|leaf| {
                        leaf.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(18), row_gap: px(16), align_items: AlignItems::FlexStart, flex_shrink: 0.0, ..default() })
                            .with_children(|row| {
                                let carried = screen.inventory.carried_containers();
                                if carried.is_empty() {
                                    row.spawn((
                                        Text::new("Nothing to carry things in. Find a pack, a sack or a belt with a pouch."),
                                        UiFonts::style(&fonts.body, 14.0, MUTED),
                                    ));
                                }
                                // The trader's shelf before the pack, with the balance above it.
                                if let Some(shelf) = barter.session.as_ref().map(|s| s.shelf).filter(|&s| showing.0 == Some(s)) {
                                    row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(10), ..default() }).with_children(|column| {
                                        if let Some(grid) = inventory.grid(shelf) {
                                            crate::barter::panel(column, &fonts, &barter, &inventory, grid_size(grid, screen.cell).x);
                                        }
                                        screen.container_in(column, &mut icons, shelf, GridView::Container(shelf), "", Some(shelf_scrolled.0));
                                    });
                                }
                                if barter.active() {
                                    // Stacked beside the shelf, not wrapped under it.
                                    row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(16), ..default() }).with_children(|stack| {
                                        for container in carried {
                                            screen.container(stack, &mut icons, container, GridView::Container(container), "");
                                        }
                                    });
                                } else {
                                    for container in carried {
                                        screen.container(row, &mut icons, container, GridView::Container(container), "");
                                    }
                                }
                            });
                        leaf.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(18), row_gap: px(16), align_items: AlignItems::FlexStart, flex_shrink: 0.0, ..default() })
                            .with_children(|row| {
                                if let Some(container) = showing.0.filter(|&c| !barter.session.as_ref().is_some_and(|s| s.shelf == c)) {
                                    screen.container(row, &mut icons, container, GridView::Container(container), "");
                                }
                                // Nothing lies about in the trader's stall.
                                if !barter.active() {
                                    screen.underfoot(row, &mut icons, &underfoot);
                                    for &bag in &underfoot.bags {
                                        screen.container(row, &mut icons, bag, GridView::Container(bag), " \u{b7} underfoot");
                                    }
                                }
                            });
                        });
                    });
                // The doll, with the quickslots and the help under it, leaving the left leaf's
                // height to the grids.
                ledger
                    .spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(12), width: px(doll::FRAME.x), ..default() })
                    .with_children(|leaf| {
                        doll::doll_leaf(leaf, &doll_assets, &inventory, &fonts, &mut icons);
                        if *tab != SheetTab::Inventory {
                            return;
                        }
                        leaf.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }).with_children(|column| {
                            column.spawn((Text::new("quickslots"), UiFonts::style(&fonts.display, 18.0, INK)));
                            crate::quickslots::strip(column, &fonts);
                        });
                        leaf.spawn((Text::new(help_text(keymap)), UiFonts::style(&fonts.body, 14.0, MUTED)));
                    });
            });
            root.spawn((
                DragGhost,
                Node { position_type: PositionType::Absolute, ..default() },
                ImageNode::default(),
                Visibility::Hidden,
                GlobalZIndex(30),
            ));
            root.spawn((
                DropHint,
                Node { position_type: PositionType::Absolute, border: UiRect::all(px(2)), border_radius: BorderRadius::all(px(6)), ..default() },
                Visibility::Hidden,
                GlobalZIndex(29),
            ));
        });
}

/// The price tag in an item's cell while bartering.
#[derive(Component, Debug)]
pub struct PriceTag;

/// What a cell's price tag says, its ink and its edge, for a barter `price`: a ware's cost in
/// its coin's metal on a gold edge (red ink if the delver cannot pay it), what the trader pays
/// on a green one. `None` for a price that is not a trade's.
pub fn cell_tag(price: card::Price) -> Option<(String, Color, Color)> {
    match price {
        card::Price::Costs { price, afford, .. } => {
            let (text, metal) = crate::ui::coin_tag(price.total_copper());
            Some((text, if afford { metal } else { BAD }, crate::ui::FRAME_GOLD))
        }
        card::Price::SellsFor { price, .. } => {
            let (text, metal) = crate::ui::coin_tag(price.total_copper());
            Some((text, metal, OK))
        }
        card::Price::Worth(_) | card::Price::Holds(_) => None,
    }
}

/// What the screen is built from.
struct ScreenLayout<'a> {
    inventory: &'a Inventory,
    /// While bartering, what tags the items' cells with their prices ([`cell_tag`]).
    barter: &'a crate::barter::Barter,
    fonts: &'a UiFonts,
    /// Logical pixels from one grid line to the next.
    cell: f32,
}

impl ScreenLayout<'_> {
    /// The floor within reach, as a paler grid under its name, always there to drop things on;
    /// bare, its empty cells and a line under the name that says so.
    fn underfoot(&self, parent: &mut ChildSpawnerCommands, icons: &mut Icons, underfoot: &Underfoot) {
        parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }).with_children(|column| {
            column.spawn(Node { align_items: AlignItems::Center, column_gap: px(10), height: px(36), ..default() }).with_children(|head| {
                head.spawn((Text::new("underfoot"), UiFonts::style(&self.fonts.display, 18.0, INK)));
                if underfoot.grid.is_empty() {
                    head.spawn((Text::new("Bare stone. Drop things here."), UiFonts::style(&self.fonts.body, 14.0, MUTED)));
                }
            });
            self.grid(column, icons, GridView::Underfoot, &underfoot.grid, true);
        });
    }

    /// A container's grid under its name, with a small picture of it.
    fn container(&self, parent: &mut ChildSpawnerCommands, icons: &mut Icons, id: ItemId, view: GridView, suffix: &str) {
        self.container_in(parent, icons, id, view, suffix, None);
    }

    /// [`ScreenLayout::container`], with the grid in a scroll area of its own at `scroll` pixels
    /// down if that is given (the trader's shelf).
    fn container_in(&self, parent: &mut ChildSpawnerCommands, icons: &mut Icons, id: ItemId, view: GridView, suffix: &str, scroll: Option<f32>) {
        let (Some(item), Some(grid)) = (self.inventory.item(id), self.inventory.grid(id)) else { return };
        let label = item.profile.holds.as_ref().map_or_else(|| item.name.clone(), |h| h.label.clone());
        parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }).with_children(|column| {
            column.spawn(Node { align_items: AlignItems::Center, column_gap: px(8), height: px(36), ..default() }).with_children(|head| {
                let size = icon::icon_size(item, false).as_vec2();
                let size = size * (34.0 / size.max_element());
                icons.spawn(head, item, false, Node { width: px(size.x), height: px(size.y), ..default() }, size.min_element().max(14.0));
                head.spawn((Text::new(format!("{label}{suffix}")), UiFonts::style(&self.fonts.display, 18.0, INK)));
            });
            match scroll {
                None => self.grid(column, icons, view, grid, false),
                Some(y) => {
                    let size = grid_size(grid, self.cell);
                    column
                        .spawn((
                            ShelfScroll,
                            ScrollPosition(Vec2::new(0.0, y)),
                            Node { width: px(size.x), height: px(shelf_view_height(size.y)), flex_shrink: 0.0, overflow: Overflow::scroll_y(), ..default() },
                        ))
                        .with_children(|area| self.grid(area, icons, view, grid, false));
                }
            }
        });
    }

    /// A grid of cells with its items drawn at their footprints. `faded` draws the cells paler,
    /// as for what lies underfoot.
    fn grid(&self, parent: &mut ChildSpawnerCommands, icons: &mut Icons, view: GridView, grid: &Grid<ItemId>, faded: bool) {
        let size = grid_size(grid, self.cell);
        let cell = self.cell;
        parent
            .spawn((
                view,
                Node {
                    width: px(size.x),
                    height: px(size.y),
                    border_radius: BorderRadius::all(px(GRID_RADIUS_PX)),
                    ..default()
                },
                BackgroundColor(LINE),
            ))
            .with_children(|cells| {
                let cell_colour = if faded { CELL.mix(&BG, 0.5) } else { CELL };
                for y in 0..grid.rows {
                    for x in 0..grid.cols {
                        let round = |at_x: bool, at_y: bool| CornerRadius::from(if at_x && at_y { px(GRID_RADIUS_PX - LINE_PX) } else { px(0) });
                        let (left, top, right, bottom) = (x == 0, y == 0, x + 1 == grid.cols, y + 1 == grid.rows);
                        cells.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(x as f32 * cell + LINE_PX),
                                top: px(y as f32 * cell + LINE_PX),
                                width: px(cell - LINE_PX),
                                height: px(cell - LINE_PX),
                                border_radius: BorderRadius {
                                    top_left: round(left, top),
                                    top_right: round(right, top),
                                    bottom_left: round(left, bottom),
                                    bottom_right: round(right, bottom),
                                },
                                ..default()
                            },
                            BackgroundColor(cell_colour),
                        ));
                    }
                }
                for entry in grid.entries() {
                    let Some(item) = self.inventory.item(entry.id) else { continue };
                    cells
                        .spawn((
                            ItemView { id: entry.id },
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(entry.at.x as f32 * cell + LINE_PX),
                                top: px(entry.at.y as f32 * cell + LINE_PX),
                                width: px(entry.size.cols as f32 * cell - LINE_PX),
                                height: px(entry.size.rows as f32 * cell - LINE_PX),
                                padding: UiRect::all(px(3)),
                                border_radius: BorderRadius::all(px(6)),
                                ..default()
                            },
                            BackgroundColor(Color::NONE),
                        ))
                        .with_children(|slot| {
                            let glyph = (entry.size.cols.min(entry.size.rows) as f32 * cell * 0.5).min(40.0);
                            icons.spawn(slot, item, entry.at.turned, Node { width: percent(100), height: percent(100), ..default() }, glyph).insert(ItemIcon);
                            if item.profile.kind == Kind::Gear && item.rarity != Rarity::Common {
                                slot.spawn((
                                    Node {
                                        position_type: PositionType::Absolute,
                                        left: px(4),
                                        top: px(4),
                                        width: px(7),
                                        height: px(7),
                                        border_radius: BorderRadius::MAX,
                                        ..default()
                                    },
                                    BackgroundColor(rarity_color(item.rarity)),
                                ));
                            }
                            if let Some((text, ink, edge)) = self.barter.tag(self.inventory, entry.id).and_then(cell_tag) {
                                slot.spawn((
                                    PriceTag,
                                    Node {
                                        position_type: PositionType::Absolute,
                                        right: px(3),
                                        bottom: px(3),
                                        padding: UiRect::new(px(4), px(4), px(0), px(1)),
                                        border: UiRect::all(px(1)),
                                        border_radius: BorderRadius::all(px(4)),
                                        ..default()
                                    },
                                    BackgroundColor(BG.with_alpha(0.88)),
                                    BorderColor::all(edge),
                                ))
                                .with_child((Text::new(text), UiFonts::style(&self.fonts.mono_bold, 11.0, ink)));
                            }
                        });
                }
            });
    }
}

/// A grid's size on the page, in logical pixels, with cells `cell` pixels apart: its cells and
/// the lines round them.
pub fn grid_size<T>(grid: &Grid<T>, cell: f32) -> Vec2 {
    Vec2::new(grid.cols as f32, grid.rows as f32) * cell + LINE_PX
}

/// The slot, grid cell or doll under `point`, the page, or the level beyond it.
fn target_under(
    point: Vec2,
    drops: &Query<(&QuickslotDrop, &ComputedNode, &UiGlobalTransform)>,
    grids: &Query<(&GridView, &ComputedNode, &UiGlobalTransform)>,
    slots: &Query<(&SlotView, &ComputedNode, &UiGlobalTransform)>,
    doll: Option<Rect>,
    ledger: Option<Rect>,
    column: Option<Rect>,
    shelf: Option<(GridView, Rect)>,
    cell: f32,
) -> Target {
    if let Some((slot, ..)) = drops.iter().find(|(_, node, transform)| node_rect(node, transform).contains(point)) {
        return Target::Quickslot(slot.0);
    }
    if let Some((slot, ..)) = slots.iter().find(|(_, node, transform)| node_rect(node, transform).contains(point)) {
        return Target::Slot(slot.0);
    }
    // The grids' rectangles follow the scroll (`UiGlobalTransform` includes it), but a grid
    // scrolled out of the column is clipped, so only the visible part of the column counts.
    for (view, node, transform) in grids {
        let rect = node_rect(node, transform);
        let in_shelf = shelf.is_none_or(|(shelved, area)| shelved != *view || area.contains(point));
        if rect.contains(point) && column.is_none_or(|c| c.contains(point)) && in_shelf {
            return Target::Cell { view: *view, cell: cell_under(rect, point, cell) };
        }
    }
    if doll.is_some_and(|rect| rect.contains(point)) {
        return Target::Doll;
    }
    match ledger {
        Some(page) if !page.contains(point) => Target::Level,
        _ => Target::Page,
    }
}

/// Starts a page at its top when the tab changes, instead of where the last page was scrolled.
fn reset_scroll_on_tab(tab: Res<SheetTab>, mut remembered: ResMut<ColumnScroll>) {
    if tab.is_changed() {
        // `FB_INVENTORY_SCROLL=<px>` starts the page scrolled that far, for captures.
        let start = std::env::var("FB_INVENTORY_SCROLL").ok().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0);
        remembered.set_if_neq(ColumnScroll(start));
    }
}

/// Scrolls the left column with the wheel over it, and while an item is held near its top or
/// bottom edge, so one can be carried to a bag out of sight.
#[allow(clippy::too_many_arguments)]
fn scroll_column(
    open: Res<ScreenOpen>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    time: Res<Time<Real>>,
    dragging: Res<Dragging>,
    mut remembered: ResMut<ColumnScroll>,
    mut columns: Query<(&mut ScrollPosition, &ComputedNode, &UiGlobalTransform), With<ScrollColumn>>,
    shelf: Query<(&ComputedNode, &UiGlobalTransform), With<ShelfScroll>>,
) {
    if !open.is(Screen::Inventory) {
        remembered.set_if_neq(ColumnScroll(0.0));
        return;
    }
    let Ok((mut position, node, transform)) = columns.single_mut() else { return };
    // Not laid out yet: leave the remembered place alone.
    if node.size().y <= 0.0 {
        return;
    }
    let max = ((node.content_size() - node.size()) * node.inverse_scale_factor).y.max(0.0);
    let rect = node_rect(node, transform);
    let point = window.and_then(|w| ui_cursor(&w, ui_scale.as_deref()));
    let mut by = 0.0;
    // Over the shelf's own area, the shelf scrolls ([`scroll_shelf`]), not the column.
    let over_shelf = point.is_some_and(|p| shelf.iter().any(|(n, t)| node_rect(n, t).contains(p)));
    if let Some(point) = point
        && !over_shelf
        && point.x >= rect.min.x
        && point.x <= rect.max.x
    {
        let lines = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        if lines != 0.0 && rect.contains(point) {
            by -= lines * SCROLL_LINE_PX;
        }
        if dragging.0.is_some_and(|d| d.active) {
            // Deeper into the edge zone, or past the edge, scrolls faster.
            let top = (EDGE_SCROLL_PX - (point.y - rect.min.y)) / EDGE_SCROLL_PX;
            let bottom = (EDGE_SCROLL_PX - (rect.max.y - point.y)) / EDGE_SCROLL_PX;
            by += (bottom.clamp(0.0, 1.0) - top.clamp(0.0, 1.0)) * EDGE_SCROLL_SPEED * time.delta_secs();
        }
    }
    let to = (position.y + by).clamp(0.0, max);
    if to != position.y {
        position.y = to;
    }
    remembered.set_if_neq(ColumnScroll(to));
}

/// Scrolls the trader's shelf with the wheel over it, and while an item is held near its top or
/// bottom edge, as [`scroll_column`] does the left column.
#[allow(clippy::too_many_arguments)]
fn scroll_shelf(
    open: Res<ScreenOpen>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    time: Res<Time<Real>>,
    dragging: Res<Dragging>,
    mut remembered: ResMut<ShelfScrolled>,
    mut areas: Query<(&mut ScrollPosition, &ComputedNode, &UiGlobalTransform), With<ShelfScroll>>,
) {
    if !open.is(Screen::Inventory) {
        remembered.set_if_neq(ShelfScrolled(0.0));
        return;
    }
    let Ok((mut position, node, transform)) = areas.single_mut() else { return };
    if node.size().y <= 0.0 {
        return;
    }
    let max = ((node.content_size() - node.size()) * node.inverse_scale_factor).y.max(0.0);
    let rect = node_rect(node, transform);
    let point = window.and_then(|w| ui_cursor(&w, ui_scale.as_deref()));
    let mut by = 0.0;
    if let Some(point) = point
        && point.x >= rect.min.x
        && point.x <= rect.max.x
    {
        let lines = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        if lines != 0.0 && rect.contains(point) {
            by -= lines * SCROLL_LINE_PX;
        }
        if dragging.0.is_some_and(|d| d.active) {
            let top = (EDGE_SCROLL_PX - (point.y - rect.min.y)) / EDGE_SCROLL_PX;
            let bottom = (EDGE_SCROLL_PX - (rect.max.y - point.y)) / EDGE_SCROLL_PX;
            by += (bottom.clamp(0.0, 1.0) - top.clamp(0.0, 1.0)) * EDGE_SCROLL_SPEED * time.delta_secs();
        }
    }
    let to = (position.y + by).clamp(0.0, max);
    if to != position.y {
        position.y = to;
    }
    remembered.set_if_neq(ShelfScrolled(to));
}

/// The grid a view shows.
fn grid_of<'a>(inventory: &'a Inventory, underfoot: &'a Underfoot, view: GridView) -> Option<&'a Grid<ItemId>> {
    match view {
        GridView::Container(id) => inventory.grid(id),
        GridView::Underfoot => Some(&underfoot.grid),
    }
}

/// Presses, drags, turns and drops items, and notes which one the mouse is over.
#[allow(clippy::too_many_arguments)]
fn drag_items(
    open: Res<ScreenOpen>,
    input: ActionInput,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    grids: Query<(&GridView, &ComputedNode, &UiGlobalTransform)>,
    slots: Query<(&SlotView, &ComputedNode, &UiGlobalTransform)>,
    ledgers: Query<(&ComputedNode, &UiGlobalTransform), With<Ledger>>,
    dolls: Query<(&ComputedNode, &UiGlobalTransform), With<DollView>>,
    mut dragging: ResMut<Dragging>,
    mut hovered: ResMut<Hovered>,
    mut inventory: ResMut<Inventory>,
    underfoot: Res<Underfoot>,
    mut floor_changes: ResMut<FloorChanges>,
    mut toast: ResMut<Toast>,
    (time, tuning, ui_scale, mut barter): (Res<Time<Real>>, Res<Tuning>, Option<Res<UiScale>>, ResMut<crate::barter::Barter>),
    (drops, mut quickslots, columns, shelf_areas): (
        Query<(&QuickslotDrop, &ComputedNode, &UiGlobalTransform)>,
        ResMut<Quickslots>,
        Query<(&ComputedNode, &UiGlobalTransform), With<ScrollColumn>>,
        Query<(&ComputedNode, &UiGlobalTransform), With<ShelfScroll>>,
    ),
    mut last_click: Local<Option<(ItemId, f32)>>,
) {
    let cell_px = tuning.interface.cell_size;
    let cursor = window.and_then(|w| ui_cursor(&w, ui_scale.as_deref()));
    if !open.is(Screen::Inventory) {
        dragging.0 = None;
        return;
    }
    let Some(point) = cursor else { return };
    let ledger = ledgers.iter().next().map(|(n, t)| node_rect(n, t));
    let doll = dolls.iter().next().map(|(n, t)| node_rect(n, t));
    let column = columns.iter().next().map(|(n, t)| node_rect(n, t));
    // The shelf's grid is clipped to its own scroll area.
    let shelf = barter.session.as_ref().zip(shelf_areas.iter().next()).map(|(s, (n, t))| (GridView::Container(s.shelf), node_rect(n, t)));
    let target = target_under(point, &drops, &grids, &slots, doll, ledger, column, shelf, cell_px);

    // What the mouse is over: an item in a grid, or one worn in a slot, with its rectangle
    // and how it would be taken up.
    let over: Option<(Rect, Drag)> = match target {
        Target::Cell { view, cell } => grid_of(&inventory, &underfoot, view).and_then(|g| g.at_cell(cell.x, cell.y).copied()).and_then(|entry| {
            let (_, node, transform) = grids.iter().find(|(v, ..)| **v == view)?;
            let min = node_rect(node, transform).min + Vec2::new(entry.at.x as f32, entry.at.y as f32) * cell_px + LINE_PX;
            let rect = Rect::from_corners(min, min + Vec2::new(entry.size.cols as f32, entry.size.rows as f32) * cell_px - LINE_PX);
            let grab = cell - IVec2::new(entry.at.x as i32, entry.at.y as i32);
            Some((rect, Drag { id: entry.id, grab, size: entry.size, turned: entry.at.turned, press: point, active: false, target: None }))
        }),
        Target::Slot(slot) => inventory.worn(slot).and_then(|id| {
            let (_, node, transform) = slots.iter().find(|(s, ..)| s.0 == slot)?;
            let size = inventory.footprint(id)?;
            Some((node_rect(node, transform), Drag { id, grab: IVec2::ZERO, size, turned: false, press: point, active: false, target: None }))
        }),
        _ => None,
    };
    let now_over = if dragging.0.is_some_and(|d| d.active) { None } else { over.map(|(rect, d)| (d.id, rect)) };
    hovered.set_if_neq(Hovered(now_over));

    if input.just_pressed(Action::DragItem)
        && let Some((_, drag)) = over
    {
        dragging.0 = Some(drag);
    }
    let Some(drag) = dragging.0.as_mut() else { return };
    if !drag.active && point.distance(drag.press) >= DRAG_START_PX {
        drag.active = true;
    }
    if drag.active && input.just_pressed(Action::TurnItem) {
        drag.turn();
    }
    drag.target = drag.active.then(|| (target, barter.check(&inventory, drag, target).unwrap_or_else(|| check_drop(&inventory, drag, target))));
    if input.just_released(Action::DragItem) {
        let drag = dragging.0.take().expect("a drag");
        let now = time.elapsed_secs();
        // A quickslot takes a binding, not the item.
        if drag.active
            && let Target::Quickslot(slot) = target
        {
            let said = crate::quickslots::bind_dropped(&mut quickslots, &inventory, drag.id, slot).unwrap_or_else(|refusal| refusal.reason().to_string());
            toast.say(said, now);
            return;
        }
        let purse = inventory.item(drag.id).and_then(|i| i.coins);
        let outcome = if drag.active {
            (if barter.active() { barter.drop_at(&mut inventory, &drag, target) } else { None }).unwrap_or_else(|| drop_at(&mut inventory, &drag, target, underfoot.grid.cols))
        } else if last_click.take().is_some_and(|(id, at)| id == drag.id && now - at <= DOUBLE_CLICK_SECONDS) {
            (if barter.active() { barter.quick(&mut inventory, drag.id) } else { None }).unwrap_or_else(|| quick(&mut inventory, drag.id))
        } else {
            *last_click = Some((drag.id, now));
            return;
        };
        match outcome {
            Ok(changes) => {
                if let Some(coins) = purse.filter(|_| inventory.item(drag.id).is_none()) {
                    toast.say(opened_line(coins), now);
                }
                floor_changes.0.extend(changes);
            }
            Err(refusal) => {
                toast.say(refusal.reason(), now);
            }
        }
    }
}

/// With no belt worn, what hung at the hip comes off ([`Inventory::unhang`]): into a carried bag
/// with room, or else set down underfoot. Every way of taking a belt off (a double-click, a drag
/// to a grid or the floor, swapping in something that is not a belt) ends here.
fn unhang_without_belt(mut inventory: ResMut<Inventory>, mut floor_changes: ResMut<FloorChanges>) {
    if inventory.worn(Slot::Hip).is_none() || inventory.worn(Slot::Belt).is_some() {
        return;
    }
    if let Some((id, Displaced::Underfoot)) = inventory.unhang() {
        floor_changes.0.push(FloorChange::SetDown(id, 0.0));
    }
}

/// For captures: holds the card over the `FB_DEBUG_HOVER`th thing in the first carried grid,
/// as if the mouse were on it; `u<n>` takes the `n`th thing underfoot instead, and `o<n>` the
/// `n`th in the open container (a chest, or a trader's shelf).
fn debug_hover(
    inventory: Res<Inventory>,
    underfoot: Res<Underfoot>,
    open: Res<OpenContainer>,
    views: Query<(&ItemView, &ComputedNode, &UiGlobalTransform)>,
    mut hovered: ResMut<Hovered>,
) {
    let spec = std::env::var("FB_DEBUG_HOVER").unwrap_or_default();
    let (place, number) = match spec.chars().next() {
        Some(c @ ('u' | 'o')) => (Some(c), &spec[1..]),
        _ => (None, spec.as_str()),
    };
    let index: usize = number.parse().unwrap_or(0);
    let nth_in = |container: Option<ItemId>| container.and_then(|c| inventory.grid(c)).and_then(|g| g.entries().get(index)).map(|e| e.id);
    let id = match place {
        Some('u') => underfoot.grid.entries().get(index).map(|e| e.id),
        Some(_) => nth_in(open.0),
        None => nth_in(inventory.carried_containers().first().copied()),
    };
    let Some(id) = id else {
        return;
    };
    if let Some((_, node, transform)) = views.iter().find(|(v, ..)| v.id == id) {
        let rect = node_rect(node, transform);
        if rect.width() > 0.0 {
            hovered.set_if_neq(Hovered(Some((id, rect))));
        }
    }
}

/// Draws the drag: the item following the mouse, the cells it would cover, and its old place
/// faded; and lightens the item the mouse is over.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn show_drag(
    dragging: Res<Dragging>,
    hovered: Res<Hovered>,
    inventory: Res<Inventory>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    grids: Query<(&GridView, &ComputedNode, &UiGlobalTransform)>,
    mut ghosts: Query<(&mut Node, &mut ImageNode, &mut Visibility), (With<DragGhost>, Without<DropHint>)>,
    mut hints: Query<(&mut Node, &mut BackgroundColor, &mut BorderColor, &mut Visibility), (With<DropHint>, Without<DragGhost>)>,
    mut views: Query<(&ItemView, &mut BackgroundColor, &Children), Without<DropHint>>,
    mut item_icons: Query<&mut ImageNode, (With<ItemIcon>, Without<DragGhost>)>,
    mut slot_borders: Query<(&SlotView, &mut BorderColor), (Without<DropHint>, Without<ItemView>)>,
    (tuning, ui_scale): (Res<Tuning>, Option<Res<UiScale>>),
    mut icons: Icons,
) {
    let cell_px = tuning.interface.cell_size;
    let drag = dragging.0.filter(|d| d.active);
    let cursor = window.and_then(|w| ui_cursor(&w, ui_scale.as_deref()));
    for (mut node, mut image, mut visibility) in &mut ghosts {
        let (Some(drag), Some(point), Some(item)) = (drag, cursor, drag.and_then(|d| inventory.item(d.id))) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let handle = icons.of(item, drag.turned);
        if image.image != handle {
            image.image = handle;
        }
        let at = point - (drag.grab.as_vec2() + 0.5) * cell_px;
        node.left = px(at.x);
        node.top = px(at.y);
        node.width = px(drag.size.cols as f32 * cell_px);
        node.height = px(drag.size.rows as f32 * cell_px);
        *visibility = Visibility::Inherited;
    }
    for (mut node, mut background, mut border, mut visibility) in &mut hints {
        let shown = drag.and_then(|d| d.target.map(|t| (d, t))).and_then(|(d, (target, fits))| match target {
            Target::Cell { view, cell } => grids.iter().find(|(v, ..)| **v == view).map(|(_, n, t)| (d, node_rect(n, t), cell, fits)),
            _ => None,
        });
        let Some((drag, rect, cell, fits)) = shown else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let at = (cell - drag.grab).max(IVec2::ZERO).as_vec2();
        let min = rect.min + at * cell_px + LINE_PX;
        node.left = px(min.x);
        node.top = px(min.y);
        node.width = px(drag.size.cols as f32 * cell_px - LINE_PX);
        node.height = px(drag.size.rows as f32 * cell_px - LINE_PX);
        let colour = if fits.is_ok() { OK } else { BAD };
        background.0 = colour.with_alpha(if fits.is_ok() { 0.3 } else { 0.26 });
        *border = BorderColor::all(colour);
        *visibility = Visibility::Inherited;
    }
    // A slot under a dragged item shows whether it may go there, green or red, as the
    // plate's `drop-ok` and `drop-bad`; otherwise filled slots are cream and empty ones tan.
    for (slot, mut border) in &mut slot_borders {
        let hint = drag.and_then(|d| d.target).and_then(|(target, fits)| match target {
            Target::Slot(s) if s == slot.0 => Some(if fits.is_ok() { OK } else { BAD }),
            _ => None,
        });
        let colour = hint.unwrap_or(if inventory.worn(slot.0).is_some() { BRIGHT } else { LINE });
        if border.top != colour {
            *border = BorderColor::all(colour);
        }
    }
    for (view, mut background, children) in &mut views {
        let dragged = drag.is_some_and(|d| d.id == view.id);
        let over = hovered.0.is_some_and(|(id, _)| id == view.id);
        let wanted = if over { BRIGHT.with_alpha(0.1) } else { Color::NONE };
        if background.0 != wanted {
            background.0 = wanted;
        }
        for child in children {
            if let Ok(mut image) = item_icons.get_mut(*child) {
                let alpha = if dragged { 0.3 } else { 1.0 };
                if image.color.alpha() != alpha {
                    image.color = Color::WHITE.with_alpha(alpha);
                }
            }
        }
    }
}

fn show_toast(toast: Res<Toast>, time: Res<Time<Real>>, mut texts: Query<&mut Text, With<ToastText>>) {
    let shown = if time.elapsed_secs() < toast.until { toast.text.as_str() } else { "" };
    for mut text in &mut texts {
        if text.0 != shown {
            text.0 = shown.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_help_names_the_bound_keys() {
        use crate::keymap::Input;
        let help = help_text(&Keymap::default());
        assert!(help.contains("Right click while dragging") && help.contains("the keys 1 to 0 use it"), "{help}");
        let mut keymap = Keymap::default();
        keymap.bindings.iter_mut().find(|b| b.action == Action::TurnItem).unwrap().inputs = vec![Input::Key(KeyCode::KeyR)];
        assert!(help_text(&keymap).contains("R while dragging"));
    }

    use super::*;

    /// The cell size as shipped.
    const CELL_PX: f32 = 72.0;

    #[test]
    fn a_delve_begins_as_the_chosen_delver_or_rolls_one() {
        let mut app = App::new();
        app.init_resource::<Character>().add_systems(Update, roll_character);
        app.update();
        let rolled = app.world().resource::<Character>().clone();
        assert_eq!(rolled, character_for(kit_seed(None, 0xc4a7)));
        let chosen = Character { name: "Brakka".into(), folk: fb_body::Folk::Dwarf, class: crate::character::Class::Cleric, ..Character::roll(11) };
        app.insert_resource(ChosenDelver(chosen.clone()));
        app.update();
        assert_eq!(*app.world().resource::<Character>(), chosen);
        assert!(app.world().get_resource::<ChosenDelver>().is_none(), "the delve takes it up");
    }

    #[test]
    fn the_delver_starts_with_clothes_a_belt_and_a_burning_torch() {
        let mut app = App::new();
        app.init_resource::<Inventory>().init_resource::<Character>().add_systems(Update, give_starting_kit);
        app.update();
        let inventory = app.world().resource::<Inventory>();
        // Only what is worn and held, the folk's own headwear among it: nothing in the pouch.
        assert!(inventory.ids().all(|id| inventory.equipped().iter().any(|(slot, _)| inventory.worn(*slot) == Some(id))));
        for base in ["Belt", "Tunic", "Trousers", "Torch"] {
            assert!(inventory.ids().filter_map(|id| inventory.item(id)).any(|item| item.base == base), "{base}");
        }
        assert!(inventory.ids().filter_map(|id| inventory.item(id)).all(|item| item.rarity == Rarity::Common));
        let torch = inventory.worn(Slot::Offhand).and_then(|id| inventory.item(id)).expect("a torch in the off hand");
        assert!(crate::carry::item_light(&torch.shape).is_some());
    }

    #[test]
    fn the_cell_size_here_is_the_shipped_one() {
        assert_eq!(Tuning::default().interface.cell_size, CELL_PX);
    }

    #[test]
    fn cells_scale_with_the_window() {
        let interface = Tuning::default().interface;
        // At 1920 by 1080 the screen is at its own size: a 1 by 3 dagger is 72 by 216.
        let dagger = Grid::<ItemId>::new(1, 3);
        assert_eq!(interface.scale(Vec2::new(1920.0, 1080.0)), 1.0);
        assert_eq!(grid_size(&dagger, interface.cell_px(Vec2::new(1920.0, 1080.0))) - LINE_PX, Vec2::new(72.0, 216.0));
        // At 720 lines it is two thirds the size; never under the 36 floor.
        assert!((interface.cell_px(Vec2::new(1280.0, 720.0)) - 48.0).abs() < 1e-4);
        assert!((interface.cell_px(Vec2::new(640.0, 360.0)) - 36.0).abs() < 1e-4);
        assert!((interface.cell_px(Vec2::new(2560.0, 1440.0)) - 96.0).abs() < 1e-4);
    }

    #[test]
    fn fb_hold_names_each_hand() {
        let some = |b: &str| Some(Some(b.to_string()));
        assert_eq!(hold_spec("Mace,Torch"), (some("Mace"), some("Torch")));
        assert_eq!(hold_spec("Staff, none"), (some("Staff"), Some(None)));
        assert_eq!(hold_spec("Bow"), (some("Bow"), None));
        assert_eq!(hold_spec(",Shield"), (None, some("Shield")));
        // A torch or lantern asked for is lit; a weapon is as rolled.
        assert!(roll_held("Torch", 4).is_some_and(|t| crate::carry::item_light(&t.shape).is_some()));
        assert_eq!(roll_held("Sword", 4).map(|s| s.shape.family()), Some(armory::Family::Sword));
    }

    fn dagger() -> fb_items::Item {
        (1..500).filter_map(|s| fb_items::roll_item("Dagger", s)).find(|d| d.profile.footprint == Footprint { cols: 1, rows: 2 }).unwrap()
    }

    #[test]
    fn a_drag_turns_about_its_grabbed_cell() {
        let mut drag = Drag {
            id: ItemId(0),
            grab: IVec2::new(0, 2),
            size: Footprint { cols: 1, rows: 3 },
            turned: false,
            press: Vec2::ZERO,
            active: true,
            target: None,
        };
        drag.turn();
        assert_eq!((drag.size, drag.turned, drag.grab), (Footprint { cols: 3, rows: 1 }, true, IVec2::new(2, 0)));
        assert_eq!(drag.placement_at(IVec2::new(4, 1)), Some(Placement::new(2, 1, true)));
        assert_eq!(drag.placement_at(IVec2::new(1, 1)), None);
    }

    #[test]
    fn dropping_moves_between_grids_and_refuses_overlaps() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.wear(pack, Slot::Back).unwrap();
        let belt = inventory.add(fb_items::roll_item("Belt", 3).unwrap());
        inventory.wear(belt, Slot::Belt).unwrap();
        let (a, b) = (inventory.add(dagger()), inventory.add(dagger()));
        inventory.put(a, pack, Placement::new(0, 0, false)).unwrap();
        inventory.put(b, pack, Placement::new(3, 0, false)).unwrap();
        let drag = |id| Drag { id, grab: IVec2::new(0, 1), size: Footprint { cols: 1, rows: 2 }, turned: false, press: Vec2::ZERO, active: true, target: None };
        // Grabbed by its lower cell and let go over the belt pouch's cell (2, 1).
        let into_belt = Target::Cell { view: GridView::Container(belt), cell: IVec2::new(2, 1) };
        assert_eq!(drop_at(&mut inventory, &drag(a), into_belt, 6), Ok(vec![]));
        assert_eq!(inventory.location(a), Some(Location::In { container: belt, at: Placement::new(2, 0, false) }));
        // Onto the other dagger: refused, and it stays where it was.
        let onto_b = Target::Cell { view: GridView::Container(pack), cell: IVec2::new(3, 1) };
        assert_eq!(check_drop(&inventory, &drag(a), onto_b), Err(Refusal::NoRoom));
        assert_eq!(drop_at(&mut inventory, &drag(a), onto_b, 6), Err(Refusal::NoRoom));
        assert_eq!(inventory.location(a), Some(Location::In { container: belt, at: Placement::new(2, 0, false) }));
        // The pack cannot go into itself.
        let pack_drag = Drag { id: pack, grab: IVec2::ZERO, size: Footprint { cols: 2, rows: 3 }, turned: false, press: Vec2::ZERO, active: true, target: None };
        let into_itself = Target::Cell { view: GridView::Container(pack), cell: IVec2::new(5, 0) };
        assert_eq!(drop_at(&mut inventory, &pack_drag, into_itself, 6), Err(Refusal::IntoItself));
        assert_eq!(drop_at(&mut inventory, &drag(b), Target::Page, 6), Ok(vec![]));
    }

    #[test]
    fn cells_are_read_from_the_grid_rectangle() {
        let rect = Rect::new(100.0, 50.0, 100.0 + 8.0 * CELL_PX + LINE_PX, 50.0 + 5.0 * CELL_PX + LINE_PX);
        assert_eq!(cell_under(rect, Vec2::new(103.0, 53.0), CELL_PX), IVec2::new(0, 0));
        assert_eq!(cell_under(rect, Vec2::new(100.0 + 7.5 * CELL_PX, 50.0 + 4.5 * CELL_PX), CELL_PX), IVec2::new(7, 4));
        assert_eq!(cell_under(rect, Vec2::new(100.0 + 2.0 * CELL_PX + 5.0, 60.0), CELL_PX), IVec2::new(2, 0));
    }

    /// The screen opens and closes over an app with the inventory plugin's state and systems,
    /// but no window, renderer or assets.
    fn screen_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Keymap>()
            .init_resource::<crate::keymap::ActiveContext>()
            .init_resource::<Inventory>()
            .init_resource::<IconCache>()
            .init_resource::<UiFonts>()
            .init_resource::<ScreenOpen>()
            .init_resource::<SheetTab>()
            .init_resource::<Dragging>()
            .init_resource::<Hovered>()
            .init_resource::<Quickslots>()
            .init_resource::<sheet::HoveredNumber>()
            .init_resource::<card::ShownCard>()
            .init_resource::<Toast>()
            .init_resource::<Underfoot>()
            .init_resource::<FloorChanges>()
            .init_resource::<OpenContainer>()
            .init_resource::<crate::barter::Barter>()
            .init_resource::<Assets<Image>>()
            .init_resource::<DollAssets>()
            .init_resource::<Character>()
            .init_resource::<Tuning>()
            .init_resource::<ColumnScroll>()
            .init_resource::<ShelfScrolled>()
            .add_systems(PreUpdate, crate::keymap::track_context)
            .add_systems(Update, (switch_screens, header::click_tabs, reset_scroll_on_tab, build_screen, crate::character::wear_abilities, sheet::update_status, sheet::update_vitals, sheet::update_gear, sheet::spawn_tip, sheet::show_breakdown, drag_items, icon::reveal_icons).chain());
        app.world_mut().spawn(ViewModelCamera { follows: Entity::PLACEHOLDER });
        app
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        // As Bevy's input system does at the start of the next frame.
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
        app.update();
    }

    /// A strip slot on the page where `at` is its centre, as the layout would make it.
    fn strip_slot(app: &mut App, slot: usize, at: Vec2) {
        app.world_mut().spawn((
            QuickslotDrop(slot),
            ComputedNode { size: Vec2::splat(44.0), inverse_scale_factor: 1.0, ..default() },
            UiGlobalTransform::from(bevy::math::Affine2::from_translation(at)),
        ));
    }

    /// Lets an active drag of `id` go with the mouse over `at`, on the inventory screen.
    fn let_go_at(app: &mut App, id: ItemId, at: Vec2) {
        let mut window = Window::default();
        window.set_cursor_position(Some(at));
        app.world_mut().spawn((window, PrimaryWindow));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        let size = app.world().resource::<Inventory>().footprint(id).unwrap();
        app.world_mut().resource_mut::<Dragging>().0 =
            Some(Drag { id, grab: IVec2::ZERO, size, turned: false, press: at, active: true, target: None });
        let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        mouse.press(MouseButton::Left);
        mouse.release(MouseButton::Left);
        app.update();
    }

    #[test]
    fn dropping_an_item_on_a_quickslot_binds_it_and_leaves_it_where_it_was() {
        let mut app = screen_app();
        let (pack, loaf) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
            inventory.wear(pack, Slot::Back).unwrap();
            let loaf = inventory.add(fb_items::roll_item("Bread", 5).unwrap());
            inventory.put(loaf, pack, Placement::new(3, 1, false)).unwrap();
            (pack, loaf)
        };
        strip_slot(&mut app, 2, Vec2::new(300.0, 300.0));
        strip_slot(&mut app, 3, Vec2::new(350.0, 300.0));
        let_go_at(&mut app, loaf, Vec2::new(303.0, 296.0));
        assert_eq!(app.world().resource::<Quickslots>().get(2), Some(loaf));
        assert_eq!(app.world().resource::<Inventory>().location(loaf), Some(Location::In { container: pack, at: Placement::new(3, 1, false) }), "bound, not moved");
        assert!(app.world().resource::<Toast>().text.ends_with("is bound to slot 3"), "{}", app.world().resource::<Toast>().text);
        assert!(app.world().resource::<Dragging>().0.is_none());
    }

    #[test]
    fn a_quickslot_refuses_what_it_cannot_use_and_what_is_not_carried() {
        let mut app = screen_app();
        let (ring, loose) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
            inventory.wear(pack, Slot::Back).unwrap();
            let ring = inventory.add(fb_items::roll_item("Ring", 5).unwrap());
            inventory.put(ring, pack, Placement::new(0, 0, false)).unwrap();
            (ring, inventory.add(fb_items::roll_item("Bread", 5).unwrap()))
        };
        strip_slot(&mut app, 0, Vec2::new(100.0, 100.0));
        let drag = |id| Drag { id, grab: IVec2::ZERO, size: Footprint { cols: 1, rows: 1 }, turned: false, press: Vec2::ZERO, active: true, target: None };
        let inventory = app.world().resource::<Inventory>();
        assert_eq!(check_drop(inventory, &drag(ring), Target::Quickslot(0)), Err(Refusal::NotUsable));
        assert_eq!(check_drop(inventory, &drag(loose), Target::Quickslot(0)), Err(Refusal::NotCarried));
        let_go_at(&mut app, ring, Vec2::new(100.0, 100.0));
        assert_eq!(*app.world().resource::<Quickslots>(), Quickslots::default());
        assert_eq!(app.world().resource::<Toast>().text, Refusal::NotUsable.reason());
    }

    #[test]
    fn the_screen_has_a_strip_of_ten_quickslots_to_drop_on() {
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyI);
        let mut strip = app.world_mut().query::<&QuickslotDrop>();
        let mut slots: Vec<usize> = strip.iter(app.world()).map(|s| s.0).collect();
        slots.sort();
        assert_eq!(slots, (0..10).collect::<Vec<_>>());
        // Under the mouse, a strip slot is the target before anything beneath it.
        press(&mut app, KeyCode::KeyI);
        let mut strip = app.world_mut().query::<&QuickslotDrop>();
        assert_eq!(strip.iter(app.world()).count(), 0, "the strip goes with the screen");
    }

    #[test]
    fn the_layout_survives_closing_and_reopening() {
        let mut app = screen_app();
        let (pack, knife) = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
            inventory.wear(pack, Slot::Back).unwrap();
            let knife = inventory.add(dagger());
            inventory.put(knife, pack, Placement::new(5, 2, true)).unwrap();
            (pack, knife)
        };
        let shown = |app: &mut App| {
            let mut views = app.world_mut().query::<(&ItemView, &Node)>();
            views.iter(app.world()).map(|(v, n)| (v.id, n.left, n.top, n.width)).collect::<Vec<_>>()
        };
        press(&mut app, KeyCode::KeyI);
        assert!(app.world().resource::<ScreenOpen>().is(Screen::Inventory));
        let first = shown(&mut app);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0], (knife, px(5.0 * CELL_PX + LINE_PX), px(2.0 * CELL_PX + LINE_PX), px(2.0 * CELL_PX - LINE_PX)));
        press(&mut app, KeyCode::KeyI);
        assert!(!app.world().resource::<ScreenOpen>().is(Screen::Inventory));
        assert!(shown(&mut app).is_empty());
        press(&mut app, KeyCode::KeyI);
        assert_eq!(shown(&mut app), first);
        assert_eq!(app.world().resource::<Inventory>().location(knife), Some(Location::In { container: pack, at: Placement::new(5, 2, true) }));
    }

    #[test]
    fn slots_show_an_initial_until_their_icon_is_drawn() {
        let mut app = screen_app();
        let knife = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
            inventory.wear(pack, Slot::Back).unwrap();
            let knife = inventory.add(dagger());
            inventory.put(knife, pack, Placement::new(0, 0, false)).unwrap();
            knife
        };
        press(&mut app, KeyCode::KeyI);
        let initials = |app: &mut App| {
            let mut placeholders = app.world_mut().query_filtered::<&Children, With<icon::IconPlaceholder>>();
            let world = app.world();
            placeholders.iter(world).flat_map(|c| c.iter()).filter_map(|c| world.get::<Text>(c)).map(|t| t.0.clone()).collect::<Vec<_>>()
        };
        // The knife in its cell, the pack in the back slot and the pack by its grid's name.
        let waiting = initials(&mut app);
        assert_eq!(waiting.len(), 3, "{waiting:?}");
        let name = app.world().resource::<Inventory>().item(knife).unwrap().name.clone();
        assert!(waiting.contains(&icon::initial(&name)), "{waiting:?}");

        let mut knife_icon = app.world_mut().query_filtered::<&ImageNode, With<ItemIcon>>();
        let image = knife_icon.iter(app.world()).next().expect("the knife's icon").image.clone();
        app.world_mut().resource_mut::<IconCache>().mark_drawn(&image);
        app.update();
        assert_eq!(initials(&mut app).len(), 2, "the knife's initial is gone");
        assert_eq!(knife_icon.iter(app.world()).next().expect("the knife's icon").image, image, "the slot shows the drawn icon");
        let mut awaiting = app.world_mut().query::<&icon::AwaitingIcon>();
        assert_eq!(awaiting.iter(app.world()).count(), 2);
    }

    #[test]
    fn i_toggles_the_screen_and_tab_does_nothing_while_it_is_closed() {
        let mut app = screen_app();
        press(&mut app, KeyCode::Tab);
        assert!(!app.world().resource::<ScreenOpen>().is(Screen::Inventory));
        assert_eq!(*app.world().resource::<SheetTab>(), SheetTab::Inventory);
        press(&mut app, KeyCode::KeyI);
        assert!(app.world().resource::<ScreenOpen>().is(Screen::Inventory));
        press(&mut app, KeyCode::KeyI);
        assert!(!app.world().resource::<ScreenOpen>().is(Screen::Inventory));
    }

    fn shift_tab(app: &mut App) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ShiftLeft);
        press(app, KeyCode::Tab);
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::ShiftLeft);
        keys.clear();
    }

    #[test]
    fn the_left_page_keeps_one_width_on_every_tab() {
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyI);
        for tab in SheetTab::ALL {
            *app.world_mut().resource_mut::<SheetTab>() = tab;
            app.update();
            let mut pages = app.world_mut().query_filtered::<&Node, With<LeafPage>>();
            let node = pages.single(app.world()).unwrap();
            assert_eq!(node.width, px(LEAF_PX), "{tab:?}");
            let mut ledgers = app.world_mut().query_filtered::<&Node, With<Ledger>>();
            let ledger = ledgers.single(app.world()).unwrap();
            assert_eq!((ledger.height, ledger.max_height), (percent(92), percent(92)), "{tab:?}: the ledger holds one height, so the page holds one place");
        }
    }

    #[test]
    fn a_barter_keeps_the_content_sized_ledger_and_tab_does_not_cycle() {
        let mut app = screen_app();
        app.insert_resource(crate::barter::Barter::started_for_test());
        press(&mut app, KeyCode::KeyI);
        let mut ledgers = app.world_mut().query_filtered::<&Node, With<Ledger>>();
        let ledger = ledgers.single(app.world()).unwrap();
        assert_eq!((ledger.height, ledger.max_height), (Val::Auto, percent(92)));
        let mut pages = app.world_mut().query_filtered::<&Node, With<LeafPage>>();
        assert_eq!(pages.single(app.world()).unwrap().width, Val::Auto);
        press(&mut app, KeyCode::Tab);
        assert_eq!(*app.world().resource::<SheetTab>(), SheetTab::Inventory, "Tab leaves the trader's shelf up");
    }

    #[test]
    fn tab_can_be_rebound() {
        let mut app = screen_app();
        app.world_mut().resource_mut::<Keymap>().bindings.iter_mut().find(|b| b.action == Action::NextTab).unwrap().inputs = vec![crate::keymap::Input::Key(KeyCode::KeyG)];
        press(&mut app, KeyCode::KeyI);
        press(&mut app, KeyCode::Tab);
        assert_eq!(*app.world().resource::<SheetTab>(), SheetTab::Inventory, "the old key does nothing");
        press(&mut app, KeyCode::KeyG);
        assert_eq!(*app.world().resource::<SheetTab>(), SheetTab::Factions);
    }

    #[test]
    fn clicking_a_tab_starts_the_new_page_at_the_top() {
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyK);
        app.world_mut().resource_mut::<ColumnScroll>().0 = 300.0;
        let mut window = Window::default();
        window.set_cursor_position(Some(Vec2::new(130.0, 20.0)));
        app.world_mut().spawn((window, PrimaryWindow));
        app.world_mut().spawn((
            header::TabLabel(SheetTab::Factions),
            ComputedNode { size: Vec2::new(100.0, 24.0), inverse_scale_factor: 1.0, ..default() },
            UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(130.0, 20.0))),
        ));
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        assert_eq!(*app.world().resource::<SheetTab>(), SheetTab::Factions);
        assert_eq!(app.world().resource::<ColumnScroll>().0, 0.0);
    }

    #[test]
    fn tab_and_shift_tab_cycle_the_tabs_and_wrap() {
        let mut app = screen_app();
        let tab = |app: &App| *app.world().resource::<SheetTab>();
        press(&mut app, KeyCode::KeyI);
        assert_eq!(tab(&app), SheetTab::Inventory);
        press(&mut app, KeyCode::Tab);
        assert_eq!(tab(&app), SheetTab::Factions);
        press(&mut app, KeyCode::Tab);
        assert_eq!(tab(&app), SheetTab::Character);
        assert!(sheet_showing(&mut app));
        press(&mut app, KeyCode::Tab);
        assert_eq!(tab(&app), SheetTab::Inventory, "wraps round");
        shift_tab(&mut app);
        assert_eq!(tab(&app), SheetTab::Character, "Shift+Tab wraps back");
        shift_tab(&mut app);
        assert_eq!(tab(&app), SheetTab::Factions);
        press(&mut app, KeyCode::KeyK);
        assert_eq!(tab(&app), SheetTab::Character, "K still shows the sheet");
        press(&mut app, KeyCode::KeyI);
        assert_eq!(tab(&app), SheetTab::Inventory, "and I the grids");
        assert!(app.world().resource::<ScreenOpen>().is(Screen::Inventory));
    }

    fn page_text(app: &mut App) -> Vec<String> {
        let mut pages = app.world_mut().query_filtered::<Entity, With<factions::FactionsPage>>();
        let Some(page) = pages.iter(app.world()).next() else { return Vec::new() };
        let mut texts = app.world_mut().query::<(&Text, Option<&ChildOf>)>();
        let world = app.world();
        let mut lines = Vec::new();
        for (text, parent) in texts.iter(world) {
            let mut up = parent.map(|p| p.parent());
            while let Some(entity) = up {
                if entity == page {
                    lines.push(text.0.clone());
                    break;
                }
                up = world.get::<ChildOf>(entity).map(|p| p.parent());
            }
        }
        lines
    }

    #[test]
    fn the_factions_tab_says_so_when_no_faction_is_met() {
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyI);
        press(&mut app, KeyCode::Tab);
        assert_eq!(page_text(&mut app), vec![factions::NO_FACTIONS_MET.to_string()]);
    }

    #[test]
    fn the_factions_tab_lists_the_met_factions_with_their_standing() {
        let mut app = screen_app();
        let run_seed = 11;
        app.insert_resource(crate::travel::Abyss { run_seed, ..default() });
        let mut record = crate::record::WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, run_seed);
        let world = fb_faction::World::new(run_seed);
        let (first, second, unmet) = (world.factions()[0].id, world.factions()[2].id, world.factions()[1].id);
        record.standings.meet(first);
        record.standings.meet(second);
        record.keys_given.push(second);
        record.note_met_on(first, 1);
        app.insert_resource(record);
        press(&mut app, KeyCode::KeyI);
        press(&mut app, KeyCode::Tab);
        let lines = page_text(&mut app);
        assert!(lines.contains(&world.faction(first).name) && lines.contains(&world.faction(second).name), "{lines:?}");
        assert!(!lines.contains(&world.faction(unmet).name));
        let band = app.world().resource::<crate::record::WorldRecord>().standings.band(first);
        assert!(lines.iter().any(|l| l.starts_with(band.label())), "{lines:?}");
        assert!(lines.contains(&"Given you their key".to_string()) && lines.contains(&"Not yet given".to_string()), "{lines:?}");
        assert!(lines.contains(&"Depth 1".to_string()), "{lines:?}");
        assert!(!lines.contains(&factions::NO_FACTIONS_MET.to_string()));
    }

    #[test]
    fn the_character_and_factions_pages_scroll_under_the_header() {
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyK);
        let mut columns = app.world_mut().query_filtered::<&Node, With<ScrollColumn>>();
        let node = columns.single(app.world()).unwrap();
        assert_eq!(node.overflow.y, OverflowAxis::Scroll, "the character sheet scrolls");
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        assert_eq!(*app.world().resource::<SheetTab>(), SheetTab::Factions);
        let mut columns = app.world_mut().query_filtered::<&Node, With<ScrollColumn>>();
        assert_eq!(columns.single(app.world()).unwrap().overflow.y, OverflowAxis::Scroll, "so does the factions page");
    }

    fn sheet_showing(app: &mut App) -> bool {
        let mut sheets = app.world_mut().query_filtered::<Entity, With<sheet::CharacterSheet>>();
        sheets.iter(app.world()).count() == 1
    }

    #[test]
    fn c_opens_the_character_tab_and_i_the_inventory_tab() {
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyK);
        assert!(app.world().resource::<ScreenOpen>().is(Screen::Inventory));
        assert_eq!(*app.world().resource::<SheetTab>(), SheetTab::Character);
        assert!(sheet_showing(&mut app));
        let mut grids = app.world_mut().query::<&GridView>();
        assert_eq!(grids.iter(app.world()).count(), 0, "the sheet replaces the grids");
        press(&mut app, KeyCode::KeyI);
        assert!(app.world().resource::<ScreenOpen>().is(Screen::Inventory), "I swaps to the inventory tab");
        assert_eq!(*app.world().resource::<SheetTab>(), SheetTab::Inventory);
        assert!(!sheet_showing(&mut app));
        press(&mut app, KeyCode::KeyK);
        assert!(sheet_showing(&mut app));
        press(&mut app, KeyCode::KeyK);
        assert!(!app.world().resource::<ScreenOpen>().is(Screen::Inventory), "K closes the character tab");
        press(&mut app, KeyCode::KeyK);
        press(&mut app, KeyCode::Escape);
        assert!(!app.world().resource::<ScreenOpen>().is(Screen::Inventory), "Esc closes it");
        press(&mut app, KeyCode::KeyI);
        assert_eq!(*app.world().resource::<SheetTab>(), SheetTab::Inventory, "the screen reopens on the inventory tab");
    }

    /// Every line of text under the character sheet, in order.
    fn sheet_text(app: &mut App) -> Vec<String> {
        let mut texts = app.world_mut().query::<(&Text, Option<&ChildOf>)>();
        let world = app.world();
        let mut lines = Vec::new();
        for (text, parent) in texts.iter(world) {
            let mut up = parent.map(|p| p.parent());
            while let Some(entity) = up {
                if world.get::<sheet::CharacterSheet>(entity).is_some() {
                    lines.push(text.0.clone());
                    break;
                }
                up = world.get::<ChildOf>(entity).map(|p| p.parent());
            }
        }
        lines
    }

    fn delver_with(app: &mut App, hunger: f64, exhaustion: u32) -> Entity {
        let mut needs = fb_character::Needs::default();
        needs.hunger.value = hunger;
        app.world_mut().spawn((crate::play::Player, crate::character::DelverNeeds(needs), crate::exhaustion::Exhaustion(fb_character::Exhaustion::new(exhaustion)), crate::character::DelverStamina::default(), crate::character::DelverMana::default())).id()
    }

    /// A rolled `base` with the real affix `name` applied and its weight set to `weight`.
    fn weighed_with(base: &str, seed: u32, affix: Option<&str>, weight: f32) -> fb_items::Item {
        let mut item = fb_items::roll_item(base, seed).unwrap();
        if let Some(name) = affix {
            let affix = fb_items::tables::affixes().iter().find(|a| a.name == name).unwrap();
            affix.apply_to_stats(&mut item.profile.stats);
            item.affixes.push(affix);
        }
        item.profile.stats.weight = weight;
        item
    }

    /// The value on the gear row `label`.
    fn gear_row(app: &mut App, label: usize) -> String {
        let mut lines = app.world_mut().query::<(&sheet::GearLine, &Text)>();
        lines.iter(app.world()).find(|(l, _)| l.0 == label).map(|(_, t)| t.0.clone()).expect("a gear line")
    }

    #[test]
    fn the_attributes_read_with_their_modifiers() {
        let mut app = screen_app();
        app.world_mut().resource_mut::<Character>().attributes = fb_character::Attributes { strength: 14, dexterity: 9, constitution: 10, intelligence: 18, wisdom: 3 };
        press(&mut app, KeyCode::KeyK);
        let text = sheet_text(&mut app);
        for want in ["STR 14 (+2)", "DEX 9 (-1)", "CON 10 (+0)", "INT 18 (+4)", "WIS 3 (-4)"] {
            assert!(text.iter().any(|t| t == want), "{want} in {text:?}");
        }
    }

    #[test]
    fn a_belt_of_the_ox_from_the_pack_changes_the_load_while_the_tab_is_open() {
        use sheet::GEAR_ROWS;
        let mut app = screen_app();
        let attributes = fb_character::Attributes { strength: 10, dexterity: 10, constitution: 10, intelligence: 10, wisdom: 10 };
        *app.world_mut().resource_mut::<Character>() = Character { folk: fb_body::Folk::Human, attributes, ..Character::roll(4) };
        let belt = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let pack = inventory.add(weighed_with("Pack", 7, None, 0.0));
            inventory.wear(pack, Slot::Back).unwrap();
            let load = inventory.add(weighed_with("Dagger", 3, None, 55.0));
            inventory.put(load, pack, Placement::new(0, 0, false)).unwrap();
            let belt = inventory.add(weighed_with("Belt", 2, Some("of the Ox"), 0.0));
            inventory.put(belt, pack, Placement::new(4, 0, false)).unwrap();
            belt
        };
        press(&mut app, KeyCode::KeyK);
        assert_eq!(GEAR_ROWS[0], "Load");
        assert_eq!(gear_row(&mut app, 0), "55 / 150 lb \u{b7} Burdened");
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            equip(&mut inventory, belt, Slot::Belt).unwrap();
        }
        app.update();
        assert_eq!(gear_row(&mut app, 0), "55 / 175 lb \u{b7} Light");
        assert!(app.world().resource::<ScreenOpen>().is(Screen::Inventory));
    }

    #[test]
    fn the_gear_rows_agree_with_the_header() {
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyK);
        let character = app.world().resource::<Character>().clone();
        let derived = character.derive(app.world().resource::<Inventory>());
        assert_eq!(gear_row(&mut app, 1), format!("{:.0} / {:.0}", derived.warmth, character.warmth_need()));
        assert_eq!(gear_row(&mut app, 4), derived.armour_class.to_string());
        assert_eq!(gear_row(&mut app, 2), "none");
        assert!(header_stats(&mut app).contains(&format!("AC {}", derived.armour_class)));
    }

    #[test]
    fn plenty_wakefulness_and_the_leech_show_on_the_sheet_with_their_breakdowns() {
        use sheet::{GEAR_ROWS, breakdown};
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyK);
        assert_eq!(&GEAR_ROWS[5..8], ["Hunger rate", "Sleepiness rate", "Heal on a kill"]);
        assert_eq!((gear_row(&mut app, 5), gear_row(&mut app, 6), gear_row(&mut app, 7)), ("usual".to_string(), "usual".to_string(), "none".to_string()));
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            // Seeds that roll no affix of their own, so each shows only the one added.
            let plain = |base: &str| (0..).find(|&s| fb_items::roll_item(base, s).unwrap().affixes.is_empty()).unwrap();
            let belt = inventory.add(weighed_with("Belt", plain("Belt"), Some("of Plenty"), 1.0));
            equip(&mut inventory, belt, Slot::Belt).unwrap();
            let ring = inventory.add(weighed_with("Ring", plain("Ring"), Some("of Wakefulness"), 0.0));
            equip(&mut inventory, ring, Slot::Ring1).unwrap();
            let sword = inventory.add(weighed_with("Sword", plain("Sword"), Some("of the Leech"), 3.0));
            equip(&mut inventory, sword, Slot::Mainhand).unwrap();
        }
        app.update();
        assert_eq!((gear_row(&mut app, 5), gear_row(&mut app, 6), gear_row(&mut app, 7)), ("\u{d7}0.85".to_string(), "\u{d7}0.85".to_string(), "+1 HP".to_string()));
        let character = app.world().resource::<Character>().clone();
        let inventory = app.world().resource::<Inventory>();
        let hunger = breakdown(fb_character::rules::defaults(), &character, inventory, 5).join(" ");
        assert!(hunger.starts_with("Hunger rate") && hunger.contains("(0.85)"), "{hunger}");
        let sleepiness = breakdown(fb_character::rules::defaults(), &character, inventory, 6).join(" ");
        assert!(sleepiness.starts_with("Sleepiness rate") && sleepiness.contains("(0.85)"), "{sleepiness}");
        let leech = breakdown(fb_character::rules::defaults(), &character, inventory, 7).join(" ");
        assert!(leech.starts_with("Heal on a kill 1") && leech.contains("(+1)"), "{leech}");
    }

    #[test]
    fn a_ring_of_the_giant_boots_of_haste_and_a_shield_of_the_rampart_show_on_the_sheet() {
        let mut app = screen_app();
        let attributes = fb_character::Attributes { strength: 13, dexterity: 10, constitution: 10, intelligence: 10, wisdom: 10 };
        *app.world_mut().resource_mut::<Character>() = Character { attributes, ..Character::roll(4) };
        press(&mut app, KeyCode::KeyK);
        let rows = |app: &mut App| [8, 9, 10, 11].map(|row| gear_row(app, row));
        assert_eq!(rows(&mut app), ["usual", "none", "none", "none"].map(String::from));
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let ring = inventory.add(weighed_with("Ring", 4, Some("of the Giant"), 0.0));
            equip(&mut inventory, ring, Slot::Ring1).unwrap();
            let boots = inventory.add(weighed_with("Boots", 4, Some("of Haste"), 1.0));
            equip(&mut inventory, boots, Slot::Feet).unwrap();
            let shield = inventory.add(weighed_with("Shield", 4, Some("of the Rampart"), 4.0));
            equip(&mut inventory, shield, Slot::Offhand).unwrap();
            let amulet = inventory.add(weighed_with("Amulet", 4, Some("of Fortune"), 0.0));
            equip(&mut inventory, amulet, Slot::Neck).unwrap();
        }
        app.update();
        app.update();
        assert_eq!(rows(&mut app), ["\u{d7}1.2", "80%", "+2", "+2 STR"].map(String::from));
        assert_eq!(app.world().resource::<Character>().strength(), 15);
        assert!(sheet_text(&mut app).iter().any(|t| t == "STR 15 (+2)"), "the attribute line shows the score as it stands");
    }

    #[test]
    fn a_goblin_shows_60_ft_of_darkvision_and_a_lizardfolk_needs_more_warmth() {
        let mut app = screen_app();
        *app.world_mut().resource_mut::<Character>() = Character { folk: fb_body::Folk::Goblin, ..Character::roll(4) };
        press(&mut app, KeyCode::KeyK);
        assert_eq!(gear_row(&mut app, 3), "60 ft");
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let belt = inventory.add(weighed_with("Belt", 2, Some("of the Mole"), 1.0));
            equip(&mut inventory, belt, Slot::Belt).unwrap();
        }
        app.update();
        assert_eq!(gear_row(&mut app, 3), "60 ft", "the best, not the sum");
        assert_eq!(gear_row(&mut app, 1).split(" / ").nth(1), Some("6"));
        app.world_mut().resource_mut::<Character>().folk = fb_body::Folk::Lizardfolk;
        app.update();
        assert_eq!(gear_row(&mut app, 1).split(" / ").nth(1), Some("8"));
    }

    /// The lines of the breakdown tooltip.
    fn tip_text(app: &mut App) -> Vec<String> {
        let mut texts = app.world_mut().query::<(&Text, &ChildOf)>();
        let world = app.world();
        texts.iter(world).filter(|(_, p)| world.get::<sheet::BreakdownTip>(p.parent()).is_some()).map(|(t, _)| t.0.clone()).collect()
    }

    fn hover_row(app: &mut App, row: Option<usize>) {
        app.world_mut().resource_mut::<sheet::HoveredNumber>().0 = row.map(|r| (r, Rect::new(10.0, 10.0 + 22.0 * r as f32, 300.0, 30.0 + 22.0 * r as f32)));
        app.update();
        app.update();
    }

    #[test]
    fn hovering_capacity_lists_the_strength_and_the_belt_of_the_ox() {
        let mut app = screen_app();
        let attributes = fb_character::Attributes { strength: 10, dexterity: 10, constitution: 10, intelligence: 10, wisdom: 10 };
        *app.world_mut().resource_mut::<Character>() = Character { folk: fb_body::Folk::Human, attributes, ..Character::roll(4) };
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let belt = inventory.add(weighed_with("Belt", 2, Some("of the Ox"), 1.0));
            equip(&mut inventory, belt, Slot::Belt).unwrap();
        }
        press(&mut app, KeyCode::KeyK);
        assert!(tip_text(&mut app).is_empty());
        hover_row(&mut app, Some(0));
        let tip = tip_text(&mut app);
        assert!(tip.iter().any(|l| l.starts_with("Capacity: STR 10 \u{d7} 15 + ") && l.contains("Belt") && l.ends_with("(+25.0) = 175 lb")), "{tip:?}");
        assert!(tip.iter().any(|l| l == "Carried: 1 lb"), "{tip:?}");
        let mut tips = app.world_mut().query_filtered::<&Visibility, With<sheet::BreakdownTip>>();
        assert_eq!(*tips.single(app.world()).unwrap(), Visibility::Inherited);
        hover_row(&mut app, None);
        assert_eq!(*tips.single(app.world()).unwrap(), Visibility::Hidden);
    }

    #[test]
    fn hovering_a_goblin_s_darkvision_names_the_folk_as_the_winner() {
        let mut app = screen_app();
        *app.world_mut().resource_mut::<Character>() = Character { folk: fb_body::Folk::Goblin, ..Character::roll(4) };
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let belt = inventory.add(weighed_with("Belt", 2, Some("of the Mole"), 1.0));
            equip(&mut inventory, belt, Slot::Belt).unwrap();
        }
        press(&mut app, KeyCode::KeyK);
        hover_row(&mut app, Some(3));
        let tip = tip_text(&mut app);
        assert!(tip.iter().any(|l| l.starts_with("Darkvision 60 ft = best of Goblin 60, ") && l.contains("Belt") && l.ends_with("(Goblin)")), "{tip:?}");
    }

    #[test]
    fn hovering_warmth_and_armour_class_explain_them() {
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyK);
        hover_row(&mut app, Some(1));
        let tip = tip_text(&mut app);
        assert!(tip.iter().any(|l| l.starts_with("Warmth ")) && tip.iter().any(|l| l == "Warmth needed 6"), "{tip:?}");
        hover_row(&mut app, Some(4));
        let tip = tip_text(&mut app);
        assert!(tip.iter().any(|l| l.starts_with("Armour class ") && l.contains("= base (10) + DEX ")), "{tip:?}");
    }

    #[test]
    fn the_mouse_over_a_row_picks_it() {
        let rows = [(0, Rect::new(0.0, 0.0, 100.0, 20.0)), (1, Rect::new(0.0, 20.0, 100.0, 40.0))];
        assert_eq!(sheet::row_under(rows, Vec2::new(5.0, 25.0)).map(|r| r.0), Some(1));
        assert_eq!(sheet::row_under(rows, Vec2::new(150.0, 5.0)), None);
    }

    #[test]
    fn the_status_reads_the_needs_exhaustion_and_load() {
        let mut app = screen_app();
        delver_with(&mut app, 0.0, 2);
        press(&mut app, KeyCode::KeyK);
        let text = sheet_text(&mut app);
        for want in ["Hunger", "Thirst", "Sleepiness", "Fed", "Quenched", "Rested", "Exhaustion 2: half speed"] {
            assert!(text.iter().any(|t| t == want), "{want} in {text:?}");
        }
    }

    #[test]
    fn the_status_changes_while_the_screen_stays_open() {
        let mut app = screen_app();
        let delver = delver_with(&mut app, 0.0, 0);
        press(&mut app, KeyCode::KeyK);
        let before = {
            let mut sheets = app.world_mut().query_filtered::<Entity, With<sheet::CharacterSheet>>();
            sheets.single(app.world()).unwrap()
        };
        assert!(sheet_text(&mut app).contains(&"Fed".to_string()));
        app.world_mut().get_mut::<crate::character::DelverNeeds>(delver).unwrap().0.hunger.value = 0.55;
        app.world_mut().get_mut::<crate::exhaustion::Exhaustion>(delver).unwrap().0 = fb_character::Exhaustion::new(2);
        app.update();
        let text = sheet_text(&mut app);
        assert!(text.contains(&"Hungry".to_string()) && !text.contains(&"Fed".to_string()), "{text:?}");
        assert!(text.contains(&"Exhaustion 2: half speed".to_string()), "{text:?}");
        let mut fills = app.world_mut().query::<(&sheet::NeedFill, &Node)>();
        let hunger = fills.iter(app.world()).find(|(f, _)| f.0 == fb_character::MeterKind::Hunger).map(|(_, n)| n.width).unwrap();
        assert_eq!(hunger, percent(55.0));
        let mut sheets = app.world_mut().query_filtered::<Entity, With<sheet::CharacterSheet>>();
        assert_eq!(sheets.single(app.world()).unwrap(), before, "the sheet was updated, not rebuilt");
    }

    #[test]
    fn stamina_and_mana_show_on_the_sheet_and_move_live() {
        let mut app = screen_app();
        let delver = delver_with(&mut app, 0.0, 0);
        press(&mut app, KeyCode::KeyK);
        let text = sheet_text(&mut app);
        for want in ["Stamina", "Mana", "Fresh", "4 / 4"] {
            assert!(text.iter().any(|t| t == want), "{want} in {text:?}");
        }
        let mut sheets = app.world_mut().query_filtered::<Entity, With<sheet::CharacterSheet>>();
        let before = sheets.single(app.world()).unwrap();
        app.world_mut().get_mut::<crate::character::DelverStamina>(delver).unwrap().0 = fb_character::Stamina { value: 0.0, winded: true };
        app.world_mut().get_mut::<crate::character::DelverMana>(delver).unwrap().0.points = 2.0;
        app.update();
        let text = sheet_text(&mut app);
        assert!(text.contains(&"Winded".to_string()) && !text.contains(&"Fresh".to_string()), "{text:?}");
        assert!(text.contains(&"2 / 4".to_string()), "{text:?}");
        let mut fills = app.world_mut().query::<(&sheet::VitalFill, &Node)>();
        let widths: Vec<_> = fills.iter(app.world()).map(|(f, n)| (f.0, n.width)).collect();
        assert!(widths.contains(&(sheet::Vital::Stamina, percent(0.0))) && widths.contains(&(sheet::Vital::Mana, percent(50.0))), "{widths:?}");
        assert_eq!(sheets.single(app.world()).unwrap(), before, "the sheet was updated, not rebuilt");
    }

    #[test]
    fn eating_with_the_sheet_open_shows_fed() {
        let mut app = screen_app();
        let delver = delver_with(&mut app, 0.6, 0);
        press(&mut app, KeyCode::KeyK);
        assert!(sheet_text(&mut app).contains(&"Hungry".to_string()));
        app.world_mut().get_mut::<crate::character::DelverNeeds>(delver).unwrap().0.consume(1.0, 0.0);
        app.update();
        assert!(sheet_text(&mut app).contains(&"Fed".to_string()));
    }

    #[test]
    fn bare_stone_underfoot_shows_its_empty_grid() {
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyI);
        let size = grid_size(&app.world().resource::<Underfoot>().grid, CELL_PX);
        let mut views = app.world_mut().query::<(&GridView, &Node, &Children)>();
        let (_, node, children) = views.iter(app.world()).find(|(v, ..)| **v == GridView::Underfoot).expect("the underfoot grid");
        assert_eq!((node.width, node.height), (px(size.x), px(size.y)));
        // In an empty corridor: 18 empty cells, 6 by 3, and nothing in them.
        let children = children.to_vec();
        assert_eq!(children.len(), 18);
        assert!(children.iter().all(|c| app.world().get::<ItemView>(*c).is_none()));
        let mut texts = app.world_mut().query::<&Text>();
        let said: Vec<String> = texts.iter(app.world()).map(|t| t.0.clone()).collect();
        assert!(said.contains(&"underfoot".to_string()) && said.contains(&"Bare stone. Drop things here.".to_string()), "{said:?}");
    }

    /// The header's line of numbers, as read from its spans.
    fn header_stats(app: &mut App) -> String {
        header_spans(app).into_iter().map(|parts| parts.into_iter().map(|(text, _)| text).collect::<String>()).collect::<Vec<_>>().join("   ")
    }

    /// The header's stats as spans with their colours, one list for each part.
    fn header_spans(app: &mut App) -> Vec<Vec<(String, Color)>> {
        let mut lines = app.world_mut().query_filtered::<&Children, With<header::HeaderStats>>();
        let parts = lines.single(app.world()).unwrap().to_vec();
        parts
            .into_iter()
            .map(|part| {
                let text = app.world().get::<Children>(part).unwrap()[0];
                let spans: Vec<Entity> = app.world().get::<Children>(text).map(|c| c.to_vec()).unwrap_or_default();
                spans.iter().filter_map(|s| Some((app.world().get::<TextSpan>(*s)?.0.clone(), app.world().get::<TextColor>(*s)?.0))).collect()
            })
            .collect()
    }

    /// The header's purse line, as read from its text and spans.
    fn header_purse(app: &mut App) -> String {
        let mut lines = app.world_mut().query_filtered::<(&Text, &Children), With<header::HeaderPurse>>();
        let (text, spans) = lines.single(app.world()).unwrap();
        let spans = spans.to_vec();
        let head = text.0.clone();
        head + &spans.iter().filter_map(|s| app.world().get::<TextSpan>(*s)).map(|t| t.0.clone()).collect::<String>()
    }

    #[test]
    fn a_tag_says_the_trade_s_price_in_its_coin() {
        use card::Price;
        use fb_items::Coins;
        let worth = Coins::default();
        let costs = |cp, afford| cell_tag(Price::Costs { price: Coins::from_copper(cp), worth, afford });
        assert_eq!(costs(250, true), Some(("2.5g".to_string(), crate::ui::COIN_GOLD, crate::ui::FRAME_GOLD)));
        assert_eq!(costs(250, false).map(|t| t.1), Some(BAD), "out of reach");
        assert_eq!(cell_tag(Price::SellsFor { price: Coins::from_copper(8 * 10), worth }), Some(("8s".to_string(), crate::ui::COIN_SILVER, OK)));
        assert_eq!(cell_tag(Price::Worth(worth)), None);
    }

    #[test]
    fn bartering_tags_every_ware_and_good_with_its_price() {
        let mut app = screen_app();
        let spot = fb_level::generate(1207, 1).folk[0];
        let trade = fb_faction::Trade { profession: fb_faction::Profession::Weaponsmith, tier: fb_faction::Tier::Apprentice, style: None };
        let person = crate::folk::person_1207(&fb_level::FolkSpot { trade: Some(trade), ..spot });
        let wares: Vec<_> = (1..4).filter_map(|s| fb_items::roll_item("Sword", s)).collect();
        let shelf = crate::shelf::Shelf { day: 0, wares: wares.clone(), sold: vec![], purse: 6000 };
        let session = {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
            inventory.wear(pack, Slot::Back).unwrap();
            let dagger = inventory.add(fb_items::roll_item("Dagger", 6).unwrap());
            inventory.stow(dagger).unwrap();
            let purse = inventory.add(purse_of(fb_items::Coins { gp: 1, sp: 0, cp: 0 }));
            inventory.stow(purse).unwrap();
            crate::barter::begin(&mut inventory, Entity::PLACEHOLDER, &person, fb_faction::Band::Neutral, &shelf).unwrap()
        };
        let shelf_id = session.shelf;
        app.world_mut().resource_mut::<crate::barter::Barter>().session = Some(session);
        app.world_mut().resource_mut::<OpenContainer>().0 = Some(shelf_id);
        press(&mut app, KeyCode::KeyI);
        let tags = app.world_mut().query_filtered::<(), With<PriceTag>>().iter(app.world()).count();
        assert_eq!(tags, wares.len() + 1, "the swords on the shelf and the dagger, not the purse or the pack");
        // No barter, no tags.
        app.world_mut().resource_mut::<crate::barter::Barter>().session = None;
        app.update();
        assert_eq!(app.world_mut().query_filtered::<(), With<PriceTag>>().iter(app.world()).count(), 0);
    }

    #[test]
    fn the_header_shows_the_coin_purse_and_opening_a_purse_fills_it() {
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyI);
        assert_eq!(header_purse(&mut app), "Purse empty");
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let purse = inventory.add(purse_of(fb_items::Coins { gp: 2, sp: 3, cp: 0 }));
            quick(&mut inventory, purse).unwrap();
        }
        app.update();
        assert_eq!(header_purse(&mut app), "Purse 2 gp 3 sp");
    }

    /// The colour of the header's span that reads `word`, and of the gear row 0 text.
    fn load_colours(app: &mut App, word: &str) -> (Color, Color) {
        let header = header_spans(app).into_iter().flatten().find(|(text, _)| text.trim() == word).unwrap_or_else(|| panic!("a span reading {word}")).1;
        let mut rows = app.world_mut().query::<(&sheet::GearLine, &TextColor)>();
        let row = rows.iter(app.world()).find(|(l, _)| l.0 == 0).map(|(_, c)| c.0).expect("the load row");
        (header, row)
    }

    #[test]
    fn a_delver_carrying_65_lb_shows_the_load_condition_on_the_sheet() {
        use crate::character::{DelverNeeds, DelverStamina, Load};
        use crate::exhaustion::Exhaustion;
        use crate::play::Player;
        let mut app = screen_app();
        app.init_resource::<crate::status::ShownConditions>().init_resource::<crate::rules::GameRules>().add_systems(Update, (crate::status::read_conditions, sheet::update_conditions).chain().after(build_screen));
        let delver = app.world_mut().spawn((Player, DelverNeeds::default(), Exhaustion::default(), DelverStamina::default(), Load::of(65.0, 165.0))).id();
        press(&mut app, KeyCode::KeyK);
        app.update();
        let said = sheet_text(&mut app);
        let line = "Burdened";
        assert!(said.iter().any(|t| t == line), "{said:?}");
        assert!(said.iter().any(|t| t == "65 lb, 39% of your capacity. Encumbered from 110 lb. Walking at 99.7% pace; sprinting tires you 1.01x as fast."), "{said:?}");
        assert!(!said.iter().any(|t| t == "No conditions"));
        // Setting it down clears the condition from the list.
        *app.world_mut().get_mut::<Load>(delver).unwrap() = Load::of(40.0, 165.0);
        app.update();
        assert!(sheet_text(&mut app).iter().any(|t| t == "No conditions"));
    }

    #[test]
    fn the_carried_load_is_coloured_by_its_band_in_the_header_and_on_the_sheet() {
        use crate::ui::{AMBER, BRIGHT, ORANGE, RED};
        let mut app = screen_app();
        let attributes = fb_character::Attributes { strength: 11, dexterity: 10, constitution: 10, intelligence: 10, wisdom: 10 };
        *app.world_mut().resource_mut::<Character>() = Character { folk: fb_body::Folk::Human, attributes, ..Character::roll(4) };
        press(&mut app, KeyCode::KeyK);
        let mut pack = None;
        // STR 11 carries 165 lb: Light to 55, Burdened to 110, Encumbered to 165.
        for (lb, word, colour) in [(40.0, "40/165", BRIGHT), (65.0, "65/165", AMBER), (137.5, "138/165", ORANGE), (172.0, "172/165", RED)] {
            {
                let mut inventory = app.world_mut().resource_mut::<Inventory>();
                if let Some(old) = pack {
                    inventory.forget(old);
                }
                let new = inventory.add(weighed_with("Pack", 7, None, lb));
                inventory.wear(new, Slot::Back).unwrap();
                pack = Some(new);
            }
            app.update();
            assert_eq!(load_colours(&mut app, word), (colour, colour), "{lb} lb");
        }
    }

    #[test]
    fn the_headers_load_is_a_hover_target_for_the_breakdown() {
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyI);
        let mut targets = app.world_mut().query::<(&sheet::GearRow, &Children)>();
        let found: Vec<usize> = targets.iter(app.world()).map(|(row, _)| row.0).collect();
        assert_eq!(found, vec![0], "only the load part of the header is a target, on the inventory tab");
        let (_, children) = targets.single(app.world()).unwrap();
        let text = children[0];
        let spans: String = app.world().get::<Children>(text).unwrap().iter().filter_map(|s| app.world().get::<TextSpan>(s)).map(|t| t.0.clone()).collect();
        assert!(spans.starts_with("Load "), "{spans:?}");
        // Hovered from the header, on the inventory tab, the breakdown opens.
        hover_row(&mut app, Some(0));
        let tip = tip_text(&mut app);
        assert!(tip.iter().any(|l| l.starts_with("Capacity: STR ")), "{tip:?}");
        let mut tips = app.world_mut().query_filtered::<&Visibility, With<sheet::BreakdownTip>>();
        assert_eq!(*tips.single(app.world()).unwrap(), Visibility::Inherited);
    }

    #[test]
    fn the_header_s_stats_update_when_equipping() {
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyI);
        let bare = header_stats(&mut app);
        assert!(bare.starts_with("AC ") && bare.contains("Load 0/"), "{bare}");
        let helm = (1..300).filter_map(|s| fb_items::roll_item("Helm", s)).find(|i| i.profile.stats.armour >= 1.0).unwrap();
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let id = inventory.add(helm);
            equip(&mut inventory, id, Slot::Head).unwrap();
        }
        app.update();
        let helmed = header_stats(&mut app);
        assert_ne!(helmed, bare);
        let character = app.world().resource::<Character>().clone();
        assert_eq!(helmed, header::stats_line(&character.derive(app.world().resource::<Inventory>())));
    }

    #[test]
    fn the_slots_sit_round_the_doll_and_empty_ones_show_their_names() {
        let mut app = screen_app();
        press(&mut app, KeyCode::KeyI);
        let mut slots = app.world_mut().query::<(&SlotView, &Node, &Children)>();
        let found: Vec<(Slot, Node, Vec<Entity>)> = slots.iter(app.world()).map(|(s, n, c)| (s.0, n.clone(), c.to_vec())).collect();
        assert_eq!(found.len(), fb_items::tables::slot_places().len());
        for place in fb_items::tables::slot_places() {
            let (_, node, children) = found.iter().find(|(s, ..)| *s == place.slot).unwrap();
            let x = px(doll::FRAME.x * place.x_percent / 100.0);
            match place.side {
                fb_items::tables::SlotSide::Left => assert_eq!(node.left, x, "{:?}", place.slot),
                fb_items::tables::SlotSide::Right => assert_eq!(node.right, x, "{:?}", place.slot),
            }
            assert_eq!(node.top, px(doll::FRAME.y * place.y_percent / 100.0));
            let labels: Vec<String> = children.iter().filter_map(|c| app.world().get::<Text>(*c)).map(|t| t.0.clone()).collect();
            assert_eq!(labels, vec![place.label.clone()], "{:?}", place.slot);
        }
        let mut views = app.world_mut().query::<&DollView>();
        assert_eq!(views.single(app.world()).unwrap().image, app.world().resource::<DollAssets>().doll);
    }

    #[test]
    fn dropping_on_a_slot_wears_what_is_made_for_it() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.wear(pack, Slot::Back).unwrap();
        let gloves = inventory.add(fb_items::roll_item("Gloves", 4).unwrap());
        inventory.put(gloves, pack, Placement::new(0, 0, false)).unwrap();
        let drag = Drag { id: gloves, grab: IVec2::ZERO, size: inventory.footprint(gloves).unwrap(), turned: false, press: Vec2::ZERO, active: true, target: None };
        assert_eq!(check_drop(&inventory, &drag, Target::Slot(Slot::Head)), Err(Refusal::WrongSlot));
        assert_eq!(drop_at(&mut inventory, &drag, Target::Slot(Slot::Hands), 6), Ok(vec![]));
        assert_eq!(inventory.worn(Slot::Hands), Some(gloves));
        // Lying on the floor, it is lifted from the level as it goes on.
        let ring = inventory.add(fb_items::roll_item("Ring", 2).unwrap());
        let drag = Drag { id: ring, ..drag };
        assert_eq!(drop_at(&mut inventory, &drag, Target::Slot(Slot::Ring2), 6), Ok(vec![FloorChange::Lifted(ring)]));
    }

    #[test]
    fn a_sword_dropped_on_the_doll_goes_in_the_main_hand_and_swaps() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.wear(pack, Slot::Back).unwrap();
        let sword = inventory.add(fb_items::roll_item("Sword", 4).unwrap());
        inventory.put(sword, pack, Placement::new(0, 0, false)).unwrap();
        let drag = Drag { id: sword, grab: IVec2::ZERO, size: inventory.footprint(sword).unwrap(), turned: false, press: Vec2::ZERO, active: true, target: None };
        assert_eq!(check_drop(&inventory, &drag, Target::Doll), Ok(()));
        assert_eq!(drop_at(&mut inventory, &drag, Target::Doll, 6), Ok(vec![]));
        assert_eq!(inventory.worn(Slot::Mainhand), Some(sword));
        // A mace lying underfoot dropped on the main hand: lifted, and the sword has nowhere
        // to go back to but the floor.
        let mace = inventory.add(fb_items::roll_item("Mace", 2).unwrap());
        let drag = Drag { id: mace, ..drag };
        assert_eq!(drop_at(&mut inventory, &drag, Target::Slot(Slot::Mainhand), 6), Ok(vec![FloorChange::Lifted(mace), FloorChange::SetDown(sword, 0.0)]));
        let bread = inventory.add(fb_items::roll_item("Bread", 1).unwrap());
        assert_eq!(check_drop(&inventory, &Drag { id: bread, ..drag }, Target::Doll), Err(Refusal::NotWearable));
    }

    /// Whether `item` posed for its icon lying `turned` stands as it is worn: its own up (+Y)
    /// still up the image and its front (+Z) toward the viewer.
    fn icon_upright(item: &fb_items::Item, turned: bool) -> bool {
        let points: Vec<Vec3> = item.shape.build().parts.iter().flat_map(|p| &p.positions).map(|&p| Vec3::from_array(p)).collect();
        let (rotation, ..) = icon::icon_pose(&points, icon::stands_upright(item), turned).unwrap();
        (rotation * Vec3::Y).y > 0.9 && (rotation * Vec3::Z).z > 0.9
    }

    #[test]
    fn r_turns_a_coif_lying_turned_back_upright() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.wear(pack, Slot::Back).unwrap();
        let coif = fb_items::roll_item("Mail coif", 3).unwrap();
        assert!(icon_upright(&coif, false), "a coif's icon stands as it is worn");
        assert!(!icon_upright(&coif, true));
        let coif = inventory.add(coif);
        inventory.put(coif, pack, Placement::new(0, 0, true)).unwrap();
        let entry = *inventory.grid(pack).unwrap().get(coif).unwrap();
        let mut drag = Drag { id: coif, grab: IVec2::ZERO, size: entry.size, turned: entry.at.turned, press: Vec2::ZERO, active: true, target: None };
        // R steps through both ways the footprint lies, back to upright.
        drag.turn();
        assert!(!drag.turned && icon_upright(inventory.item(coif).unwrap(), drag.turned));
        assert_eq!(drag.size, inventory.footprint(coif).unwrap());
        drag.turn();
        assert!(drag.turned);
        drag.turn();
        let into_pack = Target::Cell { view: GridView::Container(pack), cell: IVec2::new(4, 2) };
        assert_eq!(drop_at(&mut inventory, &drag, into_pack, 6), Ok(vec![]));
        assert_eq!(inventory.location(coif), Some(Location::In { container: pack, at: Placement::new(4, 2, false) }));
    }

    #[test]
    fn worn_gear_icons_stand_upright_and_weapons_lie_along_their_length() {
        for base in ["Mail coif", "Hood", "Helm", "Pointed hat", "Tunic", "Cloak", "Trousers", "Gloves", "Boots", "Belt", "Amulet", "Ring", "Charm", "Pack", "Sack"] {
            for seed in [1, 2, 1207] {
                let item = fb_items::roll_item(base, seed).unwrap();
                assert!(icon::stands_upright(&item) && icon_upright(&item, false), "{base} {seed}");
            }
        }
        for base in ["Sword", "Staff", "Spear", "Dagger", "Mace", "Bow", "Torch"] {
            assert!(!icon::stands_upright(&fb_items::roll_item(base, 1).unwrap()), "{base}");
        }
    }

    #[test]
    fn taking_something_off_lands_upright_where_it_fits_else_turned() {
        let mut inventory = Inventory::default();
        let belt = inventory.add(fb_items::roll_item("Belt", 3).unwrap());
        inventory.wear(belt, Slot::Belt).unwrap();
        // The belt pouch is 4 by 2. Rings fill all but its top-left two cells.
        for (x, y) in [(2, 0), (3, 0), (0, 1), (1, 1), (2, 1), (3, 1)] {
            let ring = inventory.add(fb_items::roll_item("Ring", x + 4 * y).unwrap());
            inventory.put(ring, belt, Placement::new(x as u8, y as u8, false)).unwrap();
        }
        let gloves = inventory.add(fb_items::roll_item("Gloves", 4).unwrap());
        inventory.wear(gloves, Slot::Hands).unwrap();
        let size = inventory.footprint(gloves).unwrap();
        assert_eq!(size, Footprint { cols: 1, rows: 2 }, "gloves stand a cell wide and two tall");
        // Taken off by the slot and let go over the top-left cell: upright it would run onto a
        // ring, so it lies turned.
        let drag = Drag { id: gloves, grab: IVec2::ZERO, size, turned: false, press: Vec2::ZERO, active: true, target: None };
        let corner = Target::Cell { view: GridView::Container(belt), cell: IVec2::ZERO };
        assert_eq!(check_drop(&inventory, &drag, corner), Ok(()));
        assert_eq!(drop_at(&mut inventory, &drag, corner, 6), Ok(vec![]));
        assert_eq!(inventory.location(gloves), Some(Location::In { container: belt, at: Placement::new(0, 0, true) }));
        // Where it fits upright, it lies upright.
        inventory.wear(gloves, Slot::Hands).unwrap();
        let ring = inventory.grid(belt).unwrap().at_cell(0, 1).unwrap().id;
        inventory.take_out(ring);
        assert_eq!(drop_at(&mut inventory, &drag, corner, 6), Ok(vec![]));
        assert_eq!(inventory.location(gloves), Some(Location::In { container: belt, at: Placement::new(0, 0, false) }));
    }

    #[test]
    fn double_clicking_wears_and_takes_off() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.wear(pack, Slot::Back).unwrap();
        let gloves = inventory.add(fb_items::roll_item("Gloves", 4).unwrap());
        inventory.put(gloves, pack, Placement::new(0, 0, false)).unwrap();
        assert_eq!(quick(&mut inventory, gloves), Ok(vec![]));
        assert_eq!(inventory.worn(Slot::Hands), Some(gloves));
        assert_eq!(quick(&mut inventory, gloves), Ok(vec![]));
        assert!(matches!(inventory.location(gloves), Some(Location::In { container, .. }) if container == pack));
        // Bread lying underfoot is picked up.
        let bread = inventory.add(fb_items::roll_item("Bread", 1).unwrap());
        assert_eq!(quick(&mut inventory, bread), Ok(vec![FloorChange::Lifted(bread)]));
    }

    fn purse_of(coins: fb_items::Coins) -> fb_items::Item {
        let mut parts = fb_items::ItemParts::from(fb_items::roll_item("Purse", 4).unwrap());
        parts.coins = Some(coins);
        fb_items::Item::from_parts(parts).unwrap()
    }

    #[test]
    fn double_clicking_a_purse_empties_it_into_the_coin_purse() {
        use fb_items::Coins;
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.wear(pack, Slot::Back).unwrap();
        inventory.set_coins(Coins { gp: 1, sp: 0, cp: 0 });
        let purse = inventory.add(purse_of(Coins { gp: 2, sp: 3, cp: 0 }));
        inventory.stow(purse).unwrap();
        assert_eq!(quick(&mut inventory, purse), Ok(vec![]));
        assert_eq!(inventory.coins(), Coins { gp: 3, sp: 3, cp: 0 });
        assert_eq!(inventory.item(purse), None);
        assert_eq!(inventory.contents_count(pack), 0);
        assert_eq!(opened_line(Coins { gp: 2, sp: 3, cp: 0 }), "You empty the purse: 2 gp 3 sp.");
        assert_eq!(opened_line(Coins::default()), "The purse is empty.");
        // One lying underfoot is lifted from the level as it is opened.
        let lying = inventory.add(purse_of(Coins { gp: 0, sp: 0, cp: 7 }));
        assert_eq!(quick(&mut inventory, lying), Ok(vec![FloorChange::Lifted(lying)]));
        assert_eq!(inventory.coins(), Coins { gp: 3, sp: 3, cp: 7 });
        assert_eq!(inventory.item(lying), None);
    }

    #[test]
    fn a_purse_in_a_chest_is_opened_where_it_lies() {
        let mut inventory = Inventory::default();
        let (chest, bread) = chest_with_bread(&mut inventory);
        inventory.forget(bread);
        let purse = inventory.add(purse_of(fb_items::Coins { gp: 5, sp: 0, cp: 0 }));
        inventory.put(purse, chest, Placement::new(0, 0, false)).unwrap();
        assert_eq!(quick(&mut inventory, purse), Ok(vec![]));
        assert_eq!(inventory.coins().gp, 5);
        assert_eq!(inventory.contents_count(chest), 0);
    }

    fn chest_with_bread(inventory: &mut Inventory) -> (ItemId, ItemId) {
        let chest = inventory.add(fb_items::container_item(fb_level::chest_state(armory::families::chests::ChestStyle::Chest, 3, false)));
        let bread = inventory.add(fb_items::roll_item("Bread", 1).unwrap());
        inventory.put(bread, chest, Placement::new(2, 1, false)).unwrap();
        (chest, bread)
    }

    #[test]
    fn double_clicking_takes_from_a_chest_standing_in_the_level() {
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 7).unwrap());
        inventory.wear(pack, Slot::Back).unwrap();
        let (chest, bread) = chest_with_bread(&mut inventory);
        assert_eq!(quick(&mut inventory, bread), Ok(vec![]));
        assert!(matches!(inventory.location(bread), Some(Location::In { container, .. }) if container == pack));
        assert_eq!(inventory.contents_count(chest), 0);
    }

    #[test]
    fn an_open_container_shows_as_its_own_grid_until_the_screen_closes() {
        let mut app = screen_app();
        app.add_systems(Update, close_container_with_screen.after(switch_screens).before(build_screen));
        let (chest, _) = chest_with_bread(&mut app.world_mut().resource_mut::<Inventory>());
        let grids = |app: &mut App| {
            let mut views = app.world_mut().query::<&GridView>();
            views.iter(app.world()).copied().collect::<Vec<_>>()
        };
        app.world_mut().resource_mut::<OpenContainer>().0 = Some(chest);
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        app.update();
        assert!(grids(&mut app).contains(&GridView::Container(chest)), "{:?}", grids(&mut app));
        let mut items = app.world_mut().query::<&ItemView>();
        assert_eq!(items.iter(app.world()).count(), 1, "the bread in it");
        press(&mut app, KeyCode::KeyI);
        assert_eq!(app.world().resource::<OpenContainer>().0, None);
        press(&mut app, KeyCode::KeyI);
        assert!(!grids(&mut app).contains(&GridView::Container(chest)));
    }

    /// An app that runs only the column's scrolling, over a column of `content` pixels in a
    /// 400 by 300 box centred at (200, 150), with the mouse at `at`.
    fn scroll_app(content: f32, at: Vec2) -> (App, Entity) {
        let mut app = App::new();
        app.init_resource::<ScreenOpen>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<Time<Real>>()
            .init_resource::<Dragging>()
            .init_resource::<ColumnScroll>()
            .add_systems(Update, scroll_column);
        let mut window = Window::default();
        window.set_cursor_position(Some(at));
        app.world_mut().spawn((window, PrimaryWindow));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        let column = app
            .world_mut()
            .spawn((
                ScrollColumn,
                ScrollPosition::default(),
                ComputedNode { size: Vec2::new(400.0, 300.0), content_size: Vec2::new(400.0, content), inverse_scale_factor: 1.0, ..default() },
                UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(200.0, 150.0))),
            ))
            .id();
        (app, column)
    }

    fn wheel(app: &mut App, lines: f32) {
        app.world_mut().resource_mut::<AccumulatedMouseScroll>().delta = Vec2::new(0.0, lines);
        app.update();
        app.world_mut().resource_mut::<AccumulatedMouseScroll>().delta = Vec2::ZERO;
    }

    fn offset(app: &App, column: Entity) -> f32 {
        app.world().get::<ScrollPosition>(column).unwrap().y
    }

    #[test]
    fn the_wheel_scrolls_a_tall_column_to_its_end_and_no_further() {
        let (mut app, column) = scroll_app(900.0, Vec2::new(200.0, 150.0));
        wheel(&mut app, -1.0);
        assert_eq!(offset(&app, column), SCROLL_LINE_PX);
        wheel(&mut app, -100.0);
        assert_eq!(offset(&app, column), 600.0, "900 of content in 300");
        assert_eq!(app.world().resource::<ColumnScroll>().0, 600.0, "remembered for the next rebuild");
        wheel(&mut app, 100.0);
        assert_eq!(offset(&app, column), 0.0);
    }

    #[test]
    fn a_column_that_fits_does_not_scroll() {
        let (mut app, column) = scroll_app(280.0, Vec2::new(200.0, 150.0));
        wheel(&mut app, -5.0);
        assert_eq!(offset(&app, column), 0.0);
    }

    #[test]
    fn the_wheel_scrolls_the_shelf_and_not_the_column_under_it() {
        let (mut app, column) = scroll_app(900.0, Vec2::new(200.0, 150.0));
        app.init_resource::<ShelfScrolled>().add_systems(Update, scroll_shelf);
        let shelf = app
            .world_mut()
            .spawn((
                ShelfScroll,
                ScrollPosition::default(),
                ComputedNode { size: Vec2::new(300.0, 200.0), content_size: Vec2::new(300.0, 500.0), inverse_scale_factor: 1.0, ..default() },
                UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(200.0, 150.0))),
            ))
            .id();
        wheel(&mut app, -1.0);
        assert_eq!(offset(&app, shelf), SCROLL_LINE_PX);
        assert_eq!(offset(&app, column), 0.0, "the column stays while the mouse is over the shelf");
        wheel(&mut app, -100.0);
        assert_eq!(offset(&app, shelf), 300.0, "500 of content in 200");
        assert_eq!(app.world().resource::<ShelfScrolled>().0, 300.0);
    }

    #[test]
    fn a_shelf_scrolls_in_an_area_that_fits_the_ledger_at_any_window_size() {
        // Ten pieces of three-by-two cells lay out five columns wide, seven rows deep with the
        // spare ones: taller than the area, so it scrolls, and the area is the same height in
        // interface pixels at 1280 by 720 and at 1920 by 1080, since the interface scales.
        let cell = Tuning::default().interface.cell_size;
        let grid = grid_size(&Grid::<ItemId>::new(5, 12), cell).y;
        assert!(grid > SHELF_MAX_PX && shelf_view_height(grid) == SHELF_MAX_PX);
        assert_eq!(shelf_view_height(100.0), 100.0, "a short shelf does not scroll");
        // Header (about 200), the balance panel (about 200) and the shelf's label leave room.
        let ledger = crate::tuning::InterfaceTuning::REFERENCE_HEIGHT * 0.92 - 44.0;
        assert!(200.0 + 200.0 + 40.0 + SHELF_MAX_PX <= ledger, "{ledger}");
    }

    #[test]
    fn the_wheel_off_the_column_does_nothing() {
        let (mut app, column) = scroll_app(900.0, Vec2::new(500.0, 150.0));
        wheel(&mut app, -3.0);
        assert_eq!(offset(&app, column), 0.0);
    }

    #[test]
    fn a_held_item_near_the_bottom_edge_scrolls_down_and_near_the_top_back_up() {
        let (mut app, column) = scroll_app(900.0, Vec2::new(200.0, 295.0));
        let held = Drag { id: ItemId(1), grab: IVec2::ZERO, size: Footprint { cols: 1, rows: 1 }, turned: false, press: Vec2::ZERO, active: true, target: None };
        app.world_mut().resource_mut::<Dragging>().0 = Some(held);
        let frame = |app: &mut App| {
            app.world_mut().resource_mut::<Time<Real>>().advance_by(std::time::Duration::from_millis(100));
            app.update();
        };
        frame(&mut app);
        assert!(offset(&app, column) > 40.0, "{}", offset(&app, column));
        for _ in 0..40 {
            frame(&mut app);
        }
        assert_eq!(offset(&app, column), 600.0);
        let mut windows = app.world_mut().query::<&mut Window>();
        windows.single_mut(app.world_mut()).unwrap().set_cursor_position(Some(Vec2::new(200.0, 5.0)));
        frame(&mut app);
        assert!(offset(&app, column) < 600.0);
        // Carrying nothing, the edge does nothing.
        app.world_mut().resource_mut::<Dragging>().0 = None;
        let before = offset(&app, column);
        frame(&mut app);
        assert_eq!(offset(&app, column), before);
    }

    #[test]
    fn a_grid_scrolled_out_of_the_column_is_not_a_drop_target() {
        let mut world = World::new();
        world.spawn((
            GridView::Underfoot,
            ComputedNode { size: Vec2::new(100.0, 100.0), inverse_scale_factor: 1.0, ..default() },
            UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(50.0, 50.0))),
        ));
        let mut state = bevy::ecs::system::SystemState::<(
            Query<(&QuickslotDrop, &ComputedNode, &UiGlobalTransform)>,
            Query<(&GridView, &ComputedNode, &UiGlobalTransform)>,
            Query<(&SlotView, &ComputedNode, &UiGlobalTransform)>,
        )>::new(&mut world);
        let (drops, grids, slots) = state.get(&world).unwrap();
        let page = Some(Rect::new(0.0, 0.0, 400.0, 400.0));
        let at = |column: Option<Rect>| target_under(Vec2::new(10.0, 10.0), &drops, &grids, &slots, None, page, column, None, 72.0);
        assert!(matches!(at(None), Target::Cell { .. }));
        assert!(matches!(at(Some(Rect::new(0.0, 0.0, 200.0, 200.0))), Target::Cell { cell, .. } if cell == IVec2::ZERO));
        // The column starts below this point: the grid is under the header, clipped away.
        assert_eq!(at(Some(Rect::new(0.0, 80.0, 200.0, 200.0))), Target::Page);
    }
}
