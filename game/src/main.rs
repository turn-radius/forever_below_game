//! Forever Below. For now a material test bench: one stand-in item per Armory material, pristine
//! on the top rows and a grave find on the bottom rows, lit by a flickering torch and a dim key light.
//! The stand-ins are Bevy primitives converted to `armory::Part`; the turntable replaces this
//! scene once the Armory families build real items.
//!
//! `FB_SCREENSHOT=path.png cargo run -p forever_below` saves a frame and quits.

use armory::{ItemMesh, MaterialKind, Part};
use bevy::{
    camera::{Exposure, RenderTarget},
    mesh::{Indices, VertexAttributeValues},
    prelude::*,
    render::{
        render_resource::TextureFormat,
        view::screenshot::{Screenshot, save_to_disk},
    },
};
use armory::cond::{Condition, ConditionState};
use forever_below::render::{ArmoryRenderPlugin, ItemAppearance, ItemSpawner};

const KINDS: [MaterialKind; 13] = [
    MaterialKind::Iron,
    MaterialKind::Brass,
    MaterialKind::Gold,
    MaterialKind::Silver,
    MaterialKind::Wood,
    MaterialKind::Bone,
    MaterialKind::Stone,
    MaterialKind::Leather,
    MaterialKind::Cord,
    MaterialKind::Gem,
    MaterialKind::Cloth,
    MaterialKind::Paint,
    MaterialKind::Food,
];
const COLUMNS: usize = 7;
const SPACING: f32 = 3.6;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Forever Below".into(), ..default() }),
            ..default()
        }))
        .add_plugins(ArmoryRenderPlugin)
        .insert_resource(GlobalAmbientLight { color: Color::srgb_u8(0xdf, 0xe6, 0xee), brightness: 40.0, ..default() })
        .add_systems(Startup, setup)
        .add_systems(Update, (spin_items, flicker_torch, screenshot_and_quit))
        .run();
}

#[derive(Component)]
struct Spin;

#[derive(Component)]
struct Torch;

/// Offscreen target for `FB_SCREENSHOT`, which works even when the window is hidden.
#[derive(Resource)]
struct ScreenshotTarget(Handle<Image>);

fn setup(mut commands: Commands, mut spawner: ItemSpawner, mut images: ResMut<Assets<Image>>) {
    let camera = commands
        .spawn((Camera3d::default(), Exposure::INDOOR, Transform::from_xyz(0.0, 0.0, 30.0).looking_at(Vec3::ZERO, Vec3::Y)))
        .id();
    if std::env::var_os("FB_SCREENSHOT").is_some() {
        let image = images.add(Image::new_target_texture(
            1920,
            1080,
            TextureFormat::Rgba8Unorm,
            Some(TextureFormat::Rgba8UnormSrgb),
        ));
        commands.entity(camera).insert(RenderTarget::Image(image.clone().into()));
        commands.insert_resource(ScreenshotTarget(image));
    }
    commands.spawn((
        DirectionalLight { illuminance: 400.0, ..default() },
        Transform::from_xyz(-6.0, 10.0, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Torch,
        PointLight { color: Color::srgb_u8(0xff, 0xb3, 0x6a), intensity: 1.5e6, range: 80.0, ..default() },
        Transform::from_xyz(14.0, 2.0, 12.0),
    ));

    let rows = KINDS.len().div_ceil(COLUMNS);
    for (block, state) in [(0, ConditionState::Pristine), (1, ConditionState::GraveFind)] {
        for (i, kind) in KINDS.into_iter().enumerate() {
            let (column, row) = (i % COLUMNS, block * rows + i / COLUMNS);
            let x = (column as f32 - (COLUMNS - 1) as f32 / 2.0) * SPACING;
            let y = ((2 * rows - 1) as f32 / 2.0 - row as f32) * SPACING;
            let appearance = ItemAppearance { condition: Condition::of(state), seed: i as u32 * 31 + 7, ..default() };
            let item = stand_in_item(kind, i);
            spawner.spawn(&item, &appearance).insert((Spin, Transform::from_xyz(x, y, 0.0)));
        }
    }
}

/// A one-part stand-in shape for a material.
fn stand_in_item(kind: MaterialKind, i: usize) -> ItemMesh {
    let mesh = match i % 3 {
        0 => Sphere::new(1.2).mesh().uv(48, 24),
        1 => Torus::new(0.6, 1.3).mesh().build(),
        _ => Capsule3d::new(0.7, 1.4).mesh().build(),
    };
    ItemMesh { parts: vec![part_from_mesh(&mesh, kind)] }
}

/// Copies a smooth Bevy mesh into an `armory::Part`. Its own normals serve as outline
/// normals since the shapes have no hard edges.
fn part_from_mesh(mesh: &Mesh, kind: MaterialKind) -> Part {
    let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(v)) => v.clone(),
        _ => Vec::new(),
    };
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(v)) => v.clone(),
        _ => Vec::new(),
    };
    let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(v)) => v.clone(),
        _ => Vec::new(),
    };
    let indices = mesh.indices().map(Indices::iter).map(|i| i.map(|i| i as u32).collect()).unwrap_or_default();
    Part {
        name: "stand_in",
        material: Some(kind),
        glow: if kind == MaterialKind::Gem { 0.5 } else { 0.0 },
        outline: true,
        outline_normals: normals.clone(),
        positions,
        normals,
        uvs,
        indices,
        ..default()
    }
}

fn spin_items(time: Res<Time>, mut items: Query<&mut Transform, With<Spin>>) {
    for mut transform in &mut items {
        transform.rotate_y(0.4 * time.delta_secs());
        transform.rotate_x(0.15 * time.delta_secs());
    }
}

/// The torch flicker from `docs/rendering.md` ("Light").
fn flicker_torch(time: Res<Time>, mut torches: Query<&mut PointLight, With<Torch>>) {
    let t = time.elapsed_secs();
    let flicker = 0.88 + 0.08 * (13.1 * t).sin() + 0.05 * (7.3 * t + 1.2).sin() + 0.03 * (23.7 * t).sin();
    for mut light in &mut torches {
        light.intensity = 1.5e6 * flicker;
    }
}

fn screenshot_and_quit(
    mut commands: Commands,
    target: Option<Res<ScreenshotTarget>>,
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    let (Some(target), Ok(path)) = (target, std::env::var("FB_SCREENSHOT")) else { return };
    *frame += 1;
    if *frame == 120 {
        commands.spawn(Screenshot::image(target.0.clone())).observe(save_to_disk(path));
    }
    if *frame == 180 {
        exit.write(AppExit::Success);
    }
}
