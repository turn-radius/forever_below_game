use super::*;
use crate::creatures::{CREATURE_SALT, TROLLS_FROM_DEPTH, place_creatures};
use crate::generate::{generate, generate_without_creatures};
use crate::stairs::{STAIR_SALT, place_stairs};
use crate::{LevelParams, LevelStyle};

fn count(level: &Level, species: Species) -> usize {
    level.creatures.iter().filter(|c| c.species == species).count()
}

fn room_of(level: &Level, c: &CreatureSpot) -> Option<usize> {
    level.room_at(c.x.floor() as i32, c.y.floor() as i32)
}

/// The level as the generator laid it out before the newer species: creatures from the creature
/// stream, then the stairs.
fn before(seed: u32, depth: u32) -> Level {
    let params = LevelParams::for_style(LevelStyle::for_depth(depth));
    let (mut level, layout_seed) = generate_without_creatures(seed, depth, &params);
    level.creatures = place_creatures(&level, &mut Mulberry32::new(layout_seed ^ CREATURE_SALT));
    place_stairs(&mut level, &mut Mulberry32::new(layout_seed ^ STAIR_SALT));
    level
}

fn newcomer(s: Species) -> bool {
    s.is_bat() || s.is_spider() || s.is_jackalope() || s.is_centipede()
}

#[test]
fn placement_is_deterministic() {
    for (seed, depth) in [(1, 1), (1207, 3), (77, 6), (u32::MAX, 8)] {
        let (a, b) = (generate(seed, depth), generate(seed, depth));
        assert_eq!(a.creatures, b.creatures);
        assert_eq!(a.webs, b.webs);
    }
}

#[test]
fn nothing_that_was_there_before_moves() {
    for depth in 1..=8 {
        for seed in 1..=50 {
            let (old, new) = (before(seed, depth), generate(seed, depth));
            assert!(old.webs.is_empty());
            assert_eq!(new.creatures[..old.creatures.len()], old.creatures[..], "seed {seed} depth {depth}: creatures moved");
            assert_eq!(Level { creatures: Vec::new(), webs: Vec::new(), ..new }, Level { creatures: Vec::new(), ..old }, "seed {seed} depth {depth}: the level moved");
        }
    }
}

#[test]
fn each_species_appears_only_from_its_depth() {
    let found = |species: Species, depth: u32| (1..=60).any(|seed| count(&generate(seed, depth), species) > 0);
    let none = |species: Species, depths: std::ops::Range<u32>| depths.for_each(|d| (1..=60).for_each(|s| assert_eq!(count(&generate(s, d), species), 0, "{species:?} at seed {s} depth {d}")));
    for species in [Species::GiantCentipede, Species::Bat, Species::Jackalope] {
        assert!(found(species, 1), "{species:?} on the first level");
    }
    none(Species::GiantSpider, 1..SPIDERS_FROM_DEPTH);
    assert!(found(Species::GiantSpider, SPIDERS_FROM_DEPTH));
    none(Species::GiantBat, 1..GIANT_BATS_FROM_DEPTH);
    assert!(found(Species::GiantBat, GIANT_BATS_FROM_DEPTH));
    // Giant bats take the flock's place under a high ceiling; under a low one the flock stays.
    for depth in GIANT_BATS_FROM_DEPTH..GIANT_BATS_FROM_DEPTH + 3 {
        for seed in 1..=60 {
            let level = generate(seed, depth);
            let shut_out = if level.ceiling >= GIANT_BAT_CEILING { Species::Bat } else { Species::GiantBat };
            assert_eq!(count(&level, shut_out), 0, "{shut_out:?} at seed {seed} depth {depth} under a {} ceiling", level.ceiling);
        }
    }
    none(Species::Troll, 1..TROLLS_FROM_DEPTH);
    assert!(found(Species::Troll, TROLLS_FROM_DEPTH));
}

#[test]
fn the_newcomers_keep_to_their_rules() {
    for depth in 1..=8 {
        for seed in 1..=40 {
            let level = generate(seed, depth);
            let start_room = level.start_room();
            for (i, c) in level.creatures.iter().enumerate().filter(|(_, c)| newcomer(c.species)) {
                let (x, y) = (c.x.floor() as i32, c.y.floor() as i32);
                let what = format!("seed {seed} depth {depth}: {:?}", c.species);
                let room = room_of(&level, c).unwrap_or_else(|| panic!("{what} in a corridor"));
                assert_ne!(Some(room), start_room, "{what} in the start room");
                assert!(!by_door(&level, x, y) && level.cell(x, y) == Cell::Floor, "{what} by a door");
                assert!(level.stair_at(x, y).is_none(), "{what} on a stair");
                assert!(!level.stairs.iter().any(|s| s.approach() == (x as u32, y as u32)), "{what} on a stair's approach");
                // Nobody arriving by a stair walks straight into one.
                let stair_rooms: Vec<Option<usize>> = level.stairs.iter().flat_map(|s| [level.room_at(s.x as i32, s.y as i32), level.room_at(s.approach().0 as i32, s.approach().1 as i32)]).collect();
                assert!(!stair_rooms.contains(&Some(room)), "{what} in a stair's room");
                assert!(fits(&level, &level.rooms[room], c.x, c.y, c.creature().radius(), &level.creatures[..i]), "{what} crowds something");
                if c.species.is_spider() || c.species.is_jackalope() {
                    assert!(!level.folk.iter().any(|f| level.room_at(f.x as i32, f.y as i32) == Some(room)), "{what} in a room with folk");
                }
                if c.species.is_bat() {
                    assert!(level.rooms[room].area() >= BAT_ROOM_AREA, "{what} in a small room");
                }
                // A pack stays in one room.
                assert!(level.creatures.iter().filter(|o| o.pack == c.pack).all(|o| room_of(&level, o) == Some(room)), "{what}: a pack across rooms");
            }
            assert!(count(&level, Species::Bat) <= FLOCK.1 as usize);
            assert!(count(&level, Species::GiantBat) <= GIANT_BATS.1 as usize);
            assert!(count(&level, Species::GiantSpider) <= 2);
            assert!(count(&level, Species::Jackalope) <= 1);
            assert!(count(&level, Species::Troll) <= 1);
        }
    }
}

#[test]
fn webs_hang_only_in_spider_rooms_and_never_in_a_doorway_or_the_start_room() {
    let mut nests = 0;
    for depth in 1..=8 {
        for seed in 1..=60 {
            let level = generate(seed, depth);
            let spider_rooms: Vec<Option<usize>> = level.creatures.iter().filter(|c| c.species.is_spider()).map(|c| room_of(&level, c)).collect();
            if spider_rooms.is_empty() {
                assert!(level.webs.is_empty(), "seed {seed} depth {depth}: webs without spiders");
                continue;
            }
            nests += 1;
            assert!(spider_rooms.iter().all(|r| *r == spider_rooms[0]), "seed {seed} depth {depth}: spiders in two rooms");
            for w in &level.webs {
                let (x, y) = (w.x.floor() as i32, w.y.floor() as i32);
                assert_eq!(level.room_at(x, y), spider_rooms[0], "seed {seed} depth {depth}: {w:?} outside the nest");
                assert_ne!(level.room_at(x, y), level.start_room());
                assert_eq!(level.cell(x, y), Cell::Floor);
                assert!(!by_door(&level, x, y), "seed {seed} depth {depth}: {w:?} in a doorway");
                assert!(level.stair_at(x, y).is_none());
                assert!(w.size > 0.0 && w.height >= 0.0 && w.height <= level.ceiling);
            }
        }
    }
    assert!(nests >= 20, "only {nests} nests");
}

#[test]
fn nests_are_webbed_with_each_kind_of_web() {
    let has = |k: WebKind| (1..=60).any(|seed| generate(seed, 5).webs.iter().any(|w| w.kind == k));
    assert!(has(WebKind::Corner) && has(WebKind::Ceiling) && has(WebKind::Floor));
}

/// Levels at `depth` over `seeds` holding `species`.
fn holding(seeds: std::ops::Range<u32>, depth: u32, species: Species) -> usize {
    seeds.filter(|&s| count(&generate(s, depth), species) > 0).count()
}

#[test]
fn jackalope_and_troll_frequencies_smoke() {
    let jackalopes = holding(1..81, 2, Species::Jackalope);
    assert!((1..=24).contains(&jackalopes), "{jackalopes} of 80");
    let trolls = holding(1..81, 7, Species::Troll);
    assert!((10..=45).contains(&trolls), "{trolls} of 80");
}

#[test]
#[ignore = "sweeps hundreds of levels"]
fn jackalope_and_troll_frequencies() {
    // About one level in ten, and about one in three.
    let jackalopes = holding(1..401, 2, Species::Jackalope);
    assert!((20..=62).contains(&jackalopes), "{jackalopes} of 400 levels hold a jackalope");
    let trolls = holding(1..401, 7, Species::Troll);
    assert!((100..=160).contains(&trolls), "{trolls} of 400 levels hold a troll");
}
