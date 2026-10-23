//! The words every tongue has, by topic. The key and the English are shared; the native form
//! differs by tongue ([`crate::tongue::native`]).

use serde::{Deserialize, Serialize};

use crate::DenizenKind;

/// What a line of talk is about, as far as the words it needs go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    Greeting,
    Farewell,
    /// Who they are: the speaker, the listener, the kin and the faction.
    Kin,
    /// Barter.
    Trade,
    /// Needs and requests.
    Quest,
    /// The level and the way out.
    Place,
    /// Warnings and menace.
    Threat,
}

impl Topic {
    pub const ALL: [Topic; 7] = [Topic::Greeting, Topic::Farewell, Topic::Kin, Topic::Trade, Topic::Quest, Topic::Place, Topic::Threat];

    /// The key used in data ("greeting").
    pub fn key(self) -> &'static str {
        match self {
            Topic::Greeting => "greeting",
            Topic::Farewell => "farewell",
            Topic::Kin => "kin",
            Topic::Trade => "trade",
            Topic::Quest => "quest",
            Topic::Place => "place",
            Topic::Threat => "threat",
        }
    }

    /// What a book about it is said to teach ("greetings").
    pub fn label(self) -> &'static str {
        match self {
            Topic::Greeting => "greetings",
            Topic::Farewell => "farewells",
            Topic::Kin => "kinship",
            Topic::Trade => "trade",
            Topic::Quest => "requests",
            Topic::Place => "places",
            Topic::Threat => "warnings",
        }
    }

    /// Every word of the topic, in table order.
    pub fn words(self) -> impl Iterator<Item = &'static Word> {
        VOCABULARY.iter().filter(move |w| w.topic == self)
    }
}

/// One word every tongue has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Word {
    /// Stable across tongues and saves ("hail").
    pub key: &'static str,
    /// What it means ("hail").
    pub english: &'static str,
    pub topic: Topic,
}

impl Word {
    /// How it is written in `kind`'s tongue.
    pub fn native(&self, kind: DenizenKind) -> String {
        super::native(kind, self.key)
    }
}

const fn w(topic: Topic, key: &'static str, english: &'static str) -> Word {
    Word { key, english, topic }
}

/// Words of a topic that a line about it needs: the first of its words in the table. The rest
/// are taught with them but are not needed to read a plain line.
pub const CORE_WORDS: usize = 3;

/// Every word, topic by topic, the core words of each topic first.
pub const VOCABULARY: &[Word] = &[
    w(Topic::Greeting, "hail", "hail"),
    w(Topic::Greeting, "met", "well met"),
    w(Topic::Greeting, "stranger", "stranger"),
    w(Topic::Greeting, "peace", "peace"),
    w(Topic::Greeting, "welcome", "welcome"),
    w(Topic::Greeting, "friend", "friend"),
    w(Topic::Farewell, "farewell", "farewell"),
    w(Topic::Farewell, "go", "go"),
    w(Topic::Farewell, "return", "return"),
    w(Topic::Farewell, "safe", "safe"),
    w(Topic::Farewell, "road", "road"),
    w(Topic::Farewell, "again", "again"),
    w(Topic::Kin, "i", "I"),
    w(Topic::Kin, "you", "you"),
    w(Topic::Kin, "kin", "kin"),
    w(Topic::Kin, "name", "name"),
    w(Topic::Kin, "banner", "banner"),
    w(Topic::Kin, "herd", "herd"),
    w(Topic::Trade, "trade", "trade"),
    w(Topic::Trade, "buy", "buy"),
    w(Topic::Trade, "sell", "sell"),
    w(Topic::Trade, "coin", "coin"),
    w(Topic::Trade, "price", "price"),
    w(Topic::Trade, "goods", "goods"),
    w(Topic::Quest, "need", "need"),
    w(Topic::Quest, "bring", "bring"),
    w(Topic::Quest, "help", "help"),
    w(Topic::Quest, "find", "find"),
    w(Topic::Quest, "reward", "reward"),
    w(Topic::Quest, "oath", "oath"),
    w(Topic::Place, "here", "here"),
    w(Topic::Place, "deep", "deep"),
    w(Topic::Place, "way", "way"),
    w(Topic::Place, "out", "out"),
    w(Topic::Place, "door", "door"),
    w(Topic::Place, "dark", "dark"),
    w(Topic::Threat, "beware", "beware"),
    w(Topic::Threat, "enemy", "enemy"),
    w(Topic::Threat, "blood", "blood"),
    w(Topic::Threat, "flee", "flee"),
    w(Topic::Threat, "death", "death"),
    w(Topic::Threat, "blade", "blade"),
];

/// The word with this key.
pub fn word(key: &str) -> Option<Word> {
    VOCABULARY.iter().find(|w| w.key == key).copied()
}

/// The words a plain line about `topic` needs in `kind`'s tongue: the topic's core words. The
/// vocabulary is the same for every tongue; the kind is here so a line can differ later.
pub fn needs(_kind: DenizenKind, topic: Topic) -> Vec<Word> {
    topic.words().take(CORE_WORDS).copied().collect()
}

/// [`needs`], plus the words with these `extra` keys (a line that names a thing, say). Keys
/// that are not words are ignored.
pub fn needs_with(kind: DenizenKind, topic: Topic, extra: &[&str]) -> Vec<Word> {
    let mut out = needs(kind, topic);
    for key in extra {
        if let Some(found) = word(key)
            && !out.contains(&found)
        {
            out.push(found);
        }
    }
    out
}
