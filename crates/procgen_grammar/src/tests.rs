use super::*;

const FOLK: [&str; 6] = ["human", "dwarf", "gnome", "goblin", "lizardfolk", "pigfolk"];
const INTENTIONS: [&str; 4] = ["way_out", "trade", "worship", "plunder"];

#[test]
fn same_seed_same_name() {
    for folk in FOLK {
        for intent in INTENTIONS {
            for seed in 0..20 {
                assert_eq!(faction_name(folk, intent, seed), faction_name(folk, intent, seed));
            }
        }
    }
}

#[test]
fn seeds_give_variety() {
    for folk in FOLK {
        for intent in INTENTIONS {
            let mut names: Vec<String> = (0..60).map(|s| faction_name(folk, intent, s)).collect();
            names.sort();
            names.dedup();
            assert!(names.len() >= 20, "{folk}/{intent} gave only {} names", names.len());
        }
    }
}

#[test]
fn every_symbol_in_the_name_data_exists() {
    let data = faction_names();
    assert_eq!(data.folk.len(), FOLK.len());
    assert_eq!(data.intention.len(), INTENTIONS.len());
    for folk in FOLK {
        for intent in INTENTIONS {
            let g = data.grammar(folk, intent).unwrap();
            assert_eq!(g.problems("#origin#"), vec![], "{folk}/{intent}");
        }
    }
}

#[test]
fn names_are_clean() {
    for folk in FOLK {
        for intent in INTENTIONS {
            for seed in 0..40 {
                let n = faction_name(folk, intent, seed);
                assert!(n.starts_with("the "), "{n}");
                assert!(n.chars().nth(4).is_some_and(char::is_uppercase), "{n}");
                assert!(!n.contains('#') && !n.contains("  "), "{n}");
            }
        }
    }
}

#[test]
fn unknown_key_is_nameless() {
    assert_eq!(faction_name("elf", "trade", 1), "the Nameless");
}

#[test]
fn references_expand_recursively() {
    let g = Grammar::from_pairs(&[
        ("origin", &["the #a# #b#"]),
        ("a", &["#c#"]),
        ("b", &["clan"]),
        ("c", &["Mud"]),
    ]);
    assert_eq!(expand(&g, "#origin#", 1).unwrap(), "the Mud clan");
}

#[test]
fn modifiers_work() {
    let g = Grammar::from_pairs(&[
        ("w", &["gear"]),
        ("o", &["owl"]),
        ("y", &["city"]),
        ("x", &["fox"]),
        ("k", &["key"]),
    ]);
    let e = |s: &str| expand(&g, s, 1).unwrap();
    assert_eq!(e("#w.capitalize#"), "Gear");
    assert_eq!(e("#w.s#"), "gears");
    assert_eq!(e("#w.a#"), "a gear");
    assert_eq!(e("#o.a#"), "an owl");
    assert_eq!(e("#y.s#"), "cities");
    assert_eq!(e("#x.s#"), "foxes");
    assert_eq!(e("#k.s#"), "keys");
    assert_eq!(e("#w.s.capitalize#"), "Gears");
    assert_eq!(e("#w.upper#"), "GEAR");
    assert_eq!(e("#w.upper.lower#"), "gear");
    assert_eq!(e("a ## b"), "a # b");
}

#[test]
fn unknown_modifier_and_missing_symbol_are_errors() {
    let g = Grammar::from_pairs(&[("w", &["gear"])]);
    assert_eq!(
        expand(&g, "#w.shout#", 1),
        Err(GrammarError::UnknownModifier("shout".into()))
    );
    assert_eq!(expand(&g, "#nope#", 1), Err(GrammarError::MissingSymbol("nope".into())));
    assert_eq!(g.problems("#nope.zap#").len(), 2);
}

#[test]
fn runaway_recursion_is_caught() {
    let g = Grammar::from_pairs(&[("origin", &["a #origin#"])]);
    assert_eq!(expand(&g, "#origin#", 1), Err(GrammarError::TooDeep));
    // A rule that doubles itself is stopped by the expansion budget.
    // Fifteen levels deep stays under MAX_DEPTH but doubles to 2^15 expansions.
    let rules: Vec<(String, String)> =
        (0..15).map(|i| (format!("s{i}"), format!("#s{}##s{}#", i + 1, i + 1))).collect();
    let mut g = Grammar::default();
    for (k, v) in rules {
        g.symbols.insert(k, vec![v]);
    }
    g.symbols.insert("s15".into(), vec!["x".into()]);
    assert_eq!(expand(&g, "#s0#", 1), Err(GrammarError::TooLarge));
}

#[test]
fn choices_come_from_the_seed() {
    let g = Grammar::from_pairs(&[("origin", &["a", "b", "c", "d", "e", "f", "g", "h"])]);
    let all: std::collections::BTreeSet<String> =
        (0..100).map(|s| expand(&g, "#origin#", s).unwrap()).collect();
    assert_eq!(all.len(), 8);
}

#[test]
fn shipped_names_resemble_the_hand_made_seven() {
    // The seven names in data/factions.json are the kind of thing the grammar yields:
    // each one's shape (not its exact words) should be reachable. Spot-check by pairs.
    let want = [
        ("human", "way_out", "the Lantern-keepers"),
        ("goblin", "plunder", "the Mudfoot clan"),
        ("human", "way_out", "the Brothers of the Last Door"),
        ("lizardfolk", "worship", "the Scale Court"),
        ("pigfolk", "trade", "the Hogsmeet traders"),
        ("gnome", "trade", "the Gnome Guild of Gears"),
        ("human", "worship", "the Ash Choir"),
    ];
    for (folk, intent, name) in want {
        let g = faction_names().grammar(folk, intent).unwrap();
        let found = (0..20_000).any(|s| expand(&g, "#origin#", s).unwrap() == name);
        assert!(found, "{folk}/{intent} never yields {name}");
    }
}

#[test]
fn no_name_repeats_a_word() {
    for folk in FOLK {
        for intent in INTENTIONS {
            let g = faction_names().grammar(folk, intent).unwrap();
            for seed in 0..200 {
                let n = expand(&g, "#origin#", seed).unwrap();
                let words: Vec<String> = n
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| !w.is_empty() && *w != "the" && *w != "of")
                    .map(str::to_lowercase)
                    .collect();
                for (i, w) in words.iter().enumerate() {
                    assert!(!words[i + 1..].contains(w), "{folk}/{intent} seed {seed}: {n}");
                }
            }
        }
    }
}

#[test]
fn mix_separates_folk() {
    // One shared grammar, so only the seed differs between the two calls.
    let g = Grammar::from_pairs(&[("origin", &["a", "b", "c", "d", "e", "f", "g", "h"])]);
    let differs = (0..50).filter(|&s| {
        expand(&g, "#origin#", mix(s, "human", "trade")).unwrap()
            != expand(&g, "#origin#", mix(s, "dwarf", "trade")).unwrap()
    });
    assert!(differs.count() > 20);
}

// Goldens: these move whenever data/faction_names.json or the RNG changes. Update them then.
#[test]
fn golden_names() {
    assert_eq!(faction_name("goblin", "plunder", 3), "the Pillagers of the Red Pit");
    assert_eq!(faction_name("human", "way_out", 3), "the Veil Assembly");
    assert_eq!(faction_name("pigfolk", "trade", 3), "the Hog League of Grain");
}

#[test]
fn an_action_sets_a_key_and_writes_nothing() {
    let g = Grammar::from_pairs(&[("origin", &["[sex:female]Daughters of the #place#"]), ("place", &["Gate"])]);
    let e = expand_tagged(&g, "#origin#", 1).unwrap();
    assert_eq!(e.text, "Daughters of the Gate");
    assert_eq!(e.tag("sex"), Some("female"));
    assert_eq!(e.tag("class"), None);
    assert_eq!(expand(&g, "#origin#", 1).unwrap(), "Daughters of the Gate");
    assert!(g.problems("#origin#").is_empty());
}

#[test]
fn a_set_key_reads_back_and_its_value_expands() {
    let g = Grammar::from_pairs(&[("origin", &["[who:#name#]#who# meets #who.capitalize#"]), ("name", &["ash", "bell"])]);
    for seed in 0..10 {
        let e = expand_tagged(&g, "#origin#", seed).unwrap();
        let who = e.tag("who").unwrap().to_string();
        assert_eq!(e.text, format!("{who} meets {}", apply_modifier("capitalize", &who).unwrap()));
    }
    assert!(g.problems("#origin#").is_empty());
}

#[test]
fn an_action_deep_in_a_rule_reaches_the_result() {
    let g = Grammar::from_pairs(&[("origin", &["the #kin#"]), ("kin", &["[sex:male]Sons", "[sex:male]Brothers"])]);
    assert_eq!(expand_tagged(&g, "#origin#", 4).unwrap().tag("sex"), Some("male"));
}

#[test]
fn a_bracket_that_is_not_an_action_is_text() {
    let g = Grammar::from_pairs(&[("origin", &["a [b] c [d: e"])]);
    assert_eq!(expand(&g, "#origin#", 1).unwrap(), "a [b] c [d: e");
}

#[test]
fn rules_without_actions_set_nothing() {
    let g = Grammar::from_pairs(&[("origin", &["#a# #a# #a#"]), ("a", &["x", "y", "z", "w"])]);
    for seed in 0..20 {
        assert!(expand_tagged(&g, "#origin#", seed).unwrap().tags.is_empty());
    }
}
