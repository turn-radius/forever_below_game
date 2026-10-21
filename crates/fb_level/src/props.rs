//! Props: the things standing about the level that are not picked up. Containers to open
//! (Armory chests, barrels, crates and amphorae) and scenery (rubble, bones, pillars), set
//! against the walls and in the corners of rooms.
//!
//! Props come from a fourth stream of the level seed, after the layout, the furnishing and the
//! folk, so adding or changing props never moves a wall, an item, a light or anyone standing
//! about. They keep out of doorways (and the cells beside them), corridors and the way in from
//! the entrance, clear of every item, light and folk, and every open cell stays reachable with
//! the props standing in the way.
//!
//! Where props go and how many is our own proposal; neither the seed doc nor the plate
//! furnishes a level.

use armory::build::scenery::Scenery;
use armory::families::chests::{ChestState, ChestStyle, Chests};
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::folk::by_door;
use crate::generate::{item_seed, shuffle};
use crate::{CELL, Cell, Facing, Level, Room};

/// Something standing in the level that is not picked up, at a point in grid coordinates
/// (cells, fractional).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PropSpot {
    pub x: f32,
    pub y: f32,
    /// Turn about +Y, radians, as [`ItemSpot::yaw`](crate::ItemSpot): 0 leaves the prop's
    /// front (item +Z) facing south (+y), a quarter turn faces it east.
    pub yaw: f32,
    pub prop: Prop,
}

/// What a prop is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Prop {
    /// An Armory container: the [`armory::Family::Chests`] item of `style` rolled from `seed`
    /// (`Chests::roll_as`), holding what `loot` rolls.
    Container { style: ChestStyle, seed: u32, loot: u32 },
    /// Scenery built from `seed` (`armory::build::scenery`).
    Scenery { kind: Scenery, seed: u32 },
}

/// The floor a prop covers about its centre, in world units, before its yaw turns it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FloorShape {
    /// A circle of this radius.
    Round(f32),
    /// A rectangle: half its width along item X and half its depth along item Z.
    Square { half_width: f32, half_depth: f32 },
}

impl FloorShape {
    /// The furthest it reaches from its centre.
    pub fn radius(self) -> f32 {
        match self {
            FloorShape::Round(r) => r,
            FloorShape::Square { half_width, half_depth } => half_width.hypot(half_depth),
        }
    }

    /// The same shape `factor` times as large.
    pub fn scaled(self, factor: f32) -> FloorShape {
        match self {
            FloorShape::Round(r) => FloorShape::Round(r * factor),
            FloorShape::Square { half_width, half_depth } => FloorShape::Square { half_width: half_width * factor, half_depth: half_depth * factor },
        }
    }

    /// How far it reaches along item X and item Z.
    fn half_extents(self) -> (f32, f32) {
        match self {
            FloorShape::Round(r) => (r, r),
            FloorShape::Square { half_width, half_depth } => (half_width, half_depth),
        }
    }
}

/// How much larger than true Armory scale container props are drawn, and so the floor they
/// cover: the user's call ("containers look small"), which makes a chest about a metre wide.
/// The game draws them at its `container_scale` tuning, which ships at this value; placement
/// here keeps to the constant so a seed lays out the same level whatever the tuning says.
pub const CONTAINER_SCALE: f32 = 1.6;

/// How far a chest's lid stands open once it has been opened (the Armory's `chest.open`,
/// 0 shut to 1 thrown back). Chests are set far enough off the wall for it.
pub const OPEN_LID: f32 = 0.55;

/// The Armory state of a container prop of `style` rolled from `seed`: shut, with nothing
/// heaped in it (what it holds is its loot, not its mesh), or with its lid raised to
/// [`OPEN_LID`] once `opened`. Barrels, crates and amphorae look the same either way.
pub fn chest_state(style: ChestStyle, seed: u32, opened: bool) -> ChestState {
    let mut state = Chests::roll_as(seed, style);
    state.chest.treasure = false;
    state.chest.open = if opened && style.is_lidded_box() { OPEN_LID } else { 0.0 };
    state
}

impl Prop {
    /// The floor it covers ([`FloorShape`]), the largest any seed builds. Containers are sized
    /// from `armory::families::chests` at its largest size and widest shape, handles, locks and
    /// hoops included, drawn [`CONTAINER_SCALE`] times; a test holds every style's mesh inside
    /// its shape.
    pub fn floor_shape(self) -> FloorShape {
        match self {
            Prop::Container { style, .. } => true_container_shape(style).scaled(CONTAINER_SCALE),
            Prop::Scenery { kind: Scenery::Pillar, .. } => {
                let half = Scenery::Pillar.radius() * std::f32::consts::FRAC_1_SQRT_2;
                FloorShape::Square { half_width: half, half_depth: half }
            }
            Prop::Scenery { kind, .. } => FloorShape::Round(kind.radius()),
        }
    }

    /// Room it needs behind its back (item -Z) beyond its floor shape, in world units: a
    /// chest's lid swings back over the hinges when it opens, as far as it is drawn larger.
    pub fn room_behind(self) -> f32 {
        match self {
            Prop::Container { style, .. } if style.is_lidded_box() => 1.8 * CONTAINER_SCALE,
            _ => 0.0,
        }
    }

    /// Whether it is a container.
    pub fn is_container(self) -> bool {
        matches!(self, Prop::Container { .. })
    }
}

/// The floor a container of `style` covers at true Armory scale.
fn true_container_shape(style: ChestStyle) -> FloorShape {
    match style {
        ChestStyle::Chest => FloorShape::Square { half_width: 4.5, half_depth: 2.4 },
        ChestStyle::Strongbox => FloorShape::Square { half_width: 2.6, half_depth: 1.8 },
        ChestStyle::Coffer => FloorShape::Square { half_width: 1.9, half_depth: 1.3 },
        ChestStyle::Crate => FloorShape::Square { half_width: 3.9, half_depth: 2.6 },
        ChestStyle::Barrel => FloorShape::Round(3.3),
        ChestStyle::Amphora => FloorShape::Round(2.4),
    }
}

impl PropSpot {
    /// Its centre in grid coordinates.
    pub fn centre(&self) -> (f32, f32) {
        (self.x, self.y)
    }
}

/// Salt for the props stream.
pub(crate) const PROP_SALT: u32 = 0x9409_5EA7;
/// How far the drawn walls stand into a cell at most, in world units: the game's rock relief
/// (`game/src/level/geometry.rs`, `WALL_RELIEF` 2.2 times its noise's largest swing, 0.675),
/// with a little to spare. Props keep this far off the grid's wall faces.
pub const WALL_GAP: f32 = 1.6;
/// Least distance between a prop's edge and an item, a light or someone standing, in world
/// units, so nothing stands in a brazier or on the loot.
const CLEARANCE: f32 = 5.5;
/// Least gap between two props' edges, in world units.
const PROP_GAP: f32 = 0.6;
/// Props in a room: one per this many cells of floor, give or take one, and at most
/// [`MOST_IN_A_ROOM`].
const CELLS_PER_PROP: u32 = 9;
const MOST_IN_A_ROOM: u32 = 5;
/// Most a square container is turned off square to its wall, radians, so a row of them does
/// not look ruled.
const CONTAINER_TURN: f32 = 0.06;
/// Chance that a room stands bare of props.
const BARE_ROOM: f64 = 0.15;

/// What a room's props are, by weight: mostly containers, with scenery among them.
const KINDS: &[(PropKind, u32)] = &[(PropKind::Container, 9), (PropKind::Scenery(Scenery::Rubble), 4), (PropKind::Scenery(Scenery::Bones), 3), (PropKind::Scenery(Scenery::Pillar), 2)];
/// Container styles, by weight. Every container is a prop: none lie about as loose loot
/// (`generate::LOOT` has no chests), so strongboxes and coffers stand here too, now and then.
const STYLES: &[(ChestStyle, u32)] =
    &[(ChestStyle::Chest, 4), (ChestStyle::Barrel, 6), (ChestStyle::Crate, 6), (ChestStyle::Amphora, 4), (ChestStyle::Strongbox, 1), (ChestStyle::Coffer, 1)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropKind {
    Container,
    Scenery(Scenery),
}

/// A cell along a room's walls: the walls it stands against, in [`Facing::ALL`] order.
#[derive(Clone, Debug)]
struct Nook {
    x: i32,
    y: i32,
    walls: Vec<Facing>,
}

impl Nook {
    fn is_corner(&self) -> bool {
        self.walls.len() >= 2
    }
}

/// Lays out the level's props: a few in most rooms, against the walls and in the corners, and
/// always at least one container. The first container in every level is a chest.
pub(crate) fn place_props(level: &Level, rng: &mut Mulberry32) -> Vec<PropSpot> {
    let mut placed: Vec<PropSpot> = Vec::new();
    let mut occupied = vec![false; level.cells.len()];
    let keep_clear = entrance_path(level);
    for room in &level.rooms {
        if rng.chance(BARE_ROOM) {
            continue;
        }
        let count = (room.area() / CELLS_PER_PROP + rng.int(0, 1)).clamp(1, MOST_IN_A_ROOM);
        let mut nooks = nooks(level, room, &keep_clear);
        shuffle(&mut nooks, rng);
        for _ in 0..count {
            let kind = *rng.weighted(KINDS);
            let prop = roll_prop(kind, &placed, rng);
            if let Some(spot) = nooks.iter().find_map(|nook| fit(level, nook, prop, &placed, &occupied, rng)) {
                occupied[cell_index(level, spot.x, spot.y)] = true;
                placed.push(spot);
            }
        }
    }
    // A level always holds something to open.
    if !placed.iter().any(|p| p.prop.is_container()) {
        let prop = roll_prop(PropKind::Container, &placed, rng);
        let spot = level.rooms.iter().flat_map(|room| nooks(level, room, &keep_clear)).find_map(|nook| fit(level, &nook, prop, &placed, &occupied, rng));
        placed.extend(spot);
    }
    placed
}

fn roll_prop(kind: PropKind, placed: &[PropSpot], rng: &mut Mulberry32) -> Prop {
    match kind {
        PropKind::Container => {
            let first = !placed.iter().any(|p| p.prop.is_container());
            let style = if first { ChestStyle::Chest } else { *rng.weighted(STYLES) };
            Prop::Container { style, seed: item_seed(rng), loot: item_seed(rng) }
        }
        PropKind::Scenery(kind) => Prop::Scenery { kind, seed: item_seed(rng) },
    }
}

fn cell_index(level: &Level, x: f32, y: f32) -> usize {
    (y.floor() as u32 * level.width + x.floor() as u32) as usize
}

/// The cells walked in from the entrance: the start and on in a straight line across the room
/// the entrance leads into, to the far side.
fn entrance_path(level: &Level) -> Vec<(i32, i32)> {
    let (x, y, inward) = level.start();
    let (dx, dy) = inward.step();
    let (mut x, mut y) = (x as i32, y as i32);
    let mut path = Vec::new();
    while level.is_open(x, y) {
        path.push((x, y));
        x += dx;
        y += dy;
    }
    path
}

/// The room's cells along its walls where a prop may stand: not in or beside a doorway and
/// not on the way in.
fn nooks(level: &Level, room: &Room, keep_clear: &[(i32, i32)]) -> Vec<Nook> {
    let mut nooks = Vec::new();
    for y in room.y as i32..(room.y + room.h) as i32 {
        for x in room.x as i32..(room.x + room.w) as i32 {
            // Props stand on floor, backed by rock; a hall's columns are not a wall to stand against.
            if level.cell(x, y) != Cell::Floor {
                continue;
            }
            let walls: Vec<Facing> = Facing::ALL
                .into_iter()
                .filter(|f| {
                    let (dx, dy) = f.step();
                    level.cell(x + dx, y + dy) == Cell::Rock
                })
                .collect();
            if walls.is_empty() || by_door(level, x, y) || keep_clear.contains(&(x, y)) || behind_a_gap(level, x, y, &walls) {
                continue;
            }
            nooks.push(Nook { x, y, walls });
        }
    }
    nooks
}

/// Whether a cell backed by `walls` lies straight behind a gap between two columns, where a
/// prop would block the way from the aisle to the walls.
fn behind_a_gap(level: &Level, x: i32, y: i32, walls: &[Facing]) -> bool {
    walls.iter().any(|w| {
        let (dx, dy) = w.opposite().step();
        let (gx, gy) = (x + dx, y + dy);
        level.cell(gx, gy) == Cell::Floor && level.cell(gx + dy, gy + dx) == Cell::Column && level.cell(gx - dy, gy - dx) == Cell::Column
    })
}

/// The prop set into `nook` if it fits there: its back to a wall (the first one, in a corner,
/// with its side to the second), clear of the level's items, lights and folk and of `placed`,
/// and leaving every open cell reachable. Pillars stand only in corners.
fn fit(level: &Level, nook: &Nook, prop: Prop, placed: &[PropSpot], occupied: &[bool], rng: &mut Mulberry32) -> Option<PropSpot> {
    if matches!(prop, Prop::Scenery { kind: Scenery::Pillar, .. }) && !nook.is_corner() {
        return None;
    }
    if occupied[(nook.y as u32 * level.width + nook.x as u32) as usize] {
        return None;
    }
    let shape = prop.floor_shape();
    let (half_width, half_depth) = shape.half_extents();
    let back = nook.walls[0];
    // Out from the back wall: its depth, the room it needs behind, and the wall's relief, and
    // as far again as a container's small turn ([`CONTAINER_TURN`]) swings its corners.
    let (sway_out, sway_across) = match shape {
        FloorShape::Square { .. } if prop.is_container() => (half_width * CONTAINER_TURN.sin(), (half_depth + prop.room_behind()) * CONTAINER_TURN.sin()),
        _ => (0.0, 0.0),
    };
    let out = (half_depth + prop.room_behind() + WALL_GAP + sway_out) / CELL;
    let across = (half_width + WALL_GAP + sway_across) / CELL;
    if out * 2.0 > 1.0 || across * 2.0 > 1.0 {
        return None;
    }
    let (bx, by) = back.step();
    let (cx, cy) = (nook.x as f32 + 0.5, nook.y as f32 + 0.5);
    // Against the back wall; along it, against the side wall in a corner, else anywhere the
    // prop stays inside its cell.
    let mut x = if bx != 0 { cx + bx as f32 * (0.5 - out) } else { cx };
    let mut y = if by != 0 { cy + by as f32 * (0.5 - out) } else { cy };
    let slide = 0.5 - across;
    match nook.walls.get(1) {
        Some(side) => {
            let (sx, sy) = side.step();
            x += sx as f32 * slide;
            y += sy as f32 * slide;
        }
        None => {
            let along = rng.range_f32(-slide, slide);
            if bx == 0 {
                x += along;
            } else {
                y += along;
            }
        }
    }
    // Facing away from the back wall: yaw 0 faces south, so the yaw toward `(dx, dy)` is
    // `atan2(dx, dy)`. Round things turn as they please; the rest stand square.
    let mut yaw = (-bx as f32).atan2(-by as f32);
    if matches!(shape, FloorShape::Round(_)) {
        yaw += rng.range_f32(-std::f32::consts::PI, std::f32::consts::PI);
    } else if prop.is_container() {
        yaw += rng.range_f32(-CONTAINER_TURN, CONTAINER_TURN);
    }
    let spot = PropSpot { x, y, yaw, prop };

    let reach = shape.radius();
    let near = |px: f32, py: f32, gap: f32| (px - x).hypot(py - y) * CELL < gap;
    let crowded = level.items.iter().any(|i| near(i.x, i.y, reach + CLEARANCE))
        || level.lights.iter().any(|l| near(l.x, l.y, reach + CLEARANCE))
        || level.folk.iter().any(|f| near(f.x, f.y, reach + CLEARANCE))
        || placed.iter().any(|p| near(p.x, p.y, reach + p.prop.floor_shape().radius() + PROP_GAP));
    if crowded {
        return None;
    }
    let mut occupied = occupied.to_vec();
    occupied[(nook.y as u32 * level.width + nook.x as u32) as usize] = true;
    open_cells_reachable(level, &occupied).then_some(spot)
}

/// Whether every open cell can still be walked to from the start with props standing in
/// `occupied` cells: treating those cells as rock, every other open cell is reachable, and
/// each occupied cell (which a prop against its wall only partly fills) borders one that is.
pub(crate) fn open_cells_reachable(level: &Level, occupied: &[bool]) -> bool {
    let (sx, sy, _) = level.start();
    let index = |x: i32, y: i32| (y as u32 * level.width + x as u32) as usize;
    let walkable = |x: i32, y: i32| level.is_open(x, y) && !occupied[index(x, y)];
    let mut seen = vec![false; level.cells.len()];
    let mut stack = vec![(sx as i32, sy as i32)];
    seen[index(sx as i32, sy as i32)] = true;
    while let Some((x, y)) = stack.pop() {
        for facing in Facing::ALL {
            let (dx, dy) = facing.step();
            let (nx, ny) = (x + dx, y + dy);
            if walkable(nx, ny) && !seen[index(nx, ny)] {
                seen[index(nx, ny)] = true;
                stack.push((nx, ny));
            }
        }
    }
    (0..level.height as i32).all(|y| {
        (0..level.width as i32).all(|x| {
            if !level.is_open(x, y) {
                return true;
            }
            if !occupied[index(x, y)] {
                return seen[index(x, y)];
            }
            Facing::ALL.iter().any(|f| {
                let (dx, dy) = f.step();
                walkable(x + dx, y + dy) && seen[index(x + dx, y + dy)]
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cell, generate};
    use armory::families::Generator;

    const SEEDS: std::ops::RangeInclusive<u32> = 1..=64;

    /// Every seed on the first level and on the second, which a placement test holds for alike.
    fn sweep() -> impl Iterator<Item = (u32, u32)> {
        [1, 2].into_iter().flat_map(|depth| SEEDS.map(move |seed| (seed, depth)))
    }

    fn occupied(level: &Level) -> Vec<bool> {
        let mut occupied = vec![false; level.cells.len()];
        for p in &level.props {
            occupied[cell_index(level, p.x, p.y)] = true;
        }
        occupied
    }

    #[test]
    fn a_chest_prop_is_about_a_metre_wide() {
        // 10 units is a metre: a chest at its usual size and shape, drawn at its prop scale.
        let state = chest_state(ChestStyle::Chest, 1207, false);
        let (lo, hi) = Chests::build(&state).bounds().unwrap();
        let width = (hi[0] - lo[0]) * CONTAINER_SCALE;
        assert!((8.5..=12.0).contains(&width), "{width}");
        let FloorShape::Square { half_width, .. } = Prop::Container { style: ChestStyle::Chest, seed: 0, loot: 0 }.floor_shape() else { panic!() };
        assert!(half_width * 2.0 >= width);
    }

    #[test]
    fn props_are_deterministic() {
        for seed in [1, 1207, u32::MAX] {
            assert_eq!(generate(seed, 1).props, generate(seed, 1).props);
        }
        assert_ne!(generate(1, 1).props, generate(2, 1).props);
    }

    #[test]
    fn every_level_has_a_chest_and_some_scenery_shows_up() {
        let mut scenery = std::collections::HashSet::new();
        let mut styles = std::collections::HashSet::new();
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            assert!(level.props.iter().any(|p| matches!(p.prop, Prop::Container { style: ChestStyle::Chest, .. })), "seed {seed}: no chest");
            for p in &level.props {
                match p.prop {
                    Prop::Scenery { kind, .. } => scenery.insert(kind),
                    Prop::Container { style, .. } => styles.insert(style),
                };
            }
        }
        assert_eq!(scenery.len(), Scenery::ALL.len(), "{scenery:?}");
        assert_eq!(styles.len(), STYLES.len(), "{styles:?}");
    }

    #[test]
    fn props_stand_in_rooms_against_walls_off_doorways_and_the_way_in() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let way_in = entrance_path(&level);
            for p in &level.props {
                let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
                assert_eq!(level.cell(x, y), Cell::Floor, "seed {seed}: {p:?}");
                assert!(level.room_at(x, y).is_some(), "seed {seed}: {p:?} stands in a corridor");
                assert!(!by_door(&level, x, y), "seed {seed}: {p:?} stands by a doorway");
                assert!(!way_in.contains(&(x, y)), "seed {seed}: {p:?} stands in the way in");
                // Off every wall of its cell by its extent toward it and the walls' relief.
                let shape = p.prop.floor_shape();
                let (hw, hd) = shape.half_extents();
                let front = (p.yaw.sin(), p.yaw.cos());
                for facing in Facing::ALL {
                    let (dx, dy) = facing.step();
                    if level.is_open(x + dx, y + dy) {
                        continue;
                    }
                    let toward = dx as f32 * front.0 + dy as f32 * front.1;
                    let behind = if toward < -0.5 { p.prop.room_behind() } else { 0.0 };
                    let extent = match shape {
                        FloorShape::Round(r) => r,
                        _ => toward.abs() * (hd + behind) + (1.0 - toward * toward).sqrt() * hw,
                    };
                    let edge = match facing {
                        Facing::North => p.y - y as f32,
                        Facing::South => y as f32 + 1.0 - p.y,
                        Facing::West => p.x - x as f32,
                        Facing::East => x as f32 + 1.0 - p.x,
                    } * CELL;
                    assert!(edge + 0.4 >= extent + WALL_GAP, "seed {seed}: {p:?} is {edge} from the {facing:?} wall, needs {}", extent + WALL_GAP);
                }
            }
        }
    }

    #[test]
    fn every_open_cell_stays_reachable_with_props_in_the_way() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            assert!(open_cells_reachable(&level, &occupied(&level)), "seed {seed}");
        }
    }

    #[test]
    fn props_keep_clear_of_each_other_and_the_rest() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for (i, a) in level.props.iter().enumerate() {
                let ra = a.prop.floor_shape().radius();
                for b in &level.props[i + 1..] {
                    let d = (a.x - b.x).hypot(a.y - b.y) * CELL;
                    assert!(d >= ra + b.prop.floor_shape().radius(), "seed {seed}: {a:?} and {b:?}");
                }
                let others = level.items.iter().map(|s| (s.x, s.y)).chain(level.lights.iter().map(|l| (l.x, l.y))).chain(level.folk.iter().map(|f| (f.x, f.y)));
                for (x, y) in others {
                    assert!((a.x - x).hypot(a.y - y) * CELL >= ra + CLEARANCE, "seed {seed}: {a:?} on ({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn props_do_not_move_walls_items_lights_or_folk() {
        // Seed 1207 as it was before props came (`print_level 1207`).
        let level = generate(1207, 1);
        let (sx, sy, facing) = level.start();
        assert_eq!((sx, sy, facing, level.rooms.len()), (1, 22, Facing::East, 6));
        assert_eq!((level.items.len(), level.folk.len()), (13, 6));
        assert_eq!((level.items[0].family, level.items[0].seed), (armory::Family::Torch, 3_185_309_893));
        assert_eq!((level.items[12].family, level.items[12].seed), (armory::Family::Mace, 3_024_996_869));
        assert_eq!(level.folk[5].seed, 4_135_868_862);
        assert!((level.folk[5].x - 12.5).abs() < 0.05 && (level.folk[5].y - 24.6).abs() < 0.05);
        // And everything but the props is the level as laid out before they are placed.
        let (before, _) = crate::generate::generate_without_props(1207, 1, &crate::LevelParams::default());
        assert!(before.props.is_empty() && !level.props.is_empty());
        assert_eq!(Level { props: Vec::new(), creatures: Vec::new(), stairs: Vec::new(), webs: Vec::new(), ..level }, before);
    }

    #[test]
    fn a_chest_on_1207_can_be_reached_from_the_start() {
        let level = generate(1207, 1);
        let (x, y, _) = level.start();
        let seen = crate::generate::reachable_from(&level, x as i32, y as i32);
        let chest = level.props.iter().find(|p| matches!(p.prop, Prop::Container { style: ChestStyle::Chest, .. })).expect("a chest on 1207");
        assert!(seen[cell_index(&level, chest.x, chest.y)]);
    }

    #[test]
    fn containers_stay_inside_their_floor_shapes() {
        for style in ChestStyle::ALL {
            let shape = Prop::Container { style, seed: 0, loot: 0 }.floor_shape();
            let (hw, hd) = shape.half_extents();
            for seed in 0..48 {
                // Opened as it opens in the level, at its largest and widest.
                let mut state = chest_state(style, seed, true);
                state.chest.size = 1.15;
                state.chest.shape = 1.0;
                let (lo, hi) = Chests::build(&state).bounds().unwrap();
                let (lo, hi) = (lo.map(|v| v * CONTAINER_SCALE), hi.map(|v| v * CONTAINER_SCALE));
                let behind = Prop::Container { style, seed, loot: 0 }.room_behind();
                match shape {
                    FloorShape::Round(r) => {
                        let reach = lo[0].abs().max(hi[0]).max(lo[2].abs()).max(hi[2]);
                        assert!(reach <= r, "{style:?} {seed}: {reach} > {r}");
                    }
                    FloorShape::Square { .. } => {
                        assert!(lo[0] >= -hw && hi[0] <= hw && hi[2] <= hd, "{style:?} {seed}: {lo:?} {hi:?}");
                        assert!(lo[2] >= -hd - behind, "{style:?} {seed}: the lid swings to {}", lo[2]);
                    }
                }
            }
        }
    }
}
