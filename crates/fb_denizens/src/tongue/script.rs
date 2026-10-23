//! The written form of speech the delver cannot read yet: each tongue has a script of its own,
//! and a line shown in it keeps the line's rhythm.
//!
//! Every glyph is one the UI's running-text face, Alegreya Sans (Regular, Italic and Medium in
//! `game/assets/fonts`, the talk panel's `body` faces), draws. Almendra and Noto Sans Runic are
//! not enough: Almendra has next to no Greek, and the runic face covers only runes. Alegreya
//! has no runic or invented-script blocks, so the scripts are borrowed letters: Greek for the
//! minotaurs, Cyrillic for the cyclopes, accented Latin for the fauns and stroked, hooked Latin
//! for the demons. A test reads each font's character map and checks every glyph.

use procgen_rng::Mulberry32;

use crate::DenizenKind;

/// The glyphs of a kind's script; no two kinds share one.
pub fn script(kind: DenizenKind) -> &'static str {
    match kind {
        // Greek: heavy, rounded.
        DenizenKind::Minotaur => "αβγδζθκλμνξπρσφψω",
        // Cyrillic: blocky.
        DenizenKind::Cyclops => "бвгджзийклмнпфцчшщъыьэюя",
        // Macron, breve, acute and tilde letters: soft and lilting.
        DenizenKind::Faun => "āēīōūăĕĭŏŭĩũńĺŕśźćļņŗėġ",
        // Carons, circumflexes, strokes and hooks: sharp.
        DenizenKind::Demon => "ĉĝĥĵŝšžčřŧħłđƒƠơƯưǎǐǒǔǧǻǿ",
    }
}

/// `text` in `kind`'s script: every letter and digit becomes a glyph of the script, and spaces
/// and punctuation stay, so the word lengths and the rhythm of the line show through. The same
/// `kind`, `text` and `seed` always give the same glyphs.
pub fn glyphs(kind: DenizenKind, text: &str, seed: u32) -> String {
    let set: Vec<char> = script(kind).chars().collect();
    let salt = text.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193));
    let mut rng = Mulberry32::new(seed ^ salt.rotate_left(7) ^ (kind as u32 + 1).wrapping_mul(0x9e37_79b9));
    text.chars().map(|c| if c.is_alphanumeric() { *rng.pick(&set) } else { c }).collect()
}
