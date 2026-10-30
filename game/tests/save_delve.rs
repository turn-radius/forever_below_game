//! The saved delve (#689): lying down in a camp saves the whole delve, and resuming it brings
//! back every piece of it.
//!
//! The whole game runs here without a window or a GPU, as in `world_record.rs`. A delve is
//! played on three levels with changes on each, the delver lies down in a pitched tent on the
//! third, and a second game resumes from the file. [`Snapshot`] reads the delve's state straight
//! from the live world, field by field, and never through `save::gather`, so a piece of state the
//! save forgot shows as a difference. [`every_resource_and_component_is_accounted_for`] is the
//! other net: it lists every resource and every component of the delver the game has and says
//! whether the save keeps it, so a new piece of state fails the test until someone decides.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use bevy::{
    ecs::system::RunSystemOnce,
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    time::TimeUpdateStrategy,
    window::ExitCondition,
    winit::WinitPlugin,
};
use fb_character::GameTime;
use fb_faction::{Faction, Profession, Standings, Style, Tier, Trade};
use fb_items::Coins;
use fb_level::{ItemSpot, Level, StairKind};
use forever_below::{
    camp::{Asleep, SetCamp},
    carry::{Aimed, FloorItem},
    character::{Character, DelverEffects, DelverMana, DelverNeeds, DelverRecovery, DelverStamina},
    combat::{
        Health,
        feedback::{Corpse, Struck},
        revival::Revives,
        vitals::{FALL_TIME, LAST_GASP},
    },
    commission::{Commission, CommissionScreen},
    creatures::Critter,
    enchanting::Enchanting,
    exhaustion::{Exhaustion, ExhaustionTimes},
    game_clock::GameClock,
    inventory::{FloorChange, FloorChanges, Inventory, ItemId, OpenContainer},
    launch::AppState,
    map::explore::ExploredLevels,
    menu::MenuCommand,
    play::{CurrentLevel, PlayPlugin, Player, ScreenOpen},
    player::Locomotion,
    props::{Container, PropPiece, open_container},
    quickslots::Quickslots,
    record::{LevelRecord, WorldRecord},
    render::{ArmoryRenderPlugin, ItemSpawner},
    save::{self, ResumeDelve, SaveFile, SavedGame, Saveable},
    settings::SettingsPlugin,
    title::{NewDelveSeed, TitlePlugin, creator::CreatorCommand},
    travel::{Abyss, Journey, UseStair},
    tuning::{Tuning, TuningPlugin},
};

fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("fb-save-delve-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    folder.join(save::FILE_NAME)
}

fn headless_game(file: &PathBuf) -> App {
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
    .insert_resource(SaveFile(Some(file.clone())))
    .add_plugins((SettingsPlugin, TuningPlugin, ArmoryRenderPlugin, PlayPlugin, TitlePlugin))
    .insert_resource(NewDelveSeed(Some(1207)));
    app
}

mod common;
use common::run;

/// A new delve in level 1207, a second in, which may save.
fn delving(file: &PathBuf) -> App {
    delving_in(file, 1207)
}

/// A new delve in the world of `seed`, a second in, which may save.
fn delving_in(file: &PathBuf, seed: u32) -> App {
    let mut app = headless_game(file);
    app.insert_resource(NewDelveSeed(Some(seed)));
    run(&mut app, 30);
    app.world_mut().write_message(MenuCommand::Creator(CreatorCommand::Begin));
    run(&mut app, 90);
    assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Delving);
    run(&mut app, 60);
    app.insert_resource(Saveable(true));
    app
}

fn level(app: &App) -> &Level {
    &app.world().resource::<CurrentLevel>().0
}

fn player(app: &mut App) -> Entity {
    let world = app.world_mut();
    world.query_filtered::<Entity, With<Player>>().single(world).expect("one delver")
}

fn go(app: &mut App, by: StairKind) {
    let from = app.world().resource::<Abyss>().depth;
    app.world_mut().write_message(UseStair(by));
    run(app, 200);
    assert!(app.world().get_resource::<Journey>().is_none(), "the journey is over");
    assert_eq!(app.world().resource::<Abyss>().depth, if by == StairKind::Down { from + 1 } else { from - 1 });
}

fn start_cell(app: &App) -> Vec2 {
    let (sx, sy, _) = level(app).start();
    Vec2::new(sx as f32 + 0.5, sy as f32 + 0.5)
}

fn set_down_at(app: &mut App, id: ItemId, spot: Vec2, yaw: f32) {
    app.world_mut().resource_mut::<FloorChanges>().0.push(FloorChange::SetDownAt(id, spot, yaw));
    run(app, 5);
}

/// Sets `base` from the pack down where a camp fits, as the inventory screen's F does, and
/// returns where the delver stood to do it and the way they faced.
fn set_camp_where(app: &mut App, base: &str, seed: u32) -> (Vec2, Vec2) {
    let lvl = level(app).clone();
    let blocked = app.world().resource::<forever_below::play::BlockedCells>().clone();
    let obstacles = app.world().resource::<forever_below::play::Obstacles>().0.clone();
    let set = fb_items::camp::set_up(&fb_items::roll_item(base, seed).unwrap()).unwrap();
    let (half, offset) = forever_below::camp::footprint(&set).unwrap();
    let gap = forever_below::camp::gap_for(set.camp().unwrap());
    let (feet, forward) = (0..lvl.height as i32)
        .flat_map(|y| (0..lvl.width as i32).map(move |x| (x, y)))
        .filter(|&(x, y)| lvl.cell(x, y) == fb_level::Cell::Floor)
        .flat_map(|(x, y)| [fb_level::Facing::North, fb_level::Facing::East, fb_level::Facing::South, fb_level::Facing::West].map(move |f| (x, y, f)))
        .map(|(x, y, f)| (Vec2::new(x as f32 + 0.5, y as f32 + 0.5) * fb_level::CELL, Vec2::new(f.step().0 as f32, f.step().1 as f32)))
        .find(|&(feet, forward)| {
            let site = forever_below::camp::site_ahead(feet, forward, half, gap);
            let canvas = fb_items::camp::canvas_half(&set.shape).map(|(x, z)| forever_below::camp::Site { centre: site.centre - site.to_floor(offset), half: Vec2::new(x, z), yaw: site.yaw });
            forever_below::camp::check_site(&site, &lvl, &blocked, &obstacles, &[]).is_ok() && canvas.is_none_or(|c| forever_below::camp::check_passage(&c, &lvl, &blocked, &obstacles, 3.5).is_ok())
        })
        .expect("somewhere in the level a camp fits");
    let id = app.world_mut().resource_mut::<Inventory>().add(fb_items::roll_item(base, seed).unwrap());
    let mut changes = FloorChanges::default();
    let said = forever_below::camp::lay_out(&mut app.world_mut().resource_mut::<Inventory>(), &mut changes, id, feet, forward, &lvl, &blocked, &obstacles, &[], 3.5);
    assert!(said.is_ok(), "{base}: {said:?}");
    app.world_mut().resource_mut::<FloorChanges>().0.extend(changes.0);
    run(app, 5);
    (feet, forward)
}

/// Puts the delver where they stood to set the camp, looking down at it, tired, with nothing
/// hostile near.
fn look_down_at_camp(app: &mut App, feet: Vec2, forward: Vec2, sleepiness: f64) {
    let delver = player(app);
    {
        let mut world = app.world_mut().entity_mut(delver);
        world.get_mut::<Transform>().unwrap().translation = Vec3::new(feet.x, 0.0, feet.y);
        let yaw = (-forward.x).atan2(-forward.y);
        world.get_mut::<Transform>().unwrap().rotation = Quat::from_rotation_y(yaw);
        let mut walk = world.get_mut::<Locomotion>().unwrap();
        walk.yaw = yaw;
        walk.pitch = -60f32.to_radians();
        walk.velocity = Vec2::ZERO;
        world.get_mut::<DelverNeeds>().unwrap().0.sleepiness.value = sleepiness;
    }
    // Whatever lives on the level is sent to a far corner of it, out of earshot of the camp.
    let world = app.world_mut();
    let mut creatures = world.query_filtered::<&mut Transform, With<Critter>>();
    for mut at in creatures.iter_mut(world) {
        at.translation = Vec3::new(10.0, 0.0, 10.0);
    }
    let mut eyes = world.query_filtered::<&mut Transform, With<forever_below::play::PlayerCamera>>();
    for mut eye in eyes.iter_mut(world) {
        eye.rotation = Quat::from_rotation_x(-60f32.to_radians());
    }
    run(app, 5);
    let aimed = app.world().resource::<Aimed>().0;
    assert!(aimed.is_some_and(|e| app.world().get::<SetCamp>(e).is_some()), "the camp is under the crosshair: {aimed:?}");
}

fn key(app: &mut App, key_code: KeyCode, text: &str, state: bevy::input::ButtonState) {
    app.world_mut().write_message(bevy::input::keyboard::KeyboardInput {
        key_code,
        logical_key: bevy::input::keyboard::Key::Character(text.into()),
        state,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

/// Everything the delve's state is, read from the live world a field at a time.
#[derive(Debug, PartialEq)]
struct Snapshot {
    abyss: (u32, u32),
    character: Character,
    coins: Coins,
    /// Everything worn or carried, by where it is, in full.
    carried: Vec<(String, fb_items::Item)>,
    quickslots: Vec<Option<String>>,
    at: [f32; 3],
    yaw: f32,
    pitch: f32,
    health: Health,
    needs: fb_character::Needs,
    stamina: fb_character::Stamina,
    mana: fb_character::Mana,
    exhaustion: fb_character::Exhaustion,
    times: fb_character::ExhaustionTimes,
    recovery: fb_character::recovery::Recovery,
    effects: Option<fb_character::effect::Effects>,
    clock: GameTime,
    revives: u32,
    enchanting: fb_items::scrap::EnchantingStock,
    explored: BTreeMap<u32, Vec<u8>>,
    standings: Standings,
    orders: Vec<Commission>,
    keys_given: Vec<Faction>,
    met_on: BTreeMap<Faction, BTreeSet<u32>>,
    /// The levels the delver is not on, as recorded.
    other_levels: BTreeMap<u32, LevelRecord>,
}

/// Walks what `id` holds, naming each by the slot and cells it lies in.
fn carried_in(inventory: &Inventory, id: ItemId, path: String, out: &mut Vec<(String, fb_items::Item)>, names: &mut BTreeMap<ItemId, String>) {
    out.push((path.clone(), inventory.item(id).expect("carried items are kept").clone()));
    names.insert(id, path.clone());
    if let Some(grid) = inventory.grid(id) {
        for entry in grid.entries() {
            carried_in(inventory, entry.id, format!("{path}/{},{}{}", entry.at.x, entry.at.y, if entry.at.turned { "t" } else { "" }), out, names);
        }
    }
}

fn snapshot(app: &mut App) -> Snapshot {
    let delver = player(app);
    let world = app.world();
    let abyss = world.resource::<Abyss>();
    let inventory = world.resource::<Inventory>();
    let (mut carried, mut names) = (Vec::new(), BTreeMap::new());
    for (slot, id) in inventory.worn_ids() {
        carried_in(inventory, id, format!("{slot:?}"), &mut carried, &mut names);
    }
    let body = world.entity(delver);
    let record = world.resource::<WorldRecord>();
    Snapshot {
        abyss: (abyss.run_seed, abyss.depth),
        character: world.resource::<Character>().clone(),
        coins: inventory.coins(),
        carried,
        quickslots: world.resource::<Quickslots>().0.iter().map(|bound| bound.and_then(|id| names.get(&id).cloned())).collect(),
        at: body.get::<Transform>().unwrap().translation.to_array(),
        yaw: body.get::<Locomotion>().unwrap().yaw,
        pitch: body.get::<Locomotion>().unwrap().pitch,
        health: *body.get::<Health>().unwrap(),
        needs: body.get::<DelverNeeds>().unwrap().0,
        stamina: body.get::<DelverStamina>().unwrap().0,
        mana: body.get::<DelverMana>().unwrap().0,
        exhaustion: body.get::<Exhaustion>().unwrap().0,
        times: body.get::<ExhaustionTimes>().unwrap().0,
        recovery: body.get::<DelverRecovery>().unwrap().0,
        effects: body.get::<DelverEffects>().map(|e| e.0.clone()),
        clock: world.resource::<GameClock>().time,
        revives: world.resource::<Revives>().0,
        enchanting: world.resource::<Enchanting>().0.clone(),
        explored: world.resource::<ExploredLevels>().0.iter().map(|(seed, seen)| (*seed, seen.to_bits())).collect(),
        standings: record.standings.clone(),
        orders: record.orders.clone(),
        keys_given: record.keys_given.clone(),
        met_on: record.met_on.clone(),
        other_levels: record.levels.iter().filter(|(depth, _)| **depth != abyss.depth).map(|(d, l)| (*d, l.clone())).collect(),
    }
}

/// The level the delver is on, as it lies, without where anything stands or what it is doing.
#[derive(Debug, PartialEq)]
struct Standing {
    floor: Vec<(ItemSpot, fb_items::Item)>,
    camps: Vec<fb_items::camp::Camp>,
    containers: Vec<(usize, bool, Vec<(String, u32)>)>,
    /// The urns in the niches, by niche, with what is left in each.
    urns: Vec<(usize, Vec<(String, u32)>)>,
    creatures: Vec<(u32, i32, bool)>,
}

fn standing(app: &mut App) -> Standing {
    let world = app.world_mut();
    let mut floor: Vec<_> = world.query::<&FloorItem>().iter(world).map(|f| (f.id, f.spot)).collect();
    floor.sort_by_key(|(id, _)| *id);
    let inventory = world.resource::<Inventory>().clone();
    let mut floor: Vec<(ItemSpot, fb_items::Item)> = floor.into_iter().map(|(id, spot)| (spot, inventory.item(id).unwrap().clone())).collect();
    floor.sort_by_key(|(spot, item)| (item.seed, spot.x.to_bits(), spot.y.to_bits()));
    let camps = world.query::<&SetCamp>().iter(world).map(|c| c.camp).collect();
    let mut containers: Vec<_> = world
        .query::<(&PropPiece, &Container)>()
        .iter(world)
        .map(|(p, c)| (p.index, c.opened, inventory.grid(c.id).unwrap().entries().iter().map(|e| (inventory.item(e.id).unwrap().base.clone(), inventory.item(e.id).unwrap().seed)).collect()))
        .collect();
    containers.sort_by_key(|c| c.0);
    let mut urns: Vec<_> = world
        .query::<(&forever_below::niches::NichePiece, &Container)>()
        .iter(world)
        .map(|(p, c)| (p.index, inventory.grid(c.id).unwrap().entries().iter().map(|e| (inventory.item(e.id).unwrap().base.clone(), inventory.item(e.id).unwrap().seed)).collect()))
        .collect();
    urns.sort_by_key(|u| u.0);
    let mut creatures: Vec<_> = world.query::<(&Critter, &Health, Has<Corpse>)>().iter(world).map(|(c, h, dead)| (c.spot.seed, h.hp, dead)).collect();
    creatures.sort();
    Standing { floor, camps, containers, urns, creatures }
}

/// Takes the first thing out of the first urn in a niche of this level that holds anything, if
/// there is one. Returns whether it did.
fn rob_an_urn(app: &mut App) -> bool {
    let world = app.world_mut();
    let inventory = world.resource::<Inventory>().clone();
    let mut urns: Vec<_> = world.query::<(&forever_below::niches::NichePiece, &Container)>().iter(world).map(|(p, c)| (p.index, *c)).collect();
    urns.sort_by_key(|u| u.0);
    let Some((_, urn)) = urns.into_iter().find(|(_, c)| inventory.contents_count(c.id) >= 1) else { return false };
    let taken = inventory.grid(urn.id).unwrap().entries()[0].id;
    world.resource_mut::<Inventory>().stow(taken).ok();
    true
}

/// A delve on three levels with something changed on each, ended by lying down in a pitched
/// tent on the third. Returns the game, what the delve was at that moment and the save file.
fn played(name: &str) -> (App, Snapshot, Standing, PathBuf) {
    let file = scratch(name);
    let mut app = delving(&file);

    // Level 1: a sword dropped, the nearest rat dead, a rat hurt, a chest opened and robbed.
    let at = start_cell(&app);
    let sword = app.world_mut().resource_mut::<Inventory>().add(fb_items::roll_item("Sword", 40_001).unwrap());
    set_down_at(&mut app, sword, at, 0.8);
    let mut critters: Vec<(Entity, u32)> = {
        let world = app.world_mut();
        world.query::<(Entity, &Critter)>().iter(world).map(|(e, c)| (e, c.spot.seed)).collect()
    };
    critters.sort_by_key(|c| c.1);
    let (dead, hurt) = (critters[0].0, critters[1].0);
    app.world_mut().get_mut::<Health>(hurt).unwrap().hp -= 1;
    app.world_mut().write_message(Struck { target: dead, damage: 10_000, from: Vec2::ZERO, bludgeoning: false, critical: false, by_ally: false, graze: false, fire: 0 });
    run(&mut app, 60);
    let (entity, container) = {
        let world = app.world_mut();
        let inventory = world.resource::<Inventory>().clone();
        let mut all: Vec<_> = world.query::<(Entity, &PropPiece, &Container)>().iter(world).map(|(e, p, c)| (p.index, e, *c)).collect();
        all.sort_by_key(|a| a.0);
        let (_, entity, container) = all.into_iter().find(|(_, _, c)| inventory.contents_count(c.id) >= 2).expect("a chest with something in it");
        (entity, container)
    };
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands, mut spawner: ItemSpawner, level: Res<CurrentLevel>, mut open: ResMut<ScreenOpen>, mut showing: ResMut<OpenContainer>, tuning: Res<Tuning>, pieces: Query<&PropPiece>| {
                open_container(&mut commands, &mut spawner, &level.0, entity, pieces.get(entity).unwrap(), container, &mut open, &mut showing, None, &tuning);
            },
        )
        .unwrap();
    run(&mut app, 5);
    let taken = app.world().resource::<Inventory>().grid(container.id).unwrap().entries()[0].id;
    app.world_mut().resource_mut::<Inventory>().stow(taken).ok();
    rob_an_urn(&mut app);
    go(&mut app, StairKind::Down);

    // Level 2: a sack with a dagger in it set down.
    let at = start_cell(&app);
    let sack = {
        let mut inventory = app.world_mut().resource_mut::<Inventory>();
        let sack = inventory.add(fb_items::roll_item("Sack", 40_002).unwrap());
        let dagger = inventory.add(fb_items::roll_item("Dagger", 40_003).unwrap());
        let free = inventory.grid(sack).unwrap().first_free(inventory.footprint(dagger).unwrap()).unwrap();
        inventory.put(dagger, sack, free).unwrap();
        sack
    };
    set_down_at(&mut app, sack, at, 0.1);
    rob_an_urn(&mut app);
    go(&mut app, StairKind::Down);

    // Level 3: a tent pitched, and the delve changed in every way it can be.
    let (feet, forward) = set_camp_where(&mut app, "Tent", 40_010);
    let bread = {
        let mut inventory = app.world_mut().resource_mut::<Inventory>();
        let bread = inventory.add(fb_items::roll_item("Bread", 40_020).unwrap());
        inventory.stow(bread).unwrap();
        inventory.add_coins(Coins { gp: 3, sp: 4, cp: 5 });
        bread
    };
    app.world_mut().resource_mut::<Quickslots>().bind(3, bread);
    let faction = fb_faction::World::new(1207).factions()[0].id;
    {
        let world = app.world_mut();
        let mut record = world.resource_mut::<WorldRecord>();
        record.standings.shift(faction, 25);
        record.keys_given.push(faction);
        record.note_met_on(faction, 2);
        record.note_met_on(faction, 3);
        let mut screen = CommissionScreen::begin(Trade { profession: Profession::Weaponsmith, tier: Tier::Master, style: Some(Style::Blades) }, "Oda", "the Brothers of the Last Door").unwrap();
        screen.maker_spot = 4242;
        screen.depth = 2;
        record.orders.push(screen.order(100.0, "Wren").unwrap());
        let mut character = world.resource_mut::<Character>();
        character.learned.strength = 1;
        character.books_read.push(77);
        world.resource_mut::<Enchanting>().0.essence = 7;
        world.resource_mut::<Revives>().0 = 1;
        let rules = world.resource::<forever_below::rules::GameRules>().0.clone();
        let delver = player(&mut app);
        let mut body = app.world_mut().entity_mut(delver);
        body.insert(DelverEffects::poisoned_for(&rules, 3.0));
        body.get_mut::<Health>().unwrap().hp -= 2;
        body.get_mut::<DelverNeeds>().unwrap().0.hunger.value = 0.2;
        body.get_mut::<DelverNeeds>().unwrap().0.thirst.value = 0.3;
        body.get_mut::<DelverStamina>().unwrap().0.value -= 0.3;
        body.get_mut::<ExhaustionTimes>().unwrap().0.sleep_hours = 0.7;
    }
    run(&mut app, 5);

    rob_an_urn(&mut app);
    // Lie down: F, and one frame.
    look_down_at_camp(&mut app, feet, forward, 0.6);
    key(&mut app, KeyCode::KeyF, "f", bevy::input::ButtonState::Pressed);
    app.update();
    assert!(app.world().resource::<Asleep>().0.is_some(), "asleep: {}", app.world().resource::<forever_below::inventory::Toast>().text);
    assert_eq!(app.world().resource::<forever_below::inventory::Toast>().text, "Your delve is saved.");
    let at_save = snapshot(&mut app);
    let lying = standing(&mut app);
    key(&mut app, KeyCode::KeyF, "f", bevy::input::ButtonState::Released);
    (app, at_save, lying, file)
}

/// Resumes the delve in `file` in a game of its own, through the title's Resume, up to the frame
/// its state is put back.
fn resumed(file: &PathBuf) -> (App, Snapshot) {
    let mut app = headless_game(file);
    run(&mut app, 30);
    assert!(app.world().resource::<SavedGame>().delve.is_some(), "the title found the save");
    app.world_mut().write_message(MenuCommand::ResumeDelve);
    let mut frames = 0;
    // The state is put back on the first frame of the delve; the next frame it is only played.
    loop {
        run(&mut app, 1);
        frames += 1;
        if *app.world().resource::<State<AppState>>().get() == AppState::Delving && app.world().get_resource::<ResumeDelve>().is_none() {
            break;
        }
        assert!(frames < 20, "the delve begins");
    }
    let snap = snapshot(&mut app);
    (app, snap)
}

/// An urn in a niche, robbed, stays robbed in a delve saved and resumed: the world of 4 has an urn
/// with something in it on its second level.
#[test]
fn an_urn_robbed_in_a_niche_stays_robbed_in_a_saved_delve() {
    let file = scratch("urn");
    let mut app = delving_in(&file, 4);
    go(&mut app, StairKind::Down);
    let held = |app: &mut App| -> Vec<(usize, usize)> {
        let world = app.world_mut();
        let inventory = world.resource::<Inventory>().clone();
        let mut urns: Vec<_> = world.query::<(&forever_below::niches::NichePiece, &Container)>().iter(world).map(|(p, c)| (p.index, inventory.contents_count(c.id))).collect();
        urns.sort();
        urns
    };
    let before = held(&mut app);
    assert!(before.iter().any(|u| u.1 > 0), "an urn with something in it: {before:?}");
    assert!(rob_an_urn(&mut app));
    let after = held(&mut app);
    assert_ne!(before, after, "something came out of an urn");
    let lying = standing(&mut app);

    let (feet, forward) = set_camp_where(&mut app, "Tent", 40_010);
    look_down_at_camp(&mut app, feet, forward, 0.6);
    key(&mut app, KeyCode::KeyF, "f", bevy::input::ButtonState::Pressed);
    app.update();
    assert!(app.world().resource::<Asleep>().0.is_some(), "asleep: {}", app.world().resource::<forever_below::inventory::Toast>().text);
    key(&mut app, KeyCode::KeyF, "f", bevy::input::ButtonState::Released);

    let (mut resumed_app, _) = resumed(&file);
    let mut frames = 0;
    while !resumed_app.world().resource::<forever_below::loading::LayoutTurn>().done() && frames < 60 {
        resumed_app.update();
        frames += 1;
    }
    assert_eq!(held(&mut resumed_app), after, "what was taken from the urn is still gone, and the rest is still there");
    assert_eq!(standing(&mut resumed_app).urns, lying.urns);
}

#[test]
fn a_saved_delve_comes_back_whole_on_three_levels() {
    let (_, at_save, lying, file) = played("whole");
    assert_eq!(at_save.abyss.1, 3, "the delver lay down on the third level");
    assert!(!at_save.other_levels.is_empty() && at_save.other_levels.contains_key(&1) && at_save.other_levels.contains_key(&2), "two levels left behind");
    // Everything the state holds is something, so the comparison below means something.
    assert!(at_save.effects.is_some() && at_save.quickslots[3].is_some() && !at_save.orders.is_empty() && at_save.revives == 1);
    assert!(at_save.character.books_read == vec![77] && at_save.health.hp < at_save.health.max);

    let (mut app, resumed_at) = resumed(&file);
    assert_eq!(resumed_at, at_save);
    // Frames on, the level is laid out as it was left, and the delver lies in the tent.
    let mut frames = 0;
    // (The level is laid out over the frames after the state is put back, a part a frame.)
    while (app.world().resource::<Asleep>().0.is_none() || !app.world().resource::<forever_below::loading::LayoutTurn>().done()) && frames < 60 {
        app.update();
        frames += 1;
    }
    let sleep = app.world().resource::<Asleep>().0.expect("lying in their camp");
    assert_eq!(sleep.camp, fb_items::camp::Camp::Tent);
    assert!(app.world().get::<SetCamp>(sleep.bed).is_some());
    assert!(app.world().get_resource::<forever_below::camp::PendingBed>().is_none());
    let now = standing(&mut app);
    assert_eq!(now.floor.len(), lying.floor.len(), "the floor holds as many things");
    for (a, b) in now.floor.iter().zip(&lying.floor) {
        assert_eq!((a.0, &a.1.base, a.1.seed), (b.0, &b.1.base, b.1.seed), "the same thing lies in the same place");
        if b.1.shape.lit() == Some(true) {
            // A light left burning burns on in the frames since (the world ran on).
            assert_eq!(a.1.shape.lit(), Some(true), "{} is still alight", b.1.base);
            assert!(a.1.fuel_left() <= b.1.fuel_left(), "{} burns on, never back", b.1.base);
            continue;
        }
        let (a_text, b_text) = (format!("{a:?}"), format!("{b:?}"));
        let at = a_text.bytes().zip(b_text.bytes()).position(|(x, y)| x != y).unwrap_or(0);
        let around = |t: &str| t[at.saturating_sub(60)..(at + 80).min(t.len())].to_string();
        assert!(a == b, "{} {} differs near ...{}... then ...{}...", b.1.base, b.1.seed, around(&a_text), around(&b_text));
    }
    assert_eq!(now.camps, lying.camps, "the tent is still pitched");
    assert_eq!(now.containers, lying.containers, "the chests");
    assert_eq!(now.urns, lying.urns, "the urns in the niches, and what was taken from them");
    assert_eq!(now.creatures, lying.creatures, "the creatures, hurt or dead");
    // By now the rats of this level may have come for the sleeper. Let them be, and walk up.
    let delver = player(&mut app);
    let world = app.world_mut();
    let creatures: Vec<Entity> = world.query_filtered::<Entity, With<Critter>>().iter(world).collect();
    for creature in creatures {
        world.despawn(creature);
    }
    let max = world.get::<Health>(delver).unwrap().max;
    world.get_mut::<Health>(delver).unwrap().hp = max;
    world.remove_resource::<forever_below::combat::vitals::Dying>();
    world.remove_resource::<forever_below::combat::revival::Unconscious>();
    world.resource_mut::<Asleep>().0 = None;
    // The levels left behind come back as they were: down the stairs and up again.
    go(&mut app, StairKind::Up);
    assert_eq!(app.world().resource::<Abyss>().depth, 2);
    assert!(standing(&mut app).floor.iter().any(|(_, i)| i.base == "Sack" && i.seed == 40_002), "the sack lies where it was set down");
    go(&mut app, StairKind::Up);
    let level_one = standing(&mut app);
    assert!(level_one.floor.iter().any(|(_, i)| i.base == "Sword" && i.seed == 40_001), "the sword lies where it was dropped");
    assert!(level_one.creatures.iter().any(|c| c.2), "the rat killed is still dead");
    assert!(level_one.containers.iter().any(|c| c.1), "the chest stands open");
}

#[test]
fn a_delve_that_may_not_save_writes_nothing_when_the_delver_lies_down() {
    let file = scratch("unsaveable");
    let mut app = delving(&file);
    app.insert_resource(Saveable(false));
    let (feet, forward) = set_camp_where(&mut app, "Bedroll", 40_011);
    look_down_at_camp(&mut app, feet, forward, 0.6);
    key(&mut app, KeyCode::KeyF, "f", bevy::input::ButtonState::Pressed);
    app.update();
    assert!(app.world().resource::<Asleep>().0.is_some());
    assert!(!file.exists(), "no save was written");
    assert!(app.world().resource::<forever_below::inventory::Toast>().text.starts_with("You lie down"));
}

#[test]
fn a_rested_delver_lies_down_in_a_camp_and_the_delve_is_saved_with_the_nap() {
    let file = scratch("rested_nap");
    let mut app = delving(&file);
    let (feet, forward) = set_camp_where(&mut app, "Bedroll", 40_012);
    look_down_at_camp(&mut app, feet, forward, 0.0);
    key(&mut app, KeyCode::KeyF, "f", bevy::input::ButtonState::Pressed);
    app.update();
    let sleep = app.world().resource::<Asleep>().0.expect("a rested delver lies down in a camp");
    assert!(sleep.nap_until.is_some_and(|end| end > sleep.since), "it is a nap");
    assert_eq!(app.world().resource::<forever_below::inventory::Toast>().text, "Your delve is saved.");
    let saved = forever_below::save::SavedGame::find(&file);
    let delve = saved.delve.expect("saved");
    assert_eq!(delve.nap_until, sleep.nap_until, "the nap's end is in the save");
}

#[test]
fn a_rested_delver_with_something_hostile_near_is_refused_and_nothing_is_saved() {
    let file = scratch("rested_watched");
    let mut app = delving(&file);
    let (feet, forward) = set_camp_where(&mut app, "Bedroll", 40_013);
    look_down_at_camp(&mut app, feet, forward, 0.0);
    let world = app.world_mut();
    let mut creatures = world.query_filtered::<&mut Transform, With<Critter>>();
    let mut moved = false;
    for mut at in creatures.iter_mut(world) {
        at.translation = Vec3::new(feet.x + 1.0, 0.0, feet.y);
        moved = true;
    }
    assert!(moved, "the level has a creature");
    app.update();
    key(&mut app, KeyCode::KeyF, "f", bevy::input::ButtonState::Pressed);
    app.update();
    assert!(app.world().resource::<Asleep>().0.is_none());
    assert_eq!(app.world().resource::<forever_below::inventory::Toast>().text, "You cannot sleep with something so near.");
    assert!(!file.exists(), "nothing was saved");
}

/// Every resource and every component of the delver the game has, and what the save does with
/// it. A new one is not in either list and fails here; say which it is.
#[test]
fn every_resource_and_component_is_accounted_for() {
    // (type name, how the save has it)
    const KEPT: &[(&str, &str)] = &[
        ("travel::Abyss", "run_seed and depth"),
        ("record::WorldRecord", "record: every level, standings, orders, keys, meetings"),
        ("character::Character", "delver (worn is worked out again)"),
        ("inventory::items::Inventory", "pack: what is worn and carried, with the coin purse; the floor is in the record"),
        ("quickslots::Quickslots", "pack.quickslots"),
        ("game_clock::GameClock", "clock"),
        ("combat::revival::Revives", "revives"),
        ("enchanting::Enchanting", "enchanting"),
        ("map::explore::ExploredLevels", "explored"),
        ("camp::Asleep", "camp: the delver lies down in it again on resuming"),
        ("casting::AbilitySlots", "body.ability_slots: the quickslots that hold a prayer"),
        ("casting::Chosen", "body.chosen: the prayer C casts and its tier"),
    ];
    const NOT_KEPT: &[(&str, &str)] = &[
        ("combat::revival::Prayers", "Divine Protection's prayer: a save is written on lying down, and sleep renews it"),
        ("book::Book", "the spellbook screen: open-screen state"),
        ("devotion::Offered", "an open devotion offer: the deed tally offers it again"),
        ("smoke::WispAssets", "the smoke wisp's texture, made when the app starts"),
        // Set again by the delve that starts, from what the save holds.
        ("save::SaveFile", "where the file is"),
        ("save::Saveable", "whether this delve may save: set by the title"),
        ("character::ChosenDelver", "taken up by the delve's start"),
        ("play::CurrentLevel", "rebuilt from the seed, depth and record"),
        ("play::LevelRooms", "mapped from the level"),
        ("loading::LayoutTurn", "counts the frames since the level changed"),
        ("play::BlockedCells", "worked out as the level is laid out"),
        ("play::Obstacles", "gathered each frame"),
        ("doors::Doors", "the level's doors: in the record"),
        ("dice::Dice", "a roll showing: transient screen state"),
        ("doors::PickInFlight", "a lock pick whose dice are showing: transient, and cleared when the delve ends"),
        ("dice::DiceStage", "the roll's render image, made when the app starts"),
        ("identify::Identifying", "an identify roll showing: transient"),
        ("talk::Persuading", "a persuasion roll showing: the talk is not saved"),
        ("staged::Staged", "loot rolled ahead for a level, used once"),
        ("level::PreparedStone", "the level's stone made ahead"),
        ("folk::FolkFigures", "folk figures made ahead"),
        ("travel::Below", "the level below being made"),
        ("travel::Journey", "a journey between levels: none while asleep"),
        ("inventory::bags::BagWindows", "which bags are open as windows: screen state kept for the delve, not saved"),
        ("remains::NearBodies", "the bodies in reach, found again each frame"),
        // The moment.
        ("play::ScreenOpen", "no screen is open to lie down"),
        ("play::SheetTab", "which tab the inventory opens on"),
        ("inventory::Dragging", "nothing is dragged while lying down"),
        ("inventory::Hovered", "the mouse"),
        ("inventory::OpenContainer", "closed to lie down"),
        ("inventory::Toast", "a line of text"),
        ("inventory::underfoot::Underfoot", "what lies near: the floor is in the record"),
        ("inventory::underfoot::FloorChanges", "changes to the floor, applied each frame"),
        ("inventory::ColumnScroll", "screen layout"),
        ("inventory::ShelfScrolled", "screen layout"),
        ("inventory::card::ShownCard", "screen"),
        ("inventory::sheet::HoveredNumber", "screen"),
        ("inventory::icon::IconCache", "icons drawn"),
        ("inventory::doll::DollAssets", "icons and figures drawn"),
        ("inventory::doll::DollStage", "the paper doll's figure"),
        ("fitting::FitCache", "fits made again as needed"),
        ("sparkle::StarMesh", "the star mesh, made at startup"),
        ("barter::Barter", "no barter while lying down; the shelves are in the record"),
        ("barter::BarterRequest", "a request"),
        ("carry::Aimed", "what is under the crosshair"),
        ("interact::CrosshairPrompt", "a line of text"),
        ("doors::AimedDoor", "what is under the crosshair"),
        ("doors::AimedSwitch", "what is under the crosshair"),
        ("folk::AimedFolk", "what is under the crosshair"),
        ("talk::Talk", "no talk while lying down"),
        ("talk::TalkRequest", "a request"),
        ("commission::CommissionScreen", "no screen is open to lie down; the orders are in the record"),
        ("commission::CommissionStage", "the commission screen's figure"),
        ("commission::Turn", "the commission screen's turn"),
        ("enchanter::Bench", "no screen is open to lie down"),
        ("reading::OpenPage", "no screen is open to lie down"),
        ("map::screen::MapView", "the map's view of itself"),
        ("map::screen::PlanImage", "an image"),
        ("map::screen::MarkerImage", "an image"),
        ("status::ShownConditions", "what the HUD shows"),
        ("status::ConditionIcons", "icons"),
        ("status::DebugConditions", "a debug tool"),
        ("save::SavedGame", "found on disk each time the title shows"),
        ("title::ReplacePrompt", "the title's question"),
        ("title::preview::PreviewStage", "the creator's figure"),
        ("tweak_panel::HeldSlider", "a debug tool"),
        ("menu::MenuPage", "the title and pause menu"),
        ("menu::Rebinding", "the menu"),
        ("menu::ControlsGroup", "the menu"),
        ("menu::OpenChoices", "the menu"),
        ("inspect::InspectorOpen", "a debug tool"),
        // Dice and what hunts.
        ("combat::RunDice", "drawn afresh each run, from the clock"),
        ("combat::shoot::Marksman", "a bow drawn: nothing is drawn while lying down"),
        ("combat::vitals::Pain", "the red flash"),
        ("combat::vitals::Dying", "a last gasp: not alive to lie down in it"),
        ("combat::vitals::Fallen", "death: the save is deleted"),
        ("combat::revival::Unconscious", "a choice at the edge of death: none while lying down"),
        ("combat::blood::GaspBlood", "the red edge"),
        ("combat::voice::Grunts", "sounds"),
        ("combat::feedback::CreatureSounds", "sounds"),
        ("combat::swing::SwingSound", "sounds"),
        ("combat::swing::SwingPractice", "a debug tool"),
        ("combat::guard::BlockSound", "sounds"),
        ("folk_regard::LastStriker", "who struck last, a moment"),
        ("eat::HealGlow", "the green flash"),
        ("eat::HealSound", "sounds"),
        // Sounds, music and the picture.
        ("audio::Playlist", "music"),
        ("audio::MusicSettings", "music"),
        ("audio::MusicSchedule", "music"),
        ("audio::PlaylistAudio", "music"),
        ("ambient::RumbleSchedule", "sounds"),
        ("ambient::AmbientSounds", "sounds"),
        ("level::LevelSounds", "sounds"),
        ("footsteps::FootstepSounds", "sounds"),
        ("breath::BreathSounds", "sounds"),
        ("creatures::skeletons::BoneSounds", "sounds"),
        ("folk_pose::WalkInPlace", "folk figures"),
        ("barter::portrait::TraderStage", "the trader's portrait"),
        ("render::item::OutlineMaterials", "materials"),
        ("render::screen_ink::InkStyle", "a setting"),
        // Settings and data.
        ("keymap::Keymap", "settings"),
        ("keymap::ActiveContext", "input"),
        ("keymap::InputCapture", "input"),
        ("keymap::InputHolds", "input"),
        ("keybindings::KeybindingsFile", "settings"),
        ("settings::SettingsFile", "settings"),
        ("settings::GameSettings", "settings"),
        ("rules::GameRules", "data"),
        ("rules::RulesFile", "data"),
        ("tuning::Tuning", "data"),
        ("tuning::TuningFile", "data"),
        ("ui::UiFonts", "fonts"),
        ("title::NewDelveSeed", "the title"),
        ("title::last_delver::LastDelver", "the title's own file"),
        ("title::last_delver::LastDelverFile", "the title's own file"),
        ("title::creator::ShapeMoved", "the creator"),
    ];
    const PLAYER_KEPT: &[&str] = &[
        "Transform", "player::Locomotion", "combat::Health", "character::DelverNeeds", "character::DelverStamina", "character::DelverMana", "exhaustion::Exhaustion", "exhaustion::ExhaustionTimes",
        "character::DelverRecovery", "character::DelverEffects", "casting::DelverSpellbook",
    ];
    const PLAYER_NOT_KEPT: &[(&str, &str)] = &[
        ("play::Player", "the delver itself"),
        ("character::Load", "worked out from the pack"),
        ("character::GearStamina", "worked out from the pack"),
        ("character::GearMana", "worked out from the pack"),
        ("player::GearPace", "worked out from the pack"),
        ("character::DelverVisibility", "worked out from the light and the pace"),
        ("player::Gait", "the step cycle"),
        ("combat::vitals::Shunned", "worked out from what is worn"),
        ("combat::ArmourClass", "worked out from what is worn"),
        ("combat::vitals::Constricted", "a snake's hold: broken by lying down"),
        ("combat::swing::Swing", "a swing"),
        ("combat::guard::Guard", "a guard"),
        ("player::Slowed", "a moment"),
        ("footsteps::StepState", "sounds"),
        ("kindle::Held", "the light in hand, worked out from the pack"),
    ];

    // The whole flow has been played (levels visited, a tent pitched, poisoned, asleep), so what
    // only exists then is in the world too. The save it made is one versioned file, alone.
    let (mut app, _, _, file) = played("accounted");
    let names: Vec<String> = std::fs::read_dir(file.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec![save::FILE_NAME.to_string()], "no temporary file is left behind");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.starts_with(&format!("{{\"version\":{}", save::VERSION)), "the version comes first: {}", &text[..40]);
    let delver = player(&mut app);
    let world = app.world_mut();
    let short = |name: &str| -> String {
        name.strip_prefix("forever_below::").map_or_else(|| name.to_string(), str::to_string)
    };
    let resources: BTreeSet<String> = world.iter_resources().map(|(_, info, _)| info.name().to_string()).filter(|n| n.starts_with("forever_below::")).map(|n| short(&n)).collect();
    let known: BTreeSet<&str> = KEPT.iter().map(|(n, _)| *n).chain(NOT_KEPT.iter().map(|(n, _)| *n)).collect();
    let unknown: Vec<&String> = resources.iter().filter(|n| !known.contains(n.as_str())).collect();
    assert!(unknown.is_empty(), "resources the save has not decided about (add each to KEPT or NOT_KEPT): {unknown:#?}");

    let components: BTreeSet<String> = world
        .inspect_entity(delver)
        .unwrap()
        .map(|(_, info)| info.name().to_string())
        .map(|n| if n.ends_with("::Transform") { "Transform".to_string() } else { short(&n) })
        .filter(|n| !n.starts_with("bevy_") && !n.starts_with("alloc::"))
        .collect();
    let known: BTreeSet<&str> = PLAYER_KEPT.iter().copied().chain(PLAYER_NOT_KEPT.iter().map(|(n, _)| *n)).collect();
    let unknown: Vec<&String> = components.iter().filter(|n| !known.contains(n.as_str())).collect();
    assert!(unknown.is_empty(), "components of the delver the save has not decided about (add each to PLAYER_KEPT or PLAYER_NOT_KEPT): {unknown:#?}");

    // And the other way: a name in a list that is no longer a type of the game is a stale entry.
    // (What exists only when it happens, a fall or a journey, or only in the real game's asset
    // server, is registered by then too, except these.)
    const ONLY_WHEN_IT_HAPPENS: &[&str] = &["combat::vitals::Fallen", "combat::vitals::Dying", "combat::revival::Unconscious", "travel::Journey", "travel::Below", "combat::shoot::Marksman", "combat::vitals::Constricted", "combat::swing::Swing", "combat::guard::Guard", "player::Slowed", "kindle::Held", "rules::RulesFile"];
    let registered: BTreeSet<String> = world.components().iter_registered().map(|(_, info)| short(&info.name().to_string())).collect();
    let listed = KEPT.iter().map(|(n, _)| *n).chain(NOT_KEPT.iter().map(|(n, _)| *n)).chain(PLAYER_KEPT.iter().copied()).chain(PLAYER_NOT_KEPT.iter().map(|(n, _)| *n));
    let stale: Vec<&str> = listed.filter(|n| *n != "Transform" && !ONLY_WHEN_IT_HAPPENS.contains(n) && !registered.contains(*n)).collect();
    assert!(stale.is_empty(), "names in the lists that are not types of the game: {stale:#?}");
}

#[test]
fn dying_deletes_the_save_and_the_title_offers_no_resume() {
    let (mut app, _, _, file) = played("death");
    assert!(file.exists());
    // Walked on and fell, with every revive spent.
    app.insert_resource(Revives(u32::MAX));
    app.insert_resource(forever_below::combat::revival::Prayers(u32::MAX));
    let body = player(&mut app);
    app.world_mut().get_mut::<Health>(body).unwrap().hp = 0;
    // The save goes at the lethal blow, in the last gasp, before the fall.
    run(&mut app, 10);
    assert!(app.world().get_resource::<forever_below::combat::vitals::Dying>().is_some() && app.world().get_resource::<forever_below::combat::vitals::Fallen>().is_none());
    assert!(!file.exists(), "deleted in the last gasp, so quitting then cannot keep it");
    run(&mut app, ((LAST_GASP + FALL_TIME) * 60.0) as usize + 60);
    assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Title);
    assert!(!file.exists(), "death deleted the save");
    assert!(app.world().resource::<SavedGame>().delve.is_none(), "so the title has none to resume");
}

#[test]
fn an_unreadable_save_is_set_aside_and_the_title_says_so() {
    let file = scratch("garbled");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "{\"version\":1,\"run_seed\":12").unwrap();
    let mut app = headless_game(&file);
    run(&mut app, 30);
    let found = app.world().resource::<SavedGame>();
    assert!(found.delve.is_none());
    assert_eq!(found.unreadable, Some(save::SET_ASIDE_LINE));
    assert!(!file.exists() && file.with_file_name(save::ASIDE_NAME).exists());
    assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Title, "no panic, the title stands");
}
