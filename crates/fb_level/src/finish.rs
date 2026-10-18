//! What each room and corridor is made of: the stone of its walls, how its floor is laid and how
//! damp it is, and a style for every doorway. [`finishes`] is a pure function of a level and
//! its [`LevelParams`], drawn from a stream of its own, so choosing finishes never moves a
//! layout, item, light or creature. The game and the level view draw from it.
//!
//! A room's cells share one finish; the corridors share the level's corridor finish. Moss is the
//! one thing that varies cell by cell: a low-frequency damp field over the level, a few rooms
//! that are grottos, and corridors that stay mostly dry.

use procgen_rng::Mulberry32;

use crate::{CELL, Cell, Level, LevelParams, LevelStyle};

/// Salt for the finish stream, apart from the layout's and every other stream's.
const FINISH_SALT: u32 = 0xF1A5_7011;
/// Cells between the lattice points of the damp field: its patches are about this wide.
const DAMP_SPAN: f32 = 8.0;
/// Corridors keep this share of the damp field's moss: mostly dry.
const CORRIDOR_DRYNESS: f32 = 0.35;
/// Rough rock cuts its own arches this share as often as stone halls build theirs.
const ROCK_CUT_SHARE: f64 = 1.0 / 3.0;

/// What a room's or corridor's walls are made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WallFinish {
    /// Craggy cave rock, the rough rock levels' own.
    Rock,
    /// Layered cave rock.
    Strata,
    /// Cut and coursed blocks, the stone halls' own.
    Ashlar,
    /// Rough-coursed rubble.
    Rubble,
}

/// How a floor is laid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloorFinish {
    /// Bare cave floor.
    Rock,
    Flagstone,
    Cobble,
}

/// How a doorway is framed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DoorStyle {
    /// A timber frame.
    Timber,
    RoundArch,
    PointedArch,
    /// An arch cut from the living rock.
    RockCut,
}

/// The finish of one open cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Finish {
    pub walls: WallFinish,
    pub floor: FloorFinish,
    /// How mossy the cell is, 0 to 1.
    pub moss: f32,
}

/// The finishes of a level: one per open cell, moss everywhere, and a style per doorway.
#[derive(Clone, Debug, PartialEq)]
pub struct Finishes {
    width: u32,
    height: u32,
    /// Walls and floor per cell, `None` for rock and columns.
    cells: Vec<Option<(WallFinish, FloorFinish)>>,
    /// Moss per cell, rock included so it fades past a wall's foot rather than stopping at it.
    moss: Vec<f32>,
    /// The style of each door cell, `None` elsewhere.
    doors: Vec<Option<DoorStyle>>,
}

impl Finishes {
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32).then(|| (y as u32 * self.width + x as u32) as usize)
    }

    /// The finish of open cell `(x, y)`; `None` for rock, columns and outside the grid.
    pub fn at(&self, x: i32, y: i32) -> Option<Finish> {
        let i = self.index(x, y)?;
        let (walls, floor) = self.cells[i]?;
        Some(Finish { walls, floor, moss: self.moss[i] })
    }

    /// The style of the doorway in cell `(x, y)`; `None` if that cell is not a door.
    pub fn door_style(&self, x: i32, y: i32) -> Option<DoorStyle> {
        self.doors[self.index(x, y)?]
    }

    /// Moss at a world position (x east, z south, units as the level's): the cells' moss
    /// interpolated between their centres, so it fades over a cell or two rather than stopping
    /// at a cell's edge. Outside the grid it is the edge's.
    pub fn moss_at(&self, world_x: f32, world_z: f32) -> f32 {
        let gx = world_x / CELL - 0.5;
        let gy = world_z / CELL - 0.5;
        let (x0, y0) = (gx.floor(), gy.floor());
        let (tx, ty) = (smooth(gx - x0), smooth(gy - y0));
        let at = |x: f32, y: f32| {
            let x = (x as i32).clamp(0, self.width as i32 - 1);
            let y = (y as i32).clamp(0, self.height as i32 - 1);
            self.moss[(y as u32 * self.width + x as u32) as usize]
        };
        let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1.0, y0) * tx;
        let bottom = at(x0, y0 + 1.0) * (1.0 - tx) + at(x0 + 1.0, y0 + 1.0) * tx;
        top * (1.0 - ty) + bottom * ty
    }
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn smoothstep(lo: f32, hi: f32, v: f32) -> f32 {
    smooth(((v - lo) / (hi - lo)).clamp(0.0, 1.0))
}

/// The damp field at grid point `(x, y)` (cells, fractional), 0 to 1: value noise over a
/// lattice [`DAMP_SPAN`] cells apart.
fn damp(seed: u32, x: f32, y: f32) -> f32 {
    let (gx, gy) = (x / DAMP_SPAN, y / DAMP_SPAN);
    let (ix, iy) = (gx.floor(), gy.floor());
    let (tx, ty) = (smooth(gx - ix), smooth(gy - iy));
    let lattice = |dx: i32, dy: i32| {
        let h = seed ^ ((ix as i32 + dx) as u32).wrapping_mul(0x85EB_CA6B) ^ ((iy as i32 + dy) as u32).wrapping_mul(0xC2B2_AE35);
        Mulberry32::new(h).next_f64() as f32
    };
    let top = lattice(0, 0) * (1.0 - tx) + lattice(1, 0) * tx;
    let bottom = lattice(0, 1) * (1.0 - tx) + lattice(1, 1) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// The finishes of `level` under `params`. Deterministic: the same level, seed and depth give
/// the same finishes, and what is drawn per room, per door and for the corridors does not
/// depend on the odds, so moving a slider changes only what that slider governs.
pub fn finishes(level: &Level, params: &LevelParams) -> Finishes {
    let seed = level.seed ^ level.depth.wrapping_mul(0x9E37_79B9) ^ FINISH_SALT;
    let mut rng = Mulberry32::new(seed);
    let rock = level.style == LevelStyle::RoughRock;
    let pick = |rng: &mut Mulberry32| {
        // Walls, floor, then the grotto roll: always three draws, whatever the odds.
        let (w, f, g) = (rng.next_f64(), rng.next_f64(), rng.next_f64());
        let walls = match (rock, w) {
            (true, w) if w < params.strata_chance => WallFinish::Strata,
            (true, _) => WallFinish::Rock,
            (false, w) if w < params.rubble_chance => WallFinish::Rubble,
            (false, _) => WallFinish::Ashlar,
        };
        let floor = match (f < params.cobble_chance, rock) {
            (true, _) => FloorFinish::Cobble,
            (false, true) => FloorFinish::Rock,
            (false, false) => FloorFinish::Flagstone,
        };
        (walls, floor, g < params.grotto_chance)
    };
    let (corridor_walls, corridor_floor, _) = pick(&mut rng);
    let rooms: Vec<_> = level.rooms.iter().map(|_| pick(&mut rng)).collect();

    let (w, h) = (level.width as i32, level.height as i32);
    let mut cells = vec![None; (w * h) as usize];
    let mut moss = vec![0.0f32; (w * h) as usize];
    let mut doors = vec![None; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            if !level.is_open(x, y) {
                continue;
            }
            let i = (y * w + x) as usize;
            let wet = smoothstep(0.5, 0.95, damp(seed, x as f32 + 0.5, y as f32 + 0.5)) * params.moss_amount;
            let (walls, floor, level_moss) = match level.room_at(x, y) {
                Some(r) => {
                    let (walls, floor, grotto) = rooms[r];
                    let grotto_moss = if grotto { (0.7 + 0.3 * wet) * params.moss_amount } else { 0.0 };
                    (walls, floor, wet.max(grotto_moss))
                }
                None => (corridor_walls, corridor_floor, wet * CORRIDOR_DRYNESS),
            };
            cells[i] = Some((walls, floor));
            moss[i] = level_moss.clamp(0.0, 1.0);
        }
    }
    // Rock and columns take the moss of the open cells beside them, so it runs to a wall's foot
    // and the interpolation between cells does not fade it to nothing there.
    let open_moss = moss.clone();
    for y in 0..h {
        for x in 0..w {
            if level.is_open(x, y) {
                continue;
            }
            let near = (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (x + dx, y + dy)));
            moss[(y * w + x) as usize] = near.filter(|&(nx, ny)| level.is_open(nx, ny)).map(|(nx, ny)| open_moss[(ny * w + nx) as usize]).fold(0.0, f32::max);
        }
    }
    // Doorways in cell order, two draws each; the entrance keeps its timber frame, which its
    // door hangs from.
    let entrance = (level.entrance.x as i32, level.entrance.y as i32);
    for y in 0..h {
        for x in 0..w {
            if level.cell(x, y) != Cell::Door {
                continue;
            }
            let i = (y * w + x) as usize;
            let (a, b) = (rng.next_f64(), rng.next_f64());
            doors[i] = Some(if (x, y) == entrance {
                DoorStyle::Timber
            } else if rock {
                if a < params.arch_chance * ROCK_CUT_SHARE { DoorStyle::RockCut } else { DoorStyle::Timber }
            } else if a < params.arch_chance {
                if b < 0.5 { DoorStyle::RoundArch } else { DoorStyle::PointedArch }
            } else {
                DoorStyle::Timber
            });
        }
    }
    // The entrance's doorway is not a door cell, but it has its timber frame all the same.
    if let Some(entry) = doors.get_mut((entrance.1 * w + entrance.0) as usize) {
        *entry = Some(DoorStyle::Timber);
    }
    Finishes { width: level.width, height: level.height, cells, moss, doors }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generate_with, LevelStyle};
    use std::collections::HashSet;

    fn level_at(seed: u32, depth: u32) -> (Level, LevelParams) {
        let params = LevelParams::for_style(LevelStyle::for_depth(depth));
        (generate_with(seed, depth, &params), params)
    }

    /// No chance, no moss, as the style's plain rooms.
    fn plain(depth: u32) -> LevelParams {
        LevelParams {
            moss_amount: 0.0,
            grotto_chance: 0.0,
            strata_chance: 0.0,
            rubble_chance: 0.0,
            cobble_chance: 0.0,
            arch_chance: 0.0,
            ..LevelParams::for_style(LevelStyle::for_depth(depth))
        }
    }

    #[test]
    fn finishes_are_deterministic() {
        for depth in 1..=2 {
            for seed in 1..=10 {
                let (level, params) = level_at(seed, depth);
                assert_eq!(finishes(&level, &params), finishes(&level, &params));
            }
        }
    }

    #[test]
    fn a_room_shares_one_finish_and_corridors_share_another() {
        for depth in 1..=2 {
            for seed in 1..=20 {
                let (level, params) = level_at(seed, depth);
                let f = finishes(&level, &params);
                let mut rooms: Vec<Option<(WallFinish, FloorFinish)>> = vec![None; level.rooms.len()];
                let mut corridor = None;
                for y in 0..level.height as i32 {
                    for x in 0..level.width as i32 {
                        let Some(c) = f.at(x, y) else {
                            assert!(!level.is_open(x, y));
                            continue;
                        };
                        let kind = (c.walls, c.floor);
                        let slot = match level.room_at(x, y) {
                            Some(r) => &mut rooms[r],
                            None => &mut corridor,
                        };
                        assert_eq!(*slot.get_or_insert(kind), kind, "seed {seed} depth {depth} at {x},{y}");
                    }
                }
            }
        }
    }

    #[test]
    fn every_style_shows_each_of_its_finishes_and_moss_gathers() {
        for depth in [1, 2] {
            let mut walls = HashSet::new();
            let mut floors = HashSet::new();
            let mut doors = HashSet::new();
            let (mut mossy, mut corridor, mut dry_corridor) = (0, 0, 0);
            for seed in 1..=50 {
                let (level, params) = level_at(seed, depth);
                let f = finishes(&level, &params);
                for y in 0..level.height as i32 {
                    for x in 0..level.width as i32 {
                        if let Some(c) = f.at(x, y) {
                            walls.insert(c.walls);
                            floors.insert(c.floor);
                            if c.moss > 0.6 {
                                mossy += 1;
                            }
                            if level.room_at(x, y).is_none() {
                                corridor += 1;
                                dry_corridor += (c.moss < 0.3) as u32;
                            }
                        }
                        doors.extend(f.door_style(x, y));
                    }
                }
            }
            let (want_walls, want_floors): (&[_], &[_]) = if depth == 1 {
                (&[WallFinish::Rock, WallFinish::Strata], &[FloorFinish::Rock, FloorFinish::Cobble])
            } else {
                (&[WallFinish::Ashlar, WallFinish::Rubble], &[FloorFinish::Flagstone, FloorFinish::Cobble])
            };
            assert_eq!(walls, want_walls.iter().copied().collect(), "depth {depth} walls");
            assert_eq!(floors, want_floors.iter().copied().collect(), "depth {depth} floors");
            assert!(mossy > 0, "depth {depth} has no moss above 0.6");
            assert!(dry_corridor as f32 > 0.8 * corridor as f32, "depth {depth}: {dry_corridor} of {corridor} corridor cells dry");
            let want_doors: HashSet<_> = if depth == 1 { [DoorStyle::Timber, DoorStyle::RockCut].into() } else { [DoorStyle::Timber, DoorStyle::RoundArch, DoorStyle::PointedArch].into() };
            assert_eq!(doors, want_doors, "depth {depth} doors");
        }
    }

    #[test]
    fn the_entrance_is_always_timber() {
        for depth in 1..=4 {
            for seed in 1..=50 {
                let (level, params) = level_at(seed, depth);
                let (x, y) = (level.entrance.x as i32, level.entrance.y as i32);
                assert_eq!(finishes(&level, &params).door_style(x, y), Some(DoorStyle::Timber), "seed {seed} depth {depth}");
                // Even with every door an arch.
                let arched = LevelParams { arch_chance: 1.0, ..params };
                assert_eq!(finishes(&level, &arched).door_style(x, y), Some(DoorStyle::Timber));
            }
        }
    }

    #[test]
    fn with_every_chance_at_zero_rooms_are_plain() {
        for depth in 1..=2 {
            for seed in 1..=50 {
                let params = plain(depth);
                let level = generate_with(seed, depth, &params);
                let f = finishes(&level, &params);
                let (walls, floor) = if depth == 1 { (WallFinish::Rock, FloorFinish::Rock) } else { (WallFinish::Ashlar, FloorFinish::Flagstone) };
                for y in 0..level.height as i32 {
                    for x in 0..level.width as i32 {
                        if let Some(c) = f.at(x, y) {
                            assert_eq!((c.walls, c.floor, c.moss), (walls, floor, 0.0));
                        }
                        if level.cell(x, y) == Cell::Door || (x, y) == (level.entrance.x as i32, level.entrance.y as i32) {
                            assert_eq!(f.door_style(x, y), Some(DoorStyle::Timber));
                        } else {
                            assert_eq!(f.door_style(x, y), None);
                        }
                        assert_eq!(f.moss_at((x as f32 + 0.5) * CELL, (y as f32 + 0.5) * CELL), 0.0);
                    }
                }
            }
        }
    }

    #[test]
    fn moss_is_smooth_between_cell_centres() {
        let (level, params) = level_at(7, 1);
        let f = finishes(&level, &params);
        // At a cell's centre the value is that cell's; along any line it never jumps.
        let step = CELL / 16.0;
        let mut largest = 0.0f32;
        for y in 0..level.height as i32 {
            for x in 0..level.width as i32 - 1 {
                let wz = (y as f32 + 0.5) * CELL;
                let mut last = f.moss_at(x as f32 * CELL + CELL / 2.0, wz);
                for k in 1..=16 {
                    let v = f.moss_at(x as f32 * CELL + CELL / 2.0 + k as f32 * step, wz);
                    largest = largest.max((v - last).abs());
                    last = v;
                }
            }
        }
        assert!(largest < 0.2, "moss jumps by {largest} within a sixteenth of a cell");
        let c = f.at(level.start().0 as i32, level.start().1 as i32).unwrap();
        let (sx, sy) = (level.start().0 as f32, level.start().1 as f32);
        assert!((f.moss_at((sx + 0.5) * CELL, (sy + 0.5) * CELL) - c.moss).abs() < 1e-5);
    }

    #[test]
    fn a_grotto_is_mossy_and_fades_into_the_corridor() {
        let params = LevelParams { grotto_chance: 1.0, ..LevelParams::for_style(LevelStyle::RoughRock) };
        let level = generate_with(3, 1, &params);
        let f = finishes(&level, &params);
        let room = level.rooms[0];
        let (cx, cy) = room.centre();
        assert!(f.at(cx as i32, cy as i32).unwrap().moss >= 0.7);
        assert!(f.moss_at(cx * CELL, cy * CELL) >= 0.7);
    }

    #[test]
    fn old_params_without_the_finish_section_still_load() {
        let mut value = serde_json::to_value(LevelParams::default()).unwrap();
        for name in ["moss_amount", "grotto_chance", "strata_chance", "rubble_chance", "cobble_chance", "arch_chance"] {
            value.as_object_mut().unwrap().remove(name);
        }
        let old: LevelParams = serde_json::from_value(value).unwrap();
        assert_eq!(old, LevelParams::default());
    }
}
