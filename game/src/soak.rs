//! `FB_SOAK=<seconds>`: a debug autopilot that plays the level for that long, then quits, to
//! find crashes and leaks that only show after a long session (#171).
//!
//! It plays through the same inputs a player uses where it can: it holds W and turns with the
//! mouse along random paths across the level ([`fb_level::find_path`]), turns on creatures in
//! sight and swings at them, and opens and closes the inventory (I) and the menu (Esc). Picking
//! up, opening containers, equipping and dropping go through the inventory's own rules
//! ([`crate::inventory::quick`], [`crate::inventory::drop_at`]) rather than aiming the
//! crosshair. With `--title` it presses Enter on the title to begin a new delve, a second after
//! the title shows, and first logs a `soak: title` line with the entities on it, so the title
//! after each death can be held against the first (#177).
//!
//! Every [`REPORT_SECONDS`] it logs a `soak:` line to stderr: entities, mesh, image and material
//! assets, cameras, readbacks, icons, render phases and the smoothed frame time, so counts that
//! grow without bound show in the log, and then the active cameras by name.
//! The line also gives the render world's GPU preprocessing mode, which must read
//! `PreprocessingOnly` (`crate::render`'s `indirect`): in Bevy's `Culling` mode, GPU light
//! clustering now and then read back a garbage index count and crashed the render thread after a
//! while (#171, `docs/rendering.md`). `FB_SOAK_CHURN=1` also forgets every inventory icon at
//! each report, so they are all drawn again (many icon cameras made and taken down).

use bevy::{
    core_pipeline::core_3d::Opaque3d,
    diagnostic::{DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin},
    ecs::resource::IsResource,
    input::{InputSystems, mouse::AccumulatedMouseMotion},
    pbr::Shadow,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        batching::gpu_preprocessing::{GpuPreprocessingMode, GpuPreprocessingSupport},
        gpu_readback::Readback,
        render_phase::ViewBinnedRenderPhases,
    },
};
use fb_level::{
    CELL,
    path::{find_path, line_of_sight},
};
use procgen_rng::Mulberry32;

use crate::carry::{FloorItem, REACH};
use crate::combat::feedback::Corpse;
use crate::creatures::Critter;
use crate::inventory::{
    Drag, FloorChanges, Inventory, Location, OpenContainer, Target, icon::IconCache,
};
use crate::launch::AppState;
use crate::play::{CurrentLevel, Player, Screen, ScreenOpen};
use crate::props::{Container, PropPiece};
use crate::render::{OutlineMaterial, ToonMaterial};

/// How often the soak logs its counts, seconds.
pub const REPORT_SECONDS: f32 = 10.0;

/// The autopilot, when `FB_SOAK` gives a number of seconds.
pub struct SoakPlugin {
    pub seconds: f32,
    /// Forget every icon at each report (`FB_SOAK_CHURN`).
    pub churn_icons: bool,
}

impl SoakPlugin {
    pub fn from_env() -> Option<SoakPlugin> {
        let seconds = parse_seconds(&std::env::var("FB_SOAK").ok()?)?;
        let churn_icons = crate::launch::env_set("FB_SOAK_CHURN");
        Some(SoakPlugin {
            seconds,
            churn_icons,
        })
    }
}

/// `FB_SOAK`'s value as a positive number of seconds.
pub fn parse_seconds(value: &str) -> Option<f32> {
    value
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|s| s.is_finite() && *s > 0.0)
}

impl Plugin for SoakPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        if !app.is_plugin_added::<EntityCountDiagnosticsPlugin>() {
            app.add_plugins(EntityCountDiagnosticsPlugin::default());
        }
        let seed = std::env::var("FB_SOAK_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        app.insert_resource(Soak::new(self.seconds, self.churn_icons, seed))
            .init_resource::<RenderCounts>()
            .add_systems(
                PreUpdate,
                (
                    start_from_title.run_if(in_state(AppState::Title)),
                    drive.run_if(in_state(AppState::Delving)),
                )
                    .after(InputSystems)
                    .before(crate::keymap::HoldSet),
            )
            .add_systems(Update, act.run_if(in_state(AppState::Delving)))
            .add_systems(Last, report);
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(Render, count_render_phases.in_set(RenderSystems::Cleanup));
        }
    }
}

/// The autopilot's state.
#[derive(Resource)]
pub struct Soak {
    seconds: f32,
    churn_icons: bool,
    rng: Mulberry32,
    /// Cells still to walk, nearest first.
    path: Vec<(i32, i32)>,
    /// Where it stood when it last made progress, and when.
    progress: (Vec2, f32),
    /// Keys pressed last frame for one frame, to let go now.
    tapped: Vec<KeyCode>,
    mouse_tapped: bool,
    /// When it next swings, opens a screen, closes one, equips and drops.
    next_swing: f32,
    next_screen: f32,
    close_screen: f32,
    next_shuffle: f32,
    next_report: f32,
    started: Option<f32>,
    /// When the title last showed, until a new delve is begun from it.
    on_title_since: Option<f32>,
    titles: u32,
    /// Items it set down lately, and when, left alone for a while so it does not pick each
    /// straight back up.
    dropped: Vec<(crate::inventory::ItemId, f32)>,
    pub stats: SoakStats,
}

/// What the soak has done, for the log.
#[derive(Clone, Copy, Debug, Default)]
pub struct SoakStats {
    pub swings: u32,
    pub screens: u32,
    pub picked: u32,
    pub dropped: u32,
    pub equipped: u32,
    pub opened: u32,
    pub paths: u32,
}

impl Soak {
    fn new(seconds: f32, churn_icons: bool, seed: u32) -> Self {
        Soak {
            seconds,
            churn_icons,
            rng: Mulberry32::new(seed),
            path: Vec::new(),
            progress: (Vec2::ZERO, 0.0),
            tapped: Vec::new(),
            mouse_tapped: false,
            next_swing: 0.0,
            next_screen: 8.0,
            close_screen: f32::MAX,
            next_shuffle: 5.0,
            next_report: 0.0,
            started: None,
            on_title_since: None,
            titles: 0,
            dropped: Vec::new(),
            stats: SoakStats::default(),
        }
    }

    fn tap(&mut self, keys: &mut ButtonInput<KeyCode>, key: KeyCode) {
        keys.press(key);
        self.tapped.push(key);
    }
}

/// How long the title shows before the soak begins a new delve, seconds.
const TITLE_SECONDS: f32 = 1.0;

/// On the title, Enter begins a new delve, once the title has shown a while and its entities
/// are logged.
fn start_from_title(
    mut soak: ResMut<Soak>,
    keys: Option<ResMut<ButtonInput<KeyCode>>>,
    time: Res<Time<Real>>,
    entities: Query<()>,
    resources: Query<(), With<IsResource>>,
    sounds: Query<(), With<AudioPlayer>>,
) {
    let Some(mut keys) = keys else { return };
    for key in std::mem::take(&mut soak.tapped) {
        keys.release(key);
    }
    let now = time.elapsed_secs();
    let since = *soak.on_title_since.get_or_insert(now);
    if now - since > TITLE_SECONDS && now > 2.0 {
        soak.titles += 1;
        // Bevy 0.20 keeps each resource on an entity, and keeps it when the resource is removed.
        eprintln!(
            "soak: title {} t={now:.0}s entities={} resources={} sounds={}",
            soak.titles,
            entities.iter().count(),
            resources.iter().count(),
            sounds.iter().count()
        );
        soak.on_title_since = None;
        soak.tap(&mut keys, KeyCode::Enter);
    }
}

/// The cell world point `p` is in.
fn cell_of(p: Vec2) -> (i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

/// The centre of cell `c` on the floor plane.
fn centre_of(c: (i32, i32)) -> Vec2 {
    Vec2::new((c.0 as f32 + 0.5) * CELL, (c.1 as f32 + 0.5) * CELL)
}

/// The yaw facing from `from` toward `to` on the floor (the player faces -Z at yaw 0).
fn yaw_toward(from: Vec2, to: Vec2) -> f32 {
    let d = to - from;
    (-d.x).atan2(-d.y)
}

/// A creature still standing.
type LivingCreature = (With<Critter>, Without<Corpse>, Without<Player>);

/// Walks, turns, swings and opens and closes screens through the keyboard and mouse.
#[allow(clippy::too_many_arguments)]
fn drive(
    mut soak: ResMut<Soak>,
    time: Res<Time<Real>>,
    level: Res<CurrentLevel>,
    open: Res<ScreenOpen>,
    settings: Res<crate::settings::GameSettings>,
    player: Query<&Transform, With<Player>>,
    creatures: Query<&Transform, LivingCreature>,
    containers: Query<(&GlobalTransform, &Container)>,
    keys: Option<ResMut<ButtonInput<KeyCode>>>,
    mouse: Option<ResMut<ButtonInput<MouseButton>>>,
    mut motion: ResMut<AccumulatedMouseMotion>,
) {
    let (Some(mut keys), Some(mut mouse)) = (keys, mouse) else {
        return;
    };
    let now = time.elapsed_secs();
    let soak = &mut *soak;
    for key in std::mem::take(&mut soak.tapped) {
        keys.release(key);
    }
    if std::mem::take(&mut soak.mouse_tapped) {
        mouse.release(MouseButton::Left);
    }
    let Ok(body) = player.single() else { return };
    let at = body.translation.xz();

    // Screens: open one now and then, close it a few seconds later.
    if open.0.is_some() {
        keys.release(KeyCode::KeyW);
        if now >= soak.close_screen {
            let key = if open.is(Screen::Inventory) {
                KeyCode::KeyI
            } else {
                KeyCode::Escape
            };
            soak.tap(&mut keys, key);
            soak.close_screen = f32::MAX;
            soak.next_screen = now + soak.rng.range_f32(6.0, 14.0);
        } else if soak.close_screen == f32::MAX {
            // Opened by something else (a container): close it in a while.
            soak.close_screen = now + soak.rng.range_f32(1.0, 3.0);
        }
        return;
    }
    if now >= soak.next_screen {
        let key = if soak.rng.chance(0.75) {
            KeyCode::KeyI
        } else {
            KeyCode::Escape
        };
        soak.tap(&mut keys, key);
        soak.stats.screens += 1;
        soak.close_screen = now + soak.rng.range_f32(1.0, 4.0);
        return;
    }

    // A creature in sight: turn on it, close in and swing.
    let blocked = |x: i32, y: i32| !level.0.is_open(x, y);
    let foe = creatures
        .iter()
        .map(|t| t.translation.xz())
        .filter(|p| {
            p.distance(at) < 60.0
                && line_of_sight(
                    (at.x / CELL, at.y / CELL),
                    (p.x / CELL, p.y / CELL),
                    blocked,
                )
        })
        .min_by(|a, b| a.distance(at).total_cmp(&b.distance(at)));
    let goal = match foe {
        Some(p) => Some(p),
        None => {
            if soak
                .path
                .first()
                .is_some_and(|&c| centre_of(c).distance(at) < CELL * 0.4)
            {
                soak.path.remove(0);
            }
            // Stuck, or at the end of the way: a new way to a random open cell.
            let stuck = now - soak.progress.1 > 4.0;
            if soak.path.is_empty() || stuck {
                // Now and then toward a creature or a container not yet opened, else anywhere.
                let roll = soak.rng.next_f64();
                let toward: Vec<Vec2> = if roll < 0.4 {
                    creatures.iter().map(|t| t.translation.xz()).collect()
                } else if roll < 0.6 {
                    containers
                        .iter()
                        .filter(|(_, c)| !c.opened)
                        .map(|(t, _)| t.translation().xz())
                        .collect()
                } else {
                    Vec::new()
                };
                let target = (!toward.is_empty()).then(|| cell_of(*soak.rng.pick(&toward)));
                soak.path = target
                    .and_then(|to| find_path(cell_of(at), to, 20_000, blocked))
                    .or_else(|| random_path(&mut soak.rng, &level.0, cell_of(at)))
                    .unwrap_or_default();
                soak.progress = (at, now);
                soak.stats.paths += 1;
            }
            soak.path.first().map(|&c| centre_of(c))
        }
    };
    if at.distance(soak.progress.0) > CELL * 0.5 {
        soak.progress = (at, now);
    }
    let Some(goal) = goal else {
        keys.release(KeyCode::KeyW);
        return;
    };
    // Turn with the mouse as `crate::player::look` reads it.
    let yaw = body.rotation.to_euler(EulerRot::YXZ).0;
    let turn = (yaw_toward(at, goal) - yaw + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    let per_pixel = crate::player::look_turn(Vec2::X, &settings).x;
    if per_pixel.abs() > 0.0 {
        motion.delta = Vec2::new(turn.clamp(-0.25, 0.25) / per_pixel, 0.0);
    }
    let near_foe = foe.is_some_and(|p| p.distance(at) < 9.0);
    if near_foe || turn.abs() > 1.0 {
        keys.release(KeyCode::KeyW);
    } else {
        keys.press(KeyCode::KeyW);
    }
    if foe.is_some_and(|p| p.distance(at) < 16.0) && turn.abs() < 0.4 && now >= soak.next_swing {
        mouse.press(MouseButton::Left);
        soak.mouse_tapped = true;
        soak.next_swing = now + soak.rng.range_f32(0.5, 1.0);
        soak.stats.swings += 1;
    }
}

/// A way from `from` to a random open cell, if one is found within a few tries.
fn random_path(
    rng: &mut Mulberry32,
    level: &fb_level::Level,
    from: (i32, i32),
) -> Option<Vec<(i32, i32)>> {
    let blocked = |x: i32, y: i32| !level.is_open(x, y);
    for _ in 0..20 {
        let to = (
            rng.int(0, level.width - 1) as i32,
            rng.int(0, level.height - 1) as i32,
        );
        if blocked(to.0, to.1) || to == from {
            continue;
        }
        if let Some(path) = find_path(from, to, 20_000, blocked) {
            return Some(path);
        }
    }
    None
}

/// Picks up what lies in reach, opens containers in reach, and now and then equips or drops
/// something carried, through the inventory's own rules.
#[allow(clippy::too_many_arguments)]
fn act(
    mut commands: Commands,
    mut soak: ResMut<Soak>,
    time: Res<Time<Real>>,
    player: Query<&Transform, With<Player>>,
    floor: Query<(&GlobalTransform, &FloorItem)>,
    containers: Query<(Entity, &GlobalTransform, &PropPiece, &Container)>,
    mut inventory: ResMut<Inventory>,
    mut changes: ResMut<FloorChanges>,
    mut screen: ResMut<ScreenOpen>,
    mut showing: ResMut<OpenContainer>,
    level: Res<CurrentLevel>,
    tuning: Option<Res<crate::tuning::Tuning>>,
    mut spawner: crate::render::ItemSpawner,
) {
    if screen.0.is_some() {
        return;
    }
    let Ok(body) = player.single() else { return };
    let at = body.translation;
    let now = time.elapsed_secs();
    let soak = &mut *soak;
    soak.dropped.retain(|(_, when)| now - when < 60.0);
    let recent: Vec<_> = soak.dropped.iter().map(|(id, _)| *id).collect();
    if let Some((_, item)) = floor
        .iter()
        .find(|(t, f)| !recent.contains(&f.id) && t.translation().distance(at) < REACH * 0.6)
        && let Ok(change) = crate::inventory::quick(&mut inventory, item.id)
    {
        changes.0.extend(change);
        soak.stats.picked += 1;
    }
    if let Some((entity, _, piece, &container)) = containers
        .iter()
        .find(|(_, t, _, c)| !c.opened && t.translation().distance(at) < REACH * 0.8)
    {
        crate::props::open_container(
            &mut commands,
            &mut spawner,
            &level.0,
            entity,
            piece,
            container,
            &mut screen,
            &mut showing,
            None,
            &tuning.as_deref().cloned().unwrap_or_default(),
        );
        soak.stats.opened += 1;
        return;
    }
    if now < soak.next_shuffle {
        return;
    }
    soak.next_shuffle = now + soak.rng.range_f32(2.0, 5.0);
    let carried: Vec<_> = inventory
        .ids()
        .filter(|&id| match inventory.location(id) {
            Some(Location::In { .. }) => inventory.is_carried(id),
            // Worn things too, but not the pack everything rides in.
            Some(Location::Worn(slot)) => slot != fb_items::Slot::Back,
            _ => false,
        })
        .collect();
    if carried.is_empty() {
        return;
    }
    let id = *soak.rng.pick(&carried);
    let wearable = inventory.item(id).is_some_and(|i| i.profile.slot.is_some());
    let worn = matches!(inventory.location(id), Some(Location::Worn(_)));
    if worn || (wearable && soak.rng.chance(0.6)) {
        if let Ok(change) = crate::inventory::quick(&mut inventory, id) {
            changes.0.extend(change);
            soak.stats.equipped += 1;
        }
    } else if inventory.contents_count(id) == 0 {
        let Some(size) = inventory.footprint(id) else {
            return;
        };
        let drag = Drag {
            id,
            grab: IVec2::ZERO,
            size,
            turned: false,
            press: Vec2::ZERO,
            active: true,
            target: None,
        };
        if let Ok(change) = crate::inventory::drop_at(&mut inventory, &drag, Target::Level, 4) {
            changes.0.extend(change);
            soak.dropped.push((id, now));
            soak.stats.dropped += 1;
        }
    }
}

/// Render-world counts, copied out each frame for [`report`].
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct RenderCounts {
    pub opaque_phases: usize,
    pub shadow_phases: usize,
    /// The render world's GPU preprocessing mode, by name; empty until it is read.
    pub preprocessing: &'static str,
}

/// The render world's main-world copy of [`RenderCounts`], shared through an atomic since the
/// render world runs apart.
static OPAQUE_PHASES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static SHADOW_PHASES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// The render world's [`GpuPreprocessingMode`]: 0 none read yet, then `None`, `PreprocessingOnly`
/// and `Culling` as 1, 2 and 3.
static PREPROCESSING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

const PREPROCESSING_MODES: [(GpuPreprocessingMode, &str); 3] = [
    (GpuPreprocessingMode::None, "None"),
    (GpuPreprocessingMode::PreprocessingOnly, "PreprocessingOnly"),
    (GpuPreprocessingMode::Culling, "Culling"),
];

fn count_render_phases(
    opaque: Option<Res<ViewBinnedRenderPhases<Opaque3d>>>,
    shadow: Option<Res<ViewBinnedRenderPhases<Shadow>>>,
    support: Option<Res<GpuPreprocessingSupport>>,
) {
    use std::sync::atomic::Ordering::Relaxed;
    OPAQUE_PHASES.store(opaque.map_or(0, |p| p.len()), Relaxed);
    SHADOW_PHASES.store(shadow.map_or(0, |p| p.len()), Relaxed);
    let mode = support.and_then(|s| {
        PREPROCESSING_MODES
            .iter()
            .position(|&(m, _)| m == s.max_supported_mode)
    });
    PREPROCESSING.store(mode.map_or(0, |i| i + 1), Relaxed);
}

/// Logs the counts every [`REPORT_SECONDS`], and quits once the soak's time is up.
#[allow(clippy::too_many_arguments)]
fn report(
    mut soak: ResMut<Soak>,
    time: Res<Time<Real>>,
    entities: Query<()>,
    meshes: Res<Assets<Mesh>>,
    images: Res<Assets<Image>>,
    toon: Res<Assets<ToonMaterial>>,
    standard: Res<Assets<StandardMaterial>>,
    outline: Res<Assets<OutlineMaterial>>,
    cameras: Query<(Entity, &Camera, Option<&Name>)>,
    readbacks: Query<(), With<Readback>>,
    icons: Option<ResMut<IconCache>>,
    store: Res<DiagnosticsStore>,
    mut render_counts: ResMut<RenderCounts>,
    mut exit: MessageWriter<AppExit>,
) {
    use std::sync::atomic::Ordering::Relaxed;
    let now = time.elapsed_secs();
    let started = *soak.started.get_or_insert(now);
    if now < soak.next_report {
        return;
    }
    soak.next_report = now + REPORT_SECONDS;
    *render_counts = RenderCounts {
        opaque_phases: OPAQUE_PHASES.load(Relaxed),
        shadow_phases: SHADOW_PHASES.load(Relaxed),
        preprocessing: PREPROCESSING
            .load(Relaxed)
            .checked_sub(1)
            .map_or("", |i| PREPROCESSING_MODES[i].1),
    };
    let frame_ms = store
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let active = cameras.iter().filter(|(_, c, _)| c.is_active).count();
    let (icon_count, icon_drawing) = icons
        .as_deref()
        .map_or((0, 0), |c| (c.len(), c.drawing_count()));
    eprintln!(
        "soak: t={:.0}s entities={} meshes={} images={} toon={} standard={} outline={} cameras={} active_cameras={} readbacks={} icons={} icons_drawing={} opaque_phases={} shadow_phases={} preprocessing={} frame_ms={:.1} {:?}",
        now - started,
        entities.iter().count(),
        meshes.len(),
        images.len(),
        toon.len(),
        standard.len(),
        outline.len(),
        cameras.iter().count(),
        active,
        readbacks.iter().count(),
        icon_count,
        icon_drawing,
        render_counts.opaque_phases,
        render_counts.shadow_phases,
        render_counts.preprocessing,
        frame_ms,
        soak.stats,
    );
    let named: Vec<String> = cameras
        .iter()
        .filter(|(_, c, _)| c.is_active)
        .map(|(e, _, n)| format!("{e}:{}", n.map_or("", |n| n.as_str())))
        .collect();
    eprintln!("soak: active cameras {}", named.join(", "));
    if soak.churn_icons
        && let Some(mut icons) = icons
    {
        icons.forget_drawn();
    }
    if now - started >= soak.seconds {
        eprintln!("soak: done after {:.0}s", now - started);
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soak_seconds_parse() {
        assert_eq!(parse_seconds("1200"), Some(1200.0));
        assert_eq!(parse_seconds(" 30 "), Some(30.0));
        assert_eq!(parse_seconds("0"), None);
        assert_eq!(parse_seconds("x"), None);
    }

    #[test]
    fn yaw_faces_the_goal() {
        let at = Vec2::ZERO;
        for goal in [
            Vec2::new(0.0, -5.0),
            Vec2::new(3.0, 1.0),
            Vec2::new(-2.0, 7.0),
        ] {
            let forward = Quat::from_rotation_y(yaw_toward(at, goal)) * Vec3::NEG_Z;
            assert!(
                forward.xz().normalize().abs_diff_eq(goal.normalize(), 1e-5),
                "{goal}"
            );
        }
    }

    #[test]
    fn random_paths_run_over_open_floor() {
        let level = fb_level::generate(1207, 1);
        let (x, y, _) = level.start();
        let path =
            random_path(&mut Mulberry32::new(3), &level, (x as i32, y as i32)).expect("a way");
        assert!(path.iter().all(|&(x, y)| level.is_open(x, y)));
    }
}
