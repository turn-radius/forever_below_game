//! What a note says. A note found in the abyss tells of the level it lies on: a creature that
//! killed the writer's companions, a trader worth seeing, a book left in the library, a faction's
//! hatreds. Its words are a tracery expansion of `data/notes.json`, filled with facts from the
//! level's [`Lore`] so the note is true of the world it is found in.
//!
//! The grammar has no tracery actions, so [`write_note`] picks the kind of note and the one fact
//! it tells of first, then binds single-valued symbols (`#creature#`, `#trader#`, `#hater#` and
//! the like) to that fact; a note names the same troll in every sentence. A kind is drawn only
//! when the lore has the facts it needs, and a lament needs none, so every note has words.
//!
//! Which kinds there are, their weights and the words are our own proposal.

use procgen_grammar::Grammar;
use procgen_rng::Mulberry32;
use std::sync::LazyLock;

use crate::lesson;
use crate::naming;
use crate::stats::Stat;

/// Salt for a note's words, so they never move the note's shape or anything else of its seed.
const WORDS_SALT: u32 = 0x407E_5A17;

/// A thing a faction likes or hates, or a creature: one of it and many of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Thing {
    /// "troll", "dwarf", "the Ash Choir".
    pub one: String,
    /// "trolls", "dwarves", "the Ash Choir".
    pub many: String,
}

impl Thing {
    pub fn new(one: impl Into<String>, many: impl Into<String>) -> Thing {
        Thing { one: one.into(), many: many.into() }
    }
}

/// A faction's liking or hatred of a [`Thing`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Feeling {
    /// The faction, as the grammar names it ("the Scale Court").
    pub faction: String,
    pub thing: Thing,
}

/// Someone who trades on a level.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trader {
    pub name: String,
    pub faction: String,
    /// Their trade, as the talk window writes it ("alchemist").
    pub trade: String,
    /// The key of what they sell in `data/notes.json`: `wares_<key>` ("alchemist").
    pub wares: String,
}

/// The keys of what traders sell, one per trading profession: `wares_<key>` in `data/notes.json`.
pub const WARES: [&str; 7] = ["armorer", "weaponsmith", "bowyer", "tentmaker", "general_goods", "alchemist", "enchanter"];

/// A book lying on a level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Book {
    /// What the room it lies in was built for ("library"), or `None` for a room with no purpose.
    pub place: Option<String>,
    /// What it teaches.
    pub teaches: Stat,
}

/// What a level knows of itself, as plain words: everything a note found on it may tell of.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lore {
    /// How deep the level lies, 1 for the first.
    pub depth: u32,
    /// What the level was built for ("temple"), if it remembers.
    pub past: Option<String>,
    /// The factions on it, as the grammar names them.
    pub factions: Vec<String>,
    pub likes: Vec<Feeling>,
    pub hates: Vec<Feeling>,
    /// The kinds of creature on it.
    pub creatures: Vec<Thing>,
    /// What its rooms were built for ("library", "ossuary"), each once.
    pub places: Vec<String>,
    pub traders: Vec<Trader>,
    pub books: Vec<Book>,
}

/// The kinds of note, each an `origin` symbol of `data/notes.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteKind {
    /// Fled a creature on the level.
    Creature,
    /// Points to a trader.
    Trader,
    /// Left a book in one of the level's rooms.
    Book,
    /// A faction's hatred.
    Hate,
    /// A faction's liking.
    Like,
    /// A warning about one of the level's rooms.
    Place,
    /// A word on one of the level's factions.
    Faction,
    /// Despair, needing no facts.
    Lament,
}

impl NoteKind {
    pub const ALL: [NoteKind; 8] =
        [NoteKind::Creature, NoteKind::Trader, NoteKind::Book, NoteKind::Hate, NoteKind::Like, NoteKind::Place, NoteKind::Faction, NoteKind::Lament];

    /// Its symbol in `data/notes.json`, apart from the fact symbols bound into it ([`bind`]).
    pub fn key(self) -> &'static str {
        match self {
            NoteKind::Creature => "note_creature",
            NoteKind::Trader => "note_trader",
            NoteKind::Book => "note_book",
            NoteKind::Hate => "note_hate",
            NoteKind::Like => "note_like",
            NoteKind::Place => "note_place",
            NoteKind::Faction => "note_faction",
            NoteKind::Lament => "note_lament",
        }
    }

    /// How often it is drawn among the kinds the lore can tell. A lament is rarer, so a level
    /// with facts mostly gets notes that tell of them.
    pub fn weight(self) -> u32 {
        match self {
            NoteKind::Creature | NoteKind::Trader | NoteKind::Hate => 4,
            NoteKind::Book => 3,
            NoteKind::Like | NoteKind::Place | NoteKind::Faction => 2,
            NoteKind::Lament => 1,
        }
    }

    /// Whether `lore` has the facts this kind tells of.
    pub fn told_by(self, lore: &Lore) -> bool {
        match self {
            NoteKind::Creature => !lore.creatures.is_empty(),
            NoteKind::Trader => !lore.traders.is_empty(),
            NoteKind::Book => !lore.books.is_empty(),
            NoteKind::Hate => !lore.hates.is_empty(),
            NoteKind::Like => !lore.likes.is_empty(),
            NoteKind::Place => !lore.places.is_empty(),
            NoteKind::Faction => !lore.factions.is_empty(),
            NoteKind::Lament => true,
        }
    }
}

static GRAMMAR: LazyLock<Grammar> =
    LazyLock::new(|| Grammar::from_json(include_str!("../../../data/notes.json")).unwrap_or_else(|e| panic!("data/notes.json: {e}")));

/// The notes grammar as shipped, before a note's facts are bound into it.
pub fn grammar() -> &'static Grammar {
    &GRAMMAR
}

/// "1st", "2nd", "3rd", "11th", "22nd".
pub fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// The kind of the note of `seed` found where `lore` holds, from the note's own stream.
pub fn note_kind(seed: u32, lore: &Lore) -> NoteKind {
    pick_kind(&mut Mulberry32::new(seed ^ WORDS_SALT), lore)
}

fn pick_kind(rng: &mut Mulberry32, lore: &Lore) -> NoteKind {
    let kinds: Vec<(NoteKind, u32)> = NoteKind::ALL.into_iter().filter(|k| k.told_by(lore)).map(|k| (k, k.weight())).collect();
    *rng.weighted(&kinds)
}

/// The words of the note of `seed` found where `lore` holds: a pure function of the two.
pub fn write_note(seed: u32, lore: &Lore) -> String {
    let mut rng = Mulberry32::new(seed ^ WORDS_SALT);
    let kind = pick_kind(&mut rng, lore);
    let grammar = GRAMMAR.merged(&bind(kind, lore, &mut rng));
    let words = procgen_grammar::expand_with(&grammar, &format!("#{}#", kind.key()), &mut rng)
        .unwrap_or_else(|e| panic!("data/notes.json, {}: {e}", kind.key()));
    tidy(&words)
}

/// The symbols naming the world, each bound to one value so a note keeps to the one fact it
/// tells of. Every kind gets a writer, a companion and the depth; the kind's own fact fills the
/// rest.
fn bind(kind: NoteKind, lore: &Lore, rng: &mut Mulberry32) -> Grammar {
    let writer = naming::anyone(rng);
    // A companion of another name, so no one writes of themselves as lost.
    let mut companion = naming::anyone(rng);
    while companion == writer {
        companion = naming::anyone(rng);
    }
    let mut symbols: Vec<(&str, String)> = vec![
        ("writer", writer),
        ("companion_name", companion),
        ("depth", ordinal(lore.depth.max(1))),
        ("past_line", lore.past.as_ref().map_or_else(|| "something else once".to_string(), |p| format!("part of a {p} once"))),
    ];
    match kind {
        NoteKind::Creature => {
            let c = rng.pick(&lore.creatures);
            symbols.extend([("creature", c.one.clone()), ("creatures", c.many.clone())]);
        }
        NoteKind::Trader => {
            let t = rng.pick(&lore.traders);
            symbols.extend([
                ("trader", t.name.clone()),
                ("trader_faction", t.faction.clone()),
                ("trade", t.trade.clone()),
                ("wares", format!("#wares_{}#", t.wares)),
            ]);
        }
        NoteKind::Book => {
            let b = rng.pick(&lore.books);
            let topic = lesson::long_name(b.teaches).to_lowercase();
            symbols.extend([("book_place", b.place.clone().unwrap_or_else(|| "dark".to_string())), ("book_topic", format!("#topic_{topic}#"))]);
        }
        NoteKind::Hate => {
            let f = rng.pick(&lore.hates);
            symbols.extend([("hater", f.faction.clone()), ("hated", f.thing.many.clone())]);
        }
        NoteKind::Like => {
            let f = rng.pick(&lore.likes);
            symbols.extend([("liker", f.faction.clone()), ("liked", f.thing.many.clone())]);
        }
        NoteKind::Place => symbols.push(("place", rng.pick(&lore.places).clone())),
        NoteKind::Faction => symbols.push(("faction", rng.pick(&lore.factions).clone())),
        NoteKind::Lament => {}
    }
    Grammar { symbols: symbols.into_iter().map(|(k, v)| (k.to_string(), vec![v])).collect() }
}

/// The words with doubled and trailing spaces gone, which an empty sign-off leaves.
fn tidy(words: &str) -> String {
    words.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rich() -> Lore {
        Lore {
            depth: 3,
            past: Some("temple".into()),
            factions: vec!["the Scale Court".into(), "the Ash Choir".into()],
            likes: vec![Feeling { faction: "the Ash Choir".into(), thing: Thing::new("dwarf", "dwarves") }],
            hates: vec![Feeling { faction: "the Scale Court".into(), thing: Thing::new("giant spider", "giant spiders") }],
            creatures: vec![Thing::new("troll", "trolls")],
            places: vec!["library".into(), "ossuary".into()],
            traders: vec![Trader { name: "Bera Holt".into(), faction: "the Ash Choir".into(), trade: "alchemist".into(), wares: "alchemist".into() }],
            books: vec![Book { place: Some("library".into()), teaches: Stat::Dexterity }],
        }
    }

    /// The symbols the code binds, which the shipped grammar may refer to without defining.
    const BOUND: [&str; 18] = [
        "writer", "companion_name", "depth", "past_line", "creature", "creatures", "trader", "trader_faction", "trade", "wares", "book_place", "book_topic",
        "hater", "hated", "liker", "liked", "place", "faction",
    ];

    #[test]
    fn the_grammar_is_sound_once_the_world_is_bound() {
        let mut problems = grammar().problems("#note_lament#");
        problems.retain(|p| !matches!(p, procgen_grammar::GrammarError::MissingSymbol(s) if BOUND.contains(&s.as_str())));
        assert!(problems.is_empty(), "{problems:?}");
        for sym in BOUND {
            // A fact bound over a symbol of the grammar would replace its rules.
            assert!(!grammar().symbols.contains_key(sym), "`{sym}` is both bound and in data/notes.json");
        }
        for kind in NoteKind::ALL {
            assert!(grammar().symbols.contains_key(kind.key()), "no `{}` in data/notes.json", kind.key());
        }
        for wares in WARES {
            assert!(grammar().symbols.contains_key(&format!("wares_{wares}")), "no `wares_{wares}`");
        }
        for stat in lesson::LESSONS {
            let key = format!("topic_{}", lesson::long_name(stat).to_lowercase());
            assert!(grammar().symbols.contains_key(&key), "no `{key}`");
        }
    }

    #[test]
    fn the_same_seed_and_lore_write_the_same_words() {
        for seed in 0..50 {
            assert_eq!(write_note(seed, &rich()), write_note(seed, &rich()));
        }
    }

    #[test]
    fn a_rich_lore_tells_of_every_kind_and_leaves_no_tag() {
        let lore = rich();
        let mut seen = Vec::new();
        for seed in 0..500 {
            let words = write_note(seed, &lore);
            assert!(!words.contains('#'), "seed {seed}: {words}");
            assert!(!words.is_empty(), "seed {seed}");
            let kind = note_kind(seed, &lore);
            if !seen.contains(&kind) {
                seen.push(kind);
            }
        }
        assert_eq!(seen.len(), NoteKind::ALL.len(), "kinds seen: {seen:?}");
    }

    #[test]
    fn a_note_tells_the_truth_of_its_fact() {
        let lore = rich();
        let creature = (0..500).find(|&s| note_kind(s, &lore) == NoteKind::Creature).unwrap();
        let words = write_note(creature, &lore);
        assert!(words.to_lowercase().contains("troll"), "{words}");
        let trader = (0..500).find(|&s| note_kind(s, &lore) == NoteKind::Trader).unwrap();
        let words = write_note(trader, &lore);
        assert!(words.contains("Bera Holt") && words.len() > "Bera Holt".len() + 12, "a sentence, not the bare name: {words}");
        let book = (0..500).find(|&s| note_kind(s, &lore) == NoteKind::Book).unwrap();
        let words = write_note(book, &lore);
        assert!(words.contains("library"), "{words}");
        let hate = (0..500).find(|&s| note_kind(s, &lore) == NoteKind::Hate).unwrap();
        let words = write_note(hate, &lore);
        assert!(words.contains("giant spiders") && words.to_lowercase().contains("the scale court"), "{words}");
    }

    #[test]
    fn an_empty_lore_still_writes_a_note() {
        for seed in 0..100 {
            assert_eq!(note_kind(seed, &Lore::default()), NoteKind::Lament);
            let words = write_note(seed, &Lore::default());
            assert!(!words.is_empty() && !words.contains('#'), "{words}");
        }
    }

    #[test]
    fn a_kind_is_drawn_only_when_the_lore_tells_it() {
        let lore = Lore { depth: 2, creatures: vec![Thing::new("rat", "rats")], ..Lore::default() };
        for seed in 0..200 {
            assert!(matches!(note_kind(seed, &lore), NoteKind::Creature | NoteKind::Lament));
        }
    }

    #[test]
    fn ordinals_read_as_written() {
        let got: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 101].into_iter().map(ordinal).collect();
        assert_eq!(got, ["1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "23rd", "101st"]);
    }
}
