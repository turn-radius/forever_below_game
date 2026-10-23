//! The doll view (`--doll`): one folk's body, dressed in a rolled outfit and holding its
//! weapon, turning on the Armory turntable's plate under the turntable's studio light, so
//! bodies and fitted gear can be judged as items are.
//!
//! Keys: Tab and Shift+Tab change the folk, Left and Right the seed, Up and Down the outfit
//! (`data/outfits.json`), B shows the body bare (`--bare`; `--body muscle=1,bust=0.5` sets body
//! fields over the rolled ones, `--without back` leaves a slot of the outfit off), Space pauses the turn (`--turn <radians>` starts it still at that turn), W has the doll walk in place. The panel at the
//! top right sets the walk ([`DollMotion`]): a toggle for walking or standing, and a slider for
//! its speed as a multiple of the folk's own natural pace, so the gait, the skinned gear and the
//! idle can be judged live. Command line:
//! `--doll --folk dwarf --seed 7 --outfit cleric` ([`DollChoice::from_args`]).
//!
//! The seed rolls the body ([`BodySpec::roll`]) and the outfit ([`fb_items::outfit::Outfit::roll`]),
//! whose items are rolled through `fb_items` like any other, so the same folk, seed and outfit
//! always look the same.

use armory::reflect::{Field, FieldKind};
use armory::slider::Slider;
use bevy::{camera::Exposure, mesh::skinning::SkinnedMeshInverseBindposes, prelude::*, render::view::Tonemapping, ui::IsDefaultUiCamera};
use fb_body::{BodySpec, Folk};
use fb_items::outfit::{Outfit, outfits};

use crate::figure::Figure;
use crate::folk::Stride;
use crate::folk_pose::{FolkAnim, FolkPosePlugin, spawn_rigged};
use crate::light::{StudioLight, Torch, point_light};
use crate::render::ItemSpawner;
use crate::turntable::{PLATE, PLATE_INK, SPIN_SPEED, framing_distance};
use crate::tweak_panel::{Tweak, TweakPanelPlugin, TweakPanelSystems, panel_body};
use crate::ui::{BG, INK, LINE, UiFonts};

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
        if !app.is_plugin_added::<FolkPosePlugin>() {
            app.add_plugins(FolkPosePlugin);
        }
        app.init_resource::<DollChoice>()
            .init_resource::<DollSpin>()
            .init_resource::<DollMotion>()
            .init_resource::<UiFonts>()
            .insert_resource(ClearColor(PLATE))
            .insert_resource(StudioLight::ARMORY.ambient_light(&Exposure::INDOOR))
            .add_plugins(TweakPanelPlugin::<DollMotion>::default())
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    (keys, show_figure.run_if(resource_changed::<DollChoice>)).chain().before(TweakPanelSystems),
                    (drive_motion, spin, frame_camera, show_status).chain().after(TweakPanelSystems),
                ),
            );
    }
}

/// What the doll view shows. Changing it redresses the figure.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct DollChoice {
    pub folk: Folk,
    pub seed: u32,
    /// Index into [`outfits`].
    pub outfit: usize,
    /// Show the body bare, without the outfit (`--bare`, or B), to judge its form.
    pub bare: bool,
    /// Body fields set over the rolled body's (`--body muscle=1,bust=0.5`; [`BodyShape::parse`]).
    pub body: Option<&'static str>,
    /// A slot of the outfit left off (`--without back`), to see what it does to the rest.
    pub without: Option<fb_items::Slot>,
    /// Items worn over the outfit's (`--wear "head=Mail coif,shoulders=Cloak"`), rolled from the seed.
    pub wear: Option<&'static str>,
}

impl Default for DollChoice {
    /// A human fighter from seed 1207: a class outfit, so the doll holds a weapon.
    fn default() -> Self {
        DollChoice { folk: Folk::Human, seed: 1207, outfit: outfit_index("fighter").unwrap_or(0), bare: false, body: None, without: None, wear: None }
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
                // The walk's own, read by `DollMotion::from_args`.
                "--doll" | "--walk" => {}
                "--bare" => choice.bare = true,
                "--without" => {
                    let slot = value()?;
                    choice.without = Some(serde_json::from_str(&format!("{slot:?}")).map_err(|_| format!("--without: unknown slot {slot:?}"))?);
                }
                "--wear" => {
                    let text = value()?;
                    for pair in text.split(',') {
                        let (slot, base) = pair.split_once('=').ok_or(format!("--wear: {pair:?} is not slot=Base"))?;
                        serde_json::from_str::<fb_items::Slot>(&format!("{slot:?}")).map_err(|_| format!("--wear: unknown slot {slot:?}"))?;
                        fb_items::roll_item(base, 1).ok_or(format!("--wear: unknown item {base:?}"))?;
                    }
                    choice.wear = Some(Box::leak(text.into_boxed_str()));
                }
                "--body" => {
                    let text = value()?;
                    BodySpec::typical(Folk::Human).shape().parse(&text)?;
                    choice.body = Some(Box::leak(text.into_boxed_str()));
                }
                "--speed" | "--turn" => {
                    value()?;
                }
                "--folk" => {
                    let folk = value()?;
                    choice.folk = Folk::parse(&folk).ok_or(format!("unknown folk {folk:?}; one of human, dwarf, gnome, goblin, lizard, pig, minotaur, faun"))?;
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
        let mut worn = if self.bare { vec![] } else { self.outfit().roll(self.seed, self.folk.key()).worn };
        worn.retain(|(slot, _)| Some(*slot) != self.without);
        for pair in self.wear.into_iter().flat_map(|w| w.split(',')) {
            let Some((slot, base)) = pair.split_once('=') else { continue };
            let (Ok(slot), Some(item)) = (serde_json::from_str::<fb_items::Slot>(&format!("{slot:?}")), fb_items::roll_item(base, self.seed)) else { continue };
            worn.retain(|(s, _)| *s != slot);
            worn.push((slot, item));
        }
        Figure::dress(&self.spec(), &worn, DIRT)
    }

    /// The body: rolled from the seed, with `--body`'s fields over it.
    pub fn spec(&self) -> BodySpec {
        let spec = BodySpec::roll(self.folk, self.seed);
        match self.body.and_then(|text| spec.shape().parse(text).ok()) {
            Some(shape) => spec.with_shape(shape),
            None => spec,
        }
    }

    /// Steps `by` folk through [`Folk::ALL`], wrapping.
    pub fn step_folk(&mut self, by: isize) {
        let all: Vec<Folk> = Folk::ALL.into_iter().chain(Folk::DENIZENS).collect();
        let at = all.iter().position(|&f| f == self.folk).unwrap_or(0) as isize;
        self.folk = all[(at + by).rem_euclid(all.len() as isize) as usize];
    }

    /// Steps `by` outfits, wrapping.
    pub fn step_outfit(&mut self, by: isize) {
        self.outfit = (self.outfit as isize + by).rem_euclid(outfits().len() as isize) as usize;
    }
}

/// How the doll moves, set from the panel: walking in place or standing, and how fast it walks as
/// a multiple of its folk's natural pace ([`fb_body::Gait::natural_speed`]).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct DollMotion {
    pub walking: bool,
    pub speed: f32,
}

impl Default for DollMotion {
    fn default() -> Self {
        DollMotion { walking: false, speed: 1.0 }
    }
}

impl DollMotion {
    /// Parses `--walk` (the doll walks in place from the start) and `--speed <n>` (a multiple of
    /// its natural pace); also `--flag=value`. Other arguments are [`DollChoice::from_args`]'s.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<DollMotion, String> {
        let mut motion = DollMotion::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (arg, None),
            };
            match flag.as_str() {
                "--walk" => motion.walking = true,
                "--speed" => {
                    let text = inline.or_else(|| args.next()).ok_or("--speed needs a value")?;
                    motion.speed = DOLL_SPEEDS.clamp(text.parse().map_err(|_| format!("--speed: {text:?} is not a number"))?);
                }
                _ => {}
            }
        }
        Ok(motion)
    }
}

/// The speeds the panel offers, multiples of the natural pace.
const DOLL_SPEEDS: Slider = Slider::new(0.0, 1.6, 0.05);

impl Tweak for DollMotion {
    fn fields(&self) -> Vec<Field> {
        let field = |name: &str, kind| Field { path: format!("gait.{name}"), section: "gait".into(), name: name.into(), kind };
        vec![field("walking", FieldKind::Toggle { value: self.walking }), field("speed", FieldKind::Slider { value: self.speed, range: DOLL_SPEEDS })]
    }

    fn set_slider(&mut self, path: &str, value: f32) {
        if path == "gait.speed" {
            self.speed = DOLL_SPEEDS.snap(value);
        }
    }

    fn flip(&mut self, path: &str) {
        if path == "gait.walking" {
            self.walking = !self.walking;
        }
    }

    /// The parameters never change shape.
    fn structure(&self) -> u32 {
        0
    }
}

/// How far the doll is turned, and whether it holds still.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct DollSpin {
    angle: f32,
    paused: bool,
}

impl DollSpin {
    /// Parses `--turn <radians>` (also `--turn=<radians>`): the doll starts still at that turn,
    /// 0 facing the camera, for captures. Without it the doll turns from 0.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<DollSpin, String> {
        let mut spin = DollSpin::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (arg, None),
            };
            if flag == "--turn" {
                let text = inline.or_else(|| args.next()).ok_or("--turn needs a value")?;
                spin = DollSpin { angle: text.parse().map_err(|_| format!("--turn: {text:?} is not a number"))?, paused: true };
            }
        }
        Ok(spin)
    }
}

/// The root of the doll's figure, which carries the [`Stride`] the panel sets: `natural` is its
/// folk's own walking pace.
#[derive(Component)]
struct DollFigure {
    natural: f32,
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

fn setup(mut commands: Commands, fonts: Res<UiFonts>) {
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
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(16),
                top: px(16),
                width: px(320),
                height: px(150),
                flex_direction: FlexDirection::Column,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
            BackgroundColor(BG),
            BorderColor::all(LINE),
        ))
        .with_children(|panel| {
            panel.spawn((Text::new("Walk"), UiFonts::style(&fonts.display_bold, 24.0, INK), Node { padding: UiRect::new(px(14), px(14), px(10), px(0)), ..default() }));
            panel_body(panel);
        });
    commands.spawn((
        StatusLine,
        Text::default(),
        TextFont::from_font_size(18.0),
        TextColor(PLATE_INK),
        Node { position_type: PositionType::Absolute, top: px(16.0), left: px(20.0), ..default() },
    ));
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut choice: ResMut<DollChoice>, mut spin: ResMut<DollSpin>, mut motion: ResMut<DollMotion>) {
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
    if pressed(KeyCode::KeyB) {
        choice.bare = !choice.bare;
    }
    if pressed(KeyCode::Space) {
        spin.paused = !spin.paused;
    }
    if pressed(KeyCode::KeyW) {
        motion.walking = !motion.walking;
    }
}

/// Sets the doll's pace from the panel: its natural pace times the slider while walking.
fn drive_motion(motion: Res<DollMotion>, mut figures: Query<(&DollFigure, &mut Stride)>) {
    for (figure, mut stride) in &mut figures {
        let speed = if motion.walking { figure.natural * motion.speed } else { 0.0 };
        if stride.speed != speed {
            stride.speed = speed;
        }
    }
}

/// Redresses the figure for the current choice, on a pivot at the bounds' centre.
fn show_figure(
    mut commands: Commands,
    choice: Res<DollChoice>,
    old: Query<Entity, With<Pivot>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut spawner: ItemSpawner,
) {
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
    // The figure is skinned (as the folk in the level are): its root carries the stride the panel
    // sets, a skeleton under it the joints and meshes.
    let root = commands
        .spawn((
            Name::new("Figure"),
            DollFigure { natural: figure.gait.natural_speed() },
            Stride::default(),
            Transform::from_translation(-centre),
            Visibility::default(),
            ChildOf(pivot),
        ))
        .id();
    let skeleton = commands.spawn((Name::new("Skeleton"), Transform::default(), Visibility::default(), ChildOf(root))).id();
    let anim: FolkAnim = spawn_rigged(&mut spawner, &mut bindposes, &figure, skeleton, false, choice.seed);
    if let Some(light) = figure.light {
        let colour = Color::srgb(light.anchor.color.0, light.anchor.color.1, light.anchor.color.2);
        let range = figure.body.height * HELD_LIGHT_REACH;
        let lamp = Torch::flickering(point_light(colour, HELD_LIGHT_PEAK, range, &Exposure::INDOOR), light.anchor.flicker);
        spawner.commands().spawn((lamp, Transform::from_translation(anim.in_joint(light.joint, light.at)), ChildOf(anim.joint(light.joint))));
    }
    commands.entity(skeleton).insert(anim);
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
        "{}   seed {}   {}\nTab folk   Left/Right seed   Up/Down outfit   B bare   Space pause   W walk",
        choice.folk.race().label,
        choice.seed,
        if choice.bare { "Bare" } else { &choice.outfit().label },
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
    fn the_walk_comes_from_the_command_line() {
        assert_eq!(DollMotion::from_args(args("--doll --folk dwarf")).unwrap(), DollMotion { walking: false, speed: 1.0 });
        assert_eq!(DollMotion::from_args(args("--doll --walk --speed 0.5")).unwrap(), DollMotion { walking: true, speed: 0.5 });
        assert_eq!(DollMotion::from_args(args("--walk --speed=9")).unwrap().speed, 1.6);
        assert!(DollMotion::from_args(args("--speed fast")).is_err());
        assert_eq!(DollSpin::from_args(args("--doll --turn 1.5")).unwrap(), DollSpin { angle: 1.5, paused: true });
        assert!(!DollSpin::from_args(args("--doll")).unwrap().paused);
        assert!(DollChoice::from_args(args("--turn=0.5 --seed 7")).is_ok());
        assert!(DollChoice::from_args(args("--bare")).unwrap().bare);
        let shaped = DollChoice::from_args(args("--body muscle=1,bust=0.5")).unwrap();
        assert_eq!((shaped.spec().muscle, shaped.spec().bust), (1.0, 0.5));
        assert!(DollChoice::from_args(args("--body wings=1")).is_err());
        assert_eq!(DollChoice::from_args(args("--without back")).unwrap().without, Some(fb_items::Slot::Back));
        assert!(DollChoice::from_args(args("--without tail")).is_err());
        // The choice leaves the walk's flags alone.
        assert!(DollChoice::from_args(args("--doll --walk --speed 0.5 --seed 7")).is_ok());
    }

    #[test]
    fn the_panel_sets_the_walk() {
        let mut motion = DollMotion::default();
        motion.flip("gait.walking");
        motion.set_slider("gait.speed", 0.77);
        assert_eq!(motion, DollMotion { walking: true, speed: 0.75 });
        assert_eq!(Tweak::slider(&motion, "gait.speed"), Some(0.75));
        assert_eq!(motion.fields().len(), 2);
    }

    #[test]
    fn folk_and_outfits_cycle_back_round() {
        let start = DollChoice::default();
        let mut choice = start;
        for _ in 0..Folk::ALL.len() + Folk::DENIZENS.len() {
            choice.step_folk(1);
        }
        choice.step_outfit(-1);
        choice.step_outfit(1);
        assert_eq!(choice, start);
        choice.step_folk(-1);
        assert_eq!(choice.folk, Folk::Minotaur, "Tab goes on through the denizens");
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
        let choice = DollChoice { folk: Folk::Gnome, seed: 31, outfit: outfit_index("mage").unwrap(), bare: false, body: None, without: None, wear: None };
        assert_eq!(choice.figure().pieces, choice.figure().pieces);
    }
}
