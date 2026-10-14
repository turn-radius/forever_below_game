//! Light rigs. Carried and wall-mounted torches are the main light in the abyss
//! (`docs/rendering.md`, "Light"); [`StudioLight`] is the even, camera-borne light of an item
//! viewer. Both live apart from any one scene.

use bevy::{camera::Exposure, prelude::*};

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
    /// How deep the flicker goes, 0 (steady) to 1 (the prototype's full curve), as in
    /// `armory::build::light::LightAnchor::flicker`.
    pub flicker_depth: f32,
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
        (Torch { intensity, flicker_depth: 1.0 }, PointLight { color: Torch::COLOR, intensity, range, ..default() })
    }

    /// The intensity multiplier at time `t` seconds: [`Torch::flicker`] scaled toward 1 by
    /// `flicker_depth`.
    pub fn wavering(&self, t: f32) -> f32 {
        1.0 - self.flicker_depth.clamp(0.0, 1.0) * (1.0 - Torch::flicker(t))
    }
}

fn flicker_torches(time: Res<Time>, mut torches: Query<(&Torch, &mut PointLight)>) {
    let t = time.elapsed_secs();
    for (torch, mut light) in &mut torches {
        light.intensity = torch.intensity * torch.wavering(t);
    }
}

/// A key light that rides on the camera plus an ambient fill, for looking at items rather than
/// at a scene. Strengths are in the prototypes' three.js units (r128, legacy lights): the
/// light reaching a lit surface is `ambient + key * band` times its colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StudioLight {
    /// Key strength.
    pub key: f32,
    /// Key direction in camera space: degrees around from straight behind the camera
    /// (negative is to the left) and up from level.
    pub azimuth: f32,
    pub elevation: f32,
    pub ambient: f32,
    pub ambient_color: Color,
}

impl StudioLight {
    /// The Spline Armory rig: key 0.78 at azimuth -40, elevation 42, ambient `#dfe6ee` at
    /// 0.42 (`sun` and `AmbientLight` in `reference/spline-armory.html`).
    pub const ARMORY: StudioLight = StudioLight {
        key: 0.78,
        azimuth: -40.0,
        elevation: 42.0,
        ambient: 0.42,
        ambient_color: Color::srgb(0xdf as f32 / 255.0, 0xe6 as f32 / 255.0, 0xee as f32 / 255.0),
    };

    /// The key as a camera child: spawn it with `ChildOf(camera)`. `exposure` is the camera's,
    /// so the key reads the same under any exposure.
    pub fn key_light(&self, exposure: &Exposure) -> (DirectionalLight, Transform) {
        let (azimuth, elevation) = (self.azimuth.to_radians(), self.elevation.to_radians());
        let from = Vec3::new(azimuth.sin() * elevation.cos(), elevation.sin(), azimuth.cos() * elevation.cos());
        // The toon shader divides direct light by PI, as Bevy's Lambert lobe does.
        let illuminance = self.key * std::f32::consts::PI / exposure.exposure();
        (
            DirectionalLight { color: Color::WHITE, illuminance, ..default() },
            Transform::from_translation(from * 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        )
    }

    /// The fill, for the camera's `exposure`.
    pub fn ambient_light(&self, exposure: &Exposure) -> GlobalAmbientLight {
        GlobalAmbientLight { color: self.ambient_color, brightness: self.ambient / exposure.exposure(), ..default() }
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

    #[test]
    fn a_steady_torch_does_not_flicker() {
        let steady = Torch { intensity: 1.0, flicker_depth: 0.0 };
        let full = Torch { intensity: 1.0, flicker_depth: 1.0 };
        for i in 0..100 {
            let t = i as f32 * 0.1;
            assert_eq!(steady.wavering(t), 1.0);
            assert_eq!(full.wavering(t), Torch::flicker(t));
        }
    }

    #[test]
    fn studio_key_comes_from_upper_left_behind_the_camera() {
        let (light, transform) = StudioLight::ARMORY.key_light(&Exposure::INDOOR);
        // The light shines along its forward (-Z); it comes from the opposite way.
        let from = -transform.forward();
        assert!(from.x < 0.0 && from.y > 0.0 && from.z > 0.0, "{from}");
        assert!((from.y - 42f32.to_radians().sin()).abs() < 1e-4);
        let shader_key = light.illuminance * Exposure::INDOOR.exposure() / std::f32::consts::PI;
        assert!((shader_key - 0.78).abs() < 1e-4);
    }
}
