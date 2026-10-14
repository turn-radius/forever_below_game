//! Seeded value noise for roughening surfaces (tumbled stones, chipped edges, knotted wood).
//! Pure integer hashing, so it is the same on every machine.

use super::vec3::V3;

/// A 32-bit integer hash (the "lowbias32" finaliser from Chris Wellons, "Prospecting for
/// Hash Functions", 2018, <https://nullprogram.com/blog/2018/07/31/>).
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

fn lattice(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let h = hash(seed ^ hash((x as u32).wrapping_add(hash((y as u32).wrapping_add(hash(z as u32))))));
    (h >> 8) as f32 / (1u32 << 24) as f32
}

fn fade(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Smooth value noise in `0..1` at `p`, one lattice cell per unit. Different seeds give
/// unrelated fields.
pub fn value_noise3(p: V3, seed: u32) -> f32 {
    let cell = p.map(f32::floor);
    let [ix, iy, iz] = cell.map(|c| c as i32);
    let [fx, fy, fz] = [fade(p[0] - cell[0]), fade(p[1] - cell[1]), fade(p[2] - cell[2])];
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let corner = |dx: i32, dy: i32, dz: i32| lattice(ix + dx, iy + dy, iz + dz, seed);
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), fx);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), fx);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), fx);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), fx);
    lerp(lerp(x00, x10, fy), lerp(x01, x11, fy), fz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_bounded_deterministic_and_continuous() {
        for i in 0..200 {
            let p = [i as f32 * 0.173, i as f32 * -0.311, i as f32 * 0.07];
            let v = value_noise3(p, 9);
            assert!((0.0..1.0).contains(&v));
            assert_eq!(v, value_noise3(p, 9));
            let near = value_noise3([p[0] + 1e-3, p[1], p[2]], 9);
            assert!((v - near).abs() < 0.01);
        }
        assert_ne!(value_noise3([0.5, 0.5, 0.5], 1), value_noise3([0.5, 0.5, 0.5], 2));
    }
}
