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
//! Below [`DENIZEN_DEPTH`] the abyss also holds denizen factions: a separate, salted part of the
//! pool that follows the folk factions (so the folk factions of a world are what they were before
//! denizens existed). A denizen faction ([`FactionSpec::denizen`]) is made of one kind of
//! denizen ([`FactionSpec::member_body`]); it rolls a name, alignment, intention, likes and
//! hates, colours and crest like any faction. Folk and denizen factions are alike in their
//! tastes: any faction can like or hate any other, and a folk faction takes a taste for a given
//! denizen faction with chance [`DENIZEN_TASTE_ODDS`], by a separate stream so nothing else it
//! rolled moves. A level draws the folk factions it always did and, from [`DENIZEN_DEPTH`] down, usually one denizen
//! faction and sometimes two ([`World::draw_at`]).
//!
//! The seven factions of `data/factions.json` are examples the grammar was written from, not a
//! list this crate reads.

use std::collections::HashSet;

mod trade;
pub use trade::{Master, Masters, Profession, Style, Tier, Trade, TradeRules, armour_style};

use armory::families::Generator;
use armory::families::banner::{Banner, BannerMount, BannerState};
use armory::families::shield::roll_field;
use armory::heraldry::{Division, Field, Tincture};
use fb_body::{Folk, Sex};
pub use fb_denizens::{DENIZEN_DEPTH, DenizenBody, DenizenKind, denizens_at};
use procgen_grammar::faction_named;
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
/// Denizen factions in a world's pool, inclusive: one of each kind, then up to two more.
pub const DENIZEN_POOL: (u32, u32) = (4, 6);
/// Chance that a level at or below [`DENIZEN_DEPTH`] holds a denizen faction.
pub const DENIZEN_ODDS: f64 = 0.85;
/// Chance that such a level holds a second one.
pub const SECOND_DENIZEN_ODDS: f64 = 0.2;
/// Chance that a folk faction likes or hates a given denizen faction (half each), in place of
/// one of its other tastes. Low, since most folk have never met a denizen.
pub const DENIZEN_TASTE_ODDS: f64 = 0.05;

const SIZE_SALT: u32 = 0xFAC7_0001;
const FACTION_SALT: u32 = 0xFAC7_0002;
const DRAW_SALT: u32 = 0xFAC7_0003;
const SEX_SALT: u32 = 0xFAC7_0004;
const DENIZEN_SIZE_SALT: u32 = 0xFAC7_0014;
const DENIZEN_SALT: u32 = 0xFAC7_0015;
const DENIZEN_DRAW_SALT: u32 = 0xFAC7_0016;
const DENIZEN_TASTE_SALT: u32 = 0xFAC7_0017;
const BANNER_SALT: u32 = 0xFAC7_0018;

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
    #[serde(alias = "ranger")]
    Fighter,
    Mage,
    Cleric,
    Scavenger,
}

impl Outfit {
    /// The outfit a class dresses in.
    pub fn of(class: ClassKind) -> Outfit {
        match class {
            ClassKind::Fighter => Outfit::Fighter,
            ClassKind::Mage => Outfit::Mage,
            ClassKind::Cleric => Outfit::Cleric,
        }
    }

    /// The delver's class that dresses this way, if one does.
    pub fn class(self) -> Option<ClassKind> {
        match self {
            Outfit::Fighter => Some(ClassKind::Fighter),
            Outfit::Mage => Some(ClassKind::Mage),
            Outfit::Cleric => Some(ClassKind::Cleric),
            Outfit::Scavenger => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Outfit::Fighter => "fighter",
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

/// The cloth a faction's name puts its members in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Garment {
    /// Every member wears a robe as the outermost layer ("the Sisters of Azul").
    Robe,
    /// Body armour, and a robe or surcoat over it ("the Redeemed").
    RobeOverArmour,
}

/// The trim a faction's name puts on its cloth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trim {
    /// A hem and edges in gold.
    GoldHem,
}

/// Which folk a faction's name says its members are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolkSaid {
    /// Of every folk, evenly ("the Redeemed", "the Grey Company").
    Mixed,
    /// Of the faction's own folk alone ("the Dwarven Guild").
    Own,
}

/// What a faction's name says about how its members look, none of it when the name says nothing
/// ([`Looks::is_plain`]), and then the faction is rolled as it always was.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Looks {
    /// Its colours as the name gives them, the main colour first: one or two tinctures. They are
    /// also the faction's [`field`](FactionSpec::field), and every piece of its cloth is dyed in
    /// them.
    #[serde(default)]
    pub colours: Vec<Tincture>,
    /// The armour lean the name says, which is also the faction's [`armour`](FactionSpec::armour).
    #[serde(default)]
    pub armour: Option<ArmourLean>,
    #[serde(default)]
    pub garment: Option<Garment>,
    #[serde(default)]
    pub trim: Option<Trim>,
    #[serde(default)]
    pub folk: Option<FolkSaid>,
}

impl Looks {
    /// Whether the name says nothing about looks.
    pub fn is_plain(&self) -> bool {
        *self == Looks::default()
    }

    /// What the tags of a name (`colour`, `colour2`, `armour`, `garment`, `trim`, `folk_mix`) say.
    /// A value this crate does not know says nothing.
    fn of(name: &procgen_grammar::Expansion) -> Looks {
        let tincture = |v: &str| match v {
            "or" => Some(Tincture::Or),
            "argent" => Some(Tincture::Argent),
            "gules" => Some(Tincture::Gules),
            "azure" => Some(Tincture::Azure),
            "vert" => Some(Tincture::Vert),
            "purpure" => Some(Tincture::Purpure),
            "sable" => Some(Tincture::Sable),
            _ => None,
        };
        let mut colours: Vec<Tincture> = ["colour", "colour2"].iter().filter_map(|k| name.tag(k).and_then(tincture)).collect();
        colours.dedup();
        Looks {
            colours,
            armour: match name.tag("armour") {
                Some("plate") => Some(ArmourLean::Plate),
                Some("light") => Some(ArmourLean::Leather),
                _ => None,
            },
            garment: match name.tag("garment") {
                Some("robe") => Some(Garment::Robe),
                Some("robe_over_armour") => Some(Garment::RobeOverArmour),
                _ => None,
            },
            trim: match name.tag("trim") {
                Some("gold_hem") => Some(Trim::GoldHem),
                _ => None,
            },
            folk: match name.tag("folk_mix") {
                Some("mixed") => Some(FolkSaid::Mixed),
                Some("own") => Some(FolkSaid::Own),
                _ => None,
            },
        }
    }
}

/// The field of `rolled` with the colours a name gives: the first is the main tincture, the second
/// the other one. Given one, the other is whichever rolled tincture is of the opposite kind, so a
/// colour keeps a metal and a metal a colour. A charge that would vanish into the field changes
/// to the next tincture neither of them is.
fn coloured_field(mut field: Field, colours: &[Tincture]) -> Field {
    let Some(&first) = colours.first() else { return field };
    let second = colours.get(1).copied().unwrap_or(if field.tincture.is_metal() != first.is_metal() { field.tincture } else { field.tincture2 });
    (field.tincture, field.tincture2) = (first, second);
    if field.charge_tincture == first || field.charge_tincture == second {
        let at = Tincture::ALL.iter().position(|t| *t == field.charge_tincture).unwrap_or(0);
        field.charge_tincture = (1..=Tincture::ALL.len())
            .map(|i| Tincture::ALL[(at + i) % Tincture::ALL.len()])
            .find(|t| *t != first && *t != second && t.is_metal() != second.is_metal())
            .unwrap_or(Tincture::Sable);
    }
    field
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
    #[serde(alias = "ranger")]
    Fighter,
    Mage,
    Cleric,
}

impl ClassKind {
    pub const ALL: [ClassKind; 3] = [ClassKind::Fighter, ClassKind::Mage, ClassKind::Cleric];
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
    /// The folk most of its members are. For a denizen faction ([`FactionSpec::denizen`]) this is
    /// a placeholder (human) that nothing may read: its members are denizens, not folk.
    pub folk: Folk,
    /// The sex its name says its members are ("the Sisters of the Pale Gate" are all women), if
    /// it says one; otherwise they are either.
    #[serde(default)]
    pub sex: Option<Sex>,
    /// The classes its name says its members are, none if the name says nothing ("the Daughters of
    /// the Burning Chapel" are all clerics; "the Black Reavers" fighters). Its
    /// members dress only in these, and it never hates one.
    #[serde(default)]
    pub classes: Vec<ClassKind>,
    /// Its members dress from these two outfits, which differ unless its name says one class.
    pub outfits: [Outfit; 2],
    pub alignment: Alignment,
    pub intention: Intention,
    /// One to three, none also in `hates`.
    pub likes: Vec<Preference>,
    /// One to three, none also in `likes`. An insular faction hates another folk.
    pub hates: Vec<Preference>,
    /// Its colours (two tinctures of opposite kinds, in a divided field, unless the name gives
    /// them) and its crest.
    pub field: Field,
    pub armour: ArmourLean,
    /// What its name says of how members look: colours, garment, trim and folk.
    #[serde(default)]
    pub looks: Looks,
    /// Dislikes other folk, and is made wholly of its own. Never set for a denizen faction.
    pub insular: bool,
    /// The kind of denizen its members are, if it is a denizen faction rather than a folk one.
    #[serde(default)]
    pub denizen: Option<DenizenKind>,
}

impl FactionSpec {
    /// The faction's banner as it hangs in a hall: one shape (its end, hem, fringe and finials)
    /// wherever it hangs, rod-hung, painted with exactly its field and hemmed in its charge's
    /// tincture so the hem never clashes. A pure function of the spec, from a seed of its own.
    pub fn banner(&self) -> BannerState {
        let seed = self.banner_seed();
        self.paint(Banner::roll_hall(seed, BannerMount::Rod, Banner::roll(seed).cloth.end))
    }

    /// [`FactionSpec::banner`] with its width and length drawn from the given ranges instead, as
    /// for a short one over a door: the same shape at another size.
    pub fn banner_sized(&self, width: (f32, f32), length: (f32, f32)) -> BannerState {
        let seed = self.banner_seed();
        self.paint(Banner::roll_sized(seed, BannerMount::Rod, Banner::roll(seed).cloth.end, width, length))
    }

    fn banner_seed(&self) -> u32 {
        mix(armory::crest::field_key(&self.field) as u32, BANNER_SALT, self.id.0 as u32)
    }

    fn paint(&self, mut banner: BannerState) -> BannerState {
        banner.hem.trim = self.field.charge_tincture;
        banner.field = self.field;
        banner
    }

    /// The folk of a member: always the faction's own if it is insular, otherwise its own with
    /// chance [`FOLK_LEAN`] and any folk else, from the member's stream `rng`. A faction whose
    /// name says its members are of every folk draws each one evenly from all six.
    pub fn member_folk(&self, rng: &mut Mulberry32) -> Folk {
        if self.looks.folk == Some(FolkSaid::Mixed) {
            *rng.pick(&Folk::ALL)
        } else if self.insular || rng.chance(FOLK_LEAN) {
            self.folk
        } else {
            *rng.pick(&Folk::ALL)
        }
    }

    /// The sex of a member whose seed is `seed`: the one its name says, otherwise a woman or a
    /// man with even odds. A pure function of the seed (no stream shared with the member's
    /// place), so it does not move where anyone stands.
    pub fn member_sex(&self, seed: u32) -> Sex {
        // Denizens are rolled as men: no sex-shaped build is made for their bodies yet (a succubus,
        // later, will be a woman).
        if self.denizen.is_some() {
            return Sex::Male;
        }
        self.sex.unwrap_or_else(|| if Mulberry32::new(seed ^ SEX_SALT).chance(0.5) { Sex::Female } else { Sex::Male })
    }

    /// Whether this is a denizen faction, whose members are denizens and never folk.
    pub fn is_denizen(&self) -> bool {
        self.denizen.is_some()
    }

    /// The body of a denizen member, picked from the bodies of its kind from the member's stream
    /// `rng`; `None` for a folk faction, whose members come from [`FactionSpec::member_folk`].
    pub fn member_body(&self, rng: &mut Mulberry32) -> Option<DenizenBody> {
        self.denizen.map(|kind| *rng.pick(kind.bodies()))
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
        Folk::Minotaur => "minotaur",
        Folk::Faun => "faun",
        // A denizen's name grammar is its own people's, not the folk's.
        Folk::Cyclops => "cyclops",
        Folk::WingedDemon | Folk::HornedDemon | Folk::Succubus => "demon",
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
        let mut factions: Vec<FactionSpec> = (0..count)
            .map(|i| {
                let spec = roll_faction(seed, i, count, &taken);
                taken.insert(spec.name.clone());
                spec
            })
            .collect();

        // The denizen factions follow, from their own salted streams, so the folk factions above
        // are what they were before denizens existed: one of each kind, then up to two more.
        let mut kinds: Vec<DenizenKind> = DenizenKind::ALL.to_vec();
        let mut size = Mulberry32::new(mix(seed, DENIZEN_SIZE_SALT, 0));
        let extra = size.int(DENIZEN_POOL.0, DENIZEN_POOL.1) as usize - kinds.len();
        kinds.extend((0..extra).map(|_| *size.pick(&DenizenKind::ALL)));
        debug_assert!(count + kinds.len() <= u8::MAX as usize, "a faction id is a u8");
        let total = count + kinds.len();
        for (j, kind) in kinds.into_iter().enumerate() {
            let spec = roll_denizen_faction(seed, count + j, total, kind, &taken);
            taken.insert(spec.name.clone());
            factions.push(spec);
        }


        // Folk and denizen factions are alike in what they like and hate: any faction can name
        // any other. Most folk have never met a denizen, so each folk faction takes a liking or a
        // grudge against a given denizen faction only with chance [`DENIZEN_TASTE_ODDS`]. It takes
        // the place of its last taste that is not a folk, so its list keeps its length and
        // nothing else it rolled (name, folk, dress, colours) moves.
        for i in 0..count {
            let mut rng = Mulberry32::new(mix(seed, DENIZEN_TASTE_SALT, i as u32));
            for d in count..total {
                if !rng.chance(DENIZEN_TASTE_ODDS) {
                    continue;
                }
                let likes = rng.chance(0.5);
                let spec = &mut factions[i];
                let list = if likes { &mut spec.likes } else { &mut spec.hates };
                let target = Preference::Faction(Faction(d as u8));
                if let Some(at) = list.iter().rposition(|p| !matches!(p, Preference::Folk(_))) {
                    list[at] = target;
                }
            }
        }
        World { seed, factions }
    }

    /// How many of the pool are folk factions: the first ones, which every level draws from.
    pub fn folk_count(&self) -> usize {
        self.factions.iter().take_while(|f| f.denizen.is_none()).count()
    }

    /// The folk factions of the pool, in order.
    pub fn folk_factions(&self) -> &[FactionSpec] {
        &self.factions[..self.folk_count()]
    }

    /// The denizen factions of the pool, in order: they follow the folk factions.
    pub fn denizen_factions(&self) -> &[FactionSpec] {
        &self.factions[self.folk_count()..]
    }

    /// The whole pool, in order.
    pub fn factions(&self) -> &[FactionSpec] {
        &self.factions
    }

    pub fn faction(&self, id: Faction) -> &FactionSpec {
        &self.factions[id.0 as usize % self.factions.len()]
    }

    /// The two to four folk factions a level of seed `level_seed` holds, in a seeded order. Never
    /// a denizen faction: see [`World::draw_at`].
    pub fn draw(&self, level_seed: u32) -> Vec<Faction> {
        let mut rng = Mulberry32::new(mix(self.seed, DRAW_SALT, level_seed));
        let folk = self.folk_factions();
        let count = (rng.int(PER_LEVEL.0, PER_LEVEL.1) as usize).min(folk.len());
        let mut ids: Vec<Faction> = folk.iter().map(|f| f.id).collect();
        shuffle(&mut ids, &mut rng);
        ids.truncate(count);
        ids
    }

    /// The specs a level of seed `level_seed` holds ([`World::draw`]).
    pub fn draw_specs(&self, level_seed: u32) -> Vec<FactionSpec> {
        self.draw(level_seed).into_iter().map(|id| self.faction(id).clone()).collect()
    }

    /// The factions a level of seed `level_seed`, `depth` levels down, holds: the folk draw, and
    /// from [`DENIZEN_DEPTH`] down usually one denizen faction ([`DENIZEN_ODDS`]) and sometimes
    /// a second ([`SECOND_DENIZEN_ODDS`]), last in the list. Above that depth it is exactly
    /// [`World::draw`].
    pub fn draw_at(&self, level_seed: u32, depth: u32) -> Vec<Faction> {
        let mut ids = self.draw(level_seed);
        let denizens = self.denizen_factions();
        if denizens_at(depth) && !denizens.is_empty() {
            let mut rng = Mulberry32::new(mix(self.seed, DENIZEN_DRAW_SALT, level_seed));
            if rng.chance(DENIZEN_ODDS) {
                let first = rng.pick(denizens).id;
                ids.push(first);
                if rng.chance(SECOND_DENIZEN_ODDS) {
                    let others: Vec<Faction> = denizens.iter().map(|f| f.id).filter(|id| *id != first).collect();
                    if !others.is_empty() {
                        ids.push(*rng.pick(&others));
                    }
                }
            }
        }
        ids
    }

    /// The specs a level of seed `level_seed` at `depth` holds ([`World::draw_at`]).
    pub fn draw_specs_at(&self, level_seed: u32, depth: u32) -> Vec<FactionSpec> {
        self.draw_at(level_seed, depth).into_iter().map(|id| self.faction(id).clone()).collect()
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

/// A faction rolled as it always was, with its name drawn last. A name that says a class
/// ("the Daughters of the Burning Chapel" are all clerics) then asks for the roll again with that
/// class given, the name kept: every draw before the outfits is the same, so what a name says
/// changes only that faction's own outfits, hates and what is drawn after them, and a faction
/// whose name says no class is rolled exactly as before.
fn roll_faction(world: u32, index: usize, count: usize, taken: &HashSet<String>) -> FactionSpec {
    let first = roll_with(world, index, count, taken, None);
    if first.classes.is_empty() && first.looks.is_plain() { first } else { roll_with(world, index, count, taken, Some(&first)) }
}

/// One roll of a faction. With `named` (the first roll, whose name said a class) the members
/// dress as that class and the name is kept; without it the name is drawn last.
fn roll_with(world: u32, index: usize, count: usize, taken: &HashSet<String>, named: Option<&FactionSpec>) -> FactionSpec {
    use Outfit::*;
    let mut rng = Mulberry32::new(mix(world, FACTION_SALT, index as u32));
    let id = Faction(index as u8);
    let folk = *rng.pick(&Folk::ALL);
    let intention = *rng.pick(&Intention::ALL);
    let alignment = *rng.pick(&Alignment::ALL);
    let mut insular = rng.chance(INSULAR_ODDS);
    let said = named.map(|n| n.looks.clone()).unwrap_or_default();
    // A name that says its members are of one folk, or of every folk, settles it.
    match said.folk {
        Some(FolkSaid::Own) => insular = true,
        Some(FolkSaid::Mixed) => insular = false,
        None => {}
    }
    let dressed_as: &[ClassKind] = named.map_or(&[], |n| &n.classes);

    // Dress: one outfit from what its purpose wears, one from what its folk wear.
    let by_intention: &[(Outfit, u32)] = match intention {
        Intention::WayOut => &[(Fighter, 6), (Scavenger, 1)],
        Intention::Trade => &[(Fighter, 3), (Scavenger, 3), (Mage, 1)],
        Intention::Worship => &[(Cleric, 5), (Mage, 2)],
        Intention::Plunder => &[(Fighter, 4), (Scavenger, 3)],
    };
    let by_folk: &[(Outfit, u32)] = match folk {
        Folk::Human => &[(Cleric, 2), (Mage, 2), (Fighter, 3)],
        Folk::Dwarf => &[(Cleric, 2), (Mage, 1), (Fighter, 3)],
        Folk::Gnome => &[(Mage, 3), (Scavenger, 2), (Fighter, 1)],
        Folk::Goblin => &[(Scavenger, 4), (Fighter, 2)],
        Folk::Lizardfolk => &[(Fighter, 6), (Mage, 1)],
        Folk::Pigfolk => &[(Fighter, 3), (Scavenger, 3), (Cleric, 1)],
        Folk::Minotaur => &[(Fighter, 5), (Scavenger, 2)],
        Folk::Faun => &[(Fighter, 3), (Mage, 2), (Cleric, 1)],
        // Denizens are not rolled into folk factions; their kit is a fighter's.
        Folk::Cyclops => &[(Fighter, 3), (Scavenger, 1)],
        Folk::WingedDemon => &[(Fighter, 5), (Mage, 2)],
        Folk::HornedDemon => &[(Fighter, 5), (Scavenger, 1)],
        Folk::Succubus => &[(Mage, 3), (Cleric, 2), (Fighter, 1)],
    };
    // The two differ: the second is drawn from its folk's table without the first.
    let first = *rng.weighted(by_intention);
    let rest: Vec<(Outfit, u32)> = by_folk.iter().copied().filter(|(o, _)| *o != first).collect();
    let mut outfits = [first, *rng.weighted(&rest)];
    // A name that says what they are (the second roll) dresses every member so: one class in both outfits, or two
    // classes in a drawn order.
    match *dressed_as {
        [only] => outfits = [Outfit::of(only); 2],
        [a, b] => outfits = if rng.chance(0.5) { [Outfit::of(b), Outfit::of(a)] } else { [Outfit::of(a), Outfit::of(b)] },
        _ => {}
    }

    let armour = *rng.weighted(match intention {
        Intention::WayOut => &[(ArmourLean::Mail, 3), (ArmourLean::Plate, 2), (ArmourLean::Leather, 2), (ArmourLean::Scale, 1)][..],
        Intention::Trade => &[(ArmourLean::Padded, 3), (ArmourLean::Leather, 3), (ArmourLean::Mail, 1)],
        Intention::Worship => &[(ArmourLean::Padded, 2), (ArmourLean::Mail, 2), (ArmourLean::Plate, 2), (ArmourLean::Scale, 1)],
        Intention::Plunder => &[(ArmourLean::Leather, 3), (ArmourLean::Scale, 3), (ArmourLean::Mail, 1), (ArmourLean::Plate, 1)],
    });
    let armour = said.armour.unwrap_or(armour);

    // The armory's field roll keeps the rule of tincture; the crest is always there.
    let field = coloured_field(roll_field(&mut rng, true, &DIVISIONS, 1.0), &said.colours);

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

    let (name, sex, classes, looks) = match named {
        Some(n) => (n.name.clone(), n.sex, n.classes.clone(), n.looks.clone()),
        None => {
        // The name comes first, because its words say who the members are: the `class` it sets
        // narrows the classes they dress as. A name with two class words ("the Reavers of the
        // Chapel") says two things of its members, so it is never kept: like a clash with a name
        // already in the pool, it is drawn again from another seed of the same stream.
        let (folk_key, intention_key) = (folk_key(folk), intention.key());
        let mut named = faction_named(folk_key, intention_key, rng.int(0, u32::MAX));
        for _ in 0..64 {
            if !taken.contains(&named.text) && named.conflicts.is_empty() {
                break;
            }
            named = faction_named(folk_key, intention_key, rng.int(0, u32::MAX));
        }
        let classes = named_classes(named.tag("class"));
        let looks = Looks::of(&named);
        let sex = match named.tag("sex") {
            Some("female") => Some(Sex::Female),
            Some("male") => Some(Sex::Male),
            _ => None,
        };
        let mut name = named.text;
        if taken.contains(&name) {
            name = format!("{name} of the {}th Hall", index + 1);
        }
            (name, sex, classes, looks)
        }
    };

    FactionSpec { id, name, folk, sex, classes, outfits, alignment, intention, likes, hates, field, armour, looks, insular, denizen: None }
}

/// The classes a name's `class` tag says its members are: `cleric` or `fighter`. Empty
/// for no tag, or one this crate does not know.
fn named_classes(tag: Option<&str>) -> Vec<ClassKind> {
    match tag {
        Some("cleric") => vec![ClassKind::Cleric],
        Some("fighter") => vec![ClassKind::Fighter],
        _ => Vec::new(),
    }
}

/// A denizen faction of `kind` at pool index `index` of `total`. Its members are denizens, so its
/// `folk` is a placeholder and it is never insular. Denizens wear little: the outfits are the
/// gear they carry (a fighter's weapons, a mage's trinkets) more than clothes, and the armour
/// lean is light for fauns and heavier for minotaurs and cyclopes, which the body code may
/// ignore. The kind sets the odds of an alignment (demons lean evil, fauns chaotic good or
/// neutral) and of an intention.
fn roll_denizen_faction(world: u32, index: usize, total: usize, kind: DenizenKind, taken: &HashSet<String>) -> FactionSpec {
    use Alignment::*;
    use Outfit::*;
    let mut rng = Mulberry32::new(mix(world, DENIZEN_SALT, index as u32));
    let id = Faction(index as u8);

    let (intentions, alignments, dress, armours): (&[(Intention, u32)], &[(Alignment, u32)], &[(Outfit, u32)], &[(ArmourLean, u32)]) = match kind {
        DenizenKind::Minotaur => (
            &[(Intention::Plunder, 3), (Intention::WayOut, 2), (Intention::Worship, 1)],
            &[(ChaoticEvil, 3), (NeutralEvil, 2), (LawfulNeutral, 1), (TrueNeutral, 1)],
            &[(Fighter, 5), (Scavenger, 3)],
            &[(ArmourLean::Leather, 3), (ArmourLean::Scale, 2), (ArmourLean::Plate, 1)],
        ),
        DenizenKind::Cyclops => (
            &[(Intention::Trade, 3), (Intention::Worship, 2), (Intention::Plunder, 1), (Intention::WayOut, 1)],
            &[(LawfulNeutral, 3), (TrueNeutral, 2), (LawfulEvil, 2), (ChaoticEvil, 1)],
            &[(Fighter, 3), (Scavenger, 3), (Cleric, 1)],
            &[(ArmourLean::Leather, 2), (ArmourLean::Mail, 2), (ArmourLean::Plate, 1)],
        ),
        DenizenKind::Faun => (
            &[(Intention::Worship, 3), (Intention::Trade, 2), (Intention::WayOut, 1)],
            &[(ChaoticGood, 3), (ChaoticNeutral, 3), (NeutralGood, 2), (TrueNeutral, 1)],
            &[(Fighter, 4), (Mage, 2), (Cleric, 2)],
            &[(ArmourLean::Padded, 3), (ArmourLean::Leather, 2)],
        ),
        DenizenKind::Demon => (
            &[(Intention::Worship, 3), (Intention::Plunder, 2), (Intention::Trade, 1), (Intention::WayOut, 1)],
            &[(LawfulEvil, 3), (ChaoticEvil, 3), (NeutralEvil, 2), (ChaoticNeutral, 1)],
            &[(Mage, 3), (Cleric, 3), (Fighter, 2)],
            &[(ArmourLean::Leather, 3), (ArmourLean::Scale, 3), (ArmourLean::Padded, 1)],
        ),
    };
    let intention = *rng.weighted(intentions);
    let alignment = *rng.weighted(alignments);

    // The name comes first, because its words say how members look, as a folk faction's do: the
    // colours of its field, its armour, a `class` that narrows its dress. A name with two class
    // words is drawn again, like a clash. A denizen is no one folk, so a said folk is dropped, and
    // a said sex is kept on the faction but not worn by its members (denizens are rolled male).
    let (kind_key, intention_key) = (kind.key(), intention.key());
    let mut named = faction_named(kind_key, intention_key, rng.int(0, u32::MAX));
    for _ in 0..64 {
        if !taken.contains(&named.text) && named.conflicts.is_empty() {
            break;
        }
        named = faction_named(kind_key, intention_key, rng.int(0, u32::MAX));
    }
    let classes = named_classes(named.tag("class"));
    let looks = Looks { folk: None, ..Looks::of(&named) };
    let mut name = named.text;
    if taken.contains(&name) {
        name = format!("{name} of the {}th Pit", index + 1);
    }

    let first = *rng.weighted(dress);
    let rest: Vec<(Outfit, u32)> = dress.iter().copied().filter(|(o, _)| *o != first).collect();
    let mut outfits = [first, *rng.weighted(&rest)];
    match *classes {
        [only] => outfits = [Outfit::of(only); 2],
        [a, b] => outfits = [Outfit::of(a), Outfit::of(b)],
        _ => {}
    }
    let armour = *rng.weighted(armours);
    let armour = looks.armour.unwrap_or(armour);
    let field = coloured_field(roll_field(&mut rng, true, &DIVISIONS, 1.0), &looks.colours);

    // Tastes come off two shuffled lists with nothing in both, from everything in the world:
    // creatures, the classes they do not dress in, every other faction and, unlike a folk
    // faction, any folk.
    let mut pool: Vec<Preference> = CreatureKind::ALL.iter().map(|&k| Preference::Creature(k)).collect();
    let worn: Vec<ClassKind> = outfits.iter().filter_map(|o| o.class()).collect();
    pool.extend(ClassKind::ALL.iter().filter(|c| !worn.contains(c)).map(|&c| Preference::Class(c)));
    pool.extend((0..total).filter(|&i| i != index).map(|i| Preference::Faction(Faction(i as u8))));
    pool.extend(Folk::ALL.iter().map(|&f| Preference::Folk(f)));
    let mut like_pool = pool.clone();
    let mut hate_pool = pool;
    shuffle(&mut like_pool, &mut rng);
    shuffle(&mut hate_pool, &mut rng);
    let likes: Vec<Preference> = like_pool.into_iter().take(rng.int(1, 3) as usize).collect();
    let hate_count = rng.int(1, 3) as usize;
    let hates: Vec<Preference> = hate_pool.into_iter().filter(|p| !likes.contains(p)).take(hate_count).collect();

    FactionSpec { id, name, folk: Folk::Human, sex: None, classes, outfits, alignment, intention, likes, hates, field, armour, looks, insular: false, denizen: Some(kind) }
}

fn shuffle<T>(items: &mut [T], rng: &mut Mulberry32) {
    for i in (1..items.len()).rev() {
        let j = rng.int(0, i as u32) as usize;
        items.swap(i, j);
    }
}

mod standing;
pub use standing::{Band, Delver, Favour, Standing, StandingRules, Standings, Victim};

#[cfg(test)]
mod tests;
