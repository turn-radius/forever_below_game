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
use forever_below::lab_strike::Strikes;
use forever_below::lab_time::TimeLab;
use forever_below::lab_view::{Hover, LabState, LabTab, LabViewPlugin, RulesColumn, SheetBuilds, StrikeColumn, TabPane, TimeColumn};

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
    assert_eq!(app.world_mut().query::<&StrikeColumn>().iter(app.world()).count(), 1);
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

#[test]
fn strikes_show_in_the_strike_column() {
    let mut app = headless_lab();
    for _ in 0..3 {
        app.update();
    }
    assert!(texts(&mut app).iter().any(|t| t.starts_with("Hit points 10 of 10")), "{:?}", texts(&mut app));
    let state = app.world().resource::<LabState>().clone();
    app.world_mut().resource_mut::<Strikes>().strike(&state);
    app.world_mut().resource_mut::<Strikes>().strike_hundred(&state);
    app.update();
    let shown = texts(&mut app);
    assert!(shown.iter().any(|t| t.starts_with("Giant Rat: d20 (") && t.contains("vs AC 15")), "{shown:?}");
    assert!(shown.iter().any(|t| t.starts_with("100 blows vs AC 15")), "{shown:?}");
}

#[test]
fn passing_time_shows_in_the_time_column() {
    let mut app = headless_lab();
    for _ in 0..3 {
        app.update();
    }
    assert!(texts(&mut app).iter().any(|t| t == "Day 0 00:00 · walking"), "{:?}", texts(&mut app));
    let state = app.world().resource::<LabState>().clone();
    app.world_mut().resource_mut::<TimeLab>().pass(&state, 8.0);
    app.update();
    app.update();
    let shown = texts(&mut app);
    assert!(shown.iter().any(|t| t == "Day 0 08:00 · walking"), "{shown:?}");
    assert!(shown.iter().any(|t| t == "Hunger 50% · Hungry"), "{shown:?}");
    assert!(shown.iter().any(|t| t.contains("Day 0 08:00 Hunger → Hungry")), "{shown:?}");
    assert!(shown.iter().any(|t| t == "Exhaustion 0 of 6: full speed"), "{shown:?}");
}
