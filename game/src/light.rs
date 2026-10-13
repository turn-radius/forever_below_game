//! Carried and wall-mounted light. In the abyss this is the main light (`docs/rendering.md`,
//! "Light"), so it lives apart from any one scene.

use bevy::prelude::*;

/// Flickers every [`Torch`].
pub struct LightPlugin;

impl Plugin for LightPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, flicker_torches);
    }
}

/// A flickering warm point light. Spawn with a [`PointLight`]; its intensity is driven from
/// `intensity` here every frame.
#[derive(Component, Clone, Copy, Debug)]
#[require(PointLight)]
pub struct Torch {
    /// Intensity before flicker, in lumens.
    pub intensity: f32,
}

impl Torch {
    /// The torch colour, `#ffb36a`.
    pub const COLOR: Color = Color::srgb(1.0, 0xb3 as f32 / 255.0, 0x6a as f32 / 255.0);

    /// The prototype's flicker multiplier at time `t` seconds: around 0.88, within 0.72..1.04.
    pub fn flicker(t: f32) -> f32 {
        0.88 + 0.08 * (13.1 * t).sin() + 0.05 * (7.3 * t + 1.2).sin() + 0.03 * (23.7 * t).sin()
    }

    /// A torch's point light at `intensity` lumens reaching `range` units.
    pub fn light(intensity: f32, range: f32) -> (Torch, PointLight) {
        (Torch { intensity }, PointLight { color: Torch::COLOR, intensity, range, ..default() })
    }
}

fn flicker_torches(time: Res<Time>, mut torches: Query<(&Torch, &mut PointLight)>) {
    let flicker = Torch::flicker(time.elapsed_secs());
    for (torch, mut light) in &mut torches {
        light.intensity = torch.intensity * flicker;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flicker_stays_in_band() {
        for i in 0..10_000 {
            let f = Torch::flicker(i as f32 * 0.01);
            assert!((0.72..=1.04).contains(&f), "{f}");
        }
    }
}
