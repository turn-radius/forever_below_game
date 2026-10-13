//! Bevelled slabs: a 2D outline (with holes) extruded through a depth, its edges chamfered or
//! rounded, the walls smooth where the outline curves and creased where it turns sharply.
//! Pendants, signet faces, devices, plaques, coins, badges, hammer heads, shield bosses.
//!
//! [`super::extrude`] gives every outline edge its own flat wall, right for flanges and
//! blades whose outlines are a few straight runs. A disc or an oval extruded that way shows
//! every edge as a facet; a slab shades it as the round thing it is.

use super::builder::MeshBuilder;
use super::polygon::{P2, signed_area, triangulate};
use super::vec3::{self, V3};

/// How [`bevelled_slab`] shapes the extrusion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlabOptions {
    /// Total thickness along Z, centred on z = 0.
    pub depth: f32,
    /// Width (and height) of the bevel at each face's rim. Clamped below half the depth.
    pub bevel: f32,
    /// 1 cuts a flat chamfer with hard edges; 2 or more round it over (soft, cast look).
    pub bevel_segments: usize,
    /// Outline corners that turn more than this many radians get a hard edge; gentler turns
    /// shade smooth.
    pub crease: f32,
}

impl SlabOptions {
    /// A flat chamfer; corners sharper than 40 degrees crease.
    pub fn chamfered(depth: f32, bevel: f32) -> Self {
        SlabOptions { depth, bevel, bevel_segments: 1, crease: 40f32.to_radians() }
    }

    /// A rounded-over rim in three steps; corners sharper than 40 degrees crease.
    pub fn rounded(depth: f32, bevel: f32) -> Self {
        SlabOptions { depth, bevel, bevel_segments: 3, crease: 40f32.to_radians() }
    }
}

/// One loop of an outline, with the solid on the left of travel.
struct Ring {
    points: Vec<P2>,
    /// Per vertex: the 2D outward normal coming in along the previous edge and going out
    /// along the next. Equal where the vertex is smooth.
    normals: Vec<(P2, P2)>,
    /// Per vertex: the offset that insets the vertex by one unit toward the solid.
    miters: Vec<P2>,
    /// Per vertex: perimeter parameter 0..1.
    u: Vec<f32>,
}

fn norm2(v: P2) -> P2 {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l > 1e-12 { [v[0] / l, v[1] / l] } else { [0.0, 0.0] }
}

impl Ring {
    fn new(loop_points: &[P2], solid_left_when_ccw: bool, crease: f32) -> Option<Ring> {
        let mut points: Vec<P2> = Vec::with_capacity(loop_points.len());
        for &p in loop_points {
            if points.last().is_none_or(|q: &P2| (q[0] - p[0]).hypot(q[1] - p[1]) > 1e-7) {
                points.push(p);
            }
        }
        while points.len() > 1 {
            let (a, b) = (points[0], points[points.len() - 1]);
            if (a[0] - b[0]).hypot(a[1] - b[1]) > 1e-7 {
                break;
            }
            points.pop();
        }
        if points.len() < 3 {
            return None;
        }
        let ccw = signed_area(&points) >= 0.0;
        if ccw != solid_left_when_ccw {
            points.reverse();
        }
        let n = points.len();
        // Outward is the right of travel.
        let edge: Vec<P2> = (0..n)
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % n]);
                norm2([b[1] - a[1], -(b[0] - a[0])])
            })
            .collect();
        let cos_crease = crease.cos();
        let mut normals = Vec::with_capacity(n);
        let mut miters = Vec::with_capacity(n);
        for i in 0..n {
            let (np, nn) = (edge[(i + n - 1) % n], edge[i]);
            let c = np[0] * nn[0] + np[1] * nn[1];
            let avg = norm2([np[0] + nn[0], np[1] + nn[1]]);
            normals.push(if c < cos_crease { (np, nn) } else { (avg, avg) });
            // Miter: length 1 / cos(half the turn), capped so spikes do not shoot out.
            let k = (1.0 + c).max(1e-6);
            let mut m = [(np[0] + nn[0]) / k, (np[1] + nn[1]) / k];
            let len = m[0].hypot(m[1]);
            if !len.is_finite() || len < 1e-6 {
                m = np;
            } else if len > 2.5 {
                m = [m[0] * 2.5 / len, m[1] * 2.5 / len];
            }
            miters.push(m);
        }
        let mut u = vec![0.0f32; n];
        for i in 1..n {
            u[i] = u[i - 1] + (points[i][0] - points[i - 1][0]).hypot(points[i][1] - points[i - 1][1]);
        }
        let total = u[n - 1] + (points[0][0] - points[n - 1][0]).hypot(points[0][1] - points[n - 1][1]);
        u.iter_mut().for_each(|x| *x /= total.max(1e-9));
        Some(Ring { points, normals, miters, u })
    }

    fn inset(&self, i: usize, s: f32) -> P2 {
        let (p, m) = (self.points[i], self.miters[i]);
        [p[0] - m[0] * s, p[1] - m[1] * s]
    }

    fn inset_all(&self, s: f32) -> Vec<P2> {
        (0..self.points.len()).map(|i| self.inset(i, s)).collect()
    }
}

/// One ring of a bevel profile: inset from the outline, height, and how much of the normal
/// is the 2D outward normal (`xy`) and how much faces along Z (`z`).
#[derive(Clone, Copy)]
struct Step {
    inset: f32,
    z: f32,
    xy: f32,
    nz: f32,
}

fn band(mesh: &mut MeshBuilder, ring: &Ring, a: Step, b: Step, v: (f32, f32)) {
    let n = ring.points.len();
    let normal = |n2: P2, s: Step| vec3::normalize_or([n2[0] * s.xy, n2[1] * s.xy, s.nz], [n2[0], n2[1], 0.0]);
    for i in 0..n {
        let j = (i + 1) % n;
        let (pa_i, pa_j, pb_i, pb_j) = (ring.inset(i, a.inset), ring.inset(j, a.inset), ring.inset(i, b.inset), ring.inset(j, b.inset));
        let corners: [V3; 4] = [[pa_i[0], pa_i[1], a.z], [pa_j[0], pa_j[1], a.z], [pb_j[0], pb_j[1], b.z], [pb_i[0], pb_i[1], b.z]];
        let (ni, nj) = (ring.normals[i].1, ring.normals[j].0);
        let normals = [normal(ni, a), normal(nj, a), normal(nj, b), normal(ni, b)];
        let uj = if j == 0 { 1.0 } else { ring.u[j] };
        let uvs = [[ring.u[i], v.0], [uj, v.0], [uj, v.1], [ring.u[i], v.1]];
        let face = vec3::cross(vec3::sub(corners[2], corners[0]), vec3::sub(corners[3], corners[1]));
        if vec3::length_squared(face) < 1e-18 {
            continue;
        }
        let facing = normals.iter().fold(vec3::ZERO, |s, &x| vec3::add(s, x));
        let ids: Vec<u32> = (0..4).map(|k| mesh.push_vertex(corners[k], normals[k], uvs[k])).collect();
        if vec3::dot(face, facing) >= 0.0 {
            mesh.push_triangle(ids[0], ids[1], ids[2]).push_triangle(ids[0], ids[2], ids[3]);
        } else {
            mesh.push_triangle(ids[0], ids[2], ids[1]).push_triangle(ids[0], ids[3], ids[2]);
        }
    }
}

/// Extrudes `outline` (any winding; `holes` cut through it) along Z through
/// `options.depth`, centred on z = 0, front face +Z. Each face is inset by the bevel and
/// joined to the full outline by a chamfer or a rounded rim; the wall between is smooth
/// along curves and creased at sharp corners. UVs: faces are a planar projection of XY;
/// rims and walls run u round the perimeter.
pub fn bevelled_slab(outline: &[P2], holes: &[Vec<P2>], options: &SlabOptions) -> MeshBuilder {
    let mut mesh = MeshBuilder::new();
    let Some(outer) = Ring::new(outline, true, options.crease) else {
        return mesh;
    };
    let inner: Vec<Ring> = holes.iter().filter_map(|h| Ring::new(h, false, options.crease)).collect();
    let h = options.depth.max(1e-6) * 0.5;
    let b = options.bevel.clamp(0.0, h * 0.98);

    // The profile from the front face's rim down the wall to the back face's rim.
    let mut steps: Vec<Step> = Vec::new();
    if b > 1e-7 {
        if options.bevel_segments <= 1 {
            let d = std::f32::consts::FRAC_1_SQRT_2;
            steps.push(Step { inset: b, z: h, xy: d, nz: d });
            steps.push(Step { inset: 0.0, z: h - b, xy: d, nz: d });
        } else {
            let k = options.bevel_segments;
            for s in 0..=k {
                let (sin, cos) = (std::f32::consts::FRAC_PI_2 * s as f32 / k as f32).sin_cos();
                steps.push(Step { inset: b * (1.0 - sin), z: h - b + b * cos, xy: sin, nz: cos });
            }
        }
    }
    let wall_top = Step { inset: 0.0, z: h - b, xy: 1.0, nz: 0.0 };
    let wall_bottom = Step { z: -(h - b), ..wall_top };
    let mirror = |s: Step| Step { z: -s.z, nz: -s.nz, ..s };
    let total_v = 2.0 * h;
    let v_of = |z: f32| 0.5 - z / total_v.max(1e-9) * 0.5;

    for ring in std::iter::once(&outer).chain(inner.iter()) {
        for w in steps.windows(2) {
            band(&mut mesh, ring, w[0], w[1], (v_of(w[0].z), v_of(w[1].z)));
            band(&mut mesh, ring, mirror(w[1]), mirror(w[0]), (v_of(-w[1].z), v_of(-w[0].z)));
        }
        band(&mut mesh, ring, wall_top, wall_bottom, (v_of(wall_top.z), v_of(wall_bottom.z)));
    }

    // Faces.
    let face_outline = outer.inset_all(b);
    let face_holes: Vec<Vec<P2>> = inner.iter().map(|r| r.inset_all(b)).collect();
    let all: Vec<P2> = face_outline.iter().chain(face_holes.iter().flatten()).copied().collect();
    let (min, max) = all.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), q| {
        ([lo[0].min(q[0]), lo[1].min(q[1])], [hi[0].max(q[0]), hi[1].max(q[1])])
    });
    let size = (max[0] - min[0]).max(max[1] - min[1]).max(1e-6);
    let tris = triangulate(&face_outline, &face_holes);
    for (side, front) in [(1.0f32, true), (-1.0, false)] {
        let base = mesh.vertex_count() as u32;
        for &q in &all {
            mesh.push_vertex([q[0], q[1], side * h], [0.0, 0.0, side], [(q[0] - min[0]) / size, (q[1] - min[1]) / size]);
        }
        for t in &tris {
            if front { mesh.push_triangle(base + t[0], base + t[1], base + t[2]) } else { mesh.push_triangle(base + t[0], base + t[2], base + t[1]) };
        }
    }
    mesh
}
