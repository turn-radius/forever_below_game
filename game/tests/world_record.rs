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
        vitals::{FALL_TIME, LAST_GASP},
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
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                render_creation: WgpuSettings {
                    backends: None,
                    ..default()
                }
                .into(),
                ..default()
            })
            .set(AssetPlugin {
                file_path: forever_below::tuning::asset_file_path(),
                ..default()
            })
            .disable::<WinitPlugin>(),
    )
    .insert_resource(TimeUpdateStrategy::ManualDuration(
        std::time::Duration::from_secs_f32(1.0 / 60.0),
    ))
    .insert_state(AppState::Title)
    .add_plugins((
        SettingsPlugin,
        TuningPlugin,
        ArmoryRenderPlugin,
        PlayPlugin,
        TitlePlugin,
    ))
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
    app.world_mut()
        .write_message(MenuCommand::Creator(CreatorCommand::Begin));
    run(&mut app, 90);
    assert_eq!(
        *app.world().resource::<State<AppState>>().get(),
        AppState::Delving
    );
    run(&mut app, 60);
    app
}

fn level(app: &App) -> &Level {
    &app.world().resource::<CurrentLevel>().0
}

fn player(app: &mut App) -> Entity {
    let world = app.world_mut();
    world
        .query_filtered::<Entity, With<Player>>()
        .single(world)
        .expect("one delver")
}

fn depth(app: &App) -> u32 {
    app.world().resource::<Abyss>().depth
}

fn go(app: &mut App, by: StairKind) {
    let from = depth(app);
    app.world_mut().write_message(UseStair(by));
    run(app, 200);
    assert!(
        app.world().get_resource::<Journey>().is_none(),
        "the journey is over"
    );
    assert_eq!(
        depth(app),
        if by == StairKind::Down {
            from + 1
        } else {
            from - 1
        }
    );
}

/// Down the stair and straight back up, ending on the same level.
fn round_trip(app: &mut App) {
    go(app, StairKind::Down);
    go(app, StairKind::Up);
}

/// Every item lying on the floor, with its spot.
fn lying(app: &mut App) -> Vec<(ItemSpot, fb_items::Item)> {
    let world = app.world_mut();
    let floor: Vec<_> = world
        .query::<&FloorItem>()
        .iter(world)
        .map(|f| (f.id, f.spot))
        .collect();
    let inventory = world.resource::<Inventory>();
    floor
        .into_iter()
        .map(|(id, spot)| {
            (
                spot,
                inventory
                    .item(id)
                    .expect("a floor item is in the inventory")
                    .clone(),
            )
        })
        .collect()
}

/// How many items of this base and seed the inventory keeps track of, anywhere.
fn kept(app: &App, base: &str, seed: u32) -> usize {
    let inventory = app.world().resource::<Inventory>();
    inventory
        .ids()
        .filter(|&id| {
            inventory
                .item(id)
                .is_some_and(|i| i.base == base && i.seed == seed)
        })
        .count()
}

fn set_down_at(app: &mut App, id: ItemId, spot: Vec2, yaw: f32) {
    app.world_mut()
        .resource_mut::<FloorChanges>()
        .0
        .push(FloorChange::SetDownAt(id, spot, yaw));
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
    let sword = app
        .world_mut()
        .resource_mut::<Inventory>()
        .add(fb_items::roll_item("Sword", 40_001).unwrap());
    set_down_at(&mut app, sword, at, 0.8);
    assert_eq!(kept(&app, "Sword", 40_001), 1);
    let mut before = lying(&mut app);
    by_spot(&mut before);
    assert!(
        before
            .iter()
            .any(|(spot, item)| item.seed == 40_001
                && (spot.x, spot.y, spot.yaw) == (at.x, at.y, 0.8))
    );
    let total = app.world().resource::<Inventory>().ids().count();
    for trip in 0..3 {
        round_trip(&mut app);
        let mut now = lying(&mut app);
        by_spot(&mut now);
        assert_eq!(
            now, before,
            "trip {trip}: the floor is as it was left, item for item"
        );
        assert_eq!(
            kept(&app, "Sword", 40_001),
            1,
            "trip {trip}: one sword, not two"
        );
        assert_eq!(
            app.world().resource::<Inventory>().ids().count(),
            total,
            "trip {trip}: nothing made or lost by travelling"
        );
    }
}

/// The level below is made ahead of time on the way down; coming back to it after a visit is
/// from the record, which the loot rolled ahead must not override.
#[test]
fn an_item_dropped_on_the_level_below_is_still_there_after_up_and_down_again() {
    let mut app = delving();
    go(&mut app, StairKind::Down);
    assert!(
        app.world()
            .resource::<forever_below::staged::Staged>()
            .taken
            > 0,
        "the first visit used what was made ahead"
    );
    let at = start_cell(&app);
    let sword = app
        .world_mut()
        .resource_mut::<Inventory>()
        .add(fb_items::roll_item("Sword", 40_002).unwrap());
    set_down_at(&mut app, sword, at, 0.3);
    // (What burns, a lit lantern, burns on while its level is left, so what is compared is
    // what lies where: each thing's base and seed, and the sword in full.)
    let floor = |app: &mut App| {
        let mut items = lying(app);
        by_spot(&mut items);
        items
            .into_iter()
            .map(|(spot, item)| (spot, item.base.clone(), item.seed))
            .collect::<Vec<_>>()
    };
    let before = floor(&mut app);
    let total = app.world().resource::<Inventory>().ids().count();
    for trip in 0..2 {
        go(&mut app, StairKind::Up);
        go(&mut app, StairKind::Down);
        assert_eq!(
            floor(&mut app),
            before,
            "trip {trip}: the floor is the record, not loot rolled again"
        );
        assert_eq!(kept(&app, "Sword", 40_002), 1, "trip {trip}: one sword");
        assert_eq!(
            app.world().resource::<Inventory>().ids().count(),
            total,
            "trip {trip}: nothing made or lost"
        );
    }
}

/// Sets `base` from the pack down in front of the delver at the level's start, as the inventory
/// screen's F does, and lets the floor change land.
fn set_camp(app: &mut App, base: &str, seed: u32) -> ItemId {
    set_camp_where(app, base, seed).0
}

/// [`set_camp`], and where the delver stood to do it and the way they faced.
fn set_camp_where(app: &mut App, base: &str, seed: u32) -> (ItemId, Vec2, Vec2) {
    let lvl = level(app).clone();
    let blocked = app
        .world()
        .resource::<forever_below::play::BlockedCells>()
        .clone();
    let obstacles = app
        .world()
        .resource::<forever_below::play::Obstacles>()
        .0
        .clone();
    // The first open floor where this camp, as `lay_out` would set it, is taken.
    let set = fb_items::camp::set_up(&fb_items::roll_item(base, seed).unwrap()).unwrap();
    let (half, offset) = forever_below::camp::footprint(&set).unwrap();
    let gap = forever_below::camp::gap_for(set.camp().unwrap());
    let (feet, forward) = (0..lvl.height as i32)
        .flat_map(|y| (0..lvl.width as i32).map(move |x| (x, y)))
        .filter(|&(x, y)| lvl.cell(x, y) == fb_level::Cell::Floor)
        .flat_map(|(x, y)| {
            [
                fb_level::Facing::North,
                fb_level::Facing::East,
                fb_level::Facing::South,
                fb_level::Facing::West,
            ]
            .map(move |f| (x, y, f))
        })
        .map(|(x, y, f)| {
            (
                Vec2::new(x as f32 + 0.5, y as f32 + 0.5) * fb_level::CELL,
                Vec2::new(f.step().0 as f32, f.step().1 as f32),
            )
        })
        .find(|&(feet, forward)| {
            let site = forever_below::camp::site_ahead(feet, forward, half, gap);
            let canvas =
                fb_items::camp::canvas_half(&set.shape).map(|(x, z)| forever_below::camp::Site {
                    centre: site.centre - site.to_floor(offset),
                    half: Vec2::new(x, z),
                    yaw: site.yaw,
                });
            forever_below::camp::check_site(&site, &lvl, &blocked, &obstacles, &[]).is_ok()
                && canvas.is_none_or(|c| {
                    forever_below::camp::check_passage(&c, &lvl, &blocked, &obstacles, 3.5).is_ok()
                })
        })
        .expect("somewhere in the level a camp fits");
    let id = app
        .world_mut()
        .resource_mut::<Inventory>()
        .add(fb_items::roll_item(base, seed).unwrap());
    let mut changes = FloorChanges::default();
    let said = forever_below::camp::lay_out(
        &mut app.world_mut().resource_mut::<Inventory>(),
        &mut changes,
        id,
        feet,
        forward,
        &lvl,
        &blocked,
        &obstacles,
        &[],
        3.5,
    );
    assert!(said.is_ok(), "{base}: {said:?}");
    app.world_mut()
        .resource_mut::<FloorChanges>()
        .0
        .extend(changes.0);
    run(app, 5);
    (id, feet, forward)
}

/// The set camps lying in the level: what each is, and whether it blocks the way.
fn camps(app: &mut App) -> Vec<(fb_items::camp::Camp, bool)> {
    let world = app.world_mut();
    let found: Vec<_> = world
        .query::<(&forever_below::camp::SetCamp, &FloorItem, Option<&Children>)>()
        .iter(world)
        .map(|(c, _, kids)| (c.camp, kids.map_or(vec![], |k| k.to_vec())))
        .collect();
    found
        .into_iter()
        .map(|(camp, kids)| {
            (
                camp,
                kids.into_iter()
                    .any(|k| world.get::<forever_below::play::BlocksTheWay>(k).is_some()),
            )
        })
        .collect()
}

#[test]
fn a_set_bedroll_and_a_pitched_tent_are_still_set_after_leaving_and_returning() {
    for (base, blocks) in [("Bedroll", false), ("Tent", true)] {
        let mut app = delving();
        let id = set_camp(&mut app, base, 40_010);
        let wanted = app
            .world()
            .resource::<Inventory>()
            .item(id)
            .unwrap()
            .clone();
        assert!(wanted.is_set_camp(), "{base}: set down");
        assert_eq!(
            camps(&mut app).len(),
            1,
            "{base}: lying in the level as a set camp"
        );
        assert_eq!(
            camps(&mut app)[0].1,
            blocks,
            "{base}: blocks the way only if a tent"
        );
        let floor = |app: &mut App| {
            lying(app)
                .into_iter()
                .find(|(_, i)| i.seed == 40_010 && i.base == base)
                .expect("it lies in the level")
        };
        let (spot, _) = floor(&mut app);
        for trip in 0..1 {
            round_trip(&mut app);
            let (now, item) = floor(&mut app);
            assert_eq!(now, spot, "{base} trip {trip}: where it was set");
            assert_eq!(
                item, wanted,
                "{base} trip {trip}: the same item, still set down"
            );
            assert_eq!(
                camps(&mut app),
                vec![(wanted.camp().unwrap(), blocks)],
                "{base} trip {trip}: one set camp, not two"
            );
            assert_eq!(
                kept(&app, base, 40_010),
                1,
                "{base} trip {trip}: one, not two"
            );
        }
    }
}

/// Puts the delver back where they stood to set a camp down, facing the way they faced and
/// looking down at it, with nothing hostile near and `sleepiness` on the meter. The headless game
/// has no mouse to turn the view, so the view is turned here.
fn look_down_at_camp(app: &mut App, feet: Vec2, forward: Vec2, sleepiness: f64) {
    let delver = player(app);
    {
        let mut world = app.world_mut().entity_mut(delver);
        world.get_mut::<Transform>().unwrap().translation = Vec3::new(feet.x, 0.0, feet.y);
        let yaw = (-forward.x).atan2(-forward.y);
        world.get_mut::<Transform>().unwrap().rotation = Quat::from_rotation_y(yaw);
        let mut walk = world
            .get_mut::<forever_below::player::Locomotion>()
            .unwrap();
        walk.yaw = yaw;
        walk.pitch = -60f32.to_radians();
        walk.velocity = Vec2::ZERO;
        world
            .get_mut::<forever_below::character::DelverNeeds>()
            .unwrap()
            .0
            .sleepiness
            .value = sleepiness;
    }
    let world = app.world_mut();
    let creatures: Vec<Entity> = world
        .query_filtered::<Entity, With<Critter>>()
        .iter(world)
        .collect();
    for creature in creatures {
        world.despawn(creature);
    }
    let mut eyes =
        world.query_filtered::<&mut Transform, With<forever_below::play::PlayerCamera>>();
    for mut eye in eyes.iter_mut(world) {
        eye.rotation = Quat::from_rotation_x(-60f32.to_radians());
    }
    run(app, 5);
    let aimed = app.world().resource::<forever_below::carry::Aimed>().0;
    assert!(
        aimed.is_some_and(|e| app.world().get::<forever_below::camp::SetCamp>(e).is_some()),
        "the camp is under the crosshair: {aimed:?}"
    );
}

/// Types `key` into the window: pressed, then released.
fn type_key(app: &mut App, key_code: KeyCode, text: &str) {
    for state in [
        bevy::input::ButtonState::Pressed,
        bevy::input::ButtonState::Released,
    ] {
        app.world_mut()
            .write_message(bevy::input::keyboard::KeyboardInput {
                key_code,
                logical_key: bevy::input::keyboard::Key::Character(text.into()),
                state,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
        app.update();
    }
}

/// Sleeping in a set bedroll with the whole game running: aimed at from where it was set, F lies
/// the delver down, the clock runs at the sleeping pace, sleepiness falls and they wake rested.
#[test]
fn a_delver_sleeps_in_a_set_bedroll_until_rested() {
    let mut app = delving();
    let (_, feet, forward) = set_camp_where(&mut app, "Bedroll", 40_011);
    // Tired enough to sleep, and little enough to sleep it off in a few seconds.
    look_down_at_camp(&mut app, feet, forward, 0.3);
    let delver = player(&mut app);
    let hours = |app: &App| {
        app.world()
            .resource::<forever_below::game_clock::GameClock>()
            .time
            .hours()
    };
    let before = hours(&app);
    type_key(&mut app, KeyCode::KeyF, "f");
    assert!(
        app.world()
            .resource::<forever_below::camp::Asleep>()
            .0
            .is_some(),
        "asleep: {}",
        app.world()
            .resource::<forever_below::inventory::Toast>()
            .text
    );
    // 0.3 of the meter is 2.4 game hours at eight to rest, a few seconds at 900 to one.
    run(&mut app, 900);
    let asleep = app.world().resource::<forever_below::camp::Asleep>().0;
    assert_eq!(asleep, None, "woken within fifteen seconds");
    let slept = hours(&app) - before;
    assert!((slept - 2.4).abs() < 0.3, "slept {slept} game hours");
    let sleepiness = app
        .world()
        .get::<forever_below::character::DelverNeeds>(delver)
        .unwrap()
        .0
        .sleepiness
        .value;
    assert!(
        sleepiness < 0.01,
        "rested (and a hair tired again by the frames since): {sleepiness}"
    );
    assert!(
        app.world()
            .resource::<forever_below::inventory::Toast>()
            .text
            .starts_with("You wake rested."),
        "{}",
        app.world()
            .resource::<forever_below::inventory::Toast>()
            .text
    );
}

/// Packing a set camp up with the whole game running: G on it lifts it off the floor into the
/// pack as the same item, and it stays gone when the delver leaves and returns. (The delver's own
/// small pack has room for a bedroll's roll; a tent's is too big for it, and is refused.)
#[test]
fn a_set_bedroll_is_packed_up_as_the_same_item_and_stays_gone() {
    let mut app = delving();
    let carried = fb_items::roll_item("Bedroll", 40_012).unwrap();
    let (id, feet, forward) = set_camp_where(&mut app, "Bedroll", 40_012);
    look_down_at_camp(&mut app, feet, forward, 0.0);
    type_key(&mut app, KeyCode::KeyG, "g");
    run(&mut app, 5);
    let inventory = app.world().resource::<Inventory>();
    assert_eq!(
        inventory.item(id),
        Some(&carried),
        "the bedroll it was, rolled up: {}",
        app.world()
            .resource::<forever_below::inventory::Toast>()
            .text
    );
    assert!(inventory.is_carried(id), "in the pack");
    let gone = |app: &mut App| {
        camps(app).is_empty()
            && lying(app)
                .iter()
                .all(|(_, i)| !(i.base == "Bedroll" && i.seed == 40_012))
    };
    assert!(gone(&mut app), "no longer lying in the level");
    round_trip(&mut app);
    assert!(gone(&mut app), "still gone on return");
    assert_eq!(kept(&app, "Bedroll", 40_012), 1, "one bedroll, in the pack");
}

/// A pack with no room refuses a tent, which stays pitched where it is.
#[test]
fn a_set_tent_too_big_for_the_pack_stays_pitched() {
    let mut app = delving();
    let (id, feet, forward) = set_camp_where(&mut app, "Tent", 40_013);
    // The pack filled with rope, until nothing more goes in.
    {
        let mut inventory = app.world_mut().resource_mut::<Inventory>();
        for n in 0..200 {
            let rope = inventory.add(fb_items::roll_item("Rope", 900 + n).unwrap());
            if inventory.stow(rope).is_err() {
                inventory.forget(rope);
                break;
            }
        }
    }
    look_down_at_camp(&mut app, feet, forward, 0.0);
    type_key(&mut app, KeyCode::KeyG, "g");
    run(&mut app, 5);
    assert_eq!(
        app.world()
            .resource::<forever_below::inventory::Toast>()
            .text,
        "There is no room in your pack for it."
    );
    assert!(
        app.world()
            .resource::<Inventory>()
            .item(id)
            .unwrap()
            .is_set_camp(),
        "still pitched"
    );
    assert_eq!(camps(&mut app).len(), 1, "still standing in the level");
}

#[test]
fn a_torch_burnt_on_the_floor_is_as_burnt_when_the_delver_returns() {
    let mut app = delving();
    let at = start_cell(&app);
    let torch = lit_torch(7);
    let seed = torch.seed;
    let id = app
        .world_mut()
        .resource_mut::<Inventory>()
        .add(torch.clone());
    set_down_at(&mut app, id, at, 0.0);
    // Burn it down a little where it lies, then leave.
    app.world_mut()
        .resource_mut::<Inventory>()
        .burn_lights(600.0);
    let burnt = lying(&mut app)
        .into_iter()
        .find(|(_, i)| i.seed == seed)
        .unwrap()
        .1;
    assert_ne!(burnt.shape, torch.shape, "it has burnt");
    round_trip(&mut app);
    let after = lying(&mut app)
        .into_iter()
        .find(|(_, i)| i.seed == seed)
        .expect("the torch lies where it was left")
        .1;
    // (It goes on burning while the delver is on the level, so it is as burnt as when they left,
    // which is no fresher than when it was looked at.)
    let (fresh, then, now) = (
        torch.fuel_left().unwrap(),
        burnt.fuel_left().unwrap(),
        after.fuel_left().unwrap(),
    );
    assert!(
        now <= then && then < fresh - 500.0,
        "burnt as it was left, not fresh: {fresh} then {then} now {now}"
    );
}

#[test]
fn what_was_taken_stays_gone_and_a_level_never_visited_is_as_generated() {
    let mut app = delving();
    let abyss = *app.world().resource::<Abyss>();
    assert!(
        app.world().resource::<WorldRecord>().levels.is_empty(),
        "nothing is recorded until a level is left"
    );
    go(&mut app, StairKind::Down);
    assert_eq!(
        *level(&app),
        abyss.level_at(2),
        "a level never visited is exactly its generated state"
    );
    let record = app.world().resource::<WorldRecord>();
    assert!(record.level(1).is_some() && record.level(2).is_none());
    let generated = lying(&mut app).len();
    assert!(generated > 0, "level 2 has items");
    // Take up the first thing that fits.
    let ids: Vec<_> = {
        let world = app.world_mut();
        world
            .query::<&FloorItem>()
            .iter(world)
            .map(|f| f.id)
            .collect()
    };
    let taken = ids
        .into_iter()
        .find(|&id| {
            app.world_mut()
                .resource_mut::<Inventory>()
                .pick_up(id)
                .is_ok()
        })
        .expect("something could be picked up");
    app.world_mut()
        .resource_mut::<FloorChanges>()
        .0
        .push(FloorChange::Lifted(taken));
    run(&mut app, 5);
    let (base, seed) = {
        let item = app.world().resource::<Inventory>().item(taken).unwrap();
        (item.base.clone(), item.seed)
    };
    assert_eq!(lying(&mut app).len(), generated - 1);
    for _ in 0..2 {
        go(&mut app, StairKind::Up);
        go(&mut app, StairKind::Down);
        assert_eq!(
            lying(&mut app).len(),
            generated - 1,
            "it is gone from the floor"
        );
        assert!(
            lying(&mut app)
                .iter()
                .all(|(_, i)| !(i.base == base && i.seed == seed))
        );
        assert_eq!(kept(&app, &base, seed), 1, "and it is the delver's, once");
    }
}

#[test]
fn a_creature_killed_stays_dead_and_one_hurt_stays_hurt() {
    let mut app = delving();
    let mut critters: Vec<(Entity, CreatureSpot)> = {
        let world = app.world_mut();
        world
            .query::<(Entity, &Critter)>()
            .iter(world)
            .map(|(e, c)| (e, c.spot))
            .collect()
    };
    assert!(
        critters.len() >= 2,
        "level 1 of 1207 has creatures to fight"
    );
    critters.sort_by_key(|(_, spot)| spot.seed);
    let (dead, dead_spot) = critters[0];
    let (hurt, hurt_spot) = critters[1];
    app.world_mut().get_mut::<Health>(hurt).unwrap().hp -= 1;
    let hp = app.world().get::<Health>(hurt).unwrap().hp;
    app.world_mut().write_message(Struck {
        target: dead,
        damage: 10_000,
        from: Vec2::ZERO,
        bludgeoning: false,
        critical: false,
        by_ally: false,
        graze: false,
    });
    run(&mut app, 60);
    assert!(app.world().get::<Corpse>(dead).is_some());
    for _ in 0..2 {
        round_trip(&mut app);
        let world = app.world_mut();
        let now: Vec<_> = world
            .query::<(&Critter, &Health, Has<Corpse>)>()
            .iter(world)
            .map(|(c, h, dead)| (c.spot, *h, dead))
            .collect();
        let (_, health, is_dead) = *now
            .iter()
            .find(|(s, _, _)| *s == dead_spot)
            .expect("its body is still there");
        assert!(is_dead && health.hp == 0, "a rat killed stays dead");
        let (_, health, is_dead) = *now.iter().find(|(s, _, _)| *s == hurt_spot).unwrap();
        assert!(
            !is_dead && health.hp == hp,
            "a creature hurt stays as hurt: {} not {hp}",
            health.hp
        );
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
        let free = inventory
            .grid(sack)
            .unwrap()
            .first_free(inventory.footprint(dagger).unwrap())
            .unwrap();
        inventory.put(dagger, sack, free).unwrap();
        sack
    };
    set_down_at(&mut app, sack, at, 0.0);
    for _ in 0..2 {
        round_trip(&mut app);
        let world = app.world_mut();
        let ids: Vec<_> = world
            .query::<&FloorItem>()
            .iter(world)
            .map(|f| f.id)
            .collect();
        let inventory = world.resource::<Inventory>();
        let bag = ids
            .into_iter()
            .find(|&id| inventory.item(id).is_some_and(|i| i.seed == 40_002))
            .expect("the sack lies where it was set down");
        let inside: Vec<_> = inventory
            .grid(bag)
            .unwrap()
            .entries()
            .iter()
            .map(|e| inventory.item(e.id).map(|i| (i.base.clone(), i.seed)))
            .collect();
        assert_eq!(
            inside,
            vec![Some(("Dagger".to_string(), 40_003))],
            "the bag keeps what was in it"
        );
        assert_eq!(kept(&app, "Dagger", 40_003), 1);
    }
}

#[test]
fn an_opened_chest_stays_open_with_what_is_left_in_it() {
    let mut app = delving();
    let (index, entity, container) = {
        let world = app.world_mut();
        let mut all: Vec<_> = world
            .query::<(Entity, &PropPiece, &Container)>()
            .iter(world)
            .map(|(e, p, c)| (p.index, e, *c))
            .collect();
        all.sort_by_key(|a| a.0);
        let inventory = world.resource::<Inventory>();
        all.into_iter()
            .find(|(_, _, c)| inventory.contents_count(c.id) >= 2)
            .expect("a container with something in it")
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
                open_container(
                    &mut commands,
                    &mut spawner,
                    &level.0,
                    entity,
                    pieces.get(entity).unwrap(),
                    container,
                    &mut open,
                    &mut showing,
                    None,
                    &tuning,
                );
            },
        )
        .unwrap();
    run(&mut app, 5);
    let taken = app
        .world()
        .resource::<Inventory>()
        .grid(container.id)
        .unwrap()
        .entries()[0]
        .id;
    let taken_seed = app
        .world()
        .resource::<Inventory>()
        .item(taken)
        .unwrap()
        .seed;
    app.world_mut().resource_mut::<Inventory>().stow(taken).ok();
    let left_in = |app: &App, id: ItemId| -> Vec<(String, u32)> {
        let inventory = app.world().resource::<Inventory>();
        inventory
            .grid(id)
            .unwrap()
            .entries()
            .iter()
            .map(|e| {
                (
                    inventory.item(e.id).unwrap().base.clone(),
                    inventory.item(e.id).unwrap().seed,
                )
            })
            .collect()
    };
    let remaining = left_in(&app, container.id);
    assert!(
        !remaining.iter().any(|(_, s)| *s == taken_seed),
        "one thing was taken out"
    );
    for _ in 0..2 {
        round_trip(&mut app);
        let world = app.world_mut();
        let now = world
            .query::<(&PropPiece, &Container)>()
            .iter(world)
            .find(|(p, _)| p.index == index)
            .map(|(_, c)| *c)
            .expect("the chest stands where it did");
        assert!(now.opened, "it stands open");
        assert_eq!(left_in(&app, now.id), remaining, "with what was left in it");
    }
}

#[test]
fn the_world_record_round_trips_through_serde_and_death_resets_it() {
    let mut app = delving();
    let at = start_cell(&app);
    let sword = app
        .world_mut()
        .resource_mut::<Inventory>()
        .add(fb_items::roll_item("Sword", 40_001).unwrap());
    set_down_at(&mut app, sword, at, 0.8);
    round_trip(&mut app);
    let record = app.world().resource::<WorldRecord>().clone();
    assert!(
        record
            .level(1)
            .is_some_and(|l| !l.floor.is_empty() && !l.creatures.is_empty())
    );
    let json = serde_json::to_string(&record).unwrap();
    assert_eq!(serde_json::from_str::<WorldRecord>(&json).unwrap(), record);

    // Fall, with every revive spent: the next delve starts with a clean record, and level 1 as
    // generated.
    app.insert_resource(forever_below::combat::revival::Revives(u32::MAX));
    let body = player(&mut app);
    app.world_mut().get_mut::<Health>(body).unwrap().hp = 0;
    run(&mut app, ((LAST_GASP + FALL_TIME) * 60.0) as usize + 60);
    assert_eq!(
        *app.world().resource::<State<AppState>>().get(),
        AppState::Title
    );
    app.world_mut()
        .write_message(MenuCommand::Creator(CreatorCommand::Begin));
    run(&mut app, 90);
    assert_eq!(
        *app.world().resource::<State<AppState>>().get(),
        AppState::Delving
    );
    run(&mut app, 30);
    // A fresh delve records no level yet, has met no one, and starts every faction's standing
    // afresh from the delver's alignment (#542).
    let fresh = app.world().resource::<WorldRecord>();
    assert!(fresh.levels.is_empty());
    assert_eq!(fresh.standings.met().count(), 0);
    let generated = fb_level::generate(1207, 1);
    assert_eq!(*level(&app), generated);
    assert!(
        lying(&mut app)
            .iter()
            .all(|(spot, _)| generated.items.contains(spot)),
        "level 1 is as generated again"
    );
    assert_eq!(
        kept(&app, "Sword", 40_001),
        0,
        "the dropped sword is not in the new run"
    );
}
