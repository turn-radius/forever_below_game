use std::collections::HashSet;

use super::*;
use super::phonology;
use crate::DenizenKind;

const LINE: &str = "Well met, stranger. What will you trade?";

fn minotaur_scroll() -> Teaching {
    Teaching::of_topics(DenizenKind::Minotaur, &[Topic::Greeting, Topic::Trade])
}

#[test]
fn every_tongue_has_a_distinct_name() {
    let names: HashSet<_> = DenizenKind::ALL.iter().map(|k| tongue_name(*k)).collect();
    assert_eq!(names.len(), 4);
}

#[test]
fn every_topic_has_words_for_every_tongue() {
    for kind in DenizenKind::ALL {
        for topic in Topic::ALL {
            let words: Vec<_> = topic.words().collect();
            assert!(words.len() >= CORE_WORDS, "{topic:?}");
            assert_eq!(needs(kind, topic).len(), CORE_WORDS);
            assert!(words.iter().all(|w| !w.native(kind).is_empty()));
        }
    }
}

#[test]
fn word_keys_are_unique_and_native_forms_are_unique_per_tongue() {
    let keys: HashSet<_> = VOCABULARY.iter().map(|w| w.key).collect();
    assert_eq!(keys.len(), VOCABULARY.len());
    for kind in DenizenKind::ALL {
        let spelt: HashSet<_> = VOCABULARY.iter().map(|w| w.native(kind)).collect();
        assert_eq!(spelt.len(), VOCABULARY.len(), "{kind:?}");
    }
}

#[test]
fn native_words_are_fixed_per_kind_and_differ_between_tongues() {
    for kind in DenizenKind::ALL {
        for w in VOCABULARY {
            assert_eq!(w.native(kind), native(kind, w.key));
        }
    }
    let hail: HashSet<_> = DenizenKind::ALL.iter().map(|k| native(*k, "hail")).collect();
    assert_eq!(hail.len(), 4);
}

#[test]
fn native_words_sound_like_their_tongue() {
    let share = |kind: DenizenKind, letters: &[char]| {
        let all: String = VOCABULARY.iter().map(|w| w.native(kind)).collect();
        all.chars().filter(|c| letters.contains(c)).count() as f32 / all.chars().count() as f32
    };
    // Demon words hiss; fauns' are mostly vowels; neither matches the other.
    assert!(share(DenizenKind::Demon, &['s', 'z', 'x']) > share(DenizenKind::Faun, &['s', 'z', 'x']));
    assert!(share(DenizenKind::Faun, &['a', 'e', 'i', 'o', 'u']) > share(DenizenKind::Cyclops, &['a', 'e', 'i', 'o', 'u']));
}

#[test]
fn a_minotaur_line_with_no_words_learned_is_unreadable() {
    let lex = Lexicon::new();
    for topic in Topic::ALL {
        assert!(!lex.readable(DenizenKind::Minotaur, &needs(DenizenKind::Minotaur, topic)), "{topic:?}");
    }
    assert_eq!(lex.known_share(DenizenKind::Minotaur), 0.0);
    assert!(!lex.can_barter(DenizenKind::Minotaur));
    assert!(!lex.can_take_quests(DenizenKind::Minotaur));
}

#[test]
fn a_scroll_of_greetings_and_trade_makes_those_lines_readable_and_no_others() {
    let mut lex = Lexicon::new();
    let kind = DenizenKind::Minotaur;
    let scroll = minotaur_scroll();
    assert_eq!(scroll.description(), "Minotaur phrases: greetings and trade");
    let fresh = lex.learn_teaching(&scroll);
    assert_eq!(fresh.len(), scroll.words.len());
    assert!(lex.topic_readable(kind, Topic::Greeting));
    assert!(lex.topic_readable(kind, Topic::Trade));
    for topic in [Topic::Farewell, Topic::Kin, Topic::Quest, Topic::Place, Topic::Threat] {
        assert!(!lex.topic_readable(kind, topic), "{topic:?}");
    }
    // Another tongue is untouched.
    assert!(!lex.topic_readable(DenizenKind::Cyclops, Topic::Greeting));
}

#[test]
fn enough_of_a_tongue_lets_the_delver_barter_and_take_quests() {
    let kind = DenizenKind::Minotaur;
    let mut lex = Lexicon::new();
    lex.learn_teaching(&minotaur_scroll());
    assert!(lex.can_barter(kind));
    assert!(!lex.can_take_quests(kind));
    lex.learn_teaching(&Teaching::of_topics(kind, &[Topic::Quest]));
    assert!(lex.can_take_quests(kind));

    // Without the greeting, nothing else is enough, however much of the tongue is known.
    let mut wide = Lexicon::new();
    wide.learn_teaching(&Teaching::of_topics(kind, &[Topic::Farewell, Topic::Kin, Topic::Trade, Topic::Quest, Topic::Place, Topic::Threat]));
    assert!(wide.known_share(kind) > 0.8);
    assert!(!wide.can_barter(kind));
    assert!(!wide.can_take_quests(kind));
}

#[test]
fn a_line_is_readable_once_all_its_words_are_learned() {
    let kind = DenizenKind::Faun;
    let line = needs_with(kind, Topic::Trade, &["blood", "buy"]).unwrap();
    assert_eq!(needs_with(kind, Topic::Trade, &["blood", "no_such_word"]), Err(UnknownWord("no_such_word".to_string())));
    assert_eq!(line.len(), CORE_WORDS + 1);
    let mut lex = Lexicon::new();
    lex.learn_teaching(&Teaching::of_topics(kind, &[Topic::Trade]));
    assert!(!lex.readable(kind, &line));
    lex.learn(kind, &[word("blood").unwrap()]);
    assert!(lex.readable(kind, &line));
    assert!(lex.readable(kind, &[]));
}

#[test]
fn learning_reports_only_new_words_and_words_stay_learned() {
    let kind = DenizenKind::Demon;
    let mut lex = Lexicon::new();
    let scroll = Teaching::of_topics(kind, &[Topic::Threat]);
    assert_eq!(lex.learn_teaching(&scroll).len(), 6);
    assert!(lex.learn_teaching(&scroll).is_empty());
    let more = Teaching::of_topics(kind, &[Topic::Threat, Topic::Place]);
    assert_eq!(lex.learn_teaching(&more).len(), 6);
    assert_eq!(lex.known_words(kind).len(), 12);
    assert!((lex.known_share(kind) - 12.0 / VOCABULARY.len() as f32).abs() < 1e-6);
}

#[test]
fn the_lexicon_round_trips_through_json() {
    let mut lex = Lexicon::new();
    lex.learn_teaching(&minotaur_scroll());
    lex.learn_teaching(&Teaching::of_topics(DenizenKind::Demon, &[Topic::Kin]));
    let json = serde_json::to_string(&lex).unwrap();
    let back: Lexicon = serde_json::from_str(&json).unwrap();
    assert_eq!(back, lex);
    assert!(back.can_barter(DenizenKind::Minotaur));
}

#[test]
fn teaching_is_rolled_from_the_seed() {
    for kind in DenizenKind::ALL {
        for seed in 0..50u32 {
            assert_eq!(Teaching::roll(seed, kind, TeachingSize::Scroll), Teaching::roll(seed, kind, TeachingSize::Scroll));
            let scroll = Teaching::roll(seed, kind, TeachingSize::Scroll);
            let book = Teaching::roll(seed, kind, TeachingSize::Book);
            assert!((1..=2).contains(&scroll.topics.len()), "{seed}");
            assert!((3..=5).contains(&book.topics.len()), "{seed}");
            assert!(scroll.description().starts_with(&format!("{} phrases: ", kind.label())));
            // Teaching what it says it does.
            let mut lex = Lexicon::new();
            lex.learn_teaching(&book);
            assert!(book.topics.iter().all(|t| lex.topic_readable(kind, *t)));
        }
    }
    for (size, lo, hi) in [(TeachingSize::Scroll, 1, 2), (TeachingSize::Book, 3, 5)] {
        let counts: HashSet<usize> = (0..200u32).map(|s| Teaching::roll(s, DenizenKind::Faun, size).topics.len()).collect();
        assert!(counts.contains(&lo) && counts.contains(&hi), "{size:?}: {counts:?}");
    }
    let varied: HashSet<_> = (0..50u32).map(|s| Teaching::roll(s, DenizenKind::Cyclops, TeachingSize::Scroll).topics).collect();
    assert!(varied.len() > 3);
}

#[test]
fn descriptions_list_the_topics() {
    let one = Teaching::of_topics(DenizenKind::Faun, &[Topic::Kin]);
    assert_eq!(one.description(), "Faun phrases: kinship");
    let three = Teaching::of_topics(DenizenKind::Demon, &[Topic::Threat, Topic::Greeting, Topic::Quest]);
    assert_eq!(three.description(), "Demon phrases: greetings, requests and warnings");
}

#[test]
fn glyphs_keep_spaces_and_punctuation_and_are_deterministic() {
    for kind in DenizenKind::ALL {
        let a = glyphs(kind, LINE, 7);
        assert_eq!(a, glyphs(kind, LINE, 7));
        assert_ne!(a, glyphs(kind, LINE, 8));
        assert_eq!(a.chars().count(), LINE.chars().count());
        let set = script(kind);
        for (g, t) in a.chars().zip(LINE.chars()) {
            if t.is_alphanumeric() {
                assert!(set.contains(g), "{kind:?} {g}");
            } else {
                assert_eq!(g, t);
            }
        }
        assert_ne!(a, LINE);
    }
}

#[test]
fn each_tongue_has_a_script_of_its_own() {
    for a in DenizenKind::ALL {
        let chars: Vec<char> = script(a).chars().collect();
        assert!(chars.len() >= 15, "{a:?}");
        assert_eq!(chars.iter().collect::<HashSet<_>>().len(), chars.len(), "{a:?} repeats a glyph");
        for b in DenizenKind::ALL {
            if a < b {
                assert!(script(b).chars().all(|c| !script(a).contains(c)), "{a:?} and {b:?} share a glyph");
            }
        }
    }
    for kind in DenizenKind::ALL {
        println!("{kind:?}: {} | {}", script(kind), glyphs(kind, LINE, 1));
    }
}

/// The glyph codepoints a font draws, read from its `cmap` (formats 4 and 12).
fn font_codepoints(path: &std::path::Path) -> HashSet<u32> {
    let d = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let u16at = |o: usize| u16::from_be_bytes([d[o], d[o + 1]]) as usize;
    let u32at = |o: usize| u32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as usize;
    let tables = u16at(4);
    let cmap = (0..tables).map(|i| 12 + 16 * i).find(|&r| &d[r..r + 4] == b"cmap").map(|r| u32at(r + 8)).expect("a cmap table");
    let mut out = HashSet::new();
    for i in 0..u16at(cmap + 2) {
        let sub = cmap + u32at(cmap + 4 + 8 * i + 4);
        match u16at(sub) {
            4 => {
                let segs = u16at(sub + 6) / 2;
                for s in 0..segs {
                    let end = u16at(sub + 14 + 2 * s);
                    let start = u16at(sub + 16 + 2 * segs + 2 * s);
                    out.extend((start..=end).filter(|c| *c != 0xFFFF).map(|c| c as u32));
                }
            }
            12 => {
                for g in 0..u32at(sub + 12) {
                    let at = sub + 16 + 12 * g;
                    out.extend(u32at(at) as u32..=u32at(at + 4) as u32);
                }
            }
            _ => {}
        }
    }
    out
}

#[test]
fn every_glyph_is_drawn_by_the_talk_panels_fonts() {
    let fonts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../game/assets/fonts");
    for face in ["AlegreyaSans-Regular.ttf", "AlegreyaSans-Italic.ttf", "AlegreyaSans-Medium.ttf"] {
        let drawn = font_codepoints(&fonts.join(face));
        assert!(drawn.contains(&('a' as u32)), "{face}: the cmap reads");
        for kind in DenizenKind::ALL {
            for c in script(kind).chars() {
                assert!(drawn.contains(&(c as u32)), "{face} lacks {c} (U+{:04X}) of {kind:?}", c as u32);
            }
        }
    }
}

#[test]
fn a_word_is_spelt_the_same_alone_as_in_the_table() {
    for kind in DenizenKind::ALL {
        for w in VOCABULARY {
            assert_eq!(phonology::spell_alone(kind, w.key), w.native(kind), "{kind:?} {}", w.key);
        }
    }
}

#[test]
fn a_teaching_round_trips_through_json() {
    for kind in DenizenKind::ALL {
        let teaching = Teaching::roll(11, kind, TeachingSize::Book);
        let json = serde_json::to_string(&teaching).unwrap();
        assert!(json.contains("keys"));
        assert_eq!(serde_json::from_str::<Teaching>(&json).unwrap(), teaching);
    }
    for w in VOCABULARY {
        assert_eq!(word(w.key), Some(*w));
    }
}
