//! What one book or scroll teaches: a fixed set of words of one tongue, rolled from the item's
//! seed.

use procgen_rng::Mulberry32;

use super::vocabulary::{Topic, Word};
use crate::DenizenKind;

/// How much a piece of writing teaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeachingSize {
    /// One or two topics' phrases.
    Scroll,
    /// Three to five topics' phrases.
    Book,
}

impl TeachingSize {
    fn topics(self) -> (u32, u32) {
        match self {
            TeachingSize::Scroll => (1, 2),
            TeachingSize::Book => (3, 5),
        }
    }
}

/// The words a book or scroll teaches, all of one tongue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Teaching {
    pub kind: DenizenKind,
    /// The topics it covers, in table order.
    pub topics: Vec<Topic>,
    /// Every word of those topics.
    pub words: Vec<Word>,
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

    /// The short line for the item card ("Minotaur phrases: greetings and trade").
    pub fn description(&self) -> String {
        let labels: Vec<&str> = self.topics.iter().map(|t| t.label()).collect();
        let list = match labels.as_slice() {
            [] => String::new(),
            [only] => (*only).to_string(),
            [init @ .., last] => format!("{} and {last}", init.join(", ")),
        };
        format!("{} phrases: {list}", self.kind.label())
    }
}
