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
fn no_faction_contradicts_itself() {
    for seed in 0..300 {
        for spec in World::new(seed).factions() {
            if spec.classes.len() != 1 {
                assert_ne!(spec.outfits[0], spec.outfits[1], "{}: {:?}", spec.name, spec.outfits);
            }
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

fn words(name: &str) -> Vec<String> {
    name.split(|c: char| !c.is_alphanumeric()).map(str::to_lowercase).collect()
}

/// Every `[key:value]Word` rule of `data/faction_names.json`, as (lowercase word, key, value),
/// so these tests follow the data and not a word list of their own.
fn tagged_words() -> Vec<(String, String, String)> {
    fn walk(v: &serde_json::Value, out: &mut Vec<(String, String, String)>) {
        match v {
            serde_json::Value::String(s) => {
                // A word may carry several tags.
                let mut rest = s.as_str();
                let mut set = Vec::new();
                while let Some((tag, after)) = rest.strip_prefix('[').and_then(|r| r.split_once(']')) {
                    if let Some((key, value)) = tag.split_once(':') {
                        set.push((key.to_string(), value.to_string()));
                    }
                    rest = after;
                }
                for (key, value) in set {
                    out.push((rest.to_lowercase(), key, value));
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|i| walk(i, out)),
            serde_json::Value::Object(map) => map.values().for_each(|i| walk(i, out)),
            _ => {}
        }
    }
    let data: serde_json::Value = serde_json::from_str(include_str!("../../../data/faction_names.json")).unwrap();
    let mut out = Vec::new();
    walk(&data, &mut out);
    out
}

/// The distinct values the words of `name` give `key`.
fn said(name: &str, key: &str) -> Vec<String> {
    let w = words(name);
    let mut out: Vec<String> = tagged_words().into_iter().filter(|(word, k, _)| k == key && w.contains(word)).map(|(_, _, v)| v).collect();
    out.sort();
    out.dedup();
    out
}

#[test]
fn a_name_that_says_a_class_dresses_every_member_so() {
    let (mut clerics, mut martial, mut knights, mut unsaid) = (0, 0, 0, 0);
    for seed in 0..300 {
        for spec in World::new(seed).factions() {
            let classes = said(&spec.name, "class");
            match &spec.classes[..] {
                [ClassKind::Cleric] => {
                    clerics += 1;
                    assert_eq!(classes, ["cleric"], "{}", spec.name);
                    assert_eq!(spec.outfits, [Outfit::Cleric; 2], "{}", spec.name);
                    assert!(!spec.hates.contains(&Preference::Class(ClassKind::Cleric)), "{} hates clerics", spec.name);
                }
                [ClassKind::Fighter] => {
                    knights += 1;
                    assert_eq!(classes, ["fighter"], "{}", spec.name);
                    assert_eq!(spec.outfits, [Outfit::Fighter; 2], "{}", spec.name);
                    assert!(!spec.hates.contains(&Preference::Class(ClassKind::Fighter)), "{} hates fighters", spec.name);
                }
                [a, b] => {
                    martial += 1;
                    assert_eq!(classes, ["fighter_or_ranger"], "{}", spec.name);
                    assert_eq!({ let mut c = [*a, *b]; c.sort_by_key(|k| *k as u8); c }, [ClassKind::Fighter, ClassKind::Ranger], "{}", spec.name);
                    assert!(spec.outfits.contains(&Outfit::Fighter) && spec.outfits.contains(&Outfit::Ranger), "{}: {:?}", spec.name, spec.outfits);
                    assert!(!spec.hates.contains(&Preference::Class(ClassKind::Fighter)) && !spec.hates.contains(&Preference::Class(ClassKind::Ranger)), "{}", spec.name);
                }
                [] => {
                    unsaid += 1;
                    assert!(classes.is_empty(), "{} says {classes:?} but sets no class", spec.name);
                }
                other => panic!("{}: {other:?}", spec.name),
            }
        }
    }
    assert!(clerics > 20 && martial > 20 && knights > 3 && unsaid > 400, "{clerics} cleric, {martial} martial, {knights} knights, {unsaid} unsaid");
}

#[test]
fn a_name_that_says_a_sex_makes_every_member_so() {
    let (mut women, mut men, mut unsaid) = (0, 0, 0);
    for seed in 0..300 {
        for spec in World::new(seed).factions() {
            let sexes = said(&spec.name, "sex");
            assert_eq!(spec.sex, match &sexes[..] {
                [s] if s == "female" => Some(Sex::Female),
                [s] if s == "male" => Some(Sex::Male),
                [] => None,
                other => panic!("{} says {other:?}", spec.name),
            }, "{}", spec.name);
            let members: Vec<Sex> = (0..200).map(|i| spec.member_sex(i * 7919 + seed)).collect();
            let share = members.iter().filter(|s| **s == Sex::Female).count();
            match spec.sex {
                Some(Sex::Female) => {
                    women += 1;
                    assert_eq!(share, 200, "{}", spec.name);
                }
                Some(Sex::Male) => {
                    men += 1;
                    assert_eq!(share, 0, "{}", spec.name);
                }
                None => {
                    unsaid += 1;
                    assert!((70..=130).contains(&share), "{}: {share} of 200 are women", spec.name);
                }
            }
        }
    }
    assert!(women > 20 && men > 20 && unsaid > 500, "{women} women, {men} men, {unsaid} unsaid");
}

#[test]
fn a_faction_can_be_women_and_clerics_or_men_and_clerics() {
    let (mut women, mut men) = (None, None);
    for seed in 0..3000 {
        for spec in World::new(seed).factions() {
            if spec.classes == [ClassKind::Cleric] {
                match spec.sex {
                    Some(Sex::Female) => women = women.or(Some((seed, spec.name.clone()))),
                    Some(Sex::Male) => men = men.or(Some((seed, spec.name.clone()))),
                    None => {}
                }
            }
        }
    }
    assert!(women.is_some() && men.is_some(), "women and clerics {women:?}, men and clerics {men:?}");
}

#[test]
fn no_name_says_two_classes() {
    for seed in 0..300 {
        for spec in World::new(seed).factions() {
            assert!(said(&spec.name, "class").len() < 2, "{} says two classes", spec.name);
        }
    }
    // The grammar notes a key set twice to different values.
    let g = procgen_grammar::Grammar::from_pairs(&[("origin", &["[class:cleric]a [class:fighter]b", "[class:cleric]a [class:cleric]b"])]);
    let conflicts = |seed| procgen_grammar::expand_tagged(&g, "#origin#", seed).unwrap().conflicts;
    assert!((0..20).any(|s| conflicts(s) == ["class"]) && (0..20).any(|s| conflicts(s).is_empty()));
}

#[test]
fn the_same_seed_gives_the_same_name_and_makeup() {
    for seed in 0..40 {
        let (a, b) = (World::new(seed), World::new(seed));
        for (x, y) in a.factions().iter().zip(b.factions()) {
            assert_eq!((&x.name, &x.classes, x.outfits), (&y.name, &y.classes, y.outfits));
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
