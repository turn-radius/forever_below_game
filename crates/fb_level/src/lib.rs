//! # fb_level
//!
//! One level of the abyss as plain data: a grid of cells, where the player comes in, the door
//! that shuts behind them, where items and lights lie, who stands about and what lurks there.
//! `generate(seed, depth)` is a pure function of the seed and depth, like every generator
//! here, so a level is the same every time on a platform.
//!
//! Grid coordinates: `x` runs east, `y` runs south, one cell is [`CELL`] world units (1 unit =
//! 10 cm). World position of a cell's centre is `(x + 0.5) * CELL, 0, (y + 0.5) * CELL`, with
//! the floor at world `y = 0` and +Y up. The game builds geometry and physics from this data.

use armory::Family;
use serde::{Deserialize, Serialize};

/// Width of one grid cell in world units (2 m).
pub const CELL: f32 = 20.0;

/// What fills one grid cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cell {
    /// Solid rock. Walls are drawn on the faces of rock cells that touch open cells.
    Rock,
    /// Open floor under the ceiling.
    Floor,
    /// A doorway: open floor with a door frame across it.
    Door,
    /// A column standing in a hall. It takes the whole 2 m cell and is solid, so walking,
    /// pathing and sight treat it as rock; it is not open. Unlike rock it stands inside a room,
    /// so the walls drawn on its faces are a stand-in until the geometry draws a real column.
    Column,
    /// Iron bars: the fronts and partitions of a cell block ([`past`]). Solid to walking and
    /// pathing like rock, but clear to sight.
    Bars,
    /// A gate in the bars of a cell block: open floor, drawn as a barred gate.
    Gate,
}

impl Cell {
    pub fn is_open(self) -> bool {
        !matches!(self, Cell::Rock | Cell::Column | Cell::Bars)
    }

    /// Whether sight passes through the cell: open cells and bars.
    pub fn is_clear(self) -> bool {
        self.is_open() || self == Cell::Bars
    }

    /// Whether it is a way through a wall or bars that nothing should stand in or beside: a
    /// door or a cell block's gate.
    pub fn is_doorway(self) -> bool {
        matches!(self, Cell::Door | Cell::Gate)
    }
}

/// A compass direction on the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Facing {
    North,
    East,
    South,
    West,
}

impl Facing {
    /// The grid step this facing points along, `(dx, dy)` with `y` south.
    pub fn step(self) -> (i32, i32) {
        match self {
            Facing::North => (0, -1),
            Facing::East => (1, 0),
            Facing::South => (0, 1),
            Facing::West => (-1, 0),
        }
    }

    /// Yaw in radians about +Y for a camera looking this way, with 0 looking north (-Z).
    pub fn yaw(self) -> f32 {
        match self {
            Facing::North => 0.0,
            Facing::West => std::f32::consts::FRAC_PI_2,
            Facing::South => std::f32::consts::PI,
            Facing::East => -std::f32::consts::FRAC_PI_2,
        }
    }
}

/// A rectangle of floor cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Room {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Where the player comes in: the entrance door's cell, and the way into the level from it.
/// The player starts one cell inward, facing `inward`; the door shuts behind them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entrance {
    pub x: u32,
    pub y: u32,
    pub inward: Facing,
}

/// An Armory item lying on the floor, at a point in grid coordinates (cells, fractional).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemSpot {
    pub x: f32,
    pub y: f32,
    /// Turn about +Y, radians.
    pub yaw: f32,
    pub family: Family,
    pub seed: u32,
}

/// What a fixed light in the level is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightKind {
    /// A glowcap cluster on the floor: faint cold light.
    Glowcaps,
    /// An iron brazier: warm, flickering, man-made.
    Brazier,
    /// A hearth's campfire, built from the Armory campfire family ([`crate::Prop::Campfire`]
    /// stands at the same spot): warm, flickering, lit.
    Campfire,
    /// One of a crystal cavern's largest crystals, glowing in its own colour: the light sits at
    /// the crystal's own spot ([`Level::crystal_lit_by`] finds it), so the kind needs no colour.
    Crystal,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LightSpot {
    pub x: f32,
    pub y: f32,
    pub kind: LightKind,
}

/// How a level is built, chosen by its depth ([`LevelStyle::for_depth`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LevelStyle {
    /// The first level's rough rock: small chambers and winding corridors.
    RoughRock,
    /// Wider, more regular rooms, colonnaded halls and straighter corridors.
    StoneHalls,
    /// Deep caverns where crystals grow: a few large open chambers with uneven natural floors,
    /// pillars of rock standing in them, joined by wide passages. Worked rock like rough rock,
    /// only far bigger.
    CrystalCavern,
}

impl LevelStyle {
    /// The first depth a crystal cavern can lie at.
    pub const FIRST_CAVERN_DEPTH: u32 = 5;

    /// The style of the level at `depth` (1 is the first level): rough rock at odd depths and
    /// stone halls at even ones, so the two alternate on the way down, and from depth 5 on every
    /// second rough rock level (5, 9, 13...) is a crystal cavern instead. Shallower levels never
    /// roll one.
    pub fn for_depth(depth: u32) -> Self {
        let depth = depth.max(1);
        if depth % 2 == 0 {
            LevelStyle::StoneHalls
        } else if depth >= Self::FIRST_CAVERN_DEPTH && depth % 4 == 1 {
            LevelStyle::CrystalCavern
        } else {
            LevelStyle::RoughRock
        }
    }

    /// Whether the level is worked rock, not dressed stone: rough rock and the crystal caverns
    /// share the rock's relief, finishes and doors.
    pub fn is_rock(self) -> bool {
        !matches!(self, LevelStyle::StoneHalls)
    }
}

/// One level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub seed: u32,
    /// How deep this level lies, 1 for the first.
    pub depth: u32,
    pub style: LevelStyle,
    pub width: u32,
    pub height: u32,
    /// Row-major, `cells[y * width + x]`.
    pub cells: Vec<Cell>,
    /// Ceiling height in world units.
    pub ceiling: f32,
    pub entrance: Entrance,
    pub rooms: Vec<Room>,
    pub items: Vec<ItemSpot>,
    pub lights: Vec<LightSpot>,
    /// Who stands in the level, in groups by faction ([`folk`]).
    pub folk: Vec<FolkSpot>,
    /// The two to four factions of the world's pool this level holds, whole, so the level names
    /// them without the world; each [`FolkSpot::faction`] is one of them.
    pub factions: Vec<FactionSpec>,
    /// The ground each of those factions holds, as rooms ([`claims`]). A room in no claim is
    /// wild.
    pub claims: Vec<Claim>,
    /// What each room of [`Level::rooms`] is for, indexed by room: empty for a wild room
    /// ([`roles`]).
    pub roles: Vec<Vec<RoomRole>>,
    /// Containers and scenery standing along the walls ([`props`]).
    pub props: Vec<PropSpot>,
    /// Faction banners hung on the walls of claimed rooms and beside their doors ([`banners`]).
    #[serde(default)]
    pub banners: Vec<BannerSpot>,
    /// Crystals growing in a crystal cavern ([`crystals`]); none on any other level.
    #[serde(default)]
    pub crystals: Vec<CrystalSpot>,
    /// Rats and snakes ([`creatures`]).
    pub creatures: Vec<CreatureSpot>,
    /// The ways to the levels above and below ([`stairs`]).
    pub stairs: Vec<Stair>,
    /// Webs hung in the room where giant spiders nest ([`wild`]).
    pub webs: Vec<WebSpot>,
    /// What the level was built for, if it remembers ([`past`]).
    #[serde(default)]
    pub past: Option<Past>,
    /// What each room of [`Level::rooms`] was built for, indexed by room ([`past`]).
    #[serde(default)]
    pub purposes: Vec<Option<Purpose>>,
}

impl Level {
    /// The spec of one of this level's factions (the [`FolkSpot::faction`] of anyone on it).
    ///
    /// # Panics
    /// If the level does not hold that faction.
    pub fn faction(&self, id: Faction) -> &FactionSpec {
        self.factions.iter().find(|f| f.id == id).unwrap_or_else(|| panic!("faction {} is not on this level", id.0))
    }

    /// The names of the factions on this level, as the world's items name them in their legends.
    pub fn faction_names(&self) -> Vec<String> {
        self.factions.iter().map(|f| f.name.clone()).collect()
    }

    /// The crystal a [`LightKind::Crystal`] light shines from: the one growing at its spot.
    pub fn crystal_lit_by(&self, light: &LightSpot) -> Option<&CrystalSpot> {
        if light.kind != LightKind::Crystal {
            return None;
        }
        self.crystals.iter().find(|c| c.x == light.x && c.y == light.y)
    }

    /// The cell at `(x, y)`, or rock outside the grid.
    pub fn cell(&self, x: i32, y: i32) -> Cell {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return Cell::Rock;
        }
        self.cells[(y as u32 * self.width + x as u32) as usize]
    }

    /// Whether the player can stand in cell `(x, y)`.
    pub fn is_open(&self, x: i32, y: i32) -> bool {
        self.cell(x, y).is_open()
    }

    /// World position of a grid point (cells, fractional) on the floor.
    pub fn world(x: f32, y: f32) -> [f32; 3] {
        [x * CELL, 0.0, y * CELL]
    }

    /// The cell the player starts in (one step inward from the entrance) and the way they face.
    pub fn start(&self) -> (u32, u32, Facing) {
        let (dx, dy) = self.entrance.inward.step();
        ((self.entrance.x as i32 + dx) as u32, (self.entrance.y as i32 + dy) as u32, self.entrance.inward)
    }
}


impl Facing {
    /// The four facings, in a fixed order.
    pub const ALL: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

    /// The facing pointing the other way.
    pub fn opposite(self) -> Facing {
        match self {
            Facing::North => Facing::South,
            Facing::East => Facing::West,
            Facing::South => Facing::North,
            Facing::West => Facing::East,
        }
    }
}

impl Room {
    pub fn area(&self) -> u32 {
        self.w * self.h
    }

    /// Whether cell `(x, y)` is floor of this room.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x as i32 && y >= self.y as i32 && x < (self.x + self.w) as i32 && y < (self.y + self.h) as i32
    }

    /// How far the point (`x`, `y`) lies from the room's floor, in cells: 0 inside it.
    pub fn gap_to(&self, x: f32, y: f32) -> f32 {
        let dx = (self.x as f32 - x).max(x - (self.x + self.w) as f32).max(0.0);
        let dy = (self.y as f32 - y).max(y - (self.y + self.h) as f32).max(0.0);
        dx.hypot(dy)
    }

    /// The room's centre in grid coordinates (cells, fractional).
    pub fn centre(&self) -> (f32, f32) {
        (self.x as f32 + self.w as f32 / 2.0, self.y as f32 + self.h as f32 / 2.0)
    }
}

impl Level {
    /// The stair of `kind`, if the level has one. Every level has a way down; the first has no
    /// way up.
    pub fn stair(&self, kind: StairKind) -> Option<&Stair> {
        self.stairs.iter().find(|s| s.kind == kind)
    }

    /// The stair standing in cell `(x, y)`.
    pub fn stair_at(&self, x: i32, y: i32) -> Option<&Stair> {
        self.stairs.iter().find(|s| (s.x as i32, s.y as i32) == (x, y))
    }

    /// Where a delver stands on arriving at the level's stair of kind `at`, turned away from
    /// it: at its way up, the level's start ([`Level::start`], which below the first level is
    /// in front of its stair up); at its way down, in front of that. `None` if the level has no
    /// such stair.
    pub fn arrival(&self, at: StairKind) -> Option<(u32, u32, Facing)> {
        match at {
            StairKind::Up => Some(self.start()),
            StairKind::Down => self.stair(StairKind::Down).map(Stair::arrival),
        }
    }

    /// Index into [`Level::rooms`] of the room holding cell `(x, y)`, if any.
    pub fn room_at(&self, x: i32, y: i32) -> Option<usize> {
        self.rooms.iter().position(|r| r.contains(x, y))
    }

    /// Index into [`Level::rooms`] of the room the entrance leads to: the first room reached
    /// walking inward from the start.
    pub fn start_room(&self) -> Option<usize> {
        let (x, y, inward) = self.start();
        let (dx, dy) = inward.step();
        let (mut x, mut y) = (x as i32, y as i32);
        while self.is_open(x, y) {
            if let Some(i) = self.room_at(x, y) {
                return Some(i);
            }
            x += dx;
            y += dy;
        }
        None
    }
}

pub mod banners;
pub mod crystals;
pub mod claims;
pub mod doorkind;
pub mod creatures;
pub mod door;
pub mod finish;
pub mod lore;
pub mod folk;
mod generate;
pub mod params;
pub mod past;
pub mod sight;
pub mod path;
pub mod props;
pub mod roles;
pub mod rooms;
pub mod stairs;
pub mod trades;
pub mod webs;
pub mod wild;
pub use creatures::{CreatureSpot, GIANTS_FROM_DEPTH};
pub use banners::{BannerSite, BannerSpot};
pub use crystals::{CavernStone, CrystalSpot, CrystalSurface, crystal_state};
pub use claims::{Claim, settle_claims};
pub use folk::{DenizenBody, DenizenKind, Faction, FactionSpec, FolkSpot, Trade};
pub use props::{CONTAINER_SCALE, NO_LOOT, FloorShape, OPEN_LID, Prop, PropSpot, chest_state};
pub use finish::{Character, DoorStyle, Finish, Finishes, FloorFinish, Quarter, WallFinish, finishes};
pub use generate::{generate, generate_for, generate_in, generate_with};
pub use stairs::{Stair, StairKind, depth_seed};
pub use params::LevelParams;
pub use past::{Past, Purpose};
pub use doorkind::{DoorKind, STEEL_PICK_PENALTY, SwitchKind};
pub use roles::{ClaimDoor, RoomRole};
pub use rooms::RoomGraph;
pub use wild::{WebKind, WebSpot};
