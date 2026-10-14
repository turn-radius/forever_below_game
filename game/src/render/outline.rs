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
}

impl Default for OutlineMaterial {
    /// About 2 px of Armory ink, never thicker than 7 mm (0.07 units), the prototype's
    /// largest outline (`0.032 * 2.2`).
    fn default() -> Self {
        OutlineMaterial { color: ARMORY_INK.into(), width_px: 2.0, max_width: 0.07 }
    }
}

impl Material for OutlineMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
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
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            ATTRIBUTE_OUTLINE_NORMAL.at_shader_location(1),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        descriptor.primitive.cull_mode = Some(Face::Front);
        Ok(())
    }
}
