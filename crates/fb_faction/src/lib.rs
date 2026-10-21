//! The factions of a world, rolled from its seed.
//!
//! A [`World`] rolls a pool of twelve to twenty [`FactionSpec`]s from the world (delve) seed, and a
//! level draws two to four of them from its own seed ([`World::draw`]). A faction on several levels
//! is the same spec everywhere: it is the pool's, not the level's. Everything here is a pure
//! function of the seeds, with no Bevy and no state.
//!
//! Each faction rolls a name (from `procgen_grammar`'s faction name grammar, keyed by its folk
//! and intention, and unique in the pool), the folk it leans toward, a pair of outfits, an
//! alignment, one to three likes and hates, an intention, a heraldic [`Field`] (two tinctures
//! and a crest, by the shield's rule of tincture), an armour lean and whether it is
//! [`insular`](FactionSpec::insular). Three in four of its members are its folk; an insular
//! faction is wholly its own folk ([`FactionSpec::member_folk`]).
//!
//! The seven factions of `data/factions.json` are examples the grammar was written from, not a
//! list this crate reads.

use std::collections::HashSet;

use armory::families::shield::roll_field;
use armory::heraldry::{Division, Field};
use fb_body::Folk;
use procgen_grammar::faction_name;
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

/// Factions in a world's pool, inclusive.
pub const POOL_SIZE: (u32, u32) = (12, 20);
/// Factions a level draws, inclusive.
pub const PER_LEVEL: (u32, u32) = (2, 4);
/// Chance that a member is of the faction's folk rather than any folk.
pub const FOLK_LEAN: f64 = 0.75;
/// Chance that a faction dislikes other folk and is made wholly of its own.
const INSULAR_ODDS: f64 = 0.25;

const SIZE_SALT: u32 = 0xFAC7_0001;
const FACTION_SALT: u32 = 0xFAC7_0002;
const DRAW_SALT: u32 = 0xFAC7_0003;

/// One faction, an index into its world's pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Faction(pub u8);

/// What a faction is after, which colours its talk and its needs and the words of its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intention {
    /// Holds the way out, or claims to know it.
    WayOut,
    Trade,
    Worship,
    Plunder,
}

impl Intention {
    pub const ALL: [Intention; 4] = [Intention::WayOut, Intention::Trade, Intention::Worship, Intention::Plunder];

    /// The key `data/faction_names.json` uses.
    pub fn key(self) -> &'static str {
        match self {
            Intention::WayOut => "way_out",
            Intention::Trade => "trade",
            Intention::Worship => "worship",
            Intention::Plunder => "plunder",
        }
    }
}

/// A faction's outfit: a key of `data/outfits.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outfit {
    Fighter,
    Ranger,
    Mage,
    Cleric,
    Scavenger,
}

impl Outfit {
    /// The delver's class that dresses this way, if one does.
    pub fn class(self) -> Option<ClassKind> {
        match self {
            Outfit::Fighter => Some(ClassKind::Fighter),
            Outfit::Ranger => Some(ClassKind::Ranger),
            Outfit::Mage => Some(ClassKind::Mage),
            Outfit::Cleric => Some(ClassKind::Cleric),
            Outfit::Scavenger => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Outfit::Fighter => "fighter",
            Outfit::Ranger => "ranger",
            Outfit::Mage => "mage",
            Outfit::Cleric => "cleric",
            Outfit::Scavenger => "scavenger",
        }
    }
}

/// The kind of armour a faction's members lean toward.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArmourLean {
    Padded,
    Leather,
    Mail,
    Scale,
    Plate,
}

/// The nine alignments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    LawfulGood,
    NeutralGood,
    ChaoticGood,
    LawfulNeutral,
    TrueNeutral,
    ChaoticNeutral,
    LawfulEvil,
    NeutralEvil,
    ChaoticEvil,
}

impl Alignment {
    pub const ALL: [Alignment; 9] = [
        Alignment::LawfulGood,
        Alignment::NeutralGood,
        Alignment::ChaoticGood,
        Alignment::LawfulNeutral,
        Alignment::TrueNeutral,
        Alignment::ChaoticNeutral,
        Alignment::LawfulEvil,
        Alignment::NeutralEvil,
        Alignment::ChaoticEvil,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Alignment::LawfulGood => "lawful good",
            Alignment::NeutralGood => "neutral good",
            Alignment::ChaoticGood => "chaotic good",
            Alignment::LawfulNeutral => "lawful neutral",
            Alignment::TrueNeutral => "true neutral",
            Alignment::ChaoticNeutral => "chaotic neutral",
            Alignment::LawfulEvil => "lawful evil",
            Alignment::NeutralEvil => "neutral evil",
            Alignment::ChaoticEvil => "chaotic evil",
        }
    }
}

/// The kinds of creature a faction can like or hate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureKind {
    Rats,
    Snakes,
    Spiders,
    Undead,
    Bats,
    Centipedes,
    Trolls,
    Jackalopes,
}

impl CreatureKind {
    pub const ALL: [CreatureKind; 8] = [
        CreatureKind::Rats,
        CreatureKind::Snakes,
        CreatureKind::Spiders,
        CreatureKind::Undead,
        CreatureKind::Bats,
        CreatureKind::Centipedes,
        CreatureKind::Trolls,
        CreatureKind::Jackalopes,
    ];
}

/// The delver's classes, which a faction can like or hate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassKind {
    Fighter,
    Ranger,
    Mage,
    Cleric,
}

impl ClassKind {
    pub const ALL: [ClassKind; 4] = [ClassKind::Fighter, ClassKind::Ranger, ClassKind::Mage, ClassKind::Cleric];
}

/// One thing a faction likes or hates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Preference {
    Creature(CreatureKind),
    /// Another faction of the same world.
    Faction(Faction),
    Folk(Folk),
    Class(ClassKind),
}

/// A faction, everything about it rolled from the world seed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FactionSpec {
    /// Its place in the world's pool.
    pub id: Faction,
    /// As the grammar writes it, article and all ("the Scale Court").
    pub name: String,
    /// The folk most of its members are.
    pub folk: Folk,
    /// Its members dress from these two outfits, which differ.
    pub outfits: [Outfit; 2],
    pub alignment: Alignment,
    pub intention: Intention,
    /// One to three, none also in `hates`.
    pub likes: Vec<Preference>,
    /// One to three, none also in `likes`. An insular faction hates another folk.
    pub hates: Vec<Preference>,
    /// Its colours (two tinctures of opposite kinds, in a divided field) and its crest.
    pub field: Field,
    pub armour: ArmourLean,
    /// Dislikes other folk, and is made wholly of its own.
    pub insular: bool,
}

impl FactionSpec {
    /// The folk of a member: always the faction's own if it is insular, otherwise its own with
    /// chance [`FOLK_LEAN`] and any folk else, from the member's stream `rng`.
    pub fn member_folk(&self, rng: &mut Mulberry32) -> Folk {
        if self.insular || rng.chance(FOLK_LEAN) { self.folk } else { *rng.pick(&Folk::ALL) }
    }

    /// The outfit keys of `data/outfits.json` its members dress from.
    pub fn outfit_keys(&self) -> [&'static str; 2] {
        [self.outfits[0].key(), self.outfits[1].key()]
    }

    /// The other folk it hates, if any.
    pub fn hated_folk(&self) -> impl Iterator<Item = Folk> + '_ {
        self.hates.iter().filter_map(|p| if let Preference::Folk(f) = p { Some(*f) } else { None })
    }
}

/// The key of a folk in the name grammar.
pub fn folk_key(folk: Folk) -> &'static str {
    match folk {
        Folk::Human => "human",
        Folk::Dwarf => "dwarf",
        Folk::Gnome => "gnome",
        Folk::Goblin => "goblin",
        Folk::Lizardfolk => "lizardfolk",
        Folk::Pigfolk => "pigfolk",
    }
}

/// A world's pool of factions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub seed: u32,
    factions: Vec<FactionSpec>,
}

impl World {
    /// The pool for the world (delve) seed `seed`: the same seed, the same factions.
    pub fn new(seed: u32) -> World {
        debug_assert!(POOL_SIZE.1 <= u8::MAX as u32, "a faction id is a u8");
        let count = Mulberry32::new(mix(seed, SIZE_SALT, 0)).int(POOL_SIZE.0, POOL_SIZE.1) as usize;
        let mut taken = HashSet::new();
        let factions = (0..count)
            .map(|i| {
                let spec = roll_faction(seed, i, count, &taken);
                taken.insert(spec.name.clone());
                spec
            })
            .collect();
        World { seed, factions }
    }

    /// The whole pool, in order.
    pub fn factions(&self) -> &[FactionSpec] {
        &self.factions
    }

    pub fn faction(&self, id: Faction) -> &FactionSpec {
        &self.factions[id.0 as usize % self.factions.len()]
    }

    /// The two to four factions a level of seed `level_seed` holds, in a seeded order.
    pub fn draw(&self, level_seed: u32) -> Vec<Faction> {
        let mut rng = Mulberry32::new(mix(self.seed, DRAW_SALT, level_seed));
        let count = (rng.int(PER_LEVEL.0, PER_LEVEL.1) as usize).min(self.factions.len());
        let mut ids: Vec<Faction> = self.factions.iter().map(|f| f.id).collect();
        shuffle(&mut ids, &mut rng);
        ids.truncate(count);
        ids
    }

    /// The specs a level of seed `level_seed` holds ([`World::draw`]).
    pub fn draw_specs(&self, level_seed: u32) -> Vec<FactionSpec> {
        self.draw(level_seed).into_iter().map(|id| self.faction(id).clone()).collect()
    }
}

/// A seed from a seed, a salt and a second word, a finalizer in the style of splitmix32.
fn mix(a: u32, salt: u32, b: u32) -> u32 {
    let mut h = a ^ salt.rotate_left(7) ^ b.wrapping_mul(0x9E37_79B9);
    h = (h ^ (h >> 16)).wrapping_mul(0x85EB_CA6B);
    h = (h ^ (h >> 13)).wrapping_mul(0xC2B2_AE35);
    h ^ (h >> 16)
}

/// The divisions of a crest field: always divided, so a faction's two tinctures both show.
const DIVISIONS: [(Division, u32); 11] = [
    (Division::PerPale, 3),
    (Division::PerFess, 2),
    (Division::Quarterly, 2),
    (Division::PerBend, 2),
    (Division::PerChevron, 2),
    (Division::PerSaltire, 1),
    (Division::Paly, 1),
    (Division::Barry, 1),
    (Division::Bendy, 1),
    (Division::Checky, 1),
    (Division::Lozengy, 1),
];

fn roll_faction(world: u32, index: usize, count: usize, taken: &HashSet<String>) -> FactionSpec {
    use Outfit::*;
    let mut rng = Mulberry32::new(mix(world, FACTION_SALT, index as u32));
    let id = Faction(index as u8);
    let folk = *rng.pick(&Folk::ALL);
    let intention = *rng.pick(&Intention::ALL);
    let alignment = *rng.pick(&Alignment::ALL);
    let insular = rng.chance(INSULAR_ODDS);

    // Dress: one outfit from what its purpose wears, one from what its folk wear.
    let by_intention: &[(Outfit, u32)] = match intention {
        Intention::WayOut => &[(Ranger, 3), (Fighter, 3), (Scavenger, 1)],
        Intention::Trade => &[(Ranger, 3), (Scavenger, 3), (Mage, 1)],
        Intention::Worship => &[(Cleric, 5), (Mage, 2)],
        Intention::Plunder => &[(Fighter, 3), (Scavenger, 3), (Ranger, 1)],
    };
    let by_folk: &[(Outfit, u32)] = match folk {
        Folk::Human => &[(Cleric, 2), (Mage, 2), (Fighter, 2), (Ranger, 1)],
        Folk::Dwarf => &[(Cleric, 2), (Mage, 1), (Fighter, 3)],
        Folk::Gnome => &[(Mage, 3), (Scavenger, 2), (Ranger, 1)],
        Folk::Goblin => &[(Scavenger, 4), (Fighter, 2)],
        Folk::Lizardfolk => &[(Fighter, 3), (Ranger, 3), (Mage, 1)],
        Folk::Pigfolk => &[(Ranger, 3), (Scavenger, 3), (Cleric, 1)],
    };
    // The two differ: the second is drawn from its folk's table without the first.
    let first = *rng.weighted(by_intention);
    let rest: Vec<(Outfit, u32)> = by_folk.iter().copied().filter(|(o, _)| *o != first).collect();
    let outfits = [first, *rng.weighted(&rest)];

    let armour = *rng.weighted(match intention {
        Intention::WayOut => &[(ArmourLean::Mail, 3), (ArmourLean::Plate, 2), (ArmourLean::Leather, 2), (ArmourLean::Scale, 1)][..],
        Intention::Trade => &[(ArmourLean::Padded, 3), (ArmourLean::Leather, 3), (ArmourLean::Mail, 1)],
        Intention::Worship => &[(ArmourLean::Padded, 2), (ArmourLean::Mail, 2), (ArmourLean::Plate, 2), (ArmourLean::Scale, 1)],
        Intention::Plunder => &[(ArmourLean::Leather, 3), (ArmourLean::Scale, 3), (ArmourLean::Mail, 1), (ArmourLean::Plate, 1)],
    });

    // The armory's field roll keeps the rule of tincture; the crest is always there.
    let field = roll_field(&mut rng, true, &DIVISIONS, 1.0);

    // Likes and hates come off two shuffled lists with nothing in both. An insular faction's
    // first hate is another folk (and that folk is not one it likes); no other faction hates
    // folk, though any may like one.
    let hated_folk = insular.then(|| {
        let others: Vec<Folk> = Folk::ALL.iter().copied().filter(|f| *f != folk).collect();
        *rng.pick(&others)
    });
    let mut hate_pool: Vec<Preference> = CreatureKind::ALL.iter().map(|&k| Preference::Creature(k)).collect();
    // A faction never hates a class it dresses in.
    let worn: Vec<ClassKind> = outfits.iter().filter_map(|o| o.class()).collect();
    hate_pool.extend(ClassKind::ALL.iter().filter(|c| !worn.contains(c)).map(|&c| Preference::Class(c)));
    hate_pool.extend((0..count).filter(|&i| i != index).map(|i| Preference::Faction(Faction(i as u8))));
    let mut like_pool = hate_pool.clone();
    like_pool.extend(Folk::ALL.iter().filter(|f| Some(**f) != hated_folk).map(|&f| Preference::Folk(f)));
    shuffle(&mut like_pool, &mut rng);
    shuffle(&mut hate_pool, &mut rng);
    let likes: Vec<Preference> = like_pool.into_iter().take(rng.int(1, 3) as usize).collect();
    let hate_count = rng.int(1, 3) as usize;
    let mut hates: Vec<Preference> = hated_folk.map(Preference::Folk).into_iter().collect();
    let more = hate_count.saturating_sub(hates.len());
    hates.extend(hate_pool.into_iter().filter(|p| !likes.contains(p)).take(more));

    // The name; on a clash with one already in the pool, another seed from the same stream.
    let (folk_key, intention_key) = (folk_key(folk), intention.key());
    let mut name = faction_name(folk_key, intention_key, rng.int(0, u32::MAX));
    for _ in 0..64 {
        if !taken.contains(&name) {
            break;
        }
        name = faction_name(folk_key, intention_key, rng.int(0, u32::MAX));
    }
    if taken.contains(&name) {
        name = format!("{name} of the {}th Hall", index + 1);
    }

    FactionSpec { id, name, folk, outfits, alignment, intention, likes, hates, field, armour, insular }
}

fn shuffle<T>(items: &mut [T], rng: &mut Mulberry32) {
    for i in (1..items.len()).rev() {
        let j = rng.int(0, i as u32) as usize;
        items.swap(i, j);
    }
}

mod standing;
pub use standing::{Band, Delver, Standing, StandingRules, Standings, Victim};

#[cfg(test)]
mod tests;
