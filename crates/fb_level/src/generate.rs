//! The seeded layout: rooms, the corridors and doors that join them, the entrance, and then
//! the items and lights laid through the rooms.
//!
//! Five streams come from each level seed: one lays out the rock, one furnishes it with items
//! and lights, one places the folk ([`crate::folk`]), one the props ([`crate::props`]) and one
//! the creatures ([`crate::creatures`]). Changing the loot tables or light counts never moves a
//! wall, changing the folk never moves an item, and changing the props or the creatures never
//! moves anything else.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use armory::{Family, Item};
use procgen_rng::Mulberry32;

use crate::creatures::{CREATURE_SALT, place_creatures};
use crate::claims::{CLAIM_SALT, grow_claims};
use crate::folk::{FOLK_SALT, place_folk};
use crate::props::{PROP_SALT, place_props};
use crate::roles::{ROLE_SALT, assign_roles, light_roles};
use crate::stairs::{STAIR_SALT, place_stairs};
use crate::{Cell, Entrance, Facing, ItemSpot, Level, LevelParams, LevelStyle, LightKind, LightSpot, Room};

// The layout's sizes, counts, costs and chances are [`LevelParams`], so the level view can
// move them; its defaults are the values these constants always had.
/// Salt for the furnishing stream.
const FURNISH_SALT: u32 = 0x5EED_17E5;
/// Salt for the stream that draws a level's main corridor width, so the layout stream is the
/// same whatever width is drawn.
const WIDTH_SALT: u32 = 0x31D7_4A11;

/// What the abyss leaves lying about, by weight. Families not yet ported are skipped. No
/// chests: every container stands as a prop (`crate::props`).
const LOOT: &[(Family, u32)] = &[
    (Family::Food, 12),
    (Family::Torch, 8),
    (Family::Lantern, 2),
    (Family::Bags, 6),
    (Family::Sundry, 6),
    (Family::Writing, 4),
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

/// The level for `seed` at `depth` (1 is the first level): rooms joined by corridors one cell
/// wide, an entrance door on the outer edge, items in every room but the first, and a few
/// fixed lights. Most of it is dark; the light the player carries is the main light. The depth
/// picks the [`LevelStyle`] and with it the style's own parameters
/// ([`LevelParams::for_style`]); the first level's seeds lay out exactly as they always have.
///
/// `seed` is also taken as the world seed whose pool of factions the level draws from, so
/// `generate(seed, 1)` is the first level of the delve `seed`; for the level at any depth of a
/// delve use [`generate_in`].
pub fn generate(seed: u32, depth: u32) -> Level {
    generate_with(seed, depth, &LevelParams::for_style(LevelStyle::for_depth(depth)))
}

/// The level at `depth` of the delve for `world_seed`: a pure function of the two. Its seed is
/// [`crate::depth_seed`] of them, and its factions are drawn from the world's pool
/// (`fb_faction::World`), so a faction on several levels is the same one on each.
pub fn generate_in(world_seed: u32, depth: u32, params: &LevelParams) -> Level {
    generate_for(world_seed, crate::depth_seed(world_seed, depth), depth, params)
}

/// The level for `seed` at `depth` laid out with `params` instead of the style's defaults, for
/// the level view. With the defaults it is [`generate`]'s level. Parameters out of range or
/// out of order are fitted first ([`LevelParams::fit`]).
pub fn generate_with(seed: u32, depth: u32, params: &LevelParams) -> Level {
    generate_for(seed, seed, depth, params)
}

/// The level for level seed `seed` at `depth`, its factions drawn from the pool of `world_seed`.
pub fn generate_for(world_seed: u32, seed: u32, depth: u32, params: &LevelParams) -> Level {
    let mut params = *params;
    params.fit();
    let (mut level, layout_seed) = generate_without_creatures_in(world_seed, seed, depth, &params);
    level.creatures = place_creatures(&level, &mut Mulberry32::new(layout_seed ^ CREATURE_SALT));
    place_stairs(&mut level, &mut Mulberry32::new(layout_seed ^ STAIR_SALT));
    // The newer species come last, after the stairs, so none of them can shift a stair.
    let (creatures, webs) = crate::wild::place_wild(&mut level);
    level.creatures.extend(creatures);
    level.webs = webs;
    level
}

/// The level for `seed` with its props but before its creatures, and the layout stream's seed.
#[cfg(test)]
pub(crate) fn generate_without_creatures(seed: u32, depth: u32, params: &LevelParams) -> (Level, u32) {
    generate_without_creatures_in(seed, seed, depth, params)
}

fn generate_without_creatures_in(world_seed: u32, seed: u32, depth: u32, params: &LevelParams) -> (Level, u32) {
    let (mut level, layout_seed) = generate_without_props_in(world_seed, seed, depth, params);
    level.props = place_props(&level, &mut Mulberry32::new(layout_seed ^ PROP_SALT));
    level.banners = crate::banners::place_banners(&level);
    (level, layout_seed)
}

/// The level for `seed` before its props are placed, and the layout stream's seed. Parameters
/// that rarely lay out (many rooms, wide gaps, a small grid) are relaxed after every
/// [`RELAX_EVERY`] failed tries, a room fewer at least and the entrance a cell deeper at most,
/// so every level lays out in the end. No default level needs that many tries.
#[cfg(test)]
pub(crate) fn generate_without_props(seed: u32, depth: u32, params: &LevelParams) -> (Level, u32) {
    generate_without_props_in(seed, seed, depth, params)
}

fn generate_without_props_in(world_seed: u32, seed: u32, depth: u32, params: &LevelParams) -> (Level, u32) {
    (0u32..)
        .find_map(|attempt| {
            let relax = attempt / RELAX_EVERY;
            let p = LevelParams { rooms_min: params.rooms_min.saturating_sub(relax).max(1), entrance_depth: params.entrance_depth + relax, ..*params };
            try_generate(world_seed, seed, depth, attempt, &p)
        })
        .expect("some attempt lays out a connected level")
}

/// Failed tries before a level's parameters are relaxed a step.
const RELAX_EVERY: u32 = 100;

/// How many tries the level for `seed` takes with `params`, for tests.
#[cfg(test)]
fn tries(seed: u32, depth: u32, params: &LevelParams) -> u32 {
    (0u32..).find(|&attempt| try_generate(seed, seed, depth, attempt, params).is_some()).unwrap_or(u32::MAX) + 1
}

/// One try at a level, without its props, and the seed of its layout stream. `None` if too
/// few rooms fit or the corridors could not join them all; [`generate`] then tries again from
/// a stream derived from the same seed.
fn try_generate(world_seed: u32, seed: u32, depth: u32, attempt: u32, p: &LevelParams) -> Option<(Level, u32)> {
    let layout_seed = seed ^ attempt.wrapping_mul(0x9E37_79B9);
    let mut rng = Mulberry32::new(layout_seed);

    let rooms = place_rooms(&mut rng, p);
    if rooms.len() < p.rooms_min as usize {
        return None;
    }
    let mut grid = Grid::new(p.width, p.height);
    for room in &rooms {
        grid.fill(room, Cell::Floor);
    }
    let mut doors = vec![Vec::new(); rooms.len()];
    let entrance = place_entrance(&mut grid, &rooms, &mut doors, &mut rng, p)?;
    let start = start_room_of(&rooms, &entrance)?;
    let main_width = Mulberry32::new(layout_seed ^ WIDTH_SALT).int(p.corridor_width_min, p.corridor_width_max.max(p.corridor_width_min));
    connect(&mut grid, &rooms, &mut doors, start, main_width, &mut rng, p);

    let style = LevelStyle::for_depth(depth);
    let mut level = Level {
        seed,
        depth,
        style,
        width: p.width,
        height: p.height,
        cells: grid.cells,
        ceiling: p.ceiling,
        entrance,
        rooms,
        items: Vec::new(),
        lights: Vec::new(),
        folk: Vec::new(),
        factions: Vec::new(),
        claims: Vec::new(),
        roles: Vec::new(),
        props: Vec::new(),
        banners: Vec::new(),
        creatures: Vec::new(),
        stairs: Vec::new(),
        webs: Vec::new(),
        past: None,
        purposes: Vec::new(),
    };
    // The past first, so a prison can make cell blocks of its great halls; columns keep out of them.
    crate::past::settle_past(&mut level, layout_seed, p);
    if style == LevelStyle::StoneHalls {
        // A level of stone halls has a colonnade; a layout with no hall to hold one is tried
        // again, until the parameters are relaxed. Params that make no such hall (no halls, small
        // rooms) then give a level without one rather than never finishing.
        let colonnaded = colonnade_halls(&mut level);
        if colonnaded == 0 && attempt < RELAX_EVERY {
            return None;
        }
    }
    let (sx, sy, _) = level.start();
    if !reachable_from(&level, sx as i32, sy as i32).iter().zip(&level.cells).all(|(&seen, c)| seen || !c.is_open()) {
        return None;
    }
    furnish(&mut level, &mut Mulberry32::new(layout_seed ^ FURNISH_SALT), p);
    let start_room = level.start_room().expect("the entrance leads into a room");
    level.factions = fb_faction::World::new(world_seed).draw_specs(seed);
    // A room too cramped or crowded for its claim's folk to stand in cannot start a claim; the
    // claims are grown again without it, until every claim holds a few folk at least.
    let mut cramped: Vec<usize> = Vec::new();
    let mut need = 3;
    let mut level = loop {
        let (claims, strengths) = grow_claims(&level, start_room, &cramped, &mut Mulberry32::new(layout_seed ^ CLAIM_SALT));
        let mut trial = level.clone();
        trial.claims = claims;
        // What a faction keeps is in its containers and on its traders' shelves, not on the floor.
        // Claims settle after the loot is scattered, so what lies in a claimed room is taken out
        // before folk and fires are placed, and nothing keeps clear of what is gone.
        let mut in_claim = trial.items.iter().map(|i| trial.room_at(i.x as i32, i.y as i32).is_some_and(|r| trial.is_claimed(r))).collect::<Vec<_>>().into_iter();
        trial.items.retain(|_| !in_claim.next().unwrap_or(false));
        // A faction that found no ground is not on the level.
        let held: Vec<_> = trial.claims.iter().map(|c| c.faction).collect();
        trial.factions.retain(|s| held.contains(&s.id));
        let plans = crate::trades::plan(&trial, &strengths, layout_seed);
        trial.roles = assign_roles(&trial, &plans);
        light_roles(&mut trial, &mut Mulberry32::new(layout_seed ^ ROLE_SALT));
        trial.folk = place_folk(&trial, &strengths, &plans, &mut Mulberry32::new(layout_seed ^ FOLK_SALT));
        let empty: Vec<usize> = trial.claims.iter().zip(&strengths).filter(|(c, want)| trial.folk.iter().filter(|f| f.faction == c.faction).count() < (**want).min(need)).map(|(c, _)| c.rooms[0]).collect();
        // A level with no ground for a claim of three settles for claims of one: when every claim
        // of this round came up short, or none was left to grow.
        let all_short = empty.len() == trial.claims.len();
        if empty.is_empty() && !(trial.claims.is_empty() && need > 1) {
            break trial;
        }
        cramped.extend(empty);
        if all_short && need > 1 {
            need = 1;
            cramped.clear();
        }
    };
    crate::trades::assign(&mut level, world_seed);
    Some((level, layout_seed))
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

/// Rooms of varied size scattered over the grid, kept `room_gap` apart.
fn place_rooms(rng: &mut Mulberry32, p: &LevelParams) -> Vec<Room> {
    let target = rng.int(p.rooms_min, p.rooms_max) as usize;
    let mut rooms: Vec<Room> = Vec::with_capacity(target);
    for _ in 0..p.placement_tries {
        if rooms.len() == target {
            break;
        }
        let room = roll_room(rng, p);
        if rooms.iter().all(|other| apart(&room, other, p.room_gap)) {
            rooms.push(room);
        }
    }
    rooms
}

/// One room: mostly small chambers, now and then a hall.
fn roll_room(rng: &mut Mulberry32, p: &LevelParams) -> Room {
    let grow = if rng.chance(p.hall_chance) { rng.int(p.hall_grow_min, p.hall_grow_max) } else { 0 };
    let mut w = rng.int(p.side_min, p.side_max) + grow;
    let mut h = rng.int(p.side_min, p.side_max) + grow;
    // Only drawn from when asked for, so the first level's stream is untouched.
    if p.regular_chance > 0.0 && rng.chance(p.regular_chance) {
        (w, h) = regular_sides(w, h, p);
    }
    let x = rng.int(p.room_gap, p.width - p.room_gap - w);
    let y = rng.int(p.room_gap, p.height - p.room_gap - h);
    Room { x, y, w, h }
}

/// A room's sides made regular: both even, within what fits the grid, and neither more than
/// twice the other, so it reads as a squared hall rather than a slot.
fn regular_sides(w: u32, h: u32, p: &LevelParams) -> (u32, u32) {
    let even = |n: u32| n & !1;
    let (max_w, max_h) = (even(p.width - 2 * p.room_gap), even(p.height - 2 * p.room_gap));
    let least = even(p.side_min + 1);
    let (mut w, mut h) = (even(w).max(least).min(max_w), even(h).max(least).min(max_h));
    if w > 2 * h {
        w = even(2 * h);
    }
    if h > 2 * w {
        h = even(2 * w);
    }
    (w, h)
}

/// Whether two rooms keep at least `gap` rock cells between them on some axis.
fn apart(a: &Room, b: &Room, gap: u32) -> bool {
    a.x + a.w + gap <= b.x || b.x + b.w + gap <= a.x || a.y + a.h + gap <= b.y || b.y + b.h + gap <= a.y
}

/// Rock cells between `room` and the grid edge on `side`.
fn distance_to_edge(room: &Room, side: Facing, p: &LevelParams) -> u32 {
    match side {
        Facing::North => room.y,
        Facing::South => p.height - (room.y + room.h),
        Facing::West => room.x,
        Facing::East => p.width - (room.x + room.w),
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
/// room lies deeper than `entrance_depth`.
fn place_entrance(grid: &mut Grid, rooms: &[Room], doors: &mut [Vec<Door>], rng: &mut Mulberry32, p: &LevelParams) -> Option<Entrance> {
    let first_side = rng.int(0, 3) as usize;
    let mut nearest: Option<(u32, Facing, usize)> = None;
    for k in 0..4 {
        let side = Facing::ALL[(first_side + k) % 4];
        for (i, room) in rooms.iter().enumerate() {
            let d = distance_to_edge(room, side, p);
            if nearest.is_none_or(|(best, _, _)| d < best) {
                nearest = Some((d, side, i));
            }
        }
    }
    let (depth, side, i) = nearest?;
    if depth > p.entrance_depth {
        return None;
    }
    let room = &rooms[i];

    let (lo, hi) = span(room, side);
    let along = rng.int(lo, hi - 1) as i32;
    let (mut x, mut y) = match side {
        Facing::North => (along, 0),
        Facing::South => (along, p.height as i32 - 1),
        Facing::West => (0, along),
        Facing::East => (p.width as i32 - 1, along),
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

/// The room the entrance passage leads into.
fn start_room_of(rooms: &[Room], entrance: &Entrance) -> Option<usize> {
    let (dx, dy) = entrance.inward.step();
    let (mut x, mut y) = (entrance.x as i32, entrance.y as i32);
    for _ in 0..1000 {
        if let Some(i) = rooms.iter().position(|r| r.contains(x, y)) {
            return Some(i);
        }
        x += dx;
        y += dy;
    }
    None
}

/// Joins the rooms with corridors: a spanning tree over the rooms, so all are linked, plus a
/// few loops. A link that cannot be routed is skipped; the caller checks reachability.
/// Each link is carved at its rank's width: the spine (the tree path from `start` to the
/// room furthest from it by tree links) at `main_width`, other tree links one narrower, loops
/// one cell.
fn connect(grid: &mut Grid, rooms: &[Room], doors: &mut [Vec<Door>], start: usize, main_width: u32, rng: &mut Mulberry32, p: &LevelParams) {
    let blocked = room_halos(grid, rooms);
    let (edges, tree) = links(rooms, rng, p.loop_chance);
    let spine = spine_links(rooms.len(), &edges[..tree], start);
    for (i, &(a, b)) in edges.iter().enumerate() {
        let width = if i >= tree {
            1
        } else if spine[i] {
            main_width
        } else {
            main_width.saturating_sub(1).max(1)
        };
        join(grid, &blocked, rooms, doors, (a, b), width, rng, p);
    }
}

/// Which of the tree `edges` lie on the path from `start` to the room furthest from it.
fn spine_links(rooms: usize, edges: &[(usize, usize)], start: usize) -> Vec<bool> {
    let mut hops = vec![usize::MAX; rooms];
    let mut via = vec![usize::MAX; rooms];
    let mut order = vec![start];
    hops[start] = 0;
    let mut head = 0;
    while head < order.len() {
        let room = order[head];
        head += 1;
        for (i, &(a, b)) in edges.iter().enumerate() {
            let other = if a == room { b } else if b == room { a } else { continue };
            if hops[other] == usize::MAX {
                hops[other] = hops[room] + 1;
                via[other] = i;
                order.push(other);
            }
        }
    }
    let mut spine = vec![false; edges.len()];
    let mut room = (0..rooms).filter(|&r| hops[r] != usize::MAX).max_by_key(|&r| (hops[r], std::cmp::Reverse(r))).unwrap_or(start);
    while room != start {
        let i = via[room];
        spine[i] = true;
        let (a, b) = edges[i];
        room = if a == room { b } else { a };
    }
    spine
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
/// Returns the links and how many of them, the first ones, form the tree.
fn links(rooms: &[Room], rng: &mut Mulberry32, loop_chance: f64) -> (Vec<(usize, usize)>, usize) {
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
    let tree = edges.len();
    for a in 0..rooms.len() {
        if !rng.chance(loop_chance) {
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
    (edges, tree)
}

/// Cuts a door in each of two rooms and routes a corridor between them. Leaves the grid
/// untouched and returns `false` if no route exists.
fn join(grid: &mut Grid, blocked: &[bool], rooms: &[Room], doors: &mut [Vec<Door>], (a, b): (usize, usize), width: u32, rng: &mut Mulberry32, p: &LevelParams) -> bool {
    let from = pick_door(rooms, &doors[a], a, b, rng, p.door_reuse_chance);
    let to = pick_door(rooms, &doors[b], b, a, rng, p.door_reuse_chance);
    let Some(path) = route(grid, blocked, from.outside(), from.out, to.outside(), p) else {
        return false;
    };
    for (room, door) in [(a, from), (b, to)] {
        grid.set(door.x, door.y, Cell::Door);
        if !doors[room].contains(&door) {
            doors[room].push(door);
        }
    }
    for &(x, y) in &path {
        if grid.get(x, y) == Cell::Rock {
            grid.set(x, y, Cell::Floor);
        }
    }
    if width > 1 {
        widen(grid, blocked, &path, width);
    }
    true
}

/// Opens the cells beside a carved `path` so the corridor is `width` cells across: one side
/// of the line for two, both for three. At a turn both the incoming and outgoing direction
/// widen, so corners fill. A side cell opens only if it is rock, in bounds and unblocked, and
/// the cell beyond it is not open corridor of another run, which keeps a wall of rock between
/// parallel corridors.
fn widen(grid: &mut Grid, blocked: &[bool], path: &[(i32, i32)], width: u32) {
    let mut mine: std::collections::HashSet<(i32, i32)> = path.iter().copied().collect();
    let mut sides = Vec::new();
    for (i, &(x, y)) in path.iter().enumerate() {
        let mut dirs = Vec::with_capacity(2);
        if i > 0 {
            dirs.push((x - path[i - 1].0, y - path[i - 1].1));
        }
        if i + 1 < path.len() {
            dirs.push((path[i + 1].0 - x, path[i + 1].1 - y));
        }
        for (dx, dy) in dirs {
            let (px, py) = (-dy, dx);
            sides.push(((x, y), (px, py)));
            if width > 2 {
                sides.push(((x, y), (-px, -py)));
            }
        }
    }
    for ((x, y), (px, py)) in sides {
        let (sx, sy) = (x + px, y + py);
        if !grid.in_bounds(sx, sy) || blocked[grid.index(sx, sy)] || grid.get(sx, sy) != Cell::Rock {
            continue;
        }
        let (bx, by) = (sx + px, sy + py);
        if grid.get(bx, by).is_open() && !mine.contains(&(bx, by)) {
            continue;
        }
        grid.set(sx, sy, Cell::Floor);
        mine.insert((sx, sy));
    }
}

/// The door in room `a` for a corridor toward room `b`: on the side facing `b`, lined up with
/// `b` where the two overlap, never right beside another door. Sometimes reuses a door already
/// on that side.
fn pick_door(rooms: &[Room], doors: &[Door], a: usize, b: usize, rng: &mut Mulberry32, reuse_chance: f64) -> Door {
    let (room, other) = (&rooms[a], &rooms[b]);
    let side = facing_toward(room.centre(), other.centre());
    let existing: Vec<Door> = doors.iter().copied().filter(|d| d.out == side).collect();
    if !existing.is_empty() && rng.chance(reuse_chance) {
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
fn route(grid: &Grid, blocked: &[bool], from: (i32, i32), heading: Facing, to: (i32, i32), p: &LevelParams) -> Option<Vec<(i32, i32)>> {
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
            let turn = if f == s % 4 { 0 } else { p.turn_cost };
            let beside = if !open && grid.beside_open(nx, ny) { p.beside_cost } else { 0 };
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
// Colonnades
// ---------------------------------------------------------------------------------------------

/// The longest side a colonnaded hall needs, and the shortest: two rows of columns with an aisle
/// down the middle and one along each wall.
const COLONNADE_LONG: u32 = 7;
const COLONNADE_SHORT: u32 = 5;

/// Sets two rows of columns down each hall long and wide enough, apart from the starting room and
/// any cell block.
/// The rows run one cell in from the long walls and every other cell along them, so a ring of
/// open floor stays round the walls, the gaps between columns join it to the aisle, and nobody
/// is walled off. The two cells straight in from each door are kept clear. Returns how many
/// halls got columns. The columns are a pure function of the rooms, so they draw on no stream.
fn colonnade_halls(level: &mut Level) -> usize {
    let start_room = level.start_room();
    let mut columns = Vec::new();
    for (i, room) in level.rooms.iter().enumerate() {
        let (long, short) = (room.w.max(room.h), room.w.min(room.h));
        if Some(i) == start_room || long < COLONNADE_LONG || short < COLONNADE_SHORT || level.purpose_of(i) == Some(crate::Purpose::CellBlock) {
            continue;
        }
        let along_x = room.w >= room.h;
        let (x0, y0, len, wide) = if along_x { (room.x, room.y, room.w, room.h) } else { (room.y, room.x, room.h, room.w) };
        // Every other cell along the long side, one cell off each end for an odd length and two
        // off the near end for an even one.
        let first = x0 + 1 + (len + 1) % 2;
        for row in [y0 + 1, y0 + wide - 2] {
            for along in (first..=x0 + len - 2).step_by(2) {
                let (cx, cy) = if along_x { (along, row) } else { (row, along) };
                if !straight_in_from_a_door(level, cx as i32, cy as i32) {
                    columns.push((cx, cy));
                }
            }
        }
    }
    let mut halls = std::collections::HashSet::new();
    for &(x, y) in &columns {
        level.cells[(y * level.width + x) as usize] = Cell::Column;
        halls.insert(level.room_at(x as i32, y as i32));
    }
    halls.len()
}

/// Whether `(x, y)` is one of the two cells straight in from a door.
pub(crate) fn straight_in_from_a_door(level: &Level, x: i32, y: i32) -> bool {
    Facing::ALL.iter().any(|f| {
        let (dx, dy) = f.step();
        (1..=2).any(|k| level.cell(x - dx * k, y - dy * k).is_doorway())
    })
}

// ---------------------------------------------------------------------------------------------
// Items and lights
// ---------------------------------------------------------------------------------------------

/// Lays the start light, the fixed lights, and the loot.
fn furnish(level: &mut Level, rng: &mut Mulberry32, p: &LevelParams) {
    let start_room = level.start_room().expect("the entrance leads into a room");
    let start_light = start_light(level, rng);
    level.items.push(start_light);
    let mut lights = place_lights(level, start_room, rng, p.brazier_min_area);
    // A glowcap at a wall's foot may fall on a cell block's bars, and a brazier at a room's middle
    // in front of a cell's gate; both go.
    lights.retain(|l| {
        let (x, y) = (l.x.floor() as i32, l.y.floor() as i32);
        level.cell(x, y) == Cell::Floor && !(l.kind == LightKind::Brazier && crate::folk::by_door(level, x, y))
    });
    // Braziers by thrones and altars, where no light already stands close.
    for light in crate::past::purpose_lights(level) {
        if lights.iter().all(|l| (l.x - light.x).hypot(l.y - light.y) > 1.5) {
            lights.push(light);
        }
    }
    level.lights = lights;
    let loot = scatter_items(level, start_room, rng, p);
    level.items.extend(loot);
    let books = library_books(level, rng, p);
    level.items.extend(books);
}

/// Books and scrolls lying about each library: two to four, clear of the loot and the lights.
fn library_books(level: &Level, rng: &mut Mulberry32, p: &LevelParams) -> Vec<ItemSpot> {
    use armory::families::Generator;
    use armory::families::writing::{Writing, WritingKind};
    let mut taken: Vec<(f32, f32)> = level.lights.iter().map(|l| (l.x, l.y)).chain(level.items.iter().map(|i| (i.x, i.y))).collect();
    let mut books = Vec::new();
    for r in level.rooms_for(crate::Purpose::Library).collect::<Vec<_>>() {
        for _ in 0..rng.int(2, 4) {
            let Some((x, y)) = inner_spot(level, &level.rooms[r], &taken, rng, p.spot_spacing) else { continue };
            // The first seed of a few that rolls a book or a scroll, not a note.
            let seeds: Vec<u32> = (0..8).map(|_| item_seed(rng)).collect();
            let seed = seeds.iter().copied().find(|&s| Writing::roll(s).piece.kind != WritingKind::Note).unwrap_or(seeds[0]);
            taken.push((x, y));
            books.push(ItemSpot { x, y, yaw: rng.range_f32(0.0, std::f32::consts::TAU), family: Family::Writing, seed });
        }
    }
    books
}

/// A full-range u32 seed for an item.
pub(crate) fn item_seed(rng: &mut Mulberry32) -> u32 {
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
fn place_lights(level: &Level, start_room: usize, rng: &mut Mulberry32, brazier_min_area: u32) -> Vec<LightSpot> {
    if level.style == LevelStyle::StoneHalls {
        return hall_lights(level, start_room, rng, brazier_min_area);
    }
    let mut lights = Vec::new();
    let mut by_size: Vec<usize> = (0..level.rooms.len()).filter(|&i| i != start_room).collect();
    by_size.sort_by_key(|&i| Reverse(level.rooms[i].area()));
    let halls: Vec<usize> = by_size.iter().copied().filter(|&i| level.rooms[i].area() >= brazier_min_area).collect();
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

/// Whether `room` stands with columns, a colonnaded hall.
pub(crate) fn is_colonnaded(level: &Level, room: &Room) -> bool {
    (room.y..room.y + room.h).any(|y| (room.x..room.x + room.w).any(|x| level.cell(x as i32, y as i32) == Cell::Column))
}

/// The lights of a level of stone halls: braziers between the columns of up to three colonnaded
/// halls, one more in a plain hall now and then, and glowcaps in only a room or two besides.
fn hall_lights(level: &Level, start_room: usize, rng: &mut Mulberry32, brazier_min_area: u32) -> Vec<LightSpot> {
    let mut lights = Vec::new();
    let mut by_size: Vec<usize> = (0..level.rooms.len()).filter(|&i| i != start_room).collect();
    by_size.sort_by_key(|&i| Reverse(level.rooms[i].area()));
    let mut lit: Vec<usize> = by_size.iter().copied().filter(|&i| is_colonnaded(level, &level.rooms[i])).take(level.rooms.len().div_ceil(2).saturating_sub(1).clamp(1, 3)).collect();
    for &i in &lit {
        lights.extend(colonnade_braziers(level, &level.rooms[i], rng));
    }
    if let Some(&i) = by_size.iter().find(|&&i| !lit.contains(&i) && !is_colonnaded(level, &level.rooms[i]) && level.rooms[i].area() >= brazier_min_area)
        && (lit.len() + 1) * 2 < level.rooms.len() && rng.chance(0.5) {
            lights.extend(braziers(&level.rooms[i], rng));
            lit.push(i);
        }
    let mut others: Vec<usize> = by_size.iter().copied().filter(|i| !lit.contains(i)).collect();
    shuffle(&mut others, rng);
    let room_budget = level.rooms.len().div_ceil(2).saturating_sub(lit.len()).max(1);
    let glow_rooms = (rng.int(1, 2) as usize).min(room_budget).min(others.len());
    for &i in &others[..glow_rooms] {
        lights.extend(glowcaps(&level.rooms[i], rng));
    }
    lights
}

/// Two or three braziers down the aisle of a colonnaded hall, each opposite a gap between two
/// columns and evenly spread. A hall with no such gap gets [`braziers`].
fn colonnade_braziers(level: &Level, room: &Room, rng: &mut Mulberry32) -> Vec<LightSpot> {
    let along_x = room.w >= room.h;
    let (x0, y0, len, wide) = if along_x { (room.x, room.y, room.w, room.h) } else { (room.y, room.x, room.h, room.w) };
    let row = y0 + wide / 2;
    let column = |a: u32, r: u32| {
        let (cx, cy) = if along_x { (a, r) } else { (r, a) };
        level.cell(cx as i32, cy as i32) == Cell::Column
    };
    let gaps: Vec<u32> = (x0 + 1..x0 + len - 1)
        .filter(|&a| [y0 + 1, y0 + wide - 2].into_iter().any(|r| column(a - 1, r) && column(a + 1, r) && !column(a, r)))
        .collect();
    if gaps.is_empty() {
        return braziers(room, rng);
    }
    let count = (rng.int(2, 3) as usize).min(gaps.len());
    (0..count)
        .map(|k| {
            let a = gaps[(2 * k + 1) * gaps.len() / (2 * count)] as f32 + 0.5;
            let r = row as f32 + 0.5;
            let (x, y) = if along_x { (a, r) } else { (r, a) };
            LightSpot { x, y, kind: LightKind::Brazier }
        })
        .collect()
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
fn scatter_items(level: &Level, start_room: usize, rng: &mut Mulberry32, p: &LevelParams) -> Vec<ItemSpot> {
    let table: Vec<(Family, u32)> = LOOT.iter().copied().filter(|(f, _)| Item::PORTED.contains(f)).collect();
    let mut taken: Vec<(f32, f32)> = level.lights.iter().map(|l| (l.x, l.y)).chain(level.items.iter().map(|i| (i.x, i.y))).collect();
    // Nothing lies where a throne or an altar will stand.
    taken.extend(seats(level));
    let mut items = Vec::new();
    for (i, room) in level.rooms.iter().enumerate() {
        if i == start_room || rng.chance(p.bare_room_chance) {
            continue;
        }
        let count = (room.area() / p.cells_per_item).clamp(1, p.items_per_room_max) + rng.int(0, 1);
        // A room put to a purpose keeps its walls clear for its furniture.
        let inner = level.purpose_of(i).is_some();
        for _ in 0..count {
            let Some((x, y)) = (if inner { inner_spot(level, room, &taken, rng, p.spot_spacing) } else { free_spot(level, room, &taken, rng, p.spot_spacing) }) else { continue };
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

/// Where a throne or an altar will stand: the middle of each throne room's and shrine's seat.
fn seats(level: &Level) -> Vec<(f32, f32)> {
    (0..level.rooms.len())
        .filter(|&r| matches!(level.purpose_of(r), Some(crate::Purpose::ThroneRoom | crate::Purpose::Shrine)))
        .filter_map(|r| crate::past::seat(level, r))
        .map(|((x, y), _)| (x as f32 + 0.5, y as f32 + 0.5))
        .collect()
}

/// A [`free_spot`] in a cell with no wall beside it, so furniture can stand along the walls; a
/// few tries, then any free spot.
fn inner_spot(level: &Level, room: &Room, taken: &[(f32, f32)], rng: &mut Mulberry32, spacing: f32) -> Option<(f32, f32)> {
    if room.w < 3 || room.h < 3 {
        return free_spot(level, room, taken, rng, spacing);
    }
    (0..4).find_map(|_| {
        let (x, y) = free_spot(level, room, taken, rng, spacing)?;
        let (cx, cy) = (x.floor() as i32, y.floor() as i32);
        Facing::ALL.iter().all(|f| level.is_open(cx + f.step().0, cy + f.step().1)).then_some((x, y))
    })
    .or_else(|| free_spot(level, room, taken, rng, spacing))
}

/// A point in `room`, clear of doorways and at least `spacing` from every taken spot.
fn free_spot(level: &Level, room: &Room, taken: &[(f32, f32)], rng: &mut Mulberry32, spacing: f32) -> Option<(f32, f32)> {
    (0..24).find_map(|_| {
        let cx = rng.int(room.x, room.x + room.w - 1) as i32;
        let cy = rng.int(room.y, room.y + room.h - 1) as i32;
        let x = cx as f32 + rng.range_f32(0.25, 0.75);
        let y = cy as f32 + rng.range_f32(0.25, 0.75);
        if !level.is_open(cx, cy) || level.cell(cx, cy) == Cell::Gate {
            return None;
        }
        let by_door = Facing::ALL.iter().any(|f| {
            let (dx, dy) = f.step();
            level.cell(cx + dx, cy + dy).is_doorway()
        });
        let crowded = taken.iter().any(|&(tx, ty)| (tx - x).hypot(ty - y) < spacing);
        (!by_door && !crowded).then_some((x, y))
    })
}

/// Fisher-Yates, from the level's stream.
pub(crate) fn shuffle<T>(items: &mut [T], rng: &mut Mulberry32) {
    for i in (1..items.len()).rev() {
        let j = rng.int(0, i as u32) as usize;
        items.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    use crate::{Facing, StairKind, depth_seed};

    const SEEDS: std::ops::RangeInclusive<u32> = 1..=64;

    /// Every seed on the first level and on the second, which a placement test holds for alike.
    fn sweep() -> impl Iterator<Item = (u32, u32)> {
        [1, 2].into_iter().flat_map(|depth| SEEDS.map(move |seed| (seed, depth)))
    }

    fn cell_of(level: &Level, x: f32, y: f32) -> Cell {
        level.cell(x.floor() as i32, y.floor() as i32)
    }

    #[test]
    fn the_defaults_lay_out_every_level_as_before() {
        let defaults = LevelParams::default();
        for seed in SEEDS {
            assert_eq!(generate(seed, 1), generate_with(seed, 1, &defaults), "seed {seed}");
        }
        // No default level is ever relaxed, so relaxing changes no existing seed.
        let most = (0..2000).map(|seed| tries(seed, 1, &defaults)).max().unwrap();
        assert!(most < RELAX_EVERY, "a default level took {most} tries");
    }

    /// Whether `level` is whole: an entrance on the edge leading into a room, and every open
    /// cell reachable from it.
    fn is_whole(level: &Level) -> bool {
        let (x, y, _) = level.start();
        let seen = reachable_from(level, x as i32, y as i32);
        level.start_room().is_some() && level.cells.iter().zip(&seen).all(|(c, &s)| s || !c.is_open())
    }

    #[test]
    fn every_parameter_at_either_end_still_lays_out_a_whole_level() {
        for &(_, name, range) in LevelParams::SLIDERS {
            for value in [range.min, range.max] {
                let mut params = LevelParams::default();
                params.set_slider(name, value).unwrap();
                for seed in [1, 1207] {
                    let level = generate_with(seed, 1, &params);
                    assert!(is_whole(&level), "{name} = {value}, seed {seed}");
                    assert_eq!((level.width, level.height), (params.width, params.height), "{name} = {value}");
                }
            }
        }
        // All at once at their most crowded: many rooms, wide gaps, a small grid.
        let crowded = LevelParams { width: 32, height: 28, rooms_min: 20, rooms_max: 30, side_min: 6, side_max: 14, room_gap: 8, ..LevelParams::default() };
        assert!(is_whole(&generate_with(7, 1, &crowded)));
    }

    #[test]
    fn asking_for_more_rooms_gives_more_rooms() {
        let few = LevelParams { rooms_min: 3, rooms_max: 4, ..LevelParams::default() };
        let many = LevelParams { width: 80, height: 70, rooms_min: 16, rooms_max: 20, ..LevelParams::default() };
        let rooms = |p: &LevelParams| (1..=16).map(|seed| generate_with(seed, 1, p).rooms.len()).sum::<usize>();
        assert!(rooms(&many) > 3 * rooms(&few), "{} against {}", rooms(&many), rooms(&few));
        // A wider grid gives a wider level.
        let wide = LevelParams { width: 90, ..LevelParams::default() };
        assert_eq!(generate_with(3, 1, &wide).width, 90);
    }

    #[test]
    fn levels_are_deterministic() {
        for seed in [0, 1, 1207, u32::MAX] {
            assert_eq!(generate(seed, 1), generate(seed, 1));
        }
        assert_ne!(generate(1, 1).cells, generate(2, 1).cells);
    }

    #[test]
    fn every_open_cell_is_reachable_from_the_start() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
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
            let n = generate(seed, 1).rooms.len() as u32;
            let p = LevelParams::default();
            assert!((p.rooms_min..=p.rooms_max).contains(&n), "seed {seed}: {n} rooms");
        }
    }

    #[test]
    fn rooms_vary_in_size() {
        let sizes: HashSet<(u32, u32)> = SEEDS.flat_map(|s| generate(s, 1).rooms).map(|r| (r.w, r.h)).collect();
        assert!(sizes.len() > 20, "only {} room sizes", sizes.len());
    }

    #[test]
    fn the_entrance_is_a_door_on_the_edge_opening_inward() {
        for seed in SEEDS {
            let level = generate(seed, 1);
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
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
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

    /// The widest open square outside every room, in cells (up to four).
    fn widest_corridor_square(level: &Level) -> u32 {
        let corridor = |x: i32, y: i32| level.is_open(x, y) && level.room_at(x, y).is_none();
        let mut widest = 0;
        for y in 0..level.height as i32 {
            for x in 0..level.width as i32 {
                let mut n = 0;
                while n < 4 && (0..=n).all(|k| (0..=n).all(|j| corridor(x + k, y + j))) {
                    n += 1;
                }
                widest = widest.max(n as u32);
            }
        }
        widest
    }

    #[test]
    fn corridors_are_no_wider_than_their_width() {
        for width in 1..=3 {
            let params = LevelParams { corridor_width_min: width, corridor_width_max: width, ..LevelParams::default() };
            for seed in SEEDS {
                let level = generate_with(seed, 1, &params);
                let widest = widest_corridor_square(&level);
                assert!(widest <= width, "seed {seed}, width {width}: a {widest} by {widest} open block outside the rooms");
            }
        }
    }

    #[test]
    fn corridor_width_max_one_lays_out_only_one_cell_corridors() {
        let params = LevelParams { corridor_width_min: 1, corridor_width_max: 1, ..LevelParams::default() };
        for seed in SEEDS {
            assert_eq!(widest_corridor_square(&generate_with(seed, 1, &params)), 1, "seed {seed}");
        }
    }

    #[test]
    fn wide_corridors_stay_whole_and_repeat() {
        let params = LevelParams { corridor_width_min: 3, corridor_width_max: 3, ..LevelParams::default() };
        for seed in SEEDS {
            let level = generate_with(seed, 1, &params);
            assert!(is_whole(&level), "seed {seed}");
            assert_eq!(level.cells, generate_with(seed, 1, &params).cells);
        }
    }

    #[test]
    fn stone_halls_mostly_have_a_wide_corridor() {
        let wide = SEEDS.filter(|&s| widest_corridor_square(&generate(s, 2)) >= 2).count();
        assert!(wide * 4 >= SEEDS.count() * 3, "only {wide} of {} levels have a corridor two cells wide", SEEDS.count());
    }

    #[test]
    fn no_spot_lies_in_rock() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let spots = level.items.iter().map(|i| (i.x, i.y)).chain(level.lights.iter().map(|l| (l.x, l.y)));
            for (x, y) in spots {
                assert_eq!(cell_of(&level, x, y), Cell::Floor, "seed {seed}: spot ({x}, {y}) is not on floor");
                assert!(level.room_at(x.floor() as i32, y.floor() as i32).is_some(), "seed {seed}: spot ({x}, {y}) is outside a room");
            }
        }
    }

    #[test]
    fn the_starting_room_holds_only_a_light_near_the_start() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let start_room = level.start_room().unwrap();
            let inside: Vec<&ItemSpot> =
                level.items.iter().filter(|i| level.room_at(i.x.floor() as i32, i.y.floor() as i32) == Some(start_room)).collect();
            assert_eq!(inside.len(), 1, "seed {seed}: {} items in the starting room", inside.len());
            assert!(matches!(inside[0].family, Family::Torch | Family::Lantern), "seed {seed}");
            let (sx, sy, _) = level.start();
            let d = (inside[0].x - (sx as f32 + 0.5)).hypot(inside[0].y - (sy as f32 + 0.5));
            assert!(d < (LevelParams::default().entrance_depth + 1) as f32, "seed {seed}: the start light is {d} cells from the start");
            assert!(level.lights.iter().all(|l| level.room_at(l.x as i32, l.y as i32) != Some(start_room)), "seed {seed}");
        }
    }

    #[test]
    fn items_cover_many_families() {
        let families: HashSet<Family> = (1..=32).flat_map(|s| generate(s, 1).items).map(|i| i.family).collect();
        assert!(families.len() >= 8, "only {} families: {families:?}", families.len());
    }

    #[test]
    fn writing_lies_in_generated_levels() {
        let found = (1..=32).flat_map(|s| generate(s, 1).items).filter(|i| i.family == Family::Writing).count();
        assert!(found > 0, "no writing in 32 first levels");
    }

    #[test]
    fn no_chest_lies_loose() {
        for seed in 1..=32 {
            assert!(generate(seed, 1).items.iter().all(|i| i.family != Family::Chests), "seed {seed}: a loose chest");
        }
    }

    #[test]
    fn levels_have_braziers_glowcaps_and_dark_rooms() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            // The level's own furnishing leaves most rooms dark; the halls and hearths of claims
            // are lit on top of it (#546), so count the rooms no one holds.
            let lit: HashSet<usize> = level.lights.iter().filter_map(|l| level.room_at(l.x as i32, l.y as i32)).filter(|&r| !level.is_claimed(r)).collect();
            assert!(level.lights.iter().any(|l| l.kind == LightKind::Glowcaps), "seed {seed}: no glowcaps");
            assert!(lit.len() * 2 <= level.rooms.len() + 1, "seed {seed}: {} of {} rooms lit", lit.len(), level.rooms.len());
        }
        for depth in [1, 2] {
            let braziers = SEEDS.filter(|&s| generate(s, depth).lights.iter().any(|l| l.kind == LightKind::Brazier)).count();
            assert!(braziers > 48, "only {braziers} of 64 levels at depth {depth} have a brazier");
        }
    }

    /// FNV-1a over the debug text of everything the first level held before styles existed.
    #[cfg(target_os = "macos")]
    fn snapshot_hash(level: &Level) -> u64 {
        let text = format!(
            "{:?}",
            (&level.seed, &level.width, &level.height, &level.cells, &level.ceiling, &level.entrance, &level.rooms, &level.items, &level.lights, &level.folk, &level.props, level.creatures.iter().filter(|c| c.species.is_rat() || c.species.is_snake()).collect::<Vec<_>>())
        );
        text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
    }

    // The hashes were taken on macOS. Linux's libm rounds `atan2` differently in the last bit,
    // so folk and prop yaws differ there, which is fine: a seed only has to repeat on its own
    // platform.
    #[test]
    #[cfg(target_os = "macos")]
    fn the_first_level_is_byte_identical_to_before_styles() {
        // Hashes taken from the generator before level styles were added. Seeds 7, 64, 4242 and
        // u32::MAX were retaken when giant rats left the first level (`GIANTS_FROM_DEPTH`): their
        // giant packs are rats now. All eight were retaken when writing joined the loot table
        // (`Family::Writing`), which moves the weighted pick of every floor item, and again when
        // folk left half the rooms to the creatures, which no longer share a room with them, and again
        // when the factions came from the world's pool (a level draws two to four, which changes the
        // shuffle that sets the groups), and again when folk moved into their factions' claims (#541), and again when claimed rooms
        // took roles: halls and hearths are lit, folk stand by role and props furnish the rooms (#546), and again when folk gained a trade (#544), and again when guards took their posts at the hall door and traders stood in the market (#550), and again when posts cleared the door leaves' real sweep, and again when hearths were lit by a campfire standing on its own light and braziers stood alone (#569), and again when a room that is hall and hearth got only the campfire (lights, props and folk move, not the layout or items), and again when the door sweep followed door kinds and the doorway's real hinge (seeds 1 and 4242: a guard's post moved), and again when corridors took widths by rank (#581: seeds 1, 7, 1207 and 4242 drew a main width of two, so their corridors and all that follows moved), and again when rough rock's main width became two to three, like the halls', so every level has a wide main way. Then the same again when levels took a past (#587: seed 64's first level is a hold with a cell block) and rooms were furnished by their purpose (#588), and again when claimed guardrooms kept their strongboxes (#617) and floor items left claimed rooms (#618). Then seed 1207 again when the wild groups were placed among the rats and each took a room of its own (#662), and seeds 1 and 4242 again when faction names were rolled before their members (#663), and all eight when folk gained a sex (#663), and seeds 1 and 4242 again when a name that says a class was rolled over with that class given (#663).
        let before = [
            (0u32, 0xf6aedcf09a11efeu64),
            (1, 0x964052c5ff7f73d8),
            (2, 0x3a098b31f7bedf36),
            (7, 0xd4799adb07d0e04a),
            (64, 0xf4d90e47ab648078),
            (1207, 0x46ee98a53fedc3a7),
            (4242, 0x13966fc2c4bc1d5f),
            (u32::MAX, 0x796683c83957bced),
        ];
        for (seed, hash) in before {
            let level = generate(seed, 1);
            assert_eq!((level.style, level.depth), (LevelStyle::RoughRock, 1));
            assert_eq!(snapshot_hash(&level), hash, "seed {seed}");
        }
    }

    #[test]
    fn depth_picks_the_style() {
        assert_eq!(LevelStyle::for_depth(1), LevelStyle::RoughRock);
        assert_eq!(LevelStyle::for_depth(2), LevelStyle::StoneHalls);
        assert_eq!(LevelStyle::for_depth(3), LevelStyle::RoughRock);
        assert_eq!(LevelStyle::for_depth(4), LevelStyle::StoneHalls);
        let level = generate(5, 2);
        assert_eq!((level.style, level.depth), (LevelStyle::StoneHalls, 2));
        assert_eq!(LevelParams::for_style(LevelStyle::RoughRock), LevelParams::default());
        let mut halls = LevelParams::for_style(LevelStyle::StoneHalls);
        let fitted = halls;
        halls.fit();
        assert_eq!(halls, fitted, "the stone halls defaults already fit");
    }

    fn columns_in(level: &Level, room: &Room) -> usize {
        let mut n = 0;
        for y in room.y..room.y + room.h {
            for x in room.x..room.x + room.w {
                n += (level.cell(x as i32, y as i32) == Cell::Column) as usize;
            }
        }
        n
    }

    #[test]
    fn everything_placed_on_either_level_is_reachable_from_the_entrance() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let (sx, sy, _) = level.start();
            let seen = reachable_from(&level, sx as i32, sy as i32);
            for (x, y) in spot_cells(&level) {
                assert!(seen[(y as u32 * level.width + x as u32) as usize], "seed {seed} depth {depth}: ({x}, {y}) is cut off");
            }
            assert!(!level.items.is_empty() && !level.lights.is_empty() && !level.folk.is_empty() && !level.creatures.is_empty(), "seed {seed} depth {depth}");
        }
    }

    #[test]
    fn no_floor_item_lies_in_a_claimed_room() {
        let (mut claimed_rooms, mut wild_items) = (0, 0);
        for depth in 1..=4 {
            for seed in 1..=40 {
                let level = generate(seed, depth);
                for item in &level.items {
                    let room = level.room_at(item.x as i32, item.y as i32);
                    assert!(!room.is_some_and(|r| level.is_claimed(r)), "seed {seed} depth {depth}: an item at {},{} in a claimed room", item.x, item.y);
                    wild_items += room.is_some() as usize;
                }
                claimed_rooms += (0..level.rooms.len()).filter(|&r| level.is_claimed(r)).count();
            }
        }
        assert!(claimed_rooms > 50, "{claimed_rooms} claimed rooms");
        assert!(wild_items > 200, "{wild_items} items in unclaimed rooms");
    }

    #[test]
    fn braziers_in_a_colonnade_stand_between_columns() {
        let (mut halls, mut lamps, mut between) = (0, 0, 0);
        for seed in 0..100 {
            let level = generate(seed, 2);
            for room in level.rooms.iter().filter(|r| is_colonnaded(&level, r)) {
                let braziers: Vec<_> = level.lights.iter().filter(|l| l.kind == LightKind::Brazier && room.contains(l.x as i32, l.y as i32)).collect();
                halls += !braziers.is_empty() as usize;
                for b in braziers {
                    let (x, y) = (b.x.floor() as i32, b.y.floor() as i32);
                    // Columns stand either side of it along the hall, in a row beside the aisle.
                    let along_x = room.w >= room.h;
                    let (dx, dy) = if along_x { (1, 0) } else { (0, 1) };
                    let column_beside = |k: i32| (-5..=5).any(|r| level.cell(x + dx * k - dy * r, y + dy * k - dx * r) == Cell::Column);
                    lamps += 1;
                    between += (column_beside(-1) && column_beside(1)) as usize;
                }
            }
        }
        assert!(halls >= 80, "only {halls} colonnaded halls are lit");
        // A hall with no clear gap between two columns gets its brazier at the centre instead.
        assert!(between * 10 >= lamps * 9, "only {between} of {lamps} braziers stand between columns");
    }

    #[test]
    fn folk_stand_in_the_colonnades_and_creatures_mostly_keep_out() {
        let (mut folk, mut folk_in, mut beasts, mut beasts_in, mut rooms, mut halls) = (0, 0, 0, 0, 0, 0);
        for seed in 0..100 {
            let level = generate(seed, 2);
            let in_hall = |x: f32, y: f32| level.room_at(x as i32, y as i32).is_some_and(|i| is_colonnaded(&level, &level.rooms[i]));
            let start_room = level.start_room();
            for (_, room) in level.rooms.iter().enumerate().filter(|(i, r)| Some(*i) != start_room && r.area() >= 9) {
                rooms += 1;
                halls += is_colonnaded(&level, room) as usize;
            }
            folk += level.folk.len();
            folk_in += level.folk.iter().filter(|f| in_hall(f.x, f.y)).count();
            // Rats and snakes keep out; the skeletons are the ones that stand guard there.
            beasts += level.creatures.iter().filter(|c| !c.species.is_skeleton()).count();
            beasts_in += level.creatures.iter().filter(|c| !c.species.is_skeleton() && in_hall(c.x, c.y)).count();
        }
        assert!(folk_in * 3 >= folk, "only {folk_in} of {folk} folk stand in a colonnade");
        // Creatures keep out of the halls more than the halls' share of the rooms they could take. The wild species, which no longer share a held room (#662), take the halls that are left, so the bound is the share itself.
        assert!(beasts_in * rooms <= beasts * halls, "{beasts_in} of {beasts} creatures stand in a colonnade, in {halls} of {rooms} rooms");
    }

    #[test]
    fn props_leave_the_gaps_between_columns_open() {
        for seed in 0..100 {
            let level = generate(seed, 2);
            for p in &level.props {
                let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
                let behind_gap = Facing::ALL.iter().any(|f| {
                    let (dx, dy) = f.step();
                    level.cell(x + dx, y + dy) == Cell::Floor && level.cell(x + dx + dy, y + dy + dx) == Cell::Column && level.cell(x + dx - dy, y + dy - dx) == Cell::Column
                });
                assert!(!behind_gap, "seed {seed}: {p:?} blocks a gap between columns");
            }
        }
    }

    #[test]
    fn stone_halls_have_a_colonnaded_hall_and_stay_whole() {
        for seed in 0..200 {
            let level = generate(seed, 2);
            assert!(level.rooms.iter().any(|r| columns_in(&level, r) >= 4), "seed {seed}: no colonnade");
            assert!(is_whole(&level), "seed {seed}: cells unreachable round the columns");
        }
    }

    #[test]
    fn columns_stand_only_inside_halls_and_clear_of_doors() {
        for seed in 0..200 {
            let level = generate(seed, 2);
            for y in 0..level.height as i32 {
                for x in 0..level.width as i32 {
                    if level.cell(x, y) != Cell::Column {
                        continue;
                    }
                    let room = level.room_at(x, y).unwrap_or_else(|| panic!("seed {seed}: column ({x}, {y}) outside any room"));
                    assert_ne!(Some(room), level.start_room(), "seed {seed}: column in the starting room");
                    assert!(!straight_in_from_a_door(&level, x, y), "seed {seed}: column ({x}, {y}) in a doorway's line");
                    assert!(!level.is_open(x, y));
                }
            }
        }
    }

    #[test]
    fn nothing_is_placed_on_a_column() {
        for depth in [1, 2, 3, 4] {
            for seed in 0..200 {
                let level = generate(seed, depth);
                let on_column = |x: f32, y: f32| cell_of(&level, x, y) == Cell::Column;
                assert!(!level.items.iter().any(|i| on_column(i.x, i.y)), "seed {seed} depth {depth}: item");
                assert!(!level.lights.iter().any(|i| on_column(i.x, i.y)), "seed {seed} depth {depth}: light");
                assert!(!level.folk.iter().any(|i| on_column(i.x, i.y)), "seed {seed} depth {depth}: folk");
                assert!(!level.props.iter().any(|i| on_column(i.x, i.y)), "seed {seed} depth {depth}: prop");
                assert!(!level.creatures.iter().any(|i| on_column(i.x, i.y)), "seed {seed} depth {depth}: creature");
                assert!(is_whole(&level), "seed {seed} depth {depth}");
            }
        }
    }

    #[test]
    fn rough_rock_has_no_columns_and_every_style_is_deterministic() {
        for seed in [0, 1, 1207, u32::MAX] {
            for depth in [1, 2, 3] {
                assert_eq!(generate(seed, depth), generate(seed, depth), "seed {seed} depth {depth}");
            }
            assert!(!generate(seed, 1).cells.contains(&Cell::Column));
        }
        assert_ne!(generate(1207, 1).cells, generate(1207, 2).cells);
    }

    #[test]
    fn stone_halls_run_wider_and_straighter() {
        let wide = |depth| (0..32).map(|s| generate(s, depth).rooms.iter().map(|r| r.area()).sum::<u32>() as f32 / generate(s, depth).rooms.len() as f32).sum::<f32>();
        assert!(wide(2) > wide(1), "{} against {}", wide(2), wide(1));
        assert!(generate(1207, 2).ceiling > generate(1207, 1).ceiling);
    }

    #[test]
    fn params_that_make_no_hall_still_lay_out_a_level() {
        let mut params = LevelParams::for_style(LevelStyle::StoneHalls);
        params.hall_chance = 0.0;
        params.side_max = 6;
        params.regular_chance = 0.0;
        for seed in [1, 2, 1207] {
            let level = generate_with(seed, 2, &params);
            assert!(is_whole(&level), "seed {seed}");
        }
        assert!(tries(1, 2, &params) <= RELAX_EVERY + 1);
    }

    fn spot_cells(level: &Level) -> Vec<(i32, i32)> {
        let cell = |x: f32, y: f32| (x.floor() as i32, y.floor() as i32);
        let mut out: Vec<_> = level.items.iter().map(|s| cell(s.x, s.y)).collect();
        out.extend(level.lights.iter().map(|s| cell(s.x, s.y)));
        out.extend(level.folk.iter().map(|s| cell(s.x, s.y)));
        out.extend(level.props.iter().map(|s| cell(s.x, s.y)));
        out.extend(level.creatures.iter().map(|s| cell(s.x, s.y)));
        out
    }

    #[test]
    fn every_level_has_a_reachable_way_down_far_from_the_start() {
        for depth in [1, 2, 3, 4] {
            for seed in 0..150 {
                let level = generate(seed, depth);
                let downs: Vec<_> = level.stairs.iter().filter(|s| s.kind == StairKind::Down).collect();
                assert_eq!(downs.len(), 1, "seed {seed} depth {depth}");
                let down = downs[0];
                let (ax, ay, _) = down.arrival();
                assert!(level.is_open(ax as i32, ay as i32), "seed {seed} depth {depth}: the approach is floor");
                assert_eq!(level.cell(down.x as i32, down.y as i32), Cell::Floor, "seed {seed} depth {depth}");
                let dist = crate::stairs::distances_from_start(&level);
                let at = |x: u32, y: u32| dist[(y * level.width + x) as usize];
                assert!(at(ax, ay) >= 0, "seed {seed} depth {depth}: reachable from the start");
                // At least `FAR_SHARE` as far as the furthest cell it could have stood on: free
                // floor against a wall of a room, clear of doors, with free floor in front.
                // The cells of the creatures the stair was placed among, including rats and snakes later dropped to make room for the wild species (#662).
                let (bare, layout_seed) = generate_without_creatures(seed, depth, &LevelParams::for_style(LevelStyle::for_depth(depth)));
                let mut taken = spot_cells(&level);
                taken.extend(place_creatures(&bare, &mut Mulberry32::new(layout_seed ^ CREATURE_SALT)).iter().map(|s| (s.x.floor() as i32, s.y.floor() as i32)));
                // And in wild ground: a stair never stands in a claim or behind its doors.
                let held = crate::claims::held_cells(&level, &level.claims);
                let free = |x: i32, y: i32| level.cell(x, y) == Cell::Floor && !taken.contains(&(x, y)) && !held[(y as u32 * level.width + x as u32) as usize];
                let mut furthest = 0;
                for y in 0..level.height as i32 {
                    for x in 0..level.width as i32 {
                        let near_door = (-1..=1).any(|dy| (-1..=1).any(|dx| matches!(level.cell(x + dx, y + dy), Cell::Door | Cell::Gate | Cell::Bars)));
                        for facing in Facing::ALL {
                            let (dx, dy) = facing.step();
                            let (px, py) = (x - dx, y - dy);
                            let wall_behind = !level.is_open(x + dx, y + dy);
                            let same_room = level.room_at(x, y).is_some() && level.room_at(x, y) == level.room_at(px, py);
                            if free(x, y) && free(px, py) && wall_behind && !near_door && same_room && (x, y) != (level.entrance.x as i32, level.entrance.y as i32) {
                                furthest = furthest.max(dist[(py as u32 * level.width + px as u32) as usize]);
                            }
                        }
                    }
                }
                assert!(at(ax, ay) as f32 >= furthest as f32 * crate::stairs::FAR_SHARE, "seed {seed} depth {depth}: {} of {furthest} cells away", at(ax, ay));
                // Shutting the stair's cell cuts nothing off.
                assert!(is_whole(&level), "seed {seed} depth {depth}");
            }
        }
    }

    #[test]
    fn nothing_stands_where_a_delver_arrives_from_above() {
        for depth in [1, 2, 3, 4] {
            for seed in 0..150 {
                let level = generate(seed, depth);
                let (sx, sy, _) = level.start();
                let cell = |x: f32, y: f32| (x.floor() as i32, y.floor() as i32);
                let on = |x: f32, y: f32| cell(x, y) == (sx as i32, sy as i32);
                assert!(!level.props.iter().any(|s| on(s.x, s.y)), "seed {seed} depth {depth}: a prop on the start");
                assert!(!level.folk.iter().any(|s| on(s.x, s.y)), "seed {seed} depth {depth}: someone on the start");
                assert!(!level.creatures.iter().any(|s| on(s.x, s.y)), "seed {seed} depth {depth}: a creature on the start");
                assert!(!level.items.iter().any(|s| on(s.x, s.y)), "seed {seed} depth {depth}: an item on the start");
            }
        }
    }

    #[test]
    fn nothing_else_stands_on_a_stair() {
        for depth in [1, 2, 3] {
            for seed in 0..150 {
                let level = generate(seed, depth);
                let spots = spot_cells(&level);
                for stair in &level.stairs {
                    let cell = (stair.x as i32, stair.y as i32);
                    assert!(!spots.contains(&cell), "seed {seed} depth {depth}: {:?} has something on it", stair.kind);
                    assert_eq!(level.cell(cell.0, cell.1), Cell::Floor, "seed {seed} depth {depth}: a stair stands on bare floor");
                    let (ax, ay, _) = stair.arrival();
                    assert!(level.is_open(ax as i32, ay as i32));
                    if stair.kind == StairKind::Down {
                        assert!(!spots.contains(&(ax as i32, ay as i32)), "seed {seed} depth {depth}: something in the way of the way down");
                    }
                }
            }
        }
    }

    #[test]
    fn only_levels_below_the_first_have_a_way_up_where_they_are_entered() {
        for seed in [0, 1, 1207, 4242] {
            let first = generate(seed, 1);
            assert!(first.stair(StairKind::Up).is_none(), "seed {seed}");
            assert_eq!(first.cell(first.entrance.x as i32, first.entrance.y as i32), Cell::Door);
            for depth in [2, 3, 5] {
                let level = generate(seed, depth);
                let up = level.stair(StairKind::Up).expect("a way up");
                assert_eq!((up.x, up.y), (level.entrance.x, level.entrance.y));
                assert_eq!(up.arrival(), level.start(), "arriving from above stands where the level starts");
                assert_eq!(level.arrival(StairKind::Up), Some(level.start()));
                assert_ne!(level.cell(up.x as i32, up.y as i32), Cell::Door, "the entrance is no door below the first level");
                assert!(level.is_open(up.x as i32, up.y as i32));
                assert_eq!(level.stairs.len(), 2);
            }
        }
    }

    #[test]
    fn seed_1207_has_a_way_down_on_levels_one_and_two() {
        let run = 1207;
        let first = generate(depth_seed(run, 1), 1);
        let second = generate(depth_seed(run, 2), 2);
        assert_eq!(first.seed, 1207, "the first level keeps the run's seed");
        assert_ne!(second.seed, first.seed);
        let down = first.stair(StairKind::Down).expect("a way down");
        // Coming up from level two puts you beside it, turned away.
        let (x, y, facing) = first.arrival(StairKind::Down).unwrap();
        assert_eq!((x, y), down.approach());
        assert_eq!(facing, down.facing.opposite());
        assert!(second.stair(StairKind::Down).is_some() && second.stair(StairKind::Up).is_some());
        // A depth is always the same level.
        assert_eq!(second, generate(depth_seed(run, 2), 2));
        assert_ne!(depth_seed(run, 2), depth_seed(run, 3));
        assert_ne!(depth_seed(run, 2), depth_seed(run + 1, 2));
    }
}
