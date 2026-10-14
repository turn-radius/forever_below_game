//! Ready-made solids built on the primitives: spheres, spikes, frustums, boxes and tori.
//! All sit on or about the origin with +Y up.

use std::collections::HashMap;

use super::builder::MeshBuilder;
use super::grid::Shading;
use super::lathe::{LatheOptions, lathe, lathe_fn};
use super::sweep::{SweepOptions, tube};
use super::vec3::{self, V3};

/// A latitude/longitude sphere centred on the origin, smooth shaded, poles on the Y axis.
pub fn uv_sphere(radius: f32, segments: usize, rings: usize) -> MeshBuilder {
    let options = LatheOptions { facets: segments, shading: Shading::SMOOTH, cap_start: false, cap_end: false, phase: 0.0 };
    lathe_fn(rings.max(2), &options, |t| {
        let (s, c) = (std::f32::consts::PI * (t - 0.5)).sin_cos();
        [radius * c, radius * s]
    })
}

/// A subdivided icosahedron centred on the origin, smooth shaded. Even triangles everywhere
/// with no poles; use [`MeshBuilder::flat_shaded`] for a cut stone.
pub fn icosphere(radius: f32, subdivisions: u32) -> MeshBuilder {
    let g = (1.0 + 5f32.sqrt()) * 0.5;
    let mut verts: Vec<V3> = [
        [-1.0, g, 0.0], [1.0, g, 0.0], [-1.0, -g, 0.0], [1.0, -g, 0.0],
        [0.0, -1.0, g], [0.0, 1.0, g], [0.0, -1.0, -g], [0.0, 1.0, -g],
        [g, 0.0, -1.0], [g, 0.0, 1.0], [-g, 0.0, -1.0], [-g, 0.0, 1.0],
    ]
    .iter()
    .map(|&v| vec3::normalize_or(v, vec3::Y))
    .collect();
    let mut faces: Vec<[u32; 3]> = vec![
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    for _ in 0..subdivisions {
        let mut midpoints: HashMap<(u32, u32), u32> = HashMap::new();
        let mut midpoint = |a: u32, b: u32, verts: &mut Vec<V3>| -> u32 {
            *midpoints.entry((a.min(b), a.max(b))).or_insert_with(|| {
                verts.push(vec3::normalize_or(vec3::lerp(verts[a as usize], verts[b as usize], 0.5), vec3::Y));
                verts.len() as u32 - 1
            })
        };
        faces = faces
            .iter()
            .flat_map(|&[a, b, c]| {
                let ab = midpoint(a, b, &mut verts);
                let bc = midpoint(b, c, &mut verts);
                let ca = midpoint(c, a, &mut verts);
                [[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]
            })
            .collect();
    }
    let mut mesh = MeshBuilder::new();
    for &n in &verts {
        let u = 0.5 + n[2].atan2(n[0]) / std::f32::consts::TAU;
        let v = 0.5 + n[1].clamp(-1.0, 1.0).asin() / std::f32::consts::PI;
        mesh.push_vertex(vec3::scale(n, radius), n, [u, v]);
    }
    for [a, b, c] in faces {
        let (pa, pb, pc) = (verts[a as usize], verts[b as usize], verts[c as usize]);
        let face = vec3::cross(vec3::sub(pb, pa), vec3::sub(pc, pa));
        if vec3::dot(face, pa) >= 0.0 { mesh.push_triangle(a, b, c) } else { mesh.push_triangle(a, c, b) };
    }
    mesh
}

/// A cone from a base disc of `base_radius` at y = 0 to a point at y = `length`.
/// `concavity` 0 is a straight cone; positive hollows the flanks into a thorn or claw,
/// negative swells them into a bullet. Each step of 1 doubles (or halves) the taper exponent.
pub fn spike(base_radius: f32, length: f32, sides: usize, concavity: f32, shading: Shading) -> MeshBuilder {
    let exponent = 2f32.powf(concavity.clamp(-3.0, 3.0));
    let samples = if concavity.abs() < 1e-3 { 1 } else { 8 };
    let options = LatheOptions { facets: sides, shading, cap_start: true, cap_end: false, phase: 0.0 };
    lathe_fn(samples, &options, |t| [base_radius * (1.0 - t).powf(exponent), length * t])
}

/// A truncated cone from y = 0 to y = `height`, capped at both ends.
pub fn frustum(bottom_radius: f32, top_radius: f32, height: f32, sides: usize, shading: Shading) -> MeshBuilder {
    lathe(&[[bottom_radius, 0.0], [top_radius, height]], &LatheOptions { facets: sides, shading, ..Default::default() })
}

/// A capped cylinder from y = 0 to y = `height`.
pub fn cylinder(radius: f32, height: f32, sides: usize, shading: Shading) -> MeshBuilder {
    frustum(radius, radius, height, sides, shading)
}

/// An axis-aligned box of `size`, centred on the origin, with flat faces.
pub fn cuboid(size: V3) -> MeshBuilder {
    let h = vec3::scale(size, 0.5);
    let mut mesh = MeshBuilder::new();
    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let corner = |a: f32, b: f32| {
                let mut p = [0.0; 3];
                p[axis] = sign * h[axis];
                p[u] = a * h[u];
                p[v] = b * h[v];
                p
            };
            let mut facing = [0.0; 3];
            facing[axis] = sign;
            mesh.push_flat_quad([corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)], uvs, facing);
        }
    }
    mesh
}

/// A ring about +Y in the XZ plane: `major_radius` to the tube's centre, `minor_radius` thick.
pub fn torus(major_radius: f32, minor_radius: f32, major_segments: usize, minor_sides: usize, shading: Shading) -> MeshBuilder {
    let n = major_segments.max(3);
    let path: Vec<V3> = (0..n)
        .map(|i| {
            let (s, c) = (std::f32::consts::TAU * i as f32 / n as f32).sin_cos();
            [major_radius * c, 0.0, -major_radius * s]
        })
        .collect();
    let options = SweepOptions { shading, up: Some(vec3::X), ..SweepOptions::closed_loop() };
    tube(&path, minor_sides, &options, |_| minor_radius)
}
