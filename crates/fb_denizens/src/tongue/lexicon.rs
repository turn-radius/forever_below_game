//! What the delver has learned of each tongue.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::teaching::Teaching;
use super::vocabulary::{Topic, VOCABULARY, Word, needs, word};
use crate::DenizenKind;

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

    /// Learns what a book or scroll teaches, and returns the words that were new. A topic it
    /// covers that is already fully known is swapped for the next topic not yet known (table
    /// order), so a writing never repeats what the delver has: any three scrolls of a tongue, in
    /// any order, teach all of it. Once the tongue is complete it teaches nothing.
    pub fn learn_teaching(&mut self, teaching: &Teaching) -> Vec<Word> {
        let kind = teaching.kind;
        let mut chosen: Vec<Topic> = Vec::new();
        let mut spare = 0;
        for t in Topic::ALL {
            if teaching.topics.contains(&t) {
                if self.topic_known(kind, t) {
                    spare += 1;
                } else {
                    chosen.push(t);
                }
            }
        }
        for t in Topic::ALL {
            if spare == 0 {
                break;
            }
            if !teaching.topics.contains(&t) && !self.topic_known(kind, t) {
                chosen.push(t);
                spare -= 1;
            }
        }
        let words: Vec<Word> = chosen.iter().flat_map(|t| t.words()).copied().collect();
        self.learn(kind, &words)
    }

    /// Whether every word of `topic` is learned in `kind`'s tongue.
    pub fn topic_known(&self, kind: DenizenKind, topic: Topic) -> bool {
        topic.words().all(|w| self.knows(kind, w))
    }

    /// Whether the whole tongue is learned, so no writing has anything left to teach.
    pub fn is_complete(&self, kind: DenizenKind) -> bool {
        Topic::ALL.iter().all(|t| self.topic_known(kind, *t))
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

    /// The share (0 to 1) of `kind`'s vocabulary learned, for display.
    pub fn known_share(&self, kind: DenizenKind) -> f32 {
        let known = self.known.get(&kind).map_or(0, |k| k.iter().filter(|key| word(key).is_some()).count());
        known as f32 / VOCABULARY.len() as f32
    }

    /// Whether the delver knows enough of the tongue to barter: the greeting and trade phrases.
    pub fn can_barter(&self, kind: DenizenKind) -> bool {
        self.topic_readable(kind, Topic::Greeting) && self.topic_readable(kind, Topic::Trade)
    }

    /// Whether the delver knows enough of the tongue to take quests: the greeting and quest
    /// phrases.
    pub fn can_take_quests(&self, kind: DenizenKind) -> bool {
        self.topic_readable(kind, Topic::Greeting) && self.topic_readable(kind, Topic::Quest)
    }
}
