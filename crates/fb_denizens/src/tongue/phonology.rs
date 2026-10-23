//! What each tongue sounds like. A native word is rolled from the kind and the word's key alone,
//! so it is the same in every world, run and save, and no two words of a tongue are spelt alike.

use std::collections::HashSet;
use std::sync::OnceLock;

use procgen_rng::Mulberry32;

use super::vocabulary::VOCABULARY;
use crate::DenizenKind;

/// The sounds a tongue builds its words from.
struct Phonology {
    onsets: &'static [&'static str],
    vowels: &'static [&'static str],
    codas: &'static [&'static str],
    /// Syllables in a word, at least and at most.
    syllables: (u32, u32),
}

fn phonology(kind: DenizenKind) -> Phonology {
    match kind {
        // Bovine and guttural: low vowels, grunted stops, heavy codas.
        DenizenKind::Minotaur => Phonology {
            onsets: &["m", "gr", "b", "kh", "g", "r", "d", "th", "n", "gh", "br"],
            vowels: &["o", "u", "a", "oo", "uh"],
            codas: &["r", "rg", "k", "m", "rn", "gh", "rk", "nk"],
            syllables: (1, 2),
        },
        // Booming and hard: open vowels, stops and nasals, few syllables.
        DenizenKind::Cyclops => Phonology {
            onsets: &["k", "t", "b", "d", "g", "dr", "kr", "m", "h", "bl"],
            vowels: &["o", "a", "au", "u", "oh"],
            codas: &["m", "n", "d", "nk", "mb", "k", "rd", "ng"],
            syllables: (1, 2),
        },
        // Lilting: liquids and glides, running vowels, mostly open syllables.
        DenizenKind::Faun => Phonology {
            onsets: &["l", "s", "f", "v", "n", "sh", "th", "y", "w", "m"],
            vowels: &["ae", "ia", "e", "i", "ei", "ou", "a", "ee", "io"],
            codas: &["", "", "", "l", "n", "r", "s", "th"],
            syllables: (2, 3),
        },
        // Harsh and sibilant: hissed and buzzed consonants, clipped vowels.
        DenizenKind::Demon => Phonology {
            onsets: &["z", "x", "s", "v", "th", "k", "sz", "ss", "zh", "kz"],
            vowels: &["i", "e", "a", "ix", "u"],
            codas: &["ss", "x", "th", "z", "sk", "k", "ith", "is"],
            syllables: (1, 3),
        },
    }
}

fn hash(text: &str) -> u32 {
    text.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

fn roll(kind: DenizenKind, key: &str, salt: u32) -> String {
    let p = phonology(kind);
    let mut rng = Mulberry32::new(hash(kind.key()) ^ hash(key).rotate_left(11) ^ salt.wrapping_mul(0x9e37_79b9));
    let count = rng.int(p.syllables.0, p.syllables.1);
    let mut out = String::new();
    for i in 0..count {
        out.push_str(rng.pick(p.onsets));
        out.push_str(rng.pick(p.vowels));
        // The last syllable always closes, so words end the way the tongue ends them.
        if i + 1 == count || rng.chance(0.3) {
            out.push_str(rng.pick(p.codas));
        }
    }
    out
}

fn table(kind: DenizenKind) -> &'static [String] {
    static TABLES: [OnceLock<Vec<String>>; 4] = [OnceLock::new(), OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let slot = DenizenKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
    TABLES[slot].get_or_init(|| {
        let mut taken = HashSet::new();
        VOCABULARY
            .iter()
            .map(|word| {
                let mut salt = 0;
                loop {
                    let spelt = roll(kind, word.key, salt);
                    if taken.insert(spelt.clone()) {
                        break spelt;
                    }
                    salt += 1;
                }
            })
            .collect()
    })
}

/// How the word with this `key` is spelt in `kind`'s tongue; fixed per kind and key. A key that
/// is not a word is rolled the same way, without the guarantee that it differs from the rest.
pub fn native(kind: DenizenKind, key: &str) -> String {
    match VOCABULARY.iter().position(|w| w.key == key) {
        Some(i) => table(kind)[i].clone(),
        None => roll(kind, key, 0),
    }
}
