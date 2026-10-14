//! The plate's item tables from `data/`, embedded at compile time and parsed once into typed
//! rows. `tools/extract_data.js` pulled them out of `reference/forever-below-plate.html`.
//!
//! - `bases.json`: the plate's own base items (`BASES`): slot, weight, footprint, tags, stats.
//! - `goods.json`: things carried but not worn (`GOODS`).
//! - `afx.json`: prefixes and suffixes (`AFX`), each effect ported to typed [`Modifier`]s.
//! - `families.json`: the plate's armory menu, bases grouped under a heading (`FAMILIES`).

use crate::dice::Dice;
use crate::footprint::{Footprint, Grid};
use crate::modifier::Modifier;
use crate::stats::Stats;
use crate::{Slot, Tag};
use armory::cond::Condition;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
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
}

impl Affix {
    /// Whether it can roll on an item with `tags`.
    pub fn fits(&self, tags: &[Tag]) -> bool {
        self.on.matches(tags)
    }

    /// Applies every effect, in order.
    pub fn apply(&self, stats: &mut Stats, condition: &mut Condition) {
        for m in &self.fx {
            m.apply(stats, condition);
        }
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
static BASE_GROUPS: LazyLock<Vec<BaseGroup>> = LazyLock::new(|| parse("families.json", include_str!("../../../data/families.json")));

/// `data/bases.json`, by base name.
pub fn bases() -> &'static BTreeMap<String, BaseItem> {
    &BASES
}

/// `data/goods.json`, by name.
pub fn goods() -> &'static BTreeMap<String, Good> {
    &GOODS
}

/// `data/afx.json`, in the plate's order.
pub fn affixes() -> &'static [Affix] {
    &AFFIXES
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
        assert_eq!(affixes().len(), 24);
        assert_eq!(base_groups().len(), 25);
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
