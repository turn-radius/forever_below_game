//! Light rigs. Carried and wall-mounted torches are the main light in the abyss
//! (`docs/rendering.md`, "Light"); [`StudioLight`] is the even, camera-borne light of an item
//! viewer. Both live apart from any one scene.
//!
//! Point and spot lights fall off as the toon shader draws them ([`reach_falloff`]): an
//! inverse square softened near the light and windowed to zero at its range, so a light is
//! set by how bright it is up close ([`peak_lumens`]) and how far it reaches.
//!
//! Flames flicker in the toon shader, not here: a light's intensity stays steady on the CPU and
//! the shader wavers its strength after cutting it into steps, so the steps hold still and only
//! their brightness moves ([`Torch::flickering`]).

use bevy::{
    camera::{Exposure, visibility::VisibilitySystems},
    light::PointLightShadowMap,
    prelude::*,
};

use crate::{
    level::DebugCamera,
    play::{DelveSystems, PlayerCamera},
    tuning::{LightTuning, Tuning},
};

/// Fraction of a light's range inside which its falloff stops climbing: the inverse square is
/// softened to `1 / (d^2 + core^2)` with `core = CORE_FRACTION * range`. Must match
/// `CORE_FRACTION` in `render/toon.wesl`.
pub const CORE_FRACTION: f32 = 0.4;

/// Shadow biases for lights in this game's units (10 cm). Bevy's defaults assume metres, so
/// its depth bias of 0.08 is 8 mm here, which leaves walls the light grazes in shadow acne.
/// The depth bias is in world units (5 cm); Bevy scales the normal bias by the shadow map's
/// texel size at the fragment.
pub const SHADOW_DEPTH_BIAS: f32 = 0.5;
pub const SHADOW_NORMAL_BIAS: f32 = 1.2;

/// How brightly `light` lights a surface `distance` units from it, as a multiple of full (the
/// level the toon shader gives, 1 being the surface's own colour): the inverse of
/// [`peak_lumens`] at the light's reach, times [`reach_falloff`]. `exposure` is the camera's.
pub fn level_at(light: &PointLight, distance: f32, exposure: f32) -> f32 {
    let peak = peak_lumens(1.0, light.range, exposure);
    if peak <= 0.0 { 0.0 } else { light.intensity / peak * reach_falloff(distance, light.range) }
}

/// How lit the point `at` is by `lights` (each a position and its light): their levels
/// ([`level_at`]) added, at most 1. Walls between are not accounted for.
pub fn lit_level<'a>(at: Vec3, lights: impl IntoIterator<Item = (Vec3, &'a PointLight)>, exposure: f32) -> f32 {
    lights.into_iter().map(|(position, light)| level_at(light, position.distance(at), exposure)).sum::<f32>().min(1.0)
}

/// A point or spot light's falloff at `distance` from it with reach `range`, 1 at the light
/// and 0 from `range` on. The window `(1 - (d/r)^4)^2` and the `+1` that keeps the inverse
/// square finite are Brian Karis's, from "Real Shading in Unreal Engine 4" (SIGGRAPH 2013,
/// eq. 9); Bevy's `getDistanceAttenuation` uses the same window without the `+1`. Here the
/// `+1` is measured in units of `CORE_FRACTION * range` rather than one world unit, so every
/// light has the same shape across its reach (our own choice). Mirrors `reach_attenuation` in
/// `render/toon.wesl`, times `core^2`.
pub fn reach_falloff(distance: f32, range: f32) -> f32 {
    if range <= 0.0 {
        return 0.0;
    }
    let ratio = distance / range;
    let window = (1.0 - ratio.powi(4)).clamp(0.0, 1.0).powi(2);
    let core = distance / (CORE_FRACTION * range);
    window / (1.0 + core * core)
}

/// Point light intensity in lumens that lights a surface square to it, `distance` units away,
/// to `level` times its display colour in the toon shader under a plain inverse square. Bevy
/// divides lumens by 4 pi for candela, the toon shader divides by pi (its Lambert lobe) and
/// multiplies by the camera's exposure.
pub fn lumens(level: f32, distance: f32, exposure: f32) -> f32 {
    level * 4.0 * std::f32::consts::PI * std::f32::consts::PI * distance * distance / exposure
}

/// Lumens for a light of reach `range` to light a surface square to it, right at the light, to
/// `level` times its display colour; further off it follows [`reach_falloff`]. The shader caps
/// what one light gives at full, so a `level` over 1 widens the fully lit core.
pub fn peak_lumens(level: f32, range: f32, exposure: f32) -> f32 {
    lumens(level, CORE_FRACTION * range, exposure)
}

/// A point light in `color`, `peak` times full close up (see [`peak_lumens`]), reaching
/// `range` units, for a camera with `exposure`.
pub fn point_light(color: Color, peak: f32, range: f32, exposure: &Exposure) -> PointLight {
    PointLight { color, intensity: peak_lumens(peak, range, exposure.exposure()), range, ..default() }
}

/// `light` casting shadows, with biases for this game's units.
pub fn cast_shadows(light: PointLight) -> PointLight {
    PointLight {
        shadow_maps_enabled: true,
        shadow_depth_bias: SHADOW_DEPTH_BIAS,
        shadow_normal_bias: SHADOW_NORMAL_BIAS,
        ..light
    }
}

/// Whether `light` is one that [`cast_shadows`] made, and so one [`ShadowBudgetPlugin`] decides
/// the shadows of. A light with Bevy's default biases is left as it is.
pub fn is_shadowable(light: &PointLight) -> bool {
    light.shadow_depth_bias == SHADOW_DEPTH_BIAS && light.shadow_normal_bias == SHADOW_NORMAL_BIAS
}

/// How much nearer, in world units, a light that already casts shadows counts than it is, so a
/// light about as far as the budget's last does not swap places with it every frame as the
/// player walks by. Our own tuning: a little under a cell (a cell is 40 units).
pub const SHADOW_STICKINESS: f32 = 12.0;

/// Which of `lights` (each its distance from the camera, and whether it casts shadows now)
/// cast shadows next: the `budget` nearest, a light that already does counting
/// [`SHADOW_STICKINESS`] nearer. Ties go to the earlier light.
pub fn shadow_casters(lights: &[(f32, bool)], budget: usize) -> Vec<bool> {
    let rank = |&(distance, casting): &(f32, bool)| if casting { distance - SHADOW_STICKINESS } else { distance };
    let mut order: Vec<usize> = (0..lights.len()).collect();
    order.sort_by(|&a, &b| rank(&lights[a]).total_cmp(&rank(&lights[b])));
    let mut casts = vec![false; lights.len()];
    for &light in order.iter().take(budget) {
        casts[light] = true;
    }
    casts
}

/// Lets only the lights nearest the camera cast shadows. Bevy 0.20 renders six shadow faces
/// every frame for every shadowed point light, and the level has braziers and glowcaps all
/// over it, most of them far off and swallowed by the fog; the light a shadow matters for is
/// the one beside you. The count and the size of each face are tuned ([`LightTuning`]).
///
/// Lights keep the biases [`cast_shadows`] gave them; this switches `shadow_maps_enabled`. A
/// light the tuning rebuilds (`FixedLight`, carried lights) comes back shadowed and is judged
/// again the same frame.
pub struct ShadowBudgetPlugin;

impl Plugin for ShadowBudgetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            budget_shadows.after(TransformSystems::Propagate).before(VisibilitySystems::CheckVisibility).in_set(DelveSystems),
        )
        .add_systems(Update, size_shadow_maps.run_if(resource_changed::<Tuning>).in_set(DelveSystems));
    }
}

fn budget_shadows(
    tuning: Res<Tuning>,
    cameras: Query<(&Camera, &GlobalTransform), Or<(With<PlayerCamera>, With<DebugCamera>)>>,
    mut lights: Query<(&GlobalTransform, &mut PointLight)>,
) {
    let Some((_, eye)) = cameras.iter().find(|(camera, _)| camera.is_active) else { return };
    let eye = eye.translation();
    let mut shadowable: Vec<_> = lights.iter_mut().filter(|(_, light)| is_shadowable(light)).collect();
    let ranked: Vec<(f32, bool)> = shadowable.iter().map(|(at, light)| (at.translation().distance(eye), light.shadow_maps_enabled)).collect();
    for ((_, light), cast) in shadowable.iter_mut().zip(shadow_casters(&ranked, tuning.light.shadowed_lights)) {
        if light.shadow_maps_enabled != cast {
            light.shadow_maps_enabled = cast;
        }
    }
}

fn size_shadow_maps(tuning: Res<Tuning>, mut map: ResMut<PointLightShadowMap>) {
    let LightTuning { shadow_map_size, .. } = tuning.light;
    if map.size != shadow_map_size {
        map.size = shadow_map_size;
    }
}

/// Torches, braziers and every other flame: warm point lights that flicker.
///
/// The flicker is drawn by the toon shader (`flame_flicker` in `render/toon.wesl`) after it has
/// cut the light into its reach steps, so step boundaries stay put and only brightness wavers.
/// Scaling `PointLight::intensity` every frame instead swept the step boundaries across walls
/// and floors from frame to frame, which read as heavy flicker (#144).
///
/// The shader tells a flame from a steady light (glowcaps, the debug light) by the light's
/// [`PointLight::radius`], which carries its flicker depth ([`flicker_depth`]). Bevy uses
/// `radius` only for the size of specular highlights and the penumbra of percentage-closer soft
/// shadows (`bevy_light-0.20.0/src/point_light.rs`); the toon shader draws no specular and this
/// game builds without `experimental_pbr_pcss`, so the field is otherwise unread. This channel
/// is our own convention.
pub struct Torch;

impl Torch {
    /// The torch colour, `#ffb36a`.
    pub const COLOR: Color = Color::srgb(1.0, 0xb3 as f32 / 255.0, 0x6a as f32 / 255.0);

    /// The prototype's flicker multiplier at time `t` seconds: around 0.88, within 0.72..1.04
    /// (`docs/rendering.md`, "Light"). Mirrors `flame_flicker` in `render/toon.wesl`.
    pub fn flicker(t: f32) -> f32 {
        0.88 + 0.08 * (13.1 * t).sin() + 0.05 * (7.3 * t + 1.2).sin() + 0.03 * (23.7 * t).sin()
    }

    /// The strength multiplier at time `t` seconds for a light of `depth` (0 steady, 1 the
    /// prototype's full curve): [`Torch::flicker`] scaled toward 1. Mirrors the shader.
    pub fn wavering(depth: f32, t: f32) -> f32 {
        1.0 - depth.clamp(0.0, 1.0) * (1.0 - Torch::flicker(t))
    }

    /// `light` flickering by up to `depth` of the prototype's curve, as in
    /// `armory::build::light::LightAnchor::flicker`.
    pub fn flickering(light: PointLight, depth: f32) -> PointLight {
        PointLight { radius: depth.clamp(0.0, 1.0), ..light }
    }

    /// A torch's point light, `peak` times full close up, reaching `range` units.
    pub fn light(peak: f32, range: f32, exposure: &Exposure) -> PointLight {
        Torch::flickering(point_light(Torch::COLOR, peak, range, exposure), 1.0)
    }
}

/// How deeply `light` flickers, 0 (steady) to 1, as the toon shader reads it ([`Torch`]).
pub fn flicker_depth(light: &PointLight) -> f32 {
    light.radius.clamp(0.0, 1.0)
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
        let from = self.key_from();
        // The toon shader divides direct light by PI, as Bevy's Lambert lobe does.
        let illuminance = self.key * std::f32::consts::PI / exposure.exposure();
        (
            DirectionalLight { color: Color::WHITE, illuminance, ..default() },
            Transform::from_translation(from * 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        )
    }

    /// The unit direction the key comes from, in camera space.
    pub fn key_from(&self) -> Vec3 {
        let (azimuth, elevation) = (self.azimuth.to_radians(), self.elevation.to_radians());
        Vec3::new(azimuth.sin() * elevation.cos(), elevation.sin(), azimuth.cos() * elevation.cos())
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
        for i in 0..100 {
            let t = i as f32 * 0.1;
            assert_eq!(Torch::wavering(0.0, t), 1.0);
            assert_eq!(Torch::wavering(1.0, t), Torch::flicker(t));
        }
    }

    #[test]
    fn flames_carry_their_flicker_and_steady_lights_none() {
        let exposure = Exposure::INDOOR;
        let torch = Torch::light(1.0, 60.0, &exposure);
        assert_eq!(flicker_depth(&torch), 1.0);
        assert_eq!(flicker_depth(&Torch::flickering(torch, 0.4)), 0.4);
        assert_eq!(flicker_depth(&point_light(Color::WHITE, 1.0, 60.0, &exposure)), 0.0);
        // The intensity is the steady one; the shader does the flickering.
        assert_eq!(torch.intensity, peak_lumens(1.0, 60.0, exposure.exposure()));
        assert_eq!(flicker_depth(&cast_shadows(torch)), 1.0);
    }

    #[test]
    fn falloff_is_full_at_the_light_and_dark_at_its_reach() {
        assert_eq!(reach_falloff(0.0, 60.0), 1.0);
        assert_eq!(reach_falloff(60.0, 60.0), 0.0);
        assert_eq!(reach_falloff(90.0, 60.0), 0.0);
        let samples: Vec<f32> = (0..=60).map(|d| reach_falloff(d as f32, 60.0)).collect();
        assert!(samples.windows(2).all(|w| w[1] < w[0]), "falls all the way");
        // The same shape at any reach.
        assert!((reach_falloff(15.0, 60.0) - reach_falloff(30.0, 120.0)).abs() < 1e-6);
        // Softened at the core: at the core distance it is still half, less the window.
        let core = CORE_FRACTION * 60.0;
        assert!((reach_falloff(core, 60.0) - 0.5 * (1.0 - CORE_FRACTION.powi(4)).powi(2)).abs() < 1e-6);
    }

    #[test]
    fn a_lights_level_is_its_peak_at_the_light_and_nothing_past_its_reach() {
        let exposure = Exposure::INDOOR.exposure();
        let light = point_light(Color::WHITE, 0.8, 60.0, &Exposure::INDOOR);
        assert!((level_at(&light, 0.0, exposure) - 0.8).abs() < 1e-4);
        assert!((level_at(&light, 15.0, exposure) - 0.8 * reach_falloff(15.0, 60.0)).abs() < 1e-4);
        assert_eq!(level_at(&light, 60.0, exposure), 0.0);
    }

    #[test]
    fn a_point_is_lit_by_every_light_near_it_up_to_full() {
        let exposure = Exposure::INDOOR.exposure();
        let torch = Torch::light(1.0, 60.0, &Exposure::INDOOR);
        let at = Vec3::ZERO;
        let none = lit_level(at, [], exposure);
        let far = lit_level(at, [(Vec3::new(500.0, 0.0, 0.0), &torch)], exposure);
        let near = lit_level(at, [(Vec3::new(10.0, 0.0, 0.0), &torch)], exposure);
        let two = lit_level(at, [(Vec3::new(30.0, 0.0, 0.0), &torch), (Vec3::new(-30.0, 0.0, 0.0), &torch)], exposure);
        let one = lit_level(at, [(Vec3::new(30.0, 0.0, 0.0), &torch)], exposure);
        assert_eq!((none, far), (0.0, 0.0));
        assert!(near > 0.5 && near <= 1.0, "{near}");
        assert!((two - 2.0 * one).abs() < 1e-4 || two == 1.0, "lights add: {two} against {one}");
        assert!(lit_level(at, [(Vec3::ZERO, &Torch::light(5.0, 60.0, &Exposure::INDOOR))], exposure) == 1.0, "never more than full");
    }

    #[test]
    fn peak_lumens_match_the_toon_light() {
        // The toon shader's light: lumens / 4 pi / (d^2 + core^2) / pi * exposure * window.
        let exposure = Exposure::INDOOR.exposure();
        let (range, d) = (80.0, 30.0);
        let core = CORE_FRACTION * range;
        let window = (1.0f32 - (d / range).powi(4)).powi(2);
        let peak = peak_lumens(0.8, range, exposure);
        let pi = std::f32::consts::PI;
        let level = peak / (4.0 * pi) / (d * d + core * core) / pi * exposure * window;
        assert!((level - 0.8 * reach_falloff(d, range)).abs() < 1e-4, "{level}");
        assert!((lumens(1.0, 20.0, exposure) / lumens(1.0, 10.0, exposure) - 4.0).abs() < 1e-3);
    }

    #[test]
    fn shadowed_lights_use_biases_in_game_units() {
        let light = cast_shadows(point_light(Torch::COLOR, 1.0, 60.0, &Exposure::INDOOR));
        assert!(light.shadow_maps_enabled);
        assert_eq!(light.shadow_depth_bias, SHADOW_DEPTH_BIAS);
        assert_eq!(light.shadow_normal_bias, SHADOW_NORMAL_BIAS);
    }

    #[test]
    fn only_the_nearest_lights_cast_shadows() {
        let lights: Vec<(f32, bool)> = (0..10).map(|i| (10.0 + 30.0 * i as f32, true)).collect();
        let casts = shadow_casters(&lights, 4);
        assert_eq!(casts.iter().filter(|c| **c).count(), 4);
        assert_eq!(&casts[..4], &[true; 4]);
        assert!(shadow_casters(&lights, 0).iter().all(|c| !c));
        assert!(shadow_casters(&lights, 20).iter().all(|c| *c));
    }

    #[test]
    fn a_light_near_the_cut_does_not_swap_places_every_frame() {
        // Two lights about the same distance away, one slot: whichever has it keeps it.
        for held in 0..2 {
            let mut lights = [(50.0, held == 0), (50.0, held == 1)];
            for step in 0..20 {
                // The player sways, so the other is sometimes a little nearer.
                lights[1 - held].0 = 50.0 - SHADOW_STICKINESS * 0.9 * (step % 2) as f32;
                let casts = shadow_casters(&lights, 1);
                assert_eq!(casts[held], true, "step {step}");
            }
        }
        // But one clearly nearer takes it.
        assert_eq!(shadow_casters(&[(80.0, true), (20.0, false)], 1), vec![false, true]);
    }

    #[test]
    fn the_budget_switches_shadows_by_distance_and_keeps_the_biases() {
        let mut app = App::new();
        app.add_plugins(bevy::transform::TransformPlugin)
            .init_resource::<Tuning>()
            .add_systems(PostUpdate, budget_shadows.after(TransformSystems::Propagate));
        app.world_mut().spawn((Camera::default(), PlayerCamera, Transform::default()));
        let light = cast_shadows(point_light(Torch::COLOR, 1.0, 60.0, &Exposure::INDOOR));
        for i in 0..10 {
            app.world_mut().spawn((light, Transform::from_xyz(20.0 * (i + 1) as f32, 0.0, 0.0)));
        }
        // A light without the game's biases is not the budget's to switch.
        let plain = app.world_mut().spawn((PointLight { shadow_maps_enabled: true, ..default() }, Transform::from_xyz(900.0, 0.0, 0.0))).id();
        app.update();
        let mut query = app.world_mut().query::<(&PointLight, &Transform)>();
        let mut shadowed: Vec<f32> = query.iter(app.world()).filter(|(l, _)| l.shadow_maps_enabled && is_shadowable(l)).map(|(_, t)| t.translation.x).collect();
        shadowed.sort_by(f32::total_cmp);
        assert_eq!(shadowed, vec![20.0, 40.0, 60.0, 80.0]);
        assert!(query.iter(app.world()).all(|(l, _)| l.shadow_depth_bias == SHADOW_DEPTH_BIAS || l.shadow_maps_enabled), "biases kept");
        assert_eq!(query.iter(app.world()).filter(|(l, _)| is_shadowable(l)).count(), 10);
        assert!(app.world().get::<PointLight>(plain).unwrap().shadow_maps_enabled);
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
