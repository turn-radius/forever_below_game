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
//! - The hit points show as the life orb at the bottom left of the view ([`super::orbs`]).
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
use crate::exhaustion::Exhaustion;
use crate::inventory::Inventory;
use crate::launch::AppState;
use crate::play::{BlockedCells, CurrentLevel, DelveSystems, Obstacles, Player, ScreenOpen};
use crate::player::Slowed;
use crate::ui::{BAD, BG, BRIGHT, UiFonts};

pub struct VitalsPlugin;

impl Plugin for VitalsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CreatureAttack>()
            .add_message::<Struck>()
            .init_resource::<Pain>()
            .add_systems(OnExit(AppState::Delving), end_the_delve)
            .add_systems(Update, (give_player_health, take_bites, show_vitals, break_free, fall).chain().in_set(DelveSystems))
            .add_systems(Update, spawn_vitals.in_set(DelveSystems));
    }
}

/// How long the view's edge flashes red after a bite, seconds. Our own.
pub const PAIN_TIME: f32 = 0.45;
/// How long an attack marker fades over, seconds. Our own.
pub const MARKER_FADE_TIME: f32 = 1.0;
/// How long "You fall in the abyss" shows before the title, seconds. Our own.
pub const FALL_TIME: f32 = 3.5;
/// How fast a held delver moves, as a share of their pace. Our own placeholder.
pub const HELD_PACE: f32 = 0.25;

/// Below this share of maximum hit points the view's edge darkens. Our own.
pub const HURT_BADLY: f32 = 0.25;
/// Seconds one slow pulse of the darkened edge takes. Our own.
pub const VIGNETTE_PULSE: f32 = 3.0;
/// The darkened edge's colour: a dark red. Our own.
const VIGNETTE_COLOUR: Color = Color::srgb(0.32, 0.02, 0.03);

/// How strong the dark red edge is, 0 to 1, for `hp` of `max` hit points `seconds` into the
/// delve: nothing at a quarter of `max` or more, and below that stronger the nearer 0, with a
/// slow pulse. Our own.
pub fn vignette_strength(hp: i32, max: i32, seconds: f32) -> f32 {
    let share = hp.max(0) as f32 / max.max(1) as f32;
    if share >= HURT_BADLY {
        return 0.0;
    }
    let base = 0.35 + 0.65 * (1.0 - share / HURT_BADLY);
    let pulse = 0.85 + 0.15 * (std::f32::consts::TAU * seconds / VIGNETTE_PULSE).sin();
    base * pulse
}

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

/// An attack marker on the HUD: a full-view, centred container turned to point toward an
/// attacker, with a red arc ([`MarkerArc`]) near its top edge.
#[derive(Component, Clone, Copy, Debug)]
pub struct AttackMarker {
    /// The attacker, to refresh the marker on subsequent bites.
    pub attacker: Entity,
    /// Damage this bite did, which sets the marker's strength.
    pub damage: u32,
    /// Seconds left until the marker fades away.
    pub left: f32,
    /// The attacker's position on the floor when the bite landed.
    pub attacker_pos: Vec2,
}

/// The red arc inside an [`AttackMarker`].
#[derive(Component, Debug)]
struct MarkerArc;

/// The HUD root the vitals draw into, which attack markers and the orbs ([`super::orbs`]) hang
/// from.
#[derive(Component, Debug)]
pub(crate) struct VitalsRoot;

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
    sounds: Option<Res<super::feedback::CreatureSounds>>,
    settings: Option<Res<crate::settings::GameSettings>>,
    tuning: Option<Res<crate::tuning::Tuning>>,
) {
    let game = settings.as_deref().copied().unwrap_or_default();
    let tuning = tuning.as_deref().cloned().unwrap_or_default();
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
            // One dull thud for each blow that lands.
            if let Some(sounds) = &sounds {
                commands.spawn((Name::new("blow lands"), AudioPlayer::new(sounds.thud.clone()), crate::audio::sound_effect(&game, &tuning), DespawnOnExit(AppState::Delving)));
            }
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

/// At 0 hit points, or exhaustion level 6, the delver stops and the view fades out to "You fall in the abyss"; then
/// back to the title.
#[allow(clippy::too_many_arguments)]
fn fall(
    mut commands: Commands,
    time: Res<Time>,
    fallen: Option<ResMut<Fallen>>,
    fonts: Option<Res<UiFonts>>,
    players: Query<(Entity, &Health, &Exhaustion), With<Player>>,
    view_models: Query<Entity, With<ViewModelCamera>>,
    mut overlay: Query<(&mut BackgroundColor, &Children), With<FallOverlay>>,
    mut words: Query<&mut TextColor>,
    mut screen: Option<ResMut<ScreenOpen>>,
    next: Option<ResMut<NextState<AppState>>>,
) {
    let Some(mut fallen) = fallen else {
        let Some((player, ..)) = players.iter().find(|(_, health, exhaustion)| health.is_down() || exhaustion.0.is_dead()) else { return };
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

/// The red edge round the view.
#[derive(Component, Debug)]
struct PainEdge;

/// Draws the pain edge over each new view model, as part of the HUD that the orbs and attack
/// markers join.
fn spawn_vitals(mut commands: Commands, new: Query<Entity, Added<ViewModelCamera>>) {
    for camera in &new {
        commands
            .spawn((Name::new("Vitals"), Hud, VitalsRoot, UiTargetCamera(camera), Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() }, Pickable::IGNORE))
            .with_children(|hud| {
                hud.spawn((PainEdge, Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, border: UiRect::all(px(28)), ..default() }, BorderColor::all(BAD.with_alpha(0.0))));
            });
    }
}

/// Fades the pain edge and darkens it while the delver is badly hurt, and keeps the attack
/// markers: one per attacker, made or refreshed by its bite, turned toward it, fading to nothing.
#[allow(clippy::too_many_arguments)]
fn show_vitals(
    time: Res<Time>,
    mut commands: Commands,
    mut pain: ResMut<Pain>,
    mut bites: MessageReader<CreatureAttack>,
    players: Query<(&Health, &Transform), With<Player>>,
    mut edges: Query<&mut BorderColor, With<PainEdge>>,
    mut markers: Query<(Entity, &mut AttackMarker, &mut UiTransform, &Children)>,
    mut arcs: Query<&mut BackgroundColor, With<MarkerArc>>,
    roots: Query<Entity, With<VitalsRoot>>,
) {
    let dt = time.delta_secs();
    pain.0 = (pain.0 - dt).max(0.0);
    // Read the bites even with no delver yet, so they do not pile up.
    let mut fresh: Vec<CreatureAttack> = Vec::new();
    for bite in bites.read() {
        // Bites from one attacker in a frame make one marker: the hardest, at the last place.
        match fresh.iter_mut().find(|b| b.attacker == bite.attacker) {
            Some(b) => {
                b.damage = b.damage.max(bite.damage);
                b.attacker_pos = bite.attacker_pos;
            }
            None => fresh.push(*bite),
        }
    }
    let Some((health, player)) = players.iter().next() else { return };
    let flash = 0.55 * (pain.0 / PAIN_TIME).clamp(0.0, 1.0);
    let dark = 0.7 * vignette_strength(health.hp, health.max, time.elapsed_secs());
    // A fresh blow's red flash shows over the dark edge, whichever is stronger.
    let edge_colour = if flash > 0.0 { BAD.with_alpha(flash.max(dark)) } else { VIGNETTE_COLOUR.with_alpha(dark) };
    for mut edge in &mut edges {
        *edge = BorderColor::all(edge_colour);
    }

    for bite in fresh {
        if let Some((_, mut marker, _, _)) = markers.iter_mut().find(|(_, m, _, _)| m.attacker == bite.attacker) {
            marker.damage = bite.damage;
            marker.left = MARKER_FADE_TIME;
            marker.attacker_pos = bite.attacker_pos;
        } else if let Some(root) = roots.iter().next() {
            let marker = AttackMarker { attacker: bite.attacker, damage: bite.damage, left: MARKER_FADE_TIME, attacker_pos: bite.attacker_pos };
            commands.entity(root).with_children(|hud| {
                hud.spawn((marker, Name::new("AttackMarker"), attack_marker_node(), UiTransform::IDENTITY, Pickable::IGNORE)).with_child((
                    MarkerArc,
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(50),
                        top: vmin(1.5),
                        width: vmin(34),
                        height: vmin(7),
                        margin: UiRect::left(vmin(-17)),
                        border_radius: BorderRadius::all(percent(50)),
                        ..default()
                    },
                    BackgroundColor(BAD.with_alpha(0.0)),
                    Pickable::IGNORE,
                ));
            });
        }
    }

    let player_pos = player.translation.xz();
    let (yaw, _, _) = player.rotation.to_euler(EulerRot::YXZ);
    let max_hp = health.max.max(1) as f32;
    for (entity, mut marker, mut transform, children) in &mut markers {
        marker.left -= dt;
        if marker.left <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        // UI turns clockwise on screen for a positive angle, as the angle from facing does.
        transform.rotation = Rot2::radians(marker_angle(player_pos, yaw, marker.attacker_pos));
        // A faint arc for a light bite, a strong one for a hard bite, fading to nothing.
        let strength = 0.3 + 0.7 * (marker.damage as f32 / max_hp).min(1.0);
        let fade = (marker.left / MARKER_FADE_TIME).clamp(0.0, 1.0);
        for &child in children {
            if let Ok(mut colour) = arcs.get_mut(child) {
                colour.0 = BAD.with_alpha(strength * fade);
            }
        }
    }
}

/// A marker's container: a square as big as the view's short side, centred, which the arc sits at
/// the top of; turning it about its middle carries the arc round the view.
fn attack_marker_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: percent(50),
        top: percent(50),
        width: vmin(100),
        height: vmin(100),
        margin: UiRect { left: vmin(-50), top: vmin(-50), ..default() },
        ..default()
    }
}

/// Calculates the clockwise-from-forward angle to an attacker, accounting for the delver's facing.
/// `player_pos` and `attacker_pos` are xz coordinates on the floor; `yaw` is the delver's rotation
/// around the y axis. Returns the angle in radians, normalized to [0, 2π): 0° is in front,
/// 90° is to the right, 180° is behind, 270° is to the left.
/// Our own.
pub fn marker_angle(player_pos: Vec2, yaw: f32, attacker_pos: Vec2) -> f32 {
    let (dx, dz) = (attacker_pos.x - player_pos.x, attacker_pos.y - player_pos.y);
    (dx.atan2(-dz) + yaw).rem_euclid(std::f32::consts::TAU)
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
        let attacker_pos = app.world().get::<Transform>(attacker).map(|t| t.translation.xz()).unwrap_or_default();
        app.world_mut().write_message(CreatureAttack { attacker, species, damage, poison: 0, attacker_pos });
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
    fn at_exhaustion_level_6_the_run_ends_as_at_zero_hit_points() {
        let (mut app, player, snake) = delve();
        app.update();
        bite(&mut app, snake, Species::ConstrictorSnake, 2);
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(crate::play::Screen::Inventory);
        app.update();
        assert!(app.world().get_resource::<Fallen>().is_none(), "level 0 does not fall");
        app.world_mut().get_mut::<Exhaustion>(player).unwrap().0 = fb_character::Exhaustion::new(6);
        app.update();
        assert!(app.world().get_resource::<Fallen>().is_some());
        assert_eq!(app.world().get::<Slowed>(player), Some(&Slowed(0.0)), "the delver stops");
        assert!(app.world().get::<Constricted>(player).is_none(), "the grip lets go");
        assert_eq!(app.world().resource::<ScreenOpen>().0, None, "the screens close");
        assert!(app.world().get::<Health>(player).unwrap().hp > 0, "hit points were not what ended it");
        assert_eq!(app.world_mut().query::<&FallOverlay>().iter(app.world()).count(), 1, "the overlay shows");
        for _ in 0..((FALL_TIME * 60.0) as usize + 5) {
            app.update();
        }
        assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Title);
        assert!(app.world().get_entity(player).is_err(), "the delver is gone");
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

    #[test]
    fn marker_angle_with_yaw_0_places_attacker_on_the_right_at_90_degrees() {
        let player_pos = Vec2::ZERO;
        let yaw = 0.0;
        let attacker_pos = Vec2::new(5.0, 0.0);
        let angle = marker_angle(player_pos, yaw, attacker_pos);
        // Should be π/2 (90 degrees).
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5, "angle {}", angle);
    }

    #[test]
    fn marker_angle_with_yaw_0_places_attacker_on_the_left_at_270_degrees() {
        let player_pos = Vec2::ZERO;
        let yaw = 0.0;
        let attacker_pos = Vec2::new(-5.0, 0.0);
        let angle = marker_angle(player_pos, yaw, attacker_pos);
        // Should be 3π/2 (270 degrees).
        let expected = 3.0 * std::f32::consts::FRAC_PI_2;
        assert!((angle - expected).abs() < 1e-5, "angle {}", angle);
    }

    #[test]
    fn marker_angle_with_yaw_0_places_attacker_behind_at_180_degrees() {
        let player_pos = Vec2::ZERO;
        let yaw = 0.0;
        let attacker_pos = Vec2::new(0.0, 5.0);
        let angle = marker_angle(player_pos, yaw, attacker_pos);
        // Should be π (180 degrees).
        assert!((angle - std::f32::consts::PI).abs() < 1e-5, "angle {}", angle);
    }

    #[test]
    fn marker_angle_with_yaw_90_rotates_the_perspective() {
        let player_pos = Vec2::ZERO;
        let yaw = std::f32::consts::FRAC_PI_2; // 90 degrees
        // Attacker to the player's right in world space (5, 0) is behind in rotated perspective.
        let attacker_pos = Vec2::new(5.0, 0.0);
        let angle = marker_angle(player_pos, yaw, attacker_pos);
        // Should be π (180 degrees, behind).
        assert!((angle - std::f32::consts::PI).abs() < 1e-5, "angle {}", angle);
    }

    #[test]
    fn marker_angle_with_yaw_negative_90_places_attacker_behind() {
        let player_pos = Vec2::ZERO;
        let yaw = -std::f32::consts::FRAC_PI_2; // -90 degrees
        // Attacker to the left in world space (-5, 0) is behind when facing right (-90°).
        let attacker_pos = Vec2::new(-5.0, 0.0);
        let angle = marker_angle(player_pos, yaw, attacker_pos);
        // Should be π (180 degrees, behind).
        assert!((angle - std::f32::consts::PI).abs() < 1e-5, "angle {}", angle);
    }

    #[test]
    fn marker_angle_with_yaw_180_reverses_direction() {
        let player_pos = Vec2::ZERO;
        let yaw = std::f32::consts::PI; // 180 degrees (facing backward)
        // Attacker ahead in world space (0, -5) is behind from reversed perspective.
        let attacker_pos = Vec2::new(0.0, -5.0);
        let angle = marker_angle(player_pos, yaw, attacker_pos);
        // Should be π (180 degrees, behind).
        assert!((angle - std::f32::consts::PI).abs() < 1e-5, "angle {}", angle);
    }

    #[test]
    fn marker_angle_with_yaw_90_and_attacker_ahead_in_world() {
        let player_pos = Vec2::ZERO;
        let yaw = std::f32::consts::FRAC_PI_2; // 90 degrees (facing right)
        // Attacker ahead in world space (0, -5) is to the right from rightward perspective.
        let attacker_pos = Vec2::new(0.0, -5.0);
        let angle = marker_angle(player_pos, yaw, attacker_pos);
        // Should be π/2 (90 degrees, right).
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5, "angle {}", angle);
    }

    /// A delve with a view model camera, so the vitals root (and attack markers) exist.
    fn delve_with_hud() -> (App, Entity, Entity) {
        let (mut app, player, snake) = delve();
        let follows = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(ViewModelCamera { follows });
        app.update();
        app.update();
        (app, player, snake)
    }

    fn markers(app: &mut App) -> Vec<(AttackMarker, f32)> {
        let mut query = app.world_mut().query::<(&AttackMarker, &UiTransform)>();
        query.iter(app.world()).map(|(m, t)| (*m, t.rotation.as_radians())).collect()
    }

    fn spare_attacker(app: &mut App, at: Vec3) -> Entity {
        app.world_mut().spawn(Transform::from_translation(at)).id()
    }

    #[test]
    fn a_bite_makes_one_marker_and_a_repeat_refreshes_it() {
        let (mut app, _player, snake) = delve_with_hud();
        bite(&mut app, snake, Species::Rat, 1);
        app.update();
        assert_eq!(markers(&mut app).len(), 1);
        for _ in 0..30 {
            app.update();
        }
        let before = markers(&mut app)[0].0.left;
        bite(&mut app, snake, Species::Rat, 2);
        app.update();
        let after = markers(&mut app);
        assert_eq!(after.len(), 1, "a repeat bite refreshes, not adds");
        assert!(after[0].0.left > before + 0.3, "{} then {}", before, after[0].0.left);
        assert_eq!(after[0].0.damage, 2);
    }

    #[test]
    fn two_attackers_make_two_markers() {
        let (mut app, _player, snake) = delve_with_hud();
        let other = spare_attacker(&mut app, Vec3::new(-10.0, 0.0, 0.0));
        bite(&mut app, snake, Species::Rat, 1);
        bite(&mut app, other, Species::Rat, 1);
        app.update();
        assert_eq!(markers(&mut app).len(), 2);
    }

    #[test]
    fn two_bites_in_a_frame_from_one_attacker_make_one_marker() {
        let (mut app, _player, snake) = delve_with_hud();
        let attacker_pos = Vec2::new(10.0, 0.0);
        for damage in [1, 3] {
            app.world_mut().write_message(CreatureAttack { attacker: snake, species: Species::Rat, damage, poison: 0, attacker_pos });
        }
        app.update();
        app.update();
        let found = markers(&mut app);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.damage, 3, "the harder bite sets the strength");
    }

    #[test]
    fn a_marker_is_gone_after_a_second() {
        let (mut app, _player, snake) = delve_with_hud();
        bite(&mut app, snake, Species::Rat, 1);
        app.update();
        assert_eq!(markers(&mut app).len(), 1);
        for _ in 0..62 {
            app.update();
        }
        assert!(markers(&mut app).is_empty());
    }

    #[test]
    fn the_marker_turns_to_the_right_for_an_attacker_on_the_right() {
        let (mut app, player, _snake) = delve_with_hud();
        // Turned 90 degrees to face -x, an attacker at -z is on the delver's right.
        app.world_mut().get_mut::<Transform>(player).unwrap().rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let attacker = spare_attacker(&mut app, Vec3::new(0.0, 0.0, -10.0));
        bite(&mut app, attacker, Species::Rat, 2);
        app.update();
        let found = markers(&mut app);
        assert_eq!(found.len(), 1);
        assert!((found[0].1 - std::f32::consts::FRAC_PI_2).abs() < 1e-4, "rotation {}", found[0].1);
    }

    /// The volumes of the sound effects playing, loudest last.
    fn playing(app: &mut App) -> Vec<f32> {
        let mut query = app.world_mut().query_filtered::<&PlaybackSettings, With<crate::audio::SoundEffect>>();
        let mut volumes: Vec<f32> = query.iter(app.world()).map(|p| p.volume.to_linear()).collect();
        volumes.sort_by(f32::total_cmp);
        volumes
    }

    #[test]
    fn each_landed_blow_sounds_one_thud_at_the_effects_volume() {
        let (mut app, _player, snake) = delve();
        app.insert_resource(super::super::feedback::CreatureSounds::default())
            .insert_resource(crate::settings::GameSettings { sound_volume: 0.5, ..default() })
            .init_resource::<crate::tuning::Tuning>();
        let wanted = crate::audio::effect_volume(app.world().resource(), app.world().resource());
        assert!(wanted > 0.0);
        app.update();
        assert!(playing(&mut app).is_empty(), "no blow, no thud");
        bite(&mut app, snake, Species::Rat, 1);
        assert_eq!(playing(&mut app), vec![wanted]);
        bite(&mut app, snake, Species::Rat, 1);
        assert_eq!(playing(&mut app), vec![wanted, wanted], "one thud a blow");
        app.update();
        assert_eq!(playing(&mut app).len(), 2, "and no more without one");
    }

    #[test]
    fn the_thud_is_silent_with_the_effects_volume_at_zero() {
        let (mut app, _player, snake) = delve();
        app.insert_resource(super::super::feedback::CreatureSounds::default())
            .insert_resource(crate::settings::GameSettings { sound_volume: 0.0, ..default() })
            .init_resource::<crate::tuning::Tuning>();
        bite(&mut app, snake, Species::Rat, 1);
        assert_eq!(playing(&mut app), vec![0.0]);
    }

    #[test]
    fn the_vignette_is_off_above_a_quarter_and_stronger_nearer_zero() {
        for t in [0.0, 0.7, 1.5, 2.2, 2.9] {
            assert_eq!(vignette_strength(30, 100, t), 0.0, "off at 30%");
            assert_eq!(vignette_strength(25, 100, t), 0.0, "off at exactly 25%");
            assert!(vignette_strength(20, 100, t) > 0.0, "on at 20%");
            assert!(vignette_strength(5, 100, t) > vignette_strength(20, 100, t), "stronger at 5% than 20% at {t}");
            assert!(vignette_strength(0, 100, t) >= vignette_strength(5, 100, t));
            assert!(vignette_strength(1, 100, t) <= 1.0);
        }
        // It pulses.
        assert_ne!(vignette_strength(10, 100, 0.0), vignette_strength(10, 100, VIGNETTE_PULSE / 4.0));
    }

    fn edge_colour(app: &mut App) -> Srgba {
        let mut query = app.world_mut().query_filtered::<&BorderColor, With<PainEdge>>();
        query.single(app.world()).unwrap().top.to_srgba()
    }

    #[test]
    fn the_edge_darkens_while_badly_hurt_and_clears_on_healing() {
        let (mut app, player, _snake) = delve_with_hud();
        assert_eq!(edge_colour(&mut app).alpha, 0.0);
        *app.world_mut().get_mut::<Health>(player).unwrap() = Health { hp: 2, max: 10 };
        app.update();
        let low = edge_colour(&mut app);
        assert!(low.alpha > 0.0, "dark at 20%");
        assert!(low.red < 0.5, "a dark red, not the flash: {low:?}");
        app.world_mut().get_mut::<Health>(player).unwrap().hp = 1;
        app.update();
        app.update();
        assert!(edge_colour(&mut app).alpha > 0.0);
        app.world_mut().get_mut::<Health>(player).unwrap().hp = 3;
        app.update();
        assert_eq!(edge_colour(&mut app).alpha, 0.0, "off at 30%");
    }

    #[test]
    fn a_fresh_blow_flashes_over_the_dark_edge() {
        let (mut app, player, snake) = delve_with_hud();
        *app.world_mut().get_mut::<Health>(player).unwrap() = Health { hp: 20, max: 100 };
        app.update();
        let dark = edge_colour(&mut app);
        bite(&mut app, snake, Species::Rat, 1);
        let flash = edge_colour(&mut app);
        assert!(flash.red > dark.red, "the flash is the brighter red: {flash:?} over {dark:?}");
        assert!(flash.alpha >= dark.alpha);
    }

    #[test]
    fn the_edge_is_part_of_the_hud_and_hides_under_screens() {
        use crate::carry::hide_hud_under_screens;
        let (mut app, _player, _snake) = delve_with_hud();
        app.add_systems(Update, hide_hud_under_screens);
        let mut query = app.world_mut().query_filtered::<&ChildOf, With<PainEdge>>();
        let root = query.single(app.world()).unwrap().parent();
        assert!(app.world().get::<Hud>(root).is_some());
        assert!(app.world().get::<DespawnOnExit<AppState>>(root).is_some(), "scoped to the delve");
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(crate::play::Screen::Inventory);
        app.update();
        assert_eq!(*app.world().get::<Visibility>(root).unwrap(), Visibility::Hidden);
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        app.update();
        assert_eq!(*app.world().get::<Visibility>(root).unwrap(), Visibility::Inherited);
    }
}
