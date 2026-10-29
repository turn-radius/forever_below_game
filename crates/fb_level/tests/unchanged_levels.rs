//! Choosing finishes must not move anything else: a hash of every level for seeds 1..50 at
//! depths 1..4, taken before finishes existed, and taken again when creatures were kept out of
//! the rooms folk hold and centipedes moved to the second level (#448), and again when factions
//! came from the world seed (#536), and again when folk moved into faction claims (#541), and again
//! when claimed rooms took roles (#546): halls and hearths are lit and furnished, and folk stand by role,
//! and again when halls hung their banners (#551): `levels_match_those_before_finishes` (the whole level, so it
//! counts `Level::banners`) was retaken; `layout_items_and_lights_match_those_before_factions` was not, since
//! banners are a list of their own and move no wall, item, light, prop or folk. Both were retaken when claims
//! stopped taking rooms the wild ground needs (#541), so the way down is never behind a faction: the claims moved,
//! and with them the braziers lit in their halls and hearths, the folk, the props and the stairs; walls, rooms and items did not. And again when folk gained trades (#544).
//!
//! Floats are rounded to three decimals before hashing. Each platform's libm rounds `sin`,
//! `atan2` and the like its own way, so the last bits differ between macOS and the Linux CI
//! runner; the hash must not.

use fb_level::generate;

/// The level of `seed` at `depth` with its wall niches (#799) cleared: niches are drawn last, from
/// a stream of their own, and move nothing else, so clearing them must give the hash from before.
fn without_niches(seed: u32, depth: u32) -> fb_level::Level {
    let mut level = generate(seed, depth);
    level.niches.clear();
    level
}

/// FNV-1a over the debug text of each level, with its floats rounded.
fn all_levels_hash() -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for depth in 1..=4 {
        for seed in 1..=50 {
            // A level above the caverns grows no crystals (#658): the empty list the field prints is not a change.
            for b in round_floats(&format!("{:?}", without_niches(seed, depth)).replace(", crystals: []", "").replace(", niches: []", "")).bytes() {
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
    assert_eq!(all_levels_hash(), 16978683409277137263, "snapshot taken before finishes; retaken when guards took their posts at the hall door and traders stood in the market (#550), and again when those posts cleared the door leaves' real sweep, which moves folk and what keeps clear of them, and again when the hearth's brazier became a campfire (#569), which moves lights, props and the folk who gather round, and again when the door sweep followed door kinds and the doorway's real hinge, and again when corridors took widths by rank (#581), and when rough rock's main width became two to three, and again when levels took a past and cell blocks were carved (#587), rooms were furnished by their purpose (#588) and braziers left the front of cell gates, and again when claimed guardrooms kept their strongboxes (#617) and floor items left claimed rooms (#618), and again when wild groups took rooms of their own (#662) and the rats and snakes made way for them (#662), and again when faction names took words that say who the members are (#663), and again when factions were rolled from their names (#663), and again when folk gained a sex (#663), and when the factions kept their old rolls and a name that says a class rolled over (#663), and when faction names took words that say how members look (#682), and again when factions were rolled with the colours, armour and folk their names say (#682), and again when denizen factions were rolled (#627) and took the names' looks, which moved only the factions' likes and hates at these depths (levels_above_the_denizen_depth_are_unchanged pins the rest), and again when folk and claims grew with depth (#773): claims hold the corridors between their rooms and the folk total rises from depth 2, and again when one faction held each level from depth 4 down (#774), and again when levels grew rooms with depth, claims took the pockets they cut off and trades took workshops (#775)");
}

/// A hash of every level's layout, items and lights, which come before the folk in the level's
/// streams. Since #546 the lights include a brazier added to each claimed hall and hearth that had
/// none, so the lights, and with them this hash, changed on purpose (and again for #569, when a hearth's brazier became a campfire light, one fire to a room); walls, rooms and items did not
/// move. #587 carved cell blocks in the rooms a level's past puts to that purpose, changing cells on
/// purpose; the items and lights that would have stood on the bars moved with them. Before that, factions from the world seed (#536) moved the folk only; the props, stairs, webs and
/// creatures read where folk stand (to keep clear of them and out of their rooms), so those moved
/// with them and are held by [`levels_match_those_before_finishes`] instead. It names fields, so
/// it reads levels from before `factions` existed too.
fn all_levels_before_folk_hash() -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for depth in 1..=4 {
        for seed in 1..=50 {
            let l = generate(seed, depth);
            let text = format!(
                "{:?}",
                ((&l.seed, &l.depth, &l.style, &l.width, &l.height, &l.cells, &l.ceiling, &l.entrance), (&l.rooms, &l.items, &l.lights))
            );
            for b in round_floats(&text).bytes() {
                h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
            }
        }
    }
    h
}

#[test]
fn layout_items_and_lights_match_those_before_factions() {
    assert_eq!(all_levels_before_folk_hash(), 7045278524575133398, "snapshot of everything before the folk; (denizen factions, #627, moved nothing here: depths 1 to 4 are all above DENIZEN_DEPTH); retaken when corridors took widths by rank (#581), and when rough rock's main width became two to three, and when cell blocks were carved (#587), purposed rooms took braziers, books and loot off their walls (#588) and braziers left the front of cell gates, and again when floor items left claimed rooms (#618), and again when claims held the corridors between their rooms (#773), which moved the lights of their halls and the items that were in them, and again when one faction held each level from depth 4 down (#774), which moved the lights of the halls it took, and again when levels grew rooms with depth and claims took the pockets they cut off (#775)");
}

/// The text with every faction's `likes: [..], hates: [..]` taken out: the one part of a shallow
/// level that denizen factions moved, since any faction can like or hate any other (#627).
fn strip_tastes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("likes: [") {
        out.push_str(&rest[..i]);
        let after = &rest[i..];
        let end = after.find(", field: ").expect("a faction's tastes end at its field");
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// A hash of the levels of seeds 1..=50 at `depth`, without the `denizen: None` every folk spot
/// and faction gained (#627) nor the factions' likes and hates, so it can be compared with one
/// taken before either existed.
fn depth_hash(depth: u32) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for seed in 1..=50 {
        let text = strip_tastes(&round_floats(&format!("{:?}", without_niches(seed, depth)))).replace(", denizen: None", "").replace(", crystals: []", "").replace(", niches: []", "");
        for b in text.bytes() {
            h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// Levels above the depth denizens live at (`DENIZEN_DEPTH`, 10) are what they were before denizen
/// factions existed in everything but their factions' likes and hates, which may now name a denizen
/// faction: layout, items, lights, folk, props, creatures, claims, and the factions' names, folk
/// and looks. The hashes were taken on main (#701) before denizens were rolled, with the tastes
/// taken out, and again after main's faction makeup and sexes (#663, #682) were merged in. Both were retaken when claims grew with depth (#773), which moved claims and so folk, lights and items at these depths. Depths at or below `DENIZEN_DEPTH` are deliberately not pinned: they draw denizen
/// factions, so they differ from what they were, and the other hashes here are retaken for them.
#[test]
fn levels_above_the_denizen_depth_are_unchanged() {
    assert_eq!(depth_hash(1), 13345954563120765281);
    assert_eq!(depth_hash(2), 15484164110313931935);
}

