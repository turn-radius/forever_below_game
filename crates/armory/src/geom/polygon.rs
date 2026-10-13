//! 2D polygons: orientation, triangulation (with holes), and extrusion into hard-edged slabs
//! for flanges, blades, guards, shield faces and plaques.
//!
//! Triangulation is ear clipping, with holes joined to the outline by bridge edges as in
//! David Eberly, "Triangulation by Ear Clipping" (Geometric Tools, 2002, updated 2015),
//! <https://www.geometrictools.com/Documentation/TriangulationByEarClipping.pdf>.

use super::builder::MeshBuilder;
use super::vec3::V3;

pub type P2 = [f32; 2];

fn sub2(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross2(a: P2, b: P2) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

/// Signed area: positive when the points run counter-clockwise.
pub fn signed_area(polygon: &[P2]) -> f32 {
    let n = polygon.len();
    (0..n).map(|i| cross2(polygon[i], polygon[(i + 1) % n])).sum::<f32>() * 0.5
}

fn turn(a: P2, b: P2, c: P2) -> f32 {
    cross2(sub2(b, a), sub2(c, b))
}

fn in_triangle(p: P2, a: P2, b: P2, c: P2) -> bool {
    const EPS: f32 = -1e-7;
    cross2(sub2(b, a), sub2(p, a)) >= EPS && cross2(sub2(c, b), sub2(p, b)) >= EPS && cross2(sub2(a, c), sub2(p, c)) >= EPS
}

/// Joins a clockwise `hole` into the counter-clockwise `ring` with a two-way bridge edge.
fn bridge(ring: &[u32], hole: &[u32], pts: &[P2]) -> Vec<u32> {
    let p = |i: u32| pts[i as usize];
    let m_at = (0..hole.len()).max_by(|&a, &b| p(hole[a])[0].total_cmp(&p(hole[b])[0])).unwrap_or(0);
    let m = p(hole[m_at]);
    // Cast a ray toward +x from M and find the nearest ring edge it hits.
    let mut hit: Option<(f32, usize)> = None;
    for i in 0..ring.len() {
        let (a, b) = (p(ring[i]), p(ring[(i + 1) % ring.len()]));
        if (a[1] > m[1]) == (b[1] > m[1]) {
            continue;
        }
        let x = a[0] + (m[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
        if x >= m[0] - 1e-7 && hit.is_none_or(|(best, _)| x < best) {
            hit = Some((x, i));
        }
    }
    let visible = match hit {
        Some((x, i)) => {
            let (ia, ib) = (i, (i + 1) % ring.len());
            let candidate = if p(ring[ia])[0] >= p(ring[ib])[0] { ia } else { ib };
            let hit_point = [x, m[1]];
            let cp = p(ring[candidate]);
            // Reflex ring vertices inside (M, I, P) would block the bridge; take the one
            // closest in angle to the ray instead.
            let mut best = candidate;
            let mut best_key = (0.0f32, 0.0f32);
            let mut found_blocker = false;
            for k in 0..ring.len() {
                if k == candidate {
                    continue;
                }
                let q = p(ring[k]);
                let prev = p(ring[(k + ring.len() - 1) % ring.len()]);
                let next = p(ring[(k + 1) % ring.len()]);
                let reflex = turn(prev, q, next) <= 0.0;
                let tri_ccw = turn(m, hit_point, cp) >= 0.0;
                let inside = if tri_ccw { in_triangle(q, m, hit_point, cp) } else { in_triangle(q, m, cp, hit_point) };
                if reflex && inside && q[0] >= m[0] {
                    let d = sub2(q, m);
                    let key = (d[1].abs().atan2(d[0]), d[0] * d[0] + d[1] * d[1]);
                    if !found_blocker || key < best_key {
                        best = k;
                        best_key = key;
                        found_blocker = true;
                    }
                }
            }
            best
        }
        // No edge to the right (a hole touching the outline): bridge to the nearest vertex.
        None => (0..ring.len())
            .min_by(|&a, &b| {
                let (da, db) = (sub2(p(ring[a]), m), sub2(p(ring[b]), m));
                (da[0] * da[0] + da[1] * da[1]).total_cmp(&(db[0] * db[0] + db[1] * db[1]))
            })
            .unwrap_or(0),
    };
    let mut out = Vec::with_capacity(ring.len() + hole.len() + 2);
    out.extend_from_slice(&ring[..=visible]);
    out.extend((0..=hole.len()).map(|k| hole[(m_at + k) % hole.len()]));
    out.push(ring[visible]);
    out.extend_from_slice(&ring[visible + 1..]);
    out
}

/// Triangulates a simple polygon with optional holes. Indices point into the outline's
/// points followed by each hole's, in order. Triangles are counter-clockwise whatever the
/// input winding. Holes must lie inside the outline and not touch each other.
pub fn triangulate(outline: &[P2], holes: &[Vec<P2>]) -> Vec<[u32; 3]> {
    let mut pts: Vec<P2> = outline.to_vec();
    let mut ring: Vec<u32> = (0..outline.len() as u32).collect();
    if signed_area(outline) < 0.0 {
        ring.reverse();
    }
    let mut hole_rings: Vec<Vec<u32>> = Vec::with_capacity(holes.len());
    for hole in holes {
        let base = pts.len() as u32;
        pts.extend_from_slice(hole);
        let mut h: Vec<u32> = (base..base + hole.len() as u32).collect();
        if signed_area(hole) > 0.0 {
            h.reverse();
        }
        if h.len() >= 3 {
            hole_rings.push(h);
        }
    }
    let max_x = |h: &Vec<u32>| h.iter().map(|&i| pts[i as usize][0]).fold(f32::MIN, f32::max);
    hole_rings.sort_by(|a, b| max_x(b).total_cmp(&max_x(a)));
    for h in &hole_rings {
        ring = bridge(&ring, h, &pts);
    }
    clip_ears(ring, &pts)
}

fn clip_ears(mut ring: Vec<u32>, pts: &[P2]) -> Vec<[u32; 3]> {
    let p = |i: u32| pts[i as usize];
    let mut tris = Vec::with_capacity(ring.len().saturating_sub(2));
    while ring.len() > 3 {
        let n = ring.len();
        let mut clipped = false;
        for i in 0..n {
            let (ia, ib, ic) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
            let (a, b, c) = (p(ia), p(ib), p(ic));
            if turn(a, b, c) <= 1e-12 {
                continue;
            }
            let blocked = ring.iter().any(|&k| {
                let q = p(k);
                q != a && q != b && q != c && in_triangle(q, a, b, c)
            });
            if !blocked {
                tris.push([ia, ib, ic]);
                ring.remove(i);
                clipped = true;
                break;
            }
        }
        if !clipped {
            // Numerical dead end: drop a collinear vertex if there is one, else force an ear.
            let i = (0..n).find(|&i| turn(p(ring[(i + n - 1) % n]), p(ring[i]), p(ring[(i + 1) % n])).abs() <= 1e-12).unwrap_or(0);
            let (ia, ib, ic) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
            if turn(p(ia), p(ib), p(ic)) > 1e-12 {
                tris.push([ia, ib, ic]);
            }
            ring.remove(i);
        }
    }
    if ring.len() == 3 && turn(p(ring[0]), p(ring[1]), p(ring[2])) > 1e-12 {
        tris.push([ring[0], ring[1], ring[2]]);
    }
    tris
}

/// Whether `p` lies strictly inside the closed polygon (even-odd rule; either winding).
pub fn point_in_polygon(p: P2, polygon: &[P2]) -> bool {
    let n = polygon.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]) {
            inside = !inside;
        }
    }
    inside
}

/// Shortest distance from `p` to the polygon's edges.
pub fn distance_to_polygon(p: P2, polygon: &[P2]) -> f32 {
    let n = polygon.len();
    (0..n)
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            let ab = sub2(b, a);
            let len2 = (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-12);
            let t = (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0);
            let d = [p[0] - a[0] - ab[0] * t, p[1] - a[1] - ab[1] * t];
            (d[0] * d[0] + d[1] * d[1]).sqrt()
        })
        .fold(f32::MAX, f32::min)
}

/// Extrudes a 2D outline in the XY plane through `depth` along Z, centred on z = 0, with hard
/// edges all round: flat front (+Z) and back (-Z) faces and one flat wall per outline edge.
/// Holes cut through. UVs: faces are a planar projection of XY; walls run u along the
/// perimeter and v across the depth.
pub fn extrude(outline: &[P2], holes: &[Vec<P2>], depth: f32) -> MeshBuilder {
    extrude_tapered(outline, holes, depth, depth)
}

/// [`extrude`] with the depth running linearly from `depth_at_min_x` at the outline's least
/// x to `depth_at_max_x` at its greatest: a wedge, thick at the root and thinner toward a
/// cutting edge (flanges, axe bits, guards). Both faces stay planar, so they shade flat.
pub fn extrude_tapered(outline: &[P2], holes: &[Vec<P2>], depth_at_min_x: f32, depth_at_max_x: f32) -> MeshBuilder {
    let mut mesh = MeshBuilder::new();
    if outline.len() < 3 {
        return mesh;
    }
    let all: Vec<P2> = outline.iter().chain(holes.iter().flatten()).copied().collect();
    let (min, max) = all.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), q| {
        ([lo[0].min(q[0]), lo[1].min(q[1])], [hi[0].max(q[0]), hi[1].max(q[1])])
    });
    let size = (max[0] - min[0]).max(max[1] - min[1]).max(1e-6);
    let uv = |q: P2| [(q[0] - min[0]) / size, (q[1] - min[1]) / size];
    // Half depth as a linear function of x: half(x) = h0 + slope * (x - min_x).
    let h0 = depth_at_min_x.max(0.0) * 0.5;
    let slope = (depth_at_max_x.max(0.0) * 0.5 - h0) / (max[0] - min[0]).max(1e-6);
    let half = |x: f32| h0 + slope * (x - min[0]);

    let tris = triangulate(outline, holes);
    for (side, front) in [(1.0f32, true), (-1.0, false)] {
        let normal = super::vec3::normalize_or([-slope, 0.0, side], [0.0, 0.0, side]);
        let base = mesh.vertex_count() as u32;
        for &q in &all {
            mesh.push_vertex([q[0], q[1], side * half(q[0])], normal, uv(q));
        }
        for t in &tris {
            if front { mesh.push_triangle(base + t[0], base + t[1], base + t[2]) } else { mesh.push_triangle(base + t[0], base + t[2], base + t[1]) };
        }
    }

    let outer_ccw = signed_area(outline) >= 0.0;
    let loops = std::iter::once((outline, outer_ccw)).chain(holes.iter().map(|h| (h.as_slice(), signed_area(h) < 0.0)));
    for (ring, keeps_order) in loops {
        // Outline counter-clockwise, holes clockwise: the solid is on the left of each edge.
        let ring: Vec<P2> = if keeps_order { ring.to_vec() } else { ring.iter().rev().copied().collect() };
        let perimeter: f32 = (0..ring.len()).map(|i| {
            let d = sub2(ring[(i + 1) % ring.len()], ring[i]);
            (d[0] * d[0] + d[1] * d[1]).sqrt()
        }).sum::<f32>().max(1e-6);
        let mut along = 0.0;
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            let d = sub2(b, a);
            let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
            if len < 1e-7 {
                continue;
            }
            let outward: V3 = [d[1], -d[0], 0.0];
            let (u0, u1) = (along / perimeter, (along + len) / perimeter);
            along += len;
            let (ha, hb) = (half(a[0]), half(b[0]));
            mesh.push_flat_quad(
                [[a[0], a[1], -ha], [b[0], b[1], -hb], [b[0], b[1], hb], [a[0], a[1], ha]],
                [[u0, 0.0], [u1, 0.0], [u1, 1.0], [u0, 1.0]],
                outward,
            );
        }
    }
    mesh
}
