//! Where a door's leaf hangs and rests: one rule for the game, which draws the leaf, and for
//! the folk placement, which keeps clear of it.
//!
//! A doorway's frame has `x` across the opening, `z` through it, and the leaf hinges on the
//! frame's `-x` jamb, on its `+z` face. Doors only run along the grid axes, and `through` is
//! always `+x` or `+y` (grid), so the hinge side is fixed by the world, not by which room the
//! viewer stands in. Open, the leaf stands turned 85 degrees into `+through`.

use crate::{CELL, ClaimDoor, DoorKind, DoorStyle, Level};

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
/// A doorway's clear height at the middle of a timber frame.
pub const DOOR_HEIGHT: f32 = 22.0;
/// How far a claim door's hinge pin stands off the hinge-side jamb.
pub const HINGE_SET: f32 = 0.8;
/// The gap between a claim door's latch edge and its jamb, and between its head and the frame.
pub const CLEAR: f32 = 0.12;
/// How far back from the +Z face of the opening a swinging claim leaf's middle stands.
pub const LEAF_Z: f32 = 0.5;

/// The curve of a stone or rock-cut doorway's head: straight sides of half-width `hw` up to
/// `spring`, then two arcs of radius `r` about a centre `r - hw` across the axis. A round arch
/// has `r == hw`, so both arcs share a centre.
#[derive(Clone, Copy, Debug)]
pub struct Arch {
    pub hw: f32,
    pub spring: f32,
    pub r: f32,
}

impl Arch {
    pub fn new(style: DoorStyle) -> Self {
        let hw = DOOR_WIDTH * 0.5;
        match style {
            DoorStyle::PointedArch => {
                // An equilateral-ish point: the arcs' radius is the span, the crown about 26.
                let r = DOOR_WIDTH;
                let rise = (r * r - (r - hw) * (r - hw)).sqrt();
                Arch { hw, spring: 26.0 - rise, r }
            }
            // Clear height at the middle is a little over the door's.
            _ => Arch { hw, spring: DOOR_HEIGHT - hw + 0.5, r: hw },
        }
    }

    /// The height of the circle of `radius` over `x`, or `None` past its spring.
    pub fn height(&self, x: f32, radius: f32) -> Option<f32> {
        let dx = x.abs() - (self.hw - self.r);
        (radius * radius - dx * dx > 0.0 && x.abs() < self.hw + (radius - self.r)).then(|| self.spring + (radius * radius - dx * dx).sqrt())
    }
}

/// Half the opening's width of a doorway framed in `style`.
pub fn half_width(style: DoorStyle) -> f32 {
    match style {
        DoorStyle::Timber => DOOR_WIDTH * 0.5,
        _ => Arch::new(style).hw,
    }
}

/// The span a swinging claim leaf covers across the opening, in the doorway's frame: from its
/// hinge pin to the latch-side clearance.
pub fn leaf_span(style: DoorStyle) -> (f32, f32) {
    let hw = half_width(style);
    (-hw + HINGE_SET, hw - CLEAR)
}

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
    /// The sweep of the claim door `door` hung in a doorway framed in `style`: from the pin the
    /// game hangs its leaf on, as wide as that leaf. `None` where the door has no frame, or is a
    /// portcullis, which rises into the lintel and sweeps nothing.
    pub fn of(level: &Level, door: &ClaimDoor, style: DoorStyle) -> Option<LeafSweep> {
        if door.kind == DoorKind::Portcullis {
            return None;
        }
        let t = through(level, door.x, door.y)?;
        // The frame's x axis is cross(up, through) in world, which is grid (0, -1) for through
        // (1, 0) and (1, 0) for through (0, 1).
        let across = (t.1 as f32, -(t.0 as f32));
        let (cx, cy) = (door.x as f32 + 0.5, door.y as f32 + 0.5);
        let (x0, x1) = leaf_span(style);
        let (a, b) = (x0 / CELL, LEAF_Z / CELL);
        Some(LeafSweep { hinge: (cx + across.0 * a + t.0 as f32 * b, cy + across.1 * a + t.1 as f32 * b), across, through: t, width: (x1 - x0) / CELL })
    }

    /// The sweeps of every swinging claim door of `level`, by door cell, each in its own doorway's
    /// style.
    pub fn all(level: &Level) -> Vec<((i32, i32), LeafSweep)> {
        let doors = level.claim_doors();
        if doors.iter().all(|d| d.kind == DoorKind::Portcullis) {
            return Vec::new();
        }
        let finishes = crate::finishes(level, &crate::LevelParams::for_style(level.style));
        doors.iter().filter_map(|d| LeafSweep::of(level, d, finishes.door_style(d.x, d.y).unwrap_or(DoorStyle::Timber)).map(|s| ((d.x, d.y), s))).collect()
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
