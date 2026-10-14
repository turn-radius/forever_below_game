//! Banded toon shading with procedural wear: a [`StandardMaterial`] extended with
//! [`ToonExtension`]. The shader is `toon.wesl` (lighting), `pattern.wesl` (surface patterns)
//! and `decay.wesl` (wear).

use armory::{
    Rgb,
    fabric::{SurfacePattern, Wrap},
};
use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

/// The material every Armory part is drawn with.
pub type ToonMaterial = ExtendedMaterial<StandardMaterial, ToonExtension>;

pub(super) const SHADER_PATH: &str = "embedded://forever_below/render/toon.wesl";

/// Light levels the diffuse term snaps to, darkest first.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct ToonBands {
    values: [f32; ToonBands::MAX],
    count: u32,
}

impl ToonBands {
    pub const MAX: usize = 8;

    /// `count` bands spread evenly from `floor` to 1.0, as `makeGradient` does
    /// (`docs/rendering.md`, "Toon shading"). The Armory uses a floor of 0.12, the plate 0.16.
    pub fn even(count: usize, floor: f32) -> Self {
        let count = count.clamp(1, Self::MAX);
        let mut values = [1.0; Self::MAX];
        for (i, value) in values.iter_mut().take(count).enumerate() {
            *value = if count == 1 { 1.0 } else { floor + (1.0 - floor) * i as f32 / (count - 1) as f32 };
        }
        ToonBands { values, count: count as u32 }
    }

    /// Explicit band values, darkest first. Extra values beyond [`Self::MAX`] are ignored.
    pub fn from_values(values: &[f32]) -> Self {
        let count = values.len().clamp(1, Self::MAX);
        let mut packed = [1.0; Self::MAX];
        packed[..values.len().min(Self::MAX)].copy_from_slice(&values[..values.len().min(Self::MAX)]);
        ToonBands { values: packed, count: count as u32 }
    }

    pub fn values(&self) -> &[f32] {
        &self.values[..self.count as usize]
    }
}

impl Default for ToonBands {
    /// Three bands: 0.12, 0.56, 1.0.
    fn default() -> Self {
        ToonBands::even(3, 0.12)
    }
}

/// Procedural wear for one material (`docs/rendering.md`, "Wear, rust and grime").
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Decay {
    /// `armory::MaterialKind::decay_type()`, 1..=12; 0 draws no wear.
    pub kind: u32,
    /// 0 = as made, up to 1.2 = ruinous.
    pub amount: f32,
    /// Size of the noise pattern in item space.
    pub scale: f32,
    /// Shifts the pattern so no two items wear alike.
    pub seed: f32,
}

impl Default for Decay {
    fn default() -> Self {
        Decay { kind: 0, amount: 0.0, scale: 1.6, seed: 0.0 }
    }
}

/// A floor on how thin a part may look on screen, for fine parts drawn without an ink line
/// (bowstrings, chains, claws): far off they would otherwise thin to nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct MinWidth {
    /// Narrowest the part may look, in pixels on a 1080-pixel-tall viewport; 0 for no floor.
    pub px: f32,
    /// The part's own half thickness in item units ([`super::half_thickness`]): only a part
    /// thinner than the floor is fattened, and only by the difference.
    pub half_thickness: f32,
}

/// A woven, painted or worked pattern drawn over a part (`pattern.wesl`): an
/// [`armory::fabric::SurfacePattern`] made ready for the shader.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct Pattern {
    /// `PatternKind::shader_id()`; 0 draws no pattern.
    pub id: u32,
    /// The second colour, display (sRGB) values.
    pub color: Vec3,
    /// Cells per unit (wrapped patterns) or across the field (field patterns).
    pub scale: f32,
    /// Where a wrapped pattern sits on the part ([`Wrap`]): a point on its axis, the axis, the
    /// direction of zero angle, where rows start, the whole number of cells round, whether the
    /// part is a mirrored pair, its length along the axis and its girth along it.
    pub origin: Vec3,
    pub axis: Vec3,
    pub zero: Vec3,
    pub base: f32,
    pub around: f32,
    pub mirrored: bool,
    pub length: f32,
    pub profile: [f32; Wrap::PROFILE],
}

impl Pattern {
    /// `pattern` laid on a part with these vertices.
    pub fn of(pattern: &SurfacePattern, positions: &[[f32; 3]]) -> Self {
        let wrap = Wrap::of(positions);
        let Rgb(r, g, b) = pattern.color;
        Pattern {
            id: pattern.kind.shader_id(),
            color: Vec3::new(r, g, b),
            scale: pattern.scale,
            origin: Vec3::from(wrap.origin),
            axis: Vec3::from(wrap.axis),
            zero: Vec3::from(wrap.zero),
            base: wrap.base,
            around: wrap.repeats_around(pattern.scale),
            mirrored: wrap.mirrored,
            length: wrap.length,
            profile: wrap.profile,
        }
    }
}

/// The toon half of [`ToonMaterial`]. The base colour, roughness and so on stay on the
/// [`StandardMaterial`].
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
#[uniform(100, ToonUniform)]
pub struct ToonExtension {
    pub bands: ToonBands,
    pub decay: Decay,
    /// Self-lit strength as a fraction of the base colour; fades with decay.
    pub glow: f32,
    pub min_width: MinWidth,
    pub pattern: Pattern,
}

/// GPU layout of [`ToonExtension`]; must match `ToonUniform` in `toon.wesl`.
#[derive(Clone, Copy, Debug, ShaderType)]
pub struct ToonUniform {
    band_values: [Vec4; 2],
    band_count: u32,
    decay_type: u32,
    decay_amount: f32,
    decay_scale: f32,
    decay_seed: f32,
    glow: f32,
    min_width_px: f32,
    half_thickness: f32,
    pattern_color: Vec3,
    pattern_id: u32,
    pattern_origin: Vec3,
    pattern_scale: f32,
    pattern_axis: Vec3,
    pattern_base: f32,
    pattern_zero: Vec3,
    pattern_around: f32,
    pattern_mirrored: u32,
    pattern_length: f32,
    pattern_profile: [Vec4; 4],
}

impl From<&ToonExtension> for ToonUniform {
    fn from(toon: &ToonExtension) -> Self {
        let v = toon.bands.values;
        let pattern = toon.pattern;
        ToonUniform {
            pattern_color: pattern.color,
            pattern_id: pattern.id,
            pattern_origin: pattern.origin,
            pattern_scale: pattern.scale,
            pattern_axis: pattern.axis,
            pattern_base: pattern.base,
            pattern_zero: pattern.zero,
            pattern_around: pattern.around,
            pattern_mirrored: pattern.mirrored as u32,
            pattern_length: pattern.length,
            pattern_profile: std::array::from_fn(|i| Vec4::from_slice(&pattern.profile[i * 4..i * 4 + 4])),
            band_values: [Vec4::new(v[0], v[1], v[2], v[3]), Vec4::new(v[4], v[5], v[6], v[7])],
            band_count: toon.bands.count,
            decay_type: toon.decay.kind,
            decay_amount: toon.decay.amount,
            decay_scale: toon.decay.scale,
            decay_seed: toon.decay.seed,
            glow: toon.glow,
            min_width_px: toon.min_width.px,
            half_thickness: toon.min_width.half_thickness,
        }
    }
}

impl MaterialExtension for ToonExtension {
    fn vertex_shader() -> ShaderRef {
        SHADER_PATH.into()
    }

    // Forward only: the toon lighting replaces Bevy's PBR lighting, which the deferred
    // lighting pass would apply instead.
    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bands_match_the_armory() {
        let bands = ToonBands::default();
        let expected = [0.12, 0.56, 1.0];
        assert_eq!(bands.values().len(), 3);
        for (got, want) in bands.values().iter().zip(expected) {
            assert!((got - want).abs() < 1e-6, "{got} != {want}");
        }
    }

    #[test]
    fn explicit_bands_are_kept_in_order() {
        assert_eq!(ToonBands::from_values(&[0.16, 0.58, 1.0]).values(), &[0.16, 0.58, 1.0]);
    }
}
