//! The factions tab: the inventory screen's second leaf, in the character sheet's look. It lists
//! every faction the delver has met this delve, each with its crest and colours, its folk, what
//! it likes and hates, the delver's standing with it as a word and a bar, whether it has given
//! its key, and the depths where the delver met it (`WorldRecord::met_on`). A faction not met
//! does not appear, and neither does its name in another faction's likes.
//!
//! The crest is drawn flat from the faction's heraldic field (`armory::heraldry::Field`): its
//! first tincture, the division in the second on a six by six grid of cells, and the charge as
//! a small boss at the fess point. The banners show the same field in the level.

use armory::heraldry::{Division, Field, Tincture};
use std::collections::BTreeMap;

use bevy::prelude::*;
use fb_faction::{Band, Faction, Preference, World};

use super::sheet::band_colour;
use crate::play::CurrentLevel;
use crate::record::WorldRecord;
use crate::ui::{BG, CELL, FAINT, FRAME_GOLD, INK, LINE, MUTED, OK, UiFonts};

/// The leaf under the header when the factions tab is showing.
#[derive(Component, Debug)]
pub struct FactionsPage;

/// What the page says when no faction has been met.
pub const NO_FACTIONS_MET: &str = "You have met no faction yet. Speak with the folk you find in the dark.";

/// The crest's size in logical pixels.
const CREST_W: f32 = 46.0;
const CREST_H: f32 = 56.0;
/// Cells across and down the crest's field.
const CELLS: usize = 6;
/// The standing bar's width and height.
const BAR_PX: f32 = 200.0;
const BAR_HEIGHT_PX: f32 = 10.0;

/// One met faction as the page shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: Faction,
    pub name: String,
    pub field: Field,
    pub folk: String,
    pub colours: String,
    pub standing: i32,
    pub band: Band,
    pub likes: String,
    pub hates: String,
    pub key_given: bool,
    pub levels: Vec<u32>,
}

/// A tincture's colour on the page.
pub fn tincture_colour(tincture: Tincture) -> Color {
    let armory::material::Rgb(r, g, b) = tincture.rgb();
    Color::srgb(r, g, b)
}

/// The tincture's heraldic name: "azure", "argent".
pub fn tincture_word(tincture: Tincture) -> String {
    format!("{tincture:?}").to_lowercase()
}

/// "azure and argent, a tower or": the colours of a field in words.
pub fn colours_line(field: &Field) -> String {
    let mut line = tincture_word(field.tincture);
    if field.painted && field.division != Division::Plain && field.tincture2 != field.tincture {
        line = format!("{line} and {}", tincture_word(field.tincture2));
    }
    if field.charge != armory::sections::device::Device::None {
        line = format!("{line}, a {} {}", format!("{:?}", field.charge).to_lowercase(), tincture_word(field.charge_tincture));
    }
    line
}

/// Whether cell (`i` across, `j` down) of the crest's grid is in the field's second tincture.
pub fn second_tincture(division: Division, i: usize, j: usize) -> bool {
    let x = (i as f32 + 0.5) / CELLS as f32;
    let y = (j as f32 + 0.5) / CELLS as f32;
    let (i, j) = (i as i32, j as i32);
    match division {
        Division::Plain => false,
        Division::PerPale => x > 0.5,
        Division::PerFess => y > 0.5,
        Division::Quarterly => (x > 0.5) != (y > 0.5),
        Division::PerBend => y > x,
        Division::PerChevron => y > 0.35 + (x - 0.5).abs(),
        Division::PerSaltire => (x - 0.5).abs() < (y - 0.5).abs(),
        Division::Paly => i % 2 == 1,
        Division::Barry => j % 2 == 1,
        Division::Bendy => (i - j).rem_euclid(4) < 2,
        Division::Checky => (i + j) % 2 == 1,
        Division::Lozengy => (i + j) % 3 == 0,
    }
}

/// How a preference reads: creatures and classes plural, folk by their name, another faction by
/// its name once met.
pub fn preference_word(world: &World, record: &WorldRecord, preference: Preference) -> String {
    match preference {
        Preference::Creature(kind) => format!("{kind:?}").to_lowercase(),
        Preference::Class(class) => format!("{class:?}").to_lowercase() + "s",
        Preference::Folk(folk) => format!("{} folk", folk.race().label.to_lowercase()),
        Preference::Faction(id) if record.standings.has_met(id) => world.faction(id).name.clone(),
        Preference::Faction(_) => "a faction not yet met".to_string(),
    }
}

fn preferences_line(world: &World, record: &WorldRecord, list: &[Preference]) -> String {
    if list.is_empty() {
        return "nothing known".to_string();
    }
    let unmet = list.iter().filter(|p| matches!(p, Preference::Faction(id) if !record.standings.has_met(*id))).count();
    let mut words: Vec<String> = list.iter().filter(|p| !matches!(p, Preference::Faction(id) if !record.standings.has_met(*id))).map(|&p| preference_word(world, record, p)).collect();
    if unmet > 0 {
        words.push(unmet_phrase(unmet));
    }
    words.join(", ")
}

/// "a faction not yet met", "two factions not yet met", up to ten in words.
pub fn unmet_phrase(n: usize) -> String {
    const WORDS: [&str; 11] = ["no", "a", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten"];
    let count = WORDS.get(n).map_or(n.to_string(), |w| w.to_string());
    format!("{count} {} not yet met", if n == 1 { "faction" } else { "factions" })
}

/// The met factions of the world rolled from `run_seed`, in pool order.
pub fn entries(record: &WorldRecord, run_seed: u32) -> Vec<Entry> {
    let world = World::new(run_seed);
    record
        .standings
        .met()
        .map(|id| {
            let spec = world.faction(id);
            Entry {
                id,
                name: spec.name.clone(),
                field: spec.field,
                folk: spec.folk.race().label.to_string(),
                colours: colours_line(&spec.field),
                standing: record.standings.get(id).value(),
                band: record.standings.band(id),
                likes: preferences_line(&world, record, &spec.likes),
                hates: preferences_line(&world, record, &spec.hates),
                key_given: record.keys_given.contains(&id),
                levels: record.met_on.get(&id).map(|d| d.iter().copied().collect()).unwrap_or_default(),
            }
        })
        .collect()
}

/// "Depth 2", "Depths 2, 3 and 5", or "Met before".
pub fn levels_line(levels: &[u32]) -> String {
    match levels {
        [] => "Met before".to_string(),
        [one] => format!("Depth {one}"),
        [rest @ .., last] => format!("Depths {} and {last}", rest.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")),
    }
}

/// How full the standing bar is, 0 to 1, from -100 to 100.
pub fn standing_fill(standing: i32) -> f32 {
    ((standing + 100) as f32 / 200.0).clamp(0.0, 1.0)
}

/// The crest: a shield of the field's tinctures.
fn crest(parent: &mut ChildSpawnerCommands, field: &Field) {
    parent
        .spawn((
            Name::new("Crest"),
            Node {
                width: px(CREST_W),
                height: px(CREST_H),
                flex_shrink: 0.0,
                border: UiRect::all(px(2)),
                border_radius: BorderRadius { top_left: px(3).into(), top_right: px(3).into(), bottom_right: px(CREST_W * 0.5).into(), bottom_left: px(CREST_W * 0.5).into() },
                overflow: Overflow::clip(),
                position_type: PositionType::Relative,
                ..default()
            },
            BackgroundColor(tincture_colour(field.tincture)),
            BorderColor::all(FRAME_GOLD),
        ))
        .with_children(|shield| {
            if field.painted && field.division != Division::Plain {
                let second = tincture_colour(field.tincture2);
                for j in 0..CELLS {
                    for i in 0..CELLS {
                        if second_tincture(field.division, i, j) {
                            shield.spawn((
                                Node {
                                    position_type: PositionType::Absolute,
                                    left: percent(100.0 * i as f32 / CELLS as f32),
                                    top: percent(100.0 * j as f32 / CELLS as f32),
                                    width: percent(100.0 / CELLS as f32 + 1.0),
                                    height: percent(100.0 / CELLS as f32 + 1.0),
                                    ..default()
                                },
                                BackgroundColor(second),
                            ));
                        }
                    }
                }
            }
            if field.charge != armory::sections::device::Device::None {
                let size = 100.0 * field.charge_size.clamp(0.2, 0.8) * 0.7;
                shield.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(50.0 - size * 0.5),
                        top: percent(42.0 - size * 0.5),
                        width: percent(size),
                        height: percent(size * CREST_W / CREST_H),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(6)),
                        ..default()
                    },
                    BackgroundColor(tincture_colour(field.charge_tincture)),
                    BorderColor::all(BG),
                ));
            }
        });
}

/// A labelled line: a small label in muted ink, then the words.
fn fact(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, text: String, colour: Color) {
    parent.spawn(Node { column_gap: px(8), ..default() }).with_children(|line| {
        line.spawn(Node { width: px(56), flex_shrink: 0.0, ..default() }).with_child((Text::new(label), UiFonts::style(&fonts.body, 14.0, MUTED)));
        line.spawn((Text::new(text), UiFonts::style(&fonts.body, 14.0, colour), Node { flex_shrink: 1.0, ..default() }));
    });
}

/// Builds the factions tab's body: one card per met faction, or [`NO_FACTIONS_MET`].
pub fn page(parent: &mut ChildSpawnerCommands, fonts: &UiFonts, entries: &[Entry]) {
    parent.spawn((FactionsPage, Name::new("FactionsPage"), Node { flex_direction: FlexDirection::Column, row_gap: px(10), flex_shrink: 0.0, ..default() })).with_children(|page| {
        if entries.is_empty() {
            page.spawn((Text::new(NO_FACTIONS_MET), UiFonts::style(&fonts.body, 15.0, MUTED)));
        }
        for entry in entries {
            page.spawn((
                Name::new(format!("Faction {}", entry.name)),
                Node { column_gap: px(14), padding: UiRect::all(px(10)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(6)), ..default() },
                BackgroundColor(CELL),
                BorderColor::all(FAINT),
            ))
            .with_children(|card| {
                crest(card, &entry.field);
                card.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(3), flex_grow: 1.0, flex_shrink: 1.0, ..default() }).with_children(|text| {
                    text.spawn((Text::new(entry.name.clone()), UiFonts::style(&fonts.display, 22.0, INK)));
                    text.spawn((Text::new(format!("{} \u{b7} {}", entry.folk, entry.colours)), UiFonts::style(&fonts.body, 14.0, MUTED)));
                    text.spawn(Node { align_items: AlignItems::Center, column_gap: px(10), margin: UiRect::vertical(px(3)), ..default() }).with_children(|standing| {
                        standing
                            .spawn((
                                Node { width: px(BAR_PX), height: px(BAR_HEIGHT_PX), flex_shrink: 0.0, border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(3)), overflow: Overflow::clip(), ..default() },
                                BackgroundColor(BG),
                                BorderColor::all(LINE),
                            ))
                            .with_child((Node { width: percent(standing_fill(entry.standing) * 100.0), height: percent(100), ..default() }, BackgroundColor(band_colour(entry.band))));
                        standing.spawn((Text::new(format!("{} {:+}", entry.band.label(), entry.standing)), UiFonts::style(&fonts.mono_bold, 14.0, band_colour(entry.band))));
                    });
                    fact(text, fonts, "Likes", entry.likes.clone(), INK);
                    fact(text, fonts, "Hates", entry.hates.clone(), INK);
                    if entry.key_given {
                        fact(text, fonts, "Key", "Given you their key".to_string(), OK);
                    } else {
                        fact(text, fonts, "Key", "Not yet given".to_string(), MUTED);
                    }
                    fact(text, fonts, "Met", levels_line(&entry.levels), MUTED);
                });
            });
        }
    });
}

/// Notes the depth of the level on a faction whenever the delver meets it, however they do (a
/// talk, a trade, a blow, a kill), and whenever their standing with a faction whose folk are
/// on this level changes. The first run only takes stock of what was met before, so a record
/// from before depths were kept stays unnoted rather than guessed at.
pub fn note_levels_met(record: Option<ResMut<WorldRecord>>, level: Option<Res<CurrentLevel>>, mut before: Local<Option<BTreeMap<Faction, i32>>>) {
    let (Some(mut record), Some(level)) = (record, level) else { return };
    if before.is_some() && !record.is_changed() {
        return;
    }
    let now: BTreeMap<Faction, i32> = record.standings.met().map(|f| (f, record.standings.get(f).value())).collect();
    if let Some(was) = before.as_ref() {
        let here = |f: &Faction| level.0.factions.iter().any(|spec| spec.id == *f);
        let to_note: Vec<Faction> = now.iter().filter(|&(f, v)| was.get(f) != Some(v) && (!was.contains_key(f) || here(f))).map(|(f, _)| *f).collect();
        for faction in to_note {
            if !record.was_met_on(faction, level.0.depth) {
                record.note_met_on(faction, level.0.depth);
            }
        }
    }
    *before = Some(now);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_with(seed: u32, met: &[usize]) -> (WorldRecord, Vec<Faction>) {
        let mut record = WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, seed);
        let world = World::new(seed);
        let ids: Vec<Faction> = met.iter().map(|&i| world.factions()[i].id).collect();
        for &id in &ids {
            record.standings.meet(id);
        }
        (record, ids)
    }

    #[test]
    fn only_the_met_factions_are_listed_with_their_standing() {
        let (mut record, ids) = record_with(4, &[1, 3]);
        record.standings.shift(ids[0], 60);
        let listed = entries(&record, 4);
        assert_eq!(listed.iter().map(|e| e.id).collect::<Vec<_>>(), ids);
        assert_eq!(listed[0].standing, record.standings.get(ids[0]).value());
        assert_eq!(listed[0].band, record.standings.band(ids[0]));
        assert!(!listed.iter().any(|e| e.name.is_empty() || e.folk.is_empty() || e.likes.is_empty() || e.hates.is_empty()));
        assert!(entries(&record_with(4, &[]).0, 4).is_empty());
    }

    #[test]
    fn the_key_and_the_depths_are_shown() {
        let (mut record, ids) = record_with(4, &[2]);
        assert!(!entries(&record, 4)[0].key_given);
        record.keys_given.push(ids[0]);
        record.note_met_on(ids[0], 3);
        record.note_met_on(ids[0], 1);
        let listed = entries(&record, 4);
        assert!(listed[0].key_given);
        assert_eq!(listed[0].levels, vec![1, 3]);
        assert_eq!(levels_line(&[1, 3, 5]), "Depths 1, 3 and 5");
        assert_eq!(levels_line(&[2]), "Depth 2");
    }

    #[test]
    fn a_faction_not_met_is_not_named_among_another_s_likes() {
        let world = World::new(4);
        let record = WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, 4);
        let other = world.factions()[0].id;
        assert_eq!(preference_word(&world, &record, Preference::Faction(other)), "a faction not yet met");
    }

    #[test]
    fn unmet_factions_fold_into_one_phrase_at_the_end() {
        assert_eq!(unmet_phrase(1), "a faction not yet met");
        assert_eq!(unmet_phrase(2), "two factions not yet met");
        assert_eq!(unmet_phrase(3), "three factions not yet met");
        assert_eq!(unmet_phrase(10), "ten factions not yet met");
        assert_eq!(unmet_phrase(11), "11 factions not yet met");
        let world = World::new(4);
        let record = WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, 4);
        let (a, b) = (world.factions()[0].id, world.factions()[1].id);
        let line = preferences_line(&world, &record, &[Preference::Faction(a), Preference::Creature(fb_faction::CreatureKind::Rats), Preference::Faction(b)]);
        assert_eq!(line, "rats, two factions not yet met");
    }

    #[test]
    fn a_faction_met_now_is_noted_on_this_level_and_an_old_save_is_not_guessed() {
        let mut app = App::new();
        let mut record = WorldRecord::new_delve(fb_faction::Alignment::LawfulGood, 4);
        let world = World::new(4);
        let (old, new) = (world.factions()[0].id, world.factions()[1].id);
        record.standings.meet(old);
        app.insert_resource(record);
        let mut level = fb_level::generate(4, 3);
        level.depth = 3;
        app.insert_resource(CurrentLevel(level));
        app.add_systems(Update, note_levels_met);
        app.update();
        assert!(app.world().resource::<WorldRecord>().met_on.is_empty(), "met before depths were kept: nothing invented");
        assert_eq!(entries(app.world().resource::<WorldRecord>(), 4)[0].levels, Vec::<u32>::new());
        app.world_mut().resource_mut::<WorldRecord>().standings.meet(new);
        app.update();
        let record = app.world().resource::<WorldRecord>();
        assert!(record.was_met_on(new, 3) && !record.was_met_on(old, 3));
    }

    #[test]
    fn the_standing_bar_runs_from_hostile_to_allied() {
        assert_eq!(standing_fill(-100), 0.0);
        assert_eq!(standing_fill(0), 0.5);
        assert_eq!(standing_fill(100), 1.0);
    }

    #[test]
    fn a_plain_field_has_no_second_tincture_and_a_party_one_splits() {
        let cells = |d| (0..CELLS).flat_map(|j| (0..CELLS).map(move |i| second_tincture(d, i, j))).filter(|&b| b).count();
        assert_eq!(cells(Division::Plain), 0);
        assert_eq!(cells(Division::PerPale), CELLS * CELLS / 2);
        assert_eq!(cells(Division::Quarterly), CELLS * CELLS / 2);
        for division in Division::ALL {
            let n = cells(division);
            assert!(division == Division::Plain || (n > 0 && n < CELLS * CELLS), "{division:?} paints {n} cells");
        }
    }
}
