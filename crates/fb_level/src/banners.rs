//! Faction banners: cloth hung on the walls of claimed rooms, flying the faction's colours.
//! A claim's hall door has one on the wall at each side of it, outside (the corridor or room
//! the door opens onto) and inside, and the hall and the market hang one or two more on bare
//! stretches of wall.
//!
//! They are a list of their own, not [`crate::Prop`]s: they hang on a wall face above anything
//! standing, so they never block a cell, push a creature or a stair aside, or stand in a
//! prop's way. They come from a stream of the level seed of their own, after the props, so a
//! wall, an item, a light, a prop or anyone standing never moves for them.
//!
//! The game builds each one from the Armory banner family with the faction's field, a rod
//! hung from the wall, and the rest of it (the foot, length, hem and fringe) rolled from
//! [`BannerSpot::seed`].

use procgen_rng::Mulberry32;
use serde::{Deserialize, Serialize};

use crate::folk::{Faction, by_door};
use crate::generate::{item_seed, shuffle};
use crate::roles::{RoomRole, main_door};
use crate::rooms::RoomGraph;
use crate::{CELL, Cell, Facing, Level};

/// Salt for the banner stream.
pub(crate) const BANNER_SALT: u32 = 0xBA44_E2A1;
/// How far the rod stands off the wall's grid face, in world units: the rock relief reaches
/// ([`crate::props::WALL_GAP`]) and a little over, so the cloth hangs in front of the stone.
pub const WALL_OFFSET: f32 = 1.9;
/// Least distance between two banners in a room, in cells, so a wall is not papered.
const APART: f32 = 2.0;
/// Nearest a banner hangs to a light or a prop, in cells.
const CLEAR: f32 = 0.9;

/// Where a banner hangs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BannerSite {
    /// Beside the hall door on its outer face, where a room or a widening lies outside it.
    DoorOutside,
    /// Over the hall door's lintel, facing down the corridor that feeds it.
    Lintel,
    /// Beside the hall door on its inner face.
    DoorInside,
    /// Elsewhere on the hall's walls.
    Hall,
    /// On the market's walls.
    Market,
}

/// A banner on a wall: where its rod hangs, at a point in grid coordinates just off the wall,
/// and the way it faces, out from the wall (`yaw` as [`crate::PropSpot::yaw`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BannerSpot {
    pub x: f32,
    pub y: f32,
    pub yaw: f32,
    /// Rolls the end, length, hem and fringe.
    pub seed: u32,
    /// Whose colours it flies: one of the level's factions.
    pub faction: Faction,
    pub site: BannerSite,
    /// The hall or market it hangs in, or the hall whose door it flanks.
    pub room: usize,
}

/// How far a flanking banner's centre stands from the cell's middle toward the open side, in
/// world units, so wide cloth clears the door frame and the rock that bulges by the jamb.
const JAMB_CLEARANCE: f32 = 2.5;
/// Half the masonry through a doorway's cell, and the rod's stand-off from it, in world units:
/// a lintel banner hangs on the corridor face of the doorway's infill.
const LINTEL_FACE: f32 = 1.5 + 0.9;

/// The banner on the face of `wall` (a grid step) from floor cell `cell`, if there is rock
/// there to hang it on.
fn on_wall(level: &Level, cell: (i32, i32), wall: (i32, i32), shift: (i32, i32), faction: Faction, site: BannerSite, room: usize, rng: &mut Mulberry32) -> Option<BannerSpot> {
    if level.cell(cell.0, cell.1) != Cell::Floor || level.cell(cell.0 + wall.0, cell.1 + wall.1) != Cell::Rock {
        return None;
    }
    let out = 0.5 - WALL_OFFSET / CELL;
    let slide = JAMB_CLEARANCE / CELL;
    let (x, y) = (cell.0 as f32 + 0.5 + wall.0 as f32 * out + shift.0 as f32 * slide, cell.1 as f32 + 0.5 + wall.1 as f32 * out + shift.1 as f32 * slide);
    let yaw = (-wall.0 as f32).atan2(-wall.1 as f32);
    Some(BannerSpot { x, y, yaw, seed: item_seed(rng), faction, site, room })
}

/// Hangs every claim's banners.
pub(crate) fn place_banners(level: &Level) -> Vec<BannerSpot> {
    let mut rng = Mulberry32::new(level.seed ^ BANNER_SALT);
    let graph = RoomGraph::new(level);
    let (sx, sy, _) = level.start();
    let mut banners: Vec<BannerSpot> = Vec::new();
    for claim in &level.claims {
        let faction = claim.faction;
        let door = main_door(level, &graph, claim, (sx as i32, sy as i32));
        if let Some(door) = door {
            let (i, o) = (door.inside, door.outside);
            let d = (o.0 - i.0, o.1 - i.1);
            let along = (d.1, d.0);
            let beyond = (o.0 + d.0, o.1 + d.1);
            let mut outside = 0;
            for side in [1, -1] {
                let s = (along.0 * side, along.1 * side);
                // Inside: on the wall the door is cut in, a cell to each side of it, the
                // cloth moved off the jamb.
                if let Some(b) = on_wall(level, (i.0 + s.0, i.1 + s.1), d, s, faction, BannerSite::DoorInside, door.room, &mut rng) {
                    banners.push(b);
                }
                // Outside, where a room or a widening lies: on the same wall's outer face.
                if let Some(b) = on_wall(level, (beyond.0 + s.0, beyond.1 + s.1), (-d.0, -d.1), s, faction, BannerSite::DoorOutside, door.room, &mut rng) {
                    banners.push(b);
                    outside += 1;
                }
            }
            // A straight corridor has nothing to hang flanking cloth on that faces the
            // approach: one banner over the lintel instead, facing down the corridor.
            if outside == 0 && level.cell(o.0, o.1) == Cell::Door {
                let face = LINTEL_FACE / CELL;
                let (x, y) = (o.0 as f32 + 0.5 + d.0 as f32 * face, o.1 as f32 + 0.5 + d.1 as f32 * face);
                let yaw = (d.0 as f32).atan2(d.1 as f32);
                banners.push(BannerSpot { x, y, yaw, seed: item_seed(&mut rng), faction, site: BannerSite::Lintel, room: door.room });
            }
        }
        // More inside the hall and the market, on bare walls clear of the doors.
        for (is_hall, site) in [(true, BannerSite::Hall), (false, BannerSite::Market)] {
            // The market, or the first workshop of a claim that keeps them.
            let Some(r) = (if is_hall { level.room_with_role(claim, RoomRole::Hall) } else { level.trade_room(claim) }) else { continue };
            let room = level.rooms[r];
            let mut want = rng.int(1, 2);
            if !is_hall && level.roles_of(r).contains(&RoomRole::Hall) {
                want = 1;
            }
            let mut cells: Vec<((i32, i32), (i32, i32))> = Vec::new();
            for y in room.y as i32..(room.y + room.h) as i32 {
                for x in room.x as i32..(room.x + room.w) as i32 {
                    if level.cell(x, y) != Cell::Floor || by_door(level, x, y) {
                        continue;
                    }
                    for f in Facing::ALL {
                        let w = f.step();
                        if level.cell(x + w.0, y + w.1) == Cell::Rock {
                            cells.push(((x, y), w));
                        }
                    }
                }
            }
            shuffle(&mut cells, &mut rng);
            // In the hall, the wall facing the door first, so the banners are seen on coming in.
            if let (true, Some(door)) = (is_hall, door) {
                let back = (door.inside.0 - door.outside.0, door.inside.1 - door.outside.1);
                cells.sort_by_key(|&(_, wall)| wall != back);
            }
            let mut hung = 0;
            for (cell, wall) in cells {
                if hung == want {
                    break;
                }
                let Some(b) = on_wall(level, cell, wall, (0, 0), faction, site, r, &mut rng) else { continue };
                let near = |ax: f32, ay: f32, gap: f32| (ax - b.x).hypot(ay - b.y) < gap;
                let crowded = banners.iter().any(|o| near(o.x, o.y, APART))
                    || level.lights.iter().any(|l| near(l.x, l.y, CLEAR))
                    || level.props.iter().any(|p| near(p.x, p.y, CLEAR));
                if !crowded {
                    banners.push(b);
                    hung += 1;
                }
            }
        }
    }
    banners
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate;

    fn sweep() -> impl Iterator<Item = (u32, u32)> {
        [1, 2, 3].into_iter().flat_map(|depth| (1..=64).map(move |seed| (seed, depth)))
    }

    #[test]
    fn halls_hang_their_factions_banners_at_the_door() {
        let (mut halls, mut flanked) = (0, 0);
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for claim in &level.claims {
                let hall = level.room_with_role(claim, RoomRole::Hall).unwrap();
                let at = |site| level.banners.iter().filter(|b| b.faction == claim.faction && b.site == site && b.room == hall).count();
                halls += 1;
                // Where the wall allows: most halls hang one each side of the door inside and
                // out, and none hangs more than two a side.
                assert!(at(BannerSite::DoorInside) <= 2 && at(BannerSite::DoorOutside) <= 2, "seed {seed} depth {depth}");
                // A corridor-fed door has exactly one outside banner, over its lintel, facing
                // down the corridor; where a room lies outside the flank pair hangs instead.
                let lintels = at(BannerSite::Lintel);
                assert!(lintels <= 1 && (lintels == 0 || at(BannerSite::DoorOutside) == 0), "seed {seed} depth {depth}");
                flanked += (at(BannerSite::DoorInside) >= 1 && at(BannerSite::DoorOutside) + lintels >= 1) as u32;
            }
        }
        assert!(halls > 100 && flanked * 10 >= halls * 7, "{flanked} of {halls} halls are flanked inside and out");
    }

    #[test]
    fn banners_fly_a_faction_of_the_level_and_hang_on_a_wall() {
        let mut total = 0;
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for b in level.banners.iter().filter(|b| b.site != BannerSite::Lintel) {
                total += 1;
                assert!(level.factions.iter().any(|f| f.id == b.faction), "seed {seed} depth {depth}: {b:?}");
                // The rod stands in an open cell with rock behind it.
                let (bx, by) = (-b.yaw.sin().round() as i32, -b.yaw.cos().round() as i32);
                let (cx, cy) = (b.x.floor() as i32, b.y.floor() as i32);
                assert_eq!(level.cell(cx, cy), Cell::Floor, "seed {seed} depth {depth}: {b:?}");
                assert_eq!(level.cell(cx + bx, cy + by), Cell::Rock, "seed {seed} depth {depth}: {b:?}");
            }
        }
        assert!(total > 400, "{total} banners");
    }

    #[test]
    fn no_banner_hangs_in_a_doorway_or_beside_a_door_but_the_doors_own() {
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for b in level.banners.iter().filter(|b| b.site != BannerSite::Lintel) {
                assert_ne!(level.cell(b.x.floor() as i32, b.y.floor() as i32), Cell::Door, "seed {seed} depth {depth}: {b:?}");
                if matches!(b.site, BannerSite::Hall | BannerSite::Market) {
                    assert!(!by_door(&level, b.x.floor() as i32, b.y.floor() as i32), "seed {seed} depth {depth}: {b:?}");
                }
            }
        }
    }

    #[test]
    fn a_corridor_fed_door_has_one_banner_over_its_lintel_facing_the_approach() {
        let mut lintels = 0;
        for (seed, depth) in sweep() {
            let level = generate(seed, depth);
            for b in level.banners.iter().filter(|b| b.site == BannerSite::Lintel) {
                lintels += 1;
                let (cx, cy) = (b.x.floor() as i32, b.y.floor() as i32);
                assert_eq!(level.cell(cx, cy), Cell::Door, "seed {seed} depth {depth}: {b:?}");
                // It faces along the corridor, into open floor.
                let (fx, fy) = (b.yaw.sin().round() as i32, b.yaw.cos().round() as i32);
                assert!(level.is_open(cx + fx, cy + fy), "seed {seed} depth {depth}: {b:?}");
                assert_eq!(level.banners.iter().filter(|o| o.room == b.room && o.site == BannerSite::DoorOutside).count(), 0);
            }
        }
        assert!(lintels > 20, "{lintels} lintel banners");
    }

    #[test]
    fn banners_are_the_same_for_the_same_seed() {
        for (seed, depth) in sweep().take(30) {
            assert_eq!(generate(seed, depth).banners, generate(seed, depth).banners);
        }
    }
}
