//! Choosing finishes must not move anything else: a hash of every level for seeds 1..50 at
//! depths 1..4, taken before finishes existed, and taken again when creatures were kept out of
//! the rooms folk hold and centipedes moved to the second level (#448).
//!
//! Floats are rounded to three decimals before hashing. Each platform's libm rounds `sin`,
//! `atan2` and the like its own way, so the last bits differ between macOS and the Linux CI
//! runner; the hash must not.

use fb_level::generate;

/// FNV-1a over the debug text of each level, with its floats rounded.
fn all_levels_hash() -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for depth in 1..=4 {
        for seed in 1..=50 {
            for b in round_floats(&format!("{:?}", generate(seed, depth))).bytes() {
                h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
            }
        }
    }
    h
}

/// Rewrites every float literal in debug text (`-1.2345678`, `3e-8`) to three decimals.
/// Integers and digits inside names are left alone.
fn round_floats(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < b.len() {
        let starts_number = b[i].is_ascii_digit()
            && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] == b'.'));
        if !starts_number {
            out.push(b[i] as char);
            i += 1;
            continue;
        }
        let start = i;
        let mut is_float = false;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
            is_float = true;
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i < b.len() && b[i] == b'e' {
            let mut j = i + 1;
            if j < b.len() && b[j] == b'-' {
                j += 1;
            }
            if j < b.len() && b[j].is_ascii_digit() {
                is_float = true;
                i = j;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
        }
        let literal = &text[start..i];
        if !is_float {
            out.push_str(literal);
            continue;
        }
        let rounded = format!("{:.3}", literal.parse::<f64>().unwrap());
        // A tiny negative that rounds to zero must not keep its sign on one platform only.
        if rounded == "0.000" && out.ends_with('-') {
            out.pop();
        }
        out.push_str(&rounded);
    }
    out
}

#[test]
fn rounding_touches_only_floats() {
    assert_eq!(
        round_floats("Room { x: 3, ceiling: 24.000001, yaw: -1.5707964, tiny: -2e-9, v2: 1 }"),
        "Room { x: 3, ceiling: 24.000, yaw: -1.571, tiny: 0.000, v2: 1 }"
    );
}

#[test]
fn levels_match_those_before_finishes() {
    assert_eq!(all_levels_hash(), 18114639057965414438, "snapshot taken before finishes");
}
