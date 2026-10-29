//! The search for bare wall faces that banners ([`crate::banners`]) and niches
//! ([`crate::niches`]) both make: floor cells of a room with rock behind one side, clear of
//! doors, and the test for whether a point on a wall stands too near a light or a prop.

use crate::folk::by_door;
use crate::{Cell, Facing, Level, Room};

/// Every wall face of `room`, as `(floor cell, grid step toward the rock)`, in row order and
/// the order of [`Facing::ALL`], skipping the cells beside a door. Callers shuffle the list
/// from their own stream.
pub(crate) fn bare_wall_faces(level: &Level, room: &Room) -> Vec<((i32, i32), (i32, i32))> {
    let mut faces = Vec::new();
    for y in room.y as i32..(room.y + room.h) as i32 {
        for x in room.x as i32..(room.x + room.w) as i32 {
            if level.cell(x, y) != Cell::Floor || by_door(level, x, y) {
                continue;
            }
            for f in Facing::ALL {
                let w = f.step();
                if level.cell(x + w.0, y + w.1) == Cell::Rock {
                    faces.push(((x, y), w));
                }
            }
        }
    }
    faces
}

/// Whether the point `(x, y)` in grid coordinates is within `gap` cells of a light or a prop.
pub(crate) fn near_light_or_prop(level: &Level, x: f32, y: f32, gap: f32) -> bool {
    let near = |ax: f32, ay: f32| (ax - x).hypot(ay - y) < gap;
    level.lights.iter().any(|l| near(l.x, l.y)) || level.props.iter().any(|p| near(p.x, p.y))
}

/// Every wall face of the level's corridors: floor cells in no room with rock behind one side,
/// clear of doors, in row order. Banners never use these; niches do.
pub(crate) fn corridor_wall_faces(level: &Level) -> Vec<((i32, i32), (i32, i32))> {
    let mut faces = Vec::new();
    for y in 0..level.height as i32 {
        for x in 0..level.width as i32 {
            if level.cell(x, y) != Cell::Floor || by_door(level, x, y) || level.rooms.iter().any(|r| r.contains(x, y)) {
                continue;
            }
            for f in Facing::ALL {
                let w = f.step();
                if level.cell(x + w.0, y + w.1) == Cell::Rock {
                    faces.push(((x, y), w));
                }
            }
        }
    }
    faces
}
