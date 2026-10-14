//! The pause menu: Esc, with no other screen open, brings up a dark page over the level with
//! Resume, Controls, Settings and Quit. While it is open the body stands still, the mouse is
//! free and the level's clock stops ([`Time<Virtual>`]); the music plays on.
//!
//! Controls lists every binding in the [`Keymap`], by group, so it always shows the keys the
//! game reads. `FB_OPEN_MENU=main|controls|settings` opens the menu at the start, on that
//! page, for captures.

use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};

use crate::carry::ViewModelCamera;
use crate::keymap::{Action, ActionInput, Group, Keymap};
use crate::play::{Screen, ScreenOpen, show_screen, switch_screens};
use crate::ui::{BG, BRIGHT, FAINT, INK, MUTED, UiFonts, node_rect};

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
}

/// The whole menu, over the level.
#[derive(Component, Debug)]
pub struct MenuScreen;

/// A button on the menu, and what it does.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuButton(pub MenuCommand);

/// A line of the controls list, for the binding of this action.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlRow(pub Action);

/// The menu's width in logical pixels.
const MENU_PX: f32 = 640.0;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Keymap>()
            .init_resource::<UiFonts>()
            .init_resource::<ScreenOpen>()
            .init_resource::<MenuPage>()
            .add_message::<MenuCommand>()
            .add_systems(PostStartup, open_for_capture.run_if(|| capture_page().is_some()))
            .add_systems(Update, (choose_buttons, apply_commands, pause_while_open, build_menu, light_hovered).chain().after(switch_screens));
    }
}

/// The page `FB_OPEN_MENU` asks for.
fn capture_page() -> Option<MenuPage> {
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

/// A click on a button sends its [`MenuCommand`].
fn choose_buttons(
    input: ActionInput,
    open: Res<ScreenOpen>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    buttons: Query<(&MenuButton, &ComputedNode, &UiGlobalTransform)>,
    mut chosen: MessageWriter<MenuCommand>,
) {
    if open.is(Screen::Menu)
        && input.just_pressed(Action::Choose)
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
        }
    }
}

/// One line of the controls list: an action, its name and its keys ("R or Right click").
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
                .with_children(|menu| {
                    menu.spawn((Text::new("Forever Below"), UiFonts::style(&fonts.display, 54.0, INK)));
                    menu.spawn((Node { width: percent(100), height: px(1), margin: UiRect::bottom(px(10)), ..default() }, BackgroundColor(FAINT)));
                    match *page {
                        MenuPage::Main => main_page(menu, &fonts),
                        MenuPage::Controls => controls_page(menu, &fonts, &keymap),
                        MenuPage::Settings => settings_page(menu, &fonts),
                    }
                });
        });
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

fn settings_page(menu: &mut ChildSpawnerCommands, fonts: &UiFonts) {
    heading(menu, fonts, "settings");
    menu.spawn((Text::new("Nothing to set here yet. Minus and equals turn the music down and up."), UiFonts::style(&fonts.body, 16.0, MUTED)));
    back_button(menu, fonts);
}

/// Brightens the button under the mouse.
fn light_hovered(
    open: Res<ScreenOpen>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    buttons: Query<(&MenuButton, &ComputedNode, &UiGlobalTransform, &Children)>,
    mut colours: Query<&mut TextColor>,
) {
    if !open.is(Screen::Menu) {
        return;
    }
    let over = hovered_button(window.as_deref().copied(), buttons.iter().map(|(b, n, t, _)| (b, n, t)));
    for (button, _, _, children) in &buttons {
        let colour = if over == Some(button.0) { INK } else { MUTED };
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
        assert_eq!(turn.2, "R or Right click");

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
