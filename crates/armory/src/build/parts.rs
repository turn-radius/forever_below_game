//! [`Parts`]: meshes on their way to becoming an item's [`Part`]s, in build order. Families
//! add one mesh per section and material, then [`Parts::finish`] welds the outline normals
//! and drops anything empty.

use crate::fabric::{Fabric, SurfacePattern};
use crate::gem::Gemstone;
use crate::geom::{ItemMesh, MeshBuilder, OutlineMode, Part, V3};
use crate::material::{MaterialKind, Rgb};

/// How a collected mesh is finished: its material, an optional colour of its own, a shader
/// pattern (fabric, quilting, mail, scales), a glow, and whether it gets an ink outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub material: MaterialKind,
    pub tint: Option<Rgb>,
    pub pattern: Option<SurfacePattern>,
    /// 0..=1 emissive strength.
    pub glow: f32,
    pub outline: bool,
}

impl Look {
    /// A material coloured by the item's palette, outlined.
    pub fn of(material: MaterialKind) -> Self {
        Look { material, tint: None, pattern: None, glow: 0.0, outline: true }
    }

    /// Dyed cloth with its pattern.
    pub fn cloth(fabric: &Fabric) -> Self {
        Look { tint: Some(fabric.color), pattern: fabric.surface(), ..Look::of(MaterialKind::Cloth) }
    }

    /// A material with a shader pattern (mail rings, scales, rivets).
    pub fn patterned(material: MaterialKind, pattern: SurfacePattern) -> Self {
        Look { pattern: Some(pattern), ..Look::of(material) }
    }

    /// A cut stone: gem material in the stone's own colour.
    pub fn stone(stone: Gemstone, glow: f32) -> Self {
        Look { glow: glow.clamp(0.0, 1.0), ..Look::of(MaterialKind::Gem).tinted(stone.rgb()) }
    }

    pub fn tinted(mut self, tint: Rgb) -> Self {
        self.tint = Some(tint);
        self
    }

    pub fn without_outline(mut self) -> Self {
        self.outline = false;
        self
    }
}

/// Meshes collected for an item, in build order. Empty meshes are dropped.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parts(Vec<(&'static str, MeshBuilder, Look)>);

impl Parts {
    pub fn new() -> Self {
        Self::default()
    }

    /// A part finished with any [`Look`]. Every other `add` is a shorthand for this one.
    pub fn add_look(&mut self, name: &'static str, mesh: MeshBuilder, look: Look) -> &mut Self {
        if !mesh.is_empty() {
            self.0.push((name, mesh, look));
        }
        self
    }

    /// A part of `material`, coloured by the item's palette.
    pub fn add(&mut self, name: &'static str, mesh: MeshBuilder, material: MaterialKind) -> &mut Self {
        self.add_look(name, mesh, Look::of(material))
    }

    /// A part of `material` with its own colour (fletching, anything the palette does not
    /// colour).
    pub fn add_tinted(&mut self, name: &'static str, mesh: MeshBuilder, material: MaterialKind, tint: Rgb) -> &mut Self {
        self.add_look(name, mesh, Look::of(material).tinted(tint))
    }

    /// A part too fine for an ink outline (thread, wire, a string).
    pub fn add_unoutlined(&mut self, name: &'static str, mesh: MeshBuilder, material: MaterialKind) -> &mut Self {
        self.add_look(name, mesh, Look::of(material).without_outline())
    }

    /// A cut stone, coloured by `stone`, glowing by `glow`.
    pub fn stone(&mut self, name: &'static str, mesh: MeshBuilder, stone: Gemstone, glow: f32, outline: bool) -> &mut Self {
        let look = Look::stone(stone, glow);
        self.add_look(name, mesh, if outline { look } else { look.without_outline() })
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
            .map(|(name, mesh, look)| {
                let part: Part = mesh
                    .into_part(name, OutlineMode::Welded)
                    .with_material(look.material)
                    .with_pattern(look.pattern)
                    .with_glow(look.glow);
                let part = match look.tint {
                    Some(t) => part.with_tint(t),
                    None => part,
                };
                if look.outline { part } else { part.without_outline() }
            })
            .collect();
        ItemMesh { parts }
    }
}
