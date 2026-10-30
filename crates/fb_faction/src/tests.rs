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
                // A name that says its members are of every folk leaves one in six their own.
                let share = if spec.looks.folk == Some(FolkSaid::Mixed) { 10..=60 } else { 140..=200 };
                assert!(share.contains(&own), "{}: {own} of 200 are its folk", spec.name);
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
            // Unless its name gives both its colours ("the Redeemed" are argent and or).
            if spec.looks.colours.len() < 2 {
                assert_ne!(f.tincture.is_metal(), f.tincture2.is_metal(), "{}: {:?} on {:?}", spec.name, f.tincture, f.tincture2);
            }
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
    let (mut clerics, mut knights, mut unsaid) = (0, 0, 0);
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
                [] => {
                    unsaid += 1;
                    assert!(classes.is_empty(), "{} says {classes:?} but sets no class", spec.name);
                }
                other => panic!("{}: {other:?}", spec.name),
            }
        }
    }
    assert!(clerics > 20 && knights > 3 && unsaid > 400, "{clerics} cleric, {knights} knights, {unsaid} unsaid");
}

#[test]
fn a_name_that_says_a_sex_makes_every_member_so() {
    let (mut women, mut men, mut unsaid) = (0, 0, 0);
    for seed in 0..300 {
        for spec in World::new(seed).folk_factions() {
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
        for spec in World::new(seed).folk_factions() {
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
        for p in Profession::MASTERABLE {
            assert!(masters.deeper(p, 1).is_some(), "world {seed}: no {p:?} master");
        }
    }
}

#[test]
fn trades_read_as_the_folk_card_writes_them() {
    let t = |profession, tier, style| Trade { profession, tier, style }.label();
    assert_eq!(t(Profession::Weaponsmith, Tier::Apprentice, Some(Style::Blades)), "Apprentice weaponsmith (blades)");
    assert_eq!(t(Profession::Alchemist, Tier::Master, None), "Master alchemist");
    assert_eq!(t(Profession::Healer, Tier::Expert, None), "Expert healer");
    assert_eq!(Trade::guard().label(), "Guard");
    assert!(Tier::Novice < Tier::Apprentice && Tier::Apprentice < Tier::Expert && Tier::Expert < Tier::Master);
}

fn tincture(name: &str) -> armory::heraldry::Tincture {
    use armory::heraldry::Tincture::*;
    match name {
        "or" => Or,
        "argent" => Argent,
        "gules" => Gules,
        "azure" => Azure,
        "vert" => Vert,
        "purpure" => Purpure,
        "sable" => Sable,
        other => panic!("{other}"),
    }
}

#[test]
fn a_name_that_says_a_colour_gives_the_field_that_colour() {
    let (mut one, mut two) = (0, 0);
    for seed in 0..300 {
        for spec in World::new(seed).factions() {
            let said_main = said(&spec.name, "colour");
            let said_second = said(&spec.name, "colour2");
            assert_eq!(spec.looks.colours.first().map(|t| *t), said_main.first().map(|v| tincture(v)), "{}", spec.name);
            assert_eq!(spec.looks.colours.get(1).map(|t| *t), said_second.first().map(|v| tincture(v)), "{}", spec.name);
            let Some(&main) = spec.looks.colours.first() else { continue };
            assert_eq!(spec.field.tincture, main, "{}", spec.name);
            if let Some(&second) = spec.looks.colours.get(1) {
                two += 1;
                assert_eq!(spec.field.tincture2, second, "{}", spec.name);
            } else {
                one += 1;
                assert_ne!(spec.field.tincture2.is_metal(), main.is_metal(), "{}: the other colour is of the other kind", spec.name);
            }
            assert!(spec.field.charge_tincture != spec.field.tincture && spec.field.charge_tincture != spec.field.tincture2, "{}", spec.name);
        }
    }
    assert!(one > 10 && two > 0, "{one} named one colour, {two} two");
}

#[test]
fn the_sisters_of_azul_wear_blue_robes() {
    let found = (0..4000).find_map(|seed| World::new(seed).factions().iter().find(|f| f.name == "the Sisters of Azul").cloned());
    let spec = found.expect("a world has the Sisters of Azul");
    assert_eq!(spec.field.tincture, armory::heraldry::Tincture::Azure);
    assert_eq!((spec.looks.garment, spec.looks.trim, spec.sex), (Some(Garment::Robe), None, Some(Sex::Female)));
}

#[test]
fn the_redeemed_are_knights_of_every_folk() {
    let found = (0..4000).find_map(|seed| World::new(seed).factions().iter().find(|f| f.name == "the Redeemed").cloned());
    let spec = found.expect("a world has the Redeemed");
    use armory::heraldry::Tincture::*;
    assert_eq!((spec.field.tincture, spec.field.tincture2), (Argent, Or));
    assert_eq!((spec.armour, spec.looks.garment, spec.looks.trim), (ArmourLean::Plate, Some(Garment::RobeOverArmour), Some(Trim::GoldHem)));
    assert_eq!((spec.classes.clone(), spec.outfits), (vec![ClassKind::Fighter], [Outfit::Fighter; 2]));
    assert!(!spec.insular);
    let mut rng = Mulberry32::new(9);
    let mut counts = std::collections::HashMap::new();
    for _ in 0..1200 {
        *counts.entry(spec.member_folk(&mut rng)).or_insert(0) += 1;
    }
    assert_eq!(counts.len(), Folk::ALL.len());
    assert!(counts.values().all(|n| (140..=260).contains(n)), "{counts:?}");
}

#[test]
fn the_crimson_brothers_are_men_in_red() {
    let found = (0..4000).find_map(|seed| World::new(seed).factions().iter().find(|f| f.name == "the Crimson Brothers").cloned());
    let spec = found.expect("a world has the Crimson Brothers");
    assert_eq!(spec.field.tincture, armory::heraldry::Tincture::Gules);
    assert_eq!((spec.sex, spec.looks.garment), (Some(Sex::Male), Some(Garment::Robe)));
}

#[test]
fn a_name_that_says_a_folk_keeps_it() {
    let (mut own, mut mixed) = (0, 0);
    for seed in 0..300 {
        for spec in World::new(seed).folk_factions() {
            match said(&spec.name, "folk_mix").first().map(String::as_str) {
                // The lizardfolk's "Sun" and "scale" can join into "Sunscale" with no tag on it.
                Some("own") if spec.looks.folk.is_none() && spec.name.contains("Sunscale") => {}
                Some("own") => {
                    own += 1;
                    assert!(spec.insular && spec.looks.folk == Some(FolkSaid::Own), "{}", spec.name);
                }
                Some("mixed") => {
                    mixed += 1;
                    assert!(!spec.insular && spec.looks.folk == Some(FolkSaid::Mixed), "{}", spec.name);
                }
                _ => assert_eq!(spec.looks.folk, None, "{}", spec.name),
            }
        }
    }
    assert!(own > 3 && mixed > 3, "{own} own, {mixed} mixed");
}

#[test]
fn a_name_that_says_nothing_of_looks_rolls_as_before() {
    // Rolled with no name read at all, the faction is the same: the look tags change only the
    // factions whose names say them.
    let mut plain = 0;
    for seed in 0..120 {
        let world = World::new(seed);
        let mut taken = HashSet::new();
        for (i, spec) in world.folk_factions().iter().enumerate() {
            if spec.looks.is_plain() && spec.classes.is_empty() {
                plain += 1;
                // (The tastes a denizen faction takes the place of are the one thing that moves.)
                let again = roll_with(seed, i, world.folk_count(), &taken, None);
                assert_eq!(FactionSpec { likes: vec![], hates: vec![], ..spec.clone() }, FactionSpec { likes: vec![], hates: vec![], ..again }, "{}", spec.name);
            }
            taken.insert(spec.name.clone());
        }
    }
    assert!(plain > 800, "{plain}");
}

#[test]
fn looks_are_the_same_every_run() {
    for seed in 0..40 {
        for (x, y) in World::new(seed).factions().iter().zip(World::new(seed).factions()) {
            assert_eq!((&x.looks, x.field, x.armour, x.insular), (&y.looks, y.field, y.armour, y.insular));
        }
    }
}

#[test]
fn reavers_dress_light() {
    let mut found = 0;
    for seed in 0..300 {
        for spec in World::new(seed).factions() {
            if said(&spec.name, "armour").first().map(String::as_str) == Some("light") {
                found += 1;
                assert_eq!(spec.armour, ArmourLean::Leather, "{}", spec.name);
            }
        }
    }
    assert!(found > 3, "{found}");
}

// Denizen factions.

/// The folk factions (name and folk) that worlds rolled before denizen factions existed, as the
/// names' words set them (#663, #682).
const BEFORE_DENIZENS: &[(u32, &[(&str, Folk)])] = &[
    (0, &[
        ("the Silver Raiders", Folk::Dwarf),
        ("the Stonebeard Brotherhood of Ingots", Folk::Dwarf),
        ("the Purple Tinkers", Folk::Gnome),
        ("the Scarlet Elders", Folk::Lizardfolk),
        ("the Ash-guardians", Folk::Human),
        ("the Lantern-keepers", Folk::Dwarf),
        ("the Brotherhood of the Iron Vault", Folk::Dwarf),
        ("the Bell cutpurses", Folk::Human),
        ("the Guardians of the Grey Road", Folk::Human),
        ("the Whelps of the Cackling Shrine", Folk::Goblin),
        ("the Marsh Brood of Herbs", Folk::Lizardfolk),
        ("the Redeemed", Folk::Lizardfolk),
        ("the Tinkling Workshop", Folk::Gnome),
        ("the Raiders of the Faithful Gate", Folk::Human),
        ("the Tinkling Gear tinkers", Folk::Gnome),
        ("the Clansmen of the Iron Stair", Folk::Dwarf),
        ("the Swillsnout clan", Folk::Pigfolk),
    ]),
    (1, &[
        ("the Cognook tinkers", Folk::Gnome),
        ("the Cold Mire brood", Folk::Lizardfolk),
        ("the Knights of the Sunken Road", Folk::Human),
        ("the Elders of the Last Door", Folk::Dwarf),
        ("the Guild of the Black Forge", Folk::Dwarf),
        ("the Tinkers of the Curious Chapel", Folk::Gnome),
        ("the Oath Shrine", Folk::Dwarf),
        ("the Black Magpies", Folk::Gnome),
        ("the Wick Vigil", Folk::Human),
        ("the Tinkers of Rojo", Folk::Gnome),
        ("the Knights of the Hollow Door", Folk::Dwarf),
        ("the Torch-sneaks", Folk::Goblin),
        ("the Company of the Weeping Bridge", Folk::Human),
    ]),
    (9, &[
        ("the Herders of the Muddy Door", Folk::Pigfolk),
        ("the Bell rovers", Folk::Human),
        ("the Weary Company of Cheese", Folk::Pigfolk),
        ("the Pit Court", Folk::Goblin),
        ("the Torch Moot", Folk::Goblin),
        ("the Black Clan", Folk::Dwarf),
        ("the League of the Venomous Fen", Folk::Lizardfolk),
        ("the Miretail tribe", Folk::Lizardfolk),
        ("the Burning Exchange of Coal", Folk::Dwarf),
        ("the Crooked Tinkers", Folk::Gnome),
        ("the Wallowtrot traders", Folk::Pigfolk),
        ("the Spawn of the Abbey", Folk::Goblin),
        ("the Company of the Faithful Door", Folk::Human),
        ("the Emerald Concern of Eggs", Folk::Lizardfolk),
        ("the Acorn Chorus", Folk::Pigfolk),
        ("the Gold Consortium", Folk::Gnome),
        ("the Ravenwick cutpurses", Folk::Human),
    ]),
    (1207, &[
        ("the Oathkeepers of the Iron Stair", Folk::Dwarf),
        ("the Ember-wardens", Folk::Human),
        ("the Little Troupe", Folk::Gnome),
        ("the Lens scavengers", Folk::Gnome),
        ("the Bristle foragers", Folk::Pigfolk),
        ("the Purple Brothers", Folk::Gnome),
        ("the Cognook watchers", Folk::Gnome),
        ("the Elders of the Emerald Cloister", Folk::Lizardfolk),
        ("the Key Chorus", Folk::Human),
        ("the Brothers of the Silent Chapel", Folk::Human),
        ("the Bell Abbey", Folk::Pigfolk),
        ("the Hollow Covenant", Folk::Dwarf),
        ("the Scarlet Gang", Folk::Goblin),
        ("the Troughstye sounder", Folk::Pigfolk),
        ("the Black League", Folk::Dwarf),
        ("the Burning Club", Folk::Gnome),
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

#[test]
fn a_denizen_faction_takes_its_looks_from_its_name_and_its_members_stay_men() {
    let (mut coloured, mut classed) = (0, 0);
    for seed in 0..200 {
        for spec in World::new(seed).denizen_factions() {
            assert_eq!(spec.looks.folk, None, "{}: a denizen is no one folk", spec.name);
            assert!(!spec.insular && spec.sex.is_none(), "{}", spec.name);
            // The members are denizens, rolled as men, whatever the name says of its Daughters.
            assert!((0..50).all(|i| spec.member_sex(i * 31 + seed) == Sex::Male), "{}", spec.name);
            if let Some(&first) = spec.looks.colours.first() {
                coloured += 1;
                assert_eq!(spec.field.tincture, first, "{}", spec.name);
            }
            if let [only] = spec.classes[..] {
                classed += 1;
                assert_eq!(spec.outfits, [Outfit::of(only); 2], "{}", spec.name);
                assert!(!spec.hates.contains(&Preference::Class(only)), "{}", spec.name);
            }
            if let Some(armour) = spec.looks.armour {
                assert_eq!(spec.armour, armour, "{}", spec.name);
            }
        }
    }
    assert!(coloured > 5 && classed > 20, "{coloured} named a colour, {classed} a class");
}
