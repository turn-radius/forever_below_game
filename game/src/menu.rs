//! The pause menu: Esc, with no other screen open, brings up a dark page over the level with
//! Resume, Controls, Settings and Quit. While it is open the body stands still, the mouse is
//! free and the level's clock stops ([`Time<Virtual>`]); the music plays on.
//!
//! Controls lists every binding in the [`Keymap`], one group at a time, so it always shows the
//! keys the game reads, and rebinds them. An action has up to [`MAX_SLOTS`] key slots, each a
//! button: click it, then press the key or mouse button to bind (Esc cancels; the click that
//! began the capture is not taken, and no action fires meanwhile, [`InputCapture`]). A key
//! another action uses in the same place ([`Keymap::clash`]) asks "F is Primary action. Swap?"
//! and a swap gives the other action the slot's old key. Each row and the whole list reset to
//! the defaults, Crouch has a Hold or Toggle choice, and every change is saved
//! ([`KeybindingsFile`]). Looking round, the quickslot digits and the menu click are listed but
//! not rebindable: the mouse look is not a key, the ten digits are one binding, and the click is
//! how the menu itself is used. Settings has a slider or a toggle for each [`Setting`]; a change applies the
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
use crate::keybindings::KeybindingsFile;
use crate::keymap::{Action, ActionInput, Binding, CrouchMode, Group, Input, InputCapture, Keymap};
use crate::launch::AppState;
use crate::play::{DelveSystems, Screen, ScreenOpen, show_screen, switch_screens};
use crate::settings::{GameSettings, Setting};
use crate::ui::{BG, BRIGHT, CELL, FAINT, INK, LINE, MUTED, UiFonts, node_rect, ui_cursor};

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
    /// Opens the character creator, from the title ([`crate::title`] carries it out).
    NewDelve,
    /// Begins a new delve as the last delver, from the title ([`crate::title::last_delver`]).
    Continue,
    /// A button of the character creator ([`crate::title::creator`]).
    Creator(crate::title::creator::CreatorCommand),
    /// Waits for the key or button to put in this slot of the action.
    Capture(Action, usize),
    /// Puts the action back on its default keys.
    ResetKeys(Action),
    /// Puts every action back on its default keys.
    ResetAllKeys,
    /// Crouch held or toggled, the other way.
    ToggleCrouch,
    /// Answers the clash prompt: swap the two actions' keys.
    SwapKeys,
    /// Answers the clash prompt: leave the keys as they were.
    KeepKeys,
    /// Shows a group of the controls list.
    ShowGroup(Group),
}

/// How many key slots an action shows.
pub const MAX_SLOTS: usize = 2;

/// Where the options menu is in rebinding a key.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rebinding {
    #[default]
    Idle,
    /// Waiting for a key or button for `slot` of `action`. `fresh` skips the frame of the
    /// click that began it.
    Capturing { action: Action, slot: usize, fresh: bool },
    /// `input` is in use by `other`; waiting for Swap or Cancel. `swappable` is false when
    /// the swap would leave `other` with no key, or either action sharing a key with another
    /// in some context ([`Keymap::can_swap`]); only Cancel is offered then.
    Asking { action: Action, slot: usize, input: Input, other: Action, swappable: bool },
}

/// The group the controls page shows.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlsGroup(pub Group);

impl Default for ControlsGroup {
    fn default() -> Self {
        ControlsGroup(Group::ALL[0])
    }
}

/// What the controls page shows beyond the keymap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlsView {
    pub group: Group,
    pub rebinding: Rebinding,
}

impl Default for ControlsView {
    fn default() -> Self {
        ControlsView { group: ControlsGroup::default().0, rebinding: Rebinding::Idle }
    }
}

/// Whether the controls page lets the player rebind `action`.
pub fn is_rebindable(action: Action) -> bool {
    !matches!(action, Action::Look | Action::Quickslot | Action::Choose)
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
        app.add_plugins(crate::keymap::KeymapPlugin)
            .init_resource::<UiFonts>()
            // Kept as loaded if `SettingsPlugin` came first.
            .init_resource::<GameSettings>()
            .init_resource::<ScreenOpen>()
            .init_resource::<MenuPage>()
            .init_resource::<Rebinding>()
            .init_resource::<ControlsGroup>()
            .add_message::<MenuCommand>()
            .add_systems(PostStartup, open_for_capture.run_if(in_state(AppState::Delving)).run_if(|| capture_page().is_some()))
            .add_systems(
                Update,
                (
                    choose_buttons,
                    adjust_settings,
                    apply_commands,
                    apply_rebinds,
                    capture_input,
                    sync_capture,
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

/// The button under `cursor`, in the interface's pixels ([`ui_cursor`]), if any.
pub(crate) fn hovered_button<'a>(
    cursor: Option<Vec2>,
    buttons: impl IntoIterator<Item = (&'a MenuButton, &'a ComputedNode, &'a UiGlobalTransform)>,
) -> Option<MenuCommand> {
    let point = cursor?;
    buttons.into_iter().find(|(_, node, transform)| node_rect(node, transform).contains(point)).map(|(button, ..)| button.0)
}

/// A click on a button sends its [`MenuCommand`]. Buttons exist only while a menu shows.
fn choose_buttons(
    input: ActionInput,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    buttons: Query<(&MenuButton, &ComputedNode, &UiGlobalTransform)>,
    mut chosen: MessageWriter<MenuCommand>,
) {
    if input.just_pressed(Action::Choose)
        && let Some(command) = hovered_button(window.and_then(|w| ui_cursor(&w, ui_scale.as_deref())), buttons.iter())
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
            // The title carries out NewDelve; [`apply_rebinds`] the rebinding commands.
            _ => {}
        }
    }
}

/// Writes the keymap to the keybindings file, if there is one.
fn save_keymap(keymap: &Keymap, file: Option<&KeybindingsFile>) {
    if let Some(path) = file.and_then(|f| f.0.as_deref())
        && let Err(error) = keymap.save(path)
    {
        warn!("keybindings: could not save {}: {error}", path.display());
    }
}

/// Carries out the controls page's buttons, saving the keymap after each change.
fn apply_rebinds(
    mut chosen: MessageReader<MenuCommand>,
    mut rebinding: ResMut<Rebinding>,
    mut group: ResMut<ControlsGroup>,
    mut keymap: ResMut<Keymap>,
    file: Option<Res<KeybindingsFile>>,
) {
    for command in chosen.read() {
        match *command {
            MenuCommand::Capture(action, slot) => *rebinding = Rebinding::Capturing { action, slot, fresh: true },
            MenuCommand::ShowGroup(shown) => {
                group.0 = shown;
                *rebinding = Rebinding::Idle;
            }
            MenuCommand::Show(_) | MenuCommand::Resume | MenuCommand::KeepKeys => *rebinding = Rebinding::Idle,
            // Each of these cancels an open capture or swap prompt.
            MenuCommand::ResetKeys(action) => {
                keymap.reset(action);
                save_keymap(&keymap, file.as_deref());
                *rebinding = Rebinding::Idle;
            }
            MenuCommand::ResetAllKeys => {
                keymap.reset_all();
                save_keymap(&keymap, file.as_deref());
                *rebinding = Rebinding::Idle;
            }
            MenuCommand::ToggleCrouch => {
                keymap.crouch = match keymap.crouch {
                    CrouchMode::Hold => CrouchMode::Toggle,
                    CrouchMode::Toggle => CrouchMode::Hold,
                };
                save_keymap(&keymap, file.as_deref());
                *rebinding = Rebinding::Idle;
            }
            MenuCommand::SwapKeys => {
                if let Rebinding::Asking { action, slot, input, other, swappable: true } = *rebinding {
                    keymap.swap(action, slot, input, other);
                    save_keymap(&keymap, file.as_deref());
                    *rebinding = Rebinding::Idle;
                }
            }
            MenuCommand::Quit | MenuCommand::NewDelve | MenuCommand::Continue | MenuCommand::Creator(_) => {}
        }
    }
}

/// While a slot waits for its key, takes the next key or mouse button pressed: Esc cancels,
/// a free key is bound, and one another action uses asks to swap. The frame of the click that
/// began the capture is skipped.
fn capture_input(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    mut rebinding: ResMut<Rebinding>,
    mut keymap: ResMut<Keymap>,
    file: Option<Res<KeybindingsFile>>,
    inspector: Option<Res<crate::inspect::InspectorOpen>>,
) {
    let Rebinding::Capturing { action, slot, fresh } = *rebinding else { return };
    // The debug inspector has the keyboard while it is open.
    if inspector.is_some_and(|i| i.0) {
        return;
    }
    if fresh {
        *rebinding = Rebinding::Capturing { action, slot, fresh: false };
        return;
    }
    if keys.as_deref().is_some_and(|k| k.just_pressed(KeyCode::Escape)) {
        *rebinding = Rebinding::Idle;
        return;
    }
    let pressed = keys
        .as_deref()
        .and_then(|k| k.get_just_pressed().next().copied().map(Input::Key))
        .or_else(|| mouse.as_deref().and_then(|m| m.get_just_pressed().next().copied().map(Input::Mouse)));
    let Some(input) = pressed else { return };
    let Some(binding) = keymap.binding(action) else {
        *rebinding = Rebinding::Idle;
        return;
    };
    // Already one of this action's keys: in the other slot, the two slots trade places.
    let sides = input.sides();
    if let Some(held) = binding.slots().iter().position(|s| s.iter().any(|i| sides.contains(i))) {
        if held != slot {
            keymap.exchange_slots(action, slot, held);
            save_keymap(&keymap, file.as_deref());
        }
        *rebinding = Rebinding::Idle;
        return;
    }
    match keymap.clash_all(action, input).first().copied() {
        None => {
            keymap.set_slot(action, slot, input);
            save_keymap(&keymap, file.as_deref());
            *rebinding = Rebinding::Idle;
        }
        Some(other) => {
            let swappable = keymap.can_swap(action, slot, input, other);
            *rebinding = Rebinding::Asking { action, slot, input, other, swappable };
        }
    }
}

/// Lets [`ActionInput`] know a key is being captured, and drops any rebinding when the menu
/// closes.
fn sync_capture(open: Res<ScreenOpen>, mut rebinding: ResMut<Rebinding>, mut capture: ResMut<InputCapture>) {
    if !open.is(Screen::Menu) {
        rebinding.set_if_neq(Rebinding::Idle);
    }
    capture.set_if_neq(InputCapture(matches!(*rebinding, Rebinding::Capturing { .. })));
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
    group: Res<ControlsGroup>,
    rebinding: Res<Rebinding>,
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
    if existing.is_some() && !open.is_changed() && !page.is_changed() && !keymap.is_changed() && !group.is_changed() && !rebinding.is_changed() {
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
                .with_children(|menu| menu_page(menu, &fonts, *page, &keymap, &settings, &ControlsView { group: group.0, rebinding: *rebinding }));
        });
}

/// A page of the menu under the game's name and a rule, into a column `MENU_PX` wide.
pub fn menu_page(menu: &mut ChildSpawnerCommands, fonts: &UiFonts, page: MenuPage, keymap: &Keymap, settings: &GameSettings, controls: &ControlsView) {
    menu.spawn((Text::new("Forever Below"), UiFonts::style(&fonts.display, 54.0, INK)));
    menu.spawn((Node { width: percent(100), height: px(1), margin: UiRect::bottom(px(10)), ..default() }, BackgroundColor(FAINT)));
    match page {
        MenuPage::Main => main_page(menu, fonts),
        MenuPage::Controls => controls_page(menu, fonts, keymap, controls),
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

/// A small bordered button for a key slot or a choice on the controls page: `ink` at rest,
/// cream under the mouse.
fn key_button(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, command: MenuCommand, width: f32, ink: Color) {
    parent
        .spawn((
            MenuButton(command),
            ButtonInk { idle: ink, hover: INK },
            Node {
                width: px(width),
                padding: UiRect::axes(px(6), px(2)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(3)),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(CELL),
            BorderColor::all(LINE),
        ))
        .with_children(|b| {
            b.spawn((Text::new(label), UiFonts::style(&fonts.mono, 14.0, ink)));
        });
}

/// A slot's width in logical pixels.
const SLOT_PX: f32 = 110.0;

/// One line of the controls page: the action's name, its key slots as buttons (or its keys as
/// text when they can't be rebound) and, once changed, a Reset.
fn control_row(list: &mut ChildSpawnerCommands, fonts: &UiFonts, binding: &Binding, view: &ControlsView) {
    list.spawn((
        ControlRow(binding.action),
        Node { align_items: AlignItems::Center, column_gap: px(8), border: UiRect::bottom(px(1)), padding: UiRect::bottom(px(3)), ..default() },
        BorderColor::all(FAINT),
    ))
    .with_children(|row| {
        row.spawn((Text::new(binding.name), UiFonts::style(&fonts.body, 16.0, MUTED), Node { flex_grow: 1.0, flex_basis: px(0), ..default() }));
        if !is_rebindable(binding.action) {
            row.spawn((Text::new(binding.labels().join(" or ")), UiFonts::style(&fonts.mono, 14.0, BRIGHT), Node { flex_shrink: 0.0, ..default() }));
            return;
        }
        let labels: Vec<String> = binding.slots().iter().map(|inputs| inputs[0].label()).collect();
        for slot in 0..MAX_SLOTS {
            let waiting = matches!(view.rebinding, Rebinding::Capturing { action, slot: s, .. } if action == binding.action && s == slot);
            let text = if waiting { "Press a key..." } else { labels.get(slot).map_or("none", String::as_str) };
            let ink = if waiting { INK } else if slot < labels.len() { BRIGHT } else { MUTED };
            key_button(row, fonts, text, MenuCommand::Capture(binding.action, slot), SLOT_PX, ink);
        }
        if Keymap::default().binding(binding.action).is_some_and(|d| d.inputs != binding.inputs) {
            key_button(row, fonts, "Reset", MenuCommand::ResetKeys(binding.action), 52.0, MUTED);
        } else {
            row.spawn(Node { width: px(52), ..default() });
        }
    });
}

fn controls_page(menu: &mut ChildSpawnerCommands, fonts: &UiFonts, keymap: &Keymap, view: &ControlsView) {
    heading(menu, fonts, "controls");
    menu.spawn(Node { width: percent(100), column_gap: px(8), ..default() }).with_children(|tabs| {
        for group in Group::ALL {
            let ink = if group == view.group { INK } else { MUTED };
            key_button(tabs, fonts, group.name(), MenuCommand::ShowGroup(group), 96.0, ink);
        }
    });
    menu.spawn(Node { flex_direction: FlexDirection::Column, width: percent(100), row_gap: px(6), ..default() }).with_children(|list| {
        for binding in keymap.in_group(view.group) {
            control_row(list, fonts, binding, view);
        }
        if view.group == Group::Moving {
            let mode = match keymap.crouch {
                CrouchMode::Hold => "Hold",
                CrouchMode::Toggle => "Toggle",
            };
            list.spawn((Node { align_items: AlignItems::Center, column_gap: px(8), border: UiRect::bottom(px(1)), padding: UiRect::bottom(px(3)), ..default() }, BorderColor::all(FAINT)))
                .with_children(|row| {
                    row.spawn((Text::new("Crouch while the key is"), UiFonts::style(&fonts.body, 16.0, MUTED), Node { flex_grow: 1.0, flex_basis: px(0), ..default() }));
                    key_button(row, fonts, mode, MenuCommand::ToggleCrouch, SLOT_PX, BRIGHT);
                    row.spawn(Node { width: px(52), ..default() });
                });
        }
    });
    match view.rebinding {
        Rebinding::Asking { input, other, swappable, .. } => {
            let name = keymap.binding(other).map_or("another action", |b| b.name);
            let question = if swappable { format!("{} is {}. Swap?", input.label(), name) } else { format!("{} is {}, and can't be swapped.", input.label(), name) };
            menu.spawn(Node { align_items: AlignItems::Center, column_gap: px(14), ..default() }).with_children(|ask| {
                ask.spawn((Text::new(question), UiFonts::style(&fonts.body_italic, 17.0, INK)));
                if swappable {
                    button(ask, fonts, "Swap", MenuCommand::SwapKeys, 20.0);
                }
                button(ask, fonts, "Cancel", MenuCommand::KeepKeys, 20.0);
            });
        }
        Rebinding::Capturing { .. } => {
            menu.spawn((Text::new("Press a key or button. Esc cancels."), UiFonts::style(&fonts.body_italic, 15.0, MUTED)));
        }
        Rebinding::Idle => {}
    }
    menu.spawn(Node { column_gap: px(24), margin: UiRect::top(px(8)), ..default() }).with_children(|row| {
        button(row, fonts, "Reset all", MenuCommand::ResetAllKeys, 20.0);
        button(row, fonts, "Back", MenuCommand::Show(MenuPage::Main), 20.0);
    });
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
    ui_scale: Option<Res<UiScale>>,
    controls: Query<(&SettingControl, &ComputedNode, &UiGlobalTransform)>,
    mut held: Local<Option<Setting>>,
    mut settings: ResMut<GameSettings>,
) {
    if *page != MenuPage::Settings || controls.is_empty() || !input.pressed(Action::Choose) {
        *held = None;
        return;
    }
    let Some(point) = window.and_then(|w| ui_cursor(&w, ui_scale.as_deref())) else { return };
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
pub(crate) fn light_hovered(
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    buttons: Query<(&MenuButton, &ButtonInk, &ComputedNode, &UiGlobalTransform, &Children)>,
    mut colours: Query<&mut TextColor>,
) {
    let over = hovered_button(window.and_then(|w| ui_cursor(&w, ui_scale.as_deref())), buttons.iter().map(|(b, _, n, t, _)| (b, n, t)));
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
        assert_eq!(turn.2, "R or Right click");

        // And the page shows exactly those lines, a group at a time, each with its keys.
        let mut app = menu_app();
        press(&mut app, KeyCode::Escape);
        choose(&mut app, MenuCommand::Show(MenuPage::Controls));
        for (group, lines) in controls(&keymap) {
            choose(&mut app, MenuCommand::ShowGroup(group));
            app.update();
            let mut rows = app.world_mut().query::<(&ControlRow, &Children)>();
            let shown: Vec<Action> = rows.iter(app.world()).map(|(row, _)| row.0).collect();
            assert_eq!(shown, lines.iter().map(|l| l.0).collect::<Vec<_>>(), "{group:?}");
            let names: Vec<String> = rows.iter(app.world()).filter_map(|(_, c)| app.world().get::<Text>(c[0]).map(|t| t.0.clone())).collect();
            assert_eq!(names, lines.iter().map(|l| l.1.to_string()).collect::<Vec<_>>());
        }
        // The unrebindable lines show their keys as text, the rest a button per slot.
        let slots = |app: &mut App| {
            let mut buttons = app.world_mut().query::<&MenuButton>();
            buttons.iter(app.world()).filter(|b| matches!(b.0, MenuCommand::Capture(..))).count()
        };
        choose(&mut app, MenuCommand::ShowGroup(Group::Moving));
        app.update();
        assert_eq!(slots(&mut app), 2 * 10, "two slots for each of the ten rebindable moving actions");
    }

    fn rebind_app(name: &str) -> (App, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("fb_menu_{name}_{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut app = menu_app();
        app.insert_resource(KeybindingsFile(Some(path.clone())));
        press(&mut app, KeyCode::Escape);
        choose(&mut app, MenuCommand::Show(MenuPage::Controls));
        (app, path)
    }

    fn keymap(app: &App) -> &Keymap {
        app.world().resource::<Keymap>()
    }

    fn inputs(app: &App, action: Action) -> Vec<Input> {
        keymap(app).inputs(action).to_vec()
    }

    #[test]
    fn clicking_a_slot_captures_the_next_key_and_saves_it() {
        let (mut app, path) = rebind_app("capture");
        choose(&mut app, MenuCommand::Capture(Action::Jump, 0));
        assert!(matches!(*app.world().resource::<Rebinding>(), Rebinding::Capturing { action: Action::Jump, slot: 0, .. }));
        assert!(app.world().resource::<InputCapture>().0, "actions stand still while a key is awaited");
        // The page says so.
        let mut texts = app.world_mut().query::<&Text>();
        assert!(texts.iter(app.world()).any(|t| t.0 == "Press a key..."));
        press(&mut app, KeyCode::KeyB);
        assert_eq!(inputs(&app, Action::Jump), vec![Input::Key(KeyCode::KeyB)]);
        assert_eq!(*app.world().resource::<Rebinding>(), Rebinding::Idle);
        assert!(!app.world().resource::<InputCapture>().0);
        assert_eq!(Keymap::load(&path).inputs(Action::Jump), &[Input::Key(KeyCode::KeyB)], "saved");
        // A second slot adds a key.
        choose(&mut app, MenuCommand::Capture(Action::Jump, 1));
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Middle);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        assert_eq!(inputs(&app, Action::Jump), vec![Input::Key(KeyCode::KeyB), Input::Mouse(MouseButton::Middle)]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_click_that_starts_a_capture_is_not_the_key() {
        let (mut app, path) = rebind_app("fresh");
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        choose(&mut app, MenuCommand::Capture(Action::Jump, 0));
        assert_eq!(inputs(&app, Action::Jump), vec![Input::Key(KeyCode::Space)], "same frame");
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.update();
        assert_eq!(inputs(&app, Action::Jump), vec![Input::Key(KeyCode::Space)], "the click is still down, not newly pressed");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn escape_cancels_a_capture_and_leaves_the_menu_open() {
        let (mut app, path) = rebind_app("cancel");
        choose(&mut app, MenuCommand::Capture(Action::Jump, 0));
        press(&mut app, KeyCode::Escape);
        assert_eq!(*app.world().resource::<Rebinding>(), Rebinding::Idle);
        assert_eq!(inputs(&app, Action::Jump), vec![Input::Key(KeyCode::Space)]);
        assert_eq!(open(&app), Some(Screen::Menu), "Esc did not also go back");
        assert!(!path.exists(), "nothing changed, nothing saved");
        press(&mut app, KeyCode::Escape);
        assert_eq!(open(&app), None, "and Esc works as usual again");
    }

    #[test]
    fn a_clash_offers_a_swap_and_the_swap_swaps() {
        let (mut app, path) = rebind_app("swap");
        choose(&mut app, MenuCommand::Capture(Action::Jump, 0));
        press(&mut app, KeyCode::KeyF);
        assert!(matches!(*app.world().resource::<Rebinding>(), Rebinding::Asking { other: Action::Primary, swappable: true, .. }));
        assert_eq!(inputs(&app, Action::Jump), vec![Input::Key(KeyCode::Space)], "nothing changes until it is answered");
        let mut texts = app.world_mut().query::<&Text>();
        assert!(texts.iter(app.world()).any(|t| t.0 == "F is Primary action. Swap?"));
        choose(&mut app, MenuCommand::SwapKeys);
        assert_eq!(inputs(&app, Action::Jump), vec![Input::Key(KeyCode::KeyF)]);
        assert_eq!(inputs(&app, Action::Primary), vec![Input::Key(KeyCode::Space)]);
        assert_eq!(Keymap::load(&path).inputs(Action::Primary), &[Input::Key(KeyCode::Space)]);
        // Declining leaves things be.
        choose(&mut app, MenuCommand::Capture(Action::Jump, 0));
        press(&mut app, KeyCode::KeyG);
        choose(&mut app, MenuCommand::KeepKeys);
        assert_eq!(inputs(&app, Action::Jump), vec![Input::Key(KeyCode::KeyF)]);
        assert_eq!(*app.world().resource::<Rebinding>(), Rebinding::Idle);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_menu_is_stricter_than_the_game_about_a_press_on_a_held_input() {
        let mut keymap = Keymap::default();
        assert_eq!(keymap.clash(Action::Jump, Input::Mouse(MouseButton::Right)), Some(Action::Block));
        assert_eq!(keymap.clash(Action::Stab, Input::Mouse(MouseButton::Left)), None, "Attack and Stab share the click by design");
        assert_eq!(keymap.clash(Action::Jump, Input::Mouse(MouseButton::Left)), Some(Action::Attack));
        // With Attack moved off it, only Stab holds left click: the game lets Jump press it too,
        // the menu does not.
        keymap.set_slot(Action::Attack, 0, Input::Key(KeyCode::KeyH));
        assert_eq!(keymap.conflicts(Action::Jump, Input::Mouse(MouseButton::Left)), None);
        assert_eq!(keymap.clash(Action::Jump, Input::Mouse(MouseButton::Left)), Some(Action::Stab));
        assert_eq!(keymap.clash(Action::Jump, Input::Key(KeyCode::KeyB)), None);
    }

    #[test]
    fn a_swap_that_would_leave_an_action_keyless_is_not_offered() {
        let (mut app, path) = rebind_app("keyless");
        // Jump's third slot is empty, and Primary has only F.
        choose(&mut app, MenuCommand::Capture(Action::Jump, 1));
        press(&mut app, KeyCode::KeyF);
        assert!(matches!(*app.world().resource::<Rebinding>(), Rebinding::Asking { swappable: false, .. }));
        choose(&mut app, MenuCommand::SwapKeys);
        assert_eq!(inputs(&app, Action::Primary), vec![Input::Key(KeyCode::KeyF)]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rows_and_the_whole_list_reset() {
        let (mut app, path) = rebind_app("reset");
        for (action, key) in [(Action::Jump, KeyCode::KeyB), (Action::Crouch, KeyCode::KeyN)] {
            choose(&mut app, MenuCommand::Capture(action, 0));
            press(&mut app, key);
        }
        assert_eq!(inputs(&app, Action::Jump), vec![Input::Key(KeyCode::KeyB)]);
        choose(&mut app, MenuCommand::ResetKeys(Action::Jump));
        assert_eq!(inputs(&app, Action::Jump), vec![Input::Key(KeyCode::Space)]);
        assert_eq!(inputs(&app, Action::Crouch), vec![Input::Key(KeyCode::KeyN)], "only the row; the slot held both ctrl keys");
        assert_eq!(Keymap::load(&path).inputs(Action::Jump), &[Input::Key(KeyCode::Space)]);
        choose(&mut app, MenuCommand::ToggleCrouch);
        choose(&mut app, MenuCommand::ResetAllKeys);
        assert_eq!(keymap(&app).bindings, Keymap::default().bindings);
        assert_eq!(keymap(&app).crouch, CrouchMode::Toggle, "the crouch mode is not a key");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn resetting_a_row_takes_back_a_key_another_action_moved_onto() {
        let mut keymap = Keymap::default();
        keymap.swap(Action::Jump, 0, Input::Key(KeyCode::KeyF), Action::Primary);
        keymap.reset(Action::Primary);
        assert_eq!(keymap, Keymap::default(), "Jump went back to Space, so F is free for Primary");
    }

    #[test]
    fn crouch_has_a_hold_or_toggle_row() {
        let (mut app, path) = rebind_app("crouch");
        assert_eq!(keymap(&app).crouch, CrouchMode::Hold);
        choose(&mut app, MenuCommand::ToggleCrouch);
        assert_eq!(keymap(&app).crouch, CrouchMode::Toggle);
        assert!(std::fs::read_to_string(&path).unwrap().contains("toggle"), "saved");
        choose(&mut app, MenuCommand::ToggleCrouch);
        assert_eq!(keymap(&app).crouch, CrouchMode::Hold);
        let _ = std::fs::remove_file(&path);
    }

    /// No two actions on one input in a context they share (but for the designed pairs).
    fn assert_no_shared_inputs(keymap: &Keymap) {
        for binding in &keymap.bindings {
            for &input in &binding.inputs {
                assert_eq!(keymap.clash_all(binding.action, input), vec![], "{:?} on {input:?}", binding.action);
            }
        }
    }

    #[test]
    fn a_swap_across_contexts_is_refused_rather_than_doubling_up_keys() {
        assert_no_shared_inputs(&Keymap::default());
        let (mut app, path) = rebind_app("contexts");
        // DragItem (inventory) on F clashes with Primary (level and inventory); swapping would put
        // Primary on the left click in the level, where Attack is.
        choose(&mut app, MenuCommand::Capture(Action::DragItem, 0));
        press(&mut app, KeyCode::KeyF);
        assert!(matches!(*app.world().resource::<Rebinding>(), Rebinding::Asking { other: Action::Primary, swappable: false, .. }));
        choose(&mut app, MenuCommand::SwapKeys);
        assert_eq!(*keymap(&app), Keymap::default());
        choose(&mut app, MenuCommand::KeepKeys);
        // Primary onto the left click clashes with Attack and DragItem both.
        choose(&mut app, MenuCommand::Capture(Action::Primary, 0));
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        assert!(matches!(*app.world().resource::<Rebinding>(), Rebinding::Asking { swappable: false, .. }));
        assert_eq!(keymap(&app).clash_all(Action::Primary, Input::Mouse(MouseButton::Left)), vec![Action::Attack, Action::Stab, Action::DragItem]);
        choose(&mut app, MenuCommand::SwapKeys);
        assert_no_shared_inputs(keymap(&app));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_shift_slot_moves_whole() {
        let (mut app, path) = rebind_app("shift");
        choose(&mut app, MenuCommand::Capture(Action::Jump, 0));
        press(&mut app, KeyCode::ShiftLeft);
        assert!(matches!(*app.world().resource::<Rebinding>(), Rebinding::Asking { other: Action::Sprint, swappable: true, .. }));
        choose(&mut app, MenuCommand::SwapKeys);
        let shifts = vec![Input::Key(KeyCode::ShiftLeft), Input::Key(KeyCode::ShiftRight)];
        assert_eq!(inputs(&app, Action::Jump), shifts, "both sides of the modifier");
        assert_eq!(inputs(&app, Action::Sprint), vec![Input::Key(KeyCode::Space)], "and Sprint keeps neither");
        assert_no_shared_inputs(keymap(&app));
        // Right shift names the same slot.
        choose(&mut app, MenuCommand::Capture(Action::Jump, 0));
        press(&mut app, KeyCode::ShiftRight);
        assert_eq!(inputs(&app, Action::Jump), shifts, "already its key");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_key_in_the_other_slot_trades_places() {
        let (mut app, path) = rebind_app("trade");
        choose(&mut app, MenuCommand::Capture(Action::Jump, 1));
        press(&mut app, KeyCode::KeyB);
        choose(&mut app, MenuCommand::Capture(Action::Jump, 0));
        press(&mut app, KeyCode::KeyB);
        assert_eq!(inputs(&app, Action::Jump), vec![Input::Key(KeyCode::KeyB), Input::Key(KeyCode::Space)]);
        assert_eq!(Keymap::load(&path).inputs(Action::Jump), &[Input::Key(KeyCode::KeyB), Input::Key(KeyCode::Space)]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn resets_and_the_crouch_choice_cancel_an_open_prompt() {
        let (mut app, path) = rebind_app("cancel_prompt");
        for command in [MenuCommand::ResetKeys(Action::Jump), MenuCommand::ResetAllKeys, MenuCommand::ToggleCrouch] {
            choose(&mut app, MenuCommand::Capture(Action::Jump, 0));
            choose(&mut app, command);
            assert_eq!(*app.world().resource::<Rebinding>(), Rebinding::Idle, "{command:?} over a capture");
            choose(&mut app, MenuCommand::Capture(Action::Jump, 0));
            press(&mut app, KeyCode::KeyF);
            assert!(matches!(*app.world().resource::<Rebinding>(), Rebinding::Asking { .. }));
            choose(&mut app, command);
            assert_eq!(*app.world().resource::<Rebinding>(), Rebinding::Idle, "{command:?} over a prompt");
        }
        let _ = std::fs::remove_file(&path);
    }
}
