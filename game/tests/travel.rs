//! Taking the stairs between levels (#201): the delver goes down to the level below and back up,
//! and what is theirs goes with them while what belongs to a level stays with it.
//!
//! The whole game runs here without a window or a GPU (as in `delve_scope.rs`), stepped a 60th
//! of a second a frame, from the title through New delve into level 1207.

use std::collections::BTreeMap;

use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    input::{ButtonState, keyboard::{Key, KeyboardInput}},
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    time::TimeUpdateStrategy,
    window::ExitCondition,
    winit::WinitPlugin,
};
use fb_level::{Level, StairKind, depth_seed};
use forever_below::{
    carry::FloorItem,
    combat::{Health, vitals::{FALL_TIME, LAST_GASP}},
    inventory::{Inventory, ItemId},
    launch::AppState,
    folk::FolkFigures,
    level::{DoorSwing, EntranceDoor, LevelPiece, PreparedStone, StonePiece, stone_chunks},
    staged::Staged,
    menu::MenuCommand,
    play::{BlockedCells, CurrentLevel, PlayPlugin, Player, PlayerCamera, Screen, ScreenOpen},
    player::Locomotion,
    props::PropPiece,
    render::ArmoryRenderPlugin,
    settings::SettingsPlugin,
    title::{NewDelveSeed, TitlePlugin, creator::CreatorCommand},
    loading::{LoadingLevel, LoadingScreen},
    travel::{Abyss, Arrival, Below, DepthTitle, Journey, LeavingLevel, Phase, UseStair},
    tuning::TuningPlugin,
};

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

mod common;
use common::run;

fn begin_delve(app: &mut App) {
    run(app, 30);
    app.world_mut().write_message(MenuCommand::Creator(CreatorCommand::Begin));
    run(app, 90);
    assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Delving);
}

fn level(app: &App) -> &Level {
    &app.world().resource::<CurrentLevel>().0
}

fn player(app: &mut App) -> Entity {
    let world = app.world_mut();
    world.query_filtered::<Entity, With<Player>>().single(world).expect("one delver")
}

/// Stands the delver in the middle of cell `(x, y)` turned to `yaw`.
fn stand(app: &mut App, (x, y): (u32, u32), yaw: f32) {
    let body = player(app);
    let [wx, _, wz] = Level::world(x as f32 + 0.5, y as f32 + 0.5);
    let mut at = app.world_mut().entity_mut(body);
    // (The looking system needs a window to run, which this app has none of, so the body is
    // turned as well.)
    let mut transform = at.get_mut::<Transform>().unwrap();
    transform.translation = Vec3::new(wx, 0.0, wz);
    transform.rotation = Quat::from_rotation_y(yaw);
    let mut locomotion = at.get_mut::<Locomotion>().unwrap();
    locomotion.yaw = yaw;
    locomotion.velocity = Vec2::ZERO;
}

/// Presses and releases F, as typing into the window does.
fn press_f(app: &mut App) {
    press(app, KeyCode::KeyF, "f");
}

fn press(app: &mut App, key_code: KeyCode, text: &str) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: Key::Character(text.into()),
            state,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
        app.update();
    }
}

/// What belongs to the delver: their hit points, what they wear and everything they carry.
#[derive(Debug, PartialEq)]
struct Delver {
    hp: Health,
    worn: Vec<String>,
    carried: Vec<ItemId>,
}

fn delver(app: &mut App) -> Delver {
    let body = player(app);
    let hp = *app.world().get::<Health>(body).expect("the delver has hit points");
    let inventory = app.world().resource::<Inventory>();
    let mut worn: Vec<String> = inventory.equipped().iter().map(|(slot, item)| format!("{slot:?} {}", item.title())).collect();
    worn.sort();
    let mut carried: Vec<_> = inventory.ids().filter(|&id| inventory.is_carried(id)).collect();
    carried.sort();
    Delver { hp, worn, carried }
}

/// Every piece of the level by name, how many of each, besides sounds, which come and go.
fn census(app: &mut App) -> BTreeMap<String, usize> {
    let world = app.world_mut();
    let mut counts = BTreeMap::new();
    let mut query = world.query_filtered::<(Option<&Name>, Has<AudioPlayer>), (With<LevelPiece>, Allow<Disabled>)>();
    for (name, audio) in query.iter(world) {
        if !audio {
            *counts.entry(name.map_or("unnamed".to_string(), |n| n.to_string())).or_default() += 1;
        }
    }
    counts
}

fn count<T: Component>(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query_filtered::<(), With<T>>().iter(world).count()
}

fn named(app: &mut App, name: &str) -> usize {
    let world = app.world_mut();
    world.query::<&Name>().iter(world).filter(|n| n.as_str() == name).count()
}

/// How many of the level's stone pieces are named `name`: a book's or a note's "ink" part is no
/// stone.
fn stone_named(app: &mut App, name: &str) -> usize {
    let world = app.world_mut();
    world.query_filtered::<&Name, With<StonePiece>>().iter(world).filter(|n| n.as_str() == name).count()
}

fn assert_at(app: &mut App, (x, y): (u32, u32), facing: fb_level::Facing, what: &str) {
    let body = player(app);
    let at = app.world().get::<Transform>(body).unwrap();
    let [wx, _, wz] = Level::world(x as f32 + 0.5, y as f32 + 0.5);
    assert!((at.translation.x - wx).abs() < 0.01 && (at.translation.z - wz).abs() < 0.01 && at.translation.y == 0.0, "{what}: arrived at {}", at.translation);
    let looking = at.forward().as_vec3();
    let (dx, dy) = facing.step();
    assert!((looking.x - dx as f32).abs() < 0.01 && (looking.z - dy as f32).abs() < 0.01, "{what}: looks {looking}, not {facing:?}");
}

#[test]
fn the_stairs_carry_the_delver_down_and_back_up_and_leave_each_level_whole() {
    let mut app = headless_game();
    begin_delve(&mut app);
    run(&mut app, 60);

    // Level 1 of run 1207 has a way down and, being the first, no way up.
    let first = level(&app).clone();
    assert_eq!((first.seed, first.depth), (1207, 1));
    assert_eq!(*app.world().resource::<Abyss>(), Abyss { run_seed: 1207, depth: 1, arrival: Arrival::Entered });
    let down = *first.stair(StairKind::Down).expect("a way down");
    assert!(first.stair(StairKind::Up).is_none());
    assert_eq!((named(&mut app, "stair down"), named(&mut app, "stair up")), (1, 0));
    assert!(app.world().resource::<BlockedCells>().0.contains(&(down.x as i32, down.y as i32)), "the stair cannot be walked onto");

    // Hurt a little, so hit points are seen to carry over.
    let body = player(&mut app);
    app.world_mut().get_mut::<Health>(body).unwrap().hp -= 3;
    let before = delver(&mut app);
    assert!(!before.worn.is_empty() && !before.carried.is_empty(), "the delver starts with gear: {before:?}");

    // Walk to the way down (standing before it, looking at it) and press F.
    stand(&mut app, down.approach(), down.facing.yaw());
    run(&mut app, 90);
    let level_one = census(&mut app);
    assert!(level_one.contains_key("stair down"), "{level_one:?}");
    press_f(&mut app);
    run(&mut app, 3);
    assert!(app.world().get_resource::<Journey>().is_some(), "the screen is fading");
    assert_eq!(app.world().resource::<Abyss>().depth, 1, "still on the first level while it fades out");
    run(&mut app, 50);
    assert_eq!(count::<DepthTitle>(&mut app), 1, "the depth shows on arrival");
    run(&mut app, 60);
    assert!(app.world().get_resource::<Journey>().is_none(), "the journey is over");

    // Level 2: its own seed, standing at its way up, everything of the delver's with them.
    let second = level(&app).clone();
    assert_eq!((second.depth, second.seed), (2, depth_seed(1207, 2)));
    assert_eq!(*app.world().resource::<Abyss>(), Abyss { run_seed: 1207, depth: 2, arrival: Arrival::FromAbove });
    let up = *second.stair(StairKind::Up).expect("a way up");
    let (sx, sy, facing) = second.start();
    assert_eq!((sx, sy), up.approach());
    assert_at(&mut app, (sx, sy), facing, "level 2");
    assert_eq!(delver(&mut app), before, "the delver, their gear and their hit points are as they were");
    assert!(app.world().resource::<BlockedCells>().0.contains(&(up.x as i32, up.y as i32)));
    assert_eq!((named(&mut app, "stair down"), named(&mut app, "stair up"), named(&mut app, "entrance door")), (1, 1, 0), "this level's stairs and none of the first's");
    // Nothing of level 1 is left standing in level 2.
    let world = app.world_mut();
    let items: Vec<_> = world.query::<&FloorItem>().iter(world).map(|f| f.spot).collect();
    assert!(items.iter().all(|spot| second.items.contains(spot)), "an item of another level lies here");
    let mut props: Vec<_> = world.query::<&PropPiece>().iter(world).map(|p| (p.index, p.spot)).collect();
    props.sort_by_key(|p| p.0);
    assert_eq!(props.into_iter().map(|p| p.1).collect::<Vec<_>>(), second.props, "the props are this level's");
    assert_eq!(count::<Player>(&mut app), 1);
    assert_eq!(count::<PlayerCamera>(&mut app), 1);

    // And back up: the way up, then the first level again beside its way down.
    stand(&mut app, up.approach(), up.facing.yaw());
    run(&mut app, 30);
    press_f(&mut app);
    run(&mut app, 3);
    assert!(app.world().get_resource::<Journey>().is_some());
    run(&mut app, 200);
    assert!(app.world().get_resource::<Journey>().is_none());
    assert_eq!(*level(&app), first, "a depth is always the same level");
    assert_eq!(*app.world().resource::<Abyss>(), Abyss { run_seed: 1207, depth: 1, arrival: Arrival::FromBelow });
    let (ax, ay, away) = down.arrival();
    assert_at(&mut app, (ax, ay), away, "level 1");
    assert_eq!(delver(&mut app), before);
    // The door shut behind the delver on the way in and does not swing again.
    let world = app.world_mut();
    let doors: Vec<_> = world.query::<&EntranceDoor>().iter(world).map(|d| (d.swing, d.cell)).collect();
    assert_eq!(doors, vec![(DoorSwing::Shut, (first.entrance.x as i32, first.entrance.y as i32))]);
    assert!(app.world().resource::<BlockedCells>().0.contains(&(first.entrance.x as i32, first.entrance.y as i32)));
    // Level 1 stands as it did, with nothing from level 2 in it.
    assert_eq!(census(&mut app), level_one, "the first level is laid out as before");
}

/// The level below is made in the background soon after a level is entered. Going down soon
/// after, before it can have finished, waits for it and lands on the same level and stone as
/// one made in place.
#[test]
fn going_down_at_once_waits_for_the_level_made_in_the_background() {
    let mut app = headless_game();
    begin_delve(&mut app);
    // (A few frames in, as the one below is started a few frames after an arrival.)
    run(&mut app, 20);
    let first = level(&app).clone();
    let down = *first.stair(StairKind::Down).expect("a way down");
    assert_eq!(app.world().get_resource::<Below>().map(Below::key), Some((1207, 2)), "the level below is being made");
    let built_here = app.world().resource::<FolkFigures>().built_here;
    // (The first level's stone came from the background too.)
    let stone_used = app.world().resource::<PreparedStone>().used;
    stand(&mut app, down.approach(), down.facing.yaw());
    run(&mut app, 2);
    press_f(&mut app);
    // (The fades take 72 frames of the game's fixed time step, however long the work takes.)
    run(&mut app, 90);
    assert!(app.world().get_resource::<Journey>().is_none(), "the journey is over");
    assert_eq!(app.world().get_resource::<Below>().map(Below::key), Some((1207, 3)), "the one below this is started in turn");

    // Everything laid out came from the background: no folk figure was built here, and the
    // stone, creatures, items, props, fixtures, banners and crystals were each taken once.
    assert_eq!(app.world().resource::<FolkFigures>().built_here, built_here);
    assert_eq!(app.world().resource::<PreparedStone>().used, stone_used + 1);
    assert_eq!(app.world().resource::<Staged>().taken, 6);

    let second = level(&app).clone();
    assert_eq!(second, forever_below::travel::Abyss::new(1207).level_at(2), "the level made in the background is the one made in place");
    // Its stone is laid out whole, from the background's chunks, which are used up.
    let chunks = stone_chunks(&second);
    let expected = |name: &str| chunks.iter().map(|c| match name {
        "ink" => usize::from(!c.ink.indices.is_empty()),
        "joints" => usize::from(!c.joints.indices.is_empty()),
        _ => c.surfaces.parts.iter().filter(|p| p.name == name).count(),
    }).sum::<usize>();
    for name in ["floor", "ink", "joints"] {
        assert_eq!(stone_named(&mut app, name), expected(name), "{name}");
    }
    assert!(!app.world().resource::<PreparedStone>().is_kept());
}

/// The frames go on while the next level is made: the journey waits at black in its own phase
/// over many frames, the loading screen is drawn once the wait is long enough, and the screen
/// fades away over the level once it is laid out.
#[test]
fn the_loading_screen_stands_while_the_level_is_made_and_the_frames_go_on() {
    let mut app = headless_game();
    begin_delve(&mut app);
    run(&mut app, 60);
    let down = *level(&app).stair(StairKind::Down).expect("a way down");
    stand(&mut app, down.approach(), down.facing.yaw());
    run(&mut app, 2);
    press_f(&mut app);
    let started = std::time::Instant::now();
    let (mut waiting, mut screens_seen, mut held) = (0, 0, false);
    while app.world().get_resource::<Journey>().is_none_or(|j| matches!(j.phase, Phase::FadingOut | Phase::Leaving | Phase::Loading)) {
        assert!(started.elapsed().as_secs() < 120, "the level was not made in two minutes");
        app.update();
        if app.world().get_resource::<Journey>().is_some_and(|j| j.phase == Phase::Loading) {
            if !held {
                // However quickly the level is made, the wait lasts long enough to be seen.
                app.world_mut().resource_mut::<LoadingLevel>().hold_for(0.6);
                held = true;
            }
            waiting += 1;
            screens_seen = screens_seen.max(count::<LoadingScreen>(&mut app));
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    assert!(held, "the journey waited for the level");
    assert!(waiting >= 30, "the main thread went on drawing frames while it waited: {waiting}");
    assert_eq!(screens_seen, 1, "the loading screen was drawn once");
    // Laid out and faded in, the screen goes with the black.
    let depth = app.world().resource::<Abyss>().depth;
    assert_eq!(depth, 2);
    run(&mut app, 120);
    assert!(app.world().get_resource::<Journey>().is_none());
    assert!(app.world().get_resource::<LoadingLevel>().is_none());
    assert_eq!(count::<LoadingScreen>(&mut app), 0, "the screen has faded away");
}

/// Starts a journey down and steps to its loading, holding the load up so the wait is seen.
fn into_the_loading(app: &mut App) {
    let down = *level(app).stair(StairKind::Down).expect("a way down");
    stand(app, down.approach(), down.facing.yaw());
    run(app, 2);
    press_f(app);
    let started = std::time::Instant::now();
    while !app.world().get_resource::<Journey>().is_some_and(|j| j.phase == Phase::Loading) {
        assert!(started.elapsed().as_secs() < 120, "the journey did not reach its loading");
        app.update();
    }
    app.world_mut().resource_mut::<LoadingLevel>().hold_for(1.5);
}

/// The world holds still while the screen is black: no hit points lost, no need grown, and the
/// world's clock does not move, through the load and the frames that lay the level out.
#[test]
fn the_world_holds_still_while_the_level_is_made() {
    let mut app = headless_game();
    begin_delve(&mut app);
    run(&mut app, 60);
    into_the_loading(&mut app);
    let body = player(&mut app);
    let snap = |app: &App| (app.world().resource::<Time<Virtual>>().elapsed(), format!("{:?}", app.world().get::<Health>(body)), format!("{:?}", app.world().get::<forever_below::character::DelverNeeds>(body)));
    let before = snap(&app);
    assert!(app.world().resource::<Time<Virtual>>().is_paused());
    let started = std::time::Instant::now();
    while app.world().get_resource::<Journey>().is_some_and(|j| matches!(j.phase, Phase::Loading | Phase::Settling)) {
        assert!(started.elapsed().as_secs() < 120);
        assert_eq!(snap(&app), before, "the world moved while the screen was black");
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(!app.world().resource::<Time<Virtual>>().is_paused(), "the clock runs again as the screen fades in");
    run(&mut app, 120);
    assert!(app.world().get_resource::<Journey>().is_none());
}

/// A fall that ends the delve mid-journey leaves no loading screen or load behind.
#[test]
fn ending_the_delve_mid_journey_leaves_no_loading_screen() {
    let mut app = headless_game();
    begin_delve(&mut app);
    run(&mut app, 60);
    into_the_loading(&mut app);
    app.world_mut().resource_mut::<LoadingLevel>().hold_for(30.0);
    let started = std::time::Instant::now();
    while count::<LoadingScreen>(&mut app) == 0 {
        assert!(started.elapsed().as_secs() < 120, "the screen was never drawn");
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Title);
    run(&mut app, 3);
    assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Title);
    assert_eq!(count::<LoadingScreen>(&mut app), 0);
    assert!(app.world().get_resource::<LoadingLevel>().is_none());
    assert!(app.world().get_resource::<Journey>().is_none());
    assert!(!app.world().resource::<Time<Virtual>>().is_paused());
}

/// A level that is ready at once (the prefetched one below) never shows the screen.
#[test]
fn a_level_ready_at_once_does_not_flash_the_loading_screen() {
    let mut app = headless_game();
    begin_delve(&mut app);
    // Let the one below be made.
    let started = std::time::Instant::now();
    while app.world().get_resource::<Below>().is_none_or(|below| !below.is_ready()) {
        assert!(started.elapsed().as_secs() < 120, "the level below was not made");
        run(&mut app, 1);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let down = *level(&app).stair(StairKind::Down).expect("a way down");
    stand(&mut app, down.approach(), down.facing.yaw());
    run(&mut app, 2);
    press_f(&mut app);
    let mut drawn = false;
    for _ in 0..200 {
        run(&mut app, 1);
        drawn |= count::<LoadingScreen>(&mut app) > 0;
    }
    assert_eq!(app.world().resource::<Abyss>().depth, 2);
    assert!(!drawn, "the screen was flashed for a level that was ready");
}

#[test]
fn a_stair_is_not_used_with_the_back_turned_or_from_afar() {
    let mut app = headless_game();
    begin_delve(&mut app);
    run(&mut app, 60);
    let down = *level(&app).stair(StairKind::Down).expect("a way down");
    // Beside it but looking away: F does nothing.
    stand(&mut app, down.approach(), down.facing.opposite().yaw());
    run(&mut app, 5);
    press_f(&mut app);
    run(&mut app, 5);
    assert!(app.world().get_resource::<Journey>().is_none());
    // In the starting cell, a long way off: nothing either.
    let (sx, sy, facing) = level(&app).start();
    stand(&mut app, (sx, sy), facing.yaw());
    run(&mut app, 5);
    press_f(&mut app);
    run(&mut app, 5);
    assert!(app.world().get_resource::<Journey>().is_none());
    assert_eq!(app.world().resource::<Abyss>().depth, 1);
}

#[test]
fn a_fall_on_any_depth_starts_the_next_delve_at_the_first_level_entrance() {
    let mut app = headless_game();
    begin_delve(&mut app);
    run(&mut app, 60);
    let first = level(&app).clone();
    let down = *first.stair(StairKind::Down).expect("a way down");
    stand(&mut app, down.approach(), down.facing.yaw());
    run(&mut app, 30);
    press_f(&mut app);
    run(&mut app, 200);
    assert_eq!(app.world().resource::<Abyss>().depth, 2);

    // Fall on the second level, with every revive spent.
    app.insert_resource(forever_below::combat::revival::Revives(u32::MAX));
    let body = player(&mut app);
    app.world_mut().get_mut::<Health>(body).unwrap().hp = 0;
    run(&mut app, ((LAST_GASP + FALL_TIME) * 60.0) as usize + 30);
    assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Title);
    run(&mut app, 30);
    assert_eq!(*app.world().resource::<Abyss>(), Abyss::default(), "the run is over");

    // A new delve begins at the first level's entrance, whose door shuts behind the delver.
    app.world_mut().write_message(MenuCommand::Creator(CreatorCommand::Begin));
    run(&mut app, 90);
    assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Delving);
    assert_eq!(*app.world().resource::<Abyss>(), Abyss { run_seed: 1207, depth: 1, arrival: Arrival::Entered });
    assert_eq!(*level(&app), first);
    let (sx, sy, facing) = level(&app).start();
    assert_at(&mut app, (sx, sy), facing, "the new delve");
    let world = app.world_mut();
    let doors: Vec<_> = world.query::<&EntranceDoor>().iter(world).map(|d| d.swing).collect();
    assert_eq!(doors, vec![DoorSwing::Open]);
    // Walking in shuts it.
    stand(&mut app, (sx, sy), facing.yaw());
    let (dx, dy) = facing.step();
    stand(&mut app, ((sx as i32 + dx * 2) as u32, (sy as i32 + dy * 2) as u32), facing.yaw());
    run(&mut app, 120);
    let world = app.world_mut();
    let doors: Vec<_> = world.query::<&EntranceDoor>().iter(world).map(|d| d.swing).collect();
    assert_eq!(doors, vec![DoorSwing::Shut]);
}

#[test]
fn no_screen_is_open_while_the_level_changes_under_it() {
    let mut app = headless_game();
    begin_delve(&mut app);
    run(&mut app, 60);
    // The inventory is open when the way down is taken (as a message can arrive from anywhere):
    // the journey closes it.
    app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
    run(&mut app, 20);
    app.world_mut().write_message(UseStair(StairKind::Down));
    run(&mut app, 3);
    assert!(app.world().get_resource::<Journey>().is_some());
    assert_eq!(app.world().resource::<ScreenOpen>().0, None, "the screen is closed as the journey begins");
    // And I, K, M and Esc open nothing until it is over, through the swap and the fade in.
    for (key, text) in [(KeyCode::KeyI, "i"), (KeyCode::KeyK, "k"), (KeyCode::KeyM, "m"), (KeyCode::Escape, "")] {
        press(&mut app, key, text);
        assert_eq!(app.world().resource::<ScreenOpen>().0, None, "{key:?} opened a screen mid-journey");
    }
    for _ in 0..120 {
        run(&mut app, 1);
        assert_eq!(app.world().resource::<ScreenOpen>().0, None);
    }
    assert!(app.world().get_resource::<Journey>().is_none());
    assert_eq!(app.world().resource::<Abyss>().depth, 2);
    // Once it is over they open again.
    press(&mut app, KeyCode::KeyI, "i");
    assert_eq!(app.world().resource::<ScreenOpen>().0, Some(Screen::Inventory));
}

#[test]
fn a_fallen_delver_takes_no_stairs() {
    let mut app = headless_game();
    begin_delve(&mut app);
    run(&mut app, 60);
    let down = *level(&app).stair(StairKind::Down).expect("a way down");
    stand(&mut app, down.approach(), down.facing.yaw());
    run(&mut app, 10);
    let body = player(&mut app);
    app.world_mut().get_mut::<Health>(body).unwrap().hp = 0;
    run(&mut app, 10);
    press_f(&mut app);
    app.world_mut().write_message(UseStair(StairKind::Down));
    run(&mut app, 30);
    assert!(app.world().get_resource::<Journey>().is_none(), "the fallen cannot take the stairs");
    assert_eq!(app.world().resource::<Abyss>().depth, 1);
}

#[derive(Resource, Default, Debug)]
struct Seen(Vec<(u32, u32, usize, usize)>);

#[test]
fn the_old_level_is_whole_for_the_frame_it_is_left() {
    let mut app = headless_game();
    app.init_resource::<Seen>().add_systems(
        Update,
        (|level: Res<CurrentLevel>, abyss: Res<Abyss>, journey: Res<Journey>, items: Query<&FloorItem>, props: Query<&PropPiece>, mut seen: ResMut<Seen>| {
            let _ = journey;
            seen.0.push((level.0.depth, abyss.depth, items.iter().count(), props.iter().count()));
        })
        .in_set(LeavingLevel),
    );
    begin_delve(&mut app);
    run(&mut app, 60);
    let first = level(&app).clone();
    app.world_mut().write_message(UseStair(StairKind::Down));
    run(&mut app, 200);
    assert_eq!(app.world().resource::<Abyss>().depth, 2);
    let seen = &app.world().resource::<Seen>().0;
    assert_eq!(seen.len(), 1, "it runs once, for the one frame at black: {seen:?}");
    let (level_depth, abyss_depth, items, props) = seen[0];
    assert_eq!((level_depth, abyss_depth), (1, 1), "still the level being left");
    assert!(items > 0 && items <= first.items.len(), "its floor items are all still there: {items}");
    assert_eq!(props, first.props.len(), "and its props");
}
