//! The house look for Armory items: banded toon light, wear painted in the shader, and ink
//! outlines (`docs/rendering.md`). Works for every family; nothing here knows what a mace is.
//!
//! Add [`ArmoryRenderPlugin`], then spawn items with [`ItemSpawner::spawn`].

mod item;
mod outline;
mod toon;

pub use item::{
    ArmoryItem, ArmoryPart, InkOutline, ItemAppearance, ItemPalette, ItemSpawner, OutlineMaterials,
    half_thickness, item_bounds, part_to_mesh,
};
pub use outline::{ARMORY_INK, ATTRIBUTE_OUTLINE_NORMAL, OutlineMaterial};
pub use toon::{Decay, MinWidth, Pattern, ToonBands, ToonExtension, ToonMaterial, ToonUniform};

use bevy::{prelude::*, shader::load_shader_library};

/// Registers the toon and outline materials and their WESL shaders.
pub struct ArmoryRenderPlugin;

impl Plugin for ArmoryRenderPlugin {
    fn build(&self, app: &mut App) {
        // Embedded and kept loaded so the shaders can import `super::decay`, `super::pattern` and `super::screen`.
        load_shader_library!(app, "decay.wesl");
        load_shader_library!(app, "pattern.wesl");
        load_shader_library!(app, "screen.wesl");
        load_shader_library!(app, "toon.wesl");
        load_shader_library!(app, "outline.wesl");

        app.add_plugins((MaterialPlugin::<ToonMaterial>::default(), MaterialPlugin::<OutlineMaterial>::default()))
            .init_resource::<OutlineMaterials>();
    }
}
