//! Picking a lock in play (#833): holding R at a locked claim door shows the dice.
//!
//! The whole game runs headless (as in `travel.rs`) and the key goes in as a `KeyboardInput`
//! message, pressed once and held across frames, as the window's events do. The delver carries
//! the picks in their pack, stands outside the door and looks at it.

use bevy::{
    input::{ButtonState, keyboard::{Key, KeyboardInput}},
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    time::TimeUpdateStrategy,
    window::ExitCondition,
    winit::WinitPlugin,
};
use forever_below::{
    dice::Dice,
    doors::{AimedDoor, Doors, PICK_SECONDS},
    inventory::Inventory,
    launch::AppState,
    menu::MenuCommand,
    play::{PlayPlugin, Player},
    player::Locomotion,
    record::WorldRecord,
    render::ArmoryRenderPlugin,
    settings::SettingsPlugin,
    title::{NewDelveSeed, TitlePlugin, creator::CreatorCommand},
    tuning::TuningPlugin,
};

mod common;
use common::run;

fn headless_game() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(RenderPlugin { render_creation: WgpuSettings { backends: None, ..default() }.into(), ..default() })
            .set(AssetPlugin { file_path: forever_below::tuning::asset_file_path(), ..default() })
            .disable::<WinitPlugin>(),
    )
    .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
    .insert_state(AppState::Title)
    .add_plugins((SettingsPlugin, TuningPlugin, ArmoryRenderPlugin, PlayPlugin, TitlePlugin))
    .insert_resource(NewDelveSeed(Some(1207)));
    app
}

fn key(app: &mut App, state: ButtonState) {
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::KeyR,
        logical_key: Key::Character("r".into()),
        state,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

/// Turns the delver to `yaw`.
fn face(app: &mut App, body: Entity, yaw: f32) {
    let mut at = app.world_mut().entity_mut(body);
    at.get_mut::<Transform>().unwrap().rotation = Quat::from_rotation_y(yaw);
    at.get_mut::<Locomotion>().unwrap().yaw = yaw;
}

/// A delve begun from the title, the delver outside a shut, locked claim door of a faction at
/// odds with them, looking at it, with lockpicks in their pack and no folk about. Returns the
/// game, the delver, the yaw that faces the door and the door's cell.
fn at_a_locked_door() -> (App, Entity, f32, (i32, i32)) {
    let mut app = headless_game();
    run(&mut app, 30);
    app.world_mut().write_message(MenuCommand::Creator(CreatorCommand::Begin));
    run(&mut app, 90);
    assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Delving);

    // The first claim door that can be picked, its faction wary so it is locked.
    let (cell, door) = app.world().resource::<Doors>().0.iter().find(|(_, d)| d.switched() && format!("{:?}", d.kind) != "Portcullis").map(|(c, d)| (*c, d.clone())).expect("a claim door");
    {
        let mut record = app.world_mut().resource_mut::<WorldRecord>();
        let now = record.standings.get(door.faction).value();
        record.standings.shift(door.faction, -30 - now);
    }
    // Nobody about to open it, hear the picks or come for the delver.
    let folk: Vec<Entity> = app.world_mut().query_filtered::<Entity, With<forever_below::folk::Denizen>>().iter(app.world()).collect();
    for entity in folk {
        app.world_mut().despawn(entity);
    }
    {
        // Shut, as it stands when no folk is about.
        let mut doors = app.world_mut().resource_mut::<Doors>();
        let door = doors.0.get_mut(&cell).unwrap();
        door.open = false;
        door.folk_held = false;
        door.swing = 0.0;
    }
    // Lockpicks into the pack, wherever it finds room.
    {
        let mut inventory = app.world_mut().resource_mut::<Inventory>();
        let picks = inventory.add(fb_items::roll_item("Lockpicks", 3).unwrap());
        inventory.pick_up(picks).expect("the picks fit somewhere");
        assert!(forever_below::doors::carries_lockpicks(&inventory));
    }
    // Outside the door, a cell back (a body nearer is in the doorway's blocked cell), looking at it.
    let feet = door.centre - door.inward * 14.0;
    let dir = door.centre - feet;
    let yaw = (-dir.x).atan2(-dir.y);
    let body = app.world_mut().query_filtered::<Entity, With<Player>>().single(app.world()).unwrap();
    {
        let mut at = app.world_mut().entity_mut(body);
        at.get_mut::<Transform>().unwrap().translation = Vec3::new(feet.x, 0.0, feet.y);
        let mut l = at.get_mut::<Locomotion>().unwrap();
        l.pitch = 0.0;
        l.velocity = Vec2::ZERO;
    }
    face(&mut app, body, yaw);
    run(&mut app, 10);
    assert_eq!(app.world().resource::<AimedDoor>().0, Some(cell), "the door is under the crosshair");
    (app, body, yaw, cell)
}

/// Holds R from now, `wobble` frames (start, length) looking away from the door, until the
/// dice show; the frame they did, if they did within four seconds.
fn hold_until_the_dice(app: &mut App, body: Entity, yaw: f32, wobble: Option<(usize, usize)>) -> Option<usize> {
    key(app, ButtonState::Pressed);
    for frame in 0..240 {
        if let Some((start, length)) = wobble {
            if frame == start {
                face(app, body, yaw + 2.0);
            } else if frame == start + length {
                face(app, body, yaw);
            }
        }
        run(app, 1);
        if app.world().resource::<Dice>().busy() {
            return Some(frame);
        }
    }
    None
}

#[test]
fn holding_r_at_a_locked_door_shows_the_dice() {
    let (mut app, body, yaw, _) = at_a_locked_door();
    let frame = hold_until_the_dice(&mut app, body, yaw, None).expect("the dice show");
    assert!(frame as f32 / 60.0 >= PICK_SECONDS - 0.1, "not before the hold ends: frame {frame}");
    // The panel itself is up: the roll's name and its "d20 against N" line.
    run(&mut app, 20);
    let panel = {
        let world = app.world_mut();
        let mut texts = world.query::<&Text>();
        texts.iter(world).filter(|t| t.0.starts_with("d20 against")).count()
    };
    assert_eq!(panel, 1, "the dice panel is on screen");
    key(&mut app, ButtonState::Released);
}

/// The crosshair slipping off the door for a moment (a nudge of the mouse, a head turned) does
/// not end the pick: the key is still down, and the dice show once it has been held on the door.
#[test]
fn a_slip_of_the_aim_does_not_stop_the_pick() {
    let (mut app, body, yaw, _) = at_a_locked_door();
    let frame = hold_until_the_dice(&mut app, body, yaw, Some((40, 3)));
    assert!(frame.is_some(), "the dice show after the aim slips off the door and back");
}
