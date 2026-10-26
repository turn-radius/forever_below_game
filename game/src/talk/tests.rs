use super::*;
use crate::folk_routine::{Phase, begin_routines};
use crate::inventory::Toast;
use fb_faction::{Tier, Trade};
use fb_level::FolkSpot;

/// An app with the talk, the routines' start and seed 1207's first level, no drawing.
fn talk_app() -> App {
    talk_app_on(fb_level::generate(1207, 1))
}

/// [`talk_app`] on `level`.
fn talk_app_on(level: fb_level::Level) -> App {
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
        .insert_resource(CurrentLevel(level));
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
    assert_eq!(labels, ["Who are you?", "Your trade", "Barter", "Commission", "This level", "Persuade", "The way out"]);
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
    assert_eq!(labels, ["Who are you?", "This level", "Persuade", "The way out"]);
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

// Denizens speak their tongues.

use fb_denizens::DenizenKind;
use fb_denizens::tongue::{self, Teaching};

/// A level at the denizens' depth with a minotaur in it, and the index of that spot.
fn minotaur_level() -> (fb_level::Level, usize) {
    (1..120)
        .find_map(|seed| {
            let level = fb_level::generate(seed, fb_faction::DENIZEN_DEPTH);
            let at = level.folk.iter().position(|f| f.denizen.is_some() && level.faction(f.faction).denizen == Some(DenizenKind::Minotaur))?;
            Some((level, at))
        })
        .expect("a minotaur level within 120 seeds")
}

fn minotaur_app() -> (App, Entity) {
    let (level, at) = minotaur_level();
    let mut app = talk_app_on(level);
    let folk = stand_in(&mut app, at, None, None);
    (app, folk)
}

fn record_of(app: &mut App) -> Mut<'_, WorldRecord> {
    app.world_mut().resource_mut::<WorldRecord>()
}

fn speaker_line(speaker: &Speaker, topic: Option<Topic>) -> String {
    match topic {
        None => lines::greeting(speaker),
        Some(t) => lines::answer(speaker, t, None),
    }
}

/// What the panel shows for the greeting and for each topic, as (topic, text, readable).
fn panel_lines(app: &App) -> Vec<(Option<Topic>, String, bool)> {
    let talk = app.world().resource::<Talk>();
    let speaker = talk.speaker.as_ref().expect("a speaker");
    let lexicon = app.world().resource::<WorldRecord>().lexicon();
    std::iter::once(None)
        .chain(talk.topics.iter().map(|t| Some(*t)))
        .map(|topic| {
            let (text, readable) = shown(speaker, topic, &speaker_line(speaker, topic), lexicon);
            (topic, text, readable)
        })
        .collect()
}

#[test]
fn a_neutral_minotaur_opens_the_panel_and_every_line_is_glyphs_until_words_are_learned() {
    let (mut app, folk) = minotaur_app();
    talk_to(&mut app, folk);
    assert!(app.world().resource::<ScreenOpen>().is(Screen::Talk), "denizens open the panel like folk");
    assert_eq!(app.world().resource::<Talk>().tongue, Some(DenizenKind::Minotaur));
    let labels: Vec<_> = app.world().resource::<Talk>().topics.iter().map(|t| t.label()).collect();
    assert_eq!(labels, ["Who are you?", "This level", "Dangers", "Your trade", "Barter", "Work"]);
    for (topic, text, readable) in panel_lines(&app) {
        if topic != Some(Topic::Service(Service::Barter)) {
            assert!(!readable, "{topic:?}: {text}");
            assert!(text.chars().any(|c| tongue::script(DenizenKind::Minotaur).contains(c)), "{text}");
        }
    }
    // The same glyphs every time the panel is drawn.
    assert_eq!(panel_lines(&app), panel_lines(&app));
}

#[test]
fn learning_greetings_and_trade_makes_those_lines_readable_and_the_rest_not() {
    let (mut app, folk) = minotaur_app();
    record_of(&mut app).lexicon.learn_teaching(&Teaching::of_topics(DenizenKind::Minotaur, &[tongue::Topic::Greeting, tongue::Topic::Trade]));
    talk_to(&mut app, folk);
    let lines = panel_lines(&app);
    for (topic, text, readable) in &lines {
        match topic {
            None | Some(Topic::Trade) => assert!(readable, "{topic:?}: {text}"),
            Some(Topic::Service(Service::Barter)) => {}
            Some(other) => assert!(!readable, "{other:?}: {text}"),
        }
    }
}

#[test]
fn a_line_needing_a_word_beyond_its_topics_core_stays_glyphs_until_that_word_is_learned() {
    let (mut app, folk) = minotaur_app();
    // The Kin line also needs name and banner, which are not among a topic's first three words.
    let kin = Teaching::of_topics(DenizenKind::Minotaur, &[tongue::Topic::Kin]);
    let core: Vec<_> = kin.words.iter().copied().take(tongue::CORE_WORDS).collect();
    record_of(&mut app).lexicon.learn(DenizenKind::Minotaur, &core);
    talk_to(&mut app, folk);
    let who = |app: &App| panel_lines(app).into_iter().find(|l| l.0 == Some(Topic::Who)).unwrap();
    assert!(!who(&app).2, "core words alone do not read the line that names the banner");
    record_of(&mut app).lexicon.learn_teaching(&kin);
    assert!(who(&app).2);
}

#[test]
fn barter_is_refused_without_the_trade_words_and_allowed_with_them() {
    let (mut app, folk) = minotaur_app();
    talk_to(&mut app, folk);
    let barter = app.world().resource::<Talk>().topics.iter().position(|t| *t == Topic::Service(Service::Barter)).unwrap();
    press(&mut app, DIGITS[barter]);
    let sent: Vec<ServiceChosen> = app.world_mut().resource_mut::<Messages<ServiceChosen>>().drain().collect();
    assert!(sent.is_empty(), "no barter screen without the words");
    let (topic, said) = app.world().resource::<Talk>().answer.clone().expect("a gesture line");
    assert_eq!(topic, Topic::Service(Service::Barter));
    assert!(said.contains("minotaur") && said.contains("trade"), "{said}");
    // The refusal reads as a gesture, not as speech in glyphs.
    let talk = app.world().resource::<Talk>();
    let lexicon = app.world().resource::<WorldRecord>().lexicon();
    assert_eq!(shown(talk.speaker.as_ref().unwrap(), Some(topic), &said, lexicon), (said.clone(), true));
    // Greeting words alone are not enough; the trade words too are.
    record_of(&mut app).lexicon.learn_teaching(&Teaching::of_topics(DenizenKind::Minotaur, &[tongue::Topic::Greeting]));
    press(&mut app, DIGITS[barter]);
    assert!(app.world_mut().resource_mut::<Messages<ServiceChosen>>().drain().next().is_none());
    record_of(&mut app).lexicon.learn_teaching(&Teaching::of_topics(DenizenKind::Minotaur, &[tongue::Topic::Trade]));
    press(&mut app, DIGITS[barter]);
    let sent: Vec<ServiceChosen> = app.world_mut().resource_mut::<Messages<ServiceChosen>>().drain().collect();
    assert_eq!(sent, vec![ServiceChosen { folk, service: Service::Barter }]);
}

#[test]
fn words_learned_persist_across_a_record_save_and_load() {
    let (mut app, folk) = minotaur_app();
    record_of(&mut app).lexicon.learn_teaching(&Teaching::of_topics(DenizenKind::Minotaur, &[tongue::Topic::Greeting]));
    let saved = serde_json::to_string(app.world().resource::<WorldRecord>()).expect("saves");
    let loaded: WorldRecord = serde_json::from_str(&saved).expect("loads");
    assert_eq!(loaded.lexicon(), app.world().resource::<WorldRecord>().lexicon());
    *app.world_mut().resource_mut::<WorldRecord>() = loaded;
    talk_to(&mut app, folk);
    assert!(panel_lines(&app)[0].2, "the greeting still reads after the reload");
}

#[test]
fn a_hostile_or_wary_denizen_snorts_or_turns_away_without_a_panel() {
    let (mut app, folk) = minotaur_app();
    standing(&mut app, folk, -30);
    talk_to(&mut app, folk);
    assert!(toast(&app).contains("The minotaur"), "{}", toast(&app));
    assert_eq!(app.world().resource::<ScreenOpen>().0, None);
    standing(&mut app, folk, -100);
    talk_to(&mut app, folk);
    assert!(toast(&app).contains("spits at your feet") && toast(&app).starts_with("The minotaur"), "{}", toast(&app));
    assert_eq!(app.world().resource::<ScreenOpen>().0, None);
}

// Folk speak of the denizens.

/// The folk at `index` of `level`, spoken to at `band`.
fn folk_speaker(level: &fb_level::Level, index: usize, band: Band) -> Speaker {
    let spot = level.folk[index];
    let person = crate::folk::Person::of(&spot, level.faction(spot.faction), level.depth);
    let denizen = Denizen { person, radius: 3.0, min: Vec3::ZERO, max: Vec3::ONE };
    speaker_of(&denizen, band, level, std::iter::empty(), |_| band)
}

#[test]
fn folk_speak_of_the_denizens_more_the_deeper_they_stand() {
    let level = fb_level::generate(1207, 1);
    let share = |depth: u32| {
        let with = (0..200u32).filter(|&seed| lines::topics(&Speaker { seed, depth, ..folk_speaker(&level, 0, Band::Neutral) }).contains(&Topic::Strange)).count();
        with as f32 / 200.0
    };
    let (top, mid, deep) = (share(1), share(5), share(fb_faction::DENIZEN_DEPTH - 1));
    assert!(top < mid && mid < deep, "{top} {mid} {deep}");
    assert!(top <= 0.15 && deep >= 0.45, "{top} {deep}");
    // The same folk always have the same rumour.
    let speaker = (0..400).map(|seed| Speaker { seed, depth: 9, ..folk_speaker(&level, 0, Band::Neutral) }).find(|s| lines::strange_tale(s).is_some()).expect("one with a tale");
    assert_eq!(lines::strange_tale(&speaker), lines::strange_tale(&speaker.clone()));
}

#[test]
fn deep_folk_name_the_denizen_factions_on_their_level_and_how_they_stand() {
    // A level with a minotaur faction and some folk beside it.
    let level = (1..400)
        .map(|seed| fb_level::generate(seed, fb_faction::DENIZEN_DEPTH))
        .find(|level| level.folk.iter().any(|f| f.denizen.is_none()) && level.factions.iter().any(|f| f.denizen == Some(DenizenKind::Minotaur)))
        .expect("a level with both within 400 seeds");
    let spot = level.folk.iter().position(|f| f.denizen.is_none()).expect("a folk");
    let factions: Vec<String> = level.factions.iter().filter(|f| f.denizen == Some(DenizenKind::Minotaur)).map(|f| f.name.clone()).collect();
    let hostile = folk_speaker(&level, spot, Band::Hostile);
    let topics = lines::topics(&hostile);
    let at = topics.iter().position(|t| matches!(t, Topic::Faction(_))).expect("a topic for the denizen faction");
    assert!(!topics.contains(&Topic::Strange));
    let said = lines::answer(&hostile, topics[at], None);
    assert!(factions.iter().any(|f| said.to_lowercase().contains(&f.to_lowercase())) && said.to_lowercase().contains("minotaur"), "{said} {factions:?}");
    assert!(said.contains("kill you on sight"), "a hostile faction is dangerous: {said}");
    assert!(topics[at].label_for(&hostile).starts_with("The "));
    let allied = folk_speaker(&level, spot, Band::Allied);
    assert!(lines::answer(&allied, topics[at], None).contains("one of their own"));
    assert_eq!(lines::answer(&hostile, topics[at], None), lines::answer(&hostile, topics[at], None));
}

#[test]
fn shallow_folk_have_no_denizen_faction_topics() {
    let level = fb_level::generate(1207, 1);
    let speaker = folk_speaker(&level, 0, Band::Neutral);
    assert!(!lines::topics(&speaker).iter().any(|t| matches!(t, Topic::Faction(_))));
}

/// Opens the talk with a smith and chooses Persuade.
fn persuade_app() -> (App, Entity) {
    let mut app = talk_app();
    app.init_resource::<crate::inventory::Inventory>();
    let folk = stand_in(&mut app, 0, smith(), None);
    talk_to(&mut app, folk);
    let at = app.world().resource::<Talk>().topics.iter().position(|t| *t == Topic::Persuade).expect("a Persuade topic");
    press(&mut app, DIGITS[at]);
    (app, folk)
}

fn dice_close(app: &mut App, folk: Entity, face: i32) {
    let roll = fb_character::check::Check::d20("Persuade", 10).with_natural(face);
    app.world_mut().write_message(crate::dice::RollDone { roll, purpose: crate::dice::RollPurpose::Persuade(folk) });
    app.update();
}

#[test]
fn choosing_persuade_rolls_the_dice_and_holds_the_other_topics_until_they_close() {
    let (mut app, folk) = persuade_app();
    assert!(app.world().resource::<Talk>().waiting);
    assert!(app.world().resource::<Persuading>().0.is_some());
    // The number of the first topic does nothing while the dice show.
    press(&mut app, DIGITS[0]);
    assert!(app.world().resource::<Talk>().answer.is_none());
    dice_close(&mut app, folk, 20);
    assert!(!app.world().resource::<Talk>().waiting);
}

#[test]
fn a_persuaded_folk_gives_coin_once() {
    let (mut app, folk) = persuade_app();
    let before = app.world().resource::<crate::inventory::Inventory>().coins();
    dice_close(&mut app, folk, 20);
    let inventory = app.world().resource::<crate::inventory::Inventory>();
    assert!(inventory.coins().total_copper() > before.total_copper());
    assert_eq!(app.world().resource::<WorldRecord>().persuaded.len(), 1);
    // Asked again, they say they have done what they will, and no dice roll.
    let at = app.world().resource::<Talk>().topics.iter().position(|t| *t == Topic::Persuade).unwrap();
    press(&mut app, DIGITS[at]);
    assert!(!app.world().resource::<Talk>().waiting);
    assert!(app.world().resource::<Talk>().answer.is_some());
}

#[test]
fn a_failed_persuasion_costs_a_little_standing() {
    let (mut app, folk) = persuade_app();
    let faction = faction_of(&app, folk);
    let before = app.world().resource::<WorldRecord>().standings.get(faction).value();
    dice_close(&mut app, folk, 1);
    let after = app.world().resource::<WorldRecord>().standings.get(faction).value();
    assert_eq!(before - after, persuade::FAIL_COST);
    assert!(app.world().resource::<WorldRecord>().persuaded.is_empty());
}

#[test]
fn a_folk_whose_faction_is_hostile_cannot_be_persuaded() {
    let mut app = talk_app();
    app.init_resource::<crate::inventory::Inventory>();
    let folk = stand_in(&mut app, 0, smith(), None);
    talk_to(&mut app, folk);
    let faction = faction_of(&app, folk);
    app.world_mut().resource_mut::<WorldRecord>().standings.turn_hostile(faction);
    let at = app.world().resource::<Talk>().topics.iter().position(|t| *t == Topic::Persuade).unwrap();
    press(&mut app, DIGITS[at]);
    assert!(!app.world().resource::<Talk>().waiting);
    assert!(app.world().resource::<Persuading>().0.is_none());
    assert!(app.world().resource::<Talk>().answer.is_some());
}

#[test]
fn the_try_is_on_the_record_as_the_roll_starts() {
    let (app, folk) = persuade_app();
    let speaker = app.world().resource::<Talk>().speaker.clone().unwrap();
    let who = (speaker.depth, speaker.seed);
    assert_eq!(app.world().resource::<WorldRecord>().persuasion_tries(who), 1, "noted before the dice close");
    let _ = folk;
}

#[test]
fn a_folk_who_was_failed_refuses_until_the_delver_rests() {
    let (mut app, folk) = persuade_app();
    dice_close(&mut app, folk, 1);
    let at = app.world().resource::<Talk>().topics.iter().position(|t| *t == Topic::Persuade).unwrap();
    // Asked again at once: a refusal line, no roll, no further cost.
    let faction = faction_of(&app, folk);
    let standing = app.world().resource::<WorldRecord>().standings.get(faction).value();
    press(&mut app, DIGITS[at]);
    assert!(!app.world().resource::<Talk>().waiting && app.world().resource::<Persuading>().0.is_none());
    assert_eq!(app.world().resource::<WorldRecord>().standings.get(faction).value(), standing);
    let said = app.world().resource::<Talk>().answer.clone().unwrap().1;
    assert!(said.to_lowercase().contains("sleep") || said.to_lowercase().contains("rest"), "{said}");
    // After a rest they hear the delver out again.
    app.world_mut().resource_mut::<WorldRecord>().rests += 1;
    press(&mut app, DIGITS[at]);
    assert!(app.world().resource::<Talk>().waiting);
}

#[test]
fn the_talk_cannot_be_left_while_the_dice_show() {
    let (mut app, folk) = persuade_app();
    press(&mut app, KeyCode::KeyF);
    assert!(app.world().resource::<ScreenOpen>().is(Screen::Talk));
    assert!(app.world().resource::<Talk>().waiting);
    dice_close(&mut app, folk, 20);
    press(&mut app, KeyCode::KeyF);
    assert!(!app.world().resource::<ScreenOpen>().is(Screen::Talk));
}

#[test]
fn denizens_are_not_offered_persuasion_and_the_state_resets_with_the_delve() {
    let (mut app, folk) = minotaur_app();
    talk_to(&mut app, folk);
    assert!(!app.world().resource::<Talk>().topics.contains(&Topic::Persuade));
    let (mut app, _) = persuade_app();
    assert!(app.world().resource::<Persuading>().0.is_some());
    app.world_mut().resource_mut::<NextState<crate::launch::AppState>>().set(crate::launch::AppState::Title);
    app.update();
    app.update();
    assert!(app.world().resource::<Persuading>().0.is_none());
}
