//! Direct rather than GPU-built indirect draws for every view (#163).
//!
//! Bevy 0.20 picks how far its GPU mesh preprocessing goes from the device
//! ([`GpuPreprocessingSupport`], `bevy_render-0.20.0/src/batching/gpu_preprocessing.rs`). With
//! [`GpuPreprocessingMode::Culling`], the default on this M5, the bins of a multidraw are kept on
//! the GPU from frame to frame, unpacked and given their instance slots by compute shaders, and
//! drawn indirectly (`unpack_bins`, `allocate_uniforms` and `mesh_preprocess` in
//! `bevy_pbr-0.20.0/src/render/`). [`super::ToonExtension`] is bindless, so a few hundred item
//! parts, each its own bin, share one multidraw. On that path such draws now and then took
//! another part's instance data, for whole runs of frames while the view turned: parts drawn
//! with another part's mesh or material, and point light shadow maps filled with parts in the
//! wrong places, so shadows fell where nothing stood or the whole floor went dark.
//!
//! [`GpuPreprocessingMode::PreprocessingOnly`] keeps the compute pass that builds mesh uniforms
//! but bins and batches on the CPU and draws directly, as Bevy already does on devices without
//! indirect first-instance support. Point light shadow views follow the same setting
//! (`create_point_shadow_maps` in `bevy_pbr-0.20.0/src/render/light.rs` gives them
//! `NoIndirectDrawing` unless the mode is `Culling`), so the camera-level
//! [`bevy::render::view::NoIndirectDrawing`] alone would leave the shadows on the faulty path.
//! Capping the mode is our own workaround; the numbers are in `docs/rendering.md`, "In the Bevy
//! build".

use bevy::{
    prelude::*,
    render::{
        RenderApp, RenderStartup,
        batching::gpu_preprocessing::{GpuPreprocessingMode, GpuPreprocessingSupport},
        init_gpu_resource,
    },
};

/// Draws every view directly ([`draw_directly`]) once Bevy has read the device's support.
pub(super) fn add(app: &mut App) {
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app.add_systems(RenderStartup, draw_directly.after(init_gpu_resource::<GpuPreprocessingSupport>));
    }
}

/// Caps GPU preprocessing at [`GpuPreprocessingMode::PreprocessingOnly`]; a device with less
/// keeps what it has.
fn draw_directly(mut support: ResMut<GpuPreprocessingSupport>) {
    support.max_supported_mode = capped(support.max_supported_mode);
}

fn capped(mode: GpuPreprocessingMode) -> GpuPreprocessingMode {
    match mode {
        GpuPreprocessingMode::Culling => GpuPreprocessingMode::PreprocessingOnly,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn culling_is_capped_and_lesser_modes_kept() {
        assert!(capped(GpuPreprocessingMode::Culling) == GpuPreprocessingMode::PreprocessingOnly);
        assert!(capped(GpuPreprocessingMode::PreprocessingOnly) == GpuPreprocessingMode::PreprocessingOnly);
        assert!(capped(GpuPreprocessingMode::None) == GpuPreprocessingMode::None);
    }
}
