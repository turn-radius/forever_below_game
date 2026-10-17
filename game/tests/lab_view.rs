//! The lab builds as an app without a window or a GPU (as `delve_scope` runs the game): its
//! plugin has no clashing queries, and the sheet is built, and rebuilt when the state changes.

use bevy::{
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    time::TimeUpdateStrategy,
    window::ExitCondition,
    winit::WinitPlugin,
};
use fb_items::Slot;
use forever_below::lab_view::{Control, ControlPressed, DataFolder, Hover, LabState, LabTab, LabViewPlugin, RulesColumn, SheetBuilds, TabPane, TimeColumn};

fn headless_lab() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(RenderPlugin { render_creation: WgpuSettings { backends: None, ..default() }.into(), ..default() })
            .disable::<WinitPlugin>(),
    )
    .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
    .add_plugins(LabViewPlugin);
    app
}

fn texts(app: &mut App) -> Vec<String> {
    app.world_mut().query::<&Text>().iter(app.world()).map(|t| t.0.clone()).collect()
}

#[test]
fn the_lab_builds_its_sheet_and_rebuilds_it_on_a_change() {
    let mut app = headless_lab();
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(app.world_mut().query::<&TimeColumn>().iter(app.world()).count(), 1);
    assert_eq!(app.world_mut().query::<&RulesColumn>().iter(app.world()).count(), 1);
    assert_eq!(app.world_mut().query::<&TabPane>().iter(app.world()).count(), 4);
    let shown = texts(&mut app);
    for want in ["Human Fighter", "Life", "Hunger", "Sleepiness", "Exhaustion", "Armour", "Damage", "Load", "15", "1\u{2013}8", "27", " / 150", "Sallet", "Mail Hauberk", "Messer", "+ Put on or hold an item", "Rules", "Fight", "Time and needs", "Eat, drink, equip", "Effects", "after effects land"] {
        assert!(shown.iter().any(|t| t == want), "{want} in {shown:?}");
    }
    assert!(shown.iter().all(|t| !t.contains(" = ")), "no formula: {shown:?}");
    // With no input the sheet is not built again.
    let builds = app.world().resource::<SheetBuilds>().0;
    assert!(builds >= 1);
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(app.world().resource::<SheetBuilds>().0, builds);
    // A belt of the Ox is put on: its card shows and capacity reads 175, the frame it changes.
    let ox = (1..2000).find(|&s| fb_items::roll_item("Belt", s).is_some_and(|i| i.affixes.iter().any(|a| a.name == "of the Ox"))).unwrap();
    app.world_mut().resource_mut::<LabState>().equip(Slot::Belt, "Belt", ox);
    app.update();
    app.update();
    assert_eq!(app.world().resource::<SheetBuilds>().0, builds + 1);
    let shown = texts(&mut app);
    assert!(shown.iter().any(|t| t == " / 175"), "{shown:?}");
    assert!(shown.iter().any(|t| t.contains("Belt of the Ox")), "{shown:?}");
}

#[test]
fn hovering_armour_lists_its_terms() {
    let mut app = headless_lab();
    for _ in 0..3 {
        app.update();
    }
    app.world_mut().resource_mut::<Hover>().pin(Some(0));
    app.update();
    app.update();
    let shown = texts(&mut app);
    for want in ["Armour 15", "Base", "10", "DEX", "+0", "Sallet", "+2", "Mail Hauberk", "+3"] {
        assert!(shown.iter().any(|t| t == want), "{want} in {shown:?}");
    }
}

#[test]
fn the_rules_column_lists_the_rules_and_marks_what_was_changed() {
    let mut app = headless_lab();
    for _ in 0..3 {
        app.update();
    }
    let shown = texts(&mut app);
    for want in ["Combat", "Giant Rat", "Needs", "Affixes", "Capacity per STR (lb)", "Attack bonus", "Save to the game", "Undo changes"] {
        assert!(shown.iter().any(|t| t == want), "{want} in {shown:?}");
    }
    assert!(!shown.iter().any(|t| t.ends_with(" changed")));
    forever_below::lab_rules::step(&mut app.world_mut().resource_mut::<LabState>().ruleset, "load/capacity_per_strength", 5);
    forever_below::lab_rules::step(&mut app.world_mut().resource_mut::<LabState>().ruleset, "creatures/Giant Rat/attack_bonus", -2);
    app.update();
    app.update();
    let shown = texts(&mut app);
    for want in ["2 changed", "was 4", "was 15", "was 150", " / 200"] {
        assert!(shown.iter().any(|t| t == want), "{want} in {shown:?}");
    }
    // Undo puts it back.
    let saved = app.world().resource::<forever_below::lab_view::LabRules>().saved.clone();
    app.world_mut().resource_mut::<LabState>().ruleset = saved;
    app.update();
    app.update();
    assert!(!texts(&mut app).iter().any(|t| t.ends_with(" changed") || t.starts_with("was ")));
}

fn real_data(name: &str) -> String {
    std::fs::read_to_string(format!("{}/../data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn save_writes_the_data_folder_it_is_given_and_not_the_real_one() {
    let (rules_before, affixes_before) = (real_data("rules.toml"), real_data("afx.json"));
    let dir = std::env::temp_dir().join(format!("fb_lab_app_save_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("rules.toml"), &rules_before).unwrap();
    std::fs::write(dir.join("afx.json"), &affixes_before).unwrap();
    let mut app = headless_lab();
    app.insert_resource(DataFolder(dir.clone()));
    for _ in 0..3 {
        app.update();
    }
    forever_below::lab_rules::step(&mut app.world_mut().resource_mut::<LabState>().ruleset, "load/capacity_per_strength", 5);
    app.world_mut().write_message(ControlPressed(Control::Save));
    app.update();
    app.update();
    assert!(std::fs::read_to_string(dir.join("rules.toml")).unwrap().contains("capacity_per_strength = 20"));
    assert_eq!(real_data("rules.toml"), rules_before);
    assert_eq!(real_data("afx.json"), affixes_before);
    let shown = texts(&mut app);
    assert!(shown.iter().any(|t| t == "Saved"), "{shown:?}");
    assert!(!shown.iter().any(|t| t.ends_with(" changed")), "{shown:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_save_that_fails_says_so_and_leaves_the_edits_unsaved() {
    let mut app = headless_lab();
    app.insert_resource(DataFolder("/nonexistent/fb_lab".into()));
    for _ in 0..3 {
        app.update();
    }
    forever_below::lab_rules::step(&mut app.world_mut().resource_mut::<LabState>().ruleset, "load/capacity_per_strength", 5);
    app.world_mut().write_message(ControlPressed(Control::Save));
    app.update();
    app.update();
    let shown = texts(&mut app);
    assert!(shown.iter().any(|t| t.starts_with("Couldn't save: ")), "{shown:?}");
    assert!(shown.iter().any(|t| t == "1 changed"), "{shown:?}");
}

#[test]
fn the_tabs_open_one_pane_at_a_time() {
    let mut app = headless_lab();
    for _ in 0..3 {
        app.update();
    }
    let open = |app: &mut App| app.world_mut().query::<(&TabPane, &Node)>().iter(app.world()).filter(|(_, n)| n.display != Display::None).map(|(p, _)| p.0).collect::<Vec<_>>();
    assert_eq!(open(&mut app), vec![0]);
    app.world_mut().resource_mut::<LabTab>().0 = 1;
    app.update();
    assert_eq!(open(&mut app), vec![1]);
}

fn press(app: &mut App, control: Control) {
    app.world_mut().write_message(ControlPressed(control));
    app.update();
    app.update();
}

/// The percentage a tile shows under `label`: the text after the label.
fn tile_percent(shown: &[String], label: &str) -> f64 {
    let at = shown.iter().position(|t| t == label).unwrap_or_else(|| panic!("no {label} in {shown:?}"));
    shown[at + 1].trim_end_matches('%').parse().unwrap()
}

#[test]
fn a_thousand_fights_fill_the_tiles_and_a_rule_edit_shows_what_it_was() {
    let mut app = headless_lab();
    for _ in 0..3 {
        app.update();
    }
    assert!(texts(&mut app).iter().any(|t| t.starts_with("Press 1,000 fights")));
    press(&mut app, Control::Thousand);
    let shown = texts(&mut app);
    for want in ["Giant Rat hits them", "Damage per hit", "Giant Rat blows to down them, in fights it wins", "They win", "Their life, round by round", "How many rounds a fight took"] {
        assert!(shown.iter().any(|t| t == want), "{want} in {shown:?}");
    }
    assert!((tile_percent(&shown, "Giant Rat hits them") - 50.0).abs() <= 2.0);
    assert!(!shown.iter().any(|t| t.starts_with("was ")), "nothing is edited: {shown:?}");
    // The rat's attack bonus goes from +4 to +2, and the lab runs the thousand again by itself.
    forever_below::lab_rules::step(&mut app.world_mut().resource_mut::<LabState>().ruleset, "creatures/Giant Rat/attack_bonus", -1);
    forever_below::lab_rules::step(&mut app.world_mut().resource_mut::<LabState>().ruleset, "creatures/Giant Rat/attack_bonus", -1);
    app.update();
    app.update();
    let shown = texts(&mut app);
    assert!((tile_percent(&shown, "Giant Rat hits them") - 40.0).abs() <= 2.0, "{shown:?}");
    assert!(shown.iter().any(|t| t == "was 50%"), "{shown:?}");
}

#[test]
fn one_fight_is_played_out_in_lines() {
    let mut app = headless_lab();
    for _ in 0..3 {
        app.update();
    }
    press(&mut app, Control::OneFight);
    let shown = texts(&mut app);
    assert!(shown.iter().any(|t| t.starts_with("Giant Rat bites: rolls ") && t.contains(" to hit (15 armour")), "{shown:?}");
    assert!(shown.iter().any(|t| t.contains(" wins in ") || t.contains("They win in ")), "{shown:?}");
    assert!(shown.iter().all(|t| !t.contains(" = ")), "{shown:?}");
}

#[test]
fn passing_time_shows_answers_and_the_chart_with_what_each_was() {
    use forever_below::lab_view::TimeAction;
    let mut app = headless_lab();
    for _ in 0..3 {
        app.update();
    }
    assert!(texts(&mut app).iter().any(|t| t.starts_with("Press a span")));
    press(&mut app, Control::Time(TimeAction::Pass(24.0)));
    let shown = texts(&mut app);
    for want in ["Hungry after", "8 h", "Parched after", "5 h", "Weary after", "Exhaustion 1 on", "not reached", "Needs and exhaustion over time"] {
        assert!(shown.iter().any(|t| t == want), "{want} in {shown:?}");
    }
    assert!(!shown.iter().any(|t| t.starts_with("was ")), "{shown:?}");
    // Hours walking to Starving 16 to 20: Hungry moves from 8 h to 10 h.
    for _ in 0..4 {
        forever_below::lab_rules::step(&mut app.world_mut().resource_mut::<LabState>().ruleset, "needs/hunger_hours_to_full", 1);
    }
    app.update();
    app.update();
    let shown = texts(&mut app);
    assert!(shown.iter().any(|t| t == "10 h") && shown.iter().any(|t| t == "was 8 h"), "{shown:?}");
    // Eat and Drink add to the run, and Reset starts it over.
    press(&mut app, Control::Time(TimeAction::Eat("Bread")));
    assert_eq!(app.world().resource::<forever_below::lab_view::TimeRun>().steps.len(), 2);
    press(&mut app, Control::Time(TimeAction::Reset));
    assert!(texts(&mut app).iter().any(|t| t.starts_with("Press a span")));
}

#[test]
fn the_equip_tab_feeds_and_dresses_the_character() {
    use forever_below::lab_view::{Picker, TimeAction};
    let mut app = headless_lab();
    for _ in 0..3 {
        app.update();
    }
    let shown = texts(&mut app);
    for want in ["Provisions", "Bread", "nutrition 40% \u{b7} water 0%", "Tankard", "nutrition 0% \u{b7} water 50%", "Put on or hold something", "Rarity", "Worn and held", "Take off"] {
        assert!(shown.iter().any(|t| t == want), "{want} in {shown:?}");
    }
    // Eating a roast haunch is a step of the run, and the run moves the sheet's bars.
    press(&mut app, Control::Time(TimeAction::Pass(8.0)));
    let word = |app: &App| {
        let state = app.world().resource::<LabState>();
        app.world().resource::<forever_below::lab_time::TimeLab>().needs.hunger.word_with(&state.ruleset.rules).to_string()
    };
    assert_eq!(word(&app), "Hungry");
    press(&mut app, Control::Time(TimeAction::Eat("Roast haunch")));
    assert_eq!(word(&app), "Fed");
    // The sheet's own bar shows the word.
    let shown = texts(&mut app);
    assert!(shown.iter().any(|t| t == "Fed"), "{shown:?}");
    // A moth-eaten cloak (Lice-ridden is held back until disease is simulated): pick the cloak slot, its base and the seed, and the preview shows its affix.
    let seed = (1..4000).find(|&s| fb_items::roll_item("Cloak", s).is_some_and(|i| i.affixes.iter().any(|a| a.name == "Moth-eaten"))).unwrap();
    let base = forever_below::lab_view::bases_for(Slot::Cloak).iter().position(|b| b == "Cloak").unwrap();
    press(&mut app, Control::PickSlotAt(3));
    press(&mut app, Control::PickBaseAt(base));
    press(&mut app, Control::PickSeed(seed as i64 - 1));
    assert_eq!(app.world().resource::<Picker>().seed, seed);
    let shown = texts(&mut app);
    assert!(shown.iter().any(|t| t.starts_with("Moth-eaten: ")), "the preview card: {shown:?}");
    press(&mut app, Control::PutOn);
    assert!(app.world().resource::<LabState>().slot(Slot::Cloak).base.is_some());
    // The mail comes off: armour 15 to 12, load 27 to 7 or so.
    press(&mut app, Control::Remove(Slot::Body));
    let shown = texts(&mut app);
    assert!(shown.iter().any(|t| t == "12"), "{shown:?}");
}

#[test]
fn a_search_with_no_hit_says_so_in_the_equip_tab_and_the_quick_card_has_the_rarity_row() {
    use forever_below::lab_view::{Picker, searches_run};
    let mut app = headless_lab();
    for _ in 0..3 {
        app.update();
    }
    // One seed is tried, which is not a legendary: nothing is found.
    app.world_mut().resource_mut::<Picker>().search = 1;
    press(&mut app, Control::PickRarity(Some(fb_items::Rarity::Legendary)));
    let shown = texts(&mut app);
    assert!(shown.iter().any(|t| t == "No legendary rolls from seed 1 within 1 seeds."), "{shown:?}");
    // Rebuilding the tab again does not search again.
    let searched = searches_run();
    press(&mut app, Control::Tab(2));
    press(&mut app, Control::OneFight);
    assert_eq!(searches_run(), searched);
    // The sheet's quick card opens with its rarity row and says the same.
    press(&mut app, Control::OpenPicker);
    let shown = texts(&mut app);
    assert!(shown.iter().filter(|t| *t == "Legendary").count() >= 2, "a Legendary choice in both pickers: {shown:?}");
    assert!(shown.iter().any(|t| t == "No legendary within 1 seeds."), "{shown:?}");
}
