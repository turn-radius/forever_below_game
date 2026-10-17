//! The lab (`--lab`): a character built from sliders and choices on the left, and their sheet
//! with every derived number explained on the right. No level, creatures or audio load: the
//! lab only calls the rules the game does ([`fb_character`], [`crate::character`],
//! [`crate::combat`]), so what is balanced here is what the game plays.
//!
//! The left is the shared [`crate::tweak_panel`]: folk, class, the five attributes and the
//! character seed, then for each equip slot a base (or none), an item seed and a switch for
//! each rolled affix. Switching an affix off derives that item's stats again from its base
//! with the remaining affixes ([`LabState::items`]). The right is [`sheet`]: hit points, armour
//! class, the held weapon's damage range, load against capacity, darkvision, light and the
//! meter rates, each with its [`Explained`] terms beneath. Changing anything rebuilds the
//! sheet in the same frame.
//!
//! Below the sheet is the strike column ([`crate::lab_strike`]: an attacker, Strike, ×100 and
//! Heal, and the log), and an empty one ([`TimeColumn`]) where passing time is added. Command
//! line: `--lab --seed 7` rolls the character from the seed; with none it opens on a human Fighter with every score 10.

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

use armory::reflect::{Field, FieldKind};
use armory::slider::Slider;
use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    window::{PrimaryWindow, WindowResizeConstraints},
};
use fb_body::Folk;
use fb_character::explain::{Explained, number};
use fb_creatures::Species;
use fb_character::{Activity, Attributes, LoadBand, MeterKind, modifier};
use fb_items::armory_base::{ARMORY_BASES, roll_shape};
use fb_items::profile::profile;
use fb_items::tables::{Affix, bases};
use fb_items::{Item, Slot, roll_item};

use crate::character::{Character, Class};
use crate::combat::{Weapon, first_level_hit_points_explained};
use crate::inventory::Inventory;
use crate::lab_strike::{Attacker, Strikes};
use crate::lab_time::{ACTIVITIES, STEPS, TimeLab, activity_word, clock_text, exhaustion_effect, percent_text};
use crate::tweak_panel::{PanelBody, Tweak, chip, TweakPanelPlugin, TweakPanelSystems, over_body, panel_body};
use crate::ui::{node_rect, BG, BRIGHT, FAINT, INK, LINE, MUTED, UiFonts, numbers_line};

/// Width of the panel on the left, in logical pixels.
pub const PANEL_WIDTH: f32 = 430.0;
/// The smallest window the lab is laid out for: the panel and a sheet column beside it that
/// wraps its lines.
pub const MIN_WINDOW: Vec2 = Vec2::new(960.0, 540.0);
const DEFAULT_SEED: u32 = 1207;
/// Pixels one scroll line moves the sheet.
const SCROLL_LINE: f32 = 36.0;
/// The slots the lab dresses. The hip is left out, since a lantern hangs there from a belt.
const SLOTS: [Slot; 15] = [
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
            .init_resource::<Strikes>()
            .init_resource::<TimeLab>()
            .insert_resource(ClearColor(BG))
            .add_plugins(TweakPanelPlugin::<LabState>::default())
            .add_systems(Startup, (setup, demo))
            .add_systems(
            Update,
            (
                (press_strikes, press_time),
                rebuild_sheet.run_if(resource_changed::<LabState>),
                refresh_strikes.run_if(|state: Res<LabState>, strikes: Res<Strikes>| state.is_changed() || strikes.is_changed()),
                refresh_time.run_if(|state: Res<LabState>, time: Res<TimeLab>| state.is_changed() || time.is_changed()),
                layout_right,
                scroll_sheet,
            )
                .chain()
                .after(TweakPanelSystems),
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
        let base = self.base.as_deref()?;
        let mut item = roll_item(base, self.seed)?;
        let affixes = item.affixes.clone();
        if !self.off.is_empty() {
            let mut stats = profile(base, &roll_shape(base, self.seed)?).stats;
            item.affixes.retain(|a| !self.off.contains(&a.name));
            item.affixes.iter().for_each(|a| a.apply_to_stats(&mut stats));
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
    /// The seed every strike roll comes from.
    pub strike_seed: u32,
}

impl Default for LabState {
    /// A human Fighter with every score 10, holding a messer and wearing a sallet and a mail
    /// hauberk, none of which rolled an affix, so the first view has something to read.
    fn default() -> Self {
        let attributes = Attributes { strength: 10, dexterity: 10, constitution: 10, intelligence: 10, wisdom: 10 };
        let character = Character { folk: Folk::Human, body_seed: DEFAULT_SEED, name: "Lab delver".into(), class: Class::Fighter, attributes, dirt: 0.0, worn: fb_character::AbilityBonus::default() };
        let mut state = LabState { character, seed: DEFAULT_SEED, slots: SLOTS.iter().map(|&s| LabSlot::empty(s)).collect(), activity: Activity::Walking, attacker: Attacker::Creature(Species::GiantRat), strike_seed: 1 };
        for (slot, base, seed) in [(Slot::Mainhand, "Sword", 4), (Slot::Head, "Helm", 9), (Slot::Body, "Mail", 9)] {
            state.equip(slot, base, seed);
        }
        state
    }
}

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

    /// Switches the affix named `affix` on `slot`'s item on or off.
    pub fn set_affix(&mut self, slot: Slot, affix: &str, on: bool) {
        let s = self.slot_mut(slot);
        if on {
            s.off.remove(affix);
        } else {
            s.off.insert(affix.to_string());
        }
    }

    /// Everything worn, rolled.
    pub fn items(&self) -> Vec<(Slot, Rolled)> {
        self.slots.iter().filter_map(|s| Some((s.slot, s.roll()?))).collect()
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

/// The bases a slot can hold, by name: the ones that roll an item that fits it.
fn bases_for(slot: Slot) -> Vec<String> {
    use std::sync::LazyLock;
    static TABLE: LazyLock<Vec<(Slot, Vec<String>)>> = LazyLock::new(|| {
        let mut names: BTreeSet<&str> = ARMORY_BASES.iter().map(|(n, _)| *n).collect();
        names.extend(bases().keys().map(String::as_str));
        let rolled: Vec<(&str, Item)> = names.into_iter().filter_map(|n| Some((n, roll_item(n, 1)?))).collect();
        SLOTS.iter().map(|&slot| (slot, rolled.iter().filter(|(_, i)| i.fits(slot)).map(|(n, _)| n.to_string()).collect())).collect()
    });
    TABLE.iter().find(|(s, _)| *s == slot).map(|(_, b)| b.clone()).unwrap_or_default()
}

/// The path prefix of a slot's fields: `gear.mainhand`.
fn slot_key(slot: Slot) -> String {
    format!("gear.{}", format!("{slot:?}").to_lowercase())
}

fn slot_of(key: &str) -> Option<Slot> {
    SLOTS.iter().copied().find(|&s| slot_key(s) == key)
}

fn slider(path: &str, value: f32, range: Slider) -> Field {
    let (section, name) = path.rsplit_once('.').unwrap_or(("", path));
    Field { path: path.into(), section: section.into(), name: name.into(), kind: FieldKind::Slider { value, range } }
}

fn choice(path: &str, value: &str, options: Vec<String>) -> Field {
    let (section, name) = path.rsplit_once('.').unwrap_or(("", path));
    Field { path: path.into(), section: section.into(), name: name.into(), kind: FieldKind::Choice { value: value.into(), options } }
}

/// An affix's switch: its path is the slot's, then `affix/` and the affix's name, which is
/// the field's label.
fn affix_switch(key: &str, affix: &str, on: bool) -> Field {
    Field { path: format!("{key}.affix/{affix}"), section: key.into(), name: affix.into(), kind: FieldKind::Toggle { value: on } }
}

const ATTRIBUTES: [(&str, &str); 5] = [("str", "strength"), ("dex", "dexterity"), ("con", "constitution"), ("int", "intelligence"), ("wis", "wisdom")];
const SCORE: Slider = Slider::new(3.0, 20.0, 1.0);
const SEED: Slider = Slider::new(0.0, 65_535.0, 1.0);

impl Tweak for LabState {
    fn fields(&self) -> Vec<Field> {
        let c = &self.character;
        let mut fields = vec![
            choice("character.folk", c.folk.key(), Folk::ALL.iter().map(|f| f.key().to_string()).collect()),
            choice("character.class", c.class.word(), Class::ALL.iter().map(|c| c.word().to_string()).collect()),
            slider("character.seed", self.seed as f32, SEED),
        ];
        let scores = [c.attributes.strength, c.attributes.dexterity, c.attributes.constitution, c.attributes.intelligence, c.attributes.wisdom];
        for ((short, _), score) in ATTRIBUTES.iter().zip(scores) {
            fields.push(slider(&format!("attributes.{short}"), score as f32, SCORE));
        }
        let activities = ACTIVITIES.iter().map(|&a| activity_word(a).to_string()).collect();
        fields.push(choice("time.activity", activity_word(self.activity), activities));
        let attackers = Attacker::all();
        let who = attackers.iter().map(Attacker::label).collect();
        fields.push(choice("attacker.who", &self.attacker.label(), who));
        fields.push(slider("attacker.seed", self.strike_seed as f32, SEED));
        for lab in &self.slots {
            let key = slot_key(lab.slot);
            let mut options = vec!["none".to_string()];
            options.extend(bases_for(lab.slot));
            fields.push(choice(&format!("{key}.base"), lab.base.as_deref().unwrap_or("none"), options));
            if lab.base.is_none() {
                continue;
            }
            fields.push(slider(&format!("{key}.seed"), lab.seed as f32, SEED));
            if let Some(rolled) = lab.roll() {
                fields.extend(rolled.affixes.iter().map(|a| affix_switch(&key, &a.name, !lab.off.contains(&a.name))));
                if rolled.item.shape.lit().is_some() {
                    fields.push(Field { path: format!("{key}.lit"), section: key.clone(), name: "lit".into(), kind: FieldKind::Toggle { value: lab.lit } });
                }
            }
        }
        fields
    }

    fn set_slider(&mut self, path: &str, value: f32) {
        let value = value.round().max(0.0) as u32;
        match path.split_once('.') {
            Some(("character", "seed")) => self.set_seed(value),
            Some(("attacker", "seed")) => self.strike_seed = value,
            Some(("attributes", short)) => {
                let a = &mut self.character.attributes;
                let score = (value as i32).clamp(3, 20);
                match short {
                    "str" => a.strength = score,
                    "dex" => a.dexterity = score,
                    "con" => a.constitution = score,
                    "int" => a.intelligence = score,
                    "wis" => a.wisdom = score,
                    _ => {}
                }
            }
            _ => {
                if let Some((key, "seed")) = path.rsplit_once('.')
                    && let Some(slot) = slot_of(key)
                {
                    let s = self.slot_mut(slot);
                    s.seed = value;
                    s.off.clear();
                }
            }
        }
    }

    fn cycle(&mut self, path: &str, by: isize) {
        let step = |len: usize, at: usize| (at as isize + by).rem_euclid(len as isize) as usize;
        match path {
            "character.folk" => {
                let at = Folk::ALL.iter().position(|&f| f == self.character.folk).unwrap_or(0);
                self.character.folk = Folk::ALL[step(Folk::ALL.len(), at)];
            }
            "character.class" => {
                let at = Class::ALL.iter().position(|&c| c == self.character.class).unwrap_or(0);
                self.character.class = Class::ALL[step(Class::ALL.len(), at)];
            }
            "time.activity" => {
                let at = ACTIVITIES.iter().position(|&a| a == self.activity).unwrap_or(0);
                self.activity = ACTIVITIES[step(ACTIVITIES.len(), at)];
            }
            "attacker.who" => {
                let all = Attacker::all();
                let at = all.iter().position(|a| *a == self.attacker).unwrap_or(0);
                self.attacker = all[step(all.len(), at)].clone();
            }
            _ => {
                if let Some((key, "base")) = path.rsplit_once('.')
                    && let Some(slot) = slot_of(key)
                {
                    let mut options = vec![None];
                    options.extend(bases_for(slot).into_iter().map(Some));
                    let s = self.slot_mut(slot);
                    let at = options.iter().position(|o| *o == s.base).unwrap_or(0);
                    s.base = options[step(options.len(), at)].clone();
                    s.off.clear();
                }
            }
        }
    }

    fn flip(&mut self, path: &str) {
        let Some((key, last)) = path.rsplit_once('.') else { return };
        let Some(slot) = slot_of(key) else { return };
        if last == "lit" {
            let s = self.slot_mut(slot);
            s.lit = !s.lit;
        } else if let Some(name) = last.strip_prefix("affix/") {
            let on = self.slot(slot).off.contains(name);
            self.set_affix(slot, name, on);
        }
    }

    /// Changes with the list of fields, so the panel is rebuilt when an item's affixes change
    /// but not as a slider moves.
    fn structure(&self) -> u32 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for field in self.fields() {
            field.path.hash(&mut hasher);
        }
        hasher.finish() as u32
    }

    fn slider(&self, path: &str) -> Option<f32> {
        self.fields().into_iter().find(|f| f.path == path).and_then(|f| match f.kind {
            FieldKind::Slider { value, .. } => Some(value),
            _ => None,
        })
    }
}

/// One line of the sheet: what it says, and the number with the terms it is made of.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The headline: the value with its unit, and anything the terms cannot say (a band).
    pub heading: String,
    pub explained: Explained,
}

impl Row {
    fn of(explained: Explained) -> Row {
        Row { heading: format!("{} {}{}", explained.name, number(explained.value()), if explained.unit.is_empty() { String::new() } else { format!(" {}", explained.unit) }), explained }
    }
}

/// The sheet: every derived number the lab shows for `state`, from the same rules the game
/// reads, in the order they are listed.
pub fn sheet(state: &LabState) -> Vec<Row> {
    let character = &state.character;
    let inventory = state.inventory();
    let loadout = character.loadout(&inventory);
    let strength = character.strength();
    let held = inventory.worn(Slot::Mainhand).and_then(|id| inventory.item(id));
    let weapon = Weapon::of_stats(held);
    let range = weapon.damage_range_explained(modifier(strength));
    let capacity = loadout.capacity(strength);
    let load = character.load_explained(&inventory);
    let band = LoadBand::from_load(load.value(), capacity.value());
    let mut rows = vec![
        Row::of(first_level_hit_points_explained(character)),
        Row::of(character.armour_class(&inventory)),
        Row::of(range.min),
        Row::of(range.mean),
        Row::of(range.max),
        Row { heading: format!("Load {} of {} lb: {}", number(load.value()), number(capacity.value()), band.word()), explained: load },
        Row::of(capacity),
        Row::of(loadout.darkvision()),
        Row::of(loadout.light()),
        Row::of(loadout.warmth()),
    ];
    for kind in [MeterKind::Hunger, MeterKind::Thirst, MeterKind::Sleepiness] {
        rows.push(Row::of(loadout.rate(kind, state.activity)));
    }
    rows
}

/// How many times the sheet has been built, so a test can see it is not rebuilt for nothing.
#[derive(Resource, Default)]
pub struct SheetBuilds(pub u32);

/// The window, so the right side can see how wide it is.
#[derive(Component)]
struct LabRoot;

/// What is right of the panel: one column that scrolls when the window is narrow, and the
/// sheet and the side column laid out beside each other when it is wide.
#[derive(Component)]
struct RightRegion;

/// The sheet's pane.
#[derive(Component)]
struct SheetColumn;

/// The strikes above the time, in a pane of their own.
#[derive(Component)]
struct SideColumn;

/// A node that scrolls its own overflow when it has any.
#[derive(Component)]
struct ScrollArea;

/// How wide the window must be, in logical pixels, to lay the sheet and the side column out
/// as two panes that scroll on their own.
pub const WIDE: f32 = 1500.0;

fn pane() -> Node {
    Node { min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(4), padding: UiRect::axes(px(20), px(14)), ..default() }
}

/// Where the sheet's rows are built.
#[derive(Component)]
struct SheetRows;

/// Where the strike log goes.
#[derive(Component)]
pub struct StrikeColumn;

/// A button in the strike column.
#[derive(Component, Clone, Copy, PartialEq)]
enum StrikeButton {
    Strike,
    Hundred,
    Heal,
}

/// A line of text in the strike column.
#[derive(Component, Clone, Copy, PartialEq)]
enum StrikeText {
    Note,
    Hp,
    Summary,
    Log,
}

/// Where passing time goes.
#[derive(Component)]
pub struct TimeColumn;

/// A button in the time column: pass the nth of [`STEPS`], eat, drink or start over.
#[derive(Component, Clone, Copy, PartialEq)]
enum TimeButton {
    Pass(usize),
    Eat,
    Drink,
    Reset,
}

/// A part of the time column.
#[derive(Component, Clone, Copy, PartialEq)]
enum TimeText {
    Clock,
    /// Holds the meters' rows, built again as the time changes.
    Meters,
    Exhaustion,
    Log,
}

fn setup(mut commands: Commands, fonts: Res<UiFonts>) {
    commands.spawn((Camera2d, bevy::ui::IsDefaultUiCamera));
    commands.spawn((LabRoot, Node { width: percent(100), height: percent(100), flex_direction: FlexDirection::Row, ..default() })).with_children(|root| {
        root.spawn((
            Node { width: px(PANEL_WIDTH), flex_shrink: 0.0, flex_direction: FlexDirection::Column, border: UiRect::right(px(1)), ..default() },
            BackgroundColor(BG),
            BorderColor::all(LINE),
        ))
        .with_children(|panel| {
            panel
                .spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(6), padding: UiRect::all(px(14)), border: UiRect::bottom(px(1)), ..default() }, BorderColor::all(FAINT)))
                .with_children(|header| {
                    header.spawn((Text::new("The Lab"), UiFonts::style(&fonts.display_bold, 28.0, INK)));
                    header.spawn((Text::new("Click a choice to step it, drag a slider, flip a switch."), UiFonts::style(&fonts.body, 16.0, MUTED)));
                });
            panel_body(panel);
        });
        root.spawn((
            RightRegion,
            ScrollArea,
            ScrollPosition::default(),
            Node { flex_grow: 1.0, min_width: px(0), flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), ..default() },
        ))
        .with_children(|right| {
        right
            .spawn((SheetColumn, ScrollArea, ScrollPosition::default(), pane()))
            .with_children(|column| {
                column.spawn((Text::new("The Sheet"), UiFonts::style(&fonts.display_bold, 28.0, INK)));
                column.spawn((SheetRows, Node { flex_direction: FlexDirection::Column, row_gap: px(10), margin: UiRect::top(px(8)), ..default() }));
            });
        right.spawn((SideColumn, ScrollArea, ScrollPosition::default(), pane())).with_children(|column| {
            column.spawn((StrikeColumn, Node { flex_direction: FlexDirection::Column, row_gap: px(8), margin: UiRect::top(px(24)), ..default() })).with_children(|strikes| {
                strikes.spawn((Text::new("Strikes"), UiFonts::style(&fonts.display_bold, 28.0, INK)));
                strikes.spawn((StrikeText::Note, Text::new(""), UiFonts::style(&fonts.body, 16.0, MUTED)));
                strikes.spawn(Node { column_gap: px(8), ..default() }).with_children(|row| {
                    chip(row, &fonts, "Strike", StrikeButton::Strike);
                    chip(row, &fonts, "×100", StrikeButton::Hundred);
                    chip(row, &fonts, "Heal", StrikeButton::Heal);
                });
                strikes.spawn((StrikeText::Hp, Text::new(""), UiFonts::style(&fonts.display, 21.0, BRIGHT)));
                strikes.spawn((StrikeText::Summary, Text::new(""), UiFonts::style(&fonts.mono, 15.0, BRIGHT)));
                strikes.spawn((StrikeText::Log, Text::new(""), UiFonts::style(&fonts.mono, 14.0, MUTED)));
            });
            column.spawn((TimeColumn, Node { flex_direction: FlexDirection::Column, row_gap: px(8), margin: UiRect::top(px(24)), padding: UiRect::bottom(px(30)), ..default() })).with_children(|time| {
                time.spawn((Text::new("Time"), UiFonts::style(&fonts.display_bold, 28.0, INK)));
                time.spawn((TimeText::Clock, Text::new(""), UiFonts::style(&fonts.display, 21.0, BRIGHT)));
                time.spawn(Node { column_gap: px(8), flex_wrap: FlexWrap::Wrap, row_gap: px(8), ..default() }).with_children(|row| {
                    for (i, (label, _)) in STEPS.iter().enumerate() {
                        chip(row, &fonts, label, TimeButton::Pass(i));
                    }
                    chip(row, &fonts, "Eat", TimeButton::Eat);
                    chip(row, &fonts, "Drink", TimeButton::Drink);
                    chip(row, &fonts, "Reset", TimeButton::Reset);
                });
                time.spawn((TimeText::Meters, Node { flex_direction: FlexDirection::Column, row_gap: px(8), ..default() }));
                time.spawn((TimeText::Exhaustion, Text::new(""), UiFonts::style(&fonts.display, 21.0, BRIGHT)));
                time.spawn((TimeText::Log, Text::new(""), UiFonts::style(&fonts.mono, 14.0, MUTED)));
            });
        });
        });
    });
}

/// Builds the sheet again from the state.
fn rebuild_sheet(mut builds: ResMut<SheetBuilds>, mut commands: Commands, state: Res<LabState>, fonts: Res<UiFonts>, rows: Query<Entity, With<SheetRows>>) {
    let Ok(root) = rows.single() else { return };
    builds.0 += 1;
    commands.entity(root).despawn_children();
    let rows = sheet(&state);
    commands.entity(root).with_children(|root| {
        for row in &rows {
            root.spawn(Node { flex_direction: FlexDirection::Column, border: UiRect::bottom(px(1)), padding: UiRect::bottom(px(6)), ..default() })
                .insert(BorderColor::all(FAINT))
                .with_children(|block| {
                    block.spawn((Text::new(row.heading.clone()), UiFonts::style(&fonts.display, 21.0, BRIGHT)));
                    let terms = row.explained.terms_line();
                    numbers_line(block, &fonts, &if terms.is_empty() { "= nothing adds to it".to_string() } else { format!("= {terms}") }, 15.0);
                });
        }
    });
}

/// With `FB_LAB_DEMO=1`, does what a few clicks would, for a capture: +8 h once and Strike
/// three times.
fn demo(state: Res<LabState>, mut time: ResMut<TimeLab>, mut strikes: ResMut<Strikes>) {
    if std::env::var("FB_LAB_DEMO").is_ok_and(|v| v == "1") {
        time.pass(&state, 8.0);
        (0..3).for_each(|_| strikes.strike(&state));
    }
}

/// A click on Strike, ×100 or Heal.
fn press_strikes(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    controls: Query<(&StrikeButton, &ComputedNode, &UiGlobalTransform)>,
    state: Res<LabState>,
    mut strikes: ResMut<Strikes>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(point) = window.and_then(|w| w.cursor_position()) else { return };
    let Some((button, ..)) = controls.iter().find(|(_, node, transform)| node_rect(node, transform).contains(point)) else { return };
    match button {
        StrikeButton::Strike => strikes.strike(&state),
        StrikeButton::Hundred => strikes.strike_hundred(&state),
        StrikeButton::Heal => strikes.heal(),
    }
}

/// A click on a time button.
fn press_time(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    controls: Query<(&TimeButton, &ComputedNode, &UiGlobalTransform)>,
    state: Res<LabState>,
    mut time: ResMut<TimeLab>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(point) = window.and_then(|w| w.cursor_position()) else { return };
    let Some((button, ..)) = controls.iter().find(|(_, node, transform)| node_rect(node, transform).contains(point)) else { return };
    match *button {
        TimeButton::Pass(i) => time.pass(&state, STEPS[i].1),
        TimeButton::Eat => time.eat(),
        TimeButton::Drink => time.drink(),
        TimeButton::Reset => time.reset(),
    }
}

/// Shows the clock, each meter with its band and explained rate, the exhaustion and the log.
fn refresh_time(
    mut commands: Commands,
    state: Res<LabState>,
    time: Res<TimeLab>,
    fonts: Res<UiFonts>,
    mut texts: Query<(Entity, &TimeText, Option<&mut Text>)>,
) {
    let loadout = state.character.loadout(&state.inventory());
    for (entity, which, text) in &mut texts {
        let line = match which {
            TimeText::Clock => format!("{} · {}", clock_text(time.clock.hours()), activity_word(state.activity)),
            TimeText::Exhaustion => {
                let e = time.exhaustion;
                format!("Exhaustion {} of 6: {}", e.level, exhaustion_effect(e))
            }
            TimeText::Log => time.log.iter().map(|c| c.line()).collect::<Vec<_>>().join("
"),
            TimeText::Meters => {
                let meters = [(MeterKind::Hunger, time.needs.hunger), (MeterKind::Thirst, time.needs.thirst), (MeterKind::Sleepiness, time.needs.sleepiness)];
                commands.entity(entity).despawn_children().with_children(|rows| {
                    for (kind, meter) in meters {
                        let rate = loadout.rate(kind, state.activity);
                        rows.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|block| {
                            block.spawn((Text::new(format!("{} {} · {}", rate.name.trim_end_matches(" rate"), percent_text(&meter), meter.word())), UiFonts::style(&fonts.display, 21.0, BRIGHT)));
                            numbers_line(block, &fonts, &rate.line(), 15.0);
                        });
                    }
                });
                continue;
            }
        };
        if let Some(mut text) = text {
            if text.0 != line {
                text.0 = line;
            }
        }
    }
}

/// Shows the attacker, the hit points, the last hundred and the log.
fn refresh_strikes(state: Res<LabState>, strikes: Res<Strikes>, mut texts: Query<(&StrikeText, &mut Text)>) {
    let (hp, full) = strikes.hit_points(&state);
    for (which, mut text) in &mut texts {
        let line = match which {
            StrikeText::Note => match &state.attacker {
                Attacker::Creature(_) => format!("{} strikes with its bite. The strike seed sets every roll.", state.attacker.label()),
                Attacker::Folk(_) => format!("A {}: STR 10, the base rolled from the strike seed.", state.attacker.label()),
            },
            StrikeText::Hp => format!("Hit points {} of {full} · AC {}", hp.max(0), crate::lab_strike::armour_class(&state)),
            StrikeText::Summary => strikes.summary.clone().unwrap_or_else(|| "×100 strikes a hundred times from the seed.".into()),
            StrikeText::Log => strikes.log.iter().cloned().collect::<Vec<_>>().join("\n"),
        };
        if text.0 != line {
            text.0 = line;
        }
    }
}

/// Lays the right side out for the window's width ([`WIDE`]): two panes that scroll on their
/// own, or the panes stacked in one column that scrolls.
fn layout_right(root: Query<&ComputedNode, With<LabRoot>>, mut right: Query<&mut Node, With<RightRegion>>, mut panes: Query<&mut Node, (Or<(With<SheetColumn>, With<SideColumn>)>, Without<RightRegion>)>) {
    let Ok(root) = root.single() else { return };
    let wide = is_wide(root.size.x * root.inverse_scale_factor);
    for mut node in &mut right {
        lay_out_right(&mut node, wide);
    }
    for mut node in &mut panes {
        lay_out_pane(&mut node, wide);
    }
}

/// Whether a window `width` logical pixels across gets two panes.
pub fn is_wide(width: f32) -> bool {
    width >= WIDE
}

/// The region right of the panel: its panes in a row when `wide`, else in a column that scrolls.
fn lay_out_right(node: &mut Node, wide: bool) {
    let (direction, overflow) = if wide { (FlexDirection::Row, Overflow::visible()) } else { (FlexDirection::Column, Overflow::scroll_y()) };
    if node.flex_direction != direction {
        node.flex_direction = direction;
        node.overflow = overflow;
    }
}

/// One pane: it scrolls and shares the width when `wide`, else it grows to its content.
fn lay_out_pane(node: &mut Node, wide: bool) {
    let (overflow, grow, basis) = if wide { (Overflow::scroll_y(), 1.0, px(0)) } else { (Overflow::visible(), 0.0, auto()) };
    if node.overflow != overflow {
        node.overflow = overflow;
        node.flex_grow = grow;
        node.flex_basis = basis;
    }
}

/// Scrolling away from the panel moves the pane under the mouse, or the whole right side when
/// it is one column.
fn scroll_sheet(
    scroll: Res<AccumulatedMouseScroll>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    body: Query<(&ComputedNode, &UiGlobalTransform), With<PanelBody>>,
    mut areas: Query<(&mut ScrollPosition, &ComputedNode, &UiGlobalTransform, &Node), With<ScrollArea>>,
) {
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    let Some(window) = window else { return };
    let Some(point) = window.cursor_position() else { return };
    if lines == 0.0 || over_body(&window, &body) {
        return;
    }
    for (mut position, node, transform, style) in &mut areas {
        if style.overflow.y != OverflowAxis::Scroll || !node_rect(node, transform).contains(point) {
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

    fn line_of(state: &LabState, name: &str) -> String {
        sheet(state).into_iter().find(|r| r.explained.name == name).unwrap_or_else(|| panic!("no {name} row")).explained.line()
    }

    fn value_of(state: &LabState, name: &str) -> f64 {
        sheet(state).into_iter().find(|r| r.explained.name == name).unwrap().explained.value()
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
        assert_eq!(line_of(&state, "Capacity"), "Capacity 150 lb = STR 10 × 15 (150)");
        assert_eq!(value_of(&state, "Hit points"), 10.0);
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
        assert_eq!(value_of(&state, "Armour class"), 15.0, "10 + sallet 2 + hauberk 3");
        assert_eq!((value_of(&state, "Least damage"), value_of(&state, "Mean damage"), value_of(&state, "Most damage")), (1.0, 4.5, 8.0));
        assert!(value_of(&state, "Load") > 0.0);
        assert!(sheet(&state).iter().any(|r| r.explained.name == "Warmth"));
    }

    #[test]
    fn raising_strength_to_14_lifts_capacity_and_damage() {
        let mut state = LabState::default();
        assert_eq!(value_of(&state, "Capacity"), 150.0);
        state.set_slider("attributes.str", 14.0);
        assert_eq!(value_of(&state, "Capacity"), 210.0);
        assert_eq!((value_of(&state, "Least damage"), value_of(&state, "Mean damage"), value_of(&state, "Most damage")), (3.0, 6.5, 10.0));
    }

    #[test]
    fn a_belt_of_the_ox_adds_25_and_switching_it_off_removes_it() {
        let mut state = LabState::default();
        state.equip(Slot::Belt, "Belt", seed_with("Belt", "of the Ox"));
        let line = line_of(&state, "Capacity");
        assert!(line.contains("+ ") && line.contains("of the Ox (+25)"), "{line}");
        assert_eq!(value_of(&state, "Capacity"), 175.0);
        state.flip(&format!("{}.affix/of the Ox", slot_key(Slot::Belt)));
        assert_eq!(line_of(&state, "Capacity"), "Capacity 150 lb = STR 10 × 15 (150)");
        state.flip(&format!("{}.affix/of the Ox", slot_key(Slot::Belt)));
        assert_eq!(value_of(&state, "Capacity"), 175.0);
    }

    /// What `base` rolled from `seed` has for stats with only `kept` applied to its base
    /// profile, in the order given: the oracle for [`LabSlot::roll`].
    fn stats_with(base: &str, seed: u32, kept: &[&Affix]) -> fb_items::stats::Stats {
        let mut stats = profile(base, &roll_shape(base, seed).unwrap()).stats;
        for affix in kept {
            affix.fx.iter().for_each(|m| m.apply_to_stats(&mut stats));
        }
        stats
    }

    fn slot_of_item(base: &str, seed: u32, off: &[&str]) -> LabSlot {
        LabSlot { base: Some(base.into()), seed, off: off.iter().map(|n| n.to_string()).collect(), ..LabSlot::empty(Slot::Belt) }
    }

    #[test]
    fn with_no_affix_off_the_item_is_the_rolled_item() {
        for (base, seed) in [("Belt", 2), ("Sword", 5), ("Mail", 1), ("Torch", 4)] {
            let plain = roll_item(base, seed).unwrap();
            let rolled = slot_of_item(base, seed, &[]).roll().unwrap();
            assert_eq!(rolled.item.profile.stats, plain.profile.stats, "{base} {seed}");
            assert_eq!(rolled.item.name, plain.name);
            assert_eq!(rolled.item.profile.stats, stats_with(base, seed, &plain.affixes), "the oracle agrees with roll_item");
        }
    }

    #[test]
    fn switching_one_of_two_affixes_off_keeps_the_other() {
        let (base, seed) = ["Belt", "Sword", "Mail", "Helm", "Ring", "Cloak"]
            .iter()
            .flat_map(|b| (1..3000).map(move |s| (*b, s)))
            .find(|&(b, s)| roll_item(b, s).is_some_and(|i| i.affixes.len() >= 2))
            .expect("some item rolls two affixes");
        let plain = roll_item(base, seed).unwrap();
        let (gone, kept) = (plain.affixes[0], &plain.affixes[1..]);
        let rolled = slot_of_item(base, seed, &[&gone.name]).roll().unwrap();
        assert_eq!(rolled.item.profile.stats, stats_with(base, seed, kept), "{base} {seed} without {}", gone.name);
        assert_eq!(rolled.item.affixes, kept);
        assert_eq!(rolled.affixes, plain.affixes, "the switch still lists what it rolled");
        assert!(!rolled.item.name.contains(&gone.name), "{} in {}", gone.name, rolled.item.name);
        // Every affix off is the bare base.
        let off: Vec<&str> = plain.affixes.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(slot_of_item(base, seed, &off).roll().unwrap().item.profile.stats, stats_with(base, seed, &[]));
    }

    #[test]
    fn the_sheet_agrees_with_the_games_own_numbers() {
        let mut state = LabState::default();
        for (slot, base, seed) in [(Slot::Body, "Mail", 1), (Slot::Belt, "Belt", seed_with("Belt", "of the Ox")), (Slot::Mainhand, "Sword", 3), (Slot::Head, "Helm", 2)] {
            state.equip(slot, base, seed);
        }
        let inventory = state.inventory();
        let derived = state.character.derive(&inventory);
        assert_eq!(value_of(&state, "Armour class"), derived.armour_class as f64);
        assert_eq!(value_of(&state, "Capacity"), derived.capacity as f64);
        assert!((value_of(&state, "Load") - derived.load as f64).abs() < 1e-4);
        assert_eq!(value_of(&state, "Darkvision"), derived.darkvision as f64);
        assert_eq!(value_of(&state, "Light"), derived.light as f64);
        assert!((value_of(&state, "Warmth") - derived.warmth as f64).abs() < 1e-4);
        let weapon = Weapon::in_hand(&inventory, &state.character);
        let (least, most) = weapon.damage_range(modifier(state.character.strength()));
        assert_eq!((value_of(&state, "Least damage") as u32, value_of(&state, "Most damage") as u32), (least, most));
    }

    #[test]
    fn the_panel_lists_folk_class_scores_and_every_slot() {
        let mut state = LabState::default();
        let paths: Vec<String> = state.fields().into_iter().map(|f| f.path).collect();
        for want in ["character.folk", "character.class", "character.seed", "attributes.str", "attributes.wis", "gear.head.base", "gear.mainhand.base", "gear.belt.base"] {
            assert!(paths.iter().any(|p| p == want), "{want} in {paths:?}");
        }
        assert!(!paths.iter().any(|p| p == "gear.belt.seed"), "an empty slot has no seed");
        state.cycle("gear.belt.base", 1);
        assert!(state.slot(Slot::Belt).base.is_some());
        assert!(state.fields().iter().any(|f| f.path == "gear.belt.seed"));
        state.cycle("gear.belt.base", -1);
        assert_eq!(state.slot(Slot::Belt).base, None);
        // Every choice offers none and bases that fit.
        for slot in SLOTS {
            assert!(!bases_for(slot).is_empty(), "{slot:?}");
        }
        state.cycle("character.folk", 1);
        assert_eq!(state.character.folk, Folk::Dwarf);
        state.cycle("character.class", -1);
        assert_eq!(state.character.class, Class::Cleric);
    }

    #[test]
    fn the_panel_is_rebuilt_only_when_its_rows_change() {
        let mut state = LabState::default();
        let structure = state.structure();
        state.set_slider("attributes.str", 14.0);
        assert_eq!(state.structure(), structure, "a slider only moves a value");
        state.equip(Slot::Belt, "Belt", 2);
        assert_ne!(state.structure(), structure);
    }

    #[test]
    fn the_panel_offers_every_attacker_and_a_strike_seed() {
        let mut state = LabState::default();
        let who = state.fields().into_iter().find(|f| f.path == "attacker.who").unwrap();
        let FieldKind::Choice { options, .. } = who.kind else { panic!("a choice") };
        for species in Species::ALL {
            assert!(options.contains(&species.stat_block().name.to_string()), "{options:?}");
        }
        assert!(options.contains(&"folk with Sword".to_string()), "{options:?}");
        state.cycle("attacker.who", -1);
        assert!(matches!(state.attacker, Attacker::Creature(Species::Rat)));
        state.set_slider("attacker.seed", 42.0);
        assert_eq!(state.strike_seed, 42);
    }

    #[test]
    fn the_rate_rows_follow_the_chosen_activity() {
        let mut state = LabState::default();
        let walking = value_of(&state, "Hunger rate");
        state.cycle("time.activity", -1);
        assert_eq!(state.activity, Activity::Resting);
        assert_eq!(value_of(&state, "Hunger rate"), walking * 0.5);
        state.cycle("time.activity", 2);
        assert_eq!(state.activity, Activity::Exerting);
        assert_eq!(value_of(&state, "Hunger rate"), walking * 2.0);
    }

    #[test]
    fn the_right_side_is_two_scrolling_panes_when_wide_and_one_column_when_not() {
        assert!(is_wide(1920.0) && is_wide(WIDE) && !is_wide(WIDE - 1.0) && !is_wide(MIN_WINDOW.x));
        let (mut right, mut pane) = (Node { flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), ..default() }, Node::default());
        lay_out_right(&mut right, true);
        lay_out_pane(&mut pane, true);
        assert_eq!((right.flex_direction, right.overflow), (FlexDirection::Row, Overflow::visible()));
        assert_eq!((pane.overflow, pane.flex_grow), (Overflow::scroll_y(), 1.0));
        lay_out_right(&mut right, false);
        lay_out_pane(&mut pane, false);
        assert_eq!((right.flex_direction, right.overflow), (FlexDirection::Column, Overflow::scroll_y()));
        assert_eq!((pane.overflow, pane.flex_grow), (Overflow::visible(), 0.0));
    }

    #[test]
    fn the_smallest_window_leaves_the_sheet_room() {
        assert!(MIN_WINDOW.x - PANEL_WIDTH >= 480.0);
        assert!(window().resize_constraints.min_width >= MIN_WINDOW.x);
    }
}
