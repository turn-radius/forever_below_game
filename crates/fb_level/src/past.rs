//! What a level was built for, and what its rooms were for: the level's [`Past`] and each room's
//! [`Purpose`]. A hold keeps a throne room, quarters and guardrooms; a temple its shrines and a
//! library; a prison its cell blocks. Some rooms are given no purpose and stand bare.
//!
//! The past and purposes come from a stream of the level seed of their own, after the layout's
//! corridors and before its colonnades and anything furnished, so they never move a room or a
//! corridor; a hall made a cell block gets no columns. A cell block is the one purpose that changes cells: it carves a row of barred cells,
//! each with a gate, along a wall with no doorway ([`Cell::Bars`], [`Cell::Gate`]). A room keeps
//! its purpose when a faction claims it.
//!
//! Which purposes each past gives and how many is our own proposal.

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::generate::shuffle;
use crate::{Cell, Facing, Level, LevelParams, Room};

/// Salt for the past's stream.
pub(crate) const PAST_SALT: u32 = 0x9A57_0F1D;

/// What a level was built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Past {
    /// A lord's seat: a throne room, quarters and guardrooms, a cell block below.
    Hold,
    /// A holy house: shrines, a library, an ossuary, quarters for its keepers.
    Temple,
    /// A gaol: cell blocks, guardrooms and the guards' quarters.
    Prison,
}

impl Past {
    pub const ALL: [Past; 3] = [Past::Hold, Past::Temple, Past::Prison];

    /// Its name, for the level view.
    pub fn name(self) -> &'static str {
        match self {
            Past::Hold => "hold",
            Past::Temple => "temple",
            Past::Prison => "prison",
        }
    }

    /// The purpose its first room takes, given to the room that suits it best.
    fn anchor(self) -> Purpose {
        match self {
            Past::Hold => Purpose::ThroneRoom,
            Past::Temple => Purpose::Shrine,
            Past::Prison => Purpose::CellBlock,
        }
    }

    /// The purposes its other rooms draw from: each with a weight and the most rooms that take
    /// it, the anchor's room counted.
    fn table(self) -> &'static [(Purpose, u32, usize)] {
        match self {
            Past::Hold => &[(Purpose::Quarters, 3, 3), (Purpose::Guardroom, 2, 2), (Purpose::CellBlock, 2, 1), (Purpose::Shrine, 1, 1)],
            Past::Temple => &[(Purpose::Shrine, 2, 3), (Purpose::Library, 3, 1), (Purpose::Ossuary, 2, 2), (Purpose::Quarters, 2, 2)],
            Past::Prison => &[(Purpose::CellBlock, 3, 3), (Purpose::Guardroom, 3, 2), (Purpose::Quarters, 2, 2), (Purpose::Ossuary, 1, 1)],
        }
    }
}

/// What a room was built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Purpose {
    /// Where soldiers or keepers slept: bunks in rows, a table.
    Quarters,
    /// A row of barred cells along one wall, each with a gate.
    CellBlock,
    /// A throne on a dais facing the door, braziers either side.
    ThroneRoom,
    /// An altar against the far wall, lit.
    Shrine,
    /// Bookshelves along the walls, a reading table, books lying about.
    Library,
    /// Bones heaped in the corners.
    Ossuary,
    /// A table and a weapon rack.
    Guardroom,
}

impl Purpose {
    pub const ALL: [Purpose; 7] = [Purpose::Quarters, Purpose::CellBlock, Purpose::ThroneRoom, Purpose::Shrine, Purpose::Library, Purpose::Ossuary, Purpose::Guardroom];

    /// Its name, for the level view.
    pub fn name(self) -> &'static str {
        match self {
            Purpose::Quarters => "quarters",
            Purpose::CellBlock => "cell block",
            Purpose::ThroneRoom => "throne room",
            Purpose::Shrine => "shrine",
            Purpose::Library => "library",
            Purpose::Ossuary => "ossuary",
            Purpose::Guardroom => "guardroom",
        }
    }

    /// A single letter for maps (lower case, apart from the roles' capitals).
    pub fn letter(self) -> char {
        match self {
            Purpose::Quarters => 'q',
            Purpose::CellBlock => 'c',
            Purpose::ThroneRoom => 't',
            Purpose::Shrine => 's',
            Purpose::Library => 'l',
            Purpose::Ossuary => 'o',
            Purpose::Guardroom => 'g',
        }
    }

    /// Whether `room` is big enough to be put to this purpose. A cell block also has to fit its
    /// cells ([`cell_block`]).
    fn fits(self, room: &Room) -> bool {
        let short = room.w.min(room.h);
        match self {
            Purpose::ThroneRoom => room.area() >= 20 && short >= 4,
            Purpose::Library => room.area() >= 12 && short >= 3,
            Purpose::Quarters | Purpose::Shrine => room.area() >= 9,
            Purpose::CellBlock => short >= CELL_BLOCK_DEPTH,
            Purpose::Guardroom => room.area() >= 6,
            Purpose::Ossuary => true,
        }
    }
}

impl Level {
    /// The purpose room `room` was built for, if any.
    pub fn purpose_of(&self, room: usize) -> Option<Purpose> {
        self.purposes.get(room).copied().flatten()
    }

    /// The rooms put to `purpose`.
    pub fn rooms_for(&self, purpose: Purpose) -> impl Iterator<Item = usize> + '_ {
        (0..self.rooms.len()).filter(move |&r| self.purpose_of(r) == Some(purpose))
    }
}

/// Cells deep a cell is, from the wall to its bars.
const CELL_DEPTH: u32 = 2;
/// Cells wide a cell is, at least.
const CELL_WIDTH: u32 = 2;
/// The shortest side a room needs for a cell block: the cells, their bars and an aisle two cells
/// wide.
const CELL_BLOCK_DEPTH: u32 = CELL_DEPTH + 1 + 2;
/// Fewest cells in a block.
const FEWEST_CELLS: u32 = 2;

/// Draws the level's past and its rooms' purposes, and carves its cell blocks. A pure function of
/// the level's layout and its seed's past stream.
pub(crate) fn settle_past(level: &mut Level, layout_seed: u32, p: &LevelParams) {
    let mut rng = Mulberry32::new(layout_seed ^ PAST_SALT);
    level.purposes = vec![None; level.rooms.len()];
    level.past = None;
    if !rng.chance(p.past_chance) {
        return;
    }
    let past = *rng.pick(&Past::ALL);
    level.past = Some(past);
    let start_room = level.start_room();
    let mut rooms: Vec<usize> = (0..level.rooms.len()).filter(|&r| Some(r) != start_room).collect();
    if rooms.is_empty() {
        return;
    }
    shuffle(&mut rooms, &mut rng);
    let wanted = ((rooms.len() as f64 * p.purpose_share).round() as usize).clamp(1, rooms.len());
    let mut given: Vec<(usize, Purpose)> = Vec::new();

    // The anchor first, in the room that suits it best.
    let anchor = past.anchor();
    let best = match anchor {
        // The grandest room, the largest: in the stone halls, the one that takes columns.
        Purpose::ThroneRoom => rooms.iter().copied().filter(|&r| anchor.fits(&level.rooms[r])).max_by_key(|&r| (level.rooms[r].area(), std::cmp::Reverse(r))),
        // The room deepest from the way in.
        Purpose::Shrine => {
            let (sx, sy, _) = level.start();
            let far = |r: usize| {
                let (cx, cy) = level.rooms[r].centre();
                ((cx - sx as f32).abs() + (cy - sy as f32).abs()) as u32
            };
            rooms.iter().copied().filter(|&r| anchor.fits(&level.rooms[r])).max_by_key(|&r| (far(r), std::cmp::Reverse(r)))
        }
        _ => rooms.iter().copied().find(|&r| try_purpose(level, r, anchor, &mut rng)),
    };
    // Each pick fits; a cell block's is carved as it is found.
    if let Some(r) = best {
        level.purposes[r] = Some(anchor);
        given.push((r, anchor));
    }

    for &r in &rooms {
        if given.len() >= wanted {
            break;
        }
        if level.purposes[r].is_some() {
            continue;
        }
        let room = level.rooms[r];
        let open: Vec<(Purpose, u32)> = past
            .table()
            .iter()
            .filter(|&&(purpose, _, most)| given.iter().filter(|(_, g)| *g == purpose).count() < most && purpose.fits(&room))
            .map(|&(purpose, weight, _)| (purpose, weight))
            .collect();
        if open.is_empty() {
            continue;
        }
        let purpose = *rng.weighted(&open);
        if purpose == Purpose::CellBlock && !try_purpose(level, r, purpose, &mut rng) {
            continue;
        }
        level.purposes[r] = Some(purpose);
        given.push((r, purpose));
    }
}

/// Puts room `r` to `purpose` if it can be: a cell block carves its cells and is marked; any
/// other purpose only has to fit. Returns whether it took.
fn try_purpose(level: &mut Level, r: usize, purpose: Purpose, rng: &mut Mulberry32) -> bool {
    if purpose != Purpose::CellBlock {
        return purpose.fits(&level.rooms[r]);
    }
    let Some(cells) = cell_block(level, r, rng) else { return false };
    for (x, y, cell) in cells {
        level.cells[(y as u32 * level.width + x as u32) as usize] = cell;
    }
    level.purposes[r] = Some(purpose);
    true
}

/// The cells of a cell block in room `r`, as `(x, y, cell)` to set, or `None` if none fits: a row
/// of cells [`CELL_DEPTH`] deep along a wall with no doorway, each at least [`CELL_WIDTH`] wide,
/// parted by bars and fronted by a row of bars with a gate into each. The aisle in front stays
/// whole, so every cell is reached through its gate.
pub(crate) fn cell_block(level: &Level, r: usize, rng: &mut Mulberry32) -> Option<Vec<(i32, i32, Cell)>> {
    let room = level.rooms[r];
    let first = rng.int(0, 3) as usize;
    (0..4).find_map(|k| {
        let side = Facing::ALL[(first + k) % 4];
        cell_block_on(level, &room, side, rng)
    })
}

/// A cell block along the wall on `side` of `room`, if one fits there.
fn cell_block_on(level: &Level, room: &Room, side: Facing, rng: &mut Mulberry32) -> Option<Vec<(i32, i32, Cell)>> {
    let (length, depth) = match side {
        Facing::North | Facing::South => (room.w, room.h),
        Facing::East | Facing::West => (room.h, room.w),
    };
    // Each cell and the bars beside it take CELL_WIDTH + 1, bar the last.
    let count = (length + 1) / (CELL_WIDTH + 1);
    if depth < CELL_BLOCK_DEPTH || count < FEWEST_CELLS {
        return None;
    }
    // `along` runs down the wall, `out` from it into the room.
    let at = |along: u32, out: u32| wall_cell(room, side, along, out);
    // No doorway may open into the cells or their bars.
    for along in 0..length {
        for out in 0..=CELL_DEPTH {
            let (x, y) = at(along, out);
            let doorway = Facing::ALL.iter().any(|f| {
                let (dx, dy) = f.step();
                level.cell(x + dx, y + dy) == Cell::Door
            });
            if doorway || level.cell(x, y) != Cell::Floor {
                return None;
            }
        }
    }
    // The last cell takes what the others leave over.
    let spare = length - (count * (CELL_WIDTH + 1) - 1);
    let mut cells = Vec::new();
    let mut start = 0;
    for i in 0..count {
        let width = CELL_WIDTH + if i + 1 == count { spare } else { 0 };
        let gate = start + rng.int(0, width - 1);
        for along in start..start + width {
            let (x, y) = at(along, CELL_DEPTH);
            cells.push((x, y, if along == gate { Cell::Gate } else { Cell::Bars }));
        }
        if i + 1 < count {
            // The bars between this cell and the next, down to the front.
            for out in 0..=CELL_DEPTH {
                let (x, y) = at(start + width, out);
                cells.push((x, y, Cell::Bars));
            }
        }
        start += width + 1;
    }
    Some(cells)
}

/// The cell of `room` `along` cells down its wall on `side` (from its west or north end) and
/// `out` cells in from that wall.
fn wall_cell(room: &Room, side: Facing, along: u32, out: u32) -> (i32, i32) {
    match side {
        Facing::North => ((room.x + along) as i32, (room.y + out) as i32),
        Facing::South => ((room.x + along) as i32, (room.y + room.h - 1 - out) as i32),
        Facing::West => ((room.x + out) as i32, (room.y + along) as i32),
        Facing::East => ((room.x + room.w - 1 - out) as i32, (room.y + along) as i32),
    }
}

/// How many cells long the wall on `side` of `room` is.
fn wall_length(room: &Room, side: Facing) -> u32 {
    match side {
        Facing::North | Facing::South => room.w,
        Facing::East | Facing::West => room.h,
    }
}

/// Where a room's centrepiece stands (a throne room's throne, a shrine's altar): a floor cell
/// against the wall across from the room's main doorway (of its doorways the nearest the start),
/// as near the middle of that wall as is clear of doorways, and the wall it backs onto. `None`
/// for a room with no doorway or no clear cell.
pub fn seat(level: &Level, r: usize) -> Option<((i32, i32), Facing)> {
    let room = level.rooms[r];
    let (sx, sy, _) = level.start();
    let mut doors: Vec<(i32, i32, Facing)> = Vec::new();
    for side in Facing::ALL {
        let (dx, dy) = side.step();
        for along in 0..wall_length(&room, side) {
            let (x, y) = wall_cell(&room, side, along, 0);
            if level.cell(x + dx, y + dy) == Cell::Door {
                doors.push((x + dx, y + dy, side));
            }
        }
    }
    let &(_, _, side) = doors.iter().min_by_key(|&&(x, y, _)| ((x - sx as i32).abs() + (y - sy as i32).abs(), y, x))?;
    let back = side.opposite();
    let middle = wall_length(&room, back) as i32 / 2;
    [0, -1, 1, -2, 2].into_iter().filter_map(|k| u32::try_from(middle + k).ok()).filter(|&along| along < wall_length(&room, back)).map(|along| wall_cell(&room, back, along, 0)).find(|&(x, y)| level.cell(x, y) == Cell::Floor && !crate::folk::by_door(level, x, y) && !crate::props::behind_a_gap(level, x, y, &[back])).map(|cell| (cell, back))
}

/// The lights a room's purpose brings: braziers either side of a throne, one beside an altar,
/// each a cell out from the wall and two along it from the seat, on floor. A colonnaded hall
/// has its braziers between its columns already.
pub(crate) fn purpose_lights(level: &Level) -> Vec<crate::LightSpot> {
    let mut lights = Vec::new();
    for r in 0..level.rooms.len() {
        let sides: &[i32] = match level.purpose_of(r) {
            Some(Purpose::ThroneRoom) => &[-2, 2],
            Some(Purpose::Shrine) => &[2, -2],
            _ => continue,
        };
        if crate::generate::is_colonnaded(level, &level.rooms[r]) {
            continue;
        }
        let Some(((x, y), back)) = seat(level, r) else { continue };
        let (bx, by) = back.step();
        let (ax, ay) = (-by, bx);
        let mut lit = 0;
        for &k in sides {
            let Some((lx, ly)) = [1].into_iter().map(|out| (x + ax * k - bx * out, y + ay * k - by * out)).find(|&(lx, ly)| level.cell(lx, ly) == Cell::Floor) else { continue };
            if crate::folk::by_door(level, lx, ly) {
                continue;
            }
            lights.push(crate::LightSpot { x: lx as f32 + 0.5, y: ly as f32 + 0.5, kind: crate::LightKind::Brazier });
            lit += 1;
            // A shrine takes one.
            if level.purpose_of(r) == Some(Purpose::Shrine) && lit == 1 {
                break;
            }
        }
    }
    lights
}

/// The open cells of room `r` that lie behind bars: in a cell block's cells, reached from the
/// room's doorways only through a gate.
pub fn behind_bars(level: &Level, r: usize) -> Vec<(i32, i32)> {
    let room = level.rooms[r];
    let cells: Vec<(i32, i32)> = (room.y..room.y + room.h).flat_map(|y| (room.x..room.x + room.w).map(move |x| (x as i32, y as i32))).collect();
    let free = |x: i32, y: i32| room.contains(x, y) && level.is_open(x, y) && level.cell(x, y) != Cell::Gate;
    let mut seen: Vec<(i32, i32)> = cells.iter().copied().filter(|&(x, y)| free(x, y) && Facing::ALL.iter().any(|f| level.cell(x + f.step().0, y + f.step().1) == Cell::Door)).collect();
    let mut stack = seen.clone();
    while let Some((x, y)) = stack.pop() {
        for f in Facing::ALL {
            let (nx, ny) = (x + f.step().0, y + f.step().1);
            if free(nx, ny) && !seen.contains(&(nx, ny)) {
                seen.push((nx, ny));
                stack.push((nx, ny));
            }
        }
    }
    if seen.is_empty() {
        return Vec::new();
    }
    cells.into_iter().filter(|&(x, y)| free(x, y) && !seen.contains(&(x, y))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::reachable_from;
    use crate::{LevelStyle, generate};

    fn sweep() -> impl Iterator<Item = (u32, u32)> {
        (1..=20).flat_map(|seed| (1..=4).map(move |depth| (seed, depth)))
    }

    #[test]
    fn every_past_shows_up_and_levels_hold_several_purposes() {
        let mut pasts = std::collections::HashSet::new();
        let mut varied = 0;
        for seed in 1..=20 {
            let level = generate(seed, 2);
            assert_eq!(level.style, LevelStyle::StoneHalls);
            if let Some(past) = level.past {
                pasts.insert(past);
            }
            let kinds: std::collections::HashSet<_> = level.purposes.iter().flatten().collect();
            varied += (kinds.len() >= 2) as u32;
        }
        assert_eq!(pasts.len(), Past::ALL.len(), "{pasts:?}");
        assert!(varied >= 12, "only {varied} of 20 levels hold two purposes or more");
    }

    #[test]
    fn the_starting_room_has_no_purpose_and_purposes_match_the_rooms() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            assert_eq!(level.purposes.len(), level.rooms.len(), "seed {seed} depth {depth}");
            let start = level.start_room().unwrap();
            assert_eq!(level.purpose_of(start), None, "seed {seed} depth {depth}");
            if level.past.is_none() {
                assert!(level.purposes.iter().all(Option::is_none));
            }
        }
    }

    #[test]
    fn a_hold_has_one_throne_room_at_most_in_its_largest_room() {
        let mut thrones = 0;
        for (seed, depth) in (1..=60).flat_map(|s| [(s, 2), (s, 4)]) {
            let level = generate(seed, depth);
            let rooms: Vec<usize> = level.rooms_for(Purpose::ThroneRoom).collect();
            assert!(rooms.len() <= 1, "seed {seed} depth {depth}: {rooms:?}");
            let Some(&throne) = rooms.first() else { continue };
            assert_eq!(level.past, Some(Past::Hold));
            thrones += 1;
            let start = level.start_room();
            let largest = (0..level.rooms.len()).filter(|&r| Some(r) != start).map(|r| level.rooms[r].area()).max().unwrap();
            assert_eq!(level.rooms[throne].area(), largest, "seed {seed} depth {depth}");
        }
        assert!(thrones > 0);
    }

    #[test]
    fn cell_blocks_are_reached_only_through_their_gates() {
        let mut blocks = 0;
        for (seed, depth) in (1..=80).flat_map(|s| [(s, 2), (s, 3), (s, 4)]) {
            let level = generate(seed, depth);
            for r in level.rooms_for(Purpose::CellBlock) {
                blocks += 1;
                let room = level.rooms[r];
                let gates: Vec<(i32, i32)> = (room.y..room.y + room.h).flat_map(|y| (room.x..room.x + room.w).map(move |x| (x as i32, y as i32))).filter(|&(x, y)| level.cell(x, y) == Cell::Gate).collect();
                assert!(gates.len() as u32 >= FEWEST_CELLS, "seed {seed} depth {depth}");
                // Shut every gate: what lies behind them is cut off from the start.
                let mut shut = level.clone();
                for &(x, y) in &gates {
                    shut.cells[(y as u32 * shut.width + x as u32) as usize] = Cell::Bars;
                }
                let (sx, sy, _) = shut.start();
                let seen = reachable_from(&shut, sx as i32, sy as i32);
                for &(gx, gy) in &gates {
                    let behind = Facing::ALL.iter().map(|f| f.step()).map(|(dx, dy)| (gx + dx, gy + dy)).filter(|&(x, y)| shut.is_open(x, y) && room.contains(x, y)).filter(|&(x, y)| !seen[(y as u32 * shut.width + x as u32) as usize]).count();
                    assert_eq!(behind, 1, "seed {seed} depth {depth}: gate ({gx}, {gy}) leads to {behind} shut cells");
                }
                // No doorway opens onto the bars or into a cell.
                for y in room.y as i32 - 1..=(room.y + room.h) as i32 {
                    for x in room.x as i32 - 1..=(room.x + room.w) as i32 {
                        if level.cell(x, y) != Cell::Door {
                            continue;
                        }
                        for (dx, dy) in Facing::ALL.iter().map(|f| f.step()) {
                            let (nx, ny) = (x + dx, y + dy);
                            assert!(level.cell(nx, ny) != Cell::Bars && !(room.contains(nx, ny) && !seen[(ny as u32 * shut.width + nx as u32) as usize] && shut.is_open(nx, ny)), "seed {seed} depth {depth}: door ({x}, {y})");
                        }
                    }
                }
            }
        }
        assert!(blocks >= 10, "only {blocks} cell blocks");
    }

    #[test]
    fn bars_block_walking_but_not_sight() {
        assert!(!Cell::Bars.is_open());
        assert!(Cell::Bars.is_clear());
        assert!(Cell::Gate.is_open() && Cell::Gate.is_doorway());
        assert!(!Cell::Rock.is_clear() && !Cell::Column.is_clear());
        let level = (1..=80).flat_map(|s| [(s, 2), (s, 4)]).map(|(s, d)| generate(s, d)).find(|l| l.rooms_for(Purpose::CellBlock).next().is_some()).expect("a level with a cell block");
        let (bx, by) = (0..level.height as i32).flat_map(|y| (0..level.width as i32).map(move |x| (x, y))).find(|&(x, y)| level.cell(x, y) == Cell::Bars).unwrap();
        // Across the bars from one open side to the other.
        let (dx, dy) = Facing::ALL.iter().map(|f| f.step()).find(|&(dx, dy)| level.is_open(bx + dx, by + dy) && level.is_open(bx - dx, by - dy)).expect("bars between two open cells");
        let from = ((bx + dx) as f32 + 0.5, (by + dy) as f32 + 0.5);
        let to = ((bx - dx) as f32 + 0.5, (by - dy) as f32 + 0.5);
        assert!(crate::sight::line_of_sight(&level, from, to));
    }

    #[test]
    fn no_past_when_its_chance_is_nil() {
        let params = LevelParams { past_chance: 0.0, ..LevelParams::for_style(LevelStyle::StoneHalls) };
        for seed in 1..=10 {
            let level = crate::generate_with(seed, 2, &params);
            assert_eq!(level.past, None);
            assert!(!level.cells.contains(&Cell::Bars));
        }
    }

    #[test]
    fn the_past_is_deterministic() {
        for seed in [1, 7, 1207] {
            let (a, b) = (generate(seed, 2), generate(seed, 2));
            assert_eq!((a.past, &a.purposes, &a.cells), (b.past, &b.purposes, &b.cells));
        }
    }
}
