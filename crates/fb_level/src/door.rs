//! Where a door's leaf hangs and rests: one rule for the game, which draws the leaf, and for
//! the folk placement, which keeps clear of it.
//!
//! A doorway's frame has `x` across the opening, `z` through it, and the leaf hinges on the
//! frame's `-x` jamb, on its `+z` face. Doors only run along the grid axes, and `through` is
//! always `+x` or `+y` (grid), so the hinge side is fixed by the world, not by which room the
//! viewer stands in. Open, the leaf stands turned 85 degrees into `+through`.

use crate::{CELL, Level};

/// A doorway's opening, in world units: 1.3 m wide and 2.2 m high.
pub const DOOR_WIDTH: f32 = 13.0;
/// The timber frame's depth through the wall.
pub const FRAME_DEPTH: f32 = 3.6;
/// The leaf lies over the frame, so is larger than the opening by this much on every side.
pub const LEAF_LAP: f32 = 0.6;
pub const LEAF_THICKNESS: f32 = 1.0;
/// How far a door stands open, degrees from shut.
pub const OPEN_DEGREES: f32 = 85.0;

/// The hinge line in the doorway's frame: (across, through), world units from the cell centre.
pub const HINGE_ACROSS: f32 = -(DOOR_WIDTH * 0.5 + LEAF_LAP);
pub const HINGE_THROUGH: f32 = FRAME_DEPTH * 0.5 + LEAF_THICKNESS * 0.5 + 0.05;
/// Half a standing figure's width, in cells, kept clear of the leaf.
pub const FOLK_RADIUS: f32 = 0.3;
/// The leaf's width, hinge to free edge.
pub const LEAF_WIDTH: f32 = DOOR_WIDTH + LEAF_LAP * 2.0;

/// The cell-grid direction through a door cell: `(1, 0)` or `(0, 1)`, the way its frame is
/// built (rock on both sides across the other axis), or `None` where it gets no frame.
pub fn through(level: &Level, x: i32, y: i32) -> Option<(i32, i32)> {
    if !level.is_open(x, y - 1) && !level.is_open(x, y + 1) {
        Some((1, 0))
    } else if !level.is_open(x - 1, y) && !level.is_open(x + 1, y) {
        Some((0, 1))
    } else {
        None
    }
}

/// Where a door's leaf hangs, in cells of the grid (fractional).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeafSweep {
    pub hinge: (f32, f32),
    /// Unit grid vector along the shut leaf, hinge to free edge.
    pub across: (f32, f32),
    /// Unit grid vector the open leaf lies along: through the doorway.
    pub through: (i32, i32),
    /// The leaf's width in cells.
    pub width: f32,
}

impl LeafSweep {
    /// The sweep of the door in cell `(x, y)`.
    pub fn of(level: &Level, x: i32, y: i32) -> Option<LeafSweep> {
        let t = through(level, x, y)?;
        // The frame's x axis is cross(up, through) in world, which is grid (0, -1) for through
        // (1, 0) and (1, 0) for through (0, 1).
        let across = (t.1 as f32, -(t.0 as f32));
        let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
        let (a, b) = (HINGE_ACROSS / CELL, HINGE_THROUGH / CELL);
        Some(LeafSweep { hinge: (cx + across.0 * a + t.0 as f32 * b, cy + across.1 * a + t.1 as f32 * b), across, through: t, width: LEAF_WIDTH / CELL })
    }

    /// Whether a body of `radius` cells at `(px, py)` is in the quarter disc the leaf sweeps
    /// from shut to open.
    pub fn sweeps(&self, px: f32, py: f32, radius: f32) -> bool {
        let (dx, dy) = (px - self.hinge.0, py - self.hinge.1);
        let u = dx * self.across.0 + dy * self.across.1;
        let v = dx * self.through.0 as f32 + dy * self.through.1 as f32;
        u >= -radius && v >= -radius && dx.hypot(dy) <= self.width + radius
    }
}
