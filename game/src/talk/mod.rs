//! Talking to folk: the Talk key (Primary, F by default) on a folk in reach.
//!
//! - Neutral and better folk open the dialogue panel ([`Screen::Talk`]) and hold the folk's routine,
//!   so they stop and face the delver ([`crate::folk_routine::Routine::hold`]).
//! - A folk who is Hostile or Wary to the delver, or who has turned on them, opens a parley
//!   instead ([`Reception::Parley`]): the same panel with a curt line ("Not a step closer.
//!   What do you want?") and Persuade as its one topic ([`lines::parley_topics`]). They hold
//!   their blows while it lasts ([`crate::hostility::Parleying`]), and being hostile does not
//!   end it. A failed roll against a Hostile folk, or one who has turned, closes the panel and
//!   they fight on.
//! - The panel is on the right of the view, so the folk stays in sight to its left. It shows
//!   their name, folk, trade and tier, faction and the band the faction stands in, a greeting
//!   (warmer for Friendly and Allied, curt in a parley), and the topics ([`lines::topics`]). A topic is chosen
//!   with its number key or a click, and its answer shows under the greeting. Everything is
//!   laid out for the 1920 by 1080 reference and scaled with the window ([`crate::ui`]), and the
//!   mouse is read in the interface's own pixels ([`ui_cursor`]).
//! - The Talk key again or Esc (which closes any screen, [`crate::play::switch_screens`]) leaves,
//!   and the folk's routine is released however the screen closes. A folk who falls or turns on
//!   the delver while the panel is open ends the talk (not a parley, which is with such a folk).
//! - What folk say is written in [`lines`] from the grammars in `data/talk.json`, drawn from the
//!   folk's own seed, so the same folk always say the same thing.
//! - Commission, Enchant and Scrap list for the trades that offer them and answer "Not yet."
//!   until their screens exist. When one does, [`service_ready`] says so for it and choosing the
//!   topic sends a [`ServiceChosen`] for that screen to open on.
//!
//! - A denizen (minotaur, faun, ...) answers in its own tongue ([`fb_denizens::tongue`]). Standing
//!   works as for folk (Hostile and Wary ones snort or turn away and open a parley, the rest open the panel), but
//!   each line names the words it needs ([`lines::line_needs`]) and shows as the tongue's glyphs
//!   until the delver knows them all ([`WorldRecord::lexicon`]), then in plain words. The panel
//!   says how much of the tongue is known. The Trade topic and Barter need the trade words
//!   ([`Lexicon::can_barter`]); without them the denizen shakes its head
//!   ([`lines::barter_refusal`]). There is no quest system yet: the Work topic is spoken like the
//!   rest, and [`Lexicon::can_take_quests`] gates taking one at the hook in [`choose`].
//! - Folk deep enough to know the denizens ([`fb_denizens::DENIZEN_DEPTH`]) have a topic for
//!   each denizen faction on their level; shallower, a folk sometimes has a rumour of them,
//!   more often the deeper they stand ([`lines::strange_tale`]).
//!
//! - Persuade is a d20 check, shown as dice ([`crate::dice`]; the rules are in [`persuade`]), to
//!   raise the folk's faction's standing with the delver. A success raises it by an amount scaled
//!   by the roll, a failure costs a little, and a folk gets one try per rest. Any folk can be
//!   persuaded, whatever their faction's band: the worse it stands, the harder the roll, through
//!   the standing modifier.
//!
//! `FB_LEARN=<kind>:<topic>[+<topic>...]` (`minotaur:greeting+trade`) teaches those words at
//! once, for captures. `FB_TALK=1` opens the panel on the folk nearest the delver a few dozen frames in, and
//! `FB_TALK_TOPIC=<n>` then chooses the nth topic (counting from 1; `strange` or `denizens` choose a
//! folk's topic of the denizens), for captures; `FB_TALK=folk` skips the denizens.

pub mod lines;
pub mod persuade;

#[cfg(test)]
mod tests;

use bevy::{
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use fb_faction::{Band, Profession, TradeRules, World};
use fb_level::Level;
use procgen_rng::Mulberry32;

use crate::carry::ViewModelCamera;
use crate::combat::feedback::Corpse;
use crate::character::Character;
use crate::folk::{AimedFolk, Denizen};
use crate::inventory::Inventory;
use crate::folk_routine::Routine;
use crate::hostility::{Hostility, Parleying};
use crate::inventory::Toasts;
use crate::keymap::{Action, ActionInput, Input, Keymap};
use crate::play::{CurrentLevel, DelveSystems, Player, Screen, ScreenOpen, show_screen};
use crate::record::WorldRecord;
use crate::ui::{ACCENT, BAD, BG, BRIGHT, CELL, FAINT, INK, LINE, MUTED, OK, UiFonts, node_rect, ui_cursor};
pub use lines::{KnownDenizens, MasterRumour, Reception, Service, Speaker, Topic};
use fb_denizens::DenizenKind;
use fb_denizens::tongue::Lexicon;

/// The width of the panel at the reference size, logical pixels.
pub const PANEL_WIDTH: f32 = 600.0;
/// How far the panel stands from the right edge, logical pixels.
pub const PANEL_MARGIN: f32 = 64.0;

pub struct TalkPlugin;

impl Plugin for TalkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Talk>().init_resource::<TalkRequest>().init_resource::<Persuading>().add_message::<ServiceChosen>().add_message::<crate::dice::RollRequest>().add_message::<crate::dice::RollDone>();
        if std::env::var_os("FB_TALK").is_some() {
            app.add_systems(Update, talk_for_capture.in_set(DelveSystems));
        }
        if std::env::var_os("FB_LEARN").is_some() {
            app.add_systems(Update, learn_for_capture.in_set(DelveSystems));
        }
        app.add_systems(OnExit(crate::launch::AppState::Delving), reset_talk);
        app.add_systems(Update, (click_topics, talk, start_persuasion, finish_persuasion, end_talk, build_talk_screen, hover_topics).chain().in_set(DelveSystems));
    }
}

/// Drops the talk when the delve ends, so no folk outlives it.
fn reset_talk(mut talk: ResMut<Talk>, mut persuading: ResMut<Persuading>) {
    *talk = Talk::default();
    persuading.0 = None;
}

/// Whether a service's screen exists to open: the barter screen ([`crate::barter`]), the
/// commission screen ([`crate::commission`]) and the enchanter's two ([`crate::enchanter`]).
/// Whatever builds another flips its arm.
pub fn service_ready(service: Service) -> bool {
    matches!(service, Service::Barter | Service::Commission | Service::Order | Service::Enchant | Service::Scrap)
}

/// The delver chose a service of the folk they talk to, and its screen should open. Sent only for
/// a service that is [`service_ready`].
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceChosen {
    pub folk: Entity,
    pub service: Service,
}

/// The talk in progress, if any.
#[derive(Resource, Default, Debug)]
pub struct Talk {
    /// Who the delver speaks with.
    pub folk: Option<Entity>,
    pub speaker: Option<Speaker>,
    pub greeting: String,
    pub topics: Vec<Topic>,
    /// The tongue the speaker's lines are in, if they are a denizen.
    pub tongue: Option<DenizenKind>,
    /// The topic last chosen and what was said.
    pub answer: Option<(Topic, String)>,
    /// Counts changes, so the panel knows when to draw again.
    /// The master the speaker has heard of, resolved once per talk on the first ask (the outer
    /// `None` is not yet asked), since finding the master's faction generates a level.
    rumour: Option<Option<MasterRumour>>,
    /// How many times a rumour was resolved this talk.
    pub resolved: u32,
    /// The Persuade topic was chosen and its check is not yet started ([`start_persuasion`]).
    persuade_asked: bool,
    /// A persuasion's dice are showing: no other topic is chosen until they close.
    waiting: bool,
    /// A parley with a wary or hostile folk: Persuade is the one topic, and a folk who has turned
    /// on the delver does not end it.
    pub parley: bool,
    /// The folk is Hostile, or has turned on the delver: a failed persuasion ends the parley and
    /// they attack.
    hostile: bool,
    revision: u32,
}

impl Talk {
    /// Has the speaker answer `topic` with `text`, the panel drawing it afresh.
    pub fn say(&mut self, topic: Topic, text: String) {
        self.answer = Some((topic, text));
        self.revision += 1;
    }
}

/// For captures: talk to this folk next frame as though the Talk key were pressed on them, and
/// choose this topic (counting from 1) once the panel is open.
#[derive(Resource, Default, Debug)]
pub struct TalkRequest {
    pub folk: Option<Entity>,
    pub topic: Option<usize>,
}

/// The panel's root.
#[derive(Component, Debug)]
pub struct TalkScreen;

/// A topic's row, with the number key that chooses it.
#[derive(Component, Debug)]
struct TopicButton(usize);

/// For captures (`FB_LEARN=minotaur:greeting+trade`): teaches those topics' words at once.
fn learn_for_capture(record: Option<ResMut<WorldRecord>>, mut done: Local<bool>) {
    let (Some(mut record), false) = (record, *done) else { return };
    *done = true;
    let Ok(spec) = std::env::var("FB_LEARN") else { return };
    for entry in spec.split(',') {
        let Some((kind, topics)) = entry.split_once(':') else { continue };
        let Some(kind) = DenizenKind::ALL.into_iter().find(|k| k.key() == kind.trim()) else { continue };
        let topics: Vec<_> = topics.split('+').filter_map(|t| fb_denizens::tongue::Topic::ALL.into_iter().find(|x| x.key() == t.trim())).collect();
        record.lexicon.learn_teaching(&fb_denizens::tongue::Teaching::of_topics(kind, &topics));
    }
}

/// Opens `FB_TALK` talks on the folk nearest the delver, and `FB_TALK_TOPIC` chooses a topic.
fn talk_for_capture(
    player: Query<&Transform, With<Player>>,
    folk: Query<(Entity, &Denizen, &Transform), Without<Corpse>>,
    mut request: ResMut<TalkRequest>,
    talk: Res<Talk>,
    record: Option<Res<WorldRecord>>,
    mut frames: Local<u32>,
    mut chosen: Local<bool>,
) {
    *frames += 1;
    const OPEN_AT: u32 = 40;
    if *frames == OPEN_AT {
        let Some(at) = player.iter().next().map(|t| t.translation) else { return };
        // `FB_TALK=folk` skips the denizens, for the folk who speak of them.
        let only_folk = std::env::var("FB_TALK").is_ok_and(|v| v == "folk");
        request.folk = folk.iter().filter(|(_, d, _)| !only_folk || (d.person.faction.denizen.is_none() && record.as_ref().is_none_or(|r| r.standings.band(d.person.faction.id) > Band::Wary))).min_by(|a, b| a.2.translation.distance(at).total_cmp(&b.2.translation.distance(at))).map(|(e, ..)| e);
    }
    if *frames == OPEN_AT + 8 && !*chosen && talk.folk.is_some() {
        *chosen = true;
        // A number, or `strange` or `denizens` for a folk's topic of the denizens.
        let by_name = |v: &str| match v {
            "strange" => talk.topics.iter().position(|t| *t == Topic::Strange),
            "denizens" => talk.topics.iter().position(|t| matches!(t, Topic::Faction(_))),
            "persuade" => talk.topics.iter().position(|t| *t == Topic::Persuade),
            _ => None,
        };
        if let Some(n) = std::env::var("FB_TALK_TOPIC").ok().and_then(|v| v.trim().parse::<usize>().ok().or_else(|| by_name(v.trim()).map(|i| i + 1))) {
            request.topic = Some(n);
        }
    }
}


/// Everything one folk's talk needs of the level: who the speaker is, what the level holds.
fn speaker_of<'a>(denizen: &Denizen, band: Band, level: &Level, others: impl Iterator<Item = &'a Denizen>, band_of: impl Fn(fb_faction::Faction) -> Band) -> Speaker {
    let person = &denizen.person;
    let kind = person.faction.denizen;
    let mut traders: Vec<(u32, String, Profession, String)> = others
        .filter(|d| d.person.spot.faction == person.spot.faction && d.person.spot.seed != person.spot.seed)
        .filter_map(|d| d.person.spot.trade.filter(|t| t.profession != Profession::Guard).map(|t| (d.person.spot.seed, d.person.name.clone(), t.profession, lines::place_of(level, &person.spot, &d.person.spot))))
        .collect();
    traders.sort_by_key(|(seed, ..)| *seed);
    Speaker {
        name: person.name.clone(),
        folk: kind.map_or_else(|| person.spec.folk.race().label.clone(), |k| k.label().to_string()),
        faction: person.faction.name.clone(),
        intention: person.faction.intention,
        band,
        trade: person.spot.trade,
        seed: person.spot.seed,
        depth: level.depth,
        level_factions: level.factions.iter().map(|f| f.name.clone()).collect(),
        traders: traders.into_iter().map(|(_, name, profession, place)| (name, profession, place)).collect(),
        denizen: kind,
        denizen_factions: level.factions.iter().filter_map(|f| f.denizen.map(|kind| KnownDenizens { name: f.name.clone(), kind, band: band_of(f.id) })).collect(),
    }
}

/// A master deeper in the abyss that `speaker` has heard of, from the world of `run_seed`: their
/// own trade's master if they know of one, else the nearest. The faction is the one the world
/// places them with, so the rumour is true.
fn master_rumour(speaker: &Speaker, run_seed: u32) -> Option<MasterRumour> {
    let world = World::new(run_seed);
    let masters = world.masters(&TradeRules::DEFAULT);
    let deeper: Vec<_> = masters.all().iter().filter(|m| m.depth > speaker.depth).take(3).collect();
    let mut rng = Mulberry32::new(speaker.seed ^ 0x4D52);
    let own = speaker.trade.and_then(|t| masters.deeper(t.profession, speaker.depth)).filter(|_| rng.chance(0.5));
    let master = own.or_else(|| if deeper.is_empty() { None } else { Some(*rng.pick(&deeper)) })?;
    let faction = fb_level::trades::master_faction(run_seed, master.depth, master.profession)?;
    Some(MasterRumour { calling: master.trade().label().to_lowercase(), faction: world.faction(faction).name.clone(), levels: master.depth - speaker.depth })
}

/// The answer to `topic` for the open talk, saved on [`Talk`]. A service whose screen exists is
/// sent on instead ([`ServiceChosen`]) and says nothing here.
fn choose(talk: &mut Talk, topic: Topic, run_seed: Option<u32>, lexicon: &Lexicon, services: &mut MessageWriter<ServiceChosen>) {
    let (Some(folk), Some(speaker)) = (talk.folk, talk.speaker.as_ref()) else { return };
    if talk.waiting {
        return;
    }
    if topic == Topic::Persuade && speaker.denizen.is_none() {
        talk.persuade_asked = true;
        return;
    }
    // A denizen trades only with one who has their trade words; else it shakes its head.
    if let (Some(kind), Topic::Service(Service::Barter)) = (speaker.denizen, topic)
        && !lexicon.can_barter(kind)
    {
        let said = lines::barter_refusal(speaker);
        talk.answer = Some((topic, said));
        talk.revision += 1;
        return;
    }
    // HOOK: no quest system yet. When one exists, taking a denizen's quest from the Work topic
    // is allowed only if `lexicon.can_take_quests(kind)`; until then the topic is just spoken.
    if let Topic::Service(service) = topic
        && service_ready(service)
    {
        services.write(ServiceChosen { folk, service });
        return;
    }
    let rumour = if topic == Topic::WayOut {
        if talk.rumour.is_none() {
            talk.resolved += 1;
            talk.rumour = Some(lines::knows_a_master(speaker).then(|| run_seed.and_then(|seed| master_rumour(speaker, seed))).flatten());
        }
        talk.rumour.clone().flatten()
    } else {
        None
    };
    let speaker = talk.speaker.as_ref().expect("checked above");
    let said = lines::answer(speaker, topic, rumour.as_ref());
    talk.answer = Some((topic, said));
    talk.revision += 1;
}

/// The digit keys, in the order of the topics they choose.
const DIGITS: [KeyCode; 9] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9];
const PAD_DIGITS: [KeyCode; 9] = [KeyCode::Numpad1, KeyCode::Numpad2, KeyCode::Numpad3, KeyCode::Numpad4, KeyCode::Numpad5, KeyCode::Numpad6, KeyCode::Numpad7, KeyCode::Numpad8, KeyCode::Numpad9];

/// A click on a topic's row asks for that topic (counting from 1), as its number key does.
fn click_topics(
    input: ActionInput,
    open: Res<ScreenOpen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    buttons: Query<(&TopicButton, &ComputedNode, &UiGlobalTransform)>,
    mut request: ResMut<TalkRequest>,
) {
    if !open.is(Screen::Talk) || !input.just_pressed(Action::Choose) {
        return;
    }
    let Some(point) = windows.iter().next().and_then(|w| ui_cursor(w, ui_scale.as_deref())) else { return };
    if let Some((button, ..)) = buttons.iter().find(|(_, node, at)| node_rect(node, at).contains(point)) {
        request.topic = Some(button.0 + 1);
    }
}

/// The Talk key on a folk in reach opens the talk (or turns them away); with the panel open, the
/// Talk key again leaves, and the number keys and the mouse choose topics.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn talk(
    input: ActionInput,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    keymap: Res<Keymap>,
    aimed: Res<AimedFolk>,
    mut request: ResMut<TalkRequest>,
    level: Res<CurrentLevel>,
    record: Option<Res<WorldRecord>>,
    abyss: Option<Res<crate::travel::Abyss>>,
    mut open: ResMut<ScreenOpen>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
    mut talk: ResMut<Talk>,
    mut folk: Query<(Entity, &Denizen, Option<&Hostility>, Option<&mut Routine>), Without<Corpse>>,
    mut services: MessageWriter<ServiceChosen>,
    mut commands: Commands,
) {
    let run_seed = abyss.map(|a| a.run_seed);
    if open.is(Screen::Talk) {
        let pressed = |code: &KeyCode| keys.as_ref().is_some_and(|k| k.just_pressed(*code));
        // A Primary bound to a mouse button leaves too, unless the click chose a topic.
        let leave = keymap.inputs(Action::Primary).iter().any(|i| match i {
            Input::Key(code) => pressed(code),
            Input::Mouse(button) => request.topic.is_none() && mouse.as_ref().is_some_and(|m| m.just_pressed(*button)),
            _ => false,
        });
        if leave && !talk.waiting {
            show_screen(&mut open, None, cursor.as_deref_mut().map(|c| &mut **c));
            return;
        }
        let mut pick = (0..talk.topics.len().min(9)).find(|&i| pressed(&DIGITS[i]) || pressed(&PAD_DIGITS[i])).map(|i| talk.topics[i]);
        if pick.is_none()
            && let Some(n) = request.topic.take()
        {
            pick = n.checked_sub(1).and_then(|i| talk.topics.get(i)).copied();
        }
        if let Some(topic) = pick {
            let lexicon = record.as_ref().map_or_else(Lexicon::new, |r| r.lexicon().clone());
            choose(&mut talk, topic, run_seed, &lexicon, &mut services);
        }
        return;
    }
    if open.0.is_some() {
        return;
    }
    let target = request.folk.take().or_else(|| input.just_pressed(Action::Primary).then_some(aimed.0).flatten());
    let Some(entity) = target else { return };
    let Ok((_, denizen, hostility, _)) = folk.get(entity) else { return };
    let band = record.as_ref().map_or(Band::Neutral, |r| r.standings.band(denizen.person.faction.id));
    let speaker = speaker_of(denizen, band, &level.0, folk.iter().map(|(_, d, ..)| d), |f| record.as_ref().map_or(Band::Neutral, |r| r.standings.band(f)));
    let turned = hostility.is_some_and(|h| h.is_hostile());
    let reception = lines::reception(band, turned);
    if let Ok((_, _, _, Some(mut routine))) = folk.get_mut(entity) {
        routine.hold();
    }
    talk.folk = Some(entity);
    talk.parley = reception == Reception::Parley;
    talk.hostile = talk.parley && (turned || band == Band::Hostile);
    if talk.parley {
        // Hostile or not, they hold their blows while the delver parleys.
        commands.entity(entity).insert(Parleying);
        talk.greeting = lines::parley_greeting(&speaker, turned);
        talk.topics = lines::parley_topics();
    } else {
        talk.greeting = lines::greeting(&speaker);
        talk.topics = lines::topics(&speaker);
    }
    talk.tongue = speaker.denizen;
    // A maker holding an order of the delver's offers to speak of it, after Commission.
    if record.as_ref().is_some_and(|r| r.orders.iter().any(|o| o.maker_seed == speaker.seed && o.depth == speaker.depth))
        && let Some(at) = talk.topics.iter().position(|t| *t == Topic::Service(Service::Commission))
    {
        talk.topics.insert(at + 1, Topic::Service(Service::Order));
    }
    talk.speaker = Some(speaker);
    talk.answer = None;
    talk.revision += 1;
    show_screen(&mut open, Some(Screen::Talk), cursor.as_deref_mut().map(|c| &mut **c));
}

/// The persuasion whose dice are showing.
#[derive(Resource, Default, Debug)]
struct Persuading(Option<Pending>);

#[derive(Debug, Clone, Copy)]
struct Pending {
    folk: Entity,
    faction: fb_faction::Faction,
    /// The folk, by depth and seed, for the record of who has been tried.
    who: (u32, u32),
}

/// Rolls to win the folk's faction over: a d20 check ([`persuade::check`]) to raise its standing
/// with the delver, whatever the faction's band: the worse it stands, the harder the roll. A folk
/// already tried since the delver last rested says so and no dice roll.
#[allow(clippy::too_many_arguments)]
fn start_persuasion(
    mut talk: ResMut<Talk>,
    mut pending: ResMut<Persuading>,
    character: Option<Res<Character>>,
    inventory: Option<Res<Inventory>>,
    mut record: Option<ResMut<WorldRecord>>,
    folk: Query<&Denizen>,
    mut rolls: MessageWriter<crate::dice::RollRequest>,
) {
    if !std::mem::take(&mut talk.persuade_asked) {
        return;
    }
    let (Some(entity), Some(speaker)) = (talk.folk, talk.speaker.clone()) else { return };
    let Ok(denizen) = folk.get(entity) else { return };
    let faction = denizen.person.faction.id;
    let who = (speaker.depth, speaker.seed);
    let said = |talk: &mut Talk, stage: &str, salt: u32| {
        let line = lines::persuade_line(&speaker, stage, salt);
        talk.say(Topic::Persuade, line);
    };
    let standing = record.as_ref().map_or(0, |r| r.standings.get(faction).value());
    if record.as_ref().is_some_and(|r| r.refuses(who)) {
        return said(&mut talk, "not_now", 0x66);
    }
    let (wisdom, class) = character.as_ref().map_or((0, fb_faction::ClassKind::Fighter), |c| (crate::character::modifier(c.scores().wisdom), c.class.faction_class()));
    let charm = inventory.as_ref().map_or(0, |i| i.equipped().iter().map(|(_, item)| item.profile.stats.charisma).sum::<f32>().round() as i32);
    let check = persuade::check(&format!("Win over {}", speaker.name), standing, wisdom, class, charm);
    // Drawn from the folk's own seed and how often they have been tried, so each try rolls afresh
    // and the same try rolls the same.
    let tries = record.as_ref().map_or(0, |r| r.persuasion_tries(who));
    // The try is on the record as it starts, so a reload mid-roll does not replay it.
    if let Some(record) = record.as_deref_mut() {
        record.note_persuasion(who);
    }
    let mut rng = Mulberry32::new(speaker.seed.wrapping_mul(0x9E37_79B1) ^ 0x5052 ^ tries.wrapping_mul(0x85EB_CA6B) ^ speaker.depth << 20);
    let roll = check.roll(&mut rng);
    let result = if roll.succeeded { format!("+{} standing with {}", persuade::gain(&roll), speaker.faction) } else { format!("\u{2212}{} standing with {}", persuade::FAIL_COST, speaker.faction) };
    pending.0 = Some(Pending { folk: entity, faction, who });
    said(&mut talk, "hear", 0x60);
    talk.waiting = true;
    rolls.write(crate::dice::RollRequest { roll, purpose: crate::dice::RollPurpose::Persuade(entity), result: Some(result) });
}

/// Applies a persuasion's outcome once its dice have closed: standing with the folk's faction
/// rises by [`persuade::gain`] or falls by [`persuade::FAIL_COST`], and the folk will not be
/// tried again until the delver has rested.
#[allow(clippy::too_many_arguments)]
fn finish_persuasion(
    mut done: MessageReader<crate::dice::RollDone>,
    mut talk: ResMut<Talk>,
    mut pending: ResMut<Persuading>,
    mut record: Option<ResMut<WorldRecord>>,
    mut toasts: Toasts,
    mut open: ResMut<ScreenOpen>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
) {
    for finished in done.read() {
        let crate::dice::RollPurpose::Persuade(entity) = finished.purpose else { continue };
        let Some(this) = pending.0.take_if(|p| p.folk == entity) else { continue };
        talk.waiting = false;
        let speaker = talk.speaker.clone();
        let say = |talk: &mut Talk, stage: &str, salt: u32| {
            if let (Some(speaker), true) = (&speaker, talk.folk == Some(entity)) {
                let line = lines::persuade_line(speaker, stage, salt);
                talk.say(Topic::Persuade, line);
            }
        };
        let faction = speaker.as_ref().map_or_else(|| "They".to_string(), |s| s.faction.clone());
        if let Some(record) = record.as_deref_mut() {
            let rests = record.rests;
            record.refused.insert(this.who, rests);
            let change = if finished.roll.succeeded { persuade::gain(&finished.roll) } else { -persuade::FAIL_COST };
            record.standings.shift(this.faction, change);
        }
        let here = talk.folk == Some(entity);
        if finished.roll.succeeded {
            say(&mut talk, "pass", 0x64);
            toasts.say(format!("{} thinks better of you", capital_first(&faction)));
        } else if let (true, true, Some(speaker)) = (here, talk.hostile, &speaker) {
            // Failed, a hostile folk ends the parley and attacks: the panel closes and they are
            // let go to the fight ([`end_talk`]).
            toasts.say(format!("\u{201c}{}\u{201d}", lines::persuade_line(speaker, "fail", 0x63)));
            show_screen(&mut open, None, cursor.as_deref_mut().map(|c| &mut **c));
        } else {
            say(&mut talk, "fail", 0x63);
            toasts.say(format!("{} thinks less of you for asking", capital_first(&faction)));
        }
    }
}

/// `text` with its first letter capitalised: "the Chime" is "The Chime".
fn capital_first(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map(|c| c.to_uppercase().chain(letters).collect()).unwrap_or_default()
}

/// However the screen closes, lets the folk go on, and ends a talk whose folk fell or turned on
/// the delver.
#[allow(clippy::type_complexity)]
fn end_talk(
    mut open: ResMut<ScreenOpen>,
    mut talk: ResMut<Talk>,
    mut cursor: Option<Single<&mut CursorOptions, With<PrimaryWindow>>>,
    mut folk: Query<(Option<&Hostility>, Option<&mut Routine>, Has<Corpse>)>,
    mut toasts: Toasts,
    mut commands: Commands,
) {
    let Some(entity) = talk.folk else { return };
    let gone = match folk.get(entity) {
        Ok((hostility, _, dead)) => dead || (!talk.parley && hostility.is_some_and(|h| h.is_hostile())),
        Err(_) => true,
    };
    // The enchanter's bench is the talk going on, and holds the folk the same.
    let talking = open.is(Screen::Talk) || open.is(Screen::Enchant);
    if talking && gone {
        let turned = folk.get(entity).is_ok_and(|(hostility, _, dead)| !dead && !talk.parley && hostility.is_some_and(|h| h.is_hostile()));
        if let (true, Some(speaker)) = (turned, talk.speaker.as_ref()) {
            toasts.say(format!("{} turns on you.", speaker.name));
        }
        show_screen(&mut open, None, cursor.as_deref_mut().map(|c| &mut **c));
    }
    if !talking || gone {
        if let Ok((_, Some(mut routine), _)) = folk.get_mut(entity) {
            routine.release();
        }
        // The parley is over: a hostile folk goes on with the fight, unless they were won over.
        if let Ok(mut parleyed) = commands.get_entity(entity) {
            parleyed.remove::<Parleying>();
        }
        let revision = talk.revision + 1;
        *talk = Talk { revision, ..Talk::default() };
    }
}

/// The colour a band reads in.
fn band_colour(band: Band) -> Color {
    match band {
        Band::Hostile => BAD,
        Band::Wary => ACCENT,
        Band::Neutral => MUTED,
        Band::Friendly | Band::Allied => OK,
    }
}

/// The key that leaves the talk, for the panel's foot: "F or Esc".
fn leave_keys(keymap: &Keymap) -> String {
    let mut keys = keymap.binding(Action::Primary).map(|b| b.labels()).unwrap_or_default();
    keys.extend(keymap.binding(Action::Back).map(|b| b.labels()).unwrap_or_default());
    keys.dedup();
    keys.join(" or ")
}

/// What the panel shows for a line `said` in plain words on `topic` (`None` is the greeting),
/// and whether the delver can read it: a folk's line always, a denizen's once every word it
/// needs is learned, else the tongue's glyphs, steady for the line ([`lines::glyph_seed`]).
pub fn shown(speaker: &Speaker, topic: Option<Topic>, said: &str, lexicon: &Lexicon) -> (String, bool) {
    match speaker.denizen {
        // The refusal to barter is what the denizen does, not what it says.
        Some(_) if topic == Some(Topic::Service(Service::Barter)) => (said.to_string(), true),
        Some(kind) if !lexicon.readable(kind, &lines::line_needs(speaker, topic)) => (fb_denizens::tongue::glyphs(kind, said, lines::glyph_seed(speaker, topic)), false),
        _ => (said.to_string(), true),
    }
}

/// Draws the panel while the talk is open, and again whenever it changes.
#[allow(clippy::too_many_arguments)]
fn build_talk_screen(
    mut commands: Commands,
    open: Res<ScreenOpen>,
    talk: Res<Talk>,
    keymap: Option<Res<Keymap>>,
    record: Option<Res<WorldRecord>>,
    fonts: Option<Res<UiFonts>>,
    cameras: Query<Entity, With<ViewModelCamera>>,
    screens: Query<Entity, With<TalkScreen>>,
    mut drawn: Local<Option<(u32, u32)>>,
) {
    let existing = screens.iter().next();
    let (true, Some(speaker), Some(fonts)) = (open.is(Screen::Talk), talk.speaker.as_ref(), fonts) else {
        if let Some(screen) = existing {
            commands.entity(screen).despawn();
        }
        *drawn = None;
        return;
    };
    // A denizen's lines depend on the words the delver knows, so a change in them draws again.
    let empty = Lexicon::new();
    let lexicon = record.as_ref().map_or(&empty, |r| r.lexicon());
    let known = talk.tongue.map_or(0, |k| (lexicon.known_share(k) * 1000.0) as u32);
    if existing.is_some() && *drawn == Some((talk.revision, known)) {
        return;
    }
    let Some(camera) = cameras.iter().next() else { return };
    let fallback = Keymap::default();
    let leave = leave_keys(keymap.as_deref().unwrap_or(&fallback));
    // Redraw in place: the root stays and its contents are laid out again.
    let root = match existing {
        Some(root) => {
            commands.entity(root).despawn_children();
            root
        }
        None => commands
            .spawn((
                Name::new("Talk"),
                TalkScreen,
                DespawnOnExit(crate::launch::AppState::Delving),
                UiTargetCamera(camera),
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::Center,
                    padding: UiRect::right(px(PANEL_MARGIN)),
                    ..default()
                },
                GlobalZIndex(12),
            ))
            .id(),
    };
    *drawn = Some((talk.revision, known));
    let trade = speaker.trade.map(|t| t.label());
    commands.entity(root).with_children(|root| {
        root.spawn((
            Name::new("Talk panel"),
            Node {
                width: px(PANEL_WIDTH),
                max_width: percent(96),
                max_height: percent(94),
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                padding: UiRect::new(px(26), px(26), px(20), px(22)),
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(14)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(BG),
            BorderColor::all(LINE),
            BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.55), px(0), px(14), px(0), px(34)),
        ))
        .with_children(|panel| {
            panel.spawn((Text::new(speaker.name.clone()), UiFonts::style(&fonts.display, 34.0, INK)));
            let who = match &trade {
                Some(trade) => format!("{} \u{b7} {trade}", speaker.folk),
                None => speaker.folk.clone(),
            };
            panel.spawn((Text::new(who), UiFonts::style(&fonts.body_italic, 18.0, MUTED)));
            panel.spawn((Text::default(), UiFonts::style(&fonts.body, 18.0, MUTED))).with_children(|line| {
                let mut letters = speaker.faction.chars();
                let faction: String = letters.next().map(|c| c.to_uppercase().chain(letters).collect()).unwrap_or_default();
                line.spawn((TextSpan::new(format!("{faction} \u{b7} ")), UiFonts::style(&fonts.body, 18.0, MUTED)));
                line.spawn((TextSpan::new(speaker.band.label()), UiFonts::style(&fonts.body_medium, 18.0, band_colour(speaker.band))));
            });
            panel.spawn((Node { height: px(1), margin: UiRect::vertical(px(4)), ..default() }, BackgroundColor(FAINT)));
            if let Some(kind) = talk.tongue {
                let share = (lexicon.known_share(kind) * 100.0).round() as u32;
                panel.spawn((Text::new(format!("{} \u{2014} you know {share}%", fb_denizens::tongue::tongue_name(kind))), UiFonts::style(&fonts.body_italic, 16.0, MUTED)));
            }
            let (heading, said, colour, topic) = match &talk.answer {
                Some((topic, said)) => (Some(topic.label_for(speaker)), said.clone(), BRIGHT, Some(*topic)),
                None => (None, talk.greeting.clone(), INK, None),
            };
            if let Some(heading) = heading {
                panel.spawn((Text::new(heading), UiFonts::style(&fonts.mono, 13.0, MUTED)));
            }
            // A denizen's line the delver cannot read yet is its script, dimmed and in italics.
            let (text, readable) = shown(speaker, topic, &said, lexicon);
            if readable {
                panel.spawn((Text::new(format!("\u{201c}{text}\u{201d}")), UiFonts::style(&fonts.body, 22.0, colour)));
            } else {
                // Any character may break the line, so a glyph run with no spaces still wraps in the panel.
                panel.spawn((Text::new(text), TextLayout::new(Justify::Left, LineBreak::AnyCharacter), UiFonts::style(&fonts.body_italic, 22.0, MUTED)));
            }
            panel.spawn((Node { height: px(1), margin: UiRect::vertical(px(4)), ..default() }, BackgroundColor(FAINT)));
            for (i, topic) in talk.topics.iter().enumerate() {
                panel
                    .spawn((
                        TopicButton(i),
                        Node {
                            align_items: AlignItems::Center,
                            column_gap: px(14),
                            padding: UiRect::new(px(14), px(14), px(8), px(9)),
                            border: UiRect::all(px(1.0)),
                            border_radius: BorderRadius::all(px(8)),
                            ..default()
                        },
                        BackgroundColor(CELL),
                        BorderColor::all(FAINT),
                    ))
                    .with_children(|row| {
                        row.spawn((Text::new((i + 1).to_string()), UiFonts::style(&fonts.mono_bold, 16.0, ACCENT)));
                        row.spawn((Text::new(topic.label_for(speaker)), UiFonts::style(&fonts.body, 21.0, INK)));
                    });
            }
            panel.spawn((Node { margin: UiRect::top(px(4)), ..default() }, Text::new(format!("1-{} choose \u{b7} {leave} leave", talk.topics.len().max(1))), UiFonts::style(&fonts.mono, 13.0, MUTED)));
        });
    });
}

/// Lights the topic under the mouse.
fn hover_topics(
    open: Res<ScreenOpen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
    mut buttons: Query<(&TopicButton, &ComputedNode, &UiGlobalTransform, &mut BackgroundColor, &mut BorderColor)>,
) {
    if !open.is(Screen::Talk) {
        return;
    }
    let point = windows.iter().next().and_then(|w| ui_cursor(w, ui_scale.as_deref()));
    for (_, node, at, mut fill, mut border) in &mut buttons {
        let over = point.is_some_and(|p| node_rect(node, at).contains(p));
        let (want_fill, want_border) = if over { (FAINT, LINE) } else { (CELL, FAINT) };
        if fill.0 != want_fill {
            fill.0 = want_fill;
        }
        if *border != BorderColor::all(want_border) {
            *border = BorderColor::all(want_border);
        }
    }
}
