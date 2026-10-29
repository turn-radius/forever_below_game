//! Smoke curling up off a lit censer: give an entity [`Smoke`] and soft puffs rise from a point in
//! its own space, drift on a slow breeze, spread and fade.
//!
//! Like the sparkle stars ([`crate::sparkle`]) the puffs are a handful of flat quads moved on the
//! CPU, each turned to face the delver's eye. Each is a soft round blot (a small generated
//! texture), unlit and alpha blended, so the smoke takes the incense's colour and neither lights
//! anything nor casts a shadow. Being transparent they stay out of the depth and normal prepasses
//! the screen ink reads, so they are never outlined. The sizes, rates and the breeze are our own,
//! tuned against captures.

use bevy::{asset::RenderAssetUsages, light::NotShadowCaster, mesh::{Indices, PrimitiveTopology}, prelude::*};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use procgen_rng::Mulberry32;

use crate::play::{DelveSystems, PlayerCamera};

/// Puffs in the air at once, at full density.
pub const PUFFS: u32 = 10;
/// Seconds a puff takes to rise and fade, about: each is given 75% to 125% of it.
pub const PUFF_LIFE: f32 = 3.4;
/// How far a puff rises over its life, world units (45 cm).
pub const PUFF_RISE: f32 = 4.5;
/// How far the breeze carries a puff over its life, world units.
pub const PUFF_DRIFT: f32 = 1.4;
/// A puff's radius as it leaves and at the end of its life, world units.
pub const PUFF_START: f32 = 0.18;
pub const PUFF_END: f32 = 0.85;
/// Most opaque a puff gets at full density.
pub const PUFF_ALPHA: f32 = 0.5;
/// Side of the soft round blot, pixels.
const BLOT: u32 = 48;

pub struct SmokePlugin;

impl Plugin for SmokePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PuffAssets>().add_systems(Update, (keep_puffs, drift_puffs).chain().in_set(DelveSystems));
    }
}

/// Smoke rising off an entity, from `position` in its own space.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Smoke {
    pub color: Color,
    pub position: Vec3,
    /// How wide the smoke starts, world units.
    pub spread: f32,
    /// How thick it is, 0..1: the share of [`PUFFS`] in the air, and how opaque.
    pub density: f32,
}

/// One puff, a child of its [`Smoke`].
#[derive(Component, Clone, Debug)]
pub struct Puff {
    /// Seconds since it rose; below 0 it waits to.
    age: f32,
    life: f32,
    /// Where it rose from, in the world; `None` until it does.
    from: Option<Vec3>,
    /// Its own sway, sideways, and the breeze it is carried on.
    sway: Vec2,
    index: u32,
    rng: Mulberry32,
}

impl Puff {
    fn new(index: u32, count: u32, seed: u32) -> Puff {
        Puff { age: -PUFF_LIFE * index as f32 / count.max(1) as f32, life: PUFF_LIFE, from: None, sway: Vec2::ZERO, index, rng: Mulberry32::new(seed) }
    }

    /// Ages the puff `dt` seconds. Past its life it rises again from a fresh point.
    fn age(&mut self, dt: f32, world: &GlobalTransform, smoke: &Smoke) {
        self.age += dt;
        if self.age >= self.life {
            self.age -= self.life;
            self.from = None;
        }
        if self.age >= 0.0 && self.from.is_none() {
            let rng = &mut self.rng;
            let spread = smoke.spread;
            let at = smoke.position + Vec3::new(rng.range_f32(-spread, spread), 0.0, rng.range_f32(-spread, spread)) * 0.35;
            self.from = Some(world.transform_point(at));
            self.life = PUFF_LIFE * rng.range_f32(0.75, 1.25);
            self.sway = Vec2::from_angle(rng.range_f32(0.0, std::f32::consts::TAU)) * rng.range_f32(0.1, 0.4);
        }
    }

    /// Where the puff is in the world, its radius and its opacity (0..1), or `None` while it waits.
    fn pose(&self) -> Option<(Vec3, f32, f32)> {
        let from = self.from.filter(|_| self.age >= 0.0)?;
        let (offset, radius, alpha) = curl(self.age / self.life, self.sway);
        Some((from + offset, radius, alpha))
    }
}

/// A puff `t` of the way through its life (0 to 1), swaying along `sway`: how far it has come from
/// where it rose, its radius and its opacity as a share of its most. It rises and slows, is carried
/// along the breeze (+X, a little), swells, and fades in quickly and out slowly.
pub fn curl(t: f32, sway: Vec2) -> (Vec3, f32, f32) {
    let t = t.clamp(0.0, 1.0);
    let up = PUFF_RISE * t * (1.4 - 0.4 * t);
    let wander = sway * (t * std::f32::consts::PI * 1.5).sin();
    let breeze = PUFF_DRIFT * t * t;
    let radius = PUFF_START + (PUFF_END - PUFF_START) * t.sqrt();
    let alpha = (t / 0.12).min(1.0) * (1.0 - t).powf(1.4);
    (Vec3::new(breeze + wander.x, up, wander.y), radius, alpha)
}

/// The one mesh and blot every puff is drawn with.
#[derive(Resource)]
pub struct PuffAssets {
    pub mesh: Handle<Mesh>,
    pub blot: Handle<Image>,
}

impl FromWorld for PuffAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(quad_mesh());
        let blot = world.resource_mut::<Assets<Image>>().add(blot_image());
        PuffAssets { mesh, blot }
    }
}

/// A unit-radius square in the XY plane facing +Z.
pub fn quad_mesh() -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]])
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

/// A soft white round blot: opaque at the middle, fading smoothly to nothing at the edge, broken a
/// little by a fixed ripple so it reads as smoke and not a ball.
pub fn blot_image() -> Image {
    let mut data = Vec::with_capacity((BLOT * BLOT * 4) as usize);
    for y in 0..BLOT {
        for x in 0..BLOT {
            let (u, v) = ((x as f32 + 0.5) / BLOT as f32 * 2.0 - 1.0, (y as f32 + 0.5) / BLOT as f32 * 2.0 - 1.0);
            let angle = v.atan2(u);
            let edge = 1.0 - 0.14 * (angle * 3.0).sin() * (angle * 5.0 + 1.0).cos();
            let d = (u * u + v * v).sqrt() / edge;
            let fall = (1.0 - d).clamp(0.0, 1.0);
            let alpha = fall * fall * (3.0 - 2.0 * fall);
            data.extend_from_slice(&[255, 255, 255, (alpha * 255.0) as u8]);
        }
    }
    Image::new(
        Extent3d { width: BLOT, height: BLOT, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
}

fn puff_material(smoke: &Smoke, blot: &Handle<Image>, alpha: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: smoke.color.with_alpha(alpha),
        base_color_texture: Some(blot.clone()),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        double_sided: true,
        ..default()
    }
}

/// Gives every changed [`Smoke`] its puffs, adding or taking away the difference, each with its
/// own material so it can fade alone.
fn keep_puffs(
    mut commands: Commands,
    emitters: Query<(Entity, &Smoke, Option<&Children>), Changed<Smoke>>,
    puffs: Query<&MeshMaterial3d<StandardMaterial>, With<Puff>>,
    assets: Res<PuffAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (emitter, smoke, children) in &emitters {
        let have: Vec<Entity> = children.into_iter().flatten().copied().filter(|&c| puffs.contains(c)).collect();
        let want = ((PUFFS as f32 * smoke.density.clamp(0.0, 1.0)).ceil() as u32).max(1);
        for &extra in have.iter().skip(want as usize) {
            commands.entity(extra).despawn();
        }
        for index in have.len() as u32..want {
            let seed = emitter.to_bits() as u32 ^ index.wrapping_mul(0x9e37_79b9);
            commands.spawn((
                Name::new("Puff"),
                Puff::new(index, want, seed),
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(materials.add(puff_material(smoke, &assets.blot, 0.0))),
                NotShadowCaster,
                Transform::default(),
                Visibility::Hidden,
                ChildOf(emitter),
            ));
        }
    }
}

/// Moves every puff along its curl, turned to face the eye, and fades it; hides it while it waits.
fn drift_puffs(
    time: Res<Time>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    emitters: Query<(&Smoke, &GlobalTransform)>,
    mut puffs: Query<(&mut Puff, &mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>, &ChildOf)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let facing = eyes.iter().next().map_or(Quat::IDENTITY, |eye| eye.rotation());
    for (mut puff, mut transform, mut visibility, material, parent) in &mut puffs {
        let Ok((smoke, world)) = emitters.get(parent.parent()) else { continue };
        puff.age(time.delta_secs(), world, smoke);
        let Some((at, radius, alpha)) = puff.pose().filter(|&(_, _, alpha)| alpha > 0.01) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let spin = puff.index as f32 * 1.7 + puff.age * 0.25;
        let pose = Transform { translation: at, rotation: facing * Quat::from_rotation_z(spin), scale: Vec3::splat(radius) };
        *transform = GlobalTransform::from(pose).reparented_to(world);
        if let Some(mut look) = materials.get_mut(&material.0) {
            look.base_color = smoke.color.with_alpha(alpha * PUFF_ALPHA * (0.5 + 0.5 * smoke.density));
        }
        visibility.set_if_neq(Visibility::Inherited);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_puff_rises_slowing_swells_and_fades_in_and_out() {
        let sway = Vec2::new(0.3, 0.0);
        let rows: Vec<(Vec3, f32, f32)> = (0..=10).map(|i| curl(i as f32 / 10.0, sway)).collect();
        assert!(rows.windows(2).all(|w| w[1].0.y > w[0].0.y && w[1].1 > w[0].1));
        assert!(rows[1].0.y - rows[0].0.y > rows[10].0.y - rows[9].0.y, "it slows as it rises");
        assert!((rows[10].0.y - PUFF_RISE).abs() < 1e-4);
        assert_eq!((rows[0].2, rows[10].2), (0.0, 0.0), "it fades in from nothing and out to nothing");
        assert!(rows[3].2 > 0.5, "thick early");
        assert!(rows[10].0.x > rows[5].0.x, "the breeze carries it along");
    }

    fn smoking_app(density: f32) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<PuffAssets>()
            .add_systems(Update, (keep_puffs, drift_puffs).chain());
        let smoke = Smoke { color: Color::srgb(0.9, 0.9, 0.9), position: Vec3::new(0.0, 1.0, 0.0), spread: 0.4, density };
        let emitter = app.world_mut().spawn((smoke, Transform::from_xyz(10.0, 0.0, 0.0))).id();
        (app, emitter)
    }

    fn puffs_of(app: &App, emitter: Entity) -> Vec<Entity> {
        app.world().get::<Children>(emitter).map_or(Vec::new(), |c| c.iter().filter(|&p| app.world().get::<Puff>(p).is_some()).collect())
    }

    #[test]
    fn a_smoking_entity_keeps_puffs_by_its_density_and_none_when_it_is_taken_away() {
        let (mut app, emitter) = smoking_app(1.0);
        app.update();
        assert_eq!(puffs_of(&app, emitter).len(), PUFFS as usize);
        app.world_mut().get_mut::<Smoke>(emitter).unwrap().density = 0.5;
        app.update();
        assert_eq!(puffs_of(&app, emitter).len(), (PUFFS / 2) as usize);
        // The entity loses its smoke when the censer goes out: its puffs go with the component's owner.
        app.world_mut().entity_mut(emitter).despawn();
        app.update();
        assert!(app.world().get_entity(emitter).is_err());
    }

    #[test]
    fn puffs_rise_from_the_emitter_one_after_another() {
        let smoke = Smoke { color: Color::WHITE, position: Vec3::new(0.0, 1.0, 0.0), spread: 0.4, density: 1.0 };
        let world = GlobalTransform::from_xyz(10.0, 0.0, 0.0);
        let (mut first, mut later) = (Puff::new(0, 4, 7), Puff::new(1, 4, 8));
        first.age(0.1, &world, &smoke);
        later.age(0.1, &world, &smoke);
        let from = first.from.unwrap();
        assert!((9.5..=10.5).contains(&from.x) && (from.y - 1.0).abs() < 1e-4, "{from}");
        assert!(first.pose().is_some() && later.pose().is_none(), "the second waits its turn");
    }
}
