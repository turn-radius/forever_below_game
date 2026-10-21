//! Falling unconscious instead of dying: the delver chooses to come to or to let go.
//!
//! - Brought to 0 hit points by a blow, with a revive left in this world ([`Revives`] against
//!   `[revival]` in `rules.toml`, [`fb_character::revival`]), the delver falls unconscious
//!   ([`Unconscious`]): the view darkens to "You fall unconscious" with two buttons, Come to
//!   (Enter) and Let go (Backspace). Letting go ends the run as a fall always did
//!   ([`super::vitals::begin_the_fall`]).
//! - Coming to leaves a wound that never heals ([`fb_character::revival::pick_wound`]), an
//!   effect on one body part the sheet and HUD show.
//! - If any faction's folk on the level are still friendly, the biggest such faction carries
//!   the delver to its hall, the room most of them stand in, heals them and takes a share of
//!   their coin ([`fb_character::revival::choose_hall`]). Otherwise they come to where they fell,
//!   hours later and weak, and every creature that was hunting them has gone home.
//! - A card tells them what happened, then fades.
//!
//! The revive count belongs to the world, and the world (the run's abyss) is made afresh when a
//! run begins, so [`Revives`] starts over with each new delve.

use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_character::revival::{Wound, choose_hall, coin_words, fee, in_copper, pick_wound};
use fb_level::{CELL as GRID, Level, folk::Faction};

use super::feedback::Corpse;
use super::{Health, RunDice};
use crate::carry::ViewModelCamera;
use crate::character::DelverEffects;
use crate::creatures::behaviour::{Brain, Mode};
use crate::folk::Denizen;
use crate::game_clock::GameClock;
use crate::hostility::Hostility;
use crate::inventory::Inventory;
use crate::launch::AppState;
use crate::play::{CurrentLevel, DelveSystems, Player};
use crate::player::{Locomotion, Slowed, set_grab};
use crate::rules::GameRules;
use crate::ui::{BG, BRIGHT, CELL, INK, LINE, MUTED, UiFonts, node_rect, ui_cursor};

pub struct RevivalPlugin;

impl Plugin for RevivalPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Revives>()
            .add_message::<ReviveChosen>()
            .add_systems(OnEnter(AppState::Delving), |mut revives: ResMut<Revives>| revives.0 = 0)
            .add_systems(Update, (choose, come_to, let_go, fade_wake_card).chain().in_set(DelveSystems));
        // `FB_FALL=wait|come_to|let_go`, for captures (README, "Debug switches").
        if let Ok(then) = std::env::var("FB_FALL") {
            let choice = match then.trim() {
                "come_to" => Some(ReviveButton::ComeTo),
                "let_go" => Some(ReviveButton::LetGo),
                _ => None,
            };
            app.insert_resource(DebugFall { choice, since: 0.0, down: false, chosen: false }).add_systems(Update, debug_fall.before(choose).in_set(DelveSystems));
        }
    }
}

/// What `FB_FALL` does: knocks the delver to 0 a second in, then makes its choice, if it names
/// one, two seconds after they fall unconscious.
#[derive(Resource, Clone, Copy, Debug)]
struct DebugFall {
    choice: Option<ReviveButton>,
    since: f32,
    down: bool,
    chosen: bool,
}

fn debug_fall(time: Res<Time>, mut debug: ResMut<DebugFall>, unconscious: Option<Res<Unconscious>>, mut players: Query<&mut Health, With<Player>>, mut chosen: MessageWriter<ReviveChosen>) {
    debug.since += time.delta_secs();
    if !debug.down && debug.since >= 1.0 {
        for mut health in &mut players {
            health.hp = 0;
            debug.down = true;
        }
        debug.since = 0.0;
    }
    if debug.down && unconscious.is_none() {
        debug.since = 0.0;
    }
    if let Some(choice) = debug.choice.filter(|_| debug.down && !debug.chosen && unconscious.is_some() && debug.since >= 2.0) {
        chosen.write(ReviveChosen(choice));
        debug.chosen = true;
    }
}

/// How many times the delver has come to in this world.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Revives(pub u32);

/// The delver lies unconscious, waiting on the choice.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct Unconscious;

/// The overlay that offers the choice.
#[derive(Component, Debug)]
#[require(DespawnOnExit<AppState> = DespawnOnExit(AppState::Delving))]
pub struct UnconsciousOverlay;

/// A button on the overlay.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviveButton {
    ComeTo,
    LetGo,
}

/// The choice made, by a click or a key.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReviveChosen(pub ReviveButton);

/// The card that says where the delver came to and what it cost, fading as they look round.
#[derive(Component, Debug, Default)]
#[require(DespawnOnExit<AppState> = DespawnOnExit(AppState::Delving))]
pub struct WakeCard {
    age: f32,
}

/// Seconds the wake card's black holds before it starts to clear. Our own.
const WAKE_DARK: f32 = 2.0;
/// Seconds the wake card's words show before they fade. Our own.
const WAKE_WORDS: f32 = 7.0;
/// Seconds anything on the wake card takes to fade. Our own.
const WAKE_FADE: f32 = 1.5;

/// Darkens the view to "You fall unconscious" and offers the choice; `left` is how many times
/// the delver may still come to, counting this one.
pub fn fall_unconscious(commands: &mut Commands, fonts: Option<&UiFonts>, camera: Option<Entity>, left: u32) {
    commands.insert_resource(Unconscious);
    let display = fonts.map(|f| f.display.clone()).unwrap_or_default();
    let body = fonts.map(|f| f.body.clone()).unwrap_or_default();
    let mut root = commands.spawn((
        Name::new("Unconscious"),
        UnconsciousOverlay,
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(18),
            ..default()
        },
        BackgroundColor(BG.with_alpha(0.92)),
        GlobalZIndex(10),
    ));
    if let Some(camera) = camera {
        root.insert(UiTargetCamera(camera));
    }
    let times = if left == 1 { "once more".to_string() } else { format!("{left} more times") };
    root.with_children(|overlay| {
        overlay.spawn((Text::new("You fall unconscious"), TextFont::from_font_size(52.0).with_font(display.clone()), TextColor(BRIGHT)));
        overlay.spawn((Text::new(format!("The abyss is not done with you. In this world you may come to {times}, and each time it leaves its mark.")), TextFont::from_font_size(20.0).with_font(body.clone()), TextColor(MUTED), TextLayout::justify(Justify::Center), Node { max_width: px(620), ..default() }));
        overlay.spawn(Node { column_gap: px(24), margin: UiRect::top(px(12)), ..default() }).with_children(|row| {
            for (button, label) in [(ReviveButton::ComeTo, "Come to   [Enter]"), (ReviveButton::LetGo, "Let go   [Backspace]")] {
                row.spawn((button, Node { padding: UiRect::axes(px(26), px(12)), border: UiRect::all(px(1)), ..default() }, BackgroundColor(CELL), BorderColor::all(LINE)))
                    .with_child((Text::new(label), TextFont::from_font_size(22.0).with_font(display.clone()), TextColor(INK)));
            }
        });
    });
}

/// Frees the mouse for the buttons and reads the choice: a click on a button, Enter to come to
/// or Backspace to let go. Hovering a button brightens its border.
#[allow(clippy::too_many_arguments)]
fn choose(
    unconscious: Option<Res<Unconscious>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    ui_scale: Option<Res<UiScale>>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
    mut buttons: Query<(&ReviveButton, &ComputedNode, &UiGlobalTransform, &mut BorderColor)>,
    mut chosen: MessageWriter<ReviveChosen>,
) {
    if unconscious.is_none() {
        return;
    }
    if let Some(cursor) = cursor.as_deref_mut()
        && !cursor.visible
    {
        set_grab(cursor, false);
    }
    let point = window.and_then(|w| ui_cursor(&w, ui_scale.as_deref()));
    let mut hovered = None;
    for (button, node, transform, mut border) in &mut buttons {
        let over = point.is_some_and(|p| node_rect(node, transform).contains(p));
        if over {
            hovered = Some(*button);
        }
        border.set_if_neq(BorderColor::all(if over { BRIGHT } else { LINE }));
    }
    let pressed = |key| keys.as_deref().is_some_and(|k| k.just_pressed(key));
    let clicked = mouse.as_deref().is_some_and(|m| m.just_pressed(MouseButton::Left));
    let choice = if pressed(KeyCode::Enter) || pressed(KeyCode::NumpadEnter) {
        Some(ReviveButton::ComeTo)
    } else if pressed(KeyCode::Backspace) {
        Some(ReviveButton::LetGo)
    } else {
        hovered.filter(|_| clicked)
    };
    if let Some(choice) = choice {
        chosen.write(ReviveChosen(choice));
    }
}

/// Letting go ends the run as a fall always did.
fn let_go(
    mut chosen: MessageReader<ReviveChosen>,
    mut commands: Commands,
    fonts: Option<Res<UiFonts>>,
    view_models: Query<Entity, With<ViewModelCamera>>,
    overlays: Query<Entity, With<UnconsciousOverlay>>,
) {
    if !chosen.read().any(|c| c.0 == ReviveButton::LetGo) {
        return;
    }
    commands.remove_resource::<Unconscious>();
    for overlay in &overlays {
        commands.entity(overlay).despawn();
    }
    super::vitals::begin_the_fall(&mut commands, fonts.as_deref(), view_models.iter().next());
}

/// The friendly folk standing on the level, each one's faction and position on the floor
/// (world `(x, z)`), for [`come_to`].
type Friends<'w, 's> = Query<'w, 's, (&'static GlobalTransform, &'static Denizen, &'static Hostility), Without<Corpse>>;

/// The delver as [`come_to`] moves, heals and marks them.
type Delver<'w, 's> = Query<'w, 's, (Entity, &'static mut Health, &'static mut Transform, &'static mut Locomotion, Option<&'static DelverEffects>), With<Player>>;

/// Coming to: in the hall of the friendliest faction, healed, a share of the coin poorer; or
/// alone where they fell, weak and hours later. Either way with a wound that never heals.
#[allow(clippy::too_many_arguments)]
fn come_to(
    mut chosen: MessageReader<ReviveChosen>,
    mut commands: Commands,
    rules: Res<GameRules>,
    mut revives: ResMut<Revives>,
    level: Option<Res<CurrentLevel>>,
    folk: Friends,
    mut players: Delver,
    mut brains: Query<&mut Brain>,
    mut inventory: Option<ResMut<Inventory>>,
    mut clock: Option<ResMut<GameClock>>,
    mut dice: ResMut<RunDice>,
    fonts: Option<Res<UiFonts>>,
    view_models: Query<Entity, With<ViewModelCamera>>,
    overlays: Query<Entity, With<UnconsciousOverlay>>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
) {
    if !chosen.read().any(|c| c.0 == ReviveButton::ComeTo) {
        return;
    }
    let Ok((player, mut health, mut body, mut locomotion, effects)) = players.single_mut() else { return };
    let revival = &rules.revival;
    commands.remove_resource::<Unconscious>();
    for overlay in &overlays {
        commands.entity(overlay).despawn();
    }
    if let Some(cursor) = cursor.as_deref_mut() {
        set_grab(cursor, true);
    }
    revives.0 += 1;
    let mut lines = vec![];

    // Who carries them, if anyone.
    let friends: Vec<(u8, Vec2)> = folk.iter().filter(|(.., h)| !h.is_hostile()).map(|(at, denizen, _)| (denizen.person.spot.faction.0, at.translation().xz())).collect();
    let hall = level.as_deref().and_then(|level| {
        let rooms: Vec<(u8, Option<usize>)> = friends.iter().map(|(f, at)| (*f, level.0.room_at((at.x / GRID) as i32, (at.y / GRID) as i32))).collect();
        let (faction, room) = choose_hall(&rooms)?;
        let theirs: Vec<Vec2> = friends.iter().filter(|(f, _)| *f == faction).map(|(_, at)| *at).collect();
        let cell = hall_cell(&level.0, room, &friends.iter().map(|(_, at)| *at).collect::<Vec<_>>())?;
        Some((Faction(faction), cell, theirs))
    });

    match hall {
        Some((faction, (x, y), theirs)) => {
            let [wx, _, wz] = Level::world(x as f32 + 0.5, y as f32 + 0.5);
            let at = Vec2::new(wx, wz);
            // Face the nearest of them.
            let yaw = theirs.iter().min_by(|a, b| a.distance(at).total_cmp(&b.distance(at))).map_or(locomotion.yaw, |near| (-(near.x - at.x)).atan2(-(near.y - at.y)));
            body.translation = Vec3::new(wx, 0.0, wz);
            body.rotation = Quat::from_rotation_y(yaw);
            *locomotion = Locomotion { yaw, ..default() };
            health.hp = revival.hit_points(health.max, true);
            let owed = inventory.as_deref_mut().map_or(0, |inventory| {
                let owed = fee(carried_copper(inventory), revival.hall_fee);
                charge(inventory, owed);
                owed
            });
            let name = capitalised(faction.name());
            info!("{name} carry the delver to their hall; they take {owed} cp");
            lines.push(format!("{name} found you and carried you to their hall."));
            lines.push(if owed > 0 { format!("Their healers tended you. They took {} for their trouble.", coin_words(owed)) } else { "Their healers tended you. You had no coin to give, and they will remember it.".to_string() });
        }
        None => {
            health.hp = revival.hit_points(health.max, false);
            if let Some(clock) = clock.as_deref_mut() {
                clock.advance(revival.hours_alone * 3600.0);
            }
            info!("the delver comes to alone after {} hours", revival.hours_alone);
            lines.push("You come to where you fell, hours later. No one came for you.".to_string());
            lines.push("Whatever brought you down has lost interest, for now.".to_string());
        }
    }
    // Whatever was hunting them has lost them.
    for mut brain in &mut brains {
        if matches!(brain.mode, Mode::Chase { .. }) {
            let home = brain.home;
            brain.mode = Mode::Wander { target: Some(home), wait: 0.0 };
        }
    }

    // The mark it leaves.
    let mut effects = effects.map_or_else(Default::default, |e| e.0.clone());
    let taken: Vec<&str> = effects.active.iter().map(|a| a.id.as_str()).collect();
    let Wound { effect, part, line } = pick_wound(&taken, &mut dice.0);
    effects.add(&rules.effects, effect, 1.0, Some(part), fb_character::effect::Source::Fall);
    commands.entity(player).insert(DelverEffects(effects)).remove::<Slowed>();
    lines.push(line.to_string());
    lines.push(match revival.left(revives.0) {
        0 => "The abyss will not let you come to again.".to_string(),
        1 => "The abyss will let you come to once more.".to_string(),
        n => format!("The abyss will let you come to {n} more times."),
    });
    spawn_wake_card(&mut commands, fonts.as_deref(), view_models.iter().next(), &lines);
}

/// The open cell of `room` nearest its centre that is at least a cell and a half from everyone
/// standing at `folk` (world `(x, z)`), and not a stair; any open cell of the room without one.
pub fn hall_cell(level: &Level, room: usize, folk: &[Vec2]) -> Option<(i32, i32)> {
    let room = level.rooms.get(room)?;
    let (cx, cy) = room.centre();
    let cells: Vec<(i32, i32)> = (room.y as i32..(room.y + room.h) as i32).flat_map(|y| (room.x as i32..(room.x + room.w) as i32).map(move |x| (x, y))).filter(|&(x, y)| level.is_open(x, y) && level.stair_at(x, y).is_none()).collect();
    let to_centre = |&(x, y): &(i32, i32)| (x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2);
    let clear = |&(x, y): &(i32, i32)| folk.iter().all(|f| Vec2::new((x as f32 + 0.5) * GRID, (y as f32 + 0.5) * GRID).distance(*f) >= 1.5 * GRID);
    cells.iter().filter(|c| clear(c)).min_by(|a, b| to_centre(a).total_cmp(&to_centre(b))).or_else(|| cells.iter().min_by(|a, b| to_centre(a).total_cmp(&to_centre(b)))).copied()
}

/// The copper in every purse the delver carries.
fn carried_copper(inventory: &Inventory) -> u32 {
    inventory.ids().filter(|&id| inventory.is_carried(id)).filter_map(|id| inventory.item(id)?.coins).map(in_copper).sum()
}

/// Takes `owed` copper out of the purses the delver carries, one after another.
fn charge(inventory: &mut Inventory, mut owed: u32) {
    let purses: Vec<_> = inventory.ids().filter(|&id| inventory.is_carried(id) && inventory.item(id).is_some_and(|i| i.coins.is_some())).collect();
    for id in purses {
        if owed == 0 {
            break;
        }
        owed = inventory.spend(id, owed);
    }
}

/// "the Scale Court" as the start of a line.
fn capitalised(name: &str) -> String {
    let mut letters = name.chars();
    letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default()
}

fn spawn_wake_card(commands: &mut Commands, fonts: Option<&UiFonts>, camera: Option<Entity>, lines: &[String]) {
    let body = fonts.map(|f| f.body.clone()).unwrap_or_default();
    let mut root = commands.spawn((
        Name::new("Coming to"),
        WakeCard::default(),
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(BG),
        GlobalZIndex(10),
        Pickable::IGNORE,
    ));
    if let Some(camera) = camera {
        root.insert(UiTargetCamera(camera));
    }
    root.with_children(|card| {
        card.spawn((WakeWords, Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(10), padding: UiRect::axes(px(32), px(20)), ..default() }, BackgroundColor(BG.with_alpha(0.0)))).with_children(|panel| {
            for line in lines {
                panel.spawn((Text::new(line.clone()), TextFont::from_font_size(22.0).with_font(body.clone()), TextColor(BRIGHT), TextLayout::justify(Justify::Center), Node { max_width: px(680), ..default() }));
            }
        });
    });
}

/// The panel the wake card's words sit on, so they read over a bright room once the black clears.
#[derive(Component, Debug)]
struct WakeWords;

/// Holds the wake card's black a moment, clears it to the level with the words still over it on
/// a dark panel, then fades the words and the panel.
#[allow(clippy::type_complexity)]
fn fade_wake_card(
    mut commands: Commands,
    time: Res<Time>,
    mut cards: Query<(Entity, &mut WakeCard, &mut BackgroundColor), Without<WakeWords>>,
    mut panels: Query<(&mut BackgroundColor, &Children), With<WakeWords>>,
    mut words: Query<&mut TextColor>,
) {
    for (card, mut wake, mut background) in &mut cards {
        wake.age += time.delta_secs();
        let dark = 1.0 - ((wake.age - WAKE_DARK) / WAKE_FADE).clamp(0.0, 1.0);
        background.0 = BG.with_alpha(dark * 0.97);
        let shown = 1.0 - ((wake.age - WAKE_WORDS) / WAKE_FADE).clamp(0.0, 1.0);
        for (mut panel, children) in &mut panels {
            panel.0 = BG.with_alpha(0.8 * shown);
            for &child in children {
                if let Ok(mut colour) = words.get_mut(child) {
                    colour.0 = BRIGHT.with_alpha(shown);
                }
            }
        }
        if wake.age > WAKE_WORDS + WAKE_FADE {
            commands.entity(card).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Character;
    use crate::combat::vitals::{Dying, FALL_TIME, Fallen, LAST_GASP, VitalsPlugin};
    use crate::creatures::behaviour::AttackOnDelver;
    use crate::play::ScreenOpen;
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;
    use fb_creatures::Species;

    /// A delve with a delver of 10 hit points, stepped 1/60 s a frame.
    fn delve() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(1.0 / 60.0)))
            .insert_state(AppState::Delving)
            .insert_resource(RunDice(procgen_rng::Mulberry32::new(4)))
            .init_resource::<Character>()
            .init_resource::<ScreenOpen>()
            .init_resource::<Inventory>()
            .init_resource::<GameClock>()
            .add_plugins((VitalsPlugin, RevivalPlugin));
        let player = app.world_mut().spawn((Player, Transform::default(), Locomotion::default(), Health::full(10))).id();
        app.update();
        (app, player)
    }

    /// Knocks the delver to 0 and waits out the last gasp.
    fn knock_down(app: &mut App, player: Entity) {
        app.world_mut().write_message(AttackOnDelver { attacker: player, species: Some(Species::Rat), damage: 40, poison: 0, attacker_pos: Vec2::ZERO });
        for _ in 0..((LAST_GASP * 60.0) as usize + 3) {
            app.update();
        }
    }

    fn choose(app: &mut App, button: ReviveButton) {
        app.world_mut().write_message(ReviveChosen(button));
        app.update();
    }

    fn overlays(app: &mut App) -> usize {
        app.world_mut().query::<&UnconsciousOverlay>().iter(app.world()).count()
    }

    /// A level with folk standing in a room, one of them, and the room.
    fn level_with_folk() -> (Level, fb_level::folk::FolkSpot, usize) {
        let level = (1..200).map(|seed| fb_level::generate(seed, 1)).find(|l| l.folk.iter().any(|f| l.room_at(f.x as i32, f.y as i32).is_some())).expect("a level with folk in a room");
        let spot = *level.folk.iter().find(|f| level.room_at(f.x as i32, f.y as i32).is_some()).unwrap();
        let room = level.room_at(spot.x as i32, spot.y as i32).unwrap();
        (level, spot, room)
    }

    #[test]
    fn a_felled_delver_falls_unconscious_and_comes_to_alone_wounded_and_weak() {
        let (mut app, player) = delve();
        knock_down(&mut app, player);
        assert!(app.world().get_resource::<Unconscious>().is_some());
        assert!(app.world().get_resource::<Fallen>().is_none(), "not the end yet");
        assert!(app.world().get_resource::<Dying>().is_none());
        assert_eq!(overlays(&mut app), 1);
        // Waiting changes nothing: the choice is theirs.
        for _ in 0..(FALL_TIME * 60.0) as usize {
            app.update();
        }
        assert!(app.world().get_resource::<Fallen>().is_none());
        choose(&mut app, ReviveButton::ComeTo);
        assert!(app.world().get_resource::<Unconscious>().is_none());
        assert_eq!(overlays(&mut app), 0);
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 3, "a quarter of 10, rounded");
        assert_eq!(app.world().get::<Slowed>(player), None, "they can move again");
        let effects = &app.world().get::<DelverEffects>(player).unwrap().0;
        assert_eq!(effects.active.len(), 1);
        assert_eq!(effects.active[0].source, fb_character::effect::Source::Fall);
        assert!(effects.active[0].part.is_some());
        assert!((app.world().resource::<GameClock>().game_seconds() - 4.0 * 3600.0).abs() < 1.0, "hours pass");
        assert_eq!(*app.world().resource::<Revives>(), Revives(1));
        assert_eq!(app.world_mut().query::<&WakeCard>().iter(app.world()).count(), 1);
    }

    #[test]
    fn twice_in_a_world_then_the_next_fall_ends_the_run() {
        let (mut app, player) = delve();
        for _ in 0..2 {
            knock_down(&mut app, player);
            assert!(app.world().get_resource::<Unconscious>().is_some());
            choose(&mut app, ReviveButton::ComeTo);
        }
        let effects = &app.world().get::<DelverEffects>(player).unwrap().0;
        assert_eq!(effects.active.len(), 2, "two different wounds");
        assert_ne!(effects.active[0].id, effects.active[1].id);
        knock_down(&mut app, player);
        assert!(app.world().get_resource::<Unconscious>().is_none());
        assert!(app.world().get_resource::<Fallen>().is_some(), "no third time");
        // A new delve is a new world.
        for _ in 0..((FALL_TIME * 60.0) as usize + 5) {
            app.update();
        }
        assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Title);
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Delving);
        app.update();
        assert_eq!(*app.world().resource::<Revives>(), Revives(0));
    }

    #[test]
    fn letting_go_ends_the_run() {
        let (mut app, player) = delve();
        knock_down(&mut app, player);
        choose(&mut app, ReviveButton::LetGo);
        assert!(app.world().get_resource::<Unconscious>().is_none());
        assert_eq!(overlays(&mut app), 0);
        assert!(app.world().get_resource::<Fallen>().is_some());
        for _ in 0..((FALL_TIME * 60.0) as usize + 5) {
            app.update();
        }
        assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Title);
    }

    #[test]
    fn a_friendly_faction_carries_the_delver_to_its_hall_heals_them_and_takes_half_their_coin() {
        let (mut app, player) = delve();
        let (level, spot, room) = level_with_folk();
        let at = crate::folk::stand_at(&spot);
        app.world_mut().spawn((Denizen { person: crate::folk::Person::of(&spot, 1), radius: 3.0, min: Vec3::ZERO, max: Vec3::ZERO }, at, GlobalTransform::from(at)));
        app.insert_resource(CurrentLevel(level.clone()));
        let purse = fb_items::roll_item("Purse", 7).unwrap();
        let held = in_copper(purse.coins.unwrap());
        let mut inventory = Inventory::default();
        let pack = inventory.add(fb_items::roll_item("Pack", 1).unwrap());
        inventory.wear(pack, fb_items::Slot::Back).unwrap();
        let id = inventory.add(purse);
        inventory.stow(id).unwrap();
        app.insert_resource(inventory);
        knock_down(&mut app, player);
        choose(&mut app, ReviveButton::ComeTo);
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 10, "healed in full");
        let body = app.world().get::<Transform>(player).unwrap().translation;
        assert_eq!(level.room_at((body.x / GRID) as i32, (body.z / GRID) as i32), Some(room), "in their hall");
        assert!(body.xz().distance(at.translation.xz()) >= 1.5 * GRID - 0.01, "not on top of them");
        assert_eq!(carried_copper(app.world().resource::<Inventory>()), held - fee(held, 0.5));
        assert!(app.world().resource::<GameClock>().game_seconds() < 1.0, "they were tended, not left lying");
    }

    #[test]
    fn hostile_folk_carry_no_one() {
        let (mut app, player) = delve();
        let (level, spot, _) = level_with_folk();
        let at = crate::folk::stand_at(&spot);
        app.world_mut().spawn((Denizen { person: crate::folk::Person::of(&spot, 1), radius: 3.0, min: Vec3::ZERO, max: Vec3::ZERO }, Hostility::Hostile, at, GlobalTransform::from(at)));
        app.insert_resource(CurrentLevel(level));
        knock_down(&mut app, player);
        choose(&mut app, ReviveButton::ComeTo);
        assert_eq!(app.world().get::<Health>(player).unwrap().hp, 3, "alone");
        assert_eq!(app.world().get::<Transform>(player).unwrap().translation, Vec3::ZERO, "where they fell");
    }
}
