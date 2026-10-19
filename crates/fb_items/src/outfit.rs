//! Outfits: the plate's `OUTFITS` (`data/outfits.json`), the four classes and the two starting
//! looks, and rolling one into whole items (the plate's `preparedSet`,
//! `reference/forever-below-plate.html`).
//!
//! Each slot lists candidate bases, optionally led by the chance the slot is filled at all;
//! goods are carried and ground things lie at the wearer's feet. [`Outfit::roll`] picks a
//! base per slot and rolls it through [`roll_item`], so a rolled outfit is a pure function of
//! its key, the seed and the folk.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use procgen_rng::Mulberry32;
use serde::Deserialize;
use serde_json::Value;

use crate::{Item, Odds, Slot, roll_item_with};

/// One slot's candidates: the chance it is filled, and the bases to pick from (a base listed
/// twice is twice as likely).
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub chance: f32,
    pub bases: Vec<String>,
}

/// A row of `data/outfits.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct Outfit {
    /// Its key in the table (`"fighter"`, `"fallen"`).
    pub key: String,
    /// What the plate calls it (`"Fighter"`, `"Freshly fallen"`).
    pub label: String,
    /// Slots in the table's key order.
    pub slots: Vec<(Slot, Choice)>,
    pub goods: Vec<String>,
    pub ground: Vec<String>,
}

/// An outfit rolled into items.
#[derive(Clone, Debug, PartialEq)]
pub struct OutfitSet {
    /// Each item with the slot it goes in, held items included.
    pub worn: Vec<(Slot, Item)>,
    /// Carried goods.
    pub goods: Vec<Item>,
    /// Things lying at the wearer's feet.
    pub ground: Vec<Item>,
}

impl OutfitSet {
    /// The item in `slot`, if the roll filled it.
    pub fn item(&self, slot: Slot) -> Option<&Item> {
        self.worn.iter().find(|(s, _)| *s == slot).map(|(_, item)| item)
    }
}

#[derive(Deserialize)]
struct Row {
    label: String,
    slots: BTreeMap<String, Vec<Value>>,
    #[serde(default)]
    goods: Vec<String>,
    #[serde(default)]
    ground: Vec<String>,
}

fn choice(values: &[Value]) -> Choice {
    let (chance, rest) = match values.first() {
        Some(Value::Number(n)) => (n.as_f64().unwrap_or(1.0) as f32, &values[1..]),
        _ => (1.0, values),
    };
    Choice { chance, bases: rest.iter().filter_map(|v| v.as_str().map(String::from)).collect() }
}

static OUTFITS: LazyLock<Vec<Outfit>> = LazyLock::new(|| {
    let rows: BTreeMap<String, Row> =
        serde_json::from_str(include_str!("../../../data/outfits.json")).unwrap_or_else(|e| panic!("data/outfits.json: {e}"));
    rows.into_iter()
        .map(|(key, row)| Outfit {
            key,
            label: row.label,
            slots: row
                .slots
                .iter()
                .map(|(slot, values)| {
                    let slot: Slot = serde_json::from_value(Value::String(slot.clone())).unwrap_or_else(|e| panic!("data/outfits.json: {e}"));
                    (slot, choice(values))
                })
                .collect(),
            goods: row.goods,
            ground: row.ground,
        })
        .collect()
});

/// Every outfit in `data/outfits.json`, by key.
pub fn outfits() -> &'static [Outfit] {
    &OUTFITS
}

/// The outfit with `key`, or with that label, ignoring case.
pub fn outfit(key: &str) -> Option<&'static Outfit> {
    outfits().iter().find(|o| o.key.eq_ignore_ascii_case(key) || o.label.eq_ignore_ascii_case(key))
}

/// The plate's chances of a ring, an amulet and a charm on top of any outfit, and of a gnome's
/// pointed hat when the outfit gives no headgear (`preparedSet`).
const EXTRA_RING: f64 = 0.22;
const EXTRA_AMULET: f64 = 0.14;
const EXTRA_CHARM: f64 = 0.14;
const GNOME_HAT: f64 = 0.6;

/// How many arrows or bolts a bowman starts with: a full quiver.
pub const AMMUNITION_CARRIED: usize = 20;

impl Outfit {
    /// Rolls this outfit for a character of `folk` (a key of `data/races.json`) from `seed`.
    ///
    /// As the plate's `preparedSet`: each slot is filled with its chance, from a base picked
    /// among its candidates; a gnome with nothing on its head gets a pointed hat at 60%; and
    /// any outfit may add a ring (22%), an amulet (14%) and a charm (14%) to free slots. The
    /// stream is seeded from the seed and the outfit's key as the plate's is
    /// (`seed * 13 + key.length * 977 + key.charCodeAt(0)`); each item's own seed is drawn
    /// from that stream, where the plate rolled from its one global stream.
    pub fn roll(&self, seed: u32, folk: &str) -> OutfitSet {
        self.roll_with(seed, folk, Odds::default())
    }

    /// [`Outfit::roll`] with every item's rarity rolled against `odds`: the folk met at a
    /// depth wear [`Odds::kit`] for it.
    pub fn roll_with(&self, seed: u32, folk: &str, odds: Odds) -> OutfitSet {
        let first = self.key.bytes().next().unwrap_or(0) as u32;
        let mut rng = Mulberry32::new(seed.wrapping_mul(13).wrapping_add(self.key.len() as u32 * 977).wrapping_add(first));
        let draw = |rng: &mut Mulberry32, base: &str| roll_item_with(base, (rng.next_f64() * 4_294_967_296.0) as u32, odds);
        let mut worn: Vec<(Slot, Item)> = Vec::new();
        for (slot, choice) in &self.slots {
            if choice.bases.is_empty() || (choice.chance < 1.0 && rng.next_f64() > choice.chance as f64) {
                continue;
            }
            let base = rng.pick(&choice.bases).clone();
            if let Some(item) = draw(&mut rng, &base) {
                worn.push((*slot, item));
            }
        }
        let free = |worn: &[(Slot, Item)], slot: Slot| worn.iter().all(|(s, _)| *s != slot);
        let extras = [(Slot::Head, "Pointed hat", GNOME_HAT, folk == "gnome"), (Slot::Ring1, "Ring", EXTRA_RING, true), (Slot::Neck, "Amulet", EXTRA_AMULET, true), (Slot::Charm, "Charm", EXTRA_CHARM, true)];
        for (slot, base, chance, allowed) in extras {
            if allowed && free(&worn, slot) && rng.chance(chance)
                && let Some(item) = draw(&mut rng, base)
            {
                worn.push((slot, item));
            }
        }
        // A bow or crossbow comes with a quiver or bolt case slung and twenty of its
        // ammunition, carried as goods for the wearer to stow in it (SRD 5.1 p. 67-68).
        let mut ammunition = Vec::new();
        let shoots = worn.iter().find(|(s, _)| *s == Slot::Mainhand).and_then(|(_, i)| i.profile.ranged);
        if let Some(ranged) = shoots
            && free(&worn, Slot::Quiver)
        {
            let (holder, ammo) = if ranged.ammunition == armory::Family::Bolt { ("Bolt case", "Bolt") } else { ("Quiver", "Arrow") };
            if let Some(item) = draw(&mut rng, holder) {
                worn.push((Slot::Quiver, item));
                ammunition = (0..AMMUNITION_CARRIED).filter_map(|_| draw(&mut rng, ammo)).collect();
            }
        }
        let mut goods: Vec<Item> = self.goods.iter().filter_map(|base| draw(&mut rng, base)).collect();
        goods.extend(ammunition);
        let ground = self.ground.iter().filter_map(|base| draw(&mut rng, base)).collect();
        OutfitSet { worn, goods, ground }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rarity;

    #[test]
    fn the_first_levels_folk_wear_nothing_rare() {
        // 60 folk, the outfits taken in turn. The cap is what keeps rare gear off them; the
        // sample only has to reach every outfit and see some magic.
        let o = |seed: u32| &outfits()[seed as usize % outfits().len()];
        let mut magic = 0;
        for seed in 0..60 {
            for (_, item) in o(seed).roll_with(seed, "human", Odds::kit(1)).worn {
                assert!(item.rarity <= Rarity::Magic, "{} {seed}: {} is {:?}", o(seed).key, item.base, item.rarity);
                magic += usize::from(item.rarity == Rarity::Magic);
            }
        }
        assert!(magic > 0, "no magic gear on the first level's folk");
        let deep = (0..50).flat_map(|seed| o(seed).roll_with(seed, "human", Odds::kit(5)).worn).filter(|(_, i)| i.rarity >= Rarity::Rare).count();
        assert!(deep > 0, "nothing rare on folk at depth 5");
    }

    #[test]
    fn every_outfit_reads_from_the_table() {
        let keys: Vec<&str> = outfits().iter().map(|o| o.key.as_str()).collect();
        for key in ["fighter", "ranger", "mage", "cleric", "fallen", "scavenger"] {
            assert!(keys.contains(&key), "{key}");
        }
        let fallen = outfit("Freshly fallen").unwrap();
        assert_eq!(fallen.key, "fallen");
        assert_eq!(fallen.ground, vec!["Spare torch".to_string(), "Bread".to_string()]);
        let fighter = outfit("fighter").unwrap();
        let back = &fighter.slots.iter().find(|(s, _)| *s == Slot::Back).unwrap().1;
        assert_eq!((back.chance, back.bases.clone()), (0.7, vec!["Pack".to_string()]));
    }

    #[test]
    fn rolled_outfits_are_whole_items_from_their_candidates() {
        for outfit in outfits() {
            for seed in [1, 7, 1207] {
                let set = outfit.roll(seed, "human");
                assert_eq!(set, outfit.roll(seed, "human"), "{} {seed}", outfit.key);
                for (slot, item) in &set.worn {
                    match outfit.slots.iter().find(|(s, _)| s == slot) {
                        Some((_, choice)) => assert!(choice.bases.contains(&item.base), "{}: {} in {slot:?}", outfit.key, item.base),
                        None => assert!(matches!(slot, Slot::Ring1 | Slot::Neck | Slot::Charm | Slot::Head | Slot::Quiver), "{slot:?}"),
                    }
                }
                let shoots = set.item(Slot::Mainhand).is_some_and(|i| i.profile.ranged.is_some());
                assert_eq!(set.goods.len(), outfit.goods.len() + if shoots { AMMUNITION_CARRIED } else { 0 });
                assert_eq!(set.ground.len(), outfit.ground.len());
            }
        }
    }

    #[test]
    fn a_fighter_always_holds_a_weapon() {
        let fighter = outfit("fighter").unwrap();
        for seed in 0..30 {
            let set = fighter.roll(seed, "dwarf");
            assert!(set.item(Slot::Mainhand).is_some_and(|i| i.profile.stats.is_weapon()), "{seed}");
        }
    }

    #[test]
    fn a_ranger_with_a_bow_has_a_quiver_and_twenty_arrows() {
        let ranger = outfit("ranger").unwrap();
        let archers: Vec<_> = (0..40).map(|seed| ranger.roll(seed, "human")).filter(|s| s.item(Slot::Mainhand).is_some_and(|i| i.base == "Bow")).collect();
        assert!(!archers.is_empty());
        for set in archers {
            assert_eq!(set.item(Slot::Quiver).map(|i| i.base.as_str()), Some("Quiver"));
            assert_eq!(set.goods.iter().filter(|g| g.base == "Arrow").count(), 20);
        }
    }

    #[test]
    fn gnomes_often_get_a_pointed_hat() {
        let fallen = outfit("fallen").unwrap();
        let hats = (0..100).filter(|&s| fallen.roll(s, "gnome").item(Slot::Head).is_some()).count();
        assert!((40..80).contains(&hats), "{hats}");
        assert!((0..100).all(|s| fallen.roll(s, "human").item(Slot::Head).is_none()));
    }
}
