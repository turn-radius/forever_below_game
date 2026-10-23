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
        let found = (0..300_000).any(|s| expand(&g, "#origin#", s).unwrap() == name);
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
    assert_eq!(faction_name("goblin", "plunder", 3), "the Rust pillagers");
    assert_eq!(faction_name("human", "way_out", 3), "the Children of Violeta");
    assert_eq!(faction_name("pigfolk", "trade", 3), "the Hog League of Grain");
}

/// Every name of the shipped data, with what it set.
fn all_names() -> Vec<(String, Expansion)> {
    let mut out = Vec::new();
    for folk in FOLK {
        for intent in INTENTIONS {
            for seed in 0..120 {
                let e = faction_named(folk, intent, seed);
                out.push((format!("{folk}/{intent}"), e));
            }
        }
    }
    out
}

/// Every `[key:value]Word` rule of the shipped name data, as (lowercase word, key, value), so
/// the tests follow the data rather than a list of their own.
fn tagged_words() -> Vec<(String, String, String)> {
    fn walk(v: &serde_json::Value, out: &mut Vec<(String, String, String)>) {
        match v {
            serde_json::Value::String(s) => {
                // A word may carry several tags: `[garment:robe][sex:female]Sisters`.
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

#[test]
fn names_say_what_their_members_are() {
    let names = all_names();
    let tagged = tagged_words();
    for key in ["sex", "class", "colour", "colour2", "garment", "armour", "trim", "folk_mix"] {
        assert!(tagged.iter().any(|(_, k, _)| k == key), "the data tags no {key}");
    }
    for (_, e) in &names {
        let words: Vec<String> = e.text.split(|c: char| !c.is_alphanumeric()).map(str::to_lowercase).collect();
        let said: Vec<&(String, String, String)> = tagged.iter().filter(|(w, _, _)| words.contains(w)).collect();
        for (word, key, value) in &said {
            if !e.conflicts.contains(key) {
                assert_eq!(e.tag(key), Some(value.as_str()), "{} says {word}", e.text);
            }
        }
        // A name with none of the tagged words sets nothing, and a name sets only what words say.
        for key in e.tags.keys() {
            assert!(said.iter().any(|(_, k, _)| k == key), "{}: {key} is set by no word", e.text);
        }
    }
    let seen = |key: &str, value: &str| names.iter().any(|(_, e)| e.tag(key) == Some(value));
    assert!(seen("sex", "female") && seen("sex", "male") && seen("class", "cleric") && seen("class", "fighter_or_ranger"));
    assert!(names.iter().any(|(_, e)| e.tags.is_empty()), "every name says something");
    assert!(names.iter().any(|(_, e)| e.tag("sex") == Some("female") && e.tag("class") == Some("cleric")), "no Daughters of a Chapel");
    assert!(names.iter().any(|(_, e)| e.tag("sex") == Some("male") && e.tag("class") == Some("cleric")), "no Brothers of a Chapel");
}

#[test]
fn actions_are_rule_level_only() {
    // Tracery's action inside a tag is not supported: the tag names no symbol.
    let g = Grammar::from_pairs(&[("origin", &["#[hero:#name#]story#"]), ("name", &["Ash"]), ("story", &["tale"])]);
    assert!(matches!(expand_tagged(&g, "#origin#", 1), Err(GrammarError::MissingSymbol(_))));
    // A value ends at the first `]`, so actions do not nest.
    let g = Grammar::from_pairs(&[("origin", &["[a:[b:c]]x"])]);
    let e = expand_tagged(&g, "#origin#", 1).unwrap();
    assert_eq!((e.tag("a"), e.tag("b"), e.text.as_str()), (Some("[b:c"), None, "]x"));
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

/// The first name of `folk`/`intent` that satisfies `pick`, from the first 300,000 seeds.
fn find_name(folk: &str, intent: &str, pick: impl Fn(&Expansion) -> bool) -> Option<Expansion> {
    (0..300_000).map(|seed| faction_named(folk, intent, seed)).find(|e| e.conflicts.is_empty() && pick(e))
}

#[test]
fn the_sisters_of_azul_are_women_in_blue_robes() {
    let e = find_name("human", "worship", |e| e.text == "the Sisters of Azul").expect("the Sisters of Azul can be rolled");
    assert_eq!((e.tag("colour"), e.tag("garment"), e.tag("sex")), (Some("azure"), Some("robe"), Some("female")));
    assert_eq!(e.tag("armour"), None);
}

#[test]
fn the_redeemed_are_knights_of_every_folk_in_white_and_gold() {
    let e = find_name("human", "worship", |e| e.text == "the Redeemed").expect("the Redeemed can be rolled");
    assert_eq!((e.tag("colour"), e.tag("colour2")), (Some("argent"), Some("or")));
    assert_eq!((e.tag("garment"), e.tag("armour"), e.tag("trim"), e.tag("folk_mix")), (Some("robe_over_armour"), Some("plate"), Some("gold_hem"), Some("mixed")));
}

#[test]
fn the_crimson_brothers_are_men_in_red() {
    let e = find_name("human", "worship", |e| e.text == "the Crimson Brothers").expect("the Crimson Brothers can be rolled");
    assert_eq!((e.tag("colour"), e.tag("sex"), e.tag("garment")), (Some("gules"), Some("male"), Some("robe")));
}

#[test]
fn the_grey_company_is_pale_and_mixed() {
    let e = find_name("human", "trade", |e| e.text.starts_with("the Grey Company")).expect("a Grey Company can be rolled");
    assert_eq!((e.tag("colour"), e.tag("folk_mix")), (Some("argent"), Some("mixed")));
}

#[test]
fn a_name_that_says_a_folk_and_a_mix_is_flagged() {
    // "mixed" and "own" in one name conflict, and a caller does not keep a name that conflicts.
    let clash = (0..300_000).map(|seed| faction_named("dwarf", "trade", seed)).any(|e| e.conflicts.iter().any(|k| k == "folk_mix"));
    assert!(clash, "no dwarven company clashes with its folk");
    assert!(find_name("dwarf", "trade", |e| e.tag("folk_mix") == Some("own")).is_some());
}

#[test]
fn reavers_wear_light_armour() {
    let e = find_name("human", "plunder", |e| e.text.to_lowercase().contains("reavers")).expect("reavers");
    assert_eq!(e.tag("armour"), Some("light"));
}
