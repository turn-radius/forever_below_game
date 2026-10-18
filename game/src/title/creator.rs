//! The character creator: New delve on the title opens it over the card, in place of the
//! buttons, as the settings page is. The player picks the delver's folk and class, types a name
//! or rolls one from the folk's syllables, rolls the ability scores and the look, then Begin
//! (or Enter) starts the delve with them; Back (or Esc) returns to the buttons.
//!
//! The scores roll as [`Character::roll`] rolls them: five nudges ([`Character::roll_nudges`])
//! on the folk's scores in `data/races.json`, held within 3 to 18. The [`Draft`] keeps the
//! nudges rather than the scores, so changing folk moves every score by the difference of the
//! two folk's and a good roll stays good. A rolled name is rolled again for a new folk; a typed
//! one is kept.
//!
//! The creator opens on a whole delver rolled from the clock, so New delve then Begin still
//! starts a delve at once.

use bevy::{
    input::{ButtonState, keyboard::{Key, KeyboardInput}},
    prelude::*,
};
use fb_body::Folk;
use procgen_rng::Mulberry32;

use crate::character::{Character, ChosenDelver, Class, modifier};
use crate::keymap::{Action, ActionInput};
use crate::launch::{AppState, clock_seed};
use crate::menu::{ButtonInk, MenuButton, MenuCommand};
use crate::play::CurrentLevel;
use crate::title::NewDelveSeed;
use crate::title::last_delver::LastDelver;
use crate::ui::{BG, BRIGHT, FRAME_GOLD, GOLD, INK, LINE, MUTED, UiFonts};

/// The longest name the creator takes, in characters.
pub const NAME_MAX: usize = 24;

/// The ability scores' names, in the order of the nudges.
const ABILITIES: [&str; 5] = ["Strength", "Dexterity", "Constitution", "Intelligence", "Wisdom"];

/// What a creator button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatorCommand {
    Folk(Folk),
    Class(Class),
    RollName,
    RollScores,
    RollLook,
    Begin,
    Back,
}

/// The delver being made. While it exists the title shows the creator.
#[derive(Resource, Clone, Debug)]
pub struct Draft {
    pub folk: Folk,
    pub class: Class,
    pub name: String,
    /// Whether the player typed the name; a rolled one is rolled again for a new folk.
    pub typed: bool,
    /// The seed their body is rolled from ([`fb_body::BodySpec::roll`]).
    pub body_seed: u32,
    /// What each ability is moved from the folk's score, in [`ABILITIES`]' order.
    pub nudges: [i32; 5],
    pub dirt: f32,
    rng: Mulberry32,
}

impl Draft {
    /// A whole delver rolled from `seed`.
    pub fn roll(seed: u32) -> Draft {
        let mut rng = Mulberry32::new(seed.wrapping_mul(0x9e37_79b9) ^ 0x5d1c_7a03);
        let folk = *rng.pick(&Folk::ALL);
        let class = *rng.pick(&Class::ALL);
        let mut draft = Draft { folk, class, name: String::new(), typed: false, body_seed: 0, nudges: [0; 5], dirt: 0.0, rng };
        draft.roll_name();
        draft.roll_scores();
        draft.roll_look();
        draft.dirt = 0.3 + draft.rng.next_f64() as f32 * 0.2;
        draft
    }

    /// Makes them one of `folk`, keeping their nudges; a rolled name is rolled for the folk.
    pub fn set_folk(&mut self, folk: Folk) {
        if self.folk != folk {
            self.folk = folk;
            if !self.typed {
                self.roll_name();
            }
        }
    }

    pub fn roll_name(&mut self) {
        self.name = fb_items::naming::folk_name(&mut self.rng, self.folk.key());
        self.name.truncate(self.name.char_indices().nth(NAME_MAX).map_or(self.name.len(), |(i, _)| i));
        self.typed = false;
    }

    pub fn roll_scores(&mut self) {
        self.nudges = Character::roll_nudges(&mut self.rng);
    }

    pub fn roll_look(&mut self) {
        self.body_seed = (self.rng.next_f64() * 4_294_967_296.0) as u32;
    }

    /// Adds typed text to the name, up to [`NAME_MAX`] characters; control characters are
    /// dropped.
    pub fn type_text(&mut self, text: &str) {
        for c in text.chars().filter(|c| !c.is_control()) {
            if self.name.chars().count() >= NAME_MAX {
                break;
            }
            // Typing into a rolled name starts a new one.
            if !self.typed {
                self.name.clear();
                self.typed = true;
            }
            self.name.push(c);
        }
    }

    pub fn backspace(&mut self) {
        self.typed = true;
        self.name.pop();
    }

    /// Whether they can begin: a name with something in it.
    pub fn can_begin(&self) -> bool {
        !self.name.trim().is_empty()
    }

    /// The delver they make.
    pub fn character(&self) -> Character {
        Character {
            folk: self.folk,
            body_seed: self.body_seed,
            name: self.name.trim().to_string(),
            class: self.class,
            attributes: Character::scores_from(self.folk, self.nudges),
            dirt: self.dirt,
            worn: default(),
        }
    }

    /// Applies a button's command (all but Begin and Back, which [`run_creator`] carries out).
    pub fn apply(&mut self, command: CreatorCommand) {
        match command {
            CreatorCommand::Folk(folk) => self.set_folk(folk),
            CreatorCommand::Class(class) => self.class = class,
            CreatorCommand::RollName => self.roll_name(),
            CreatorCommand::RollScores => self.roll_scores(),
            CreatorCommand::RollLook => self.roll_look(),
            CreatorCommand::Begin | CreatorCommand::Back => {}
        }
    }
}

/// Opens the creator on a delver rolled from the clock.
pub fn open_creator(commands: &mut Commands) {
    commands.insert_resource(Draft::roll(clock_seed()));
}

/// Begins a delve as `delver`, into `seed`'s level or one from the clock.
pub fn begin_delve(commands: &mut Commands, next: &mut NextState<AppState>, seed: &NewDelveSeed, delver: Character) {
    let seed = seed.0.unwrap_or_else(clock_seed);
    info!("title: {} goes down into level {seed}", delver.name);
    commands.insert_resource(CurrentLevel(fb_level::generate(seed, 1)));
    commands.insert_resource(ChosenDelver(delver));
    commands.remove_resource::<Draft>();
    next.set(AppState::Delving);
}

/// Carries out the creator's buttons, the typed name, Enter (Begin) and Esc (Back), and the
/// title's Continue. Beginning from the creator or continuing remembers the delver as the
/// [`LastDelver`].
#[allow(clippy::too_many_arguments)]
pub fn run_creator(
    mut commands: Commands,
    draft: Option<ResMut<Draft>>,
    last: Option<ResMut<LastDelver>>,
    input: ActionInput,
    mut chosen: MessageReader<MenuCommand>,
    mut typed: MessageReader<KeyboardInput>,
    seed: Res<NewDelveSeed>,
    mut next: ResMut<NextState<AppState>>,
) {
    let Some(mut draft) = draft else {
        for command in chosen.read() {
            match command {
                // `FB_NEW_DELVE` begins without the creator, as a delver rolled from the clock.
                MenuCommand::Creator(CreatorCommand::Begin) => begin_delve(&mut commands, &mut next, &seed, Draft::roll(clock_seed()).character()),
                MenuCommand::Continue => {
                    if let Some(delver) = last.as_ref().and_then(|l| l.0.clone()) {
                        begin_delve(&mut commands, &mut next, &seed, delver);
                    }
                }
                _ => {}
            }
        }
        typed.clear();
        return;
    };
    // The press that opened the creator is not its own.
    let fresh = draft.is_added();
    let mut begin = !fresh && input.just_pressed(Action::NewDelve);
    let mut back = !fresh && input.just_pressed(Action::Back);
    if fresh {
        typed.clear();
    }
    for command in chosen.read() {
        let MenuCommand::Creator(command) = *command else { continue };
        match command {
            CreatorCommand::Begin => begin = true,
            CreatorCommand::Back => back = true,
            other => draft.apply(other),
        }
    }
    for key in typed.read().filter(|k| k.state == ButtonState::Pressed) {
        match &key.logical_key {
            Key::Backspace => draft.backspace(),
            Key::Enter | Key::Escape | Key::Tab => {}
            _ => {
                if let Some(text) = &key.text {
                    draft.type_text(text);
                }
            }
        }
    }
    if back {
        commands.remove_resource::<Draft>();
    } else if begin && draft.can_begin() {
        let delver = draft.character();
        if let Some(mut last) = last {
            last.0 = Some(delver.clone());
        }
        begin_delve(&mut commands, &mut next, &seed, delver);
    }
}

/// A choice among several: gold and ruled in gold when it is the one chosen.
fn chip(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, command: CreatorCommand, chosen: bool) {
    let idle = if chosen { FRAME_GOLD } else { MUTED };
    parent
        .spawn((
            MenuButton(MenuCommand::Creator(command)),
            ButtonInk { idle, hover: INK },
            Node { padding: UiRect::axes(px(12), px(4)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(4)), ..default() },
            BackgroundColor(BG),
            BorderColor::all(if chosen { FRAME_GOLD } else { LINE }),
        ))
        .with_child((Text::new(label), UiFonts::style(&fonts.body_medium, 20.0, idle)));
}

/// A row of the form: its label on the left, its controls after.
fn row(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, controls: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(10), ..default() }).with_children(|line| {
        line.spawn((Node { width: px(96), flex_shrink: 0.0, ..default() }, children![(Text::new(label), UiFonts::style(&fonts.display, 22.0, MUTED))]));
        line.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, column_gap: px(8), row_gap: px(8), ..default() })
            .with_children(controls);
    });
}

/// How the class casts and how tough it is, in a few words.
fn class_note(class: Class) -> String {
    let casting = match class.casting() {
        fb_character::Casting::Martial => "no spells to speak of",
        fb_character::Casting::Arcane => "casts from INT",
        fb_character::Casting::Divine => "casts from WIS",
    };
    format!("d{} hit die, {casting}", class.hit_die())
}

/// Builds the creator's page under `parent`.
pub fn creator_page(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, draft: &Draft) {
    parent.spawn((Text::new("Who goes below?"), UiFonts::style(&fonts.display, 40.0, INK)));
    parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(16), width: percent(100), ..default() }).with_children(|form| {
        row(form, fonts, "Folk", |line| {
            for folk in Folk::ALL {
                chip(line, fonts, &folk.race().label, CreatorCommand::Folk(folk), draft.folk == folk);
            }
        });
        row(form, fonts, "Class", |line| {
            for class in Class::ALL {
                let word = class.word();
                let label = word[..1].to_uppercase() + &word[1..];
                chip(line, fonts, &label, CreatorCommand::Class(class), draft.class == class);
            }
            line.spawn((Text::new(class_note(draft.class)), UiFonts::style(&fonts.body_italic, 18.0, MUTED)));
        });
        row(form, fonts, "Name", |line| {
            line.spawn((
                Node { width: px(300), padding: UiRect::axes(px(10), px(4)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(4)), ..default() },
                BackgroundColor(BG),
                BorderColor::all(if draft.can_begin() { LINE } else { GOLD }),
            ))
            .with_child((Text::new(draft.name.clone()), UiFonts::style(&fonts.display, 24.0, BRIGHT), children![(TextSpan::new("_"), UiFonts::style(&fonts.display, 24.0, GOLD))]));
            chip(line, fonts, "Roll a name", CreatorCommand::RollName, false);
        });
        row(form, fonts, "Scores", |line| {
            line.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }).with_children(|table| {
                let scores = Character::scores_from(draft.folk, draft.nudges);
                let values = [scores.strength, scores.dexterity, scores.constitution, scores.intelligence, scores.wisdom];
                for (i, ability) in ABILITIES.into_iter().enumerate() {
                    let base = draft.folk.race().stats[i];
                    table.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Baseline, column_gap: px(14), ..default() }).with_children(|line| {
                        let cell = |width: f32| Node { width: px(width), ..default() };
                        line.spawn((cell(130.0), children![(Text::new(ability), UiFonts::style(&fonts.body, 20.0, INK))]));
                        line.spawn((cell(36.0), children![(Text::new(format!("{:>2}", values[i])), UiFonts::style(&fonts.mono_bold, 20.0, BRIGHT))]));
                        line.spawn((cell(40.0), children![(Text::new(format!("{:+}", modifier(values[i]))), UiFonts::style(&fonts.mono, 18.0, MUTED))]));
                        let note = format!("{} {base}, rolled {:+}", draft.folk.race().label.to_lowercase(), draft.nudges[i]);
                        line.spawn((Text::new(note), UiFonts::style(&fonts.body_italic, 16.0, MUTED)));
                    });
                }
            });
            chip(line, fonts, "Roll scores", CreatorCommand::RollScores, false);
        });
        row(form, fonts, "Look", |line| chip(line, fonts, "Roll a look", CreatorCommand::RollLook, false));
    });
    parent.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(16), margin: UiRect::top(px(8)), ..default() }).with_children(|buttons| {
        chip(buttons, fonts, "Back", CreatorCommand::Back, false);
        chip(buttons, fonts, "Begin the delve", CreatorCommand::Begin, draft.can_begin());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_drafts_the_same_delver() {
        let (a, b) = (Draft::roll(9), Draft::roll(9));
        assert_eq!(a.character(), b.character());
        assert!(a.can_begin());
        assert_ne!(Draft::roll(9).character(), Draft::roll(10).character());
    }

    #[test]
    fn a_new_folk_keeps_the_nudges() {
        let mut draft = Draft::roll(3);
        draft.set_folk(Folk::Human);
        draft.nudges = [4, -5, 0, 2, -1];
        let human = draft.character().attributes;
        draft.set_folk(Folk::Goblin);
        let goblin = draft.character().attributes;
        assert_eq!(draft.nudges, [4, -5, 0, 2, -1]);
        assert_eq!(goblin, Character::scores_from(Folk::Goblin, draft.nudges));
        let (h, g) = (Folk::Human.race().stats, Folk::Goblin.race().stats);
        assert_eq!(goblin.constitution - human.constitution, g[2] - h[2], "a nudge of 0 moves with the folk's own score");
        for nudges in [[9; 5], [-20; 5]] {
            let scores = Character::scores_from(Folk::Dwarf, nudges);
            for value in [scores.strength, scores.dexterity, scores.constitution, scores.intelligence, scores.wisdom] {
                assert!((3..=18).contains(&value), "{value}");
            }
        }
    }

    #[test]
    fn rolled_names_follow_the_folk_and_typed_ones_stay() {
        let mut draft = Draft::roll(5);
        draft.set_folk(Folk::Human);
        let rolled = draft.name.clone();
        draft.set_folk(Folk::Dwarf);
        assert_ne!(draft.name, rolled, "a rolled name is rolled again");
        draft.type_text("Brakka");
        assert_eq!(draft.name, "Brakka", "typing replaces a rolled name");
        draft.set_folk(Folk::Gnome);
        assert_eq!(draft.name, "Brakka");
        draft.backspace();
        assert_eq!(draft.name, "Brakk");
        draft.type_text(&"x".repeat(40));
        assert_eq!(draft.name.chars().count(), NAME_MAX);
    }

    #[test]
    fn an_empty_name_cannot_begin() {
        let mut draft = Draft::roll(1);
        draft.type_text(" ");
        assert!(!draft.can_begin());
        draft.backspace();
        assert!(!draft.can_begin());
    }

    #[test]
    fn rolling_scores_or_look_changes_only_that() {
        let mut draft = Draft::roll(8);
        let before = draft.character();
        let mut changed = false;
        for _ in 0..4 {
            draft.roll_scores();
            changed |= draft.character().attributes != before.attributes;
        }
        assert!(changed);
        assert_eq!((draft.character().body_seed, draft.character().name), (before.body_seed, before.name.clone()));
        let attributes = draft.character().attributes;
        draft.roll_look();
        assert_ne!(draft.body_seed, before.body_seed);
        assert_eq!(draft.character().attributes, attributes);
    }
}
