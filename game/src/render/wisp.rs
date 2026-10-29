//! Wisps: soft light drawn on a flat card turned to the eye ([`WispMaterial`], `wisp.wesl`), for
//! glows that are light rather than a thing, such as Divine Light's bauble
//! (`crate::casting`). A heart is a bright core in a softer body whose edge wavers and ripples,
//! so it shimmers like heat over a flame; a mote is a small soft dot. Both are unlit and added
//! onto what is behind them, so they glow in the dark, light nothing and cast no shadow, and
//! being transparent they stay out of the prepasses the screen ink reads, so are never outlined.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

pub(super) const SHADER_PATH: &str = "embedded://forever_below/render/wisp.wesl";

/// A wisp's light: `color` at its rim and `core` at its middle, at `strength` (0 for none).
/// `wobble` is how far a heart's edge wavers (0 holds it round); `phase` sets each wisp's own
/// moment so two do not shimmer together; `mote` is 1 for a small soft dot, 0 for a heart.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Copy, PartialEq)]
pub struct WispMaterial {
    #[uniform(0)]
    pub color: LinearRgba,
    #[uniform(0)]
    pub core: LinearRgba,
    #[uniform(0)]
    pub strength: f32,
    #[uniform(0)]
    pub wobble: f32,
    #[uniform(0)]
    pub phase: f32,
    #[uniform(0)]
    pub mote: f32,
}

impl WispMaterial {
    /// A heart of `color` round a `core`.
    pub fn heart(color: Color, core: Color, phase: f32) -> WispMaterial {
        WispMaterial { color: color.into(), core: core.into(), strength: 1.0, wobble: 1.0, phase, mote: 0.0 }
    }

    /// A mote of `color`.
    pub fn mote(color: Color) -> WispMaterial {
        WispMaterial { color: color.into(), core: Color::WHITE.into(), strength: 1.0, wobble: 0.0, phase: 0.0, mote: 1.0 }
    }
}

impl Material for WispMaterial {
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
}

/// The card every wisp is drawn on: a unit square in the XY plane facing +Z, its UVs 0 to 1
/// corner to corner.
pub fn wisp_card() -> Mesh {
    let positions = vec![[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [0.5, 0.5, 0.0], [-0.5, 0.5, 0.0]];
    let uvs = vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}
