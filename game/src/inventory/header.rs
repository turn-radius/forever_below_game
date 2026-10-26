//! The header over the inventory's left leaf: who the delver is and what their gear makes of
//! them, then the leaf's tabs (the plate's `.ident`, `renderIdent` and `.tabbar`,
//! `reference/forever-below-plate.html`).
//!
//! A portrait of the doll's head in a gold frame on Stygian blue, the delver's name in the
//! display face, a line of folk, height and class, and a line of armour class, load and light
//! in Plex Mono with the values bright. The tabs are inventory, factions and character, a leaf
//! each; clicking one shows it ([`click_tabs`]) and Tab and Shift+Tab step through them
//! ([`crate::play::next_screen`]). The showing tab is bright.

use bevy::{
    prelude::*,
    ui::{ColorStop, InterpolationColorSpace, RadialGradient, RadialGradientShape, UiPosition},
};
use fb_body::Body;

use super::doll::PORTRAIT;
use super::items::Inventory;
use crate::character::{Character, Derived};
use crate::play::{Screen, ScreenOpen, SheetTab};
use crate::ui::{
    ACCENT, FAINT, FRAME_GOLD, INK, MUTED, STYGIAN, STYGIAN_DEEP, STYGIAN_LIGHT, UiFonts,
    coin_spans, node_rect, numbers_line_tinted, ui_cursor,
};
use bevy::window::PrimaryWindow;

/// The line of numbers under the name.
#[derive(Component, Debug)]
pub struct HeaderStats;

/// The line under the numbers with the delver's coin purse ([`Inventory::coins`]).
#[derive(Component, Debug)]
pub struct HeaderPurse;

/// The line saying why a drop was refused, beside the tabs.
#[derive(Component, Debug)]
pub struct ToastText;

/// The leaf's tabs, in the order of [`SheetTab::ALL`].
pub const TABS: [&str; 3] = ["inventory", "factions", "character"];

/// The place of a tab in [`TABS`].
fn tab_index(tab: SheetTab) -> usize {
    SheetTab::ALL.iter().position(|&t| t == tab).unwrap_or(0)
}

/// A tab's label in the bar: what clicking it shows.
#[derive(Component, Debug)]
pub struct TabLabel(pub SheetTab);

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
    format!(
        "{} \u{b7} {} \u{b7} {}",
        character.folk.race().label,
        feet_and_inches(height),
        character.class.word()
    )
}

/// "AC 11   Load 34/165 lb   Light 25 ft": the light's reach, or with none the darkvision's,
/// or "No light" (the plate's `renderIdent`).
pub fn stats_line(derived: &Derived) -> String {
    stats_parts(derived).join("   ")
}

/// Which of [`stats_parts`] is the load.
pub const LOAD_PART: usize = 1;

/// The three parts of [`stats_line`]: armour class, load and sight. The header lays them out
/// side by side so the load can be hovered on its own.
pub fn stats_parts(derived: &Derived) -> [String; 3] {
    let sight = if derived.light > 0.0 {
        format!("Light {:.0} ft", derived.light)
    } else if derived.darkvision > 0.0 {
        format!("Darkvision {:.0} ft", derived.darkvision)
    } else {
        "No light".to_string()
    };
    [
        format!("AC {}", derived.armour_class),
        format!("Load {:.0}/{:.0} lb", derived.load, derived.capacity),
        sight,
    ]
}

/// The carried number as `stats_line` writes it ("65/165") with the colour its band gives it, or
/// `None` while Light, which keeps the plain colour.
pub fn load_tint(derived: &Derived) -> Option<(String, Color)> {
    crate::ui::load_colour(derived.load_band).map(|colour| {
        (
            format!("{:.0}/{:.0}", derived.load, derived.capacity),
            colour,
        )
    })
}

/// The portrait's backdrop: the plate's `radial-gradient(90% 80% at 50% 38%, #3a55a0,
/// var(--stygian) 55%, var(--stygian-2))`.
fn backdrop() -> BackgroundGradient {
    BackgroundGradient::from(
        RadialGradient::new(
            UiPosition::CENTER.at(percent(50), percent(38)),
            RadialGradientShape::Ellipse(percent(90), percent(80)),
            vec![
                ColorStop::percent(STYGIAN_LIGHT, 0),
                ColorStop::percent(STYGIAN, 55),
                ColorStop::percent(STYGIAN_DEEP, 100),
            ],
        )
        .in_color_space(InterpolationColorSpace::Srgba),
    )
}

/// A portrait in its gold frame on Stygian blue, `size` logical pixels (the plate's `.portrait`):
/// the delver's in the header and the trader's on the barter screen.
pub fn portrait_frame(parent: &mut ChildSpawnerCommands, portrait: Handle<Image>, size: Vec2) {
    parent
        .spawn((
            Node {
                width: px(size.x),
                height: px(size.y),
                flex_shrink: 0.0,
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(4)),
                overflow: Overflow::clip(),
                ..default()
            },
            BorderColor::all(FRAME_GOLD),
            backdrop(),
        ))
        .with_child((
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            ImageNode::new(portrait),
        ));
}

/// Builds the header and the tab bar.
pub fn header(
    parent: &mut ChildSpawnerCommands,
    fonts: &UiFonts,
    rules: &fb_character::rules::Rules,
    character: &Character,
    inventory: &Inventory,
    portrait: Handle<Image>,
    showing: SheetTab,
) {
    parent
        .spawn(Node {
            column_gap: px(16),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|ident| {
            portrait_frame(ident, portrait, PORTRAIT);
            ident
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(3),
                    ..default()
                })
                .with_children(|text| {
                    text.spawn((
                        Text::new(character.name.clone()),
                        UiFonts::style(&fonts.display, 34.0, INK),
                    ));
                    text.spawn((
                        Text::new(identity_line(character)),
                        UiFonts::style(&fonts.body, 14.0, MUTED),
                    ));
                    // Each part in its own box, three spaces apart as `stats_line` writes them; the load's is
                    // a hover target for the load breakdown, as the sheet's Load row is.
                    text.spawn((
                        HeaderStats,
                        Node {
                            margin: UiRect::top(px(2)),
                            column_gap: px(23),
                            ..default()
                        },
                    ))
                    .with_children(|line| {
                        let derived = character.derive_with(rules, inventory);
                        let tint = load_tint(&derived);
                        for (i, part) in stats_parts(&derived).iter().enumerate() {
                            let mut at = line.spawn(Node::default());
                            if i == LOAD_PART {
                                at.insert(super::sheet::GearRow(0));
                            }
                            at.with_children(|at| {
                                numbers_line_tinted(
                                    at,
                                    fonts,
                                    part,
                                    13.0,
                                    tint.as_ref()
                                        .filter(|_| i == LOAD_PART)
                                        .map(|(n, c)| (n.as_str(), *c)),
                                )
                            });
                        }
                    });
                    text.spawn((
                        HeaderPurse,
                        Text::new("Purse "),
                        UiFonts::style(&fonts.mono, 13.0, MUTED),
                    ))
                    .with_children(|line| {
                        let coins = inventory.coins();
                        if coins.is_empty() {
                            line.spawn((
                                TextSpan::new("empty"),
                                UiFonts::style(&fonts.mono, 13.0, MUTED),
                            ));
                        } else {
                            coin_spans(line, fonts, coins, 13.0);
                        }
                    });
                });
        });
    parent
        .spawn((
            Node {
                column_gap: px(24),
                align_items: AlignItems::Baseline,
                border: UiRect::bottom(px(1)),
                padding: UiRect::bottom(px(6)),
                ..default()
            },
            BorderColor::all(FAINT),
        ))
        .with_children(|bar| {
            for (i, tab) in TABS.iter().enumerate() {
                bar.spawn((
                    TabLabel(SheetTab::ALL[i]),
                    Text::new(*tab),
                    UiFonts::style(
                        &fonts.display,
                        24.0,
                        if i == tab_index(showing) { INK } else { MUTED },
                    ),
                ));
            }
            bar.spawn((
                ToastText,
                Text::default(),
                UiFonts::style(&fonts.body, 15.0, ACCENT),
            ));
        });
}

/// The tab whose label holds `point`, of the labels `rects`.
pub fn tab_under(
    rects: impl IntoIterator<Item = (SheetTab, Rect)>,
    point: Vec2,
) -> Option<SheetTab> {
    rects
        .into_iter()
        .find(|(_, rect)| rect.contains(point))
        .map(|(tab, _)| tab)
}

/// Shows the tab whose label is clicked while the inventory screen is open.
pub fn click_tabs(
    open: Res<ScreenOpen>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    labels: Query<(&TabLabel, &ComputedNode, &UiGlobalTransform)>,
    mut tab: ResMut<SheetTab>,
) {
    if !open.is(Screen::Inventory) || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(point) = window.and_then(|w| ui_cursor(&w, ui_scale.as_deref())) else {
        return;
    };
    if let Some(clicked) = tab_under(labels.iter().map(|(l, n, t)| (l.0, node_rect(n, t))), point) {
        tab.set_if_neq(clicked);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Class;
    use fb_body::Folk;

    #[test]
    fn clicking_a_tab_label_shows_that_tab() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<ScreenOpen>()
            .init_resource::<SheetTab>()
            .add_systems(Update, click_tabs);
        let mut window = Window::default();
        window.set_cursor_position(Some(Vec2::new(130.0, 20.0)));
        app.world_mut().spawn((window, PrimaryWindow));
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        for (i, tab) in SheetTab::ALL.into_iter().enumerate() {
            app.world_mut().spawn((
                TabLabel(tab),
                ComputedNode {
                    size: Vec2::new(100.0, 24.0),
                    inverse_scale_factor: 1.0,
                    ..default()
                },
                UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(
                    50.0 + 120.0 * i as f32,
                    20.0,
                ))),
            ));
        }
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert_eq!(
            *app.world().resource::<SheetTab>(),
            SheetTab::Factions,
            "the second label"
        );
        {
            let mut m = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
            m.release(MouseButton::Left);
            m.clear();
        }
        app.world_mut()
            .query::<&mut Window>()
            .single_mut(app.world_mut())
            .unwrap()
            .set_cursor_position(Some(Vec2::new(250.0, 20.0)));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert_eq!(*app.world().resource::<SheetTab>(), SheetTab::Character);
        {
            let mut m = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
            m.release(MouseButton::Left);
            m.clear();
        }
        app.world_mut()
            .query::<&mut Window>()
            .single_mut(app.world_mut())
            .unwrap()
            .set_cursor_position(Some(Vec2::new(500.0, 20.0)));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert_eq!(
            *app.world().resource::<SheetTab>(),
            SheetTab::Character,
            "off the labels nothing changes"
        );
    }

    #[test]
    fn the_tab_bar_names_inventory_factions_character() {
        assert_eq!(TABS, ["inventory", "factions", "character"]);
        assert_eq!(tab_index(SheetTab::Factions), 1);
    }

    #[test]
    fn heights_read_in_feet_and_inches() {
        // 17.8 units is 178 cm, 70 inches.
        assert_eq!(feet_and_inches(17.8), "5 ft 10 in");
        assert_eq!(feet_and_inches(10.2), "3 ft 4 in");
    }

    #[test]
    fn the_lines_read_as_the_plate_s() {
        use fb_character::LoadBand;
        let delver = Character {
            folk: Folk::Human,
            class: Class::Cleric,
            ..Character::roll(2)
        };
        let line = identity_line(&delver);
        assert!(
            line.starts_with("Human \u{b7} ") && line.ends_with(" \u{b7} cleric"),
            "{line}"
        );
        let derived = Derived {
            armour_class: 11,
            load: 34.4,
            capacity: 165.0,
            load_band: LoadBand::Light,
            warmth: 2.0,
            light: 25.0,
            darkvision: 0.0,
            hunger_rate: 1.0,
            sleepiness_rate: 1.0,
            heal_on_kill: 0.0,
            speed: 1.0,
            block_chance: 0.0,
            luck: 0.0,
            abilities: Default::default(),
            to_hit: 0.0,
            ranged_to_hit: 0.0,
            attack_rate: 1.0,
            stamina_refill: 1.0,
            stamina_cost: 1.0,
            ward: [0.0; 6],
            shun: [0.0; 6],
        };
        assert_eq!(stats_line(&derived), "AC 11   Load 34/165 lb   Light 25 ft");
        assert_eq!(
            stats_line(&Derived {
                light: 0.0,
                darkvision: 60.0,
                ..derived
            }),
            "AC 11   Load 34/165 lb   Darkvision 60 ft"
        );
        assert_eq!(
            stats_line(&Derived {
                light: 0.0,
                ..derived
            }),
            "AC 11   Load 34/165 lb   No light"
        );
    }
}
