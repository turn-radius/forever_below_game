// A wisp (`WispMaterial` in `wisp.rs`): soft light on a card turned to the eye, added onto what
// is behind it. A heart is a tight near-white core in a softer body and a faint wide halo; its
// edge wavers by a few drifting harmonics about its middle, and fine ripples run out through
// the body, so it shimmers and seems to bend the air round it. A mote is a small soft dot that
// twinkles. The sizes and rates are our own, tuned against captures.

import bevy_pbr::render::{
    forward_io::VertexOutput,
    mesh_view_bindings as view_bindings,
};

struct Wisp {
    color: vec4<f32>,
    core: vec4<f32>,
    strength: f32,
    wobble: f32,
    phase: f32,
    mote: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> wisp: Wisp;

// How far the heart's edge wavers at full wobble, as a share of its radius, for each harmonic.
const WAVER: vec3<f32> = vec3(0.11, 0.07, 0.05);
// The core's, body's and halo's widths, as shares of the card's half width.
const CORE: f32 = 0.13;
const BODY: f32 = 0.34;
const HALO: f32 = 4.2;
// Rings per half width of the ripples, and how fast they run outward, radians a second.
const RIPPLE_RINGS: f32 = 26.0;
const RIPPLE_RATE: f32 = 5.5;

fn heart(p: vec2<f32>, t: f32) -> vec3<f32> {
    let r = length(p);
    let a = atan2(p.y, p.x);
    let waver = 1.0 + wisp.wobble * (
        WAVER.x * sin(3.0 * a + 1.7 * t)
        + WAVER.y * sin(5.0 * a - 2.3 * t + 1.0)
        + WAVER.z * sin(7.0 * a + 3.1 * t + 2.0)
    );
    let q = r / waver;
    let core = exp(-pow(q / CORE, 2.0));
    let body = exp(-pow(q / BODY, 2.0));
    let halo = exp(-q * HALO);
    let ripple = 0.5 + 0.5 * sin(q * RIPPLE_RINGS - t * RIPPLE_RATE + 2.0 * sin(2.0 * a + t));
    let swell = 0.88 + 0.12 * sin(t * 2.3);
    let light = core * 1.5 + body * (0.55 + 0.3 * ripple) * swell + halo * 0.3 * (0.75 + 0.25 * ripple);
    let colour = mix(wisp.color.rgb, wisp.core.rgb, clamp(core + body * 0.35, 0.0, 1.0));
    // Gone before the card's edge, so its square never shows.
    let edge = 1.0 - smoothstep(0.8, 1.0, r);
    return colour * light * edge;
}

fn mote(p: vec2<f32>) -> vec3<f32> {
    let r = length(p);
    let dot = exp(-pow(r / 0.28, 2.0)) + exp(-r * 5.0) * 0.25;
    let edge = 1.0 - smoothstep(0.8, 1.0, r);
    return mix(wisp.color.rgb, wisp.core.rgb, exp(-pow(r / 0.12, 2.0))) * dot * edge;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.uv * 2.0 - 1.0;
    let t = view_bindings::globals.time + wisp.phase;
    var light: vec3<f32>;
    if wisp.mote > 0.5 {
        light = mote(p);
    } else {
        light = heart(p, t);
    }
    // Alpha 0: added onto what is behind, whether the pass blends added or premultiplied.
    return vec4(light * wisp.strength, 0.0);
}
