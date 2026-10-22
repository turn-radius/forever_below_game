//! What a level knows of itself, in words: its [`Lore`], everything a note found on it may tell
//! of ([`fb_items::notes`]). Its factions and what they like and hate, the creatures in it, what
//! its rooms were built for, who trades in it and the books lying in it. Read straight from the
//! level, so a note's words are true of the level it lies on.

use armory::Family;
use fb_body::Folk;
use fb_creatures::Species;
use fb_faction::{ClassKind, CreatureKind, Preference, Profession};
use fb_items::notes::{Book, Feeling, Lore, Thing, Trader};

use crate::Level;

impl Level {
    /// What this level knows of itself, for the notes found on it.
    pub fn lore(&self) -> Lore {
        let mut creatures: Vec<Thing> = Vec::new();
        for c in &self.creatures {
            let thing = species(c.species);
            if !creatures.contains(&thing) {
                creatures.push(thing);
            }
        }
        let mut places: Vec<String> = Vec::new();
        for p in self.purposes.iter().flatten() {
            if !places.iter().any(|n| n == p.name()) {
                places.push(p.name().to_string());
            }
        }
        let feelings = |of: fn(&fb_faction::FactionSpec) -> &Vec<Preference>| -> Vec<Feeling> {
            self.factions
                .iter()
                .flat_map(|f| of(f).iter().filter_map(|p| Some(Feeling { faction: f.name.clone(), thing: self.preference(*p)? })))
                .collect()
        };
        Lore {
            depth: self.depth,
            past: self.past.map(|p| p.name().to_string()),
            factions: self.faction_names(),
            likes: feelings(|f| &f.likes),
            hates: feelings(|f| &f.hates),
            creatures,
            places,
            traders: self.traders(),
            books: self.books(),
        }
    }

    /// Everyone on the level who sells: guards do not.
    fn traders(&self) -> Vec<Trader> {
        self.folk
            .iter()
            .filter_map(|spot| {
                let trade = spot.trade?;
                let wares = wares(trade.profession)?;
                Some(Trader { name: spot.name(), faction: self.faction(spot.faction).name.clone(), trade: trade.profession.label().to_string(), wares: wares.to_string() })
            })
            .collect()
    }

    /// The books lying loose on the level, and where.
    fn books(&self) -> Vec<Book> {
        self.items
            .iter()
            .filter(|spot| spot.family == Family::Writing)
            .filter_map(|spot| {
                let teaches = fb_items::roll_loot(spot.family, spot.seed, fb_items::Odds::default())?.lesson()?;
                let place = self.room_at(spot.x as i32, spot.y as i32).and_then(|r| self.purposes.get(r).copied().flatten()).map(|p| p.name().to_string());
                Some(Book { place, teaches })
            })
            .collect()
    }

    /// A faction's like or hate in words, or `None` for a faction this level does not hold, which
    /// its notes cannot name.
    fn preference(&self, p: Preference) -> Option<Thing> {
        Some(match p {
            Preference::Creature(c) => creature_kind(c),
            Preference::Faction(id) => {
                let name = self.factions.iter().find(|f| f.id == id)?.name.clone();
                Thing::new(name.clone(), name)
            }
            Preference::Folk(f) => folk(f),
            Preference::Class(c) => class(c),
        })
    }
}

/// The key of what a trader sells ([`fb_items::notes::WARES`]); `None` for a guard.
pub fn wares(profession: Profession) -> Option<&'static str> {
    Some(match profession {
        Profession::Armorer => "armorer",
        Profession::Weaponsmith => "weaponsmith",
        Profession::Bowyer => "bowyer",
        Profession::Tentmaker => "tentmaker",
        Profession::GeneralGoods => "general_goods",
        Profession::Alchemist => "alchemist",
        Profession::Enchanter => "enchanter",
        Profession::Guard => return None,
    })
}

/// A creature in words, one and many.
pub fn species(s: Species) -> Thing {
    let (one, many) = match s {
        Species::Rat => ("rat", "rats"),
        Species::GiantRat => ("giant rat", "giant rats"),
        Species::PoisonousSnake => ("viper", "vipers"),
        Species::ConstrictorSnake => ("constrictor", "constrictors"),
        Species::Skeleton => ("skeleton", "skeletons"),
        Species::GiantSpider => ("giant spider", "giant spiders"),
        Species::Troll => ("troll", "trolls"),
        Species::Jackalope => ("jackalope", "jackalopes"),
        Species::GiantCentipede => ("giant centipede", "giant centipedes"),
        Species::Bat => ("bat", "bats"),
        Species::GiantBat => ("giant bat", "giant bats"),
    };
    Thing::new(one, many)
}

/// A kind of creature a faction feels for, in words.
fn creature_kind(c: CreatureKind) -> Thing {
    let (one, many) = match c {
        CreatureKind::Rats => ("rat", "rats"),
        CreatureKind::Snakes => ("snake", "snakes"),
        CreatureKind::Spiders => ("spider", "spiders"),
        CreatureKind::Undead => ("walking corpse", "the undead"),
        CreatureKind::Bats => ("bat", "bats"),
        CreatureKind::Centipedes => ("centipede", "centipedes"),
        CreatureKind::Trolls => ("troll", "trolls"),
        CreatureKind::Jackalopes => ("jackalope", "jackalopes"),
    };
    Thing::new(one, many)
}

fn folk(f: Folk) -> Thing {
    let (one, many) = match f {
        Folk::Human => ("human", "humans"),
        Folk::Dwarf => ("dwarf", "dwarves"),
        Folk::Gnome => ("gnome", "gnomes"),
        Folk::Goblin => ("goblin", "goblins"),
        Folk::Lizardfolk => ("lizardfolk", "lizardfolk"),
        Folk::Pigfolk => ("pigfolk", "pigfolk"),
    };
    Thing::new(one, many)
}

fn class(c: ClassKind) -> Thing {
    let (one, many) = match c {
        ClassKind::Fighter => ("fighter", "fighters"),
        ClassKind::Ranger => ("ranger", "rangers"),
        ClassKind::Mage => ("mage", "mages"),
        ClassKind::Cleric => ("cleric", "clerics"),
    };
    Thing::new(one, many)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate;
    use fb_items::notes::{NoteKind, WARES, note_kind, write_note};

    #[test]
    fn a_level_lore_is_true_of_the_level() {
        for seed in 1..=24 {
            for depth in [1, 3] {
                let level = generate(seed, depth);
                let lore = level.lore();
                assert_eq!(lore.depth, depth);
                assert_eq!(lore.factions, level.faction_names());
                for c in &level.creatures {
                    assert!(lore.creatures.contains(&species(c.species)), "seed {seed}: {:?} missing", c.species);
                }
                for f in &level.factions {
                    for thing in f.hates.iter().filter_map(|h| level.preference(*h)) {
                        assert!(lore.hates.iter().any(|x| x.faction == f.name && x.thing == thing), "seed {seed}: {}'s hate", f.name);
                    }
                }
                let traders = level.folk.iter().filter(|s| s.trade.is_some_and(|t| t.profession != Profession::Guard)).count();
                assert_eq!(lore.traders.len(), traders, "seed {seed}");
            }
        }
    }

    #[test]
    fn a_trader_is_named_as_the_folk_is_named() {
        let level = (1..=40).map(|s| generate(s, 1)).find(|l| !l.lore().traders.is_empty()).expect("a level with a trader");
        let spot = level.folk.iter().find(|s| s.trade.is_some_and(|t| t.profession != Profession::Guard)).unwrap();
        let name = fb_items::naming::folk_name(&mut procgen_rng::Mulberry32::new(spot.seed ^ crate::folk::NAME_SALT), spot.folk.key());
        assert_eq!(level.lore().traders[0].name, name);
    }

    #[test]
    fn every_trade_sells_wares_the_grammar_knows() {
        for p in Profession::TRADES {
            assert!(WARES.contains(&wares(p).unwrap()), "{p:?}");
        }
        assert_eq!(wares(Profession::Guard), None);
    }

    #[test]
    fn generated_levels_write_notes_of_many_kinds() {
        let mut seen: Vec<NoteKind> = Vec::new();
        for seed in 1..=16 {
            let lore = generate(seed, 2).lore();
            for n in 0..16 {
                let words = write_note(n, &lore);
                assert!(!words.contains('#') && !words.is_empty(), "level {seed} note {n}: {words}");
                let kind = note_kind(n, &lore);
                if !seen.contains(&kind) {
                    seen.push(kind);
                }
            }
        }
        assert!(seen.len() >= 6, "kinds seen: {seen:?}");
    }
}
