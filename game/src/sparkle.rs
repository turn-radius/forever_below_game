//! Little four-pointed stars that drift up off a thing and fade: give an entity [`Sparkles`]
//! and that many stars rise from a box in its own space, each turned to face the delver's eye.
//! A legendary lying on the floor gives them off over its shimmer (`crate::carry::floor_sparkles`),
//! the user's call ("a subtle particle effect of little 4 pointed stars drifting up from them").
//!
//! The stars are a handful of flat meshes moved on the CPU, not a particle system: a few per
//! item is all the effect asks for. They are unlit and drawn added onto what is behind them
//! (`AlphaMode::Add`), so they glow in the dark, light nothing and cast no shadow; being
//! transparent they stay out of the depth and normal prepasses the screen ink reads, so they
//! are never outlined. A star fades by shrinking, so every star shares its emitter's material.
//! The sizes, rates and the star's shape are our own, tuned against captures.

use bevy::{asset::RenderAssetUsages, light::NotShadowCaster, mesh::{Indices, PrimitiveTopology}, prelude::*};
use procgen_rng::Mulberry32;

use crate::play::{DelveSystems, PlayerCamera};

/// Seconds a star takes to rise and fade, about: each is given 75% to 125% of it.
pub const STAR_LIFE: f32 = 2.6;
/// How far a star rises over its life, world units (50 cm).
pub const STAR_RISE: f32 = 5.0;
/// How far a star wanders sideways as it rises, world units.
pub const STAR_SWAY: f32 = 0.35;
/// A star's radius to the tip of a point at its fullest, world units (4 cm).
pub const STAR_RADIUS: f32 = 0.4;
/// The radius between the points, as a share of [`STAR_RADIUS`]: low for thin points.
const STAR_WAIST: f32 = 0.22;
/// Share of its life a star takes to grow to its full size.
const STAR_GROW: f32 = 0.15;
/// How fast a star twinkles, radians a second.
const TWINKLE_RATE: f32 = 9.0;
/// How fast a star turns, radians a second.
const SPIN_RATE: f32 = 0.8;
/// How much a star's colour is lifted toward white, so it reads as light.
const STAR_WHITEN: f32 = 0.35;

pub struct SparklePlugin;

impl Plugin for SparklePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StarMesh>().add_systems(Update, (keep_stars, rise_stars).chain().in_set(DelveSystems));
    }
}

/// Stars rising off an entity: `count` in the air at once, in `color`, each from a point in
/// the box `min` to `max` of the entity's own space.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Sparkles {
    pub color: Color,
    pub count: u32,
    pub min: Vec3,
    pub max: Vec3,
}

/// One star, a child of its [`Sparkles`].
#[derive(Component, Clone, Debug)]
pub struct Star {
    /// Seconds since it rose; below 0 it is still waiting to.
    age: f32,
    life: f32,
    /// Where it rose from, in the world; `None` until it does.
    from: Option<Vec3>,
    /// The way it sways, and its twinkle's phase.
    sway: Vec2,
    phase: f32,
    rng: Mulberry32,
}

impl Star {
    /// The `index`th of `count` stars, waiting its turn so the stars rise one after another.
    fn new(index: u32, count: u32, seed: u32) -> Star {
        Star { age: -STAR_LIFE * index as f32 / count.max(1) as f32, life: STAR_LIFE, from: None, sway: Vec2::ZERO, phase: 0.0, rng: Mulberry32::new(seed) }
    }

    /// Ages the star `dt` seconds. A star past its life rises again from a fresh point of the
    /// box `min` to `max`, carried into the world by `world`.
    fn age(&mut self, dt: f32, world: &GlobalTransform, min: Vec3, max: Vec3) {
        self.age += dt;
        if self.age >= self.life {
            self.age -= self.life;
            self.from = None;
        }
        if self.age >= 0.0 && self.from.is_none() {
            let rng = &mut self.rng;
            let at = Vec3::new(rng.range_f32(min.x, max.x), rng.range_f32(min.y, max.y), rng.range_f32(min.z, max.z));
            self.from = Some(world.transform_point(at));
            self.life = STAR_LIFE * rng.range_f32(0.75, 1.25);
            let angle = rng.range_f32(0.0, std::f32::consts::TAU);
            self.sway = Vec2::from_angle(angle) * STAR_SWAY * rng.range_f32(0.4, 1.0);
            self.phase = rng.range_f32(0.0, std::f32::consts::TAU);
        }
    }

    /// Where the star is in the world, its size as a share of [`STAR_RADIUS`] and its turn
    /// about the line of sight, or `None` while it waits to rise.
    fn pose(&self) -> Option<(Vec3, f32, f32)> {
        let from = self.from.filter(|_| self.age >= 0.0)?;
        let (offset, size) = rise(self.age / self.life, self.sway);
        let twinkle = 0.75 + 0.25 * (self.age * TWINKLE_RATE + self.phase).sin();
        Some((from + offset, size * twinkle, self.phase + self.age * SPIN_RATE))
    }
}

/// A star `t` of the way through its life (0 to 1), swaying along `sway`: how far it has come
/// from where it rose, and its size as a share of its fullest. It rises quickly and slows,
/// swaying out and back once, grows over [`STAR_GROW`] of its life and shrinks away to nothing.
pub fn rise(t: f32, sway: Vec2) -> (Vec3, f32) {
    let t = t.clamp(0.0, 1.0);
    let up = STAR_RISE * t * (1.5 - 0.5 * t);
    let aside = sway * (t * std::f32::consts::PI).sin();
    let size = (t / STAR_GROW).min(1.0) * (1.0 - t);
    (Vec3::new(aside.x, up, aside.y), size)
}

/// The one mesh every star is drawn with: a flat four-pointed star of radius 1 facing +Z.
#[derive(Resource)]
pub struct StarMesh(pub Handle<Mesh>);

impl FromWorld for StarMesh {
    fn from_world(world: &mut World) -> Self {
        StarMesh(world.resource_mut::<Assets<Mesh>>().add(star_mesh()))
    }
}

/// A flat four-pointed star of radius 1 in the XY plane, facing +Z: a fan about its middle
/// out to four points, with [`STAR_WAIST`] between them.
pub fn star_mesh() -> Mesh {
    let mut positions = vec![[0.0, 0.0, 0.0]];
    positions.extend((0..8).map(|i| {
        let angle = i as f32 * std::f32::consts::FRAC_PI_4;
        let r = if i % 2 == 0 { 1.0 } else { STAR_WAIST };
        [r * angle.cos(), r * angle.sin(), 0.0]
    }));
    let indices = (0..8u32).flat_map(|i| [0, 1 + i, 1 + (i + 1) % 8]).collect();
    let count = positions.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count])
        .with_inserted_indices(Indices::U32(indices))
}

/// The material a star in `color` is drawn with: unlit, a little whiter than `color` and
/// added onto what is behind it.
pub fn star_material(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color.mix(&Color::WHITE, STAR_WHITEN),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        cull_mode: None,
        ..default()
    }
}

/// Gives every changed [`Sparkles`] its `count` of stars, adding or taking away the difference.
fn keep_stars(
    mut commands: Commands,
    emitters: Query<(Entity, &Sparkles, Option<&Children>), Changed<Sparkles>>,
    stars: Query<&MeshMaterial3d<StandardMaterial>, With<Star>>,
    mesh: Res<StarMesh>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (emitter, sparkles, children) in &emitters {
        let have: Vec<Entity> = children.into_iter().flatten().copied().filter(|&c| stars.contains(c)).collect();
        for &extra in have.iter().skip(sparkles.count as usize) {
            commands.entity(extra).despawn();
        }
        let material = match have.first().and_then(|&s| stars.get(s).ok()) {
            Some(material) => {
                if let Some(mut look) = materials.get_mut(&material.0) {
                    look.base_color = star_material(sparkles.color).base_color;
                }
                material.0.clone()
            }
            None => materials.add(star_material(sparkles.color)),
        };
        for index in have.len() as u32..sparkles.count {
            let seed = emitter.to_bits() as u32 ^ index.wrapping_mul(0x9e37_79b9);
            commands.spawn((
                Name::new("Star"),
                Star::new(index, sparkles.count, seed),
                Mesh3d(mesh.0.clone()),
                MeshMaterial3d(material.clone()),
                NotShadowCaster,
                Transform::default(),
                Visibility::Hidden,
                ChildOf(emitter),
            ));
        }
    }
}

/// Moves every star along its rise, turned to face the eye, and hides it while it waits.
fn rise_stars(
    time: Res<Time>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    emitters: Query<(&Sparkles, &GlobalTransform)>,
    mut stars: Query<(&mut Star, &mut Transform, &mut Visibility, &ChildOf)>,
) {
    let facing = eyes.iter().next().map_or(Quat::IDENTITY, |eye| eye.rotation());
    for (mut star, mut transform, mut visibility, parent) in &mut stars {
        let Ok((sparkles, world)) = emitters.get(parent.parent()) else { continue };
        star.age(time.delta_secs(), world, sparkles.min, sparkles.max);
        let Some((at, size, spin)) = star.pose().filter(|&(_, size, _)| size > 0.0) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let pose = Transform { translation: at, rotation: facing * Quat::from_rotation_z(spin), scale: Vec3::splat(size * STAR_RADIUS) };
        *transform = GlobalTransform::from(pose).reparented_to(world);
        visibility.set_if_neq(Visibility::Inherited);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_star_rises_slowing_and_shrinks_away() {
        let sway = Vec2::new(STAR_SWAY, 0.0);
        let heights: Vec<f32> = (0..=10).map(|i| rise(i as f32 / 10.0, sway).0.y).collect();
        assert!(heights.windows(2).all(|w| w[1] > w[0]), "{heights:?}");
        assert!(heights[1] - heights[0] > heights[10] - heights[9], "it slows as it rises");
        assert!((heights[10] - STAR_RISE).abs() < 1e-4);
        assert_eq!(rise(0.0, sway).1, 0.0);
        assert!((rise(STAR_GROW, sway).1 - (1.0 - STAR_GROW)).abs() < 1e-5);
        assert_eq!(rise(1.0, sway).1, 0.0);
        // It sways out and comes back over its rise.
        assert!(rise(0.5, sway).0.x > 0.9 * STAR_SWAY && rise(1.0, sway).0.x.abs() < 1e-4);
    }

    #[test]
    fn the_star_mesh_has_four_long_points() {
        let mesh = star_mesh();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        let tips = positions.iter().filter(|p| (Vec3::from_array(**p).length() - 1.0).abs() < 1e-5).count();
        assert_eq!(tips, 4);
        assert_eq!(mesh.indices().unwrap().len(), 8 * 3);
    }

    fn sparkling_app(count: u32) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<StarMesh>()
            .add_systems(Update, (keep_stars, rise_stars).chain());
        let sparkles = Sparkles { color: Color::srgb(1.0, 0.6, 0.3), count, min: Vec3::splat(-1.0), max: Vec3::splat(1.0) };
        let emitter = app.world_mut().spawn((sparkles, Transform::from_xyz(10.0, 0.0, 0.0))).id();
        (app, emitter)
    }

    fn stars_of(app: &mut App, emitter: Entity) -> Vec<Entity> {
        app.world().get::<Children>(emitter).map_or(Vec::new(), |c| c.iter().filter(|&s| app.world().get::<Star>(s).is_some()).collect())
    }

    #[test]
    fn an_emitter_keeps_its_count_of_stars_as_the_count_changes() {
        let (mut app, emitter) = sparkling_app(5);
        app.update();
        assert_eq!(stars_of(&mut app, emitter).len(), 5);
        app.world_mut().get_mut::<Sparkles>(emitter).unwrap().count = 2;
        app.update();
        assert_eq!(stars_of(&mut app, emitter).len(), 2);
        app.world_mut().get_mut::<Sparkles>(emitter).unwrap().count = 0;
        app.update();
        assert!(stars_of(&mut app, emitter).is_empty());
    }

    #[test]
    fn stars_rise_from_their_box_one_after_another() {
        let mut star = Star::new(0, 4, 7);
        let mut later = Star::new(1, 4, 8);
        let world = GlobalTransform::from_xyz(10.0, 0.0, 0.0);
        star.age(0.1, &world, Vec3::splat(-1.0), Vec3::splat(1.0));
        later.age(0.1, &world, Vec3::splat(-1.0), Vec3::splat(1.0));
        let from = star.from.unwrap();
        assert!((9.0..=11.0).contains(&from.x) && (-1.0..=1.0).contains(&from.y), "{from}");
        assert!(star.pose().is_some() && later.pose().is_none(), "the second waits its turn");
        // Past its life it rises again from a fresh point.
        star.age(STAR_LIFE * 1.3, &world, Vec3::splat(-1.0), Vec3::splat(1.0));
        assert_ne!(star.from, Some(from));
    }
}
