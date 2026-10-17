//! The header over the inventory's left leaf: who the delver is and what their gear makes of
//! them, then the leaf's tabs (the plate's `.ident`, `renderIdent` and `.tabbar`,
//! `reference/forever-below-plate.html`).
//!
//! A portrait of the doll's head in a gold frame on Stygian blue, the delver's name in the
//! display face, a line of folk, height and class, and a line of armour class, load and light
//! in Plex Mono with the values bright. The tabs are the plate's three; the inventory and character
//! tabs have a leaf each, and the outfit tab is a label for now. The showing tab is bright.

use bevy::{
    prelude::*,
    ui::{ColorStop, InterpolationColorSpace, RadialGradient, RadialGradientShape, UiPosition},
};
use fb_body::Body;

use super::doll::PORTRAIT;
use super::items::Inventory;
use crate::character::{Character, Derived};
use crate::play::SheetTab;
use crate::ui::{ACCENT, FAINT, FRAME_GOLD, INK, MUTED, STYGIAN, STYGIAN_DEEP, STYGIAN_LIGHT, UiFonts, numbers_line};

/// The line of numbers under the name.
#[derive(Component, Debug)]
pub struct HeaderStats;

/// The line saying why a drop was refused, beside the tabs.
#[derive(Component, Debug)]
pub struct ToastText;

/// The leaf's tabs, the active one first in the plate's order.
pub const TABS: [&str; 3] = ["inventory", "outfit", "character"];

/// The place of a tab in [`TABS`].
fn tab_index(tab: SheetTab) -> usize {
    match tab {
        SheetTab::Inventory => 0,
        SheetTab::Character => 2,
    }
}

/// Feet and inches for a height in world units (10 cm), rounded as the plate's `renderIdent`
/// rounds them: to the centimetre, then to the inch.
pub fn feet_and_inches(height: f32) -> String {
    let cm = (height * 10.0).round();
    let inches = (cm / 2.54).round() as u32;
    format!("{} ft {} in", inches / 12, inches % 12)
}

/// "Human · 5 ft 10 in · cleric".
pub fn identity_line(character: &Character) -> String {
    let height = Body::new(&character.spec()).height;
    format!("{} \u{b7} {} \u{b7} {}", character.folk.race().label, feet_and_inches(height), character.class.word())
}

/// "AC 11   Load 34/165 lb   Light 25 ft": the light's reach, or with none the darkvision's,
/// or "No light" (the plate's `renderIdent`).
pub fn stats_line(derived: &Derived) -> String {
    let sight = if derived.light > 0.0 {
        format!("Light {:.0} ft", derived.light)
    } else if derived.darkvision > 0.0 {
        format!("Darkvision {:.0} ft", derived.darkvision)
    } else {
        "No light".to_string()
    };
    format!("AC {}   Load {:.0}/{:.0} lb   {sight}", derived.armour_class, derived.load, derived.capacity)
}

/// The portrait's backdrop: the plate's `radial-gradient(90% 80% at 50% 38%, #3a55a0,
/// var(--stygian) 55%, var(--stygian-2))`.
fn backdrop() -> BackgroundGradient {
    BackgroundGradient::from(
        RadialGradient::new(
            UiPosition::CENTER.at(percent(50), percent(38)),
            RadialGradientShape::Ellipse(percent(90), percent(80)),
            vec![ColorStop::percent(STYGIAN_LIGHT, 0), ColorStop::percent(STYGIAN, 55), ColorStop::percent(STYGIAN_DEEP, 100)],
        )
        .in_color_space(InterpolationColorSpace::Srgba),
    )
}

/// Builds the header and the tab bar.
pub fn header(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, character: &Character, inventory: &Inventory, portrait: Handle<Image>, showing: SheetTab) {
    parent.spawn(Node { column_gap: px(16), align_items: AlignItems::Center, ..default() }).with_children(|ident| {
        ident
            .spawn((
                Node {
                    width: px(PORTRAIT.x),
                    height: px(PORTRAIT.y),
                    flex_shrink: 0.0,
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(4)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BorderColor::all(FRAME_GOLD),
                backdrop(),
            ))
            .with_child((Node { width: percent(100), height: percent(100), ..default() }, ImageNode::new(portrait)));
        ident.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(3), ..default() }).with_children(|text| {
            text.spawn((Text::new(character.name.clone()), UiFonts::style(&fonts.display, 34.0, INK)));
            text.spawn((Text::new(identity_line(character)), UiFonts::style(&fonts.body, 14.0, MUTED)));
            text.spawn((HeaderStats, Node { margin: UiRect::top(px(2)), ..default() })).with_children(|line| {
                numbers_line(line, fonts, &stats_line(&character.derive(inventory)), 13.0);
            });
        });
    });
    parent
        .spawn((
            Node { column_gap: px(24), align_items: AlignItems::Baseline, border: UiRect::bottom(px(1)), padding: UiRect::bottom(px(6)), ..default() },
            BorderColor::all(FAINT),
        ))
        .with_children(|bar| {
            for (i, tab) in TABS.iter().enumerate() {
                bar.spawn((Text::new(*tab), UiFonts::style(&fonts.display, 24.0, if i == tab_index(showing) { INK } else { MUTED })));
            }
            bar.spawn((ToastText, Text::default(), UiFonts::style(&fonts.body, 15.0, ACCENT)));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Class;
    use fb_body::Folk;

    #[test]
    fn heights_read_in_feet_and_inches() {
        // 17.8 units is 178 cm, 70 inches.
        assert_eq!(feet_and_inches(17.8), "5 ft 10 in");
        assert_eq!(feet_and_inches(10.2), "3 ft 4 in");
    }

    #[test]
    fn the_lines_read_as_the_plate_s() {
        use fb_character::LoadBand;
        let delver = Character { folk: Folk::Human, class: Class::Cleric, ..Character::roll(2) };
        let line = identity_line(&delver);
        assert!(line.starts_with("Human \u{b7} ") && line.ends_with(" \u{b7} cleric"), "{line}");
        let derived = Derived { armour_class: 11, load: 34.4, capacity: 165.0, load_band: LoadBand::Light, warmth: 2.0, light: 25.0, darkvision: 0.0, hunger_rate: 1.0, sleepiness_rate: 1.0, heal_on_kill: 0.0, speed: 1.0, block_chance: 0.0, abilities: Default::default() };
        assert_eq!(stats_line(&derived), "AC 11   Load 34/165 lb   Light 25 ft");
        assert_eq!(stats_line(&Derived { light: 0.0, darkvision: 60.0, ..derived }), "AC 11   Load 34/165 lb   Darkvision 60 ft");
        assert_eq!(stats_line(&Derived { light: 0.0, ..derived }), "AC 11   Load 34/165 lb   No light");
    }
}
