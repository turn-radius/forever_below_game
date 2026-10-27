use std::collections::BTreeSet;

use super::*;
use crate::creatures::{CREATURE_SALT, TROLLS_FROM_DEPTH, place_creatures};
use crate::generate::{generate, generate_without_creatures};
use crate::stairs::{STAIR_SALT, place_stairs};
use crate::LevelParams;

fn count(level: &Level, species: Species) -> usize {
    level.creatures.iter().filter(|c| c.species == species).count()
}

fn room_of(level: &Level, c: &CreatureSpot) -> Option<usize> {
    level.room_at(c.x.floor() as i32, c.y.floor() as i32)
}

/// The level as the generator laid it out before the newer species: creatures from the creature
/// stream, then the stairs.
fn before(seed: u32, depth: u32) -> Level {
    let params = LevelParams::for_depth(depth);
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
            // Rats, snakes and surplus skeleton packs may make way for the newcomers (#662); none
            // that stays moves, and every troll stays.
            let kept: Vec<CreatureSpot> = new.creatures.iter().filter(|c| !newcomer(c.species)).copied().collect();
            let mut rest = old.creatures.iter();
            assert!(kept.iter().all(|k| rest.any(|o| o == k)), "seed {seed} depth {depth}: creatures moved");
            let trolls = |l: &[CreatureSpot]| l.iter().filter(|c| c.species == Species::Troll).count();
            assert_eq!(trolls(&kept), trolls(&old.creatures), "seed {seed} depth {depth}: a troll made way");
            // The crystals of a cavern and the lights of its largest come after everything else.
            let bare = |mut l: Level| {
                l.creatures.clear();
                l.crystals.clear();
                l.items.retain(|i| i.family != armory::Family::Crystal);
                l.lights.retain(|light| light.kind != crate::LightKind::Crystal);
                l
            };
            assert_eq!(Level { webs: Vec::new(), ..bare(new) }, bare(old), "seed {seed} depth {depth}: the level moved");
        }
    }
}

#[test]
fn each_species_appears_only_from_its_depth() {
    let found = |species: Species, depth: u32| (1..=60).any(|seed| count(&generate(seed, depth), species) > 0);
    let none = |species: Species, depths: std::ops::Range<u32>| depths.for_each(|d| (1..=60).for_each(|s| assert_eq!(count(&generate(s, d), species), 0, "{species:?} at seed {s} depth {d}")));
    for species in [Species::Bat, Species::Jackalope] {
        assert!(found(species, 1), "{species:?} on the first level");
    }
    none(Species::GiantCentipede, 1..CENTIPEDES_FROM_DEPTH);
    assert!(found(Species::GiantCentipede, CENTIPEDES_FROM_DEPTH));
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
            let mut level = generate(seed, depth);
            // The crystals' lights come after the creatures and stand at the crystals, which keep clear of them.
            level.lights.retain(|l| l.kind != crate::LightKind::Crystal);
            level.items.retain(|i| i.family != armory::Family::Crystal);
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
    let jackalopes = holding(1..81, 3, Species::Jackalope);
    assert!((1..=24).contains(&jackalopes), "{jackalopes} of 80");
    let trolls = holding(1..81, 7, Species::Troll);
    assert!((10..=45).contains(&trolls), "{trolls} of 80");
}

#[test]
#[ignore = "sweeps hundreds of levels"]
fn jackalope_and_troll_frequencies() {
    // The jackalope is counted on the third level: the rough-rock levels (the even ones) have few rooms the others leave free, so it is rare there (#662).
    // About one level in ten, and a little under one in three: a troll's odds are one in three,
    // but now and then folk hold the only hall large enough for it.
    let jackalopes = holding(1..401, 3, Species::Jackalope);
    assert!((20..=62).contains(&jackalopes), "{jackalopes} of 400 levels hold a jackalope");
    let trolls = holding(1..401, 7, Species::Troll);
    assert!((75..=160).contains(&trolls), "{trolls} of 400 levels hold a troll");
}

/// Each group of creatures (a pack) with the room it stands in and its members, over a level.
fn packs(level: &Level) -> Vec<(u32, Option<usize>, Vec<Species>)> {
    let mut packs: Vec<(u32, Option<usize>, Vec<Species>)> = Vec::new();
    for c in &level.creatures {
        match packs.iter_mut().find(|p| p.0 == c.pack) {
            Some(p) => p.2.push(c.species),
            None => packs.push((c.pack, room_of(level, c), vec![c.species])),
        }
    }
    packs
}

/// Checks #662 on one level: no room holds a wild group and another group, and no room with a
/// wild group holds more creatures than its floor does.
fn assert_spread(level: &Level) {
    let (seed, depth) = (level.seed, level.depth);
    for (r, room) in level.rooms.iter().enumerate() {
        let here: Vec<_> = packs(level).into_iter().filter(|p| p.1 == Some(r)).collect();
        assert!(here.len() <= 1, "seed {seed} depth {depth}: room {r} holds {} groups", here.len());
        if here.iter().any(|p| p.2.iter().any(|&s| newcomer(s))) {
            assert_eq!(here.len(), 1, "seed {seed} depth {depth}: room {r} holds {} groups", here.len());
            let cap = (room.area() / CELLS_PER_CREATURE).max(1) as usize;
            assert!(here[0].2.len() <= cap, "seed {seed} depth {depth}: {} creatures in the {} cells of room {r}", here[0].2.len(), room.area());
        }
    }
}

fn sweep_spread(seeds: std::ops::RangeInclusive<u32>) {
    for depth in 1..=4 {
        for seed in seeds.clone() {
            assert_spread(&generate(seed, depth));
        }
    }
}

#[test]
fn wild_groups_keep_a_room_each_smoke() {
    sweep_spread(1..=25);
}

#[test]
#[ignore]
fn wild_groups_keep_a_room_each() {
    sweep_spread(1..=300);
}

/// How many large rooms the level's own creatures and folk leave free for the wild species.
fn spare_rooms(level: &Level) -> usize {
    let old = before(level.seed, level.depth);
    let start = old.start_room();
    (0..old.rooms.len())
        .filter(|&i| {
            let stair = old.stairs.iter().any(|s| {
                let (ax, ay) = s.approach();
                old.room_at(s.x as i32, s.y as i32) == Some(i) || old.room_at(ax as i32, ay as i32) == Some(i)
            });
            Some(i) != start && old.rooms[i].area() >= MIN_ROOM_AREA && !stair && !has_folk(&old, i) && !old.creatures.iter().any(|c| room_of(&old, c) == Some(i))
        })
        .count()
}

#[test]
fn levels_with_rooms_to_spare_keep_their_wild_groups() {
    let (mut levels, mut bats, mut centipedes, mut spiders) = (0, 0, 0, 0);
    for seed in 1..=300 {
        let level = generate(seed, SPIDERS_FROM_DEPTH);
        if spare_rooms(&level) < 3 {
            continue;
        }
        levels += 1;
        let has = |f: fn(Species) -> bool| level.creatures.iter().any(|c| f(c.species)) as u32;
        bats += has(Species::is_bat);
        centipedes += has(Species::is_centipede);
        spiders += has(Species::is_spider);
    }
    assert!(levels >= 20, "{levels} levels with room to spare");
    // Before #662: bats on 0.4 of levels, centipedes on all, spiders on 0.48.
    let rate = |n: u32| n as f64 / levels as f64;
    assert!(rate(bats) >= 0.3, "bats on {bats} of {levels}");
    assert!(rate(centipedes) >= 0.9, "centipedes on {centipedes} of {levels}");
    assert!(rate(spiders) >= 0.35, "spiders on {spiders} of {levels}");
}

/// The level as it stands when the wild species are about to be placed: the creatures made room
/// for them, the stairs in.
fn ready(seed: u32, depth: u32) -> Level {
    let mut level = before(seed, depth);
    make_room_for_wild(&mut level);
    level
}

#[test]
fn rooms_opening_onto_no_occupied_room_are_offered_first() {
    let (mut mixed, mut checked) = (0, 0);
    for depth in 1..=4 {
        for seed in 1..=150 {
            let level = ready(seed, depth);
            let p = Placer { level: &level, start_room: level.start_room(), placed: level.creatures.clone(), pack: 0, links: room_links(&level) };
            let rooms = p.rooms(MIN_ROOM_AREA, &mut Mulberry32::new(seed));
            let beside: Vec<bool> = rooms.iter().map(|&i| p.next_to_held(i)).collect();
            checked += 1;
            assert!(beside.windows(2).all(|w| !w[0] || w[1]), "seed {seed} depth {depth}: a room beside an occupied one comes before one that is not");
            mixed += (beside.contains(&true) && beside.contains(&false)) as u32;
        }
    }
    assert!(mixed >= 20, "only {mixed} of {checked} levels offered both kinds of room");
}

#[test]
fn links_are_direct_neighbours() {
    let (mut rooms, mut linked) = (0, 0);
    for seed in 1..=60 {
        let level = generate(seed, 2);
        for (i, l) in room_links(&level).iter().enumerate() {
            assert!(!l.contains(&i));
            rooms += 1;
            linked += (l.len() > 3) as u32;
        }
    }
    assert!(linked * 4 < rooms, "{linked} of {rooms} rooms open onto more than three others");
}

#[test]
fn the_plan_is_what_is_placed() {
    let mut roomy = 0;
    for depth in 1..=4 {
        for seed in 1..=150 {
            let level = generate(seed, depth);
            let plan = Plan::of(&level);
            let placed = |f: fn(Species) -> bool| level.creatures.iter().any(|c| f(c.species));
            // Nothing is placed that the plan did not ask for.
            assert!(!placed(Species::is_spider) || plan.spiders.is_some(), "seed {seed} depth {depth}: spiders unplanned");
            assert!(!placed(Species::is_bat) || plan.bats.is_some(), "seed {seed} depth {depth}: bats unplanned");
            assert!(!placed(Species::is_jackalope) || plan.jackalope.is_some(), "seed {seed} depth {depth}: a jackalope unplanned");
            let cent: BTreeSet<u32> = level.creatures.iter().filter(|c| c.species.is_centipede()).map(|c| c.pack).collect();
            assert!(cent.len() as u32 <= plan.centipedes.0, "seed {seed} depth {depth}: {} centipede groups of {}", cent.len(), plan.centipedes.0);
            // And with rooms in plenty the spiders and centipedes it asked for are there.
            if spare_rooms(&level) >= 4 {
                roomy += 1;
                assert_eq!(placed(Species::is_spider), plan.spiders.is_some(), "seed {seed} depth {depth}: spiders");
                assert_eq!(cent.len() as u32, plan.centipedes.0, "seed {seed} depth {depth}: centipedes");
            }
        }
    }
    assert!(roomy >= 5, "{roomy} levels with rooms in plenty");
}

/// Before the wild species, the level's own creatures share a room only where a skeleton pack
/// and a rat pack or snake do (the stone halls, where the rats fall back on the guarded halls):
/// the odd levels and the first never double up, so `make_room_for_wild` has nothing to undo there.
#[test]
fn only_skeleton_rooms_are_shared_before_the_wild_species() {
    for depth in 1..=4 {
        for seed in 1..=100 {
            let level = before(seed, depth);
            for r in 0..level.rooms.len() {
                let here: Vec<_> = level.creatures.iter().filter(|c| room_of(&level, c) == Some(r)).collect();
                let packs: BTreeSet<u32> = here.iter().map(|c| c.pack).collect();
                assert!(packs.len() < 2 || here.iter().any(|c| c.species.is_skeleton()), "seed {seed} depth {depth}: room {r} holds {} groups and no skeleton", packs.len());
            }
        }
    }
}

/// Giving every group a room costs the small levels creatures; this keeps the cost from growing
/// unnoticed. Measured over seeds 1 to 300 at the time (#662): the even depths, which have the
/// fewest free rooms, held 2 on their barest level and 7.4 and 5.9 on average.
#[test]
fn small_levels_keep_a_crowd_of_creatures() {
    for depth in [2, 4] {
        let counts: Vec<usize> = (1..=150).map(|s| generate(s, depth).creatures.len()).collect();
        let (least, mean) = (*counts.iter().min().unwrap(), counts.iter().sum::<usize>() as f64 / counts.len() as f64);
        assert!(least >= 2, "depth {depth}: a level holds only {least} creatures");
        assert!(mean >= 5.0, "depth {depth}: {mean:.2} creatures a level");
    }
    // Seed 1207's first level keeps its two rats.
    let species: Vec<_> = generate(1207, 1).creatures.iter().map(|c| c.species).collect();
    assert_eq!(species, [Species::Rat, Species::Rat]);
}

