//! The house look for Armory items: banded toon light, wear painted in the shader, and ink
//! outlines (`docs/rendering.md`). Works for every family; nothing here knows what a mace is.
//!
//! Add [`ArmoryRenderPlugin`], then spawn items with [`ItemSpawner::spawn`].

pub mod flame;
mod indirect;
mod item;
mod outline;
mod toon;

pub use item::{
    ArmoryItem, ArmoryPart, InkOutline, ItemAppearance, ItemPalette, ItemSpawner, OutlineMaterials,
    half_thickness, item_bounds, part_to_mesh,
};
pub use outline::{ARMORY_INK, ATTRIBUTE_OUTLINE_NORMAL, OutlineMaterial};
pub use toon::{Decay, Etch, EtchStyle, MinWidth, Pattern, Shimmer, ToonBands, ToonExtension, ToonMaterial, ToonUniform};

use bevy::{light::cluster::GlobalClusterSettings, prelude::*, shader::load_shader_library};

/// Registers the toon and outline materials and their WESL shaders, and has every view draw
/// directly rather than through Bevy's GPU-built indirect draws (`indirect`, #163), with lights
/// clustered on the CPU ([`cluster_lights_on_the_cpu`], #171).
pub struct ArmoryRenderPlugin;

impl Plugin for ArmoryRenderPlugin {
    fn build(&self, app: &mut App) {
        // Embedded and kept loaded so the shaders can import `super::decay`, `super::pattern`,
        // `super::etch`, `super::screen` and `super::shimmer`.
        load_shader_library!(app, "decay.wesl");
        load_shader_library!(app, "pattern.wesl");
        load_shader_library!(app, "etch.wesl");
        load_shader_library!(app, "screen.wesl");
        load_shader_library!(app, "shimmer.wesl");
        load_shader_library!(app, "toon.wesl");
        load_shader_library!(app, "outline.wesl");

        app.add_plugins((MaterialPlugin::<ToonMaterial>::default(), MaterialPlugin::<OutlineMaterial>::default()))
            .init_resource::<OutlineMaterials>()
            .add_systems(Startup, cluster_lights_on_the_cpu)
            .add_systems(Update, flame::flicker_flames);
        indirect::add(app);
    }
}

/// Assigns lights to clusters on the CPU, as Bevy did before 0.20, rather than with 0.20's GPU
/// clustering, through `GlobalClusterSettings::gpu_clustering`, Bevy's documented switch ("To
/// disable GPU light clustering, set this to `None`", `bevy_light-0.20.0/src/cluster/mod.rs`).
///
/// GPU clustering grows each view's light index list to the next power of two of a count read
/// back from the GPU, unchecked (`ViewClusteringReadbackData::update_from_metadata`,
/// `bevy_pbr-0.20.0/src/cluster/gpu.rs:403-406` and `419-421`), inside the readback's buffer map
/// callback while it holds the view's mutex (`gpu.rs:1157`). A garbage count past 2^31 overflows
/// `u32::next_power_of_two`, the mutex is poisoned, and the render thread panics on its next
/// lock: the crash after a while of play (#171). Capping GPU preprocessing (`indirect`) stopped
/// the garbage counts; this takes the unchecked readback out altogether.
pub fn cluster_lights_on_the_cpu(settings: Option<ResMut<GlobalClusterSettings>>) {
    if let Some(mut settings) = settings {
        settings.gpu_clustering = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::light::cluster::GlobalClusterGpuSettings;

    #[test]
    fn lights_are_clustered_on_the_cpu() {
        // GPU clustering's unchecked readback crashed the render thread after a while (#171).
        let mut app = App::new();
        app.insert_resource(GlobalClusterSettings {
            supports_storage_buffers: true,
            clustered_decals_are_usable: false,
            gpu_clustering: Some(GlobalClusterGpuSettings { initial_z_slice_list_capacity: 1024, initial_index_list_capacity: 65536 }),
            max_uniform_buffer_clusterable_objects: 204,
            view_cluster_bindings_max_indices: 16384,
        })
        .add_systems(Startup, cluster_lights_on_the_cpu);
        app.update();
        assert!(app.world().resource::<GlobalClusterSettings>().gpu_clustering.is_none());
    }
}
