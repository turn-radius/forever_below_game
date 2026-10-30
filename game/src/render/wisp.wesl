// A wisp (`WispMaterial` in `wisp.rs`): soft light on a card turned to the eye, added onto what
// is behind it. A heart is a tight near-white core in a softer body and a faint wide halo; its
// edge wavers by a few drifting harmonics about its middle, and fine ripples run out through
// the body, so it shimmers and seems to bend the air round it. A mote is a small soft dot that
// twinkles. A spark is a streak along the card's height, brightest at its head (the top); a
// beam is a shaft soft at its sides, brightest at its foot (the bottom) and fading as it
// rises; a ring is a thin bright ring with a faint glow inside it. A tagged wisp (an effect's
// particle) takes its colour and strength from its mesh's tag, an sRGB colour and alpha packed
// as four bytes, so every card of a kind shares one material. The sizes and rates are our own,
// tuned against captures.

import bevy_pbr::render::{
    forward_io::VertexOutput,
    mesh_functions,
    mesh_view_bindings as view_bindings,
};

struct Wisp {
    color: vec4<f32>,
    core: vec4<f32>,
    strength: f32,
    wobble: f32,
    phase: f32,
    kind: f32,
    tagged: f32,
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

fn heart(p: vec2<f32>, t: f32, c: vec3<f32>, k: vec3<f32>) -> vec3<f32> {
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
    let colour = mix(c, k, clamp(core + body * 0.35, 0.0, 1.0));
    // Gone before the card's edge, so its square never shows.
    let edge = 1.0 - smoothstep(0.8, 1.0, r);
    return colour * light * edge;
}

fn mote(p: vec2<f32>, c: vec3<f32>, k: vec3<f32>) -> vec3<f32> {
    let r = length(p);
    let dot = exp(-pow(r / 0.28, 2.0)) + exp(-r * 5.0) * 0.25;
    let edge = 1.0 - smoothstep(0.8, 1.0, r);
    return mix(c, k, exp(-pow(r / 0.12, 2.0))) * dot * edge;
}

fn spark(p: vec2<f32>, c: vec3<f32>, k: vec3<f32>) -> vec3<f32> {
    // 0 at the tail, 1 at the head.
    let h = (1.0 - p.y) * 0.5;
    let across = exp(-pow(p.x / 0.2, 2.0));
    let along = smoothstep(0.0, 0.6, h) * (1.0 - smoothstep(0.9, 1.0, h));
    let head = exp(-pow((h - 0.9) / 0.1, 2.0));
    let edge = 1.0 - smoothstep(0.8, 1.0, abs(p.x));
    return mix(c, k, clamp(head + across * 0.3, 0.0, 1.0)) * across * (along * 0.9 + head * 0.9) * edge;
}

fn beam(p: vec2<f32>, c: vec3<f32>, k: vec3<f32>) -> vec3<f32> {
    // 0 at the foot, 1 at the top.
    let h = (1.0 - p.y) * 0.5;
    let sides = 1.0 - smoothstep(0.0, 1.0, abs(p.x));
    let core = exp(-pow(p.x / 0.28, 2.0));
    let rise = pow(1.0 - h, 1.5) * (1.0 - smoothstep(0.85, 1.0, h));
    return mix(c, k, core) * (sides * sides * 0.6 + core * 0.9) * rise;
}

fn ring(p: vec2<f32>, c: vec3<f32>, k: vec3<f32>) -> vec3<f32> {
    let r = length(p);
    let band = exp(-pow((r - 0.82) / 0.07, 2.0));
    let glow = exp(-pow((r - 0.82) / 0.22, 2.0)) * 0.35;
    let inside = (1.0 - smoothstep(0.0, 0.82, r)) * 0.06;
    let edge = 1.0 - smoothstep(0.9, 1.0, r);
    return mix(c, k, band) * (band * 1.3 + glow + inside) * edge;
}

// How far toward white a tagged card's core washes.
const TAG_CORE_WHITE: f32 = 0.5;

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.uv * 2.0 - 1.0;
    var c = wisp.color.rgb;
    var k = wisp.core.rgb;
    var strength = wisp.strength;
    var phase = wisp.phase;
    if wisp.tagged > 0.5 {
        let tag = unpack4x8unorm(mesh_functions::get_tag(in.instance_index));
        c = srgb_to_linear(tag.rgb);
        k = mix(c, vec3(1.0), TAG_CORE_WHITE);
        strength = tag.a * wisp.strength;
        phase = f32(in.instance_index % 997u) * 0.37;
    }
    let t = view_bindings::globals.time + phase;
    var light: vec3<f32>;
    let kind = i32(wisp.kind + 0.5);
    if kind == 1 {
        light = mote(p, c, k);
    } else if kind == 2 {
        light = spark(p, c, k);
    } else if kind == 3 {
        light = beam(p, c, k);
    } else if kind == 4 {
        light = ring(p, c, k);
    } else {
        light = heart(p, t, c, k);
    }
    // Alpha 0: added onto what is behind, whether the pass blends added or premultiplied.
    return vec4(light * strength, 0.0);
}
