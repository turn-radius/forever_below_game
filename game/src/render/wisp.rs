//! Wisps: soft light drawn on a flat card turned to the eye ([`WispMaterial`], `wisp.wesl`), for
//! glows that are light rather than a thing, such as Divine Light's bauble
//! (`crate::casting`) and the particles of data-driven effects (`crate::effects`). A heart is a
//! bright core in a softer body whose edge wavers and ripples, so it shimmers like heat over a
//! flame; a mote is a small soft dot; a spark is a streak, brightest at its head; a beam is a
//! tall shaft brightest at its foot; a ring is a thin ring (laid flat on the floor by whoever
//! places it). All are unlit and added
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
/// moment so two do not shimmer together; `kind` is the card's picture ([`WispKind`]).
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
    pub kind: f32,
    /// 1 for a card whose colour and strength come from its mesh's tag ([`WispMaterial::tagged`]).
    #[uniform(0)]
    pub tagged: f32,
}

/// The pictures a wisp card can show; the number is what `wisp.wesl` switches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WispKind {
    Heart = 0,
    Mote = 1,
    Spark = 2,
    Beam = 3,
    Ring = 4,
}

impl WispMaterial {
    /// A card of `kind` whose colour and strength each mesh gives in its [`bevy::mesh::MeshTag`]:
    /// sRGB red, green, blue and alpha, a byte each, red the lowest. One of these serves every
    /// card of its kind, so cards change colour without a material of their own to rebuild.
    pub fn tagged(kind: WispKind) -> WispMaterial {
        WispMaterial { color: Color::WHITE.into(), core: Color::WHITE.into(), strength: 1.0, wobble: if kind == WispKind::Heart { 1.0 } else { 0.0 }, phase: 0.0, kind: kind as u32 as f32, tagged: 1.0 }
    }

    /// The tag a tagged card of `rgb` (sRGB) at `alpha` carries.
    pub fn tag([r, g, b]: [f32; 3], alpha: f32) -> u32 {
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
        byte(r) | byte(g) << 8 | byte(b) << 16 | byte(alpha) << 24
    }

    /// A heart of `color` round a `core`.
    pub fn heart(color: Color, core: Color, phase: f32) -> WispMaterial {
        WispMaterial { color: color.into(), core: core.into(), strength: 1.0, wobble: 1.0, phase, kind: WispKind::Heart as u32 as f32, tagged: 0.0 }
    }

    /// A mote of `color`.
    pub fn mote(color: Color) -> WispMaterial {
        WispMaterial { color: color.into(), core: Color::WHITE.into(), strength: 1.0, wobble: 0.0, phase: 0.0, kind: WispKind::Mote as u32 as f32, tagged: 0.0 }
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
