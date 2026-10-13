//! Plain mesh data for one item. The game turns each [`Part`] into a Bevy mesh with a toon
//! material and an ink outline.

use crate::fabric::SurfacePattern;
use crate::material::{MaterialKind, Rgb};

/// One drawable piece of an item with a single material.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Part {
    /// Which section made it (`"shaft"`, `"flanges"`...), for debugging and per-part effects.
    pub name: &'static str,
    pub material: Option<MaterialKind>,
    /// Overrides the palette colour for this part (gems carry their own colour).
    pub tint: Option<Rgb>,
    /// A pattern the shader draws over the part: fabric, quilting, mail rings, scales.
    pub pattern: Option<SurfacePattern>,
    /// 0..=1 emissive strength (glowing gems).
    pub glow: f32,
    /// False for small parts that should not get an ink outline.
    pub outline: bool,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Normals smoothed across hard edges, for the inverted-hull outline.
    pub outline_normals: Vec<[f32; 3]>,
    /// Triangle list, counter-clockwise front faces.
    pub indices: Vec<u32>,
}

impl Part {
    pub fn with_material(mut self, material: MaterialKind) -> Self {
        self.material = Some(material);
        self
    }

    pub fn with_tint(mut self, tint: Rgb) -> Self {
        self.tint = Some(tint);
        self
    }

    pub fn with_pattern(mut self, pattern: Option<SurfacePattern>) -> Self {
        self.pattern = pattern;
        self
    }

    pub fn with_glow(mut self, glow: f32) -> Self {
        self.glow = glow.clamp(0.0, 1.0);
        self
    }

    /// Opts a small part out of the ink outline.
    pub fn without_outline(mut self) -> Self {
        self.outline = false;
        self
    }

    /// Checks the buffers are drawable: equal lengths, whole triangles, indices in range,
    /// finite values, unit normals and outline normals.
    pub fn validate(&self) -> Result<(), String> {
        let n = self.positions.len();
        let name = self.name;
        if self.normals.len() != n || self.uvs.len() != n || self.outline_normals.len() != n {
            return Err(format!("{name}: buffer lengths differ"));
        }
        if !self.indices.len().is_multiple_of(3) {
            return Err(format!("{name}: index count is not a multiple of 3"));
        }
        if let Some(i) = self.indices.iter().find(|&&i| i as usize >= n) {
            return Err(format!("{name}: index {i} out of range ({n} vertices)"));
        }
        let finite = |v: &[f32]| v.iter().all(|x| x.is_finite());
        if !self.positions.iter().all(|p| finite(p)) || !self.uvs.iter().all(|p| finite(p)) {
            return Err(format!("{name}: non-finite position or uv"));
        }
        for (label, set) in [("normal", &self.normals), ("outline normal", &self.outline_normals)] {
            if let Some(v) = set.iter().find(|v| !finite(&v[..]) || ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() - 1.0).abs() > 1e-3) {
                return Err(format!("{name}: {label} {v:?} is not unit length"));
            }
        }
        Ok(())
    }
}

/// A whole item in item space: +Y up along the haft, origin at the butt, 1 unit = 10 cm.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemMesh {
    pub parts: Vec<Part>,
}
