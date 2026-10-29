//! Incense: a dish of it loads a censer ("Fuel · one hour of incense smoke", `data/goods.json`).
//! Loading uses the dish up and fills the censer with that incense, replacing whatever was left
//! in it. The censer burns it down as a lantern burns oil ([`Item::burn`]) and goes out when it
//! is gone.
//!
//! What each kind protects against belongs to the systems that read the kind off a lit censer
//! ([`Item::burning_incense`]); nothing here gives an effect.

use armory::families::sundry::SundryKind;
use armory::incense::IncenseKind;

use crate::item::Item;

/// The base of a dish of incense.
pub const INCENSE: &str = "Incense";
/// The base of the censer.
pub const CENSER: &str = "Censer";

impl Item {
    /// The kind of incense in this dish; `None` if it is not a dish of incense.
    pub fn incense_kind(&self) -> Option<IncenseKind> {
        match &self.shape {
            armory::Item::Sundry(s) if s.article.kind == SundryKind::Incense => Some(s.article.incense),
            _ => None,
        }
    }

    /// Whether this is a censer (whatever it burns, lit or not).
    pub fn is_censer(&self) -> bool {
        matches!(&self.shape, armory::Item::Lantern(s) if s.body.style == armory::families::lantern::LanternStyle::Censer)
    }

    /// The incense a lit censer is burning right now, or `None` if this is not a censer, it is out
    /// or it has none left. This is the hook for what each kind does while it smokes.
    pub fn burning_incense(&self) -> Option<IncenseKind> {
        if !self.is_censer() || self.shape.lit() != Some(true) {
            return None;
        }
        self.shape.incense()
    }

    /// Whether this censer has room for a load: it is a censer and not full.
    pub fn censer_room(&self) -> bool {
        self.is_censer() && self.fuel_capacity().is_some_and(|cap| self.fuel_left().is_some_and(|left| left < cap * 0.999))
    }

    /// Loads this censer from the dish `dish`. Returns whether it took: the dish is a dish of
    /// incense and this a censer with room. The caller uses the dish up. The fuel follows the
    /// censer's `BurnTime`.
    pub fn load_from(&mut self, dish: &Item) -> bool {
        let Some(kind) = dish.incense_kind() else { return false };
        if !self.censer_room() || !self.shape.load_incense(kind) {
            return false;
        }
        self.shape.scale_fuel(self.profile.stats.burn_time);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::roll_item;

    fn lit_censer(seed: u32) -> Item {
        let mut censer = roll_item(CENSER, seed).unwrap();
        censer.shape.load_incense(IncenseKind::Cedar);
        censer.shape.set_lit(true).unwrap();
        censer
    }

    #[test]
    fn a_censer_is_a_lantern_that_hangs_at_the_hip() {
        use crate::Slot;
        let censer = roll_item(CENSER, 3).unwrap();
        assert!(censer.is_censer() && censer.shape.family() == armory::Family::Lantern);
        assert_eq!(censer.slots(), vec![Slot::Offhand, Slot::Mainhand, Slot::Hip]);
        assert!(!roll_item("Lantern", 3).unwrap().is_censer());
        assert!(censer.profile.stats.light > 0.0 && censer.profile.stats.light < 30.0);
    }

    #[test]
    fn every_incense_dish_holds_a_kind() {
        let kinds: std::collections::HashSet<_> = (1..60).map(|s| roll_item(INCENSE, s).unwrap().incense_kind().unwrap()).collect();
        assert_eq!(kinds.len(), 3);
        assert!(roll_item("Candle", 3).unwrap().incense_kind().is_none());
    }

    #[test]
    fn a_dish_loads_an_empty_censer_with_its_kind_and_it_burns_down() {
        let mut censer = lit_censer(5);
        censer.burn(1.0e9);
        assert_eq!((censer.shape.lit(), censer.burning_incense()), (Some(false), None), "used up, it goes out");
        let dish = (1..60).map(|s| roll_item(INCENSE, s).unwrap()).find(|d| d.incense_kind() == Some(IncenseKind::Frankincense)).unwrap();
        assert!(censer.censer_room() && censer.load_from(&dish));
        assert_eq!(censer.shape.incense(), Some(IncenseKind::Frankincense));
        assert!(!censer.censer_room(), "a full censer takes no more");
        censer.shape.set_lit(true).unwrap();
        assert_eq!(censer.burning_incense(), Some(IncenseKind::Frankincense));
        let full = censer.fuel_left().unwrap();
        censer.burn(600.0);
        assert!((full - censer.fuel_left().unwrap() - 600.0).abs() < 1.0);
    }

    #[test]
    fn only_a_censer_takes_incense_and_oil_does_not_fill_it() {
        let dish = roll_item(INCENSE, 2).unwrap();
        let mut lantern = roll_item("Lantern", 3).unwrap();
        assert!(!lantern.load_from(&dish));
        let mut censer = lit_censer(5);
        censer.burn(600.0);
        let mut flask = roll_item(crate::LAMP_OIL, 3).unwrap();
        assert_eq!(censer.refill_from(&mut flask), 0.0);
        assert!(!censer.load_from(&roll_item("Candle", 1).unwrap()));
    }
}
