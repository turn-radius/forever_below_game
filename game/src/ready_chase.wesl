// The ready chase round a node whose timing has run out (`ChaseMaterial` in `time_left.rs`): a
// bright highlight running clockwise along a rounded border from the top-left corner, with a
// tail fading behind it. `head` is how far round it has run, in laps; below 0 nothing shows.

import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct Chase {
    glow: vec4<f32>,
    head: f32,
    // How bright, 0 to 1, so the last lap can fade.
    strength: f32,
}

@group(1) @binding(0) var<uniform> chase: Chase;

// The line's width and corner radius in pixels, and the tail's length in laps.
const LINE: f32 = 2.0;
const RADIUS: f32 = 5.0;
const TAIL: f32 = 0.35;

// Signed distance from `p` to a rounded box of half size `half` and corner radius `r`.
fn rounded_box(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2(r);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// How far round the border `p` lies, in laps clockwise from the top-left corner.
fn along(p: vec2<f32>, half: vec2<f32>) -> f32 {
    let lap = 4.0 * (half.x + half.y);
    let n = p / half;
    if n.y <= -abs(n.x) {
        return (p.x + half.x) / lap;
    }
    if n.x >= abs(n.y) {
        return (2.0 * half.x + p.y + half.y) / lap;
    }
    if n.y >= abs(n.x) {
        return (2.0 * half.x + 2.0 * half.y + half.x - p.x) / lap;
    }
    return (4.0 * half.x + 2.0 * half.y + half.y - p.y) / lap;
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    if chase.head < 0.0 || chase.strength <= 0.0 {
        return vec4(0.0);
    }
    let p = (in.uv - vec2(0.5)) * in.size;
    let half = in.size * 0.5 - vec2(LINE);
    let d = abs(rounded_box(p, half, RADIUS));
    let line = 1.0 - smoothstep(LINE * 0.5, LINE * 0.5 + 1.0, d);
    // How far behind the head this point is, in laps; only the tail lights.
    let behind = fract(chase.head - along(p, half) + 1.0);
    let tail = select(0.0, pow(1.0 - behind / TAIL, 2.0), behind < TAIL);
    // A little halo round the head.
    let halo = (1.0 - smoothstep(0.0, 4.0, d)) * select(0.0, 1.0 - behind / 0.06, behind < 0.06) * 0.6;
    let a = max(line * tail, halo) * chase.strength * chase.glow.a;
    return vec4(mix(chase.glow.rgb, vec3(1.0), tail * 0.5), a);
}
