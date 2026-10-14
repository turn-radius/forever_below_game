// The title lettering, striped as on the title card (reference/concept/forever-below-title-card.jpg):
// each letter ruled across in gold bands, with a solid gold rim round it and a soft shadow
// behind. The letters come in as coverage in the alpha of `glyphs`, white text an offscreen
// camera drew; the bands are laid in texels of that image, so they keep their spacing at any
// size on screen.

import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct Lettering {
    gold: vec4<f32>,
    // Between the bands, inside a letter.
    shade: vec4<f32>,
    shadow: vec4<f32>,
    // Texels from the top of one band to the top of the next.
    band_period: f32,
    // The share of each period the gold band fills.
    band_fill: f32,
    // The solid rim's width, in texels.
    rim: f32,
    // How far the shadow blurs, in texels.
    shadow_blur: f32,
    // Where the shadow falls, in texels (down and right).
    shadow_offset: vec2<f32>,
}

@group(1) @binding(0) var<uniform> lettering: Lettering;
@group(1) @binding(1) var glyphs: texture_2d<f32>;
@group(1) @binding(2) var glyph_sampler: sampler;

const TAU: f32 = 6.2831853;
const RING: u32 = 16u;

fn coverage(uv: vec2<f32>) -> f32 {
    return textureSampleLevel(glyphs, glyph_sampler, uv, 0.0).a;
}

// The least coverage on a ring `radius` texels round `uv`: below a half when the letter's edge
// is within that distance.
fn ring_min(uv: vec2<f32>, texel: vec2<f32>, radius: f32) -> f32 {
    var least = 1.0;
    for (var i = 0u; i < RING; i++) {
        let angle = TAU * f32(i) / f32(RING);
        least = min(least, coverage(uv + vec2<f32>(cos(angle), sin(angle)) * radius * texel));
    }
    return least;
}

// The mean coverage over two rings `radius` texels round `uv` and its centre: a cheap blur.
fn ring_mean(uv: vec2<f32>, texel: vec2<f32>, radius: f32) -> f32 {
    var sum = coverage(uv);
    for (var i = 0u; i < RING; i++) {
        let angle = TAU * f32(i) / f32(RING);
        let dir = vec2<f32>(cos(angle), sin(angle)) * texel;
        sum += coverage(uv + dir * radius) + coverage(uv + dir * radius * 0.5);
    }
    return sum / f32(2u * RING + 1u);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(glyphs));
    let texel = 1.0 / size;
    let inside = coverage(in.uv);

    // Solid near the edge, banded within.
    let rim = 1.0 - smoothstep(0.35, 0.65, ring_min(in.uv, texel, lettering.rim));
    let row = in.uv.y * size.y / lettering.band_period;
    let along = fract(row);
    let soft = max(fwidth(row), 1e-4);
    let band = smoothstep(0.0, soft, along) * (1.0 - smoothstep(lettering.band_fill - soft, lettering.band_fill, along));
    let gold = max(rim, band);
    let letter = mix(lettering.shade, lettering.gold, gold);
    let letter_alpha = inside * letter.a;

    // The shadow, under the letter.
    let shadow_alpha = ring_mean(in.uv - lettering.shadow_offset * texel, texel, lettering.shadow_blur) * lettering.shadow.a;

    let alpha = letter_alpha + shadow_alpha * (1.0 - letter_alpha);
    if alpha <= 0.0 {
        return vec4<f32>(0.0);
    }
    let rgb = (letter.rgb * letter_alpha + lettering.shadow.rgb * shadow_alpha * (1.0 - letter_alpha)) / alpha;
    return vec4<f32>(rgb, alpha);
}
