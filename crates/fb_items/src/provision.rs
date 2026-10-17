//! What eating or drinking a food or drink base does to the delver's hunger and thirst
//! (`docs/items.md`, "Food and drink"). Our own table; the plate only describes these as food.
//! Each value is how much it takes off the meter, where a meter runs from 0 (Fed) up to 1.
//! Healing is not here: the draught's cure is a separate rule.

use armory::families::food::FoodStyle;

use crate::item::Item;

/// How much one item takes off hunger and thirst.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Provision {
    pub nutrition: f64,
    pub water: f64,
}

/// What a swallow is called: bread is eaten, anything poured is drunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Consume {
    Eat,
    Drink,
}

impl Consume {
    pub fn verb(self) -> &'static str {
        match self {
            Consume::Eat => "Eat",
            Consume::Drink => "Drink",
        }
    }

    /// For "You eat the bread".
    pub fn past(self) -> &'static str {
        match self {
            Consume::Eat => "eat",
            Consume::Drink => "drink",
        }
    }
}

const fn food(nutrition: f64) -> Provision {
    Provision { nutrition, water: 0.0 }
}

const fn drink(water: f64) -> Provision {
    Provision { nutrition: 0.0, water }
}

/// Every base that is used up by eating or drinking. The waterskin and the lamp oil are not:
/// a waterskin is a vessel that stays and is drunk from ([`Item::sip`]). The potion and the
/// healing draught are drinks that quench nothing here.
pub const PROVISIONS: &[(&str, Provision)] = &[
    ("Bread", food(0.4)),
    ("Cheese", food(0.3)),
    ("Smoked fish", food(0.3)),
    ("Apple", food(0.15)),
    ("Meat pie", food(0.5)),
    ("Sausages", food(0.35)),
    ("Roast haunch", food(0.6)),
    ("Blind cavefish", food(0.25)),
    ("Glowcap", food(0.1)),
    ("Tankard", drink(0.5)),
    ("Wine", drink(0.3)),
    ("Potion", drink(0.0)),
    ("Healing draught", drink(0.0)),
];

/// How much of a waterskin's fill one swallow takes, and how much thirst it quenches. A full
/// skin is four swallows. Our own.
pub const SWALLOW: f32 = 0.25;

/// What base `base` takes off the meters, or `None` if it is not eaten or drunk.
pub fn provision(base: &str) -> Option<Provision> {
    PROVISIONS.iter().find(|(name, _)| *name == base).map(|(_, p)| *p)
}

impl Item {
    /// What eating or drinking this does, or `None` for anything that cannot be.
    pub fn provision(&self) -> Option<Provision> {
        provision(&self.base)
    }

    /// Whether it is eaten or drunk, or `None` if it is neither.
    pub fn consume(&self) -> Option<Consume> {
        self.provision()?;
        Some(match &self.shape {
            armory::Item::Food(s) if matches!(s.provision.style, FoodStyle::Tankard | FoodStyle::Bottle | FoodStyle::Potion) => Consume::Drink,
            _ => Consume::Eat,
        })
    }

    /// Whether this is a waterskin with water left to drink from.
    pub fn can_sip(&self) -> bool {
        matches!(&self.shape, armory::Item::Food(s) if s.provision.style == FoodStyle::Waterskin && s.provision.is_full())
    }

    /// Takes a swallow from a waterskin: its fill drops by up to [`SWALLOW`], and the water that
    /// quenches is what came out. `None`, and nothing changed, if it is not a skin with water.
    pub fn sip(&mut self) -> Option<Provision> {
        if !self.can_sip() {
            return None;
        }
        let armory::Item::Food(s) = &mut self.shape else { return None };
        let taken = s.provision.fill.min(SWALLOW);
        s.provision.fill -= taken;
        Some(drink(taken as f64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::roll_item;

    #[test]
    fn the_table_names_real_bases() {
        for (base, _) in PROVISIONS {
            assert!(roll_item(base, 1).is_some(), "{base}");
        }
    }

    #[test]
    fn a_loaf_cheese_apple_and_tankard_have_their_values() {
        assert_eq!(provision("Bread").unwrap().nutrition, 0.4);
        assert_eq!(provision("Cheese").unwrap().nutrition, 0.3);
        assert_eq!(provision("Apple").unwrap().nutrition, 0.15);
        assert_eq!(provision("Tankard").unwrap().water, 0.5);
        assert_eq!(provision("Healing draught"), Some(Provision { nutrition: 0.0, water: 0.0 }));
        assert!(provision("Torch").is_none() && provision("Waterskin").is_none());
    }

    #[test]
    fn a_waterskin_is_drunk_from_a_swallow_at_a_time() {
        let fill = |item: &Item| match &item.shape {
            armory::Item::Food(s) => s.provision.fill,
            _ => unreachable!(),
        };
        let mut skin = roll_item("Waterskin", 3).unwrap();
        assert!(skin.consume().is_none() && skin.can_sip());
        let before = fill(&skin);
        let swallow = skin.sip().unwrap();
        assert!((swallow.water - SWALLOW as f64).abs() < 1e-6 && swallow.nutrition == 0.0);
        assert!((before - fill(&skin) - SWALLOW).abs() < 1e-6);
        while skin.sip().is_some() {}
        assert!(!skin.can_sip());
        assert!(roll_item("Bread", 3).unwrap().sip().is_none());
    }

    #[test]
    fn a_sipped_skins_fill_survives_a_round_trip() {
        let mut skin = roll_item("Waterskin", 3).unwrap();
        skin.sip().unwrap();
        let back: armory::Item = serde_json::from_str(&serde_json::to_string(&skin.shape).unwrap()).unwrap();
        assert_eq!(back, skin.shape);
    }

    #[test]
    fn drinks_are_drunk_and_the_rest_eaten() {
        for seed in 1..20 {
            assert_eq!(roll_item("Tankard", seed).unwrap().consume(), Some(Consume::Drink));
            assert_eq!(roll_item("Wine", seed).unwrap().consume(), Some(Consume::Drink));
            assert_eq!(roll_item("Healing draught", seed).unwrap().consume(), Some(Consume::Drink));
            assert_eq!(roll_item("Bread", seed).unwrap().consume(), Some(Consume::Eat));
            assert_eq!(roll_item("Torch", seed).unwrap().consume(), None);
        }
    }
}
