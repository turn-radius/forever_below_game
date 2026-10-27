//! What eating or drinking a food or drink base does to the delver's hunger and thirst
//! (`docs/items.md`, "Food and drink"). Our own table; the plate only describes these as food.
//! Each value is how much it takes off the meter, where a meter runs from 0 (Fed) up to 1.
//! Healing is not here: the draught's cure is a separate rule.

use armory::families::food::{Brew, FoodStyle, PotionSize};

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
    ("Vial", drink(0.0)),
    ("Small potion", drink(0.0)),
    ("Potion", drink(0.0)),
    ("Large potion", drink(0.0)),
    ("Healing vial", drink(0.0)),
    ("Small healing draught", drink(0.0)),
    ("Healing draught", drink(0.0)),
    ("Large healing draught", drink(0.0)),
    ("Elixir of the Hare", drink(0.0)),
    ("Elixir of Vigil", drink(0.0)),
    ("Elixir of Fasting", drink(0.0)),
    ("Elixir of Embers", drink(0.0)),
];

/// Every potion base and the size of bottle it is. The plain ones hold the brew their seed
/// rolls, the healing ones the healing brew ([`crate::armory_base`]). The old `Potion` and
/// `Healing draught` are the medium size, so old saves keep their meaning.
pub const POTIONS: &[(&str, PotionSize)] = &[
    ("Vial", PotionSize::Vial),
    ("Small potion", PotionSize::Small),
    ("Potion", PotionSize::Medium),
    ("Large potion", PotionSize::Large),
    ("Healing vial", PotionSize::Vial),
    ("Small healing draught", PotionSize::Small),
    ("Healing draught", PotionSize::Medium),
    ("Large healing draught", PotionSize::Large),
];

/// The healing potion base of `size`.
pub fn healing_base(size: PotionSize) -> &'static str {
    match size {
        PotionSize::Vial => "Healing vial",
        PotionSize::Small => "Small healing draught",
        PotionSize::Medium => "Healing draught",
        PotionSize::Large => "Large healing draught",
    }
}

/// What a healing potion restores: `dice` d4 and a bonus, so a bigger flask heals more. Our own
/// numbers, the medium one SRD 5.1's Potion of Healing (2d4 + 2) and the large its Greater (4d4 + 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Heal {
    pub dice: u32,
    pub bonus: u32,
}

impl Heal {
    pub fn of(size: PotionSize) -> Heal {
        match size {
            PotionSize::Vial => Heal { dice: 1, bonus: 1 },
            PotionSize::Small => Heal { dice: 2, bonus: 0 },
            PotionSize::Medium => Heal { dice: 2, bonus: 2 },
            PotionSize::Large => Heal { dice: 4, bonus: 4 },
        }
    }

    pub fn least(self) -> u32 {
        self.dice + self.bonus
    }

    pub fn most(self) -> u32 {
        self.dice * 4 + self.bonus
    }
}

/// What drinking an elixir does: the id of an effect in `data/effects.json`, at an intensity
/// (`docs/items.md`, "Elixirs"). Our own. The game gives it to the delver.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Elixir {
    pub effect: &'static str,
    pub intensity: f64,
}

/// Every elixir, the alchemist's goods: base and its effect.
pub const ELIXIRS: &[(&str, Elixir)] = &[
    ("Elixir of the Hare", Elixir { effect: "swift", intensity: 1.0 }),
    ("Elixir of Vigil", Elixir { effect: "wakeful", intensity: 1.0 }),
    ("Elixir of Fasting", Elixir { effect: "full_bellied", intensity: 1.0 }),
    ("Elixir of Embers", Elixir { effect: "warmed", intensity: 1.0 }),
];

/// The effect elixir `base` gives, or `None` if it is not an elixir.
pub fn elixir(base: &str) -> Option<Elixir> {
    ELIXIRS.iter().find(|(name, _)| *name == base).map(|(_, e)| *e)
}

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

    /// The size of bottle this is, if it is a potion.
    pub fn potion_size(&self) -> Option<PotionSize> {
        POTIONS.iter().find(|(name, _)| *name == self.base).map(|(_, size)| *size)
    }

    /// What drinking this gives the delver, if it is an elixir.
    pub fn elixir(&self) -> Option<Elixir> {
        elixir(&self.base)
    }

    /// Whether it is eaten or drunk, or `None` if it is neither.
    pub fn consume(&self) -> Option<Consume> {
        self.provision()?;
        Some(match &self.shape {
            armory::Item::Food(s) if matches!(s.provision.style, FoodStyle::Tankard | FoodStyle::Bottle | FoodStyle::Potion) => Consume::Drink,
            _ => Consume::Eat,
        })
    }

    /// What a potion with something in it holds, named by its colour as the Armory names it
    /// (an Antidote, a Mana Potion); `None` for anything else.
    pub fn brew(&self) -> Option<Brew> {
        match &self.shape {
            armory::Item::Food(s) if s.provision.style == FoodStyle::Potion && s.provision.is_full() => Some(Brew::nearest(s.provision.liquid)),
            _ => None,
        }
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

    #[test]
    fn elixirs_roll_and_are_drunk() {
        for (base, e) in ELIXIRS {
            let item = roll_item(base, 5).unwrap();
            assert_eq!(item.consume(), Some(Consume::Drink));
            assert_eq!(item.elixir(), Some(*e));
            assert_eq!(item.title(), *base);
        }
        assert!(roll_item("Healing draught", 5).unwrap().elixir().is_none());
        // Distinct colours.
        let liquids: Vec<_> = ELIXIRS.iter().map(|(b, _)| match roll_item(b, 5).unwrap().shape { armory::Item::Food(s) => s.provision.liquid, _ => unreachable!() }).collect();
        for (i, a) in liquids.iter().enumerate() {
            assert!(liquids[i + 1..].iter().all(|b| a != b));
        }
    }

    #[test]
    fn bedrolls_and_tinderboxes_roll() {
        for base in ["Bedroll", "Tinderbox"] {
            let item = roll_item(base, 9).unwrap();
            assert_eq!(item.title(), base);
            assert!(item.consume().is_none());
        }
    }

    #[test]
    fn a_potion_holds_the_brew_it_is_named_for() {
        let antidote = (0..200).map(|seed| roll_item("Potion", seed).unwrap()).find(|p| p.brew() == Some(Brew::Antidote)).expect("some potion is an antidote");
        assert_eq!(antidote.title(), "Antidote");
        assert_eq!(roll_item("Healing draught", 4).unwrap().brew(), Some(Brew::Healing));
        assert_eq!(roll_item("Bread", 4).unwrap().brew(), None);
    }

    #[test]
    fn potions_come_in_four_sizes_that_hold_less_or_more_and_stack_by_size_and_brew() {
        use crate::roll_item;
        let sizes: Vec<_> = ["Vial", "Small potion", "Potion", "Large potion"].map(|b| roll_item(b, 9).unwrap()).into_iter().collect();
        let limits: Vec<u32> = sizes.iter().map(|p| p.stack_limit()).collect();
        assert_eq!(limits, vec![5, 4, 3, 2], "our own stack sizes: the more it holds the fewer share a cell");
        let height = |p: &Item| p.shape.build().bounds().unwrap().1[1];
        assert!(sizes.windows(2).all(|w| height(&w[0]) < height(&w[1])), "each size stands taller");
        // Two small healing draughts stack, whatever seeds made their bottles; a small and a large do not.
        let (a, b) = (roll_item("Small healing draught", 1).unwrap(), roll_item("Small healing draught", 2).unwrap());
        assert!(a.can_stack_with(&b));
        assert!(!a.can_stack_with(&roll_item("Large healing draught", 1).unwrap()));
        assert_eq!(a.stack_limit(), 4);
        // Another brew of one size is another stack.
        let find = |brew| (0..300).map(|seed| roll_item("Small potion", seed).unwrap()).find(|p| p.brew() == Some(brew)).unwrap();
        let (mana, healing) = (find(Brew::Mana), find(Brew::Healing));
        assert!(!mana.can_stack_with(&healing) && mana.can_stack_with(&mana.clone()));
        // Old saves' bases are the medium size.
        assert_eq!(roll_item("Potion", 3).unwrap().potion_size(), Some(PotionSize::Medium));
        assert_eq!(roll_item("Healing draught", 3).unwrap().potion_size(), Some(PotionSize::Medium));
        assert_eq!(roll_item("Elixir of Vigil", 3).unwrap().potion_size(), None);
    }

    #[test]
    fn a_healing_and_a_mana_potion_of_one_size_and_seed_share_a_bottle() {
        use armory::families::food::Food;
        for (size, base) in [(PotionSize::Vial, "Healing vial"), (PotionSize::Large, "Large healing draught")] {
            let healing = roll_item(base, 12).unwrap();
            assert_eq!((healing.brew(), healing.potion_size(), healing_base(size)), (Some(Brew::Healing), Some(size), base));
            let armory::Item::Food(h) = &healing.shape else { panic!() };
            let mana = Food::roll_potion(12, size, Brew::Mana);
            assert_eq!((h.provision.size, h.provision.shape, h.provision.count), (mana.provision.size, mana.provision.shape, mana.provision.count));
        }
    }
}
