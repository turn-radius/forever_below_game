//! Turning Armory mesh data into Bevy entities: one mesh per [`armory::Part`], drawn with a
//! [`ToonMaterial`] and, unless the part opts out, an ink hull child.

use armory::{ItemMesh, MaterialKind, Part, Rgb, cond::Condition, etch::Etching, palette::Palette};
use bevy::{
    asset::RenderAssetUsages,
    ecs::system::SystemParam,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    platform::collections::HashMap,
    prelude::*,
    shape::Aabb3d,
};

use super::outline::{ATTRIBUTE_OUTLINE_NORMAL, OutlineMaterial};
use super::toon::{Decay, Etch, MinWidth, Pattern, Shimmer, ToonBands, ToonExtension, ToonMaterial};

/// Base colour for every material kind: the armory palette for paletted metals, wood and
/// so on, plus the kinds it leaves to other sources (untinted gems, cloth, paint, food).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemPalette {
    pub materials: Palette,
    pub gem: Rgb,
    pub glass: Rgb,
    pub cloth: Rgb,
    pub paint: Rgb,
    pub food: Rgb,
}

impl ItemPalette {
    /// The Armory's render colours (the prototype's third `PALETTES` entry: iron `#a3aab0`,
    /// brass `#c29b48`, wood `#8a6a45`, leather `#2f3a33`) and its glass (`glassMat('#dfe9ee')`).
    /// Gem, cloth, paint and food are our own picks: a sapphire blue, undyed wool, oxblood
    /// paint and a bread-crust brown.
    pub const ARMORY: ItemPalette = ItemPalette {
        materials: Palette::PRESETS[2],
        gem: Rgb::hex(0x3f6fb0),
        glass: Rgb::hex(0xdfe9ee),
        cloth: Rgb::hex(0x8c7b62),
        paint: Rgb::hex(0x7a2e26),
        food: Rgb::hex(0xb0814a),
    };

    pub fn get(&self, kind: MaterialKind) -> Rgb {
        self.materials.get(kind).unwrap_or(match kind {
            MaterialKind::Cloth => self.cloth,
            MaterialKind::Paint => self.paint,
            MaterialKind::Food => self.food,
            MaterialKind::Glass => self.glass,
            // Skin always carries its folk's colour as a tint; this is a fallback only (the
            // second human skin in `data/races.json`).
            MaterialKind::Skin => Rgb::hex(0xe2b48f),
            // Hair likewise always carries its colour (the third in `data/hair.json`).
            MaterialKind::Hair => Rgb::hex(0x5a3a22),
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
    /// Multiplies `Condition::scale` for the wear pattern, and the etching's scale: the
    /// prototype uses 8 for jewellery and 3 for food (2 for food's etching, which no food has),
    /// 1 otherwise.
    pub decay_scale_factor: f32,
    pub bands: ToonBands,
    pub outline: OutlineMaterial,
    /// Narrowest a part without an ink line may look, in pixels at 1080p (`MinWidth::px`).
    pub min_width_px: f32,
    /// The item's etching (`Item::etching`), drawn on the parts its target covers.
    pub etching: Option<Etching>,
    /// A shimmer over every part (a legendary lying on the floor); none by default.
    pub shimmer: Shimmer,
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
            min_width_px: 1.5,
            etching: None,
            shimmer: Shimmer::default(),
        }
    }
}

impl ItemAppearance {
    /// The toon material for one part. `Part::tint` overrides the palette colour, and
    /// `Part::pattern` is drawn over it.
    pub fn material_for(&self, part: &Part) -> ToonMaterial {
        // A flame takes its colour from its vertices (`part_to_mesh`), so its material is white.
        let colour = if is_flame(part) { Rgb(1.0, 1.0, 1.0) } else { part.tint.or(part.material.map(|kind| self.palette.get(kind))).unwrap_or(Rgb(0.5, 0.5, 0.5)) };
        let decay = Decay {
            kind: part.material.map_or(0, MaterialKind::decay_type),
            amount: part.material.map_or(0.0, |kind| self.condition.decay(kind)) * light_fastness(part),
            scale: self.condition.scale * self.decay_scale_factor,
            seed: (self.seed % 997) as f32 * 0.137,
        };
        // Cut stones always shine a little (`gemMat`); `glow` adds to it.
        let glow = match part.material {
            Some(MaterialKind::Gem) => 0.25 + part.glow * 0.45,
            _ => part.glow * 0.45,
        };
        let see_through = is_see_through(part);
        ToonMaterial {
            base: StandardMaterial {
                base_color: Color::srgba(colour.0, colour.1, colour.2, if see_through { GLASS_OPACITY } else { 1.0 }),
                alpha_mode: if see_through {
                    AlphaMode::Blend
                } else if is_flame(part) {
                    // Mostly see-through (vertex colour alpha, `part_to_mesh`), so the burning
                    // head shows through. Premultiplied rather than Blend, which the toon
                    // shader premultiplies itself: Blend writes depth for glass
                    // (`ToonExtension::specialize`), which would hide the flame's core behind
                    // its own veil and the head behind its flame.
                    AlphaMode::Premultiplied
                } else {
                    AlphaMode::Opaque
                },
                perceptual_roughness: 1.0,
                reflectance: 0.0,
                ..default()
            },
            // Outlined parts already read through their ink line.
            extension: ToonExtension {
                bands: self.bands,
                decay,
                glow,
                min_width: if part.outline {
                    MinWidth::default()
                } else {
                    MinWidth { px: self.min_width_px, half_thickness: half_thickness(part) }
                },
                pattern: part.pattern.map_or(Pattern::default(), |pattern| Pattern::of(&pattern, &part.positions)),
                etch: match (self.etching, part.material) {
                    (Some(etching), Some(material)) => Etch::on(&etching, material, self.decay_scale_factor),
                    _ => Etch::default(),
                },
                shimmer: self.shimmer,
            },
        }
    }
}

/// How much of the light behind it glass stops, at its thinnest: the prototype's `glassMat`
/// opacity (0.32; its bottles use 0.3). The toon shader thickens it towards the silhouette.
pub const GLASS_OPACITY: f32 = 0.32;

/// Whether a part is drawn see-through (glass), blended over what lies behind it, with no
/// shadow of its own and its ink hull drawn after it.
pub fn is_see_through(part: &Part) -> bool {
    part.material == Some(MaterialKind::Glass)
}

/// How much of its wear a part shows, from its glow: 1 for an unlit part, falling to 0 for one
/// that glows at full. A part that gives off light is drawn by its own light, and a flame or a
/// coal does not cloud like a cut stone, so a worn brazier keeps a bright flame. Graded rather
/// than all or nothing so a faintly glowing stone still clouds with age (our own rule; the
/// prototype's `gemMat` clouds every gem alike).
pub fn light_fastness(part: &Part) -> f32 {
    1.0 - part.glow.clamp(0.0, 1.0)
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
    if is_flame(part) {
        // Colour and opacity by height along the flame (`flame::flame_colour`, `flame_alpha`).
        let layer = if part.name == "flame_core" { super::flame::FlameLayer::Core } else { super::flame::FlameLayer::Veil };
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, super::flame::flame_vertex_colours(&part.positions, layer));
    }
    mesh
}

/// Whether a part is a flame (its veil or the core inside it): drawn fading from see-through
/// at its base to solid at its tip.
pub fn is_flame(part: &Part) -> bool {
    matches!(part.name, "flame" | "flame_core")
}

/// A part's half thickness, estimated as volume over surface area (`2V / A`) so it needs no
/// knowledge of the part's shape: exactly the radius of a tube, and the full thickness of a
/// thin plate, which errs on the thick side. Zero for a part with no area.
pub fn half_thickness(part: &Part) -> f32 {
    let points: Vec<Vec3> = part.positions.iter().map(|&p| Vec3::from_array(p)).collect();
    // Measured about the part's own centre, which keeps the sum accurate far from the origin.
    let centre = points.iter().sum::<Vec3>() / points.len().max(1) as f32;
    let (mut volume, mut area) = (0.0, 0.0);
    for triangle in part.indices.as_chunks::<3>().0 {
        let [a, b, c] = triangle.map(|i| points[i as usize] - centre);
        volume += a.dot(b.cross(c)) / 6.0;
        area += (b - a).cross(c - a).length() * 0.5;
    }
    if area > 0.0 { (2.0 * volume / area).max(0.0) } else { 0.0 }
}

/// Item-space bounds of every part, or `None` for an item with no vertices.
pub fn item_bounds(item: &ItemMesh) -> Option<Aabb3d> {
    let mut points = item.parts.iter().flat_map(|part| &part.positions).map(|&p| Vec3::from_array(p)).peekable();
    points.peek()?;
    Some(Aabb3d::from_point_cloud(Isometry3d::IDENTITY, points))
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
pub struct OutlineMaterials(HashMap<[u32; 7], Handle<OutlineMaterial>>);

impl OutlineMaterials {
    fn get_or_add(&mut self, assets: &mut Assets<OutlineMaterial>, material: OutlineMaterial) -> Handle<OutlineMaterial> {
        let [r, g, b, a] = material.color.to_f32_array();
        let [r, g, b, a, width, max] = [r, g, b, a, material.width_px, material.max_width].map(f32::to_bits);
        let key = [r, g, b, a, width, max, material.see_through as u32];
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
        let glass_outline =
            self.shared_outlines.get_or_add(&mut self.outline_materials, OutlineMaterial { see_through: true, ..appearance.outline });
        let root = self.commands.spawn((ArmoryItem, Transform::default(), Visibility::default())).id();
        let mut flame = None;
        for part in &item.parts {
            let mesh = self.meshes.add(part_to_mesh(part));
            let material = self.toon_materials.add(appearance.material_for(part));
            let see_through = is_see_through(part);
            let mut part_entity =
                self.commands.spawn((ArmoryPart, Name::new(part.name), Mesh3d(mesh.clone()), MeshMaterial3d(material), ChildOf(root)));
            if see_through {
                // Glass lets light through: its shadow would darken the drink inside it.
                part_entity.insert(NotShadowCaster);
            }
            // A flame and the core inside it move as one, about the flame's base.
            if part.name == "flame" {
                flame = super::flame::Flame::of(&part.positions);
            }
            if matches!(part.name, "flame" | "flame_core")
                && let Some(flame) = flame
            {
                part_entity.insert(flame);
            }
            let part_entity = part_entity.id();
            if part.outline {
                let ink = if see_through { glass_outline.clone() } else { outline.clone() };
                self.commands.spawn((InkOutline, Mesh3d(mesh), MeshMaterial3d(ink), ChildOf(part_entity)));
            }
        }
        self.commands.entity(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::toon::EtchStyle;
    use armory::fabric::{PatternKind, SurfacePattern};

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
    fn a_flame_is_drawn_blended_from_its_vertices() {
        let flame = |name| Part {
            name,
            material: Some(MaterialKind::Gem),
            glow: 1.0,
            positions: vec![[0.0, 1.0, 0.0], [0.5, 1.0, 0.0], [0.0, 3.0, 0.0]],
            normals: vec![[0.0, 1.0, 0.0]; 3],
            indices: vec![0, 1, 2],
            ..default()
        };
        let veil = flame("flame");
        let material = ItemAppearance::default().material_for(&veil);
        assert!(matches!(material.base.alpha_mode, AlphaMode::Premultiplied));
        assert_eq!(material.base.base_color, Color::srgb(1.0, 1.0, 1.0));
        let mesh = part_to_mesh(&veil);
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) else { panic!("no vertex colours") };
        assert!((colors[0][3] - 0.15).abs() < 1e-5 && colors[2][3] > colors[0][3], "{colors:?}");
        // Yellow at the base, redder at the tip.
        assert!(colors[0][1] > colors[2][1]);
        // The core is paler and more opaque than the veil at the same height.
        let core = part_to_mesh(&flame("flame_core"));
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(core)) = core.attribute(Mesh::ATTRIBUTE_COLOR) else { panic!() };
        assert!(core[0][2] > colors[0][2] && core[0][3] > colors[0][3]);
        // Other glowing parts stay opaque and uncoloured.
        let ember = flame("embers");
        assert!(matches!(ItemAppearance::default().material_for(&ember).base.alpha_mode, AlphaMode::Opaque));
        assert!(part_to_mesh(&ember).attribute(Mesh::ATTRIBUTE_COLOR).is_none());
    }

    #[test]
    fn flames_do_not_wear() {
        let grave = ItemAppearance { condition: Condition::of(armory::cond::ConditionState::GraveFind), ..default() };
        let flame = Part { material: Some(MaterialKind::Gem), glow: 1.0, ..default() };
        let gem = Part { material: Some(MaterialKind::Gem), glow: 0.5, ..default() };
        assert_eq!(grave.material_for(&flame).extension.decay.amount, 0.0);
        let full = grave.condition.decay(MaterialKind::Gem);
        assert!((grave.material_for(&gem).extension.decay.amount - full * 0.5).abs() < 1e-6);
    }

    #[test]
    fn bounds_cover_every_part() {
        let part = |positions: Vec<[f32; 3]>| Part { positions, ..default() };
        assert!(item_bounds(&ItemMesh::default()).is_none());
        let item = ItemMesh { parts: vec![part(vec![[0.0, 0.0, 0.0], [1.0, 2.0, 0.5]]), part(vec![[-1.0, 8.0, 0.0]])] };
        let bounds = item_bounds(&item).unwrap();
        assert_eq!(Vec3::from(bounds.min), Vec3::new(-1.0, 0.0, 0.0));
        assert_eq!(Vec3::from(bounds.max), Vec3::new(1.0, 8.0, 0.5));
    }

    /// A closed cylinder along Y: `sides` around, capped at both ends.
    fn cylinder(radius: f32, length: f32, sides: u32) -> Part {
        let mut positions = vec![[0.0, 0.0, 0.0], [0.0, length, 0.0]];
        for i in 0..sides {
            let (sin, cos) = (std::f32::consts::TAU * i as f32 / sides as f32).sin_cos();
            positions.push([cos * radius, 0.0, sin * radius]);
            positions.push([cos * radius, length, sin * radius]);
        }
        let mut indices = Vec::new();
        for i in 0..sides {
            let (a, b) = (2 + 2 * i, 2 + 2 * ((i + 1) % sides));
            indices.extend([a, a + 1, b, b, a + 1, b + 1, 0, a, b, 1, b + 1, a + 1]);
        }
        Part { positions, indices, ..default() }
    }

    #[test]
    fn half_thickness_of_a_string_is_its_radius() {
        let string = cylinder(0.01, 12.0, 24);
        let estimate = half_thickness(&string);
        assert!((estimate - 0.01).abs() < 0.0005, "{estimate}");
        assert_eq!(half_thickness(&Part::default()), 0.0);
    }

    #[test]
    fn only_parts_without_ink_get_a_minimum_width() {
        let appearance = ItemAppearance::default();
        let string = Part { outline: false, ..cylinder(0.01, 12.0, 24) };
        let limb = Part { outline: true, ..cylinder(0.1, 12.0, 24) };
        let fine = appearance.material_for(&string).extension.min_width;
        assert_eq!(fine.px, appearance.min_width_px);
        assert!((fine.half_thickness - 0.01).abs() < 0.0005);
        assert_eq!(appearance.material_for(&limb).extension.min_width, MinWidth::default());
    }

    fn patterned(kind: PatternKind, scale: f32) -> Part {
        let pattern = SurfacePattern { kind, color: Rgb::hex(0xe0d4b0), scale };
        Part { material: Some(MaterialKind::Cloth), pattern: Some(pattern), ..cylinder(1.0, 4.0, 48) }
    }

    #[test]
    fn every_pattern_kind_draws_its_own_pattern() {
        let appearance = ItemAppearance::default();
        let ids: Vec<u32> = PatternKind::ALL.iter().map(|&kind| appearance.material_for(&patterned(kind, 2.0)).extension.pattern.id).collect();
        for (kind, id) in PatternKind::ALL.iter().zip(&ids) {
            assert_eq!(*id, kind.shader_id());
            assert_eq!(ids.iter().filter(|other| *other == id).count(), 1, "{kind:?}");
        }
    }

    #[test]
    fn pattern_scale_follows_the_surface_pattern() {
        let appearance = ItemAppearance::default();
        for scale in [1.0, 2.5, 9.0] {
            let pattern = appearance.material_for(&patterned(PatternKind::Mail, scale)).extension.pattern;
            assert_eq!(pattern.scale, scale);
            assert_eq!(pattern.color, Vec3::new(0xe0 as f32 / 255.0, 0xd4 as f32 / 255.0, 0xb0 as f32 / 255.0));
            // The cylinder's girth is about 2 pi, so `scale` cells to a unit give about 2 pi scale
            // round it, rounded to an even count.
            let expected = 2.0 * (std::f32::consts::PI * scale).round();
            assert!((pattern.around - expected).abs() <= 2.0, "{} {expected}", pattern.around);
            assert_eq!(pattern.around % 2.0, 0.0);
        }
    }

    #[test]
    fn glass_is_see_through_and_keeps_its_ink() {
        let appearance = ItemAppearance::default();
        let glass = Part { material: Some(MaterialKind::Glass), outline: true, ..cylinder(1.0, 3.0, 24) };
        let drink = Part { material: Some(MaterialKind::Gem), outline: false, ..cylinder(0.9, 2.0, 24) };
        let material = appearance.material_for(&glass);
        assert_eq!(material.base.alpha_mode, AlphaMode::Blend);
        assert_eq!(material.base.base_color.alpha(), GLASS_OPACITY);
        assert_eq!(material.base.base_color.with_alpha(1.0), { let Rgb(r, g, b) = appearance.palette.glass; Color::srgb(r, g, b) });
        assert!(is_see_through(&glass) && glass.outline);
        // The drink inside, and every opaque part, draw as before.
        let inside = appearance.material_for(&drink);
        assert_eq!(inside.base.alpha_mode, AlphaMode::Opaque);
        assert_eq!(inside.base.base_color.alpha(), 1.0);
        // A see-through hull goes in the transparent pass, sorted after the glass.
        let ink = OutlineMaterial { see_through: true, ..default() };
        assert_eq!(ink.alpha_mode(), AlphaMode::Blend);
        assert!(ink.depth_bias() > 0.0);
        assert_eq!(OutlineMaterial::default().alpha_mode(), AlphaMode::Opaque);
        assert_eq!(OutlineMaterial::default().depth_bias(), 0.0);
    }

    #[test]
    fn etching_goes_only_on_its_target() {
        use armory::etch::{EtchFinish, EtchPattern, EtchTarget, Etching};
        let etching = Etching { on: true, pattern: EtchPattern::Runes, target: EtchTarget::Metals, ..Etching::default() };
        let appearance = ItemAppearance { etching: Some(etching), ..default() };
        let part = |material| Part { material: Some(material), ..cylinder(0.2, 8.0, 12) };
        let blade = appearance.material_for(&part(MaterialKind::Iron)).extension.etch;
        let grip = appearance.material_for(&part(MaterialKind::Leather)).extension.etch;
        assert_eq!(blade.id, EtchPattern::Runes.shader_id());
        assert_eq!(blade.style, EtchStyle::Engraved);
        assert_eq!(grip, Etch::default(), "leather is not a metal");
        // Blackened lines darken; inlaid ones take the inlay colour (gold by default).
        assert!(!blade.inlaid && blade.style.darken() < 0.5);
        let inlaid = ItemAppearance { etching: Some(Etching { finish: EtchFinish::Inlaid, ..etching }), ..default() };
        let gold = inlaid.material_for(&part(MaterialKind::Iron)).extension.etch;
        assert!(gold.inlaid);
        assert_eq!(gold.color, Vec3::new(0xd4 as f32 / 255.0, 0xa8 as f32 / 255.0, 0x38 as f32 / 255.0));
        // Jewellery's pattern is scaled up with its wear (`decay_scale_factor`).
        let ring = ItemAppearance { decay_scale_factor: 8.0, ..appearance };
        assert_eq!(ring.material_for(&part(MaterialKind::Gold)).extension.etch.scale, etching.scale * 8.0);
        // Cloth takes it as embroidery; an item with no etching draws none.
        let embroidered = Etching { target: EtchTarget::Cloth, ..etching };
        let cloth = ItemAppearance { etching: Some(embroidered), ..default() }.material_for(&part(MaterialKind::Cloth));
        assert_eq!(cloth.extension.etch.style, EtchStyle::Embroidered);
        assert_eq!(ItemAppearance::default().material_for(&part(MaterialKind::Iron)).extension.etch, Etch::default());
    }

    #[test]
    fn a_part_without_a_pattern_draws_none() {
        let appearance = ItemAppearance::default();
        let plain = Part { material: Some(MaterialKind::Cloth), ..cylinder(1.0, 4.0, 48) };
        let material = appearance.material_for(&plain);
        assert_eq!(material.extension.pattern, Pattern::default());
        assert_eq!(material.extension.pattern.id, 0);
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
