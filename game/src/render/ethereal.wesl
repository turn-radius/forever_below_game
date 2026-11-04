// An ethereal shield (`EtherealMaterial` in `ethereal.rs`): a golden, see-through shield added
// onto what is behind it. The mesh's UV is the shield's own plane, -1 to 1, so `e` is how far
// toward the edge a point lies. Light is a faint fill, a fresnel rim that brightens toward the
// silhouette, a bright line just inside the edge, bands that run round the edge, and rings that
// run in from it. `flare` brightens all of it; the bands' place is `band_phase`.

import bevy_pbr::render::{
    forward_io::VertexOutput,
    mesh_view_bindings as view_bindings,
};

struct Ethereal {
    color: vec4<f32>,
    intensity: f32,
    band_phase: f32,
    flare: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> ethereal: Ethereal;

const FILL: f32 = 0.045;
const RIM_LINE: f32 = 1.2;
const AROUND_BANDS: f32 = 5.0;
const INWARD_RINGS: f32 = 16.0;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.uv;
    // A squarish distance, 1 at the edge of the shield's bounds.
    let e = pow(pow(abs(p.x), 3.0) + pow(abs(p.y), 3.0), 1.0 / 3.0);
    let angle = atan2(p.y, p.x);
    let phase = ethereal.band_phase;
    let flare = ethereal.flare;

    let view_dir = normalize(view_bindings::view.world_position - in.world_position.xyz);
    let facing = abs(dot(normalize(in.world_normal), view_dir));
    let fresnel = pow(1.0 - facing, 2.2);

    let near_edge = smoothstep(0.45, 1.0, e);
    let line = exp(-pow((e - 0.92) / 0.05, 2.0));
    let around = pow(0.5 + 0.5 * sin(angle * AROUND_BANDS - phase * 1.6), 8.0);
    let inward = pow(0.5 + 0.5 * sin(e * INWARD_RINGS - phase * 2.4), 6.0);
    let shimmer = 0.85 + 0.15 * sin(phase * 0.9 + e * 5.0);

    var light = FILL * shimmer
        + fresnel * 0.4
        + line * (RIM_LINE + around * 2.6)
        + near_edge * inward * (0.5 + flare * 1.0)
        + near_edge * around * 0.35;
    light = light * (1.0 + flare * 1.3);
    let hot = vec3(1.0, 0.93, 0.7);
    let colour = mix(ethereal.color.rgb, hot, clamp(line * around * 0.7 + flare * 0.4, 0.0, 1.0));
    // Alpha 0: added onto what is behind, whether the pass blends added or premultiplied.
    return vec4(colour * light * ethereal.intensity, 0.0);
}
