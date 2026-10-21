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
use armory::families::campfire::{self, Campfire, CampfireState, FireForm};
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
    /// (`Chests::roll_as`), holding what `loot` rolls, or nothing if `loot` is [`NO_LOOT`].
    Container { style: ChestStyle, seed: u32, loot: u32 },
    /// Scenery built from `seed` (`armory::build::scenery`).
    Scenery { kind: Scenery, seed: u32 },
    /// A hearth's campfire: the Armory campfire family's set form, lit
    /// ([`campfire_state`] of `seed`). It stands at its [`crate::LightKind::Campfire`].
    Campfire { seed: u32 },
}

/// The `loot` of a container with nothing in it: the stalls of a market, which is set dressing
/// (its traders sell from their own shelves). Whoever stocks a container skips this seed.
pub const NO_LOOT: u32 = u32::MAX;

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

/// The floor a campfire covers, in world units: its ring of stones and the feet of the rig
/// at the largest [`campfire_state`] draws (a big shared fire, its ring about ten units across). A test holds every seed's mesh inside it.
pub const CAMPFIRE_RADIUS: f32 = 7.3;

/// The Armory state of the campfire of `seed`: the set form, lit, at the larger sizes (it
/// is the hearth's fire), with the rig and pot the family rolls from the seed: a tripod or a
/// hook with a pot, or just the fire.
pub fn campfire_state(seed: u32) -> CampfireState {
    let mut state = Campfire::roll_as(seed, FireForm::Set);
    state.fire.lit = true;
    state.fire.size = Mulberry32::new(seed ^ 0xF1AE_5123).range_f32(1.9, 2.1);
    state.fit(&campfire::LIMITS);
    state
}

/// The seed of the campfire lit in cell `(x, y)` of the level of `level_seed`: the light and
/// the prop both take it, so they agree on the fire without either keeping it.
pub fn campfire_seed(level_seed: u32, x: f32, y: f32) -> u32 {
    level_seed.wrapping_mul(0x9E37_79B1) ^ ((x.floor() as u32) << 16 | y.floor() as u32).wrapping_mul(0x85EB_CA6B)
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
            // A bedroll lies along item X, human length.
            Prop::Scenery { kind: Scenery::Bedroll, .. } => FloorShape::Square { half_width: 8.5, half_depth: 3.0 },
            Prop::Scenery { kind, .. } => FloorShape::Round(kind.radius()),
            Prop::Campfire { .. } => FloorShape::Round(CAMPFIRE_RADIUS),
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

    /// Whether it is a container with something to find in it: markets' stalls are empty
    /// ([`NO_LOOT`]).
    pub fn holds_loot(self) -> bool {
        matches!(self, Prop::Container { loot, .. } if loot != NO_LOOT)
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
    // The rooms factions hold are furnished for what they are used for, first; the rest of the
    // level gets props as it always did.
    furnish_roles(level, &keep_clear, &mut placed, &mut occupied, &mut Mulberry32::new(level.seed ^ ROLE_PROP_SALT));
    for (r, room) in level.rooms.iter().enumerate() {
        if rng.chance(BARE_ROOM) || !level.roles_of(r).is_empty() {
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
    if !placed.iter().any(|p| p.prop.holds_loot()) {
        let prop = roll_prop(PropKind::Container, &placed, rng);
        let spot = level.rooms.iter().flat_map(|room| nooks(level, room, &keep_clear)).find_map(|nook| fit(level, &nook, prop, &placed, &occupied, rng));
        placed.extend(spot);
    }
    placed
}

/// Salt for the stream that furnishes the rooms of claims.
const ROLE_PROP_SALT: u32 = 0xF0E5_B0D5;
/// Most bedrolls in a sleeping room.
const MOST_BEDROLLS: usize = 6;
/// Share of a faction's members that have a bedroll, when the room has the walls for them.
const BEDROLL_SHARE: f32 = 0.75;
/// What a market sets out as stalls, and what a store lays by, by weight.
const MARKET_STYLES: &[(ChestStyle, u32)] = &[(ChestStyle::Crate, 4), (ChestStyle::Chest, 3), (ChestStyle::Barrel, 3), (ChestStyle::Amphora, 2), (ChestStyle::Coffer, 1)];
const STORE_STYLES: &[(ChestStyle, u32)] = &[(ChestStyle::Crate, 4), (ChestStyle::Chest, 3), (ChestStyle::Barrel, 2)];
/// How close a light is to a prop that stands on it to count as its own, in cells.
const OWN_LIGHT: f32 = 0.3;

/// Furnishes the rooms of claims by their roles ([`crate::RoomRole`]): a campfire
/// (a tripod or hook and a pot, or just the fire) at the hearth's light and a keg beside it, a bedroll for most of the faction lying along a wall
/// of the sleeping room, empty stalls of crates and chests in the market (traders sell from
/// their own shelves) and stocked ones in the stores. The hall's furnishing is its brazier,
/// lit with the roles. Nothing stands in or beside a doorway, on the way in or in the way of
/// anyone standing, and every open cell stays reachable.
fn furnish_roles(level: &Level, keep_clear: &[(i32, i32)], placed: &mut Vec<PropSpot>, occupied: &mut [bool], rng: &mut Mulberry32) {
    use crate::RoomRole;
    for claim in &level.claims {
        let members = level.folk.iter().filter(|f| f.faction == claim.faction).count();
        for &r in &claim.rooms {
            let room = &level.rooms[r];
            let mut nooks = nooks(level, room, keep_clear);
            shuffle(&mut nooks, rng);
            // The fire first, whatever else the room is for: it stands on its light.
            if let Some(fire) = level.lights.iter().find(|l| l.kind == crate::LightKind::Campfire && room.contains(l.x as i32, l.y as i32)) {
                let prop = Prop::Campfire { seed: campfire_seed(level.seed, fire.x, fire.y) };
                if let Some(spot) = fit_on_light(level, keep_clear, (fire.x, fire.y), prop, placed, occupied) {
                    occupied[cell_index(level, spot.x, spot.y)] = true;
                    placed.push(spot);
                }
            }
            for &role in level.roles_of(r) {
                match role {
                    RoomRole::Hall => {}
                    RoomRole::Hearth => {
                        let keg = roll_container(placed, &[(ChestStyle::Barrel, 2), (ChestStyle::Crate, 1)], true, rng);
                        put_in_nook(level, &nooks, keg, placed, occupied, rng);
                    }
                    RoomRole::Sleeping => {
                        let beds = ((members as f32 * BEDROLL_SHARE).ceil() as usize).clamp(1, MOST_BEDROLLS);
                        for _ in 0..beds {
                            let bed = Prop::Scenery { kind: Scenery::Bedroll, seed: item_seed(rng) };
                            let Some(spot) = fit_row(level, &nooks, keep_clear, bed, placed, occupied) else { break };
                            placed.push(spot);
                        }
                    }
                    RoomRole::Market | RoomRole::Store => {
                        let market = role == RoomRole::Market;
                        let (styles, base) = if market { (MARKET_STYLES, 3) } else { (STORE_STYLES, 2) };
                        let count = (base + room.area() / 12).min(base + 2);
                        for _ in 0..count {
                            let prop = roll_container(placed, styles, market, rng);
                            put_in_nook(level, &nooks, prop, placed, occupied, rng);
                        }
                    }
                }
            }
        }
    }
}

/// A container of a style drawn from `styles`, holding nothing if `empty`. A level's first
/// loot container is a chest, as ever.
fn roll_container(placed: &[PropSpot], styles: &[(ChestStyle, u32)], empty: bool, rng: &mut Mulberry32) -> Prop {
    let first = !empty && !placed.iter().any(|p| p.prop.holds_loot());
    let style = if first { ChestStyle::Chest } else { *rng.weighted(styles) };
    let (seed, loot) = (item_seed(rng), item_seed(rng));
    Prop::Container { style, seed, loot: if empty { NO_LOOT } else { loot } }
}

/// Sets `prop` in the first of `nooks` it fits.
fn put_in_nook(level: &Level, nooks: &[Nook], prop: Prop, placed: &mut Vec<PropSpot>, occupied: &mut [bool], rng: &mut Mulberry32) {
    if let Some(spot) = nooks.iter().find_map(|nook| fit(level, nook, prop, placed, occupied, rng)) {
        occupied[cell_index(level, spot.x, spot.y)] = true;
        placed.push(spot);
    }
}

/// Whether something at `(px, py)` is nearer to `spot` than `gap` world units from its edge,
/// counting the spot as a circle of its floor shape's radius.
fn near_spot(spot: &PropSpot, px: f32, py: f32, gap: f32) -> bool {
    (px - spot.x).hypot(py - spot.y) * CELL < spot.prop.floor_shape().radius() + gap
}

/// Whether `spot` stands clear of the level's items, lights and folk, and of `placed`.
fn clear_of_all(level: &Level, spot: &PropSpot, placed: &[PropSpot]) -> bool {
    let reach = spot.prop.floor_shape().radius();
    !(level.items.iter().any(|i| near_spot(spot, i.x, i.y, CLEARANCE))
        || level.lights.iter().any(|l| near_spot(spot, l.x, l.y, CLEARANCE))
        || level.folk.iter().any(|f| near_spot(spot, f.x, f.y, CLEARANCE))
        || placed.iter().any(|p| (p.x - spot.x).hypot(p.y - spot.y) * CELL < reach + p.prop.floor_shape().radius() + PROP_GAP))
}

/// A bedroll lying along a wall of the room across two cells, if it fits: against the wall
/// at the line between two neighbouring cells along it, both floor, free, off doorways and
/// the way in, backed by rock, clear of everything, with the open cells still reachable.
/// Marks both cells in `occupied`.
fn fit_row(level: &Level, nooks: &[Nook], keep_clear: &[(i32, i32)], prop: Prop, placed: &[PropSpot], occupied: &mut [bool]) -> Option<PropSpot> {
    let (_, half_depth) = prop.floor_shape().half_extents();
    let out = (half_depth + WALL_GAP) / CELL;
    let free = |x: i32, y: i32, back: (i32, i32), wall: Facing, occupied: &[bool]| {
        level.cell(x, y) == Cell::Floor
            && !behind_a_gap(level, x, y, &[wall])
            && level.cell(x + back.0, y + back.1) == Cell::Rock
            && !occupied[(y as u32 * level.width + x as u32) as usize]
            && !by_door(level, x, y)
            && !keep_clear.contains(&(x, y))
    };
    for nook in nooks {
        for &wall in &nook.walls {
            let (bx, by) = wall.step();
            for (ax, ay) in [(-by, bx), (by, -bx)] {
                let (x2, y2) = (nook.x + ax, nook.y + ay);
                if !free(nook.x, nook.y, (bx, by), wall, occupied) || !free(x2, y2, (bx, by), wall, occupied) {
                    continue;
                }
                let x = nook.x as f32 + 0.5 + ax as f32 * 0.5 + bx as f32 * (0.5 - out);
                let y = nook.y as f32 + 0.5 + ay as f32 * 0.5 + by as f32 * (0.5 - out);
                let yaw = (-bx as f32).atan2(-by as f32);
                let spot = PropSpot { x, y, yaw, prop };
                if !clear_of_all(level, &spot, placed) {
                    continue;
                }
                let mut marked = occupied.to_vec();
                marked[cell_index(level, nook.x as f32, nook.y as f32)] = true;
                marked[cell_index(level, x2 as f32, y2 as f32)] = true;
                if open_cells_reachable(level, &marked) {
                    occupied.copy_from_slice(&marked);
                    return Some(spot);
                }
            }
        }
    }
    None
}

/// A prop set on the light at `at` (grid coordinates), if it fits: on floor off doorways, the
/// way in and the gaps between columns, clear of items, folk and `placed` (the light it stands on is the
/// point), with the open cells still reachable.
fn fit_on_light(level: &Level, keep_clear: &[(i32, i32)], at: (f32, f32), prop: Prop, placed: &[PropSpot], occupied: &[bool]) -> Option<PropSpot> {
    let (x, y) = at;
    let (cx, cy) = (x.floor() as i32, y.floor() as i32);
    if level.cell(cx, cy) != Cell::Floor || by_door(level, cx, cy) || keep_clear.contains(&(cx, cy)) || occupied[cell_index(level, x, y)] {
        return None;
    }
    if behind_a_gap(level, cx, cy, &Facing::ALL) {
        return None;
    }
    let turn = match prop {
        Prop::Campfire { seed } => seed as f32 / u32::MAX as f32 * std::f32::consts::TAU,
        _ => 0.0,
    };
    let spot = PropSpot { x, y, yaw: turn, prop };
    // Any light but the one it stands on (the others are lights of the room standing close).
    if level.items.iter().any(|i| near_spot(&spot, i.x, i.y, CLEARANCE))
        || level.lights.iter().filter(|l| (l.x - x).hypot(l.y - y) > OWN_LIGHT).any(|l| near_spot(&spot, l.x, l.y, CLEARANCE))
        || level.folk.iter().any(|f| near_spot(&spot, f.x, f.y, 0.0))
        || placed.iter().any(|p| near_spot(&spot, p.x, p.y, p.prop.floor_shape().radius() + PROP_GAP))
    {
        return None;
    }
    let mut occupied = occupied.to_vec();
    occupied[cell_index(level, x, y)] = true;
    open_cells_reachable(level, &occupied).then_some(spot)
}

fn roll_prop(kind: PropKind, placed: &[PropSpot], rng: &mut Mulberry32) -> Prop {
    match kind {
        PropKind::Container => {
            let first = !placed.iter().any(|p| p.prop.holds_loot());
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
pub(crate) fn entrance_path(level: &Level) -> Vec<(i32, i32)> {
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
pub(crate) fn behind_a_gap(level: &Level, x: i32, y: i32, walls: &[Facing]) -> bool {
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
                    Prop::Campfire { .. } => true,
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
                    // A campfire stands on its own light.
                    if matches!(a.prop, Prop::Campfire { .. }) && (a.x - x).hypot(a.y - y) <= OWN_LIGHT {
                        continue;
                    }
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
        // Folk stand by their rooms' roles since #546, guards at their doors and traders in the market since #550.
        assert_eq!(level.folk[5].seed, 3_668_467_538);
        // Gathered round the hearth's campfire since #569.
        assert!((level.folk[5].x - 11.55).abs() < 0.06 && (level.folk[5].y - 11.33).abs() < 0.06);
        // And everything but the props is the level as laid out before they are placed.
        let (before, _) = crate::generate::generate_without_props(1207, 1, &crate::LevelParams::default());
        assert!(before.props.is_empty() && !level.props.is_empty());
        assert_eq!(Level { props: Vec::new(), banners: Vec::new(), creatures: Vec::new(), stairs: Vec::new(), webs: Vec::new(), ..level }, before);
    }

    /// Props of `kind` standing in `room`.
    fn scenery_in(level: &Level, room: usize, kind: Scenery) -> usize {
        level.props.iter().filter(|p| matches!(p.prop, Prop::Scenery { kind: k, .. } if k == kind) && level.room_at(p.x as i32, p.y as i32) == Some(room)).count()
    }

    #[test]
    fn a_sleeping_room_has_a_bedroll_for_most_of_its_members() {
        let (mut members_all, mut beds_all, mut bare, mut claims) = (0, 0, 0, 0);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for claim in &level.claims {
                let members = level.folk.iter().filter(|f| f.faction == claim.faction).count();
                let room = level.room_with_role(claim, crate::RoomRole::Sleeping).unwrap();
                let beds = scenery_in(&level, room, Scenery::Bedroll);
                // A cramped room, its fire and its folk in the way, has no wall cell left to lay one in.
                bare += (beds == 0) as u32;
                claims += 1;
                assert!(beds <= members.max(1), "seed {seed} depth {depth}: {beds} bedrolls for {members} folk");
                members_all += members;
                beds_all += beds;
            }
        }
        assert!(beds_all * 3 >= members_all * 2, "{beds_all} bedrolls for {members_all} folk");
        // Measured: 29 of 313 (9.3%); 20 of 311 before the hearth's campfire (#569). Every one of
        // them is a one-room claim, hall, hearth and sleeping room at once, where the hall's
        // brazier and the hearth's campfire stand apart and leave no wall to lay a bedroll along.
        assert!(bare * 32 <= claims * 3, "{bare} of {claims} sleeping rooms have no bedroll");
    }

    #[test]
    fn markets_set_out_stalls_and_stores_stock() {
        let (mut markets, mut bare_markets, mut stores) = (0, 0, 0);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for claim in &level.claims {
                let containers = |room: usize| level.props.iter().filter(|p| p.prop.is_container() && level.room_at(p.x as i32, p.y as i32) == Some(room)).collect::<Vec<_>>();
                if let Some(room) = level.room_with_role(claim, crate::RoomRole::Market) {
                    markets += 1;
                    // Set dressing: traders sell from their own shelves. A market doubled up in
                    // a small hall may have no wall left to set a stall against.
                    bare_markets += containers(room).is_empty() as u32;
                    assert!(containers(room).iter().all(|p| !p.prop.holds_loot()), "seed {seed} depth {depth}: a stocked stall");
                }
                if let Some(room) = level.room_with_role(claim, crate::RoomRole::Store) {
                    stores += 1;
                    assert!(!containers(room).is_empty(), "seed {seed} depth {depth}: a store with no stock");
                    assert!(containers(room).iter().all(|p| p.prop.holds_loot()), "seed {seed} depth {depth}: an empty store shelf");
                }
            }
        }
        // Claims are seldom big enough for a spare room, so stores are rare.
        assert!(markets > 8, "{markets} markets, {stores} stores");
        // Measured: every one of the sweep's markets has stalls.
        assert_eq!(bare_markets, 0, "{bare_markets} of {markets} markets have no stalls");
    }

    #[test]
    fn every_hearth_has_one_lit_campfire_and_no_brazier() {
        use crate::{LightKind, RoomRole};
        let (mut hearths, mut with_rig, mut with_pot, mut bare) = (0, 0, 0, 0);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for claim in &level.claims {
                let r = level.room_with_role(claim, RoomRole::Hearth).unwrap();
                let room = level.rooms[r];
                let fires: Vec<_> = level.props.iter().filter(|p| matches!(p.prop, Prop::Campfire { .. }) && room.contains(p.x as i32, p.y as i32)).collect();
                assert_eq!(fires.len(), 1, "seed {seed} depth {depth}: {} campfires in the hearth", fires.len());
                let lights: Vec<_> = level.lights.iter().filter(|l| l.kind == LightKind::Campfire && room.contains(l.x as i32, l.y as i32)).collect();
                assert_eq!(lights.len(), 1, "seed {seed} depth {depth}: {} campfire lights", lights.len());
                assert!((lights[0].x - fires[0].x).hypot(lights[0].y - fires[0].y) <= OWN_LIGHT, "seed {seed} depth {depth}: the light is not at the fire");
                if !level.roles_of(r).contains(&RoomRole::Hall) {
                    assert!(!level.lights.iter().any(|l| l.kind == LightKind::Brazier && room.contains(l.x as i32, l.y as i32)), "seed {seed} depth {depth}: a brazier in the hearth");
                }
                let Prop::Campfire { seed: fire } = fires[0].prop else { unreachable!() };
                let state = campfire_state(fire);
                assert!(state.fire.lit && state.fire.form == FireForm::Set);
                hearths += 1;
                with_rig += (state.fire.rig != campfire::Rig::Bare) as u32;
                with_pot += (state.fire.rig != campfire::Rig::Bare && state.fire.pot) as u32;
                bare += (state.fire.rig == campfire::Rig::Bare) as u32;
            }
        }
        assert!(hearths > 40 && with_rig > 0 && with_pot > 0 && bare > 0, "{hearths} hearths, {with_rig} rigged, {with_pot} potted, {bare} bare");
    }

    #[test]
    fn every_campfire_light_has_its_prop_and_every_hearth_is_lit() {
        use crate::{LightKind, RoomRole};
        let (mut hearths, mut brazier_hearths) = (0, 0);
        for depth in 1..=4 {
            for seed in 1..=100 {
                let level = generate(seed, depth);
                for l in level.lights.iter().filter(|l| l.kind == LightKind::Campfire) {
                    let props = level.props.iter().filter(|p| matches!(p.prop, Prop::Campfire { .. }) && (p.x - l.x).hypot(p.y - l.y) <= OWN_LIGHT).count();
                    assert_eq!(props, 1, "seed {seed} depth {depth}: a campfire light at ({}, {}) with {props} fires", l.x, l.y);
                }
                for p in level.props.iter().filter(|p| matches!(p.prop, Prop::Campfire { .. })) {
                    assert!(level.lights.iter().any(|l| l.kind == LightKind::Campfire && (p.x - l.x).hypot(p.y - l.y) <= OWN_LIGHT), "seed {seed} depth {depth}: a fire with no light");
                }
                for claim in &level.claims {
                    let room = level.rooms[level.room_with_role(claim, RoomRole::Hearth).unwrap()];
                    hearths += 1;
                    let campfire = level.lights.iter().any(|l| l.kind == LightKind::Campfire && room.contains(l.x as i32, l.y as i32));
                    let brazier = level.lights.iter().any(|l| l.kind == LightKind::Brazier && room.contains(l.x as i32, l.y as i32));
                    assert!(campfire || brazier, "seed {seed} depth {depth}: a dark hearth");
                    brazier_hearths += (!campfire) as u32;
                }
            }
        }
        // A hearth with no room for a fire falls back to a brazier; it is rare.
        assert!(brazier_hearths * 50 <= hearths, "{brazier_hearths} of {hearths} hearths have only a brazier");
    }

    #[test]
    fn braziers_stand_alone_and_halls_keep_theirs() {
        use crate::{LightKind, RoomRole};
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for brazier in level.lights.iter().filter(|l| l.kind == LightKind::Brazier) {
                for p in &level.props {
                    let gap = (p.x - brazier.x).hypot(p.y - brazier.y) * CELL;
                    assert!(gap > p.prop.floor_shape().radius(), "seed {seed} depth {depth}: {p:?} sits over or touches the brazier at ({}, {})", brazier.x, brazier.y);
                }
            }
            for claim in &level.claims {
                let room = level.rooms[level.room_with_role(claim, RoomRole::Hall).unwrap()];
                assert!(level.lights.iter().any(|l| l.kind == LightKind::Brazier && room.contains(l.x as i32, l.y as i32)), "seed {seed} depth {depth}: a hall without its brazier");
            }
        }
    }

    #[test]
    fn campfires_stay_inside_their_floor_shape() {
        for seed in 0..96 {
            let state = campfire_state(seed);
            assert_eq!(state, campfire_state(seed));
            let mesh = armory::Item::Campfire(state).build();
            let reach = armory::build::scenery::floor_reach(&mesh);
            assert!(reach <= CAMPFIRE_RADIUS, "seed {seed}: reaches {reach} > {CAMPFIRE_RADIUS}");
            assert!(armory::families::campfire::Campfire::light(&campfire_state(seed)).is_some());
        }
    }

    #[test]
    fn rooms_no_one_holds_get_no_role_props() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for p in &level.props {
                let room = level.room_at(p.x as i32, p.y as i32).unwrap();
                if matches!(p.prop, Prop::Scenery { kind: Scenery::Bedroll, .. } | Prop::Campfire { .. }) {
                    assert!(level.is_claimed(room), "seed {seed} depth {depth}: {p:?} in a wild room");
                }
                // Claimed rooms get only what their roles furnish them with: no rubble, bones or pillars.
                if level.is_claimed(room) {
                    assert!(!matches!(p.prop, Prop::Scenery { kind: Scenery::Rubble | Scenery::Bones | Scenery::Pillar, .. }), "seed {seed} depth {depth}: {p:?} in a claim");
                }
            }
        }
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
