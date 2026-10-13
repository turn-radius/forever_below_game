//! Turning Armory mesh data into Bevy entities: one mesh per [`armory::Part`], drawn with a
//! [`ToonMaterial`] and, unless the part opts out, an ink hull child.

use armory::{ItemMesh, MaterialKind, Part, Rgb, cond::Condition, palette::Palette};
use bevy::{
    asset::RenderAssetUsages,
    ecs::system::SystemParam,
    mesh::{Indices, PrimitiveTopology},
    platform::collections::HashMap,
    prelude::*,
};

use super::outline::{ATTRIBUTE_OUTLINE_NORMAL, OutlineMaterial};
use super::toon::{Decay, ToonBands, ToonExtension, ToonMaterial};

/// Base colour for every material kind: the armory palette for paletted metals, wood and
/// so on, plus the kinds it leaves to other sources (untinted gems, cloth, paint, food).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemPalette {
    pub materials: Palette,
    pub gem: Rgb,
    pub cloth: Rgb,
    pub paint: Rgb,
    pub food: Rgb,
}

impl ItemPalette {
    /// The Armory's render colours (the prototype's third `PALETTES` entry: iron `#a3aab0`,
    /// brass `#c29b48`, wood `#8a6a45`, leather `#2f3a33`). Gem, cloth, paint and food are our
    /// own picks: a sapphire blue, undyed wool, oxblood paint and a bread-crust brown.
    pub const ARMORY: ItemPalette = ItemPalette {
        materials: Palette::PRESETS[2],
        gem: Rgb::hex(0x3f6fb0),
        cloth: Rgb::hex(0x8c7b62),
        paint: Rgb::hex(0x7a2e26),
        food: Rgb::hex(0xb0814a),
    };

    pub fn get(&self, kind: MaterialKind) -> Rgb {
        self.materials.get(kind).unwrap_or(match kind {
            MaterialKind::Cloth => self.cloth,
            MaterialKind::Paint => self.paint,
            MaterialKind::Food => self.food,
            _ => self.gem,
        })
    }
}

impl Default for ItemPalette {
    fn default() -> Self {
        ItemPalette::ARMORY
    }
}

/// Everything about an item's look that is not geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemAppearance {
    pub palette: ItemPalette,
    pub condition: Condition,
    /// The item's seed, so its wear pattern is its own.
    pub seed: u32,
    /// Multiplies `Condition::scale` for the wear pattern: the prototype uses 8 for
    /// jewellery and 3 for food, 1 otherwise.
    pub decay_scale_factor: f32,
    pub bands: ToonBands,
    pub outline: OutlineMaterial,
}

impl Default for ItemAppearance {
    fn default() -> Self {
        ItemAppearance {
            palette: ItemPalette::default(),
            condition: Condition::default(),
            seed: 0,
            decay_scale_factor: 1.0,
            bands: ToonBands::default(),
            outline: OutlineMaterial::default(),
        }
    }
}

impl ItemAppearance {
    /// The toon material for one part. `Part::tint` overrides the palette colour.
    pub fn material_for(&self, part: &Part) -> ToonMaterial {
        let colour = part.tint.or(part.material.map(|kind| self.palette.get(kind))).unwrap_or(Rgb(0.5, 0.5, 0.5));
        let decay = Decay {
            kind: part.material.map_or(0, MaterialKind::decay_type),
            amount: part.material.map_or(0.0, |kind| self.condition.decay(kind)),
            scale: self.condition.scale * self.decay_scale_factor,
            seed: (self.seed % 997) as f32 * 0.137,
        };
        // Cut stones always shine a little (`gemMat`); `glow` adds to it.
        let glow = match part.material {
            Some(MaterialKind::Gem) => 0.25 + part.glow * 0.45,
            _ => part.glow * 0.45,
        };
        ToonMaterial {
            base: StandardMaterial {
                base_color: Color::srgb(colour.0, colour.1, colour.2),
                perceptual_roughness: 1.0,
                reflectance: 0.0,
                ..default()
            },
            extension: ToonExtension { bands: self.bands, decay, glow },
        }
    }
}

/// Converts one part to a Bevy mesh: positions, normals, uvs (when present), outline normals
/// (falling back to the normals) and a `u32` triangle list.
pub fn part_to_mesh(part: &Part) -> Mesh {
    let vertex_count = part.positions.len();
    let outline_normals = if part.outline_normals.len() == vertex_count { &part.outline_normals } else { &part.normals };
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, part.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, part.normals.clone())
        .with_inserted_attribute(ATTRIBUTE_OUTLINE_NORMAL, outline_normals.clone())
        .with_inserted_indices(Indices::U32(part.indices.clone()));
    if part.uvs.len() == vertex_count {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, part.uvs.clone());
    }
    mesh
}

/// Marks the root entity of a spawned Armory item.
#[derive(Component, Debug, Default)]
pub struct ArmoryItem;

/// Marks one part of an Armory item.
#[derive(Component, Debug)]
pub struct ArmoryPart;

/// Marks an ink hull.
#[derive(Component, Debug)]
pub struct InkOutline;

/// Outline materials shared between items with the same outline settings, so hulls batch.
#[derive(Resource, Default)]
pub struct OutlineMaterials(HashMap<[u32; 6], Handle<OutlineMaterial>>);

impl OutlineMaterials {
    fn get_or_add(&mut self, assets: &mut Assets<OutlineMaterial>, material: OutlineMaterial) -> Handle<OutlineMaterial> {
        let [r, g, b, a] = material.color.to_f32_array();
        let key = [r, g, b, a, material.width_px, material.max_width].map(f32::to_bits);
        self.0.entry(key).or_insert_with(|| assets.add(material)).clone()
    }
}

/// Spawns Armory items from their mesh data.
#[derive(SystemParam)]
pub struct ItemSpawner<'w, 's> {
    commands: Commands<'w, 's>,
    meshes: ResMut<'w, Assets<Mesh>>,
    toon_materials: ResMut<'w, Assets<ToonMaterial>>,
    outline_materials: ResMut<'w, Assets<OutlineMaterial>>,
    shared_outlines: ResMut<'w, OutlineMaterials>,
}

impl ItemSpawner<'_, '_> {
    /// Spawns `item` as an [`ArmoryItem`] root at the origin with one [`ArmoryPart`] child
    /// per part, each with an [`InkOutline`] child unless `Part::outline` is false. Position
    /// the item by setting the root's `Transform`; parts keep an identity transform so their
    /// local space stays item space, which the wear shader relies on.
    ///
    /// Returns the root's commands. Insert on it rather than through another `Commands` in the
    /// same system: each `Commands` has its own queue, and the other one may apply first.
    pub fn spawn(&mut self, item: &ItemMesh, appearance: &ItemAppearance) -> EntityCommands<'_> {
        let outline = self.shared_outlines.get_or_add(&mut self.outline_materials, appearance.outline);
        let root = self.commands.spawn((ArmoryItem, Transform::default(), Visibility::default())).id();
        for part in &item.parts {
            let mesh = self.meshes.add(part_to_mesh(part));
            let material = self.toon_materials.add(appearance.material_for(part));
            let part_entity = self
                .commands
                .spawn((ArmoryPart, Name::new(part.name), Mesh3d(mesh.clone()), MeshMaterial3d(material), ChildOf(root)))
                .id();
            if part.outline {
                self.commands.spawn((InkOutline, Mesh3d(mesh), MeshMaterial3d(outline.clone()), ChildOf(part_entity)));
            }
        }
        self.commands.entity(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_covers_every_kind() {
        let palette = ItemPalette::ARMORY;
        assert_eq!(palette.get(MaterialKind::Iron), Rgb::hex(0xa3aab0));
        assert_eq!(palette.get(MaterialKind::Cloth), palette.cloth);
    }

    #[test]
    fn tint_overrides_the_palette() {
        let part = Part { material: Some(MaterialKind::Gem), tint: Some(Rgb(1.0, 0.0, 0.0)), ..default() };
        let material = ItemAppearance::default().material_for(&part);
        assert_eq!(material.base.base_color, Color::srgb(1.0, 0.0, 0.0));
        assert_eq!(material.extension.decay.kind, 5);
    }

    #[test]
    fn part_mesh_carries_outline_normals() {
        let part = Part {
            positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            indices: vec![0, 1, 2],
            ..default()
        };
        let mesh = part_to_mesh(&part);
        assert!(mesh.attribute(ATTRIBUTE_OUTLINE_NORMAL).is_some());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_none());
        assert_eq!(mesh.indices().map(Indices::len), Some(3));
    }
}
