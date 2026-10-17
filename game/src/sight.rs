//! The edge of the view closing in while an effect takes the delver's sight (stinging eyes,
//! dazzle): a soft shade round the view, reaching further in and darker the less they see
//! ([`fb_character::effect::Effects::sight`]). Our own way of showing it in first person.
//!
//! The shade is one of a few radial gradients drawn once ([`STEPS`], each reaching further in),
//! stretched over the view and tinted to its darkness, so it costs nothing while sight is whole.

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::carry::{Hud, ViewModelCamera};
use crate::character::DelverEffects;
use crate::play::{DelveSystems, Player};

/// How many shades there are, each reaching further in than the last.
pub const STEPS: usize = 8;
/// How far in the deepest shade begins to fall off, as a share of the way from the edge to
/// the centre.
const DEEPEST: f32 = 0.7;
/// The shade's darkness with no sight at all.
const DARKEST: f32 = 0.92;
/// The shade's colour: a smoky near-black.
const SHADE: Color = Color::srgb(0.05, 0.04, 0.03);
/// The gradients' size in pixels; stretched over the view, so small is enough.
const SIZE: (u32, u32) = (160, 90);

pub struct SightPlugin;

impl Plugin for SightPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_veil, close_in).chain().in_set(DelveSystems));
    }
}

/// The pale wash of glare over a view, while dazzled.
#[derive(Component, Debug)]
struct Glare;

/// The glare's colour: a warm near-white.
const GLARE: Color = Color::srgb(1.0, 0.96, 0.86);

/// The shade over a view, with its gradients, shallowest first. They live on the veil, so
/// they go with it when the delve ends.
#[derive(Component, Debug)]
struct Veil(Vec<Handle<Image>>);

/// Which shade to show and how dark, when the delver sees `sight` of their usual distance:
/// `None` while it is whole.
pub fn veil(sight: f64) -> Option<(usize, f32)> {
    let lost = (1.0 - sight).clamp(0.0, 1.0) as f32;
    (lost > 1e-3).then(|| (((lost * STEPS as f32).ceil() as usize).clamp(1, STEPS) - 1, DARKEST * lost.sqrt()))
}

/// The shade's opacity at `(x, y)` (each -1 to 1 across the view) for gradient `step`: clear in
/// the middle, falling smoothly to full at the edge, beginning further in for a deeper step.
pub fn shade_at(x: f32, y: f32, step: usize) -> f32 {
    let reach = DEEPEST * (step + 1) as f32 / STEPS as f32;
    // A rounded rectangle: 1 at the middle of each edge, a little more at the corners.
    let out = (x.powi(4) + y.powi(4)).powf(0.25);
    let from = 1.0 - reach;
    let t = ((out - from) / reach).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn shade_image(step: usize) -> Image {
    let (w, h) = SIZE;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for j in 0..h {
        for i in 0..w {
            let (x, y) = ((i as f32 + 0.5) / w as f32 * 2.0 - 1.0, (j as f32 + 0.5) / h as f32 * 2.0 - 1.0);
            rgba.extend_from_slice(&[255, 255, 255, (shade_at(x, y, step) * 255.0).round() as u8]);
        }
    }
    Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

/// Draws the gradients once and lays a shade over each new view.
fn spawn_veil(mut commands: Commands, new: Query<Entity, Added<ViewModelCamera>>, mut images: ResMut<Assets<Image>>) {
    for camera in &new {
        let handles: Vec<Handle<Image>> = (0..STEPS).map(|step| images.add(shade_image(step))).collect();
        commands.spawn((
            Name::new("Sight veil"),
            Hud,
            UiTargetCamera(camera),
            Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() },
            ImageNode { image: handles[0].clone(), color: SHADE.with_alpha(0.0), ..default() },
            Visibility::Hidden,
            Pickable::IGNORE,
            // Under the rest of the HUD, so the orbs and badges stay readable through it.
            GlobalZIndex(-1),
            Veil(handles),
        ));
        commands.spawn((
            Name::new("Glare"),
            Glare,
            Hud,
            UiTargetCamera(camera),
            Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() },
            BackgroundColor(GLARE.with_alpha(0.0)),
            Pickable::IGNORE,
            GlobalZIndex(-1),
        ));
    }
}

/// Picks the shade and its darkness for the delver's sight now.
fn close_in(rules: Option<Res<crate::rules::GameRules>>, players: Query<Option<&DelverEffects>, With<Player>>, mut veils: Query<(&Veil, &mut ImageNode, &mut Visibility)>, mut glares: Query<&mut BackgroundColor, With<Glare>>) {
    let Some(effects) = players.iter().next() else { return };
    let defs = &crate::rules::GameRules::of(&rules).effects;
    let sight = effects.map_or(1.0, |e| e.0.sight(defs));
    let glare = effects.map_or(0.0, |e| e.0.glare(defs)) as f32;
    for mut wash in &mut glares {
        wash.set_if_neq(BackgroundColor(GLARE.with_alpha(glare)));
    }
    let shown = veil(sight);
    for (shades, mut node, mut visibility) in &mut veils {
        visibility.set_if_neq(if shown.is_some() { Visibility::Inherited } else { Visibility::Hidden });
        if let Some((step, dark)) = shown {
            if node.image != shades.0[step] {
                node.image = shades.0[step].clone();
            }
            // Glare whitens the edge as darkness shades it: dazzled eyes see a bright rim.
            let colour = if glare > 0.0 { GLARE.with_alpha(dark) } else { SHADE.with_alpha(dark) };
            if node.color != colour {
                node.color = colour;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_sight_shows_nothing_and_less_sight_reaches_further_in_and_darker() {
        assert_eq!(veil(1.0), None);
        let (some, more) = (veil(0.8).unwrap(), veil(0.4).unwrap());
        assert!(more.0 > some.0 && more.1 > some.1, "{some:?} then {more:?}");
        assert_eq!(veil(0.0).unwrap().0, STEPS - 1);
    }

    #[test]
    fn a_shade_is_clear_in_the_middle_full_at_the_corners_and_deeper_steps_reach_further_in() {
        for step in 0..STEPS {
            assert_eq!(shade_at(0.0, 0.0, step), 0.0, "clear in the middle");
            assert!(shade_at(1.0, 1.0, step) > 0.99, "full at the corners");
        }
        assert!(shade_at(0.6, 0.0, STEPS - 1) > shade_at(0.6, 0.0, 0), "the deepest reaches further in");
    }
}
