//! The simulation running in play (#229). On the fixed step, after the game clock advances,
//! the delver's needs and exhaustion step by the game time that has passed
//! ([`fb_character::ExhaustionTimes::step`], which steps the needs itself), at the activity their
//! movement says ([`delver_activity`]). Folk step too: the ones near the delver every step, the
//! far ones once a game minute by all that has passed, which is the same result because every
//! step is exact for any length.

use bevy::prelude::*;
use fb_character::{Activity, Exhaustion, ExhaustionTimes, Needs};

use crate::character::{Character, DelverNeeds, DelverStamina, Load, activity_for};
use crate::combat::swing::Swing;
use crate::exhaustion::{Exhaustion as DelverExhaustion, ExhaustionTimes as DelverTimes};
use crate::game_clock::{GameClock, advance_game_clock};
use crate::inventory::Inventory;
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, Player, ScreenOpen};
use crate::player::Locomotion;

pub struct SimulatePlugin;

impl Plugin for SimulatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (step_the_delver, step_folk).after(advance_game_clock).in_set(DelveSystems))
            .add_systems(FixedUpdate, step_stamina.in_set(DelveSystems));
    }
}

/// Folk farther than this from the delver (world units, 10 cm each) step once a game minute.
pub const NEAR_FOLK: f32 = 40.0;
/// A game minute, in game hours.
const MINUTE: f64 = 1.0 / 60.0;
/// Slower than this (units a second) the delver is standing.
const STANDING: f32 = 0.5;
/// What a ration takes off a hungry person's hunger (a stand-in for #196's routines).
const RATION: f64 = 0.4;
/// The hunger band where folk reach for a ration: Hungry.
const HUNGRY_BAND: usize = 2;

/// The delver's activity from how they are moving: standing is Resting, walking Walking, and
/// sprinting, swinging or moving burdened Exerting ([`activity_for`]).
///
/// Sleeping is not reachable yet: when resting in bed arrives, it returns
/// [`Activity::Sleeping`] here.
pub fn delver_activity(velocity: Vec2, sprinting: bool, swinging: bool, load: Load) -> Activity {
    activity_for(velocity.length() > STANDING, sprinting, swinging, load.0)
}

/// Steps the delver's needs and exhaustion by the game time since the last step. Gear's rates
/// are put on the meters only when the character or the inventory changes (or the delver is new),
/// and stay there. Steps nothing while the clock holds still (a screen is open).
#[allow(clippy::type_complexity)]
fn step_the_delver(
    clock: Res<GameClock>,
    character: Res<Character>,
    inventory: Res<Inventory>,
    input: ActionInput,
    mut last: Local<f64>,
    mut players: Query<(&Locomotion, Has<Swing>, &Load, &mut DelverNeeds, &mut DelverExhaustion, &mut DelverTimes), With<Player>>,
) {
    let now = clock.game_seconds();
    // A new delve restarts the clock below where this last stood.
    let passed = if now < *last { now } else { now - *last };
    *last = now;
    let hours = passed / 3600.0;
    let changed = character.is_changed() || inventory.is_changed();
    let mut gear = None;
    for (locomotion, swinging, load, mut needs, mut exhaustion, mut times) in &mut players {
        if changed || needs.is_added() {
            let gear = gear.get_or_insert_with(|| character.gear(&inventory));
            gear.apply_to(&mut needs.0);
        }
        if hours <= 0.0 {
            continue;
        }
        let sprinting = input.pressed(Action::Hurry);
        let activity = delver_activity(locomotion.velocity, sprinting, swinging, *load);
        let (mut n, mut e, mut t) = (needs.0, exhaustion.0, times.0);
        t.step(&mut n, &mut e, activity, hours, crate::character::modifier(character.constitution()));
        needs.0 = n;
        times.0 = t;
        // The level moves a few times in a run, so it is only marked changed when it does.
        exhaustion.set_if_neq(DelverExhaustion(e));
    }
}

/// What the delver's moving means for their stamina: sprinting while moving and not winded
/// drains it (Exerting), walking refills it slowly and standing refills it fully. Unlike
/// [`delver_activity`], swinging and walking under a load do not count: a swing takes its own
/// bite (`crate::combat::swing`), and the load only makes sprinting drain faster.
pub fn stamina_activity(velocity: Vec2, sprinting: bool, winded: bool) -> Activity {
    if velocity.length() <= STANDING {
        Activity::Resting
    } else if sprinting && !winded {
        Activity::Exerting
    } else {
        Activity::Walking
    }
}

/// Steps the delver's stamina by the real seconds of the fixed step, not the game clock: it
/// is a pool of seconds (a full one lasts 30 seconds of sprinting and refills in 8), so it
/// must not speed up or slow down with the clock's ratio. It runs on the same fixed step as
/// the needs, so a sprint is seen by one tick, and it holds still while a screen is open, as
/// the clock does. The component is written only when the pool actually changes, so a full
/// pool at rest marks nothing.
fn step_stamina(
    time: Res<Time<Fixed>>,
    screen: Res<ScreenOpen>,
    input: ActionInput,
    mut players: Query<(&Locomotion, &Load, &mut DelverStamina), With<Player>>,
) {
    let seconds = time.delta_secs_f64();
    if screen.0.is_some() || seconds <= 0.0 {
        return;
    }
    let sprinting = input.pressed(Action::Hurry);
    for (locomotion, load, mut stamina) in &mut players {
        let activity = stamina_activity(locomotion.velocity, sprinting, !stamina.0.can_exert());
        let mut next = stamina.0;
        next.step(seconds, activity, load.0);
        stamina.set_if_neq(DelverStamina(next));
    }
}

/// A person's simulation: their needs and the time they have spent in the worst of them. Folk
/// stand still, so they are always Resting.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Simulated {
    pub needs: Needs,
    pub times: ExhaustionTimes,
    pub exhaustion: Exhaustion,
    /// The game hour they were last stepped to; none until the first step sees the clock.
    pub last: Option<f64>,
}

impl Simulated {
    /// Steps by `hours` of game time resting, then eats a ration if Hungry.
    pub fn advance(&mut self, hours: f64) {
        self.times.step(&mut self.needs, &mut self.exhaustion, Activity::Resting, hours, 0);
        if self.needs.hunger.band() >= HUNGRY_BAND {
            self.needs.consume(RATION, 0.0);
        }
    }

    /// Whether they are due a step at game hour `now`: a far one waits until a game minute has
    /// passed. One that has not seen the clock yet is not (it notes the time first).
    pub fn due(&self, now: f64, far: bool) -> bool {
        self.last.is_some_and(|last| now > last && (!far || now - last >= MINUTE))
    }

    /// Brings them up to game hour `now`, by all the time since their last step.
    pub fn catch_up(&mut self, now: f64) {
        let last = self.last.replace(now).unwrap_or(now);
        if now > last {
            self.advance(now - last);
        }
    }
}

/// Steps every [`Simulated`] person: near the delver every step, far ones once a game minute.
fn step_folk(clock: Res<GameClock>, players: Query<&Transform, With<Player>>, mut folk: Query<(&Transform, &mut Simulated)>) {
    let now = clock.time.hours();
    let delver = players.iter().next().map(|t| t.translation.xz());
    for (transform, mut simulated) in &mut folk {
        let far = delver.is_some_and(|d| d.distance(transform.translation.xz()) > NEAR_FOLK);
        // Nothing is marked changed for one that is not due.
        if simulated.due(now, far) {
            simulated.catch_up(now);
        } else if simulated.last.is_none_or(|last| now < last) {
            // First seen, or a new delve has restarted the clock.
            simulated.last = Some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::state::app::StatesPlugin;
    use fb_items::Slot;

    use super::*;
    use crate::game_clock::GameClockPlugin;
    use crate::keymap::Keymap;
    use crate::launch::AppState;
    use crate::play::{Screen, ScreenOpen, run_delve_systems_while_delving};
    use crate::tuning::Tuning;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_state(AppState::Delving)
            .init_resource::<Tuning>()
            .init_resource::<ScreenOpen>()
            .init_resource::<Keymap>()
            .init_resource::<Character>()
            .init_resource::<Inventory>()
            .add_plugins((GameClockPlugin, SimulatePlugin));
        run_delve_systems_while_delving(&mut app);
        app.update();
        app.update();
        app
    }

    /// A delver walking at a steady pace.
    fn walker(app: &mut App) -> Entity {
        app.world_mut().spawn((Player, Transform::default(), Locomotion { velocity: Vec2::new(5.0, 0.0), ..default() })).id()
    }

    /// Runs `seconds` of fixed time, one second per step; at the default ratio of 12, five
    /// minutes of it are a game hour.
    fn run(app: &mut App, seconds: u32) {
        for _ in 0..seconds {
            app.world_mut().resource_mut::<Time<Fixed>>().advance_by(Duration::from_secs(1));
            app.world_mut().run_schedule(FixedUpdate);
        }
    }

    fn stamina(app: &App, who: Entity) -> fb_character::Stamina {
        app.world().get::<DelverStamina>(who).unwrap().0
    }

    fn sprinting(app: &mut App) {
        app.init_resource::<ButtonInput<KeyCode>>();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ShiftLeft);
    }

    #[test]
    fn sprinting_drains_stamina_over_thirty_seconds_and_stopping_refills_it() {
        let mut app = app();
        sprinting(&mut app);
        let delver = walker(&mut app);
        run(&mut app, 15);
        let half = stamina(&app, delver).value;
        assert!((half - 0.5).abs() < 0.02, "{half}");
        // Emptied inside the 30 seconds, then winded and walking.
        run(&mut app, 15);
        assert!(stamina(&app, delver).winded && stamina(&app, delver).value < 0.1, "{:?}", stamina(&app, delver));
        // Stop sprinting and stand: empty to full in about 8 seconds.
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::ShiftLeft);
        app.world_mut().get_mut::<Locomotion>(delver).unwrap().velocity = Vec2::ZERO;
        run(&mut app, 8);
        assert!(stamina(&app, delver).value > 0.98 && !stamina(&app, delver).winded);
    }

    #[test]
    fn stamina_follows_real_seconds_whatever_the_clock_ratio() {
        let mut app = app();
        app.world_mut().resource_mut::<Tuning>().clock.game_seconds_per_second = 120.0;
        sprinting(&mut app);
        let delver = walker(&mut app);
        run(&mut app, 15);
        assert!((stamina(&app, delver).value - 0.5).abs() < 0.02);
    }

    #[test]
    fn an_encumbered_delver_drains_twice_as_fast() {
        let mut app = app();
        sprinting(&mut app);
        let delver = walker(&mut app);
        app.world_mut().entity_mut(delver).insert(Load(fb_character::LoadBand::Encumbered));
        run(&mut app, 10);
        assert!((stamina(&app, delver).value - (1.0 - 10.0 * 2.0 / 30.0)).abs() < 0.02);
    }

    #[test]
    fn a_winded_delver_walks_instead_of_sprinting() {
        let mut app = app();
        sprinting(&mut app);
        let delver = walker(&mut app);
        app.world_mut().get_mut::<DelverStamina>(delver).unwrap().0 = fb_character::Stamina { value: 0.0, winded: true };
        run(&mut app, 4);
        // Walking refills while Hurry is still held.
        assert!(stamina(&app, delver).value > 0.2, "{:?}", stamina(&app, delver));
    }

    #[test]
    fn an_open_screen_holds_stamina_and_a_full_pool_is_not_rewritten() {
        let mut app = app();
        sprinting(&mut app);
        let delver = walker(&mut app);
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        run(&mut app, 10);
        assert_eq!(stamina(&app, delver).value, 1.0);
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        app.world_mut().get_mut::<Locomotion>(delver).unwrap().velocity = Vec2::ZERO;
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::ShiftLeft);
        run(&mut app, 1);
        app.world_mut().clear_trackers();
        run(&mut app, 3);
        assert!(!app.world().entity(delver).get_ref::<DelverStamina>().unwrap().is_changed(), "full and still marks nothing");
    }

    fn needs(app: &App, who: Entity) -> Needs {
        app.world().get::<DelverNeeds>(who).unwrap().0
    }

    fn with_affix(base: &str, seed: u32, name: &str) -> fb_items::Item {
        let mut item = fb_items::roll_item(base, seed).unwrap();
        let affix = fb_items::tables::affixes().iter().find(|a| a.name == name).unwrap();
        affix.apply_to_stats(&mut item.profile.stats);
        item.affixes.push(affix);
        item
    }

    #[test]
    fn a_delver_walking_for_a_game_hour_is_one_sixteenth_hungry() {
        let mut app = app();
        let delver = walker(&mut app);
        run(&mut app, 300);
        let hunger = needs(&app, delver).hunger.value;
        assert!((hunger - 1.0 / 16.0).abs() < 1e-3, "{hunger}");
    }

    #[test]
    fn standing_is_resting_and_sprinting_or_swinging_is_exerting() {
        let still = Vec2::ZERO;
        let going = Vec2::new(5.0, 0.0);
        let light = Load::default();
        assert_eq!(delver_activity(still, true, true, light), Activity::Resting);
        assert_eq!(delver_activity(going, false, false, light), Activity::Walking);
        assert_eq!(delver_activity(going, true, false, light), Activity::Exerting);
        assert_eq!(delver_activity(going, false, true, light), Activity::Exerting);
        assert_eq!(delver_activity(going, false, false, Load(fb_character::LoadBand::Burdened)), Activity::Exerting);
    }

    #[test]
    fn holding_hurry_while_walking_burns_twice_as_fast() {
        let mut app = app();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ShiftLeft);
        let delver = walker(&mut app);
        run(&mut app, 300);
        let hunger = needs(&app, delver).hunger.value;
        assert!((hunger - 2.0 / 16.0).abs() < 1e-3, "{hunger}");
    }

    #[test]
    fn standing_still_rests() {
        let mut app = app();
        let delver = app.world_mut().spawn((Player, Locomotion::default())).id();
        run(&mut app, 300);
        let hunger = needs(&app, delver).hunger.value;
        assert!((hunger - 1.0 / 32.0).abs() < 1e-3, "{hunger}");
    }

    #[test]
    fn a_belt_of_plenty_slows_hunger_by_85_hundredths() {
        let mut app = app();
        let delver = walker(&mut app);
        {
            let mut inventory = app.world_mut().resource_mut::<Inventory>();
            let belt = inventory.add(with_affix("Belt", 2, "of Plenty"));
            inventory.wear(belt, Slot::Belt).unwrap();
        }
        run(&mut app, 300);
        let hunger = needs(&app, delver).hunger.value;
        assert!((hunger - 0.85 / 16.0).abs() < 1e-3, "{hunger}");
    }

    #[test]
    fn a_starving_delver_gains_exhaustion_after_three_days_with_a_modifier_of_zero() {
        let mut app = app();
        app.world_mut().resource_mut::<Character>().attributes.constitution = 10;
        // An hour a step.
        app.world_mut().resource_mut::<Tuning>().clock.game_seconds_per_second = 3600.0;
        let delver = walker(&mut app);
        {
            let mut n = app.world_mut().get_mut::<DelverNeeds>(delver).unwrap();
            n.0.hunger.value = 1.0;
            // Only hunger counts here.
            n.0.thirst.rate_per_hour = 0.0;
            n.0.sleepiness.rate_per_hour = 0.0;
        }
        run(&mut app, 71);
        assert_eq!(app.world().get::<DelverExhaustion>(delver).unwrap().0.level, 0);
        run(&mut app, 2);
        assert_eq!(app.world().get::<DelverExhaustion>(delver).unwrap().0.level, 1);
    }

    #[test]
    fn an_open_screen_freezes_the_needs() {
        let mut app = app();
        let delver = walker(&mut app);
        app.world_mut().resource_mut::<ScreenOpen>().0 = Some(Screen::Inventory);
        run(&mut app, 300);
        assert_eq!(needs(&app, delver).hunger.value, 0.0);
        app.world_mut().resource_mut::<ScreenOpen>().0 = None;
        run(&mut app, 300);
        assert!(needs(&app, delver).hunger.value > 0.06);
    }

    #[test]
    fn a_new_delve_starts_with_fresh_needs() {
        let mut app = app();
        let first = walker(&mut app);
        run(&mut app, 300);
        assert!(needs(&app, first).hunger.value > 0.06);
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Title);
        app.update();
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Delving);
        app.update();
        assert!(app.world().get_entity(first).is_err(), "the old delver is gone");
        let second = walker(&mut app);
        assert_eq!(needs(&app, second), Needs::default());
        run(&mut app, 30);
        let hunger = needs(&app, second).hunger.value;
        assert!(hunger > 0.0 && hunger < 0.01, "only the new run's time: {hunger}");
    }

    fn folk_at(app: &mut App, x: f32) -> Entity {
        app.world_mut().spawn((Transform::from_xyz(x, 0.0, 0.0), Simulated::default())).id()
    }

    #[test]
    fn far_folk_stepped_by_the_minute_match_near_folk_stepped_every_step() {
        let mut app = app();
        walker(&mut app);
        let near = folk_at(&mut app, 5.0);
        let far = folk_at(&mut app, 200.0);
        // Two game hours.
        run(&mut app, 600);
        // A far one is up to a minute behind; coming near, it steps by everything it owes.
        app.world_mut().get_mut::<Transform>(far).unwrap().translation.x = 5.0;
        run(&mut app, 1);
        let (near, far) = (*app.world().get::<Simulated>(near).unwrap(), *app.world().get::<Simulated>(far).unwrap());
        for (a, b) in [(near.needs.hunger, far.needs.hunger), (near.needs.thirst, far.needs.thirst), (near.needs.sleepiness, far.needs.sleepiness)] {
            assert!(a.value > 0.0);
            assert!((a.value - b.value).abs() < 1e-4, "{a:?} against {b:?}");
        }
    }

    #[test]
    fn far_folk_wait_for_their_minute() {
        let mut app = app();
        walker(&mut app);
        let far = folk_at(&mut app, 200.0);
        run(&mut app, 4);
        assert_eq!(app.world().get::<Simulated>(far).unwrap().needs.thirst.value, 0.0, "48 game seconds");
        run(&mut app, 2);
        assert!(app.world().get::<Simulated>(far).unwrap().needs.thirst.value > 0.0);
    }

    #[test]
    fn a_hungry_person_eats_a_ration_and_is_fed() {
        let mut app = app();
        walker(&mut app);
        let folk = folk_at(&mut app, 5.0);
        app.world_mut().get_mut::<Simulated>(folk).unwrap().needs.hunger.value = 0.5;
        run(&mut app, 2);
        let hunger = app.world().get::<Simulated>(folk).unwrap().needs.hunger;
        assert_eq!(hunger.word(), "Fed", "{hunger:?}");
        assert!(hunger.value > 0.09, "a ration takes 0.4: {}", hunger.value);
    }
}
