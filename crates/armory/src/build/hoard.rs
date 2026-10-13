//! Loose valuables: coins, scattered or heaped. A purse's spilled coins and a treasure
//! chest's hoard (`coins()` and the treasure mound in `buildChest`, in
//! `reference/spline_armory_engine.js`).

use procgen_rng::Mulberry32;

use crate::geom::{GridOptions, LatheOptions, MeshBuilder, Shading, V3, lathe, parametric_surface};
use crate::geom::vec3;

/// One coin of `radius`, lying flat with its lower face on y = 0: a disc with a raised rim.
pub fn coin(radius: f32) -> MeshBuilder {
    let (r, h) = (radius, radius * 0.16);
    let profile = [[0.0, 0.0], [r, 0.0], [r, h], [r * 0.82, h], [r * 0.8, h * 0.8], [0.0, h * 0.8]];
    lathe(&profile, &LatheOptions { facets: 20, shading: Shading::BANDED, cap_start: false, cap_end: false, phase: 0.0 })
}

/// `count` coins of `radius` dropped round `center` within `spread` (x and z), tipped and
/// turned at random and lying on or a little above `center`'s height, as if spilled.
pub fn coin_scatter(rng: &mut Mulberry32, center: V3, spread: [f32; 2], count: u32, radius: f32) -> MeshBuilder {
    let one = coin(radius);
    let mut mesh = MeshBuilder::new();
    for _ in 0..count {
        let mut c = one.clone();
        let tilt = [rng.range_f32(-0.5, 0.5), rng.range_f32(-0.5, 0.5)];
        let turn = rng.range_f32(0.0, std::f32::consts::TAU);
        let offset = [rng.range_f32(-0.5, 0.5) * spread[0], rng.range_f32(0.0, 0.6) * radius, rng.range_f32(-0.5, 0.5) * spread[1]];
        // A tipped coin rests on its rim, so lift it by how far the tip drops it.
        let lift = radius * tilt[0].sin().hypot(tilt[1].sin());
        c.rotate_y(turn).rotate_x(tilt[0]).rotate_z(tilt[1]).translate(vec3::add(center, [offset[0], offset[1] + lift, offset[2]]));
        mesh.append(&c);
    }
    mesh
}

/// A heaped dome of coins on y = 0: `half` is its half width (x), height (y) and half depth
/// (z). The surface is lumped at about the size of a coin of `coin_radius`, the lumps fixed by
/// `seed`. Closed underneath.
pub fn coin_heap(half: V3, coin_radius: f32, seed: u32) -> MeshBuilder {
    let mut rng = Mulberry32::new(seed);
    let waves: Vec<(f32, f32, f32, f32)> = (0..5)
        .map(|_| {
            let angle = rng.range_f32(0.0, std::f32::consts::TAU);
            let freq = rng.range_f32(0.7, 1.3) / (coin_radius * 2.2);
            (angle.cos() * freq, angle.sin() * freq, rng.range_f32(0.0, std::f32::consts::TAU), freq)
        })
        .collect();
    let lump = |x: f32, z: f32| waves.iter().map(|&(fx, fz, ph, _)| (x * fx + z * fz + ph).sin()).sum::<f32>() / waves.len() as f32;
    let segments = 48;
    let rings = 16;
    let options = GridOptions { wrap_around: true, wrap_along: false, shading: Shading::SMOOTH, row_v: None };
    let mut mesh = parametric_surface(segments, rings, &options, |u, v| {
        let (sa, ca) = (u * std::f32::consts::TAU).sin_cos();
        let (se, ce) = (v * std::f32::consts::FRAC_PI_2).sin_cos();
        let p = [half[0] * ce * ca, half[1] * se, -half[2] * ce * sa];
        // Lumps fade at the rim so the heap meets its floor.
        let bump = coin_radius * 0.45 * lump(p[0], p[2]) * (v * 3.0).min(1.0);
        let n = vec3::normalize_or([p[0] / half[0].max(1e-4).powi(2), p[1] / half[1].max(1e-4).powi(2), p[2] / half[2].max(1e-4).powi(2)], vec3::Y);
        vec3::add(p, vec3::scale(n, bump))
    });
    let rim: Vec<V3> = (0..segments)
        .map(|i| {
            let (sa, ca) = (std::f32::consts::TAU * i as f32 / segments as f32).sin_cos();
            [half[0] * ca, 0.0, -half[2] * sa]
        })
        .collect();
    mesh.push_fan_cap(&rim, vec3::neg(vec3::Y));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    #[test]
    fn hoards_are_drawable_and_rest_on_the_floor() {
        let mut rng = Mulberry32::new(3);
        let scatter = coin_scatter(&mut rng, [0.0, 0.0, 0.0], [1.0, 1.0], 8, 0.12);
        let heap = coin_heap([1.0, 0.6, 0.8], 0.12, 9);
        for mesh in [scatter, heap] {
            let (lo, _) = mesh.bounds().unwrap();
            assert!(lo[1] > -0.03, "{lo:?}");
            mesh.into_part("hoard", OutlineMode::Welded).validate().unwrap();
        }
    }
}
