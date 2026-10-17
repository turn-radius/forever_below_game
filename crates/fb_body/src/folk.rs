//! The six folk and their rows in `data/races.json` (the plate prototype's `RACES`), with
//! the colour tables in `data/eyes.json` and `data/hair*.json`.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use armory::Rgb;
use serde::{Deserialize, Serialize};

/// One of the six folk who delve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Folk {
    Human,
    Dwarf,
    Gnome,
    Goblin,
    #[serde(rename = "lizard")]
    Lizardfolk,
    #[serde(rename = "pig")]
    Pigfolk,
}

impl Folk {
    pub const ALL: [Folk; 6] = [Folk::Human, Folk::Dwarf, Folk::Gnome, Folk::Goblin, Folk::Lizardfolk, Folk::Pigfolk];

    /// The folk's key in `data/races.json`.
    pub fn key(self) -> &'static str {
        match self {
            Folk::Human => "human",
            Folk::Dwarf => "dwarf",
            Folk::Gnome => "gnome",
            Folk::Goblin => "goblin",
            Folk::Lizardfolk => "lizard",
            Folk::Pigfolk => "pig",
        }
    }

    /// Parses a key from `data/races.json`, or the folk's name ("lizardfolk", "pigfolk").
    pub fn parse(text: &str) -> Option<Folk> {
        let text = text.trim().to_ascii_lowercase();
        Folk::ALL.into_iter().find(|f| f.key() == text || f.race().label.to_ascii_lowercase() == text)
    }

    /// The folk's row in `data/races.json`.
    pub fn race(self) -> &'static Race {
        &RACES[&self]
    }
}

/// Ear shapes (`RACES[*].ears`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ears {
    Round,
    /// Short and pointed, rising (gnomes).
    Pointed,
    /// Long blades swept out to the side (goblins).
    Long,
    /// Folded forward and down (pigfolk).
    Flop,
    None,
}

/// Nose shapes (`RACES[*].nose`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Nose {
    Human,
    Broad,
    /// A round bulb (gnomes).
    Bulb,
    /// A long pointed nose (goblins).
    Long,
    /// A reptile's snout and jaws (lizardfolk).
    Snout,
    /// A pig's snout disc (pigfolk).
    Pig,
}

/// Eye kinds (`RACES[*].eyes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EyeKind {
    /// White, a coloured iris from `data/eyes.json`, a pupil.
    Human,
    /// Yellow all over with a slit pupil (goblins).
    Yellow,
    /// Large amber eyes with a slit pupil under a brow ridge (lizardfolk).
    Amber,
    /// Small dark beads (pigfolk).
    Small,
}

/// Hair styles (`RACES[*].hairs`); labels as the plate's hair menu gives them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HairStyle {
    Crop,
    Shaggy,
    Long,
    Ponytail,
    Topknot,
    Bald,
    /// A ridge of spikes front to back (the plate's "Ridge").
    Mohawk,
    /// A lizard's frill of spines (the plate's "Frill").
    Crest,
}

impl HairStyle {
    pub const ALL: [HairStyle; 8] =
        [HairStyle::Crop, HairStyle::Shaggy, HairStyle::Long, HairStyle::Ponytail, HairStyle::Topknot, HairStyle::Bald, HairStyle::Mohawk, HairStyle::Crest];
}

/// Beards (the plate's `C.beard`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Beard {
    None,
    Stubble,
    Short,
    Full,
    Long,
    Forked,
}

impl Beard {
    pub const ALL: [Beard; 6] = [Beard::None, Beard::Stubble, Beard::Short, Beard::Full, Beard::Long, Beard::Forked];
}

/// A `#rrggbb` colour from the data tables.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hex(pub Rgb);

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let v = u32::from_str_radix(s.trim_start_matches('#'), 16).map_err(serde::de::Error::custom)?;
        Ok(Hex(Rgb::hex(v)))
    }
}

/// One folk's proportions and looks: a row of `data/races.json`. Lengths are multiples of a
/// human's unless noted; units are 10 cm.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Race {
    pub label: String,
    /// Typical height.
    pub h: f32,
    /// Head radius before scaling with height, and its width, height and depth factors.
    #[serde(rename = "headR")]
    pub head_r: f32,
    pub hw: f32,
    pub hh: f32,
    pub hd: f32,
    /// 0..=1: how much the jaw narrows, how far the chin juts, how flat the face is.
    pub jaw: f32,
    pub chin: f32,
    pub flat: f32,
    /// Neck length and thickness.
    pub neck: f32,
    #[serde(rename = "neckR")]
    pub neck_r: f32,
    /// Hip height as a share of height.
    pub leg: f32,
    pub girth: f32,
    /// Limb thickness, arm length and foot length.
    pub limb: f32,
    pub arm: f32,
    pub foot: f32,
    pub belly: f32,
    pub skins: Vec<Hex>,
    pub hairs: Vec<HairStyle>,
    /// Chance of a beard.
    #[serde(rename = "beardP")]
    pub beard_chance: f32,
    pub ears: Ears,
    pub nose: Nose,
    pub eyes: EyeKind,
    /// Strength, dexterity, constitution, intelligence, wisdom before rolling.
    pub stats: [i32; 5],
    /// Darkvision, in feet.
    pub dv: u32,
    /// 1 for a tail (lizardfolk).
    #[serde(default)]
    pub tail: u32,
}

fn parse<T: for<'de> Deserialize<'de>>(name: &str, json: &str) -> T {
    serde_json::from_str(json).unwrap_or_else(|e| panic!("data/{name}: {e}"))
}

fn colours(name: &str, json: &str) -> Vec<Rgb> {
    parse::<Vec<Hex>>(name, json).into_iter().map(|h| h.0).collect()
}

static RACES: LazyLock<BTreeMap<Folk, Race>> = LazyLock::new(|| parse("races.json", include_str!("../../../data/races.json")));
static EYES: LazyLock<Vec<Rgb>> = LazyLock::new(|| colours("eyes.json", include_str!("../../../data/eyes.json")));
static HAIR: LazyLock<Vec<Rgb>> = LazyLock::new(|| colours("hair.json", include_str!("../../../data/hair.json")));
static HAIR_GOBLIN: LazyLock<Vec<Rgb>> = LazyLock::new(|| colours("hair_gob.json", include_str!("../../../data/hair_gob.json")));
static HAIR_LIZARD: LazyLock<Vec<Rgb>> = LazyLock::new(|| colours("hair_liz.json", include_str!("../../../data/hair_liz.json")));

/// Iris colours (`data/eyes.json`).
pub fn eye_colours() -> &'static [Rgb] {
    &EYES
}

/// The hair colours a folk draws from: `data/hair_liz.json` for lizardfolk frills,
/// `data/hair_gob.json` for goblins, `data/hair.json` for everyone else.
pub fn hair_colours(folk: Folk) -> &'static [Rgb] {
    match folk {
        Folk::Lizardfolk => &HAIR_LIZARD,
        Folk::Goblin => &HAIR_GOBLIN,
        _ => &HAIR,
    }
}

/// Grey and white hair, which other folk turn with age (the plate's `#9a9690`, `#d6d1c6`).
pub const GREY_HAIR: [Rgb; 2] = [Rgb::hex(0x9a9690), Rgb::hex(0xd6d1c6)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_folk_has_a_row() {
        for folk in Folk::ALL {
            let r = folk.race();
            assert!(r.h > 8.0 && !r.skins.is_empty() && !r.hairs.is_empty(), "{folk:?}");
            assert_eq!(Folk::parse(folk.key()), Some(folk));
        }
        assert_eq!(Folk::parse("Lizardfolk"), Some(Folk::Lizardfolk));
        assert_eq!(Folk::Human.race().h, 17.2);
        assert_eq!(Folk::Lizardfolk.race().tail, 1);
        assert_eq!(eye_colours().len(), 5);
        assert!(!hair_colours(Folk::Goblin).is_empty());
    }
}
