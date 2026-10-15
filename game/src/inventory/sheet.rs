//! The character tab: the inventory screen's third leaf, in the plate's look (the dark ledger's
//! palette, Almendra headings, Plex Mono values). It shows only what the game simulates.
//!
//! The status section reads the delver's `DelverNeeds`, `Exhaustion` and load band. The screen
//! builds the sheet when the tab opens or the gear changes ([`super::build_screen`]); while it
//! is open [`update_status`] rewrites only the words and bars that moved, so nothing is rebuilt
//! when the delver eats or drinks.

use bevy::prelude::*;
use fb_character::{Exhaustion as ExhaustionLevel, Meter, MeterKind, Needs};

use super::items::Inventory;
use crate::character::{Character, DelverNeeds};
use crate::exhaustion::Exhaustion;
use crate::play::Player;
use crate::ui::{ACCENT, BAD, BRIGHT, CELL, FAINT, INK, LINE, MUTED, OK, UiFonts};

/// The leaf under the header when the character tab is showing.
#[derive(Component, Debug)]
pub struct CharacterSheet;

/// The filled part of a need's bar.
#[derive(Component, Debug)]
pub struct NeedFill(pub MeterKind);

/// A need's word: Fed, Peckish, Hungry.
#[derive(Component, Debug)]
pub struct NeedWord(pub MeterKind);

/// The exhaustion line: "Exhaustion 2: half speed".
#[derive(Component, Debug)]
pub struct ExhaustionLine;

/// The load band's word.
#[derive(Component, Debug)]
pub struct LoadWord;

/// Width of a label column and of a bar, in logical pixels.
const LABEL_PX: f32 = 120.0;
const BAR_PX: f32 = 180.0;
const BAR_HEIGHT_PX: f32 = 10.0;

/// The meters in the order they are shown, with their labels.
pub const NEEDS: [(MeterKind, &str); 3] = [(MeterKind::Hunger, "Hunger"), (MeterKind::Thirst, "Thirst"), (MeterKind::Sleepiness, "Sleepiness")];

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

/// What an exhaustion level does, in the words the game plays by: only the slowing is simulated.
pub fn exhaustion_effect(exhaustion: ExhaustionLevel) -> &'static str {
    if exhaustion.is_dead() {
        "death"
    } else if exhaustion.speed_multiplier() <= 0.0 {
        "cannot move"
    } else if exhaustion.speed_multiplier() < 1.0 {
        "half speed"
    } else {
        "no ill effect"
    }
}

/// "Exhaustion 2: half speed".
pub fn exhaustion_text(exhaustion: ExhaustionLevel) -> String {
    format!("Exhaustion {}: {}", exhaustion.level, exhaustion_effect(exhaustion))
}

/// A heading over a section, in the display face with a rule beneath.
fn heading(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, title: &str) {
    parent
        .spawn((Node { border: UiRect::bottom(px(1)), padding: UiRect::bottom(px(2)), margin: UiRect::top(px(6)), ..default() }, BorderColor::all(FAINT)))
        .with_child((Text::new(title), UiFonts::style(&fonts.display, 20.0, INK)));
}

/// A row: its label in the body face, then what `fill_row` puts after it.
fn row(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, fill_row: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent.spawn(Node { align_items: AlignItems::Center, column_gap: px(12), min_height: px(22), ..default() }).with_children(|row| {
        row.spawn(Node { width: px(LABEL_PX), flex_shrink: 0.0, ..default() }).with_child((Text::new(label), UiFonts::style(&fonts.body, 15.0, MUTED)));
        fill_row(row);
    });
}

/// Builds the character tab's body: the status section.
pub fn sheet(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, character: &Character, inventory: &Inventory, needs: &Needs, exhaustion: ExhaustionLevel) {
    parent.spawn((CharacterSheet, Name::new("CharacterSheet"), Node { flex_direction: FlexDirection::Column, row_gap: px(4), ..default() })).with_children(|sheet| {
        heading(sheet, fonts, "status");
        for (kind, label) in NEEDS {
            let meter = meter(needs, kind);
            row(sheet, fonts, label, |row| {
                row.spawn((
                    Node { width: px(BAR_PX), height: px(BAR_HEIGHT_PX), flex_shrink: 0.0, border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(3)), overflow: Overflow::clip(), ..default() },
                    BackgroundColor(CELL),
                    BorderColor::all(LINE),
                ))
                .with_child((NeedFill(kind), Node { width: percent(fill(meter) * 100.0), height: percent(100), ..default() }, BackgroundColor(need_colour(meter))));
                row.spawn((NeedWord(kind), Text::new(meter.word()), UiFonts::style(&fonts.mono_bold, 14.0, need_colour(meter))));
            });
        }
        row(sheet, fonts, "Load", |row| {
            row.spawn((LoadWord, Text::new(character.derive(inventory).load_band.word()), UiFonts::style(&fonts.mono_bold, 14.0, BRIGHT)));
        });
        row(sheet, fonts, "Exhaustion", |row| {
            row.spawn((ExhaustionLine, Text::new(exhaustion_text(exhaustion)), UiFonts::style(&fonts.mono_bold, 14.0, BRIGHT)));
        });
        // Stamina and mana go here once their meters land (#267, #268): a bar and a word each,
        // as the needs above, read by the same live system.
    });
}

/// Keeps the open sheet's bars and words true to the delver, writing only what changed.
pub fn update_status(
    players: Query<(Ref<DelverNeeds>, Ref<Exhaustion>), With<Player>>,
    character: Res<Character>,
    inventory: Res<Inventory>,
    mut fills: Query<(&NeedFill, &mut Node, &mut BackgroundColor)>,
    mut words: Query<(&NeedWord, &mut Text, &mut TextColor), (Without<LoadWord>, Without<ExhaustionLine>)>,
    mut loads: Query<&mut Text, (With<LoadWord>, Without<NeedWord>, Without<ExhaustionLine>)>,
    mut exhaustions: Query<&mut Text, (With<ExhaustionLine>, Without<NeedWord>, Without<LoadWord>)>,
) {
    let Ok((needs, exhaustion)) = players.single() else { return };
    if !needs.is_changed() && !exhaustion.is_changed() && !character.is_changed() && !inventory.is_changed() {
        return;
    }
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
        if text.0 != meter.word() {
            text.0 = meter.word().to_string();
        }
        colour.set_if_neq(TextColor(need_colour(meter)));
    }
    let band = character.derive(&inventory).load_band.word();
    for mut text in &mut loads {
        if text.0 != band {
            text.0 = band.to_string();
        }
    }
    let line = exhaustion_text(exhaustion.0);
    for mut text in &mut exhaustions {
        if text.0 != line {
            text.0 = line.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meter_at(kind: MeterKind, value: f64) -> Meter {
        Meter { value, ..Meter::new(kind) }
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

    #[test]
    fn exhaustion_says_what_it_does() {
        assert_eq!(exhaustion_text(ExhaustionLevel::new(0)), "Exhaustion 0: no ill effect");
        assert_eq!(exhaustion_text(ExhaustionLevel::new(2)), "Exhaustion 2: half speed");
        assert_eq!(exhaustion_text(ExhaustionLevel::new(5)), "Exhaustion 5: cannot move");
        assert_eq!(exhaustion_text(ExhaustionLevel::new(6)), "Exhaustion 6: death");
    }
}
