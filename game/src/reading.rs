//! Reading what is written on a thing: a note's words on a page over the view
//! ([`fb_items::Item::words`]). Reading a note ([`crate::eat::Eating::read`]) opens the page, and
//! the next key or click puts it down. That press is spent on the page: it is cleared before
//! anything else in the frame reads it, so Esc does not also open the menu, nor a click take
//! what lies under the cursor.
//!
//! `FB_READ=<seed>` writes a note of that seed on the level a moment in and opens it, for captures.
//! `FB_TEACH=<kind>[:book|scroll][:seed][,...]` puts a book or scroll written in that tongue
//! (`minotaur`, `cyclops`, `faun`, `demon`) in the pack, and `FB_TEACH_READ=1` reads every one a
//! moment into the delve, so the toast and the sheet's tongues can be captured.

use bevy::prelude::*;

use crate::play::DelveSystems;
use crate::ui::{BG, FAINT, GOLD, INK, MUTED, UiFonts};

pub struct ReadingPlugin;

impl Plugin for ReadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpenPage>()
            .add_systems(PreUpdate, put_page_down.after(bevy::input::InputSystems))
            .add_systems(Update, show_page.in_set(DelveSystems))
            .add_systems(OnExit(crate::launch::AppState::Delving), close_page);
        if std::env::var_os("FB_TEACH_READ").is_some() {
            app.add_systems(Update, read_teaching_for_capture.in_set(DelveSystems));
        }
        if std::env::var_os("FB_READ").is_some() {
            app.add_systems(
                Update,
                read_for_capture.before(show_page).in_set(DelveSystems),
            );
        }
    }
}

/// Reads a book or scroll written in a denizen tongue: the words it teaches join the `lexicon`
/// for the rest of the delve. Returns the toast's line, "You learn 12 words of Minotaur speech:
/// greetings and trade.", counting only the words that were new, or "You already know these
/// words." when none were.
pub fn study(
    lexicon: &mut fb_denizens::tongue::Lexicon,
    teaching: &fb_denizens::tongue::Teaching,
) -> String {
    let learned = lexicon.learn_teaching(teaching);
    let n = learned.len();
    if n == 0 {
        return "You already know these words.".to_string();
    }
    format!(
        "You learn {n} {} of {}: {}.",
        if n == 1 { "word" } else { "words" },
        fb_denizens::tongue::tongue_name(teaching.kind),
        teaching.topic_list()
    )
}

/// The book or scroll `FB_TEACH`'s entry `spec` names (`minotaur:scroll:7`), the `index`th of the
/// list: written in the tongue, rolled from the seed (1000 and the index if none). `None`, with a
/// warning, for an entry that names no tongue.
pub fn teaching_for_capture(spec: &str, index: u32) -> Option<fb_items::Item> {
    use fb_denizens::tongue::{Teaching, TeachingSize};
    let mut parts = spec.split(':').map(str::trim);
    let name = parts.next().unwrap_or_default();
    let Some(kind) = fb_denizens::DenizenKind::ALL
        .into_iter()
        .find(|k| k.key() == name.to_lowercase())
    else {
        warn!("FB_TEACH: no tongue called {name:?}");
        return None;
    };
    let (mut base, mut seed) = ("Scroll", 1000 + index);
    for part in parts {
        match part.parse::<u32>() {
            Ok(n) => seed = n,
            Err(_) if part.eq_ignore_ascii_case("book") => base = "Book",
            Err(_) => {}
        }
    }
    let size = if base == "Book" {
        TeachingSize::Book
    } else {
        TeachingSize::Scroll
    };
    let mut parts = fb_items::ItemParts::from(fb_items::roll_item(base, seed)?);
    parts.teaching = Some(Teaching::roll(seed, kind, size));
    fb_items::Item::from_parts(parts)
}

/// Reads every book and scroll in the pack that teaches words, a moment into the delve.
fn read_teaching_for_capture(
    mut eating: crate::eat::Eating,
    inventory: Res<crate::inventory::Inventory>,
    mut toasts: crate::inventory::Toasts,
    mut frames: Local<u32>,
) {
    *frames += 1;
    const READ_AT: u32 = 60;
    if *frames != READ_AT {
        return;
    }
    let taught: Vec<_> = inventory
        .carried_containers()
        .into_iter()
        .filter_map(|c| inventory.grid(c))
        .flat_map(|g| g.entries().iter().map(|e| e.id).collect::<Vec<_>>())
        .filter(|&id| inventory.item(id).is_some_and(|i| i.teaching().is_some()))
        .collect();
    for id in taught {
        if let Some(line) = eating.read(&inventory, id) {
            toasts.say(line);
        }
    }
}

/// The page being read: its title and words, `None` with nothing open.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct OpenPage {
    pub page: Option<Page>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub title: String,
    pub words: String,
}

impl OpenPage {
    /// Opens a page with `words` under `title`.
    pub fn open(&mut self, title: impl Into<String>, words: impl Into<String>) {
        self.page = Some(Page {
            title: title.into(),
            words: words.into(),
        });
    }

    pub fn is_open(&self) -> bool {
        self.page.is_some()
    }
}

/// The page on screen.
#[derive(Component, Debug)]
pub struct PageView;

/// Width of the page, in px of the reference interface (scaled with the window).
const PAGE_WIDTH: f32 = 520.0;

/// Opens the note `FB_READ` names, written on the current level, a moment into the delve.
fn read_for_capture(
    level: Res<crate::play::CurrentLevel>,
    mut open: ResMut<OpenPage>,
    mut frames: Local<u32>,
) {
    *frames += 1;
    const OPEN_AT: u32 = 40;
    if *frames != OPEN_AT {
        return;
    }
    let seed = std::env::var("FB_READ")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(1);
    let Some(note) = fb_items::roll_item("Note", seed).map(|n| n.in_world(&level.0.lore())) else {
        return;
    };
    if let Some(words) = &note.words {
        open.open(note.title(), words.clone());
    }
}

/// Closes the page on the first key or mouse button pressed after it opened, and spends that
/// press. It runs as input is read, before the frame's other systems: a page opens in `Update`,
/// so the press that opened it was read a frame earlier.
fn put_page_down(
    mut open: ResMut<OpenPage>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    if !open.is_open() {
        return;
    }
    let pressed_keys: Vec<KeyCode> = keys.get_just_pressed().copied().collect();
    let pressed_buttons: Vec<MouseButton> = mouse.get_just_pressed().copied().collect();
    if pressed_keys.is_empty() && pressed_buttons.is_empty() {
        return;
    }
    for key in pressed_keys {
        keys.clear_just_pressed(key);
    }
    for button in pressed_buttons {
        mouse.clear_just_pressed(button);
    }
    open.page = None;
}

/// Puts the page down as the delve ends, so none is left open, unseen, in the next.
fn close_page(mut open: ResMut<OpenPage>) {
    open.page = None;
}

/// Draws the open page over the view, or clears it once put down.
fn show_page(
    mut commands: Commands,
    open: Res<OpenPage>,
    shown: Query<Entity, With<PageView>>,
    cameras: Query<Entity, With<crate::carry::ViewModelCamera>>,
    fonts: Option<Res<UiFonts>>,
) {
    if !open.is_changed() {
        return;
    }
    for entity in &shown {
        commands.entity(entity).despawn();
    }
    let (Some(page), Some(fonts)) = (&open.page, fonts) else {
        return;
    };
    let mut root = commands.spawn((
        Name::new("PageView"),
        PageView,
        DespawnOnExit(crate::launch::AppState::Delving),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::BLACK.with_alpha(0.55)),
        GlobalZIndex(90),
    ));
    if let Some(camera) = cameras.iter().next() {
        root.insert(UiTargetCamera(camera));
    }
    root.with_children(|screen| {
        screen
            .spawn((
                Node {
                    width: px(PAGE_WIDTH),
                    max_width: percent(90),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(14),
                    padding: UiRect::axes(px(34), px(28)),
                    border: UiRect::all(px(2)),
                    ..default()
                },
                BackgroundColor(BG.mix(&GOLD, 0.16)),
                BorderColor::all(GOLD),
            ))
            .with_children(|sheet| {
                sheet.spawn((
                    Text::new(page.title.clone()),
                    UiFonts::style(&fonts.display, 22.0, GOLD),
                ));
                sheet.spawn((
                    Node {
                        height: px(1),
                        ..default()
                    },
                    BackgroundColor(FAINT),
                ));
                sheet.spawn((
                    Text::new(page.words.clone()),
                    UiFonts::style(&fonts.body_italic, 19.0, INK),
                    TextLayout::justify(Justify::Left),
                ));
                sheet.spawn((
                    Text::new("Any key to put it down"),
                    UiFonts::style(&fonts.body, 12.0, MUTED),
                ));
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<OpenPage>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_systems(Update, put_page_down);
        app
    }

    #[test]
    fn studying_counts_the_new_words_and_says_when_none_are() {
        use fb_denizens::tongue::{Lexicon, Teaching, Topic};
        let greetings = Teaching::of_topics(
            fb_denizens::DenizenKind::Minotaur,
            &[Topic::Greeting, Topic::Trade],
        );
        let mut lexicon = Lexicon::new();
        let said = study(&mut lexicon, &greetings);
        assert_eq!(
            said,
            format!(
                "You learn {} words of Minotaur speech: greetings and trade.",
                greetings.words.len()
            )
        );
        assert_eq!(
            study(&mut lexicon, &greetings),
            "You already know these words."
        );
        let more = Teaching::of_topics(
            fb_denizens::DenizenKind::Minotaur,
            &[Topic::Trade, Topic::Quest],
        );
        assert!(study(&mut lexicon, &more).contains("of Minotaur speech: trade and requests"));
    }

    #[test]
    fn capture_entries_name_a_tongue_a_size_and_a_seed() {
        let book = teaching_for_capture("Demon:book:5", 0).unwrap();
        assert_eq!(
            (book.base.as_str(), book.seed, book.tongue()),
            ("Book", 5, Some(fb_denizens::DenizenKind::Demon))
        );
        let scroll = teaching_for_capture("cyclops", 3).unwrap();
        assert_eq!((scroll.base.as_str(), scroll.seed), ("Scroll", 1003));
        assert!(teaching_for_capture("orc", 0).is_none());
    }

    #[test]
    fn a_press_puts_the_page_down_and_is_spent_on_it() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<OpenPage>()
            .open("Note", "I escaped from a troll.");
        app.update();
        assert!(
            app.world().resource::<OpenPage>().is_open(),
            "nothing pressed"
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(
            !app.world().resource::<OpenPage>().is_open(),
            "Esc puts it down"
        );
        assert!(
            !app.world()
                .resource::<ButtonInput<KeyCode>>()
                .just_pressed(KeyCode::Escape),
            "and nothing else sees the Esc"
        );
        app.world_mut()
            .resource_mut::<OpenPage>()
            .open("Note", "Again.");
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert!(
            !app.world().resource::<OpenPage>().is_open(),
            "a click puts it down"
        );
        assert!(
            !app.world()
                .resource::<ButtonInput<MouseButton>>()
                .just_pressed(MouseButton::Left),
            "and the click lands on nothing else"
        );
    }
}
