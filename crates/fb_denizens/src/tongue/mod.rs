//! The tongues of the denizens. Each [`DenizenKind`] speaks its own: the delver is not born
//! knowing any of them, and learns one word at a time from books and scrolls found in the abyss
//! ([`Teaching`]). What the delver has learned is a [`Lexicon`], saved with the delve so words
//! stay learned for its whole length.
//!
//! - [`VOCABULARY`]: the words every tongue has, grouped by [`Topic`]. A word has a stable key,
//!   its English meaning and, in each tongue, a fixed native form ([`native`]) grown from that
//!   tongue's phonology, so minotaur words rumble and demon words hiss. Native spellings depend
//!   on the kind alone, never on a world's seed.
//! - A line of talk names the words it needs ([`needs`], [`needs_with`]); the delver can read it
//!   once all of them are learned ([`Lexicon::readable`]). Until then the line is shown as
//!   [`glyphs`], the tongue's own script, with the line's rhythm kept.
//! - [`Lexicon::can_barter`] and [`Lexicon::can_take_quests`] say when enough of a tongue is
//!   known to trade and to take work: the greeting phrases plus the trade or quest phrases.
//!
//! The vocabulary and phonologies are Rust constants, not a JSON table: they are small, the
//! phonology is code-shaped (rolled syllables), and tests can name them. Our own rules, for a
//! designer to tune.

mod lexicon;
mod phonology;
mod script;
mod teaching;
mod vocabulary;

pub use lexicon::Lexicon;
pub use phonology::native;
pub use script::{glyphs, script};
pub use teaching::{Teaching, TeachingSize};
pub use vocabulary::{CORE_WORDS, Topic, VOCABULARY, Word, UnknownWord, needs, needs_with, word};

use crate::DenizenKind;

/// The name of a kind's tongue, as the interface shows it ("Minotaur speech").
pub fn tongue_name(kind: DenizenKind) -> &'static str {
    match kind {
        DenizenKind::Minotaur => "Minotaur speech",
        DenizenKind::Cyclops => "Cyclopean",
        DenizenKind::Faun => "Sylvan",
        DenizenKind::Demon => "Infernal",
    }
}

#[cfg(test)]
mod tests;
