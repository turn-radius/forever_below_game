use super::*;
use std::collections::HashSet;

const WORLDS: std::ops::Range<u32> = 0..64;

#[test]
fn a_pool_is_twelve_to_twenty_and_deterministic() {
    for seed in WORLDS {
        let world = World::new(seed);
        assert!((POOL_SIZE.0 as usize..=POOL_SIZE.1 as usize).contains(&world.factions().len()), "world {seed}: {}", world.factions().len());
        assert_eq!(world, World::new(seed));
        for (i, f) in world.factions().iter().enumerate() {
            assert_eq!(f.id, Faction(i as u8));
        }
    }
    assert_ne!(World::new(1), World::new(2));
}

#[test]
fn a_level_draws_two_to_four_the_same_each_time() {
    for seed in WORLDS {
        let world = World::new(seed);
        for level in [seed, seed.wrapping_add(7), 1207] {
            let drawn = world.draw(level);
            assert!((2..=4).contains(&drawn.len()), "{drawn:?}");
            assert_eq!(drawn, world.draw(level));
            let distinct: HashSet<_> = drawn.iter().collect();
            assert_eq!(distinct.len(), drawn.len());
        }
    }
    let world = World::new(5);
    assert!((0..50).any(|l| world.draw(l) != world.draw(l + 1)), "every level draws alike");
}

#[test]
fn a_faction_is_one_spec_on_every_level() {
    let world = World::new(9);
    let mut seen = std::collections::HashMap::new();
    for level in 0..40 {
        for spec in world.draw_specs(level) {
            assert_eq!(*seen.entry(spec.id).or_insert_with(|| spec.clone()), spec);
            assert_eq!(&spec, world.faction(spec.id));
        }
    }
    assert!(seen.len() > 6, "40 levels drew only {} factions", seen.len());
}

#[test]
fn names_are_unique_in_a_pool() {
    for seed in 0..200 {
        let world = World::new(seed);
        let names: HashSet<&str> = world.factions().iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names.len(), world.factions().len(), "world {seed}");
        assert!(world.factions().iter().all(|f| f.name != "the Nameless" && !f.name.is_empty()));
    }
}

#[test]
fn insular_factions_are_wholly_their_folk() {
    let mut insular = 0;
    for seed in WORLDS {
        for spec in World::new(seed).factions() {
            let mut rng = Mulberry32::new(seed);
            let members: Vec<Folk> = (0..200).map(|_| spec.member_folk(&mut rng)).collect();
            if spec.insular {
                insular += 1;
                assert!(members.iter().all(|f| *f == spec.folk), "{}", spec.name);
                assert!(spec.hated_folk().any(|f| f != spec.folk), "{} hates no other folk", spec.name);
            } else {
                assert_eq!(spec.hated_folk().count(), 0, "{}", spec.name);
                let own = members.iter().filter(|f| **f == spec.folk).count();
                assert!((140..=200).contains(&own), "{}: {own} of 200 are its folk", spec.name);
                assert!(members.iter().any(|f| *f != spec.folk), "{} is never mixed", spec.name);
            }
        }
    }
    assert!(insular > 20, "only {insular} insular factions in 64 worlds");
}

#[test]
fn every_field_keeps_the_rule_of_tincture() {
    for seed in WORLDS {
        for spec in World::new(seed).factions() {
            let f = spec.field;
            assert!(f.painted && f.division != Division::Plain, "{}", spec.name);
            assert_ne!(f.tincture.is_metal(), f.tincture2.is_metal(), "{}: {:?} on {:?}", spec.name, f.tincture, f.tincture2);
            assert_ne!(f.charge, armory::sections::device::Device::None, "{} has no crest", spec.name);
            assert_ne!(f.charge_tincture, f.tincture);
            assert_ne!(f.charge_tincture, f.tincture2);
        }
    }
}

#[test]
fn likes_and_hates_are_one_to_three_and_never_both() {
    for seed in WORLDS {
        let world = World::new(seed);
        for spec in world.factions() {
            assert!((1..=3).contains(&spec.likes.len()), "{}: {:?}", spec.name, spec.likes);
            assert!((1..=3).contains(&spec.hates.len()), "{}: {:?}", spec.name, spec.hates);
            assert!(spec.likes.iter().all(|p| !spec.hates.contains(p)), "{}", spec.name);
            for p in spec.likes.iter().chain(&spec.hates) {
                if let Preference::Faction(f) = p {
                    assert_ne!(*f, spec.id, "{} thinks of itself", spec.name);
                    assert!((f.0 as usize) < world.factions().len());
                }
            }
        }
    }
}

#[test]
fn factions_vary_across_a_world() {
    let world = World::new(1207);
    let folk: HashSet<Folk> = world.factions().iter().map(|f| f.folk).collect();
    let intentions: HashSet<Intention> = world.factions().iter().map(|f| f.intention).collect();
    assert!(folk.len() >= 4 && intentions.len() >= 3, "{folk:?} {intentions:?}");
}
