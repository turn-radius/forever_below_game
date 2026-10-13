//! [`Parts`]: meshes on their way to becoming an item's [`Part`]s, in build order. Families
//! add one mesh per section and material, then [`Parts::finish`] welds the outline normals
//! and drops anything empty.

use crate::build::gem::stone_part;
use crate::gem::Gemstone;
use crate::geom::{ItemMesh, MeshBuilder, OutlineMode, Part, V3};
use crate::material::{MaterialKind, Rgb};

/// How a collected mesh is finished.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Look {
    Material { material: MaterialKind, tint: Option<Rgb>, outline: bool },
    Stone { stone: Gemstone, glow: f32, outline: bool },
}

/// Meshes collected for an item, in build order. Empty meshes are dropped.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parts(Vec<(&'static str, MeshBuilder, Look)>);

impl Parts {
    pub fn new() -> Self {
        Self::default()
    }

    /// A part of `material`, coloured by the item's palette.
    pub fn add(&mut self, name: &'static str, mesh: MeshBuilder, material: MaterialKind) -> &mut Self {
        self.push(name, mesh, Look::Material { material, tint: None, outline: true })
    }

    /// A part of `material` with its own colour (fletching, anything the palette does not
    /// colour).
    pub fn add_tinted(&mut self, name: &'static str, mesh: MeshBuilder, material: MaterialKind, tint: Rgb) -> &mut Self {
        self.push(name, mesh, Look::Material { material, tint: Some(tint), outline: true })
    }

    /// A part too fine for an ink outline (thread, wire, a string).
    pub fn add_unoutlined(&mut self, name: &'static str, mesh: MeshBuilder, material: MaterialKind) -> &mut Self {
        self.push(name, mesh, Look::Material { material, tint: None, outline: false })
    }

    /// A cut stone, coloured by `stone`, glowing by `glow`.
    pub fn stone(&mut self, name: &'static str, mesh: MeshBuilder, stone: Gemstone, glow: f32, outline: bool) -> &mut Self {
        self.push(name, mesh, Look::Stone { stone, glow, outline })
    }

    fn push(&mut self, name: &'static str, mesh: MeshBuilder, look: Look) -> &mut Self {
        if !mesh.is_empty() {
            self.0.push((name, mesh, look));
        }
        self
    }

    /// Appends every part of `other`, in order.
    pub fn extend(&mut self, other: Parts) -> &mut Self {
        self.0.extend(other.0);
        self
    }

    /// Whether a part of this name has been collected.
    pub fn contains(&self, name: &str) -> bool {
        self.0.iter().any(|(n, _, _)| *n == name)
    }

    /// Rotates every mesh's +Y onto `up`, then moves its origin to `origin` (see
    /// [`MeshBuilder::place`]): sets an assembly built upright onto a point of a haft.
    pub fn place(&mut self, origin: V3, up: V3) -> &mut Self {
        for (_, mesh, _) in &mut self.0 {
            mesh.place(origin, up);
        }
        self
    }

    /// Every collected mesh, mutably (to place a whole assembly at once).
    pub fn meshes_mut(&mut self) -> impl Iterator<Item = &mut MeshBuilder> {
        self.0.iter_mut().map(|(_, m, _)| m)
    }

    /// Finishes every mesh into a [`Part`] with welded outline normals.
    pub fn finish(self) -> ItemMesh {
        let parts = self
            .0
            .into_iter()
            .map(|(name, mesh, look)| match look {
                Look::Material { material, tint, outline } => {
                    let part = mesh.into_part(name, OutlineMode::Welded).with_material(material);
                    let part = match tint {
                        Some(t) => part.with_tint(t),
                        None => part,
                    };
                    if outline { part } else { part.without_outline() }
                }
                Look::Stone { stone, glow, outline } => {
                    let part: Part = stone_part(name, mesh, stone, glow);
                    if outline { part } else { part.without_outline() }
                }
            })
            .collect();
        ItemMesh { parts }
    }
}
