//! Daises: a part of a stone hall's floor stepped a few stairs up, or sunk a few stairs down.
//! They are decoration only: the cells stay floor to walking, pathing and sight, and the room
//! places whatever it places on them as it did before.
//!
//! A shrine's altar and a throne room's throne always stand on a raised dais against their
//! wall, with the braziers beside them on its top. Some colonnaded halls get one too: a raised
//! platform across the end facing their main door, or the floor between the colonnades sunk.
//!
//! Whether a hall has one, raised or sunk, its steps and its size come from a hash of the level
//! seed and the room, drawn from no stream, like [`Level::hall_is_fluted`]: placing daises moves
//! nothing else about a seed. Only levels of stone halls have them. The stairs keep off them.
//!
//! The steps are one [`RISE`] high and one [`TREAD`] deep, laid inside the dais's rectangle on
//! the sides open to the room. Walking height is smooth across a riser ([`Dais::height_at`]);
//! the drawn stone is crisp ([`Dais::treads`]).

use serde::{Deserialize, Serialize};

use crate::{CELL, Cell, Facing, Level, LevelStyle};

/// How high one step rises, in world units (20 cm).
pub const RISE: f32 = 2.0;
/// How deep one step's tread is, in world units (50 cm).
pub const TREAD: f32 = 5.0;
/// The run over which the walking height climbs a riser, just inside it.
pub const RAMP: f32 = 2.4;

/// Salt for the hash that decides a hall's dais.
const DAIS_SALT: u32 = 0xda15_0001;

/// A rectangle of floor cells stepped up or down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dais {
    /// The cell rectangle, `w` by `h` from `(x, y)`.
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// How many steps, one to three.
    pub steps: u8,
    /// Sunk below the floor instead of raised above it.
    pub sunk: bool,
    /// Which sides have steps, north, east, south, west: those with open floor beyond them.
    /// A side against a wall has none.
    pub stepped: [bool; 4],
}

/// One flat surface of a dais as drawn: a world rectangle `[x0, x1, z0, z1]` with its top at
/// `top` (negative in a sunk dais).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tread {
    pub rect: [f32; 4],
    pub top: f32,
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Dais {
    /// Whether cell `(cx, cy)` lies in the rectangle.
    pub fn has_cell(&self, cx: i32, cy: i32) -> bool {
        cx >= self.x as i32 && cy >= self.y as i32 && cx < (self.x + self.w) as i32 && cy < (self.y + self.h) as i32
    }

    /// The rectangle in world units, `[x0, x1, z0, z1]`.
    pub fn rect(&self) -> [f32; 4] {
        [self.x as f32 * CELL, (self.x + self.w) as f32 * CELL, self.y as f32 * CELL, (self.y + self.h) as f32 * CELL]
    }

    /// The signed height of the top step: positive raised, negative sunk.
    pub fn top_height(&self) -> f32 {
        let h = self.steps as f32 * RISE;
        if self.sunk { -h } else { h }
    }

    /// The rectangle of the `k`th step (1 the outermost), `1..=steps`: the dais drawn in by a
    /// tread for each step before it on the sides that are stepped.
    pub fn region(&self, k: u8) -> [f32; 4] {
        let [x0, x1, z0, z1] = self.rect();
        let inset = (k.saturating_sub(1)) as f32 * TREAD;
        let s = |i: usize| if self.stepped[i] { inset } else { 0.0 };
        [x0 + s(3), x1 - s(1), z0 + s(0), z1 - s(2)]
    }

    /// The height of the drawn tread under world `(x, z)`, signed, 0 off the dais.
    pub fn crisp_at(&self, x: f32, z: f32) -> f32 {
        let inside = |r: [f32; 4]| x >= r[0] && x < r[1] && z >= r[2] && z < r[3];
        let k = (1..=self.steps).rev().find(|&k| inside(self.region(k))).unwrap_or(0);
        let h = k as f32 * RISE;
        if self.sunk { -h } else { h }
    }

    /// The flat surfaces the dais is drawn with, each at its crisp height. A raised dais is
    /// nested boxes, the `k`th at `k` rises; a sunk one is the strip of each step and the
    /// bottom, so no tread covers a lower one.
    pub fn treads(&self) -> Vec<Tread> {
        let mut out = Vec::new();
        for k in 1..=self.steps {
            let outer = self.region(k);
            if !self.sunk {
                out.push(Tread { rect: outer, top: k as f32 * RISE });
                continue;
            }
            let top = -(k as f32) * RISE;
            if k == self.steps {
                out.push(Tread { rect: outer, top });
                continue;
            }
            let inner = self.region(k + 1);
            // The ring between the two: the north and south strips across the whole width, the
            // west and east strips between them.
            let [ox0, ox1, oz0, oz1] = outer;
            let [ix0, ix1, iz0, iz1] = inner;
            for rect in [[ox0, ox1, oz0, iz0], [ox0, ox1, iz1, oz1], [ox0, ix0, iz0, iz1], [ix1, ox1, iz0, iz1]] {
                if rect[1] - rect[0] > 0.05 && rect[3] - rect[2] > 0.05 {
                    out.push(Tread { rect, top });
                }
            }
        }
        out
    }

    /// How far inside the stepped edges the world point is: the least distance to a stepped
    /// side. `None` off the dais.
    fn depth_in(&self, x: f32, z: f32) -> Option<f32> {
        let [x0, x1, z0, z1] = self.rect();
        if x < x0 || x > x1 || z < z0 || z > z1 {
            return None;
        }
        let sides = [z - z0, x1 - x, z1 - z, x - x0];
        Some(sides.iter().enumerate().filter(|&(i, _)| self.stepped[i]).map(|(_, &d)| d).fold(f32::INFINITY, f32::min))
    }

    /// The height of the floor to walk on over world `(x, z)` relative to the level's floor, 0
    /// off the dais: smooth, climbing each riser over the [`RAMP`] just inside it, so the body
    /// rises as it steps onto a tread. Signed: negative when sunk.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let Some(depth) = self.depth_in(x, z) else { return 0.0 };
        let mut up = 0.0;
        for k in 0..self.steps {
            up += RISE * smooth((depth - k as f32 * TREAD) / RAMP);
        }
        if self.sunk { -up } else { up }
    }
}

impl Level {
    /// The height of the drawn tread under world `(x, z)`, for what stands still on a dais.
    pub fn dais_floor(&self, x: f32, z: f32) -> f32 {
        self.daises.iter().map(|d| d.crisp_at(x, z)).find(|&h| h != 0.0).unwrap_or(0.0)
    }

    /// The dais over cell `(cx, cy)`, if it has one.
    pub fn dais_of_cell(&self, cx: i32, cy: i32) -> Option<&Dais> {
        self.daises.iter().find(|d| d.has_cell(cx, cy))
    }

    /// Whether cell `(cx, cy)` is in a dais.
    pub fn on_dais(&self, cx: i32, cy: i32) -> bool {
        self.dais_of_cell(cx, cy).is_some()
    }

    /// How far the floor to walk on stands above (or below, negative) the level's floor level at
    /// world `(x, z)`: the dais under it, smoothly, and 0 on a level with none.
    pub fn dais_height(&self, x: f32, z: f32) -> f32 {
        if self.daises.is_empty() {
            return 0.0;
        }
        self.daises.iter().map(|d| d.height_at(x, z)).find(|&h| h != 0.0).unwrap_or(0.0)
    }
}

/// A well-mixed hash of the level seed, `salt` and a room's rectangle.
fn mix(level: &Level, salt: u32, room: usize) -> u32 {
    let r = level.rooms[room];
    let mut h = level.seed ^ salt.wrapping_mul(0x9e37_79b1);
    for v in [r.x, r.y, r.w, r.h] {
        h = (h ^ v).wrapping_mul(0x85eb_ca6b);
        h ^= h >> 15;
    }
    h = (h ^ (h >> 13)).wrapping_mul(0xc2b2_ae35);
    h ^ (h >> 16)
}

/// The daises of `level`, a pure function of it: none unless it is a level of stone halls.
pub(crate) fn place_daises(level: &Level) -> Vec<Dais> {
    if level.style != LevelStyle::StoneHalls {
        return Vec::new();
    }
    let mut out = Vec::new();
    for r in 0..level.rooms.len() {
        let h = mix(level, DAIS_SALT, r);
        let dais = match level.purpose_of(r) {
            Some(crate::Purpose::ThroneRoom) => seat_dais(level, r, 1 + (h >> 4) % 3),
            Some(crate::Purpose::Shrine) => seat_dais(level, r, 1 + (h >> 4) % 2 + (h >> 9) % 2),
            _ if crate::generate::is_colonnaded(level, &level.rooms[r]) => match h % 100 {
                0..=17 => platform(level, r, 1 + (h >> 8) % 3),
                18..=35 => sunk_floor(level, r, 1 + (h >> 8) % 3),
                _ => None,
            },
            _ => None,
        };
        out.extend(dais);
    }
    out
}

/// Whether the cells of `x0..x1` by `y0..y1` can be a dais: all floor (or, for a platform, a
/// column too) of one room, and clear of what the level has already placed, which a dais never
/// moves: no doorway within a cell of them (a door's sweep), no stair or its approach within a
/// cell, no banner hung over one, and not the way in.
fn fit(level: &Level, room: usize, [x0, y0, x1, y1]: [i32; 4], columns: bool) -> bool {
    let (sx, sy, _) = level.start();
    let taken = |x: i32, y: i32| {
        level.stairs.iter().any(|s| {
            let (ax, ay) = s.approach();
            [(s.x, s.y), (ax, ay)].iter().any(|&(px, py)| (px as i32 - x).abs().max((py as i32 - y).abs()) <= 1)
        }) || level.banners.iter().any(|b| (b.x.floor() as i32, b.y.floor() as i32) == (x, y))
            || (sx as i32 - x).abs().max((sy as i32 - y).abs()) <= 1
    };
    (y0..y1).all(|y| {
        (x0..x1).all(|x| {
            let cell = level.cell(x, y);
            let ok = cell == Cell::Floor || (columns && cell == Cell::Column);
            ok && level.room_at(x, y) == Some(room)
                && !taken(x, y)
                && (-1..=1).all(|dy| (-1..=1).all(|dx| !level.cell(x + dx, y + dy).is_doorway()))
        })
    })
}

/// A dais on the cells `x0..x1` by `y0..y1`, with steps on the sides that have open floor
/// beyond them.
fn build(level: &Level, [x0, y0, x1, y1]: [i32; 4], steps: u32, sunk: bool) -> Dais {
    let open = |x: i32, y: i32| level.cell(x, y) != Cell::Rock;
    let stepped = [
        (x0..x1).any(|x| open(x, y0 - 1)),
        (y0..y1).any(|y| open(x1, y)),
        (x0..x1).any(|x| open(x, y1)),
        (y0..y1).any(|y| open(x0 - 1, y)),
    ];
    let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
    // Steps on both sides of a dais must leave a top to stand on.
    let narrow = w.min(h) as f32 * CELL;
    let fits = ((narrow * 0.5 - 4.0) / TREAD).floor().max(1.0) as u32;
    Dais { x: x0 as u32, y: y0 as u32, w, h, steps: steps.min(fits).clamp(1, 3) as u8, sunk, stepped }
}

/// The raised dais under a throne or an altar: the biggest of a few sizes that fits against
/// its wall, centred on the seat. Wide enough for the braziers either side to stand on its top.
fn seat_dais(level: &Level, room: usize, steps: u32) -> Option<Dais> {
    let ((sx, sy), back) = crate::past::seat(level, room)?;
    let (bx, by) = back.step();
    let (ax, ay) = (-by, bx);
    // The biggest rectangle against the wall that holds the seat and clears what is placed: the
    // seat's three cells either side first, then lopsided ones, then the narrower.
    let mut sizes: Vec<(i32, i32, i32)> = Vec::new();
    for depth in 1..=3 {
        for left in 0..=3 {
            for right in 0..=3 {
                if (left + right + 1) * depth >= 3 {
                    sizes.push((left, right, depth));
                }
            }
        }
    }
    sizes.sort_by_key(|&(l, r, d)| (std::cmp::Reverse((l + r + 1) * d), (l - r).abs(), std::cmp::Reverse(d)));
    for (left, right, depth) in sizes {
        let corners = [(-left, 0), (right, depth - 1)].map(|(k, d)| (sx + ax * k - bx * d, sy + ay * k - by * d));
        let rect = [corners[0].0.min(corners[1].0), corners[0].1.min(corners[1].1), corners[0].0.max(corners[1].0) + 1, corners[0].1.max(corners[1].1) + 1];
        if fit(level, room, rect, true) {
            return Some(build(level, rect, steps, false));
        }
    }
    None
}

/// A raised platform across the end of a colonnaded hall facing its main door, two cells deep.
fn platform(level: &Level, room: usize, steps: u32) -> Option<Dais> {
    let (_, back) = crate::past::seat(level, room)?;
    let r = level.rooms[room];
    let (x0, y0, x1, y1) = (r.x as i32, r.y as i32, (r.x + r.w) as i32, (r.y + r.h) as i32);
    let depth = 2;
    let rect = match back {
        Facing::North => [x0, y0, x1, y0 + depth],
        Facing::South => [x0, y1 - depth, x1, y1],
        Facing::West => [x0, y0, x0 + depth, y1],
        Facing::East => [x1 - depth, y0, x1, y1],
    };
    fit(level, room, rect, true).then(|| build(level, rect, steps, false))
}

/// The floor between the colonnades sunk, a cell in from the ends of the hall.
fn sunk_floor(level: &Level, room: usize, steps: u32) -> Option<Dais> {
    let r = level.rooms[room];
    let (x0, y0, x1, y1) = (r.x as i32, r.y as i32, (r.x + r.w) as i32, (r.y + r.h) as i32);
    // The columns stand in the second row from each long wall; the aisle is what lies between.
    let rect = if r.w >= r.h { [x0 + 1, y0 + 2, x1 - 1, y1 - 2] } else { [x0 + 2, y0 + 1, x1 - 2, y1 - 1] };
    if rect[2] - rect[0] < 2 || rect[3] - rect[1] < 2 {
        return None;
    }
    if !fit(level, room, rect, false) {
        return None;
    }
    // Every side of a pit is stepped (open floor beyond it), so its rim wall is drawn all round.
    let dais = build(level, rect, steps, true);
    dais.stepped.iter().all(|&s| s).then_some(dais)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate;

    fn stone_levels() -> impl Iterator<Item = Level> {
        (1..=60).flat_map(|seed| [2, 4].map(|depth| generate(seed, depth)))
    }

    #[test]
    fn daises_are_the_same_every_time() {
        for seed in 1..=20 {
            assert_eq!(generate(seed, 1).daises, generate(seed, 1).daises, "seed {seed}");
        }
    }

    #[test]
    fn every_shrine_and_throne_stands_on_a_raised_dais_when_it_fits() {
        let (mut seats, mut raised) = (0, 0);
        for level in stone_levels() {
            for r in 0..level.rooms.len() {
                if !matches!(level.purpose_of(r), Some(crate::Purpose::ThroneRoom | crate::Purpose::Shrine)) {
                    continue;
                }
                let Some(((x, y), _)) = crate::past::seat(&level, r) else { continue };
                seats += 1;
                if let Some(d) = level.dais_of_cell(x, y) {
                    assert!(!d.sunk && (1..=3).contains(&d.steps), "seed {}", level.seed);
                    raised += 1;
                }
            }
        }
        assert!(seats > 0 && raised * 4 >= seats * 3, "{raised} of {seats} seats raised");
    }

    #[test]
    fn some_halls_have_a_platform_and_some_a_sunk_floor() {
        let all: Vec<Dais> = (1..=150).flat_map(|s| [2, 4].map(|d| generate(s, d))).flat_map(|l| l.daises).collect();
        assert!(all.iter().any(|d| !d.sunk), "no raised dais");
        assert!(all.iter().any(|d| d.sunk), "no sunk floor");
        assert!(all.iter().all(|d| (1..=3).contains(&d.steps)));
    }

    #[test]
    fn a_dais_is_floor_without_doors_beside_it_and_no_two_share_a_room() {
        for level in stone_levels() {
            let mut rooms = Vec::new();
            for d in &level.daises {
                let room = level.room_at(d.x as i32, d.y as i32).unwrap();
                assert!(!rooms.contains(&room), "two daises in room {room}");
                rooms.push(room);
                for y in d.y as i32..(d.y + d.h) as i32 {
                    for x in d.x as i32..(d.x + d.w) as i32 {
                        assert!(level.cell(x, y).is_open() || level.cell(x, y) == Cell::Column);
                        assert!(Facing::ALL.iter().all(|f| !level.cell(x + f.step().0, y + f.step().1).is_doorway()));
                    }
                }
            }
        }
    }

    #[test]
    fn no_stair_stands_on_a_dais_and_other_levels_have_none() {
        for seed in 1..=80 {
            for depth in 1..=8 {
                let level = generate(seed, depth);
                if level.style != LevelStyle::StoneHalls {
                    assert!(level.daises.is_empty());
                }
                for b in &level.banners {
                    assert!(!level.on_dais(b.x.floor() as i32, b.y.floor() as i32), "seed {seed} depth {depth}: a banner over a dais");
                }
                for s in &level.stairs {
                    assert!(!level.on_dais(s.x as i32, s.y as i32), "seed {seed} depth {depth}");
                    let (ax, ay) = s.approach();
                    assert!(!level.on_dais(ax as i32, ay as i32), "seed {seed} depth {depth}");
                }
            }
        }
    }

    #[test]
    fn the_walking_height_climbs_each_step_and_is_smooth() {
        let up = Dais { x: 2, y: 2, w: 5, h: 3, steps: 3, sunk: false, stepped: [false, true, true, true] };
        let down = Dais { sunk: true, ..up };
        let [x0, _, z0, z1] = up.rect();
        let z = (z0 + z1) * 0.5;
        assert_eq!(up.height_at(x0 - 4.0, z), 0.0);
        assert!((up.height_at(x0 + RAMP, z) - RISE).abs() < 1e-4, "{}", up.height_at(x0 + RAMP, z));
        assert!((up.height_at(x0 + TREAD + RAMP, z) - 2.0 * RISE).abs() < 1e-4);
        assert!((up.height_at(x0 + 40.0, z) - 3.0 * RISE).abs() < 1e-5);
        assert!((down.height_at(x0 + 40.0, z) + 3.0 * RISE).abs() < 1e-5);
        // No step between neighbouring points is more than a fraction of a rise.
        let mut last = up.height_at(x0 - 4.0, z);
        let mut x = x0 - 4.0;
        while x < x0 + 40.0 {
            x += 0.1;
            let h = up.height_at(x, z);
            assert!((h - last).abs() < 0.2, "jump at {x}");
            last = h;
        }
        // The side against the wall is flush: full height right to its edge.
        assert!((up.height_at(x0 + 40.0, z0 + 0.1) - 3.0 * RISE).abs() < 1e-5);
    }

    #[test]
    fn every_sunk_floor_is_stepped_on_all_four_sides() {
        let mut sunk = 0;
        for level in stone_levels() {
            for d in level.daises.iter().filter(|d| d.sunk) {
                assert_eq!(d.stepped, [true; 4], "seed {}", level.seed);
                sunk += 1;
            }
        }
        assert!(sunk > 0);
    }

    #[test]
    fn a_thing_set_down_rests_on_the_tread_at_its_front_edge() {
        let up = Dais { x: 2, y: 2, w: 5, h: 3, steps: 3, sunk: false, stepped: [false, true, true, true] };
        let [x0, _, z0, z1] = up.rect();
        let z = (z0 + z1) * 0.5;
        // Just inside each riser the drawn tread is the whole rise; the walking floor is still climbing.
        for k in 0..3 {
            let x = x0 + k as f32 * TREAD + 0.1;
            assert_eq!(up.crisp_at(x, z), (k + 1) as f32 * RISE);
            assert!(up.height_at(x, z) < up.crisp_at(x, z) - 0.5 * RISE);
        }
        let down = Dais { sunk: true, ..up };
        assert_eq!(down.crisp_at(x0 + 0.1, z), -RISE);
    }

    #[test]
    fn treads_cover_the_dais_and_a_level_reads_the_height_it_stands_on() {
        for level in stone_levels() {
            for d in &level.daises {
                let treads = d.treads();
                assert!(!treads.is_empty());
                let top = treads.iter().map(|t| t.top).fold(0.0f32, |a, b| if b.abs() > a.abs() { b } else { a });
                assert!((top - d.top_height()).abs() < 1e-5);
                let [x0, x1, z0, z1] = d.rect();
                let mid = level.dais_height((x0 + x1) * 0.5, (z0 + z1) * 0.5);
                assert!(mid.abs() > 0.0 && mid.signum() == d.top_height().signum(), "seed {}", level.seed);
            }
        }
    }
}
