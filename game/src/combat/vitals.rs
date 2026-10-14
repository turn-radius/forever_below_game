//! The delver's hit points: bites take them, the view shows them, and at 0 the run ends.
//!
//! - The delver starts with a 1st-level character's hit points
//!   ([`super::first_level_hit_points`]).
//! - Each [`CreatureAttack`] takes its bite, and a poisonous snake's venom after the delver's
//!   Constitution save ([`super::bite_damage`]). The view's edge flashes red.
//! - A constrictor's bite also holds the delver ([`Constricted`]), who moves at a quarter of
//!   their pace until they strike it (or it dies, or they break more than a body's length
//!   away). The SRD's constrictor grapples with a separate Constrict attack and restrains its
//!   target, whose speed becomes 0 (SRD 5.1 p. 370, "Constrict"; p. 358, "Grappled"); the
//!   slowing, and breaking free by a blow, are our own placeholder.
//! - The hit points show low on the left of the view, "HP 8/12" in mono over a thin bar, in
//!   the page's colours.
//! - At 0 the run ends ([`Fallen`]): the delver stops, the view fades to near black with "You
//!   fall in the abyss", and after [`FALL_TIME`] seconds the game goes back to the title, where
//!   New delve begins again. Leaving the delve clears everything it spawned (each is scoped
//!   to it, [`crate::play::DelveSystems`]) and what it kept ([`end_the_delve`]). The SRD's death saving throws (p. 97) are left out.

use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_creatures::Species;

use super::feedback::{Corpse, Struck};
use super::{Health, RunDice, bite_damage, first_level_hit_points};
use crate::carry::{Aimed, Hud, ViewModelCamera};
use crate::character::Character;
use crate::creatures::Critter;
use crate::creatures::behaviour::{CreatureAttack, reach};
use crate::inventory::Inventory;
use crate::launch::AppState;
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Obstacles, Player, ScreenOpen};
use crate::player::Slowed;
use crate::ui::{BAD, BG, BRIGHT, FAINT, MUTED, UiFonts};

pub struct VitalsPlugin;

impl Plugin for VitalsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CreatureAttack>()
            .add_message::<Struck>()
            .init_resource::<Pain>()
            .add_systems(OnExit(AppState::Delving), end_the_delve)
            .add_systems(Update, (give_player_health, take_bites, break_free, fall, show_vitals).chain().in_set(DelveSystems))
            .add_systems(Update, spawn_vitals.in_set(DelveSystems));
    }
}

/// How long the view's edge flashes red after a bite, seconds. Our own.
pub const PAIN_TIME: f32 = 0.45;
/// How long "You fall in the abyss" shows before the title, seconds. Our own.
pub const FALL_TIME: f32 = 3.5;
/// How fast a held delver moves, as a share of their pace. Our own placeholder.
pub const HELD_PACE: f32 = 0.25;

/// Seconds left of the red flash round the view's edge.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Pain(pub f32);

/// The run has ended: seconds left before the title.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Fallen(pub f32);

/// On the delver while a constrictor holds them.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Constricted {
    pub by: Entity,
}

/// The delver's hit points, from who they are, once they stand in the level; `FB_PLAYER_HP=<n>`
/// starts them on `n` of those instead, to check a fall quickly.
fn give_player_health(mut commands: Commands, character: Res<Character>, new: Query<Entity, (With<Player>, Without<Health>)>) {
    let start = std::env::var("FB_PLAYER_HP").ok().and_then(|n| n.trim().parse::<i32>().ok());
    for player in &new {
        commands.entity(player).insert(starting_health(first_level_hit_points(&character), start));
    }
}

/// Full hit points of `max`, or `start` of them (at least 1, at most `max`).
pub fn starting_health(max: i32, start: Option<i32>) -> Health {
    Health { hp: start.map_or(max, |n| n.clamp(1, max)), max }
}

/// Each bite takes its damage from the delver; a constrictor's holds them.
fn take_bites(
    mut commands: Commands,
    mut bites: MessageReader<CreatureAttack>,
    character: Res<Character>,
    mut dice: ResMut<RunDice>,
    mut pain: ResMut<Pain>,
    bodies: Query<(), With<Corpse>>,
    mut players: Query<(Entity, &mut Health), With<Player>>,
) {
    for bite in bites.read() {
        if bodies.contains(bite.attacker) {
            continue;
        }
        for (player, mut health) in &mut players {
            if health.is_down() {
                continue;
            }
            let damage = bite_damage(bite, character.constitution(), &mut dice.0);
            health.hurt(damage);
            pain.0 = PAIN_TIME;
            info!("{:?} bites for {damage}: {} of {} hit points left", bite.species, health.hp, health.max);
            if bite.species == Species::ConstrictorSnake && !health.is_down() {
                commands.entity(player).insert((Constricted { by: bite.attacker }, Slowed(HELD_PACE)));
            }
        }
    }
}

/// A held delver breaks free by striking the constrictor, or when it dies or they get away.
fn break_free(
    mut commands: Commands,
    mut struck: MessageReader<Struck>,
    tuning: Option<Res<crate::tuning::Tuning>>,
    players: Query<(Entity, &Constricted, &Transform), With<Player>>,
    snakes: Query<(&Critter, &Transform), Without<Corpse>>,
) {
    let blows: Vec<Entity> = struck.read().map(|s| s.target).collect();
    let radius = tuning.as_deref().cloned().unwrap_or_default().moving.body_radius;
    for (player, held, at) in &players {
        let still = snakes.get(held.by).is_ok_and(|(critter, snake)| snake.translation.xz().distance(at.translation.xz()) <= reach(critter, radius) + radius);
        if blows.contains(&held.by) || !still {
            commands.entity(player).remove::<(Constricted, Slowed)>();
        }
    }
}

/// The overlay that says the run is over.
#[derive(Component, Debug)]
#[require(DespawnOnExit<crate::launch::AppState> = DespawnOnExit(crate::launch::AppState::Delving))]
pub struct FallOverlay;

/// At 0 hit points the delver stops and the view fades out to "You fall in the abyss"; then
/// back to the title.
#[allow(clippy::too_many_arguments)]
fn fall(
    mut commands: Commands,
    time: Res<Time>,
    fallen: Option<ResMut<Fallen>>,
    fonts: Option<Res<UiFonts>>,
    players: Query<(Entity, &Health), With<Player>>,
    view_models: Query<Entity, With<ViewModelCamera>>,
    mut overlay: Query<(&mut BackgroundColor, &Children), With<FallOverlay>>,
    mut words: Query<&mut TextColor>,
    mut screen: Option<ResMut<ScreenOpen>>,
    next: Option<ResMut<NextState<AppState>>>,
) {
    let Some(mut fallen) = fallen else {
        let Some((player, _)) = players.iter().find(|(_, h)| h.is_down()) else { return };
        commands.insert_resource(Fallen(FALL_TIME));
        commands.entity(player).insert(Slowed(0.0)).remove::<Constricted>();
        if let Some(screen) = screen.as_deref_mut() {
            screen.0 = None;
        }
        info!("the delver falls in the abyss");
        let mut root = commands.spawn((
            Name::new("You fall"),
            FallOverlay,
            Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(BG.with_alpha(0.0)),
            GlobalZIndex(10),
        ));
        if let Some(camera) = view_models.iter().next() {
            root.insert(UiTargetCamera(camera));
        }
        let font = fonts.as_deref().map(|f| f.display.clone()).unwrap_or_default();
        root.with_child((Text::new("You fall in the abyss"), TextFont::from_font_size(52.0).with_font(font), TextColor(BRIGHT.with_alpha(0.0))));
        return;
    };
    fallen.0 -= time.delta_secs();
    // Fades in over the first second and a half.
    let shown = ((FALL_TIME - fallen.0) / 1.5).clamp(0.0, 1.0);
    for (mut background, children) in &mut overlay {
        background.0 = BG.with_alpha(0.9 * shown);
        for &child in children {
            if let Ok(mut colour) = words.get_mut(child) {
                colour.0 = BRIGHT.with_alpha(shown);
            }
        }
    }
    if fallen.0 <= 0.0
        && let Some(mut next) = next
    {
        next.set(AppState::Title);
    }
}

/// The hit points readout.
#[derive(Component, Debug)]
struct HitPointsText;
/// The bar's filled part.
#[derive(Component, Debug)]
struct HitPointsBar;
/// The red edge round the view.
#[derive(Component, Debug)]
struct PainEdge;

/// The bar's width, logical pixels.
const BAR_WIDTH: f32 = 140.0;

/// Draws the hit points and the pain edge over each new view model, as part of the HUD.
fn spawn_vitals(mut commands: Commands, fonts: Option<Res<UiFonts>>, new: Query<Entity, Added<ViewModelCamera>>) {
    for camera in &new {
        let mono = fonts.as_deref().map(|f| f.mono.clone()).unwrap_or_default();
        commands
            .spawn((Name::new("Vitals"), Hud, UiTargetCamera(camera), Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() }, Pickable::IGNORE))
            .with_children(|hud| {
                hud.spawn((PainEdge, Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, border: UiRect::all(px(28)), ..default() }, BorderColor::all(BAD.with_alpha(0.0))));
                hud.spawn(Node { position_type: PositionType::Absolute, left: px(28), bottom: px(24), flex_direction: FlexDirection::Column, row_gap: px(5), ..default() }).with_children(|corner| {
                    corner.spawn((HitPointsText, Text::new("HP"), TextFont::from_font_size(17.0).with_font(mono), TextColor(BRIGHT.with_alpha(0.9))));
                    corner
                        .spawn((Node { width: px(BAR_WIDTH), height: px(5), border: UiRect::all(px(1)), ..default() }, BackgroundColor(FAINT.with_alpha(0.8)), BorderColor::all(MUTED.with_alpha(0.5))))
                        .with_child((HitPointsBar, Node { width: percent(100), height: percent(100), ..default() }, BackgroundColor(BAD.with_alpha(0.9))));
                });
            });
    }
}

/// Keeps the readout and bar to the delver's hit points, and fades the pain edge.
fn show_vitals(
    time: Res<Time>,
    mut pain: ResMut<Pain>,
    players: Query<&Health, With<Player>>,
    mut texts: Query<&mut Text, With<HitPointsText>>,
    mut bars: Query<&mut Node, With<HitPointsBar>>,
    mut edges: Query<&mut BorderColor, With<PainEdge>>,
) {
    pain.0 = (pain.0 - time.delta_secs()).max(0.0);
    let Some(health) = players.iter().next() else { return };
    let line = hit_points_line(health);
    for mut text in &mut texts {
        if text.0 != line {
            text.0 = line.clone();
        }
    }
    for mut bar in &mut bars {
        bar.width = percent(100.0 * health.hp.max(0) as f32 / health.max.max(1) as f32);
    }
    let alpha = 0.55 * (pain.0 / PAIN_TIME).clamp(0.0, 1.0);
    for mut edge in &mut edges {
        *edge = BorderColor::all(BAD.with_alpha(alpha));
    }
}

/// "HP 8/12".
pub fn hit_points_line(health: &Health) -> String {
    format!("HP {}/{}", health.hp.max(0), health.max)
}

/// Leaving the delve clears what it kept besides its entities: the level, what the delver
/// carried and the state of their view, so the next delve starts afresh. Everything the delve
/// spawned carries `DespawnOnExit(AppState::Delving)` (on its root, or required by its marker
/// component) and Bevy despawns it on the way out, as its state-scoped entities do
/// (<https://docs.rs/bevy/0.20.0/bevy/state/state_scoped/struct.DespawnOnExit.html>).
pub fn end_the_delve(mut commands: Commands, mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>) {
    commands.remove_resource::<CurrentLevel>();
    commands.insert_resource(Inventory::default());
    commands.insert_resource(ScreenOpen::default());
    commands.insert_resource(Obstacles::default());
    commands.insert_resource(BlockedCells::default());
    commands.insert_resource(Aimed::default());
    commands.insert_resource(Pain::default());
    commands.remove_resource::<Fallen>();
    if let Some(cursor) = cursor.as_deref_mut() {
        crate::player::set_grab(cursor, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;

    /// A delve with a delver of 10 hit points and one constrictor, stepped 1/60 s a frame.
    fn delve() -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_state(AppState::Delving)
            .insert_resource(RunDice(procgen_rng::Mulberry32::new(4)))
            .init_resource::<Character>()
            .init_resource::<ScreenOpen>()
            .add_plugins(VitalsPlugin);
        let player = app.world_mut().spawn((Player, Transform::default(), Health::full(10))).id();
        let level = fb_level::generate(1207);
        let spot = *level.creatures.iter().find(|c| c.species == Species::ConstrictorSnake).unwrap();
        let creature = spot.creature();
        let snake = app.world_mut().spawn((Critter::new(spot, creature), Transform::from_xyz(10.0, 0.0, 0.0))).id();
        (app, player, snake)
    }

    fn bite(app: &mut App, attacker: Entity, species: Species, damage: u32) {
        app.world_mut().write_message(CreatureAttack { attacker, species, damage, poison: 0 });
        app.update();
    }

    #[test]
    fn a_delver_can_start_on_fewer_hit_points() {
        assert_eq!(starting_health(12, None), Health::full(12));
        assert_eq!(starting_health(12, Some(1)), Health { hp: 1, max: 12 });
        assert_eq!(starting_health(12, Some(40)), Health::full(12));
        assert_eq!(starting_health(12, Some(0)), Health { hp: 1, max: 12 });
    }

    #[test]
    fn bites_take_hit_points_and_flash_the_edge() {
        let (mut app, player, snake) = delve();
        app.update();
        bite(&mut app, snake, Species::Rat, 3);
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 7);
        assert!(app.world().resource::<Pain>().0 > 0.0);
        assert_eq!(hit_points_line(app.world().get::<Health>(player).unwrap()), "HP 7/10");
    }

    #[test]
    fn a_constrictor_holds_you_until_you_strike_it() {
        let (mut app, player, snake) = delve();
        app.update();
        bite(&mut app, snake, Species::ConstrictorSnake, 2);
        assert_eq!(app.world().get::<Constricted>(player), Some(&Constricted { by: snake }));
        assert_eq!(app.world().get::<Slowed>(player), Some(&Slowed(HELD_PACE)));
        app.update();
        assert!(app.world().get::<Constricted>(player).is_some(), "still held");
        app.world_mut().write_message(Struck { target: snake, damage: 1, from: Vec2::ZERO });
        app.update();
        assert!(app.world().get::<Constricted>(player).is_none());
        assert!(app.world().get::<Slowed>(player).is_none());
    }

    #[test]
    fn at_zero_hit_points_the_run_ends_and_the_title_returns() {
        let (mut app, player, snake) = delve();
        app.update();
        bite(&mut app, snake, Species::Rat, 25);
        assert!(app.world().get::<Health>(player).unwrap().is_down());
        app.update();
        assert!(app.world().get_resource::<Fallen>().is_some());
        assert_eq!(app.world().get::<Slowed>(player), Some(&Slowed(0.0)), "the delver stops");
        for _ in 0..((FALL_TIME * 60.0) as usize + 5) {
            app.update();
        }
        assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Title);
        // Leaving the delve cleared it.
        assert!(app.world().get_entity(player).is_err(), "the delver is gone");
        assert!(app.world().get_entity(snake).is_err(), "the creatures are gone");
        assert!(app.world().get_resource::<Fallen>().is_none());
    }

    #[test]
    fn a_new_delve_after_a_fall_starts_clean() {
        use crate::inventory::{Hovered, OpenContainer, leave_the_delve};
        let (mut app, player, snake) = delve();
        app.init_resource::<crate::inventory::DollStage>()
            .init_resource::<crate::inventory::icon::IconCache>()
            .insert_resource(OpenContainer(Some(crate::inventory::ItemId(3))))
            .add_systems(OnExit(AppState::Delving), leave_the_delve);
        // A hover card made for this delve's view.
        let card = app.world_mut().spawn((Node::default(), DespawnOnExit(AppState::Delving))).id();
        app.update();
        bite(&mut app, snake, Species::Rat, 25);
        for _ in 0..((FALL_TIME * 60.0) as usize + 5) {
            app.update();
        }
        assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Title);
        for gone in [player, snake, card] {
            assert!(app.world().get_entity(gone).is_err(), "{gone} outlived the delve");
        }
        assert_eq!(*app.world().resource::<OpenContainer>(), OpenContainer(None));
        assert_eq!(*app.world().resource::<Hovered>(), Hovered(None));
        assert_eq!(*app.world().resource::<Pain>(), Pain(0.0));
        // New delve: a fresh delver stands up whole, and nothing of the last run is left over.
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Delving);
        app.update();
        let fresh = app.world_mut().spawn((Player, Transform::default())).id();
        app.update();
        let health = *app.world().get::<Health>(fresh).unwrap();
        assert!(health.hp == health.max && health.hp > 0);
        assert!(app.world().get_resource::<Fallen>().is_none());
        app.update();
        assert!(app.world().get_resource::<Fallen>().is_none(), "the new delver is not fallen");
    }
}
