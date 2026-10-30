//! What a level knows of itself, in words: its [`Lore`], everything a note found on it may tell
//! of ([`fb_items::notes`]). Its factions and what they like and hate, the creatures in it, what
//! its rooms were built for, who trades in it and the books lying in it. Read straight from the
//! level, so a note's words are true of the level it lies on.

use armory::Family;
use fb_body::Folk;
use fb_creatures::Species;
use fb_faction::{ClassKind, CreatureKind, Preference, Profession};
use fb_items::notes::{Book, Feeling, Lore, Thing, Trader};

use fb_denizens::DenizenKind;

use crate::Level;

/// How far (in cells) from a denizen faction's rooms a spot still counts as beside its ground.
pub const NEAR_GROUND: f32 = 10.0;

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
            tongues: self.tongues(),
            denizens: self.factions.iter().filter_map(|f| Some((f.name.clone(), f.denizen?))).collect(),
            prayers: Vec::new(),
        }
    }

    /// The tongues of the denizen factions on the level, each once.
    fn tongues(&self) -> Vec<DenizenKind> {
        let mut kinds: Vec<DenizenKind> = self.factions.iter().filter_map(|f| f.denizen).collect();
        kinds.sort();
        kinds.dedup();
        kinds
    }

    /// The kind of denizen whose ground a spot at (`x`, `y`) lies in or beside: inside a denizen
    /// faction's claim, or within [`NEAR_GROUND`] cells of one of its rooms. The nearest
    /// claim wins when it is beside two. `None` where no denizen holds ground nearby.
    pub fn denizen_ground_near(&self, x: f32, y: f32) -> Option<DenizenKind> {
        self.claims
            .iter()
            .filter_map(|claim| {
                let kind = self.faction(claim.faction).denizen?;
                let gap = claim.rooms.iter().map(|&r| self.rooms[r].gap_to(x, y)).fold(f32::MAX, f32::min);
                (gap <= NEAR_GROUND).then_some((gap, kind))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, kind)| kind)
    }

    /// Whether a spot at (`x`, `y`) lies by a shrine or in a faction's hall: within [`NEAR_GROUND`]
    /// cells of a shrine's room, or inside a room a faction holds. Prayer scrolls are commoner
    /// there ([`fb_items::prayers`]).
    pub fn holy_ground_near(&self, x: f32, y: f32) -> bool {
        self.rooms_for(crate::Purpose::Shrine).any(|r| self.rooms[r].gap_to(x, y) <= NEAR_GROUND)
            || self.claims.iter().any(|claim| claim.rooms.iter().any(|&r| self.rooms[r].gap_to(x, y) <= 0.0))
    }

    /// What the container prop `spot` holds as it is found, rolled for `odds`: nothing for
    /// scenery or a container set out empty ([`crate::NO_LOOT`]). The writing in a chest or crate
    /// by a denizen faction's ground may be in its tongue ([`fb_items::Item::in_world_near`]).
    pub fn container_contents(&self, spot: &crate::props::PropSpot, odds: fb_items::Odds, lore: &Lore) -> Vec<fb_items::Item> {
        let crate::props::Prop::Container { style, loot, .. } = spot.prop else { return Vec::new() };
        if loot == crate::NO_LOOT {
            return Vec::new();
        }
        let near = self.denizen_ground_near(spot.x, spot.y);
        let holy = self.holy_ground_near(spot.x, spot.y);
        fb_items::roll_contents(style, loot, odds).into_iter().map(|i| i.in_world_at(lore, near, holy)).collect()
    }

    /// The item lying at `spot` as it is found, rolled for `odds` and written on or put in a
    /// tongue as the level holds ([`fb_items::Item::in_world_near`]): a book or scroll by a
    /// denizen faction's ground may be written in its tongue. `lore` is `self.lore()`, passed in
    /// so a level's items share one.
    pub fn floor_item(&self, spot: &crate::ItemSpot, odds: fb_items::Odds, lore: &Lore) -> Option<fb_items::Item> {
        let item = fb_items::roll_loot(spot.family, spot.seed, odds)?;
        Some(item.in_world_at(lore, self.denizen_ground_near(spot.x, spot.y), self.holy_ground_near(spot.x, spot.y)))
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
        // Each book as it is found ([`Level::floor_item`]), so one written in a tongue, which
        // teaches words, is not told of as a lesson. Only the tongues matter to that.
        let found = Lore { depth: self.depth, tongues: self.tongues(), denizens: self.factions.iter().filter_map(|f| Some((f.name.clone(), f.denizen?))).collect(), ..Lore::default() };
        self.items
            .iter()
            .filter(|spot| spot.family == Family::Writing)
            .filter_map(|spot| {
                let teaches = self.floor_item(spot, fb_items::Odds::default(), &found)?.lesson()?;
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
        Profession::Healer => "healer",
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
        Folk::Minotaur => ("minotaur", "minotaurs"),
        Folk::Faun => ("faun", "fauns"),
        Folk::Cyclops => ("cyclops", "cyclopes"),
        Folk::WingedDemon => ("winged demon", "winged demons"),
        Folk::HornedDemon => ("horned demon", "horned demons"),
        Folk::Succubus => ("succubus", "succubi"),
    };
    Thing::new(one, many)
}

fn class(c: ClassKind) -> Thing {
    let (one, many) = match c {
        ClassKind::Fighter => ("fighter", "fighters"),
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
    fn writing_in_a_tongue_lies_by_denizen_ground_at_depth_only() {
        let (mut near, mut far, mut near_total, mut far_total) = (0, 0, 0, 0);
        for world in 1..=24 {
            for depth in 10..=11 {
                let level = crate::generate(crate::depth_seed(world, depth), depth);
                let lore = level.lore();
                for spot in level.items.iter().filter(|s| s.family == Family::Writing) {
                    let Some(item) = level.floor_item(spot, fb_items::Odds::default(), &lore) else { continue };
                    let ground = level.denizen_ground_near(spot.x, spot.y);
                    if let Some(kind) = item.tongue() {
                        // A level with no denizen faction rolls a tongue from all of them.
                        assert!(lore.tongues.is_empty() || lore.tongues.contains(&kind) || ground == Some(kind), "world {world} depth {depth}");
                        if let Some(g) = ground {
                            assert_eq!(kind, g, "by a faction's ground, in its tongue");
                        }
                    }
                    let writing = matches!(item.shape, armory::Item::Writing(ref w) if w.piece.kind != armory::families::writing::WritingKind::Note);
                    if !writing {
                        continue;
                    }
                    if ground.is_some() {
                        near_total += 1;
                        near += item.tongue().is_some() as u32;
                    } else {
                        far_total += 1;
                        far += item.tongue().is_some() as u32;
                    }
                }
            }
        }
        println!("books and scrolls in a tongue: near {near}/{near_total}, far {far}/{far_total}");
        assert!(near > 0, "some are written in a tongue by a faction's ground");
    }

    #[test]
    fn a_note_tells_of_a_book_as_it_is_found_so_never_a_lesson_a_tongue_replaced() {
        for world in 1..=12 {
            for depth in [10, 11] {
                let level = crate::generate(crate::depth_seed(world, depth), depth);
                let lore = level.lore();
                let lessons: Vec<_> = level.items.iter().filter(|s| s.family == Family::Writing).filter_map(|s| level.floor_item(s, fb_items::Odds::default(), &lore)?.lesson()).collect();
                let told: Vec<_> = lore.books.iter().map(|b| b.teaches).collect();
                assert_eq!(told, lessons, "world {world} depth {depth}");
            }
        }
    }

    #[test]
    fn a_shallow_delve_meets_a_rumour_or_two_in_nine_levels() {
        let mut met = Vec::new();
        for world in 1..=10 {
            let mut count = 0;
            for depth in 1..fb_denizens::DENIZEN_DEPTH {
                let level = crate::generate(crate::depth_seed(world, depth), depth);
                let lore = level.lore();
                for spot in &level.items {
                    let item = level.floor_item(spot, fb_items::Odds::default(), &lore);
                    count += item.is_some_and(|i| i.is_note() && fb_items::tongues::rumour(i.seed, depth).is_some()) as u32;
                    assert!(level.floor_item(spot, fb_items::Odds::default(), &lore).is_none_or(|i| i.teaching().is_none()), "nothing teaches words above the denizens");
                }
            }
            met.push(count);
        }
        let mean = met.iter().sum::<u32>() as f32 / met.len() as f32;
        println!("rumour notes met over nine levels: mean {mean}, worlds with none {}", met.iter().filter(|&&c| c == 0).count());
        assert!((0.2..=3.5).contains(&mean), "{mean}");
    }

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
        let name = fb_items::naming::folk_name(&mut procgen_rng::Mulberry32::new(spot.seed ^ crate::folk::NAME_SALT), spot.folk.key(), Some(spot.sex));
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
