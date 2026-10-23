//! What the delver has learned of each tongue.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::teaching::Teaching;
use super::vocabulary::{Topic, VOCABULARY, Word, needs, word};
use crate::DenizenKind;

/// The share of a tongue (0 to 1) known well enough to barter without the trade phrases. Our
/// own rule, for a designer to tune.
pub const BARTER_SHARE: f32 = 0.4;

/// The share of a tongue known well enough to take quests without the quest phrases. Our own
/// rule, for a designer to tune.
pub const QUEST_SHARE: f32 = 0.6;

/// The words the delver has learned, per tongue. Saved with the delve record, so words stay
/// learned for the rest of the delve; the next delver starts with none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lexicon {
    known: BTreeMap<DenizenKind, BTreeSet<String>>,
}

impl Lexicon {
    /// A delver who knows no tongue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Learns `words` of `kind`'s tongue, and returns the ones that were new.
    pub fn learn(&mut self, kind: DenizenKind, words: &[Word]) -> Vec<Word> {
        let known = self.known.entry(kind).or_default();
        words.iter().filter(|w| known.insert(w.key.to_string())).copied().collect()
    }

    /// Learns what a book or scroll teaches, and returns the words that were new.
    pub fn learn_teaching(&mut self, teaching: &Teaching) -> Vec<Word> {
        self.learn(teaching.kind, &teaching.words)
    }

    /// Whether the delver knows this word of `kind`'s tongue.
    pub fn knows(&self, kind: DenizenKind, word: &Word) -> bool {
        self.known.get(&kind).is_some_and(|k| k.contains(word.key))
    }

    /// The words learned of `kind`'s tongue, in table order.
    pub fn known_words(&self, kind: DenizenKind) -> Vec<Word> {
        VOCABULARY.iter().filter(|w| self.knows(kind, w)).copied().collect()
    }

    /// Whether a line needing these words can be read: all of them are learned. A line that
    /// needs none is always readable.
    pub fn readable(&self, kind: DenizenKind, needs: &[Word]) -> bool {
        needs.iter().all(|w| self.knows(kind, w))
    }

    /// Whether a plain line about `topic` can be read.
    pub fn topic_readable(&self, kind: DenizenKind, topic: Topic) -> bool {
        self.readable(kind, &needs(kind, topic))
    }

    /// The share (0 to 1) of `kind`'s vocabulary learned.
    pub fn known_share(&self, kind: DenizenKind) -> f32 {
        let known = self.known.get(&kind).map_or(0, |k| k.iter().filter(|key| word(key).is_some()).count());
        known as f32 / VOCABULARY.len() as f32
    }

    /// Whether the delver knows enough of the tongue to barter: the greeting and trade phrases,
    /// or [`BARTER_SHARE`] of the tongue.
    pub fn can_barter(&self, kind: DenizenKind) -> bool {
        (self.topic_readable(kind, Topic::Greeting) && self.topic_readable(kind, Topic::Trade)) || self.known_share(kind) >= BARTER_SHARE
    }

    /// Whether the delver knows enough of the tongue to take quests: the greeting and quest
    /// phrases, or [`QUEST_SHARE`] of the tongue.
    pub fn can_take_quests(&self, kind: DenizenKind) -> bool {
        (self.topic_readable(kind, Topic::Greeting) && self.topic_readable(kind, Topic::Quest)) || self.known_share(kind) >= QUEST_SHARE
    }
}
