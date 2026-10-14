//! Finding the way across a level's grid: whether one point can see another
//! ([`line_of_sight`]), the shortest way between two cells round the rock ([`find_path`]), and
//! the furthest point along such a way that can be walked to straight ([`steer_toward`]).
//!
//! Points are in grid coordinates (cells, fractional), as everywhere in `fb_level`. Which cells
//! stop sight and movement is up to the caller (`blocked(x, y)`), so a shut door or a prop's
//! cell can count as rock.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Whether the straight line from `from` to `to` crosses no cell where `blocked` holds.
/// Walks every cell the line passes through, by Amanatides and Woo's grid traversal ("A Fast
/// Voxel Traversal Algorithm for Ray Tracing", Eurographics 1987,
/// <http://www.cse.yorku.ca/~amana/research/grid.pdf>); a line through a cell's corner counts
/// the cells either side, so sight never slips between two rock cells touching at a corner.
pub fn line_of_sight(from: (f32, f32), to: (f32, f32), blocked: impl Fn(i32, i32) -> bool) -> bool {
    let (mut x, mut y) = (from.0.floor() as i32, from.1.floor() as i32);
    let (end_x, end_y) = (to.0.floor() as i32, to.1.floor() as i32);
    if blocked(x, y) {
        return false;
    }
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let step_x = if dx > 0.0 { 1 } else { -1 };
    let step_y = if dy > 0.0 { 1 } else { -1 };
    // How far along the line (0..1) the next x and y cell boundaries lie, and the step between.
    let boundary = |p: f32, d: f32, step: i32| if d == 0.0 { f32::INFINITY } else { ((p.floor() + if step > 0 { 1.0 } else { 0.0 }) - p) / d };
    let mut t_x = boundary(from.0, dx, step_x);
    let mut t_y = boundary(from.1, dy, step_y);
    let delta_x = if dx == 0.0 { f32::INFINITY } else { (1.0 / dx).abs() };
    let delta_y = if dy == 0.0 { f32::INFINITY } else { (1.0 / dy).abs() };
    let mut guard = 0;
    while (x, y) != (end_x, end_y) && guard < 4096 {
        guard += 1;
        if (t_x - t_y).abs() < 1e-6 {
            // Through a corner: both neighbours must be open.
            if blocked(x + step_x, y) || blocked(x, y + step_y) {
                return false;
            }
            x += step_x;
            y += step_y;
            t_x += delta_x;
            t_y += delta_y;
        } else if t_x < t_y {
            if t_x > 1.0 {
                break;
            }
            x += step_x;
            t_x += delta_x;
        } else {
            if t_y > 1.0 {
                break;
            }
            y += step_y;
            t_y += delta_y;
        }
        if blocked(x, y) {
            return false;
        }
    }
    true
}

/// The cells of the shortest way from cell `from` to cell `to`, both included, stepping to any
/// of the eight neighbours but cutting no corner of rock (a diagonal step needs both cells
/// beside it open), or `None` if there is none within `limit` cells searched. A* search (Hart,
/// Nilsson and Raphael, "A Formal Basis for the Heuristic Determination of Minimum Cost
/// Paths", 1968) with the octile distance as its heuristic, which never overestimates on
/// such a grid, so the way found is a shortest one.
pub fn find_path(from: (i32, i32), to: (i32, i32), limit: usize, blocked: impl Fn(i32, i32) -> bool) -> Option<Vec<(i32, i32)>> {
    if blocked(from.0, from.1) || blocked(to.0, to.1) {
        return None;
    }
    // Costs in tenths of a cell: 10 straight, 14 diagonal.
    let octile = |a: (i32, i32)| {
        let (dx, dy) = ((a.0 - to.0).abs(), (a.1 - to.1).abs());
        10 * dx.max(dy) + 4 * dx.min(dy)
    };
    let mut open = BinaryHeap::new();
    let mut best: std::collections::HashMap<(i32, i32), (i32, (i32, i32))> = std::collections::HashMap::new();
    best.insert(from, (0, from));
    open.push(Reverse((octile(from), 0, from)));
    while let Some(Reverse((_, cost, at))) = open.pop() {
        if at == to {
            let mut path = vec![at];
            let mut cell = at;
            while cell != from {
                cell = best[&cell].1;
                path.push(cell);
            }
            path.reverse();
            return Some(path);
        }
        if best.get(&at).is_some_and(|&(c, _)| c < cost) {
            continue;
        }
        if best.len() > limit {
            return None;
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let next = (at.0 + dx, at.1 + dy);
            if blocked(next.0, next.1) {
                continue;
            }
            let diagonal = dx != 0 && dy != 0;
            if diagonal && (blocked(at.0 + dx, at.1) || blocked(at.0, at.1 + dy)) {
                continue;
            }
            let step = if diagonal { 14 } else { 10 };
            let next_cost = cost + step;
            if best.get(&next).is_none_or(|&(c, _)| next_cost < c) {
                best.insert(next, (next_cost, at));
                open.push(Reverse((next_cost + octile(next), next_cost, next)));
            }
        }
    }
    None
}

/// Where to head for from `at` along `path` (cells, as [`find_path`] gives) toward `goal`:
/// the goal itself if it can be seen, else the centre of the furthest cell of the path that
/// can ("string pulling"), so a creature cuts across a room rather than stepping from cell
/// centre to cell centre. `clearance` (cells) is how far either side of the line must also be
/// clear, for a body with some width.
pub fn steer_toward(at: (f32, f32), goal: (f32, f32), path: &[(i32, i32)], clearance: f32, blocked: impl Fn(i32, i32) -> bool) -> (f32, f32) {
    let clear = |to: (f32, f32)| {
        let (dx, dy) = (to.0 - at.0, to.1 - at.1);
        let length = dx.hypot(dy).max(1e-6);
        let (nx, ny) = (-dy / length * clearance, dx / length * clearance);
        [(0.0, 0.0), (nx, ny), (-nx, -ny)].iter().all(|&(ox, oy)| line_of_sight((at.0 + ox, at.1 + oy), (to.0 + ox, to.1 + oy), &blocked))
    };
    if clear(goal) {
        return goal;
    }
    let centre = |c: &(i32, i32)| (c.0 as f32 + 0.5, c.1 as f32 + 0.5);
    path.iter().rev().map(centre).find(|&p| clear(p)).or_else(|| path.get(1).or(path.first()).map(centre)).unwrap_or(goal)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map from rows of `#` (rock) and `.` (floor); outside it is rock.
    fn map(rows: &'static [&'static str]) -> impl Fn(i32, i32) -> bool + Copy {
        move |x, y| {
            if x < 0 || y < 0 {
                return true;
            }
            rows.get(y as usize).and_then(|row| row.as_bytes().get(x as usize)).is_none_or(|&c| c == b'#')
        }
    }

    const ROOMS: &[&str] = &[
        "##########", //
        "#....#...#",
        "#....#...#",
        "#....+...#",
        "#....#...#",
        "##########",
    ];

    fn rooms() -> impl Fn(i32, i32) -> bool + Copy {
        let blocked = map(ROOMS);
        // A doorway is open.
        move |x, y| blocked(x, y) && !(x == 5 && y == 3)
    }

    #[test]
    fn sight_stops_at_rock_and_passes_through_a_doorway() {
        let blocked = rooms();
        assert!(line_of_sight((1.5, 1.5), (4.5, 4.5), blocked), "across a room");
        assert!(!line_of_sight((2.5, 1.5), (7.5, 1.5), blocked), "through a wall");
        assert!(line_of_sight((2.5, 3.5), (8.5, 3.5), blocked), "through the doorway");
        assert!(!line_of_sight((1.5, 1.5), (8.5, 4.5), blocked), "past the doorway's jamb");
        assert!(line_of_sight((3.2, 2.7), (3.2, 2.7), blocked), "a point sees itself");
    }

    #[test]
    fn sight_never_slips_between_rock_touching_at_a_corner() {
        let blocked = map(&["....", ".#..", "..#.", "...."]);
        assert!(!line_of_sight((0.5, 3.5), (3.5, 0.5), blocked), "the diagonal between two rock cells is shut");
        assert!(line_of_sight((0.5, 0.5), (3.5, 3.5), map(&["....", "....", "....", "...."])));
    }

    #[test]
    fn a_path_goes_round_the_wall_and_through_the_doorway() {
        let blocked = rooms();
        let path = find_path((1, 1), (8, 1), 1000, blocked).unwrap();
        assert_eq!((path[0], *path.last().unwrap()), ((1, 1), (8, 1)));
        assert!(path.contains(&(5, 3)), "{path:?}");
        for w in path.windows(2) {
            let (a, b) = (w[0], w[1]);
            assert!((a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1, "{a:?} to {b:?}");
            assert!(!blocked(b.0, b.1));
            if a.0 != b.0 && a.1 != b.1 {
                assert!(!blocked(b.0, a.1) && !blocked(a.0, b.1), "cut a corner from {a:?} to {b:?}");
            }
        }
        // Shortest: four diagonals and straight steps, no detour.
        assert!(path.len() <= 9, "{path:?}");
    }

    #[test]
    fn no_path_through_solid_rock() {
        let sealed = map(&["#####", "#.#.#", "#####"]);
        assert_eq!(find_path((1, 1), (3, 1), 1000, sealed), None);
        assert_eq!(find_path((0, 0), (1, 1), 1000, sealed), None, "from rock");
        assert_eq!(find_path((1, 1), (1, 1), 1000, sealed), Some(vec![(1, 1)]));
    }

    #[test]
    fn steering_cuts_across_but_not_through_rock() {
        let blocked = rooms();
        let path = find_path((1, 1), (8, 1), 1000, blocked).unwrap();
        // From inside the first room the goal is hidden; head for the doorway's cell or short of it.
        let target = steer_toward((1.5, 1.5), (8.5, 1.5), &path, 0.1, blocked);
        assert!(line_of_sight((1.5, 1.5), target, blocked));
        assert!(target.0 > 2.0, "cuts across the room toward the door: {target:?}");
        // In view, straight at the goal.
        assert_eq!(steer_toward((6.5, 1.5), (8.5, 2.5), &[(6, 1), (7, 2), (8, 2)], 0.1, blocked), (8.5, 2.5));
    }

    #[test]
    fn seed_1207_joins_every_room_to_the_start() {
        let level = crate::generate(1207);
        let (sx, sy, _) = level.start();
        let blocked = |x, y| !level.is_open(x, y);
        for room in &level.rooms {
            let (cx, cy) = room.centre();
            let path = find_path((sx as i32, sy as i32), (cx as i32, cy as i32), 10_000, blocked);
            assert!(path.is_some(), "{room:?}");
        }
    }
}
