//! The seeded layout: rooms, the corridors and doors that join them, the entrance, and then
//! the items and lights laid through the rooms.
//!
//! Two streams come from each level seed: one lays out the rock, one furnishes it. Changing
//! the loot tables or light counts never moves a wall.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use armory::{Family, Item};
use procgen_rng::Mulberry32;

use crate::{Cell, Entrance, Facing, ItemSpot, Level, LightKind, LightSpot, Room};

const WIDTH: u32 = 48;
const HEIGHT: u32 = 40;
/// Ceiling height in world units (3 m).
const CEILING: f32 = 30.0;
/// How many rooms a level has, inclusive.
const ROOM_COUNT: (u32, u32) = (6, 12);
/// Rock cells kept between two rooms, and between a room and the grid edge. Three leaves a
/// door on each side and a corridor cell between them.
const ROOM_GAP: u32 = 3;
const PLACEMENT_TRIES: u32 = 400;
/// Deepest rock between the entrance edge and the first room, so the start light is in reach.
const MAX_ENTRANCE_DEPTH: u32 = 6;
/// Extra cost for a corridor to turn, so corridors run straight and bend rarely.
const TURN_COST: u32 = 4;
/// Extra cost for a corridor to run beside another one rather than share or cross it.
const BESIDE_CORRIDOR_COST: u32 = 6;
/// Chance that a room links to its nearest unlinked neighbour as well, making loops.
const LOOP_CHANCE: f64 = 0.3;
/// Chance that a corridor reuses a door already on that side of the room.
const DOOR_REUSE_CHANCE: f64 = 0.6;
/// Salt for the furnishing stream.
const FURNISH_SALT: u32 = 0x5EED_17E5;
/// Rooms at least this big may hold a brazier.
const BRAZIER_MIN_AREA: u32 = 25;
/// Closest two spots (items or lights) may lie, in cells.
const SPOT_SPACING: f32 = 0.9;

/// What the abyss leaves lying about, by weight. Families not yet ported are skipped.
const LOOT: &[(Family, u32)] = &[
    (Family::Food, 12),
    (Family::Torch, 8),
    (Family::Lantern, 2),
    (Family::Bags, 6),
    (Family::Chests, 2),
    (Family::Sundry, 6),
    (Family::Dagger, 5),
    (Family::Club, 4),
    (Family::Sword, 4),
    (Family::Mace, 3),
    (Family::Axe, 3),
    (Family::Spear, 3),
    (Family::Staff, 2),
    (Family::Bow, 2),
    (Family::Crossbow, 1),
    (Family::Shield, 3),
    (Family::Helm, 3),
    (Family::Hood, 3),
    (Family::Hat, 2),
    (Family::Garment, 3),
    (Family::Belt, 2),
    (Family::Hands, 3),
    (Family::Legs, 2),
    (Family::Feet, 3),
    (Family::Ring, 1),
    (Family::Amulet, 1),
    (Family::Charm, 1),
];

/// The level for `seed`: rooms joined by corridors one cell wide, an entrance door on the
/// outer edge, items in every room but the first, and a few fixed lights. Most of it is dark;
/// the light the player carries is the main light.
pub fn generate(seed: u32) -> Level {
    (0u32..)
        .find_map(|attempt| try_generate(seed, attempt))
        .expect("some attempt lays out a connected level")
}

/// One try at a level. `None` if too few rooms fit or the corridors could not join them all;
/// [`generate`] then tries again from a stream derived from the same seed.
fn try_generate(seed: u32, attempt: u32) -> Option<Level> {
    let layout_seed = seed ^ attempt.wrapping_mul(0x9E37_79B9);
    let mut rng = Mulberry32::new(layout_seed);

    let rooms = place_rooms(&mut rng);
    if rooms.len() < ROOM_COUNT.0 as usize {
        return None;
    }
    let mut grid = Grid::new(WIDTH, HEIGHT);
    for room in &rooms {
        grid.fill(room, Cell::Floor);
    }
    let mut doors = vec![Vec::new(); rooms.len()];
    let entrance = place_entrance(&mut grid, &rooms, &mut doors, &mut rng)?;
    connect(&mut grid, &rooms, &mut doors, &mut rng);

    let mut level = Level {
        seed,
        width: WIDTH,
        height: HEIGHT,
        cells: grid.cells,
        ceiling: CEILING,
        entrance,
        rooms,
        items: Vec::new(),
        lights: Vec::new(),
    };
    let (sx, sy, _) = level.start();
    if !reachable_from(&level, sx as i32, sy as i32).iter().zip(&level.cells).all(|(&seen, c)| seen || !c.is_open()) {
        return None;
    }
    furnish(&mut level, &mut Mulberry32::new(layout_seed ^ FURNISH_SALT));
    Some(level)
}

// ---------------------------------------------------------------------------------------------
// The grid
// ---------------------------------------------------------------------------------------------

/// The cells being carved, before they become a [`Level`].
struct Grid {
    width: u32,
    height: u32,
    cells: Vec<Cell>,
}

impl Grid {
    fn new(width: u32, height: u32) -> Self {
        Self { width, height, cells: vec![Cell::Rock; (width * height) as usize] }
    }

    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32
    }

    fn index(&self, x: i32, y: i32) -> usize {
        (y as u32 * self.width + x as u32) as usize
    }

    fn xy(&self, index: usize) -> (i32, i32) {
        ((index as u32 % self.width) as i32, (index as u32 / self.width) as i32)
    }

    fn get(&self, x: i32, y: i32) -> Cell {
        if self.in_bounds(x, y) { self.cells[self.index(x, y)] } else { Cell::Rock }
    }

    fn set(&mut self, x: i32, y: i32, cell: Cell) {
        let i = self.index(x, y);
        self.cells[i] = cell;
    }

    fn fill(&mut self, room: &Room, cell: Cell) {
        for y in room.y..room.y + room.h {
            for x in room.x..room.x + room.w {
                self.set(x as i32, y as i32, cell);
            }
        }
    }

    /// Whether opening rock at `(x, y)` would make a 2×2 block of open cells, widening a
    /// corridor past one cell.
    fn would_widen(&self, x: i32, y: i32) -> bool {
        [(-1, -1), (0, -1), (-1, 0), (0, 0)].iter().any(|&(ox, oy)| {
            let square = [(x + ox, y + oy), (x + ox + 1, y + oy), (x + ox, y + oy + 1), (x + ox + 1, y + oy + 1)];
            square.iter().filter(|&&c| c != (x, y)).all(|&(cx, cy)| self.get(cx, cy).is_open())
        })
    }

    /// Whether `(x, y)` has an open cell beside it (not diagonally).
    fn beside_open(&self, x: i32, y: i32) -> bool {
        Facing::ALL.iter().any(|f| {
            let (dx, dy) = f.step();
            self.get(x + dx, y + dy).is_open()
        })
    }
}

/// Every cell reachable from `(x, y)` by steps between open cells, as a mask over
/// [`Level::cells`].
pub(crate) fn reachable_from(level: &Level, x: i32, y: i32) -> Vec<bool> {
    let mut seen = vec![false; level.cells.len()];
    if !level.is_open(x, y) {
        return seen;
    }
    let index = |x: i32, y: i32| (y as u32 * level.width + x as u32) as usize;
    let mut stack = vec![(x, y)];
    seen[index(x, y)] = true;
    while let Some((x, y)) = stack.pop() {
        for facing in Facing::ALL {
            let (dx, dy) = facing.step();
            let (nx, ny) = (x + dx, y + dy);
            if level.is_open(nx, ny) && !seen[index(nx, ny)] {
                seen[index(nx, ny)] = true;
                stack.push((nx, ny));
            }
        }
    }
    seen
}

// ---------------------------------------------------------------------------------------------
// Rooms
// ---------------------------------------------------------------------------------------------

/// Rooms of varied size scattered over the grid, kept [`ROOM_GAP`] apart.
fn place_rooms(rng: &mut Mulberry32) -> Vec<Room> {
    let target = rng.int(ROOM_COUNT.0, ROOM_COUNT.1) as usize;
    let mut rooms: Vec<Room> = Vec::with_capacity(target);
    for _ in 0..PLACEMENT_TRIES {
        if rooms.len() == target {
            break;
        }
        let room = roll_room(rng);
        if rooms.iter().all(|other| apart(&room, other)) {
            rooms.push(room);
        }
    }
    rooms
}

/// One room: mostly small chambers, now and then a hall.
fn roll_room(rng: &mut Mulberry32) -> Room {
    let grow = if rng.chance(0.2) { rng.int(3, 5) } else { 0 };
    let w = rng.int(3, 7) + grow;
    let h = rng.int(3, 7) + grow;
    let x = rng.int(ROOM_GAP, WIDTH - ROOM_GAP - w);
    let y = rng.int(ROOM_GAP, HEIGHT - ROOM_GAP - h);
    Room { x, y, w, h }
}

/// Whether two rooms keep at least [`ROOM_GAP`] rock cells between them on some axis.
fn apart(a: &Room, b: &Room) -> bool {
    a.x + a.w + ROOM_GAP <= b.x || b.x + b.w + ROOM_GAP <= a.x || a.y + a.h + ROOM_GAP <= b.y || b.y + b.h + ROOM_GAP <= a.y
}

/// Rock cells between `room` and the grid edge on `side`.
fn distance_to_edge(room: &Room, side: Facing) -> u32 {
    match side {
        Facing::North => room.y,
        Facing::South => HEIGHT - (room.y + room.h),
        Facing::West => room.x,
        Facing::East => WIDTH - (room.x + room.w),
    }
}

// ---------------------------------------------------------------------------------------------
// Doors and corridors
// ---------------------------------------------------------------------------------------------

/// A doorway in the rock beside a room, and the way out of the room through it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Door {
    x: i32,
    y: i32,
    out: Facing,
}

impl Door {
    /// The corridor cell just outside the door.
    fn outside(&self) -> (i32, i32) {
        let (dx, dy) = self.out.step();
        (self.x + dx, self.y + dy)
    }
}

/// The door cell on `side` of `room`, at `along` (an x for north and south, a y for east and
/// west).
fn door_on(room: &Room, side: Facing, along: u32) -> Door {
    let (x, y) = match side {
        Facing::North => (along as i32, room.y as i32 - 1),
        Facing::South => (along as i32, (room.y + room.h) as i32),
        Facing::West => (room.x as i32 - 1, along as i32),
        Facing::East => ((room.x + room.w) as i32, along as i32),
    };
    Door { x, y, out: side }
}

/// The span of cells along `side` of `room` where a door can sit.
fn span(room: &Room, side: Facing) -> (u32, u32) {
    match side {
        Facing::North | Facing::South => (room.x, room.x + room.w),
        Facing::East | Facing::West => (room.y, room.y + room.h),
    }
}

/// The entrance: a door on the outer edge nearest a room, and a straight corridor from it
/// into that room. Carved before the other corridors so they route around it. `None` if every
/// room lies deeper than [`MAX_ENTRANCE_DEPTH`].
fn place_entrance(grid: &mut Grid, rooms: &[Room], doors: &mut [Vec<Door>], rng: &mut Mulberry32) -> Option<Entrance> {
    let first_side = rng.int(0, 3) as usize;
    let mut nearest: Option<(u32, Facing, usize)> = None;
    for k in 0..4 {
        let side = Facing::ALL[(first_side + k) % 4];
        for (i, room) in rooms.iter().enumerate() {
            let d = distance_to_edge(room, side);
            if nearest.is_none_or(|(best, _, _)| d < best) {
                nearest = Some((d, side, i));
            }
        }
    }
    let (depth, side, i) = nearest?;
    if depth > MAX_ENTRANCE_DEPTH {
        return None;
    }
    let room = &rooms[i];

    let (lo, hi) = span(room, side);
    let along = rng.int(lo, hi - 1) as i32;
    let (mut x, mut y) = match side {
        Facing::North => (along, 0),
        Facing::South => (along, HEIGHT as i32 - 1),
        Facing::West => (0, along),
        Facing::East => (WIDTH as i32 - 1, along),
    };
    let inward = side.opposite();
    let (dx, dy) = inward.step();
    let entrance = Entrance { x: x as u32, y: y as u32, inward };
    grid.set(x, y, Cell::Door);
    loop {
        x += dx;
        y += dy;
        if room.contains(x + dx, y + dy) {
            grid.set(x, y, Cell::Door);
            doors[i].push(Door { x, y, out: side });
            return Some(entrance);
        }
        grid.set(x, y, Cell::Floor);
    }
}

/// Joins the rooms with corridors: a spanning tree over the rooms, so all are linked, plus a
/// few loops. A link that cannot be routed is skipped; the caller checks reachability.
fn connect(grid: &mut Grid, rooms: &[Room], doors: &mut [Vec<Door>], rng: &mut Mulberry32) {
    let blocked = room_halos(grid, rooms);
    for (a, b) in links(rooms, rng) {
        join(grid, &blocked, rooms, doors, a, b, rng);
    }
}

/// Cells corridors may not enter: the grid's outer ring, every room, and the ring of rock
/// around each room (where only doors are cut).
fn room_halos(grid: &Grid, rooms: &[Room]) -> Vec<bool> {
    let mut blocked = vec![false; grid.cells.len()];
    for (i, b) in blocked.iter_mut().enumerate() {
        let (x, y) = grid.xy(i);
        let edge = x == 0 || y == 0 || x == grid.width as i32 - 1 || y == grid.height as i32 - 1;
        let near_room = rooms.iter().any(|r| {
            x >= r.x as i32 - 1 && y >= r.y as i32 - 1 && x <= (r.x + r.w) as i32 && y <= (r.y + r.h) as i32
        });
        *b = edge || near_room;
    }
    blocked
}

/// Which rooms to join: Prim's spanning tree by centre distance, then for some rooms an extra
/// link to their nearest room not yet linked to them.
fn links(rooms: &[Room], rng: &mut Mulberry32) -> Vec<(usize, usize)> {
    let dist = |a: usize, b: usize| {
        let ((ax, ay), (bx, by)) = (rooms[a].centre(), rooms[b].centre());
        (ax - bx).powi(2) + (ay - by).powi(2)
    };
    let mut linked = vec![false; rooms.len()];
    linked[0] = true;
    let mut edges = Vec::with_capacity(rooms.len() + 2);
    while edges.len() + 1 < rooms.len() {
        let (a, b) = (0..rooms.len())
            .filter(|&a| linked[a])
            .flat_map(|a| (0..rooms.len()).filter(|&b| !linked[b]).map(move |b| (a, b)))
            .min_by(|&(a, b), &(c, d)| dist(a, b).total_cmp(&dist(c, d)))
            .expect("an unlinked room remains");
        linked[b] = true;
        edges.push((a, b));
    }
    for a in 0..rooms.len() {
        if !rng.chance(LOOP_CHANCE) {
            continue;
        }
        let has = |edges: &[(usize, usize)], b: usize| edges.iter().any(|&e| e == (a, b) || e == (b, a));
        let nearest = (0..rooms.len())
            .filter(|&b| b != a && !has(&edges, b))
            .min_by(|&b, &c| dist(a, b).total_cmp(&dist(a, c)));
        if let Some(b) = nearest {
            edges.push((a, b));
        }
    }
    edges
}

/// Cuts a door in each of two rooms and routes a corridor between them. Leaves the grid
/// untouched and returns `false` if no route exists.
fn join(grid: &mut Grid, blocked: &[bool], rooms: &[Room], doors: &mut [Vec<Door>], a: usize, b: usize, rng: &mut Mulberry32) -> bool {
    let from = pick_door(rooms, &doors[a], a, b, rng);
    let to = pick_door(rooms, &doors[b], b, a, rng);
    let Some(path) = route(grid, blocked, from.outside(), from.out, to.outside()) else {
        return false;
    };
    for (room, door) in [(a, from), (b, to)] {
        grid.set(door.x, door.y, Cell::Door);
        if !doors[room].contains(&door) {
            doors[room].push(door);
        }
    }
    for (x, y) in path {
        if grid.get(x, y) == Cell::Rock {
            grid.set(x, y, Cell::Floor);
        }
    }
    true
}

/// The door in room `a` for a corridor toward room `b`: on the side facing `b`, lined up with
/// `b` where the two overlap, never right beside another door. Sometimes reuses a door already
/// on that side.
fn pick_door(rooms: &[Room], doors: &[Door], a: usize, b: usize, rng: &mut Mulberry32) -> Door {
    let (room, other) = (&rooms[a], &rooms[b]);
    let side = facing_toward(room.centre(), other.centre());
    let existing: Vec<Door> = doors.iter().copied().filter(|d| d.out == side).collect();
    if !existing.is_empty() && rng.chance(DOOR_REUSE_CHANCE) {
        return *rng.pick(&existing);
    }

    let (lo, hi) = span(room, side);
    let (olo, ohi) = span(other, side);
    let clear = |along: &u32| {
        let door = door_on(room, side, *along);
        existing.iter().all(|d| (d.x - door.x).abs() + (d.y - door.y).abs() > 1)
    };
    let lined_up: Vec<u32> = (lo.max(olo)..hi.min(ohi)).filter(clear).collect();
    let anywhere: Vec<u32> = (lo..hi).filter(clear).collect();
    match (lined_up.is_empty(), anywhere.is_empty()) {
        (false, _) => door_on(room, side, *rng.pick(&lined_up)),
        (true, false) => door_on(room, side, *rng.pick(&anywhere)),
        (true, true) => *rng.pick(&existing),
    }
}

/// The compass side of a point at `from` that faces `to`, by the larger axis.
fn facing_toward(from: (f32, f32), to: (f32, f32)) -> Facing {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    if dx.abs() > dy.abs() {
        if dx > 0.0 { Facing::East } else { Facing::West }
    } else if dy > 0.0 {
        Facing::South
    } else {
        Facing::North
    }
}

/// The cheapest corridor from `from` (leaving along `heading`) to `to`, as the cells to open.
/// Dijkstra over (cell, heading), so turns cost extra and corridors run straight. Corridors
/// may share or cross open corridor, never widen one, and keep out of `blocked`.
fn route(grid: &Grid, blocked: &[bool], from: (i32, i32), heading: Facing, to: (i32, i32)) -> Option<Vec<(i32, i32)>> {
    let heading_index = |f: Facing| Facing::ALL.iter().position(|&g| g == f).expect("a facing");
    let state = |x: i32, y: i32, f: usize| grid.index(x, y) * 4 + f;
    let mut cost = vec![u32::MAX; grid.cells.len() * 4];
    let mut prev = vec![usize::MAX; grid.cells.len() * 4];
    let mut queue = BinaryHeap::new();
    let start = state(from.0, from.1, heading_index(heading));
    cost[start] = 0;
    queue.push(Reverse((0u32, start)));

    while let Some(Reverse((c, s))) = queue.pop() {
        if c > cost[s] {
            continue;
        }
        let (x, y) = grid.xy(s / 4);
        if (x, y) == to {
            let mut path = vec![(x, y)];
            let mut s = s;
            while prev[s] != usize::MAX {
                s = prev[s];
                path.push(grid.xy(s / 4));
            }
            return Some(path);
        }
        for (f, facing) in Facing::ALL.iter().enumerate() {
            let (dx, dy) = facing.step();
            let (nx, ny) = (x + dx, y + dy);
            if !grid.in_bounds(nx, ny) || blocked[grid.index(nx, ny)] {
                continue;
            }
            let open = grid.get(nx, ny).is_open();
            if !open && grid.would_widen(nx, ny) {
                continue;
            }
            let turn = if f == s % 4 { 0 } else { TURN_COST };
            let beside = if !open && grid.beside_open(nx, ny) { BESIDE_CORRIDOR_COST } else { 0 };
            let next = state(nx, ny, f);
            let nc = c + 1 + turn + beside;
            if nc < cost[next] {
                cost[next] = nc;
                prev[next] = s;
                queue.push(Reverse((nc, next)));
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------
// Items and lights
// ---------------------------------------------------------------------------------------------

/// Lays the start light, the fixed lights, and the loot.
fn furnish(level: &mut Level, rng: &mut Mulberry32) {
    let start_room = level.start_room().expect("the entrance leads into a room");
    let start_light = start_light(level, rng);
    level.items.push(start_light);
    level.lights = place_lights(level, start_room, rng);
    let loot = scatter_items(level, start_room, rng);
    level.items.extend(loot);
}

/// A full-range u32 seed for an item.
fn item_seed(rng: &mut Mulberry32) -> u32 {
    (rng.next_f64() * 4_294_967_296.0) as u32
}

/// One torch or lantern just inside the starting room, a step to the side of the way in.
fn start_light(level: &Level, rng: &mut Mulberry32) -> ItemSpot {
    let (x, y, inward) = level.start();
    let (dx, dy) = inward.step();
    let (mut cx, mut cy) = (x as i32, y as i32);
    while level.room_at(cx, cy).is_none() {
        cx += dx;
        cy += dy;
    }
    // One cell further in, then one to either side that stays in the room.
    let room = level.rooms[level.room_at(cx, cy).expect("inside the room")];
    let (cx, cy) = (cx + dx, cy + dy);
    let (sx, sy) = (dy, dx);
    let sides = if rng.chance(0.5) { [1, -1] } else { [-1, 1] };
    let (px, py) = sides
        .iter()
        .map(|&k| (cx + sx * k, cy + sy * k))
        .find(|&(px, py)| room.contains(px, py))
        .unwrap_or((cx, cy));
    let family = if rng.chance(0.7) || !Family::Lantern.is_ported() { Family::Torch } else { Family::Lantern };
    ItemSpot {
        x: px as f32 + rng.range_f32(0.35, 0.65),
        y: py as f32 + rng.range_f32(0.35, 0.65),
        yaw: rng.range_f32(0.0, std::f32::consts::TAU),
        family,
        seed: item_seed(rng),
    }
}

/// Braziers in one or two of the larger rooms, glowcaps along the walls of a few others. The
/// starting room and most of the rest stay dark.
fn place_lights(level: &Level, start_room: usize, rng: &mut Mulberry32) -> Vec<LightSpot> {
    let mut lights = Vec::new();
    let mut by_size: Vec<usize> = (0..level.rooms.len()).filter(|&i| i != start_room).collect();
    by_size.sort_by_key(|&i| Reverse(level.rooms[i].area()));
    let halls: Vec<usize> = by_size.iter().copied().filter(|&i| level.rooms[i].area() >= BRAZIER_MIN_AREA).collect();
    let lit_halls = match halls.len() {
        0 => 0,
        1 => 1,
        _ => rng.int(1, 2) as usize,
    };
    for &i in &halls[..lit_halls] {
        lights.extend(braziers(&level.rooms[i], rng));
    }

    let mut others: Vec<usize> = by_size.iter().copied().filter(|i| !halls[..lit_halls].contains(i)).collect();
    shuffle(&mut others, rng);
    // At most half the rooms carry any light.
    let room_budget = (level.rooms.len() / 2).saturating_sub(lit_halls).max(1);
    let glow_rooms = (rng.int(2, 3) as usize).min(room_budget).min(others.len());
    for &i in &others[..glow_rooms] {
        lights.extend(glowcaps(&level.rooms[i], rng));
    }
    lights
}

/// One brazier at the centre of a hall, or two along the middle of a long one.
fn braziers(room: &Room, rng: &mut Mulberry32) -> Vec<LightSpot> {
    let (cx, cy) = (room.x as f32 + (room.w / 2) as f32 + 0.5, room.y as f32 + (room.h / 2) as f32 + 0.5);
    let brazier = |x: f32, y: f32| LightSpot { x, y, kind: LightKind::Brazier };
    let long = room.w.max(room.h) >= 8;
    if !(long && rng.chance(0.6)) {
        return vec![brazier(cx, cy)];
    }
    if room.w >= room.h {
        let third = (room.w / 3) as f32;
        vec![brazier(room.x as f32 + third + 0.5, cy), brazier((room.x + room.w) as f32 - third - 0.5, cy)]
    } else {
        let third = (room.h / 3) as f32;
        vec![brazier(cx, room.y as f32 + third + 0.5), brazier(cx, (room.y + room.h) as f32 - third - 0.5)]
    }
}

/// One to three glowcap clusters at the foot of a room's walls.
fn glowcaps(room: &Room, rng: &mut Mulberry32) -> Vec<LightSpot> {
    (0..rng.int(1, 3))
        .map(|_| {
            let side = *rng.pick(&Facing::ALL);
            let (dx, dy) = side.step();
            let x = match dx {
                1 => (room.x + room.w) as f32 - 0.25,
                -1 => room.x as f32 + 0.25,
                _ => rng.range_f32(room.x as f32 + 0.3, (room.x + room.w) as f32 - 0.3),
            };
            let y = match dy {
                1 => (room.y + room.h) as f32 - 0.25,
                -1 => room.y as f32 + 0.25,
                _ => rng.range_f32(room.y as f32 + 0.3, (room.y + room.h) as f32 - 0.3),
            };
            LightSpot { x, y, kind: LightKind::Glowcaps }
        })
        .collect()
}

/// Loot through every room but the starting one: more in bigger rooms, a few rooms bare,
/// never in a doorway's path or on top of a light or another item.
fn scatter_items(level: &Level, start_room: usize, rng: &mut Mulberry32) -> Vec<ItemSpot> {
    let table: Vec<(Family, u32)> = LOOT.iter().copied().filter(|(f, _)| Item::PORTED.contains(f)).collect();
    let mut taken: Vec<(f32, f32)> = level.lights.iter().map(|l| (l.x, l.y)).chain(level.items.iter().map(|i| (i.x, i.y))).collect();
    let mut items = Vec::new();
    for (i, room) in level.rooms.iter().enumerate() {
        if i == start_room || rng.chance(0.15) {
            continue;
        }
        let count = (room.area() / 10).clamp(1, 5) + rng.int(0, 1);
        for _ in 0..count {
            let Some((x, y)) = free_spot(level, room, &taken, rng) else { continue };
            taken.push((x, y));
            items.push(ItemSpot {
                x,
                y,
                yaw: rng.range_f32(0.0, std::f32::consts::TAU),
                family: *rng.weighted(&table),
                seed: item_seed(rng),
            });
        }
    }
    items
}

/// A point in `room`, clear of doorways and at least [`SPOT_SPACING`] from every taken spot.
fn free_spot(level: &Level, room: &Room, taken: &[(f32, f32)], rng: &mut Mulberry32) -> Option<(f32, f32)> {
    (0..24).find_map(|_| {
        let cx = rng.int(room.x, room.x + room.w - 1) as i32;
        let cy = rng.int(room.y, room.y + room.h - 1) as i32;
        let x = cx as f32 + rng.range_f32(0.25, 0.75);
        let y = cy as f32 + rng.range_f32(0.25, 0.75);
        let by_door = Facing::ALL.iter().any(|f| {
            let (dx, dy) = f.step();
            level.cell(cx + dx, cy + dy) == Cell::Door
        });
        let crowded = taken.iter().any(|&(tx, ty)| (tx - x).hypot(ty - y) < SPOT_SPACING);
        (!by_door && !crowded).then_some((x, y))
    })
}

/// Fisher-Yates, from the level's stream.
fn shuffle<T>(items: &mut [T], rng: &mut Mulberry32) {
    for i in (1..items.len()).rev() {
        let j = rng.int(0, i as u32) as usize;
        items.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const SEEDS: std::ops::RangeInclusive<u32> = 1..=64;

    fn cell_of(level: &Level, x: f32, y: f32) -> Cell {
        level.cell(x.floor() as i32, y.floor() as i32)
    }

    #[test]
    fn levels_are_deterministic() {
        for seed in [0, 1, 1207, u32::MAX] {
            assert_eq!(generate(seed), generate(seed));
        }
        assert_ne!(generate(1).cells, generate(2).cells);
    }

    #[test]
    fn every_open_cell_is_reachable_from_the_start() {
        for seed in SEEDS {
            let level = generate(seed);
            let (x, y, _) = level.start();
            let seen = reachable_from(&level, x as i32, y as i32);
            for (i, cell) in level.cells.iter().enumerate() {
                assert!(!cell.is_open() || seen[i], "seed {seed}: cell {i} is cut off");
            }
        }
    }

    #[test]
    fn room_count_is_in_range() {
        for seed in SEEDS {
            let n = generate(seed).rooms.len() as u32;
            assert!((ROOM_COUNT.0..=ROOM_COUNT.1).contains(&n), "seed {seed}: {n} rooms");
        }
    }

    #[test]
    fn rooms_vary_in_size() {
        let sizes: HashSet<(u32, u32)> = SEEDS.flat_map(|s| generate(s).rooms).map(|r| (r.w, r.h)).collect();
        assert!(sizes.len() > 20, "only {} room sizes", sizes.len());
    }

    #[test]
    fn the_entrance_is_a_door_on_the_edge_opening_inward() {
        for seed in SEEDS {
            let level = generate(seed);
            let e = level.entrance;
            assert_eq!(level.cell(e.x as i32, e.y as i32), Cell::Door, "seed {seed}");
            let edge = match e.inward {
                Facing::South => e.y == 0,
                Facing::North => e.y == level.height - 1,
                Facing::East => e.x == 0,
                Facing::West => e.x == level.width - 1,
            };
            assert!(edge, "seed {seed}: entrance {e:?} is not on the edge it opens from");
            let (x, y, _) = level.start();
            assert!(level.is_open(x as i32, y as i32), "seed {seed}: start is in rock");
            assert!(level.start_room().is_some(), "seed {seed}: the entrance leads to no room");
        }
    }

    #[test]
    fn corridors_enter_rooms_through_doors() {
        for seed in SEEDS {
            let level = generate(seed);
            for (i, room) in level.rooms.iter().enumerate() {
                let mut doors = 0;
                for y in room.y as i32 - 1..=(room.y + room.h) as i32 {
                    for x in room.x as i32 - 1..=(room.x + room.w) as i32 {
                        if room.contains(x, y) {
                            continue;
                        }
                        let cell = level.cell(x, y);
                        assert_ne!(cell, Cell::Floor, "seed {seed}: room {i} opens at ({x}, {y}) without a door");
                        doors += (cell == Cell::Door) as u32;
                    }
                }
                assert!(doors > 0, "seed {seed}: room {i} has no door");
            }
        }
    }

    #[test]
    fn corridors_are_one_cell_wide() {
        for seed in SEEDS {
            let level = generate(seed);
            let corridor = |x: i32, y: i32| level.is_open(x, y) && level.room_at(x, y).is_none();
            for y in 0..level.height as i32 - 1 {
                for x in 0..level.width as i32 - 1 {
                    let square = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)];
                    assert!(!square.iter().all(|&(x, y)| corridor(x, y)), "seed {seed}: wide corridor at ({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn no_spot_lies_in_rock() {
        for seed in SEEDS {
            let level = generate(seed);
            let spots = level.items.iter().map(|i| (i.x, i.y)).chain(level.lights.iter().map(|l| (l.x, l.y)));
            for (x, y) in spots {
                assert_eq!(cell_of(&level, x, y), Cell::Floor, "seed {seed}: spot ({x}, {y}) is not on floor");
                assert!(level.room_at(x.floor() as i32, y.floor() as i32).is_some(), "seed {seed}: spot ({x}, {y}) is outside a room");
            }
        }
    }

    #[test]
    fn the_starting_room_holds_only_a_light_near_the_start() {
        for seed in SEEDS {
            let level = generate(seed);
            let start_room = level.start_room().unwrap();
            let inside: Vec<&ItemSpot> =
                level.items.iter().filter(|i| level.room_at(i.x.floor() as i32, i.y.floor() as i32) == Some(start_room)).collect();
            assert_eq!(inside.len(), 1, "seed {seed}: {} items in the starting room", inside.len());
            assert!(matches!(inside[0].family, Family::Torch | Family::Lantern), "seed {seed}");
            let (sx, sy, _) = level.start();
            let d = (inside[0].x - (sx as f32 + 0.5)).hypot(inside[0].y - (sy as f32 + 0.5));
            assert!(d < (MAX_ENTRANCE_DEPTH + 1) as f32, "seed {seed}: the start light is {d} cells from the start");
            assert!(level.lights.iter().all(|l| level.room_at(l.x as i32, l.y as i32) != Some(start_room)), "seed {seed}");
        }
    }

    #[test]
    fn items_cover_many_families() {
        let families: HashSet<Family> = (1..=32).flat_map(|s| generate(s).items).map(|i| i.family).collect();
        assert!(families.len() >= 8, "only {} families: {families:?}", families.len());
    }

    #[test]
    fn levels_have_braziers_glowcaps_and_dark_rooms() {
        for seed in SEEDS {
            let level = generate(seed);
            let lit: HashSet<usize> = level.lights.iter().filter_map(|l| level.room_at(l.x as i32, l.y as i32)).collect();
            assert!(level.lights.iter().any(|l| l.kind == LightKind::Glowcaps), "seed {seed}: no glowcaps");
            assert!(lit.len() * 2 <= level.rooms.len() + 1, "seed {seed}: {} of {} rooms lit", lit.len(), level.rooms.len());
        }
        let braziers = SEEDS.filter(|&s| generate(s).lights.iter().any(|l| l.kind == LightKind::Brazier)).count();
        assert!(braziers > 48, "only {braziers} of 64 levels have a brazier");
    }
}
