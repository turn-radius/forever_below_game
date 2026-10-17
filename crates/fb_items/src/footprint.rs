//! Inventory grids: the cells an item covers, and the grid a container opens into.

use serde::{Deserialize, Serialize};

/// How many inventory cells an item covers (about 24 cm per cell, `docs/items.md`). The
/// tables write it as the plate's `[w, h]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "[u8; 2]", into = "[u8; 2]")]
pub struct Footprint {
    pub cols: u8,
    pub rows: u8,
}

impl From<[u8; 2]> for Footprint {
    fn from([cols, rows]: [u8; 2]) -> Self {
        Footprint { cols, rows }
    }
}

impl From<Footprint> for [u8; 2] {
    fn from(f: Footprint) -> Self {
        [f.cols, f.rows]
    }
}

/// The grid a container opens into (the plate's `holds`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grid {
    pub cols: u8,
    pub rows: u8,
    /// What the grid is called on screen ("Belt pouch").
    pub label: String,
    /// The families it takes; empty takes anything. A quiver takes arrows and a bolt case
    /// bolts, and later holders (scabbards, key rings, scroll cases) can name theirs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepts: Vec<armory::Family>,
}

impl Grid {
    /// Whether an item of `family` may be put in this grid.
    pub fn takes(&self, family: armory::Family) -> bool {
        self.accepts.is_empty() || self.accepts.contains(&family)
    }
}
