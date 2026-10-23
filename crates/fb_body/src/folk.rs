//! The six folk, the denizens built on the same body, and their rows in `data/races.json` (the plate prototype's `RACES`), with
//! the colour tables in `data/eyes.json` and `data/hair*.json`.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use armory::Rgb;
use fb_denizens::DenizenBody;
use serde::{Deserialize, Serialize};

/// One of the six folk who delve, or one of the denizens of the abyss who share their body
/// ([`Folk::DENIZENS`]): a denizen is a race of the folk body, with the same skin, rig, gait and
/// fitting, and turns on the shared parts (horns, hooves, a tail, a muzzle) by its row.
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
    /// A bull-headed denizen (`fb_denizens::DenizenBody::Minotaur`).
    Minotaur,
}

impl Race {
    /// Whether the race stands on its toes with cloven hooves: it takes no boots and its cloth
    /// ends at the hock.
    pub fn hooved(&self) -> bool {
        self.legs.is_some()
    }
}

impl Folk {
    /// The six folk who delve. Factions and members roll from these; the denizens are
    /// [`Folk::DENIZENS`].
    pub const ALL: [Folk; 6] = [Folk::Human, Folk::Dwarf, Folk::Gnome, Folk::Goblin, Folk::Lizardfolk, Folk::Pigfolk];

    /// The denizens whose bodies are built on the folk body so far.
    pub const DENIZENS: [Folk; 1] = [Folk::Minotaur];

    /// Whether this is a denizen, not one of the six folk.
    pub fn is_denizen(self) -> bool {
        Folk::DENIZENS.contains(&self)
    }

    /// The denizen body this folk row stands for, if it is a denizen.
    pub fn denizen_body(self) -> Option<DenizenBody> {
        match self {
            Folk::Minotaur => Some(DenizenBody::Minotaur),
            _ => None,
        }
    }

    /// The folk row a denizen body is built as, if it is built yet.
    pub fn of_denizen(body: DenizenBody) -> Option<Folk> {
        match body {
            DenizenBody::Minotaur => Some(Folk::Minotaur),
            _ => None,
        }
    }

    /// The folk's key in `data/races.json`.
    pub fn key(self) -> &'static str {
        match self {
            Folk::Human => "human",
            Folk::Dwarf => "dwarf",
            Folk::Gnome => "gnome",
            Folk::Goblin => "goblin",
            Folk::Lizardfolk => "lizard",
            Folk::Pigfolk => "pig",
            Folk::Minotaur => "minotaur",
        }
    }

    /// Parses a key from `data/races.json`, or the folk's name ("lizardfolk", "pigfolk").
    pub fn parse(text: &str) -> Option<Folk> {
        let text = text.trim().to_ascii_lowercase();
        Folk::ALL.into_iter().chain(Folk::DENIZENS).find(|f| f.key() == text || f.race().label.to_ascii_lowercase() == text)
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
    /// Broad leaves set out from under the horns, drooping and swept back (minotaurs).
    Leaf,
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
    /// A bull's broad skull and long deep muzzle with its nose pad (minotaurs); see
    /// [`Race::muzzle`].
    Muzzle,
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
    /// Large amber eyes set at the sides of the head, with a horizontal pupil (a bull's).
    Bovine,
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
    /// A forelock over the brow and a shaggy mane down the neck and over the withers (a bull's).
    Mane,
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
    /// Denizen parts any race can turn on by giving its row the field: horns, legs that stand
    /// on the toes with cloven hooves, a jointed tail, a coat and a bull's muzzle. See
    /// [`crate::parts`].
    #[serde(default)]
    pub horns: Option<HornRow>,
    #[serde(default)]
    pub legs: Option<Digitigrade>,
    #[serde(default)]
    pub tail_shape: Option<TailRow>,
    #[serde(default)]
    pub muzzle: Option<MuzzleRow>,
    /// The body's hunch: how far the head is thrust forward of the neck's column, as a share of
    /// the body's scale, and the hump at the withers, as a share of the chest's depth.
    #[serde(default)]
    pub hunch: f32,
    #[serde(default)]
    pub hump: f32,
    /// Shoulders' breadth, over a human's at the same height.
    #[serde(default = "one")]
    pub shoulders: f32,
    /// Front-to-back depth of the chest and waist over a human's (a bull's deep barrel chest).
    #[serde(default = "one")]
    pub depth: f32,
    /// A hide with a coat: the skins are coat colours, and a seed may mark it with a blaze,
    /// socks or pied patches ([`crate::parts::Marking`]).
    #[serde(default)]
    pub coat: bool,
}

fn one() -> f32 {
    1.0
}

fn default_radius() -> Span {
    [0.3, 0.4]
}

fn default_curl() -> Span {
    [0.9, 2.0]
}

/// A range a seed rolls a value in.
pub type Span = [f32; 2];

/// The horns a race grows ([`crate::parts::HornSpec`] is the pair a body rolls from this).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct HornRow {
    /// Length along the curve, in half widths of the head.
    pub length: Span,
    /// 0 straight to 1 a deep hook; 0 close and forward to 1 wide out; 0 level to 1 lifted.
    pub curve: Span,
    pub spread: Span,
    pub rise: Span,
    /// How worn each tip is, 0 sharp to 1 a stub.
    pub wear: Span,
    /// Chance that one of the pair is broken off.
    pub broken: f32,
    /// Root radius in half widths of the head, by the horn's length share (short horns are
    /// slim, long ones heavy), and how far the heading turns toward the front along its length,
    /// radians, by the curve share (up to about 4.5 for a ram's spiral).
    #[serde(default = "default_radius")]
    pub radius: Span,
    #[serde(default = "default_curl")]
    pub curl: Span,
    /// Where on the skull's top (share along it) the horns root.
    pub root: f32,
}

/// Legs that stand on the toes: a short thigh sloping forward to the knee, a long shin back to a
/// high hock, and a pastern forward again to a cloven hoof. Lengths are shares of the hip's
/// height, so they scale with the body (a faun's goat legs are a shorter, lower `hock`).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Digitigrade {
    /// How high the hock is: the pastern drops this far to the floor.
    pub hock: f32,
    /// How far up from the hock to the hip the knee sits.
    pub knee_height: f32,
    /// How far the knee stands in front of the hip.
    pub knee_forward: f32,
    /// How far the hoof's coronet stands in front of the hock, as a share of the hock's height.
    pub pastern: f32,
    /// The hoof's breadth as a share of the leg's radius, and how far its toes part (0..1).
    pub hoof: f32,
    pub split: f32,
}

/// What a tail ends in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TailEnd {
    /// A tuft of hair (a bull's).
    Tuft,
    /// A spade-shaped blade (a devil's); not built yet.
    Spade,
}

/// A jointed tail: its bones carry it in the rig, so it swings.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct TailRow {
    pub bones: usize,
    pub end: TailEnd,
    /// Its length as a share of the hip's height, and its radius at the root as a share of the
    /// leg's.
    pub length: f32,
    pub radius: f32,
}

/// A muzzle: a long, deep snout lofted along the face from a broad skull (a bull's).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct MuzzleRow {
    /// How long the skull and muzzle are, in the head's depths.
    pub length: Span,
    /// Chance of a ring through the nose.
    pub ring: f32,
    /// How big the ears are.
    pub ears: Span,
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
