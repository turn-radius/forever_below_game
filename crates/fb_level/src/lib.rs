//! # fb_level
//!
//! One level of the abyss as plain data: a grid of cells, where the player comes in, the door
//! that shuts behind them, and where items and lights lie. `generate(seed)` is a pure function
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

/// The level for `seed`.
///
/// TODO(#54): rooms and corridors. Until then this is one hand-laid room with a corridor, so
/// the game can be built against the data.
pub fn generate(seed: u32) -> Level {
    let (width, height) = (12u32, 10u32);
    let mut cells = vec![Cell::Rock; (width * height) as usize];
    let mut open = |x: u32, y: u32, c: Cell| cells[(y * width + x) as usize] = c;
    for y in 2..7 {
        for x in 3..9 {
            open(x, y, Cell::Floor);
        }
    }
    for y in 7..9 {
        open(5, y, Cell::Floor);
    }
    open(5, 9, Cell::Door);
    let families = [Family::Mace, Family::Torch, Family::Food, Family::Sword, Family::Bags];
    let items = families
        .iter()
        .enumerate()
        .map(|(i, &family)| ItemSpot { x: 3.6 + i as f32 * 1.1, y: 2.7 + (i % 2) as f32 * 3.0, yaw: i as f32, family, seed: seed.wrapping_add(i as u32) })
        .collect();
    Level {
        seed,
        width,
        height,
        cells,
        ceiling: 30.0,
        entrance: Entrance { x: 5, y: 9, inward: Facing::North },
        rooms: vec![Room { x: 3, y: 2, w: 6, h: 5 }],
        items,
        lights: vec![LightSpot { x: 8.5, y: 2.5, kind: LightKind::Glowcaps }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_deterministic() {
        assert_eq!(generate(1207), generate(1207));
    }

    #[test]
    fn the_player_starts_on_open_floor_inside_the_entrance() {
        let level = generate(1207);
        let (x, y, _) = level.start();
        assert!(level.is_open(x as i32, y as i32));
        assert_eq!(level.cell(level.entrance.x as i32, level.entrance.y as i32), Cell::Door);
    }
}
