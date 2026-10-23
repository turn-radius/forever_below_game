//! # fb_creatures
//!
//! Creatures of the abyss as plain data: rats, snakes, skeletons, giant spiders, trolls, jackalopes, giant
//! centipedes, bats and minotaurs, something to fight or to talk to.
//!
//! - [`centipede`] rolls a [`CentipedeState`]: a spine of plates, each with a pair of legs, a
//!   head with antennae, simple eyes and forcipules, in a colouring and banding.
//! - [`rat`] and [`snake`] roll a typed state from a seed ([`RatState`], [`SnakeState`]): the
//!   species (rat or giant rat, poisonous or constrictor snake), size, build, colouring and
//!   pattern. [`Creature::of`] rolls one of a species chosen by the caller.
//! - [`skeleton`] rolls a [`SkeletonState`]: a human's bones with seeded height, build, colour,
//!   cracks and missing pieces, laid out on an `fb_body` human's joints so gear fits it as it
//!   fits a body.
//! - [`spider`] rolls a [`SpiderState`]: a giant spider's leg span, abdomen, colouring, markings and
//!   hair, with eight jointed legs that [`pose`] plants on the floor one by one.
//! - [`troll`] rolls a [`TrollState`]: a stooped, long-armed giant of warty green or grey skin on
//!   the same human joints, moving with the skeleton's walk and swing poses plus a hunch, and
//!   carrying the regeneration rule ([`troll::Regeneration`]) that makes it hard to kill.
//! - [`jackalope`] rolls a [`JackalopeState`]: a large hare's body, ears and tufted tail with a pair
//!   of branching antlers, its size, ear length, antler tines and spread and fur colour from the
//!   seed.
//! - [`bat`] rolls a [`BatState`]: a bat or a giant bat with its wingspan, ear and nose shape,
//!   coat and wing membrane; the first creatures that fly, hanging from the ceiling or beating
//!   their wings at a height above the floor ([`pose::BatPose`]).
//! - [`minotaur`] rolls a [`MinotaurState`]: a bull-headed giant half again a man's height, on
//!   digitigrade legs ending in cloven hooves, the first of the denizens (intelligent folk of the
//!   deep, who hold ground and are spoken to by gesture). [`denizen`] holds what the denizen
//!   bodies share: the upright frame on a human's joints, horns, hooves, tails and hands, and
//!   the mapping between [`fb_denizens::DenizenBody`] and [`Species`].
//! - Each state builds a [`Rig`]: named bones, each carrying its own [`armory::ItemMesh`] built
//!   from Armory geometry in the bone's own space, so the game can move the bones without a
//!   skeleton (a rat's body, head, legs and tail; a snake's head, jaw, tongue and spine) or a
//!   skeleton's bones.
//! - [`pose`] turns a gait, a slither, a coil, a rear, a bite or a swing into bone placements, so the
//!   game only copies them onto transforms, and the rules (feet on the floor, a body that never
//!   sinks into it) are tested here.
//! - Each species carries its D&D 5e SRD 5.1 numbers ([`StatBlock`]): hit points, bite damage,
//!   speed, senses and size. Combat uses them as placeholders until the character simulation
//!   is designed.
//!
//! Space: 1 unit = 10 cm. A creature stands with its feet or belly on y = 0, facing +Z, its
//! left toward +X, as `fb_body` bodies do.
//!
//! This work includes material taken from the System Reference Document 5.1 ("SRD 5.1") by
//! Wizards of the Coast LLC and available at
//! <https://dnd.wizards.com/resources/systems-reference-document>. The SRD 5.1 is licensed under
//! the Creative Commons Attribution 4.0 International License available at
//! <https://creativecommons.org/licenses/by/4.0/legalcode>.
//!
//! Plain Rust, no Bevy; every creature is a pure function of its seed.

pub mod bat;
pub mod centipede;
pub mod denizen;
pub mod gear;
pub mod jackalope;
pub mod minotaur;
pub mod pose;
pub mod rat;
pub mod rig;
pub mod shape;
pub mod skeleton;
pub mod snake;
pub mod spider;
pub mod troll;

pub use bat::{BatColouring, BatEars, BatMembrane, BatNose, BatState};
pub use centipede::{CentipedeBanding, CentipedeColouring, CentipedeState};
pub use rat::{RatColouring, RatPattern, RatState};
pub use gear::{GearPiece, SkeletonGear};
pub use jackalope::{JackalopeColouring, JackalopeState};
pub use minotaur::MinotaurState;
pub use rig::{Bone, Rig};
pub use skeleton::SkeletonState;
pub use snake::{SnakeColouring, SnakePattern, SnakeState};
pub use spider::{SpiderColouring, SpiderMarking, SpiderState};
pub use troll::TrollState;

pub use fb_items::range::Range;
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

/// The SRD 5.1 in its CC-BY-4.0 edition; `#page=` links below are its PDF pages, which match
/// the page numbers printed on them.
pub const SRD: &str = "https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf";

/// Feet to world units: 1 ft is 0.3048 m, and a unit is 10 cm.
pub const UNITS_PER_FOOT: f32 = 3.048;

/// A creature's size category ([SRD 5.1 p. 92, "Size Categories"](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=92)).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Size {
    Tiny,
    Small,
    Medium,
    Large,
}

impl Size {
    /// The side of the square of floor it controls in combat, in feet: Tiny 2½ by 2½ ft, Small
    /// and Medium 5 by 5 ft, Large 10 by 10 ft.
    pub fn space_feet(self) -> f32 {
        match self {
            Size::Tiny => 2.5,
            Size::Small | Size::Medium => 5.0,
            Size::Large => 10.0,
        }
    }

    /// [`Size::space_feet`] in world units.
    pub fn space(self) -> f32 {
        self.space_feet() * UNITS_PER_FOOT
    }

    /// The size below, if any.
    pub fn smaller(self) -> Option<Size> {
        match self {
            Size::Tiny => None,
            Size::Small => Some(Size::Tiny),
            Size::Medium => Some(Size::Small),
            Size::Large => Some(Size::Medium),
        }
    }
}

/// Damage or hit dice as a stat block writes them: `count`d`sides` + `bonus`. A flat number
/// (the rat's "1 piercing damage") is no dice and a bonus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dice {
    pub count: u32,
    pub sides: u32,
    pub bonus: i32,
}

impl Dice {
    pub const fn new(count: u32, sides: u32, bonus: i32) -> Dice {
        Dice { count, sides, bonus }
    }

    /// A flat amount, no dice.
    pub const fn flat(amount: i32) -> Dice {
        Dice { count: 0, sides: 0, bonus: amount }
    }

    /// The average, rounded down, as stat blocks print it ([SRD 5.1 p. 255](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=255),
    /// "Hit Points": "a monster with 2d8 hit points has 9 hit points on average (2 × 4½)").
    pub fn average(self) -> i32 {
        ((self.count * (self.sides + 1)) as i32 + 2 * self.bonus).div_euclid(2)
    }

    /// One roll from `rng`, never below zero.
    pub fn roll(self, rng: &mut Mulberry32) -> u32 {
        let dice: i32 = (0..self.count).map(|_| rng.int(1, self.sides.max(1)) as i32).sum();
        (dice + self.bonus).max(0) as u32
    }
}

impl std::fmt::Display for Dice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.count, self.bonus) {
            (0, b) => write!(f, "{b}"),
            (c, 0) => write!(f, "{c}d{}", self.sides),
            (c, b) if b > 0 => write!(f, "{c}d{} + {b}", self.sides),
            (c, b) => write!(f, "{c}d{} - {}", self.sides, -b),
        }
    }
}

/// What a creature is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Species {
    Rat,
    GiantRat,
    PoisonousSnake,
    ConstrictorSnake,
    Skeleton,
    GiantSpider,
    Troll,
    Jackalope,
    GiantCentipede,
    Bat,
    GiantBat,
    Minotaur,
}

/// The six ability scores of a stat block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Abilities {
    pub strength: u8,
    pub dexterity: u8,
    pub constitution: u8,
    pub intelligence: u8,
    pub wisdom: u8,
    pub charisma: u8,
}

impl Abilities {
    /// The modifier of a score ([SRD 5.1 p. 76](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=76)):
    /// half the score less 10, rounded down.
    pub fn modifier(score: u8) -> i32 {
        (score as i32 - 10).div_euclid(2)
    }
}

/// The kinds of damage the stat blocks here mention.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageType {
    Bludgeoning,
    Piercing,
    Poison,
    Slashing,
    Fire,
    Acid,
}

/// The conditions the stat blocks here make a creature immune to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Condition {
    Exhaustion,
    Poisoned,
}

/// A ranged weapon attack: to hit, damage on a hit, and the range in feet at which it is
/// shot normally and at disadvantage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RangedAttack {
    pub name: &'static str,
    pub to_hit: i32,
    pub damage: Dice,
    pub range: Range,
}

/// The SRD 5.1 numbers this game uses from a creature's stat block. Placeholders for combat
/// until the character simulation is designed; nothing else of the stat block is modelled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatBlock {
    /// The stat block's name.
    pub name: &'static str,
    pub size: Size,
    pub abilities: Abilities,
    /// The average hit points the block prints, and the dice they come from.
    pub hit_points: u32,
    pub hit_dice: Dice,
    /// Walking speed in feet per 6-second round.
    pub speed_feet: f32,
    /// Flying speed in feet per 6-second round; 0 for a creature that cannot fly.
    pub fly_speed_feet: f32,
    /// Armour Class: what an attack roll must reach to hit it.
    pub armour_class: i32,
    /// What the bite adds to its attack roll (the block's "+N to hit").
    pub attack_bonus: i32,
    /// Its melee attack's damage on a hit (piercing): a bite, or a skeleton's shortsword.
    pub bite: Dice,
    /// Its ranged weapon attack, if it has one (the skeleton's shortbow).
    pub ranged: Option<RangedAttack>,
    /// Damage it takes double of.
    pub vulnerabilities: &'static [DamageType],
    /// Damage that does nothing to it.
    pub immunities: &'static [DamageType],
    /// Conditions that cannot be put on it.
    pub condition_immunities: &'static [Condition],
    /// Poison the bite adds, halved on a successful DC 10 Constitution save (the poisonous
    /// snake's), if any. Saves are not modelled yet.
    pub venom: Option<Dice>,
    /// How far it senses without light (darkvision or blindsight), feet.
    pub senses_feet: f32,
    /// The challenge rating as the block prints it, and the experience it is worth.
    pub challenge: &'static str,
    pub xp: u32,
    /// The page of the SRD 5.1 the block is on.
    pub page: u32,
}

impl Species {
    pub const ALL: [Species; 12] = [Species::Rat, Species::GiantRat, Species::PoisonousSnake, Species::ConstrictorSnake, Species::Skeleton, Species::GiantSpider, Species::Troll, Species::Jackalope, Species::GiantCentipede, Species::Bat, Species::GiantBat, Species::Minotaur];

    /// The SRD 5.1 stat block, read from the CC-BY-4.0 PDF ([`SRD`]):
    ///
    /// - [Rat, p. 387](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=387):
    ///   Tiny, Armor Class 10, Hit Points 1 (1d4 − 1), Speed 20 ft., darkvision 30 ft.; Bite:
    ///   +0 to hit, 1 piercing damage.
    /// - [Giant Rat, p. 378](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=378):
    ///   Small, Armor Class 12, Hit Points 7 (2d6), Speed 30 ft., darkvision 60 ft.; Bite: +4 to
    ///   hit, 4 (1d4 + 2) piercing.
    /// - [Poisonous Snake, p. 386](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=386):
    ///   Tiny, Armor Class 13, Hit Points 2 (1d4), Speed 30 ft., blindsight 10 ft.; Bite: +5 to
    ///   hit, 1 piercing, and 5 (2d4) poison on a failed DC 10 Constitution save, half on a
    ///   success.
    /// - [Constrictor Snake, p. 369](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=369):
    ///   **Large**, Hit Points 13 (2d10 + 2), Speed 30 ft., blindsight 10 ft.; Bite: 5 (1d6 + 2)
    ///   piercing. Its Constrict (p. 370) is left for later.
    /// - [Skeleton, p. 346](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=346):
    ///   Medium undead, Armor Class 13 (armor scraps), Hit Points 13 (2d8 + 4), Speed 30 ft.,
    ///   STR 10 DEX 14 CON 15 INT 6 WIS 8 CHA 5, vulnerable to bludgeoning, immune to poison and
    ///   to exhaustion and being poisoned, darkvision 60 ft., Challenge 1/4 (50 XP); Shortsword:
    ///   +4 to hit, reach 5 ft., 5 (1d6 + 2) piercing, and Shortbow: +4 to hit, range 80/320
    ///   ft., 5 (1d6 + 2) piercing. [`StatBlock::bite`] is the shortsword's damage; the gear a
    ///   skeleton carries sets what it actually swings.
    /// - [Giant Spider, p. 379](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=379):
    ///   Large beast, Armor Class 14, Hit Points 26 (4d10 + 4), Speed 30 ft. and climb 30 ft.,
    ///   STR 14 DEX 16 CON 12 INT 2 WIS 11 CHA 4, blindsight 10 ft. and darkvision 60 ft.,
    ///   Challenge 1 (200 XP); Bite: +5 to hit, reach 5 ft., 7 (1d8 + 3) piercing, and the
    ///   target makes a DC 11 Constitution saving throw, taking 9 (2d8) poison damage on a
    ///   failure or half as much on a success. Its Web attack and its climbing walls wait for
    ///   later.
    /// - [Troll, p. 351](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=351):
    ///   Large giant, Armor Class 15 (natural armor), Hit Points 84 (8d10 + 40), Speed 30 ft.,
    ///   STR 18 DEX 13 CON 20 INT 7 WIS 9 CHA 7, darkvision 60 ft., Challenge 5 (1,800 XP);
    ///   Multiattack: Bite +7 to hit, 7 (1d6 + 4) piercing, and two Claws +7 to hit, 14
    ///   (2d6 + 4) slashing each ([`troll::CLAW`]). Its Regeneration is [`troll::Regeneration`].
    /// - [Minotaur, p. 333](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=333):
    ///   Large monstrosity, Armor Class 14 (natural armor), Hit Points 76 (9d10 + 27), Speed 40
    ///   ft., STR 18 DEX 11 CON 16 INT 6 WIS 16 CHA 9, darkvision 60 ft., Challenge 3 (700 XP);
    ///   Greataxe: +6 to hit, reach 5 ft., 17 (2d12 + 4) slashing ([`minotaur::GREATAXE`]), and
    ///   Gore: +6 to hit, reach 5 ft., 13 (2d8 + 4) piercing, which is [`StatBlock::bite`]. Its
    ///   Charge, Reckless and Labyrinthine Recall wait for later.
    /// - Jackalope: the SRD has none, so the block is our own. Small beast, Armor Class 14, Hit
    ///   Points 9 (2d6 + 2), Speed 50 ft., darkvision 60 ft., Challenge 1/4 (50 XP); Antlers: +4
    ///   to hit, 5 (1d6 + 2) piercing. A giant rat's Small frame
    ///   ([p. 378](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=378)) with a
    ///   skeleton's Challenge ([p. 346](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=346))
    ///   and a hare's speed. [`StatBlock::bite`] is the antlers' damage.
    /// - [Giant Centipede, p. 374](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=374):
    ///   Small beast, Armor Class 13, Hit Points 4 (1d6 + 1), Speed 30 ft. (and climb 30 ft.,
    ///   which is not modelled), STR 5 DEX 14 CON 12 INT 1 WIS 7 CHA 3, blindsight 30 ft.,
    ///   Challenge 1/4 (50 XP); Bite: +4 to hit, 4 (1d4 + 2) piercing, and 10 (3d6) poison on a
    ///   failed DC 11 Constitution save. [`StatBlock::venom`] is the 3d6; the save the game
    ///   rolls for any venom is shared, not this block's own DC.
    /// - [Bat, p. 367](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=367):
    ///   Tiny beast, Armor Class 12, Hit Points 1 (1d4 − 1), Speed 5 ft., fly 30 ft., STR 2 DEX
    ///   15 CON 8 INT 2 WIS 12 CHA 4, blindsight 60 ft., Challenge 0 (10 XP); Bite: +0 to hit,
    ///   1 piercing damage.
    /// - [Giant Bat, p. 373](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=373):
    ///   **Large** beast, Armor Class 13, Hit Points 22 (4d10), Speed 10 ft., fly 60 ft., STR 15
    ///   DEX 16 CON 11 INT 2 WIS 12 CHA 6, blindsight 60 ft., Challenge 1/4 (50 XP); Bite: +4 to
    ///   hit, 5 (1d6 + 2) piercing. Its Echolocation (no blindsight while deafened) is left for
    ///   later.
    pub fn stat_block(self) -> StatBlock {
        let ab = |strength, dexterity, constitution, intelligence, wisdom, charisma| Abilities { strength, dexterity, constitution, intelligence, wisdom, charisma };
        match self {
            Species::Rat => StatBlock {
                name: "Rat",
                size: Size::Tiny,
                abilities: ab(2, 11, 9, 2, 10, 4),
                hit_points: 1,
                hit_dice: Dice::new(1, 4, -1),
                speed_feet: 20.0,
                fly_speed_feet: 0.0,
                armour_class: 10,
                attack_bonus: 0,
                bite: Dice::flat(1),
                ranged: None,
                vulnerabilities: &[],
                immunities: &[],
                condition_immunities: &[],
                venom: None,
                senses_feet: 30.0,
                challenge: "0",
                xp: 10,
                page: 387,
            },
            Species::GiantRat => StatBlock {
                name: "Giant Rat",
                size: Size::Small,
                abilities: ab(7, 15, 11, 2, 10, 4),
                hit_points: 7,
                hit_dice: Dice::new(2, 6, 0),
                speed_feet: 30.0,
                fly_speed_feet: 0.0,
                armour_class: 12,
                attack_bonus: 4,
                bite: Dice::new(1, 4, 2),
                ranged: None,
                vulnerabilities: &[],
                immunities: &[],
                condition_immunities: &[],
                venom: None,
                senses_feet: 60.0,
                challenge: "1/8",
                xp: 25,
                page: 378,
            },
            Species::PoisonousSnake => StatBlock {
                name: "Poisonous Snake",
                size: Size::Tiny,
                abilities: ab(2, 16, 11, 1, 10, 3),
                hit_points: 2,
                hit_dice: Dice::new(1, 4, 0),
                speed_feet: 30.0,
                fly_speed_feet: 0.0,
                armour_class: 13,
                attack_bonus: 5,
                bite: Dice::flat(1),
                ranged: None,
                vulnerabilities: &[],
                immunities: &[],
                condition_immunities: &[],
                venom: Some(Dice::new(2, 4, 0)),
                senses_feet: 10.0,
                challenge: "1/8",
                xp: 25,
                page: 386,
            },
            Species::ConstrictorSnake => StatBlock {
                name: "Constrictor Snake",
                size: Size::Large,
                abilities: ab(15, 14, 12, 1, 10, 3),
                hit_points: 13,
                hit_dice: Dice::new(2, 10, 2),
                speed_feet: 30.0,
                fly_speed_feet: 0.0,
                armour_class: 12,
                attack_bonus: 4,
                bite: Dice::new(1, 6, 2),
                ranged: None,
                vulnerabilities: &[],
                immunities: &[],
                condition_immunities: &[],
                venom: None,
                senses_feet: 10.0,
                challenge: "1/4",
                xp: 50,
                page: 369,
            },
            Species::Skeleton => StatBlock {
                name: "Skeleton",
                size: Size::Medium,
                abilities: ab(10, 14, 15, 6, 8, 5),
                hit_points: 13,
                hit_dice: Dice::new(2, 8, 4),
                speed_feet: 30.0,
                fly_speed_feet: 0.0,
                armour_class: 13,
                attack_bonus: 4,
                bite: Dice::new(1, 6, 2),
                ranged: Some(RangedAttack { name: "Shortbow", to_hit: 4, damage: Dice::new(1, 6, 2), range: Range::new(80.0, 320.0) }),
                vulnerabilities: &[DamageType::Bludgeoning],
                immunities: &[DamageType::Poison],
                condition_immunities: &[Condition::Exhaustion, Condition::Poisoned],
                venom: None,
                senses_feet: 60.0,
                challenge: "1/4",
                xp: 50,
                page: 346,
            },
            Species::GiantSpider => StatBlock {
                name: "Giant Spider",
                size: Size::Large,
                abilities: ab(14, 16, 12, 2, 11, 4),
                hit_points: 26,
                hit_dice: Dice::new(4, 10, 4),
                speed_feet: 30.0,
                fly_speed_feet: 0.0,
                armour_class: 14,
                attack_bonus: 5,
                bite: Dice::new(1, 8, 3),
                ranged: None,
                vulnerabilities: &[],
                immunities: &[],
                condition_immunities: &[],
                venom: Some(Dice::new(2, 8, 0)),
                senses_feet: 60.0,
                challenge: "1",
                xp: 200,
                page: 379,
            },
            Species::Troll => StatBlock {
                name: "Troll",
                size: Size::Large,
                abilities: ab(18, 13, 20, 7, 9, 7),
                hit_points: 84,
                hit_dice: Dice::new(8, 10, 40),
                speed_feet: 30.0,
                fly_speed_feet: 0.0,
                armour_class: 15,
                attack_bonus: 7,
                bite: Dice::new(1, 6, 4),
                ranged: None,
                vulnerabilities: &[],
                immunities: &[],
                condition_immunities: &[],
                venom: None,
                senses_feet: 60.0,
                challenge: "5",
                xp: 1800,
                page: 351,
            },
            Species::Jackalope => StatBlock {
                name: "Jackalope",
                size: Size::Small,
                abilities: ab(8, 17, 12, 3, 13, 6),
                hit_points: 9,
                hit_dice: Dice::new(2, 6, 2),
                speed_feet: 50.0,
                fly_speed_feet: 0.0,
                armour_class: 14,
                attack_bonus: 4,
                bite: Dice::new(1, 6, 2),
                ranged: None,
                vulnerabilities: &[],
                immunities: &[],
                condition_immunities: &[],
                venom: None,
                senses_feet: 60.0,
                challenge: "1/4",
                xp: 50,
                // Not in the SRD: our own, so no page.
                page: 0,
            },
            Species::GiantCentipede => StatBlock {
                name: "Giant Centipede",
                size: Size::Small,
                abilities: ab(5, 14, 12, 1, 7, 3),
                hit_points: 4,
                hit_dice: Dice::new(1, 6, 1),
                speed_feet: 30.0,
                fly_speed_feet: 0.0,
                armour_class: 13,
                attack_bonus: 4,
                bite: Dice::new(1, 4, 2),
                ranged: None,
                vulnerabilities: &[],
                immunities: &[],
                condition_immunities: &[],
                venom: Some(Dice::new(3, 6, 0)),
                senses_feet: 30.0,
                challenge: "1/4",
                xp: 50,
                page: 374,
            },
            Species::Bat => StatBlock {
                name: "Bat",
                size: Size::Tiny,
                abilities: ab(2, 15, 8, 2, 12, 4),
                hit_points: 1,
                hit_dice: Dice::new(1, 4, -1),
                speed_feet: 5.0,
                fly_speed_feet: 30.0,
                armour_class: 12,
                attack_bonus: 0,
                bite: Dice::flat(1),
                ranged: None,
                vulnerabilities: &[],
                immunities: &[],
                condition_immunities: &[],
                venom: None,
                senses_feet: 60.0,
                challenge: "0",
                xp: 10,
                page: 367,
            },
            Species::GiantBat => StatBlock {
                name: "Giant Bat",
                size: Size::Large,
                abilities: ab(15, 16, 11, 2, 12, 6),
                hit_points: 22,
                hit_dice: Dice::new(4, 10, 0),
                speed_feet: 10.0,
                fly_speed_feet: 60.0,
                armour_class: 13,
                attack_bonus: 4,
                bite: Dice::new(1, 6, 2),
                ranged: None,
                vulnerabilities: &[],
                immunities: &[],
                condition_immunities: &[],
                venom: None,
                senses_feet: 60.0,
                challenge: "1/4",
                xp: 50,
                page: 373,
            },
            Species::Minotaur => StatBlock {
                name: "Minotaur",
                size: Size::Large,
                abilities: ab(18, 11, 16, 6, 16, 9),
                hit_points: 76,
                hit_dice: Dice::new(9, 10, 27),
                speed_feet: 40.0,
                fly_speed_feet: 0.0,
                armour_class: 14,
                attack_bonus: 6,
                bite: Dice::new(2, 8, 4),
                ranged: None,
                vulnerabilities: &[],
                immunities: &[],
                condition_immunities: &[],
                venom: None,
                senses_feet: 60.0,
                challenge: "3",
                xp: 700,
                page: 333,
            },
        }
    }

    pub fn is_bat(self) -> bool {
        matches!(self, Species::Bat | Species::GiantBat)
    }

    /// Whether it flies: it has a height above the floor, and no footsteps.
    pub fn is_flying(self) -> bool {
        self.stat_block().fly_speed_feet > 0.0
    }

    /// Its flying speed in world units per second, as [`Species::speed`] gives its walking
    /// speed; 0 for a creature that cannot fly.
    pub fn fly_speed(self) -> f32 {
        self.stat_block().fly_speed_feet * UNITS_PER_FOOT / 6.0
    }

    /// The kind of creature gear wards against: vermin (rats, bats, centipedes), undead,
    /// spiders, serpents, beasts (the jackalope) and trolls.
    pub fn foe(self) -> fb_items::Foe {
        use fb_items::Foe;
        match self {
            Species::Rat | Species::GiantRat | Species::Bat | Species::GiantBat | Species::GiantCentipede => Foe::Vermin,
            Species::Skeleton => Foe::Undead,
            Species::GiantSpider => Foe::Spider,
            Species::PoisonousSnake | Species::ConstrictorSnake => Foe::Serpent,
            Species::Jackalope | Species::Minotaur => Foe::Beast,
            Species::Troll => Foe::Troll,
        }
    }

    pub fn is_rat(self) -> bool {
        matches!(self, Species::Rat | Species::GiantRat)
    }

    pub fn is_snake(self) -> bool {
        matches!(self, Species::PoisonousSnake | Species::ConstrictorSnake)
    }

    pub fn is_skeleton(self) -> bool {
        self == Species::Skeleton
    }

    pub fn is_spider(self) -> bool {
        self == Species::GiantSpider
    }

    pub fn is_troll(self) -> bool {
        self == Species::Troll
    }

    pub fn is_minotaur(self) -> bool {
        self == Species::Minotaur
    }

    /// The denizen body this species is, if it is one: the bodies of `fb_denizens` that have a
    /// species built so far. Cyclopes, fauns and demons wait for later.
    pub fn denizen_body(self) -> Option<fb_denizens::DenizenBody> {
        match self {
            Species::Minotaur => Some(fb_denizens::DenizenBody::Minotaur),
            _ => None,
        }
    }

    /// The species that is the denizen body `body`; `None` for a body not built yet.
    pub fn of_denizen(body: fb_denizens::DenizenBody) -> Option<Species> {
        Species::ALL.into_iter().find(|s| s.denizen_body() == Some(body))
    }

    pub fn is_jackalope(self) -> bool {
        self == Species::Jackalope
    }

    pub fn is_centipede(self) -> bool {
        self == Species::GiantCentipede
    }

    /// Whether its venom leaves a victim it would drop to 0 hit points alive (at 1, and
    /// poisoned) rather than dead: the giant centipede's and the giant spider's bites, "If the
    /// poison damage reduces the target to 0 hit points, the target is stable but poisoned"
    /// ([SRD 5.1 p. 374](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=374),
    /// [p. 379](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=379)). The
    /// bite's own damage can still kill.
    pub fn venom_spares(self) -> bool {
        matches!(self, Species::GiantCentipede | Species::GiantSpider)
    }

    /// Its walking speed in world units per second: the stat block's feet per 6-second round
    /// ([SRD 5.1 p. 90](https://media.wizards.com/2023/downloads/dnd/SRD_CC_v5.1.pdf#page=90),
    /// "a round represents about 6 seconds").
    pub fn speed(self) -> f32 {
        self.stat_block().speed_feet * UNITS_PER_FOOT / 6.0
    }
}

/// One creature: a rat, a snake, a skeleton, a spider, a troll, a jackalope, a centipede, a bat or a minotaur, rolled from a seed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Creature {
    Rat(RatState),
    Snake(SnakeState),
    Skeleton(SkeletonState),
    Spider(SpiderState),
    Troll(TrollState),
    Jackalope(JackalopeState),
    Centipede(CentipedeState),
    Bat(BatState),
    Minotaur(MinotaurState),
}

impl Creature {
    /// A creature of `species` rolled from `seed`.
    pub fn of(species: Species, seed: u32) -> Creature {
        if species.is_bat() {
            Creature::Bat(BatState::roll_as(seed, species))
        } else if species.is_rat() {
            Creature::Rat(RatState::roll_as(seed, species))
        } else if species.is_skeleton() {
            Creature::Skeleton(SkeletonState::roll(seed))
        } else if species.is_spider() {
            Creature::Spider(SpiderState::roll(seed))
        } else if species.is_troll() {
            Creature::Troll(TrollState::roll(seed))
        } else if species.is_minotaur() {
            Creature::Minotaur(MinotaurState::roll(seed))
        } else if species.is_jackalope() {
            Creature::Jackalope(JackalopeState::roll(seed))
        } else if species.is_centipede() {
            Creature::Centipede(CentipedeState::roll(seed))
        } else {
            Creature::Snake(SnakeState::roll_as(seed, species))
        }
    }

    pub fn species(&self) -> Species {
        match self {
            Creature::Rat(r) => r.species,
            Creature::Snake(s) => s.species,
            Creature::Skeleton(_) => Species::Skeleton,
            Creature::Spider(_) => Species::GiantSpider,
            Creature::Troll(_) => Species::Troll,
            Creature::Jackalope(_) => Species::Jackalope,
            Creature::Centipede(c) => c.species,
            Creature::Bat(b) => b.species,
            Creature::Minotaur(_) => Species::Minotaur,
        }
    }

    pub fn seed(&self) -> u32 {
        match self {
            Creature::Rat(r) => r.seed,
            Creature::Snake(s) => s.seed,
            Creature::Skeleton(s) => s.seed,
            Creature::Spider(s) => s.seed,
            Creature::Troll(s) => s.seed,
            Creature::Jackalope(j) => j.seed,
            Creature::Centipede(c) => c.seed,
            Creature::Bat(b) => b.seed,
            Creature::Minotaur(m) => m.seed,
        }
    }

    /// Its bones and their meshes, in the rest pose.
    pub fn rig(&self) -> Rig {
        match self {
            Creature::Rat(r) => r.build(),
            Creature::Snake(s) => s.build(),
            Creature::Skeleton(s) => s.build(),
            Creature::Spider(s) => s.build(),
            Creature::Troll(s) => s.build(),
            Creature::Jackalope(j) => j.build(),
            Creature::Centipede(c) => c.build(),
            Creature::Bat(b) => b.build(),
            Creature::Minotaur(m) => m.build(),
        }
    }

    /// How far it reaches across the floor from its centre, in its rest pose, world units: the
    /// circle it keeps others out of.
    pub fn radius(&self) -> f32 {
        match self {
            Creature::Rat(r) => r.radius(),
            Creature::Snake(s) => s.radius(),
            Creature::Skeleton(s) => s.radius(),
            Creature::Spider(s) => s.radius(),
            Creature::Troll(s) => s.radius(),
            Creature::Jackalope(j) => j.radius(),
            Creature::Centipede(c) => c.radius(),
            Creature::Bat(b) => b.radius(),
            Creature::Minotaur(m) => m.radius(),
        }
    }
}

/// A rat or giant rat rolled from `seed`; one in five is a giant rat. Our own odds.
pub fn rat(seed: u32) -> RatState {
    RatState::roll(seed)
}

/// A poisonous snake or a constrictor rolled from `seed`; two in three are poisonous. Our own
/// odds.
pub fn snake(seed: u32) -> SnakeState {
    SnakeState::roll(seed)
}

/// A skeleton rolled from `seed`.
pub fn skeleton(seed: u32) -> SkeletonState {
    SkeletonState::roll(seed)
}

/// A giant spider rolled from `seed`.
pub fn spider(seed: u32) -> SpiderState {
    SpiderState::roll(seed)
}

/// A troll rolled from `seed`.
pub fn troll(seed: u32) -> TrollState {
    TrollState::roll(seed)
}

/// A jackalope rolled from `seed`.
pub fn jackalope(seed: u32) -> JackalopeState {
    JackalopeState::roll(seed)
}

/// A giant centipede rolled from `seed`.
pub fn centipede(seed: u32) -> CentipedeState {
    CentipedeState::roll(seed)
}

/// A minotaur rolled from `seed`.
pub fn minotaur(seed: u32) -> MinotaurState {
    MinotaurState::roll(seed)
}

/// A bat or a giant bat rolled from `seed`; one in five is a giant bat. Our own odds.
pub fn bat(seed: u32) -> BatState {
    BatState::roll(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_species_is_a_kind_of_foe_and_every_kind_has_one() {
        use fb_items::Foe;
        let kinds: Vec<Foe> = Species::ALL.into_iter().map(Species::foe).collect();
        assert!(Foe::ALL.iter().all(|f| kinds.contains(f)), "a kind no species is");
        assert_eq!(Species::Rat.foe(), Foe::Vermin);
        assert_eq!(Species::GiantBat.foe(), Foe::Vermin);
        assert_eq!(Species::GiantCentipede.foe(), Foe::Vermin);
        assert_eq!(Species::Skeleton.foe(), Foe::Undead);
        assert_eq!(Species::ConstrictorSnake.foe(), Foe::Serpent);
        assert_eq!(Species::Jackalope.foe(), Foe::Beast);
    }

    #[test]
    fn the_minotaur_stat_block_is_the_srd_numbers() {
        let m = Species::Minotaur.stat_block();
        assert_eq!((m.name, m.size, m.armour_class, m.hit_points, m.hit_dice, m.speed_feet, m.fly_speed_feet), ("Minotaur", Size::Large, 14, 76, Dice::new(9, 10, 27), 40.0, 0.0));
        assert_eq!((m.abilities.strength, m.abilities.dexterity, m.abilities.constitution, m.abilities.intelligence, m.abilities.wisdom, m.abilities.charisma), (18, 11, 16, 6, 16, 9));
        assert_eq!((m.senses_feet, m.challenge, m.xp, m.attack_bonus, m.bite, m.page), (60.0, "3", 700, 6, Dice::new(2, 8, 4), 333));
        assert_eq!(minotaur::GREATAXE, Dice::new(2, 12, 4));
        assert_eq!((m.bite.average(), minotaur::GREATAXE.average(), m.hit_dice.average()), (13, 17, 76));
        assert!(Species::Minotaur.is_minotaur() && !Species::Troll.is_minotaur());
        // 40 ft. a round is 20 units a second, a little over 20.3 as 40 * 3.048 / 6.
        assert!((Species::Minotaur.speed() - 20.32).abs() < 0.01);
        assert_eq!(Creature::of(Species::Minotaur, 5).species(), Species::Minotaur);
        assert_eq!(Creature::of(Species::Minotaur, 5).seed(), 5);
    }

    #[test]
    fn species_and_denizen_bodies_map_both_ways() {
        use fb_denizens::DenizenBody;
        assert_eq!(Species::Minotaur.denizen_body(), Some(DenizenBody::Minotaur));
        assert_eq!(Species::of_denizen(DenizenBody::Minotaur), Some(Species::Minotaur));
        // The bodies not built yet have no species, and no other species is a denizen.
        for body in [DenizenBody::Cyclops, DenizenBody::Faun, DenizenBody::WingedDemon, DenizenBody::HornedDemon, DenizenBody::Succubus] {
            assert_eq!(Species::of_denizen(body), None, "{body:?}");
        }
        assert!(Species::ALL.iter().filter(|s| s.denizen_body().is_some()).all(|&s| Species::of_denizen(s.denizen_body().unwrap()) == Some(s)));
        assert_eq!(Species::ALL.iter().filter(|s| s.denizen_body().is_some()).count(), 1);
    }

    #[test]
    fn the_bats_stat_blocks_are_the_srd_numbers() {
        let bat = Species::Bat.stat_block();
        assert_eq!((bat.size, bat.armour_class, bat.hit_points, bat.hit_dice, bat.speed_feet, bat.fly_speed_feet), (Size::Tiny, 12, 1, Dice::new(1, 4, -1), 5.0, 30.0));
        assert_eq!((bat.abilities.strength, bat.abilities.dexterity, bat.abilities.constitution, bat.abilities.intelligence, bat.abilities.wisdom, bat.abilities.charisma), (2, 15, 8, 2, 12, 4));
        assert_eq!((bat.senses_feet, bat.challenge, bat.xp, bat.attack_bonus, bat.bite, bat.page), (60.0, "0", 10, 0, Dice::flat(1), 367));
        let giant = Species::GiantBat.stat_block();
        assert_eq!((giant.size, giant.armour_class, giant.hit_points, giant.hit_dice, giant.speed_feet, giant.fly_speed_feet), (Size::Large, 13, 22, Dice::new(4, 10, 0), 10.0, 60.0));
        assert_eq!((giant.abilities.strength, giant.abilities.dexterity, giant.abilities.constitution, giant.abilities.intelligence, giant.abilities.wisdom, giant.abilities.charisma), (15, 16, 11, 2, 12, 6));
        assert_eq!((giant.senses_feet, giant.challenge, giant.xp, giant.attack_bonus, giant.bite, giant.page), (60.0, "1/4", 50, 4, Dice::new(1, 6, 2), 373));
        // Only bats fly; a bat flies at its fly speed, 30 ft. a round being 15 units a second.
        assert!(Species::ALL.iter().all(|s| s.is_flying() == s.is_bat()));
        assert!((Species::Bat.fly_speed() - 15.24).abs() < 0.01 && (Species::GiantBat.fly_speed() - 30.48).abs() < 0.01);
        assert_eq!(Species::Rat.fly_speed(), 0.0);
    }

    #[test]
    fn stat_blocks_carry_the_srd_armour_class_and_attack_bonus() {
        let numbers = |s: Species| (s.stat_block().armour_class, s.stat_block().attack_bonus);
        assert_eq!(numbers(Species::Rat), (10, 0));
        assert_eq!(numbers(Species::GiantRat), (12, 4));
        assert_eq!(numbers(Species::PoisonousSnake), (13, 5));
        assert_eq!(numbers(Species::ConstrictorSnake), (12, 4));
    }

    #[test]
    fn stat_blocks_are_the_srd_numbers() {
        let rat = Species::Rat.stat_block();
        assert_eq!((rat.size, rat.hit_points, rat.hit_dice, rat.bite, rat.page), (Size::Tiny, 1, Dice::new(1, 4, -1), Dice::flat(1), 387));
        let giant = Species::GiantRat.stat_block();
        assert_eq!((giant.size, giant.hit_points, giant.hit_dice, giant.bite, giant.page), (Size::Small, 7, Dice::new(2, 6, 0), Dice::new(1, 4, 2), 378));
        let viper = Species::PoisonousSnake.stat_block();
        assert_eq!((viper.size, viper.hit_points, viper.bite, viper.venom, viper.page), (Size::Tiny, 2, Dice::flat(1), Some(Dice::new(2, 4, 0)), 386));
        let constrictor = Species::ConstrictorSnake.stat_block();
        assert_eq!((constrictor.size, constrictor.hit_points, constrictor.hit_dice, constrictor.bite), (Size::Large, 13, Dice::new(2, 10, 2), Dice::new(1, 6, 2)));
        // Each block's printed average is its dice's average (1d4 − 1 is 1.5, printed 1).
        for species in Species::ALL {
            let block = species.stat_block();
            assert_eq!(block.hit_dice.average(), block.hit_points as i32, "{species:?}");
        }
        assert_eq!(Dice::new(1, 4, 2).average(), 4);
        assert_eq!(Dice::new(1, 6, 2).average(), 5);
        assert_eq!(Dice::new(2, 4, 0).average(), 5);
    }

    #[test]
    fn the_jackalope_stat_block_is_the_issues_numbers() {
        let block = Species::Jackalope.stat_block();
        assert_eq!((block.name, block.size, block.armour_class, block.speed_feet, block.senses_feet), ("Jackalope", Size::Small, 14, 50.0, 60.0));
        assert_eq!((block.hit_points, block.hit_dice, block.attack_bonus, block.bite), (9, Dice::new(2, 6, 2), 4, Dice::new(1, 6, 2)));
        assert_eq!((block.challenge, block.xp, block.venom, block.ranged.is_none()), ("1/4", 50, None, true));
        assert!(Species::Jackalope.is_jackalope() && !Species::Jackalope.is_rat() && !Species::Jackalope.is_snake());
        // 50 ft a round is 2.5 m a second.
        assert!((Species::Jackalope.speed() - 25.4).abs() < 0.01);
        assert_eq!(Creature::of(Species::Jackalope, 5).species(), Species::Jackalope);
        assert_eq!(Creature::of(Species::Jackalope, 5).seed(), 5);
    }

    #[test]
    fn dice_roll_in_their_range() {
        let mut rng = Mulberry32::new(9);
        for _ in 0..500 {
            let r = Dice::new(1, 4, 2).roll(&mut rng);
            assert!((3..=6).contains(&r));
            assert!(Dice::new(1, 4, -1).roll(&mut rng) <= 3);
        }
        assert_eq!(Dice::flat(1).roll(&mut rng), 1);
        assert_eq!(Dice::new(2, 10, 2).to_string(), "2d10 + 2");
        assert_eq!(Dice::new(1, 4, -1).to_string(), "1d4 - 1");
        assert_eq!(Dice::flat(1).to_string(), "1");
    }

    #[test]
    fn speeds_are_feet_per_round() {
        // 20 ft a round is about a metre a second.
        assert!((Species::Rat.speed() - 10.16).abs() < 0.01);
        assert!((Species::GiantRat.speed() - 15.24).abs() < 0.01);
    }

    #[test]
    fn spaces_are_the_srd_squares() {
        assert!((Size::Tiny.space() - 7.62).abs() < 1e-3);
        assert_eq!(Size::Small.space(), Size::Medium.space());
        assert!((Size::Large.space() - 30.48).abs() < 1e-3);
    }
}
