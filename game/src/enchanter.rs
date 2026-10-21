//! An enchanter's bench: the Enchant and Scrap topics of their talk ([`crate::talk`]) open it.
//!
//! - **Enchant.** Pick one of the gear you carry (worn or packed) from the list on the left. The
//!   right shows its card and what it carries, a Strip button on each affix, the affixes the
//!   enchanter could add (the pools the item could roll from, within their tier) and Reforge, each
//!   with what it costs in essence, special materials and coin ([`fb_items::enchant`]). Doing
//!   it spends from the delver's [`Enchanting`] stock and carried purses ([`crate::enchanting`]),
//!   and the item changes where it lies; its card stays up as the result, with the new history
//!   line.
//! - **Scrap.** Pick gear and scrap it into essence and materials ([`crate::enchanting::scrap_item`]);
//!   what it gave shows as the result.
//! - The screen is laid out for the 1920 by 1080 reference and scales with the window
//!   ([`crate::ui`]); the mouse is read in the interface's own pixels ([`ui_cursor`]). Back
//!   returns to the talk; Esc leaves, and the folk's hold is released however it closes
//!   ([`crate::talk`] treats the bench as the talk going on).
//! - `FB_ENCHANT=<select|strip|reforge|offer|legendary|scrap>` fills the delver's pack and stock a few frames in
//!   and, once the bench is open (`FB_TALK=1 FB_TALK_TOPIC=<n>` opens it), picks an item and does
//!   that, for captures.

use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_items::enchant::{self, Enchanter, Refusal};
use fb_items::scrap::scrap;
use fb_items::tables::Affix;
use fb_items::{Coins, Item};

use crate::carry::ViewModelCamera;
use crate::enchanting::{self, Declined, Enchanting, Work, cost_line, quote, shortfall, tier_of, wallet};
use crate::inventory::card::card_text;
use crate::inventory::{Inventory, ItemId};
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, Screen, ScreenOpen, show_screen};
use crate::talk::{Service, ServiceChosen, Talk};
use crate::ui::{ACCENT, BAD, BG, CELL, FAINT, INK, LINE, MUTED, OK, UiFonts, node_rect, rarity_color, ui_cursor};

/// The width of the bench at the reference size, logical pixels.
const WIDTH: f32 = 1560.0;
/// Its height.
const HEIGHT: f32 = 900.0;
/// The width of the list of what the delver carries.
const LIST_WIDTH: f32 = 520.0;
/// How far one line of the wheel scrolls, logical pixels.
const SCROLL_LINE: f32 = 56.0;

pub struct EnchanterPlugin;

impl Plugin for EnchanterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Bench>();
        if std::env::var("FB_ENCHANT").is_ok_and(|v| !v.is_empty()) {
            app.add_systems(Update, (enchanting::bench_kit_for_capture, drive_for_capture).in_set(DelveSystems));
        }
        app.add_systems(OnExit(crate::launch::AppState::Delving), |mut bench: ResMut<Bench>| *bench = Bench::default());
        app.add_systems(Update, (open_bench, click_bench, close_bench, build_bench, hover_bench, scroll_bench).chain().in_set(DelveSystems));
    }
}

/// What a button at the bench does.
#[derive(Clone, Copy, Debug)]
enum Do {
    Pick(ItemId),
    Strip(&'static Affix),
    Add(&'static Affix),
    Reforge,
    Legend(Option<&'static Affix>),
    Scrap,
    Back,
}

/// A button at the bench. `enabled` ones light under the mouse; `scrolls` ones sit in a scrolled
/// column and count only where that column shows.
#[derive(Component, Debug)]
struct BenchButton {
    act: Do,
    enabled: bool,
    scrolls: bool,
}

/// A column that scrolls with the wheel.
#[derive(Component, Debug)]
struct ScrollColumn;

/// The bench's root.
#[derive(Component, Debug)]
pub struct BenchScreen;

/// What the last scrapping gave, for the result card.
#[derive(Clone, Debug, PartialEq)]
struct Scrapped {
    title: String,
    gave: String,
}

/// The bench in use.
#[derive(Debug)]
pub struct Open {
    mode: Service,
    by: Enchanter,
    selected: Option<ItemId>,
    /// What the last thing done said, and whether it went well.
    notice: Option<(String, bool)>,
    scrapped: Option<Scrapped>,
    /// The level the bench is on, for the legend a made legendary tells.
    level: u32,
}

#[derive(Resource, Default, Debug)]
pub struct Bench {
    open: Option<Open>,
    revision: u32,
}

impl Bench {
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    fn changed(&mut self) {
        self.revision += 1;
    }
}

/// The gear the delver carries that the bench could work on, richest first.
fn candidates(inventory: &Inventory, mode: Service) -> Vec<ItemId> {
    let mut ids: Vec<ItemId> = inventory
        .ids()
        .filter(|&id| inventory.is_carried(id))
        .filter(|&id| {
            let Some(item) = inventory.item(id) else { return false };
            match mode {
                Service::Scrap => inventory.contents_count(id) == 0 && scrap(item).is_some(),
                _ => enchant::enchantable(item),
            }
        })
        .collect();
    ids.sort_by_key(|&id| inventory.item(id).map(|i| (std::cmp::Reverse(i.rarity), i.name.clone())));
    ids
}

/// The talk chose Enchant or Scrap: open the bench on the speaker.
fn open_bench(mut chosen: MessageReader<ServiceChosen>, talk: Res<Talk>, level: Option<Res<crate::play::CurrentLevel>>, mut bench: ResMut<Bench>, mut open: ResMut<ScreenOpen>, mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>) {
    for ServiceChosen { service, .. } in chosen.read() {
        if !matches!(service, Service::Enchant | Service::Scrap) {
            continue;
        }
        let Some(speaker) = talk.speaker.as_ref() else { continue };
        let tier = speaker.trade.map_or(enchant::Tier::Novice, |t| tier_of(t.tier));
        bench.open = Some(Open { mode: *service, by: Enchanter { name: speaker.name.clone(), tier, faction: speaker.faction.clone() }, selected: None, notice: None, scrapped: None, level: level.as_ref().map_or(0, |l| l.0.depth) });
        bench.changed();
        show_screen(&mut open, Some(Screen::Enchant), cursor.as_deref_mut().map(|c| &mut **c));
    }
}

/// Forgets the bench when the screen closes.
fn close_bench(open: Res<ScreenOpen>, mut bench: ResMut<Bench>) {
    if bench.open.is_some() && !open.is(Screen::Enchant) {
        bench.open = None;
        bench.changed();
    }
}

/// Does `act` at the bench. Returns whether to go back to the talk.
fn act(bench: &mut Bench, inventory: &mut Inventory, stock: &mut Enchanting, act: Do) -> bool {
    let Some(open) = bench.open.as_mut() else { return false };
    match act {
        Do::Back => return true,
        Do::Pick(id) => {
            open.selected = Some(id);
            open.notice = None;
            open.scrapped = None;
        }
        Do::Strip(affix) => finish(open, inventory, stock, Work::Strip(affix)),
        Do::Add(affix) => finish(open, inventory, stock, Work::Add(affix)),
        Do::Reforge => finish(open, inventory, stock, Work::Reforge),
        Do::Legend(affix) => finish(open, inventory, stock, Work::Legend(affix)),
        Do::Scrap => {
            if let Some(id) = open.selected {
                let title = inventory.item(id).map(|i| i.title().to_string()).unwrap_or_default();
                match enchanting::scrap_item(inventory, &mut stock.0, id) {
                    Some(salvage) => {
                        let mut gave = vec![format!("{} essence", salvage.essence)];
                        gave.extend(salvage.materials.iter().map(|m| format!("{} \u{d7}{}", fb_items::scrap::material_name(&m.id), m.count)));
                        open.scrapped = Some(Scrapped { title: title.clone(), gave: gave.join(" \u{b7} ") });
                        open.notice = Some((format!("Scrapped {title}."), true));
                        open.selected = None;
                    }
                    None => open.notice = Some(("That cannot be scrapped.".into(), false)),
                }
            }
        }
    }
    bench.changed();
    false
}

/// Does `work` on the selected item and says what came of it.
fn finish(open: &mut Open, inventory: &mut Inventory, stock: &mut Enchanting, work: Work) {
    let Some(id) = open.selected else { return };
    let verb = match work {
        Work::Add(a) => format!("Added {}", a.name),
        Work::Strip(a) => format!("Stripped {}", a.name),
        Work::Reforge => "Reforged it".to_string(),
        Work::Legend(_) => "Made it legendary".to_string(),
    };
    open.notice = Some(match enchanting::do_work_in(inventory, &mut stock.0, id, work, &open.by, open.level) {
        Ok(cost) => (format!("{verb}. Paid {}.", cost_line(&cost)), true),
        Err(declined @ (Declined::Refused(_) | Declined::Short(_))) => (declined.reason(), false),
    });
}

/// A click on a button.
#[allow(clippy::too_many_arguments)]
fn click_bench(
    input: ActionInput,
    mut bench: ResMut<Bench>,
    mut inventory: ResMut<Inventory>,
    mut stock: ResMut<Enchanting>,
    mut screen: ResMut<ScreenOpen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    buttons: Query<(&BenchButton, &ComputedNode, &UiGlobalTransform)>,
    columns: Query<(&ComputedNode, &UiGlobalTransform), With<ScrollColumn>>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
) {
    if !screen.is(Screen::Enchant) || !input.just_pressed(Action::Choose) {
        return;
    }
    let Some(point) = windows.iter().next().and_then(|w| ui_cursor(w, ui_scale.as_deref())) else { return };
    let hit = buttons.iter().find(|(b, node, at)| node_rect(node, at).contains(point) && (!b.scrolls || columns.iter().any(|(n, t)| node_rect(n, t).contains(point))));
    let Some((button, ..)) = hit else { return };
    if !button.enabled {
        return;
    }
    if act(&mut bench, &mut inventory, &mut stock, button.act) {
        show_screen(&mut screen, Some(Screen::Talk), cursor.as_deref_mut().map(|c| &mut **c));
    }
}

/// Lights the enabled button under the mouse.
fn hover_bench(
    open: Res<ScreenOpen>,
    bench: Res<Bench>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    columns: Query<(&ComputedNode, &UiGlobalTransform), With<ScrollColumn>>,
    mut buttons: Query<(&BenchButton, &ComputedNode, &UiGlobalTransform, &mut BackgroundColor, &mut BorderColor)>,
) {
    if !open.is(Screen::Enchant) {
        return;
    }
    let point = windows.iter().next().and_then(|w| ui_cursor(w, ui_scale.as_deref()));
    for (button, node, at, mut fill, mut border) in &mut buttons {
        let over = button.enabled && point.is_some_and(|p| node_rect(node, at).contains(p) && (!button.scrolls || columns.iter().any(|(n, t)| node_rect(n, t).contains(p))));
        let picked = matches!(button.act, Do::Pick(id) if bench.open.as_ref().and_then(|o| o.selected) == Some(id));
        let (want_fill, want_border) = if over { (FAINT, LINE) } else if picked { (CELL, ACCENT) } else { (CELL, FAINT) };
        if fill.0 != want_fill {
            fill.0 = want_fill;
        }
        if *border != BorderColor::all(want_border) {
            *border = BorderColor::all(want_border);
        }
    }
}

/// The wheel moves the column under the mouse.
fn scroll_bench(
    open: Res<ScreenOpen>,
    scroll: Res<AccumulatedMouseScroll>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    mut columns: Query<(&mut ScrollPosition, &ComputedNode, &UiGlobalTransform), With<ScrollColumn>>,
) {
    if !open.is(Screen::Enchant) {
        return;
    }
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    let Some(point) = windows.iter().next().and_then(|w| ui_cursor(w, ui_scale.as_deref())) else { return };
    if lines == 0.0 {
        return;
    }
    for (mut position, node, at) in &mut columns {
        if node_rect(node, at).contains(point) {
            let max = ((node.content_size() - node.size()) * node.inverse_scale_factor).y.max(0.0);
            position.y = (position.y - lines * SCROLL_LINE).clamp(0.0, max);
        }
    }
}

/// A line of text.
fn line(parent: &mut ChildSpawnerCommands, _fonts: &UiFonts, text: impl Into<String>, font: &Handle<Font>, size: f32, colour: Color) {
    parent.spawn((Text::new(text), UiFonts::style(font, size, colour)));
}

/// A button's frame.
fn frame(button: BenchButton) -> impl Bundle {
    (
        button,
        Node {
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            column_gap: px(16),
            padding: UiRect::new(px(14), px(14), px(7), px(8)),
            border: UiRect::all(px(1.0)),
            border_radius: BorderRadius::all(px(8)),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(CELL),
        BorderColor::all(FAINT),
    )
}

/// The delver's stock on one line: essence, materials and coin.
fn stock_line(stock: &Enchanting, purse: Coins) -> String {
    format!("{} \u{b7} {}", stock.0.line(), if purse.is_empty() { "no coin".to_string() } else { purse.short() })
}

/// Draws the bench while it is open, and again whenever it changes.
#[allow(clippy::too_many_arguments)]
fn build_bench(
    mut commands: Commands,
    screen: Res<ScreenOpen>,
    bench: Res<Bench>,
    inventory: Res<Inventory>,
    stock: Res<Enchanting>,
    fonts: Option<Res<UiFonts>>,
    cameras: Query<Entity, With<ViewModelCamera>>,
    roots: Query<Entity, With<BenchScreen>>,
    mut drawn: Local<Option<u32>>,
) {
    let existing = roots.iter().next();
    let (true, Some(open), Some(fonts)) = (screen.is(Screen::Enchant), bench.open.as_ref(), fonts) else {
        if let Some(root) = existing {
            commands.entity(root).despawn();
        }
        *drawn = None;
        return;
    };
    if existing.is_some() && *drawn == Some(bench.revision) {
        return;
    }
    let Some(camera) = cameras.iter().next() else { return };
    let root = match existing {
        Some(root) => {
            commands.entity(root).despawn_children();
            root
        }
        None => commands
            .spawn((
                Name::new("Enchanter's bench"),
                BenchScreen,
                DespawnOnExit(crate::launch::AppState::Delving),
                UiTargetCamera(camera),
                Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                GlobalZIndex(12),
            ))
            .id(),
    };
    *drawn = Some(bench.revision);
    let purse = wallet(&inventory);
    let list = candidates(&inventory, open.mode);
    let by = &open.by;
    commands.entity(root).with_children(|root| {
        root.spawn((
            Name::new("Bench panel"),
            Node {
                width: px(WIDTH),
                height: px(HEIGHT),
                max_width: percent(96),
                max_height: percent(94),
                flex_direction: FlexDirection::Column,
                row_gap: px(12),
                padding: UiRect::new(px(28), px(28), px(20), px(20)),
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(14)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(BG),
            BorderColor::all(LINE),
            BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.55), px(0), px(14), px(0), px(34)),
        ))
        .with_children(|panel| {
            // The head: who, and what the delver has to spend.
            panel.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::FlexEnd, column_gap: px(24), flex_shrink: 0.0, ..default() }).with_children(|head| {
                head.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|who| {
                    line(who, &fonts, if open.mode == Service::Scrap { "Scrap" } else { "Enchant" }, &fonts.display, 34.0, INK);
                    line(who, &fonts, format!("{}, {} enchanter of {}", by.name, by.tier.label(), by.faction), &fonts.body_italic, 18.0, MUTED);
                });
                line(head, &fonts, stock_line(&stock, purse), &fonts.mono, 15.0, ACCENT);
            });
            panel.spawn((Node { height: px(1), flex_shrink: 0.0, ..default() }, BackgroundColor(FAINT)));
            panel.spawn(Node { flex_grow: 1.0, min_height: px(0), column_gap: px(26), ..default() }).with_children(|body| {
                // The list of what the delver carries.
                body.spawn((ScrollColumn, ScrollPosition::default(), Node { width: px(LIST_WIDTH), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: px(6), overflow: Overflow::scroll_y(), ..default() })).with_children(|col| {
                    line(col, &fonts, "You carry", &fonts.mono, 13.0, MUTED);
                    if list.is_empty() {
                        line(col, &fonts, if open.mode == Service::Scrap { "Nothing they could take apart." } else { "Nothing they could work on." }, &fonts.body_italic, 18.0, MUTED);
                    }
                    for &id in &list {
                        let Some(item) = inventory.item(id) else { continue };
                        let picked = open.selected == Some(id);
                        let mut row = col.spawn(frame(BenchButton { act: Do::Pick(id), enabled: true, scrolls: true }));
                        if picked {
                            row.insert(BorderColor::all(ACCENT));
                        }
                        row.with_children(|row| {
                            row.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|text| {
                                line(text, &fonts, item.title().to_string(), &fonts.body_medium, 20.0, rarity_color(item.rarity));
                                if open.mode == Service::Scrap {
                                    if let Some(s) = scrap(item) {
                                        line(text, &fonts, format!("Gives {} essence", s.essence), &fonts.body, 15.0, MUTED);
                                    }
                                } else if !item.affixes.is_empty() {
                                    let names: Vec<&str> = item.affixes.iter().map(|a| a.name.as_str()).collect();
                                    line(text, &fonts, names.join(" \u{b7} "), &fonts.body, 15.0, if item.affixes.iter().any(|a| !a.good) { BAD } else { MUTED });
                                }
                            });
                        });
                    }
                });
                // The right: the picked item and the work on it, or what was scrapped.
                body.spawn((ScrollColumn, ScrollPosition::default(), Node { flex_grow: 1.0, min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(7), overflow: Overflow::scroll_y(), ..default() })).with_children(|col| {
                    let picked = open.selected.and_then(|id| inventory.item(id).map(|i| (id, i)));
                    match picked {
                        Some((id, item)) => detail(col, &fonts, open, &inventory, &stock, purse, id, item),
                        None => {
                            if let Some(done) = &open.scrapped {
                                line(col, &fonts, "Scrapped", &fonts.mono, 13.0, MUTED);
                                line(col, &fonts, done.title.clone(), &fonts.display, 30.0, INK);
                                line(col, &fonts, format!("It gave {}.", done.gave), &fonts.body, 20.0, OK);
                            } else {
                                line(col, &fonts, if open.mode == Service::Scrap { "Pick something to take apart." } else { "Pick something to enchant." }, &fonts.body_italic, 20.0, MUTED);
                                if open.mode == Service::Enchant {
                                    line(col, &fonts, tier_note(by.tier), &fonts.body, 17.0, MUTED);
                                }
                            }
                        }
                    }
                });
            });
            panel.spawn((Node { height: px(1), flex_shrink: 0.0, ..default() }, BackgroundColor(FAINT)));
            // The foot: how the last thing went, and the way back.
            panel.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, column_gap: px(24), flex_shrink: 0.0, ..default() }).with_children(|foot| {
                match &open.notice {
                    Some((text, ok)) => line(foot, &fonts, text.clone(), &fonts.body_medium, 19.0, if *ok { OK } else { BAD }),
                    None => line(foot, &fonts, "Click to choose \u{b7} wheel scrolls \u{b7} Esc leaves", &fonts.mono, 13.0, MUTED),
                }
                foot.spawn(frame(BenchButton { act: Do::Back, enabled: true, scrolls: false })).with_children(|b| {
                    line(b, &fonts, "Back to the talk", &fonts.body, 19.0, INK);
                });
            });
        });
    });
}

/// What this tier of enchanter will and will not do, in a line.
fn tier_note(tier: enchant::Tier) -> String {
    match tier {
        enchant::Tier::Novice => "A novice enchanter only strips flaws.".into(),
        enchant::Tier::Apprentice => "An apprentice strips any affix and adds magic-tier ones, making magic items.".into(),
        enchant::Tier::Expert => "An expert adds affixes up to rare-tier, making rare items.".into(),
        enchant::Tier::Master => "A master adds affixes of any tier, even legendary ones, which ask for a rare material, and can make a rare item a legend.".into(),
    }
}

/// Why the bench has no Strip to offer for `item` at `by`, if it has none.
fn strip_note(item: &Item, by: &Enchanter) -> Option<&'static str> {
    if !enchant::removable(item, by.tier).is_empty() {
        return None;
    }
    Some(match enchant::unchangeable(item) {
        Some(refusal) => refusal.reason(),
        None if item.affixes.is_empty() => "It carries no affix.",
        None => Refusal::TooHigh.reason(),
    })
}

/// Why the bench will not reforge `item` for `by`, if it will not.
fn reforge_note(item: &Item, by: &Enchanter) -> Option<&'static str> {
    enchant::reforge(item, enchanting::reforge_seed(item), by).err().map(Refusal::reason)
}

/// Why the bench will not make `item` legendary for `by`, if it will not.
fn legend_note(item: &Item, by: &Enchanter) -> Option<&'static str> {
    enchant::make_legendary(item, None, by, 0).err().map(Refusal::reason)
}

/// Why the bench adds nothing to `item`, when it is the item that will not take it.
fn add_note(item: &Item) -> Option<&'static str> {
    enchant::unchangeable(item).map(Refusal::reason)
}

/// The right column for a picked item.
#[allow(clippy::too_many_arguments)]
fn detail(col: &mut ChildSpawnerCommands, fonts: &UiFonts, open: &Open, inventory: &Inventory, stock: &Enchanting, purse: Coins, id: ItemId, item: &Item) {
    let card = card_text(item, Some((inventory, id)));
    line(col, fonts, card.title.clone(), &fonts.display, 32.0, rarity_color(card.rarity));
    line(col, fonts, card.meta.clone(), &fonts.body_italic, 16.0, MUTED);
    for a in &card.affixes {
        let text = format!("{}  \u{b7}  {}", a.name, a.text);
        line(col, fonts, text, &fonts.body, 18.0, if a.good { OK } else { BAD });
    }
    for h in card.history.iter().rev().take(3).collect::<Vec<_>>().into_iter().rev() {
        line(col, fonts, h.clone(), &fonts.body_italic, 15.0, MUTED);
    }
    col.spawn((Node { height: px(1), margin: UiRect::vertical(px(6)), flex_shrink: 0.0, ..default() }, BackgroundColor(FAINT)));
    if open.mode == Service::Scrap {
        let Some(salvage) = scrap(item) else { return };
        let mut gives = vec![format!("{} essence", salvage.essence)];
        gives.extend(salvage.materials.iter().map(|m| fb_items::scrap::material_name(&m.id).to_string()));
        line(col, fonts, "Taking it apart gives", &fonts.mono, 13.0, MUTED);
        line(col, fonts, gives.join(" \u{b7} "), &fonts.body, 20.0, OK);
        col.spawn(frame(BenchButton { act: Do::Scrap, enabled: true, scrolls: true })).with_children(|b| {
            line(b, fonts, "Scrap it", &fonts.body_medium, 21.0, BAD);
            line(b, fonts, "it cannot be undone", &fonts.body_italic, 15.0, MUTED);
        });
        return;
    }
    let by = &open.by;
    let priced = |work: Work| -> (Option<String>, bool) {
        match quote(item, work, by) {
            Ok(cost) => (Some(cost_line(&cost)), shortfall(&stock.0, purse, &cost).is_none()),
            Err(_) => (None, false),
        }
    };
    // Strip.
    let removable = enchant::removable(item, by.tier);
    line(col, fonts, "Strip", &fonts.mono, 13.0, MUTED);
    if removable.is_empty() {
        line(col, fonts, strip_note(item, by).unwrap_or_default(), &fonts.body_italic, 17.0, MUTED);
    }
    for affix in removable {
        let (cost, affordable) = priced(Work::Strip(affix));
        col.spawn(frame(BenchButton { act: Do::Strip(affix), enabled: cost.is_some(), scrolls: true })).with_children(|b| {
            b.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|t| {
                line(t, fonts, format!("Strip \u{201c}{}\u{201d}", affix.name), &fonts.body_medium, 19.0, if affix.good { INK } else { BAD });
                line(t, fonts, affix.text.clone(), &fonts.body, 15.0, MUTED);
            });
            line(b, fonts, cost.unwrap_or_default(), &fonts.mono, 14.0, if affordable { INK } else { BAD });
        });
    }
    // Reforge.
    line(col, fonts, "Reforge", &fonts.mono, 13.0, MUTED);
    let (cost, affordable) = priced(Work::Reforge);
    match cost {
        Some(cost) => {
            col.spawn(frame(BenchButton { act: Do::Reforge, enabled: true, scrolls: true })).with_children(|b| {
                b.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|t| {
                    line(t, fonts, "Reforge it", &fonts.body_medium, 19.0, INK);
                    line(t, fonts, "Rolls every affix again at its rarity. Cheaper than choosing; a flaw is possible.", &fonts.body, 15.0, MUTED);
                });
                line(b, fonts, cost, &fonts.mono, 14.0, if affordable { INK } else { BAD });
            });
        }
        None => {
            line(col, fonts, reforge_note(item, by).unwrap_or(Refusal::TooHigh.reason()), &fonts.body_italic, 17.0, MUTED);
        }
    }
    // Legend.
    line(col, fonts, "Legend", &fonts.mono, 13.0, MUTED);
    if let Some(why) = legend_note(item, by) {
        line(col, fonts, why, &fonts.body_italic, 17.0, MUTED);
    } else {
        let options = enchant::legendary_options(item, by.tier);
        let choices: Vec<Option<&'static Affix>> = if options.is_empty() { vec![None] } else { options.into_iter().map(Some).collect() };
        for affix in choices {
            let (cost, affordable) = priced(Work::Legend(affix));
            col.spawn(frame(BenchButton { act: Do::Legend(affix), enabled: cost.is_some(), scrolls: true })).with_children(|b| {
                b.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|t| {
                    match affix {
                        Some(a) => {
                            line(t, fonts, format!("Make legendary with \u{201c}{}\u{201d}", a.name), &fonts.body_medium, 19.0, rarity_color(fb_items::Rarity::Legendary));
                            line(t, fonts, a.text.clone(), &fonts.body, 15.0, MUTED);
                        }
                        None => {
                            line(t, fonts, "Make legendary", &fonts.body_medium, 19.0, rarity_color(fb_items::Rarity::Legendary));
                            line(t, fonts, "Gives it a title and a legend. It has no room for another affix.", &fonts.body, 15.0, MUTED);
                        }
                    }
                });
                line(b, fonts, cost.unwrap_or_default(), &fonts.mono, 14.0, if affordable { INK } else { BAD });
            });
        }
    }
    // Add.
    line(col, fonts, "Add", &fonts.mono, 13.0, MUTED);
    line(col, fonts, add_note(item).map_or_else(|| tier_note(by.tier), String::from), &fonts.body_italic, 15.0, MUTED);
    for affix in enchant::addable(item, by.tier) {
        let (cost, affordable) = priced(Work::Add(affix));
        col.spawn(frame(BenchButton { act: Do::Add(affix), enabled: cost.is_some(), scrolls: true })).with_children(|b| {
            b.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|t| {
                line(t, fonts, format!("Add \u{201c}{}\u{201d}", affix.name), &fonts.body_medium, 19.0, rarity_color(affix.least));
                line(t, fonts, affix.text.clone(), &fonts.body, 15.0, MUTED);
            });
            line(b, fonts, cost.unwrap_or_default(), &fonts.mono, 14.0, if affordable { INK } else { BAD });
        });
    }
}

/// For captures: with the bench open, picks an item and does what `FB_ENCHANT` says.
fn drive_for_capture(mut bench: ResMut<Bench>, mut inventory: ResMut<Inventory>, mut stock: ResMut<Enchanting>, mut frames: Local<u32>) {
    let Some(open) = bench.open.as_ref() else { return };
    *frames += 1;
    let what = std::env::var("FB_ENCHANT").unwrap_or_default();
    let mode = open.mode;
    let pick = |want: &dyn Fn(&Item) -> bool| candidates(&inventory, mode).into_iter().find(|&id| inventory.item(id).is_some_and(want));
    match *frames {
        3 => {
            let id = match what.as_str() {
                "reforge" => pick(&|i| i.base == "Dagger"),
                "legendary" | "offer" => pick(&|i| i.base == "Cloak"),
                "scrap" => pick(&|i| i.base == "Sword"),
                _ => pick(&|i| i.affixes.iter().any(|a| !a.good)),
            }
            .or_else(|| pick(&|_| true));
            if let Some(id) = id {
                act(&mut bench, &mut inventory, &mut stock, Do::Pick(id));
            }
        }
        8 => {
            let Some(item) = bench.open.as_ref().and_then(|o| o.selected).and_then(|id| inventory.item(id)).cloned() else { return };
            let open_tier = bench.open.as_ref().map_or(enchant::Tier::Novice, |o| o.by.tier);
            let todo = match what.as_str() {
                "strip" => item.affixes.iter().find(|a| !a.good).map(|a| Do::Strip(a)),
                "reforge" => Some(Do::Reforge),
                "legendary" => Some(Do::Legend(enchant::legendary_options(&item, open_tier).first().copied())),
                "scrap" => Some(Do::Scrap),
                _ => None,
            };
            if let Some(todo) = todo {
                act(&mut bench, &mut inventory, &mut stock, todo);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_items::enchant::Tier;
    use fb_items::{Odds, roll_item_with};

    fn by(tier: Tier) -> Enchanter {
        Enchanter { name: "Ysolde".into(), tier, faction: "the Ash Rite".into() }
    }

    fn bench_with(inventory: &mut Inventory, item: Item, tier: Tier, mode: Service) -> (Bench, ItemId) {
        let id = inventory.add(item);
        let open = Open { mode, by: by(tier), selected: Some(id), notice: None, scrapped: None, level: 1 };
        (Bench { open: Some(open), revision: 0 }, id)
    }

    #[test]
    fn the_bench_lists_gear_and_never_a_purse() {
        let mut inventory = Inventory::default();
        let sword = inventory.add(roll_item_with("Sword", 5, Odds::default()).unwrap());
        let purse = inventory.add(fb_items::roll_item("Purse", 4).unwrap());
        inventory.equip(sword, fb_items::Slot::Mainhand).unwrap();
        let list = candidates(&inventory, Service::Enchant);
        assert_eq!(list, vec![sword]);
        assert!(!list.contains(&purse));
    }

    #[test]
    fn picking_and_scrapping_through_the_bench() {
        let mut inventory = Inventory::default();
        let mut stock = Enchanting::default();
        let sword = roll_item_with("Sword", 5, Odds::loot(6)).unwrap();
        let (mut bench, id) = bench_with(&mut inventory, sword, Tier::Novice, Service::Scrap);
        inventory.equip(id, fb_items::Slot::Mainhand).unwrap();
        assert!(!act(&mut bench, &mut inventory, &mut stock, Do::Scrap));
        assert!(inventory.item(id).is_none() && stock.0.essence > 0);
        let open = bench.open.as_ref().unwrap();
        assert!(open.scrapped.is_some() && open.selected.is_none());
        assert!(act(&mut bench, &mut inventory, &mut stock, Do::Back));
    }

    #[test]
    fn a_legendary_item_gives_its_real_reason_everywhere() {
        let cloak = (0..20_000).filter_map(|s| roll_item_with("Cloak", s, Odds::loot(8))).find(|i| i.rarity == fb_items::Rarity::Rare).unwrap();
        let legend = enchant::make_legendary(&cloak, None, &by(Tier::Master), 3).unwrap();
        for tier in [Tier::Expert, Tier::Master] {
            let by = by(tier);
            let want = "A legend cannot be changed.";
            assert_eq!(strip_note(&legend, &by), Some(want));
            assert_eq!(reforge_note(&legend, &by), Some(want));
            assert_eq!(add_note(&legend), Some(want));
        }
        assert_eq!(legend_note(&legend, &by(Tier::Master)), Some("A legend cannot be changed."));
        assert_eq!(legend_note(&legend, &by(Tier::Expert)), Some("Only a master enchanter can make a legend."));
        // A rare item an expert is asked about, and a common one a master is.
        assert_eq!(legend_note(&cloak, &by(Tier::Expert)), Some("Only a master enchanter can make a legend."));
        assert_eq!(legend_note(&cloak, &by(Tier::Master)), None);
        assert_eq!(strip_note(&cloak, &by(Tier::Novice)), Some("Beyond this enchanter's craft."));
    }
}
