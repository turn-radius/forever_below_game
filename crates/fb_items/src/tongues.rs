//! Writing in a denizen tongue, and the rumours of the people who speak them.
//!
//! From [`DENIZEN_DEPTH`] down, books and scrolls found in the abyss are sometimes written in a
//! denizen's tongue and teach its words ([`Item::teaching`]): the delver reads them to learn
//! phrases ([`fb_denizens::tongue::Lexicon`]). They turn up more often by a denizen faction's
//! ground, and then in that faction's tongue; elsewhere in the tongue of one of the level's
//! denizen factions, or of any people if the level has none. A book teaches either an ability
//! score ([`Item::lesson`]) or words, never both: one written in a tongue teaches words.
//!
//! Above that depth few have met the denizens, but a note now and then carries a rumour of
//! them ([`rumour`]), in the reader's own language, more often the deeper it lies. From that depth
//! down folk know them, and a share of notes speak plainly of the level's denizen factions
//! ([`knowledge`]). Neither teaches any words.
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

/// Chance that a note on the first level is a rumour of the denizens: folk near the top have
/// seldom heard of them.
pub const RUMOUR_ODDS_TOP: f64 = 0.05;
/// Chance for a note on the level just above [`fb_denizens::DENIZEN_DEPTH`], where folk are
/// deep enough to know. Between the two the chance rises evenly with depth ([`rumour_odds`]).
pub const RUMOUR_ODDS_DEEP: f64 = 0.6;
/// Chance that a note at [`fb_denizens::DENIZEN_DEPTH`] or below speaks of a denizen faction on
/// the level, by name and kind, rather than of the usual things. Folk down there know.
pub const KNOWLEDGE_ODDS: f64 = 0.4;
/// Salt for whether a deep note speaks of the level's denizens, and of which faction.
const KNOWLEDGE_SALT: u32 = 0x4B2D_77E1;

/// The chance a note at `depth` (above the denizens) is a rumour: [`RUMOUR_ODDS_TOP`] on level
/// 1 rising evenly to [`RUMOUR_ODDS_DEEP`] on the level above [`fb_denizens::DENIZEN_DEPTH`];
/// none at or below it, where notes speak plainly ([`knowledge`]).
pub fn rumour_odds(depth: u32) -> f64 {
    if fb_denizens::denizens_at(depth) {
        return 0.0;
    }
    let last = fb_denizens::DENIZEN_DEPTH - 1;
    let along = (depth.clamp(1, last) - 1) as f64 / (last - 1) as f64;
    RUMOUR_ODDS_TOP + (RUMOUR_ODDS_DEEP - RUMOUR_ODDS_TOP) * along
}

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
    if !rng.chance(rumour_odds(depth)) {
        return None;
    }
    let kind = *rng.pick(&DenizenKind::ALL);
    Some((kind, tell(kind, &mut rng)))
}

/// What a note at or below [`fb_denizens::DENIZEN_DEPTH`] says of the denizen faction on the
/// level, if it speaks of one ([`KNOWLEDGE_ODDS`]): the faction by name and kind, drawn from the
/// `knows_<kind>` origins of data/rumours.json. A pure function of the seed and the lore, from
/// the note's own stream. `None` above that depth or on a level with no denizen faction.
pub fn knowledge(seed: u32, lore: &Lore) -> Option<String> {
    if !fb_denizens::denizens_at(lore.depth) || lore.denizens.is_empty() {
        return None;
    }
    let mut rng = Mulberry32::new(seed ^ KNOWLEDGE_SALT);
    if !rng.chance(KNOWLEDGE_ODDS) {
        return None;
    }
    let (name, kind) = rng.pick(&lore.denizens).clone();
    let key = format!("knows_{}", kind.key());
    let bound = Grammar { symbols: [("faction".to_string(), vec![name])].into_iter().collect() };
    let words = procgen_grammar::expand_with(&GRAMMAR.merged(&bound), &format!("#{key}#"), &mut rng).unwrap_or_else(|e| panic!("data/rumours.json, {key}: {e}"));
    Some(words.split_whitespace().collect::<Vec<_>>().join(" "))
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
        assert!(told.len() > 500 && told.len() < 1_000, "{}", told.len());
        for kind in DenizenKind::ALL {
            assert!(told.iter().any(|(k, _)| *k == kind), "{kind:?}");
        }
        for (_, words) in &told {
            assert!(!words.contains('#') && words.ends_with(['.', '!']), "{words}");
        }
        assert!((0..4_000).all(|s| rumour(s, fb_denizens::DENIZEN_DEPTH).is_none()), "the denizens are known by then");
        let share = |depth| (0..4_000).filter(|&s| rumour(s, depth).is_some()).count() as f64 / 4_000.0;
        assert!((share(1) - RUMOUR_ODDS_TOP).abs() < 0.02 && (share(9) - RUMOUR_ODDS_DEEP).abs() < 0.03, "{} {}", share(1), share(9));
        assert!(share(1) < share(4) && share(4) < share(7) && share(7) < share(9), "rising with depth");
        assert_eq!(rumour(77, 3), rumour(77, 3), "the same seed, the same rumour");
    }

    #[test]
    fn deep_notes_speak_of_the_levels_denizens_by_name_and_teach_nothing() {
        let lore = Lore { depth: fb_denizens::DENIZEN_DEPTH, denizens: vec![("the Red Maze".into(), DenizenKind::Minotaur)], tongues: vec![DenizenKind::Minotaur], ..Lore::default() };
        let told: Vec<_> = (0..2_000).filter_map(|s| knowledge(s, &lore).map(|w| (s, w))).collect();
        assert!(told.len() > 650 && told.len() < 950, "{}", told.len());
        assert!(told.iter().all(|(_, w)| !w.contains('#') && w.to_lowercase().contains("the red maze")), "names the faction");
        let (seed, words) = &told[0];
        let note = roll_item("Note", *seed).unwrap().in_world(&lore);
        assert_eq!(note.words.as_deref(), Some(words.as_str()));
        assert_eq!((note.teaching(), note.lesson()), (None, None));
        assert!(knowledge(*seed, &Lore { denizens: vec![], ..lore.clone() }).is_none(), "no denizen faction, nothing to tell");
        assert!(knowledge(*seed, &Lore { depth: 4, ..lore }).is_none(), "above the denizens it is a rumour instead");
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
            // The faction's name is bound where a note is written.
            let bound = Grammar { symbols: [("faction".to_string(), vec!["the Red Maze".to_string()])].into_iter().collect() };
            for start in [rumour_key(kind), format!("knows_{}", kind.key())] {
                let problems = GRAMMAR.merged(&bound).problems(&format!("#{start}#"));
                assert!(problems.is_empty(), "{kind:?}: {problems:?}");
            }
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
