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
}

impl Cell {
    pub fn is_open(self) -> bool {
        !matches!(self, Cell::Rock | Cell::Column)
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
}

impl LevelStyle {
    /// The style of the level at `depth` (1 is the first level): rough rock at odd depths and
    /// stone halls at even ones, so the two alternate on the way down.
    pub fn for_depth(depth: u32) -> Self {
        if depth.max(1) % 2 == 1 { LevelStyle::RoughRock } else { LevelStyle::StoneHalls }
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
    /// Containers and scenery standing along the walls ([`props`]).
    pub props: Vec<PropSpot>,
    /// Rats and snakes ([`creatures`]).
    pub creatures: Vec<CreatureSpot>,
    /// The ways to the levels above and below ([`stairs`]).
    pub stairs: Vec<Stair>,
    /// Webs hung in the room where giant spiders nest ([`wild`]).
    pub webs: Vec<WebSpot>,
}

impl Level {
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

pub mod creatures;
pub mod finish;
pub mod folk;
mod generate;
pub mod params;
pub mod sight;
pub mod path;
pub mod props;
pub mod rooms;
pub mod stairs;
pub mod webs;
pub mod wild;
pub use creatures::{CreatureSpot, GIANTS_FROM_DEPTH};
pub use folk::{FACTIONS, Faction, FactionRow, FolkSpot};
pub use props::{CONTAINER_SCALE, FloorShape, OPEN_LID, Prop, PropSpot, chest_state};
pub use finish::{Character, DoorStyle, Finish, Finishes, FloorFinish, Quarter, WallFinish, finishes};
pub use generate::{generate, generate_with};
pub use stairs::{Stair, StairKind, depth_seed};
pub use params::LevelParams;
pub use rooms::RoomGraph;
pub use wild::{WebKind, WebSpot};
