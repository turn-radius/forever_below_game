//! Curved plates: a 2D outline (with holes) laid onto a surface and given a thickness.
//!
//! A shield's face is an outline on a dished or bent board; a helmet bowl is an outline on a
//! surface of revolution, with eye slits and breaths as holes; a nasal, a cheek plate, a
//! spangen band or a heraldic charge is an outline on the surface of the thing it covers.
//! Greaves, vambraces, couters and cuirass plates are the same again, so this takes no
//! family's state: just the outline, a [`Surface`] and [`PlateOptions`].
//!
//! The outline is filled by [`tessellate`], so the shell follows the surface however it
//! bends. Each triangle samples the surface normal just inside itself, and corners whose
//! samples agree share one vertex: a smooth surface shades smooth, while a crease (the ridge
//! between two facets of a segmented bowl, the spine of a pavise) splits into a hard edge
//! wherever a grid line runs along it. Put grid lines on creases with
//! [`PlateOptions::cell`] and [`PlateOptions::origin`].

use crate::geom::polygon::P2;
use crate::geom::vec3::{self, V3};
use crate::geom::{Frame, LatheOptions, MeshBuilder, Shading, SweepOptions, lathe, spline_profile, sweep_frames, tessellate};

/// Where a plate's outline lands in item space: `point(p)` for a point `p` of the outline's
/// plane. The plate's face points along `d point / dx  x  d point / dy`.
pub trait Surface {
    fn point(&self, p: P2) -> V3;

    /// The unit normal at `p`, by central differences `h` apart. Falls back to sampling a
    /// little further out where the surface pinches to a pole.
    fn normal(&self, p: P2, h: f32) -> V3 {
        let mut h = h.max(1e-6);
        for _ in 0..6 {
            let du = vec3::sub(self.point([p[0] + h, p[1]]), self.point([p[0] - h, p[1]]));
            let dv = vec3::sub(self.point([p[0], p[1] + h]), self.point([p[0], p[1] - h]));
            let cross = vec3::cross(du, dv);
            if vec3::length(cross) > 1e-14
                && let Some(n) = vec3::try_normalize(cross)
            {
                return n;
            }
            h *= 8.0;
        }
        vec3::Z
    }
}

impl<F: Fn(P2) -> V3> Surface for F {
    fn point(&self, p: P2) -> V3 {
        self(p)
    }
}

/// The flat plane z = 0: a plate on it is an extrusion with a dense face.
pub fn flat(p: P2) -> V3 {
    [p[0], p[1], 0.0]
}

/// How [`plate`] shapes a shell.
#[derive(Clone, Copy)]
pub struct PlateOptions<'a> {
    /// Distance between the face and the back. Zero gives a single sheet (face only).
    pub thickness: f32,
    /// Where the middle of the shell sits along the surface normal: 0 centres it on the
    /// surface, `-thickness / 2` hangs it inside (a helmet whose surface is its outer skin),
    /// `+thickness / 2` lays it on top (a band riveted over a bowl).
    pub offset: f32,
    /// Grid cell size in the outline's plane, x and y. Smaller follows the surface closer.
    pub cell: [f32; 2],
    /// A point grid lines pass through.
    pub origin: P2,
    /// Sampled normals further apart than this (radians) stay split as a hard edge.
    pub crease: f32,
    /// Outline edges, by their endpoints, that get no wall: seams where a plate wraps round
    /// and meets itself.
    pub open_edge: Option<&'a dyn Fn(P2, P2) -> bool>,
    /// The rectangle `(min, size)` mapped to uv 0..=1 on the face and back; the outline's
    /// bounds when `None`.
    pub uv_rect: Option<(P2, P2)>,
}

impl Default for PlateOptions<'_> {
    fn default() -> Self {
        PlateOptions { thickness: 0.05, offset: 0.0, cell: [0.1, 0.1], origin: [0.0, 0.0], crease: 0.3, open_edge: None, uv_rect: None }
    }
}

impl std::fmt::Debug for PlateOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlateOptions")
            .field("thickness", &self.thickness)
            .field("offset", &self.offset)
            .field("cell", &self.cell)
            .field("origin", &self.origin)
            .field("crease", &self.crease)
            .field("open_edge", &self.open_edge.is_some())
            .field("uv_rect", &self.uv_rect)
            .finish()
    }
}

/// A plate's three surfaces, kept apart so a family can give them different materials (a
/// painted face on a plain board, a mail lining).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plate {
    /// The side the surface normal points to.
    pub face: MeshBuilder,
    pub back: MeshBuilder,
    /// The walls round the outline and the holes. UV u runs along the edge, v across it.
    pub edge: MeshBuilder,
}

impl Plate {
    /// All three surfaces as one mesh.
    pub fn merged(&self) -> MeshBuilder {
        MeshBuilder::merged([&self.face, &self.back, &self.edge])
    }
}

/// A point on a surface and the unit normal there, `offset` out along the normal.
pub fn on_surface(surface: &impl Surface, p: P2, offset: f32, h: f32) -> (V3, V3) {
    let n = surface.normal(p, h);
    (vec3::add(surface.point(p), vec3::scale(n, offset)), n)
}

/// Frames along a path drawn on a surface, lifted `offset` along its normal: `normal` is
/// the surface normal and `binormal = tangent x normal`, so walking a counter-clockwise ring
/// the binormal points out of it. For rims, bindings, rolled edges and straps that follow a
/// plate's edge or lie along its face.
pub fn surface_frames(surface: &impl Surface, path: &[P2], closed: bool, offset: f32, h: f32) -> Vec<Frame> {
    let n = path.len();
    let points: Vec<(V3, V3)> = path.iter().map(|&p| on_surface(surface, p, offset, h)).collect();
    (0..n)
        .map(|k| {
            let (prev, next) = match (k, closed) {
                (0, false) => (0, 1.min(n - 1)),
                (k, false) if k + 1 == n => (k - 1, k),
                _ => ((k + n - 1) % n, (k + 1) % n),
            };
            let (origin, normal) = points[k];
            let t = vec3::sub(points[next].0, points[prev].0);
            let tangent = vec3::normalize_or(vec3::sub(t, vec3::scale(normal, vec3::dot(t, normal))), vec3::any_perpendicular(normal));
            Frame { origin, tangent, normal, binormal: vec3::cross(tangent, normal) }
        })
        .collect()
}

/// Sweeps `section` along a path drawn on a surface (see [`surface_frames`]): x of the
/// section runs along the surface normal, y across the path (outward on a counter-clockwise
/// ring). A rim binding, a rolled edge, a strap.
pub fn sweep_on_surface(surface: &impl Surface, path: &[P2], closed: bool, offset: f32, section: &[P2], h: f32) -> MeshBuilder {
    let frames = surface_frames(surface, path, closed, offset, h);
    let options = if closed { SweepOptions::closed_loop() } else { SweepOptions::default() };
    sweep_frames(&frames, section, &options, |_| 1.0)
}

/// A rounded rectangle from `min` to `max`, `n` points, its corners rounded by `roundness`
/// (0 square, 1 an ellipse): a superellipse. Sections for bindings, straps and bars.
pub fn rounded_rect(min: P2, max: P2, roundness: f32, n: usize) -> Vec<P2> {
    let c = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
    let r = [(max[0] - min[0]) * 0.5, (max[1] - min[1]) * 0.5];
    let e = 2.0 / (2.0 + 8.0 * (1.0 - roundness.clamp(0.0, 1.0)));
    (0..n.max(4))
        .map(|k| {
            let a = std::f32::consts::TAU * k as f32 / n.max(4) as f32;
            let (s, co) = a.sin_cos();
            [c[0] + r[0] * co.signum() * co.abs().powf(e), c[1] + r[1] * s.signum() * s.abs().powf(e)]
        })
        .collect()
}

/// A domed rivet or stud head of `radius`, base on y = 0, crown at `0.6 * radius`.
pub fn rivet_head(radius: f32, sides: usize) -> MeshBuilder {
    let r = radius;
    let control = [[r, 0.0], [r * 0.96, r * 0.22], [r * 0.72, r * 0.48], [r * 0.32, r * 0.6], [0.0, r * 0.62]];
    lathe(&spline_profile(&control, 8), &LatheOptions { facets: sides.max(6), shading: Shading::SMOOTH, cap_start: true, cap_end: false, phase: 0.0 })
}

/// Copies of `mesh` (built upright on y = 0) set on the surface at each of `at`, lifted
/// `offset` and standing along the normal there.
pub fn scatter_on_surface(surface: &impl Surface, mesh: &MeshBuilder, at: &[P2], offset: f32, h: f32) -> MeshBuilder {
    let mut out = MeshBuilder::new();
    for &p in at {
        let (origin, normal) = on_surface(surface, p, offset, h);
        let mut copy = mesh.clone();
        copy.place(origin, normal);
        out.append(&copy);
    }
    out
}

/// Lays `outline` minus `holes` onto `surface` as a shell (see the module docs). Outline
/// and holes may wind either way. Holes must lie inside the outline and not touch it.
pub fn plate(outline: &[P2], holes: &[Vec<P2>], surface: &impl Surface, options: &PlateOptions) -> Plate {
    let mut out = Plate::default();
    let tess = tessellate(outline, holes, options.cell, options.origin);
    if tess.triangles.is_empty() {
        return out;
    }
    let cell_min = options.cell[0].abs().min(options.cell[1].abs()).max(1e-4);
    let h = cell_min * 1e-3;
    let cos_crease = options.crease.cos();
    let n_points = tess.points.len();

    // Normals sampled just inside each triangle, clustered per point by direction.
    let mut clusters: Vec<Vec<(V3, V3)>> = vec![Vec::new(); n_points]; // (sum, first)
    let mut corner_cluster: Vec<[usize; 3]> = Vec::with_capacity(tess.triangles.len());
    for tri in &tess.triangles {
        let pts = tri.map(|k| tess.points[k as usize]);
        let centre = [(pts[0][0] + pts[1][0] + pts[2][0]) / 3.0, (pts[0][1] + pts[1][1] + pts[2][1]) / 3.0];
        let mut ids = [0usize; 3];
        for c in 0..3 {
            let q = [pts[c][0] + (centre[0] - pts[c][0]) * 0.2, pts[c][1] + (centre[1] - pts[c][1]) * 0.2];
            let n = surface.normal(q, h);
            let list = &mut clusters[tri[c] as usize];
            ids[c] = match list.iter().position(|&(_, first)| vec3::dot(first, n) >= cos_crease) {
                Some(k) => {
                    list[k].0 = vec3::add(list[k].0, n);
                    k
                }
                None => {
                    list.push((n, n));
                    list.len() - 1
                }
            };
        }
        corner_cluster.push(ids);
    }
    let base: Vec<V3> = tess.points.iter().map(|&p| surface.point(p)).collect();
    // The direction a point moves to make the face and back: every sample round it.
    let lift: Vec<V3> = (0..n_points)
        .map(|k| {
            let sum = clusters[k].iter().fold(vec3::ZERO, |acc, &(s, _)| vec3::add(acc, s));
            vec3::try_normalize(sum).unwrap_or_else(|| surface.normal(tess.points[k], h))
        })
        .collect();

    let (uv_min, uv_size) = options.uv_rect.unwrap_or_else(|| {
        let (lo, hi) = outline.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), q| {
            ([lo[0].min(q[0]), lo[1].min(q[1])], [hi[0].max(q[0]), hi[1].max(q[1])])
        });
        (lo, [(hi[0] - lo[0]).max(1e-6), (hi[1] - lo[1]).max(1e-6)])
    });
    let uv = |p: P2| [(p[0] - uv_min[0]) / uv_size[0], (p[1] - uv_min[1]) / uv_size[1]];
    let at = |k: usize, height: f32| vec3::add(base[k], vec3::scale(lift[k], height));

    let thick = options.thickness > 0.0;
    let sides: &[(f32, bool)] = if thick {
        &[(options.offset + options.thickness * 0.5, true), (options.offset - options.thickness * 0.5, false)]
    } else {
        &[(options.offset, true)]
    };
    for &(height, front) in sides {
        let mesh = if front { &mut out.face } else { &mut out.back };
        let mut ids: Vec<Vec<Option<u32>>> = clusters.iter().map(|c| vec![None; c.len()]).collect();
        for (tri, cl) in tess.triangles.iter().zip(&corner_cluster) {
            let corners = [0, 1, 2].map(|c| at(tri[c] as usize, height));
            let area = vec3::length(vec3::cross(vec3::sub(corners[1], corners[0]), vec3::sub(corners[2], corners[0])));
            if area < 1e-10 {
                continue;
            }
            let v = [0, 1, 2].map(|c| {
                let (k, j) = (tri[c] as usize, cl[c]);
                *ids[k][j].get_or_insert_with(|| {
                    let n = vec3::normalize_or(clusters[k][j].0, lift[k]);
                    mesh.push_vertex(corners[c], if front { n } else { vec3::neg(n) }, uv(tess.points[k]))
                })
            });
            if front { mesh.push_triangle(v[0], v[1], v[2]) } else { mesh.push_triangle(v[0], v[2], v[1]) };
        }
    }

    if thick {
        let (top, bottom) = (options.offset + options.thickness * 0.5, options.offset - options.thickness * 0.5);
        for ring in &tess.boundary {
            edge_walls(&mut out.edge, ring, &tess.points, surface, &|k| at(k, top), &|k| at(k, bottom), &lift, options, h);
        }
    }
    out
}

/// The walls round one boundary loop (solid on its left), smooth round gentle bends and hard
/// at corners.
#[allow(clippy::too_many_arguments)]
fn edge_walls(
    mesh: &mut MeshBuilder,
    ring: &[u32],
    points: &[P2],
    surface: &impl Surface,
    top: &dyn Fn(usize) -> V3,
    bottom: &dyn Fn(usize) -> V3,
    lift: &[V3],
    options: &PlateOptions,
    h: f32,
) {
    let n = ring.len();
    if n < 3 {
        return;
    }
    let cos_corner = 0.6f32.cos();
    // Outward wall normal of each edge k -> k + 1.
    let normals: Vec<Option<V3>> = (0..n)
        .map(|k| {
            let (a, b) = (ring[k] as usize, ring[(k + 1) % n] as usize);
            let (pa, pb) = (points[a], points[b]);
            if options.open_edge.is_some_and(|open| open(pa, pb)) {
                return None;
            }
            let d = [pb[0] - pa[0], pb[1] - pa[1]];
            let len = d[0].hypot(d[1]).max(1e-9);
            let out2 = [d[1] / len, -d[0] / len];
            let mid = [(pa[0] + pb[0]) * 0.5, (pa[1] + pb[1]) * 0.5];
            let step = h * 10.0;
            let o3 = vec3::sub(surface.point([mid[0] + out2[0] * step, mid[1] + out2[1] * step]), surface.point([mid[0] - out2[0] * step, mid[1] - out2[1] * step]));
            let up = vec3::normalize_or(vec3::add(lift[a], lift[b]), vec3::Z);
            vec3::try_normalize(vec3::sub(o3, vec3::scale(up, vec3::dot(o3, up))))
        })
        .collect();
    // Shared with the neighbouring wall round a gentle bend, its own at a corner.
    let corner_normal = |edge: usize, other: usize| -> V3 {
        let own = normals[edge].unwrap_or(vec3::Z);
        match normals[other] {
            Some(o) if vec3::dot(o, own) >= cos_corner => vec3::normalize_or(vec3::add(o, own), own),
            _ => own,
        }
    };
    let perimeter: f32 = (0..n).map(|k| vec3::distance(top(ring[k] as usize), top(ring[(k + 1) % n] as usize))).sum::<f32>().max(1e-6);
    let mut along = 0.0;
    for k in 0..n {
        let next = (k + 1) % n;
        let (a, b) = (ring[k] as usize, ring[next] as usize);
        let (ta, tb, ba, bb) = (top(a), top(b), bottom(a), bottom(b));
        let len = vec3::distance(ta, tb);
        let (u0, u1) = (along / perimeter, (along + len) / perimeter);
        along += len;
        let Some(own) = normals[k] else { continue };
        if len < 1e-7 && vec3::distance(ba, bb) < 1e-7 {
            continue;
        }
        let na = corner_normal(k, (k + n - 1) % n);
        let nb = corner_normal(k, next);
        let i0 = mesh.push_vertex(ta, na, [u0, 1.0]);
        let i1 = mesh.push_vertex(ba, na, [u0, 0.0]);
        let i2 = mesh.push_vertex(bb, nb, [u1, 0.0]);
        let i3 = mesh.push_vertex(tb, nb, [u1, 1.0]);
        let face = vec3::cross(vec3::sub(bb, ta), vec3::sub(tb, ba));
        if vec3::dot(face, own) >= 0.0 {
            mesh.push_triangle(i0, i1, i2).push_triangle(i0, i2, i3);
        } else {
            mesh.push_triangle(i0, i2, i1).push_triangle(i0, i3, i2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::OutlineMode;

    fn disc(r: f32, n: usize) -> Vec<P2> {
        (0..n).map(|k| {
            let (s, c) = (std::f32::consts::TAU * k as f32 / n as f32).sin_cos();
            [r * c, r * s]
        }).collect()
    }

    fn check(mesh: &MeshBuilder) {
        let part = mesh.clone().into_part("plate", OutlineMode::Welded);
        part.validate().unwrap();
        for tri in part.indices.as_chunks::<3>().0 {
            let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| i as usize);
            let face = vec3::cross(vec3::sub(part.positions[b], part.positions[a]), vec3::sub(part.positions[c], part.positions[a]));
            if vec3::length(face) < 1e-9 {
                continue;
            }
            let n = vec3::add(vec3::add(part.normals[a], part.normals[b]), part.normals[c]);
            assert!(vec3::dot(face, n) > 0.0, "triangle winds against its normals");
        }
    }

    #[test]
    fn a_dished_disc_faces_out_and_closes() {
        let dish = |p: P2| [p[0], p[1], 0.3 * (1.0 - (p[0] * p[0] + p[1] * p[1]) / 4.0)];
        let plate = plate(&disc(2.0, 48), &[disc(0.4, 12)], &dish, &PlateOptions { thickness: 0.1, cell: [0.15, 0.15], ..Default::default() });
        check(&plate.face);
        check(&plate.back);
        check(&plate.edge);
        let merged = plate.merged();
        check(&merged);
        // The face is on the +z side of the back everywhere.
        let face_mid = plate.face.bounds().unwrap();
        let back_mid = plate.back.bounds().unwrap();
        assert!(face_mid.1[2] > back_mid.1[2]);
        assert!(plate.face.normals.iter().all(|n| n[2] > 0.5));
        assert!(plate.back.normals.iter().all(|n| n[2] < -0.5));
    }

    #[test]
    fn creases_on_grid_lines_stay_hard() {
        // A roof: two planes meeting along x = 0.
        let roof = |p: P2| [p[0], p[1], 0.5 - 0.5 * p[0].abs()];
        let square = vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
        let plate = plate(&square, &[], &roof, &PlateOptions { thickness: 0.0, cell: [0.1, 0.1], ..Default::default() });
        check(&plate.face);
        let ridge: Vec<V3> = plate.face.positions.iter().zip(&plate.face.normals).filter(|(p, _)| p[0].abs() < 1e-5).map(|(_, &n)| n).collect();
        assert!(ridge.iter().any(|n| n[0] > 0.3) && ridge.iter().any(|n| n[0] < -0.3), "ridge should split");
        assert!(plate.back.is_empty() && plate.edge.is_empty());
    }
}
