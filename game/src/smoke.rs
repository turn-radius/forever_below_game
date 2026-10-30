//! Smoke off a lit censer: give an entity [`Smoke`] and one thin wisp trails up from a point in its
//! own space, curling on a slow breeze, widening and thinning out as it goes.
//!
//! The wisp is a ribbon. The emitter drops a point at its bowl every [`STEP`] seconds; each point
//! keeps the world place it was left, and rises, wanders and is carried by the breeze on its own
//! by age, so a censer that swings or is carried off bends the wisp behind it. One dynamic mesh per
//! emitter is rebuilt each frame as a strip along the points, turned across the delver's view, thin
//! at the bowl and several times wider at the top, clear at both ends. A seeded tileable noise
//! texture runs along it (U across, V along, scrolling up over time so it curls and wavers rather
//! than slides). Unlit and alpha blended, so the smoke takes the incense's colour and neither
//! lights anything nor casts a shadow; being transparent it stays out of the depth and normal
//! prepasses the screen ink reads, so it is never outlined. The sizes, rates and breeze are our own,
//! tuned against captures.

use std::collections::VecDeque;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::{asset::RenderAssetUsages, light::NotShadowCaster, mesh::{Indices, PrimitiveTopology}, prelude::*};
use procgen_rng::Mulberry32;

use crate::play::{DelveSystems, PlayerCamera};

/// Seconds between the points the emitter drops.
pub const STEP: f32 = 1.0 / 30.0;
/// Seconds a point takes to rise and fade, about.
pub const WISP_LIFE: f32 = 2.2;
/// How far a point rises over its life, world units (48 cm).
pub const WISP_RISE: f32 = 4.8;
/// How far the breeze carries a point over its life, world units.
pub const WISP_DRIFT: f32 = 1.2;
/// How far a point wanders sideways at the top of its life, world units.
pub const WISP_WANDER: f32 = 0.55;
/// The wisp's half width as it leaves the bowl and at the top of its life, world units.
pub const WIDTH_START: f32 = 0.08;
pub const WIDTH_END: f32 = 0.8;
/// Most opaque the wisp gets at full density.
pub const PUFF_ALPHA: f32 = 1.0;
/// Seconds a new end of the wisp takes to fade in (at the bowl, or where the incense ran out).
const FADE_IN: f32 = 0.3;
/// Cycles of the noise texture along the whole wisp, and how fast it scrolls up it, cycles a second.
const V_REPEAT: f32 = 1.8;
const V_SCROLL: f32 = 0.35;
/// Extra samples cut between each pair of points, so the strip curves instead of zigzagging.
const SUBDIVIDE: usize = 2;
/// Side of the noise texture, pixels.
const TEX: u32 = 128;

pub struct SmokePlugin;

impl Plugin for SmokePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WispAssets>().add_systems(Update, (keep_trails, drift_trails).chain().in_set(DelveSystems));
    }
}

/// Smoke rising off an entity, from `position` in its own space.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Smoke {
    pub color: Color,
    pub position: Vec3,
    /// How wide the smoke starts, world units.
    pub spread: f32,
    /// How thick it is, 0..1: how opaque the wisp is; none at 0 (the incense has run out).
    pub density: f32,
}

/// One point left in the air by the bowl.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrailPoint {
    /// Where it was left, in the world.
    pub born: Vec3,
    /// Seconds since.
    pub age: f32,
    /// The emitter's clock when it was left: its sway is smooth in this, so neighbours curl together.
    pub clock: f32,
}

/// The wisp of a [`Smoke`], a child of it: its points, oldest first, and the mesh drawn along them.
#[derive(Component, Clone, Debug)]
pub struct Trail {
    pub points: VecDeque<TrailPoint>,
    clock: f32,
    since: f32,
    phase: Vec2,
}

impl Trail {
    fn new(seed: u32) -> Trail {
        let mut rng = Mulberry32::new(seed);
        Trail { points: VecDeque::new(), clock: 0.0, since: 0.0, phase: Vec2::new(rng.range_f32(0.0, 6.28), rng.range_f32(0.0, 6.28)) }
    }

    /// Ages the points `dt` seconds, drops the ones that have risen out, and, if `bowl` is given,
    /// leaves a new one there every [`STEP`].
    pub fn advance(&mut self, dt: f32, bowl: Option<Vec3>) {
        self.clock += dt;
        for point in &mut self.points {
            point.age += dt;
        }
        while self.points.front().is_some_and(|p| p.age >= WISP_LIFE) {
            self.points.pop_front();
        }
        let Some(bowl) = bowl else {
            self.since = 0.0;
            return;
        };
        self.since += dt;
        while self.since >= STEP {
            self.since -= STEP;
            self.points.push_back(TrailPoint { born: bowl, age: self.since, clock: self.clock - self.since });
        }
    }

    /// Where a point is now.
    pub fn place(&self, point: &TrailPoint) -> Vec3 {
        point.born + curl(point.age / WISP_LIFE, point.clock, self.phase)
    }
}

/// How far a point `t` of the way through its life (0 to 1) has come from where it was left: it
/// rises and slows, is carried along the breeze (+X, a little), and wanders sideways more the
/// higher it is, on a sway smooth in `clock` so the points of one wisp curl together.
pub fn curl(t: f32, clock: f32, phase: Vec2) -> Vec3 {
    let t = t.clamp(0.0, 1.0);
    let up = WISP_RISE * t * (1.4 - 0.4 * t);
    let sway = Vec2::new((clock * 2.1 + phase.x).sin() * 0.6 + (clock * 0.9 + phase.y).sin() * 0.4, (clock * 1.7 + phase.y).sin() * 0.6 + (clock * 0.8 + phase.x).sin() * 0.4);
    let wander = sway * WISP_WANDER * t.sqrt() * t;
    Vec3::new(WISP_DRIFT * t * t + wander.x, up, wander.y)
}

/// The half width of the wisp `t` of the way up: thin at the bowl, several times wider at the top.
pub fn width(t: f32) -> f32 {
    WIDTH_START + (WIDTH_END - WIDTH_START) * t.clamp(0.0, 1.0)
}

/// How opaque the wisp is `t` of the way up, `since_end` seconds above its lower end (0..1): clear
/// at the lower end and at the top, so there is no seam and no hard end.
pub fn fade(t: f32, since_end: f32) -> f32 {
    let x = (since_end / FADE_IN).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x) * (1.0 - t.clamp(0.0, 1.0)).powf(0.9)
}

/// The noise texture every wisp is drawn with.
#[derive(Resource)]
pub struct WispAssets {
    pub texture: Handle<Image>,
}

impl FromWorld for WispAssets {
    fn from_world(world: &mut World) -> Self {
        WispAssets { texture: world.resource_mut::<Assets<Image>>().add(wisp_texture()) }
    }
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

    /// Three octaves, 0..1, tiling every whole 1 in x and y (each octave's lattice divides it).
    fn fbm(&self, x: f32, y: f32) -> f32 {
        (self.at(x, y) * 0.55 + self.at(x * 2.0, y * 2.0) * 0.3 + self.at(x * 4.0, y * 4.0) * 0.15).clamp(0.0, 1.0)
    }
}

/// The wisp's texture, [`TEX`] pixels square, white, tileable both ways (so V can scroll): seeded
/// noise in the alpha, cut across by a soft edge in U (clear at both sides), and a little brighter
/// in the core than at the edges.
pub fn wisp_texture() -> Image {
    let noise = Noise::new(&mut Mulberry32::new(0x5eed_a11c), 5);
    let mut data = vec![0u8; (TEX * TEX * 4) as usize];
    for y in 0..TEX {
        for x in 0..TEX {
            let (u, v) = ((x as f32 + 0.5) / TEX as f32, (y as f32 + 0.5) / TEX as f32);
            // Warped, so the tears curl; the warp is itself tileable.
            let wobble = |s: f32| noise.fbm(u + s, v + 0.5 * s) - 0.5;
            let body = noise.fbm(u + wobble(0.13) * 0.4, v + wobble(0.57) * 0.4);
            let thick = ((body - 0.3) / 0.4).clamp(0.0, 1.0);
            let thick = thick * thick * (3.0 - 2.0 * thick);
            // 0 at either side, 1 in the middle.
            let core = (u * std::f32::consts::PI).sin().powf(1.2);
            let alpha = (0.25 + 0.75 * thick) * core;
            let light = 0.78 + 0.22 * core;
            let at = ((y * TEX + x) * 4) as usize;
            let g = (light * 255.0) as u8;
            data[at..at + 4].copy_from_slice(&[g, g, g, (alpha * 255.0) as u8]);
        }
    }
    let mut image = Image::new(
        Extent3d { width: TEX, height: TEX, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: ImageAddressMode::Repeat, address_mode_v: ImageAddressMode::Repeat, ..ImageSamplerDescriptor::linear() });
    image
}

fn wisp_material(smoke: &Smoke, texture: &Handle<Image>) -> StandardMaterial {
    StandardMaterial {
        base_color: smoke.color.with_alpha(PUFF_ALPHA * (0.5 + 0.5 * smoke.density.clamp(0.0, 1.0))),
        base_color_texture: Some(texture.clone()),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        double_sided: true,
        ..default()
    }
}

/// One sample along the wisp, for the strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub at: Vec3,
    /// How far up its life, 0..1.
    pub t: f32,
    /// Seconds above the lower end of the wisp.
    pub since_end: f32,
    /// How much of it the walls leave, 0..1.
    pub clear: f32,
}

/// The strip's vertex buffers.
#[derive(Default, Debug)]
pub struct Strip {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

/// Cuts [`SUBDIVIDE`] samples between each pair of `samples` (Catmull-Rom through their places).
pub fn smooth(samples: &[Sample]) -> Vec<Sample> {
    let n = samples.len();
    let mut out = Vec::with_capacity(n * (SUBDIVIDE + 1));
    for i in 0..n.saturating_sub(1) {
        let (p0, p1, p2, p3) = (samples[i.saturating_sub(1)], samples[i], samples[i + 1], samples[(i + 2).min(n - 1)]);
        for k in 0..=SUBDIVIDE {
            let f = k as f32 / (SUBDIVIDE + 1) as f32;
            let (f2, f3) = (f * f, f * f * f);
            let at = 0.5 * ((2.0 * p1.at) + (p2.at - p0.at) * f + (2.0 * p0.at - 5.0 * p1.at + 4.0 * p2.at - p3.at) * f2 + (3.0 * p1.at - p0.at - 3.0 * p2.at + p3.at) * f3);
            out.push(Sample { at, t: p1.t + (p2.t - p1.t) * f, since_end: p1.since_end + (p2.since_end - p1.since_end) * f, clear: p1.clear + (p2.clear - p1.clear) * f });
        }
    }
    if let Some(&last) = samples.last() {
        out.push(last);
    }
    out
}

/// The camera-facing strip along `samples` (lower end first), seen along `forward`: two vertices at
/// each, across the strip's direction and the view, `width` apart, clear at both ends. `scroll` is
/// how far the texture has run up it, in cycles.
pub fn strip(samples: &[Sample], forward: Vec3, scroll: f32) -> Strip {
    let mut out = Strip::default();
    let n = samples.len();
    if n < 2 {
        return out;
    }
    for (i, s) in samples.iter().enumerate() {
        let tangent = (samples[(i + 1).min(n - 1)].at - samples[i.saturating_sub(1)].at).normalize_or(Vec3::Y);
        let side = tangent.cross(forward).try_normalize().unwrap_or_else(|| forward.any_orthonormal_vector());
        let half = width(s.t) * (0.6 + 0.4 * s.clear);
        let alpha = fade(s.t, s.since_end) * s.clear;
        let v = s.t * V_REPEAT - scroll;
        for (u, sign) in [(0.0, -1.0), (1.0, 1.0)] {
            out.positions.push((s.at + side * half * sign).to_array());
            out.uvs.push([u, v]);
            out.colors.push([1.0, 1.0, 1.0, alpha]);
        }
    }
    for i in 0..(n as u32 - 1) {
        let b = i * 2;
        out.indices.extend([b, b + 1, b + 3, b, b + 3, b + 2]);
    }
    out
}

fn blank_mesh() -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new())
        .with_inserted_indices(Indices::U32(Vec::new()))
}

/// How much of a point of `radius` at `at` the walls leave, 0..1: nothing when it is in rock or a
/// shut door, and less the nearer one stands to it, so smoke thins out against a wall and never
/// cuts hard through it. `solid` says whether a level cell is a wall.
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

/// Gives every changed [`Smoke`] a wisp of its own the first time, and keeps its material to the
/// smoke's colour and density.
fn keep_trails(
    mut commands: Commands,
    emitters: Query<(Entity, &Smoke, Option<&Children>), Changed<Smoke>>,
    trails: Query<&MeshMaterial3d<StandardMaterial>, With<Trail>>,
    assets: Res<WispAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (emitter, smoke, children) in &emitters {
        if let Some(material) = children.into_iter().flatten().find_map(|&c| trails.get(c).ok()) {
            if let Some(mut look) = materials.get_mut(&material.0) {
                *look = wisp_material(smoke, &assets.texture);
            }
            continue;
        }
        let seed = emitter.to_bits() as u32 ^ 0x51ed_270b;
        commands.spawn((
            Name::new("Wisp"),
            Trail::new(seed),
            Mesh3d(meshes.add(blank_mesh())),
            MeshMaterial3d(materials.add(wisp_material(smoke, &assets.texture))),
            NotShadowCaster,
            NoFrustumCulling,
            Transform::default(),
            Visibility::Hidden,
            ChildOf(emitter),
        ));
    }
}

/// Leaves points at each lit bowl, moves them, and rebuilds every wisp's strip across the view.
/// Points keep the world place they were left, so a censer that swings or is carried off leaves a
/// bending trail behind it. The strip is built in world space, so its entity is set to sit at the
/// world's origin however its emitter moves.
#[allow(clippy::type_complexity)]
fn drift_trails(
    mut commands: Commands,
    time: Res<Time>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    emitters: Query<(&Smoke, &GlobalTransform)>,
    mut trails: Query<(Entity, &mut Trail, &mut Transform, &mut Visibility, &Mesh3d, &ChildOf)>,
    mut meshes: ResMut<Assets<Mesh>>,
    walls: Walls,
) {
    let forward = eyes.iter().next().map_or(Vec3::NEG_Z, |eye| eye.forward().as_vec3());
    for (entity, mut trail, mut transform, mut visibility, mesh, parent) in &mut trails {
        let Ok((smoke, world)) = emitters.get(parent.parent()) else {
            commands.entity(entity).despawn();
            continue;
        };
        let emitting = smoke.density > 0.01;
        let bowl = world.transform_point(smoke.position);
        trail.advance(time.delta_secs(), emitting.then_some(bowl));
        // Lower end first: the live point at the bowl, then the newest left and on up.
        let mut samples: Vec<Sample> = Vec::with_capacity(trail.points.len() + 1);
        if emitting && !trail.points.is_empty() {
            samples.push(Sample { at: bowl, t: 0.0, since_end: 0.0, clear: 1.0 });
        }
        samples.extend(trail.points.iter().rev().map(|p| {
            let t = p.age / WISP_LIFE;
            let at = trail.place(p);
            Sample { at, t, since_end: 0.0, clear: walls.clearance(at, width(t)) }
        }));
        let low = samples.first().map_or(0.0, |s| s.t * WISP_LIFE);
        for s in &mut samples {
            s.since_end = (s.t * WISP_LIFE - low).max(0.0);
        }
        let strip = strip(&smooth(&samples), forward, time.elapsed_secs() * V_SCROLL);
        let Some(mut mesh) = meshes.get_mut(&mesh.0) else { continue };
        let count = strip.positions.len();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, strip.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, strip.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, strip.colors);
        mesh.insert_indices(Indices::U32(strip.indices));
        *transform = GlobalTransform::IDENTITY.reparented_to(world);
        visibility.set_if_neq(if count >= 4 { Visibility::Inherited } else { Visibility::Hidden });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(at: Vec3, secs: f32) -> Trail {
        let mut trail = Trail::new(9);
        let mut t = 0.0;
        while t < secs {
            trail.advance(1.0 / 60.0, Some(at));
            t += 1.0 / 60.0;
        }
        trail
    }

    #[test]
    fn points_are_left_at_the_bowl_every_step_and_rise_out_of_the_trail() {
        let trail = lit(Vec3::new(10.0, 1.0, 0.0), 1.0);
        assert!((28..=31).contains(&trail.points.len()), "{}", trail.points.len());
        let ages: Vec<f32> = trail.points.iter().map(|p| p.age).collect();
        assert!(ages.windows(2).all(|w| w[0] > w[1]), "oldest first");
        let long = lit(Vec3::ZERO, WISP_LIFE * 2.0);
        assert!(long.points.len() <= (WISP_LIFE / STEP) as usize + 2, "a short ring: {}", long.points.len());
    }

    #[test]
    fn a_point_stays_where_it_was_left_when_its_emitter_moves_on() {
        let mut trail = lit(Vec3::new(10.0, 1.0, 0.0), 0.5);
        let born: Vec<Vec3> = trail.points.iter().map(|p| p.born).collect();
        let before = trail.place(&trail.points[0]);
        // The censer is swung 6 units on; the points already left are carried only by their own curl.
        trail.advance(0.5, Some(Vec3::new(16.0, 1.0, 0.0)));
        assert!(trail.points.iter().zip(&born).all(|(p, &b)| p.born == b), "left where they were born");
        let after = trail.place(&trail.points[0]);
        assert!((after.x - before.x).abs() < 1.5 && after.x < 12.0, "{before} -> {after}: still trailing near where it was left");
        assert!(after.y > before.y);
        assert_eq!(trail.place(&TrailPoint { born: Vec3::ONE, age: 0.0, clock: 3.0 }), Vec3::ONE, "a new point is at the bowl");
    }

    #[test]
    fn a_point_rises_slowing_and_is_carried_along_the_breeze() {
        let rows: Vec<Vec3> = (0..=10).map(|i| curl(i as f32 / 10.0, 1.0, Vec2::new(1.0, 2.0))).collect();
        assert!(rows.windows(2).all(|w| w[1].y > w[0].y));
        assert!(rows[1].y - rows[0].y > rows[10].y - rows[9].y, "it slows as it rises");
        assert!((rows[10].y - WISP_RISE).abs() < 1e-4);
        assert!(rows[10].x > rows[5].x - WISP_WANDER, "the breeze carries it along");
    }

    #[test]
    fn the_wisp_is_clear_at_both_ends_and_widens_with_age() {
        assert_eq!(fade(0.0, 0.0), 0.0, "clear at the bowl");
        assert_eq!(fade(1.0, 5.0), 0.0, "clear at the top");
        assert!(fade(0.4, 1.0) > 0.3);
        assert_eq!(fade(0.5, 0.0), 0.0, "and where the incense ran out, from its new lower end");
        assert!(width(1.0) > width(0.0) * 4.0, "several times wider at the top");
        assert!((0..10).all(|i| width((i + 1) as f32 / 10.0) > width(i as f32 / 10.0)));
    }

    #[test]
    fn the_strip_faces_the_view_with_clear_ends_and_a_growing_width() {
        let samples: Vec<Sample> = (0..=20).map(|i| Sample { at: Vec3::new(0.0, i as f32 * 0.3, 0.0), t: i as f32 / 20.0, since_end: i as f32 * 0.15, clear: 1.0 }).collect();
        let smoothed = smooth(&samples);
        assert!(smoothed.len() > samples.len() * 2, "cut finer");
        let strip = strip(&smoothed, Vec3::NEG_Z, 0.0);
        assert_eq!(strip.positions.len(), smoothed.len() * 2);
        assert_eq!(strip.indices.len(), (smoothed.len() - 1) * 6);
        let across = |i: usize| (Vec3::from_array(strip.positions[i * 2 + 1]) - Vec3::from_array(strip.positions[i * 2])).length();
        let last = smoothed.len() - 1;
        assert!(across(last) > across(1) * 4.0, "{} -> {}", across(1), across(last));
        assert!(strip.colors[0][3] == 0.0 && strip.colors[last * 2][3] == 0.0, "no hard ends");
        assert!(strip.colors[smoothed.len()][3] > 0.2, "and solid between");
        let flat = Vec3::from_array(strip.positions[1]) - Vec3::from_array(strip.positions[0]);
        assert!(flat.z.abs() < 1e-4, "across the view, not along it: {flat}");
        assert!(strip.uvs.iter().all(|uv| uv[0] == 0.0 || uv[0] == 1.0));
    }

    #[test]
    fn no_wisp_is_left_once_the_incense_runs_out() {
        let mut trail = lit(Vec3::ZERO, 2.0);
        assert!(!trail.points.is_empty());
        let mut t = 0.0;
        while t < WISP_LIFE + 0.1 {
            trail.advance(1.0 / 60.0, None);
            t += 1.0 / 60.0;
        }
        assert!(trail.points.is_empty(), "every point rose out");
        assert!(strip(&smooth(&[]), Vec3::NEG_Z, 0.0).positions.is_empty());
    }

    fn smoking_app(density: f32) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<WispAssets>()
            .add_systems(Update, (keep_trails, drift_trails).chain());
        let smoke = Smoke { color: Color::srgb(0.9, 0.9, 0.9), position: Vec3::new(0.0, 1.0, 0.0), spread: 0.4, density };
        let emitter = app.world_mut().spawn((smoke, Transform::from_xyz(10.0, 0.0, 0.0))).id();
        (app, emitter)
    }

    fn trail_of(app: &App, emitter: Entity) -> Option<Trail> {
        app.world().get::<Children>(emitter)?.iter().find_map(|c| app.world().get::<Trail>(c).cloned())
    }

    #[test]
    fn a_smoking_entity_gets_one_wisp_and_an_unlit_one_leaves_none() {
        let (mut app, emitter) = smoking_app(1.0);
        app.update();
        app.update();
        assert_eq!(app.world().get::<Children>(emitter).unwrap().len(), 1);
        app.world_mut().get_mut::<Smoke>(emitter).unwrap().density = 0.5;
        app.update();
        assert_eq!(app.world().get::<Children>(emitter).unwrap().len(), 1, "a change keeps the one wisp");
        // The censer goes out: its smoke is taken away and the wisp goes with the entity.
        app.world_mut().entity_mut(emitter).despawn();
        app.update();
        assert!(app.world().get_entity(emitter).is_err());
        let (mut dark, emitter) = smoking_app(0.0);
        for _ in 0..5 {
            dark.update();
        }
        assert!(trail_of(&dark, emitter).is_none_or(|t| t.points.is_empty()), "no incense, no wisp");
    }

    #[test]
    fn the_texture_is_soft_clear_at_the_sides_and_tiles_up_and_down() {
        let image = wisp_texture();
        let data = image.data.as_ref().unwrap();
        let side = TEX as usize;
        let alpha = |x: usize, y: usize| data[(y * side + x) * 4 + 3];
        assert!((0..side).all(|y| alpha(0, y) < 12 && alpha(side - 1, y) < 12), "clear at either side");
        let middle: Vec<u8> = (0..side * side / 2).map(|i| alpha(side / 4 + i % (side / 2), i / (side / 2))).collect();
        assert!(middle.iter().any(|&a| a > 150) && middle.iter().any(|&a| a < 90), "uneven");
        let soft = middle.iter().filter(|&&a| (10..240).contains(&a)).count();
        assert!(soft * 3 > middle.len(), "soft, not a cut-out");
        let step = |a: usize, b: usize| (0..side).map(|x| (alpha(x, a) as i32 - alpha(x, b) as i32).abs()).max().unwrap();
        let (seam, ordinary) = (step(0, side - 1), step(40, 41));
        assert!(seam <= ordinary.max(25) + 15, "tiles: seam {seam}, ordinary step {ordinary}");
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
