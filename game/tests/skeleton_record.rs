//! Skeletons and the world record (#291): a skeleton killed lies bare when its level is come
//! back to, its gear on the floor exactly once; one that lived comes back in its gear.
//!
//! The whole game runs here without a window or a GPU, as in `world_record.rs`, stepped a 60th
//! of a second a frame, from the title through New delve into level 1207 and down the stair to
//! the stone halls of depth 2.

use bevy::{
    prelude::*,
    render::{RenderPlugin, settings::WgpuSettings},
    time::TimeUpdateStrategy,
    window::ExitCondition,
    winit::WinitPlugin,
};
use fb_level::{CreatureSpot, Level, StairKind};
use forever_below::{
    carry::FloorItem,
    combat::{
        Health,
        feedback::{Corpse, Struck},
    },
    creatures::Critter,
    inventory::{Inventory, ItemId},
    launch::AppState,
    menu::MenuCommand,
    play::{CurrentLevel, PlayPlugin, Player},
    record::WorldRecord,
    render::ArmoryRenderPlugin,
    settings::SettingsPlugin,
    title::{NewDelveSeed, TitlePlugin, creator::CreatorCommand},
    travel::{Abyss, Journey, UseStair},
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

fn run(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
    }
}

/// A new delve in level 1207, a second in. The delver is made too hardy for the level's rats to
/// down between stairs, which would stop the round trips these tests take: what bites where
/// moves whenever a level's layout or furnishing does.
fn delving() -> App {
    let mut app = headless_game();
    run(&mut app, 30);
    app.world_mut().write_message(MenuCommand::Creator(CreatorCommand::Begin));
    run(&mut app, 90);
    assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Delving);
    let world = app.world_mut();
    let player = world.query_filtered::<Entity, With<Player>>().single(world).expect("one delver");
    *world.get_mut::<Health>(player).expect("the delver has hit points") = Health::full(10_000);
    run(&mut app, 60);
    app
}

fn level(app: &App) -> &Level {
    &app.world().resource::<CurrentLevel>().0
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

use forever_below::creatures::{
    Bones,
    skeletons::{Looted, WornGear},
};

/// Down the stair to the stone halls of depth 2, where the skeletons stand.
fn in_the_halls() -> App {
    let mut app = delving();
    go(&mut app, StairKind::Down);
    run(&mut app, 30);
    app
}

/// The skeleton standing where `spot` is, if any.
fn skeleton_at(app: &mut App, spot: CreatureSpot) -> Option<Entity> {
    let world = app.world_mut();
    world.query::<(Entity, &Critter)>().iter(world).find(|(_, c)| c.spot == spot).map(|(e, _)| e)
}

/// A skeleton of the level that wears or holds something, the first by seed (the first halls' dead
/// wear little, `fb_items::undead::FIRST_HALLS`).
fn a_kitted_skeleton(app: &mut App) -> (CreatureSpot, Vec<(String, u32)>) {
    let world = app.world_mut();
    let mut kitted: Vec<(CreatureSpot, Vec<(String, u32)>)> = world
        .query::<&Critter>()
        .iter(world)
        .filter(|c| c.spot.species.is_skeleton())
        .filter_map(|c| {
            let kit = c.kit.as_ref()?;
            let worn: Vec<(String, u32)> = kit.outfit.worn.iter().map(|(_, i)| (i.base.to_string(), i.seed)).collect();
            (!worn.is_empty()).then_some((c.spot, worn))
        })
        .collect();
    kitted.sort_by_key(|(spot, _)| spot.seed);
    kitted.into_iter().next().expect("the stone halls have a skeleton with gear")
}

/// How many of `gear` lie on the floor, counting each piece.
fn gear_on_the_floor(app: &mut App, gear: &[(String, u32)]) -> usize {
    let lying: Vec<(String, u32)> = {
        let world = app.world_mut();
        let floor: Vec<ItemId> = world.query::<&FloorItem>().iter(world).map(|f| f.id).collect();
        let inventory = world.resource::<Inventory>();
        floor.into_iter().filter_map(|id| inventory.item(id)).map(|i| (i.base.to_string(), i.seed)).collect()
    };
    gear.iter().map(|g| lying.iter().filter(|l| *l == g).count()).sum()
}

/// The pieces of gear drawn on `skeleton`'s bones.
fn worn_pieces(app: &mut App, skeleton: Entity) -> usize {
    let bones = app.world().get::<Bones>(skeleton).expect("a skeleton has bones").0.clone();
    let world = app.world_mut();
    world.query_filtered::<&ChildOf, With<WornGear>>().iter(world).filter(|p| bones.contains(&p.parent())).count()
}

fn strike_dead(app: &mut App, skeleton: Entity) {
    app.world_mut().write_message(Struck { target: skeleton, damage: 10_000, from: Vec2::ZERO, bludgeoning: false, critical: false, by_ally: false, graze: false });
}

#[test]
fn a_skeleton_killed_then_the_level_left_and_revisited_lies_bare_with_its_gear_on_the_floor_once() {
    let mut app = in_the_halls();
    let (spot, gear) = a_kitted_skeleton(&mut app);
    let skeleton = skeleton_at(&mut app, spot).unwrap();
    assert!(worn_pieces(&mut app, skeleton) >= gear.len(), "it stands in its gear");
    assert_eq!(gear_on_the_floor(&mut app, &gear), 0, "nothing of it lies about while it stands");
    strike_dead(&mut app, skeleton);
    run(&mut app, 60);
    assert!(app.world().get::<Corpse>(skeleton).is_some());
    assert_eq!(gear_on_the_floor(&mut app, &gear), gear.len(), "everything it wore lies where it fell");
    for trip in 0..2 {
        round_trip(&mut app);
        let body = skeleton_at(&mut app, spot).expect("its body is still there");
        assert!(app.world().get::<Corpse>(body).is_some() && app.world().get::<Looted>(body).is_some(), "trip {trip}: a looted body");
        assert_eq!(worn_pieces(&mut app, body), 0, "trip {trip}: the body lies bare");
        assert_eq!(gear_on_the_floor(&mut app, &gear), gear.len(), "trip {trip}: its gear lies on the floor exactly once");
    }
    let inventory = app.world().resource::<Inventory>();
    for (base, seed) in &gear {
        assert_eq!(inventory.ids().filter(|&id| inventory.item(id).is_some_and(|i| &i.base == base && i.seed == *seed)).count(), 1, "{base} {seed} is kept once");
    }
}

#[test]
fn a_skeleton_killed_just_before_the_delver_leaves_still_drops_its_gear_once() {
    let mut app = in_the_halls();
    let (spot, gear) = a_kitted_skeleton(&mut app);
    let skeleton = skeleton_at(&mut app, spot).unwrap();
    // Struck down and the stair taken in the same breath.
    strike_dead(&mut app, skeleton);
    app.world_mut().write_message(UseStair(StairKind::Down));
    run(&mut app, 200);
    assert_eq!(depth(&app), 3);
    go(&mut app, StairKind::Up);
    let body = skeleton_at(&mut app, spot).expect("its body is there");
    assert!(app.world().get::<Corpse>(body).is_some() && app.world().get::<Looted>(body).is_some());
    assert_eq!(worn_pieces(&mut app, body), 0, "bare");
    assert_eq!(gear_on_the_floor(&mut app, &gear), gear.len(), "its gear lies on the floor once");
    round_trip(&mut app);
    assert_eq!(gear_on_the_floor(&mut app, &gear), gear.len(), "and still once after another trip");
}

#[test]
fn a_living_skeleton_comes_back_in_its_gear_where_it_stood_and_as_hurt() {
    let mut app = in_the_halls();
    let (spot, gear) = a_kitted_skeleton(&mut app);
    let skeleton = skeleton_at(&mut app, spot).unwrap();
    let worn = worn_pieces(&mut app, skeleton);
    assert!(worn >= gear.len());
    app.world_mut().get_mut::<Health>(skeleton).unwrap().hp -= 3;
    let hp = app.world().get::<Health>(skeleton).unwrap().hp;
    let index = level(&app).creatures.iter().position(|s| *s == spot).unwrap();
    go(&mut app, StairKind::Down);
    // Back up, looked at the first frame it stands again, before it can wander off.
    app.world_mut().write_message(UseStair(StairKind::Up));
    let mut again = None;
    for _ in 0..200 {
        app.update();
        if depth(&app) == 2 {
            again = skeleton_at(&mut app, spot);
            if again.is_some() {
                break;
            }
        }
    }
    // It wandered while the delver left; it comes back where the record has it.
    let stood = Vec3::from_array(app.world().resource::<WorldRecord>().level(2).unwrap().creatures[&index].at);
    let again = again.expect("it is still there");
    assert!(app.world().get::<Corpse>(again).is_none() && app.world().get::<Looted>(again).is_none());
    assert_eq!(worn_pieces(&mut app, again), worn, "it wears what it wore");
    assert_eq!(app.world().get::<Health>(again).unwrap().hp, hp, "as hurt as it was");
    assert!(app.world().get::<Transform>(again).unwrap().translation.distance(stood) < 1e-3, "where it stood: {} from {stood}", app.world().get::<Transform>(again).unwrap().translation);
    assert_eq!(gear_on_the_floor(&mut app, &gear), 0, "none of its gear lies about");
}
