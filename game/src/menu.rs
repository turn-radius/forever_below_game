//! The pause menu: Esc, with no other screen open, brings up a dark page over the level with
//! Resume, Controls, Settings and Quit. While it is open the body stands still, the mouse is
//! free and the level's clock stops ([`Time<Virtual>`]); the music plays on.
//!
//! Controls lists every binding in the [`Keymap`], by group, so it always shows the keys the
//! game reads. Settings has a slider or a toggle for each [`Setting`]; a change applies the
//! same frame and is saved to `settings.json` ([`crate::settings`]). `FB_OPEN_MENU=main|controls|settings` opens the menu at the start, on that
//! page, for captures.
//!
//! The buttons and the settings page work wherever they are on screen: the title screen
//! ([`crate::title`]) builds its own buttons and shows the same settings page ([`menu_page`]).

use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};

use crate::carry::ViewModelCamera;
use crate::keymap::{Action, ActionInput, Group, Keymap};
use crate::launch::AppState;
use crate::play::{DelveSystems, Screen, ScreenOpen, show_screen, switch_screens};
use crate::settings::{GameSettings, Setting};
use crate::ui::{BG, BRIGHT, CELL, FAINT, INK, LINE, MUTED, UiFonts, node_rect};

/// The page of the menu on show.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuPage {
    #[default]
    Main,
    Controls,
    Settings,
}

impl MenuPage {
    /// The page named `name` ("main", "controls" or "settings").
    pub fn named(name: &str) -> Option<MenuPage> {
        match name {
            "main" => Some(MenuPage::Main),
            "controls" => Some(MenuPage::Controls),
            "settings" => Some(MenuPage::Settings),
            _ => None,
        }
    }
}

/// What a menu button does when chosen.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuCommand {
    /// Closes the menu and goes back to the level.
    Resume,
    /// Shows a page of the menu.
    Show(MenuPage),
    /// Leaves the game.
    Quit,
    /// Begins a new delve, from the title ([`crate::title`] carries it out).
    NewDelve,
}

/// The whole menu, over the level.
#[derive(Component, Debug)]
#[require(DespawnOnExit<crate::launch::AppState> = DespawnOnExit(crate::launch::AppState::Delving))]
pub struct MenuScreen;

/// A button on the menu, and what it does.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[require(ButtonInk)]
pub struct MenuButton(pub MenuCommand);

/// The colours of a button's label: at rest, and with the mouse over it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ButtonInk {
    pub idle: Color,
    pub hover: Color,
}

impl Default for ButtonInk {
    /// The pause menu's: muted tan, cream under the mouse.
    fn default() -> Self {
        ButtonInk { idle: MUTED, hover: INK }
    }
}

/// A line of the controls list, for the binding of this action.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlRow(pub Action);

/// Where a setting is changed: a slider's bar, or a toggle's box. A click or drag here sets it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingControl(pub Setting);

/// A slider's filled part, or a toggle's mark.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingFill(pub Setting);

/// A slider's knob.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingKnob(pub Setting);

/// A setting's value as text ("70%", "On").
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingValue(pub Setting);

/// A slider's width in logical pixels.
const SLIDER_PX: f32 = 280.0;

/// The menu's width in logical pixels.
pub const MENU_PX: f32 = 640.0;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Keymap>()
            .init_resource::<UiFonts>()
            // Kept as loaded if `SettingsPlugin` came first.
            .init_resource::<GameSettings>()
            .init_resource::<ScreenOpen>()
            .init_resource::<MenuPage>()
            .add_message::<MenuCommand>()
            .add_systems(PostStartup, open_for_capture.run_if(in_state(AppState::Delving)).run_if(|| capture_page().is_some()))
            .add_systems(
                Update,
                (
                    choose_buttons,
                    adjust_settings,
                    apply_commands,
                    pause_while_open.in_set(DelveSystems),
                    build_menu.in_set(DelveSystems),
                    show_settings,
                    light_hovered,
                )
                    .chain()
                    .after(switch_screens),
            );
    }
}

/// The page `FB_OPEN_MENU` asks for.
pub fn capture_page() -> Option<MenuPage> {
    std::env::var("FB_OPEN_MENU").ok().and_then(|name| MenuPage::named(&name))
}

fn open_for_capture(mut open: ResMut<ScreenOpen>, mut page: ResMut<MenuPage>, mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>) {
    *page = capture_page().unwrap_or_default();
    show_screen(&mut open, Some(Screen::Menu), cursor.as_deref_mut().map(|c| &mut **c));
}

/// Stops the level's clock while the menu is open, and opens it on the main page next time.
fn pause_while_open(open: Res<ScreenOpen>, time: Option<ResMut<Time<Virtual>>>, mut page: ResMut<MenuPage>) {
    if !open.is_changed() {
        return;
    }
    let paused = open.is(Screen::Menu);
    if let Some(mut time) = time {
        if paused {
            time.pause();
        } else {
            time.unpause();
        }
    }
    if !paused {
        page.set_if_neq(MenuPage::Main);
    }
}

/// The button under the mouse, if any.
fn hovered_button<'a>(
    window: Option<&Window>,
    buttons: impl IntoIterator<Item = (&'a MenuButton, &'a ComputedNode, &'a UiGlobalTransform)>,
) -> Option<MenuCommand> {
    let point = window?.cursor_position()?;
    buttons.into_iter().find(|(_, node, transform)| node_rect(node, transform).contains(point)).map(|(button, ..)| button.0)
}

/// A click on a button sends its [`MenuCommand`]. Buttons exist only while a menu shows.
fn choose_buttons(
    input: ActionInput,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    buttons: Query<(&MenuButton, &ComputedNode, &UiGlobalTransform)>,
    mut chosen: MessageWriter<MenuCommand>,
) {
    if input.just_pressed(Action::Choose)
        && let Some(command) = hovered_button(window.as_deref().copied(), buttons.iter())
    {
        chosen.write(command);
    }
}

fn apply_commands(
    mut chosen: MessageReader<MenuCommand>,
    mut open: ResMut<ScreenOpen>,
    mut page: ResMut<MenuPage>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
    mut exit: MessageWriter<AppExit>,
) {
    for command in chosen.read() {
        match *command {
            MenuCommand::Resume => show_screen(&mut open, None, cursor.as_deref_mut().map(|c| &mut **c)),
            MenuCommand::Show(shown) => *page = shown,
            MenuCommand::Quit => {
                exit.write(AppExit::Success);
            }
            MenuCommand::NewDelve => {}
        }
    }
}

/// One line of the controls list: an action, its name and its keys ("Right click").
pub type ControlLine = (Action, &'static str, String);

/// The controls list: each group in turn with its bindings' lines.
pub fn controls(keymap: &Keymap) -> Vec<(Group, Vec<ControlLine>)> {
    Group::ALL
        .iter()
        .map(|&group| (group, keymap.in_group(group).map(|b| (b.action, b.name, b.labels().join(" or "))).collect()))
        .collect()
}

/// Builds the menu when it opens or changes page; takes it down when it closes.
#[allow(clippy::too_many_arguments)]
fn build_menu(
    mut commands: Commands,
    open: Res<ScreenOpen>,
    page: Res<MenuPage>,
    keymap: Res<Keymap>,
    settings: Res<GameSettings>,
    fonts: Res<UiFonts>,
    menus: Query<Entity, With<MenuScreen>>,
    cameras: Query<Entity, With<ViewModelCamera>>,
) {
    let existing = menus.iter().next();
    if !open.is(Screen::Menu) {
        if let Some(menu) = existing {
            commands.entity(menu).despawn();
        }
        return;
    }
    if existing.is_some() && !open.is_changed() && !page.is_changed() && !keymap.is_changed() {
        return;
    }
    // Drawn by the camera that draws over the held torch, as the HUD and inventory are.
    let Some(camera) = cameras.iter().next() else { return };
    if let Some(menu) = existing {
        commands.entity(menu).despawn();
    }
    commands
        .spawn((
            Name::new("Menu"),
            MenuScreen,
            UiTargetCamera(camera),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            // The page, with the level showing faintly through. Bevy blends UI in linear light,
            // where 10% of a lit wall already reads brown over the near-black page.
            BackgroundColor(BG.with_alpha(0.96)),
            GlobalZIndex(20),
        ))
        .with_children(|root| {
            root.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(18), width: px(MENU_PX), ..default() })
                .with_children(|menu| menu_page(menu, &fonts, *page, &keymap, &settings));
        });
}

/// A page of the menu under the game's name and a rule, into a column `MENU_PX` wide.
pub fn menu_page(menu: &mut ChildSpawnerCommands, fonts: &UiFonts, page: MenuPage, keymap: &Keymap, settings: &GameSettings) {
    menu.spawn((Text::new("Forever Below"), UiFonts::style(&fonts.display, 54.0, INK)));
    menu.spawn((Node { width: percent(100), height: px(1), margin: UiRect::bottom(px(10)), ..default() }, BackgroundColor(FAINT)));
    match page {
        MenuPage::Main => main_page(menu, fonts),
        MenuPage::Controls => controls_page(menu, fonts, keymap),
        MenuPage::Settings => settings_page(menu, fonts, keymap, settings),
    }
}

/// A button: a word in the body face, muted until the mouse is over it.
fn button(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, command: MenuCommand, size: f32) {
    parent.spawn((MenuButton(command), Node { padding: UiRect::axes(px(18), px(4)), ..default() })).with_children(|b| {
        b.spawn((Text::new(label), UiFonts::style(&fonts.body_medium, size, MUTED)));
    });
}

fn main_page(menu: &mut ChildSpawnerCommands, fonts: &UiFonts) {
    menu.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(6), ..default() }).with_children(|list| {
        for (label, command) in [
            ("Resume", MenuCommand::Resume),
            ("Controls", MenuCommand::Show(MenuPage::Controls)),
            ("Settings", MenuCommand::Show(MenuPage::Settings)),
            ("Quit", MenuCommand::Quit),
        ] {
            button(list, fonts, label, command, 26.0);
        }
    });
}

/// A page's heading, in the display face, as the inventory's is.
fn heading(menu: &mut ChildSpawnerCommands, fonts: &UiFonts, text: &str) {
    menu.spawn(Node { width: percent(100), ..default() }).with_child((Text::new(text), UiFonts::style(&fonts.display, 26.0, INK)));
}

fn back_button(menu: &mut ChildSpawnerCommands, fonts: &UiFonts) {
    menu.spawn(Node { margin: UiRect::top(px(8)), ..default() }).with_children(|row| button(row, fonts, "Back", MenuCommand::Show(MenuPage::Main), 20.0));
}

fn controls_page(menu: &mut ChildSpawnerCommands, fonts: &UiFonts, keymap: &Keymap) {
    heading(menu, fonts, "controls");
    menu.spawn(Node { width: percent(100), flex_wrap: FlexWrap::Wrap, column_gap: px(40), row_gap: px(20), ..default() }).with_children(|columns| {
        for (group, rows) in controls(keymap) {
            columns
                .spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(5), width: px((MENU_PX - 40.0) / 2.0), ..default() })
                .with_children(|list| {
                    list.spawn((Text::new(group.name()), UiFonts::style(&fonts.display, 19.0, INK)));
                    for (action, name, keys) in rows {
                        list.spawn((
                            ControlRow(action),
                            Node { justify_content: JustifyContent::SpaceBetween, column_gap: px(12), border: UiRect::bottom(px(1)), padding: UiRect::bottom(px(3)), ..default() },
                            BorderColor::all(FAINT),
                        ))
                        .with_children(|row| {
                            row.spawn((Text::new(name), UiFonts::style(&fonts.body, 16.0, MUTED)));
                            row.spawn((Text::new(keys), UiFonts::style(&fonts.mono, 14.0, BRIGHT), Node { flex_shrink: 0.0, ..default() }));
                        });
                    }
                });
        }
    });
    back_button(menu, fonts);
}

fn settings_page(menu: &mut ChildSpawnerCommands, fonts: &UiFonts, keymap: &Keymap, settings: &GameSettings) {
    heading(menu, fonts, "settings");
    menu.spawn(Node { flex_direction: FlexDirection::Column, width: percent(100), row_gap: px(10), ..default() }).with_children(|list| {
        for setting in Setting::ALL {
            list.spawn(Node { align_items: AlignItems::Center, column_gap: px(20), border: UiRect::bottom(px(1)), padding: UiRect::bottom(px(8)), ..default() })
                .insert(BorderColor::all(FAINT))
                .with_children(|row| {
                    row.spawn((Text::new(setting.name()), UiFonts::style(&fonts.body, 17.0, MUTED), Node { width: px(200), ..default() }));
                    match setting.range() {
                        Some(_) => slider(row, setting, setting.fraction(settings)),
                        None => toggle(row, setting, setting.fraction(settings) >= 0.5),
                    }
                    row.spawn((
                        SettingValue(setting),
                        Text::new(setting.shown(settings)),
                        UiFonts::style(&fonts.mono, 15.0, BRIGHT),
                        TextLayout::justify(Justify::Right),
                        Node { flex_grow: 1.0, ..default() },
                    ));
                });
        }
    });
    let keys = |action| keymap.binding(action).map(|b| b.labels().join(" or ")).unwrap_or_default();
    menu.spawn((
        Node { width: percent(100), ..default() },
        children![(
            Text::new(format!("The music also turns down and up with {} and {}.", keys(Action::MusicQuieter), keys(Action::MusicLouder))),
            UiFonts::style(&fonts.body_italic, 15.0, MUTED),
        )],
    ));
    back_button(menu, fonts);
}

/// A slider in the ledger's colours: a cell-dark bar ruled in tan, filled to its value in muted
/// tan, with a cream knob. The node is taller than the bar so it is easy to catch.
fn slider(row: &mut ChildSpawnerCommands, setting: Setting, fraction: f32) {
    row.spawn((SettingControl(setting), Node { width: px(SLIDER_PX), height: px(22), align_items: AlignItems::Center, ..default() })).with_children(|bar| {
        bar.spawn((
            Node { width: percent(100), height: px(6), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(3)), ..default() },
            BackgroundColor(CELL),
            BorderColor::all(LINE),
        ))
        .with_child((SettingFill(setting), Node { width: percent(fraction * 100.0), height: percent(100), border_radius: BorderRadius::all(px(3)), ..default() }, BackgroundColor(MUTED)));
        bar.spawn((
            SettingKnob(setting),
            Node {
                position_type: PositionType::Absolute,
                left: percent(fraction * 100.0),
                width: px(14),
                height: px(14),
                margin: UiRect::left(px(-7)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(INK),
            BorderColor::all(BG),
        ));
    });
}

/// A toggle: a small box ruled in tan, with a cream mark when on.
fn toggle(row: &mut ChildSpawnerCommands, setting: Setting, on: bool) {
    row.spawn((SettingControl(setting), Node { width: px(SLIDER_PX), height: px(22), align_items: AlignItems::Center, ..default() })).with_children(|cell| {
        cell.spawn((
            Node {
                width: px(20),
                height: px(20),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(4)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(CELL),
            BorderColor::all(LINE),
        ))
        .with_child((
            SettingFill(setting),
            Node { width: px(10), height: px(10), border_radius: BorderRadius::all(px(2)), ..default() },
            BackgroundColor(INK),
            if on { Visibility::Inherited } else { Visibility::Hidden },
        ));
    });
}

/// How far along a slider drawn at `rect` the point `x` is, 0 to 1.
pub fn fraction_along(rect: Rect, x: f32) -> f32 {
    if rect.width() <= 0.0 {
        return 0.0;
    }
    ((x - rect.min.x) / rect.width()).clamp(0.0, 1.0)
}

/// A click on a toggle flips it; a press on a slider sets it under the mouse and drags it
/// until the button comes up.
#[allow(clippy::too_many_arguments)]
fn adjust_settings(
    input: ActionInput,
    page: Res<MenuPage>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    controls: Query<(&SettingControl, &ComputedNode, &UiGlobalTransform)>,
    mut held: Local<Option<Setting>>,
    mut settings: ResMut<GameSettings>,
) {
    if *page != MenuPage::Settings || controls.is_empty() || !input.pressed(Action::Choose) {
        *held = None;
        return;
    }
    let Some(point) = window.and_then(|w| w.cursor_position()) else { return };
    let mut next = *settings;
    if input.just_pressed(Action::Choose)
        && let Some((control, ..)) = controls.iter().find(|(_, node, transform)| node_rect(node, transform).contains(point))
    {
        match control.0.range() {
            Some(_) => *held = Some(control.0),
            None => {
                let flipped = 1.0 - control.0.get(&next);
                control.0.set(&mut next, flipped);
            }
        }
    }
    if let Some(setting) = *held
        && let Some((_, node, transform)) = controls.iter().find(|(c, ..)| c.0 == setting)
    {
        setting.set_fraction(&mut next, fraction_along(node_rect(node, transform), point.x));
    }
    settings.set_if_neq(next);
}

/// Keeps the sliders, toggles and values on the settings page showing the settings, which the
/// keyboard can change too.
fn show_settings(
    settings: Res<GameSettings>,
    mut fills: Query<(&SettingFill, &mut Node, &mut Visibility), Without<SettingKnob>>,
    mut knobs: Query<(&SettingKnob, &mut Node), Without<SettingFill>>,
    mut values: Query<(&SettingValue, &mut Text)>,
) {
    if !settings.is_changed() {
        return;
    }
    for (fill, mut node, mut visibility) in &mut fills {
        let fraction = fill.0.fraction(&settings);
        if fill.0.range().is_some() {
            let width = percent(fraction * 100.0);
            if node.width != width {
                node.width = width;
            }
        } else {
            visibility.set_if_neq(if fraction >= 0.5 { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
    for (knob, mut node) in &mut knobs {
        let left = percent(knob.0.fraction(&settings) * 100.0);
        if node.left != left {
            node.left = left;
        }
    }
    for (value, mut text) in &mut values {
        let shown = value.0.shown(&settings);
        if text.0 != shown {
            text.0 = shown;
        }
    }
}

/// Brightens the button under the mouse.
fn light_hovered(
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    buttons: Query<(&MenuButton, &ButtonInk, &ComputedNode, &UiGlobalTransform, &Children)>,
    mut colours: Query<&mut TextColor>,
) {
    let over = hovered_button(window.as_deref().copied(), buttons.iter().map(|(b, _, n, t, _)| (b, n, t)));
    for (button, ink, _, _, children) in &buttons {
        let colour = if over == Some(button.0) { ink.hover } else { ink.idle };
        for child in children {
            if let Ok(mut text) = colours.get_mut(*child) {
                text.set_if_neq(TextColor(colour));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::window::CursorGrabMode;

    fn menu_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<crate::play::SheetTab>()
            .add_systems(Update, switch_screens)
            .add_plugins(MenuPlugin);
        app.world_mut().spawn(ViewModelCamera { follows: Entity::PLACEHOLDER });
        app.world_mut().spawn((Window::default(), CursorOptions::default(), PrimaryWindow));
        app
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
        app.update();
    }

    fn open(app: &App) -> Option<Screen> {
        app.world().resource::<ScreenOpen>().0
    }

    fn grabbed(app: &mut App) -> bool {
        let mut cursors = app.world_mut().query::<&CursorOptions>();
        cursors.single(app.world()).unwrap().grab_mode != CursorGrabMode::None
    }

    fn choose(app: &mut App, command: MenuCommand) {
        app.world_mut().write_message(command);
        app.update();
    }

    #[test]
    fn escape_closes_the_inventory_then_opens_the_menu() {
        let mut app = menu_app();
        press(&mut app, KeyCode::KeyI);
        assert_eq!(open(&app), Some(Screen::Inventory));
        press(&mut app, KeyCode::Escape);
        assert_eq!(open(&app), None);
        assert!(grabbed(&mut app));
        press(&mut app, KeyCode::Escape);
        assert_eq!(open(&app), Some(Screen::Menu));
        assert!(!grabbed(&mut app), "the mouse is free over the menu");
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
        let mut menus = app.world_mut().query_filtered::<(), With<MenuScreen>>();
        assert_eq!(menus.iter(app.world()).count(), 1);
        press(&mut app, KeyCode::Escape);
        assert_eq!(open(&app), None);
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
        assert_eq!(menus.iter(app.world()).count(), 0);
    }

    #[test]
    fn resume_goes_back_to_the_level_with_the_mouse() {
        let mut app = menu_app();
        press(&mut app, KeyCode::Escape);
        choose(&mut app, MenuCommand::Show(MenuPage::Controls));
        assert_eq!(*app.world().resource::<MenuPage>(), MenuPage::Controls);
        choose(&mut app, MenuCommand::Resume);
        assert_eq!(open(&app), None);
        assert!(grabbed(&mut app));
        press(&mut app, KeyCode::Escape);
        assert_eq!(*app.world().resource::<MenuPage>(), MenuPage::Main, "it opens on the main page again");
    }

    #[test]
    fn quit_leaves_the_game() {
        let mut app = menu_app();
        press(&mut app, KeyCode::Escape);
        choose(&mut app, MenuCommand::Quit);
        let exits = app.world().resource::<Messages<AppExit>>();
        assert!(exits.iter_current_update_messages().any(|e| *e == AppExit::Success));
    }

    #[test]
    fn sliders_read_the_mouse_along_their_bar() {
        let bar = Rect::new(100.0, 10.0, 380.0, 32.0);
        assert_eq!(fraction_along(bar, 100.0), 0.0);
        assert_eq!(fraction_along(bar, 240.0), 0.5);
        assert_eq!(fraction_along(bar, 500.0), 1.0);
        assert_eq!(fraction_along(bar, 0.0), 0.0);
        assert_eq!(fraction_along(Rect::new(5.0, 0.0, 5.0, 10.0), 9.0), 0.0);
    }

    #[test]
    fn the_settings_page_shows_every_setting_and_follows_the_keys() {
        let mut app = menu_app();
        app.add_systems(Update, crate::audio::turn_music_volume.before(switch_screens));
        press(&mut app, KeyCode::Escape);
        choose(&mut app, MenuCommand::Show(MenuPage::Settings));
        app.update();
        let mut controls = app.world_mut().query::<&SettingControl>();
        assert_eq!(controls.iter(app.world()).map(|c| c.0).collect::<Vec<_>>(), Setting::ALL.to_vec());
        let shown = |app: &mut App, setting: Setting| {
            let mut values = app.world_mut().query::<(&SettingValue, &Text)>();
            values.iter(app.world()).find(|(v, _)| v.0 == setting).map(|(_, t)| t.0.clone()).unwrap()
        };
        assert_eq!(shown(&mut app, Setting::MusicVolume), "100%");
        assert_eq!(shown(&mut app, Setting::Fullscreen), "Off");
        press(&mut app, KeyCode::Minus);
        assert_eq!(shown(&mut app, Setting::MusicVolume), "90%", "minus still turns the music down, and the page shows it");
        app.world_mut().resource_mut::<GameSettings>().fullscreen = true;
        app.update();
        assert_eq!(shown(&mut app, Setting::Fullscreen), "On");
    }

    #[test]
    fn the_main_page_has_its_four_buttons() {
        let mut app = menu_app();
        press(&mut app, KeyCode::Escape);
        let mut buttons = app.world_mut().query::<&MenuButton>();
        let shown: Vec<MenuCommand> = buttons.iter(app.world()).map(|b| b.0).collect();
        assert_eq!(shown, vec![MenuCommand::Resume, MenuCommand::Show(MenuPage::Controls), MenuCommand::Show(MenuPage::Settings), MenuCommand::Quit]);
    }

    #[test]
    fn the_controls_list_is_the_keymap() {
        let keymap = Keymap::default();
        let listed: Vec<Action> = controls(&keymap).into_iter().flat_map(|(_, rows)| rows.into_iter().map(|(a, ..)| a)).collect();
        let mut bound: Vec<Action> = keymap.bindings.iter().map(|b| b.action).collect();
        bound.sort_by_key(|a| Group::ALL.iter().position(|&g| keymap.binding(*a).unwrap().group == g));
        assert_eq!(listed, bound, "every binding, once, by group");
        let turn = controls(&keymap).into_iter().flat_map(|(_, rows)| rows).find(|(a, ..)| *a == Action::TurnItem).unwrap();
        assert_eq!(turn.2, "Right click");

        // And the page shows exactly those lines, each with its keys.
        let mut app = menu_app();
        press(&mut app, KeyCode::Escape);
        choose(&mut app, MenuCommand::Show(MenuPage::Controls));
        app.update();
        let mut rows = app.world_mut().query::<(&ControlRow, &Children)>();
        let shown: Vec<(Action, Vec<String>)> = rows
            .iter(app.world())
            .map(|(row, children)| (row.0, children.iter().filter_map(|c| app.world().get::<Text>(c)).map(|t| t.0.clone()).collect()))
            .collect();
        let wanted: Vec<(Action, Vec<String>)> =
            controls(&keymap).into_iter().flat_map(|(_, rows)| rows).map(|(a, name, keys)| (a, vec![name.to_string(), keys])).collect();
        assert_eq!(shown, wanted);
    }
}
