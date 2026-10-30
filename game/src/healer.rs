//! Healers: the clerics a faction keeps who tend the delver for coin ([`fb_faction::Profession::Healer`]).
//!
//! A healer's Heal topic ([`Service::Heal`]) is listed by [`crate::talk::lines::topics`] for a
//! healer and by [`crate::talk::lines::parley_topics`] for one who will not talk, and
//! [`crate::talk`] asks [`refusal`] first whenever it is chosen. A willing healer's choice opens
//! the Heal screen ([`Screen::Heal`]) over the level, the one place they tend the delver:
//!
//! - **Hit points.** Restoring what is missing, at [`fb_character::healer::HealerRules::per_hit_point`]
//!   copper for each, so the price grows with the hurt.
//! - **Poison.** Curing the `poisoned` effect, the same removal as the Antidote's
//!   ([`crate::eat::CURED_BY_ANTIDOTE`]). There is no disease in the simulation yet, so there is
//!   nothing to cure of that sort.
//! - **Wounds.** Each permanent wound a fall left ([`fb_character::revival::WOUNDS`]) is listed
//!   with its price. Healing one removes its effect and gives back one revive: the count of
//!   times the delver has come to ([`Revives`]) goes down by one, so the revives left rise by one,
//!   never above the world's starting count (`[revival] revives`). With the revives already full
//!   the wound is still removed and the revives stay as they are.
//!
//! Each line shows its price, set by how the healer's faction regards the delver
//! ([`regard_of`]: Friendly cheaper than Indifferent, Allied cheapest), and is paid from the
//! delver's coin purse and the purses they carry ([`crate::enchanting::pay`]), as the enchanter's
//! work is. Too little coin is refused with a line. With nothing to tend the healer says so and
//! the screen does not open. The numbers are `[healer]` in `data/rules.toml`.
//!
//! The screen is laid out for the 1920 by 1080 reference and scales with the window
//! ([`crate::ui`]); the mouse is read in the interface's own pixels ([`ui_cursor`]). Back
//! returns to the talk; Esc leaves, and the folk's hold is released however it closes
//! ([`crate::talk`] treats the screen as the talk going on).
//!
//! Captures: `FB_SPAWN=folk:0:healer FB_TALK=folk FB_TALK_BAND=friendly FB_TALK_TOPIC=heal`
//! opens it, and `FB_HEAL_KIT=<hurt,poisoned,wounded,poor>` (comma-separated) starts the delver at
//! half their hit points, poisoned, with a lame leg and one revive spent and/or with a thin purse.

use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_character::effect::Effects;
use fb_character::healer::Regard;
use fb_character::revival::{WOUNDS, coin_words};
use fb_character::rules::Rules;
use fb_faction::{Band, Profession};

use crate::carry::ViewModelCamera;
use crate::character::DelverEffects;
use crate::combat::Health;
use crate::combat::revival::Revives;
use crate::enchanting::{pay, wallet};
use crate::inventory::Inventory;
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, Player, Screen, ScreenOpen, show_screen};
use crate::rules::GameRules;
use crate::talk::lines::{self, Service, Speaker, Topic};
use crate::talk::{ServiceChosen, Talk};
use crate::ui::{ACCENT, BAD, BG, BRIGHT, CELL, FAINT, INK, LINE, MUTED, OK, UiFonts, node_rect, ui_cursor};

/// Whether `speaker` is a healer. Only clerics are ([`fb_level::FolkSpot::class`]).
pub fn is_healer(speaker: &Speaker) -> bool {
    speaker.denizen.is_none() && speaker.trade.is_some_and(|t| t.profession == Profession::Healer)
}

/// Why this healer will not tend the delver, if they will not: a Hostile or Wary faction's healer
/// refuses, and so does any healer who has turned on the delver (`parley`, the talk is only a
/// parley). `None` for a healer who will tend, and for anyone who is not a healer.
pub fn refusal(speaker: &Speaker, parley: bool) -> Option<String> {
    if !is_healer(speaker) {
        return None;
    }
    let symbol = match speaker.band {
        Band::Hostile => "heal_refuse_hostile",
        Band::Wary => "heal_refuse_wary",
        _ if parley => "heal_refuse_hostile",
        _ => return None,
    };
    Some(lines::say(speaker, symbol, 0x60 + Service::Heal as u32, &[]))
}

/// What a willing healer says of what they can tend.
pub fn offer(speaker: &Speaker) -> String {
    lines::say(speaker, "heal_offer", 0x61, &[])
}

/// The answer to the Heal topic: the refusal, else the offer.
pub fn answer(speaker: &Speaker, parley: bool) -> String {
    refusal(speaker, parley).unwrap_or_else(|| offer(speaker))
}

/// What a healer says when there is nothing to tend.
pub fn nothing_to_tend(speaker: &Speaker) -> String {
    lines::say(speaker, "heal_nothing", 0x62, &[])
}

/// How `band` regards the delver as far as the healer's price goes. Hostile and Wary healers
/// do not tend, so they read as Indifferent here and never get as far as a price.
pub fn regard_of(band: Band) -> Regard {
    match band {
        Band::Friendly => Regard::Friendly,
        Band::Allied => Regard::Allied,
        _ => Regard::Indifferent,
    }
}

/// What a healer can do for the delver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Care {
    /// Restore every hit point that is missing.
    HitPoints,
    /// Cure the poisoning.
    Poison,
    /// Heal the permanent wound with this effect id ([`WOUNDS`]), and give back a revive.
    Wound(&'static str),
}

/// One line of the healer's list: what they can tend and what it costs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tending {
    pub care: Care,
    pub label: String,
    pub detail: String,
    /// Copper.
    pub price: u32,
}

/// What a healer can tend in a delver of `health` with `effects`, who has come to `revives`
/// times in this world, with prices at `regard`. Empty when there is nothing to tend.
pub fn menu(rules: &Rules, regard: Regard, health: Health, effects: &Effects, revives: u32) -> Vec<Tending> {
    let prices = &rules.healer;
    let mut list = vec![];
    let missing = health.max - health.hp;
    if missing > 0 {
        list.push(Tending { care: Care::HitPoints, label: "Restore hit points".into(), detail: format!("{missing} of {} missing", health.max), price: prices.hit_points(missing, regard) });
    }
    if effects.get(crate::eat::CURED_BY_ANTIDOTE, None).is_some() {
        list.push(Tending { care: Care::Poison, label: "Cure the poison".into(), detail: "The venom in your blood".into(), price: prices.poison(regard) });
    }
    for wound in WOUNDS.iter().filter(|w| effects.get(w.effect, Some(w.part)).is_some()) {
        let name = rules.effects.get(wound.effect).map_or(wound.effect, |def| def.name.as_str());
        let gives = if rules.revival.give_back(revives) < revives { "Gives back a revive" } else { "Your revives are already full" };
        list.push(Tending { care: Care::Wound(wound.effect), label: format!("Heal {}", name.to_lowercase()), detail: gives.into(), price: prices.wound(regard) });
    }
    list
}

/// Has the healer tend `care`, taking the price from the delver's purses. Healing a wound
/// removes it and gives back a revive: `revives` is how many times the delver has come to, and
/// goes down by one ([`fb_character::revival::RevivalRules::give_back`]). The line to tell them
/// on success; on failure the line saying why, and nothing changed.
pub fn tend(care: Care, rules: &Rules, regard: Regard, health: &mut Health, effects: &mut Effects, inventory: &mut Inventory, revives: &mut u32) -> Result<String, String> {
    let Some(line) = menu(rules, regard, *health, effects, *revives).into_iter().find(|l| l.care == care) else {
        return Err("There is nothing of that to tend.".into());
    };
    let held = wallet(inventory).total_copper();
    if held < line.price as u64 || !pay(inventory, line.price as u64) {
        return Err(format!("It costs {}, and you carry {}.", coin_words(line.price), coin_words(held.min(u32::MAX as u64) as u32)));
    }
    let paid = coin_words(line.price);
    Ok(match care {
        Care::HitPoints => {
            health.hp = health.max;
            format!("Your wounds close. Paid {paid}.")
        }
        Care::Poison => {
            effects.remove(crate::eat::CURED_BY_ANTIDOTE, None);
            format!("The venom is gone. Paid {paid}.")
        }
        Care::Wound(id) => {
            if let Some(wound) = WOUNDS.iter().find(|w| w.effect == id) {
                effects.remove(wound.effect, Some(wound.part));
            }
            let gave_back = rules.revival.give_back(*revives) < *revives;
            *revives = rules.revival.give_back(*revives);
            let left = rules.revival.left(*revives);
            let name = rules.effects.get(id).map_or(id, |def| def.name.as_str()).to_lowercase();
            let times = if left == 1 { "once more".to_string() } else { format!("{left} more times") };
            if gave_back {
                format!("Your {name} is mended, and the abyss will let you come to {times}. Paid {paid}.")
            } else {
                format!("Your {name} is mended. Paid {paid}.")
            }
        }
    })
}

pub struct HealerPlugin;

impl Plugin for HealerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Healing>();
        app.add_systems(OnExit(crate::launch::AppState::Delving), |mut healing: ResMut<Healing>| *healing = Healing::default());
        app.add_systems(Update, (open_heal, click_heal, close_heal, build_heal, hover_heal).chain().in_set(DelveSystems));
        if std::env::var("FB_HEAL_KIT").is_ok_and(|v| !v.is_empty()) {
            app.add_systems(Update, heal_kit_for_capture.in_set(DelveSystems));
        }
    }
}

/// The healing in progress, if any.
#[derive(Resource, Default, Debug)]
pub struct Healing {
    open: Option<Open>,
    revision: u32,
}

impl Healing {
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }
}

#[derive(Debug)]
struct Open {
    name: String,
    faction: String,
    regard: Regard,
    /// What the healer said on seeing the delver.
    said: String,
    /// What the healer says when there is nothing left to tend.
    nothing: String,
    /// What the last thing done said, and whether it went well.
    notice: Option<(String, bool)>,
}

/// A button on the screen.
#[derive(Component, Clone, Copy, Debug)]
struct HealButton {
    act: Act,
}

#[derive(Clone, Copy, Debug)]
enum Act {
    Care(Care),
    Back,
}

/// The screen's root.
#[derive(Component, Debug)]
struct HealScreen;

/// The width of the panel at the reference size, logical pixels.
const WIDTH: f32 = 760.0;

/// The Heal topic of a willing healer opens the screen; with nothing to tend they say so instead.
pub(crate) fn open_heal(
    mut chosen: MessageReader<ServiceChosen>,
    mut talk: ResMut<Talk>,
    mut healing: ResMut<Healing>,
    rules: Option<Res<GameRules>>,
    revives: Option<Res<Revives>>,
    players: Query<(&Health, Option<&DelverEffects>), With<Player>>,
    mut open: ResMut<ScreenOpen>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
) {
    for ServiceChosen { service, .. } in chosen.read() {
        if *service != Service::Heal {
            continue;
        }
        let Some(speaker) = talk.speaker.as_ref() else { continue };
        let regard = regard_of(speaker.band);
        let rules = GameRules::of(&rules);
        let Ok((health, effects)) = players.single() else { continue };
        let none = Effects::default();
        let list = menu(rules, regard, *health, effects.map_or(&none, |e| &e.0), revives.as_ref().map_or(0, |r| r.0));
        if list.is_empty() {
            let said = nothing_to_tend(speaker);
            talk.answer = Some((Topic::Service(Service::Heal), said));
            talk.revision += 1;
            continue;
        }
        healing.open = Some(Open { name: speaker.name.clone(), faction: speaker.faction.clone(), regard, said: offer(speaker), nothing: nothing_to_tend(speaker), notice: None });
        healing.revision += 1;
        show_screen(&mut open, Some(Screen::Heal), cursor.as_deref_mut().map(|c| &mut **c));
    }
}

/// Forgets the healing when the screen closes.
fn close_heal(open: Res<ScreenOpen>, mut healing: ResMut<Healing>) {
    if healing.open.is_some() && !open.is(Screen::Heal) {
        healing.open = None;
        healing.revision += 1;
    }
}

/// A click on a button.
#[allow(clippy::too_many_arguments)]
fn click_heal(
    input: ActionInput,
    mut healing: ResMut<Healing>,
    rules: Option<Res<GameRules>>,
    mut revives: ResMut<Revives>,
    mut players: Query<(Entity, &mut Health, Option<&mut DelverEffects>), With<Player>>,
    mut inventory: ResMut<Inventory>,
    mut screen: ResMut<ScreenOpen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    buttons: Query<(&HealButton, &ComputedNode, &UiGlobalTransform)>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
    mut commands: Commands,
) {
    if !screen.is(Screen::Heal) || !input.just_pressed(Action::Choose) {
        return;
    }
    let Some(point) = windows.iter().next().and_then(|w| ui_cursor(w, ui_scale.as_deref())) else { return };
    let Some((button, ..)) = buttons.iter().find(|(_, node, at)| node_rect(node, at).contains(point)) else { return };
    let care = match button.act {
        Act::Back => {
            show_screen(&mut screen, Some(Screen::Talk), cursor.as_deref_mut().map(|c| &mut **c));
            return;
        }
        Act::Care(care) => care,
    };
    let Some(open) = healing.open.as_mut() else { return };
    let Ok((player, mut health, effects)) = players.single_mut() else { return };
    let rules = GameRules::of(&rules);
    let mut local = effects.as_ref().map(|e| e.0.clone()).unwrap_or_default();
    open.notice = Some(match tend(care, rules, open.regard, &mut health, &mut local, &mut inventory, &mut revives.0) {
        Ok(line) => {
            if let Some(mut effects) = effects {
                effects.0 = local.clone();
                // The component goes with the last effect, as the clock takes it off.
                if local.active.is_empty() {
                    commands.entity(player).remove::<DelverEffects>();
                }
            }
            (line, true)
        }
        Err(line) => (line, false),
    });
    healing.revision += 1;
}

/// Lights the button under the mouse.
fn hover_heal(
    open: Res<ScreenOpen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    mut buttons: Query<(&HealButton, &ComputedNode, &UiGlobalTransform, &mut BackgroundColor, &mut BorderColor)>,
) {
    if !open.is(Screen::Heal) {
        return;
    }
    let point = windows.iter().next().and_then(|w| ui_cursor(w, ui_scale.as_deref()));
    for (_, node, at, mut fill, mut border) in &mut buttons {
        let over = point.is_some_and(|p| node_rect(node, at).contains(p));
        let (back, line) = if over { (FAINT, LINE) } else { (CELL, ACCENT.with_alpha(0.55)) };
        fill.set_if_neq(BackgroundColor(back));
        border.set_if_neq(BorderColor::all(line));
    }
}

/// Draws the screen while it is open, and again whenever it changes.
#[allow(clippy::too_many_arguments)]
fn build_heal(
    mut commands: Commands,
    screen: Res<ScreenOpen>,
    healing: Res<Healing>,
    rules: Option<Res<GameRules>>,
    inventory: Res<Inventory>,
    revives: Option<Res<Revives>>,
    players: Query<(&Health, Option<&DelverEffects>), With<Player>>,
    fonts: Option<Res<UiFonts>>,
    cameras: Query<Entity, With<ViewModelCamera>>,
    roots: Query<Entity, With<HealScreen>>,
    mut drawn: Local<Option<u32>>,
) {
    let existing = roots.iter().next();
    let (true, Some(open), Some(fonts)) = (screen.is(Screen::Heal), healing.open.as_ref(), fonts) else {
        if let Some(root) = existing {
            commands.entity(root).despawn();
        }
        *drawn = None;
        return;
    };
    if existing.is_some() && *drawn == Some(healing.revision) {
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
                Name::new("Healer"),
                HealScreen,
                DespawnOnExit(crate::launch::AppState::Delving),
                UiTargetCamera(camera),
                Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                GlobalZIndex(12),
            ))
            .id(),
    };
    *drawn = Some(healing.revision);
    let rules = GameRules::of(&rules);
    let purse = wallet(&inventory);
    let none = Effects::default();
    let list = players.single().map(|(health, effects)| menu(rules, open.regard, *health, effects.map_or(&none, |e| &e.0), revives.as_ref().map_or(0, |r| r.0))).unwrap_or_default();
    let button_frame = |act| {
        (
            HealButton { act },
            Node { align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween, column_gap: px(16), padding: UiRect::axes(px(16), px(10)), border: UiRect::all(px(1.5)), border_radius: BorderRadius::all(px(8)), flex_shrink: 0.0, ..default() },
            BackgroundColor(CELL),
            BorderColor::all(ACCENT.with_alpha(0.55)),
        )
    };
    commands.entity(root).with_children(|root| {
        root.spawn((
            Name::new("Healer panel"),
            Node {
                width: px(WIDTH),
                max_width: percent(94),
                max_height: percent(94),
                flex_direction: FlexDirection::Column,
                row_gap: px(12),
                padding: UiRect::axes(px(30), px(24)),
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
            panel.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::FlexEnd, column_gap: px(24), ..default() }).with_children(|head| {
                head.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|who| {
                    who.spawn((Text::new("Heal"), UiFonts::style(&fonts.display, 34.0, INK)));
                    who.spawn((Text::new(format!("{}, healer of {}", open.name, open.faction)), UiFonts::style(&fonts.body_italic, 18.0, MUTED)));
                });
                head.spawn((Text::new(if purse.is_empty() { "no coin".to_string() } else { purse.short() }), UiFonts::style(&fonts.mono, 15.0, ACCENT)));
            });
            panel.spawn((Node { height: px(1), flex_shrink: 0.0, ..default() }, BackgroundColor(FAINT)));
            let said = if list.is_empty() { &open.nothing } else { &open.said };
            panel.spawn((Text::new(format!("\u{201c}{said}\u{201d}")), UiFonts::style(&fonts.body_italic, 19.0, MUTED)));
            for line in &list {
                let affordable = purse.total_copper() >= line.price as u64;
                panel.spawn(button_frame(Act::Care(line.care))).with_children(|button| {
                    button.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|text| {
                        text.spawn((Text::new(line.label.clone()), UiFonts::style(&fonts.display, 24.0, BRIGHT)));
                        text.spawn((Text::new(line.detail.clone()), UiFonts::style(&fonts.body, 16.0, MUTED)));
                    });
                    button.spawn((Text::new(coin_words(line.price)), UiFonts::style(&fonts.mono, 18.0, if affordable { ACCENT } else { BAD })));
                });
            }
            if let Some((text, good)) = &open.notice {
                panel.spawn((Text::new(text.clone()), UiFonts::style(&fonts.body_medium, 19.0, if *good { OK } else { BAD })));
            }
            panel.spawn(button_frame(Act::Back)).with_children(|button| {
                button.spawn((Text::new("Back"), UiFonts::style(&fonts.display, 22.0, BRIGHT)));
            });
        });
    });
}

/// For captures: `FB_HEAL_KIT=<hurt,poisoned,wounded,poor>` puts the delver at half their hit
/// points, poisoned, with a lame leg and one revive spent, and/or with a thin purse, once, a few
/// frames in.
#[allow(clippy::too_many_arguments)]
fn heal_kit_for_capture(mut players: Query<(Entity, &mut Health, Option<&DelverEffects>), With<Player>>, mut inventory: ResMut<Inventory>, mut revives: ResMut<Revives>, rules: Option<Res<GameRules>>, mut commands: Commands, mut frames: Local<u32>) {
    *frames += 1;
    if *frames != 6 {
        return;
    }
    let kit = std::env::var("FB_HEAL_KIT").unwrap_or_default();
    let has = |word: &str| kit.split(',').any(|w| w.trim() == word);
    let Ok((player, mut health, effects)) = players.single_mut() else { return };
    let rules = GameRules::of(&rules);
    if has("hurt") {
        health.hp = (health.max / 2).max(1);
    }
    let mut effects = effects.map(|e| e.0.clone()).unwrap_or_default();
    if has("poisoned") {
        effects = DelverEffects::poisoned_for(rules, 6.0).0;
    }
    if has("wounded") {
        let wound = WOUNDS[0];
        effects.add(&rules.effects, wound.effect, 1.0, Some(wound.part), fb_character::effect::Source::Fall);
        revives.0 = 1;
    }
    if !effects.active.is_empty() {
        commands.entity(player).insert(DelverEffects(effects));
    }
    inventory.set_coins(if has("poor") { fb_items::Coins { gp: 0, sp: 0, cp: 3 } } else { fb_items::Coins { gp: 5, sp: 0, cp: 0 } });
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_character::effect::Source;
    use fb_items::{Coins, Slot};

    fn purse(copper: u64) -> Inventory {
        let mut inventory = Inventory::default();
        inventory.set_coins(Coins::from_copper(copper));
        inventory
    }

    fn poisoned() -> Effects {
        DelverEffects::poisoned_for(rules(), 6.0).0
    }

    fn lame() -> Effects {
        let mut effects = Effects::default();
        effects.add(&rules().effects, "lame_leg", 1.0, Some(Slot::Legs), Source::Fall);
        effects
    }

    fn rules() -> &'static Rules {
        fb_character::rules::defaults()
    }

    /// The price of `care` for a delver as given, at `regard`.
    fn price_of(care: Care, regard: Regard, health: Health, effects: &Effects, revives: u32) -> u32 {
        menu(rules(), regard, health, effects, revives).into_iter().find(|l| l.care == care).expect("offered").price
    }

    #[test]
    fn half_hit_points_and_poisoned_pay_and_come_away_whole() {
        let mut health = Health { hp: 6, max: 12 };
        let mut effects = poisoned();
        let mut inventory = purse(500);
        let both = menu(rules(), Regard::Indifferent, health, &effects, 0);
        assert_eq!(both.iter().map(|l| l.care).collect::<Vec<_>>(), [Care::HitPoints, Care::Poison]);
        let cost = both.iter().map(|l| l.price as u64).sum::<u64>();
        for line in &both {
            assert!(tend(line.care, rules(), Regard::Indifferent, &mut health, &mut effects, &mut inventory, &mut 0).is_ok());
        }
        assert_eq!(health.hp, 12);
        assert!(effects.get("poisoned", None).is_none());
        assert_eq!(wallet(&inventory).total_copper(), 500 - cost);
        assert!(menu(rules(), Regard::Indifferent, health, &effects, 0).is_empty());
    }

    #[test]
    fn the_price_of_hit_points_grows_with_what_is_missing() {
        let effects = Effects::default();
        let price = |hp| price_of(Care::HitPoints, Regard::Indifferent, Health { hp, max: 12 }, &effects, 0);
        assert!(price(2) > price(8));
    }

    #[test]
    fn curing_poison_leaves_other_effects() {
        let mut effects = poisoned();
        effects.add(&rules().effects, "lame_leg", 1.0, Some(Slot::Legs), Source::Fall);
        let mut health = Health::full(12);
        assert!(tend(Care::Poison, rules(), Regard::Indifferent, &mut health, &mut effects, &mut purse(500), &mut 0).is_ok());
        assert!(effects.get("poisoned", None).is_none() && effects.get("lame_leg", Some(Slot::Legs)).is_some());
    }

    #[test]
    fn a_delver_with_nothing_wrong_has_nothing_to_tend() {
        assert!(menu(rules(), Regard::Friendly, Health::full(12), &Effects::default(), 0).is_empty());
        let mut inventory = purse(500);
        let refused = tend(Care::HitPoints, rules(), Regard::Friendly, &mut Health::full(12), &mut Effects::default(), &mut inventory, &mut 0);
        assert!(refused.is_err());
        assert_eq!(wallet(&inventory).total_copper(), 500);
    }

    #[test]
    fn too_little_coin_is_refused_and_nothing_changes() {
        let mut health = Health { hp: 6, max: 12 };
        let mut effects = poisoned();
        let mut inventory = purse(3);
        let refused = tend(Care::HitPoints, rules(), Regard::Indifferent, &mut health, &mut effects, &mut inventory, &mut 0);
        assert!(refused.as_ref().is_err_and(|line| line.contains("costs")), "{refused:?}");
        assert_eq!((health.hp, wallet(&inventory).total_copper()), (6, 3));
        assert!(effects.get("poisoned", None).is_some());
    }

    #[test]
    fn a_friendly_healer_charges_less_than_an_indifferent_one() {
        let hurt = Health { hp: 4, max: 12 };
        let effects = poisoned();
        let price = |regard| menu(rules(), regard, hurt, &effects, 0).iter().map(|l| l.price).sum::<u32>();
        assert!(price(regard_of(Band::Friendly)) < price(regard_of(Band::Neutral)));
        assert!(price(regard_of(Band::Allied)) < price(regard_of(Band::Friendly)));
    }

    #[test]
    fn the_price_comes_out_of_change_in_the_purse() {
        let mut inventory = Inventory::default();
        inventory.set_coins(Coins { gp: 1, sp: 0, cp: 0 });
        let mut health = Health { hp: 11, max: 12 };
        assert!(tend(Care::HitPoints, rules(), Regard::Indifferent, &mut health, &mut Effects::default(), &mut inventory, &mut 0).is_ok());
        assert_eq!(wallet(&inventory).total_copper(), 95);
    }

    #[test]
    fn a_lame_leg_with_one_revive_left_heals_for_a_second_revive() {
        // Two revives in the world, one used: one left.
        let mut effects = lame();
        let mut used = 1;
        let mut inventory = purse(1000);
        let offered = menu(rules(), Regard::Indifferent, Health::full(12), &effects, used);
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].care, Care::Wound("lame_leg"));
        assert_eq!(offered[0].price, 500);
        assert!(offered[0].detail.contains("revive"));
        let said = tend(Care::Wound("lame_leg"), rules(), Regard::Indifferent, &mut Health::full(12), &mut effects, &mut inventory, &mut used).unwrap();
        assert!(effects.get("lame_leg", Some(Slot::Legs)).is_none());
        assert_eq!(rules().revival.left(used), 2, "{said}");
        assert_eq!(wallet(&inventory).total_copper(), 500);
        // Walks at full pace.
        assert_eq!(effects.pace(&rules().effects), 1.0);
    }

    #[test]
    fn revives_never_rise_above_the_worlds_starting_count() {
        let mut effects = lame();
        let mut used = 0;
        let mut inventory = purse(1000);
        let offered = menu(rules(), Regard::Indifferent, Health::full(12), &effects, used);
        assert!(offered[0].detail.contains("already full"), "{}", offered[0].detail);
        assert!(tend(Care::Wound("lame_leg"), rules(), Regard::Indifferent, &mut Health::full(12), &mut effects, &mut inventory, &mut used).is_ok());
        assert!(effects.get("lame_leg", Some(Slot::Legs)).is_none());
        assert_eq!((used, rules().revival.left(used)), (0, rules().revival.revives));
        assert_eq!(wallet(&inventory).total_copper(), 500, "the wound is still paid for");
    }

    #[test]
    fn a_wound_is_refused_for_want_of_coin() {
        let mut effects = lame();
        let mut used = 1;
        let mut inventory = purse(100);
        let refused = tend(Care::Wound("lame_leg"), rules(), Regard::Indifferent, &mut Health::full(12), &mut effects, &mut inventory, &mut used);
        assert!(refused.as_ref().is_err_and(|line| line.contains("costs")), "{refused:?}");
        assert!(effects.get("lame_leg", Some(Slot::Legs)).is_some());
        assert_eq!((used, wallet(&inventory).total_copper()), (1, 100));
    }

    #[test]
    fn every_wound_a_fall_leaves_can_be_healed_and_a_friend_pays_less() {
        for wound in WOUNDS {
            let mut effects = Effects::default();
            effects.add(&rules().effects, wound.effect, 1.0, Some(wound.part), Source::Fall);
            let friendly = price_of(Care::Wound(wound.effect), Regard::Friendly, Health::full(12), &effects, 1);
            let plain = price_of(Care::Wound(wound.effect), Regard::Indifferent, Health::full(12), &effects, 1);
            assert!(friendly < plain, "{}", wound.effect);
            assert!(tend(Care::Wound(wound.effect), rules(), Regard::Indifferent, &mut Health::full(12), &mut effects, &mut purse(1000), &mut 1).is_ok());
            assert!(effects.active.is_empty(), "{}", wound.effect);
        }
    }

    #[test]
    fn a_healed_delver_survives_a_save_and_load() {
        let mut health = Health { hp: 3, max: 12 };
        let mut effects = lame();
        let mut used = 2;
        tend(Care::Wound("lame_leg"), rules(), Regard::Friendly, &mut health, &mut effects, &mut purse(2000), &mut used).unwrap();
        tend(Care::HitPoints, rules(), Regard::Friendly, &mut health, &mut effects, &mut purse(2000), &mut used).unwrap();
        // What the save keeps: hit points and effects as data, the revives count as a number.
        let saved: Effects = serde_json::from_str(&serde_json::to_string(&effects).unwrap()).unwrap();
        assert_eq!((health.hp, used), (12, 1));
        assert!(saved.get("lame_leg", Some(Slot::Legs)).is_none() && saved.active.is_empty());
    }
}
