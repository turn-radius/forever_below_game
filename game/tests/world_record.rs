//! The world record (#285): every level keeps everything that happened on it, and coming back
//! rebuilds it from its seed as it was left.
//!
//! The whole game runs here without a window or a GPU, as in `travel.rs`, stepped a 60th of a
//! second a frame, from the title through New delve into level 1207.

use bevy::{
    ecs::system::RunSystemOnce,
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    time::TimeUpdateStrategy,
    window::ExitCondition,
    winit::WinitPlugin,
};
use fb_level::{CreatureSpot, ItemSpot, Level, StairKind};
use forever_below::{
    carry::{FloorItem, lit_torch},
    combat::{
        Health,
        feedback::{Corpse, Struck},
        vitals::FALL_TIME,
    },
    creatures::Critter,
    inventory::{FloorChange, FloorChanges, Inventory, ItemId, OpenContainer},
    launch::AppState,
    menu::MenuCommand,
    play::{CurrentLevel, PlayPlugin, Player, ScreenOpen},
    props::{Container, PropPiece, open_container},
    record::WorldRecord,
    render::{ArmoryRenderPlugin, ItemSpawner},
    settings::SettingsPlugin,
    title::{NewDelveSeed, TitlePlugin, creator::CreatorCommand},
    travel::{Abyss, Journey, UseStair},
    tuning::{Tuning, TuningPlugin},
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

fn run(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
    }
}

/// A new delve in level 1207, a second in.
fn delving() -> App {
    let mut app = headless_game();
    run(&mut app, 30);
    app.world_mut().write_message(MenuCommand::Creator(CreatorCommand::Begin));
    run(&mut app, 90);
    assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Delving);
    run(&mut app, 60);
    app
}

fn level(app: &App) -> &Level {
    &app.world().resource::<CurrentLevel>().0
}

fn player(app: &mut App) -> Entity {
    let world = app.world_mut();
    world.query_filtered::<Entity, With<Player>>().single(world).expect("one delver")
}

fn depth(app: &App) -> u32 {
    app.world().resource::<Abyss>().depth
}

fn go(app: &mut App, by: StairKind) {
    let from = depth(app);
    app.world_mut().write_message(UseStair(by));
    run(app, 200);
    assert!(app.world().get_resource::<Journey>().is_none(), "the journey is over");
    assert_eq!(depth(app), if by == StairKind::Down { from + 1 } else { from - 1 });
}

/// Down the stair and straight back up, ending on the same level.
fn round_trip(app: &mut App) {
    go(app, StairKind::Down);
    go(app, StairKind::Up);
}

/// Every item lying on the floor, with its spot.
fn lying(app: &mut App) -> Vec<(ItemSpot, fb_items::Item)> {
    let world = app.world_mut();
    let floor: Vec<_> = world.query::<&FloorItem>().iter(world).map(|f| (f.id, f.spot)).collect();
    let inventory = world.resource::<Inventory>();
    floor.into_iter().map(|(id, spot)| (spot, inventory.item(id).expect("a floor item is in the inventory").clone())).collect()
}

/// How many items of this base and seed the inventory keeps track of, anywhere.
fn kept(app: &App, base: &str, seed: u32) -> usize {
    let inventory = app.world().resource::<Inventory>();
    inventory.ids().filter(|&id| inventory.item(id).is_some_and(|i| i.base == base && i.seed == seed)).count()
}

fn set_down_at(app: &mut App, id: ItemId, spot: Vec2, yaw: f32) {
    app.world_mut().resource_mut::<FloorChanges>().0.push(FloorChange::SetDownAt(id, spot, yaw));
    run(app, 5);
}

fn start_cell(app: &App) -> Vec2 {
    let (sx, sy, _) = level(app).start();
    Vec2::new(sx as f32 + 0.5, sy as f32 + 0.5)
}

fn by_spot(items: &mut [(ItemSpot, fb_items::Item)]) {
    items.sort_by_key(|(spot, item)| (item.seed, spot.x.to_bits(), spot.y.to_bits()));
}

#[test]
fn a_dropped_sword_lies_where_it_was_dropped_after_round_trips_and_is_never_duplicated() {
    let mut app = delving();
    let at = start_cell(&app);
    let sword = app.world_mut().resource_mut::<Inventory>().add(fb_items::roll_item("Sword", 40_001).unwrap());
    set_down_at(&mut app, sword, at, 0.8);
    assert_eq!(kept(&app, "Sword", 40_001), 1);
    let mut before = lying(&mut app);
    by_spot(&mut before);
    assert!(before.iter().any(|(spot, item)| item.seed == 40_001 && (spot.x, spot.y, spot.yaw) == (at.x, at.y, 0.8)));
    let total = app.world().resource::<Inventory>().ids().count();
    for trip in 0..3 {
        round_trip(&mut app);
        let mut now = lying(&mut app);
        by_spot(&mut now);
        assert_eq!(now, before, "trip {trip}: the floor is as it was left, item for item");
        assert_eq!(kept(&app, "Sword", 40_001), 1, "trip {trip}: one sword, not two");
        assert_eq!(app.world().resource::<Inventory>().ids().count(), total, "trip {trip}: nothing made or lost by travelling");
    }
}

/// The level below is made ahead of time on the way down; coming back to it after a visit is
/// from the record, which the loot rolled ahead must not override.
#[test]
fn an_item_dropped_on_the_level_below_is_still_there_after_up_and_down_again() {
    let mut app = delving();
    go(&mut app, StairKind::Down);
    assert!(app.world().resource::<forever_below::staged::Staged>().taken > 0, "the first visit used what was made ahead");
    let at = start_cell(&app);
    let sword = app.world_mut().resource_mut::<Inventory>().add(fb_items::roll_item("Sword", 40_002).unwrap());
    set_down_at(&mut app, sword, at, 0.3);
    // (What burns, a lit lantern, burns on while its level is left, so what is compared is
    // what lies where: each thing's base and seed, and the sword in full.)
    let floor = |app: &mut App| {
        let mut items = lying(app);
        by_spot(&mut items);
        items.into_iter().map(|(spot, item)| (spot, item.base.clone(), item.seed)).collect::<Vec<_>>()
    };
    let before = floor(&mut app);
    let total = app.world().resource::<Inventory>().ids().count();
    for trip in 0..2 {
        go(&mut app, StairKind::Up);
        go(&mut app, StairKind::Down);
        assert_eq!(floor(&mut app), before, "trip {trip}: the floor is the record, not loot rolled again");
        assert_eq!(kept(&app, "Sword", 40_002), 1, "trip {trip}: one sword");
        assert_eq!(app.world().resource::<Inventory>().ids().count(), total, "trip {trip}: nothing made or lost");
    }
}

#[test]
fn a_torch_burnt_on_the_floor_is_as_burnt_when_the_delver_returns() {
    let mut app = delving();
    let at = start_cell(&app);
    let torch = lit_torch(7);
    let seed = torch.seed;
    let id = app.world_mut().resource_mut::<Inventory>().add(torch.clone());
    set_down_at(&mut app, id, at, 0.0);
    // Burn it down a little where it lies, then leave.
    app.world_mut().resource_mut::<Inventory>().burn_lights(600.0);
    let burnt = lying(&mut app).into_iter().find(|(_, i)| i.seed == seed).unwrap().1;
    assert_ne!(burnt.shape, torch.shape, "it has burnt");
    round_trip(&mut app);
    let after = lying(&mut app).into_iter().find(|(_, i)| i.seed == seed).expect("the torch lies where it was left").1;
    // (It goes on burning while the delver is on the level, so it is as burnt as when they left,
    // which is no fresher than when it was looked at.)
    let (fresh, then, now) = (torch.fuel_left().unwrap(), burnt.fuel_left().unwrap(), after.fuel_left().unwrap());
    assert!(now <= then && then < fresh - 500.0, "burnt as it was left, not fresh: {fresh} then {then} now {now}");
}

#[test]
fn what_was_taken_stays_gone_and_a_level_never_visited_is_as_generated() {
    let mut app = delving();
    let abyss = *app.world().resource::<Abyss>();
    assert!(app.world().resource::<WorldRecord>().levels.is_empty(), "nothing is recorded until a level is left");
    go(&mut app, StairKind::Down);
    assert_eq!(*level(&app), abyss.level_at(2), "a level never visited is exactly its generated state");
    let record = app.world().resource::<WorldRecord>();
    assert!(record.level(1).is_some() && record.level(2).is_none());
    let generated = lying(&mut app).len();
    assert!(generated > 0, "level 2 has items");
    // Take up the first thing that fits.
    let ids: Vec<_> = {
        let world = app.world_mut();
        world.query::<&FloorItem>().iter(world).map(|f| f.id).collect()
    };
    let taken = ids.into_iter().find(|&id| app.world_mut().resource_mut::<Inventory>().pick_up(id).is_ok()).expect("something could be picked up");
    app.world_mut().resource_mut::<FloorChanges>().0.push(FloorChange::Lifted(taken));
    run(&mut app, 5);
    let (base, seed) = {
        let item = app.world().resource::<Inventory>().item(taken).unwrap();
        (item.base.clone(), item.seed)
    };
    assert_eq!(lying(&mut app).len(), generated - 1);
    for _ in 0..2 {
        go(&mut app, StairKind::Up);
        go(&mut app, StairKind::Down);
        assert_eq!(lying(&mut app).len(), generated - 1, "it is gone from the floor");
        assert!(lying(&mut app).iter().all(|(_, i)| !(i.base == base && i.seed == seed)));
        assert_eq!(kept(&app, &base, seed), 1, "and it is the delver's, once");
    }
}

#[test]
fn a_creature_killed_stays_dead_and_one_hurt_stays_hurt() {
    let mut app = delving();
    let mut critters: Vec<(Entity, CreatureSpot)> = {
        let world = app.world_mut();
        world.query::<(Entity, &Critter)>().iter(world).map(|(e, c)| (e, c.spot)).collect()
    };
    assert!(critters.len() >= 2, "level 1 of 1207 has creatures to fight");
    critters.sort_by_key(|(_, spot)| spot.seed);
    let (dead, dead_spot) = critters[0];
    let (hurt, hurt_spot) = critters[1];
    app.world_mut().get_mut::<Health>(hurt).unwrap().hp -= 1;
    let hp = app.world().get::<Health>(hurt).unwrap().hp;
    app.world_mut().write_message(Struck { target: dead, damage: 10_000, from: Vec2::ZERO, bludgeoning: false, critical: false });
    run(&mut app, 60);
    assert!(app.world().get::<Corpse>(dead).is_some());
    for _ in 0..2 {
        round_trip(&mut app);
        let world = app.world_mut();
        let now: Vec<_> = world.query::<(&Critter, &Health, Has<Corpse>)>().iter(world).map(|(c, h, dead)| (c.spot, *h, dead)).collect();
        let (_, health, is_dead) = *now.iter().find(|(s, _, _)| *s == dead_spot).expect("its body is still there");
        assert!(is_dead && health.hp == 0, "a rat killed stays dead");
        let (_, health, is_dead) = *now.iter().find(|(s, _, _)| *s == hurt_spot).unwrap();
        assert!(!is_dead && health.hp == hp, "a creature hurt stays as hurt: {} not {hp}", health.hp);
    }
}

#[test]
fn a_bag_set_down_keeps_its_contents_after_a_round_trip() {
    let mut app = delving();
    let at = start_cell(&app);
    let sack = {
        let mut inventory = app.world_mut().resource_mut::<Inventory>();
        let sack = inventory.add(fb_items::roll_item("Sack", 40_002).unwrap());
        let dagger = inventory.add(fb_items::roll_item("Dagger", 40_003).unwrap());
        let free = inventory.grid(sack).unwrap().first_free(inventory.footprint(dagger).unwrap()).unwrap();
        inventory.put(dagger, sack, free).unwrap();
        sack
    };
    set_down_at(&mut app, sack, at, 0.0);
    for _ in 0..2 {
        round_trip(&mut app);
        let world = app.world_mut();
        let ids: Vec<_> = world.query::<&FloorItem>().iter(world).map(|f| f.id).collect();
        let inventory = world.resource::<Inventory>();
        let bag = ids.into_iter().find(|&id| inventory.item(id).is_some_and(|i| i.seed == 40_002)).expect("the sack lies where it was set down");
        let inside: Vec<_> = inventory.grid(bag).unwrap().entries().iter().map(|e| inventory.item(e.id).map(|i| (i.base.clone(), i.seed))).collect();
        assert_eq!(inside, vec![Some(("Dagger".to_string(), 40_003))], "the bag keeps what was in it");
        assert_eq!(kept(&app, "Dagger", 40_003), 1);
    }
}

#[test]
fn an_opened_chest_stays_open_with_what_is_left_in_it() {
    let mut app = delving();
    let (index, entity, container) = {
        let world = app.world_mut();
        let mut all: Vec<_> = world.query::<(Entity, &PropPiece, &Container)>().iter(world).map(|(e, p, c)| (p.index, e, *c)).collect();
        all.sort_by_key(|a| a.0);
        let inventory = world.resource::<Inventory>();
        all.into_iter().find(|(_, _, c)| inventory.contents_count(c.id) >= 2).expect("a container with something in it")
    };
    // Open it, as the pick-up key does, and take one thing out.
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut spawner: ItemSpawner,
                  level: Res<CurrentLevel>,
                  mut open: ResMut<ScreenOpen>,
                  mut showing: ResMut<OpenContainer>,
                  tuning: Res<Tuning>,
                  pieces: Query<&PropPiece>| {
                open_container(&mut commands, &mut spawner, &level.0, entity, pieces.get(entity).unwrap(), container, &mut open, &mut showing, None, &tuning);
            },
        )
        .unwrap();
    run(&mut app, 5);
    let taken = app.world().resource::<Inventory>().grid(container.id).unwrap().entries()[0].id;
    let taken_seed = app.world().resource::<Inventory>().item(taken).unwrap().seed;
    app.world_mut().resource_mut::<Inventory>().stow(taken).ok();
    let left_in = |app: &App, id: ItemId| -> Vec<(String, u32)> {
        let inventory = app.world().resource::<Inventory>();
        inventory.grid(id).unwrap().entries().iter().map(|e| (inventory.item(e.id).unwrap().base.clone(), inventory.item(e.id).unwrap().seed)).collect()
    };
    let remaining = left_in(&app, container.id);
    assert!(!remaining.iter().any(|(_, s)| *s == taken_seed), "one thing was taken out");
    for _ in 0..2 {
        round_trip(&mut app);
        let world = app.world_mut();
        let now = world.query::<(&PropPiece, &Container)>().iter(world).find(|(p, _)| p.index == index).map(|(_, c)| *c).expect("the chest stands where it did");
        assert!(now.opened, "it stands open");
        assert_eq!(left_in(&app, now.id), remaining, "with what was left in it");
    }
}

#[test]
fn the_world_record_round_trips_through_serde_and_death_resets_it() {
    let mut app = delving();
    let at = start_cell(&app);
    let sword = app.world_mut().resource_mut::<Inventory>().add(fb_items::roll_item("Sword", 40_001).unwrap());
    set_down_at(&mut app, sword, at, 0.8);
    round_trip(&mut app);
    let record = app.world().resource::<WorldRecord>().clone();
    assert!(record.level(1).is_some_and(|l| !l.floor.is_empty() && !l.creatures.is_empty()));
    let json = serde_json::to_string(&record).unwrap();
    assert_eq!(serde_json::from_str::<WorldRecord>(&json).unwrap(), record);

    // Fall: the next delve starts with a clean record, and level 1 as generated.
    let body = player(&mut app);
    app.world_mut().get_mut::<Health>(body).unwrap().hp = 0;
    run(&mut app, (FALL_TIME * 60.0) as usize + 60);
    assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Title);
    app.world_mut().write_message(MenuCommand::Creator(CreatorCommand::Begin));
    run(&mut app, 90);
    assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Delving);
    run(&mut app, 30);
    assert_eq!(*app.world().resource::<WorldRecord>(), WorldRecord::default());
    let generated = fb_level::generate(1207, 1);
    assert_eq!(*level(&app), generated);
    assert!(lying(&mut app).iter().all(|(spot, _)| generated.items.contains(spot)), "level 1 is as generated again");
    assert_eq!(kept(&app, "Sword", 40_001), 0, "the dropped sword is not in the new run");
}
