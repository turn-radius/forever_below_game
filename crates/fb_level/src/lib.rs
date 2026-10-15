//! # fb_level
//!
//! One level of the abyss as plain data: a grid of cells, where the player comes in, the door
//! that shuts behind them, where items and lights lie, who stands about and what lurks there. `generate(seed)` is a pure function
//! of the seed, like every generator here, so a level is the same on every machine.
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
}

impl Cell {
    pub fn is_open(self) -> bool {
        !matches!(self, Cell::Rock)
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

/// One level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub seed: u32,
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
pub mod folk;
mod generate;
pub mod params;
pub mod sight;
pub mod path;
pub mod props;
pub use creatures::CreatureSpot;
pub use folk::{FACTIONS, Faction, FactionRow, FolkSpot};
pub use props::{CONTAINER_SCALE, FloorShape, OPEN_LID, Prop, PropSpot, chest_state};
pub use generate::{generate, generate_with};
pub use params::LevelParams;
