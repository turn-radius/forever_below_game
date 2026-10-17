//! The ways between levels as plain mesh data, with no Bevy in them: a stair going down into the
//! floor and a stair going up into the ceiling, each filling one cell of the level
//! ([`fb_level::Stair`]).
//!
//! A stair is built in its own space: the origin on the floor at the cell's centre, `x` across
//! the stair, `y` up, and `z` the way into it, so the approach is at `-z` and the back of the
//! stair at `+z`. [`stair_transform`] puts it in the level. The level's own stone leaves the
//! cell open where the stair goes through: no floor under a way down ([`opens_floor`]) and no
//! ceiling over a way up ([`opens_ceiling`]).

use armory::{
    ItemMesh, MaterialKind,
    build::{parts::{Look, Parts}, plank::plank},
    geom::{MeshBuilder, V3},
};
use fb_level::{CELL, Stair, StairKind};
use procgen_rng::Mulberry32;

use super::geometry::{CEILING_STONE, FLOOR_STONE, WALL_STONE, cell_centre, facing_vector};

/// The stone either side of the way, and behind it, in world units.
const SIDE: f32 = 1.6;
const BACK: f32 = 1.6;
/// How far a way down's parapet stands above the floor.
const PARAPET: f32 = 1.8;
/// How deep a way down's steps go, and the height of one: a flight of this many steps goes
/// `STEPS_DOWN * RISE_DOWN` below the floor, out of the light's reach.
const STEPS_DOWN: usize = 12;
const RISE_DOWN: f32 = 1.7;
/// How high a way up's steps climb past the ceiling, into the dark, and the height of one.
const CLIMB_PAST_CEILING: f32 = 6.0;
const RISE_UP: f32 = 2.0;
/// How far the shaft above a way up goes past the ceiling, closed by a cap.
const SHAFT: f32 = 8.0;

/// Whether the cell's floor is left out because a way down goes through it.
pub fn opens_floor(stair: Option<&Stair>) -> bool {
    stair.is_some_and(|s| s.kind == StairKind::Down)
}

/// Whether the cell's ceiling is left out because a way up goes through it.
pub fn opens_ceiling(stair: Option<&Stair>) -> bool {
    stair.is_some_and(|s| s.kind == StairKind::Up)
}

/// Where the stair's space sits in the level: its cell's centre on the floor, turned so `+z`
/// runs the way the stair faces. Returns the translation and the yaw about +Y.
pub fn stair_transform(stair: &Stair) -> (V3, f32) {
    let [dx, _, dz] = facing_vector(stair.facing);
    // A turn about +Y by `yaw` takes +Z to `(sin yaw, 0, cos yaw)`.
    (cell_centre(stair.x as i32, stair.y as i32), dx.atan2(dz))
}

/// A block of `size` (x, y, z) centred at `at`.
fn block(size: V3, at: V3, bevel: f32) -> MeshBuilder {
    let mut mesh = plank(size[0], size[1], size[2], bevel);
    mesh.translate(at);
    mesh
}

/// The stair of `kind` in a level with this `ceiling`, in the stair's own space. `seed` hand-cuts
/// the steps, a hair uneven.
pub fn stair_mesh(kind: StairKind, ceiling: f32, seed: u32) -> ItemMesh {
    let mut rng = Mulberry32::new(seed ^ 0x57A1_C0DE);
    let half = CELL * 0.5;
    let (mut steps, mut walls) = (MeshBuilder::new(), MeshBuilder::new());
    let across = CELL - 2.0 * SIDE;
    // Where the steps end against the back wall.
    let run_length = CELL - BACK;
    let (bottom, top) = match kind {
        StairKind::Down => (-(STEPS_DOWN as f32 + 1.0) * RISE_DOWN, PARAPET),
        StairKind::Up => (0.0, ceiling + SHAFT),
    };
    match kind {
        StairKind::Down => {
            let run = run_length / STEPS_DOWN as f32;
            for k in 0..STEPS_DOWN {
                let tread = -(k as f32 + 1.0) * RISE_DOWN + (rng.range_f32(0.0, 1.0) - 0.5) * 0.12;
                let (z0, z1) = (-half + k as f32 * run, -half + (k as f32 + 1.0) * run);
                steps.append(&block([across, tread - bottom, z1 - z0], [0.0, (tread + bottom) * 0.5, (z0 + z1) * 0.5], 0.2));
            }
        }
        StairKind::Up => {
            let count = ((ceiling + CLIMB_PAST_CEILING) / RISE_UP).ceil() as usize;
            let run = run_length / count as f32;
            for k in 0..count {
                let tread = (k as f32 + 1.0) * RISE_UP + (rng.range_f32(0.0, 1.0) - 0.5) * 0.12;
                let (z0, z1) = (-half + k as f32 * run, -half + (k as f32 + 1.0) * run);
                steps.append(&block([across, tread, z1 - z0], [0.0, tread * 0.5, (z0 + z1) * 0.5], 0.2));
            }
        }
    }
    // The stringers either side and the wall behind, from the foot of the flight to the top.
    let tall = top - bottom;
    for side in [-1.0f32, 1.0] {
        walls.append(&block([SIDE, tall, CELL], [side * (half - SIDE * 0.5), (top + bottom) * 0.5, 0.0], 0.2));
    }
    walls.append(&block([across, tall, BACK], [0.0, (top + bottom) * 0.5, half - BACK * 0.5], 0.2));

    let mut parts = Parts::new();
    parts.add_look("steps", steps, Look::of(MaterialKind::Stone).tinted(FLOOR_STONE).without_outline());
    parts.add_look("stringers", walls, Look::of(MaterialKind::Stone).tinted(WALL_STONE).without_outline());
    if kind == StairKind::Up {
        // The cap that closes the shaft over the dark, so no sky of nothing shows through it.
        let cap = block([CELL, 1.0, CELL], [0.0, top + 0.5, 0.0], 0.1);
        parts.add_look("cap", cap, Look::of(MaterialKind::Stone).tinted(CEILING_STONE).without_outline());
    }
    parts.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fb_level::Facing;

    fn stair(kind: StairKind, facing: Facing) -> Stair {
        Stair { x: 3, y: 4, facing, kind }
    }

    fn bounds(mesh: &ItemMesh) -> (V3, V3) {
        mesh.bounds().expect("the stair has a shape")
    }

    #[test]
    fn a_way_down_goes_below_the_floor_and_stays_in_its_cell() {
        let mesh = stair_mesh(StairKind::Down, 24.0, 1207);
        let (min, max) = bounds(&mesh);
        assert!(min[1] < -(STEPS_DOWN as f32) * RISE_DOWN, "{min:?}");
        assert!((PARAPET - 0.1..=PARAPET + 0.1).contains(&max[1]), "{max:?}");
        for axis in [0, 2] {
            assert!(min[axis] >= -CELL * 0.5 - 0.01 && max[axis] <= CELL * 0.5 + 0.01, "axis {axis}: {min:?} {max:?}");
        }
        assert!(mesh.parts.iter().all(|p| !p.indices.is_empty()));
    }

    #[test]
    fn a_way_up_climbs_through_the_ceiling_and_is_closed_above_it() {
        for ceiling in [24.0, 30.0] {
            let mesh = stair_mesh(StairKind::Up, ceiling, 1207);
            let (min, max) = bounds(&mesh);
            assert!(min[1].abs() < 0.01, "{min:?}");
            assert!(max[1] > ceiling + SHAFT, "{max:?}");
            assert!(mesh.parts.iter().any(|p| p.name == "cap"));
            // The steps climb past the ceiling.
            let steps = mesh.parts.iter().find(|p| p.name == "steps").unwrap();
            assert!(steps.positions.iter().any(|p| p[1] > ceiling + 2.0));
        }
    }

    #[test]
    fn the_stair_is_the_same_every_time() {
        let a = stair_mesh(StairKind::Down, 24.0, 5);
        assert_eq!(a, stair_mesh(StairKind::Down, 24.0, 5));
        assert_ne!(a, stair_mesh(StairKind::Down, 24.0, 6));
    }

    #[test]
    fn the_stair_faces_the_way_it_points() {
        for facing in Facing::ALL {
            let (at, yaw) = stair_transform(&stair(StairKind::Down, facing));
            assert_eq!(at, cell_centre(3, 4));
            let (dx, dy) = facing.step();
            let along = [yaw.sin(), yaw.cos()];
            assert!((along[0] - dx as f32).abs() < 1e-5 && (along[1] - dy as f32).abs() < 1e-5, "{facing:?}: {along:?}");
        }
    }

    #[test]
    fn only_the_stairs_own_side_of_the_cell_is_opened() {
        let down = stair(StairKind::Down, Facing::North);
        let up = stair(StairKind::Up, Facing::North);
        assert!(opens_floor(Some(&down)) && !opens_ceiling(Some(&down)));
        assert!(opens_ceiling(Some(&up)) && !opens_floor(Some(&up)));
        assert!(!opens_floor(None) && !opens_ceiling(None));
    }
}
