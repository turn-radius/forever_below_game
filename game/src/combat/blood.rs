//! Blood round the edge of the view through the last gasp: splotches that bloom in from every
//! side and blink, so a delver at 0 hit points sees it and reaches for a draught. Drawn by one
//! shader (`blood.wesl`) over the whole view; how strong and how far in it is comes from
//! [`GaspBlood`], which [`super::vitals`] sets each frame. Our own.

use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::{Shader, ShaderRef, load_shader_library},
};

use super::vitals::VitalsRoot;
use crate::play::DelveSystems;

const SHADER_PATH: &str = "embedded://forever_below/combat/blood.wesl";

/// The blood's thin red at a splotch's edge: dark, not the interface's bright red. Our own.
const BLOOD: Color = Color::srgb(0.32, 0.02, 0.02);
/// The near black where it pools. Our own.
const BLOOD_DEEP: Color = Color::srgb(0.1, 0.0, 0.01);

pub struct BloodPlugin;

impl Plugin for BloodPlugin {
    fn build(&self, app: &mut App) {
        if app.world().contains_resource::<Assets<Shader>>() {
            load_shader_library!(app, "blood.wesl");
            app.add_plugins(UiMaterialPlugin::<BloodMaterial>::default());
        } else if app.world().contains_resource::<AssetServer>() {
            app.init_asset::<BloodMaterial>();
        }
        app.init_resource::<GaspBlood>().add_systems(
            Update,
            (spawn_blood, show_blood).chain().in_set(DelveSystems),
        );
    }
}

/// How the blood shows now: `strength` 0 (none) to 1, and how far in it reaches, in screen
/// heights.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct GaspBlood {
    pub strength: f32,
    pub spread: f32,
}

/// The blood over the view.
#[derive(Component, Debug)]
struct Blood;

/// How the blood is drawn (`blood.wesl`).
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct BloodMaterial {
    #[uniform(0)]
    pub style: BloodStyle,
}

#[derive(ShaderType, Clone, Copy, Debug, PartialEq)]
pub struct BloodStyle {
    pub colour: LinearRgba,
    pub deep: LinearRgba,
    pub spread: f32,
    pub strength: f32,
    pub seed: f32,
}

impl UiMaterial for BloodMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }
}

/// Lays the blood over each new view, hidden until a gasp: the HUD's first child and below its
/// siblings, so the orbs and the quickslots stay readable over it.
fn spawn_blood(
    mut commands: Commands,
    roots: Query<Entity, Added<VitalsRoot>>,
    materials: Option<ResMut<Assets<BloodMaterial>>>,
) {
    let Some(mut materials) = materials else {
        return;
    };
    for root in &roots {
        let blood = commands
            .spawn((
                Name::new("Blood"),
                Blood,
                MaterialNode(materials.add(BloodMaterial {
                    style: BloodStyle {
                        colour: BLOOD.into(),
                        deep: BLOOD_DEEP.into(),
                        spread: 0.0,
                        strength: 0.0,
                        seed: 0.0,
                    },
                })),
                Node {
                    width: percent(100),
                    height: percent(100),
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ZIndex(-1),
                Visibility::Hidden,
                Pickable::IGNORE,
            ))
            .id();
        commands.entity(root).insert_child(0, blood);
    }
}

/// Writes [`GaspBlood`] into the shader, only when it changes, and hides the node without it.
/// Each gasp lays its blood out afresh.
fn show_blood(
    gasp: Res<GaspBlood>,
    mut blood: Query<(&MaterialNode<BloodMaterial>, &mut Visibility), With<Blood>>,
    materials: Option<ResMut<Assets<BloodMaterial>>>,
    mut gasps: Local<f32>,
    mut was: Local<bool>,
) {
    let Some(mut materials) = materials else {
        return;
    };
    if !gasp.is_changed() {
        return;
    }
    let showing = gasp.strength > 0.0;
    if showing && !*was {
        *gasps += 1.0;
    }
    *was = showing;
    for (node, mut visibility) in &mut blood {
        visibility.set_if_neq(if showing {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if let Some(mut material) = materials.get_mut(&node.0) {
            material.style.strength = gasp.strength;
            material.style.spread = gasp.spread;
            material.style.seed = *gasps;
        }
    }
}
