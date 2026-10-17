// The visibility eye: an almond-shaped eye in a gold rim that opens as the delver gets easier to
// see and closes in the dark. Wide open it shows its iris and a small pupil; nearly closed it is a
// lidded line with a wide pupil, as eyes dilate in the dark. Drawn in one pass inside the node's
// rectangle; the iris is a circle whatever the rectangle's proportions.

import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct Eye {
    sclera: vec4<f32>,
    iris: vec4<f32>,
    iris_deep: vec4<f32>,
    gold: vec4<f32>,
    gold_dark: vec4<f32>,
    // The dark glow behind the eye that lets it read on any background.
    backing: vec4<f32>,
    // How open, from 0 (a closed line) to 1 (wide).
    open: f32,
}

@group(1) @binding(0) var<uniform> eye: Eye;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let half_width = in.size.x * 0.5;
    // Units of half the width: x runs -1 to 1, and y by the same scale (down is positive).
    let p = (in.uv - vec2<f32>(0.5)) * in.size / half_width;
    let aa = 1.2 / half_width;
    // The tallest the almond gets, half its height, leaving room for the rim.
    let tall = in.size.y / in.size.x * 0.92;

    let shape = max(1.0 - p.x * p.x, 0.0);
    let open = clamp(eye.open, 0.0, 1.0);
    // The upper lid arches higher than the lower one.
    let upper = tall * open * pow(shape, 0.8);
    let lower = tall * open * 0.78 * shape;
    // How far inside the lids, vertically: positive within the eye.
    let d = select(lower - p.y, upper + p.y, p.y < 0.0);

    let rim = 0.085;
    let tips = 1.0 - smoothstep(1.0 - aa * 2.0, 1.0, abs(p.x));
    let body = smoothstep(-rim - aa, -rim + aa, d) * tips;
    let ball = smoothstep(-aa, aa, d) * tips;

    // The soft dark glow round the almond.
    let around = vec2<f32>(p.x / 1.05, p.y / (tall * 1.5 + 0.02));
    // It fades out before the node's edge, or the box would show.
    let fade = (1.0 - smoothstep(0.55, 1.0, abs(p.x))) * (1.0 - smoothstep(0.45, 1.0, abs(p.y) * in.size.x / in.size.y));
    let halo = exp(-dot(around, around) * 2.2) * eye.backing.a * fade;

    // The rim: bright gold on top, darker below, as a lit metal wire.
    let lit = clamp(0.5 - p.y * 1.6, 0.0, 1.0);
    let metal = mix(eye.gold_dark.rgb, eye.gold.rgb, lit);

    // The eyeball: shaded darker under the lids, with the iris and pupil.
    let lid_shade = 0.6 + 0.4 * smoothstep(0.0, tall * 0.55 * max(open, 0.05), d);
    let iris_radius = min(0.34, tall * 0.92);
    let r = length(p);
    let iris_cover = 1.0 - smoothstep(iris_radius - aa, iris_radius + aa, r);
    let pupil_radius = iris_radius * (0.34 + 0.25 * (1.0 - open));
    let pupil_cover = 1.0 - smoothstep(pupil_radius - aa, pupil_radius + aa, r);
    let limbal = smoothstep(iris_radius * 0.72, iris_radius, r);
    var iris_color = mix(eye.iris.rgb, eye.iris_deep.rgb, limbal);
    iris_color = mix(iris_color, vec3<f32>(0.01), pupil_cover);
    var eyeball = mix(eye.sclera.rgb * lid_shade, iris_color * lid_shade, iris_cover);
    // A small glint on the iris.
    let glint_at = p - vec2<f32>(-iris_radius * 0.38, -iris_radius * 0.4);
    eyeball += vec3<f32>(0.9) * exp(-dot(glint_at, glint_at) / (iris_radius * iris_radius * 0.03)) * iris_cover * smoothstep(0.1, 0.5, open);

    let color = mix(metal, eyeball, ball / max(body, 1e-4));
    let alpha = max(body, halo);
    let rgb = mix(eye.backing.rgb, color, body / max(alpha, 1e-4));
    return vec4<f32>(rgb, alpha);
}
