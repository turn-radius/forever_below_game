//! [`PartList`]: meshes on their way to becoming an item's [`Part`]s, each with the material
//! or stone it will be drawn in. Families collect sections into one, move groups of them
//! (a head assembly onto the top of a haft), and finish it into an [`ItemMesh`].

use super::gem::stone_part;
use crate::gem::Gemstone;
use crate::geom::{ItemMesh, MeshBuilder, OutlineMode, Part, V3};
use crate::material::MaterialKind;

/// How a listed mesh is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PartLook {
    Material(MaterialKind),
    /// A cut stone in its own colour, glowing by `glow`; small stones skip the ink outline.
    Stone { stone: Gemstone, glow: f32, outline: bool },
}

/// Named meshes in build order. Empty meshes are dropped as they are added.
#[derive(Clone, Debug, Default)]
pub struct PartList(pub Vec<(&'static str, MeshBuilder, PartLook)>);

impl PartList {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a mesh drawn in `material`.
    pub fn add(&mut self, name: &'static str, mesh: MeshBuilder, material: MaterialKind) {
        if !mesh.is_empty() {
            self.0.push((name, mesh, PartLook::Material(material)));
        }
    }

    /// Adds a stone drawn in its own colour.
    pub fn stone(&mut self, name: &'static str, mesh: MeshBuilder, stone: Gemstone, glow: f32, outline: bool) {
        if !mesh.is_empty() {
            self.0.push((name, mesh, PartLook::Stone { stone, glow, outline }));
        }
    }

    /// Appends every part of `other`, in order.
    pub fn extend(&mut self, other: PartList) {
        self.0.extend(other.0);
    }

    /// Whether a part of this name is listed.
    pub fn contains(&self, name: &str) -> bool {
        self.0.iter().any(|(n, _, _)| *n == name)
    }

    /// Rotates every part's +Y onto `up`, then moves its origin to `origin` (see
    /// [`MeshBuilder::place`]): sets an assembly built upright onto a point of a haft.
    pub fn place(&mut self, origin: V3, up: V3) {
        for (_, mesh, _) in &mut self.0 {
            mesh.place(origin, up);
        }
    }

    /// Finishes every mesh into a [`Part`] with welded outline normals.
    pub fn finish(self) -> ItemMesh {
        let parts = self
            .0
            .into_iter()
            .map(|(name, mesh, look)| -> Part {
                match look {
                    PartLook::Material(m) => mesh.into_part(name, OutlineMode::Welded).with_material(m),
                    PartLook::Stone { stone, glow, outline } => {
                        let part = stone_part(name, mesh, stone, glow);
                        if outline { part } else { part.without_outline() }
                    }
                }
            })
            .collect();
        ItemMesh { parts }
    }
}
