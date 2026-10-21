//! What each room of a claim is for: a hall by the claim's main door, a hearth, a place to
//! sleep, a market where the faction trades, and stores in any rooms left over.
//!
//! Roles come from a stream of the level seed of their own, after the claims and before the
//! folk, so who stands where follows from them. They never move a wall or an item. The hearth
//! and the hall each get a brazier ([`light_roles`]) where the level's own furnishing left none
//! in the room; props furnish each role in [`crate::props`], and folk stand by role in
//! [`crate::folk`].
//!
//! A claim of few rooms doubles up: a room takes every role the claim has no room left for, the
//! hall first.

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::folk::by_door;
use crate::generate::shuffle;
use crate::rooms::RoomGraph;
use crate::{Cell, Claim, Level, LightKind, LightSpot};

/// Salt for the role stream.
pub(crate) const ROLE_SALT: u32 = 0x601E_5A11;

/// What a room of a claim is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoomRole {
    /// The claim's room nearest its main entrance, a doorway into wild ground.
    Hall,
    /// Where the faction gathers round a fire and cooks.
    Hearth,
    /// Where its members bed down.
    Sleeping,
    /// Where it trades: stalls of goods. Only a faction that trades has one.
    Market,
    /// Spare rooms: stocks laid by.
    Store,
}

impl RoomRole {
    /// A single letter for maps.
    pub fn letter(self) -> char {
        match self {
            RoomRole::Hall => 'H',
            RoomRole::Hearth => 'F',
            RoomRole::Sleeping => 'S',
            RoomRole::Market => 'M',
            RoomRole::Store => 'T',
        }
    }
}

impl Level {
    /// The roles of room `room`: empty for a room no faction holds.
    pub fn roles_of(&self, room: usize) -> &[RoomRole] {
        self.roles.get(room).map_or(&[], Vec::as_slice)
    }

    /// The first room of `claim` with `role`.
    pub fn room_with_role(&self, claim: &Claim, role: RoomRole) -> Option<usize> {
        claim.rooms.iter().copied().find(|&r| self.roles_of(r).contains(&role))
    }

    /// Whether the faction `claim` belongs to has traders, so keeps a market.
    pub fn trades(&self, claim: &Claim) -> bool {
        self.room_with_role(claim, RoomRole::Market).is_some()
    }
}

/// A door on the edge of a claim: a door cell of one of its rooms that opens onto ground the
/// claim does not hold. Its faction decides who it is locked to ([`Level::claim_doors`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimDoor {
    pub x: i32,
    pub y: i32,
    pub faction: crate::Faction,
    /// Index into [`Level::rooms`] of the claim's room the door is in.
    pub room: usize,
    /// The claim's main door: the one of its hall nearest where the delver starts. At Neutral
    /// standing it stands open to the delver while the claim's other doors are locked.
    pub main: bool,
}

impl Level {
    /// Every door on the edge of a claim, in reading order, the entrance excluded. A door
    /// between two rooms of the same claim is no claim door.
    pub fn claim_doors(&self) -> Vec<ClaimDoor> {
        let entrance = (self.entrance.x as i32, self.entrance.y as i32);
        let (sx, sy, _) = self.start();
        let mut doors = Vec::new();
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                if self.cell(x, y) != Cell::Door || (x, y) == entrance {
                    continue;
                }
                // A door stands in the wall beside its room, or on the room's own edge.
                let near: Vec<(i32, i32)> = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().map(|(dx, dy)| (x + dx, y + dy)).filter(|&(nx, ny)| self.is_open(nx, ny)).collect();
                let Some((room, claim)) = near.iter().filter_map(|&(nx, ny)| self.room_at(nx, ny)).find_map(|r| Some((r, self.claim_of(r)?))) else { continue };
                let leads_out = near.iter().any(|&(nx, ny)| (nx, ny) != (x, y) && self.room_at(nx, ny).is_none_or(|r| !claim.rooms.contains(&r)));
                if leads_out {
                    doors.push(ClaimDoor { x, y, faction: claim.faction, room, main: false });
                }
            }
        }
        // The main door of each claim: of the doors of its hall, the one nearest the start.
        for claim in &self.claims {
            let Some(hall) = self.room_with_role(claim, RoomRole::Hall) else { continue };
            let nearest = doors.iter().enumerate().filter(|(_, d)| d.room == hall).min_by_key(|(_, d)| ((d.x - sx as i32).abs() + (d.y - sy as i32).abs(), d.y, d.x)).map(|(i, _)| i);
            if let Some(i) = nearest {
                doors[i].main = true;
            }
        }
        doors
    }
}

/// The roles of every room of the level, indexed by room: empty where no one holds it.
/// `plans` are the claims' trades ([`crate::trades::plan`]): a claim with a trader in them gets a market.
pub(crate) fn assign_roles(level: &Level, plans: &[Vec<Option<fb_faction::Trade>>]) -> Vec<Vec<RoomRole>> {
    let graph = RoomGraph::new(level);
    let (sx, sy, _) = level.start();
    let mut roles = vec![Vec::new(); level.rooms.len()];
    for (claim, plan) in level.claims.iter().zip(plans) {
        let hall = openings(level, &graph, claim, (sx as i32, sy as i32)).first().map_or(claim.rooms[0], |o| o.room);
        let centre = |r: usize| level.rooms[r].centre();
        let (hx, hy) = centre(hall);
        let mut others: Vec<usize> = claim.rooms.iter().copied().filter(|&r| r != hall).collect();
        others.sort_by(|&a, &b| {
            let d = |r: usize| (centre(r).0 - hx).hypot(centre(r).1 - hy);
            d(a).total_cmp(&d(b)).then(a.cmp(&b))
        });
        let market = plan.iter().flatten().any(|t| t.profession != fb_faction::Profession::Guard);
        roles[hall].push(RoomRole::Hall);
        match others.len() {
            0 => {
                roles[hall].push(RoomRole::Hearth);
                roles[hall].push(RoomRole::Sleeping);
                if market {
                    roles[hall].push(RoomRole::Market);
                }
            }
            1 => {
                roles[others[0]].push(RoomRole::Sleeping);
                roles[hall].push(RoomRole::Hearth);
                if market {
                    roles[hall].push(RoomRole::Market);
                }
            }
            n => {
                // Nearest the hall is the hearth, deepest in is where they sleep, and the
                // market sits between when there is room for one.
                roles[others[0]].push(RoomRole::Hearth);
                roles[others[n - 1]].push(RoomRole::Sleeping);
                let mut middle = 1..n - 1;
                if market {
                    match middle.next() {
                        Some(i) => roles[others[i]].push(RoomRole::Market),
                        None => roles[hall].push(RoomRole::Market),
                    }
                }
                for i in middle {
                    roles[others[i]].push(RoomRole::Store);
                }
            }
        }
    }
    roles
}

/// A way out of a claim: the claim's floor cell `cell` in room `room`, with an open cell beside
/// it, in direction `out`, that leads into ground the claim does not hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Opening {
    pub room: usize,
    pub cell: (i32, i32),
    pub out: (i32, i32),
    /// Manhattan distance from the delver's start to `cell`.
    pub from_start: i32,
}

/// Every opening of `claim`, nearest `from` (the delver's start) first: the first is the claim's
/// main door, and its room is the hall.
pub(crate) fn openings(level: &Level, graph: &RoomGraph, claim: &Claim, from: (i32, i32)) -> Vec<Opening> {
    let mut found = Vec::new();
    for &r in &claim.rooms {
        let room = &level.rooms[r];
        for y in room.y as i32..(room.y + room.h) as i32 {
            for x in room.x as i32..(room.x + room.w) as i32 {
                if level.cell(x, y) != Cell::Floor {
                    continue;
                }
                for out in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (x + out.0, y + out.1);
                    if room.contains(nx, ny) || !level.is_open(nx, ny) {
                        continue;
                    }
                    if graph.rooms_of(nx, ny).iter().any(|o| !claim.rooms.contains(o)) {
                        found.push(Opening { room: r, cell: (x, y), out, from_start: (x - from.0).abs() + (y - from.1).abs() });
                    }
                }
            }
        }
    }
    found.sort_by_key(|o| (o.from_start, o.room, o.cell, o.out));
    found
}

/// The claim's main door: the room it opens from, the room's cell inside it and the open cell
/// just outside, where the faction's banners hang beside it ([`crate::banners`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MainDoor {
    pub room: usize,
    pub inside: (i32, i32),
    pub outside: (i32, i32),
}

/// The opening of `claim` nearest `from` that leads out of it, if it has one: the first of
/// [`openings`].
pub(crate) fn main_door(level: &Level, graph: &RoomGraph, claim: &Claim, from: (i32, i32)) -> Option<MainDoor> {
    openings(level, graph, claim, from).first().map(|o| MainDoor { room: o.room, inside: o.cell, outside: (o.cell.0 + o.out.0, o.cell.1 + o.out.1) })
}

/// Lights a brazier in each hall and hearth the level's furnishing left without one, at the
/// open cell nearest the room's middle that is clear of doors, items and other lights.
pub(crate) fn light_roles(level: &mut Level, rng: &mut Mulberry32) {
    let mut wanted: Vec<usize> = (0..level.rooms.len()).filter(|&r| level.roles_of(r).iter().any(|&role| matches!(role, RoomRole::Hall | RoomRole::Hearth))).collect();
    // The order rooms are lit in decides which of two close cells a light takes; the stream
    // keeps it a function of the seed alone.
    shuffle(&mut wanted, rng);
    for r in wanted {
        let room = level.rooms[r];
        if level.lights.iter().any(|l| l.kind == LightKind::Brazier && room.contains(l.x as i32, l.y as i32)) {
            continue;
        }
        let (cx, cy) = room.centre();
        let floor: Vec<(i32, i32)> = (room.y as i32..(room.y + room.h) as i32)
            .flat_map(|y| (room.x as i32..(room.x + room.w) as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| level.cell(x, y) == Cell::Floor)
            .collect();
        let clear = |&(x, y): &(i32, i32), from_item: f32, from_light: f32| {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let far = |ax: f32, ay: f32, d: f32| (ax - px).hypot(ay - py) >= d;
            level.items.iter().all(|i| far(i.x, i.y, from_item)) && level.lights.iter().all(|l| far(l.x, l.y, from_light))
        };
        // The best cell: off the doors, clear of items and lights; a cramped room gives up
        // clearance, then the doors, rather than stand dark.
        let tiers: [&dyn Fn(&(i32, i32)) -> bool; 3] = [&|c| !by_door(level, c.0, c.1) && clear(c, 0.9, 2.0), &|c| !by_door(level, c.0, c.1) && clear(c, 0.55, 0.0), &|_| true];
        let mut cells: Vec<(i32, i32)> = tiers.iter().map(|ok| floor.iter().copied().filter(|c| ok(c)).collect::<Vec<_>>()).find(|c| !c.is_empty()).unwrap_or_default();
        cells.sort_by(|a, b| {
            let d = |c: &(i32, i32)| (c.0 as f32 + 0.5 - cx).hypot(c.1 as f32 + 0.5 - cy);
            d(a).total_cmp(&d(b)).then(a.cmp(b))
        });
        if let Some(&(x, y)) = cells.first() {
            level.lights.push(LightSpot { x: x as f32 + 0.5, y: y as f32 + 0.5, kind: LightKind::Brazier });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate;

    fn sweep() -> impl Iterator<Item = (u32, u32)> {
        [1, 2].into_iter().flat_map(|depth| (1..=64).map(move |seed| (seed, depth)))
    }

    #[test]
    fn every_claim_has_a_hall_and_only_claimed_rooms_have_roles() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for claim in &level.claims {
                assert!(level.room_with_role(claim, RoomRole::Hall).is_some(), "seed {seed} depth {depth}: {claim:?} has no hall");
                for role in [RoomRole::Hearth, RoomRole::Sleeping] {
                    assert!(level.room_with_role(claim, role).is_some(), "seed {seed} depth {depth}: no {role:?}");
                }
                assert_eq!(level.room_with_role(claim, RoomRole::Market).is_some(), level.trades(claim), "seed {seed} depth {depth}");
            }
            for r in 0..level.rooms.len() {
                assert_eq!(level.roles_of(r).is_empty(), !level.is_claimed(r), "seed {seed} depth {depth}: room {r}");
            }
        }
    }

    #[test]
    fn halls_and_hearths_have_fires() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for claim in &level.claims {
                for role in [RoomRole::Hall, RoomRole::Hearth] {
                    let r = level.room_with_role(claim, role).unwrap();
                    let room = level.rooms[r];
                    assert!(level.lights.iter().any(|l| l.kind == LightKind::Brazier && room.contains(l.x as i32, l.y as i32)), "seed {seed} depth {depth}: {role:?} is dark");
                }
            }
        }
    }

    #[test]
    fn traders_stand_in_their_market() {
        let (mut markets, mut staffed) = (0, 0);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for claim in level.claims.iter().filter(|c| level.trades(c)) {
                let market = level.room_with_role(claim, RoomRole::Market).unwrap();
                markets += 1;
                let there = level.folk.iter().filter(|f| f.faction == claim.faction && level.room_at(f.x as i32, f.y as i32) == Some(market)).count();
                staffed += (there >= 1) as u32;
            }
        }
        // A market too cramped to stand in (a room of 3 by 3 round its fire) is left to the folk's other rooms.
        assert!(markets > 8 && staffed * 10 >= markets * 8, "{staffed} of {markets} markets have a trader");
    }

    #[test]
    fn claim_doors_lie_on_claim_edges_and_each_hall_has_one_main() {
        let (mut doors, mut mains) = (0, 0);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let all = level.claim_doors();
            for d in &all {
                assert_eq!(level.cell(d.x, d.y), Cell::Door);
                assert_eq!(level.claim_of(d.room).map(|c| c.faction), Some(d.faction), "seed {seed} depth {depth}");
            }
            doors += all.len();
            for claim in &level.claims {
                let n = all.iter().filter(|d| d.faction == claim.faction && d.main).count();
                assert!(n <= 1, "seed {seed} depth {depth}: {n} main doors");
                mains += n;
            }
            assert_eq!(all, level.claim_doors());
        }
        assert!(doors > 40 && mains > 20, "{doors} claim doors, {mains} main");
    }

    #[test]
    fn roles_are_the_same_for_the_same_seed() {
        for (seed, depth) in sweep().take(20) {
            assert_eq!(generate(seed, depth).roles, generate(seed, depth).roles);
        }
    }
}
