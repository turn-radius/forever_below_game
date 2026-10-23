//! Writing in a denizen tongue, and the rumours of the people who speak them.
//!
//! From [`DENIZEN_DEPTH`] down, books and scrolls found in the abyss are sometimes written in a
//! denizen's tongue and teach its words ([`Item::teaching`]): the delver reads them to learn
//! phrases ([`fb_denizens::tongue::Lexicon`]). They turn up more often by a denizen faction's
//! ground, and then in that faction's tongue; elsewhere in the tongue of one of the level's
//! denizen factions, or of any people if the level has none. A book teaches either an ability
//! score ([`Item::lesson`]) or words, never both: one written in a tongue teaches words.
//!
//! Above that depth no one has met the denizens, but a note now and then carries a rumour of
//! them ([`rumour`]), in the reader's own language. It teaches nothing.
//!
//! All our own rules, for a designer to tune.

use armory::families::writing::WritingKind;
use fb_denizens::DenizenKind;
use fb_denizens::tongue::{Teaching, TeachingSize};
use procgen_grammar::Grammar;
use procgen_rng::Mulberry32;
use std::sync::LazyLock;

use crate::Item;
use crate::notes::Lore;

/// Salt for whether a book or scroll teaches words, and in which tongue.
const TEACHING_SALT: u32 = 0x7E4C_D1A5;

/// Salt for whether a note carries a rumour, and which people it tells of.
const RUMOUR_SALT: u32 = 0x2B07_9E31;

/// Chance that a scroll found by a denizen faction's ground is written in its tongue.
pub const SCROLL_NEAR: f64 = 0.5;
/// Chance that a scroll found anywhere else at depth is written in a denizen tongue.
pub const SCROLL_FAR: f64 = 0.06;
/// Chance for a book by a denizen faction's ground. Books are rarer, so more of them teach words.
pub const BOOK_NEAR: f64 = 0.7;
/// Chance for a book anywhere else at depth.
pub const BOOK_FAR: f64 = 0.2;

/// Chance that a note above [`fb_denizens::DENIZEN_DEPTH`] is a rumour of the denizens. Notes
/// are scarce on the floor (a couple over nine levels, with the ones in crates), so this gives a delver one or
/// two over the first nine levels.
pub const RUMOUR_ODDS: f64 = 0.35;

static GRAMMAR: LazyLock<Grammar> =
    LazyLock::new(|| Grammar::from_json(include_str!("../../../data/rumours.json")).unwrap_or_else(|e| panic!("data/rumours.json: {e}")));

/// The rumours grammar as shipped.
pub fn grammar() -> &'static Grammar {
    &GRAMMAR
}

/// The `rumour_<kind>` symbol of data/rumours.json for `kind`.
pub fn rumour_key(kind: DenizenKind) -> String {
    format!("rumour_{}", kind.key())
}

/// The rumour of the note of `seed`, or `None` if that note carries none (most do not): a pure
/// function of the seed and the depth, from the note's own stream. Only above the depth the
/// denizens live at. Returns the people it tells of with its words.
pub fn rumour(seed: u32, depth: u32) -> Option<(DenizenKind, String)> {
    if fb_denizens::denizens_at(depth) {
        return None;
    }
    let mut rng = Mulberry32::new(seed ^ RUMOUR_SALT);
    if !rng.chance(RUMOUR_ODDS) {
        return None;
    }
    let kind = *rng.pick(&DenizenKind::ALL);
    Some((kind, tell(kind, &mut rng)))
}

/// A rumour of `kind`'s people, drawn from `rng`.
pub fn tell(kind: DenizenKind, rng: &mut Mulberry32) -> String {
    let key = rumour_key(kind);
    let words = procgen_grammar::expand_with(&GRAMMAR, &format!("#{key}#"), rng).unwrap_or_else(|e| panic!("data/rumours.json, {key}: {e}"));
    words.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Item {
    /// The words this teaches the reader, if it is a book or scroll written in a denizen tongue.
    /// Recorded on the item when it is found ([`Item::in_world_near`]), so an item that changes
    /// later keeps it.
    pub fn teaching(&self) -> Option<&Teaching> {
        self.teaching.as_ref()
    }

    /// The tongue it is written in, if it teaches words.
    pub fn tongue(&self) -> Option<DenizenKind> {
        self.teaching.as_ref().map(|t| t.kind)
    }

    /// This item found on a level that knows `lore` of itself, with a denizen faction's ground
    /// at hand if `near` names that faction's kind: a book or scroll at [`fb_denizens::DENIZEN_DEPTH`]
    /// or below may be written in a tongue ([`SCROLL_NEAR`], [`BOOK_NEAR`] by its ground and
    /// [`SCROLL_FAR`], [`BOOK_FAR`] elsewhere). Otherwise as [`Item::in_world`]. Pure in the
    /// item's seed, the lore and `near`; one already written in a tongue is left as it is.
    pub fn in_world_near(mut self, lore: &Lore, near: Option<DenizenKind>) -> Item {
        if self.teaching.is_none() && fb_denizens::denizens_at(lore.depth) {
            let (size, here, there) = match &self.shape {
                armory::Item::Writing(s) if s.piece.kind == WritingKind::Book => (TeachingSize::Book, BOOK_NEAR, BOOK_FAR),
                armory::Item::Writing(s) if s.piece.kind == WritingKind::Scroll => (TeachingSize::Scroll, SCROLL_NEAR, SCROLL_FAR),
                _ => (TeachingSize::Scroll, 0.0, 0.0),
            };
            let mut rng = Mulberry32::new(self.seed ^ TEACHING_SALT);
            if rng.chance(if near.is_some() { here } else { there }) {
                let pool: &[DenizenKind] = if lore.tongues.is_empty() { &DenizenKind::ALL } else { &lore.tongues };
                let kind = near.unwrap_or_else(|| *rng.pick(pool));
                self.teaching = Some(Teaching::roll(self.seed, kind, size));
            }
        }
        self.in_world(lore)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roll_item;

    fn deep(tongues: Vec<DenizenKind>) -> Lore {
        Lore { depth: fb_denizens::DENIZEN_DEPTH, tongues, ..Lore::default() }
    }

    #[test]
    fn rumours_are_rare_readable_and_only_above_the_denizens() {
        let told: Vec<_> = (0..4_000).filter_map(|s| rumour(s, 3)).collect();
        assert!(told.len() > 1_000 && told.len() < 1_800, "{}", told.len());
        for kind in DenizenKind::ALL {
            assert!(told.iter().any(|(k, _)| *k == kind), "{kind:?}");
        }
        for (_, words) in &told {
            assert!(!words.contains('#') && words.ends_with(['.', '!']), "{words}");
        }
        assert!((0..4_000).all(|s| rumour(s, fb_denizens::DENIZEN_DEPTH).is_none()), "the denizens are known by then");
        assert_eq!(rumour(77, 3), rumour(77, 3), "the same seed, the same rumour");
    }

    #[test]
    fn a_rumour_note_carries_its_words_and_teaches_nothing() {
        let lore = Lore { depth: 4, ..Lore::default() };
        let seed = (0..4_000).find(|&s| rumour(s, 4).is_some()).unwrap();
        let note = roll_item("Note", seed).unwrap().in_world(&lore);
        assert_eq!(note.words.as_deref(), rumour(seed, 4).map(|r| r.1).as_deref());
        assert_eq!((note.teaching(), note.lesson()), (None, None));
    }

    #[test]
    fn grammar_has_no_loose_ends() {
        for kind in DenizenKind::ALL {
            let problems = GRAMMAR.problems(&format!("#{}#", rumour_key(kind)));
            assert!(problems.is_empty(), "{kind:?}: {problems:?}");
            for s in 0..200 {
                assert!(!tell(kind, &mut Mulberry32::new(s)).contains('#'));
            }
        }
    }

    #[test]
    fn teaching_is_found_only_at_depth_and_more_by_a_denizen_faction() {
        let shallow = Lore { depth: fb_denizens::DENIZEN_DEPTH - 1, ..Lore::default() };
        for base in ["Book", "Scroll"] {
            let count = |lore: &Lore, near: Option<DenizenKind>| (0..400).filter(|&s| roll_item(base, s).unwrap().in_world_near(lore, near).teaching().is_some()).count();
            assert_eq!(count(&shallow, Some(DenizenKind::Faun)), 0, "{base}");
            let lore = deep(vec![DenizenKind::Faun]);
            let (near, far) = (count(&lore, Some(DenizenKind::Faun)), count(&lore, None));
            assert!(near > far * 2 && far > 0, "{base}: near {near}, far {far}");
        }
    }

    #[test]
    fn the_tongue_follows_the_ground_or_the_level() {
        let lore = deep(vec![DenizenKind::Cyclops]);
        for s in 0..300 {
            let book = roll_item("Book", s).unwrap();
            if let Some(kind) = book.clone().in_world_near(&lore, Some(DenizenKind::Faun)).tongue() {
                assert_eq!(kind, DenizenKind::Faun);
            }
            if let Some(kind) = book.in_world_near(&lore, None).tongue() {
                assert_eq!(kind, DenizenKind::Cyclops, "elsewhere, the level's own people");
            }
        }
    }

    #[test]
    fn a_book_in_a_tongue_teaches_words_not_a_score_and_keeps_them() {
        let lore = deep(vec![DenizenKind::Minotaur]);
        let book = (0..200).map(|s| roll_item("Book", s).unwrap().in_world_near(&lore, Some(DenizenKind::Minotaur))).find(|b| b.teaching().is_some()).unwrap();
        assert_eq!(book.lesson(), None);
        let teaching = book.teaching().unwrap().clone();
        assert_eq!(book.clone().in_world_near(&deep(vec![DenizenKind::Demon]), None).teaching(), Some(&teaching), "kept wherever it goes");
        let again = crate::Item::from_parts(crate::ItemParts::from(book.clone())).unwrap();
        assert_eq!(again, book);
    }

    #[test]
    fn notes_and_other_things_never_teach() {
        let lore = deep(vec![DenizenKind::Demon]);
        for base in ["Note", "Ring", "Bread"] {
            assert!((0..100).all(|s| roll_item(base, s).unwrap().in_world_near(&lore, Some(DenizenKind::Demon)).teaching().is_none()), "{base}");
        }
    }
}
