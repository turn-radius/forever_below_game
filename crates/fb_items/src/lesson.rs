//! What a book teaches. Every book teaches one of the five ability scores, fixed by its seed
//! ([`Item::lesson`]): reading it raises the reader's score by [`LESSON_POINTS`] for good, once
//! per reader (the game keeps who has read what). A book found in a denizen tongue teaches its
//! words instead, and a scroll can too ([`crate::tongues`]); notes teach nothing. Our own rule,
//! the user's call; books are rare to match ([`crate::armory_base::find_weight`]).

use armory::families::writing::WritingKind;
use procgen_rng::Mulberry32;

use crate::Item;
use crate::stats::Stat;

/// The scores a book can teach, one each.
pub const LESSONS: [Stat; 5] = [Stat::Strength, Stat::Dexterity, Stat::Constitution, Stat::Intelligence, Stat::Wisdom];

/// Points a book adds to the score it teaches.
pub const LESSON_POINTS: i32 = 1;

/// The short name of a score a book teaches, as the sheet writes it ("WIS").
pub fn short_name(stat: Stat) -> &'static str {
    match stat {
        Stat::Strength => "STR",
        Stat::Dexterity => "DEX",
        Stat::Constitution => "CON",
        Stat::Intelligence => "INT",
        Stat::Wisdom => "WIS",
        _ => "",
    }
}

/// The full name of a score a book teaches ("Wisdom").
pub fn long_name(stat: Stat) -> &'static str {
    match stat {
        Stat::Strength => "Strength",
        Stat::Dexterity => "Dexterity",
        Stat::Constitution => "Constitution",
        Stat::Intelligence => "Intelligence",
        Stat::Wisdom => "Wisdom",
        _ => "",
    }
}

impl Item {
    /// The ability score reading this teaches, or `None` for anything that is not a book. Its
    /// own stream of the seed (the mixing constant is our own), so the shape is unaffected.
    pub fn lesson(&self) -> Option<Stat> {
        // A book written in a denizen tongue teaches its words instead ([`Item::teaching`]).
        if self.teaching.is_some() {
            return None;
        }
        let armory::Item::Writing(state) = &self.shape else { return None };
        (state.piece.kind == WritingKind::Book).then(|| *Mulberry32::new(self.seed ^ 0x7EAC_4B00).pick(&LESSONS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roll_item;

    #[test]
    fn a_book_teaches_one_score_from_its_seed_and_writing_else_teaches_nothing() {
        let book = roll_item("Book", 12).unwrap();
        assert!(book.lesson().is_some());
        assert_eq!(book.lesson(), roll_item("Book", 12).unwrap().lesson(), "the same seed, the same lesson");
        for base in ["Scroll", "Note", "Ring", "Bread"] {
            assert_eq!(roll_item(base, 12).unwrap().lesson(), None, "{base}");
        }
    }

    #[test]
    fn books_teach_every_score() {
        let taught: std::collections::HashSet<_> = (0..200).filter_map(|s| roll_item("Book", s).unwrap().lesson()).collect();
        assert_eq!(taught.len(), LESSONS.len());
    }
}
