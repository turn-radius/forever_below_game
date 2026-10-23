use super::*;
use std::collections::HashSet;

const WORLDS: std::ops::Range<u32> = 0..64;

#[test]
fn a_pool_is_twelve_to_twenty_and_deterministic() {
    for seed in WORLDS {
        let world = World::new(seed);
        assert!((POOL_SIZE.0 as usize..=POOL_SIZE.1 as usize).contains(&world.folk_count()), "world {seed}: {}", world.folk_count());
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
        for spec in World::new(seed).folk_factions() {
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
fn no_faction_contradicts_itself() {
    for seed in 0..300 {
        for spec in World::new(seed).factions() {
            assert_ne!(spec.outfits[0], spec.outfits[1], "{}: {:?}", spec.name, spec.outfits);
            for hate in &spec.hates {
                assert!(!spec.likes.contains(hate), "{} likes and hates {hate:?}", spec.name);
                assert_ne!(*hate, Preference::Faction(spec.id), "{} hates itself", spec.name);
                if let Preference::Class(c) = hate {
                    assert!(spec.outfits.iter().all(|o| o.class() != Some(*c)), "{} hates {c:?} and dresses in it", spec.name);
                }
            }
            assert!(!spec.likes.contains(&Preference::Faction(spec.id)), "{} likes itself", spec.name);
        }
    }
}

#[test]
fn factions_vary_across_a_world() {
    let world = World::new(1207);
    let folk: HashSet<Folk> = world.folk_factions().iter().map(|f| f.folk).collect();
    let intentions: HashSet<Intention> = world.folk_factions().iter().map(|f| f.intention).collect();
    assert!(folk.len() >= 4 && intentions.len() >= 3, "{folk:?} {intentions:?}");
}

#[test]
fn masters_work_from_level_six_one_to_a_trade_on_a_level() {
    let rules = TradeRules::DEFAULT;
    for seed in WORLDS {
        let world = World::new(seed);
        let masters = world.masters(&rules);
        assert_eq!(masters, world.masters(&rules));
        for depth in 1..=rules.deepest_master {
            let here: Vec<_> = masters.on(depth).collect();
            let kinds: HashSet<_> = here.iter().map(|m| m.profession).collect();
            assert_eq!(kinds.len(), here.len());
            assert!(here.len() <= 2 && (here.is_empty() || depth >= rules.master_from));
            for m in here {
                assert_eq!(m.style.is_some(), m.profession.is_smith());
            }
        }
        // Every trade has a master somewhere below, to hear a rumour of.
        for p in Profession::TRADES {
            assert!(masters.deeper(p, 1).is_some(), "world {seed}: no {p:?} master");
        }
    }
}

#[test]
fn trades_read_as_the_folk_card_writes_them() {
    let t = |profession, tier, style| Trade { profession, tier, style }.label();
    assert_eq!(t(Profession::Weaponsmith, Tier::Apprentice, Some(Style::Blades)), "Apprentice weaponsmith (blades)");
    assert_eq!(t(Profession::Alchemist, Tier::Master, None), "Master alchemist");
    assert_eq!(Trade::guard().label(), "Guard");
    assert!(Tier::Novice < Tier::Apprentice && Tier::Apprentice < Tier::Expert && Tier::Expert < Tier::Master);
}

// Denizen factions.

/// The folk factions (name and folk) that worlds rolled before denizen factions existed.
const BEFORE_DENIZENS: &[(u32, &[(&str, Folk)])] = &[
    (0, &[
        ("the Stone grudge-bearers", Folk::Dwarf),
        ("the Stonebeard Guild of Ale", Folk::Dwarf),
        ("the Lamp Chime", Folk::Gnome),
        ("the Crest Vigil", Folk::Lizardfolk),
        ("the Wardens of the Drowned Gate", Folk::Human),
        ("the Stonekin of the Sworn Halls", Folk::Dwarf),
        ("the Deephold brewers", Folk::Dwarf),
        ("the Rovers of the Sunken Road", Folk::Human),
        ("the Lost Fellowship", Folk::Human),
        ("the Spit Cackle", Folk::Goblin),
        ("the Stonetail traders", Folk::Lizardfolk),
        ("the Hiss of the Last Tail", Folk::Lizardfolk),
        ("the Lens Court", Folk::Gnome),
        ("the Morwick reavers", Folk::Human),
        ("the Brotherhood of the Ticking Hatch", Folk::Gnome),
        ("the Sons of the Forsaken Cinder", Folk::Dwarf),
        ("the Trufflepen raiders", Folk::Pigfolk),
    ]),
    (1, &[
        ("the Gnomish Brotherhood of Springs", Folk::Gnome),
        ("the Scale Synod", Folk::Lizardfolk),
        ("the Last Gate brotherhood", Folk::Human),
        ("the Thanes of the Sworn Door", Folk::Dwarf),
        ("the Granitefist factors", Folk::Dwarf),
        ("the Spindle Workshop", Folk::Gnome),
        ("the Elders of the Deep Gate", Folk::Dwarf),
        ("the Boltgimble troupe", Folk::Gnome),
        ("the Wardens of the Weeping Candle", Folk::Human),
        ("the Lens Court", Folk::Gnome),
        ("the Chant of the Forsaken Forge", Folk::Dwarf),
        ("the Bone-watchers", Folk::Goblin),
        ("the Black Concern of Ale", Folk::Human),
    ]),
    (9, &[
        ("the Watchers of the Bristled Midden", Folk::Pigfolk),
        ("the Elmvale league", Folk::Human),
        ("the League of the Honest Pen", Folk::Pigfolk),
        ("the Whelps of the Rotten Door", Folk::Goblin),
        ("the Grub Choir", Folk::Goblin),
        ("the Silent House", Folk::Dwarf),
        ("the Emerald Sun herbalists", Folk::Lizardfolk),
        ("the Venomous Brood", Folk::Lizardfolk),
        ("the Stonebrow ingot-sellers", Folk::Dwarf),
        ("the Wickwick magpies", Folk::Gnome),
        ("the Bristlepen traders", Folk::Pigfolk),
        ("the Rite of the Bloody Pit", Folk::Goblin),
        ("the Human League of Salt", Folk::Human),
        ("the Sunscale League of Eggs", Folk::Lizardfolk),
        ("the Bristle Sounder", Folk::Pigfolk),
        ("the Syndicate of the Little Workshop", Folk::Gnome),
        ("the Cutpurses of the Silent Stair", Folk::Human),
    ]),
    (1207, &[
        ("the Stonehelm wardens", Folk::Dwarf),
        ("the Grey Synod", Folk::Human),
        ("the Ratchet scavengers", Folk::Gnome),
        ("the Springnook troupe", Folk::Gnome),
        ("the Muddy Tuskers", Folk::Pigfolk),
        ("the Whistle Choir", Folk::Gnome),
        ("the Whirring Burrow troupe", Folk::Gnome),
        ("the Sunless Vigil", Folk::Lizardfolk),
        ("the Ash Rite", Folk::Human),
        ("the Chain Choir", Folk::Human),
        ("the Weary Acorn Chorus", Folk::Pigfolk),
        ("the Rune Choir", Folk::Dwarf),
        ("the Slimeclaw swappers", Folk::Goblin),
        ("the Tuskers of the Muddy Stye", Folk::Pigfolk),
        ("the Unbroken Forge factors", Folk::Dwarf),
        ("the Humming Circle", Folk::Gnome),
    ]),
];

#[test]
fn the_folk_factions_are_what_they_were_before_denizens() {
    for (seed, want) in BEFORE_DENIZENS {
        let world = World::new(*seed);
        let got: Vec<(&str, Folk)> = world.folk_factions().iter().map(|f| (f.name.as_str(), f.folk)).collect();
        assert_eq!(&got[..], *want, "world {seed}");
    }
}

#[test]
fn a_pool_holds_four_to_six_denizen_factions_after_the_folk_ones() {
    for seed in 0..200 {
        let world = World::new(seed);
        let denizens = world.denizen_factions();
        assert!((DENIZEN_POOL.0 as usize..=DENIZEN_POOL.1 as usize).contains(&denizens.len()), "world {seed}: {}", denizens.len());
        assert!(world.factions().len() <= u8::MAX as usize);
        assert!(world.folk_factions().iter().all(|f| f.denizen.is_none() && !f.is_denizen()));
        for (i, f) in denizens.iter().enumerate() {
            assert_eq!(f.id, Faction((world.folk_count() + i) as u8));
            assert!(f.is_denizen() && !f.insular);
        }
        // Every kind has a faction.
        for kind in DenizenKind::ALL {
            assert!(denizens.iter().any(|f| f.denizen == Some(kind)), "world {seed}: no {kind:?} faction");
        }
        assert_eq!(world, World::new(seed));
    }
}

#[test]
fn a_denizen_faction_is_made_of_its_kind() {
    let mut seen = HashSet::new();
    for seed in 0..64 {
        for spec in World::new(seed).denizen_factions() {
            let kind = spec.denizen.unwrap();
            let mut rng = Mulberry32::new(seed);
            for _ in 0..60 {
                let body = spec.member_body(&mut rng).expect("a denizen faction has bodies");
                assert_eq!(body.kind(), kind, "{}", spec.name);
                seen.insert(body);
            }
        }
        for spec in World::new(seed).folk_factions() {
            assert_eq!(spec.member_body(&mut Mulberry32::new(seed)), None, "{}", spec.name);
        }
    }
    assert_eq!(seen.len(), DenizenBody::ALL.len(), "every body turns up: {seen:?}");
}

#[test]
fn denizen_alignments_fit_their_kind() {
    let (mut demons, mut evil, mut fauns, mut chaotic_kind) = (0, 0, 0, 0);
    for seed in 0..200 {
        for spec in World::new(seed).denizen_factions() {
            match spec.denizen.unwrap() {
                DenizenKind::Demon => {
                    demons += 1;
                    evil += matches!(spec.alignment, Alignment::LawfulEvil | Alignment::NeutralEvil | Alignment::ChaoticEvil) as u32;
                }
                DenizenKind::Faun => {
                    fauns += 1;
                    chaotic_kind += matches!(spec.alignment, Alignment::ChaoticGood | Alignment::ChaoticNeutral) as u32;
                    assert!(!matches!(spec.alignment, Alignment::LawfulEvil | Alignment::NeutralEvil | Alignment::ChaoticEvil), "{}", spec.name);
                }
                _ => {}
            }
        }
    }
    assert!(evil * 10 >= demons * 8, "{evil} of {demons} demon factions are evil");
    assert!(chaotic_kind * 10 >= fauns * 6, "{chaotic_kind} of {fauns} fauns lean chaotic");
}

#[test]
fn denizen_names_are_unique_and_follow_their_kind() {
    let mut kinds_seen: std::collections::HashMap<DenizenKind, HashSet<String>> = Default::default();
    for seed in 0..200 {
        let world = World::new(seed);
        for spec in world.denizen_factions() {
            assert!(spec.name.starts_with("the ") && !spec.name.contains('#'), "{}", spec.name);
            kinds_seen.entry(spec.denizen.unwrap()).or_default().insert(spec.name.clone());
        }
    }
    for kind in DenizenKind::ALL {
        assert!(kinds_seen[&kind].len() > 30, "{kind:?} gave {} names", kinds_seen[&kind].len());
    }
}

#[test]
fn denizen_tastes_reach_across_factions_and_folk() {
    let (mut hate_folk, mut like_folk_factions, mut hate_folk_factions) = (0, 0, 0);
    for seed in 0..64 {
        let world = World::new(seed);
        let is_folk_faction = |p: &Preference| matches!(p, Preference::Faction(f) if !world.faction(*f).is_denizen());
        for spec in world.denizen_factions() {
            hate_folk += spec.hates.iter().any(|p| matches!(p, Preference::Folk(_))) as u32;
            like_folk_factions += spec.likes.iter().any(is_folk_faction) as u32;
            hate_folk_factions += spec.hates.iter().any(is_folk_faction) as u32;
            assert!(spec.hates.iter().chain(&spec.likes).all(|p| *p != Preference::Faction(spec.id)));
            assert!(spec.likes.iter().all(|p| !spec.hates.contains(p)));
            assert!((1..=3).contains(&spec.likes.len()) && (1..=3).contains(&spec.hates.len()), "{}", spec.name);
        }
        for spec in world.folk_factions() {
            assert_eq!(spec.hated_folk().count() > 0, spec.insular, "{}", spec.name);
        }
    }
    assert!(hate_folk > 20 && like_folk_factions > 20 && hate_folk_factions > 20, "{hate_folk} {like_folk_factions} {hate_folk_factions}");
}

#[test]
fn folk_factions_rarely_like_or_hate_a_denizen_faction() {
    let (mut factions, mut with_taste) = (0, 0);
    for seed in 0..200 {
        let world = World::new(seed);
        for spec in world.folk_factions() {
            factions += 1;
            let tastes = spec.likes.iter().chain(&spec.hates).filter(|p| matches!(p, Preference::Faction(f) if world.faction(*f).is_denizen())).count();
            with_taste += (tastes > 0) as u32;
            assert!((1..=3).contains(&spec.likes.len()) && (1..=3).contains(&spec.hates.len()), "{}", spec.name);
            assert!(spec.likes.iter().all(|p| !spec.hates.contains(p)), "{}", spec.name);
            assert_eq!(spec.hated_folk().count() > 0, spec.insular, "{}", spec.name);
        }
    }
    // 5 in 100 for each of four to six denizen factions, a good share of them replaced again.
    let share = with_taste as f64 / factions as f64;
    assert!((0.1..0.4).contains(&share), "{with_taste} of {factions} folk factions have a denizen taste");
}

#[test]
fn above_the_denizen_depth_a_level_draws_exactly_the_folk_draw() {
    for seed in WORLDS {
        let world = World::new(seed);
        for level in [seed, seed.wrapping_add(7), 1207] {
            for depth in 1..DENIZEN_DEPTH {
                assert_eq!(world.draw_at(level, depth), world.draw(level));
                assert_eq!(world.draw_specs_at(level, depth), world.draw_specs(level));
            }
            // The draw is what it was before denizens: the same shuffle of the folk factions.
            let mut rng = Mulberry32::new(mix(world.seed, DRAW_SALT, level));
            let count = (rng.int(PER_LEVEL.0, PER_LEVEL.1) as usize).min(world.folk_count());
            let mut ids: Vec<Faction> = (0..world.folk_count()).map(|i| Faction(i as u8)).collect();
            shuffle(&mut ids, &mut rng);
            ids.truncate(count);
            assert_eq!(world.draw(level), ids);
            assert!(world.draw_specs(level).iter().all(|f| f.denizen.is_none()));
        }
    }
}

#[test]
fn deeper_levels_draw_the_folk_and_usually_one_denizen_faction() {
    let (mut levels, mut with_one, mut with_two) = (0, 0, 0);
    for seed in WORLDS {
        let world = World::new(seed);
        for level in 0..30 {
            for depth in [DENIZEN_DEPTH, DENIZEN_DEPTH + 4] {
                let drawn = world.draw_at(level, depth);
                assert_eq!(drawn, world.draw_at(level, depth));
                assert_eq!(&drawn[..world.draw(level).len()], &world.draw(level)[..], "the folk come first, as ever");
                let distinct: HashSet<_> = drawn.iter().collect();
                assert_eq!(distinct.len(), drawn.len());
                let denizens = drawn.iter().filter(|f| world.faction(**f).is_denizen()).count();
                assert!(denizens <= 2 && drawn.len() - denizens == world.draw(level).len());
                levels += 1;
                with_one += (denizens >= 1) as u32;
                with_two += (denizens == 2) as u32;
            }
        }
    }
    assert!(with_one * 10 >= levels * 7, "{with_one} of {levels} levels have a denizen faction");
    assert!(with_two > 0 && with_two * 3 < with_one, "{with_two} of {with_one} have two");
}
