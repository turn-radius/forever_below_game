//! The simulation running in play (#229). On the fixed step, after the game clock advances,
//! the delver's needs and exhaustion step by the game time that has passed
//! ([`fb_character::ExhaustionTimes::step`], which steps the needs itself), at the activity their
//! movement says ([`delver_activity`]). Folk step too: the ones near the delver every step, the
//! far ones once a game minute by all that has passed, which is the same result because every
//! step is exact for any length.

use bevy::prelude::*;
use fb_character::rules::Rules;
use fb_character::{Activity, Exhaustion, ExhaustionTimes, Needs};

use crate::character::{Character, DelverMana, DelverEffects, DelverNeeds, DelverRecovery, DelverStamina, Load, activity_for};
use crate::combat::Health;
use crate::tuning::Tuning;
use crate::combat::swing::Swing;
use crate::exhaustion::{Exhaustion as DelverExhaustion, ExhaustionTimes as DelverTimes};
use crate::game_clock::{GameClock, advance_game_clock};
use crate::inventory::Inventory;
use crate::keymap::{Action, ActionInput};
use crate::play::{DelveSystems, Player, ScreenOpen};
use crate::rules::GameRules;
use crate::player::Locomotion;

pub struct SimulatePlugin;

impl Plugin for SimulatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameRules>().add_systems(FixedUpdate, (step_the_delver, step_folk).after(advance_game_clock).in_set(DelveSystems))
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
pub fn delver_activity(rules: &fb_character::rules::Rules, velocity: Vec2, sprinting: bool, swinging: bool, load: Load) -> Activity {
    activity_for(velocity.length() > STANDING, sprinting, swinging, load.0.band_with(rules))
}

/// Steps the delver's needs and exhaustion by the game time since the last step, and mends
/// their hit points while they are fed, watered and unpoisoned ([`fb_character::recovery`], at
/// the tuning's `recovery.minutes_per_hit_point`). The rules' rates and gear's are put on the
/// meters only when the character, the inventory or the rules change (or the delver is new),
/// and stay there. Steps nothing while the clock holds still (a screen is open).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn step_the_delver(
    clock: Res<GameClock>,
    character: Res<Character>,
    inventory: Res<Inventory>,
    rules: Res<GameRules>,
    tuning: Res<Tuning>,
    input: ActionInput,
    mut last: Local<f64>,
    mut players: Query<(Entity, Option<&Transform>, &Locomotion, Has<Swing>, &Load, &mut DelverNeeds, &mut DelverExhaustion, &mut DelverTimes, &mut DelverMana, Option<&mut DelverEffects>, &mut DelverRecovery, Option<&mut Health>, Has<crate::combat::guard::Guard>), With<Player>>,
    fires: Query<(&Transform, &crate::level::FixedLight), Without<Player>>,
    level: Option<Res<crate::play::CurrentLevel>>,
    mut wearing_down: Local<f64>,
    mut commands: Commands,
) {
    let now = clock.game_seconds();
    // A new delve restarts the clock below where this last stood.
    let passed = if now < *last { now } else { now - *last };
    *last = now;
    let hours = passed / 3600.0;
    let rules_changed = rules.is_changed() && !rules.is_added();
    let changed = character.is_changed() || inventory.is_changed() || rules_changed;
    let mut gear = None;
    let mut exposures = None;
    for (player, at, locomotion, swinging, load, mut needs, mut exhaustion, mut times, mut mana, effects, mut recovery, health, guarding) in &mut players {
        if changed || needs.is_added() {
            let gear = gear.get_or_insert_with(|| character.gear(&inventory));
            // The rates a meter is made with are the defaults', so they are only put on again
            // when the rules change or a new delver is made under rules that are not the defaults.
            if rules_changed || (needs.is_added() && rules.needs != fb_character::rules::defaults().needs) {
                rules.apply_to(&mut needs.0);
            }
            gear.apply_to(&mut needs.0);
            // The pool follows the character's casting ability, a new delver's starting full.
            let max = character.mana_pool_with(&rules);
            if mana.0.max != max {
                let mut pool = mana.0;
                if mana.is_added() {
                    pool = fb_character::Mana::full(max);
                } else {
                    pool.resize(max);
                }
                mana.0 = pool;
            }
        }
        if hours <= 0.0 {
            continue;
        }
        // No sprint with a shield raised (`crate::player::walk`).
        let sprinting = input.pressed(Action::Sprint) && !guarding;
        let activity = delver_activity(&rules, locomotion.velocity, sprinting, swinging, *load);
        let (mut n, mut e, mut t) = (needs.0, exhaustion.0, times.0);
        // Gear's rates and the effects' on them (the cold makes hunger rise faster), put on
        // afresh each step so an effect that ends takes its share with it.
        let worn = gear.get_or_insert_with(|| character.gear(&inventory));
        match effects.as_deref() {
            Some(on) => on.0.apply_rates(&rules.effects, &mut n, worn),
            None => fb_character::effect::Effects::default().apply_rates(&rules.effects, &mut n, worn),
        }
        t.step_with(&rules, &mut n, &mut e, activity, hours, crate::character::modifier(character.constitution()));
        needs.0 = n;
        times.0 = t;
        // Only written when the pool changes, so a full one marks nothing.
        let mut pool = mana.0;
        pool.step_with(&rules, hours, activity);
        mana.set_if_neq(DelverMana(pool));
        // The level moves a few times in a run, so it is only marked changed when it does.
        exhaustion.set_if_neq(DelverExhaustion(e));
        // Fed, watered and with nothing stopping it (venom does), they mend; the effects are read
        // before this step wears them.
        if let Some(mut health) = health.filter(|h| !h.is_down()) {
            let stopped = effects.as_deref().is_some_and(|e| e.0.stops_recovery(&rules.effects));
            let recovering = fb_character::recovery::recovers(&n, stopped);
            let mut next = recovery.0;
            let points = gear.get_or_insert_with(|| character.gear(&inventory)).recovery;
            let gained = next.step(hours, tuning.recovery.minutes_for(points), recovering, health.hp, health.max);
            recovery.set_if_neq(DelverRecovery(next));
            if gained > 0 {
                health.heal(gained);
            }
            // Effects that wear the body down (freezing) take hit points by game time, the
            // fractions kept until they make a whole one.
            *wearing_down += effects.as_deref().map_or(0.0, |e| e.0.hit_points_per_hour(&rules.effects)) * hours;
            if *wearing_down >= 1.0 {
                let lost = wearing_down.floor();
                *wearing_down -= lost;
                health.hurt(lost as u32);
            }
        }
        // Effects run their course by game time under what keeps acting on the delver (a smoky
        // light in hand), and the component comes on with the first and goes with the last.
        let exposures = exposures.get_or_insert_with(|| {
            let around = surroundings(&character, &inventory, &rules, at.map_or(Vec3::NAN, |t| t.translation), &fires, level.as_deref().map_or(1, |l| l.0.depth));
            crate::exposure::exposures(&inventory, &rules.effects, &around)
        });
        match effects {
            Some(mut effects) => {
                effects.0.step(&rules.effects, hours, exposures);
                if effects.0.active.is_empty() {
                    commands.entity(player).remove::<DelverEffects>();
                }
            }
            None if !exposures.is_empty() => {
                let mut fresh = fb_character::effect::Effects::default();
                fresh.step(&rules.effects, hours, exposures);
                if !fresh.active.is_empty() {
                    commands.entity(player).insert(DelverEffects(fresh));
                }
            }
            None => {}
        }
    }
}

/// The cold and the fire as they reach a delver standing at `at` on level `depth`: their worn
/// warmth and any lit flame in hand weighed against their need and the level's chill
/// ([`fb_character::rules::WarmthRules`]), and whether a brazier's warmth reaches them.
fn surroundings(character: &Character, inventory: &Inventory, rules: &GameRules, at: Vec3, fires: &Query<(&Transform, &crate::level::FixedLight), Without<Player>>, depth: u32) -> crate::exposure::Surroundings {
    let warmth = &rules.warmth;
    let by_fire = fires.iter().any(|(fire, kind)| matches!(kind, crate::level::FixedLight::Brazier | crate::level::FixedLight::Campfire) && fire.translation.xz().distance(at.xz()) as f64 <= warmth.brazier_reach);
    let flame = inventory.equipped().iter().any(|(slot, item)| matches!(slot, fb_items::Slot::Mainhand | fb_items::Slot::Offhand) && item.shape.lit() == Some(true));
    let fire = if by_fire { warmth.brazier } else { 0.0 } + if flame { warmth.held_flame } else { 0.0 };
    let worn = character.loadout(inventory).warmth().value();
    crate::exposure::Surroundings {
        shortfall: warmth.shortfall(character.warmth_need() as f64, depth, worn, fire),
        by_fire,
        cold_blooded: character.folk == fb_body::Folk::Lizardfolk,
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
    rules: Res<GameRules>,
    input: ActionInput,
    mut players: Query<(&Locomotion, &Load, &mut DelverStamina, Has<crate::combat::guard::Guard>, Option<&crate::character::GearStamina>), With<Player>>,
) {
    let seconds = time.delta_secs_f64();
    if screen.0.is_some() || seconds <= 0.0 {
        return;
    }
    let sprinting = input.pressed(Action::Sprint);
    for (locomotion, load, mut stamina, guarding, gear) in &mut players {
        let activity = stamina_activity(locomotion.velocity, sprinting && !guarding, !stamina.0.can_exert());
        let mut next = stamina.0;
        let gear = gear.copied().unwrap_or_default();
        next.step_geared(&rules, seconds, activity, load.0, gear.refill as f64, gear.cost as f64);
        stamina.set_if_neq(DelverStamina(next));
    }
}

/// A person's simulation: their needs and the time they have spent in the worst of them. Folk
/// stand still, so they are always Resting.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
pub struct Simulated {
    #[reflect(remote = crate::inspect::remote::NeedsDef)]
    pub needs: Needs,
    #[reflect(remote = crate::inspect::remote::ExhaustionTimesDef)]
    pub times: ExhaustionTimes,
    #[reflect(remote = crate::inspect::remote::ExhaustionDef)]
    pub exhaustion: Exhaustion,
    /// The game hour they were last stepped to; none until the first step sees the clock.
    pub last: Option<f64>,
}

impl Simulated {
    /// Steps by `hours` of game time resting, then eats a ration if Hungry.
    pub fn advance(&mut self, hours: f64) {
        self.advance_with(fb_character::rules::defaults(), hours);
    }

    /// [`Simulated::advance`] under `rules`, whose rates go on their meters first.
    pub fn advance_with(&mut self, rules: &Rules, hours: f64) {
        rules.apply_to(&mut self.needs);
        self.times.step_with(rules, &mut self.needs, &mut self.exhaustion, Activity::Resting, hours, 0);
        if self.needs.hunger.band_with(rules) >= HUNGRY_BAND {
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
        self.catch_up_with(fb_character::rules::defaults(), now);
    }

    /// [`Simulated::catch_up`] under `rules`.
    pub fn catch_up_with(&mut self, rules: &Rules, now: f64) {
        let last = self.last.replace(now).unwrap_or(now);
        if now > last {
            self.advance_with(rules, now - last);
        }
    }
}

/// Steps every [`Simulated`] person: near the delver every step, far ones once a game minute.
fn step_folk(clock: Res<GameClock>, rules: Res<GameRules>, players: Query<&Transform, With<Player>>, mut folk: Query<(&Transform, &mut Simulated)>) {
    let now = clock.time.hours();
    let delver = players.iter().next().map(|t| t.translation.xz());
    for (transform, mut simulated) in &mut folk {
        let far = delver.is_some_and(|d| d.distance(transform.translation.xz()) > NEAR_FOLK);
        // Nothing is marked changed for one that is not due.
        if simulated.due(now, far) {
            simulated.catch_up_with(&rules, now);
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
        // Warm enough that the cold keeps out of tests about other things: a bare delver with
        // no torch is short of warmth even on the first level.
        app.world_mut().resource_mut::<GameRules>().0.warmth.comfort = 100.0;
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
    fn gear_refills_stamina_faster_and_slows_the_sprint_drain() {
        use crate::character::GearStamina;
        let refilled = |refill: f32| {
            let mut app = app();
            let delver = app.world_mut().spawn((Player, Transform::default(), Locomotion::default(), GearStamina { refill, cost: 1.0 })).id();
            app.world_mut().get_mut::<DelverStamina>(delver).unwrap().0 = fb_character::Stamina { value: 0.0, winded: true };
            run(&mut app, 7);
            stamina(&app, delver).value
        };
        assert!(refilled(1.0) < 0.9, "8 s to fill at the usual rate");
        assert!(refilled(1.3) > 0.98, "of Second Wind fills in 8 / 1.3 s");
        let mut app = app();
        sprinting(&mut app);
        let delver = app.world_mut().spawn((Player, Transform::default(), Locomotion { velocity: Vec2::new(5.0, 0.0), ..default() }, GearStamina { refill: 1.0, cost: 0.8 })).id();
        run(&mut app, 15);
        assert!((stamina(&app, delver).value - (1.0 - 15.0 * 0.8 / 30.0)).abs() < 0.02);
    }

    #[test]
    fn the_cold_makes_hunger_rise_faster_and_its_share_goes_when_it_does() {
        let hunger_after = |cold: bool| {
            let mut app = app();
            let delver = walker(&mut app);
            // Exactly at their need, so the cold put on them fades only at its own pace.
            app.world_mut().resource_mut::<GameRules>().0.warmth.comfort = 6.0;
            if cold {
                let mut on = fb_character::effect::Effects::default();
                on.add(&fb_character::rules::defaults().effects, crate::exposure::COLD, 1.0, None, fb_character::effect::Source::Surroundings);
                app.world_mut().entity_mut(delver).insert(DelverEffects(on));
            }
            run(&mut app, 30);
            app.world().get::<DelverNeeds>(delver).unwrap().0.hunger.value
        };
        let (warm, cold) = (hunger_after(false), hunger_after(true));
        assert!(cold > warm * 1.2, "the cold made them hungrier: {warm} warm, {cold} cold");
    }

    #[test]
    fn venom_wears_off_after_a_game_hour_and_its_component_goes() {
        let mut app = app();
        let delver = walker(&mut app);
        app.world_mut().entity_mut(delver).insert(DelverEffects::poisoned_for(fb_character::rules::defaults(), 1.0));
        run(&mut app, 150);
        let half = app.world().get::<DelverEffects>(delver).expect("still poisoned half an hour on").poisoned_hours().expect("poisoned");
        assert!((half - 0.5).abs() < 0.01, "{half:?}");
        run(&mut app, 151);
        assert!(app.world().get::<DelverEffects>(delver).is_none(), "worn off after five real minutes");
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
        app.world_mut().entity_mut(delver).insert(Load::of(165.0, 165.0));
        run(&mut app, 10);
        assert!((stamina(&app, delver).value - (1.0 - 10.0 * 2.0 / 30.0)).abs() < 0.02);
    }

    #[test]
    fn a_load_between_the_edges_drains_by_the_curve() {
        let mut app = app();
        sprinting(&mut app);
        let delver = walker(&mut app);
        // 110 lb of 165 is t = 0.5: 1.25 times as fast, with no step at the band's edge.
        app.world_mut().entity_mut(delver).insert(Load::of(110.0, 165.0));
        run(&mut app, 10);
        assert!((stamina(&app, delver).value - (1.0 - 10.0 * 1.25 / 30.0)).abs() < 0.02);
    }

    #[test]
    fn a_winded_delver_walks_instead_of_sprinting() {
        let mut app = app();
        sprinting(&mut app);
        let delver = walker(&mut app);
        app.world_mut().get_mut::<DelverStamina>(delver).unwrap().0 = fb_character::Stamina { value: 0.0, winded: true };
        run(&mut app, 4);
        // Walking refills while Sprint is still held.
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

    fn mana(app: &App, who: Entity) -> fb_character::Mana {
        app.world().get::<DelverMana>(who).unwrap().0
    }

    fn cast_as(app: &mut App, class: crate::character::Class, intelligence: i32, wisdom: i32) {
        let mut character = app.world_mut().resource_mut::<Character>();
        character.class = class;
        character.attributes.intelligence = intelligence;
        character.attributes.wisdom = wisdom;
    }

    #[test]
    fn a_mage_with_int_16_has_a_larger_pool_than_one_with_int_10() {
        let pool = |intelligence| {
            let mut app = app();
            cast_as(&mut app, crate::character::Class::Mage, intelligence, 10);
            let delver = walker(&mut app);
            run(&mut app, 1);
            mana(&app, delver)
        };
        let (clever, average) = (pool(16), pool(10));
        assert!(clever.max > average.max, "{clever:?} against {average:?}");
        assert_eq!((clever.points, average.points), (clever.max, average.max), "a new delver starts full");
    }

    #[test]
    fn a_clerics_pool_follows_wis_and_fighters_have_a_small_one() {
        let max = |class, intelligence, wisdom| {
            let mut app = app();
            cast_as(&mut app, class, intelligence, wisdom);
            let delver = walker(&mut app);
            run(&mut app, 1);
            mana(&app, delver).max
        };
        use crate::character::Class;
        assert!(max(Class::Cleric, 10, 16) > max(Class::Cleric, 10, 10));
        assert_eq!(max(Class::Cleric, 18, 10), max(Class::Cleric, 10, 10), "INT does not size a cleric's pool");
        assert_eq!(max(Class::Fighter, 18, 18), 4.0);
    }

    #[test]
    fn resting_for_eight_game_hours_refills_an_empty_pool_and_walking_is_slower() {
        let mut app = app();
        app.world_mut().resource_mut::<Tuning>().clock.game_seconds_per_second = 3600.0;
        cast_as(&mut app, crate::character::Class::Mage, 16, 10);
        let resting = app.world_mut().spawn((Player, Locomotion::default())).id();
        run(&mut app, 1);
        app.world_mut().get_mut::<DelverMana>(resting).unwrap().0.points = 0.0;
        run(&mut app, 4);
        let half = mana(&app, resting);
        assert!((half.fraction() - 0.5).abs() < 0.01, "{half:?}");
        run(&mut app, 4);
        assert!((mana(&app, resting).fraction() - 1.0).abs() < 1e-6);
        // Walking refills at a quarter of that.
        let walking = walker(&mut app);
        run(&mut app, 1);
        app.world_mut().get_mut::<DelverMana>(walking).unwrap().0.points = 0.0;
        run(&mut app, 4);
        assert!(mana(&app, walking).fraction() < 0.2, "{:?}", mana(&app, walking));
    }

    #[test]
    fn a_full_pool_is_not_rewritten_and_a_changed_character_resizes_it() {
        let mut app = app();
        cast_as(&mut app, crate::character::Class::Mage, 16, 10);
        let delver = walker(&mut app);
        run(&mut app, 2);
        app.world_mut().clear_trackers();
        run(&mut app, 3);
        assert!(!app.world().entity(delver).get_ref::<DelverMana>().unwrap().is_changed(), "full marks nothing");
        app.world_mut().get_mut::<DelverMana>(delver).unwrap().0.points = 12.5;
        cast_as(&mut app, crate::character::Class::Mage, 18, 10);
        run(&mut app, 1);
        let pool = mana(&app, delver);
        assert_eq!(pool.max, 30.0);
        assert!((pool.fraction() - 0.5).abs() < 0.01, "kept half full: {pool:?}");
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
    fn a_fed_and_watered_delver_mends_a_point_every_ten_game_minutes_up_to_the_maximum() {
        let mut app = app();
        let delver = walker(&mut app);
        app.world_mut().entity_mut(delver).insert(Health { hp: 3, max: 8 });
        // Ten game minutes are fifty real seconds at the usual clock.
        run(&mut app, 45);
        assert_eq!(app.world().get::<Health>(delver).unwrap().hp, 3);
        run(&mut app, 6);
        assert_eq!(app.world().get::<Health>(delver).unwrap().hp, 4);
        run(&mut app, 600);
        assert_eq!(app.world().get::<Health>(delver).unwrap().hp, 8, "never above the maximum");
    }

    #[test]
    fn a_hungry_thirsty_or_poisoned_delver_does_not_mend() {
        for case in 0..3 {
            let mut app = app();
            let delver = walker(&mut app);
            app.world_mut().entity_mut(delver).insert(Health { hp: 3, max: 8 });
            app.world_mut().run_schedule(FixedUpdate);
            let mut entity = app.world_mut().entity_mut(delver);
            match case {
                0 => entity.get_mut::<DelverNeeds>().unwrap().0.hunger.value = 0.3,
                1 => entity.get_mut::<DelverNeeds>().unwrap().0.thirst.value = 0.3,
                _ => {
                    entity.insert(DelverEffects::poisoned_for(fb_character::rules::defaults(), 1.0));
                }
            }
            run(&mut app, 120);
            assert_eq!(app.world().get::<Health>(delver).unwrap().hp, 3, "case {case}");
        }
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
        let rules = fb_character::rules::defaults();
        assert_eq!(delver_activity(rules, still, true, true, light), Activity::Resting);
        assert_eq!(delver_activity(rules, going, false, false, light), Activity::Walking);
        assert_eq!(delver_activity(rules, going, true, false, light), Activity::Exerting);
        assert_eq!(delver_activity(rules, going, false, true, light), Activity::Exerting);
        assert_eq!(delver_activity(rules, going, false, false, Load::sample(fb_character::LoadBand::Burdened)), Activity::Exerting);
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
    fn editing_the_rules_changes_the_needs_rates_in_a_running_game() {
        let mut app = app();
        let delver = walker(&mut app);
        app.world_mut().resource_mut::<Tuning>().clock.game_seconds_per_second = 3600.0;
        run(&mut app, 1);
        let hunger = |app: &App| app.world().get::<DelverNeeds>(delver).unwrap().0.hunger.rate_per_hour;
        assert_eq!(hunger(&app), 1.0 / 16.0);
        app.world_mut().resource_mut::<GameRules>().needs.hunger_hours_to_full = 8.0;
        run(&mut app, 1);
        assert_eq!(hunger(&app), 1.0 / 8.0);
        let before = app.world().get::<DelverNeeds>(delver).unwrap().0.hunger.value;
        run(&mut app, 1);
        let after = app.world().get::<DelverNeeds>(delver).unwrap().0.hunger.value;
        assert!((after - before - 1.0 / 8.0).abs() < 1e-9, "{before} -> {after}");
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
