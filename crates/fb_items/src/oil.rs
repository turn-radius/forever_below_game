//! Lamp oil: a flask of it refills a lantern ("Fuel · six hours of lantern light",
//! `data/goods.json`). The flask's fill is its oil, a full flask one full lantern's worth: it
//! fills any lantern from empty, whatever the lantern's `BurnTime`, which decides how long that
//! oil then burns. Our own reading of the plate's line, which nothing used.

use armory::families::food::FoodStyle;

use crate::item::Item;

/// The base of the flask of lamp oil.
pub const LAMP_OIL: &str = "Lamp oil";

impl Item {
    /// Whether this is a flask of lamp oil with oil left in it.
    pub fn has_oil(&self) -> bool {
        self.base == LAMP_OIL && matches!(&self.shape, armory::Item::Food(s) if s.provision.style == FoodStyle::Flask && s.provision.is_full())
    }

    /// How much more oil this lantern takes, as a share of a full one; `None` if it is not a
    /// lantern.
    pub fn oil_room(&self) -> Option<f32> {
        if self.shape.family() != armory::Family::Lantern {
            return None;
        }
        let capacity = self.fuel_capacity()?.max(1.0);
        Some((1.0 - self.fuel_left()? / capacity).max(0.0))
    }

    /// Fills this lantern from the lamp-oil `flask`, as far as either goes: the flask's fill drops
    /// by the share of a full lantern poured. Dregs too little to pour again go in with the rest.
    /// Returns the share poured; 0, and nothing changed, if this is not a lantern, it is full, or
    /// the flask holds no oil.
    pub fn refill_from(&mut self, flask: &mut Item) -> f32 {
        if !flask.has_oil() {
            return 0.0;
        }
        let Some(capacity) = self.fuel_capacity().map(|c| c.max(1.0)) else { return 0.0 };
        let armory::Item::Food(oil) = &mut flask.shape else { return 0.0 };
        let poured = self.shape.refuel(oil.provision.fill * capacity, capacity) / capacity;
        oil.provision.fill = (oil.provision.fill - poured).max(0.0);
        if !oil.provision.is_full() {
            oil.provision.fill = 0.0;
        }
        poured
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::roll_item;

    fn fill(flask: &Item) -> f32 {
        match &flask.shape {
            armory::Item::Food(s) => s.provision.fill,
            _ => unreachable!(),
        }
    }

    /// A lit lantern whose `BurnTime` is `burn_time`, with `share` of a full one's oil; out if
    /// it has none.
    fn lantern_at(share: f32, burn_time: f32) -> Item {
        let mut lantern = roll_item("Lantern", 3).unwrap();
        lantern.profile.stats.burn_time = burn_time;
        lantern.shape.set_lit(true).unwrap();
        lantern.burn(1.0e9);
        let capacity = lantern.fuel_capacity().unwrap();
        if share > 0.0 {
            lantern.shape.refuel(capacity * share, capacity);
            lantern.shape.set_lit(true).unwrap();
        }
        lantern
    }

    fn empty_lantern(burn_time: f32) -> Item {
        lantern_at(0.0, burn_time)
    }

    fn full_flask() -> Item {
        let mut flask = roll_item(LAMP_OIL, 3).unwrap();
        if let armory::Item::Food(s) = &mut flask.shape {
            s.provision.fill = 1.0;
        }
        flask
    }

    #[test]
    fn every_lamp_oil_flask_is_sold_with_oil() {
        for seed in 1..50 {
            assert!(roll_item(LAMP_OIL, seed).unwrap().has_oil(), "seed {seed}");
        }
        assert!(!roll_item("Waterskin", 3).unwrap().has_oil());
        assert!(!roll_item("Lantern", 3).unwrap().has_oil());
    }

    #[test]
    fn a_full_flask_fills_an_empty_lantern_and_is_emptied() {
        let (mut lantern, mut flask) = (empty_lantern(1.0), full_flask());
        assert_eq!(lantern.oil_room(), Some(1.0));
        assert!((lantern.refill_from(&mut flask) - 1.0).abs() < 1e-6);
        assert_eq!(lantern.fuel_left(), lantern.fuel_capacity());
        assert!(!flask.has_oil() && fill(&flask) == 0.0);
        assert_eq!(lantern.oil_room(), Some(0.0));
    }

    #[test]
    fn a_half_burnt_lantern_takes_half_a_flask() {
        let mut lantern = lantern_at(0.5, 1.0);
        let mut flask = full_flask();
        assert!((lantern.refill_from(&mut flask) - 0.5).abs() < 1e-4);
        assert!((fill(&flask) - 0.5).abs() < 1e-4 && flask.has_oil());
        assert_eq!(lantern.shape.lit(), Some(true), "a lit lantern stays lit");
    }

    #[test]
    fn a_long_burning_lantern_burns_the_same_oil_longer() {
        let (mut lantern, mut flask) = (empty_lantern(2.0), full_flask());
        lantern.refill_from(&mut flask);
        assert!((lantern.fuel_left().unwrap() - armory::kindle::LANTERN_SECONDS * 2.0).abs() < 1.0);
    }

    #[test]
    fn a_full_lantern_or_an_empty_flask_pours_nothing() {
        let mut full = lantern_at(1.0, 1.0);
        let mut flask = full_flask();
        assert_eq!(full.refill_from(&mut flask), 0.0);
        assert_eq!(fill(&flask), 1.0);
        let mut lantern = empty_lantern(1.0);
        lantern.refill_from(&mut flask);
        let mut again = empty_lantern(1.0);
        assert_eq!(again.refill_from(&mut flask), 0.0);
        assert_eq!(again.fuel_left(), Some(0.0));
    }

    #[test]
    fn only_a_lantern_is_refilled() {
        let mut torch = roll_item("Torch", 3).unwrap();
        let mut flask = full_flask();
        assert_eq!(torch.oil_room(), None);
        assert_eq!(torch.refill_from(&mut flask), 0.0);
        assert_eq!(fill(&flask), 1.0);
    }
}
