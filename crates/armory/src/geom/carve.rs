//! Carving: pressing spherical hollows into a finished surface. Bites out of apples, loaves
//! and roasts; gouges and dents anywhere else.

use std::collections::HashMap;

use super::builder::MeshBuilder;
use super::vec3::{self, V3};

/// One spherical hollow: everything of the surface inside the ball sinks onto its far
/// boundary, so the hollow takes the ball's shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hollow {
    pub center: V3,
    pub radius: f32,
}

impl Hollow {
    /// A hollow of `radius` pressed into a surface that curves about `center` with radius
    /// `surface_radius` (a ball, a fruit, the round of a loaf), from `direction`. The ball sits
    /// far enough out that the surface clearly faces its centre all round the rim, which
    /// [`carve_hollows`] needs for a clean, shelving edge; the hollow is then about
    /// `surface_radius + radius - 1.12 * sqrt(surface_radius² + radius²)` deep.
    pub fn bite(center: V3, direction: V3, surface_radius: f32, radius: f32) -> Hollow {
        let reach = (surface_radius * surface_radius + radius * radius).sqrt() * 1.12;
        Hollow { center: vec3::add(center, vec3::scale(vec3::normalize_or(direction, vec3::X), reach)), radius }
    }

    /// Signed distance from `p` to the ball's surface (negative inside).
    fn depth(&self, p: V3) -> f32 {
        vec3::distance(p, self.center) - self.radius
    }
}

/// Signed distance to the union of the balls, and the nearest ball.
fn nearest(hollows: &[Hollow], p: V3) -> (f32, &Hollow) {
    hollows
        .iter()
        .map(|h| (h.depth(p), h))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .expect("at least one hollow")
}

/// Presses `hollows` into a closed, finely divided surface. Each vertex inside a ball sinks
/// along its normal onto the ball's far side, so the surface must face the ball's centre
/// where they meet (centres outside the surface, as [`Hollow::bite`] places them), or the rim
/// tears. Triangles that straddle the rim are split exactly on it, so the edge is clean.
/// Returns `(kept, carved)`: the untouched surface, and the newly exposed hollow (shaded as
/// the inside of the ball) to give the inside's material, flesh or crumb. Hollows apply in
/// order, so overlapping ones leave a scalloped edge, like teeth marks.
pub fn carve_hollows(mesh: &MeshBuilder, hollows: &[Hollow]) -> (MeshBuilder, MeshBuilder) {
    if hollows.is_empty() {
        return (mesh.clone(), MeshBuilder::new());
    }
    // Each vertex after carving (None where it stays put), with the hollow's normal there.
    let sunk: Vec<Option<(V3, V3)>> = mesh
        .positions
        .iter()
        .zip(&mesh.normals)
        .map(|(&p, &n)| {
            if nearest(hollows, p).0 >= 0.0 {
                return None;
            }
            let mut q = p;
            for h in hollows {
                let d = vec3::sub(q, h.center);
                let inside = vec3::length_squared(d) - h.radius * h.radius;
                if inside < 0.0 {
                    // |d - t n| = radius, taking the root that goes inward (t > 0).
                    let b = vec3::dot(n, d);
                    q = vec3::sub(q, vec3::scale(n, b + (b * b - inside).sqrt()));
                }
            }
            let normal = vec3::normalize_or(vec3::sub(nearest(hollows, q).1.center, q), n);
            Some((q, normal))
        })
        .collect();

    let mut kept = Side::default();
    let mut carved = Side::default();
    for tri in mesh.indices.as_chunks::<3>().0 {
        let inside: Vec<bool> = tri.iter().map(|&i| sunk[i as usize].is_some()).collect();
        if inside.iter().all(|&x| !x) {
            let ids: Vec<u32> = tri.iter().map(|&i| kept.original(mesh, i)).collect();
            kept.polygon(&ids);
            continue;
        }
        if inside.iter().all(|&x| x) {
            let ids: Vec<u32> = tri.iter().map(|&i| carved.sunk(mesh, i, sunk[i as usize].unwrap())).collect();
            carved.polygon(&ids);
            continue;
        }
        // Straddles the rim: walk the corners, cutting each crossing edge on the rim.
        let (mut outside_poly, mut inside_poly) = (Vec::new(), Vec::new());
        for k in 0..3 {
            let (i, j) = (tri[k], tri[(k + 1) % 3]);
            if inside[k] {
                inside_poly.push(carved.sunk(mesh, i, sunk[i as usize].unwrap()));
            } else {
                outside_poly.push(kept.original(mesh, i));
            }
            if inside[k] != inside[(k + 1) % 3] {
                let (point, normal, uv) = rim_crossing(mesh, hollows, i, j);
                let key = (i.min(j), i.max(j));
                outside_poly.push(kept.crossing(key, point, normal, uv));
                let hollow_normal = vec3::normalize_or(vec3::sub(nearest(hollows, point).1.center, point), normal);
                inside_poly.push(carved.crossing(key, point, hollow_normal, uv));
            }
        }
        kept.polygon(&outside_poly);
        carved.polygon(&inside_poly);
    }
    (kept.mesh, carved.mesh)
}

/// Where edge `i`-`j` (one end inside a ball, one outside) crosses the rim, by bisection,
/// with the normal and uv interpolated there.
fn rim_crossing(mesh: &MeshBuilder, hollows: &[Hollow], i: u32, j: u32) -> (V3, V3, [f32; 2]) {
    let (a, b) = (mesh.positions[i as usize], mesh.positions[j as usize]);
    let a_inside = nearest(hollows, a).0 < 0.0;
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if (nearest(hollows, vec3::lerp(a, b, mid)).0 < 0.0) == a_inside {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let t = 0.5 * (lo + hi);
    let (na, nb) = (mesh.normals[i as usize], mesh.normals[j as usize]);
    let (ua, ub) = (mesh.uvs[i as usize], mesh.uvs[j as usize]);
    (vec3::lerp(a, b, t), vec3::normalize_or(vec3::lerp(na, nb, t), na), [ua[0] + (ub[0] - ua[0]) * t, ua[1] + (ub[1] - ua[1]) * t])
}

/// One side of the cut, adding each source vertex and each rim crossing once, so shading
/// stays smooth across shared edges.
#[derive(Default)]
struct Side {
    mesh: MeshBuilder,
    vertices: HashMap<u32, u32>,
    crossings: HashMap<(u32, u32), u32>,
}

impl Side {
    fn original(&mut self, src: &MeshBuilder, i: u32) -> u32 {
        let mesh = &mut self.mesh;
        *self.vertices.entry(i).or_insert_with(|| mesh.push_vertex(src.positions[i as usize], src.normals[i as usize], src.uvs[i as usize]))
    }

    fn sunk(&mut self, src: &MeshBuilder, i: u32, (position, normal): (V3, V3)) -> u32 {
        let mesh = &mut self.mesh;
        *self.vertices.entry(i).or_insert_with(|| mesh.push_vertex(position, normal, src.uvs[i as usize]))
    }

    fn crossing(&mut self, key: (u32, u32), position: V3, normal: V3, uv: [f32; 2]) -> u32 {
        let mesh = &mut self.mesh;
        *self.crossings.entry(key).or_insert_with(|| mesh.push_vertex(position, normal, uv))
    }

    /// Fans a convex polygon (a triangle, or the quad left when a corner is cut off),
    /// skipping slivers.
    fn polygon(&mut self, ids: &[u32]) {
        for k in 1..ids.len().saturating_sub(1) {
            let [a, b, c] = [ids[0], ids[k], ids[k + 1]].map(|i| self.mesh.positions[i as usize]);
            if vec3::length_squared(vec3::cross(vec3::sub(b, a), vec3::sub(c, a))) < 1e-14 {
                continue;
            }
            self.mesh.push_triangle(ids[0], ids[k], ids[k + 1]);
        }
    }
}
