//! [`Placement`]: where a form's or an item's local space sits on a body.

use armory::geom::vec3::{self, Mat3, V3};
use armory::geom::MeshBuilder;
use armory::{ItemMesh, Part};

/// A rigid placement of a local space in body space: the local axes map onto the columns of
/// `rotation`, and the local origin onto `origin`. A placement may mirror (a left hand built
/// from a right [`armory::build::form::HandForm`]); the `apply` methods keep faces outward
/// either way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub rotation: Mat3,
    pub origin: V3,
}

impl Placement {
    pub const IDENTITY: Placement = Placement { rotation: Mat3::IDENTITY, origin: vec3::ZERO };

    /// Local axes `x`, `y`, `z` (orthonormal) at `origin`.
    pub fn new(origin: V3, x: V3, y: V3, z: V3) -> Placement {
        Placement { rotation: Mat3::from_cols(x, y, z), origin }
    }

    pub fn translation(origin: V3) -> Placement {
        Placement { origin, ..Placement::IDENTITY }
    }

    /// The local `+X`, `+Y` and `+Z` in body space.
    pub fn x(&self) -> V3 {
        self.rotation.cols[0]
    }

    pub fn y(&self) -> V3 {
        self.rotation.cols[1]
    }

    pub fn z(&self) -> V3 {
        self.rotation.cols[2]
    }

    /// Whether the placement mirrors (a left-handed basis).
    pub fn is_mirrored(&self) -> bool {
        self.rotation.determinant() < 0.0
    }

    /// A local point in body space.
    pub fn point(&self, p: V3) -> V3 {
        vec3::add(self.origin, self.rotation.transform(p))
    }

    /// A local direction in body space.
    pub fn direction(&self, d: V3) -> V3 {
        self.rotation.transform(d)
    }

    /// This placement applied after `inner`: `inner`'s local space placed in this one's.
    pub fn then(&self, inner: &Placement) -> Placement {
        Placement { rotation: self.rotation.mul_mat(&inner.rotation), origin: self.point(inner.origin) }
    }

    /// Moves a mesh built in local space onto the body.
    pub fn apply(&self, mesh: &mut MeshBuilder) {
        mesh.transform(&self.rotation).translate(self.origin);
    }

    /// Moves a finished part onto the body.
    pub fn apply_to_part(&self, part: &mut Part) {
        part.transform(&self.rotation, self.origin);
        if self.is_mirrored() {
            for tri in part.indices.as_chunks_mut::<3>().0 {
                tri.swap(1, 2);
            }
        }
    }

    /// Moves every part of an item onto the body.
    pub fn apply_to_item(&self, mesh: &mut ItemMesh) {
        for part in &mut mesh.parts {
            self.apply_to_part(part);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use armory::geom::{OutlineMode, uv_sphere};

    fn volume(part: &Part) -> f32 {
        part.indices
            .chunks(3)
            .map(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| part.positions[i as usize]);
                vec3::dot(a, vec3::cross(b, c)) / 6.0
            })
            .sum()
    }

    #[test]
    fn mirrored_placements_keep_faces_outward() {
        let mirror = Placement::new([0.0; 3], [-1.0, 0.0, 0.0], vec3::Y, vec3::Z);
        assert!(mirror.is_mirrored());
        let mut part = uv_sphere(1.0, 12, 8).into_part("ball", OutlineMode::Shading);
        let before = volume(&part);
        mirror.apply_to_part(&mut part);
        assert!(volume(&part) > 0.0 && (volume(&part) - before).abs() < 1e-3);
        let mut mesh = uv_sphere(1.0, 12, 8);
        mirror.apply(&mut mesh);
        assert!(volume(&mesh.into_part("ball", OutlineMode::Shading)) > 0.0);
    }

    #[test]
    fn placements_compose() {
        let a = Placement::new([1.0, 0.0, 0.0], vec3::Z, vec3::Y, vec3::neg(vec3::X));
        let b = Placement::translation([0.0, 2.0, 1.0]);
        let p = a.then(&b).point([1.0, 0.0, 0.0]);
        let q = a.point(b.point([1.0, 0.0, 0.0]));
        assert!(vec3::distance(p, q) < 1e-6);
    }
}
