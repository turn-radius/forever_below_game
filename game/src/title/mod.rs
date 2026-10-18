//! The title screen: the title card's sunlit valley, the game's name in striped gold, and New
//! delve, Settings and Quit.
//!
//! The valley is the user's concept art (`reference/concept/forever-below-title-card.jpg`),
//! standing in until there is a rendered scene. It is cropped above the card's own lettering
//! (`assets/ui/title-card.png`) and drawn to cover the screen without stretching
//! ([`cover_rect`]). The lettering is real text in Almendra Bold, laid out as on the card (a
//! tall F beside both lines) by an offscreen camera into an image, which [`LetteringMaterial`]
//! rules in gold bands with a solid rim, as the card's letters are.
//!
//! New delve rolls a level from the clock (or `--seed`'s, with `--title`) and enters
//! [`AppState::Delving`], where the level, the player and their character are made
//! (`OnEnter(AppState::Delving)`). Settings is the pause menu's own page ([`menu_page`]), and
//! the buttons are the pause menu's [`MenuButton`]s, so choosing and hovering work as there.

pub mod creator;
pub mod last_delver;

use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType, TextureFormat},
    shader::{ShaderRef, load_shader_library},
};

use crate::keymap::{Action, ActionInput, Keymap};
use crate::launch::AppState;
use crate::menu::{ButtonInk, MENU_PX, MenuButton, MenuCommand, MenuPage, capture_page, menu_page};
use crate::settings::GameSettings;
use crate::ui::{BG, FRAME_GOLD, INK, LINE, UiFonts};
use creator::{CreatorCommand, Draft, creator_page, open_creator, run_creator};
use last_delver::{LastDelver, LastDelverPlugin};

const SHADER_PATH: &str = "embedded://forever_below/title/lettering.wesl";

/// The title card, cropped above its own lettering.
pub const CARD_PATH: &str = "ui/title-card.png";

/// The lettering image's size in pixels. Its aspect is the lettering node's.
const LETTERING_SIZE: UVec2 = UVec2::new(1800, 860);

/// The title's gold, matched by eye to the card's letters.
pub const LETTERING_GOLD: Color = Color::srgb(0.96, 0.77, 0.43);

/// The seed New delve plays; a fresh one from the clock when `None`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NewDelveSeed(pub Option<u32>);

/// The title card image, sized to cover the screen.
#[derive(Component, Debug)]
pub struct TitleCard;

/// The striped lettering.
#[derive(Component, Debug)]
pub struct TitleLettering;

/// The buttons, or the settings page, whichever the [`MenuPage`] shows.
#[derive(Component, Debug)]
pub struct TitlePanel;

/// The root of the title screen, which the panel is built under.
#[derive(Component, Debug)]
pub struct TitleScreen;

/// Gold bands across white text drawn into `glyphs` (`lettering.wesl`).
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct LetteringMaterial {
    #[uniform(0)]
    pub style: LetteringStyle,
    #[texture(1)]
    #[sampler(2)]
    pub glyphs: Handle<Image>,
}

/// How the lettering is ruled. Lengths are in texels of the lettering image.
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct LetteringStyle {
    pub gold: LinearRgba,
    pub shade: LinearRgba,
    pub shadow: LinearRgba,
    pub band_period: f32,
    pub band_fill: f32,
    pub rim: f32,
    pub shadow_blur: f32,
    pub shadow_offset: Vec2,
}

impl Default for LetteringStyle {
    /// The card's proportions: about eleven bands to the x-height, each a little under half
    /// gold, and a rim a twentieth of the x-height. Measured from the card; the shade between
    /// bands and the shadow are our own, for the brighter sky the title sits on here.
    fn default() -> Self {
        LetteringStyle {
            gold: LETTERING_GOLD.into(),
            shade: Color::srgba(0.30, 0.19, 0.07, 0.55).into(),
            shadow: Color::srgba(0.05, 0.035, 0.02, 0.65).into(),
            band_period: 20.0,
            band_fill: 0.5,
            rim: 8.0,
            shadow_blur: 10.0,
            shadow_offset: Vec2::new(5.0, 7.0),
        }
    }
}

impl UiMaterial for LetteringMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }
}

pub struct TitlePlugin;

impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "lettering.wesl");
        if !app.is_plugin_added::<LastDelverPlugin>() {
            app.add_plugins(LastDelverPlugin);
        }
        app.add_plugins(UiMaterialPlugin::<LetteringMaterial>::default())
            .init_resource::<UiFonts>()
            .init_resource::<Keymap>()
            .init_resource::<GameSettings>()
            .init_resource::<MenuPage>()
            .init_resource::<NewDelveSeed>()
            .add_message::<MenuCommand>()
            .add_systems(OnEnter(AppState::Title), spawn_title)
            .add_systems(OnExit(AppState::Title), |mut commands: Commands| commands.remove_resource::<Draft>())
            .add_systems(Update, (choose_new_delve_after.run_if(|| new_delve_wait().is_some()), title_controls, run_creator, fit_card, build_panel).chain().run_if(in_state(AppState::Title)));
    }
}

/// The part of an image `image` pixels across that covers a viewport `viewport` across
/// without stretching: all of the image's narrower side, centred along the other.
pub fn cover_rect(image: Vec2, viewport: Vec2) -> Rect {
    if image.min_element() <= 0.0 || viewport.min_element() <= 0.0 {
        return Rect::from_corners(Vec2::ZERO, image.max(Vec2::ZERO));
    }
    let scale = (viewport.x / image.x).max(viewport.y / image.y);
    Rect::from_center_size(image * 0.5, viewport / scale)
}

fn spawn_title(
    mut commands: Commands,
    assets: Res<AssetServer>,
    fonts: Res<UiFonts>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<LetteringMaterial>>,
    mut page: ResMut<MenuPage>,
    seed: Res<NewDelveSeed>,
) {
    // `FB_OPEN_MENU=settings` with `--title` opens on the settings page, for captures, and
    // `FB_OPEN_MENU=creator` on the creator, drafting from `--seed` (or 1).
    *page = capture_page().filter(|&p| p != MenuPage::Controls).unwrap_or_default();
    if std::env::var("FB_OPEN_MENU").is_ok_and(|name| name == "creator") {
        commands.insert_resource(Draft::roll(seed.0.unwrap_or(1)));
    }
    let glyphs = images.add(Image::new_target_texture(LETTERING_SIZE.x, LETTERING_SIZE.y, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb)));
    let scribe = commands
        .spawn((
            Name::new("Title lettering camera"),
            DespawnOnExit(AppState::Title),
            Camera2d,
            Camera { order: -1, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(glyphs.clone().into()),
        ))
        .id();
    lay_out_lettering(&mut commands, &fonts, scribe);

    let camera = commands.spawn((Name::new("Title camera"), DespawnOnExit(AppState::Title), Camera2d, IsDefaultUiCamera)).id();
    let material = materials.add(LetteringMaterial { style: LetteringStyle::default(), glyphs });
    commands
        .spawn((
            Name::new("Title"),
            TitleScreen,
            DespawnOnExit(AppState::Title),
            UiTargetCamera(camera),
            Node { width: percent(100), height: percent(100), ..default() },
            BackgroundColor(BG),
        ))
        .with_children(|screen| {
            screen.spawn((
                TitleCard,
                ImageNode { image: assets.load(CARD_PATH), image_mode: NodeImageMode::Stretch, ..default() },
                Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
            ));
            screen.spawn((
                TitleLettering,
                MaterialNode(material),
                Node {
                    position_type: PositionType::Absolute,
                    left: vw(4),
                    top: vh(6),
                    width: vw(62),
                    aspect_ratio: Some(LETTERING_SIZE.x as f32 / LETTERING_SIZE.y as f32),
                    ..default()
                },
            ));
        });
}

/// The name as the card sets it, in the lettering image's pixels: a tall F whose stem runs
/// down beside both lines, "orever" beside its top and "Below" tucked under, in from the F.
fn lay_out_lettering(commands: &mut Commands, fonts: &UiFonts, scribe: Entity) {
    let white = Color::WHITE;
    let placed = |left: f32, top: f32| Node { position_type: PositionType::Absolute, left: px(left), top: px(top), ..default() };
    commands
        .spawn((DespawnOnExit(AppState::Title), UiTargetCamera(scribe), Node { width: percent(100), height: percent(100), ..default() }))
        .with_children(|page| {
            page.spawn((Text::new("F"), UiFonts::style(&fonts.display_bold, 748.0, white), placed(24.0, -130.0)));
            page.spawn((Text::new("orever"), UiFonts::style(&fonts.display_bold, 457.0, white), placed(430.0, 12.0)));
            page.spawn((Text::new("Below"), UiFonts::style(&fonts.display_bold, 450.0, white), placed(236.0, 372.0)));
        });
}

/// Keeps the card covering the screen as the window changes.
fn fit_card(images: Res<Assets<Image>>, mut cards: Query<(&mut ImageNode, &ComputedNode), With<TitleCard>>) {
    for (mut card, node) in &mut cards {
        let Some(image) = images.get(&card.image) else { continue };
        let rect = Some(cover_rect(image.size_f32(), node.size));
        if card.rect != rect {
            card.rect = rect;
        }
    }
}

/// `FB_NEW_DELVE=<seconds>`: how long the title waits before beginning a delve by itself, past
/// the creator, for captures of a run going back to the title and on into the next delve.
fn new_delve_wait() -> Option<f32> {
    std::env::var("FB_NEW_DELVE").ok().and_then(|s| s.trim().parse::<f32>().ok())
}

/// Begins a delve once the title has shown for [`new_delve_wait`] seconds, each time it shows.
fn choose_new_delve_after(time: Res<Time>, mut shown: Local<f32>, mut chosen: MessageWriter<MenuCommand>) {
    *shown += time.delta_secs();
    if *shown >= new_delve_wait().unwrap_or(f32::MAX) {
        *shown = 0.0;
        chosen.write(MenuCommand::Creator(CreatorCommand::Begin));
    }
}

/// New delve (its button, or Enter on the main page) opens the character creator
/// ([`creator`]), which begins the delve; Esc on the settings page goes back to the buttons.
fn title_controls(mut commands: Commands, input: ActionInput, mut chosen: MessageReader<MenuCommand>, mut page: ResMut<MenuPage>, draft: Option<Res<Draft>>) {
    let chose = chosen.read().filter(|c| **c == MenuCommand::NewDelve).count() > 0;
    if draft.is_some() {
        return;
    }
    if chose || (*page == MenuPage::Main && input.just_pressed(Action::NewDelve)) {
        *page = MenuPage::Main;
        open_creator(&mut commands);
    } else if *page != MenuPage::Main && input.just_pressed(Action::Back) {
        *page = MenuPage::Main;
    }
}

/// A button for the bright card: a box of the ledger's page ruled in tan, its word cream, gold
/// under the mouse. The page is opaque: Bevy blends UI in linear light, where even 4% of the
/// card's white sky washes the near-black page to grey.
fn ledger_button(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, command: MenuCommand) {
    parent
        .spawn((
            MenuButton(command),
            ButtonInk { idle: INK, hover: FRAME_GOLD },
            Node {
                min_width: px(240),
                padding: UiRect::axes(px(18), px(8)),
                justify_content: JustifyContent::Center,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(4)),
                ..default()
            },
            BackgroundColor(BG),
            BorderColor::all(LINE),
        ))
        .with_child((Text::new(label), UiFonts::style(&fonts.body_medium, 24.0, INK)));
}

/// The width of the creator's panel in logical pixels.
const CREATOR_PX: f32 = 900.0;

/// A dark ledger panel `width` across in the middle of the screen, holding what `fill` builds.
fn centred_panel(screen: &mut ChildSpawnerCommands, width: f32, fill: impl FnOnce(&mut ChildSpawnerCommands)) {
    screen
        .spawn((
            TitlePanel,
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
        ))
        .with_children(|centre| {
            centre
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: px(18),
                        width: px(width),
                        max_width: percent(96),
                        padding: UiRect::all(px(32)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(6)),
                        ..default()
                    },
                    BackgroundColor(BG),
                    BorderColor::all(LINE),
                ))
                .with_children(fill);
        });
}

/// Builds the buttons (Continue as the [`LastDelver`] first, when there is one), the settings
/// page or the character creator in a dark panel, when the
/// page or the [`Draft`] changes; the lettering steps aside for the panels.
#[allow(clippy::too_many_arguments)]
fn build_panel(
    mut commands: Commands,
    page: Res<MenuPage>,
    draft: Option<Res<Draft>>,
    last: Option<Res<LastDelver>>,
    keymap: Res<Keymap>,
    settings: Res<GameSettings>,
    fonts: Res<UiFonts>,
    screens: Query<Entity, With<TitleScreen>>,
    panels: Query<Entity, With<TitlePanel>>,
    mut lettering: Query<&mut Visibility, With<TitleLettering>>,
    mut drafted: Local<bool>,
) {
    let Some(screen) = screens.iter().next() else { return };
    let drafting = draft.is_some();
    let changed = page.is_changed() || draft.as_ref().is_some_and(|d| d.is_changed()) || *drafted != drafting;
    if panels.iter().next().is_some() && !changed {
        return;
    }
    *drafted = drafting;
    for panel in &panels {
        commands.entity(panel).despawn();
    }
    for mut shown in &mut lettering {
        shown.set_if_neq(if *page == MenuPage::Main && !drafting { Visibility::Inherited } else { Visibility::Hidden });
    }
    commands.entity(screen).with_children(|screen| match (draft.as_deref(), *page) {
        (Some(draft), _) => centred_panel(screen, CREATOR_PX, |panel| creator_page(panel, &fonts, draft)),
        (None, MenuPage::Main) => {
            screen
                .spawn((
                    TitlePanel,
                    Node {
                        position_type: PositionType::Absolute,
                        right: vw(7),
                        bottom: vh(12),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(12),
                        ..default()
                    },
                ))
                .with_children(|list| {
                    if let Some(delver) = last.as_ref().and_then(|l| l.0.as_ref()) {
                        ledger_button(list, &fonts, &format!("Continue as {}", delver.name), MenuCommand::Continue);
                    }
                    ledger_button(list, &fonts, "New delve", MenuCommand::NewDelve);
                    ledger_button(list, &fonts, "Settings", MenuCommand::Show(MenuPage::Settings));
                    ledger_button(list, &fonts, "Quit", MenuCommand::Quit);
                });
        }
        (None, shown) => centred_panel(screen, MENU_PX + 64.0, |panel| menu_page(panel, &fonts, shown, &keymap, &settings, &crate::menu::ControlsView::default())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::CurrentLevel;
    use bevy::state::app::StatesPlugin;

    #[test]
    fn the_card_covers_the_screen_without_stretching() {
        let card = Vec2::new(1842.0, 1036.0);
        // A wider screen shows the card's full width and crops top and bottom evenly.
        let wide = cover_rect(card, Vec2::new(2560.0, 1080.0));
        assert!((wide.width() - card.x).abs() < 1e-3);
        assert!((wide.width() / wide.height() - 2560.0 / 1080.0).abs() < 1e-4);
        assert!((wide.min.y - (card.y - wide.max.y)).abs() < 1e-3);
        // A taller one shows its full height.
        let tall = cover_rect(card, Vec2::new(1080.0, 1350.0));
        assert!((tall.height() - card.y).abs() < 1e-3);
        assert!((tall.width() / tall.height() - 1080.0 / 1350.0).abs() < 1e-4);
        assert!(tall.min.x >= 0.0 && tall.max.x <= card.x);
        // Before layout, the whole card.
        assert_eq!(cover_rect(card, Vec2::ZERO), Rect::from_corners(Vec2::ZERO, card));
    }

    fn title_app(seed: Option<u32>) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Title)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Keymap>()
            .init_resource::<crate::keymap::ActiveContext>()
            .add_systems(PreUpdate, crate::keymap::track_context)
            .init_resource::<MenuPage>()
            .insert_resource(NewDelveSeed(seed))
            .add_message::<MenuCommand>()
            .add_message::<bevy::input::keyboard::KeyboardInput>()
            .add_systems(Update, (title_controls, run_creator).chain().run_if(in_state(AppState::Title)));
        app
    }

    fn state(app: &App) -> AppState {
        *app.world().resource::<State<AppState>>().get()
    }

    fn tap(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
        app.update();
    }

    fn type_text(app: &mut App, text: &str) {
        use bevy::input::{ButtonState, keyboard::{Key, KeyboardInput}};
        for c in text.chars() {
            app.world_mut().write_message(KeyboardInput {
                key_code: KeyCode::KeyA,
                logical_key: Key::Character(c.to_string().into()),
                state: ButtonState::Pressed,
                text: Some(c.to_string().into()),
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
        }
        app.update();
    }

    fn chosen(app: &App) -> Option<crate::character::Character> {
        app.world().get_resource::<crate::character::ChosenDelver>().map(|c| c.0.clone())
    }

    #[test]
    fn new_delve_opens_the_creator_and_begin_goes_down() {
        let mut app = title_app(Some(42));
        app.update();
        app.world_mut().write_message(MenuCommand::NewDelve);
        app.update();
        app.update();
        assert_eq!(state(&app), AppState::Title, "the creator comes first");
        assert!(app.world().get_resource::<CurrentLevel>().is_none(), "no level is made for the title");
        app.world_mut().resource_mut::<Draft>().apply(CreatorCommand::Folk(fb_body::Folk::Dwarf));
        app.world_mut().write_message(MenuCommand::Creator(CreatorCommand::Class(crate::character::Class::Cleric)));
        app.update();
        type_text(&mut app, "Brakka");
        let made = app.world().resource::<Draft>().character();
        app.world_mut().write_message(MenuCommand::Creator(CreatorCommand::Begin));
        app.update();
        app.update();
        assert_eq!(state(&app), AppState::Delving);
        assert_eq!(app.world().resource::<CurrentLevel>().0.seed, 42);
        let delver = chosen(&app).expect("the delve begins as the drafted delver");
        assert_eq!(delver, made);
        assert_eq!((delver.folk, delver.class, delver.name.as_str()), (fb_body::Folk::Dwarf, crate::character::Class::Cleric, "Brakka"));
        assert!(app.world().get_resource::<Draft>().is_none());
    }

    #[test]
    fn enter_opens_the_creator_and_enter_again_begins() {
        let mut app = title_app(None);
        tap(&mut app, KeyCode::Enter);
        assert_eq!(state(&app), AppState::Title, "the press that opens the creator does not begin");
        assert!(app.world().get_resource::<Draft>().is_some());
        tap(&mut app, KeyCode::Enter);
        app.update();
        assert_eq!(state(&app), AppState::Delving);
        assert!(app.world().get_resource::<CurrentLevel>().is_some());
    }

    #[test]
    fn an_empty_name_does_not_begin() {
        let mut app = title_app(None);
        tap(&mut app, KeyCode::Enter);
        let mut draft = app.world_mut().resource_mut::<Draft>();
        while !draft.name.is_empty() {
            draft.backspace();
        }
        tap(&mut app, KeyCode::Enter);
        app.update();
        assert_eq!(state(&app), AppState::Title);
    }

    #[test]
    fn escape_leaves_the_creator_for_the_buttons() {
        let mut app = title_app(None);
        tap(&mut app, KeyCode::Enter);
        tap(&mut app, KeyCode::Escape);
        assert!(app.world().get_resource::<Draft>().is_none());
        assert_eq!(*app.world().resource::<MenuPage>(), MenuPage::Main);
        assert_eq!(state(&app), AppState::Title);
    }

    #[test]
    fn continue_goes_down_again_as_the_last_delver_into_a_new_level() {
        let mut app = title_app(None);
        let last = crate::character::Character { name: "Brakka".into(), ..crate::character::Character::roll(6) };
        app.insert_resource(LastDelver(Some(last.clone())));
        let mut levels = Vec::new();
        for _ in 0..2 {
            app.update();
            app.world_mut().write_message(MenuCommand::Continue);
            app.update();
            app.update();
            assert_eq!(state(&app), AppState::Delving);
            assert_eq!(chosen(&app), Some(last.clone()));
            levels.push(app.world().resource::<CurrentLevel>().0.seed);
            app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Title);
            app.update();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_ne!(levels[0], levels[1], "each continue is a fresh level from the clock");
    }

    #[test]
    fn continue_without_a_last_delver_does_nothing() {
        let mut app = title_app(None);
        app.insert_resource(LastDelver(None));
        app.update();
        app.world_mut().write_message(MenuCommand::Continue);
        app.update();
        app.update();
        assert_eq!(state(&app), AppState::Title);
    }

    #[test]
    fn beginning_from_the_creator_remembers_the_delver() {
        let mut app = title_app(Some(3));
        app.insert_resource(LastDelver(None));
        tap(&mut app, KeyCode::Enter);
        let made = app.world().resource::<Draft>().character();
        tap(&mut app, KeyCode::Enter);
        assert_eq!(app.world().resource::<LastDelver>().0, Some(made));
    }

    #[test]
    fn begin_without_the_creator_rolls_a_delver() {
        let mut app = title_app(Some(7));
        app.update();
        app.world_mut().write_message(MenuCommand::Creator(CreatorCommand::Begin));
        app.update();
        app.update();
        assert_eq!(state(&app), AppState::Delving);
        assert!(chosen(&app).is_some());
    }

    #[test]
    fn escape_on_the_settings_page_goes_back_to_the_buttons() {
        let mut app = title_app(None);
        *app.world_mut().resource_mut::<MenuPage>() = MenuPage::Settings;
        // Enter does nothing off the main page.
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Enter);
        app.update();
        assert_eq!(state(&app), AppState::Title);
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::Enter);
        keys.clear();
        keys.press(KeyCode::Escape);
        app.update();
        assert_eq!(*app.world().resource::<MenuPage>(), MenuPage::Main);
        assert_eq!(state(&app), AppState::Title);
    }
}
