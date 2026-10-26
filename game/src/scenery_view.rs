//! The scenery view (`--scenery`): a grid of one kind of scenery from consecutive seeds,
//! standing on a plate with its backs to -Z, seen from the front and a little above. For
//! looking over what the generators build before a room is furnished with it.
//!
//! Keys: Left and Right turn to the previous and next seeds, Tab and Shift+Tab switch kind,
//! A and D turn the view, W and S tilt it. Command line: `--scenery --kind bunk --seed 7
//! --count 6` ([`SceneryView::from_args`]); the kinds are `Scenery`'s snake_case names.

use armory::{build::scenery::Scenery, cond::ConditionState};
use bevy::{
    camera::Exposure, prelude::*, render::view::Tonemapping, ui::IsDefaultUiCamera,
    window::PrimaryWindow,
};

use crate::level::fixture_appearance;
use crate::light::StudioLight;
use crate::render::{ItemSpawner, item_bounds};
use crate::turntable::{PLATE, PLATE_INK};

/// The ceiling each piece is built under, as the level's default.
const CEILING: f32 = 30.0;
/// Clear floor between neighbours, in world units.
const GAP: f32 = 3.0;

pub struct SceneryViewPlugin;

impl Plugin for SceneryViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneryView>()
            .insert_resource(ClearColor(PLATE))
            .insert_resource(StudioLight::ARMORY.ambient_light(&Exposure::INDOOR))
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    keys,
                    show.run_if(resource_changed::<SceneryView>),
                    frame_camera,
                    status,
                )
                    .chain(),
            );
    }
}

/// What the view shows. Changing it rebuilds the grid.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct SceneryView {
    pub kind: Scenery,
    pub first_seed: u32,
    pub count: u32,
    /// Radians the view is turned from square on, and tilted down.
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for SceneryView {
    fn default() -> Self {
        SceneryView {
            kind: Scenery::Bunk,
            first_seed: 1207,
            count: 6,
            yaw: 0.35,
            pitch: 0.45,
        }
    }
}

impl SceneryView {
    /// Parses `--kind`, `--seed`, `--count`, `--yaw` and `--pitch` (also `--flag=value`);
    /// `--scenery` itself is skipped. `args` excludes the program name.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<SceneryView, String> {
        let mut view = SceneryView::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (arg, None),
            };
            let mut value = || {
                inline
                    .clone()
                    .or_else(|| args.next())
                    .ok_or(format!("{flag} needs a value"))
            };
            match flag.as_str() {
                "--scenery" => {}
                "--kind" => view.kind = parse_kind(&value()?)?,
                "--seed" => view.first_seed = number(&flag, &value()?)?,
                "--count" => view.count = number::<u32>(&flag, &value()?)?.clamp(1, 12),
                "--yaw" => view.yaw = number(&flag, &value()?)?,
                "--pitch" => view.pitch = number::<f32>(&flag, &value()?)?.clamp(0.0, 1.4),
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        Ok(view)
    }

    pub fn seeds(&self) -> impl Iterator<Item = u32> + use<> {
        let first = self.first_seed;
        (0..self.count).map(move |i| first.wrapping_add(i))
    }

    /// Steps to the next kind (or, negative, the previous).
    pub fn step_kind(&mut self, by: isize) {
        let at = Scenery::ALL
            .iter()
            .position(|&k| k == self.kind)
            .unwrap_or(0) as isize;
        self.kind = Scenery::ALL[(at + by).rem_euclid(Scenery::ALL.len() as isize) as usize];
    }
}

fn number<T: std::str::FromStr>(flag: &str, text: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{flag}: {text:?} is not a number"))
}

fn kind_key(kind: Scenery) -> String {
    format!("{kind:?}").to_lowercase()
}

/// A kind by its name, with or without an underscore (`weapon_rack`, `weaponrack`).
pub fn parse_kind(name: &str) -> Result<Scenery, String> {
    let wanted = name.replace('_', "").to_lowercase();
    Scenery::ALL
        .into_iter()
        .find(|&k| kind_key(k) == wanted)
        .ok_or_else(|| {
            format!(
                "unknown scenery {name:?}; one of: {}",
                Scenery::ALL.map(kind_key).join(", ")
            )
        })
}

#[derive(Component)]
struct ViewCamera;

#[derive(Component)]
struct OnView;

#[derive(Component)]
struct StatusLine;

fn setup(mut commands: Commands) {
    let exposure = Exposure::INDOOR;
    commands
        .spawn((
            ViewCamera,
            Camera3d::default(),
            exposure,
            Tonemapping::Linear,
            IsDefaultUiCamera,
            Transform::from_xyz(0.0, 30.0, 80.0).looking_at(Vec3::ZERO, Vec3::Y),
        ))
        .with_child(StudioLight::ARMORY.key_light(&exposure));
    commands.spawn((
        StatusLine,
        Text::default(),
        TextFont::from_font_size(18.0),
        TextColor(PLATE_INK),
        Node {
            position_type: PositionType::Absolute,
            top: px(16.0),
            left: px(20.0),
            ..default()
        },
    ));
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut view: ResMut<SceneryView>) {
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if keys.just_pressed(KeyCode::Tab) {
        view.step_kind(if shift { -1 } else { 1 });
    }
    let count = view.count;
    if keys.just_pressed(KeyCode::ArrowRight) {
        view.first_seed = view.first_seed.wrapping_add(count);
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        view.first_seed = view.first_seed.wrapping_sub(count);
    }
    for (key, dyaw, dpitch) in [
        (KeyCode::KeyA, -0.25, 0.0),
        (KeyCode::KeyD, 0.25, 0.0),
        (KeyCode::KeyW, 0.0, -0.1),
        (KeyCode::KeyS, 0.0, 0.1),
    ] {
        if keys.just_pressed(key) {
            view.yaw += dyaw;
            view.pitch = (view.pitch + dpitch).clamp(0.0, 1.4);
        }
    }
}

/// Columns of the grid: up to three across.
fn columns(count: u32) -> u32 {
    count.min(3)
}

fn show(
    mut commands: Commands,
    view: Res<SceneryView>,
    old: Query<Entity, With<OnView>>,
    mut spawner: ItemSpawner,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let seeds: Vec<u32> = view.seeds().collect();
    let meshes: Vec<_> = seeds
        .iter()
        .map(|&seed| view.kind.build(seed, CEILING))
        .collect();
    let cell = 2.0 * view.kind.radius() + GAP;
    let cols = columns(view.count);
    let rows = view.count.div_ceil(cols);
    for (i, (&seed, mesh)) in seeds.iter().zip(&meshes).enumerate() {
        let (col, row) = (i as u32 % cols, i as u32 / cols);
        let at = Vec3::new(
            (col as f32 - (cols - 1) as f32 * 0.5) * cell,
            0.0,
            (row as f32 - (rows - 1) as f32 * 0.5) * cell,
        );
        if item_bounds(mesh).is_none() {
            continue;
        }
        let appearance = fixture_appearance(seed, ConditionState::Serviceable);
        spawner
            .spawn(mesh, &appearance)
            .insert((OnView, Transform::from_translation(at)));
    }
}

/// Pulls the camera back far enough to hold the whole grid in view at its angle.
fn frame_camera(
    view: Res<SceneryView>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<(&Projection, &mut Transform), With<ViewCamera>>,
) {
    let Ok((Projection::Perspective(lens), mut transform)) = cameras.single_mut() else {
        return;
    };
    let aspect = windows
        .single()
        .map_or(16.0 / 9.0, |w| w.width() / w.height().max(1.0));
    let cell = 2.0 * view.kind.radius() + GAP;
    let cols = columns(view.count);
    let rows = view.count.div_ceil(cols);
    let (width, depth) = (cols as f32 * cell, rows as f32 * cell);
    let height = match view.kind {
        Scenery::Pillar => CEILING,
        Scenery::Throne | Scenery::Bookshelf => 25.0,
        Scenery::WeaponRack => 27.0,
        Scenery::Bunk => 15.0,
        _ => 12.0,
    };
    let (sin, cos) = view.pitch.sin_cos();
    let half = (lens.fov * 0.5).tan();
    // What the grid spans across the screen at this angle: its width (a little more when turned) and its
    // depth foreshortened plus its height.
    let across = (width * view.yaw.cos().abs() + depth * view.yaw.sin().abs()) * 0.5;
    let up = (depth * sin + height * cos) * 0.5;
    let distance = (across / (half * aspect)).max(up / half) * 1.0 + depth * 0.2;
    let centre = Vec3::new(0.0, height * 0.4, 0.0);
    let offset = Vec3::new(view.yaw.sin() * cos, sin, view.yaw.cos() * cos) * distance;
    *transform = Transform::from_translation(centre + offset).looking_at(centre, Vec3::Y);
}

fn status(view: Res<SceneryView>, mut text: Query<&mut Text, With<StatusLine>>) {
    if !view.is_changed() {
        return;
    }
    let seeds: Vec<u32> = view.seeds().collect();
    let first = seeds.first().copied().unwrap_or(0);
    let last = seeds.last().copied().unwrap_or(0);
    for mut text in &mut text {
        text.0 = format!(
            "{:?}   seeds {first}-{last}   {} cm across\nLeft/Right seeds   Tab kind   A/D turn   W/S tilt",
            view.kind,
            (2.0 * view.kind.radius()) as u32 * 10,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_the_command_line() {
        let view = SceneryView::from_args(args("--scenery --kind weapon_rack --seed=42 --count 3"))
            .unwrap();
        assert_eq!(
            (view.kind, view.first_seed, view.count),
            (Scenery::WeaponRack, 42, 3)
        );
        assert!(SceneryView::from_args(args("--kind chair")).is_err());
    }

    #[test]
    fn every_kind_has_a_name_that_parses_back() {
        for kind in Scenery::ALL {
            assert_eq!(parse_kind(&kind_key(kind)), Ok(kind));
        }
    }

    #[test]
    fn tab_wraps_round_the_kinds() {
        let mut view = SceneryView {
            kind: Scenery::WeaponRack,
            ..default()
        };
        view.step_kind(1);
        assert_eq!(view.kind, Scenery::Rubble);
        view.step_kind(-1);
        assert_eq!(view.kind, Scenery::WeaponRack);
    }
}
