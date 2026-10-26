//! Finding out what a magic item does.
//!
//! An item found below can be unknown ([`Item::unidentified`]): it reads as "Unknown ring" and
//! its affixes are hidden from the card until the delver identifies it. What it is made of and
//! what it does are the same either way (its stats count, worn or not); only the card hides.
//! Identifying is a check ([`dc`] is its target), and the margin decides how much is learned
//! ([`learned`]): a roll that meets the target reveals every affix, one that falls a little short
//! reveals some, one that falls far short reveals nothing. An item can be tried again, and what
//! was revealed stays revealed.
//!
//! [`Known`] is the state, kept on the item and in its record. An item with no affix has
//! nothing to hide and is always fully known, and so is every item rolled but not found.

use serde::{Deserialize, Serialize};

use crate::{Item, Rarity};

/// How much of an item's magic the delver has found out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Known {
    /// Everything: the real name and every affix. What every item is until found unknown, and
    /// how a record from before items could be unknown reads.
    #[default]
    Full,
    /// This many of its affixes, in the order the card lists them, are revealed.
    Partly(u8),
    /// None of it.
    Unknown,
}

impl Known {
    /// For serde: a fully known item writes nothing.
    pub fn is_full(&self) -> bool {
        *self == Known::Full
    }
}

/// How far short of the target a roll may fall before it learns nothing.
pub const NOTHING_BELOW: i32 = 8;

/// The roll that identifies an item of `rarity`.
pub fn dc(rarity: Rarity) -> i32 {
    match rarity {
        Rarity::Common => 8,
        Rarity::Magic => 12,
        Rarity::Rare => 15,
        Rarity::Legendary => 18,
    }
}

/// What a roll of `total` against `target` reveals of an item with `affixes` affixes, given what
/// was `before`. A roll that meets the target reveals all of them; one that falls short reveals a
/// share that shrinks to nothing [`NOTHING_BELOW`] under it, at least one while it is above that.
/// What was already revealed stays so.
pub fn learned(before: Known, affixes: usize, total: i32, target: i32) -> Known {
    let had = match before {
        Known::Full => return Known::Full,
        Known::Partly(n) => n as usize,
        Known::Unknown => 0,
    };
    if affixes == 0 || total >= target {
        return Known::Full;
    }
    let share = ((total - (target - NOTHING_BELOW)) as f32 / NOTHING_BELOW as f32).clamp(0.0, 1.0);
    let revealed = if share > 0.0 { ((share * affixes as f32).floor() as usize).max(1) } else { 0 };
    let now = revealed.max(had).min(affixes.saturating_sub(1));
    match now {
        0 => Known::Unknown,
        n => Known::Partly(n as u8),
    }
}

impl Item {
    /// This item as found: unknown if it is magic or better with affixes to hide, else as it is.
    pub fn unidentified(mut self) -> Item {
        if self.rarity >= Rarity::Magic && !self.affixes.is_empty() {
            self.known = Known::Unknown;
        }
        self
    }

    /// Whether every affix is revealed.
    pub fn is_known(&self) -> bool {
        self.known == Known::Full
    }

    /// The name the card shows: the real one once known; "Unknown ring" while nothing is, and
    /// "Partly known ring" while some is, since a name tells every affix.
    pub fn shown_title(&self) -> String {
        match self.known {
            Known::Full => self.title().to_string(),
            Known::Partly(_) => format!("Partly known {}", self.base.to_lowercase()),
            Known::Unknown => format!("Unknown {}", self.base.to_lowercase()),
        }
    }

    /// The affixes the card lists: all once known, the first few while partly known, none while unknown.
    pub fn shown_affixes(&self) -> &[&'static crate::tables::Affix] {
        match self.known {
            Known::Full => &self.affixes,
            Known::Partly(n) => &self.affixes[..(n as usize).min(self.affixes.len())],
            Known::Unknown => &[],
        }
    }

    /// How many affixes are still hidden.
    pub fn hidden_affixes(&self) -> usize {
        self.affixes.len() - self.shown_affixes().len()
    }

    /// The roll that identifies it: [`dc`] of its rarity.
    pub fn identify_dc(&self) -> i32 {
        dc(self.rarity)
    }

    /// Sets what a roll of `total` against its [`Item::identify_dc`] revealed ([`learned`]).
    pub fn reveal(&mut self, total: i32) {
        self.known = learned(self.known, self.affixes.len(), total, self.identify_dc());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roll_item;

    fn magic_ring() -> Item {
        (1..4000).filter_map(|s| crate::roll_item_with("Ring", s, crate::Odds::loot(10))).find(|i| i.rarity >= Rarity::Rare && i.affixes.len() >= 3).expect("a ring with three affixes")
    }

    #[test]
    fn a_found_magic_ring_is_unknown_and_hides_its_affixes() {
        let ring = magic_ring();
        assert!(ring.is_known() && !ring.affixes.is_empty());
        let found = ring.clone().unidentified();
        assert_eq!(found.known, Known::Unknown);
        assert_eq!(found.shown_title(), "Unknown ring");
        assert!(found.shown_affixes().is_empty());
        assert_eq!(found.hidden_affixes(), ring.affixes.len());
        // What it does is unchanged.
        assert_eq!(found.profile.stats, ring.profile.stats);
    }

    #[test]
    fn a_common_item_has_nothing_to_hide() {
        let plain = (1..200).filter_map(|s| roll_item("Ring", s)).find(|i| i.affixes.is_empty()).expect("a plain ring");
        assert!(plain.unidentified().is_known());
    }

    #[test]
    fn a_high_roll_reveals_every_affix_and_a_low_one_some() {
        let mut ring = magic_ring().unidentified();
        let dc = ring.identify_dc();
        let n = ring.affixes.len();
        ring.reveal(dc - 4);
        assert!(matches!(ring.known, Known::Partly(k) if k as usize >= 1 && (k as usize) < n), "{:?}", ring.known);
        assert_eq!(ring.shown_affixes().len(), ring.affixes.len() - ring.hidden_affixes());
        assert_eq!(ring.shown_title(), "Partly known ring");
        ring.reveal(dc);
        assert!(ring.is_known());
        assert_eq!(ring.shown_title(), ring.title());
        assert_eq!(ring.shown_affixes().len(), n);
    }

    #[test]
    fn a_very_low_roll_learns_nothing_and_what_was_learned_stays() {
        let mut ring = magic_ring().unidentified();
        ring.reveal(ring.identify_dc() - NOTHING_BELOW);
        assert_eq!(ring.known, Known::Unknown);
        ring.reveal(ring.identify_dc() - 3);
        let Known::Partly(first) = ring.known else { panic!("{:?}", ring.known) };
        ring.reveal(ring.identify_dc() - NOTHING_BELOW);
        assert_eq!(ring.known, Known::Partly(first));
    }

    #[test]
    fn known_survives_taking_an_item_apart_and_putting_it_back() {
        let mut ring = magic_ring().unidentified();
        ring.reveal(ring.identify_dc() - 3);
        let back = Item::from_parts(crate::ItemParts::from(ring.clone())).unwrap();
        assert_eq!(back, ring);
    }

    #[test]
    fn a_roll_never_reveals_everything_by_falling_short() {
        for total in -5..30 {
            let k = learned(Known::Unknown, 4, total, 15);
            assert_eq!(k == Known::Full, total >= 15, "{total}");
        }
    }
}
