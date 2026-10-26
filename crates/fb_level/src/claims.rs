//! Ground the factions hold: each faction on a level claims a connected set of rooms.
//!
//! A claim grows out from a seed room along the level's room graph ([`RoomGraph`]), from its
//! own stream of the level seed, after the layout and the furnishing and before the folk, so
//! changing claims never moves a wall, an item or a light. Its size follows the faction's
//! strength, the members it is to place. Claims never overlap, the starting room is never
//! claimed, and a room no one claims is wild: creatures live there.
//!
//! Ground is fixed per seed. Only the delver changes it ([`settle_claims`]): a faction down to
//! one member keeps the one room that member holds, and a faction with no one left holds none.

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::generate::{is_colonnaded, shuffle};
use crate::rooms::RoomGraph;
use crate::{Faction, Level};

/// Salt for the claim stream.
pub(crate) const CLAIM_SALT: u32 = 0xC1A1_4D07;
/// Members a room of a claim is sized for.
const MEMBERS_PER_ROOM: usize = 3;
/// Smallest room a claim starts from or a group gathers in.
pub(crate) const MIN_ROOM_AREA: u32 = 9;
/// Smallest room a trading faction's claim starts from, if it holds but the one.
const TRADER_SEED_AREA: u32 = 16;

/// The numbers claims grow by with depth, here so a designer can tune them. The abyss near the
/// top is a scatter of small camps; further down the factions are bigger and take more ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClaimRules {
    /// Folk a level aims for in all at depth 1, inclusive, shared out among its factions.
    pub folk_shallow: (u32, u32),
    /// Folk a level aims for at `levels_off` and below.
    pub folk_deep: (u32, u32),
    /// The depth growth stops at: everything below is as this one.
    pub levels_off: u32,
    /// Share of a level's candidate rooms the factions may claim at depth 1.
    pub share_shallow: f64,
    /// Share of them at `levels_off` and below.
    pub share_deep: f64,
}

impl ClaimRules {
    pub const DEFAULT: ClaimRules = ClaimRules { folk_shallow: (6, 10), folk_deep: (16, 24), levels_off: 12, share_shallow: 0.5, share_deep: 2.0 / 3.0 };

    /// How far `depth` is along the growth from depth 1 (0.0) to `levels_off` (1.0).
    pub fn progress(&self, depth: u32) -> f64 {
        if self.levels_off <= 1 { 1.0 } else { (depth.saturating_sub(1) as f64 / (self.levels_off - 1) as f64).min(1.0) }
    }

    /// The least and most folk a level at `depth` aims for.
    pub fn folk_range(&self, depth: u32) -> (u32, u32) {
        let t = self.progress(depth);
        let lerp = |a: u32, b: u32| (a as f64 + (b as f64 - a as f64) * t).round() as u32;
        (lerp(self.folk_shallow.0, self.folk_deep.0), lerp(self.folk_shallow.1, self.folk_deep.1))
    }

    /// The share of its candidate rooms a level at `depth` lets the factions claim.
    pub fn claim_share(&self, depth: u32) -> f64 {
        self.share_shallow + (self.share_deep - self.share_shallow) * self.progress(depth)
    }

    /// The most rooms the factions may claim of `candidates` rooms at `depth`.
    pub fn claim_budget(&self, candidates: usize, depth: u32) -> usize {
        ((candidates as f64 * self.claim_share(depth) + 1e-9).floor() as usize).max(1)
    }
}

impl Default for ClaimRules {
    fn default() -> Self {
        ClaimRules::DEFAULT
    }
}

/// The rooms one faction holds on a level.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub faction: Faction,
    /// Indexes into [`Level::rooms`]: connected through the room graph, the seed room first.
    pub rooms: Vec<usize>,
}

impl Level {
    /// The claim over room `room`, if some faction holds it.
    pub fn claim_of(&self, room: usize) -> Option<&Claim> {
        self.claims.iter().find(|c| c.rooms.contains(&room))
    }

    /// Whether any faction holds room `room`.
    pub fn is_claimed(&self, room: usize) -> bool {
        self.claim_of(room).is_some()
    }
}

/// Grows a claim for each of the level's factions, and how many members each is to place
/// (the same order as the claims). A faction that finds no room to seed in gets no claim. No
/// claim starts in a room of `avoid`, ones where no folk were found space to stand.
pub(crate) fn grow_claims(level: &Level, start_room: usize, avoid: &[usize], rng: &mut Mulberry32) -> (Vec<Claim>, Vec<usize>) {
    let n = level.factions.len();
    if n == 0 {
        return (Vec::new(), Vec::new());
    }
    // The creatures keep the rest of the rooms (half at the top, a third far down), so a small
    // level claims for fewer factions than it holds; the rest stand on no ground and place no one.
    let rules = ClaimRules::DEFAULT;
    let candidates = (0..level.rooms.len()).filter(|&i| i != start_room && level.rooms[i].area() >= MIN_ROOM_AREA).count();
    let mut budget = rules.claim_budget(candidates, level.depth);
    let (least, most) = rules.folk_range(level.depth);
    let total = rng.int(least, most) as usize;
    let mut order: Vec<usize> = (0..n).collect();
    shuffle(&mut order, rng);
    // A denizen faction is why the level is deep: when the creatures leave room for only some of
    // the factions, the denizens are the ones that keep theirs (the shuffle's order otherwise).
    order.sort_by_key(|&k| !level.factions[k].is_denizen());
    order.truncate(budget.min(n));
    let m = order.len();
    let mut strength = vec![0; n];
    for (j, &k) in order.iter().enumerate() {
        strength[k] = total / m + (j < total % m) as usize;
    }
    let halls = level.style == crate::LevelStyle::StoneHalls;
    let hall = |r: usize| halls && is_colonnaded(level, &level.rooms[r]);
    let graph = RoomGraph::new(level);
    let wild_whole = |trial: &[Claim]| wild_stays_whole(level, &graph, trial);
    let mut owner: Vec<Option<usize>> = vec![None; level.rooms.len()];
    let mut claims: Vec<Claim> = Vec::new();
    let mut strengths = Vec::new();
    for (j, &k) in order.iter().enumerate() {
        let mut seeds: Vec<usize> = (0..level.rooms.len()).filter(|&i| i != start_room && owner[i].is_none() && level.rooms[i].area() >= MIN_ROOM_AREA).collect();
        shuffle(&mut seeds, rng);
        // Folk of the halls hold the colonnades, leaving the creatures the rest.
        seeds.sort_by_key(|&r| !hall(r));
        // Traders set out a market, which a bare nine cells has no room for.
        let min_seed = if level.factions[k].intention == fb_faction::Intention::Trade { TRADER_SEED_AREA } else { MIN_ROOM_AREA };
        // The first seed whose loss leaves the wild ground whole; none, and the faction has no ground.
        let fits = |rooms: &[usize], claims: &[Claim]| {
            let mut trial = claims.to_vec();
            trial.push(Claim { faction: level.factions[k].id, rooms: rooms.to_vec() });
            wild_whole(&trial)
        };
        let Some(&seed) = seeds.iter().find(|&&r| !avoid.contains(&r) && level.rooms[r].area() >= min_seed && fits(&[r], &claims)) else { continue };
        let extra = rng.int(0, 1) as usize;
        // One room stays back for each faction still to place.
        let spare = budget.saturating_sub(m - j - 1).max(1);
        let want = (strength[k].div_ceil(MEMBERS_PER_ROOM) + extra).clamp(1, spare);
        budget = budget.saturating_sub(want);
        let mut rooms = vec![seed];
        owner[seed] = Some(k);
        while rooms.len() < want {
            let mut frontier: Vec<usize> = rooms
                .iter()
                .flat_map(|&r| graph.links(r).iter().copied())
                .filter(|&r| r != start_room && owner[r].is_none() && level.rooms[r].area() >= MIN_ROOM_AREA)
                .collect();
            frontier.sort_unstable();
            frontier.dedup();
            // A room whose loss would cut wild rooms off from the start, or from each other, stays wild.
            frontier.retain(|&r| fits(&[rooms.as_slice(), &[r]].concat(), &claims));
            if frontier.iter().any(|&r| hall(r)) {
                frontier.retain(|&r| hall(r));
            }
            if frontier.is_empty() {
                break;
            }
            let next = frontier[rng.int(0, frontier.len() as u32 - 1) as usize];
            owner[next] = Some(k);
            rooms.push(next);
        }
        claims.push(Claim { faction: level.factions[k].id, rooms });
        strengths.push(strength[k]);
    }
    (claims, strengths)
}

/// Whether the ground `claims` leave free is one piece with the start: every open cell outside
/// their rooms and their edge doors can be walked to from the start without crossing either.
/// The stairs lie in that ground, so the way down is never behind a faction.
pub(crate) fn wild_stays_whole(level: &Level, graph: &RoomGraph, claims: &[Claim]) -> bool {
    let held = held_cells_in(level, graph, claims);
    let index = |x: i32, y: i32| (y as u32 * level.width + x as u32) as usize;
    let (sx, sy, _) = level.start();
    let mut seen = vec![false; level.cells.len()];
    seen[index(sx as i32, sy as i32)] = true;
    let mut stack = vec![(sx as i32, sy as i32)];
    let mut reached = 1;
    while let Some((x, y)) = stack.pop() {
        for facing in crate::Facing::ALL {
            let (dx, dy) = facing.step();
            let (nx, ny) = (x + dx, y + dy);
            if level.is_open(nx, ny) && !held[index(nx, ny)] && !seen[index(nx, ny)] {
                seen[index(nx, ny)] = true;
                reached += 1;
                stack.push((nx, ny));
            }
        }
    }
    let wild = (0..level.cells.len()).filter(|&i| level.cells[i].is_open() && !held[i]).count();
    reached == wild
}

/// Whether open cell `(x, y)` is ground of a claim over `rooms`: a cell of one of its rooms, or
/// of a corridor that opens into its rooms alone. A claim of several rooms holds the corridors
/// between them, which lead nowhere else, so the wild ground need not reach them.
pub(crate) fn in_ground(level: &Level, graph: &RoomGraph, rooms: &[usize], x: i32, y: i32) -> bool {
    if !level.is_open(x, y) {
        return false;
    }
    match level.room_at(x, y) {
        Some(r) => rooms.contains(&r),
        None => {
            let opens = graph.rooms_of(x, y);
            !opens.is_empty() && opens.iter().all(|r| rooms.contains(r))
        }
    }
}

/// The cells `claims` shut to the delver in the wild: their rooms' open cells, the corridors
/// between those rooms and their edge doors, as a flag per cell.
pub(crate) fn held_cells(level: &Level, claims: &[Claim]) -> Vec<bool> {
    held_cells_in(level, &RoomGraph::new(level), claims)
}

/// [`held_cells`] with the level's room graph at hand.
pub(crate) fn held_cells_in(level: &Level, graph: &RoomGraph, claims: &[Claim]) -> Vec<bool> {
    let index = |x: i32, y: i32| (y as u32 * level.width + x as u32) as usize;
    let mut held = vec![false; level.cells.len()];
    for y in 0..level.height as i32 {
        for x in 0..level.width as i32 {
            if claims.iter().any(|c| in_ground(level, graph, &c.rooms, x, y)) {
                held[index(x, y)] = true;
            }
        }
    }
    for door in level.edge_doors_in(graph, claims) {
        held[index(door.x, door.y)] = true;
    }
    held
}

/// The claims of `level` after the delver's work on it. `alive[i]` is where folk `i` of
/// [`Level::folk`] now stands (grid coordinates), or `None` if they have fallen; a folk past
/// the end of `alive` is as generated.
///
/// A faction with several members left keeps its claim whole, and so does one that never had more than one. One left of several keeps the room they
/// hold (the room they stand in if it is one of the claim's, otherwise the claim's room nearest
/// them). None left, and it holds nothing: only the delver wipes a faction out.
pub fn settle_claims(level: &Level, alive: &[Option<(f32, f32)>]) -> Vec<Claim> {
    level
        .claims
        .iter()
        .filter_map(|claim| {
            let living: Vec<(f32, f32)> = level
                .folk
                .iter()
                .enumerate()
                .filter(|(_, f)| f.faction == claim.faction)
                .filter_map(|(i, f)| match alive.get(i) {
                    Some(Some(at)) => Some(*at),
                    Some(None) => None,
                    None => Some((f.x, f.y)),
                })
                .collect();
            let generated = level.folk.iter().filter(|f| f.faction == claim.faction).count();
            match living.as_slice() {
                [] => None,
                // Only a faction the delver thinned to one shrinks; one that was always alone is untouched.
                [(x, y)] if claim.rooms.len() > 1 && generated > 1 => {
                    let here = level.room_at(x.floor() as i32, y.floor() as i32).filter(|r| claim.rooms.contains(r));
                    let held = here.unwrap_or_else(|| {
                        let dist = |r: &usize| {
                            let (cx, cy) = level.rooms[*r].centre();
                            (cx - x).hypot(cy - y)
                        };
                        *claim.rooms.iter().min_by(|a, b| dist(a).total_cmp(&dist(b))).expect("a claim holds a room")
                    });
                    Some(Claim { faction: claim.faction, rooms: vec![held] })
                }
                _ => Some(claim.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate;
    use std::collections::HashSet;

    fn sweep() -> impl Iterator<Item = (u32, u32)> {
        [1, 2].into_iter().flat_map(|depth| (1..=64).map(move |seed| (seed, depth)))
    }

    /// Depths the claim rules change at: the top, the middle and where growth levels off.
    const DEPTHS: [u32; 3] = [1, 6, 12];

    fn deep_sweep() -> impl Iterator<Item = (u32, u32)> {
        DEPTHS.into_iter().flat_map(|depth| (1..=24).map(move |seed| (seed, depth)))
    }

    #[test]
    fn the_folk_a_level_aims_for_grow_with_depth_and_level_off() {
        let rules = ClaimRules::DEFAULT;
        assert_eq!(rules.folk_range(1), (6, 10));
        assert_eq!(rules.folk_range(12), (16, 24));
        assert_eq!(rules.folk_range(40), (16, 24));
        let mut last = rules.folk_range(1);
        for depth in 2..=14 {
            let now = rules.folk_range(depth);
            assert!(now.0 >= last.0 && now.1 >= last.1, "depth {depth}");
            last = now;
        }
        assert_eq!(rules.claim_share(1), 0.5);
        assert!((rules.claim_share(12) - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(rules.claim_share(30), rules.claim_share(12));
    }

    #[test]
    fn deeper_levels_hold_more_folk_and_more_rooms() {
        // Depths 1 and 11 are both rough rock, so their levels are as big.
        let (mut shallow, mut deep) = ((0, 0), (0, 0));
        for seed in 1..=24 {
            let (a, b) = (generate(seed, 1), generate(seed, 11));
            shallow = (shallow.0 + a.folk.len(), shallow.1 + a.claims.iter().map(|c| c.rooms.len()).sum::<usize>());
            deep = (deep.0 + b.folk.len(), deep.1 + b.claims.iter().map(|c| c.rooms.len()).sum::<usize>());
        }
        assert!(deep.0 * 10 >= shallow.0 * 15, "folk {shallow:?} -> {deep:?}");
        assert!(deep.1 > shallow.1, "rooms {shallow:?} -> {deep:?}");
    }

    #[test]
    fn claims_are_the_same_for_the_same_seed() {
        for seed in [1, 1207, u32::MAX] {
            assert_eq!(generate(seed, 1).claims, generate(seed, 1).claims);
        }
        assert_ne!(generate(1, 1).claims, generate(2, 1).claims);
    }

    #[test]
    fn every_faction_on_a_level_holds_exactly_one_claim_and_every_claim_is_a_levels_faction() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            assert!(!level.claims.is_empty(), "seed {seed} depth {depth}");
            for spec in &level.factions {
                assert_eq!(level.claims.iter().filter(|c| c.faction == spec.id).count(), 1, "seed {seed} depth {depth}: {}", spec.name);
            }
            for c in &level.claims {
                assert!(level.factions.iter().any(|s| s.id == c.faction), "seed {seed} depth {depth}");
            }
        }
    }

    #[test]
    fn claims_leave_the_creatures_the_rest_of_the_rooms_and_hold_only_rooms_folk_can_stand_in() {
        let rules = ClaimRules::DEFAULT;
        for (seed, depth) in sweep().chain(deep_sweep()) {
            let level = generate(seed, depth);
            let candidates = level.rooms.len() - 1;
            let claimed: usize = level.claims.iter().map(|c| c.rooms.len()).sum();
            let share = (candidates as f64 * rules.claim_share(depth)).ceil() as usize;
            assert!(claimed <= share, "seed {seed} depth {depth}: {claimed} of {candidates}");
            for c in &level.claims {
                assert!(c.rooms.iter().all(|&r| level.rooms[r].area() >= MIN_ROOM_AREA), "seed {seed} depth {depth}: small room claimed");
            }
        }
    }

    #[test]
    fn a_faction_that_was_always_alone_keeps_its_claim() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            let alive: Vec<_> = level.folk.iter().map(|f| Some((f.x, f.y))).collect();
            for c in level.claims.iter().filter(|c| level.folk.iter().filter(|f| f.faction == c.faction).count() <= 1) {
                assert!(settle_claims(&level, &alive).contains(c));
            }
        }
    }

    #[test]
    fn no_room_is_in_two_claims_and_the_start_room_is_never_claimed() {
        for (seed, depth) in sweep().chain(deep_sweep()) {
            let level = generate(seed, depth);
            let start = level.start_room().unwrap();
            let mut seen = HashSet::new();
            for claim in &level.claims {
                assert!(!claim.rooms.is_empty());
                for &r in &claim.rooms {
                    assert!(seen.insert(r), "seed {seed} depth {depth}: room {r} is in two claims");
                    assert_ne!(r, start, "seed {seed} depth {depth}: the start room is claimed");
                }
            }
            assert!(seen.len() < level.rooms.len(), "seed {seed} depth {depth}: no wild room left");
        }
    }

    #[test]
    fn claims_are_connected() {
        for (seed, depth) in sweep().chain(deep_sweep()) {
            let level = generate(seed, depth);
            let graph = RoomGraph::new(&level);
            for claim in &level.claims {
                let mut reached = vec![claim.rooms[0]];
                let mut i = 0;
                while i < reached.len() {
                    for &next in graph.links(reached[i]) {
                        if claim.rooms.contains(&next) && !reached.contains(&next) {
                            reached.push(next);
                        }
                    }
                    i += 1;
                }
                assert_eq!(reached.len(), claim.rooms.len(), "seed {seed} depth {depth}: {claim:?} is split");
            }
        }
    }

    #[test]
    fn every_folk_stands_in_their_factions_claim() {
        for (seed, depth) in sweep().chain(deep_sweep()) {
            let level = generate(seed, depth);
            for f in &level.folk {
                let room = level.room_at(f.x as i32, f.y as i32).unwrap();
                let claim = level.claim_of(room).unwrap_or_else(|| panic!("seed {seed} depth {depth}: {f:?} stands in a wild room"));
                assert_eq!(claim.faction, f.faction, "seed {seed} depth {depth}");
            }
        }
    }

    #[test]
    fn creatures_keep_out_of_claims() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for c in &level.creatures {
                if let Some(room) = level.room_at(c.x as i32, c.y as i32) {
                    assert!(!level.is_claimed(room), "seed {seed} depth {depth}: {c:?} in a claim");
                }
            }
        }
    }

    /// Whether `(x, y)` is ground a claim holds: a room, a corridor between its rooms or an
    /// edge door.
    fn held_by_claims(level: &Level, graph: &RoomGraph, doors: &HashSet<(i32, i32)>, x: i32, y: i32) -> bool {
        doors.contains(&(x, y)) || level.claims.iter().any(|c| in_ground(level, graph, &c.rooms, x, y))
    }

    /// Cells the delver can walk from the start without entering a claim's ground or passing a
    /// claim door, as a flag per cell.
    fn wild_reach(level: &Level) -> Vec<bool> {
        let graph = RoomGraph::new(level);
        let doors: HashSet<(i32, i32)> = level.claim_doors().iter().map(|d| (d.x, d.y)).collect();
        let index = |x: i32, y: i32| (y as u32 * level.width + x as u32) as usize;
        let mut seen = vec![false; level.cells.len()];
        let (sx, sy, _) = level.start();
        let mut stack = vec![(sx as i32, sy as i32)];
        seen[index(sx as i32, sy as i32)] = true;
        while let Some((x, y)) = stack.pop() {
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                let held = held_by_claims(level, &graph, &doors, nx, ny);
                if level.is_open(nx, ny) && !held && !seen[index(nx, ny)] {
                    seen[index(nx, ny)] = true;
                    stack.push((nx, ny));
                }
            }
        }
        seen
    }

    /// Seeds and depths whose stairs the delver cannot reach through wild ground.
    fn blocked_levels(seeds: std::ops::RangeInclusive<u32>, depths: std::ops::RangeInclusive<u32>) -> Vec<(u32, u32)> {
        let mut blocked = Vec::new();
        for depth in depths {
            for seed in seeds.clone() {
                let level = generate(seed, depth);
                let seen = wild_reach(&level);
                let reaches = |x: u32, y: u32| seen[(y * level.width + x) as usize];
                let ok = !level.stairs.is_empty()
                    && level.stairs.iter().all(|s| {
                        let (ax, ay) = s.approach();
                        reaches(ax, ay)
                    });
                if !ok {
                    blocked.push((seed, depth));
                }
            }
        }
        blocked
    }

    #[test]
    fn the_start_reaches_every_stair_through_wild_ground() {
        let blocked = blocked_levels(1..=200, 1..=4);
        assert!(blocked.is_empty(), "{} blocked (seed, depth): {:?}", blocked.len(), blocked);
        for depth in [6, 12] {
            let blocked = blocked_levels(1..=60, depth..=depth);
            assert!(blocked.is_empty(), "{} blocked (seed, depth): {:?}", blocked.len(), blocked);
        }
    }

    #[test]
    fn hardly_a_level_is_left_without_folk() {
        // The regrow falls back to claims of one when no room can hold a claim of three, so a
        // level ends with no claim only where no room can start one at all.
        let empty: Vec<_> = sweep().filter(|&(seed, depth)| generate(seed, depth).claims.is_empty()).collect();
        let total = sweep().count();
        assert!(empty.len() * 50 <= total, "{} of {total} levels have no claim: {:?}", empty.len(), empty);
    }

    #[test]
    fn wild_ground_stays_connected() {
        for (seed, depth) in sweep().chain(deep_sweep()) {
            let level = generate(seed, depth);
            let seen = wild_reach(&level);
            let doors: HashSet<(i32, i32)> = level.claim_doors().iter().map(|d| (d.x, d.y)).collect();
            let graph = RoomGraph::new(&level);
            for y in 0..level.height as i32 {
                for x in 0..level.width as i32 {
                    let held = held_by_claims(&level, &graph, &doors, x, y);
                    if level.is_open(x, y) && !held {
                        assert!(seen[(y as u32 * level.width + x as u32) as usize], "seed {seed} depth {depth}: wild cell ({x}, {y}) is cut off");
                    }
                }
            }
        }
    }

    /// A level with a claim of several rooms held by a faction of two or more folk.
    fn big_claim() -> (Level, usize) {
        for seed in 1..=64 {
            let level = generate(seed, 1);
            for (k, c) in level.claims.iter().enumerate() {
                if c.rooms.len() > 1 && level.folk.iter().filter(|f| f.faction == c.faction).count() > 1 {
                    return (level, k);
                }
            }
        }
        panic!("no claim of several rooms");
    }

    #[test]
    fn a_faction_down_to_one_member_keeps_the_one_room_they_hold() {
        let (level, k) = big_claim();
        let faction = level.claims[k].faction;
        let members: Vec<usize> = (0..level.folk.len()).filter(|&i| level.folk[i].faction == faction).collect();
        let mut alive: Vec<Option<(f32, f32)>> = level.folk.iter().map(|f| Some((f.x, f.y))).collect();
        for &i in &members[1..] {
            alive[i] = None;
        }
        let held = level.room_at(level.folk[members[0]].x as i32, level.folk[members[0]].y as i32).unwrap();
        let settled = settle_claims(&level, &alive);
        assert_eq!(settled.len(), level.claims.len());
        assert_eq!(settled[k], Claim { faction, rooms: vec![held] });
        for (a, b) in settled.iter().zip(&level.claims).filter(|(a, _)| a.faction != faction) {
            assert_eq!(a, b);
        }
        // Several still alive: whole.
        alive[members[1]] = Some((level.folk[members[1]].x, level.folk[members[1]].y));
        assert_eq!(settle_claims(&level, &alive)[k], level.claims[k]);
        // Last one falls: nothing held.
        let mut none = alive.clone();
        members.iter().for_each(|&i| none[i] = None);
        assert!(settle_claims(&level, &none).iter().all(|c| c.faction != faction));
    }

    #[test]
    fn nobody_fallen_changes_nothing() {
        for (seed, depth) in sweep().take(40) {
            let level = generate(seed, depth);
            let alive: Vec<_> = level.folk.iter().map(|f| Some((f.x, f.y))).collect();
            assert_eq!(settle_claims(&level, &alive), level.claims);
        }
    }
}
