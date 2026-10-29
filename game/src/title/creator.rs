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
    window::PrimaryWindow,
};
use fb_body::{BodyShape, BodySpec, Folk, ShapeSlider};
use procgen_rng::Mulberry32;

use crate::character::{Character, Class, Sex, modifier};
use crate::keymap::{Action, ActionInput};
use crate::launch::clock_seed;
use crate::menu::{ButtonInk, MenuButton, MenuCommand, fraction_along};
use crate::title::NewDelveSeed;
use crate::inventory::doll::doll_leaf;
use crate::inventory::{DollAssets, Icons};
use crate::title::last_delver::LastDelver;
use crate::title::preview::preview_inventory;
use crate::ui::{BG, BRIGHT, CELL, FRAME_GOLD, GOLD, INK, LINE, MUTED, UiFonts, node_rect, ui_cursor};

/// The longest name the creator takes, in characters.
pub const NAME_MAX: usize = 24;

/// The ability scores' names, in the order of the nudges.
const ABILITIES: [&str; 5] = ["Strength", "Dexterity", "Constitution", "Intelligence", "Wisdom"];

/// What a creator button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatorCommand {
    Folk(Folk),
    Sex(Sex),
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
    /// The body's shape: the rolled look's, until a slider moves it.
    pub shape: BodyShape,
    /// Whether a slider has moved the shape; a rolled one is rolled again for a new folk.
    pub shaped: bool,
    pub sex: Sex,
    pub dirt: f32,
    rng: Mulberry32,
}

impl Draft {
    /// A whole delver rolled from `seed`.
    pub fn roll(seed: u32) -> Draft {
        let mut rng = Mulberry32::new(seed.wrapping_mul(0x9e37_79b9) ^ 0x5d1c_7a03);
        let folk = *rng.pick(&Folk::ALL);
        let class = *rng.pick(&Class::ALL);
        let shape = BodySpec::typical(folk).shape();
        let sex = *rng.pick(&Sex::ALL);
        let mut draft = Draft { folk, class, name: String::new(), typed: false, body_seed: 0, nudges: [0; 5], shape, shaped: false, sex, dirt: 0.0, rng };
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
            if !self.shaped {
                self.shape = self.rolled_shape();
            }
        }
    }

    /// The look's shape for the chosen sex: hips and bust within the sex's range, rolled from
    /// the body seed. Lizardfolk have no bust.
    fn rolled_shape(&self) -> BodyShape {
        let mut shape = BodySpec::roll(self.folk, self.body_seed).shape();
        (shape.figure, shape.bust) = fb_body::figure_bust(self.sex, self.folk, self.body_seed);
        shape
    }

    /// The body sliders the chosen sex shows: a woman has no beard to cut.
    pub fn sliders(&self) -> impl Iterator<Item = &'static ShapeSlider> {
        let sex = self.sex;
        BodyShape::SLIDERS.iter().filter(move |s| !(sex == Sex::Female && s.key == "beard"))
    }

    /// Makes them `sex`: the hips and bust take the sex's range, other sliders stay.
    pub fn set_sex(&mut self, sex: Sex) {
        if self.sex != sex {
            self.sex = sex;
            let rolled = self.rolled_shape();
            (self.shape.figure, self.shape.bust) = (rolled.figure, rolled.bust);
        }
    }

    pub fn roll_name(&mut self) {
        self.name = fb_items::naming::folk_name(&mut self.rng, self.folk.key(), Some(self.sex));
        self.name.truncate(self.name.char_indices().nth(NAME_MAX).map_or(self.name.len(), |(i, _)| i));
        self.typed = false;
    }

    pub fn roll_scores(&mut self) {
        self.nudges = Character::roll_nudges(&mut self.rng);
    }

    /// A new body seed, and the shape it rolls.
    pub fn roll_look(&mut self) {
        self.body_seed = (self.rng.next_f64() * 4_294_967_296.0) as u32;
        self.shape = self.rolled_shape();
        self.shaped = false;
    }

    /// Sets the body slider `key` ([`BodyShape::SLIDERS`]) to `value`.
    pub fn set_shape(&mut self, key: &str, value: f32) {
        self.shape.set(key, value);
        self.shaped = true;
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
            shape: Some(self.shape),
            sex: Some(self.sex),
            name: self.name.trim().to_string(),
            class: self.class,
            attributes: Character::scores_from(self.folk, self.nudges),
            dirt: self.dirt,
            worn: default(),
            learned: default(),
            books_read: Vec::new(),
            devotion: None,
            deeds: Default::default(),
        }
    }

    /// Applies a button's command (all but Begin and Back, which [`run_creator`] carries out).
    pub fn apply(&mut self, command: CreatorCommand) {
        match command {
            CreatorCommand::Folk(folk) => self.set_folk(folk),
            CreatorCommand::Sex(sex) => self.set_sex(sex),
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
pub fn begin_delve(commands: &mut Commands, seed: &NewDelveSeed, delver: Character) {
    let seed = seed.0.unwrap_or_else(clock_seed);
    info!("title: {} goes down into level {seed}", delver.name);
    // The level is made off the main thread under the loading screen, and the delve opens on it
    // when it is ready ([`crate::loading`]).
    commands.remove_resource::<Draft>();
    crate::loading::start_delve(commands, seed, 1, None, crate::loading::Opening::New(delver));
}

/// Begins a delve the player chose (the creator's Begin, or Continue as): [`begin_delve`], and
/// when no `--seed` names the level, one that may save and that replaces any saved delve
/// ([`crate::save::begin_players_delve`]).
fn begin_players_delve(commands: &mut Commands, seed: &NewDelveSeed, delver: Character) {
    begin_delve(commands, seed, delver);
    if seed.0.is_none() {
        commands.queue(crate::save::begin_players_delve);
    }
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
) {
    let Some(mut draft) = draft else {
        for command in chosen.read() {
            match command {
                // `FB_NEW_DELVE` begins without the creator, as a delver rolled from the clock.
                MenuCommand::Creator(CreatorCommand::Begin) => begin_delve(&mut commands, &seed, Draft::roll(clock_seed()).character()),
                MenuCommand::Continue => {
                    if let Some(delver) = last.as_ref().and_then(|l| l.0.clone()) {
                        begin_players_delve(&mut commands, &seed, delver);
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
        begin_players_delve(&mut commands, &seed, delver);
    }
}

/// `FB_ROLL_LOOK=<seconds>`: how often the creator rolls a look by itself, for captures of
/// the doll changing.
pub fn roll_look_wait() -> Option<f32> {
    std::env::var("FB_ROLL_LOOK").ok().and_then(|s| s.trim().parse::<f32>().ok())
}

/// Chooses Roll a look every [`roll_look_wait`] seconds while the creator is open.
pub fn roll_look_every(time: Res<Time>, draft: Option<Res<Draft>>, mut waited: Local<f32>, mut chosen: MessageWriter<MenuCommand>) {
    if draft.is_none() {
        return;
    }
    *waited += time.delta_secs();
    if *waited >= roll_look_wait().unwrap_or(f32::MAX) {
        *waited = 0.0;
        chosen.write(MenuCommand::Creator(CreatorCommand::RollLook));
    }
}

/// A body slider's bar ([`BodyShape::SLIDERS`]): a click or drag along it sets it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeBar(pub &'static str);

/// A body slider's filled part, knob and value text, kept up as it is dragged.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeFill(pub &'static str);
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeKnob(pub &'static str);
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeValue(pub &'static str);

/// Set when a body slider moves the draft's shape. Dragging changes the shape without marking
/// the draft changed, so the page is not built again every frame of a drag (it was, and the
/// drag crawled); the sliders move in place ([`show_shape`]) and the paper doll refits.
#[derive(Resource, Default)]
pub struct ShapeMoved(pub bool);

/// A body slider's width in logical pixels.
const SHAPE_PX: f32 = 200.0;

/// A press on a body slider sets it under the mouse and drags it until the button comes up.
/// The bar's place is kept from the press, should the page be built again meanwhile.
pub fn drag_shape(
    input: ActionInput,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    bars: Query<(&ShapeBar, &ComputedNode, &UiGlobalTransform)>,
    mut held: Local<Option<(&'static str, Rect)>>,
    draft: Option<ResMut<Draft>>,
    ui_scale: Option<Res<UiScale>>,
    mut moved: ResMut<ShapeMoved>,
) {
    let Some(mut draft) = draft else {
        *held = None;
        return;
    };
    if !input.pressed(Action::Choose) {
        *held = None;
        return;
    }
    let Some(point) = window.and_then(|w| ui_cursor(&w, ui_scale.as_deref())) else { return };
    if input.just_pressed(Action::Choose) {
        *held = bars.iter().map(|(bar, node, transform)| (bar.0, node_rect(node, transform))).find(|(_, rect)| rect.contains(point));
    }
    if let Some((key, rect)) = *held
        && let Some(slider) = BodyShape::SLIDERS.iter().find(|s| s.key == key)
    {
        let value = slider.low + (slider.high - slider.low) * fraction_along(rect, point.x);
        // In steps of a hundredth of the range, so a still hand does not redress the doll.
        let step = (slider.high - slider.low) / 100.0;
        let value = slider.low + ((value - slider.low) / step).round() * step;
        if draft.shape.get(key) != Some(value) {
            draft.bypass_change_detection().set_shape(key, value);
            moved.0 = true;
        }
    }
}

/// Keeps the body sliders' fills, knobs and values on the draft's shape.
pub fn show_shape(
    draft: Option<Res<Draft>>,
    mut fills: Query<(&ShapeFill, &mut Node), Without<ShapeKnob>>,
    mut knobs: Query<(&ShapeKnob, &mut Node), Without<ShapeFill>>,
    mut values: Query<(&ShapeValue, &mut Text)>,
) {
    let Some(draft) = draft else { return };
    let fraction = |key: &str| {
        let slider = BodyShape::SLIDERS.iter().find(|s| s.key == key);
        slider.map_or(0.0, |s| ((draft.shape.get(key).unwrap_or(s.low) - s.low) / (s.high - s.low)).clamp(0.0, 1.0))
    };
    for (fill, mut node) in &mut fills {
        let width = percent(fraction(fill.0) * 100.0);
        if node.width != width {
            node.width = width;
        }
    }
    for (knob, mut node) in &mut knobs {
        let left = percent(fraction(knob.0) * 100.0);
        if node.left != left {
            node.left = left;
        }
    }
    for (value, mut text) in &mut values {
        if let Some(slider) = BodyShape::SLIDERS.iter().find(|s| s.key == value.0) {
            let shown = shape_value(&draft, slider);
            if text.0 != shown {
                text.0 = shown;
            }
        }
    }
}

/// What a body slider reads: the height in centimetres, the rest as a share.
fn shape_value(draft: &Draft, slider: &ShapeSlider) -> String {
    let value = draft.shape.get(slider.key).unwrap_or(0.0);
    if slider.key == "height" { format!("{:.0} cm", draft.folk.race().h * value * 10.0) } else { format!("{:.0}%", value * 100.0) }
}

/// The words at each end of a body slider.
fn shape_ends(slider: &ShapeSlider) -> (&'static str, &'static str) {
    match slider.key {
        "height" => ("short", "tall"),
        "build" => ("lean", "heavy"),
        "slender" => ("stocky", "slender"),
        "muscle" => ("soft", "strong"),
        "figure" => ("shoulders", "hips"),
        "bust" => ("flat", "full"),
        "beard" => ("none", "long"),
        _ => ("high", "round"),
    }
}

/// The body sliders, a line each: label, the low end's word, the bar, the high end's, the value.
fn shape_sliders(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, draft: &Draft) {
    parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }).with_children(|table| {
        for slider in draft.sliders() {
            let value = draft.shape.get(slider.key).unwrap_or(0.0);
            let fraction = ((value - slider.low) / (slider.high - slider.low)).clamp(0.0, 1.0);
            let (low, high) = shape_ends(slider);
            table.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|line| {
                let cell = |width: f32, justify: JustifyContent| Node { width: px(width), justify_content: justify, ..default() };
                line.spawn((cell(150.0, JustifyContent::Start), children![(Text::new(slider.label), UiFonts::style(&fonts.body, 18.0, INK))]));
                line.spawn((cell(72.0, JustifyContent::End), children![(Text::new(low), UiFonts::style(&fonts.body_italic, 15.0, MUTED))]));
                line.spawn((ShapeBar(slider.key), Node { width: px(SHAPE_PX), height: px(20), align_items: AlignItems::Center, ..default() })).with_children(|bar| {
                    bar.spawn((Node { width: percent(100), height: px(6), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(3)), ..default() }, BackgroundColor(CELL), BorderColor::all(LINE)))
                        .with_child((ShapeFill(slider.key), Node { width: percent(fraction * 100.0), height: percent(100), border_radius: BorderRadius::all(px(3)), ..default() }, BackgroundColor(MUTED)));
                    bar.spawn((
                        ShapeKnob(slider.key),
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
                line.spawn((cell(72.0, JustifyContent::Start), children![(Text::new(high), UiFonts::style(&fonts.body_italic, 15.0, MUTED))]));
                line.spawn((cell(64.0, JustifyContent::End), children![(ShapeValue(slider.key), Text::new(shape_value(draft, slider)), UiFonts::style(&fonts.mono, 16.0, BRIGHT))]));
            });
        }
    });
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

/// Builds the creator's page under `parent`: the form beside the delver's paper doll
/// ([`super::preview`]).
pub fn creator_page(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, draft: &Draft, dolls: &DollAssets, icons: &mut Icons) {
    parent.spawn((Text::new("Who goes below?"), UiFonts::style(&fonts.display, 40.0, INK)));
    let mut body = parent.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(24), width: percent(100), ..default() });
    body.with_children(|body| {
        body.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(16), flex_grow: 1.0, ..default() }).with_children(|form| creator_form(form, fonts, draft));
        doll_leaf(body, dolls, &preview_inventory(draft), fonts, icons);
    });
    parent.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(16), margin: UiRect::top(px(8)), ..default() }).with_children(|buttons| {
        chip(buttons, fonts, "Back", CreatorCommand::Back, false);
        chip(buttons, fonts, "Begin the delve", CreatorCommand::Begin, draft.can_begin());
    });
}

/// The creator's choices, a row each.
fn creator_form(form: &mut ChildSpawnerCommands, fonts: &UiFonts, draft: &Draft) {
    row(form, fonts, "Folk", |line| {
        for folk in Folk::ALL {
            chip(line, fonts, &folk.race().label, CreatorCommand::Folk(folk), draft.folk == folk);
        }
    });
    row(form, fonts, "Sex", |line| {
        for sex in Sex::ALL {
            chip(line, fonts, sex.label(), CreatorCommand::Sex(sex), draft.sex == sex);
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
    row(form, fonts, "Body", |line| shape_sliders(line, fonts, draft));
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

    #[test]
    fn the_body_sliders_shape_the_delver() {
        let mut draft = Draft::roll(8);
        let rolled = |d: &Draft| {
            let spec = d.character().spec();
            let roll = BodySpec::roll(d.folk, d.body_seed);
            (spec.muscle, spec.slender, spec.stature, spec.frame, spec.jaw) == (roll.muscle, roll.slender, roll.stature, roll.frame, roll.jaw)
        };
        assert!(rolled(&draft), "the rolled look to begin with");
        draft.set_shape("muscle", 1.0);
        draft.set_shape("height", 0.9);
        let spec = draft.character().spec();
        assert_eq!((spec.muscle, spec.stature), (1.0, 0.9));
        // A new folk keeps a shape the sliders set; a new look rolls it again.
        draft.set_folk(if draft.folk == Folk::Dwarf { Folk::Gnome } else { Folk::Dwarf });
        assert_eq!(draft.character().spec().muscle, 1.0);
        draft.roll_look();
        assert!(!draft.shaped);
        assert!(rolled(&draft));
        assert!(BodyShape::SLIDERS.iter().all(|s| !shape_value(&draft, s).is_empty()));
    }

    #[test]
    fn the_sex_sets_the_figure_and_the_beard() {
        for seed in 1..20 {
            let mut draft = Draft::roll(seed);
            draft.set_folk(Folk::Human);
            draft.set_sex(Sex::Female);
            let woman = draft.character().spec();
            assert!(woman.figure >= 0.55 && woman.bust >= 0.3 && woman.beard == fb_body::Beard::None, "{woman:?}");
            draft.set_shape("muscle", 0.9);
            draft.set_sex(Sex::Male);
            let man = draft.character().spec();
            assert!(man.figure <= 0.4 && man.bust == 0.0 && man.muscle == 0.9, "the other sliders stay: {man:?}");
            // Rolling a look keeps the sex.
            draft.roll_look();
            assert_eq!((draft.sex, draft.character().spec().bust), (Sex::Male, 0.0));
            draft.set_sex(Sex::Female);
            draft.set_folk(Folk::Lizardfolk);
            draft.roll_look();
            assert_eq!(draft.character().spec().bust, 0.0, "lizardfolk have none");
        }
        // No woman is bearded, a dwarf neither, and she has no Beard slider.
        let mut dwarf = Draft::roll(3);
        dwarf.set_folk(Folk::Dwarf);
        dwarf.set_sex(Sex::Female);
        assert!((1..40).all(|s| {
            dwarf.body_seed = s;
            dwarf.character().spec().beard == fb_body::Beard::None
        }));
        assert!(dwarf.sliders().all(|s| s.key != "beard"));
        // A man's beard follows its slider: none at 0, to mid-chest at 1.
        dwarf.set_sex(Sex::Male);
        assert!(dwarf.sliders().any(|s| s.key == "beard"));
        dwarf.set_shape("beard", 0.0);
        assert_eq!(dwarf.character().spec().beard, fb_body::Beard::None);
        dwarf.set_shape("beard", 1.0);
        assert_eq!(dwarf.character().spec().beard_length, 1.0);
        assert_ne!(dwarf.character().spec().beard, fb_body::Beard::None);
    }

    #[test]
    fn dragging_a_body_slider_moves_it() {
        use crate::keymap::{ActiveContext, Context, Keymap};
        let mut app = App::new();
        app.init_resource::<Keymap>().init_resource::<ButtonInput<MouseButton>>().init_resource::<ShapeMoved>().insert_resource(ActiveContext(Context::Menus)).insert_resource(Draft::roll(8));
        app.add_systems(Update, drag_shape);
        let mut window = Window::default();
        window.set_cursor_position(Some(Vec2::new(150.0, 50.0)));
        let window = app.world_mut().spawn((window, PrimaryWindow)).id();
        // The muscle bar, 200 across from x 100 to 300.
        app.world_mut().spawn((ShapeBar("muscle"), ComputedNode { size: Vec2::new(200.0, 20.0), inverse_scale_factor: 1.0, ..default() }, UiGlobalTransform::from_translation(Vec2::new(200.0, 50.0))));
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        assert!((app.world().resource::<Draft>().shape.muscle - 0.25).abs() < 0.011, "the press sets it under the mouse");
        assert!(app.world().resource::<ShapeMoved>().0 && !app.world().resource_ref::<Draft>().is_changed(), "without building the page again");
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.world_mut().get_mut::<Window>(window).unwrap().set_cursor_position(Some(Vec2::new(250.0, 90.0)));
        app.update();
        assert!((app.world().resource::<Draft>().shape.muscle - 0.75).abs() < 0.011, "and the drag carries it, off the bar too");
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Left);
        app.update();
        app.world_mut().get_mut::<Window>(window).unwrap().set_cursor_position(Some(Vec2::new(120.0, 50.0)));
        app.update();
        assert!((app.world().resource::<Draft>().shape.muscle - 0.75).abs() < 0.011, "let go, it stays");
    }
}
