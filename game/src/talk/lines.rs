//! What folk say, as plain data in and plain text out. Every line is expanded from the small
//! grammars of `data/talk.json` with `procgen_grammar`, seeded by the folk's own seed and the
//! topic, so the same folk always say the same thing. Nothing here knows Bevy.

use fb_faction::{Band, Intention, Profession, Tier, Trade};
use procgen_grammar::{Grammar, expand_with};
use procgen_rng::Mulberry32;
use std::sync::OnceLock;

/// What a folk will do when the delver speaks to them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reception {
    /// They spit at the delver's feet and say nothing.
    Spits,
    /// They say one line and turn away.
    Turns,
    /// They talk.
    Talks,
}

/// How a folk receives the delver: a Hostile faction, or a folk who has turned on them, spits;
/// a Wary one turns away; the rest talk.
pub fn reception(band: Band, turned_on_delver: bool) -> Reception {
    match band {
        _ if turned_on_delver => Reception::Spits,
        Band::Hostile => Reception::Spits,
        Band::Wary => Reception::Turns,
        Band::Neutral | Band::Friendly | Band::Allied => Reception::Talks,
    }
}

/// The toast for a folk who will not speak: "Hilda Stonefoot spits at your feet."
pub fn spit_line(speaker: &Speaker) -> String {
    format!("{} spits at your feet.", speaker.name)
}

/// A service a trade offers beyond talk. Each has a screen of its own, which later work builds;
/// until it does, the topic answers "Not yet." ([`super::service_ready`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Service {
    Barter,
    Commission,
    /// Collecting what was commissioned of this folk; offered only to a delver with an order
    /// with them.
    Order,
    Enchant,
    Scrap,
}

/// Something to ask a folk about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Topic {
    Who,
    Trade,
    Service(Service),
    Level,
    WayOut,
}

impl Topic {
    /// The line the panel lists it by.
    pub fn label(self) -> &'static str {
        match self {
            Topic::Who => "Who are you?",
            Topic::Trade => "Your trade",
            Topic::Service(Service::Barter) => "Barter",
            Topic::Service(Service::Commission) => "Commission",
            Topic::Service(Service::Order) => "Your order",
            Topic::Service(Service::Enchant) => "Enchant",
            Topic::Service(Service::Scrap) => "Scrap",
            Topic::Level => "This level",
            Topic::WayOut => "The way out",
        }
    }
}

/// Who is speaking, and the little of the level they know.
#[derive(Clone, Debug, PartialEq)]
pub struct Speaker {
    pub name: String,
    /// Their folk's label: "Dwarf".
    pub folk: String,
    pub faction: String,
    pub intention: Intention,
    pub band: Band,
    pub trade: Option<Trade>,
    /// The folk's own seed, which every line they say is drawn from.
    pub seed: u32,
    pub depth: u32,
    /// Every faction that holds ground on the level, by name.
    pub level_factions: Vec<String>,
    /// The traders of the speaker's faction on the level, as (name, profession, where they stand
    /// from the speaker's post: "in the market, past the hall").
    pub traders: Vec<(String, Profession, String)>,
}

/// A master the world places deeper down, for a rumour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MasterRumour {
    /// "master weaponsmith (blades)".
    pub calling: String,
    pub faction: String,
    /// How many levels below the speaker.
    pub levels: u32,
}

/// The topics a folk offers, in order: who they are; for a trade, the trade and what it offers
/// beyond talk; the level; the way out.
pub fn topics(speaker: &Speaker) -> Vec<Topic> {
    let mut list = vec![Topic::Who];
    if let Some(trade) = speaker.trade {
        list.push(Topic::Trade);
        if trade.shop("", "").is_some() {
            list.push(Topic::Service(Service::Barter));
        }
        match trade.profession {
            Profession::Armorer | Profession::Weaponsmith | Profession::Bowyer | Profession::Tentmaker => list.push(Topic::Service(Service::Commission)),
            Profession::Enchanter => list.extend([Topic::Service(Service::Enchant), Topic::Service(Service::Scrap)]),
            _ => {}
        }
    }
    list.extend([Topic::Level, Topic::WayOut]);
    list
}

/// What they say on being greeted, warmer as the faction thinks better of the delver.
pub fn greeting(speaker: &Speaker) -> String {
    let symbol = match speaker.band {
        Band::Allied => "greet_allied",
        Band::Friendly => "greet_friendly",
        _ => "greet_neutral",
    };
    say(speaker, symbol, 0x01, &[])
}

/// The one line a Wary folk says; they keep walking, which reads as turning away.
pub fn wary_line(speaker: &Speaker) -> String {
    say(speaker, "wary", 0x02, &[])
}

/// "Apprentice weaponsmith (blades)", lower case, as a trade reads mid-sentence.
fn calling(trade: &Trade) -> String {
    trade.label().to_lowercase()
}

/// Their answer to `topic`. `rumour` is a master deeper down, if the caller found one.
pub fn answer(speaker: &Speaker, topic: Topic, rumour: Option<&MasterRumour>) -> String {
    match topic {
        Topic::Who => who(speaker),
        Topic::Trade => trade(speaker),
        Topic::Service(_) => say(speaker, "not_yet", 0x05, &[]),
        Topic::Level => level(speaker),
        Topic::WayOut => way_out(speaker, rumour),
    }
}

fn who(speaker: &Speaker) -> String {
    let mut text = say(speaker, &format!("who_{}", speaker.intention.key()), 0x03, &[]);
    match speaker.trade {
        Some(trade) if trade.profession == Profession::Guard => {
            text.push(' ');
            text.push_str(&say(speaker, "who_guard", 0x04, &[]));
        }
        Some(trade) => {
            text.push(' ');
            text.push_str(&say(speaker, "who_calling", 0x04, &[("calling", calling(&trade))]));
        }
        None => {}
    }
    text
}

fn trade(speaker: &Speaker) -> String {
    let Some(trade) = speaker.trade else { return say(speaker, "not_yet", 0x06, &[]) };
    let key = match trade.profession {
        Profession::Armorer => "trade_armorer",
        Profession::Weaponsmith => "trade_weaponsmith",
        Profession::Bowyer => "trade_bowyer",
        Profession::Tentmaker => "trade_tentmaker",
        Profession::GeneralGoods => "trade_general_goods",
        Profession::Alchemist => "trade_alchemist",
        Profession::Enchanter => "trade_enchanter",
        Profession::Guard => "trade_guard",
    };
    if trade.profession == Profession::Guard {
        return format!("{} {}", say(speaker, key, 0x07, &[]), guard_traders(speaker));
    }
    let tier = match trade.tier {
        Tier::Novice => "tier_novice",
        Tier::Apprentice => "tier_apprentice",
        Tier::Expert => "tier_expert",
        Tier::Master => "tier_master",
    };
    let mut text = say(speaker, key, 0x07, &[]);
    if let Some(style) = trade.style {
        text.push_str(&format!(" I favour {}.", style.label()));
    }
    text.push(' ');
    text.push_str(&say(speaker, tier, 0x08, &[]));
    text
}

/// A guard points the delver to a trader of the faction, if the level has one.
fn guard_traders(speaker: &Speaker) -> String {
    if speaker.traders.is_empty() {
        return say(speaker, "guard_no_traders", 0x09, &[]);
    }
    let mut rng = Mulberry32::new(speaker.seed ^ 0x7A);
    let (name, profession, place) = rng.pick(&speaker.traders).clone();
    say(speaker, "guard_traders", 0x09, &[("trader", name), ("trader_calling", profession.label().to_string()), ("trader_place", place)])
}

/// Where `other` stands, as one at `from` would point to it: "in the market, past the hall". A
/// guard stands at the hall door, so every room but the hall itself is past it.
pub fn place_of(level: &fb_level::Level, from: &fb_level::FolkSpot, other: &fb_level::FolkSpot) -> String {
    use fb_level::RoomRole;
    let room_of = |s: &fb_level::FolkSpot| level.room_at(s.x as i32, s.y as i32);
    let Some(there) = room_of(other) else { return "somewhere on our ground".to_string() };
    let roles = level.roles_of(there);
    let named = [
        (RoomRole::Market, "in the market"),
        (RoomRole::Hearth, "by the hearth"),
        (RoomRole::Sleeping, "where we sleep"),
        (RoomRole::Store, "among the stores"),
        (RoomRole::Hall, "here in the hall"),
    ];
    let Some(&(_, text)) = named.iter().find(|(r, _)| roles.contains(r)) else { return "somewhere on our ground".to_string() };
    let at_hall = room_of(from).is_some_and(|h| level.roles_of(h).contains(&RoomRole::Hall));
    if at_hall && !roles.contains(&RoomRole::Hall) { format!("{text}, past the hall") } else { text.to_string() }
}

fn level(speaker: &Speaker) -> String {
    let names = &speaker.level_factions;
    let list = match names.as_slice() {
        [] => "no one".to_string(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    };
    let extras = [("factions", list), ("count", count_word(names.len())), ("ordinal", ordinal(speaker.depth))];
    say(speaker, if names.len() <= 1 { "level_alone" } else { "level" }, 0x0A, &extras)
}

fn way_out(speaker: &Speaker, rumour: Option<&MasterRumour>) -> String {
    if let Some(m) = rumour {
        let levels = if m.levels == 1 { "one level".to_string() } else { format!("{} levels", count_word(m.levels as usize).to_lowercase()) };
        return say(speaker, "way_master", 0x0B, &[("master_calling", m.calling.clone()), ("master_faction", m.faction.clone()), ("master_levels", levels)]);
    }
    say(speaker, "way_out", 0x0B, &[])
}

/// Whether this folk has heard of a master deeper down: more often if they have a trade.
pub fn knows_a_master(speaker: &Speaker) -> bool {
    let mut rng = Mulberry32::new(speaker.seed ^ 0x4D41);
    rng.chance(if speaker.trade.is_some() { 0.6 } else { 0.25 })
}

/// "one" to "twelve", then the figure.
pub fn count_word(n: usize) -> String {
    const WORDS: [&str; 13] = ["No", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten", "Eleven", "Twelve"];
    WORDS.get(n).map_or_else(|| n.to_string(), |w| w.to_string())
}

/// "first", "second" ... "tenth", then "11th".
pub fn ordinal(n: u32) -> String {
    const WORDS: [&str; 10] = ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth"];
    if let Some(word) = n.checked_sub(1).and_then(|i| WORDS.get(i as usize)) {
        return (*word).to_string();
    }
    let suffix = match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// The shipped grammar.
pub fn grammar() -> &'static Grammar {
    static DATA: OnceLock<Grammar> = OnceLock::new();
    DATA.get_or_init(|| Grammar::from_json(include_str!("../../../data/talk.json")).expect("data/talk.json parses"))
}

/// The symbols every line may use, from the speaker.
fn base(speaker: &Speaker) -> Vec<(&'static str, String)> {
    let mut symbols = vec![("name", speaker.name.clone()), ("folk", speaker.folk.to_lowercase()), ("faction", speaker.faction.clone())];
    if let Some(trade) = speaker.trade {
        symbols.push(("calling", calling(&trade)));
    }
    symbols
}

/// A line from the grammar's `symbol`, drawn from the speaker's seed and `salt` (one per kind of
/// line, so a folk's lines don't all pick alike), with `extra` symbols defined for it.
fn say(speaker: &Speaker, symbol: &str, salt: u32, extra: &[(&'static str, String)]) -> String {
    let mut symbols = base(speaker);
    symbols.extend(extra.iter().cloned());
    let overlay = Grammar { symbols: symbols.into_iter().map(|(k, v)| (k.to_string(), vec![v])).collect() };
    let grammar = grammar().merged(&overlay);
    let mut rng = Mulberry32::new(speaker.seed.wrapping_mul(0x9E37_79B1) ^ salt);
    expand_with(&grammar, &format!("#{symbol}#"), &mut rng).unwrap_or_else(|_| "...".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_faction::Style;

    fn speaker(trade: Option<Trade>, band: Band) -> Speaker {
        Speaker {
            name: "Hilda Stonefoot".into(),
            folk: "Dwarf".into(),
            faction: "the Ash Choir".into(),
            intention: Intention::Trade,
            band,
            trade,
            seed: 4242,
            depth: 3,
            level_factions: vec!["the Ash Choir".into(), "the Scale Court".into(), "the Lantern-keepers".into()],
            traders: vec![("Borin Ironhand".into(), Profession::Armorer, "in the market, past the hall".into())],
        }
    }

    fn smith() -> Trade {
        Trade { profession: Profession::Weaponsmith, tier: Tier::Apprentice, style: Some(Style::Blades) }
    }

    /// Every symbol a line refers to is defined, for every folk, whatever the level.
    #[test]
    fn the_grammar_is_sound_with_the_symbols_a_speaker_supplies() {
        let mut symbols = base(&speaker(Some(smith()), Band::Neutral));
        symbols.extend(["trader", "trader_calling", "trader_place", "factions", "count", "ordinal", "master_calling", "master_faction", "master_levels"].map(|k| (k, "x".to_string())));
        let overlay = Grammar { symbols: symbols.into_iter().map(|(k, v)| (k.to_string(), vec![v])).collect() };
        let grammar = grammar().merged(&overlay);
        for symbol in starts::START_SYMBOLS {
            assert!(grammar.problems(&format!("#{symbol}#")).is_empty(), "{symbol}: {:?}", grammar.problems(&format!("#{symbol}#")));
        }
        assert!(grammar.problems("#greet_neutral#").is_empty());
    }

    /// Every symbol the grammar has gives text from every speaker kind and never the error line.
    #[test]
    fn every_line_expands() {
        for seed in 0..40 {
            for trade in [None, Some(smith()), Some(Trade::guard()), Some(Trade { profession: Profession::Enchanter, tier: Tier::Master, style: None })] {
                for band in [Band::Neutral, Band::Friendly, Band::Allied, Band::Wary] {
                    let s = Speaker { seed, ..speaker(trade, band) };
                    let rumour = MasterRumour { calling: "master alchemist".into(), faction: "the Scale Court".into(), levels: 8 };
                    let mut lines = vec![greeting(&s), wary_line(&s), answer(&s, Topic::Who, None), answer(&s, Topic::Level, None), answer(&s, Topic::WayOut, None), answer(&s, Topic::WayOut, Some(&rumour))];
                    lines.extend(topics(&s).into_iter().map(|t| answer(&s, t, None)));
                    for line in lines {
                        assert!(!line.contains("...") && !line.contains('#') && !line.is_empty(), "seed {seed}: {line:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_same_folk_say_the_same_thing() {
        let s = speaker(Some(smith()), Band::Friendly);
        assert_eq!(greeting(&s), greeting(&s.clone()));
        assert_eq!(answer(&s, Topic::Who, None), answer(&s, Topic::Who, None));
        assert_eq!(answer(&s, Topic::Trade, None), answer(&s, Topic::Trade, None));
    }

    #[test]
    fn different_folk_do_not_all_say_the_same() {
        let greetings: std::collections::HashSet<String> = (0..30).map(|seed| greeting(&Speaker { seed, ..speaker(None, Band::Neutral) })).collect();
        assert!(greetings.len() > 3, "{greetings:?}");
    }

    #[test]
    fn the_bands_decide_who_talks() {
        assert_eq!(reception(Band::Hostile, false), Reception::Spits);
        assert_eq!(reception(Band::Wary, false), Reception::Turns);
        for band in [Band::Neutral, Band::Friendly, Band::Allied] {
            assert_eq!(reception(band, false), Reception::Talks);
            assert_eq!(reception(band, true), Reception::Spits, "a folk who turned on the delver will not talk");
        }
        assert_eq!(spit_line(&speaker(None, Band::Hostile)), "Hilda Stonefoot spits at your feet.");
    }

    #[test]
    fn trades_add_their_topics() {
        let labels = |t| topics(&speaker(t, Band::Neutral)).into_iter().map(Topic::label).collect::<Vec<_>>();
        assert_eq!(labels(None), ["Who are you?", "This level", "The way out"]);
        assert_eq!(labels(Some(smith())), ["Who are you?", "Your trade", "Barter", "Commission", "This level", "The way out"]);
        assert_eq!(labels(Some(Trade { profession: Profession::Enchanter, tier: Tier::Expert, style: None })), ["Who are you?", "Your trade", "Enchant", "Scrap", "This level", "The way out"]);
        assert_eq!(labels(Some(Trade { profession: Profession::GeneralGoods, tier: Tier::Novice, style: None })), ["Who are you?", "Your trade", "Barter", "This level", "The way out"]);
        assert_eq!(labels(Some(Trade::guard())), ["Who are you?", "Your trade", "This level", "The way out"]);
    }

    #[test]
    fn services_are_not_yet() {
        let s = speaker(Some(smith()), Band::Neutral);
        assert_eq!(answer(&s, Topic::Service(Service::Commission), None), "Not yet.");
    }

    #[test]
    fn a_guard_names_a_trader_of_its_faction_or_says_there_is_none() {
        let guard = speaker(Some(Trade::guard()), Band::Neutral);
        let said = answer(&guard, Topic::Trade, None);
        assert!(said.contains("Borin Ironhand") && said.contains("armorer"), "{said}");
        let none = Speaker { traders: Vec::new(), ..guard };
        let said = answer(&none, Topic::Trade, None);
        assert!(!said.contains("Borin") && (said.contains("no trade") || said.contains("no market") || said.contains("Nobody")), "{said}");
    }

    #[test]
    fn the_trade_line_names_the_style_and_the_tier() {
        let said = answer(&speaker(Some(smith()), Band::Neutral), Topic::Trade, None);
        assert!(said.contains("I favour blades."), "{said}");
    }

    #[test]
    fn the_level_line_names_every_faction() {
        let said = answer(&speaker(None, Band::Neutral), Topic::Level, None);
        for name in ["Ash Choir", "Scale Court", "Lantern-keepers"] {
            assert!(said.to_lowercase().contains(&name.to_lowercase()), "{said}");
        }
        assert!(said.contains("third"), "{said}");
    }

    #[test]
    fn a_rumoured_master_is_named() {
        let rumour = MasterRumour { calling: "master weaponsmith (blades)".into(), faction: "the Scale Court".into(), levels: 5 };
        let said = answer(&speaker(None, Band::Neutral), Topic::WayOut, Some(&rumour));
        assert!(said.contains("master weaponsmith (blades)") && said.contains("the Scale Court") && said.contains("five levels"), "{said}");
    }

    #[test]
    fn counting_words() {
        assert_eq!(count_word(3), "Three");
        assert_eq!(ordinal(1), "first");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(22), "22nd");
        assert_eq!(ordinal(31), "31st");
    }

    mod starts {
        /// The symbols a line is started from.
        pub const START_SYMBOLS: [&str; 27] = [
            "greet_neutral", "greet_friendly", "greet_allied", "wary", "who_way_out", "who_trade", "who_worship", "who_plunder", "who_calling", "who_guard", "trade_armorer", "trade_weaponsmith",
            "trade_bowyer", "trade_tentmaker", "trade_general_goods", "trade_alchemist", "trade_enchanter", "trade_guard", "guard_traders", "guard_no_traders", "tier_novice", "tier_master", "level",
            "level_alone", "way_out", "way_master", "not_yet",
        ];
    }
}
