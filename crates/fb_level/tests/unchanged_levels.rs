//! Choosing finishes must not move anything else: a hash of every level for seeds 1..50 at
//! depths 1..4, taken before finishes existed.

use fb_level::generate;

/// FNV-1a over the debug text of each level.
fn all_levels_hash() -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for depth in 1..=4 {
        for seed in 1..=50 {
            for b in format!("{:?}", generate(seed, depth)).bytes() {
                h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
            }
        }
    }
    h
}

#[test]
fn levels_match_those_before_finishes() {
    assert_eq!(all_levels_hash(), 4133262373390105476, "snapshot taken before finishes");
}
