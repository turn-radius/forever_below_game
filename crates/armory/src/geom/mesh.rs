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
    /// How open each vertex is to the light, 1 for open and lower in a corner or crease, which
    /// the shader darkens by (the level's stone bakes it). Empty for a part with none, which
    /// the shader draws unshaded.
    pub occlusion: Vec<f32>,
    /// A second per-vertex channel for the shader, empty for a part with none. The level's
    /// stone keeps how mossy each vertex is in `x` (`y` is spare); skinned folk use the same
    /// slot for their rest normals, which `part_to_skinned_mesh` writes over it.
    pub uvs_b: Vec<[f32; 2]>,
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
        if !self.occlusion.is_empty() && self.occlusion.len() != n {
            return Err(format!("{name}: occlusion length differs"));
        }
        if !self.uvs_b.is_empty() && self.uvs_b.len() != n {
            return Err(format!("{name}: second uv length differs"));
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

impl Part {
    /// Rotates by `rotation` (orthonormal) and then moves by `offset`, normals and outline
    /// normals with it: sets a finished part (another item's, hung on this one) in place.
    pub fn transform(&mut self, rotation: &crate::geom::Mat3, offset: [f32; 3]) -> &mut Self {
        for p in &mut self.positions {
            let r = rotation.transform(*p);
            *p = [r[0] + offset[0], r[1] + offset[1], r[2] + offset[2]];
        }
        for n in self.normals.iter_mut().chain(self.outline_normals.iter_mut()) {
            *n = rotation.transform(*n);
        }
        self
    }
}

/// A whole item in item space: +Y up along the haft, origin at the butt, 1 unit = 10 cm.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemMesh {
    pub parts: Vec<Part>,
}

impl ItemMesh {
    /// [`Part::transform`] on every part.
    pub fn transform(&mut self, rotation: &crate::geom::Mat3, offset: [f32; 3]) -> &mut Self {
        for part in &mut self.parts {
            part.transform(rotation, offset);
        }
        self
    }

    /// `pieces` copies of an item lying along +X (arrows, bolts) laid in one bundle, for a stack in a small icon:
    /// fanned across `spread` radians about a point near its tail, like a sheaf and the whole bundle turned
    /// `tilt` radians in the XY plane, each copy `thicken` times as wide as built (a shaft
    /// a fifth of a cell across is a hairline at icon size). A pure function of its arguments.
    pub fn fanned(&self, pieces: usize, spread: f32, tilt: f32, thicken: f32) -> ItemMesh {
        let Some((lo, hi)) = self.bounds() else { return ItemMesh::default() };
        let mid = [lo[0] + (hi[0] - lo[0]) * 0.3, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
        let mut out = ItemMesh::default();
        for i in 0..pieces.max(1) {
            let t = if pieces > 1 { i as f32 / (pieces - 1) as f32 - 0.5 } else { 0.0 };
            let (sin, cos) = (tilt + spread * t).sin_cos();
            // Each copy stepped back a little so the fanned copies do not share a plane.
            let back = i as f32 * 0.02;
            let turn = |v: [f32; 3]| [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos, v[2]];
            for part in &self.parts {
                let mut part = part.clone();
                for p in &mut part.positions {
                    let r = turn([p[0] - mid[0], (p[1] - mid[1]) * thicken, (p[2] - mid[2]) * thicken]);
                    *p = [r[0] + mid[0], r[1] + mid[1], r[2] + mid[2] + back];
                }
                // The scale is not uniform, so a normal takes its inverse transpose (the scale's
                // inverse on y and z), then the turn, and is made unit again.
                for n in part.normals.iter_mut().chain(part.outline_normals.iter_mut()) {
                    let r = turn([n[0], n[1] / thicken, n[2] / thicken]);
                    let len = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt().max(1e-6);
                    *n = [r[0] / len, r[1] / len, r[2] / len];
                }
                out.parts.push(part);
            }
        }
        out
    }

    /// Lowest and highest corner over every part, or `None` when empty.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut points = self.parts.iter().flat_map(|p| &p.positions).peekable();
        points.peek()?;
        Some(points.fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
            ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
        }))
    }
}
