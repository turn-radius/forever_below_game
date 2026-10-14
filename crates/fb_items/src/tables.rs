//! The plate's item tables from `data/`, embedded at compile time and parsed once into typed
//! rows. `tools/extract_data.js` pulled them out of `reference/forever-below-plate.html`.
//!
//! - `bases.json`: the plate's own base items (`BASES`): slot, weight, footprint, tags, stats.
//! - `goods.json`: things carried but not worn (`GOODS`).
//! - `sab.json`: the Armory family each base is rolled from (`SAB`).
//! - `afx.json`: prefixes and suffixes (`AFX`).
//! - `families.json`: the plate's armory menu, bases grouped under a heading (`FAMILIES`).

use crate::dice::Dice;
use crate::footprint::{Footprint, Grid};
use crate::{Slot, Tag};
use armory::Family;
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

/// Prototype source not yet ported to Rust, kept verbatim with the extractor's `fn: ` prefix.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JsSource(pub String);

/// A row of `data/sab.json`: the Armory family a base is rolled from. The extractor lost the
/// `acc` filters' arguments (`inS('garment','style',['tunic'])` became `arr.includes(s[k][f])`),
/// so only the family is usable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SabEntry {
    #[serde(rename = "fam")]
    pub family: Family,
    #[serde(default, rename = "acc")]
    pub accept: Option<JsSource>,
    #[serde(default, rename = "tw")]
    pub tweak: Option<JsSource>,
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
    #[serde(default)]
    pub fx: Option<JsSource>,
    /// The card's line ("+2 warmth").
    #[serde(rename = "t")]
    pub text: String,
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
static SAB: LazyLock<BTreeMap<String, SabEntry>> = LazyLock::new(|| parse("sab.json", include_str!("../../../data/sab.json")));
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

/// `data/sab.json`, by base name.
pub fn sab() -> &'static BTreeMap<String, SabEntry> {
    &SAB
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
        assert_eq!(sab().len(), 42);
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

        assert_eq!(sab()["Chest"].family, Family::Chests);
        let warm = &affixes()[0];
        assert_eq!((warm.name.as_str(), warm.kind, warm.good, warm.on), ("Warm", AffixKind::Prefix, true, On::Tag(Tag::Cloth)));
        assert_eq!(affixes().iter().find(|a| a.name == "of Plenty").map(|a| a.on), Some(On::Any));
        assert_eq!(base_groups()[3].label, "Clubs and axes");
    }
}
