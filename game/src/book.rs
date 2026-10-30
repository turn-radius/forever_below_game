//! The spellbook: an open book a caster writes prayers in ([`Action::Book`], B).
//!
//! - **Left page.** A picker for each kind of term (effect, reach, duration) that lists every
//!   term of that kind for the caster's school. A term the caster has not unlocked shows greyed,
//!   with what unlocks it, and cannot be picked ([`Draft::pick`]). Under the pickers a name
//!   field (click it and type; Suggest writes a seeded name from a grammar) and Write, which
//!   keeps the prayer in the delver's [`Spellbook`] under that name.
//! - **Right page.** The prayer being written: how its terms combine, and what it costs, how far
//!   it reaches and how long it lasts at tiers 1 to 3, updating as the terms change. Below, the
//!   prayers written so far: Choose sets the one the Pray key casts ([`Chosen`]) and Slot binds
//!   it to a quickslot, the next free one on each click ([`AbilitySlots`]).
//! - The book is laid out for the 1920 by 1080 reference and scales with the window
//!   ([`crate::ui`]); the mouse is read in the interface's own pixels ([`ui_cursor`]). It is a
//!   screen like the others: the delver stands still behind it, and Esc or B closes it.
//! - `FB_OPEN_BOOK=1` opens it a few frames in, for captures; `FB_OPEN_BOOK=swap` first
//!   swaps "around you" for "touch", `FB_OPEN_BOOK=write` writes a second prayer, `FB_OPEN_BOOK=heal` writes "Mending Hand" from
//!   Heal (with `FB_UNLOCK_TERMS=heal,cure` to unlock terms at the start).

use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_character::casting::{ComposeError, MAX_TIER, Spellbook, TermKind, TermRules, Written, compose_with, points};
use fb_character::Casting;
use fb_character::rules::Rules;
use procgen_grammar::Grammar;

use crate::carry::ViewModelCamera;
use crate::casting::{AbilitySlots, Chosen, DelverSpellbook};
use crate::character::Character;
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, Player, Screen, ScreenOpen, show_screen};
use crate::quickslots::{Quickslots, SLOTS, number};
use crate::rules::GameRules;
use crate::ui::{ACCENT, BAD, BG, BRIGHT, CELL, FAINT, GOLD, INK, LINE, MUTED, OK, UiFonts, node_rect, ui_cursor};

/// The book's width at the reference size, logical pixels.
const WIDTH: f32 = 1500.0;
/// Its height.
const HEIGHT: f32 = 860.0;
/// The longest name a prayer can have.
pub const NAME_LENGTH: usize = 36;

pub struct BookPlugin;

impl Plugin for BookPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Book>().add_systems(OnExit(crate::launch::AppState::Delving), |mut book: ResMut<Book>| *book = Book::default()).add_systems(
            Update,
            (open_for_capture, toggle_book, type_name, click_book, forget_when_closed, build_book, hover_book, own_the_keyboard).chain().in_set(DelveSystems),
        );
    }
}

/// The prayer being written: one term of each kind, by id, and a name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    pub effect: String,
    pub reach: String,
    pub duration: String,
    pub name: String,
}

impl Draft {
    /// A draft with `written`'s terms and no name.
    pub fn from_terms(written: &Written) -> Draft {
        Draft { effect: written.effect.clone(), reach: written.reach.clone(), duration: written.duration.clone(), name: String::new() }
    }

    /// The term id of `kind`.
    pub fn term(&self, kind: TermKind) -> &str {
        match kind {
            TermKind::Effect => &self.effect,
            TermKind::Reach => &self.reach,
            TermKind::Duration => &self.duration,
        }
    }

    /// The draft as a prayer, named or not, whatever its terms.
    pub fn as_written(&self, school: Casting) -> Written {
        Written { name: self.name.trim().to_string(), school, effect: self.effect.clone(), reach: self.reach.clone(), duration: self.duration.clone() }
    }

    /// Puts `id` in the place of its kind. A term the caster has not unlocked, one of another
    /// kind or school, or one that does not exist is refused and the draft is as it was.
    pub fn pick(&mut self, rules: &Rules, book: &Spellbook, school: Casting, kind: TermKind, id: &str) -> Result<(), ComposeError> {
        let mut next = self.clone();
        match kind {
            TermKind::Effect => next.effect = id.to_string(),
            TermKind::Reach => next.reach = id.to_string(),
            TermKind::Duration => next.duration = id.to_string(),
        }
        next.as_written(school).validate_with(rules, &book.unlocked)?;
        *self = next;
        Ok(())
    }

    /// Writes the draft into `book` under its name, replacing a prayer of the same name. Returns
    /// whether it replaced one. An empty name is refused.
    pub fn save(&self, rules: &Rules, book: &mut Spellbook, school: Casting) -> Result<bool, ComposeError> {
        let written = compose_with(rules, &book.unlocked, school, &self.name, &self.effect, &self.reach, &self.duration)?;
        let replaced = book.find(&written.name).is_some();
        book.save_with(rules, written)?;
        Ok(replaced)
    }
}

/// What unlocks a term the caster does not have yet.
pub fn unlocked_by(rules: &Rules, school: Casting, id: &str) -> &'static str {
    match id {
        "bane" | "frighten" => "unlocked by a favored foe",
        "charm" => "unlocked by an alignment",
        _ if Spellbook::scroll_terms(rules, school).iter().any(|t| t == id) => "unlocked by a prayer scroll",
        _ => "not yet unlocked",
    }
}

/// How long a duration lasts, in words.
pub fn lasts(seconds: f64) -> String {
    if seconds <= 0.0 {
        "instant".into()
    } else if seconds >= 3600.0 {
        format!("{} h", points(seconds / 3600.0))
    } else if seconds >= 60.0 {
        format!("{} min", points(seconds / 60.0))
    } else {
        format!("{} s", points(seconds))
    }
}

/// How far a reach reaches, in words.
pub fn reaches(feet: f64) -> String {
    if feet <= 0.0 { "yourself".into() } else { format!("{} ft", points(feet)) }
}

/// What a term adds to the prayer, for the page that shows how they combine.
pub fn term_effect(term: &TermRules) -> String {
    let mut parts = vec![format!("+{} mana", points(term.cost))];
    match term.kind {
        TermKind::Reach => parts.push(format!("reaches {}", reaches(term.range))),
        TermKind::Duration => parts.push(format!("lasts {}", lasts(term.seconds))),
        TermKind::Effect => {}
    }
    parts.join(", ")
}

fn grammar() -> &'static Grammar {
    static NAMES: std::sync::OnceLock<Grammar> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        Grammar::from_pairs(&[
            ("origin", &["#adj# #noun#", "#noun# of #virtue#", "#adj# #noun# of #virtue#"]),
            ("adj", &["Gentle", "Steadfast", "Hallowed", "Quiet", "Patient", "Tender", "Lowly", "Unbroken", "Kindly"]),
            ("noun", &["Prayer", "Vigil", "Mercy", "Hymn", "Blessing", "Litany", "Psalm", "Rite"]),
            ("virtue", &["Dawn", "Mercy", "Ash", "Candles", "the Deep", "the Hearth", "Small Hours", "the Faithful"]),
        ])
    })
}

/// The seed of the `n`th suggestion: spread out, since nearby seeds read alike.
pub fn name_seed(n: u32) -> u32 {
    n.wrapping_mul(2_654_435_761).wrapping_add(7)
}

/// A name for a prayer, from the grammar and a seed.
pub fn suggest_name(seed: u32) -> String {
    procgen_grammar::expand(grammar(), "#origin#", seed).unwrap_or_else(|_| "Prayer".into())
}

/// What the book is showing.
#[derive(Resource, Clone, Debug, Default)]
pub struct Book {
    pub draft: Draft,
    /// Whether typing goes into the name field.
    pub focus: bool,
    /// What the last click did, and whether it worked.
    pub notice: Option<(String, bool)>,
    /// How many names have been suggested; each is the next seed.
    pub suggested: u32,
    revision: u32,
    open: bool,
}

impl Book {
    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

/// What a button in the book does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Do {
    Pick(TermKind, String),
    Focus,
    Suggest,
    Save,
    Choose(String),
    Slot(String),
}

/// The caster's things a click works on.
pub struct Desk<'a> {
    pub rules: &'a Rules,
    pub school: Casting,
    pub spellbook: &'a mut Spellbook,
    pub chosen: &'a mut Chosen,
    pub abilities: &'a mut AbilitySlots,
    pub items: &'a mut Quickslots,
}

/// Does `act` at the book and says what came of it in `book.notice`.
pub fn act(book: &mut Book, desk: &mut Desk, act: Do) {
    if act != Do::Focus {
        book.focus = false;
    }
    match act {
        Do::Pick(kind, id) => {
            book.notice = match book.draft.pick(desk.rules, desk.spellbook, desk.school, kind, &id) {
                Ok(()) => None,
                Err(why) => Some((why.say(), false)),
            };
        }
        Do::Focus => book.focus = true,
        Do::Suggest => {
            book.draft.name = suggest_name(name_seed(book.suggested));
            book.suggested += 1;
            book.notice = None;
        }
        Do::Save => {
            book.notice = Some(match book.draft.save(desk.rules, desk.spellbook, desk.school) {
                Ok(true) => (format!("Wrote {} over the old page.", book.draft.name.trim()), true),
                Ok(false) => (format!("Wrote {}.", book.draft.name.trim()), true),
                Err(why) => (why.say(), false),
            });
        }
        Do::Choose(name) => {
            if let Some(written) = desk.spellbook.find(&name) {
                book.draft = Draft::from_terms(written);
                desk.chosen.name = name.clone();
                book.notice = Some((format!("{name} is what the Pray key casts."), true));
            }
        }
        Do::Slot(name) => {
            let start = (0..SLOTS).find(|&s| desk.abilities.get(s) == Some(name.as_str())).map_or(0, |s| s + 1);
            let free = (0..SLOTS).map(|n| (start + n) % SLOTS).find(|&s| desk.items.get(s).is_none());
            book.notice = Some(match free {
                Some(slot) => {
                    desk.abilities.bind(slot, &name);
                    (format!("{name} is bound to slot {}.", number(slot)), true)
                }
                None => ("Every slot holds an item.".to_string(), false),
            });
        }
    }
    book.changed();
}

#[derive(Component)]
struct BookScreen;

/// A button on a page. `selected` and `enabled` tint it; a locked term is not enabled.
#[derive(Component, Clone)]
struct BookButton {
    act: Do,
    enabled: bool,
    selected: bool,
}

/// `FB_OPEN_BOOK`, a few frames in: `swap` also swaps "around you" for "touch", `write` writes
/// "Hand of Mercy" with it, for captures.
#[allow(clippy::too_many_arguments)]
fn open_for_capture(
    mut frames: Local<u32>,
    mut book: ResMut<Book>,
    mut open: ResMut<ScreenOpen>,
    character: Option<Res<Character>>,
    rules: Res<GameRules>,
    mut players: Query<&mut DelverSpellbook, With<Player>>,
    chosen: Res<Chosen>,
) {
    *frames += 1;
    let Ok(mode) = std::env::var("FB_OPEN_BOOK") else { return };
    if *frames != 90 || mode.is_empty() || character.is_none() {
        return;
    }
    let school = character.as_deref().map_or(Casting::Divine, |c| c.class.casting());
    // `FB_UNLOCK_TERMS=heal,cure` unlocks those terms first, as a prayer scroll will.
    if let (Ok(ids), Some(mut sp)) = (std::env::var("FB_UNLOCK_TERMS"), players.iter_mut().next()) {
        for id in ids.split(',').map(str::trim).filter(|i| !i.is_empty()) {
            sp.0.unlock_with(&rules, id);
        }
    }
    open_book(&mut book, &mut open, None, &players.as_readonly(), &chosen);
    if mode == "heal" {
        if let Some(sp) = players.iter().next() {
            let _ = book.draft.pick(&rules, &sp.0, school, TermKind::Effect, "heal");
            let _ = book.draft.pick(&rules, &sp.0, school, TermKind::Reach, "touch");
        }
        book.draft.name = "Mending Hand".into();
        if let Some(mut sp) = players.iter_mut().next() {
            let _ = book.draft.save(&rules, &mut sp.0, school);
        }
    }
    if mode == "swap" || mode == "write" {
        if let Some(sp) = players.iter().next() {
            let _ = book.draft.pick(&rules, &sp.0, school, TermKind::Reach, "touch");
        }
    }
    if mode == "write" {
        book.draft.name = "Hand of Mercy".into();
        if let Some(mut sp) = players.iter_mut().next() {
            let _ = book.draft.save(&rules, &mut sp.0, school);
        }
    }
    book.changed();
}

fn open_book(book: &mut Book, open: &mut ScreenOpen, cursor: Option<&mut CursorOptions>, players: &Query<&DelverSpellbook, With<Player>>, chosen: &Chosen) {
    let from = players.iter().next().and_then(|b| b.0.find(&chosen.name).or(b.0.written.first())).cloned().unwrap_or_else(Written::divine_light);
    book.draft = Draft::from_terms(&from);
    book.focus = false;
    book.notice = None;
    book.open = true;
    book.changed();
    show_screen(open, Some(Screen::Book), cursor);
}

/// B opens the book for a caster and closes it again.
#[allow(clippy::too_many_arguments)]
fn toggle_book(
    input: ActionInput,
    mut book: ResMut<Book>,
    mut open: ResMut<ScreenOpen>,
    character: Option<Res<Character>>,
    chosen: Res<Chosen>,
    players: Query<&DelverSpellbook, With<Player>>,
    mut toast: ResMut<crate::inventory::Toast>,
    time: Option<Res<Time<Real>>>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
    journey: Option<Res<crate::travel::Journey>>,
) {
    if !input.just_pressed(Action::Book) || book.focus || journey.is_some() {
        return;
    }
    if open.is(Screen::Book) {
        show_screen(&mut open, None, cursor.as_deref_mut().map(|c| &mut **c));
    } else if open.0.is_none() {
        if character.is_none_or(|c| c.class.casting() == Casting::Martial) {
            toast.say("You have no prayers or spells to write.", time.map_or(0.0, |t| t.elapsed_secs()));
            return;
        }
        open_book(&mut book, &mut open, cursor.as_deref_mut().map(|c| &mut **c), &players, &chosen);
    }
}

/// Typing goes into the name field while it has the focus.
fn type_name(mut keys: MessageReader<KeyboardInput>, raw: Res<ButtonInput<KeyCode>>, open: Res<ScreenOpen>, mut book: ResMut<Book>) {
    if !open.is(Screen::Book) || !book.focus {
        keys.clear();
        return;
    }
    // Esc leaves the field first; with it unfocused, the next Esc closes the book.
    if raw.just_pressed(KeyCode::Escape) {
        keys.clear();
        book.focus = false;
        book.changed();
        return;
    }
    let mut changed = false;
    for key in keys.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        match &key.logical_key {
            Key::Character(text) => {
                for c in text.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '\'' | '-')) {
                    if book.draft.name.chars().count() < NAME_LENGTH {
                        book.draft.name.push(c);
                        changed = true;
                    }
                }
            }
            Key::Space if book.draft.name.chars().count() < NAME_LENGTH => {
                book.draft.name.push(' ');
                changed = true;
            }
            Key::Backspace => changed |= book.draft.name.pop().is_some(),
            Key::Enter => {
                book.focus = false;
                changed = true;
            }
            _ => {}
        }
    }
    if changed {
        book.changed();
    }
}

/// While the name field has the focus no action fires, so typing "-" or a letter that opens a
/// screen does nothing else. Runs last, so the frame an Esc unfocuses the field still reads as
/// captured and does not also close the book.
fn own_the_keyboard(book: Res<Book>, open: Res<ScreenOpen>, mut capture: ResMut<crate::keymap::InputCapture>) {
    capture.set_if_neq(crate::keymap::InputCapture(book.focus && open.is(Screen::Book)));
}

/// A click on a button.
#[allow(clippy::too_many_arguments)]
fn click_book(
    input: ActionInput,
    mut book: ResMut<Book>,
    open: Res<ScreenOpen>,
    rules: Res<GameRules>,
    character: Option<Res<Character>>,
    mut chosen: ResMut<Chosen>,
    mut abilities: ResMut<AbilitySlots>,
    mut items: ResMut<Quickslots>,
    mut players: Query<&mut DelverSpellbook, With<Player>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    buttons: Query<(&BookButton, &ComputedNode, &UiGlobalTransform)>,
) {
    if !open.is(Screen::Book) || !input.just_pressed(Action::Choose) {
        return;
    }
    let Some(point) = windows.iter().next().and_then(|w| ui_cursor(w, ui_scale.as_deref())) else { return };
    let Some(mut spellbook) = players.iter_mut().next() else { return };
    let school = character.as_deref().map_or(Casting::Divine, |c| c.class.casting());
    match buttons.iter().find(|(_, node, at)| node_rect(node, at).contains(point)) {
        Some((button, ..)) if button.enabled => {
            let mut desk = Desk { rules: &rules, school, spellbook: &mut spellbook.0, chosen: &mut chosen, abilities: &mut abilities, items: &mut items };
            act(&mut book, &mut desk, button.act.clone());
        }
        Some(_) => {}
        None => {
            if book.focus {
                book.focus = false;
                book.changed();
            }
        }
    }
}

fn forget_when_closed(open: Res<ScreenOpen>, mut book: ResMut<Book>) {
    if book.open && !open.is(Screen::Book) {
        book.open = false;
        book.focus = false;
        book.changed();
    }
}

/// Lights the enabled button under the mouse.
fn hover_book(open: Res<ScreenOpen>, windows: Query<&Window, With<PrimaryWindow>>, ui_scale: Option<Res<UiScale>>, mut buttons: Query<(&BookButton, &ComputedNode, &UiGlobalTransform, &mut BackgroundColor, &mut BorderColor)>) {
    if !open.is(Screen::Book) {
        return;
    }
    let point = windows.iter().next().and_then(|w| ui_cursor(w, ui_scale.as_deref()));
    for (button, node, at, mut fill, mut border) in &mut buttons {
        let over = button.enabled && point.is_some_and(|p| node_rect(node, at).contains(p));
        let (want_fill, want_border) = button_colours(button, over);
        if fill.0 != want_fill {
            fill.0 = want_fill;
        }
        if *border != BorderColor::all(want_border) {
            *border = BorderColor::all(want_border);
        }
    }
}

fn button_colours(button: &BookButton, over: bool) -> (Color, Color) {
    if over {
        (FAINT, LINE)
    } else if button.selected {
        (CELL.mix(&ACCENT, 0.18), ACCENT)
    } else if !button.enabled {
        (CELL.with_alpha(0.55), FAINT)
    } else {
        (CELL, FAINT)
    }
}

/// The terms of `kind` for the book: those the caster has first, then the locked ones.
fn ordered<'a>(rules: &'a Rules, book: &Spellbook, school: Casting, kind: TermKind) -> impl Iterator<Item = (&'a String, &'a TermRules)> {
    let mut terms: Vec<_> = rules.casting.terms.iter().filter(|(_, t)| t.kind == kind && t.school == school).collect();
    // A term nothing grants stays out of the picker until something does; the devotion and
    // prayer scroll terms show locked meanwhile.
    let scroll = Spellbook::scroll_terms(rules, school);
    terms.retain(|(id, _)| book.unlocked.contains(*id) || Spellbook::DEVOTION_TERMS.contains(&id.as_str()) || scroll.contains(id));
    terms.sort_by_key(|(id, _)| !book.unlocked.contains(*id));
    terms.into_iter()
}

/// The paper of a page.
fn page_colour() -> Color {
    CELL.mix(&GOLD, 0.2)
}

fn line(parent: &mut ChildSpawnerCommands, text: impl Into<String>, font: &Handle<Font>, size: f32, colour: Color) {
    parent.spawn((Text::new(text), UiFonts::style(font, size, colour)));
}

fn frame(button: BookButton, node: Node) -> impl Bundle {
    let (fill, border) = button_colours(&button, false);
    (button, Node { border: UiRect::all(px(1.0)), border_radius: BorderRadius::all(px(7)), flex_shrink: 0.0, ..node }, BackgroundColor(fill), BorderColor::all(border))
}

fn rule(parent: &mut ChildSpawnerCommands) {
    parent.spawn((Node { height: px(1), flex_shrink: 0.0, margin: UiRect::vertical(px(4)), ..default() }, BackgroundColor(LINE.with_alpha(0.6))));
}

/// Draws the book while it is open, and again whenever it changes.
#[allow(clippy::too_many_arguments)]
fn build_book(
    mut commands: Commands,
    screen: Res<ScreenOpen>,
    book: Res<Book>,
    rules: Res<GameRules>,
    character: Option<Res<Character>>,
    chosen: Res<Chosen>,
    abilities: Res<AbilitySlots>,
    fonts: Option<Res<UiFonts>>,
    players: Query<&DelverSpellbook, With<Player>>,
    cameras: Query<Entity, With<ViewModelCamera>>,
    roots: Query<Entity, With<BookScreen>>,
    mut drawn: Local<Option<u32>>,
) {
    let existing = roots.iter().next();
    let (true, Some(fonts), Some(spellbook)) = (screen.is(Screen::Book), fonts, players.iter().next()) else {
        if let Some(root) = existing {
            commands.entity(root).despawn();
        }
        *drawn = None;
        return;
    };
    if existing.is_some() && *drawn == Some(book.revision) {
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
                Name::new("Spellbook"),
                BookScreen,
                DespawnOnExit(crate::launch::AppState::Delving),
                UiTargetCamera(camera),
                Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                GlobalZIndex(12),
            ))
            .id(),
    };
    *drawn = Some(book.revision);
    let rules: &Rules = &rules;
    let school = character.as_deref().map_or(Casting::Divine, |c| c.class.casting());
    let sp = &spellbook.0;
    let noun = if school == Casting::Arcane { "spell" } else { "prayer" };
    let draft = &book.draft;
    let shown = draft.as_written(school);
    let name = if shown.name.is_empty() { format!("Unnamed {noun}") } else { shown.name.clone() };

    commands.entity(root).with_children(|root| {
        // The cover, with the two pages and the spine between them.
        root.spawn((
            Name::new("Book"),
            Node {
                width: px(WIDTH),
                height: px(HEIGHT),
                max_width: percent(96),
                max_height: percent(94),
                padding: UiRect::all(px(14)),
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(14)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(BG),
            BorderColor::all(LINE),
            BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.55), px(0), px(14), px(0), px(34)),
        ))
        .with_children(|cover| {
            let page = |extra: UiRect| Node {
                flex_grow: 1.0,
                flex_basis: px(0),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                padding: UiRect::axes(px(34), px(24)),
                border: extra,
                overflow: Overflow::clip(),
                ..default()
            };
            // Left page: the pickers, the name and Write.
            cover.spawn((Name::new("Left page"), page(UiRect::default()), BackgroundColor(page_colour()), BorderColor::all(LINE))).with_children(|left| {
                line(left, format!("Write a {noun}"), &fonts.display, 32.0, BRIGHT);
                line(left, "Pick one term of each kind. They combine into what it does, how far it reaches and how long it lasts.", &fonts.body_italic, 16.0, MUTED);
                rule(left);
                for (kind, title) in [(TermKind::Effect, "Effect"), (TermKind::Reach, "Reach"), (TermKind::Duration, "Duration")] {
                    line(left, title, &fonts.body_medium, 18.0, GOLD.mix(&INK, 0.35));
                    left.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(8), row_gap: px(8), ..default() }).with_children(|row| {
                        for (id, term) in ordered(rules, sp, school, kind) {
                            let unlocked = sp.unlocked.contains(id);
                            let button = BookButton { act: Do::Pick(kind, id.clone()), enabled: unlocked, selected: draft.term(kind) == id };
                            row.spawn(frame(button, Node { flex_direction: FlexDirection::Column, min_width: px(150), padding: UiRect::axes(px(12), px(7)), row_gap: px(1), ..default() })).with_children(|b| {
                                let ink = if unlocked { INK } else { MUTED.with_alpha(0.55) };
                                line(b, term.name.clone(), &fonts.body_medium, 18.0, ink);
                                if unlocked {
                                    line(b, term_effect(term), &fonts.mono, 12.0, MUTED);
                                } else {
                                    line(b, unlocked_by(rules, school, id), &fonts.body_italic, 13.0, BAD.mix(&MUTED, 0.5).with_alpha(0.8));
                                }
                            });
                        }
                    });
                }
                rule(left);
                line(left, "Name", &fonts.body_medium, 18.0, GOLD.mix(&INK, 0.35));
                left.spawn(Node { column_gap: px(8), align_items: AlignItems::Stretch, ..default() }).with_children(|row| {
                    let field = BookButton { act: Do::Focus, enabled: true, selected: book.focus };
                    row.spawn(frame(field, Node { flex_grow: 1.0, flex_basis: px(0), padding: UiRect::axes(px(12), px(8)), ..default() })).with_children(|f| {
                        if draft.name.is_empty() && !book.focus {
                            line(f, format!("Click to name your {noun}"), &fonts.body_italic, 18.0, MUTED.with_alpha(0.7));
                        } else {
                            line(f, format!("{}{}", draft.name, if book.focus { "|" } else { "" }), &fonts.body, 20.0, BRIGHT);
                        }
                    });
                    row.spawn(frame(BookButton { act: Do::Suggest, enabled: true, selected: false }, Node { padding: UiRect::axes(px(14), px(8)), align_items: AlignItems::Center, ..default() })).with_children(|b| {
                        line(b, "Suggest", &fonts.body_medium, 17.0, INK);
                    });
                    row.spawn(frame(BookButton { act: Do::Save, enabled: true, selected: false }, Node { padding: UiRect::axes(px(18), px(8)), align_items: AlignItems::Center, ..default() })).with_children(|b| {
                        line(b, "Write", &fonts.body_medium, 17.0, OK);
                    });
                });
                if let Some((text, ok)) = &book.notice {
                    line(left, text.clone(), &fonts.body_italic, 17.0, if *ok { OK } else { BAD });
                }
            });
            // The spine.
            cover.spawn((Node { width: px(10), flex_shrink: 0.0, ..default() }, BackgroundColor(BG.mix(&GOLD, 0.15))));
            // Right page: the prayer, how it combines, its tiers, and the written ones.
            cover.spawn((Name::new("Right page"), page(UiRect::default()), BackgroundColor(page_colour().mix(&INK, 0.04)), BorderColor::all(LINE))).with_children(|right| {
                line(right, name, &fonts.display, 32.0, BRIGHT);
                let terms: Vec<String> = [&shown.effect, &shown.reach, &shown.duration].iter().filter_map(|id| rules.casting.terms.get(id.as_str())).map(|t| t.name.clone()).collect();
                line(right, terms.join("  \u{b7}  "), &fonts.body_italic, 18.0, MUTED);
                rule(right);
                line(right, "How the terms combine", &fonts.body_medium, 18.0, GOLD.mix(&INK, 0.35));
                for id in [&shown.effect, &shown.reach, &shown.duration] {
                    let Some(term) = rules.casting.terms.get(id.as_str()) else { continue };
                    right.spawn(Node { column_gap: px(12), ..default() }).with_children(|row| {
                        row.spawn(Node { width: px(120), flex_shrink: 0.0, ..default() }).with_children(|c| line(c, format!("{:?}", term.kind), &fonts.mono, 14.0, MUTED));
                        row.spawn(Node { width: px(150), flex_shrink: 0.0, ..default() }).with_children(|c| line(c, term.name.clone(), &fonts.body_medium, 18.0, INK));
                        line(row, term_effect(term), &fonts.mono, 14.0, INK);
                    });
                }
                line(right, "The cost is the sum of the terms. Range and duration grow with the tier you cast at.", &fonts.body_italic, 15.0, MUTED);
                rule(right);
                right.spawn(Node { column_gap: px(12), ..default() }).with_children(|row| {
                    for (w, head) in [(90.0, "Tier"), (110.0, "Mana"), (130.0, "Range"), (140.0, "Lasts")] {
                        row.spawn(Node { width: px(w), ..default() }).with_children(|c| line(c, head, &fonts.mono, 14.0, MUTED));
                    }
                });
                for tier in 1..=MAX_TIER {
                    let p = shown.preview_with(rules, tier);
                    right.spawn(Node { column_gap: px(12), ..default() }).with_children(|row| {
                        for (w, text) in [(90.0, tier.to_string()), (110.0, points(p.cost)), (130.0, reaches(p.range)), (140.0, lasts(p.duration))] {
                            row.spawn(Node { width: px(w), ..default() }).with_children(|c| line(c, text, &fonts.mono, 19.0, BRIGHT));
                        }
                    });
                }
                rule(right);
                line(right, format!("Written {noun}s"), &fonts.body_medium, 18.0, GOLD.mix(&INK, 0.35));
                for w in &sp.written {
                    let is_chosen = chosen.name == w.name;
                    let slot = (0..SLOTS).find(|&s| abilities.get(s) == Some(w.name.as_str()));
                    right.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                        row.spawn(Node { flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Column, min_width: px(0), ..default() }).with_children(|c| {
                            line(c, w.name.clone(), &fonts.body_medium, 19.0, INK);
                            let names: Vec<String> = [&w.effect, &w.reach, &w.duration].iter().filter_map(|id| rules.casting.terms.get(id.as_str())).map(|t| t.name.clone()).collect();
                            let p = w.preview_with(rules, 1);
                            line(c, format!("{} \u{b7} {} mana at tier 1", names.join(", "), points(p.cost)), &fonts.mono, 12.0, MUTED);
                        });
                        let choose = BookButton { act: Do::Choose(w.name.clone()), enabled: true, selected: is_chosen };
                        row.spawn(frame(choose, Node { padding: UiRect::axes(px(12), px(6)), ..default() })).with_children(|b| line(b, if is_chosen { "Chosen" } else { "Choose" }, &fonts.body_medium, 16.0, if is_chosen { OK } else { INK }));
                        let bind = BookButton { act: Do::Slot(w.name.clone()), enabled: true, selected: slot.is_some() };
                        row.spawn(frame(bind, Node { padding: UiRect::axes(px(12), px(6)), min_width: px(86), justify_content: JustifyContent::Center, ..default() }))
                            .with_children(|b| line(b, slot.map_or("Slot".to_string(), |s| format!("Slot {}", number(s))), &fonts.body_medium, 16.0, INK));
                    });
                }
                right.spawn(Node { flex_grow: 1.0, ..default() });
                line(right, "B or Esc closes the book. C casts the chosen one, V steps the tier.", &fonts.body_italic, 14.0, MUTED);
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleric() -> (Rules, Spellbook) {
        let rules = Rules::default();
        let book = Spellbook::starting_with(&rules, Casting::Divine);
        (rules, book)
    }

    fn start() -> (Rules, Spellbook, Draft) {
        let (rules, book) = cleric();
        let draft = Draft::from_terms(&Written::divine_light());
        (rules, book, draft)
    }

    #[test]
    fn changing_a_term_updates_the_preview() {
        let (rules, book, mut draft) = start();
        let before = draft.as_written(Casting::Divine).preview_with(&rules, 1);
        assert_eq!((before.cost, before.range), (3.0, 20.0), "light around you, short");
        draft.pick(&rules, &book, Casting::Divine, TermKind::Reach, "touch").unwrap();
        let after = draft.as_written(Casting::Divine).preview_with(&rules, 1);
        assert_eq!((after.cost, after.range), (1.0 + 0.0 + 1.0, 5.0), "touch costs nothing and reaches 5 ft");
        assert_ne!(before, after);
        draft.pick(&rules, &book, Casting::Divine, TermKind::Duration, "long").unwrap();
        assert_eq!(draft.as_written(Casting::Divine).preview_with(&rules, 3).duration, 5400.0);
    }

    #[test]
    fn a_locked_term_cannot_be_picked() {
        let (rules, book, mut draft) = start();
        let before = draft.clone();
        for (kind, id) in [(TermKind::Effect, "bane"), (TermKind::Effect, "charm"), (TermKind::Reach, "cone")] {
            assert!(matches!(draft.pick(&rules, &book, Casting::Divine, kind, id), Err(ComposeError::Locked(_))), "{id}");
        }
        assert_eq!(draft, before, "the draft is as it was");
        let rules = fb_character::rules::defaults();
        assert!(unlocked_by(rules, Casting::Divine, "frighten").contains("favored foe"));
        for id in Spellbook::scroll_terms(rules, Casting::Divine) {
            assert_eq!(unlocked_by(rules, Casting::Divine, &id), "unlocked by a prayer scroll", "{id}");
        }
        assert_eq!(unlocked_by(rules, Casting::Divine, "cone"), "unlocked by a prayer scroll");
        assert_eq!(unlocked_by(rules, Casting::Divine, "light"), "not yet unlocked");
    }

    #[test]
    fn an_unlocked_devotion_term_can_be_picked() {
        let (rules, mut book, mut draft) = start();
        book.unlock_with(&rules, "bane");
        draft.pick(&rules, &book, Casting::Divine, TermKind::Effect, "bane").unwrap();
        assert_eq!(draft.effect, "bane");
    }

    #[test]
    fn a_term_in_the_wrong_place_is_refused() {
        let (rules, book, mut draft) = start();
        assert!(matches!(draft.pick(&rules, &book, Casting::Divine, TermKind::Reach, "ward"), Err(ComposeError::WrongKind { .. })));
    }

    #[test]
    fn saving_adds_a_named_prayer() {
        let (rules, mut book, mut draft) = start();
        draft.pick(&rules, &book, Casting::Divine, TermKind::Reach, "touch").unwrap();
        draft.name = "  Hand of Mercy ".into();
        assert_eq!(draft.save(&rules, &mut book, Casting::Divine), Ok(false));
        let written = book.find("Hand of Mercy").expect("kept under its trimmed name");
        assert_eq!((written.effect.as_str(), written.reach.as_str()), ("light", "touch"));
        assert_eq!(book.written.len(), 2, "Divine Light is still there");
        assert_eq!(draft.save(&rules, &mut book, Casting::Divine), Ok(true), "the same name is rewritten");
        assert_eq!(book.written.len(), 2);
    }

    #[test]
    fn an_empty_name_is_refused() {
        let (rules, mut book, mut draft) = start();
        for name in ["", "   "] {
            draft.name = name.into();
            assert_eq!(draft.save(&rules, &mut book, Casting::Divine), Err(ComposeError::Unnamed));
        }
        assert_eq!(book.written.len(), 1);
    }

    #[test]
    fn esc_with_the_field_focused_keeps_the_book_and_the_name() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<KeyboardInput>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<crate::keymap::InputCapture>()
            .insert_resource(ScreenOpen(Some(Screen::Book)))
            .insert_resource(Book { draft: Draft { name: "Hand".into(), ..default() }, focus: true, open: true, ..default() })
            .add_systems(Update, (type_name, own_the_keyboard).chain());
        app.update();
        assert!(app.world().resource::<crate::keymap::InputCapture>().0, "actions stand still while typing");
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
        app.update();
        let book = app.world().resource::<Book>();
        assert!(!book.focus);
        assert_eq!(book.draft.name, "Hand");
        assert!(app.world().resource::<ScreenOpen>().is(Screen::Book));
        assert!(!app.world().resource::<crate::keymap::InputCapture>().0);
    }

    #[test]
    fn a_scroll_term_shows_in_the_picker_locked() {
        let (rules, book) = cleric();
        let ids: Vec<_> = ordered(&rules, &book, Casting::Divine, TermKind::Reach).map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"cone"), "a scroll term shows locked in Reach");
        let effects: Vec<_> = ordered(&rules, &book, Casting::Divine, TermKind::Effect).map(|(id, _)| id.as_str()).collect();
        assert!(effects.contains(&"charm"), "a devotion term shows locked");
        assert_eq!(unlocked_by(&rules, Casting::Divine, "charm"), "unlocked by an alignment");
        assert!(!book.unlocked.contains("cone"), "and stays locked until a scroll is read");
    }

    #[test]
    fn suggested_names_are_seeded() {
        assert_eq!(suggest_name(5), suggest_name(5));
        assert!((0..20).map(|n| suggest_name(name_seed(n))).collect::<std::collections::BTreeSet<_>>().len() > 5);
        assert!((0..200).all(|n| { let name = suggest_name(name_seed(n)); !name.is_empty() && name.chars().count() <= NAME_LENGTH }));
    }

    fn at_the_desk(f: impl FnOnce(&mut Book, &mut Desk)) -> (Book, Spellbook, Chosen, AbilitySlots) {
        let (rules, mut spellbook) = cleric();
        let (mut chosen, mut abilities, mut items) = (Chosen::default(), AbilitySlots::default(), Quickslots::default());
        let mut book = Book { draft: Draft::from_terms(&Written::divine_light()), ..default() };
        f(&mut book, &mut Desk { rules: &rules, school: Casting::Divine, spellbook: &mut spellbook, chosen: &mut chosen, abilities: &mut abilities, items: &mut items });
        (book, spellbook, chosen, abilities)
    }

    #[test]
    fn choosing_a_written_prayer_sets_what_pray_casts() {
        let (_, _, chosen, _) = at_the_desk(|book, desk| {
            act(book, desk, Do::Pick(TermKind::Reach, "touch".into()));
            book.draft.name = "Hand of Mercy".into();
            act(book, desk, Do::Save);
            act(book, desk, Do::Choose("Hand of Mercy".into()));
        });
        assert_eq!(chosen.name, "Hand of Mercy");
    }

    #[test]
    fn slot_cycles_through_free_slots() {
        let (_, _, _, abilities) = at_the_desk(|book, desk| {
            act(book, desk, Do::Slot("Divine Light".into()));
            act(book, desk, Do::Slot("Divine Light".into()));
        });
        assert_eq!(abilities.get(1), Some("Divine Light"));
        assert_eq!(abilities.get(0), None, "it left the first slot");
    }

    #[test]
    fn the_book_notes_why_a_pick_failed_and_clicking_a_name_field_takes_the_focus() {
        let (book, ..) = at_the_desk(|book, desk| {
            act(book, desk, Do::Focus);
            assert!(book.focus);
            act(book, desk, Do::Pick(TermKind::Effect, "bane".into()));
        });
        assert!(!book.focus);
        assert!(book.notice.as_ref().is_some_and(|(t, ok)| !ok && t.contains("unlocked")));
    }
}
