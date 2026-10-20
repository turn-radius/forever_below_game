//! The character tab: the inventory screen's third leaf, in the plate's look (the dark ledger's
//! palette, Almendra headings, Plex Mono values). It shows only what the game simulates.
//!
//! The attributes and the gear section come from `Character::derive`, so they agree with the
//! header. The conditions section lists what the delver is under, as the HUD's badges do
//! ([`crate::status`]): each condition's badge, its name in its severity's colour and what it
//! does, so the player can read them with the mouse locked or free ([`update_conditions`]).
//! The status section reads the delver's `DelverNeeds`, `Exhaustion` and load band. The screen
//! builds the sheet when the tab opens or the gear changes ([`super::build_screen`]); while it
//! is open [`update_status`] rewrites only the words and bars that moved, so nothing is rebuilt
//! when the delver eats or drinks.

use bevy::prelude::*;
use fb_character::explain::Explained;
use fb_character::rules::Rules;
use fb_character::{Exhaustion as ExhaustionLevel, Mana, Meter, MeterKind, Needs, Stamina, modifier};

use super::card::place_beside;
use super::items::Inventory;
use crate::carry::ViewModelCamera;
use crate::character::{Character, DelverMana, DelverNeeds, DelverStamina, Derived};
use crate::exhaustion::Exhaustion;
use crate::play::{Player, Screen, ScreenOpen, SheetTab};
use crate::rules::GameRules;
use crate::status::{ConditionIcons, ShownConditions, badge, severity_colour};
use crate::ui::{ACCENT, BAD, BG, BRIGHT, CELL, FAINT, INK, LINE, MUTED, OK, UiFonts, node_rect};
use bevy::window::PrimaryWindow;

/// The leaf under the header when the character tab is showing.
#[derive(Component, Debug)]
pub struct CharacterSheet;

/// The filled part of a need's bar.
#[derive(Component, Debug)]
pub struct NeedFill(pub MeterKind);

/// A need's word: Fed, Peckish, Hungry.
#[derive(Component, Debug)]
pub struct NeedWord(pub MeterKind);

/// Which of the two pools a vital bar or word shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vital {
    Stamina,
    Mana,
}

/// The filled part of the stamina or mana bar.
#[derive(Component, Debug)]
pub struct VitalFill(pub Vital);

/// The stamina or mana word: Fresh, Winded; 5 / 10.
#[derive(Component, Debug)]
pub struct VitalWord(pub Vital);

/// The delver's stamina and mana, as the sheet shows them.
#[derive(Clone, Copy, Debug, Default)]
pub struct Vitals {
    pub stamina: Stamina,
    pub mana: Mana,
}

/// How full a vital's bar is, 0 to 1: a full bar is the best.
pub fn vital_fill(vitals: &Vitals, vital: Vital) -> f32 {
    match vital {
        Vital::Stamina => vitals.stamina.value.clamp(0.0, 1.0) as f32,
        Vital::Mana => vitals.mana.fraction() as f32,
    }
}

/// A vital's word: stamina Fresh, Tiring, Spent or Winded; mana "5 / 10".
pub fn vital_word(vitals: &Vitals, vital: Vital) -> String {
    match vital {
        Vital::Stamina if vitals.stamina.winded => "Winded".to_string(),
        Vital::Stamina if vitals.stamina.value > 0.66 => "Fresh".to_string(),
        Vital::Stamina if vitals.stamina.value > 0.33 => "Tiring".to_string(),
        Vital::Stamina => "Spent".to_string(),
        Vital::Mana => format!("{:.0} / {:.0}", vitals.mana.points, vitals.mana.max),
    }
}

/// The colour of a vital: stamina calm, amber, red as it empties; mana always the bright ink.
pub fn vital_colour(vitals: &Vitals, vital: Vital) -> Color {
    match vital {
        Vital::Stamina if vitals.stamina.winded || vitals.stamina.value <= 0.33 => BAD,
        Vital::Stamina if vitals.stamina.value <= 0.66 => ACCENT,
        Vital::Stamina => OK,
        Vital::Mana => BRIGHT,
    }
}

/// The list of conditions on the sheet, filled by [`update_conditions`].
#[derive(Component, Debug)]
pub struct ConditionList;

/// What the conditions section says when the delver is under none.
pub const NO_CONDITIONS: &str = "No conditions";

/// A condition's badge on the sheet, in logical pixels across.
const CONDITION_BADGE_PX: f32 = 24.0;

/// The exhaustion line: "Exhaustion 2: half speed".
#[derive(Component, Debug)]
pub struct ExhaustionLine;

/// One of the five attribute lines, by its place in [`ATTRIBUTES`].
#[derive(Component, Debug)]
pub struct AttributeLine(pub usize);

/// One of what-gear-makes-of-them lines, by its place in [`GEAR_ROWS`].
#[derive(Component, Debug)]
pub struct GearLine(pub usize);

/// Width of a label column and of a bar, in logical pixels.
const LABEL_PX: f32 = 120.0;
const BAR_PX: f32 = 180.0;
const BAR_HEIGHT_PX: f32 = 10.0;

/// The meters in the order they are shown, with their labels.
pub const NEEDS: [(MeterKind, &str); 3] = [(MeterKind::Hunger, "Hunger"), (MeterKind::Thirst, "Thirst"), (MeterKind::Sleepiness, "Sleepiness")];

/// A gear row's whole line, by its place in [`GEAR_ROWS`]: what hovering a number is measured
/// against.
#[derive(Component, Debug)]
pub struct GearRow(pub usize);

/// The gear row under the mouse, with its rectangle on screen in logical pixels.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct HoveredNumber(pub Option<(usize, Rect)>);

/// The tooltip listing where a hovered number comes from: one on the screen.
#[derive(Component, Debug)]
pub struct BreakdownTip;

/// Width of the breakdown tooltip, in logical pixels.
const TIP_WIDTH: f32 = 380.0;

/// The attributes' short names, in the order shown.
pub const ATTRIBUTES: [&str; 5] = ["STR", "DEX", "CON", "INT", "WIS"];

/// The rows of what the delver's gear makes of them.
pub const GEAR_ROWS: [&str; 15] = ["Load", "Warmth", "Light", "Darkvision", "Armour class", "Hunger rate", "Sleepiness rate", "Heal on a kill", "Speed", "Block chance", "Luck", "Abilities", "To hit", "Ranged to hit", "Attack rate"];

/// "STR 14 (+2)": the score and its SRD modifier.
pub fn attribute_text(name: &str, score: i32) -> String {
    format!("{name} {score} ({:+})", modifier(score))
}

/// The five attribute lines.
pub fn attribute_lines(character: &Character) -> [String; 5] {
    // As they stand, with what they wear (the Abilities row says how much that is).
    let a = character.scores();
    let scores = [a.strength, a.dexterity, a.constitution, a.intelligence, a.wisdom];
    std::array::from_fn(|i| attribute_text(ATTRIBUTES[i], scores[i]))
}

/// The values of [`GEAR_ROWS`], from `derived` (so they agree with the header): "55 / 150 lb
/// \u{b7} Burdened", "4 / 6", "20 ft", "60 ft", "13", "\u{d7}0.85", "usual", "+1 HP", "\u{d7}1.2",
/// "20%", "+2", "+2 STR, -1 CON", then the to hit rows "+1" and "+2" (or "none") and the attack rate "\u{d7}1.1".
pub fn gear_values(character: &Character, derived: &Derived) -> [String; 15] {
    let feet = |f: f32| if f > 0.0 { format!("{f:.0} ft") } else { "none".to_string() };
    let points = |p: f32| if p.round() == 0.0 { "none".to_string() } else { format!("{:+}", p.round()) };
    let rate = |r: f32| if (r - 1.0).abs() < 1e-6 { "usual".to_string() } else { format!("\u{d7}{}", fb_character::explain::number(r as f64)) };
    [
        format!("{:.0} / {:.0} lb \u{b7} {}", derived.load, derived.capacity, derived.load_band.word()),
        format!("{:.0} / {:.0}", derived.warmth, character.warmth_need()),
        feet(derived.light),
        feet(derived.darkvision),
        derived.armour_class.to_string(),
        rate(derived.hunger_rate),
        rate(derived.sleepiness_rate),
        if derived.heal_on_kill > 0.0 { format!("+{} HP", fb_character::explain::number(derived.heal_on_kill as f64)) } else { "none".to_string() },
        rate(derived.speed),
        if derived.block_chance > 0.0 { format!("{}%", (derived.block_chance * 100.0).round()) } else { "none".to_string() },
        if derived.luck > 0.0 { format!("+{}", fb_character::explain::number(derived.luck as f64)) } else { "none".to_string() },
        {
            let given: Vec<String> = derived.abilities.named().into_iter().filter(|(_, by)| *by != 0).map(|(name, by)| format!("{by:+} {name}")).collect();
            if given.is_empty() { "none".to_string() } else { given.join(", ") }
        },
        points(derived.to_hit),
        points(derived.ranged_to_hit),
        rate(derived.attack_rate),
    ]
}

fn meter(needs: &Needs, kind: MeterKind) -> &Meter {
    match kind {
        MeterKind::Hunger => &needs.hunger,
        MeterKind::Thirst => &needs.thirst,
        MeterKind::Sleepiness => &needs.sleepiness,
    }
}

/// How full a need's bar is, 0 to 1: the meter's value, so a full bar is the worst.
pub fn fill(meter: &Meter) -> f32 {
    meter.value.clamp(0.0, 1.0) as f32
}

/// The colour of a need's word and bar: calm through the first band, amber in the middle,
/// red when it is bad.
pub fn need_colour(meter: &Meter) -> Color {
    match meter.band() {
        0 | 1 => OK,
        2 => ACCENT,
        _ => BAD,
    }
}

/// What an exhaustion level does under `rules`, in the words the game plays by: only the
/// slowing is simulated. Half speed is said so; any other slowing gives its multiple.
pub fn exhaustion_effect(exhaustion: ExhaustionLevel, rules: &Rules) -> String {
    let speed = exhaustion.speed_multiplier_with(rules);
    if exhaustion.is_dead() {
        "death".to_string()
    } else if speed <= 0.0 {
        "cannot move".to_string()
    } else if speed == 0.5 {
        "half speed".to_string()
    } else if speed < 1.0 {
        format!("speed ×{}", fb_character::explain::number(speed as f64))
    } else {
        "no ill effect".to_string()
    }
}

/// "Exhaustion 2: half speed".
pub fn exhaustion_text(exhaustion: ExhaustionLevel, rules: &Rules) -> String {
    format!("Exhaustion {}: {}", exhaustion.level, exhaustion_effect(exhaustion, rules))
}

/// A heading over a section, in the display face with a rule beneath.
fn heading(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, title: &str) {
    parent
        .spawn((Node { border: UiRect::bottom(px(1)), padding: UiRect::bottom(px(2)), margin: UiRect::top(px(6)), ..default() }, BorderColor::all(FAINT)))
        .with_child((Text::new(title), UiFonts::style(&fonts.display, 20.0, INK)));
}

/// A row: its label in the body face, then what `fill_row` puts after it.
fn row(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, gear: Option<usize>, fill_row: impl FnOnce(&mut ChildSpawnerCommands)) {
    let mut row = parent.spawn(Node { align_items: AlignItems::Center, column_gap: px(12), min_height: px(22), ..default() });
    if let Some(i) = gear {
        row.insert(GearRow(i));
    }
    row.with_children(|row| {
        row.spawn(Node { width: px(LABEL_PX), flex_shrink: 0.0, ..default() }).with_child((Text::new(label), UiFonts::style(&fonts.body, 15.0, MUTED)));
        fill_row(row);
    });
}

/// Builds the character tab's body: the status section.
#[allow(clippy::too_many_arguments)]
pub fn sheet(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, rules: &Rules, character: &Character, inventory: &Inventory, needs: &Needs, exhaustion: ExhaustionLevel, vitals: Vitals) {
    parent.spawn((CharacterSheet, Name::new("CharacterSheet"), Node { flex_direction: FlexDirection::Column, row_gap: px(4), ..default() })).with_children(|sheet| {
        heading(sheet, fonts, "attributes");
        sheet.spawn(Node { column_gap: px(18), flex_wrap: FlexWrap::Wrap, row_gap: px(4), ..default() }).with_children(|line| {
            for (i, text) in attribute_lines(character).into_iter().enumerate() {
                line.spawn((AttributeLine(i), Text::new(text), UiFonts::style(&fonts.mono_bold, 15.0, BRIGHT)));
            }
        });
        heading(sheet, fonts, "conditions");
        sheet.spawn((ConditionList, Name::new("Conditions"), Node { flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }));
        heading(sheet, fonts, "what your gear makes of you");
        let values = gear_values(character, &character.derive_with(rules, inventory));
        for (i, label) in GEAR_ROWS.into_iter().enumerate() {
            row(sheet, fonts, label, Some(i), |row| {
                row.spawn((GearLine(i), Text::new(values[i].clone()), UiFonts::style(&fonts.mono_bold, 14.0, BRIGHT)));
            });
        }
        heading(sheet, fonts, "status");
        for (kind, label) in NEEDS {
            let meter = meter(needs, kind);
            row(sheet, fonts, label, None, |row| {
                row.spawn((
                    Node { width: px(BAR_PX), height: px(BAR_HEIGHT_PX), flex_shrink: 0.0, border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(3)), overflow: Overflow::clip(), ..default() },
                    BackgroundColor(CELL),
                    BorderColor::all(LINE),
                ))
                .with_child((NeedFill(kind), Node { width: percent(fill(meter) * 100.0), height: percent(100), ..default() }, BackgroundColor(need_colour(meter))));
                row.spawn((NeedWord(kind), Text::new(meter.word_with(rules)), UiFonts::style(&fonts.mono_bold, 14.0, need_colour(meter))));
            });
        }
        row(sheet, fonts, "Exhaustion", None, |row| {
            row.spawn((ExhaustionLine, Text::new(exhaustion_text(exhaustion, rules)), UiFonts::style(&fonts.mono_bold, 14.0, BRIGHT)));
        });
        for (vital, label) in [(Vital::Stamina, "Stamina"), (Vital::Mana, "Mana")] {
            row(sheet, fonts, label, None, |row| {
                row.spawn((
                    Node { width: px(BAR_PX), height: px(BAR_HEIGHT_PX), flex_shrink: 0.0, border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(3)), overflow: Overflow::clip(), ..default() },
                    BackgroundColor(CELL),
                    BorderColor::all(LINE),
                ))
                .with_child((VitalFill(vital), Node { width: percent(vital_fill(&vitals, vital) * 100.0), height: percent(100), ..default() }, BackgroundColor(vital_colour(&vitals, vital))));
                row.spawn((VitalWord(vital), Text::new(vital_word(&vitals, vital)), UiFonts::style(&fonts.mono_bold, 14.0, vital_colour(&vitals, vital))));
            });
        }
    });
}

/// Where each gear number comes from, one line per term set, for the row `row` of
/// [`GEAR_ROWS`]: the folk, each item and each affix that gave to it, from the same
/// [`Character::loadout`] and [`Character::armour_class`] the game plays by.
pub fn breakdown(rules: &Rules, character: &Character, inventory: &Inventory, row: usize) -> Vec<String> {
    let loadout = character.loadout(inventory);
    match row {
        0 => {
            let mut load = Explained::new("Load", "lb");
            for (slot, item) in inventory.equipped() {
                let weight = item.profile.stats.weight + inventory.worn(slot).map_or(0.0, |id| inventory.contents_weight(id));
                load = if load.terms().is_empty() { load.base(&item.name, weight as f64) } else { load.add(&item.name, weight as f64) };
            }
            vec![load.line(), loadout.capacity_with(rules, character.strength()).line()]
        }
        1 => vec![loadout.warmth().line(), format!("Warmth needed {}", character.warmth_need())],
        2 => vec![loadout.light().line()],
        3 => vec![loadout.darkvision().line()],
        4 => vec![character.armour_class_with(rules, inventory).line()],
        5 => vec![loadout.rate_with(rules, MeterKind::Hunger, fb_character::Activity::Walking).line()],
        6 => vec![loadout.rate_with(rules, MeterKind::Sleepiness, fb_character::Activity::Walking).line()],
        7 => vec![loadout.heal_on_kill().line()],
        8 => vec![loadout.speed().line()],
        9 => vec![loadout.block_chance().line()],
        10 => vec![loadout.luck().line()],
        12 => vec![loadout.to_hit().line()],
        13 => vec![loadout.ranged_to_hit().line()],
        14 => vec![loadout.attack_rate().line()],
        _ => {
            let lines: Vec<String> = ["STR", "DEX", "CON", "INT", "WIS"].into_iter().map(|a| loadout.ability(a)).filter(|e| e.value() != 0.0).map(|e| e.line()).collect();
            if lines.is_empty() { vec!["Nothing worn adds to an ability score".to_string()] } else { lines }
        }
    }
}

/// The gear row under `point`, of the rows `rects`.
pub fn row_under(rects: impl IntoIterator<Item = (usize, Rect)>, point: Vec2) -> Option<(usize, Rect)> {
    rects.into_iter().find(|(_, rect)| rect.contains(point))
}

/// Notes which gear row the mouse is over.
pub fn hover_numbers(
    open: Res<ScreenOpen>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    rows: Query<(&GearRow, &ComputedNode, &UiGlobalTransform)>,
    mut hovered: ResMut<HoveredNumber>,
) {
    let over = if open.is(Screen::Inventory) {
        window.and_then(|w| w.cursor_position()).and_then(|point| row_under(rows.iter().map(|(r, n, t)| (r.0, node_rect(n, t))), point))
    } else {
        None
    };
    hovered.set_if_neq(HoveredNumber(over));
}

/// Spawns the tooltip, hidden, once there is a camera to draw it.
pub fn spawn_tip(mut commands: Commands, tips: Query<(), With<BreakdownTip>>, cameras: Query<Entity, With<ViewModelCamera>>) {
    if !tips.is_empty() {
        return;
    }
    let Some(camera) = cameras.iter().next() else { return };
    commands.spawn((
        Name::new("Breakdown tip"),
        DespawnOnExit(crate::launch::AppState::Delving),
        BreakdownTip,
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            width: px(TIP_WIDTH),
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            padding: UiRect::all(px(12)),
            border: UiRect::all(px(1.5)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(BG),
        BorderColor::all(LINE),
        GlobalZIndex(40),
        Visibility::Hidden,
        Pickable::IGNORE,
    ));
}

/// Fills the tooltip with the hovered number's breakdown and places it beside the row. It is
/// refilled only when the hovered row, the character or the inventory changed, and waits a
/// frame to be measured before it shows.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn show_breakdown(
    mut commands: Commands,
    open: Res<ScreenOpen>,
    tab: Res<SheetTab>,
    hovered: Res<HoveredNumber>,
    character: Res<Character>,
    inventory: Res<Inventory>,
    rules: Option<Res<GameRules>>,
    fonts: Res<UiFonts>,
    mut tips: Query<(Entity, &mut Node, &mut Visibility, &ComputedNode, Option<&ComputedUiRenderTargetInfo>), With<BreakdownTip>>,
    mut shown: Local<Option<usize>>,
) {
    let rules_changed = rules.as_ref().is_some_and(|r| r.is_changed());
    for (tip, mut node, mut visibility, computed, target) in &mut tips {
        let over = hovered.0.filter(|_| open.is(Screen::Inventory) && *tab == SheetTab::Character);
        let Some((row, rect)) = over else {
            visibility.set_if_neq(Visibility::Hidden);
            *shown = None;
            continue;
        };
        if *shown != Some(row) || hovered.is_changed() || character.is_changed() || inventory.is_changed() || rules_changed {
            commands.entity(tip).despawn_children().with_children(|tip| {
                tip.spawn((Text::new(GEAR_ROWS[row]), UiFonts::style(&fonts.display, 18.0, INK)));
                for line in breakdown(GameRules::of(&rules), &character, &inventory, row) {
                    tip.spawn((Text::new(line), UiFonts::style(&fonts.mono, 12.0, MUTED)));
                }
            });
            *shown = Some(row);
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        let size = computed.size * computed.inverse_scale_factor;
        let screen = target.map_or(Vec2::new(1280.0, 720.0), |t| t.logical_size());
        let at = place_beside(rect, size, screen);
        node.left = px(at.x);
        node.top = px(at.y);
        visibility.set_if_neq(Visibility::Inherited);
    }
}

/// Keeps the open sheet's attributes and gear numbers true to the delver, writing only the
/// lines that changed, and only when the character or the inventory did.
pub fn update_gear(
    character: Res<Character>,
    inventory: Res<Inventory>,
    rules: Option<Res<GameRules>>,
    mut attributes: Query<(&AttributeLine, &mut Text), Without<GearLine>>,
    mut gear: Query<(&GearLine, &mut Text), Without<AttributeLine>>,
) {
    let rules_changed = rules.as_ref().is_some_and(|r| r.is_changed());
    if !character.is_changed() && !inventory.is_changed() && !rules_changed {
        return;
    }
    let lines = attribute_lines(&character);
    for (AttributeLine(i), mut text) in &mut attributes {
        if text.0 != lines[*i] {
            text.0 = lines[*i].clone();
        }
    }
    let values = gear_values(&character, &character.derive_with(GameRules::of(&rules), &inventory));
    for (GearLine(i), mut text) in &mut gear {
        if text.0 != values[*i] {
            text.0 = values[*i].clone();
        }
    }
}

/// Keeps the open sheet's need bars, words and exhaustion line true to the delver, writing
/// only what changed.
pub fn update_status(
    rules: Option<Res<GameRules>>,
    players: Query<(Ref<DelverNeeds>, Ref<Exhaustion>), With<Player>>,
    mut fills: Query<(&NeedFill, &mut Node, &mut BackgroundColor)>,
    mut words: Query<(&NeedWord, &mut Text, &mut TextColor), Without<ExhaustionLine>>,
    mut exhaustions: Query<&mut Text, (With<ExhaustionLine>, Without<NeedWord>)>,
) {
    let Ok((needs, exhaustion)) = players.single() else { return };
    let rules_changed = rules.as_ref().is_some_and(|r| r.is_changed());
    if !needs.is_changed() && !exhaustion.is_changed() && !rules_changed {
        return;
    }
    let rules = GameRules::of(&rules);
    for (NeedFill(kind), mut node, mut colour) in &mut fills {
        let meter = meter(&needs.0, *kind);
        let width = percent(fill(meter) * 100.0);
        if node.width != width {
            node.width = width;
        }
        colour.set_if_neq(BackgroundColor(need_colour(meter)));
    }
    for (NeedWord(kind), mut text, mut colour) in &mut words {
        let meter = meter(&needs.0, *kind);
        if text.0 != meter.word_with(rules) {
            text.0 = meter.word_with(rules).to_string();
        }
        colour.set_if_neq(TextColor(need_colour(meter)));
    }
    let line = exhaustion_text(exhaustion.0, rules);
    for mut text in &mut exhaustions {
        if text.0 != line {
            text.0 = line.clone();
        }
    }
}

/// Fills the sheet's conditions section with the conditions the HUD shows ([`ShownConditions`]),
/// worst first: each one's badge, name and what it does, or [`NO_CONDITIONS`]. Rebuilt only
/// when the section is new or the conditions changed.
pub fn update_conditions(
    mut commands: Commands,
    shown: Option<Res<ShownConditions>>,
    lists: Query<(Entity, Ref<ConditionList>)>,
    fonts: Res<UiFonts>,
    mut icons: Option<ResMut<ConditionIcons>>,
    mut images: Option<ResMut<Assets<Image>>>,
) {
    let changed = shown.as_ref().is_some_and(|s| s.is_changed());
    let none = Vec::new();
    let conditions = shown.as_deref().map_or(&none, |s| &s.0);
    for (list, added) in &lists {
        if !changed && !added.is_added() {
            continue;
        }
        commands.entity(list).despawn_children().with_children(|list| {
            if conditions.is_empty() {
                list.spawn((Text::new(NO_CONDITIONS), UiFonts::style(&fonts.body, 15.0, MUTED)));
            }
            for condition in conditions {
                let colour = severity_colour(condition.severity);
                let icon = icons.as_deref_mut().and_then(|icons| icons.get(condition.kind, images.as_deref_mut()));
                list.spawn(Node { align_items: AlignItems::Center, column_gap: px(10), min_height: px(CONDITION_BADGE_PX), ..default() }).with_children(|line| {
                    badge(line, condition, px(CONDITION_BADGE_PX), px(2), icon);
                    line.spawn(Node { width: px(LABEL_PX - CONDITION_BADGE_PX - 10.0), flex_shrink: 0.0, ..default() })
                        .with_child((Text::new(condition.name.clone()), UiFonts::style(&fonts.mono_bold, 14.0, colour)));
                    line.spawn((Text::new(condition.effect.clone()), UiFonts::style(&fonts.body, 14.0, INK)));
                });
            }
        });
    }
}

/// Keeps the open sheet's stamina and mana bars and words true to the delver, writing only
/// what changed.
#[allow(clippy::type_complexity)]
pub fn update_vitals(
    players: Query<(Option<Ref<DelverStamina>>, Option<Ref<DelverMana>>), With<Player>>,
    mut fills: Query<(&VitalFill, &mut Node, &mut BackgroundColor)>,
    mut words: Query<(&VitalWord, &mut Text, &mut TextColor)>,
) {
    let Ok((stamina, mana)) = players.single() else { return };
    if !stamina.as_ref().is_some_and(|s| s.is_changed()) && !mana.as_ref().is_some_and(|m| m.is_changed()) {
        return;
    }
    let vitals = Vitals { stamina: stamina.map_or_else(Default::default, |s| s.0), mana: mana.map_or_else(Default::default, |m| m.0) };
    for (VitalFill(vital), mut node, mut colour) in &mut fills {
        let width = percent(vital_fill(&vitals, *vital) * 100.0);
        if node.width != width {
            node.width = width;
        }
        colour.set_if_neq(BackgroundColor(vital_colour(&vitals, *vital)));
    }
    for (VitalWord(vital), mut text, mut colour) in &mut words {
        let word = vital_word(&vitals, *vital);
        if text.0 != word {
            text.0 = word;
        }
        colour.set_if_neq(TextColor(vital_colour(&vitals, *vital)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meter_at(kind: MeterKind, value: f64) -> Meter {
        Meter { value, ..Meter::new(kind) }
    }

    #[test]
    fn the_sheets_armour_class_follows_the_loaded_rules() {
        let mut app = App::new();
        app.init_resource::<Character>().init_resource::<Inventory>().init_resource::<GameRules>().add_systems(Update, update_gear);
        let ac = app.world_mut().spawn((GearLine(4), Text::new(""))).id();
        let tip = |app: &App| app.world().get::<Text>(ac).unwrap().0.clone();
        app.update();
        let before: i32 = tip(&app).parse().unwrap();
        app.world_mut().resource_mut::<GameRules>().combat.armour_base = 14;
        app.update();
        assert_eq!(tip(&app), (before + 4).to_string());
        // The tooltip's breakdown reads the same rules.
        let rules = app.world().resource::<GameRules>().0.clone();
        let (character, inventory) = (app.world().resource::<Character>(), app.world().resource::<Inventory>());
        assert!(breakdown(&rules, character, inventory, 4)[0].contains("base (14)"), "{:?}", breakdown(&rules, character, inventory, 4));
    }

    #[test]
    fn exhaustion_is_worded_from_the_rules() {
        let mut rules = Rules::default();
        rules.exhaustion.slowed_from = 1;
        rules.exhaustion.slowed_speed = 0.75;
        rules.exhaustion.halted_from = 4;
        assert_eq!(exhaustion_text(ExhaustionLevel::new(1), &rules), "Exhaustion 1: speed ×0.75");
        assert_eq!(exhaustion_text(ExhaustionLevel::new(3), &rules), "Exhaustion 3: speed ×0.75");
        assert_eq!(exhaustion_text(ExhaustionLevel::new(4), &rules), "Exhaustion 4: cannot move");
    }

    #[test]
    fn the_words_follow_the_bands() {
        assert_eq!(meter_at(MeterKind::Hunger, 0.0).word(), "Fed");
        assert_eq!(meter_at(MeterKind::Hunger, 0.3).word(), "Peckish");
        assert_eq!(meter_at(MeterKind::Hunger, 0.55).word(), "Hungry");
        assert_eq!(need_colour(&meter_at(MeterKind::Hunger, 0.0)), OK);
        assert_eq!(need_colour(&meter_at(MeterKind::Hunger, 0.55)), ACCENT);
        assert_eq!(need_colour(&meter_at(MeterKind::Hunger, 0.9)), BAD);
    }

    /// The sheet's conditions section, with `shown` as the delver's conditions, and the lines
    /// it reads.
    fn sheet_with(shown: Vec<fb_character::conditions::Condition>) -> (App, Vec<String>) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<UiFonts>().init_resource::<ConditionIcons>().init_resource::<Assets<Image>>().insert_resource(ShownConditions(shown));
        app.add_systems(Update, update_conditions);
        app.world_mut().spawn(ConditionList);
        app.update();
        let lines = texts(&mut app);
        (app, lines)
    }

    /// The texts under the list, in the order they are laid out.
    fn texts(app: &mut App) -> Vec<String> {
        fn walk(world: &World, at: Entity, out: &mut Vec<String>) {
            if let Some(text) = world.get::<Text>(at) {
                out.push(text.0.clone());
            }
            for &child in world.get::<Children>(at).map(|c| &c[..]).unwrap_or_default() {
                walk(world, child, out);
            }
        }
        let mut lists = app.world_mut().query_filtered::<Entity, With<ConditionList>>();
        let list = lists.single(app.world()).unwrap();
        let mut out = Vec::new();
        walk(app.world(), list, &mut out);
        out
    }

    #[test]
    fn the_conditions_section_lists_each_condition_with_its_badge_and_says_so_when_there_are_none() {
        use fb_character::conditions::{Circumstances, conditions};
        let (_, empty) = sheet_with(vec![]);
        assert_eq!(empty, vec![NO_CONDITIONS.to_string()]);
        let now = Circumstances { held_pace: Some(0.25), ..Circumstances::default() };
        let rules = fb_character::rules::defaults();
        let poisoned = crate::character::DelverEffects::poisoned_for(rules, 1.0);
        let (mut app, lines) = sheet_with(conditions(&now, &poisoned.0, &rules.effects));
        assert_eq!(lines, ["Held", "Moving at 25% pace until you strike what holds you", "Poisoned", "Attacking at disadvantage; wears off in 60 minutes"]);
        let mut badges = app.world_mut().query::<(&Name, &BorderColor)>();
        let rims: Vec<(String, BorderColor)> = badges.iter(app.world()).map(|(n, c)| (n.to_string(), *c)).collect();
        assert!(rims.contains(&("Poisoned badge".to_string(), BorderColor::all(severity_colour(fb_character::conditions::Severity::Moderate)))), "{rims:?}");
        let mut icons = app.world_mut().query::<&ImageNode>();
        assert_eq!(icons.iter(app.world()).count(), 2, "each badge has its icon");
        // The list follows the conditions as they change.
        app.world_mut().resource_mut::<ShownConditions>().0.clear();
        app.update();
        assert_eq!(texts(&mut app), vec![NO_CONDITIONS.to_string()]);
    }

    #[test]
    fn exhaustion_says_what_it_does() {
        assert_eq!(exhaustion_text(ExhaustionLevel::new(0), &Rules::default()), "Exhaustion 0: no ill effect");
        assert_eq!(exhaustion_text(ExhaustionLevel::new(2), &Rules::default()), "Exhaustion 2: half speed");
        assert_eq!(exhaustion_text(ExhaustionLevel::new(5), &Rules::default()), "Exhaustion 5: cannot move");
        assert_eq!(exhaustion_text(ExhaustionLevel::new(6), &Rules::default()), "Exhaustion 6: death");
    }
}
