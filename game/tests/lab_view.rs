//! The lab builds as an app without a window or a GPU (as `delve_scope` runs the game): its
//! plugin and the shared tweak panel have no clashing queries, and the sheet and the panel
//! are built, and rebuilt when the state changes.

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
use forever_below::lab_view::{LabState, LabViewPlugin, SheetBuilds, StrikeColumn, TimeColumn};

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
    assert!(texts(&mut app).iter().any(|t| t == "Capacity 150 lb"), "{:?}", texts(&mut app));
    // With no input the sheet is not built again.
    let builds = app.world().resource::<SheetBuilds>().0;
    assert!(builds >= 1);
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(app.world().resource::<SheetBuilds>().0, builds);
    // A change builds it once more, the same frame.
    app.world_mut().resource_mut::<LabState>().equip(Slot::Belt, "Belt", 2);
    app.world_mut().resource_mut::<LabState>().character.attributes.strength = 14;
    app.update();
    app.update();
    assert_eq!(app.world().resource::<SheetBuilds>().0, builds + 1);
    assert!(texts(&mut app).iter().any(|t| t == "Capacity 210 lb"), "{:?}", texts(&mut app));
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
