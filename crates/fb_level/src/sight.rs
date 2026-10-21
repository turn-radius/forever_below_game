//! What the delver has seen of a level, for the map ([`Explored`]). A cell is seen once it is
//! in the eye's view, in line of sight across the grid, and lit: within reach of a light that
//! itself has a line to it. A dark room stays unseen until a light reaches it, and nothing
//! behind rock is ever seen.
//!
//! Positions here are in cells: `x` east, `y` south, a cell's centre at `(x + 0.5, y + 0.5)`.
//! Only open cells are recorded; the map draws the walls round them.

use crate::Level;

/// The eye: where it stands, which way it looks (a direction on the grid), how wide it sees
/// to either side (radians) and how far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eye {
    pub x: f32,
    pub y: f32,
    pub forward: (f32, f32),
    pub half_fov: f32,
    pub range: f32,
}

/// A light: where it shines from and how far it lights, in cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lamp {
    pub x: f32,
    pub y: f32,
    pub reach: f32,
}

/// Whether nothing but open cells lies between two points ([`crate::path::line_of_sight`]).
pub fn line_of_sight(level: &Level, from: (f32, f32), to: (f32, f32)) -> bool {
    crate::path::line_of_sight(from, to, |x, y| !level.is_open(x, y))
}

/// Points of a cell a line may reach to see it: its middle and a little in from each corner,
/// so a cell half hidden behind a door frame still counts.
fn points_of(x: i32, y: i32) -> [(f32, f32); 5] {
    let (x, y) = (x as f32, y as f32);
    [(x + 0.5, y + 0.5), (x + 0.15, y + 0.15), (x + 0.85, y + 0.15), (x + 0.15, y + 0.85), (x + 0.85, y + 0.85)]
}

/// Whether some point of cell `(x, y)` is in line of sight from `from`.
fn sees_cell(level: &Level, from: (f32, f32), x: i32, y: i32) -> bool {
    points_of(x, y).iter().any(|&p| line_of_sight(level, from, p))
}

/// Every open cell `eye` sees lit by one of `lamps`, as indices into [`Level::cells`].
pub fn seen_cells(level: &Level, eye: &Eye, lamps: &[Lamp]) -> Vec<usize> {
    let (fx, fy) = eye.forward;
    let len = (fx * fx + fy * fy).sqrt().max(1e-6);
    let (fx, fy) = (fx / len, fy / len);
    let cos_half = eye.half_fov.cos();
    let reach = eye.range.ceil() as i32;
    let (ex, ey) = (eye.x.floor() as i32, eye.y.floor() as i32);
    let mut seen = Vec::new();
    for y in (ey - reach).max(0)..=(ey + reach).min(level.height as i32 - 1) {
        for x in (ex - reach).max(0)..=(ex + reach).min(level.width as i32 - 1) {
            if !level.is_open(x, y) {
                continue;
            }
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            let (vx, vy) = (cx - eye.x, cy - eye.y);
            let distance = (vx * vx + vy * vy).sqrt();
            // The cell stood in, and those beside it, are always in view.
            let near = (x - ex).abs() <= 1 && (y - ey).abs() <= 1;
            if distance > eye.range || (!near && (vx * fx + vy * fy) / distance < cos_half) {
                continue;
            }
            if !near && !sees_cell(level, (eye.x, eye.y), x, y) {
                continue;
            }
            let lit = lamps.iter().any(|l| (l.x - cx).hypot(l.y - cy) <= l.reach + 0.5 && sees_cell(level, (l.x, l.y), x, y));
            if lit {
                seen.push((y as u32 * level.width + x as u32) as usize);
            }
        }
    }
    seen
}

/// The cells of one level seen so far, kept for the run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Explored {
    pub width: u32,
    pub height: u32,
    cells: Vec<bool>,
}

impl Explored {
    /// Nothing of `level` seen yet.
    pub fn new(level: &Level) -> Self {
        Explored { width: level.width, height: level.height, cells: vec![false; level.cells.len()] }
    }

    /// Whether cell `(x, y)` has been seen.
    pub fn is_seen(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32 && self.cells[(y as u32 * self.width + x as u32) as usize]
    }

    /// How many cells have been seen.
    pub fn count(&self) -> usize {
        self.cells.iter().filter(|&&s| s).count()
    }

    /// Marks what `eye` sees lit by `lamps`. Returns how many cells were seen for the first
    /// time.
    pub fn look(&mut self, level: &Level, eye: &Eye, lamps: &[Lamp]) -> usize {
        let mut new = 0;
        for i in seen_cells(level, eye, lamps) {
            if !self.cells[i] {
                self.cells[i] = true;
                new += 1;
            }
        }
        new
    }

    /// Every cell seen, for a level view that shows it all.
    pub fn all(level: &Level) -> Self {
        Explored { width: level.width, height: level.height, cells: level.cells.iter().map(|c| c.is_open()).collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cell, Entrance, Facing, Room};

    /// A level drawn from rows of `#` (rock), `.` (floor) and `+` (door).
    fn drawn(rows: &[&str]) -> Level {
        let cells = rows
            .iter()
            .flat_map(|r| r.chars())
            .map(|c| match c {
                '.' => Cell::Floor,
                '+' => Cell::Door,
                _ => Cell::Rock,
            })
            .collect();
        Level {
            seed: 0,
            depth: 1,
            style: crate::LevelStyle::RoughRock,
            width: rows[0].len() as u32,
            height: rows.len() as u32,
            cells,
            ceiling: 30.0,
            entrance: Entrance { x: 0, y: 0, inward: Facing::East },
            rooms: vec![Room { x: 1, y: 1, w: 1, h: 1 }],
            items: Vec::new(),
            lights: Vec::new(),
            folk: Vec::new(),
            factions: Vec::new(),
            props: Vec::new(),
            creatures: Vec::new(),
            stairs: Vec::new(),
            webs: Vec::new(),
        }
    }

    /// Two rooms, west and east, joined by a door in the wall between them.
    const TWO_ROOMS: [&str; 7] = [
        "###########",
        "#....#....#",
        "#....#....#",
        "#....+....#",
        "#....#....#",
        "#....#....#",
        "###########",
    ];

    fn eye_at(x: f32, y: f32, forward: (f32, f32)) -> Eye {
        Eye { x, y, forward, half_fov: 1.0, range: 20.0 }
    }

    #[test]
    fn walking_into_a_lit_room_sees_it() {
        let level = drawn(&TWO_ROOMS);
        let mut explored = Explored::new(&level);
        // Standing in the west room with a torch, looking east.
        let eye = eye_at(1.5, 3.5, (1.0, 0.0));
        explored.look(&level, &eye, &[Lamp { x: 1.5, y: 3.5, reach: 6.0 }]);
        assert!(explored.is_seen(3, 1) && explored.is_seen(4, 5), "the room ahead is seen");
        assert!(explored.is_seen(5, 3), "and the doorway");
    }

    #[test]
    fn cells_behind_walls_stay_unseen() {
        let level = drawn(&TWO_ROOMS);
        let mut explored = Explored::new(&level);
        let eye = eye_at(2.5, 1.5, (1.0, 0.0));
        explored.look(&level, &eye, &[Lamp { x: 2.5, y: 1.5, reach: 20.0 }]);
        // Through the doorway a strip of the east room shows, but its corners behind the wall
        // do not.
        assert!(!explored.is_seen(6, 1), "behind the wall beside the door");
        assert!(!explored.is_seen(9, 1), "the far corner, out of line with the doorway");
    }

    #[test]
    fn a_dark_room_stays_unseen_until_it_is_lit() {
        let level = drawn(&TWO_ROOMS);
        let mut explored = Explored::new(&level);
        // At the doorway with no light, looking into the east room.
        let eye = eye_at(5.5, 3.5, (1.0, 0.0));
        explored.look(&level, &eye, &[]);
        assert!(!explored.is_seen(8, 3), "too dark to see");
        // A brazier in the east room lights it.
        explored.look(&level, &eye, &[Lamp { x: 8.5, y: 3.5, reach: 4.0 }]);
        assert!(explored.is_seen(8, 3) && explored.is_seen(9, 1));
        // A light behind the wall lights nothing on this side.
        let mut other = Explored::new(&level);
        other.look(&level, &eye_at(1.5, 3.5, (-1.0, 0.0)), &[Lamp { x: 8.5, y: 1.5, reach: 20.0 }]);
        assert!(!other.is_seen(1, 1), "a light can't reach through rock");
    }

    #[test]
    fn what_is_behind_the_eye_is_unseen_and_seen_cells_stay_seen() {
        let level = drawn(&TWO_ROOMS);
        let mut explored = Explored::new(&level);
        let lamp = [Lamp { x: 3.5, y: 3.5, reach: 10.0 }];
        explored.look(&level, &eye_at(3.5, 3.5, (1.0, 0.0)), &lamp);
        assert!(!explored.is_seen(1, 3), "behind, two cells back");
        let before = explored.count();
        // Turning round sees it; what was seen before stays seen.
        let new = explored.look(&level, &eye_at(3.5, 3.5, (-1.0, 0.0)), &lamp);
        assert!(new > 0 && explored.is_seen(1, 3) && explored.is_seen(4, 3));
        assert_eq!(explored.count(), before + new);
    }

    #[test]
    fn a_generated_level_is_seen_room_by_room() {
        let level = crate::generate(1207, 1);
        let (x, y, inward) = level.start();
        let (dx, dy) = inward.step();
        let mut explored = Explored::new(&level);
        let (ex, ey) = (x as f32 + 0.5 + dx as f32 * 3.0, y as f32 + 0.5 + dy as f32 * 3.0);
        explored.look(&level, &Eye { x: ex, y: ey, forward: (dx as f32, dy as f32), half_fov: 0.9, range: 30.0 }, &[Lamp { x: ex, y: ey, reach: 7.0 }]);
        let open = level.cells.iter().filter(|c| c.is_open()).count();
        assert!(explored.count() > 4 && explored.count() < open / 2, "{} of {open}", explored.count());
        assert!(explored.is_seen(ex as i32, ey as i32));
    }
}
