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
    /// A goat-legged denizen with a human's upper body (`fb_denizens::DenizenBody::Faun`).
    Faun,
    /// A one-eyed giant of the abyss. Not one of the six who delve, so it is not in
    /// [`Folk::ALL`]; see [`Folk::DENIZENS`].
    Cyclops,
    /// A lean demon with bat wings (`fb_denizens::DenizenBody::WingedDemon`).
    #[serde(rename = "winged_demon")]
    WingedDemon,
    /// A big, brutish demon with curling horns and no wings (`fb_denizens::DenizenBody::HornedDemon`).
    #[serde(rename = "horned_demon")]
    HornedDemon,
    /// A slender demon with small horns and wings (`fb_denizens::DenizenBody::Succubus`).
    Succubus,
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
    pub const DENIZENS: &'static [Folk] = &[Folk::Minotaur, Folk::Faun, Folk::Cyclops, Folk::WingedDemon, Folk::HornedDemon, Folk::Succubus];

    /// The folk and the denizens built on their body, for tests that sweep every body.
    pub fn every() -> Vec<Folk> {
        Folk::ALL.into_iter().chain(Folk::DENIZENS.iter().copied()).collect()
    }

    /// The folk and the denizens whose fitting the sweeps in this crate cover: the six folk and
    /// the cyclops (the minotaur's parts have their own tests, in `denizen_tests`).
    pub fn fitted() -> Vec<Folk> {
        Folk::ALL.into_iter().chain([Folk::Cyclops]).collect()
    }

    /// Whether this is a denizen, not one of the six folk.
    pub fn is_denizen(self) -> bool {
        Folk::DENIZENS.contains(&self)
    }

    /// The denizen body this folk row stands for, if it is a denizen.
    pub fn denizen_body(self) -> Option<DenizenBody> {
        match self {
            Folk::Minotaur => Some(DenizenBody::Minotaur),
            Folk::Faun => Some(DenizenBody::Faun),
            Folk::Cyclops => Some(DenizenBody::Cyclops),
            Folk::WingedDemon => Some(DenizenBody::WingedDemon),
            Folk::HornedDemon => Some(DenizenBody::HornedDemon),
            Folk::Succubus => Some(DenizenBody::Succubus),
            _ => None,
        }
    }

    /// The folk row a denizen body is built as, if it is built yet.
    pub fn of_denizen(body: DenizenBody) -> Option<Folk> {
        match body {
            DenizenBody::Minotaur => Some(Folk::Minotaur),
            DenizenBody::Faun => Some(Folk::Faun),
            DenizenBody::Cyclops => Some(Folk::Cyclops),
            DenizenBody::WingedDemon => Some(Folk::WingedDemon),
            DenizenBody::HornedDemon => Some(Folk::HornedDemon),
            DenizenBody::Succubus => Some(Folk::Succubus),
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
            Folk::Faun => "faun",
            Folk::Cyclops => "cyclops",
            Folk::WingedDemon => "winged_demon",
            Folk::HornedDemon => "horned_demon",
            Folk::Succubus => "succubus",
        }
    }

    /// Parses a key from `data/races.json`, or the folk's name ("lizardfolk", "pigfolk").
    pub fn parse(text: &str) -> Option<Folk> {
        let text = text.trim().to_ascii_lowercase().replace(['-', '_'], " ");
        Folk::every().into_iter().find(|f| f.key().replace('_', " ") == text || f.race().label.to_ascii_lowercase() == text)
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
    /// One large eye, centred under one heavy brow (cyclopes): a white, a seeded iris from
    /// `data/eyes.json` and a round pupil, under a drooping lid.
    One,
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
    /// Tight curls all over the crown, each a small lock of its own (fauns).
    Curly,
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
    /// Bat-like wings on the back ([`crate::wings`]).
    #[serde(default)]
    pub wings: Option<WingRow>,
    /// A heavier brow ridge over the eyes, added to the usual (a horned demon's).
    #[serde(default)]
    pub brow: f32,
    /// The SRD 5.1 stat block the race's scores come from, if it has one.
    #[serde(default)]
    pub srd: Option<Srd>,
    /// Hands end in claws, not nails.
    #[serde(default)]
    pub claws: bool,
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
    /// How far the spine bends forward at rest, radians: the chest and shoulders lean over the
    /// hips, the head lifts back to look ahead and the arms still hang straight down. 0 for folk.
    /// Applied in the rig's rest pose by the gait (`Gait::pose`), so the skin and every worn layer
    /// lean with it through their weights.
    #[serde(default)]
    pub stoop: f32,
    /// Front-to-back depth of the chest and waist over a human's (a bull's deep barrel chest).
    #[serde(default = "one")]
    pub depth: f32,
    /// A hide with a coat: the skins are coat colours, and a seed may mark it with a blaze,
    /// socks or pied patches ([`crate::parts::Marking`]).
    #[serde(default)]
    pub coat: bool,
    /// Fur on the legs only (a faun's): the skins stay a human's, the legs and the pasterns are
    /// a fur colour a seed picks, and shaggy locks hang from the thighs and shanks.
    #[serde(default)]
    pub fur: Option<FurRow>,
    /// Added to the body's muscle: a heavy-boned race is brawny at any roll (0 for the folk).
    #[serde(default)]
    pub brawn: f32,
    /// Toes drawn on a bare foot (0 for the folk, whose feet are bare shapes).
    #[serde(default)]
    pub toes: u32,
    /// A denizen's SRD stat block (the folk who delve have none: they are rolled as characters).
    #[serde(default)]
    pub block: Option<StatBlock>,
}

/// Fur that grows on a race's legs only.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct FurRow {
    /// The colours a seed picks one from.
    pub colours: Vec<Hex>,
    /// How long and thick the locks are, 1 for a faun's.
    pub shag: f32,
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

/// Where a denizen's stat block is from: the SRD 5.1 creature and page, and the numbers the combat
/// task uses (the scores are the row's `stats`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Srd {
    pub creature: String,
    pub page: u32,
    pub cr: String,
    pub ac: u32,
    pub hp: u32,
    /// What is changed from the block, if anything.
    #[serde(default)]
    pub note: String,
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
    /// A spade-shaped blade (a devil's).
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
    /// How far the tail is raised from hanging, radians: 0 hangs, about 2 stands up and curls
    /// back (a goat's).
    #[serde(default)]
    pub lift: f32,
    /// How much more each bone swings back than the one before it (radians per bone), lifting a
    /// long tail's end into an S; 0 hangs it.
    #[serde(default)]
    pub curve: f32,
}

/// The wings a race grows: a bat's, a membrane stretched on an arm and fingers, rolled within
/// these ranges ([`crate::parts::WingRoll`]).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct WingRow {
    /// Each wing's open reach from the shoulder, as a share of the body's height.
    pub span: Span,
    /// How open they rest, 0 folded against the back to 1 spread.
    pub open: Span,
    /// Chance that the membrane is torn.
    pub tear: f32,
    /// Membrane colours; empty draws the skin darkened.
    #[serde(default)]
    pub membrane: Vec<Hex>,
    /// Fingers on the wing, 3 or 4.
    pub fingers: usize,
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

/// A denizen's stat block as the SRD gives it: what the 5-number `stats` row cannot hold.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct StatBlock {
    pub size: String,
    pub kind: String,
    pub armour_class: i32,
    pub hit_points: i32,
    /// Count, sides and bonus of the hit dice (12d12 + 60).
    pub hit_dice: [i32; 3],
    /// Walking speed in feet.
    pub speed: u32,
    pub charisma: i32,
    pub attacks: Vec<BlockAttack>,
    pub source: String,
}

/// One attack of a [`StatBlock`]: to hit and damage dice (count, sides, bonus).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct BlockAttack {
    pub name: String,
    pub bonus: i32,
    pub dice: [i32; 3],
    pub damage: String,
    pub range: String,
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
static HAIR_DEMON: LazyLock<Vec<Rgb>> = LazyLock::new(|| colours("hair_demon.json", include_str!("../../../data/hair_demon.json")));
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
        Folk::WingedDemon | Folk::HornedDemon | Folk::Succubus => &HAIR_DEMON,
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

    #[test]
    fn the_cyclops_is_a_denizen_on_the_folk_body() {
        assert!(!Folk::ALL.contains(&Folk::Cyclops));
        assert!(Folk::DENIZENS.contains(&Folk::Cyclops) && Folk::Cyclops.is_denizen() && !Folk::Human.is_denizen());
        assert_eq!(Folk::parse("cyclops"), Some(Folk::Cyclops));
        assert_eq!(Folk::Cyclops.denizen_body(), Some(fb_denizens::DenizenBody::Cyclops));
        assert_eq!(Folk::of_denizen(fb_denizens::DenizenBody::Cyclops), Some(Folk::Cyclops));
        assert_eq!(Folk::Human.denizen_body(), None);
        let r = Folk::Cyclops.race();
        assert_eq!(r.eyes, EyeKind::One);
        assert!(r.h / Folk::Human.race().h > 1.6 && r.h / Folk::Human.race().h < 1.8);
        // Our own, after the SRD 5.1 Hill Giant: STR 22 DEX 10 CON 20 INT 7 WIS 8 CHA 8, AC 14, 115 hp (10d12 + 50), 30 ft.
        assert_eq!(r.stats, [22, 10, 20, 7, 8]);
        let b = r.block.as_ref().unwrap();
        assert_eq!((b.armour_class, b.hit_points, b.hit_dice, b.speed, b.charisma), (14, 115, [10, 12, 50], 30, 8));
        assert_eq!(b.attacks.iter().map(|a| a.dice).collect::<Vec<_>>(), [[3, 8, 6], [3, 10, 6]]);
        assert!(Folk::ALL.into_iter().all(|f| f.race().block.is_none()));
    }
}
