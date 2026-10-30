//! Prayer scrolls: writing that teaches a cleric one Divine term ([`Item::prayer`]).
//!
//! A scroll found in the abyss now and then names one term of the Divine school that a cleric
//! does not start with, rolled from the scroll's seed among [`prayer_terms`]. A cleric who reads
//! it can write with that term in the spellbook (`fb_character::casting::Spellbook::unlock`); any
//! other reader learns nothing. The term is recorded on the item when it is found
//! ([`Item::in_world_at`]) and saved with it, so an item that changes later keeps it.
//! Reading does not use the scroll up, as with the scrolls that teach a denizen tongue.
//!
//! The terms come from `[casting.terms]` of `data/rules.toml` as shipped: every Divine term not
//! known from the start, except the ones a devotion grants. This crate cannot see
//! `fb_character`, so it reads the same file itself; a term added there joins the pool.
//!
//! All our own rules, for a designer to tune.

use procgen_rng::Mulberry32;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::LazyLock;

use crate::Item;

/// Salt for whether a scroll is a prayer scroll, and which term it teaches.
const PRAYER_SALT: u32 = 0x5A91_C3D7;

/// Chance that a scroll found anywhere else is a prayer scroll.
pub const PRAYER_FAR: f64 = 0.06;
/// Chance for a scroll found by a shrine or in a faction's hall.
pub const PRAYER_NEAR: f64 = 0.4;

/// The terms a devotion grants, which a scroll never teaches
/// (`fb_character::casting::Spellbook::DEVOTION_TERMS`).
const DEVOTION_TERMS: [&str; 3] = ["bane", "frighten", "charm"];

/// One term a prayer scroll can teach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrayerTerm {
    /// Its key in `[casting.terms]`, the id the spellbook unlocks.
    pub id: String,
    /// What the book shows ("Heal").
    pub name: String,
}

#[derive(Deserialize)]
struct Rules {
    casting: Casting,
}
#[derive(Deserialize)]
struct Casting {
    terms: BTreeMap<String, Term>,
}
#[derive(Deserialize)]
struct Term {
    name: String,
    school: String,
    #[serde(default)]
    known: bool,
}

static TERMS: LazyLock<Vec<PrayerTerm>> = LazyLock::new(|| {
    let rules: Rules = toml::from_str(include_str!("../../../data/rules.toml")).unwrap_or_else(|e| panic!("data/rules.toml: {e}"));
    rules
        .casting
        .terms
        .into_iter()
        .filter(|(id, t)| t.school == "Divine" && !t.known && !DEVOTION_TERMS.contains(&id.as_str()))
        .map(|(id, t)| PrayerTerm { id, name: t.name })
        .collect()
});

/// The terms a prayer scroll can teach, in key order.
pub fn prayer_terms() -> &'static [PrayerTerm] {
    &TERMS
}

/// What the scroll for `term` is a prayer of: "Healing" for Heal, "the Cone" for a term with no
/// word of its own.
pub fn prayer_noun(term: &PrayerTerm) -> String {
    match term.id.as_str() {
        "heal" => "Healing".to_string(),
        "cure" => "Curing".to_string(),
        _ => format!("the {}", term.name),
    }
}

/// The term of id `id` a scroll may teach, if it is one.
pub fn prayer_term(id: &str) -> Option<&'static PrayerTerm> {
    prayer_terms().iter().find(|t| t.id == id)
}

/// The scroll of `seed` teaching `term` (a key of [`prayer_terms`]), named for it: "Scroll of
/// Healing". For a healer's stock. `None` for a term no scroll teaches.
pub fn prayer_scroll(seed: u32, term: &str) -> Option<Item> {
    let term = prayer_term(term)?;
    let mut scroll = crate::roll_item("Scroll", seed)?;
    scroll.prayer = Some(term.id.clone());
    scroll.name = format!("Scroll of {}", prayer_noun(term));
    Some(scroll)
}

impl Item {
    /// The Divine term this scroll teaches a cleric, if it is a prayer scroll.
    pub fn prayer(&self) -> Option<&str> {
        self.prayer.as_deref()
    }

    /// The card's lines for a prayer scroll, "Prayer of Healing" and "Teaches a cleric the Heal
    /// term".
    pub fn prayer_line(&self) -> Option<String> {
        let term = prayer_term(self.prayer()?)?;
        Some(format!("Prayer of {}\nTeaches a cleric the {} term", prayer_noun(term), term.name))
    }

    /// Makes this scroll a prayer scroll if its seed says so ([`PRAYER_NEAR`] by holy ground,
    /// [`PRAYER_FAR`] elsewhere): rolled from its own stream, so the same seed names the same
    /// term. A no-op for anything but a plain scroll, one that already teaches, and when no term
    /// is eligible.
    pub(crate) fn rolled_prayer(mut self, holy: bool) -> Item {
        let is_scroll = matches!(&self.shape, armory::Item::Writing(s) if s.piece.kind == armory::families::writing::WritingKind::Scroll);
        if !is_scroll || self.prayer.is_some() || self.teaching.is_some() || prayer_terms().is_empty() {
            return self;
        }
        let mut rng = Mulberry32::new(self.seed ^ PRAYER_SALT);
        if rng.chance(if holy { PRAYER_NEAR } else { PRAYER_FAR }) {
            let term = rng.pick(prayer_terms());
            if self.affixes.is_empty() {
                self.name = format!("Scroll of {}", prayer_noun(term));
            }
            self.prayer = Some(term.id.clone());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::Lore;
    use crate::roll_item;

    #[test]
    fn the_terms_are_divine_not_known_at_the_start_and_not_a_devotions() {
        let ids: Vec<_> = prayer_terms().iter().map(|t| t.id.as_str()).collect();
        assert!(!ids.is_empty());
        for devotion in DEVOTION_TERMS {
            assert!(!ids.contains(&devotion));
        }
        assert!(!ids.contains(&"light") && !ids.contains(&"ward"));
    }

    #[test]
    fn the_same_seed_names_the_same_term_and_only_scrolls_are_prayers() {
        let found = |base: &str, seed, holy| roll_item(base, seed).unwrap().in_world_at(&Lore::default(), None, holy);
        let prayers: Vec<_> = (0..400).filter_map(|s| found("Scroll", s, true).prayer().map(|p| (s, p.to_string()))).collect();
        assert!(prayers.len() > 100 && prayers.len() < 220, "{}", prayers.len());
        for (s, p) in &prayers {
            assert_eq!(found("Scroll", *s, true).prayer(), Some(p.as_str()));
            assert!(prayer_term(p).is_some());
        }
        for base in ["Book", "Note", "Ring"] {
            assert!((0..200).all(|s| found(base, s, true).prayer().is_none()), "{base}");
        }
    }

    #[test]
    fn they_are_commoner_by_holy_ground() {
        let count = |holy| (0..1_000).filter(|&s| roll_item("Scroll", s).unwrap().in_world_at(&Lore::default(), None, holy).prayer().is_some()).count();
        assert!(count(true) > count(false) * 3, "{} {}", count(true), count(false));
    }

    #[test]
    fn the_constructor_names_the_scroll_and_the_card_says_what_it_teaches() {
        let term = &prayer_terms()[0];
        let scroll = prayer_scroll(5, &term.id).unwrap();
        assert_eq!(scroll.prayer(), Some(term.id.as_str()));
        assert_eq!(scroll.name, format!("Scroll of {}", prayer_noun(term)));
        let line = scroll.prayer_line().unwrap();
        assert!(line.contains(&format!("Teaches a cleric the {} term", term.name)), "{line}");
        assert!(prayer_scroll(5, "bane").is_none() && prayer_scroll(5, "nonsense").is_none());
        assert_eq!(scroll.clone().in_world_at(&Lore::default(), None, true).prayer(), scroll.prayer(), "kept wherever it goes");
    }
}
