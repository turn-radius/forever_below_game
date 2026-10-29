//! How long something has left, shown on a UI node as a grey veil that sweeps clockwise from
//! twelve o'clock over the time already gone ([`TimeLeft`], drawn by `time_left.wesl`). Full
//! time left shows nothing; as it runs down the veil grows round until the node is covered.
//! Any slot or button can carry one: a quickslot holding Divine Light shows the prayer's time
//! left this way (`crate::casting`), and other spells and effects can do the same.

use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::{Shader, ShaderRef, load_shader_library},
};

use crate::play::DelveSystems;

const SHADER_PATH: &str = "embedded://forever_below/time_left.wesl";

/// The veil's colour over what is spent, and the bright line along its sweeping edge.
const VEIL: Color = Color::srgba(0.04, 0.04, 0.05, 0.8);
const SWEEP: Color = Color::srgba(1.0, 0.93, 0.7, 0.85);

pub struct TimeLeftPlugin;

impl Plugin for TimeLeftPlugin {
    fn build(&self, app: &mut App) {
        // Without a renderer (the logic tests) there is no shader to load, only the material
        // asset for the systems to write.
        if app.world().contains_resource::<Assets<Shader>>() {
            load_shader_library!(app, "time_left.wesl");
            app.add_plugins(UiMaterialPlugin::<TimeLeftMaterial>::default());
        } else if app.world().contains_resource::<AssetServer>() {
            app.init_asset::<TimeLeftMaterial>();
        }
        app.add_systems(Update, (veil_new, sweep_veils).chain().in_set(DelveSystems));
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

/// How far round the veil has swept, 0 to 1 clockwise from twelve o'clock, for `left` of the
/// time remaining: none at full time, the whole way round at none.
pub fn swept(left: f32) -> f32 {
    (1.0 - left).clamp(0.0, 1.0)
}

/// Gives each new [`TimeLeft`] node its veil.
fn veil_new(mut commands: Commands, new: Query<Entity, Added<TimeLeft>>, materials: Option<ResMut<Assets<TimeLeftMaterial>>>) {
    let Some(mut materials) = materials else { return };
    for node in &new {
        let style = VeilStyle { veil: VEIL.into(), sweep: SWEEP.into(), left: 1.0 };
        commands.entity(node).with_child((
            Veil,
            MaterialNode(materials.add(TimeLeftMaterial { style })),
            Node { position_type: PositionType::Absolute, left: px(0), right: px(0), top: px(0), bottom: px(0), ..default() },
            Visibility::Hidden,
            Pickable::IGNORE,
        ));
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
}
