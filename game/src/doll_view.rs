//! The doll view (`--doll`): one folk's body, dressed in a rolled outfit and holding its
//! weapon, turning on the Armory turntable's plate under the turntable's studio light, so
//! bodies and fitted gear can be judged as items are.
//!
//! Keys: Tab and Shift+Tab change the folk, Left and Right the seed, Up and Down the outfit
//! (`data/outfits.json`), Space pauses the turn. Command line:
//! `--doll --folk dwarf --seed 7 --outfit cleric` ([`DollChoice::from_args`]).
//!
//! The seed rolls the body ([`BodySpec::roll`]) and the outfit ([`fb_items::outfit::Outfit::roll`]),
//! whose items are rolled through `fb_items` like any other, so the same folk, seed and outfit
//! always look the same.

use bevy::{camera::Exposure, prelude::*, render::view::Tonemapping, ui::IsDefaultUiCamera};
use fb_body::{BodySpec, Folk};
use fb_items::outfit::{Outfit, outfits};

use crate::figure::{Figure, spawn_figure};
use crate::light::{StudioLight, Torch, point_light};
use crate::render::ItemSpawner;
use crate::turntable::{PLATE, PLATE_INK, SPIN_SPEED, framing_distance};

/// Share of the view's height the figure fills.
const FILL: f32 = 0.86;
/// How dirty the doll view's bodies are: a little, so skin shows its wear (the plate rolls
/// 0.08 to 0.38 for a class outfit).
const DIRT: f32 = 0.2;
/// The held light's strength close up and its reach in multiples of the body's height, on
/// top of the studio light (our own tuning: warm on the near arm and side, gone by the far
/// one).
const HELD_LIGHT_PEAK: f32 = 0.5;
const HELD_LIGHT_REACH: f32 = 0.9;

pub struct DollViewPlugin;

impl Plugin for DollViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DollChoice>()
            .init_resource::<DollSpin>()
            .insert_resource(ClearColor(PLATE))
            .insert_resource(StudioLight::ARMORY.ambient_light(&Exposure::INDOOR))
            .add_systems(Startup, setup)
            .add_systems(Update, (keys, show_figure.run_if(resource_changed::<DollChoice>), spin, frame_camera, show_status).chain());
    }
}

/// What the doll view shows. Changing it redresses the figure.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DollChoice {
    pub folk: Folk,
    pub seed: u32,
    /// Index into [`outfits`].
    pub outfit: usize,
}

impl Default for DollChoice {
    /// A human fighter from seed 1207: a class outfit, so the doll holds a weapon.
    fn default() -> Self {
        DollChoice { folk: Folk::Human, seed: 1207, outfit: outfit_index("fighter").unwrap_or(0) }
    }
}

fn outfit_index(key: &str) -> Option<usize> {
    let wanted = fb_items::outfit::outfit(key)?;
    outfits().iter().position(|o| o.key == wanted.key)
}

impl DollChoice {
    /// Parses `--folk <folk>`, `--seed <n>` and `--outfit <key or label>` (also
    /// `--flag=value`); `--doll` itself is skipped. `args` excludes the program name.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<DollChoice, String> {
        let mut choice = DollChoice::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (arg, None),
            };
            let mut value = || inline.clone().or_else(|| args.next()).ok_or(format!("{flag} needs a value"));
            match flag.as_str() {
                "--doll" => {}
                "--folk" => {
                    let folk = value()?;
                    choice.folk = Folk::parse(&folk).ok_or(format!("unknown folk {folk:?}; one of human, dwarf, gnome, goblin, lizard, pig"))?;
                }
                "--seed" => {
                    let seed = value()?;
                    choice.seed = seed.parse().map_err(|_| format!("--seed: {seed:?} is not a u32"))?;
                }
                "--outfit" => {
                    let key = value()?;
                    let keys = || outfits().iter().map(|o| o.key.as_str()).collect::<Vec<_>>().join(", ");
                    choice.outfit = outfit_index(&key).ok_or_else(|| format!("unknown outfit {key:?}; one of {}", keys()))?;
                }
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        Ok(choice)
    }

    pub fn outfit(&self) -> &'static Outfit {
        &outfits()[self.outfit % outfits().len()]
    }

    /// The figure for this choice.
    pub fn figure(&self) -> Figure {
        let worn = self.outfit().roll(self.seed, self.folk.key()).worn;
        Figure::dress(&BodySpec::roll(self.folk, self.seed), &worn, DIRT)
    }

    /// Steps `by` folk through [`Folk::ALL`], wrapping.
    pub fn step_folk(&mut self, by: isize) {
        let at = Folk::ALL.iter().position(|&f| f == self.folk).unwrap_or(0) as isize;
        self.folk = Folk::ALL[(at + by).rem_euclid(Folk::ALL.len() as isize) as usize];
    }

    /// Steps `by` outfits, wrapping.
    pub fn step_outfit(&mut self, by: isize) {
        self.outfit = (self.outfit as isize + by).rem_euclid(outfits().len() as isize) as usize;
    }
}

#[derive(Resource, Default)]
struct DollSpin {
    angle: f32,
    paused: bool,
}

/// The turning pivot under the figure, and the figure's bounds size.
#[derive(Component)]
struct Pivot {
    size: Vec3,
}

#[derive(Component)]
struct DollCamera;

#[derive(Component)]
struct StatusLine;

fn setup(mut commands: Commands) {
    let exposure = Exposure::INDOOR;
    commands
        .spawn((
            DollCamera,
            Camera3d::default(),
            exposure,
            // As on the turntable: a tone curve would flatten the bands.
            Tonemapping::Linear,
            IsDefaultUiCamera,
            Transform::from_xyz(0.0, 0.0, 60.0),
        ))
        .with_child(StudioLight::ARMORY.key_light(&exposure));
    commands.spawn((
        StatusLine,
        Text::default(),
        TextFont::from_font_size(18.0),
        TextColor(PLATE_INK),
        Node { position_type: PositionType::Absolute, top: px(16.0), left: px(20.0), ..default() },
    ));
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut choice: ResMut<DollChoice>, mut spin: ResMut<DollSpin>) {
    let pressed = |key| keys.just_pressed(key);
    if pressed(KeyCode::Tab) {
        let back = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
        choice.step_folk(if back { -1 } else { 1 });
    }
    if pressed(KeyCode::ArrowRight) {
        choice.seed = choice.seed.wrapping_add(1);
    }
    if pressed(KeyCode::ArrowLeft) {
        choice.seed = choice.seed.wrapping_sub(1);
    }
    if pressed(KeyCode::ArrowDown) {
        choice.step_outfit(1);
    }
    if pressed(KeyCode::ArrowUp) {
        choice.step_outfit(-1);
    }
    if pressed(KeyCode::Space) {
        spin.paused = !spin.paused;
    }
}

/// Redresses the figure for the current choice, on a pivot at the bounds' centre.
fn show_figure(mut commands: Commands, choice: Res<DollChoice>, old: Query<Entity, With<Pivot>>, mut spawner: ItemSpawner) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let figure = choice.figure();
    let Some((min, max)) = figure.bounds() else { return };
    // Turn about the body's own axis (x = z = 0), centred in height on everything it holds.
    let centre = Vec3::new(0.0, (min.y + max.y) * 0.5, 0.0);
    let reach = min.xz().abs().max(max.xz().abs());
    let size = Vec3::new(reach.x * 2.0, max.y - min.y, reach.y * 2.0);
    let pivot = commands.spawn((Pivot { size }, Transform::from_translation(centre), Visibility::default())).id();
    let root = spawn_figure(&mut commands, &mut spawner, &figure);
    commands.entity(root).insert((Transform::from_translation(-centre), ChildOf(pivot)));
    if let Some(light) = figure.light {
        let colour = Color::srgb(light.anchor.color.0, light.anchor.color.1, light.anchor.color.2);
        let range = figure.body.height * HELD_LIGHT_REACH;
        let lamp = Torch::flickering(point_light(colour, HELD_LIGHT_PEAK, range, &Exposure::INDOOR), light.anchor.flicker);
        commands.entity(root).with_child((lamp, Transform::from_translation(light.at)));
    }
}

fn spin(time: Res<Time>, mut state: ResMut<DollSpin>, mut pivots: Query<&mut Transform, With<Pivot>>) {
    if !state.paused {
        state.angle = (state.angle + SPIN_SPEED * time.delta_secs()) % std::f32::consts::TAU;
    }
    for mut transform in &mut pivots {
        transform.rotation = Quat::from_rotation_y(state.angle);
    }
}

/// Stands the camera back from the pivot so the whole turning figure fills [`FILL`] of the view.
fn frame_camera(pivots: Query<(&Pivot, &Transform), Without<DollCamera>>, mut cameras: Query<(&Projection, &mut Transform), With<DollCamera>>) {
    let Some((pivot, at)) = pivots.iter().next() else { return };
    for (projection, mut transform) in &mut cameras {
        let Projection::Perspective(perspective) = projection else { continue };
        let distance = framing_distance(pivot.size, perspective, FILL);
        let target = Transform::from_translation(at.translation + Vec3::Z * distance).looking_at(at.translation, Vec3::Y);
        if *transform != target {
            *transform = target;
        }
    }
}

fn show_status(choice: Res<DollChoice>, mut status: Query<&mut Text, With<StatusLine>>) {
    if !choice.is_changed() {
        return;
    }
    let line = format!(
        "{}   seed {}   {}\nTab folk   Left/Right seed   Up/Down outfit   Space pause",
        choice.folk.race().label,
        choice.seed,
        choice.outfit().label,
    );
    for mut text in &mut status {
        text.0.clone_from(&line);
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
        let choice = DollChoice::from_args(args("--doll --folk dwarf --seed 7")).unwrap();
        assert_eq!((choice.folk, choice.seed, choice.outfit().key.as_str()), (Folk::Dwarf, 7, "fighter"));
        let choice = DollChoice::from_args(args("--folk=lizardfolk --outfit cleric")).unwrap();
        assert_eq!((choice.folk, choice.outfit().key.as_str()), (Folk::Lizardfolk, "cleric"));
        assert_eq!(DollChoice::from_args(vec!["--outfit".into(), "Freshly fallen".into()]).unwrap().outfit().key, "fallen");
        assert!(DollChoice::from_args(args("--folk elf")).is_err());
        assert!(DollChoice::from_args(args("--outfit wizard")).is_err());
        assert!(DollChoice::from_args(args("--seed")).is_err());
    }

    #[test]
    fn folk_and_outfits_cycle_back_round() {
        let start = DollChoice::default();
        let mut choice = start;
        for _ in 0..Folk::ALL.len() {
            choice.step_folk(1);
        }
        choice.step_outfit(-1);
        choice.step_outfit(1);
        assert_eq!(choice, start);
        choice.step_folk(-1);
        assert_eq!(choice.folk, Folk::Pigfolk);
    }

    #[test]
    fn every_folk_renders_dressed_and_holding_a_weapon() {
        for folk in Folk::ALL {
            let choice = DollChoice { folk, seed: 7, ..default() };
            let weapon = choice.outfit().roll(7, folk.key()).item(fb_items::Slot::Mainhand).map(|i| i.title().to_string()).unwrap();
            let figure = choice.figure();
            assert!(figure.pieces.iter().any(|p| p.name == weapon), "{folk:?}");
            assert!(figure.pieces.len() >= 4, "{folk:?}: dressed");
        }
    }

    #[test]
    fn a_choice_looks_the_same_every_time() {
        let choice = DollChoice { folk: Folk::Gnome, seed: 31, outfit: outfit_index("mage").unwrap() };
        assert_eq!(choice.figure().pieces, choice.figure().pieces);
    }
}
