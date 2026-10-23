//! The denizens of the abyss: intelligent creatures of the deeper levels, who form factions of
//! their own beside the folk factions and hold ground the same way.
//!
//! - A [`DenizenKind`] is a people: minotaurs, cyclopes, fauns and demons. Each speaks its own
//!   tongue, which the delver learns word by word from books and scrolls; until then their
//!   speech is unreadable.
//! - A kind's members come in one or more bodies ([`DenizenKind::bodies`]): demons come in three
//!   ([`DenizenBody::WingedDemon`], [`DenizenBody::HornedDemon`], [`DenizenBody::Succubus`]), the
//!   others in one. The bodies themselves are `fb_creatures` species.
//! - Denizens live only at [`DENIZEN_DEPTH`] and below; shallower levels hold folk factions only.
//!
//! - [`tongue`] holds the tongues: each kind's vocabulary and native words, the script its speech
//!   shows in until it is learned, what a book or scroll teaches, and the delver's `Lexicon`.
//!
//! Plain Rust, no Bevy; everything here is a pure function of its inputs.

pub mod tongue;

use serde::{Deserialize, Serialize};

/// The shallowest level (1 is the first) on which denizen factions are found. Our own rule.
pub const DENIZEN_DEPTH: u32 = 10;

/// Whether denizen factions can be found on a level `depth` down.
pub fn denizens_at(depth: u32) -> bool {
    depth >= DENIZEN_DEPTH
}

/// A people of the abyss who are not folk, each with its own tongue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenizenKind {
    Minotaur,
    Cyclops,
    Faun,
    Demon,
}

impl DenizenKind {
    pub const ALL: [DenizenKind; 4] = [DenizenKind::Minotaur, DenizenKind::Cyclops, DenizenKind::Faun, DenizenKind::Demon];

    /// The key used in data tables and grammars ("minotaur").
    pub fn key(self) -> &'static str {
        match self {
            DenizenKind::Minotaur => "minotaur",
            DenizenKind::Cyclops => "cyclops",
            DenizenKind::Faun => "faun",
            DenizenKind::Demon => "demon",
        }
    }

    /// One of them, as the interface writes it ("Minotaur").
    pub fn label(self) -> &'static str {
        match self {
            DenizenKind::Minotaur => "Minotaur",
            DenizenKind::Cyclops => "Cyclops",
            DenizenKind::Faun => "Faun",
            DenizenKind::Demon => "Demon",
        }
    }

    /// The bodies its members come in.
    pub fn bodies(self) -> &'static [DenizenBody] {
        match self {
            DenizenKind::Minotaur => &[DenizenBody::Minotaur],
            DenizenKind::Cyclops => &[DenizenBody::Cyclops],
            DenizenKind::Faun => &[DenizenBody::Faun],
            DenizenKind::Demon => &[DenizenBody::WingedDemon, DenizenBody::HornedDemon, DenizenBody::Succubus],
        }
    }
}

/// The body a denizen stands in: one per kind, and three for demons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenizenBody {
    Minotaur,
    Cyclops,
    Faun,
    WingedDemon,
    HornedDemon,
    Succubus,
}

impl DenizenBody {
    pub const ALL: [DenizenBody; 6] =
        [DenizenBody::Minotaur, DenizenBody::Cyclops, DenizenBody::Faun, DenizenBody::WingedDemon, DenizenBody::HornedDemon, DenizenBody::Succubus];

    /// The people it belongs to.
    pub fn kind(self) -> DenizenKind {
        match self {
            DenizenBody::Minotaur => DenizenKind::Minotaur,
            DenizenBody::Cyclops => DenizenKind::Cyclops,
            DenizenBody::Faun => DenizenKind::Faun,
            DenizenBody::WingedDemon | DenizenBody::HornedDemon | DenizenBody::Succubus => DenizenKind::Demon,
        }
    }

    /// As the interface writes it ("Winged demon").
    pub fn label(self) -> &'static str {
        match self {
            DenizenBody::Minotaur => "Minotaur",
            DenizenBody::Cyclops => "Cyclops",
            DenizenBody::Faun => "Faun",
            DenizenBody::WingedDemon => "Winged demon",
            DenizenBody::HornedDemon => "Horned demon",
            DenizenBody::Succubus => "Succubus",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denizens_live_only_at_their_depth_and_below() {
        assert!(!denizens_at(1));
        assert!(!denizens_at(DENIZEN_DEPTH - 1));
        assert!(denizens_at(DENIZEN_DEPTH));
        assert!(denizens_at(DENIZEN_DEPTH + 10));
    }

    #[test]
    fn every_body_belongs_to_the_kind_that_lists_it() {
        for kind in DenizenKind::ALL {
            assert!(!kind.bodies().is_empty());
            assert!(kind.bodies().iter().all(|b| b.kind() == kind), "{kind:?}");
        }
        let listed: usize = DenizenKind::ALL.iter().map(|k| k.bodies().len()).sum();
        assert_eq!(listed, DenizenBody::ALL.len());
        assert_eq!(DenizenKind::Demon.bodies().len(), 3);
    }
}
