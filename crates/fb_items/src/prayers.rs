//! Prayer scrolls: writing that teaches a cleric one Divine term ([`Item::prayer`]).
//!
//! A scroll found in the abyss now and then names one term of the Divine school that a cleric
//! does not start with, rolled from the scroll's seed among the level's pool
//! ([`crate::notes::Lore::prayers`]). A cleric who reads it can write with that term in the
//! spellbook (`fb_character::casting::Spellbook::unlock`); any other reader learns nothing. The
//! term is recorded on the item when it is found ([`Item::in_world_at`]) and saved with it, so an
//! item that changes later keeps it. Reading does not use the scroll up, as with the scrolls that
//! teach a denizen tongue.
//!
//! The pool is the game's to fill from its rules: every Divine term not known from the start,
//! except the ones a devotion grants (`fb_character::casting::scroll_terms`). This crate keeps
//! no list of its own, and with an empty pool no scroll is a prayer.
//!
//! All our own rules, for a designer to tune.

use procgen_rng::Mulberry32;

use crate::Item;

/// Salt for whether a scroll is a prayer scroll, and which term it teaches.
const PRAYER_SALT: u32 = 0x5A91_C3D7;

/// Chance that a scroll found anywhere else is a prayer scroll.
pub const PRAYER_FAR: f64 = 0.06;
/// Chance for a scroll found by a shrine or in a faction's hall.
pub const PRAYER_NEAR: f64 = 0.4;

/// What the scroll for the term `id` is a prayer of: "Healing" for `heal`, "the Cone" for a term
/// with no word of its own.
pub fn prayer_noun(id: &str) -> String {
    match id {
        "heal" => "Healing".to_string(),
        "cure" => "Curing".to_string(),
        _ => format!("the {}", term_name(id)),
    }
}

/// The term `id` as the book shows it, "Heal": the id with its first letter raised and its
/// underscores spaces.
pub fn term_name(id: &str) -> String {
    let spaced = id.replace('_', " ");
    let mut letters = spaced.chars();
    letters.next().map(|c| c.to_uppercase().chain(letters).collect()).unwrap_or_default()
}

/// The scroll of `seed` teaching the term `term` (an id of `[casting.terms]`), named for it:
/// "Scroll of Healing". For a healer's stock. `None` for an empty id.
pub fn prayer_scroll(seed: u32, term: &str) -> Option<Item> {
    if term.is_empty() {
        return None;
    }
    let mut scroll = crate::roll_item("Scroll", seed)?;
    scroll.prayer = Some(term.to_string());
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
        let id = self.prayer()?;
        Some(format!("Prayer of {}\nTeaches a cleric the {} term", prayer_noun(id), term_name(id)))
    }

    /// Makes this scroll a prayer scroll if its seed says so ([`PRAYER_NEAR`] by holy ground,
    /// [`PRAYER_FAR`] elsewhere), naming a term from `pool`: rolled from its own stream, so the
    /// same seed and pool name the same term. A no-op for anything but a plain scroll, one that
    /// already teaches, and when the pool is empty.
    pub(crate) fn rolled_prayer(mut self, pool: &[String], holy: bool) -> Item {
        let is_scroll = matches!(&self.shape, armory::Item::Writing(s) if s.piece.kind == armory::families::writing::WritingKind::Scroll);
        if !is_scroll || self.prayer.is_some() || self.teaching.is_some() || pool.is_empty() {
            return self;
        }
        let mut rng = Mulberry32::new(self.seed ^ PRAYER_SALT);
        if rng.chance(if holy { PRAYER_NEAR } else { PRAYER_FAR }) {
            let term = rng.pick(pool);
            if self.affixes.is_empty() {
                self.name = format!("Scroll of {}", prayer_noun(term));
            }
            self.prayer = Some(term.clone());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::Lore;
    use crate::roll_item;

    fn lore(pool: &[&str]) -> Lore {
        Lore { prayers: pool.iter().map(|t| t.to_string()).collect(), ..Lore::default() }
    }

    #[test]
    fn the_same_seed_names_the_same_term_and_the_terms_vary() {
        let pool = lore(&["heal", "cure", "cone"]);
        let found = |seed| roll_item("Scroll", seed).unwrap().in_world_at(&pool, None, true);
        let prayers: Vec<_> = (0..400).filter_map(|s| found(s).prayer().map(|p| (s, p.to_string()))).collect();
        assert!(prayers.len() > 100 && prayers.len() < 220, "{}", prayers.len());
        for (s, p) in &prayers {
            assert_eq!(found(*s).prayer(), Some(p.as_str()), "the same seed, the same term");
        }
        for term in ["heal", "cure", "cone"] {
            assert!(prayers.iter().any(|(_, p)| p == term), "{term} is taught by some seed");
        }
    }

    #[test]
    fn only_scrolls_are_prayers_and_an_empty_pool_has_none() {
        let pool = lore(&["heal", "cure"]);
        for base in ["Book", "Note", "Ring"] {
            assert!((0..200).all(|s| roll_item(base, s).unwrap().in_world_at(&pool, None, true).prayer().is_none()), "{base}");
        }
        assert!((0..400).all(|s| roll_item("Scroll", s).unwrap().in_world_at(&Lore::default(), None, true).prayer().is_none()));
    }

    #[test]
    fn they_are_commoner_by_holy_ground() {
        let pool = lore(&["heal", "cure"]);
        let count = |holy| (0..1_000).filter(|&s| roll_item("Scroll", s).unwrap().in_world_at(&pool, None, holy).prayer().is_some()).count();
        assert!(count(true) > count(false) * 3, "{} {}", count(true), count(false));
    }

    #[test]
    fn the_constructor_names_the_scroll_and_the_card_says_what_it_teaches() {
        let scroll = prayer_scroll(5, "heal").unwrap();
        assert_eq!(scroll.prayer(), Some("heal"));
        assert_eq!(scroll.name, "Scroll of Healing");
        assert_eq!(scroll.prayer_line().unwrap(), "Prayer of Healing\nTeaches a cleric the Heal term");
        assert!(prayer_scroll(5, "").is_none());
        assert_eq!(scroll.clone().in_world_at(&lore(&["cure"]), None, true).prayer(), Some("heal"), "kept wherever it goes");
        assert_eq!(prayer_scroll(5, "cone").unwrap().name, "Scroll of the Cone");
    }
}
