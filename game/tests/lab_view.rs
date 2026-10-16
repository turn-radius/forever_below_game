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
use forever_below::lab_view::{LabState, LabViewPlugin, StrikeColumn, TimeColumn};

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
    assert!(texts(&mut app).iter().any(|t| t.starts_with("Capacity 150 lb")), "{:?}", texts(&mut app));
    app.world_mut().resource_mut::<LabState>().equip(Slot::Belt, "Belt", 2);
    app.world_mut().resource_mut::<LabState>().character.attributes.strength = 14;
    app.update();
    app.update();
    assert!(texts(&mut app).iter().any(|t| t.starts_with("Capacity 210 lb") || t.starts_with("Capacity 2")), "{:?}", texts(&mut app));
}
