//! Ink outlines: an inverted hull drawn with [`OutlineMaterial`] on a second entity that shares
//! the part's mesh. The mesh carries [`ATTRIBUTE_OUTLINE_NORMAL`], normals smoothed across
//! hard edges, which the vertex shader (`outline.wesl`) pushes along.

use bevy::{
    mesh::{MeshVertexAttribute, MeshVertexBufferLayoutRef},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, Face, RenderPipelineDescriptor, SpecializedMeshPipelineError, VertexFormat,
    },
    shader::ShaderRef,
};

pub(super) const SHADER_PATH: &str = "embedded://forever_below/render/outline.wesl";

/// Normals welded by position and averaged, so faceted parts get a closed ink hull.
/// A high random id avoids clashing with other custom attributes (see the
/// [`MeshVertexAttribute`] docs).
pub const ATTRIBUTE_OUTLINE_NORMAL: MeshVertexAttribute =
    MeshVertexAttribute::new("OutlineNormal", 2_315_870_419, VertexFormat::Float32x3);

/// The Armory's ink, `#1a1f22`.
pub const ARMORY_INK: Color = Color::srgb(0x1a as f32 / 255.0, 0x1f as f32 / 255.0, 0x22 as f32 / 255.0);

/// Flat ink drawn on the back faces of a pushed-out copy of the mesh.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Copy, PartialEq)]
pub struct OutlineMaterial {
    #[uniform(0)]
    pub color: LinearRgba,
    /// Line width in pixels on a 1080-pixel-tall viewport; scales with the viewport.
    #[uniform(0)]
    pub width_px: f32,
    /// Largest push in world units, so far-off and small things get thinner lines.
    #[uniform(0)]
    pub max_width: f32,
    /// The hull of a see-through part (glass): drawn after it, so it rings the part's
    /// silhouette without showing through it. See `alpha_mode` below.
    pub see_through: bool,
}

impl Default for OutlineMaterial {
    /// About 2 px of Armory ink, never thicker than 7 mm (0.07 units), the prototype's
    /// largest outline (`0.032 * 2.2`).
    fn default() -> Self {
        OutlineMaterial { color: ARMORY_INK.into(), width_px: 2.0, max_width: 0.07, see_through: false }
    }
}

impl Material for OutlineMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }

    /// An inverted hull fills the whole silhouette behind the part, so behind glass it would
    /// show through as a dark fill. A see-through part's hull is drawn in the transparent pass
    /// instead, sorted just after the glass (which writes depth, `ToonExtension::specialize`),
    /// so only the ring outside the glass passes the depth test.
    fn alpha_mode(&self) -> AlphaMode {
        if self.see_through { AlphaMode::Blend } else { AlphaMode::Opaque }
    }

    /// Sorts a see-through hull after its part: both share a mesh, so they sit at the same
    /// distance, and Bevy draws the transparent pass far to near by view depth plus this bias
    /// (`TransparentSortingInfo3d::sort_distance`).
    fn depth_bias(&self) -> f32 {
        if self.see_through { 0.01 } else { 0.0 }
    }

    // The hull only draws ink in the main pass: no depth prepass, no shadows.
    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let mut attributes = vec![Mesh::ATTRIBUTE_POSITION.at_shader_location(0), ATTRIBUTE_OUTLINE_NORMAL.at_shader_location(1)];
        // A skinned hull (a folk's) moves with the joints, as Bevy's own pipeline lays out the
        // joints (`bevy_pbr::render::mesh::setup_morph_and_skinning_defs`, 6 and 7 without
        // vertex colours or tangents past that, which this layout leaves out).
        if layout.0.contains(Mesh::ATTRIBUTE_JOINT_INDEX) && layout.0.contains(Mesh::ATTRIBUTE_JOINT_WEIGHT) {
            attributes.push(Mesh::ATTRIBUTE_JOINT_INDEX.at_shader_location(6));
            attributes.push(Mesh::ATTRIBUTE_JOINT_WEIGHT.at_shader_location(7));
        }
        let vertex_layout = layout.0.get_layout(&attributes)?;
        descriptor.vertex.buffers = vec![vertex_layout];
        descriptor.primitive.cull_mode = Some(Face::Front);
        Ok(())
    }
}
