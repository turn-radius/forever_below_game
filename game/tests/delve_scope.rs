//! Leaving the delve clears everything it spawned, by Bevy's state-scoped entities (#177).
//!
//! The whole game runs here without a window or a GPU (Bevy's `no_renderer` example: a
//! `RenderPlugin` with no backends), stepped a 60th of a second a frame.

use std::collections::HashSet;

use bevy::{
    ecs::{component::ComponentId, entity_disabling::Disabled, query::Allow, resource::{IS_RESOURCE, IsResource}},
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    state::state::PreviousState,
    time::TimeUpdateStrategy,
    window::ExitCondition,
    winit::WinitPlugin,
};
use forever_below::{
    combat::{Health, vitals::FALL_TIME},
    inventory::InventoryScreen,
    launch::AppState,
    map::screen::MapScreen,
    menu::{MenuCommand, MenuScreen},
    play::{CurrentLevel, PlayPlugin, Player, Screen, ScreenOpen},
    render::ArmoryRenderPlugin,
    settings::SettingsPlugin,
    title::{NewDelveSeed, TitlePlugin},
    tuning::TuningPlugin,
};

/// The game as `main` builds it, opening on the title, with New delve going into level 1207.
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

fn run(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
    }
}

fn state(app: &App) -> AppState {
    *app.world().resource::<State<AppState>>().get()
}

/// Chooses New delve on the title and lets the delve set itself up.
fn begin_delve(app: &mut App) {
    app.world_mut().write_message(MenuCommand::NewDelve);
    run(app, 90);
    assert_eq!(state(app), AppState::Delving);
}

/// Drops the delver to 0 hit points and waits out the fall, back to the title.
fn fall(app: &mut App) {
    let world = app.world_mut();
    let player = world.query_filtered::<Entity, With<Player>>().single(world).expect("one delver");
    world.get_mut::<Health>(player).expect("the delver has hit points").hp = 0;
    run(app, (FALL_TIME * 60.0) as usize + 30);
    assert_eq!(state(app), AppState::Title);
    run(app, 30);
}

fn every_entity(app: &mut App) -> Vec<Entity> {
    let world = app.world_mut();
    world.query_filtered::<Entity, Allow<Disabled>>().iter(world).collect()
}

/// Every entity as its name and kinds of component, so two visits to the title compare
/// whatever the entities' ids. Two kinds of resource entity (Bevy 0.20 keeps each resource on
/// an entity of its own) are left out, as neither grows: Bevy's `PreviousState<AppState>`,
/// made by the first change of state and kept from then on, and the empty entity a removed
/// resource leaves, which Bevy keeps for that resource and reuses when it is inserted again
/// (`World::remove_resource` takes the value and leaves the entity; `insert_resource` looks it
/// up first).
fn census(app: &mut App) -> Vec<String> {
    let entities = every_entity(app);
    let world = app.world();
    let skip = world.components().component_id::<PreviousState<AppState>>();
    // The title's music comes and goes on its own schedule (`crate::audio`), fading out and
    // starting the next track whenever it does; the delve's sounds are scoped to it like the
    // rest of what it spawns (the other test).
    let audio = world.components().component_id::<AudioPlayer>();
    let mut census: Vec<String> = entities
        .into_iter()
        .filter_map(|entity| {
            let mut kinds: Vec<ComponentId> = world.inspect_entity(entity).ok()?.map(|(id, _)| id).collect();
            if [skip, audio].into_iter().flatten().any(|id| kinds.contains(&id)) || kinds == [IS_RESOURCE] {
                return None;
            }
            kinds.sort();
            Some(format!("{:?} {kinds:?}", world.get::<Name>(entity).map(Name::as_str)))
        })
        .collect();
    census.sort();
    census
}

/// Every entity that is neither scoped to the delve nor under one that is, and was not there
/// on the title before it. Resources (entities in Bevy 0.20) and observers are left out.
fn unscoped(app: &mut App, before: &HashSet<Entity>) -> Vec<String> {
    let entities = every_entity(app);
    let world = app.world();
    let scoped = |mut entity: Entity| loop {
        if world.get::<DespawnOnExit<AppState>>(entity).is_some_and(|scope| scope.0 == AppState::Delving) {
            return true;
        }
        match world.get::<ChildOf>(entity) {
            Some(child_of) => entity = child_of.parent(),
            None => return false,
        }
    };
    entities
        .into_iter()
        .filter(|&e| !before.contains(&e) && !scoped(e))
        .filter(|&e| world.get::<Observer>(e).is_none() && world.get::<IsResource>(e).is_none())
        .map(|e| {
            let name = world.get::<Name>(e).map_or("unnamed", Name::as_str);
            let kinds: Vec<String> = world.inspect_entity(e).unwrap().map(|(_, c)| format!("{:?}", c.name())).collect();
            format!("{e} {name}: {}", kinds.join(", "))
        })
        .collect()
}

#[test]
fn everything_the_delve_spawns_is_scoped_to_it() {
    let mut app = headless_game();
    run(&mut app, 30);
    let before: HashSet<Entity> = every_entity(&mut app).into_iter().collect();
    begin_delve(&mut app);
    let mut checks = vec![("the level", None)];
    // The screens over the level too: the inventory (with its doll, portrait and icons), then
    // the menu.
    checks.extend([("the inventory", Some(Screen::Inventory)), ("the map", Some(Screen::Map)), ("the menu", Some(Screen::Menu))]);
    for (what, screen) in checks {
        if screen.is_some() {
            app.world_mut().resource_mut::<ScreenOpen>().0 = screen;
            run(&mut app, 20);
        }
        let world = app.world_mut();
        let shown = match screen {
            None => world.query_filtered::<(), With<Player>>().iter(world).count(),
            Some(Screen::Inventory) => world.query_filtered::<(), With<InventoryScreen>>().iter(world).count(),
            Some(Screen::Menu) => world.query_filtered::<(), With<MenuScreen>>().iter(world).count(),
            Some(Screen::Map) => world.query_filtered::<(), With<MapScreen>>().iter(world).count(),
        };
        assert_eq!(shown, 1, "{what} is not showing");
        let left = unscoped(&mut app, &before);
        assert!(left.is_empty(), "with {what} showing, {} entities of the delve are not scoped to it:\n{}", left.len(), left.join("\n"));
    }
}

#[test]
fn after_a_fall_the_title_holds_what_a_fresh_launch_did() {
    let mut app = headless_game();
    run(&mut app, 30);
    let fresh = census(&mut app);
    assert!(app.world().get_resource::<CurrentLevel>().is_none());
    for fall_number in 1..=2 {
        begin_delve(&mut app);
        fall(&mut app);
        let mut gained = census(&mut app);
        let mut lost = Vec::new();
        for entity in &fresh {
            match gained.iter().position(|e| e == entity) {
                Some(i) => {
                    gained.remove(i);
                }
                None => lost.push(entity.clone()),
            }
        }
        assert!(gained.is_empty() && lost.is_empty(), "after fall {fall_number} the title gained {gained:#?} and lost {lost:#?}");
        assert!(app.world().get_resource::<CurrentLevel>().is_none(), "the level outlived the delve");
    }
}

#[test]
fn the_delve_draws_the_orbs_and_the_inventory_hides_them() {
    use forever_below::character::{DelverMana, DelverStamina};
    use forever_below::combat::orbs::Orb;

    let mut app = headless_game();
    run(&mut app, 30);
    begin_delve(&mut app);
    let world = app.world_mut();
    let player = world.query_filtered::<Entity, With<Player>>().single(world).expect("one delver");
    assert!(world.get::<DelverStamina>(player).is_some() && world.get::<DelverMana>(player).is_some(), "every delver has the pools");
    let hud: Vec<Entity> = world.query_filtered::<&ChildOf, With<Orb>>().iter(world).map(|c| c.parent()).collect();
    assert_eq!(hud.len(), 3, "a life, stamina and mana orb");
    assert_eq!(*app.world().get::<Visibility>(hud[0]).unwrap(), Visibility::Inherited);
    app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
    run(&mut app, 10);
    assert_eq!(*app.world().get::<Visibility>(hud[0]).unwrap(), Visibility::Hidden, "hidden under the inventory");
    app.world_mut().resource_mut::<ScreenOpen>().0 = None;
    run(&mut app, 10);
    assert_eq!(*app.world().get::<Visibility>(hud[0]).unwrap(), Visibility::Inherited);
}
