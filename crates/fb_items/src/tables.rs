//! The plate's item tables from `data/`, embedded at compile time and parsed once into typed
//! rows. `tools/extract_data.js` pulled them out of `reference/forever-below-plate.html`.
//!
//! - `bases.json`: the plate's own base items (`BASES`): slot, weight, footprint, tags, stats.
//! - `goods.json`: things carried but not worn (`GOODS`).
//! - `afx.json`: prefixes and suffixes (`AFX`), each effect ported to typed [`Modifier`]s.
//! - `food_desc.json`: the card line for each food style (`FOOD_DESC`).
//! - `syl.json`, `factions.json`, `places.json`: name syllables per folk, and the factions
//!   and places legendary histories name (`SYL`, `FACTIONS`, `PLACES`).
//! - `families.json`: the plate's armory menu, bases grouped under a heading (`FAMILIES`).
//! - `slots.json`: where each equipment slot sits round the doll (`SLOTS`).

use crate::dice::Dice;
use crate::footprint::{Footprint, Grid};
use crate::modifier::Modifier;
use crate::stats::Stats;
use crate::{Rarity, Slot, Tag};
use armory::cond::Condition;
use armory::families::food::FoodStyle;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

/// A base from `data/bases.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BaseItem {
    pub slot: Slot,
    /// Pounds.
    #[serde(rename = "wt")]
    pub weight: f32,
    #[serde(rename = "fp")]
    pub footprint: Footprint,
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub warmth: f32,
    #[serde(default, rename = "ac")]
    pub armour: u8,
    #[serde(default, rename = "dmg")]
    pub damage: Option<Dice>,
    /// How far its light reaches, in feet.
    #[serde(default)]
    pub light: f32,
    /// The material words the plate picks from for the name ("Linen", "Oak").
    #[serde(rename = "mats")]
    pub materials: Vec<String>,
    /// The grid it opens into, for a container.
    #[serde(default)]
    pub holds: Option<Grid>,
}

/// A good from `data/goods.json`: carried, not worn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Good {
    #[serde(rename = "fp")]
    pub footprint: Footprint,
    /// Pounds.
    #[serde(rename = "wt")]
    pub weight: f32,
    /// The card's line ("Food · one day").
    #[serde(rename = "desc")]
    pub description: String,
    /// How many share one cell.
    #[serde(default = "one")]
    pub stack: u8,
    /// Something to eat.
    #[serde(default, deserialize_with = "flag")]
    pub food: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AffixKind {
    #[serde(rename = "pre")]
    Prefix,
    #[serde(rename = "suf")]
    Suffix,
}

/// Which items an affix can roll on: any item, or items carrying one tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum On {
    Any,
    #[serde(untagged)]
    Tag(Tag),
}

impl On {
    pub fn matches(self, tags: &[Tag]) -> bool {
        match self {
            On::Any => true,
            On::Tag(t) => tags.contains(&t),
        }
    }
}

/// A prefix or suffix from `data/afx.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Affix {
    #[serde(rename = "n")]
    pub name: String,
    #[serde(rename = "k")]
    pub kind: AffixKind,
    /// A good affix rather than a flaw.
    #[serde(rename = "g", deserialize_with = "flag")]
    pub good: bool,
    pub on: On,
    /// What it does, ported from the plate's `fx` source, its card text and `dressState`.
    pub fx: Vec<Modifier>,
    /// The card's line ("+2 warmth").
    #[serde(rename = "t")]
    pub text: String,
    /// Its kind of effect ("speed", "armour"): no item carries two affixes of one group, so a
    /// stronger tier does not stack with a weaker. With none, it is only kept from itself.
    #[serde(default, rename = "grp")]
    pub group: Option<String>,
    /// The least rarity it rolls on, so the stronger tiers come only on rarer items.
    #[serde(default = "least_rarity", rename = "r")]
    pub least: Rarity,
    /// The slots it rolls on, as well as its `on` tag; with none, any slot.
    #[serde(default, rename = "at")]
    pub slots: Vec<Slot>,
}

fn least_rarity() -> Rarity {
    Rarity::Common
}

impl Affix {
    /// Whether it can roll on an item with `tags`, whatever its slot.
    pub fn fits(&self, tags: &[Tag]) -> bool {
        self.on.matches(tags)
    }

    /// Whether it can roll on an item with `tags` worn in `slot` (`None` for a thing not worn):
    /// its tag, and its slots when it names any. A ring rolled for either ring slot is
    /// [`Slot::Ring1`].
    pub fn fits_item(&self, tags: &[Tag], slot: Option<Slot>) -> bool {
        self.fits(tags) && (self.slots.is_empty() || slot.is_some_and(|s| self.slots.contains(&if s == Slot::Ring2 { Slot::Ring1 } else { s })))
    }

    /// Whether it may sit on one item with `other`: not the same affix, nor one of its group.
    pub fn stacks_with(&self, other: &Affix) -> bool {
        self.name != other.name && (self.group.is_none() || self.group != other.group)
    }

    /// Whether it rolls: some effect of it changes a stat the game reads ([`Modifier::in_play`]).
    /// The rest are held back until their systems exist (`crate::rarity::pick_affix`).
    pub fn in_play(&self) -> bool {
        self.fx.iter().any(Modifier::in_play)
    }

    /// Applies every effect, in order.
    pub fn apply(&self, stats: &mut Stats, condition: &mut Condition) {
        self.apply_to_stats(stats);
        self.apply_to_condition(condition);
    }

    pub fn apply_to_stats(&self, stats: &mut Stats) {
        self.fx.iter().for_each(|m| m.apply_to_stats(stats));
    }

    /// The wear it leaves on the shape (a flaw's rust, fray and rot).
    pub fn apply_to_condition(&self, condition: &mut Condition) {
        self.fx.iter().for_each(|m| m.apply_to_condition(condition));
    }
}

/// A heading of the plate's armory menu and the bases under it (a row of
/// `data/families.json`). Not an [`armory::Family`]: "Clubs and axes" spans two.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "(String, Vec<String>)", into = "(String, Vec<String>)")]
pub struct BaseGroup {
    pub label: String,
    pub bases: Vec<String>,
}

impl From<(String, Vec<String>)> for BaseGroup {
    fn from((label, bases): (String, Vec<String>)) -> Self {
        BaseGroup { label, bases }
    }
}

impl From<BaseGroup> for (String, Vec<String>) {
    fn from(g: BaseGroup) -> Self {
        (g.label, g.bases)
    }
}

/// How big a slot is drawn round the doll.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlotSize {
    Big,
    Small,
}

/// Which side of the doll a slot is on, and so which edge its position is measured from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotSide {
    #[serde(rename = "L")]
    Left,
    #[serde(rename = "R")]
    Right,
}

/// A row of `data/slots.json`: a slot, its label, size and side, and its place round the
/// doll as percentages of the doll's frame, from the left edge (or the right, for a right
/// side slot) and from the top.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "(Slot, String, SlotSize, SlotSide, f32, f32)")]
pub struct SlotPlace {
    pub slot: Slot,
    pub label: String,
    pub size: SlotSize,
    pub side: SlotSide,
    pub x_percent: f32,
    pub y_percent: f32,
}

impl From<(Slot, String, SlotSize, SlotSide, f32, f32)> for SlotPlace {
    fn from((slot, label, size, side, x_percent, y_percent): (Slot, String, SlotSize, SlotSide, f32, f32)) -> Self {
        SlotPlace { slot, label, size, side, x_percent, y_percent }
    }
}

fn one() -> u8 {
    1
}

/// The plate writes flags as `0`/`1`; accept those or a JSON boolean.
fn flag<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Flag {
        Bool(bool),
        Int(u8),
    }
    Ok(match Flag::deserialize(d)? {
        Flag::Bool(b) => b,
        Flag::Int(i) => i != 0,
    })
}

fn parse<T: for<'de> Deserialize<'de>>(name: &str, json: &str) -> T {
    serde_json::from_str(json).unwrap_or_else(|e| panic!("data/{name}: {e}"))
}

static BASES: LazyLock<BTreeMap<String, BaseItem>> = LazyLock::new(|| parse("bases.json", include_str!("../../../data/bases.json")));
static GOODS: LazyLock<BTreeMap<String, Good>> = LazyLock::new(|| parse("goods.json", include_str!("../../../data/goods.json")));
static AFFIXES: LazyLock<Vec<Affix>> = LazyLock::new(|| parse("afx.json", include_str!("../../../data/afx.json")));
static SLOTS: LazyLock<Vec<SlotPlace>> = LazyLock::new(|| parse("slots.json", include_str!("../../../data/slots.json")));
static BASE_GROUPS: LazyLock<Vec<BaseGroup>> = LazyLock::new(|| parse("families.json", include_str!("../../../data/families.json")));

/// `data/bases.json`, by base name.
pub fn bases() -> &'static BTreeMap<String, BaseItem> {
    &BASES
}

static FOOD_DESCRIPTIONS: LazyLock<HashMap<FoodStyle, String>> = LazyLock::new(|| parse("food_desc.json", include_str!("../../../data/food_desc.json")));

/// `data/food_desc.json`, by food style.
pub fn food_descriptions() -> &'static HashMap<FoodStyle, String> {
    &FOOD_DESCRIPTIONS
}

/// A folk's first and second name syllables.
pub type Syllables = (Vec<String>, Vec<String>);

static SYLLABLES: LazyLock<BTreeMap<String, Syllables>> = LazyLock::new(|| parse("syl.json", include_str!("../../../data/syl.json")));
static FACTIONS: LazyLock<Vec<String>> = LazyLock::new(|| parse("factions.json", include_str!("../../../data/factions.json")));
static PLACES: LazyLock<Vec<String>> = LazyLock::new(|| parse("places.json", include_str!("../../../data/places.json")));

/// `data/syl.json`: for each folk, first and second name syllables.
pub fn syllables() -> &'static BTreeMap<String, Syllables> {
    &SYLLABLES
}

/// `data/factions.json`.
pub fn factions() -> &'static [String] {
    &FACTIONS
}

/// `data/places.json`.
pub fn places() -> &'static [String] {
    &PLACES
}

/// `data/goods.json`, by name.
pub fn goods() -> &'static BTreeMap<String, Good> {
    &GOODS
}

/// `data/afx.json`, in the plate's order.
pub fn affixes() -> &'static [Affix] {
    &AFFIXES
}

/// `data/slots.json`: every equipment slot and where it sits round the doll, in the plate's
/// order.
pub fn slot_places() -> &'static [SlotPlace] {
    &SLOTS
}

/// `data/families.json`, in menu order.
pub fn base_groups() -> &'static [BaseGroup] {
    &BASE_GROUPS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_parses_with_its_entries() {
        assert_eq!(bases().len(), 13);
        assert_eq!(goods().len(), 13);
        assert_eq!(affixes().len(), 52);
        assert_eq!(slot_places().len(), 16);
        let head = &slot_places()[0];
        assert_eq!((head.slot, head.label.as_str(), head.size, head.side, head.x_percent, head.y_percent), (Slot::Head, "Head", SlotSize::Big, SlotSide::Left, 4.0, 5.0));
        let charm = slot_places().iter().find(|p| p.slot == Slot::Charm).unwrap();
        assert_eq!((charm.slot, charm.side), (Slot::Charm, SlotSide::Right));
        // The hip sits beside the belt, on its side of the doll and level with it.
        let (belt, hip) = (slot_places().iter().find(|p| p.slot == Slot::Belt).unwrap(), slot_places().iter().find(|p| p.slot == Slot::Hip).unwrap());
        assert_eq!((hip.side, hip.y_percent, hip.size), (belt.side, belt.y_percent, belt.size));
        assert_eq!(base_groups().len(), 25);
        assert_eq!(food_descriptions().len(), 12);
        assert_eq!((syllables().len(), factions().len(), places().len()), (6, 7, 9));
        assert_eq!(base_groups().iter().map(|g| g.bases.len()).sum::<usize>(), 63);
    }

    #[test]
    fn rows_read_their_fields() {
        let club = &bases()["Club"];
        assert_eq!(club.slot, Slot::Mainhand);
        assert_eq!(club.damage, Some(Dice::new(1, 4)));
        assert_eq!(club.footprint, Footprint { cols: 1, rows: 3 });
        assert_eq!(club.tags, [Tag::Weapon, Tag::Wood]);
        let belt = &bases()["Belt"];
        assert_eq!(belt.holds.as_ref().map(|g| (g.cols, g.rows)), Some((4, 2)));
        assert_eq!(bases()["Lantern"].light, 30.0);

        let bread = &goods()["Bread"];
        assert!(bread.food);
        assert_eq!(bread.stack, 3);
        assert!(!goods()["Rope"].food);
        assert_eq!(goods()["Rope"].stack, 1);

        let warm = &affixes()[0];
        assert_eq!((warm.name.as_str(), warm.kind, warm.good, warm.on), ("Warm", AffixKind::Prefix, true, On::Tag(Tag::Cloth)));
        assert_eq!(affixes().iter().find(|a| a.name == "of Plenty").map(|a| a.on), Some(On::Any));
        assert_eq!(base_groups()[3].label, "Clubs and axes");
    }

    use crate::modifier::ConditionSlider;
    use crate::stats::Stat;

    /// A mid-range item to apply affixes to: a weapon or not, pristine.
    fn baseline(weapon: bool) -> (Stats, Condition) {
        let stats = Stats {
            weight: 4.0,
            armour: 2.0,
            damage: weapon.then_some(Dice::new(1, 6)),
            warmth: 2.0,
            light: 20.0,
            ..Stats::default()
        };
        (stats, Condition::default())
    }

    /// The stats and condition sliders that differ between two states.
    fn changes(before: &(Stats, Condition), after: &(Stats, Condition)) -> (Vec<Stat>, Vec<ConditionSlider>) {
        let stats = Stat::ALL.into_iter().filter(|&s| before.0.get(s) != after.0.get(s)).collect();
        let sliders = ConditionSlider::ALL.into_iter().filter(|&c| c.get(&before.1) != c.get(&after.1)).collect();
        (stats, sliders)
    }

    fn named(m: &Modifier) -> (Vec<Stat>, Vec<ConditionSlider>) {
        match *m {
            Modifier::Add { stat, .. } | Modifier::Scale { stat, .. } => (vec![stat], vec![]),
            Modifier::DamageOrArmour { .. } => (vec![Stat::DamageBonus, Stat::Armour], vec![]),
            Modifier::Condition { slider, .. } => (vec![], vec![slider]),
        }
    }

    #[test]
    fn every_affix_has_typed_effects() {
        assert!(!include_str!("../../../data/afx.json").contains("fn:"), "JS source left in data/afx.json");
        for a in affixes() {
            assert!(!a.fx.is_empty(), "{} does nothing", a.name);
        }
    }

    #[test]
    fn an_effect_changes_only_what_it_names() {
        for a in affixes() {
            for m in &a.fx {
                let (stats, sliders) = named(m);
                let mut changed_any = false;
                for weapon in [false, true] {
                    let before = baseline(weapon);
                    let mut after = before;
                    m.apply(&mut after.0, &mut after.1);
                    let (cs, cc) = changes(&before, &after);
                    assert!(cs.iter().all(|s| stats.contains(s)), "{}: {m:?} changed {cs:?}", a.name);
                    assert!(cc.iter().all(|c| sliders.contains(c)), "{}: {m:?} changed {cc:?}", a.name);
                    assert_eq!(after.0.damage, before.0.damage);
                    changed_any |= !cs.is_empty() || !cc.is_empty();
                }
                assert!(changed_any, "{}: {m:?} changes nothing", a.name);
            }
        }
    }

    #[test]
    fn flaws_worsen_and_good_affixes_improve() {
        for a in affixes() {
            for m in &a.fx {
                assert_eq!(m.is_improvement(), a.good, "{}: {m:?}", a.name);
            }
            for weapon in [false, true] {
                let before = baseline(weapon);
                let mut after = before;
                a.apply(&mut after.0, &mut after.1);
                let (stats, sliders) = changes(&before, &after);
                for s in stats {
                    let rose = after.0.get(s) > before.0.get(s);
                    assert_eq!(rose == s.higher_is_better(), a.good, "{}: {s:?} moved the wrong way", a.name);
                }
                if a.good {
                    assert!(sliders.is_empty(), "{} wears the item", a.name);
                }
                for c in sliders {
                    assert!(c.get(&after.1) > c.get(&before.1), "{}: {c:?} fell", a.name);
                }
            }
        }
    }

    #[test]
    fn each_new_affix_changes_its_stat_by_its_amount() {
        let cases: &[(&str, Stat, f32)] = &[
            ("Fleet", Stat::Speed, 1.1),
            ("of Haste", Stat::Speed, 1.2),
            ("Sluggish", Stat::Speed, 0.9),
            ("Vicious", Stat::DamageBonus, 2.0),
            ("of Carnage", Stat::DamageBonus, 3.0),
            ("Accurate", Stat::ToHit, 2.0),
            ("of Precision", Stat::ToHit, 3.0),
            ("Reinforced", Stat::Armour, 4.0),
            ("of the Bulwark", Stat::Armour, 5.0),
            ("of Deflection", Stat::BlockChance, 0.1),
            ("of the Rampart", Stat::BlockChance, 0.2),
            ("of the Owl", Stat::Darkvision, 30.0),
            ("of the Bat", Stat::Darkvision, 60.0),
            ("of Might", Stat::Strength, 1.0),
            ("of the Giant", Stat::Strength, 2.0),
            ("Feeble", Stat::Strength, -1.0),
            ("of Deftness", Stat::Dexterity, 1.0),
            ("of the Fox", Stat::Dexterity, 2.0),
            ("Clumsy", Stat::Dexterity, -1.0),
            ("of Vigour", Stat::Constitution, 1.0),
            ("of the Bear", Stat::Constitution, 2.0),
            ("Frail", Stat::Constitution, -1.0),
            ("of Wit", Stat::Intelligence, 1.0),
            ("of the Sage", Stat::Intelligence, 2.0),
            ("of Insight", Stat::Wisdom, 1.0),
            ("of the Seer", Stat::Wisdom, 2.0),
            ("of Presence", Stat::Charisma, 1.0),
            ("of the Silver Tongue", Stat::Charisma, 2.0),
        ];
        for &(name, stat, want) in cases {
            let a = affixes().iter().find(|a| a.name == name).unwrap_or_else(|| panic!("{name}"));
            let (mut s, mut c) = baseline(stat == Stat::DamageBonus || stat == Stat::ToHit);
            a.apply(&mut s, &mut c);
            assert!((s.get(stat) - want).abs() < 1e-6, "{name}: {stat:?} is {}", s.get(stat));
        }
    }

    #[test]
    fn affixes_fit_their_tags() {
        let warm = affixes().iter().find(|a| a.name == "Warm").unwrap();
        assert!(warm.fits(&[Tag::Cloth, Tag::Wear]));
        assert!(!warm.fits(&[Tag::Metal, Tag::Weapon]));
        let ox = affixes().iter().find(|a| a.name == "of the Ox").unwrap();
        assert!(ox.fits(&[]));
    }

    #[test]
    fn the_plates_effects_are_kept() {
        let apply = |name: &str, weapon: bool| {
            let (mut s, mut c) = baseline(weapon);
            affixes().iter().find(|a| a.name == name).unwrap().apply(&mut s, &mut c);
            (s, c)
        };
        assert_eq!(apply("Warm", false).0.warmth, 4.0);
        assert!((apply("Featherlight", false).0.weight - 2.4).abs() < 1e-6);
        assert_eq!(apply("Moth-eaten", false).0.warmth, 1.0);
        assert_eq!(apply("Smoky", false).0.light, 15.0);
        assert_eq!(apply("of the Ox", false).0.carry, 25.0);
        let (sword, rust) = apply("Rusted", true);
        assert_eq!((sword.damage_bonus, sword.armour, rust.rust, rust.wear), (-1.0, 2.0, 1.5, 0.75));
        assert_eq!(apply("Rusted", false).0.armour, 1.0);
    }
}
