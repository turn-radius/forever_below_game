//! The lab (`--lab`): a designer's tool for balancing the character rules, laid out as a
//! character sheet in three columns. On the left is the sheet itself ([`crate::lab_sheet`]):
//! the figure in the gear it wears, folk, class and seed, the five attributes, the bars, and
//! Armour, Damage and Load as large numbers whose tooltips list the terms they are made of,
//! then a card for each thing worn or held. In the middle are the tests to run on the
//! character, in tabs: the fight ([`crate::lab_strike`]) and passing time ([`crate::lab_time`]),
//! with room for more. On the right is the rules column, for the numbers the game runs on.
//!
//! No level, creatures or audio load: the lab only calls the rules the game does
//! ([`fb_character`], [`crate::character`], [`crate::combat`]) under the loaded
//! [`crate::rules::GameRules`], so what is balanced here is what the game plays. The sheet is
//! built again only when the state, the time, the strikes, the rules or the picker change.
//! Command line: `--lab --seed 7` rolls the character from the seed; with none it opens on a
//! human Fighter with every score 10.

use std::collections::BTreeSet;

use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    window::{PrimaryWindow, WindowResizeConstraints},
};
use fb_body::Folk;
use fb_character::explain::number;
use fb_character::{Activity, Attributes, MeterKind};
use fb_creatures::Species;
use fb_items::armory_base::{ARMORY_BASES, roll_shape};
use fb_items::profile::profile;
use fb_items::tables::{Affix, bases};
use fb_items::{Item, Rarity, Slot, roll_item};
use procgen_rng::Mulberry32;

use crate::character::{Character, Class};
use crate::inventory::Inventory;
use crate::lab_figure::LabFigure;
use crate::lab_rules::{DicePart, FieldKind, RuleSet, Strengths, default_strength, strengthened};
use crate::lab_sheet::{self, GearCard, SheetView, signed};
use crate::lab_chart::Chart;
use crate::lab_fight::{FightRecord, FightSummary, THOUSAND};
use crate::lab_strike::Attacker;
use crate::lab_time::{ACTIVITIES, TimeLab, activity_word, clock_text};
use crate::lab_timeline::{Goal, Step, Timeline};
use crate::rules::GameRules;
use crate::ui::{ACCENT, BAD, OK, BG, BRIGHT, CELL, FAINT, FRAME_GOLD, INK, LINE, MUTED, UiFonts, node_rect, rarity_color};

/// Width of the sheet on the left and of the rules on the right, in logical pixels.
pub const SHEET_WIDTH: f32 = 380.0;
pub const RULES_WIDTH: f32 = 340.0;
/// The smallest window the lab is laid out for: the sheet, the rules and a middle column wide
/// enough for the tabs.
pub const MIN_WINDOW: Vec2 = Vec2::new(1120.0, 630.0);
const DEFAULT_SEED: u32 = 1207;
/// Pixels one scroll line moves a column.
const SCROLL_LINE: f32 = 36.0;
/// The bars' colours: life, stamina and mana.
pub const LIFE: Color = Color::srgb(0.71, 0.28, 0.25);
pub const STAMINA: Color = Color::srgb(0.79, 0.64, 0.23);
pub const MANA: Color = Color::srgb(0.29, 0.47, 0.74);
/// The page behind the rules column, a shade darker than the sheet's.
const RULES_PAGE: Color = Color::srgb(0.086, 0.075, 0.059);
/// The page behind a tile or a card, and behind the tab that is open.
const OPEN_TAB: Color = Color::srgb(0.14, 0.12, 0.085);
/// The slots the lab dresses. The hip is left out, since a lantern hangs there from a belt.
pub const SLOTS: [Slot; 15] = [
    Slot::Head,
    Slot::Neck,
    Slot::Shirt,
    Slot::Cloak,
    Slot::Body,
    Slot::Mainhand,
    Slot::Offhand,
    Slot::Hands,
    Slot::Belt,
    Slot::Legs,
    Slot::Feet,
    Slot::Back,
    Slot::Ring1,
    Slot::Ring2,
    Slot::Charm,
];
/// The tabs in the middle and whether each can be opened yet.
pub const TABS: [(&str, bool); 4] = [("Fight", true), ("Time and needs", true), ("Eat, drink, equip", true), ("Effects", false)];

/// The window the lab opens in, no smaller than [`MIN_WINDOW`].
pub fn window() -> Window {
    Window {
        title: "Forever Below".into(),
        resize_constraints: WindowResizeConstraints { min_width: MIN_WINDOW.x, min_height: MIN_WINDOW.y, ..default() },
        ..default()
    }
}

pub struct LabViewPlugin;

impl Plugin for LabViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LabState>()
            .init_resource::<UiFonts>()
            .init_resource::<SheetBuilds>()
            .init_resource::<FightLab>()
            .init_resource::<FightImages>()
            .init_resource::<TimeLab>()
            .init_resource::<TimeRun>()
            .init_resource::<EquipBuilds>()
            .init_resource::<TimeImage>()
            .init_resource::<LabTab>()
            .init_resource::<Picker>()
            .init_resource::<LabFigure>()
            .init_resource::<Hover>()
            .init_resource::<CurrentSheet>()
            .init_resource::<LabRules>()
            .init_resource::<DataFolder>()
            .init_resource::<SaveNote>()
            .add_message::<ControlPressed>()
            .insert_resource(ClearColor(BG))
            .add_systems(Startup, (setup, demo, make_chart_images, make_time_image))
            .add_systems(
                Update,
                (
                    follow_disk,
                    press_controls,
                    apply_pressed,
                    show_save_note,
                    rebuild_sheet.run_if(sheet_changed),
                    rebuild_rules.run_if(|state: Res<LabState>, rules: Res<LabRules>| state.is_changed() || rules.is_changed()),
                    run_fights,
                    refresh_fight.run_if(resource_changed::<FightLab>),
                    rebuild_equip.run_if(resource_changed::<SheetBuilds>),
                    run_time,
                    refresh_time_view.run_if(resource_changed::<TimeRun>),
                    refresh_controls.run_if(|state: Res<LabState>| state.is_changed()),
                    show_tabs.run_if(resource_changed::<LabTab>),
                    (hover_tiles, show_tip).chain(),
                    scroll_columns,
                )
                    .chain(),
            );
    }
}

/// What one equip slot holds: a base rolled from a seed, with the affixes switched off.
#[derive(Clone, Debug, PartialEq)]
pub struct LabSlot {
    pub slot: Slot,
    /// The base name, or `None` for an empty slot.
    pub base: Option<String>,
    pub seed: u32,
    /// Names of the rolled affixes switched off.
    pub off: BTreeSet<String>,
    /// Whether a torch or lantern burns.
    pub lit: bool,
}

/// An item as the lab wears it, with every affix it rolled.
#[derive(Clone, Debug, PartialEq)]
pub struct Rolled {
    /// The item with its stats and name derived without the switched-off affixes.
    pub item: Item,
    /// Every affix it rolled, in order, the switched-off ones too.
    pub affixes: Vec<&'static Affix>,
}

impl LabSlot {
    fn empty(slot: Slot) -> LabSlot {
        LabSlot { slot, base: None, seed: 1, off: BTreeSet::new(), lit: true }
    }

    /// The item this slot holds: rolled from its base and seed, then its stats and name
    /// derived again from the base profile with only the affixes still on. With none off the
    /// item is exactly [`roll_item`]'s. Switching an affix off changes only those two: the
    /// rarity and a legendary's title and history stay as rolled, and so does the shape's
    /// dressing.
    pub fn roll(&self) -> Option<Rolled> {
        self.roll_with(&Strengths::new())
    }

    /// [`LabSlot::roll`] with the affixes at `strengths` (an affix not named has its own).
    pub fn roll_with(&self, strengths: &Strengths) -> Option<Rolled> {
        let base = self.base.as_deref()?;
        let mut item = roll_item(base, self.seed)?;
        let affixes = item.affixes.clone();
        let stronger = affixes.iter().any(|a| strengths.get(&a.name).is_some_and(|s| *s != default_strength(a)));
        if !self.off.is_empty() || stronger {
            let mut stats = profile(base, &roll_shape(base, self.seed)?).stats;
            item.affixes.retain(|a| !self.off.contains(&a.name));
            item.affixes.iter().for_each(|a| strengthened(a, strengths).apply_to_stats(&mut stats));
            item.shape.scale_fuel(stats.burn_time);
            item.profile.stats = stats;
            item.name = fb_items::naming::item_name(&fb_items::naming::noun(base, &item.shape), &item.affixes);
        }
        let _ = item.shape.set_lit(self.lit);
        Some(Rolled { item, affixes })
    }
}

/// The delver being tried: who they are and what each slot holds.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct LabState {
    pub character: Character,
    /// The seed the character was last rolled from.
    pub seed: u32,
    pub slots: Vec<LabSlot>,
    /// What the character is doing while time passes ([`crate::lab_time`]); the meter rates
    /// follow it.
    pub activity: Activity,
    /// Who strikes the character ([`crate::lab_strike`]).
    pub attacker: Attacker,
    /// The seed every fight is rolled from.
    pub strike_seed: u32,
    /// Whether the character strikes first in a round, as the attacker does by default.
    pub swapped: bool,
    /// The rules and affix strengths in force: what the lab is editing, which the sheet, the
    /// strikes and the time all read.
    pub ruleset: RuleSet,
}

impl Default for LabState {
    /// A human Fighter with every score 10, holding a messer and wearing a sallet and a mail
    /// hauberk, none of which rolled an affix, so the first view has something to read.
    fn default() -> Self {
        let attributes = Attributes { strength: 10, dexterity: 10, constitution: 10, intelligence: 10, wisdom: 10 };
        let character = Character { folk: Folk::Human, body_seed: DEFAULT_SEED, name: "Lab delver".into(), class: Class::Fighter, attributes, dirt: 0.0 };
        let mut state = LabState { character, seed: DEFAULT_SEED, slots: SLOTS.iter().map(|&s| LabSlot::empty(s)).collect(), activity: Activity::Walking, attacker: Attacker::Creature(Species::GiantRat), strike_seed: 1, swapped: false, ruleset: RuleSet::default() };
        for (slot, base, seed) in [(Slot::Mainhand, "Sword", 4), (Slot::Head, "Helm", 9), (Slot::Body, "Mail", 9)] {
            state.equip(slot, base, seed);
        }
        state
    }
}

/// The scores a character can have in the lab.
const SCORES: std::ops::RangeInclusive<i32> = 3..=20;

impl LabState {
    /// Parses `--seed <n>` (also `--seed=n`); `--lab` itself is skipped.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<LabState, String> {
        let mut state = LabState::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (arg, None),
            };
            match flag.as_str() {
                "--lab" => {}
                "--seed" => {
                    let text = inline.or_else(|| args.next()).ok_or("--seed needs a value")?;
                    state.set_seed(text.parse().map_err(|_| format!("--seed: {text:?} is not a u32"))?);
                }
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        Ok(state)
    }

    /// Rolls the class and the scores again from `seed`, keeping the folk the lab chose.
    pub fn set_seed(&mut self, seed: u32) {
        self.seed = seed;
        self.character = Character::roll_folk(self.character.folk, seed);
    }

    /// Rolls the next seed's class and scores.
    pub fn reroll(&mut self) {
        self.set_seed(self.seed.wrapping_add(1));
    }

    pub fn slot(&self, slot: Slot) -> &LabSlot {
        self.slots.iter().find(|s| s.slot == slot).expect("every lab slot is listed")
    }

    fn slot_mut(&mut self, slot: Slot) -> &mut LabSlot {
        self.slots.iter_mut().find(|s| s.slot == slot).expect("every lab slot is listed")
    }

    /// Puts `base` (rolled from `seed`) in `slot`, with every affix on.
    pub fn equip(&mut self, slot: Slot, base: &str, seed: u32) {
        *self.slot_mut(slot) = LabSlot { base: Some(base.to_string()), seed, ..LabSlot::empty(slot) };
    }

    /// Takes whatever is in `slot` off.
    pub fn remove(&mut self, slot: Slot) {
        *self.slot_mut(slot) = LabSlot::empty(slot);
    }

    /// Switches the affix named `affix` on `slot`'s item on or off.
    pub fn set_affix(&mut self, slot: Slot, affix: &str, on: bool) {
        let s = self.slot_mut(slot);
        if on {
            s.off.remove(affix);
        } else {
            s.off.insert(affix.to_string());
        }
    }

    /// Flips the affix named `affix` on `slot`'s item.
    pub fn flip_affix(&mut self, slot: Slot, affix: &str) {
        let on = self.slot(slot).off.contains(affix);
        self.set_affix(slot, affix, on);
    }

    /// Sets the `which`th score (STR, DEX, CON, INT, WIS) to `score`, kept within 3 to 20.
    pub fn set_score(&mut self, which: usize, score: i32) {
        let a = &mut self.character.attributes;
        let score = score.clamp(*SCORES.start(), *SCORES.end());
        match which {
            0 => a.strength = score,
            1 => a.dexterity = score,
            2 => a.constitution = score,
            3 => a.intelligence = score,
            4 => a.wisdom = score,
            _ => {}
        }
    }

    /// Raises (or with a negative `by`, lowers) the `which`th score.
    pub fn step_score(&mut self, which: usize, by: i32) {
        let a = &self.character.attributes;
        let now = [a.strength, a.dexterity, a.constitution, a.intelligence, a.wisdom].get(which).copied().unwrap_or(10);
        self.set_score(which, now + by);
    }

    /// Steps `by` folk through [`Folk::ALL`], wrapping.
    pub fn cycle_folk(&mut self, by: isize) {
        let at = Folk::ALL.iter().position(|&f| f == self.character.folk).unwrap_or(0);
        self.character.folk = Folk::ALL[step(Folk::ALL.len(), at, by)];
    }

    /// Steps `by` classes through [`Class::ALL`], wrapping.
    pub fn cycle_class(&mut self, by: isize) {
        let at = Class::ALL.iter().position(|&c| c == self.character.class).unwrap_or(0);
        self.character.class = Class::ALL[step(Class::ALL.len(), at, by)];
    }

    /// Steps `by` activities through [`ACTIVITIES`], wrapping.
    pub fn cycle_activity(&mut self, by: isize) {
        let at = ACTIVITIES.iter().position(|&a| a == self.activity).unwrap_or(0);
        self.activity = ACTIVITIES[step(ACTIVITIES.len(), at, by)];
    }

    /// Steps `by` attackers through [`Attacker::all`], wrapping.
    pub fn cycle_attacker(&mut self, by: isize) {
        let all = Attacker::all();
        let at = all.iter().position(|a| *a == self.attacker).unwrap_or(0);
        self.attacker = all[step(all.len(), at, by)].clone();
    }

    /// Everything worn, rolled.
    pub fn items(&self) -> Vec<(Slot, Rolled)> {
        self.slots.iter().filter_map(|s| Some((s.slot, s.roll_with(&self.ruleset.strengths)?))).collect()
    }

    /// What the worn items make an [`Inventory`]: each in its slot, so the game's own rules
    /// read it. An item the slot refuses is left out.
    pub fn inventory(&self) -> Inventory {
        let mut inventory = Inventory::default();
        for (slot, rolled) in self.items() {
            let id = inventory.add(rolled.item);
            let _ = inventory.wear(id, slot);
        }
        inventory
    }
}

/// `at` moved `by` places along a ring of `len`.
fn step(len: usize, at: usize, by: isize) -> usize {
    (at as isize + by).rem_euclid(len as isize) as usize
}

/// The bases a slot can hold, by name: the ones that roll an item that fits it.
pub fn bases_for(slot: Slot) -> Vec<String> {
    use std::sync::LazyLock;
    static TABLE: LazyLock<Vec<(Slot, Vec<String>)>> = LazyLock::new(|| {
        let mut names: BTreeSet<&str> = ARMORY_BASES.iter().map(|(n, _)| *n).collect();
        names.extend(bases().keys().map(String::as_str));
        let rolled: Vec<(&str, Item)> = names.into_iter().filter_map(|n| Some((n, roll_item(n, 1)?))).collect();
        SLOTS.iter().map(|&slot| (slot, rolled.iter().filter(|(_, i)| i.fits(slot)).map(|(n, _)| n.to_string()).collect())).collect()
    });
    TABLE.iter().find(|(s, _)| *s == slot).map(|(_, b)| b.clone()).unwrap_or_default()
}

/// The tab open in the middle, by its place in [`TABS`].
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LabTab(pub usize);

/// The "Put on or hold an item" card: the slot, base and seed being chosen.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Picker {
    pub open: bool,
    /// Index into [`SLOTS`].
    pub slot: usize,
    /// Index into the slot's [`bases_for`].
    pub base: usize,
    pub seed: u32,
    /// The rarity wanted: the first seed from `seed` on that rolls it is used. `None` takes the seed as it is.
    pub rarity: Option<Rarity>,
}

/// How many seeds are tried to find one of a chosen rarity.
pub const RARITY_SEARCH: u32 = 5000;

impl Default for Picker {
    fn default() -> Self {
        Picker { open: false, slot: 0, base: 0, seed: 1, rarity: None }
    }
}

impl Picker {
    pub fn slot(&self) -> Slot {
        SLOTS[self.slot % SLOTS.len()]
    }

    /// The base chosen, if the slot takes any.
    pub fn base_name(&self) -> Option<String> {
        let bases = bases_for(self.slot());
        (!bases.is_empty()).then(|| bases[self.base % bases.len()].clone())
    }

    pub fn step_slot(&mut self, by: isize) {
        self.slot = step(SLOTS.len(), self.slot % SLOTS.len(), by);
        self.base = 0;
    }

    pub fn step_base(&mut self, by: isize) {
        let len = bases_for(self.slot()).len();
        if len > 0 {
            self.base = step(len, self.base % len, by);
        }
    }

    pub fn step_seed(&mut self, by: i64) {
        self.seed = (self.seed as i64 + by).clamp(0, u32::MAX as i64) as u32;
    }

    /// The seed the choice rolls from: the seed itself, or with a rarity chosen the first seed
    /// from it on (within [`RARITY_SEARCH`]) that rolls that rarity.
    pub fn resolved_seed(&self) -> Option<u32> {
        let base = self.base_name()?;
        match self.rarity {
            None => Some(self.seed),
            Some(rarity) => (0..RARITY_SEARCH).map(|i| self.seed.wrapping_add(i)).find(|&s| roll_item(&base, s).is_some_and(|item| item.rarity == rarity)),
        }
    }

    /// The item the choice rolls, to preview.
    pub fn preview(&self) -> Option<Item> {
        roll_item(&self.base_name()?, self.resolved_seed()?)
    }

    /// Puts the choice on, or into the hand, and closes the card.
    pub fn put_on(&mut self, state: &mut LabState) {
        if let (Some(base), Some(seed)) = (self.base_name(), self.resolved_seed()) {
            state.equip(self.slot(), &base, seed);
        }
        self.open = false;
    }
}

/// The rules as last saved to the files, which the edited ones in [`LabState::ruleset`] are
/// compared with.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct LabRules {
    pub saved: RuleSet,
}

/// How many times the sheet has been built, so a test can see it is not rebuilt for nothing.
#[derive(Resource, Default)]
pub struct SheetBuilds(pub u32);

/// The sheet as last built, so a tooltip reads the numbers that are on screen.
#[derive(Resource, Default)]
pub struct CurrentSheet(pub Option<SheetView>);

/// The big number the mouse is over, by its place among the sheet's tiles, or one a demo holds
/// up for a capture.
#[derive(Resource, Default, Clone, Copy, PartialEq)]
pub struct Hover {
    pub tile: Option<usize>,
    /// The played-out fight's line the mouse is over.
    pub line: Option<usize>,
    pinned: Option<usize>,
}

impl Hover {
    /// Holds the `tile`th big number's tooltip up whether or not the mouse is over it.
    pub fn pin(&mut self, tile: Option<usize>) {
        self.pinned = tile;
    }
}

/// What a click on a control does.
#[derive(Component, Clone, Debug, PartialEq)]
pub enum Control {
    Folk(isize),
    Class(isize),
    Reroll,
    Score(usize, i32),
    Remove(Slot),
    Affix(Slot, String),
    OpenPicker,
    ClosePicker,
    PickSlot(isize),
    PickBase(isize),
    PickSeed(i64),
    PutOn,
    /// Picks the slot, the base (by its place among the slot's bases) or the rarity wanted.
    PickSlotAt(usize),
    PickBaseAt(usize),
    PickRarity(Option<Rarity>),
    Tab(usize),
    Attacker(isize),
    StrikeSeed(i64),
    Activity(isize),
    /// Has the character strike first.
    Swap,
    /// Adds to the Time tab's run.
    Time(TimeAction),
    /// Plays one fight, or a thousand.
    OneFight,
    Thousand,
    /// Steps a number of the rules, or flips a switch.
    Rule(String, i32),
    Dice(String, DicePart, i32),
    Save,
    Undo,
}

impl Control {
    /// Does what the control says to the lab, changing only what it needs to.
    pub fn apply(&self, state: &mut LabState, picker: &mut Picker, tab: &mut LabTab, saved: &mut RuleSet) {
        match self {
            Control::Folk(by) => state.cycle_folk(*by),
            Control::Class(by) => state.cycle_class(*by),
            Control::Reroll => state.reroll(),
            Control::Score(which, by) => state.step_score(*which, *by),
            Control::Remove(slot) => state.remove(*slot),
            Control::Affix(slot, name) => state.flip_affix(*slot, name),
            Control::OpenPicker => picker.open = true,
            Control::ClosePicker => picker.open = false,
            Control::PickSlot(by) => picker.step_slot(*by),
            Control::PickBase(by) => picker.step_base(*by),
            Control::PickSeed(by) => picker.step_seed(*by),
            Control::PutOn => picker.put_on(state),
            Control::PickSlotAt(i) => {
                picker.slot = *i % SLOTS.len();
                picker.base = 0;
            }
            Control::PickBaseAt(i) => picker.base = *i,
            Control::PickRarity(rarity) => picker.rarity = *rarity,
            Control::Tab(i) if TABS.get(*i).is_some_and(|t| t.1) => tab.0 = *i,
            Control::Tab(_) => {}
            Control::Attacker(by) => state.cycle_attacker(*by),
            Control::StrikeSeed(by) => state.strike_seed = (state.strike_seed as i64 + by).clamp(0, u32::MAX as i64) as u32,
            Control::Activity(by) => state.cycle_activity(*by),
            Control::Swap => state.swapped = !state.swapped,
            Control::OneFight | Control::Thousand | Control::Time(_) => {}
            Control::Rule(path, dir) => crate::lab_rules::step(&mut state.ruleset, path, *dir),
            Control::Dice(path, part, dir) => crate::lab_rules::step_dice(&mut state.ruleset, path, *part, *dir),
            Control::Save => *saved = state.ruleset.clone(),
            Control::Undo => state.ruleset = saved.clone(),
        }
    }
}

/// The window, so a test can see the three columns.
#[derive(Component)]
struct LabRoot;

/// The sheet's column, which scrolls.
#[derive(Component)]
pub struct SheetColumn;

/// Where the sheet is built.
#[derive(Component)]
struct SheetRows;

/// The middle column's tab content, by its place in [`TABS`].
#[derive(Component)]
pub struct TabPane(pub usize);

/// The rules column.
#[derive(Component)]
pub struct RulesColumn;

/// Where the rules' sections are built, which scrolls.
#[derive(Component)]
struct RulesBody;

/// The header's count of what has changed.
#[derive(Component)]
struct RulesCount;

/// A node that scrolls its own overflow when it has any.
#[derive(Component)]
struct ScrollArea;

/// One of the sheet's big numbers, by its place among them.
#[derive(Component)]
struct TileNode(usize);

/// The tooltip over a big number.
#[derive(Component)]
struct TipBox;

/// A tab button's place in [`TABS`], so the open one can be marked.
#[derive(Component)]
struct TabButton(usize);

/// A text that follows the lab state outside the sheet.
#[derive(Component, Clone, Copy, PartialEq)]
enum ControlText {
    Side,
    Attacker,
    StrikeSeed,
    Activity,
}

/// What the Fight tab has run: the thousand fights under the edited rules and under the saved
/// ones, from the same seed, and the one fight played out.
#[derive(Resource, Default)]
pub struct FightLab {
    /// Set by the buttons; [`run_fights`] does them.
    want_many: bool,
    want_one: bool,
    /// Once a thousand has been run, a change to the state or the rules runs it again.
    auto: bool,
    /// How many One fights have been asked for, which moves the fight's seed.
    ones: u32,
    /// How many runs there have been, so a tooltip or a chart knows it is out of date.
    pub generation: u32,
    /// The thousand fights now, and under the saved rules.
    pub many: Option<(FightSummary, FightSummary)>,
    pub one: Option<FightRecord>,
    /// The words at the edges of the life chart and the histogram.
    axes: Option<(Chart, Chart)>,
}

/// The two charts' images.
#[derive(Resource, Default)]
pub struct FightImages {
    life: Option<Handle<Image>>,
    histogram: Option<Handle<Image>>,
}

/// Where the fight's answers, charts and played-out lines are built.
#[derive(Component)]
struct FightResults;

/// One line of the played-out fight, by its place in the fight's blows.
#[derive(Component)]
struct LogLine(usize);

fn blank_chart((w, h): (u32, u32)) -> Image {
    let mut image = Image::new(
        bevy::render::render_resource::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        vec![0; (w * h * 4) as usize],
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD | bevy::asset::RenderAssetUsages::MAIN_WORLD,
    );
    image.sampler = bevy::image::ImageSampler::linear();
    image
}

/// Makes the charts' images.
fn make_chart_images(mut images: ResMut<FightImages>, assets: Option<ResMut<Assets<Image>>>) {
    let Some(mut assets) = assets else { return };
    images.life = Some(assets.add(blank_chart(crate::lab_chart::LIFE_SIZE)));
    images.histogram = Some(assets.add(blank_chart(crate::lab_chart::HISTOGRAM_SIZE)));
}

/// Runs the fights that were asked for, and again after a change when a thousand has been run.
/// The thousand runs under the edited rules and under the saved ones from the same seed.
fn run_fights(state: Res<LabState>, saved: Res<LabRules>, mut lab: ResMut<FightLab>, images: Res<FightImages>, assets: Option<ResMut<Assets<Image>>>) {
    let changed = lab.auto && (state.is_changed() || saved.is_changed());
    let (many, one) = (lab.want_many || changed, lab.want_one || (changed && lab.one.is_some()));
    if !many && !one {
        return;
    }
    let seed = state.strike_seed;
    if many {
        let before = LabState { ruleset: saved.saved.clone(), ..state.clone() };
        let now = crate::lab_fight::fights(&state, &state.ruleset.rules, seed, THOUSAND);
        let was = crate::lab_fight::fights(&before, &saved.saved.rules, seed, THOUSAND);
        let (life, histogram) = (crate::lab_chart::life_chart(&now, Some(&was)), crate::lab_chart::histogram_chart(&now, Some(&was)));
        if let Some(mut assets) = assets {
            for (handle, chart) in [(&images.life, &life), (&images.histogram, &histogram)] {
                if let Some(mut image) = handle.as_ref().and_then(|h| assets.get_mut(h)) {
                    image.data = Some(chart.canvas.pixels.clone());
                }
            }
        }
        lab.axes = Some((life, histogram));
        lab.many = Some((now, was));
        lab.auto = true;
    }
    if one {
        let mut rng = Mulberry32::new(seed.wrapping_add(lab.ones));
        lab.one = Some(crate::lab_fight::fight(&state, &state.ruleset.rules, &mut rng));
    }
    lab.want_many = false;
    lab.want_one = false;
    lab.generation += 1;
}

/// What a button of the Time tab does to the run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimeAction {
    /// Passes this many game hours.
    Pass(f64),
    UntilHungry,
    UntilExhaustion,
    /// Eats or drinks the provision of this base.
    Eat(&'static str),
    Drink(&'static str),
    /// Starts the run over.
    Reset,
}

/// The Time tab's run: the steps asked for so far, and the run they make under the edited rules
/// and under the saved ones.
#[derive(Resource, Default)]
pub struct TimeRun {
    pub steps: Vec<Step>,
    dirty: bool,
    pub now: Option<Timeline>,
    pub was: Option<Timeline>,
    chart: Option<Chart>,
}

impl TimeRun {
    /// Adds what `action` says to the run, under `activity`.
    pub fn push(&mut self, action: TimeAction, activity: Activity) {
        match action {
            TimeAction::Pass(hours) => self.steps.push(Step::Pass(hours, activity)),
            TimeAction::UntilHungry => self.steps.push(Step::Until(Goal::Hungry, activity)),
            TimeAction::UntilExhaustion => self.steps.push(Step::Until(Goal::Exhaustion, activity)),
            TimeAction::Eat(base) => self.steps.push(Step::Eat(base)),
            TimeAction::Drink(base) => self.steps.push(Step::Drink(base)),
            TimeAction::Reset => self.steps.clear(),
        }
        self.dirty = true;
    }
}

/// The time chart's image.
#[derive(Resource, Default)]
pub struct TimeImage(Option<Handle<Image>>);

/// Where the time tab's answers and chart are built.
#[derive(Component)]
pub struct TimeColumn;

/// Makes the time chart's image.
fn make_time_image(mut image: ResMut<TimeImage>, assets: Option<ResMut<Assets<Image>>>) {
    let Some(mut assets) = assets else { return };
    image.0 = Some(assets.add(blank_chart(crate::lab_chart::TIME_SIZE)));
}

/// Plays the run again, under the edited rules and the saved ones, when a step is added or the
/// state or the rules change; the sheet's needs follow the end of it.
fn run_time(state: Res<LabState>, saved: Res<LabRules>, mut run: ResMut<TimeRun>, mut time: ResMut<TimeLab>, image: Res<TimeImage>, assets: Option<ResMut<Assets<Image>>>) {
    if !(run.dirty || (!run.steps.is_empty() && (state.is_changed() || saved.is_changed()))) {
        return;
    }
    let before = LabState { ruleset: saved.saved.clone(), ..state.clone() };
    let now = crate::lab_timeline::run(&state, &run.steps);
    let was = crate::lab_timeline::run(&before, &run.steps);
    let chart = crate::lab_chart::time_chart(&now, Some(&was), &state.ruleset.rules.needs.band_edges);
    if let (Some(mut assets), Some(handle)) = (assets, &image.0)
        && let Some(mut texture) = assets.get_mut(handle)
    {
        texture.data = Some(chart.canvas.pixels.clone());
    }
    time.set_if_neq(now.lab.clone());
    run.chart = Some(chart);
    run.now = Some(now);
    run.was = Some(was);
    run.dirty = false;
}

/// The time chart with its words: the axes, and the words of each band at the right, one line
/// per need in its colour.
fn time_figure(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, chart: &Chart, image: &Option<Handle<Image>>, rules: &fb_character::rules::Rules) {
    let ink = |c: [u8; 4]| Color::srgb_u8(c[0], c[1], c[2]);
    parent
        .spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(6), padding: UiRect::all(px(12)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(8)), ..default() }, BackgroundColor(CELL), BorderColor::all(FAINT)))
        .with_children(|figure| {
            words(figure, &fonts.body_medium, 16.0, BRIGHT, "Needs and exhaustion over time");
            figure.spawn(Node { column_gap: px(12), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|legend| {
                for (name, colour) in [("hunger", crate::lab_chart::HUNGER), ("thirst", crate::lab_chart::THIRST), ("sleepiness", crate::lab_chart::SLEEP), ("exhaustion, 0 to 6 over the same height", crate::lab_chart::EXHAUSTION)] {
                    words(legend, &fonts.body, 12.0, ink(colour), name);
                }
                words(legend, &fonts.body, 12.0, MUTED, "faint dashed: saved rules \u{b7} ticks at the bottom: eaten or drunk \u{b7} red line: death");
            });
            figure.spawn(Node { column_gap: px(6), ..default() }).with_children(|plot| {
                plot.spawn(Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::End, ..default() }).with_children(|axis| {
                    words(axis, &fonts.body, 12.0, MUTED, chart.y_labels.0.clone());
                    words(axis, &fonts.body, 12.0, MUTED, chart.y_labels.1.clone());
                });
                if let Some(image) = image {
                    plot.spawn((ImageNode::new(image.clone()), Node { flex_grow: 1.0, min_width: px(0), aspect_ratio: Some(1200.0 / 420.0), ..default() }));
                    // Over the image: its box at the left of the band words and right of the axis words.
                    plot.spawn(Node { position_type: PositionType::Absolute, left: px(34), right: px(84), top: px(0), bottom: px(0), ..default() }).with_children(|over| {
                        for (at, label) in &chart.marks {
                            over.spawn((Text::new(label.clone()), UiFonts::style(&fonts.body_medium, 13.0, BAD), Node { position_type: PositionType::Absolute, left: percent(at * 100.0 + 0.6), top: px(18), ..default() }));
                        }
                    });
                }
                plot.spawn(Node { flex_direction: FlexDirection::Column, width: px(78), ..default() }).with_children(|band_words| {
                    for band in (0..=3).rev() {
                        band_words.spawn(Node { flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, ..default() }).with_children(|slot| {
                            for (kind, colour) in [(MeterKind::Hunger, crate::lab_chart::HUNGER), (MeterKind::Thirst, crate::lab_chart::THIRST), (MeterKind::Sleepiness, crate::lab_chart::SLEEP)] {
                                words(slot, &fonts.body, 11.0, ink(colour), rules.needs.word(kind, band).to_string());
                            }
                        });
                    }
                });
            });
            figure.spawn(Node { justify_content: JustifyContent::SpaceBetween, margin: UiRect::right(px(84)), ..default() }).with_children(|axis| {
                words(axis, &fonts.body, 12.0, MUTED, chart.x_labels.0.clone());
                words(axis, &fonts.body, 12.0, MUTED, chart.x_labels.1.clone());
            });
        });
}

/// The Time tab's answers and chart, built again when the run changes.
fn refresh_time_view(mut commands: Commands, run: Res<TimeRun>, image: Res<TimeImage>, state: Res<LabState>, saved: Res<LabRules>, fonts: Res<UiFonts>, column: Query<Entity, With<TimeColumn>>) {
    let Ok(column) = column.single() else { return };
    commands.entity(column).despawn_children().with_children(|c| match (&run.now, &run.was, &run.chart) {
        (Some(now), Some(was), Some(chart)) if !run.steps.is_empty() => {
            let (here, before) = (crate::lab_timeline::answers(now, &state.ruleset.rules), crate::lab_timeline::answers(was, &saved.saved.rules));
            c.spawn(Node { column_gap: px(12), ..default() }).with_children(|row| {
                for (a, b) in here.iter().zip(&before).filter(|(a, b)| !(a.danger && a.value == b.value && a.value == "not reached")) {
                    row.spawn((Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0), flex_direction: FlexDirection::Column, padding: UiRect::all(px(12)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(8)), ..default() }, BackgroundColor(CELL), BorderColor::all(if a.danger && a.value != "not reached" { BAD } else { FAINT })))
                        .with_children(|tile| {
                            words(tile, &fonts.body, 13.0, MUTED, a.label.clone());
                            words(tile, &fonts.mono_bold, if a.value == "not reached" || a.value.len() > 8 { 18.0 } else { 30.0 }, if a.danger && a.value != "not reached" { BAD } else { BRIGHT }, a.value.clone());
                            match crate::lab_rules::was_text(&a.value, &b.value) {
                                Some(was) => words(tile, &fonts.body, 13.0, FRAME_GOLD, was),
                                None => words(tile, &fonts.body, 13.0, MUTED, " "),
                            }
                        });
                }
            });
            time_figure(c, &fonts, chart, &image.0, &state.ruleset.rules);
            words(c, &fonts.body, 14.0, MUTED, format!("{} so far, {} steps. Each button adds to the run; Reset starts it over.", clock_text(now.end()), run.steps.len()));
        }
        _ => words(c, &fonts.body, 16.0, MUTED, "Press a span to pass time and see when each need and exhaustion arrive."),
    });
}

/// Where the Eat, drink, equip tab is built.
#[derive(Component)]
pub struct EquipColumn;

/// How many times the equip tab has been built.
#[derive(Resource, Default)]
pub struct EquipBuilds(pub u32);

/// A heading in the equip tab.
fn heading(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, text: &str) {
    parent.spawn(Node { margin: UiRect::top(px(6)), border: UiRect::bottom(px(1)), padding: UiRect::bottom(px(3)), ..default() }).insert(BorderColor::all(FAINT)).with_child((Text::new(text), UiFonts::style(&fonts.display_bold, 22.0, INK)));
}

/// A choice among others: the button of `label`, marked when it is the one chosen.
fn choice(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, control: Control, chosen: bool) {
    let id = button(parent, fonts, label, control);
    if chosen {
        parent.commands().entity(id).insert((BorderColor::all(FRAME_GOLD), BackgroundColor(OPEN_TAB)));
    }
}

/// A row of a label and then what `fill` puts after it, wrapping.
fn labelled(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, fill: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent.spawn(Node { column_gap: px(8), align_items: AlignItems::FlexStart, ..default() }).with_children(|row| {
        row.spawn(Node { width: px(56), flex_shrink: 0.0, padding: UiRect::top(px(6)), ..default() }).with_child((Text::new(label), UiFonts::style(&fonts.body, 16.0, MUTED)));
        row.spawn(Node { flex_grow: 1.0, flex_basis: px(0), column_gap: px(6), row_gap: px(6), flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, ..default() }).with_children(fill);
    });
}

/// Builds the Eat, drink, equip tab: every provision with what it does and a button, a picker of
/// slot, base, seed and rarity with the card of what it rolls, and each thing worn with Take off.
fn rebuild_equip(mut commands: Commands, mut builds: ResMut<EquipBuilds>, picker: Res<Picker>, current: Res<CurrentSheet>, fonts: Res<UiFonts>, column: Query<Entity, With<EquipColumn>>) {
    let Ok(column) = column.single() else { return };
    builds.0 += 1;
    let fonts = &*fonts;
    commands.entity(column).despawn_children().with_children(|c| {
        heading(c, fonts, "Provisions");
        c.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(16), row_gap: px(2), ..default() }).with_children(|list| {
            for (base, p) in fb_items::provision::PROVISIONS {
                let drinks = roll_item(base, 1).and_then(|i| i.consume()) == Some(fb_items::Consume::Drink);
                list.spawn(Node { width: percent(48.5), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, padding: UiRect::axes(px(10), px(2)), ..default() }).with_children(|row| {
                    row.spawn(Node { column_gap: px(14), align_items: AlignItems::Baseline, ..default() }).with_children(|what| {
                        words(what, &fonts.body_medium, 16.0, BRIGHT, *base);
                        words(what, &fonts.body, 14.0, MUTED, format!("nutrition {}% \u{b7} water {}%", number(p.nutrition * 100.0), number(p.water * 100.0)));
                    });
                    let action = if drinks { TimeAction::Drink(base) } else { TimeAction::Eat(base) };
                    button(row, fonts, if drinks { "Drink" } else { "Eat" }, Control::Time(action));
                });
            }
        });
        heading(c, fonts, "Put on or hold something");
        c.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(8), padding: UiRect::all(px(12)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(8)), ..default() }, BackgroundColor(CELL), BorderColor::all(FAINT))).with_children(|card| {
            labelled(card, fonts, "Slot", |row| {
                for (i, slot) in SLOTS.iter().enumerate() {
                    let label = match slot {
                        Slot::Ring1 => "Ring 1".to_string(),
                        Slot::Ring2 => "Ring 2".to_string(),
                        slot => lab_sheet::slot_word(*slot),
                    };
                    choice(row, fonts, &label, Control::PickSlotAt(i), i == picker.slot);
                }
            });
            labelled(card, fonts, "Base", |row| {
                for (i, base) in bases_for(picker.slot()).iter().enumerate() {
                    choice(row, fonts, base, Control::PickBaseAt(i), i == picker.base);
                }
            });
            labelled(card, fonts, "Seed", |row| {
                for (label, by) in [("\u{2212}100", -100), ("\u{2212}10", -10), ("\u{2212}1", -1)] {
                    button(row, fonts, label, Control::PickSeed(by));
                }
                row.spawn(Node { min_width: px(56), justify_content: JustifyContent::Center, ..default() }).with_child((Text::new(picker.seed.to_string()), UiFonts::style(&fonts.mono_bold, 16.0, BRIGHT)));
                for (label, by) in [("+1", 1), ("+10", 10), ("+100", 100)] {
                    button(row, fonts, label, Control::PickSeed(by));
                }
            });
            labelled(card, fonts, "Rarity", |row| {
                choice(row, fonts, "Any", Control::PickRarity(None), picker.rarity.is_none());
                for (label, rarity) in [("Normal", Rarity::Common), ("Magic", Rarity::Magic), ("Rare", Rarity::Rare), ("Legendary", Rarity::Legendary)] {
                    choice(row, fonts, label, Control::PickRarity(Some(rarity)), picker.rarity == Some(rarity));
                }
            });
            match (picker.resolved_seed(), picker.preview()) {
                (Some(seed), Some(item)) => {
                    if seed != picker.seed {
                        words(card, &fonts.body, 14.0, MUTED, format!("The first seed from {} that rolls {} is {seed}.", picker.seed, picker.rarity.map_or("any", rarity_word)));
                    }
                    gear_card(card, fonts, &lab_sheet::preview_card(picker.slot(), &item), CardMode::Preview);
                }
                _ => words(card, &fonts.body, 14.0, BAD, format!("No {} rolls from seed {} within {RARITY_SEARCH} seeds.", picker.rarity.map_or("item", rarity_word), picker.seed)),
            }
            card.spawn(Node::default()).with_children(|row| {
                button(row, fonts, "Put on", Control::PutOn);
            });
        });
        heading(c, fonts, "Worn and held");
        match &current.0 {
            Some(view) if !view.cards.is_empty() => {
                for card in &view.cards {
                    gear_card(c, fonts, card, CardMode::TakeOff);
                }
            }
            _ => words(c, &fonts.body, 15.0, MUTED, "Nothing is worn or held."),
        }
    });
}

/// A rarity as the picker says it.
fn rarity_word(rarity: Rarity) -> &'static str {
    match rarity {
        Rarity::Common => "normal",
        Rarity::Magic => "magic",
        Rarity::Rare => "rare",
        Rarity::Legendary => "legendary",
    }
}

/// A button in the ledger's look, with `control` on it.
fn button(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, control: Control) -> Entity {
    let mut button = parent.spawn((
        control,
        Node {
            padding: UiRect::axes(px(10), px(5)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            min_height: px(30),
            ..default()
        },
        BackgroundColor(CELL),
        BorderColor::all(LINE),
    ));
    button.with_child((Text::new(label), UiFonts::style(&fonts.body_medium, 16.0, INK)));
    button.id()
}

/// A button whose words follow the lab state ([`refresh_controls`]).
fn state_button(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, control: Control, words: ControlText) {
    let id = button(parent, fonts, "", control);
    parent.commands().entity(id).despawn_children().with_child((words, Text::new(""), UiFonts::style(&fonts.body_medium, 16.0, INK)));
}

fn setup(mut commands: Commands, fonts: Res<UiFonts>, mut hover: ResMut<Hover>) {
    commands.spawn((Camera2d, bevy::ui::IsDefaultUiCamera));
    commands.spawn((LabRoot, Node { width: percent(100), height: percent(100), flex_direction: FlexDirection::Row, ..default() })).with_children(|root| {
        // The sheet.
        root.spawn((
            SheetColumn,
            ScrollArea,
            ScrollPosition::default(),
            Node { width: px(SHEET_WIDTH), flex_shrink: 0.0, min_height: px(0), flex_direction: FlexDirection::Column, border: UiRect::right(px(1)), overflow: Overflow::scroll_y(), ..default() },
            BackgroundColor(BG),
            BorderColor::all(FAINT),
        ))
        .with_child((SheetRows, Node { flex_direction: FlexDirection::Column, row_gap: px(14), padding: UiRect::all(px(16)), ..default() }));
        // The tests.
        root.spawn(Node { flex_grow: 1.0, min_width: px(0), flex_direction: FlexDirection::Column, padding: UiRect::new(px(24), px(24), px(20), px(0)), row_gap: px(14), ..default() }).with_children(|middle| {
            middle.spawn((Node { column_gap: px(4), border: UiRect::bottom(px(1)), flex_wrap: FlexWrap::Wrap, ..default() }, BorderColor::all(FAINT))).with_children(|bar| {
                for (i, (label, enabled)) in TABS.iter().enumerate() {
                    let mut tab = bar.spawn((
                        TabButton(i),
                        Node {
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::axes(px(14), px(6)),
                            border: UiRect::bottom(px(2)),
                            border_radius: BorderRadius::top(px(6)),
                            min_height: px(44),
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                        BorderColor::all(Color::NONE),
                    ));
                    if *enabled {
                        tab.insert(Control::Tab(i));
                    }
                    let ink = if *enabled { INK } else { LINE };
                    tab.with_child((Text::new(*label), UiFonts::style(&fonts.body_medium, 17.0, ink)));
                    if !enabled {
                        tab.with_child((Text::new("after effects land"), UiFonts::style(&fonts.body, 12.0, LINE)));
                    }
                }
            });
            for i in 0..TABS.len() {
                middle
                    .spawn((
                        TabPane(i),
                        ScrollArea,
                        ScrollPosition::default(),
                        Node {
                            display: if i == 0 { Display::Flex } else { Display::None },
                            flex_grow: 1.0,
                            min_height: px(0),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(10),
                            overflow: Overflow::scroll_y(),
                            padding: UiRect::bottom(px(30)),
                            ..default()
                        },
                    ))
                    .with_children(|pane| match i {
                        0 => {
                            pane.spawn(Node { column_gap: px(8), row_gap: px(8), flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, ..default() }).with_children(|row| {
                                row.spawn((ControlText::Side, Text::new(""), UiFonts::style(&fonts.body, 16.0, MUTED)));
                                state_button(row, &fonts, Control::Attacker(1), ControlText::Attacker);
                                button(row, &fonts, "Swap sides", Control::Swap);
                                row.spawn(Node { flex_grow: 1.0, ..default() });
                                row.spawn((Text::new("seed"), UiFonts::style(&fonts.body, 16.0, MUTED)));
                                button(row, &fonts, "\u{2212}", Control::StrikeSeed(-1));
                                row.spawn((ControlText::StrikeSeed, Text::new(""), UiFonts::style(&fonts.mono_bold, 16.0, BRIGHT)));
                                button(row, &fonts, "+", Control::StrikeSeed(1));
                                button(row, &fonts, "One fight", Control::OneFight);
                                let id = button(row, &fonts, "1,000 fights", Control::Thousand);
                                row.commands().entity(id).insert((BorderColor::all(FRAME_GOLD), BackgroundColor(OPEN_TAB)));
                            });
                            pane.spawn((FightResults, Node { flex_direction: FlexDirection::Column, row_gap: px(12), ..default() }));
                        }
                        1 => {
                            pane.spawn(Node { column_gap: px(8), row_gap: px(8), flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, ..default() }).with_children(|row| {
                                row.spawn((Text::new("While they are"), UiFonts::style(&fonts.body, 16.0, MUTED)));
                                state_button(row, &fonts, Control::Activity(1), ControlText::Activity);
                            });
                            pane.spawn(Node { column_gap: px(8), row_gap: px(8), flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, ..default() }).with_children(|row| {
                                for (label, hours) in [("8 h", 8.0), ("1 day", 24.0), ("3 days", 72.0), ("7 days", 168.0)] {
                                    button(row, &fonts, label, Control::Time(TimeAction::Pass(hours)));
                                }
                                button(row, &fonts, "Until Hungry", Control::Time(TimeAction::UntilHungry));
                                button(row, &fonts, "Until exhaustion 1", Control::Time(TimeAction::UntilExhaustion));
                                row.spawn(Node { flex_grow: 1.0, ..default() });
                                button(row, &fonts, "Eat", Control::Time(TimeAction::Eat(crate::lab_time::RATION)));
                                button(row, &fonts, "Drink", Control::Time(TimeAction::Drink(crate::lab_time::DRAUGHT)));
                                button(row, &fonts, "Reset", Control::Time(TimeAction::Reset));
                            });
                            pane.spawn((TimeColumn, Node { flex_direction: FlexDirection::Column, row_gap: px(12), ..default() }));
                        }
                        2 => {
                            pane.spawn((EquipColumn, Node { flex_direction: FlexDirection::Column, row_gap: px(12), ..default() }));
                        }
                        _ => {}
                    });
            }
        });
        // The rules.
        root.spawn((
            RulesColumn,
            Node { width: px(RULES_WIDTH), flex_shrink: 0.0, min_height: px(0), flex_direction: FlexDirection::Column, row_gap: px(10), padding: UiRect::all(px(16)), border: UiRect::left(px(1)), ..default() },
            BackgroundColor(RULES_PAGE),
            BorderColor::all(FAINT),
        ))
        .with_children(|rules| {
            rules.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Baseline, ..default() }).with_children(|head| {
                head.spawn((Text::new("Rules"), UiFonts::style(&fonts.display_bold, 26.0, INK)));
                head.spawn((RulesCount, Text::new(""), UiFonts::style(&fonts.body_medium, 15.0, FRAME_GOLD)));
            });
            rules.spawn((Text::new("The numbers the game runs on. Change one and the sheet and the tests show it, compared with before."), UiFonts::style(&fonts.body, 14.0, MUTED)));
            rules.spawn((RulesBody, ScrollArea, ScrollPosition::default(), Node { flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column, row_gap: px(4), overflow: Overflow::scroll_y(), ..default() }));
            rules.spawn((SaveNoteText, Text::new(""), UiFonts::style(&fonts.body, 14.0, BAD)));
            rules.spawn(Node { column_gap: px(8), ..default() }).with_children(|row| {
                for (label, control, gold) in [("Save to the game", Control::Save, true), ("Undo changes", Control::Undo, false)] {
                    let id = button(row, &fonts, label, control);
                    row.commands().entity(id).insert(Node { flex_grow: 1.0, flex_basis: px(0), min_height: px(40), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(4)), ..default() });
                    if gold {
                        row.commands().entity(id).insert(BorderColor::all(FRAME_GOLD));
                    }
                }
            });
        });
        // The tooltip, over everything.
        root.spawn((
            TipBox,
            Visibility::Hidden,
            GlobalZIndex(20),
            Node {
                position_type: PositionType::Absolute,
                width: px(260),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                padding: UiRect::axes(px(12), px(10)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(6)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.17, 0.145, 0.10)),
            BorderColor::all(FRAME_GOLD),
        ));
    });
    if std::env::var("FB_LAB_DEMO").is_ok_and(|v| v == "1") {
        hover.pinned = Some(0);
    }
}

/// Whether anything the sheet shows has changed.
#[allow(clippy::too_many_arguments)]
fn sheet_changed(state: Res<LabState>, time: Res<TimeLab>, picker: Res<Picker>, figure: Res<LabFigure>, saved: Res<LabRules>) -> bool {
    state.is_changed() || time.is_changed() || picker.is_changed() || figure.is_changed() || saved.is_changed()
}

/// A line of text in the sheet.
fn words(parent: &mut ChildSpawnerCommands, font: &Handle<Font>, size: f32, colour: Color, text: impl Into<String>) {
    parent.spawn((Text::new(text), UiFonts::style(font, size, colour)));
}

/// A bar of `fill` in `colour`, `height` tall, with a rounded end.
fn bar_track(parent: &mut ChildSpawnerCommands, fill: f32, colour: Color, height: f32) {
    parent
        .spawn((Node { flex_grow: 1.0, height: px(height), border_radius: BorderRadius::all(px(height / 2.0)), ..default() }, BackgroundColor(Color::srgb(0.165, 0.141, 0.106))))
        .with_child((Node { width: percent(fill * 100.0), height: px(height), border_radius: BorderRadius::all(px(height / 2.0)), ..default() }, BackgroundColor(colour)));
}

/// A row of a bar: its label, the bar and the words beside it.
fn bar_row(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, bar: &lab_sheet::Bar, height: f32, word_colour: Color) {
    parent.spawn(Node { align_items: AlignItems::Center, column_gap: px(10), ..default() }).with_children(|row| {
        row.spawn(Node { width: px(84), flex_shrink: 0.0, ..default() }).with_child((Text::new(bar.label), UiFonts::style(&fonts.body, 16.0, BRIGHT)));
        bar_track(row, bar.fill, bar.colour, height);
        row.spawn(Node { width: px(76), flex_shrink: 0.0, flex_direction: FlexDirection::Column, align_items: AlignItems::End, ..default() }).with_children(|words_column| {
            words_column.spawn((Text::new(bar.text.clone()), UiFonts::style(&fonts.body, 16.0, word_colour)));
            if let Some(was) = &bar.was {
                words_column.spawn((Text::new(was.clone()), UiFonts::style(&fonts.body, 12.0, FRAME_GOLD)));
            }
        });
    });
}

/// A small square button with `label`.
fn small_button(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, control: Control) {
    parent
        .spawn((control, Node { width: px(17), height: px(17), flex_shrink: 0.0, justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(3)), ..default() }, BorderColor::all(LINE)))
        .with_child((Text::new(label), UiFonts::style(&fonts.body_medium, 15.0, MUTED)));
}

/// The sheet's whole body.
fn build_sheet(sheet: &mut ChildSpawnerCommands, fonts: &UiFonts, view: &SheetView, picker: &Picker, figure: &LabFigure) {
    // Who they are.
    sheet.spawn(Node { column_gap: px(16), align_items: AlignItems::Center, ..default() }).with_children(|head| {
        let mut frame = head.spawn((
            Node { width: px(crate::lab_figure::FRAME.x), height: px(crate::lab_figure::FRAME.y), flex_shrink: 0.0, border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(6)), ..default() },
            BackgroundColor(CELL),
            BorderColor::all(LINE),
        ));
        if let Some(image) = &figure.image {
            frame.with_child((ImageNode::new(image.clone()), Node { width: percent(100), height: percent(100), ..default() }));
        }
        head.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), min_width: px(0), ..default() }).with_children(|who| {
            words(who, &fonts.display_bold, 26.0, INK, view.title.clone());
            words(who, &fonts.body, 16.0, MUTED, view.caption.clone());
            who.spawn(Node { column_gap: px(6), row_gap: px(6), flex_wrap: FlexWrap::Wrap, margin: UiRect::top(px(6)), ..default() }).with_children(|chips| {
                button(chips, fonts, "Next folk", Control::Folk(1));
                button(chips, fonts, "Next class", Control::Class(1));
                button(chips, fonts, "Reroll", Control::Reroll);
            });
        });
    });
    // Attributes.
    sheet.spawn(Node { column_gap: px(4), ..default() }).with_children(|row| {
        for (which, a) in view.attributes.iter().enumerate() {
            row.spawn((
                Node { flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, padding: UiRect::axes(px(0), px(7)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(6)), ..default() },
                BackgroundColor(CELL),
                BorderColor::all(FAINT),
            ))
            .with_children(|tile| {
                words(tile, &fonts.body, 14.0, MUTED, a.name);
                words(tile, &fonts.mono_bold, 22.0, BRIGHT, a.score.to_string());
                tile.spawn(Node { align_items: AlignItems::Center, column_gap: px(2), ..default() }).with_children(|line| {
                    small_button(line, fonts, "\u{2212}", Control::Score(which, -1));
                    let colour = match a.modifier {
                        m if m > 0 => FRAME_GOLD,
                        m if m < 0 => ACCENT,
                        _ => MUTED,
                    };
                    line.spawn(Node { width: px(22), justify_content: JustifyContent::Center, ..default() }).with_child((Text::new(signed(a.modifier as f64)), UiFonts::style(&fonts.body_medium, 15.0, colour)));
                    small_button(line, fonts, "+", Control::Score(which, 1));
                });
            });
        }
    });
    // Life, stamina and mana, then the needs.
    sheet.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(9), ..default() }).with_children(|bars| {
        for b in [&view.life, &view.stamina, &view.mana] {
            bar_row(bars, fonts, b, 12.0, BRIGHT);
        }
        bars.spawn(Node { height: px(2), ..default() });
        for b in &view.needs {
            bar_row(bars, fonts, b, 8.0, MUTED);
        }
        bars.spawn(Node { align_items: AlignItems::Center, column_gap: px(10), ..default() }).with_children(|row| {
            row.spawn(Node { width: px(84), flex_shrink: 0.0, ..default() }).with_child((Text::new("Exhaustion"), UiFonts::style(&fonts.body, 16.0, BRIGHT)));
            row.spawn(Node { flex_grow: 1.0, column_gap: px(4), ..default() }).with_children(|ladder| {
                for step in 1..=lab_sheet::EXHAUSTION_STEPS {
                    let (fill, edge) = match (step <= view.exhaustion.0, step == lab_sheet::EXHAUSTION_STEPS) {
                        (true, true) => (BAD, BAD),
                        (true, false) => (ACCENT, ACCENT),
                        (false, true) => (Color::NONE, Color::srgb(0.48, 0.23, 0.20)),
                        (false, false) => (Color::NONE, LINE),
                    };
                    ladder.spawn((Node { flex_grow: 1.0, flex_basis: px(0), height: px(8), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(2)), ..default() }, BackgroundColor(fill), BorderColor::all(edge)));
                }
            });
            row.spawn(Node { width: px(70), flex_shrink: 0.0, justify_content: JustifyContent::End, ..default() }).with_child((Text::new(view.exhaustion.1.clone()), UiFonts::style(&fonts.body, 16.0, if view.exhaustion.0 >= lab_sheet::EXHAUSTION_STEPS { BAD } else { MUTED })));
        });
    });
    // Armour, damage and load.
    sheet.spawn(Node { column_gap: px(8), ..default() }).with_children(|row| {
        for (i, tile) in view.tiles.iter().enumerate() {
            row.spawn((
                TileNode(i),
                Node { flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, padding: UiRect::axes(px(4), px(10)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(6)), ..default() },
                BackgroundColor(CELL),
                BorderColor::all(if i == 0 { FRAME_GOLD } else { FAINT }),
            ))
            .with_children(|t| {
                words(t, &fonts.body, 14.0, MUTED, tile.label);
                t.spawn(Node { align_items: AlignItems::Baseline, ..default() }).with_children(|value| {
                    words(value, &fonts.mono_bold, 28.0, BRIGHT, tile.value.clone());
                    if let Some(small) = &tile.small {
                        words(value, &fonts.mono, 15.0, MUTED, small.clone());
                    }
                });
                if let Some(was) = &tile.was {
                    words(t, &fonts.body, 13.0, FRAME_GOLD, was.clone());
                }
            });
        }
    });
    // Gear.
    words(sheet, &fonts.display_bold, 19.0, INK, "Worn and held");
    for card in &view.cards {
        gear_card(sheet, fonts, card, CardMode::Sheet);
    }
    if picker.open {
        picker_card(sheet, fonts, picker);
    } else {
        sheet
            .spawn((Control::OpenPicker, Node { justify_content: JustifyContent::Center, padding: UiRect::all(px(9)), min_height: px(40), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(6)), ..default() }, BorderColor::all(LINE)))
            .with_child((Text::new("+ Put on or hold an item"), UiFonts::style(&fonts.body_medium, 16.0, FRAME_GOLD)));
    }
}

/// A card for one thing worn or held: its name in its rarity's colour, the slot and what it
/// does in plain words on the left, what it adds on the right, and a way to take it off.
/// What a gear card offers: the sheet's, with a cross and switchable affixes; the equip tab's, with
/// Take off; or a preview, with nothing to press.
#[derive(Clone, Copy, PartialEq)]
enum CardMode {
    Sheet,
    TakeOff,
    Preview,
}

fn gear_card(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, card: &GearCard, mode: CardMode) {
    parent
        .spawn((Node { justify_content: JustifyContent::SpaceBetween, column_gap: px(8), padding: UiRect::axes(px(10), px(8)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(6)), ..default() }, BackgroundColor(CELL), BorderColor::all(FAINT)))
        .with_children(|c| {
            c.spawn(Node { flex_direction: FlexDirection::Column, flex_shrink: 1.0, min_width: px(0), ..default() }).with_children(|left| {
                words(left, &fonts.body_medium, 17.0, rarity_color(card.rarity), card.title.clone());
                if card.affixes.is_empty() {
                    words(left, &fonts.body, 14.0, MUTED, format!("{} \u{b7} plain", card.slot_word));
                } else {
                    words(left, &fonts.body, 14.0, MUTED, card.slot_word.clone());
                    for affix in &card.affixes {
                        let colour = match (affix.on, affix.good) {
                            (false, _) => LINE,
                            (true, true) => MUTED,
                            (true, false) => BAD,
                        };
                        let text = format!("{}: {}{}", affix.name, affix.text, if affix.on { "" } else { " (off)" });
                        let line = left.spawn(Node::default()).id();
                        if mode != CardMode::Preview {
                            left.commands().entity(line).insert(Control::Affix(card.slot, affix.name.clone()));
                        }
                        left.commands().entity(line).with_child((Text::new(text), UiFonts::style(&fonts.body, 14.0, colour)));
                    }
                }
            });
            c.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::End, flex_shrink: 0.0, ..default() }).with_children(|right| {
                for (i, add) in card.adds.iter().enumerate() {
                    words(right, &fonts.body, 14.0, if i + 1 == card.adds.len() { MUTED } else { BRIGHT }, add.clone());
                }
                match mode {
                    CardMode::Sheet => {
                        right.spawn(Node { margin: UiRect::top(px(4)), ..default() }).with_children(|x| small_button(x, fonts, "\u{d7}", Control::Remove(card.slot)));
                    }
                    CardMode::TakeOff => {
                        right.spawn(Node { margin: UiRect::top(px(4)), ..default() }).with_children(|x| {
                            button(x, fonts, "Take off", Control::Remove(card.slot));
                        });
                    }
                    CardMode::Preview => {}
                }
            });
        });
}

/// The card that opens from "Put on or hold an item": a slot, a base and a seed, with the
/// item they roll, and Put on and Cancel.
fn picker_card(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, picker: &Picker) {
    let slot = lab_sheet::slot_word(picker.slot());
    let base = picker.base_name().unwrap_or_else(|| "nothing fits".to_string());
    parent
        .spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(8), padding: UiRect::all(px(10)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(6)), ..default() }, BackgroundColor(CELL), BorderColor::all(FRAME_GOLD)))
        .with_children(|card| {
            for (label, value, down, up) in [
                ("Slot", slot, Control::PickSlot(-1), Control::PickSlot(1)),
                ("Base", base, Control::PickBase(-1), Control::PickBase(1)),
                ("Seed", picker.seed.to_string(), Control::PickSeed(-1), Control::PickSeed(1)),
            ] {
                card.spawn(Node { align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|row| {
                    row.spawn(Node { width: px(48), ..default() }).with_child((Text::new(label), UiFonts::style(&fonts.body, 16.0, MUTED)));
                    button(row, fonts, "\u{2039}", down);
                    row.spawn(Node { flex_grow: 1.0, justify_content: JustifyContent::Center, ..default() }).with_child((Text::new(value), UiFonts::style(&fonts.body_medium, 16.0, BRIGHT)));
                    button(row, fonts, "\u{203a}", up);
                });
            }
            if let Some(item) = picker.preview() {
                words(card, &fonts.body_medium, 17.0, rarity_color(item.rarity), item.title().to_string());
            }
            card.spawn(Node { column_gap: px(8), ..default() }).with_children(|row| {
                button(row, fonts, "Put on", Control::PutOn);
                button(row, fonts, "Cancel", Control::ClosePicker);
            });
        });
}

/// Builds the sheet again from the state.
#[allow(clippy::too_many_arguments)]
fn rebuild_sheet(
    mut builds: ResMut<SheetBuilds>,
    mut current: ResMut<CurrentSheet>,
    mut commands: Commands,
    state: Res<LabState>,
    time: Res<TimeLab>,
    picker: Res<Picker>,
    figure: Res<LabFigure>,
    saved: Res<LabRules>,
    fonts: Res<UiFonts>,
    rows: Query<Entity, With<SheetRows>>,
) {
    let Ok(root) = rows.single() else { return };
    builds.0 += 1;
    let view = lab_sheet::view_marked(&state, &time, &saved.saved);
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|root| build_sheet(root, &fonts, &view, &picker, &figure));
    current.0 = Some(view);
}

/// With `FB_LAB_DEMO=1`, does what a few clicks would, for a capture: three edits to the rules
/// (Capacity per STR to 20, the giant rat's attack bonus to +2, hours walking to Starving to 20),
/// a 14 day walk (which ends in death), 1,000 fights and One fight, and holds Armour's tooltip up ([`setup`]).
/// `FB_LAB_TAB=1` opens that tab.
fn demo(mut state: ResMut<LabState>, mut run: ResMut<TimeRun>, mut fight: ResMut<FightLab>, mut tab: ResMut<LabTab>) {
    if std::env::var("FB_LAB_DEMO").is_ok_and(|v| v == "1") {
        crate::lab_rules::step(&mut state.ruleset, "load/capacity_per_strength", 5);
        crate::lab_rules::step(&mut state.ruleset, "creatures/Giant Rat/attack_bonus", -1);
        crate::lab_rules::step(&mut state.ruleset, "creatures/Giant Rat/attack_bonus", -1);
        for _ in 0..4 {
            crate::lab_rules::step(&mut state.ruleset, "needs/hunger_hours_to_full", 1);
        }
        run.push(TimeAction::Pass(168.0), state.activity);
        run.push(TimeAction::Pass(168.0), state.activity);
        fight.want_many = true;
        fight.want_one = true;
        if let Some(open) = std::env::var("FB_LAB_TAB").ok().and_then(|t| t.parse().ok()) {
            tab.0 = open;
        }
    }
}

/// The cursor, if the window has it.
fn cursor(window: &Option<Single<&Window, With<PrimaryWindow>>>) -> Option<Vec2> {
    window.as_ref().and_then(|w| w.cursor_position())
}

/// A control pressed, by a click on it or by a test.
#[derive(Message, Clone, Debug)]
pub struct ControlPressed(pub Control);

/// Where `rules.toml` and `afx.json` are saved to and read from.
#[derive(Resource, Clone, Debug)]
pub struct DataFolder(pub std::path::PathBuf);

impl Default for DataFolder {
    fn default() -> Self {
        DataFolder(crate::rules::data_folder().into())
    }
}

/// What the last save said: shown in the rules footer, "Saved" for a moment, a failure until
/// the next save.
#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct SaveNote {
    pub failed: Option<String>,
    /// Real seconds at which "Saved" stops showing.
    saved_until: Option<f64>,
}

/// The footer's line of that.
#[derive(Component)]
struct SaveNoteText;

/// How long "Saved" shows, in seconds.
const SAVED_SHOWN: f64 = 2.5;

/// A click on a control of the sheet or the tabs is a [`ControlPressed`].
fn press_controls(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    controls: Query<(&Control, &ComputedNode, &UiGlobalTransform)>,
    mut pressed: MessageWriter<ControlPressed>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(point) = cursor(&window) else { return };
    if let Some((control, ..)) = controls.iter().find(|(_, node, transform)| node_rect(node, transform).contains(point)) {
        pressed.write(ControlPressed(control.clone()));
    }
}

/// Does what each pressed control says. Save writes the files first, and the edits stay unsaved
/// if that fails.
#[allow(clippy::too_many_arguments)]
fn apply_pressed(
    mut pressed: MessageReader<ControlPressed>,
    time: Res<Time<Real>>,
    folder: Res<DataFolder>,
    mut note: ResMut<SaveNote>,
    mut state: ResMut<LabState>,
    mut picker: ResMut<Picker>,
    mut tab: ResMut<LabTab>,
    mut saved: ResMut<LabRules>,
    mut fight: ResMut<FightLab>,
    mut run: ResMut<TimeRun>,
) {
    for ControlPressed(control) in pressed.read() {
        match control {
            Control::Time(action) => run.push(*action, state.activity),
            Control::OneFight => {
                fight.want_one = true;
                fight.ones += 1;
            }
            Control::Thousand => fight.want_many = true,
            _ => {}
        }
        if *control == Control::Save {
            match crate::lab_rules::save_files(&folder.0, &state.ruleset, &|a, b| std::fs::rename(a, b)) {
                Ok(()) => *note = SaveNote { failed: None, saved_until: Some(time.elapsed_secs_f64() + SAVED_SHOWN) },
                Err(error) => {
                    warn!("lab: could not save the rules: {error}");
                    *note = SaveNote { failed: Some(error), saved_until: None };
                    continue;
                }
            }
        }
        let (mut next_state, mut next_picker, mut next_tab, mut next_saved) = (state.clone(), picker.clone(), *tab, saved.saved.clone());
        control.apply(&mut next_state, &mut next_picker, &mut next_tab, &mut next_saved);
        saved.set_if_neq(LabRules { saved: next_saved });
        state.set_if_neq(next_state);
        picker.set_if_neq(next_picker);
        tab.set_if_neq(next_tab);
    }
}

/// Shows the save note in the footer, and takes "Saved" away when its time is up.
fn show_save_note(time: Res<Time<Real>>, mut note: ResMut<SaveNote>, mut line: Query<(&mut Text, &mut TextColor), With<SaveNoteText>>) {
    if note.saved_until.is_some_and(|t| time.elapsed_secs_f64() >= t) {
        note.saved_until = None;
    }
    let (words, colour) = match (&note.failed, note.saved_until) {
        (Some(why), _) => (format!("Couldn't save: {why}"), BAD),
        (None, Some(_)) => ("Saved".to_string(), OK),
        _ => (String::new(), OK),
    };
    for (mut text, mut ink) in &mut line {
        if text.0 != words {
            text.0 = words.clone();
        }
        if ink.0 != colour {
            ink.0 = colour;
        }
    }
}

/// When the rules file changes on disk (an edit outside the lab, or the lab's own save coming
/// back), the saved rules follow it, and the affix strengths are read again from their file;
/// the edited ones follow too unless there are unsaved changes.
fn follow_disk(game: Option<Res<GameRules>>, folder: Res<DataFolder>, mut saved: ResMut<LabRules>, mut state: ResMut<LabState>) {
    let Some(game) = game.filter(|g| g.is_changed()) else { return };
    let mut next = saved.saved.clone();
    next.rules = game.0.clone();
    if let Some(strengths) = std::fs::read_to_string(folder.0.join("afx.json")).ok().and_then(|t| crate::lab_rules::strengths_from_json(&t)) {
        next.strengths = strengths;
    }
    if next == saved.saved {
        return;
    }
    let clean = state.ruleset == saved.saved;
    saved.saved = next;
    if clean {
        state.ruleset = saved.saved.clone();
    }
}

/// One field of the rules column: its words and `was`, and the controls that change it.
fn rule_field(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, field: &crate::lab_rules::Field) {
    let changed = field.was.is_some();
    parent
        .spawn((
            Node { flex_direction: FlexDirection::Column, row_gap: px(4), padding: UiRect::axes(px(6), px(4)), border_radius: BorderRadius::all(px(4)), ..default() },
            BackgroundColor(if changed { Color::srgb(0.17, 0.145, 0.10) } else { Color::NONE }),
        ))
        .with_children(|f| {
            f.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|row| {
                row.spawn(Node { flex_direction: FlexDirection::Column, flex_shrink: 1.0, min_width: px(0), ..default() }).with_children(|label| {
                    words(label, &fonts.body, 15.0, if changed { BRIGHT } else { MUTED }, field.label.clone());
                    if let Some(was) = &field.was {
                        words(label, &fonts.body, 12.0, FRAME_GOLD, format!("was {was}"));
                    }
                });
                row.spawn(Node { align_items: AlignItems::Center, column_gap: px(4), flex_shrink: 0.0, ..default() }).with_children(|controls| match field.kind {
                    FieldKind::Switch => {
                        button(controls, fonts, &field.now, Control::Rule(field.path.clone(), 1));
                    }
                    FieldKind::Number => {
                        small_button(controls, fonts, "\u{2212}", Control::Rule(field.path.clone(), -1));
                        controls.spawn(Node { width: px(46), justify_content: JustifyContent::Center, ..default() }).with_child((Text::new(field.now.clone()), UiFonts::style(&fonts.mono_bold, 15.0, BRIGHT)));
                        small_button(controls, fonts, "+", Control::Rule(field.path.clone(), 1));
                    }
                    FieldKind::Dice => {
                        controls.spawn((Text::new(field.now.clone()), UiFonts::style(&fonts.mono_bold, 15.0, BRIGHT)));
                    }
                });
            });
            if field.kind == FieldKind::Dice {
                f.spawn(Node { column_gap: px(10), justify_content: JustifyContent::End, ..default() }).with_children(|row| {
                    for (caption, part) in [("dice", DicePart::Count), ("sides", DicePart::Sides), ("bonus", DicePart::Bonus)] {
                        row.spawn(Node { align_items: AlignItems::Center, column_gap: px(3), ..default() }).with_children(|group| {
                            words(group, &fonts.body, 12.0, MUTED, caption);
                            small_button(group, fonts, "\u{2212}", Control::Dice(field.path.clone(), part, -1));
                            small_button(group, fonts, "+", Control::Dice(field.path.clone(), part, 1));
                        });
                    }
                });
            }
        });
}

/// Builds the rules column's sections and its count of changes, when the edited or the saved
/// rules are not what was last built.
fn rebuild_rules(
    mut commands: Commands,
    state: Res<LabState>,
    saved: Res<LabRules>,
    fonts: Res<UiFonts>,
    body: Query<Entity, With<RulesBody>>,
    mut count: Query<&mut Text, With<RulesCount>>,
    mut built: Local<Option<(RuleSet, RuleSet)>>,
) {
    let Ok(body) = body.single() else { return };
    if built.as_ref().is_some_and(|(e, s)| *e == state.ruleset && *s == saved.saved) {
        return;
    }
    *built = Some((state.ruleset.clone(), saved.saved.clone()));
    let changed = crate::lab_rules::changed(&state.ruleset, &saved.saved).len();
    for mut text in &mut count {
        text.0 = if changed == 0 { String::new() } else { format!("{changed} changed") };
    }
    let sections = crate::lab_rules::sections(&state.ruleset, &saved.saved);
    commands.entity(body).despawn_children().with_children(|body| {
        for section in &sections {
            body.spawn(Node { margin: UiRect::top(px(10)), ..default() }).with_child((Text::new(section.title.clone()), UiFonts::style(&fonts.body_medium, 17.0, INK)));
            for field in &section.fields {
                rule_field(body, &fonts, field);
            }
        }
    });
}

/// Opens the tab that is chosen and marks its button.
fn show_tabs(tab: Res<LabTab>, mut panes: Query<(&TabPane, &mut Node)>, mut buttons: Query<(&TabButton, &mut BackgroundColor, &mut BorderColor)>) {
    for (pane, mut node) in &mut panes {
        node.display = if pane.0 == tab.0 { Display::Flex } else { Display::None };
    }
    for (button, mut background, mut border) in &mut buttons {
        let open = button.0 == tab.0;
        background.0 = if open { OPEN_TAB } else { Color::NONE };
        *border = BorderColor::all(if open { FRAME_GOLD } else { Color::NONE });
    }
}

/// Keeps the words on the middle column's own controls to the lab state.
fn refresh_controls(state: Res<LabState>, mut texts: Query<(&ControlText, &mut Text)>) {
    for (which, mut text) in &mut texts {
        let line = match which {
            ControlText::Side => if state.swapped { "They attack".to_string() } else { "They are attacked by".to_string() },
            ControlText::Attacker => state.attacker.label(),
            ControlText::StrikeSeed => state.strike_seed.to_string(),
            ControlText::Activity => activity_word(state.activity).to_string(),
        };
        if text.0 != line {
            text.0 = line;
        }
    }
}

/// A percentage as the tiles write it: `50%`.
fn rate_text(rate: f64) -> String {
    format!("{:.0}%", rate * 100.0)
}

/// A mean with one decimal, or a dash for none.
fn mean(value: Option<f64>) -> String {
    value.map_or_else(|| "\u{2013}".to_string(), |v| format!("{v:.1}"))
}

/// The attacker's blows to down them, or "rarely wins" when it wins under 5% of fights, too few
/// for a mean to mean anything.
fn blows_text(summary: &FightSummary) -> String {
    if summary.attacker_wins < 0.05 { "rarely wins".to_string() } else { mean(summary.blows_to_down) }
}

/// The four answers of a thousand fights, each with the saved rules' value when it differs:
/// the label, the value, and `was`.
fn fight_tiles(now: &FightSummary, was: &FightSummary) -> Vec<(String, String, Option<String>)> {
    let name = &now.attacker;
    let rows = [
        (format!("{name} hits them"), rate_text(now.attacker_hit_rate), rate_text(was.attacker_hit_rate)),
        ("Damage per hit".to_string(), mean(now.damage_per_hit), mean(was.damage_per_hit)),
        (format!("{name} blows to down them, in fights it wins"), blows_text(now), blows_text(was)),
        ("They win".to_string(), rate_text(now.they_win), rate_text(was.they_win)),
    ];
    rows.into_iter().map(|(label, value, before)| (label, value.clone(), crate::lab_rules::was_text(&value, &before))).collect()
}

/// A chart with its caption, the words at its edges and the image.
fn chart_figure(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, title: &str, note: &str, chart: &Chart, image: &Option<Handle<Image>>, aspect: f32, grow: f32) {
    parent
        .spawn((Node { flex_grow: grow, flex_basis: px(0), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), padding: UiRect::all(px(12)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(8)), ..default() }, BackgroundColor(CELL), BorderColor::all(FAINT)))
        .with_children(|figure| {
            words(figure, &fonts.body_medium, 16.0, BRIGHT, title);
            words(figure, &fonts.body, 12.0, MUTED, note);
            figure.spawn(Node { column_gap: px(6), ..default() }).with_children(|plot| {
                plot.spawn(Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::End, ..default() }).with_children(|axis| {
                    words(axis, &fonts.body, 12.0, MUTED, chart.y_labels.0.clone());
                    words(axis, &fonts.body, 12.0, MUTED, chart.y_labels.1.clone());
                });
                if let Some(image) = image {
                    plot.spawn((ImageNode::new(image.clone()), Node { flex_grow: 1.0, min_width: px(0), aspect_ratio: Some(aspect), ..default() }));
                }
            });
            figure.spawn(Node { ..default() }).with_children(|axis| {
                let share = chart.draw.unwrap_or(0.0) * 100.0;
                axis.spawn(Node { width: percent(100.0 - share), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|ends| {
                    words(ends, &fonts.body, 12.0, MUTED, chart.x_labels.0.clone());
                    words(ends, &fonts.body, 12.0, MUTED, chart.x_labels.1.clone());
                });
                if chart.draw.is_some() {
                    axis.spawn(Node { width: percent(share), justify_content: JustifyContent::Center, ..default() }).with_child((Text::new("draw"), UiFonts::style(&fonts.body, 12.0, MUTED)));
                }
            });
        });
}

/// The fight's answers and charts, and the one fight played out, built again when a run
/// finishes.
fn refresh_fight(mut commands: Commands, lab: Res<FightLab>, images: Res<FightImages>, fonts: Res<UiFonts>, results: Query<Entity, With<FightResults>>) {
    let Ok(results) = results.single() else { return };
    commands.entity(results).despawn_children().with_children(|r| {
        match (&lab.many, &lab.axes) {
            (Some((now, was)), Some((life, histogram))) => {
                r.spawn(Node { column_gap: px(12), ..default() }).with_children(|row| {
                    for (label, value, was) in fight_tiles(now, was) {
                        row.spawn((Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0), flex_direction: FlexDirection::Column, padding: UiRect::all(px(12)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(8)), ..default() }, BackgroundColor(CELL), BorderColor::all(FAINT)))
                            .with_children(|tile| {
                                words(tile, &fonts.body, 13.0, MUTED, label);
                                words(tile, &fonts.mono_bold, if value == "rarely wins" { 20.0 } else { 30.0 }, BRIGHT, value);
                                match was {
                                    Some(was) => words(tile, &fonts.body, 13.0, FRAME_GOLD, was),
                                    None => words(tile, &fonts.body, 13.0, MUTED, format!("of {} fights", number(now.fights as f64))),
                                }
                            });
                    }
                });
                r.spawn(Node { column_gap: px(12), ..default() }).with_children(|row| {
                    chart_figure(row, &fonts, "Their life, round by round", "gold: now \u{b7} grey: saved rules \u{b7} band: middle half of fights \u{b7} ended fights keep their final life", life, &images.life, 960.0 / 360.0, 3.0);
                    chart_figure(row, &fonts, "How many rounds a fight took", "gold: now \u{b7} grey: saved rules \u{b7} orange: draws", histogram, &images.histogram, 640.0 / 360.0, 2.0);
                });
            }
            _ => words(r, &fonts.body, 16.0, MUTED, "Press 1,000 fights to see how often each side hits, how long a fight lasts and who wins."),
        }
        r.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(6), padding: UiRect::all(px(12)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(8)), ..default() }, BackgroundColor(CELL), BorderColor::all(FAINT))).with_children(|log| {
            log.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|head| {
                words(head, &fonts.body_medium, 16.0, BRIGHT, "One fight, played out");
                words(head, &fonts.body, 13.0, MUTED, "hover a line for the dice");
            });
            match &lab.one {
                Some(record) => {
                    for (i, blow) in record.blows.iter().enumerate() {
                        log.spawn((LogLine(i), Node::default())).with_child((Text::new(blow.line.clone()), UiFonts::style(&fonts.body, 15.0, if blow.by_attacker { MUTED } else { BRIGHT })));
                    }
                    let end = match record.winner {
                        Some(crate::lab_fight::Side::Them) => format!("They win in {} rounds.", record.rounds),
                        Some(crate::lab_fight::Side::Attacker) => format!("{} wins in {} rounds.", record.attacker, record.rounds),
                        None => format!("Neither is down after {} rounds.", record.rounds),
                    };
                    words(log, &fonts.body_medium, 15.0, FRAME_GOLD, end);
                }
                None => words(log, &fonts.body, 15.0, MUTED, "Press One fight."),
            }
        });
    });
}

/// A blow's tooltip: the attack roll's terms, then the damage's.
fn blow_tip(blow: &crate::lab_fight::Blow) -> lab_sheet::Tip {
    let mut sections = vec![lab_sheet::TipSection { heading: Some(format!("Attack {}", number(blow.attack.roll.value()))), rows: lab_sheet::tip_rows(&blow.attack.roll) }];
    if let Some(damage) = &blow.damage {
        sections.push(lab_sheet::TipSection { heading: Some(format!("Damage {}", number(damage.value()))), rows: lab_sheet::tip_rows(damage) });
    }
    lab_sheet::Tip { title: format!("Against armour {}", blow.attack.armour_class), sections }
}

/// Which big number or played-out line the mouse is over, or the number a demo holds up.
fn hover_tiles(
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    tiles: Query<(&TileNode, &ComputedNode, &UiGlobalTransform)>,
    lines: Query<(&LogLine, &ComputedNode, &UiGlobalTransform)>,
    mut hover: ResMut<Hover>,
) {
    let point = cursor(&window);
    let over = |rect: &ComputedNode, transform: &UiGlobalTransform| point.is_some_and(|p| node_rect(rect, transform).contains(p));
    let tile = tiles.iter().find(|(_, n, t)| over(n, t)).map(|(tile, ..)| tile.0);
    let line = if tile.is_none() { lines.iter().find(|(_, n, t)| over(n, t)).map(|(l, ..)| l.0) } else { None };
    let tile = tile.or(if line.is_none() { hover.pinned } else { None });
    if hover.tile != tile || hover.line != line {
        hover.tile = tile;
        hover.line = line;
    }
}

/// Shows the tooltip of the hovered number or line under it, built again only when the hover,
/// the sheet or the fight changes.
#[allow(clippy::too_many_arguments)]
fn show_tip(
    mut commands: Commands,
    hover: Res<Hover>,
    current: Res<CurrentSheet>,
    fight: Res<FightLab>,
    fonts: Res<UiFonts>,
    tiles: Query<(&TileNode, &ComputedNode, &UiGlobalTransform)>,
    lines: Query<(&LogLine, &ComputedNode, &UiGlobalTransform)>,
    root: Query<&ComputedNode, With<LabRoot>>,
    mut tip: Query<(Entity, &mut Node, &mut Visibility, &ComputedNode), With<TipBox>>,
    mut shown: Local<Option<(bool, usize, u32, u32)>>,
    builds: Res<SheetBuilds>,
) {
    let Ok((entity, mut node, mut visibility, size)) = tip.single_mut() else { return };
    let content = match (hover.tile, hover.line) {
        (Some(i), _) => current.0.as_ref().and_then(|s| s.tiles.get(i)).map(|t| ((false, i, builds.0, 0), t.tip.clone())),
        (None, Some(i)) => fight.one.as_ref().and_then(|r| r.blows.get(i)).map(|b| ((true, i, 0, fight.generation), blow_tip(b))),
        _ => None,
    };
    let Some((key, content)) = content else {
        if *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
        *shown = None;
        return;
    };
    if *shown != Some(key) {
        *shown = Some(key);
        commands.entity(entity).despawn_children().with_children(|t| tip_body(t, &fonts, &content));
    }
    let rect = if key.0 {
        lines.iter().find(|(l, ..)| l.0 == key.1).map(|(_, n, t)| node_rect(n, t))
    } else {
        tiles.iter().find(|(t, ..)| t.0 == key.1).map(|(_, n, t)| node_rect(n, t))
    };
    let Some(rect) = rect else { return };
    let window = root.single().map_or(Vec2::MAX, |r| r.size * r.inverse_scale_factor);
    let height = size.size.y * size.inverse_scale_factor;
    let left = if key.0 { rect.min.x + 40.0 } else { rect.min.x };
    let (left, top) = (left.min(window.x - 270.0).max(8.0), (rect.max.y + 6.0).min(window.y - height - 8.0).max(8.0));
    if node.left != px(left) || node.top != px(top) {
        node.left = px(left);
        node.top = px(top);
    }
    if *visibility != Visibility::Visible {
        *visibility = Visibility::Visible;
    }
}

/// A tooltip's title and the label and value rows of each section.
fn tip_body(tip: &mut ChildSpawnerCommands, fonts: &UiFonts, content: &lab_sheet::Tip) {
    words(tip, &fonts.body_medium, 17.0, BRIGHT, content.title.clone());
    for section in &content.sections {
        if let Some(heading) = &section.heading {
            words(tip, &fonts.body_medium, 14.0, FRAME_GOLD, heading.clone());
        }
        for (label, value) in &section.rows {
            tip.spawn(Node { justify_content: JustifyContent::SpaceBetween, column_gap: px(12), ..default() }).with_children(|row| {
                words(row, &fonts.body, 15.0, MUTED, label.clone());
                words(row, &fonts.body_medium, 15.0, BRIGHT, value.clone());
            });
        }
    }
}

/// Scrolling moves the column under the mouse, if it has more than fits.
fn scroll_columns(
    scroll: Res<AccumulatedMouseScroll>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mut areas: Query<(&mut ScrollPosition, &ComputedNode, &UiGlobalTransform, &Node), With<ScrollArea>>,
) {
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    let Some(point) = cursor(&window) else { return };
    if lines == 0.0 {
        return;
    }
    for (mut position, node, transform, style) in &mut areas {
        if style.display == Display::None || style.overflow.y != OverflowAxis::Scroll || !node_rect(node, transform).contains(point) {
            continue;
        }
        let max = ((node.content_size() - node.size()) * node.inverse_scale_factor).y.max(0.0);
        position.y = (position.y - lines * SCROLL_LINE).clamp(0.0, max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    /// A seed whose rolled `base` has the affix named `affix`.
    fn seed_with(base: &str, affix: &str) -> u32 {
        (1..2000).find(|&s| roll_item(base, s).is_some_and(|i| i.affixes.iter().any(|a| a.name == affix))).unwrap_or_else(|| panic!("no {base} of {affix}"))
    }

    #[test]
    fn opens_on_a_human_fighter() {
        let state = LabState::from_args(args("--lab")).unwrap();
        assert_eq!((state.character.folk, state.character.class), (Folk::Human, Class::Fighter));
        assert_eq!(state.character.attributes.strength, 10);
    }

    #[test]
    fn the_command_line_rolls_a_character_from_the_seed() {
        let state = LabState::from_args(args("--lab --seed 7")).unwrap();
        assert_eq!(state.seed, 7);
        assert_eq!(state.character, Character::roll_folk(Folk::Human, 7));
        assert!(LabState::from_args(args("--seed x")).is_err());
        assert!(LabState::from_args(args("--wat")).is_err());
    }

    #[test]
    fn opens_dressed_and_armed() {
        let state = LabState::default();
        let held = state.slot(Slot::Mainhand).roll().unwrap().item;
        assert_eq!((held.base.as_str(), held.name.as_str()), ("Sword", "Messer"));
        assert_eq!(state.items().len(), 3);
    }

    #[test]
    fn reroll_takes_the_next_seed_and_keeps_the_folk() {
        let mut state = LabState::default();
        state.cycle_folk(1);
        state.reroll();
        assert_eq!((state.seed, state.character.folk), (DEFAULT_SEED + 1, Folk::Dwarf));
        assert_eq!(state.character, Character::roll_folk(Folk::Dwarf, DEFAULT_SEED + 1));
    }

    #[test]
    fn scores_step_within_three_and_twenty() {
        let mut state = LabState::default();
        state.step_score(0, 4);
        assert_eq!(state.character.attributes.strength, 14);
        state.step_score(0, 50);
        assert_eq!(state.character.attributes.strength, 20);
        state.step_score(4, -50);
        assert_eq!(state.character.attributes.wisdom, 3);
        state.step_score(9, 1);
    }

    #[test]
    fn folk_class_activity_and_attacker_cycle_both_ways() {
        let mut state = LabState::default();
        state.cycle_folk(1);
        assert_eq!(state.character.folk, Folk::Dwarf);
        state.cycle_folk(-2);
        assert_eq!(state.character.folk, Folk::Pigfolk);
        state.cycle_class(-1);
        assert_eq!(state.character.class, Class::Cleric);
        state.cycle_activity(-1);
        assert_eq!(state.activity, Activity::Resting);
        let was = state.attacker.clone();
        state.cycle_attacker(1);
        state.cycle_attacker(-1);
        assert_eq!(state.attacker, was);
    }

    #[test]
    fn a_card_can_be_taken_off_and_an_affix_flipped() {
        let mut state = LabState::default();
        state.remove(Slot::Head);
        assert_eq!(state.items().len(), 2);
        state.equip(Slot::Belt, "Belt", seed_with("Belt", "of the Ox"));
        state.flip_affix(Slot::Belt, "of the Ox");
        assert!(state.slot(Slot::Belt).off.contains("of the Ox"));
        state.flip_affix(Slot::Belt, "of the Ox");
        assert!(state.slot(Slot::Belt).off.is_empty());
    }

    #[test]
    fn the_picker_puts_a_chosen_base_and_seed_in_a_slot() {
        let mut state = LabState::default();
        let mut picker = Picker { open: true, ..Picker::default() };
        picker.step_slot(SLOTS.iter().position(|&s| s == Slot::Belt).unwrap() as isize);
        assert_eq!(picker.slot(), Slot::Belt);
        let ox = seed_with("Belt", "of the Ox");
        let at = bases_for(Slot::Belt).iter().position(|b| b == "Belt").unwrap();
        picker.step_base(at as isize);
        picker.step_seed(ox as i64 - 1);
        assert_eq!((picker.base_name().as_deref(), picker.seed), (Some("Belt"), ox));
        assert!(picker.preview().unwrap().name.contains("of the Ox"));
        picker.put_on(&mut state);
        assert!(!picker.open);
        assert_eq!(state.slot(Slot::Belt).base.as_deref(), Some("Belt"));
        picker.step_seed(-1_000_000);
        assert_eq!(picker.seed, 0);
    }

    /// Applies `control` to a fresh lab, with the saved rules as given.
    fn press(control: Control, state: &mut LabState, picker: &mut Picker, tab: &mut LabTab, saved: &mut RuleSet) {
        control.apply(state, picker, tab, saved);
    }

    #[test]
    fn controls_change_only_what_they_name() {
        let (mut state, mut picker, mut tab, mut saved) = (LabState::default(), Picker::default(), LabTab::default(), RuleSet::default());
        let was = state.clone();
        press(Control::Tab(1), &mut state, &mut picker, &mut tab, &mut saved);
        assert_eq!((tab, &state, &picker), (LabTab(1), &was, &Picker::default()));
        press(Control::Tab(3), &mut state, &mut picker, &mut tab, &mut saved);
        assert_eq!(tab, LabTab(1), "Effects waits for effects");
        press(Control::OpenPicker, &mut state, &mut picker, &mut tab, &mut saved);
        assert!(picker.open);
        press(Control::Score(1, 2), &mut state, &mut picker, &mut tab, &mut saved);
        assert_eq!(state.character.attributes.dexterity, 12);
        press(Control::StrikeSeed(-5), &mut state, &mut picker, &mut tab, &mut saved);
        assert_eq!(state.strike_seed, 0);
    }

    #[test]
    fn the_sheets_controls_do_what_they_say() {
        let (mut state, mut picker, mut tab, mut saved) = (LabState::default(), Picker::default(), LabTab::default(), RuleSet::default());
        let mut go = |c: Control, state: &mut LabState, picker: &mut Picker| press(c, state, picker, &mut tab, &mut saved);
        go(Control::Folk(1), &mut state, &mut picker);
        assert_eq!(state.character.folk, Folk::Dwarf);
        go(Control::Class(1), &mut state, &mut picker);
        assert_eq!(state.character.class, Class::Ranger);
        go(Control::Reroll, &mut state, &mut picker);
        assert_eq!(state.seed, DEFAULT_SEED + 1);
        go(Control::Remove(Slot::Head), &mut state, &mut picker);
        assert_eq!(state.slot(Slot::Head).base, None);
        let ox = seed_with("Belt", "of the Ox");
        state.equip(Slot::Belt, "Belt", ox);
        go(Control::Affix(Slot::Belt, "of the Ox".into()), &mut state, &mut picker);
        assert!(state.slot(Slot::Belt).off.contains("of the Ox"));
        go(Control::OpenPicker, &mut state, &mut picker);
        go(Control::PickSlot(1), &mut state, &mut picker);
        assert_eq!(picker.slot(), SLOTS[1]);
        go(Control::PickBase(1), &mut state, &mut picker);
        assert_eq!(picker.base, 1 % bases_for(picker.slot()).len());
        go(Control::PickSeed(5), &mut state, &mut picker);
        assert_eq!(picker.seed, 6);
        go(Control::PutOn, &mut state, &mut picker);
        assert!(!picker.open && state.slot(SLOTS[1]).base.is_some() && state.slot(SLOTS[1]).seed == 6);
        go(Control::OpenPicker, &mut state, &mut picker);
        go(Control::ClosePicker, &mut state, &mut picker);
        assert!(!picker.open);
    }

    #[test]
    fn editing_the_rules_moves_the_numbers_and_save_and_undo_settle_them() {
        use crate::lab_rules::changed;
        let (mut state, mut picker, mut tab, mut saved) = (LabState::default(), Picker::default(), LabTab::default(), RuleSet::default());
        let rat = "creatures/Giant Rat/attack_bonus".to_string();
        for _ in 0..2 {
            press(Control::Rule(rat.clone(), -1), &mut state, &mut picker, &mut tab, &mut saved);
        }
        assert_eq!(state.ruleset.rules.creatures["Giant Rat"].attack_bonus, 2);
        assert_eq!(changed(&state.ruleset, &saved), vec![rat]);
        press(Control::Undo, &mut state, &mut picker, &mut tab, &mut saved);
        assert!(changed(&state.ruleset, &saved).is_empty());
        press(Control::Rule("load/capacity_per_strength".into(), 1), &mut state, &mut picker, &mut tab, &mut saved);
        press(Control::Dice("creatures/Giant Rat/bite".into(), DicePart::Bonus, 1), &mut state, &mut picker, &mut tab, &mut saved);
        assert_eq!(changed(&state.ruleset, &saved).len(), 2);
        press(Control::Save, &mut state, &mut picker, &mut tab, &mut saved);
        assert!(changed(&state.ruleset, &saved).is_empty());
        assert_eq!(saved.rules.load.capacity_per_strength, 16.0);
    }

    #[test]
    fn a_stronger_affix_changes_the_gear_on_the_sheet() {
        let mut state = LabState::default();
        let ox = seed_with("Belt", "of the Ox");
        state.equip(Slot::Belt, "Belt", ox);
        let capacity = |s: &LabState| s.character.loadout(&s.inventory()).capacity_with(&s.ruleset.rules, 10).value();
        assert_eq!(capacity(&state), 175.0);
        crate::lab_rules::step(&mut state.ruleset, "affixes/of the Ox/0", 1);
        let ox_by = state.ruleset.strengths["of the Ox"][0];
        assert!(ox_by > 25.0, "{ox_by}");
        assert_eq!(capacity(&state), 150.0 + ox_by as f64);
    }

    #[test]
    fn every_slot_offers_a_base() {
        for slot in SLOTS {
            assert!(!bases_for(slot).is_empty(), "{slot:?}");
        }
    }

    #[test]
    fn the_smallest_window_leaves_the_tests_room() {
        assert!(MIN_WINDOW.x - SHEET_WIDTH - RULES_WIDTH >= 400.0);
        assert!(window().resize_constraints.min_width >= MIN_WINDOW.x);
    }

    #[test]
    fn the_picker_can_ask_for_a_rarity_and_finds_the_first_seed_that_rolls_it() {
        let mut picker = Picker { slot: SLOTS.iter().position(|&s| s == Slot::Belt).unwrap(), ..Picker::default() };
        picker.base = bases_for(Slot::Belt).iter().position(|b| b == "Belt").unwrap();
        picker.seed = 100;
        assert_eq!(picker.resolved_seed(), Some(100));
        for rarity in [Rarity::Common, Rarity::Magic, Rarity::Rare] {
            picker.rarity = Some(rarity);
            let seed = picker.resolved_seed().unwrap_or_else(|| panic!("no {rarity:?} belt"));
            assert!(seed >= 100);
            assert_eq!(picker.preview().unwrap().rarity, rarity);
            assert!((100..seed).all(|s| roll_item("Belt", s).unwrap().rarity != rarity), "the first one");
        }
        let mut state = LabState::default();
        picker.rarity = Some(Rarity::Magic);
        let seed = picker.resolved_seed().unwrap();
        picker.put_on(&mut state);
        assert_eq!(state.slot(Slot::Belt).seed, seed);
        assert_eq!(state.slot(Slot::Belt).roll().unwrap().item.rarity, Rarity::Magic);
    }

    #[test]
    fn the_equip_tab_controls_pick_slot_base_and_rarity_directly() {
        let (mut state, mut picker, mut tab, mut saved) = (LabState::default(), Picker::default(), LabTab::default(), RuleSet::default());
        press(Control::PickSlotAt(3), &mut state, &mut picker, &mut tab, &mut saved);
        assert_eq!(picker.slot(), Slot::Cloak);
        press(Control::PickBaseAt(1), &mut state, &mut picker, &mut tab, &mut saved);
        assert_eq!(picker.base, 1);
        press(Control::PickRarity(Some(Rarity::Rare)), &mut state, &mut picker, &mut tab, &mut saved);
        assert_eq!(picker.rarity, Some(Rarity::Rare));
        press(Control::PickSlotAt(0), &mut state, &mut picker, &mut tab, &mut saved);
        assert_eq!((picker.slot(), picker.base), (Slot::Head, 0), "a new slot starts at its first base");
    }
}
