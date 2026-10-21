use super::*;
use crate::folk_routine::{Phase, begin_routines};
use crate::inventory::Toast;
use fb_faction::{Tier, Trade};
use fb_level::FolkSpot;

/// An app with the talk, the routines' start and seed 1207's first level, no drawing.
fn talk_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin, bevy::state::app::StatesPlugin))
        .insert_state(crate::launch::AppState::Delving)
        .init_resource::<Keymap>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ScreenOpen>()
        .init_resource::<Toast>()
        .init_resource::<WorldRecord>()
        .init_resource::<AimedFolk>()
        .init_resource::<crate::tuning::Tuning>()
        .insert_resource(CurrentLevel(fb_level::generate(1207, 1)));
    crate::play::run_delve_systems_while_delving(&mut app);
    app.add_plugins(TalkPlugin).add_systems(Update, begin_routines.in_set(DelveSystems));
    app.update();
    app
}

/// A stand-in for the folk at `index`, a trade given, with a routine.
fn stand_in(app: &mut App, index: usize, trade: Option<Trade>, faction: Option<fb_faction::Faction>) -> Entity {
    let mut spot: FolkSpot = app.world().resource::<CurrentLevel>().0.folk[index];
    spot.trade = trade;
    if let Some(faction) = faction {
        spot.faction = faction;
    }
    let level = app.world().resource::<CurrentLevel>().0.clone();
    let person = crate::folk::Person::of(&spot, level.faction(spot.faction), 1);
    let denizen = Denizen { person, radius: 3.0, min: Vec3::new(-2.0, 0.0, -2.0), max: Vec3::new(2.0, 17.0, 2.0) };
    let entity = app.world_mut().spawn((denizen, crate::folk::stand_at(&spot))).id();
    app.update();
    entity
}

/// Looks at `folk` and presses Talk for a frame.
fn talk_to(app: &mut App, folk: Entity) {
    app.world_mut().resource_mut::<AimedFolk>().0 = Some(folk);
    press(app, KeyCode::KeyF);
}

fn press(app: &mut App, key: KeyCode) {
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
    app.update();
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.release(key);
    keys.clear();
    app.update();
}

fn faction_of(app: &App, folk: Entity) -> fb_faction::Faction {
    app.world().get::<Denizen>(folk).unwrap().person.spot.faction
}

fn standing(app: &mut App, folk: Entity, amount: i32) {
    let faction = faction_of(app, folk);
    app.world_mut().resource_mut::<WorldRecord>().standings.shift(faction, amount);
}

fn toast(app: &App) -> String {
    app.world().resource::<Toast>().text.clone()
}

fn phase(app: &App, folk: Entity) -> Phase {
    app.world().get::<Routine>(folk).unwrap().phase
}

fn smith() -> Option<Trade> {
    Some(Trade { profession: Profession::Weaponsmith, tier: Tier::Apprentice, style: None })
}

#[test]
fn talking_to_a_neutral_folk_opens_the_panel_and_holds_their_routine() {
    let mut app = talk_app();
    let folk = stand_in(&mut app, 0, smith(), None);
    assert_ne!(phase(&app, folk), Phase::Held);
    talk_to(&mut app, folk);
    assert!(app.world().resource::<ScreenOpen>().is(Screen::Talk));
    assert_eq!(phase(&app, folk), Phase::Held);
    let talk = app.world().resource::<Talk>();
    assert_eq!(talk.folk, Some(folk));
    assert!(!talk.greeting.is_empty());
    let labels: Vec<_> = talk.topics.iter().map(|t| t.label()).collect();
    assert_eq!(labels, ["Who are you?", "Your trade", "Barter", "Commission", "This level", "The way out"]);
}

#[test]
fn closing_the_panel_lets_the_folk_go() {
    let mut app = talk_app();
    let folk = stand_in(&mut app, 0, smith(), None);
    talk_to(&mut app, folk);
    assert_eq!(phase(&app, folk), Phase::Held);
    // The Talk key again leaves.
    press(&mut app, KeyCode::KeyF);
    assert!(!app.world().resource::<ScreenOpen>().is(Screen::Talk));
    assert_ne!(phase(&app, folk), Phase::Held);
    assert_eq!(app.world().resource::<Talk>().folk, None);
    // Esc, which the screens' own system turns into a close, releases them too.
    talk_to(&mut app, folk);
    assert_eq!(phase(&app, folk), Phase::Held);
    app.world_mut().resource_mut::<ScreenOpen>().0 = None;
    app.update();
    assert_ne!(phase(&app, folk), Phase::Held);
}

#[test]
fn a_hostile_folk_spits_and_no_panel_opens() {
    let mut app = talk_app();
    let folk = stand_in(&mut app, 0, None, None);
    standing(&mut app, folk, -100);
    talk_to(&mut app, folk);
    let name = app.world().get::<Denizen>(folk).unwrap().person.name.clone();
    assert_eq!(toast(&app), format!("{name} spits at your feet."));
    assert_eq!(app.world().resource::<ScreenOpen>().0, None);
    assert_ne!(phase(&app, folk), Phase::Held);
}

#[test]
fn a_folk_who_has_turned_on_the_delver_spits_whatever_their_faction_thinks() {
    let mut app = talk_app();
    let folk = stand_in(&mut app, 0, None, None);
    app.world_mut().entity_mut(folk).insert(Hostility::Hostile);
    talk_to(&mut app, folk);
    assert!(toast(&app).ends_with("spits at your feet."), "{}", toast(&app));
    assert_eq!(app.world().resource::<ScreenOpen>().0, None);
}

#[test]
fn a_wary_folk_says_a_line_and_no_panel_opens() {
    let mut app = talk_app();
    let folk = stand_in(&mut app, 0, smith(), None);
    standing(&mut app, folk, -30);
    assert_eq!(app.world().resource::<WorldRecord>().standings.band(faction_of(&app, folk)), Band::Wary);
    talk_to(&mut app, folk);
    let said = toast(&app);
    assert!(said.starts_with('\u{201c}') && said.ends_with('\u{201d}') && said.len() > 6, "{said}");
    assert_eq!(app.world().resource::<ScreenOpen>().0, None);
    assert_eq!(app.world().resource::<Talk>().folk, None);
    assert_ne!(phase(&app, folk), Phase::Held);
}

#[test]
fn a_friendly_folk_talks_and_a_folk_with_no_trade_has_no_trade_topics() {
    let mut app = talk_app();
    let folk = stand_in(&mut app, 0, None, None);
    standing(&mut app, folk, 20);
    talk_to(&mut app, folk);
    assert!(app.world().resource::<ScreenOpen>().is(Screen::Talk));
    let talk = app.world().resource::<Talk>();
    let labels: Vec<_> = talk.topics.iter().map(|t| t.label()).collect();
    assert_eq!(labels, ["Who are you?", "This level", "The way out"]);
    let warm = lines::grammar().symbols["greet_friendly"].iter().any(|rule| {
        let head = rule.split('#').next().unwrap_or("");
        !head.is_empty() && talk.greeting.starts_with(head)
    });
    assert!(warm, "{}", talk.greeting);
}

#[test]
fn the_number_keys_choose_topics() {
    let mut app = talk_app();
    let folk = stand_in(&mut app, 0, smith(), None);
    talk_to(&mut app, folk);
    press(&mut app, KeyCode::Digit1);
    let (topic, said) = app.world().resource::<Talk>().answer.clone().expect("an answer");
    assert_eq!(topic, Topic::Who);
    assert!(said.contains(&app.world().get::<Denizen>(folk).unwrap().person.name), "{said}");
    // Barter and commission open their screens rather than answering.
    press(&mut app, KeyCode::Digit3);
    let sent: Vec<ServiceChosen> = app.world_mut().resource_mut::<Messages<ServiceChosen>>().drain().collect();
    assert_eq!(sent, vec![ServiceChosen { folk, service: Service::Barter }]);
    press(&mut app, KeyCode::Digit4);
    let sent: Vec<ServiceChosen> = app.world_mut().resource_mut::<Messages<ServiceChosen>>().drain().collect();
    assert_eq!(sent, vec![ServiceChosen { folk, service: Service::Commission }]);
    assert_eq!(app.world().resource::<Talk>().answer.as_ref().map(|a| a.0), Some(Topic::Who));
    // Past the last topic nothing happens.
    press(&mut app, KeyCode::Digit9);
    assert_eq!(app.world().resource::<Talk>().answer.as_ref().map(|a| a.0), Some(Topic::Who));
}

#[test]
fn a_guard_names_a_trader_of_its_faction() {
    let mut app = talk_app();
    let guard = stand_in(&mut app, 0, Some(Trade::guard()), None);
    let faction = faction_of(&app, guard);
    let trader = stand_in(&mut app, 1, Some(Trade { profession: Profession::Alchemist, tier: Tier::Novice, style: None }), Some(faction));
    let trader_name = app.world().get::<Denizen>(trader).unwrap().person.name.clone();
    talk_to(&mut app, guard);
    press(&mut app, KeyCode::Digit2);
    let (_, said) = app.world().resource::<Talk>().answer.clone().expect("an answer");
    assert!(said.contains(&trader_name) && said.contains("alchemist"), "{said}");
}

#[test]
fn the_same_folk_always_say_the_same_greeting() {
    let greet = || {
        let mut app = talk_app();
        let folk = stand_in(&mut app, 0, smith(), None);
        talk_to(&mut app, folk);
        app.world().resource::<Talk>().greeting.clone()
    };
    assert_eq!(greet(), greet());
}

#[test]
fn a_folk_who_falls_ends_the_talk() {
    let mut app = talk_app();
    let folk = stand_in(&mut app, 0, smith(), None);
    talk_to(&mut app, folk);
    app.world_mut().entity_mut(folk).insert(Corpse { since: 0.0, fell: Transform::IDENTITY });
    app.update();
    assert!(!app.world().resource::<ScreenOpen>().is(Screen::Talk));
}

#[test]
fn a_folk_who_turns_on_the_delver_mid_talk_ends_it_with_a_toast() {
    let mut app = talk_app();
    let folk = stand_in(&mut app, 0, smith(), None);
    talk_to(&mut app, folk);
    let name = app.world().get::<Denizen>(folk).unwrap().person.name.clone();
    app.world_mut().entity_mut(folk).insert(Hostility::Hostile);
    app.update();
    assert!(!app.world().resource::<ScreenOpen>().is(Screen::Talk));
    assert_eq!(toast(&app), format!("{name} turns on you."));
}

#[test]
fn the_rumour_is_resolved_once_per_talk() {
    let mut app = talk_app();
    app.insert_resource(crate::travel::Abyss::new(1207));
    let folk = stand_in(&mut app, 0, smith(), None);
    talk_to(&mut app, folk);
    let way_out = app.world().resource::<Talk>().topics.len();
    press(&mut app, DIGITS[way_out - 1]);
    let first = app.world().resource::<Talk>().answer.clone();
    press(&mut app, DIGITS[way_out - 1]);
    assert_eq!(app.world().resource::<Talk>().answer, first);
    assert!(first.is_some());
    assert_eq!(app.world().resource::<Talk>().resolved, 1);
}

#[test]
fn leaving_the_delve_drops_the_talk() {
    let mut app = talk_app();
    let folk = stand_in(&mut app, 0, smith(), None);
    talk_to(&mut app, folk);
    app.world_mut().resource_mut::<NextState<crate::launch::AppState>>().set(crate::launch::AppState::Title);
    app.update();
    app.update();
    assert_eq!(app.world().resource::<Talk>().folk, None);
}

#[test]
fn an_enchanters_enchant_and_scrap_topics_send_the_service_on() {
    let mut app = talk_app();
    let trade = Some(Trade { profession: Profession::Enchanter, tier: Tier::Expert, style: None });
    let folk = stand_in(&mut app, 0, trade, None);
    talk_to(&mut app, folk);
    // Who, Trade, Enchant, Scrap.
    for (digit, service) in [(KeyCode::Digit3, Service::Enchant), (KeyCode::Digit4, Service::Scrap)] {
        press(&mut app, digit);
        let sent: Vec<ServiceChosen> = app.world_mut().resource_mut::<Messages<ServiceChosen>>().drain().collect();
        assert_eq!(sent, vec![ServiceChosen { folk, service }]);
    }
    assert_eq!(app.world().resource::<Talk>().answer, None, "the bench answers, not the panel");
}
