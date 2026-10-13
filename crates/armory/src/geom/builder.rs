//! [`MeshBuilder`]: the working form of a part. Primitives return one, families transform and
//! merge them, and [`MeshBuilder::into_part`] finishes it into a [`Part`] with outline normals.

use super::mesh::Part;
use super::outline::{OutlineMode, smooth_outline_normals, welded_outline_normals};
use super::vec3::{self, Mat3, V3};

/// Growable triangle mesh: positions, shading normals, uvs and a counter-clockwise index list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshBuilder {
    pub positions: Vec<V3>,
    pub normals: Vec<V3>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl MeshBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Axis-aligned bounds `(min, max)` of every vertex, or `None` for an empty mesh.
    pub fn bounds(&self) -> Option<(V3, V3)> {
        let first = *self.positions.first()?;
        Some(self.positions.iter().fold((first, first), |(lo, hi), p| {
            ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
        }))
    }

    /// Rotates the mesh's +Y onto `up`, then moves its origin to `origin`: sets a part built
    /// upright onto a point of a haft, a head or a surface.
    pub fn place(&mut self, origin: V3, up: V3) -> &mut Self {
        self.align_y_to(up).translate(origin)
    }

    /// Rotates the mesh's +Y onto `up` and its +Z onto `forward` (made perpendicular to `up`),
    /// then moves its origin to `origin`: sets a part that has a facing as well as an up,
    /// such as a buckle on a strap or a lock plate on a wall.
    pub fn place_basis(&mut self, origin: V3, up: V3, forward: V3) -> &mut Self {
        let y = vec3::normalize_or(up, vec3::Y);
        let z = vec3::try_normalize(vec3::sub(forward, vec3::scale(y, vec3::dot(forward, y)))).unwrap_or_else(|| vec3::any_perpendicular(y));
        let x = vec3::cross(y, z);
        self.transform(&Mat3::from_cols(x, y, z)).translate(origin)
    }

    /// Appends one vertex and returns its index.
    pub fn push_vertex(&mut self, position: V3, normal: V3, uv: [f32; 2]) -> u32 {
        let index = self.positions.len() as u32;
        self.positions.push(position);
        self.normals.push(normal);
        self.uvs.push(uv);
        index
    }

    /// Appends a triangle over existing vertices, counter-clockwise when seen from the front.
    pub fn push_triangle(&mut self, a: u32, b: u32, c: u32) -> &mut Self {
        self.indices.extend_from_slice(&[a, b, c]);
        self
    }

    /// Appends a flat triangle with its own vertices, wound so its front faces along `facing`.
    pub fn push_flat_triangle(&mut self, corners: [V3; 3], uvs: [[f32; 2]; 3], facing: V3) -> &mut Self {
        let [a, b, c] = corners;
        let face = vec3::cross(vec3::sub(b, a), vec3::sub(c, a));
        let normal = vec3::normalize_or(face, vec3::normalize_or(facing, vec3::Y));
        let flip = vec3::dot(face, facing) < 0.0;
        let normal = if flip { vec3::neg(normal) } else { normal };
        let ids: Vec<u32> = (0..3).map(|i| self.push_vertex(corners[i], normal, uvs[i])).collect();
        if flip { self.push_triangle(ids[0], ids[2], ids[1]) } else { self.push_triangle(ids[0], ids[1], ids[2]) }
    }

    /// Appends a flat quad (corners in order around its edge) with its own vertices, wound so
    /// its front faces along `facing`.
    pub fn push_flat_quad(&mut self, corners: [V3; 4], uvs: [[f32; 2]; 4], facing: V3) -> &mut Self {
        let [a, b, c, d] = corners;
        // Cross of the diagonals: the quad's area-weighted normal, robust to one collapsed edge.
        let face = vec3::cross(vec3::sub(c, a), vec3::sub(d, b));
        let flip = vec3::dot(face, facing) < 0.0;
        let normal = vec3::normalize_or(if flip { vec3::neg(face) } else { face }, vec3::normalize_or(facing, vec3::Y));
        let ids: Vec<u32> = (0..4).map(|i| self.push_vertex(corners[i], normal, uvs[i])).collect();
        if flip {
            self.push_triangle(ids[0], ids[2], ids[1]).push_triangle(ids[0], ids[3], ids[2])
        } else {
            self.push_triangle(ids[0], ids[1], ids[2]).push_triangle(ids[0], ids[2], ids[3])
        }
    }

    /// Closes a star-shaped ring of points with a flat fan around its centroid, facing `facing`.
    /// The cap gets its own vertices, so its rim is a hard edge. UVs are a planar projection.
    pub fn push_fan_cap(&mut self, ring: &[V3], facing: V3) -> &mut Self {
        if ring.len() < 3 {
            return self;
        }
        let normal = vec3::normalize_or(facing, vec3::Y);
        let center = vec3::centroid(ring);
        let u_axis = vec3::any_perpendicular(normal);
        let v_axis = vec3::cross(normal, u_axis);
        let radius = ring.iter().map(|&p| vec3::distance(p, center)).fold(1e-6f32, f32::max);
        let uv = |p: V3| {
            let d = vec3::sub(p, center);
            [0.5 + 0.5 * vec3::dot(d, u_axis) / radius, 0.5 + 0.5 * vec3::dot(d, v_axis) / radius]
        };
        let c = self.push_vertex(center, normal, [0.5, 0.5]);
        let rim: Vec<u32> = ring.iter().map(|&p| self.push_vertex(p, normal, uv(p))).collect();
        for i in 0..rim.len() {
            let (a, b) = (rim[i], rim[(i + 1) % rim.len()]);
            let face = vec3::cross(vec3::sub(ring[i], center), vec3::sub(ring[(i + 1) % ring.len()], center));
            if vec3::length_squared(face) < 1e-14 {
                continue;
            }
            if vec3::dot(face, normal) >= 0.0 { self.push_triangle(c, a, b) } else { self.push_triangle(c, b, a) };
        }
        self
    }

    /// Appends another mesh (its indices are offset onto the new vertices).
    pub fn append(&mut self, other: &MeshBuilder) -> &mut Self {
        let offset = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.uvs.extend_from_slice(&other.uvs);
        self.indices.extend(other.indices.iter().map(|i| i + offset));
        self
    }

    /// Merges many meshes into one.
    pub fn merged<'a>(meshes: impl IntoIterator<Item = &'a MeshBuilder>) -> MeshBuilder {
        let mut out = MeshBuilder::new();
        for m in meshes {
            out.append(m);
        }
        out
    }

    pub fn translate(&mut self, offset: V3) -> &mut Self {
        for p in &mut self.positions {
            *p = vec3::add(*p, offset);
        }
        self
    }

    /// Any linear map (rotation, scale, shear, mirror) about the origin. Normals use the
    /// cofactor matrix, and a mirroring map reverses the winding so faces stay front facing.
    pub fn transform(&mut self, matrix: &Mat3) -> &mut Self {
        let normal_matrix = matrix.cofactor();
        let det = matrix.determinant();
        for p in &mut self.positions {
            *p = matrix.transform(*p);
        }
        for n in &mut self.normals {
            let mapped = normal_matrix.transform(*n);
            let mapped = if det < 0.0 { vec3::neg(mapped) } else { mapped };
            *n = vec3::normalize_or(mapped, *n);
        }
        if det < 0.0 {
            self.flip_winding_only();
        }
        self
    }

    /// Rotates about an axis through the origin, right-handed, by `angle` radians.
    pub fn rotate(&mut self, axis: V3, angle: f32) -> &mut Self {
        self.transform(&Mat3::from_axis_angle(axis, angle))
    }

    pub fn rotate_x(&mut self, angle: f32) -> &mut Self {
        self.rotate(vec3::X, angle)
    }

    pub fn rotate_y(&mut self, angle: f32) -> &mut Self {
        self.rotate(vec3::Y, angle)
    }

    pub fn rotate_z(&mut self, angle: f32) -> &mut Self {
        self.rotate(vec3::Z, angle)
    }

    /// Rotates so the mesh's +Y axis points along `direction` (place a spike on a surface).
    pub fn align_y_to(&mut self, direction: V3) -> &mut Self {
        self.transform(&Mat3::from_rotation_arc(vec3::Y, direction))
    }

    /// Scales per axis about the origin. Negative factors mirror (and keep faces outward).
    pub fn scale(&mut self, factors: V3) -> &mut Self {
        self.transform(&Mat3::from_scale(factors))
    }

    pub fn scale_uniform(&mut self, factor: f32) -> &mut Self {
        self.scale([factor; 3])
    }

    /// Turns the mesh inside out: reverses every triangle and negates the normals.
    pub fn flip(&mut self) -> &mut Self {
        for n in &mut self.normals {
            *n = vec3::neg(*n);
        }
        self.flip_winding_only()
    }

    fn flip_winding_only(&mut self) -> &mut Self {
        for tri in self.indices.as_chunks_mut::<3>().0 {
            tri.swap(1, 2);
        }
        self
    }

    /// Adds a flipped copy so the surface shows from both sides (cloth, banners, leaves).
    pub fn make_double_sided(&mut self) -> &mut Self {
        let mut back = self.clone();
        back.flip();
        self.append(&back)
    }

    /// Unshares every vertex and gives each triangle its own face normal (cut gems, rough stone).
    pub fn flat_shaded(&self) -> MeshBuilder {
        let mut out = MeshBuilder::new();
        for tri in self.indices.as_chunks::<3>().0 {
            let corners = [tri[0], tri[1], tri[2]].map(|i| self.positions[i as usize]);
            let uvs = [tri[0], tri[1], tri[2]].map(|i| self.uvs.get(i as usize).copied().unwrap_or_default());
            let face = vec3::cross(vec3::sub(corners[1], corners[0]), vec3::sub(corners[2], corners[0]));
            let fallback = self.normals.get(tri[0] as usize).copied().unwrap_or(vec3::Y);
            let normal = vec3::normalize_or(face, fallback);
            let base = out.positions.len() as u32;
            for (p, uv) in corners.into_iter().zip(uvs) {
                out.push_vertex(p, normal, uv);
            }
            out.push_triangle(base, base + 1, base + 2);
        }
        out
    }

    /// Finishes into a drawable [`Part`] with the default look (outlined, no material yet).
    pub fn into_part(self, name: &'static str, outline: OutlineMode) -> Part {
        let outline_normals = match outline {
            OutlineMode::Welded => welded_outline_normals(&self.positions, &self.normals, &self.indices),
            OutlineMode::Shading => smooth_outline_normals(&self.normals),
        };
        Part {
            name,
            material: None,
            tint: None,
            glow: 0.0,
            outline: true,
            positions: self.positions,
            normals: self.normals,
            uvs: self.uvs,
            outline_normals,
            indices: self.indices,
        }
    }
}
