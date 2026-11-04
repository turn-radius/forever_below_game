//! Ethereal shields: a real Armory shield drawn as golden light ([`EtherealMaterial`],
//! `ethereal.wesl`). Unlit and added onto what is behind it, so it is see-through, writes no
//! depth, casts no shadow and is never outlined. Its rim glows at the silhouette, a faint fill
//! lies over the face, and bands of energy run round and in from the edge.

use bevy::{
    mesh::MeshVertexBufferLayoutRef,
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError},
    shader::ShaderRef,
};

pub(super) const SHADER_PATH: &str = "embedded://forever_below/render/ethereal.wesl";

/// The light of one ethereal shield. `intensity` scales all of it (0 for none); `band_phase` is
/// where its bands are, advanced by whoever owns it (faster at a higher `band_speed`, so a flare
/// can race them without a jump); `flare` (0 to 1) brightens the rim and bands for a blow.
/// The mesh's UVs are the shield's own plane scaled to -1..1 ([`ethereal_mesh`]).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Copy, PartialEq)]
pub struct EtherealMaterial {
    #[uniform(0)]
    pub color: LinearRgba,
    #[uniform(0)]
    pub intensity: f32,
    #[uniform(0)]
    pub band_phase: f32,
    #[uniform(0)]
    pub flare: f32,
    /// Radians a second the bands run at rest (not sent to the shader: it moves `band_phase`).
    pub band_speed: f32,
}

impl EtherealMaterial {
    pub fn gold() -> EtherealMaterial {
        EtherealMaterial { color: LinearRgba::rgb(1.0, 0.72, 0.22), intensity: 1.0, band_phase: 0.0, flare: 0.0, band_speed: 1.0 }
    }
}

impl Material for EtherealMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    // Both faces: the back of a shield shows through its front.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// An Armory part as an ethereal mesh: [`super::part_to_mesh`] with its UVs replaced by the
/// vertex's place in the shield's plane, `centre` and `half` being the shield's bounds in that
/// plane (`axes` picks which two of x, y, z are its width and height), so the shader knows how
/// near the edge it is.
pub fn ethereal_mesh(part: &armory::Part, axes: [usize; 2], centre: [f32; 2], half: [f32; 2]) -> Mesh {
    let mut mesh = super::part_to_mesh(part);
    let uvs: Vec<[f32; 2]> = part.positions.iter().map(|p| [(p[axes[0]] - centre[0]) / half[0], (p[axes[1]] - centre[1]) / half[1]]).collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh
}
