//! Smoke curling up off a lit censer: give an entity [`Smoke`] and soft puffs rise from a point in
//! its own space, drift on a slow breeze, spread and fade.
//!
//! Like the sparkle stars ([`crate::sparkle`]) the puffs are a handful of flat quads moved on the
//! CPU, each turned to face the delver's eye. A puff is a few overlapping wisps at small offsets
//! (so the column has thickness seen edge-on), each a noisy cloud cut from a small generated atlas
//! at its own variant and turn, lighter on top than below, unlit and alpha blended, so the smoke
//! takes the incense's colour and neither lights
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
pub const PUFF_ALPHA: f32 = 0.7;
/// Side of one cloud in the atlas, pixels; the atlas is [`VARIANTS`] clouds across by as many down.
const CLOUD: u32 = 64;
/// Clouds along one side of the atlas: `ATLAS_SIDE * ATLAS_SIDE` variants.
const ATLAS_SIDE: u32 = 2;
pub const VARIANTS: u32 = ATLAS_SIDE * ATLAS_SIDE;
/// Wisps overlapping in one puff.
pub const WISPS: usize = 3;
/// How dark the underside of a wisp is against its top, 0..1 (1 is as bright as the top).
const UNDERSIDE: f32 = 0.5;

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
    /// How much of it the nearest wall leaves, 0..1, eased.
    clear: f32,
}

/// One wisp of a [`Puff`], a child of it: its own cloud of the atlas, turn and place in the puff.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Wisp {
    pub variant: u32,
    /// Turn of the cloud on its quad, radians.
    pub turn: f32,
    /// Where it sits in the puff, in puff radii, world axes.
    pub offset: Vec3,
    /// Its size against the puff's.
    pub size: f32,
}

impl Wisp {
    /// The `n`th wisp of a puff rolled from `rng`: a different cloud for each of the puff's wisps.
    fn roll(rng: &mut Mulberry32, n: usize, first: u32) -> Wisp {
        let variant = (first + n as u32) % VARIANTS;
        let turn = rng.range_f32(0.0, std::f32::consts::TAU);
        let offset = if n == 0 { Vec3::ZERO } else { Vec3::new(rng.range_f32(-0.5, 0.5), rng.range_f32(-0.3, 0.45), rng.range_f32(-0.5, 0.5)) };
        Wisp { variant, turn, offset, size: if n == 0 { 1.0 } else { rng.range_f32(0.6, 0.9) } }
    }

    /// The material transform that picks this wisp's cloud out of the atlas, turned.
    pub fn uv_transform(&self) -> bevy::math::Affine2 {
        let cell = 1.0 / ATLAS_SIDE as f32;
        let origin = Vec2::new((self.variant % ATLAS_SIDE) as f32, (self.variant / ATLAS_SIDE) as f32) * cell;
        let turn = Mat2::from_angle(self.turn) * cell;
        // uv' = origin + cell/2 + turn * (uv - 1/2), the cloud turned about its middle.
        bevy::math::Affine2::from_mat2_translation(turn, origin + Vec2::splat(cell * 0.5) - turn * Vec2::splat(0.5))
    }
}

impl Puff {
    fn new(index: u32, count: u32, seed: u32) -> Puff {
        Puff { age: -PUFF_LIFE * index as f32 / count.max(1) as f32, life: PUFF_LIFE, from: None, sway: Vec2::ZERO, index, rng: Mulberry32::new(seed), clear: 1.0 }
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

/// The one mesh and atlas every wisp is drawn with.
#[derive(Resource)]
pub struct PuffAssets {
    pub mesh: Handle<Mesh>,
    pub atlas: Handle<Image>,
}

impl FromWorld for PuffAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(quad_mesh());
        let atlas = world.resource_mut::<Assets<Image>>().add(cloud_atlas());
        PuffAssets { mesh, atlas }
    }
}

/// A unit-radius square in the XY plane facing +Z, lighter along its top edge than its bottom
/// (vertex colour, so the shading stays upright however the cloud on it is turned).
pub fn quad_mesh() -> Mesh {
    let (top, bottom) = ([1.0, 1.0, 1.0, 1.0], [UNDERSIDE, UNDERSIDE, UNDERSIDE, 1.0]);
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![bottom, bottom, top, top])
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

/// Smooth value noise on a lattice of `cells` per side, rolled from a seed.
struct Noise {
    cells: usize,
    values: Vec<f32>,
}

impl Noise {
    fn new(rng: &mut Mulberry32, cells: usize) -> Noise {
        Noise { cells, values: (0..cells * cells).map(|_| rng.range_f32(0.0, 1.0)).collect() }
    }

    /// The noise at `(x, y)` in 0..1 of the lattice, wrapping at its edges.
    fn at(&self, x: f32, y: f32) -> f32 {
        let (fx, fy) = (x * self.cells as f32, y * self.cells as f32);
        let (ix, iy) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - ix, fy - iy);
        let (tx, ty) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
        let n = self.cells as i32;
        let v = |dx: i32, dy: i32| self.values[((iy as i32 + dy).rem_euclid(n) * n + (ix as i32 + dx).rem_euclid(n)) as usize];
        let (top, bottom) = (v(0, 0) + (v(1, 0) - v(0, 0)) * tx, v(0, 1) + (v(1, 1) - v(0, 1)) * tx);
        top + (bottom - top) * ty
    }

    /// Three octaves, 0..1.
    fn fbm(&self, x: f32, y: f32) -> f32 {
        (self.at(x, y) * 0.55 + self.at(x * 2.0 + 0.31, y * 2.0 + 0.17) * 0.3 + self.at(x * 4.0 + 0.62, y * 4.0 + 0.43) * 0.15).clamp(0.0, 1.0)
    }
}

/// One cloud's opacity at `(u, v)` in -1..1 of its cell: an uneven wisp, thick towards the middle,
/// its edge torn by noise (warped, so the tears curl) and gone before the cell's own edge.
fn cloud_alpha(noise: &Noise, u: f32, v: f32) -> f32 {
    let (x, y) = (u * 0.5 + 0.5, v * 0.5 + 0.5);
    let (wx, wy) = (noise.fbm(x + 0.13, y + 0.71) - 0.5, noise.fbm(x + 0.57, y + 0.29) - 0.5);
    let body = noise.fbm(x + wx * 0.7, y + wy * 0.7);
    let d = (u * u + v * v).sqrt();
    let fall = (1.0 - d / 0.95).clamp(0.0, 1.0);
    // The cloud is its noise above a threshold that rises towards the rim, softly.
    let level = (body + 0.5 * fall - 0.45).clamp(0.0, 1.0);
    let thick = (level / 0.4).clamp(0.0, 1.0);
    thick * thick * (3.0 - 2.0 * thick) * fall.sqrt()
}

/// The atlas of [`VARIANTS`] white clouds, each cut from its own seeded noise, clear at the cell's
/// edge so a turned cloud never shows its cell's borders.
pub fn cloud_atlas() -> Image {
    let side = CLOUD * ATLAS_SIDE;
    let mut data = vec![0u8; (side * side * 4) as usize];
    for variant in 0..VARIANTS {
        let noise = Noise::new(&mut Mulberry32::new(0x5eed_0000 + variant * 7919), 5);
        let (ox, oy) = ((variant % ATLAS_SIDE) * CLOUD, (variant / ATLAS_SIDE) * CLOUD);
        for y in 0..CLOUD {
            for x in 0..CLOUD {
                let (u, v) = ((x as f32 + 0.5) / CLOUD as f32 * 2.0 - 1.0, (y as f32 + 0.5) / CLOUD as f32 * 2.0 - 1.0);
                let alpha = cloud_alpha(&noise, u, v);
                let at = (((oy + y) * side + ox + x) * 4) as usize;
                data[at..at + 4].copy_from_slice(&[255, 255, 255, (alpha * 255.0) as u8]);
            }
        }
    }
    Image::new(
        Extent3d { width: side, height: side, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
}

fn wisp_material(smoke: &Smoke, atlas: &Handle<Image>, wisp: &Wisp) -> StandardMaterial {
    StandardMaterial {
        base_color: smoke.color.with_alpha(0.0),
        base_color_texture: Some(atlas.clone()),
        uv_transform: wisp.uv_transform(),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        double_sided: true,
        ..default()
    }
}

/// How much of a puff of `radius` at `at` the walls leave, 0..1: nothing when its middle is in
/// rock or a shut door, and less the nearer one stands to its middle, so smoke thins out against
/// a wall and never cuts hard through it. `solid` says whether a level cell is a wall.
pub fn wall_clearance(at: Vec3, radius: f32, solid: impl Fn(i32, i32) -> bool) -> f32 {
    let cell = |p: f32| (p / fb_level::CELL).floor() as i32;
    let (cx, cy) = (cell(at.x), cell(at.z));
    if solid(cx, cy) {
        return 0.0;
    }
    // Distance from the middle to each side of its own cell, if the cell beyond is a wall.
    let (lx, lz) = (at.x - cx as f32 * fb_level::CELL, at.z - cy as f32 * fb_level::CELL);
    let sides = [(solid(cx - 1, cy), lx), (solid(cx + 1, cy), fb_level::CELL - lx), (solid(cx, cy - 1), lz), (solid(cx, cy + 1), fb_level::CELL - lz)];
    let near = sides.iter().filter(|(wall, _)| *wall).map(|&(_, d)| d).fold(f32::INFINITY, f32::min);
    if near.is_infinite() {
        return 1.0;
    }
    let t = (near / (radius * 0.9).max(1e-3)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The level's walls and shut doors, if one is loaded.
#[derive(bevy::ecs::system::SystemParam)]
struct Walls<'w> {
    level: Option<Res<'w, crate::play::CurrentLevel>>,
    blocked: Option<Res<'w, crate::play::BlockedCells>>,
    doors: Option<Res<'w, crate::doors::Doors>>,
}

impl Walls<'_> {
    fn clearance(&self, at: Vec3, radius: f32) -> f32 {
        // The censer in hand is drawn on the view model's stage, far below the level, where no wall is.
        let Some(level) = self.level.as_deref().filter(|_| at.y > crate::carry::VIEW_MODEL_STAGE.y * 0.5) else { return 1.0 };
        let none = crate::play::BlockedCells::default();
        let cells = self.blocked.as_deref().unwrap_or(&none);
        wall_clearance(at, radius, crate::doors::solid_for(&level.0, cells, self.doors.as_deref(), None))
    }
}

/// Gives every changed [`Smoke`] its puffs, adding or taking away the difference, each with a few
/// wisps of its own, each wisp with its own material so it can fade alone.
fn keep_puffs(
    mut commands: Commands,
    emitters: Query<(Entity, &Smoke, Option<&Children>), Changed<Smoke>>,
    puffs: Query<(), With<Puff>>,
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
            let wisps = wisps_of(seed);
            let puff = commands.spawn((Name::new("Puff"), Puff::new(index, want, seed), Transform::default(), Visibility::Hidden, ChildOf(emitter))).id();
            for wisp in wisps {
                commands.spawn((
                    Name::new("Wisp"),
                    wisp,
                    Mesh3d(assets.mesh.clone()),
                    MeshMaterial3d(materials.add(wisp_material(smoke, &assets.atlas, &wisp))),
                    NotShadowCaster,
                    Transform::default(),
                    Visibility::Inherited,
                    ChildOf(puff),
                ));
            }
        }
    }
}

/// The wisps of the puff rolled from `seed`.
pub fn wisps_of(seed: u32) -> [Wisp; WISPS] {
    let mut rng = Mulberry32::new(seed ^ 0x51ed_270b);
    let first = rng.int(0, VARIANTS - 1);
    std::array::from_fn(|n| Wisp::roll(&mut rng, n, first))
}

/// Moves every puff along its curl, turned to face the eye, and fades it (and thins it against
/// walls); hides it while it waits. Puffs keep the world place they rose from, so a censer that
/// swings or is carried off leaves a bending trail behind it.
#[allow(clippy::type_complexity)]
fn drift_puffs(
    time: Res<Time>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    emitters: Query<(&Smoke, &GlobalTransform)>,
    mut puffs: Query<(&mut Puff, &mut Transform, &mut Visibility, &ChildOf, &Children)>,
    wisps: Query<(&Wisp, &MeshMaterial3d<StandardMaterial>)>,
    mut wisp_poses: Query<&mut Transform, (With<Wisp>, Without<Puff>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    walls: Walls,
) {
    let facing = eyes.iter().next().map_or(Quat::IDENTITY, |eye| eye.rotation());
    for (mut puff, mut transform, mut visibility, parent, children) in &mut puffs {
        let Ok((smoke, world)) = emitters.get(parent.parent()) else { continue };
        puff.age(time.delta_secs(), world, smoke);
        let Some((at, radius, alpha)) = puff.pose().filter(|&(_, _, alpha)| alpha > 0.01) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        // Ease towards what the walls leave, so a puff drifting in thins instead of snapping.
        let clear = walls.clearance(at, radius);
        puff.clear += (clear - puff.clear) * (time.delta_secs() * 8.0).min(1.0);
        let alpha = alpha * puff.clear;
        if alpha < 0.01 {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        let pose = Transform { translation: at, rotation: facing, scale: Vec3::splat(radius * (0.6 + 0.4 * puff.clear)) };
        *transform = GlobalTransform::from(pose).reparented_to(world);
        // The wisps sit at offsets fixed in the world, so a puff seen edge-on or from another side
        // still has thickness.
        let unturn = facing.inverse();
        for &child in children {
            let (Ok((wisp, material)), Ok(mut local)) = (wisps.get(child), wisp_poses.get_mut(child)) else { continue };
            local.translation = unturn * wisp.offset;
            local.scale = Vec3::splat(wisp.size);
            if let Some(mut look) = materials.get_mut(&material.0) {
                look.base_color = smoke.color.with_alpha(alpha * PUFF_ALPHA * (0.5 + 0.5 * smoke.density) * (1.0 - 0.18 * wisp.offset.length()));
            }
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

    #[test]
    fn puffs_differ_in_cloud_and_turn_and_each_has_distinct_wisps() {
        let rolls: Vec<[Wisp; WISPS]> = (0..12u32).map(|i| wisps_of(i.wrapping_mul(0x9e37_79b9) ^ 77)).collect();
        assert_eq!(rolls, (0..12u32).map(|i| wisps_of(i.wrapping_mul(0x9e37_79b9) ^ 77)).collect::<Vec<_>>(), "rolled from the seed alone");
        let variants: std::collections::BTreeSet<u32> = rolls.iter().map(|w| w[0].variant).collect();
        assert!(variants.len() >= 3, "{variants:?}");
        let turns: std::collections::BTreeSet<u32> = rolls.iter().map(|w| w[0].turn.to_bits()).collect();
        assert_eq!(turns.len(), rolls.len(), "no two puffs turned alike");
        for wisps in &rolls {
            let own: std::collections::BTreeSet<u32> = wisps.iter().map(|w| w.variant).collect();
            assert_eq!(own.len(), WISPS, "a puff's wisps are different clouds");
            assert!(wisps[1].offset != Vec3::ZERO, "the others sit off its middle, for thickness");
        }
    }

    #[test]
    fn the_atlas_is_soft_and_uneven_and_clear_at_each_cells_edge() {
        let image = cloud_atlas();
        let data = image.data.as_ref().unwrap();
        let side = (CLOUD * ATLAS_SIDE) as usize;
        let alpha = |x: usize, y: usize| data[(y * side + x) * 4 + 3];
        for variant in 0..VARIANTS as usize {
            let (ox, oy) = (variant % ATLAS_SIDE as usize * CLOUD as usize, variant / ATLAS_SIDE as usize * CLOUD as usize);
            let c = CLOUD as usize;
            assert!((0..c).all(|i| alpha(ox + i, oy) == 0 && alpha(ox + i, oy + c - 1) == 0 && alpha(ox, oy + i) == 0 && alpha(ox + c - 1, oy + i) == 0), "variant {variant} touches its edge");
            let cell: Vec<u8> = (0..c * c).map(|i| alpha(ox + i % c, oy + i / c)).collect();
            assert!(cell.iter().any(|&a| a > 150), "variant {variant} has a body");
            let soft = cell.iter().filter(|&&a| (10..240).contains(&a)).count();
            assert!(soft * 4 > cell.iter().filter(|&&a| a > 0).count(), "variant {variant} has soft edges, not a cut-out");
        }
        let cells: Vec<Vec<u8>> = (0..VARIANTS as usize).map(|v| (0..CLOUD as usize * 4).map(|i| alpha(v % 2 * CLOUD as usize + 16 + i % 32, v / 2 * CLOUD as usize + 16 + i / 32)).collect()).collect();
        assert!(cells.windows(2).all(|w| w[0] != w[1]), "the variants differ");
    }

    #[test]
    fn a_puff_stays_where_it_rose_when_its_emitter_moves_on() {
        let smoke = Smoke { color: Color::WHITE, position: Vec3::new(0.0, 1.0, 0.0), spread: 0.0, density: 1.0 };
        let mut puff = Puff::new(0, 1, 5);
        puff.age(0.5, &GlobalTransform::from_xyz(10.0, 0.0, 0.0), &smoke);
        let before = puff.pose().unwrap().0;
        // The censer is swung 6 units on; the puff is carried only by its own curl.
        puff.age(0.5, &GlobalTransform::from_xyz(16.0, 0.0, 0.0), &smoke);
        let after = puff.pose().unwrap().0;
        assert!((after.x - before.x).abs() < 1.0 && after.x < 12.0, "{before} -> {after}: still trailing near where it was born");
        assert!(after.y > before.y);
    }

    #[test]
    fn smoke_thins_against_a_wall_and_goes_in_one() {
        let wall = |x: i32, _: i32| x >= 3;
        let c = fb_level::CELL;
        let open = wall_clearance(Vec3::new(0.5 * c, 1.0, 0.5 * c), 0.8, wall);
        let close = wall_clearance(Vec3::new(2.992 * c, 1.0, 0.5 * c), 0.8, wall);
        let inside = wall_clearance(Vec3::new(3.5 * c, 1.0, 0.5 * c), 0.8, wall);
        assert_eq!(open, 1.0);
        assert!(close < 0.5 && close > 0.0, "{close}");
        assert_eq!(inside, 0.0);
        let nearer = wall_clearance(Vec3::new(2.998 * c, 1.0, 0.5 * c), 0.8, wall);
        assert!(nearer < close, "fades smoothly");
    }
}
