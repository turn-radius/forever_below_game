//! Spell and item effects played from data: every `data/effects/*.toml` ([`fb_effects::Effect`])
//! is read into the [`EffectLibrary`] by file name, and reloaded when it is saved, so an edited
//! file changes the next play. Send a [`PlayEffect`] to play one by name at a caster and some
//! targets; [`fb_effects::Sim`] throws its particles, each drawn as a [`WispMaterial`] card
//! (turned to the eye, or lying on the floor for a ring), and its light is a real point light in
//! the level and another on the view model's stage, so the stone round it and what is held are
//! both lit. Everything goes when the effect ends, and with the delve. See `docs/effects.md`.
//!
//! The files live in the `data` asset source like `rules.toml` ([`crate::rules`]); the folder is
//! read once at startup, so a file added while the game runs shows after a restart, while an
//! edit to one that was there shows at once.

use std::collections::HashMap;

use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    camera::Exposure,
    light::NotShadowCaster,
    prelude::*,
};
use fb_effects::{Card, Effect, LightOut, ParticleOut, Places, Sim, Target};

use crate::carry::{VIEW_MODEL_LAYER, ViewModelCamera};
use crate::casting::{stage_light, stage_spot};
use crate::launch::AppState;
use crate::light::{cast_shadows, point_light};
use crate::play::{DelveSystems, Player, PlayerCamera};
use crate::render::{WispKind, WispMaterial, wisp_card};
use crate::rules::DATA_SOURCE;

/// The folder under the `data` source the effects are read from.
pub const EFFECTS_FOLDER: &str = "effects";

/// How much of its colour a card's core washes toward white.
const CORE_WHITE: f32 = 0.5;
/// A card lying on the floor stands this far over it, so it never fights the floor's depth.
const FLOOR_LIFT: f32 = 0.15;
/// The tall beam's height when its emitter gives no tail, as a multiple of its width.
const BEAM_ASPECT: f32 = 8.0;

/// A `data/effects/*.toml` file as an asset.
#[derive(Asset, TypePath, Deref, Clone, Debug)]
pub struct EffectFile(pub Effect);

/// Reads an effect file. A file that is not a valid effect is an error the asset server logs,
/// and the effect stays as it was.
#[derive(Default, TypePath)]
pub struct EffectLoader;

impl AssetLoader for EffectLoader {
    type Asset = EffectFile;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(&self, reader: &mut dyn Reader, _settings: &(), _load_context: &mut LoadContext<'_>) -> Result<EffectFile, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Effect::from_toml(&String::from_utf8_lossy(&bytes)).map(EffectFile).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    fn extensions(&self) -> &[&str] {
        &["toml"]
    }
}

/// Every effect by file name (`smite` for `smite.toml`).
#[derive(Resource, Default, Debug)]
pub struct EffectLibrary {
    files: HashMap<String, Handle<EffectFile>>,
    effects: HashMap<String, Effect>,
}

impl EffectLibrary {
    /// The effect called `name`.
    pub fn get(&self, name: &str) -> Option<&Effect> {
        self.effects.get(name)
    }

    /// Adds or replaces the effect called `name`, as a loaded file does.
    pub fn insert(&mut self, name: &str, effect: Effect) {
        self.effects.insert(name.to_string(), effect);
    }

    /// The names of every effect, sorted.
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.effects.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }
}

/// Who an effect comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Caster {
    /// The delver, from the first-person camera.
    Delver,
    /// A creature or person, from `eye` units over where it stands.
    Entity { entity: Entity, eye: f32 },
    /// A place, looking along `forward`.
    Point { eye: Vec3, forward: Vec3 },
}

/// What an effect plays on. An effect placed by the target plays on each one given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectTarget {
    /// A creature or person, followed as it moves, `height` units tall.
    Entity { entity: Entity, height: f32 },
    /// A point on the floor.
    Point(Vec3),
}

/// Plays the effect called `name`. The one way anything shows an effect.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct PlayEffect {
    pub name: String,
    pub caster: Caster,
    pub targets: Vec<EffectTarget>,
    /// Seeds its particles: the same seed plays the same.
    pub seed: u32,
}

impl PlayEffect {
    /// `name` from the delver's eye, on nothing in particular.
    pub fn new(name: &str, seed: u32) -> PlayEffect {
        PlayEffect { name: name.to_string(), caster: Caster::Delver, targets: Vec::new(), seed }
    }

    /// Also plays on `entity`, `height` units tall.
    pub fn on(mut self, entity: Entity, height: f32) -> PlayEffect {
        self.targets.push(EffectTarget::Entity { entity, height });
        self
    }

    /// Also plays on the floor at `point`.
    pub fn at(mut self, point: Vec3) -> PlayEffect {
        self.targets.push(EffectTarget::Point(point));
        self
    }

    /// Cast by `caster` instead of the delver.
    pub fn from(mut self, caster: Caster) -> PlayEffect {
        self.caster = caster;
        self
    }
}

/// An effect being played: what it is the effect of, and what it last drew.
#[derive(Component, Debug)]
#[require(DespawnOnExit<AppState> = DespawnOnExit(AppState::Delving))]
pub struct Playing {
    sim: Sim,
    caster: Caster,
    targets: Vec<EffectTarget>,
    /// Where the caster and targets were last found, kept when one has gone.
    places: Places,
    particles: Vec<ParticleOut>,
    lights: Vec<LightOut>,
    /// The cards drawn so far, reused as particles come and go.
    cards: Vec<Entity>,
}

impl Playing {
    /// The particles it drew last step.
    pub fn particles(&self) -> &[ParticleOut] {
        &self.particles
    }

    /// Whether the effect has played out.
    pub fn finished(&self) -> bool {
        self.sim.finished()
    }

    /// Ends it now: no more is thrown and its light sputters out. A lasting effect only ends so.
    pub fn stop(&mut self) {
        self.sim.stop();
    }
}

/// A point light of a playing effect in the level, with the intensity it has at full.
#[derive(Component, Clone, Copy, Debug)]
struct EffectLight {
    of: Entity,
    index: usize,
    full: f32,
}

/// The light a playing effect casts on the view model's stage, a child of the stage camera.
#[derive(Component, Clone, Copy, Debug)]
struct EffectStageLight {
    of: Entity,
    full: f32,
}

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EffectLibrary>().add_message::<PlayEffect>();
        let has_source = app.world().get_resource::<AssetServer>().is_some_and(|server| server.get_source(DATA_SOURCE).is_ok());
        if has_source {
            app.init_asset::<EffectFile>().init_asset_loader::<EffectLoader>().add_systems(Startup, load_effect_files).add_systems(PreUpdate, apply_effect_files);
        }
        app.add_systems(
            Update,
            (
                debug_play_effect,
                start_effects,
                step_effects,
                draw_cards.run_if(resource_exists::<Assets<WispMaterial>>),
                light_effects,
                light_the_stage,
            )
                .chain()
                .in_set(DelveSystems),
        );
    }
}

/// Loads every file in the effects folder.
fn load_effect_files(mut library: ResMut<EffectLibrary>, assets: Res<AssetServer>) {
    let folder = std::path::PathBuf::from(crate::rules::data_folder()).join(EFFECTS_FOLDER);
    let Ok(entries) = std::fs::read_dir(&folder) else {
        warn!("effects: no folder {folder:?}");
        return;
    };
    let mut names: Vec<String> = entries.flatten().filter_map(|e| e.file_name().into_string().ok()).filter_map(|f| f.strip_suffix(".toml").map(str::to_string)).collect();
    names.sort_unstable();
    for name in names {
        let handle = assets.load::<EffectFile>(format!("{DATA_SOURCE}://{EFFECTS_FOLDER}/{name}.toml"));
        library.files.insert(name, handle);
    }
}

/// Copies a file into the library when it loads and each time it is saved.
fn apply_effect_files(mut events: MessageReader<AssetEvent<EffectFile>>, files: Res<Assets<EffectFile>>, mut library: ResMut<EffectLibrary>) {
    for event in events.read() {
        let (AssetEvent::Added { id } | AssetEvent::Modified { id }) = *event else { continue };
        let Some(name) = library.files.iter().find(|(_, handle)| handle.id() == id).map(|(name, _)| name.clone()) else { continue };
        if let Some(file) = files.get(id) {
            info!("effects: applied {name}.toml");
            library.effects.insert(name, file.0.clone());
        }
    }
}

/// Where the caster and targets are now, from `last` where one cannot be found.
fn find_places(caster: Caster, targets: &[EffectTarget], last: &Places, things: &Query<&GlobalTransform, Without<Playing>>, eyes: &Query<&GlobalTransform, With<PlayerCamera>>) -> Places {
    let mut places = last.clone();
    match caster {
        Caster::Delver => {
            if let Some(eye) = eyes.iter().next() {
                places.caster = eye.translation().to_array();
                places.forward = eye.forward().as_vec3().to_array();
            }
        }
        Caster::Entity { entity, eye } => {
            if let Ok(at) = things.get(entity) {
                places.caster = (at.translation() + Vec3::Y * eye).to_array();
                places.forward = at.forward().as_vec3().to_array();
            }
        }
        Caster::Point { eye, forward } => {
            places.caster = eye.to_array();
            places.forward = forward.to_array();
        }
    }
    places.targets.resize(targets.len(), Target { feet: places.caster, height: 0.0 });
    for (target, found) in targets.iter().zip(&mut places.targets) {
        match *target {
            EffectTarget::Entity { entity, height } => {
                if let Ok(at) = things.get(entity) {
                    *found = Target { feet: at.translation().to_array(), height };
                }
            }
            EffectTarget::Point(p) => *found = Target { feet: p.to_array(), height: 0.0 },
        }
    }
    places
}

/// Starts each requested effect: a [`Playing`] entity the other systems step and draw.
fn start_effects(
    mut commands: Commands,
    mut requests: MessageReader<PlayEffect>,
    library: Res<EffectLibrary>,
    things: Query<&GlobalTransform, Without<Playing>>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
) {
    for request in requests.read() {
        let Some(effect) = library.get(&request.name) else {
            warn!("effects: no effect called {}", request.name);
            continue;
        };
        let blank = Places { caster: [0.0; 3], forward: [0.0, 0.0, -1.0], targets: Vec::new() };
        let places = find_places(request.caster, &request.targets, &blank, &things, &eyes);
        commands.spawn((
            Name::new(format!("Effect {}", request.name)),
            Playing { sim: Sim::new(effect.clone(), request.seed, request.targets.len()), caster: request.caster, targets: request.targets.clone(), places, particles: Vec::new(), lights: Vec::new(), cards: Vec::new() },
            Transform::default(),
            Visibility::default(),
        ));
    }
}

/// Steps each playing effect and despawns it, cards and lights with it, once it has played out.
fn step_effects(
    mut commands: Commands,
    time: Res<Time>,
    mut playing: Query<(Entity, &mut Playing)>,
    things: Query<&GlobalTransform, Without<Playing>>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
) {
    for (entity, mut p) in &mut playing {
        let places = find_places(p.caster, &p.targets, &p.places, &things, &eyes);
        p.sim.step(time.delta_secs(), &places);
        p.particles = p.sim.particles();
        p.lights = p.sim.lights(&places);
        p.places = places;
        if p.sim.finished() {
            commands.entity(entity).despawn();
        }
    }
}

/// The picture a card shows and how it is placed: wisps and motes face the eye, sparks stretch
/// along their travel, beams stand on their foot and turn about it, rings lie on the floor.
/// Returns the card's kind and its transform.
fn place_card(p: &ParticleOut, eye: Vec3) -> (WispKind, Transform) {
    let at = Vec3::from_array(p.pos);
    let away = at - eye;
    match p.card {
        Card::Wisp | Card::Mote => {
            let kind = if p.card == Card::Wisp { WispKind::Heart } else { WispKind::Mote };
            (kind, Transform::from_translation(at).looking_to(away, Vec3::Y).with_scale(Vec3::splat(p.size)))
        }
        Card::Spark => {
            let v = Vec3::from_array(p.vel);
            let dir = v.try_normalize().unwrap_or(Vec3::Y);
            let length = (v.length() * p.tail).max(p.size);
            // The head is at the particle; the card stretches back behind it.
            (WispKind::Spark, Transform::from_translation(at - dir * length * 0.5).looking_to(away, dir).with_scale(Vec3::new(p.size, length, 1.0)))
        }
        Card::Beam => {
            let height = if p.tail > 0.0 { p.tail } else { p.size * BEAM_ASPECT };
            let across = Vec3::new(away.x, 0.0, away.z);
            (WispKind::Beam, Transform::from_translation(at + Vec3::Y * height * 0.5).looking_to(across, Vec3::Y).with_scale(Vec3::new(p.size, height, 1.0)))
        }
        Card::Ring => (WispKind::Ring, Transform::from_translation(at + Vec3::Y * FLOOR_LIFT).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)).with_scale(Vec3::splat(p.size))),
    }
}

/// Draws each playing effect's particles on cards, spawning more when it throws more than it
/// has cards and hiding the spare ones.
#[allow(clippy::type_complexity)]
fn draw_cards(
    mut commands: Commands,
    mut playing: Query<(Entity, &mut Playing)>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    mut cards: Query<(&mut Transform, &mut Visibility, &MeshMaterial3d<WispMaterial>)>,
    mut shared: Local<Option<Handle<Mesh>>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WispMaterial>>,
) {
    let Some(eye) = eyes.iter().next().map(GlobalTransform::translation) else { return };
    let mesh = shared.get_or_insert_with(|| meshes.add(wisp_card())).clone();
    for (entity, mut p) in &mut playing {
        let Playing { particles, cards: pool, .. } = &mut *p;
        while pool.len() < particles.len() {
            let material = materials.add(WispMaterial::card(WispKind::Mote, Color::WHITE, Color::WHITE, 0.0, 0.0));
            pool.push(commands.spawn((Name::new("Effect card"), Mesh3d(mesh.clone()), MeshMaterial3d(material), NotShadowCaster, Transform::from_scale(Vec3::ZERO), Visibility::Hidden, ChildOf(entity))).id());
        }
        for (i, card) in pool.iter().enumerate() {
            let Ok((mut at, mut visible, material)) = cards.get_mut(*card) else { continue };
            let Some(particle) = particles.get(i) else {
                if *visible != Visibility::Hidden {
                    *visible = Visibility::Hidden;
                }
                continue;
            };
            let (kind, transform) = place_card(particle, eye);
            *at = transform;
            *visible = Visibility::Inherited;
            let [r, g, b] = particle.colour;
            let color = Color::srgb(r, g, b);
            let core = color.mix(&Color::WHITE, CORE_WHITE);
            if let Some(mut m) = materials.get_mut(&material.0) {
                *m = WispMaterial::card(kind, color, core, particle.alpha, particle.id as f32 * 0.37);
            }
        }
    }
}

/// Lights the level with each playing effect's light: a shadowed point light for each the
/// effect has, following its position and brightness.
fn light_effects(mut commands: Commands, playing: Query<(Entity, &Playing)>, mut lights: Query<(Entity, &EffectLight, &mut PointLight, &mut Transform)>) {
    for (entity, p) in &playing {
        let Some(light) = p.sim.effect().light else { continue };
        for (index, out) in p.lights.iter().enumerate() {
            let existing = lights.iter_mut().find(|(_, l, _, _)| l.of == entity && l.index == index);
            if let Some((_, l, mut point, mut at)) = existing {
                at.translation = Vec3::from_array(out.pos);
                let intensity = l.full * out.brightness.max(0.0);
                if point.intensity != intensity {
                    point.intensity = intensity;
                }
                continue;
            }
            let [r, g, b] = light.colour;
            let mut point = cast_shadows(point_light(Color::srgb(r, g, b), light.peak, light.reach, &Exposure::INDOOR));
            let full = point.intensity;
            point.intensity = 0.0;
            commands.spawn((Name::new("Effect light"), EffectLight { of: entity, index, full }, point, Transform::from_translation(Vec3::from_array(out.pos)), ChildOf(entity)));
        }
    }
}

/// Lights what is held with each playing effect's first light, on the view model's stage the way
/// Divine Light does ([`crate::casting`]): the level's lights never reach it. Goes when the
/// effect does.
#[allow(clippy::type_complexity)]
fn light_the_stage(
    mut commands: Commands,
    playing: Query<&Playing>,
    stages: Query<(Entity, Option<&Exposure>), With<ViewModelCamera>>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    mut lights: Query<(Entity, &EffectStageLight, &mut PointLight, &mut Transform), Without<Playing>>,
    owners: Query<Entity, With<Playing>>,
) {
    let eye = eyes.iter().next();
    for (entity, stage, mut light, mut at) in &mut lights {
        let Ok(p) = playing.get(stage.of) else {
            commands.entity(entity).despawn();
            continue;
        };
        let Some(out) = p.lights.first() else { continue };
        if let Some(eye) = eye {
            at.translation = stage_spot(eye, Vec3::from_array(out.pos));
        }
        light.intensity = stage.full * out.brightness.max(0.0);
    }
    let Some((stage, exposure)) = stages.iter().next() else { return };
    for owner in &owners {
        let Ok(p) = playing.get(owner) else { continue };
        let Some(effect_light) = p.sim.effect().light else { continue };
        if lights.iter().any(|(_, l, _, _)| l.of == owner) {
            continue;
        }
        let [r, g, b] = effect_light.colour;
        let (light, full) = stage_light(Color::srgb(r, g, b), crate::casting::HOLY_STAGE_PEAK * effect_light.peak, exposure);
        commands.spawn((
            Name::new("Effect light on the hands"),
            EffectStageLight { of: owner, full },
            light,
            Transform::default(),
            bevy::camera::visibility::RenderLayers::layer(VIEW_MODEL_LAYER),
            ChildOf(stage),
        ));
    }
}

/// For captures (`FB_PLAY_EFFECT=<name>`): a moment into the delve, plays that effect from the
/// eye on a point of the floor 40 units ahead.
fn debug_play_effect(mut frames: Local<u32>, mut plays: MessageWriter<PlayEffect>, eyes: Query<&GlobalTransform, With<PlayerCamera>>, players: Query<&GlobalTransform, With<Player>>) {
    *frames += 1;
    if *frames != 60 {
        return;
    }
    let Ok(name) = std::env::var("FB_PLAY_EFFECT") else { return };
    let (Some(eye), Some(feet)) = (eyes.iter().next(), players.iter().next()) else { return };
    let ahead = Vec3::new(eye.forward().x, 0.0, eye.forward().z).normalize_or(Vec3::NEG_Z);
    plays.write(PlayEffect::new(name.trim(), 7).at(feet.translation() + ahead * 40.0));
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::state::app::StatesPlugin;

    use super::*;
    use crate::play::{EYE_HEIGHT, run_delve_systems_while_delving};

    const SAMPLE: &str = include_str!("../../data/effects/test_burst.toml");

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, bevy::asset::AssetPlugin::default()))
            .insert_state(AppState::Delving)
            .init_asset::<Mesh>()
            .init_asset::<WispMaterial>()
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)))
            .add_plugins(EffectsPlugin);
        run_delve_systems_while_delving(&mut app);
        app.world_mut().spawn((Player, GlobalTransform::default()));
        app.world_mut().spawn((PlayerCamera, GlobalTransform::from_translation(Vec3::new(0.0, EYE_HEIGHT, 0.0))));
        app.world_mut().resource_mut::<EffectLibrary>().insert("test_burst", Effect::from_toml(SAMPLE).unwrap());
        app.update();
        app
    }

    fn play(app: &mut App, seed: u32) {
        app.world_mut().resource_mut::<Messages<PlayEffect>>().write(PlayEffect::new("test_burst", seed).at(Vec3::new(0.0, 0.0, -40.0)));
    }

    fn playing(app: &mut App) -> usize {
        app.world_mut().query::<&Playing>().iter(app.world()).count()
    }

    #[test]
    fn playing_an_effect_spawns_its_cards_and_light_and_despawns_them_at_the_end() {
        let mut app = app();
        play(&mut app, 1);
        for _ in 0..8 {
            app.update();
        }
        assert_eq!(playing(&mut app), 1);
        let cards = app.world_mut().query::<&MeshMaterial3d<WispMaterial>>().iter(app.world()).count();
        assert!(cards > 30, "{cards}");
        let lights = app.world_mut().query::<&EffectLight>().iter(app.world()).count();
        assert_eq!(lights, 1);
        for _ in 0..40 {
            app.update();
        }
        assert_eq!(playing(&mut app), 0);
        assert_eq!(app.world_mut().query::<&MeshMaterial3d<WispMaterial>>().iter(app.world()).count(), 0);
        assert_eq!(app.world_mut().query::<&EffectLight>().iter(app.world()).count(), 0);
    }

    #[test]
    fn the_same_seed_plays_the_same_particles() {
        let run = |seed: u32| {
            let mut app = app();
            play(&mut app, seed);
            for _ in 0..6 {
                app.update();
            }
            app.world_mut().query::<&Playing>().single(app.world()).unwrap().particles().to_vec()
        };
        assert_eq!(run(5), run(5));
        assert_ne!(run(5), run(6));
    }

    #[test]
    fn editing_a_file_changes_the_next_play() {
        let mut app = app();
        play(&mut app, 1);
        app.update();
        app.update();
        let before = app.world_mut().query::<&Playing>().single(app.world()).unwrap().sim.effect().length;
        let mut edited = Effect::from_toml(SAMPLE).unwrap();
        edited.length = 5.0;
        app.world_mut().resource_mut::<EffectLibrary>().insert("test_burst", edited);
        play(&mut app, 1);
        app.update();
        let lengths: Vec<f32> = app.world_mut().query::<&Playing>().iter(app.world()).map(|p| p.sim.effect().length).collect();
        assert!(lengths.contains(&before) && lengths.contains(&5.0), "{lengths:?}");
    }

    #[test]
    fn an_unknown_effect_plays_nothing() {
        let mut app = app();
        app.world_mut().resource_mut::<Messages<PlayEffect>>().write(PlayEffect::new("nothing_by_that_name", 1));
        app.update();
        assert_eq!(playing(&mut app), 0);
    }

    #[test]
    fn an_effect_plays_on_each_target_it_is_given() {
        let mut app = app();
        let message = PlayEffect::new("test_burst", 1).at(Vec3::new(-10.0, 0.0, -40.0)).at(Vec3::new(10.0, 0.0, -40.0));
        app.world_mut().resource_mut::<Messages<PlayEffect>>().write(message);
        for _ in 0..3 {
            app.update();
        }
        let lights = app.world_mut().query::<&EffectLight>().iter(app.world()).count();
        assert_eq!(lights, 2);
    }

    #[test]
    fn the_shipped_effects_all_read() {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../data").join(EFFECTS_FOLDER);
        for entry in std::fs::read_dir(folder).unwrap().flatten() {
            let text = std::fs::read_to_string(entry.path()).unwrap();
            let effect = Effect::from_toml(&text).unwrap_or_else(|e| panic!("{:?}: {e}", entry.path()));
            assert_eq!(Effect::from_toml(&effect.to_toml().unwrap()).unwrap(), effect, "{:?}", entry.path());
        }
    }
}
