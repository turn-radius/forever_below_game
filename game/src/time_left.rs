//! How long something has left, shown on a UI node as a grey veil that sweeps clockwise from
//! twelve o'clock over the time already gone ([`TimeLeft`], drawn by `time_left.wesl`). Full
//! time left shows nothing; as it runs down the veil grows round until the node is covered.
//! Any slot or button can carry one: a quickslot holding Divine Light shows the prayer's time
//! left this way (`crate::casting`), and other spells and effects can do the same. When the
//! time runs out the node is ready again, and a highlight chases twice round its border,
//! clockwise from the top-left corner (`ready_chase.wesl`).

use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::{Shader, ShaderRef, load_shader_library},
};

use crate::play::DelveSystems;

const SHADER_PATH: &str = "embedded://forever_below/time_left.wesl";
const CHASE_PATH: &str = "embedded://forever_below/ready_chase.wesl";

/// The veil's colour over what is spent, and the bright line along its sweeping edge.
const VEIL: Color = Color::srgba(0.04, 0.04, 0.05, 0.8);
const SWEEP: Color = Color::srgba(1.0, 0.93, 0.7, 0.85);
/// The ready chase's gold, how long it runs in seconds, how many laps it makes in that time,
/// and how far it stands out round the node, pixels.
const CHASE_GLOW: Color = Color::srgb(1.0, 0.82, 0.4);
pub const READY_SECONDS: f32 = 1.4;
const READY_LAPS: f32 = 2.0;
const CHASE_OUT: f32 = 3.0;

pub struct TimeLeftPlugin;

impl Plugin for TimeLeftPlugin {
    fn build(&self, app: &mut App) {
        // Without a renderer (the logic tests) there is no shader to load, only the material
        // asset for the systems to write.
        if app.world().contains_resource::<Assets<Shader>>() {
            load_shader_library!(app, "time_left.wesl");
            load_shader_library!(app, "ready_chase.wesl");
            app.add_plugins((UiMaterialPlugin::<TimeLeftMaterial>::default(), UiMaterialPlugin::<ChaseMaterial>::default()));
        } else if app.world().contains_resource::<AssetServer>() {
            app.init_asset::<TimeLeftMaterial>().init_asset::<ChaseMaterial>();
        }
        app.add_systems(Update, (veil_new, sweep_veils, start_chases, run_chases).chain().in_set(DelveSystems));
    }
}

/// The share of its time something has left, 0 to 1, on a node to veil by it. `None` shows
/// nothing, as for something not running at all.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct TimeLeft(pub Option<f32>);

/// The veil drawn over a [`TimeLeft`] node: a child filling it, above its other children.
#[derive(Component, Clone, Copy, Debug)]
struct Veil;

/// How the veil is drawn (`time_left.wesl`).
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct TimeLeftMaterial {
    #[uniform(0)]
    pub style: VeilStyle,
}

#[derive(ShaderType, Clone, Copy, Debug, PartialEq)]
pub struct VeilStyle {
    pub veil: LinearRgba,
    pub sweep: LinearRgba,
    /// The share of time left, 0 to 1.
    pub left: f32,
}

impl UiMaterial for TimeLeftMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }
}

/// The ready chase round a [`TimeLeft`] node: a child standing just outside it. How long since
/// the node's time ran out, in seconds (infinite while it is not running), and whether the node
/// was timing something last frame.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ReadyChase {
    pub since: f32,
    running: bool,
}

impl Default for ReadyChase {
    fn default() -> Self {
        ReadyChase { since: f32::INFINITY, running: false }
    }
}

/// How the ready chase is drawn (`ready_chase.wesl`).
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct ChaseMaterial {
    #[uniform(0)]
    pub style: ChaseStyle,
}

#[derive(ShaderType, Clone, Copy, Debug, PartialEq)]
pub struct ChaseStyle {
    pub glow: LinearRgba,
    /// How far round the highlight has run, in laps; below 0 it does not show.
    pub head: f32,
    pub strength: f32,
}

impl UiMaterial for ChaseMaterial {
    fn fragment_shader() -> ShaderRef {
        CHASE_PATH.into()
    }
}

/// Where the ready chase is `t` seconds after the time ran out: how far round, in laps, and
/// how bright, 0 to 1. [`READY_LAPS`] laps clockwise from the top-left corner, the last one
/// fading; `None` before it starts or once it has finished.
pub fn chase_at(t: f32) -> Option<(f32, f32)> {
    if !t.is_finite() || !(0.0..READY_SECONDS).contains(&t) {
        return None;
    }
    let share = t / READY_SECONDS;
    let fade = ((1.0 - share) * READY_LAPS).min(1.0);
    Some((share * READY_LAPS, fade))
}

/// How far round the veil has swept, 0 to 1 clockwise from twelve o'clock, for `left` of the
/// time remaining: none at full time, the whole way round at none.
pub fn swept(left: f32) -> f32 {
    (1.0 - left).clamp(0.0, 1.0)
}

/// Gives each new [`TimeLeft`] node its veil and its ready chase.
fn veil_new(mut commands: Commands, new: Query<Entity, Added<TimeLeft>>, materials: Option<ResMut<Assets<TimeLeftMaterial>>>, chases: Option<ResMut<Assets<ChaseMaterial>>>) {
    let (Some(mut materials), Some(mut chases)) = (materials, chases) else { return };
    for node in &new {
        let style = VeilStyle { veil: VEIL.into(), sweep: SWEEP.into(), left: 1.0 };
        commands.entity(node).with_child((
            Veil,
            MaterialNode(materials.add(TimeLeftMaterial { style })),
            Node { position_type: PositionType::Absolute, left: px(0), right: px(0), top: px(0), bottom: px(0), ..default() },
            Visibility::Hidden,
            Pickable::IGNORE,
        ));
        let chase = ChaseStyle { glow: CHASE_GLOW.into(), head: -1.0, strength: 0.0 };
        commands.entity(node).with_child((
            ReadyChase::default(),
            MaterialNode(chases.add(ChaseMaterial { style: chase })),
            Node { position_type: PositionType::Absolute, left: px(-CHASE_OUT), right: px(-CHASE_OUT), top: px(-CHASE_OUT), bottom: px(-CHASE_OUT), ..default() },
            Visibility::Hidden,
            Pickable::IGNORE,
        ));
    }
}

/// Starts the ready chase on each node whose time has just run out.
fn start_chases(nodes: Query<(&TimeLeft, &Children)>, mut chases: Query<&mut ReadyChase>) {
    for (time, children) in &nodes {
        for &child in children {
            let Ok(mut chase) = chases.get_mut(child) else { continue };
            let running = time.0.is_some();
            if chase.running && !running {
                chase.since = 0.0;
            }
            if running {
                chase.since = f32::INFINITY;
            }
            if chase.running != running {
                chase.running = running;
            }
        }
    }
}

/// Runs each ready chase round its border and hides it when it is done.
fn run_chases(time: Res<Time>, mut chases: Query<(&mut ReadyChase, &MaterialNode<ChaseMaterial>, &mut Visibility)>, mut materials: Option<ResMut<Assets<ChaseMaterial>>>) {
    let dt = time.delta_secs();
    for (mut chase, material, mut shown) in &mut chases {
        if chase.since.is_finite() {
            chase.since += dt;
        }
        let at = chase_at(chase.since);
        let want = if at.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *shown != want {
            *shown = want;
        }
        let Some((head, strength)) = at else {
            chase.since = f32::INFINITY;
            continue;
        };
        if let Some(mut look) = materials.as_mut().and_then(|m| m.get_mut(&material.0)) {
            look.style.head = head;
            look.style.strength = strength;
        }
    }
}

/// Sweeps each veil to its node's time left, hidden while nothing runs.
fn sweep_veils(
    nodes: Query<(&TimeLeft, &Children)>,
    mut veils: Query<(&MaterialNode<TimeLeftMaterial>, &mut Visibility), With<Veil>>,
    mut materials: Option<ResMut<Assets<TimeLeftMaterial>>>,
) {
    for (time, children) in &nodes {
        for &child in children {
            let Ok((material, mut shown)) = veils.get_mut(child) else { continue };
            let want = if time.0.is_some() { Visibility::Inherited } else { Visibility::Hidden };
            if *shown != want {
                *shown = want;
            }
            let Some(left) = time.0 else { continue };
            if let Some(mut look) = materials.as_mut().and_then(|m| m.get_mut(&material.0))
                && look.style.left != left
            {
                look.style.left = left;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_veil_sweeps_round_as_the_time_runs_out() {
        assert_eq!(swept(1.0), 0.0, "nothing covered at full time");
        assert_eq!(swept(0.75), 0.25, "a quarter round, to three o'clock");
        assert_eq!(swept(0.0), 1.0, "the whole way round at none");
        assert_eq!(swept(-0.5), 1.0);
        assert_eq!(swept(1.5), 0.0);
    }

    #[test]
    fn the_ready_chase_runs_twice_round_from_the_top_left_and_fades() {
        assert_eq!(chase_at(f32::INFINITY), None, "nothing while idle");
        assert_eq!(chase_at(0.0), Some((0.0, 1.0)), "it starts at the top-left corner, bright");
        let (head, strength) = chase_at(READY_SECONDS * 0.25).unwrap();
        assert!((head - 0.5).abs() < 1e-5 && strength == 1.0, "half way round the first lap: {head}");
        let (head, strength) = chase_at(READY_SECONDS * 0.75).unwrap();
        assert!((head - 1.5).abs() < 1e-5 && (strength - 0.5).abs() < 1e-5, "fading on the second: {head} {strength}");
        assert_eq!(chase_at(READY_SECONDS), None, "done after two laps");
    }

    #[test]
    fn a_node_whose_time_runs_out_starts_its_chase() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<Assets<TimeLeftMaterial>>().init_resource::<Assets<ChaseMaterial>>();
        app.add_systems(Update, (veil_new, sweep_veils, start_chases, run_chases).chain());
        let node = app.world_mut().spawn((Node::default(), TimeLeft(Some(0.4)))).id();
        app.update();
        app.update();
        let chase = |app: &mut App| *app.world_mut().query::<&ReadyChase>().single(app.world()).unwrap();
        assert!(chase(&mut app).since.is_infinite(), "no chase while it times");
        app.world_mut().entity_mut(node).insert(TimeLeft(None));
        app.update();
        assert!(chase(&mut app).since < READY_SECONDS, "it chases once the time is out");
        let shown = app.world_mut().query_filtered::<&Visibility, With<ReadyChase>>().single(app.world()).unwrap();
        assert_eq!(*shown, Visibility::Inherited);
    }
}
