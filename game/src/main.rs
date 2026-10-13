//! Forever Below. This is only a smoke test: a window, a camera, a light and a placeholder.
//! Milestone 1 replaces the cube with an Armory item drawn in the toon-and-ink look.

use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Forever Below".into(), ..default() }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((Camera3d::default(), Transform::from_xyz(2.5, 2.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((DirectionalLight { illuminance: 8_000.0, ..default() }, Transform::from_xyz(-3.0, 6.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.70, 0.55, 0.32), ..default() })),
    ));
}
