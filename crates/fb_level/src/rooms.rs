//! How far apart two places in a level are, counted in rooms.
//!
//! [`RoomGraph`] links two rooms that open into each other or into the same corridor (a run of
//! open cells outside every room). Two cells are as many rooms apart as the fewest links between
//! the rooms they lie in ([`RoomGraph::rooms_apart`]): 0 in the same room, 1 in the next. A cell
//! in a corridor lies in every room the corridor opens into, so a corridor never counts as a
//! room of its own.

use std::collections::{BTreeSet, VecDeque};

use crate::Level;

/// The rooms of a level and which of them open into each other. Built once per level
/// ([`RoomGraph::new`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomGraph {
    width: i32,
    height: i32,
    /// For each cell, row-major, the rooms it lies in: its own, or its corridor's.
    rooms_of: Vec<u32>,
    /// The rooms of each place in `rooms_of`, sorted.
    places: Vec<Vec<usize>>,
    /// For each room, the rooms one link away, sorted.
    links: Vec<Vec<usize>>,
}

/// No place: rock, a column, or a corridor that opens into no room.
const NOWHERE: u32 = u32::MAX;

const STEPS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

impl RoomGraph {
    pub fn new(level: &Level) -> RoomGraph {
        let (width, height) = (level.width as i32, level.height as i32);
        let index = |x: i32, y: i32| (y * width + x) as usize;
        // Each room is the place of the same number; corridors follow.
        let mut places: Vec<Vec<usize>> = (0..level.rooms.len()).map(|r| vec![r]).collect();
        let mut rooms_of = vec![NOWHERE; (width * height) as usize];
        let mut links = vec![BTreeSet::new(); level.rooms.len()];
        for y in 0..height {
            for x in 0..width {
                if !level.is_open(x, y) {
                    continue;
                }
                let Some(room) = level.room_at(x, y) else { continue };
                rooms_of[index(x, y)] = room as u32;
                for (dx, dy) in STEPS {
                    if let Some(next) = level.room_at(x + dx, y + dy).filter(|&next| next != room && level.is_open(x + dx, y + dy)) {
                        links[room].insert(next);
                    }
                }
            }
        }
        let mut seen = vec![false; rooms_of.len()];
        let mut corridor = Vec::new();
        for y in 0..height {
            for x in 0..width {
                if !level.is_open(x, y) || seen[index(x, y)] || level.room_at(x, y).is_some() {
                    continue;
                }
                // Flood the corridor, gathering the rooms it opens into.
                let place = places.len() as u32;
                let mut rooms = BTreeSet::new();
                let mut stack = vec![(x, y)];
                seen[index(x, y)] = true;
                corridor.clear();
                while let Some((cx, cy)) = stack.pop() {
                    corridor.push(index(cx, cy));
                    for (dx, dy) in STEPS {
                        let (nx, ny) = (cx + dx, cy + dy);
                        if !level.is_open(nx, ny) {
                            continue;
                        }
                        if let Some(room) = level.room_at(nx, ny) {
                            rooms.insert(room);
                        } else if !seen[index(nx, ny)] {
                            seen[index(nx, ny)] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
                for &a in &rooms {
                    links[a].extend(rooms.iter().copied().filter(|&b| b != a));
                }
                if !rooms.is_empty() {
                    for &cell in &corridor {
                        rooms_of[cell] = place;
                    }
                    places.push(rooms.into_iter().collect());
                }
            }
        }
        RoomGraph { width, height, rooms_of, places, links: links.into_iter().map(|l| l.into_iter().collect()).collect() }
    }

    /// The rooms cell `(x, y)` lies in: its own room, or every room its corridor opens into.
    /// Empty for rock, a column, or a corridor that leads to no room.
    pub fn rooms_of(&self, x: i32, y: i32) -> &[usize] {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return &[];
        }
        match self.rooms_of[(y * self.width + x) as usize] {
            NOWHERE => &[],
            place => &self.places[place as usize],
        }
    }

    /// The rooms one link from `room`.
    pub fn links(&self, room: usize) -> &[usize] {
        self.links.get(room).map_or(&[], Vec::as_slice)
    }

    /// The fewest links between the rooms cells `a` and `b` lie in: 0 when they share one.
    /// `None` when either lies in no room or no way joins them.
    pub fn rooms_apart(&self, a: (i32, i32), b: (i32, i32)) -> Option<u32> {
        let goal = self.rooms_of(b.0, b.1);
        let mut apart = vec![None; self.links.len()];
        let mut queue = VecDeque::new();
        for &room in self.rooms_of(a.0, a.1) {
            apart[room] = Some(0);
            queue.push_back(room);
        }
        while let Some(room) = queue.pop_front() {
            let here = apart[room]?;
            if goal.contains(&room) {
                return Some(here);
            }
            for &next in self.links(room) {
                if apart[next].is_none() {
                    apart[next] = Some(here + 1);
                    queue.push_back(next);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cell, Entrance, Facing, LevelStyle, Room};

    /// A level of `rows`: `#` rock, `.` floor, `+` a door; rooms as given.
    fn level(rows: &[&str], rooms: Vec<Room>) -> Level {
        let cells = rows.iter().flat_map(|row| row.chars()).map(|c| match c {
            '.' => Cell::Floor,
            '+' => Cell::Door,
            _ => Cell::Rock,
        });
        Level {
            seed: 0,
            depth: 1,
            style: LevelStyle::RoughRock,
            width: rows[0].len() as u32,
            height: rows.len() as u32,
            cells: cells.collect(),
            ceiling: 30.0,
            entrance: Entrance { x: 0, y: 0, inward: Facing::South },
            rooms,
            items: vec![],
            lights: vec![],
            folk: vec![],
            factions: vec![],
            claims: vec![],
            roles: vec![],
            props: vec![],
            banners: Vec::new(),
            creatures: vec![],
            stairs: vec![],
            webs: vec![],
            past: None,
            purposes: Vec::new(),
        }
    }

    fn room(x: u32, y: u32, w: u32, h: u32) -> Room {
        Room { x, y, w, h }
    }

    /// Four rooms in a row, A to D, each joined to the next by a corridor through a door, and an
    /// alcove off C.
    fn row_of_rooms() -> Level {
        level(
            &[
                "###############",
                "#..+.+..#..#..#",
                "#..#.#..+..+..#",
                "#..#...#.#.####",
                "###############",
            ],
            vec![room(1, 1, 2, 3), room(6, 1, 2, 2), room(9, 1, 2, 2), room(12, 1, 2, 2)],
        )
    }

    #[test]
    fn the_same_room_is_none_apart_and_each_corridor_is_one_link() {
        let graph = RoomGraph::new(&row_of_rooms());
        assert_eq!(graph.rooms_apart((1, 1), (2, 3)), Some(0));
        assert_eq!(graph.rooms_apart((1, 1), (6, 1)), Some(1), "A and B share a corridor");
        assert_eq!(graph.rooms_apart((1, 1), (12, 2)), Some(3));
        assert_eq!(graph.rooms_apart((12, 2), (1, 1)), Some(3), "the same both ways");
    }

    #[test]
    fn a_corridor_lies_in_every_room_it_opens_into() {
        let graph = RoomGraph::new(&row_of_rooms());
        assert_eq!(graph.rooms_of(4, 2), &[0, 1]);
        assert_eq!(graph.rooms_apart((4, 2), (1, 1)), Some(0));
        assert_eq!(graph.rooms_apart((4, 2), (9, 1)), Some(1));
        // The corridor from A also bends round into B from below.
        assert_eq!(graph.rooms_of(6, 3), &[0, 1]);
        assert_eq!(graph.rooms_of(8, 3), &[1, 2]);
        assert_eq!(graph.rooms_of(10, 3), &[2], "an alcove off C lies only in C");
        assert_eq!(graph.links(1), &[0, 2]);
        assert_eq!(graph.links(2), &[1, 3]);
    }

    #[test]
    fn rooms_whose_floors_meet_are_linked() {
        let joined = level(&["######", "#....#", "######"], vec![room(1, 1, 2, 1), room(3, 1, 2, 1)]);
        let graph = RoomGraph::new(&joined);
        assert_eq!(graph.links(0), &[1]);
        assert_eq!(graph.rooms_apart((1, 1), (4, 1)), Some(1));
    }

    #[test]
    fn rock_and_unjoined_rooms_are_nowhere() {
        let graph = RoomGraph::new(&row_of_rooms());
        assert_eq!(graph.rooms_of(0, 0), &[] as &[usize]);
        assert_eq!(graph.rooms_of(-1, 40), &[] as &[usize]);
        assert_eq!(graph.rooms_apart((0, 0), (1, 1)), None);
        let apart = level(&["#######", "#..#..#", "#######"], vec![room(1, 1, 2, 1), room(4, 1, 2, 1)]);
        assert_eq!(RoomGraph::new(&apart).rooms_apart((1, 1), (4, 1)), None);
    }

    #[test]
    fn every_room_of_a_generated_level_is_reached_from_its_start() {
        for depth in 1..=4 {
            let level = crate::generate(7, depth);
            let graph = RoomGraph::new(&level);
            let (x, y, _) = level.start();
            for (i, room) in level.rooms.iter().enumerate() {
                let (cx, cy) = (room.x as i32, room.y as i32);
                assert!(graph.rooms_apart((x as i32, y as i32), (cx, cy)).is_some(), "depth {depth}: room {i} cut off");
            }
        }
    }
}
