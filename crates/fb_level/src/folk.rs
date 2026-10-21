//! Folk met in the level: who stands where, in groups by faction.
//!
//! The factions come from the world's pool (`fb_faction::World`, rolled from the world seed): a
//! level holds the two to four its seed draws ([`crate::Level::factions`]), and each group is
//! one of them. A faction's folk leans its members (an insular one is wholly its own folk).
//!
//! Folk come from a third stream of the level seed, after the layout and the furnishing, so
//! adding or changing folk never moves a wall, an item or a light.

use fb_body::Folk;
use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::claims::MIN_ROOM_AREA;
use crate::generate::{is_colonnaded, item_seed, shuffle};
use crate::rooms::RoomGraph;
use crate::{Cell, Claim, Level, Room, RoomRole};
pub use fb_faction::{Faction, FactionSpec, Profession, Trade};

/// Someone standing in the level, at a point in grid coordinates (cells, fractional).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FolkSpot {
    pub x: f32,
    pub y: f32,
    /// The way they face, radians about +Y, as [`crate::Facing::yaw`]: 0 faces north (-y), a
    /// quarter turn faces west.
    pub yaw: f32,
    pub folk: Folk,
    /// Rolls their body, name and outfit.
    pub seed: u32,
    pub faction: Faction,
    /// Their profession, tier and style, if they have one ([`crate::trades`]).
    #[serde(default)]
    pub trade: Option<Trade>,
}

impl FolkSpot {
    /// The unit grid direction they face, `(dx, dy)` with `y` south.
    pub fn facing(&self) -> (f32, f32) {
        (-self.yaw.sin(), -self.yaw.cos())
    }
}

/// Salt for the folk stream.
pub(crate) const FOLK_SALT: u32 = 0xF01C_5A17;
/// Folk in a level, inclusive.
pub const FOLK_COUNT: (u32, u32) = (3, 10);
/// Rings a group tries, each [`RING_STEP`] cells wider than the last.
const RINGS: usize = 4;
const RING_STEP: f32 = 0.5;
/// Folk in one group, inclusive.
const GROUP_SIZE: (u32, u32) = (2, 4);
/// Least distance between two folk, in cells (1.4 m).
const FOLK_SPACING: f32 = 0.7;
/// Least distance from a folk to an item or a light, in cells, so nobody stands in a brazier
/// or on the loot.
const CLEARANCE: f32 = 0.55;
/// Least distance from a folk to a room's wall, in cells.
const WALL_MARGIN: f32 = 0.3;

/// Lays out the level's folk by what they do. `plans[k]` is claim `k`'s camp, one entry per
/// member it places (`strengths[k]`; [`crate::trades::plan`]).
///
/// - **Guards** stand at the claim's hall door, on the floor cell just inside its outward
///   opening and facing out; a second guard at another outward doorway of the claim, or else
///   beside the first ([`posts`]).
/// - **Traders** stand in the market, gathered as if talking, and fall back to the claim's
///   other rooms where the market is too cramped.
/// - **The rest** gather round the hearth, then the hall and the sleeping room, in groups
///   facing a point (a brazier where the room has one).
///
/// Never in the starting room (no claim holds it), a corridor, or beside a doorway (the guards'
/// posts apart). A room too cramped for a whole group takes what fits, and later passes top up
/// the groups already placed. Whoever does not fit is dropped, plain folk first.
pub(crate) fn place_folk(level: &Level, strengths: &[usize], plans: &[Vec<Option<Trade>>], rng: &mut Mulberry32) -> Vec<FolkSpot> {
    let lit = |i: &usize| level.lights.iter().any(|l| level.room_at(l.x as i32, l.y as i32) == Some(*i));
    let graph = RoomGraph::new(level);
    let (sx, sy, _) = level.start();
    let mut folk: Vec<FolkSpot> = Vec::new();
    for ((claim, &strength), plan) in level.claims.iter().zip(strengths).zip(plans) {
        let faction = level.faction(claim.faction);
        let is_guard = |t: &Option<Trade>| t.is_some_and(|t| t.profession == Profession::Guard);
        let (guards, rest): (Vec<_>, Vec<_>) = plan.iter().copied().take(strength).partition(is_guard);
        let (traders, plain): (Vec<_>, Vec<_>) = rest.into_iter().partition(|t| t.is_some());

        // Guards first, at their posts.
        let mut at_post = 0;
        for (x, y, yaw) in posts(level, &graph, claim, (sx as i32, sy as i32), guards.len()) {
            let folk_kind = faction.member_folk(rng);
            folk.push(FolkSpot { x, y, yaw, folk: folk_kind, seed: item_seed(rng), faction: faction.id, trade: Some(Trade::guard()) });
            at_post += 1;
        }

        let mut rooms: Vec<usize> = claim.rooms.iter().copied().filter(|&i| level.rooms[i].area() >= MIN_ROOM_AREA).collect();
        shuffle(&mut rooms, rng);
        // Where the rest stand follows their room's role. Lit rooms first within a role: people
        // gather at the light.
        let rank = |i: &usize| {
            let roles = level.roles_of(*i);
            [RoomRole::Hearth, RoomRole::Hall, RoomRole::Sleeping, RoomRole::Market].iter().position(|r| roles.contains(r)).unwrap_or(4)
        };
        if level.style == crate::LevelStyle::StoneHalls {
            // Folk of the halls stand among the columns first.
            rooms.sort_by_key(|i| (!is_colonnaded(level, &level.rooms[*i]), rank(i), !lit(i)));
        } else {
            rooms.sort_by_key(|i| (rank(i), !lit(i)));
        }
        let groups: Vec<(usize, Gathering)> = rooms.iter().map(|&i| (i, Gathering::in_room(level, &level.rooms[i], rng))).collect();
        let markets: Vec<(usize, Gathering)> = groups.iter().copied().filter(|&(i, _)| level.roles_of(i).contains(&RoomRole::Market)).collect();

        // Guards a claim had no post for gather with the rest.
        let spare_guards = guards.len() - at_post;
        let start = folk.len();
        fill(level, faction, spare_guards, &groups, &mut folk, rng);
        for f in &mut folk[start..] {
            f.trade = Some(Trade::guard());
        }
        // Traders in the market, then wherever there is room.
        let start = folk.len();
        let first = fill(level, faction, traders.len(), &markets, &mut folk, rng);
        fill(level, faction, traders.len() - first, &groups, &mut folk, rng);
        for (f, t) in folk[start..].iter_mut().zip(&traders) {
            f.trade = *t;
        }
        fill(level, faction, plain.len(), &groups, &mut folk, rng);
        // A claim too cramped for anyone to gather or stand at a post still holds its first
        // member, on the first free floor cell (off the doorways if it can be).
        if strength > 0 && !folk.iter().any(|f| f.faction == faction.id) {
            let cells: Vec<(i32, i32)> = claim.rooms.iter().flat_map(|&r| (level.rooms[r].y as i32..(level.rooms[r].y + level.rooms[r].h) as i32).flat_map(move |y| (level.rooms[r].x as i32..(level.rooms[r].x + level.rooms[r].w) as i32).map(move |x| (x, y)))).filter(|&(x, y)| level.cell(x, y) == Cell::Floor).collect();
            let free = |&&(x, y): &&(i32, i32)| folk.iter().all(|f| (f.x - x as f32 - 0.5).hypot(f.y - y as f32 - 0.5) >= FOLK_SPACING);
            let clear = |&&(x, y): &&(i32, i32)| level.items.iter().all(|i| (i.x - x as f32 - 0.5).hypot(i.y - y as f32 - 0.5) >= CLEARANCE) && level.lights.iter().all(|l| (l.x - x as f32 - 0.5).hypot(l.y - y as f32 - 0.5) >= CLEARANCE);
            let best = cells.iter().filter(free).find(|c| clear(c) && !by_door(level, c.0, c.1)).or_else(|| cells.iter().filter(free).find(|c| clear(c))).or_else(|| cells.iter().find(free));
            if let Some(&(x, y)) = best {
                let (cx, cy) = level.rooms[level.room_at(x, y).unwrap_or(claim.rooms[0])].centre();
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                folk.push(FolkSpot { x: px, y: py, yaw: (px - cx).atan2(py - cy), folk: faction.member_folk(rng), seed: item_seed(rng), faction: faction.id, trade: plan.first().copied().flatten() });
            }
        }
    }
    folk
}

/// Up to `count` of `faction` in the `groups`' rooms, three passes: the first in groups of two
/// to four, the others topping up one at a time. Returns how many stand.
fn fill(level: &Level, faction: &FactionSpec, count: usize, groups: &[(usize, Gathering)], folk: &mut Vec<FolkSpot>, rng: &mut Mulberry32) -> usize {
    let mut placed = 0;
    for pass in 0..3 {
        for &(i, gathering) in groups {
            let left = count.saturating_sub(placed);
            if left == 0 {
                return placed;
            }
            let size = if pass == 0 { rng.int(GROUP_SIZE.0, GROUP_SIZE.1) as usize } else { 1 }.min(left);
            let made = gathering.gather(level, &level.rooms[i], faction, size, folk, rng);
            placed += made.len();
            folk.extend(made);
        }
    }
    placed
}

/// Where `n` guards of `claim` stand, `(x, y, yaw)`. A post is a floor cell of the opening's room
/// one step along the wall from the cell behind the doorway, never on a door's axis (not the
/// doorway, not the cell behind it, so the way in stays whole for the delver and for a door's
/// swing, nor where the open leaf sweeps, [`crate::door`]), clear of items and lights, and the guard faces the doorway. The first guard takes the
/// hall's main opening (the side away from the door's hinge first); the second takes another outward opening three
/// cells or more away, else the other side of the main one; later ones step further along the
/// wall. Fewer guards are posted when no such cell is free.
pub fn posts(level: &Level, graph: &RoomGraph, claim: &Claim, from: (i32, i32), n: usize) -> Vec<(f32, f32, f32)> {
    use crate::roles::Opening;
    let openings = crate::roles::openings(level, graph, claim, from);
    let Some(&main) = openings.first() else { return Vec::new() };
    let clear = |c: (i32, i32)| {
        let (px, py) = (c.0 as f32 + 0.5, c.1 as f32 + 0.5);
        level.items.iter().all(|i| (i.x - px).hypot(i.y - py) >= CLEARANCE) && level.lights.iter().all(|l| (l.x - px).hypot(l.y - py) >= CLEARANCE)
    };
    // Every claim door's leaf: its door cell and where it hangs.
    let doors = crate::door::LeafSweep::all(level);
    // The cells along the wall from `o`'s, nearest first, that make a post.
    let beside = |o: &Opening| -> Vec<(i32, i32)> {
        // The leaf hinges on the frame's jamb on the world's `-across` side and stands open
        // through the doorway (`door::LeafSweep`, the rule the game draws by): the side away
        // from the hinge first, and the hinge side's first cell last.
        let door = doors.iter().find(|s| s.0 == o.cell || s.0 == (o.cell.0 + o.out.0, o.cell.1 + o.out.1)).map(|s| s.1);
        let mut right = (-o.out.1, o.out.0);
        if let Some(d) = door {
            if right.0 as f32 * d.across.0 + right.1 as f32 * d.across.1 < 0.0 {
                right = (-right.0, -right.1);
            }
        }
        let room = &level.rooms[o.room];
        let mut cells = Vec::new();
        for (step, sign) in [(1, 1), (2, 1), (3, 1), (1, -1), (2, -1), (3, -1)] {
            {
                let c = (o.cell.0 + right.0 * step * sign, o.cell.1 + right.1 * step * sign);
                // Nor on any door's axis, nor where any door's open leaf sweeps.
                let on_an_axis = openings.iter().any(|d| d.cell == c || (d.cell.0 + d.out.0, d.cell.1 + d.out.1) == c);
                let swept = doors.iter().any(|s| s.1.sweeps(c.0 as f32 + 0.5, c.1 as f32 + 0.5, crate::door::FOLK_RADIUS));
                if room.contains(c.0, c.1) && level.cell(c.0, c.1) == Cell::Floor && !on_an_axis && !swept && clear(c) {
                    cells.push(c);
                }
            }
        }
        cells
    };
    let stand = |c: (i32, i32), o: &Opening| {
        let (px, py) = (c.0 as f32 + 0.5, c.1 as f32 + 0.5);
        let (dx, dy) = ((o.cell.0 + o.out.0) as f32 + 0.5 - px, (o.cell.1 + o.out.1) as f32 + 0.5 - py);
        (px, py, (-dx).atan2(-dy))
    };
    let mut out: Vec<(f32, f32, f32)> = Vec::new();
    let mut taken: Vec<(i32, i32)> = Vec::new();
    let mut main_cells = beside(&main).into_iter();
    if n > 0 {
        if let Some(c) = main_cells.next() {
            out.push(stand(c, &main));
            taken.push(c);
        }
    }
    if n > 1 {
        let apart = |o: &Opening| (o.cell.0 - main.cell.0).abs().max((o.cell.1 - main.cell.1).abs()) >= 3;
        let other = openings.iter().filter(|o| apart(o)).find_map(|o| beside(o).into_iter().find(|c| !taken.contains(c)).map(|c| (c, *o)));
        match other {
            Some((c, o)) => {
                out.push(stand(c, &o));
                taken.push(c);
            }
            None => {
                if let Some(c) = main_cells.next() {
                    out.push(stand(c, &main));
                    taken.push(c);
                }
            }
        }
    }
    for c in main_cells {
        if out.len() >= n {
            break;
        }
        out.push(stand(c, &main));
    }
    out.truncate(n);
    out
}

/// Where a group stands: round a centre, at about a radius, starting at an angle.
#[derive(Clone, Copy, Debug)]
struct Gathering {
    centre: (f32, f32),
    radius: f32,
    start: f32,
}

impl Gathering {
    /// Round the room's campfire, or its brazier, if it has one, otherwise round a point well
    /// inside it.
    fn in_room(level: &Level, room: &Room, rng: &mut Mulberry32) -> Gathering {
        let in_room = |kind| level.lights.iter().find(|l| l.kind == kind && room.contains(l.x as i32, l.y as i32));
        let brazier = in_room(crate::LightKind::Campfire).or_else(|| in_room(crate::LightKind::Brazier));
        let centre = match brazier {
            Some(b) => (b.x, b.y),
            None => (rng.range_f32(room.x as f32 + 1.0, (room.x + room.w) as f32 - 1.0), rng.range_f32(room.y as f32 + 1.0, (room.y + room.h) as f32 - 1.0)),
        };
        // Wider round a fire.
        let radius = if brazier.is_some() { rng.range_f32(1.0, 1.3) } else { rng.range_f32(0.55, 0.8) };
        Gathering { centre, radius, start: rng.range_f32(0.0, std::f32::consts::TAU) }
    }

    /// Up to `size` of `faction` round the centre, clear of `others`: evenly round the ring
    /// with a little jitter, then round wider rings for those who did not fit.
    fn gather(&self, level: &Level, room: &Room, faction: &FactionSpec, size: usize, others: &[FolkSpot], rng: &mut Mulberry32) -> Vec<FolkSpot> {
        let (cx, cy) = self.centre;
        let around = size.max(3);
        let mut placed: Vec<FolkSpot> = Vec::new();
        for k in 0..around * RINGS {
            if placed.len() == size {
                break;
            }
            let ring = (k / around) as f32;
            let angle = self.start + k as f32 / around as f32 * std::f32::consts::TAU + ring * 0.5 + rng.range_f32(-0.25, 0.25);
            let r = (self.radius + ring * RING_STEP) * rng.range_f32(0.92, 1.08);
            let (x, y) = (cx + angle.cos() * r, cy + angle.sin() * r);
            if !fits(level, room, x, y, others.iter().chain(&placed)) {
                continue;
            }
            let folk = faction.member_folk(rng);
            // Facing the centre, give or take: yaw 0 faces -y, so the yaw toward `(dx, dy)` is
            // `atan2(-dx, -dy)`.
            let yaw = (x - cx).atan2(y - cy) + rng.range_f32(-0.3, 0.3);
            placed.push(FolkSpot { x, y, yaw, folk, seed: item_seed(rng), faction: faction.id, trade: None });
        }
        placed
    }
}

/// Whether someone can stand at `(x, y)` in `room`: on its floor, off its walls, not in or
/// beside a doorway, and clear of items, lights and `others`.
fn fits<'a>(level: &Level, room: &Room, x: f32, y: f32, mut others: impl Iterator<Item = &'a FolkSpot>) -> bool {
    let inside = x >= room.x as f32 + WALL_MARGIN
        && y >= room.y as f32 + WALL_MARGIN
        && x <= (room.x + room.w) as f32 - WALL_MARGIN
        && y <= (room.y + room.h) as f32 - WALL_MARGIN;
    if !inside {
        return false;
    }
    let (cx, cy) = (x.floor() as i32, y.floor() as i32);
    if level.cell(cx, cy) != Cell::Floor || by_door(level, cx, cy) {
        return false;
    }
    let near = |px: f32, py: f32, d: f32| (px - x).hypot(py - y) < d;
    !level.items.iter().any(|i| near(i.x, i.y, CLEARANCE))
        && !level.lights.iter().any(|l| near(l.x, l.y, CLEARANCE))
        && !others.any(|f| near(f.x, f.y, FOLK_SPACING))
}

/// Whether cell `(x, y)` is beside a doorway, or diagonally beside one, so nobody stands in
/// the way in or out.
pub(crate) fn by_door(level: &Level, x: i32, y: i32) -> bool {
    (-1..=1).any(|dy| (-1..=1).any(|dx| level.cell(x + dx, y + dy).is_doorway()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate;
    use crate::generate::reachable_from;
    use crate::generate_in;
    use std::collections::{HashMap, HashSet};

    const SEEDS: std::ops::RangeInclusive<u32> = 1..=64;

    /// Every seed on the first level and on the second, which a placement test holds for alike.
    fn sweep() -> impl Iterator<Item = (u32, u32)> {
        [1, 2].into_iter().flat_map(|depth| SEEDS.map(move |seed| (seed, depth)))
    }

    #[test]
    fn a_level_holds_two_to_four_of_its_worlds_factions() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let world = fb_faction::World::new(seed);
            let drawn = world.draw_specs(seed);
            assert!((2..=4).contains(&drawn.len()));
            // Those of the draw that found ground, in the draw's order.
            assert!(!level.factions.is_empty() && level.factions.iter().all(|s| drawn.contains(s)), "seed {seed}");
            assert!(level.factions.windows(2).all(|w| drawn.iter().position(|d| *d == w[0]) < drawn.iter().position(|d| *d == w[1])));
            for f in &level.folk {
                assert!(level.factions.iter().any(|s| s.id == f.faction), "seed {seed}: {f:?} is of a faction the level does not hold");
            }
        }
    }

    #[test]
    fn a_world_draws_the_same_spec_on_every_level() {
        let world = 1207;
        let mut seen = HashMap::new();
        for depth in 1..=12 {
            for spec in generate_in(world, depth, &crate::LevelParams::for_style(crate::LevelStyle::for_depth(depth))).factions {
                assert_eq!(*seen.entry(spec.id).or_insert_with(|| spec.clone()), spec);
            }
        }
        assert!(seen.len() > 3, "{} factions in 12 levels", seen.len());
    }

    #[test]
    fn insular_folk_are_all_their_factions_folk_on_every_level() {
        let mut insular = 0;
        for world in 1..=48 {
            for depth in 1..=3 {
                let level = generate_in(world, depth, &crate::LevelParams::for_style(crate::LevelStyle::for_depth(depth)));
                for f in &level.folk {
                    let spec = level.faction(f.faction);
                    if spec.insular {
                        insular += 1;
                        assert_eq!(f.folk, spec.folk, "world {world} depth {depth}: {} has a {:?}", spec.name, f.folk);
                    }
                }
            }
        }
        assert!(insular > 20, "only {insular} folk of insular factions");
    }

    #[test]
    fn folk_are_deterministic() {
        for seed in [1, 1207, u32::MAX] {
            assert_eq!(generate(seed, 1).folk, generate(seed, 1).folk);
        }
        assert_ne!(generate(1, 1).folk, generate(2, 1).folk);
    }

    #[test]
    fn each_level_has_three_to_ten_folk() {
        for seed in SEEDS {
            let n = generate(seed, 1).folk.len() as u32;
            // A level whose wild ground leaves a claim only a small room at its end holds fewer.
            assert!((1..=FOLK_COUNT.1).contains(&n), "seed {seed}: {n} folk");
            assert!(n >= FOLK_COUNT.0 || generate(seed, 1).rooms.len() <= 8, "seed {seed}: {n} folk");
        }
    }

    #[test]
    fn nobody_stands_in_the_starting_room_a_corridor_a_doorway_or_rock() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let start_room = level.start_room().unwrap();
            for f in &level.folk {
                let (x, y) = (f.x.floor() as i32, f.y.floor() as i32);
                assert_eq!(level.cell(x, y), Cell::Floor, "seed {seed}: {f:?} is not on floor");
                let room = level.room_at(x, y);
                assert!(room.is_some(), "seed {seed}: {f:?} stands in a corridor");
                assert_ne!(room, Some(start_room), "seed {seed}: {f:?} stands in the starting room");
                // A guard's post is the cell beside the doorway.
                assert!(f.trade.is_some_and(|t| t.profession == Profession::Guard) || !by_door(&level, x, y), "seed {seed}: {f:?} stands by a doorway");
            }
        }
    }

    #[test]
    fn folk_keep_apart_and_off_the_loot_and_lights() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for (i, a) in level.folk.iter().enumerate() {
                for b in &level.folk[i + 1..] {
                    assert!((a.x - b.x).hypot(a.y - b.y) >= FOLK_SPACING, "seed {seed}: {a:?} and {b:?}");
                }
                let spots = level.items.iter().map(|s| (s.x, s.y)).chain(level.lights.iter().map(|l| (l.x, l.y)));
                for (x, y) in spots {
                    assert!((a.x - x).hypot(a.y - y) >= CLEARANCE, "seed {seed}: {a:?} on ({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn groups_are_one_faction_to_a_room_and_mostly_its_folk() {
        let mut leaning = (0, 0);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let mut by_room = HashMap::new();
            for f in &level.folk {
                let room = level.room_at(f.x as i32, f.y as i32).unwrap();
                assert_eq!(*by_room.entry(room).or_insert(f.faction), f.faction, "seed {seed}: two factions in room {room}");
                leaning.0 += (f.folk == level.faction(f.faction).folk) as u32;
                leaning.1 += 1;
            }
        }
        assert!(leaning.0 * 10 >= leaning.1 * 6, "only {} of {} folk are their faction's folk", leaning.0, leaning.1);
    }

    #[test]
    fn levels_show_more_than_one_faction() {
        let factions: HashSet<Faction> = (1..=32).flat_map(|s| generate(s, 1).folk).map(|f| f.faction).collect();
        assert!(factions.len() >= 2, "{factions:?}");
        let folk: HashSet<Folk> = (1..=32).flat_map(|s| generate(s, 1).folk).map(|f| f.folk).collect();
        assert_eq!(folk.len(), Folk::ALL.len(), "{folk:?}");
    }

    #[test]
    fn folk_on_1207_can_be_reached_from_the_start() {
        let level = generate(1207, 1);
        let (x, y, _) = level.start();
        let seen = reachable_from(&level, x as i32, y as i32);
        assert!(!level.folk.is_empty());
        for f in &level.folk {
            let i = (f.y as u32 * level.width + f.x as u32) as usize;
            assert!(seen[i], "{f:?} is cut off");
        }
    }

    #[test]
    fn folk_do_not_move_the_loot() {
        // Seed 1207's items as they lay before folk came (`print_level 1207`).
        let level = generate(1207, 1);
        assert_eq!(level.items.len(), 13);
        assert_eq!((level.items[0].family, level.items[0].seed), (armory::Family::Torch, 3_185_309_893));
        assert_eq!((level.items[12].family, level.items[12].seed), (armory::Family::Mace, 3_024_996_869));
        assert!((level.items[12].x - 10.6).abs() < 0.05 && (level.items[12].y - 10.5).abs() < 0.05);
    }

    /// The guards of `claim`, in placement order.
    fn guards_of<'a>(level: &'a Level, claim: &Claim) -> Vec<&'a FolkSpot> {
        level.folk.iter().filter(|f| f.faction == claim.faction && f.trade.is_some_and(|t| t.profession == Profession::Guard)).collect()
    }

    /// How far from a door's axis a guard's centre must stand for the delver to pass: the
    /// delver's body radius and the widest folk footprint (world units), in cells.
    const AXIS_CLEARANCE: f32 = (3.5 + 5.0) / 20.0;

    #[test]
    fn a_guard_stands_beside_the_halls_door_facing_the_doorway() {
        let (mut claims, mut posted) = (0, 0);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let graph = RoomGraph::new(&level);
            let (sx, sy, _) = level.start();
            for claim in &level.claims {
                let guards = guards_of(&level, claim);
                if guards.is_empty() {
                    continue;
                }
                claims += 1;
                let main = crate::roles::openings(&level, &graph, claim, (sx as i32, sy as i32))[0];
                assert_eq!(level.room_with_role(claim, RoomRole::Hall), Some(main.room));
                let door = ((main.cell.0 + main.out.0) as f32 + 0.5, (main.cell.1 + main.out.1) as f32 + 0.5);
                let side = (main.out.1.abs(), main.out.0.abs());
                // A guard one to three cells along the wall from the cell behind the door.
                let beside = guards.iter().find(|g| {
                    let (cx, cy) = (g.x.floor() as i32 - main.cell.0, g.y.floor() as i32 - main.cell.1);
                    (1..=3).any(|k| [k, -k].iter().any(|&j| (cx, cy) == (side.0 * j, side.1 * j)))
                });
                let Some(g) = beside else { continue };
                let (dx, dy) = g.facing();
                let (tx, ty) = (door.0 - g.x, door.1 - g.y);
                let len = tx.hypot(ty);
                assert!((dx * tx + dy * ty) / len > 0.999, "seed {seed} depth {depth}: guard does not face the doorway");
                posted += 1;
            }
        }
        assert!(claims > 100, "{claims} guarded claims");
        // Measured: 323 of 336 guarded claims have a guard at the main door; in the other 13
        // no cell beside it is clear of loot and torches, off the doors' axes and off the hinge
        // sides (one such cell may be a cramped claim's only floor).
        assert!(posted + 13 >= claims, "{posted} of {claims}");
    }

    #[test]
    fn no_guard_stands_on_a_doors_axis_and_the_way_in_stays_wide() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let graph = RoomGraph::new(&level);
            let (sx, sy, _) = level.start();
            for claim in &level.claims {
                let openings = crate::roles::openings(&level, &graph, claim, (sx as i32, sy as i32));
                let mine = guards_of(&level, claim);
                // The first guards stand at posts; any more gather with the rest.
                let posted = posts(&level, &graph, claim, (sx as i32, sy as i32), mine.len()).len();
                for g in mine.into_iter().take(posted) {
                    let cell = (g.x.floor() as i32, g.y.floor() as i32);
                    for (cell, sweep) in crate::door::LeafSweep::all(&level) {
                        assert!(!sweep.sweeps(g.x, g.y, crate::door::FOLK_RADIUS), "seed {seed} depth {depth}: guard in the leaf of door {cell:?}");
                    }
                    for o in &openings {
                        let door = (o.cell.0 + o.out.0, o.cell.1 + o.out.1);
                        assert!(cell != o.cell && cell != door, "seed {seed} depth {depth}: guard on a door's axis at {cell:?}");
                        // On the doorway cell and the cell behind it, the guard keeps clear of the line.
                        let (cx, cy) = (o.cell.0 as f32 + 0.5, o.cell.1 as f32 + 0.5);
                        let (rx, ry) = (g.x - cx, g.y - cy);
                        let along = -(rx * o.out.0 as f32 + ry * o.out.1 as f32);
                        let across = rx * o.out.1 as f32 - ry * o.out.0 as f32;
                        if (-1.5..=0.5).contains(&along) {
                            assert!(across.abs() >= AXIS_CLEARANCE, "seed {seed} depth {depth}: guard {across} from a door's axis");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn guards_stand_clear_of_loot_and_torches_and_the_claims_post_nearly_every_guard() {
        let (mut guards, mut short_claims, mut claims, mut missing) = (0, 0, 0, 0);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let graph = RoomGraph::new(&level);
            let (sx, sy, _) = level.start();
            for claim in &level.claims {
                let mine = guards_of(&level, claim);
                if mine.is_empty() {
                    continue;
                }
                claims += 1;
                let posts = posts(&level, &graph, claim, (sx as i32, sy as i32), mine.len());
                let short = mine.len() - posts.len();
                short_claims += (short > 0) as u32;
                missing += short as u32;
                guards += mine.len() as u32;
            }
        }
        // Measured: 13 of 351 guards, in 13 of 336 claims, have no post and gather with the folk.
        assert!(short_claims <= 13 && missing <= 13 && guards > 300, "{missing} of {guards} guards without a post in {short_claims} of {claims} claims");
        assert!(missing * 100 <= guards * 4, "{missing} of {guards} guards have no post");
    }

    #[test]
    fn a_second_guard_has_another_doorway_or_stands_beside_the_first() {
        let mut pairs = 0;
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for claim in &level.claims {
                let guards = guards_of(&level, claim);
                if let [a, b, ..] = guards[..] {
                    pairs += 1;
                    assert!((a.x - b.x).hypot(a.y - b.y) >= FOLK_SPACING, "seed {seed} depth {depth}: guards on one spot");
                }
            }
        }
        assert!(pairs > 0, "no claim posted two guards");
    }

    #[test]
    fn every_trader_stands_in_their_market_where_there_is_room() {
        let (mut traders, mut in_market, mut markets, mut staffed) = (0, 0, 0, 0);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for claim in &level.claims {
                let market = level.room_with_role(claim, RoomRole::Market);
                if market.is_some() {
                    markets += 1;
                    staffed += level.folk.iter().any(|f| f.faction == claim.faction && f.trade.is_some_and(|t| t.profession != Profession::Guard) && level.room_at(f.x as i32, f.y as i32) == market) as u32;
                }
                for f in level.folk.iter().filter(|f| f.faction == claim.faction && f.trade.is_some_and(|t| t.profession != Profession::Guard)) {
                    traders += 1;
                    in_market += (market.is_some() && level.room_at(f.x as i32, f.y as i32) == market) as u32;
                }
            }
        }
        // Measured over the sweep: 256 of 259 traders stand in a market and 205 of 206 markets
        // have one (the other's room is too cramped for its two traders, who stand in the
        // claim's other rooms; `talk::lines::place_of` reads where they actually are).
        assert!(markets > 100 && staffed + 1 >= markets, "{staffed} of {markets} markets have a trader");
        assert!(traders > 50 && in_market + 3 >= traders, "{in_market} of {traders} traders stand in a market");
    }

    #[test]
    fn facing_follows_the_yaw() {
        let spot = |yaw: f32| FolkSpot { x: 0.0, y: 0.0, yaw, folk: Folk::Human, seed: 0, faction: Faction(0), trade: None };
        let (dx, dy) = spot(crate::Facing::North.yaw()).facing();
        assert!(dx.abs() < 1e-6 && (dy + 1.0).abs() < 1e-6);
        let (dx, dy) = spot(crate::Facing::West.yaw()).facing();
        assert!((dx + 1.0).abs() < 1e-6 && dy.abs() < 1e-6);
    }
}

