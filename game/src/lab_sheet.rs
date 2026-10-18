//! What the lab's character sheet says ([`crate::lab_view`] draws it): the figure's captions,
//! the five attributes, the bars, the three big numbers with the tooltips that explain them,
//! and a card for each thing worn or held. Plain data from the lab's state and the loaded rules,
//! so it runs without Bevy's renderer and is tested as text.
//!
//! The terms of an [`Explained`] number are shown as label and value rows ([`tip_rows`]), never
//! as a formula: `Base 10`, `DEX +0`, `Sallet +2`.

use bevy::prelude::Color;
use fb_character::explain::{Explained, Op, number};
use fb_character::rules::Rules;
use fb_character::{Meter, modifier};
use fb_items::tables::slot_places;
use fb_items::{Rarity, Slot};

use crate::combat::{Weapon, first_level_hit_points_explained_with};
use crate::lab_rules::was_text;
use crate::lab_time::TimeLab;
use crate::lab_view::{LabState, Rolled};
use crate::ui::{ACCENT, BAD, OK};

/// The five attributes, in the order shown.
pub const ATTRIBUTE_NAMES: [&str; 5] = ["STR", "DEX", "CON", "INT", "WIS"];
/// The levels of exhaustion above none, the last being death.
pub const EXHAUSTION_STEPS: u32 = fb_character::Exhaustion::DEATH;

/// One attribute tile.
#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    pub name: &'static str,
    pub score: i32,
    pub modifier: i32,
}

/// A bar: its name, how full it is from 0 to 1, the words beside it, and its colour.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub label: &'static str,
    pub fill: f32,
    pub text: String,
    pub colour: Color,
    /// What the text was under the saved rules, when the edited ones moved it: `was 25 s`.
    pub was: Option<String>,
}

/// A group of tooltip rows under an optional heading.
#[derive(Clone, Debug, PartialEq)]
pub struct TipSection {
    pub heading: Option<String>,
    pub rows: Vec<(String, String)>,
}

/// What hovering a big number shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Tip {
    pub title: String,
    pub sections: Vec<TipSection>,
}

/// A big number: Armour, Damage or Load.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub label: &'static str,
    pub value: String,
    /// What follows the value in a smaller size: the capacity after the load.
    pub small: Option<String>,
    pub tip: Tip,
    /// What it was under the saved rules, when the edited ones moved it: `was 13`.
    pub was: Option<String>,
}

/// An affix on a card: what it does, its name, whether it helps, and whether it is on.
#[derive(Clone, Debug, PartialEq)]
pub struct AffixRow {
    pub name: String,
    pub text: String,
    pub good: bool,
    pub on: bool,
}

/// A card for one thing worn or held.
#[derive(Clone, Debug, PartialEq)]
pub struct GearCard {
    pub slot: Slot,
    pub title: String,
    pub rarity: Rarity,
    pub slot_word: String,
    pub affixes: Vec<AffixRow>,
    /// What it adds, one per line: `AC +2`, `1d8`, `4 lb`.
    pub adds: Vec<String>,
}

/// Everything the sheet shows.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetView {
    /// `Human Fighter`.
    pub title: String,
    /// `seed 1207`.
    pub caption: String,
    pub attributes: Vec<Attribute>,
    pub life: Bar,
    pub stamina: Bar,
    pub mana: Bar,
    pub needs: Vec<Bar>,
    /// The exhaustion level, 0 to [`EXHAUSTION_STEPS`], and its words: `0 of 6`.
    pub exhaustion: (u32, String),
    pub tiles: Vec<Tile>,
    pub cards: Vec<GearCard>,
}

/// A word with its first letter capital.
pub fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// A signed amount: `+2`, `+0`, `−1`.
pub fn signed(amount: f64) -> String {
    if amount < 0.0 { format!("\u{2212}{}", number(-amount)) } else { format!("+{}", number(amount)) }
}

/// What a slot is called on a card: `Head`, `Main hand`, `Ring`.
pub fn slot_word(slot: Slot) -> String {
    match slot {
        Slot::Ring1 | Slot::Ring2 => "Ring".to_string(),
        Slot::Body => "Body".to_string(),
        _ => slot_places().iter().find(|p| p.slot == slot).map_or_else(|| format!("{slot:?}"), |p| capitalised(&p.label.to_lowercase())),
    }
}

/// A term's label as the tooltip writes it: an attribute and its score (`DEX 10`) is the
/// attribute alone, and the first letter is capital.
fn term_label(label: &str) -> String {
    if is_attribute_label(label) { label[..3].to_string() } else { capitalised(label) }
}

/// Whether a term's label is an attribute and its score: `DEX 10`.
fn is_attribute_label(label: &str) -> bool {
    label.split_once(' ').is_some_and(|(name, score)| ATTRIBUTE_NAMES.contains(&name) && score.parse::<i32>().is_ok())
}

/// The rows a number's terms make: each term's label and what it brought.
pub fn tip_rows(explained: &Explained) -> Vec<(String, String)> {
    explained
        .terms()
        .iter()
        // A worn thing that adds nothing (a sword's armour) is left out; an attribute stays at +0.
        .filter(|t| !(t.op == Op::Add && t.amount == 0.0 && !is_attribute_label(&t.label)))
        .map(|t| {
            let amount = match t.op {
                Op::Base | Op::BestOf => number(t.amount),
                Op::Add => signed(t.amount),
                Op::Multiply => format!("\u{d7}{}", number(t.amount)),
                Op::Clamp { .. } => number(t.amount),
                _ => number(t.amount),
            };
            (term_label(&t.label), amount)
        })
        .collect()
}

fn section(heading: Option<String>, explained: &Explained) -> TipSection {
    TipSection { heading, rows: tip_rows(explained) }
}

fn bar(label: &'static str, fill: f32, text: impl Into<String>, colour: Color) -> Bar {
    Bar { label, fill: fill.clamp(0.0, 1.0), text: text.into(), colour, was: None }
}

/// A need's bar: the meter's value, so a full bar is the worst, in the band's word.
fn need(label: &'static str, meter: &Meter, rules: &Rules) -> Bar {
    let colour = match meter.band_with(rules) {
        0 | 1 => OK,
        2 => ACCENT,
        _ => BAD,
    };
    bar(label, meter.value as f32, meter.word_with(rules), colour)
}

fn card(slot: Slot, rolled: &Rolled, off: &std::collections::BTreeSet<String>) -> GearCard {
    let item = &rolled.item;
    let s = &item.profile.stats;
    let mut adds = Vec::new();
    if s.armour != 0.0 {
        adds.push(format!("AC {}", signed(s.armour as f64)));
    }
    if let Some(dice) = s.damage {
        let bonus = if s.damage_bonus != 0.0 { signed(s.damage_bonus as f64) } else { String::new() };
        adds.push(format!("{dice}{bonus}"));
    }
    adds.push(format!("{} lb", number(s.weight as f64)));
    GearCard {
        slot,
        title: item.title().to_string(),
        rarity: item.rarity,
        slot_word: slot_word(slot),
        affixes: rolled.affixes.iter().map(|a| AffixRow { name: a.name.clone(), text: a.text.clone(), good: a.good, on: !off.contains(&a.name) }).collect(),
        adds,
    }
}

/// The card of an item not yet put on, for the picker's preview: every affix on.
pub fn preview_card(slot: Slot, item: &fb_items::Item) -> GearCard {
    card(slot, &Rolled { item: item.clone(), affixes: item.affixes.clone() }, &std::collections::BTreeSet::new())
}

/// The sheet for `state`, with the time the lab has had so far, under `rules`.
pub fn view(state: &LabState, time: &TimeLab, rules: &Rules) -> SheetView {
    let character = &state.character;
    let inventory = state.inventory();
    let loadout = character.loadout(&inventory);
    let strength = character.strength();
    let held = inventory.worn(Slot::Mainhand).and_then(|id| inventory.item(id));
    let range = Weapon::of_stats(held).damage_range_explained(modifier(strength));
    let capacity = loadout.capacity_with(rules, strength);
    let load = character.load_explained(&inventory);
    let armour = character.armour_class_with(rules, &inventory);
    let (least, most) = (range.min.value(), range.max.value());
    let hit_points = first_level_hit_points_explained_with(rules, character).value() as i32;
    let hp = hit_points;
    let attributes = [character.attributes.strength, character.attributes.dexterity, character.attributes.constitution, character.attributes.intelligence, character.attributes.wisdom]
        .iter()
        .zip(ATTRIBUTE_NAMES)
        .map(|(&score, name)| Attribute { name, score, modifier: modifier(score) })
        .collect();
    let mana = character.mana_pool_with(rules);
    let tiles = vec![
        Tile { label: "Armour", value: number(armour.value().round()), small: None, tip: Tip { title: format!("Armour {}", number(armour.value().round())), sections: vec![section(None, &armour)] }, was: None },
        Tile {
            label: "Damage",
            value: format!("{}\u{2013}{}", number(least), number(most)),
            small: None,
            tip: Tip {
                title: format!("Damage {}\u{2013}{}", number(least), number(most)),
                sections: vec![section(Some(format!("Least {}", number(least))), &range.min), section(Some(format!("Most {}", number(most))), &range.max), section(Some(format!("Average {}", number(range.mean.value()))), &range.mean)],
            },
            was: None,
        },
        Tile {
            label: "Load",
            value: number(load.value()),
            small: Some(format!(" / {}", number(capacity.value()))),
            tip: Tip { title: format!("Load {} of {} lb", number(load.value()), number(capacity.value())), sections: vec![section(Some("Carrying".into()), &load), section(Some("Can carry".into()), &capacity)] },
            was: None,
        },
    ];
    SheetView {
        title: format!("{} {}", character.folk.race().label, capitalised(character.class.word())),
        caption: format!("seed {}", state.seed),
        attributes,
        life: bar("Life", hp as f32 / hit_points.max(1) as f32, format!("{} / {hit_points}", hp.max(0)), crate::lab_view::LIFE),
        stamina: bar("Stamina", 1.0, format!("{} s", number(rules.stamina.sprint_seconds)), crate::lab_view::STAMINA),
        mana: bar("Mana", 1.0, format!("{} / {}", number(mana), number(mana)), crate::lab_view::MANA),
        needs: [("Hunger", &time.needs.hunger), ("Thirst", &time.needs.thirst), ("Sleepiness", &time.needs.sleepiness)].into_iter().map(|(label, meter)| need(label, meter, rules)).collect(),
        exhaustion: (time.exhaustion.level, if time.exhaustion.is_dead() { "Dead".to_string() } else { format!("{} of {EXHAUSTION_STEPS}", time.exhaustion.level) }),
        tiles,
        cards: state.slots.iter().filter_map(|s| Some(card(s.slot, &s.roll()?, &s.off))).collect(),
    }
}

/// The sheet for `state` under its own rules, with what each number was under `saved` beside the
/// ones the edit moved ([`crate::lab_rules::was_text`]).
pub fn view_marked(state: &LabState, time: &TimeLab, saved: &crate::lab_rules::RuleSet) -> SheetView {
    let mut now = view(state, time, &state.ruleset.rules);
    let before = view(&LabState { ruleset: saved.clone(), ..state.clone() }, time, &saved.rules);
    for (tile, was) in now.tiles.iter_mut().zip(&before.tiles) {
        tile.was = if tile.value == was.value {
            let small = |t: &Tile| t.small.as_deref().map(|s| s.trim_start_matches(" / ").to_string()).unwrap_or_default();
            was_text(&small(tile), &small(was))
        } else {
            was_text(&format!("{}{}", tile.value, tile.small.clone().unwrap_or_default()), &format!("{}{}", was.value, was.small.clone().unwrap_or_default()))
        };
    }
    let pairs = |v: &SheetView| -> Vec<String> { [&v.life, &v.stamina, &v.mana].into_iter().chain(&v.needs).map(|b| b.text.clone()).collect() };
    let was_texts = pairs(&before);
    for (bar, was) in [&mut now.life, &mut now.stamina, &mut now.mana].into_iter().chain(now.needs.iter_mut()).zip(was_texts) {
        bar.was = was_text(&bar.text, &was);
    }
    now
}

/// Every word on the sheet, for a test that no formula is written out.
pub fn sheet_text(view: &SheetView) -> Vec<String> {
    let mut text = vec![view.title.clone(), view.caption.clone()];
    text.extend(view.attributes.iter().map(|a| format!("{} {} {}", a.name, a.score, signed(a.modifier as f64))));
    text.extend([&view.life, &view.stamina, &view.mana].into_iter().chain(&view.needs).map(|b| format!("{} {}", b.label, b.text)));
    text.push(view.exhaustion.1.clone());
    for tile in &view.tiles {
        text.push(format!("{} {}{}", tile.label, tile.value, tile.small.clone().unwrap_or_default()));
    }
    for card in &view.cards {
        text.push(format!("{} {}", card.title, card.slot_word));
        text.extend(card.affixes.iter().map(|a| format!("{}: {}", a.name, a.text)));
        text.extend(card.adds.iter().cloned());
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_character::rules::defaults;
    use fb_items::roll_item;

    fn default_view() -> SheetView {
        view(&LabState::default(), &TimeLab::default(), defaults())
    }

    fn seed_with(base: &str, affix: &str) -> u32 {
        (1..2000).find(|&s| roll_item(base, s).is_some_and(|i| i.affixes.iter().any(|a| a.name == affix))).unwrap_or_else(|| panic!("no {base} of {affix}"))
    }

    fn tile<'a>(view: &'a SheetView, label: &str) -> &'a Tile {
        view.tiles.iter().find(|t| t.label == label).unwrap()
    }

    #[test]
    fn the_default_fighter_reads_as_a_sheet() {
        let view = default_view();
        assert_eq!((view.title.as_str(), view.caption.as_str()), ("Human Fighter", "seed 1207"));
        assert_eq!((tile(&view, "Armour").value.as_str(), tile(&view, "Damage").value.as_str()), ("15", "1\u{2013}8"));
        let load = tile(&view, "Load");
        assert_eq!((load.value.as_str(), load.small.as_deref()), ("27", Some(" / 150")));
        assert_eq!(view.attributes.iter().map(|a| (a.name, a.score, a.modifier)).collect::<Vec<_>>(), vec![("STR", 10, 0), ("DEX", 10, 0), ("CON", 10, 0), ("INT", 10, 0), ("WIS", 10, 0)]);
        assert_eq!((view.life.text.as_str(), view.mana.text.as_str(), view.stamina.text.as_str()), ("10 / 10", "4 / 4", "30 s"));
        assert_eq!(view.needs.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(), vec!["Fed", "Quenched", "Rested"]);
        assert_eq!(view.exhaustion, (0, "0 of 6".to_string()));
    }

    #[test]
    fn the_armour_tooltip_lists_its_terms_as_rows() {
        let view = default_view();
        let rows = &tile(&view, "Armour").tip.sections[0].rows;
        let row = |label: &str, value: &str| (label.to_string(), value.to_string());
        assert_eq!(rows, &vec![row("Base", "10"), row("DEX", "+0"), row("Sallet", "+2"), row("Mail Hauberk", "+3")]);
        assert_eq!(tile(&view, "Armour").tip.title, "Armour 15");
    }

    #[test]
    fn the_default_fighter_wears_a_sallet_and_a_hauberk_and_holds_a_messer() {
        let view = default_view();
        let cards: Vec<(&str, &str, &[String])> = view.cards.iter().map(|c| (c.title.as_str(), c.slot_word.as_str(), c.adds.as_slice())).collect();
        assert_eq!(cards.len(), 3, "{cards:?}");
        for (title, slot, adds) in &cards {
            match *title {
                "Sallet" => assert_eq!((*slot, adds[0].as_str()), ("Head", "AC +2")),
                "Mail Hauberk" => assert_eq!((*slot, adds[0].as_str()), ("Body", "AC +3")),
                "Messer" => assert_eq!((*slot, adds[0].as_str()), ("Main hand", "1d8")),
                other => panic!("{other}"),
            }
        }
    }

    #[test]
    fn a_belt_of_the_ox_adds_its_card_and_lifts_capacity_to_175() {
        let mut state = LabState::default();
        state.equip(Slot::Belt, "Belt", seed_with("Belt", "of the Ox"));
        let view = view(&state, &TimeLab::default(), defaults());
        assert_eq!(tile(&view, "Load").small.as_deref(), Some(" / 175"));
        let belt = view.cards.iter().find(|c| c.slot == Slot::Belt).expect("the belt's card");
        assert!(belt.title.contains("of the Ox"), "{}", belt.title);
        assert!(belt.affixes.iter().any(|a| a.name == "of the Ox" && a.on), "{:?}", belt.affixes);
        let capacity = &tile(&view, "Load").tip.sections[1];
        assert!(capacity.rows.iter().any(|(label, value)| label.contains("of the Ox") && value == "+25"), "{capacity:?}");
    }

    #[test]
    fn an_affix_switched_off_stays_on_the_card_but_stops_counting() {
        let mut state = LabState::default();
        state.equip(Slot::Belt, "Belt", seed_with("Belt", "of the Ox"));
        state.set_affix(Slot::Belt, "of the Ox", false);
        let view = view(&state, &TimeLab::default(), defaults());
        assert_eq!(tile(&view, "Load").small.as_deref(), Some(" / 150"));
        let belt = view.cards.iter().find(|c| c.slot == Slot::Belt).unwrap();
        assert!(belt.affixes.iter().any(|a| a.name == "of the Ox" && !a.on));
    }

    #[test]
    fn strength_moves_the_damage_and_the_load() {
        let mut state = LabState::default();
        state.set_score(0, 14);
        let view = view(&state, &TimeLab::default(), defaults());
        assert_eq!((tile(&view, "Damage").value.as_str(), tile(&view, "Load").small.as_deref()), ("3\u{2013}10", Some(" / 210")));
        assert_eq!(view.attributes[0].modifier, 2);
    }

    #[test]
    fn the_loaded_rules_set_the_numbers() {
        let mut rules = defaults().clone();
        rules.combat.armour_base = 12;
        rules.load.capacity_per_strength = 20.0;
        rules.stamina.sprint_seconds = 45.0;
        let view = view(&LabState::default(), &TimeLab::default(), &rules);
        assert_eq!(tile(&view, "Armour").value, "17");
        assert_eq!(tile(&view, "Load").small.as_deref(), Some(" / 200"));
        assert_eq!(view.stamina.text, "45 s");
    }

    #[test]
    fn time_shows_in_the_bars() {
        let state = LabState::default();
        let mut time = TimeLab::default();
        time.pass(&state, 8.0);
        let view = view(&state, &time, defaults());
        assert_eq!(view.needs[0].text, "Hungry");
        assert!(view.needs[0].fill > 0.4);
        assert!(view.life.text.ends_with("/ 10"));
    }

    #[test]
    fn no_text_on_the_sheet_is_a_formula() {
        let mut state = LabState::default();
        state.equip(Slot::Belt, "Belt", seed_with("Belt", "of the Ox"));
        let view = view(&state, &TimeLab::default(), defaults());
        for line in sheet_text(&view) {
            assert!(!line.contains(" = "), "{line}");
        }
    }

    #[test]
    fn term_rows_are_labels_and_amounts() {
        let e = Explained::new("Thing", "").base("base", 10.0).add("DEX 14", 2.0).add("Cursed", -1.0).times("Halved", 0.5);
        let row = |l: &str, v: &str| (l.to_string(), v.to_string());
        assert_eq!(tip_rows(&e), vec![row("Base", "10"), row("DEX", "+2"), row("Cursed", "\u{2212}1"), row("Halved", "\u{d7}0.5")]);
    }

    #[test]
    fn slots_are_named_as_the_doll_names_them() {
        assert_eq!(slot_word(Slot::Head), "Head");
        assert_eq!(slot_word(Slot::Ring2), "Ring");
        assert!(slot_word(Slot::Mainhand).to_lowercase().contains("hand"), "{}", slot_word(Slot::Mainhand));
    }

    #[test]
    fn a_number_the_edit_moved_says_what_it_was() {
        let saved = crate::lab_rules::RuleSet::default();
        let mut state = LabState::default();
        let plain = view_marked(&state, &TimeLab::default(), &saved);
        assert!(plain.tiles.iter().all(|t| t.was.is_none()) && plain.stamina.was.is_none());
        crate::lab_rules::set_leaf(&mut state.ruleset, "load/capacity_per_strength", crate::lab_rules::Leaf::Float(20.0));
        crate::lab_rules::set_leaf(&mut state.ruleset, "combat/armour_base", crate::lab_rules::Leaf::Int(13));
        crate::lab_rules::set_leaf(&mut state.ruleset, "stamina/sprint_seconds", crate::lab_rules::Leaf::Float(25.0));
        let view = view_marked(&state, &TimeLab::default(), &saved);
        let load = tile(&view, "Load");
        assert_eq!((load.value.as_str(), load.small.as_deref(), load.was.as_deref()), ("27", Some(" / 200"), Some("was 150")));
        let armour = tile(&view, "Armour");
        assert_eq!((armour.value.as_str(), armour.was.as_deref()), ("18", Some("was 15")));
        assert_eq!((view.stamina.text.as_str(), view.stamina.was.as_deref()), ("25 s", Some("was 30 s")));
        assert!(tile(&view, "Damage").was.is_none());
    }

    #[test]
    fn exhaustion_at_the_death_level_reads_dead() {
        let mut time = TimeLab { exhaustion: fb_character::Exhaustion::new(5), ..Default::default() };
        assert_eq!(view(&LabState::default(), &time, defaults()).exhaustion, (5, "5 of 6".to_string()));
        time.exhaustion = fb_character::Exhaustion::new(6);
        assert_eq!(view(&LabState::default(), &time, defaults()).exhaustion, (6, "Dead".to_string()));
    }

    #[test]
    fn taking_the_mail_off_drops_armour_by_3_and_load_by_20_pounds() {
        let mut state = LabState::default();
        let with = view(&state, &TimeLab::default(), defaults());
        state.remove(Slot::Body);
        let without = view(&state, &TimeLab::default(), defaults());
        assert_eq!((tile(&with, "Armour").value.as_str(), tile(&without, "Armour").value.as_str()), ("15", "12"));
        assert_eq!((tile(&with, "Load").value.as_str(), tile(&without, "Load").value.as_str()), ("27", "7"));
        assert!(without.cards.iter().all(|c| c.slot != Slot::Body));
    }

    #[test]
    fn a_moth_eaten_cloak_shows_its_flaw_on_its_card() {
        let seed = (1..4000).find(|&s| roll_item("Cloak", s).is_some_and(|i| i.affixes.iter().any(|a| a.name == "Moth-eaten"))).expect("some cloak is moth-eaten");
        let item = roll_item("Cloak", seed).unwrap();
        let preview = preview_card(Slot::Cloak, &item);
        assert!(preview.affixes.iter().any(|a| a.name == "Moth-eaten" && !a.good && a.on), "{:?}", preview.affixes);
        let mut state = LabState::default();
        state.equip(Slot::Cloak, "Cloak", seed);
        let view = view(&state, &TimeLab::default(), defaults());
        let worn = view.cards.iter().find(|c| c.slot == Slot::Cloak).unwrap();
        assert!(worn.affixes.iter().any(|a| a.name == "Moth-eaten"), "{:?}", worn.affixes);
    }
}
