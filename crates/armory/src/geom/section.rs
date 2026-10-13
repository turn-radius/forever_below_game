//! Cut faces: the flat face left where a solid is sliced, with a band of rind round its edge.
//! A cut cheese shows its rind and paste, a sliced loaf its crust and crumb, a cut pie its
//! pastry and filling, a split log its bark and heartwood.
//!
//! Faces are built flat in the XY plane facing +Z; the caller rotates them onto the cut.

use super::builder::MeshBuilder;
use super::polygon::{P2, signed_area, triangulate};

/// Moves every point of a polyline `distance` to the left of its direction of travel (inward
/// for a counter-clockwise outline, or for a lathe profile walked with the outside on its
/// right). Each point moves along the mean of its two edge normals.
pub fn inset_polyline(points: &[P2], distance: f32, closed: bool) -> Vec<P2> {
    let n = points.len();
    if n < 2 {
        return points.to_vec();
    }
    let edge_normal = |a: P2, b: P2| -> Option<P2> {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx.hypot(dy);
        (len > 1e-9).then(|| [-dy / len, dx / len])
    };
    (0..n)
        .map(|i| {
            let prev = if i > 0 { Some(i - 1) } else if closed { Some(n - 1) } else { None };
            let next = if i + 1 < n { Some(i + 1) } else if closed { Some(0) } else { None };
            let a = prev.and_then(|p| edge_normal(points[p], points[i]));
            let b = next.and_then(|q| edge_normal(points[i], points[q]));
            let normal = match (a, b) {
                (Some(a), Some(b)) => {
                    // Mitred: both edges move the full distance, up to three times it at
                    // sharp corners.
                    let m = [a[0] + b[0], a[1] + b[1]];
                    let len = m[0].hypot(m[1]);
                    if len > 1e-6 {
                        let unit = [m[0] / len, m[1] / len];
                        let miter = 1.0 / (unit[0] * a[0] + unit[1] * a[1]).max(1.0 / 3.0);
                        [unit[0] * miter, unit[1] * miter]
                    } else {
                        a
                    }
                }
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => [0.0, 0.0],
            };
            [points[i][0] + normal[0] * distance, points[i][1] + normal[1] * distance]
        })
        .collect()
}

/// Planar UVs for a face: the bounding square of `outline` mapped to 0..=1.
fn face_uv(outline: &[P2]) -> impl Fn(P2) -> [f32; 2] {
    let (lo, hi) = outline.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), q| {
        ([lo[0].min(q[0]), lo[1].min(q[1])], [hi[0].max(q[0]), hi[1].max(q[1])])
    });
    let size = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-6);
    move |q: P2| [(q[0] - lo[0]) / size, (q[1] - lo[1]) / size]
}

/// A flat polygon facing +Z (any winding in, counter-clockwise triangles out).
fn flat_polygon(points: &[P2], uv: &impl Fn(P2) -> [f32; 2]) -> MeshBuilder {
    let mut mesh = MeshBuilder::new();
    if points.len() < 3 {
        return mesh;
    }
    for &q in points {
        mesh.push_vertex([q[0], q[1], 0.0], [0.0, 0.0, 1.0], uv(q));
    }
    for t in triangulate(points, &[]) {
        mesh.push_triangle(t[0], t[1], t[2]);
    }
    mesh
}

/// The band between two matching polylines, flat, facing +Z.
fn band(outer: &[P2], inner: &[P2], closed: bool, uv: &impl Fn(P2) -> [f32; 2]) -> MeshBuilder {
    let mut mesh = MeshBuilder::new();
    let n = outer.len().min(inner.len());
    let segments = if closed { n } else { n.saturating_sub(1) };
    for i in 0..segments {
        let j = (i + 1) % n;
        let quad = [outer[i], outer[j], inner[j], inner[i]];
        let area = signed_area(&quad).abs();
        if area < 1e-10 {
            continue;
        }
        mesh.push_flat_quad(quad.map(|q| [q[0], q[1], 0.0]), quad.map(&uv), [0.0, 0.0, 1.0]);
    }
    mesh
}

/// The cut face of a solid whose section is the closed `outline` (in the XY plane), split
/// into `(rind, core)`: a band `rind` thick round the edge, and the flat core inside it. Both
/// face +Z. With `rind` at zero the rind mesh is empty and the core is the whole face.
pub fn rind_face(outline: &[P2], rind: f32) -> (MeshBuilder, MeshBuilder) {
    let ccw: Vec<P2> = if signed_area(outline) < 0.0 { outline.iter().rev().copied().collect() } else { outline.to_vec() };
    let uv = face_uv(&ccw);
    if rind <= 0.0 {
        return (MeshBuilder::new(), flat_polygon(&ccw, &uv));
    }
    let inner = inset_polyline(&ccw, rind, true);
    (band(&ccw, &inner, true, &uv), flat_polygon(&inner, &uv))
}

/// The face left where a turned solid is cut along its axis: the lathe `profile` (`[radius,
/// y]`, walked with the outside on its right, as for [`super::lathe`]) closed along the axis.
/// Returned as `(rind, core)` like [`rind_face`], in the XY plane with x the radius, facing +Z.
/// The rind follows the profile only, never the axis. Rotate the face about +Y by the angle of
/// the cut (and flip it for the far side of a sector).
pub fn profile_face(profile: &[[f32; 2]], rind: f32) -> (MeshBuilder, MeshBuilder) {
    let mut outer: Vec<P2> = profile.iter().map(|&[r, y]| [r.max(0.0), y]).collect();
    if outer.len() < 2 {
        return (MeshBuilder::new(), MeshBuilder::new());
    }
    if outer[0][0] > 1e-5 {
        outer.insert(0, [0.0, outer[0][1]]);
    }
    if outer[outer.len() - 1][0] > 1e-5 {
        outer.push([0.0, outer[outer.len() - 1][1]]);
    }
    let uv = face_uv(&outer);
    if rind <= 0.0 {
        return (MeshBuilder::new(), flat_polygon(&outer, &uv));
    }
    // Walked with the outside on the right, the solid is on the left: inset to the left.
    let mut inner = inset_polyline(&outer, rind, false);
    let last = inner.len() - 1;
    // The ends stay on the axis: the rind runs along the profile, not across the cut.
    for i in [0, last] {
        inner[i][0] = 0.0;
    }
    for q in &mut inner {
        q[0] = q[0].max(0.0);
    }
    (band(&outer, &inner, false, &uv), flat_polygon(&inner, &uv))
}
