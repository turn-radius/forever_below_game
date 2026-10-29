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
fn at_a_locked_door(back: f32) -> (App, Entity, f32, (i32, i32)) {
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
    let body = app.world_mut().query_filtered::<Entity, With<Player>>().single(app.world()).unwrap();
    let yaw = stand_back(&mut app, body, cell, back);
    assert_eq!(app.world().resource::<AimedDoor>().0, Some(cell), "the door is under the crosshair");
    (app, body, yaw, cell)
}

/// Stands the delver `back` units outside the door at `cell`, looking at it; the yaw that does.
fn stand_back(app: &mut App, body: Entity, cell: (i32, i32), back: f32) -> f32 {
    let door = app.world().resource::<Doors>().0[&cell].clone();
    let feet = door.centre - door.inward * back;
    let dir = door.centre - feet;
    let yaw = (-dir.x).atan2(-dir.y);
    {
        let mut at = app.world_mut().entity_mut(body);
        at.get_mut::<Transform>().unwrap().translation = Vec3::new(feet.x, 0.0, feet.y);
        let mut l = at.get_mut::<Locomotion>().unwrap();
        l.pitch = 0.0;
        l.velocity = Vec2::ZERO;
    }
    face(app, body, yaw);
    run(app, 10);
    yaw
}

/// Holds R from now, with the delver turned away from the door on each frame `away` says, until
/// the dice show; the frame they did, if they did within four seconds. `jitter` is the
/// degrees of yaw and pitch the look wanders by (a hand on the mouse, the head bobbing).
fn hold_until_the_dice(app: &mut App, body: Entity, yaw: f32, away: impl Fn(usize) -> bool, jitter: f32) -> Option<usize> {
    key(app, ButtonState::Pressed);
    for frame in 0..240 {
        let wander = jitter.to_radians() * (frame as f32 * 1.7).sin();
        face(app, body, if away(frame) { yaw + 2.0 } else { yaw + wander });
        app.world_mut().entity_mut(body).get_mut::<Locomotion>().unwrap().pitch = jitter.to_radians() * (frame as f32 * 0.9).cos();
        run(app, 1);
        if app.world().resource::<Dice>().busy() {
            return Some(frame);
        }
    }
    None
}

#[test]
fn holding_r_at_a_locked_door_shows_the_dice() {
    let (mut app, body, yaw, _) = at_a_locked_door(14.0);
    let frame = hold_until_the_dice(&mut app, body, yaw, |_| false, 0.0).expect("the dice show");
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

/// The crosshair slipping off the door (a nudge of the mouse) for a frame in every four does not
/// stop the pick: R is still down and the time held carries over, so the dice show in about the
/// hold's 1.5 s, not never.
#[test]
fn an_aim_that_drops_a_frame_in_four_still_picks() {
    let (mut app, body, yaw, _) = at_a_locked_door(14.0);
    let frame = hold_until_the_dice(&mut app, body, yaw, |f| f % 4 == 3, 0.0).expect("the dice show");
    assert!(frame as f32 / 60.0 < 2.1, "the dice showed at frame {frame}");
}

/// An aim gone past the grace starts the hold over.
#[test]
fn an_aim_gone_for_good_starts_the_hold_over() {
    let (mut app, body, yaw, _) = at_a_locked_door(14.0);
    let frame = hold_until_the_dice(&mut app, body, yaw, |f| (30..60).contains(&f), 0.0).expect("the dice show");
    assert!(frame >= 60 + 85, "the hold started over once the aim was back: frame {frame}");
}

/// The delver picks from each distance a body can stand at (a shut door's cell is a wall, so it
/// halts about 11.5 units off the door's middle) to the edge of the reach, the look wandering a
/// little and the head bobbing.
#[test]
fn it_picks_from_every_distance_the_prompt_shows() {
    let (mut app, body, _, cell) = at_a_locked_door(19.0);
    for distance in [2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 13.5, 14.0, 16.0, 18.0, 19.0] {
        let yaw = stand_back(&mut app, body, cell, distance);
        let rest = app.world().get::<Transform>(body).unwrap().translation;
        let rest = Vec2::new(rest.x, rest.z).distance(app.world().resource::<Doors>().0[&cell].centre);
        let aimed = app.world().resource::<AimedDoor>().0 == Some(cell);
        let frame = hold_until_the_dice(&mut app, body, yaw, |_| false, 0.7);
        eprintln!("stood {distance} units off, at rest {rest:.1}, aimed {aimed}: dice at frame {frame:?}");
        assert!(frame.is_some_and(|f| f < 150) || !aimed || !(13.4..19.0).contains(&rest), "{distance} units off: {frame:?}");
        // Let the roll close, and shut and lock the door again for the next stand.
        key(&mut app, ButtonState::Released);
        for _ in 0..900 {
            if !app.world().resource::<Dice>().busy() {
                break;
            }
            run(&mut app, 1);
        }
        run(&mut app, 5);
        let mut doors = app.world_mut().resource_mut::<Doors>();
        let door = doors.0.get_mut(&cell).unwrap();
        door.unlocked = false;
        door.open = false;
        door.swing = 0.0;
    }
}

fn walk_key(app: &mut App, state: ButtonState) {
    app.world_mut().write_message(KeyboardInput { key_code: KeyCode::KeyW, logical_key: Key::Character("w".into()), state, text: None, repeat: false, window: Entity::PLACEHOLDER });
}

/// Walking up to the door with W held, as a player does, the delver halts at the shut door and
/// picks from where they stop.
#[test]
fn walking_up_to_the_door_and_picking() {
    let (mut app, body, _, cell) = at_a_locked_door(19.0);
    let yaw = stand_back(&mut app, body, cell, 60.0);
    walk_key(&mut app, ButtonState::Pressed);
    run(&mut app, 240);
    walk_key(&mut app, ButtonState::Released);
    run(&mut app, 30);
    let rest = app.world().get::<Transform>(body).unwrap().translation;
    let rest = Vec2::new(rest.x, rest.z).distance(app.world().resource::<Doors>().0[&cell].centre);
    eprintln!("walked up: at rest {rest:.1} units off, aimed {:?}", app.world().resource::<AimedDoor>().0);
    let frame = hold_until_the_dice(&mut app, body, yaw, |_| false, 0.7);
    eprintln!("walked up: dice at frame {frame:?}");
    assert!(frame.is_some_and(|f| f < 150), "{frame:?}");
}
