//! What one book or scroll teaches: a fixed set of words of one tongue, rolled from the item's
//! seed. A scroll covers three or four of the seven topics and a book five to seven; reading
//! swaps topics already known for ones not yet known (see `Lexicon::learn_teaching`), so three
//! writings teach a whole tongue.

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use super::vocabulary::{Topic, Word, word};
use crate::DenizenKind;

/// How much a piece of writing teaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeachingSize {
    /// Three or four topics' phrases.
    Scroll,
    /// Five to seven topics' phrases.
    Book,
}

impl TeachingSize {
    fn topics(self) -> (u32, u32) {
        match self {
            TeachingSize::Scroll => (3, 4),
            TeachingSize::Book => (5, 7),
        }
    }
}

/// The words a book or scroll teaches, all of one tongue. Saved as the kind, the topics and the
/// words' keys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "TeachingSaved", into = "TeachingSaved")]
pub struct Teaching {
    pub kind: DenizenKind,
    /// The topics it covers, in table order.
    pub topics: Vec<Topic>,
    /// Every word of those topics.
    pub words: Vec<Word>,
}

/// A [`Teaching`] as saved: words by key, so a word the vocabulary no longer has is dropped.
#[derive(Serialize, Deserialize)]
struct TeachingSaved {
    kind: DenizenKind,
    topics: Vec<Topic>,
    keys: Vec<String>,
}

impl From<Teaching> for TeachingSaved {
    fn from(t: Teaching) -> Self {
        TeachingSaved { kind: t.kind, topics: t.topics, keys: t.words.iter().map(|w| w.key.to_string()).collect() }
    }
}

impl From<TeachingSaved> for Teaching {
    fn from(s: TeachingSaved) -> Self {
        Teaching { kind: s.kind, topics: s.topics, words: s.keys.iter().filter_map(|k| word(k)).collect() }
    }
}

impl Teaching {
    /// What an item with this `seed` teaches of `kind`'s tongue: `size` decides how many topics.
    pub fn roll(seed: u32, kind: DenizenKind, size: TeachingSize) -> Self {
        let mut rng = Mulberry32::new(seed ^ (kind as u32 + 1).wrapping_mul(0x85eb_ca6b));
        let (lo, hi) = size.topics();
        let count = rng.int(lo, hi) as usize;
        let mut pool = Topic::ALL.to_vec();
        let mut picked = Vec::new();
        while picked.len() < count && !pool.is_empty() {
            let i = rng.int(0, pool.len() as u32 - 1) as usize;
            picked.push(pool.remove(i));
        }
        Self::of_topics(kind, &picked)
    }

    /// A teaching of every word of these topics, such as a scroll of greetings and trade.
    pub fn of_topics(kind: DenizenKind, topics: &[Topic]) -> Self {
        let topics: Vec<Topic> = Topic::ALL.iter().filter(|t| topics.contains(t)).copied().collect();
        let words = topics.iter().flat_map(|t| t.words()).copied().collect();
        Teaching { kind, topics, words }
    }

    /// The topics in a phrase ("greetings and trade", "greetings, trade and quests").
    pub fn topic_list(&self) -> String {
        let labels: Vec<&str> = self.topics.iter().map(|t| t.label()).collect();
        match labels.as_slice() {
            [] => String::new(),
            [only] => (*only).to_string(),
            [init @ .., last] => format!("{} and {last}", init.join(", ")),
        }
    }

    /// The short line for the item card ("Minotaur phrases: greetings and trade").
    pub fn description(&self) -> String {
        format!("{} phrases: {}", self.kind.label(), self.topic_list())
    }
}
